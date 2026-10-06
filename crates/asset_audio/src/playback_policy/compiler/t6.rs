use super::*;

pub(super) struct T6CueCompiler;

impl CueCompiler for T6CueCompiler {
    fn prepare(&self, _row: &CapturedAlias, _channel: Option<&EntChannel>) -> CueSemantics {
        CueSemantics {
            spatial: Some(Err(SpatialPolicyFailure::Unsupported(AssetNamespace::T6))),
            limits: [VoiceLimit::default(); 2],
            group: GroupSelection::Ungrouped,
            speaker_gains: None,
            mixer_group_supported: false,
            zero_volume_unity: true,
            streamed_decode: crate::StreamedDecodePolicy::Detected,
            secondary: (
                SecondaryActivation::OnPrimaryPrepared,
                SecondaryPolicySource::PrimaryPreparedCompatibility,
            ),
        }
    }
}
