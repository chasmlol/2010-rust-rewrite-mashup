use std::sync::Arc;

use sim::{ClientId, Snapshot, SnapshotMeta, Tick};

use crate::client::predict::CmdSeq;
use crate::transport::delta::SnapshotDelta;
use crate::transport::meta_wire::{
    META_SEGMENTS, SnapshotMetaSectionBytes, WorldObjectSyncDecoder, decode_snapshot_meta,
    encode_snapshot_meta_body, encode_world_objects_wire,
};
use crate::transport::netfields::compute_state_hash;
use crate::transport::reliable::{
    ReliablePayload, decode_reliable_payload, encode_reliable_payload,
};
use crate::transport::wire::{WireError, WireReader, WireWriter};

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub tick: Tick,

    pub state_hash: u32,
    pub acks: Vec<(ClientId, CmdSeq)>,
    pub snapshot_delta: SnapshotDelta,

    pub snapshot_meta: SnapshotMeta,

    pub world_objects_wire: Vec<u8>,

    pub reliable: ReliablePayload,

    pub svc_sounds: Vec<crate::SvcSound>,

    pub svc_scores: Option<String>,

    pub svc_card_slots: Vec<crate::SvcCardSlot>,

    pub svc_open_menus: Vec<crate::SvcOpenMenu>,
    pub svc_hud_splashes: Vec<crate::SvcHudSplash>,

    pub svc_game_notifies: Vec<crate::SvcGameNotify>,
}

pub const FRAME_SEGMENTS: usize = META_SEGMENTS + 2;

pub type FrameSegments = [u32; FRAME_SEGMENTS];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameTail {
    pub reliable: ReliablePayload,
    pub svc_sounds: Vec<crate::SvcSound>,
    pub svc_scores: Option<String>,
    pub svc_card_slots: Vec<crate::SvcCardSlot>,
    pub svc_open_menus: Vec<crate::SvcOpenMenu>,
    pub svc_hud_splashes: Vec<crate::SvcHudSplash>,
    pub svc_game_notifies: Vec<crate::SvcGameNotify>,
}

#[derive(Debug)]
pub struct WireMeta {
    bytes: Vec<u8>,
    ends: [usize; META_BODY_SEGMENTS],
}

const META_BODY_SEGMENTS: usize = META_SEGMENTS - 1;

impl WireMeta {
    pub fn without_reliable_events(meta: &SnapshotMeta) -> Self {
        let mut out = WireWriter::new();
        let sizes = encode_snapshot_meta_body(&mut out, meta, |record| {
            !sim::sim_event_is_reliable(&record.event)
        });
        let mut ends = [0usize; META_BODY_SEGMENTS];
        let mut at = 0usize;
        for (end, len) in ends.iter_mut().zip(sizes.segments()) {
            at += len;
            *end = at;
        }
        Self {
            bytes: out.finish(),
            ends,
        }
    }

    pub fn segment(&self, index: usize) -> &[u8] {
        let start = index.checked_sub(1).map_or(0, |prev| self.ends[prev]);
        &self.bytes[start..self.ends[index]]
    }
}

#[derive(Debug)]
pub struct FrameParts {
    pub head: Vec<u8>,
    pub meta: Arc<WireMeta>,
    pub world_objects: Vec<u8>,
    pub tail: Vec<u8>,
}

impl FrameParts {
    pub fn new(
        tick: Tick,
        state_hash: u32,
        acks: &[(ClientId, CmdSeq)],
        snapshot_delta: &SnapshotDelta,
        meta: Arc<WireMeta>,
        world_objects_wire: &[u8],
        tail: &FrameTail,
    ) -> Self {
        let mut head = WireWriter::new();
        encode_head(&mut head, tick, state_hash, acks);
        snapshot_delta.encode(&mut head);
        let mut world_objects = WireWriter::new();
        encode_world_objects_wire(&mut world_objects, world_objects_wire);
        let mut tail_out = WireWriter::new();
        encode_tail(&mut tail_out, tail);
        Self {
            head: head.finish(),
            meta,
            world_objects: world_objects.finish(),
            tail: tail_out.finish(),
        }
    }

    pub fn segments(&self) -> [&[u8]; FRAME_SEGMENTS] {
        let mut out = [&self.head[..]; FRAME_SEGMENTS];
        for (index, slot) in out[1..META_SEGMENTS].iter_mut().enumerate() {
            *slot = self.meta.segment(index);
        }
        out[META_SEGMENTS] = &self.world_objects;
        out[FRAME_SEGMENTS - 1] = &self.tail;
        out
    }

    pub fn to_raw(&self) -> Vec<u8> {
        let segments = self.segments();
        let mut out = Vec::with_capacity(segments.iter().map(|s| s.len()).sum());
        for segment in segments {
            out.extend_from_slice(segment);
        }
        out
    }
}

fn encode_head(
    out: &mut WireWriter,
    tick: Tick,
    state_hash: u32,
    acks: &[(ClientId, CmdSeq)],
) -> usize {
    let mark = out.len();
    out.put_u32(tick.0);
    out.put_u32(state_hash);
    debug_assert!(acks.len() <= u16::MAX as usize);
    out.put_u16(acks.len() as u16);
    for (client, seq) in acks {
        out.put_u32(client.0);
        out.put_u32(seq.0);
    }
    out.len() - mark
}

fn encode_tail(out: &mut WireWriter, tail: &FrameTail) -> usize {
    let mark = out.len();
    encode_reliable_payload(
        out,
        tail.reliable.ack_through,
        &tail.reliable.rows,
        tail.reliable.dropped_oldest,
    );
    crate::svc_sound::encode_svc_sounds(out, &tail.svc_sounds);
    crate::svc_scores::encode_svc_scores(out, tail.svc_scores.as_deref());
    crate::svc_playercard::encode_svc_card_slots(out, &tail.svc_card_slots);
    crate::svc_playercard::encode_svc_open_menus(out, &tail.svc_open_menus);
    crate::svc_playercard::encode_svc_hud_splashes(out, &tail.svc_hud_splashes);
    crate::svc_gamenotify::encode_svc_game_notifies(out, &tail.svc_game_notifies);
    out.len() - mark
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameSectionBytes {
    pub header: usize,
    pub snapshot_delta: usize,
    pub meta: SnapshotMetaSectionBytes,
    pub reliable: usize,
    pub svc: usize,
    pub total: usize,
}

impl FrameSectionBytes {
    pub fn named_sum(self) -> usize {
        self.header + self.snapshot_delta + self.meta.total() + self.reliable + self.svc
    }
}

impl Frame {
    pub fn encode(&self, out: &mut WireWriter) {
        let _ = self.encode_sections(out);
    }

    pub fn section_bytes(&self) -> FrameSectionBytes {
        let mut out = WireWriter::new();
        self.encode_sections(&mut out)
    }

    fn encode_sections(&self, out: &mut WireWriter) -> FrameSectionBytes {
        let start = out.len();
        let header = encode_head(out, self.tick, self.state_hash, &self.acks);
        let mut mark = out.len();
        self.snapshot_delta.encode(out);
        let snapshot_delta = out.len() - mark;
        let mut meta = encode_snapshot_meta_body(out, &self.snapshot_meta, |_| true);
        meta.world_objects = encode_world_objects_wire(out, &self.world_objects_wire);
        mark = out.len();
        encode_reliable_payload(
            out,
            self.reliable.ack_through,
            &self.reliable.rows,
            self.reliable.dropped_oldest,
        );
        let reliable = out.len() - mark;
        mark = out.len();
        crate::svc_sound::encode_svc_sounds(out, &self.svc_sounds);
        crate::svc_scores::encode_svc_scores(out, self.svc_scores.as_deref());
        crate::svc_playercard::encode_svc_card_slots(out, &self.svc_card_slots);
        crate::svc_playercard::encode_svc_open_menus(out, &self.svc_open_menus);
        crate::svc_playercard::encode_svc_hud_splashes(out, &self.svc_hud_splashes);
        crate::svc_gamenotify::encode_svc_game_notifies(out, &self.svc_game_notifies);
        let svc = out.len() - mark;
        FrameSectionBytes {
            header,
            snapshot_delta,
            meta,
            reliable,
            svc,
            total: out.len() - start,
        }
    }

    pub fn decode(
        input: &mut WireReader<'_>,
        world_decoder: &mut WorldObjectSyncDecoder,
    ) -> Result<Self, WireError> {
        Self::decode_segments(input, world_decoder).map(|(frame, _)| frame)
    }

    pub fn decode_segments(
        input: &mut WireReader<'_>,
        world_decoder: &mut WorldObjectSyncDecoder,
    ) -> Result<(Self, FrameSegments), WireError> {
        let start = input.remaining();
        let tick = Tick(input.get_u32()?);
        let state_hash = input.get_u32()?;
        let ack_count = input.get_u16()? as usize;
        let mut acks = Vec::with_capacity(ack_count.min(64));
        for _ in 0..ack_count {
            let client = ClientId(input.get_u32()?);
            acks.push((client, CmdSeq(input.get_u32()?)));
        }
        let snapshot_delta = SnapshotDelta::decode(input)?;
        let head_end = input.remaining();
        let mut meta_ends = [0usize; META_SEGMENTS];
        let (snapshot_meta, world_objects_wire) =
            decode_snapshot_meta(input, world_decoder, &mut meta_ends)?;
        let reliable = decode_reliable_payload(input)?;
        let svc_sounds = crate::svc_sound::decode_svc_sounds(input)?;
        let svc_scores = crate::svc_scores::decode_svc_scores(input)?;
        let svc_card_slots = crate::svc_playercard::decode_svc_card_slots(input)?;
        let svc_open_menus = crate::svc_playercard::decode_svc_open_menus(input)?;
        let svc_hud_splashes = crate::svc_playercard::decode_svc_hud_splashes(input)?;
        let svc_game_notifies = crate::svc_gamenotify::decode_svc_game_notifies(input)?;
        let mut segments = [0u32; FRAME_SEGMENTS];
        segments[0] = (start - head_end) as u32;
        let mut prev = head_end;
        for (slot, end) in segments[1..=META_SEGMENTS].iter_mut().zip(meta_ends) {
            *slot = (prev - end) as u32;
            prev = end;
        }
        segments[FRAME_SEGMENTS - 1] = (prev - input.remaining()) as u32;
        let frame = Self {
            tick,
            state_hash,
            acks,
            snapshot_delta,
            snapshot_meta,
            world_objects_wire,
            reliable,
            svc_sounds,
            svc_scores,
            svc_card_slots,
            svc_open_menus,
            svc_hud_splashes,
            svc_game_notifies,
        };
        Ok((frame, segments))
    }

    pub fn ack_for(&self, client: ClientId) -> Option<CmdSeq> {
        self.acks
            .iter()
            .find(|(id, _)| *id == client)
            .map(|(_, seq)| *seq)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = WireWriter::new();
        self.encode(&mut out);
        out.finish()
    }
}

pub trait Transport {
    fn send(&mut self, frame: &Frame) -> Result<(), TransportError>;

    fn recv(&mut self) -> Result<Option<Frame>, TransportError>;
}

#[derive(Debug)]
pub enum TransportError {
    Ended,

    Wire(WireError),

    Io(std::io::Error),
}

impl core::fmt::Display for TransportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TransportError::Ended => write!(f, "frame stream ended"),
            TransportError::Wire(e) => write!(f, "{e}"),
            TransportError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<WireError> for TransportError {
    fn from(value: WireError) -> Self {
        TransportError::Wire(value)
    }
}

impl From<std::io::Error> for TransportError {
    fn from(value: std::io::Error) -> Self {
        TransportError::Io(value)
    }
}

#[derive(Debug, Default)]
pub struct LoopbackTransport {
    queue: std::collections::VecDeque<Vec<u8>>,
    world_decoder: WorldObjectSyncDecoder,
}

impl LoopbackTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pending(&self) -> usize {
        self.queue.len()
    }
}

impl Transport for LoopbackTransport {
    fn send(&mut self, frame: &Frame) -> Result<(), TransportError> {
        self.queue.push_back(frame.to_bytes());
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<Frame>, TransportError> {
        let Some(bytes) = self.queue.pop_front() else {
            return Ok(None);
        };
        let mut reader = WireReader::new(&bytes);
        Ok(Some(Frame::decode(&mut reader, &mut self.world_decoder)?))
    }
}

pub fn frame_from_tick(coder: &mut crate::SnapshotEncoder, snapshot: &Snapshot) -> Frame {
    frame_from_acked_tick(coder, snapshot, Vec::new())
}

pub fn frame_from_acked_tick(
    coder: &mut crate::SnapshotEncoder,
    snapshot: &Snapshot,
    acks: Vec<(ClientId, CmdSeq)>,
) -> Frame {
    frame_from_acked_tick_with_reliable(
        coder,
        snapshot,
        acks,
        ReliablePayload {
            ack_through: 0,
            rows: Vec::new(),
            dropped_oldest: 0,
        },
    )
}

pub fn frame_from_acked_tick_with_reliable(
    coder: &mut crate::SnapshotEncoder,
    snapshot: &Snapshot,
    acks: Vec<(ClientId, CmdSeq)>,
    reliable: ReliablePayload,
) -> Frame {
    let world_objects_wire = coder
        .encode_world_objects(snapshot.tick, &snapshot.meta.world_objects)
        .to_vec();
    Frame {
        tick: snapshot.tick,
        state_hash: compute_state_hash(&snapshot.players),
        acks,
        snapshot_delta: coder.encode(snapshot),
        snapshot_meta: snapshot.meta.clone(),
        world_objects_wire,
        reliable,
        svc_sounds: Vec::new(),
        svc_scores: None,
        svc_card_slots: Vec::new(),
        svc_open_menus: Vec::new(),
        svc_hud_splashes: Vec::new(),
        svc_game_notifies: Vec::new(),
    }
}

pub fn authoritative_snapshot_hash(snapshot: &Snapshot) -> u64 {
    let mut encoder = crate::SnapshotEncoder::new();
    let frame = frame_from_tick(&mut encoder, snapshot);
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in frame.to_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
