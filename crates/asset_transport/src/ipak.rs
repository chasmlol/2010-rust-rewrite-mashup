use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const MAGIC: [u8; 4] = *b"KAPI";
const VERSION: u32 = 0x50000;
const SECTION_INDEX: u32 = 1;
const SECTION_DATA: u32 = 2;
const BLOCK_HEADER: usize = 128;
const COMMANDS_PER_BLOCK: usize = 31;
const COMMAND_RAW: u32 = 0;
const COMMAND_LZO: u32 = 1;
const COMMAND_OUTPUT_CAP: usize = 0x8000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    name_hash: u32,
    data_hash: u32,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    offset: u64,
    size: u32,
}

pub struct IPak {
    path: PathBuf,
    data_offset: u64,
    index: Vec<(Key, Entry)>,
}

pub fn ipak_name_hash(name: &str) -> u32 {
    name.bytes()
        .fold(0u32, |hash, b| hash.wrapping_mul(33) ^ u32::from(b | 0x20))
}

fn le32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

impl IPak {
    pub fn open(path: &Path) -> Result<Self, String> {
        let fail = |what: &str| format!("{}: {what}", path.display());
        let mut file = File::open(path).map_err(|e| fail(&e.to_string()))?;
        let mut head = [0u8; 16];
        file.read_exact(&mut head)
            .map_err(|e| fail(&e.to_string()))?;
        if head[..4] != MAGIC {
            return Err(fail("not a little-endian ipak"));
        }
        if le32(&head, 4) != VERSION {
            return Err(fail(&format!("ipak version {:#x}", le32(&head, 4))));
        }
        let section_count = le32(&head, 12) as usize;
        let mut sections = vec![0u8; 16 * section_count];
        file.read_exact(&mut sections)
            .map_err(|e| fail(&e.to_string()))?;
        let mut index_section = None;
        let mut data_offset = None;
        for s in sections.as_chunks::<16>().0.iter() {
            match le32(s, 0) {
                SECTION_INDEX => index_section = Some((le32(s, 4), le32(s, 12))),
                SECTION_DATA => data_offset = Some(u64::from(le32(s, 4))),
                _ => {}
            }
        }
        let (index_offset, count) = index_section.ok_or_else(|| fail("no index section"))?;
        let data_offset = data_offset.ok_or_else(|| fail("no data section"))?;
        let mut raw = vec![0u8; 16 * count as usize];
        file.seek(SeekFrom::Start(u64::from(index_offset)))
            .and_then(|_| file.read_exact(&mut raw))
            .map_err(|e| fail(&e.to_string()))?;
        let mut index: Vec<(Key, Entry)> = raw
            .as_chunks::<16>()
            .0
            .iter()
            .map(|e| {
                (
                    Key {
                        data_hash: le32(e, 0),
                        name_hash: le32(e, 4),
                    },
                    Entry {
                        offset: u64::from(le32(e, 8)),
                        size: le32(e, 12),
                    },
                )
            })
            .collect();
        index.sort_by_key(|(key, _)| *key);
        Ok(Self {
            path: path.to_owned(),
            data_offset,
            index,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn contains(&self, name_hash: u32, data_hash: u32) -> bool {
        self.find(name_hash, data_hash).is_some()
    }

    fn find(&self, name_hash: u32, data_hash: u32) -> Option<Entry> {
        let key = Key {
            name_hash,
            data_hash,
        };
        self.index
            .binary_search_by_key(&key, |(k, _)| *k)
            .ok()
            .map(|i| self.index[i].1)
    }

    pub fn read(&self, name_hash: u32, data_hash: u32) -> Option<Result<Vec<u8>, String>> {
        let entry = self.find(name_hash, data_hash)?;
        Some(self.read_entry(entry))
    }

    fn read_entry(&self, entry: Entry) -> Result<Vec<u8>, String> {
        let fail = |what: String| format!("{}: {what}", self.path.display());
        let mut file = File::open(&self.path).map_err(|e| fail(e.to_string()))?;
        let mut stored = vec![0u8; entry.size as usize];
        file.seek(SeekFrom::Start(self.data_offset + entry.offset))
            .and_then(|_| file.read_exact(&mut stored))
            .map_err(|e| fail(e.to_string()))?;
        inflate_entry(&stored).map_err(fail)
    }
}

fn inflate_entry(stored: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos + BLOCK_HEADER <= stored.len() {
        let head = &stored[pos..pos + BLOCK_HEADER];
        let count_offset = le32(head, 0);
        let count = (count_offset >> 24) as usize;
        let offset = (count_offset & 0x00FF_FFFF) as usize;
        if count > COMMANDS_PER_BLOCK {
            return Err(format!("ipak block at {pos} has {count} commands"));
        }
        let mut cursor = pos + BLOCK_HEADER;
        for i in 0..count {
            let command = le32(head, 4 + 4 * i);
            let size = (command & 0x00FF_FFFF) as usize;
            let kind = command >> 24;
            let body = stored
                .get(cursor..cursor + size)
                .ok_or_else(|| format!("ipak command {i} at {cursor} runs past the entry"))?;
            match kind {
                COMMAND_RAW | COMMAND_LZO if offset != out.len() && i == 0 => {
                    return Err(format!(
                        "ipak block at {pos} continues at {offset}, entry is at {}",
                        out.len()
                    ));
                }
                COMMAND_RAW => out.extend_from_slice(body),
                COMMAND_LZO => {
                    let inflated = lzokay_native::decompress_all(body, Some(COMMAND_OUTPUT_CAP))
                        .map_err(|e| format!("ipak LZO command {i} at {cursor}: {e:?}"))?;
                    out.extend_from_slice(&inflated);
                }
                _ => {}
            }
            cursor += size;
        }
        pos = cursor.next_multiple_of(BLOCK_HEADER);
    }
    Ok(out)
}
