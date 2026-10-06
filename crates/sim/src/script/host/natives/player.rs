use super::super::args::{arg, float, int, kind, optional, string, vector};
use super::super::arrays::new_array;
use super::super::players::{LinkView, PlayerLink};
use super::super::tables::perk_slot_code;
use crate::frame::FrameWorld;
use crate::script::Namespace::Method;
use crate::script::{Arc, NativeRegistry, Runtime, Value, runtime};
use crate::script_player;
use crate::world::ClientId;
use bevy_ecs::prelude::World;

pub(crate) fn player(world: &World, receiver: &Value) -> Result<u32, String> {
    match receiver {
        Value::Object(id) => world
            .resource::<Runtime>()
            .player_client(*id)
            .ok_or_else(|| "receiver is not a player".into()),
        _ => Err("receiver is not a player".into()),
    }
}

fn slot<'w>(
    world: &'w mut World,
    client: u32,
) -> Result<&'w mut super::super::players::PlayerSlot, String> {
    world
        .resource_mut::<Runtime>()
        .into_inner()
        .players
        .get_mut(&client)
        .ok_or_else(|| "player has disconnected".into())
}

pub(crate) fn present(
    world: &mut World,
    receiver: &Value,
    key: &'static str,
    args: &[Value],
) -> Result<Value, String> {
    let client = player(world, receiver)?;
    slot(world, client)?.presented.insert(key, args.to_vec());
    Ok(Value::Undefined)
}

pub(crate) fn text(value: &Value) -> Result<Arc<str>, String> {
    match value {
        Value::String(s) => Ok(s.clone().into()),
        Value::LocalizedString(s) => Ok(s.clone()),
        Value::Int(n) => Ok(n.to_string().into()),
        Value::Vector(v) => Ok(format!("{} {} {}", v[0], v[1], v[2]).into()),
        Value::Float(f) => Ok(runtime::to_text(&Value::Float(*f))
            .unwrap_or_default()
            .into()),
        other => Err(format!("{} is not a string", kind(other))),
    }
}

fn client_dvar_value(value: &Value, params: &[Value]) -> Result<String, String> {
    let mut out = match value {
        Value::LocalizedString(key) => format!("@{key}"),
        other => text(other)?.to_string(),
    };
    for param in params {
        out.push(crate::HUD_PRINT_ARG_SEPARATOR);
        out.push_str(&text(param)?);
    }
    Ok(out)
}

fn publish_client_dvar(world: &mut World, client: u32, name: &str, value: String) {
    let value = if let Some(setting) = crate::TargetBoxDvar::named(name) {
        let Some(value) = setting.parse(&value) else {
            return;
        };
        let value = value.to_string();
        let mut runtime = world.resource_mut::<Runtime>();
        if runtime.local_presentation_client == Some(ClientId(client)) {
            runtime.pending_local_dvars.push((setting, value));
            return;
        }
        value
    } else {
        value
    };
    let mut frame = FrameWorld::from_world(world);
    if frame.client_meta(ClientId(client)).is_none() {
        return;
    }
    let dvars = &mut frame.client_meta_mut(ClientId(client)).client_dvars;
    match dvars.iter_mut().find(|(key, _)| key == name) {
        Some(row) => row.1 = value,
        None => dvars.push((name.to_owned(), value)),
    }
}

pub(crate) fn send_menu_command(world: &mut World, client: u32, kind: crate::MenuCommandKind) {
    let mut frame = FrameWorld::from_world(world);
    if frame.client_meta(ClientId(client)).is_none() {
        return;
    }
    frame
        .client_meta_mut(ClientId(client))
        .push_menu_command(kind);
}

fn data_keys(values: &[Value]) -> Result<Vec<structured_data_iw4::Key<'_>>, String> {
    values
        .iter()
        .map(|value| match value {
            Value::String(name) => {
                let bytes = name.as_bytes();
                let end = bytes
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(bytes.len());
                std::str::from_utf8(&bytes[..end])
                    .map(structured_data_iw4::Key::Name)
                    .map_err(|_| "player data key is not a schema name".to_owned())
            }
            Value::Int(index) => Ok(structured_data_iw4::Key::Index(*index)),
            other => Err(format!(
                "player data key must be a string or int, not {}",
                kind(other)
            )),
        })
        .collect()
}

fn cast_data_keys(
    store: &crate::PersistentDataStore,
    client: ClientId,
    values: &[Value],
) -> Result<Vec<Value>, String> {
    use structured_data_iw4::DataType as T;
    let mut converted = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let keys = data_keys(&converted)?;
        let value = match store.path_type(client, &keys).map_err(data_error)? {
            T::Struct(_) | T::EnumArray(_) => {
                Value::String(super::super::args::byte_string(values, index)?)
            }
            T::IndexedArray(_) => match value {
                Value::Int(_) => value.clone(),
                other => {
                    return Err(format!(
                        "parameter {} is {}, not an int",
                        index + 1,
                        kind(other)
                    ));
                }
            },
            _ => {
                return Err(data_error(crate::PersistentDataError::Field(
                    structured_data_iw4::Error::ExtraKey,
                )));
            }
        };
        converted.push(value);
    }
    Ok(converted)
}

fn data_value<'a>(
    ty: structured_data_iw4::DataType,
    value: &'a Value,
) -> Result<structured_data_iw4::Value<'a>, String> {
    use structured_data_iw4::{DataType as T, Value as V};
    match (ty, value) {
        (T::Int | T::Byte | T::Short, Value::Int(n)) => Ok(V::Int(*n)),
        (T::Bool, Value::Int(n)) => Ok(V::Bool(*n != 0)),
        (T::Float, Value::Float(n)) => Ok(V::Float(*n)),
        (T::Float, Value::Int(n)) => Ok(V::Float(*n as f32)),
        (T::String(_), Value::String(s)) => Ok(V::Bytes(s.as_bytes())),
        (T::Enum(_), Value::String(s)) => std::str::from_utf8(s.as_bytes())
            .map(V::String)
            .map_err(|_| "player data value is not an enum name".to_owned()),
        _ => Err(format!(
            "{} is not a value for player data type {ty:?}",
            kind(value)
        )),
    }
}

fn data_error(error: crate::PersistentDataError) -> String {
    format!("player data: {error:?}")
}

pub(crate) fn check_data_write(world: &World) -> Result<(), String> {
    if world
        .resource::<Runtime>()
        .program
        .as_ref()
        .is_some_and(|program| program.has_impure_scripts())
    {
        return Err("player data cannot be changed after loading external scripts".into());
    }
    if world
        .resource::<Runtime>()
        .dvars
        .get("developer_script")
        .and_then(|value| value.parse::<i32>().ok())
        .is_some_and(|value| value != 0)
    {
        return Err("player data cannot be changed with developer_script enabled".into());
    }
    Ok(())
}

fn data_client(world: &mut World, client: u32) -> Result<ClientId, String> {
    slot(world, client)?;
    if FrameWorld::from_world(world)
        .client_meta(ClientId(client))
        .is_none()
    {
        return Err("player has disconnected".into());
    }
    Ok(ClientId(client))
}

pub(crate) fn write_class_data(
    world: &mut World,
    client: u32,
    fields: &[(Vec<Value>, Value)],
) -> Result<(), String> {
    if fields.is_empty() {
        return Ok(());
    }
    let keys = fields
        .iter()
        .map(|(keys, _)| data_keys(keys))
        .collect::<Result<Vec<_>, _>>()?;
    let store = world.resource::<crate::PersistentDataStore>();
    let values = fields
        .iter()
        .zip(&keys)
        .map(|((_, value), keys)| {
            data_value(
                store
                    .field_type(ClientId(client), keys)
                    .map_err(data_error)?,
                value,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let writes = keys
        .iter()
        .zip(values)
        .map(|(keys, value)| (keys.as_slice(), value))
        .collect::<Vec<_>>();
    world
        .resource_mut::<crate::PersistentDataStore>()
        .write_many(ClientId(client), &writes)
        .map_err(data_error)?;
    Ok(())
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Method, "ishost", |world, receiver, _| {
        let client = player(world, receiver)?;
        Ok(Value::Int((client == 0).into()))
    });
    registry.register(Method, "getguid", |world, receiver, _| {
        let client = player(world, receiver)?;
        Ok(Value::String(
            format!("{:016x}", u64::from(client) + 1).into(),
        ))
    });
    registry.register(Method, "getxuid", |world, receiver, _| {
        let client = player(world, receiver)?;
        Ok(Value::String(format!("{:x}", u64::from(client) + 1).into()))
    });
    registry.register(Method, "isusingonlinedataoffline", |world, receiver, _| {
        player(world, receiver)?;
        Ok(Value::Int(0))
    });
    registry.register(Method, "getrestedtime", |world, receiver, _| {
        player(world, receiver)?;
        Ok(Value::Int(0))
    });
    registry.register(Method, "setclientdvar", |world, receiver, args| {
        let client = player(world, receiver)?;
        let name: Arc<str> = string(args, 0)?.to_ascii_lowercase().into();
        let value = text(arg(args, 1)?)?;
        let sent = client_dvar_value(arg(args, 1)?, &args[2..])?;
        slot(world, client)?.dvars.insert(name.clone(), value);
        publish_client_dvar(world, client, &name, sent);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setclientdvars", |world, receiver, args| {
        let client = player(world, receiver)?;
        if args.len() % 2 != 0 {
            return Err("setclientdvars takes name/value pairs".into());
        }
        let mut pairs: Vec<(Arc<str>, Arc<str>)> = Vec::new();
        let mut sent = Vec::new();
        for i in (0..args.len()).step_by(2) {
            let name: Arc<str> = string(args, i)?.to_ascii_lowercase().into();
            sent.push((name.clone(), client_dvar_value(&args[i + 1], &[])?));
            pairs.push((name, text(&args[i + 1])?));
        }
        slot(world, client)?.dvars.extend(pairs);
        for (name, value) in sent {
            publish_client_dvar(world, client, &name, value);
        }
        Ok(Value::Undefined)
    });
    for name in ["openmenu", "openpopupmenu", "openmenunomouse"] {
        registry.register(Method, name, |world, receiver, args| {
            let client = player(world, receiver)?;
            let menu: Arc<str> = string(args, 0)?.to_ascii_lowercase().into();
            if let Some(cs_index) = hud_iw4::script_menu_cs_index(&menu) {
                FrameWorld::from_world(world).push_player_card_open(ClientId(client), cs_index);
            } else {
                send_menu_command(
                    world,
                    client,
                    crate::MenuCommandKind::Open(menu.to_string()),
                );
            }
            slot(world, client)?.menu = Some(menu);
            Ok(Value::Int(1))
        });
    }
    registry.register(Method, "setcarddisplayslot", |world, receiver, args| {
        let client = player(world, receiver)?;
        let source = player(world, arg(args, 0)?)?;
        let card = int(args, 1)?;
        if !(0..hud_iw4::PLAYER_CARD_SCRIPT_SLOT_COUNT as i32).contains(&card) {
            return Err(format!("card display slot {card} is out of range"));
        }
        FrameWorld::from_world(world).push_player_card_slot(
            ClientId(client),
            ClientId(source),
            card,
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "showhudsplash", |world, receiver, args| {
        let client = player(world, receiver)?;
        let splash = string(args, 0)?;
        let splash_slot = int(args, 1)?;
        if !(0..5).contains(&splash_slot) {
            return Err(format!("hud splash slot {splash_slot} is out of range"));
        }
        let optional_number = optional(args, 2, int)?.unwrap_or(0);
        FrameWorld::from_world(world).push_hud_splash(
            ClientId(client),
            splash,
            splash_slot,
            optional_number,
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "closepopupmenu", |world, receiver, _| {
        let client = player(world, receiver)?;
        send_menu_command(world, client, crate::MenuCommandKind::ClosePopup);
        slot(world, client)?.menu = None;
        Ok(Value::Undefined)
    });
    for name in ["closemenu", "closeingamemenu"] {
        registry.register(Method, name, |world, receiver, _| {
            let client = player(world, receiver)?;
            send_menu_command(world, client, crate::MenuCommandKind::CloseInGame);
            slot(world, client)?.menu = None;
            Ok(Value::Undefined)
        });
    }
    registry.register(Method, "getplayerdata", |world, receiver, args| {
        let client = player(world, receiver)?;
        let client = data_client(world, client)?;
        let store = world.resource::<crate::PersistentDataStore>();
        let converted = cast_data_keys(store, client, args)?;
        let keys = data_keys(&converted)?;
        if let Some((index, false)) = store.enum_index(client, &keys).map_err(data_error)? {
            diag::warn!(
                Sim,
                "gsc: invalid player data enum index {index}; using index zero"
            );
        }
        use structured_data_iw4::Value as V;
        Ok(match store.read(client, &keys).map_err(data_error)? {
            V::Int(n) => Value::Int(n),
            V::Bool(n) => Value::Int(n.into()),
            V::Float(n) => Value::Float(n),
            V::String(s) => Value::string(s),
            V::Bytes(s) => Value::byte_string(s),
        })
    });
    registry.register(Method, "setplayerdata", |world, receiver, args| {
        let client = player(world, receiver)?;
        check_data_write(world)?;
        let client = data_client(world, client)?;
        let (value, keys) = args
            .split_last()
            .ok_or("setplayerdata needs a key and a value")?;
        let converted =
            cast_data_keys(world.resource::<crate::PersistentDataStore>(), client, keys)?;
        let keys = data_keys(&converted)?;
        let ty = world
            .resource::<crate::PersistentDataStore>()
            .field_type(client, &keys)
            .map_err(data_error)?;
        let value = if matches!(
            ty,
            structured_data_iw4::DataType::String(_) | structured_data_iw4::DataType::Enum(_)
        ) {
            Value::String(super::super::args::byte_string(args, args.len() - 1)?)
        } else {
            value.clone()
        };
        let value = data_value(ty, &value)?;
        world
            .resource_mut::<crate::PersistentDataStore>()
            .write(client, &keys, value)
            .map_err(data_error)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "notifyonplayercommand", |world, receiver, args| {
        let client = player(world, receiver)?;
        let notify: Arc<str> = super::super::args::byte_string(args, 0)?.symbol_key();
        let command: Arc<str> = string(args, 1)?.to_ascii_lowercase().into();
        let slot = slot(world, client)?;
        if !slot
            .commands
            .iter()
            .any(|(c, n)| *c == command && *n == notify)
        {
            slot.commands.push((command, notify));
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "setrank", |world, receiver, args| {
        let client = player(world, receiver)?;
        let rank = int(args, 0)?;
        let prestige = optional(args, 1, int)?.unwrap_or(0);
        let mut frame = FrameWorld::from_world(world);
        if frame.client_meta(ClientId(client)).is_some() {
            let meta = frame.client_meta_mut(ClientId(client));
            meta.rank = rank;
            meta.prestige = prestige;
        }
        Ok(Value::Undefined)
    });
    for name in ["updatescores", "updatedmscores"] {
        registry.register(Method, name, |world, receiver, _| {
            player(world, receiver)?;
            Ok(Value::Undefined)
        });
    }

    register_body(registry);
    register_inventory(registry);
    register_shield(registry);
    register_death(registry);

    registry.register(Method, "sayall", |world, receiver, args| {
        let client = player(world, receiver)?;
        super::super::hud::chat(world, client, false, args)
    });
    registry.register(Method, "sayteam", |world, receiver, args| {
        let client = player(world, receiver)?;
        super::super::hud::chat(world, client, true, args)
    });

    macro_rules! presented {
        ($($name:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, args| {
                present(world, receiver, $name, args)
            });
        )*};
    }
    presented!(
        "setcardtitle",
        "setcardicon",
        "setcardnameplate",
        "setweaponhudiconoverride",
        "pingplayer",
        "kc_regweaponforfxremoval",
        "setviewmodel",
        "forceusehinton",
        "forceusehintoff",
        "predictstreampos",
    );
    macro_rules! answers {
        ($value:expr => $($name:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, _| {
                player(world, receiver)?;
                Ok($value)
            });
        )*};
    }
    answers!(Value::Int(1) => "isitemunlocked");
    registry.register(Method, "playerhide", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let mut frame = FrameWorld::from_world(world);
        if let Some(ps) = frame.player_mut(id) {
            ps.e_flags |= playerstate_iw4::eflags::NODRAW;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "playerforcedeathanim", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let inflictor = match args.first() {
            Some(Value::Object(obj)) if world.resource::<Runtime>().live(obj) => {
                match super::super::players::entity_field(world, *obj, "origin") {
                    Value::Vector(v) => Some(v),
                    _ => None,
                }
            }
            _ => None,
        };
        let means = optional(args, 1, string)?;
        let weapon = optional(args, 2, string)?;
        let hitloc = optional(args, 3, string)?;
        let v_dir = optional(args, 4, vector)?;
        let explosive_mod = matches!(
            means.as_deref(),
            Some(
                "MOD_EXPLOSIVE_BULLET"
                    | "MOD_GRENADE"
                    | "MOD_GRENADE_SPLASH"
                    | "MOD_PROJECTILE"
                    | "MOD_PROJECTILE_SPLASH"
                    | "MOD_EXPLOSIVE"
            )
        );
        let hitloc = hitloc
            .and_then(|name| weapon_iw4::HITLOC_NAMES.iter().position(|n| *n == name))
            .map_or(0, |i| i as u8);
        let mut frame = FrameWorld::from_world(world);
        let weapon = weapon.and_then(|name| frame.weapon_index_by_script_name(&name));
        crate::damage::force_death_anim(
            &mut frame,
            id,
            weapon,
            explosive_mod,
            inflictor,
            hitloc,
            v_dir.unwrap_or([0.0; 3]),
        );
        Ok(Value::Int(0))
    });
    registry.register(Method, "isusingturret", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let frame = FrameWorld::from_world(world);
        let using = frame.player(id).is_some_and(|ps| {
            ps.e_flags
                & (playerstate_iw4::eflags::TURRET_ACTIVE_PRONE
                    | playerstate_iw4::eflags::TURRET_ACTIVE_DUCK)
                != 0
        });
        Ok(Value::Int(using.into()))
    });
    answers!(Value::Vector([0.0; 3]) => "getthirdpersoncrosshairoffset");
}

const PLAYER_ANIM_TREE: &str = "multiplayer";

fn tick(world: &World) -> crate::Tick {
    world.resource::<crate::step::StepRequest>().tick
}

fn maybe_player(world: &World, args: &[Value], index: usize) -> Option<ClientId> {
    match args.get(index) {
        Some(Value::Object(id)) => world.resource::<Runtime>().player_client(*id).map(ClientId),
        _ => None,
    }
}

fn entity_kind(
    world: &World,
    receiver: &Value,
) -> Option<(u64, super::super::entities::EntityKind)> {
    let Value::Object(id) = receiver else {
        return None;
    };
    world
        .resource::<Runtime>()
        .entities
        .get(id)
        .map(|e| (*id, e.kind.clone()))
}

pub(crate) fn corpse_anim(world: &World, receiver: &Value) -> Result<Option<Arc<str>>, String> {
    match entity_kind(world, receiver) {
        Some((_, super::super::entities::EntityKind::Corpse { anim, .. })) => Ok(anim),
        _ => Err("receiver is not a corpse".into()),
    }
}

fn item_number(world: &World, receiver: &Value) -> Result<i32, String> {
    match entity_kind(world, receiver) {
        Some((_, super::super::entities::EntityKind::Item(number))) => Ok(number),
        _ => Err("receiver is not a weapon item".into()),
    }
}

fn anim_clip(
    world: &mut World,
    args: &[Value],
    index: usize,
) -> Result<Arc<xmodel_runtime::AnimClip>, String> {
    let name = match arg(args, index)? {
        Value::Animation { name, .. } => name.clone(),
        other => return Err(format!("{} is not an animation", kind(other))),
    };
    FrameWorld::from_world(world)
        .player_anim_clip_named(&name)
        .ok_or_else(|| format!("animation '{name}' is not loaded in the simulation"))
}

pub(crate) const SCAVENGER_ITEM_CLASS: &str = "scavenger_item";

pub(crate) fn new_item_entity(
    world: &mut World,
    number: i32,
    classname: &str,
) -> Result<Value, String> {
    let origin = FrameWorld::from_world(world)
        .dropped_item_by_number(number)
        .map_or([0.0; 3], |i| i.origin);
    let mut runtime = world.resource_mut::<Runtime>();
    let id = runtime.create_entity(super::super::entities::EntityKind::Item(number), classname)?;
    runtime.set_object_field(id, "origin", Value::Vector(origin));
    Ok(Value::Object(id))
}

fn register_death(registry: &mut NativeRegistry) {
    registry.register(Method, "finishplayerdamage", |world, receiver, args| {
        let client = player(world, receiver)?;
        let now = super::super::players::now_ms(world);
        if slot(world, client)?
            .last_stand_until_ms
            .is_some_and(|until| now < until)
        {
            return Ok(Value::Undefined);
        }
        let id = ClientId(client);
        let attacker = maybe_player(world, args, 1);
        let inflictor_entity = super::super::players::damage_entity(world, args.first());
        let attacker_entity = super::super::players::damage_entity(world, args.get(1));
        let amount = int(args, 2)?;
        if args
            .get(8)
            .is_some_and(|value| matches!(value, Value::String(s) if s.as_ref() == "shield"))
        {
            return Ok(Value::Undefined);
        }
        let dir = match args.get(7) {
            Some(Value::Vector(v)) => Some(*v),
            _ => None,
        };
        let commit = world
            .resource::<Runtime>()
            .current_hit
            .as_ref()
            .filter(|hit| hit.victim == id)
            .and_then(|hit| hit.commit);
        let tick = tick(world);
        let owed = || -> Vec<Value> {
            let at = |i: usize| args.get(i).cloned().unwrap_or(Value::Undefined);
            vec![
                inflictor_entity.clone(),
                attacker_entity.clone(),
                at(2),
                at(4),
                at(5),
                at(7),
                at(8),
                at(9),
                Value::Int(0),
            ]
        };
        let finish = {
            let mut frame = FrameWorld::from_world(world);
            let finish = script_player::finish_damage(&mut frame, id, amount, dir);
            if finish == script_player::Finish::Killed {
                script_player::kill(&mut frame, tick, id, attacker, commit);
            }
            finish
        };
        match finish {
            script_player::Finish::Hurt => {}
            script_player::Finish::LastStand => {
                slot(world, client)?.last_stand_until_ms = Some(now + 500);
                super::super::players::owe(world, client, super::super::players::LAST_STAND, owed())
            }
            script_player::Finish::Killed => {
                super::super::players::owe(world, client, super::super::players::KILLED, owed())
            }
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "suicide", |world, receiver, _| {
        let client = player(world, receiver)?;
        let tick = tick(world);
        super::super::players::suicide(world, tick, client);
        Ok(Value::Undefined)
    });
    registry.register(Method, "laststandrevive", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        slot(world, id.0)?.last_stand_until_ms = None;
        script_player::revive(&mut FrameWorld::from_world(world), id);
        Ok(Value::Undefined)
    });
    registry.register(
        crate::script::Namespace::Function,
        "obituary",
        |world, _, args| {
            let Some(victim) = maybe_player(world, args, 0) else {
                return Err("obituary victim is not a player".into());
            };
            let attacker = maybe_player(world, args, 1);
            let weapon = weapon_arg(world, args, 2)?;
            let weapon = attacker.map_or(weapon, |attacker| {
                super::super::players::bridged_weapon(world, attacker.0, weapon)
            });
            let means = string(args, 3)?;
            let tick = tick(world);
            script_player::obituary(
                &mut FrameWorld::from_world(world),
                tick,
                victim,
                attacker,
                weapon,
                &means,
            );
            Ok(Value::Undefined)
        },
    );
    registry.register(Method, "cloneplayer", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let tick = tick(world);
        let mut frame = FrameWorld::from_world(world);
        let Some(ps) = frame.player(id).copied() else {
            return Ok(Value::Undefined);
        };
        let anim = frame
            .player_anim_clip(ps.legs_anim)
            .map(|clip| Arc::<str>::from(clip.name.as_str()));
        let Some(slot) = script_player::clone_corpse(&mut frame, tick, id) else {
            return Ok(Value::Undefined);
        };
        let mut runtime = world.resource_mut::<Runtime>();
        let body = runtime.create_entity(
            super::super::entities::EntityKind::Corpse { slot, anim },
            "player_corpse",
        )?;
        runtime.set_object_field(body, "origin", Value::Vector(ps.origin));
        runtime.set_object_field(body, "angles", Value::Vector([0.0, ps.viewangles[1], 0.0]));
        Ok(Value::Object(body))
    });
    registry.register(Method, "getcorpseanim", |world, receiver, _| {
        let name = corpse_anim(world, receiver)?.ok_or("the corpse has no death animation")?;
        Ok(Value::Animation {
            tree: PLAYER_ANIM_TREE.into(),
            name,
        })
    });
    registry.register(Method, "isragdoll", |world, receiver, _| {
        corpse_anim(world, receiver)?;
        Ok(Value::Int(0))
    });
    registry.register(Method, "startragdoll", |world, receiver, _| {
        corpse_anim(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(
        crate::script::Namespace::Function,
        "getanimlength",
        |world, _, args| Ok(Value::Float(anim_clip(world, args, 0)?.duration())),
    );
    registry.register(
        crate::script::Namespace::Function,
        "animhasnotetrack",
        |world, _, args| {
            let clip = anim_clip(world, args, 0)?;
            let note = string(args, 1)?;
            Ok(Value::Int(
                clip.notifies
                    .iter()
                    .any(|n| n.name.eq_ignore_ascii_case(&note))
                    .into(),
            ))
        },
    );
    registry.register(
        crate::script::Namespace::Function,
        "getnotetracktimes",
        |world, _, args| {
            let clip = anim_clip(world, args, 0)?;
            let note = string(args, 1)?;
            let times: Vec<Value> = clip
                .notifies
                .iter()
                .filter(|n| n.name.eq_ignore_ascii_case(&note))
                .map(|n| Value::Float(n.time))
                .collect();
            new_array(world, times)
        },
    );
    registry.register(Method, "dropitem", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = player_weapon(world, id, args, 0)?;
        let tick = tick(world);
        match crate::item::drop_weapon(&mut FrameWorld::from_world(world), tick, id, weapon) {
            Some(number) => new_item_entity(world, number, "weapon_item"),
            None => Ok(Value::Undefined),
        }
    });
    for name in ["dropscavengerbag", "dropscavengeritem"] {
        registry.register(Method, name, |world, receiver, args| {
            let id = client_of(world, receiver)?;
            let weapon = player_weapon(world, id, args, 0)?;
            let tick = tick(world);
            let mut frame = FrameWorld::from_world(world);
            match crate::item::drop_scavenger_item(&mut frame, tick, id, weapon) {
                Some(number) => new_item_entity(world, number, SCAVENGER_ITEM_CLASS),
                None => Ok(Value::Undefined),
            }
        });
    }
    registry.register(Method, "itemweaponsetammo", |world, receiver, args| {
        let number = item_number(world, receiver)?;
        let clip = int(args, 0)?;
        let stock = int(args, 1)?;
        let clip_l = optional(args, 2, int)?;
        let mut frame = FrameWorld::from_world(world);
        if let Some(item) = frame.dropped_item_mut_by_number(number) {
            item.clip_r = clip.max(0);
            item.stock = stock.max(0);
            if let Some(clip_l) = clip_l {
                item.clip_l = clip_l.max(0);
            }
        }
        Ok(Value::Undefined)
    });
}

const OFFHAND_CLASSES: [&str; 6] = ["none", "frag", "smoke", "flash", "throwingknife", "other"];

fn offhand_class(name: &str) -> i32 {
    OFFHAND_CLASSES
        .iter()
        .skip(1)
        .position(|class| *class == name)
        .map_or(0, |i| i as i32 + 1)
}

fn offhand_class_of(world: &mut World, name: &str) -> i32 {
    match offhand_class(name) {
        0 => {
            let frame = FrameWorld::from_world(world);
            frame
                .weapon_index_by_script_name(name)
                .and_then(|weapon| frame.offhand_loadout_row(weapon))
                .map_or(0, |facts| facts.offhand_class)
        }
        class => class,
    }
}

fn client_of(world: &World, receiver: &Value) -> Result<ClientId, String> {
    player(world, receiver).map(ClientId)
}

pub(crate) fn link_to(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    view: LinkView,
) -> Result<Value, String> {
    let client = player(world, receiver)?;
    let parent = super::engine::entity_id(world, arg(args, 0)?)?;
    if world.resource::<Runtime>().player_client(parent) == Some(client) {
        return Err("cannot link an entity to itself".into());
    }
    let tag = match args.get(1) {
        None | Some(Value::Undefined) => None,
        Some(_) => Some(string(args, 1)?),
    }
    .filter(|tag| !tag.is_empty() && !tag.eq_ignore_ascii_case("tag_origin"))
    .map(Arc::<str>::from);
    let arc = |index| -> Result<f32, String> {
        Ok(optional(args, index, float)?
            .unwrap_or(180.0)
            .clamp(0.0, 180.0))
    };
    let clamp = (view != LinkView::Absolute && args.len() > 3)
        .then(|| -> Result<[f32; 4], String> { Ok([-arc(5)?, arc(6)?, -arc(3)?, arc(4)?]) })
        .transpose()?;
    let (base, axis) = super::super::players::link_parent_pose(world, parent, tag.as_deref());
    let origin = FrameWorld::from_world(world)
        .player(ClientId(client))
        .map_or(base, |ps| ps.origin);
    let delta: [f32; 3] = std::array::from_fn(|i| origin[i] - base[i]);
    let local = if view == LinkView::WeaponDelta {
        [0.0; 3]
    } else {
        std::array::from_fn(|i| (0..3).map(|j| delta[j] * axis[i][j]).sum())
    };
    let use_tag_angles = optional(args, 7, int)?.is_some_and(|value| value != 0);
    let restore_view = (view == LinkView::WeaponDelta)
        .then(|| {
            FrameWorld::from_world(world)
                .player(ClientId(client))
                .map(|ps| ps.viewangles)
        })
        .flatten();
    super::super::players::link_player(
        world,
        client,
        PlayerLink {
            parent,
            tag,
            origin: local,
            angles: [0.0; 3],
            view,
            clamp,
            parent_axis: axis,
            view_fraction: optional(args, 2, float)?.unwrap_or(1.0).clamp(0.0, 1.0),
            use_tag_angles,
            link_axis: if use_tag_angles {
                axis
            } else {
                math_iw4::angles_to_axis([0.0; 3])
            },
            restore_view,
        },
    );
    Ok(Value::Undefined)
}

fn flag(args: &[Value], index: usize) -> Result<bool, String> {
    Ok(optional(args, index, int)?.unwrap_or(1) != 0)
}

fn weapon_arg(world: &mut World, args: &[Value], index: usize) -> Result<u32, String> {
    let name = string(args, index)?;
    script_player::weapon_named(&FrameWorld::from_world(world), &name)
}

fn player_weapon(
    world: &mut World,
    id: ClientId,
    args: &[Value],
    index: usize,
) -> Result<u32, String> {
    let weapon = weapon_arg(world, args, index)?;
    let native = super::super::players::bridged_weapon(world, id.0, weapon);
    let frame = FrameWorld::from_world(world);
    if frame
        .weapon_combat_row(native)
        .is_some_and(|facts| facts.weap_type == weapon_iw4::WEAPTYPE_SHIELD)
        && let Some(ps) = frame.player(id)
        && !ps.weapons.contains(&(native as i32))
        && let Some(owned) = ps
            .weapons
            .iter()
            .copied()
            .filter_map(|weapon| u32::try_from(weapon).ok())
            .find(|weapon| {
                frame
                    .weapon_combat_row(*weapon)
                    .is_some_and(|facts| facts.weap_type == weapon_iw4::WEAPTYPE_SHIELD)
            })
    {
        return Ok(owned);
    }
    Ok(native)
}

fn weapon_value(world: &mut World, weapon: u32) -> Value {
    Value::String(script_player::weapon_name(&FrameWorld::from_world(world), weapon).into())
}

fn controls(
    world: &mut World,
    receiver: &Value,
    set: impl FnOnce(&mut crate::match_state::ScriptControls),
) -> Result<Value, String> {
    let id = client_of(world, receiver)?;
    let mut frame = FrameWorld::from_world(world);
    set(&mut frame.client_meta_mut(id).controls);
    Ok(Value::Undefined)
}

fn register_body(registry: &mut NativeRegistry) {
    registry.register(Method, "spawn", |world, receiver, args| {
        let client = player(world, receiver)?;
        let origin = vector(args, 0)?;
        let angles = vector(args, 1)?;
        let state = slot(world, client)?.sessionstate.clone();
        slot(world, client)?.last_stand_until_ms = None;
        let tick = world.resource::<crate::step::StepRequest>().tick;
        script_player::spawn(
            &mut FrameWorld::from_world(world),
            tick,
            ClientId(client),
            origin,
            angles,
            &state,
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "freezecontrols", |world, receiver, args| {
        let on = flag(args, 0)?;
        controls(world, receiver, |c| c.frozen = on)
    });
    registry.register(Method, "allowjump", |world, receiver, args| {
        let on = flag(args, 0)?;
        controls(world, receiver, |c| c.jump_disabled = !on)
    });
    registry.register(Method, "canmantle", |world, receiver, args| {
        if !args.is_empty() {
            return Err("CanMantle expects no arguments".into());
        }
        let id = client_of(world, receiver)?;
        Ok(Value::Int(
            crate::step::script_mantle(&mut FrameWorld::from_world(world), id, false)?.into(),
        ))
    });
    registry.register(Method, "forcemantle", |world, receiver, args| {
        if !args.is_empty() {
            return Err("ForceMantle expects no arguments".into());
        }
        let id = client_of(world, receiver)?;
        if !crate::step::script_mantle(&mut FrameWorld::from_world(world), id, true)? {
            return Err("player cannot mantle the current ledge".into());
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "allowads", |world, receiver, args| {
        let allow = flag(args, 0)?;
        let id = client_of(world, receiver)?;
        let mut frame = FrameWorld::from_world(world);
        let ps = frame.player_mut(id).ok_or("player has not spawned")?;
        if allow {
            ps.weap_flags &= !playerstate_iw4::weap_flags::NO_ADS;
        } else {
            ps.weap_flags |= playerstate_iw4::weap_flags::NO_ADS;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "allowsprint", |world, receiver, args| {
        let allow = flag(args, 0)?;
        let id = client_of(world, receiver)?;
        let mut frame = FrameWorld::from_world(world);
        let ps = frame.player_mut(id).ok_or("player has not spawned")?;
        if allow {
            ps.pm_flags &= !playerstate_iw4::pm_flags::SPRINT_BLOCKED;
        } else {
            ps.pm_flags |= playerstate_iw4::pm_flags::SPRINT_BLOCKED;
            movement_iw4::end_sprint(
                ps,
                &playerstate_iw4::UserCmd {
                    server_time: ps.command_time,
                    ..Default::default()
                },
            );
        }
        Ok(Value::Undefined)
    });
    macro_rules! control {
        ($($name:literal => $field:ident = $value:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, _| {
                controls(world, receiver, |c| c.$field = $value)
            });
        )*};
    }
    control!(
        "disableweapons" => weapons_disabled = true,
        "enableweapons" => weapons_disabled = false,
        "disableoffhandweapons" => offhands_disabled = true,
        "enableoffhandweapons" => offhands_disabled = false,
        "disableweaponswitch" => switch_disabled = true,
        "enableweaponswitch" => switch_disabled = false,
        "disableusability" => usability_disabled = true,
        "enableusability" => usability_disabled = false,
    );
    registry.register(Method, "setmovespeedscale", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let scale = float(args, 0)?;
        if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
            ps.move_speed_scale_multiplier = scale;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "setnormalhealth", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let fraction = float(args, 0)?.clamp(0.0, 1.0);
        if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
            ps.health = ((ps.max_health as f32 * fraction) as i32).max(1);
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "getplayerangles", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        Ok(Value::Vector(
            FrameWorld::from_world(world)
                .player(id)
                .map_or([0.0; 3], |ps| ps.viewangles),
        ))
    });
    registry.register(Method, "setvelocity", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let velocity = vector(args, 0)?;
        if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
            ps.velocity = velocity;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "setplayerangles", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let angles = vector(args, 0)?;
        FrameWorld::from_world(world).set_viewangles(id, angles);
        Ok(Value::Undefined)
    });
    registry.register(Method, "getstance", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        Ok(Value::string(script_player::stance(
            &FrameWorld::from_world(world),
            id,
        )))
    });
    registry.register(Method, "setstance", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let stance = string(args, 0)?;
        let mut frame = FrameWorld::from_world(world);
        if let Some(ps) = frame.player_mut(id) {
            use playerstate_iw4::eflags::{DUCK, PRONE};
            ps.e_flags &= !(DUCK | PRONE);
            ps.e_flags |= match stance.as_str() {
                "crouch" => DUCK,
                "prone" => PRONE,
                _ => 0,
            };
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "isonground", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let frame = FrameWorld::from_world(world);
        Ok(Value::Int(
            frame
                .player(id)
                .is_some_and(|ps| ps.ground_entity_num != playerstate_iw4::ENTITYNUM_NONE)
                .into(),
        ))
    });
    registry.register(Method, "isonladder", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let frame = FrameWorld::from_world(world);
        Ok(Value::Int(
            frame
                .player(id)
                .is_some_and(|ps| ps.pm_flags & playerstate_iw4::pm_flags::LADDER != 0)
                .into(),
        ))
    });
    registry.register(Method, "ismantling", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let frame = FrameWorld::from_world(world);
        Ok(Value::Int(
            frame
                .player(id)
                .is_some_and(|ps| ps.mantle_flags & playerstate_iw4::mantle_flags::ACTIVE != 0)
                .into(),
        ))
    });
    registry.register(Method, "playerads", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        Ok(Value::Float(
            FrameWorld::from_world(world)
                .player(id)
                .map_or(0.0, |ps| ps.f_weapon_pos_frac),
        ))
    });
    macro_rules! button {
        ($($name:literal => $mask:expr),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, _| {
                let id = client_of(world, receiver)?;
                let held = script_player::buttons(&mut FrameWorld::from_world(world), id);
                Ok(Value::Int((held & ($mask) != 0).into()))
            });
        )*};
    }
    {
        use playerstate_iw4::buttons::*;
        button!(
            "attackbuttonpressed" => ATTACK,
            "usebuttonpressed" => USE | USE_RELOAD,
            "meleebuttonpressed" => MELEE_CHARGE,
            "fragbuttonpressed" => FRAG,
            "secondaryoffhandbuttonpressed" => SMOKE,
            "adsbuttonpressed" => ADS,
            "jumpbuttonpressed" => JUMP,
        );
    }
    registry.register(Method, "buttonpressed", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let key = string(args, 0)?.to_ascii_lowercase();
        let mask = crate::script::command_buttons(&key);
        let held = script_player::buttons(&mut FrameWorld::from_world(world), id);
        Ok(Value::Int((held & mask != 0).into()))
    });
    registry.register(Method, "playerlinkto", |world, receiver, args| {
        link_to(world, receiver, args, LinkView::Free)
    });
    registry.register(Method, "playerlinktodelta", |world, receiver, args| {
        link_to(world, receiver, args, LinkView::Delta)
    });
    registry.register(
        Method,
        "playerlinkweaponviewtodelta",
        |world, receiver, args| link_to(world, receiver, args, LinkView::WeaponDelta),
    );
    registry.register(Method, "playerlinktoabsolute", |world, receiver, args| {
        link_to(world, receiver, args, LinkView::Absolute)
    });
    for name in ["playerlinkedoffsetenable", "playerlinkedoffsetdisable"] {
        registry.register(Method, name, |world, receiver, _| {
            client_of(world, receiver)?;
            Ok(Value::Undefined)
        });
    }
    registry.register(Method, "setactionslot", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let index = int(args, 0)?;
        let slot = usize::try_from(index - 1)
            .ok()
            .filter(|slot| *slot < 4)
            .ok_or_else(|| format!("action slot {index} is out of range 1..4"))?;
        let kind = string(args, 1)?.to_ascii_lowercase();
        let (kind, param) = match kind.as_str() {
            "" | "none" => (0, 0),
            "weapon" => (1, weapon_arg(world, args, 2)? as i32),
            "altmode" => (2, 0),
            "nightvision" => (3, 0),
            other => return Err(format!("invalid action slot type '{other}'")),
        };
        if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
            ps.action_slot_type[slot] = kind;
            ps.action_slot_param[slot] = param;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "shellshock", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let name = string(args, 0)?.to_ascii_lowercase();
        let seconds = float(args, 1)?;
        if seconds < 0.0 {
            return Err(format!("shellshock duration {seconds} is negative"));
        }
        let index = *world
            .resource::<Runtime>()
            .precached
            .get(&("shellshock", name.clone()))
            .ok_or_else(|| format!("shellshock '{name}' was not precached"))?;
        let mut frame = FrameWorld::from_world(world);
        let shock = frame
            .shock(&name)
            .cloned()
            .ok_or_else(|| format!("no shock file for shellshock '{name}'"))?;
        let now = crate::level_time_ms(frame.ecs().resource::<crate::step::StepRequest>().tick);
        let Some(ps) = frame.player_mut(id) else {
            return Ok(Value::Undefined);
        };
        ps.shellshock_index = index;
        ps.shellshock_time = now;
        ps.shellshock_duration = (seconds * 1000.0) as i32;
        let alive = ps.health > 0;
        if alive {
            ps.pm_flags |= playerstate_iw4::pm_flags::SHELLSHOCKED;
        }
        frame.client_meta_mut(id).shellshock = Some(shock);
        if alive {
            crate::combat::apply_player_anim_event(&mut frame, id, 20);
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "stopshellshock", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
            ps.shellshock_duration = 0;
            ps.pm_flags &= !playerstate_iw4::pm_flags::SHELLSHOCKED;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "radarjamon", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
            ps.e_flags |= playerstate_iw4::eflags::RADAR_JAM;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "radarjamoff", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
            ps.e_flags &= !playerstate_iw4::eflags::RADAR_JAM;
        }
        Ok(Value::Undefined)
    });
}

fn shield_tag(args: &[Value], index: usize) -> Result<bool, String> {
    match string(args, index)?.as_str() {
        "tag_weapon_left" => Ok(false),
        "tag_shield_back" => Ok(true),
        tag => Err(format!("unsupported shield tag '{tag}'")),
    }
}

fn shield_weapon(frame: &FrameWorld, id: ClientId, model: &str) -> Option<u32> {
    let attached = frame
        .client_meta(id)
        .and_then(|meta| meta.shield)
        .map(|shield| shield.weapon as i32);
    let owned = frame.player(id).map_or([0; 15], |ps| ps.weapons);
    frame.shield_weapon_for_model(model, attached.into_iter().chain(owned))
}

fn register_shield(registry: &mut NativeRegistry) {
    registry.register(Method, "attachshieldmodel", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let model = string(args, 0)?;
        let on_back = shield_tag(args, 1)?;
        let mut frame = FrameWorld::from_world(world);
        let weapon = shield_weapon(&frame, id, &model)
            .ok_or_else(|| format!("shield model '{model}' unavailable"))?;
        frame.client_meta_mut(id).shield = Some(crate::ShieldAttachment { weapon, on_back });
        Ok(Value::Undefined)
    });
    registry.register(Method, "moveshieldmodel", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let model = string(args, 0)?;
        let from = shield_tag(args, 1)?;
        let on_back = shield_tag(args, 2)?;
        let mut frame = FrameWorld::from_world(world);
        let weapon = shield_weapon(&frame, id, &model)
            .ok_or_else(|| format!("shield model '{model}' unavailable"))?;
        let attachment = &mut frame.client_meta_mut(id).shield;
        if *attachment
            == Some(crate::ShieldAttachment {
                weapon,
                on_back: from,
            })
        {
            *attachment = Some(crate::ShieldAttachment { weapon, on_back });
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "detachshieldmodel", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let model = string(args, 0)?;
        let on_back = shield_tag(args, 1)?;
        let mut frame = FrameWorld::from_world(world);
        let weapon = shield_weapon(&frame, id, &model);
        let attachment = &mut frame.client_meta_mut(id).shield;
        if attachment.is_some_and(|s| Some(s.weapon) == weapon && s.on_back == on_back) {
            *attachment = None;
        }
        Ok(Value::Undefined)
    });
}

fn register_inventory(registry: &mut NativeRegistry) {
    registry.register(
        crate::script::Namespace::Function,
        "getweaponmodel",
        |world, _, args| {
            let weapon = weapon_arg(world, args, 0)?;
            let frame = FrameWorld::from_world(world);
            let model = frame
                .weapon_world_model(weapon)
                .map_or("", |(model, _)| model);
            Ok(Value::String(model.into()))
        },
    );
    registry.register(
        crate::script::Namespace::Function,
        "getweaponhidetags",
        |world, _, args| {
            let weapon = weapon_arg(world, args, 0)?;
            let tags: Vec<Value> = FrameWorld::from_world(world)
                .weapon_world_model(weapon)
                .map(|(_, tags)| {
                    tags.iter()
                        .map(|t| Value::String(t.as_str().into()))
                        .collect()
                })
                .unwrap_or_default();
            new_array(world, tags)
        },
    );
    registry.register(Method, "giveweapon", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let named = weapon_arg(world, args, 0)?;
        let weapon = player_weapon(world, id, args, 0)?;
        let t5 = world
            .resource::<Runtime>()
            .program
            .as_ref()
            .is_some_and(|program| program.rules() == crate::script::Realm::T5);
        let model = if t5 {
            (optional(args, 2, int)?.unwrap_or(0) & 63) as u8
        } else {
            optional(args, 1, int)?.unwrap_or(0).clamp(0, 15) as u8
        };
        let model = if named != weapon {
            super::super::players::selected_camouflage(world, id.0, weapon).unwrap_or(model)
        } else {
            model
        };
        let akimbo = !t5 && optional(args, 2, int)?.unwrap_or(0) != 0;
        script_player::give_weapon(&mut FrameWorld::from_world(world), id, weapon, akimbo)?;
        script_player::set_weapon_model(&mut FrameWorld::from_world(world), id, weapon, model);
        super::super::players::give_carried_insertion(world, id.0, receiver, named)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "takeweapon", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = player_weapon(world, id, args, 0)?;
        script_player::take_weapon(&mut FrameWorld::from_world(world), id, weapon);
        Ok(Value::Undefined)
    });
    registry.register(Method, "takeallweapons", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        script_player::take_all_weapons(&mut FrameWorld::from_world(world), id);
        Ok(Value::Undefined)
    });
    registry.register(Method, "hasweapon", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = player_weapon(world, id, args, 0)?;
        Ok(Value::Int(
            script_player::has_weapon(&FrameWorld::from_world(world), id, weapon).into(),
        ))
    });
    registry.register(Method, "setspawnweapon", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = player_weapon(world, id, args, 0)?;
        script_player::set_spawn_weapon(&mut FrameWorld::from_world(world), id, weapon)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "switchtoweapon", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = player_weapon(world, id, args, 0)?;
        script_player::switch_to_weapon(&mut FrameWorld::from_world(world), id, weapon);
        Ok(Value::Undefined)
    });
    registry.register(
        Method,
        "switchtoweaponimmediate",
        |world, receiver, args| {
            let id = client_of(world, receiver)?;
            let weapon = player_weapon(world, id, args, 0)?;
            script_player::switch_to_weapon_immediate(
                &mut FrameWorld::from_world(world),
                id,
                weapon,
            )?;
            Ok(Value::Undefined)
        },
    );
    registry.register(Method, "getcurrentweaponclipammo", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let frame = FrameWorld::from_world(world);
        let weapon = frame.player(id).map_or(0, |ps| ps.weapon);
        Ok(Value::Int(script_player::ammo_clip(&frame, id, weapon)))
    });
    registry.register(Method, "isreloading", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let frame = FrameWorld::from_world(world);
        let reloading = frame.player(id).is_some_and(|ps| {
            [ps.weaponstate_primary, ps.weaponstate_secondary]
                .into_iter()
                .any(|state| {
                    weapon_iw4::WeaponState::from_i32(state)
                        .is_ok_and(|state| state.is_reload_family())
                })
        });
        Ok(Value::Int(reloading.into()))
    });
    registry.register(Method, "isswitchingweapon", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let frame = FrameWorld::from_world(world);
        let switching = frame.player(id).is_some_and(|ps| {
            [ps.weaponstate_primary, ps.weaponstate_secondary]
                .into_iter()
                .any(super::super::weapons::is_changing_weapon)
        });
        Ok(Value::Int(switching.into()))
    });
    registry.register(Method, "isdualwielding", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let frame = FrameWorld::from_world(world);
        let dual = frame.player(id).is_some_and(|ps| {
            weapon_iw4::num_hands_for_held(&ps.weapons, &ps.weapon_data, ps.weapon) != 0
        });
        Ok(Value::Int(dual.into()))
    });
    registry.register(Method, "getcurrentweapon", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let weapon = FrameWorld::from_world(world)
            .player(id)
            .map_or(0, |ps| ps.weapon);
        let weapon = super::super::players::script_weapon(world, id.0, weapon);
        Ok(weapon_value(world, weapon))
    });
    registry.register(Method, "getcurrentprimaryweapon", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let weapon = FrameWorld::from_world(world)
            .player(id)
            .map_or(0, |ps| ps.weapon_primary);
        let weapon = super::super::players::script_weapon(world, id.0, weapon);
        Ok(weapon_value(world, weapon))
    });
    macro_rules! weapon_list {
        ($($name:literal => $list:ident),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, _| {
                let id = client_of(world, receiver)?;
                let list = script_player::WeaponList::$list;
                let weapons = script_player::weapons(&FrameWorld::from_world(world), id, list);
                let names = weapons
                    .into_iter()
                    .map(|w| {
                        let w = super::super::players::script_weapon(world, id.0, w);
                        weapon_value(world, w)
                    })
                    .collect();
                new_array(world, names)
            });
        )*};
    }
    weapon_list!(
        "getweaponslistall" => All,
        "getweaponslistprimaries" => Primaries,
        "getweaponslistoffhands" => Offhands,
        "getweaponslistitems" => Items,
        "getweaponslistexclusives" => Exclusives,
    );
    registry.register(Method, "getweaponammoclip", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = player_weapon(world, id, args, 0)?;
        Ok(Value::Int(script_player::ammo_clip(
            &FrameWorld::from_world(world),
            id,
            weapon,
        )))
    });
    registry.register(Method, "getammocount", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = player_weapon(world, id, args, 0)?;
        let frame = FrameWorld::from_world(world);
        Ok(Value::Int(
            script_player::ammo_clip(&frame, id, weapon)
                + script_player::ammo_stock(&frame, id, weapon),
        ))
    });
    registry.register(Method, "ischangingweapon", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let state = FrameWorld::from_world(world)
            .player(id)
            .map_or(0, |ps| ps.weaponstate_primary);
        Ok(Value::Int(
            super::super::weapons::is_changing_weapon(state).into(),
        ))
    });
    registry.register(Method, "getweaponammostock", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = player_weapon(world, id, args, 0)?;
        Ok(Value::Int(script_player::ammo_stock(
            &FrameWorld::from_world(world),
            id,
            weapon,
        )))
    });
    registry.register(Method, "setweaponammoclip", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = player_weapon(world, id, args, 0)?;
        let count = int(args, 1)?;
        script_player::set_ammo_clip(&mut FrameWorld::from_world(world), id, weapon, count);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setweaponammostock", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = player_weapon(world, id, args, 0)?;
        let count = int(args, 1)?;
        script_player::set_ammo_stock(&mut FrameWorld::from_world(world), id, weapon, count);
        Ok(Value::Undefined)
    });
    registry.register(Method, "givemaxammo", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = player_weapon(world, id, args, 0)?;
        script_player::give_max_ammo(&mut FrameWorld::from_world(world), id, weapon);
        Ok(Value::Undefined)
    });
    registry.register(Method, "givestartammo", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = player_weapon(world, id, args, 0)?;
        script_player::give_start_ammo(&mut FrameWorld::from_world(world), id, weapon);
        Ok(Value::Undefined)
    });
    registry.register(Method, "anyammoforweaponmodes", |world, receiver, args| {
        let id = client_of(world, receiver)?;
        let weapon = player_weapon(world, id, args, 0)?;
        let frame = FrameWorld::from_world(world);
        let any = script_player::ammo_clip(&frame, id, weapon) > 0
            || script_player::ammo_stock(&frame, id, weapon) > 0;
        Ok(Value::Int(any.into()))
    });
    registry.register(Method, "setperk", |world, receiver, args| {
        let client = player(world, receiver)?;
        let name: Arc<str> = string(args, 0)?.into();
        script_player::set_perk(
            &mut FrameWorld::from_world(world),
            ClientId(client),
            &name,
            true,
        );
        if let Some((index, code)) = perk_slot_code(world, &name)
            && let Some(ps) = FrameWorld::from_world(world).player_mut(ClientId(client))
        {
            ps.perk_slots[index] = code;
        }
        slot(world, client)?.perks.insert(name);
        Ok(Value::Undefined)
    });
    registry.register(Method, "unsetperk", |world, receiver, args| {
        let client = player(world, receiver)?;
        let name: Arc<str> = string(args, 0)?.into();
        script_player::set_perk(
            &mut FrameWorld::from_world(world),
            ClientId(client),
            &name,
            false,
        );
        if let Some((index, code)) = perk_slot_code(world, &name)
            && let Some(ps) = FrameWorld::from_world(world).player_mut(ClientId(client))
            && ps.perk_slots[index] == code
        {
            ps.perk_slots[index] = 0;
        }
        slot(world, client)?.perks.remove(&name);
        Ok(Value::Undefined)
    });
    registry.register(Method, "hasperk", |world, receiver, args| {
        let client = player(world, receiver)?;
        let name = string(args, 0)?;
        Ok(Value::Int(
            slot(world, client)?.perks.contains(name.as_str()).into(),
        ))
    });
    registry.register(Method, "clearperks", |world, receiver, _| {
        let client = player(world, receiver)?;
        script_player::clear_perks(&mut FrameWorld::from_world(world), ClientId(client));
        slot(world, client)?.perks.clear();
        Ok(Value::Undefined)
    });
    macro_rules! offhand_class {
        ($($name:literal => $field:ident),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, args| {
                let id = client_of(world, receiver)?;
                let mut class = offhand_class_of(world, &string(args, 0)?);
                if let Some(bridge) = world.resource::<Runtime>().weapon_bridge.get(&id.0).cloned() {
                    let frame = FrameWorld::from_world(world);
                    if let Some(native_class) = bridge.iter().find_map(|(stand_in, native)| {
                        let source = frame.equipment_facts_for(*stand_in)?;
                        let target = frame.equipment_facts_for(*native)?;
                        (source.offhand_class == class).then_some(target.offhand_class)
                    }) {
                        class = native_class;
                    }
                }
                if let Some(ps) = FrameWorld::from_world(world).player_mut(id) {
                    ps.$field = class;
                }
                let name = OFFHAND_CLASSES.get(class as usize).copied().unwrap_or("none");
                Ok(Value::string(name))
            });
        )*};
    }
    offhand_class!(
        "setoffhandprimaryclass" => offhand_primary,
        "setoffhandsecondaryclass" => offhand_secondary,
        "switchtooffhand" => offhand_primary,
    );
    registry.register(Method, "getoffhandprimaryclass", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let class = FrameWorld::from_world(world)
            .player(id)
            .map_or(0, |ps| ps.offhand_primary);
        Ok(Value::string(
            OFFHAND_CLASSES
                .get(class as usize)
                .copied()
                .unwrap_or("none"),
        ))
    });
    registry.register(Method, "getoffhandsecondaryclass", |world, receiver, _| {
        let id = client_of(world, receiver)?;
        let class = FrameWorld::from_world(world)
            .player(id)
            .map_or(0, |ps| ps.offhand_secondary);
        Ok(Value::string(match class {
            2 => "smoke",
            3 => "flash",
            _ => "none",
        }))
    });
}
