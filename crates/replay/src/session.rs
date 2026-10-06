use std::path::{Path, PathBuf};

use net::{Frame, SnapshotDecoder, SnapshotEncoder, Transport, frame_from_tick};
use sim::Snapshot;

use crate::file::{FileTransport, MatchRecordIdentity, ReplayError, demo_path, sanitize_demo_name};

pub fn auto_demo_name(index: u32) -> String {
    format!("demo{index:04}")
}

pub fn next_free_demo_name(artifacts_root: &Path) -> String {
    for index in 0..10_000u32 {
        let name = auto_demo_name(index);
        if !demo_path(artifacts_root, &name).exists() {
            return name;
        }
    }

    format!(
        "demo{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default()
    )
}

#[derive(Debug)]
pub struct Recording {
    transport: FileTransport,
    encoder: SnapshotEncoder,
    name: String,
    path: PathBuf,
    ticks: u64,
}

impl Recording {
    pub fn start(artifacts_root: &Path, requested: &str) -> Result<Self, ReplayError> {
        Self::start_with_identity(artifacts_root, requested, MatchRecordIdentity::default())
    }

    pub fn start_with_identity(
        artifacts_root: &Path,
        requested: &str,
        identity: MatchRecordIdentity,
    ) -> Result<Self, ReplayError> {
        let name =
            sanitize_demo_name(requested).unwrap_or_else(|| next_free_demo_name(artifacts_root));
        let path = demo_path(artifacts_root, &name);
        let transport = FileTransport::recording_with_identity(&path, identity)?;
        Ok(Self {
            transport,
            encoder: SnapshotEncoder::new(),
            name,
            path,
            ticks: 0,
        })
    }

    pub fn start_at_path(
        path: PathBuf,
        identity: MatchRecordIdentity,
    ) -> Result<Self, ReplayError> {
        let name = path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("clip")
            .to_owned();
        let transport = FileTransport::recording_with_identity(&path, identity)?;
        Ok(Self {
            transport,
            encoder: SnapshotEncoder::new(),
            name,
            path,
            ticks: 0,
        })
    }

    pub fn record(&mut self, snapshot: &Snapshot) -> Result<(), ReplayError> {
        let frame = frame_from_tick(&mut self.encoder, snapshot);
        self.transport
            .send(&frame)
            .map_err(|e| ReplayError::Io(std::io::Error::other(e.to_string())))?;
        self.ticks += 1;
        Ok(())
    }

    pub fn record_clip_ring(&mut self, ring: &crate::clip::ClipRing) -> Result<u64, ReplayError> {
        for snapshot in ring.iter() {
            self.record(snapshot)?;
        }
        Ok(ring.len() as u64)
    }

    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn stop(mut self) -> Result<(u64, PathBuf), ReplayError> {
        self.transport.finish()?;
        Ok((self.ticks, self.path))
    }
}

#[derive(Debug)]
pub struct Playback {
    transport: FileTransport,
    decoder: SnapshotDecoder,
    path: PathBuf,
    ticks: u64,
    identity: MatchRecordIdentity,
}

impl Playback {
    pub fn open(artifacts_root: &Path, requested: &str) -> Result<Self, ReplayError> {
        Self::open_path(&crate::clip::resolve_playback_path(
            artifacts_root,
            requested,
        ))
    }

    pub fn open_path(path: &Path) -> Result<Self, ReplayError> {
        let transport = FileTransport::playback(path)?;
        let identity = transport
            .identity()
            .expect("playback FileTransport always has a reader");
        Ok(Self {
            transport,
            decoder: SnapshotDecoder::new(),
            path: path.to_path_buf(),
            ticks: 0,
            identity,
        })
    }

    pub fn next_tick(&mut self) -> Result<Option<PlayedTick>, ReplayError> {
        let frame = match self.transport.recv() {
            Ok(Some(frame)) => frame,
            Ok(None) | Err(net::TransportError::Ended) => return Ok(None),
            Err(net::TransportError::Wire(e)) => return Err(ReplayError::Wire(e)),
            Err(net::TransportError::Io(e)) => return Err(ReplayError::Io(e)),
        };
        let mut snapshot = self.decoder.decode(&frame.snapshot_delta)?;
        snapshot.meta = frame.snapshot_meta.clone();
        self.ticks += 1;
        Ok(Some(PlayedTick { snapshot, frame }))
    }

    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn identity(&self) -> MatchRecordIdentity {
        self.identity
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayedTick {
    pub snapshot: Snapshot,

    pub frame: Frame,
}
