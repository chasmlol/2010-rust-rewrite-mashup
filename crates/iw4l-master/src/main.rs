#![forbid(unsafe_code)]

mod updates;

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Write};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use master_protocol::{
    ALPN, AdmissionFailure, Advert, AdvertId, Channel, ControlFrame, ControlHello, ControlRequest,
    ControlResponse, EndpointRole, MAX_BOOTSTRAP_STREAM_BYTES, MAX_CONCURRENT_BOOTSTRAP,
    MAX_LIST_ADVERTS, MAX_RELAY_UNI_STREAMS, MemberId, PeerEvent, RelayDatagram, RequestBody,
    ResponseBody, RoomPhase, RoomView, SESSION_IDLE, SESSION_KEEP_ALIVE, ServiceError,
    SessionCloseReason, StatusResponse, decode_relay, decode_relay_stream, decode_stream_payload,
    encode_relay, encode_relay_stream, encode_stream_frame, stream_frame_len,
};
use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls_platform_verifier::ConfigVerifierExt;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{Mutex, Semaphore};

type Error = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, Error>;

const MAX_ROOMS: usize = 10_000;
const CONTROL_CAP: usize = 32;
const CONTROL_PRIORITY: i32 = 0;

const BOOTSTRAP_PRIORITY: i32 = -32;
const CLI_DEADLINE: Duration = Duration::from_secs(8);
const BOOTSTRAP_FORWARD: Duration = Duration::from_secs(8);
const HELLO_DEADLINE: Duration = Duration::from_secs(8);
const MAX_CONNECTIONS: usize = 256;
/// A household or a LAN party shares one address.
const MAX_CONNECTIONS_PER_ADDRESS: usize = 32;

const CLOSE_SERVICE_ERROR: u32 = 1;
const CLOSE_SERVICE_DONE: u32 = 0;

const MAX_CLOSE_REASON_BYTES: usize = 120;

struct AddressSlot {
    counts: Arc<std::sync::Mutex<HashMap<IpAddr, usize>>>,
    ip: IpAddr,
}

impl AddressSlot {
    fn claim(counts: &Arc<std::sync::Mutex<HashMap<IpAddr, usize>>>, ip: IpAddr) -> Option<Self> {
        let mut held = counts.lock().expect("address counts poisoned");
        let count = held.entry(ip).or_insert(0);
        if *count >= MAX_CONNECTIONS_PER_ADDRESS {
            return None;
        }
        *count += 1;
        Some(Self {
            counts: Arc::clone(counts),
            ip,
        })
    }
}

impl Drop for AddressSlot {
    fn drop(&mut self) {
        let Ok(mut held) = self.counts.lock() else {
            return;
        };
        if let Some(count) = held.get_mut(&self.ip) {
            *count -= 1;
            if *count == 0 {
                held.remove(&self.ip);
            }
        }
    }
}

enum ConnTask {
    Writer(Result<()>),
    Bootstrap,
}

enum Command {
    Serve {
        bind: SocketAddr,
        cert: PathBuf,
        key: PathBuf,
        updates: PathBuf,
    },
    Status(ClientTarget),
    List(ClientTarget),
    PrintUnit(UnitSpec),
}

/// Everything `serve` will be started with. Kept next to the `serve` parser so
/// a new flag cannot reach one without the other.
struct UnitSpec {
    updates: PathBuf,
    channel: Channel,
    exec: PathBuf,
    cert: PathBuf,
    key: PathBuf,
    user: String,
    group: String,
}

struct ClientTarget {
    connect: SocketAddr,
    server_name: String,
    ca_cert: Option<PathBuf>,
}

struct Peer {
    player_name: String,
    connection: quinn::Connection,
    control_tx: tokio::sync::mpsc::Sender<ControlFrame>,
}

struct PasswordVerifier {
    salt: [u8; 16],
    digest: ring::digest::Digest,
}

impl PasswordVerifier {
    fn new(password: &str) -> Option<Self> {
        if password.is_empty() {
            return None;
        }
        let salt = random_bytes();
        Some(Self {
            salt,
            digest: Self::digest(salt, password),
        })
    }
    fn digest(salt: [u8; 16], password: &str) -> ring::digest::Digest {
        let mut context = ring::digest::Context::new(&ring::digest::SHA256);
        context.update(&salt);
        context.update(password.as_bytes());
        context.finish()
    }
    fn matches(&self, password: &str) -> bool {
        self.digest
            .as_ref()
            .iter()
            .zip(Self::digest(self.salt, password).as_ref())
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
    }
}

struct Room {
    view: RoomView,
    password: Option<PasswordVerifier>,
    host_connection_id: u64,
    member_of: HashMap<u64, MemberId>,
    connection_of: HashMap<MemberId, u64>,
}

#[derive(Default)]
struct ServiceState {
    rooms: HashMap<AdvertId, Room>,
    peers: HashMap<u64, Peer>,
    membership: HashMap<u64, AdvertId>,
    generation: u64,
}

struct HandleOutcome {
    response: ControlResponse,
    publishes: Vec<(u64, ControlFrame)>,
}

impl ServiceState {
    fn attach_peer(
        &mut self,
        connection_id: u64,
        player_name: String,
        connection: quinn::Connection,
        control_tx: tokio::sync::mpsc::Sender<ControlFrame>,
    ) {
        self.peers.insert(
            connection_id,
            Peer {
                player_name,
                connection,
                control_tx,
            },
        );
    }

    fn publish_view(&self, room_id: AdvertId) -> Vec<(u64, ControlFrame)> {
        let Some(room) = self.rooms.get(&room_id) else {
            return Vec::new();
        };
        let frame = ControlFrame::RoomView(room.view.clone());
        room.member_of
            .keys()
            .copied()
            .map(|connection_id| (connection_id, frame.clone()))
            .collect()
    }

    fn close_room(
        &mut self,
        room_id: AdvertId,
        reason: SessionCloseReason,
    ) -> Vec<(u64, ControlFrame)> {
        let Some(room) = self.rooms.remove(&room_id) else {
            return Vec::new();
        };
        self.generation = self.generation.wrapping_add(1);
        let frame = ControlFrame::Closed {
            room_id,
            epoch: room.view.epoch,
            reason,
        };
        let mut publishes = Vec::new();
        for connection_id in room.member_of.keys().copied() {
            self.membership.remove(&connection_id);
            publishes.push((connection_id, frame.clone()));
        }
        publishes
    }

    fn disconnect(&mut self, connection_id: u64) -> Vec<(u64, ControlFrame)> {
        self.peers.remove(&connection_id);
        let Some(room_id) = self.membership.get(&connection_id).copied() else {
            return Vec::new();
        };
        let Some(room) = self.rooms.get(&room_id) else {
            self.membership.remove(&connection_id);
            return Vec::new();
        };
        if room.host_connection_id == connection_id {
            return self.close_room(room_id, SessionCloseReason::HostLeft);
        }
        self.remove_member(connection_id)
    }

    fn remove_member(&mut self, connection_id: u64) -> Vec<(u64, ControlFrame)> {
        let Some(room_id) = self.membership.remove(&connection_id) else {
            return Vec::new();
        };
        let Some(room) = self.rooms.get_mut(&room_id) else {
            return Vec::new();
        };
        let Some(member_id) = room.member_of.remove(&connection_id) else {
            return Vec::new();
        };
        room.connection_of.remove(&member_id);
        room.view.members.retain(|id| *id != member_id);
        room.view.member_names.remove(&member_id);
        room.view.revision = room.view.revision.wrapping_add(1).max(1);
        self.generation = self.generation.wrapping_add(1);
        self.publish_view(room_id)
    }

    fn bump_room(&mut self, room_id: AdvertId) {
        if let Some(room) = self.rooms.get_mut(&room_id) {
            room.view.revision = room.view.revision.wrapping_add(1).max(1);
        }
        self.generation = self.generation.wrapping_add(1);
    }

    fn host_of(&self, connection_id: u64) -> std::result::Result<AdvertId, ServiceError> {
        let room_id = *self
            .membership
            .get(&connection_id)
            .ok_or(ServiceError::NotMember)?;
        let room = self
            .rooms
            .get(&room_id)
            .ok_or(ServiceError::UnknownAdvert)?;
        if room.host_connection_id != connection_id {
            return Err(ServiceError::NotHost);
        }
        Ok(room_id)
    }

    fn member_of(
        &self,
        connection_id: u64,
    ) -> std::result::Result<(AdvertId, MemberId), ServiceError> {
        let room_id = *self
            .membership
            .get(&connection_id)
            .ok_or(ServiceError::NotMember)?;
        let room = self
            .rooms
            .get(&room_id)
            .ok_or(ServiceError::UnknownAdvert)?;
        let member_id = *room
            .member_of
            .get(&connection_id)
            .ok_or(ServiceError::NotMember)?;
        Ok((room_id, member_id))
    }

    fn peer_connection(&self, connection_id: u64) -> Option<quinn::Connection> {
        self.peers
            .get(&connection_id)
            .map(|peer| peer.connection.clone())
    }

    fn route_target(
        &self,
        connection_id: u64,
        relay: &RelayDatagram<'_>,
    ) -> Result<(quinn::Connection, Vec<u8>)> {
        match relay {
            RelayDatagram::ClientToHost(payload) => {
                let (room_id, member_id) = self.member_of(connection_id)?;
                let room = self
                    .rooms
                    .get(&room_id)
                    .ok_or(ServiceError::UnknownAdvert)?;
                let host = self
                    .peer_connection(room.host_connection_id)
                    .ok_or(ServiceError::NotHost)?;
                let outgoing = encode_relay(RelayDatagram::ServiceToHost { member_id, payload })?;
                Ok((host, outgoing))
            }
            RelayDatagram::HostToMember { member_id, payload } => {
                let room_id = self.host_of(connection_id)?;
                let room = self
                    .rooms
                    .get(&room_id)
                    .ok_or(ServiceError::UnknownAdvert)?;
                let target_id = *room
                    .connection_of
                    .get(member_id)
                    .ok_or(ServiceError::NotMember)?;
                let target = self
                    .peer_connection(target_id)
                    .ok_or(ServiceError::NotMember)?;
                let outgoing = encode_relay(RelayDatagram::ServiceToMember(payload))?;
                Ok((target, outgoing))
            }
            RelayDatagram::ServiceToHost { .. } | RelayDatagram::ServiceToMember(_) => {
                Err(ServiceError::Malformed.into())
            }
        }
    }

    fn route_target_stream(
        &self,
        connection_id: u64,
        relay: &RelayDatagram<'_>,
    ) -> Result<(quinn::Connection, Vec<u8>)> {
        match relay {
            RelayDatagram::ClientToHost(payload) => {
                let (room_id, member_id) = self.member_of(connection_id)?;
                let room = self
                    .rooms
                    .get(&room_id)
                    .ok_or(ServiceError::UnknownAdvert)?;
                let host = self
                    .peer_connection(room.host_connection_id)
                    .ok_or(ServiceError::NotHost)?;
                let outgoing =
                    encode_relay_stream(RelayDatagram::ServiceToHost { member_id, payload })?;
                Ok((host, outgoing))
            }
            RelayDatagram::HostToMember { member_id, payload } => {
                let room_id = self.host_of(connection_id)?;
                let room = self
                    .rooms
                    .get(&room_id)
                    .ok_or(ServiceError::UnknownAdvert)?;
                let target_id = *room
                    .connection_of
                    .get(member_id)
                    .ok_or(ServiceError::NotMember)?;
                let target = self
                    .peer_connection(target_id)
                    .ok_or(ServiceError::NotMember)?;
                let outgoing = encode_relay_stream(RelayDatagram::ServiceToMember(payload))?;
                Ok((target, outgoing))
            }
            RelayDatagram::ServiceToHost { .. } | RelayDatagram::ServiceToMember(_) => {
                Err(ServiceError::Malformed.into())
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    match parse_args()? {
        Command::Serve {
            bind,
            cert,
            key,
            updates,
        } => {
            tokio::select! {
                result = serve(bind, &cert, &key) => result,
                result = updates::serve(bind, cert.clone(), key.clone(), updates) => result,
            }
        }
        Command::Status(target) => tokio::time::timeout(CLI_DEADLINE, status(&target))
            .await
            .map_err(|_| "master status deadline (connect + RPC)")?,
        Command::List(target) => tokio::time::timeout(CLI_DEADLINE, list(&target))
            .await
            .map_err(|_| "master list deadline (connect + RPC)")?,
        Command::PrintUnit(spec) => print_unit(&spec),
    }
}

fn parse_args() -> Result<Command> {
    let mut args = std::env::args().skip(1);
    let command = args
        .next()
        .ok_or("usage: iw4l-master serve|status|list|print-unit ...")?;
    let mut updates = PathBuf::from("updates");
    let mut bind = None;
    let mut cert = None;
    let mut key = None;
    let mut connect = None;
    let mut server_name = None;
    let mut ca_cert = None;
    let mut channel = None;
    let mut exec = None;
    let mut user = None;
    let mut group = None;
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--updates" => updates = PathBuf::from(value),
            "--bind" => bind = Some(value.parse()?),
            "--cert" => cert = Some(PathBuf::from(value)),
            "--key" => key = Some(PathBuf::from(value)),
            "--connect" => connect = Some(value.parse()?),
            "--server-name" => server_name = Some(value),
            "--ca-cert" => ca_cert = Some(PathBuf::from(value)),
            "--channel" => channel = Some(value.parse()?),
            "--exec" => exec = Some(PathBuf::from(value)),
            "--user" => user = Some(value),
            "--group" => group = Some(value),
            _ => return Err(format!("unknown option {flag}").into()),
        }
    }
    match command.as_str() {
        "serve" => Ok(Command::Serve {
            bind: bind.ok_or("serve requires --bind HOST:PORT")?,
            cert: cert.ok_or("serve requires --cert PATH")?,
            key: key.ok_or("serve requires --key PATH")?,
            updates,
        }),
        "status" | "list" => {
            let target = ClientTarget {
                connect: connect.ok_or("command requires --connect HOST:PORT")?,
                server_name: server_name.ok_or("command requires --server-name NAME")?,
                ca_cert,
            };
            if command == "status" {
                Ok(Command::Status(target))
            } else {
                Ok(Command::List(target))
            }
        }
        "print-unit" => Ok(Command::PrintUnit(UnitSpec {
            updates,
            channel: channel.ok_or("print-unit requires --channel prod|dev")?,
            exec: exec.ok_or("print-unit requires --exec PATH")?,
            cert: cert.ok_or("print-unit requires --cert PATH")?,
            key: key.ok_or("print-unit requires --key PATH")?,
            user: user.unwrap_or_else(|| "iw4l".to_string()),
            group: group.unwrap_or_else(|| "iw4l".to_string()),
        })),
        _ => Err(format!("unknown command {command}").into()),
    }
}

/// The systemd unit for `spec`, on stdout. The binary that parses `serve`
/// writes the `ExecStart` that invokes it: `cargo xtask master install` pipes
/// this straight into `/etc/systemd/system/`, so the two cannot disagree about
/// a flag, and `Channel` alone decides the port.
fn print_unit(spec: &UnitSpec) -> Result<()> {
    let UnitSpec {
        updates,
        channel,
        exec,
        cert,
        key,
        user,
        group,
    } = spec;
    let port = channel.port();
    let updates = updates.display();
    let exec = exec.display();
    let cert = cert.display();
    let key = key.display();
    write!(
        std::io::stdout(),
        "[Unit]
Description=IW4L {channel} master and relay
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User={user}
Group={group}
ExecStart={exec} serve --bind 0.0.0.0:{port} --cert {cert} --key {key} --updates {updates}
Restart=on-failure
RestartSec=2
NoNewPrivileges=true
PrivateDevices=true
PrivateTmp=true
ProtectHome=true
ProtectSystem=strict

[Install]
WantedBy=multi-user.target
"
    )?;
    Ok(())
}

async fn serve(bind: SocketAddr, cert_path: &Path, key_path: &Path) -> Result<()> {
    let certs = load_certificates(cert_path)?;
    let key = load_private_key(key_path)?;
    let mut crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    crypto.alpn_protocols = vec![ALPN.to_vec()];

    let mut config =
        quinn::ServerConfig::with_crypto(Arc::new(QuicServerConfig::try_from(crypto)?));
    let transport = Arc::get_mut(&mut config.transport)
        .ok_or("new server transport config unexpectedly shared")?;
    transport.max_concurrent_bidi_streams(1_u8.into());
    transport.max_concurrent_uni_streams(MAX_RELAY_UNI_STREAMS.into());
    transport.max_idle_timeout(Some(
        quinn::IdleTimeout::try_from(SESSION_IDLE).expect("session idle fits QUIC VarInt"),
    ));
    transport.keep_alive_interval(Some(SESSION_KEEP_ALIVE));
    transport.datagram_receive_buffer_size(Some(128 * 1024));
    transport.datagram_send_buffer_size(128 * 1024);

    let endpoint = quinn::Endpoint::server(config, bind)?;
    let state = Arc::new(Mutex::new(ServiceState::default()));
    let next_connection_id = Arc::new(AtomicU64::new(1));
    writeln!(
        std::io::stderr(),
        "iw4l-master listening on {}",
        endpoint.local_addr()?
    )?;
    writeln!(
        std::io::stderr(),
        "iw4l-master build={} wire={} alpn={:?}",
        env!("CARGO_PKG_VERSION"),
        master_protocol::PROTOCOL_VERSION,
        ALPN
    )?;
    let connection_slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    let address_counts = Arc::new(std::sync::Mutex::new(HashMap::new()));
    while let Some(incoming) = endpoint.accept().await {
        // No slot before the address is proven: a forged Initial would hold
        // one until its handshake timed out.
        if !incoming.remote_address_validated() && incoming.may_retry() {
            let _ = incoming.retry();
            continue;
        }
        let remote = incoming.remote_address();
        let Some(address_slot) = AddressSlot::claim(&address_counts, remote.ip()) else {
            let _ = writeln!(
                std::io::stderr(),
                "connection refused: {remote} at per-address cap {MAX_CONNECTIONS_PER_ADDRESS}"
            );
            incoming.refuse();
            continue;
        };
        let Ok(permit) = connection_slots.clone().try_acquire_owned() else {
            let _ = writeln!(
                std::io::stderr(),
                "connection refused: at connection cap {MAX_CONNECTIONS}"
            );
            incoming.refuse();
            continue;
        };
        let state = Arc::clone(&state);
        let connection_id = next_connection_id.fetch_add(1, Ordering::Relaxed);
        let started = Instant::now();
        let _ = writeln!(
            std::io::stderr(),
            "connection {connection_id} incoming remote={remote}"
        );
        tokio::spawn(async move {
            let _permit = permit;
            let _address_slot = address_slot;
            match incoming.await {
                Ok(connection) => {
                    let _ = writeln!(
                        std::io::stderr(),
                        "connection {connection_id} handshake ok remote={remote} elapsed_ms={}",
                        started.elapsed().as_millis()
                    );
                    handle_connection(state, connection_id, connection).await;
                }
                Err(error) => {
                    let _ = writeln!(
                        std::io::stderr(),
                        "connection {connection_id} handshake failed remote={remote} elapsed_ms={}: {error}",
                        started.elapsed().as_millis()
                    );
                }
            }
        });
    }
    Ok(())
}

async fn handle_connection(
    state: Arc<Mutex<ServiceState>>,
    connection_id: u64,
    connection: quinn::Connection,
) {
    let outcome = run_connection(&state, connection_id, connection.clone()).await;
    let seat = state
        .lock()
        .await
        .member_of(connection_id)
        .map(|(room_id, member_id)| format!("room={room_id} member={member_id}"))
        .unwrap_or_else(|_| "room=- member=-".to_owned());
    match outcome {
        Ok(()) => connection.close(CLOSE_SERVICE_DONE.into(), b"session closed"),
        Err(error) => {
            let reason = error.to_string();
            let _ = writeln!(
                std::io::stderr(),
                "connection {connection_id} closed: {reason} ({seat})"
            );
            let mut reason = reason.into_bytes();
            reason.truncate(MAX_CLOSE_REASON_BYTES);
            connection.close(CLOSE_SERVICE_ERROR.into(), &reason);
        }
    }
    let publishes = state.lock().await.disconnect(connection_id);
    dispatch_publishes(&state, publishes).await;
}

async fn run_connection(
    state: &Arc<Mutex<ServiceState>>,
    connection_id: u64,
    connection: quinn::Connection,
) -> Result<()> {
    let (mut send, recv) = tokio::time::timeout(HELLO_DEADLINE, connection.accept_bi())
        .await
        .map_err(|_| "hello deadline (accept_bi)")??;
    send.set_priority(CONTROL_PRIORITY)?;
    let (recv, hello) = tokio::time::timeout(HELLO_DEADLINE, read_owned_frame(recv))
        .await
        .map_err(|_| "hello deadline (first frame)")?;
    let hello = hello?;
    let ControlFrame::Hello(hello) = hello else {
        return Err("first control frame must be Hello".into());
    };
    let _ = writeln!(
        std::io::stderr(),
        "connection {connection_id} hello role={:?} wire={} game={} build={:?}",
        hello.role,
        hello.protocol_version,
        hello.game_protocol,
        hello.build
    );
    let (control_tx, mut control_rx) = tokio::sync::mpsc::channel::<ControlFrame>(CONTROL_CAP);
    state.lock().await.attach_peer(
        connection_id,
        normalize_player_name(&hello.player_name),
        connection.clone(),
        control_tx,
    );
    let mut children = tokio::task::JoinSet::new();
    children.spawn(async move {
        let result = async {
            while let Some(frame) = control_rx.recv().await {
                write_frame(&mut send, &frame).await?;
            }
            Result::<()>::Ok(())
        }
        .await;
        ConnTask::Writer(result)
    });
    let bootstrap_slots = Arc::new(Semaphore::new(MAX_CONCURRENT_BOOTSTRAP as usize));
    let mut next_control = Box::pin(read_owned_frame(recv));
    let outcome = loop {
        tokio::select! {
            (recv, frame) = &mut next_control => {
                next_control.set(read_owned_frame(recv));
                let frame = frame?;
                match frame {
                    ControlFrame::Request(request) => {
                        let outcome = handle_request(state, connection_id, request).await;
                        let mut publishes = outcome.publishes;
                        publishes.push((
                            connection_id,
                            ControlFrame::Response(outcome.response),
                        ));
                        dispatch_publishes(state, publishes).await;
                    }
                    ControlFrame::Relay(bytes) => {
                        let relay = decode_relay_stream(&bytes)?;
                        let current = state.lock().await;
                        let (target, outgoing) = current.route_target_stream(connection_id, &relay)?;
                        let peer = current.peers.values().find(|peer| peer.connection.stable_id() == target.stable_id()).ok_or(ServiceError::NotMember)?;
                        if peer.control_tx.try_send(ControlFrame::Relay(outgoing)).is_err() {
                            target.close(0_u8.into(), b"control queue exhausted");
                            break Err("control queue exhausted".into());
                        }
                    }
                    ControlFrame::Hello(_) => break Err("duplicate Hello".into()),
                    _ => break Err("client sent a non-request control frame".into()),
                }
            }
            stream = connection.accept_uni() => {
                let mut recv = stream?;
                let Ok(permit) = bootstrap_slots.clone().try_acquire_owned() else {
                    let _ = writeln!(
                        std::io::stderr(),
                        "connection {connection_id} bootstrap refused: no slot"
                    );
                    let _ = recv.stop(0_u8.into());
                    continue;
                };
                children.spawn(bootstrap_forward(
                    Arc::clone(state),
                    connection_id,
                    recv,
                    permit,
                ));
            }
            datagram = connection.read_datagram() => {
                let bytes = datagram?;
                if let Err(error) = relay_datagram(state, connection_id, &bytes).await {
                    let _ = writeln!(std::io::stderr(), "relay datagram: {error}");
                }
            }
            done = children.join_next() => {
                match done {
                    None => {}
                    Some(Ok(ConnTask::Writer(Err(error)))) => {
                        break Err(error);
                    }
                    Some(Ok(ConnTask::Writer(Ok(())))) => {
                        break Err("control writer ended".into());
                    }
                    Some(Ok(ConnTask::Bootstrap)) => {}
                    Some(Err(join)) => break Err(join.into()),
                }
            }
        }
    };
    children.abort_all();
    while children.join_next().await.is_some() {}
    outcome
}

async fn dispatch_publishes(state: &Mutex<ServiceState>, publishes: Vec<(u64, ControlFrame)>) {
    let mut failed = Vec::new();
    {
        let current = state.lock().await;
        for (connection_id, frame) in publishes {
            let Some(peer) = current.peers.get(&connection_id) else {
                continue;
            };
            match peer.control_tx.try_send(frame) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) | Err(TrySendError::Closed(_)) => {
                    failed.push(connection_id);
                    peer.connection
                        .close(0_u8.into(), b"control delivery failed");
                }
            }
        }
    }
    for connection_id in failed {
        let extra = state.lock().await.disconnect(connection_id);
        Box::pin(dispatch_publishes(state, extra)).await;
    }
}

async fn handle_request(
    state: &Mutex<ServiceState>,
    connection_id: u64,
    request: ControlRequest,
) -> HandleOutcome {
    let request_id = request.request_id;
    let mut state = state.lock().await;
    let (body, publishes) = match request.body {
        RequestBody::Status => (ResponseBody::Status(StatusResponse::CURRENT), Vec::new()),
        RequestBody::CreateRoom {
            name,
            map,
            mode,
            max_players,
            requires,
            available,
            password,
        } => create_room(
            &mut state,
            connection_id,
            name,
            map,
            mode,
            max_players,
            requires,
            available,
            password,
        ),
        RequestBody::JoinRoom {
            room_id,
            have,
            password,
        } => join_room(&mut state, connection_id, room_id, have, password),
        RequestBody::SetPassword { password } => match state.host_of(connection_id) {
            Ok(room_id) => {
                let room = state.rooms.get_mut(&room_id).expect("host room");
                room.password = PasswordVerifier::new(&password);
                room.view.password_protected = room.password.is_some();
                state.bump_room(room_id);
                (
                    ResponseBody::RoomUpdated {
                        view: state.rooms[&room_id].view.clone(),
                    },
                    state.publish_view(room_id),
                )
            }
            Err(error) => (ResponseBody::Error(error), Vec::new()),
        },
        RequestBody::LeaveRoom => leave_room(&mut state, connection_id),
        RequestBody::SetOptions {
            map,
            mode,
            joinable,
        } => set_options(&mut state, connection_id, map, mode, joinable),
        RequestBody::StartMatch { map, mode } => start_match(&mut state, connection_id, map, mode),
        RequestBody::EndMatch { room_id, epoch } => {
            end_match(&mut state, connection_id, room_id, epoch)
        }
        RequestBody::CloseRoom => match state.host_of(connection_id) {
            Ok(room_id) => (
                ResponseBody::Ack,
                state.close_room(room_id, SessionCloseReason::HostLeft),
            ),
            Err(error) => (ResponseBody::Error(error), Vec::new()),
        },
        RequestBody::ListRooms => {
            let mut adverts: Vec<Advert> = state
                .rooms
                .values()
                .map(|room| room.view.advert())
                .collect();
            adverts.sort_by_key(|advert| advert.id);
            adverts.truncate(MAX_LIST_ADVERTS);
            (
                ResponseBody::RoomList {
                    generation: state.generation,
                    adverts,
                },
                Vec::new(),
            )
        }
        RequestBody::HostWorldReady {
            room_id,
            epoch,
            map,
            weapons,
            classes,
        } => host_world_ready(
            &mut state,
            connection_id,
            room_id,
            epoch,
            map,
            weapons,
            classes,
        ),
        RequestBody::MapLoaded {
            epoch,
            map,
            weapons,
            classes,
        } => forward_to_host(&state, connection_id, |member_id| PeerEvent::MapLoaded {
            member_id,
            epoch,
            map,
            weapons,
            classes,
        }),
        RequestBody::BootstrapApplied {
            epoch,
            bootstrap_id,
            snapshot_seq,
            connection,
        } => forward_to_host(&state, connection_id, |member_id| {
            PeerEvent::BootstrapApplied {
                member_id,
                epoch,
                bootstrap_id,
                snapshot_seq,
                connection,
            }
        }),
        RequestBody::EnterMatch {
            member_id,
            epoch,
            bootstrap_id,
            connection_id: admitted_connection,
            client_id,
        } => enter_match(
            &state,
            connection_id,
            member_id,
            epoch,
            bootstrap_id,
            admitted_connection,
            client_id,
        ),
        RequestBody::AdmissionFailed {
            member_id,
            epoch,
            reason,
        } => admission_failed(&state, connection_id, member_id, epoch, reason),
        RequestBody::VoteToSkip => forward_to_host(&state, connection_id, |member_id| {
            PeerEvent::VoteToSkip { member_id }
        }),
    };
    HandleOutcome {
        response: ControlResponse { request_id, body },
        publishes,
    }
}

fn create_room(
    state: &mut ServiceState,
    connection_id: u64,
    name: String,
    map: String,
    mode: String,
    max_players: u8,
    requires: master_protocol::ContentFlags,
    available: master_protocol::ContentFlags,
    password: String,
) -> (ResponseBody, Vec<(u64, ControlFrame)>) {
    if state.membership.contains_key(&connection_id) {
        return (
            ResponseBody::Error(ServiceError::AlreadyHosting),
            Vec::new(),
        );
    }
    if state.rooms.len() >= MAX_ROOMS {
        return (
            ResponseBody::Error(ServiceError::AdvertCapacity),
            Vec::new(),
        );
    }
    let room_id = AdvertId(random_bytes());
    let member_id = MemberId(random_bytes());
    let view = RoomView {
        room_id,
        name,
        host: member_id,
        members: vec![member_id],
        member_names: [(member_id, state.peers[&connection_id].player_name.clone())].into(),
        map,
        mode,
        joinable: true,
        revision: 1,
        epoch: 0,
        phase: RoomPhase::Gathering,
        max_players,
        requires,
        available,
        password_protected: !password.is_empty(),
    };
    let mut member_of = HashMap::new();
    member_of.insert(connection_id, member_id);
    let mut connection_of = HashMap::new();
    connection_of.insert(member_id, connection_id);
    state.rooms.insert(
        room_id,
        Room {
            view: view.clone(),
            password: PasswordVerifier::new(&password),
            host_connection_id: connection_id,
            member_of,
            connection_of,
        },
    );
    state.membership.insert(connection_id, room_id);
    state.generation = state.generation.wrapping_add(1);
    (
        ResponseBody::RoomCreated {
            member_id,
            view: view.clone(),
        },
        vec![(connection_id, ControlFrame::RoomView(view))],
    )
}

fn join_room(
    state: &mut ServiceState,
    connection_id: u64,
    room_id: AdvertId,
    have: master_protocol::ContentFlags,
    password: String,
) -> (ResponseBody, Vec<(u64, ControlFrame)>) {
    if let Some(existing) = state.membership.get(&connection_id).copied() {
        if existing == room_id {
            let room = state.rooms.get(&room_id).expect("membership names a room");
            let member_id = *room.member_of.get(&connection_id).expect("host/member map");
            return (
                ResponseBody::RoomJoined {
                    member_id,
                    view: room.view.clone(),
                },
                Vec::new(),
            );
        }
        return (
            ResponseBody::Error(ServiceError::AlreadyHosting),
            Vec::new(),
        );
    }
    let Some(room) = state.rooms.get(&room_id) else {
        return (ResponseBody::Error(ServiceError::UnknownAdvert), Vec::new());
    };
    if room
        .password
        .as_ref()
        .is_some_and(|verifier| !verifier.matches(&password))
    {
        return (
            ResponseBody::Error(ServiceError::IncorrectPassword),
            Vec::new(),
        );
    }
    if !room.view.advert().allows_join_request() {
        return (ResponseBody::Error(ServiceError::Locked), Vec::new());
    }
    if !room.view.requires.missing_from(have).is_empty() {
        return (
            ResponseBody::Error(ServiceError::MissingContent {
                requires: room.view.requires,
                have,
            }),
            Vec::new(),
        );
    }
    if room.view.members.len() >= room.view.max_players as usize {
        return (ResponseBody::Error(ServiceError::Full), Vec::new());
    }
    let member_id = MemberId(random_bytes());
    let room = state.rooms.get_mut(&room_id).expect("room still present");
    room.member_of.insert(connection_id, member_id);
    room.connection_of.insert(member_id, connection_id);
    room.view.members.push(member_id);
    room.view
        .member_names
        .insert(member_id, state.peers[&connection_id].player_name.clone());
    room.view.revision = room.view.revision.wrapping_add(1).max(1);
    state.membership.insert(connection_id, room_id);
    state.generation = state.generation.wrapping_add(1);
    let view = room.view.clone();
    (
        ResponseBody::RoomJoined {
            member_id,
            view: view.clone(),
        },
        state.publish_view(room_id),
    )
}

fn leave_room(
    state: &mut ServiceState,
    connection_id: u64,
) -> (ResponseBody, Vec<(u64, ControlFrame)>) {
    let Some(room_id) = state.membership.get(&connection_id).copied() else {
        return (ResponseBody::RoomLeft, Vec::new());
    };
    let publishes = if state
        .rooms
        .get(&room_id)
        .is_some_and(|room| room.host_connection_id == connection_id)
    {
        state.close_room(room_id, SessionCloseReason::HostLeft)
    } else {
        state.remove_member(connection_id)
    };
    (ResponseBody::RoomLeft, publishes)
}

fn set_options(
    state: &mut ServiceState,
    connection_id: u64,
    map: String,
    mode: String,
    joinable: bool,
) -> (ResponseBody, Vec<(u64, ControlFrame)>) {
    match state.host_of(connection_id) {
        Ok(room_id) => {
            let room = state.rooms.get_mut(&room_id).expect("host room");
            if room.view.phase.in_match() && (room.view.map != map || room.view.mode != mode) {
                return (ResponseBody::Error(ServiceError::InvalidAdvert), Vec::new());
            }
            room.view.map = map;
            room.view.mode = mode;
            room.view.joinable = joinable;
            state.bump_room(room_id);
            let view = state.rooms[&room_id].view.clone();
            (
                ResponseBody::RoomUpdated { view: view.clone() },
                state.publish_view(room_id),
            )
        }
        Err(error) => (ResponseBody::Error(error), Vec::new()),
    }
}

fn start_match(
    state: &mut ServiceState,
    connection_id: u64,
    map: String,
    mode: String,
) -> (ResponseBody, Vec<(u64, ControlFrame)>) {
    match state.host_of(connection_id) {
        Ok(room_id) => {
            let room = state.rooms.get_mut(&room_id).expect("host room");
            if room.view.phase.in_match() && (room.view.map != map || room.view.mode != mode) {
                return (ResponseBody::Error(ServiceError::InvalidAdvert), Vec::new());
            }
            room.view.map = map;
            room.view.mode = mode;
            if room.view.phase.in_match() {
                return (
                    ResponseBody::RoomUpdated {
                        view: room.view.clone(),
                    },
                    Vec::new(),
                );
            } else {
                room.view.epoch = room.view.epoch.wrapping_add(1).max(1);
                room.view.phase = RoomPhase::Loading;
            }
            state.bump_room(room_id);
            let view = state.rooms[&room_id].view.clone();
            (
                ResponseBody::RoomUpdated { view: view.clone() },
                state.publish_view(room_id),
            )
        }
        Err(error) => (ResponseBody::Error(error), Vec::new()),
    }
}

fn end_match(
    state: &mut ServiceState,
    connection_id: u64,
    expected_room: AdvertId,
    epoch: u32,
) -> (ResponseBody, Vec<(u64, ControlFrame)>) {
    match state.host_of(connection_id) {
        Ok(room_id) => {
            let room = state.rooms.get_mut(&room_id).expect("host room");
            if room_id != expected_room
                || epoch == 0
                || room.view.epoch != epoch
                || !room.view.phase.in_match()
            {
                return (ResponseBody::Ack, Vec::new());
            }
            room.view.phase = RoomPhase::Gathering;
            state.bump_room(room_id);
            (ResponseBody::Ack, state.publish_view(room_id))
        }
        Err(error) => (ResponseBody::Error(error), Vec::new()),
    }
}

fn host_world_ready(
    state: &mut ServiceState,
    connection_id: u64,
    expected_room: AdvertId,
    epoch: u32,
    map: u64,
    weapons: u64,
    classes: u64,
) -> (ResponseBody, Vec<(u64, ControlFrame)>) {
    match state.host_of(connection_id) {
        Ok(room_id) => {
            let room = state.rooms.get_mut(&room_id).expect("host room");
            if room_id != expected_room
                || epoch == 0
                || room.view.epoch != epoch
                || !room.view.phase.in_match()
            {
                return (ResponseBody::Ack, Vec::new());
            }
            room.view.phase = RoomPhase::Running;
            state.bump_room(room_id);
            let mut publishes = state.publish_view(room_id);
            let event = ControlFrame::PeerEvent(PeerEvent::HostWorldReady {
                epoch,
                map,
                weapons,
                classes,
            });
            if let Some(room) = state.rooms.get(&room_id) {
                for peer in room.member_of.keys().copied() {
                    publishes.push((peer, event.clone()));
                }
            }
            (ResponseBody::Ack, publishes)
        }
        Err(error) => (ResponseBody::Error(error), Vec::new()),
    }
}

fn forward_to_host(
    state: &ServiceState,
    connection_id: u64,
    event: impl FnOnce(MemberId) -> PeerEvent,
) -> (ResponseBody, Vec<(u64, ControlFrame)>) {
    match state.member_of(connection_id) {
        Ok((room_id, member_id)) => {
            let Some(room) = state.rooms.get(&room_id) else {
                return (ResponseBody::Error(ServiceError::UnknownAdvert), Vec::new());
            };
            (
                ResponseBody::Ack,
                vec![(
                    room.host_connection_id,
                    ControlFrame::PeerEvent(event(member_id)),
                )],
            )
        }
        Err(error) => (ResponseBody::Error(error), Vec::new()),
    }
}

fn enter_match(
    state: &ServiceState,
    connection_id: u64,
    member_id: MemberId,
    epoch: u32,
    bootstrap_id: u32,
    admitted_connection: u64,
    client_id: u32,
) -> (ResponseBody, Vec<(u64, ControlFrame)>) {
    match state.host_of(connection_id) {
        Ok(room_id) => {
            let Some(room) = state.rooms.get(&room_id) else {
                return (ResponseBody::Error(ServiceError::UnknownAdvert), Vec::new());
            };
            let Some(&target) = room.connection_of.get(&member_id) else {
                return (ResponseBody::Error(ServiceError::NotMember), Vec::new());
            };
            (
                ResponseBody::Ack,
                vec![(
                    target,
                    ControlFrame::PeerEvent(PeerEvent::EnterMatch {
                        member_id,
                        epoch,
                        bootstrap_id,
                        connection_id: admitted_connection,
                        client_id,
                    }),
                )],
            )
        }
        Err(error) => (ResponseBody::Error(error), Vec::new()),
    }
}

fn admission_failed(
    state: &ServiceState,
    connection_id: u64,
    member_id: MemberId,
    epoch: u32,
    reason: AdmissionFailure,
) -> (ResponseBody, Vec<(u64, ControlFrame)>) {
    match state.host_of(connection_id) {
        Ok(room_id) => {
            let Some(room) = state.rooms.get(&room_id) else {
                return (ResponseBody::Error(ServiceError::UnknownAdvert), Vec::new());
            };
            let Some(&target) = room.connection_of.get(&member_id) else {
                return (ResponseBody::Error(ServiceError::NotMember), Vec::new());
            };
            (
                ResponseBody::Ack,
                vec![(
                    target,
                    ControlFrame::PeerEvent(PeerEvent::AdmissionFailed {
                        member_id,
                        epoch,
                        reason,
                    }),
                )],
            )
        }
        Err(error) => (ResponseBody::Error(error), Vec::new()),
    }
}

async fn bootstrap_forward(
    state: Arc<Mutex<ServiceState>>,
    connection_id: u64,
    mut recv: quinn::RecvStream,
    _permit: tokio::sync::OwnedSemaphorePermit,
) -> ConnTask {
    let result = tokio::time::timeout(BOOTSTRAP_FORWARD, async {
        let bytes = recv.read_to_end(MAX_BOOTSTRAP_STREAM_BYTES).await?;
        let relay = decode_relay_stream(&bytes)?;
        let (target, outgoing) = {
            let current = state.lock().await;
            current.route_target_stream(connection_id, &relay)?
        };
        let mut send = target.open_uni().await?;
        send.set_priority(BOOTSTRAP_PRIORITY)?;
        send.write_all(&outgoing).await?;
        send.finish()?;
        Result::<()>::Ok(())
    })
    .await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            let _ = writeln!(std::io::stderr(), "bootstrap forward: {error}");
        }
        Err(_) => {
            let _ = writeln!(std::io::stderr(), "bootstrap forward: transfer deadline");
        }
    }
    ConnTask::Bootstrap
}

async fn relay_datagram(
    state: &Mutex<ServiceState>,
    connection_id: u64,
    bytes: &[u8],
) -> Result<()> {
    let relay = decode_relay(bytes)?;
    let (target, outgoing) = {
        let current = state.lock().await;
        current.route_target(connection_id, &relay)?
    };
    target.send_datagram(outgoing.into())?;
    Ok(())
}

async fn status(target: &ClientTarget) -> Result<()> {
    let (_endpoint, connection) = connect_client(target).await?;
    let response = rpc(
        &connection,
        ControlRequest {
            request_id: 1,
            body: RequestBody::Status,
        },
        EndpointRole::Cli,
    )
    .await?;
    match response.body {
        ResponseBody::Status(status) if status == StatusResponse::CURRENT => {
            writeln!(
                std::io::stdout(),
                "protocol={} opaque={} members={}",
                status.protocol_version,
                status.max_opaque_payload,
                status.max_session_members
            )?;
        }
        body => return Err(format!("unexpected Status response: {body:?}").into()),
    }
    connection.close(0_u8.into(), b"status complete");
    Ok(())
}

async fn list(target: &ClientTarget) -> Result<()> {
    let (_endpoint, connection) = connect_client(target).await?;
    let response = rpc(
        &connection,
        ControlRequest {
            request_id: 1,
            body: RequestBody::ListRooms,
        },
        EndpointRole::Cli,
    )
    .await?;
    match response.body {
        ResponseBody::RoomList {
            generation,
            adverts,
        } => {
            let mut output = std::io::stdout().lock();
            writeln!(output, "generation={generation} adverts={}", adverts.len())?;
            for advert in adverts {
                writeln!(
                    output,
                    "{}\t{}/{}\t{}\t{}\tlocked={}\tin_match={}\tgen={}\trequires=0x{:02x}\t{}",
                    advert.id,
                    advert.players,
                    advert.max_players,
                    advert.map,
                    advert.mode,
                    u8::from(advert.locked),
                    u8::from(advert.in_match),
                    advert.generation,
                    advert.requires.0,
                    advert.name
                )?;
            }
        }
        body => return Err(format!("unexpected List response: {body:?}").into()),
    }
    connection.close(0_u8.into(), b"list complete");
    Ok(())
}

async fn connect_client(target: &ClientTarget) -> Result<(quinn::Endpoint, quinn::Connection)> {
    let mut crypto = if let Some(path) = target.ca_cert.as_deref() {
        let mut roots = rustls::RootCertStore::empty();
        for cert in load_certificates(path)? {
            roots.add(cert)?;
        }
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth()
    } else {
        rustls::ClientConfig::with_platform_verifier()?
    };
    crypto.alpn_protocols = vec![ALPN.to_vec()];
    let client_config = quinn::ClientConfig::new(Arc::new(QuicClientConfig::try_from(crypto)?));
    let bind = if target.connect.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let mut endpoint = quinn::Endpoint::client(bind.parse()?)?;
    endpoint.set_default_client_config(client_config);
    let connection = endpoint
        .connect(target.connect, &target.server_name)?
        .await?;
    Ok((endpoint, connection))
}

async fn rpc(
    connection: &quinn::Connection,
    request: ControlRequest,
    role: EndpointRole,
) -> Result<ControlResponse> {
    let request_id = request.request_id;
    let (mut send, mut recv) = connection.open_bi().await?;
    send.set_priority(CONTROL_PRIORITY)?;
    write_frame(
        &mut send,
        &ControlFrame::Hello(ControlHello {
            protocol_version: master_protocol::PROTOCOL_VERSION,
            game_protocol: 0,
            role,
            build: "iw4l-master".into(),
            player_name: String::new(),
        }),
    )
    .await?;
    write_frame(&mut send, &ControlFrame::Request(request)).await?;
    loop {
        match read_frame(&mut recv).await? {
            ControlFrame::Response(response) if response.request_id == request_id => {
                return Ok(response);
            }
            ControlFrame::RoomView(_)
            | ControlFrame::Closed { .. }
            | ControlFrame::PeerEvent(_)
            | ControlFrame::Relay(_) => {
                continue;
            }
            other => {
                return Err(format!(
                    "unexpected control frame while waiting for response: {other:?}"
                )
                .into());
            }
        }
    }
}

async fn read_owned_frame(
    mut recv: quinn::RecvStream,
) -> (quinn::RecvStream, Result<ControlFrame>) {
    let frame = read_frame(&mut recv).await;
    (recv, frame)
}

async fn read_frame(recv: &mut quinn::RecvStream) -> Result<ControlFrame> {
    let mut header = [0_u8; 4];
    recv.read_exact(&mut header).await?;
    let len = stream_frame_len(header)?;
    let mut body = vec![0_u8; len];
    recv.read_exact(&mut body).await?;
    Ok(decode_stream_payload(&body)?)
}

async fn write_frame(send: &mut quinn::SendStream, frame: &ControlFrame) -> Result<()> {
    send.write_all(&encode_stream_frame(frame)?).await?;
    Ok(())
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).expect("OS randomness unavailable");
    bytes
}

fn load_certificates(path: &Path) -> Result<Vec<CertificateDer<'static>>> {
    let mut reader = BufReader::new(File::open(path)?);
    let certs: Vec<_> = rustls_pemfile::certs(&mut reader).collect::<std::io::Result<_>>()?;
    if certs.is_empty() {
        return Err(format!("{} contains no certificates", path.display()).into());
    }
    Ok(certs)
}

fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>> {
    let mut reader = BufReader::new(File::open(path)?);
    rustls_pemfile::private_key(&mut reader)?
        .ok_or_else(|| format!("{} contains no private key", path.display()).into())
}

fn normalize_player_name(name: &str) -> String {
    let name: String = name
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(16)
        .collect();
    if name.is_empty() {
        "Player".into()
    } else {
        name
    }
}
