use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};

use crate::media::{PcmError, RenderMedia};
use crate::render_core::{
    Assignment, AudioScope, InstanceState, InstanceStatus, PHYSICAL_VOICES, QUANTUM, RenderShared,
    RenderVoiceId,
};

pub struct OfflineSound {
    pub samples: Arc<[f32]>,
    pub channels: u16,
    pub sample_rate: u32,
    pub looping: bool,
    pub gain: f32,
    pub speed: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfflineError {
    Media(PcmError),
    InvalidParameters,
    VoiceBudget,
}

pub struct OfflineVoice(Arc<InstanceState>);

impl OfflineVoice {
    pub fn status(&self) -> InstanceStatus {
        self.0.status()
    }
    pub fn has_reached(&self, status: InstanceStatus) -> bool {
        self.0.has_reached(status)
    }
    pub fn source_frame_position(&self) -> f64 {
        f64::from_bits(self.0.cursor.load(Ordering::Acquire))
    }
    pub fn stop(&self) {
        self.0.stopped.store(true, Ordering::Release);
    }
}

pub struct OfflineRenderer {
    shared: RenderShared,
    next_id: u64,
}

impl Default for OfflineRenderer {
    fn default() -> Self {
        Self {
            shared: RenderShared::new(),
            next_id: 1,
        }
    }
}

impl OfflineRenderer {
    pub fn audio_frame(&self) -> u64 {
        self.shared.frame.load(Ordering::Acquire)
    }

    pub fn set_master_volume(&mut self, gain: f32) -> Result<(), OfflineError> {
        if !gain.is_finite() || gain < 0.0 {
            return Err(OfflineError::InvalidParameters);
        }
        self.shared.master.store(gain.to_bits(), Ordering::Release);
        Ok(())
    }

    pub fn schedule(
        &mut self,
        sound: OfflineSound,
        start_frame: u64,
    ) -> Result<OfflineVoice, OfflineError> {
        if !sound.gain.is_finite()
            || sound.gain < 0.0
            || !sound.speed.is_finite()
            || !(0.01..=4.0).contains(&sound.speed)
        {
            return Err(OfflineError::InvalidParameters);
        }
        let media = RenderMedia::from_pcm(sound.samples, sound.channels, sound.sample_rate)
            .map_err(OfflineError::Media)?;
        self.retire_resources();
        let instance = Arc::new(InstanceState {
            id: self.next_id,
            scope: AudioScope::Menu,
            epoch: 0,
            stopped: AtomicBool::new(false),
            finish_attack: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            audible: AtomicBool::new(true),
            gain: AtomicU32::new(sound.gain.to_bits()),
            rate: AtomicU32::new(sound.speed.to_bits()),
            priority: AtomicU32::new(0),
            rejection: AtomicU8::new(0),
            cursor: AtomicU64::new(0.0f64.to_bits()),
            rendered_at: AtomicU64::new(0),
            requested_at: std::time::Instant::now(),
            first_device_us: AtomicU64::new(u64::MAX),
            first_device_frame: AtomicU64::new(u64::MAX),
            device_frames: AtomicU64::new(0),
            null_frames: AtomicU64::new(0),
            status: AtomicU8::new(InstanceStatus::Scheduled as u8),
            transitions: AtomicU8::new(
                (1 << InstanceStatus::Accepted as u8) | (1 << InstanceStatus::Scheduled as u8),
            ),
        });
        self.next_id += 1;
        let assignment = Assignment {
            voice: RenderVoiceId(self.next_id - 1),
            media,
            instance: instance.clone(),
            start_frame,
            start_cursor: 0.0,
            looping: sound.looping,
        };
        // &mut self guarantees the single publisher/reclaimer required by slots.
        unsafe { self.shared.publish(assignment) }.map_err(|_| OfflineError::VoiceBudget)?;
        Ok(OfflineVoice(instance))
    }

    pub fn render_block(&mut self, output: &mut [[f32; 2]; QUANTUM]) {
        self.shared.render(output);
    }

    pub fn retire_resources(&mut self) {
        for index in 0..PHYSICAL_VOICES {
            // Retirement is explicit so resource destruction stays outside rendering.
            if let Some(assignment) = unsafe { self.shared.reclaim(index) } {
                assignment.instance.retire();
            }
        }
    }
}
