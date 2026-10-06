#![no_std]
#![forbid(unsafe_code)]

mod equipment;
pub use equipment::{EquipmentRuntimeFacts, WEAPCLASS_THROWINGKNIFE};

mod ads_allow;
mod ads_overlay;
mod ammo;
pub mod event_sound;
mod fire_sound;
mod fire_weapon;
mod kick;
mod melee;
mod offhand;
mod penetration;
mod placement;
mod player_anim_type;
mod reload;
mod response;
mod spread;
mod sprint;
mod sway;
mod tick;
mod unlocated;
mod view_bob;
mod viewmodel;
mod viewweapon;
mod weap_anim;
mod weap_anim_rate;
mod weapon_change;
mod weaponcomplete;
mod weapondef;
mod weaponstate;

pub use ads_allow::{AdsAllowWeaponFacts, OTHER_FLAG_PLAYER, WEAP_FLAG_NO_ADS, is_ads_allowed};
pub use ads_overlay::{AdsOverlayScrub, ads_overlay_scrub};
pub use ammo::{
    AMMO_TABLE_BYTES, AMMOCLIP_TABLE_BYTES, WEAPON_DATA_BYTES, ammo_row_present, ammo_table_key,
    clip_row_present, clip_table_key, create_akimbo_viewmodel_trees, ensure_clip_row,
    get_ammo_index, get_ammo_not_in_clip, get_ammo_player_both_clips, get_clip_for_hand,
    get_clip_index, get_total_ammo_in_clips, get_weapon_dual_wield_byte,
    has_akimbo_viewmodel_anims, latch_weapon_dual_wield, num_hands, num_hands_for_held,
    player_weapons_find_slot, set_ammo_not_in_clip, set_clip_for_hand, set_weapon_dual_wield_byte,
    set_weapon_model_for_held, spend_clip_for_hand, weapon_model_for_held,
};
pub use event_sound::{
    EV_RELOAD, EV_RELOAD_END, EV_RELOAD_FROM_EMPTY, EV_RELOAD_START, begin_reload_event,
    play_note_mapped_rumble_alias, play_note_mapped_sound_alias, reload_insert_event,
    slot_for_event as weapondef_sound_slot,
};
pub use fire_sound::{select_cg_fire_sound_ptr, select_fire_last_sound_ptr, select_fire_sound_ptr};
pub use fire_weapon::{
    BULLET_MAX_RANGE, FireWeaponKind, HITLOC_COUNT, HITLOC_NAMES, LOCATION_DAMAGE_IDENTITY,
    ROCKET_SPREAD_PLANE, WEAPCLASS_GRENADE, WEAPCLASS_PISTOL, WEAPCLASS_SPREAD, WEAPCLASS_TURRET,
    WEAPTYPE_BULLET, WEAPTYPE_GRENADE, WEAPTYPE_PROJECTILE, WEAPTYPE_SHIELD, bake_location_damage,
    fire_weapon_kind, location_damage_is_valid, location_damage_scale,
};
pub use kick::{
    CAMERA_LIMIT_DEGREES, CAMERA_PEAK_SECONDS, FireRecoilImpulse, FireRecoilPsScales, GunKickRange,
    GunRecoilPlacementState, GunRecoilResponse, MODEL_PEAK_SECONDS, ViewKickRange,
    calculate_weapon_position_gun_recoil, fire_recoil_gun_range, fire_recoil_view_range,
    gun_recoil_angle_contribution, recoil_ads_weight, start_firing_restrict_kick_time,
    weapon_fire_recoil,
};
pub use melee::{
    BUTTON_MELEE, MELEE_TRACE_OFFSETS, MeleeChargeState, MeleeWeaponFacts,
    PLAYER_MELEE_HEIGHT_DEFAULT, PLAYER_MELEE_RANGE_DEFAULT, PLAYER_MELEE_WIDTH_DEFAULT,
    melee_charge_start, melee_trace_count, melee_trace_end, melee_weaponstate_blocks,
    weapon_advance_melee, weapon_has_charge_melee, weapon_melee_to_end, weapon_melee_to_fire,
    weapon_settle_ready, weapon_start_melee, weapon_start_melee_uses_charge, weapon_try_melee,
};
pub use offhand::{
    BUTTON_FRAG, BUTTON_SMOKE, CURSOR_HINT_NONE, OFFHAND_INV_SLOTS, OffhandCmd, OffhandInvRow,
    get_first_available_offhand, weapon_advance_offhand, weapon_check_for_offhand,
    weapon_enter_offhand, weapon_offhand_end, weapon_offhand_hold, weapon_offhand_prepare,
    weapon_offhand_start, weapon_offhand_throw, weapon_update_grenade_throw,
};
pub use penetration::{
    ADVANCE_DOT_MIN, ADVANCE_TRACE_FWD, ADVANCE_TRACE_REV, BulletPenFacts, CONTENTS_GLASS,
    MAX_EXTENDED_STEPS, MAX_PENETRATE_STEPS, PEN_SURF_TYPE_COUNT, PEN_THICKNESS_FLOOR,
    PENETRATE_TYPE_COUNT, PenetrationDepthTable, REV_END_EPS, RIFLE_COLLATERAL_SCALE,
    SURF_NOPENETRATE, SURF_TYPE_FLESH, SURFACE_TYPE_NAMES, advance_trace, depth_surface_type,
    split_pen_key,
};
pub use placement::{
    DUAL_WIELD_VIEW_MODEL_OFFSET_LEFT_SCALE, GUN_DAMAGE_ADS_HALF, GUN_DAMAGE_DEFLECT_MS,
    GUN_DAMAGE_OVERLAY_MIX, GUN_DAMAGE_RETURN_MS, PLACEMENT_ASSEMBLE_STEP_COUNT,
    StanceTransitionFadeGlobals, VIEWHEIGHT_TARGET_CROUCH, VIEWHEIGHT_TARGET_PRONE,
    WEAPON_BOB_AMP_DUCKED, WEAPON_BOB_AMP_PRONE, WEAPON_BOB_AMP_SPRINTING, WEAPON_BOB_AMP_STANDING,
    WEAPON_BOB_AMPLITUDE_BASE, WEAPON_BOB_AMPLITUDE_ROLL, WEAPON_BOB_LAG, WEAPON_BOB_MAX,
    WEAPON_BOB_UP_PHASE, WEAPON_IDLE_AMOUNT_DEFAULT, WEAPON_IDLE_FACTOR_LERP,
    WEAPON_IDLE_PITCH_FREQ, WEAPON_IDLE_ROLL_FREQ, WEAPON_IDLE_SIN_SCALE,
    WEAPON_IDLE_TIME_MS_SCALE, WEAPON_IDLE_YAW_FREQ, WeaponBobInputs, WeaponBobState,
    WeaponBobWaveformInputs, WeaponIdleInputs, WeaponMovementKinematics, WeaponMovementOfsInputs,
    WeaponPlacementAssembleStep, WeaponPlacementContribution, WeaponPlacementPsInputs,
    WeaponPlacementState, WeaponStanceStaticOfsInputs, apply_idle_sway_scale,
    base_stance_movement_angles, calculate_weapon_movement_bob,
    calculate_weapon_movement_bob_waveform, calculate_weapon_movement_targets,
    dual_wield_view_model_origin_add, placement_movement_channel_scale, stance_movement_lerp,
    stance_transition_fade, weapon_bob_add_to_angles, weapon_bob_ads_attenuation,
    weapon_bob_apply_ads_attenuation, weapon_bob_rotate_origin, weapon_bob_set_waveform,
    weapon_damage_kick_angles, weapon_idle_amount_speed, weapon_placement_apply_angles,
    weapon_placement_apply_origin, weapon_placement_assemble, weapon_placement_jump_land_ofs,
    weapon_stance_static_ofs,
};
pub use player_anim_type::{PLAYER_ANIM_TYPE_COUNT, PLAYER_ANIM_TYPE_NAMES};
pub use reload::{
    DualMagTimes, ReloadDelayedOutcome, reload_clip, reload_weaponstate_may_credit,
    weapon_allow_reload, weapon_arm_reload_add_delay, weapon_process_input_wants_reload,
    weapon_reload_delayed_action,
};
pub use response::{LookSway, RecoilAxis, SwayConfig};
pub use spread::{
    AIM_SPREAD_AIR_DECAY, AIM_SPREAD_AIR_VIEWCHANGE, AIM_SPREAD_MOVE_SPEED_THRESHOLD_DEFAULT,
    AIM_SPREAD_SCALE_MAX, AIM_SPREAD_TURN_SCALE, AimSpreadMotion, AimSpreadState,
    PERK_BULLETACCURACY, PERK_WEAP_SPREAD_MULTIPLIER_DEFAULT, PM_TYPE_NORMAL_LINKED, SHORT2ANGLE,
    SpreadCone, SpreadOverrideState, VIEWHEIGHT_CROUCH_SEAM, VIEWHEIGHT_PRONE,
    VIEWHEIGHT_PRONE_SPAN, VIEWHEIGHT_STAND_SPAN, WeaponAimSpreadDecayFacts, WeaponSpreadFacts,
    add_aim_spread_fire, adjust_aim_spread_scale, fire_weapon_spread_degrees,
    get_spread_for_weapon, perk_weap_spread_multiplier,
};
pub use sprint::{weapon_advance_sprint, weapon_check_for_sprint};
pub use sway::{
    SwayContribution, SwaySpringState, WeaponSwayParams, lerp_sway_params, sway_config,
    sway_contribution, sway_from_drive, sway_shellshock_landing_scale,
};
pub use tick::{
    AimAssistRanges, BURST_COOLDOWN_DEFAULT_MS, BUTTON_ATTACK, BUTTON_RELOAD, BUTTON_THROW,
    CHECK_FIRING_AMMO_DRY_FIRE_MS, CapturedCombatInput, MissingCombatFacts, PERK_FASTRELOAD,
    PERK_WEAP_RELOAD_MULTIPLIER_DEFAULT, WeaponCmd, WeaponCombatFacts, WeaponHandState,
    WeaponHostRules, WeaponTickEvent, get_weapon_fire_button, perk_fastreload_eligible,
    spawn_clip_stock, spawn_weapon_hand, weapon_hands, weapon_ordinary, weapon_time_adjust,
};
pub use view_bob::{
    BG_VIEW_KICK_MAX, BG_VIEW_KICK_MIN, BG_VIEW_KICK_SCALE, EFLAGS_TURRET_VEHICLE, LAND_DEFLECT_MS,
    LAND_END_MS, LAND_RETURN_MS, LAND_VIEW_DIP_FALL_IN, LAND_VIEW_DIP_MAX,
    PERK_LIGHTWEIGHT_VIEW_BOB_BIT, PERK_LIGHTWEIGHT_VIEW_BOB_SCALE, VIEW_BOB_AMP_DUCKED,
    VIEW_BOB_AMP_DUCKED_ADS, VIEW_BOB_AMP_PRONE, VIEW_BOB_AMP_SPRINTING, VIEW_BOB_AMP_STANDING,
    VIEW_BOB_AMP_STANDING_ADS, VIEW_BOB_MAX, VIEW_DAMAGE_DEFLECT_MS, VIEW_DAMAGE_RETURN_MS,
    VIEW_DAMAGE_UNDIRECTED, VIEW_ORG_BOB_Z_MIN_OFS, VIEWWEAPON_LAND_SCALE, ViewAngleBob,
    ViewAngleBobInputs, ViewDamageFeedback, ViewOrgBob, ViewOrgBobInputs, calc_view_bob_pitch,
    calc_view_bob_roll, crash_land_fall_height, crash_land_view_dip, damage_feedback_kick,
    land_origin_weight, land_origin_z, should_apply_view_org_bob, view_angle_bob, view_bob_cycle,
    view_damage_angles, view_kick_amplitude, view_org_bob, viewweapon_land_origin_z,
};
pub use viewmodel::get_viewmodel_weapon_index;
pub use viewweapon::{
    viewweapon_composed_world_forward, viewweapon_iron_ads_saves_composed_axis,
    viewweapon_save_gun_pitch_yaw, viewweapon_save_offset_movement, viewweapon_view_to_world_delta,
};
pub use weap_anim::{
    WEAP_ANIM_RESTART_BIT, continue_weapon_anim, set_fps_fire_anim, set_rechamber_anim,
    set_weap_anim, start_weapon_anim, weap_anim_event, weapon_idle_weap_anim,
};
pub use weap_anim_rate::{
    ACTION_GOAL_TIME_SECS, ACTIVE_GOAL_WEIGHT, ANIM_RATE_TABLE, AnimRateOffsets,
    IDLE_INTERRUPT_GOAL_TIME_SECS, INACTIVE_GOAL_WEIGHT, WEAP_ANIM_EVENT_MASK, WEAPON_ANIM_SLOTS,
    known_complete_rate_timer_offset, known_rate_timer_offset, playback_rate,
    slot_for_weap_anim_event, slot_uses_native_rate, weap_anim_extra,
};
pub use weapon_change::{
    PMF_CHANGE_BLOCK, begin_weapon_change, check_for_change_admits, finish_putaway_to_cmd,
    finish_putaway_while_holstered, traversal_forces_holster, weapon_check_for_change,
};
pub use weaponcomplete::WeaponCompleteDef;
pub use weapondef::WeaponDef;
pub use weaponstate::{
    FireType, WeaponDecodeError, WeaponState, viewmodel_rocket_should_be_attached,
};

pub const IW4_CAMOS: [&str; 9] = [
    "none",
    "woodland",
    "desert",
    "arctic",
    "digital",
    "red_urban",
    "red_tiger",
    "blue_tiger",
    "orange_fall",
];
