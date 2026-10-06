use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const MAGIC: [u8; 4] = *b"2UX#";
const ENTRY_LEN: usize = 20;

pub const SAB_FRAME_RATES: [u32; 9] = [
    8000, 12000, 16000, 24000, 32000, 44100, 48000, 96000, 192000,
];

pub const SAB_FORMAT_PCMS16: u8 = 0;
pub const SAB_FORMAT_FLAC: u8 = 8;

#[derive(Clone, Copy, Debug)]
pub struct SabEntry {
    pub id: u32,
    pub size: u32,
    pub offset: u32,
    pub frame_count: u32,
    pub frame_rate_index: u8,
    pub channels: u8,
    pub looping: bool,
    pub format: u8,
}

impl SabEntry {
    pub fn frame_rate(&self) -> Option<u32> {
        SAB_FRAME_RATES
            .get(usize::from(self.frame_rate_index))
            .copied()
    }
}

pub struct SoundAssetBank {
    path: PathBuf,
    entries: HashMap<u32, SabEntry>,
}

fn le32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

impl SoundAssetBank {
    pub fn open(path: &Path) -> Result<Self, String> {
        let fail = |what: String| format!("{}: {what}", path.display());
        let mut file = File::open(path).map_err(|e| fail(e.to_string()))?;
        let mut head = [0u8; 0x30];
        file.read_exact(&mut head)
            .map_err(|e| fail(e.to_string()))?;
        if head[..4] != MAGIC {
            return Err(fail("not a sound asset bank".into()));
        }
        let entry_size = le32(&head, 8) as usize;
        if entry_size != ENTRY_LEN {
            return Err(fail(format!("entry size {entry_size}")));
        }
        let count = le32(&head, 0x14) as usize;
        let entry_offset = u64::from_le_bytes(head[0x28..0x30].try_into().unwrap());
        let mut table = vec![0u8; ENTRY_LEN * count];
        file.seek(SeekFrom::Start(entry_offset))
            .and_then(|_| file.read_exact(&mut table))
            .map_err(|e| fail(e.to_string()))?;
        let entries = table
            .as_chunks::<ENTRY_LEN>()
            .0
            .iter()
            .map(|e| {
                let entry = SabEntry {
                    id: le32(e, 0),
                    size: le32(e, 4),
                    offset: le32(e, 8),
                    frame_count: le32(e, 12),
                    frame_rate_index: e[16],
                    channels: e[17],
                    looping: e[18] != 0,
                    format: e[19],
                };
                (entry.id, entry)
            })
            .collect();
        Ok(Self {
            path: path.to_owned(),
            entries,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entry(&self, id: u32) -> Option<SabEntry> {
        self.entries.get(&id).copied()
    }

    pub fn read(&self, id: u32) -> Option<Result<(SabEntry, Vec<u8>), String>> {
        let entry = self.entry(id)?;
        let read = || -> std::io::Result<Vec<u8>> {
            let mut file = File::open(&self.path)?;
            let mut bytes = vec![0u8; entry.size as usize];
            file.seek(SeekFrom::Start(u64::from(entry.offset)))?;
            file.read_exact(&mut bytes)?;
            Ok(bytes)
        };
        Some(
            read()
                .map(|bytes| (entry, bytes))
                .map_err(|e| format!("{}: entry {id:08x}: {e}", self.path.display())),
        )
    }
}

pub fn open_sound_asset_banks(sound_dir: &Path) -> (Vec<SoundAssetBank>, Vec<String>) {
    let mut report = Vec::new();
    let Ok(entries) = std::fs::read_dir(sound_dir) else {
        report.push(format!("cannot read {}", sound_dir.display()));
        return (Vec::new(), report);
    };
    let mut paths: Vec<_> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("sabs") || ext.eq_ignore_ascii_case("sabl")
            })
        })
        .collect();
    paths.sort();
    let banks = paths
        .iter()
        .filter_map(|path| match SoundAssetBank::open(path) {
            Ok(bank) => Some(bank),
            Err(error) => {
                report.push(error);
                None
            }
        })
        .collect();
    (banks, report)
}

pub fn snd_hash_name(name: &str) -> u32 {
    if name.is_empty() {
        return 0;
    }
    let hash = name.bytes().fold(0x1505u32, |hash, b| {
        u32::from(b.to_ascii_lowercase()).wrapping_add(hash.wrapping_mul(0x1003F))
    });
    hash.max(1)
}
