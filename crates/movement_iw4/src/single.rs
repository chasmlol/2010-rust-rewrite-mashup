use playerstate_iw4::{ENTITYNUM_NONE, PM_TYPE_NORMAL_LINKED, PlayerState, UserCmd};

use crate::{
    AdsFracContext, AdsIntentContext, AirMoveContext, CheckLadderContext, CollisionBackend,
    LadderAttachBackend, LadderMoveContext, LadderTraceHit, MantleCapViewContext,
    MantleCapsuleTrace, MantleCheckContext, MantleFindLedgeContext, MantleMoveContext,
    MantleRootDelta, MantleXAnimLength, MeleeChargeWeaponDelays, Pml, SprintContext,
    ViewAngleClamp, WalkMoveContext, air_move, calc_melee_charge_time, check_ladder_move,
    complete_ground_trace, drop_timers, end_tick_velocity, footstep_event, footsteps_bob_cycle,
    ladder_footsteps, ladder_move, mantle, melee_charge_move, should_make_footsteps,
    sync_stance_tail, update_ads_frac, update_ads_intent, update_sprint, update_stance_flags,
    update_stance_target, update_view_angles, update_view_height, walk_move,
};
use playerstate_iw4::pm_flags;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundTraceInput {
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub tracemask: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveBounds {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub tracemask: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct PmoveResult {
    pub pml: Pml,

    pub bounds: MoveBounds,
    pub stance_event: Option<u8>,
    pub reset_torso: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct PmoveSingleContext {
    pub walk: WalkMoveContext,

    pub air: AirMoveContext,

    pub bounds: MoveBounds,

    pub view_angles: ViewAngleClamp,

    pub sprint: SprintContext,

    pub ads_intent: AdsIntentContext,

    pub ads_frac: AdsFracContext,

    pub melee_charge: MeleeChargeWeaponDelays,

    pub player_melee_range: f32,

    pub old_buttons: u32,

    pub weapon_blocks_prone: bool,

    pub can_hold_breath: bool,
}

#[allow(clippy::too_many_lines)]
pub fn pmove<C: CollisionBackend, L: MantleXAnimLength, R: MantleRootDelta>(
    ps: &mut PlayerState,
    cmd: &mut UserCmd,
    context: PmoveSingleContext,
    collision: &C,
    lengths: &L,
    root: &R,
) -> PmoveResult {
    let msec = clamped_msec(cmd.server_time.wrapping_sub(ps.command_time));
    ps.command_time = cmd.server_time;

    let mut pml = Pml {
        forward: [0.0; 3],
        right: [0.0; 3],
        up: [0.0; 3],
        frametime: (msec as f32) * 0.001_f32,
        msec,
        walking: 0,
        ground_plane: 0,
        almost_ground_plane: 0,
        ground_trace: [0; 11],
        previous_origin: ps.origin,
        previous_velocity: ps.velocity,
        holdrand: holdrand(ps.viewangles[1], cmd.server_time),
        jump_animations: [None; 4],
        mantle_movetype: None,
        landing_animation: false,
        fall_damage: 0,
    };

    update_view_angles(ps, cmd, context.view_angles);

    let (forward, right, up) = math_iw4::angle_vectors(ps.viewangles);
    pml.forward = forward;
    pml.right = right;
    pml.up = up;

    mantle::clear_hint(ps);

    let _ads = update_ads_intent(ps, cmd, context.old_buttons, context.ads_intent);
    let mut sprint = context.sprint;
    if ps.pm_flags & 3 != 0 {
        sprint.stand_up_clear = collision
            .trace(GroundTraceInput {
                start: ps.origin,
                end: ps.origin,
                mins: context.bounds.mins,
                maxs: context.bounds.maxs,
                tracemask: context.bounds.tracemask & !0x0200_0000,
            })
            .allsolid
            == 0;
    }
    let sprint_change = update_sprint(ps, cmd, context.old_buttons, sprint);
    let previous_stance = ps.pm_flags & 3;
    update_stance_flags(
        ps,
        cmd,
        collision,
        context.bounds,
        context.weapon_blocks_prone,
    );
    let stance_event = if ps.pm_flags & pm_flags::SPRINTING == 0 {
        match (previous_stance, ps.pm_flags & 3) {
            (1, 0) => Some(16),
            (1, 2) => Some(12),
            (2, 0) => Some(14),
            (0, 2) => Some(13),
            _ => None,
        }
    } else {
        None
    };
    let stance_change = update_stance_target(ps);
    if stance_change == crate::StanceChange::EnteredProne
        && ps.pm_flags & pm_flags::LAST_STAND == 0
        && ps.pm_time == 0
    {
        ps.pm_flags |= pm_flags::JUMPING;
        ps.pm_time = 1800;
    }
    let reset_torso = stance_change != crate::StanceChange::Unchanged
        || sprint_change == crate::SprintResult::Started;
    update_view_height(ps, &pml, cmd);
    let mut bounds = context.bounds;
    bounds.maxs[2] = sync_stance_tail(ps);

    update_ads_frac(ps, pml.msec, context.ads_frac);
    crate::breath::update_hold_breath(ps, cmd.buttons, pml.msec, context.can_hold_breath);

    if ps.pm_type == PM_TYPE_NORMAL_LINKED {
        // The trigger link owns the origin: no walk, no air move, no jump, and
        // velocity zeroed every pmove. Viewangles and stance stay live above,
        // and the weapon ticks separately.
        ps.pm_flags &= !pm_flags::LADDER;
        ps.ground_entity_num = ENTITYNUM_NONE;
        ps.velocity = [0.0; 3];
        drop_timers(ps, &pml);
        return PmoveResult {
            pml,
            bounds,
            stance_event,
            reset_torso,
        };
    }

    if (ps.pm_flags & pm_flags::MANTLE) == 0 {
        // Mantle root motion owns the path through the ledge. Ground solid
        // correction during that path would push the player back off it.
        complete_ground_trace(ps, &mut pml, bounds, cmd.forwardmove, collision);
        let mut mantle_tracer = CollisionMantleTrace { collision, bounds };
        let _ = mantle::check(
            ps,
            MantleCheckContext {
                find: MantleFindLedgeContext::default(),
                buttons: cmd.buttons,
                forwardmove: cmd.forwardmove,
                facing_xy: [pml.forward[0], pml.forward[1]],
                tracemask: bounds.tracemask,
            },
            &mut mantle_tracer,
            lengths,
            root,
        );
    }

    if (ps.pm_flags & pm_flags::MANTLE) != 0 {
        mantle::cap_view(ps, MantleCapViewContext::default());
        mantle::advance(ps, pml.msec, MantleMoveContext::default(), lengths, root);
        pml.mantle_movetype = Some((mantle::active_xanim(ps, lengths) + 21) as u8);
        return PmoveResult {
            pml,
            bounds,
            stance_event,
            reset_torso,
        };
    }

    drop_timers(ps, &pml);

    {
        let mut ladder_backend = CollisionLadderBackend { collision, bounds };
        if check_ladder_move(
            ps,
            CheckLadderContext {
                server_time: cmd.server_time,
                walking: pml.walking != 0,
                forward_xy: [pml.forward[0], pml.forward[1]],
                forwardmove: cmd.forwardmove,
            },
            &mut ladder_backend,
        ) {
            pml.record_jump_animation(crate::JumpAnimation::Forward, true);
        }
    }

    if (ps.pm_flags & pm_flags::LADDER) != 0 {
        ladder_move(
            ps,
            &mut pml,
            cmd,
            LadderMoveContext {
                jump: context.walk.jump,
                old_buttons: context.old_buttons,
                player_spectate_speed_scale: context.air.player_spectate_speed_scale,
            },
            bounds,
            collision,
        );
    } else {
        calc_melee_charge_time(ps, context.melee_charge, context.player_melee_range);
        if (ps.pm_flags & pm_flags::MELEE_CHARGE) != 0 {
            melee_charge_move(
                ps,
                &pml,
                bounds.mins,
                bounds.maxs,
                bounds.tracemask,
                collision,
            );
        } else if pml.walking == 0 {
            air_move(ps, &pml, cmd, context.air, bounds, collision);
        } else {
            walk_move(ps, &mut pml, cmd, context.walk, bounds, collision);
        }
    }

    complete_ground_trace(ps, &mut pml, bounds, cmd.forwardmove, collision);

    if (ps.pm_flags & pm_flags::LADDER) != 0 {
        ladder_footsteps(ps, pml.msec, cmd.server_time);
    } else {
        let old_bob = ps.bob_cycle as u8;
        footsteps_bob_cycle(
            ps,
            pml.msec,
            cmd.forwardmove,
            cmd.rightmove,
            pml.almost_ground_plane != 0,
            cmd.server_time,
            context.walk.cmd_scale,
        );
        footstep_event(
            ps,
            old_bob,
            ps.bob_cycle as u8,
            pml.ground_trace[4],
            should_make_footsteps(ps),
        );
    }

    end_tick_velocity(ps, &pml);
    PmoveResult {
        pml,
        bounds,
        stance_event,
        reset_torso,
    }
}

struct CollisionMantleTrace<'a, C: CollisionBackend> {
    collision: &'a C,
    bounds: MoveBounds,
}

impl<C: CollisionBackend> MantleCapsuleTrace for CollisionMantleTrace<'_, C> {
    fn trace(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        contentmask: u32,
    ) -> trace_iw4::Trace {
        let _ = self.bounds;
        self.collision.trace(GroundTraceInput {
            start,
            end,
            mins,
            maxs,
            tracemask: contentmask,
        })
    }
}

struct CollisionLadderBackend<'a, C: CollisionBackend> {
    collision: &'a C,
    bounds: MoveBounds,
}

impl<C: CollisionBackend> LadderAttachBackend for CollisionLadderBackend<'_, C> {
    fn ladder_trace(
        &mut self,
        origin: [f32; 3],
        dir: [f32; 3],
        dist: f32,
    ) -> Option<LadderTraceHit> {
        let end = [
            origin[0] + dir[0] * dist,
            origin[1] + dir[1] * dist,
            origin[2] + dir[2] * dist,
        ];
        let mut mins = self.bounds.mins;
        let mut maxs = self.bounds.maxs;
        mins[0] += 6.0;
        mins[1] += 6.0;
        mins[2] = 8.0;
        maxs[0] -= 6.0;
        maxs[1] -= 6.0;
        if maxs[2] < 8.0 {
            maxs[2] = mins[2];
        }
        let hit = self.collision.trace(GroundTraceInput {
            start: origin,
            end,
            mins,
            maxs,
            tracemask: self.bounds.tracemask,
        });
        if hit.fraction >= 1.0 {
            return None;
        }
        Some(LadderTraceHit {
            fraction: hit.fraction,
            normal: hit.normal,
            surface_flags: hit.surface_flags,
        })
    }
}

pub use pmove as PmoveSingle;

fn clamped_msec(delta: i32) -> i32 {
    delta.clamp(1, 200)
}

fn holdrand(view_yaw: f32, server_time: i32) -> i32 {
    let mut value = view_yaw.to_bits().wrapping_add(server_time as u32);

    for _ in 0..4 {
        value = value.wrapping_mul(0x343fdu32).wrapping_add(0x269ec3u32);
    }
    value as i32
}
