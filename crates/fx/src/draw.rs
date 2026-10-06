use fx_iw4::{
    FxElemType, FxRandomChannel, FxTrailEmittedVert, FxTrailSegmentDrawState, FxTrailVertex,
    draw_elem_handler_present, elem_norm_time, elem_random_seed, sample_life_span_msec, sample_u16,
    spark_fountain_cluster_draw_allows, spark_fountain_slot_for_handle, trail_compute_u,
    trail_emit_index_quad, trail_emit_segment_verts, trail_uncompress_basis, vec3_length_sq,
    vec3_normalize,
};

use crate::elem::{FX_ELEM_HANDLE_NONE, elem_slot_for_handle};
use crate::system::FxSystemHost;
use crate::trail::{FX_TRAIL_HANDLE_NONE, trail_elem_slot_for_handle, trail_slot_for_handle};

#[derive(Clone, Copy, Debug)]
pub struct FxDrawElemContext<'a> {
    pub def_name: &'a str,
    pub catalog_index: u16,
    pub def_index: u8,
    pub elem_type: u8,
    pub origin: [f32; 3],
    pub axis: [[f32; 3]; 3],
    pub norm_time: f32,
    pub age_msec: i32,
    pub life_msec: i32,

    pub elem_random_seed: u64,

    pub sequence: u8,
    pub base_vel: [f32; 3],

    pub flags: i32,

    pub at_rest_fraction: u8,

    pub packed_lighting: [u8; 3],
    pub packed_lighting_src: crate::FxPackedLightingSrc,

    pub elem_handle: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct FxSparkDrawQuery<'a> {
    pub def_name: &'a str,
    pub catalog_index: u16,
    pub def_index: u8,
    pub norm_time: f32,
    pub age_msec: i32,
    pub life_msec: i32,

    pub elem_random_seed: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct FxDrawTrailContext<'a> {
    pub def_name: &'a str,
    pub catalog_index: u16,
    pub def_index: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct FxDrawTrailSampleContext<'a> {
    pub def_name: &'a str,
    pub catalog_index: u16,
    pub def_index: u8,
    pub sample_sequence: u8,
    pub sample_origin: [f32; 3],
    pub spawn_dist: f32,
    pub msec_begin: i32,
    pub norm_time: f32,
    pub age_msec: i32,
    pub life_msec: i32,

    pub elem_random_seed: u64,
}

#[derive(Clone, Debug)]
pub struct FxTrailDrawDef {
    pub visual_count: u8,
    pub flags: i32,
    pub life_base: i32,
    pub life_amp: i32,
    pub scroll_time_msec: i32,

    pub repeat_dist: i32,
    pub verts: Vec<FxTrailVertex>,

    pub inds: Vec<u16>,

    pub material_name: String,

    pub material_index: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
pub struct FxTrailSampleVisual {
    pub size: [f32; 2],
    pub color_rgba: [u8; 4],
    pub rotation: f32,
}

#[derive(Clone, Debug)]
pub struct FxSpriteInstance {
    pub viewmodel: bool,
    pub origin: [f32; 3],

    pub size0: f32,

    pub size1: f32,
    pub color_rgba: [u8; 4],
    pub elem_type: u8,
    pub def_name: std::sync::Arc<str>,
    pub catalog_index: u16,
    pub def_index: u8,
    pub material_name: std::sync::Arc<str>,

    pub material_index: Option<usize>,

    pub axis: [[f32; 3]; 3],

    pub rotation_rad: f32,

    pub vel_dir: [f32; 3],

    pub atlas: fx_iw4::FxSpriteAtlasUv,

    pub flags: i32,

    pub lighting_sample: [u8; 3],
    pub lighting_frac: u8,
    pub packed_lighting_src: crate::FxPackedLightingSrc,
}

#[derive(Clone, Debug, Default)]
pub struct FxTrailMeshInstance {
    pub def_name: String,
    pub def_index: u8,
    pub material_name: String,

    pub material_index: Option<usize>,
    pub verts: Vec<FxTrailEmittedVert>,

    pub index_pairs: Vec<[u16; 2]>,
}

#[derive(Clone, Debug, Default)]
pub struct FxGenerateVertsOut {
    pub sprites: Vec<FxSpriteInstance>,
    pub trail_meshes: Vec<FxTrailMeshInstance>,
    pub skipped_dormant: u32,
    pub skipped_no_visual: u32,
    pub skipped_null_handler: u32,

    pub skipped_unsupported_type: u32,
    pub skipped_cloud: u32,
    pub skipped_spark_cloud: u32,
    pub skipped_spark_fountain: u32,
    pub skipped_model: u32,
    pub skipped_omni_light: u32,
    pub skipped_spot_light: u32,

    pub skipped_other_type: u32,
    pub skipped_no_lookup: u32,
    pub skipped_stopped: u32,

    pub skipped_no_trail_def: u32,

    pub spark_clouds: Vec<crate::spark::FxSparkCloudInstance>,

    pub clouds: Vec<FxCloudInstance>,

    pub fountains: Vec<FxFountainInstance>,

    pub omni_lights: Vec<FxElemLightInstance>,

    pub spot_lights: Vec<FxElemLightInstance>,

    pub models: Vec<FxModelInstance>,

    pub spark_cloud_history_empty: u32,

    pub spark_cloud_no_size1: u32,
}

#[derive(Clone, Debug)]
pub struct FxCloudInstance {
    pub def_name: String,
    pub catalog_index: u16,
    pub def_index: u8,
    pub cloud: fx_iw4::GfxParticleCloud,
}

#[derive(Clone, Debug)]
pub struct FxFountainInstance {
    pub def_name: String,
    pub catalog_index: u16,
    pub def_index: u8,
    pub cloud: fx_iw4::GfxParticleCloud,
    pub cells: Vec<[fx_iw4::GfxPosTexVertex; 8]>,
}

#[derive(Clone, Debug)]
pub struct FxElemLightInstance {
    pub def_name: String,
    pub def_index: u8,
    pub is_spot: bool,
    pub origin: [f32; 3],
    pub radius: f32,
    pub color_bgr: [f32; 3],

    pub axis: [[f32; 3]; 3],
}

#[derive(Clone, Debug)]
pub struct FxModelInstance {
    pub def_name: String,
    pub def_index: u8,

    pub model_index: usize,
    pub elem_handle: u16,
    pub origin: [f32; 3],
    pub axis: [[f32; 3]; 3],
    pub scale: f32,

    pub flags: i32,
}

const FX_STATUS_DRAW_SKIP: u32 = 0x4000;

const FX_ELEM_FLAG_TRAIL_DIR_BASIS: i32 = 0x4000;

pub fn generate_verts(
    host: &FxSystemHost,
    mut on_elem: impl FnMut(FxDrawElemContext<'_>) -> Option<FxSpriteInstance>,
) -> FxGenerateVertsOut {
    generate_verts_with_trails(
        host,
        &mut on_elem,
        &mut |_, _| None,
        &mut |_| None,
        &mut |_| None,
        &mut |_| None,
        &mut |_| None,
        &mut |_| None,
        [0.0; 3],
    )
}

pub fn generate_verts_with_trails(
    host: &FxSystemHost,
    on_elem: &mut dyn FnMut(FxDrawElemContext<'_>) -> Option<FxSpriteInstance>,
    on_trail_def: &mut dyn FnMut(FxDrawTrailContext<'_>, u8) -> Option<FxTrailDrawDef>,
    on_trail_sample: &mut dyn FnMut(FxDrawTrailSampleContext<'_>) -> Option<FxTrailSampleVisual>,
    on_spark_size1: &mut dyn FnMut(FxSparkDrawQuery<'_>) -> Option<f32>,
    on_cloud: &mut dyn FnMut(FxDrawElemContext<'_>) -> Option<FxCloudInstance>,
    on_light: &mut dyn FnMut(FxDrawElemContext<'_>) -> Option<FxElemLightInstance>,
    on_model: &mut dyn FnMut(FxDrawElemContext<'_>) -> Option<FxModelInstance>,
    camera: [f32; 3],
) -> FxGenerateVertsOut {
    let mut out = FxGenerateVertsOut::default();
    let start = host.first_active_effect as u32;
    let end = host.first_new_effect as u32;
    let mut cursor = start;
    while cursor != end {
        let handle = host.handle_at_ring(cursor);
        if let Some(slot) = host.slot_index_for_handle(handle) {
            draw_effect_sprites(
                host,
                slot,
                on_elem,
                on_spark_size1,
                on_cloud,
                on_light,
                on_model,
                camera,
                &mut out,
            );
            draw_effect_trails(host, slot, on_trail_def, on_trail_sample, &mut out);
        }
        cursor = cursor.wrapping_add(1);
    }
    out
}

fn draw_effect_sprites(
    host: &FxSystemHost,
    effect_slot: usize,
    on_elem: &mut dyn FnMut(FxDrawElemContext<'_>) -> Option<FxSpriteInstance>,
    on_spark_size1: &mut dyn FnMut(FxSparkDrawQuery<'_>) -> Option<f32>,
    on_cloud: &mut dyn FnMut(FxDrawElemContext<'_>) -> Option<FxCloudInstance>,
    on_light: &mut dyn FnMut(FxDrawElemContext<'_>) -> Option<FxElemLightInstance>,
    on_model: &mut dyn FnMut(FxDrawElemContext<'_>) -> Option<FxModelInstance>,
    camera: [f32; 3],
    out: &mut FxGenerateVertsOut,
) {
    let effect = match host.effect_at(effect_slot) {
        Some(e) if e.ring_resident => e,
        _ => return,
    };
    if (effect.status & FX_STATUS_DRAW_SKIP) != 0 {
        out.skipped_stopped = out.skipped_stopped.saturating_add(1);
        return;
    }

    for class in 0..3 {
        let mut handle = effect.first_elem_handle[class];
        while handle != FX_ELEM_HANDLE_NONE {
            let next = elem_slot_for_handle(handle)
                .and_then(|s| host.elems.get(s).map(|e| e.next_elem_handle))
                .unwrap_or(FX_ELEM_HANDLE_NONE);
            draw_one_elem(
                host,
                effect_slot,
                handle,
                on_elem,
                on_spark_size1,
                on_cloud,
                on_light,
                on_model,
                camera,
                out,
            );
            handle = next;
        }
    }
}

fn draw_one_elem(
    host: &FxSystemHost,
    effect_slot: usize,
    handle: u16,
    on_elem: &mut dyn FnMut(FxDrawElemContext<'_>) -> Option<FxSpriteInstance>,
    on_spark_size1: &mut dyn FnMut(FxSparkDrawQuery<'_>) -> Option<f32>,
    on_cloud: &mut dyn FnMut(FxDrawElemContext<'_>) -> Option<FxCloudInstance>,
    on_light: &mut dyn FnMut(FxDrawElemContext<'_>) -> Option<FxElemLightInstance>,
    on_model: &mut dyn FnMut(FxDrawElemContext<'_>) -> Option<FxModelInstance>,
    camera: [f32; 3],
    out: &mut FxGenerateVertsOut,
) {
    let Some(elem_slot) = elem_slot_for_handle(handle) else {
        return;
    };
    let Some(elem) = host.elems.get(elem_slot).filter(|e| e.occupied) else {
        return;
    };
    if elem.visual_count == 0 {
        out.skipped_no_visual = out.skipped_no_visual.saturating_add(1);
        return;
    }
    if host.msec_now < elem.msec_begin {
        out.skipped_dormant = out.skipped_dormant.saturating_add(1);
        return;
    }
    let death = elem.msec_begin.wrapping_add(elem.life_span_msec);
    if host.msec_now >= death {
        out.skipped_dormant = out.skipped_dormant.saturating_add(1);
        return;
    }

    let Some(elem_ty) = FxElemType::from_u8(elem.elem_type) else {
        out.skipped_unsupported_type = out.skipped_unsupported_type.saturating_add(1);
        out.skipped_other_type = out.skipped_other_type.saturating_add(1);
        return;
    };
    if elem_ty.draw_elem_handler_is_null() {
        out.skipped_null_handler = out.skipped_null_handler.saturating_add(1);
        return;
    }
    if !matches!(
        elem_ty,
        FxElemType::Billboard | FxElemType::Oriented | FxElemType::Tail
    ) {
        if elem_ty == FxElemType::Cloud {
            let effect = match host.effect_at(effect_slot) {
                Some(e) => e,
                None => return,
            };
            let age = host.msec_now.wrapping_sub(elem.msec_begin);
            let norm = elem_norm_time(age, elem.life_span_msec);
            let elem_random_seed =
                elem_random_seed(effect.random_seed, elem.sequence, elem.msec_begin);
            let origin = crate::spark::spark_elem_world_origin(
                elem.origin,
                elem.flags,
                &effect.frame_now(),
                &effect.frame_when_played(),
                Some(elem.orient_spawn_params(elem_random_seed)),
            );
            let ctx = FxDrawElemContext {
                def_name: effect.def_name.as_str(),
                catalog_index: effect.catalog_index,
                def_index: elem.def_index,
                elem_type: elem.elem_type,
                origin,
                axis: effect.axis,
                norm_time: norm,
                age_msec: age,
                life_msec: elem.life_span_msec,
                elem_random_seed,
                sequence: elem.sequence,
                base_vel: elem.base_vel,
                flags: elem.flags,
                at_rest_fraction: elem.at_rest_fraction,
                packed_lighting: effect.packed_lighting,
                packed_lighting_src: effect.packed_lighting_src,
                elem_handle: handle,
            };
            match on_cloud(ctx) {
                Some(inst) if inst.cloud.placement_scale != 0.0 && inst.cloud.size0 != 0.0 => {
                    out.clouds.push(inst);
                }
                _ => {
                    out.skipped_cloud = out.skipped_cloud.saturating_add(1);
                }
            }
            return;
        }
        let _ = draw_elem_handler_present(elem.elem_type);
        out.skipped_unsupported_type = out.skipped_unsupported_type.saturating_add(1);
        match elem_ty {
            FxElemType::SparkCloud => {
                let effect = match host.effect_at(effect_slot) {
                    Some(e) => e,
                    None => return,
                };
                let age = host.msec_now.wrapping_sub(elem.msec_begin);
                let norm = elem_norm_time(age, elem.life_span_msec);
                let seed = elem_random_seed(effect.random_seed, elem.sequence, elem.msec_begin);
                let Some(size1) = on_spark_size1(FxSparkDrawQuery {
                    def_name: effect.def_name.as_str(),
                    catalog_index: effect.catalog_index,
                    def_index: elem.def_index,
                    norm_time: norm,
                    age_msec: age,
                    life_msec: elem.life_span_msec,
                    elem_random_seed: seed,
                }) else {
                    out.spark_cloud_no_size1 = out.spark_cloud_no_size1.saturating_add(1);
                    out.skipped_spark_cloud = out.skipped_spark_cloud.saturating_add(1);
                    return;
                };
                match crate::spark::build_spark_cloud_instance(
                    host,
                    elem.spark_cloud_handle,
                    effect.def_name.as_str(),
                    effect.catalog_index,
                    elem.def_index,
                    host.msec_now,
                    size1,
                ) {
                    Some(inst) => out.spark_clouds.push(inst),
                    None => {
                        out.spark_cloud_history_empty =
                            out.spark_cloud_history_empty.saturating_add(1);
                        out.skipped_spark_cloud = out.skipped_spark_cloud.saturating_add(1);
                    }
                }
            }
            FxElemType::SparkFountain => {
                let cluster = spark_fountain_slot_for_handle(elem.spark_cloud_handle)
                    .and_then(|s| host.spark_fountains.get(s));
                let (ready, spark_n) = cluster.map(|c| (c.ready, c.spark_n)).unwrap_or((0, 0));
                if !spark_fountain_cluster_draw_allows(ready, spark_n, i32::from(spark_n)) {
                    out.skipped_spark_fountain = out.skipped_spark_fountain.saturating_add(1);
                    return;
                }
                let effect = match host.effect_at(effect_slot) {
                    Some(e) => e,
                    None => return,
                };
                let age = host.msec_now.wrapping_sub(elem.msec_begin);
                let norm = elem_norm_time(age, elem.life_span_msec);
                let elem_random_seed =
                    elem_random_seed(effect.random_seed, elem.sequence, elem.msec_begin);
                let origin = crate::spark::spark_elem_world_origin(
                    elem.origin,
                    elem.flags,
                    &effect.frame_now(),
                    &effect.frame_when_played(),
                    Some(elem.orient_spawn_params(elem_random_seed)),
                );
                let ctx = FxDrawElemContext {
                    def_name: effect.def_name.as_str(),
                    catalog_index: effect.catalog_index,
                    def_index: elem.def_index,
                    elem_type: elem.elem_type,
                    origin,
                    axis: effect.axis,
                    norm_time: norm,
                    age_msec: age,
                    life_msec: elem.life_span_msec,
                    elem_random_seed,
                    sequence: elem.sequence,
                    base_vel: elem.base_vel,
                    flags: elem.flags,
                    at_rest_fraction: elem.at_rest_fraction,
                    packed_lighting: effect.packed_lighting,
                    packed_lighting_src: effect.packed_lighting_src,
                    elem_handle: handle,
                };
                match on_cloud(ctx) {
                    Some(inst)
                        if inst.cloud.placement_scale != 0.0
                            && inst.cloud.size0 != 0.0
                            && inst.cloud.flags & fx_iw4::FX_PARTICLE_CLOUD_FLAG_SPARK == 0 =>
                    {
                        let cells = crate::spark_fountain::emit_spark_fountain_custom_cells(
                            host,
                            elem.spark_cloud_handle,
                            camera,
                            inst.cloud.size0,
                            age,
                        );
                        if cells.is_empty() {
                            out.skipped_spark_fountain =
                                out.skipped_spark_fountain.saturating_add(1);
                        } else {
                            out.fountains.push(FxFountainInstance {
                                def_name: inst.def_name,
                                catalog_index: inst.catalog_index,
                                def_index: inst.def_index,
                                cloud: inst.cloud,
                                cells,
                            });
                        }
                    }
                    _ => {
                        out.skipped_spark_fountain = out.skipped_spark_fountain.saturating_add(1);
                    }
                }
            }
            FxElemType::Model => {
                let effect = match host.effect_at(effect_slot) {
                    Some(e) => e,
                    None => return,
                };
                let age = host.msec_now.wrapping_sub(elem.msec_begin);
                let norm = elem_norm_time(age, elem.life_span_msec);
                let elem_random_seed =
                    elem_random_seed(effect.random_seed, elem.sequence, elem.msec_begin);
                let origin = crate::spark::spark_elem_world_origin(
                    elem.origin,
                    elem.flags,
                    &effect.frame_now(),
                    &effect.frame_when_played(),
                    Some(elem.orient_spawn_params(elem_random_seed)),
                );
                let ctx = FxDrawElemContext {
                    def_name: effect.def_name.as_str(),
                    catalog_index: effect.catalog_index,
                    def_index: elem.def_index,
                    elem_type: elem.elem_type,
                    origin,
                    axis: effect.axis,
                    norm_time: norm,
                    age_msec: age,
                    life_msec: elem.life_span_msec,
                    elem_random_seed,
                    sequence: elem.sequence,
                    base_vel: elem.base_vel,
                    flags: elem.flags,
                    at_rest_fraction: elem.at_rest_fraction,
                    packed_lighting: effect.packed_lighting,
                    packed_lighting_src: effect.packed_lighting_src,
                    elem_handle: handle,
                };
                match on_model(ctx) {
                    Some(model) if model.scale != 0.0 => out.models.push(model),
                    _ => out.skipped_model = out.skipped_model.saturating_add(1),
                }
            }
            FxElemType::OmniLight | FxElemType::SpotLight => {
                let effect = match host.effect_at(effect_slot) {
                    Some(e) => e,
                    None => return,
                };
                let age = host.msec_now.wrapping_sub(elem.msec_begin);
                let norm = elem_norm_time(age, elem.life_span_msec);
                let elem_random_seed =
                    elem_random_seed(effect.random_seed, elem.sequence, elem.msec_begin);
                let origin = crate::spark::spark_elem_world_origin(
                    elem.origin,
                    elem.flags,
                    &effect.frame_now(),
                    &effect.frame_when_played(),
                    Some(elem.orient_spawn_params(elem_random_seed)),
                );
                let ctx = FxDrawElemContext {
                    def_name: effect.def_name.as_str(),
                    catalog_index: effect.catalog_index,
                    def_index: elem.def_index,
                    elem_type: elem.elem_type,
                    origin,
                    axis: effect.axis,
                    norm_time: norm,
                    age_msec: age,
                    life_msec: elem.life_span_msec,
                    elem_random_seed,
                    sequence: elem.sequence,
                    base_vel: elem.base_vel,
                    flags: elem.flags,
                    at_rest_fraction: elem.at_rest_fraction,
                    packed_lighting: effect.packed_lighting,
                    packed_lighting_src: effect.packed_lighting_src,
                    elem_handle: handle,
                };
                match on_light(ctx) {
                    Some(inst) if inst.is_spot => out.spot_lights.push(inst),
                    Some(inst) => out.omni_lights.push(inst),
                    None if elem_ty == FxElemType::SpotLight => {
                        out.skipped_spot_light = out.skipped_spot_light.saturating_add(1);
                    }
                    None => {
                        out.skipped_omni_light = out.skipped_omni_light.saturating_add(1);
                    }
                }
            }
            _ => {
                out.skipped_other_type = out.skipped_other_type.saturating_add(1);
            }
        }
        return;
    }

    let effect = match host.effect_at(effect_slot) {
        Some(e) => e,
        None => return,
    };
    let age = host.msec_now.wrapping_sub(elem.msec_begin);
    let norm = elem_norm_time(age, elem.life_span_msec);
    let elem_random_seed = elem_random_seed(effect.random_seed, elem.sequence, elem.msec_begin);
    let origin = crate::spark::spark_elem_world_origin(
        elem.origin,
        elem.flags,
        &effect.frame_now(),
        &effect.frame_when_played(),
        Some(elem.orient_spawn_params(elem_random_seed)),
    );
    let ctx = FxDrawElemContext {
        def_name: effect.def_name.as_str(),
        catalog_index: effect.catalog_index,
        def_index: elem.def_index,
        elem_type: elem.elem_type,
        origin,
        axis: effect.axis,
        norm_time: norm,
        age_msec: age,
        life_msec: elem.life_span_msec,
        elem_random_seed,
        sequence: elem.sequence,
        base_vel: elem.base_vel,
        flags: elem.flags,
        at_rest_fraction: elem.at_rest_fraction,
        packed_lighting: effect.packed_lighting,
        packed_lighting_src: effect.packed_lighting_src,
        elem_handle: handle,
    };
    match on_elem(ctx) {
        Some(mut sprite) => {
            let dobj = fx_iw4::bolt_dobj(effect.bolt_packed);
            sprite.viewmodel = (fx_iw4::FX_BOLT_VIEWMODEL_DOBJ_BASE
                ..fx_iw4::FX_BOLT_VIEWMODEL_DOBJ_BASE + 2)
                .contains(&dobj);
            out.sprites.push(sprite);
        }
        None => out.skipped_no_lookup = out.skipped_no_lookup.saturating_add(1),
    }
}

fn draw_effect_trails(
    host: &FxSystemHost,
    effect_slot: usize,
    on_trail_def: &mut dyn FnMut(FxDrawTrailContext<'_>, u8) -> Option<FxTrailDrawDef>,
    on_trail_sample: &mut dyn FnMut(FxDrawTrailSampleContext<'_>) -> Option<FxTrailSampleVisual>,
    out: &mut FxGenerateVertsOut,
) {
    let effect = match host.effect_at(effect_slot) {
        Some(e) if e.ring_resident => e,
        _ => return,
    };
    if (effect.status & FX_STATUS_DRAW_SKIP) != 0 {
        return;
    }
    let mut handle = effect.first_trail_handle;
    if handle == FX_TRAIL_HANDLE_NONE {
        return;
    }
    let def_name = effect.def_name.clone();
    let catalog_index = effect.catalog_index;
    let effect_seed = effect.random_seed;
    while handle != FX_TRAIL_HANDLE_NONE {
        let Some(trail_slot) = trail_slot_for_handle(handle) else {
            break;
        };
        let Some(trail) = host.trails.get(trail_slot).filter(|t| t.occupied) else {
            break;
        };
        let next = trail.next_trail_handle;
        let def_index = trail.def_index as u8;
        let first_elem = trail.first_elem_handle;
        let last_elem = trail.last_elem_handle;
        let ctx = FxDrawTrailContext {
            def_name: def_name.as_str(),
            catalog_index,
            def_index,
        };
        let Some(trail_def) = on_trail_def(ctx, def_index) else {
            out.skipped_no_trail_def = out.skipped_no_trail_def.saturating_add(1);
            handle = next;
            continue;
        };
        generate_trail_verts(
            host,
            &def_name,
            catalog_index,
            def_index,
            effect_seed,
            first_elem,
            last_elem,
            &trail_def,
            on_trail_sample,
            out,
        );
        handle = next;
    }
}

fn generate_trail_verts(
    host: &FxSystemHost,
    def_name: &str,
    catalog_index: u16,
    def_index: u8,
    effect_seed: u64,
    first_elem: u16,
    last_elem: u16,
    trail_def: &FxTrailDrawDef,
    on_trail_sample: &mut dyn FnMut(FxDrawTrailSampleContext<'_>) -> Option<FxTrailSampleVisual>,
    out: &mut FxGenerateVertsOut,
) {
    if trail_def.visual_count == 0 {
        out.skipped_no_visual = out.skipped_no_visual.saturating_add(1);
        return;
    }
    if first_elem == FX_TRAIL_HANDLE_NONE {
        return;
    }
    if trail_def.repeat_dist == 0
        || trail_def.verts.is_empty()
        || trail_def.inds.len() < 2
        || trail_def.inds.len() % 2 != 0
    {
        out.skipped_no_trail_def = out.skipped_no_trail_def.saturating_add(1);
        return;
    }

    let msec_draw = host.msec_now;
    let vert_count = trail_def.verts.len() as u16;

    let mut cursor = first_elem;
    let mut first_live = FX_TRAIL_HANDLE_NONE;
    while cursor != FX_TRAIL_HANDLE_NONE {
        let Some(slot) = trail_elem_slot_for_handle(cursor) else {
            break;
        };
        let Some(elem) = host.trail_elems.get(slot).filter(|e| e.occupied) else {
            break;
        };
        if elem.msec_begin <= msec_draw {
            first_live = cursor;
            break;
        }
        cursor = elem.next_trail_elem_handle;
    }
    if first_live == FX_TRAIL_HANDLE_NONE {
        return;
    }

    let first_spawn_dist = trail_elem_slot_for_handle(first_live)
        .and_then(|s| host.trail_elems.get(s))
        .map(|e| e.spawn_dist)
        .unwrap_or(0.0);
    let u_offset = trail_compute_u(
        first_spawn_dist,
        trail_def.repeat_dist,
        trail_def.scroll_time_msec,
        msec_draw,
    ) as f32;

    let mut mesh = FxTrailMeshInstance {
        def_name: def_name.to_owned(),
        def_index,
        material_name: trail_def.material_name.clone(),
        material_index: trail_def.material_index,
        verts: Vec::new(),
        index_pairs: Vec::new(),
    };

    let mut last_state: Option<FxTrailSegmentDrawState> = None;
    let mut last_norm = 1.0_f32;
    let mut segment_count: u16 = 0;
    let mut sample_handle = first_live;
    let mut lookup_miss = false;

    while sample_handle != FX_TRAIL_HANDLE_NONE {
        let Some(slot) = trail_elem_slot_for_handle(sample_handle) else {
            break;
        };
        let Some(elem) = host.trail_elems.get(slot).filter(|e| e.occupied).cloned() else {
            break;
        };
        let next = elem.next_trail_elem_handle;
        if elem.msec_begin > msec_draw {
            sample_handle = next;
            continue;
        }

        let seed = fx_iw4::trail_random_seed(effect_seed, elem.sequence as i8);
        let life_rand = sample_u16(seed, FxRandomChannel::Life);
        let life_msec = sample_life_span_msec(trail_def.life_base, trail_def.life_amp, life_rand);
        let age = msec_draw.wrapping_sub(elem.msec_begin);
        let norm_time = elem_norm_time(age, life_msec);

        let sample_ctx = FxDrawTrailSampleContext {
            def_name,
            catalog_index,
            def_index,
            sample_sequence: elem.sequence,
            sample_origin: elem.origin,
            spawn_dist: elem.spawn_dist,
            msec_begin: elem.msec_begin,
            norm_time,
            age_msec: age,
            life_msec,
            elem_random_seed: seed,
        };
        let Some(visual) = on_trail_sample(sample_ctx) else {
            lookup_miss = true;
            sample_handle = next;
            continue;
        };

        let mut basis = trail_uncompress_basis(&elem.basis);

        if (trail_def.flags & FX_ELEM_FLAG_TRAIL_DIR_BASIS) != 0 && sample_handle != first_elem {
            if let Some(prev) = last_state {
                let delta = [
                    elem.origin[0] - prev.pos_world[0],
                    elem.origin[1] - prev.pos_world[1],
                    elem.origin[2] - prev.pos_world[2],
                ];
                let delta = vec3_normalize(delta);
                let mut side = [delta[1], -delta[0], 0.0];
                if vec3_length_sq(side) > 0.0 {
                    side = vec3_normalize(side);
                    let up = [
                        delta[1] * side[2] - delta[2] * side[1],
                        delta[2] * side[0] - delta[0] * side[2],
                        delta[0] * side[1] - delta[1] * side[0],
                    ];
                    let up = vec3_normalize(up);
                    basis = [side, up];
                }
            }
        }

        let u_coord = elem.spawn_dist / (trail_def.repeat_dist as f32) + u_offset;
        let mut state = FxTrailSegmentDrawState {
            pos_world: elem.origin,
            basis,
            rotation: visual.rotation,
            size: visual.size,
            u_coord,
            color_rgba: visual.color_rgba,
        };

        if norm_time < 1.0 {
            if sample_handle == first_elem {
                if elem.sequence == 0 {
                    state.color_rgba[3] = 0;
                }
            } else if let Some(prev) = last_state {
                if last_norm >= 1.0 {
                    let alpha = (1.0 - last_norm) / (norm_time - last_norm);
                    let lerped = lerp_segment_state(&prev, &state, alpha);
                    emit_segment_into(&trail_def.verts, &lerped, &mut mesh.verts);
                    segment_count = segment_count.saturating_add(1);
                }
                if sample_handle == last_elem {
                    state.color_rgba[3] = 0;
                }
            }
            emit_segment_into(&trail_def.verts, &state, &mut mesh.verts);
            segment_count = segment_count.saturating_add(1);
        }

        last_norm = norm_time;
        last_state = Some(state);
        sample_handle = next;
    }

    if lookup_miss {
        out.skipped_no_lookup = out.skipped_no_lookup.saturating_add(1);
    }

    if segment_count > 1 {
        let mut base: u16 = 0;
        for _ in 0..(segment_count - 1) {
            for pair in trail_def.inds.chunks_exact(2) {
                let quads = trail_emit_index_quad(pair[0], pair[1], base, vert_count);
                mesh.index_pairs.extend_from_slice(&quads);
            }
            base = base.wrapping_add(vert_count);
        }
    }

    if !mesh.verts.is_empty() {
        out.trail_meshes.push(mesh);
    }
}

fn lerp_segment_state(
    a: &FxTrailSegmentDrawState,
    b: &FxTrailSegmentDrawState,
    t: f32,
) -> FxTrailSegmentDrawState {
    let lerp3 = |x: [f32; 3], y: [f32; 3]| {
        [
            x[0] + t * (y[0] - x[0]),
            x[1] + t * (y[1] - x[1]),
            x[2] + t * (y[2] - x[2]),
        ]
    };
    FxTrailSegmentDrawState {
        pos_world: lerp3(a.pos_world, b.pos_world),
        basis: [lerp3(a.basis[0], b.basis[0]), lerp3(a.basis[1], b.basis[1])],
        rotation: a.rotation + t * (b.rotation - a.rotation),
        size: [
            a.size[0] + t * (b.size[0] - a.size[0]),
            a.size[1] + t * (b.size[1] - a.size[1]),
        ],
        u_coord: a.u_coord + t * (b.u_coord - a.u_coord),
        color_rgba: a.color_rgba,
    }
}

fn emit_segment_into(
    verts: &[FxTrailVertex],
    state: &FxTrailSegmentDrawState,
    out: &mut Vec<FxTrailEmittedVert>,
) {
    let start = out.len();
    out.resize(
        start + verts.len(),
        FxTrailEmittedVert {
            xyz: [0.0; 3],
            color_rgba: [0; 4],
            u: 0.0,
            v: 0.0,
            texcoord_packed: 0,
            normal_packed: 0,
            tangent_packed: 0.0,
        },
    );
    trail_emit_segment_verts(verts, state, &mut out[start..]);
}
