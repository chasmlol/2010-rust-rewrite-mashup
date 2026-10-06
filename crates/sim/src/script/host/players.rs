use crate::frame::FrameWorld;
use crate::script::runtime::{raise, run_now};
use crate::script::{Arc, BTreeMap, Fault, Location, Runtime, Value, VecDeque};
use crate::world::ClientId;
use bevy_ecs::prelude::World;

const CONNECT: &str = "maps/mp/gametypes/_callbacksetup::codecallback_playerconnect";
const DISCONNECT: &str = "maps/mp/gametypes/_callbacksetup::codecallback_playerdisconnect";
const DAMAGE: &str = "maps/mp/gametypes/_callbacksetup::codecallback_playerdamage";
pub(crate) const KILLED: &str = "maps/mp/gametypes/_callbacksetup::codecallback_playerkilled";
pub(crate) const LAST_STAND: &str =
    "maps/mp/gametypes/_callbacksetup::codecallback_playerlaststand";

pub(crate) fn now_ms(world: &World) -> i64 {
    i64::from(world.resource::<crate::step::StepRequest>().tick.0) * i64::from(crate::MATCH_TICK_MS)
}

pub(crate) fn player_object(world: &World, client: u32) -> Value {
    world
        .resource::<Runtime>()
        .players
        .get(&client)
        .map_or(Value::Undefined, |slot| Value::Object(slot.object))
}

pub(crate) fn player_damage(world: &mut World, tick: crate::Tick, hit: &crate::script_player::Hit) {
    if crate::script_player::god_mode(&FrameWorld::from_world(world), hit.victim) {
        return;
    }
    let victim = player_object(world, hit.victim.0);
    let Value::Object(object) = victim else {
        return;
    };
    let runtime = world.resource::<Runtime>();
    if !runtime.entities[&object].accepts_damage(hit.flags)
        || runtime.engine.players_ignore_radius_damage
            && hit.flags & crate::script_player::IDFLAGS_RADIUS != 0
    {
        return;
    }
    let attacker = match hit.attacker.map(|a| player_object(world, a.0)) {
        Some(attacker) if attacker != Value::Undefined => attacker,
        _ => world_entity(world),
    };
    let weapon = event_weapon(world, hit.attacker.map_or(u32::MAX, |a| a.0), hit.weapon);
    let weapon = crate::script_player::weapon_name(&FrameWorld::from_world(world), weapon);
    let hitloc = weapon_iw4::HITLOC_NAMES
        .get(usize::from(hit.hitloc))
        .copied()
        .unwrap_or("none");
    let inflictor = hit
        .inflictor
        .and_then(|id| projectile_entity(world, id, hit))
        .unwrap_or_else(|| attacker.clone());
    let args = vec![
        inflictor,
        attacker,
        Value::Int(hit.amount),
        Value::Int(hit.flags),
        Value::string(hit.means),
        Value::String(weapon.into()),
        Value::Vector(hit.point),
        Value::Vector(hit.dir),
        Value::string(hitloc),
        Value::Int(0),
    ];
    world.resource_mut::<Runtime>().current_hit = Some(hit.clone());
    let now = i64::from(tick.0) * i64::from(crate::MATCH_TICK_MS);
    let result = run_now(world, DAMAGE, victim, args, now);
    world.resource_mut::<Runtime>().current_hit = None;
    if result.is_ok() {
        // Deaths from this hit are settled before the caller continues.
        settle_deaths(world);
    }
}

pub(crate) fn crush_player(world: &mut World, victim: ClientId, pusher: u64, point: [f32; 3]) {
    if crate::script_player::god_mode(&FrameWorld::from_world(world), victim) {
        return;
    }
    let target = player_object(world, victim.0);
    let Value::Object(object) = target else {
        return;
    };
    let runtime = world.resource::<Runtime>();
    if !runtime.entities[&object].accepts_damage(0) || !runtime.entities.contains_key(&pusher) {
        return;
    }
    let args = vec![
        Value::Object(pusher),
        Value::Object(pusher),
        Value::Int(99999),
        Value::Int(0),
        Value::string("MOD_CRUSH"),
        Value::string("none"),
        Value::Vector(point),
        Value::Vector([0.0, 0.0, -1.0]),
        Value::string("none"),
        Value::Int(0),
    ];
    let now = now_ms(world);
    if run_now(world, DAMAGE, target, args, now).is_ok() {
        settle_deaths(world);
    }
}

fn world_entity(world: &World) -> Value {
    world
        .resource::<Runtime>()
        .engine
        .world
        .map_or(Value::Undefined, Value::Object)
}

pub(crate) fn damage_entity(world: &World, value: Option<&Value>) -> Value {
    let runtime = world.resource::<Runtime>();
    match value {
        Some(Value::Object(object))
            if runtime.entities.contains_key(object)
                || runtime.player_client(*object).is_some() =>
        {
            Value::Object(*object)
        }
        _ => world_entity(world),
    }
}

fn projectile_entity(
    world: &mut World,
    id: crate::ProjectileId,
    hit: &crate::script_player::Hit,
) -> Option<Value> {
    let mut runtime = world.resource_mut::<Runtime>();
    let object = runtime
        .entities
        .iter()
        .find(|(_, e)| matches!(e.kind, super::entities::EntityKind::Missile(of) if of == id))
        .map(|(object, _)| *object)?;
    if hit.flags & crate::script_player::IDFLAGS_RADIUS != 0 {
        runtime.set_object_field(object, "origin", Value::Vector(hit.point));
    }
    Some(Value::Object(object))
}

pub(crate) fn flashbang(
    world: &mut World,
    target: ClientId,
    origin: [f32; 3],
    amount_distance: f32,
    amount_angle: f32,
    attacker: ClientId,
) {
    let victim = player_object(world, target.0);
    if victim == Value::Undefined {
        return;
    }
    let attacker = player_object(world, attacker.0);
    raise(
        world,
        victim,
        "flashbang",
        vec![
            Value::Vector(origin),
            Value::Float(amount_distance.into()),
            Value::Float(amount_angle.into()),
            attacker,
        ],
    );
}

pub(crate) fn owe(world: &mut World, client: u32, callback: &'static str, args: Vec<Value>) {
    world
        .resource_mut::<Runtime>()
        .deaths
        .push_back((client, callback, args));
}

pub(crate) fn suicide(world: &mut World, tick: crate::Tick, client: u32) {
    let id = crate::ClientId(client);
    let mut frame = FrameWorld::from_world(world);
    if !frame
        .client_meta(id)
        .is_some_and(|m| m.lifecycle == crate::ClientLifecycle::Alive)
    {
        return;
    }
    crate::script_player::kill(&mut frame, tick, id, Some(id), None);
    let me = player_object(world, client);
    owe(
        world,
        client,
        KILLED,
        vec![
            me.clone(),
            me,
            Value::Int(100_000),
            Value::string("MOD_SUICIDE"),
            Value::string("none"),
            Value::Vector([0.0; 3]),
            Value::string("none"),
            Value::Int(0),
            Value::Int(0),
        ],
    );
}

pub(crate) fn force_death(world: &mut World, tick: crate::Tick, client: u32) {
    suicide(world, tick, client);
    settle_deaths(world);
}

fn profile_data(world: &World, profile: crate::PlayerProfile) -> Option<Vec<(Vec<Value>, Value)>> {
    let tables = &world.resource::<Runtime>().tables;
    let titles = super::tables::table(tables, "mp/cardTitleTable.csv")?;
    let emblems = super::tables::table(tables, "mp/cardIconTable.csv")?;
    let streaks = super::tables::table(tables, "mp/killstreakTable.csv")?;
    let title = titles
        .cell(profile.title as usize, 0)
        .filter(|s| !s.is_empty())?;
    let emblem = emblems
        .cell(profile.emblem as usize, 0)
        .filter(|s| !s.is_empty())?;
    let mut data = vec![
        (vec![Value::string("cardTitle")], Value::string(title)),
        (vec![Value::string("cardIcon")], Value::string(emblem)),
    ];
    let mut costs = Vec::new();
    for (slot, row) in profile.killstreaks.into_iter().enumerate() {
        let row = row as usize;
        let cost = streaks.cell(row, 4)?.parse::<u32>().ok()?;
        if !(1..=25).contains(&cost) || costs.contains(&cost) {
            return None;
        }
        costs.push(cost);
        let name = streaks.cell(row, 1)?;
        if matches!(name, "none" | "sentry") {
            return None;
        }
        data.push((
            vec![Value::string("killstreaks"), Value::Int(slot as i32)],
            Value::string(name),
        ));
    }
    Some(data)
}

pub(crate) fn set_profile(world: &mut World, client: u32, profile: crate::PlayerProfile) -> bool {
    if FrameWorld::from_world(world)
        .client_meta(ClientId(client))
        .is_none()
    {
        return false;
    }
    let Some(data) = profile_data(world, profile) else {
        return false;
    };
    if super::natives::player::write_class_data(world, client, &data).is_err() {
        return false;
    }
    let mut frame = FrameWorld::from_world(world);
    let meta = frame.client_meta_mut(ClientId(client));
    meta.player_card_title = profile.title;
    meta.player_card_icon = profile.emblem;
    true
}

const GIVE_KILLSTREAK: &str = "maps/mp/killstreaks/_killstreaks::trygivekillstreak";

pub(crate) fn give_killstreak(world: &mut World, client: u32, name: &str) -> bool {
    let me = player_object(world, client);
    if me == Value::Undefined {
        return false;
    }
    let lookup = [
        Value::string("mp/killstreakTable.csv"),
        Value::Int(1),
        Value::string(name),
        Value::Int(4),
    ];
    let Ok(Ok(cost)) = super::tables::table_lookup(world, &lookup).map(|c| c.parse::<i32>()) else {
        return false;
    };
    let now = now_ms(world);
    run_now(
        world,
        GIVE_KILLSTREAK,
        me,
        vec![Value::string(name), Value::Int(cost)],
        now,
    )
    .is_ok()
}

pub(crate) fn settle_deaths(world: &mut World) {
    let now = now_ms(world);
    loop {
        let Some((client, callback, args)) = world.resource_mut::<Runtime>().deaths.pop_front()
        else {
            return;
        };
        let victim = player_object(world, client);
        if victim == Value::Undefined {
            continue;
        }
        if callback == KILLED {
            let attacker = args.get(1).cloned().unwrap_or(Value::Undefined);
            raise(world, victim.clone(), "death", vec![attacker]);
        }
        if run_now(world, callback, victim, args, now).is_err() {
            return;
        }
    }
}

pub(crate) const TEAM_MENU: &str = "team_marinesopfor";
const CLASS_MENU: &str = "changeclass";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MenuAnswer {
    menu: Arc<str>,
    response: Arc<str>,
    data: Vec<(Vec<Value>, Value)>,
}

impl MenuAnswer {
    pub(crate) fn values(&self) -> impl Iterator<Item = &Value> {
        self.data.iter().map(|(_, value)| value)
    }
}

pub(crate) fn answer_menu(world: &mut World, client: u32, menu: &str, response: &str) {
    push_answer(
        world,
        client,
        MenuAnswer {
            menu: menu.into(),
            response: response.into(),
            data: Vec::new(),
        },
    );
}

pub(crate) fn answer_join(world: &mut World, client: u32) {
    if world.resource_mut::<Runtime>().joined.insert(client) {
        answer_menu(world, client, TEAM_MENU, "autoassign");
    }
}

pub(crate) fn note_team_answer(world: &mut World, client: u32, menu: &str) {
    if menu == TEAM_MENU {
        world.resource_mut::<Runtime>().joined.insert(client);
    }
}

fn push_answer(world: &mut World, client: u32, answer: MenuAnswer) {
    world
        .resource_mut::<Runtime>()
        .menu_answers
        .entry(client)
        .or_default()
        .push_back(answer);
}

const T5_DEFAULT_CLASSES: [&str; 5] = ["smg_mp", "cqb_mp", "assault_mp", "lmg_mp", "sniper_mp"];

pub(crate) fn is_t5(world: &World) -> bool {
    world
        .resource::<Runtime>()
        .program
        .as_ref()
        .is_some_and(|p| p.rules() == crate::script::Realm::T5)
}

pub(crate) fn choose_default_class(world: &mut World, client: u32, index: u8) {
    world
        .resource_mut::<Runtime>()
        .selected_classes
        .remove(&client);
    let realm = world
        .resource::<Runtime>()
        .program
        .as_ref()
        .map(|p| p.rules());
    let response = match realm {
        Some(crate::script::Realm::T5) => {
            T5_DEFAULT_CLASSES[index as usize % T5_DEFAULT_CLASSES.len()].to_owned()
        }
        _ => format!("class{}", index % 5),
    };
    answer_menu(world, client, CLASS_MENU, &response);
}

pub(crate) fn t5_class_response(class: crate::ClassId) -> Option<String> {
    match class.0 {
        slot @ 0..5 => Some(format!("custom{}", slot + 1)),
        slot @ 5..10 => Some(format!("prestige{}", slot - 4)),
        _ => None,
    }
}

const IW4_STAND_INS: [&str; 4] = ["m4_mp", "usp_mp", "frag_grenade_mp", "flash_grenade_mp"];
const T5_STAND_INS: [&str; 4] = ["ak47_mp", "m1911_mp", "frag_grenade_mp", "flash_grenade_mp"];

pub(crate) fn stand_in_for(world: &mut World, slot: usize, weapon: u32) -> Option<u32> {
    let realm = world
        .resource::<Runtime>()
        .program
        .as_ref()
        .map_or(crate::script::Realm::Iw4, |p| p.rules());
    let frame = FrameWorld::from_world(world);
    let setup = frame.weapon_setup(weapon).filter(|_| weapon != 0)?;
    if setup.realm == realm {
        return None;
    }
    if realm == crate::script::Realm::Iw4
        && frame
            .weapon_combat_row(weapon)
            .is_some_and(|facts| facts.weap_type == weapon_iw4::WEAPTYPE_SHIELD)
    {
        return frame.weapon_index_by_script_name("riotshield_mp");
    }
    if realm == crate::script::Realm::Iw4
        && let Some(facts) = frame.missile_launch_facts(weapon)
        && facts.require_lock_to_fire
    {
        let launcher = match facts.missile_guidance {
            1 => Some("stinger_mp"),
            3 => Some("javelin_mp"),
            _ => None,
        };
        if let Some(launcher) = launcher {
            return frame.weapon_index_by_script_name(launcher);
        }
    }
    let stand_ins = if realm == crate::script::Realm::T5 {
        T5_STAND_INS
    } else {
        IW4_STAND_INS
    };
    frame.weapon_index_by_script_name(stand_ins[slot])
}

fn bridge_class_weapon(world: &mut World, client: u32, slot: usize, weapon: u32) {
    let Some(stand_in) = stand_in_for(world, slot, weapon) else {
        return;
    };
    let mut runtime = world.resource_mut::<Runtime>();
    let bridge = runtime.weapon_bridge.entry(client).or_default();
    bridge.retain(|(from, _)| *from != stand_in);
    bridge.push((stand_in, weapon));
}

fn offhand_stand_in(world: &mut World, weapon: u32) -> Option<u32> {
    let realm = world
        .resource::<Runtime>()
        .program
        .as_ref()
        .map_or(crate::script::Realm::Iw4, |p| p.rules());
    let frame = FrameWorld::from_world(world);
    let stand_in = frame
        .weapon_setup(weapon)
        .filter(|setup| setup.realm != realm)
        .and_then(|setup| setup.stand_in.as_deref());
    frame
        .weapon_index_by_script_name(stand_in.unwrap_or(frame.weapon_script_name(weapon)))
        .filter(|&named| weapon != 0 && named != weapon)
}

fn bridge_offhand(world: &mut World, client: u32, weapon: u32) {
    let Some(stand_in) = offhand_stand_in(world, weapon) else {
        return;
    };
    let mut runtime = world.resource_mut::<Runtime>();
    let bridge = runtime.weapon_bridge.entry(client).or_default();
    bridge.retain(|(from, _)| *from != stand_in);
    bridge.push((stand_in, weapon));
}

const OWN_T6_EQUIPMENT: [&str; 5] = [
    "bouncingbetty_mp",
    "trophy_system_mp",
    "sensor_grenade_mp",
    "emp_grenade_mp",
    "proximity_grenade_mp",
];

pub(crate) fn event_weapon(world: &mut World, client: u32, weapon: u32) -> u32 {
    let script = script_weapon(world, client, weapon);
    let frame = FrameWorld::from_world(world);
    if script != weapon && OWN_T6_EQUIPMENT.contains(&frame.weapon_script_name(weapon)) {
        weapon
    } else {
        script
    }
}

const INSERTION: &str = "flare_mp";

const INSERTION_CARRIER: &str = "smoke_grenade_mp";

const GIVE_PERK: &str = "maps/mp/perks/_perks::giveperk";

const INSERTION_GLOW_MODEL: &str = "mil_emergency_flare_mp";

const INSERTION_PLANT_REACH: f32 = 256.0;

const INSERTION_LIGHTS: [(&str, &str); 2] = [
    (
        "misc/flare_ambient_green",
        "misc/fx_equip_tac_insert_light_grn",
    ),
    ("misc/flare_ambient", "misc/fx_equip_tac_insert_light_red"),
];

const INSERTION_LIGHT_REACH: f32 = 16.0;

pub(crate) fn insertion_light(
    world: &World,
    effect: &str,
    origin: [f32; 3],
) -> Option<(&'static str, [f32; 3])> {
    let (_, light) = INSERTION_LIGHTS
        .iter()
        .find(|(flare, _)| *flare == effect)?;
    world
        .resource::<Runtime>()
        .insertion_spots
        .iter()
        .find(|(spot, _)| {
            spot.iter()
                .zip(origin)
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f32>()
                <= INSERTION_LIGHT_REACH * INSERTION_LIGHT_REACH
        })
        .map(|(spot, _)| (*light, *spot))
}

pub(crate) fn note_insertion_throw(
    world: &mut World,
    client: u32,
    script: u32,
    native: u32,
    grenade: u64,
) {
    if script == native || FrameWorld::from_world(world).weapon_script_name(script) != INSERTION {
        return;
    }
    let model: Arc<str> = format!("{}{native}", crate::WEAPON_MODEL_PREFIX).into();
    world
        .resource_mut::<Runtime>()
        .thrown_insertions
        .insert(client, (model, grenade));
}

pub(crate) fn dress_insertion_glow(world: &mut World, object: u64, model: &str) {
    if model != INSERTION_GLOW_MODEL {
        return;
    }
    let origin = match world
        .resource_mut::<Runtime>()
        .object_field(object, "origin")
    {
        Value::Vector(origin) => origin,
        _ => return,
    };
    let near = |a: [f32; 3], b: [f32; 3], reach: f32| {
        a.iter().zip(b).map(|(a, b)| (a - b) * (a - b)).sum::<f32>() <= reach * reach
    };
    let placed = world
        .resource::<Runtime>()
        .insertion_spots
        .iter()
        .find(|(spot, _)| near(*spot, origin, 1.0))
        .map(|(_, model)| model.clone());
    let model = match placed {
        Some(model) => model,
        None => {
            let throwers: Vec<u32> = world
                .resource::<Runtime>()
                .thrown_insertions
                .keys()
                .copied()
                .collect();
            let frame = FrameWorld::from_world(world);
            let thrower = throwers.into_iter().find(|&client| {
                frame
                    .player(ClientId(client))
                    .is_some_and(|ps| near(ps.origin, origin, INSERTION_PLANT_REACH))
            });
            let mut runtime = world.resource_mut::<Runtime>();
            let Some((model, grenade)) =
                thrower.and_then(|client| runtime.thrown_insertions.remove(&client))
            else {
                return;
            };
            if runtime.entities.contains_key(&grenade)
                && !runtime.pending_deletes.contains(&grenade)
            {
                runtime.pending_deletes.push(grenade);
            }
            if runtime.insertion_spots.len() >= 32 {
                runtime.insertion_spots.remove(0);
            }
            runtime.insertion_spots.push((origin, model.clone()));
            model
        }
    };
    let mut runtime = world.resource_mut::<Runtime>();
    let Some(entity) = runtime.entities.get_mut(&object) else {
        return;
    };
    let tag: Arc<str> = "tag_origin".into();
    if !entity.attachments.iter().any(|(m, _)| *m == model) {
        entity.attachments.push((model, tag));
    }
}

fn bridge_insertion_carrier(world: &mut World, client: u32, weapon: u32) {
    let frame = FrameWorld::from_world(world);
    let Some(carrier) = frame.weapon_index_by_script_name(INSERTION_CARRIER) else {
        return;
    };
    let stand_in = offhand_stand_in(world, weapon).unwrap_or(weapon);
    if weapon == 0 || FrameWorld::from_world(world).weapon_script_name(stand_in) != INSERTION {
        return;
    }
    let mut runtime = world.resource_mut::<Runtime>();
    let bridge = runtime.weapon_bridge.entry(client).or_default();
    bridge.retain(|(from, _)| *from != carrier);
    bridge.push((carrier, weapon));
}

fn mark_t6_offhands(world: &mut World, client: u32, class: &crate::ClassDef) {
    let given = |world: &mut World, weapon: u32, tactical: bool| -> Value {
        let Some(stand_in) = offhand_stand_in(world, weapon) else {
            return Value::Undefined;
        };
        let frame = FrameWorld::from_world(world);
        let name = frame.weapon_script_name(stand_in);
        Value::string(if tactical && name == INSERTION {
            INSERTION_CARRIER
        } else {
            name
        })
    };
    let lethal = given(world, class.lethal, false);
    let tactical = given(world, class.tactical, true);
    let Value::Object(player) = player_object(world, client) else {
        return;
    };
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.set_object_field(player, "t6lethal", lethal);
    runtime.set_object_field(player, "t6tactical", tactical);
}

pub(crate) fn give_carried_insertion(
    world: &mut World,
    client: u32,
    receiver: &Value,
    weapon: u32,
) -> Result<(), String> {
    let frame = FrameWorld::from_world(world);
    if frame.weapon_script_name(weapon) != INSERTION_CARRIER
        || bridged_weapon(world, client, weapon) == weapon
    {
        return Ok(());
    }
    crate::script::start(
        world,
        GIVE_PERK,
        receiver.clone(),
        vec![Value::String("specialty_tacticalinsertion".into())],
    )
    .map(|_| ())
    .map_err(|fault| fault.to_string())
}

pub(crate) fn bridged_weapon(world: &World, client: u32, weapon: u32) -> u32 {
    world
        .resource::<Runtime>()
        .weapon_bridge
        .get(&client)
        .and_then(|bridge| bridge.iter().find(|(from, _)| *from == weapon))
        .map_or(weapon, |(_, native)| *native)
}

pub(crate) fn script_weapon(world: &mut World, client: u32, weapon: u32) -> u32 {
    if let Some(stand_in) = world
        .resource::<Runtime>()
        .weapon_bridge
        .get(&client)
        .and_then(|bridge| bridge.iter().find(|(_, native)| *native == weapon))
        .map(|(stand_in, _)| *stand_in)
    {
        return stand_in;
    }
    let needs_lock_bridge = FrameWorld::from_world(world)
        .missile_launch_facts(weapon)
        .is_some_and(|facts| facts.require_lock_to_fire && matches!(facts.missile_guidance, 1 | 3));
    if needs_lock_bridge
        && world.resource::<Runtime>().players.contains_key(&client)
        && let Some(stand_in) = stand_in_for(world, 1, weapon)
    {
        bridge_class_weapon(world, client, 1, weapon);
        return stand_in;
    }
    weapon
}

pub(crate) fn personal_class(
    world: &World,
    client: u32,
    class: crate::ClassId,
) -> Option<crate::ClassDef> {
    world
        .resource::<Runtime>()
        .personal_classes
        .get(&(client, class.0))
        .cloned()
}

pub(crate) fn selected_camouflage(world: &World, client: u32, weapon: u32) -> Option<u8> {
    let runtime = world.resource::<Runtime>();
    let class = runtime
        .personal_classes
        .get(&(client, *runtime.selected_classes.get(&client)?))?;
    [class.primary, class.secondary]
        .iter()
        .position(|own| *own == weapon)
        .map(|slot| class.camos[slot])
}

pub(crate) fn choose_class(world: &mut World, client: u32, class: &crate::ClassDef) {
    world
        .resource_mut::<Runtime>()
        .selected_classes
        .insert(client, class.id.0);
    world
        .resource_mut::<Runtime>()
        .personal_classes
        .insert((client, class.id.0), class.clone());
    let realm = world
        .resource::<Runtime>()
        .program
        .as_ref()
        .map(|p| p.rules());
    world
        .resource_mut::<Runtime>()
        .weapon_bridge
        .remove(&client);
    bridge_class_weapon(world, client, 0, class.primary);
    bridge_class_weapon(world, client, 1, class.secondary);
    if realm == Some(crate::script::Realm::T5) {
        bridge_class_weapon(world, client, 2, class.lethal);
        bridge_class_weapon(world, client, 3, class.tactical);
        if let Some(response) = t5_class_response(class.id) {
            answer_menu(world, client, CLASS_MENU, &response);
        }
        return;
    }
    bridge_offhand(world, client, class.lethal);
    bridge_offhand(world, client, class.tactical);
    bridge_insertion_carrier(world, client, class.tactical);
    mark_t6_offhands(world, client, class);
    let data = class_profile_data(world, class);
    push_answer(
        world,
        client,
        MenuAnswer {
            menu: CLASS_MENU.into(),
            response: format!("custom{}", class.id.0 as usize % 10 + 1).into(),
            data,
        },
    );
}

fn class_profile_data(world: &mut World, class: &crate::ClassDef) -> Vec<(Vec<Value>, Value)> {
    let weapons = [
        stand_in_for(world, 0, class.primary).unwrap_or(class.primary),
        stand_in_for(world, 1, class.secondary).unwrap_or(class.secondary),
    ];
    let lethal = offhand_stand_in(world, class.lethal).unwrap_or(class.lethal);
    let tactical = offhand_stand_in(world, class.tactical).unwrap_or(class.tactical);
    let index = class.id.0 as usize % 10;
    let frame = FrameWorld::from_world(world);
    let name = |weapon: u32| -> String {
        if weapon == 0 {
            "none".into()
        } else {
            frame.weapon_script_name(weapon).to_owned()
        }
    };
    let setup = |weapon: u32| -> (String, [String; 2]) {
        let mut attachments = [String::from("none"), String::from("none")];
        let Some(setup) = frame.weapon_setup(weapon) else {
            let full = name(weapon);
            return (
                full.strip_suffix("_mp").unwrap_or(&full).to_owned(),
                attachments,
            );
        };
        for (slot, attachment) in attachments.iter_mut().zip(&setup.attachments) {
            slot.clone_from(attachment);
        }
        (setup.base.clone(), attachments)
    };
    let prefix = format!("customClasses.{index}");
    let mut data = Vec::new();
    let mut put = |key: String, value: &str| {
        let keys = key
            .split('.')
            .map(|part| {
                part.parse::<i32>()
                    .map(Value::Int)
                    .unwrap_or_else(|_| Value::string(part))
            })
            .collect();
        data.push((keys, Value::String(value.into())));
    };
    for (setup_index, weapon) in weapons.into_iter().enumerate() {
        let (base, attachments) = setup(weapon);
        let key = format!("{prefix}.weaponSetups.{setup_index}");
        put(format!("{key}.weapon"), &base);
        put(format!("{key}.attachment.0"), &attachments[0]);
        put(format!("{key}.attachment.1"), &attachments[1]);
        let camo = class.camos.get(setup_index).copied().unwrap_or(0);
        put(
            format!("{key}.camo"),
            crate::match_state::IW4_CAMOS
                .get(usize::from(camo))
                .copied()
                .unwrap_or("none"),
        );
    }
    let equipment = match name(lethal) {
        _ if lethal == 0 => "specialty_null".to_owned(),
        insertion if insertion == INSERTION => "specialty_tacticalinsertion".to_owned(),
        other => other,
    };
    put(format!("{prefix}.perks.0"), &equipment);
    let mut perks = ["specialty_null"; 3];
    for id in class.perks {
        if let (Some(slot), Some(perk)) = (
            crate::match_state::perk_slot_from_class_catalog(id),
            crate::match_state::class_catalog_perk_name(id),
        ) {
            perks[slot] = perk;
        }
    }
    for (slot, perk) in perks.iter().enumerate() {
        put(format!("{prefix}.perks.{}", slot + 1), perk);
    }
    let deathstreak = if class.deathstreak.is_empty() {
        "specialty_null"
    } else {
        &class.deathstreak
    };
    put(format!("{prefix}.perks.4"), deathstreak);
    let tactical = match name(tactical) {
        insertion if insertion == INSERTION => INSERTION_CARRIER.to_owned(),
        other => other,
    };
    put(
        format!("{prefix}.specialGrenade"),
        tactical.strip_suffix("_mp").unwrap_or(&tactical),
    );

    data
}

fn menu_kind(menu: &str) -> Option<bool> {
    if menu == TEAM_MENU {
        Some(false)
    } else if menu.starts_with(CLASS_MENU) {
        Some(true)
    } else {
        None
    }
}

fn deliver_answers(world: &mut World, client: u32) {
    // Answers queued ahead of the open menu's kind were for a menu the scripts skipped.
    let runtime = world.resource::<Runtime>();
    let Some(slot) = runtime.players.get(&client) else {
        return;
    };
    if !slot.begun {
        return;
    }
    let skipped = slot
        .menu
        .as_deref()
        .and_then(menu_kind)
        .and_then(|open| {
            runtime
                .menu_answers
                .get(&client)?
                .iter()
                .position(|answer| menu_kind(&answer.menu) == Some(open))
        })
        .unwrap_or(0);
    if skipped > 0 {
        let mut runtime = world.resource_mut::<Runtime>();
        if let Some(queue) = runtime.menu_answers.get_mut(&client) {
            queue.drain(..skipped);
        }
    }
    let runtime = world.resource::<Runtime>();
    let Some(slot) = runtime.players.get(&client) else {
        return;
    };
    if runtime
        .menu_answers
        .get(&client)
        .is_none_or(VecDeque::is_empty)
    {
        return;
    }
    let in_game = matches!(&*slot.sessionstate, "playing" | "dead");
    if slot.menu.is_none() && !in_game {
        return;
    }
    let object = slot.object;
    let mut runtime = world.resource_mut::<Runtime>();
    let answer = runtime
        .menu_answers
        .get_mut(&client)
        .and_then(VecDeque::pop_front)
        .expect("checked above");
    let slot = runtime.players.get_mut(&client).expect("checked above");
    slot.menu = None;
    if let Err(message) = super::natives::player::write_class_data(world, client, &answer.data) {
        world.resource_mut::<Runtime>().fault = Some(Fault::at(
            &Location {
                module: "<engine>".into(),
                function: "class selection".into(),
                line: 0,
                column: 0,
            },
            message,
        ));
        return;
    }
    raise(
        world,
        Value::Object(object),
        "menuresponse",
        vec![
            Value::String(answer.menu.into()),
            Value::String(answer.response.into()),
        ],
    );
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlayerSlot {
    pub object: u64,
    pub begun: bool,
    pub sessionstate: Arc<str>,
    pub dvars: BTreeMap<Arc<str>, Arc<str>>,
    pub menu: Option<Arc<str>>,
    pub commands: Vec<(Arc<str>, Arc<str>)>,
    pub presented: BTreeMap<&'static str, Vec<Value>>,
    pub perks: std::collections::BTreeSet<Arc<str>>,
    pub spectate: BTreeMap<Arc<str>, bool>,
    pub spectator: super::spectators::Spectator,
    pub seat: crate::ScriptSeat,
    pub weapon: u32,
    pub switching: bool,
    pub last_stand_until_ms: Option<i64>,
    pub has_radar: bool,
    pub radar_mode: crate::RadarMode,
    pub radar_blocked: bool,
    pub link: Option<PlayerLink>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LinkView {
    WeaponDelta,
    Free,
    Delta,
    Absolute,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlayerLink {
    pub parent: u64,
    pub tag: Option<Arc<str>>,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub view: LinkView,
    pub clamp: Option<[f32; 4]>,
    pub parent_axis: [[f32; 3]; 3],
    pub view_fraction: f32,
    pub use_tag_angles: bool,
    pub link_axis: [[f32; 3]; 3],
    pub restore_view: Option<[f32; 3]>,
}

impl PlayerSlot {
    fn new(object: u64) -> Self {
        Self {
            object,
            begun: false,
            sessionstate: "spectator".into(),
            dvars: BTreeMap::new(),
            menu: None,
            commands: Vec::new(),
            presented: BTreeMap::new(),
            perks: Default::default(),
            spectate: BTreeMap::new(),
            spectator: Default::default(),
            seat: crate::ScriptSeat::default(),
            weapon: 0,
            switching: false,
            last_stand_until_ms: None,
            has_radar: false,
            radar_mode: crate::RadarMode::Normal,
            radar_blocked: false,
            link: None,
        }
    }
}

pub(crate) fn script_seats(world: &World) -> Vec<(ClientId, crate::ScriptSeat)> {
    world
        .resource::<Runtime>()
        .players
        .iter()
        .filter(|(_, slot)| &*slot.sessionstate == "spectator" && slot.spectator.target.is_some())
        .map(|(client, slot)| {
            let mut seat = slot.seat;
            seat.spectator_client = slot.spectator.target.unwrap() as i32;
            (ClientId(*client), seat)
        })
        .collect()
}

const SEAT_FIELDS: [&str; 6] = [
    "forcespectatorclient",
    "killcamentity",
    "killcamentitylookat",
    "archivetime",
    "psoffsettime",
    "killcamlength",
];

const RADAR_FIELDS: [&str; 3] = ["hasradar", "radarmode", "isradarblocked"];

pub(crate) fn publish_radar(world: &mut World) {
    let rows: Vec<(u32, bool, crate::RadarMode, bool)> = world
        .resource::<Runtime>()
        .players
        .iter()
        .map(|(client, slot)| (*client, slot.has_radar, slot.radar_mode, slot.radar_blocked))
        .collect();
    for (client, has, mode, blocked) in rows {
        let team = match load_field(world, client, "sessionteam") {
            Some(Value::String(team)) => team.to_string(),
            _ => String::new(),
        };
        let engine = &world.resource::<Runtime>().engine;
        let team_on = engine.team_radar.get(&team).is_some_and(|on| *on != 0);
        let team_blocked = engine.team_radar_blocked.contains(&team);
        let radar = if (has || team_on) && !blocked && !team_blocked {
            mode
        } else {
            crate::RadarMode::Off
        };
        let mut frame = FrameWorld::from_world(world);
        if frame.client_meta(ClientId(client)).is_some() {
            let meta = frame.client_meta_mut(ClientId(client));
            meta.radar = radar;
            meta.radar_blocked = blocked || team_blocked;
        }
    }
}

fn load_seat_field(seat: &crate::ScriptSeat, name: &str) -> Value {
    match name {
        "forcespectatorclient" => Value::Int(seat.spectator_client),
        "killcamentity" => Value::Int(seat.kill_cam_entity),
        "killcamentitylookat" => Value::Int(seat.look_at_entity),
        "archivetime" => Value::Float(seat.archive_ms as f32 / 1000.0),
        "psoffsettime" => Value::Int(seat.ps_offset_ms),
        _ => Value::Float(seat.length_ms as f32 / 1000.0),
    }
}

fn store_seat_field(seat: &mut crate::ScriptSeat, name: &str, value: &Value) -> Result<(), String> {
    let number = match value {
        Value::Int(n) => *n as f32,
        Value::Float(f) => *f,
        Value::Undefined => -1.0,
        other => return Err(format!("player field {name} takes a number, not {other:?}")),
    };
    let ms = (number.max(0.0) * 1000.0).round() as i32;
    match name {
        "forcespectatorclient" => seat.spectator_client = number as i32,
        "killcamentity" => seat.kill_cam_entity = number as i32,
        "killcamentitylookat" => seat.look_at_entity = number as i32,
        "archivetime" => seat.archive_ms = ms,
        "psoffsettime" => seat.ps_offset_ms = number as i32,
        _ => seat.length_ms = ms,
    }
    Ok(())
}

pub(crate) fn apply_disconnects(world: &mut World) {
    if !world
        .resource::<crate::step::StepRequest>()
        .reason
        .advances_authority_world()
    {
        return;
    }
    let disconnects = std::mem::take(&mut world.resource_mut::<Runtime>().disconnects);
    for client in disconnects {
        FrameWorld::from_world(world).retire_client(ClientId(client));
    }
}

pub(crate) fn disconnect_player(world: &mut World, client: u32) {
    super::triggers::release_client_claims(world, client);
    {
        let mut runtime = world.resource_mut::<Runtime>();
        runtime
            .personal_classes
            .retain(|(owner, _), _| *owner != client);
        runtime.weapon_bridge.remove(&client);
        runtime.selected_classes.remove(&client);
        if runtime.local_presentation_client == Some(ClientId(client)) {
            runtime.pending_local_dvars.clear();
        }
    }
    let Some(slot) = world.resource::<Runtime>().players.get(&client).cloned() else {
        return;
    };
    let now = now_ms(world);
    let _ = run_now(
        world,
        DISCONNECT,
        Value::Object(slot.object),
        Vec::new(),
        now,
    );
    world
        .resource_mut::<crate::PersistentDataStore>()
        .unbind(crate::ClientId(client));
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.players.remove(&client);
    if runtime.local_presentation_client == Some(ClientId(client)) {
        runtime.pending_local_dvars.clear();
    }
    runtime.menu_answers.remove(&client);
    runtime.joined.remove(&client);
    runtime.delete_entity(slot.object);
    let owned: Vec<u64> = runtime
        .entities
        .iter()
        .filter(|(_, e)| e.audience == super::entities::HudAudience::Client(client))
        .map(|(id, _)| *id)
        .collect();
    drop(runtime);
    for id in owned {
        super::hud::destroy(world, id);
    }
}

pub(crate) fn sync_players(world: &mut World) {
    let request = world.resource::<crate::step::StepRequest>();
    if !request.reason.advances_authority_world() {
        return;
    }
    let now = i64::from(request.tick.0) * i64::from(crate::MATCH_TICK_MS);
    let runtime = world.resource::<Runtime>();
    if runtime.program.is_none() || runtime.fault.is_some() || !runtime.started {
        return;
    }
    let clients: Vec<(u32, bool)> = {
        let frame = FrameWorld::from_world(world);
        frame
            .client_ids_sorted()
            .into_iter()
            .map(|id| {
                let joined = frame
                    .client_meta(id)
                    .is_some_and(|m| m.lifecycle != crate::ClientLifecycle::Connecting);
                (id.0, joined)
            })
            .collect()
    };
    for (client, joined) in clients {
        let slot = world.resource::<Runtime>().players.get(&client).cloned();
        match slot {
            Some(slot) if !slot.begun && joined => {
                raise(world, Value::Object(slot.object), "begin", Vec::new());
                if let Some(slot) = world.resource_mut::<Runtime>().players.get_mut(&client) {
                    slot.begun = true;
                }
            }
            Some(_) => {}
            None => {
                let mut runtime = world.resource_mut::<Runtime>();
                let object = match runtime.create_player(client) {
                    Ok(object) => object,
                    Err(message) => {
                        runtime.fault = Some(Fault::at(
                            &Location {
                                module: "<engine>".into(),
                                function: "connect".into(),
                                line: 0,
                                column: 0,
                            },
                            message,
                        ));
                        return;
                    }
                };
                runtime.players.insert(client, PlayerSlot::new(object));
                let kept = runtime.restored_pers.remove(&client);
                let pers = match kept {
                    Some(kept) => super::restart::attach(&mut runtime, kept),
                    None => super::arrays::new_array(world, Vec::new()),
                };
                match pers {
                    Ok(pers) => world
                        .resource_mut::<Runtime>()
                        .set_object_field(object, "pers", pers),
                    Err(_) => return,
                }
                if run_now(world, CONNECT, Value::Object(object), Vec::new(), now).is_err() {
                    return;
                }
            }
        }
        deliver_answers(world, client);
    }
    settle_deaths(world);
}

pub(crate) fn team_name(team: i32) -> &'static str {
    match team {
        entity_iw4::TEAM_AXIS => "axis",
        entity_iw4::TEAM_ALLIES => "allies",
        entity_iw4::TEAM_SPECTATOR => "spectator",
        _ => "none",
    }
}

fn client_name(name: &[u8]) -> String {
    let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
    String::from_utf8_lossy(&name[..end]).into_owned()
}

pub(crate) fn load_field(world: &mut World, client: u32, name: &str) -> Option<Value> {
    let id = ClientId(client);
    if name == "sessionstate" {
        let runtime = world.resource::<Runtime>();
        return runtime
            .players
            .get(&client)
            .map(|slot| Value::String(slot.sessionstate.clone().into()));
    }
    if SEAT_FIELDS.contains(&name) {
        let runtime = world.resource::<Runtime>();
        return runtime
            .players
            .get(&client)
            .map(|slot| load_seat_field(&slot.seat, name));
    }
    if RADAR_FIELDS.contains(&name) {
        let slot = world.resource::<Runtime>().players.get(&client)?;
        return Some(match name {
            "hasradar" => Value::Int(slot.has_radar.into()),
            "isradarblocked" => Value::Int(slot.radar_blocked.into()),
            _ => Value::string(match slot.radar_mode {
                crate::RadarMode::Fast => "fast_radar",
                crate::RadarMode::Constant => "constant_radar",
                _ => "normal_radar",
            }),
        });
    }
    let frame = FrameWorld::from_world(world);
    let ps = frame.player(id);
    let meta = frame.client_meta(id);
    Some(match name {
        "origin" => Value::Vector(ps.map_or([0.0; 3], |ps| ps.origin)),
        "angles" => Value::Vector(ps.map_or([0.0; 3], |ps| ps.viewangles)),
        "health" => Value::Int(ps.map_or(0, |ps| ps.health)),
        "maxhealth" => Value::Int(
            meta.map(|m| m.max_health)
                .filter(|&n| n > 0)
                .unwrap_or(ps.map_or(0, |ps| ps.max_health)),
        ),
        "name" => Value::String(client_name(&meta?.name).into()),
        "score" => Value::Int(meta?.score),
        "kills" => Value::Int(meta?.kills),
        "deaths" => Value::Int(meta?.deaths),
        "sessionteam" => Value::string(team_name(meta?.client_state_team)),
        _ => return None,
    })
}

pub(crate) fn entity_field(world: &mut World, id: u64, name: &str) -> Value {
    if let Some(client) = world.resource::<Runtime>().player_client(id)
        && let Some(value) = load_field(world, client, name)
    {
        return value;
    }
    world.resource_mut::<Runtime>().object_field(id, name)
}

pub(crate) fn alive_on_team(world: &mut World, team: &str) -> i32 {
    let clients: Vec<u32> = world
        .resource::<Runtime>()
        .players
        .iter()
        .filter(|(_, slot)| &*slot.sessionstate == "playing")
        .map(|(client, _)| *client)
        .collect();
    let mut alive = 0;
    for client in clients {
        let on_team = load_field(world, client, "sessionteam") == Some(Value::string(team));
        let health = load_field(world, client, "health");
        if on_team && matches!(health, Some(Value::Int(n)) if n > 0) {
            alive += 1;
        }
    }
    alive
}

pub(crate) fn store_field(
    world: &mut World,
    client: u32,
    name: &str,
    value: &Value,
) -> Result<bool, String> {
    let id = ClientId(client);
    let int = |value: &Value| match value {
        Value::Int(n) => Ok(*n),
        Value::Float(f) => Ok(*f as i32),
        other => Err(format!("player field {name} takes a number, not {other:?}")),
    };
    if SEAT_FIELDS.contains(&name) {
        if let Some(slot) = world.resource_mut::<Runtime>().players.get_mut(&client) {
            store_seat_field(&mut slot.seat, name, value)?;
        }
        return Ok(true);
    }
    if RADAR_FIELDS.contains(&name) {
        let mode = match (name, value) {
            ("radarmode", Value::String(mode)) => Some(match &**mode {
                "normal_radar" => crate::RadarMode::Normal,
                "fast_radar" => crate::RadarMode::Fast,
                "constant_radar" => crate::RadarMode::Constant,
                other => return Err(format!("invalid radarmode '{other}'")),
            }),
            ("radarmode", other) => return Err(format!("radarmode takes a string, not {other:?}")),
            _ => None,
        };
        let on = match mode {
            Some(_) => false,
            None => int(value)? != 0,
        };
        if let Some(slot) = world.resource_mut::<Runtime>().players.get_mut(&client) {
            match (name, mode) {
                (_, Some(mode)) => slot.radar_mode = mode,
                ("hasradar", _) => slot.has_radar = on,
                _ => slot.radar_blocked = on,
            }
        }
        return Ok(true);
    }
    match name {
        "sessionstate" => {
            let Value::String(state) = value else {
                return Err("sessionstate takes a string".into());
            };
            if !matches!(&**state, "playing" | "dead" | "spectator" | "intermission") {
                return Err(format!("invalid sessionstate '{state}'"));
            }
            if let Some(slot) = world.resource_mut::<Runtime>().players.get_mut(&client) {
                slot.sessionstate = state.clone().into();
            }
        }
        "origin" | "angles" => {
            let Value::Vector(v) = value else {
                return Err(format!("player field {name} takes a vector"));
            };
            let mut frame = FrameWorld::from_world(world);
            if name == "origin" {
                frame.set_origin(id, *v);
            } else {
                frame.set_viewangles(id, *v);
            }
        }
        "health" => {
            let n = int(value)?;
            if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
                ps.health = n;
            }
        }
        "maxhealth" => {
            let n = int(value)?.max(1);
            let mut frame = FrameWorld::from_world(world);
            frame.client_meta_mut(id).max_health = n;
            if let Some(ps) = frame.player_mut(id) {
                ps.max_health = n;
                ps.health = ps.health.min(n);
            }
        }
        "score" | "kills" | "deaths" => {
            let n = int(value)?;
            let mut frame = FrameWorld::from_world(world);
            if frame.client_meta(id).is_some() {
                let meta = frame.client_meta_mut(id);
                match name {
                    "score" => meta.score = n,
                    "kills" => meta.kills = n,
                    _ => meta.deaths = n,
                }
            }
        }
        "sessionteam" | "team" => {
            let Value::String(team) = value else {
                if name == "team" {
                    return Ok(false);
                }
                return Err("sessionteam takes a string".into());
            };
            let team = match &**team {
                "axis" => entity_iw4::TEAM_AXIS,
                "allies" => entity_iw4::TEAM_ALLIES,
                "spectator" => entity_iw4::TEAM_SPECTATOR,
                "none" => entity_iw4::TEAM_FREE,
                _ if name == "team" => return Ok(false),
                other => return Err(format!("invalid sessionteam '{other}'")),
            };
            let mut frame = FrameWorld::from_world(world);
            let team = if name == "team"
                && !frame.bootstrap_ref().kind.is_team()
                && team != entity_iw4::TEAM_SPECTATOR
            {
                entity_iw4::TEAM_FREE
            } else {
                team
            };
            if frame.client_meta(id).is_some() {
                let meta = frame.client_meta_mut(id);
                meta.client_state_team = team;
                if matches!(
                    meta.lifecycle,
                    crate::ClientLifecycle::Spectating | crate::ClientLifecycle::ChoosingClass
                ) {
                    meta.lifecycle = crate::script_player::spectator_lifecycle(team);
                }
            }
        }
        "name" => return Err("player field name is read-only".into()),
        _ => return Ok(false),
    }
    Ok(name != "team")
}

const PM_TYPE_NORMAL: i32 = 0;

fn angle_delta(a: f32, b: f32) -> f32 {
    let d = (a - b) % 360.0;
    if d > 180.0 {
        d - 360.0
    } else if d < -180.0 {
        d + 360.0
    } else {
        d
    }
}

pub(crate) fn link_parent_pose(
    world: &mut World,
    parent: u64,
    tag: Option<&str>,
) -> ([f32; 3], [[f32; 3]; 3]) {
    if let Some(tag) = tag
        && let Some(pose) = super::presence::tag_world(world, parent, tag)
    {
        return pose;
    }
    let vector = |value| match value {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    };
    let origin = vector(entity_field(world, parent, "origin"));
    let angles = vector(entity_field(world, parent, "angles"));
    (origin, math_iw4::angles_to_axis(angles))
}

pub(crate) fn link_player(world: &mut World, client: u32, link: PlayerLink) {
    let id = ClientId(client);
    let mut frame = FrameWorld::from_world(world);
    if frame.client_meta(id).is_some() {
        frame.client_meta_mut(id).controls.linked = true;
    }
    if let Some(ps) = frame.player_mut(id) {
        if ps.pm_type == PM_TYPE_NORMAL {
            ps.pm_type = playerstate_iw4::PM_TYPE_NORMAL_LINKED;
        }
        ps.velocity = [0.0; 3];
        if link.view == LinkView::WeaponDelta {
            ps.link_flags |= playerstate_iw4::LINK_FLAGS_WEAPON_VIEW_ONLY;
            ps.link_weapon_angles = ps.viewangles;
        }
    }
    if let Some(slot) = world.resource_mut::<Runtime>().players.get_mut(&client) {
        slot.link = Some(link);
    }
}

pub(crate) fn unlink_player(world: &mut World, client: u32) {
    let id = ClientId(client);
    let link = world
        .resource_mut::<Runtime>()
        .players
        .get_mut(&client)
        .and_then(|slot| slot.link.take());
    let mut frame = FrameWorld::from_world(world);
    if frame.client_meta(id).is_some() {
        frame.client_meta_mut(id).controls.linked = false;
        frame.client_meta_mut(id).linked_weapon_view = None;
    }
    if let Some(ps) = frame.player_mut(id) {
        if ps.pm_type == playerstate_iw4::PM_TYPE_NORMAL_LINKED {
            ps.pm_type = PM_TYPE_NORMAL;
        }
        if let Some(link) = link.filter(|link| link.view == LinkView::WeaponDelta) {
            ps.link_flags &= !playerstate_iw4::LINK_FLAGS_WEAPON_VIEW_ONLY;
            if let Some(view) = link.restore_view {
                for i in 0..3 {
                    ps.delta_angles[i] += angle_delta(view[i], ps.viewangles[i]);
                }
                ps.viewangles = view;
            }
        }
    }
}

fn transpose(m: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    std::array::from_fn(|i| std::array::from_fn(|j| m[j][i]))
}

fn scale_rotation(m: [[f32; 3]; 3], fraction: f32) -> [[f32; 3]; 3] {
    if fraction >= 1.0 {
        return m;
    }
    let cos = ((m[0][0] + m[1][1] + m[2][2] - 1.0) * 0.5).clamp(-1.0, 1.0);
    let angle = cos.acos() * fraction.max(0.0);
    let axis = [m[1][2] - m[2][1], m[2][0] - m[0][2], m[0][1] - m[1][0]];
    let len = axis.iter().map(|v| v * v).sum::<f32>().sqrt();
    if len < 1e-6 {
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    }
    let [x, y, z] = axis.map(|v| v / len);
    let (s, c) = angle.sin_cos();
    let t = 1.0 - c;
    [
        [t * x * x + c, t * x * y + s * z, t * x * z - s * y],
        [t * x * y - s * z, t * y * y + c, t * y * z + s * x],
        [t * x * z + s * y, t * y * z - s * x, t * z * z + c],
    ]
}

/// Carries linked players with their parents before pmove runs; pmove leaves
/// a `PM_TYPE_NORMAL_LINKED` origin alone.
pub(crate) fn apply_player_links(world: &mut World) {
    let links: Vec<(u32, PlayerLink)> = world
        .resource::<Runtime>()
        .players
        .iter()
        .filter_map(|(client, slot)| Some((*client, slot.link.clone()?)))
        .collect();
    for (client, link) in links {
        let id = ClientId(client);
        let still_linked = FrameWorld::from_world(world)
            .client_meta(id)
            .is_some_and(|meta| meta.controls.linked);
        let parent_alive = world
            .resource::<Runtime>()
            .objects
            .contains_key(&link.parent);
        if !still_linked || !parent_alive {
            unlink_player(world, client);
            continue;
        }
        let (base, axis) = link_parent_pose(world, link.parent, link.tag.as_deref());
        let (child_axis, origin) = math_iw4::matrix_multiply43(
            math_iw4::angles_to_axis(link.angles),
            link.origin,
            axis,
            base,
        );
        let parent = math_iw4::axis_to_angles(axis);
        let entity_num = world
            .resource::<Runtime>()
            .entities
            .get(&link.parent)
            .map_or(playerstate_iw4::ENTITYNUM_NONE, |entity| entity.number);
        let mut frame = FrameWorld::from_world(world);
        if link.view != LinkView::WeaponDelta {
            frame.set_origin(id, origin);
        }
        let Some(mut view) = frame.player(id).map(|ps| ps.viewangles) else {
            continue;
        };
        let mut link_axis = link.link_axis;
        if matches!(link.view, LinkView::Delta | LinkView::WeaponDelta) {
            let turn = scale_rotation(
                math_iw4::matrix_multiply(transpose(link.parent_axis), axis),
                link.view_fraction,
            );
            let turned = math_iw4::axis_to_angles(math_iw4::matrix_multiply(
                math_iw4::angles_to_axis(view),
                turn,
            ));
            view[0] = turned[0];
            view[1] = turned[1];
            link_axis = if link.use_tag_angles {
                axis
            } else {
                math_iw4::matrix_multiply(link.link_axis, turn)
            };
        }
        match (link.view, link.clamp) {
            (LinkView::Absolute, _) => view = math_iw4::axis_to_angles(child_axis),
            (_, Some([pitch_min, pitch_max, yaw_min, yaw_max])) => {
                let pitch = angle_delta(view[0], parent[0]).clamp(pitch_min, pitch_max);
                let yaw = angle_delta(view[1], parent[1]).clamp(yaw_min, yaw_max);
                view = [parent[0] + pitch, parent[1] + yaw, view[2]];
            }
            _ => {}
        }
        if let Some(ps) = frame.player_mut(id) {
            for i in 0..3 {
                ps.delta_angles[i] += angle_delta(view[i], ps.viewangles[i]);
            }
            ps.viewangles = view;
            if link.view == LinkView::WeaponDelta {
                ps.link_weapon_angles = view;
            }
        }
        if link.view == LinkView::WeaponDelta {
            let relative = math_iw4::axis_to_angles(math_iw4::matrix_multiply(
                math_iw4::angles_to_axis(view),
                transpose(link_axis),
            ));
            let angles = [view[0], view[1], -relative[2]];
            frame.client_meta_mut(id).linked_weapon_view = Some(crate::LinkedWeaponView {
                entity_num,
                origin,
                angles,
            });
        }
        if let Some(slot) = world.resource_mut::<Runtime>().players.get_mut(&client)
            && let Some(link) = slot.link.as_mut()
        {
            link.parent_axis = axis;
            link.link_axis = link_axis;
        }
    }
}
