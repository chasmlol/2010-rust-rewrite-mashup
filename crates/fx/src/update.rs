use fx_iw4::{
    FX_ELEM_TYPE_SPARK_CLOUD, FX_ELEM_TYPE_SPARK_FOUNTAIN, FX_STATUS_HAS_PENDING_LOOP_ELEMS,
    FX_STATUS_REF_COUNT_MASK_IW4, FxOrientFrame, FxUpdateEffectBolt, axis_to_quat,
    begin_iterating_over_effects_exclusive, bolt_compose_orientation, bolt_mark_lost,
    elem_norm_time, elem_random_seed, elem_uses_collision, end_iterating_over_effects,
    end_iterating_runs_gc, get_orientation, unit_quat_to_axis, update_effect_bolt, vector_vectors,
};
use std::collections::HashMap;

use crate::elem::{FX_ELEM_HANDLE_NONE, elem_slot_for_handle};
use crate::gaps::{ChildSpawn, FxGapCause};
use crate::spark::{
    FxSparkFillVisual, spark_elem_axis, spark_elem_world_origin, update_spark_history,
};
use crate::spawn::{free_elem, stop_effect_non_recursive, stop_pending_loop};
use crate::system::{FX_CATALOG_INDEX_NONE, FxEffectSlot, FxSystemHost};

fn slot_def_name(e: &FxEffectSlot) -> String {
    if e.catalog_index == FX_CATALOG_INDEX_NONE {
        e.def_name.clone()
    } else {
        String::new()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FxElemMotionQuery<'a> {
    pub def_name: &'a str,

    pub catalog_index: u16,
    pub def_index: u8,
    pub age0: f32,
    pub age1: f32,
    pub life_ms: f32,
    pub dt_sec: f32,
    pub base_vel: [f32; 3],

    pub elem_random_seed: u64,

    pub origin: [f32; 3],
    pub prev_msec: i32,
    pub msec_now: i32,
    pub msec_begin: i32,

    pub effect_axis: [[f32; 3]; 3],

    pub orient: fx_iw4::FxOrientation,

    pub at_rest_fraction: u8,
}

#[derive(Clone, Debug)]
pub struct PendingCollide {
    pub handle: u16,
    def_name: String,
    catalog_index: u16,
    def_index: u8,
    age0: f32,
    age1: f32,
    life_ms: f32,
    dt_sec: f32,
    base_vel: [f32; 3],
    elem_random_seed: u64,
    origin: [f32; 3],
    prev_msec: i32,
    msec_now: i32,
    msec_begin: i32,
    effect_axis: [[f32; 3]; 3],
    orient: fx_iw4::FxOrientation,
    at_rest_fraction: u8,
}

impl PendingCollide {
    pub fn query(&self) -> FxElemMotionQuery<'_> {
        FxElemMotionQuery {
            def_name: self.def_name.as_str(),
            catalog_index: self.catalog_index,
            def_index: self.def_index,
            age0: self.age0,
            age1: self.age1,
            life_ms: self.life_ms,
            dt_sec: self.dt_sec,
            base_vel: self.base_vel,
            elem_random_seed: self.elem_random_seed,
            origin: self.origin,
            prev_msec: self.prev_msec,
            msec_now: self.msec_now,
            msec_begin: self.msec_begin,
            effect_axis: self.effect_axis,
            orient: self.orient,
            at_rest_fraction: self.at_rest_fraction,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FxElemMotionResult {
    pub origin_delta: [f32; 3],
    pub base_vel: [f32; 3],

    pub remove: bool,

    pub at_rest_fraction: Option<u8>,

    pub spawn_impact: Option<FxImpactSpawn>,
}

#[derive(Clone, Copy, Debug)]
pub struct FxImpactSpawn {
    pub origin: [f32; 3],

    pub pre_vel: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxChildKind {
    Impact,
    Death,

    Emitted,
}

#[derive(Clone, Copy, Debug)]
pub struct FxChildSpawnRequest<'a> {
    pub kind: FxChildKind,
    pub parent_def_name: &'a str,
    pub catalog_index: u16,
    pub def_index: u8,
    pub origin: [f32; 3],
    pub axis: [[f32; 3]; 3],
    pub msec: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct FxElemTraceHit {
    pub fraction: f32,
    pub normal: [f32; 3],
    pub startsolid: bool,
    pub allsolid: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct FxEmitQuery<'a> {
    pub def_name: &'a str,
    pub catalog_index: u16,
    pub def_index: u8,
    pub origin_begin: [f32; 3],
    pub origin_end: [f32; 3],
    pub msec_update_begin: i32,
    pub msec_update_end: i32,
    pub emit_residual: u8,
    pub elem_random_seed: u64,
    pub flags: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct FxSparkFillQuery<'a> {
    pub def_name: &'a str,
    pub catalog_index: u16,
    pub def_index: u8,
    pub age_msec: i32,
    pub life_msec: i32,
    pub elem_random_seed: u64,
    pub norm_time: f32,
}

pub fn update(
    host: &mut FxSystemHost,
    non_bolted_only: bool,
    camera_origin: [f32; 3],
    mut pending_loop: impl FnMut(&mut FxSystemHost, usize, i32, i32, &str) -> Option<i32>,
    mut motion: impl FnMut(FxElemMotionQuery<'_>) -> Option<FxElemMotionResult>,
    mut on_emit: impl FnMut(FxEmitQuery<'_>) -> Option<fx_iw4::FxEmitSchedule>,
    mut on_child: impl FnMut(&mut FxSystemHost, FxChildSpawnRequest<'_>) -> bool,
    mut on_trail_def: impl FnMut(u16, u8) -> Option<crate::def::FxElemDefInfo>,
    mut on_trail_trace: impl FnMut(
        [f32; 3],
        [f32; 3],
        [f32; 3],
        [f32; 3],
        u32,
    ) -> Option<crate::trail::FxTrailCollideHit>,
    mut on_trail_vel_graphs: impl FnMut(
        u16,
        u8,
    )
        -> (Vec<fx_iw4::FxElemVec3Range>, Vec<fx_iw4::FxElemVec3Range>),
    mut on_spark_fill: impl FnMut(FxSparkFillQuery<'_>) -> Option<FxSparkFillVisual>,
    collide_parallel: impl FnOnce(&[PendingCollide]) -> HashMap<u16, Option<FxElemMotionResult>>,
    mut on_fountain_trace: impl FnMut([f32; 3], [f32; 3]) -> (f32, [f32; 3]),
) {
    host.iterator_count = begin_iterating_over_effects_exclusive(host.iterator_count);

    let start = host.first_active_effect as u32;
    let end = host.first_new_effect as u32;
    let mut pending_apply: Vec<(usize, i32, i32)> = Vec::new();
    let mut collide_jobs: Vec<PendingCollide> = Vec::new();
    let pass_count = if non_bolted_only { 1 } else { 2 };
    for pass in 0..pass_count {
        let want_bolted = pass == 1;
        let mut cursor = start;
        while cursor != end {
            let handle = host.handle_at_ring(cursor);
            if let Some(slot) = host.slot_index_for_handle(handle) {
                if let Some((prev_msec, msec_now)) =
                    stamp_and_pending_loop(host, slot, want_bolted, &mut pending_loop)
                {
                    collect_collide_jobs(host, slot, prev_msec, msec_now, &mut collide_jobs);
                    pending_apply.push((slot, prev_msec, msec_now));
                }
            }
            cursor = cursor.wrapping_add(1);
        }
    }

    let collide_cache = collide_parallel(&collide_jobs);

    for (slot, prev_msec, msec_now) in pending_apply {
        apply_effect_partial(
            host,
            slot,
            prev_msec,
            msec_now,
            camera_origin,
            &mut motion,
            &collide_cache,
            &mut on_emit,
            &mut on_child,
            &mut on_trail_def,
            &mut on_trail_trace,
            &mut on_trail_vel_graphs,
            &mut on_spark_fill,
            &mut on_fountain_trace,
        );
    }

    host.iterator_count = end_iterating_over_effects(host.iterator_count);
    if end_iterating_runs_gc(host.iterator_count, host.needs_garbage_collection) {
        host.run_garbage_collection();
    }
}

fn apply_fx_update_effect_bolt_to_slot(host: &mut FxSystemHost, slot: usize) {
    let lost_handle = {
        let Some(effect) = host.effect_at_mut(slot) else {
            return;
        };
        match update_effect_bolt(
            effect.bolt,
            effect.bolt_packed,
            effect.bolt_centity_teleport,
            effect.bolt_bone_pose.is_some(),
        ) {
            FxUpdateEffectBolt::Skip => None,
            FxUpdateEffectBolt::Refresh => {
                if let Some((origin, axis)) = effect.bolt_bone_pose {
                    let bone_q = axis_to_quat(axis);
                    let (quat, composed) = bolt_compose_orientation(
                        effect.bolt_parent_quat,
                        effect.bolt_parent_origin,
                        bone_q,
                        origin,
                    );
                    effect.origin = composed;
                    effect.axis = unit_quat_to_axis(quat);
                }
                None
            }
            FxUpdateEffectBolt::Lost => {
                effect.bolt_packed = bolt_mark_lost(effect.bolt_packed);
                Some(slot)
            }
        }
    };
    if let Some(slot) = lost_handle {
        stop_effect_non_recursive(host, slot);
    }
}

fn stamp_and_pending_loop(
    host: &mut FxSystemHost,
    slot: usize,
    want_bolted: bool,
    pending_loop: &mut impl FnMut(&mut FxSystemHost, usize, i32, i32, &str) -> Option<i32>,
) -> Option<(i32, i32)> {
    let frame = host.frame_stamp;
    let msec_now = host.msec_now;

    let effect = match host.effect_at_mut(slot) {
        Some(e) if e.ring_resident => e,
        _ => return None,
    };

    if (effect.bolt != 0xff) != want_bolted {
        return None;
    }

    let prev = effect.frame_stamp;
    effect.frame_stamp = frame;
    if prev == frame {
        return None;
    }

    let effect = match host.effect_at(slot) {
        Some(e) if e.ring_resident => e,
        _ => return None,
    };
    if (effect.status & FX_STATUS_REF_COUNT_MASK_IW4) == 0 {
        return None;
    }
    if effect.msec_last_update > msec_now {
        return None;
    }
    apply_fx_update_effect_bolt_to_slot(host, slot);
    let prev_msec = match host.effect_at(slot) {
        Some(e) => e.msec_last_update,
        None => return None,
    };

    let (pending, def_name, msec_begin) = match host.effect_at(slot) {
        Some(e) => (
            (e.status & FX_STATUS_HAS_PENDING_LOOP_ELEMS) != 0,
            slot_def_name(e),
            e.msec_begin,
        ),
        None => return None,
    };
    if pending {
        if let Some(msec_looping_life) =
            pending_loop(host, slot, prev_msec, msec_now, def_name.as_str())
        {
            if msec_looping_life < msec_now.wrapping_sub(msec_begin) {
                stop_pending_loop(host, slot);
            }
        }
    }
    Some((prev_msec, msec_now))
}

fn collect_collide_jobs(
    host: &FxSystemHost,
    effect_slot: usize,
    prev_msec: i32,
    msec_now: i32,
    jobs: &mut Vec<PendingCollide>,
) {
    for class in 0..3 {
        let mut handle = host
            .effect_at(effect_slot)
            .map(|e| e.first_elem_handle[class])
            .unwrap_or(FX_ELEM_HANDLE_NONE);
        while handle != FX_ELEM_HANDLE_NONE {
            let next = elem_slot_for_handle(handle)
                .and_then(|s| host.elems.get(s).map(|e| e.next_elem_handle))
                .unwrap_or(FX_ELEM_HANDLE_NONE);
            if let Some(job) =
                pending_collide_for_elem(host, effect_slot, handle, prev_msec, msec_now)
            {
                jobs.push(job);
            }
            handle = next;
        }
    }
}

fn pending_collide_for_elem(
    host: &FxSystemHost,
    effect_slot: usize,
    handle: u16,
    prev_msec: i32,
    msec_now: i32,
) -> Option<PendingCollide> {
    let slot = elem_slot_for_handle(handle)?;
    let elem = host.elems.get(slot).filter(|e| e.occupied)?;
    if msec_now < elem.msec_begin {
        return None;
    }
    let death = elem.msec_begin.wrapping_add(elem.life_span_msec);
    if msec_now >= death {
        return None;
    }
    if !elem_uses_collision(elem.flags) {
        return None;
    }
    let def_index = elem.def_index;
    let base_vel = elem.base_vel;
    let sequence = elem.sequence;
    let origin = elem.origin;
    let msec_begin = elem.msec_begin;
    let life_msec = elem.life_span_msec.max(1);
    let flags = elem.flags;
    let (def_name, catalog_index, effect_seed, now, alt) = match host.effect_at(effect_slot) {
        Some(e) => (
            slot_def_name(e),
            e.catalog_index,
            e.random_seed,
            e.frame_now(),
            e.frame_when_played(),
        ),
        None => return None,
    };
    let elem_seed = elem_random_seed(effect_seed, sequence, msec_begin);
    let spawn = host
        .elems
        .get(slot)
        .map(|e| e.orient_spawn_params(elem_seed));
    let orient = crate::spark::spark_elem_orientation(flags, &now, &alt, spawn);
    let life_ms = life_msec as f32;
    Some(PendingCollide {
        handle,
        def_name,
        catalog_index,
        def_index,
        age0: ((prev_msec.saturating_sub(msec_begin)).max(0) as f32) / life_ms,
        age1: ((msec_now.saturating_sub(msec_begin)).max(0) as f32) / life_ms,
        life_ms,
        dt_sec: (msec_now.saturating_sub(prev_msec)).max(0) as f32 * 0.001,
        base_vel,
        elem_random_seed: elem_seed,
        origin,
        prev_msec,
        msec_now,
        msec_begin,
        effect_axis: now.axis,
        orient,
        at_rest_fraction: elem.at_rest_fraction,
    })
}

pub(crate) fn apply_update_effect_partial_trails(
    host: &mut FxSystemHost,
    slot: usize,
    prev_msec: i32,
    msec_now: i32,
    on_trail_def: &mut impl FnMut(u16, u8) -> Option<crate::def::FxElemDefInfo>,
    on_trail_trace: &mut impl FnMut(
        [f32; 3],
        [f32; 3],
        [f32; 3],
        [f32; 3],
        u32,
    ) -> Option<crate::trail::FxTrailCollideHit>,
    on_trail_vel_graphs: &mut impl FnMut(
        u16,
        u8,
    )
        -> (Vec<fx_iw4::FxElemVec3Range>, Vec<fx_iw4::FxElemVec3Range>),
) {
    let Some(e) = host.effect_at(slot) else {
        return;
    };
    if e.first_trail_handle == crate::trail::FX_TRAIL_HANDLE_NONE {
        return;
    }
    let looping = (e.status & FX_STATUS_HAS_PENDING_LOOP_ELEMS) != 0;
    let distance = e.distance + fx_iw4::vec3_distance(e.origin_last, e.origin);
    crate::trail::apply_partial_last_trail_spawn_dist(
        host,
        slot,
        prev_msec,
        msec_now,
        distance,
        looping,
        on_trail_def,
        on_trail_trace,
        on_trail_vel_graphs,
    );
}

fn apply_effect_partial(
    host: &mut FxSystemHost,
    slot: usize,
    prev_msec: i32,
    msec_now: i32,
    camera_origin: [f32; 3],
    motion: &mut impl FnMut(FxElemMotionQuery<'_>) -> Option<FxElemMotionResult>,
    collide_cache: &HashMap<u16, Option<FxElemMotionResult>>,
    on_emit: &mut impl FnMut(FxEmitQuery<'_>) -> Option<fx_iw4::FxEmitSchedule>,
    on_child: &mut impl FnMut(&mut FxSystemHost, FxChildSpawnRequest<'_>) -> bool,
    on_trail_def: &mut impl FnMut(u16, u8) -> Option<crate::def::FxElemDefInfo>,
    on_trail_trace: &mut impl FnMut(
        [f32; 3],
        [f32; 3],
        [f32; 3],
        [f32; 3],
        u32,
    ) -> Option<crate::trail::FxTrailCollideHit>,
    on_trail_vel_graphs: &mut impl FnMut(
        u16,
        u8,
    )
        -> (Vec<fx_iw4::FxElemVec3Range>, Vec<fx_iw4::FxElemVec3Range>),
    on_spark_fill: &mut impl FnMut(FxSparkFillQuery<'_>) -> Option<FxSparkFillVisual>,
    on_fountain_trace: &mut impl FnMut([f32; 3], [f32; 3]) -> (f32, [f32; 3]),
) {
    let has_trails = host
        .effect_at(slot)
        .is_some_and(|e| e.first_trail_handle != crate::trail::FX_TRAIL_HANDLE_NONE);
    for class in 0..3 {
        update_partial_for_class(
            host,
            slot,
            class,
            prev_msec,
            msec_now,
            motion,
            collide_cache,
            on_emit,
            on_child,
            on_spark_fill,
            on_fountain_trace,
        );
    }

    if has_trails {
        crate::trail::update_effect_trails(
            host,
            slot,
            i32::MIN,
            i32::MAX,
            prev_msec,
            msec_now,
            camera_origin,
            &mut *on_trail_def,
        );

        apply_update_effect_partial_trails(
            host,
            slot,
            prev_msec,
            msec_now,
            on_trail_def,
            on_trail_trace,
            on_trail_vel_graphs,
        );
        drain_pending_trail_impacts(host, on_child);
    }

    crate::sort::sort_new_elems_in_effect(host, slot, camera_origin);
    if let Some(effect) = host.effect_at_mut(slot) {
        let delta = fx_iw4::vec3_distance(effect.origin_last, effect.origin);
        effect.distance += delta;
        effect.msec_last_update = msec_now;
        effect.commit_frame_last_from_now();
    }
}

fn update_partial_for_class(
    host: &mut FxSystemHost,
    effect_slot: usize,
    class: usize,
    prev_msec: i32,
    msec_now: i32,
    motion: &mut impl FnMut(FxElemMotionQuery<'_>) -> Option<FxElemMotionResult>,
    collide_cache: &HashMap<u16, Option<FxElemMotionResult>>,
    on_emit: &mut impl FnMut(FxEmitQuery<'_>) -> Option<fx_iw4::FxEmitSchedule>,
    on_child: &mut impl FnMut(&mut FxSystemHost, FxChildSpawnRequest<'_>) -> bool,
    on_spark_fill: &mut impl FnMut(FxSparkFillQuery<'_>) -> Option<FxSparkFillVisual>,
    on_fountain_trace: &mut impl FnMut([f32; 3], [f32; 3]) -> (f32, [f32; 3]),
) {
    let mut handle = host
        .effect_at(effect_slot)
        .map(|e| e.first_elem_handle[class])
        .unwrap_or(FX_ELEM_HANDLE_NONE);
    while handle != FX_ELEM_HANDLE_NONE {
        let next = elem_slot_for_handle(handle)
            .and_then(|s| host.elems.get(s).map(|e| e.next_elem_handle))
            .unwrap_or(FX_ELEM_HANDLE_NONE);
        let keep = update_element(
            host,
            effect_slot,
            handle,
            prev_msec,
            msec_now,
            motion,
            collide_cache,
            on_emit,
            on_child,
            on_spark_fill,
            on_fountain_trace,
        );
        if !keep {
            free_elem(host, handle);
        }
        handle = next;
    }
}

fn update_element(
    host: &mut FxSystemHost,
    effect_slot: usize,
    handle: u16,
    prev_msec: i32,
    msec_now: i32,
    motion: &mut impl FnMut(FxElemMotionQuery<'_>) -> Option<FxElemMotionResult>,
    collide_cache: &HashMap<u16, Option<FxElemMotionResult>>,
    on_emit: &mut impl FnMut(FxEmitQuery<'_>) -> Option<fx_iw4::FxEmitSchedule>,
    on_child: &mut impl FnMut(&mut FxSystemHost, FxChildSpawnRequest<'_>) -> bool,
    on_spark_fill: &mut impl FnMut(FxSparkFillQuery<'_>) -> Option<FxSparkFillVisual>,
    on_fountain_trace: &mut impl FnMut([f32; 3], [f32; 3]) -> (f32, [f32; 3]),
) -> bool {
    let Some(slot) = elem_slot_for_handle(handle) else {
        return false;
    };
    let Some(elem) = host.elems.get(slot).filter(|e| e.occupied) else {
        return false;
    };
    if msec_now < elem.msec_begin {
        return true;
    }
    let death = elem.msec_begin.wrapping_add(elem.life_span_msec);
    let def_index = elem.def_index;
    let flags = elem.flags;
    let msec_begin = elem.msec_begin;
    let life_msec = elem.life_span_msec.max(1);
    let base_vel = elem.base_vel;
    let sequence = elem.sequence;
    let origin = elem.origin;
    let elem_type = elem.elem_type;
    let spark_handle = elem.spark_cloud_handle;
    let at_rest_fraction = elem.at_rest_fraction;
    let (def_name, catalog_index, effect_seed, now, alt) = match host.effect_at(effect_slot) {
        Some(e) => (
            slot_def_name(e),
            e.catalog_index,
            e.random_seed,
            e.frame_now(),
            e.frame_when_played(),
        ),
        None => return true,
    };
    let elem_seed = elem_random_seed(effect_seed, sequence, msec_begin);
    let spawn = host
        .elems
        .get(slot)
        .map(|e| e.orient_spawn_params(elem_seed));
    let orient = crate::spark::spark_elem_orientation(flags, &now, &alt, spawn);

    if msec_now >= death {
        spawn_death_child(
            host,
            on_child,
            def_name.as_str(),
            catalog_index,
            def_index,
            flags,
            origin,
            &now,
            &alt,
            spawn,
            msec_now,
        );
        return false;
    }

    let life_ms = life_msec as f32;
    let age0 = ((prev_msec.saturating_sub(msec_begin)).max(0) as f32) / life_ms;
    let age1 = ((msec_now.saturating_sub(msec_begin)).max(0) as f32) / life_ms;
    let dt_sec = (msec_now.saturating_sub(prev_msec)).max(0) as f32 * 0.001;
    let q = FxElemMotionQuery {
        def_name: def_name.as_str(),
        catalog_index,
        def_index,
        age0,
        age1,
        life_ms,
        dt_sec,
        base_vel,
        elem_random_seed: elem_random_seed(effect_seed, sequence, msec_begin),
        origin,
        prev_msec,
        msec_now,
        msec_begin,
        effect_axis: now.axis,
        orient,
        at_rest_fraction,
    };
    match collide_cache
        .get(&handle)
        .copied()
        .unwrap_or_else(|| motion(q))
    {
        Some(result) => {
            let origin_end = [
                origin[0] + result.origin_delta[0],
                origin[1] + result.origin_delta[1],
                origin[2] + result.origin_delta[2],
            ];
            if let Some(elem) = host.elems.get_mut(slot) {
                elem.origin = origin_end;
                elem.base_vel = result.base_vel;
                if let Some(frac) = result.at_rest_fraction {
                    elem.at_rest_fraction = frac;
                }
            }
            let emit_residual = host.elems.get(slot).map(|e| e.emit_residual).unwrap_or(0);
            let elem_seed = elem_random_seed(effect_seed, sequence, msec_begin);
            if let Some(sched) = on_emit(FxEmitQuery {
                def_name: def_name.as_str(),
                catalog_index,
                def_index,
                origin_begin: origin,
                origin_end,
                msec_update_begin: prev_msec,
                msec_update_end: msec_now,
                emit_residual,
                elem_random_seed: elem_seed,
                flags,
            }) {
                if let Some(elem) = host.elems.get_mut(slot) {
                    elem.emit_residual = sched.new_residual;
                }
                let travel = [
                    origin_end[0] - origin[0],
                    origin_end[1] - origin[1],
                    origin_end[2] - origin[2],
                ];
                let axis = if (flags & fx_iw4::FX_ELEM_EMIT_ORIENT_AXIS) != 0 {
                    host.gaps
                        .raise(FxGapCause::EmitOrientQuatNotUnpacked { def_index });
                    now.axis
                } else {
                    vector_vectors(travel)
                };
                for spawn in sched.spawns() {
                    let spawn_origin = fx_iw4::emit_lerp_origin(origin, origin_end, spawn.lerp);
                    let played = on_child(
                        host,
                        FxChildSpawnRequest {
                            kind: FxChildKind::Emitted,
                            parent_def_name: def_name.as_str(),
                            catalog_index,
                            def_index,
                            origin: spawn_origin,
                            axis,
                            msec: spawn.msec_at_spawn,
                        },
                    );
                    if !played {
                        host.gaps.raise(FxGapCause::ChildSpawnRefused {
                            child: ChildSpawn::Emitted,
                            def_index,
                        });
                    }
                }
            }
            if let Some(impact) = result.spawn_impact {
                let played = on_child(
                    host,
                    FxChildSpawnRequest {
                        kind: FxChildKind::Impact,
                        parent_def_name: def_name.as_str(),
                        catalog_index,
                        def_index,
                        origin: impact.origin,
                        axis: vector_vectors(impact.pre_vel),
                        msec: msec_now,
                    },
                );
                if !played {
                    host.gaps.raise(FxGapCause::ChildSpawnRefused {
                        child: ChildSpawn::Impact,
                        def_index,
                    });
                }
            }
            if result.remove {
                let death_origin = match host.elems.get(slot) {
                    Some(e) => e.origin,
                    None => origin,
                };
                spawn_death_child(
                    host,
                    on_child,
                    def_name.as_str(),
                    catalog_index,
                    def_index,
                    flags,
                    death_origin,
                    &now,
                    &alt,
                    spawn,
                    msec_now,
                );
                return false;
            }
        }
        None => {
            host.gaps.raise(FxGapCause::ElemDefNotFound { def_index });
        }
    }

    if elem_type == FX_ELEM_TYPE_SPARK_CLOUD {
        if let Some(visual) = on_spark_fill(FxSparkFillQuery {
            def_name: def_name.as_str(),
            catalog_index,
            def_index,
            age_msec: msec_now.saturating_sub(msec_begin).max(0),
            life_msec,
            elem_random_seed: elem_random_seed(effect_seed, sequence, msec_begin),
            norm_time: elem_norm_time(msec_now.saturating_sub(msec_begin).max(0), life_msec),
        }) {
            let (origin_now, at_rest_now, spawn, seed) = match host.elems.get(slot) {
                Some(e) => {
                    let seed = elem_random_seed(effect_seed, sequence, msec_begin);
                    (
                        e.origin,
                        e.at_rest_fraction,
                        Some(e.orient_spawn_params(seed)),
                        seed,
                    )
                }
                None => (
                    origin,
                    at_rest_fraction,
                    None,
                    elem_random_seed(effect_seed, sequence, msec_begin),
                ),
            };
            let world = spark_elem_world_origin(origin_now, flags, &now, &alt, spawn);
            let axis = spark_elem_axis(
                visual.spawn_angles,
                visual.angular_velocity,
                seed,
                msec_now.saturating_sub(msec_begin).max(0),
                life_msec,
                at_rest_now,
                orient.axis,
            );
            update_spark_history(
                host,
                spark_handle,
                world,
                axis,
                visual.size0,
                visual.scale,
                visual.color_rgba,
                flags,
                msec_now,
            );
        }
    }
    if elem_type == FX_ELEM_TYPE_SPARK_FOUNTAIN {
        crate::spark_fountain::update_spark_fountain(host, spark_handle, on_fountain_trace);
    }
    true
}

fn drain_pending_trail_impacts(
    host: &mut FxSystemHost,
    on_child: &mut impl FnMut(&mut FxSystemHost, FxChildSpawnRequest<'_>) -> bool,
) {
    let impacts = core::mem::take(&mut host.pending_trail_impacts);
    for imp in impacts {
        let played = on_child(
            host,
            FxChildSpawnRequest {
                kind: FxChildKind::Impact,
                parent_def_name: imp.parent_def_name.as_str(),
                catalog_index: imp.catalog_index,
                def_index: imp.def_index,
                origin: imp.origin,
                axis: vector_vectors(imp.pre_vel),
                msec: imp.msec,
            },
        );
        if !played {
            host.gaps.raise(FxGapCause::ChildSpawnRefused {
                child: ChildSpawn::Impact,
                def_index: imp.def_index,
            });
        }
    }
}

fn spawn_death_child(
    host: &mut FxSystemHost,
    on_child: &mut impl FnMut(&mut FxSystemHost, FxChildSpawnRequest<'_>) -> bool,
    parent_def_name: &str,
    catalog_index: u16,
    def_index: u8,
    flags: i32,
    elem_origin: [f32; 3],
    effect_now: &FxOrientFrame,
    effect_alt: &FxOrientFrame,
    spawn: Option<fx_iw4::FxOrientSpawnParams>,
    msec: i32,
) {
    let orient = get_orientation(flags, effect_now, effect_alt, spawn);
    let world = fx_iw4::orientation_pos_to_world(orient.origin, orient.axis, elem_origin);
    let played = on_child(
        host,
        FxChildSpawnRequest {
            kind: FxChildKind::Death,
            parent_def_name,
            catalog_index,
            def_index,
            origin: world,
            axis: orient.axis,
            msec,
        },
    );
    if !played {
        host.gaps.raise(FxGapCause::ChildSpawnRefused {
            child: ChildSpawn::Death,
            def_index,
        });
    }
}
