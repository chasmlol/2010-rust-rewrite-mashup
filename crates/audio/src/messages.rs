use bevy::prelude::*;

use asset_core::AssetNamespace;

pub const SND_ENT_LOCAL: u32 = u32::MAX;

#[derive(Clone, Debug)]
pub struct PlayAlias {
    pub event: Option<crate::AudioEvent>,
    pub namespace: AssetNamespace,
    pub alias: String,

    pub fallback: Option<String>,
    pub origin_inches: Option<[f32; 3]>,
    pub snd_ent: Option<u32>,
}

#[derive(Message, Clone, Debug)]
pub enum AliasCommand {
    Play(PlayAlias),
    PlayPitched {
        sound: PlayAlias,
        pitch: f32,
    },
    StopEntity {
        snd_ent: u32,
    },
    Stop {
        namespace: AssetNamespace,
        alias: String,
        snd_ent: Option<u32>,
    },
}

#[derive(Message, Clone, Debug)]
pub struct Footstep {
    pub event: Option<crate::AudioEvent>,
    pub alias: &'static str,
    pub fallback: &'static str,
    pub origin_inches: Option<[f32; 3]>,
    pub snd_ent: Option<u32>,
}

#[derive(Message, Clone, Debug)]
pub struct WeaponSound {
    pub event: Option<crate::AudioEvent>,
    pub namespace: AssetNamespace,
    pub alias: String,
    pub origin_inches: Option<[f32; 3]>,
    pub snd_ent: Option<u32>,
}

#[derive(Message, Clone, Debug)]
pub struct BoundWeaponSound {
    pub event: Option<crate::AudioEvent>,
    pub bank_revision: u64,
    pub index: usize,
    pub origin_inches: Option<[f32; 3]>,
    pub snd_ent: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct ViewmodelNotetrack {
    pub event: crate::AudioEvent,
    pub name: String,
}

#[derive(Message, Clone, Debug)]
pub struct ViewmodelNotetracks {
    pub generation: frame::WorldGeneration,
    pub client: sim::ClientId,
    pub life: sim::LifeSequence,
    pub weapon: u32,
    pub records: Vec<ViewmodelNotetrack>,
    pub discarded: u64,
}

#[derive(Message, Clone, Debug)]
pub struct LandSound {
    pub event: Option<crate::AudioEvent>,
    pub alias: &'static str,
    pub fallback: &'static str,
    pub origin_inches: Option<[f32; 3]>,
    pub snd_ent: Option<u32>,
}

pub fn ent_from_number(number: i32) -> Option<u32> {
    u32::try_from(number).ok()
}
