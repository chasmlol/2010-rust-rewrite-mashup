use std::path::PathBuf;

use crate::{SAB_FORMAT_FLAC, SAB_FORMAT_PCMS16, SabEntry};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SabCodec {
    PcmS16,
    Flac,
    Unsupported(u8),
}

#[derive(Clone, Debug)]
pub struct SabMediaSource {
    pub bank: PathBuf,
    pub entry: SabEntry,
}

impl SabMediaSource {
    pub fn codec(&self) -> SabCodec {
        match self.entry.format {
            SAB_FORMAT_PCMS16 => SabCodec::PcmS16,
            SAB_FORMAT_FLAC => SabCodec::Flac,
            format => SabCodec::Unsupported(format),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StreamedDecodePolicy {
    Detected,
    WmaContainerWithWaveCompatibility,
}
