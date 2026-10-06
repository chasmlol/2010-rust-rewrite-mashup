#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PassColorSpace {
    #[default]
    Linear,
    GammaEncoded,
    Unknown,
}

impl PassColorSpace {
    pub const fn port_mix(self) -> u64 {
        match self {
            Self::Linear => 0,
            Self::GammaEncoded => 0xC01C_5ACE,
            Self::Unknown => 0xA11C_0001,
        }
    }
}
