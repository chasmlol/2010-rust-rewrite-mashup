mod compiler;

use std::sync::Arc;

use crate::{AssetNamespace, CapturedAlias, ChannelKey, EntChannel, VoicePriority};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VoiceLimitMode {
    #[default]
    Unlimited,
    Oldest,
    Reject,
    Priority,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VoiceLimitSource {
    #[default]
    NotAuthored,
    Native,
    UnknownFlagsUnlimitedCompatibility,
    ZeroOldestCountOneCompatibility,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VoiceLimit {
    pub mode: VoiceLimitMode,
    pub count: u8,
    pub per_emitter: bool,
    pub source: VoiceLimitSource,
}

#[derive(Clone, Copy, Debug)]
pub struct ChannelAdmission {
    pub key: ChannelKey,
    pub maximum: i32,
    pub restricted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpatialPolicyFailure {
    Unsupported(AssetNamespace),
    MissingChannel(AssetNamespace),
    MissingFalloffCurve,
    InvalidFalloffCurve,
}

#[derive(Clone, Debug)]
pub struct SpatialPlaybackPolicy {
    pub dist_min: f32,
    pub dist_max: f32,
    pub knots: Arc<[[f32; 2]]>,
    pub near_knots: Option<Arc<[[f32; 2]]>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScalarRangePolicy {
    Authored([f32; 2]),
    ZeroRangeUnityCompatibility,
}

impl ScalarRangePolicy {
    fn sample(self, random: f32) -> f32 {
        match self {
            Self::Authored([low, high]) => crate::lerp_range(low, high, random),
            Self::ZeroRangeUnityCompatibility => 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GroupGainPolicy {
    Ungrouped,
    Resolved {
        native_index: u32,
        scale: f32,
    },
    Invalid {
        native_index: u32,
        error: crate::MixerGroupError,
    },
    MissingGroupUnityCompatibility {
        native_index: u32,
    },
    UnknownIndexUnityCompatibility,
}

pub const MAX_SECONDARY_DEPTH: u8 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecondaryActivation {
    OnResolution,
    OnPrimaryPrepared,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecondaryPolicySource {
    T5IndependentCompatibility,
    PrimaryPreparedCompatibility,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerLifetime {
    ParentGroup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerPitch {
    IndependentAuthoredRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerFailure {
    Independent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecondaryLayerPolicy {
    pub alias: String,
    pub activation: SecondaryActivation,
    pub source: SecondaryPolicySource,
    pub lifetime: LayerLifetime,
    pub pitch: LayerPitch,
    pub failure: LayerFailure,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnsupportedCueFeature {
    StartDelay(i32),
    Chain(String),
    MixerGroup(String),
    SpeakerMap(String),
    SpeakerGains,
}

#[derive(Clone, Debug)]
pub struct CueCompositionPolicy {
    pub secondary: Option<SecondaryLayerPolicy>,
    pub unsupported: Arc<[UnsupportedCueFeature]>,
}

impl GroupGainPolicy {
    fn scale(self) -> Result<f32, crate::MixerGroupError> {
        match self {
            Self::Resolved { scale, .. } => Ok(scale),
            Self::Invalid { error, .. } => Err(error),
            _ => Ok(1.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopingPolicy {
    OneShot,
    Loop,
    UnknownOneShotCompatibility,
}

impl LoopingPolicy {
    pub fn is_looping(self) -> bool {
        self == Self::Loop
    }

    fn compile(authored: Option<bool>) -> Self {
        match authored {
            Some(false) => Self::OneShot,
            Some(true) => Self::Loop,
            None => Self::UnknownOneShotCompatibility,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AliasPlaybackPolicy {
    pub namespace: AssetNamespace,
    looping: LoopingPolicy,
    streamed_decode: crate::StreamedDecodePolicy,
    pub channel: Option<ChannelKey>,
    pub channel_admission: Option<ChannelAdmission>,
    pub priority: Option<VoicePriority>,
    pub spatial: Option<Result<SpatialPlaybackPolicy, SpatialPolicyFailure>>,
    pub limits: [VoiceLimit; 2],
    pub volume_range: ScalarRangePolicy,
    pub group_gain: GroupGainPolicy,
    pub pitch_range: ScalarRangePolicy,
    pub composition: CueCompositionPolicy,
    pub stereo_speaker_gains: Option<[[f32; 2]; 2]>,
    pub loaded_binding_origin: crate::LoadedBindingOrigin,
}

impl AliasPlaybackPolicy {
    pub fn volume(&self, random: f32) -> Result<f32, crate::MixerGroupError> {
        Ok(self.volume_range.sample(random) * self.group_gain.scale()?)
    }

    pub fn pitch(&self, random: f32) -> f32 {
        self.pitch_range.sample(random)
    }

    pub fn streamed_decode(&self) -> crate::StreamedDecodePolicy {
        self.streamed_decode
    }

    pub fn looping(&self) -> LoopingPolicy {
        self.looping
    }

    pub(crate) fn compile(
        catalog: &crate::SoundCatalog,
        alias: usize,
        variant: usize,
    ) -> Option<Self> {
        compiler::compile(catalog, alias, variant)
    }
}
