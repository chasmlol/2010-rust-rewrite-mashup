use playerstate_iw4::UserCmd;

use crate::identities::MatchPhase;
use crate::world::ClientId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ClassId(pub u32);

pub type ActionRequestId = u32;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClientAction {
    JoinMatch {
        request_id: ActionRequestId,
    },

    LeaveMatch {
        request_id: ActionRequestId,
    },

    SelectClass {
        request_id: ActionRequestId,
        class_id: ClassId,
        revision: u32,
        loadout: crate::PersonalClass,
    },

    GiveWeapon {
        request_id: ActionRequestId,
        weapon: u32,
        model: u8,
    },

    ChangeWeaponConfiguration {
        request_id: ActionRequestId,
        from: u32,
        to: u32,
    },

    ForceDeath {
        request_id: ActionRequestId,
    },

    SpawnClient {
        request_id: ActionRequestId,
    },

    ForceSpawn {
        request_id: ActionRequestId,
        pick: SpawnPick,
    },

    SpawnIntermission {
        request_id: ActionRequestId,
    },

    SetMatchPhase {
        request_id: ActionRequestId,
        phase: MatchPhase,
    },

    Move {
        request_id: ActionRequestId,
        origin: [f32; 3],
        angles: [f32; 3],
    },

    BeginScriptMoverRotateVelocity {
        request_id: ActionRequestId,
        speed: f32,
    },

    ToggleGod {
        request_id: ActionRequestId,
    },

    DebugDamage {
        request_id: ActionRequestId,
        amount: i32,
    },

    SetProfile {
        request_id: ActionRequestId,
        profile: PlayerProfile,
    },

    SetName {
        request_id: ActionRequestId,
        name: [u8; 16],
    },

    UseCopycat {
        request_id: ActionRequestId,
    },

    ActionSlot {
        request_id: ActionRequestId,
        slot: u8,
    },

    ChooseDefaultClass {
        request_id: ActionRequestId,
        index: u8,
    },

    MenuResponse {
        request_id: ActionRequestId,
        menu: [u8; MENU_RESPONSE_BYTES],
        response: [u8; MENU_RESPONSE_BYTES],
    },

    ResupplyAmmo {
        request_id: ActionRequestId,
    },

    GiveKillstreak {
        request_id: ActionRequestId,
        name: [u8; MENU_RESPONSE_BYTES],
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerProfile {
    pub title: u32,
    pub emblem: u32,
    pub killstreaks: [u32; 3],
}

pub const MENU_RESPONSE_BYTES: usize = 48;

pub fn menu_response_field(text: &str) -> Option<[u8; MENU_RESPONSE_BYTES]> {
    let bytes = text.as_bytes();
    if bytes.len() > MENU_RESPONSE_BYTES {
        return None;
    }
    let mut field = [0u8; MENU_RESPONSE_BYTES];
    field[..bytes.len()].copy_from_slice(bytes);
    Some(field)
}

pub fn menu_response_text(field: &[u8; MENU_RESPONSE_BYTES]) -> &str {
    let len = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    std::str::from_utf8(&field[..len]).unwrap_or("")
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpawnPick {
    Seeded(u64),
    At { origin: [f32; 3], yaw: f32 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommandSequence(pub u32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerCommand {
    pub client: ClientId,
    pub command: UserCmd,
    pub sequence: Option<CommandSequence>,
}

impl PlayerCommand {
    pub fn unsequenced(client: ClientId, command: UserCmd) -> Self {
        Self {
            client,
            command,
            sequence: None,
        }
    }

    pub fn sequenced(client: ClientId, command: UserCmd, sequence: CommandSequence) -> Self {
        Self {
            client,
            command,
            sequence: Some(sequence),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TickInput {
    pub cmds: Vec<PlayerCommand>,
    pub actions: Vec<(ClientId, ClientAction)>,
    pub shot_samples: Vec<((ClientId, i32), crate::ShotSampleProvenance)>,
}

impl TickInput {
    pub fn from_cmds(cmds: Vec<(ClientId, UserCmd)>) -> Self {
        Self {
            cmds: cmds
                .into_iter()
                .map(|(client, command)| PlayerCommand::unsequenced(client, command))
                .collect(),
            actions: Vec::new(),
            shot_samples: Vec::new(),
        }
    }

    pub fn canonicalize(&mut self) {
        self.cmds.sort_by_key(|input| input.client.0);
        self.shot_samples
            .sort_by_key(|((id, time), _)| (id.0, *time));

        self.actions
            .sort_by_key(|(id, action)| (id.0, action_request_id(action)));
    }
}

pub fn action_request_id(action: &ClientAction) -> ActionRequestId {
    match *action {
        ClientAction::JoinMatch { request_id }
        | ClientAction::LeaveMatch { request_id }
        | ClientAction::SelectClass { request_id, .. }
        | ClientAction::GiveWeapon { request_id, .. }
        | ClientAction::ChangeWeaponConfiguration { request_id, .. }
        | ClientAction::ForceDeath { request_id }
        | ClientAction::SpawnClient { request_id }
        | ClientAction::ForceSpawn { request_id, .. }
        | ClientAction::SpawnIntermission { request_id }
        | ClientAction::SetMatchPhase { request_id, .. }
        | ClientAction::Move { request_id, .. }
        | ClientAction::BeginScriptMoverRotateVelocity { request_id, .. }
        | ClientAction::ToggleGod { request_id }
        | ClientAction::DebugDamage { request_id, .. }
        | ClientAction::SetName { request_id, .. }
        | ClientAction::SetProfile { request_id, .. }
        | ClientAction::UseCopycat { request_id }
        | ClientAction::ActionSlot { request_id, .. }
        | ClientAction::ChooseDefaultClass { request_id, .. }
        | ClientAction::MenuResponse { request_id, .. }
        | ClientAction::GiveKillstreak { request_id, .. }
        | ClientAction::ResupplyAmmo { request_id } => request_id,
    }
}
