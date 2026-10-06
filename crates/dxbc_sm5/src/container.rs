use alloc::vec::Vec;
use core::fmt;

const MAGIC: &[u8; 4] = b"DXBC";
const HEADER_LEN: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerError {
    BadMagic,
    Truncated { at: usize, needed: usize },
    ChunkOutOfRange { index: usize, offset: usize },
}

impl fmt::Display for ContainerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => f.write_str("not a DXBC container"),
            Self::Truncated { at, needed } => {
                write!(f, "container needs {needed} bytes at {at}")
            }
            Self::ChunkOutOfRange { index, offset } => {
                write!(f, "chunk {index} at {offset} lies outside the container")
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Chunk<'a> {
    pub fourcc: [u8; 4],
    pub data: &'a [u8],
}

#[derive(Clone, Debug)]
pub struct Container<'a> {
    pub chunks: Vec<Chunk<'a>>,
}

pub(crate) fn read_u32(bytes: &[u8], at: usize) -> Result<u32, ContainerError> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(ContainerError::Truncated { at, needed: 4 })
}

impl<'a> Container<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ContainerError> {
        if bytes.get(..4) != Some(MAGIC.as_slice()) {
            return Err(ContainerError::BadMagic);
        }
        if bytes.len() < HEADER_LEN {
            return Err(ContainerError::Truncated {
                at: 0,
                needed: HEADER_LEN,
            });
        }
        let total = (read_u32(bytes, 24)? as usize).min(bytes.len());
        let count = read_u32(bytes, 28)? as usize;
        let bytes = &bytes[..total];
        let mut chunks = Vec::with_capacity(count);
        for index in 0..count {
            let offset = read_u32(bytes, HEADER_LEN + 4 * index)? as usize;
            let head = bytes
                .get(offset..offset + 8)
                .ok_or(ContainerError::ChunkOutOfRange { index, offset })?;
            let size = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
            let data = bytes
                .get(offset + 8..offset + 8 + size)
                .ok_or(ContainerError::ChunkOutOfRange { index, offset })?;
            chunks.push(Chunk {
                fourcc: [head[0], head[1], head[2], head[3]],
                data,
            });
        }
        Ok(Self { chunks })
    }

    pub fn chunk(&self, fourcc: &[u8; 4]) -> Option<&'a [u8]> {
        self.chunks
            .iter()
            .find(|chunk| &chunk.fourcc == fourcc)
            .map(|chunk| chunk.data)
    }

    pub fn program(&self) -> Option<&'a [u8]> {
        self.chunk(b"SHEX").or_else(|| self.chunk(b"SHDR"))
    }
}
