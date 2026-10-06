#![forbid(unsafe_code)]

use core::{fmt, str::FromStr};

pub const PROTOCOL_VERSION: u16 = 10;
pub const ALPN: &[u8] = b"iw4l-master/10";
pub const MAX_CONTROL_BYTES: usize = 16 * 1024;
pub const MAX_OPAQUE_PAYLOAD: usize = 1100;

pub const MAX_RELAY_STREAM_BYTES: usize = 2 + 16 + MAX_OPAQUE_PAYLOAD;

pub const MAX_BOOTSTRAP_PAYLOAD: usize = 256 * 1024;
pub const MAX_BOOTSTRAP_STREAM_BYTES: usize = 2 + 16 + MAX_BOOTSTRAP_PAYLOAD;

pub const MAX_RELAY_UNI_STREAMS: u32 = 8;
pub const MAX_CONCURRENT_BOOTSTRAP: u32 = 4;
pub const MAX_SESSION_MEMBERS: u8 = 18;
pub const MAX_ADVERT_NAME_BYTES: usize = 48;
pub const MAX_MAP_BYTES: usize = 64;
pub const MAX_MODE_BYTES: usize = 24;
pub const MAX_BUILD_BYTES: usize = 64;
pub const MAX_PLAYER_NAME_BYTES: usize = 64;
pub const MAX_LIST_ADVERTS: usize = 128;

pub const SESSION_IDLE: core::time::Duration = core::time::Duration::from_secs(8);
pub const SESSION_KEEP_ALIVE: core::time::Duration = core::time::Duration::from_secs(1);

/// One published master: UDP port, systemd unit, and the certificate label a
/// client verifies against. `serve`, `print-unit` and `cargo xtask` all read
/// these three from here, so the unit on the VPS cannot drift away from the
/// flags the binary parses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Prod,
    Dev,
}

impl Channel {
    pub const ALL: [Self; 2] = [Self::Prod, Self::Dev];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prod => "prod",
            Self::Dev => "dev",
        }
    }

    /// The UDP port `serve --bind` listens on and the client dials.
    pub const fn port(self) -> u16 {
        match self {
            Self::Prod => 4433,
            Self::Dev => 4434,
        }
    }

    pub const fn unit(self) -> &'static str {
        match self {
            Self::Prod => "iw4l-master.service",
            Self::Dev => "iw4l-master-dev.service",
        }
    }

    /// The channel label included in server certificates.
    /// A fixed label, never the host — `docs/MASTER.md` explains why a master
    /// therefore needs no domain.
    pub const fn server_name(self) -> &'static str {
        match self {
            Self::Prod => "iw4l-prod",
            Self::Dev => "iw4l-dev",
        }
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownChannel;

impl fmt::Display for UnknownChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "channel must be prod or dev")
    }
}

impl std::error::Error for UnknownChannel {}

impl FromStr for Channel {
    type Err = UnknownChannel;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "prod" => Ok(Self::Prod),
            "dev" => Ok(Self::Dev),
            _ => Err(UnknownChannel),
        }
    }
}

const MAGIC: [u8; 4] = *b"IW4M";
const HEADER_BYTES: usize = 17;
const STREAM_LEN_BYTES: usize = 4;

const KIND_HELLO: u8 = 0;
const KIND_REQUEST: u8 = 1;
const KIND_RESPONSE: u8 = 2;
const KIND_ROOM_VIEW: u8 = 3;
const KIND_CLOSED: u8 = 4;
const KIND_PEER_EVENT: u8 = 5;

const TAG_STATUS: u8 = 1;
const TAG_CREATE_ROOM: u8 = 2;
const TAG_JOIN_ROOM: u8 = 3;
const TAG_LEAVE_ROOM: u8 = 4;
const TAG_SET_OPTIONS: u8 = 5;
const TAG_START_MATCH: u8 = 6;
const TAG_END_MATCH: u8 = 7;
const TAG_CLOSE_ROOM: u8 = 8;
const TAG_LIST_ROOMS: u8 = 9;
const TAG_HOST_WORLD_READY: u8 = 10;
const TAG_MAP_LOADED: u8 = 11;
const TAG_BOOTSTRAP_APPLIED: u8 = 12;
const TAG_ENTER_MATCH: u8 = 13;
const TAG_VOTE_TO_SKIP: u8 = 14;
const TAG_SET_PASSWORD: u8 = 16;
pub const MAX_PASSWORD_BYTES: usize = 64;
const TAG_ADMISSION_FAILED: u8 = 15;
const TAG_ERROR: u8 = 0x7f;

const EVENT_HOST_WORLD_READY: u8 = 1;
const EVENT_MAP_LOADED: u8 = 2;
const EVENT_BOOTSTRAP_APPLIED: u8 = 3;
const EVENT_ENTER_MATCH: u8 = 4;
const EVENT_VOTE_TO_SKIP: u8 = 5;
const EVENT_ADMISSION_FAILED: u8 = 6;

macro_rules! id_type {
    ($name:ident, $len:expr) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub [u8; $len]);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                for byte in self.0 {
                    write!(f, "{byte:02x}")?;
                }
                Ok(())
            }
        }

        impl FromStr for $name {
            type Err = ProtocolError;

            fn from_str(raw: &str) -> Result<Self, ProtocolError> {
                if raw.len() != $len * 2 {
                    return Err(ProtocolError::InvalidId);
                }
                let mut bytes = [0; $len];
                for (index, byte) in bytes.iter_mut().enumerate() {
                    *byte = u8::from_str_radix(&raw[index * 2..index * 2 + 2], 16)
                        .map_err(|_| ProtocolError::InvalidId)?;
                }
                Ok(Self(bytes))
            }
        }
    };
}

id_type!(AdvertId, 16);
id_type!(MemberId, 16);

pub type RoomId = AdvertId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ContentFlags(pub u8);

impl ContentFlags {
    pub const fn missing_from(self, have: Self) -> Self {
        Self(self.0 & !have.0)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionFailure {
    Deadline,

    Cancelled,
    MapContent { host: u64, peer: u64 },
    WeaponRegistry { host: u64, peer: u64 },
    ClassRegistry { host: u64, peer: u64 },
}

impl AdmissionFailure {
    fn encode(self, out: &mut Writer) {
        match self {
            Self::Deadline => out.u8(1),
            Self::Cancelled => out.u8(2),
            Self::MapContent { host, peer }
            | Self::WeaponRegistry { host, peer }
            | Self::ClassRegistry { host, peer } => {
                out.u8(match self {
                    Self::MapContent { .. } => 3,
                    Self::WeaponRegistry { .. } => 4,
                    Self::ClassRegistry { .. } => 5,
                    _ => unreachable!(),
                });
                out.u64(host);
                out.u64(peer);
            }
        }
    }

    fn decode(input: &mut Reader<'_>) -> Result<Self, ProtocolError> {
        Ok(match input.u8()? {
            1 => Self::Deadline,
            2 => Self::Cancelled,
            3 => Self::MapContent {
                host: input.u64()?,
                peer: input.u64()?,
            },
            4 => Self::WeaponRegistry {
                host: input.u64()?,
                peer: input.u64()?,
            },
            5 => Self::ClassRegistry {
                host: input.u64()?,
                peer: input.u64()?,
            },
            _ => return Err(ProtocolError::InvalidBody),
        })
    }
}

impl fmt::Display for AdmissionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Deadline => write!(f, "peer admission deadline"),
            Self::Cancelled => write!(f, "peer admission cancelled"),
            Self::MapContent { host, peer } => {
                write!(f, "map content mismatch: host={host:016x} peer={peer:016x}")
            }
            Self::WeaponRegistry { host, peer } => write!(
                f,
                "weapon table mismatch: host={host:016x} peer={peer:016x}"
            ),
            Self::ClassRegistry { host, peer } => write!(
                f,
                "class catalog mismatch: host={host:016x} peer={peer:016x}; use the same game build as the host"
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointRole {
    Cli,
    Host,
    Join,
}

impl EndpointRole {
    fn wire(self) -> u8 {
        match self {
            Self::Cli => 0,
            Self::Host => 1,
            Self::Join => 2,
        }
    }

    fn from_wire(value: u8) -> Result<Self, ProtocolError> {
        match value {
            0 => Ok(Self::Cli),
            1 => Ok(Self::Host),
            2 => Ok(Self::Join),
            _ => Err(ProtocolError::InvalidBody),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlHello {
    pub protocol_version: u16,
    pub game_protocol: u32,
    pub role: EndpointRole,
    pub build: String,
    pub player_name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomPhase {
    Gathering,
    Loading,
    Running,
}

impl RoomPhase {
    pub const fn in_match(self) -> bool {
        !matches!(self, Self::Gathering)
    }

    fn wire(self) -> u8 {
        match self {
            Self::Gathering => 0,
            Self::Loading => 1,
            Self::Running => 2,
        }
    }

    fn from_wire(value: u8) -> Result<Self, ProtocolError> {
        match value {
            0 => Ok(Self::Gathering),
            1 => Ok(Self::Loading),
            2 => Ok(Self::Running),
            _ => Err(ProtocolError::InvalidBody),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomView {
    pub room_id: AdvertId,
    pub name: String,
    pub host: MemberId,
    pub members: Vec<MemberId>,
    pub member_names: std::collections::HashMap<MemberId, String>,
    pub map: String,
    pub mode: String,
    pub joinable: bool,
    pub revision: u64,
    pub epoch: u32,
    pub phase: RoomPhase,
    pub max_players: u8,
    pub requires: ContentFlags,
    pub available: ContentFlags,
    pub password_protected: bool,
}

impl RoomView {
    pub fn advert(&self) -> Advert {
        Advert {
            id: self.room_id,
            name: self
                .member_names
                .get(&self.host)
                .cloned()
                .unwrap_or_else(|| self.name.clone()),
            map: self.map.clone(),
            mode: self.mode.clone(),
            players: u8::try_from(self.members.len()).unwrap_or(MAX_SESSION_MEMBERS),
            max_players: self.max_players,
            locked: !self.joinable,
            in_match: self.phase.in_match(),
            requires: self.requires,
            available: self.available,
            password_protected: self.password_protected,
            generation: self.revision,
        }
    }

    pub fn contains(&self, member_id: MemberId) -> bool {
        self.members.iter().any(|id| *id == member_id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Advert {
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
    pub generation: u64,
}

impl Advert {
    pub const fn allows_join_request(&self) -> bool {
        request_join_allowed(self.locked)
    }
}

pub const fn request_join_allowed(locked: bool) -> bool {
    !locked
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestBody {
    Status,
    CreateRoom {
        name: String,
        map: String,
        mode: String,
        max_players: u8,
        requires: ContentFlags,
        available: ContentFlags,
        password: String,
    },
    JoinRoom {
        room_id: AdvertId,
        have: ContentFlags,
        password: String,
    },
    SetPassword {
        password: String,
    },
    LeaveRoom,
    SetOptions {
        map: String,
        mode: String,
        joinable: bool,
    },
    StartMatch {
        map: String,
        mode: String,
    },
    EndMatch {
        room_id: AdvertId,
        epoch: u32,
    },
    CloseRoom,
    ListRooms,
    HostWorldReady {
        room_id: AdvertId,
        epoch: u32,
        map: u64,
        weapons: u64,
        classes: u64,
    },
    MapLoaded {
        epoch: u32,
        map: u64,
        weapons: u64,
        classes: u64,
    },
    BootstrapApplied {
        epoch: u32,
        bootstrap_id: u32,
        snapshot_seq: u32,
        connection: u64,
    },
    EnterMatch {
        member_id: MemberId,
        epoch: u32,
        bootstrap_id: u32,
        connection_id: u64,
        client_id: u32,
    },

    AdmissionFailed {
        member_id: MemberId,
        epoch: u32,
        reason: AdmissionFailure,
    },
    VoteToSkip,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlRequest {
    pub request_id: u64,
    pub body: RequestBody,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusResponse {
    pub protocol_version: u16,
    pub max_opaque_payload: u16,
    pub max_session_members: u8,
}

impl StatusResponse {
    pub const CURRENT: Self = Self {
        protocol_version: PROTOCOL_VERSION,
        max_opaque_payload: MAX_OPAQUE_PAYLOAD as u16,
        max_session_members: MAX_SESSION_MEMBERS,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceError {
    Malformed,
    InvalidAdvert,
    AdvertCapacity,
    AlreadyHosting,
    UnknownAdvert,
    Full,
    Locked,
    IncorrectPassword,
    NotHost,
    NotMember,
    MissingContent {
        requires: ContentFlags,
        have: ContentFlags,
    },
}

impl ServiceError {
    fn from_reader(body: &mut Reader<'_>) -> Result<Self, ProtocolError> {
        let value = body.u8()?;
        Ok(match value {
            1 => Self::Malformed,
            2 => Self::InvalidAdvert,
            3 => Self::AdvertCapacity,
            4 => Self::AlreadyHosting,
            5 => Self::UnknownAdvert,
            7 => Self::Full,
            8 => Self::Locked,
            15 => Self::IncorrectPassword,
            11 => Self::NotHost,
            13 => Self::NotMember,
            14 => Self::MissingContent {
                requires: ContentFlags(body.u8()?),
                have: ContentFlags(body.u8()?),
            },
            _ => return Err(ProtocolError::InvalidBody),
        })
    }

    fn write(self, out: &mut Writer) {
        let tag = match self {
            Self::Malformed => 1,
            Self::InvalidAdvert => 2,
            Self::AdvertCapacity => 3,
            Self::AlreadyHosting => 4,
            Self::UnknownAdvert => 5,
            Self::Full => 7,
            Self::Locked => 8,
            Self::IncorrectPassword => 15,
            Self::NotHost => 11,
            Self::NotMember => 13,
            Self::MissingContent { .. } => 14,
        };
        out.u8(tag);
        if let Self::MissingContent { requires, have } = self {
            out.u8(requires.0);
            out.u8(have.0);
        }
    }
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ServiceError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResponseBody {
    Status(StatusResponse),
    RoomCreated {
        member_id: MemberId,
        view: RoomView,
    },
    RoomJoined {
        member_id: MemberId,
        view: RoomView,
    },
    RoomLeft,
    RoomUpdated {
        view: RoomView,
    },
    RoomList {
        generation: u64,
        adverts: Vec<Advert>,
    },
    Ack,
    Error(ServiceError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionCloseReason {
    HostLeft,
    LeaseExpired,
    Overload,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlResponse {
    pub request_id: u64,
    pub body: ResponseBody,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeerEvent {
    HostWorldReady {
        epoch: u32,
        map: u64,
        weapons: u64,
        classes: u64,
    },
    MapLoaded {
        member_id: MemberId,
        epoch: u32,
        map: u64,
        weapons: u64,
        classes: u64,
    },
    BootstrapApplied {
        member_id: MemberId,
        epoch: u32,
        bootstrap_id: u32,
        snapshot_seq: u32,
        connection: u64,
    },
    EnterMatch {
        member_id: MemberId,
        epoch: u32,
        bootstrap_id: u32,
        connection_id: u64,
        client_id: u32,
    },
    AdmissionFailed {
        member_id: MemberId,
        epoch: u32,
        reason: AdmissionFailure,
    },
    VoteToSkip {
        member_id: MemberId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlFrame {
    Relay(Vec<u8>),
    Hello(ControlHello),
    Request(ControlRequest),
    Response(ControlResponse),
    RoomView(RoomView),
    Closed {
        room_id: AdvertId,
        epoch: u32,
        reason: SessionCloseReason,
    },
    PeerEvent(PeerEvent),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayDatagram<'a> {
    ClientToHost(&'a [u8]),
    HostToMember {
        member_id: MemberId,
        payload: &'a [u8],
    },
    ServiceToHost {
        member_id: MemberId,
        payload: &'a [u8],
    },
    ServiceToMember(&'a [u8]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    Truncated,
    BadMagic,
    VersionMismatch { ours: u16, theirs: u16 },
    UnknownTag(u8),
    LengthExceeded(usize),
    LengthMismatch { declared: usize, actual: usize },
    InvalidBody,
    InvalidUtf8,
    InvalidId,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => write!(f, "control message is truncated"),
            Self::BadMagic => write!(f, "control message has invalid magic"),
            Self::VersionMismatch { ours, theirs } => {
                write!(f, "protocol version ours={ours} theirs={theirs}")
            }
            Self::UnknownTag(tag) => write!(f, "unknown control tag {tag}"),
            Self::LengthExceeded(len) => write!(f, "control message length {len} exceeds limit"),
            Self::LengthMismatch { declared, actual } => {
                write!(f, "control body length declared={declared} actual={actual}")
            }
            Self::InvalidBody => write!(f, "control message body is invalid"),
            Self::InvalidUtf8 => write!(f, "control string is not UTF-8"),
            Self::InvalidId => write!(f, "identifier is not canonical lowercase hex"),
        }
    }
}

impl std::error::Error for ProtocolError {}

pub fn encode_stream_frame(frame: &ControlFrame) -> Result<Vec<u8>, ProtocolError> {
    let inner = encode_control_frame(frame)?;
    if inner.len() > MAX_CONTROL_BYTES {
        return Err(ProtocolError::LengthExceeded(inner.len()));
    }
    let mut out = Vec::with_capacity(STREAM_LEN_BYTES + inner.len());
    out.extend_from_slice(&(inner.len() as u32).to_be_bytes());
    out.extend_from_slice(&inner);
    Ok(out)
}

pub fn decode_stream_payload(inner: &[u8]) -> Result<ControlFrame, ProtocolError> {
    decode_control_frame(inner)
}

pub fn stream_frame_len(header: [u8; 4]) -> Result<usize, ProtocolError> {
    let len = u32::from_be_bytes(header) as usize;
    if len == 0 || len > MAX_CONTROL_BYTES {
        return Err(ProtocolError::LengthExceeded(len));
    }
    Ok(len)
}

pub fn encode_request(request: &ControlRequest) -> Result<Vec<u8>, ProtocolError> {
    encode_control_frame(&ControlFrame::Request(request.clone()))
}

pub fn decode_request(bytes: &[u8]) -> Result<ControlRequest, ProtocolError> {
    match decode_control_frame(bytes)? {
        ControlFrame::Request(request) => Ok(request),
        _ => Err(ProtocolError::InvalidBody),
    }
}

pub fn encode_response(response: &ControlResponse) -> Result<Vec<u8>, ProtocolError> {
    encode_control_frame(&ControlFrame::Response(response.clone()))
}

pub fn decode_response(bytes: &[u8]) -> Result<ControlResponse, ProtocolError> {
    match decode_control_frame(bytes)? {
        ControlFrame::Response(response) => Ok(response),
        _ => Err(ProtocolError::InvalidBody),
    }
}

pub fn encode_control_frame(frame: &ControlFrame) -> Result<Vec<u8>, ProtocolError> {
    let (kind, request_id, body) = match frame {
        ControlFrame::Relay(bytes) => {
            if bytes.len() > MAX_CONTROL_BYTES - HEADER_BYTES {
                return Err(ProtocolError::LengthExceeded(bytes.len()));
            }
            (6, 0, bytes.clone())
        }
        ControlFrame::Hello(hello) => {
            let mut out = Writer::default();
            out.u16(hello.protocol_version);
            out.u32(hello.game_protocol);
            out.u8(hello.role.wire());
            out.string(&hello.build, MAX_BUILD_BYTES)?;
            out.string(&hello.player_name, MAX_PLAYER_NAME_BYTES)?;
            (KIND_HELLO, 0, out.0)
        }
        ControlFrame::Request(request) => (
            KIND_REQUEST,
            request.request_id,
            encode_request_body(&request.body)?,
        ),
        ControlFrame::Response(response) => (
            KIND_RESPONSE,
            response.request_id,
            encode_response_body(&response.body)?,
        ),
        ControlFrame::RoomView(view) => (KIND_ROOM_VIEW, 0, encode_room_view(view)?),
        ControlFrame::Closed {
            room_id,
            epoch,
            reason,
        } => {
            let mut out = Writer::default();
            out.bytes(&room_id.0);
            out.u32(*epoch);
            out.u8(reason.wire());
            (KIND_CLOSED, 0, out.0)
        }
        ControlFrame::PeerEvent(event) => (KIND_PEER_EVENT, 0, encode_peer_event(event)),
    };
    encode_message(kind, request_id, body)
}

pub fn decode_control_frame(bytes: &[u8]) -> Result<ControlFrame, ProtocolError> {
    let header = decode_header(bytes)?;
    let mut body = Reader::new(header.body);
    let frame = match header.tag {
        6 => ControlFrame::Relay(header.body.to_vec()),
        KIND_HELLO => {
            let hello = ControlHello {
                protocol_version: body.u16()?,
                game_protocol: body.u32()?,
                role: EndpointRole::from_wire(body.u8()?)?,
                build: body.string(MAX_BUILD_BYTES)?,
                player_name: body.string(MAX_PLAYER_NAME_BYTES)?,
            };
            body.finish()?;
            ControlFrame::Hello(hello)
        }
        KIND_REQUEST => {
            let request = ControlRequest {
                request_id: header.request_id,
                body: decode_request_body(&mut body)?,
            };
            body.finish()?;
            ControlFrame::Request(request)
        }
        KIND_RESPONSE => {
            let response = ControlResponse {
                request_id: header.request_id,
                body: decode_response_body(&mut body)?,
            };
            body.finish()?;
            ControlFrame::Response(response)
        }
        KIND_ROOM_VIEW => {
            let view = decode_room_view(&mut body)?;
            body.finish()?;
            ControlFrame::RoomView(view)
        }
        KIND_CLOSED => {
            let room_id = AdvertId(body.array()?);
            let epoch = body.u32()?;
            let reason = SessionCloseReason::from_wire(body.u8()?)?;
            body.finish()?;
            ControlFrame::Closed {
                room_id,
                epoch,
                reason,
            }
        }
        KIND_PEER_EVENT => {
            let event = decode_peer_event(&mut body)?;
            body.finish()?;
            ControlFrame::PeerEvent(event)
        }
        tag => return Err(ProtocolError::UnknownTag(tag)),
    };
    Ok(frame)
}

fn encode_request_body(body: &RequestBody) -> Result<Vec<u8>, ProtocolError> {
    let mut out = Writer::default();
    match body {
        RequestBody::Status => out.u8(TAG_STATUS),
        RequestBody::CreateRoom {
            name,
            map,
            mode,
            max_players,
            requires,
            available,
            password,
        } => {
            validate_advert(name, map, mode, *max_players)?;
            out.u8(TAG_CREATE_ROOM);
            out.string(name, MAX_ADVERT_NAME_BYTES)?;
            out.string(map, MAX_MAP_BYTES)?;
            out.string(mode, MAX_MODE_BYTES)?;
            out.u8(*max_players);
            out.u8(requires.0);
            out.u8(available.0);
            out.string(password, MAX_PASSWORD_BYTES)?;
        }
        RequestBody::JoinRoom {
            room_id,
            have,
            password,
        } => {
            out.u8(TAG_JOIN_ROOM);
            out.bytes(&room_id.0);
            out.u8(have.0);
            out.string(password, MAX_PASSWORD_BYTES)?;
        }
        RequestBody::SetPassword { password } => {
            out.u8(TAG_SET_PASSWORD);
            out.string(password, MAX_PASSWORD_BYTES)?;
        }
        RequestBody::LeaveRoom => out.u8(TAG_LEAVE_ROOM),
        RequestBody::SetOptions {
            map,
            mode,
            joinable,
        } => {
            validate_map_mode(map, mode)?;
            out.u8(TAG_SET_OPTIONS);
            out.string(map, MAX_MAP_BYTES)?;
            out.string(mode, MAX_MODE_BYTES)?;
            out.u8(u8::from(*joinable));
        }
        RequestBody::StartMatch { map, mode } => {
            validate_map_mode(map, mode)?;
            out.u8(TAG_START_MATCH);
            out.string(map, MAX_MAP_BYTES)?;
            out.string(mode, MAX_MODE_BYTES)?;
        }
        RequestBody::EndMatch { room_id, epoch } => {
            out.u8(TAG_END_MATCH);
            out.bytes(&room_id.0);
            out.u32(*epoch);
        }
        RequestBody::CloseRoom => out.u8(TAG_CLOSE_ROOM),
        RequestBody::ListRooms => out.u8(TAG_LIST_ROOMS),
        RequestBody::HostWorldReady {
            room_id,
            epoch,
            map,
            weapons,
            classes,
        } => {
            out.u8(TAG_HOST_WORLD_READY);
            out.bytes(&room_id.0);
            out.u32(*epoch);
            out.u64(*map);
            out.u64(*weapons);
            out.u64(*classes);
        }
        RequestBody::MapLoaded {
            epoch,
            map,
            weapons,
            classes,
        } => {
            out.u8(TAG_MAP_LOADED);
            out.u32(*epoch);
            out.u64(*map);
            out.u64(*weapons);
            out.u64(*classes);
        }
        RequestBody::BootstrapApplied {
            epoch,
            bootstrap_id,
            snapshot_seq,
            connection,
        } => {
            out.u8(TAG_BOOTSTRAP_APPLIED);
            out.u32(*epoch);
            out.u32(*bootstrap_id);
            out.u32(*snapshot_seq);
            out.u64(*connection);
        }
        RequestBody::EnterMatch {
            member_id,
            epoch,
            bootstrap_id,
            connection_id,
            client_id,
        } => {
            out.u8(TAG_ENTER_MATCH);
            out.bytes(&member_id.0);
            out.u32(*epoch);
            out.u32(*bootstrap_id);
            out.u64(*connection_id);
            out.u32(*client_id);
        }
        RequestBody::AdmissionFailed {
            member_id,
            epoch,
            reason,
        } => {
            out.u8(TAG_ADMISSION_FAILED);
            out.bytes(&member_id.0);
            out.u32(*epoch);
            reason.encode(&mut out);
        }
        RequestBody::VoteToSkip => out.u8(TAG_VOTE_TO_SKIP),
    }
    Ok(out.0)
}

fn decode_request_body(body: &mut Reader<'_>) -> Result<RequestBody, ProtocolError> {
    Ok(match body.u8()? {
        TAG_STATUS => RequestBody::Status,
        TAG_CREATE_ROOM => {
            let name = body.string(MAX_ADVERT_NAME_BYTES)?;
            let map = body.string(MAX_MAP_BYTES)?;
            let mode = body.string(MAX_MODE_BYTES)?;
            let max_players = body.u8()?;
            let requires = ContentFlags(body.u8()?);
            let available = ContentFlags(body.u8()?);
            let password = body.string(MAX_PASSWORD_BYTES)?;
            validate_advert(&name, &map, &mode, max_players)?;
            RequestBody::CreateRoom {
                name,
                map,
                mode,
                max_players,
                requires,
                available,
                password,
            }
        }
        TAG_JOIN_ROOM => RequestBody::JoinRoom {
            room_id: AdvertId(body.array()?),
            have: ContentFlags(body.u8()?),
            password: body.string(MAX_PASSWORD_BYTES)?,
        },
        TAG_SET_PASSWORD => RequestBody::SetPassword {
            password: body.string(MAX_PASSWORD_BYTES)?,
        },
        TAG_LEAVE_ROOM => RequestBody::LeaveRoom,
        TAG_SET_OPTIONS => {
            let map = body.string(MAX_MAP_BYTES)?;
            let mode = body.string(MAX_MODE_BYTES)?;
            let joinable = bool_byte(body.u8()?)?;
            validate_map_mode(&map, &mode)?;
            RequestBody::SetOptions {
                map,
                mode,
                joinable,
            }
        }
        TAG_START_MATCH => {
            let map = body.string(MAX_MAP_BYTES)?;
            let mode = body.string(MAX_MODE_BYTES)?;
            validate_map_mode(&map, &mode)?;
            RequestBody::StartMatch { map, mode }
        }
        TAG_END_MATCH => RequestBody::EndMatch {
            room_id: AdvertId(body.array()?),
            epoch: body.u32()?,
        },
        TAG_CLOSE_ROOM => RequestBody::CloseRoom,
        TAG_LIST_ROOMS => RequestBody::ListRooms,
        TAG_HOST_WORLD_READY => RequestBody::HostWorldReady {
            room_id: AdvertId(body.array()?),
            epoch: body.u32()?,
            map: body.u64()?,
            weapons: body.u64()?,
            classes: body.u64()?,
        },
        TAG_MAP_LOADED => RequestBody::MapLoaded {
            epoch: body.u32()?,
            map: body.u64()?,
            weapons: body.u64()?,
            classes: body.u64()?,
        },
        TAG_BOOTSTRAP_APPLIED => RequestBody::BootstrapApplied {
            epoch: body.u32()?,
            bootstrap_id: body.u32()?,
            snapshot_seq: body.u32()?,
            connection: body.u64()?,
        },
        TAG_ENTER_MATCH => RequestBody::EnterMatch {
            member_id: MemberId(body.array()?),
            epoch: body.u32()?,
            bootstrap_id: body.u32()?,
            connection_id: body.u64()?,
            client_id: body.u32()?,
        },
        TAG_ADMISSION_FAILED => RequestBody::AdmissionFailed {
            member_id: MemberId(body.array()?),
            epoch: body.u32()?,
            reason: AdmissionFailure::decode(body)?,
        },
        TAG_VOTE_TO_SKIP => RequestBody::VoteToSkip,
        tag => return Err(ProtocolError::UnknownTag(tag)),
    })
}

fn encode_response_body(body: &ResponseBody) -> Result<Vec<u8>, ProtocolError> {
    let mut out = Writer::default();
    match body {
        ResponseBody::Status(status) => {
            out.u8(TAG_STATUS);
            out.u16(status.protocol_version);
            out.u16(status.max_opaque_payload);
            out.u8(status.max_session_members);
        }
        ResponseBody::RoomCreated { member_id, view } => {
            out.u8(TAG_CREATE_ROOM);
            out.bytes(&member_id.0);
            out.0.extend(encode_room_view(view)?);
        }
        ResponseBody::RoomJoined { member_id, view } => {
            out.u8(TAG_JOIN_ROOM);
            out.bytes(&member_id.0);
            out.0.extend(encode_room_view(view)?);
        }
        ResponseBody::RoomLeft => out.u8(TAG_LEAVE_ROOM),
        ResponseBody::RoomUpdated { view } => {
            out.u8(TAG_SET_OPTIONS);
            out.0.extend(encode_room_view(view)?);
        }
        ResponseBody::RoomList {
            generation,
            adverts,
        } => {
            if adverts.len() > MAX_LIST_ADVERTS {
                return Err(ProtocolError::LengthExceeded(adverts.len()));
            }
            out.u8(TAG_LIST_ROOMS);
            out.u64(*generation);
            out.u16(adverts.len() as u16);
            for advert in adverts {
                out.bytes(&advert.id.0);
                out.string(&advert.name, MAX_ADVERT_NAME_BYTES)?;
                out.string(&advert.map, MAX_MAP_BYTES)?;
                out.string(&advert.mode, MAX_MODE_BYTES)?;
                out.u8(advert.players);
                out.u8(advert.max_players);
                out.u8(u8::from(advert.locked));
                out.u8(u8::from(advert.in_match));
                out.u8(advert.requires.0);
                out.u8(advert.available.0);
                out.u8(u8::from(advert.password_protected));
                out.u64(advert.generation);
            }
        }
        ResponseBody::Ack => out.u8(TAG_HOST_WORLD_READY),
        ResponseBody::Error(error) => {
            out.u8(TAG_ERROR);
            error.write(&mut out);
        }
    }
    Ok(out.0)
}

fn decode_response_body(body: &mut Reader<'_>) -> Result<ResponseBody, ProtocolError> {
    Ok(match body.u8()? {
        TAG_STATUS => ResponseBody::Status(StatusResponse {
            protocol_version: body.u16()?,
            max_opaque_payload: body.u16()?,
            max_session_members: body.u8()?,
        }),
        TAG_CREATE_ROOM => ResponseBody::RoomCreated {
            member_id: MemberId(body.array()?),
            view: decode_room_view(body)?,
        },
        TAG_JOIN_ROOM => ResponseBody::RoomJoined {
            member_id: MemberId(body.array()?),
            view: decode_room_view(body)?,
        },
        TAG_LEAVE_ROOM => ResponseBody::RoomLeft,
        TAG_SET_OPTIONS => ResponseBody::RoomUpdated {
            view: decode_room_view(body)?,
        },
        TAG_LIST_ROOMS => {
            let generation = body.u64()?;
            let count = body.u16()? as usize;
            if count > MAX_LIST_ADVERTS {
                return Err(ProtocolError::LengthExceeded(count));
            }
            let mut adverts = Vec::with_capacity(count);
            for _ in 0..count {
                adverts.push(Advert {
                    id: AdvertId(body.array()?),
                    name: body.string(MAX_ADVERT_NAME_BYTES)?,
                    map: body.string(MAX_MAP_BYTES)?,
                    mode: body.string(MAX_MODE_BYTES)?,
                    players: body.u8()?,
                    max_players: body.u8()?,
                    locked: bool_byte(body.u8()?)?,
                    in_match: bool_byte(body.u8()?)?,
                    requires: ContentFlags(body.u8()?),
                    available: ContentFlags(body.u8()?),
                    password_protected: bool_byte(body.u8()?)?,
                    generation: body.u64()?,
                });
            }
            ResponseBody::RoomList {
                generation,
                adverts,
            }
        }
        TAG_HOST_WORLD_READY => ResponseBody::Ack,
        TAG_ERROR => ResponseBody::Error(ServiceError::from_reader(body)?),
        tag => return Err(ProtocolError::UnknownTag(tag)),
    })
}

fn encode_room_view(view: &RoomView) -> Result<Vec<u8>, ProtocolError> {
    if view.members.len() > MAX_SESSION_MEMBERS as usize {
        return Err(ProtocolError::LengthExceeded(view.members.len()));
    }
    validate_advert(&view.name, &view.map, &view.mode, view.max_players)?;
    let mut out = Writer::default();
    out.bytes(&view.room_id.0);
    out.bytes(&view.host.0);
    out.u64(view.revision);
    out.u32(view.epoch);
    out.u8(view.phase.wire());
    out.u8(u8::from(view.joinable));
    out.u8(view.max_players);
    out.u8(view.requires.0);
    out.u8(view.available.0);
    out.u8(u8::from(view.password_protected));
    out.string(&view.name, MAX_ADVERT_NAME_BYTES)?;
    out.string(&view.map, MAX_MAP_BYTES)?;
    out.string(&view.mode, MAX_MODE_BYTES)?;
    out.u8(view.members.len() as u8);
    for member in &view.members {
        out.bytes(&member.0);
        out.string(
            view.member_names
                .get(member)
                .map_or("Player", String::as_str),
            MAX_PLAYER_NAME_BYTES,
        )?;
    }
    Ok(out.0)
}

fn decode_room_view(body: &mut Reader<'_>) -> Result<RoomView, ProtocolError> {
    let room_id = AdvertId(body.array()?);
    let host = MemberId(body.array()?);
    let revision = body.u64()?;
    let epoch = body.u32()?;
    let phase = RoomPhase::from_wire(body.u8()?)?;
    let joinable = bool_byte(body.u8()?)?;
    let max_players = body.u8()?;
    let requires = ContentFlags(body.u8()?);
    let available = ContentFlags(body.u8()?);
    let password_protected = bool_byte(body.u8()?)?;
    let name = body.string(MAX_ADVERT_NAME_BYTES)?;
    let map = body.string(MAX_MAP_BYTES)?;
    let mode = body.string(MAX_MODE_BYTES)?;
    let count = body.u8()? as usize;
    if count > MAX_SESSION_MEMBERS as usize {
        return Err(ProtocolError::LengthExceeded(count));
    }
    let mut members = Vec::with_capacity(count);
    let mut member_names = std::collections::HashMap::new();
    for _ in 0..count {
        let member = MemberId(body.array()?);
        members.push(member);
        member_names.insert(member, body.string(MAX_PLAYER_NAME_BYTES)?);
    }
    Ok(RoomView {
        room_id,
        name,
        host,
        members,
        member_names,
        map,
        mode,
        joinable,
        revision,
        epoch,
        phase,
        max_players,
        requires,
        available,
        password_protected,
    })
}

fn encode_peer_event(event: &PeerEvent) -> Vec<u8> {
    let mut out = Writer::default();
    match event {
        PeerEvent::HostWorldReady {
            epoch,
            map,
            weapons,
            classes,
        } => {
            out.u8(EVENT_HOST_WORLD_READY);
            out.u32(*epoch);
            out.u64(*map);
            out.u64(*weapons);
            out.u64(*classes);
        }
        PeerEvent::MapLoaded {
            member_id,
            epoch,
            map,
            weapons,
            classes,
        } => {
            out.u8(EVENT_MAP_LOADED);
            out.bytes(&member_id.0);
            out.u32(*epoch);
            out.u64(*map);
            out.u64(*weapons);
            out.u64(*classes);
        }
        PeerEvent::BootstrapApplied {
            member_id,
            epoch,
            bootstrap_id,
            snapshot_seq,
            connection,
        } => {
            out.u8(EVENT_BOOTSTRAP_APPLIED);
            out.bytes(&member_id.0);
            out.u32(*epoch);
            out.u32(*bootstrap_id);
            out.u32(*snapshot_seq);
            out.u64(*connection);
        }
        PeerEvent::EnterMatch {
            member_id,
            epoch,
            bootstrap_id,
            connection_id,
            client_id,
        } => {
            out.u8(EVENT_ENTER_MATCH);
            out.bytes(&member_id.0);
            out.u32(*epoch);
            out.u32(*bootstrap_id);
            out.u64(*connection_id);
            out.u32(*client_id);
        }
        PeerEvent::AdmissionFailed {
            member_id,
            epoch,
            reason,
        } => {
            out.u8(EVENT_ADMISSION_FAILED);
            out.bytes(&member_id.0);
            out.u32(*epoch);
            reason.encode(&mut out);
        }
        PeerEvent::VoteToSkip { member_id } => {
            out.u8(EVENT_VOTE_TO_SKIP);
            out.bytes(&member_id.0);
        }
    }
    out.0
}

fn decode_peer_event(body: &mut Reader<'_>) -> Result<PeerEvent, ProtocolError> {
    Ok(match body.u8()? {
        EVENT_HOST_WORLD_READY => PeerEvent::HostWorldReady {
            epoch: body.u32()?,
            map: body.u64()?,
            weapons: body.u64()?,
            classes: body.u64()?,
        },
        EVENT_MAP_LOADED => PeerEvent::MapLoaded {
            member_id: MemberId(body.array()?),
            epoch: body.u32()?,
            map: body.u64()?,
            weapons: body.u64()?,
            classes: body.u64()?,
        },
        EVENT_BOOTSTRAP_APPLIED => PeerEvent::BootstrapApplied {
            member_id: MemberId(body.array()?),
            epoch: body.u32()?,
            bootstrap_id: body.u32()?,
            snapshot_seq: body.u32()?,
            connection: body.u64()?,
        },
        EVENT_ENTER_MATCH => PeerEvent::EnterMatch {
            member_id: MemberId(body.array()?),
            epoch: body.u32()?,
            bootstrap_id: body.u32()?,
            connection_id: body.u64()?,
            client_id: body.u32()?,
        },
        EVENT_ADMISSION_FAILED => PeerEvent::AdmissionFailed {
            member_id: MemberId(body.array()?),
            epoch: body.u32()?,
            reason: AdmissionFailure::decode(body)?,
        },
        EVENT_VOTE_TO_SKIP => PeerEvent::VoteToSkip {
            member_id: MemberId(body.array()?),
        },
        tag => return Err(ProtocolError::UnknownTag(tag)),
    })
}

pub fn encode_relay(datagram: RelayDatagram<'_>) -> Result<Vec<u8>, ProtocolError> {
    encode_relay_bounded(datagram, MAX_OPAQUE_PAYLOAD)
}

pub fn encode_relay_stream(datagram: RelayDatagram<'_>) -> Result<Vec<u8>, ProtocolError> {
    encode_relay_bounded(datagram, MAX_BOOTSTRAP_PAYLOAD)
}

fn encode_relay_bounded(
    datagram: RelayDatagram<'_>,
    max_payload: usize,
) -> Result<Vec<u8>, ProtocolError> {
    let (tag, member, payload) = match datagram {
        RelayDatagram::ClientToHost(payload) => (1, None, payload),
        RelayDatagram::HostToMember { member_id, payload } => (2, Some(member_id), payload),
        RelayDatagram::ServiceToHost { member_id, payload } => (3, Some(member_id), payload),
        RelayDatagram::ServiceToMember(payload) => (4, None, payload),
    };
    if payload.len() > max_payload {
        return Err(ProtocolError::LengthExceeded(payload.len()));
    }
    let mut out = Vec::with_capacity(2 + member.map_or(0, |_| 16) + payload.len());
    out.push(PROTOCOL_VERSION as u8);
    out.push(tag);
    if let Some(member) = member {
        out.extend_from_slice(&member.0);
    }
    out.extend_from_slice(payload);
    Ok(out)
}

pub fn decode_relay(bytes: &[u8]) -> Result<RelayDatagram<'_>, ProtocolError> {
    decode_relay_bounded(bytes, MAX_OPAQUE_PAYLOAD)
}

pub fn decode_relay_stream(bytes: &[u8]) -> Result<RelayDatagram<'_>, ProtocolError> {
    decode_relay_bounded(bytes, MAX_BOOTSTRAP_PAYLOAD)
}

fn decode_relay_bounded(
    bytes: &[u8],
    max_payload: usize,
) -> Result<RelayDatagram<'_>, ProtocolError> {
    if bytes.len() < 2 {
        return Err(ProtocolError::Truncated);
    }
    if bytes[0] != PROTOCOL_VERSION as u8 {
        return Err(ProtocolError::VersionMismatch {
            ours: PROTOCOL_VERSION,
            theirs: bytes[0] as u16,
        });
    }
    let (member, payload) = if matches!(bytes[1], 2 | 3) {
        if bytes.len() < 18 {
            return Err(ProtocolError::Truncated);
        }
        (
            Some(MemberId(bytes[2..18].try_into().expect("length checked"))),
            &bytes[18..],
        )
    } else {
        (None, &bytes[2..])
    };
    if payload.len() > max_payload {
        return Err(ProtocolError::LengthExceeded(payload.len()));
    }
    match (bytes[1], member) {
        (1, None) => Ok(RelayDatagram::ClientToHost(payload)),
        (2, Some(member_id)) => Ok(RelayDatagram::HostToMember { member_id, payload }),
        (3, Some(member_id)) => Ok(RelayDatagram::ServiceToHost { member_id, payload }),
        (4, None) => Ok(RelayDatagram::ServiceToMember(payload)),
        (tag, _) => Err(ProtocolError::UnknownTag(tag)),
    }
}

fn validate_advert(
    name: &str,
    map: &str,
    mode: &str,
    max_players: u8,
) -> Result<(), ProtocolError> {
    if name.is_empty()
        || name.len() > MAX_ADVERT_NAME_BYTES
        || map.is_empty()
        || map.len() > MAX_MAP_BYTES
        || mode.is_empty()
        || mode.len() > MAX_MODE_BYTES
        || !(2..=MAX_SESSION_MEMBERS).contains(&max_players)
    {
        return Err(ProtocolError::InvalidBody);
    }
    Ok(())
}

fn validate_map_mode(map: &str, mode: &str) -> Result<(), ProtocolError> {
    if map.is_empty() || map.len() > MAX_MAP_BYTES || mode.is_empty() || mode.len() > MAX_MODE_BYTES
    {
        return Err(ProtocolError::InvalidBody);
    }
    Ok(())
}

fn bool_byte(value: u8) -> Result<bool, ProtocolError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ProtocolError::InvalidBody),
    }
}

fn encode_message(tag: u8, request_id: u64, body: Vec<u8>) -> Result<Vec<u8>, ProtocolError> {
    if HEADER_BYTES + body.len() > MAX_CONTROL_BYTES || body.len() > u16::MAX as usize {
        return Err(ProtocolError::LengthExceeded(body.len()));
    }
    let mut bytes = Vec::with_capacity(HEADER_BYTES + body.len());
    bytes.extend_from_slice(&MAGIC);
    bytes.extend_from_slice(&PROTOCOL_VERSION.to_be_bytes());
    bytes.push(tag);
    bytes.extend_from_slice(&request_id.to_be_bytes());
    bytes.extend_from_slice(&(body.len() as u16).to_be_bytes());
    bytes.extend_from_slice(&body);
    Ok(bytes)
}

struct Header<'a> {
    tag: u8,
    request_id: u64,
    body: &'a [u8],
}

fn decode_header(bytes: &[u8]) -> Result<Header<'_>, ProtocolError> {
    if bytes.len() > MAX_CONTROL_BYTES {
        return Err(ProtocolError::LengthExceeded(bytes.len()));
    }
    if bytes.len() < HEADER_BYTES {
        return Err(ProtocolError::Truncated);
    }
    if bytes[..4] != MAGIC {
        return Err(ProtocolError::BadMagic);
    }
    let version = u16::from_be_bytes([bytes[4], bytes[5]]);
    if version != PROTOCOL_VERSION {
        return Err(ProtocolError::VersionMismatch {
            ours: PROTOCOL_VERSION,
            theirs: version,
        });
    }
    let declared = u16::from_be_bytes([bytes[15], bytes[16]]) as usize;
    let body = &bytes[HEADER_BYTES..];
    if body.len() != declared {
        return Err(ProtocolError::LengthMismatch {
            declared,
            actual: body.len(),
        });
    }
    Ok(Header {
        tag: bytes[6],
        request_id: u64::from_be_bytes(bytes[7..15].try_into().expect("header length checked")),
        body,
    })
}

#[derive(Default)]
struct Writer(Vec<u8>);

impl Writer {
    fn bytes(&mut self, value: &[u8]) {
        self.0.extend_from_slice(value);
    }

    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }

    fn u16(&mut self, value: u16) {
        self.bytes(&value.to_be_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.bytes(&value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_be_bytes());
    }

    fn string(&mut self, value: &str, max: usize) -> Result<(), ProtocolError> {
        if value.len() > max || value.len() > u8::MAX as usize {
            return Err(ProtocolError::LengthExceeded(value.len()));
        }
        self.u8(value.len() as u8);
        self.bytes(value.as_bytes());
        Ok(())
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], ProtocolError> {
        let end = self
            .cursor
            .checked_add(n)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(ProtocolError::Truncated)?;
        let bytes = &self.bytes[self.cursor..end];
        self.cursor = end;
        Ok(bytes)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], ProtocolError> {
        self.take(N)?
            .try_into()
            .map_err(|_| ProtocolError::Truncated)
    }

    fn u8(&mut self) -> Result<u8, ProtocolError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, ProtocolError> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, ProtocolError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, ProtocolError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    fn string(&mut self, max: usize) -> Result<String, ProtocolError> {
        let len = self.u8()? as usize;
        if len > max {
            return Err(ProtocolError::LengthExceeded(len));
        }
        let value =
            core::str::from_utf8(self.take(len)?).map_err(|_| ProtocolError::InvalidUtf8)?;
        Ok(value.to_owned())
    }

    fn finish(self) -> Result<(), ProtocolError> {
        if self.cursor == self.bytes.len() {
            Ok(())
        } else {
            Err(ProtocolError::InvalidBody)
        }
    }
}

impl SessionCloseReason {
    fn wire(self) -> u8 {
        match self {
            Self::HostLeft => 0,
            Self::LeaseExpired => 1,
            Self::Overload => 2,
        }
    }

    fn from_wire(value: u8) -> Result<Self, ProtocolError> {
        match value {
            0 => Ok(Self::HostLeft),
            1 => Ok(Self::LeaseExpired),
            2 => Ok(Self::Overload),
            _ => Err(ProtocolError::InvalidBody),
        }
    }
}
