pub const MAGIC_SIGNED: &[u8; 8] = b"TAff0100";

pub const MAGIC_ENCRYPTED: &[u8; 8] = b"PHEEBs71";

pub const ZONE_VERSION_PC: u32 = 0x93;

pub const ZONE_NAME_LEN: usize = 32;

pub const CHUNKS_OFFSET: usize = 0x138;

pub const MAX_XFILE_COUNT: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileHeader {
    pub version: u32,
    name: [u8; ZONE_NAME_LEN],
    name_len: usize,
}

impl FileHeader {
    pub fn name(&self) -> &[u8] {
        &self.name[..self.name_len]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileHeaderError {
    TooShort { len: usize },
    BadMagic { got: [u8; 8] },
    BadVersion { got: u32 },
    NotEncrypted { got: [u8; 8] },
    EmptyName,
}

pub fn parse_file_header(bytes: &[u8]) -> Result<FileHeader, FileHeaderError> {
    if bytes.len() < CHUNKS_OFFSET {
        return Err(FileHeaderError::TooShort { len: bytes.len() });
    }
    let magic: [u8; 8] = bytes[0..8].try_into().unwrap();
    if &magic != MAGIC_SIGNED {
        return Err(FileHeaderError::BadMagic { got: magic });
    }
    let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if version != ZONE_VERSION_PC {
        return Err(FileHeaderError::BadVersion { got: version });
    }
    let marker: [u8; 8] = bytes[12..20].try_into().unwrap();
    if &marker != MAGIC_ENCRYPTED {
        return Err(FileHeaderError::NotEncrypted { got: marker });
    }
    let name: [u8; ZONE_NAME_LEN] = bytes[0x18..0x18 + ZONE_NAME_LEN].try_into().unwrap();
    let name_len = name.iter().position(|&b| b == 0).unwrap_or(ZONE_NAME_LEN);
    if name_len == 0 {
        return Err(FileHeaderError::EmptyName);
    }
    Ok(FileHeader {
        version,
        name,
        name_len,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chunk<'a> {
    pub index: usize,
    pub bytes: &'a [u8],
}

pub struct Chunks<'a> {
    file: &'a [u8],
    pos: usize,
    index: usize,
    done: bool,
}

impl<'a> Chunks<'a> {
    pub fn new(file: &'a [u8]) -> Self {
        Self {
            file,
            pos: CHUNKS_OFFSET,
            index: 0,
            done: false,
        }
    }
}

impl<'a> Iterator for Chunks<'a> {
    type Item = Result<Chunk<'a>, usize>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done || self.pos + 4 > self.file.len() {
            return None;
        }
        let at = self.pos;
        let len = u32::from_le_bytes(self.file[at..at + 4].try_into().unwrap()) as usize;
        if len == 0 {
            self.done = true;
            return None;
        }
        let start = at + 4;
        let Some(bytes) = self.file.get(start..start + len) else {
            self.done = true;
            return Some(Err(at));
        };
        self.pos = start + len;
        let index = self.index;
        self.index += 1;
        Some(Ok(Chunk { index, bytes }))
    }
}
