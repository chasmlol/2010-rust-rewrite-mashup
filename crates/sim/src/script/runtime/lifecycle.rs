use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::World;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::script::host;
use crate::script::{
    Fault, LevelData, Location, NativeRegistry, Program, Runtime, Thread, ThreadState, Value,
};

use super::{INSTRUCTION_BUDGET, execute, new_thread};

/// Runs during install, before the map's `script_struct` blocks are appended to `level.struct`.
pub const STRUCT_INIT: &str = "codescripts/struct::initstructs";

pub(crate) fn copy_state(source: &World, target: &mut World) {
    target.insert_resource(source.resource::<Runtime>().clone());
    target.insert_resource(source.resource::<host::mechanics::Mechanics>().clone());
    target.insert_resource(source.resource::<NativeRegistry>().clone());
    for entity in source.iter_entities() {
        if let Some(thread) = entity.get::<Thread>() {
            target.spawn(thread.clone());
        }
    }
}

pub(crate) fn reset(world: &mut World) {
    let ids: Vec<_> = world
        .query_filtered::<Entity, bevy_ecs::query::With<Thread>>()
        .iter(world)
        .collect();
    for id in ids {
        world.despawn(id);
    }
    world.insert_resource(Runtime::default());
    world.insert_resource(host::mechanics::Mechanics::default());
}

pub(crate) fn install(
    world: &mut World,
    program: Program,
    natives: NativeRegistry,
    level: LevelData,
) -> Result<(), Fault> {
    let plan = host::restart::RestartPlan {
        natives: natives.clone(),
        entities: Arc::new(level.entities),
        tables: Arc::new(level.tables),
        keys: Arc::new(level.keys),
        entries: Vec::new(),
        schemas: level.schemas,
        player_data_defaults: level.player_data_defaults.map(Arc::new),
    };
    install_level(world, Arc::new(program), plan)
}

pub(crate) fn install_level(
    world: &mut World,
    program: Arc<Program>,
    plan: host::restart::RestartPlan,
) -> Result<(), Fault> {
    let natives = plan.natives.clone();
    let location = Location {
        module: "<runtime>".into(),
        function: "install".into(),
        line: 0,
        column: 0,
    };
    if world.resource::<Runtime>().program.is_some() {
        return Err(Fault::at(
            &location,
            "program already installed; reset the match before replacing scripts",
        ));
    }
    let mut unbound: Vec<_> = program
        .natives
        .iter()
        .filter(|b| natives.get(b.namespace, b.name).is_none())
        .map(|b| b.name)
        .collect();
    unbound.sort_unstable();
    if !unbound.is_empty() {
        return Err(Fault::at(
            &location,
            format!("{} unbound natives: {}", unbound.len(), unbound.join(" ")),
        ));
    }
    world
        .resource_mut::<crate::PersistentDataStore>()
        .install_schemas(plan.schemas.clone())
        .map_err(|error| Fault::at(&location, format!("persistent data schema: {error:?}")))?;
    world
        .resource_mut::<crate::PersistentDataStore>()
        .install_defaults(plan.player_data_defaults.clone());
    let bound = program
        .natives
        .iter()
        .map(|b| natives.get(b.namespace, b.name).unwrap())
        .collect();
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.program = Some(program.clone());
    runtime.natives = bound;
    runtime.objects.insert(0, BTreeMap::new());
    runtime.objects.insert(1, BTreeMap::new());
    runtime.objects.insert(2, BTreeMap::new());
    runtime.next_object = 3;
    runtime.next_entity_number = playerstate_iw4::GENTITY_SPAWN_BASE;
    runtime.tables = plan.tables.clone();
    runtime.rng = u32::from_le_bytes(program.fingerprint()[..4].try_into().unwrap()) | 1;
    runtime.loading = true;
    world.insert_resource(natives);
    if let Some(&function) = program.names.get(STRUCT_INIT) {
        let mut thread = new_thread(world, &program, function, Value::Object(0), Vec::new())
            .map_err(|m| Fault::at(&location, m))?;
        world.resource_mut::<Runtime>().budget = INSTRUCTION_BUDGET;
        thread.state = ThreadState::Runnable;
        execute(world, &program, &mut thread, 0);
        let mut runtime = world.resource_mut::<Runtime>();
        if let Some(fault) = runtime.fault.clone() {
            return Err(fault);
        }
        if thread.state != ThreadState::Complete {
            return Err(Fault::at(&location, format!("{STRUCT_INIT} must not wait")));
        }
        runtime.started = false;
    }
    let entities = plan.entities.clone();
    let keys = plan.keys.clone();
    world.resource_mut::<Runtime>().restart = Some(Arc::new(plan));
    world
        .resource_mut::<Runtime>()
        .spawn_map_entities(&entities, &keys)
        .map_err(|m| Fault::at(&location, m))?;
    host::presence::initialize_map_models(world);
    Ok(())
}
