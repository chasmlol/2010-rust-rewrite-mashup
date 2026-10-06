use bevy::prelude::Resource;
use playerstate_iw4::UserCmd;
use sim::{ClientId, Tick};
use std::collections::{HashMap, VecDeque};

use crate::client::predict::CmdSeq;

pub const AUTHORITY_HZ: f64 = 20.0;

pub const AUTHORITY_MS: i32 = 50;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ServerTime(i32);

impl ServerTime {
    pub const fn from_ms(ms: i32) -> Self {
        Self(ms)
    }

    pub fn from_tick(tick: Tick) -> Self {
        let ms = i32::try_from(tick.0)
            .unwrap_or(0)
            .saturating_mul(AUTHORITY_MS);
        Self(ms)
    }

    pub const fn ms(self) -> i32 {
        self.0
    }
}

pub const MAX_REDUNDANT_CMDS: usize = crate::client::predict::DEFAULT_HISTORY_CAP;

pub const MAX_QUEUED_COMMANDS_PER_PEER: usize = MAX_REDUNDANT_CMDS;

pub const MAX_QUEUED_COMMAND_MS: i32 = 1000;

pub(crate) const COMMAND_TIME_STEP_MS: i32 = 200;

pub const MAX_COMMAND_STALL_MS: i32 = 1000;

pub const MAX_COMMANDS_PER_PEER_PER_FRAME: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputBacklogFault {
    QueueCount { queued: usize },

    QueueDuration { queued_ms: i32 },

    NoProgress { stalled_ms: i32 },
}

impl core::fmt::Display for InputBacklogFault {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::QueueCount { queued } => write!(
                f,
                "InputBacklogExceeded: {queued} unique commands queued (max \
                 {MAX_QUEUED_COMMANDS_PER_PEER})"
            ),
            Self::QueueDuration { queued_ms } => write!(
                f,
                "InputBacklogExceeded: {queued_ms} ms of command time queued (max \
                 {MAX_QUEUED_COMMAND_MS})"
            ),
            Self::NoProgress { stalled_ms } => write!(
                f,
                "InputBacklogExceeded: command queue made no progress for {stalled_ms} ms (max \
                 {MAX_COMMAND_STALL_MS})"
            ),
        }
    }
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AuthorityClock {
    pub tick: u32,
    pub time_ms: i32,
}

impl AuthorityClock {
    pub fn advance(&mut self) -> u32 {
        self.time_ms = self.time_ms.wrapping_add(AUTHORITY_MS);
        self.tick = self.tick.wrapping_add(1);
        self.tick
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GatheredCommands {
    pub cmds: Vec<sim::PlayerCommand>,

    pub acks: Vec<(ClientId, CmdSeq)>,

    pub proxied: Vec<ClientId>,

    pub samples: Vec<(ClientId, i32, sim::ShotSampleProvenance)>,

    pub backlog_faults: Vec<(ClientId, InputBacklogFault)>,
}

#[derive(Resource, Debug, Default)]
pub struct ClientCommandInbox {
    queued: HashMap<ClientId, VecDeque<(Option<CmdSeq>, UserCmd, sim::ShotSampleProvenance)>>,

    last_consumed: HashMap<ClientId, UserCmd>,

    last_acked_seq: HashMap<ClientId, u32>,

    blocked_head: HashMap<ClientId, (CmdSeq, i32)>,
}

impl ClientCommandInbox {
    pub fn push(
        &mut self,
        id: ClientId,
        seq: Option<CmdSeq>,
        cmd: UserCmd,
        sample: Option<sim::ShotSampleProvenance>,
    ) {
        if let Some(s) = seq {
            if self
                .last_acked_seq
                .get(&id)
                .is_some_and(|&last| s.0 <= last)
            {
                return;
            }
            if self
                .queued
                .get(&id)
                .is_some_and(|q| q.iter().any(|(qs, _, _)| *qs == Some(s)))
            {
                return;
            }
        }
        let queue = self.queued.entry(id).or_default();

        if queue.len() > MAX_QUEUED_COMMANDS_PER_PEER {
            return;
        }
        let row = (
            seq,
            cmd,
            sample.unwrap_or(sim::ShotSampleProvenance::NO_CLAIM),
        );
        if let Some(sequence) = seq {
            let at = queue
                .iter()
                .position(|(other, _, _)| other.is_some_and(|other| other > sequence))
                .unwrap_or(queue.len());
            queue.insert(at, row);
        } else {
            queue.push_back(row);
        }
    }

    pub fn clear(&mut self) {
        self.queued.clear();
        self.last_consumed.clear();
        self.last_acked_seq.clear();
        self.blocked_head.clear();
    }

    pub fn retire_client(&mut self, id: ClientId) {
        self.queued.remove(&id);
        self.last_consumed.remove(&id);
        self.last_acked_seq.remove(&id);
        self.blocked_head.remove(&id);
    }

    pub fn pending(&self, id: ClientId) -> usize {
        self.queued.get(&id).map_or(0, VecDeque::len)
    }

    pub fn unique_queue_depth(&self, id: ClientId) -> usize {
        self.pending(id)
    }

    pub fn backlog_fault(&mut self, id: ClientId, time_ms: i32) -> Option<InputBacklogFault> {
        let queue = self.queued.get(&id)?;
        let sequenced = queue.len() - queue.iter().filter(|(seq, _, _)| seq.is_none()).count();
        if sequenced == 0 {
            return None;
        }
        if sequenced > MAX_QUEUED_COMMANDS_PER_PEER {
            return Some(InputBacklogFault::QueueCount { queued: sequenced });
        }
        let mut queued_ms: i32 = 0;
        let mut previous: Option<i32> = self
            .last_consumed
            .get(&id)
            .map(|cmd| cmd.server_time)
            .or_else(|| {
                queue
                    .iter()
                    .find(|(seq, _, _)| seq.is_some())
                    .map(|(_, cmd, _)| time_ms.min(cmd.server_time))
            });
        for (_, cmd, _) in queue.iter().filter(|(seq, _, _)| seq.is_some()) {
            if let Some(previous) = previous {
                queued_ms = queued_ms.saturating_add(
                    cmd.server_time
                        .saturating_sub(previous)
                        .clamp(0, COMMAND_TIME_STEP_MS),
                );
            }
            previous = Some(cmd.server_time);
        }
        if queued_ms > MAX_QUEUED_COMMAND_MS {
            return Some(InputBacklogFault::QueueDuration { queued_ms });
        }
        let head = queue.iter().find_map(|(seq, _, _)| *seq)?;
        let blocked = self.blocked_head.entry(id).or_insert((head, time_ms));
        if blocked.0 != head {
            *blocked = (head, time_ms);
        }
        let stalled_ms = time_ms.saturating_sub(blocked.1);
        if stalled_ms > MAX_COMMAND_STALL_MS {
            return Some(InputBacklogFault::NoProgress { stalled_ms });
        }
        None
    }

    pub fn last_acked_seq(&self, id: ClientId) -> Option<CmdSeq> {
        self.last_acked_seq.get(&id).copied().map(CmdSeq)
    }

    pub fn last_consumed(&self, id: ClientId) -> Option<UserCmd> {
        self.last_consumed.get(&id).copied()
    }

    fn known_clients(&self) -> Vec<ClientId> {
        let mut ids: Vec<ClientId> = self
            .queued
            .keys()
            .chain(self.last_consumed.keys())
            .copied()
            .collect();
        ids.sort_by_key(|id| id.0);
        ids.dedup();
        ids
    }

    pub fn take_for_tick(&mut self, time_ms: i32) -> GatheredCommands {
        let mut out = GatheredCommands::default();
        for id in self.known_clients() {
            // A datagram burst can contain more than one frame of runnable work.
            // Consume the bounded frame budget before measuring remaining debt.
            // Count and blocked-head limits still apply before any simulation.
            if let Some(fault) = self.backlog_fault(id, time_ms)
                && !matches!(fault, InputBacklogFault::QueueDuration { .. })
            {
                out.backlog_faults.push((id, fault));
                continue;
            }

            let mut expected = self
                .last_acked_seq
                .get(&id)
                .copied()
                .unwrap_or(0)
                .wrapping_add(1);
            let take = self
                .queued
                .get(&id)
                .map(|queue| {
                    queue
                        .iter()
                        .take_while(|(seq, cmd, _)| {
                            if let Some(seq) = seq {
                                if seq.0 != expected || cmd.server_time > time_ms {
                                    return false;
                                }
                                expected = expected.wrapping_add(1);
                            }
                            true
                        })
                        .take(MAX_COMMANDS_PER_PEER_PER_FRAME)
                        .count()
                })
                .unwrap_or(0);
            let mut drained: Vec<_> = self
                .queued
                .get_mut(&id)
                .map(|queue| queue.drain(..take).collect())
                .unwrap_or_default();
            if drained.iter().all(|(seq, _, _)| seq.is_none()) && drained.len() > 1 {
                drained.drain(..drained.len() - 1);
            }
            drained.sort_by_key(|(seq, _, _)| *seq);
            if drained.is_empty() {
                if let Some(fault) = self.backlog_fault(id, time_ms) {
                    out.backlog_faults.push((id, fault));
                    continue;
                }
                if let Some(&acked) = self.last_acked_seq.get(&id) {
                    out.acks.push((id, CmdSeq(acked)));
                    continue;
                }
                if let Some(mut cmd) = self.last_consumed.get(&id).copied() {
                    cmd.server_time = time_ms;
                    self.last_consumed.insert(id, cmd);
                    out.proxied.push(id);
                    out.cmds.push(sim::PlayerCommand::unsequenced(id, cmd));
                }
                continue;
            }
            self.blocked_head.remove(&id);
            let mut ack = None;
            for (seq, mut cmd, sample) in drained {
                cmd.server_time = if seq.is_some() {
                    cmd.server_time
                } else {
                    time_ms
                };
                self.last_consumed.insert(id, cmd);
                if let Some(seq) = seq {
                    self.last_acked_seq.insert(id, seq.0);
                    ack = Some(seq);
                }

                out.samples.push((id, cmd.server_time, sample));
                out.cmds.push(sim::PlayerCommand {
                    client: id,
                    command: cmd,
                    sequence: seq.map(|seq| sim::CommandSequence(seq.0)),
                });
            }
            if let Some(seq) = ack {
                out.acks.push((id, seq));
            }
            if let Some(fault) = self.backlog_fault(id, time_ms) {
                out.backlog_faults.push((id, fault));
            }
        }
        out
    }
}

pub const MAX_PENDING_ACTIONS_PER_CLIENT: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionEnqueueError {
    QueueFull { pending: usize },

    DuplicateRequestId { request_id: sim::ActionRequestId },
}

impl core::fmt::Display for ActionEnqueueError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::QueueFull { pending } => {
                write!(f, "action queue full ({pending} unresolved requests)")
            }
            Self::DuplicateRequestId { request_id } => {
                write!(
                    f,
                    "request_id {request_id} is already pending with another payload"
                )
            }
        }
    }
}

#[derive(Resource, Debug, Default)]
pub struct ClientActionInbox {
    pending: Vec<(ClientId, sim::ClientAction)>,
    started: HashMap<(ClientId, sim::ActionRequestId), std::time::Instant>,

    received: Vec<(ClientId, sim::ClientAction)>,
    overflowed: std::collections::HashSet<ClientId>,
}

impl ClientActionInbox {
    pub fn push(
        &mut self,
        id: ClientId,
        action: sim::ClientAction,
    ) -> Result<(), ActionEnqueueError> {
        let request_id = sim::action_request_id(&action);
        for (owner, existing) in &self.pending {
            if *owner != id || sim::action_request_id(existing) != request_id {
                continue;
            }
            if *existing == action {
                return Ok(());
            }
            return Err(ActionEnqueueError::DuplicateRequestId { request_id });
        }
        let held = self
            .pending
            .iter()
            .filter(|(owner, _)| *owner == id)
            .count();
        if held >= MAX_PENDING_ACTIONS_PER_CLIENT {
            return Err(ActionEnqueueError::QueueFull { pending: held });
        }
        self.pending.push((id, action));
        Ok(())
    }

    pub fn mark_dispatched(&mut self, id: ClientId, request_id: sim::ActionRequestId) {
        if self
            .pending
            .iter()
            .any(|(owner, action)| *owner == id && sim::action_request_id(action) == request_id)
        {
            self.started
                .entry((id, request_id))
                .or_insert_with(std::time::Instant::now);
        }
    }

    pub fn push_from_peer(&mut self, id: ClientId, action: sim::ClientAction) {
        if self.received.contains(&(id, action)) {
            return;
        }
        if self
            .received
            .iter()
            .filter(|(owner, _)| *owner == id)
            .count()
            >= MAX_PENDING_ACTIONS_PER_CLIENT
        {
            self.overflowed.insert(id);
            return;
        }
        self.received.push((id, action));
    }

    pub fn take_overflowed(&mut self) -> impl Iterator<Item = ClientId> + '_ {
        self.overflowed.drain()
    }

    pub fn timed_out(&self) -> bool {
        self.started
            .values()
            .any(|at| at.elapsed() > std::time::Duration::from_secs(8))
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub fn pending(&self) -> &[(ClientId, sim::ClientAction)] {
        &self.pending
    }

    pub fn pending_for(&self, id: ClientId) -> impl Iterator<Item = sim::ClientAction> + '_ {
        self.pending
            .iter()
            .filter(move |(owner, _)| *owner == id)
            .map(|(_, action)| *action)
    }

    pub fn retire(&mut self, id: ClientId, request_id: sim::ActionRequestId) -> bool {
        self.started.remove(&(id, request_id));
        let before = self.pending.len();
        self.pending
            .retain(|(owner, action)| *owner != id || sim::action_request_id(action) != request_id);
        self.pending.len() != before
    }

    pub fn retire_client(&mut self, id: ClientId) {
        self.started.retain(|(owner, _), _| *owner != id);
        self.pending.retain(|(owner, _)| *owner != id);
        self.received.retain(|(owner, _)| *owner != id);
        self.overflowed.remove(&id);
    }

    pub fn clear(&mut self) {
        self.started.clear();
        self.pending.clear();
        self.received.clear();
        self.overflowed.clear();
    }

    pub fn gather(&mut self) -> Vec<(ClientId, sim::ClientAction)> {
        let mut out = self.pending.clone();
        for (id, action) in &out {
            self.mark_dispatched(*id, sim::action_request_id(action));
        }
        out.append(&mut self.received);
        out
    }
}

pub fn run_fixed_authority_stream(
    world: &mut sim::SimWorld,
    ticks: u32,
    mut sample: impl FnMut(u32, i32) -> Vec<(ClientId, UserCmd)>,
) -> Vec<sim::Snapshot> {
    let mut clock = AuthorityClock::default();
    let mut out = Vec::with_capacity(ticks as usize);
    for _ in 0..ticks {
        let tick = clock.advance();
        let cmds = sample(tick, clock.time_ms);
        out.push(
            sim::step(
                world,
                sim::Tick(tick),
                &sim::TickInput::from_cmds(cmds),
                AUTHORITY_MS,
                sim::StepReason::AuthorityFrame,
            )
            .snapshot,
        );
    }
    out
}
