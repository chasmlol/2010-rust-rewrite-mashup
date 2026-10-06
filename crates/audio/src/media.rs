use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct LivePan {
    channels: Arc<AtomicU64>,
}

impl LivePan {
    pub fn unity() -> Self {
        Self::new(1.0, 1.0)
    }

    pub fn new(left: f32, right: f32) -> Self {
        Self {
            channels: Arc::new(AtomicU64::new(
                u64::from(left.to_bits()) | (u64::from(right.to_bits()) << 32),
            )),
        }
    }

    pub fn set(&self, left: f32, right: f32) {
        self.channels.store(
            u64::from(left.to_bits()) | (u64::from(right.to_bits()) << 32),
            Ordering::Relaxed,
        );
    }

    pub fn get(&self) -> (f32, f32) {
        let channels = self.channels.load(Ordering::Relaxed);
        (
            f32::from_bits(channels as u32),
            f32::from_bits((channels >> 32) as u32),
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GainEnvelope {
    from: f32,
    to: f32,
    start: u64,
    end: u64,
}

impl Default for GainEnvelope {
    fn default() -> Self {
        Self {
            from: 1.0,
            to: 1.0,
            start: 0,
            end: 0,
        }
    }
}

impl GainEnvelope {
    pub fn sample(self, frame: u64) -> f32 {
        if frame < self.start {
            return self.from;
        }
        if frame >= self.end || self.end <= self.start {
            return self.to;
        }
        let t = (frame - self.start) as f64 / (self.end - self.start) as f64;
        (f64::from(self.from) + (f64::from(self.to) - f64::from(self.from)) * t) as f32
    }
}

#[derive(Debug)]
struct GainState {
    writer: Mutex<GainEnvelope>,
    sequence: AtomicU64,
    values: AtomicU64,
    start: AtomicU64,
    end: AtomicU64,
}

#[derive(Clone, Debug)]
pub(crate) struct LiveGain(Arc<GainState>);

impl Default for LiveGain {
    fn default() -> Self {
        Self(Arc::new(GainState {
            writer: Mutex::new(GainEnvelope::default()),
            sequence: AtomicU64::new(0),
            values: AtomicU64::new(
                u64::from(1.0f32.to_bits()) | (u64::from(1.0f32.to_bits()) << 32),
            ),
            start: AtomicU64::new(0),
            end: AtomicU64::new(0),
        }))
    }
}

impl LiveGain {
    pub(crate) fn set(&self, gain: f32) {
        self.fade(0, gain, 0);
    }
    pub(crate) fn fade(&self, frame: u64, to: f32, frames: u64) {
        let mut current = self
            .0
            .writer
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let to = if to.is_finite() { to.max(0.0) } else { 0.0 };
        *current = GainEnvelope {
            from: current.sample(frame),
            to,
            start: frame,
            end: frame.saturating_add(frames),
        };
        self.0.sequence.fetch_add(1, Ordering::SeqCst);
        self.0.values.store(
            u64::from(current.from.to_bits()) | (u64::from(current.to.to_bits()) << 32),
            Ordering::SeqCst,
        );
        self.0.start.store(current.start, Ordering::SeqCst);
        self.0.end.store(current.end, Ordering::SeqCst);
        self.0.sequence.fetch_add(1, Ordering::SeqCst);
    }
    fn snapshot(&self, cached: &mut GainEnvelope) {
        // A concurrent writer leaves the render-owned previous snapshot intact;
        // all fields are atomic and SeqCst orders the two sequence observations.
        let before = self.0.sequence.load(Ordering::SeqCst);
        if before & 1 != 0 {
            return;
        }
        let values = self.0.values.load(Ordering::SeqCst);
        let next = GainEnvelope {
            from: f32::from_bits(values as u32),
            to: f32::from_bits((values >> 32) as u32),
            start: self.0.start.load(Ordering::SeqCst),
            end: self.0.end.load(Ordering::SeqCst),
        };
        if before == self.0.sequence.load(Ordering::SeqCst) {
            *cached = next;
        }
    }
}

pub(crate) struct RenderGains {
    pan: [f32; 2],
    envelopes: [GainEnvelope; 3],
}

impl RenderGains {
    pub fn sample(&self, frame: u64) -> [f32; 2] {
        let gain = self
            .envelopes
            .iter()
            .map(|envelope| envelope.sample(frame))
            .product::<f32>();
        [self.pan[0] * gain, self.pan[1] * gain]
    }
}

pub(crate) struct CueRelease {
    gain: LiveGain,
    end: AtomicU64,
}

impl Default for CueRelease {
    fn default() -> Self {
        Self {
            gain: LiveGain::default(),
            end: AtomicU64::new(u64::MAX),
        }
    }
}

impl CueRelease {
    pub fn fade_in(&self, frame: u64, frames: u64) {
        self.gain.set(0.0);
        self.gain.fade(frame, 1.0, frames);
    }
    pub fn fade_to(&self, frame: u64, to: f32, frames: u64) -> bool {
        if self.requested() {
            return false;
        }
        self.gain.fade(frame, to, frames);
        true
    }
    pub fn release(&self, frame: u64, frames: u64) {
        self.gain.fade(frame, 0.0, frames);
        self.end.store(
            frame.saturating_add(frames).min(u64::MAX - 1),
            Ordering::Release,
        );
    }
    pub fn requested(&self) -> bool {
        self.end.load(Ordering::Acquire) != u64::MAX
    }
    pub fn expired(&self, frame: u64) -> bool {
        frame >= self.end.load(Ordering::Acquire)
    }
}

#[derive(Clone)]
pub(crate) struct RenderMedia {
    pcm: PcmBuffer,
    pub pan: Option<LivePan>,
    pub output_gain: [f32; 2],
    pub gain: Option<LiveGain>,
    pub channel_gain: Option<LiveGain>,
    pub release: Option<Arc<CueRelease>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcmError {
    UnsupportedChannels,
    UnsupportedBitDepth,
    InvalidRate,
    Empty,
    PartialFrame,
    NonFinite,
    MemoryLimit,
}

#[derive(Clone, Debug)]
pub(crate) struct PcmBuffer {
    allocation: Arc<PcmAllocation>,
    len: usize,
    channels: u16,
    rate: u32,
}

pub(crate) const CHUNK_SAMPLES: usize = 1 << 16;

#[derive(Debug)]
enum Samples {
    F32(Arc<[f32]>),
    Chunked(Box<[Box<[i16]>]>),
    Zone { bytes: Arc<[u8]>, wide: bool },
}

#[derive(Debug)]
struct PcmAllocation {
    samples: Samples,
    _reservation: crate::pcm_budget::PcmReservation,
}

impl PcmBuffer {
    pub(crate) fn validate_geometry(channels: u16, rate: u32) -> Result<(), PcmError> {
        if !(1..=2).contains(&channels) {
            return Err(PcmError::UnsupportedChannels);
        }
        if rate == 0 {
            return Err(PcmError::InvalidRate);
        }
        Ok(())
    }

    pub(crate) fn validate_size(samples: usize, channels: u16, rate: u32) -> Result<(), PcmError> {
        Self::validate_geometry(channels, rate)?;
        if samples == 0 {
            return Err(PcmError::Empty);
        }
        if !samples.is_multiple_of(usize::from(channels)) {
            return Err(PcmError::PartialFrame);
        }
        Ok(())
    }

    fn with_reservation(
        samples: Samples,
        len: usize,
        channels: u16,
        rate: u32,
        reservation: crate::pcm_budget::PcmReservation,
    ) -> Self {
        Self {
            allocation: Arc::new(PcmAllocation {
                samples,
                _reservation: reservation,
            }),
            len,
            channels,
            rate,
        }
    }

    pub fn from_pcm(samples: Arc<[f32]>, channels: u16, rate: u32) -> Result<Self, PcmError> {
        Self::validate_size(samples.len(), channels, rate)?;
        if samples.iter().any(|sample| !sample.is_finite()) {
            return Err(PcmError::NonFinite);
        }
        let reservation =
            crate::pcm_budget::PcmReservation::reserve(samples.len() * size_of::<f32>())?;
        let len = samples.len();
        Ok(Self::with_reservation(
            Samples::F32(samples),
            len,
            channels,
            rate,
            reservation,
        ))
    }

    pub(crate) fn from_chunks(
        chunks: Box<[Box<[i16]>]>,
        len: usize,
        channels: u16,
        rate: u32,
        reservation: crate::pcm_budget::PcmReservation,
    ) -> Result<Self, PcmError> {
        Self::validate_size(len, channels, rate)?;
        Ok(Self::with_reservation(
            Samples::Chunked(chunks),
            len,
            channels,
            rate,
            reservation,
        ))
    }

    pub(crate) fn from_zone(
        bytes: Arc<[u8]>,
        bits: i32,
        channels: u16,
        rate: u32,
    ) -> Result<Self, PcmError> {
        Self::validate_geometry(channels, rate)?;
        let wide = match bits {
            8 => false,
            16 => true,
            _ => return Err(PcmError::UnsupportedBitDepth),
        };
        let sample_bytes = if wide { 2 } else { 1 };
        if !bytes
            .len()
            .is_multiple_of(sample_bytes * usize::from(channels))
        {
            return Err(PcmError::PartialFrame);
        }
        let len = bytes.len() / sample_bytes;
        Self::validate_size(len, channels, rate)?;
        let reservation = crate::pcm_budget::PcmReservation::reserve(0)?;
        Ok(Self::with_reservation(
            Samples::Zone { bytes, wide },
            len,
            channels,
            rate,
            reservation,
        ))
    }

    pub(crate) fn prefix_frames(&self, frames: usize) -> Result<Self, PcmError> {
        let samples = frames
            .checked_mul(usize::from(self.channels))
            .ok_or(PcmError::PartialFrame)?;
        if samples == 0 {
            return Err(PcmError::Empty);
        }
        if samples > self.len {
            return Err(PcmError::PartialFrame);
        }
        Ok(Self {
            len: samples,
            ..self.clone()
        })
    }

    fn sample(&self, index: usize) -> f32 {
        match &self.allocation.samples {
            Samples::F32(samples) => samples[index],
            Samples::Chunked(chunks) => {
                f32::from(chunks[index / CHUNK_SAMPLES][index % CHUNK_SAMPLES]) / 32768.0
            }
            Samples::Zone { bytes, wide: true } => {
                f32::from(i16::from_le_bytes([bytes[index * 2], bytes[index * 2 + 1]])) / 32768.0
            }
            Samples::Zone { bytes, wide: false } => (f32::from(bytes[index]) - 128.0) / 128.0,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }
    pub(crate) fn resident_bytes(&self) -> usize {
        match &self.allocation.samples {
            Samples::F32(samples) => samples.len() * size_of::<f32>(),
            Samples::Chunked(chunks) => {
                chunks.iter().map(|chunk| chunk.len()).sum::<usize>() * size_of::<i16>()
            }
            Samples::Zone { .. } => 0,
        }
    }
    pub(crate) fn shared(&self) -> bool {
        Arc::strong_count(&self.allocation) > 1
    }
    pub(crate) fn channels(&self) -> u16 {
        self.channels
    }
    pub(crate) fn rate(&self) -> u32 {
        self.rate
    }
}

impl RenderMedia {
    pub fn from_pcm(samples: Arc<[f32]>, channels: u16, rate: u32) -> Result<Self, PcmError> {
        PcmBuffer::from_pcm(samples, channels, rate).map(Self::from_buffer)
    }

    pub(crate) fn from_buffer(pcm: PcmBuffer) -> Self {
        Self {
            pcm,
            pan: None,
            output_gain: [1.0; 2],
            gain: None,
            channel_gain: None,
            release: None,
        }
    }

    pub(crate) fn rate(&self) -> u32 {
        self.pcm.rate()
    }
    pub fn frames(&self) -> usize {
        self.pcm.len() / usize::from(self.pcm.channels)
    }

    pub fn gains(&self, cached: &mut [GainEnvelope; 3]) -> RenderGains {
        for (binding, cache) in [
            self.gain.as_ref(),
            self.channel_gain.as_ref(),
            self.release.as_ref().map(|release| &release.gain),
        ]
        .into_iter()
        .zip(cached.iter_mut())
        {
            if let Some(binding) = binding {
                binding.snapshot(cache);
            } else {
                *cache = GainEnvelope::default();
            }
        }
        let (left, right) = self.pan.as_ref().map_or((1.0, 1.0), LivePan::get);
        RenderGains {
            pan: [left, right],
            envelopes: *cached,
        }
    }

    fn frame(&self, index: usize) -> [f32; 2] {
        let start = index * usize::from(self.pcm.channels);
        let left = self.pcm.sample(start);
        let right = if self.pcm.channels == 1 {
            left
        } else {
            self.pcm.sample(start + 1)
        };
        [left * self.output_gain[0], right * self.output_gain[1]]
    }

    pub fn interpolate(&self, position: f64, looping: bool) -> [f32; 2] {
        let first = position as usize;
        let last = self.frames() - 1;
        let next = if first >= last {
            if looping { 0 } else { last }
        } else {
            first + 1
        };
        let a = self.frame(first.min(last));
        let b = self.frame(next);
        let t = (position - first as f64) as f32;
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
    }
}
