mod compiler;
mod error;
pub mod host;
mod ir;
pub mod profile;
mod program;
mod runtime;
mod source;
mod value;
pub(crate) mod vm;

pub(crate) use bevy_ecs::prelude::Resource;
pub(crate) use std::collections::{BTreeMap, VecDeque};
pub(crate) use std::sync::Arc;

pub(crate) use runtime::Runtime;

pub use error::{Fault, Location};
pub(crate) use host::controls::{
    action_slot_command, command_buttons, player_commands, select_location,
};
pub use host::entities::{
    KeyType, LevelData, StringTable, parse_entity_string, parse_radiant_keys,
};
pub(crate) use host::entity_damage::{
    EntityHit, HitTarget, ScriptBlast, ScriptHit, damage_entity, radius_targets,
};
pub(crate) use host::mechanics::Mechanics;
pub(crate) use host::mechanics::advance_mechanics;
pub use host::natives::engine::{EXIT_LEVEL, MAP_RESTART};
pub(crate) use host::natives::iw4::set_dvar;
pub(crate) use host::players::{
    answer_join, answer_menu, apply_disconnects, apply_player_links, choose_class,
    choose_default_class, disconnect_player, flashbang, force_death, give_killstreak, is_t5,
    note_team_answer, personal_class, player_damage, script_seats, set_profile, sync_players,
};
pub(crate) use host::presence::sync_presence;
pub use host::registry::{Native, NativeRegistry};
pub(crate) use host::restart::restart_level;
pub(crate) use host::weapons::sync_engine_events;
pub use ir::IR_VERSION;
pub(crate) use ir::{Binary, Callee, Function, Global, Op, Unary};
pub use profile::catalog::{Builtin, Catalog, Namespace, Owner};
pub use profile::iw4_startup::Iw4Startup;
pub use program::{ModuleIdentity, Program, Realm, Site};
pub(crate) use runtime::{
    advance_scheduler, copy_state, healthy, install, preflight, reset, start, take_signals,
};
pub use source::{FileSources, SourceOrigin, SourceResolver, decode_source, normalize_module};
pub(crate) use value::ArrayKey;
pub use value::{ScriptString, Value};
pub(crate) use vm::state::{Frame, Thread, ThreadState, Waiter, WaiterKind};

pub(crate) use host::entity_damage::{
    destructible_attacker, destructible_callback, destructible_debris, destructible_effect,
    set_destructible_model,
};
