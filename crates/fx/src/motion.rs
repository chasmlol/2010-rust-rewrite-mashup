use fx_iw4::{
    FX_ELEM_AT_REST_NONE, collide_marks_at_rest, collide_substep_schedule,
    collision_reflect_base_vel_delta, elem_dies_on_touch, elem_gravity_accel_z_sampled,
    elem_update_has_velocity_graph, elem_uses_collision, elem_uses_vel_local, elem_uses_vel_world,
    get_at_rest_fraction, get_velocity_at_time, impact_child_speed_allows,
    integrate_velocity_graph, orientation_pos_from_world, orientation_pos_to_world,
    sample_reflection_factor, trace_mask, vec3_length_sq,
};

use crate::update::{FxElemMotionQuery, FxElemMotionResult, FxElemTraceHit, FxImpactSpawn};

pub fn evaluate_elem_motion(
    flags: i32,
    gravity_base: f32,
    gravity_amp: f32,
    elem_random_seed: u64,
    vel_graph_local: &[fx_iw4::FxElemVec3Range],
    vel_graph_world: &[fx_iw4::FxElemVec3Range],
    q: FxElemMotionQuery<'_>,
) -> Option<FxElemMotionResult> {
    if elem_uses_collision(flags) {
        return None;
    }

    Some(integrate_free_flight(
        flags,
        gravity_base,
        gravity_amp,
        elem_random_seed,
        vel_graph_local,
        vel_graph_world,
        q,
    ))
}

pub fn evaluate_elem_collide_motion(
    flags: i32,
    gravity_base: f32,
    gravity_amp: f32,
    reflection_base: f32,
    reflection_amp: f32,
    coll_mins: [f32; 3],
    coll_maxs: [f32; 3],
    use_item_clip: bool,
    has_effect_on_impact: bool,
    elem_random_seed: u64,
    vel_graph_local: &[fx_iw4::FxElemVec3Range],
    vel_graph_world: &[fx_iw4::FxElemVec3Range],
    q: FxElemMotionQuery<'_>,
    mut trace: impl FnMut([f32; 3], [f32; 3], [f32; 3], [f32; 3], u32) -> FxElemTraceHit,
) -> Option<FxElemMotionResult> {
    if !elem_uses_collision(flags) {
        return evaluate_elem_motion(
            flags,
            gravity_base,
            gravity_amp,
            elem_random_seed,
            vel_graph_local,
            vel_graph_world,
            q,
        );
    }

    if q.at_rest_fraction != FX_ELEM_AT_REST_NONE {
        return Some(FxElemMotionResult {
            origin_delta: [0.0; 3],
            base_vel: q.base_vel,
            remove: false,
            at_rest_fraction: Some(q.at_rest_fraction),
            spawn_impact: None,
        });
    }

    let mask = trace_mask(use_item_clip);
    let mut origin = q.origin;
    let mut base_vel = q.base_vel;
    let mut total_delta = [0.0f32; 3];
    let mut remove = false;
    let mut spawn_impact = None;
    let mut at_rest_fraction = None;

    for step in collide_substep_schedule(q.prev_msec, q.msec_now) {
        let mut t0 = step.msec_start;
        let t_end = step.msec_end;

        for _ in 0..8 {
            if t0 >= t_end || remove || at_rest_fraction.is_some() {
                break;
            }
            let life_ms = q.life_ms.max(1.0);
            let age0 = ((t0.saturating_sub(q.msec_begin)).max(0) as f32) / life_ms;
            let age1 = ((t_end.saturating_sub(q.msec_begin)).max(0) as f32) / life_ms;
            let dt_sec = (t_end.saturating_sub(t0)).max(0) as f32 * 0.001;
            let sub_q = FxElemMotionQuery {
                def_name: q.def_name,
                catalog_index: q.catalog_index,
                def_index: q.def_index,
                age0,
                age1,
                life_ms,
                dt_sec,
                base_vel,
                elem_random_seed,
                origin,
                prev_msec: t0,
                msec_now: t_end,
                msec_begin: q.msec_begin,
                effect_axis: q.effect_axis,
                orient: q.orient,
                at_rest_fraction: q.at_rest_fraction,
            };
            let free = integrate_free_flight(
                flags,
                gravity_base,
                gravity_amp,
                elem_random_seed,
                vel_graph_local,
                vel_graph_world,
                sub_q,
            );
            let start_local = origin;
            let end_local = [
                start_local[0] + free.origin_delta[0],
                start_local[1] + free.origin_delta[1],
                start_local[2] + free.origin_delta[2],
            ];
            let (start, end) = (
                orientation_pos_to_world(q.orient.origin, q.orient.axis, start_local),
                orientation_pos_to_world(q.orient.origin, q.orient.axis, end_local),
            );
            let hit = trace(start, end, coll_mins, coll_maxs, mask);
            if hit.startsolid || hit.allsolid {
                base_vel = [0.0; 3];
                break;
            }
            if hit.fraction >= 1.0 {
                total_delta[0] += free.origin_delta[0];
                total_delta[1] += free.origin_delta[1];
                total_delta[2] += free.origin_delta[2];
                origin = end_local;
                base_vel = free.base_vel;
                break;
            }

            let hit_world = [
                start[0] + (end[0] - start[0]) * hit.fraction,
                start[1] + (end[1] - start[1]) * hit.fraction,
                start[2] + (end[2] - start[2]) * hit.fraction,
            ];
            let hit_pos = orientation_pos_from_world(q.orient.origin, q.orient.axis, hit_world);
            total_delta[0] += hit_pos[0] - start_local[0];
            total_delta[1] += hit_pos[1] - start_local[1];
            total_delta[2] += hit_pos[2] - start_local[2];
            origin = hit_pos;

            let age_at_hit_msec = (t0.saturating_sub(q.msec_begin)).max(0) as f32
                + (t_end.saturating_sub(t0)).max(0) as f32 * hit.fraction;
            let pre_vel = get_velocity_at_time(
                flags,
                base_vel,
                age_at_hit_msec,
                life_ms,
                vel_graph_local,
                vel_graph_world,
                q.effect_axis,
                elem_random_seed,
            );
            let pre_speed_sq = vec3_length_sq(pre_vel);
            if has_effect_on_impact && impact_child_speed_allows(pre_speed_sq) {
                spawn_impact = Some(FxImpactSpawn {
                    origin: hit_world,
                    pre_vel,
                });
            }

            if elem_dies_on_touch(flags) {
                remove = true;
                break;
            }

            let reflection =
                sample_reflection_factor(reflection_base, reflection_amp, elem_random_seed);
            let scaled = [
                pre_vel[0] * reflection,
                pre_vel[1] * reflection,
                pre_vel[2] * reflection,
            ];
            let hit_at_start = hit.fraction <= 0.0;
            if hit_at_start && collide_marks_at_rest(scaled, hit.normal[2]) {
                let recip = 1.0 / life_ms;
                let frac = get_at_rest_fraction(t0 as f32, q.msec_begin as f32, recip);
                let u = if frac < 0.0 {
                    0
                } else if frac > 255.0 {
                    255
                } else {
                    frac as u8
                };
                at_rest_fraction = Some(u);
                base_vel = [0.0; 3];
                break;
            }

            let dvel = collision_reflect_base_vel_delta(pre_vel, hit.normal, reflection);
            base_vel = [
                base_vel[0] + dvel[0],
                base_vel[1] + dvel[1],
                base_vel[2] + dvel[2],
            ];

            let span = (t_end.saturating_sub(t0)).max(0) as f32;
            t0 += (span * hit.fraction) as i32;
            if t0 <= step.msec_start {
                t0 = step.msec_start.saturating_add(1);
            }
        }
        if remove || at_rest_fraction.is_some() {
            break;
        }
    }

    Some(FxElemMotionResult {
        origin_delta: total_delta,
        base_vel,
        remove,
        at_rest_fraction,
        spawn_impact,
    })
}

fn integrate_free_flight(
    flags: i32,
    gravity_base: f32,
    gravity_amp: f32,
    elem_random_seed: u64,
    local_samples: &[fx_iw4::FxElemVec3Range],
    world_samples: &[fx_iw4::FxElemVec3Range],
    q: FxElemMotionQuery<'_>,
) -> FxElemMotionResult {
    let mut stored = q.origin;
    if elem_uses_vel_local(flags) && local_samples.len() >= 2 {
        let d =
            integrate_velocity_graph(local_samples, q.age0, q.age1, q.life_ms, elem_random_seed);
        stored[0] += d[0];
        stored[1] += d[1];
        stored[2] += d[2];
    }
    let mut world = orientation_pos_to_world(q.orient.origin, q.orient.axis, stored);
    if elem_uses_vel_world(flags) && world_samples.len() >= 2 {
        let d =
            integrate_velocity_graph(world_samples, q.age0, q.age1, q.life_ms, elem_random_seed);
        world[0] += d[0];
        world[1] += d[1];
        world[2] += d[2];
    }
    let mut base_vel = q.base_vel;
    if elem_update_has_velocity_graph(flags) {
        {
            let dt = q.dt_sec;
            let g = elem_gravity_accel_z_sampled(gravity_base, gravity_amp, elem_random_seed);
            world[0] += base_vel[0] * dt;
            world[1] += base_vel[1] * dt;
            world[2] += base_vel[2] * dt;
            world[2] -= g * dt * dt * 0.5;
            base_vel[2] -= g * dt;
        }
        stored = orientation_pos_from_world(q.orient.origin, q.orient.axis, world);
    }
    FxElemMotionResult {
        origin_delta: [
            stored[0] - q.origin[0],
            stored[1] - q.origin[1],
            stored[2] - q.origin[2],
        ],
        base_vel,
        remove: false,
        at_rest_fraction: None,
        spawn_impact: None,
    }
}
