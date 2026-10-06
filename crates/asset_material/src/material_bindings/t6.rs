use super::{BindingWriter, FrameStep, MaterialFrameBindingInputs, float4_bits};
use crate::t6_techset::{
    CODE_T6_GRID_SH, CODE_T6_HDR_CONTROL_0, CODE_T6_HDR_CONTROL_1, CODE_T6_REFLECTION_SH,
    CODE_T6_SAMPLE_DECODE,
};

pub(super) fn prepare_exposure(steps: &mut Vec<FrameStep>, requested: &[u16]) {
    if requested.contains(&CODE_T6_HDR_CONTROL_0) {
        steps.push(FrameStep::ExponentialExposure);
    }
    super::push_static(
        steps,
        requested,
        vec![(CODE_T6_HDR_CONTROL_1, float4_bits([1.0, 0.0, 0.0, 0.0]))],
    );
    let rows = CODE_T6_REFLECTION_SH
        .into_iter()
        .chain(CODE_T6_GRID_SH)
        .zip([[1.0, 1.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0], [0.0; 4]].repeat(2))
        .map(|(i, r)| (i, float4_bits(r)))
        .collect();
    super::push_static(steps, requested, rows);
    if requested.contains(&CODE_T6_SAMPLE_DECODE) {
        steps.push(FrameStep::LightSampleDecode);
    }
}
pub(super) fn produce_exposure(sources: &mut BindingWriter<'_>, stops: Option<f32>) {
    let reciprocal = stops.unwrap_or(0.0).exp2();
    sources.set_constant_rows(
        CODE_T6_HDR_CONTROL_0,
        &[float4_bits([
            reciprocal.recip(),
            0.0,
            reciprocal,
            reciprocal,
        ])],
    );
}
pub(super) fn produce_sample_decode(
    sources: &mut BindingWriter<'_>,
    inputs: &MaterialFrameBindingInputs,
) {
    sources.set_constant_rows(
        CODE_T6_SAMPLE_DECODE,
        &[float4_bits([
            inputs.world.model_lighting_decode_scale,
            inputs.world.reflection_probe_alpha_weight,
            0.0,
            0.0,
        ])],
    );
}
