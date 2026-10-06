use crate::bullet::bullet_damage_at_distance;
use crate::bullet_collision::{
    BulletTraceQuery, ColliderId, EntityCollisionEpoch, EntityCollisionTraceGeom,
    HistorySampleVerdict, MASK_BULLET_WORLD, bullet_trace_segments_filtered, glass_piece_from_hit,
};
use crate::frame::FrameWorld;
use crate::identities::{DamageSource, LifeSequence, MatchRng, PelletId, ShotId};
use crate::match_state::{ClientLifecycle, EventAudience};
use crate::world::{ClientId, Tick};
use crate::world_objects::glass_piece_is_solid;
use anim_iw4::{
    ANIM_COND_RIOTSHIELDNEXT, ANIM_ET_DROPWEAPON, ANIM_ET_FIREWEAPON, ANIM_ET_KNIFE_MELEE,
    ANIM_ET_KNIFE_MELEE_CHARGE, ANIM_ET_MELEEATTACK, ANIM_ET_RAISEWEAPON, ANIM_ET_RELOAD,
};
use entity_iw4::glass_add_damage;
use movement_iw4::{Pml, is_in_air, mantle::is_weapon_inactive};
use playerstate_iw4::{ENTITYNUM_NONE, PlayerState, mantle_flags};
use std::cell::RefCell;
use std::collections::HashMap;
use weapon_iw4::{
    AIM_SPREAD_MOVE_SPEED_THRESHOLD_DEFAULT, AimSpreadMotion, AimSpreadState, CURSOR_HINT_NONE,
    FireWeaponKind, MELEE_TRACE_OFFSETS, MeleeChargeState, OFFHAND_INV_SLOTS, OffhandCmd,
    OffhandInvRow, PLAYER_MELEE_HEIGHT_DEFAULT, PLAYER_MELEE_RANGE_DEFAULT,
    PLAYER_MELEE_WIDTH_DEFAULT, SpreadOverrideState, WeaponCmd, WeaponHandState, WeaponTickEvent,
    add_aim_spread_fire, adjust_aim_spread_scale, ammo_row_present, ammo_table_key,
    begin_reload_event, clip_row_present, clip_table_key, fire_weapon_kind,
    fire_weapon_spread_degrees, get_ammo_not_in_clip, get_clip_for_hand, get_spread_for_weapon,
    melee_trace_count, melee_trace_end, num_hands_for_held, reload_insert_event,
    set_ammo_not_in_clip, set_clip_for_hand, weapon_hands,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AcceptedShot {
    pub shot_id: ShotId,
    pub fire_cause: Option<crate::FireCause>,
    pub attacker: ClientId,
    pub attacker_life: LifeSequence,

    pub hand: u8,
    pub weapon: u32,
    pub ammo_used: i32,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub ads_frac: f32,

    pub view_height_current: f32,

    pub aim_spread_scale: f32,

    pub perks0: u32,
    pub combat_seed: u32,
    pub owner_velocity: [f32; 3],
    pub spread_degrees: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Emission {
    pub combat_seed: u32,
    pub shot_id: ShotId,
    pub fire_cause: Option<crate::FireCause>,
    pub pellet: PelletId,
    pub attacker: ClientId,
    pub attacker_life: LifeSequence,
    pub hand: u8,
    pub weapon: u32,
    pub origin: [f32; 3],
    pub direction: [f32; 3],
    pub max_range: f32,
    pub base_damage: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerCollisionRepresentation {
    StandingAabbV1,
    PosedBonesV1,
}

impl PlayerCollisionRepresentation {
    pub const fn dump_label(self) -> &'static str {
        match self {
            Self::StandingAabbV1 => "aabb",
            Self::PosedBonesV1 => "bones",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityClipKind {
    CollTris,
    BoneBoxes,
    LinkedBrush,
}

impl EntityClipKind {
    pub const fn dump_label(self) -> &'static str {
        match self {
            Self::CollTris => "colltris",
            Self::BoneBoxes => "boxes",
            Self::LinkedBrush => "brush",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShotCollisionGeometry {
    Miss,
    World,
    Player {
        representation: PlayerCollisionRepresentation,
        history: HistorySampleVerdict,
    },
    Entity {
        epoch: EntityCollisionEpoch,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShotCollisionVerdict {
    pub shot_id: ShotId,
    pub pellet: PelletId,
    pub attacker: ClientId,
    pub geometry: ShotCollisionGeometry,
    pub terminal: Option<ColliderId>,
    pub startsolid: bool,
    pub bone_center: Option<[f32; 3]>,
    pub bone_half_size: Option<[f32; 3]>,
    pub xmodel_contents: Option<u32>,
    pub model_key: Option<String>,

    pub end: Option<[f32; 3]>,

    pub entity_clip: Option<EntityClipKind>,

    pub impact_n: u32,

    pub event_n: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TracePhaseOutput {
    pub shot_verdicts: Vec<ShotCollisionVerdict>,
}

pub(crate) fn advance_weapon_command(
    world: &mut FrameWorld,
    tick: Tick,
    id: ClientId,
    cmd: playerstate_iw4::UserCmd,
    msec: i32,
    command_sequence: Option<crate::CommandSequence>,
) -> Vec<AcceptedShot> {
    let msec = msec.clamp(1, 200);
    let frametime = msec as f32 / 1000.0;
    let mut accepted = Vec::new();
    {
        let id = &id;
        let cmd = &cmd;
        if !world
            .client_meta(*id)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
        {
            return accepted;
        }
        if let Some(mut ps) = world.player(*id).copied() {
            let parent = u32::from(cmd.weapon_mapped);
            let target = u32::from(cmd.weapon);
            if target != 0
                && parent != target
                && ps.weapons.contains(&(parent as i32))
                && world
                    .combat_facts_for(parent)
                    .is_some_and(|f| f.alternate_weapon == target)
            {
                if let Some(facts) = world.combat_facts_for(target) {
                    let ammo = weapon_iw4::ammo_table_key(facts.ammo_index, target);
                    let clip = weapon_iw4::clip_table_key(facts.clip_index, target);
                    let (initial_clip, _, initial_stock) = weapon_iw4::spawn_clip_stock(&facts, 0);
                    if !ammo_row_present(&ps.ammo, ammo) {
                        set_ammo_not_in_clip(&mut ps.ammo, ammo, initial_stock);
                    }
                    if !clip_row_present(&ps.ammoclip, clip) {
                        set_clip_for_hand(&mut ps.ammoclip, clip, 0, initial_clip);
                    }
                    world.client_meta_mut(*id).set_ammo(
                        target,
                        get_clip_for_hand(&ps.ammoclip, clip, 0),
                        get_ammo_not_in_clip(&ps.ammo, ammo),
                    );
                    *world.player_mut(*id).expect("present player") = ps;
                }
            }
        }
        let old_buttons = world
            .old_buttons_mut()
            .iter()
            .find(|(c, _)| c == id)
            .map_or(0, |(_, b)| *b);
        let old_angles = world
            .old_cmd_angles_mut()
            .iter()
            .find(|(c, _)| c == id)
            .map_or(cmd.angles, |(_, a)| *a);

        let Some(ps) = world.player(*id).copied() else {
            return accepted;
        };

        let facts_weapon = if ps.weapon != 0 {
            ps.weapon
        } else if cmd.weapon != 0 {
            u32::from(cmd.weapon)
        } else {
            return accepted;
        };
        let Some(facts) = world.combat_facts_for(facts_weapon) else {
            return accepted;
        };

        {
            let mut state = AimSpreadState {
                aim_spread_scale: ps.aim_spread_scale,
                spread_override: ps.spread_override,
                spread_override_state: ps.spread_override_state,
            };
            let motion = AimSpreadMotion {
                frametime,
                cmd_angles: cmd.angles,
                old_angles,
                forwardmove: cmd.forwardmove,
                rightmove: cmd.rightmove,
                velocity_xy: [ps.velocity[0], ps.velocity[1]],
                speed: ps.speed,
                move_speed_threshold: AIM_SPREAD_MOVE_SPEED_THRESHOLD_DEFAULT,
            };
            adjust_aim_spread_scale(
                &mut state,
                &facts.spread_facts(),
                &facts.aim_spread_decay_facts(),
                ps.ground_entity_num,
                ps.pm_type,
                ps.e_flags,
                ps.f_weapon_pos_frac,
                &motion,
            );
            if let Some(ps_mut) = world.player_mut(*id) {
                ps_mut.aim_spread_scale = state.aim_spread_scale;
                ps_mut.spread_override_state = state.spread_override_state;
            }
        }
        let Some(ps) = world.player(*id).copied() else {
            return accepted;
        };
        let meta = world.client_meta(*id).cloned().unwrap_or_default();
        let started_weapon = ps.weapon;
        let ammo_weapon = if started_weapon != 0 {
            started_weapon
        } else {
            facts_weapon
        };
        let last_hand = num_hands_for_held(&ps.weapons, &ps.weapon_data, ammo_weapon);
        let (meta_clip, meta_stock) = meta.ammo_for(ammo_weapon);
        let ammo_index = weapon_iw4::ammo_table_key(facts.ammo_index, ammo_weapon);
        let clip_index = weapon_iw4::clip_table_key(facts.clip_index, ammo_weapon);

        let stock = if ammo_row_present(&ps.ammo, ammo_index) {
            get_ammo_not_in_clip(&ps.ammo, ammo_index)
        } else {
            meta_stock
        };
        let clip0 = if clip_row_present(&ps.ammoclip, clip_index) {
            get_clip_for_hand(&ps.ammoclip, clip_index, 0)
        } else {
            meta_clip
        };
        let clip1 = if clip_row_present(&ps.ammoclip, clip_index) {
            get_clip_for_hand(&ps.ammoclip, clip_index, 1)
        } else {
            0
        };

        let quick_reload = facts.dual_mag.is_some() && meta.quick_reload_ready(ammo_weapon);
        let mut hands = [
            WeaponHandState {
                weapon: started_weapon,
                weaponstate: ps.weaponstate_primary,
                weapon_time: ps.weapon_time,
                weapon_delay: ps.weapon_delay,
                weap_anim: ps.weap_anim,
                hand_index: 0,
                clip: clip0,
                stock,
                shot_count: meta.weapon_shot_count,
                burst_latch: meta.burst_latch,
                rechamber_pending: meta.rechamber_pending,
                delayed_rechamber: false,
                weapon_restrict_kick_time: ps.weapon_restrict_kick_time,
                quick_reload,
            },
            WeaponHandState {
                weapon: started_weapon,
                weaponstate: ps.weaponstate_secondary,
                weapon_time: ps.weapon_time_secondary,
                weapon_delay: ps.weapon_delay_secondary,
                weap_anim: ps.weap_anim_secondary,
                hand_index: 1,
                clip: if last_hand >= 1 { clip1 } else { 0 },
                stock,
                shot_count: ps.weapon_shot_count_secondary as u8,
                burst_latch: meta.burst_latch_secondary,
                rechamber_pending: meta.rechamber_pending_secondary,
                delayed_rechamber: false,
                weapon_restrict_kick_time: ps.weapon_restrict_kick_time_secondary,
                quick_reload,
            },
        ];
        let locked_fire = world.missile_launch_facts(ps.weapon).is_none_or(|f| {
            !f.require_lock_to_fire || meta.weapon_lock.can_fire(ps.weapon, meta.life_sequence.0)
        });
        let fire_buttons = if locked_fire {
            cmd.buttons
        } else {
            cmd.buttons & !playerstate_iw4::buttons::ATTACK
        };
        let selected_airdrop_marker =
            world.weapon_script_name(ps.weapon) == crate::equipment::AIRDROP_MARKER_WEAPON;
        let marker_offhand_class = i32::MAX;
        let mut wcmd = WeaponCmd {
            msec,
            server_time: cmd.server_time,
            stun_time: ps.stun_time,
            buttons: fire_buttons
                | if selected_airdrop_marker && cmd.buttons & playerstate_iw4::buttons::ATTACK != 0
                {
                    playerstate_iw4::buttons::FRAG
                } else {
                    0
                },
            old_buttons: old_buttons
                | if selected_airdrop_marker && old_buttons & playerstate_iw4::buttons::ATTACK != 0
                {
                    playerstate_iw4::buttons::FRAG
                } else {
                    0
                },

            cmd_weapon: if cmd.weapon != 0 {
                cmd.weapon
            } else {
                ps.weapon as u16
            },
            pm_flags: ps.pm_flags,
            weap_flags: ps.weap_flags,
            pm_type: ps.pm_type,
            e_flags: ps.e_flags,
            last_weapon_hand: last_hand,
            f_weapon_pos_frac: ps.f_weapon_pos_frac,
            melee_charge_yaw: cmd.melee_charge_yaw,
            melee_charge_dist: cmd.melee_charge_dist,
            player_melee_range: PLAYER_MELEE_RANGE_DEFAULT,

            is_in_air: {
                let pml = Pml {
                    forward: [0.0; 3],
                    right: [0.0; 3],
                    up: [0.0; 3],
                    frametime,
                    msec,
                    walking: 0,
                    ground_plane: u32::from(ps.ground_entity_num != ENTITYNUM_NONE),
                    almost_ground_plane: 0,
                    ground_trace: [0; 11],
                    previous_origin: [0.0; 3],
                    previous_velocity: [0.0; 3],
                    holdrand: 0,
                    jump_animations: [None; 4],
                    mantle_movetype: None,
                    landing_animation: false,
                    fall_damage: 0,
                };
                is_in_air(&ps, &pml)
            },
            melee_charge: MeleeChargeState {
                pm_flags: ps.pm_flags,
                pm_type: ps.pm_type,
                e_flags: ps.e_flags,
                melee_charge_yaw: ps.melee_charge_yaw,
                melee_charge_dist: ps.melee_charge_dist,
                melee_charge_time: ps.melee_charge_time,
            },
            melee_started: None,

            mantle_weapon_inactive: is_weapon_inactive(&ps, true),
            mantle_quick_raise: (ps.mantle_flags & mantle_flags::QUICK) != 0,
            cmd_weapon_owned: {
                let w = if cmd.weapon != 0 {
                    u32::from(cmd.weapon)
                } else {
                    ps.weapon
                };
                w == 0
                    || ps.weapons.contains(&(w as i32))
                    || (ps.weapons.contains(&(i32::from(cmd.weapon_mapped)))
                        && world
                            .combat_facts_for(u32::from(cmd.weapon_mapped))
                            .is_some_and(|f| f.alternate_weapon == w))
            },

            cmd_weapon_pistol_quick: world
                .combat_facts_for(u32::from(cmd.weapon))
                .is_some_and(|f| f.weap_class == 5),
            alternate_switch: cmd.weapon != 0
                && (facts.alternate_weapon == u32::from(cmd.weapon)
                    || (facts.inventory_type == 3 && ps.weapon_primary == u32::from(cmd.weapon))),
            switch_alternate_raise_time_ms: world
                .combat_facts_for(u32::from(cmd.weapon))
                .map_or(0, |f| f.alternate_raise_time_ms),
            switch_raise_time_ms: {
                let w = if cmd.weapon != 0 {
                    u32::from(cmd.weapon)
                } else {
                    ps.weapon
                };
                world
                    .combat_facts_for(w)
                    .map(|f| f.raise_time_ms)
                    .unwrap_or(0)
            },
            switch_quick_raise_time_ms: {
                let w = if cmd.weapon != 0 {
                    u32::from(cmd.weapon)
                } else {
                    ps.weapon
                };
                world
                    .combat_facts_for(w)
                    .map(|f| f.quick_raise_time_ms)
                    .unwrap_or(0)
            },
            switch_first_raise_time_ms: {
                let w = if cmd.weapon != 0 {
                    u32::from(cmd.weapon)
                } else {
                    ps.weapon
                };
                if weapon_used_before(&ps, w) {
                    0
                } else {
                    world
                        .combat_facts_for(w)
                        .map_or(0, |f| f.first_raise_time_ms.max(0))
                }
            },
            perks0: ps.perks[0],
            perk_weap_reload_multiplier: weapon_iw4::PERK_WEAP_RELOAD_MULTIPLIER_DEFAULT,
            offhand: {
                let mut inventory = [OffhandInvRow::default(); OFFHAND_INV_SLOTS];
                for (i, &slot) in ps.weapons.iter().enumerate() {
                    if slot <= 0 {
                        continue;
                    }
                    let weapon = slot as u32;
                    let (clip, stock) = meta.ammo_for(weapon);
                    let eq = world.equipment_facts_for(weapon);
                    let combat = world.combat_facts_for(weapon);
                    inventory[i] = OffhandInvRow {
                        weapon,
                        offhand_class: if selected_airdrop_marker && weapon == ps.weapon {
                            marker_offhand_class
                        } else {
                            eq.map(|e| e.offhand_class).unwrap_or(0)
                        },
                        ammo: clip + stock,
                        hold_fire_time_ms: eq.map(|e| e.hold_fire_time_ms).unwrap_or(0),
                        fire_time_ms: combat.map(|f| f.fire_time_ms).unwrap_or(0),
                        fire_delay_ms: combat.map(|f| f.fire_delay_ms).unwrap_or(0),
                        fuse_time_ms: eq.map(|e| e.fuse_time_ms).unwrap_or(0),
                        cook_off_hold: eq.map(|e| e.cook_off_hold).unwrap_or(false),
                        offhand_hold_is_cancelable: combat
                            .and_then(|f| f.offhand_hold_is_cancelable),
                        weap_type: combat.map(|f| f.weap_type).unwrap_or(0),
                        has_detonator: eq.is_some_and(|e| e.has_detonator),
                        detonate_delay_ms: eq.map_or(0, |e| e.detonate_delay_ms),
                        detonate_time_ms: eq.map_or(0, |e| e.detonate_time_ms),
                    };
                }
                OffhandCmd {
                    inventory,
                    offhand_primary: if selected_airdrop_marker {
                        marker_offhand_class
                    } else {
                        ps.offhand_primary
                    },
                    offhand_secondary: ps.offhand_secondary,
                    cmd_off_hand_index: cmd.off_hand_index,
                    cmd_off_hand_owned: cmd.off_hand_index != 0
                        && ps
                            .weapons
                            .iter()
                            .any(|&slot| slot == i32::from(cmd.off_hand_index)),
                    cursor_hint_ent: CURSOR_HINT_NONE,
                    held_quick_drop_time_ms: facts.quick_drop_time_ms,
                    off_hand_index: ps.off_hand_index,
                    grenade_time_left: ps.grenade_time_left,
                }
            },
        };
        let clip_before = hands[0].clip;
        let events = weapon_hands(&mut hands, &facts, &mut wcmd, last_hand);
        let hand0 = hands[0];
        for &(hand, event) in events.iter().flatten() {
            if hand != 0 {
                continue;
            }
            match event {
                WeaponTickEvent::PutawayStarted
                    if ps.pm_flags & playerstate_iw4::pm_flags::MANTLE == 0 =>
                {
                    apply_player_anim_event_with_target(
                        world,
                        *id,
                        ANIM_ET_DROPWEAPON,
                        Some(u32::from(wcmd.cmd_weapon)),
                    );
                }
                WeaponTickEvent::RaiseStarted
                    if started_weapon != 0
                        && hand0.weaponstate
                            != weapon_iw4::WeaponState::RaisingAltswitch as i32 =>
                {
                    apply_player_anim_event(world, *id, ANIM_ET_RAISEWEAPON);
                }
                _ => {}
            }
        }
        if let Some(charge) = wcmd.melee_started {
            if let Some(ps_mut) = world.player_mut(*id) {
                movement_iw4::add_predictable_event(
                    ps_mut,
                    entity_iw4::EntityEventKind::MELEE_SWIPE.0,
                    0,
                );
            }
            let anim = match (facts.knife_model != 0, charge) {
                (true, true) => ANIM_ET_KNIFE_MELEE_CHARGE,
                (true, false) => ANIM_ET_KNIFE_MELEE,
                (false, _) => ANIM_ET_MELEEATTACK,
            };
            apply_player_anim_event(world, *id, anim);
        }

        if let Some(ps_mut) = world.player_mut(*id) {
            ps_mut.weapon = hand0.weapon;
            if hand0.weapon != started_weapon {
                if hand0.weaponstate == weapon_iw4::WeaponState::RaisingAltswitch as i32 {
                    ps_mut.aim_spread_scale = ps_mut.aim_spread_scale.max(128.0);
                }
                ps_mut.weapon_primary = if hand0.weapon == u32::from(cmd.weapon) {
                    u32::from(cmd.weapon_mapped)
                } else {
                    0
                };
            }
            ps_mut.weaponstate_primary = hand0.weaponstate;
            ps_mut.weapon_time = hand0.weapon_time;
            ps_mut.weapon_delay = hand0.weapon_delay;
            ps_mut.weap_anim = hand0.weap_anim;
            ps_mut.weapon_restrict_kick_time = hand0.weapon_restrict_kick_time;
            ps_mut.last_weapon_hand = last_hand;
            ps_mut.pm_flags = wcmd.pm_flags;
            ps_mut.weap_flags = wcmd.weap_flags;
            ps_mut.off_hand_index = wcmd.offhand.off_hand_index;
            ps_mut.grenade_time_left = wcmd.offhand.grenade_time_left;
            ps_mut.melee_charge_yaw = wcmd.melee_charge.melee_charge_yaw;
            ps_mut.melee_charge_dist = wcmd.melee_charge.melee_charge_dist;
            ps_mut.melee_charge_time = wcmd.melee_charge.melee_charge_time;
            if last_hand >= 1 {
                let h1 = hands[1];
                ps_mut.weaponstate_secondary = h1.weaponstate;
                ps_mut.weapon_time_secondary = h1.weapon_time;
                ps_mut.weapon_delay_secondary = h1.weapon_delay;
                ps_mut.weap_anim_secondary = h1.weap_anim;
                ps_mut.weapon_shot_count_secondary = i32::from(h1.shot_count);
                ps_mut.weapon_restrict_kick_time_secondary = h1.weapon_restrict_kick_time;
            }
        }
        let life = world
            .client_meta(*id)
            .map(|m| m.life_sequence)
            .unwrap_or_default();
        let meta = world.client_meta_mut(*id);
        if hand0.weapon != started_weapon {
            if started_weapon != 0 {
                meta.set_ammo(started_weapon, hand0.clip, hand0.stock);
            }
        } else if hand0.weapon != 0 {
            meta.set_ammo(hand0.weapon, hand0.clip, hand0.stock);
        }
        if hand0.weapon != 0 {
            meta.mirror_held_ammo(hand0.weapon);
        }
        meta.weapon_shot_count = hand0.shot_count;
        meta.burst_latch = hand0.burst_latch;
        meta.rechamber_pending = hand0.rechamber_pending;
        meta.burst_latch_secondary = last_hand >= 1 && hands[1].burst_latch;
        meta.rechamber_pending_secondary = last_hand >= 1 && hands[1].rechamber_pending;
        if facts.dual_mag.is_some() && started_weapon != 0 {
            meta.set_quick_reload_ready(started_weapon, hand0.quick_reload);
        }

        if ammo_index != 0 || clip_index != 0 {
            if let Some(ps_mut) = world.player_mut(*id) {
                if ammo_index != 0 {
                    let _ = set_ammo_not_in_clip(&mut ps_mut.ammo, ammo_index, hands[0].stock);
                }
                if clip_index != 0 {
                    let _ = set_clip_for_hand(&mut ps_mut.ammoclip, clip_index, 0, hands[0].clip);
                    if last_hand >= 1 {
                        let _ =
                            set_clip_for_hand(&mut ps_mut.ammoclip, clip_index, 1, hands[1].clip);
                    }
                }
            }
        }

        if hands[0].delayed_rechamber {
            if let Some(ps) = world.player_mut(*id) {
                movement_iw4::add_predictable_event(
                    ps,
                    entity_iw4::EntityEventKind::RECHAMBER_WEAPON.0,
                    0,
                );
            }
        }
        if hands[0].clip > clip_before {
            if let Some(ps) = world.player_mut(*id) {
                movement_iw4::add_predictable_event(
                    ps,
                    entity_iw4::EntityEventKind::RELOAD_ADDAMMO.0,
                    0,
                );
            }
        }

        let mut fire_ordinal = 0u16;
        for slot in events.into_iter().flatten() {
            let (_hand_i, ev) = slot;
            match ev {
                WeaponTickEvent::Detonated { weapon } => {
                    world
                        .weapon_notes
                        .push(crate::equipment::WeaponNote::DetonationRequested {
                            owner: *id,
                            weapon,
                        });
                    if let Some(ps) = world.player_mut(*id) {
                        movement_iw4::add_predictable_event(
                            ps,
                            entity_iw4::EntityEventKind::DETONATE.0,
                            weapon as i32,
                        );
                    }
                }
                WeaponTickEvent::ReloadAmmoAdded { shells: _ } => {}
                WeaponTickEvent::RechamberWeapon => {
                    if !hands[_hand_i as usize].delayed_rechamber {
                        if let Some(ps) = world.player_mut(*id) {
                            movement_iw4::add_predictable_event(
                                ps,
                                entity_iw4::EntityEventKind::RECHAMBER_WEAPON.0,
                                0,
                            );
                        }
                    }
                }
                WeaponTickEvent::EjectBrass => {
                    let kind = if _hand_i == 1 {
                        entity_iw4::EntityEventKind::EJECT_BRASS_LEFT
                    } else {
                        entity_iw4::EntityEventKind::EJECT_BRASS
                    };
                    let pending = world.client_meta_mut(*id).pending_brass[_hand_i as usize]
                        .take()
                        .filter(|pending| {
                            pending.life == life && pending.weapon == hands[_hand_i as usize].weapon
                        });
                    let origin = world.player(*id).map_or([0.0; 3], |ps| ps.origin);
                    world.push_entity_event(
                        tick,
                        EventAudience::All,
                        kind,
                        crate::EntityEventPayload {
                            number: id.0 as i32,
                            weapon: hands[_hand_i as usize].weapon,
                            hand: _hand_i,
                            origin,
                            correlation: pending.map_or(0, |pending| pending.shot.0),
                            fire_cause: pending.and_then(|pending| pending.cause),
                            ..Default::default()
                        },
                    );
                }
                WeaponTickEvent::ShotAccepted { ammo_used } => {
                    let hand = &hands[_hand_i as usize];
                    let shot_id = world.alloc_shot_id();
                    let fire_cause = command_sequence.map(|command| crate::FireCause {
                        client: *id,
                        life,
                        command,
                        ordinal: fire_ordinal,
                        hand: _hand_i,
                    });
                    fire_ordinal += 1;
                    let weapon = hand.weapon;
                    if facts.bolt_action {
                        world.client_meta_mut(*id).pending_brass[_hand_i as usize] =
                            Some(crate::PendingBrass {
                                cause: fire_cause,
                                life,
                                shot: shot_id,
                                weapon,
                            });
                    }
                    let Some(ps) = world.player(*id).copied() else {
                        continue;
                    };
                    apply_player_anim_event(world, *id, ANIM_ET_FIREWEAPON);
                    let combat_seed = world.combat_rng_mut().next_u32();
                    let origin = world
                        .client_meta(*id)
                        .and_then(|meta| meta.linked_weapon_view)
                        .map_or(
                            [
                                ps.origin[0],
                                ps.origin[1],
                                ps.origin[2] + ps.view_height_current,
                            ],
                            |view| view.origin,
                        );

                    let mut shot_angles = ps.viewangles;
                    for (angle, offset) in shot_angles.iter_mut().zip(cmd.gun_angle_offset) {
                        if offset.is_finite() {
                            *angle += offset.clamp(-45.0, 45.0);
                        }
                    }
                    let last_shot = hand.clip == 0 && facts.fire_type != 5;
                    world.push_entity_event(
                        tick,
                        EventAudience::All,
                        entity_iw4::predicted_weapon_fire_event(_hand_i as i32, last_shot),
                        crate::EntityEventPayload {
                            number: id.0 as i32,
                            weapon,
                            correlation: shot_id.0,
                            fire_cause,
                            hand: _hand_i,
                            origin,
                            direction: shot_angles,
                            ..Default::default()
                        },
                    );
                    world
                        .weapon_notes
                        .push(crate::equipment::WeaponNote::Fired { owner: *id });

                    if let Some(ps_mut) = world.player_mut(*id) {
                        add_aim_spread_fire(
                            &mut ps_mut.aim_spread_scale,
                            ps.f_weapon_pos_frac,
                            facts.hip_spread_fire_add,
                        );
                    }
                    let aim_spread_scale = world
                        .player(*id)
                        .map(|p| p.aim_spread_scale)
                        .unwrap_or(ps.aim_spread_scale);
                    let ads_frac = ps.f_weapon_pos_frac.clamp(0.0, 1.0);
                    let override_state = SpreadOverrideState::from_i32(ps.spread_override_state);
                    let cone = get_spread_for_weapon(
                        ps.view_height_current,
                        ps.spread_override,
                        override_state,
                        &facts.spread_facts(),
                        weapon_iw4::perk_weap_spread_multiplier(ps.perks[0]),
                    );
                    let spread_degrees = fire_weapon_spread_degrees(
                        cone,
                        facts.ads_spread,
                        ads_frac,
                        aim_spread_scale,
                    );
                    accepted.push(AcceptedShot {
                        shot_id,
                        fire_cause,
                        attacker: *id,
                        attacker_life: life,
                        hand: _hand_i,
                        weapon,
                        ammo_used,
                        origin,
                        angles: shot_angles,
                        ads_frac,
                        view_height_current: ps.view_height_current,
                        aim_spread_scale,
                        perks0: ps.perks[0],
                        combat_seed,
                        owner_velocity: ps.velocity,
                        spread_degrees,
                    });
                }
                WeaponTickEvent::OffhandPrepare { weapon } => {
                    world
                        .weapon_notes
                        .push(crate::equipment::WeaponNote::Pullback { owner: *id, weapon });
                    if let Some(ps) = world.player_mut(*id) {
                        movement_iw4::add_predictable_event(
                            ps,
                            entity_iw4::EntityEventKind::PREP_OFFHAND.0,
                            weapon as i32,
                        );
                    }
                }
                WeaponTickEvent::OffhandUsed {
                    weapon,
                    remaining_fuse_ms,
                } => {
                    let combat = world.weapon_combat_row(weapon);
                    let meta = world.client_meta_mut(*id);
                    let (clip, stock) = meta.ammo_for(weapon);
                    if clip + stock > 0 {
                        if clip > 0 {
                            meta.set_ammo(weapon, clip - 1, stock);
                        } else {
                            meta.set_ammo(weapon, 0, stock - 1);
                        }
                    }
                    if let Some(facts) = combat {
                        if let Some(ps) = world.player_mut(*id) {
                            spend_ps_offhand_round(ps, weapon, facts);
                        }
                    }
                    if crate::equipment::spawn_offhand_projectile(
                        world,
                        *id,
                        weapon,
                        tick,
                        remaining_fuse_ms,
                    ) {
                        if let Some(ps) = world.player(*id).copied() {
                            let origin = [
                                ps.origin[0],
                                ps.origin[1],
                                ps.origin[2] + ps.view_height_current,
                            ];
                            world.push_entity_event(
                                tick,
                                EventAudience::All,
                                entity_iw4::EntityEventKind::USE_OFFHAND,
                                crate::EntityEventPayload {
                                    number: id.0 as i32,
                                    weapon,
                                    origin,
                                    event_parm: weapon as i32,
                                    ..Default::default()
                                },
                            );
                        }
                    }
                }
                WeaponTickEvent::OffhandCookedOff { weapon } => {
                    let combat = world.weapon_combat_row(weapon);
                    let meta = world.client_meta_mut(*id);
                    let (clip, stock) = meta.ammo_for(weapon);
                    if clip + stock > 0 {
                        if clip > 0 {
                            meta.set_ammo(weapon, clip - 1, stock);
                        } else {
                            meta.set_ammo(weapon, 0, stock - 1);
                        }
                    }
                    if let Some(facts) = combat {
                        if let Some(ps) = world.player_mut(*id) {
                            spend_ps_offhand_round(ps, weapon, facts);
                        }
                    }
                    crate::equipment::explode_offhand_in_hand(world, *id, weapon, tick);
                }
                WeaponTickEvent::ReloadStarted => {
                    apply_player_anim_event(world, *id, ANIM_ET_RELOAD);
                    let hand = &hands[_hand_i as usize];
                    let event = begin_reload_event(&facts, hand.clip);
                    if let Some(ps) = world.player_mut(*id) {
                        movement_iw4::add_predictable_event(
                            ps,
                            entity_iw4::EntityEventKind::RESET_ADS.0,
                            0,
                        );
                        movement_iw4::add_predictable_event(ps, event, 0);
                    }
                    world
                        .weapon_notes
                        .push(crate::equipment::WeaponNote::ReloadStarted { owner: *id });
                }
                WeaponTickEvent::ReloadInsert => {
                    let hand = &hands[_hand_i as usize];
                    let event = reload_insert_event(&facts, hand.clip);
                    if let Some(ps) = world.player_mut(*id) {
                        movement_iw4::add_predictable_event(ps, event, 0);
                    }
                }
                WeaponTickEvent::ReloadEnded => {
                    if let Some(ps) = world.player_mut(*id) {
                        movement_iw4::add_predictable_event(
                            ps,
                            entity_iw4::EntityEventKind::RELOAD_END.0,
                            0,
                        );
                    }
                }
                WeaponTickEvent::EmptyClick => {
                    if let Some(ps) = world.player_mut(*id) {
                        movement_iw4::add_predictable_event(
                            ps,
                            entity_iw4::EntityEventKind::NOAMMO.0,
                            0,
                        );
                    }
                }
                WeaponTickEvent::AlternateStarted => {
                    if let Some(ps) = world.player_mut(*id) {
                        movement_iw4::add_predictable_event(
                            ps,
                            entity_iw4::EntityEventKind::WEAPON_ALT.0,
                            i32::from(cmd.weapon),
                        );
                    }
                }
                WeaponTickEvent::PutawayStarted => {
                    if let Some(ps) = world.player_mut(*id) {
                        movement_iw4::add_predictable_event(
                            ps,
                            entity_iw4::EntityEventKind::PUTAWAY_WEAPON.0,
                            0,
                        );
                    }
                }
                WeaponTickEvent::RaiseStarted => {
                    let hand = &hands[_hand_i as usize];
                    let first = hand.weap_anim as u32 & weapon_iw4::WEAP_ANIM_EVENT_MASK
                        == weapon_iw4::weap_anim_event::FIRST_RAISE;
                    let weapon = hand.weapon;
                    if let Some(ps) = world.player_mut(*id) {
                        mark_weapon_used(ps, weapon);
                        movement_iw4::add_predictable_event(
                            ps,
                            if first {
                                entity_iw4::EntityEventKind::FIRST_RAISE_WEAPON.0
                            } else {
                                entity_iw4::EntityEventKind::RAISE_WEAPON.0
                            },
                            0,
                        );
                    }
                }
                WeaponTickEvent::MeleeFired => {
                    let hand = &hands[_hand_i as usize];
                    let weapon = hand.weapon;
                    let Some(ps) = world.player(*id).copied() else {
                        continue;
                    };
                    let origin = [
                        ps.origin[0],
                        ps.origin[1],
                        ps.origin[2] + ps.view_height_current,
                    ];
                    world.push_entity_event(
                        tick,
                        EventAudience::All,
                        entity_iw4::EntityEventKind::FIRE_MELEE,
                        crate::EntityEventPayload {
                            number: id.0 as i32,
                            weapon,
                            origin,
                            direction: ps.viewangles,
                            ..Default::default()
                        },
                    );
                    if world.publishes_snapshot() {
                        fire_weapon_melee(world, tick, *id, life, weapon, origin, ps.viewangles);
                    }
                }
                WeaponTickEvent::StunnedStarted => {
                    apply_player_anim_event(world, *id, 21);
                }
                WeaponTickEvent::RaiseFinished | WeaponTickEvent::DropFinished => {}
            }
        }
        let meta = world.client_meta_mut(*id);
        for (index, hand) in hands.iter().enumerate() {
            if !hand.rechamber_pending
                || index > last_hand as usize
                || meta.pending_brass[index]
                    .is_some_and(|pending| pending.life != life || pending.weapon != hand.weapon)
            {
                meta.pending_brass[index] = None;
            }
        }
    }
    accepted
}

pub(crate) fn apply_player_anim_event(world: &mut FrameWorld, id: ClientId, event: u8) {
    apply_player_anim_event_with_target(world, id, event, None);
}

fn apply_player_anim_event_with_target(
    world: &mut FrameWorld,
    id: ClientId,
    event: u8,
    next_weapon: Option<u32>,
) {
    apply_player_anim_event_inner(
        world,
        id,
        event,
        next_weapon,
        !matches!(event, ANIM_ET_RAISEWEAPON | 11..=16),
    );
}

pub(crate) fn apply_player_anim_event_forced(
    world: &mut FrameWorld,
    id: ClientId,
    event: u8,
    force: bool,
) {
    apply_player_anim_event_inner(world, id, event, None, force);
}

fn apply_player_anim_event_inner(
    world: &mut FrameWorld,
    id: ClientId,
    event: u8,
    next_weapon: Option<u32>,
    force: bool,
) {
    let Some(script) = world.player_anim_script() else {
        return;
    };
    let mut seed = world.anim_event_seed();
    let Some(ps) = world.player(id).copied() else {
        return;
    };
    let (view_w, primary) = crate::pmove_anim_weapon_ids(&ps);
    let view_facts = world.combat_facts_for(view_w);
    let primary_facts = world.combat_facts_for(primary);
    let movetype =
        crate::player_anim_script::event_anim_movetype(&ps, world.last_anim_movetype(id));
    let strafing = world.last_anim_strafing(id);
    let mut conds = crate::anim_conditions_from_pmove(
        &ps,
        view_facts,
        primary_facts,
        Some(movetype),
        strafing,
        world.anim_command_buttons(id),
    );
    if let Some(next_weapon) = next_weapon {
        conds.set_value(
            ANIM_COND_RIOTSHIELDNEXT,
            u32::from(
                world
                    .combat_facts_for(next_weapon)
                    .is_some_and(|facts| facts.player_anim_type == 15),
            ),
        );
    }
    {
        let Some(ps) = world.player_mut(id) else {
            return;
        };
        script.apply_event(ps, event, &conds, &mut seed, force);
    }
    world.set_anim_event_seed(seed);
}

pub fn spread_pellet_direction(
    angles: [f32; 3],
    spread_degrees: f32,
    rng: &mut MatchRng,
) -> [f32; 3] {
    spread_direction_on_plane(angles, spread_degrees, rng, 1.0)
}

pub fn spread_direction_on_plane(
    angles: [f32; 3],
    spread_degrees: f32,
    rng: &mut MatchRng,
    plane: f32,
) -> [f32; 3] {
    let (forward, right, up) = math_iw4::angle_vectors(angles);
    if spread_degrees <= 0.0 {
        return forward;
    }
    let unit = |draw: u32| draw as f32 / u32::MAX as f32;
    let radius = unit(rng.next_u32());
    let theta = unit(rng.next_u32()) * core::f32::consts::TAU;
    let lateral = plane * spread_degrees.to_radians().tan() * radius;
    let q = [
        plane * forward[0] + lateral * (theta.cos() * right[0] + theta.sin() * up[0]),
        plane * forward[1] + lateral * (theta.cos() * right[1] + theta.sin() * up[1]),
        plane * forward[2] + lateral * (theta.cos() * right[2] + theta.sin() * up[2]),
    ];
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt();
    if len > 0.0 {
        [q[0] / len, q[1] / len, q[2] / len]
    } else {
        forward
    }
}

pub(crate) fn phase_emit(world: &FrameWorld, shots: &[AcceptedShot]) -> Vec<Emission> {
    let mut out = Vec::new();
    for shot in shots {
        let Some(facts) = world.combat_facts_for(shot.weapon) else {
            continue;
        };

        match fire_weapon_kind(facts.weap_type, facts.weap_class) {
            Some(FireWeaponKind::Bullet) => {
                if facts.damage <= 0 || facts.bullet_range() <= 0.0 {
                    continue;
                }
            }
            _ => continue,
        }
        let mut rng = MatchRng::new(shot.combat_seed as u64);
        let barrels = if facts.fire_type == 5 {
            shot.ammo_used.max(1)
        } else {
            1
        };
        let pellet_count = (facts.pellet_count() * barrels).clamp(1, u16::MAX as i32) as u16;
        for pellet in 0..pellet_count {
            out.push(Emission {
                combat_seed: shot.combat_seed,
                shot_id: shot.shot_id,
                fire_cause: shot.fire_cause,
                pellet: PelletId(pellet),
                attacker: shot.attacker,
                attacker_life: shot.attacker_life,
                hand: shot.hand,
                weapon: shot.weapon,
                origin: shot.origin,
                direction: spread_pellet_direction(shot.angles, shot.spread_degrees, &mut rng),
                max_range: facts.bullet_range(),
                base_damage: facts.damage,
            });
        }
    }
    out
}

fn segment_is_shield(collider: Option<ColliderId>) -> bool {
    matches!(
        collider,
        Some(ColliderId::Player {
            hitloc: crate::shield::HITLOC,
            ..
        })
    )
}

pub(crate) fn phase_trace(
    world: &mut FrameWorld,
    tick: Tick,
    emissions: &[Emission],
) -> TracePhaseOutput {
    let mut output = TracePhaseOutput::default();
    let mut pending: std::collections::VecDeque<_> =
        emissions.iter().copied().map(|em| (em, 0u8)).collect();
    while let Some((em, bounces)) = pending.pop_front() {
        let Some(facts) = world.combat_facts_for(em.weapon) else {
            continue;
        };
        let query = world.lagcomp_query_for(em.attacker, tick);
        let end = [
            em.origin[0] + em.direction[0] * em.max_range,
            em.origin[1] + em.direction[1] * em.max_range,
            em.origin[2] + em.direction[2] * em.max_range,
        ];
        let pen = world.bullet_pen_facts_for(em.weapon);
        let glass_damage = RefCell::new(
            world
                .world_objects()
                .glass_damage_pairs()
                .into_iter()
                .collect::<HashMap<u32, u16>>(),
        );
        let glass_seen = RefCell::new(Vec::<u32>::new());
        let on_glass_hit = |piece: u16, end: [f32; 3]| {
            let pane = u32::from(piece);
            if glass_seen.borrow().contains(&pane) {
                return;
            }
            glass_seen.borrow_mut().push(pane);
            let dist = {
                let dx = end[0] - em.origin[0];
                let dy = end[1] - em.origin[1];
                let dz = end[2] - em.origin[2];
                (dx * dx + dy * dy + dz * dz).sqrt()
            };
            let scaled = bullet_damage_at_distance(&facts, dist).max(0) as u32;
            let mut map = glass_damage.borrow_mut();
            let cur = map.entry(pane).or_insert(0);
            *cur = glass_add_damage(*cur, scaled);
        };
        let (segments, terminal) = bullet_trace_segments_filtered(
            world.clip_brushes(),
            world.clip_bsp(),
            world.clip_cmodels(),
            world.clip_mesh(),
            &query.players.poses,
            &query.entities.rows,
            &BulletTraceQuery {
                start: em.origin,
                end,
                mask: MASK_BULLET_WORLD,
                ignore: Some(em.attacker),
                ignore_hit: None,
                ignore_model: None,
            },
            pen,
            world.penetration_table(),
            &|piece| {
                glass_piece_is_solid(
                    glass_damage
                        .borrow()
                        .get(&(u32::from(piece)))
                        .copied()
                        .unwrap_or(0),
                )
            },
            Some(&on_glass_hit),
        );
        if bounces < 8
            && facts.weap_type == weapon_iw4::WEAPTYPE_BULLET
            && facts.weap_class != weapon_iw4::WEAPCLASS_SPREAD
            && !pen.explosive_bullet
            && let Some(hit) = segments
                .last()
                .filter(|hit| !hit.startsolid && segment_is_shield(hit.collider))
        {
            let mut rng = MatchRng::new(
                u64::from(em.combat_seed) ^ (u64::from(em.pellet.0) << 32) ^ u64::from(bounces),
            );
            let chance = (pen.ricochet_chance * 0.5).clamp(0.0, 1.0);
            let roll = (rng.next_u32() >> 8) as f32 / 16_777_216.0;
            if roll <= chance
                && chance > 0.0
                && let Some(direction) = crate::shield::ricochet_direction(em.direction, hit.normal)
            {
                pending.push_back((
                    Emission {
                        origin: std::array::from_fn(|i| hit.end[i] + direction[i]),
                        direction,
                        ..em
                    },
                    bounces + 1,
                ));
            }
        }
        let entity_epoch = entity_collision_epoch(terminal, &query.entities.rows);
        let startsolid = segments.first().is_some_and(|s| s.startsolid);
        let impact_n = segments
            .iter()
            .filter(|s| bullet_process_on_hit(s.collider))
            .count() as u32;
        let event_n = segments
            .iter()
            .filter(|s| {
                bullet_process_on_hit(s.collider)
                    && entity_iw4::bullet_hit_event(
                        if segment_is_shield(s.collider) {
                            3
                        } else {
                            facts.impact_type
                        },
                        false,
                    )
                    .is_some()
            })
            .count() as u32;
        let (bone_center, bone_half_size, xmodel_contents, model_key) =
            hit_dump(terminal, &query.entities.rows);
        output.shot_verdicts.push(ShotCollisionVerdict {
            shot_id: em.shot_id,
            pellet: em.pellet,
            attacker: em.attacker,
            geometry: shot_collision_geometry(
                terminal,
                query.players.verdict,
                entity_epoch,
                player_representation(terminal, &query.players.poses),
            ),
            terminal,
            startsolid,
            bone_center,
            bone_half_size,
            xmodel_contents,
            model_key,
            end: segments.last().map(|s| s.end),
            entity_clip: entity_clip_kind(terminal, &query.entities.rows),
            impact_n,
            event_n,
        });
        if segments.is_empty() {
            continue;
        }
        let mut glass_hit: Vec<u32> = Vec::new();
        let mut mob_shot = false;
        if world.publishes_snapshot() && crate::voxel::active() {
            let first_surface = segments
                .iter()
                .find(|s| matches!(s.collider, None | Some(ColliderId::World { .. })))
                .map_or(end, |s| s.end);
            let mut stop = first_surface;
            if let Some((key, dist, up)) = crate::voxel::mob_on_segment(em.origin, first_surface) {
                // As a player hit: the weapon's damage at that range, times
                // its multiplier for where on the body it struck.
                let scale = facts.location_scale(crate::voxel::mob_hitloc(up));
                let damage = bullet_damage_at_distance(&facts, dist).max(0) as f32 * scale;
                crate::voxel::push_mob_shot(key, damage, em.origin);
                mob_shot = true;
                let length = {
                    let d: [f32; 3] = std::array::from_fn(|k| first_surface[k] - em.origin[k]);
                    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-3)
                };
                stop = std::array::from_fn(|k| em.origin[k] + (first_surface[k] - em.origin[k]) * (dist / length));
            }
            let ray_damage = bullet_damage_at_distance(&facts, 0.0).max(0) as f32;
            crate::voxel::push_ray(em.origin, stop, ray_damage);
        }
        for segment in &segments {
            let exit = segment.surface_flags & fx_iw4::FX_IMPACT_EXIT_SURFACE_FLAG != 0;
            let dist = {
                let dx = segment.end[0] - em.origin[0];
                let dy = segment.end[1] - em.origin[1];
                let dz = segment.end[2] - em.origin[2];
                (dx * dx + dy * dy + dz * dz).sqrt()
            };
            let scaled =
                ((bullet_damage_at_distance(&facts, dist) as f32) * segment.damage_mult) as i32;
            if !exit && world.publishes_snapshot() {
                let means = crate::script_player::means(
                    world,
                    DamageSource::Shot(em.shot_id),
                    em.weapon,
                    0,
                    false,
                );
                crate::script::host::triggers::damage_line(
                    world.ecs(),
                    segment.start,
                    segment.end,
                    scaled,
                    em.attacker,
                    None,
                    means,
                );
            }
            let mut flesh_flags = 0u8;
            if !exit
                && let Some(ColliderId::Player {
                    client: victim,
                    life: victim_life,
                    hitloc,
                }) = segment.collider
            {
                let head = hud_iw4::obituary_is_headshot(hitloc);
                let mut fatal = false;
                if world.publishes_snapshot()
                    && scaled > 0
                    && let Some(vmeta) = world.client_meta(victim)
                    && vmeta.lifecycle == ClientLifecycle::Alive
                {
                    let attempt = crate::DamageAttempt {
                        splash: false,
                        source: DamageSource::Shot(em.shot_id),
                        pellet: em.pellet,
                        attacker: em.attacker,
                        attacker_life: em.attacker_life,
                        target: victim,
                        target_life: victim_life,
                        weapon: em.weapon,
                        amount: scaled,
                        killcam_entity_start_time: 0,
                        inflictor_origin: None,
                        hitloc,
                    };
                    fatal = matches!(
                        crate::damage::apply_damage_attempt(world, tick, &attempt),
                        crate::DamageOutcome::Died(_)
                    );
                }
                if hitloc != crate::shield::HITLOC {
                    flesh_flags = fx_iw4::flesh_hit_flags(head, fatal) as u8;
                }
            }
            let payload = crate::EntityEventPayload {
                number: em.attacker.0 as i32,
                attacker_entity_num: em.attacker.0 as i32,
                other_entity_num: match segment.collider {
                    Some(
                        ColliderId::EntityDObjBone { owner, .. }
                        | ColliderId::EntityLinkedBrush { owner, .. },
                    ) => owner
                        .script_model()
                        .and_then(|id| world.gentity_number(id))
                        .unwrap_or(ENTITYNUM_NONE),
                    Some(ColliderId::Player { client, .. }) => client.0 as i32,
                    _ => ENTITYNUM_NONE,
                },
                event_parm: i32::from(flesh_flags),
                weapon: em.weapon,
                correlation: em.shot_id.0,
                fire_cause: em.fire_cause,
                pellet: em.pellet.0,
                hand: em.hand,
                origin: segment.end,
                origin2: segment.start,
                direction: segment.normal,
                surf_type: segment.surf_type,
                surface_flags: segment.surface_flags,
                simulation_flags: u8::from(segment.penetrated),
            };
            let victim = match segment.collider {
                Some(ColliderId::Player { client, .. }) => Some(client),
                _ => None,
            };
            if matches!(segment.collider, None | Some(ColliderId::World { .. }))
                && !exit
                && !mob_shot
                && world.publishes_snapshot()
                && crate::voxel::active()
            {
                let body = facts.location_scale(4).max(1.0);
                crate::voxel::push_shot(segment.end, segment.normal, scaled.max(0) as f32 * body);
            }
            if bullet_process_on_hit(segment.collider) {
                if let Some(world_event) = entity_iw4::bullet_hit_event(
                    if segment_is_shield(segment.collider) {
                        3
                    } else {
                        facts.impact_type
                    },
                    false,
                ) {
                    let world_audience =
                        victim.map_or(EventAudience::All, EventAudience::AllExcept);
                    world.push_entity_event(tick, world_audience, world_event, payload);
                    if let Some(victim) = victim
                        && let Some(local_event) = entity_iw4::bullet_hit_event(
                            if segment_is_shield(segment.collider) {
                                3
                            } else {
                                facts.impact_type
                            },
                            true,
                        )
                    {
                        world.push_entity_event(
                            tick,
                            EventAudience::Client(victim),
                            local_event,
                            payload,
                        );
                    }
                } else if fx_iw4::impact_table_row(facts.impact_type, false).is_some() {
                    world.push_pellet_fx(crate::PelletFxRecord {
                        attacker: em.attacker.0 as i32,
                        weapon: em.weapon,
                        correlation: em.shot_id.0,
                        pellet: em.pellet.0,
                        hand: em.hand,
                        start: segment.start,
                        end: segment.end,
                        normal: segment.normal,
                        surf_type: segment.surf_type,
                        surface_flags: segment.surface_flags,
                        flesh_flags,
                    });
                }
            } else {
                world.push_pellet_fx(crate::PelletFxRecord {
                    attacker: em.attacker.0 as i32,
                    weapon: em.weapon,
                    correlation: em.shot_id.0,
                    pellet: em.pellet.0,
                    hand: em.hand,
                    start: segment.start,
                    end: segment.end,
                    normal: [0.0; 3],
                    surf_type: segment.surf_type,
                    surface_flags: segment.surface_flags,
                    flesh_flags: 0,
                });
            }
            if exit || !world.publishes_snapshot() || scaled <= 0 {
                continue;
            }
            match segment.collider {
                Some(ColliderId::Player { .. }) => {}
                Some(
                    ColliderId::EntityDObjBone { owner, .. }
                    | ColliderId::EntityLinkedBrush { owner, .. },
                ) => {
                    if let Some(ColliderId::EntityDObjBone { bone, .. }) = segment.collider
                        && crate::t5_destructible::apply_hit(
                            world,
                            tick,
                            owner,
                            bone,
                            scaled as u32,
                            Some(em.attacker),
                        )
                    {
                        continue;
                    }
                    if let Some(target) = owner.script_model() {
                        let bone = match segment.collider {
                            Some(ColliderId::EntityDObjBone { bone, .. }) => {
                                Some(usize::from(bone))
                            }
                            _ => None,
                        };
                        let means = crate::script_player::means(
                            world,
                            DamageSource::Shot(em.shot_id),
                            em.weapon,
                            0,
                            false,
                        );
                        crate::script::damage_entity(
                            world.ecs(),
                            &crate::script::EntityHit {
                                target,
                                amount: scaled,
                                attacker: Some(em.attacker),
                                means,
                                weapon: em.weapon,
                                point: segment.end,
                                dir: em.direction,
                                bone,
                                flags: 0,
                            },
                        );
                    }
                }
                Some(ColliderId::World { .. }) => {
                    if let Some(piece) = glass_piece_from_hit(segment.hit_type, segment.hit_id) {
                        if glass_hit.contains(&piece) {
                            continue;
                        }
                        glass_hit.push(piece);
                        let at_time_ms = i32::try_from(tick.0.saturating_mul(crate::MATCH_TICK_MS))
                            .unwrap_or(i32::MAX);
                        let mut holdrand = *world.stuck_holdrand_mut();
                        world.world_objects_mut().apply_glass_hit(
                            piece,
                            scaled as u32,
                            at_time_ms,
                            segment.end,
                            em.direction,
                            &mut || crate::item::random_unit(&mut holdrand),
                        );
                        *world.stuck_holdrand_mut() = holdrand;
                    }
                }
                None => {}
            }
        }
    }
    world.record_shot_collision_verdicts(&output.shot_verdicts);
    output
}

fn fire_weapon_melee(
    world: &mut FrameWorld,
    tick: Tick,
    attacker: ClientId,
    attacker_life: LifeSequence,
    weapon: u32,
    origin: [f32; 3],
    angles: [f32; 3],
) {
    let Some(facts) = world.combat_facts_for(weapon) else {
        return;
    };
    if facts.melee_damage <= 0 {
        return;
    }
    let (forward, right, up) = math_iw4::angle_vectors(angles);
    let range = PLAYER_MELEE_RANGE_DEFAULT;
    let width = PLAYER_MELEE_WIDTH_DEFAULT;
    let height = PLAYER_MELEE_HEIGHT_DEFAULT;
    let query = world.lagcomp_query_for(attacker, tick);
    let glass_pairs = world.world_objects().glass_damage_pairs();
    let is_solid = |piece| {
        crate::world_objects::glass_piece_is_solid(
            glass_pairs
                .iter()
                .find(|(id, _)| *id == u32::from(piece))
                .map(|(_, d)| *d)
                .unwrap_or(0),
        )
    };
    let mut best_frac = 1.0f32;
    let mut best_hit: Option<crate::bullet_collision::BulletTraceSegment> = None;
    let n = melee_trace_count(width, height);
    for (index, offset) in MELEE_TRACE_OFFSETS.iter().take(n).enumerate() {
        let end = melee_trace_end(origin, forward, right, up, range, width, height, *offset);
        let (segments, _) = bullet_trace_segments_filtered(
            world.clip_brushes(),
            world.clip_bsp(),
            world.clip_cmodels(),
            world.clip_mesh(),
            &query.players.poses,
            &query.entities.rows,
            &BulletTraceQuery {
                start: origin,
                end,
                mask: MASK_BULLET_WORLD,
                ignore: Some(attacker),
                ignore_hit: None,
                ignore_model: None,
            },
            weapon_iw4::BulletPenFacts::default(),
            world.penetration_table(),
            &is_solid,
            None,
        );
        if index == 0
            && world.publishes_snapshot()
            && let Some(segment) = segments.first()
        {
            crate::script::host::triggers::damage_line(
                world.ecs(),
                origin,
                segment.end,
                facts.melee_damage,
                attacker,
                None,
                "MOD_MELEE",
            );
        }
        let Some(segment) = segments.iter().find(|s| s.collider.is_some()) else {
            continue;
        };
        if segment.surface_flags & 0x10 != 0 {
            continue;
        }
        let ray = [end[0] - origin[0], end[1] - origin[1], end[2] - origin[2]];
        let hit = [
            segment.end[0] - origin[0],
            segment.end[1] - origin[1],
            segment.end[2] - origin[2],
        ];
        let ray_len2 = ray[0] * ray[0] + ray[1] * ray[1] + ray[2] * ray[2];
        if ray_len2 <= 0.0 {
            continue;
        }
        let frac = (hit[0] * hit[0] + hit[1] * hit[1] + hit[2] * hit[2]).sqrt() / ray_len2.sqrt();
        if frac >= 1.0 || frac > best_frac {
            continue;
        }
        best_frac = frac;
        best_hit = Some(*segment);
    }
    let Some(segment) = best_hit else {
        return;
    };
    let amount = facts.melee_damage + (world.combat_rng_mut().next_u32() % 5) as i32;
    if matches!(segment.collider, None | Some(ColliderId::World { .. }))
        && world.publishes_snapshot()
        && crate::voxel::active()
    {
        crate::voxel::push_shot(segment.end, segment.normal, amount as f32);
    }
    let (kind, other) = match segment.collider {
        Some(ColliderId::Player { client, .. }) => {
            (entity_iw4::EntityEventKind::MELEE_HIT, client.0 as i32)
        }
        _ => (
            entity_iw4::EntityEventKind::MELEE_MISS,
            i32::from(trace_iw4::ENTITYNUM_WORLD),
        ),
    };
    world.push_entity_event(
        tick,
        EventAudience::All,
        kind,
        crate::EntityEventPayload {
            number: attacker.0 as i32,
            attacker_entity_num: attacker.0 as i32,
            other_entity_num: other,
            event_parm: i32::from(facts.knife_model != 0),
            weapon,
            origin: segment.end,
            direction: forward,
            surf_type: segment.surf_type,
            surface_flags: segment.surface_flags,
            ..Default::default()
        },
    );
    match segment.collider {
        Some(ColliderId::Player {
            client: victim,
            life: victim_life,
            hitloc,
        }) => {
            if let Some(vmeta) = world.client_meta(victim)
                && vmeta.lifecycle == ClientLifecycle::Alive
            {
                let attempt = crate::DamageAttempt {
                    splash: false,
                    source: DamageSource::Melee,
                    pellet: PelletId(0),
                    attacker,
                    attacker_life,
                    target: victim,
                    target_life: victim_life,
                    weapon,
                    amount,
                    killcam_entity_start_time: 0,
                    inflictor_origin: Some(origin),
                    hitloc,
                };
                let _ = crate::damage::apply_damage_attempt(world, tick, &attempt);
                world.push_entity_event(
                    tick,
                    EventAudience::All,
                    entity_iw4::EntityEventKind::MELEE_BLOOD,
                    crate::EntityEventPayload {
                        number: attacker.0 as i32,
                        attacker_entity_num: attacker.0 as i32,
                        other_entity_num: victim.0 as i32,
                        weapon,
                        origin: segment.end,
                        direction: forward,
                        surf_type: segment.surf_type,
                        surface_flags: segment.surface_flags,
                        ..Default::default()
                    },
                );
            }
        }
        Some(ColliderId::World { .. }) => {
            if let Some(piece) = glass_piece_from_hit(segment.hit_type, segment.hit_id) {
                let at_time_ms =
                    i32::try_from(tick.0.saturating_mul(crate::MATCH_TICK_MS)).unwrap_or(i32::MAX);
                let mut holdrand = *world.stuck_holdrand_mut();
                world.world_objects_mut().apply_glass_hit(
                    piece,
                    u32::from(crate::world_objects::GLASS_MELEE_DAMAGE),
                    at_time_ms,
                    segment.end,
                    forward,
                    &mut || crate::item::random_unit(&mut holdrand),
                );
                *world.stuck_holdrand_mut() = holdrand;
            }
        }
        _ => {}
    }
}

fn entity_collision_epoch(
    terminal: Option<ColliderId>,
    rows: &[EntityCollisionTraceGeom],
) -> Option<EntityCollisionEpoch> {
    let owner = match terminal? {
        ColliderId::EntityDObjBone { owner, .. } | ColliderId::EntityLinkedBrush { owner, .. } => {
            owner
        }
        ColliderId::World { .. } | ColliderId::Player { .. } => return None,
    };
    rows.iter()
        .find(|row| row.owner == owner)
        .map(|row| row.epoch)
}

fn hit_dump(
    terminal: Option<ColliderId>,
    rows: &[EntityCollisionTraceGeom],
) -> (
    Option<[f32; 3]>,
    Option<[f32; 3]>,
    Option<u32>,
    Option<String>,
) {
    let Some(ColliderId::EntityDObjBone { owner, bone, .. }) = terminal else {
        return (None, None, None, None);
    };
    let Some(row) = rows.iter().find(|row| row.owner == owner) else {
        return (None, None, None, None);
    };
    let bone = row
        .collision
        .as_ref()
        .and_then(|collision| collision.bones.iter().find(|b| b.bone == bone));
    (
        bone.map(|b| b.center),
        bone.map(|b| b.half_size),
        row.dobj_contents,
        row.model_key.clone(),
    )
}

fn player_representation(
    terminal: Option<ColliderId>,
    poses: &[crate::bullet_collision::PlayerCollisionPose],
) -> PlayerCollisionRepresentation {
    let Some(ColliderId::Player { client, .. }) = terminal else {
        return PlayerCollisionRepresentation::StandingAabbV1;
    };
    match poses.iter().find(|pose| pose.client == client) {
        Some(pose) if !pose.bones.is_empty() => PlayerCollisionRepresentation::PosedBonesV1,
        _ => PlayerCollisionRepresentation::StandingAabbV1,
    }
}

fn entity_clip_kind(
    terminal: Option<ColliderId>,
    rows: &[EntityCollisionTraceGeom],
) -> Option<EntityClipKind> {
    match terminal {
        Some(ColliderId::EntityLinkedBrush { .. }) => Some(EntityClipKind::LinkedBrush),
        Some(ColliderId::EntityDObjBone { owner, .. }) => {
            let coll = rows
                .iter()
                .find(|row| row.owner == owner)
                .and_then(|row| row.collision.as_ref())
                .and_then(|collision| collision.coll.as_ref());
            Some(
                if crate::bullet_collision::coll_tris_clip_available(coll, MASK_BULLET_WORLD) {
                    EntityClipKind::CollTris
                } else {
                    EntityClipKind::BoneBoxes
                },
            )
        }
        _ => None,
    }
}

fn shot_collision_geometry(
    terminal: Option<ColliderId>,
    player_history: HistorySampleVerdict,
    entity_epoch: Option<EntityCollisionEpoch>,
    representation: PlayerCollisionRepresentation,
) -> ShotCollisionGeometry {
    match terminal {
        Some(ColliderId::Player { .. }) => ShotCollisionGeometry::Player {
            representation,
            history: player_history,
        },
        Some(ColliderId::EntityDObjBone { .. } | ColliderId::EntityLinkedBrush { .. }) => {
            match entity_epoch {
                Some(epoch) => ShotCollisionGeometry::Entity { epoch },
                None => ShotCollisionGeometry::Miss,
            }
        }
        Some(ColliderId::World { .. }) => ShotCollisionGeometry::World,
        None => match player_history {
            HistorySampleVerdict::Refused { .. } => ShotCollisionGeometry::Player {
                representation,
                history: player_history,
            },
            _ => ShotCollisionGeometry::Miss,
        },
    }
}

pub(crate) fn bullet_process_on_hit(collider: Option<ColliderId>) -> bool {
    collider.is_some()
}

fn spend_ps_offhand_round(ps: &mut PlayerState, weapon: u32, facts: weapon_iw4::WeaponCombatFacts) {
    let clip_key = clip_table_key(facts.clip_index, weapon);
    if clip_key != 0 && clip_row_present(&ps.ammoclip, clip_key) {
        let clip = get_clip_for_hand(&ps.ammoclip, clip_key, 0);
        if clip > 0 {
            let _ = set_clip_for_hand(&mut ps.ammoclip, clip_key, 0, clip - 1);
            return;
        }
    }
    let ammo_key = ammo_table_key(facts.ammo_index, weapon);
    if ammo_key != 0 && ammo_row_present(&ps.ammo, ammo_key) {
        let stock = get_ammo_not_in_clip(&ps.ammo, ammo_key);
        if stock > 0 {
            let _ = set_ammo_not_in_clip(&mut ps.ammo, ammo_key, stock - 1);
        }
    }
}

fn weapon_used_before(ps: &PlayerState, weapon: u32) -> bool {
    ps.weapons
        .iter()
        .position(|&held| held == weapon as i32)
        .is_none_or(|slot| ps.weapon_data[slot * WEAPON_DATA_STRIDE] != 0)
}

pub(crate) fn mark_weapon_used(ps: &mut PlayerState, weapon: u32) {
    if let Some(slot) = ps.weapons.iter().position(|&held| held == weapon as i32) {
        ps.weapon_data[slot * WEAPON_DATA_STRIDE] = 1;
    }
}

const WEAPON_DATA_STRIDE: usize = 5;

pub(crate) fn raise_given_weapon(
    ps: &mut PlayerState,
    weapon: u32,
    hand: &weapon_iw4::WeaponHandState,
) {
    mark_weapon_used(ps, weapon);
    if hand.weap_anim as u32 & weapon_iw4::WEAP_ANIM_EVENT_MASK
        == weapon_iw4::weap_anim_event::FIRST_RAISE
    {
        movement_iw4::add_predictable_event(
            ps,
            entity_iw4::EntityEventKind::FIRST_RAISE_WEAPON.0,
            0,
        );
    }
}
