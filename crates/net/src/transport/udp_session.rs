use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::io;
use std::sync::{Arc, Mutex};

use bevy::prelude::Resource;
use master_protocol::MemberId;
use playerstate_iw4::UserCmd;
use sim::{ClientAction, ClientId, Snapshot, Tick};

use crate::authority::runtime::ClientShotSamples;
use crate::client::predict::CmdSeq;
use crate::transport::acked_baseline::AckedBaselineTable;
use crate::transport::bootstrap::{
    BootstrapAck, BootstrapLane, BootstrapMessage, BootstrapTransaction, QueuedBootstrap,
    decode_bootstrap, encode_bootstrap, epoch_applies,
};
use crate::transport::delta::{SnapshotDecoder, SnapshotEncoder};
use crate::transport::frame::{Frame, FrameParts, FrameSegments, FrameTail, WireMeta};
use crate::transport::loopback_live::ReceivedTick;
use crate::transport::meta_wire::WorldObjectSyncDecoder;
use crate::transport::netfields::compute_state_hash;
use crate::transport::protocol::{
    ClientPacket, ConnectionId, ConnectionTable, HandshakeHello, HandshakeReject, PacketHeader,
    ProtocolLimits, ServerPacket, SnapshotPayload, decode_client_packet, decode_server_packet,
};
use crate::transport::segment_delta::{self, MetaPatchCache};
use crate::transport::udp_socket::UdpSendError;
use crate::transport::wire::WireReader;

use super::account_wire::{AccountMessage, profile_payload};

const RELAY_MAIL_CAP: usize = 64;
pub(crate) const CMDS_PER_PACKET: usize = 16;

#[derive(Clone, Debug)]
pub struct RelayMailbox {
    inbound: Arc<Mutex<DataLanes>>,
    outbound: Arc<Mutex<DataLanes>>,
    control_inbound: Arc<Mutex<Vec<(MemberId, Vec<u8>)>>>,
    control_inbound_drained: Arc<tokio::sync::Notify>,
    control_outbound: Arc<Mutex<Vec<(MemberId, Vec<u8>)>>>,
    cap: usize,
}

impl RelayMailbox {
    pub fn new(cap: usize) -> Self {
        Self {
            inbound: Arc::new(Mutex::new(DataLanes::default())),
            outbound: Arc::new(Mutex::new(DataLanes::default())),
            control_inbound: Arc::new(Mutex::new(Vec::new())),
            control_inbound_drained: Arc::new(tokio::sync::Notify::new()),
            control_outbound: Arc::new(Mutex::new(Vec::new())),
            cap,
        }
    }

    pub fn push_control_inbound(
        &self,
        member: MemberId,
        bytes: Vec<u8>,
    ) -> Result<(), &'static str> {
        push_mail(&self.control_inbound, self.cap, member, bytes)
    }
    pub fn take_control_inbound(&self) -> Vec<(MemberId, Vec<u8>)> {
        let packets = take_mail(&self.control_inbound);
        self.control_inbound_drained.notify_one();
        packets
    }

    // The control reader is the sole producer. Keep the capacity wait in its
    // select loop so cancellation and local commands still run under pressure.
    pub(crate) async fn wait_control_inbound_capacity(&self) {
        loop {
            let drained = self.control_inbound_drained.notified();
            if self
                .control_inbound
                .lock()
                .expect("relay mailbox poisoned")
                .len()
                < self.cap
            {
                return;
            }
            drained.await;
        }
    }
    pub fn push_control_outbound(
        &self,
        member: MemberId,
        bytes: Vec<u8>,
    ) -> Result<(), &'static str> {
        push_mail(&self.control_outbound, self.cap, member, bytes)
    }
    pub fn take_control_outbound(&self) -> Vec<(MemberId, Vec<u8>)> {
        take_mail(&self.control_outbound)
    }

    pub fn with_default_cap() -> Self {
        Self::new(RELAY_MAIL_CAP)
    }

    pub fn push_inbound(&self, member: MemberId, bytes: Vec<u8>) -> bool {
        lock_lanes(&self.inbound).push(self.cap, member, bytes)
    }

    pub fn take_inbound(&self) -> Vec<(MemberId, Vec<u8>)> {
        lock_lanes(&self.inbound).take()
    }

    pub fn push_outbound(&self, member: MemberId, bytes: Vec<u8>) -> bool {
        lock_lanes(&self.outbound).push(self.cap, member, bytes)
    }

    pub fn take_outbound(&self) -> Vec<(MemberId, Vec<u8>)> {
        lock_lanes(&self.outbound).take()
    }
}

#[derive(Debug, Default)]
struct DataLanes {
    lanes: BTreeMap<MemberId, VecDeque<Vec<u8>>>,
}

impl DataLanes {
    fn push(&mut self, cap: usize, member: MemberId, bytes: Vec<u8>) -> bool {
        let lane = self.lanes.entry(member).or_default();
        let dropped = lane.len() >= cap;
        if dropped {
            lane.pop_front();
        }
        lane.push_back(bytes);
        dropped
    }

    fn take(&mut self) -> Vec<(MemberId, Vec<u8>)> {
        let mut out = Vec::new();
        for (member, lane) in std::mem::take(&mut self.lanes) {
            out.extend(lane.into_iter().map(|bytes| (member, bytes)));
        }
        out
    }
}

fn lock_lanes(lanes: &Mutex<DataLanes>) -> std::sync::MutexGuard<'_, DataLanes> {
    lanes.lock().expect("relay mailbox poisoned")
}

fn push_mail(
    queue: &Mutex<Vec<(MemberId, Vec<u8>)>>,
    cap: usize,
    member: MemberId,
    bytes: Vec<u8>,
) -> Result<(), &'static str> {
    let mut queue = queue.lock().expect("relay mailbox poisoned");
    if queue.len() >= cap {
        return Err("relay mailbox full");
    }
    queue.push((member, bytes));
    Ok(())
}

fn take_mail(queue: &Mutex<Vec<(MemberId, Vec<u8>)>>) -> Vec<(MemberId, Vec<u8>)> {
    std::mem::take(&mut *queue.lock().expect("relay mailbox poisoned"))
}

fn relay_send_error(message: &'static str) -> UdpSendError {
    UdpSendError::Io(io::Error::new(io::ErrorKind::WouldBlock, message))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommittedAdmission {
    pub member_id: MemberId,
    pub epoch: u32,
    pub bootstrap_id: u32,
    pub connection_id: u64,

    pub first_commit: bool,
}

#[derive(Resource)]
pub struct UdpAuthorityHub {
    relay: RelayMailbox,
    pub hello: HandshakeHello,
    pub limits: ProtocolLimits,
    pub connections: ConnectionTable,
    peers: HashMap<ConnectionId, MemberId>,
    replication: HashMap<ConnectionId, PeerReplicationState>,

    bootstrap: Option<Arc<BootstrapLane>>,
    next_bootstrap_id: HashMap<ConnectionId, u32>,
    member_by_conn: HashMap<ConnectionId, master_protocol::MemberId>,
    committed_admissions: Vec<CommittedAdmission>,

    denied: HashSet<master_protocol::MemberId>,
    accounts_required: bool,
}

#[derive(Debug)]
struct PeerReplicationState {
    baseline: AckedBaselineTable,
    encoder: SnapshotEncoder,
    next_snap_seq: u32,
    out_seq: u32,
    in_ack: u32,
    last_full: Option<Tick>,
    sent_ticks: std::collections::VecDeque<Tick>,
    admission: PeerAdmission,
    last_control_seq: Option<u16>,
    control_drops_sent: u32,
    account: Option<PeerAccount>,
}

#[derive(Debug)]
struct PeerAccount {
    challenge: crate::AccountChallenge,
    challenge_sent: bool,
    refusal_pending: bool,
    owner: Option<sim::AccountId>,
    offered: Option<sim::AccountSnapshot>,
}

#[derive(Debug)]
enum PeerAdmission {
    Uncommitted,
    Pending(QueuedBootstrap),
    Committed { bootstrap_id: u32, epoch: u32 },
}

impl PeerReplicationState {
    fn new() -> Self {
        Self {
            baseline: AckedBaselineTable::new(32),
            encoder: SnapshotEncoder::new(),
            next_snap_seq: 1,
            out_seq: 0,
            in_ack: 0,
            last_full: None,
            sent_ticks: std::collections::VecDeque::new(),
            admission: PeerAdmission::Uncommitted,
            last_control_seq: None,
            control_drops_sent: 0,
            account: None,
        }
    }

    fn queue_control(
        &mut self,
        mailbox: &RelayMailbox,
        member: MemberId,
        connection: ConnectionId,
        epoch: u32,
        mut payload: crate::ReliablePayload,
    ) -> Result<(), UdpSendError> {
        // QUIC control is ordered and reliable. Keep rows until their application
        // ACK, but enqueue each only once instead of repeating the pending window.
        payload.rows.retain(|(seq, _)| {
            self.last_control_seq
                .is_none_or(|last| crate::transport::reliable::seq_after(*seq, last))
        });
        if payload.rows.is_empty() && payload.dropped_oldest == self.control_drops_sent {
            return Ok(());
        }
        let last = payload
            .rows
            .last()
            .map(|(seq, _)| *seq)
            .or(self.last_control_seq);
        let dropped = payload.dropped_oldest;
        let packet = ServerPacket::Control {
            header: PacketHeader {
                connection,
                sequence: 0,
                ack: 0,
                epoch,
            },
            payload,
        };
        mailbox
            .push_control_outbound(member, packet.to_bytes())
            .map_err(relay_send_error)?;
        self.last_control_seq = last;
        self.control_drops_sent = dropped;
        Ok(())
    }

    fn reset_match(&mut self) {
        *self = Self::new();
    }

    fn admits_gameplay(&self, relay: bool) -> bool {
        if !relay {
            return true;
        }
        matches!(self.admission, PeerAdmission::Committed { .. })
    }
}

const FULL_SNAPSHOT_RESEND_TICKS: u32 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppliedAction {
    Commit,
    RepeatEnter,
    Ignore,
}

fn applied_action(
    pending: Option<(u32, u32, u32)>,
    committed: Option<u32>,
    bootstrap_id: u32,
    snapshot_seq: u32,
    epoch: u32,
) -> AppliedAction {
    match pending {
        Some(expected) if expected == (bootstrap_id, snapshot_seq, epoch) => AppliedAction::Commit,
        _ if committed == Some(bootstrap_id) => AppliedAction::RepeatEnter,
        _ => AppliedAction::Ignore,
    }
}

impl UdpAuthorityHub {
    pub fn relay(hello: HandshakeHello, first_client: u32, mailbox: RelayMailbox) -> Self {
        let limits = hello.limits;
        Self {
            relay: mailbox,
            hello,
            limits,
            connections: ConnectionTable::starting_at(first_client),
            peers: HashMap::new(),
            replication: HashMap::new(),
            bootstrap: None,
            next_bootstrap_id: HashMap::new(),
            member_by_conn: HashMap::new(),
            committed_admissions: Vec::new(),
            denied: HashSet::new(),
            accounts_required: false,
        }
    }

    pub fn mailbox(&self) -> RelayMailbox {
        self.relay.clone()
    }

    pub fn attach_bootstrap(&mut self, lane: Arc<BootstrapLane>) {
        self.bootstrap = Some(lane);
    }

    fn enroll_relay_member(&mut self, member_id: MemberId, occupied: &[u32]) -> ConnectionId {
        if let Some((conn, _)) = self.member_by_conn.iter().find(|(_, id)| **id == member_id) {
            return *conn;
        }
        let (conn, _) = self.connections.accept_new(occupied);
        self.peers.insert(conn, member_id);
        self.replication.insert(conn, PeerReplicationState::new());
        self.member_by_conn.insert(conn, member_id);
        conn
    }

    pub fn reconcile_relay_membership(
        &mut self,
        members: &[MemberId],
        local: MemberId,
        occupied: &[u32],
    ) {
        for member_id in members {
            if *member_id == local || self.denied.contains(member_id) {
                continue;
            }
            self.enroll_relay_member(*member_id, occupied);
        }
    }

    pub fn take_committed_admissions(&mut self) -> Vec<CommittedAdmission> {
        let mut ready = Vec::new();
        let live = self.live_packet_epoch();
        self.committed_admissions.retain(|admission| {
            if admission.epoch != live
                || !self
                    .replication
                    .contains_key(&ConnectionId(admission.connection_id))
            {
                return false;
            }

            let bound = !self.accounts_required
                || self
                    .replication
                    .get(&ConnectionId(admission.connection_id))
                    .and_then(|peer| peer.account.as_ref())
                    .is_some_and(|account| account.owner.is_some() && account.offered.is_some());
            if bound {
                ready.push(*admission);
            }
            !bound
        });
        ready
    }

    pub fn client_of_member(&self, member_id: master_protocol::MemberId) -> Option<ClientId> {
        let conn = self
            .member_by_conn
            .iter()
            .find_map(|(conn, id)| (*id == member_id).then_some(*conn))?;
        self.connections.client_of(conn).map(ClientId)
    }

    pub fn deny_member(&mut self, member_id: master_protocol::MemberId) -> Option<ClientId> {
        let client = self.retire_member(member_id);
        self.denied.insert(member_id);
        client
    }

    pub fn retire_member(&mut self, member_id: master_protocol::MemberId) -> Option<ClientId> {
        self.denied.remove(&member_id);
        let conn = self
            .member_by_conn
            .iter()
            .find_map(|(conn, id)| (*id == member_id).then_some(*conn))?;
        self.retire_connection(conn)
    }

    pub fn retire_client(&mut self, client: ClientId) -> bool {
        self.retire_client_with_reason(client, "ConnectionRetired")
    }

    pub fn retire_client_with_reason(&mut self, client: ClientId, reason: &str) -> bool {
        let Some(conn) = self
            .peers
            .keys()
            .copied()
            .find(|conn| self.connections.resolve(*conn, 0) == Ok(client.0))
        else {
            return false;
        };
        if let Some(member) = self.member_by_conn.get(&conn).copied() {
            let packet = ServerPacket::Control {
                header: PacketHeader {
                    connection: conn,
                    sequence: 0,
                    ack: 0,
                    epoch: self.live_packet_epoch(),
                },
                payload: crate::ReliablePayload {
                    ack_through: 0,
                    rows: vec![(1, crate::ReliableRow::Failure(reason.to_owned()))],
                    dropped_oldest: 0,
                },
            };
            if let Err(error) = self.relay.push_control_outbound(member, packet.to_bytes()) {
                diag::warn!(Net, "retire control: {error}");
            }
            self.denied.insert(member);
        }
        self.retire_connection(conn).is_some()
    }

    pub fn retire_connection(&mut self, conn: ConnectionId) -> Option<ClientId> {
        let client = self.connections.retire(conn).map(ClientId);
        self.peers.remove(&conn);
        self.member_by_conn.remove(&conn);
        self.replication.remove(&conn);
        self.next_bootstrap_id.remove(&conn);
        self.committed_admissions
            .retain(|admission| admission.connection_id != conn.0);
        client
    }

    fn send_outgoing(&mut self, member: MemberId, bytes: &[u8]) -> Result<(), UdpSendError> {
        if self.relay.push_outbound(member, bytes.to_vec()) {
            perf::net_leg("mail_drop_out", 0, 1);
        }
        Ok(())
    }

    fn live_packet_epoch(&self) -> u32 {
        self.bootstrap
            .as_ref()
            .map(|lane| lane.epoch())
            .unwrap_or(0)
    }

    pub fn reset_match(&mut self) {
        self.accounts_required = false;
        self.next_bootstrap_id.clear();
        self.committed_admissions.clear();
        self.denied.clear();
        for peer in self.replication.values_mut() {
            peer.reset_match();
        }
        if let Some(lane) = &self.bootstrap {
            lane.set_epoch(0);
        }
    }

    pub fn ingress(
        &mut self,
        cmd_inbox: &mut crate::ClientCommandInbox,
        action_inbox: &mut crate::ClientActionInbox,
        _samples: Option<&mut ClientShotSamples>,
        mut reliable: Option<&mut crate::ReliableEventHub>,
        account_context: Option<(&mut sim::SimWorld, frame::MatchKey)>,
    ) -> Result<(), String> {
        let (mut account_world, match_key) = match account_context {
            Some((world, key)) => (Some(world), key),
            None => (None, frame::MatchKey::NONE),
        };
        self.accounts_required = self.bootstrap.is_some()
            && account_world
                .as_ref()
                .is_some_and(|world| world.gsc_realm() == Some(sim::script::Realm::Iw4));
        self.apply_admission_acks();
        if self.accounts_required {
            self.offer_accounts(account_world.as_deref().unwrap(), match_key)?;
        }
        let mut packets: Vec<(Vec<u8>, MemberId, bool)> = Vec::new();
        for (member, bytes) in self.relay.take_inbound() {
            packets.push((bytes, member, false));
        }
        for (member, bytes) in self.relay.take_control_inbound() {
            packets.push((bytes, member, true));
        }
        for (bytes, from, control) in packets {
            if control {
                match AccountMessage::decode(&bytes) {
                    Ok(Some(message)) => {
                        if self.accounts_required {
                            self.apply_account_message(
                                from,
                                message,
                                account_world.as_deref_mut().unwrap(),
                                match_key,
                            )?;
                        }
                        continue;
                    }
                    Err(error) => {
                        diag::warn!(Net, "account control refused: {error}");
                        continue;
                    }
                    Ok(None) => {}
                }
            }
            let packet = match decode_client_packet(&bytes, &self.limits) {
                Ok(p) => p,
                Err(_) => continue,
            };
            match packet {
                ClientPacket::Connect(_) => {}
                ClientPacket::Commands {
                    header,
                    claimed_client,
                    cmds,
                    samples: cmd_samples,
                    actions,
                    reliable_ack,
                } => {
                    if self.peers.get(&header.connection) != Some(&from) {
                        continue;
                    }
                    if !header.applies_to_epoch(self.live_packet_epoch()) {
                        diag::warn!(
                            Net,
                            "authority ignored commands for epoch {} (live {})",
                            header.epoch,
                            self.live_packet_epoch()
                        );
                        continue;
                    }
                    let relay = self.bootstrap.is_some();
                    if !self
                        .replication
                        .get(&header.connection)
                        .is_some_and(|peer| peer.admits_gameplay(relay))
                    {
                        continue;
                    }
                    if self.accounts_required
                        && !self
                            .replication
                            .get(&header.connection)
                            .and_then(|peer| peer.account.as_ref())
                            .is_some_and(|account| {
                                account.challenge.match_key == match_key
                                    && account.owner.is_some()
                                    && account.offered.is_some()
                            })
                    {
                        continue;
                    }
                    let client = match self.connections.resolve(header.connection, claimed_client) {
                        Ok(id) => ClientId(id),
                        Err(_) => continue,
                    };
                    if let Some(peer) = self.replication.get_mut(&header.connection) {
                        peer.in_ack = header.sequence;
                    }
                    if let Some(reliable) = reliable.as_deref_mut() {
                        reliable.ack(client, reliable_ack);
                    }
                    if let Some((seq, _)) = cmds.last() {
                        perf::net_leg("cmd_in", seq.0, cmds.len() as u32);
                    }
                    for (seq, cmd) in cmds {
                        let mut sample = cmd_samples
                            .iter()
                            .find(|(sample_seq, _)| *sample_seq == seq)
                            .map(|(_, sample)| *sample);
                        if let Some(claim) = sample.as_mut() {
                            if claim.quality.claims_history()
                                && !self
                                    .replication
                                    .get(&header.connection)
                                    .is_some_and(|peer| {
                                        peer.sent_ticks.contains(&claim.left)
                                            && peer.sent_ticks.contains(&claim.right)
                                    })
                            {
                                claim.alpha = f32::NAN;
                            }
                        }
                        cmd_inbox.push(client, Some(seq), cmd, sample);
                    }
                    if control {
                        for action in actions {
                            action_inbox.push_from_peer(client, action);
                        }
                    }
                }
                ClientPacket::SnapshotAck {
                    header,
                    snapshot_seq,
                } => {
                    if self.peers.get(&header.connection) != Some(&from) {
                        continue;
                    }
                    if !header.applies_to_epoch(self.live_packet_epoch()) {
                        diag::warn!(
                            Net,
                            "authority ignored snapshot ack for epoch {} (live {})",
                            header.epoch,
                            self.live_packet_epoch()
                        );
                        continue;
                    }
                    if let Some(peer) = self.replication.get_mut(&header.connection) {
                        peer.in_ack = header.sequence;
                        let _ = peer.baseline.ack(snapshot_seq);
                    }
                }
            }
        }
        Ok(())
    }

    fn offer_accounts(
        &mut self,
        world: &sim::SimWorld,
        match_key: frame::MatchKey,
    ) -> Result<(), String> {
        for (connection, peer) in &mut self.replication {
            if !matches!(peer.admission, PeerAdmission::Committed { epoch, .. } if epoch == match_key.match_epoch)
            {
                continue;
            }
            let member = self.member_by_conn[connection];
            if peer
                .account
                .as_ref()
                .is_some_and(|account| account.challenge.match_key != match_key)
            {
                peer.account = None;
            }
            if peer.account.is_none() {
                peer.account = Some(PeerAccount {
                    challenge: crate::AccountChallenge::new(match_key, *connection, member)
                        .map_err(|error| format!("account challenge: {error:?}"))?,
                    challenge_sent: false,
                    refusal_pending: false,
                    owner: None,
                    offered: None,
                });
            }
            let account = peer.account.as_mut().unwrap();
            if !account.challenge_sent {
                let bytes = AccountMessage::Challenge(account.challenge)
                    .encode()
                    .map_err(|error| error.to_string())?;
                if self.relay.push_control_outbound(member, bytes).is_err() {
                    continue;
                }
                account.challenge_sent = true;
            }
            if account.refusal_pending {
                let bytes = AccountMessage::Refused(account.challenge)
                    .encode()
                    .map_err(|error| error.to_string())?;
                if self.relay.push_control_outbound(member, bytes).is_err() {
                    continue;
                }
                account.refusal_pending = false;
            }
            if let Some(owner) = account.owner
                && let Some(snapshot) = world.persistent_data().snapshot(owner)
                && account.offered.as_ref() != Some(&snapshot)
            {
                let bytes = AccountMessage::Update {
                    challenge: account.challenge,
                    snapshot: snapshot.clone(),
                }
                .encode()
                .map_err(|error| error.to_string())?;
                if self.relay.push_control_outbound(member, bytes).is_ok() {
                    account.offered = Some(snapshot);
                }
            }
        }
        Ok(())
    }

    fn apply_account_message(
        &mut self,
        from: MemberId,
        message: AccountMessage,
        world: &mut sim::SimWorld,
        match_key: frame::MatchKey,
    ) -> Result<(), String> {
        let context = match &message {
            AccountMessage::Profile { challenge, .. } | AccountMessage::Saved { challenge, .. } => {
                *challenge
            }
            _ => return Ok(()),
        };
        if context.match_key != match_key
            || context.member != from
            || self.peers.get(&context.connection) != Some(&from)
        {
            return Ok(());
        }
        let Some(account) = self
            .replication
            .get_mut(&context.connection)
            .and_then(|peer| peer.account.as_mut())
        else {
            return Ok(());
        };
        if account.challenge != context || !account.challenge_sent {
            return Ok(());
        }
        match message {
            AccountMessage::Profile {
                proof, snapshot, ..
            } => {
                let verified = profile_payload(proof.account, snapshot.as_ref())
                    .map_err(|error| error.to_string())
                    .and_then(|payload| {
                        proof
                            .verify(&context, &payload)
                            .map_err(|error| format!("{error:?}"))
                    });
                let accepted = verified.and_then(|owner| {
                    if account.owner.is_some_and(|old| old != owner) {
                        return Err("account owner cannot change".into());
                    }
                    let client = ClientId(
                        self.connections
                            .resolve(context.connection, 0)
                            .map_err(|error| format!("{error:?}"))?,
                    );
                    world
                        .persistent_data_mut()
                        .admit(client, owner, snapshot.as_ref(), None)
                        .map_err(|error| format!("{error:?}"))?;
                    account.owner = Some(owner);
                    account.refusal_pending = false;
                    Ok(())
                });
                if let Err(error) = accepted {
                    diag::warn!(Net, "remote account admission refused: {error}");
                    account.refusal_pending = true;
                }
            }
            AccountMessage::Saved {
                account: owner,
                revision,
                ..
            } if account.owner == Some(owner)
                && account
                    .offered
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.revision == revision) =>
            {
                world
                    .persistent_data_mut()
                    .acknowledge_saved(owner, revision);
            }
            _ => {}
        }
        self.offer_accounts(world, match_key)
    }

    fn apply_admission_acks(&mut self) {
        let Some(lane) = &self.bootstrap else {
            return;
        };
        let live = lane.epoch();
        for ack in lane.take_acks() {
            self.commit_admission(ack, live);
        }
    }

    fn commit_admission(&mut self, ack: BootstrapAck, live: u32) {
        if !epoch_applies(ack.epoch, live) {
            diag::warn!(
                Net,
                "authority ignored bootstrap applied for epoch {} (live {live})",
                ack.epoch
            );
            return;
        }
        match self.member_by_conn.get(&ack.connection).copied() {
            Some(mapped) if mapped == ack.member_id => {}
            _ => {
                diag::warn!(
                    Net,
                    "authority ignored bootstrap applied: connection {:?} does not map to member {}",
                    ack.connection,
                    ack.member_id
                );
                return;
            }
        }
        let Some(peer) = self.replication.get_mut(&ack.connection) else {
            diag::warn!(
                Net,
                "authority ignored bootstrap applied: no peer on {:?}",
                ack.connection
            );
            return;
        };
        let pending = match &peer.admission {
            PeerAdmission::Pending(offer) => {
                Some((offer.bootstrap_id, offer.snapshot_seq, offer.epoch))
            }
            _ => None,
        };
        let committed = match &peer.admission {
            PeerAdmission::Committed { bootstrap_id, .. } => Some(*bootstrap_id),
            _ => None,
        };
        let action = applied_action(
            pending,
            committed,
            ack.bootstrap_id,
            ack.snapshot_seq,
            ack.epoch,
        );
        let first_commit = match action {
            AppliedAction::Ignore => {
                diag::warn!(
                    Net,
                    "authority ignored bootstrap applied {} for member {}: no matching offer",
                    ack.bootstrap_id,
                    ack.member_id
                );
                return;
            }
            AppliedAction::RepeatEnter => false,
            AppliedAction::Commit => {
                peer.admission = PeerAdmission::Committed {
                    bootstrap_id: ack.bootstrap_id,
                    epoch: ack.epoch,
                };
                let _ = peer.baseline.ack(ack.snapshot_seq);
                true
            }
        };
        self.committed_admissions.push(CommittedAdmission {
            member_id: ack.member_id,
            epoch: ack.epoch,
            bootstrap_id: ack.bootstrap_id,
            connection_id: ack.connection.0,
            first_commit,
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fanout_with_seats(
        &mut self,
        snapshot: &Snapshot,
        acks: &[(ClientId, CmdSeq)],
        mut for_peer: impl FnMut(ClientId, &Snapshot) -> Option<Snapshot>,
        mut pending_svc: Option<&mut crate::PendingSvcSounds>,
        mut pending_playercard: Option<&mut crate::PendingPlayerCard>,
        mut pending_gamenotify: Option<&mut crate::PendingGameNotify>,
        reliable: Option<&crate::ReliableEventHub>,
        scores_due: bool,
    ) -> Result<(), UdpSendError> {
        let mut last_err = None;
        let live_epoch = self.live_packet_epoch();
        let peer_ids: Vec<ConnectionId> = self.peers.keys().copied().collect();
        let mut live: Option<Arc<Snapshot>> = None;
        let mut live_meta: Option<Arc<WireMeta>> = None;
        let mut meta_patches = MetaPatchCache::default();
        for conn in peer_ids {
            let Some(target) = self.peers.get(&conn).copied() else {
                continue;
            };
            let Some(client_u32) = self.connections.resolve(conn, 0).ok() else {
                continue;
            };
            // Do not capture a bootstrap while the peer is loading content. The
            // snapshot's clock starts when admission is ready to send it.
            if let Some(lane) = &self.bootstrap
                && !lane
                    .host_map_ready
                    .lock()
                    .expect("bootstrap readiness poisoned")
                    .contains(&target)
            {
                continue;
            }
            let client = ClientId(client_u32);
            let peer = self
                .replication
                .entry(conn)
                .or_insert_with(PeerReplicationState::new);
            if let (Some(lane), PeerAdmission::Pending(offer)) = (&self.bootstrap, &peer.admission)
                && offer.epoch == lane.epoch()
            {
                continue;
            }
            if peer.admits_gameplay(self.bootstrap.is_some()) {
                if let Some(reliable) = reliable {
                    if let Err(error) = peer.queue_control(
                        &self.relay,
                        target,
                        conn,
                        live_epoch,
                        reliable.payload(client),
                    ) {
                        last_err = Some(error);
                    }
                }
            }
            let resync = peer.baseline.clear_missing_ack();
            if resync {
                diag::warn!(
                    Net,
                    "acked baseline missing for {target:?}: sending a full snapshot"
                );
            }
            let baseline_seq = peer.baseline.baseline_seq_for_encode();
            if !peer.baseline.may_encode_against(baseline_seq) {
                diag::warn!(
                    Net,
                    "snapshot encode refused for {target:?} against baseline {baseline_seq}"
                );
                continue;
            }
            if baseline_seq == 0
                && !resync
                && let Some(last) = peer.last_full
                && snapshot.tick.0.saturating_sub(last.0) < FULL_SNAPSHOT_RESEND_TICKS
            {
                continue;
            }
            if baseline_seq == 0
                && let Some(lane) = &self.bootstrap
                && lane.epoch() == 0
            {
                continue;
            }
            let mut encoder = std::mem::take(&mut peer.encoder);
            if baseline_seq == 0 {
                peer.last_full = Some(snapshot.tick);
                encoder.reset();
            } else if let Some(baseline) = peer.baseline.baseline_for_encode() {
                encoder.adopt_baseline(baseline);
            }
            let peer_acks: Vec<(ClientId, CmdSeq)> = acks
                .iter()
                .copied()
                .filter(|(id, _)| *id == client)
                .collect();
            let (peer_snap, meta) = match for_peer(client, snapshot) {
                Some(own) => {
                    let meta = Arc::new(WireMeta::without_reliable_events(&own.meta));
                    (Arc::new(own), meta)
                }
                None => (
                    Arc::clone(live.get_or_insert_with(|| Arc::new(snapshot.clone()))),
                    Arc::clone(live_meta.get_or_insert_with(|| {
                        Arc::new(WireMeta::without_reliable_events(&snapshot.meta))
                    })),
                ),
            };
            let world_objects_wire = encoder
                .encode_world_objects(peer_snap.tick, &peer_snap.meta.world_objects)
                .to_vec();
            let snapshot_delta = encoder.encode(&peer_snap);
            peer.encoder = encoder;
            let mut tail = FrameTail::default();
            if let Some(pending) = pending_svc.as_mut() {
                tail.svc_sounds = pending.take_for(client);
            }
            if let Some(pending) = pending_playercard.as_mut() {
                let (slots, menus, splashes) = pending.take_for(client);
                tail.svc_card_slots = slots;
                tail.svc_open_menus = menus;
                tail.svc_hud_splashes = splashes;
            }
            if let Some(pending) = pending_gamenotify.as_mut() {
                tail.svc_game_notifies = pending.take_for(client);
            }
            if scores_due {
                tail.svc_scores = Some(crate::format_scoreboard_from_snapshot(&peer_snap));
            }
            let snapshot_seq = peer.next_snap_seq;
            peer.next_snap_seq = snapshot_seq.wrapping_add(1);
            let parts = FrameParts::new(
                peer_snap.tick,
                compute_state_hash(&peer_snap.players),
                &peer_acks,
                &snapshot_delta,
                meta,
                &world_objects_wire,
                &tail,
            );
            if parts
                .segments()
                .iter()
                .map(|segment| segment.len())
                .sum::<usize>()
                > segment_delta::MAX_RECONSTRUCTED_FRAME_BYTES
            {
                return Err(UdpSendError::Oversized {
                    encoded_len: parts.segments().iter().map(|segment| segment.len()).sum(),
                    max_packet_bytes: segment_delta::MAX_RECONSTRUCTED_FRAME_BYTES,
                });
            }
            let payload = match peer.baseline.baseline_parts_for_encode() {
                Some(baseline) if baseline_seq != 0 => {
                    let delta = segment_delta::encode(&parts, baseline, &mut meta_patches);
                    SnapshotPayload::against_baseline(&delta, || parts.to_raw())
                }
                _ => SnapshotPayload::Plain(parts.to_raw()),
            };
            peer.baseline.remember(snapshot_seq, peer_snap, parts);
            peer.out_seq = peer.out_seq.wrapping_add(1);
            let header = PacketHeader {
                connection: conn,
                sequence: peer.out_seq,
                ack: peer.in_ack,
                epoch: live_epoch,
            };
            let packet = ServerPacket::Snapshot {
                header,
                baseline_seq,
                snapshot_seq,
                payload,
            };
            let relay_bootstrap = self.bootstrap.is_some() && baseline_seq == 0;
            let already_admitted = matches!(peer.admission, PeerAdmission::Committed { .. });
            if relay_bootstrap && !already_admitted {
                let Some(lane) = &self.bootstrap else {
                    continue;
                };
                let epoch = lane.epoch();
                let bootstrap_id = {
                    let id = self.next_bootstrap_id.entry(conn).or_insert(1);
                    let current = *id;
                    *id = id.wrapping_add(1).max(1);
                    current
                };
                let packet_bytes = packet.to_bytes();
                let tick_b = snapshot.tick.0;
                match BootstrapTransaction::from_snapshot(
                    epoch,
                    bootstrap_id,
                    tick_b,
                    snapshot_seq,
                    conn,
                    packet_bytes,
                ) {
                    Ok(txn) => {
                        let offer_len = txn.offer_bytes.len();
                        let queued = match txn.enqueue(lane, target) {
                            Ok(queued) => queued,
                            Err(error) => {
                                diag::warn!(
                                    Net,
                                    "bootstrap offer deferred for {target:?}: {error}"
                                );
                                continue;
                            }
                        };
                        peer.admission = PeerAdmission::Pending(queued);
                        diag::info!(
                            Net,
                            "bootstrap-io session=host event=bootstrap offer encoded peer={target:?} epoch={epoch} bootstrap_id={bootstrap_id} snapshot_seq={snapshot_seq} bytes={}",
                            offer_len
                        );
                        peer.sent_ticks.push_back(snapshot.tick);
                        while peer.sent_ticks.len() > 32 {
                            peer.sent_ticks.pop_front();
                        }
                    }
                    Err(error) => {
                        diag::warn!(Net, "bootstrap offer encode failed for {target:?}: {error}");
                    }
                }
                continue;
            }
            let bytes = packet.to_bytes();
            perf::net_leg("snap_out", snapshot.tick.0, bytes.len() as u32);
            if let Err(e) = self.send_outgoing(target, &bytes) {
                last_err = Some(e);
            } else if let Some(peer) = self.replication.get_mut(&conn) {
                peer.sent_ticks.push_back(snapshot.tick);
                while peer.sent_ticks.len() > 32 {
                    peer.sent_ticks.pop_front();
                }
            }
        }
        match last_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

#[derive(Resource)]
pub struct UdpClientLink {
    relay: RelayMailbox,
    pub hello: HandshakeHello,
    pub limits: ProtocolLimits,
    pub connection: Option<ConnectionId>,
    pub assigned_client: Option<ClientId>,

    baselines: BTreeMap<u32, (Snapshot, Vec<u8>, FrameSegments)>,
    out_seq: u32,
    in_ack: u32,
    last_snapshot_seq: Option<u32>,
    required_baseline_seq: u32,
    applied_bootstrap_id: Option<u32>,
    last_applied_offer: Option<(u32, u32)>,
    pending_applied: Vec<BootstrapMessage>,

    held_bootstrap: Vec<Vec<u8>>,

    failed: Option<HandshakeReject>,
    bootstrap: Option<Arc<BootstrapLane>>,
    controls: Vec<crate::ReliablePayload>,
    account_messages: Vec<AccountMessage>,
    account_challenge: Option<crate::AccountChallenge>,
    account_profile: Option<Vec<u8>>,
    account_ack: Option<sim::AccountSnapshot>,
    account_offer: Option<sim::AccountSnapshot>,
    sent_actions: HashSet<sim::ActionRequestId>,
}

impl UdpClientLink {
    pub fn relay(hello: HandshakeHello, mailbox: RelayMailbox) -> Self {
        let limits = hello.limits;
        Self {
            relay: mailbox,
            hello,
            limits,
            connection: None,
            assigned_client: None,
            baselines: BTreeMap::new(),
            out_seq: 0,
            in_ack: 0,
            last_snapshot_seq: None,
            required_baseline_seq: 0,
            applied_bootstrap_id: None,
            last_applied_offer: None,
            pending_applied: Vec::new(),
            controls: Vec::new(),
            account_messages: Vec::new(),
            account_challenge: None,
            account_profile: None,
            account_ack: None,
            account_offer: None,
            sent_actions: HashSet::new(),
            held_bootstrap: Vec::new(),
            failed: None,
            bootstrap: None,
        }
    }

    pub fn mailbox(&self) -> RelayMailbox {
        self.relay.clone()
    }

    fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), UdpSendError> {
        if self.relay.push_outbound(MemberId([0; 16]), bytes.to_vec()) {
            perf::net_leg("mail_drop_out", 0, 1);
        }
        Ok(())
    }

    pub fn attach_bootstrap(&mut self, lane: Arc<BootstrapLane>) {
        self.bootstrap = Some(lane);
    }

    pub fn handshake_reject(&self) -> Option<HandshakeReject> {
        self.failed
    }

    pub fn has_applied_snapshot(&self) -> bool {
        self.last_snapshot_seq.is_some()
    }

    pub fn match_epoch(&self) -> u32 {
        self.bootstrap
            .as_ref()
            .map(|lane| lane.epoch())
            .unwrap_or(0)
    }

    pub fn has_entered_match(&self) -> bool {
        let Some(lane) = &self.bootstrap else {
            return true;
        };
        matches!(
            self.applied_bootstrap_id,
            Some(applied) if applied != 0 && applied == lane.entered_bootstrap()
        )
    }

    pub fn admission_bootstrap_id(&self) -> Option<u32> {
        self.applied_bootstrap_id.filter(|id| *id != 0)
    }

    pub fn note_applied_bootstrap(&mut self, bootstrap_id: u32) {
        self.applied_bootstrap_id = Some(bootstrap_id);
    }

    pub fn note_reject(&mut self, reason: HandshakeReject) {
        self.failed = Some(reason);
    }

    pub fn note_accept(&mut self, connection: ConnectionId, client: ClientId) {
        if self.failed.is_some() {
            return;
        }
        self.connection = Some(connection);
        self.assigned_client = Some(client);
    }

    pub fn note_applied_snapshot(&mut self, snapshot_seq: u32) {
        self.last_snapshot_seq = Some(
            self.last_snapshot_seq
                .map_or(snapshot_seq, |last| last.max(snapshot_seq)),
        );
    }

    pub fn flush_applied_after_adopt(&mut self) {
        let pending = std::mem::take(&mut self.pending_applied);
        let Some(lane) = self.bootstrap.clone() else {
            self.pending_applied = pending;
            return;
        };
        let mut leftover = Vec::new();
        let mut blocked = false;
        for applied in pending {
            if blocked {
                leftover.push(applied);
                continue;
            }
            if let BootstrapMessage::Applied { bootstrap_id, .. } = &applied {
                self.applied_bootstrap_id = Some(*bootstrap_id);
            }
            match encode_bootstrap(&applied) {
                Ok(bytes) => {
                    if let Err(error) = lane.push_to_worker(None, bytes) {
                        diag::warn!(Net, "bootstrap applied not queued: {error}");
                        leftover.push(applied);
                        blocked = true;
                    }
                }
                Err(error) => {
                    diag::warn!(Net, "bootstrap applied encode failed: {error}");
                    leftover.push(applied);
                    blocked = true;
                }
            }
        }
        self.pending_applied = leftover;
    }

    pub fn reset_match(&mut self) {
        self.connection = None;
        self.assigned_client = None;
        self.baselines.clear();
        self.out_seq = 0;
        self.in_ack = 0;
        self.last_snapshot_seq = None;
        self.required_baseline_seq = 0;
        self.applied_bootstrap_id = None;
        self.last_applied_offer = None;
        self.pending_applied.clear();
        self.held_bootstrap.clear();
        self.controls.clear();
        self.account_messages.clear();
        self.account_challenge = None;
        self.account_profile = None;
        self.account_ack = None;
        self.account_offer = None;
        self.sent_actions.clear();
        self.failed = None;
        if let Some(lane) = &self.bootstrap {
            lane.set_epoch(0);
        }
    }

    pub fn take_controls(&mut self) -> Vec<crate::ReliablePayload> {
        std::mem::take(&mut self.controls)
    }

    pub fn recv_ticks(&mut self) -> Result<Vec<ReceivedTick>, String> {
        let mut ticks = Vec::new();
        let mut snap_acks = Vec::new();
        for (_, bytes) in self.relay.take_inbound() {
            let packet = match decode_server_packet(&bytes, &self.limits) {
                Ok(p) => p,
                Err(_) => continue,
            };
            if !matches!(packet, ServerPacket::Control { .. }) {
                self.apply_server_packet(packet, &mut ticks, &mut snap_acks)?;
            }
        }
        self.drain_bootstrap_offers(&mut ticks, &mut snap_acks)?;
        for (_, bytes) in self.relay.take_control_inbound() {
            if let Some(message) =
                AccountMessage::decode(&bytes).map_err(|error| error.to_string())?
            {
                self.account_messages.push(message);
                continue;
            }
            let packet = decode_server_packet(&bytes, &self.limits).map_err(|e| e.to_string())?;
            if matches!(packet, ServerPacket::Control { .. }) {
                self.apply_server_packet(packet, &mut ticks, &mut snap_acks)?;
            }
        }
        self.adopt_entered_client();
        if self.bootstrap.is_none() || self.has_entered_match() {
            for seq in snap_acks {
                let _ = self.send_snapshot_ack(seq);
            }
        }
        Ok(ticks)
    }

    pub fn poll_accounts(
        &mut self,
        identity: Option<crate::SessionIdentity>,
        account: Option<&mut crate::LocalAccount>,
        schemas: &BTreeMap<String, Arc<structured_data_iw4::DefinitionSet>>,
        receipt: Option<&crate::AccountSaveReceipt>,
    ) -> Result<(), String> {
        let Some(identity) = identity else {
            self.account_messages.clear();
            return Ok(());
        };
        if self.account_challenge.is_some_and(|context| {
            context.match_key != identity.match_key()
                || context.member != identity.member_id
                || context.connection != self.connection.unwrap_or(ConnectionId(0))
        }) {
            self.account_challenge = None;
            self.account_profile = None;
            self.account_ack = None;
            self.account_offer = None;
        }
        let mut account = account;
        for message in std::mem::take(&mut self.account_messages) {
            let context = match &message {
                AccountMessage::Challenge(context) | AccountMessage::Refused(context) => *context,
                AccountMessage::Update { challenge, .. } => *challenge,
                _ => continue,
            };
            if context.match_key != identity.match_key()
                || context.member != identity.member_id
                || self.connection != Some(context.connection)
            {
                continue;
            }
            let local = account
                .as_deref_mut()
                .ok_or("no durable local account for remote admission")?;
            match message {
                AccountMessage::Challenge(context) => {
                    if self.account_challenge.is_some_and(|old| old != context) {
                        return Err("account challenge changed within a connection".into());
                    }
                    if local.id != local.key.account() {
                        return Err("local account signing owner mismatch".into());
                    }
                    let payload = profile_payload(local.id, local.snapshot.as_ref())
                        .map_err(|error| error.to_string())?;
                    let message = AccountMessage::Profile {
                        challenge: context,
                        proof: local.key.prove(&context, &payload),
                        snapshot: local.snapshot.clone(),
                    };
                    if self.account_challenge != Some(context) {
                        self.account_profile =
                            Some(message.encode().map_err(|error| error.to_string())?);
                        self.account_challenge = Some(context);
                    }
                }
                AccountMessage::Update { snapshot, .. }
                    if self.account_challenge == Some(context) =>
                {
                    if snapshot.account != local.id || local.id != local.key.account() {
                        return Err("remote account update owner mismatch".into());
                    }
                    if self
                        .account_offer
                        .as_ref()
                        .is_some_and(|old| snapshot.revision < old.revision)
                    {
                        continue;
                    }
                    if self
                        .account_offer
                        .as_ref()
                        .is_some_and(|old| old.revision == snapshot.revision && old != &snapshot)
                    {
                        return Err("conflicting account updates at one revision".into());
                    }
                    let mut validation = sim::PersistentDataStore::default();
                    validation
                        .install_schemas(schemas.clone())
                        .map_err(|error| format!("account schema: {error:?}"))?;
                    validation
                        .import(snapshot.clone())
                        .map_err(|error| format!("account update: {error:?}"))?;
                    self.account_offer = Some(snapshot.clone());
                    local.snapshot = Some(snapshot);
                }
                AccountMessage::Refused(_) if self.account_challenge == Some(context) => {
                    return Err("host refused account admission".into());
                }
                _ => {}
            }
        }
        if let Some(profile) = &self.account_profile {
            if self
                .relay
                .push_control_outbound(MemberId([0; 16]), profile.clone())
                .is_err()
            {
                return Ok(());
            }
            self.account_profile = None;
        }
        if let (Some(context), Some(local), Some(receipt)) =
            (self.account_challenge, account, receipt)
            && let Some(snapshot) = &local.snapshot
            && self.account_offer.as_ref() == Some(snapshot)
            && receipt.0.as_ref() == Some(snapshot)
            && self.account_ack.as_ref() != Some(snapshot)
        {
            let message = AccountMessage::Saved {
                challenge: context,
                account: local.id,
                revision: snapshot.revision,
            };
            if self
                .relay
                .push_control_outbound(
                    MemberId([0; 16]),
                    message.encode().map_err(|error| error.to_string())?,
                )
                .is_ok()
            {
                self.account_ack = Some(snapshot.clone());
            }
        }
        Ok(())
    }

    fn adopt_entered_client(&mut self) {
        let Some(lane) = &self.bootstrap else {
            return;
        };
        let entered = lane.entered_bootstrap();
        if entered == 0 || self.applied_bootstrap_id != Some(entered) {
            return;
        }
        let client = ClientId(lane.entered_client());
        if client.0 != 0 && self.assigned_client != Some(client) {
            self.assigned_client = Some(client);
        }
    }

    fn drain_bootstrap_offers(
        &mut self,
        ticks: &mut Vec<ReceivedTick>,
        snap_acks: &mut Vec<u32>,
    ) -> Result<(), String> {
        let (live, from_lane) = {
            let Some(lane) = &self.bootstrap else {
                return Ok(());
            };
            (lane.epoch(), lane.take_from_worker())
        };
        let mut offers = std::mem::take(&mut self.held_bootstrap);
        offers.extend(from_lane.into_iter().map(|ingress| ingress.bytes));
        for bytes in offers {
            match decode_bootstrap(&bytes) {
                Ok(Some(BootstrapMessage::Offer {
                    epoch,
                    bootstrap_id,
                    tick_b,
                    snapshot_seq,
                    connection,
                    packet,
                })) => {
                    if !epoch_applies(epoch, live) {
                        diag::warn!(
                            Net,
                            "client ignored bootstrap offer for epoch {epoch} (live {live})"
                        );
                        continue;
                    }
                    diag::info!(
                        Net,
                        "bootstrap-io session=join event=bootstrap offer epoch={epoch} live={live} bootstrap_id={bootstrap_id} snapshot_seq={snapshot_seq} bytes={}",
                        packet.len()
                    );
                    if self.connection.is_none() {
                        self.connection = Some(connection);
                    }
                    match self.apply_server_bytes(&packet, ticks, snap_acks) {
                        Ok(true) => {
                            self.last_applied_offer = Some((bootstrap_id, snapshot_seq));
                            self.pending_applied.push(BootstrapMessage::Applied {
                                epoch,
                                bootstrap_id,
                                tick_b,
                                snapshot_seq,
                                connection,
                            });
                        }
                        Ok(false) => {
                            if self.last_applied_offer == Some((bootstrap_id, snapshot_seq)) {
                                self.pending_applied.push(BootstrapMessage::Applied {
                                    epoch,
                                    bootstrap_id,
                                    tick_b,
                                    snapshot_seq,
                                    connection,
                                });
                            }
                        }
                        Err(error) => {
                            diag::warn!(
                                Net,
                                "bootstrap offer named the connection; snapshot not applied: {error}"
                            );
                        }
                    }
                }
                Ok(Some(BootstrapMessage::Applied { .. })) => {
                    diag::warn!(Net, "client ignored a bootstrap applied on the offer lane");
                }
                Ok(None) => {
                    diag::warn!(Net, "client ignored non-bootstrap bytes on the offer lane");
                }
                Err(error) => {
                    diag::warn!(Net, "client ignored malformed bootstrap offer: {error}");
                }
            }
        }
        Ok(())
    }

    fn apply_server_bytes(
        &mut self,
        bytes: &[u8],
        ticks: &mut Vec<ReceivedTick>,
        snap_acks: &mut Vec<u32>,
    ) -> Result<bool, String> {
        let packet = decode_server_packet(bytes, &self.limits).map_err(|e| e.to_string())?;
        self.apply_server_packet(packet, ticks, snap_acks)
    }

    fn apply_server_packet(
        &mut self,
        packet: ServerPacket,
        ticks: &mut Vec<ReceivedTick>,
        snap_acks: &mut Vec<u32>,
    ) -> Result<bool, String> {
        match packet {
            ServerPacket::Control { header, payload } => {
                if self.failed.is_none()
                    && self.connection == Some(header.connection)
                    && header.applies_to_epoch(self.match_epoch())
                {
                    self.controls.push(payload);
                }
                Ok(false)
            }
            ServerPacket::Accept {
                connection,
                assigned_client,
                ..
            } => {
                if self.failed.is_none() {
                    self.note_accept(connection, ClientId(assigned_client));
                }
                Ok(false)
            }
            ServerPacket::Reject(reason) => {
                self.note_reject(reason);
                let ours = self.hello.content;
                Err(format!(
                    "handshake rejected: {reason}; our content gameplay={:016x} \
                     map={:016x} models={:016x} weapons={:016x} classes={:016x}",
                    ours.gameplay, ours.map, ours.models, ours.weapons, ours.classes
                ))
            }
            ServerPacket::Snapshot {
                header,
                baseline_seq,
                snapshot_seq,
                payload,
            } => {
                if self.failed.is_some() || self.connection != Some(header.connection) {
                    return Ok(false);
                }
                if !header.applies_to_epoch(self.match_epoch()) {
                    diag::warn!(
                        Net,
                        "client ignored snapshot for epoch {} (live {})",
                        header.epoch,
                        self.match_epoch()
                    );
                    return Ok(false);
                }
                if self.baselines.contains_key(&snapshot_seq) {
                    return Ok(false);
                }
                let baseline = if baseline_seq == 0 {
                    None
                } else {
                    match self.baselines.get(&baseline_seq) {
                        Some(baseline) => Some(baseline),
                        None => return Ok(false),
                    }
                };
                let wire_len = payload.wire_len();
                let Some(payload) =
                    payload.decode(baseline.map(|(_, raw, segments)| (raw.as_slice(), segments)))
                else {
                    return Ok(false);
                };
                let baseline = baseline.map(|(snapshot, _, _)| snapshot.clone());
                let mut decoder = SnapshotDecoder::new();
                let mut world_decoder = WorldObjectSyncDecoder::default();
                if let Some(baseline) = baseline.as_ref() {
                    decoder.adopt_baseline(baseline);
                    world_decoder.adopt_baseline(baseline.meta.world_objects.clone());
                }
                self.in_ack = header.sequence;
                let mut input = WireReader::new(&payload);
                let (frame, segments) = Frame::decode_segments(&mut input, &mut world_decoder)
                    .map_err(|e| e.to_string())?;
                let mut snapshot = decoder
                    .decode(&frame.snapshot_delta)
                    .map_err(|e| e.to_string())?;
                snapshot.meta = frame.snapshot_meta.clone();
                self.note_applied_snapshot(snapshot_seq);
                self.baselines
                    .insert(snapshot_seq, (snapshot.clone(), payload, segments));
                self.required_baseline_seq = self.required_baseline_seq.max(baseline_seq);
                self.retain_applied_baseline();
                snap_acks.push(snapshot_seq);
                perf::net_leg("snap_in", snapshot.tick.0, wire_len as u32);
                ticks.push(ReceivedTick { snapshot, frame });
                Ok(true)
            }
        }
    }

    fn retain_applied_baseline(&mut self) {
        let pinned = self.last_snapshot_seq;
        while self.baselines.len() > 64 {
            let oldest = self
                .baselines
                .keys()
                .copied()
                .find(|seq| Some(*seq) != pinned && *seq != self.required_baseline_seq);
            match oldest {
                Some(seq) => {
                    self.baselines.remove(&seq);
                }
                None => break,
            }
        }
    }

    fn send_snapshot_ack(&mut self, snapshot_seq: u32) -> Result<(), UdpSendError> {
        let Some(connection) = self.connection else {
            return Ok(());
        };
        self.out_seq = self.out_seq.wrapping_add(1);
        let packet = ClientPacket::SnapshotAck {
            header: PacketHeader {
                connection,
                sequence: self.out_seq,
                ack: self.in_ack,
                epoch: self.match_epoch(),
            },
            snapshot_seq,
        };
        self.send_bytes(&packet.to_bytes())
    }

    pub fn has_unsent_actions(&self, actions: &[ClientAction]) -> bool {
        actions
            .iter()
            .any(|action| !self.sent_actions.contains(&sim::action_request_id(action)))
    }

    pub fn send_commands(
        &mut self,
        cmds: &[(CmdSeq, UserCmd, sim::ShotSampleProvenance)],
        actions: &[ClientAction],
        reliable_ack: u16,
    ) -> Result<(), UdpSendError> {
        let Some(connection) = self.connection else {
            return Ok(());
        };
        if let Some(seq) = self.last_snapshot_seq {
            let _ = self.send_snapshot_ack(seq);
        }

        let count = CMDS_PER_PACKET.min(usize::from(self.limits.max_cmds_per_tick));
        if count == 0 {
            return Err(relay_send_error("peer permits no commands per packet"));
        }
        for chunk in cmds.chunks(count) {
            self.send_command_packet(connection, chunk, &[], reliable_ack)?;
        }

        self.sent_actions.retain(|id| {
            actions
                .iter()
                .any(|action| sim::action_request_id(action) == *id)
        });
        let fresh: Vec<_> = actions
            .iter()
            .copied()
            .filter(|action| !self.sent_actions.contains(&sim::action_request_id(action)))
            .collect();
        if !fresh.is_empty() || cmds.is_empty() {
            self.send_command_packet(connection, &[], &fresh, reliable_ack)?;
            self.sent_actions
                .extend(fresh.iter().map(sim::action_request_id));
        }
        Ok(())
    }

    fn send_command_packet(
        &mut self,
        connection: ConnectionId,
        cmds: &[(CmdSeq, UserCmd, sim::ShotSampleProvenance)],
        actions: &[ClientAction],
        reliable_ack: u16,
    ) -> Result<(), UdpSendError> {
        self.out_seq = self.out_seq.wrapping_add(1);
        let packet = ClientPacket::Commands {
            header: PacketHeader {
                connection,
                sequence: self.out_seq,
                ack: self.in_ack,
                epoch: self.match_epoch(),
            },
            claimed_client: self.assigned_client.map(|c| c.0).unwrap_or(0),
            cmds: cmds.iter().map(|(seq, cmd, _)| (*seq, *cmd)).collect(),
            samples: cmds
                .iter()
                .map(|(seq, _, sample)| (*seq, *sample))
                .collect(),
            actions: actions.to_vec(),
            reliable_ack,
        };
        if let Some((seq, _, _)) = cmds.last() {
            perf::net_leg("cmd_out", seq.0, cmds.len() as u32);
        }
        if cmds.is_empty() {
            self.relay
                .push_control_outbound(MemberId([0; 16]), packet.to_bytes())
                .map_err(relay_send_error)
        } else {
            self.send_bytes(&packet.to_bytes())
        }
    }
}
