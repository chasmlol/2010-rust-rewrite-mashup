use bevy_ecs::prelude::Resource;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use crate::script::host;
use crate::script::{ArrayKey, Fault, Native, Program, StringTable, Value};

#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct Runtime {
    pub(crate) program: Option<Arc<Program>>,
    pub(crate) natives: Vec<Native>,
    pub(crate) next_serial: u64,
    pub fault: Option<Fault>,
    pub(crate) fault_reported: bool,
    pub(crate) errors: BTreeMap<(String, String), u64>,
    /// Deleted entities scripts may still hold. They keep their fields until
    /// the end of the frame that deleted them, then read as undefined.
    pub(crate) dead: std::collections::BTreeSet<u64>,
    pub(crate) dying: Vec<u64>,
    pub(crate) last_tick: Option<crate::Tick>,
    pub(crate) started: bool,
    pub(crate) objects: BTreeMap<u64, BTreeMap<u32, Value>>,
    pub(crate) next_object: u64,
    pub(crate) arrays: BTreeMap<u64, BTreeMap<ArrayKey, Value>>,
    pub(crate) dynamic_symbols: BTreeMap<Arc<str>, u32>,
    pub(crate) buckets: BTreeMap<i64, VecDeque<u64>>,
    pub(crate) spawned: Vec<u64>,
    pub(crate) waiters: Vec<crate::script::Waiter>,
    pub(crate) dvars: BTreeMap<String, String>,
    pub(crate) local_presentation_dvars: bool,
    pub(crate) local_presentation_client: Option<crate::ClientId>,
    pub(crate) pending_local_dvars: Vec<(crate::TargetBoxDvar, String)>,
    pub(crate) loading: bool,
    pub(crate) precached: BTreeMap<(&'static str, String), i32>,
    pub(crate) presented: BTreeMap<&'static str, Vec<Value>>,
    pub(crate) unsupported: BTreeMap<&'static str, u64>,
    pub(crate) budget: usize,
    pub(crate) suspended: Vec<u64>,
    pub(crate) suspended_frames: usize,
    /// Endons that fired on a suspended thread; applied when its child yields.
    pub(crate) pending_unwinds: Vec<(u64, usize)>,
    pub(crate) entities: BTreeMap<u64, host::entities::ScriptEntity>,
    pub(crate) hud_slots: BTreeMap<u64, usize>,
    pub(crate) next_entity_number: i32,
    pub(crate) tables: Arc<BTreeMap<String, StringTable>>,
    pub(crate) rng: u32,
    pub(crate) pending_notifies: Vec<(Value, Arc<str>, Vec<Value>)>,
    pub(crate) signals: Vec<Arc<str>>,
    pub(crate) engine: host::entities::EngineState,
    pub(crate) players: BTreeMap<u32, host::players::PlayerSlot>,
    pub(crate) menu_answers: BTreeMap<u32, VecDeque<host::players::MenuAnswer>>,
    pub(crate) selected_classes: BTreeMap<u32, u32>,
    pub(crate) personal_classes: BTreeMap<(u32, u32), crate::ClassDef>,
    pub(crate) weapon_bridge: BTreeMap<u32, Vec<(u32, u32)>>,
    pub(crate) thrown_insertions: BTreeMap<u32, (Arc<str>, u64)>,
    pub(crate) insertion_spots: Vec<([f32; 3], Arc<str>)>,
    pub(crate) disconnects: std::collections::BTreeSet<u32>,
    pub(crate) kicks: BTreeMap<u32, String>,
    pub(crate) joined: std::collections::BTreeSet<u32>,
    pub(crate) current_hit: Option<crate::script_player::Hit>,
    pub(crate) deaths: VecDeque<(u32, &'static str, Vec<Value>)>,
    pub(crate) exit_level: bool,
    pub(crate) shown: BTreeMap<u64, host::presence::Shown>,
    pub(crate) retired_presence: Vec<(crate::ScriptModelId, bool)>,
    pub(crate) next_spawned_presence: u32,
    pub(crate) blasts: Vec<host::entity_damage::ScriptBlast>,
    pub(crate) hits: Vec<host::entity_damage::ScriptHit>,
    pub(crate) use_held: std::collections::BTreeSet<u32>,
    pub(crate) fired_once: std::collections::BTreeSet<u64>,
    pub(crate) require_look_at: std::collections::BTreeSet<u64>,
    pub(crate) server_info: std::collections::BTreeSet<String>,
    pub(crate) missiles: BTreeMap<crate::ProjectileId, u64>,
    pub(crate) missiles_seen_ms: i32,
    pub(crate) grenade_touches: Vec<host::triggers::GrenadeTouch>,
    pub(crate) lingering: Vec<(i64, u64)>,
    pub(crate) pending_deletes: Vec<u64>,
    pub(crate) vehicles: BTreeMap<u64, host::vehicles::Heli>,
    pub(crate) planes: BTreeMap<u64, host::vehicles::Plane>,
    pub(crate) use_selected: BTreeMap<u32, u64>,
    pub(crate) t5: host::natives::t5::T5State,
    pub(crate) restart: Option<Arc<host::restart::RestartPlan>>,
    pub(crate) finished: bool,
    pub(crate) pending_restart: Option<bool>,
    pub(crate) restored_pers: BTreeMap<u32, host::restart::Detached>,
}

impl Runtime {
    pub(crate) fn program_fingerprint(&self) -> Option<[u8; 32]> {
        self.program.as_ref().map(|program| program.fingerprint())
    }

    pub(crate) fn live(&self, id: &u64) -> bool {
        self.objects.contains_key(id) && !self.dead.contains(id)
    }

    pub(crate) fn symbol(&mut self, name: &str) -> u32 {
        let program = self.program.as_ref().unwrap();
        if let Some(&id) = program.symbol_ids.get(name) {
            return id;
        }
        let next = (program.symbols.len() + self.dynamic_symbols.len()) as u32;
        *self.dynamic_symbols.entry(name.into()).or_insert(next)
    }
}
