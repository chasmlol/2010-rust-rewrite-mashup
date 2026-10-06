use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use assets::LoadingScreen;
use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::tasks::IoTaskPool;
use frame::{AppScreen, HasWorld};

use render_frontend::prepare::scene::cull::DpvsFrameStats;

pub const CAPTURE_SETTLE_FRAMES: u32 = 30;

pub const CAPTURE_WAIT_LIMIT: std::time::Duration = std::time::Duration::from_secs(180);

/// How long a scenario exit waits for a screenshot still being written. The
/// process exit kills I/O workers, so the file must be finished first.
const CAPTURE_DRAIN_LIMIT: std::time::Duration = std::time::Duration::from_secs(20);

const USER_QUIT_DRAIN_LIMIT: std::time::Duration = std::time::Duration::from_millis(100);

static EXIT_DRAIN_BUDGET_MS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(CAPTURE_DRAIN_LIMIT.as_millis() as u64);

fn exit_drain_budget() -> std::time::Duration {
    std::time::Duration::from_millis(
        EXIT_DRAIN_BUDGET_MS.load(std::sync::atomic::Ordering::Relaxed),
    )
}

pub fn exit_is_user_quit() {
    EXIT_DRAIN_BUDGET_MS.fetch_min(
        USER_QUIT_DRAIN_LIMIT.as_millis() as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureRequest {
    pub path: PathBuf,

    pub exit_after_capture: bool,
}

impl CaptureRequest {
    pub fn from_env() -> Option<CaptureRequest> {
        let path = std::env::var_os("IW4L_SCREENSHOT")?;
        Some(CaptureRequest {
            path: PathBuf::from(path),
            exit_after_capture: false,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureFrameFacts {
    pub screen: AppScreen,

    pub loading_overlay: bool,

    pub has_world: bool,

    pub submitted_batches: u32,

    pub g0_world: u32,
}

impl CaptureFrameFacts {
    pub fn content_up(self) -> bool {
        self.obstruction().is_none()
    }

    pub fn world_submitted(self) -> bool {
        self.submitted_batches > 0 || self.g0_world > 0
    }

    fn obstruction(self) -> Option<CaptureWait> {
        if self.screen == AppScreen::Loading {
            return Some(CaptureWait::MatchLoading);
        }
        if self.loading_overlay {
            return Some(CaptureWait::LoadingOverlayUp);
        }

        if self.has_world && !self.world_submitted() {
            return Some(CaptureWait::WorldNotSubmitted);
        }
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureWait {
    MatchLoading,

    LoadingOverlayUp,

    WorldNotSubmitted,

    Settling { frames_left: u32 },
}

impl CaptureWait {
    pub fn label(self) -> String {
        match self {
            CaptureWait::MatchLoading => "match still loading".to_owned(),
            CaptureWait::LoadingOverlayUp => "loading overlay still drawn".to_owned(),
            CaptureWait::WorldNotSubmitted => "world handed over, no batch submitted".to_owned(),
            CaptureWait::Settling { frames_left } => {
                format!("settling ({frames_left} frames left)")
            }
        }
    }
}

pub fn capture_wait(facts: CaptureFrameFacts, settled_frames: u32) -> Option<CaptureWait> {
    facts.obstruction().or_else(|| {
        (settled_frames < CAPTURE_SETTLE_FRAMES).then(|| CaptureWait::Settling {
            frames_left: CAPTURE_SETTLE_FRAMES - settled_frames,
        })
    })
}

#[derive(Clone, Debug)]
struct PendingCapture {
    request: CaptureRequest,
    waited_frames: u32,
    queued_at: std::time::Instant,

    reported: Option<std::mem::Discriminant<CaptureWait>>,
}

impl PendingCapture {
    fn should_report(&mut self, wait: CaptureWait) -> bool {
        let kind = std::mem::discriminant(&wait);
        let fresh = self.reported != Some(kind);
        self.reported = Some(kind);
        fresh
    }
}

/// One finished write, as the worker that did it saw it.
struct CaptureWrite {
    path: PathBuf,
    error: Option<String>,
}

/// Writes finished since the last frame looked. An `Arc` rather than a
/// channel because the only thing crossing back is a handful of completions a
/// frame, and the queue drains whatever is there.
type CaptureWrites = Arc<Mutex<Vec<CaptureWrite>>>;

/// Encodes and writes handed to a worker and not finished, process-wide.
///
/// The queue resource cannot see these at the moment that matters: a demo ends
/// in `std::process::exit` from `Last`, and a system that reads `AppExit` may
/// never run again. This is what the exit handler waits on.
static WRITES_IN_FLIGHT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Wait for the encodes still running, however the process is leaving.
///
/// Armed once, the first time a readback is handed over. Bounded: a stuck
/// write costs the exit a few seconds rather than hanging it, and the file it
/// was writing is then incomplete — which the queue's own report says.
#[cfg(unix)]
fn arm_write_drain_at_exit() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static ARMED: AtomicBool = AtomicBool::new(false);
    if ARMED.swap(true, Ordering::SeqCst) {
        return;
    }
    diag::exit::at_exit(|| {
        let until = std::time::Instant::now() + exit_drain_budget();
        while WRITES_IN_FLIGHT.load(Ordering::Relaxed) > 0 && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    });
}

#[cfg(not(unix))]
fn arm_write_drain_at_exit() {}

#[derive(Resource, Default)]
pub struct CaptureQueue {
    pending: VecDeque<PendingCapture>,

    settled_frames: u32,

    /// Readbacks handed to an I/O worker and not yet reported back. Lowered
    /// when the worker says it finished, not when it was handed over: the
    /// file does not exist until then, and `exit_after_drained` is waiting on
    /// the file.
    writing: usize,

    write_completed: bool,

    exit_when_drained: bool,

    writes: CaptureWrites,
}

impl CaptureQueue {
    pub fn push(&mut self, request: CaptureRequest) {
        if request.exit_after_capture {
            self.exit_when_drained = true;
        }
        self.pending.push_back(PendingCapture {
            request,
            waited_frames: 0,
            queued_at: std::time::Instant::now(),
            reported: None,
        });
    }

    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    pub fn pending_paths(&self) -> Vec<PathBuf> {
        self.pending
            .iter()
            .map(|p| p.request.path.clone())
            .collect()
    }

    pub fn exit_after_drained(&mut self) -> bool {
        if self.pending.is_empty() && self.writing == 0 {
            return false;
        }
        self.exit_when_drained = true;
        true
    }

    pub fn owed_at_exit(&self) -> (usize, usize) {
        (self.pending.len(), self.writing)
    }

    /// Book every write a worker finished since the last look.
    fn collect_writes(&mut self) {
        let finished = std::mem::take(
            &mut *self
                .writes
                .lock()
                .unwrap_or_else(|poison| poison.into_inner()),
        );
        for write in finished {
            self.writing = self.writing.saturating_sub(1);
            self.write_completed = true;
            match write.error {
                Some(error) => diag::error!(
                    Launch,
                    "screenshot: {} was not written: {error}",
                    write.path.display()
                ),
                None => diag::info!(Launch, "screenshot: wrote {}", write.path.display()),
            }
        }
    }
}

/// Encode and write one readback. Runs on an I/O worker, never on the frame.
///
/// The alpha channel carries brightness when HDR is on, so it is dropped
/// rather than saved as opacity.
fn write_capture(path: &Path, image: Image) -> Result<(), String> {
    let dynamic = image
        .try_into_dynamic()
        .map_err(|error| format!("readback is not a saveable format: {error}"))?;
    dynamic
        .to_rgb8()
        .save(path)
        .map_err(|error| error.to_string())
}

/// What the observer leaves behind in the event once the pixels have been
/// moved out of it. One texel, no allocation: the readback is tens of
/// megabytes and copying it to get it off the frame would be most of what
/// this change is removing.
fn emptied_readback() -> Image {
    Image::new_uninit(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::empty(),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn capture_frame(
    mut commands: Commands,
    mut queue: ResMut<CaptureQueue>,
    screen: Option<Res<AppScreen>>,
    loading: Option<Res<LoadingScreen>>,
    has_world: Option<Res<HasWorld>>,
    stats: Option<Res<DpvsFrameStats>>,
    working: Option<Res<render_gpu::ColourWorkingSet>>,
    mut exit: MessageWriter<AppExit>,
) {
    let submitted_batches = stats.as_ref().map(|s| s.submitted_batches).unwrap_or(0);
    let g0_world = stats
        .as_ref()
        .map(|s| s.g0_world_surfs.len() as u32)
        .unwrap_or(0);
    let facts = CaptureFrameFacts {
        screen: screen.map(|s| *s).unwrap_or_default(),
        loading_overlay: loading.is_some(),
        has_world: has_world.map(|w| w.0).unwrap_or(false),
        submitted_batches,
        g0_world,
    };

    let pipelines_ready =
        !facts.has_world || working.is_some_and(|set| set.hits > 0 && set.pipeline_not_ready == 0);
    queue.settled_frames = if facts.content_up() && pipelines_ready {
        queue.settled_frames.saturating_add(1)
    } else {
        0
    };

    queue.collect_writes();
    take_ready_capture(&mut commands, &mut queue, facts);
    // A capture is four things — the request waiting, the readback, the encode
    // and the file write — and only the first two are on this thread at all.
    // The flag stays up while a worker is still writing so that the frames a
    // capture was in flight over are marked, and so a frame-performance pair
    // can be read with those frames left out.
    perf::frames::set_state(
        perf::frames::flag::SCREENSHOT,
        !queue.pending.is_empty() || queue.writing > 0,
    );

    if queue.write_completed {
        queue.write_completed = false;
    } else if queue.exit_when_drained && queue.pending.is_empty() && queue.writing == 0 {
        exit.write(AppExit::Success);
    }
}

fn take_ready_capture(commands: &mut Commands, queue: &mut CaptureQueue, facts: CaptureFrameFacts) {
    let settled_frames = queue.settled_frames;
    let Some(front) = queue.pending.front_mut() else {
        return;
    };
    front.waited_frames = front.waited_frames.saturating_add(1);
    let waited = front.waited_frames;

    match capture_wait(facts, settled_frames) {
        Some(wait) => {
            if front.queued_at.elapsed() >= CAPTURE_WAIT_LIMIT {
                let failed = queue.pending.pop_front().expect("front exists");
                diag::error!(
                    Launch,
                    "screenshot: gave up on {} after {} frames — {}",
                    failed.request.path.display(),
                    waited,
                    wait.label()
                );
                return;
            }

            if front.should_report(wait) {
                diag::info!(
                    Launch,
                    "screenshot: holding {} — {}",
                    front.request.path.display(),
                    wait.label()
                );
            }
        }
        None => {
            let ready = queue.pending.pop_front().expect("front exists");
            let path = ready.request.path.clone();
            diag::info!(
                Launch,
                "screenshot: writing {} (screen {:?}, world batches {}, g0_world {}, settled {} frames, \
                 queued {} frames ago)",
                path.display(),
                facts.screen,
                facts.submitted_batches,
                facts.g0_world,
                settled_frames,
                waited
            );
            queue.writing += 1;
            arm_write_drain_at_exit();
            let writes = queue.writes.clone();
            commands.spawn(Screenshot::primary_window()).observe(
                move |mut captured: On<ScreenshotCaptured>| {
                    // The readback landed here. What this frame pays is the
                    // move of the pixels out of the event; the PNG encode and
                    // the file write are an I/O worker's, and the completion
                    // comes back through `writes`.
                    perf::frames::mark(perf::frames::flag::SCREENSHOT);
                    let image =
                        std::mem::replace(&mut captured.event_mut().image, emptied_readback());
                    let path = path.clone();
                    let writes = writes.clone();
                    WRITES_IN_FLIGHT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    IoTaskPool::get()
                        .spawn(async move {
                            let error = write_capture(&path, image).err();
                            writes
                                .lock()
                                .unwrap_or_else(|poison| poison.into_inner())
                                .push(CaptureWrite { path, error });
                            WRITES_IN_FLIGHT.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                        })
                        .detach();
                },
            );
        }
    }
}

/// Finish what is still in flight before the process leaves, and say what
/// could not be finished.
///
/// A worker mid-encode is simply gone once the process exits, and the file it
/// was writing is absent or half a PNG. So the exit waits for the workers it
/// started — bounded, because a stuck write must cost seconds and a log line,
/// not the run. The same wait is armed on `atexit`, for the exits that leave
/// without an `AppExit` anyone can read; this one is where it gets said.
pub(crate) fn report_unwritten_captures(
    mut queue: ResMut<CaptureQueue>,
    mut exit: MessageReader<AppExit>,
) {
    if exit.read().next().is_none() {
        return;
    }
    let budget = exit_drain_budget();
    let until = std::time::Instant::now() + budget;
    while queue.writing > 0 && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(2));
        queue.collect_writes();
    }
    if queue.writing > 0 {
        diag::error!(
            Launch,
            "screenshot: {} write(s) had not finished after {}ms — the files they were encoding are incomplete",
            queue.writing,
            budget.as_millis(),
        );
    }
    for path in queue.pending_paths() {
        diag::error!(
            Launch,
            "screenshot: {} was never written — process exiting with the request queued",
            path.display()
        );
    }
}
