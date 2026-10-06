use fx_iw4::{
    FX_TRAIL_ELEM_POOL_CAPACITY, FX_TRAIL_ELEM_RUNTIME_STRIDE, FX_TRAIL_POOL_CAPACITY,
    FX_TRAIL_RUNTIME_STRIDE, FxOrientFrame, FxOrientSpawnParams, FxRandomChannel, FxTrailSplit,
    collide_substep_schedule, collision_reflect_base_vel_delta, compress_basis_from_axis,
    elem_dies_on_touch, elem_gravity_accel_z_sampled, elem_skips_position_update,
    elem_update_has_velocity_graph, elem_uses_collision, elem_uses_vel_local, elem_uses_vel_world,
    get_orientation, get_velocity_at_time, impact_child_speed_allows, integrate_velocity_graph,
    orientation_pos_from_world, orientation_pos_to_world, sample_life_span_msec,
    sample_reflection_factor, sample_u16, spawn_origin_world, status_is_unique_done, trace_mask,
    trail_elem_base_vel_z_pack, trail_elem_handle_for_slot, trail_elem_keep, trail_elem_norm_ages,
    trail_handle_for_slot, trail_random_seed, trail_split_interpolant_msec,
    trail_split_interpolant_t, trail_split_lerp_axis, trail_split_lerp_origin,
    trail_split_skips_update, trail_split_window, vec3_length_sq,
};

use crate::def::FxElemDefInfo;
use crate::gaps::FxGapCause;
use crate::system::FxSystemHost;

pub const FX_TRAIL_HANDLE_NONE: u16 = 0xffff;

#[derive(Clone, Debug)]
pub struct FxTrailSlot {
    pub occupied: bool,

    pub next_trail_handle: u16,
    pub first_elem_handle: u16,
    pub last_elem_handle: u16,
    pub def_index: i8,
    pub sequence: i8,

    pub split_leftover: f32,
}

impl Default for FxTrailSlot {
    fn default() -> Self {
        Self {
            occupied: false,
            next_trail_handle: FX_TRAIL_HANDLE_NONE,
            first_elem_handle: FX_TRAIL_HANDLE_NONE,
            last_elem_handle: FX_TRAIL_HANDLE_NONE,
            def_index: 0,
            sequence: 0,
            split_leftover: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FxTrailElemSlot {
    pub occupied: bool,
    pub origin: [f32; 3],
    pub spawn_dist: f32,
    pub msec_begin: i32,

    pub next_trail_elem_handle: u16,
    pub base_vel_z: i16,
    pub basis: [i8; 6],
    pub sequence: u8,
}

impl Default for FxTrailElemSlot {
    fn default() -> Self {
        Self {
            occupied: false,
            origin: [0.0; 3],
            spawn_dist: 0.0,
            msec_begin: 0,
            next_trail_elem_handle: FX_TRAIL_HANDLE_NONE,
            base_vel_z: 0,
            basis: [0; 6],
            sequence: 0,
        }
    }
}

#[inline]
pub fn trail_slot_for_handle(handle: u16) -> Option<usize> {
    if handle == FX_TRAIL_HANDLE_NONE {
        return None;
    }
    let byte = (handle as u32) << 2;
    if byte % FX_TRAIL_RUNTIME_STRIDE as u32 != 0 {
        return None;
    }
    let slot = (byte / FX_TRAIL_RUNTIME_STRIDE as u32) as usize;
    (slot < FX_TRAIL_POOL_CAPACITY as usize).then_some(slot)
}

#[inline]
pub fn trail_elem_slot_for_handle(handle: u16) -> Option<usize> {
    if handle == FX_TRAIL_HANDLE_NONE {
        return None;
    }
    let byte = (handle as u32) << 2;
    if byte % FX_TRAIL_ELEM_RUNTIME_STRIDE as u32 != 0 {
        return None;
    }
    let slot = (byte / FX_TRAIL_ELEM_RUNTIME_STRIDE as u32) as usize;
    (slot < FX_TRAIL_ELEM_POOL_CAPACITY as usize).then_some(slot)
}

pub fn alloc_trail_elem(host: &mut FxSystemHost) -> Option<usize> {
    let dense = host.trail_elem_first_free?;
    if dense >= host.trail_elems.len() {
        host.trail_elem_first_free = None;
        host.trail_elem_alloc_failures = host.trail_elem_alloc_failures.saturating_add(1);
        return None;
    }
    let next = host.trail_elems[dense].next_trail_elem_handle;
    host.trail_elem_first_free = if next == FX_TRAIL_HANDLE_NONE {
        None
    } else {
        Some(next as usize)
    };
    host.trail_elem_live_count = host.trail_elem_live_count.saturating_add(1);
    Some(dense)
}

fn free_trail_elem_first(
    host: &mut FxSystemHost,
    effect_slot: usize,
    trail_slot: usize,
    handle: u16,
) {
    free_trail_elem(host, effect_slot, trail_slot, handle, FX_TRAIL_HANDLE_NONE);
}

fn free_trail_elem(
    host: &mut FxSystemHost,
    effect_slot: usize,
    trail_slot: usize,
    handle: u16,
    prev_handle: u16,
) {
    if prev_handle == FX_TRAIL_HANDLE_NONE {
        if host.trails.get(trail_slot).map(|t| t.first_elem_handle) != Some(handle) {
            return;
        }
    } else {
        let Some(prev_slot) = trail_elem_slot_for_handle(prev_handle) else {
            return;
        };
        if host
            .trail_elems
            .get(prev_slot)
            .map(|e| e.next_trail_elem_handle)
            != Some(handle)
        {
            return;
        }
    }
    let Some(elem_slot) = trail_elem_slot_for_handle(handle) else {
        return;
    };
    if !host.trail_elems.get(elem_slot).is_some_and(|e| e.occupied) {
        return;
    }
    let next = host.trail_elems[elem_slot].next_trail_elem_handle;
    if host.trails[trail_slot].last_elem_handle == handle {
        host.trails[trail_slot].last_elem_handle = prev_handle;
    }
    if prev_handle == FX_TRAIL_HANDLE_NONE {
        host.trails[trail_slot].first_elem_handle = next;
    } else if let Some(prev_slot) = trail_elem_slot_for_handle(prev_handle) {
        host.trail_elems[prev_slot].next_trail_elem_handle = next;
    }
    let next_free = host
        .trail_elem_first_free
        .map(|d| d as u16)
        .unwrap_or(FX_TRAIL_HANDLE_NONE);
    host.trail_elems[elem_slot] = FxTrailElemSlot {
        occupied: false,
        next_trail_elem_handle: next_free,
        ..FxTrailElemSlot::default()
    };
    host.trail_elem_first_free = Some(elem_slot);
    host.trail_elem_live_count = host.trail_elem_live_count.saturating_sub(1);
    let (own_handle, unique) = match host.effect_at(effect_slot) {
        Some(e) => (e.own_handle, status_is_unique_done(e.status)),
        None => return,
    };
    if unique {
        host.del_ref_to_effect(own_handle);
    }
    if let Some(effect) = host.effect_at_mut(effect_slot) {
        effect.status = effect.status.wrapping_sub(1);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FxTrailCollideHit {
    pub fraction: f32,
    pub normal: [f32; 3],
    pub startsolid: bool,
    pub allsolid: bool,
}

fn trail_graph_integrate(
    origin: [f32; 3],
    bvz: f32,
    dt: f32,
    g: f32,
    orient: fx_iw4::FxOrientation,
) -> ([f32; 3], f32) {
    let mut world = orientation_pos_to_world(orient.origin, orient.axis, origin);
    world[2] += bvz * dt;
    world[2] -= g * dt * dt * 0.5;
    let bvz = bvz - g * dt;
    (
        orientation_pos_from_world(orient.origin, orient.axis, world),
        bvz,
    )
}

fn trail_apply_get_graph_delta(
    origin: [f32; 3],
    flags: i32,
    age0: f32,
    age1: f32,
    life_ms: f32,
    vel_local: &[fx_iw4::FxElemVec3Range],
    vel_world: &[fx_iw4::FxElemVec3Range],
    orient: fx_iw4::FxOrientation,
    seed: u64,
) -> [f32; 3] {
    let mut stored = origin;
    if elem_uses_vel_local(flags) && vel_local.len() >= 2 {
        let d = integrate_velocity_graph(vel_local, age0, age1, life_ms, seed);
        stored[0] += d[0];
        stored[1] += d[1];
        stored[2] += d[2];
    }
    let mut world = orientation_pos_to_world(orient.origin, orient.axis, stored);
    if elem_uses_vel_world(flags) && vel_world.len() >= 2 {
        let d = integrate_velocity_graph(vel_world, age0, age1, life_ms, seed);
        world[0] += d[0];
        world[1] += d[1];
        world[2] += d[2];
    }
    orientation_pos_from_world(orient.origin, orient.axis, world)
}

fn trail_update_elem_motion(
    origin: [f32; 3],
    bvz: f32,
    dt: f32,
    g: f32,
    flags: i32,
    age0: f32,
    age1: f32,
    life_ms: f32,
    vel_local: &[fx_iw4::FxElemVec3Range],
    vel_world: &[fx_iw4::FxElemVec3Range],
    orient: fx_iw4::FxOrientation,
    seed: u64,
) -> ([f32; 3], f32) {
    let after = trail_apply_get_graph_delta(
        origin, flags, age0, age1, life_ms, vel_local, vel_world, orient, seed,
    );
    trail_graph_integrate(after, bvz, dt, g, orient)
}

fn apply_trail_sample_position(
    host: &mut FxSystemHost,
    effect_slot: usize,
    trail_slot: usize,
    elem_handle: u16,
    prev_elem_handle: u16,
    def: FxElemDefInfo,
    def_index: u8,
    prev_msec: i32,
    msec_now: i32,
    now: &FxOrientFrame,
    alt: &FxOrientFrame,
    random_seed: u64,
    vel_local: &[fx_iw4::FxElemVec3Range],
    vel_world: &[fx_iw4::FxElemVec3Range],
    on_trail_trace: &mut impl FnMut(
        [f32; 3],
        [f32; 3],
        [f32; 3],
        [f32; 3],
        u32,
    ) -> Option<FxTrailCollideHit>,
) {
    if elem_skips_position_update(def.elem_type, def.flags) {
        return;
    }
    let Some(elem_slot) = trail_elem_slot_for_handle(elem_handle) else {
        return;
    };
    let Some(elem) = host.trail_elems.get(elem_slot).filter(|e| e.occupied) else {
        return;
    };
    let seed = trail_random_seed(random_seed, elem.sequence as i8);
    let life = sample_life_span_msec(
        def.life_base,
        def.life_amp,
        sample_u16(seed, FxRandomChannel::Life),
    );
    let ages = trail_elem_norm_ages(prev_msec, msec_now, elem.msec_begin, life);
    let life_f = life as f32;
    let prev = if prev_msec < elem.msec_begin {
        elem.msec_begin
    } else {
        prev_msec
    };
    let spawn = FxOrientSpawnParams {
        spawn_origin: def.spawn_origin,
        spawn_offset_radius: [def.spawn_offset_radius_base, def.spawn_offset_radius_amp],
        spawn_offset_height: [def.spawn_offset_height_base, def.spawn_offset_height_amp],
        seed,
    };
    let orient = get_orientation(def.flags, now, alt, Some(spawn));
    let g = elem_gravity_accel_z_sampled(def.gravity_base, def.gravity_amp, seed);
    let graph = elem_update_has_velocity_graph(def.flags);
    if elem_uses_collision(def.flags) {
        let mut origin = elem.origin;
        let mut bvz = elem.base_vel_z as f32;
        let msec_begin = elem.msec_begin;
        let mask = trace_mask(def.use_item_clip != 0);
        for step in collide_substep_schedule(prev, msec_now) {
            let mut t0 = step.msec_start;
            let t_end = step.msec_end;

            for _ in 0..8 {
                if t0 >= t_end {
                    break;
                }
                let dt = (t_end.wrapping_sub(t0) as f32) * 0.001;
                let (age0, age1) = trail_elem_norm_ages(t0, t_end, msec_begin, life);
                let (end_origin, end_bvz) = trail_update_elem_motion(
                    origin, bvz, dt, g, def.flags, age0, age1, life_f, vel_local, vel_world,
                    orient, seed,
                );
                let start_w = orientation_pos_to_world(orient.origin, orient.axis, origin);
                let end_w = orientation_pos_to_world(orient.origin, orient.axis, end_origin);
                let Some(hit) = on_trail_trace(start_w, end_w, def.coll_mins, def.coll_maxs, mask)
                else {
                    host.gaps
                        .raise(FxGapCause::NoWorldClipForCollide { def_index });
                    return;
                };
                if hit.startsolid || hit.allsolid {
                    bvz = 0.0;
                    break;
                }
                if hit.fraction >= 1.0 {
                    origin = end_origin;
                    bvz = end_bvz;
                    break;
                }
                let hit_w = [
                    start_w[0] + (end_w[0] - start_w[0]) * hit.fraction,
                    start_w[1] + (end_w[1] - start_w[1]) * hit.fraction,
                    start_w[2] + (end_w[2] - start_w[2]) * hit.fraction,
                ];
                origin = orientation_pos_from_world(orient.origin, orient.axis, hit_w);
                let age_at_hit_msec = (t0.saturating_sub(msec_begin)).max(0) as f32
                    + (t_end.saturating_sub(t0)).max(0) as f32 * hit.fraction;
                let pre_vel = get_velocity_at_time(
                    def.flags,
                    [0.0, 0.0, end_bvz],
                    age_at_hit_msec,
                    life_f,
                    vel_local,
                    vel_world,
                    now.axis,
                    seed,
                );
                if def.has_effect_on_impact && impact_child_speed_allows(vec3_length_sq(pre_vel)) {
                    let (parent_def_name, catalog_index) = match host.effect_at(effect_slot) {
                        Some(e) => (e.def_name.clone(), e.catalog_index),
                        None => (String::new(), crate::system::FX_CATALOG_INDEX_NONE),
                    };
                    host.pending_trail_impacts
                        .push(crate::system::PendingTrailImpact {
                            parent_def_name,
                            catalog_index,
                            def_index,
                            origin: hit_w,
                            pre_vel,
                            msec: msec_now,
                        });
                }
                if elem_dies_on_touch(def.flags) {
                    free_trail_elem(host, effect_slot, trail_slot, elem_handle, prev_elem_handle);
                    return;
                }
                let reflection =
                    sample_reflection_factor(def.reflection_base, def.reflection_amp, seed);
                let delta =
                    collision_reflect_base_vel_delta([0.0, 0.0, end_bvz], hit.normal, reflection);
                bvz = end_bvz + delta[2];
                let span = (t_end.saturating_sub(t0)).max(0) as f32;
                t0 += (span * hit.fraction) as i32;
                if t0 <= step.msec_start {
                    t0 = step.msec_start.saturating_add(1);
                }
            }
        }
        let Some(elem) = host.trail_elems.get_mut(elem_slot) else {
            return;
        };
        elem.origin = origin;
        elem.base_vel_z = trail_elem_base_vel_z_pack(bvz);
        return;
    }
    let origin = elem.origin;
    let bvz = elem.base_vel_z as f32;
    if graph {
        let dt = (msec_now.wrapping_sub(prev) as f32) * 0.001;
        let (stored, bvz) = trail_update_elem_motion(
            origin, bvz, dt, g, def.flags, ages.0, ages.1, life_f, vel_local, vel_world, orient,
            seed,
        );
        let Some(elem) = host.trail_elems.get_mut(elem_slot) else {
            return;
        };
        elem.origin = stored;
        elem.base_vel_z = trail_elem_base_vel_z_pack(bvz);
        return;
    }

    let stored = trail_apply_get_graph_delta(
        origin, def.flags, ages.0, ages.1, life_f, vel_local, vel_world, orient, seed,
    );
    let Some(elem) = host.trail_elems.get_mut(elem_slot) else {
        return;
    };
    elem.origin = stored;
}

pub fn alloc_trail(host: &mut FxSystemHost) -> Option<u16> {
    let Some(dense) = host.trail_first_free else {
        host.trail_alloc_failures = host.trail_alloc_failures.saturating_add(1);
        return None;
    };
    if dense >= host.trails.len() {
        host.trail_first_free = None;
        host.trail_alloc_failures = host.trail_alloc_failures.saturating_add(1);
        return None;
    }
    let next = host.trails[dense].next_trail_handle;
    host.trail_first_free = if next == FX_TRAIL_HANDLE_NONE {
        None
    } else {
        Some(next as usize)
    };
    host.trail_live_count = host.trail_live_count.saturating_add(1);
    let handle = trail_handle_for_slot(dense as u32);
    host.trails[dense] = FxTrailSlot {
        occupied: true,
        next_trail_handle: FX_TRAIL_HANDLE_NONE,
        first_elem_handle: FX_TRAIL_HANDLE_NONE,
        last_elem_handle: FX_TRAIL_HANDLE_NONE,
        def_index: 0,
        sequence: 0,
        split_leftover: 0.0,
    };
    Some(handle)
}

pub fn update_trail(
    host: &mut FxSystemHost,
    effect_slot: usize,
    trail_handle: u16,
    elem_def: FxElemDefInfo,
    sample_origin: [f32; 3],
    sample_axis: [[f32; 3]; 3],
    msec: i32,
    spawn_dist: f32,
) -> bool {
    let Some(trail_slot) = trail_slot_for_handle(trail_handle) else {
        return false;
    };
    if !host.trails.get(trail_slot).is_some_and(|t| t.occupied) {
        return false;
    }

    let effect_seed = match host.effect_at(effect_slot) {
        Some(e) if e.ring_resident => e.random_seed,
        _ => return false,
    };

    let sequence = host.trails[trail_slot].sequence;
    let seed = trail_random_seed(effect_seed, sequence);

    let mut msec_begin = elem_def.delay_base.wrapping_add(msec);
    if elem_def.delay_amp != 0 {
        let delay_rand = sample_u16(seed, FxRandomChannel::Delay);
        msec_begin =
            msec_begin.wrapping_add(sample_life_span_msec(0, elem_def.delay_amp, delay_rand));
    }

    let life_rand = sample_u16(seed, FxRandomChannel::Life);
    let life_ms = sample_life_span_msec(elem_def.life_base, elem_def.life_amp, life_rand);

    let within_life = host.msec_now < life_ms.wrapping_add(msec_begin);
    if !(elem_def.keep_alive_by_child() || within_life) {
        return false;
    }

    let Some(elem_dense) = alloc_trail_elem(host) else {
        return false;
    };
    let elem_handle = trail_elem_handle_for_slot(elem_dense as u32);

    if let Some(effect) = host.effect_at_mut(effect_slot) {
        effect.status = effect.status.wrapping_add(1);
    }

    let last = host.trails[trail_slot].last_elem_handle;
    if last == FX_TRAIL_HANDLE_NONE {
        host.trails[trail_slot].first_elem_handle = elem_handle;
    } else if let Some(last_slot) = trail_elem_slot_for_handle(last) {
        host.trail_elems[last_slot].next_trail_elem_handle = elem_handle;
    }

    let seq_byte = sequence as u8;
    let origin = spawn_origin_world(
        sample_origin,
        sample_axis,
        elem_def.spawn_origin,
        elem_def.flags,
        elem_def.spawn_offset_radius_base,
        elem_def.spawn_offset_radius_amp,
        elem_def.spawn_offset_height_base,
        elem_def.spawn_offset_height_amp,
        seed,
    );
    host.trail_elems[elem_dense] = FxTrailElemSlot {
        occupied: true,
        origin,
        spawn_dist,
        msec_begin,
        next_trail_elem_handle: FX_TRAIL_HANDLE_NONE,
        base_vel_z: 0,
        basis: compress_basis_from_axis(sample_axis),
        sequence: seq_byte,
    };
    host.trails[trail_slot].last_elem_handle = elem_handle;
    host.trails[trail_slot].sequence = sequence.wrapping_add(1);
    true
}

pub fn update_effect_trails(
    host: &mut FxSystemHost,
    effect_slot: usize,
    def_index_begin: i32,
    def_index_end: i32,
    prev_msec: i32,
    msec_now: i32,
    camera_origin: [f32; 3],
    mut on_trail_def: impl FnMut(u16, u8) -> Option<FxElemDefInfo>,
) {
    let (catalog_index, mut handle, begin, end, distance, looping, msec_begin) =
        match host.effect_at(effect_slot) {
            Some(e) if e.ring_resident => (
                e.catalog_index,
                e.first_trail_handle,
                FxOrientFrame {
                    origin: e.origin_last,
                    axis: e.axis_last,
                },
                e.frame_now(),
                e.distance,
                e.status & fx_iw4::FX_STATUS_HAS_PENDING_LOOP_ELEMS != 0,
                e.msec_begin,
            ),
            _ => return,
        };

    let distance_delta = fx_iw4::vec3_distance(begin.origin, end.origin);
    let arc_delta = fx_iw4::effect_orient_arc(begin.axis, end.axis);
    while handle != FX_TRAIL_HANDLE_NONE {
        let Some(trail_slot) = trail_slot_for_handle(handle) else {
            break;
        };
        let Some(trail) = host.trails.get(trail_slot).filter(|t| t.occupied) else {
            break;
        };
        let next = trail.next_trail_handle;
        if !looping && (prev_msec != msec_begin || trail.first_elem_handle != FX_TRAIL_HANDLE_NONE)
        {
            handle = next;
            continue;
        }
        let trail_def_index = trail.def_index as u8;
        let def_index = i32::from(trail_def_index);
        if def_index_begin <= def_index && def_index < def_index_end {
            let Some(elem_def) = on_trail_def(catalog_index, trail.def_index as u8) else {
                host.gaps.raise(FxGapCause::TrailDefNotFound {
                    def_index: trail_def_index,
                });
                handle = next;
                continue;
            };

            if host.trails[trail_slot].first_elem_handle == FX_TRAIL_HANDLE_NONE {
                update_trail(
                    host,
                    effect_slot,
                    handle,
                    elem_def,
                    begin.origin,
                    begin.axis,
                    prev_msec,
                    distance,
                );
            } else if looping && prev_msec < msec_now {
                // Replace the moving endpoint; only split samples stay in the history.
                let last = host.trails[trail_slot].last_elem_handle;
                let mut prev = host.trails[trail_slot].first_elem_handle;
                while let Some(slot) = trail_elem_slot_for_handle(prev) {
                    let next = host.trail_elems[slot].next_trail_elem_handle;
                    if next == last {
                        free_trail_elem(host, effect_slot, trail_slot, last, prev);
                        host.trails[trail_slot].sequence =
                            host.trails[trail_slot].sequence.wrapping_sub(1);
                        break;
                    }
                    prev = next;
                }
            }
            if looping && prev_msec < msec_now {
                let leftover_in = host.trails[trail_slot].split_leftover;
                let split = trail_split_window(
                    leftover_in,
                    elem_def.inv_split_time,
                    (msec_now - prev_msec) as f32,
                    elem_def.inv_split_dist,
                    distance_delta,
                    elem_def.inv_split_arc_dist,
                    arc_delta,
                );
                let (leftover, extra) = match split {
                    FxTrailSplit::Hold { leftover } => (leftover, 0),
                    FxTrailSplit::Interpolate { leftover, extra } => (leftover, extra),
                };
                host.trails[trail_slot].split_leftover = leftover;
                if extra > 0 {
                    let acc = leftover + extra as f32;
                    for k in 1..=extra {
                        let t = trail_split_interpolant_t(k as f32, leftover_in, acc);
                        let sample_origin = trail_split_lerp_origin(begin.origin, end.origin, t);
                        let seq = host.trails[trail_slot].sequence as u8;
                        if trail_split_skips_update(
                            seq,
                            elem_def.spawn_range_base,
                            elem_def.spawn_range_amp,
                            camera_origin,
                            sample_origin,
                        ) {
                            host.trails[trail_slot].sequence = (seq as i8).wrapping_add(1);
                        } else {
                            let msec = trail_split_interpolant_msec(prev_msec, msec_now, t);
                            let sample_axis = trail_split_lerp_axis(begin.axis, end.axis, t);
                            update_trail(
                                host,
                                effect_slot,
                                handle,
                                elem_def,
                                sample_origin,
                                sample_axis,
                                msec,
                                distance + distance_delta * t,
                            );
                        }
                    }
                }
                update_trail(
                    host,
                    effect_slot,
                    handle,
                    elem_def,
                    end.origin,
                    end.axis,
                    msec_now,
                    distance + distance_delta,
                );
            }
        }
        handle = next;
    }
}

pub fn apply_partial_last_trail_spawn_dist(
    host: &mut FxSystemHost,
    effect_slot: usize,
    prev_msec: i32,
    msec_now: i32,
    spawn_dist: f32,
    looping: bool,
    mut on_trail_def: impl FnMut(u16, u8) -> Option<FxElemDefInfo>,
    mut on_trail_trace: impl FnMut(
        [f32; 3],
        [f32; 3],
        [f32; 3],
        [f32; 3],
        u32,
    ) -> Option<FxTrailCollideHit>,
    mut on_trail_vel_graphs: impl FnMut(
        u16,
        u8,
    )
        -> (Vec<fx_iw4::FxElemVec3Range>, Vec<fx_iw4::FxElemVec3Range>),
) {
    let (catalog_index, now, alt, random_seed, mut handle) = match host.effect_at(effect_slot) {
        Some(e) if e.ring_resident => (
            e.catalog_index,
            e.frame_now(),
            e.frame_when_played(),
            e.random_seed,
            e.first_trail_handle,
        ),
        _ => return,
    };
    let origin = now.origin;
    let axis = now.axis;
    while handle != FX_TRAIL_HANDLE_NONE {
        let Some(trail_slot) = trail_slot_for_handle(handle) else {
            break;
        };
        let Some(trail) = host.trails.get(trail_slot).filter(|t| t.occupied) else {
            break;
        };
        let next_trail = trail.next_trail_handle;
        let last = trail.last_elem_handle;
        let def_index = trail.def_index as u8;
        let elem_def = on_trail_def(catalog_index, def_index);
        {
            let mut prev = FX_TRAIL_HANDLE_NONE;
            let mut still_prefix = true;
            let mut cur = trail.first_elem_handle;
            while cur != FX_TRAIL_HANDLE_NONE {
                let Some(elem_slot) = trail_elem_slot_for_handle(cur) else {
                    break;
                };
                let Some(elem) = host.trail_elems.get(elem_slot).filter(|e| e.occupied) else {
                    break;
                };
                let next_elem = elem.next_trail_elem_handle;
                let keep = match elem_def {
                    Some(def) => {
                        let seed = trail_random_seed(random_seed, elem.sequence as i8);
                        let life = sample_life_span_msec(
                            def.life_base,
                            def.life_amp,
                            sample_u16(seed, FxRandomChannel::Life),
                        );
                        trail_elem_keep(msec_now, elem.msec_begin, life)
                    }
                    None => true,
                };
                if !keep {
                    if still_prefix && prev != FX_TRAIL_HANDLE_NONE {
                        free_trail_elem_first(host, effect_slot, trail_slot, prev);
                    }
                } else {
                    still_prefix = false;
                }
                prev = cur;
                cur = next_elem;
            }
            if still_prefix && prev != FX_TRAIL_HANDLE_NONE && prev == last {
                free_trail_elem_first(host, effect_slot, trail_slot, prev);
                handle = next_trail;
                continue;
            }
        }
        if prev_msec != msec_now
            && let Some(def) = elem_def
        {
            let (vel_local, vel_world) = on_trail_vel_graphs(catalog_index, def_index);
            let mut prev_elem = FX_TRAIL_HANDLE_NONE;
            let mut cur = host.trails[trail_slot].first_elem_handle;
            while cur != FX_TRAIL_HANDLE_NONE {
                let Some(elem_slot) = trail_elem_slot_for_handle(cur) else {
                    break;
                };
                let Some(elem) = host.trail_elems.get(elem_slot).filter(|e| e.occupied) else {
                    break;
                };
                let next_elem = elem.next_trail_elem_handle;
                apply_trail_sample_position(
                    host,
                    effect_slot,
                    trail_slot,
                    cur,
                    prev_elem,
                    def,
                    def_index,
                    prev_msec,
                    msec_now,
                    &now,
                    &alt,
                    random_seed,
                    vel_local.as_slice(),
                    vel_world.as_slice(),
                    &mut on_trail_trace,
                );
                if host.trail_elems.get(elem_slot).is_some_and(|e| e.occupied) {
                    prev_elem = cur;
                }
                cur = next_elem;
            }
        }
        if looping {
            if let Some(elem_slot) =
                trail_elem_slot_for_handle(host.trails[trail_slot].last_elem_handle)
            {
                if let Some(elem) = host.trail_elems.get_mut(elem_slot).filter(|e| e.occupied) {
                    elem.spawn_dist = spawn_dist;
                    if let Some(elem_def) = elem_def {
                        let seed = trail_random_seed(random_seed, elem.sequence as i8);
                        elem.origin = spawn_origin_world(
                            origin,
                            axis,
                            elem_def.spawn_origin,
                            elem_def.flags,
                            elem_def.spawn_offset_radius_base,
                            elem_def.spawn_offset_radius_amp,
                            elem_def.spawn_offset_height_base,
                            elem_def.spawn_offset_height_amp,
                            seed,
                        );
                        elem.basis = compress_basis_from_axis(axis);
                    }
                }
            }
        }
        handle = next_trail;
    }
}
