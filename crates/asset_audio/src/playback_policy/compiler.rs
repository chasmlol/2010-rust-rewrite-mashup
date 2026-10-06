use super::*;
mod common;
mod iw4;
mod iw5;
mod t5;
mod t6;
use common::native_curve;

struct CueSemantics {
    spatial: Option<Result<SpatialPlaybackPolicy, SpatialPolicyFailure>>,
    limits: [VoiceLimit; 2],
    group: GroupSelection,
    zero_volume_unity: bool,
    speaker_gains: Option<[[f32; 2]; 2]>,
    mixer_group_supported: bool,
    streamed_decode: crate::StreamedDecodePolicy,
    secondary: (SecondaryActivation, SecondaryPolicySource),
}

enum GroupSelection {
    Ungrouped,
    Index(u32),
    Unknown,
}

trait CueCompiler {
    fn prepare(&self, row: &CapturedAlias, channel: Option<&EntChannel>) -> CueSemantics;
}

pub(super) fn compile(
    catalog: &crate::SoundCatalog,
    alias: usize,
    variant: usize,
) -> Option<AliasPlaybackPolicy> {
    let sound = catalog.sound_at(alias)?;
    let row = sound.aliases.get(variant)?;
    let namespace = AssetNamespace::from_zone_game(sound.game);
    let channels = catalog.channels_in(namespace);
    let group_volumes = catalog.mixer_group_volumes_in(namespace);
    let channel = sound
        .ent_channel(variant)
        .map(|id| ChannelKey { namespace, id });
    let channel_info = channel.and_then(|key| channels?.get(key.id as usize));
    let compiler: &dyn CueCompiler = match namespace {
        AssetNamespace::Iw4 => &iw4::Iw4CueCompiler,
        AssetNamespace::Iw5 => &iw5::Iw5CueCompiler,
        AssetNamespace::T5 => &t5::T5CueCompiler,
        AssetNamespace::T6 => &t6::T6CueCompiler,
    };
    let semantics = compiler.prepare(row, channel_info);
    let group_gain = match semantics.group {
        GroupSelection::Index(native_index) => {
            match group_volumes.and_then(|volumes| volumes.get(native_index as usize)) {
                Some(&Ok(scale)) => GroupGainPolicy::Resolved {
                    native_index,
                    scale,
                },
                Some(&Err(error)) => GroupGainPolicy::Invalid {
                    native_index,
                    error,
                },
                None => GroupGainPolicy::MissingGroupUnityCompatibility { native_index },
            }
        }
        GroupSelection::Unknown => GroupGainPolicy::UnknownIndexUnityCompatibility,
        GroupSelection::Ungrouped => GroupGainPolicy::Ungrouped,
    };
    Some(AliasPlaybackPolicy {
        namespace,
        looping: LoopingPolicy::compile(row.is_looping()),
        streamed_decode: semantics.streamed_decode,
        composition: CueCompositionPolicy::compile(
            row,
            semantics.mixer_group_supported,
            semantics.speaker_gains,
            semantics.secondary.0,
            semantics.secondary.1,
        ),
        stereo_speaker_gains: semantics.speaker_gains,
        loaded_binding_origin: row.loaded_binding_origin,
        channel,
        channel_admission: channel
            .zip(channel_info)
            .map(|(key, info)| ChannelAdmission {
                key,
                maximum: info.max_voices,
                restricted: info.is_restricted,
            }),
        priority: row.voice_priority.clone(),
        spatial: semantics.spatial,
        limits: semantics.limits,
        volume_range: if semantics.zero_volume_unity && row.vol_min == 0.0 && row.vol_max == 0.0 {
            ScalarRangePolicy::ZeroRangeUnityCompatibility
        } else {
            ScalarRangePolicy::Authored([row.vol_min, row.vol_max])
        },
        group_gain,
        pitch_range: if row.pitch_min == 0.0 && row.pitch_max == 0.0 {
            ScalarRangePolicy::ZeroRangeUnityCompatibility
        } else {
            ScalarRangePolicy::Authored([row.pitch_min, row.pitch_max])
        },
    })
}

impl CueCompositionPolicy {
    fn compile(
        row: &CapturedAlias,
        mixer_group_supported: bool,
        speaker_gains: Option<[[f32; 2]; 2]>,
        activation: SecondaryActivation,
        source: SecondaryPolicySource,
    ) -> Self {
        let secondary = row
            .secondary
            .as_ref()
            .filter(|alias| !alias.is_empty())
            .map(|alias| SecondaryLayerPolicy {
                alias: alias.clone(),
                activation,
                source,
                lifetime: LayerLifetime::ParentGroup,
                pitch: LayerPitch::IndependentAuthoredRange,
                failure: LayerFailure::Independent,
            });
        let mut unsupported = Vec::new();
        if row.start_delay != 0 {
            unsupported.push(UnsupportedCueFeature::StartDelay(row.start_delay));
        }
        if let Some(name) = row.chain.as_ref().filter(|name| !name.is_empty()) {
            unsupported.push(UnsupportedCueFeature::Chain(name.clone()));
        }
        if !mixer_group_supported
            && let Some(name) = row.mixer_group.as_ref().filter(|name| !name.is_empty())
        {
            unsupported.push(UnsupportedCueFeature::MixerGroup(name.clone()));
        }
        if speaker_gains.is_none()
            && let Some(name) = row.speaker_map.as_ref().filter(|name| !name.is_empty())
        {
            unsupported.push(UnsupportedCueFeature::SpeakerMap(name.clone()));
        }
        if row.stereo_speaker_gains.is_some() && speaker_gains.is_none() {
            unsupported.push(UnsupportedCueFeature::SpeakerGains);
        }
        Self {
            secondary,
            unsupported: unsupported.into(),
        }
    }
}
