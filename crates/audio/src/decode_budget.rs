use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

const DECODE_LIMIT_BYTES: usize = 64 * 1024 * 1024;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static REFUSED: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug)]
pub struct DecodeMemory {
    pub limit_bytes: usize,
    pub live_bytes: usize,
    pub peak_bytes: usize,
    pub refused: u64,
}

pub fn decode_memory() -> DecodeMemory {
    DecodeMemory {
        limit_bytes: DECODE_LIMIT_BYTES,
        live_bytes: LIVE.load(Ordering::Acquire),
        peak_bytes: PEAK.load(Ordering::Relaxed),
        refused: REFUSED.load(Ordering::Relaxed),
    }
}

pub(crate) struct DecodeReservation(usize);

impl DecodeReservation {
    pub fn reserve(bytes: usize) -> Result<Self, crate::media::PcmError> {
        if let Ok(before) = LIVE.fetch_update(Ordering::AcqRel, Ordering::Relaxed, |live| {
            live.checked_add(bytes)
                .filter(|&total| total <= DECODE_LIMIT_BYTES)
        }) {
            PEAK.fetch_max(before + bytes, Ordering::Relaxed);
            return Ok(Self(bytes));
        }
        REFUSED.fetch_add(1, Ordering::Relaxed);
        Err(crate::media::PcmError::MemoryLimit)
    }
}

impl Drop for DecodeReservation {
    fn drop(&mut self) {
        LIVE.fetch_sub(self.0, Ordering::AcqRel);
    }
}

pub(crate) struct DecodeSamples {
    chunks: Vec<Box<[i16]>>,
    current: Vec<i16>,
    len: usize,
    reservation: crate::pcm_budget::PcmReservation,
    reserved_chunks: usize,
    expected_len: Option<usize>,
}

impl asset_audio::XwmaPcmSink for DecodeSamples {
    type Workspace = DecodeReservation;

    fn reserve_workspace(
        &mut self,
        bytes: usize,
    ) -> Result<Self::Workspace, asset_audio::XwmaDecodeError> {
        DecodeReservation::reserve(bytes).map_err(|_| asset_audio::XwmaDecodeError::MemoryLimit)
    }

    fn extend(&mut self, samples: &[i16]) -> Result<(), asset_audio::XwmaDecodeError> {
        self.extend(samples)
            .map_err(|_| asset_audio::XwmaDecodeError::MemoryLimit)
    }

    fn sample_count(&self) -> usize {
        self.len()
    }

    fn visit_samples(
        &self,
        visitor: &mut dyn FnMut(&[i16]) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        self.visit_samples(visitor)
    }
}

impl DecodeSamples {
    pub fn new() -> Result<Self, crate::media::PcmError> {
        Ok(Self {
            chunks: Vec::new(),
            current: Vec::new(),
            len: 0,
            reservation: crate::pcm_budget::PcmReservation::reserve(0)?,
            reserved_chunks: 0,
            expected_len: None,
        })
    }

    pub fn for_frames(
        frames: usize,
        channels: u16,
        rate: u32,
    ) -> Result<Self, crate::media::PcmError> {
        let samples = frames
            .checked_mul(usize::from(channels))
            .ok_or(crate::media::PcmError::MemoryLimit)?;
        crate::media::PcmBuffer::validate_size(samples, channels, rate)?;
        let chunks = samples.div_ceil(crate::media::CHUNK_SAMPLES);
        let bytes = samples
            .checked_mul(size_of::<i16>())
            .ok_or(crate::media::PcmError::MemoryLimit)?;
        Ok(Self {
            reservation: crate::pcm_budget::PcmReservation::reserve(bytes)?,
            reserved_chunks: chunks,
            expected_len: Some(samples),
            chunks: Vec::new(),
            current: Vec::new(),
            len: 0,
        })
    }

    pub fn extend(&mut self, mut samples: &[i16]) -> Result<(), crate::media::PcmError> {
        use crate::media::CHUNK_SAMPLES;
        if self.expected_len.is_some_and(|expected| {
            self.len
                .checked_add(samples.len())
                .is_none_or(|len| len > expected)
        }) {
            return Err(crate::media::PcmError::PartialFrame);
        }
        while !samples.is_empty() {
            if self.current.len() == CHUNK_SAMPLES || self.current.capacity() == 0 {
                if !self.current.is_empty() {
                    self.chunks
                        .push(std::mem::take(&mut self.current).into_boxed_slice());
                }
                let additional = if self.reserved_chunks == 0 {
                    Some(crate::pcm_budget::PcmReservation::reserve(
                        CHUNK_SAMPLES * size_of::<i16>(),
                    )?)
                } else {
                    None
                };
                let capacity = self.expected_len.map_or(CHUNK_SAMPLES, |expected| {
                    (expected - self.len).min(CHUNK_SAMPLES)
                });
                let mut current = Vec::new();
                current
                    .try_reserve_exact(capacity)
                    .map_err(|_| crate::media::PcmError::MemoryLimit)?;
                if let Some(reservation) = additional {
                    self.reservation.absorb(reservation);
                } else {
                    self.reserved_chunks -= 1;
                }
                self.current = current;
            }
            let take = samples.len().min(CHUNK_SAMPLES - self.current.len());
            self.current.extend_from_slice(&samples[..take]);
            self.len += take;
            samples = &samples[take..];
        }
        Ok(())
    }

    pub fn into_pcm(
        mut self,
        channels: u16,
        rate: u32,
    ) -> Result<crate::media::PcmBuffer, crate::media::PcmError> {
        if self
            .expected_len
            .is_some_and(|expected| self.len != expected)
        {
            return Err(crate::media::PcmError::PartialFrame);
        }
        if !self.current.is_empty() {
            self.chunks.push(self.current.into_boxed_slice());
        }
        self.reservation.shrink_to(
            self.len
                .checked_mul(size_of::<i16>())
                .ok_or(crate::media::PcmError::MemoryLimit)?,
        )?;
        crate::media::PcmBuffer::from_chunks(
            self.chunks.into_boxed_slice(),
            self.len,
            channels,
            rate,
            self.reservation,
        )
    }

    pub fn visit_samples(
        &self,
        visitor: &mut dyn FnMut(&[i16]) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        for chunk in &self.chunks {
            visitor(chunk)?;
        }
        visitor(&self.current)
    }

    pub fn len(&self) -> usize {
        self.len
    }
}
