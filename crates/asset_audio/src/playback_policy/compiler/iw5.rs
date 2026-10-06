use super::*;

pub(super) struct Iw5CueCompiler;

impl CueCompiler for Iw5CueCompiler {
    fn prepare(&self, row: &CapturedAlias, channel: Option<&EntChannel>) -> CueSemantics {
        CueSemantics {
            spatial: match channel {
                Some(info) if info.is_3d => Some(native_curve(row)),
                Some(_) => None,
                None => Some(Err(SpatialPolicyFailure::MissingChannel(
                    AssetNamespace::Iw5,
                ))),
            },
            limits: [VoiceLimit::default(); 2],
            group: row
                .vol_mod_index
                .map_or(GroupSelection::Ungrouped, GroupSelection::Index),
            speaker_gains: common::iw_speaker_gains(row),
            mixer_group_supported: true,
            zero_volume_unity: true,
            streamed_decode: crate::StreamedDecodePolicy::Detected,
            secondary: (
                SecondaryActivation::OnPrimaryPrepared,
                SecondaryPolicySource::PrimaryPreparedCompatibility,
            ),
        }
    }
}
