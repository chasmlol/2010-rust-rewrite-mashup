use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::Arc;

use asset_audio::CreateFxOneshot;
use asset_game::{FxDefinitions, FxName, OwnedFxEffectDef};
use bevy::tasks::ComputeTaskPool;
use fx::{
    FX_CATALOG_INDEX_NONE, FxBoltTarget, FxChildKind, FxCloudInstance, FxDrawElemContext,
    FxDrawTrailContext, FxDrawTrailSampleContext, FxEffectDefInfo, FxElemDefInfo,
    FxElemLightInstance, FxElemMotionResult, FxElemTraceHit, FxGapCause, FxGenerateVertsOut,
    FxModelInstance, FxPackedLightingSrc, FxPlayPose, FxPlayRequest, FxSparkDrawQuery,
    FxSparkFillQuery, FxSparkFillVisual, FxSpriteInstance, FxSystemHost, FxTrailCollideHit,
    FxTrailDrawDef, FxTrailSampleVisual, PendingCollide, PlayResult, axis_from_hit_normal,
    evaluate_elem_collide_motion, evaluate_elem_motion, generate_verts_with_trails, play_bolted,
    play_oriented, spawn_impact_or_death_effect, spawn_looping_partial, spawn_oriented,
    update as fx_update,
};
use fx_iw4::{
    FX_ELEM_ATLAS_OFF, FX_ELEM_ATLAS_SIZE, FxElemType, FxRandomChannel, apply_lighting_frac_bgra,
    build_cloud, clamp_elem_rotation_time, cull_cloud, cull_elem_light,
    effect_def_needs_lighting_sample, elem_light_color_bgr, elem_uses_lighting_frac,
    evaluate_rotation_total, evaluate_scale, evaluate_size0, evaluate_size1, get_elem_angles_axis,
    get_velocity_at_time, sample_f32, sprite_atlas_uv, vec3_normalize, vis_blocker_add_prepared,
};
use math_iw4::angle_vectors;
use sim::SimWorld;

pub trait FxScene {
    fn sample_atpoint_rgb(&self, origin: [f32; 3]) -> Option<[u8; 3]>;
    fn finish_impact_marks(
        &self,
        host: &mut FxSystemHost,
        def_index: u8,
        against_world: bool,
        against_models: bool,
    );
    fn box_surfaces_prelude_ran(&self, host: &FxSystemHost) -> bool;
}

#[derive(Clone, Copy)]
pub struct FxDrawCull<'a> {
    pub elem_draw: bool,
    pub planes: &'a [[f32; 4]],
}

#[derive(Debug, Default, Clone)]
pub struct FxCreateFxBoot {
    pub named: u32,
    pub held: u32,
    pub miss_def: u32,

    pub failed: Vec<String>,
}

#[derive(Debug, Default, Clone)]
pub struct FxVertsGaps {
    pub lighting_frac: u32,

    pub lighting_frac_def_indices: Vec<u8>,

    pub lighting_applied: u32,

    pub lighting_white: u32,

    pub lighting_atpoint: u32,

    pub lighting_miss: u32,
    pub no_def: u32,
    pub no_elem: u32,

    pub no_material_visual: u32,
    pub no_size0_sample: u32,
    pub size0_not_positive: u32,
    pub no_size1_sample: u32,
    pub size1_not_positive: u32,

    pub color_fallback: u32,

    pub catalog_index_n: u32,

    pub catalog_name_n: u32,
}

#[derive(Clone, Debug, Default)]
pub struct FxElemInfoCache {
    by_index: Vec<Arc<[FxElemDefInfo]>>,
    synced_len: usize,
    synced_first: Option<String>,
}

impl FxElemInfoCache {
    pub fn sync(&mut self, catalog: &FxDefinitions) {
        let first = catalog.names().next().map(str::to_owned);
        if self.synced_len == catalog.len() && self.synced_first == first {
            return;
        }
        self.by_index = catalog
            .effects()
            .map(|effect| Arc::<[FxElemDefInfo]>::from(owned_elem_infos(effect)))
            .collect();
        self.synced_len = catalog.len();
        self.synced_first = first;
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn arc_for(
        &self,
        catalog: &FxDefinitions,
        effect: &OwnedFxEffectDef,
    ) -> Arc<[FxElemDefInfo]> {
        catalog
            .index_of(effect)
            .and_then(|index| self.by_index.get(index).cloned())
            .unwrap_or_else(|| Arc::from(owned_elem_infos(effect)))
    }
}

fn def_info<'a>(effect: &OwnedFxEffectDef, elems: &'a [FxElemDefInfo]) -> FxEffectDefInfo<'a> {
    FxEffectDefInfo {
        msec_looping_life: effect.view.msec_looping_life,
        looping_count: effect.view.looping_count,
        one_shot_count: effect.view.one_shot_count,
        emission_count: effect.view.emission_count,
        elems,
    }
}

fn stamp_packed_lighting(
    host: &mut FxSystemHost,
    handle: u16,
    catalog: &FxDefinitions,
    world: Option<&dyn FxScene>,
) {
    let Some(effect) = host.slot_for_handle(handle) else {
        return;
    };
    let index = effect.catalog_index;
    let origin = effect.origin;
    let needs = catalog_lookup(catalog, index)
        .is_some_and(|d| effect_def_needs_lighting_sample(d.view.flags));
    if !needs {
        return;
    }
    let rgb = world.and_then(|world| world.sample_atpoint_rgb(origin));
    let Some(slot) = host.slot_for_handle_mut(handle) else {
        return;
    };
    match rgb {
        Some(rgb) => {
            slot.packed_lighting = rgb;
            slot.packed_lighting_src = FxPackedLightingSrc::AtPointRgb;
        }
        None => {
            slot.packed_lighting_src = FxPackedLightingSrc::Missing;
        }
    }
}

fn stamp_play_lighting(
    host: &mut FxSystemHost,
    catalog: &FxDefinitions,
    world: Option<&dyn FxScene>,
    result: PlayResult,
) -> PlayResult {
    if let Some(handle) = result.handle() {
        stamp_packed_lighting(host, handle, catalog, world);
    }
    result
}

pub fn restamp_missing_packed_lighting(host: &mut FxSystemHost, world: Option<&dyn FxScene>) {
    let Some(world) = world else {
        return;
    };
    let pending: Vec<(u16, [f32; 3])> = host
        .live_effects()
        .filter(|e| e.packed_lighting_src == FxPackedLightingSrc::Missing)
        .map(|e| (e.own_handle, e.origin))
        .collect();
    for (handle, origin) in pending {
        let Some(rgb) = world.sample_atpoint_rgb(origin) else {
            continue;
        };
        if let Some(slot) = host.slot_for_handle_mut(handle) {
            slot.packed_lighting = rgb;
            slot.packed_lighting_src = FxPackedLightingSrc::AtPointRgb;
        }
    }
}

fn evaluate_color_rgba(samples: &[u8], intervals: u8, time: f32, random: f32) -> Option<[u8; 4]> {
    fx_iw4::evaluate_color_bgra(samples, intervals, time, random).map(|[b, g, r, a]| [r, g, b, a])
}

fn apply_elem_lighting(
    color: [u8; 4],
    lighting_frac: u8,
    packed: [u8; 3],
    src: FxPackedLightingSrc,
    def_index: u8,
    gaps: &mut FxVertsGaps,
) -> [u8; 4] {
    if !elem_uses_lighting_frac(lighting_frac) {
        return color;
    }
    gaps.lighting_frac = gaps.lighting_frac.saturating_add(1);
    if src == FxPackedLightingSrc::Missing {
        gaps.lighting_miss = gaps.lighting_miss.saturating_add(1);
        gaps.lighting_frac_def_indices.push(def_index);
        return color;
    }
    gaps.lighting_applied = gaps.lighting_applied.saturating_add(1);
    match src {
        FxPackedLightingSrc::White => {
            gaps.lighting_white = gaps.lighting_white.saturating_add(1);
        }
        FxPackedLightingSrc::AtPointRgb => {
            gaps.lighting_atpoint = gaps.lighting_atpoint.saturating_add(1);
        }
        FxPackedLightingSrc::Missing => {}
    }
    let [r, g, b, a] = color;
    let [b, g, r, a] = apply_lighting_frac_bgra([b, g, r, a], packed, lighting_frac);
    [r, g, b, a]
}

pub fn boot_createfx_effects(
    host: &mut FxSystemHost,
    catalog: &FxDefinitions,
    cache: &FxElemInfoCache,
    emitters: &[CreateFxOneshot],
    msec_now: i32,
    world: Option<&dyn FxScene>,
) -> FxCreateFxBoot {
    host.set_msec_now(msec_now);
    let mut census = FxCreateFxBoot::default();
    for shot in emitters {
        let Some(effect) = catalog.resolve_createfx_id(&shot.fxid) else {
            census.miss_def = census.miss_def.saturating_add(1);
            continue;
        };
        census.named = census.named.saturating_add(1);
        let elem_infos = cache.arc_for(catalog, effect);
        let result = spawn_oriented(
            host,
            FxPlayRequest {
                def_name: effect.name.as_str(),
                pose: FxPlayPose {
                    origin: shot.origin_inches,
                    axis: axis_from_angles_deg(shot.angles_deg),

                    msec: createfx_spawn_msec(msec_now, shot.delay),
                },
                wants_spotlight: false,
                catalog_index: catalog_index_of(catalog, effect),
                def: Some(def_info(effect, &elem_infos)),
            },
        );
        let result = stamp_play_lighting(host, catalog, world, result);
        if matches!(result, PlayResult::Held { .. }) {
            census.held = census.held.saturating_add(1);
        } else if let PlayResult::Failed(e) = result {
            census.failed.push(format!("{}: {e:?}", effect.name));
        }
    }
    drain_spawn_side_effects(host, catalog, cache, world);
    restamp_missing_packed_lighting(host, world);
    census
}

fn createfx_spawn_msec(msec_now: i32, delay_seconds: f32) -> i32 {
    msec_now.wrapping_add((delay_seconds * 1000.0) as i32)
}

pub fn play_named_oriented_in_world(
    host: &mut FxSystemHost,
    catalog: &FxDefinitions,
    cache: &FxElemInfoCache,
    name: FxName<'_>,
    origin: [f32; 3],
    axis: [[f32; 3]; 3],
    world: Option<&dyn FxScene>,
) -> Option<PlayResult> {
    let msec = host.msec_now;
    let result = play_named_at(host, catalog, cache, name, origin, axis, msec, world)?;
    drain_spawn_side_effects(host, catalog, cache, world);
    Some(result)
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_named_oriented_in_world(
    host: &mut FxSystemHost,
    catalog: &FxDefinitions,
    cache: &FxElemInfoCache,
    name: FxName<'_>,
    origin: [f32; 3],
    axis: [[f32; 3]; 3],
    msec: i32,
    world: Option<&dyn FxScene>,
) -> Option<PlayResult> {
    let effect = name.resolve(catalog)?;
    let elem_infos = cache.arc_for(catalog, effect);
    let result = spawn_oriented(
        host,
        FxPlayRequest {
            def_name: effect.name.as_str(),
            pose: FxPlayPose { origin, axis, msec },
            wants_spotlight: false,
            catalog_index: catalog_index_of(catalog, effect),
            def: Some(def_info(effect, &elem_infos)),
        },
    );
    let result = stamp_play_lighting(host, catalog, world, result);
    drain_spawn_side_effects(host, catalog, cache, world);
    Some(result)
}

#[allow(clippy::too_many_arguments)]
pub fn play_named_oriented_at_msec(
    host: &mut FxSystemHost,
    catalog: &FxDefinitions,
    cache: &FxElemInfoCache,
    name: FxName<'_>,
    origin: [f32; 3],
    axis: [[f32; 3]; 3],
    msec: i32,
    world: Option<&dyn FxScene>,
) -> Option<PlayResult> {
    let result = play_named_at(host, catalog, cache, name, origin, axis, msec, world)?;
    drain_spawn_side_effects(host, catalog, cache, world);
    Some(result)
}

pub fn play_named_bolted_in_world(
    host: &mut FxSystemHost,
    catalog: &FxDefinitions,
    cache: &FxElemInfoCache,
    name: FxName<'_>,
    target: FxBoltTarget,
    world: Option<&dyn FxScene>,
) -> Option<PlayResult> {
    let effect = name.resolve(catalog)?;
    let elem_infos = cache.arc_for(catalog, effect);
    let result = play_bolted(
        host,
        FxPlayRequest {
            def_name: effect.name.as_str(),
            pose: FxPlayPose {
                origin: target.orientation.origin,
                axis: target.orientation.axis,
                msec: host.msec_now,
            },
            wants_spotlight: false,
            catalog_index: catalog_index_of(catalog, effect),
            def: Some(def_info(effect, &elem_infos)),
        },
        target,
    );
    let result = stamp_play_lighting(host, catalog, world, result);
    drain_spawn_side_effects(host, catalog, cache, world);
    Some(result)
}

fn play_named_at(
    host: &mut FxSystemHost,
    catalog: &FxDefinitions,
    cache: &FxElemInfoCache,
    name: FxName<'_>,
    origin: [f32; 3],
    axis: [[f32; 3]; 3],
    msec: i32,
    world: Option<&dyn FxScene>,
) -> Option<PlayResult> {
    let effect = name.resolve(catalog)?;

    let elem_infos = cache.arc_for(catalog, effect);
    let result = play_oriented(
        host,
        FxPlayRequest {
            def_name: effect.name.as_str(),
            pose: FxPlayPose { origin, axis, msec },
            wants_spotlight: false,
            catalog_index: catalog_index_of(catalog, effect),
            def: Some(def_info(effect, &elem_infos)),
        },
    );
    Some(stamp_play_lighting(host, catalog, world, result))
}

pub fn drain_spawn_runners(
    host: &mut FxSystemHost,
    catalog: &FxDefinitions,
    cache: &FxElemInfoCache,
    world: Option<&dyn FxScene>,
) {
    const MAX_DRAIN: usize = 256;
    let mut steps = 0;
    while !host.pending_runners.is_empty() && steps < MAX_DRAIN {
        steps += 1;
        let batch = std::mem::take(&mut host.pending_runners);
        for req in batch {
            let Some(edge) = catalog_lookup(catalog, req.catalog_index)
                .and_then(|parent| parent.elems.get(req.def_index as usize))
                .and_then(|elem| elem.runner_child_edge(req.random_seed))
            else {
                skip_runner(host, &req);
                continue;
            };

            if edge.is_absent() {
                continue;
            }

            let Some(index) = edge.bound_index() else {
                skip_runner(host, &req);
                continue;
            };
            let Some(child) = catalog.def_at(index).filter(|d| !d.elems.is_empty()) else {
                skip_runner(host, &req);
                continue;
            };
            let child_name = child.name.clone();
            let previous_mark_entity = host.spawn_mark_entity;
            host.spawn_mark_entity = req.mark_entity;
            let played = play_def_at(
                host,
                catalog,
                cache,
                child,
                req.origin,
                req.axis,
                req.msec_begin,
                world,
            )
            .is_some();
            host.spawn_mark_entity = previous_mark_entity;
            if !played {
                skip_runner(host, &req);
                continue;
            }
            host.last_runner_child = Some(child_name);
            host.last_runner_msec = Some(req.msec_begin);
            host.last_runner_rot_deg = req.rot_deg;
        }
    }
}

fn play_def_at(
    host: &mut FxSystemHost,
    catalog: &FxDefinitions,
    cache: &FxElemInfoCache,
    effect: &OwnedFxEffectDef,
    origin: [f32; 3],
    axis: [[f32; 3]; 3],
    msec: i32,
    world: Option<&dyn FxScene>,
) -> Option<PlayResult> {
    if effect.elems.is_empty() {
        return None;
    }
    let elem_infos = cache.arc_for(catalog, effect);
    let result = play_oriented(
        host,
        FxPlayRequest {
            def_name: effect.name.as_str(),
            pose: FxPlayPose { origin, axis, msec },
            wants_spotlight: false,
            catalog_index: catalog_index_of(catalog, effect),
            def: Some(def_info(effect, &elem_infos)),
        },
    );
    Some(stamp_play_lighting(host, catalog, world, result))
}

fn skip_runner(host: &mut FxSystemHost, req: &fx::PendingRunnerSpawn) {
    host.last_runner_parent = Some(req.parent_name.clone()).filter(|name| !name.is_empty());
    host.last_runner_elem = Some(req.def_index);
    host.gaps.raise(FxGapCause::ElemRunnerSpawnSkipped {
        def_index: req.def_index,
    });
}

fn drain_spawn_side_effects(
    host: &mut FxSystemHost,
    catalog: &FxDefinitions,
    cache: &FxElemInfoCache,
    world: Option<&dyn FxScene>,
) {
    drain_spawn_runners(host, catalog, cache, world);
    drain_spawn_decals(host, catalog, world);
}

pub fn drain_spawn_decals(
    host: &mut FxSystemHost,
    catalog: &FxDefinitions,
    world: Option<&dyn FxScene>,
) {
    const MAX_DRAIN: usize = 256;
    let mut steps = 0;
    while !host.pending_decals.is_empty() && steps < MAX_DRAIN {
        steps += 1;
        let batch = std::mem::take(&mut host.pending_decals);
        for req in batch {
            record_spawn_decal(host, catalog, &req, world);
        }
    }
}

fn record_spawn_decal(
    host: &mut FxSystemHost,
    catalog: &FxDefinitions,
    req: &fx::PendingDecalSpawn,
    world: Option<&dyn FxScene>,
) {
    host.last_mark_alloc_slot = None;
    host.last_decal_parent = Some(req.parent_name.clone()).filter(|name| !name.is_empty());
    host.last_decal_elem = Some(req.def_index);
    host.last_decal_msec = Some(req.msec_begin);
    host.last_decal_origin = Some(req.origin);
    host.last_decal_axis = Some(req.axis);
    host.last_decal_bolt = Some(req.bolt);
    host.last_decal_mark_entity = req.mark_entity;
    host.last_decal_vis = None;
    host.last_decal_size0 = None;
    host.last_decal_rotation = None;
    host.last_decal_mat0_edge = None;
    host.last_decal_mat1_edge = None;
    host.last_decal_mat0 = None;
    host.last_decal_mat1 = None;
    host.last_decal_color = None;

    if let Some(elem) = catalog_lookup(catalog, req.catalog_index)
        .and_then(|parent| parent.elems.get(req.def_index as usize))
    {
        host.last_decal_vis =
            Some(fx_iw4::elem_visual_index(elem.view.visual_count, req.random_seed) as u8);
        let rand_size = sample_f32(req.random_seed, FxRandomChannel::Size);
        host.last_decal_size0 = evaluate_size0(
            elem.vis_samples.as_slice(),
            elem.view.vis_state_interval_count,
            0.0,
            rand_size,
        );
        host.last_decal_rotation = evaluate_rotation_total(
            elem.vis_samples.as_slice(),
            elem.view.vis_state_interval_count,
            0.0,
            req.random_seed,
            elem.view.initial_rotation,
            1.0,
        );
        let rand_color = sample_f32(req.random_seed, FxRandomChannel::Color);
        host.last_decal_color = evaluate_color_rgba(
            elem.vis_samples.as_slice(),
            elem.view.vis_state_interval_count,
            0.0,
            rand_color,
        )
        .map(|[r, g, b, a]| u32::from_le_bytes([b, g, r, a]));
        if let Some(visual) = elem.decal_mark_pair(req.random_seed) {
            host.last_decal_mat0_edge = visual.mark_edge_kind(0).map(str::to_owned);
            host.last_decal_mat1_edge = visual.mark_edge_kind(1).map(str::to_owned);
            host.last_decal_mat0 = visual.mark_bound_name(0).map(str::to_owned);
            host.last_decal_mat1 = visual.mark_bound_name(1).map(str::to_owned);
        }
    }

    let result = host.record_impact_mark_from_decal(req.bolt);
    if result.entered && world.is_none() {
        host.note_box_surfaces_skip();
    }
    if result.against_world || result.against_models {
        if let Some(scene) = world {
            scene.finish_impact_marks(
                host,
                req.def_index,
                result.against_world,
                result.against_models,
            );
        } else {
            if result.against_world {
                host.note_world_go_skip(req.def_index);
            }
            if result.against_models {
                host.note_models_go_skip(req.def_index);
            }
        }
    } else if result.entered {
        if let Some(scene) = world {
            if scene.box_surfaces_prelude_ran(host) {
                host.note_box_surfaces_run();
            } else {
                host.note_box_surfaces_skip();
            }
        }
    }
    if !result.entered {
        host.note_impact_mark_outer_skip(req.def_index);
    }
    host.push_mark_trace(&result);
}

pub fn catalog_lookup(catalog: &FxDefinitions, index: u16) -> Option<&OwnedFxEffectDef> {
    (index != FX_CATALOG_INDEX_NONE)
        .then(|| catalog.def_at(index as usize))
        .flatten()
}

fn catalog_lookup_draw<'a>(
    catalog: &'a FxDefinitions,
    index: u16,
    index_n: &Cell<u32>,
    name_n: &Cell<u32>,
) -> Option<&'a OwnedFxEffectDef> {
    if index != FX_CATALOG_INDEX_NONE {
        index_n.set(index_n.get().saturating_add(1));
        catalog.def_at(index as usize)
    } else {
        name_n.set(name_n.get().saturating_add(1));
        None
    }
}

fn catalog_index_of(catalog: &FxDefinitions, effect: &OwnedFxEffectDef) -> u16 {
    catalog
        .index_of(effect)
        .and_then(|i| u16::try_from(i).ok())
        .unwrap_or(FX_CATALOG_INDEX_NONE)
}

struct CollideJobOut {
    handle: u16,
    result: Option<FxElemMotionResult>,
    gap: Option<u8>,
}

fn eval_pending_collide(
    catalog: &FxDefinitions,
    clip_world: Option<&SimWorld>,
    job: &PendingCollide,
) -> CollideJobOut {
    let q = job.query();
    let mut out = CollideJobOut {
        handle: job.handle,
        result: None,
        gap: None,
    };
    let Some(effect) = catalog_lookup(catalog, q.catalog_index) else {
        return out;
    };
    let Some(elem) = effect.elems.get(q.def_index as usize) else {
        return out;
    };
    if fx_iw4::elem_uses_collision(elem.view.flags) {
        let Some(world) = clip_world else {
            out.gap = Some(q.def_index);
            return out;
        };
        out.result = evaluate_elem_collide_motion(
            elem.view.flags,
            elem.view.gravity_base,
            elem.view.gravity_amplitude,
            elem.view.reflection_factor[0],
            elem.view.reflection_factor[1],
            elem.view.coll_mins,
            elem.view.coll_maxs,
            elem.view.use_item_clip != 0,
            !elem.effect_on_impact.is_absent(),
            q.elem_random_seed,
            elem.vel_graph_local.as_slice(),
            elem.vel_graph_world.as_slice(),
            q,
            |start, end, mins, maxs, mask| {
                let t = world.trace_world(start, end, mins, maxs, mask);
                FxElemTraceHit {
                    fraction: t.fraction,
                    normal: t.normal,
                    startsolid: t.startsolid != 0,
                    allsolid: t.allsolid != 0,
                }
            },
        );
        return out;
    }
    out.result = evaluate_elem_motion(
        elem.view.flags,
        elem.view.gravity_base,
        elem.view.gravity_amplitude,
        q.elem_random_seed,
        elem.vel_graph_local.as_slice(),
        elem.vel_graph_world.as_slice(),
        q,
    );
    out
}

const COLLIDE_JOBS_PER_WORKER: usize = 16;

fn collide_worker_count(jobs: usize) -> usize {
    static THREADS: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let threads = *THREADS.get_or_init(|| {
        std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(1).max(1))
            .unwrap_or(1)
    });
    threads.min(jobs / COLLIDE_JOBS_PER_WORKER).max(1)
}

fn run_collide_jobs(
    catalog: &FxDefinitions,
    clip_world: Option<&SimWorld>,
    jobs: &[PendingCollide],
) -> (HashMap<u16, Option<FxElemMotionResult>>, Vec<u8>) {
    let workers = collide_worker_count(jobs.len());
    let outs = if workers <= 1 || jobs.len() == 1 {
        jobs.iter()
            .map(|job| eval_pending_collide(catalog, clip_world, job))
            .collect::<Vec<_>>()
    } else {
        let chunk_len = jobs.len().div_ceil(workers);

        ComputeTaskPool::get()
            .scope(|scope| {
                for chunk in jobs.chunks(chunk_len) {
                    scope.spawn(async move {
                        chunk
                            .iter()
                            .map(|job| eval_pending_collide(catalog, clip_world, job))
                            .collect::<Vec<_>>()
                    });
                }
            })
            .into_iter()
            .flatten()
            .collect()
    };
    let mut map = HashMap::with_capacity(outs.len());
    let mut gaps = Vec::new();
    for out in outs {
        if let Some(def_index) = out.gap {
            gaps.push(def_index);
        }
        map.insert(out.handle, out.result);
    }
    (map, gaps)
}

pub fn tick_fx_non_dependent(
    host: &mut FxSystemHost,
    catalog: &FxDefinitions,
    cache: &FxElemInfoCache,
    msec: i32,
    camera_origin: [f32; 3],
    clip_world: Option<&SimWorld>,
    world: Option<&dyn FxScene>,
) {
    tick_fx_pass(
        host,
        catalog,
        cache,
        msec,
        camera_origin,
        clip_world,
        world,
        true,
        true,
    )
}

pub fn tick_fx_remaining(
    host: &mut FxSystemHost,
    catalog: &FxDefinitions,
    cache: &FxElemInfoCache,
    msec: i32,
    camera_origin: [f32; 3],
    clip_world: Option<&SimWorld>,
    world: Option<&dyn FxScene>,
) {
    tick_fx_pass(
        host,
        catalog,
        cache,
        msec,
        camera_origin,
        clip_world,
        world,
        false,
        false,
    )
}

fn tick_fx_pass(
    host: &mut FxSystemHost,
    catalog: &FxDefinitions,
    cache: &FxElemInfoCache,
    msec: i32,
    camera_origin: [f32; 3],
    clip_world: Option<&SimWorld>,
    world: Option<&dyn FxScene>,
    advance_frame: bool,
    non_bolted_only: bool,
) {
    host.set_msec_now(msec);
    if advance_frame {
        host.advance_frame_stamp();
    }

    let collide_gap_indices = RefCell::new(Vec::new());
    let mut emit_rand_gap_indices = Vec::new();
    fx_update(
        host,
        non_bolted_only,
        camera_origin,
        |sys, slot, prev, now, _| {
            let index = sys
                .effect_at(slot)
                .map(|e| e.catalog_index)
                .unwrap_or(FX_CATALOG_INDEX_NONE);
            let effect = catalog_lookup(catalog, index)?;
            let elems = cache.arc_for(catalog, effect);
            spawn_looping_partial(sys, slot, def_info(effect, &elems), prev, now);
            Some(effect.view.msec_looping_life)
        },
        |q| {
            let out = (|| {
                let effect = catalog_lookup(catalog, q.catalog_index)?;
                let elem = effect.elems.get(q.def_index as usize)?;
                if fx_iw4::elem_uses_collision(elem.view.flags) {
                    let Some(world) = clip_world else {
                        collide_gap_indices.borrow_mut().push(q.def_index);
                        return None;
                    };
                    return evaluate_elem_collide_motion(
                        elem.view.flags,
                        elem.view.gravity_base,
                        elem.view.gravity_amplitude,
                        elem.view.reflection_factor[0],
                        elem.view.reflection_factor[1],
                        elem.view.coll_mins,
                        elem.view.coll_maxs,
                        elem.view.use_item_clip != 0,
                        !elem.effect_on_impact.is_absent(),
                        q.elem_random_seed,
                        elem.vel_graph_local.as_slice(),
                        elem.vel_graph_world.as_slice(),
                        q,
                        |start, end, mins, maxs, mask| {
                            let t = world.trace_world(start, end, mins, maxs, mask);
                            FxElemTraceHit {
                                fraction: t.fraction,
                                normal: t.normal,
                                startsolid: t.startsolid != 0,
                                allsolid: t.allsolid != 0,
                            }
                        },
                    );
                }
                evaluate_elem_motion(
                    elem.view.flags,
                    elem.view.gravity_base,
                    elem.view.gravity_amplitude,
                    q.elem_random_seed,
                    elem.vel_graph_local.as_slice(),
                    elem.vel_graph_world.as_slice(),
                    q,
                )
            })();
            out
        },
        |q| {
            let effect = catalog_lookup(catalog, q.catalog_index)?;
            let elem = effect.elems.get(q.def_index as usize)?;
            if elem.effect_emitted.is_absent() {
                return None;
            }
            let (base, max) = fx_iw4::emit_dist_range(
                elem.view.emit_dist[0],
                elem.view.emit_dist[1],
                elem.view.emit_dist_variance[0],
                elem.view.emit_dist_variance[1],
                q.elem_random_seed,
            );

            if elem.view.emit_dist_variance[1] != 0.0 {
                emit_rand_gap_indices.push(q.def_index);
            }
            Some(fx_iw4::process_emitting_schedule(
                q.emit_residual,
                q.origin_begin,
                q.origin_end,
                q.msec_update_begin,
                q.msec_update_end,
                base,
                max,
                || 0.0,
            ))
        },
        |sys, req| {
            let Some(parent) = catalog_lookup(catalog, req.catalog_index) else {
                return false;
            };
            let Some(elem) = parent.elems.get(req.def_index as usize) else {
                return false;
            };
            let child_edge = match req.kind {
                FxChildKind::Impact => elem.effect_on_impact,
                FxChildKind::Death => elem.effect_on_death,
                FxChildKind::Emitted => elem.effect_emitted,
            };

            if child_edge.is_absent() {
                return true;
            }

            let Some(index) = child_edge.bound_index() else {
                return false;
            };
            let Some(child) = catalog.def_at(index).filter(|d| !d.elems.is_empty()) else {
                return false;
            };
            let child_name = child.name.as_str();
            let elems = cache.arc_for(catalog, child);
            match spawn_impact_or_death_effect(
                sys,
                FxPlayRequest {
                    def_name: child_name,
                    pose: FxPlayPose {
                        origin: req.origin,
                        axis: req.axis,
                        msec: req.msec,
                    },
                    wants_spotlight: false,
                    catalog_index: catalog_index_of(catalog, child),
                    def: Some(def_info(child, &elems)),
                },
            ) {
                PlayResult::PlayedReleased { .. } | PlayResult::Held { .. } => true,
                PlayResult::Failed(_) => false,
            }
        },
        |index, def_index| {
            let effect = catalog_lookup(catalog, index)?;
            cache
                .arc_for(catalog, effect)
                .get(def_index as usize)
                .copied()
        },
        |start, end, mins, maxs, mask| {
            let Some(world) = clip_world else {
                return None;
            };

            let t = world.trace_world(start, end, mins, maxs, mask);
            Some(FxTrailCollideHit {
                fraction: t.fraction,
                normal: t.normal,
                startsolid: t.startsolid != 0,
                allsolid: t.allsolid != 0,
            })
        },
        |index, def_index| {
            catalog_lookup(catalog, index)
                .and_then(|effect| effect.elems.get(def_index as usize))
                .map(|elem| (elem.vel_graph_local.clone(), elem.vel_graph_world.clone()))
                .unwrap_or_else(|| (Vec::new(), Vec::new()))
        },
        |q: FxSparkFillQuery<'_>| {
            let effect = catalog_lookup(catalog, q.catalog_index)?;
            let elem = effect.elems.get(q.def_index as usize)?;
            let rand_size = sample_f32(q.elem_random_seed, FxRandomChannel::Size);
            let rand_scale = sample_f32(q.elem_random_seed, FxRandomChannel::Scale);
            let rand_color = sample_f32(q.elem_random_seed, FxRandomChannel::Color);
            let size0 = evaluate_size0(
                elem.vis_samples.as_slice(),
                elem.view.vis_state_interval_count,
                q.norm_time,
                rand_size,
            )?;
            let scale = evaluate_scale(
                elem.vis_samples.as_slice(),
                elem.view.vis_state_interval_count,
                q.norm_time,
                rand_scale,
            )
            .unwrap_or(0.0);
            let color = evaluate_color_rgba(
                elem.vis_samples.as_slice(),
                elem.view.vis_state_interval_count,
                q.norm_time,
                rand_color,
            )
            .unwrap_or([255, 255, 255, 255]);
            Some(FxSparkFillVisual {
                size0,
                scale,
                color_rgba: color,
                spawn_angles: elem.view.spawn_angles,
                angular_velocity: elem.view.angular_velocity,
            })
        },
        |jobs| {
            if jobs.is_empty() {
                return HashMap::new();
            }
            let (map, mut gaps) = run_collide_jobs(catalog, clip_world, jobs);
            collide_gap_indices.borrow_mut().append(&mut gaps);
            map
        },
        |start, end| {
            let Some(world) = clip_world else {
                return (1.0, [0.0, 0.0, 1.0]);
            };
            let t = world.trace_world(
                start,
                end,
                fx_iw4::FX_SPARK_FOUNTAIN_TRACE_BOUNDS,
                fx_iw4::FX_SPARK_FOUNTAIN_TRACE_BOUNDS,
                fx_iw4::FX_SPARK_FOUNTAIN_TRACE_MASK,
            );
            (t.fraction, t.normal)
        },
    );
    let collide_gap_indices = collide_gap_indices.into_inner();
    for def_index in collide_gap_indices {
        host.gaps
            .raise(FxGapCause::NoWorldClipForCollide { def_index });
    }
    for def_index in emit_rand_gap_indices {
        host.gaps
            .raise(FxGapCause::EmitRandVarianceNotApplied { def_index });
    }
    drain_spawn_side_effects(host, catalog, cache, world);
}

pub fn build_fx_verts(
    host: &mut FxSystemHost,
    catalog: &FxDefinitions,
    cull: FxDrawCull<'_>,
    camera: [f32; 3],
) -> (FxGenerateVertsOut, FxVertsGaps) {
    let mut gaps = FxVertsGaps::default();
    let catalog_index_n = Cell::new(0u32);
    let catalog_name_n = Cell::new(0u32);
    let vis_blocker_adds = RefCell::new(Vec::new());
    // Most adjacent particles use the same definition/material. Share their
    // diagnostic names instead of allocating two strings per generated sprite.
    // The cache lives only as long as this generation call.
    let mut sprite_names: Option<(Arc<str>, Arc<str>)> = None;
    let mut on_elem = |ctx: FxDrawElemContext<'_>| -> Option<FxSpriteInstance> {
        let Some(effect) = catalog_lookup_draw(
            catalog,
            ctx.catalog_index,
            &catalog_index_n,
            &catalog_name_n,
        ) else {
            gaps.no_def = gaps.no_def.saturating_add(1);
            return None;
        };
        let Some(elem) = effect.elems.get(ctx.def_index as usize) else {
            gaps.no_elem = gaps.no_elem.saturating_add(1);
            return None;
        };
        let Some((material_index, material_name)) = elem.primary_material() else {
            gaps.no_material_visual = gaps.no_material_visual.saturating_add(1);
            return None;
        };
        let rand_size = sample_f32(ctx.elem_random_seed, FxRandomChannel::Size);
        let rand_color = sample_f32(ctx.elem_random_seed, FxRandomChannel::Color);
        let Some(size0) = evaluate_size0(
            elem.vis_samples.as_slice(),
            elem.view.vis_state_interval_count,
            ctx.norm_time,
            rand_size,
        ) else {
            gaps.no_size0_sample = gaps.no_size0_sample.saturating_add(1);
            return None;
        };
        if size0 <= 0.0 {
            gaps.size0_not_positive = gaps.size0_not_positive.saturating_add(1);
            return None;
        }

        let size1 = if ctx.elem_type == 2 {
            let Some(s1) = evaluate_size1(
                elem.vis_samples.as_slice(),
                elem.view.vis_state_interval_count,
                ctx.norm_time,
                rand_size,
            ) else {
                gaps.no_size1_sample = gaps.no_size1_sample.saturating_add(1);
                return None;
            };
            if s1 <= 0.0 {
                gaps.size1_not_positive = gaps.size1_not_positive.saturating_add(1);
                return None;
            }
            s1
        } else {
            0.0
        };
        let (mut color, color_missing) = match evaluate_color_rgba(
            elem.vis_samples.as_slice(),
            elem.view.vis_state_interval_count,
            ctx.norm_time,
            rand_color,
        ) {
            Some(c) => (c, false),
            None => ([255, 255, 255, 255], true),
        };
        if color_missing {
            gaps.color_fallback = gaps.color_fallback.saturating_add(1);
        }
        let vis_alpha = color[3];

        color = apply_elem_lighting(
            color,
            elem.view.lighting_frac,
            ctx.packed_lighting,
            ctx.packed_lighting_src,
            ctx.def_index,
            &mut gaps,
        );

        let axis = if ctx.elem_type == 1 {
            get_elem_angles_axis(
                elem.view.spawn_angles,
                elem.view.angular_velocity,
                ctx.elem_random_seed,
                ctx.age_msec as f32,
                ctx.axis,
            )
        } else {
            ctx.axis
        };

        let vel_dir = if ctx.elem_type == 2 {
            let vel = get_velocity_at_time(
                ctx.flags,
                ctx.base_vel,
                ctx.age_msec as f32,
                ctx.life_msec.max(1) as f32,
                elem.vel_graph_local.as_slice(),
                elem.vel_graph_world.as_slice(),
                ctx.axis,
                ctx.elem_random_seed,
            );
            vec3_normalize(vel)
        } else {
            [0.0; 3]
        };
        let rotation_rad = evaluate_rotation_total(
            elem.vis_samples.as_slice(),
            elem.view.vis_state_interval_count,
            ctx.norm_time,
            ctx.elem_random_seed,
            elem.view.initial_rotation,
            ctx.life_msec as f32,
        )
        .unwrap_or(0.0);
        let atlas = if elem.raw.len() >= FX_ELEM_ATLAS_OFF + FX_ELEM_ATLAS_SIZE {
            sprite_atlas_uv(
                &elem.raw[FX_ELEM_ATLAS_OFF..FX_ELEM_ATLAS_OFF + FX_ELEM_ATLAS_SIZE],
                ctx.elem_random_seed,
                ctx.sequence,
                ctx.age_msec,
                ctx.norm_time,
            )
        } else {
            fx_iw4::FxSpriteAtlasUv::FULL
        };

        vis_blocker_adds.borrow_mut().push((
            elem.view.flags,
            ctx.origin,
            size0,
            vis_alpha,
            elem.view.fade_in_range,
            elem.view.fade_out_range,
        ));
        if !sprite_names.as_ref().is_some_and(|(def, material)| {
            def.as_ref() == ctx.def_name && material.as_ref() == material_name
        }) {
            sprite_names = Some((Arc::from(ctx.def_name), Arc::from(material_name)));
        }
        let (def_name, material_name) = sprite_names.as_ref().unwrap();
        Some(FxSpriteInstance {
            viewmodel: false,
            origin: ctx.origin,
            size0,
            size1,
            color_rgba: color,
            elem_type: ctx.elem_type,
            def_name: Arc::clone(def_name),
            catalog_index: ctx.catalog_index,
            def_index: ctx.def_index,
            material_name: Arc::clone(material_name),
            material_index: Some(material_index),
            axis,
            rotation_rad,
            vel_dir,
            atlas,
            flags: ctx.flags,
            lighting_sample: ctx.packed_lighting,
            lighting_frac: elem.view.lighting_frac,
            packed_lighting_src: ctx.packed_lighting_src,
        })
    };

    let mut on_trail_def = |ctx: FxDrawTrailContext<'_>, def_index: u8| -> Option<FxTrailDrawDef> {
        let effect = catalog_lookup_draw(
            catalog,
            ctx.catalog_index,
            &catalog_index_n,
            &catalog_name_n,
        )?;
        let elem = effect.elems.get(def_index as usize)?;
        let trail = elem.trail_def.as_ref()?;
        if trail.verts.is_empty() || trail.inds.len() < 2 || trail.repeat_dist == 0 {
            return None;
        }
        let (material_index, material_name) = elem.primary_material()?;
        Some(FxTrailDrawDef {
            visual_count: elem.view.visual_count,
            flags: elem.view.flags,
            life_base: elem.view.life_span_msec_base,
            life_amp: elem.view.life_span_msec_amplitude,
            scroll_time_msec: trail.scroll_time_msec,
            repeat_dist: trail.repeat_dist,
            verts: trail.verts.clone(),
            inds: trail.inds.clone(),
            material_name: material_name.to_owned(),
            material_index: Some(material_index),
        })
    };
    let mut on_trail_sample = |ctx: FxDrawTrailSampleContext<'_>| -> Option<FxTrailSampleVisual> {
        let effect = catalog_lookup_draw(
            catalog,
            ctx.catalog_index,
            &catalog_index_n,
            &catalog_name_n,
        )?;
        let elem = effect.elems.get(ctx.def_index as usize)?;
        let rand_size = sample_f32(ctx.elem_random_seed, FxRandomChannel::Size);
        let rand_color = sample_f32(ctx.elem_random_seed, FxRandomChannel::Color);
        let size0 = evaluate_size0(
            elem.vis_samples.as_slice(),
            elem.view.vis_state_interval_count,
            ctx.norm_time,
            rand_size,
        )?;
        if size0 <= 0.0 {
            return None;
        }
        let size1 = evaluate_size1(
            elem.vis_samples.as_slice(),
            elem.view.vis_state_interval_count,
            ctx.norm_time,
            rand_size,
        )
        .unwrap_or(size0);
        let color = evaluate_color_rgba(
            elem.vis_samples.as_slice(),
            elem.view.vis_state_interval_count,
            ctx.norm_time,
            rand_color,
        )
        .unwrap_or([255, 255, 255, 255]);
        let rotation = evaluate_rotation_total(
            elem.vis_samples.as_slice(),
            elem.view.vis_state_interval_count,
            ctx.norm_time,
            ctx.elem_random_seed,
            elem.view.initial_rotation,
            ctx.life_msec as f32,
        )
        .unwrap_or(0.0);

        vis_blocker_adds.borrow_mut().push((
            elem.view.flags,
            ctx.sample_origin,
            size0,
            color[3],
            elem.view.fade_in_range,
            elem.view.fade_out_range,
        ));
        Some(FxTrailSampleVisual {
            size: [size0, size1],
            color_rgba: color,
            rotation,
        })
    };

    let mut on_spark_size1 = |ctx: FxSparkDrawQuery<'_>| -> Option<f32> {
        let effect = catalog_lookup_draw(
            catalog,
            ctx.catalog_index,
            &catalog_index_n,
            &catalog_name_n,
        )?;
        let elem = effect.elems.get(ctx.def_index as usize)?;
        let rand_size = sample_f32(ctx.elem_random_seed, FxRandomChannel::Size);
        let size1 = evaluate_size1(
            elem.vis_samples.as_slice(),
            elem.view.vis_state_interval_count,
            ctx.norm_time,
            rand_size,
        )?;
        (size1 >= 0.0).then_some(size1)
    };
    let mut on_cloud = |ctx: FxDrawElemContext<'_>| -> Option<FxCloudInstance> {
        let effect = catalog_lookup_draw(
            catalog,
            ctx.catalog_index,
            &catalog_index_n,
            &catalog_name_n,
        )?;
        let elem = effect.elems.get(ctx.def_index as usize)?;
        let rand_size = sample_f32(ctx.elem_random_seed, FxRandomChannel::Size);
        let rand_scale = sample_f32(ctx.elem_random_seed, FxRandomChannel::Scale);
        let rand_color = sample_f32(ctx.elem_random_seed, FxRandomChannel::Color);
        let size0 = evaluate_size0(
            elem.vis_samples.as_slice(),
            elem.view.vis_state_interval_count,
            ctx.norm_time,
            rand_size,
        )?;
        let size1 = evaluate_size1(
            elem.vis_samples.as_slice(),
            elem.view.vis_state_interval_count,
            ctx.norm_time,
            rand_size,
        )
        .unwrap_or(size0);
        let scale = evaluate_scale(
            elem.vis_samples.as_slice(),
            elem.view.vis_state_interval_count,
            ctx.norm_time,
            rand_scale,
        )?;
        if cull_cloud(
            cull.elem_draw,
            cull.planes,
            cull.planes.len() as u32,
            ctx.flags,
            ctx.origin,
            size0,
            size1,
            scale,
        ) {
            return None;
        }
        let (color, _) = match evaluate_color_rgba(
            elem.vis_samples.as_slice(),
            elem.view.vis_state_interval_count,
            ctx.norm_time,
            rand_color,
        ) {
            Some(c) => (c, false),
            None => ([255, 255, 255, 255], true),
        };
        let rot_t = clamp_elem_rotation_time(
            ctx.age_msec as f32,
            ctx.life_msec as f32,
            ctx.at_rest_fraction,
        );
        let axis = get_elem_angles_axis(
            elem.view.spawn_angles,
            elem.view.angular_velocity,
            ctx.elem_random_seed,
            rot_t,
            ctx.axis,
        );
        let vel = get_velocity_at_time(
            ctx.flags,
            ctx.base_vel,
            ctx.age_msec as f32,
            ctx.life_msec.max(1) as f32,
            elem.vel_graph_local.as_slice(),
            elem.vel_graph_world.as_slice(),
            ctx.axis,
            ctx.elem_random_seed,
        );
        Some(FxCloudInstance {
            def_name: ctx.def_name.to_owned(),
            catalog_index: ctx.catalog_index,
            def_index: ctx.def_index,
            cloud: build_cloud(
                ctx.origin,
                axis,
                size0,
                size1,
                scale,
                color,
                ctx.flags,
                vel,
                ctx.age_msec as f32,
            ),
        })
    };
    let mut on_light = |ctx: FxDrawElemContext<'_>| -> Option<FxElemLightInstance> {
        let effect = catalog_lookup_draw(
            catalog,
            ctx.catalog_index,
            &catalog_index_n,
            &catalog_name_n,
        )?;
        let elem = effect.elems.get(ctx.def_index as usize)?;
        let rand_size = sample_f32(ctx.elem_random_seed, FxRandomChannel::Size);
        let rand_scale = sample_f32(ctx.elem_random_seed, FxRandomChannel::Scale);
        let rand_color = sample_f32(ctx.elem_random_seed, FxRandomChannel::Color);
        let size0 = evaluate_size0(
            elem.vis_samples.as_slice(),
            elem.view.vis_state_interval_count,
            ctx.norm_time,
            rand_size,
        )?;
        if size0 <= 0.0 {
            return None;
        }
        let scale = evaluate_scale(
            elem.vis_samples.as_slice(),
            elem.view.vis_state_interval_count,
            ctx.norm_time,
            rand_scale,
        )?;
        if cull_elem_light(
            cull.elem_draw,
            cull.planes,
            cull.planes.len() as u32,
            ctx.flags,
            ctx.origin,
            size0,
        ) {
            return None;
        }
        let color = evaluate_color_rgba(
            elem.vis_samples.as_slice(),
            elem.view.vis_state_interval_count,
            ctx.norm_time,
            rand_color,
        )?;
        let rot_t = clamp_elem_rotation_time(
            ctx.age_msec as f32,
            ctx.life_msec as f32,
            ctx.at_rest_fraction,
        );
        let axis = get_elem_angles_axis(
            elem.view.spawn_angles,
            elem.view.angular_velocity,
            ctx.elem_random_seed,
            rot_t,
            ctx.axis,
        );
        Some(FxElemLightInstance {
            def_name: ctx.def_name.to_owned(),
            def_index: ctx.def_index,
            is_spot: ctx.elem_type == FxElemType::SpotLight as u8,
            origin: ctx.origin,
            radius: size0,
            color_bgr: elem_light_color_bgr(color, scale),
            axis,
        })
    };
    let mut on_model = |ctx: FxDrawElemContext<'_>| {
        evaluate_fx_model_instance(catalog, ctx, &catalog_index_n, &catalog_name_n)
    };
    let out = generate_verts_with_trails(
        host,
        &mut on_elem,
        &mut on_trail_def,
        &mut on_trail_sample,
        &mut on_spark_size1,
        &mut on_cloud,
        &mut on_light,
        &mut on_model,
        camera,
    );
    for (flags, origin, size0, alpha, fade_in, fade_out) in vis_blocker_adds.into_inner() {
        vis_blocker_add_prepared(
            &mut host.vis_blocker_read,
            flags,
            origin,
            size0,
            alpha,
            fade_in,
            fade_out,
            camera,
        );
    }
    gaps.catalog_index_n = catalog_index_n.get();
    gaps.catalog_name_n = catalog_name_n.get();
    (out, gaps)
}

fn evaluate_fx_model_instance(
    catalog: &FxDefinitions,
    ctx: FxDrawElemContext<'_>,
    catalog_index_n: &Cell<u32>,
    catalog_name_n: &Cell<u32>,
) -> Option<FxModelInstance> {
    let effect = catalog_lookup_draw(catalog, ctx.catalog_index, catalog_index_n, catalog_name_n)?;
    let elem = effect.elems.get(ctx.def_index as usize)?;
    let model_index = elem.model_edge(ctx.elem_random_seed)?.bound_index()?;
    let rand_scale = sample_f32(ctx.elem_random_seed, FxRandomChannel::Scale);
    let scale = evaluate_scale(
        elem.vis_samples.as_slice(),
        elem.view.vis_state_interval_count,
        ctx.norm_time,
        rand_scale,
    )?;
    if scale == 0.0 {
        return None;
    }
    let rot_t = clamp_elem_rotation_time(
        ctx.age_msec as f32,
        ctx.life_msec as f32,
        ctx.at_rest_fraction,
    );
    let axis = get_elem_angles_axis(
        elem.view.spawn_angles,
        elem.view.angular_velocity,
        ctx.elem_random_seed,
        rot_t,
        ctx.axis,
    );
    Some(FxModelInstance {
        def_name: ctx.def_name.to_owned(),
        def_index: ctx.def_index,
        model_index,
        elem_handle: ctx.elem_handle,
        origin: ctx.origin,
        axis,
        scale,
        flags: ctx.flags,
    })
}

pub(crate) fn owned_elem_infos(effect: &OwnedFxEffectDef) -> Vec<FxElemDefInfo> {
    effect
        .elems
        .iter()
        .map(|e| FxElemDefInfo {
            elem_type: e.view.elem_type,
            spawn_a: e.view.spawn_a,
            spawn_b: e.view.spawn_b,
            delay_base: e.view.spawn_delay_msec_base,
            delay_amp: e.view.spawn_delay_msec_amplitude,
            life_base: e.view.life_span_msec_base,
            life_amp: e.view.life_span_msec_amplitude,
            flags: e.view.flags,
            visual_count: e.view.visual_count,
            vis_state_interval_count: e.view.vis_state_interval_count,
            spawn_range_base: e.view.spawn_range_base,
            spawn_range_amp: e.view.spawn_range_amplitude,
            spawn_origin: e.view.spawn_origin,
            spawn_offset_radius_base: e.view.spawn_offset_radius_base,
            spawn_offset_radius_amp: e.view.spawn_offset_radius_amplitude,
            spawn_offset_height_base: e.view.spawn_offset_height_base,
            spawn_offset_height_amp: e.view.spawn_offset_height_amplitude,
            has_effect_on_impact: !e.effect_on_impact.is_absent(),
            has_effect_on_death: !e.effect_on_death.is_absent(),
            has_effect_emitted: !e.effect_emitted.is_absent(),
            sort_order: e.view.sort_order,
            spark_count: e.spark_fountain_def.map(|d| d.spark_count).unwrap_or(0),
            spark_vel_min: e.spark_fountain_def.map(|d| d.vel_min).unwrap_or(0.0),
            spark_vel_max: e.spark_fountain_def.map(|d| d.vel_max).unwrap_or(0.0),
            spark_vel_cone_frac: e.spark_fountain_def.map(|d| d.vel_cone_frac).unwrap_or(0.0),
            spark_gravity: e.spark_fountain_def.map(|d| d.gravity).unwrap_or(0.0),
            spark_length: e.spark_fountain_def.map(|d| d.spark_length).unwrap_or(0.0),
            spark_loop_time: e.spark_fountain_def.map(|d| d.loop_time).unwrap_or(0.0),
            spark_boost_time: e.spark_fountain_def.map(|d| d.boost_time).unwrap_or(0.0),
            spark_boost_factor: e.spark_fountain_def.map(|d| d.boost_factor).unwrap_or(0.0),
            spark_bounce_frac: e.spark_fountain_def.map(|d| d.bounce_frac).unwrap_or(0.0),
            spark_bounce_rand: e.spark_fountain_def.map(|d| d.bounce_rand).unwrap_or(0.0),
            inv_split_dist: e
                .trail_def
                .as_ref()
                .map(|d| d.inv_split_dist)
                .unwrap_or(0.0),
            inv_split_arc_dist: e
                .trail_def
                .as_ref()
                .map(|d| d.inv_split_arc_dist)
                .unwrap_or(0.0),
            inv_split_time: e
                .trail_def
                .as_ref()
                .map(|d| d.inv_split_time)
                .unwrap_or(0.0),
            gravity_base: e.view.gravity_base,
            gravity_amp: e.view.gravity_amplitude,
            reflection_base: e.view.reflection_factor[0],
            reflection_amp: e.view.reflection_factor[1],
            coll_mins: e.view.coll_mins,
            coll_maxs: e.view.coll_maxs,
            use_item_clip: e.view.use_item_clip,
            spawn_angles: e.view.spawn_angles,
            angular_velocity: e.view.angular_velocity,
        })
        .collect()
}

fn axis_from_angles_deg(angles_deg: [f32; 3]) -> [[f32; 3]; 3] {
    let (forward, _right, _up) = angle_vectors(angles_deg);
    axis_from_hit_normal(forward)
}
