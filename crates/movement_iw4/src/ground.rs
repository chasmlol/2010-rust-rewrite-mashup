use playerstate_iw4::{ENTITYNUM_NONE, PlayerState};
use trace_iw4::Trace;

use crate::{CollisionBackend, GroundTraceInput, MoveBounds, Pml, jump};

const STARTSOLID_RETRY_EPSILON: f32 = 0.001;

pub fn complete_ground_trace<C: CollisionBackend>(
    ps: &mut PlayerState,
    pml: &mut Pml,
    bounds: MoveBounds,
    forwardmove: i8,
    collision: &C,
) {
    let origin = ps.origin;
    let mounted = (ps.e_flags & 0xc00) != 0;
    let input = GroundTraceInput {
        start: [
            origin[0],
            origin[1],
            origin[2] + if mounted { 0.0 } else { 0.25 },
        ],
        end: [
            origin[0],
            origin[1],
            origin[2] - if mounted { 1.0 } else { 0.25 },
        ],
        mins: bounds.mins,
        maxs: bounds.maxs,
        tracemask: bounds.tracemask,
    };

    let mut trace = collision.trace(input);
    copy_ground_trace(pml, &trace);

    if trace.allsolid != 0 {
        let Some(corrected) =
            collision.correct_solid(origin, bounds.mins, bounds.maxs, bounds.tracemask)
        else {
            clear_ground_state(ps, pml);
            jump::clear_state(ps);
            return;
        };
        ps.origin = corrected.origin;
        trace = corrected.trace;
        copy_ground_trace(pml, &trace);
    }

    if trace.startsolid != 0 {
        let retry = GroundTraceInput {
            start: [
                ps.origin[0],
                ps.origin[1],
                ps.origin[2] - STARTSOLID_RETRY_EPSILON,
            ],
            end: [
                ps.origin[0],
                ps.origin[1],
                ps.origin[2] - if mounted { 1.0 } else { 0.25 },
            ],
            ..input
        };
        trace = collision.trace(retry);
        if trace.startsolid != 0 {
            clear_ground_state(ps, pml);
            return;
        }
        copy_ground_trace(pml, &trace);
    }

    if trace.fraction == 1.0 {
        let was_grounded = ps.ground_entity_num != ENTITYNUM_NONE;
        let depth = if was_grounded { 64.0 } else { 1.0 };
        let below = collision.trace(GroundTraceInput {
            start: ps.origin,
            end: [ps.origin[0], ps.origin[1], ps.origin[2] - depth],
            ..input
        });
        if was_grounded && below.fraction == 1.0 {
            pml.record_jump_animation(jump_animation(forwardmove), true);
        }
        clear_ground_state(ps, pml);
        pml.almost_ground_plane = u32::from(if was_grounded {
            below.fraction < 1.0 / 64.0
        } else {
            below.fraction != 1.0
        });
        return;
    }

    let velocity_dot_normal = ps.velocity[2] * trace.normal[2]
        + trace.normal[0] * ps.velocity[0]
        + ps.velocity[1] * trace.normal[1];
    if (ps.pm_flags & 8) == 0 && ps.velocity[2] > 0.0 && 10.0 < velocity_dot_normal {
        pml.record_jump_animation(jump_animation(forwardmove), false);
        clear_ground_state(ps, pml);
        return;
    }

    if trace.walkable != 0 && trace.contents != 0x0200_0000 {
        pml.ground_plane = 1;
        pml.almost_ground_plane = 1;
        pml.walking = 1;
        if ps.ground_entity_num == ENTITYNUM_NONE {
            crate::crash_land(ps, pml);
        }
        let entity = trace_entity_id(&trace);
        ps.ground_entity_num = entity;
        collision.touch_entity(entity);
    } else {
        ps.ground_entity_num = ENTITYNUM_NONE;
        pml.ground_plane = 1;
        pml.almost_ground_plane = 1;
        pml.walking = 0;
        jump::clear_state(ps);
    }
}

fn clear_ground_state(ps: &mut PlayerState, pml: &mut Pml) {
    ps.ground_entity_num = ENTITYNUM_NONE;
    pml.ground_plane = 0;
    pml.almost_ground_plane = 0;
    pml.walking = 0;
}

fn copy_ground_trace(pml: &mut Pml, trace: &Trace) {
    pml.ground_trace = [
        trace.fraction.to_bits(),
        trace.normal[0].to_bits(),
        trace.normal[1].to_bits(),
        trace.normal[2].to_bits(),
        trace.surface_flags,
        trace.contents,
        trace.material,
        trace.hit_type as u32,
        u32::from(trace.hit_id) | (u32::from(trace.model_index) << 16),
        u32::from(trace.part_name) | (u32::from(trace.part_group) << 16),
        u32::from(trace.allsolid)
            | (u32::from(trace.startsolid) << 8)
            | (u32::from(trace.walkable) << 16),
    ];
}

fn trace_entity_id(trace: &Trace) -> i32 {
    i32::from(trace_iw4::trace_get_entity_hit_id(
        trace.hit_type,
        trace.hit_id,
    ))
}

fn jump_animation(forwardmove: i8) -> crate::JumpAnimation {
    if forwardmove < 0 {
        crate::JumpAnimation::Backward
    } else {
        crate::JumpAnimation::Forward
    }
}
