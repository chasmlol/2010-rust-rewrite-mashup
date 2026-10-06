#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum T5WmaProfile {
    Mono44100,
    Stereo48000,
}

impl T5WmaProfile {
    pub const FRAME_SAMPLES: usize = 2048;
    pub const PRIMING_FRAMES: usize = 2;

    pub fn from_geometry(channels: u32, rate: u32) -> Option<Self> {
        match (channels, rate) {
            (1, 44100) => Some(Self::Mono44100),
            (2, 48000) => Some(Self::Stereo48000),
            _ => None,
        }
    }

    pub const fn packet_bytes(self) -> usize {
        match self {
            Self::Mono44100 => 2230,
            Self::Stereo48000 => 4096,
        }
    }
}
