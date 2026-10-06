pub const BREATH_HOLD_TIME_MS: i32 = 4500;
pub const BREATH_GASP_TIME_MS: i32 = 1000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerState {
    pub command_time: i32,
    pub pm_type: i32,
    pub pm_time: i32,
    pub pm_flags: u32,
    pub other_flags: u32,
    pub link_flags: u32,
    pub bob_cycle: i32,
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    pub grenade_time_left: i32,
    pub throw_back_grenade_owner: i32,
    pub gravity: i32,
    pub leanf: f32,
    pub speed: i32,
    pub delta_angles: [f32; 3],
    pub ground_entity_num: i32,
    pub v_ladder_vec: [f32; 3],
    pub jump_time: i32,
    pub jump_origin_z: f32,
    pub legs_timer: i32,
    pub legs_anim: i32,
    pub torso_timer: i32,
    pub torso_anim: i32,
    pub damage_timer: i32,
    pub damage_duration: i32,
    pub flinch_yaw_anim: i32,
    pub corpse_index: i32,
    pub movement_dir: i32,
    pub e_flags: u32,
    pub viewlocked: i32,
    pub viewlocked_ent_num: i32,
    pub event_sequence: i32,
    pub events_0: i32,
    pub events_1: i32,
    pub events_2: i32,
    pub events_3: i32,
    pub event_parms_0: i32,
    pub event_parms_1: i32,
    pub event_parms_2: i32,
    pub event_parms_3: i32,
    pub old_event_sequence: i32,
    pub viewangles: [f32; 3],
    pub view_height_target: i32,
    pub view_height_current: f32,
    pub view_height_lerp_time: i32,
    pub view_height_lerp_target: i32,
    pub view_height_lerp_down: i32,
    pub damage_event: u32,
    pub damage_yaw: u32,
    pub damage_pitch: u32,
    pub damage_count: i32,
    pub damage_flags: u32,
    pub health: i32,
    pub max_health: i32,
    pub link_weapon_angles: [f32; 3],
    pub cursor_hint: i32,
    pub cursor_hint_string: i32,
    pub cursor_hint_ent_index: i32,
    pub cursor_hint_dual_wield: i32,
    pub sprint_button_up_required: i32,
    pub sprint_delay: i32,
    pub last_sprint_start: i32,
    pub last_sprint_end: i32,
    pub sprint_start_max_length: i32,
    pub move_speed_scale_multiplier: f32,
    pub mantle_yaw: f32,
    pub mantle_timer: i32,
    pub mantle_trans_index: i32,
    pub mantle_flags: u32,
    pub weap_anim: i32,
    pub weapon_time: i32,
    pub weapon_delay: i32,
    pub weapon_restrict_kick_time: i32,
    pub weaponstate_primary: i32,
    pub weap_hand_flags: i32,
    pub weapon_shot_count: i32,
    pub weap_anim_secondary: i32,
    pub weapon_time_secondary: i32,
    pub weapon_delay_secondary: i32,
    pub weapon_restrict_kick_time_secondary: i32,
    pub weaponstate_secondary: i32,
    pub weap_hand_flags_secondary: i32,
    pub weapon_shot_count_secondary: i32,
    pub weapons: [i32; 15],
    pub weapon_data: [u8; 76],
    pub off_hand_index: i32,
    pub offhand_primary: i32,
    pub offhand_secondary: i32,
    pub weapon: u32,
    pub weapon_primary: u32,
    pub weap_flags: u32,
    pub f_weapon_pos_frac: f32,
    pub hold_breath_timer: i32,
    pub hold_breath_scale: f32,
    pub aim_spread_scale: f32,
    pub ads_delay_time: i32,
    pub spread_override: i32,
    pub spread_override_state: i32,
    pub last_weapon_hand: i32,
    pub ammo: [u8; 0x78],
    pub ammoclip: [u8; 0xb4],
    pub melee_charge_yaw: f32,
    pub melee_charge_dist: i32,
    pub melee_charge_time: i32,
    pub perks: [u32; 2],
    pub perk_slots: [u32; 8],
    pub action_slot_type: [i32; 4],
    pub action_slot_param: [i32; 4],
    pub shellshock_index: i32,
    pub shellshock_time: i32,
    pub shellshock_duration: i32,
    pub stun_time: i32,
    pub objectives: [u8; 0x380],
    pub delta_time: i32,
    pub kill_cam_entity: i32,
    pub kill_cam_look_at_entity: i32,
    pub kill_cam_client_num: i32,
    pub recoil_scale: i32,
}

pub mod eflags {
    pub const TELEPORT: u32 = 0x2;

    pub const DUCK: u32 = 0x4;

    pub const PRONE: u32 = 0x8;

    pub const NODRAW: u32 = 0x20;

    pub const KILLCAM_PRESERVED: u32 = 0x80;

    pub const TURRET_ACTIVE_PRONE: u32 = 0x400;

    pub const TURRET_ACTIVE_DUCK: u32 = 0x800;

    pub const RADAR_JAM: u32 = 0x200000;
}

pub mod other_flags {
    pub const AC130: u32 = 0x8000;

    pub const EMP_JAMMED: u32 = 0x400;

    pub const DEAD_KILLCAM_TPV: u32 = 0x800;

    pub const PLAYER: u32 = 0x1000;
}

pub mod pm_flags {
    pub const PRONE: u32 = 0x1;

    pub const CROUCH: u32 = 0x2;

    pub const MANTLE: u32 = 0x4;

    pub const LADDER: u32 = 0x8;

    pub const ADS_INTENT: u32 = 0x10;

    pub const BACKWARDS_RUN: u32 = 0x20;

    pub const WALKING: u32 = 0x40;

    pub const TIME_HARDLANDING: u32 = 0x80;

    pub const PRONEMOVE_OVERRIDDEN: u32 = 0x200;

    pub const LADDER_FALL: u32 = 0x1000;

    pub const JUMPING: u32 = 0x2000;

    pub const SPRINTING: u32 = 0x4000;

    pub const SHELLSHOCKED: u32 = 0x8000;

    pub const MELEE_CHARGE: u32 = 0x10000;

    pub const SPRINT_BLOCKED: u32 = 0x0002_0000;

    pub const LAST_STAND: u32 = 0x0040_0000;
}

pub mod weap_flags {
    pub const HOLD_BREATH: u32 = 0x4;

    pub const NIGHT_VISION: u32 = 0x40;

    pub const OFFHAND_VIEW: u32 = 0x2;

    pub const NO_ADS: u32 = 0x20;

    pub const DOUBLEBARREL_RECOIL: u32 = 0x200;

    pub const RECOIL_SCALE: u32 = 0x400;
}

#[must_use]
pub fn get_viewmodel_weapon_index(ps: &PlayerState) -> u32 {
    if (ps.weap_flags & weap_flags::OFFHAND_VIEW) != 0 {
        u32::try_from(ps.off_hand_index).unwrap_or(0)
    } else {
        ps.weapon
    }
}

pub mod mantle_flags {
    pub const OVER: u32 = 1 << 0;

    pub const ACTIVE: u32 = 1 << 3;

    pub const QUICK: u32 = 1 << 4;
    pub const FAST_MANTLE: u32 = 1 << 6;
}

pub const ENTITYNUM_NONE: i32 = 0x7FF;

impl PlayerState {
    pub const ZERO: Self = Self {
        command_time: 0,
        pm_type: 0,
        pm_time: 0,
        pm_flags: 0,
        other_flags: 0,
        link_flags: 0,
        link_weapon_angles: [0.0; 3],
        bob_cycle: 0,
        origin: [0.0; 3],
        velocity: [0.0; 3],
        grenade_time_left: 0,
        throw_back_grenade_owner: 0,
        gravity: 0,
        leanf: 0.0,
        speed: 0,
        delta_angles: [0.0; 3],
        ground_entity_num: 0,
        v_ladder_vec: [0.0; 3],
        jump_time: 0,
        jump_origin_z: 0.0,
        legs_timer: 0,
        legs_anim: 0,
        torso_timer: 0,
        torso_anim: 0,
        damage_timer: 0,
        damage_duration: 0,
        flinch_yaw_anim: 0,
        corpse_index: 0,
        movement_dir: 0,
        e_flags: 0,
        viewlocked: 0,
        viewlocked_ent_num: ENTITYNUM_NONE,
        event_sequence: 0,
        events_0: 0,
        events_1: 0,
        events_2: 0,
        events_3: 0,
        event_parms_0: 0,
        event_parms_1: 0,
        event_parms_2: 0,
        event_parms_3: 0,
        old_event_sequence: 0,
        viewangles: [0.0; 3],
        view_height_target: 0,
        view_height_current: 0.0,
        view_height_lerp_time: 0,
        view_height_lerp_target: 0,
        view_height_lerp_down: 0,
        damage_event: 0,
        damage_yaw: 0,
        damage_pitch: 0,
        damage_count: 0,
        damage_flags: 0,
        health: 0,
        max_health: 0,
        cursor_hint: 0,
        cursor_hint_string: 0,
        cursor_hint_ent_index: 0,
        cursor_hint_dual_wield: 0,
        sprint_button_up_required: 0,
        sprint_delay: 0,
        last_sprint_start: 0,
        last_sprint_end: 0,
        sprint_start_max_length: 0,
        move_speed_scale_multiplier: 0.0,
        mantle_yaw: 0.0,
        mantle_timer: 0,
        mantle_trans_index: 0,
        mantle_flags: 0,
        weap_anim: 0,
        weapon_time: 0,
        weapon_delay: 0,
        weapon_restrict_kick_time: 0,
        weaponstate_primary: 0,
        weap_hand_flags: 0,
        weapon_shot_count: 0,
        weap_anim_secondary: 0,
        weapon_time_secondary: 0,
        weapon_delay_secondary: 0,
        weapon_restrict_kick_time_secondary: 0,
        weaponstate_secondary: 0,
        weap_hand_flags_secondary: 0,
        weapon_shot_count_secondary: 0,
        weapons: [0; 15],
        weapon_data: [0; 76],
        off_hand_index: 0,
        offhand_primary: 0,
        offhand_secondary: 0,
        weapon: 0,
        weapon_primary: 0,
        weap_flags: 0,
        f_weapon_pos_frac: 0.0,
        hold_breath_timer: 0,
        hold_breath_scale: 1.0,
        aim_spread_scale: 0.0,
        ads_delay_time: 0,
        spread_override: 0,
        spread_override_state: 0,
        last_weapon_hand: 0,
        ammo: [0; 0x78],
        ammoclip: [0; 0xb4],
        melee_charge_yaw: 0.0,
        melee_charge_dist: 0,
        melee_charge_time: 0,
        perks: [0; 2],
        perk_slots: [0; 8],
        action_slot_type: [0; 4],
        action_slot_param: [0; 4],
        shellshock_index: 0,
        shellshock_time: 0,
        shellshock_duration: 0,
        stun_time: 0,
        objectives: [0; 0x380],
        delta_time: 0,
        kill_cam_entity: 0,
        kill_cam_look_at_entity: 0,
        kill_cam_client_num: 0,
        recoil_scale: 0,
    };

    pub fn anim(&self) -> AnimPair {
        AnimPair {
            legs_anim: self.legs_anim,
            torso_anim: self.torso_anim,
        }
    }
}

pub const PERK_PISTOLDEATH: u32 = 1 << 7;

pub const PERK_QUIETER: u32 = 1 << 8;

pub const PERK_FASTMANTLE: u32 = 1 << 19;

pub const PERK_SCAVENGER: u32 = 1 << 22;

pub const PERK_HOLDBREATH: u32 = 1 << 4;

pub const PERK_EXTRA_BREATH_MS: i32 = 5000;

pub const PERK_QUICKDRAW: u32 = 1 << 26;

pub const PERK_QUICKDRAW_SPEED_SCALE: f32 = 1.5;

pub const PERK_COLDBLOODED: u32 = 1 << 27;

pub const PERK_HEARTBREAKER: u32 = 1 << 28;

pub const PERK1_SPYGAME: u32 = 1;

pub fn breath_hold_time_ms(ps: &PlayerState) -> i32 {
    if ps.perks[0] & PERK_HOLDBREATH != 0 {
        BREATH_HOLD_TIME_MS + PERK_EXTRA_BREATH_MS
    } else {
        BREATH_HOLD_TIME_MS
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnimPair {
    pub legs_anim: i32,

    pub torso_anim: i32,
}
