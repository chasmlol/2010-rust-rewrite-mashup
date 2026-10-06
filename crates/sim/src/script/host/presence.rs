use super::entities::{EntityKind, SPAWNED_PRESENCE_BASE};
use crate::ScriptModelId;
use crate::bullet_collision::{AuthorityDObjState, EntityCollisionCapabilities};
use crate::frame::FrameWorld;
use crate::gentity::ScriptMoverGentity;
use crate::script::{Arc, BTreeMap, Runtime, Value};
use bevy_ecs::prelude::World;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Shown {
    origin: [f32; 3],
    angles: [f32; 3],
    model: Option<Arc<str>>,
    attachments: Vec<(Arc<str>, Arc<str>)>,
    hidden: bool,
    shown_to: u64,
    solid: bool,
    moving: bool,
}

struct Wanted {
    collision_only: bool,
    object: u64,
    presence: ScriptModelId,
    origin: [f32; 3],
    angles: [f32; 3],
    model: Option<Arc<str>>,
    attachments: Vec<(Arc<str>, Arc<str>)>,
    hidden: bool,
    shown_to: u64,
    solid: bool,
    part_ops: Vec<(Arc<str>, bool)>,
    anim_op: Option<Option<Arc<str>>>,
}

fn near(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.01)
}

fn near_angles(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter()
        .zip(b)
        .all(|(a, b)| math_iw4::angle_subtract(*a, b).abs() < 0.01)
}

fn vector(runtime: &mut Runtime, id: u64, name: &str) -> [f32; 3] {
    match runtime.object_field(id, name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

fn model_field(runtime: &mut Runtime, id: u64) -> Option<Arc<str>> {
    match runtime.object_field(id, "model") {
        Value::String(model) if !model.is_empty() && !model.starts_with('*') => Some(model.into()),
        _ => None,
    }
}

pub(crate) fn initialize_map_models(world: &mut World) {
    let models: BTreeMap<ScriptModelId, Arc<str>> = FrameWorld::from_world(world)
        .entity_collision_capabilities()
        .iter()
        .filter_map(|row| {
            Some((
                row.owner.script_model()?,
                row.dobj.as_ref()?.current_model.as_str().into(),
            ))
        })
        .collect();
    let mut runtime = world.resource_mut::<Runtime>();
    let objects: Vec<_> = runtime
        .entities
        .iter()
        .filter(|(_, entity)| entity.kind == EntityKind::Map)
        .filter_map(|(object, entity)| Some((*object, models.get(&entity.presence?)?.clone())))
        .collect();
    for (object, model) in objects {
        runtime.set_object_field(object, "model", Value::String(model.into()));
    }
}

pub(crate) fn spawn_presence(world: &mut World, origin: [f32; 3]) -> Result<ScriptModelId, String> {
    let mut runtime = world.resource_mut::<Runtime>();
    let serial = runtime.next_spawned_presence;
    runtime.next_spawned_presence = serial
        .checked_add(1)
        .filter(|n| *n < SPAWNED_PRESENCE_BASE)
        .ok_or("spawned script model identifiers exhausted")?;
    let id = ScriptModelId::from_wire(SPAWNED_PRESENCE_BASE + serial);
    let mut frame = FrameWorld::from_world(world);
    frame
        .spawn_script_mover(id, origin, [0.0; 3])
        .map_err(|_| "no free entities".to_owned())?;
    frame.insert_collision_owner(EntityCollisionCapabilities::current_tick(
        crate::AuthorityModelOwner::ScriptModel(id),
        None,
        Vec::new(),
    ));
    Ok(id)
}

/// The observability boundary for script writes: native collision reads go
/// through here, so a query sees every pose, solidity and visibility change a
/// native or field store made earlier in the same tick.
pub(crate) fn settled(world: &mut World) -> FrameWorld<'_> {
    settle_collision(world);
    FrameWorld::from_world(world)
}

/// Collision rows only. Networked mover state is left to `present`: posing a
/// mover mid-tick would break its tick-to-tick velocity.
pub(crate) fn settle_collision(world: &mut World) {
    let mut runtime = world.resource_mut::<Runtime>();
    let placed: Vec<(u64, ScriptModelId, bool, bool)> = runtime
        .entities
        .iter()
        .filter_map(|(id, e)| Some((*id, e.presence?, e.hidden, e.solid)))
        .collect();
    let wanted: BTreeMap<ScriptModelId, ([f32; 3], [f32; 3], bool, bool)> = placed
        .into_iter()
        .map(|(object, presence, hidden, solid)| {
            let origin = vector(&mut runtime, object, "origin");
            let angles = vector(&mut runtime, object, "angles");
            (presence, (origin, angles, hidden, solid))
        })
        .collect();
    let mut frame = FrameWorld::from_world(world);
    for row in frame.entity_collision_capabilities_mut() {
        let Some(&(origin, angles, hidden, solid)) =
            row.owner.script_model().and_then(|id| wanted.get(&id))
        else {
            continue;
        };
        row.hidden = hidden;
        row.solid = solid;
        let settled = row
            .followed_pose
            .is_some_and(|(at, facing)| near(at, origin) && near_angles(facing, angles));
        if settled {
            continue;
        }
        row.followed_pose = Some((origin, angles));
        if let Some(dobj) = row.dobj.as_mut() {
            dobj.set_world_pose(origin, angles);
        }
        for brush in &mut row.linked_brushes {
            brush.origin = origin;
            brush.angles = angles;
        }
    }
}

pub(crate) fn sync_presence(world: &mut World) {
    let request = world.resource::<crate::step::StepRequest>();
    if !request.reason.advances_authority_world() {
        return;
    }
    let tick = request.tick;
    let now = crate::level_time_ms(tick);
    settle_collision(world);
    super::entity_damage::apply_script_blasts(world, tick);
    publish_loop_sounds(world);
    super::objectives::publish(world);
    super::controls::sync_script_locks(world);
    super::triggers::dispatch_triggers(world);
    present(world, now);
    settle_collision(world);
    resolve_link_tags(world);
}

fn present(world: &mut World, now: i32) {
    let retired = std::mem::take(&mut world.resource_mut::<Runtime>().retired_presence);
    let wanted = collect_wanted(world);
    if retired.is_empty() && wanted.is_empty() {
        return;
    }
    let mut frame = FrameWorld::from_world(world);
    let movers: BTreeMap<ScriptModelId, ScriptMoverGentity> =
        crate::frame::collect_script_movers(frame.ecs())
            .into_iter()
            .map(|mover| (mover.id, mover))
            .collect();
    for (id, spawned) in retired {
        let Some(mover) = movers.get(&id) else {
            continue;
        };
        if spawned {
            frame.remove_script_mover_by_number(mover.state.number);
            frame.remove_collision_owner(id);
        } else if let Some(mover) = frame.script_mover_mut_by_number(mover.state.number) {
            mover.state.e_flags |= entity_iw4::CG_SCRIPT_MOVER_NODRAW;
            mover.nonsolid = true;
        }
    }
    let mut updates = Vec::new();
    for want in wanted {
        let Some(mover) = movers.get(&want.presence) else {
            continue;
        };
        let first = frame
            .ecs()
            .resource::<Runtime>()
            .shown
            .get(&want.object)
            .cloned();
        let number = first.is_none().then_some(mover.state.number);
        let shown = first.unwrap_or_else(|| Shown {
            origin: mover.state.tr_base,
            angles: mover.state.apos_tr_base,
            model: frame
                .collision_owner_mut(want.presence)
                .and_then(|row| row.dobj.as_ref())
                .map(|dobj| dobj.current_model.as_str().into()),
            attachments: Vec::new(),
            hidden: mover.state.e_flags & entity_iw4::CG_SCRIPT_MOVER_NODRAW != 0,
            shown_to: mover.shown_to,
            solid: !mover.nonsolid,
            moving: false,
        });
        let posed = !near(shown.origin, want.origin) || !near_angles(shown.angles, want.angles);
        if let Some(mover) = frame.script_mover_mut_by_number(mover.state.number) {
            if want.hidden || want.collision_only {
                mover.state.e_flags |= entity_iw4::CG_SCRIPT_MOVER_NODRAW;
            } else {
                mover.state.e_flags &= !entity_iw4::CG_SCRIPT_MOVER_NODRAW;
            }
            mover.nonsolid = !want.solid;
            mover.shown_to = want.shown_to;
        }
        if posed || shown.moving {
            frame.set_script_mover_pose(mover.state.number, now, want.origin, want.angles);
        }
        let reshaped = want.model != shown.model || want.attachments != shown.attachments;
        if reshaped || !want.part_ops.is_empty() || want.anim_op.is_some() {
            present_model(&mut frame, &want);
        }
        updates.push((
            want.object,
            number,
            Shown {
                origin: if posed { want.origin } else { shown.origin },
                angles: if posed { want.angles } else { shown.angles },
                model: want.model,
                attachments: want.attachments,
                hidden: want.hidden,
                shown_to: want.shown_to,
                solid: want.solid,
                moving: posed,
            },
        ));
    }
    let movers = crate::frame::collect_script_movers(frame.ecs());
    crate::presence::follow_movers(frame.entity_collision_capabilities_mut(), &movers, now);
    let mut runtime = world.resource_mut::<Runtime>();
    for (object, number, shown) in updates {
        let Some(entity) = runtime.entities.get_mut(&object) else {
            continue;
        };
        if let Some(number) = number.filter(|_| !matches!(entity.kind, EntityKind::Missile(_))) {
            entity.number = number;
        }
        runtime.shown.insert(object, shown);
    }
}

fn tag_lookup(world: &mut World, object: u64, tag: &str) -> Option<Option<[f32; 3]>> {
    let presence = world
        .resource::<Runtime>()
        .entities
        .get(&object)?
        .presence?;
    let frame = settled(world);
    let dobj = frame
        .entity_collision_capabilities()
        .iter()
        .find(|row| row.owner.script_model() == Some(presence))?
        .dobj
        .as_ref()?;
    Some(dobj.tag_world_pose(tag).map(|(at, _)| {
        dobj.world_from_model
            .inverse()
            .transform_point3(glam::Vec3::from(at))
            .to_array()
    }))
}

pub(crate) fn tag_world(
    world: &mut World,
    object: u64,
    tag: &str,
) -> Option<([f32; 3], [[f32; 3]; 3])> {
    let presence = world
        .resource::<Runtime>()
        .entities
        .get(&object)?
        .presence?;
    let frame = settled(world);
    let matrix = frame
        .entity_collision_capabilities()
        .iter()
        .find(|row| row.owner.script_model() == Some(presence))?
        .dobj
        .as_ref()?
        .tag_world_matrix(tag)?;
    let axis = |v: glam::Vec4| v.truncate().normalize_or_zero().to_array();
    Some((
        matrix.w_axis.truncate().to_array(),
        [
            axis(matrix.x_axis),
            axis(matrix.y_axis),
            axis(matrix.z_axis),
        ],
    ))
}

pub(crate) fn tag_offset(world: &mut World, object: u64, tag: &str) -> Option<[f32; 3]> {
    tag_lookup(world, object, tag).flatten()
}

fn resolve_link_tags(world: &mut World) {
    let pending: Vec<(u64, u64, Arc<str>)> = world
        .resource::<Runtime>()
        .entities
        .iter()
        .filter_map(|(id, e)| {
            let link = e.linked_to.as_ref().filter(|l| l.tag_offset.is_none())?;
            Some((*id, link.parent, link.tag.clone()?))
        })
        .collect();
    for (id, parent, tag) in pending {
        let Some(offset) = tag_lookup(world, parent, &tag) else {
            continue;
        };
        if let Some(link) = world
            .resource_mut::<Runtime>()
            .entities
            .get_mut(&id)
            .and_then(|e| e.linked_to.as_mut())
        {
            link.tag_offset = Some(offset.unwrap_or([0.0; 3]));
        }
    }
}

fn collect_wanted(world: &mut World) -> Vec<Wanted> {
    let mut runtime = world.resource_mut::<Runtime>();
    let ids: Vec<(u64, ScriptModelId, bool, u64, bool)> = runtime
        .entities
        .iter()
        .filter_map(|(id, e)| e.presence.map(|p| (*id, p, e.hidden, e.shown_to, e.solid)))
        .collect();
    let mut wanted = Vec::with_capacity(ids.len());
    for (object, presence, hidden, shown_to, solid) in ids {
        let origin = vector(&mut runtime, object, "origin");
        let angles = vector(&mut runtime, object, "angles");
        let model = model_field(&mut runtime, object);
        let entity = runtime.entities.get_mut(&object).unwrap();
        let part_ops = std::mem::take(&mut entity.part_ops);
        let anim_op = entity.anim_op.take();
        let attachments = entity.attachments.clone();
        let collision_only = matches!(entity.kind, EntityKind::Missile(_));
        let unchanged = part_ops.is_empty()
            && anim_op.is_none()
            && runtime.shown.get(&object).is_some_and(|shown| {
                !shown.moving
                    && shown.hidden == hidden
                    && shown.shown_to == shown_to
                    && shown.solid == solid
                    && shown.model == model
                    && shown.attachments == attachments
                    && near(shown.origin, origin)
                    && near_angles(shown.angles, angles)
            });
        if unchanged {
            continue;
        }
        wanted.push(Wanted {
            collision_only,
            object,
            presence,
            origin,
            angles,
            model,
            attachments,
            hidden,
            shown_to,
            solid,
            part_ops,
            anim_op,
        });
    }
    wanted
}

fn present_model(frame: &mut FrameWorld, want: &Wanted) {
    let capability = want
        .model
        .as_deref()
        .and_then(|model| frame.model_capability(model))
        .flatten();
    let anim = want
        .anim_op
        .as_ref()
        .and_then(|op| op.as_deref().and_then(|clip| frame.script_model_anim(clip)));
    let Some(row) = frame.collision_owner_mut(want.presence) else {
        return;
    };
    match (&want.model, row.dobj.as_mut()) {
        (None, _) => row.dobj = None,
        (Some(model), Some(dobj)) => {
            if dobj.current_model != **model {
                dobj.replace_model(model, capability);
            }
        }
        (Some(model), None) => {
            row.dobj = Some(AuthorityDObjState::at_pose(
                model,
                capability,
                want.origin,
                want.angles,
            ));
            row.followed_pose = Some((want.origin, want.angles));
        }
    }
    let Some(dobj) = row.dobj.as_mut() else {
        return;
    };
    let attachments: Vec<(&str, &str)> = want
        .attachments
        .iter()
        .map(|(model, tag)| (&**model, &**tag))
        .collect();
    dobj.set_attachments(&attachments);
    for (tag, hidden) in &want.part_ops {
        dobj.set_tag_hidden(tag, *hidden);
    }
    match &want.anim_op {
        Some(Some(clip)) => {
            let anim = anim.unwrap_or(crate::ScriptModelPlayAnim {
                looping: false,
                frequency: 0.0,
            });
            dobj.begin_script_model_play_anim(clip, anim.looping, anim.frequency);
        }
        Some(None) => dobj.clear_script_model_play_anim(),
        None => {}
    }
}

const UNPRESENTED_LOOP_OWNER: u32 = 0x2000_0000;

fn publish_loop_sounds(world: &mut World) {
    let mut runtime = world.resource_mut::<Runtime>();
    let speaking: Vec<(u64, Option<ScriptModelId>, Arc<str>, i32)> = runtime
        .entities
        .iter()
        .filter_map(|(object, e)| Some((*object, e.presence, e.loop_sound.clone()?, e.number)))
        .collect();
    let placed: Vec<(ScriptModelId, Arc<str>, [f32; 3], Option<u32>)> = speaking
        .into_iter()
        .map(|(object, presence, alias, number)| {
            let owner = presence.unwrap_or_else(|| {
                ScriptModelId::from_wire(UNPRESENTED_LOOP_OWNER | (object as u32 & 0x0fff_ffff))
            });
            (
                owner,
                alias,
                vector(&mut runtime, object, "origin"),
                u32::try_from(number).ok(),
            )
        })
        .collect();
    let mut frame = FrameWorld::from_world(world);
    let rows = placed
        .into_iter()
        .map(
            |(owner, alias, origin, snd_ent)| crate::world_objects::DestructibleLoopSound {
                snd_ent,
                owner,
                alias_index: frame.sound_alias_index(&alias),
                origin,
            },
        )
        .collect();
    frame.world_objects_mut().set_destructible_loop_sounds(rows);
}

impl Runtime {
    // GSC can link solid visual models to a missile (for example its grenade indicator).
    // Those models remain hittable, but must not obstruct their own parent missile.
    pub(crate) fn missile_collision_models(&self, id: crate::ProjectileId) -> Vec<ScriptModelId> {
        let Some(&missile) = self.missiles.get(&id) else {
            return Vec::new();
        };
        self.entities
            .iter()
            .filter_map(|(&object, entity)| {
                let presence = entity.presence?;
                let mut parent = object;
                for _ in 0..self.entities.len() {
                    if parent == missile {
                        return Some(presence);
                    }
                    parent = self.entities.get(&parent)?.linked_to.as_ref()?.parent;
                }
                None
            })
            .collect()
    }

    pub(crate) fn presence_of(&self, value: &Value) -> Option<ScriptModelId> {
        self.entity(value).and_then(|(_, e)| e.presence)
    }

    pub(crate) fn presented_by(&self, id: ScriptModelId) -> Option<u64> {
        self.entities
            .iter()
            .find(|(_, e)| e.presence == Some(id) && e.kind != EntityKind::HudElem)
            .map(|(object, _)| *object)
    }
}
