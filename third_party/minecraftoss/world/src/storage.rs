//! Chunk persistence for one dimension: vanilla 26.3 region files under
//! `<world>/dimensions/<namespace>/<path>/region`, chunks in the
//! `SerializableChunkData` layout (`minecraftoss_core::chunk_nbt`).
//!
//! Chunks of every status are saved, as vanilla saves proto chunks: a
//! chunk that was decorated keeps its decoration and is never decorated
//! again after a reload. FULL chunks' entities live in the entity storage,
//! `<dimension>/entities`, which the level writes as its entities leave
//! (`EntityStorage`); a FULL chunk saved before the level ever wrote its
//! entities seeds the storage with the ones generation made.

use minecraftoss_core::anvil::{region_of, region_path, RegionFile, StoredChunk};
use minecraftoss_core::chunk::ChunkStatus;
use minecraftoss_core::chunk_nbt::{read_chunk, write_chunk, write_entities};
use minecraftoss_core::nbt::Tag;
use minecraftoss_core::{Chunk, ChunkPos, Registries};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct ChunkStorage {
    dir: PathBuf,
    entities: EntityRegions,
    registries: Arc<Registries>,
    min_y: i32,
    height: i32,
    regions: Mutex<Regions>,
    /// Held while region files are written, so two flushes never write the
    /// same file at once; loads and saves only take the region caches.
    writing: Mutex<()>,
    read_only: bool,
}

/// Region files of the entity storage.
struct EntityRegions {
    dir: PathBuf,
    regions: Mutex<Regions>,
}

#[derive(Default)]
struct Regions {
    loaded: HashMap<(i32, i32), RegionFile>,
    dirty: HashSet<(i32, i32)>,
}

impl ChunkStorage {
    /// Storage for a dimension (`minecraft:overworld`) of the world at `world`.
    pub fn new(world: &Path, dimension: &str, registries: Arc<Registries>, min_y: i32, height: i32) -> Self {
        let (namespace, path) = dimension.split_once(':').unwrap_or(("minecraft", dimension));
        let modern_base = world.join("dimensions").join(namespace).join(path);
        let legacy_base = match (namespace, path) {
            ("minecraft", "overworld") => world.to_path_buf(),
            ("minecraft", "the_nether" | "nether") => world.join("DIM-1"),
            ("minecraft", "the_end" | "end") => world.join("DIM1"),
            _ => modern_base.clone(),
        };
        let base = if modern_base.join("region").is_dir() {
            modern_base
        } else if legacy_base.join("region").is_dir() {
            legacy_base
        } else {
            modern_base
        };
        let read_only = std::env::var("IW4L_MINECRAFT_READONLY")
            .ok()
            .map(|v| !matches!(v.trim().to_ascii_lowercase().as_str(), "0" | "false" | "no" | "off"))
            .unwrap_or_else(|| base.join("region").is_dir());
        Self {
            dir: base.join("region"),
            entities: EntityRegions { dir: base.join("entities"), regions: Mutex::default() },
            registries,
            min_y,
            height,
            regions: Mutex::default(),
            writing: Mutex::default(),
            read_only,
        }
    }

    fn with_region<T>(&self, region: (i32, i32), f: impl FnOnce(&mut RegionFile) -> T) -> Result<T, String> {
        with_region(&self.dir, &self.regions, region, f)
    }

    fn with_entity_region<T>(&self, region: (i32, i32), f: impl FnOnce(&mut RegionFile) -> T) -> Result<T, String> {
        with_region(&self.entities.dir, &self.entities.regions, region, f)
    }

    /// A saved chunk, if there is one. Only the lookup holds the region
    /// cache; the chunk is decompressed and read outside it.
    pub fn load(&self, pos: ChunkPos) -> Result<Option<Chunk>, String> {
        let (region, index) = region_of(pos);
        let Some(stored) = self.with_region(region, |file| file.stored(index))? else { return Ok(None) };
        let mut chunk = read_chunk(&stored.parse()?, &self.registries, self.min_y, self.height)?;
        if chunk.status == ChunkStatus::Full {
            let entities = self.with_entity_region(region, |file| file.stored(index))?.map(|stored| stored.parse()).transpose()?;
            chunk.generation.entities = entities.and_then(|t| t.get("Entities").and_then(Tag::as_list).map(<[Tag]>::to_vec)).unwrap_or_default();
        }
        Ok(Some(chunk))
    }

    /// Queues a chunk for the next `flush`, compressed before the region
    /// cache is taken.
    pub fn save(&self, chunk: &Chunk) -> Result<(), String> {
        let started = std::time::Instant::now();
        let tag = write_chunk(chunk, &self.registries, 0);
        crate::chunk_map::stage_times::add(crate::chunk_map::stage_times::SAVE_NBT, started);
        let started = std::time::Instant::now();
        let stored = StoredChunk::encode(&tag);
        crate::chunk_map::stage_times::add(crate::chunk_map::stage_times::SAVE_COMPRESS, started);
        let (region, index) = region_of(chunk.pos);
        store(&self.dir, &self.regions, region, index, stored)?;
        // The level owns a FULL chunk's entities once it has saved them.
        if chunk.status == ChunkStatus::Full && !self.with_entity_region(region, |file| file.contains(index))? {
            self.save_entities(chunk.pos, &chunk.generation.entities)?;
        }
        Ok(())
    }

    /// A chunk's saved entities (`EntityStorage.loadEntities`): `None` when
    /// the storage has no entry for it yet.
    pub fn load_entities(&self, pos: ChunkPos) -> Result<Option<Vec<Tag>>, String> {
        let (region, index) = region_of(pos);
        let tag = self.with_entity_region(region, |file| file.stored(index))?.map(|stored| stored.parse()).transpose()?;
        Ok(tag.map(|t| t.get("Entities").and_then(Tag::as_list).map(<[Tag]>::to_vec).unwrap_or_default()))
    }

    /// Queues a chunk's entities for the next `flush` (`EntityStorage.storeEntities`).
    pub fn save_entities(&self, pos: ChunkPos, entities: &[Tag]) -> Result<(), String> {
        let (region, index) = region_of(pos);
        let stored = StoredChunk::encode(&write_entities(pos, entities));
        store(&self.entities.dir, &self.entities.regions, region, index, stored)
    }

    /// Writes every region with unsaved chunks: each is copied (its chunks
    /// are shared) under the region cache and written outside it.
    pub fn flush(&self) -> Result<(), String> {
        if self.read_only {
            for regions in [&self.regions, &self.entities.regions] {
                regions.lock().expect("region cache").dirty.clear();
            }
            return Ok(());
        }
        let _writing = self.writing.lock().expect("region writer");
        for (dir, regions) in [(&self.dir, &self.regions), (&self.entities.dir, &self.entities.regions)] {
            let snapshots: Vec<((i32, i32), RegionFile)> = {
                let mut regions = regions.lock().expect("region cache");
                let dirty: Vec<(i32, i32)> = regions.dirty.drain().collect();
                dirty.into_iter().filter_map(|region| regions.loaded.get(&region).map(|file| (region, file.clone()))).collect()
            };
            for (region, file) in snapshots {
                file.write(&region_path(dir, region))?;
            }
        }
        Ok(())
    }
}

/// Sets a stored chunk and marks its region for the next flush.
fn store(dir: &Path, regions: &Mutex<Regions>, region: (i32, i32), index: usize, stored: StoredChunk) -> Result<(), String> {
    let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as u32);
    with_region(dir, regions, region, |file| file.set_stored(index, stored, timestamp))?;
    regions.lock().expect("region cache").dirty.insert(region);
    Ok(())
}

fn with_region<T>(dir: &Path, regions: &Mutex<Regions>, region: (i32, i32), f: impl FnOnce(&mut RegionFile) -> T) -> Result<T, String> {
    let mut regions = regions.lock().expect("region cache");
    if !regions.loaded.contains_key(&region) {
        let path = region_path(dir, region);
        let file = if path.exists() { RegionFile::read(&path)? } else { RegionFile::new() };
        regions.loaded.insert(region, file);
    }
    Ok(f(regions.loaded.get_mut(&region).expect("inserted")))
}
