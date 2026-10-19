//! Chunk NBT in the 26.3 layout (`SerializableChunkData`): sections with
//! packed block-state and biome containers, heightmaps, status and the
//! post-processing lists, plus the generation data this engine keeps.
//!
//! Block entities, scheduled ticks, light (`isLightOn` once lit) and the
//! entities generation created (`entities`, as proto chunks keep them) are
//! in vanilla form. Not yet: structure starts and references (written
//! empty), and FULL chunks' entities in the separate entity storage.

use crate::biome::BiomeId;
use crate::block::BlockStateId;
use crate::chunk::{Chunk, ChunkStatus, HeightmapKind, SECTION_BIOMES, SECTION_VOLUME};
use crate::nbt::Tag;
use crate::palette::PalettedContainer;
use crate::pos::ChunkPos;
use crate::registries::Registries;
use std::collections::BTreeMap;

/// `SharedConstants.getCurrentVersion().dataVersion()` for 26.3.
pub const DATA_VERSION: i32 = 5023;

const STATUS_NAMES: [&str; 10] = [
    "minecraft:empty",
    "minecraft:structure_starts",
    "minecraft:structure_references",
    "minecraft:biomes",
    "minecraft:terrain",
    "minecraft:features",
    "minecraft:initialize_light",
    "minecraft:light",
    "minecraft:spawn",
    "minecraft:full",
];

fn heightmap_name(kind: HeightmapKind) -> &'static str {
    match kind {
        HeightmapKind::WorldSurfaceWg => "WORLD_SURFACE_WG",
        HeightmapKind::WorldSurface => "WORLD_SURFACE",
        HeightmapKind::OceanFloorWg => "OCEAN_FLOOR_WG",
        HeightmapKind::OceanFloor => "OCEAN_FLOOR",
        HeightmapKind::MotionBlocking => "MOTION_BLOCKING",
        HeightmapKind::MotionBlockingNoLeaves => "MOTION_BLOCKING_NO_LEAVES",
    }
}

fn compound(entries: impl IntoIterator<Item = (&'static str, Tag)>) -> Tag {
    Tag::Compound(entries.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

/// `Mth.ceillog2`.
fn ceil_log2(n: usize) -> u32 {
    if n <= 1 { 0 } else { usize::BITS - (n - 1).leading_zeros() }
}

/// `SimpleBitStorage`: values never span two longs.
fn pack(values: &[usize], bits: u32) -> Vec<i64> {
    let per_long = (64 / bits) as usize;
    let mut out = vec![0i64; values.len().div_ceil(per_long)];
    for (i, &v) in values.iter().enumerate() {
        out[i / per_long] |= (v as i64) << ((i % per_long) as u32 * bits);
    }
    out
}

fn unpack(data: &[i64], bits: u32, count: usize) -> Result<Vec<usize>, String> {
    let per_long = (64 / bits) as usize;
    if data.len() != count.div_ceil(per_long) {
        return Err(format!("packed array of {} longs for {count} {bits}-bit values", data.len()));
    }
    let mask = (1u64 << bits) - 1;
    Ok((0..count).map(|i| ((data[i / per_long] as u64 >> ((i % per_long) as u32 * bits)) & mask) as usize).collect())
}

/// `PalettedContainer.pack`: palette in first-appearance order, then the
/// storage width its strategy picks for that palette size.
fn write_container<T: Copy + Eq + std::hash::Hash, const N: usize>(container: &PalettedContainer<T, N>, block_states: bool, name: impl Fn(T) -> Tag) -> Tag {
    let values = container.to_array();
    // The palette in order of first appearance (`PalettedContainer.pack`).
    // Palettes are short and runs are long, so the previous value and a
    // scan of the palette find most entries; a map takes over for long ones.
    let mut palette: Vec<T> = Vec::new();
    let mut index: Option<std::collections::HashMap<T, usize>> = None;
    let mut last: Option<(T, usize)> = None;
    let ids: Vec<usize> = values
        .iter()
        .map(|&v| {
            if let Some((previous, id)) = last {
                if previous == v {
                    return id;
                }
            }
            let id = match &mut index {
                Some(index) => *index.entry(v).or_insert_with(|| {
                    palette.push(v);
                    palette.len() - 1
                }),
                None => match palette.iter().position(|&p| p == v) {
                    Some(id) => id,
                    None => {
                        palette.push(v);
                        if palette.len() > 64 {
                            index = Some(palette.iter().enumerate().map(|(i, &p)| (p, i)).collect());
                        }
                        palette.len() - 1
                    }
                },
            };
            last = Some((v, id));
            id
        })
        .collect();
    let bits = match ceil_log2(palette.len()) {
        0 => 0,
        b if block_states && b <= 4 => 4,
        b => b,
    };
    let mut map = BTreeMap::new();
    map.insert("palette".to_owned(), Tag::List(palette.into_iter().map(name).collect()));
    if bits > 0 {
        map.insert("data".to_owned(), Tag::LongArray(pack(&ids, bits)));
    }
    Tag::Compound(map)
}

fn read_container<T: Copy + Eq, const N: usize>(tag: Option<&Tag>, block_states: bool, parse: impl Fn(&Tag) -> Result<T, String>) -> Result<PalettedContainer<T, N>, String> {
    let tag = tag.ok_or("missing paletted container")?;
    let palette: Vec<T> = tag.get("palette").and_then(Tag::as_list).ok_or("container lacks a palette")?.iter().map(parse).collect::<Result<_, _>>()?;
    if palette.len() == 1 {
        return Ok(PalettedContainer::Single(palette[0]));
    }
    let bits = match ceil_log2(palette.len()) {
        b if block_states && b <= 4 => 4,
        b => b,
    };
    let data = match tag.get("data") {
        Some(Tag::LongArray(data)) => data,
        _ => return Err("multi-value container lacks data".into()),
    };
    let ids = unpack(data, bits, N)?;
    let mut values: Box<[T; N]> = vec![palette[0]; N].into_boxed_slice().try_into().unwrap_or_else(|_| unreachable!());
    for (slot, id) in values.iter_mut().zip(ids) {
        *slot = *palette.get(id).ok_or_else(|| format!("palette index {id} out of range"))?;
    }
    Ok(PalettedContainer::Direct(values))
}

/// A block state as 26.3 writes it: its block id for the default state,
/// otherwise `{id, properties}` with every property.
fn state_tag(registries: &Registries, state: BlockStateId) -> Tag {
    let blocks = &registries.blocks;
    let info = blocks.block(blocks.block_of(state));
    let id = Tag::String(info.name.as_str().to_owned());
    if state == info.default_state() {
        return id;
    }
    let properties = info
        .properties()
        .iter()
        .map(|p| (p.name.to_string(), Tag::String(blocks.property(state, &p.name).unwrap_or_default().to_owned())))
        .collect();
    compound([("id", id), ("properties", Tag::Compound(properties))])
}

fn parse_state(registries: &Registries, tag: &Tag) -> Result<BlockStateId, String> {
    let (id, properties) = match tag {
        Tag::String(id) => (id.as_str(), None),
        Tag::Compound(_) => {
            let id = tag.get("id").or_else(|| tag.get("Name")).and_then(Tag::as_str).ok_or("block state lacks an id")?;
            (id, tag.get("properties").or_else(|| tag.get("Properties")).and_then(Tag::as_compound))
        }
        _ => return Err("bad block state tag".into()),
    };
    let text = match properties {
        Some(p) if !p.is_empty() => format!("{id}[{}]", p.iter().map(|(k, v)| format!("{k}={}", v.as_str().unwrap_or_default())).collect::<Vec<_>>().join(",")),
        _ => id.to_owned(),
    };
    if let Ok(state) = registries.blocks.parse_state(&text) {
        return Ok(state);
    }
    if let Some(block_id) = registries.blocks.block_by_name(id) {
        return Ok(registries.blocks.block(block_id).default_state());
    }
    Ok(BlockStateId::AIR)
}

/// Serializes a chunk.
pub fn write_chunk(chunk: &Chunk, registries: &Registries, last_update: i64) -> Tag {
    let min_section = chunk.min_section_y();
    // `SerializableChunkData.write`: one entry per light section, from one
    // below the build range to one above; outside the range only when it
    // carries light.
    let light = chunk.light.as_deref();
    let light_layer = |layers: Option<&Vec<Option<Box<[u8; 2048]>>>>, sy: i32| -> Option<Tag> {
        let light = light?;
        let layer = layers?.get((sy - light.min_section) as usize)?.as_ref()?;
        Some(Tag::ByteArray(layer.iter().map(|&b| b as i8).collect()))
    };
    let mut sections: Vec<Tag> = Vec::new();
    for sy in min_section - 1..=min_section + chunk.sections().len() as i32 {
        let mut tag = BTreeMap::new();
        if let Some(section) = usize::try_from(sy - min_section).ok().and_then(|i| chunk.sections().get(i)) {
            tag.insert("block_states".to_owned(), write_container(&section.blocks, true, |s| state_tag(registries, s)));
            tag.insert("biomes".to_owned(), write_container(&section.biomes, false, |b: BiomeId| Tag::String(registries.biomes.get(b).name.as_str().to_owned())));
        }
        if let Some(block) = light_layer(light.map(|l| &l.block), sy) {
            tag.insert("BlockLight".to_owned(), block);
        }
        if let Some(sky) = light_layer(light.map(|l| &l.sky), sy) {
            tag.insert("SkyLight".to_owned(), sky);
        }
        if !tag.is_empty() {
            tag.insert("Y".to_owned(), Tag::Byte(sy as i8));
            sections.push(Tag::Compound(tag));
        }
    }
    let bits = ceil_log2(chunk.height() as usize + 1);
    let heightmaps: BTreeMap<String, Tag> = chunk
        .status
        .heightmaps()
        .iter()
        .map(|&kind| {
            let values: Vec<usize> = (0..256).map(|i| (chunk.heightmaps.get(kind, i % 16, i / 16) - chunk.min_y()) as usize).collect();
            (heightmap_name(kind).to_owned(), Tag::LongArray(pack(&values, bits)))
        })
        .collect();
    let mut post_processing: Vec<Vec<Tag>> = vec![Vec::new(); chunk.sections().len()];
    for &(x, y, z) in &chunk.generation.post_processing {
        let section = ((y >> 4) - min_section) as usize;
        if let Some(list) = post_processing.get_mut(section) {
            list.push(Tag::Short(((x & 15) | ((y & 15) << 4) | ((z & 15) << 8)) as i16));
        }
    }
    let (mut block_ticks, mut fluid_ticks) = (Vec::new(), Vec::new());
    for t in &chunk.generation.ticks {
        let (x, y, z) = t.pos;
        let tick = compound([("i", Tag::String(t.id.clone())), ("x", Tag::Int(x)), ("y", Tag::Int(y)), ("z", Tag::Int(z)), ("t", Tag::Int(t.delay)), ("p", Tag::Int(t.priority))]);
        if t.fluid { fluid_ticks.push(tick) } else { block_ticks.push(tick) }
    }
    // `getBlockEntityNbtForSaving`: a block entity wins over a pending tag;
    // FULL chunks mark which were block entities with `keepPacked`.
    let full = chunk.status == ChunkStatus::Full;
    let mut block_entities = Vec::new();
    let store = &chunk.block_entities;
    for nbt in store.entities.values() {
        block_entities.push(with_keep_packed(nbt, full.then_some(false)));
    }
    for (pos, nbt) in &store.pending {
        if !store.entities.contains_key(pos) {
            block_entities.push(with_keep_packed(nbt, full.then_some(true)));
        }
    }
    let structures = compound([("References", Tag::Compound(BTreeMap::new())), ("starts", Tag::Compound(BTreeMap::new()))]);
    let mut tag = compound([
        ("DataVersion", Tag::Int(DATA_VERSION)),
        ("xPos", Tag::Int(chunk.pos.x)),
        ("zPos", Tag::Int(chunk.pos.z)),
        ("yPos", Tag::Int(min_section)),
        ("Status", Tag::String(STATUS_NAMES[chunk.status.index()].to_owned())),
        ("LastUpdate", Tag::Long(last_update)),
        ("InhabitedTime", Tag::Long(chunk.inhabited_time)),
        ("isLightOn", Tag::Byte(i8::from(light.is_some_and(|l| l.correct)))),
        ("sections", Tag::List(sections)),
        ("Heightmaps", Tag::Compound(heightmaps)),
        ("PostProcessing", Tag::List(post_processing.into_iter().map(Tag::List).collect())),
        ("block_ticks", Tag::List(block_ticks)),
        ("fluid_ticks", Tag::List(fluid_ticks)),
        ("block_entities", Tag::List(block_entities)),
        ("structures", structures),
    ]);
    // Proto chunks keep generation's entities; FULL chunks' live in the
    // entity storage (`write_entities`).
    if chunk.status != ChunkStatus::Full {
        if let Tag::Compound(map) = &mut tag {
            map.insert("entities".to_owned(), Tag::List(chunk.generation.entities.clone()));
        }
    }
    tag
}

/// An entity storage chunk (`EntityStorage`): `Position` and `Entities`.
pub fn write_entities(pos: ChunkPos, entities: &[Tag]) -> Tag {
    compound([
        ("DataVersion", Tag::Int(DATA_VERSION)),
        ("Position", Tag::IntArray(vec![pos.x, pos.z])),
        ("Entities", Tag::List(entities.to_vec())),
    ])
}

/// A block entity tag with `keepPacked` set to the given value, or removed.
fn with_keep_packed(nbt: &Tag, keep_packed: Option<bool>) -> Tag {
    let mut nbt = nbt.clone();
    if let Tag::Compound(map) = &mut nbt {
        match keep_packed {
            Some(value) => map.insert("keepPacked".to_owned(), Tag::Byte(i8::from(value))),
            None => map.remove("keepPacked"),
        };
    }
    nbt
}

/// Reads a chunk written by vanilla 26.3 or by `write_chunk`.
pub fn read_chunk(tag: &Tag, registries: &Registries, min_y: i32, height: i32) -> Result<Chunk, String> {
    let int = |key: &str| tag.get(key).and_then(Tag::as_i64).map(|v| v as i32).ok_or_else(|| format!("chunk lacks {key}"));
    let pos = ChunkPos::new(int("xPos")?, int("zPos")?);
    let status_str = tag.get("Status").and_then(Tag::as_str).unwrap_or("minecraft:full");
    let status_trimmed = status_str.strip_prefix("minecraft:").unwrap_or(status_str);
    let status = STATUS_NAMES
        .iter()
        .position(|&s| s.strip_prefix("minecraft:").unwrap_or(s) == status_trimmed)
        .map(|i| ChunkStatus::ALL[i])
        .unwrap_or(ChunkStatus::Full);
    let plains = registries.biomes.id("minecraft:plains").unwrap_or(BiomeId(0));
    let mut chunk = Chunk::new(pos, min_y, height, plains);
    chunk.status = status;
    chunk.inhabited_time = tag.get("InhabitedTime").and_then(Tag::as_i64).unwrap_or(0);
    let min_section = chunk.min_section_y();
    let sections = tag.get("sections").and_then(Tag::as_list).unwrap_or(&[]);
    for section in sections {
        let y = section.get("Y").and_then(Tag::as_i64).ok_or("section lacks Y")? as i32;
        let Some(slot) = chunk.sections_mut().get_mut((y - min_section) as usize).filter(|_| y >= min_section) else { continue };
        if section.get("block_states").is_some() {
            slot.blocks = read_container::<BlockStateId, SECTION_VOLUME>(section.get("block_states"), true, |t| parse_state(registries, t))?;
            slot.emitters = std::sync::OnceLock::new();
        }
        if section.get("biomes").is_some() {
            slot.biomes = read_container::<BiomeId, SECTION_BIOMES>(section.get("biomes"), false, |t| {
                let name = t.as_str().ok_or("biome is not a string")?;
                Ok(registries.biomes.id(name).unwrap_or(plains))
            })?;
        }
        slot.recount(registries);
    }
    // Light arrays are saved whenever the light engine has data, before the
    // chunk's light is complete too.
    let correct = tag.get("isLightOn").and_then(Tag::as_i64) == Some(1);
    if correct || sections.iter().any(|s| s.get("BlockLight").is_some() || s.get("SkyLight").is_some()) {
        let count = chunk.sections().len() + 2;
        let mut light = crate::light::ChunkLight { min_section: min_section - 1, correct, block: vec![None; count], sky: vec![None; count] };
        for section in sections {
            let Some(y) = section.get("Y").and_then(Tag::as_i64) else { continue };
            let Some(i) = usize::try_from(y as i32 - light.min_section).ok().filter(|&i| i < count) else { continue };
            for (key, layers) in [("BlockLight", &mut light.block), ("SkyLight", &mut light.sky)] {
                if let Some(Tag::ByteArray(data)) = section.get(key) {
                    if data.len() == 2048 {
                        let mut layer = Box::new([0u8; 2048]);
                        for (slot, &b) in layer.iter_mut().zip(data) {
                            *slot = b as u8;
                        }
                        layers[i] = Some(layer);
                    }
                }
            }
        }
        chunk.light = Some(std::sync::Arc::new(light));
    }
    let bits = ceil_log2(height as usize + 1);
    if let Some(maps) = tag.get("Heightmaps").and_then(Tag::as_compound) {
        for kind in HeightmapKind::ALL {
            if let Some(Tag::LongArray(data)) = maps.get(heightmap_name(kind)) {
                let values = unpack(data, bits, 256)?;
                for (i, v) in values.into_iter().enumerate() {
                    chunk.heightmaps.set(kind, i % 16, i / 16, v as i32 + min_y);
                }
            }
        }
    }
    if let Some(lists) = tag.get("PostProcessing").and_then(Tag::as_list) {
        for (i, list) in lists.iter().enumerate() {
            for packed in list.as_list().unwrap_or(&[]) {
                let p = packed.as_i64().ok_or("bad post-processing entry")? as i32;
                let section_y = min_section + i as i32;
                chunk.generation.post_processing.push((pos.min_block_x() + (p & 15), section_y * 16 + ((p >> 4) & 15), pos.min_block_z() + ((p >> 8) & 15)));
            }
        }
    }
    for (key, fluid) in [("block_ticks", false), ("fluid_ticks", true)] {
        for tick in tag.get(key).and_then(Tag::as_list).unwrap_or(&[]) {
            let c = |k: &str| tick.get(k).and_then(Tag::as_i64).map(|v| v as i32).ok_or("bad tick");
            let id = tick.get("i").and_then(Tag::as_str).ok_or("tick lacks a type")?.to_owned();
            chunk.generation.ticks.push(crate::chunk::ScheduledTick { pos: (c("x")?, c("y")?, c("z")?), fluid, id, delay: c("t")?, priority: c("p")? });
        }
    }
    for entry in tag.get("block_entities").and_then(Tag::as_list).unwrap_or(&[]) {
        let c = |k: &str| entry.get(k).and_then(Tag::as_i64).map(|v| v as i32).ok_or("block entity lacks a position");
        let pos = (c("x")?, c("y")?, c("z")?);
        let packed = entry.get("keepPacked").and_then(Tag::as_i64).map(|v| v != 0);
        let nbt = with_keep_packed(entry, None);
        // A proto chunk reads every tag as pending (`setBlockEntityNbt`).
        if status == ChunkStatus::Full && packed == Some(false) {
            chunk.block_entities.entities.insert(pos, nbt);
        } else {
            chunk.block_entities.pending.insert(pos, nbt);
        }
    }
    chunk.generation.entities = tag.get("entities").and_then(Tag::as_list).map(<[Tag]>::to_vec).unwrap_or_default();
    Ok(chunk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packing_round_trips_and_keeps_values_in_one_long() {
        let values: Vec<usize> = (0..4096).map(|i| (i * 7) % 20).collect();
        let packed = pack(&values, 5);
        assert_eq!(packed.len(), 4096usize.div_ceil(12));
        assert_eq!(unpack(&packed, 5, 4096).unwrap(), values);
        assert_eq!(ceil_log2(385), 9);
        assert_eq!(crate::chunk::block_index(1, 2, 3), (2 << 8) | (3 << 4) | 1);
    }
}
