use bevy::prelude::Resource;
use sim::{ClientId, LifeSequence};

use crate::controller::HostController;

pub const MAX_HOST_BOTS: u32 = 20;

const BOT_NAMES: [&str; 52] = [
    "contextrot",
    "t.me/contextrot",
    "mossy.exe",
    "xXAfterglowXx",
    "dad_on_wifi",
    "soupEnjoyer",
    "k1netic",
    "lastSlice",
    "n0vaKid",
    "grape_soda",
    "sleepyJules",
    "pixelPete",
    "cereal4dinner",
    "altTabAndy",
    "bennyFPS",
    "duckOnDeck",
    "mango_juice",
    "spareMouse",
    "justMika",
    "oatmilk97",
    "t0ast",
    "LowBattery",
    "lilOrbit",
    "raincheck_",
    "ctrlZed",
    "frostbyte88",
    "[LAN]lukas",
    "snackDealer",
    "whiffWizard",
    "bluejay_",
    "NotNowNate",
    "laggyLemon",
    "heyitsEm",
    "VelvetRush",
    "c0ffeeBreak",
    "tinyMonitor",
    "zippy_42",
    "oneMoreRound",
    "roofcat",
    "DizzyDylan",
    "pocketLint",
    "SundayCasual",
    "Ranger",
    "Crash",
    "Visor",
    "Ariel",
    "Nova",
    "Cipher",
    "Phantom",
    "Wyatt",
    "Toby",
    "Gryphon",
];

#[derive(Resource, Debug, Default)]
pub struct BotAddQueue(pub Vec<BotAddRequest>);

#[derive(Debug)]
pub struct BotAddRequest {
    pub count: u32,
    pub dummy: bool,
}

impl BotAddQueue {
    pub fn push(&mut self, count: u32) {
        self.0.push(BotAddRequest {
            count: count.max(1),
            dummy: false,
        });
    }

    pub fn push_dummy(&mut self, count: u32) {
        self.0.push(BotAddRequest {
            count: count.max(1),
            dummy: true,
        });
    }

    pub fn drain(&mut self) -> Vec<BotAddRequest> {
        core::mem::take(&mut self.0)
    }
}

#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct BotHold(pub bool);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BotTpTarget {
    All,
    Id(ClientId),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BotTpWhere {
    Absolute {
        origin: [f32; 3],
        yaw: Option<f32>,
        pitch: Option<f32>,
    },

    Above {
        height: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BotTpRequest {
    pub target: BotTpTarget,
    pub where_: BotTpWhere,
}

#[derive(Resource, Debug, Default)]
pub struct BotTpQueue(pub Vec<BotTpRequest>);

impl BotTpQueue {
    pub fn push(&mut self, request: BotTpRequest) {
        self.0.push(request);
    }

    pub fn drain(&mut self) -> Vec<BotTpRequest> {
        core::mem::take(&mut self.0)
    }
}

#[derive(Resource, Debug, Default)]
pub struct BotFireQueue(pub Vec<BotTpTarget>);

impl BotFireQueue {
    pub fn push(&mut self, target: BotTpTarget) {
        self.0.push(target);
    }

    pub fn drain(&mut self) -> Vec<BotTpTarget> {
        core::mem::take(&mut self.0)
    }
}

#[derive(Debug)]
pub struct BotSlot {
    pub id: ClientId,
    pub name: &'static str,
    pub account: sim::AccountId,
    pub brain: Option<HostController>,
    pub joined: bool,
    pub class_picked_in: Option<LifeSequence>,
    pub class_picks: u32,
}

#[derive(Resource, Debug)]
pub struct BotRoster {
    pub bots: Vec<BotSlot>,
    pub next_client: u32,
    pub seed: u64,
}

impl Default for BotRoster {
    fn default() -> Self {
        Self {
            bots: Vec::new(),

            next_client: 1,
            seed: 0xb075_0001,
        }
    }
}

impl BotRoster {
    pub fn is_bot(&self, id: ClientId) -> bool {
        self.bots.iter().any(|bot| bot.id == id)
    }

    // `taken` are the ids real clients already own. A bot minted onto one of
    // them *is* that client as far as the roster is concerned: `is_bot` claims
    // the player, and the slot is dead weight because no system can drive an
    // id someone else is already playing.
    pub fn add_bots(&mut self, count: u32, taken: &[ClientId], dummy: bool) -> Vec<ClientId> {
        let room = MAX_HOST_BOTS.saturating_sub(self.bots.len() as u32);
        let count = count.min(room);
        let seed = self.seed;
        let mut added = Vec::with_capacity(count as usize);
        for i in 0..count {
            let Some(id) = self.claim_id(taken) else {
                break;
            };
            let mut account = [0; 16];
            while account == [0; 16] {
                if let Err(error) = getrandom::fill(&mut account) {
                    diag::warn!(Sim, "bots: account identity creation failed: {error}");
                    return added;
                }
            }
            let brain = (!dummy)
                .then(|| HostController::new(seed ^ (u64::from(id.0) << 32) ^ u64::from(i)));
            let available: Vec<_> = BOT_NAMES
                .iter()
                .copied()
                .filter(|name| !self.bots.iter().any(|bot| bot.name == *name))
                .collect();
            let random = u64::from_le_bytes(account[..8].try_into().unwrap());
            let name = available[(random % available.len() as u64) as usize];
            self.bots.push(BotSlot {
                id,
                name,
                account: sim::AccountId(account),
                brain,
                joined: false,
                class_picked_in: None,
                class_picks: 0,
            });
            added.push(id);
        }
        added
    }

    fn claim_id(&mut self, taken: &[ClientId]) -> Option<ClientId> {
        // Only `bots + taken` ids are spoken for, so one candidate more than
        // that always turns up a free one.
        let candidates = self
            .bots
            .len()
            .saturating_add(taken.len())
            .saturating_add(1);
        for _ in 0..candidates {
            let id = ClientId(self.next_client);
            self.next_client = self.next_client.wrapping_add(1).max(1);
            if !taken.contains(&id) && !self.is_bot(id) {
                return Some(id);
            }
        }
        None
    }
}

pub fn default_class_index(seed: u64, client: ClientId, pick: u32) -> u8 {
    let mix = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(u64::from(client.0).wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add(u64::from(pick).wrapping_mul(0x94D0_49BB_1331_11EB));
    (mix >> 33) as u8
}
