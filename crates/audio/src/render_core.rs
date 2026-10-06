use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};

use crate::admission::AdmissionFailure;
use crate::media::RenderMedia;

pub const SAMPLE_RATE: u32 = 48_000;
pub const QUANTUM: usize = 128;
pub const PHYSICAL_VOICES: usize = 128;
pub(crate) const MIN_ATTACK_FRAMES: u64 = SAMPLE_RATE as u64 / 50;

const FREE: u8 = 0;
const PUBLISHED: u8 = 1;
const RETIRED: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AudioScope {
    Menu,
    Match,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InstanceStatus {
    Requested,
    Accepted,
    Scheduled,
    Started,
    Virtual,
    SourceEnded,
    Finished,
    Retired,
}

pub(crate) struct InstanceState {
    pub id: u64,
    pub scope: AudioScope,
    pub epoch: u64,
    pub stopped: AtomicBool,
    pub finish_attack: AtomicBool,
    pub paused: AtomicBool,
    pub audible: AtomicBool,
    pub gain: AtomicU32,
    pub rate: AtomicU32,
    pub priority: AtomicU32,
    pub rejection: AtomicU8,
    pub cursor: AtomicU64,
    pub rendered_at: AtomicU64,
    pub requested_at: std::time::Instant,
    pub first_device_us: AtomicU64,
    pub first_device_frame: AtomicU64,
    pub device_frames: AtomicU64,
    pub null_frames: AtomicU64,
    pub status: AtomicU8,
    pub transitions: AtomicU8,
}

impl InstanceState {
    pub fn status(&self) -> InstanceStatus {
        match self.status.load(Ordering::Acquire) {
            0 => InstanceStatus::Requested,
            1 => InstanceStatus::Accepted,
            2 => InstanceStatus::Scheduled,
            3 => InstanceStatus::Started,
            4 => InstanceStatus::Virtual,
            5 => InstanceStatus::SourceEnded,
            6 => InstanceStatus::Finished,
            _ => InstanceStatus::Retired,
        }
    }

    pub fn set_status(&self, status: InstanceStatus) {
        self.transitions
            .fetch_or(1 << status as u8, Ordering::Release);
        self.status.store(status as u8, Ordering::Release);
    }

    pub fn has_reached(&self, status: InstanceStatus) -> bool {
        self.transitions.load(Ordering::Acquire) & (1 << status as u8) != 0
    }

    pub fn retire(&self) {
        self.set_status(InstanceStatus::Finished);
        self.set_status(InstanceStatus::Retired);
    }

    pub fn reject(&self, reason: AdmissionFailure) {
        self.stopped.store(true, Ordering::Release);
        self.rejection.store(reason as u8, Ordering::Release);
        self.retire();
    }

    pub fn rejection(&self) -> Option<AdmissionFailure> {
        match self.rejection.load(Ordering::Acquire) {
            1 => Some(AdmissionFailure::QueueFull),
            2 => Some(AdmissionFailure::LogicalBudget),
            3 => Some(AdmissionFailure::PhysicalBudget),
            4 => Some(AdmissionFailure::Concurrency),
            5 => Some(AdmissionFailure::Cancelled),
            6 => Some(AdmissionFailure::StaleScope),
            7 => Some(AdmissionFailure::OutputUnavailable),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct RenderVoiceId(pub u64);

pub(crate) struct Assignment {
    pub voice: RenderVoiceId,
    pub media: RenderMedia,
    pub instance: Arc<InstanceState>,
    pub start_frame: u64,
    pub start_cursor: f64,
    pub looping: bool,
}

struct Slot {
    phase: AtomicU8,
    assignment: UnsafeCell<Option<Assignment>>,
}

// Only the control owner writes FREE slots or takes RETIRED assignments. Release/
// acquire phase changes transfer ownership. Rendering never destroys assignments.
unsafe impl Sync for Slot {}

impl Slot {
    fn new() -> Self {
        Self {
            phase: AtomicU8::new(FREE),
            assignment: UnsafeCell::new(None),
        }
    }
}

#[derive(Clone, Copy)]
struct Cursor {
    voice: RenderVoiceId,
    position: f64,
    gains: [crate::media::GainEnvelope; 3],
}

struct RenderState {
    cursors: [Cursor; PHYSICAL_VOICES],
}

pub(crate) struct RenderShared {
    slots: [Slot; PHYSICAL_VOICES],
    rendering: AtomicBool,
    state: UnsafeCell<RenderState>,
    pub frame: AtomicU64,
    pub match_epoch: AtomicU64,
    pub master: AtomicU32,
    pub busy_blocks: AtomicU64,
    pub device_active: AtomicBool,
    pub device_blocks: AtomicU64,
    pub device_callbacks: AtomicU64,
    pub device_required: AtomicBool,
    pub null_blocks: AtomicU64,
    pub device_underruns: AtomicU64,
    pub cancelled: AtomicBool,
    pub peak: AtomicU32,
}

// The bounded rendering lease exclusively owns RenderState. Slot payload access
// additionally follows Slot's phase protocol; control never touches RenderState.
unsafe impl Sync for RenderShared {}

struct RenderLease<'a>(&'a AtomicBool);

impl Drop for RenderLease<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl RenderShared {
    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| Slot::new()),
            rendering: AtomicBool::new(false),
            state: UnsafeCell::new(RenderState {
                cursors: [Cursor {
                    voice: RenderVoiceId(0),
                    position: 0.0,
                    gains: [crate::media::GainEnvelope::default(); 3],
                }; PHYSICAL_VOICES],
            }),
            frame: AtomicU64::new(0),
            match_epoch: AtomicU64::new(0),
            master: AtomicU32::new(1.0f32.to_bits()),
            busy_blocks: AtomicU64::new(0),
            device_active: AtomicBool::new(false),
            device_blocks: AtomicU64::new(0),
            device_callbacks: AtomicU64::new(0),
            device_required: AtomicBool::new(false),
            null_blocks: AtomicU64::new(0),
            device_underruns: AtomicU64::new(0),
            cancelled: AtomicBool::new(false),
            peak: AtomicU32::new(0),
        }
    }

    pub fn render(&self, output: &mut [[f32; 2]; QUANTUM]) -> bool {
        self.render_for(output, None)
    }

    pub fn render_for(&self, output: &mut [[f32; 2]; QUANTUM], device: Option<bool>) -> bool {
        output.fill([0.0; 2]);
        if self
            .rendering
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            self.busy_blocks.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        let _lease = RenderLease(&self.rendering);
        if device.is_some_and(|device| device != self.device_active.load(Ordering::Acquire)) {
            return false;
        }
        // The lease is held until every reference to this state has expired.
        let state = unsafe { &mut *self.state.get() };
        let frame = self.frame.load(Ordering::Relaxed);
        let epoch = self.match_epoch.load(Ordering::Acquire);
        let cancelled = self.cancelled.load(Ordering::Acquire);
        let hold_one_shots = device == Some(false) && self.device_required.load(Ordering::Relaxed);
        for (index, slot) in self.slots.iter().enumerate() {
            if slot.phase.load(Ordering::Acquire) != PUBLISHED {
                continue;
            }
            let ended = {
                // Control cannot replace a PUBLISHED payload until we retire it.
                let assignment = unsafe { (&*slot.assignment.get()).as_ref().unwrap() };
                let (ended, rendered) = render_assignment(
                    assignment,
                    &mut state.cursors[index],
                    frame,
                    epoch,
                    cancelled,
                    hold_one_shots,
                    output,
                );
                match device {
                    Some(true) => {
                        if rendered != 0
                            && assignment.instance.first_device_us.load(Ordering::Relaxed)
                                == u64::MAX
                        {
                            assignment
                                .instance
                                .first_device_frame
                                .store(frame.max(assignment.start_frame), Ordering::Relaxed);
                            assignment.instance.first_device_us.store(
                                assignment.instance.requested_at.elapsed().as_micros() as u64,
                                Ordering::Release,
                            );
                        }
                        assignment
                            .instance
                            .device_frames
                            .fetch_add(rendered as u64, Ordering::Relaxed);
                    }
                    Some(false) => {
                        assignment
                            .instance
                            .null_frames
                            .fetch_add(rendered as u64, Ordering::Relaxed);
                    }
                    None => {}
                }
                ended
            };
            if ended {
                slot.phase.store(RETIRED, Ordering::Release);
            }
        }
        let master = f32::from_bits(self.master.load(Ordering::Relaxed));
        let mut peak = f32::from_bits(self.peak.load(Ordering::Relaxed));
        for sample in output.iter_mut().flatten() {
            let value = *sample * master;
            *sample = if value.is_finite() {
                value.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            peak = peak.max(sample.abs());
        }
        self.peak.store(peak.to_bits(), Ordering::Relaxed);
        self.frame.store(frame + QUANTUM as u64, Ordering::Release);
        match device {
            Some(true) => {
                self.device_blocks.fetch_add(1, Ordering::Relaxed);
            }
            Some(false) => {
                self.null_blocks.fetch_add(1, Ordering::Relaxed);
            }
            None => {}
        }
        true
    }

    // These operations require the sole control owner, including in offline use.
    pub unsafe fn publish(&self, assignment: Assignment) -> Result<usize, Assignment> {
        let Some(index) = self
            .slots
            .iter()
            .position(|slot| slot.phase.load(Ordering::Acquire) == FREE)
        else {
            return Err(assignment);
        };
        let slot = &self.slots[index];
        unsafe { *slot.assignment.get() = Some(assignment) };
        slot.phase.store(PUBLISHED, Ordering::Release);
        Ok(index)
    }

    pub unsafe fn reclaim(&self, index: usize) -> Option<Assignment> {
        let slot = self.slots.get(index)?;
        if slot.phase.load(Ordering::Acquire) != RETIRED {
            return None;
        }
        let assignment = unsafe { (&mut *slot.assignment.get()).take() };
        slot.phase.store(FREE, Ordering::Release);
        assignment
    }
}

fn render_assignment(
    assignment: &Assignment,
    cursor: &mut Cursor,
    frame: u64,
    epoch: u64,
    cancelled: bool,
    hold_one_shots: bool,
    output: &mut [[f32; 2]; QUANTUM],
) -> (bool, usize) {
    let instance = &assignment.instance;
    if cancelled
        || instance.stopped.load(Ordering::Acquire)
        || (instance.scope == AudioScope::Match && instance.epoch != epoch)
        || assignment.media.release.as_ref().is_some_and(|release| {
            release.expired(frame)
                || (release.requested() && !instance.has_reached(InstanceStatus::Started))
        })
    {
        instance.set_status(InstanceStatus::Finished);
        return (true, 0);
    }
    if hold_one_shots && !assignment.looping {
        return (false, 0);
    }
    let rate = f32::from_bits(instance.rate.load(Ordering::Relaxed));
    let step = f64::from(assignment.media.rate()) / f64::from(SAMPLE_RATE) * f64::from(rate);
    if cursor.voice != assignment.voice {
        cursor.voice = assignment.voice;
        cursor.position = assignment.start_cursor;
        cursor.gains = [crate::media::GainEnvelope::default(); 3];
        if assignment.looping && !instance.paused.load(Ordering::Relaxed) {
            cursor.position += frame.saturating_sub(assignment.start_frame) as f64 * step;
        }
    }
    if !instance.audible.load(Ordering::Acquire) {
        instance
            .cursor
            .store(cursor.position.to_bits(), Ordering::Release);
        instance.rendered_at.store(frame, Ordering::Release);
        instance.set_status(InstanceStatus::Virtual);
        return (true, 0);
    }
    if instance.paused.load(Ordering::Relaxed) {
        return (false, 0);
    }
    let first = assignment
        .start_frame
        .saturating_sub(frame)
        .min(QUANTUM as u64) as usize;
    if first == QUANTUM {
        return (false, 0);
    }
    instance.set_status(InstanceStatus::Started);
    let gain = f32::from_bits(instance.gain.load(Ordering::Relaxed));
    let frames = assignment.media.frames();
    let gains = assignment.media.gains(&mut cursor.gains);
    let mut rendered = 0;
    for (offset, target) in output[first..].iter_mut().enumerate() {
        let audio_frame = frame + first as u64 + offset as u64;
        if assignment
            .media
            .release
            .as_ref()
            .is_some_and(|release| release.expired(audio_frame))
        {
            instance
                .cursor
                .store(cursor.position.to_bits(), Ordering::Release);
            instance.rendered_at.store(audio_frame, Ordering::Release);
            instance.set_status(InstanceStatus::Finished);
            return (true, rendered);
        }
        if cursor.position >= frames as f64 {
            if assignment.looping {
                cursor.position %= frames as f64;
            } else {
                instance
                    .cursor
                    .store(cursor.position.to_bits(), Ordering::Release);
                instance.set_status(InstanceStatus::SourceEnded);
                return (true, rendered);
            }
        }
        let sample = assignment
            .media
            .interpolate(cursor.position, assignment.looping);
        let gains = gains.sample(audio_frame);
        target[0] += sample[0] * gain * gains[0];
        target[1] += sample[1] * gain * gains[1];
        cursor.position += step;
        rendered += 1;
    }
    instance
        .cursor
        .store(cursor.position.to_bits(), Ordering::Release);
    instance
        .rendered_at
        .store(frame + QUANTUM as u64, Ordering::Release);
    if !assignment.looping && cursor.position >= frames as f64 {
        instance.set_status(InstanceStatus::SourceEnded);
        return (true, rendered);
    }
    if instance.finish_attack.load(Ordering::Acquire)
        && instance.device_frames.load(Ordering::Relaxed)
            + instance.null_frames.load(Ordering::Relaxed)
            + rendered as u64
            >= MIN_ATTACK_FRAMES
    {
        instance.set_status(InstanceStatus::Finished);
        return (true, rendered);
    }
    (false, rendered)
}
