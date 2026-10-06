use playerstate_iw4::{ENTITYNUM_NONE, PlayerState, pm_flags};

use crate::{CollisionBackend, GroundTraceInput, Pml, jump};

#[path = "contact_solver.rs"]
mod contact_solver;

use contact_solver::{Contact, Motion, Settings};

fn settings(ps: &PlayerState, pml: &Pml, gravity: Option<f32>) -> Settings {
    let ground = (pml.ground_plane != 0)
        .then(|| [1, 2, 3].map(|index| f32::from_bits(pml.ground_trace[index])));
    let stance_height: f32 = if ps.pm_flags & pm_flags::PRONE != 0 {
        10.0
    } else {
        18.0
    };
    let ladder = ps.pm_flags & pm_flags::LADDER != 0;
    let supported = ps.ground_entity_num != ENTITYNUM_NONE;
    let step_height = if supported || (ladder && ps.velocity[2] > 0.0) {
        stance_height
    } else if ps.pm_flags & pm_flags::JUMPING != 0 {
        jump::get_step_height(ps, ps.origin)
            .unwrap_or(0.0)
            .min(stance_height)
    } else {
        0.0
    };
    Settings {
        dt: pml.frametime,
        gravity,
        ground,
        step_height,
        snap_down: if ground.is_some() && !ladder {
            9.0
        } else {
            0.0
        },
        landing_normal_z: 0.3,
    }
}

fn trace_callback<C: CollisionBackend>(
    collision: &C,
    mins: [f32; 3],
    maxs: [f32; 3],
    tracemask: u32,
) -> impl Fn([f32; 3], [f32; 3]) -> Contact + '_ {
    move |start, end| {
        let hit = collision.trace(GroundTraceInput {
            start,
            end,
            mins,
            maxs,
            tracemask,
        });
        Contact {
            fraction: hit.fraction,
            end: hit.endpos,
            normal: hit.normal,
            blocked: hit.allsolid != 0,
            walkable: hit.walkable != 0,
        }
    }
}

pub fn slide_move<C: CollisionBackend>(
    ps: &mut PlayerState,
    pml: &Pml,
    collision: &C,
    mins: [f32; 3],
    maxs: [f32; 3],
    tracemask: u32,
    gravity: Option<f32>,
) -> bool {
    let motion = Motion {
        origin: ps.origin,
        velocity: ps.velocity,
    };
    let (result, obstructed) = contact_solver::slide(
        motion,
        settings(ps, pml, gravity),
        &trace_callback(collision, mins, maxs, tracemask),
    );
    ps.origin = result.origin;
    ps.velocity = result.velocity;
    obstructed
}

pub fn step_slide_move<C: CollisionBackend>(
    ps: &mut PlayerState,
    pml: &Pml,
    collision: &C,
    mins: [f32; 3],
    maxs: [f32; 3],
    tracemask: u32,
    gravity: Option<f32>,
) {
    let ladder = ps.pm_flags & pm_flags::LADDER != 0;
    if ladder || (pml.ground_plane == 0 && ps.pm_time != 0) {
        jump::clear_state(ps);
    }
    let motion = Motion {
        origin: ps.origin,
        velocity: ps.velocity,
    };
    let result = contact_solver::traverse(
        motion,
        settings(ps, pml, gravity),
        &trace_callback(collision, mins, maxs, tracemask),
    );
    ps.origin = result.origin;
    ps.velocity = result.velocity;
}

pub(crate) fn project_velocity(velocity: &mut [f32; 3], normal: &[f32; 3]) {
    contact_solver::project_ground(velocity, normal);
}
