use bevy::math::Mat4;
use bevy::prelude::*;
pub use render_frame::code_math::{
    camera_relative_view_projection, code_transpose_matrix_row4, code_transpose_matrix_rows,
    float4_bits,
};

use super::frame_products::{MaterialFrameInputs, MaterialGeneration};
use super::material_runtime::{RuntimeCodeSources, RuntimeImageId};
use crate::prepare::scene::camera::FpvLens;
use crate::prepare::scene::model_lighting_atlas::WorldModelLightingAtlas;
use crate::prepare::scene::view_parms::PreparedSceneView;
use hud_iw4::{GfxCmdBufSource2d, begin_view, gfx_scene_def_float_time};
use net::FrameClock;

pub use crate::prepare::scene::view_parms::{host_clip_from_view, pack_live_view_parms};

pub const FIRST_CODE_MATRIX: u16 = 0x4c;

pub const CODE_LIGHT_POSITION: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_POSITION;

pub const CODE_LIGHT_DIFFUSE: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_DIFFUSE;

pub const CODE_LIGHT_SPECULAR: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_SPECULAR;

pub const CODE_LIGHT_SPOTDIR: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_SPOTDIR;

pub const CODE_LIGHT_SPOTFACTORS: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_SPOTFACTORS;

pub const CODE_LIGHT_FALLOFF_PLACEMENT: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_FALLOFF_PLACEMENT;

pub const CODE_GAMETIME: u16 = 0x07;

const GAMETIME_TWO_PI: f64 = f64::from_bits(0x4019_21FB_6000_0000);

const GAMETIME_WRAP: f64 = f64::from_bits(0x40E5_1800_0000_0000);

pub const CODE_FOG: u16 = 0x25;

pub const CODE_FOG_COLOR_LINEAR: u16 = 0x26;

pub const CODE_FOG_COLOR_GAMMA: u16 = 0x27;

pub const CODE_FOG_SUN_CONSTS: u16 = 0x28;

pub const CODE_FOG_SUN_COLOR_LINEAR: u16 = 0x29;

pub const CODE_FOG_SUN_COLOR_GAMMA: u16 = 0x2a;

pub const CODE_FOG_SUN_DIR: u16 = 0x2b;

pub const CODE_RENDER_TARGET_SIZE: u16 = 0x16;

pub const CODE_CLIP_SPACE_LOOKUP_SCALE: u16 = 0x3f;

pub const CODE_CLIP_SPACE_LOOKUP_OFFSET: u16 = 0x40;

const VIEWPORT_ONE: f32 = 1.0;

const VIEWPORT_HALF: f32 = 0.5;

pub const CODE_LIGHTING_LOOKUP_SCALE: u16 = 0x22;

pub const CODE_BASE_LIGHTING_COORDS: u16 = 0x3a;

pub const CODE_LIGHT_PROBE_AMBIENT: u16 = 0x3b;

pub const CODE_MESH_ARG_0: u16 = 0x4a;

pub const CODE_MESH_ARG_1: u16 = 0x4b;

pub const CODE_TRANSPOSE_VIEW_PROJECTION: u16 = 0x56;

pub const CODE_TRANSPOSE_WORLD0: u16 = 0x62;

pub const CODE_TRANSPOSE_WORLD1: u16 = 0x6e;

pub const CODE_TRANSPOSE_WORLD2: u16 = 0x7a;

pub const CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0: u16 = 0x6a;

pub const CODE_TRANSPOSE_WORLD_VIEW_PROJECTION1: u16 = 0x76;
pub const CODE_TRANSPOSE_WORLD_VIEW_PROJECTION2: u16 = 0x82;

pub const CODE_TRANSPOSE_WORLD_VIEW0: u16 = 0x66;
pub const CODE_TRANSPOSE_WORLD_VIEW1: u16 = 0x72;
pub const CODE_TRANSPOSE_WORLD_VIEW2: u16 = 0x7e;

pub const CODE_INVERSE_WORLD_VIEW0: u16 = 0x65;

pub const CODE_INVERSE_TRANSPOSE_WORLD_VIEW0: u16 = 0x67;

pub const CODE_MATERIAL_COLOR: u16 = 0x24;

pub const CODE_TRANSPOSE_PROJECTION: u16 = 0x52;

pub const CODE_TRANSPOSE_WORLD_OUTDOOR_LOOKUP: u16 = 0x5e;

pub const CODE_TEXTURE_OUTDOOR: u32 = 0x0e;

pub const CODE_TEXTURE_OUTDOOR_SAMPLER: u8 = 0x62;

pub const CODE_DEPTH_FROM_CLIP: u16 = 0x49;

pub const R_ZNEAR_DEPTHHACK: f32 = hud_iw4::R_ZNEAR_DEPTHHACK_DEFAULT;

pub const CODE_TEXTURE_MODEL_LIGHTING: u32 = lighting_iw4::TEXTURE_SRC_CODE_MODEL_LIGHTING as u32;

pub const CODE_TEXTURE_MODEL_LIGHTING_SAMPLER: u8 = 0xe2;

pub const CODE_TEXTURE_LIGHT_ATTENUATION: u32 =
    lighting_iw4::TEXTURE_SRC_CODE_LIGHT_ATTENUATION as u32;

pub const CODE_TEXTURE_FLOATZ: u32 = 0x0f;

pub const CODE_TEXTURE_RESOLVED_POST_SUN: u32 = 9;

pub const CODE_TEXTURE_FLOATZ_SAMPLER: u8 = 0x61;

pub use super::fog::MapFrameFog;

pub use crate::prepare::scene::world::{
    LightAttenuationBind, MapDirPrimaryLight, MaterialLightOverrides,
};

#[derive(Clone, Copy, Debug, Resource)]
pub struct MapT5SunParseExposure {
    pub exposure: f32,
}

#[derive(Clone, Copy, Debug, Resource)]
pub struct MapT5TreeScatter {
    pub intensity: f32,
    pub amount: f32,
}

#[derive(Clone, Copy, Debug, Resource)]
pub struct MapOutdoor {
    pub image: Option<RuntimeImageId>,
    pub lookup: [u32; 16],
}

#[derive(Clone, Copy, Debug, Default, Resource)]
pub struct MapSunEffects {
    pub def: Option<render_frame::SunEffectsDef>,
}

#[derive(Clone, Debug, Default, Resource)]
pub struct MapPrimaryLights {
    pub lights: Vec<lighting_iw4::GfxLightPack>,

    pub attenuation: Vec<LightAttenuationBind>,

    pub overrides: Vec<MaterialLightOverrides>,

    pub reflection_probe_sh: Vec<Option<[[f32; 4]; 3]>>,

    pub dynamic: Option<DynamicLightBind>,
}

#[derive(Clone, Copy, Debug)]
pub struct DynamicLightBind {
    pub attenuation: LightAttenuationBind,
    pub falloff_image_width: Option<u16>,
    pub lmap_lookup_start: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GfxViewport {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandContextRefusal {
    MissingViewProjection,
    PrimaryLightNotDir,
    ViewportHasZeroRenderTarget,
}

pub fn default_world_transpose_rows(view_origin: Vec3) -> Vec<[u32; 4]> {
    code_transpose_matrix_rows(Mat4::from_translation(-view_origin))
}

#[must_use]
pub fn world_transpose_rows_from_set3d(world: [f32; 16]) -> Vec<[u32; 4]> {
    code_transpose_matrix_rows(Mat4::from_cols_array(&world))
}

pub use asset_material::fog_color_linear_and_gamma;

pub fn produce_frame_fog(
    sources: &mut RuntimeCodeSources,
    fog: &asset_world::ExpFog,
    r_fog_enabled: bool,
) -> Result<(), CommandContextRefusal> {
    let density = fog.density();
    let fog_row = [
        0.0,
        1.0 - fog.max_opacity,
        -density,
        fog.start_dist * density,
    ];
    sources.set_constant_rows(CODE_FOG, &[float4_bits(fog_row)]);

    let (linear, gamma) = fog_color_linear_and_gamma(fog.color_rgb, 1.0);
    sources.set_constant_rows(CODE_FOG_COLOR_LINEAR, &[float4_bits(linear)]);
    sources.set_constant_rows(CODE_FOG_COLOR_GAMMA, &[float4_bits(gamma)]);

    match fog.sun.as_ref() {
        Some(sun) => produce_iw4_sun_fog(sources, fog.density(), sun),
        None => {}
    }
    if !r_fog_enabled {
        sources.set_constant_rows(CODE_FOG, &[float4_bits([0.0, 1.0, 0.0, 0.0])]);
    }
    Ok(())
}

fn produce_iw4_sun_fog(sources: &mut RuntimeCodeSources, density: f32, sun: &asset_world::SunFog) {
    let begin = (sun.begin_angle_deg.to_radians()).cos();
    let end = (sun.end_angle_deg.to_radians()).cos();
    let mut slope = 100.0;
    if end < begin {
        slope = 1.0 / (begin - end);
    }
    let sun_consts = [-density * sun.scale, end, slope, -density];
    sources.set_constant_rows(CODE_FOG_SUN_CONSTS, &[float4_bits(sun_consts)]);

    let (sun_linear, sun_gamma) = fog_color_linear_and_gamma(sun.color_rgb, 1.0);
    sources.set_constant_rows(CODE_FOG_SUN_COLOR_LINEAR, &[float4_bits(sun_linear)]);
    sources.set_constant_rows(CODE_FOG_SUN_COLOR_GAMMA, &[float4_bits(sun_gamma)]);

    let length = math_iw4::vec3_length(sun.sun_dir);
    let dir = if length > 0.0 {
        sun.sun_dir.map(|v| v / length)
    } else {
        sun.sun_dir
    };
    sources.set_constant_rows(
        CODE_FOG_SUN_DIR,
        &[float4_bits([dir[0], dir[1], dir[2], 0.0])],
    );
}

pub fn produce_update_viewport(
    sources: &mut RuntimeCodeSources,
    rt_width: i32,
    rt_height: i32,
    viewport: GfxViewport,
) -> Result<(), CommandContextRefusal> {
    if rt_width <= 0 || rt_height <= 0 {
        return Err(CommandContextRefusal::ViewportHasZeroRenderTarget);
    }
    let inv_w = VIEWPORT_ONE / rt_width as f32;
    let inv_h = VIEWPORT_ONE / rt_height as f32;
    let scale_x = inv_w * viewport.width as f32 * VIEWPORT_HALF;
    let scale_y = inv_h * viewport.height as f32 * VIEWPORT_HALF;
    sources.set_constant_rows(
        CODE_RENDER_TARGET_SIZE,
        &[float4_bits([
            rt_width as f32,
            rt_height as f32,
            inv_w,
            inv_h,
        ])],
    );
    sources.set_constant_rows(
        CODE_CLIP_SPACE_LOOKUP_SCALE,
        &[float4_bits([scale_x, -scale_y, 0.0, 1.0])],
    );
    sources.set_constant_rows(
        CODE_CLIP_SPACE_LOOKUP_OFFSET,
        &[float4_bits([
            VIEWPORT_HALF * inv_w + viewport.x as f32 * inv_w + scale_x,
            VIEWPORT_HALF * inv_h + scale_y + viewport.y as f32 * inv_h,
            0.0,
            0.0,
        ])],
    );
    Ok(())
}

pub fn produce_lighting_lookup_scale(sources: &mut RuntimeCodeSources, inv_image_height: f32) {
    let scale = lighting_iw4::model_lighting_lookup_scale(inv_image_height);
    sources.set_constant_rows(CODE_LIGHTING_LOOKUP_SCALE, &[float4_bits(scale.as_array())]);
}

pub fn produce_model_lighting_code_texture(
    sources: &mut RuntimeCodeSources,
) -> Result<(), super::material_runtime::CodeSourceError> {
    sources.set_texture(
        CODE_TEXTURE_MODEL_LIGHTING,
        CODE_TEXTURE_MODEL_LIGHTING_SAMPLER,
    )
}

pub fn produce_floatz_code_texture(
    sources: &mut RuntimeCodeSources,
) -> Result<(), super::material_runtime::CodeSourceError> {
    sources.set_texture(CODE_TEXTURE_FLOATZ, CODE_TEXTURE_FLOATZ_SAMPLER)
}

pub fn produce_sun_shadow_code_texture(
    sources: &mut RuntimeCodeSources,
) -> Result<(), super::material_runtime::CodeSourceError> {
    sources.set_texture(
        super::CODE_TEXTURE_SHADOWMAP_SUN,
        super::SunShadowSamplingPath::Fallback.code_image_sampler(),
    )
}

pub fn produce_spot_shadow_code_texture(
    sources: &mut RuntimeCodeSources,
) -> Result<(), super::material_runtime::CodeSourceError> {
    sources.set_texture(
        super::CODE_TEXTURE_SHADOWMAP_SPOT,
        super::SunShadowSamplingPath::Fallback.code_image_sampler(),
    )
}

pub fn produce_sun_shadow_receiver_constants(
    sources: &mut RuntimeCodeSources,
    frame: super::SunShadowForcedFrame,
) {
    sources.set_constant_rows(
        super::CODE_SHADOWMAP_SWITCH_PARTITION,
        &[float4_bits(frame.receiver.switch_partition)],
    );
    sources.set_constant_rows(
        super::CODE_SHADOWMAP_SCALE,
        &[float4_bits(frame.receiver.shadowmap_scale)],
    );
    sources.set_constant_rows(
        super::CODE_SUN_SHADOWMAP_PIXEL_ADJUST,
        &[float4_bits(frame.pixel_adjust)],
    );
    sources.set_constant_rows(
        super::CODE_SHADOWMAP_LOOKUP_TRANSPOSE,
        &code_transpose_matrix_row4(frame.lookup.transpose()),
    );
    sources.set_constant_rows(
        super::CODE_SHADOWMAP_POLYGON_OFFSET,
        &[float4_bits(frame.partitions[0].polygon_offset)],
    );
}

pub fn produce_world_view_family(sources: &mut RuntimeCodeSources, world_view: Mat4) {
    let inverse = world_view.inverse();
    sources.set_constant_rows(
        CODE_INVERSE_WORLD_VIEW0,
        &code_transpose_matrix_row4(inverse.transpose()),
    );
    sources.set_constant_rows(
        CODE_TRANSPOSE_WORLD_VIEW0,
        &code_transpose_matrix_row4(world_view),
    );
    sources.set_constant_rows(
        CODE_INVERSE_TRANSPOSE_WORLD_VIEW0,
        &code_transpose_matrix_row4(inverse),
    );
}

pub fn produce_material_color_cmdbuf_init(sources: &mut RuntimeCodeSources) {
    sources.set_constant_rows(
        CODE_MATERIAL_COLOR,
        &[float4_bits([f32::MAX, f32::MAX, f32::MAX, 0.0])],
    );
}

const NO_FRAME_FOG: asset_world::ExpFog = asset_world::ExpFog {
    start_dist: 0.0,
    halfway_dist: 0.0,
    color_rgb: [0.0; 3],
    max_opacity: 0.0,
    transition_time: 0.0,
    sun: Some(asset_world::SunFog {
        color_rgb: [0.0; 3],
        sun_dir: [1.0, 0.0, 0.0],
        begin_angle_deg: 0.0,
        end_angle_deg: 0.0,
        scale: 0.0,
    }),
    volumetric: None,
};

pub fn produce_top_pair_command_context(
    sources: &mut RuntimeCodeSources,
    clip_from_world: Mat4,
    view_from_world: Mat4,
    view_origin: Vec3,
    world0: Vec<[u32; 4]>,
    fog: Option<&asset_world::ExpFog>,
    fog_enabled: bool,
) -> Result<(), CommandContextRefusal> {
    sources.set_constant_rows(
        CODE_TRANSPOSE_VIEW_PROJECTION,
        &code_transpose_matrix_row4(camera_relative_view_projection(
            clip_from_world,
            view_origin,
        )),
    );
    sources.set_constant(CODE_TRANSPOSE_WORLD0, world0);
    sources.set_constant_rows(
        CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0,
        &code_transpose_matrix_row4(clip_from_world),
    );
    produce_world_view_family(sources, view_from_world);
    produce_material_color_cmdbuf_init(sources);
    produce_depth_from_clip(sources, false);
    let (frame_fog, fog_enabled) = match fog {
        Some(fog) => (fog, fog_enabled),
        None => (&NO_FRAME_FOG, false),
    };
    produce_frame_fog(sources, frame_fog, fog_enabled)
}

pub fn produce_dir_primary_light(
    sources: &mut RuntimeCodeSources,
    light: &MapDirPrimaryLight,
) -> Result<(), CommandContextRefusal> {
    if light.light_type != lighting_iw4::GFX_LIGHT_TYPE_DIR {
        return Err(CommandContextRefusal::PrimaryLightNotDir);
    }
    let position = lighting_iw4::dir_light_position(light.direction);
    let diffuse = light
        .t5_diffuse_color
        .map(|v| [v[0], v[1], v[2], 1.0])
        .unwrap_or_else(|| lighting_iw4::light_diffuse(light.color, light.diffuse_color_scale));
    let specular = light
        .t5_specular_color
        .map(|v| [v[0], v[1], v[2], 1.0])
        .unwrap_or_else(|| lighting_iw4::light_specular(light.color, light.specular_color_scale));
    sources.set_constant_rows(CODE_LIGHT_POSITION, &[float4_bits(position)]);
    sources.set_constant_rows(CODE_LIGHT_DIFFUSE, &[float4_bits(diffuse)]);
    sources.set_constant_rows(CODE_LIGHT_SPECULAR, &[float4_bits(specular)]);
    Ok(())
}

pub fn produce_depth_from_clip(sources: &mut RuntimeCodeSources, viewmodel: bool) {
    let w = if viewmodel { -1.0 } else { 1.0 };
    sources.set_constant_rows(CODE_DEPTH_FROM_CLIP, &[float4_bits([0.0, 0.0, 0.0, w])]);
}

pub fn produce_game_time(sources: &mut RuntimeCodeSources, game_time: f32) {
    let frac = game_time - game_time.floor();
    let angle = (f64::from(frac) * GAMETIME_TWO_PI) as f32;
    let (sin, cos) = angle.sin_cos();
    let wrapped = (f64::from(game_time) % GAMETIME_WRAP) as f32;
    sources.set_constant_rows(CODE_GAMETIME, &[float4_bits([sin, cos, frac, wrapped])]);
}

pub(crate) fn update_command_context_code_sources(
    mut runtime: ResMut<MaterialFrameInputs>,
    generation: Res<MaterialGeneration>,
    fog: Option<Res<MapFrameFog>>,
    fog_dvars: Res<super::fog::FogDvars>,
    mut dfog: ResMut<super::DrawMethodDfog>,
    dir_light: Option<Res<MapDirPrimaryLight>>,
    t5_exposure: Option<Res<MapT5SunParseExposure>>,
    t5_tree_scatter: Option<Res<MapT5TreeScatter>>,
    outdoor: Option<Res<MapOutdoor>>,
    lighting: Option<Res<WorldModelLightingAtlas>>,
    cg_clock: Option<Res<FrameClock>>,
    prepared: Res<PreparedSceneView>,
    cameras: Query<&Camera, With<FpvLens>>,
    scene: Option<Res<crate::prepare::scene::world::WorldScene>>,
) {
    if !prepared.ready {
        return;
    }
    let view = prepared.view_from_world;
    let clip_from_view = prepared.clip_from_view;
    let clip_from_world = prepared.clip_from_world;
    let viewmodel_clip_from_world =
        host_clip_from_view(prepared.fov, prepared.aspect, prepared.depth_hack_near) * view;
    let eye = prepared.eye;
    let mat_frame = &mut *runtime;
    mat_frame.view_origin = eye;
    mat_frame.clip_from_world = Some(clip_from_world);
    mat_frame.view_from_world = Some(view);
    mat_frame.clip_from_view = Some(clip_from_view);
    mat_frame.viewmodel_clip_from_world = Some(viewmodel_clip_from_world);
    mat_frame.viewmodel_near = Some(prepared.depth_hack_near);
    mat_frame.outdoor = outdoor.as_deref().copied();

    mat_frame.code_sources = RuntimeCodeSources::default();
    mat_frame.sun_shadow = None;
    let float_time_ms = cg_clock.map(|c| c.time()).unwrap_or(0);
    mat_frame.float_time = float_time_ms as f32 / 1000.0;
    let float_time = mat_frame.float_time;
    let sampled_fog = fog.as_ref().map(|fog| fog.sample(float_time_ms));

    dfog.0 = sampled_fog.as_ref().is_some_and(|fog| fog.sun.is_some());

    let mut source = GfxCmdBufSource2d::default();
    source.render_target_width = prepared.rt_w;
    source.render_target_height = prepared.rt_h;
    source.scene_viewport = prepared.scene_viewport;
    let full_vp = hud_iw4::GfxViewport {
        x: 0,
        y: 0,
        width: prepared.rt_w,
        height: prepared.rt_h,
    };
    source.viewport_select =
        if prepared.rt_w <= 0 || prepared.rt_h <= 0 || prepared.scene_viewport == full_vp {
            hud_iw4::GFX_VIEWPORT_FULL
        } else {
            0
        };
    let begun = begin_view(
        &mut source,
        &gfx_scene_def_float_time(float_time),
        &prepared.parms,
    );
    let world0 = world_transpose_rows_from_set3d(begun.set_3d.world);
    match produce_top_pair_command_context(
        &mut mat_frame.code_sources,
        clip_from_world,
        view,
        eye,
        world0,
        sampled_fog.as_ref(),
        fog_dvars.enabled,
    ) {
        Ok(()) => {}
        Err(CommandContextRefusal::MissingViewProjection) => {}
        Err(CommandContextRefusal::PrimaryLightNotDir) => {}
        Err(CommandContextRefusal::ViewportHasZeroRenderTarget) => {}
    }
    if let Ok(camera) = cameras.single()
        && let (Some(rt), Some(vp)) = (
            camera.physical_target_size(),
            camera.physical_viewport_rect(),
        )
    {
        let size = vp.size();
        if let (Ok(rt_w), Ok(rt_h), Ok(x), Ok(y), Ok(w), Ok(h)) = (
            i32::try_from(rt.x),
            i32::try_from(rt.y),
            i32::try_from(vp.min.x),
            i32::try_from(vp.min.y),
            i32::try_from(size.x),
            i32::try_from(size.y),
        ) {
            let _ = produce_update_viewport(
                &mut mat_frame.code_sources,
                rt_w,
                rt_h,
                GfxViewport {
                    x,
                    y,
                    width: w,
                    height: h,
                },
            );
        }
    }
    if let Some(light) = dir_light.as_deref() {
        let _ = produce_dir_primary_light(&mut mat_frame.code_sources, light);
    }
    if let Some(scene) = scene.as_deref() {
        let catalog = &generation.catalog;
        let generation = catalog.generation_id();
        if mat_frame
            .material_bindings
            .as_ref()
            .is_none_or(|bindings| bindings.generation_id() != generation)
        {
            mat_frame.material_bindings = Some(asset_material::compile_material_bindings(catalog));
        }
        let fog = sampled_fog.as_ref().unwrap_or(&NO_FRAME_FOG);
        let inputs = asset_material::MaterialFrameBindingInputs {
            eye,
            clip_from_view,
            world_from_view: view.inverse(),
            target_size: [prepared.rt_w, prepared.rt_h],
            time: float_time,
            exposure: t5_exposure.as_deref().map(|e| e.exposure),
            world: scene.material_world,
            tree_scatter: t5_tree_scatter.as_deref().map(|s| [s.intensity, s.amount]),
            fog: crate::prepare::scene::world_bindings::fog(fog),
            fog_enabled: sampled_fog.is_some() && fog_dvars.enabled,
            sun: dir_light
                .as_deref()
                .filter(|l| l.light_type == lighting_iw4::GFX_LIGHT_TYPE_DIR)
                .map(crate::prepare::scene::world_bindings::sun),
        };
        mat_frame
            .material_bindings
            .as_ref()
            .expect("material bindings compiled")
            .bind_frame(catalog, &mut mat_frame.code_sources, &inputs)
            .expect("material binding generation correlated");
    } else {
        mat_frame.material_bindings = None;
    }

    produce_game_time(&mut mat_frame.code_sources, begun.float_time);
    if let Some(lighting) = lighting.as_deref()
        && let Some(inv_h) =
            lighting_iw4::model_lighting_inv_image_height(lighting.dims.image_height)
    {
        produce_lighting_lookup_scale(&mut mat_frame.code_sources, inv_h);
        let _ = produce_model_lighting_code_texture(&mut mat_frame.code_sources);
    }
    let _ = produce_floatz_code_texture(&mut mat_frame.code_sources);
    let _ = produce_spot_shadow_code_texture(&mut mat_frame.code_sources);
    let _ = mat_frame
        .code_sources
        .set_texture(CODE_TEXTURE_RESOLVED_POST_SUN, 0x62);
    if let Some(light) = dir_light.as_deref() {
        let _ = produce_sun_shadow_code_texture(&mut mat_frame.code_sources);

        if let Some(scene) = scene.as_deref()
            && let Some(bounds) = scene.world_bounds
        {
            let world_mid = [bounds[0], bounds[1], bounds[2]];
            let world_half = [bounds[3], bounds[4], bounds[5]];
            let shadow_forward = super::sun_shadow_forward_from_light_dir(light.direction);
            let world = prepared.view_from_world.inverse();
            let forward = world.transform_vector3(Vec3::NEG_Z).normalize_or_zero();
            let right = world.transform_vector3(Vec3::X).normalize_or_zero();
            let up = world.transform_vector3(Vec3::Y).normalize_or_zero();
            let (tx, ty) = super::tan_half_fov_from_clip(clip_from_view);
            let frame = super::forced_fallback_frame(
                shadow_forward,
                super::SunShadowCamera {
                    origin: eye.to_array(),
                    forward: forward.to_array(),
                    right: right.to_array(),
                    up: up.to_array(),
                    tan_half_fov_x: tx,
                    tan_half_fov_y: ty,
                    z_near: prepared.near,
                },
                world_mid,
                world_half,
                scene.sun_sample_size_near,
            );
            produce_sun_shadow_receiver_constants(&mut mat_frame.code_sources, frame);
            mat_frame.sun_shadow = Some(frame);
        }
    }
}
