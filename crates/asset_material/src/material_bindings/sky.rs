use super::{BindingWriter, FrameStep, float4_bits};
use crate::t5_code_remap::{
    LEFTOVER_T5_CODE_BASE, T5_CODE_SKY_COLOR_MULTIPLIER, T5_CODE_SKY_TRANSITION,
};

pub(super) fn prepare(steps: &mut Vec<FrameStep>, requested: &[u16]) {
    let candidates = [
        18,
        19,
        20,
        LEFTOVER_T5_CODE_BASE + T5_CODE_SKY_TRANSITION,
        crate::t6_techset::CODE_T6_SKY_COLOR_MULTIPLIER,
        LEFTOVER_T5_CODE_BASE + T5_CODE_SKY_COLOR_MULTIPLIER,
    ];
    if super::demands_any(requested, &candidates) {
        let fixed = vec![
            (18, float4_bits([1.0, 0.0, 0.0, 0.0])),
            (19, float4_bits([0.0, 1.0, 0.0, 0.0])),
            (20, float4_bits([0.0, 0.0, 1.0, 0.0])),
            (LEFTOVER_T5_CODE_BASE + T5_CODE_SKY_TRANSITION, [0; 4]),
        ]
        .into_iter()
        .filter(|(i, _)| requested.contains(i))
        .collect();
        let targets = candidates[4..]
            .iter()
            .copied()
            .filter(|i| requested.contains(i))
            .collect();
        steps.push(FrameStep::Sky { fixed, targets });
    }
}

pub(super) fn produce(
    sources: &mut BindingWriter<'_>,
    fixed: &[(u16, [u32; 4])],
    targets: &[u16],
    authored: [f32; 4],
    forward_z: f32,
) {
    for &(index, row) in fixed {
        sources.set_constant_rows(index, &[row]);
    }
    if !targets.is_empty() {
        let row = float4_bits([sky_intensity(authored, forward_z); 4]);
        for &index in targets {
            sources.set_constant_rows(index, &[row]);
        }
    }
}
fn sky_intensity([angle0, angle1, factor0, factor1]: [f32; 4], forward_z: f32) -> f32 {
    let radians = f32::from_bits(0x3c8efa35);
    let cos0 = (((90.0 - angle0) * radians) as f64).cos() as f32;
    let cos1 = (((90.0 - angle1) * radians) as f64).cos() as f32;
    let delta = cos1 - cos0;
    let blend = if delta.abs() <= f32::from_bits(0x38d1b717) {
        0.0
    } else {
        let t = ((forward_z - cos0) / delta).clamp(0.0, 1.0);
        t * t
    };
    (1.0 - blend) * factor0 + blend * factor1
}
