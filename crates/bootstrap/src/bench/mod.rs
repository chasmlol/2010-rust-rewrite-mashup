//! `make bench`: one run, three independent reports and a package.
//!
//! [`load_report`] is the cost of getting a map on screen; [`frame_report`] is
//! the cost of a frame once it is there; [`counter_report`] is what the frame
//! asked the machine to do to cost that. They share a process and nothing else
//! — any of them can be MISS while the others stand — and the split between the
//! first two is the first playable frame, where the recorder switches phase.
//!
//! All three are printed to stdout at exit and written to
//! `iw4l-artifacts/bench/<stamp>.txt`, so a run can be diffed against the one
//! before it. The same text, the [`manifest`] that says what the run *was* and
//! the [`summary`] of the same numbers as data go to the run's own directory,
//! next to the trace it recorded. Off unless `IW4L_BENCH` is set.

mod counter_report;
mod facts;
mod frame_report;
mod frames_section;
mod identity;
mod load_report;
mod manifest;
mod milestones;
mod summary;
mod table;
mod tables;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use asset_transport::LoadProgress;
use bevy::prelude::*;
use render::diag::capture::{CaptureQueue, CaptureRequest};

use crate::bench::milestones::Milestones;

/// When the shell command was typed, when the recipe passes it. `make` stamps
/// `IW4L_BENCH_STARTED_NS` so the report can charge cargo and process startup
/// to the load instead of quietly starting the clock after them.
static COMMAND_START: OnceLock<Instant> = OnceLock::new();

/// Where the report is written, remembered so the exit hook can reach it.
static ARTIFACTS: OnceLock<PathBuf> = OnceLock::new();

/// The report is printed once, by whichever path gets there first.
static REPORTED: AtomicBool = AtomicBool::new(false);

/// Called before anything else in the process. Reads the environment once so no
/// later code pays for a `getenv` per span.
pub fn arm() {
    perf::stats::arm();
    asset_transport::load_jobs::arm(enabled());
    if !enabled() {
        return;
    }
    let now = Instant::now();
    match std::env::var("IW4L_BENCH_STARTED_NS") {
        Ok(stamp) => match shell_start(&stamp, now) {
            Some(started) => {
                let _ = COMMAND_START.set(started);
            }
            None => eprintln!(
                "bench: IW4L_BENCH_STARTED_NS is not a unix nanosecond stamp; the report will time from process start"
            ),
        },
        Err(std::env::VarError::NotPresent) => {}
        Err(error) => eprintln!("bench: IW4L_BENCH_STARTED_NS: {error}"),
    }
}

fn shell_start(stamp: &str, now: Instant) -> Option<Instant> {
    let stamp = stamp.parse::<u128>().ok()?;
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos()
        .checked_sub(stamp)?;
    now.checked_sub(std::time::Duration::from_nanos(
        u64::try_from(elapsed).ok()?,
    ))
}

/// Whether this process collects a bench report. The recorder in `perf` reads
/// the same variable; this is the one answer both use.
pub fn enabled() -> bool {
    perf::stats::enabled()
}

pub fn announce_runtime(app: &mut App) {
    app.add_systems(Last, facts::announce);
}

/// Arm the milestone watcher and queue the settled screenshot of the spawned
/// world. Called once, with the app assembled and the load request in place.
///
/// The capture waits for the loading overlay to come down, which a demo now
/// does as soon as the first snapshot is presented — so it settles during the
/// load, long before `quit` could be left owing it.
pub fn insert(
    app: &mut App,
    zone: &str,
    demo: Option<&str>,
    role: &str,
    artifacts: &Path,
    progress: LoadProgress,
) {
    facts::workload(zone, demo, role);
    facts::scheduling(app);
    // Off the main thread: a release binary is a few hundred megabytes and
    // digesting it at exit would charge the run it is describing.
    identity::spawn(
        demo.map(Path::new),
        artifacts.parent().unwrap_or_else(|| Path::new(".")),
    );
    let screenshot = milestones::screenshot_path(artifacts, zone);
    if let Some(parent) = screenshot.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        diag::error!(Launch, "bench: create {}: {error}", parent.display());
        return;
    }
    let _ = std::fs::remove_file(&screenshot);
    app.world_mut()
        .get_resource_mut::<CaptureQueue>()
        .expect("CaptureQueue: RenderPlugin must be added before the bench capture")
        .push(CaptureRequest {
            path: screenshot.clone(),
            exit_after_capture: false,
        });
    diag::info!(Launch, "bench: screenshot {}", screenshot.display());
    milestones::install(Milestones::new(zone.to_owned(), screenshot, progress));
    let _ = ARTIFACTS.set(artifacts.to_path_buf());
    arm_exit_hook();
    app.add_systems(Last, (milestones::poll, facts::collect));
    #[cfg(not(unix))]
    app.add_systems(PostUpdate, report_on_app_exit);
}

/// `std::process::exit` on Windows runs no `atexit` handler: report from
/// `PostUpdate` of the frame that sent `AppExit`, before `Last` exits.
#[cfg(not(unix))]
fn report_on_app_exit(mut exit: MessageReader<AppExit>) {
    if exit.read().last().is_none() {
        return;
    }
    if let Some(artifacts) = ARTIFACTS.get() {
        report(artifacts, None);
    }
}

/// would measure a whole run and print nothing.
#[cfg(unix)]
fn arm_exit_hook() {
    static ARMED: AtomicBool = AtomicBool::new(false);
    if ARMED.swap(true, Ordering::SeqCst) {
        return;
    }
    diag::exit::at_exit(|| {
        let Some(artifacts) = ARTIFACTS.get() else {
            return;
        };
        // Close the open frame before the trace is flushed, so the last wall
        // has an end event in the trace as well as a row in the table. Closed
        // after the flush, its Perfetto slice stays open and every span of
        // that frame reads as falling outside every wall.
        perf::stats::close_open_frame();
        // This hook was armed after Perfetto's, and exit hooks run
        // last-armed first, so the trace is still open here; flushing it is what gives the
        // heading a run directory to name. Flushing twice is a no-op.
        let trace = perf::flush().ok().flatten();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            report(artifacts, trace);
        }));
    });
}

#[cfg(not(unix))]
fn arm_exit_hook() {}

/// Print both reports and write them next to the run. `trace` is the Perfetto
/// run directory when one was recorded — the report names it rather than
/// reading it, because the two are different tools over the same run.
pub fn finish(artifacts: &Path, trace: Option<PathBuf>) {
    report(artifacts, trace);
}

fn report(artifacts: &Path, trace: Option<PathBuf>) {
    if REPORTED.swap(true, Ordering::SeqCst) {
        return;
    }
    // The clock opens a frame at the top of `First` and closes it at the top
    // of the next one, so the frame the process exits from has never been
    // closed. Closing it here is what puts its spans in a row; the row carries
    // `partial`, because its wall is how far the frame got and not a frame
    // time to compare against the others. The exit hook has usually done this
    // already, before flushing the trace; closing a closed clock does nothing.
    perf::stats::close_open_frame();
    let lines = match milestones::with(|bench| {
        bench.take_lane_snapshot();
        let mut lines = Vec::new();
        heading(&bench.zone, trace.as_deref(), &mut lines);
        load_report::render(bench, COMMAND_START.get().copied(), &mut lines);
        lines.push(String::new());
        frame_report::render(&mut lines);
        lines
    }) {
        Some(lines) => lines,
        None => {
            let mut lines = Vec::new();
            heading("<none>", trace.as_deref(), &mut lines);
            lines.push(
                "[1/3] MAP LOAD: MISS — this run loaded no map, so there was nothing to time."
                    .to_owned(),
            );
            lines.push(String::new());
            frame_report::render(&mut lines);
            lines
        }
    };

    let mut lines = lines;
    match write_report(artifacts, &lines) {
        Ok(path) => lines.push(format!("report: {}", path.display())),
        Err(error) => lines.push(format!("report: not written ({error})")),
    }
    for line in write_run_package(artifacts, &lines) {
        lines.push(line);
    }
    for line in &lines {
        diag::announce_stdout(line);
    }
    diag::info!(Launch, "bench: {} report lines", lines.len());
}

fn heading(zone: &str, trace: Option<&Path>, out: &mut Vec<String>) {
    let rule = "=".repeat(96);
    out.push(rule.clone());
    out.push(format!(
        "IW4L bench — zone={zone} command={:?}",
        std::env::args().collect::<Vec<_>>().join(" ")
    ));
    match trace {
        Some(dir) => out.push(format!(
            "perfetto run: {} (`cargo xtask bench` reads it; this report does not)",
            dir.join("trace.pftrace").display()
        )),
        None => out.push(
            "perfetto run: none — IW4L_PERF was off, or the flush failed. The report below is in-process and does not need it."
                .to_owned(),
        ),
    }
    out.push(rule);
    out.push(String::new());
}

fn write_report(artifacts: &Path, lines: &[String]) -> Result<PathBuf, String> {
    let dir = artifacts.join("bench");
    std::fs::create_dir_all(&dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
    let path = dir.join(format!("{}.txt", stamp()));
    let mut body = lines.join("\n");
    body.push('\n');
    std::fs::write(&path, body).map_err(|error| format!("write {}: {error}", path.display()))?;
    Ok(path)
}

/// The run's own directory: `report.txt`, `manifest.json` and `summary.json`
/// next to the `trace.pftrace` the same run wrote.
///
/// The flat `bench/<stamp>.txt` above stays where it is — it is the file a
/// human diffs against the last run and nothing should move it. This is the
/// package: one directory per run id, holding everything needed to read the
/// report months later without knowing what else was true that afternoon.
fn write_run_package(artifacts: &Path, lines: &[String]) -> Vec<String> {
    let dir = match perf::run::dir() {
        Ok(dir) => dir,
        Err(error) => return vec![format!("run package: not written ({error})")],
    };
    let mut out = Vec::new();

    let mut body = lines.join("\n");
    body.push('\n');
    let report = dir.join("report.txt");
    if let Err(error) = std::fs::write(&report, body) {
        out.push(format!(
            "run package: {} not written ({error})",
            report.display()
        ));
    }

    let facts = facts::snapshot();
    let path = dir.join("manifest.json");
    let manifest = manifest::build(&facts, perf::run::id().as_deref(), artifacts);
    match manifest::write(&path, &manifest) {
        Ok(()) => out.push(format!("run package: {}", dir.display())),
        Err(error) => out.push(format!("run package: manifest not written ({error})")),
    }

    let path = dir.join("summary.json");
    match summary::write(&path, &facts) {
        Ok(()) => {}
        Err(error) => out.push(format!("run package: summary not written ({error})")),
    }

    let frames = perf::frames::snapshot();
    let path = dir.join("frames.csv");
    if frames.rows.is_empty() {
        out.push(
            "run package: frames.csv not written — the per-frame recorder kept no row (IW4L_BENCH_FRAMES=0, or the run drew nothing)"
                .to_owned(),
        );
    } else {
        match tables::write_frames(&path, &frames) {
            Ok(()) => out.push(format!(
                "run package: frames.csv {} rows{}",
                frames.rows.len(),
                if frames.dropped == 0 {
                    String::new()
                } else {
                    format!(
                        ", {} dropped — the table held {} and the run was longer",
                        frames.dropped, frames.capacity
                    )
                },
            )),
            Err(error) => out.push(format!("run package: frames.csv not written ({error})")),
        }
    }

    let jobs = asset_transport::load_jobs::snapshot();
    let path = dir.join("load_jobs.csv");
    if jobs.rows.is_empty() {
        out.push("run package: load_jobs.csv not written — no load job was recorded".to_owned());
    } else {
        let origin = COMMAND_START
            .get()
            .copied()
            .or_else(|| milestones::with(|bench| bench.t0))
            .unwrap_or_else(Instant::now);
        match tables::write_jobs(&path, &jobs, origin) {
            Ok(()) => out.push(format!(
                "run package: load_jobs.csv {} rows ({} mode){}",
                jobs.rows.len(),
                if jobs.per_asset {
                    "per-asset"
                } else {
                    "per-class"
                },
                if jobs.dropped == 0 {
                    String::new()
                } else {
                    format!(", {} dropped", jobs.dropped)
                },
            )),
            Err(error) => out.push(format!("run package: load_jobs.csv not written ({error})")),
        }
    }
    out
}

/// Seconds since the epoch, zero-padded: the reports sort by name in the order
/// they were run, on every platform, with no date library.
fn stamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0);
    format!("{seconds:012}")
}
