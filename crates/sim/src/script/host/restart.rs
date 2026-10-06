use super::entities::{EntityKind, KeyType};
use crate::frame::FrameWorld;
use crate::script::runtime::{install_level, reset, run_now};
use crate::script::{
    Arc, ArrayKey, BTreeMap, Fault, Location, NativeRegistry, Runtime, StringTable, Value,
};
use bevy_ecs::prelude::World;

#[derive(Clone)]
pub(crate) struct RestartPlan {
    pub natives: NativeRegistry,
    pub entities: Arc<Vec<Vec<(String, String)>>>,
    pub tables: Arc<BTreeMap<String, StringTable>>,
    pub keys: Arc<BTreeMap<String, KeyType>>,
    pub entries: Vec<String>,
    pub schemas: BTreeMap<String, Arc<structured_data_iw4::DefinitionSet>>,
    pub player_data_defaults: Option<Arc<crate::PlayerDataDefaults>>,
}

impl std::fmt::Debug for RestartPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RestartPlan")
            .field("entries", &self.entries)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Detached {
    Value(Value),
    Array(Vec<(ArrayKey, Detached)>),
}

const MAX_DEPTH: usize = 64;

fn detach(runtime: &Runtime, value: &Value, depth: usize) -> Option<Detached> {
    match value {
        Value::Undefined
        | Value::Int(_)
        | Value::Float(_)
        | Value::String(_)
        | Value::LocalizedString(_)
        | Value::Vector(_) => Some(Detached::Value(value.clone())),
        Value::Array(id) if depth < MAX_DEPTH => Some(Detached::Array(
            runtime
                .arrays
                .get(id)?
                .iter()
                .filter_map(|(key, value)| Some((key.clone(), detach(runtime, value, depth + 1)?)))
                .collect(),
        )),
        _ => None,
    }
}

pub(crate) fn attach(runtime: &mut Runtime, value: Detached) -> Result<Value, String> {
    match value {
        Detached::Value(value) => Ok(value),
        Detached::Array(rows) => {
            let mut array = BTreeMap::new();
            for (key, value) in rows {
                array.insert(key, attach(runtime, value)?);
            }
            let id = runtime.next_object;
            runtime.next_object = id.checked_add(1).ok_or("object identifier exhausted")?;
            runtime.arrays.insert(id, array);
            Ok(Value::Array(id))
        }
    }
}

fn symbol_name(runtime: &Runtime, id: u32) -> Option<Arc<str>> {
    let program = runtime.program.as_ref()?;
    program.symbols.get(id as usize).cloned().or_else(|| {
        runtime
            .dynamic_symbols
            .iter()
            .find(|(_, symbol)| **symbol == id)
            .map(|(name, _)| name.clone())
    })
}

fn field(runtime: &Runtime, object: u64, name: &str) -> Option<Value> {
    let program = runtime.program.as_ref()?;
    let id = program
        .symbol_ids
        .get(name)
        .or_else(|| runtime.dynamic_symbols.get(name))?;
    runtime.objects.get(&object)?.get(id).cloned()
}

pub(crate) fn restart_level(world: &mut World, tick: crate::Tick) {
    let runtime = world.resource::<Runtime>();
    let Some(persist) = runtime.pending_restart else {
        return;
    };
    if runtime.fault.is_some() {
        return;
    }
    let (Some(program), Some(plan)) = (runtime.program.clone(), runtime.restart.clone()) else {
        return;
    };
    let game: Vec<(Arc<str>, Detached)> = runtime
        .objects
        .get(&1)
        .into_iter()
        .flatten()
        .filter_map(|(id, value)| Some((symbol_name(runtime, *id)?, detach(runtime, value, 0)?)))
        .collect();
    let pers: BTreeMap<u32, Detached> = runtime
        .players
        .iter()
        .filter(|_| persist)
        .filter_map(|(client, slot)| {
            let pers = field(runtime, slot.object, "pers")?;
            Some((*client, detach(runtime, &pers, 0)?))
        })
        .collect();
    let dvars = runtime.dvars.clone();
    let local_presentation_dvars = runtime.local_presentation_dvars;
    let local_presentation_client = runtime.local_presentation_client;
    let pending_local_dvars = runtime.pending_local_dvars.clone();
    let weapon_bridge = runtime.weapon_bridge.clone();
    let personal_classes = runtime.personal_classes.clone();
    let next_presence = runtime.next_spawned_presence;
    let huds: Vec<u64> = runtime.hud_slots.keys().copied().collect();
    let clients: Vec<u32> = runtime.players.keys().copied().collect();
    let spawned: Vec<crate::ScriptModelId> = runtime
        .entities
        .values()
        .filter(|e| matches!(e.kind, EntityKind::Spawned | EntityKind::Vehicle))
        .filter_map(|e| e.presence)
        .collect();

    for id in huds {
        super::hud::destroy(world, id);
    }
    for client in clients {
        super::players::unlink_player(world, client);
    }
    {
        let mut frame = FrameWorld::from_world(world);
        for mover in crate::frame::collect_script_movers(frame.ecs()) {
            if spawned.contains(&mover.id) {
                frame.remove_script_mover_by_number(mover.state.number);
                frame.remove_collision_owner(mover.id);
            }
        }
        for projectile in crate::frame::collect_projectiles(frame.ecs()) {
            frame.remove_projectile_by_number(projectile.entnum);
        }
        for number in frame.dropped_item_numbers_sorted() {
            frame.remove_dropped_item_by_number(number);
        }
        crate::t5_destructible::restart(&mut frame);
        frame.restart_level_phase();
        for id in frame.client_ids_sorted() {
            let (origin, angles) = frame
                .player(id)
                .map_or(([0.0; 3], [0.0; 3]), |ps| (ps.origin, ps.viewangles));
            crate::script_player::spawn(&mut frame, tick, id, origin, angles, "spectator");
        }
    }

    let match_data = std::mem::take(&mut world.resource_mut::<Runtime>().engine.match_data);
    reset(world);
    {
        let mut runtime = world.resource_mut::<Runtime>();
        runtime.dvars = dvars;
        runtime.local_presentation_dvars = local_presentation_dvars;
        runtime.local_presentation_client = local_presentation_client;
        runtime.pending_local_dvars = pending_local_dvars;
        runtime.engine.match_data = match_data;
    }
    if let Err(fault) = install_level(world, program, (*plan).clone()) {
        world.resource_mut::<Runtime>().fault = Some(fault);
        return;
    }
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.last_tick = Some(tick);
    runtime.weapon_bridge = weapon_bridge;
    runtime.personal_classes = personal_classes;
    runtime.next_spawned_presence = next_presence;
    runtime.restored_pers = pers;
    for (name, value) in game {
        match attach(&mut runtime, value) {
            Ok(value) => runtime.set_object_field(1, &name, value),
            Err(message) => {
                runtime.fault = Some(Fault::at(
                    &Location {
                        module: "<engine>".into(),
                        function: "map_restart".into(),
                        line: 0,
                        column: 0,
                    },
                    message,
                ));
                return;
            }
        }
    }
    drop(runtime);
    for entry in &plan.entries {
        if let Err(fault) = run_now(
            world,
            entry,
            Value::level(),
            Vec::new(),
            i64::from(crate::level_time_ms(tick)),
        ) {
            world.resource_mut::<Runtime>().fault = Some(fault);
            return;
        }
    }
}
