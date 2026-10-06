use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use crate::transport::protocol::{ConnectionId, MAX_PACKET_BYTES};
use crate::transport::wire::{WireError, WireReader, WireWriter};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapTransaction {
    pub epoch: u32,
    pub bootstrap_id: u32,
    pub tick_b: u32,
    pub snapshot_seq: u32,
    pub connection: ConnectionId,
    pub offer_bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueuedBootstrap {
    pub epoch: u32,
    pub bootstrap_id: u32,
    pub tick_b: u32,
    pub snapshot_seq: u32,
    pub connection: ConnectionId,
}

impl BootstrapTransaction {
    pub fn enqueue(
        self,
        lane: &BootstrapLane,
        member: master_protocol::MemberId,
    ) -> Result<QueuedBootstrap, &'static str> {
        if !epoch_applies(self.epoch, lane.epoch()) {
            return Err("bootstrap epoch is no longer live");
        }
        lane.push_to_worker(Some(member), self.offer_bytes)?;
        Ok(QueuedBootstrap {
            epoch: self.epoch,
            bootstrap_id: self.bootstrap_id,
            tick_b: self.tick_b,
            snapshot_seq: self.snapshot_seq,
            connection: self.connection,
        })
    }

    pub fn from_snapshot(
        epoch: u32,
        bootstrap_id: u32,
        tick_b: u32,
        snapshot_seq: u32,
        connection: ConnectionId,
        packet: Vec<u8>,
    ) -> Result<Self, WireError> {
        let offer_bytes = encode_bootstrap(&BootstrapMessage::Offer {
            epoch,
            bootstrap_id,
            tick_b,
            snapshot_seq,
            connection,
            packet,
        })?;
        Ok(Self {
            epoch,
            bootstrap_id,
            tick_b,
            snapshot_seq,
            connection,
            offer_bytes,
        })
    }
}

const BOOTSTRAP_MAGIC: [u8; 2] = *b"BS";
const BOOTSTRAP_VERSION: u8 = 1;
const BOOTSTRAP_OFFER: u8 = 1;
const BOOTSTRAP_APPLIED: u8 = 2;

pub const MAX_BOOTSTRAP_PENDING: usize = 8;

pub const MAX_PENDING_ADMISSION_ACKS: usize = master_protocol::MAX_SESSION_MEMBERS as usize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootstrapMessage {
    Offer {
        epoch: u32,
        bootstrap_id: u32,
        tick_b: u32,
        snapshot_seq: u32,
        connection: ConnectionId,
        packet: Vec<u8>,
    },
    Applied {
        epoch: u32,
        bootstrap_id: u32,
        tick_b: u32,
        snapshot_seq: u32,
        connection: ConnectionId,
    },
}

pub fn epoch_applies(message_epoch: u32, live_epoch: u32) -> bool {
    message_epoch != 0 && message_epoch == live_epoch
}

pub fn encode_bootstrap(message: &BootstrapMessage) -> Result<Vec<u8>, WireError> {
    let mut out = WireWriter::new();
    out.put_u8(BOOTSTRAP_MAGIC[0]);
    out.put_u8(BOOTSTRAP_MAGIC[1]);
    out.put_u8(BOOTSTRAP_VERSION);
    match message {
        BootstrapMessage::Offer {
            epoch,
            bootstrap_id,
            tick_b,
            snapshot_seq,
            connection,
            packet,
        } => {
            if packet.len() as u32 > MAX_PACKET_BYTES {
                return Err(WireError::Malformed(
                    "bootstrap packet exceeds max_packet_bytes",
                ));
            }
            out.put_u8(BOOTSTRAP_OFFER);
            out.put_u32(*epoch);
            out.put_u32(*bootstrap_id);
            out.put_u32(*tick_b);
            out.put_u32(*snapshot_seq);
            out.put_u32((connection.0 >> 32) as u32);
            out.put_u32(connection.0 as u32);
            out.put_u32(packet.len() as u32);
            out.put_bytes(packet);
        }
        BootstrapMessage::Applied {
            epoch,
            bootstrap_id,
            tick_b,
            snapshot_seq,
            connection,
        } => {
            out.put_u8(BOOTSTRAP_APPLIED);
            out.put_u32(*epoch);
            out.put_u32(*bootstrap_id);
            out.put_u32(*tick_b);
            out.put_u32(*snapshot_seq);
            out.put_u32((connection.0 >> 32) as u32);
            out.put_u32(connection.0 as u32);
        }
    }
    Ok(out.finish())
}

pub fn decode_bootstrap(bytes: &[u8]) -> Result<Option<BootstrapMessage>, WireError> {
    if !bytes.starts_with(&BOOTSTRAP_MAGIC) {
        return Ok(None);
    }
    let mut input = WireReader::new(bytes);
    let _ = input.get_u8()?;
    let _ = input.get_u8()?;
    if input.get_u8()? != BOOTSTRAP_VERSION {
        return Err(WireError::Malformed("unknown bootstrap version"));
    }
    let message = match input.get_u8()? {
        BOOTSTRAP_OFFER => {
            let epoch = input.get_u32()?;
            let bootstrap_id = input.get_u32()?;
            let tick_b = input.get_u32()?;
            let snapshot_seq = input.get_u32()?;
            let hi = input.get_u32()? as u64;
            let lo = input.get_u32()? as u64;
            let len = input.get_u32()? as usize;
            if len as u32 > MAX_PACKET_BYTES {
                return Err(WireError::Malformed(
                    "bootstrap packet exceeds max_packet_bytes",
                ));
            }
            let mut packet = vec![0_u8; len];
            input.get_bytes(&mut packet)?;
            BootstrapMessage::Offer {
                epoch,
                bootstrap_id,
                tick_b,
                snapshot_seq,
                connection: ConnectionId((hi << 32) | lo),
                packet,
            }
        }
        BOOTSTRAP_APPLIED => {
            let epoch = input.get_u32()?;
            let bootstrap_id = input.get_u32()?;
            let tick_b = input.get_u32()?;
            let snapshot_seq = input.get_u32()?;
            let hi = input.get_u32()? as u64;
            let lo = input.get_u32()? as u64;
            BootstrapMessage::Applied {
                epoch,
                bootstrap_id,
                tick_b,
                snapshot_seq,
                connection: ConnectionId((hi << 32) | lo),
            }
        }
        _ => return Err(WireError::Malformed("unknown bootstrap tag")),
    };
    if !input.is_empty() {
        return Err(WireError::Malformed("trailing bootstrap bytes"));
    }
    Ok(Some(message))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapIngress {
    pub member_id: Option<master_protocol::MemberId>,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootstrapAck {
    pub member_id: master_protocol::MemberId,
    pub epoch: u32,
    pub bootstrap_id: u32,
    pub snapshot_seq: u32,
    pub connection: ConnectionId,
}

pub struct BootstrapLane {
    pub(super) host_map_ready: Mutex<HashSet<master_protocol::MemberId>>,
    to_worker: Mutex<VecDeque<(Option<master_protocol::MemberId>, Vec<u8>)>>,
    from_worker: Mutex<VecDeque<BootstrapIngress>>,
    acks: Mutex<VecDeque<BootstrapAck>>,
    epoch: AtomicU32,
    entered_bootstrap: AtomicU32,
    entered_client: AtomicU32,
}

impl BootstrapLane {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            host_map_ready: Mutex::new(HashSet::new()),
            to_worker: Mutex::new(VecDeque::new()),
            from_worker: Mutex::new(VecDeque::new()),
            acks: Mutex::new(VecDeque::new()),
            epoch: AtomicU32::new(0),
            entered_bootstrap: AtomicU32::new(0),
            entered_client: AtomicU32::new(0),
        })
    }

    pub fn set_epoch(&self, epoch: u32) {
        let prev = self.epoch.swap(epoch, Ordering::AcqRel);
        if prev != epoch {
            self.host_map_ready
                .lock()
                .expect("bootstrap readiness poisoned")
                .clear();
            self.entered_bootstrap.store(0, Ordering::Release);
            self.entered_client.store(0, Ordering::Release);
            self.to_worker
                .lock()
                .expect("bootstrap lane poisoned")
                .clear();
            self.from_worker
                .lock()
                .expect("bootstrap lane poisoned")
                .clear();
            self.acks.lock().expect("bootstrap lane poisoned").clear();
        }
    }

    pub fn epoch(&self) -> u32 {
        self.epoch.load(Ordering::Acquire)
    }

    pub fn note_entered(&self, bootstrap_id: u32, client_id: u32) {
        self.entered_client.store(client_id, Ordering::Release);
        self.entered_bootstrap
            .store(bootstrap_id, Ordering::Release);
    }

    pub fn entered_bootstrap(&self) -> u32 {
        self.entered_bootstrap.load(Ordering::Acquire)
    }

    pub fn entered_client(&self) -> u32 {
        self.entered_client.load(Ordering::Acquire)
    }

    pub fn push_to_worker(
        &self,
        peer: Option<master_protocol::MemberId>,
        bytes: Vec<u8>,
    ) -> Result<(), &'static str> {
        let mut queue = self.to_worker.lock().expect("bootstrap lane poisoned");
        if queue.len() >= MAX_BOOTSTRAP_PENDING {
            return Err("bootstrap to-worker queue is full");
        }
        queue.push_back((peer, bytes));
        Ok(())
    }

    pub fn take_to_worker(&self) -> Vec<(Option<master_protocol::MemberId>, Vec<u8>)> {
        let mut queue = self.to_worker.lock().expect("bootstrap lane poisoned");
        queue.drain(..).collect()
    }

    pub fn push_from_worker(
        &self,
        member_id: Option<master_protocol::MemberId>,
        bytes: Vec<u8>,
    ) -> Result<(), &'static str> {
        let mut queue = self.from_worker.lock().expect("bootstrap lane poisoned");
        if queue.len() >= MAX_BOOTSTRAP_PENDING {
            return Err("bootstrap from-worker queue is full");
        }
        queue.push_back(BootstrapIngress { member_id, bytes });
        Ok(())
    }

    pub fn take_from_worker(&self) -> Vec<BootstrapIngress> {
        let mut queue = self.from_worker.lock().expect("bootstrap lane poisoned");
        queue.drain(..).collect()
    }

    pub fn push_ack(&self, ack: BootstrapAck) -> Result<(), &'static str> {
        let mut queue = self.acks.lock().expect("bootstrap lane poisoned");
        if let Some(pending) = queue
            .iter_mut()
            .find(|pending| pending.member_id == ack.member_id)
        {
            *pending = ack;
            return Ok(());
        }
        if queue.len() >= MAX_PENDING_ADMISSION_ACKS {
            return Err("bootstrap ack queue is full");
        }
        queue.push_back(ack);
        Ok(())
    }

    pub fn take_acks(&self) -> Vec<BootstrapAck> {
        let mut queue = self.acks.lock().expect("bootstrap lane poisoned");
        queue.drain(..).collect()
    }
}
