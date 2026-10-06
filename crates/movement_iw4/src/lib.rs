#![no_std]
#![forbid(unsafe_code)]

mod accelerate;
mod ads_frac;
mod ads_intent;
mod air;
mod breath;
mod check_prone;
mod cmdscale;
mod collision;
mod correct_solid;
mod crash;
mod dmgtimer;
mod drop_timers;
mod events;
mod footstep;
mod friction;
mod ground;
mod integrate;
mod is_in_air;
pub mod jump;
mod ladder;
pub mod mantle;
mod melee_charge;
pub mod penetration;
mod pml;
mod pmove;
pub mod recovery;
mod single;
mod slide;
mod snap;
mod sprint;
mod stance;
mod viewangles;
mod walk;

pub use accelerate::accelerate;
pub use ads_frac::{AdsFracContext, update_ads_frac};
pub use ads_intent::{AdsIntentContext, AdsIntentResult, BUTTON_ADS, update_ads_intent};
pub use air::{AirMoveContext, air_move};
pub use check_prone::{PRONE_CHECK_HEIGHT, PRONE_FEET_DIST, check_prone, player_prone_allowed};
pub use cmdscale::{CmdScaleWalkContext, cmd_scale_walk};
pub use collision::CollisionBackend;
pub use correct_solid::{CorrectSolidOutcome, correct_solid};
pub use crash::{crash_land, crash_land_fall_height, fall_damage};
pub use dmgtimer::{
    ANIM_MT_FLINCH_FORWARD, PLAYER_DMGTIMER_FLINCH_TIME_MS, PLAYER_DMGTIMER_MAX_TIME,
    PLAYER_DMGTIMER_MIN_SCALE, PLAYER_DMGTIMER_STUMBLE_TIME_MS, PLAYER_DMGTIMER_TIME_PER_POINT,
    damage_scale_walk, damage_window_open, update_damage_timer, walk_move_drop_damage_timer,
};
pub use drop_timers::drop_timers;
pub use events::{SequencedPlayerEvent, add_event, add_predictable_event, consume_player_events};
pub use footstep::{
    LADDER_SURFACE_FLAGS, LADDER_SURFACE_TYPE, SURFACE_TYPE_NAMES, bob_cycle_wrapped,
    footstep_event, footstep_event_type, footsteps_anim_move_type, footsteps_bob_cycle,
    get_bob_max_speed, ladder_footsteps, should_make_footsteps, surface_type_index,
    surface_type_name, surface_type_to_name,
};
pub use friction::friction;
pub use ground::complete_ground_trace;
pub use integrate::predict_integrate;
pub use is_in_air::is_in_air;
pub use jump::{JumpAnimation, JumpCheckContext, JumpCheckResult, JumpLaunchContext};
pub use ladder::{
    CheckLadderContext, LADDER_ATTRACT_SPEED, LADDER_JUMP_BLOCK_MS, LADDER_TRACE_DIST_AIR,
    LADDER_TRACE_DIST_WALK, LadderAttachBackend, LadderMoveContext, LadderTraceHit, SURF_LADDER,
    check_ladder_move, clear_ladder_flag, ladder_attract_velocity, ladder_move, set_ladder_flag,
};
pub use mantle::{
    CONTENTS_MANTLE, CreateAnimsMantleRootDelta, FlatMantleAnimLength, MANTLE_CHECK_RADIUS_DEFAULT,
    MANTLE_CHECK_RANGE_DEFAULT, MANTLE_CLEARANCE_MAXS_Z, MANTLE_FORWARD_DIST, MANTLE_FRONT_MAXS_Z,
    MANTLE_HALF_WIDTH, MANTLE_LEDGE_FLOOR_Z, MANTLE_LEDGE_HEIGHTS, MANTLE_OVER_FORWARD,
    MANTLE_PLAYER_RADIUS, MANTLE_VIEW_YAWCAP_DEFAULT, MANTLE_XANIM_NAMES, MANTLE_XANIM_NAMES_FR,
    MANTLE_XANIM_TREE_SIZE, MantleCapViewContext, MantleCapsuleTrace, MantleCheckContext,
    MantleFindLedgeContext, MantleFrontProbeCast, MantleLedgeBackend, MantleLedgeProbe,
    MantleLedgeProbeLog, MantleMoveContext, MantleResults, MantleRootDelta, MantleXAnimLength,
    SURF_MANTLE_ON_OR_OVER, SURF_MANTLE_OVER, ZeroMantleRootDelta,
};
pub use melee_charge::{
    MeleeChargeWeaponDelays, PLAYER_MELEE_RANGE_DEFAULT as MELEE_CHARGE_PLAYER_MELEE_RANGE_DEFAULT,
    calc_melee_charge_time, melee_charge_clear, melee_charge_move,
};
pub use pml::Pml;
pub use pmove::Pmove;
pub use single::{
    GroundTraceInput, MoveBounds, PmoveResult, PmoveSingle, PmoveSingleContext, pmove,
};
pub(crate) use slide::project_velocity;
pub use slide::{slide_move, step_slide_move};
pub use snap::{end_tick_velocity, snap_vector};
pub use sprint::{
    PERK_MARATHON, SprintContext, SprintResult, end_sprint, get_max_sprint_time,
    sprint_ending_buttons, sprint_forward_below_minimum, sprint_recharge_penalty_ms,
    sprint_start_interfering_buttons, sprint_time_remaining, update_sprint,
};
pub use stance::{
    CROUCH_MAXS_Z, PRONE_MAXS_Z, STAND_MAXS_Z, StanceChange, StanceSurface, stance_speed_scale,
    stance_surface_type, sync_stance_tail, update_stance_flags, update_stance_target,
    update_view_height, view_height, view_height_lerp_duration,
};
pub use viewangles::{ANGLE2SHORT, SHORT2ANGLE, ViewAngleClamp, update_view_angles};
pub use walk::{WalkMoveContext, walk_move};
