use super::{FrameStep, float4_bits};
const CODE_LEFTOVER_IW5_EYEOFFSET: u16 =
    crate::iw5_tech_map::LEFTOVER_IW5_CODE_BASE + crate::iw5_tech_map::IW5_CODE_EYEOFFSET;
const CODE_LEFTOVER_IW5_SAT_R: u16 =
    crate::iw5_tech_map::LEFTOVER_IW5_CODE_BASE + crate::iw5_tech_map::IW5_CODE_COLOR_SATURATION_R;
const CODE_LEFTOVER_IW5_SAT_G: u16 =
    crate::iw5_tech_map::LEFTOVER_IW5_CODE_BASE + crate::iw5_tech_map::IW5_CODE_COLOR_SATURATION_G;
const CODE_LEFTOVER_IW5_SAT_B: u16 =
    crate::iw5_tech_map::LEFTOVER_IW5_CODE_BASE + crate::iw5_tech_map::IW5_CODE_COLOR_SATURATION_B;
const R_FILM_TWEAK_SATURATION_DEFAULT: f32 = 1.0;

pub(super) fn prepare(steps: &mut Vec<FrameStep>, requested: &[u16]) {
    if requested.contains(&CODE_LEFTOVER_IW5_EYEOFFSET) {
        steps.push(FrameStep::EyeOffset(CODE_LEFTOVER_IW5_EYEOFFSET));
    }
    let rows = color_saturation_matrix(R_FILM_TWEAK_SATURATION_DEFAULT);
    super::push_static(
        steps,
        requested,
        [
            CODE_LEFTOVER_IW5_SAT_R,
            CODE_LEFTOVER_IW5_SAT_G,
            CODE_LEFTOVER_IW5_SAT_B,
        ]
        .into_iter()
        .zip(rows.map(float4_bits))
        .collect(),
    );
}
fn color_saturation_matrix(saturation: f32) -> [[f32; 4]; 3] {
    let r = (1.0 - saturation) * 0.25;
    let g = (1.0 - saturation) * 0.5;
    [
        [r + saturation, r, r, 0.0],
        [g, g + saturation, g, 0.0],
        [r, r, r + saturation, 0.0],
    ]
}
