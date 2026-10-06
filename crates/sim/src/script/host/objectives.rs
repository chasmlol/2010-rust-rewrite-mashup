use super::args::{arg, int, string, vector};
use crate::frame::FrameWorld;
use crate::script::runtime::type_name;
use crate::script::{Namespace, NativeRegistry, Runtime, Value};
use crate::{CompassObjective, ObjectiveMatch, ObjectiveState, ScriptEffect};
use bevy_ecs::prelude::World;
use gamemode_iw4::Team;

const MAX_OBJECTIVES: i32 = 32;
const ENGINE_SERVER_INFO: &[&str] = &["ui_bomb_timer", "mapname", "g_gametype"];

#[derive(Clone, Debug, Default)]
pub(crate) struct ScriptObjective {
    state: ObjectiveState,
    origin: [f32; 3],
    entity: Option<Value>,
    team: Team,
    icon: String,
    viewer: Option<u32>,
}

impl ScriptObjective {
    pub(crate) fn entity(&self) -> Option<&Value> {
        self.entity.as_ref()
    }
}

fn index(args: &[Value]) -> Result<u8, String> {
    let index = int(args, 0)?;
    if !(0..MAX_OBJECTIVES).contains(&index) {
        return Err(format!("index {index} is an illegal objective index"));
    }
    Ok(index as u8)
}

fn state(args: &[Value], at: usize) -> Result<ObjectiveState, String> {
    let name = string(args, at)?;
    ObjectiveState::from_script(&name).ok_or_else(|| format!("Illegal objective state \"{name}\""))
}

fn objective(world: &mut World, index: u8) -> bevy_ecs::world::Mut<'_, ScriptObjective> {
    world
        .resource_mut::<Runtime>()
        .map_unchanged(|runtime| runtime.engine.objectives.entry(index).or_default())
}

fn place(world: &mut World, index: u8, target: &Value) -> Result<(), String> {
    match target {
        Value::Vector(origin) => {
            let mut row = objective(world, index);
            row.origin = *origin;
            row.entity = None;
        }
        Value::Object(_) => objective(world, index).entity = Some(target.clone()),
        other => return Err(format!("objective position cannot be {}", type_name(other))),
    }
    Ok(())
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::Function;
    registry.register(Function, "objective_add", |world, _, args| {
        let index = index(args)?;
        let state = state(args, 1)?;
        *objective(world, index) = ScriptObjective {
            state,
            ..Default::default()
        };
        if let Some(target) = args.get(2) {
            place(world, index, target)?;
        }
        if args.len() > 3 {
            objective(world, index).icon = string(args, 3)?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "objective_delete", |world, _, args| {
        let index = index(args)?;
        world
            .resource_mut::<Runtime>()
            .engine
            .objectives
            .remove(&index);
        Ok(Value::Undefined)
    });
    registry.register(Function, "objective_state", |world, _, args| {
        let index = index(args)?;
        objective(world, index).state = state(args, 1)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "objective_icon", |world, _, args| {
        let index = index(args)?;
        objective(world, index).icon = string(args, 1)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "objective_position", |world, _, args| {
        let index = index(args)?;
        let origin = vector(args, 1)?;
        place(world, index, &Value::Vector(origin))?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "objective_onentity", |world, _, args| {
        let index = index(args)?;
        let target = arg(args, 1)?.clone();
        if !matches!(target, Value::Object(_)) {
            return Err(format!("{} is not an entity", type_name(&target)));
        }
        place(world, index, &target)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "objective_team", |world, _, args| {
        let index = index(args)?;
        if let Some(client) = world.resource::<Runtime>().player_client_of(arg(args, 1)?) {
            let mut objective = objective(world, index);
            objective.team = Team::Free;
            objective.viewer = Some(client);
            return Ok(Value::Undefined);
        }
        let team = match string(args, 1)?.as_str() {
            "axis" => Team::Axis,
            "allies" => Team::Allies,
            "none" | "free" | "neutral" => Team::Free,
            other => return Err(format!("'{other}' is an illegal team string")),
        };
        let mut objective = objective(world, index);
        objective.team = team;
        objective.viewer = None;
        Ok(Value::Undefined)
    });
}

pub(crate) fn publish(world: &mut World) {
    let vehicles = super::vehicles::compass_rows(world);
    let vehicle_targets = super::vehicles::hud_targets(world);
    let rows: Vec<(u8, ScriptObjective)> = world
        .resource::<Runtime>()
        .engine
        .objectives
        .iter()
        .map(|(index, row)| (*index, row.clone()))
        .collect();
    let mut compass = Vec::with_capacity(rows.len());
    for (index, row) in rows {
        let origin = match &row.entity {
            Some(Value::Object(id)) if world.resource::<Runtime>().live(id) => {
                match super::players::entity_field(world, *id, "origin") {
                    Value::Vector(origin) => origin,
                    _ => row.origin,
                }
            }
            _ => row.origin,
        };
        compass.push(CompassObjective {
            index,
            state: row.state,
            origin,
            team: row.team,
            icon: row.icon,
            viewer: row.viewer,
        });
    }
    let runtime = world.resource::<Runtime>();
    let score = |team: &str| runtime.engine.team_scores.get(team).copied().unwrap_or(0);
    let scores = [0, score("axis"), score("allies")];
    let engine = ENGINE_SERVER_INFO
        .iter()
        .filter(|name| !runtime.server_info.contains(**name))
        .filter_map(|name| Some((name.to_string(), runtime.dvars.get(*name)?.clone())));
    let server_info = runtime
        .server_info
        .iter()
        .map(|name| {
            let value = runtime.dvars.get(name).cloned().unwrap_or_default();
            (name.clone(), value)
        })
        .chain(engine)
        .chain(
            runtime
                .dvars
                .iter()
                .filter(|(name, _)| {
                    crate::is_postfx_dvar(name) && !runtime.server_info.contains(*name)
                })
                .map(|(name, value)| (name.clone(), value.clone())),
        )
        .collect();
    let game_end_time = runtime.engine.game_end_time;
    let slow_motion = runtime.engine.slow_motion;
    let ambient = runtime
        .program
        .is_some()
        .then(|| runtime.engine.ambient.clone().unwrap_or_default());
    let ac130_ambient = runtime
        .program
        .is_some()
        .then(|| runtime.engine.ac130_ambient.clone().unwrap_or_default());
    let rumble_aliases = runtime
        .precached
        .iter()
        .filter(|((kind, _), _)| *kind == "rumble")
        .map(|((_, name), index)| (*index, name.clone()))
        .collect();
    let scripted_effects = runtime.program.is_some();
    let naked_vision = runtime.engine.naked_vision.clone();
    let thermal_vision = runtime.engine.thermal_vision.clone();
    let thermal_body_material = runtime.engine.thermal_body_material.clone();
    let missile_vision = runtime.engine.missile_vision.clone();
    let night_vision = runtime.engine.night_vision.clone();
    let pain_vision = runtime.engine.pain_vision.clone();
    let fog = runtime.engine.fog;
    let now = crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick);
    let rows: Vec<(u64, super::entities::PersistentFx, Option<u64>)> = runtime
        .engine
        .effects
        .iter()
        .filter_map(|(id, fx)| {
            let entity = runtime.entities.get(id)?;
            let viewers = entity.hidden.then_some(entity.shown_to);
            Some((*id, fx.clone(), viewers))
        })
        .collect();
    let mut runtime = world.resource_mut::<Runtime>();
    runtime
        .engine
        .effects
        .retain(|id, _| rows.iter().any(|(row, _, _)| row == id));
    runtime.engine.earthquakes.retain(|quake| quake.active(now));
    let earthquakes = runtime.engine.earthquakes.clone();
    let mut frame = FrameWorld::from_world(world);
    let effects = rows
        .into_iter()
        .map(|(id, fx, viewers)| ScriptEffect {
            id: id as u32,
            effect: frame.effect_name_index(&fx.name),
            origin: fx.origin,
            forward: fx.forward,
            up: fx.up,
            start_ms: fx.start_ms,
            repeat_ms: fx.repeat_ms,
            cull_distance: fx.cull_distance,
            viewers,
        })
        .collect();
    frame.objectives = ObjectiveMatch {
        scores,
        compass,
        vehicles,
        vehicle_targets,
        server_info,
        game_end_time,
        slow_motion,
        ambient,
        ac130_ambient,
        rumble_aliases,
        scripted_effects,
        effects,
        fog,
        earthquakes,
        naked_vision,
        thermal_vision,
        thermal_body_material,
        missile_vision,
        night_vision,
        pain_vision,
    };
}
