use crate::damage::{DamageAttempt, DamageOutcome, DeathCommit};
use crate::frame::FrameWorld;
use crate::identities::DamageSource;
use crate::match_state::{ClientLifecycle, EventAudience, SimEvent};
use crate::step::{arm_held_weapon, seed_ps_ammo_tables};
use crate::world::{
    ClientId, Tick, gsc_give_weapon_is_akimbo, inventory_add_weapon, spawn_player_state,
};
use movement_iw4::SHORT2ANGLE;

pub(crate) fn spectator_lifecycle(team: i32) -> ClientLifecycle {
    if team == entity_iw4::TEAM_SPECTATOR {
        ClientLifecycle::Spectating
    } else {
        ClientLifecycle::ChoosingClass
    }
}

pub(crate) fn spawn(
    world: &mut FrameWorld,
    tick: Tick,
    id: ClientId,
    origin: [f32; 3],
    angles: [f32; 3],
    sessionstate: &str,
) {
    let (origin, angles) = match world.client_meta_mut(id).forced_spawn.take() {
        Some(pick) if sessionstate == "playing" => {
            forced_point(world, id, pick).unwrap_or((origin, angles))
        }
        _ => (origin, angles),
    };
    world.unlink_player_area(id);
    world.client_meta_mut(id).shield = None;
    let mut ps = spawn_player_state(origin, angles);
    if let Some(max) = world
        .client_meta(id)
        .map(|m| m.max_health)
        .filter(|&n| n > 0)
    {
        ps.max_health = max;
        ps.health = max;
    }
    if let Some(cmd) = world.old_cmd_angles(id) {
        ps.delta_angles = std::array::from_fn(|axis| angles[axis] - cmd[axis] as f32 * SHORT2ANGLE);
    }
    let prev_teleport = world
        .player(id)
        .map(|p| p.e_flags & playerstate_iw4::eflags::TELEPORT)
        .unwrap_or(0);
    ps.e_flags = (ps.e_flags & !playerstate_iw4::eflags::TELEPORT) | prev_teleport;
    ps.e_flags ^= playerstate_iw4::eflags::TELEPORT;
    let lifecycle = match sessionstate {
        "playing" => ClientLifecycle::Alive,
        "intermission" => {
            ps.pm_type = playerstate_iw4::PM_TYPE_INTERMISSION;
            ClientLifecycle::Intermission
        }
        "dead" => {
            ps.pm_type = playerstate_iw4::PM_TYPE_DEAD;
            ClientLifecycle::Dead
        }
        _ => {
            ps.pm_type = playerstate_iw4::PM_TYPE_SPECTATOR;
            spectator_lifecycle(
                world
                    .client_meta(id)
                    .map_or(entity_iw4::TEAM_FREE, |m| m.client_state_team),
            )
        }
    };
    *world.ensure_player(id) = ps;
    diag::info!(
        Sim,
        "spawn: script client={} {sessionstate} at [{:.1}, {:.1}, {:.1}]",
        id.0,
        origin[0],
        origin[1],
        origin[2]
    );
    let life_sequence = {
        let meta = world.client_meta_mut(id);
        meta.lifecycle = lifecycle;
        meta.controls = crate::ScriptControls::default();
        if lifecycle != ClientLifecycle::Alive {
            return;
        }
        meta.life_sequence = meta.life_sequence.next();
        meta.dead_since_tick = None;
        meta.item_use_spawn_ms = crate::corpse::level_time_ms(tick);
        meta.item_use_entity = None;
        meta.clear_ammo_inventory();
        meta.weapon_shot_count = 0;
        meta.burst_latch = false;
        meta.burst_latch_secondary = false;
        meta.rechamber_pending = false;
        meta.rechamber_pending_secondary = false;
        meta.pending_brass = [None; 2];
        meta.life_sequence
    };
    let class_id = world
        .client_meta(id)
        .and_then(|m| m.loadout.as_ref().map(|l| l.class_id))
        .unwrap_or(crate::ClassId(0));
    world.push_event(
        tick,
        EventAudience::Client(id),
        SimEvent::Spawned {
            class_id,
            spawn_index: 0,
            life_sequence,
        },
    );
    world.link_player_standing_area(id);
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Hit {
    pub victim: ClientId,
    pub attacker: Option<ClientId>,
    pub amount: i32,
    pub flags: i32,
    pub means: &'static str,
    pub weapon: u32,
    pub point: [f32; 3],
    pub dir: [f32; 3],
    pub hitloc: u8,
    pub inflictor: Option<crate::ProjectileId>,
    pub commit: Option<DeathCommit>,
}

pub(crate) const IDFLAGS_RADIUS: i32 = 1;

fn means_of_death(world: &FrameWorld, intent: &DamageAttempt) -> &'static str {
    means(
        world,
        intent.source,
        intent.weapon,
        intent.hitloc,
        intent.splash,
    )
}

pub(crate) fn means(
    world: &FrameWorld,
    source: DamageSource,
    weapon: u32,
    hitloc: u8,
    splash: bool,
) -> &'static str {
    let facts = world.combat_facts_for(weapon);
    let grenade = facts.is_some_and(|f| f.weap_class == weapon_iw4::WEAPCLASS_GRENADE);
    match source {
        DamageSource::Melee => "MOD_MELEE",
        DamageSource::Shot(_) if hud_iw4::obituary_is_headshot(hitloc) => "MOD_HEAD_SHOT",
        DamageSource::Shot(_)
            if facts.is_some_and(|f| f.weap_class == weapon_iw4::WEAPCLASS_PISTOL) =>
        {
            "MOD_PISTOL_BULLET"
        }
        DamageSource::Shot(_) => "MOD_RIFLE_BULLET",
        DamageSource::Projectile(_) if splash && grenade => "MOD_GRENADE_SPLASH",
        DamageSource::Projectile(_) if splash => "MOD_PROJECTILE_SPLASH",
        DamageSource::Projectile(_)
            if world
                .equipment_facts_for(weapon)
                .is_some_and(|f| f.impact_payload_weapon != 0) =>
        {
            "MOD_IMPACT"
        }
        DamageSource::Projectile(_) if grenade => "MOD_GRENADE",
        DamageSource::Projectile(_) => "MOD_PROJECTILE",
        DamageSource::Radius(_) => "MOD_EXPLOSIVE",
    }
}

fn normalized(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > 0.0 {
        v.map(|c| c / len)
    } else {
        [0.0; 3]
    }
}

pub(crate) fn damage(
    world: &mut FrameWorld,
    tick: Tick,
    intent: &DamageAttempt,
    amount: i32,
) -> DamageOutcome {
    let victim_origin = world.player(intent.target).map_or([0.0; 3], |ps| ps.origin);
    let from = intent
        .inflictor_origin
        .or_else(|| world.player(intent.attacker).map(|ps| ps.origin))
        .unwrap_or(victim_origin);
    let splash = intent.splash;
    let hit = Hit {
        victim: intent.target,
        attacker: Some(intent.attacker),
        amount,
        flags: if splash { IDFLAGS_RADIUS } else { 0 },
        means: means_of_death(world, intent),
        weapon: intent.weapon,
        point: intent.inflictor_origin.unwrap_or(victim_origin),
        dir: normalized([0, 1, 2].map(|i| victim_origin[i] - from[i])),
        hitloc: intent.hitloc,
        inflictor: match intent.source {
            DamageSource::Projectile(id) => Some(id),
            _ => None,
        },
        commit: Some(DeathCommit {
            victim: intent.target,
            victim_life: intent.target_life,
            attacker: intent.attacker,
            attacker_life: intent.attacker_life,
            source: intent.source,
            pellet: intent.pellet,
            weapon: intent.weapon,
            amount,
            killcam_entity_start_time: intent.killcam_entity_start_time,
            inflictor_origin: intent.inflictor_origin,
            hitloc: intent.hitloc,
        }),
    };
    let commit = hit.commit;
    crate::script::player_damage(world.ecs(), tick, &hit);
    let meta = world.client_meta(intent.target);
    if meta.is_some_and(|m| {
        m.lifecycle == ClientLifecycle::Dead && m.life_sequence == intent.target_life
    }) {
        return DamageOutcome::Died(commit.expect("set above"));
    }
    DamageOutcome::Nonlethal {
        health_after: world.player(intent.target).map_or(0, |ps| ps.health),
    }
}

pub(crate) fn debug_damage(world: &mut FrameWorld, tick: Tick, id: ClientId, amount: i32) {
    let origin = world.player(id).map_or([0.0; 3], |ps| ps.origin);
    let hit = Hit {
        victim: id,
        attacker: None,
        amount,
        flags: 0,
        means: "MOD_UNKNOWN",
        weapon: 0,
        point: origin,
        dir: [0.0; 3],
        hitloc: 0,
        inflictor: None,
        commit: None,
    };
    crate::script::player_damage(world.ecs(), tick, &hit);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Finish {
    Hurt,
    LastStand,
    Killed,
}

pub(crate) fn god_mode(world: &FrameWorld, id: ClientId) -> bool {
    world.bootstrap_ref().allow_debug_actions
        && world.client_meta(id).is_some_and(|meta| meta.god_mode)
}

pub(crate) fn finish_damage(
    world: &mut FrameWorld,
    id: ClientId,
    amount: i32,
    dir: Option<[f32; 3]>,
) -> Finish {
    if god_mode(world, id) {
        return Finish::Hurt;
    }
    if !world
        .client_meta(id)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        return Finish::Hurt;
    }
    let Some(ps) = world.player_mut(id) else {
        return Finish::Hurt;
    };
    movement_iw4::update_damage_timer(ps, amount, dir);
    ps.health = (ps.health - amount.max(0)).max(0);
    ps.damage_count = ps.damage_count.saturating_add(1);
    ps.damage_event = ps.damage_event.wrapping_add(1);
    if ps.health > 0 {
        return Finish::Hurt;
    }
    use playerstate_iw4::pm_flags::LAST_STAND;
    if ps.perks[0] & playerstate_iw4::PERK_PISTOLDEATH != 0 && ps.pm_flags & LAST_STAND == 0 {
        ps.health = 1;
        ps.pm_type = playerstate_iw4::PM_TYPE_LAST_STAND;
        ps.view_height_target = movement_iw4::view_height::LAST_STAND;
        ps.pm_flags |= LAST_STAND;
        return Finish::LastStand;
    }
    Finish::Killed
}

pub(crate) fn revive(world: &mut FrameWorld, id: ClientId) {
    if let Some(ps) = world.player_mut(id) {
        ps.pm_type = 0;
        ps.pm_flags &= !playerstate_iw4::pm_flags::LAST_STAND;
        ps.view_height_target = movement_iw4::view_height::CROUCH;
    }
}

pub(crate) fn kill(
    world: &mut FrameWorld,
    tick: Tick,
    victim: ClientId,
    attacker: Option<ClientId>,
    commit: Option<DeathCommit>,
) {
    if !world
        .client_meta(victim)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        return;
    }
    let life_sequence = {
        let meta = world.client_meta_mut(victim);
        meta.lifecycle = ClientLifecycle::Dead;
        meta.dead_since_tick = Some(tick.0);
        meta.controls.switch_to = 0;
        meta.life_sequence
    };
    crate::script::host::triggers::release_client_claims(world.ecs(), victim.0);
    crate::damage::play_death(world, victim, attacker, commit.as_ref());
    if let Some(ps) = world.player_mut(victim) {
        ps.health = 0;
        ps.pm_flags &= !playerstate_iw4::pm_flags::LAST_STAND;
    }
    world.unlink_player_area(victim);
    let attacker_life = attacker.and_then(|a| world.client_meta(a).map(|m| m.life_sequence));
    world.push_event(
        tick,
        EventAudience::All,
        SimEvent::Died {
            victim,
            life_sequence,
            attacker,
            attacker_life,
            source: commit.map(|c| c.source),
            weapon: commit.map_or(0, |c| c.weapon),
            killcam_entity_start_time: commit.map_or(0, |c| c.killcam_entity_start_time),
        },
    );
}

pub(crate) fn obituary(
    world: &mut FrameWorld,
    tick: Tick,
    victim: ClientId,
    attacker: Option<ClientId>,
    weapon: u32,
    means: &str,
) {
    let (weap_type, weap_class) = world
        .combat_facts_for(weapon)
        .map_or((0, 0), |f| (f.weap_type, f.weap_class));
    let means = match means {
        "MOD_HEAD_SHOT" => hud_iw4::MOD_HEAD_SHOT,
        "MOD_MELEE" => hud_iw4::MOD_MELEE,
        "MOD_SUICIDE" => hud_iw4::MOD_SUICIDE,
        _ => 0,
    };
    let attacker = attacker.unwrap_or(victim);
    world.push_entity_event(
        tick,
        EventAudience::All,
        entity_iw4::EntityEventKind::OBITUARY,
        crate::EntityEventPayload {
            number: victim.0 as i32,
            other_entity_num: victim.0 as i32,
            attacker_entity_num: attacker.0 as i32,
            event_parm: hud_iw4::pack_obituary_event_parm(weapon, means, weap_type, weap_class),
            weapon,
            ..Default::default()
        },
    );
}

pub(crate) fn clone_corpse(world: &mut FrameWorld, tick: Tick, id: ClientId) -> Option<u8> {
    let ps = world.player(id).copied()?;
    let slot =
        crate::corpse::occupy_player_clone(world, id, &ps, crate::corpse::level_time_ms(tick));
    if let Some(ps) = world.player_mut(id) {
        ps.corpse_index = i32::from(slot);
        ps.e_flags |= 0x20000;
    }
    Some(slot)
}

fn forced_point(
    world: &mut FrameWorld,
    id: ClientId,
    pick: crate::SpawnPick,
) -> Option<([f32; 3], [f32; 3])> {
    let team = world
        .client_meta(id)
        .map_or(entity_iw4::TEAM_FREE, |m| m.client_state_team);
    let avoid: Vec<[f32; 3]> = world
        .alive_origins()
        .into_iter()
        .filter(|&o| world.player(id).is_none_or(|ps| ps.origin != o))
        .collect();
    let report = crate::spawn::decide_forced_spawn(world, pick, &avoid, team);
    crate::step::log_forced_spawn(id, pick, &report);
    report.accepted.map(|d| (d.traced_origin, d.raw_angles))
}

pub(crate) fn move_to_forced_spawn(world: &mut FrameWorld, id: ClientId, pick: crate::SpawnPick) {
    let Some((origin, angles)) = forced_point(world, id, pick) else {
        return;
    };
    world.unlink_player_area(id);
    world.set_origin(id, origin);
    world.set_viewangles(id, angles);
    if let Some(ps) = world.player_mut(id) {
        ps.velocity = [0.0; 3];
        ps.health = ps.max_health;
        ps.e_flags ^= playerstate_iw4::eflags::TELEPORT;
    }
    let meta = world.client_meta_mut(id);
    meta.life_sequence = meta.life_sequence.next();
    world.link_player_standing_area(id);
}

pub(crate) fn weapon_named(world: &FrameWorld, name: &str) -> Result<u32, String> {
    if name == "none" {
        return Ok(0);
    }
    world
        .weapon_index_by_script_name(name)
        .filter(|&w| w != 0)
        .ok_or_else(|| format!("unknown weapon '{name}'"))
}

pub(crate) fn weapon_name(world: &FrameWorld, weapon: u32) -> String {
    if weapon == 0 {
        "none".into()
    } else {
        world.weapon_script_name(weapon).to_owned()
    }
}

fn living(world: &FrameWorld, id: ClientId) -> Result<(), String> {
    if world.player(id).is_none() {
        return Err("player has not spawned".into());
    }
    Ok(())
}

pub(crate) fn give_weapon(
    world: &mut FrameWorld,
    id: ClientId,
    weapon: u32,
    akimbo: bool,
) -> Result<(), String> {
    living(world, id)?;
    if world.weapon_is_melee_only(weapon) {
        return Ok(());
    }
    let facts = world
        .combat_facts_for(weapon)
        .ok_or_else(|| format!("weapon {} has no combat data", weapon_name(world, weapon)))?;
    let akimbo =
        akimbo || facts.dual_wield || gsc_give_weapon_is_akimbo(world.weapon_script_name(weapon));
    let offhand = world
        .equipment_facts_for(weapon)
        .filter(|eq| eq.is_offhand());
    let had = world
        .player(id)
        .is_some_and(|ps| ps.weapons.contains(&(weapon as i32)));
    let ps = world.player_mut(id).expect("checked above");
    inventory_add_weapon(ps, weapon, akimbo);
    if !ps.weapons.contains(&(weapon as i32)) {
        return Err("the player's inventory is full".into());
    }
    if had {
        return Ok(());
    }
    let (clip, clip_alt, stock) = match &offhand {
        Some(eq) => (eq.spawn_clip_count(), 0, 0),
        None => weapon_iw4::spawn_clip_stock(&facts, i32::from(akimbo)),
    };
    seed_ps_ammo_tables(ps, weapon, &facts, clip, clip_alt, akimbo, stock);
    world.client_meta_mut(id).set_ammo(weapon, clip, stock);
    Ok(())
}

pub(crate) fn set_weapon_model(world: &mut FrameWorld, id: ClientId, weapon: u32, model: u8) {
    if let Some(ps) = world.player_mut(id) {
        weapon_iw4::set_weapon_model_for_held(&ps.weapons, &mut ps.weapon_data, weapon, model);
    }
}

pub(crate) fn take_weapon(world: &mut FrameWorld, id: ClientId, weapon: u32) {
    let Some(ps) = world.player_mut(id) else {
        return;
    };
    if let Some(slot) = ps.weapons.iter().position(|&w| w == weapon as i32) {
        ps.weapons[slot] = 0;
        ps.weapon_data[slot * 5..slot * 5 + 5].fill(0);
    }
    if ps.weapon == weapon {
        ps.weapon = 0;
    }
    if ps.weapon_primary == weapon {
        ps.weapon_primary = 0;
    }
    let meta = world.client_meta_mut(id);
    meta.ammo_by_weapon.retain(|row| row.0 != weapon);
    if meta.controls.switch_to == weapon {
        meta.controls.switch_to = 0;
    }
}

pub(crate) fn take_all_weapons(world: &mut FrameWorld, id: ClientId) {
    let Some(ps) = world.player_mut(id) else {
        return;
    };
    ps.weapons = [0; 15];
    ps.weapon_data.fill(0);
    ps.ammo.fill(0);
    ps.ammoclip.fill(0);
    ps.weapon = 0;
    ps.weapon_primary = 0;
    ps.offhand_primary = 0;
    ps.offhand_secondary = 0;
    let meta = world.client_meta_mut(id);
    meta.clear_ammo_inventory();
    meta.controls.switch_to = 0;
}

pub(crate) fn set_spawn_weapon(
    world: &mut FrameWorld,
    id: ClientId,
    weapon: u32,
) -> Result<(), String> {
    living(world, id)?;
    let facts = world
        .combat_facts_for(weapon)
        .unwrap_or_else(weapon_iw4::WeaponCombatFacts::none);
    let ammo = world.client_meta(id).map(|m| m.ammo_for(weapon));
    let ps = world.player_mut(id).expect("checked above");
    let (ammo_before, clip_before) = (ps.ammo, ps.ammoclip);
    ps.weapon = weapon;
    arm_held_weapon(ps, weapon, &facts);
    ps.ammo = ammo_before;
    ps.ammoclip = clip_before;
    let meta = world.client_meta_mut(id);
    if let Some((clip, stock)) = ammo {
        meta.set_ammo(weapon, clip, stock);
    }
    meta.mirror_held_ammo(weapon);
    meta.controls.switch_to = 0;
    Ok(())
}

pub(crate) fn switch_to_weapon_immediate(
    world: &mut FrameWorld,
    id: ClientId,
    weapon: u32,
) -> Result<(), String> {
    if !has_weapon(world, id, weapon) {
        return Err("player does not own that weapon".into());
    }
    set_spawn_weapon(world, id, weapon)?;
    let ps = world
        .player_mut(id)
        .expect("set_spawn_weapon checked player");
    ps.weaponstate_primary = weapon_iw4::WeaponState::Ready as i32;
    ps.weaponstate_secondary = weapon_iw4::WeaponState::Ready as i32;
    ps.weapon_time = 0;
    ps.weapon_time_secondary = 0;
    ps.weapon_delay = 0;
    ps.weapon_delay_secondary = 0;
    weapon_iw4::set_weap_anim(
        &mut ps.weap_anim,
        &mut ps.weap_anim_secondary,
        ps.last_weapon_hand,
        weapon_iw4::weap_anim_event::IDLE,
    );
    let meta = world.client_meta_mut(id);
    meta.weapon_shot_count = 0;
    meta.burst_latch = false;
    meta.burst_latch_secondary = false;
    meta.rechamber_pending = false;
    meta.rechamber_pending_secondary = false;
    meta.pending_brass = [None; 2];
    Ok(())
}

pub(crate) fn switch_to_weapon(world: &mut FrameWorld, id: ClientId, weapon: u32) {
    if world
        .player(id)
        .is_some_and(|ps| ps.weapons.contains(&(weapon as i32)))
    {
        world.client_meta_mut(id).controls.switch_to = weapon;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WeaponList {
    All,
    Primaries,
    Offhands,
    Items,
    Exclusives,
}

pub(crate) fn weapons(world: &FrameWorld, id: ClientId, list: WeaponList) -> Vec<u32> {
    let Some(ps) = world.player(id) else {
        return Vec::new();
    };
    ps.weapons
        .iter()
        .filter(|&&w| w > 0)
        .map(|&w| w as u32)
        .filter(|&w| {
            let kind = world.combat_facts_for(w).map_or(0, |f| f.inventory_type);
            match list {
                WeaponList::All => true,
                WeaponList::Primaries => kind == 0,
                WeaponList::Offhands => kind == 1,
                WeaponList::Items => kind == 2,
                WeaponList::Exclusives => kind == 4,
            }
        })
        .collect()
}

pub(crate) fn has_weapon(world: &FrameWorld, id: ClientId, weapon: u32) -> bool {
    weapon != 0
        && world
            .player(id)
            .is_some_and(|ps| ps.weapons.contains(&(weapon as i32)))
}

pub(crate) fn ammo_clip(world: &FrameWorld, id: ClientId, weapon: u32) -> i32 {
    let (Some(ps), Some(facts)) = (world.player(id), world.combat_facts_for(weapon)) else {
        return 0;
    };
    let key = weapon_iw4::clip_table_key(facts.clip_index, weapon);
    weapon_iw4::get_clip_for_hand(&ps.ammoclip, key, 0)
}

pub(crate) fn ammo_stock(world: &FrameWorld, id: ClientId, weapon: u32) -> i32 {
    let (Some(ps), Some(facts)) = (world.player(id), world.combat_facts_for(weapon)) else {
        return 0;
    };
    let key = weapon_iw4::ammo_table_key(facts.ammo_index, weapon);
    weapon_iw4::get_ammo_not_in_clip(&ps.ammo, key)
}

pub(crate) fn set_ammo_clip(world: &mut FrameWorld, id: ClientId, weapon: u32, count: i32) {
    let Some(facts) = world.combat_facts_for(weapon) else {
        return;
    };
    let count = count.clamp(0, facts.clip_size.max(0));
    let key = weapon_iw4::clip_table_key(facts.clip_index, weapon);
    if let Some(ps) = world.player_mut(id) {
        let _ = weapon_iw4::set_clip_for_hand(&mut ps.ammoclip, key, 0, count);
    }
    let stock = ammo_stock(world, id, weapon);
    world.client_meta_mut(id).set_ammo(weapon, count, stock);
}

pub(crate) fn set_ammo_stock(world: &mut FrameWorld, id: ClientId, weapon: u32, count: i32) {
    let Some(facts) = world.combat_facts_for(weapon) else {
        return;
    };
    let count = count.clamp(0, facts.max_ammo.max(0));
    let key = weapon_iw4::ammo_table_key(facts.ammo_index, weapon);
    if let Some(ps) = world.player_mut(id) {
        let _ = weapon_iw4::set_ammo_not_in_clip(&mut ps.ammo, key, count);
    }
    let clip = ammo_clip(world, id, weapon);
    world.client_meta_mut(id).set_ammo(weapon, clip, count);
}

pub(crate) fn give_max_ammo(world: &mut FrameWorld, id: ClientId, weapon: u32) {
    let Some(facts) = world.combat_facts_for(weapon) else {
        return;
    };
    if let Some(eq) = world
        .equipment_facts_for(weapon)
        .filter(|eq| eq.is_offhand())
    {
        set_ammo_clip(world, id, weapon, eq.clip_size.max(eq.start_ammo));
        return;
    }
    set_ammo_stock(world, id, weapon, facts.max_ammo);
}

pub(crate) fn give_start_ammo(world: &mut FrameWorld, id: ClientId, weapon: u32) {
    let Some(facts) = world.combat_facts_for(weapon) else {
        return;
    };
    if let Some(eq) = world
        .equipment_facts_for(weapon)
        .filter(|eq| eq.is_offhand())
    {
        set_ammo_clip(world, id, weapon, eq.spawn_clip_count());
        return;
    }
    let (clip, _, stock) = weapon_iw4::spawn_clip_stock(&facts, 0);
    set_ammo_clip(world, id, weapon, clip);
    set_ammo_stock(world, id, weapon, stock);
}

fn perk_bits(name: &str) -> ([u32; 2], u32) {
    let perk = match name {
        "specialty_fastreload" => weapon_iw4::PERK_FASTRELOAD,
        "specialty_coldblooded" => playerstate_iw4::PERK_COLDBLOODED,
        "specialty_lightweight" => weapon_iw4::PERK_LIGHTWEIGHT_VIEW_BOB_BIT,
        "specialty_scavenger" => playerstate_iw4::PERK_SCAVENGER,
        "specialty_quieter" => playerstate_iw4::PERK_QUIETER,
        "specialty_heartbreaker" => playerstate_iw4::PERK_HEARTBREAKER,
        "specialty_marathon" => movement_iw4::PERK_MARATHON,
        "specialty_bulletaccuracy" => weapon_iw4::PERK_BULLETACCURACY,
        "specialty_pistoldeath" => playerstate_iw4::PERK_PISTOLDEATH,
        "specialty_fastmantle" => playerstate_iw4::PERK_FASTMANTLE,
        "specialty_quickdraw" => playerstate_iw4::PERK_QUICKDRAW,
        "specialty_holdbreath" => playerstate_iw4::PERK_HOLDBREATH,
        _ => 0,
    };
    let perk1 = match name {
        "specialty_spygame" => playerstate_iw4::PERK1_SPYGAME,
        _ => 0,
    };
    let e_flags = match name {
        "specialty_localjammer" => playerstate_iw4::eflags::RADAR_JAM,
        _ => 0,
    };
    ([perk, perk1], e_flags)
}

pub(crate) fn set_perk(world: &mut FrameWorld, id: ClientId, name: &str, on: bool) {
    let (perks, e_flags) = perk_bits(name);
    if let Some(ps) = world.player_mut(id) {
        for (word, perk) in ps.perks.iter_mut().zip(perks) {
            if on {
                *word |= perk;
            } else {
                *word &= !perk;
            }
        }
        if on {
            ps.e_flags |= e_flags;
        } else {
            ps.e_flags &= !e_flags;
        }
    }
}

pub(crate) fn clear_perks(world: &mut FrameWorld, id: ClientId) {
    if let Some(ps) = world.player_mut(id) {
        ps.perks = [0; 2];
        ps.perk_slots = [0; 8];
        ps.e_flags &= !playerstate_iw4::eflags::RADAR_JAM;
    }
}

pub(crate) fn stance(world: &FrameWorld, id: ClientId) -> &'static str {
    let flags = world.player(id).map_or(0, |ps| ps.e_flags);
    if flags & playerstate_iw4::eflags::PRONE != 0 {
        "prone"
    } else if flags & playerstate_iw4::eflags::DUCK != 0 {
        "crouch"
    } else {
        "stand"
    }
}

pub(crate) fn buttons(world: &mut FrameWorld, id: ClientId) -> u32 {
    let input = &world.ecs().resource::<crate::step::StepRequest>().input;
    let mut commands = input
        .cmds
        .iter()
        .filter(|command| command.client == id)
        .peekable();
    if commands.peek().is_some() {
        return commands.fold(0, |buttons, command| buttons | command.command.buttons);
    }
    world
        .old_buttons_mut()
        .iter()
        .find(|(c, _)| *c == id)
        .map_or(0, |(_, b)| *b)
}

pub(crate) fn constrain_cmd(
    world: &mut FrameWorld,
    id: ClientId,
    cmd: &mut playerstate_iw4::UserCmd,
) {
    use playerstate_iw4::buttons;
    let held = world.player(id).map_or(0, |ps| ps.weapon);
    let Some(mut controls) = world.client_meta(id).map(|m| m.controls) else {
        return;
    };
    if controls.switch_to != 0 && controls.switch_to == held {
        controls.switch_to = 0;
        world.client_meta_mut(id).controls.switch_to = 0;
    }
    if controls.frozen {
        cmd.forwardmove = 0;
        cmd.rightmove = 0;
        cmd.buttons &= buttons::PRONE | buttons::CROUCH | buttons::STANCE_HELD;
    }
    if controls.linked {
        cmd.forwardmove = 0;
        cmd.rightmove = 0;
        cmd.buttons &= !(buttons::JUMP | buttons::SPRINT);
    }
    if controls.jump_disabled {
        cmd.buttons &= !buttons::JUMP;
    }
    if controls.weapons_disabled {
        cmd.buttons &= !(buttons::ATTACK | buttons::THROW | buttons::ADS | buttons::MELEE_CHARGE);
    }
    if controls.offhands_disabled || controls.weapons_disabled {
        cmd.buttons &= !(buttons::FRAG | buttons::SMOKE);
    }
    if controls.usability_disabled {
        cmd.buttons &= !(buttons::USE | buttons::USE_RELOAD);
    }
    if controls.switch_to != 0 {
        cmd.weapon = controls.switch_to as u16;
    } else if controls.switch_disabled || controls.frozen {
        cmd.weapon = held as u16;
    }
}
