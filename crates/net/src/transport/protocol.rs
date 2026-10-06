use bevy::prelude::Resource;
use playerstate_iw4::UserCmd;
use sim::ClientAction;

use crate::PROTOCOL_VERSION;
use crate::authority::inbox::AUTHORITY_HZ;
use crate::client::predict::CmdSeq;
use crate::transport::delta::{decode_usercmd, encode_usercmd};
use crate::transport::frame::FrameSegments;
use crate::transport::meta_wire::{decode_action, encode_action};
use crate::transport::segment_delta;
use crate::transport::wire::{WireError, WireReader, WireWriter};

pub const UDP_IMPLEMENTED: bool = true;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContentFingerprint {
    pub gameplay: u64,

    pub map: u64,

    pub models: u64,

    pub weapons: u64,

    pub classes: u64,
}

impl ContentFingerprint {
    pub fn from_world(world: &sim::SimWorld) -> Self {
        let components = world.content_components();
        Self {
            gameplay: world.content_digest(),
            map: components.map,
            models: components.models,
            weapons: components.weapons,
            classes: components.classes,
        }
    }

    pub fn match_descriptor(self) -> Option<MatchDescriptor> {
        MatchDescriptor::from_fingerprint(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Resource)]
pub struct MatchDescriptor {
    pub map: u64,
    pub weapons: u64,
    pub classes: u64,
}

impl MatchDescriptor {
    pub fn from_components(components: sim::ContentComponents) -> Option<Self> {
        if components.map == 0 {
            None
        } else {
            Some(Self {
                map: components.map,
                weapons: components.weapons,
                classes: components.classes,
            })
        }
    }

    pub fn from_fingerprint(content: ContentFingerprint) -> Option<Self> {
        if content.map == 0 {
            None
        } else {
            Some(Self {
                map: content.map,
                weapons: content.weapons,
                classes: content.classes,
            })
        }
    }

    pub fn from_world(world: &sim::SimWorld) -> Option<Self> {
        Self::from_components(world.content_components())
    }
}

pub const MAX_PACKET_BYTES: u32 = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtocolLimits {
    pub authority_hz: u32,
    pub max_cmds_per_tick: u16,
    pub max_actions_per_tick: u16,

    pub max_packet_bytes: u32,
}

impl ProtocolLimits {
    pub const fn default_listen() -> Self {
        Self {
            authority_hz: AUTHORITY_HZ as u32,

            max_cmds_per_tick: crate::MAX_REDUNDANT_CMDS as u16,
            max_actions_per_tick: 32,
            max_packet_bytes: MAX_PACKET_BYTES,
        }
    }
}

impl Default for ProtocolLimits {
    fn default() -> Self {
        Self::default_listen()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HandshakeHello {
    pub protocol_version: u32,
    pub content: ContentFingerprint,
    pub limits: ProtocolLimits,
}

impl HandshakeHello {
    pub fn current(content: ContentFingerprint) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            content,
            limits: ProtocolLimits::default_listen(),
        }
    }

    pub fn from_world(world: &sim::SimWorld) -> Self {
        Self::current(ContentFingerprint::from_world(world))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandshakeReject {
    ProtocolVersion { ours: u32, theirs: u32 },
    MapContent { ours: u64, theirs: u64 },
    WeaponRegistry { ours: u64, theirs: u64 },
    ClassRegistry { ours: u64, theirs: u64 },
    TickRate { ours: u32, theirs: u32 },
    Limits,
}

impl core::fmt::Display for HandshakeReject {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ProtocolVersion { ours, theirs } => {
                write!(f, "protocol version ours={ours} theirs={theirs}")
            }
            Self::MapContent { ours, theirs } => write!(
                f,
                "map content mismatch (spawns/clip) ours={ours:016x} theirs={theirs:016x}"
            ),
            Self::WeaponRegistry { ours, theirs } => write!(
                f,
                "weapon table mismatch (installed catalogs differ) ours={ours:016x} theirs={theirs:016x}"
            ),
            Self::ClassRegistry { ours, theirs } => write!(
                f,
                "class catalog mismatch ours={ours:016x} theirs={theirs:016x}"
            ),
            Self::TickRate { ours, theirs } => {
                write!(f, "tick rate ours={ours} theirs={theirs}")
            }
            Self::Limits => write!(f, "protocol limits mismatch"),
        }
    }
}

pub fn evaluate_handshake(
    server: &HandshakeHello,
    client: &HandshakeHello,
) -> Result<(), HandshakeReject> {
    if server.protocol_version != client.protocol_version {
        return Err(HandshakeReject::ProtocolVersion {
            ours: server.protocol_version,
            theirs: client.protocol_version,
        });
    }

    if server.content.map == 0
        || client.content.map == 0
        || server.content.map != client.content.map
    {
        return Err(HandshakeReject::MapContent {
            ours: server.content.map,
            theirs: client.content.map,
        });
    }
    if server.content.weapons != client.content.weapons {
        return Err(HandshakeReject::WeaponRegistry {
            ours: server.content.weapons,
            theirs: client.content.weapons,
        });
    }
    if server.content.classes != client.content.classes {
        return Err(HandshakeReject::ClassRegistry {
            ours: server.content.classes,
            theirs: client.content.classes,
        });
    }
    if server.limits.authority_hz != client.limits.authority_hz {
        return Err(HandshakeReject::TickRate {
            ours: server.limits.authority_hz,
            theirs: client.limits.authority_hz,
        });
    }
    if server.limits != client.limits {
        return Err(HandshakeReject::Limits);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ConnectionId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PacketHeader {
    pub connection: ConnectionId,
    pub sequence: u32,

    pub ack: u32,

    pub epoch: u32,
}

impl PacketHeader {
    pub fn applies_to_epoch(self, live: u32) -> bool {
        self.epoch == live
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClientPacket {
    Connect(HandshakeHello),

    Commands {
        header: PacketHeader,
        claimed_client: u32,
        cmds: Vec<(CmdSeq, UserCmd)>,

        samples: Vec<(CmdSeq, sim::ShotSampleProvenance)>,
        actions: Vec<ClientAction>,

        reliable_ack: u16,
    },

    SnapshotAck {
        header: PacketHeader,
        snapshot_seq: u32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServerPacket {
    Control {
        header: PacketHeader,
        payload: crate::ReliablePayload,
    },
    Accept {
        connection: ConnectionId,
        assigned_client: u32,
        hello: HandshakeHello,
    },
    Reject(HandshakeReject),
    Snapshot {
        header: PacketHeader,

        baseline_seq: u32,
        snapshot_seq: u32,

        payload: SnapshotPayload,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotPayload {
    Plain(Vec<u8>),
    AgainstBaseline {
        decoded_len: u32,
        compressed: Vec<u8>,
    },
}

impl SnapshotPayload {
    pub fn against_baseline(delta: &[u8], raw: impl FnOnce() -> Vec<u8>) -> Self {
        match segment_delta::compress(delta) {
            Ok(compressed) if compressed.len() <= MAX_PACKET_BYTES as usize => {
                Self::AgainstBaseline {
                    decoded_len: delta.len() as u32,
                    compressed,
                }
            }
            Ok(_) => Self::Plain(raw()),
            Err(error) => {
                diag::warn!(Net, "snapshot baseline compression failed: {error}");
                Self::Plain(raw())
            }
        }
    }

    pub fn decode(self, baseline: Option<(&[u8], &FrameSegments)>) -> Option<Vec<u8>> {
        match self {
            Self::Plain(payload) => {
                (payload.len() <= segment_delta::MAX_RECONSTRUCTED_FRAME_BYTES).then_some(payload)
            }
            Self::AgainstBaseline {
                decoded_len,
                compressed,
            } => {
                let (baseline, segments) = baseline?;
                let delta = segment_delta::decompress(&compressed, decoded_len as usize).ok()?;
                if delta.len() != decoded_len as usize {
                    return None;
                }
                segment_delta::decode(&delta, baseline, segments)
            }
        }
    }

    pub fn wire_len(&self) -> usize {
        match self {
            Self::Plain(payload) => payload.len(),
            Self::AgainstBaseline { compressed, .. } => compressed.len(),
        }
    }
}

const TAG_CLIENT_CONNECT: u8 = 1;
const TAG_CLIENT_COMMANDS: u8 = 2;
const TAG_CLIENT_SNAP_ACK: u8 = 3;
const TAG_SERVER_ACCEPT: u8 = 10;
const TAG_SERVER_REJECT: u8 = 11;
const TAG_SERVER_SNAPSHOT: u8 = 12;
const TAG_SERVER_COMPRESSED_SNAPSHOT: u8 = 14;
const TAG_SERVER_BASELINE_SNAPSHOT: u8 = 15;

fn encode_shot_sample(out: &mut WireWriter, sample: &sim::ShotSampleProvenance) {
    out.put_u32(sample.left.0);
    out.put_u32(sample.right.0);
    out.put_u32(sample.alpha.to_bits());
    out.put_u8(shot_sample_quality_tag(sample.quality));
}

fn decode_shot_sample(input: &mut WireReader<'_>) -> Result<sim::ShotSampleProvenance, WireError> {
    let left = sim::Tick(input.get_u32()?);
    let right = sim::Tick(input.get_u32()?);
    let alpha = f32::from_bits(input.get_u32()?);
    let quality = shot_sample_quality_from_tag(input.get_u8()?)
        .ok_or(WireError::Malformed("unknown ShotSampleQuality tag"))?;
    Ok(sim::ShotSampleProvenance {
        left,
        right,
        alpha,
        quality,
    })
}

const fn shot_sample_quality_tag(quality: sim::ShotSampleQuality) -> u8 {
    match quality {
        sim::ShotSampleQuality::None => 0,
        sim::ShotSampleQuality::Exact => 1,
        sim::ShotSampleQuality::Interpolated => 2,
        sim::ShotSampleQuality::Held => 3,
        sim::ShotSampleQuality::Starved => 4,
    }
}

const fn shot_sample_quality_from_tag(tag: u8) -> Option<sim::ShotSampleQuality> {
    match tag {
        0 => Some(sim::ShotSampleQuality::None),
        1 => Some(sim::ShotSampleQuality::Exact),
        2 => Some(sim::ShotSampleQuality::Interpolated),
        3 => Some(sim::ShotSampleQuality::Held),
        4 => Some(sim::ShotSampleQuality::Starved),
        _ => None,
    }
}

fn put_u64(out: &mut WireWriter, value: u64) {
    out.put_u32((value >> 32) as u32);
    out.put_u32(value as u32);
}

fn get_u64(input: &mut WireReader<'_>) -> Result<u64, WireError> {
    let hi = input.get_u32()? as u64;
    Ok((hi << 32) | input.get_u32()? as u64)
}

fn put_hello(out: &mut WireWriter, hello: &HandshakeHello) {
    out.put_u32(hello.protocol_version);
    put_u64(out, hello.content.gameplay);
    put_u64(out, hello.content.map);
    put_u64(out, hello.content.models);
    put_u64(out, hello.content.weapons);
    put_u64(out, hello.content.classes);
    out.put_u32(hello.limits.authority_hz);
    out.put_u16(hello.limits.max_cmds_per_tick);
    out.put_u16(hello.limits.max_actions_per_tick);
    out.put_u32(hello.limits.max_packet_bytes);
}

fn get_hello(input: &mut WireReader<'_>) -> Result<HandshakeHello, WireError> {
    let protocol_version = input.get_u32()?;
    let gameplay = get_u64(input)?;
    let map = get_u64(input)?;
    let models = get_u64(input)?;
    let weapons = get_u64(input)?;
    let classes = get_u64(input)?;
    Ok(HandshakeHello {
        protocol_version,
        content: ContentFingerprint {
            gameplay,
            map,
            models,
            weapons,
            classes,
        },
        limits: ProtocolLimits {
            authority_hz: input.get_u32()?,
            max_cmds_per_tick: input.get_u16()?,
            max_actions_per_tick: input.get_u16()?,
            max_packet_bytes: input.get_u32()?,
        },
    })
}

fn put_header(out: &mut WireWriter, header: &PacketHeader) {
    out.put_u32((header.connection.0 >> 32) as u32);
    out.put_u32(header.connection.0 as u32);
    out.put_u32(header.sequence);
    out.put_u32(header.ack);
    out.put_u32(header.epoch);
}

fn get_header(input: &mut WireReader<'_>) -> Result<PacketHeader, WireError> {
    let hi = input.get_u32()? as u64;
    let lo = input.get_u32()? as u64;
    Ok(PacketHeader {
        connection: ConnectionId((hi << 32) | lo),
        sequence: input.get_u32()?,
        ack: input.get_u32()?,
        epoch: input.get_u32()?,
    })
}

fn put_reject(out: &mut WireWriter, reject: HandshakeReject) {
    match reject {
        HandshakeReject::ProtocolVersion { ours, theirs } => {
            out.put_u8(1);
            out.put_u32(ours);
            out.put_u32(theirs);
        }
        HandshakeReject::MapContent { ours, theirs } => {
            out.put_u8(2);
            put_u64(out, ours);
            put_u64(out, theirs);
        }
        HandshakeReject::WeaponRegistry { ours, theirs } => {
            out.put_u8(3);
            put_u64(out, ours);
            put_u64(out, theirs);
        }
        HandshakeReject::ClassRegistry { ours, theirs } => {
            out.put_u8(4);
            put_u64(out, ours);
            put_u64(out, theirs);
        }
        HandshakeReject::TickRate { ours, theirs } => {
            out.put_u8(5);
            out.put_u32(ours);
            out.put_u32(theirs);
        }
        HandshakeReject::Limits => out.put_u8(6),
    }
}

fn get_reject(input: &mut WireReader<'_>) -> Result<HandshakeReject, WireError> {
    match input.get_u8()? {
        1 => Ok(HandshakeReject::ProtocolVersion {
            ours: input.get_u32()?,
            theirs: input.get_u32()?,
        }),
        2 => Ok(HandshakeReject::MapContent {
            ours: get_u64(input)?,
            theirs: get_u64(input)?,
        }),
        3 => Ok(HandshakeReject::WeaponRegistry {
            ours: get_u64(input)?,
            theirs: get_u64(input)?,
        }),
        4 => Ok(HandshakeReject::ClassRegistry {
            ours: get_u64(input)?,
            theirs: get_u64(input)?,
        }),
        5 => Ok(HandshakeReject::TickRate {
            ours: input.get_u32()?,
            theirs: input.get_u32()?,
        }),
        6 => Ok(HandshakeReject::Limits),
        _ => Err(WireError::Malformed("unknown HandshakeReject tag")),
    }
}

impl ClientPacket {
    pub fn encode(&self, out: &mut WireWriter) {
        match self {
            Self::Connect(hello) => {
                out.put_u8(TAG_CLIENT_CONNECT);
                put_hello(out, hello);
            }
            Self::Commands {
                header,
                claimed_client,
                cmds,
                samples,
                actions,
                reliable_ack,
            } => {
                out.put_u8(TAG_CLIENT_COMMANDS);
                put_header(out, header);
                out.put_u32(*claimed_client);
                out.put_u16(*reliable_ack);
                out.put_u16(cmds.len() as u16);
                for (seq, cmd) in cmds {
                    out.put_u32(seq.0);
                    encode_usercmd(out, cmd);
                }
                out.put_u16(samples.len() as u16);
                for (seq, sample) in samples {
                    out.put_u32(seq.0);
                    encode_shot_sample(out, sample);
                }
                out.put_u16(actions.len() as u16);
                for action in actions {
                    encode_action(out, action);
                }
            }
            Self::SnapshotAck {
                header,
                snapshot_seq,
            } => {
                out.put_u8(TAG_CLIENT_SNAP_ACK);
                put_header(out, header);
                out.put_u32(*snapshot_seq);
            }
        }
    }

    pub fn decode(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        match input.get_u8()? {
            TAG_CLIENT_CONNECT => Ok(Self::Connect(get_hello(input)?)),
            TAG_CLIENT_COMMANDS => {
                let header = get_header(input)?;
                let claimed_client = input.get_u32()?;
                let reliable_ack = input.get_u16()?;
                let cmd_count = input.get_u16()? as usize;
                let mut cmds = Vec::with_capacity(cmd_count.min(64));
                for _ in 0..cmd_count {
                    let seq = CmdSeq(input.get_u32()?);
                    cmds.push((seq, decode_usercmd(input)?));
                }
                let sample_count = input.get_u16()? as usize;
                let mut samples = Vec::with_capacity(sample_count.min(64));
                for _ in 0..sample_count {
                    let seq = CmdSeq(input.get_u32()?);
                    samples.push((seq, decode_shot_sample(input)?));
                }
                let action_count = input.get_u16()? as usize;
                let mut actions = Vec::with_capacity(action_count.min(64));
                for _ in 0..action_count {
                    actions.push(decode_action(input)?);
                }
                Ok(Self::Commands {
                    header,
                    claimed_client,
                    cmds,
                    samples,
                    actions,
                    reliable_ack,
                })
            }
            TAG_CLIENT_SNAP_ACK => Ok(Self::SnapshotAck {
                header: get_header(input)?,
                snapshot_seq: input.get_u32()?,
            }),
            _ => Err(WireError::Malformed("unknown ClientPacket tag")),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = WireWriter::new();
        self.encode(&mut out);
        out.finish()
    }
}

impl ServerPacket {
    pub fn encode(&self, out: &mut WireWriter) {
        match self {
            Self::Control { header, payload } => {
                out.put_u8(13);
                put_header(out, header);
                crate::transport::reliable::encode_reliable_payload(
                    out,
                    payload.ack_through,
                    &payload.rows,
                    payload.dropped_oldest,
                );
            }
            Self::Accept {
                connection,
                assigned_client,
                hello,
            } => {
                out.put_u8(TAG_SERVER_ACCEPT);
                out.put_u32((connection.0 >> 32) as u32);
                out.put_u32(connection.0 as u32);
                out.put_u32(*assigned_client);
                put_hello(out, hello);
            }
            Self::Reject(reject) => {
                out.put_u8(TAG_SERVER_REJECT);
                put_reject(out, *reject);
            }
            Self::Snapshot {
                header,
                baseline_seq,
                snapshot_seq,
                payload,
            } => {
                let payload = match payload {
                    SnapshotPayload::Plain(payload) => payload,
                    SnapshotPayload::AgainstBaseline {
                        decoded_len,
                        compressed,
                    } => {
                        out.put_u8(TAG_SERVER_BASELINE_SNAPSHOT);
                        put_header(out, header);
                        out.put_u32(*baseline_seq);
                        out.put_u32(*snapshot_seq);
                        out.put_u32(*decoded_len);
                        out.put_u32(compressed.len() as u32);
                        out.put_bytes(compressed);
                        return;
                    }
                };
                let compressed = match zstd::bulk::compress(payload, 1) {
                    Ok(bytes) if bytes.len() + 4 < payload.len() => Some(bytes),
                    Ok(_) => None,
                    Err(error) => {
                        diag::warn!(Net, "snapshot compression failed: {error}");
                        None
                    }
                };
                out.put_u8(if compressed.is_some() {
                    TAG_SERVER_COMPRESSED_SNAPSHOT
                } else {
                    TAG_SERVER_SNAPSHOT
                });
                put_header(out, header);
                out.put_u32(*baseline_seq);
                out.put_u32(*snapshot_seq);
                if let Some(compressed) = &compressed {
                    out.put_u32(payload.len() as u32);
                    out.put_u32(compressed.len() as u32);
                    out.put_bytes(compressed);
                    return;
                }
                out.put_u32(payload.len() as u32);
                out.put_bytes(payload);
            }
        }
    }

    pub fn decode(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        match input.get_u8()? {
            13 => Ok(Self::Control {
                header: get_header(input)?,
                payload: crate::transport::reliable::decode_reliable_payload(input)?,
            }),
            TAG_SERVER_ACCEPT => {
                let hi = input.get_u32()? as u64;
                let lo = input.get_u32()? as u64;
                Ok(Self::Accept {
                    connection: ConnectionId((hi << 32) | lo),
                    assigned_client: input.get_u32()?,
                    hello: get_hello(input)?,
                })
            }
            TAG_SERVER_REJECT => Ok(Self::Reject(get_reject(input)?)),
            TAG_SERVER_BASELINE_SNAPSHOT => {
                let header = get_header(input)?;
                let baseline_seq = input.get_u32()?;
                let snapshot_seq = input.get_u32()?;
                let decoded_len = input.get_u32()?;
                if decoded_len > MAX_PACKET_BYTES {
                    return Err(WireError::Malformed("snapshot exceeds decoded size limit"));
                }
                let len = input.get_u32()? as usize;
                if len > MAX_PACKET_BYTES as usize || len > input.remaining() {
                    return Err(WireError::Malformed("invalid snapshot payload length"));
                }
                let mut compressed = vec![0u8; len];
                input.get_bytes(&mut compressed)?;
                Ok(Self::Snapshot {
                    header,
                    baseline_seq,
                    snapshot_seq,
                    payload: SnapshotPayload::AgainstBaseline {
                        decoded_len,
                        compressed,
                    },
                })
            }
            tag @ (TAG_SERVER_SNAPSHOT | TAG_SERVER_COMPRESSED_SNAPSHOT) => {
                let header = get_header(input)?;
                let baseline_seq = input.get_u32()?;
                let snapshot_seq = input.get_u32()?;
                let decoded_len = if tag == TAG_SERVER_COMPRESSED_SNAPSHOT {
                    let len = input.get_u32()? as usize;
                    if len > MAX_PACKET_BYTES as usize {
                        return Err(WireError::Malformed("snapshot exceeds decoded size limit"));
                    }
                    Some(len)
                } else {
                    None
                };
                let payload_len = input.get_u32()? as usize;
                if payload_len > MAX_PACKET_BYTES as usize || payload_len > input.remaining() {
                    return Err(WireError::Malformed("invalid snapshot payload length"));
                }
                let mut payload = vec![0u8; payload_len];
                input.get_bytes(&mut payload)?;
                if let Some(decoded_len) = decoded_len {
                    payload = segment_delta::decompress(&payload, decoded_len)
                        .map_err(|_| WireError::Malformed("invalid compressed snapshot"))?;
                    if payload.len() != decoded_len {
                        return Err(WireError::Malformed("snapshot decoded length mismatch"));
                    }
                }
                Ok(Self::Snapshot {
                    header,
                    baseline_seq,
                    snapshot_seq,
                    payload: SnapshotPayload::Plain(payload),
                })
            }
            _ => Err(WireError::Malformed("unknown ServerPacket tag")),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = WireWriter::new();
        self.encode(&mut out);
        out.finish()
    }
}

pub fn decode_client_packet(
    bytes: &[u8],
    limits: &ProtocolLimits,
) -> Result<ClientPacket, WireError> {
    if bytes.len() as u32 > limits.max_packet_bytes {
        return Err(WireError::Malformed("datagram exceeds max_packet_bytes"));
    }
    let mut input = WireReader::new(bytes);
    let packet = ClientPacket::decode(&mut input)?;
    if !input.is_empty() {
        return Err(WireError::Malformed("trailing bytes after ClientPacket"));
    }
    if let ClientPacket::Commands { cmds, actions, .. } = &packet {
        if cmds.len() as u16 > limits.max_cmds_per_tick {
            return Err(WireError::Malformed("cmd_count exceeds limit"));
        }
        if actions.len() as u16 > limits.max_actions_per_tick {
            return Err(WireError::Malformed("action_count exceeds limit"));
        }
    }
    Ok(packet)
}

pub fn decode_server_packet(
    bytes: &[u8],
    limits: &ProtocolLimits,
) -> Result<ServerPacket, WireError> {
    if bytes.len() as u32 > limits.max_packet_bytes {
        return Err(WireError::Malformed("datagram exceeds max_packet_bytes"));
    }
    let mut input = WireReader::new(bytes);
    let packet = ServerPacket::decode(&mut input)?;
    if !input.is_empty() {
        return Err(WireError::Malformed("trailing bytes after ServerPacket"));
    }
    Ok(packet)
}

#[derive(Debug, Default)]
pub struct ConnectionTable {
    next_conn: u64,
    next_client: u32,

    assigned: std::collections::HashMap<ConnectionId, u32>,
}

impl ConnectionTable {
    pub fn starting_at(first_client: u32) -> Self {
        Self {
            next_conn: 0,
            next_client: first_client,
            assigned: std::collections::HashMap::new(),
        }
    }

    pub fn accept_new(&mut self, occupied: &[u32]) -> (ConnectionId, u32) {
        while occupied.contains(&self.next_client) {
            self.next_client = self.next_client.wrapping_add(1);
        }
        self.next_conn = self.next_conn.wrapping_add(1);
        let conn = ConnectionId(self.next_conn);
        let client = self.next_client;
        self.next_client = self.next_client.wrapping_add(1);
        self.assigned.insert(conn, client);
        (conn, client)
    }

    pub fn resolve(
        &self,
        connection: ConnectionId,
        _claimed_client: u32,
    ) -> Result<u32, IdentityError> {
        self.assigned
            .get(&connection)
            .copied()
            .ok_or(IdentityError::UnknownConnection)
    }

    pub fn retire(&mut self, connection: ConnectionId) -> Option<u32> {
        self.assigned.remove(&connection)
    }

    pub fn client_of(&self, connection: ConnectionId) -> Option<u32> {
        self.assigned.get(&connection).copied()
    }

    pub fn clients(&self) -> impl Iterator<Item = u32> + '_ {
        self.assigned.values().copied()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityError {
    UnknownConnection,
}

impl core::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownConnection => write!(f, "connection id is not registered"),
        }
    }
}
