use bevy::prelude::*;
use sim::ClientId;

use crate::transport::wire::{WireError, WireReader, WireWriter};

pub const SVC_PLAY_LOCAL: u8 = 0x70;

pub const SVC_STOP_LOCAL: u8 = 0x68;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SvcSound {
    pub stop: bool,

    pub index: u8,
}

impl SvcSound {
    pub fn play(index: u8) -> Self {
        Self { stop: false, index }
    }

    pub fn stop(index: u8) -> Self {
        Self { stop: true, index }
    }

    pub fn tag(self) -> u8 {
        if self.stop {
            SVC_STOP_LOCAL
        } else {
            SVC_PLAY_LOCAL
        }
    }

    pub fn from_tag(tag: u8, index: u8) -> Result<Self, WireError> {
        match tag {
            SVC_PLAY_LOCAL => Ok(Self::play(index)),
            SVC_STOP_LOCAL => Ok(Self::stop(index)),
            _ => Err(WireError::Malformed("svc sound tag is not 'p' or 'h'")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingAlias {
    client: ClientId,
    stop: bool,
    alias: String,
}

#[derive(Resource, Debug, Default)]
pub struct PendingSvcSounds {
    queued: Vec<PendingAlias>,
    indexed: Vec<(ClientId, SvcSound)>,

    pub dropped_index_zero: u64,

    pub stranded: u64,

    pub last_queued_alias: Option<String>,

    pub last_index: u8,

    pub last_dropped_alias: Option<String>,
}

impl PendingSvcSounds {
    pub fn push_alias(&mut self, client: ClientId, stop: bool, alias: impl Into<String>) {
        let alias = alias.into();
        self.last_queued_alias = Some(alias.clone());
        self.queued.push(PendingAlias {
            client,
            stop,
            alias,
        });
    }

    pub fn push_alias_u32(&mut self, client: u32, stop: bool, alias: impl Into<String>) {
        self.push_alias(ClientId(client), stop, alias);
    }

    pub fn occupy_cs(&mut self, world: &mut sim::SimWorld) {
        for cmd in self.queued.drain(..) {
            let index = world.sound_alias_index(&cmd.alias);
            if index == 0 {
                self.dropped_index_zero = self.dropped_index_zero.saturating_add(1);
                self.last_dropped_alias = Some(cmd.alias);
                continue;
            }
            self.last_index = index;
            let svc = if cmd.stop {
                SvcSound::stop(index)
            } else {
                SvcSound::play(index)
            };
            self.indexed.push((cmd.client, svc));
        }
    }

    pub fn adopt_sounds(&mut self, effects: &[sim::PendingLocalSound]) {
        for sound in effects.iter().cloned() {
            if sound.alias_index == 0 {
                self.dropped_index_zero = self.dropped_index_zero.saturating_add(1);
                continue;
            }
            let svc = if sound.stop {
                SvcSound::stop(sound.alias_index)
            } else {
                SvcSound::play(sound.alias_index)
            };
            self.indexed.push((sound.recipient, svc));
        }
    }

    pub fn take_for(&mut self, client: ClientId) -> Vec<SvcSound> {
        let mut out = Vec::new();
        self.indexed.retain(|(id, cmd)| {
            if *id == client {
                out.push(*cmd);
                false
            } else {
                true
            }
        });
        out
    }

    pub fn drain_stranded(&mut self) -> Vec<(ClientId, SvcSound)> {
        let leftover = core::mem::take(&mut self.indexed);
        self.stranded = self.stranded.saturating_add(leftover.len() as u64);
        leftover
    }

    pub fn is_empty(&self) -> bool {
        self.queued.is_empty() && self.indexed.is_empty()
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SvcLocalSound {
    pub stop: bool,
    pub index: u8,
}

pub fn encode_svc_sounds(out: &mut WireWriter, commands: &[SvcSound]) {
    debug_assert!(commands.len() <= u8::MAX as usize);
    out.put_u8(commands.len() as u8);
    for cmd in commands {
        out.put_u8(cmd.tag());
        out.put_u8(cmd.index);
    }
}

pub fn decode_svc_sounds(input: &mut WireReader<'_>) -> Result<Vec<SvcSound>, WireError> {
    let count = input.get_u8()? as usize;
    let mut commands = Vec::with_capacity(count);
    for _ in 0..count {
        let tag = input.get_u8()?;
        let index = input.get_u8()?;
        commands.push(SvcSound::from_tag(tag, index)?);
    }
    Ok(commands)
}
