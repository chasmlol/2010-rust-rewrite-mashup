use crate::ScriptModelId;
use crate::frame::FrameWorld;
use crate::script::runtime::raise;
use crate::script::{Runtime, Value};
use crate::world::ClientId;
use bevy_ecs::prelude::World;

#[derive(Clone, Debug)]
pub(crate) struct EntityHit {
    pub target: ScriptModelId,
    pub amount: i32,
    pub attacker: Option<ClientId>,
    pub means: &'static str,
    pub weapon: u32,
    pub point: [f32; 3],
    pub dir: [f32; 3],
    pub bone: Option<usize>,
    pub flags: i32,
}

const MEANS: &[&str] = &[
    "MOD_UNKNOWN",
    "MOD_PISTOL_BULLET",
    "MOD_RIFLE_BULLET",
    "MOD_EXPLOSIVE_BULLET",
    "MOD_GRENADE",
    "MOD_GRENADE_SPLASH",
    "MOD_PROJECTILE",
    "MOD_PROJECTILE_SPLASH",
    "MOD_MELEE",
    "MOD_HEAD_SHOT",
    "MOD_CRUSH",
    "MOD_FALLING",
    "MOD_SUICIDE",
    "MOD_TRIGGER_HURT",
    "MOD_EXPLOSIVE",
    "MOD_IMPACT",
    "MOD_BURNED",
    "MOD_GAS",
    "MOD_HIT_BY_OBJECT",
    "MOD_BAYONET",
    "MOD_TELEFRAG",
    "MOD_ARTILLERY",
    "MOD_DOGS",
];

pub(crate) fn means_named(name: &str) -> Result<&'static str, String> {
    MEANS
        .iter()
        .copied()
        .find(|means| means.eq_ignore_ascii_case(name))
        .ok_or_else(|| format!("unknown means of death '{name}'"))
}

#[derive(Clone, Debug)]
pub(crate) struct ScriptBlast {
    pub origin: [f32; 3],
    pub radius: f32,
    pub max: f32,
    pub min: f32,
    pub attacker: Option<ClientId>,
    pub inflictor: Option<ScriptModelId>,
    pub means: &'static str,
    pub weapon: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct ScriptHit {
    pub piece: Option<i32>,
    pub target: HitTarget,
    pub amount: i32,
    pub origin: [f32; 3],
    pub attacker: Option<ClientId>,
    pub inflictor: Option<ScriptModelId>,
    pub means: &'static str,
    pub weapon: u32,
    pub flags: i32,
    pub hitloc: u8,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum HitTarget {
    Player(ClientId),
    Entity(ScriptModelId),
}

pub(crate) fn apply_script_blasts(world: &mut World, tick: crate::Tick) {
    let (blasts, hits) = match world.get_resource_mut::<Runtime>() {
        Some(mut runtime) => (
            std::mem::take(&mut runtime.blasts),
            std::mem::take(&mut runtime.hits),
        ),
        None => return,
    };
    for hit in hits {
        match hit.target {
            HitTarget::Player(_) => {
                crate::damage::apply_script_hit(&mut FrameWorld::from_world(world), tick, &hit);
            }
            HitTarget::Entity(target) => {
                if let Some(piece) = hit.piece {
                    crate::t5_destructible::apply_piece_hit(
                        &mut FrameWorld::from_world(world),
                        tick,
                        crate::AuthorityModelOwner::ScriptModel(target),
                        piece,
                        hit.amount,
                        hit.attacker,
                    );
                    continue;
                }
                let mut runtime = world.resource_mut::<Runtime>();
                let at = match runtime
                    .presented_by(target)
                    .map(|o| runtime.object_field(o, "origin"))
                {
                    Some(Value::Vector(at)) => at,
                    _ => hit.origin,
                };
                drop(runtime);
                damage_entity(
                    world,
                    &EntityHit {
                        target,
                        amount: hit.amount,
                        attacker: hit.attacker,
                        means: hit.means,
                        weapon: hit.weapon,
                        point: hit.origin,
                        dir: std::array::from_fn(|i| at[i] - hit.origin[i]),
                        bone: None,
                        flags: hit.flags,
                    },
                );
            }
        }
    }
    for blast in blasts {
        let mut frame = FrameWorld::from_world(world);
        crate::damage::apply_script_blast(&mut frame, tick, &blast);
        for (target, mid, dist) in radius_targets(world, blast.origin, blast.radius) {
            if Some(target) == blast.inflictor {
                continue;
            }
            let amount =
                gamemode_iw4::radius_damage_amount(blast.max, blast.min, blast.radius, dist, 1.0);
            if amount <= 0 {
                continue;
            }
            damage_entity(
                world,
                &EntityHit {
                    target,
                    amount,
                    attacker: blast.attacker,
                    means: blast.means,
                    weapon: blast.weapon,
                    point: mid,
                    dir: std::array::from_fn(|i| mid[i] - blast.origin[i]),
                    bone: None,
                    flags: 1,
                },
            );
        }
    }
}

pub(crate) fn radius_targets(
    world: &mut World,
    origin: [f32; 3],
    radius: f32,
) -> Vec<(ScriptModelId, [f32; 3], f32)> {
    let Some(mut runtime) = world.get_resource_mut::<Runtime>() else {
        return Vec::new();
    };
    let candidates: Vec<(u64, ScriptModelId)> = runtime
        .entities
        .iter()
        .filter(|(_, entity)| entity.accepts_damage(crate::script_player::IDFLAGS_RADIUS))
        .filter_map(|(object, entity)| Some((*object, entity.presence?)))
        .collect();
    let placed: Vec<(ScriptModelId, [f32; 3])> = candidates
        .into_iter()
        .map(
            |(object, id)| match runtime.object_field(object, "origin") {
                Value::Vector(at) => (id, at),
                _ => (id, [0.0; 3]),
            },
        )
        .collect();
    let frame = FrameWorld::from_world(world);
    let mut targets = Vec::new();
    for (id, at) in placed {
        let (mid, half) = frame
            .entity_collision_capabilities()
            .iter()
            .find(|row| row.owner.script_model() == Some(id))
            .and_then(|row| row.linked_brushes.first())
            .and_then(|brush| {
                let model = frame
                    .clip_cmodels()
                    .models
                    .get(brush.cmodel_handle as usize)?;
                Some((
                    std::array::from_fn(|i| {
                        brush.origin[i] + (model.mins[i] + model.maxs[i]) * 0.5
                    }),
                    std::array::from_fn(|i| (model.maxs[i] - model.mins[i]) * 0.5),
                ))
            })
            .unwrap_or((at, [0.0; 3]));
        let dist = gamemode_iw4::radius_damage_distance_to_aabb(origin, mid, half);
        if dist < radius {
            targets.push((id, mid, dist));
        }
    }
    targets.sort_by(|a, b| a.2.total_cmp(&b.2));
    targets
}

pub(crate) fn damage_entity(world: &mut World, hit: &EntityHit) -> bool {
    let Some(object) = world
        .get_resource::<Runtime>()
        .and_then(|runtime| runtime.presented_by(hit.target))
        .filter(|object| world.resource::<Runtime>().entities[object].accepts_damage(hit.flags))
    else {
        return false;
    };
    let frame = FrameWorld::from_world(world);
    let tag = hit
        .bone
        .and_then(|bone| {
            frame
                .entity_collision_capabilities()
                .iter()
                .find(|row| row.owner.script_model() == Some(hit.target))
                .and_then(|row| row.dobj.as_ref())
                .and_then(|dobj| dobj.bone_name(bone))
                .map(str::to_owned)
        })
        .unwrap_or_default();
    let weapon = crate::script_player::weapon_name(&frame, hit.weapon);
    let attacker = hit.attacker.map_or(Value::Undefined, |a| {
        super::players::player_object(world, a.0)
    });
    if world.resource::<Runtime>().entities[&object].kind == super::entities::EntityKind::Vehicle {
        super::vehicles::damage(world, object, hit, attacker, &weapon, &tag);
        return true;
    }
    let mut runtime = world.resource_mut::<Runtime>();
    let model = match runtime.object_field(object, "model") {
        Value::String(model) => model,
        _ => "".into(),
    };
    let before = match runtime.object_field(object, "health") {
        Value::Int(health) => health,
        Value::Float(health) => health as i32,
        _ => 0,
    };
    let after = before.saturating_sub(hit.amount);
    runtime.set_object_field(object, "health", Value::Int(after));
    drop(runtime);
    let receiver = Value::Object(object);
    raise(
        world,
        receiver.clone(),
        "damage",
        vec![
            Value::Int(hit.amount),
            attacker.clone(),
            Value::Vector(hit.dir),
            Value::Vector(hit.point),
            Value::string(hit.means),
            Value::String(model),
            Value::String(tag.as_str().into()),
            Value::string(""),
            Value::Int(hit.flags),
            Value::String(weapon.into()),
        ],
    );
    if before > 0 && after <= 0 {
        raise(world, receiver, "death", vec![attacker]);
    }
    true
}

pub(crate) fn destructible_callback(
    world: &mut World,
    owner: crate::AuthorityModelOwner,
    entry: &str,
    args: Vec<Value>,
) {
    let Some(object) = owner
        .script_model()
        .and_then(|id| world.get_resource::<Runtime>()?.presented_by(id))
    else {
        return;
    };
    let name = format!("iw4l_maps/destructibles::{entry}");
    let now = super::players::now_ms(world);
    if let Err(fault) =
        crate::script::runtime::run_now(world, &name, Value::Object(object), args, now)
    {
        diag::warn!(Sim, "destructible callback unavailable: {fault:?}");
    }
}

pub(crate) fn destructible_attacker(world: &World, attacker: Option<crate::ClientId>) -> Value {
    attacker.map_or(Value::Undefined, |client| {
        super::players::player_object(world, client.0)
    })
}

pub(crate) fn destructible_effect(
    world: &mut World,
    name: String,
    origin: [f32; 3],
    direction: [f32; 3],
) {
    let now = super::players::now_ms(world);
    let Some(mut runtime) = world.get_resource_mut::<Runtime>() else {
        return;
    };
    let Ok(id) = runtime.create_entity(super::entities::EntityKind::Spawned, "script_model") else {
        return;
    };
    let forward = glam::Vec3::from_array(direction)
        .try_normalize()
        .unwrap_or(glam::Vec3::Z);
    runtime.engine.effects.insert(
        id,
        super::entities::PersistentFx {
            name,
            origin,
            forward: forward.to_array(),
            up: forward.any_orthonormal_vector().to_array(),
            start_ms: Some(now as i32),
            repeat_ms: 0,
            cull_distance: 0.0,
        },
    );
}

pub(crate) fn set_destructible_model(
    world: &mut World,
    owner: crate::AuthorityModelOwner,
    model: Option<&str>,
    sound: Option<&str>,
) {
    let Some(mut runtime) = world.get_resource_mut::<Runtime>() else {
        return;
    };
    let Some(object) = owner.script_model().and_then(|id| runtime.presented_by(id)) else {
        return;
    };
    if let Some(model) = model {
        runtime.set_object_field(object, "model", Value::string(model));
    }
    runtime.entities.get_mut(&object).unwrap().loop_sound = sound.map(std::sync::Arc::from);
}

pub(crate) fn destructible_debris(
    world: &mut World,
    dobj: crate::AuthorityDObjState,
    offset: [f32; 3],
    half: [f32; 3],
    velocity: [f32; 3],
) {
    let origin = dobj.world_from_model.w_axis.truncate().to_array();
    let rotation = dobj.world_from_model;
    let forward = rotation.x_axis.truncate();
    let angles = [
        -forward.z.atan2(forward.truncate().length()).to_degrees(),
        forward.y.atan2(forward.x).to_degrees(),
        0.0,
    ];
    let Ok(object) = world
        .resource_mut::<Runtime>()
        .create_entity(super::entities::EntityKind::Spawned, "script_model")
    else {
        return;
    };
    let presence = match super::presence::spawn_presence(world, origin) {
        Ok(presence) => presence,
        Err(_) => {
            world.resource_mut::<Runtime>().delete_entity(object);
            return;
        }
    };
    let model = dobj.current_model.clone();
    let mut frame = FrameWorld::from_world(world);
    if let Some(row) = frame.collision_owner_mut(presence) {
        row.dobj = Some(dobj);
        row.solid = false;
    }
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.set_object_field(object, "origin", Value::Vector(origin));
    runtime.set_object_field(object, "angles", Value::Vector(angles));
    runtime.set_object_field(object, "model", Value::string(&model));
    let entity = runtime.entities.get_mut(&object).unwrap();
    entity.presence = Some(presence);
    entity.solid = false;
    drop(runtime);
    let now = super::players::now_ms(world);
    world
        .resource_mut::<Runtime>()
        .lingering
        .push((now + 30000, object));
    world
        .resource_mut::<super::mechanics::Mechanics>()
        .launch_piece(object, velocity, offset, half);
}
