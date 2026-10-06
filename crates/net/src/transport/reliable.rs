use sim::{ActionRequestId, SimEvent};

use crate::transport::meta_wire::{decode_event, encode_event};
use crate::transport::wire::{WireError, WireReader, WireWriter};

pub const MAX_PENDING_RELIABLE: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionVerdict {
    Applied,

    Accepted,

    Refused,

    PayloadMismatch,

    Expired,
}

impl ActionVerdict {
    const fn tag(self) -> u8 {
        match self {
            Self::Applied => 0,
            Self::Accepted => 4,
            Self::Refused => 1,
            Self::PayloadMismatch => 2,
            Self::Expired => 3,
        }
    }

    const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Applied),
            4 => Some(Self::Accepted),
            1 => Some(Self::Refused),
            2 => Some(Self::PayloadMismatch),
            3 => Some(Self::Expired),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ReliableRow {
    Failure(String),
    Sound(crate::SvcSound),
    ScriptAudio(sim::ScriptAudioCommand),
    Card(crate::SvcCardSlot),
    Menu(crate::SvcOpenMenu),
    Splash(crate::SvcHudSplash),
    Notify(crate::SvcGameNotify),
    Scores(String),

    Event(SimEvent),
    FireCommands(Vec<sim::FireCommandResult>),

    ActionOutcome {
        request_id: ActionRequestId,
        verdict: ActionVerdict,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReliableEventQueue {
    next_seq: u16,

    ack_through: u16,

    pending: Vec<(u16, ReliableRow)>,

    pub dropped_oldest: u32,
}

impl ReliableEventQueue {
    pub fn new() -> Self {
        Self {
            next_seq: 1,
            ..Self::default()
        }
    }

    pub fn push(&mut self, row: ReliableRow) -> u16 {
        if self.pending.len() >= MAX_PENDING_RELIABLE {
            self.dropped_oldest = self.dropped_oldest.saturating_add(1);
            return 0;
        }
        if self.next_seq == 0 {
            self.next_seq = 1;
        }
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        self.pending.push((seq, row));
        seq
    }

    pub fn push_event(&mut self, event: SimEvent) -> u16 {
        self.push(ReliableRow::Event(event))
    }

    pub fn push_outcome(&mut self, request_id: ActionRequestId, verdict: ActionVerdict) -> u16 {
        self.push(ReliableRow::ActionOutcome {
            request_id,
            verdict,
        })
    }

    pub fn ack(&mut self, through: u16) {
        if through == 0 {
            return;
        }
        if seq_after(through, self.ack_through) {
            self.ack_through = through;
        }
        let keep = self.ack_through;
        self.pending.retain(|(seq, _)| seq_after(*seq, keep));
    }

    pub fn ack_through(&self) -> u16 {
        self.ack_through
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub fn pending(&self) -> &[(u16, ReliableRow)] {
        &self.pending
    }

    pub fn payload(&self) -> ReliablePayload {
        ReliablePayload {
            ack_through: self.ack_through,
            rows: self.pending.clone(),
            dropped_oldest: self.dropped_oldest,
        }
    }
}

pub(crate) fn seq_after(a: u16, b: u16) -> bool {
    a != b && a.wrapping_sub(b) < 0x8000
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReliablePayload {
    pub ack_through: u16,
    pub rows: Vec<(u16, ReliableRow)>,
    pub dropped_oldest: u32,
}

impl ReliablePayload {
    pub fn owes_nothing() -> Self {
        Self {
            ack_through: 0,
            rows: Vec::new(),
            dropped_oldest: 0,
        }
    }
}

const ROW_TAG_EVENT: u8 = 0;
const ROW_TAG_OUTCOME: u8 = 1;

pub fn encode_reliable_payload(
    out: &mut WireWriter,
    ack_through: u16,
    rows: &[(u16, ReliableRow)],
    dropped_oldest: u32,
) {
    debug_assert!(rows.len() <= u16::MAX as usize);
    out.put_u16(ack_through);
    out.put_u16(rows.len() as u16);
    for (seq, row) in rows {
        out.put_u16(*seq);
        match row {
            ReliableRow::FireCommands(results) => {
                out.put_u8(10);
                super::fire_result::encode_fire_results(out, results);
            }
            ReliableRow::ScriptAudio(value) => {
                out.put_u8(9);
                crate::svc_script_audio::encode_script_audio(out, value);
            }
            ReliableRow::Sound(value) => {
                out.put_u8(2);
                crate::svc_sound::encode_svc_sounds(out, std::slice::from_ref(value));
            }
            ReliableRow::Card(value) => {
                out.put_u8(3);
                crate::svc_playercard::encode_svc_card_slots(out, std::slice::from_ref(value));
            }
            ReliableRow::Menu(value) => {
                out.put_u8(4);
                crate::svc_playercard::encode_svc_open_menus(out, std::slice::from_ref(value));
            }
            ReliableRow::Splash(value) => {
                out.put_u8(5);
                crate::svc_playercard::encode_svc_hud_splashes(out, std::slice::from_ref(value));
            }
            ReliableRow::Notify(value) => {
                out.put_u8(6);
                crate::svc_gamenotify::encode_svc_game_notifies(out, std::slice::from_ref(value));
            }
            ReliableRow::Failure(value) => {
                out.put_u8(8);
                crate::svc_scores::encode_svc_scores(out, Some(value));
            }
            ReliableRow::Scores(value) => {
                out.put_u8(7);
                crate::svc_scores::encode_svc_scores(out, Some(value));
            }
            ReliableRow::Event(event) => {
                out.put_u8(ROW_TAG_EVENT);
                encode_event(out, event);
            }
            ReliableRow::ActionOutcome {
                request_id,
                verdict,
            } => {
                out.put_u8(ROW_TAG_OUTCOME);
                out.put_u32(*request_id);
                out.put_u8(verdict.tag());
            }
        }
    }
    out.put_u32(dropped_oldest);
}

pub fn decode_reliable_payload(input: &mut WireReader<'_>) -> Result<ReliablePayload, WireError> {
    let ack_through = input.get_u16()?;
    let count = input.get_u16()? as usize;
    if count > MAX_PENDING_RELIABLE {
        return Err(WireError::Malformed("reliable row capacity"));
    }
    let mut rows = Vec::with_capacity(count.min(MAX_PENDING_RELIABLE));
    for _ in 0..count {
        let seq = input.get_u16()?;
        let row = match input.get_u8()? {
            10 => ReliableRow::FireCommands(super::fire_result::decode_fire_results(input)?),
            ROW_TAG_EVENT => ReliableRow::Event(decode_event(input)?),
            ROW_TAG_OUTCOME => {
                let request_id = input.get_u32()?;
                let verdict = ActionVerdict::from_tag(input.get_u8()?)
                    .ok_or(WireError::Malformed("unknown ActionVerdict tag"))?;
                ReliableRow::ActionOutcome {
                    request_id,
                    verdict,
                }
            }
            9 => ReliableRow::ScriptAudio(crate::svc_script_audio::decode_script_audio(input)?),
            2 => {
                let mut values = crate::svc_sound::decode_svc_sounds(input)?;
                if values.len() != 1 {
                    return Err(WireError::Malformed("control row count"));
                }
                ReliableRow::Sound(values.remove(0))
            }
            3 => {
                let mut values = crate::svc_playercard::decode_svc_card_slots(input)?;
                if values.len() != 1 {
                    return Err(WireError::Malformed("control row count"));
                }
                ReliableRow::Card(values.remove(0))
            }
            4 => {
                let mut values = crate::svc_playercard::decode_svc_open_menus(input)?;
                if values.len() != 1 {
                    return Err(WireError::Malformed("control row count"));
                }
                ReliableRow::Menu(values.remove(0))
            }
            5 => {
                let mut values = crate::svc_playercard::decode_svc_hud_splashes(input)?;
                if values.len() != 1 {
                    return Err(WireError::Malformed("control row count"));
                }
                ReliableRow::Splash(values.remove(0))
            }
            6 => {
                let mut values = crate::svc_gamenotify::decode_svc_game_notifies(input)?;
                if values.len() != 1 {
                    return Err(WireError::Malformed("control row count"));
                }
                ReliableRow::Notify(values.remove(0))
            }
            8 => ReliableRow::Failure(
                crate::svc_scores::decode_svc_scores(input)?
                    .ok_or(WireError::Malformed("missing failure"))?,
            ),
            7 => ReliableRow::Scores(
                crate::svc_scores::decode_svc_scores(input)?
                    .ok_or(WireError::Malformed("missing scores"))?,
            ),
            _ => return Err(WireError::Malformed("unknown ReliableRow tag")),
        };
        rows.push((seq, row));
    }
    let dropped_oldest = input.get_u32()?;
    Ok(ReliablePayload {
        ack_through,
        rows,
        dropped_oldest,
    })
}

#[derive(bevy::prelude::Resource, Clone, Debug, Default)]
pub struct ReliableEventHub {
    queues: std::collections::HashMap<sim::ClientId, ReliableEventQueue>,
}

impl ReliableEventHub {
    pub fn queue(&self, client: sim::ClientId) -> Option<&ReliableEventQueue> {
        self.queues.get(&client)
    }

    pub fn queue_mut(&mut self, client: sim::ClientId) -> &mut ReliableEventQueue {
        self.queues
            .entry(client)
            .or_insert_with(ReliableEventQueue::new)
    }

    pub fn push_event(&mut self, client: sim::ClientId, event: SimEvent) -> u16 {
        self.queue_mut(client).push_event(event)
    }

    pub fn push_outcome(
        &mut self,
        client: sim::ClientId,
        request_id: ActionRequestId,
        verdict: ActionVerdict,
    ) -> u16 {
        let queue = self.queue_mut(client);
        if let Some((seq, _)) = queue.pending().iter().find(|(_, row)| {
            *row == ReliableRow::ActionOutcome {
                request_id,
                verdict,
            }
        }) {
            return *seq;
        }
        queue.push_outcome(request_id, verdict)
    }

    pub fn payload(&self, client: sim::ClientId) -> ReliablePayload {
        self.queues
            .get(&client)
            .map_or_else(ReliablePayload::owes_nothing, ReliableEventQueue::payload)
    }

    pub fn ack(&mut self, client: sim::ClientId, through: u16) {
        if let Some(queue) = self.queues.get_mut(&client) {
            queue.ack(through);
        }
    }

    // For a client with no wire that will ever read this queue and ack it
    // back (a host-driven bot), draining it here is the only ack it gets.
    // Left unacked, broadcast events (deaths, hit markers, ...) pile up past
    // `MAX_PENDING_RELIABLE` and the peer looks like an overflowed connection
    // — which gets it retired as if it had disconnected.
    pub fn ack_all(&mut self, client: sim::ClientId) {
        if let Some(queue) = self.queues.get_mut(&client)
            && let Some(&(seq, _)) = queue.pending().last()
        {
            queue.ack(seq);
        }
    }

    pub fn retire(&mut self, client: sim::ClientId) {
        self.queues.remove(&client);
    }

    pub fn clear(&mut self) {
        self.queues.clear();
    }

    pub fn overflowed_clients(&self) -> impl Iterator<Item = sim::ClientId> + '_ {
        self.queues
            .iter()
            .filter_map(|(client, queue)| (queue.dropped_oldest != 0).then_some(*client))
    }

    pub fn len(&self) -> usize {
        self.queues.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queues.is_empty()
    }
}
