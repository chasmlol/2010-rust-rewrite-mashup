use super::super::args::{arg, float, int, kind, optional, string, vector};
use super::super::arrays::new_array;
use super::super::entities::{EntityKind, ScriptEntity};
use super::super::mechanics::{Mechanics, Motion, MotionPath};
use super::iw4::precache;
use super::math::distance_sq;
use crate::bullet_collision::{
    MASK_PLAYER_SOLID, MASK_SHOT, PLAYER_MAXS, PLAYER_MINS, TraceOutcome,
};
use crate::script::{Arc, ArrayKey, Namespace, NativeRegistry, Runtime, Value};
use bevy_ecs::prelude::World;

const ZERO: [f32; 3] = [0.0; 3];

fn runtime(world: &mut World) -> bevy_ecs::world::Mut<'_, Runtime> {
    world.resource_mut::<Runtime>()
}

fn now_ms(world: &World) -> i64 {
    world
        .get_resource::<crate::step::StepRequest>()
        .map_or(0, |r| i64::from(r.tick.0) * i64::from(crate::MATCH_TICK_MS))
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    add(a, scale(sub(b, a), t))
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn transpose(m: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    std::array::from_fn(|row| std::array::from_fn(|col| m[col][row]))
}

fn describe(value: &Value) -> &'static str {
    match value {
        Value::Object(_) => "not an entity",
        other => kind(other),
    }
}

pub(crate) fn entity_id(world: &World, value: &Value) -> Result<u64, String> {
    match world.resource::<Runtime>().entity(value) {
        Some((id, e)) if e.kind != EntityKind::HudElem => Ok(id),
        Some(_) => Err("hud element is not an entity".into()),
        None => Err(format!("{} is not an entity", describe(value))),
    }
}

fn with_entity(
    world: &mut World,
    receiver: &Value,
    change: impl FnOnce(&mut ScriptEntity),
) -> Result<Value, String> {
    let id = entity_id(world, receiver)?;
    change(runtime(world).entities.get_mut(&id).unwrap());
    Ok(Value::Undefined)
}

fn vector_field(world: &mut World, id: u64, name: &str) -> [f32; 3] {
    match super::super::players::entity_field(world, id, name) {
        Value::Vector(v) => v,
        _ => ZERO,
    }
}

fn origin_of(world: &mut World, value: &Value) -> Result<[f32; 3], String> {
    match value {
        Value::Object(id) if world.resource::<Runtime>().live(id) => {
            Ok(vector_field(world, *id, "origin"))
        }
        Value::Vector(v) => Ok(*v),
        other => Err(format!("{} has no origin", describe(other))),
    }
}

pub(crate) fn keyed_array(world: &mut World, pairs: Vec<(&str, Value)>) -> Result<Value, String> {
    let mut runtime = runtime(world);
    let id = runtime.next_object;
    runtime.next_object = id.checked_add(1).ok_or("object identifier exhausted")?;
    runtime.arrays.insert(
        id,
        pairs
            .into_iter()
            .map(|(k, v)| (ArrayKey::String(k.into()), v))
            .collect(),
    );
    Ok(Value::Array(id))
}

pub(crate) fn trace(
    world: &mut World,
    start: [f32; 3],
    end: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    mask: u32,
) -> trace_iw4::Trace {
    super::super::presence::settled(world).trace_world(start, end, mins, maxs, mask)
}

const SIGHT_CONE_MASK: u32 = 0x0801;
const DAMAGE_CONE_MASK: u32 = 0x0080_2011;

fn shot_mask(args: &[Value]) -> Result<u32, String> {
    Ok(if int(args, 2)? != 0 {
        MASK_SHOT
    } else {
        MASK_SHOT & !crate::bullet_collision::CONTENTS_BODY
    })
}

#[derive(Clone, Copy, Default)]
pub(crate) struct TraceIgnore {
    pub client: Option<crate::ClientId>,
    pub other_client: Option<crate::ClientId>,
    pub model: Option<crate::ScriptModelId>,
}

impl TraceIgnore {
    pub(crate) fn with(mut self, other: TraceIgnore) -> Self {
        self.other_client = other.client;
        self.model = self.model.or(other.model);
        self
    }
}

pub(crate) fn trace_ignore(world: &World, value: Option<&Value>) -> TraceIgnore {
    let runtime = world.resource::<Runtime>();
    let Some(value) = value else {
        return TraceIgnore::default();
    };
    if let Some(client) = runtime.player_client_of(value) {
        return TraceIgnore {
            client: Some(crate::ClientId(client)),
            ..Default::default()
        };
    }
    TraceIgnore {
        model: runtime.entity(value).and_then(|(_, e)| e.presence),
        ..Default::default()
    }
}

pub(crate) fn entity_trace(
    world: &mut World,
    start: [f32; 3],
    end: [f32; 3],
    mask: u32,
    ignore: TraceIgnore,
) -> TraceOutcome {
    super::super::presence::settled(world).current_sensor_trace(
        crate::bullet_collision::BulletTraceQuery {
            start,
            end,
            mask,
            ignore: ignore.client,
            ignore_hit: ignore.other_client,
            ignore_model: ignore.model,
        },
    )
}

fn surface_name(collider: crate::bullet_collision::ColliderId) -> &'static str {
    use crate::bullet_collision::ColliderId;
    let flags = match collider {
        ColliderId::World { surface_flags, .. }
        | ColliderId::EntityDObjBone { surface_flags, .. }
        | ColliderId::EntityLinkedBrush { surface_flags, .. } => surface_flags,
        ColliderId::Player { .. } => return "flesh",
    };
    weapon_iw4::SURFACE_TYPE_NAMES
        .get(trace_iw4::surface_type_from_flags(flags) as usize)
        .copied()
        .unwrap_or("default")
}

pub(crate) fn collider_entity(
    world: &World,
    collider: crate::bullet_collision::ColliderId,
) -> Value {
    use crate::bullet_collision::ColliderId;
    let runtime = world.resource::<Runtime>();
    let found = match collider {
        ColliderId::World { .. } => None,
        ColliderId::Player { client, .. } => runtime.players.get(&client.0).map(|s| s.object),
        ColliderId::EntityDObjBone { owner, .. } | ColliderId::EntityLinkedBrush { owner, .. } => {
            let model = owner.script_model();
            runtime
                .entities
                .iter()
                .find(|(_, e)| e.presence.is_some() && e.presence == model)
                .map(|(id, _)| *id)
        }
    };
    found.map_or(Value::Undefined, Value::Object)
}

fn cone_trace(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    mask: u32,
) -> Result<Value, String> {
    let origin = vector(args, 0)?;
    let id = entity_id(world, receiver)?;
    let ignore = trace_ignore(world, Some(receiver)).with(trace_ignore(world, args.get(1)));
    let target = vector_field(world, id, "origin");
    let player = runtime(world).player_client(id);
    let (center, right, up) = match player {
        Some(client) => {
            let eye_height = crate::frame::FrameWorld::from_world(world)
                .player(crate::ClientId(client))
                .map_or(0.0, |ps| ps.view_height_current);
            let forward = normalize([origin[0] - target[0], origin[1] - target[1], 0.0]);
            (
                add(target, [0.0, 0.0, eye_height * 0.5]),
                [-forward[1] * 15.0, forward[0] * 15.0, 0.0],
                [0.0, 0.0, eye_height * 0.5],
            )
        }
        None => {
            let (mins, maxs) = super::super::triggers::entity_bounds(world, id);
            let center = add(target, scale(add(mins, maxs), 0.5));
            let corner = scale(sub(maxs, mins), 0.5);
            let v = normalize(sub(origin, center));
            let side = normalize([-v[1], v[0], 0.0]);
            let up = cross(v, side);
            let right_radius = (corner[0] * side[0]).abs() + (corner[1] * side[1]).abs();
            let up_radius: f32 = (0..3).map(|i| (corner[i] * up[i]).abs()).sum();
            (center, scale(side, right_radius), scale(up, up_radius))
        }
    };
    let points = [
        center,
        add(add(center, right), up),
        add(sub(center, right), up),
        sub(add(center, right), up),
        sub(sub(center, right), up),
    ];
    let hits = points
        .iter()
        .filter(|point| {
            matches!(
                entity_trace(world, origin, **point, mask, ignore),
                TraceOutcome::Miss { .. }
            )
        })
        .count();
    Ok(Value::Float(match (player, hits) {
        (_, 0) => 0.0,
        (Some(_), hits) => (hits as f32 / 3.0).min(1.0),
        (None, _) => 1.0,
    }))
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = dot(v, v).sqrt();
    if len > 0.0 { scale(v, 1.0 / len) } else { ZERO }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn trace_passed(world: &mut World, start: [f32; 3], end: [f32; 3], mask: u32) -> bool {
    let t = trace(world, start, end, ZERO, ZERO, mask);
    t.fraction >= 1.0 && t.startsolid == 0
}

fn matching_entities(world: &mut World, args: &[Value]) -> Result<Vec<u64>, String> {
    let value = string(args, 0)?;
    let key = string(args, 1)?.to_ascii_lowercase();
    let mut runtime = runtime(world);
    let field = runtime.symbol(&key);
    let runtime = &*runtime;
    Ok(runtime
        .entities
        .iter()
        .filter(|(_, e)| e.kind != EntityKind::HudElem)
        .map(|(id, _)| *id)
        .filter(|id| {
            matches!(
                runtime.objects.get(id).and_then(|f| f.get(&field)),
                Some(Value::String(s)) if **s == *value
            )
        })
        .collect())
}

fn classname_prefix(world: &World, ids: Vec<u64>, prefix: &str) -> Vec<u64> {
    let runtime = world.resource::<Runtime>();
    ids.into_iter()
        .filter(|id| runtime.entities[id].classname.starts_with(prefix))
        .collect()
}

fn single(ids: Vec<u64>, what: &str) -> Result<Value, String> {
    match ids.as_slice() {
        [] => Ok(Value::Undefined),
        [id] => Ok(Value::Object(*id)),
        _ => Err(format!("{what} used with more than one entity")),
    }
}

fn objects(world: &mut World, ids: Vec<u64>) -> Result<Value, String> {
    new_array(world, ids.into_iter().map(Value::Object).collect())
}

fn array_values(world: &World, value: &Value) -> Result<Vec<Value>, String> {
    let Value::Array(id) = value else {
        return Err(format!("{} is not an array", kind(value)));
    };
    Ok(world
        .resource::<Runtime>()
        .arrays
        .get(id)
        .ok_or("invalid array reference")?
        .values()
        .cloned()
        .collect())
}

fn seconds_ms(seconds: f32) -> Result<i64, String> {
    if !(seconds > 0.0) {
        return Err("total time must be positive".into());
    }
    Ok((seconds * 1000.0).round().max(1.0) as i64)
}

fn start_motion(
    world: &mut World,
    receiver: &Value,
    field: &'static str,
    path: MotionPath,
    duration_ms: i64,
    done: &'static str,
) -> Result<Value, String> {
    let start_ms = now_ms(world);
    let object = entity_id(world, receiver)?;
    if field == "origin" && world.resource::<Mechanics>().sliding(object) {
        return Err("stop slide movement before starting an origin move".into());
    }
    world.resource_mut::<Mechanics>().start(
        object,
        Motion {
            field,
            path,
            start_ms,
            duration_ms,
            done,
        },
    );
    Ok(Value::Undefined)
}

fn ramp(args: &[Value], time: f32, from: [f32; 3], to: [f32; 3]) -> Result<MotionPath, String> {
    let accel = optional(args, 2, float)?.unwrap_or(0.0);
    let decel = optional(args, 3, float)?.unwrap_or(0.0);
    if accel < 0.0 || decel < 0.0 {
        return Err("accel and decel time must not be negative".into());
    }
    if accel + decel > time {
        return Err("accel time plus decel time is greater than total time".into());
    }
    Ok(MotionPath::Linear {
        from,
        to,
        accel,
        decel,
    })
}

fn move_axis(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    axis: usize,
) -> Result<Value, String> {
    if !(2..=4).contains(&args.len()) {
        return Err(
            "axis move expects distance, seconds and optional acceleration/deceleration".into(),
        );
    }
    let delta = float(args, 0)?;
    let time = float(args, 1)?;
    let accel = optional(args, 2, float)?.unwrap_or(0.0);
    let decel = optional(args, 3, float)?.unwrap_or(0.0);
    if !delta.is_finite() || !time.is_finite() || !accel.is_finite() || !decel.is_finite() {
        return Err("axis move parameters must be finite".into());
    }
    let duration = seconds_ms(time)?;
    let id = entity_id(world, receiver)?;
    if world.resource::<Runtime>().player_client(id).is_some() {
        return Err("axis moves require a non-player entity".into());
    }
    let from = vector_field(world, id, "origin");
    let mut to = from;
    to[axis] += delta;
    if from.iter().chain(to.iter()).any(|v| !v.is_finite()) {
        return Err("axis move pose must be finite".into());
    }
    start_motion(
        world,
        receiver,
        "origin",
        ramp(args, time, from, to)?,
        duration,
        "movedone",
    )
}

fn rotate_by(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    axis: usize,
) -> Result<Value, String> {
    let delta = float(args, 0)?;
    let time = float(args, 1)?;
    let duration = seconds_ms(time)?;
    let id = entity_id(world, receiver)?;
    let from = vector_field(world, id, "angles");
    let mut to = from;
    to[axis] += delta;
    start_motion(
        world,
        receiver,
        "angles",
        ramp(args, time, from, to)?,
        duration,
        "rotatedone",
    )
}

fn add_angle(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    axis: usize,
) -> Result<Value, String> {
    let delta = float(args, 0)?;
    let id = entity_id(world, receiver)?;
    let mut angles = vector_field(world, id, "angles");
    angles[axis] += delta;
    runtime(world).set_object_field(id, "angles", Value::Vector(angles));
    Ok(Value::Undefined)
}

fn name(world: &World, id: i32) -> Result<String, String> {
    world
        .resource::<Runtime>()
        .precached
        .iter()
        .find(|((kind, _), index)| *kind == "fx" && **index == id)
        .map(|((_, name), _)| name.clone())
        .ok_or_else(|| format!("effect id {id} was not loaded"))
}

fn fx_on_tag(
    world: &mut World,
    args: &[Value],
    kind: entity_iw4::EntityEventKind,
) -> Result<Value, String> {
    let name = name(world, int(args, 0)?)?;
    let entity = arg(args, 1)?.clone();
    let tag = string(args, 2)?;
    let presence = runtime(world).presence_of(&entity);
    let fallback = (origin_of(world, &entity)?, [0.0, 0.0, 1.0]);
    let mut frame = super::super::presence::settled(world);
    let number = presence.and_then(|id| frame.gentity_number(id));
    let dobj = presence.and_then(|id| {
        frame
            .entity_collision_capabilities()
            .iter()
            .find(|row| row.owner.script_model() == Some(id))?
            .dobj
            .as_ref()
            .map(|dobj| (dobj.tag_bone(&tag), dobj.tag_world_pose(&tag)))
    });
    let (bone, pose) = dobj.unwrap_or((None, None));
    let (origin, direction) = pose.unwrap_or(fallback);
    let index = frame.effect_name_index(&name);
    match (number, bone.and_then(|bone| i32::try_from(bone).ok())) {
        (Some(number), Some(bone)) => {
            let tick = world.resource::<crate::step::StepRequest>().tick;
            crate::frame::FrameWorld::from_world(world).push_entity_event(
                tick,
                crate::EventAudience::All,
                kind,
                crate::EntityEventPayload {
                    number,
                    event_parm: i32::from(index) | bone << 8,
                    origin,
                    direction,
                    ..Default::default()
                },
            );
        }
        _ if kind == entity_iw4::EntityEventKind::PLAY_FX_ON_TAG => world_event(
            world,
            entity_iw4::EntityEventKind::PLAY_FX,
            index,
            origin,
            direction,
        ),
        _ => {}
    }
    Ok(Value::Undefined)
}

fn world_event(
    world: &mut World,
    kind: entity_iw4::EntityEventKind,
    index: u8,
    origin: [f32; 3],
    direction: [f32; 3],
) {
    let tick = world.resource::<crate::step::StepRequest>().tick;
    crate::frame::FrameWorld::from_world(world).push_entity_event(
        tick,
        crate::EventAudience::All,
        kind,
        crate::EntityEventPayload {
            number: i32::from(trace_iw4::ENTITYNUM_WORLD),
            event_parm: i32::from(index),
            origin,
            direction,
            ..Default::default()
        },
    );
}

pub(crate) fn sound_to(
    world: &mut World,
    audience: crate::EventAudience,
    alias: &str,
    number: i32,
    origin: [f32; 3],
) {
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let mut frame = crate::frame::FrameWorld::from_world(world);
    let index = frame.sound_alias_index(alias);
    frame.push_entity_event(
        tick,
        audience,
        entity_iw4::EntityEventKind::SOUND_ALIAS,
        crate::EntityEventPayload {
            number,
            event_parm: i32::from(index),
            origin,
            ..Default::default()
        },
    );
}

fn team_clients(world: &mut World, team: &str, except: Option<u32>) -> Vec<crate::ClientId> {
    let team = match team {
        "axis" => entity_iw4::TEAM_AXIS,
        "allies" => entity_iw4::TEAM_ALLIES,
        "spectator" => entity_iw4::TEAM_SPECTATOR,
        _ => return Vec::new(),
    };
    let frame = crate::frame::FrameWorld::from_world(world);
    frame
        .client_ids_sorted()
        .into_iter()
        .filter(|id| Some(id.0) != except)
        .filter(|id| {
            frame
                .client_meta(*id)
                .is_some_and(|m| m.client_state_team == team)
        })
        .collect()
}

pub(crate) fn play_sound_at(
    world: &mut World,
    origin: [f32; 3],
    alias: &str,
) -> Result<(), String> {
    if crate::frame::FrameWorld::from_world(world).script_sound_is_looping(alias)? {
        return Err("cannot play a looping alias as a one-shot sound".into());
    }
    let index = crate::frame::FrameWorld::from_world(world).sound_alias_index(alias);
    if index == 0 {
        return Err("sound alias configstring table is full".into());
    }
    world_event(
        world,
        entity_iw4::EntityEventKind::SOUND_ALIAS,
        index,
        origin,
        ZERO,
    );
    Ok(())
}

fn radius_damage(
    world: &mut World,
    inflictor: Option<crate::ScriptModelId>,
    args: &[Value],
) -> Result<Value, String> {
    let origin = vector(args, 0)?;
    let radius = float(args, 1)?;
    let max = float(args, 2)?;
    let min = float(args, 3)?;
    let attacker = match args.get(4) {
        Some(Value::Object(id)) => runtime(world).player_client(*id).map(crate::ClientId),
        _ => None,
    };
    let means = optional(args, 5, string)?
        .map(|name| super::super::entity_damage::means_named(&name))
        .transpose()?
        .unwrap_or("MOD_EXPLOSIVE");
    let weapon = match optional(args, 6, string)? {
        Some(name) if name != "none" => {
            crate::script_player::weapon_named(&crate::frame::FrameWorld::from_world(world), &name)?
        }
        _ => 0,
    };
    runtime(world)
        .blasts
        .push(super::super::entity_damage::ScriptBlast {
            origin,
            radius,
            max,
            min,
            attacker,
            inflictor,
            means,
            weapon,
        });
    Ok(Value::Undefined)
}

fn part(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    hidden: bool,
) -> Result<Value, String> {
    let tag: Arc<str> = string(args, 0)?.into();
    with_entity(world, receiver, |e| e.part_ops.push((tag, hidden)))
}

fn team_key(args: &[Value]) -> Result<String, String> {
    let team = string(args, 0)?;
    match team.as_str() {
        "allies" | "axis" | "free" | "spectator" | "none" | "neutral" => Ok(team),
        other => Err(format!("'{other}' is an illegal team string")),
    }
}

fn bind_match_data(
    world: &mut World,
    scope: super::super::match_data::Scope,
    args: &[Value],
) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("match data definition takes one schema name".into());
    }
    let name = string(args, 0)?.replace('\\', "/").to_ascii_lowercase();
    let store = world
        .get_resource::<crate::PersistentDataStore>()
        .ok_or("match data schema store is not installed")?;
    let schema = store
        .schemas()
        .get(&name)
        .cloned()
        .ok_or_else(|| format!("match data schema '{name}' is not installed"))?;
    runtime(world)
        .engine
        .match_data
        .bind(scope, Arc::clone(&schema))?;
    let definition = &schema.definitions[0];
    diag::info!(
        Sim,
        "match data schema bound: name={name} version={} checksum={} bytes={}",
        definition.version,
        definition.checksum,
        definition.size
    );
    Ok(Value::Undefined)
}

fn weapon_facts(
    world: &mut World,
    args: &[Value],
) -> Result<weapon_iw4::WeaponCombatFacts, String> {
    let name = string(args, 0)?;
    if name == "none" || name.is_empty() {
        return Ok(Default::default());
    }
    let frame = crate::frame::FrameWorld::from_world(world);
    let index = frame
        .weapon_index_by_script_name(&name)
        .ok_or_else(|| format!("unknown weapon '{name}'"))?;
    Ok(frame.weapon_combat_row(index).unwrap_or_default())
}

const WEAPON_TYPES: &[&str] = &["bullet", "grenade", "projectile", "riotshield"];
const WEAPON_CLASSES: &[&str] = &[
    "rifle",
    "sniper",
    "mg",
    "smg",
    "spread",
    "pistol",
    "grenade",
    "rocketlauncher",
    "turret",
    "throwingknife",
    "non-player",
    "item",
];
const INVENTORY_TYPES: &[&str] = &[
    "primary",
    "offhand",
    "item",
    "altmode",
    "exclusive",
    "scavenger",
];

fn enum_name(names: &[&str], index: i32) -> Value {
    Value::string(
        names
            .get(index.max(0) as usize)
            .copied()
            .unwrap_or(names[0]),
    )
}

fn signal(world: &mut World, name: &str) -> Result<Value, String> {
    runtime(world).signals.push(name.into());
    Ok(Value::Undefined)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    register_entities(registry);
    register_placement(registry);
    register_appearance(registry);
    register_motion(registry);
    register_attachments(registry);
    register_sound_and_fx(registry);
    register_entity_state(registry);
    super::super::hud::register(registry);
    register_traces(registry);
    register_match(registry);
    register_level(registry);
    register_weapon_facts(registry);
    register_damage(registry);
    register_refused(registry);
}

fn register_entities(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.register(Function, "getent", |world, _, args| {
        single(matching_entities(world, args)?, "getent")
    });
    registry.register(Function, "getentarray", |world, _, args| {
        let ids = if args.is_empty() {
            let runtime = world.resource::<Runtime>();
            runtime
                .entities
                .iter()
                .filter(|(_, e)| e.kind != EntityKind::HudElem)
                .map(|(id, _)| *id)
                .collect()
        } else {
            matching_entities(world, args)?
        };
        objects(world, ids)
    });
    registry.register(Function, "getvehiclenode", |world, _, args| {
        let ids = matching_entities(world, args)?;
        single(
            classname_prefix(world, ids, "info_vehicle_node"),
            "getvehiclenode",
        )
    });
    registry.register(Function, "getvehiclenodearray", |world, _, args| {
        let ids = matching_entities(world, args)?;
        let ids = classname_prefix(world, ids, "info_vehicle_node");
        objects(world, ids)
    });
    registry.register(Function, "isspawner", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "getteamplayersalive", |world, _, args| {
        let team = team_key(args)?;
        Ok(Value::Int(super::super::players::alive_on_team(
            world, &team,
        )))
    });
    registry.register(Function, "isplayer", |world, _, args| {
        let value = arg(args, 0)?;
        let player = matches!(value, Value::Object(id)
            if world.resource::<Runtime>().player_client(*id).is_some());
        Ok(Value::Int(player.into()))
    });
    registry.register(Function, "isalive", |world, _, args| {
        let value = arg(args, 0)?.clone();
        let Ok(id) = entity_id(world, &value) else {
            return Ok(Value::Int(0));
        };
        let health = match super::super::players::entity_field(world, id, "health") {
            Value::Int(n) => n as f32,
            Value::Float(n) => n,
            _ => 0.0,
        };
        Ok(Value::Int((health > 0.0).into()))
    });
    registry.register(Method, "piecestage", |world, receiver, args| {
        let piece = int(args, 0)?;
        let stage = int(args, 1)?;
        let target = runtime(world)
            .presence_of(receiver)
            .ok_or("piecestage requires a model")?;
        Ok(Value::Int(i32::from(
            piece >= 0
                && stage >= 0
                && crate::t5_destructible::stage_matches(
                    &crate::frame::FrameWorld::from_world(world),
                    crate::AuthorityModelOwner::ScriptModel(target),
                    piece as usize,
                    stage as usize,
                ),
        )))
    });
    registry.register(Method, "damagepiece", |world, receiver, args| {
        let amount = int(args, 0)?;
        let piece = int(args, 1)?;
        let target = runtime(world)
            .presence_of(receiver)
            .ok_or("damagepiece requires a model")?;
        let attacker = args
            .get(2)
            .and_then(|v| runtime(world).player_client_of(v))
            .map(crate::ClientId);
        let origin = origin_of(world, receiver)?;
        runtime(world)
            .hits
            .push(super::super::entity_damage::ScriptHit {
                piece: Some(piece),
                target: super::super::entity_damage::HitTarget::Entity(target),
                amount,
                origin,
                attacker,
                inflictor: Some(target),
                means: "MOD_EXPLOSIVE",
                weapon: 0,
                flags: 1,
                hitloc: 0,
            });
        Ok(Value::Undefined)
    });
    registry.register(Function, "spawn", |world, _, args| {
        let classname = string(args, 0)?;
        let origin = vector(args, 1)?;
        let flags = optional(args, 2, int)?.unwrap_or(0);
        if !matches!(
            classname.as_str(),
            "script_origin"
                | "script_model"
                | "trigger_radius"
                | "info_notnull"
                | "info_notnull_big"
        ) {
            return Err(format!("unable to spawn \"{classname}\" entity"));
        }
        let cylinder = if classname == "trigger_radius" {
            Some((float(args, 3)?, float(args, 4)?))
        } else {
            None
        };
        let presence = if matches!(classname.as_str(), "script_model" | "script_origin") {
            Some(super::super::presence::spawn_presence(world, origin)?)
        } else {
            None
        };
        let number =
            presence.and_then(|id| crate::frame::FrameWorld::from_world(world).gentity_number(id));
        let mut runtime = runtime(world);
        let id = runtime.create_entity(EntityKind::Spawned, &classname)?;
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        runtime.set_object_field(id, "angles", Value::Vector(ZERO));
        runtime.set_object_field(id, "spawnflags", Value::Int(flags));
        let entity = runtime.entities.get_mut(&id).unwrap();
        entity.cylinder = cylinder;
        entity.presence = presence;
        if let Some(number) = number {
            entity.number = number;
        }
        Ok(Value::Object(id))
    });
    registry.register(Function, "sortbydistance", |world, _, args| {
        let values = array_values(world, arg(args, 0)?)?;
        let origin = vector(args, 1)?;
        let mut keyed = Vec::with_capacity(values.len());
        for value in values {
            keyed.push((distance_sq(origin_of(world, &value)?, origin), value));
        }
        keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
        new_array(world, keyed.into_iter().map(|(_, v)| v).collect())
    });

    registry.register(Method, "delete", |world, receiver, args| {
        if !args.is_empty() {
            return Err("delete expects no arguments".into());
        }
        let id = entity_id(world, receiver)?;
        if runtime(world).player_client(id).is_some() {
            return Err("cannot delete a client entity".into());
        }
        if runtime(world).pending_deletes.contains(&id) {
            return Ok(Value::Undefined);
        }
        let item = match runtime(world).entities.get(&id).map(|e| &e.kind) {
            Some(super::super::entities::EntityKind::Item(number)) => Some(*number),
            _ => None,
        };
        if let Some(number) = item {
            crate::frame::FrameWorld::from_world(world).remove_dropped_item_by_number(number);
        }
        // Script code can still read fields after delete() in the same frame
        // (for example UAV bookkeeping). Retire at the scheduler's frame boundary.
        runtime(world).pending_deletes.push(id);
        crate::script::runtime::raise(world, Value::Object(id), "death", Vec::new());
        Ok(Value::Undefined)
    });
}

fn register_placement(registry: &mut NativeRegistry) {
    use Namespace::Method;

    registry.register(Method, "setorigin", |world, receiver, args| {
        let origin = vector(args, 0)?;
        let id = entity_id(world, receiver)?;
        if let Some(client) = runtime(world).player_client(id) {
            crate::frame::FrameWorld::from_world(world).set_origin(crate::ClientId(client), origin);
            return Ok(Value::Undefined);
        }
        world.resource_mut::<Mechanics>().stop(id, "origin");
        runtime(world).set_object_field(id, "origin", Value::Vector(origin));
        Ok(Value::Undefined)
    });
    registry.register(Method, "getorigin", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        Ok(Value::Vector(vector_field(world, id, "origin")))
    });
    registry.register(Method, "gettagorigin", |world, receiver, args| {
        let id = entity_id(world, receiver)?;
        let tag = string(args, 0)?;
        let origin = vector_field(world, id, "origin");
        let offset = super::super::presence::tag_offset(world, id, &tag).unwrap_or(ZERO);
        let axis = math_iw4::angles_to_axis(vector_field(world, id, "angles"));
        Ok(Value::Vector(std::array::from_fn(|i| {
            origin[i] + axis[0][i] * offset[0] + axis[1][i] * offset[1] + axis[2][i] * offset[2]
        })))
    });
    registry.register(Method, "geteye", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        if let Some(client) = runtime(world).player_client(id)
            && let Some(view) = crate::frame::FrameWorld::from_world(world)
                .client_meta(crate::ClientId(client))
                .and_then(|meta| meta.linked_weapon_view)
        {
            return Ok(Value::Vector(view.origin));
        }
        let origin = vector_field(world, id, "origin");
        let height = match runtime(world).player_client(id) {
            Some(client) => crate::frame::FrameWorld::from_world(world)
                .player(crate::ClientId(client))
                .map_or(0.0, |ps| ps.view_height_current),
            None => 0.0,
        };
        Ok(Value::Vector(add(origin, [0.0, 0.0, height])))
    });
    registry.register(Method, "getpointinbounds", |world, receiver, args| {
        let id = entity_id(world, receiver)?;
        let origin = vector_field(world, id, "origin");
        let scale = [float(args, 0)?, float(args, 1)?, float(args, 2)?];
        let (mins, maxs) = super::super::triggers::entity_bounds(world, id);
        Ok(Value::Vector(std::array::from_fn(|i| {
            origin[i] + (maxs[i] + mins[i]) * 0.5 + (maxs[i] - mins[i]) * 0.5 * scale[i]
        })))
    });
    registry.register(Method, "gettagangles", |world, receiver, args| {
        let id = entity_id(world, receiver)?;
        let tag = string(args, 0)?;
        match super::super::presence::tag_world(world, id, &tag) {
            Some((_, axis)) => Ok(Value::Vector(math_iw4::axis_to_angles(axis))),
            None if tag.eq_ignore_ascii_case("tag_origin") => {
                Ok(Value::Vector(vector_field(world, id, "angles")))
            }
            None => Err(format!("tag '{tag}' does not exist on entity")),
        }
    });
    registry.register(Method, "getvelocity", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        if let Some(client) = runtime(world).player_client(id) {
            return Ok(Value::Vector(
                crate::frame::FrameWorld::from_world(world)
                    .player(crate::ClientId(client))
                    .map_or(ZERO, |ps| ps.velocity),
            ));
        }
        let now = now_ms(world);
        Ok(Value::Vector(
            world.resource::<Mechanics>().velocity(id, now),
        ))
    });
    registry.register(Method, "getentitynumber", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        Ok(Value::Int(world.resource::<Runtime>().entities[&id].number))
    });
}

fn register_appearance(registry: &mut NativeRegistry) {
    use Namespace::Method;

    registry.register(Method, "setmodel", |world, receiver, args| {
        let model = string(args, 0)?;
        let id = entity_id(world, receiver)?;
        runtime(world).set_object_field(id, "model", Value::string(&model));
        crate::script::host::players::dress_insertion_glow(world, id, &model);
        Ok(Value::Undefined)
    });
    registry.register(
        Method,
        "clonebrushmodeltoscriptmodel",
        |world, receiver, args| {
            let id = entity_id(world, receiver)?;
            let source = entity_id(world, arg(args, 0)?)?;
            let mut runtime = runtime(world);
            let Some(brush) = runtime.entities[&source].brush else {
                return Ok(Value::Undefined);
            };
            let origin = match runtime.object_field(id, "origin") {
                Value::Vector(v) => v,
                _ => ZERO,
            };
            let angles = match runtime.object_field(id, "angles") {
                Value::Vector(v) => v,
                _ => ZERO,
            };
            let entity = runtime.entities.get_mut(&id).unwrap();
            entity.brush = Some(brush);
            let presence = entity.presence;
            drop(runtime);
            if let Some(presence) = presence
                && let Some(row) =
                    crate::frame::FrameWorld::from_world(world).collision_owner_mut(presence)
            {
                row.linked_brushes = vec![crate::bullet_collision::LinkedBrushCollisionBrush {
                    cmodel_handle: brush,
                    origin,
                    angles,
                }];
            }
            Ok(Value::Undefined)
        },
    );
    registry.register(Method, "hide", |world, receiver, _| {
        with_entity(world, receiver, |e| {
            e.hidden = true;
            e.shown_to = 0;
        })
    });
    registry.register(Method, "show", |world, receiver, _| {
        with_entity(world, receiver, |e| {
            e.hidden = false;
            e.shown_to = 0;
        })
    });
    registry.register(Method, "showtoplayer", |world, receiver, args| {
        let player = args.first().ok_or("showToPlayer: missing player")?;
        let client = runtime(world)
            .player_client_of(player)
            .filter(|client| *client < 64)
            .ok_or("showToPlayer: parameter 1 is not a player")?;
        with_entity(world, receiver, |e| e.shown_to |= 1 << client)
    });
    registry.register(Method, "solid", |world, receiver, _| {
        with_entity(world, receiver, |e| e.solid = true)
    });
    registry.register(Method, "notsolid", |world, receiver, _| {
        with_entity(world, receiver, |e| e.solid = false)
    });
    registry.register(Method, "setcontents", |world, receiver, args| {
        let contents = int(args, 0)?;
        let mut previous = 0;
        with_entity(world, receiver, |e| {
            previous = match e.contents {
                0 if e.solid => crate::bullet_collision::CONTENTS_SOLID as i32,
                c => c,
            };
            e.contents = contents;
            e.solid = contents != 0;
        })?;
        Ok(Value::Int(previous))
    });
    registry.register(Method, "setlightintensity", |world, receiver, args| {
        let value = float(args, 0)?;
        with_entity(world, receiver, |e| e.light = value)
    });
    registry.register(Method, "getlightintensity", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        Ok(Value::Float(
            world.resource::<Runtime>().entities[&id].light,
        ))
    });
}

fn slide_object(world: &mut World, receiver: &Value) -> Result<u64, String> {
    let object = entity_id(world, receiver)?;
    let entity = &world.resource::<Runtime>().entities[&object];
    if !matches!(
        &*entity.classname,
        "script_model" | "script_brushmodel" | "script_origin" | "light"
    ) {
        return Err(
            "slide movement requires a script_model, script_brushmodel, script_origin or light"
                .into(),
        );
    }
    Ok(object)
}

fn register_motion(registry: &mut NativeRegistry) {
    use Namespace::Method;

    registry.register(Method, "linkto", |world, receiver, args| {
        if let Some(client) = runtime(world).player_client_of(receiver) {
            super::player::link_to(
                world,
                receiver,
                &args[..args.len().min(2)],
                super::super::players::LinkView::Free,
            )?;
            if let Some(origin) = optional(args, 2, vector)? {
                let angles = optional(args, 3, vector)?.unwrap_or(ZERO);
                if let Some(link) = runtime(world)
                    .players
                    .get_mut(&client)
                    .and_then(|slot| slot.link.as_mut())
                {
                    link.origin = origin;
                    link.angles = angles;
                }
            }
            return Ok(Value::Undefined);
        }
        let parent = entity_id(world, arg(args, 0)?)?;
        let id = entity_id(world, receiver)?;
        if parent == id {
            return Err("cannot link an entity to itself".into());
        }
        let tag = optional(args, 1, string)?
            .filter(|tag| !tag.is_empty() && !tag.eq_ignore_ascii_case("tag_origin"))
            .map(Arc::<str>::from);
        let (origin, angles) = match optional(args, 2, vector)? {
            Some(offset) => (offset, optional(args, 3, vector)?.unwrap_or(ZERO)),
            None => {
                let parent_axis = math_iw4::angles_to_axis(vector_field(world, parent, "angles"));
                let delta = sub(
                    vector_field(world, id, "origin"),
                    vector_field(world, parent, "origin"),
                );
                let child_axis = math_iw4::angles_to_axis(vector_field(world, id, "angles"));
                (
                    std::array::from_fn(|i| dot(delta, parent_axis[i])),
                    math_iw4::axis_to_angles(math_iw4::matrix_multiply(
                        child_axis,
                        transpose(parent_axis),
                    )),
                )
            }
        };
        let tag_offset = tag.is_none().then_some(ZERO);
        runtime(world).entities.get_mut(&id).unwrap().linked_to =
            Some(super::super::entities::Link {
                parent,
                origin,
                angles,
                tag,
                tag_offset,
            });
        Ok(Value::Undefined)
    });
    registry.register(Method, "unlink", |world, receiver, _| {
        if let Some(client) = runtime(world).player_client_of(receiver) {
            super::super::players::unlink_player(world, client);
        }
        with_entity(world, receiver, |e| e.linked_to = None)
    });
    registry.register(Method, "moveslide", |world, receiver, args| {
        if args.len() != 3 {
            return Err("MoveSlide expects center offset, radius and velocity".into());
        }
        let object = slide_object(world, receiver)?;
        let center = vector(args, 0)?;
        let radius = float(args, 1)?;
        let velocity = vector(args, 2)?;
        let origin = vector_field(world, object, "origin");
        if !radius.is_finite()
            || radius < 0.0
            || center
                .iter()
                .chain(velocity.iter())
                .chain(origin.iter())
                .any(|v| !v.is_finite())
        {
            return Err(
                "slide pose, bounds and velocity must be finite with a nonnegative radius".into(),
            );
        }
        if center
            .iter()
            .any(|v| !(v - radius).is_finite() || !(v + radius).is_finite())
        {
            return Err("slide bounds must be finite".into());
        }
        world.resource_mut::<Mechanics>().slide(
            object,
            super::super::mechanics::Slide {
                center,
                radius,
                velocity,
            },
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "stopmoveslide", |world, receiver, args| {
        if !args.is_empty() {
            return Err("StopMoveSlide expects no arguments".into());
        }
        let object = slide_object(world, receiver)?;
        world.resource_mut::<Mechanics>().stop_slide(object);
        Ok(Value::Undefined)
    });
    registry.register(Method, "movex", |world, receiver, args| {
        move_axis(world, receiver, args, 0)
    });
    registry.register(Method, "movey", |world, receiver, args| {
        move_axis(world, receiver, args, 1)
    });
    registry.register(Method, "movez", |world, receiver, args| {
        move_axis(world, receiver, args, 2)
    });
    registry.register(Method, "islinked", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        let runtime = world.resource::<Runtime>();
        let parent = if let Some(client) = runtime.player_client_of(receiver) {
            runtime
                .players
                .get(&client)
                .and_then(|slot| slot.link.as_ref())
                .map(|link| link.parent)
        } else {
            runtime.entities[&id]
                .linked_to
                .as_ref()
                .map(|link| link.parent)
        };
        Ok(Value::Int(i32::from(
            parent.is_some_and(|parent| runtime.live(&parent)),
        )))
    });
    registry.register(Method, "localtoworldcoords", |world, receiver, args| {
        if args.len() != 1 {
            return Err("LocalToWorldCoords expects a local vector".into());
        }
        let local = vector(args, 0)?;
        let id = entity_id(world, receiver)?;
        let origin = vector_field(world, id, "origin");
        let angles = vector_field(world, id, "angles");
        if local
            .iter()
            .chain(origin.iter())
            .chain(angles.iter())
            .any(|v| !v.is_finite())
        {
            return Err("transform pose and vector must be finite".into());
        }
        let axis = math_iw4::angles_to_axis(angles);
        let point = std::array::from_fn(|i| {
            origin[i] + local[0] * axis[0][i] + local[1] * axis[1][i] + local[2] * axis[2][i]
        });
        if point.iter().any(|v: &f32| !v.is_finite()) {
            return Err("transformed point must be finite".into());
        }
        Ok(Value::Vector(point))
    });
    registry.register(Method, "moveto", |world, receiver, args| {
        let to = vector(args, 0)?;
        let time = float(args, 1)?;
        let duration = seconds_ms(time)?;
        let id = entity_id(world, receiver)?;
        let from = vector_field(world, id, "origin");
        start_motion(
            world,
            receiver,
            "origin",
            ramp(args, time, from, to)?,
            duration,
            "movedone",
        )
    });
    registry.register(Method, "movegravity", |world, receiver, args| {
        let velocity = vector(args, 0)?;
        let duration = seconds_ms(float(args, 1)?)?;
        let id = entity_id(world, receiver)?;
        let from = vector_field(world, id, "origin");
        start_motion(
            world,
            receiver,
            "origin",
            MotionPath::Ballistic { from, velocity },
            duration,
            "movedone",
        )
    });
    registry.register(Method, "rotateto", |world, receiver, args| {
        let to = vector(args, 0)?;
        let time = float(args, 1)?;
        let duration = seconds_ms(time)?;
        let id = entity_id(world, receiver)?;
        let from = vector_field(world, id, "angles");
        let to = std::array::from_fn(|i| {
            let delta = (to[i] - from[i]) % 360.0;
            let delta = if delta > 180.0 {
                delta - 360.0
            } else if delta < -180.0 {
                delta + 360.0
            } else {
                delta
            };
            from[i] + delta
        });
        start_motion(
            world,
            receiver,
            "angles",
            ramp(args, time, from, to)?,
            duration,
            "rotatedone",
        )
    });
    registry.register(Method, "rotatepitch", |world, receiver, args| {
        rotate_by(world, receiver, args, 0)
    });
    registry.register(Method, "rotateyaw", |world, receiver, args| {
        rotate_by(world, receiver, args, 1)
    });
    registry.register(Method, "rotateroll", |world, receiver, args| {
        rotate_by(world, receiver, args, 2)
    });
    registry.register(Method, "rotatevelocity", |world, receiver, args| {
        let velocity = vector(args, 0)?;
        let time = float(args, 1)?;
        let duration = seconds_ms(time)?;
        let id = entity_id(world, receiver)?;
        let from = vector_field(world, id, "angles");
        let to = add(from, scale(velocity, time));
        start_motion(
            world,
            receiver,
            "angles",
            ramp(args, time, from, to)?,
            duration,
            "rotatedone",
        )
    });
    registry.register(Method, "addpitch", |world, receiver, args| {
        add_angle(world, receiver, args, 0)
    });
    registry.register(Method, "addyaw", |world, receiver, args| {
        add_angle(world, receiver, args, 1)
    });
    registry.register(Method, "addroll", |world, receiver, args| {
        add_angle(world, receiver, args, 2)
    });
    registry.register(Method, "istouching", |world, receiver, args| {
        let id = entity_id(world, receiver)?;
        let other = arg(args, 0)?.clone();
        let Ok(other) = entity_id(world, &other) else {
            return Ok(Value::Int(0));
        };
        Ok(Value::Int(
            super::super::triggers::is_touching(world, id, other).into(),
        ))
    });
    registry.register(Method, "placespawnpoint", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        let start = vector_field(world, id, "origin");
        let up = add(start, [0.0, 0.0, 128.0]);
        let t = trace(
            world,
            start,
            up,
            PLAYER_MINS,
            PLAYER_MAXS,
            MASK_PLAYER_SOLID,
        );
        let top = lerp(start, up, t.fraction);
        let down = add(top, [0.0, 0.0, -262_144.0]);
        let t = trace(
            world,
            top,
            down,
            PLAYER_MINS,
            PLAYER_MAXS,
            MASK_PLAYER_SOLID,
        );
        let ground = lerp(top, down, t.fraction);
        runtime(world).set_object_field(id, "origin", Value::Vector(ground));
        Ok(Value::Undefined)
    });
}

fn register_attachments(registry: &mut NativeRegistry) {
    use Namespace::Method;

    registry.register(Method, "attach", |world, receiver, args| {
        let model: Arc<str> = string(args, 0)?.into();
        let tag: Arc<str> = optional(args, 1, string)?.unwrap_or_default().into();
        let id = entity_id(world, receiver)?;
        let mut runtime = runtime(world);
        let entity = runtime.entities.get_mut(&id).unwrap();
        if entity
            .attachments
            .iter()
            .any(|(m, t)| *m == model && *t == tag)
        {
            return Err(format!("model '{model}' already attached to tag '{tag}'"));
        }
        entity.attachments.push((model, tag));
        Ok(Value::Undefined)
    });
    registry.register(Method, "detach", |world, receiver, args| {
        let model: Arc<str> = string(args, 0)?.into();
        let tag: Option<Arc<str>> = optional(args, 1, string)?.map(Into::into);
        let id = entity_id(world, receiver)?;
        let mut runtime = runtime(world);
        let entity = runtime.entities.get_mut(&id).unwrap();
        let index = entity
            .attachments
            .iter()
            .position(|(m, t)| *m == model && tag.as_ref().is_none_or(|tag| t == tag))
            .ok_or_else(|| format!("model '{model}' is not attached"))?;
        entity.attachments.remove(index);
        Ok(Value::Undefined)
    });
    registry.register(Method, "detachall", |world, receiver, _| {
        with_entity(world, receiver, |e| e.attachments.clear())
    });
    registry.register(Method, "getattachsize", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        Ok(Value::Int(
            world.resource::<Runtime>().entities[&id].attachments.len() as i32,
        ))
    });
    registry.register(Method, "getattachmodelname", |world, receiver, args| {
        let index = int(args, 0)?;
        let id = entity_id(world, receiver)?;
        let entity = &world.resource::<Runtime>().entities[&id];
        let (model, _) = entity
            .attachments
            .get(index.max(0) as usize)
            .ok_or("bad attachment index")?;
        Ok(Value::String(model.clone().into()))
    });
    registry.register(Method, "getattachtagname", |world, receiver, args| {
        let index = int(args, 0)?;
        let id = entity_id(world, receiver)?;
        let entity = &world.resource::<Runtime>().entities[&id];
        let (_, tag) = entity
            .attachments
            .get(index.max(0) as usize)
            .ok_or("bad attachment index")?;
        Ok(Value::String(tag.clone().into()))
    });
}

fn register_sound_and_fx(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.register(Function, "playfx", |world, _, args| {
        let name = name(world, int(args, 0)?)?;
        let origin = vector(args, 1)?;
        let forward = optional(args, 2, vector)?.unwrap_or([0.0, 0.0, 1.0]);
        let index = crate::frame::FrameWorld::from_world(world).effect_name_index(&name);
        world_event(
            world,
            entity_iw4::EntityEventKind::PLAY_FX,
            index,
            origin,
            forward,
        );
        Ok(Value::Undefined)
    });
    registry.register(Function, "playfxontag", |world, _, args| {
        fx_on_tag(world, args, entity_iw4::EntityEventKind::PLAY_FX_ON_TAG)
    });
    registry.register(Function, "stopfxontag", |world, _, args| {
        fx_on_tag(world, args, entity_iw4::EntityEventKind::STOP_FX_ON_TAG)
    });
    registry.register(Function, "playsoundatpos", |world, _, args| {
        let origin = vector(args, 0)?;
        let alias = string(args, 1)?;
        play_sound_at(world, origin, &alias)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "playloopsound", |world, receiver, args| {
        let alias: Arc<str> = string(args, 0)?.into();
        with_entity(world, receiver, |e| e.loop_sound = Some(alias))
    });
    registry.register(Method, "stoploopsound", |world, receiver, _| {
        with_entity(world, receiver, |e| e.loop_sound = None)
    });
    for name in ["playsound", "playsoundasmaster"] {
        registry.register(Method, name, |world, receiver, args| {
            entity_id(world, receiver)?;
            let alias = string(args, 0)?;
            let origin = origin_of(world, receiver)?;
            play_sound_at(world, origin, &alias)?;
            if args.len() != 1 {
                return Err("expected one sound alias argument".into());
            }
            Ok(Value::Undefined)
        });
    }
    registry.register(Method, "playsoundtoplayer", |world, receiver, args| {
        let alias = string(args, 0)?;
        let client = super::player::player(world, arg(args, 1)?)?;
        let origin = origin_of(world, receiver)?;
        let number = i32::from(trace_iw4::ENTITYNUM_WORLD);
        sound_to(
            world,
            crate::EventAudience::Client(crate::ClientId(client)),
            &alias,
            number,
            origin,
        );
        Ok(Value::Undefined)
    });
    registry.register(Method, "playsoundtoteam", |world, receiver, args| {
        let alias = string(args, 0)?;
        let team = string(args, 1)?;
        let except = match args.get(2) {
            Some(value) if !matches!(value, Value::Undefined) => {
                Some(super::player::player(world, value)?)
            }
            _ => None,
        };
        let origin = origin_of(world, receiver)?;
        let number = i32::from(trace_iw4::ENTITYNUM_WORLD);
        let clients = team_clients(world, &team, except);
        if !clients.is_empty() {
            sound_to(
                world,
                crate::EventAudience::Clients(clients),
                &alias,
                number,
                origin,
            );
        }
        Ok(Value::Undefined)
    });
}

fn register_entity_state(registry: &mut NativeRegistry) {
    use Namespace::Method;

    macro_rules! entity_accepts {
        ($($name:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, _| {
                entity_id(world, receiver)?;
                Ok(Value::Undefined)
            });
        )*};
    }
    registry.register(Method, "setcandamage", |world, receiver, args| {
        let on = int(args, 0)? != 0;
        with_entity(world, receiver, |e| e.can_damage = on)
    });
    registry.register(Method, "setcanradiusdamage", |world, receiver, args| {
        let on = int(args, 0)? != 0;
        with_entity(world, receiver, |e| e.can_radius_damage = on)
    });
    registry.register(Method, "hidepart", |world, receiver, args| {
        part(world, receiver, args, true)
    });
    registry.register(Method, "showpart", |world, receiver, args| {
        part(world, receiver, args, false)
    });
    registry.register(Method, "scriptmodelplayanim", |world, receiver, args| {
        let clip = match arg(args, 0)? {
            Value::Animation { name, .. } => name.clone(),
            Value::String(name) => name.clone().into(),
            other => return Err(format!("{} is not an animation", kind(other))),
        };
        with_entity(world, receiver, |e| e.anim_op = Some(Some(clip)))
    });
    registry.register(Method, "scriptmodelclearanim", |world, receiver, _| {
        with_entity(world, receiver, |e| e.anim_op = Some(None))
    });
    entity_accepts!["willneverchange", "laseron", "laseroff", "logstring",];
}

fn register_traces(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.register(Function, "bullettrace", |world, _, args| {
        let (start, end) = (vector(args, 0)?, vector(args, 1)?);
        let mask = shot_mask(args)?;
        let ignore = trace_ignore(world, args.get(3));
        let outcome = entity_trace(world, start, end, mask, ignore);
        let (fraction, normal, collider) = match outcome {
            TraceOutcome::Hit {
                fraction,
                normal,
                collider,
                ..
            } => (fraction, normal, Some(collider)),
            TraceOutcome::StartSolid { collider, .. } => (0.0, ZERO, collider),
            _ => (1.0, ZERO, None),
        };
        let (normal, surface) = match collider {
            Some(collider) if fraction < 1.0 => (normal, surface_name(collider)),
            _ => {
                let d = sub(end, start);
                let len = dot(d, d).sqrt();
                (if len > 0.0 { scale(d, 1.0 / len) } else { ZERO }, "none")
            }
        };
        let entity = collider.map_or(Value::Undefined, |c| collider_entity(world, c));
        keyed_array(
            world,
            vec![
                ("fraction", Value::Float(fraction)),
                ("position", Value::Vector(lerp(start, end, fraction))),
                ("entity", entity),
                ("normal", Value::Vector(normal)),
                ("surfacetype", Value::string(surface)),
            ],
        )
    });
    registry.register(Function, "bullettracepassed", |world, _, args| {
        let (start, end) = (vector(args, 0)?, vector(args, 1)?);
        let mask = shot_mask(args)?;
        let ignore = trace_ignore(world, args.get(3));
        let passed = matches!(
            entity_trace(world, start, end, mask, ignore),
            TraceOutcome::Miss { .. }
        );
        Ok(Value::Int(passed.into()))
    });
    registry.register(Function, "physicstrace", |world, _, args| {
        let (start, end) = (vector(args, 0)?, vector(args, 1)?);
        let t = trace(world, start, end, ZERO, ZERO, MASK_PLAYER_SOLID);
        Ok(Value::Vector(lerp(start, end, t.fraction)))
    });
    registry.register(Function, "playerphysicstrace", |world, _, args| {
        let (start, end) = (vector(args, 0)?, vector(args, 1)?);
        let t = trace(
            world,
            start,
            end,
            PLAYER_MINS,
            PLAYER_MAXS,
            MASK_PLAYER_SOLID,
        );
        Ok(Value::Vector(lerp(start, end, t.fraction)))
    });
    registry.register(Function, "spawnsighttrace", |world, _, args| {
        let (start, end) = (vector(args, 1)?, vector(args, 2)?);
        Ok(Value::Int(
            trace_passed(world, start, end, MASK_SHOT).into(),
        ))
    });
    registry.register(Function, "canspawn", |world, _, args| {
        let origin = vector(args, 0)?;
        let t = trace(
            world,
            origin,
            origin,
            PLAYER_MINS,
            PLAYER_MAXS,
            MASK_PLAYER_SOLID,
        );
        Ok(Value::Int((t.startsolid == 0 && t.allsolid == 0).into()))
    });
    registry.register(Function, "positionwouldtelefrag", |world, _, args| {
        let origin = vector(args, 0)?;
        let frame = crate::frame::FrameWorld::from_world(world);
        let blocked = frame.client_ids_sorted().into_iter().any(|id| {
            frame.player(id).is_some_and(|ps| {
                ps.pm_type < playerstate_iw4::PM_TYPE_SPECTATOR
                    && frame
                        .client_meta(id)
                        .is_some_and(|m| m.lifecycle == crate::ClientLifecycle::Alive)
                    && (0..3).all(|i| {
                        origin[i] + PLAYER_MINS[i] <= ps.origin[i] + PLAYER_MAXS[i]
                            && origin[i] + PLAYER_MAXS[i] >= ps.origin[i] + PLAYER_MINS[i]
                    })
            })
        });
        Ok(Value::Int(blocked.into()))
    });
    registry.register(Method, "sightconetrace", |world, receiver, args| {
        cone_trace(world, receiver, args, SIGHT_CONE_MASK)
    });
    registry.register(Method, "damageconetrace", |world, receiver, args| {
        cone_trace(world, receiver, args, DAMAGE_CONE_MASK)
    });
}

fn register_match(registry: &mut NativeRegistry) {
    use Namespace::Function;

    registry.register(Function, "setteamscore", |world, _, args| {
        let team = team_key(args)?;
        let score = int(args, 1)?;
        runtime(world).engine.team_scores.insert(team, score);
        Ok(Value::Undefined)
    });
    registry.register(Function, "getteamscore", |world, _, args| {
        let team = team_key(args)?;
        Ok(Value::Int(
            runtime(world)
                .engine
                .team_scores
                .get(&team)
                .copied()
                .unwrap_or(0),
        ))
    });
    registry.register(Function, "setteamradar", |world, _, args| {
        let team = team_key(args)?;
        let on = int(args, 1)?;
        runtime(world).engine.team_radar.insert(team, on);
        Ok(Value::Undefined)
    });
    registry.register(Function, "getteamradar", |world, _, args| {
        let team = team_key(args)?;
        Ok(Value::Int(
            runtime(world)
                .engine
                .team_radar
                .get(&team)
                .copied()
                .unwrap_or(0),
        ))
    });
    registry.register(Function, "blockteamradar", |world, _, args| {
        let team = team_key(args)?;
        runtime(world).engine.team_radar_blocked.insert(team);
        Ok(Value::Undefined)
    });
    registry.register(Function, "unblockteamradar", |world, _, args| {
        let team = team_key(args)?;
        runtime(world).engine.team_radar_blocked.remove(&team);
        Ok(Value::Undefined)
    });
    registry.register(Function, "setgameendtime", |world, _, args| {
        let time = int(args, 0)?;
        runtime(world).engine.game_end_time = time;
        Ok(Value::Undefined)
    });
    registry.register(Function, "setmapcenter", |world, _, args| {
        let center = vector(args, 0)?;
        runtime(world).engine.map_center = center;
        Ok(Value::Undefined)
    });
    registry.register(Function, "setwinningteam", |world, _, args| {
        let team = team_key(args)?;
        runtime(world).engine.winning_team = Some(team);
        Ok(Value::Undefined)
    });
    registry.register(Function, "getnorthyaw", |world, _, _| {
        let yaw = runtime(world)
            .engine
            .worldspawn
            .get("northyaw")
            .map_or(0.0, |v| super::iw4::atof(v) as f32);
        Ok(Value::Float(yaw))
    });
    for name in ["logprint", "logstring"] {
        registry.register(Function, name, |_, _, args| {
            diag::info!(Sim, "gsc log: {}", string(args, 0)?.trim_end());
            Ok(Value::Undefined)
        });
    }
    registry.register(Function, "exitlevel", |world, _, _| {
        let mut state = runtime(world);
        if std::mem::replace(&mut state.finished, true) {
            return Err("exitlevel already called".into());
        }
        state.exit_level = true;
        drop(state);
        signal(world, EXIT_LEVEL)
    });
    registry.register(Function, "map_restart", |world, _, args| {
        let persist = !args.is_empty() && int(args, 0)? != 0;
        let mut state = runtime(world);
        if std::mem::replace(&mut state.finished, true) {
            return Err("map_restart already called".into());
        }
        state.pending_restart = Some(persist);
        drop(state);
        signal(world, MAP_RESTART)
    });
    registry.register(Function, "setmatchdatadef", |world, _, args| {
        bind_match_data(world, super::super::match_data::Scope::Match, args)
    });
    registry.register(Function, "setclientmatchdatadef", |world, _, args| {
        bind_match_data(world, super::super::match_data::Scope::Client, args)
    });
    registry.register(Function, "setmatchdata", |world, _, args| {
        runtime(world)
            .engine
            .match_data
            .set(super::super::match_data::Scope::Match, args)
    });
    registry.register(Function, "getmatchdata", |world, _, args| {
        runtime(world)
            .engine
            .match_data
            .get(super::super::match_data::Scope::Match, args)
    });
    registry.register(Function, "setclientmatchdata", |world, _, args| {
        runtime(world)
            .engine
            .match_data
            .set(super::super::match_data::Scope::Client, args)
    });
    registry.register(Function, "getclientmatchdata", |world, _, args| {
        runtime(world)
            .engine
            .match_data
            .get(super::super::match_data::Scope::Client, args)
    });
}

fn register_level(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.register(Function, "kick", |world, _, args| {
        if !(1..=2).contains(&args.len()) {
            return Err("Kick expects a client number and optional reason".into());
        }
        if !float(args, 0)?.is_finite() {
            return Err("Kick client number must be finite".into());
        }
        let number = int(args, 0)?;
        let client = u32::try_from(number).map_err(|_| "no client with that number")?;
        if crate::frame::FrameWorld::from_world(world)
            .client_meta(crate::ClientId(client))
            .is_none()
        {
            return Err("no client with that number".into());
        }
        let reason = optional(args, 1, string)?.unwrap_or_else(|| "EXE_PLAYERKICKED".into());
        if reason.is_empty() || reason.len() > 256 || reason.chars().any(char::is_control) {
            return Err("Kick reason must be 1–256 bytes without control characters".into());
        }
        let mut runtime = runtime(world);
        runtime.kicks.entry(client).or_insert(reason);
        runtime.disconnects.insert(client);
        Ok(Value::Undefined)
    });

    registry.register(Function, "soundexists", |world, _, args| {
        let name = string(args, 0)?;
        match crate::frame::FrameWorld::from_world(world).script_sound_exists(&name) {
            Some(exists) => Ok(Value::Int(i32::from(exists))),
            None => crate::script::host::registry::unavailable(
                world,
                "soundexists",
                "sound alias catalog is not installed",
            ),
        }
    });

    for name in ["precacheturret", "precachevehicle", "precachefxteamthermal"] {
        registry.register(Function, name, |world, _, args| {
            precache(world, "asset", string(args, 0)?).map(Value::Int)
        });
    }
    fn entity(
        world: &mut World,
        args: &[Value],
        origin_at: usize,
        orient_at: usize,
        repeat_ms: i32,
        cull_distance: f32,
    ) -> Result<Value, String> {
        let mut name = name(world, int(args, 0)?)?;
        let mut origin = vector(args, origin_at)?;
        let mut forward = optional(args, orient_at, vector)?.unwrap_or([0.0, 0.0, 1.0]);
        let mut up = optional(args, orient_at + 1, vector)?;
        if let Some((light, at)) =
            crate::script::host::players::insertion_light(world, &name, origin)
        {
            name = light.to_owned();
            origin = at;
            forward = [0.0, 0.0, 1.0];
            up = Some([1.0, 0.0, 0.0]);
        }
        let forward = glam::Vec3::from_array(forward)
            .try_normalize()
            .ok_or("effect forward vector is zero")?;
        let up = match up {
            Some(up) => {
                let up = glam::Vec3::from_array(up);
                (up - forward * up.dot(forward))
                    .try_normalize()
                    .ok_or("effect up vector is parallel to forward")?
            }
            None => forward.any_orthonormal_vector(),
        };
        let now = crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick);
        let mut runtime = runtime(world);
        let id = runtime.create_entity(EntityKind::Spawned, "script_model")?;
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        runtime.engine.effects.insert(
            id,
            super::super::entities::PersistentFx {
                name,
                origin,
                forward: forward.to_array(),
                up: up.to_array(),
                start_ms: (repeat_ms > 0).then_some(now),
                repeat_ms,
                cull_distance,
            },
        );
        Ok(Value::Object(id))
    }
    registry.register(Function, "spawnfx", |world, _, args| {
        entity(world, args, 1, 2, 0, 0.0)
    });
    registry.register(Function, "playloopedfx", |world, _, args| {
        let repeat = float(args, 1)?;
        if repeat <= 0.0 {
            return Err("playloopedfx repeat delay must be positive".into());
        }
        let cull = optional(args, 3, float)?.unwrap_or(0.0);
        entity(world, args, 2, 4, (repeat * 1000.0) as i32, cull)
    });
    registry.register(Function, "triggerfx", |world, _, args| {
        let id = entity_id(world, arg(args, 0)?)?;
        let delay = optional(args, 1, float)?.unwrap_or(0.0);
        let now = crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick);
        let mut runtime = runtime(world);
        let fx = runtime
            .engine
            .effects
            .get_mut(&id)
            .ok_or("triggerfx expects an effect entity")?;
        fx.start_ms = Some(now + (delay * 1000.0) as i32);
        Ok(Value::Undefined)
    });
    registry.register(Method, "usetriggerrequirelookat", |world, receiver, _| {
        let id = entity_id(world, receiver)?;
        runtime(world).require_look_at.insert(id);
        Ok(Value::Undefined)
    });
    macro_rules! presented {
        ($($name:literal),* $(,)?) => {$(
            registry.register(Function, $name, |world, _, args| {
                runtime(world).presented.insert($name, args.to_vec());
                Ok(Value::Undefined)
            });
        )*};
    }
    registry.register(Function, "earthquake", super::scene_effects::earthquake);
    registry.register(
        Function,
        "setslowmotion",
        super::scene_effects::set_slow_motion,
    );
    presented!["obituary", "playfxontagforclients", "setclientnamemode",];
    macro_rules! unavailable {
        ($reason:literal: $($name:literal),* $(,)?) => {$(
            registry.register(Function, $name, |world, _, _| {
                super::super::registry::unavailable(world, $name, $reason)
            });
        )*};
    }
    unavailable!("no script ranking service is connected": "sendranks", "setplayerteamrank");
    unavailable!("no script match-data upload service is connected": "sendmatchdata", "sendclientmatchdata");
    unavailable!("IW4 lobby termination is not bound to the IW4L lobby lifecycle": "endlobby");
    unavailable!("IW4 party termination is not bound to the IW4L party lifecycle": "endparty");
}

fn register_weapon_facts(registry: &mut NativeRegistry) {
    use Namespace::Function;

    registry.register(Function, "isweapondetonationtimed", |world, _, args| {
        let name = string(args, 0)?;
        let frame = crate::frame::FrameWorld::from_world(world);
        let weapon = crate::script_player::weapon_named(&frame, &name)?;
        Ok(Value::Int(
            frame
                .equipment_facts_for(weapon)
                .is_some_and(|facts| facts.timed_detonation)
                .into(),
        ))
    });
    registry.register(Function, "weaponclass", |world, _, args| {
        Ok(enum_name(
            WEAPON_CLASSES,
            weapon_facts(world, args)?.weap_class,
        ))
    });
    registry.register(Function, "weapontype", |world, _, args| {
        Ok(enum_name(
            WEAPON_TYPES,
            weapon_facts(world, args)?.weap_type,
        ))
    });
    registry.register(Function, "weaponinventorytype", |world, _, args| {
        Ok(enum_name(
            INVENTORY_TYPES,
            weapon_facts(world, args)?.inventory_type,
        ))
    });
    registry.register(Function, "weaponclipsize", |world, _, args| {
        Ok(Value::Int(weapon_facts(world, args)?.clip_size))
    });
    registry.register(Function, "weaponmaxammo", |world, _, args| {
        Ok(Value::Int(weapon_facts(world, args)?.max_ammo))
    });
    registry.register(Function, "weaponfiretime", |world, _, args| {
        Ok(Value::Float(
            weapon_facts(world, args)?.fire_time_ms as f32 / 1000.0,
        ))
    });
    registry.register(Function, "weaponinheritsperks", |world, _, args| {
        Ok(Value::Int(weapon_facts(world, args)?.inherits_perks.into()))
    });
    registry.register(Function, "weaponaltweaponname", |world, _, args| {
        let alternate = weapon_facts(world, args)?.alternate_weapon;
        if alternate == 0 {
            return Ok(Value::string("none"));
        }
        let frame = crate::frame::FrameWorld::from_world(world);
        Ok(Value::string(frame.weapon_script_name(alternate)))
    });
}

fn register_damage(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.register(Function, "setplayerignoreradiusdamage", |world, _, args| {
        let ignore = int(args, 0)? != 0;
        runtime(world).engine.players_ignore_radius_damage = ignore;
        Ok(Value::Undefined)
    });
    registry.register(Function, "radiusdamage", |world, _, args| {
        radius_damage(world, None, args)
    });
    registry.register(Method, "radiusdamage", |world, receiver, args| {
        let inflictor = runtime(world).presence_of(receiver);
        radius_damage(world, inflictor, args)
    });
    registry.register(Function, "glassradiusdamage", |_, _, args| {
        vector(args, 0)?;
        float(args, 1)?;
        float(args, 2)?;
        float(args, 3)?;
        Ok(Value::Undefined)
    });
}

fn register_refused(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    macro_rules! refused {
        ($namespace:ident: $($name:literal),* => $message:literal) => {$(
            registry.register($namespace, $name, |_, _, _| Err($message.into()));
        )*};
    }
    refused!(Function: "getanimlength", "animhasnotetrack", "getnotetracktimes"
        => "animation data is not loaded in the simulation");
    refused!(Function: "getweaponmodel", "getweaponhidetags"
        => "weapon models are not loaded in the simulation");
    refused!(Method: "getcorpseanim", "startragdoll", "isragdoll" => "receiver is not a corpse");
    refused!(Method: "itemweaponsetammo" => "receiver is not a weapon item");
    refused!(Method: "allowjump", "allowspectateteam", "anyammoforweaponmodes", "attachshieldmodel",
        "attackbuttonpressed", "beginlocationselection", "buttonpressed", "cameralinkto",
        "cameraunlink", "canplayerplacesentry", "clearperks", "clientclaimtrigger",
        "clientreleasetrigger", "cloneplayer", "closeingamemenu", "closemenu", "closepopupmenu",
        "controlslinkto", "controlsunlink", "detachshieldmodel", "disableoffhandweapons",
        "disableusability", "disableweapons", "disableweaponswitch", "dropitem", "dropscavengerbag",
        "enableoffhandweapons", "enableusability", "enableweapons", "enableweaponswitch",
        "endlocationselection", "finishplayerdamage", "forceusehintoff", "forceusehinton",
        "fragbuttonpressed", "freezecontrols", "getcurrentprimaryweapon", "getcurrentweapon",
        "getguid", "getoffhandsecondaryclass", "getplayerangles", "getplayerdata", "getrestedtime",
        "getspectatingplayer", "getstance", "getthirdpersoncrosshairoffset", "getweaponammoclip",
        "getweaponammostock", "getweaponslistall", "getweaponslistexclusives",
        "getweaponslistitems", "getweaponslistoffhands", "getweaponslistprimaries", "getxuid",
        "givemaxammo", "givestartammo", "giveweapon", "hasperk", "hasweapon",
        "isfiringturret", "ishost", "isitemunlocked", "ismantling", "isonground",
        "isonladder", "isusingonlinedataoffline", "isusingturret", "kc_regweaponforfxremoval",
        "laststandrevive", "meleebuttonpressed", "moveshieldmodel", "notifyonplayercommand",
        "openmenu", "openpopupmenu", "pingplayer", "player_recoilscaleoff", "player_recoilscaleon",
        "playerads", "playerforcedeathanim", "playerhide", "playerlinkedoffsetenable",
        "playerlinkto", "playerlinkweaponviewtodelta", "playlocalsound", "predictstreampos",
        "radarjamoff", "radarjamon", "remotecamerasoundscapeoff", "resetspreadoverride", "sayall",
        "sayteam", "secondaryoffhandbuttonpressed", "setactionslot", "setblurforplayer",
        "setcarddisplayslot", "setcardicon", "setcardnameplate", "setcardtitle", "setclientdvar",
        "setclientdvars", "setdepthoffield", "setempjammed", "setmovespeedscale", "setnormalhealth",
        "setoffhandprimaryclass", "setoffhandsecondaryclass", "setperk", "setplayerangles",
        "setplayerdata", "setrank", "setrearviewrenderenabled", "setspawnweapon",
        "setspectatedefaults", "setspreadoverride", "setstance", "setviewmodel",
        "setweaponammoclip", "setweaponammostock", "setweaponhudiconoverride", "shellshock",
        "showhudsplash", "spawn", "startac130", "stopac130", "stoplocalsound", "stoprumble",
        "stopshellshock", "stunplayer", "suicide", "switchtoweapon", "takeallweapons",
        "takeweapon", "thermalvisionfofoverlayoff", "thermalvisionfofoverlayon",
        "thermalvisionoff", "thermalvisionon", "unsetperk", "updatedmscores", "updatescores",
        "usebuttonpressed", "viewkick", "visionsetmissilecamforplayer", "visionsetnakedforplayer",
        "visionsetthermalforplayer", "weaponlockfinalize", "weaponlockfree",
        "weaponlocknoclearance", "weaponlockstart", "weaponlocktargettooclose",
        "worldpointinreticle_circle"
        => "receiver is not a player");
}

pub const EXIT_LEVEL: &str = "engine:exitlevel";
pub const MAP_RESTART: &str = "engine:map_restart";
