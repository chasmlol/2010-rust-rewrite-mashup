use playerstate_iw4::PlayerState;

use crate::transport::wire::{WireError, WireReader, WireWriter};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Replication {
    Replicated,

    Excluded(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Validation {
    Exact,

    Tolerance { epsilon: f32, reason: &'static str },

    AdoptOnly(&'static str),
}

#[derive(Clone, Copy, Debug)]
pub struct PsNetField {
    pub rust_name: &'static str,

    pub replication: Replication,

    pub validation: Validation,
}

impl PsNetField {
    pub const fn is_replicated(&self) -> bool {
        matches!(self.replication, Replication::Replicated)
    }

    pub const fn is_validated(&self) -> bool {
        !matches!(self.validation, Validation::AdoptOnly(_))
    }
}

macro_rules! ps_netfields {
    ($($rust:ident : $kind:ident = $rep:expr, $val:expr;)*) => {
        pub const PS_NETFIELDS: &[PsNetField] = &[
            $(PsNetField {
                rust_name: stringify!($rust),
                replication: $rep,
                validation: $val,
            },)*
        ];

        pub(crate) fn field_differs(a: &PlayerState, b: &PlayerState, index: usize) -> bool {
            let mut i = 0usize;
            $(
                if i == index {
                    return differs_by_bits!($kind, a.$rust, b.$rust);
                }
                i += 1;
            )*
            let _ = i;
            false
        }

        pub(crate) fn field_distance(a: &PlayerState, b: &PlayerState, index: usize) -> f32 {
            let mut i = 0usize;
            $(
                if i == index {
                    return distance_by_kind!($kind, a.$rust, b.$rust);
                }
                i += 1;
            )*
            let _ = i;
            0.0
        }

        pub(crate) fn write_field(out: &mut WireWriter, ps: &PlayerState, index: usize) {
            let mut i = 0usize;
            $(
                if i == index {
                    write_by_kind!($kind, out, ps.$rust);
                    return;
                }
                i += 1;
            )*
            let _ = i;
        }

        pub(crate) fn read_field(
            input: &mut WireReader<'_>,
            ps: &mut PlayerState,
            index: usize,
        ) -> Result<(), WireError> {
            let mut i = 0usize;
            $(
                if i == index {
                    read_by_kind!($kind, input, ps.$rust);
                    return Ok(());
                }
                i += 1;
            )*
            let _ = i;
            Err(WireError::UnknownField(index))
        }
    };
}

macro_rules! differs_by_bits {
    (i32, $a:expr, $b:expr) => {
        $a != $b
    };
    (u32, $a:expr, $b:expr) => {
        $a != $b
    };
    (f32, $a:expr, $b:expr) => {
        $a.to_bits() != $b.to_bits()
    };
    (vec3, $a:expr, $b:expr) => {
        $a[0].to_bits() != $b[0].to_bits()
            || $a[1].to_bits() != $b[1].to_bits()
            || $a[2].to_bits() != $b[2].to_bits()
    };
    (i32array, $a:expr, $b:expr) => {
        $a != $b
    };
    (u32x2, $a:expr, $b:expr) => {
        $a != $b
    };
    (u32x8, $a:expr, $b:expr) => {
        $a != $b
    };
    (opaque, $a:expr, $b:expr) => {
        $a != $b
    };
}

macro_rules! distance_by_kind {
    (i32, $a:expr, $b:expr) => {
        ($a as f32 - $b as f32).abs()
    };
    (u32, $a:expr, $b:expr) => {
        ($a as f32 - $b as f32).abs()
    };
    (f32, $a:expr, $b:expr) => {
        ($a - $b).abs()
    };
    (vec3, $a:expr, $b:expr) => {{
        let mut worst = 0.0f32;
        for axis in 0..3 {
            let d = ($a[axis] - $b[axis]).abs();
            if d > worst {
                worst = d;
            }
        }
        worst
    }};
    (i32array, $a:expr, $b:expr) => {
        if $a == $b { 0.0f32 } else { f32::INFINITY }
    };
    (u32x2, $a:expr, $b:expr) => {
        if $a == $b { 0.0f32 } else { f32::INFINITY }
    };
    (u32x8, $a:expr, $b:expr) => {
        if $a == $b { 0.0f32 } else { f32::INFINITY }
    };
    (opaque, $a:expr, $b:expr) => {
        if $a == $b { 0.0f32 } else { f32::INFINITY }
    };
}

macro_rules! write_by_kind {
    (i32, $out:expr, $v:expr) => {
        $out.put_i32($v)
    };
    (u32, $out:expr, $v:expr) => {
        $out.put_u32($v)
    };
    (f32, $out:expr, $v:expr) => {
        $out.put_f32($v)
    };
    (vec3, $out:expr, $v:expr) => {
        for component in $v {
            $out.put_f32(component);
        }
    };
    (i32array, $out:expr, $v:expr) => {
        for slot in $v {
            $out.put_i32(slot);
        }
    };
    (u32x2, $out:expr, $v:expr) => {
        for slot in $v {
            $out.put_u32(slot);
        }
    };
    (u32x8, $out:expr, $v:expr) => {
        for slot in $v {
            $out.put_u32(slot);
        }
    };
    (opaque, $out:expr, $v:expr) => {
        $out.put_bytes(&$v)
    };
}

macro_rules! read_by_kind {
    (i32, $input:expr, $slot:expr) => {
        $slot = $input.get_i32()?
    };
    (u32, $input:expr, $slot:expr) => {
        $slot = $input.get_u32()?
    };
    (f32, $input:expr, $slot:expr) => {
        $slot = $input.get_f32()?
    };
    (vec3, $input:expr, $slot:expr) => {
        for axis in 0..3 {
            $slot[axis] = $input.get_f32()?;
        }
    };
    (i32array, $input:expr, $slot:expr) => {
        for value in &mut $slot {
            *value = $input.get_i32()?;
        }
    };
    (u32x2, $input:expr, $slot:expr) => {
        for slot in 0..2 {
            $slot[slot] = $input.get_u32()?;
        }
    };
    (u32x8, $input:expr, $slot:expr) => {
        for slot in 0..8 {
            $slot[slot] = $input.get_u32()?;
        }
    };
    (opaque, $input:expr, $slot:expr) => {
        $input.get_bytes(&mut $slot)?
    };
}

ps_netfields! {
    command_time: i32 = Replication::Replicated, Validation::Exact;
    pm_type: i32 = Replication::Replicated, Validation::Exact;
    pm_time: i32 = Replication::Replicated, Validation::Exact;
    pm_flags: u32 = Replication::Replicated, Validation::Exact;
    other_flags: u32 = Replication::Replicated, Validation::Exact;
    link_flags: u32 = Replication::Replicated, Validation::Exact;
    bob_cycle: i32 = Replication::Replicated, Validation::Exact;
    origin: vec3 = Replication::Replicated, Validation::Exact;
    velocity: vec3 = Replication::Replicated, Validation::Exact;
    grenade_time_left: i32 = Replication::Replicated, Validation::Exact;
    throw_back_grenade_owner: i32 = Replication::Replicated, Validation::Exact;
    gravity: i32 = Replication::Replicated, Validation::Exact;
    leanf: f32 = Replication::Replicated, Validation::Exact;
    speed: i32 = Replication::Replicated, Validation::Exact;
    delta_angles: vec3 = Replication::Replicated, Validation::Exact;
    ground_entity_num: i32 = Replication::Replicated, Validation::Exact;
    v_ladder_vec: vec3 = Replication::Replicated, Validation::Exact;
    jump_time: i32 = Replication::Replicated, Validation::Exact;
    jump_origin_z: f32 = Replication::Replicated, Validation::Exact;
    legs_timer: i32 = Replication::Replicated, Validation::Exact;
    legs_anim: i32 = Replication::Replicated, Validation::Exact;
    torso_timer: i32 = Replication::Replicated, Validation::Exact;
    torso_anim: i32 = Replication::Replicated, Validation::Exact;
    damage_timer: i32 = Replication::Replicated, Validation::Exact;
    damage_duration: i32 = Replication::Replicated, Validation::Exact;
    flinch_yaw_anim: i32 = Replication::Replicated, Validation::Exact;
    corpse_index: i32 = Replication::Replicated, Validation::Exact;
    movement_dir: i32 = Replication::Replicated, Validation::Exact;
    e_flags: u32 = Replication::Replicated, Validation::Exact;
    event_sequence: i32 = Replication::Replicated, Validation::Exact;
    events_0: i32 = Replication::Replicated, Validation::Exact;
    events_1: i32 = Replication::Replicated, Validation::Exact;
    events_2: i32 = Replication::Replicated, Validation::Exact;
    events_3: i32 = Replication::Replicated, Validation::Exact;
    event_parms_0: i32 = Replication::Replicated, Validation::Exact;
    event_parms_1: i32 = Replication::Replicated, Validation::Exact;
    event_parms_2: i32 = Replication::Replicated, Validation::Exact;
    event_parms_3: i32 = Replication::Replicated, Validation::Exact;
    old_event_sequence: i32 = Replication::Replicated, Validation::Exact;
    viewangles: vec3 = Replication::Replicated, Validation::Exact;
    view_height_target: i32 = Replication::Replicated, Validation::Exact;
    view_height_current: f32 = Replication::Replicated, Validation::Exact;
    view_height_lerp_time: i32 = Replication::Replicated, Validation::Exact;
    view_height_lerp_target: i32 = Replication::Replicated, Validation::Exact;
    view_height_lerp_down: i32 = Replication::Replicated, Validation::Exact;
    damage_event: u32 = Replication::Replicated, Validation::AdoptOnly(
        "authority damage feedback (`damage_event` is a counter the server bumps). Same argument as health",
    );
    damage_yaw: u32 = Replication::Replicated, Validation::AdoptOnly(
        "direction of an incoming hit the client never saw fired; authority-only damage feedback",
    );
    damage_pitch: u32 = Replication::Replicated, Validation::AdoptOnly(
        "direction of an incoming hit the client never saw fired; authority-only damage feedback",
    );
    damage_count: i32 = Replication::Replicated, Validation::AdoptOnly(
        "magnitude of an incoming hit the client never saw fired; authority-only damage feedback",
    );
    damage_flags: u32 = Replication::Replicated, Validation::AdoptOnly(
        "classification of an incoming hit the client never saw fired; authority-only damage feedback",
    );
    health: i32 = Replication::Replicated, Validation::AdoptOnly(
        "resolved from other clients' commands, which a predicting client does not have. A difference here is missing information, not a prediction error, and counting it as one would make every firefight look like a desync",
    );
    max_health: i32 = Replication::Replicated, Validation::Exact;
    link_weapon_angles: vec3 = Replication::Replicated, Validation::Exact;
    cursor_hint: i32 = Replication::Replicated, Validation::AdoptOnly("authority-selected interaction target");
    cursor_hint_string: i32 = Replication::Replicated, Validation::AdoptOnly("authority-selected interaction target");
    cursor_hint_ent_index: i32 = Replication::Replicated, Validation::AdoptOnly("authority-selected interaction target");
    viewlocked: i32 = Replication::Replicated, Validation::AdoptOnly("authority-selected view lock");
    viewlocked_ent_num: i32 = Replication::Replicated, Validation::AdoptOnly("authority-selected view lock target");
    cursor_hint_dual_wield: i32 = Replication::Replicated, Validation::AdoptOnly("authority-selected interaction target");
    sprint_button_up_required: i32 = Replication::Replicated, Validation::Exact;
    sprint_delay: i32 = Replication::Replicated, Validation::Exact;
    last_sprint_start: i32 = Replication::Replicated, Validation::Exact;
    last_sprint_end: i32 = Replication::Replicated, Validation::Exact;
    sprint_start_max_length: i32 = Replication::Replicated, Validation::Exact;
    move_speed_scale_multiplier: f32 = Replication::Replicated, Validation::Exact;
    mantle_yaw: f32 = Replication::Replicated, Validation::Exact;
    mantle_timer: i32 = Replication::Replicated, Validation::Exact;
    mantle_trans_index: i32 = Replication::Replicated, Validation::Exact;
    mantle_flags: u32 = Replication::Replicated, Validation::Exact;
    weap_anim: i32 = Replication::Replicated, Validation::Exact;
    weapon_time: i32 = Replication::Replicated, Validation::Exact;
    weapon_delay: i32 = Replication::Replicated, Validation::Exact;
    weapon_restrict_kick_time: i32 = Replication::Replicated, Validation::Exact;
    weaponstate_primary: i32 = Replication::Replicated, Validation::Exact;
    weap_hand_flags: i32 = Replication::Replicated, Validation::Exact;
    weapon_shot_count: i32 = Replication::Replicated, Validation::Exact;
    weap_anim_secondary: i32 = Replication::Replicated, Validation::Exact;
    weapon_time_secondary: i32 = Replication::Replicated, Validation::Exact;
    weapon_delay_secondary: i32 = Replication::Replicated, Validation::Exact;
    weapon_restrict_kick_time_secondary: i32 = Replication::Replicated, Validation::Exact;
    weaponstate_secondary: i32 = Replication::Replicated, Validation::Exact;
    weap_hand_flags_secondary: i32 = Replication::Replicated, Validation::Exact;
    weapon_shot_count_secondary: i32 = Replication::Replicated, Validation::Exact;
    weapons: i32array = Replication::Replicated, Validation::Exact;
    weapon_data: opaque = Replication::Replicated, Validation::Exact;
    off_hand_index: i32 = Replication::Replicated, Validation::Exact;
    offhand_primary: i32 = Replication::Replicated, Validation::Exact;
    offhand_secondary: i32 = Replication::Replicated, Validation::Exact;
    weapon: u32 = Replication::Replicated, Validation::Exact;
    weapon_primary: u32 = Replication::Replicated, Validation::Exact;
    weap_flags: u32 = Replication::Replicated, Validation::Exact;
    f_weapon_pos_frac: f32 = Replication::Replicated, Validation::Exact;
    aim_spread_scale: f32 = Replication::Replicated, Validation::Exact;
    ads_delay_time: i32 = Replication::Replicated, Validation::Exact;
    spread_override: i32 = Replication::Replicated, Validation::Exact;
    spread_override_state: i32 = Replication::Replicated, Validation::Exact;
    last_weapon_hand: i32 = Replication::Replicated, Validation::Exact;
    ammo: opaque = Replication::Replicated, Validation::Exact;
    ammoclip: opaque = Replication::Replicated, Validation::Exact;
    melee_charge_yaw: f32 = Replication::Replicated, Validation::Exact;
    melee_charge_dist: i32 = Replication::Replicated, Validation::Exact;
    melee_charge_time: i32 = Replication::Replicated, Validation::Exact;
    perks: u32x2 = Replication::Replicated, Validation::Exact;
    perk_slots: u32x8 = Replication::Replicated, Validation::Exact;
    action_slot_type: i32array = Replication::Replicated, Validation::Exact;
    action_slot_param: i32array = Replication::Replicated, Validation::Exact;
    shellshock_index: i32 = Replication::Replicated, Validation::Exact;
    shellshock_time: i32 = Replication::Replicated, Validation::Exact;
    shellshock_duration: i32 = Replication::Replicated, Validation::Exact;
    stun_time: i32 = Replication::Replicated, Validation::Exact;
    objectives: opaque = Replication::Replicated, Validation::Exact;
    delta_time: i32 = Replication::Replicated, Validation::Exact;
    kill_cam_entity: i32 = Replication::Replicated, Validation::Exact;
    kill_cam_look_at_entity: i32 = Replication::Replicated, Validation::Exact;
    kill_cam_client_num: i32 = Replication::Replicated, Validation::Exact;
    recoil_scale: i32 = Replication::Replicated, Validation::Exact;
    hold_breath_timer: i32 = Replication::Replicated, Validation::Exact;
    hold_breath_scale: f32 = Replication::Replicated, Validation::Exact;
}

pub const PS_FIELD_COUNT: usize = PS_NETFIELDS.len();

pub fn player_state_diff(a: &PlayerState, b: &PlayerState) -> Vec<&'static str> {
    (0..PS_FIELD_COUNT)
        .filter(|&index| field_differs(a, b, index))
        .map(|index| PS_NETFIELDS[index].rust_name)
        .collect()
}

pub fn ps_deviation(predicted: &PlayerState, authoritative: &PlayerState) -> Option<Deviation> {
    for (index, row) in PS_NETFIELDS.iter().enumerate() {
        match row.validation {
            Validation::AdoptOnly(_) => continue,
            Validation::Exact => {
                if field_differs(predicted, authoritative, index) {
                    return Some(Deviation {
                        field: row.rust_name,
                        index,
                        distance: field_distance(predicted, authoritative, index),
                    });
                }
            }
            Validation::Tolerance { epsilon, .. } => {
                let distance = field_distance(predicted, authoritative, index);
                if distance > epsilon {
                    return Some(Deviation {
                        field: row.rust_name,
                        index,
                        distance,
                    });
                }
            }
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Deviation {
    pub field: &'static str,

    pub index: usize,

    pub distance: f32,
}

pub fn adopt_only_fields() -> Vec<(&'static str, &'static str)> {
    PS_NETFIELDS
        .iter()
        .filter_map(|row| match row.validation {
            Validation::AdoptOnly(reason) => Some((row.rust_name, reason)),
            _ => None,
        })
        .collect()
}

pub fn compute_state_hash(players: &[(sim::ClientId, PlayerState)]) -> u32 {
    let mut writer = WireWriter::new();
    for (client, ps) in players {
        writer.put_u32(client.0);
        for (index, row) in PS_NETFIELDS.iter().enumerate() {
            if row.is_validated() {
                write_field(&mut writer, ps, index);
            }
        }
    }
    fnv1a32(&writer.finish())
}

fn fnv1a32(bytes: &[u8]) -> u32 {
    const OFFSET: u32 = 0x811c_9dc5;
    const PRIME: u32 = 0x0100_0193;
    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}
