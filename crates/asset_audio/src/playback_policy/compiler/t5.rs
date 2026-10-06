use super::*;

pub(super) struct T5CueCompiler;

impl CueCompiler for T5CueCompiler {
    fn prepare(&self, row: &CapturedAlias, _channel: Option<&EntChannel>) -> CueSemantics {
        CueSemantics {
            spatial: match row.flags {
                Some(flags) if flags & 2 != 0 => Some(native_curve(row)),
                Some(_) => None,
                None => Some(Err(SpatialPolicyFailure::Unsupported(AssetNamespace::T5))),
            },
            limits: [
                t5_limit(row, 25, row.limit_count, false),
                t5_limit(row, 27, row.entity_limit_count, true),
            ],
            group: row.flags.map_or(GroupSelection::Unknown, |flags| {
                GroupSelection::Index((flags >> 16) & 0x3f)
            }),
            speaker_gains: None,
            mixer_group_supported: false,
            zero_volume_unity: false,
            streamed_decode: crate::StreamedDecodePolicy::WmaContainerWithWaveCompatibility,
            secondary: (
                SecondaryActivation::OnResolution,
                SecondaryPolicySource::T5IndependentCompatibility,
            ),
        }
    }
}

fn t5_limit(row: &CapturedAlias, shift: u32, count: Option<u8>, per_emitter: bool) -> VoiceLimit {
    let Some(count) = count else {
        return VoiceLimit::default();
    };
    let mode = match (row.flags.unwrap_or(0) >> shift) & 3u32 {
        1 => VoiceLimitMode::Oldest,
        2 => VoiceLimitMode::Reject,
        3 => VoiceLimitMode::Priority,
        _ => VoiceLimitMode::Unlimited,
    };
    VoiceLimit {
        mode,
        count: if mode == VoiceLimitMode::Oldest {
            count.max(1)
        } else {
            count
        },
        per_emitter,
        source: if row.flags.is_none() {
            VoiceLimitSource::UnknownFlagsUnlimitedCompatibility
        } else if mode == VoiceLimitMode::Oldest && count == 0 {
            VoiceLimitSource::ZeroOldestCountOneCompatibility
        } else {
            VoiceLimitSource::Native
        },
    }
}
