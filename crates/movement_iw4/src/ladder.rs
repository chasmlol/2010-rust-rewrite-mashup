use playerstate_iw4::{ENTITYNUM_NONE, PlayerState, UserCmd, pm_flags};

pub const LADDER_JUMP_BLOCK_MS: i32 = 300;

pub const LADDER_TRACE_DIST_AIR: f32 = 30.0;

pub const LADDER_TRACE_DIST_WALK: f32 = 8.0;

pub const LADDER_ATTRACT_SPEED: f32 = 50.0;

pub const SURF_LADDER: u32 = 0x8;

pub trait LadderAttachBackend {
    fn ladder_trace(
        &mut self,
        origin: [f32; 3],
        dir: [f32; 3],
        dist: f32,
    ) -> Option<LadderTraceHit>;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LadderTraceHit {
    pub fraction: f32,
    pub normal: [f32; 3],
    pub surface_flags: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CheckLadderContext {
    pub server_time: i32,

    pub walking: bool,

    pub forward_xy: [f32; 2],

    pub forwardmove: i8,
}

pub fn clear_ladder_flag(ps: &mut PlayerState) {
    if (ps.pm_flags & pm_flags::LADDER) != 0 {
        ps.pm_flags = (ps.pm_flags & !pm_flags::LADDER) | pm_flags::LADDER_FALL;
    }
}

pub fn set_ladder_flag(ps: &mut PlayerState) {
    ps.pm_flags |= pm_flags::LADDER;
}

fn ladder_stance_blocks_attach(ps: &PlayerState) -> bool {
    let h = ps.view_height_target;
    let token = if h == 0x16 {
        3
    } else if h == 0x28 {
        2
    } else if h == 0x0b {
        1
    } else {
        0
    };
    token == 1 || token == 3
}

pub fn check_ladder_move(
    ps: &mut PlayerState,
    context: CheckLadderContext,
    backend: &mut impl LadderAttachBackend,
) -> bool {
    if context.walking {
        ps.pm_flags &= !pm_flags::LADDER_FALL;
    }

    let early_ok =
        ps.pm_time == 0 || (ps.pm_flags & pm_flags::LADDER) != 0 || (ps.pm_flags & 0x180) == 0;
    if !early_ok {
        return false;
    }

    let fell_off_in_air =
        (ps.pm_flags & pm_flags::LADDER) != 0 && ps.ground_entity_num == ENTITYNUM_NONE;

    let (check_dir, tracedist) = if fell_off_in_air {
        (
            [
                -ps.v_ladder_vec[0],
                -ps.v_ladder_vec[1],
                -ps.v_ladder_vec[2],
            ],
            if context.walking {
                LADDER_TRACE_DIST_WALK
            } else {
                LADDER_TRACE_DIST_AIR
            },
        )
    } else {
        let mut dir = [context.forward_xy[0], context.forward_xy[1], 0.0];
        let len = libm::sqrtf(dir[0] * dir[0] + dir[1] * dir[1]);
        if len > 0.0 {
            dir[0] /= len;
            dir[1] /= len;
        }
        (
            dir,
            if context.walking {
                LADDER_TRACE_DIST_WALK
            } else {
                LADDER_TRACE_DIST_AIR
            },
        )
    };

    if ps.pm_type >= 8 {
        ps.ground_entity_num = ENTITYNUM_NONE;
        clear_ladder_flag(ps);
        return false;
    }

    if (ps.pm_flags & pm_flags::LADDER_FALL) != 0
        || ladder_stance_blocks_attach(ps)
        || context.server_time.wrapping_sub(ps.jump_time) < LADDER_JUMP_BLOCK_MS
    {
        clear_ladder_flag(ps);
        return false;
    }

    let Some(hit) = backend.ladder_trace(ps.origin, check_dir, tracedist) else {
        clear_ladder_flag(ps);
        return fell_off_in_air;
    };
    if hit.fraction >= 1.0
        || (hit.surface_flags & SURF_LADDER) == 0
        || (context.walking && context.forwardmove <= 0)
    {
        clear_ladder_flag(ps);
        return fell_off_in_air;
    }

    if (ps.pm_flags & pm_flags::LADDER) != 0 {
        set_ladder_flag(ps);
        return false;
    }

    ps.v_ladder_vec = hit.normal;
    let recheck = [
        -ps.v_ladder_vec[0],
        -ps.v_ladder_vec[1],
        -ps.v_ladder_vec[2],
    ];
    let Some(hit2) = backend.ladder_trace(ps.origin, recheck, tracedist) else {
        clear_ladder_flag(ps);
        return fell_off_in_air;
    };
    if hit2.fraction < 1.0 && (hit2.surface_flags & SURF_LADDER) != 0 {
        set_ladder_flag(ps);
        false
    } else {
        clear_ladder_flag(ps);
        fell_off_in_air
    }
}

pub fn ladder_attract_velocity(ps: &mut PlayerState) {
    let f_side = ps.velocity[0] * ps.v_ladder_vec[0] + ps.velocity[1] * ps.v_ladder_vec[1];
    ps.velocity[0] += -f_side * ps.v_ladder_vec[0];
    ps.velocity[1] += -f_side * ps.v_ladder_vec[1];
    if ps.velocity[0] == 0.0 && ps.velocity[1] == 0.0 && ps.velocity[2] == 0.0 {
        return;
    }
    let horiz = ps.velocity[0] * ps.velocity[0] + ps.velocity[1] * ps.velocity[1];
    let vert = ps.velocity[2] * ps.velocity[2];
    if vert >= horiz {
        ps.velocity[0] += -LADDER_ATTRACT_SPEED * ps.v_ladder_vec[0];
        ps.velocity[1] += -LADDER_ATTRACT_SPEED * ps.v_ladder_vec[1];
    }
}

const LADDER_UPSCALE_BIAS: f32 = 0.25;
const LADDER_UPSCALE_MUL: f32 = 2.5;

const LADDER_UPSCALE_MIN: f32 = -1.0;

const LADDER_CLIMB_WISH_SCALE: f32 = 0.5;

const LADDER_ACCEL: f32 = 9.0;

const LADDER_RIGHT_SCALE: f32 = 0.2;

const LADDER_SIDE_FRICTION: f32 = 16.0;

#[derive(Clone, Copy, Debug)]
pub struct LadderMoveContext {
    pub jump: crate::JumpLaunchContext,
    pub old_buttons: u32,
    pub player_spectate_speed_scale: f32,
}

pub fn ladder_move<C: crate::CollisionBackend>(
    ps: &mut PlayerState,
    pml: &mut crate::Pml,
    cmd: &mut UserCmd,
    context: LadderMoveContext,
    bounds: crate::MoveBounds,
    collision: &C,
) {
    use crate::{
        JumpCheckContext, JumpCheckResult, accelerate, air_move, jump, stance_surface_type,
        step_slide_move,
    };

    let gate = JumpCheckContext {
        time_since_jump: cmd.server_time.wrapping_sub(ps.jump_time),
        old_buttons: context.old_buttons,
        stance_surface_type: stance_surface_type(ps) as u8,
    };
    if let JumpCheckResult::Launched { .. } = jump::check(ps, pml, cmd, gate, context.jump) {
        air_move(
            ps,
            pml,
            cmd,
            crate::AirMoveContext {
                player_spectate_speed_scale: context.player_spectate_speed_scale,
                shellshock_gravity_scale: 1.0,
                shellshock_gravity_bias: 0.0,
            },
            bounds,
            collision,
        );
        return;
    }

    let mut upscale = (pml.forward[2] + LADDER_UPSCALE_BIAS) * LADDER_UPSCALE_MUL;
    if upscale > 1.0 {
        upscale = 1.0;
    } else if upscale < LADDER_UPSCALE_MIN {
        upscale = LADDER_UPSCALE_MIN;
    }

    pml.forward[2] = 0.0;
    let _ = normalize3(&mut pml.forward);
    let mut right = pml.right;
    right[2] = 0.0;
    let _ = normalize3(&mut right);

    let n = ps.v_ladder_vec;
    let dot = right[0] * n[0] + right[1] * n[1] + right[2] * n[2];
    right[0] -= dot * n[0];
    right[1] -= dot * n[1];
    right[2] -= dot * n[2];
    pml.right = right;

    let scale = ladder_cmd_scale(ps, cmd, context.player_spectate_speed_scale);
    let mut wishvel = [0.0_f32; 3];
    if cmd.forwardmove != 0 {
        wishvel[2] = LADDER_CLIMB_WISH_SCALE * upscale * scale * (cmd.forwardmove as f32);
    }
    if cmd.rightmove != 0 {
        let lateral = scale * LADDER_RIGHT_SCALE * (cmd.rightmove as f32);
        wishvel[0] += lateral * pml.right[0];
        wishvel[1] += lateral * pml.right[1];
        wishvel[2] += lateral * pml.right[2];
    }
    let mut wishdir = wishvel;
    let wishspeed = normalize3(&mut wishdir);
    accelerate(ps, pml, &wishdir, wishspeed, LADDER_ACCEL);

    if cmd.forwardmove == 0 {
        if ps.velocity[2] <= 0.0 {
            let next = ps.velocity[2] + (ps.gravity as f32) * pml.frametime;
            ps.velocity[2] = if next <= 0.0 { next } else { 0.0 };
        } else {
            let next = ps.velocity[2] - (ps.gravity as f32) * pml.frametime;
            ps.velocity[2] = if next >= 0.0 { next } else { 0.0 };
        }
    }

    if cmd.rightmove == 0 {
        ladder_side_friction(ps, pml);
    }

    if pml.walking == 0 {
        ladder_attract_velocity(ps);
    }

    step_slide_move(
        ps,
        pml,
        collision,
        bounds.mins,
        bounds.maxs,
        bounds.tracemask,
        None,
    );
}

fn ladder_side_friction(ps: &mut PlayerState, pml: &crate::Pml) {
    let mut side_dir = [pml.right[0], pml.right[1]];
    let len = libm::sqrtf(side_dir[0] * side_dir[0] + side_dir[1] * side_dir[1]);
    if len > 0.0 {
        side_dir[0] /= len;
        side_dir[1] /= len;
    }
    let side = ps.velocity[0] * side_dir[0] + ps.velocity[1] * side_dir[1];
    if side == 0.0 {
        return;
    }
    ps.velocity[0] -= side * side_dir[0];
    ps.velocity[1] -= side * side_dir[1];
    let mut drop = side * pml.frametime * LADDER_SIDE_FRICTION;
    if side.abs() <= drop.abs() {
        return;
    }
    if drop.abs() < 1.0 {
        drop = if drop >= 0.0 { 1.0 } else { -1.0 };
    }
    ps.velocity[0] += (side - drop) * side_dir[0];
    ps.velocity[1] += (side - drop) * side_dir[1];
}

fn normalize3(v: &mut [f32; 3]) -> f32 {
    let len = libm::sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    if len > 0.0 {
        let inv = 1.0 / len;
        v[0] *= inv;
        v[1] *= inv;
        v[2] *= inv;
    }
    len
}

fn ladder_cmd_scale(ps: &PlayerState, cmd: &UserCmd, spectate_speed_scale: f32) -> f32 {
    let forward = cmd.forwardmove as f32;
    let right = cmd.rightmove as f32;
    let magnitude = libm::sqrtf(forward * forward + right * right);
    let largest = forward.abs().max(right.abs());
    if largest == 0.0 {
        return 0.0;
    }
    let mut scale = (ps.speed as f32 * largest) / (magnitude * 127.0);
    if (ps.pm_flags & pm_flags::WALKING) != 0 || ps.leanf != 0.0 {
        scale *= 0.4;
    }
    scale *= match ps.pm_type {
        2 => 3.0,
        3 => 6.0,
        5 => spectate_speed_scale,
        _ => 1.0,
    };
    scale
}
