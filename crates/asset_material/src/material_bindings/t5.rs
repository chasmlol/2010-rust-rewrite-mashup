use super::{BindingWriter, FrameStep, MaterialSunInputs, float4_bits};
use bevy::math::{Mat4, Vec4};
const VIEWPORT_ONE: f32 = 1.0;
const T5_HDRCONTROL_HOST_EXPOSURE: f32 = 1.0;
const CODE_LEFTOVER_T5_VPOSX: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_VPOSX_TO_WORLD;
const CODE_LEFTOVER_T5_VPOSY: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_VPOSY_TO_WORLD;
const CODE_LEFTOVER_T5_VPOS1: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_VPOS1_TO_WORLD;
const CODE_LEFTOVER_T5_EYEOFFSET: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_EYEOFFSET;
const CODE_LEFTOVER_T5_SUN_POSITION: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_SUN_POSITION;
const CODE_LEFTOVER_T5_SUN_DIFFUSE: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_SUN_DIFFUSE;
const CODE_LEFTOVER_T5_SUN_SPECULAR: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_SUN_SPECULAR;
const CODE_LEFTOVER_T5_HDRCONTROL_0: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_HDRCONTROL_0;
const CODE_LEFTOVER_T5_HDRCONTROL_1: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_HDRCONTROL_1;
const CODE_LEFTOVER_T5_LIGHT_HERO_SCALE: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_LIGHT_HERO_SCALE;
const CODE_LEFTOVER_T5_HERO_LIGHTING_R: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_HERO_LIGHTING_R;
const CODE_LEFTOVER_T5_HERO_LIGHTING_G: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_HERO_LIGHTING_G;
const CODE_LEFTOVER_T5_HERO_LIGHTING_B: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_HERO_LIGHTING_B;
const CODE_LEFTOVER_T5_GENERIC_PARAM4: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_GENERIC_PARAM4;
const CODE_LEFTOVER_T5_GENERIC_PARAM5: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_GENERIC_PARAM5;
const CODE_LEFTOVER_T5_GENERIC_PARAM6: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_GENERIC_PARAM6;
const CODE_LEFTOVER_T5_WIND_DIRECTION: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_WIND_DIRECTION;
const CODE_LEFTOVER_T5_GRASS_WIND_FORCE0: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_GRASS_WIND_FORCE0;
const CODE_LEFTOVER_T5_VARIANT_WIND_SPRING_0: u16 = crate::t5_code_remap::LEFTOVER_T5_CODE_BASE
    + crate::t5_code_remap::T5_CODE_VARIANT_WIND_SPRING_0;
const CODE_LEFTOVER_T5_TREECANOPY_PARMS: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_TREECANOPY_PARMS;
const CODE_LEFTOVER_T5_CUSTOMWIND_CENTER: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_CUSTOMWIND_CENTER;
const CODE_LEFTOVER_T5_CUSTOMWIND_SPRING: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_CUSTOMWIND_SPRING;
const CODE_LEFTOVER_T5_CHARACTER_CHARRED_AMOUNT: u16 = crate::t5_code_remap::LEFTOVER_T5_CODE_BASE
    + crate::t5_code_remap::T5_CODE_CHARACTER_CHARRED_AMOUNT;
const T5_HDRCONTROL_EXPOSURE_DIVISOR: f32 = 8.0;

pub(super) fn produce_leftover_t5_sun_constants(
    sources: &mut BindingWriter<'_>,
    light: &MaterialSunInputs,
) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_SUN_POSITION,
        &[float4_bits(lighting_iw4::dir_light_position(
            light.direction,
        ))],
    );
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_SUN_DIFFUSE,
        &[float4_bits(leftover_t5_sun_color(
            light.diffuse_color,
            light.color,
        ))],
    );
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_SUN_SPECULAR,
        &[float4_bits(leftover_t5_sun_color(
            light.specular_color,
            light.color,
        ))],
    );
}
fn leftover_t5_sun_color(t5: Option<[f32; 4]>, color: [f32; 3]) -> [f32; 4] {
    match t5 {
        Some(v) => [v[0], v[1], v[2], 1.0],
        None => [color[0], color[1], color[2], 1.0],
    }
}
pub(super) fn produce_leftover_t5_hdrcontrol(sources: &mut BindingWriter<'_>, exposure: f32) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_HDRCONTROL_0,
        &[float4_bits([
            exposure / T5_HDRCONTROL_EXPOSURE_DIVISOR,
            0.0,
            0.0,
            0.0,
        ])],
    );
}
fn produce_leftover_t5_light_hero_scale(sources: &mut BindingWriter<'_>) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_LIGHT_HERO_SCALE,
        &[float4_bits([1.0, 1.0, 1.0, 1.0])],
    );
}
fn produce_leftover_t5_hero_lighting_matrix(sources: &mut BindingWriter<'_>) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_HERO_LIGHTING_R,
        &[float4_bits([1.0, 0.0, 0.0, 0.0])],
    );
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_HERO_LIGHTING_G,
        &[float4_bits([0.0, 1.0, 0.0, 0.0])],
    );
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_HERO_LIGHTING_B,
        &[float4_bits([0.0, 0.0, 1.0, 0.0])],
    );
}
fn produce_leftover_t5_generic_param4(sources: &mut BindingWriter<'_>) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_GENERIC_PARAM4,
        &[float4_bits([1.0, 1.0, 1.0, 1.0])],
    );
}
fn produce_leftover_t5_generic_param5(sources: &mut BindingWriter<'_>) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_GENERIC_PARAM5,
        &[float4_bits([1.0, 1.0, 1.0, 1.0])],
    );
}
fn produce_leftover_t5_generic_param6(sources: &mut BindingWriter<'_>) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_GENERIC_PARAM6,
        &[float4_bits([1.0, 1.0, 1.0, 1.0])],
    );
}
fn produce_leftover_t5_wind_shader_constants(sources: &mut BindingWriter<'_>) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_WIND_DIRECTION,
        &[float4_bits([1.0, 0.0, 0.0, 0.0])],
    );
    for index in 0u16..16 {
        sources.set_constant_rows(
            CODE_LEFTOVER_T5_VARIANT_WIND_SPRING_0 + index,
            &[float4_bits([0.0, 0.0, 0.0, 0.0])],
        );
    }
}
fn produce_leftover_t5_custom_wind_constants(sources: &mut BindingWriter<'_>) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_CUSTOMWIND_CENTER,
        &[float4_bits([0.0, 0.0, 0.0, 0.0])],
    );
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_CUSTOMWIND_SPRING,
        &[float4_bits([0.0, 0.0, 0.0, 0.0])],
    );
}
fn produce_leftover_t5_character_charred_amount(sources: &mut BindingWriter<'_>) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_CHARACTER_CHARRED_AMOUNT,
        &[float4_bits([0.0, 0.0, 0.0, 0.0])],
    );
}
fn produce_leftover_t5_grass_wind_force0(sources: &mut BindingWriter<'_>) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_GRASS_WIND_FORCE0,
        &[float4_bits([0.0, 0.0, 0.0, 0.0])],
    );
}
pub(super) fn produce_leftover_t5_treecanopy_parms(
    sources: &mut BindingWriter<'_>,
    intensity: f32,
    amount: f32,
) {
    sources.set_constant_rows(
        CODE_LEFTOVER_T5_TREECANOPY_PARMS,
        &[float4_bits([intensity, amount, 0.0, 0.0])],
    );
}
pub(super) fn produce_leftover_t5_code_consts(
    sources: &mut BindingWriter<'_>,
    clip_from_view: Mat4,
    world_from_view: Mat4,
    rt_width: i32,
    rt_height: i32,
) {
    if rt_width <= 0 || rt_height <= 0 {
        return;
    }
    let inv_w = VIEWPORT_ONE / rt_width as f32;
    let inv_h = VIEWPORT_ONE / rt_height as f32;
    let p00 = clip_from_view.x_axis.x;
    let p11 = clip_from_view.y_axis.y;
    if p00.abs() < 1e-12 || p11.abs() < 1e-12 {
        return;
    }
    let scale_x = (-2.0 * inv_w) / p00;
    let scale_y = (2.0 * inv_h) / p11;

    let vposx = world_from_view.x_axis * scale_x;
    let vposy = world_from_view.y_axis * scale_y;
    let vpos1 = world_from_view * Vec4::new(1.0 / p00, -1.0 / p11, 1.0, 0.0);
    sources.set_constant_rows(CODE_LEFTOVER_T5_VPOSX, &[float4_bits(vposx.to_array())]);
    sources.set_constant_rows(CODE_LEFTOVER_T5_VPOSY, &[float4_bits(vposy.to_array())]);
    sources.set_constant_rows(CODE_LEFTOVER_T5_VPOS1, &[float4_bits(vpos1.to_array())]);
}

pub(super) fn prepare_camera(steps: &mut Vec<FrameStep>, requested: &[u16]) {
    if requested.contains(&CODE_LEFTOVER_T5_EYEOFFSET) {
        steps.push(FrameStep::EyeOffset(CODE_LEFTOVER_T5_EYEOFFSET));
    }
    if super::demands_any(
        requested,
        &[
            CODE_LEFTOVER_T5_VPOSX,
            CODE_LEFTOVER_T5_VPOSY,
            CODE_LEFTOVER_T5_VPOS1,
        ],
    ) {
        steps.push(FrameStep::ViewportToWorld);
    }
}
pub(super) fn prepare_sun(steps: &mut Vec<FrameStep>, requested: &[u16]) {
    if super::demands_any(
        requested,
        &[
            CODE_LEFTOVER_T5_SUN_POSITION,
            CODE_LEFTOVER_T5_SUN_DIFFUSE,
            CODE_LEFTOVER_T5_SUN_SPECULAR,
        ],
    ) {
        steps.push(FrameStep::Sun);
    }
}
pub(super) fn prepare_exposure(steps: &mut Vec<FrameStep>, requested: &[u16]) {
    if requested.contains(&CODE_LEFTOVER_T5_HDRCONTROL_0) {
        steps.push(FrameStep::LinearExposure {
            default: T5_HDRCONTROL_HOST_EXPOSURE,
        });
    }
    super::push_static(
        steps,
        requested,
        vec![(
            CODE_LEFTOVER_T5_HDRCONTROL_1,
            float4_bits([1.0, 0.0, 0.0, 0.0]),
        )],
    );
}
pub(super) fn prepare_defaults(steps: &mut Vec<FrameStep>, requested: &[u16]) {
    let mut bank = render_material::RuntimeCodeSources::default();
    let mut writer = BindingWriter {
        requested,
        sources: &mut bank,
    };
    produce_leftover_t5_light_hero_scale(&mut writer);
    produce_leftover_t5_hero_lighting_matrix(&mut writer);
    for code in [
        crate::t5_code_remap::T5_CODE_GENERIC_PARAM0,
        crate::t5_code_remap::T5_CODE_GENERIC_PARAM1,
        crate::t5_code_remap::T5_CODE_EXTRA_CAM_PARAM,
    ] {
        writer.set_constant_rows(
            crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + code,
            &[[0; 4]],
        );
    }
    super::push_static(
        steps,
        requested,
        requested
            .iter()
            .filter_map(|&i| bank.constant_arc(i).map(|r| (i, r[0])))
            .collect(),
    );
}
pub(super) fn prepare_water(steps: &mut Vec<FrameStep>, requested: &[u16]) {
    let base =
        crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_POSTFX_CONTROL0;
    let wave_number = f32::from_bits(0x40c9_0fdb);
    super::push_static(
        steps,
        requested,
        (0..4)
            .map(|i| (base + i, float4_bits([wave_number, 0.0, 1.0, 0.0])))
            .collect(),
    );
    if requested.contains(&(base + 4)) {
        let gravity = f32::from_bits(0x43c1_1c29);
        steps.push(FrameStep::Water {
            target: base + 4,
            angular_speed: ((wave_number * gravity) as f64).sqrt(),
        });
    }
    super::push_static(
        steps,
        requested,
        vec![(base + 5, [0; 4]), (base + 6, [0; 4])],
    );
}
pub(super) fn prepare_environment_defaults(steps: &mut Vec<FrameStep>, requested: &[u16]) {
    let mut bank = render_material::RuntimeCodeSources::default();
    let mut writer = BindingWriter {
        requested,
        sources: &mut bank,
    };
    produce_leftover_t5_generic_param4(&mut writer);
    produce_leftover_t5_generic_param5(&mut writer);
    produce_leftover_t5_generic_param6(&mut writer);
    produce_leftover_t5_wind_shader_constants(&mut writer);
    produce_leftover_t5_custom_wind_constants(&mut writer);
    produce_leftover_t5_grass_wind_force0(&mut writer);
    produce_leftover_t5_character_charred_amount(&mut writer);
    super::push_static(
        steps,
        requested,
        requested
            .iter()
            .filter_map(|&i| bank.constant_arc(i).map(|r| (i, r[0])))
            .collect(),
    );
    if requested.contains(&CODE_LEFTOVER_T5_TREECANOPY_PARMS) {
        steps.push(FrameStep::TreeScatter);
    }
}
