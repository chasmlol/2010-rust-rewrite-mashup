use super::args::{arg, string, vector};
use super::entities::EntityKind;
use crate::ProjectileId;
use crate::equipment::WeaponNote;
use crate::frame::FrameWorld;
use crate::script::Namespace::{Function, Method};
use crate::script::runtime::raise;
use crate::script::{Arc, NativeRegistry, Runtime, Value};
use crate::world::ClientId;
use bevy_ecs::prelude::World;
pub(crate) use weapon_iw4::WEAPTYPE_PROJECTILE;
use weapon_iw4::{WEAPTYPE_GRENADE, WeaponState};

const GRENADE_LINGER_MS: i64 = 30_000; // threads on the grenade keep running after it explodes

pub(crate) fn is_changing_weapon(state: i32) -> bool {
    matches!(
        WeaponState::from_i32(state),
        Ok(WeaponState::Raising
            | WeaponState::RaisingAltswitch
            | WeaponState::Dropping
            | WeaponState::DroppingQuick
            | WeaponState::DroppingAltswitch)
    )
}

fn dropping(state: i32) -> bool {
    matches!(
        WeaponState::from_i32(state),
        Ok(WeaponState::Dropping | WeaponState::DroppingQuick | WeaponState::DroppingAltswitch)
    )
}

fn weapon_name(world: &mut World, weapon: u32) -> Value {
    Value::String(crate::script_player::weapon_name(&FrameWorld::from_world(world), weapon).into())
}

fn now_ms(world: &World) -> i32 {
    crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick)
}

fn missile_of(world: &World, receiver: &Value) -> Result<(u64, ProjectileId, i32), String> {
    match world.resource::<Runtime>().entity(receiver) {
        Some((object, e)) => match e.kind {
            EntityKind::Missile(id) => Ok((object, id, e.number)),
            _ => Err("receiver is not a missile".into()),
        },
        None => Err("receiver is not a missile".into()),
    }
}

fn adopt(
    world: &mut World,
    projectile: &crate::ProjectileState,
    classname: &str,
) -> Result<u64, String> {
    let now = now_ms(world);
    let model =
        Value::string(FrameWorld::from_world(world).weapon_projectile_model(projectile.weapon));
    let facts = FrameWorld::from_world(world).equipment_facts_for(projectile.weapon);
    let presence = if facts.is_some_and(|facts| !facts.timed_detonation && facts.stickiness != 0) {
        Some(super::presence::spawn_presence(world, projectile.origin)?)
    } else {
        None
    };
    let mut runtime = world.resource_mut::<Runtime>();
    let object = runtime.create_entity(EntityKind::Missile(projectile.id), classname)?;
    let owner = runtime
        .players
        .get(&projectile.owner.0)
        .map(|slot| slot.object);
    let entity = runtime.entities.get_mut(&object).unwrap();
    entity.presence = presence;
    entity.missile_owner = owner;
    entity.number = projectile.entnum;
    runtime.set_object_field(object, "model", model);
    runtime.set_object_field(object, "origin", Value::Vector(projectile.origin_at(now)));
    runtime.set_object_field(
        object,
        "angles",
        Value::Vector(entity_iw4::evaluate_trajectory(&projectile.apos, now)),
    );
    runtime.missiles.insert(projectile.id, object);
    Ok(object)
}

pub(crate) fn launch(
    world: &mut World,
    owner: ClientId,
    weapon: u32,
    start: [f32; 3],
    end: [f32; 3],
) -> Result<Value, String> {
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let projectile = crate::missile::magic_bullet(
        &mut FrameWorld::from_world(world),
        tick,
        owner,
        weapon,
        start,
        end,
    )?;
    let object = adopt(world, &projectile, "rocket")?;
    Ok(Value::Object(object))
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Function, "getmissileowner", |world, _, args| {
        if args.len() != 1 {
            return Err("GetMissileOwner requires one missile argument".into());
        }
        let (object, _, _) = missile_of(world, &args[0])?;
        let runtime = world.resource::<Runtime>();
        let owner = runtime.entities[&object]
            .missile_owner
            .filter(|owner| runtime.players.values().any(|slot| slot.object == *owner));
        Ok(owner.map_or(Value::Undefined, Value::Object))
    });
    registry.register(Method, "predictgrenade", |world, receiver, _| {
        let (_, id, number) = missile_of(world, receiver)?;
        let now = now_ms(world);
        let frame = FrameWorld::from_world(world);
        let position = frame
            .projectile_by_number(number)
            .filter(|p| p.id == id && p.live)
            .and_then(|p| crate::equipment::predict_projectile(&frame, p, now, 3000));
        Ok(Value::Vector(position.unwrap_or([0.0; 3])))
    });
    registry.register(Function, "magicbullet", |world, _, args| {
        let name = string(args, 0)?;
        let weapon = crate::script_player::weapon_named(&FrameWorld::from_world(world), &name)?;
        let (start, end) = (vector(args, 1)?, vector(args, 2)?);
        let owner = match args.get(3) {
            Some(value) => world
                .resource::<Runtime>()
                .player_client_of(value)
                .map(ClientId)
                .ok_or("MagicBullet owner is not a player")?,
            None => return Err("MagicBullet needs an owning player".into()),
        };
        launch(world, owner, weapon, start, end)
    });
    registry.register(Method, "detonate", |world, receiver, _| {
        let (_, id, number) = missile_of(world, receiver)?;
        let now = now_ms(world);
        let mut frame = FrameWorld::from_world(world);
        if let Some(projectile) = frame
            .projectile_mut_by_number(number)
            .filter(|p| p.id == id && p.live)
        {
            projectile.detonate_at_ms = Some(now);
            projectile.grounded = true;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "controlslinkto", |world, receiver, args| {
        let client = super::natives::player::player(world, receiver)?;
        let (_, id, number) = missile_of(world, arg(args, 0)?)?;
        let now = now_ms(world);
        let mut frame = FrameWorld::from_world(world);
        let Some(projectile) = frame.projectile_by_number(number).filter(|p| p.id == id) else {
            return Ok(Value::Undefined);
        };
        let angles = entity_iw4::evaluate_trajectory(&projectile.apos, now);
        if frame.client_meta(ClientId(client)).is_some() {
            frame.client_meta_mut(ClientId(client)).remote_missile = Some(crate::RemoteMissile {
                projectile: id,
                entnum: number,
                angles,
                ..Default::default()
            });
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "controlsunlink", |world, receiver, _| {
        let client = super::natives::player::player(world, receiver)?;
        let now = now_ms(world);
        let mut frame = FrameWorld::from_world(world);
        if let Some(link) = frame
            .client_meta(ClientId(client))
            .and_then(|m| m.remote_missile)
        {
            frame.client_meta_mut(ClientId(client)).remote_missile = Some(crate::RemoteMissile {
                unlink_at_ms: Some(now),
                ..link
            });
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "cameralinkto", |world, receiver, args| {
        let client = super::natives::player::player(world, receiver)?;
        super::natives::engine::entity_id(world, arg(args, 0)?)?;
        world
            .resource::<Runtime>()
            .players
            .get(&client)
            .ok_or("player has disconnected")?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "cameraunlink", |world, receiver, _| {
        let client = super::natives::player::player(world, receiver)?;
        let mut frame = FrameWorld::from_world(world);
        if frame.client_meta(ClientId(client)).is_some() {
            frame.client_meta_mut(ClientId(client)).remote_missile = None;
        }
        Ok(Value::Undefined)
    });
}

pub(crate) fn sync_engine_events(world: &mut World) {
    let request = world.resource::<crate::step::StepRequest>();
    if !request.reason.advances_authority_world() {
        return;
    }
    let notes = std::mem::take(&mut FrameWorld::from_world(world).weapon_notes);
    let runtime = world.resource::<Runtime>();
    if runtime.program.is_none() || runtime.fault.is_some() || !runtime.started {
        return;
    }
    notify_weapon_changes(world);
    for note in &notes {
        let (owner, notify, args) = match *note {
            WeaponNote::Pullback { owner, weapon } => {
                let weapon = super::players::script_weapon(world, owner.0, weapon);
                (owner, "grenade_pullback", vec![weapon_name(world, weapon)])
            }
            WeaponNote::DetonationRequested { owner, weapon } => {
                let weapon = super::players::script_weapon(world, owner.0, weapon);
                (owner, "detonate", vec![weapon_name(world, weapon)])
            }
            WeaponNote::Fired { owner } => (owner, "begin_firing", Vec::new()),
            WeaponNote::ReloadStarted { owner } => (owner, "reload_start", Vec::new()),
            WeaponNote::Detonated { .. } | WeaponNote::Stuck { .. } => continue,
        };
        let player = super::players::player_object(world, owner.0);
        if player != Value::Undefined {
            raise(world, player, notify, args);
        }
    }
    adopt_fired(world);
    super::triggers::dispatch_grenade_touches(world);
    super::guidance::advance(world);
    super::turrets::advance(world);
    settle_projectiles(world, &notes);
    settle_items(world);
    super::vehicles::advance(world);
    super::spectators::advance(world);
    super::players::publish_radar(world);
    super::physics::select_usables(world);
}

fn notify_weapon_changes(world: &mut World) {
    let slots: Vec<(u32, u64, u32, bool)> = world
        .resource::<Runtime>()
        .players
        .iter()
        .map(|(client, slot)| (*client, slot.object, slot.weapon, slot.switching))
        .collect();
    for (client, object, last, was_switching) in slots {
        let Some(ps) = FrameWorld::from_world(world)
            .player(ClientId(client))
            .copied()
        else {
            continue;
        };
        let switching = dropping(ps.weaponstate_primary);
        let current = super::players::script_weapon(world, client, ps.weapon);
        if switching && !was_switching {
            let name = weapon_name(world, current);
            raise(
                world,
                Value::Object(object),
                "weapon_switch_started",
                vec![name],
            );
        }
        if ps.weapon != last {
            let name = weapon_name(world, current);
            raise(world, Value::Object(object), "weapon_change", vec![name]);
        }
        if let Some(slot) = world.resource_mut::<Runtime>().players.get_mut(&client) {
            slot.weapon = ps.weapon;
            slot.switching = switching;
        }
    }
}

fn adopt_fired(world: &mut World) {
    let now = now_ms(world);
    let seen = std::mem::replace(&mut world.resource_mut::<Runtime>().missiles_seen_ms, now);
    let touches = world
        .resource::<Runtime>()
        .grenade_touches
        .iter()
        .map(|touch| touch.projectile)
        .collect::<Vec<_>>();
    let mut adopted = std::collections::BTreeSet::new();
    let fresh: Vec<crate::ProjectileState> = crate::frame::collect_projectiles(world)
        .into_iter()
        .chain(touches)
        .filter(|p| p.live && p.spawn_time_ms > seen)
        .filter(|p| !world.resource::<Runtime>().missiles.contains_key(&p.id))
        .filter(|p| adopted.insert(p.id))
        .collect();
    for projectile in fresh {
        let player = super::players::player_object(world, projectile.owner.0);
        if player == Value::Undefined {
            continue;
        }
        let weap_type = FrameWorld::from_world(world)
            .combat_facts_for(projectile.weapon)
            .map_or(-1, |facts| facts.weap_type);
        let (classname, notify) = match weap_type {
            WEAPTYPE_GRENADE => ("grenade", "grenade_fire"),
            WEAPTYPE_PROJECTILE => ("rocket", "missile_fire"),
            _ => continue,
        };
        let object = match adopt(world, &projectile, classname) {
            Ok(object) => object,
            Err(message) => {
                diag::warn!(Sim, "gsc: {message}; {notify} not raised");
                continue;
            }
        };
        let weapon = super::players::event_weapon(world, projectile.owner.0, projectile.weapon);
        super::players::note_insertion_throw(
            world,
            projectile.owner.0,
            weapon,
            projectile.weapon,
            object,
        );
        let name = weapon_name(world, weapon);
        let model = format!("{}{}", crate::WEAPON_MODEL_PREFIX, projectile.weapon);
        let native = weapon_name(world, projectile.weapon);
        {
            let mut runtime = world.resource_mut::<Runtime>();
            runtime.set_object_field(object, "nativename", native);
            runtime.set_object_field(object, "weaponname", name.clone());
            runtime.set_object_field(object, "weaponmodel", Value::string(&model));
        }
        raise(world, player, notify, vec![Value::Object(object), name]);
    }
}

fn settle_items(world: &mut World) {
    let items: Vec<(u64, i32, Arc<str>)> = {
        let runtime = world.resource::<Runtime>();
        runtime
            .entities
            .iter()
            .filter(|(id, _)| !runtime.pending_deletes.contains(id))
            .filter_map(|(id, e)| match e.kind {
                EntityKind::Item(number) => Some((*id, number, e.classname.clone())),
                _ => None,
            })
            .collect()
    };
    let pickups = FrameWorld::from_world(world).item_pickups_mut().clone();
    let mut retired = Vec::new();
    for pickup in pickups {
        let Some((object, _, classname)) = items
            .iter()
            .find(|(_, number, _)| *number == pickup.from_entnum)
            .cloned()
        else {
            continue;
        };
        let player = super::players::player_object(world, pickup.picker as u32);
        let receiver = Value::Object(object);
        if classname.as_ref() == super::natives::player::SCAVENGER_ITEM_CLASS {
            raise(world, receiver.clone(), "scavenger", vec![player]);
        } else {
            let swapped = if pickup.swapped_entnum == playerstate_iw4::ENTITYNUM_NONE {
                Ok(Value::Undefined)
            } else {
                super::natives::player::new_item_entity(world, pickup.swapped_entnum, "weapon_item")
            };
            let swapped = swapped.unwrap_or_else(|message| {
                diag::warn!(Sim, "gsc: {message}; dropped weapon has no entity");
                Value::Undefined
            });
            raise(world, receiver.clone(), "trigger", vec![player, swapped]);
        }
        retire(world, object);
        retired.push(object);
    }
    for (object, number, _) in items {
        if !retired.contains(&object)
            && FrameWorld::from_world(world)
                .dropped_item_by_number(number)
                .is_none()
        {
            retire(world, object);
        }
    }
}

fn retire(world: &mut World, object: u64) {
    raise(world, Value::Object(object), "death", Vec::new());
    world.resource_mut::<Runtime>().pending_deletes.push(object);
}

fn settle_projectiles(world: &mut World, notes: &[WeaponNote]) {
    let now = now_ms(world);
    let tracked: Vec<(ProjectileId, u64)> = world
        .resource::<Runtime>()
        .missiles
        .iter()
        .map(|(id, object)| (*id, *object))
        .collect();
    for (id, object) in tracked {
        let Some(number) = world
            .resource::<Runtime>()
            .entities
            .get(&object)
            .map(|e| e.number)
        else {
            world.resource_mut::<Runtime>().missiles.remove(&id);
            if let Some(number) = crate::frame::collect_projectiles(world)
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.entnum)
            {
                FrameWorld::from_world(world).remove_projectile_by_number(number);
            }
            continue;
        };
        let detonated = notes.iter().find_map(|note| match *note {
            WeaponNote::Detonated { id: at, origin } if at == id => Some(origin),
            _ => None,
        });
        let stuck = notes
            .iter()
            .any(|note| matches!(*note, WeaponNote::Stuck { id: at } if at == id));
        let flying = FrameWorld::from_world(world)
            .projectile_by_number(number)
            .filter(|p| p.id == id && p.live);
        let receiver = Value::Object(object);
        match (flying, detonated) {
            (Some(projectile), None) => {
                let mut runtime = world.resource_mut::<Runtime>();
                runtime.set_object_field(
                    object,
                    "origin",
                    Value::Vector(projectile.origin_at(now)),
                );
                runtime.set_object_field(
                    object,
                    "angles",
                    Value::Vector(entity_iw4::evaluate_trajectory(&projectile.apos, now)),
                );
                drop(runtime);
                if stuck {
                    raise(world, receiver, "missile_stuck", vec![Value::Undefined]);
                }
            }
            _ => {
                let mut runtime = world.resource_mut::<Runtime>();
                runtime.missiles.remove(&id);
                if let Some(origin) = detonated {
                    runtime.set_object_field(object, "origin", Value::Vector(origin));
                }
                let entity = runtime.entities.get_mut(&object).unwrap();
                let lingers = entity.classname.as_ref() == "grenade" && entity.presence.is_none();
                entity.can_damage = false;
                if let Some(presence) = entity.presence.take() {
                    runtime.retired_presence.push((presence, true));
                }
                drop(runtime);
                if let Some(origin) = detonated {
                    raise(
                        world,
                        receiver.clone(),
                        "explode",
                        vec![Value::Vector(origin)],
                    );
                }
                if lingers {
                    world
                        .resource_mut::<Runtime>()
                        .lingering
                        .push((i64::from(now) + GRENADE_LINGER_MS, object));
                } else {
                    raise(world, receiver, "death", Vec::new());
                    world.resource_mut::<Runtime>().pending_deletes.push(object);
                }
            }
        }
    }
    let due: Vec<u64> = {
        let mut runtime = world.resource_mut::<Runtime>();
        let (due, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut runtime.lingering)
            .into_iter()
            .partition(|(at, _)| *at <= i64::from(now));
        runtime.lingering = kept;
        due.into_iter().map(|(_, object)| object).collect()
    };
    for object in due {
        if world.resource::<Runtime>().entities.contains_key(&object) {
            raise(world, Value::Object(object), "death", Vec::new());
            world.resource_mut::<Runtime>().pending_deletes.push(object);
        }
    }
}
