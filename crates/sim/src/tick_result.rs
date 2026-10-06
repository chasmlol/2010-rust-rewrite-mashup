use std::sync::Arc;

use crate::{ClientAction, ClientId, Snapshot, TickInput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionOutcome {
    Applied,
    Accepted,
    Refused,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActionResult {
    pub client: ClientId,
    pub action: ClientAction,
    pub outcome: ActionOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireCommandRefusal {
    MatchInactive,
    NotAlive,
    MissingPlayer,
    StaleCommand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireCommandOutcome {
    NotRun(FireCommandRefusal),
    Executed {
        accepted: [Option<crate::FireCause>; 2],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FireCommandResult {
    pub client: ClientId,
    pub life: Option<crate::LifeSequence>,
    pub command: crate::CommandSequence,
    pub outcome: FireCommandOutcome,
}

#[derive(Clone, Debug, Default)]
pub struct TickEffects {
    pub prints: Vec<crate::PendingPrint>,
    pub local_sounds: Vec<crate::PendingLocalSound>,
    pub player_cards: Vec<crate::PendingPlayerCardEvent>,
    pub script_audio: Vec<crate::ScriptAudioCommand>,
    pub kicks: Vec<(ClientId, String)>,
}

#[derive(Clone, Debug)]
pub struct TickResult {
    pub input: TickInput,
    pub snapshot: Snapshot,
    pub action_results: Vec<ActionResult>,
    pub fire_results: Vec<FireCommandResult>,
    pub effects: TickEffects,
    pub weapon_script_names: Arc<[String]>,
    pub pending_final_kill: Option<(ClientId, ClientId)>,
    pub script_seats: Vec<(ClientId, crate::ScriptSeat)>,
    pub script_exit_level: bool,
}
