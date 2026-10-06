use std::collections::{HashMap, HashSet};
use std::fmt;
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use master_protocol::{
    ALPN, AdmissionFailure, AdvertId, ContentFlags, ControlFrame, ControlHello, ControlRequest,
    ControlResponse, EndpointRole, MAX_BOOTSTRAP_STREAM_BYTES, MAX_CONCURRENT_BOOTSTRAP,
    MAX_RELAY_UNI_STREAMS, MemberId, RelayDatagram, RequestBody, ResponseBody, RoomView,
    SESSION_IDLE, SESSION_KEEP_ALIVE, SessionCloseReason, decode_relay, decode_relay_stream,
    decode_stream_payload, encode_relay, encode_relay_stream, encode_stream_frame,
    stream_frame_len,
};
use quinn::crypto::rustls::QuicClientConfig;
use tokio_util::sync::CancellationToken;

use crate::authority::runtime::AuthorityWorld;
use crate::session_core::{
    HostMatchApply, HostMatchCore, HostMatchEffect, HostMatchEvent, HostWorldReady, SessionCore,
    SessionEvent,
};
use crate::transport::bootstrap::{
    BootstrapAck, BootstrapLane, BootstrapMessage, decode_bootstrap, epoch_applies,
};
use crate::transport::fragment::{Fragmenter, Reassembler};
use crate::transport::protocol::{ContentFingerprint, HandshakeHello, MatchDescriptor};
use crate::transport::udp_session::{RelayMailbox, UdpAuthorityHub, UdpClientLink};

type Error = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, Error>;

pub const CONTENT_IW4: u8 = 1 << 0;
pub const CONTENT_IW5: u8 = 1 << 1;
pub const CONTENT_T5: u8 = 1 << 2;
pub const CONTENT_T6: u8 = 1 << 3;

pub const fn content_inventory(iw4: bool, iw5: bool, t5: bool, t6: bool) -> ContentFlags {
    ContentFlags(
        (if iw4 { CONTENT_IW4 } else { 0 })
            | (if iw5 { CONTENT_IW5 } else { 0 })
            | (if t5 { CONTENT_T5 } else { 0 })
            | (if t6 { CONTENT_T6 } else { 0 }),
    )
}

pub fn content_required_by_map(map: &str) -> Result<ContentFlags> {
    let namespace = map
        .split_once(':')
        .map_or("iw4", |(namespace, _)| namespace);
    Ok(ContentFlags(match namespace {
        // A Minecraft map stands on an IW4 map for its weapons and bodies.
        "iw4" | "minecraft" => CONTENT_IW4,
        "iw5" => CONTENT_IW5,
        "t5" => CONTENT_T5,
        "t6" => CONTENT_T6,
        other => return Err(format!("unknown content namespace `{other}` in map `{map}`").into()),
    }))
}

pub fn content_names(flags: ContentFlags) -> String {
    let mut names = Vec::new();
    if flags.0 & CONTENT_IW4 != 0 {
        names.push("iw4");
    }
    if flags.0 & CONTENT_IW5 != 0 {
        names.push("iw5");
    }
    if flags.0 & CONTENT_T5 != 0 {
        names.push("t5");
    }
    if flags.0 & CONTENT_T6 != 0 {
        names.push("t6");
    }
    if flags.0 & !(CONTENT_IW4 | CONTENT_IW5 | CONTENT_T5 | CONTENT_T6) != 0 {
        names.push("unknown");
    }
    names.join(",")
}

const RELAY_PACKET_BYTES: usize = crate::transport::protocol::MAX_PACKET_BYTES as usize;
const HOST_DATA_CAP: usize = 64;
const HOST_CONTROL_CAP: usize = 32;

const BOOTSTRAP_PRIORITY: i32 = -32;
const IO_DEADLINE: Duration = Duration::from_secs(8);
const LEAVE_DRAIN: Duration = Duration::from_millis(500);
const DATAGRAM_SEND_BUFFER: usize = 1024 * 1024;
const QUEUE_POLL: Duration = Duration::from_millis(5);
const WATCHDOG_INTERVAL: Duration = Duration::from_secs(1);

fn blocking_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn endpoint_build() -> String {
    format!(
        "iw4l-net/{} game={} master={}",
        env!("CARGO_PKG_VERSION"),
        crate::PROTOCOL_VERSION,
        master_protocol::PROTOCOL_VERSION
    )
}

fn relay_fragmenter() -> Fragmenter {
    Fragmenter::new(master_protocol::MAX_OPAQUE_PAYLOAD, RELAY_PACKET_BYTES)
}

fn relay_reassembler() -> Reassembler {
    Reassembler::new(master_protocol::MAX_OPAQUE_PAYLOAD, RELAY_PACKET_BYTES)
}

fn take_commands(commands: &Mutex<Vec<MasterBridgeCommand>>) -> Vec<MasterBridgeCommand> {
    let mut queue = commands.lock().expect("master command queue poisoned");
    std::mem::take(&mut *queue)
}

fn classify_try_send<T>(
    result: std::result::Result<(), tokio::sync::mpsc::error::TrySendError<T>>,
    operation: &'static str,
) -> std::result::Result<(), QueueLoss> {
    match result {
        Ok(()) => Ok(()),
        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => Err(QueueLoss::Full { operation }),
        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
            Err(QueueLoss::Closed { operation })
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueueLoss {
    Full { operation: &'static str },
    Closed { operation: &'static str },
}

impl fmt::Display for QueueLoss {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Full { operation } => write!(f, "{operation} queue full"),
            Self::Closed { operation } => write!(f, "{operation} queue closed"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionIdentity {
    pub attempt_id: u64,
    pub room_id: AdvertId,
    pub member_id: MemberId,
    pub epoch: u32,
}

impl SessionIdentity {
    pub const fn unassigned(attempt_id: u64) -> Self {
        Self {
            attempt_id,
            room_id: AdvertId([0; 16]),
            member_id: MemberId([0; 16]),
            epoch: 0,
        }
    }

    pub fn match_key(&self) -> frame::MatchKey {
        if self.epoch == 0 || self.room_id.0 == [0; 16] {
            frame::MatchKey::NONE
        } else {
            frame::MatchKey::new(self.room_id.0, self.epoch)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportFault {
    pub operation: &'static str,
    pub role: &'static str,
    pub source: String,
    pub close_reason: Option<String>,
}

impl TransportFault {
    pub fn new(operation: &'static str, role: &'static str, source: impl Into<String>) -> Self {
        Self {
            operation,
            role,
            source: source.into(),
            close_reason: None,
        }
    }

    fn with_connection(
        operation: &'static str,
        role: &'static str,
        source: impl fmt::Display,
        connection: &quinn::Connection,
    ) -> Self {
        Self {
            operation,
            role,
            source: source.to_string(),
            close_reason: connection.close_reason().map(|error| error.to_string()),
        }
    }
}

impl fmt::Display for TransportFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}: {}", self.role, self.operation, self.source)?;
        if let Some(close) = &self.close_reason {
            write!(f, " (quic close: {close})")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MasterLifecycleFact {
    MemberLeft {
        member_id: MemberId,
    },
    SessionClosed {
        reason: SessionCloseReason,
    },

    AdmissionFailed {
        member_id: MemberId,
        reason: AdmissionFailure,
    },
}

fn observe_live_start(in_match: bool, start_nonce: u32) -> bool {
    in_match && start_nonce != 0
}

fn push_fact(facts: &Mutex<Vec<MasterLifecycleFact>>, fact: MasterLifecycleFact) {
    facts.lock().expect("master facts poisoned").push(fact);
}

fn forget_relay_member(
    member_id: MemberId,
    members: &mut HashSet<MemberId>,
    skip_voters: &mut HashSet<MemberId>,
    map_ready: &Mutex<HashSet<MemberId>>,
    facts: &Mutex<Vec<MasterLifecycleFact>>,
) -> bool {
    if !members.remove(&member_id) {
        return false;
    }
    skip_voters.remove(&member_id);
    map_ready
        .lock()
        .expect("bootstrap readiness poisoned")
        .remove(&member_id);
    push_fact(facts, MasterLifecycleFact::MemberLeft { member_id });
    true
}

#[derive(Clone, Debug)]
struct MasterTarget {
    address: String,
    server_name: String,
    ca_pem: String,
}

fn master_target() -> Result<Option<MasterTarget>> {
    if let Some(community) = updater::selected() {
        return Ok(Some(MasterTarget {
            address: community.master.address.clone(),
            server_name: community.master.server_name.clone(),
            ca_pem: community.updates.ca_pem.clone(),
        }));
    }
    Ok(None)
}

#[derive(Clone, Debug)]
struct HostConfig {
    password: String,
    auto_start_map: bool,
    target: MasterTarget,
    name: String,
    map: String,
    mode: String,
    max_players: u8,
    requires: ContentFlags,
    have: ContentFlags,
}

#[derive(Clone, Debug)]
struct JoinConfig {
    password: String,
    target: MasterTarget,
    advert_id: AdvertId,
    map: String,
    mode: String,
    have: ContentFlags,
}

#[derive(Clone, Debug)]
struct BrowserConfig {
    target: MasterTarget,
    have: ContentFlags,
}

#[derive(Clone, Debug)]
enum MasterLaunchMode {
    Disabled,
    Browser(BrowserConfig),
    Host(HostConfig),
    Join(JoinConfig),
}

#[derive(Resource, Clone, Debug)]
pub struct MasterLaunchIntent(MasterLaunchMode);

impl MasterLaunchIntent {
    pub fn browser_from_env(have: ContentFlags) -> Result<Self> {
        let Some(target) = master_target()? else {
            return Ok(Self::disabled());
        };
        Ok(Self(MasterLaunchMode::Browser(BrowserConfig {
            target,
            have,
        })))
    }

    pub fn from_env_for_map(map: &str, have: ContentFlags, requires: ContentFlags) -> Result<Self> {
        let Some(target) = master_target()? else {
            return Ok(Self::disabled());
        };
        let host = std::env::var("IW4L_MASTER_HOST_NAME").ok();
        let join = std::env::var("IW4L_MASTER_JOIN").ok();
        match (host, join) {
            (Some(name), None) if !name.trim().is_empty() => {
                Ok(Self(MasterLaunchMode::Host(HostConfig {
                    password: std::env::var("IW4L_MASTER_PASSWORD").unwrap_or_default(),
                    auto_start_map: true,
                    target,
                    name,
                    map: map.to_owned(),
                    mode: std::env::var("IW4L_GAMETYPE").unwrap_or_else(|_| "dm".into()),
                    max_players: parse_max_players()?,
                    requires,
                    have,
                })))
            }
            (None, Some(advert_id)) => Ok(Self(MasterLaunchMode::Join(JoinConfig {
                password: std::env::var("IW4L_MASTER_PASSWORD").unwrap_or_default(),
                target,
                advert_id: advert_id.parse()?,
                map: map.to_owned(),
                mode: std::env::var("IW4L_GAMETYPE").unwrap_or_else(|_| "dm".into()),
                have,
            }))),
            (Some(_), Some(_)) => {
                Err("set only one of IW4L_MASTER_HOST_NAME or IW4L_MASTER_JOIN".into())
            }
            _ => Err("community master requires IW4L_MASTER_HOST_NAME or IW4L_MASTER_JOIN".into()),
        }
    }

    pub const fn is_join(&self) -> bool {
        matches!(self.0, MasterLaunchMode::Join(_))
    }

    pub const fn enabled(&self) -> bool {
        !matches!(self.0, MasterLaunchMode::Disabled)
    }

    pub const fn disabled() -> Self {
        Self(MasterLaunchMode::Disabled)
    }

    pub const fn configured(&self) -> bool {
        !matches!(self.0, MasterLaunchMode::Disabled)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MasterAdvert {
    pub id: AdvertId,
    pub name: String,
    pub map: String,
    pub mode: String,
    pub players: u8,
    pub max_players: u8,
    pub locked: bool,
    pub in_match: bool,
    pub requires: ContentFlags,
    pub available: ContentFlags,
    pub password_protected: bool,
    pub missing: ContentFlags,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MasterBrowserSnapshot {
    pub community_name: String,
    pub ping_ms: Option<u64>,
    pub generation: u64,
    pub loading: bool,
    pub adverts: Vec<MasterAdvert>,
    pub error: Option<String>,
    pub have: ContentFlags,
}

#[derive(Resource)]
pub struct MasterBrowser {
    state: Arc<Mutex<MasterBrowserSnapshot>>,
    refresh: Arc<AtomicU64>,
    cancel: CancellationToken,
    worker: Option<JoinHandle<()>>,
}

impl MasterBrowser {
    pub fn snapshot(&self) -> MasterBrowserSnapshot {
        self.state
            .lock()
            .expect("master browser state poisoned")
            .clone()
    }

    pub fn refresh(&self) {
        self.refresh.fetch_add(1, Ordering::Relaxed);
    }
}

impl Drop for MasterBrowser {
    fn drop(&mut self) {
        self.cancel.cancel();
        let _ = self.worker.take();
    }
}

#[derive(Clone, Debug)]
pub enum MasterMenuAction {
    Refresh,
    SetPassword {
        password: String,
    },
    Host {
        password: String,
        map: String,
        mode: String,
    },
    Join {
        password: String,
        advert_id: AdvertId,
        map: String,
        mode: String,
    },
    UpdateLobby {
        map: String,
        mode: String,
    },
    StartMatch {
        map: String,
        mode: String,
    },
    VoteToSkip,
    LeaveLobby,
}

#[derive(Clone, Debug)]
enum MasterBridgeCommand {
    SetPassword {
        password: String,
    },
    UpdateLobby {
        map: String,
        mode: String,
    },
    StartMatch {
        map: String,
        mode: String,
    },
    VoteToSkip,
    MapLoaded {
        epoch: u32,
        map: u64,
        weapons: u64,
        classes: u64,
    },
    HostWorldReady {
        epoch: u32,
        map: u64,
        weapons: u64,
        classes: u64,
        #[allow(dead_code)]
        load_key: frame::LocalLoadKey,
    },
    MatchEnded {
        match_key: frame::MatchKey,
    },
    AuthorityProgress,
    AdmitEnter {
        member_id: MemberId,
        epoch: u32,
        bootstrap_id: u32,
        connection_id: Option<u64>,
        client_id: u32,
    },
    Shutdown,
}

#[derive(Resource, Default)]
pub struct PendingMasterMenuAction(pub Option<MasterMenuAction>);

fn parse_max_players() -> Result<u8> {
    match std::env::var("IW4L_MASTER_MAX_PLAYERS") {
        Ok(raw) => {
            let value = raw.parse::<u8>()?;
            if !(2..=master_protocol::MAX_SESSION_MEMBERS).contains(&value) {
                return Err(format!("IW4L_MASTER_MAX_PLAYERS={value} must be 2..=18").into());
            }
            Ok(value)
        }
        Err(_) => Ok(master_protocol::MAX_SESSION_MEMBERS),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MasterBridgeState {
    Connecting {
        identity: SessionIdentity,
    },
    Hosting {
        identity: SessionIdentity,
        name: String,
        map: String,
        mode: String,
        members: Vec<MemberId>,
        member_names: std::collections::HashMap<MemberId, String>,
        max_players: u8,
        skip_votes: u8,
        in_match: bool,
    },
    Joining {
        identity: SessionIdentity,
    },
    Joined {
        identity: SessionIdentity,
        name: String,
        map: String,
        mode: String,
        members: Vec<MemberId>,
        member_names: std::collections::HashMap<MemberId, String>,
        max_players: u8,
        skip_votes: u8,
        in_match: bool,
    },
    Closed {
        identity: SessionIdentity,
        reason: SessionCloseReason,
    },
    Left {
        identity: SessionIdentity,
    },
    Failed {
        identity: SessionIdentity,
        error: TransportFault,
    },
}

impl MasterBridgeState {
    pub fn identity(&self) -> SessionIdentity {
        match *self {
            Self::Connecting { identity }
            | Self::Hosting { identity, .. }
            | Self::Joining { identity }
            | Self::Joined { identity, .. }
            | Self::Closed { identity, .. }
            | Self::Left { identity }
            | Self::Failed { identity, .. } => identity,
        }
    }

    pub fn map(&self) -> Option<&str> {
        match self {
            Self::Hosting { map, .. } | Self::Joined { map, .. } => Some(map),
            _ => None,
        }
    }

    pub fn mode(&self) -> Option<&str> {
        match self {
            Self::Hosting { mode, .. } | Self::Joined { mode, .. } => Some(mode),
            _ => None,
        }
    }

    pub fn members(&self) -> &[MemberId] {
        match self {
            Self::Hosting { members, .. } | Self::Joined { members, .. } => members,
            _ => &[],
        }
    }

    pub fn skip_votes(&self) -> u8 {
        match *self {
            Self::Hosting { skip_votes, .. } | Self::Joined { skip_votes, .. } => skip_votes,
            _ => 0,
        }
    }

    pub fn in_match(&self) -> bool {
        match *self {
            Self::Hosting { in_match, .. } | Self::Joined { in_match, .. } => in_match,
            _ => false,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Failed { .. } | Self::Closed { .. } | Self::Left { .. }
        )
    }
}

#[derive(Resource)]
pub struct MasterBridge {
    state: Arc<Mutex<MasterBridgeState>>,
    commands: Arc<Mutex<Vec<MasterBridgeCommand>>>,
    installed_load: Arc<Mutex<Option<frame::LocalLoadKey>>>,
    mailbox: RelayMailbox,
    bootstrap: Arc<BootstrapLane>,
    close: CancellationToken,
    facts: Arc<Mutex<Vec<MasterLifecycleFact>>>,
    incarnation: u64,
    worker: Option<JoinHandle<()>>,
}

impl MasterBridge {
    pub fn state(&self) -> MasterBridgeState {
        self.state.lock().expect("master state poisoned").clone()
    }

    pub fn fail(&self, reason: &str) {
        publish_failed(
            &self.state,
            self.state().identity(),
            TransportFault::new("gameplay", "local", reason),
        );
        self.request_close();
    }

    fn request_close(&self) {
        self.send(MasterBridgeCommand::Shutdown);
        self.close.cancel();
    }

    pub fn mailbox(&self) -> RelayMailbox {
        self.mailbox.clone()
    }

    fn send(&self, command: MasterBridgeCommand) {
        self.commands
            .lock()
            .expect("master command queue poisoned")
            .push(command);
    }

    pub fn set_installed_load(&self, load: Option<frame::LocalLoadKey>) {
        *self.installed_load.lock().expect("installed load poisoned") = load;
    }

    pub fn report_map_loaded(&self, epoch: u32, descriptor: crate::MatchDescriptor) {
        self.send(MasterBridgeCommand::MapLoaded {
            epoch,
            map: descriptor.map,
            weapons: descriptor.weapons,
            classes: descriptor.classes,
        });
    }

    pub fn report_host_world_ready(
        &self,
        epoch: u32,
        descriptor: crate::MatchDescriptor,
        load_key: frame::LocalLoadKey,
    ) {
        self.send(MasterBridgeCommand::HostWorldReady {
            epoch,
            map: descriptor.map,
            weapons: descriptor.weapons,
            classes: descriptor.classes,
            load_key,
        });
    }

    pub fn report_authority_progress(&self) {
        self.send(MasterBridgeCommand::AuthorityProgress);
    }

    pub fn report_match_ended(&self, match_key: frame::MatchKey) {
        self.send(MasterBridgeCommand::MatchEnded { match_key });
    }

    pub fn start_hosted_match(&self, map: String, mode: String) {
        self.send(MasterBridgeCommand::StartMatch { map, mode });
    }

    pub fn leave(&self) {
        self.request_close();
    }

    pub fn admit_enter(
        &self,
        member_id: MemberId,
        epoch: u32,
        bootstrap_id: u32,
        connection_id: Option<u64>,
        client_id: u32,
    ) {
        self.send(MasterBridgeCommand::AdmitEnter {
            member_id,
            epoch,
            bootstrap_id,
            connection_id,
            client_id,
        });
    }

    pub fn incarnation(&self) -> u64 {
        self.incarnation
    }

    pub fn drain_facts(&self) -> Vec<MasterLifecycleFact> {
        std::mem::take(&mut *self.facts.lock().expect("master facts poisoned"))
    }
}

impl Drop for MasterBridge {
    fn drop(&mut self) {
        if let Ok(mut queue) = self.commands.lock() {
            queue.push(MasterBridgeCommand::Shutdown);
        }
        self.close.cancel();
        let _ = self.worker.take();
    }
}

#[derive(Resource, Default)]
pub struct MasterMatchStart(Option<MasterMatchOffer>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MasterMatchOffer {
    pub map: String,
    pub mode: String,
    pub match_key: frame::MatchKey,
}

impl MasterMatchStart {
    pub fn take(&mut self) -> Option<MasterMatchOffer> {
        self.0.take()
    }
}

pub fn arm_master_bridge(
    settings: Res<frame::GameSettings>,
    intent: Res<MasterLaunchIntent>,
    role: Res<frame::RuntimeRole>,
    authority: Option<Res<AuthorityWorld>>,
    prediction: Option<Res<crate::ClientPredictionState>>,
    descriptor: Option<Res<crate::MatchDescriptor>>,
    bridge: Option<Res<MasterBridge>>,
    udp_hub: Option<Res<UdpAuthorityHub>>,
    udp_link: Option<Res<UdpClientLink>>,
    mut commands: Commands,
) {
    if let Some(bridge) = bridge {
        let Some(descriptor) = descriptor.as_deref() else {
            return;
        };
        let world = authority
            .as_ref()
            .map(|authority| &authority.0)
            .or_else(|| prediction.as_ref().map(|prediction| prediction.0.world()));
        if world.is_none_or(|world| !world.has_world_clip()) {
            return;
        }
        let Some(mut hello) = handshake_for_match(world, Some(descriptor)) else {
            return;
        };
        hello.limits.max_packet_bytes = RELAY_PACKET_BYTES as u32;
        if udp_hub.is_none() && matches!(bridge.state(), MasterBridgeState::Hosting { .. }) {
            let mut hub = UdpAuthorityHub::relay(hello, 1, bridge.mailbox());
            hub.attach_bootstrap(Arc::clone(&bridge.bootstrap));
            commands.insert_resource(hub);
        }
        if udp_link.is_none()
            && matches!(
                bridge.state(),
                MasterBridgeState::Joining { .. } | MasterBridgeState::Joined { .. }
            )
        {
            let mut link = UdpClientLink::relay(hello, bridge.mailbox());
            link.attach_bootstrap(Arc::clone(&bridge.bootstrap));
            diag::info!(
                Net,
                "master client relay mailbox — protocol={} offering map={:016x} weapons={:016x} classes={:016x}",
                crate::PROTOCOL_VERSION,
                hello.content.map,
                hello.content.weapons,
                hello.content.classes
            );
            commands.insert_resource(link);
        }
        return;
    }
    match &intent.0 {
        MasterLaunchMode::Disabled | MasterLaunchMode::Browser(_) => {}
        MasterLaunchMode::Host(config) => {
            if *role != frame::RuntimeRole::Listen || udp_hub.is_some() {
                return;
            }
            let relay = spawn_host(config.clone(), settings.player_name.clone());
            diag::info!(
                Net,
                "master public lobby arming for {}",
                config.target.address
            );
            commands.insert_resource(relay);
        }
        MasterLaunchMode::Join(config) => {
            if *role != frame::RuntimeRole::Client || udp_link.is_some() {
                return;
            }
            let relay = spawn_join(config.clone(), settings.player_name.clone());
            diag::info!(
                Net,
                "master joining lobby advert {} through {} ({}/{})",
                config.advert_id,
                config.target.address,
                config.map,
                config.mode
            );
            commands.insert_resource(relay);
        }
    }
}

fn arm_master_browser(
    intent: Res<MasterLaunchIntent>,
    browser: Option<Res<MasterBrowser>>,
    mut commands: Commands,
) {
    let MasterLaunchMode::Browser(browser_config) = &intent.0 else {
        if browser.is_some() {
            commands.remove_resource::<MasterBrowser>();
        }
        return;
    };
    if browser.is_some() {
        return;
    }
    let state = Arc::new(Mutex::new(MasterBrowserSnapshot {
        community_name: updater::selected()
            .map(|community| community.name.clone())
            .unwrap_or_default(),
        loading: true,
        have: browser_config.have,
        ..default()
    }));
    let refresh = Arc::new(AtomicU64::new(0));
    let cancel = CancellationToken::new();
    let worker_state = Arc::clone(&state);
    let worker_refresh = Arc::clone(&refresh);
    let worker_cancel = cancel.clone();
    let target = browser_config.target.clone();

    let worker = std::thread::Builder::new()
        .name("iw4l-master-browser".into())
        .spawn(move || browser_worker(target, worker_state, worker_refresh, worker_cancel))
        .expect("spawn master browser thread");
    commands.insert_resource(MasterBrowser {
        state,
        refresh,
        cancel,
        worker: Some(worker),
    });
}

fn apply_master_menu_action(
    mut pending: ResMut<PendingMasterMenuAction>,
    mut intent: ResMut<MasterLaunchIntent>,
    mut role: ResMut<frame::RuntimeRole>,
    settings: Res<frame::GameSettings>,
    browser: Option<Res<MasterBrowser>>,
    bridge: Option<Res<MasterBridge>>,
    mut commands: Commands,
) {
    let Some(action) = pending.0.take() else {
        return;
    };
    match action {
        MasterMenuAction::Refresh => {
            if let Some(browser) = browser {
                browser.refresh();
            }
            return;
        }
        MasterMenuAction::SetPassword { password } => {
            if let Some(bridge) = bridge {
                bridge.send(MasterBridgeCommand::SetPassword { password });
            }
            return;
        }
        MasterMenuAction::UpdateLobby { map, mode } => {
            if let Some(bridge) = bridge {
                bridge.send(MasterBridgeCommand::UpdateLobby { map, mode });
            } else {
                diag::warn!(Net, "master lobby update requested without a live bridge");
            }
            return;
        }
        MasterMenuAction::StartMatch { map, mode } => {
            if let Some(bridge) = bridge {
                bridge.send(MasterBridgeCommand::StartMatch { map, mode });
            } else {
                diag::warn!(Net, "master lobby start requested without a live bridge");
            }
            return;
        }
        MasterMenuAction::VoteToSkip => {
            if let Some(bridge) = bridge {
                bridge.send(MasterBridgeCommand::VoteToSkip);
            } else {
                diag::warn!(Net, "master lobby vote requested without a live bridge");
            }
            return;
        }
        MasterMenuAction::LeaveLobby => {
            if let Some(bridge) = bridge {
                bridge.leave();
                commands.remove_resource::<MasterBridge>();
            }
            commands.remove_resource::<UdpAuthorityHub>();
            commands.remove_resource::<UdpClientLink>();
            let browser = match &intent.0 {
                MasterLaunchMode::Browser(config) => Some(config.clone()),
                MasterLaunchMode::Host(config) => Some(BrowserConfig {
                    target: config.target.clone(),
                    have: config.have,
                }),
                MasterLaunchMode::Join(config) => Some(BrowserConfig {
                    target: config.target.clone(),
                    have: config.have,
                }),
                MasterLaunchMode::Disabled => None,
            };
            if let Some(browser) = browser {
                intent.0 = MasterLaunchMode::Browser(browser);
            }
            *role = frame::RuntimeRole::Listen;
            return;
        }
        MasterMenuAction::Host { .. } | MasterMenuAction::Join { .. } => {}
    }
    let MasterLaunchMode::Browser(browser) = &intent.0 else {
        return;
    };
    let browser = browser.clone();
    match action {
        MasterMenuAction::Refresh
        | MasterMenuAction::SetPassword { .. }
        | MasterMenuAction::UpdateLobby { .. }
        | MasterMenuAction::StartMatch { .. }
        | MasterMenuAction::VoteToSkip
        | MasterMenuAction::LeaveLobby => unreachable!(),
        MasterMenuAction::Host {
            map,
            mode,
            password,
        } => {
            let requires = match content_required_by_map(&map) {
                Ok(value) => value,
                Err(error) => {
                    diag::warn!(Net, "master host content: {error}");
                    return;
                }
            };
            intent.0 = MasterLaunchMode::Host(HostConfig {
                password,
                auto_start_map: false,
                target: browser.target,
                name: settings.player_name.clone(),
                map,
                mode,
                max_players: match parse_max_players() {
                    Ok(value) => value,
                    Err(error) => {
                        diag::warn!(Net, "master host settings: {error}");
                        return;
                    }
                },
                requires,
                have: browser.have,
            });
            *role = frame::RuntimeRole::Listen;
        }
        MasterMenuAction::Join {
            password,
            advert_id,
            map,
            mode,
        } => {
            intent.0 = MasterLaunchMode::Join(JoinConfig {
                password,
                target: browser.target,
                advert_id,
                map,
                mode,
                have: browser.have,
            });
            *role = frame::RuntimeRole::Client;
        }
    }
}

fn browser_worker(
    target: MasterTarget,
    state: Arc<Mutex<MasterBrowserSnapshot>>,
    refresh: Arc<AtomicU64>,
    cancel: CancellationToken,
) {
    let runtime = match blocking_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            state.lock().expect("master browser state poisoned").error = Some(error.to_string());
            return;
        }
    };
    runtime.block_on(async move {
        loop {
            if cancel.is_cancelled() {
                return;
            }
            state.lock().expect("master browser state poisoned").loading = true;
            let result: Result<(u64, Vec<master_protocol::Advert>, u64)> =
                io_timeout(&cancel, async {
                    let (_endpoint, connection) = connect(&target, &cancel).await?;
                    let (mut send, mut recv) = io_timeout(&cancel, connection.open_bi()).await?;
                    send.set_priority(0)?;
                    write_frame(
                        &mut send,
                        &ControlFrame::Hello(ControlHello {
                            protocol_version: master_protocol::PROTOCOL_VERSION,
                            game_protocol: crate::PROTOCOL_VERSION,
                            role: EndpointRole::Cli,
                            build: endpoint_build(),
                            player_name: String::new(),
                        }),
                    )
                    .await?;
                    write_frame(
                        &mut send,
                        &ControlFrame::Request(ControlRequest {
                            request_id: 1,
                            body: RequestBody::ListRooms,
                        }),
                    )
                    .await?;
                    loop {
                        match read_frame(&mut recv).await? {
                            ControlFrame::Response(ControlResponse {
                                request_id: 1,
                                body:
                                    ResponseBody::RoomList {
                                        generation,
                                        adverts,
                                    },
                            }) => {
                                return Result::Ok((
                                    generation,
                                    adverts,
                                    connection.rtt().as_millis().max(1) as u64,
                                ));
                            }
                            ControlFrame::Response(ControlResponse {
                                body: ResponseBody::Error(error),
                                ..
                            }) => return Err(error.into()),
                            ControlFrame::RoomView(_)
                            | ControlFrame::Closed { .. }
                            | ControlFrame::PeerEvent(_)
                            | ControlFrame::Hello(_)
                            | ControlFrame::Relay(_) => {}
                            other => {
                                return Err(format!("unexpected browser frame {other:?}").into());
                            }
                        }
                    }
                })
                .await;
            let mut current = state.lock().expect("master browser state poisoned");
            current.loading = false;
            match result {
                Ok((generation, adverts, ping_ms)) => {
                    current.ping_ms = Some(ping_ms);
                    current.generation = generation;
                    let have = current.have;
                    current.adverts = adverts
                        .into_iter()
                        .map(|advert| MasterAdvert {
                            id: advert.id,
                            name: advert.name,
                            map: advert.map,
                            mode: advert.mode,
                            players: advert.players,
                            max_players: advert.max_players,
                            locked: advert.locked,
                            in_match: advert.in_match,
                            requires: advert.requires,
                            available: advert.available,
                            password_protected: advert.password_protected,
                            missing: advert.requires.missing_from(have),
                        })
                        .collect();
                    current.error = None;
                }
                Err(error) => {
                    current.ping_ms = None;
                    let message = error.to_string();
                    if !cancel.is_cancelled() && current.error.as_deref() != Some(message.as_str())
                    {
                        diag::warn!(
                            Net,
                            "master browser request failed target={} server_name={}: {}",
                            target.address,
                            target.server_name,
                            message
                        );
                    }
                    current.error = Some(message);
                }
            }
            drop(current);

            let seen_refresh = refresh.load(Ordering::Relaxed);
            for _ in 0..50 {
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {}
                }
                if refresh.load(Ordering::Relaxed) != seen_refresh {
                    break;
                }
            }
        }
    });
}

struct WorkerCtx {
    player_name: String,
    state: Arc<Mutex<MasterBridgeState>>,
    commands: Arc<Mutex<Vec<MasterBridgeCommand>>>,
    installed_load: Arc<Mutex<Option<frame::LocalLoadKey>>>,
    mailbox: RelayMailbox,
    bootstrap: Arc<BootstrapLane>,
    cancel: CancellationToken,
    close: CancellationToken,
    facts: Arc<Mutex<Vec<MasterLifecycleFact>>>,
    identity: SessionIdentity,
    role: &'static str,
}

static BRIDGE_INCARNATION: AtomicU64 = AtomicU64::new(1);
static ATTEMPT_ID: AtomicU64 = AtomicU64::new(1);

fn spawn_host(config: HostConfig, player_name: String) -> MasterBridge {
    spawn_worker("host", SessionKind::Host(config), player_name)
}

fn spawn_join(config: JoinConfig, player_name: String) -> MasterBridge {
    spawn_worker("join", SessionKind::Join(config), player_name)
}

enum SessionKind {
    Host(HostConfig),
    Join(JoinConfig),
}

fn spawn_worker(role: &'static str, kind: SessionKind, player_name: String) -> MasterBridge {
    let attempt_id = ATTEMPT_ID.fetch_add(1, Ordering::Relaxed);
    let mut identity = SessionIdentity::unassigned(attempt_id);
    if let SessionKind::Join(config) = &kind {
        identity.room_id = config.advert_id;
    }
    let state = Arc::new(Mutex::new(MasterBridgeState::Connecting { identity }));
    let commands = Arc::new(Mutex::new(Vec::new()));
    let installed_load = Arc::new(Mutex::new(None));
    let bootstrap = BootstrapLane::new();
    let mailbox = RelayMailbox::new(HOST_DATA_CAP);
    let cancel = CancellationToken::new();
    let close = CancellationToken::new();
    let facts = Arc::new(Mutex::new(Vec::new()));
    let incarnation = BRIDGE_INCARNATION.fetch_add(1, Ordering::Relaxed);
    let ctx = WorkerCtx {
        player_name,
        state: Arc::clone(&state),
        commands: Arc::clone(&commands),
        installed_load: Arc::clone(&installed_load),
        mailbox: mailbox.clone(),
        bootstrap: Arc::clone(&bootstrap),
        cancel: cancel.clone(),
        close: close.clone(),
        facts: Arc::clone(&facts),
        identity,
        role,
    };
    let thread_state = Arc::clone(&state);
    let thread_identity = identity;

    let worker = std::thread::Builder::new()
        .name("iw4l-master-bridge".into())
        .spawn(move || {
            if let Err(error) = session_worker(kind, ctx) {
                publish_failed(&thread_state, thread_identity, error);
            }
        })
        .expect("spawn master bridge thread");
    MasterBridge {
        state,
        commands,
        installed_load,
        mailbox,
        bootstrap,
        close,
        facts,
        incarnation,
        worker: Some(worker),
    }
}

fn apply_master_lifecycle(
    bridge: Option<Res<MasterBridge>>,
    mut hub: Option<ResMut<UdpAuthorityHub>>,
    mut authority: Option<ResMut<AuthorityWorld>>,
    mut pending_notify: Option<ResMut<crate::PendingGameNotify>>,
) {
    let Some(bridge) = bridge else {
        return;
    };
    let state = bridge.state();
    if let Some(hub) = hub.as_mut()
        && matches!(state, MasterBridgeState::Hosting { .. })
    {
        let occupied: Vec<_> = authority
            .as_ref()
            .map(|authority| {
                authority
                    .0
                    .clients_scoreboard()
                    .into_iter()
                    .map(|(client, _)| client.0)
                    .collect()
            })
            .unwrap_or_else(Vec::new);
        hub.reconcile_relay_membership(state.members(), state.identity().member_id, &occupied);
    }
    for fact in bridge.drain_facts() {
        match fact {
            MasterLifecycleFact::MemberLeft { member_id } => {
                let name = hub.as_ref().and_then(|hub| {
                    let client = hub.client_of_member(member_id)?;
                    let authority = authority.as_ref()?;
                    Some(crate::client_name_string(&authority.0, client))
                });
                if let Some(hub) = hub.as_mut()
                    && let Some(client) = hub.retire_member(member_id)
                    && let Some(authority) = authority.as_mut()
                {
                    authority.0.retire_client(client);
                }
                if let (Some(name), Some(pending)) = (name, pending_notify.as_mut()) {
                    pending.push_left(name);
                }
            }
            MasterLifecycleFact::AdmissionFailed { member_id, reason } => {
                diag::warn!(Net, "admission failed for member {member_id}: {reason}");
                if let Some(hub) = hub.as_mut()
                    && let Some(client) = hub.deny_member(member_id)
                    && let Some(authority) = authority.as_mut()
                {
                    authority.0.retire_client(client);
                }
            }
            MasterLifecycleFact::SessionClosed { .. } => {}
        }
    }
    if let Some(hub) = hub.as_mut() {
        for admission in hub.take_committed_admissions() {
            let client = hub.client_of_member(admission.member_id);
            bridge.admit_enter(
                admission.member_id,
                admission.epoch,
                admission.bootstrap_id,
                Some(admission.connection_id),
                client.map(|client| client.0).unwrap_or(0),
            );
        }
    }
}

pub fn register_master_bridge(app: &mut App) {
    app.init_resource::<PendingMasterMenuAction>()
        .add_systems(
            Update,
            (arm_master_browser, apply_master_menu_action)
                .chain()
                .in_set(crate::ClientSet::Load),
        )
        .add_systems(
            Update,
            (
                arm_master_bridge,
                open_hosted_epoch_when_world_is_live,
                apply_master_lifecycle,
            )
                .chain()
                .in_set(crate::ClientSet::Load),
        )
        .add_systems(
            Update,
            crate::signon::drive_match_boundary
                .in_set(crate::ClientSet::Load)
                .after(frame::SessionSwapApplied),
        )
        .add_systems(Update, observe_master_bridge.in_set(crate::ClientSet::Diag));
    {
        app.add_systems(
            FixedUpdate,
            refresh_relay_authority_hello.in_set(crate::AuthoritySet::Advance),
        );
        app.add_systems(
            FixedUpdate,
            report_committed_authority_progress.after(crate::AuthoritySet::Snapshot),
        );
    }
}

fn report_committed_authority_progress(
    bridge: Option<Res<MasterBridge>>,
    server_tick: Option<Res<crate::authority::runtime::ServerTick>>,
    hold: Option<Res<crate::AuthorityLoadHold>>,
) {
    let Some(bridge) = bridge else {
        return;
    };
    if hold.is_some_and(|hold| hold.0) {
        return;
    }
    if server_tick.is_some_and(|tick| tick.0.is_some()) {
        bridge.report_authority_progress();
    }
}

fn open_hosted_epoch_when_world_is_live(
    bridge: Option<Res<MasterBridge>>,
    intent: Res<MasterLaunchIntent>,
    admission: Res<crate::ClientAdmission>,
    has_world: Option<Res<frame::HasWorld>>,
    launch: Option<Res<frame::LaunchIdentity>>,
    hold: Option<Res<crate::AuthorityLoadHold>>,
    mut sent: Local<Option<(u64, frame::LocalLoadKey)>>,
) {
    let Some(bridge) = bridge else {
        return;
    };
    let MasterLaunchMode::Host(config) = &intent.0 else {
        return;
    };
    if !config.auto_start_map
        || !has_world.is_some_and(|world| world.0)
        || hold.is_some_and(|hold| hold.0)
    {
        return;
    }

    let Some(load) = admission
        .core
        .installed()
        .filter(|load| load.match_key.is_none())
    else {
        return;
    };
    let key = (bridge.incarnation(), load);
    if *sent == Some(key) {
        return;
    }
    let MasterBridgeState::Hosting {
        in_match: false, ..
    } = bridge.state()
    else {
        return;
    };
    let Some(launch) = launch else {
        return;
    };
    bridge.start_hosted_match(launch.zone.clone(), config.mode.clone());
    *sent = Some(key);
}

fn observe_master_bridge(
    bridge: Option<Res<MasterBridge>>,
    mut previous: Local<Option<MasterBridgeState>>,
    mut seen_start: Local<Option<(AdvertId, u32)>>,
    mut pending_start: ResMut<MasterMatchStart>,
) {
    let Some(bridge) = bridge else {
        return;
    };
    let current = bridge.state();
    let identity = current.identity();
    if current.in_match()
        && observe_live_start(true, identity.epoch)
        && seen_start.as_ref() != Some(&(identity.room_id, identity.epoch))
        && let (Some(map), Some(mode)) = (current.map(), current.mode())
    {
        pending_start.0 = Some(MasterMatchOffer {
            map: map.to_owned(),
            mode: mode.to_owned(),
            match_key: identity.match_key(),
        });
        *seen_start = Some((identity.room_id, identity.epoch));
    }
    if previous.as_ref() == Some(&current) {
        return;
    }
    match &current {
        MasterBridgeState::Failed { error, identity } => {
            diag::warn!(
                Net,
                "master relay failed attempt={} room={} epoch={}: {error}",
                identity.attempt_id,
                identity.room_id,
                identity.epoch
            );
        }
        _ => diag::info!(Net, "master relay: {current:?}"),
    }
    if let Some(path) = std::env::var_os("IW4L_MASTER_STATUS_FILE") {
        let path = PathBuf::from(path);
        let state = match &current {
            MasterBridgeState::Hosting { .. } => "hosting",
            MasterBridgeState::Joined { .. } => "joined",
            MasterBridgeState::Failed { .. } => "failed",
            MasterBridgeState::Closed { .. } => "closed",
            MasterBridgeState::Left { .. } => "left",
            _ => "connecting",
        };
        let body = format!(
            "state={state}\nroom={}\nmembers={}\nin_match={}\n",
            identity.room_id,
            current.members().len(),
            current.in_match()
        );
        let temporary = path.with_extension("tmp");
        if let Err(error) =
            std::fs::write(&temporary, body).and_then(|()| std::fs::rename(&temporary, &path))
        {
            diag::warn!(Net, "master status file: {error}");
        }
    }
    *previous = Some(current);
}

fn host_transition_event(line: &str) -> &str {
    line.split(" event=")
        .nth(1)
        .and_then(|rest| rest.split(" stage_after=").next())
        .unwrap_or("")
}

fn should_log_host_transition(applied: &HostMatchApply) -> bool {
    if !applied.effects.is_empty() {
        return true;
    }
    let event = host_transition_event(&applied.transition);
    event != "tick" && event != "authority progress"
}

fn apply_host(core: &mut HostMatchCore, event: HostMatchEvent) -> HostMatchApply {
    let applied = core.apply(event);
    if should_log_host_transition(&applied) {
        diag::info!(Net, "session-transition {}", applied.transition);
    }
    applied
}

fn execute_host_match_effects(
    effects: Vec<HostMatchEffect>,
    map_ready: &Mutex<HashSet<MemberId>>,
    facts: &Mutex<Vec<MasterLifecycleFact>>,
    control_tx: &tokio::sync::mpsc::Sender<ControlFrame>,
    request_id: &mut u64,
    role: &'static str,
) -> std::result::Result<(), TransportFault> {
    let mut fail = None;
    for effect in effects {
        match effect {
            HostMatchEffect::PublishReady { match_key, ready } => {
                enqueue_control(
                    control_tx,
                    ControlFrame::Request(ControlRequest {
                        request_id: take_id(request_id),
                        body: RequestBody::HostWorldReady {
                            room_id: AdvertId(match_key.session_id),
                            epoch: match_key.match_epoch,
                            map: ready.map,
                            weapons: ready.weapons,
                            classes: ready.classes,
                        },
                    }),
                    role,
                )?;
            }
            HostMatchEffect::PublishEnd { match_key } => {
                map_ready
                    .lock()
                    .expect("bootstrap readiness poisoned")
                    .clear();
                enqueue_control(
                    control_tx,
                    ControlFrame::Request(ControlRequest {
                        request_id: take_id(request_id),
                        body: RequestBody::EndMatch {
                            room_id: AdvertId(match_key.session_id),
                            epoch: match_key.match_epoch,
                        },
                    }),
                    role,
                )?;
            }
            HostMatchEffect::FlushBootstraps { member } => {
                map_ready
                    .lock()
                    .expect("bootstrap readiness poisoned")
                    .insert(member);
            }
            HostMatchEffect::Enter { .. } => {}
            HostMatchEffect::CancelPeer {
                member,
                reason,
                match_key,
            } => {
                map_ready
                    .lock()
                    .expect("bootstrap readiness poisoned")
                    .remove(&member);
                push_fact(
                    facts,
                    MasterLifecycleFact::AdmissionFailed {
                        member_id: member,
                        reason,
                    },
                );
                enqueue_control(
                    control_tx,
                    ControlFrame::Request(ControlRequest {
                        request_id: take_id(request_id),
                        body: RequestBody::AdmissionFailed {
                            member_id: member,
                            epoch: match_key.match_epoch,
                            reason,
                        },
                    }),
                    role,
                )?;
            }
            HostMatchEffect::Fail { fail: session_fail } => {
                fail = Some(session_fail);
            }
        }
    }
    match fail {
        Some(fail) => Err(TransportFault::new("host_match", role, fail.to_string())),
        None => Ok(()),
    }
}

fn session_worker(kind: SessionKind, ctx: WorkerCtx) -> std::result::Result<(), TransportFault> {
    let runtime = blocking_runtime()
        .map_err(|error| TransportFault::new("runtime", ctx.role, error.to_string()))?;
    runtime.block_on(session_main(kind, ctx))
}

async fn session_main(
    kind: SessionKind,
    ctx: WorkerCtx,
) -> std::result::Result<(), TransportFault> {
    let WorkerCtx {
        player_name,
        state,
        commands,
        installed_load,
        mailbox,
        bootstrap,
        cancel,
        close,
        facts,
        mut identity,
        role,
    } = ctx;
    let early_close = tokio::spawn({
        let close = close.clone();
        let cancel = cancel.clone();
        async move {
            close.cancelled().await;
            cancel.cancel();
        }
    });
    let (is_host, target, create, join) = match kind {
        SessionKind::Host(config) => (true, config.target.clone(), Some(config), None),
        SessionKind::Join(config) => (false, config.target.clone(), None, Some(config)),
    };
    let hello_role = if is_host {
        EndpointRole::Host
    } else {
        EndpointRole::Join
    };
    diag::info!(
        Net,
        "master {role} connect begin attempt={} target={} server_name={} build={}",
        identity.attempt_id,
        target.address,
        target.server_name,
        endpoint_build()
    );
    let (_endpoint, connection) = connect(&target, &cancel)
        .await
        .map_err(|error| TransportFault::new("connect", role, error.to_string()))?;
    diag::info!(
        Net,
        "master {role} hello build={} wire={} game={} attempt={}",
        endpoint_build(),
        master_protocol::PROTOCOL_VERSION,
        crate::PROTOCOL_VERSION,
        identity.attempt_id
    );
    let (mut send, recv) = io_timeout(&cancel, connection.open_bi())
        .await
        .map_err(|error| TransportFault::with_connection("open_bi", role, error, &connection))?;
    send.set_priority(0).map_err(|error| {
        TransportFault::with_connection("control_priority", role, error, &connection)
    })?;
    write_frame(
        &mut send,
        &ControlFrame::Hello(ControlHello {
            protocol_version: master_protocol::PROTOCOL_VERSION,
            game_protocol: crate::PROTOCOL_VERSION,
            role: hello_role,
            build: endpoint_build(),
            player_name,
        }),
    )
    .await
    .map_err(|error| TransportFault::with_connection("hello", role, error, &connection))?;

    let (control_tx, mut control_rx) = tokio::sync::mpsc::channel::<ControlFrame>(HOST_CONTROL_CAP);
    let mut children = tokio::task::JoinSet::new();
    {
        let writer_connection = connection.clone();
        let writer_cancel = cancel.clone();
        let mut writer = send;
        let writer_role = role;
        children.spawn(async move {
            loop {
                tokio::select! {
                    _ = writer_cancel.cancelled() => return Ok(()),
                    frame = control_rx.recv() => {
                        let Some(frame) = frame else { return Ok(()) };
                        write_frame(&mut writer, &frame).await.map_err(|error| {
                            TransportFault::with_connection(
                                "control_write",
                                writer_role,
                                error,
                                &writer_connection,
                            )
                        })?;
                    }
                }
            }
        });
    }
    children.spawn(datagram_ingress(
        connection.clone(),
        mailbox.clone(),
        cancel.clone(),
        role,
        is_host,
    ));
    children.spawn(uni_ingress(
        connection.clone(),
        Arc::clone(&bootstrap),
        cancel.clone(),
        role,
    ));
    children.spawn(gameplay_egress(
        connection.clone(),
        mailbox.clone(),
        cancel.clone(),
        role,
        is_host,
    ));
    let (bootstrap_jobs, bootstrap_rx) =
        tokio::sync::mpsc::channel::<(frame::MatchKey, MemberId, Vec<u8>)>(
            MAX_CONCURRENT_BOOTSTRAP as usize,
        );
    let (prepared_tx, mut prepared_rx) =
        tokio::sync::mpsc::channel::<HostMatchEvent>(MAX_CONCURRENT_BOOTSTRAP as usize);
    children.spawn(bootstrap_egress(
        connection.clone(),
        bootstrap_rx,
        prepared_tx,
        cancel.clone(),
        role,
    ));

    let mut request_id = 1_u64;
    let first = if let Some(config) = create.as_ref() {
        ControlRequest {
            request_id: take_id(&mut request_id),
            body: RequestBody::CreateRoom {
                name: config.name.clone(),
                map: config.map.clone(),
                mode: config.mode.clone(),
                max_players: config.max_players,
                requires: config.requires,
                available: config.have,
                password: config.password.clone(),
            },
        }
    } else {
        let config = join.as_ref().expect("join config");
        ControlRequest {
            request_id: take_id(&mut request_id),
            body: RequestBody::JoinRoom {
                room_id: config.advert_id,
                password: config.password.clone(),
                have: config.have,
            },
        }
    };
    early_close.abort();
    if cancel.is_cancelled() {
        return Ok(());
    }
    enqueue_control(&control_tx, ControlFrame::Request(first), role)?;

    let mut session = SessionCore::for_connection(identity.room_id.0, identity.attempt_id);
    let mut host_match = HostMatchCore::default();
    let mut members: HashSet<MemberId> = HashSet::new();
    let mut skip_voters: HashSet<MemberId> = HashSet::new();
    let map_ready = &bootstrap.host_map_ready;
    let mut last_view: Option<RoomView> = None;
    let mut started_epoch = 0_u32;
    let mut leave_request: Option<u64> = None;
    let mut leave_at: Option<tokio::time::Instant> = None;
    let mut closing = false;
    let mut watchdog = tokio::time::interval(WATCHDOG_INTERVAL);
    watchdog.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let mut next_control = Box::pin(read_owned_frame(recv));
    let outcome = loop {
        if cancel.is_cancelled() {
            break Ok(());
        }
        if !closing
            && let Err(error) = pump_local_queues(
                identity.match_key(),
                &bootstrap,
                &control_tx,
                &bootstrap_jobs,
                role,
                is_host,
                &mut request_id,
                map_ready,
            )
        {
            break Err(error);
        }
        let mut control_error = None;
        for (member_id, payload) in mailbox.take_control_outbound() {
            let envelope = if is_host {
                RelayDatagram::HostToMember {
                    member_id,
                    payload: &payload,
                }
            } else {
                RelayDatagram::ClientToHost(&payload)
            };
            match master_protocol::encode_relay_stream(envelope) {
                Ok(bytes) => {
                    if let Err(error) =
                        enqueue_control(&control_tx, ControlFrame::Relay(bytes), role)
                    {
                        control_error = Some(error);
                        break;
                    }
                }
                Err(error) => {
                    control_error = Some(TransportFault::new(
                        "control_encode",
                        role,
                        error.to_string(),
                    ));
                    break;
                }
            }
        }
        if let Some(error) = control_error {
            break Err(error);
        }
        let mut command_error = None;
        for command in take_commands(&commands) {
            match handle_command(
                command,
                &installed_load,
                &control_tx,
                &mut request_id,
                &facts,
                &mut host_match,
                map_ready,
                role,
                is_host,
                closing,
            ) {
                Ok(CommandEffect::Continue) => {}
                Ok(CommandEffect::BeginClose { request_id }) => {
                    diag::info!(
                        Net,
                        "master {role} closing: {} request={request_id}",
                        if is_host { "CloseRoom" } else { "LeaveRoom" }
                    );
                    closing = true;
                    leave_request = Some(request_id);
                    leave_at = Some(tokio::time::Instant::now() + LEAVE_DRAIN);
                }
                Err(error) => {
                    command_error = Some(error);
                    break;
                }
            }
        }
        if let Some(error) = command_error {
            break Err(error);
        }
        tokio::select! {
            _ = cancel.cancelled() => break Ok(()),
            _ = close.cancelled(), if !closing => {}
            _ = async {
                match leave_at {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending().await,
                }
            } => {
                diag::warn!(
                    Net,
                    "master {role} closed: relay did not confirm within {}ms",
                    LEAVE_DRAIN.as_millis()
                );
                break Ok(());
            }
            prepared = prepared_rx.recv() => {
                let Some(event) = prepared else {
                    break Err(TransportFault::new(
                        "bootstrap_worker",
                        role,
                        "prepared mailbox closed",
                    ));
                };
                let applied = apply_host(&mut host_match, event);
                if let Err(error) = execute_host_match_effects(
                    applied.effects,
                    map_ready,
                    &facts,
                    &control_tx,
                    &mut request_id,
                    role,
                ) {
                    break Err(error);
                }
            }
            _ = watchdog.tick() => {
                if is_host {
                    let tick = apply_host(&mut host_match, HostMatchEvent::Tick { now_ms: now_ms() });
                    if let Err(error) = execute_host_match_effects(
                        tick.effects,
                        map_ready,
                        &facts,
                        &control_tx,
                        &mut request_id,
                        role,
                    ) {
                        break Err(error);
                    }
                }
            }
            (recv, frame) = async {
                mailbox.wait_control_inbound_capacity().await;
                (&mut next_control).await
            } => {
                next_control.set(read_owned_frame(recv));
                match frame {
                    Ok(ControlFrame::Relay(bytes)) => {
                        let ingress = match master_protocol::decode_relay_stream(&bytes) {
                            Ok(RelayDatagram::ServiceToHost { member_id, payload }) if is_host => Some((member_id, payload.to_vec())),
                            Ok(RelayDatagram::ServiceToMember(payload)) if !is_host => Some((MemberId([0; 16]), payload.to_vec())),
                            _ => None,
                        };
                        let Some((member, packet)) = ingress else { break Err(TransportFault::new("control_relay", role, "wrong relay direction")); };
                        if let Err(error) = mailbox.push_control_inbound(member, packet) { break Err(TransportFault::new("control_relay", role, error)); }
                    }
                    Ok(frame) => {
                        let close_now = closing
                            && match &frame {
                                ControlFrame::Response(response) => {
                                    Some(response.request_id) == leave_request
                                }
                                ControlFrame::Closed { .. } => true,
                                _ => false,
                            };
                        if let Err(error) = handle_frame(
                            frame,
                            &state,
                            &facts,
                            &control_tx,
                            &mut request_id,
                            &bootstrap,
                            &mut session,
                            &mut host_match,
                            &mut identity,
                            &mut members,
                            &mut skip_voters,
                            map_ready,
                            &mut last_view,
                            &mut started_epoch,
                            is_host,
                            role,
                        ) {
                            break Err(error);
                        }
                        if close_now {
                            diag::info!(Net, "master {role} closed: relay confirmed");
                            break Ok(());
                        }
                    }
                    Err(error) => {
                        break Err(TransportFault::with_connection(
                            "control_read",
                            role,
                            error,
                            &connection,
                        ));
                    }
                }
            }
            child = children.join_next() => {
                match child {
                    None => {}
                    Some(result) => match flatten_task(result, "child", role) {
                        Ok(()) if cancel.is_cancelled() || closing => break Ok(()),
                        Ok(()) => {
                            break Err(TransportFault::new("child", role, "task ended"));
                        }
                        Err(error) => break Err(error),
                    },
                }
            }
            _ = tokio::time::sleep(QUEUE_POLL) => {}
        }
    };

    cancel.cancel();
    connection.close(0_u8.into(), b"session closed");
    drop(bootstrap_jobs);
    let _ = tokio::time::timeout(LEAVE_DRAIN, async {
        while children.join_next().await.is_some() {}
    })
    .await;
    if outcome.is_ok() && closing {
        publish_left(&state, identity);
    }
    outcome
}

enum CommandEffect {
    Continue,
    BeginClose { request_id: u64 },
}

fn take_id(request_id: &mut u64) -> u64 {
    let id = *request_id;
    *request_id = request_id.saturating_add(1);
    id
}

fn enqueue_control(
    tx: &tokio::sync::mpsc::Sender<ControlFrame>,
    frame: ControlFrame,
    role: &'static str,
) -> std::result::Result<(), TransportFault> {
    classify_try_send(tx.try_send(frame), "control_write").map_err(|loss| {
        TransportFault::new(
            match loss {
                QueueLoss::Full { operation } | QueueLoss::Closed { operation } => operation,
            },
            role,
            loss.to_string(),
        )
    })
}

fn flatten_task(
    result: std::result::Result<std::result::Result<(), TransportFault>, tokio::task::JoinError>,
    operation: &'static str,
    role: &'static str,
) -> std::result::Result<(), TransportFault> {
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error),
        Err(join) => Err(TransportFault::new(operation, role, join.to_string())),
    }
}

fn publish_failed(
    state: &Mutex<MasterBridgeState>,
    fallback: SessionIdentity,
    error: TransportFault,
) {
    let mut current = state.lock().expect("master state poisoned");
    if current.is_terminal() {
        return;
    }
    let identity = current.identity();
    let identity = if identity.attempt_id == 0 {
        fallback
    } else {
        identity
    };
    diag::warn!(Net, "master relay failed: {error}");
    *current = MasterBridgeState::Failed { identity, error };
}

fn publish_closed(
    state: &Mutex<MasterBridgeState>,
    identity: SessionIdentity,
    reason: SessionCloseReason,
) {
    let mut current = state.lock().expect("master state poisoned");
    if current.is_terminal() {
        return;
    }
    let identity = current
        .identity()
        .attempt_id
        .checked_sub(0)
        .map(|_| current.identity())
        .filter(|id| id.attempt_id != 0)
        .unwrap_or(identity);
    *current = MasterBridgeState::Closed { identity, reason };
}

fn publish_left(state: &Mutex<MasterBridgeState>, identity: SessionIdentity) {
    let mut current = state.lock().expect("master state poisoned");
    if current.is_terminal() {
        return;
    }
    *current = MasterBridgeState::Left { identity };
}

fn publish_room_view(
    state: &Mutex<MasterBridgeState>,
    identity: SessionIdentity,
    view: &RoomView,
    skip_votes: u8,
    is_host: bool,
    local_member: MemberId,
) {
    let mut current = state.lock().expect("master state poisoned");
    if current.is_terminal() {
        return;
    }
    let in_match = view.phase.in_match();
    if is_host {
        *current = MasterBridgeState::Hosting {
            identity,
            name: view.name.clone(),
            map: view.map.clone(),
            mode: view.mode.clone(),
            members: view.members.clone(),
            member_names: view.member_names.clone(),
            max_players: view.max_players,
            skip_votes,
            in_match,
        };
        return;
    }
    if view.contains(local_member) {
        *current = MasterBridgeState::Joined {
            identity,
            name: view.name.clone(),
            map: view.map.clone(),
            mode: view.mode.clone(),
            members: view.members.clone(),
            member_names: view.member_names.clone(),
            max_players: view.max_players,
            skip_votes,
            in_match,
        };
    } else {
        *current = MasterBridgeState::Joining { identity };
    }
}

fn apply_authoritative_view(
    session: &mut SessionCore,
    view: &RoomView,
    member: MemberId,
    skip_votes: u8,
) -> bool {
    let applied = session.apply(SessionEvent::AuthoritativeState {
        session_id: view.room_id.0,
        member,
        revision: view.revision,
        match_epoch: view.epoch,
        in_match: view.phase.in_match(),
        start_nonce: view.epoch,
        map: view.map.clone(),
        mode: view.mode.clone(),
        members: view.members.clone(),
        skip_votes,
    });
    if applied.changed {
        diag::info!(Net, "session-transition {}", applied.transition);
    }
    applied.changed
}

fn sync_membership_facts(
    previous: Option<&RoomView>,
    view: &RoomView,
    members: &mut HashSet<MemberId>,
    skip_voters: &mut HashSet<MemberId>,
    map_ready: &Mutex<HashSet<MemberId>>,
    facts: &Mutex<Vec<MasterLifecycleFact>>,
) {
    let previous_members: HashSet<MemberId> = previous
        .map(|view| view.members.iter().copied().collect())
        .unwrap_or_default();
    let next: HashSet<MemberId> = view.members.iter().copied().collect();

    for member_id in previous_members.difference(&next) {
        forget_relay_member(*member_id, members, skip_voters, map_ready, facts);
    }
    *members = next;
}

fn handle_command(
    command: MasterBridgeCommand,
    installed_load: &Mutex<Option<frame::LocalLoadKey>>,
    control_tx: &tokio::sync::mpsc::Sender<ControlFrame>,
    request_id: &mut u64,
    facts: &Mutex<Vec<MasterLifecycleFact>>,
    host_match: &mut HostMatchCore,
    map_ready: &Mutex<HashSet<MemberId>>,
    role: &'static str,
    is_host: bool,
    closing: bool,
) -> std::result::Result<CommandEffect, TransportFault> {
    if closing {
        return Ok(CommandEffect::Continue);
    }
    match command {
        MasterBridgeCommand::SetPassword { password } => {
            enqueue_control(
                control_tx,
                ControlFrame::Request(ControlRequest {
                    request_id: take_id(request_id),
                    body: RequestBody::SetPassword { password },
                }),
                role,
            )?;
        }
        MasterBridgeCommand::UpdateLobby { map, mode } => {
            enqueue_control(
                control_tx,
                ControlFrame::Request(ControlRequest {
                    request_id: take_id(request_id),
                    body: RequestBody::SetOptions {
                        map,
                        mode,
                        joinable: true,
                    },
                }),
                role,
            )?;
        }
        MasterBridgeCommand::StartMatch { map, mode } => {
            enqueue_control(
                control_tx,
                ControlFrame::Request(ControlRequest {
                    request_id: take_id(request_id),
                    body: RequestBody::StartMatch { map, mode },
                }),
                role,
            )?;
        }
        MasterBridgeCommand::VoteToSkip => {
            enqueue_control(
                control_tx,
                ControlFrame::Request(ControlRequest {
                    request_id: take_id(request_id),
                    body: RequestBody::VoteToSkip,
                }),
                role,
            )?;
        }
        MasterBridgeCommand::MapLoaded {
            epoch,
            map,
            weapons,
            classes,
        } => {
            enqueue_control(
                control_tx,
                ControlFrame::Request(ControlRequest {
                    request_id: take_id(request_id),
                    body: RequestBody::MapLoaded {
                        epoch,
                        map,
                        weapons,
                        classes,
                    },
                }),
                role,
            )?;
            if is_host {
                let applied = apply_host(
                    host_match,
                    HostMatchEvent::MapLoaded {
                        member: MemberId([0; 16]),
                        loaded: HostWorldReady {
                            epoch,
                            map,
                            weapons,
                            classes,
                        },
                        now_ms: now_ms(),
                    },
                );
                execute_host_match_effects(
                    applied.effects,
                    map_ready,
                    facts,
                    control_tx,
                    request_id,
                    role,
                )?;
            }
        }
        MasterBridgeCommand::HostWorldReady {
            epoch,
            map,
            weapons,
            classes,
            load_key,
        } => {
            let installed = installed_load.lock().expect("installed load poisoned");
            if !is_host
                || *installed != Some(load_key)
                || !load_key.belongs_to(host_match.match_key())
                || epoch != load_key.match_key.match_epoch
            {
                return Ok(CommandEffect::Continue);
            }
            host_match.set_incarnation(load_key.incarnation);
            host_match.set_local_load_id(Some(load_key.local_load_request_id));
            let ready = HostWorldReady {
                epoch,
                map,
                weapons,
                classes,
            };
            let mut applied = apply_host(
                host_match,
                HostMatchEvent::AuthorityReady {
                    ready,
                    now_ms: now_ms(),
                },
            );
            if applied.accepted {
                applied.effects.insert(
                    0,
                    HostMatchEffect::PublishReady {
                        match_key: load_key.match_key,
                        ready,
                    },
                );
            }
            execute_host_match_effects(
                applied.effects,
                map_ready,
                facts,
                control_tx,
                request_id,
                role,
            )?;
        }
        MasterBridgeCommand::MatchEnded { match_key } => {
            if !is_host {
                return Ok(CommandEffect::Continue);
            }
            let mut applied = apply_host(host_match, HostMatchEvent::MatchEnded { match_key });
            if applied.accepted {
                applied
                    .effects
                    .push(HostMatchEffect::PublishEnd { match_key });
            }
            execute_host_match_effects(
                applied.effects,
                map_ready,
                facts,
                control_tx,
                request_id,
                role,
            )?;
        }
        MasterBridgeCommand::AuthorityProgress => {
            if !is_host {
                return Ok(CommandEffect::Continue);
            }
            let applied = apply_host(
                host_match,
                HostMatchEvent::AuthorityProgress { now_ms: now_ms() },
            );
            execute_host_match_effects(
                applied.effects,
                map_ready,
                facts,
                control_tx,
                request_id,
                role,
            )?;
        }

        MasterBridgeCommand::AdmitEnter {
            member_id,
            epoch,
            bootstrap_id,
            connection_id,
            client_id,
        } => {
            if !is_host {
                return Ok(CommandEffect::Continue);
            }
            let applied = apply_host(
                host_match,
                HostMatchEvent::Applied {
                    match_key: frame::MatchKey::new(host_match.match_key().session_id, epoch),
                    member: member_id,
                    bootstrap_id,
                    connection_id,
                },
            );
            if !applied.accepted {
                return Ok(CommandEffect::Continue);
            }
            execute_host_match_effects(
                applied.effects,
                map_ready,
                facts,
                control_tx,
                request_id,
                role,
            )?;
            enqueue_control(
                control_tx,
                ControlFrame::Request(ControlRequest {
                    request_id: take_id(request_id),
                    body: RequestBody::EnterMatch {
                        member_id,
                        epoch,
                        bootstrap_id,
                        connection_id: connection_id.unwrap_or(0),
                        client_id,
                    },
                }),
                role,
            )?;
        }
        MasterBridgeCommand::Shutdown => {
            let id = take_id(request_id);
            let body = if is_host {
                RequestBody::CloseRoom
            } else {
                RequestBody::LeaveRoom
            };
            enqueue_control(
                control_tx,
                ControlFrame::Request(ControlRequest {
                    request_id: id,
                    body,
                }),
                role,
            )?;
            return Ok(CommandEffect::BeginClose { request_id: id });
        }
    }
    Ok(CommandEffect::Continue)
}

fn handle_frame(
    frame: ControlFrame,
    state: &Mutex<MasterBridgeState>,
    facts: &Mutex<Vec<MasterLifecycleFact>>,
    control_tx: &tokio::sync::mpsc::Sender<ControlFrame>,
    request_id: &mut u64,
    bootstrap: &BootstrapLane,
    session: &mut SessionCore,
    host_match: &mut HostMatchCore,
    identity: &mut SessionIdentity,
    members: &mut HashSet<MemberId>,
    skip_voters: &mut HashSet<MemberId>,
    map_ready: &Mutex<HashSet<MemberId>>,
    last_view: &mut Option<RoomView>,
    started_epoch: &mut u32,
    is_host: bool,
    role: &'static str,
) -> std::result::Result<(), TransportFault> {
    match frame {
        ControlFrame::Relay(_) => Err(TransportFault::new(
            "control_relay",
            role,
            "relay must be consumed before lifecycle",
        )),
        ControlFrame::Hello(_) => Ok(()),
        ControlFrame::Request(_) => Err(TransportFault::new(
            "control_read",
            role,
            "service sent a request on the client stream",
        )),
        ControlFrame::Response(response) => {
            let (view, member) = match response.body {
                ResponseBody::RoomCreated { member_id, view }
                | ResponseBody::RoomJoined { member_id, view } => (view, member_id),
                ResponseBody::RoomUpdated { view } => (view, identity.member_id),
                ResponseBody::Error(error) => {
                    return Err(TransportFault::new("control_rpc", role, error.to_string()));
                }
                ResponseBody::RoomLeft
                | ResponseBody::Ack
                | ResponseBody::Status(_)
                | ResponseBody::RoomList { .. } => return Ok(()),
            };
            apply_room_view(
                view,
                member,
                state,
                facts,
                control_tx,
                request_id,
                bootstrap,
                session,
                host_match,
                identity,
                members,
                skip_voters,
                map_ready,
                last_view,
                started_epoch,
                is_host,
                role,
            )
        }
        ControlFrame::RoomView(view) => apply_room_view(
            view,
            identity.member_id,
            state,
            facts,
            control_tx,
            request_id,
            bootstrap,
            session,
            host_match,
            identity,
            members,
            skip_voters,
            map_ready,
            last_view,
            started_epoch,
            is_host,
            role,
        ),
        ControlFrame::Closed {
            room_id,
            epoch,
            reason,
        } => {
            if room_id != identity.room_id || epoch != identity.epoch {
                return Ok(());
            }
            session.apply(SessionEvent::CloseSession);
            push_fact(facts, MasterLifecycleFact::SessionClosed { reason });
            publish_closed(state, *identity, reason);
            Ok(())
        }
        ControlFrame::PeerEvent(event) => apply_peer_event(
            event,
            facts,
            control_tx,
            request_id,
            session,
            host_match,
            identity,
            skip_voters,
            map_ready,
            bootstrap,
            is_host,
            role,
            state,
            last_view,
        ),
    }
}

fn apply_room_view(
    view: RoomView,
    local_member: MemberId,
    state: &Mutex<MasterBridgeState>,
    facts: &Mutex<Vec<MasterLifecycleFact>>,
    control_tx: &tokio::sync::mpsc::Sender<ControlFrame>,
    request_id: &mut u64,
    bootstrap: &BootstrapLane,
    session: &mut SessionCore,
    host_match: &mut HostMatchCore,
    identity: &mut SessionIdentity,
    members: &mut HashSet<MemberId>,
    skip_voters: &mut HashSet<MemberId>,
    map_ready: &Mutex<HashSet<MemberId>>,
    last_view: &mut Option<RoomView>,
    started_epoch: &mut u32,
    is_host: bool,
    role: &'static str,
) -> std::result::Result<(), TransportFault> {
    // Acceptance owns the first mutation. Rejected views cannot touch bridge
    // identity, membership, bootstrap atomics, admission, or UI projection.
    let skip_votes =
        u8::try_from(skip_voters.iter().filter(|id| view.contains(**id)).count()).unwrap_or(0);
    if !apply_authoritative_view(session, &view, local_member, skip_votes) {
        return Ok(());
    }
    identity.member_id = local_member;
    identity.room_id = view.room_id;
    identity.epoch = view.epoch;
    sync_membership_facts(
        last_view.as_ref(),
        &view,
        members,
        skip_voters,
        map_ready,
        facts,
    );
    if is_host && view.phase.in_match() && view.epoch != 0 && *started_epoch != view.epoch {
        let applied = apply_host(
            host_match,
            HostMatchEvent::Start {
                match_key: identity.match_key(),
                now_ms: now_ms(),
            },
        );
        execute_host_match_effects(
            applied.effects,
            map_ready,
            facts,
            control_tx,
            request_id,
            role,
        )?;
        *started_epoch = view.epoch;
    }
    if is_host && !view.phase.in_match() && *started_epoch != 0 {
        let applied = apply_host(
            host_match,
            HostMatchEvent::MatchEnded {
                match_key: frame::MatchKey::new(view.room_id.0, *started_epoch),
            },
        );
        execute_host_match_effects(
            applied.effects,
            map_ready,
            facts,
            control_tx,
            request_id,
            role,
        )?;
    }

    let live = session.view();
    bootstrap.set_epoch(if live.in_match { live.start_nonce } else { 0 });
    publish_room_view(
        state,
        *identity,
        &view,
        u8::try_from(skip_voters.len()).unwrap_or(0),
        is_host,
        identity.member_id,
    );
    *last_view = Some(view);
    Ok(())
}

fn apply_peer_event(
    event: master_protocol::PeerEvent,
    facts: &Mutex<Vec<MasterLifecycleFact>>,
    control_tx: &tokio::sync::mpsc::Sender<ControlFrame>,
    request_id: &mut u64,
    session: &mut SessionCore,
    host_match: &mut HostMatchCore,
    identity: &SessionIdentity,
    skip_voters: &mut HashSet<MemberId>,
    map_ready: &Mutex<HashSet<MemberId>>,
    bootstrap: &BootstrapLane,
    is_host: bool,
    role: &'static str,
    state: &Mutex<MasterBridgeState>,
    last_view: &Option<RoomView>,
) -> std::result::Result<(), TransportFault> {
    match event {
        master_protocol::PeerEvent::HostWorldReady {
            epoch,
            map,
            weapons,
            classes,
        } if is_host => {
            let applied = apply_host(
                host_match,
                HostMatchEvent::AuthorityReady {
                    ready: HostWorldReady {
                        epoch,
                        map,
                        weapons,
                        classes,
                    },
                    now_ms: now_ms(),
                },
            );
            execute_host_match_effects(
                applied.effects,
                map_ready,
                facts,
                control_tx,
                request_id,
                role,
            )?;
        }
        master_protocol::PeerEvent::MapLoaded {
            member_id,
            epoch,
            map,
            weapons,
            classes,
        } => {
            if is_host {
                let applied = apply_host(
                    host_match,
                    HostMatchEvent::MapLoaded {
                        member: member_id,
                        loaded: HostWorldReady {
                            epoch,
                            map,
                            weapons,
                            classes,
                        },
                        now_ms: now_ms(),
                    },
                );
                // A peer that reported its map and drew no effect is a peer
                // that will wait out the match in the loading screen. Name the
                // gate that owes it an offer.
                if applied.effects.is_empty() {
                    diag::warn!(
                        Net,
                        "peer map loaded admits nothing: epoch={epoch} host_phase={:?} host_match={:?} host_world={:?} peer={:?} — no bootstrap offer will follow",
                        host_match.phase(),
                        host_match.match_key(),
                        host_match.host_world(),
                        host_match.peer(member_id).map(|peer| peer.phase),
                    );
                }
                execute_host_match_effects(
                    applied.effects,
                    map_ready,
                    facts,
                    control_tx,
                    request_id,
                    role,
                )?;
            }
        }

        master_protocol::PeerEvent::BootstrapApplied {
            member_id,
            epoch,
            bootstrap_id,
            snapshot_seq,
            connection: connection_id,
        } if is_host => {
            if let Err(error) = bootstrap.push_ack(BootstrapAck {
                member_id,
                epoch,
                bootstrap_id,
                snapshot_seq,
                connection: crate::transport::protocol::ConnectionId(connection_id),
            }) {
                return Err(TransportFault::new("bootstrap_ack", role, error));
            }
        }
        master_protocol::PeerEvent::AdmissionFailed {
            member_id,
            epoch,
            reason,
        } if !is_host => {
            if member_id != identity.member_id || !epoch_applies(epoch, identity.epoch) {
                return Ok(());
            }
            return Err(TransportFault::new("admission", role, reason.to_string()));
        }
        master_protocol::PeerEvent::EnterMatch {
            member_id,
            epoch,
            bootstrap_id,
            client_id,
            ..
        } => {
            if member_id == identity.member_id {
                bootstrap.note_entered(bootstrap_id, client_id);
                let applied = session.apply(SessionEvent::EnterMatch {
                    epoch,
                    bootstrap_id,
                });
                if applied.changed {
                    diag::info!(Net, "session-transition {}", applied.transition);
                }
            }
        }
        master_protocol::PeerEvent::VoteToSkip { member_id } => {
            skip_voters.insert(member_id);
            if let Some(view) = last_view {
                publish_room_view(
                    state,
                    *identity,
                    view,
                    u8::try_from(skip_voters.len()).unwrap_or(0),
                    is_host,
                    identity.member_id,
                );
            }
        }
        _ => {}
    }
    Ok(())
}

fn enqueue_bootstrap_job(
    match_key: frame::MatchKey,
    jobs: &tokio::sync::mpsc::Sender<(frame::MatchKey, MemberId, Vec<u8>)>,
    member: MemberId,
    bytes: Vec<u8>,
    role: &'static str,
) -> std::result::Result<(), TransportFault> {
    if !matches!(decode_bootstrap(&bytes), Ok(Some(BootstrapMessage::Offer { epoch, .. })) if !match_key.is_none() && epoch == match_key.match_epoch)
    {
        return Ok(());
    }
    classify_try_send(jobs.try_send((match_key, member, bytes)), "bootstrap_job").map_err(|loss| {
        TransportFault::new(
            match loss {
                QueueLoss::Full { operation } | QueueLoss::Closed { operation } => operation,
            },
            role,
            loss.to_string(),
        )
    })
}

fn pump_local_queues(
    match_key: frame::MatchKey,
    bootstrap: &BootstrapLane,
    control_tx: &tokio::sync::mpsc::Sender<ControlFrame>,
    jobs: &tokio::sync::mpsc::Sender<(frame::MatchKey, MemberId, Vec<u8>)>,
    role: &'static str,
    is_host: bool,
    request_id: &mut u64,
    map_ready: &Mutex<HashSet<MemberId>>,
) -> std::result::Result<(), TransportFault> {
    for (peer, bytes) in bootstrap.take_to_worker() {
        if is_host {
            let Some(member) = peer else {
                continue;
            };
            if map_ready
                .lock()
                .expect("bootstrap readiness poisoned")
                .contains(&member)
            {
                enqueue_bootstrap_job(match_key, jobs, member, bytes, role)?;
            } else {
                // The capture side already refuses to snapshot a peer that has
                // not reported its map, so reaching here means the readiness
                // was dropped between the two — the peer would otherwise wait
                // out the whole match with nothing said.
                return Err(TransportFault::new(
                    "bootstrap_readiness",
                    role,
                    "queued bootstrap lost map readiness",
                ));
            }
            continue;
        }
        match decode_bootstrap(&bytes) {
            Ok(Some(BootstrapMessage::Applied {
                epoch,
                bootstrap_id,
                snapshot_seq,
                connection: conn,
                ..
            })) => {
                enqueue_control(
                    control_tx,
                    ControlFrame::Request(ControlRequest {
                        request_id: take_id(request_id),
                        body: RequestBody::BootstrapApplied {
                            epoch,
                            bootstrap_id,
                            snapshot_seq,
                            connection: conn.0,
                        },
                    }),
                    role,
                )?;
            }
            Ok(_) => {}
            Err(error) => {
                diag::warn!(Net, "master join ignored bootstrap to-worker: {error}");
            }
        }
    }
    Ok(())
}

async fn bootstrap_egress(
    connection: quinn::Connection,
    mut jobs: tokio::sync::mpsc::Receiver<(frame::MatchKey, MemberId, Vec<u8>)>,
    prepared: tokio::sync::mpsc::Sender<HostMatchEvent>,
    cancel: CancellationToken,
    role: &'static str,
) -> std::result::Result<(), TransportFault> {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            job = jobs.recv() => {
                let Some((match_key, member, bytes)) = job else {
                    return Ok(());
                };
                send_bootstrap_stream(&connection, member, &bytes, &cancel, role).await?;
                if let Ok(Some(BootstrapMessage::Offer {
                    bootstrap_id,
                    connection: conn,
                    ..
                })) = decode_bootstrap(&bytes)
                {
                    classify_try_send(
                        prepared.try_send(HostMatchEvent::BootstrapPrepared {
                            match_key,
                            member,
                            bootstrap_id,
                            connection_id: Some(conn.0),
                        }),
                        "bootstrap_prepared",
                    )
                    .map_err(|loss| {
                        TransportFault::new(
                            match loss {
                                QueueLoss::Full { operation } | QueueLoss::Closed { operation } => {
                                    operation
                                }
                            },
                            role,
                            loss.to_string(),
                        )
                    })?;
                }
            }
        }
    }
}

async fn datagram_ingress(
    connection: quinn::Connection,
    mailbox: RelayMailbox,
    cancel: CancellationToken,
    role: &'static str,
    is_host: bool,
) -> std::result::Result<(), TransportFault> {
    let mut reassemblers: HashMap<MemberId, Reassembler> = HashMap::new();
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            datagram = connection.read_datagram() => {
                let bytes = match datagram {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        return Err(TransportFault::with_connection(
                            "read_datagram",
                            role,
                            error,
                            &connection,
                        ));
                    }
                };
                let decoded = match decode_relay(&bytes) {
                    Ok(decoded) => decoded,
                    Err(error) => {
                        diag::warn!(Net, "master {role} ignored relay datagram: {error}");
                        continue;
                    }
                };
                let (member, payload) = match decoded {
                    RelayDatagram::ServiceToHost { member_id, payload } if is_host => {
                        (member_id, payload)
                    }
                    RelayDatagram::ServiceToMember(payload) if !is_host => {
                        (MemberId([0; 16]), payload)
                    }
                    other => {
                        diag::warn!(Net, "master {role} ignored unexpected datagram {other:?}");
                        continue;
                    }
                };
                let reassembler = reassemblers.entry(member).or_insert_with(relay_reassembler);
                match reassembler.push(payload) {
                    Ok(Some(packet)) => {
                        if mailbox.push_inbound(member, packet) {
                            perf::net_leg("mail_drop_in", 0, 1);
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        diag::warn!(Net, "master {role} fragment refused: {error}");
                    }
                }
            }
        }
    }
}

async fn uni_ingress(
    connection: quinn::Connection,
    bootstrap: Arc<BootstrapLane>,
    cancel: CancellationToken,
    role: &'static str,
) -> std::result::Result<(), TransportFault> {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            stream = connection.accept_uni() => {
                let mut recv = match stream {
                    Ok(recv) => recv,
                    Err(error) => {
                        return Err(TransportFault::with_connection(
                            "accept_uni",
                            role,
                            error,
                            &connection,
                        ));
                    }
                };
                let bytes = match tokio::time::timeout(
                    IO_DEADLINE,
                    recv.read_to_end(MAX_BOOTSTRAP_STREAM_BYTES),
                )
                .await
                {
                    Ok(Ok(bytes)) => bytes,
                    Ok(Err(error)) => {
                        return Err(TransportFault::with_connection(
                            "bootstrap_uni_read",
                            role,
                            error,
                            &connection,
                        ));
                    }
                    Err(_) => {
                        return Err(TransportFault::with_connection(
                            "bootstrap_uni_read",
                            role,
                            "deadline",
                            &connection,
                        ));
                    }
                };
                match decode_relay_stream(&bytes) {
                    Ok(RelayDatagram::ServiceToMember(payload) | RelayDatagram::HostToMember { payload, .. }) => {
                        if let Err(error) = bootstrap.push_from_worker(None, payload.to_vec()) {
                            return Err(match error {
                                "bootstrap from-worker queue is full" => TransportFault::new(
                                    "bootstrap_from_worker",
                                    role,
                                    QueueLoss::Full {
                                        operation: "bootstrap_from_worker",
                                    }
                                    .to_string(),
                                ),
                                other => TransportFault::new("bootstrap_from_worker", role, other),
                            });
                        }
                    }
                    Ok(other) => {
                        diag::warn!(Net, "master {role} ignored unexpected uni {other:?}");
                    }
                    Err(error) => {
                        diag::warn!(Net, "master {role} ignored unfinished bootstrap stream: {error}");
                    }
                }
            }
        }
    }
}

async fn gameplay_egress(
    connection: quinn::Connection,
    mailbox: RelayMailbox,
    cancel: CancellationToken,
    role: &'static str,
    is_host: bool,
) -> std::result::Result<(), TransportFault> {
    let mut fragmenter = relay_fragmenter();
    let mut tick = tokio::time::interval(QUEUE_POLL);
    let mut sampled = tokio::time::Instant::now();
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            _ = tick.tick() => {
                if sampled.elapsed() >= Duration::from_millis(100) {
                    sampled = tokio::time::Instant::now();
                    let stats = connection.stats();
                    perf::net_path(
                        role,
                        stats.path.rtt.as_micros() as u64,
                        stats.path.cwnd,
                        stats.path.lost_packets,
                        stats.path.congestion_events,
                        (DATAGRAM_SEND_BUFFER - connection.datagram_send_buffer_space()) as u64,
                    );
                }
                for (member, packet) in mailbox.take_outbound() {
                    let fragments = match fragmenter.split(&packet) {
                        Ok(fragments) => fragments,
                        Err(error) => {
                            diag::warn!(Net, "master {role} packet not splittable: {error}");
                            continue;
                        }
                    };
                    for fragment in fragments {
                        let datagram = if is_host {
                            RelayDatagram::HostToMember {
                                member_id: member,
                                payload: &fragment,
                            }
                        } else {
                            RelayDatagram::ClientToHost(&fragment)
                        };
                        let bytes = encode_relay(datagram).map_err(|error| {
                            TransportFault::with_connection("encode_relay", role, error, &connection)
                        })?;
                        if let Err(error) = connection.send_datagram(bytes.into()) {
                            return Err(TransportFault::with_connection(
                                "send_datagram",
                                role,
                                error,
                                &connection,
                            ));
                        }
                    }
                }
            }
        }
    }
}

async fn send_bootstrap_stream(
    connection: &quinn::Connection,
    member_id: MemberId,
    payload: &[u8],
    cancel: &CancellationToken,
    role: &'static str,
) -> std::result::Result<(), TransportFault> {
    let bytes = encode_relay_stream(RelayDatagram::HostToMember { member_id, payload }).map_err(
        |error| TransportFault::with_connection("encode_bootstrap", role, error, connection),
    )?;
    if bytes.len() > MAX_BOOTSTRAP_STREAM_BYTES {
        return Err(TransportFault::with_connection(
            "encode_bootstrap",
            role,
            format!(
                "relay bootstrap envelope {} bytes exceeds stream budget {MAX_BOOTSTRAP_STREAM_BYTES}",
                bytes.len()
            ),
            connection,
        ));
    }
    let mut send = io_timeout(cancel, connection.open_uni())
        .await
        .map_err(|error| {
            TransportFault::with_connection("open_bootstrap_uni", role, error, connection)
        })?;
    send.set_priority(BOOTSTRAP_PRIORITY).map_err(|error| {
        TransportFault::with_connection("bootstrap_priority", role, error, connection)
    })?;
    io_timeout(cancel, send.write_all(&bytes))
        .await
        .map_err(|error| {
            TransportFault::with_connection("bootstrap_uni_write", role, error, connection)
        })?;
    send.finish().map_err(|error| {
        TransportFault::with_connection("bootstrap_uni_finish", role, error, connection)
    })?;
    Ok(())
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

async fn io_timeout<T>(
    cancel: &CancellationToken,
    fut: impl std::future::Future<Output = std::result::Result<T, impl Into<Error>>>,
) -> Result<T> {
    tokio::select! {
        _ = cancel.cancelled() => Err("session cancelled".into()),
        result = tokio::time::timeout(IO_DEADLINE, fut) => {
            match result {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(error)) => Err(error.into()),
                Err(_) => Err("master I/O deadline".into()),
            }
        }
    }
}

async fn connect(
    target: &MasterTarget,
    cancel: &CancellationToken,
) -> Result<(quinn::Endpoint, quinn::Connection)> {
    let address = target
        .address
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| format!("{} resolved no addresses", target.address))?;
    let mut crypto = rustls::ClientConfig::builder()
        .with_root_certificates(updater::trust_roots(&target.ca_pem)?)
        .with_no_client_auth();
    crypto.alpn_protocols = vec![ALPN.to_vec()];
    let mut client_config = quinn::ClientConfig::new(Arc::new(QuicClientConfig::try_from(crypto)?));
    let mut transport = quinn::TransportConfig::default();
    transport.max_idle_timeout(Some(
        quinn::IdleTimeout::try_from(SESSION_IDLE).expect("session idle fits QUIC VarInt"),
    ));
    transport.keep_alive_interval(Some(SESSION_KEEP_ALIVE));
    transport.max_concurrent_uni_streams(MAX_RELAY_UNI_STREAMS.into());
    client_config.transport_config(Arc::new(transport));

    let bind = if address.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let mut endpoint = quinn::Endpoint::client(bind.parse()?)?;
    endpoint.set_default_client_config(client_config);
    let local = endpoint.local_addr()?;
    let trust = "community-ca";
    diag::info!(
        Net,
        "master quic handshake begin local={} remote={} server_name={} trust={} alpn={:?}",
        local,
        address,
        target.server_name,
        trust,
        ALPN
    );
    let started = Instant::now();
    let connecting = endpoint.connect(address, &target.server_name)?;
    let connection = io_timeout(cancel, connecting)
        .await
        .map_err(|error| -> Error {
            format!(
            "quic handshake: {error}; local={local} remote={address} server_name={} elapsed_ms={}",
            target.server_name,
            started.elapsed().as_millis()
        )
        .into()
        })?;
    diag::info!(
        Net,
        "master quic handshake ok local={} remote={} elapsed_ms={}",
        local,
        connection.remote_address(),
        started.elapsed().as_millis()
    );
    Ok((endpoint, connection))
}

fn handshake_for_match(
    world: Option<&sim::SimWorld>,
    descriptor: Option<&MatchDescriptor>,
) -> Option<HandshakeHello> {
    let descriptor = descriptor?;
    let mut content = ContentFingerprint {
        map: descriptor.map,
        weapons: descriptor.weapons,
        classes: descriptor.classes,
        gameplay: 0,
        models: 0,
    };
    if let Some(world) = world.filter(|world| world.has_world_clip()) {
        let live = ContentFingerprint::from_world(world);
        content.gameplay = live.gameplay;
        content.models = live.models;
    }
    Some(HandshakeHello::current(content))
}

fn refresh_relay_authority_hello(
    hub: Option<ResMut<UdpAuthorityHub>>,
    authority: Option<Res<AuthorityWorld>>,
    descriptor: Option<Res<MatchDescriptor>>,
) {
    let Some(mut hub) = hub else {
        return;
    };
    let Some(descriptor) = descriptor.as_deref() else {
        return;
    };
    let Some(authority) = authority else {
        return;
    };
    let Some(hello) = handshake_for_match(Some(&authority.0), Some(descriptor)) else {
        return;
    };
    let content = hello.content;
    if hub.hello.content == content {
        return;
    }
    let was = hub.hello.content;

    if (was.map, was.weapons, was.classes) != (content.map, content.weapons, content.classes) {
        diag::info!(
            Net,
            "udp authority content now map={:016x} weapons={:016x} classes={:016x} (was {:016x} / {:016x} / {:016x})",
            content.map,
            content.weapons,
            content.classes,
            was.map,
            was.weapons,
            was.classes
        );
    }
    hub.hello.content = content;
}
