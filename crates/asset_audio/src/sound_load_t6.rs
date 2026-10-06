use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use crate::sound_catalog::{CapturedAlias, CapturedSound, LoadedSoundPcm, SoundCatalog};
use crate::{AssetEdge, AssetEdgeReason, ZoneGame, ZoneOwner};
use asset_transport::{SoundAssetBank, snd_hash_name};
use fastfile_t6::{AssetType, Ptr, ZoneLoad};

const SND_BANK_ALIAS_COUNT: usize = 4;
const SND_BANK_ALIAS: usize = 8;
const SND_ALIAS_LIST: u32 = 20;
const SND_ALIAS_LIST_ID: usize = 4;
const SND_ALIAS_LIST_HEAD: usize = 8;
const SND_ALIAS_LIST_COUNT: usize = 12;
const SND_ALIAS: u32 = 96;
const SND_ALIAS_SECONDARY: usize = 12;
const SND_ALIAS_FLAGS0: usize = 24;

fn le16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn decode_ptr(raw: u32) -> Option<Ptr> {
    if raw == 0 || raw >= 0xFFFF_FFFE {
        return None;
    }
    let e = raw - 1;
    Some(Ptr {
        block: (e >> 29) as u8,
        offset: e & 0x1FFF_FFFF,
    })
}

pub fn t6_sound_banks(zone: &Path) -> (Vec<SoundAssetBank>, Vec<String>) {
    match zone.parent().and_then(Path::parent).and_then(Path::parent) {
        Some(root) => asset_transport::open_sound_asset_banks(&root.join("sound")),
        None => (
            Vec::new(),
            vec![format!("{}: no install root", zone.display())],
        ),
    }
}

pub fn capture_t6_sounds<'n>(
    zone: &Path,
    loads: &[&ZoneLoad],
    banks: &[SoundAssetBank],
    names: impl IntoIterator<Item = &'n str>,
) -> (SoundCatalog, Vec<String>, Vec<String>) {
    let game = ZoneGame::T6;
    let mut report = Vec::new();
    let mut lists: HashMap<u32, (&ZoneLoad, Ptr, u32)> = HashMap::new();
    for &load in loads {
        for bank in load.assets.iter().filter(|a| a.ty == AssetType::SoundBank) {
            let (Some(count), Some(array)) = (
                bank.header
                    .get(SND_BANK_ALIAS_COUNT..SND_BANK_ALIAS_COUNT + 4),
                bank.header
                    .get(SND_BANK_ALIAS..SND_BANK_ALIAS + 4)
                    .and_then(|b| decode_ptr(le32(b, 0))),
            ) else {
                continue;
            };
            for i in 0..le32(count, 0) {
                let Ok(list) = load.blocks.bytes(array.at(i * SND_ALIAS_LIST), 20) else {
                    continue;
                };
                if let Some(head) = decode_ptr(le32(list, SND_ALIAS_LIST_HEAD)) {
                    lists.entry(le32(list, SND_ALIAS_LIST_ID)).or_insert((
                        load,
                        head,
                        le32(list, SND_ALIAS_LIST_COUNT),
                    ));
                }
            }
        }
    }

    let mut catalog = SoundCatalog::default();
    catalog.set_capture_zone(ZoneOwner::from_zone_path(zone));
    catalog.set_capture_game(game);
    let mut loaded: BTreeMap<u32, Option<String>> = BTreeMap::new();
    let mut filled = Vec::new();
    let mut queue: Vec<String> = names.into_iter().map(str::to_owned).collect();
    queue.reverse();
    let mut seen: HashSet<String> = queue.iter().cloned().collect();
    while let Some(name) = queue.pop() {
        let name = name.as_str();
        let Some(&(load, head, count)) = lists.get(&snd_hash_name(name)) else {
            report.push(format!("t6 sound {name}: no alias"));
            continue;
        };
        let mut aliases = Vec::with_capacity(count as usize);
        for k in 0..count {
            let Ok(row) = load
                .blocks
                .bytes(head.at(k * SND_ALIAS), SND_ALIAS as usize)
            else {
                continue;
            };
            let asset = le32(row, 16);
            loaded
                .entry(asset)
                .or_insert_with(|| load_asset(&mut catalog, banks, asset, name, &mut report));
            let loaded_name = format!("t6/{asset:08x}");
            let secondary = decode_ptr(le32(row, SND_ALIAS_SECONDARY))
                .and_then(|p| load.blocks.cstr(p).ok())
                .and_then(|b| std::str::from_utf8(b).ok())
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
            if let Some(secondary) = &secondary
                && seen.insert(secondary.clone())
            {
                queue.push(secondary.clone());
            }
            aliases.push(CapturedAlias {
                alias_name: name.to_owned(),
                secondary,
                loaded_name: Some(loaded_name.clone()),
                loaded: AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
                file_type: Some(1),
                file_name: Some(loaded_name),
                vol_min: f32::from(le16(row, 60)) / 65535.0,
                vol_max: f32::from(le16(row, 62)) / 65535.0,
                pitch_min: f32::from(le16(row, 64)) / 32767.0,
                pitch_max: f32::from(le16(row, 66)) / 32767.0,
                dist_min: f32::from(le16(row, 68)),
                dist_max: f32::from(le16(row, 70)),
                start_delay: i32::from(le16(row, 54)),
                looping: Some(le32(row, SND_ALIAS_FLAGS0) & 1 != 0),
                probability: f32::from(row[88]) / 255.0,
                ..Default::default()
            });
        }
        if aliases.is_empty() {
            continue;
        }
        catalog.ingest_sound(CapturedSound {
            name: name.to_owned(),
            aliases,
            game: ZoneGame::T6,
            zone: catalog.capture_zone_for_ingest(),
        });
        filled.push(name.to_owned());
    }
    catalog.resolve_loaded_edges();
    catalog.publish();
    (catalog, filled, report)
}

fn load_asset(
    catalog: &mut SoundCatalog,
    banks: &[SoundAssetBank],
    id: u32,
    alias: &str,
    report: &mut Vec<String>,
) -> Option<String> {
    let Some((bank, entry)) = banks
        .iter()
        .find_map(|bank| bank.entry(id).map(|entry| (bank, entry)))
    else {
        report.push(format!("t6 sound {alias}: asset {id:08x} in no bank"));
        return None;
    };
    let source = crate::SabMediaSource {
        bank: bank.path().to_owned(),
        entry,
    };
    if let crate::SabCodec::Unsupported(format) = source.codec() {
        report.push(format!(
            "t6 sound {alias}: asset {id:08x} has unsupported SAB codec {format}"
        ));
    }
    let sound = LoadedSoundPcm::from_sab(source, ZoneGame::T6, catalog.capture_zone_for_ingest());
    let name = sound.name.clone();
    catalog.ingest_loaded(sound);
    Some(name)
}

pub fn t6_sound_names(loads: &[&ZoneLoad]) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    for load in loads {
        for bank in load.assets.iter().filter(|a| a.ty == AssetType::SoundBank) {
            let Some(count) = bank.header.get(4..8).map(|b| le32(b, 0)) else {
                continue;
            };
            let Some(array) = bank.header.get(8..12).and_then(|b| decode_ptr(le32(b, 0))) else {
                continue;
            };
            for i in 0..count {
                let Ok(list) = load.blocks.bytes(array.at(i * SND_ALIAS_LIST), 20) else {
                    continue;
                };
                let Some(name) = decode_ptr(le32(list, 0))
                    .and_then(|p| load.blocks.cstr(p).ok())
                    .and_then(|b| std::str::from_utf8(b).ok())
                else {
                    continue;
                };
                if snd_hash_name(name) == le32(list, 4) {
                    names.insert(name.to_owned());
                }
            }
        }
    }
    names.into_iter().collect()
}
