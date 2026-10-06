use bevy::prelude::*;
use sim::ClientId;

use crate::transport::wire::{WireError, WireReader, WireWriter};

pub const SVC_CARD_SLOT: u8 = b'K';

pub const SVC_OPEN_MENU: u8 = b'u';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SvcCardSlot {
    pub client: u32,
    pub slot: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SvcOpenMenu {
    pub cs_index: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PendingKind {
    CardSlot {
        source: ClientId,
        slot: i32,
    },
    OpenMenu {
        cs_index: i32,
    },
    Splash {
        key: String,
        slot: i32,
        optional: i32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingRow {
    recipient: ClientId,
    kind: PendingKind,
}

#[derive(Resource, Debug, Default)]
pub struct PendingPlayerCard {
    rows: Vec<PendingRow>,

    pub stranded: u64,
}

impl PendingPlayerCard {
    pub fn push_slot(&mut self, recipient: ClientId, source: ClientId, slot: i32) {
        self.rows.push(PendingRow {
            recipient,
            kind: PendingKind::CardSlot { source, slot },
        });
    }

    pub fn push_open(&mut self, recipient: ClientId, cs_index: i32) {
        self.rows.push(PendingRow {
            recipient,
            kind: PendingKind::OpenMenu { cs_index },
        });
    }

    pub fn adopt_events(&mut self, effects: &[sim::PendingPlayerCardEvent]) {
        for ev in effects.iter().cloned() {
            match ev.kind {
                sim::PendingPlayerCardKind::SetSlot { source, slot } => {
                    self.push_slot(ev.recipient, source, slot);
                }
                sim::PendingPlayerCardKind::Splash {
                    key,
                    slot,
                    optional,
                } => {
                    self.rows.push(PendingRow {
                        recipient: ev.recipient,
                        kind: PendingKind::Splash {
                            key,
                            slot,
                            optional,
                        },
                    });
                }
                sim::PendingPlayerCardKind::OpenMenu { cs_index } => {
                    self.push_open(ev.recipient, cs_index);
                }
            }
        }
    }

    pub fn take_for(
        &mut self,
        client: ClientId,
    ) -> (Vec<SvcCardSlot>, Vec<SvcOpenMenu>, Vec<SvcHudSplash>) {
        let mut splashes = Vec::new();
        let mut slots = Vec::new();
        let mut menus = Vec::new();
        self.rows.retain(|row| {
            if row.recipient != client {
                return true;
            }
            match &row.kind {
                PendingKind::Splash {
                    key,
                    slot,
                    optional,
                } => {
                    splashes.push(SvcHudSplash {
                        key: key.clone(),
                        slot: *slot,
                        optional: *optional,
                    });
                }
                PendingKind::CardSlot { source, slot } => {
                    slots.push(SvcCardSlot {
                        client: source.0,
                        slot: *slot,
                    });
                }
                PendingKind::OpenMenu { cs_index } => {
                    menus.push(SvcOpenMenu {
                        cs_index: *cs_index,
                    });
                }
            }
            false
        });
        (slots, menus, splashes)
    }

    pub fn drain_stranded(&mut self) -> usize {
        let leftover = core::mem::take(&mut self.rows);
        let n = leftover.len();
        self.stranded = self.stranded.saturating_add(n as u64);
        n
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SvcCardSlotCmd {
    pub client: u32,
    pub slot: i32,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SvcOpenMenuCmd {
    pub cs_index: i32,
}

pub fn encode_svc_card_slots(out: &mut WireWriter, commands: &[SvcCardSlot]) {
    debug_assert!(commands.len() <= u8::MAX as usize);
    out.put_u8(commands.len() as u8);
    for cmd in commands {
        out.put_u32(cmd.client);
        out.put_i32(cmd.slot);
    }
}

pub fn decode_svc_card_slots(input: &mut WireReader<'_>) -> Result<Vec<SvcCardSlot>, WireError> {
    let count = input.get_u8()? as usize;
    let mut commands = Vec::with_capacity(count);
    for _ in 0..count {
        commands.push(SvcCardSlot {
            client: input.get_u32()?,
            slot: input.get_i32()?,
        });
    }
    Ok(commands)
}

pub fn encode_svc_open_menus(out: &mut WireWriter, commands: &[SvcOpenMenu]) {
    debug_assert!(commands.len() <= u8::MAX as usize);
    out.put_u8(commands.len() as u8);
    for cmd in commands {
        out.put_i32(cmd.cs_index);
    }
}

pub fn decode_svc_open_menus(input: &mut WireReader<'_>) -> Result<Vec<SvcOpenMenu>, WireError> {
    let count = input.get_u8()? as usize;
    let mut commands = Vec::with_capacity(count);
    for _ in 0..count {
        commands.push(SvcOpenMenu {
            cs_index: input.get_i32()?,
        });
    }
    Ok(commands)
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct SvcHudSplash {
    pub key: String,
    pub slot: i32,
    pub optional: i32,
}

pub fn encode_svc_hud_splashes(out: &mut WireWriter, commands: &[SvcHudSplash]) {
    out.put_u16(u16::try_from(commands.len()).expect("HUD splash count"));
    for cmd in commands {
        out.put_u16(u16::try_from(cmd.key.len()).expect("HUD splash key length"));
        out.put_bytes(cmd.key.as_bytes());
        out.put_i32(cmd.slot);
        out.put_i32(cmd.optional);
    }
}
pub fn decode_svc_hud_splashes(input: &mut WireReader<'_>) -> Result<Vec<SvcHudSplash>, WireError> {
    let count = input.get_u16()?;
    let mut commands = Vec::new();
    for _ in 0..count {
        let mut key = vec![0; input.get_u16()? as usize];
        input.get_bytes(&mut key)?;
        let key =
            String::from_utf8(key).map_err(|_| WireError::Malformed("HUD splash key UTF-8"))?;
        let slot = input.get_i32()?;
        if !(0..5).contains(&slot) {
            return Err(WireError::Malformed("HUD splash slot"));
        }
        commands.push(SvcHudSplash {
            key,
            slot,
            optional: input.get_i32()?,
        });
    }
    Ok(commands)
}
