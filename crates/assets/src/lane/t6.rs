mod common_compile;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use bevy::prelude::Image;

use super::{CommonCensus, LoadedWorld, MaterialPopulation, ZoneLane};
use crate::lane_capability::{LaneStatus, PreparedCapability};
use asset_core::ZoneGame;
use asset_transport::ZoneImage;
use asset_transport::progress::{LoadProgress, StageId};
use asset_world::WorldDrawPolicy;

struct T6ModelCapture {
    pub skel: asset_model::ModelSkel,
    pub surface_materials: Vec<Option<String>>,
    pub view: bool,
}

struct T6MaterialCapture {
    pub native: Option<T6NativeMaterial>,
}

struct T6NativeMaterial {
    pub header: Vec<u8>,
    pub technique_set: String,
    pub textures: Vec<asset_material::t6_techset::T6Texture>,
    pub constants: Vec<asset_material::MaterialConstant>,
    pub state: asset_material::t6_techset::T6MaterialState,
}

#[derive(Default)]
struct T6Content {
    pub path: std::path::PathBuf,
    pub models: Vec<T6ModelCapture>,
    pub materials: BTreeMap<String, T6MaterialCapture>,
    pub sound_names: std::collections::BTreeSet<String>,
    pub xanims: asset_anim::XAnimBuild,
    pub note_sounds: std::collections::BTreeSet<String>,
    pub hands: Option<String>,
    pub techsets: BTreeMap<String, asset_material::t6_techset::T6TechniqueSet>,
    pub melee: Option<asset_game::T6Melee>,
    pub fx: Vec<asset_game::T6FxCapture>,
    pub fx_materials: BTreeMap<String, T6MaterialCapture>,
    pub report: Vec<String>,
}

fn t6_script_model_instance(
    placement: &asset_world::ScriptModelPlacement,
    scene_assets: &mut asset_world::MapXModelSceneCatalog,
) -> asset_world::ScriptModelSceneInstance {
    use bevy::math::{EulerRot, Quat, Vec3};
    let current_model = asset_world::MapXModelAssetKey(placement.model.clone());
    if scene_assets.get(&current_model).is_none() {
        scene_assets.insert(
            current_model.clone(),
            asset_world::MapXModelSceneAsset::Unavailable {
                reason: "T6 script model XModel is not in the map zone",
            },
        );
    }
    let [pitch, yaw, roll] = placement.angles.map(f32::to_radians);
    asset_world::ScriptModelSceneInstance {
        id: placement.id,
        dobj_state: xmodel_runtime::DObjSemanticState::bind_pose(placement.model.clone(), 1, 1),
        current_model,
        transform: bevy::prelude::Transform {
            translation: Vec3::from_array(placement.origin),
            rotation: Quat::from_euler(EulerRot::ZYX, yaw, pitch, roll),
            scale: Vec3::ONE,
        },
        lighting_origin: placement.lighting_origin.unwrap_or(placement.origin),
        metadata: asset_world::ScriptModelMetadata {
            gameobject: placement.gameobject.clone(),
            targetname: placement.targetname.clone(),
            script_noteworthy: placement.script_noteworthy.clone(),
            destructible_type: placement.destructible_type.clone(),
            destructible_def: placement.destructible_def.clone(),
            t5_destructible: None,
            target: placement.target.clone(),
            script_exploder: placement.script_exploder.clone(),
            brush_link: placement.brush_link.clone(),
            script_accumulate: placement.script_accumulate,
            script_threshold: placement.script_threshold,
            script_destructable_area: placement.script_destructable_area.clone(),
            script_fxid: placement.script_fxid.clone(),
        },
    }
}

fn melee_weapon(load: &fastfile_t6::ZoneLoad) -> Option<asset_game::T6Melee> {
    use fastfile_t6::weapon::{WeaponView, def, weap_anim};
    let weapon = load
        .assets
        .iter()
        .filter_map(|asset| WeaponView::new(load, asset))
        .find(|weapon| weapon.name() == Some(asset_game::T6_MELEE_WEAPON))?;
    let clip = |slot: usize| {
        weapon
            .xanim(slot as u32)
            .filter(|name| !name.is_empty())
            .map(|name| {
                format!(
                    "{}{}",
                    asset_game::T6_XANIM_PREFIX,
                    name.to_ascii_lowercase()
                )
            })
    };
    Some(asset_game::T6Melee {
        knife: asset_game::t6_model_name(weapon.def_asset_array_name(def::GUN_XMODEL, 0)?),
        melee: clip(weap_anim::MELEE)?,
        charge: clip(weap_anim::MELEE_CHARGE),
    })
}

const COLOR_MAP_SAMPLER_HASH: u32 = 0xa0ab_1041;
const HANDS_ZONE: &str = "faction_seals_mp.ff";
const HANDS_MODEL: &str = "c_usa_mp_seal6_longsleeve_viewhands";

fn hands_zone(common: &Path, report: &mut Vec<String>) -> Option<fastfile_t6::ZoneLoad> {
    let path = common.with_file_name(HANDS_ZONE);
    let image = match asset_transport::open_t6_zone(&path) {
        Ok(image) => image,
        Err(error) => {
            report.push(format!("t6 hands: {}: {error:?}", path.display()));
            return None;
        }
    };
    let (load, walked) = fastfile_t6::load_zone(schema().ok()?, &image.bytes, |_, _| true);
    if let Err(error) = walked {
        report.push(format!("t6 hands: {} walk: {error:?}", path.display()));
    }
    Some(load)
}

fn xanim_notes<'z>(load: &'z fastfile_t6::ZoneLoad, header: &[u8]) -> Vec<&'z str> {
    let count = u32::from(header.get(34).copied().unwrap_or(0));
    let Some(raw) = header_u32(header, 96).filter(|&raw| raw != 0 && raw < 0xFFFF_FFFE) else {
        return Vec::new();
    };
    let e = raw - 1;
    let notify = fastfile_t6::Ptr {
        block: (e >> 29) as u8,
        offset: e & 0x1FFF_FFFF,
    };
    (0..count)
        .filter_map(|i| {
            let id = load.blocks.bytes(notify.at(8 * i), 2).ok()?;
            load.script_string(u16::from_le_bytes([id[0], id[1]]))
        })
        .collect()
}

fn capture_xanims(
    load: &fastfile_t6::ZoneLoad,
    others: &[fastfile_t6::ZoneLoad],
    content: &mut T6Content,
) {
    use fastfile_t6::weapon::WeaponView;
    let mut wanted = std::collections::BTreeSet::new();
    for asset in &load.assets {
        let Some(weapon) = WeaponView::new(load, asset) else {
            continue;
        };
        if weapon.name().is_some() {
            wanted.extend(asset_game::t6_weapon_xanim_names(weapon));
            wanted.extend(asset_game::t6_attachment_xanim_names(weapon));
        }
    }
    let (total, mut captured) = (wanted.len(), 0usize);
    let assets = std::iter::once(load)
        .chain(others)
        .flat_map(|load| load.assets.iter().map(move |asset| (load, asset)));
    for (load, asset) in assets {
        if asset.ty != fastfile_t6::AssetType::XAnimParts
            || !header_str(load, &asset.header, 0)
                .is_some_and(|name| wanted.remove(&name.to_ascii_lowercase()))
        {
            continue;
        }
        if content.xanims.capture_xanim_t6(
            asset_core::AssetNamespace::T6,
            asset_game::T6_XANIM_PREFIX,
            load,
            asset,
        ) {
            captured += 1;
            content.note_sounds.extend(
                xanim_notes(load, &asset.header)
                    .into_iter()
                    .filter_map(|note| {
                        asset_game::t5_inline_note_alias(note, asset_game::T5_NOTE_SOUND_PREFIX)
                    })
                    .map(str::to_owned),
            );
        }
    }
    content.report.push(format!(
        "t6 xanims: {captured} of {total} weapon animations captured; {} notetrack sounds",
        content.note_sounds.len()
    ));
}

fn capture_sounds(
    path: &Path,
    load: &fastfile_t6::ZoneLoad,
    claim: Option<asset_audio::ZoneSoundCapture>,
    content: &mut T6Content,
) {
    let mut names = std::collections::BTreeSet::new();
    for asset in &load.assets {
        let Some(weapon) = fastfile_t6::weapon::WeaponView::new(load, asset) else {
            continue;
        };
        if weapon.name().is_some() {
            names.extend(asset_game::t6_weapon_sound_names(weapon));
            names.extend(asset_game::t6_attachment_sound_names(weapon));
        }
    }
    names.extend(content.note_sounds.iter().cloned());
    names.extend(
        asset_game::T6_EQUIPMENT_SOUNDS
            .iter()
            .map(|&name| name.to_owned()),
    );
    let (banks, mut report) = asset_audio::t6_sound_banks(path);
    let foley = foley_zone(path, &mut report);
    let loads: Vec<&fastfile_t6::ZoneLoad> = std::iter::once(load).chain(&foley).collect();
    let (catalog, filled, gaps) =
        asset_audio::capture_t6_sounds(path, &loads, &banks, names.iter().map(String::as_str));
    report.push(format!(
        "t6 sounds: {} of {} weapon aliases read (+{} secondary layers, {} audio assets) from {} sound banks; {} gaps",
        filled.iter().filter(|name| names.contains(*name)).count(),
        names.len(),
        filled.iter().filter(|name| !names.contains(*name)).count(),
        catalog.loaded_sounds().len(),
        banks.len(),
        gaps.len()
    ));
    report.extend(gaps.into_iter().take(32));
    if let Some(mut capture) = claim {
        capture.set_t6(catalog);
        capture.deposit(Ok(()));
    } else {
        report.push("t6 sounds: common_mp sound source already claimed".to_owned());
    }
    content.sound_names = filled.into_iter().collect();
    content.report.extend(report);
}

fn patch_zone(common: &Path, report: &mut Vec<String>) -> Option<fastfile_t6::ZoneLoad> {
    let path = common.with_file_name("patch_mp.ff");
    let image = match asset_transport::open_t6_zone(&path) {
        Ok(image) => image,
        Err(error) => {
            report.push(format!("t6 content: {}: {error:?}", path.display()));
            return None;
        }
    };
    let (load, walked) = fastfile_t6::load_zone(schema().ok()?, &image.bytes, |_, _| true);
    if let Err(error) = walked {
        report.push(format!("t6 content: {} walk: {error:?}", path.display()));
    }
    Some(load)
}

fn foley_zone(common: &Path, report: &mut Vec<String>) -> Option<fastfile_t6::ZoneLoad> {
    let dir = common.parent()?;
    let smallest = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("mp_") && name.ends_with(".ff")
        })
        .filter_map(|entry| Some((entry.metadata().ok()?.len(), entry.path())))
        .min()
        .map(|(_, path)| path);
    let Some(path) = smallest else {
        report.push(format!(
            "t6 sounds: no map zone in {} for weapon foley",
            dir.display()
        ));
        return None;
    };
    let image = match asset_transport::open_t6_zone(&path) {
        Ok(image) => image,
        Err(error) => {
            report.push(format!("t6 sounds: {}: {error:?}", path.display()));
            return None;
        }
    };
    let (load, walked) = fastfile_t6::load_zone(schema().ok()?, &image.bytes, |_, ty| {
        ty != fastfile_t6::AssetType::SoundBank
    });
    if let Err(error) = walked {
        report.push(format!("t6 sounds: {} walk: {error:?}", path.display()));
    }
    report.push(format!("t6 sounds: weapon foley from {}", path.display()));
    Some(load)
}

pub struct T6Lane;

impl T6Lane {
    pub const GAME: ZoneGame = ZoneGame::T6;
    pub const CAPABILITIES: &'static [(PreparedCapability, LaneStatus)] = &[
        (PreparedCapability::Envelope, LaneStatus::SupportedPopulated),
        (
            PreparedCapability::PreparedWorld,
            LaneStatus::MissingDecoder,
        ),
        (
            PreparedCapability::CollisionSpawns,
            LaneStatus::MissingDecoder,
        ),
        (
            PreparedCapability::WeaponCatalog,
            LaneStatus::SupportedPopulated,
        ),
        (
            PreparedCapability::BodySkeleton,
            LaneStatus::SupportedPopulated,
        ),
        (
            PreparedCapability::PlayableFfa,
            LaneStatus::UnsupportedByRuntimeProfile,
        ),
    ];
}

fn schema() -> Result<&'static fastfile_t6::schema::Schema, String> {
    static SCHEMA: OnceLock<Result<fastfile_t6::schema::Schema, String>> = OnceLock::new();
    SCHEMA
        .get_or_init(|| fastfile_t6::schema::parse().map_err(|e| format!("T6 load plan: {e:?}")))
        .as_ref()
        .map_err(Clone::clone)
}

const COLOR_MAP_HASH: u32 = 0xa0ab_1041;
const MATERIAL_TEXTURE_COUNT: usize = 84;
const MATERIAL_TEXTURE_TABLE: usize = 96;
const MATERIAL_TEXTURE_DEF: u32 = 16;
const IMAGE_STREAMED_PART_HASH: usize = 40;
const IMAGE_STREAMED_PART_COUNT: usize = 60;
const IMAGE_NAME: usize = 72;
const IMAGE_HASH: usize = 76;

fn header_u32(header: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(header.get(at..at + 4)?.try_into().ok()?))
}

fn header_str<'z>(load: &'z fastfile_t6::ZoneLoad, header: &[u8], at: usize) -> Option<&'z str> {
    let raw = header_u32(header, at)?;
    if raw == 0 || raw >= 0xFFFF_FFFE {
        return None;
    }
    let e = raw - 1;
    let p = fastfile_t6::Ptr {
        block: (e >> 29) as u8,
        offset: e & 0x1FFF_FFFF,
    };
    core::str::from_utf8(load.blocks.cstr(p).ok()?).ok()
}

fn open_ipaks(zone: &Path, report: &mut Vec<String>) -> Vec<asset_transport::IPak> {
    let Some(dir) = zone.parent() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        report.push(format!("t6 content: cannot read {}", dir.display()));
        return Vec::new();
    };
    let mut paths: Vec<_> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ipak"))
        })
        .collect();
    paths.sort();
    paths
        .iter()
        .filter_map(|path| match asset_transport::IPak::open(path) {
            Ok(ipak) => Some(ipak),
            Err(error) => {
                report.push(format!("t6 content: {error}"));
                None
            }
        })
        .collect()
}

fn t6_sampler_state(state: u8) -> u8 {
    const FILTER_LINEAR: u8 = 2;
    let filter = state & 0b111;
    (state & !0b111) | filter.min(FILTER_LINEAR)
}

type DecodedTextures = BTreeMap<String, Arc<Image>>;

fn decode_ptr(raw: u32) -> Option<fastfile_t6::Ptr> {
    (raw != 0 && raw < 0xFFFF_FFFE).then(|| fastfile_t6::Ptr {
        block: ((raw - 1) >> 29) as u8,
        offset: (raw - 1) & 0x1FFF_FFFF,
    })
}

const MATERIAL_CONSTANT_COUNT: usize = 85;
const MATERIAL_TECHNIQUE_SET: u32 = 92;
const MATERIAL_CONSTANT_TABLE: usize = 100;
const MATERIAL_STATE_BITS_ENTRY: usize = 48;
const MATERIAL_STATE_BITS_TABLE: usize = 104;
const MATERIAL_STATE_BITS: u32 = 20;
const MATERIAL_CONSTANT_DEF: u32 = 32;
const TECHNIQUE_SET_WORLD_VERT_FORMAT: usize = 4;
const TECHNIQUE_SET_TECHNIQUES: usize = 8;
const TECHNIQUE_HEADER: u32 = 8;
const MATERIAL_PASS: u32 = 24;
const VERTEX_DECL_HEADER: usize = 36;
const SHADER_ARGUMENT: u32 = 12;

fn read_technique_set(
    load: &fastfile_t6::ZoneLoad,
    header: &[u8],
) -> Option<asset_material::t6_techset::T6TechniqueSet> {
    use asset_material::t6_techset::{
        T6Argument, T6Pass, T6Technique, T6TechniqueSet, argument_type,
    };
    let blocks = &load.blocks;
    let u16_le = |b: &[u8], at: usize| u16::from_le_bytes([b[at], b[at + 1]]);
    let shader = |pointer: Option<fastfile_t6::Ptr>| -> Option<(String, Vec<u8>)> {
        let header = blocks.bytes(pointer?, 16).ok()?;
        let program = decode_ptr(header_u32(header, 8)?)?;
        let size = header_u32(header, 12)? as usize;
        Some((
            header_str(load, header, 0).unwrap_or("").to_owned(),
            blocks.bytes(program, size).ok()?.to_vec(),
        ))
    };
    let techniques = (0..T6_TECHNIQUE_COUNT)
        .map(|index| {
            let technique = decode_ptr(header_u32(header, TECHNIQUE_SET_TECHNIQUES + 4 * index)?)?;
            let head = blocks.bytes(technique, TECHNIQUE_HEADER as usize).ok()?;
            let (flags, pass_count) = (u16_le(head, 4), u16_le(head, 6));
            let passes = (0..u32::from(pass_count))
                .map(|p| {
                    let pass = blocks
                        .bytes(
                            technique.at(TECHNIQUE_HEADER + MATERIAL_PASS * p),
                            MATERIAL_PASS as usize,
                        )
                        .ok()?;
                    let (vertex_name, vertex) = shader(decode_ptr(header_u32(pass, 4)?))?;
                    let (pixel_name, pixel) = shader(decode_ptr(header_u32(pass, 8)?))?;
                    let count = u32::from(pass[12]) + u32::from(pass[13]) + u32::from(pass[14]);
                    let table = decode_ptr(header_u32(pass, 20)?);
                    let arguments = (0..count)
                        .filter_map(|a| {
                            let raw = blocks.bytes(table?.at(SHADER_ARGUMENT * a), 12).ok()?;
                            let kind = u16_le(raw, 0);
                            let def = header_u32(raw, 8)?;
                            let literal = matches!(
                                kind,
                                argument_type::LITERAL_VERTEX_CONST
                                    | argument_type::LITERAL_PIXEL_CONST
                            )
                            .then(|| {
                                let words = blocks.bytes(decode_ptr(def)?, 16).ok()?;
                                Some(core::array::from_fn(|i| {
                                    header_u32(words, 4 * i).unwrap_or(0)
                                }))
                            })
                            .flatten();
                            Some(T6Argument {
                                kind,
                                offset: u16_le(raw, 2),
                                size: raw[4],
                                buffer: u16_le(raw, 6),
                                def,
                                literal,
                            })
                        })
                        .collect();
                    let layer_routing = match decode_ptr(header_u32(pass, 0)?) {
                        Some(decl) => {
                            let decl = blocks.bytes(decl, VERTEX_DECL_HEADER).ok()?;
                            decl[4..]
                                .as_chunks::<2>()
                                .0
                                .iter()
                                .take(usize::from(decl[0]))
                                .copied()
                                .filter(|[source, _]| (6..=9).contains(source))
                                .collect()
                        }
                        None => Vec::new(),
                    };
                    Some(T6Pass {
                        vertex_name,
                        vertex,
                        pixel_name,
                        pixel,
                        custom_sampler_flags: pass[15],
                        arguments,
                        layer_routing,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            Some(T6Technique { flags, passes })
        })
        .collect();
    Some(T6TechniqueSet {
        name: header_str(load, header, 0)?
            .trim_start_matches(',')
            .to_owned(),
        world_vert_format: header[TECHNIQUE_SET_WORLD_VERT_FORMAT],
        techniques,
    })
}

const T6_TECHNIQUE_COUNT: usize = 36;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ColourMapAlpha {
    Mask,
    Gloss,
}

#[allow(clippy::too_many_arguments)]
fn capture_native(
    load: &fastfile_t6::ZoneLoad,
    zones: &[&fastfile_t6::ZoneLoad],
    address: fastfile_t6::Ptr,
    material: &fastfile_t6::LoadedAsset,
    colour_alpha: ColourMapAlpha,
    ipaks: &[asset_transport::IPak],
    decoded: &mut DecodedTextures,
    techsets: &mut BTreeMap<String, asset_material::t6_techset::T6TechniqueSet>,
    report: &mut Vec<String>,
) -> Option<T6NativeMaterial> {
    let techset = material
        .field(MATERIAL_TECHNIQUE_SET)
        .map(|index| &load.assets[index])
        .or_else(|| load.asset_at(address.at(MATERIAL_TECHNIQUE_SET)))?;
    let technique_set = header_str(load, &techset.header, 0)?
        .trim_start_matches(',')
        .to_owned();
    if !techsets.contains_key(&technique_set) {
        let defined = |set: &asset_material::t6_techset::T6TechniqueSet| {
            set.techniques.iter().any(Option::is_some)
        };
        let read = read_technique_set(load, &techset.header)
            .filter(|set| defined(set))
            .or_else(|| {
                zones.iter().find_map(|zone| {
                    zone.assets
                        .iter()
                        .filter(|asset| asset.ty == fastfile_t6::AssetType::TechniqueSet)
                        .filter(|asset| {
                            header_str(zone, &asset.header, 0)
                                .is_some_and(|name| name.trim_start_matches(',') == technique_set)
                        })
                        .filter_map(|asset| read_technique_set(zone, &asset.header))
                        .find(|set| defined(set))
                })
            });
        match read {
            Some(set) => {
                techsets.insert(technique_set.clone(), set);
            }
            None => {
                report.push(format!(
                    "t6 native: {technique_set}: technique set unreadable"
                ));
                return None;
            }
        }
    }
    let header = &material.header;
    let entries = header
        .get(MATERIAL_STATE_BITS_ENTRY..MATERIAL_STATE_BITS_ENTRY + 36)?
        .try_into()
        .ok()?;
    let count = usize::from(*header.get(86)?);
    let mut rows = Vec::with_capacity(count);
    if count > 0 {
        let table = decode_ptr(header_u32(header, MATERIAL_STATE_BITS_TABLE)?)?;
        for row in 0..count {
            let bytes = load
                .blocks
                .bytes(table.at(u32::try_from(row).ok()? * MATERIAL_STATE_BITS), 8)
                .ok()?;
            rows.push([header_u32(bytes, 0)?, header_u32(bytes, 4)?]);
        }
    }
    let state = asset_material::t6_techset::T6MaterialState { entries, rows };
    let lit_state = state
        .first_bits(asset_material::t6_techset::T6_TECHNIQUE_LIT)
        .map(|bits| bits[0]);
    let opaque = colour_alpha == ColourMapAlpha::Mask
        && lit_state.is_some_and(|bits| {
            asset_material::MaterialDrawMode::from_state_bits([bits, 0])
                == asset_material::MaterialDrawMode::Opaque
        });
    let count = u32::from(*header.get(MATERIAL_TEXTURE_COUNT)?);
    let table = decode_ptr(header_u32(header, MATERIAL_TEXTURE_TABLE)?);
    let mut textures = Vec::new();
    for index in 0..count {
        let def = table?.at(index * MATERIAL_TEXTURE_DEF);
        let Ok(bytes) = load.blocks.bytes(def, MATERIAL_TEXTURE_DEF as usize) else {
            continue;
        };
        let (name_hash, sampler_state, semantic) = (header_u32(bytes, 0)?, bytes[6], bytes[7]);
        let Some(image) = load.asset_in(material, def.at(12)) else {
            continue;
        };
        let wanted = header_str(load, &image.header, IMAGE_NAME)
            .unwrap_or("")
            .trim_start_matches(',');
        let (image_load, image) = if image.header[4] != 0 {
            (load, image)
        } else {
            zones
                .iter()
                .find_map(|zone| {
                    zone.assets
                        .iter()
                        .find(|candidate| {
                            candidate.ty == fastfile_t6::AssetType::Image
                                && candidate.header[4] != 0
                                && header_str(zone, &candidate.header, IMAGE_NAME)
                                    .is_some_and(|name| name.trim_start_matches(',') == wanted)
                        })
                        .map(|image| (*zone, image))
                })
                .unwrap_or((load, image))
        };
        let image_name = header_str(image_load, &image.header, IMAGE_NAME)
            .unwrap_or("")
            .to_owned();
        let texels = match decoded.get(&image_name) {
            Some(texels) => texels.clone(),
            None => {
                let texels = decode_map_image(image_load, image, ipaks);
                match texels {
                    Ok(texels) => {
                        let texels = Arc::new(texels);
                        decoded.insert(image_name.clone(), texels.clone());
                        texels
                    }
                    Err(error) => {
                        report.push(format!("T6 material image {image_name}: {error}"));
                        continue;
                    }
                }
            }
        };
        let (image_name, texels) = if opaque && name_hash == COLOR_MAP_SAMPLER_HASH {
            let name = format!("{image_name}$opaque");
            let texels = decoded
                .entry(name.clone())
                .or_insert_with(|| Arc::new(asset_material::with_opaque_alpha(&texels)))
                .clone();
            (name, texels)
        } else {
            (image_name, texels)
        };
        textures.push(asset_material::t6_techset::T6Texture {
            name_hash,
            sampler_state: t6_sampler_state(sampler_state),
            semantic,
            image: image_name,
            texels,
        });
    }
    let constant_count = u32::from(*header.get(MATERIAL_CONSTANT_COUNT)?);
    let constant_table = decode_ptr(header_u32(header, MATERIAL_CONSTANT_TABLE)?);
    let constants = (0..constant_count)
        .filter_map(|index| {
            let bytes = load
                .blocks
                .bytes(
                    constant_table?.at(index * MATERIAL_CONSTANT_DEF),
                    MATERIAL_CONSTANT_DEF as usize,
                )
                .ok()?;
            Some(asset_material::MaterialConstant {
                name_hash: header_u32(bytes, 0)?,
                name: bytes[4..16].try_into().ok()?,
                literal: core::array::from_fn(|i| {
                    f32::from_bits(header_u32(bytes, 16 + 4 * i).unwrap_or(0))
                }),
            })
        })
        .collect();
    let constants: Vec<asset_material::MaterialConstant> = constants;
    Some(T6NativeMaterial {
        header: material.header.clone(),
        technique_set,
        textures,
        constants,
        state,
    })
}

fn read_streamed_image(
    load: &fastfile_t6::ZoneLoad,
    image: &fastfile_t6::LoadedAsset,
    ipaks: &[asset_transport::IPak],
) -> Result<(String, Vec<u8>), String> {
    let name = header_str(load, &image.header, IMAGE_NAME)
        .ok_or("image without a name")?
        .to_owned();
    if image
        .header
        .get(IMAGE_STREAMED_PART_COUNT)
        .copied()
        .unwrap_or(0)
        == 0
    {
        return Err(format!("{name}: not streamed"));
    }
    let hash = header_u32(&image.header, IMAGE_HASH).ok_or("short image")?;
    let part = header_u32(&image.header, IMAGE_STREAMED_PART_HASH).ok_or("short image")?;
    let bytes = ipaks
        .iter()
        .find_map(|ipak| ipak.read(hash, part))
        .ok_or_else(|| format!("{name}: in no image package"))??;
    Ok((name, bytes))
}

fn decode_map_image(
    load: &fastfile_t6::ZoneLoad,
    asset: &fastfile_t6::LoadedAsset,
    ipaks: &[asset_transport::IPak],
) -> Result<Image, String> {
    use bevy::asset::RenderAssetUsages;
    use bevy::image::ImageSampler;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    if asset.header[IMAGE_STREAMED_PART_COUNT] != 0 {
        let (_, bytes) = read_streamed_image(load, asset, ipaks)?;
        if asset.header[4] == 5 {
            return asset_material::decode_iwi_cubemap_native(&bytes, false);
        }
        return asset_material::decode_iwi_texture_native(&bytes, 2, false);
    }
    let source = asset
        .image_data
        .as_ref()
        .ok_or("T6 image has no pixel definition")?;
    let half = |at| u16::from_le_bytes(asset.header[at..at + 2].try_into().unwrap());
    let (width, height, depth) = (half(20), half(22), half(24));
    let name = header_str(load, &asset.header, IMAGE_NAME).unwrap_or("");
    if asset.header[4] == 5 {
        let format = match source.format {
            71 => u32::from_le_bytes(*b"DXT1"),
            77 => u32::from_le_bytes(*b"DXT5"),
            other => return Err(format!("T6 cubemap {name}: unsupported format {other}")),
        };
        return asset_material::decode_reflection_probe_cubemap(&asset_material::AuthoredImage {
            namespace: asset_core::AssetNamespace::T6,
            name: asset_core::AssetRef::Real(name.to_owned()),
            map_type: 5,
            semantic: asset.header[5],
            category: asset.header[6],
            use_srgb_reads: false,
            width,
            height,
            depth,
            level_count: source.level_count,
            format,
            payload: Arc::new(source.payload.clone()),
            decoded: None,
            common_owned: false,
            decoded_variant: None,
            decoded_by: None,
            pending_decode: None,
        });
    }
    if source.level_count != 1 || depth != 1 || width == 0 || height == 0 {
        return Err(format!("T6 image {name}: unsupported dimensions/mips"));
    }
    let (data, stride) = match source.format {
        28 => (source.payload.clone(), 4),
        65 => (
            source
                .payload
                .iter()
                .flat_map(|&alpha| [0, 0, 0, alpha])
                .collect(),
            4,
        ),
        other => return Err(format!("T6 image {name}: unsupported format {other}")),
    };
    if data.len() != usize::from(width) * usize::from(height) * stride {
        return Err(format!("T6 image {name}: pixel count mismatch"));
    }
    let mut image = Image::new(
        Extent3d {
            width: u32::from(width),
            height: u32::from(height),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    Ok(image)
}

const ICON_ZONES: [&str; 3] = ["code_post_gfx_mp.ff", "patch_ui_mp.ff", "ui_mp.ff"];

fn capture_weapon_icons(
    common: &Path,
    loads: &[&fastfile_t6::ZoneLoad],
    ipaks: &[asset_transport::IPak],
    report: &mut Vec<String>,
) -> Vec<(String, Arc<[u8]>)> {
    let mut wanted = std::collections::BTreeSet::new();
    for load in loads {
        for asset in &load.assets {
            if let Some(weapon) = fastfile_t6::weapon::WeaponView::new(load, asset) {
                let overlays = weapon
                    .attachment_uniques()
                    .filter_map(|unique| {
                        unique.asset_field_name(fastfile_t6::weapon::unique::OVERLAY_MATERIAL)
                    })
                    .chain(
                        weapon.variant_asset_name(fastfile_t6::weapon::variant::OVERLAY_MATERIAL),
                    );
                wanted.extend(overlays.map(str::to_ascii_lowercase));
                if let Some(icon) = weapon.def_loaded_asset_name(fastfile_t6::weapon::def::HUD_ICON)
                {
                    wanted.insert(asset_game::t6_model_name(icon).to_ascii_lowercase());
                }
            }
            let Some(table) = asset_game::capture_t6_string_table(load, asset) else {
                continue;
            };
            let attachments = table.name.eq_ignore_ascii_case("mp/attachmentTable.csv");
            if !attachments && !asset_game::is_stats_table_name(&table.name) {
                continue;
            }
            for row in 0..table.rows {
                let cell = |column: usize| {
                    table
                        .cells
                        .get(row * table.columns + column)
                        .map_or("", String::as_str)
                };
                if (attachments || cell(2).starts_with("weapon_")) && !cell(6).is_empty() {
                    wanted.insert(cell(6).to_ascii_lowercase());
                }
            }
        }
    }
    let extra: Vec<_> = ICON_ZONES
        .iter()
        .filter_map(|name| {
            let path = common.with_file_name(name);
            let image = match asset_transport::open_t6_zone(&path) {
                Ok(image) => image,
                Err(error) => {
                    report.push(format!("t6 icons: {}: {error:?}", path.display()));
                    return None;
                }
            };
            let (load, walked) = fastfile_t6::load_zone(schema().ok()?, &image.bytes, |_, _| true);
            if let Err(error) = walked {
                report.push(format!("t6 icons: {} walk: {error:?}", path.display()));
            }
            Some(load)
        })
        .collect();
    let mut icons = Vec::new();
    let mut failed = Vec::new();
    let all: Vec<&fastfile_t6::ZoneLoad> = loads.iter().copied().chain(&extra).collect();
    for load in loads.iter().copied().chain(&extra) {
        for asset in &load.assets {
            if asset.ty != fastfile_t6::AssetType::Material {
                continue;
            }
            let Some(name) = header_str(load, &asset.header, 0).map(str::to_ascii_lowercase) else {
                continue;
            };
            let presentation = name.starts_with("loadscreen_mp_")
                || name.starts_with("faction_")
                || (name.starts_with("menu_mp_") && name.ends_with("_map_select_final"));
            if !wanted.remove(&name) && !presentation {
                continue;
            }
            match capture_icon(load, asset, &all, ipaks) {
                Ok(iwi) => icons.push((name, iwi)),
                Err(error) => failed.push(format!("{name}: {error}")),
            }
        }
    }
    for load in &all {
        for asset in &load.assets {
            if asset.ty != fastfile_t6::AssetType::Image {
                continue;
            }
            let Some(name) = header_str(load, &asset.header, IMAGE_NAME)
                .map(str::to_ascii_lowercase)
                .filter(|name| wanted.contains(name))
            else {
                continue;
            };
            let decoded =
                read_streamed_image(load, asset, ipaks).and_then(|(_, bytes)| checked_iwi(bytes));
            match decoded {
                Ok(iwi) => {
                    wanted.remove(&name);
                    icons.push((name, iwi));
                }
                Err(error) => failed.push(format!("{name}: {error}")),
            }
        }
    }
    report.push(format!(
        "t6 icons: {} UI images decoded; {} failed {failed:?}; {} in no zone read {wanted:?}",
        icons.len(),
        failed.len(),
        wanted.len()
    ));
    icons
}

fn capture_icon(
    load: &fastfile_t6::ZoneLoad,
    material: &fastfile_t6::LoadedAsset,
    zones: &[&fastfile_t6::ZoneLoad],
    ipaks: &[asset_transport::IPak],
) -> Result<Arc<[u8]>, String> {
    let header = &material.header;
    let count = u32::from(*header.get(MATERIAL_TEXTURE_COUNT).ok_or("short material")?);
    let raw = header_u32(header, MATERIAL_TEXTURE_TABLE).ok_or("short material")?;
    if raw == 0 {
        return Err("no textures".to_owned());
    }
    let e = raw - 1;
    let table = fastfile_t6::Ptr {
        block: (e >> 29) as u8,
        offset: e & 0x1FFF_FFFF,
    };
    for index in 0..count {
        let def = table.at(index * MATERIAL_TEXTURE_DEF);
        if load.blocks.u32_at(def).ok() != Some(COLOR_MAP_HASH) {
            continue;
        }
        let image = load
            .asset_in(material, def.at(12))
            .ok_or("colour map without an image")?;
        let (name, bytes) = match read_streamed_image(load, image, ipaks) {
            Ok(read) => read,
            Err(error) => header_str(load, &image.header, IMAGE_NAME)
                .and_then(|name| name.strip_prefix(','))
                .and_then(|name| {
                    zones.iter().find_map(|zone| {
                        zone.assets
                            .iter()
                            .filter(|asset| asset.ty == fastfile_t6::AssetType::Image)
                            .filter(|asset| {
                                header_str(zone, &asset.header, IMAGE_NAME) == Some(name)
                            })
                            .find_map(|asset| read_streamed_image(zone, asset, ipaks).ok())
                    })
                })
                .ok_or(error)?,
        };
        return checked_iwi(bytes).map_err(|e| format!("{name}: {e}"));
    }
    Err("no colour map".to_owned())
}

fn checked_iwi(bytes: Vec<u8>) -> Result<Arc<[u8]>, String> {
    asset_material::decode_iwi_rgba(&bytes)?;
    Ok(bytes.into())
}

fn attachment_rest(offset: [f32; 3], angles: [f32; 3]) -> (bevy::math::Quat, bevy::math::Vec3) {
    let [pitch, yaw, roll] = angles.map(f32::to_radians);
    (
        bevy::math::Quat::from_euler(bevy::math::EulerRot::ZYX, yaw, pitch, roll),
        bevy::math::Vec3::from_array(offset),
    )
}

fn capture_content(
    path: &Path,
    load: &fastfile_t6::ZoneLoad,
    others: &[fastfile_t6::ZoneLoad],
    ipaks: &[asset_transport::IPak],
) -> T6Content {
    use fastfile_t6::weapon::{WeaponView, def};
    let mut content = T6Content {
        path: path.to_owned(),
        ..Default::default()
    };
    let zones: Vec<&fastfile_t6::ZoneLoad> = std::iter::once(load).chain(others).collect();
    let mut models: BTreeMap<&str, (&fastfile_t6::ZoneLoad, &fastfile_t6::LoadedAsset)> =
        BTreeMap::new();
    for load in std::iter::once(load).chain(others) {
        for asset in &load.assets {
            if let Some(name) = asset_model::T6Model::new(load, asset).and_then(|m| m.name())
                && !name.starts_with(',')
            {
                models.entry(name).or_insert((load, asset));
            }
        }
    }
    let mut wanted: BTreeMap<String, (bool, bool)> = BTreeMap::new();
    let mut placements: BTreeMap<String, ([f32; 3], [f32; 3])> = BTreeMap::new();
    let mut copies: BTreeMap<String, (String, Option<String>)> = BTreeMap::new();
    for asset in &load.assets {
        let Some(weapon) = WeaponView::new(load, asset) else {
            continue;
        };
        if weapon.name().is_none() {
            continue;
        }

        for (name, view) in [
            (weapon.def_asset_array_name(def::GUN_XMODEL, 0), true),
            (weapon.def_asset_array_name(def::WORLD_MODEL, 0), false),
            (weapon.def_asset_name(def::PROJECTILE_MODEL), false),
            (weapon.name().and_then(asset_game::t6_planted_model), false),
        ] {
            if let Some(name) = name {
                wanted
                    .entry(asset_game::t6_model_name(name))
                    .or_insert((view, false));
            }
        }
        for unique in weapon.attachment_uniques() {
            let placed = [true, false]
                .into_iter()
                .flat_map(|view| {
                    asset_game::t6_attachment_models(unique, view)
                        .into_iter()
                        .map(move |placed| (placed, view))
                })
                .chain(asset_game::t6_attachment_ads_model(unique).map(|placed| (placed, true)));
            for (placed, view) in placed {
                placements.insert(placed.copy.clone(), (placed.offset, placed.angles));
                wanted.entry(placed.copy.clone()).or_insert((view, false));
                copies.insert(placed.copy, (placed.model, placed.tag));
            }
        }
        for view in [true, false] {
            for slot in 0..fastfile_t6::weapon::variant::ATTACH_MODEL_COUNT {
                if let Some((name, offset, angles)) = weapon.attached_model(slot, view) {
                    let name = asset_game::t6_model_name(name);
                    placements.entry(name.clone()).or_insert((offset, angles));
                    wanted.entry(name).or_insert((view, false));
                }
            }
        }
    }
    if !wanted.is_empty() {
        wanted.insert(HANDS_MODEL.to_owned(), (true, true));
        if let Some(knife) = melee_weapon(load).map(|melee| melee.knife) {
            wanted.entry(knife).or_insert((true, false));
        }
    }
    content.melee = melee_weapon(load);
    let (mut failed, mut decoded, mut missing) = (0usize, 0usize, 0usize);
    let mut native_textures = DecodedTextures::new();
    for (name, (view, hands)) in wanted {
        let copy = copies.get(&name);
        let source = copy.map_or(name.as_str(), |(model, _)| model.as_str());
        let Some(&(load, asset)) = models.get(source) else {
            content
                .report
                .push(format!("t6 content: {name}: in no zone read"));
            continue;
        };
        let Some(model) = asset_model::T6Model::new(load, asset) else {
            continue;
        };
        let Some(mut skel) = asset_model::capture_model_skel_t6(model, |_| None) else {
            failed += 1;
            content
                .report
                .push(format!("t6 content: {name}: model capture failed"));
            continue;
        };
        if let Some(&(offset, angles)) = placements.get(&name)
            && let Some(pose) = skel.pose.as_mut()
        {
            pose.root_rest = Some(attachment_rest(offset, angles));
        }
        if let Some((_, tag)) = copy {
            skel.name = name.clone();
            skel.mount_tag = tag.clone();
        }
        let mut surface_materials = Vec::with_capacity(model.surface_count());
        for surface in 0..model.surface_count() {
            let material = model
                .material_slot(surface)
                .and_then(|slot| load.asset_in(asset, slot));
            let material_name = material
                .and_then(|m| header_str(load, &m.header, 0))
                .map(str::to_owned);
            if let (Some(material), Some(material_name)) = (material, &material_name)
                && !content.materials.contains_key(material_name)
            {
                let reported = content.report.len();
                let native = model
                    .material_slot(surface)
                    .and_then(|slot| load.blocks.ptr_at(slot).ok().flatten())
                    .and_then(|address| {
                        capture_native(
                            load,
                            &zones,
                            address,
                            material,
                            ColourMapAlpha::Mask,
                            ipaks,
                            &mut native_textures,
                            &mut content.techsets,
                            &mut content.report,
                        )
                    });
                missing += content.report.len() - reported;
                decoded += native.as_ref().map_or(0, |capture| capture.textures.len());
                content
                    .materials
                    .insert(material_name.clone(), T6MaterialCapture { native });
            }
            surface_materials.push(material_name);
        }
        if hands {
            content.hands = Some(name.clone());
        }
        content.models.push(T6ModelCapture {
            skel,
            surface_materials,
            view,
        });
    }
    content.report.push(format!(
        "t6 content: {} models ({failed} failed), {} materials, {decoded} native textures decoded, {missing} missing; {} image packages",
        content.models.len(),
        content.materials.len(),
        ipaks.len()
    ));
    capture_effects(&zones, ipaks, &mut content);
    content
}

const FX_ELEM_DEF: u32 = 292;
const FX_ELEM_TYPE: usize = 184;
const FX_ELEM_VEL_SAMPLES: usize = 188;
const FX_ELEM_VIS_SAMPLES: usize = 192;
const FX_ELEM_VISUALS: u32 = 196;
const FX_ELEM_CHILDREN: [usize; 3] = [224, 228, 232];
const FX_ELEM_VEL_SAMPLE: usize = 96;
const FX_ELEM_VIS_SAMPLE: usize = 48;
const FX_EFFECT_DEF_ELEMS: usize = 28;
const FX_ELEM_LAST_SPRITE: u8 = 6;
const FX_ELEM_MODEL: u8 = 7;
const FX_ELEM_SOUND: u8 = 10;
const FX_ELEM_RUNNER: u8 = 12;

fn capture_effects(
    zones: &[&fastfile_t6::ZoneLoad],
    ipaks: &[asset_transport::IPak],
    content: &mut T6Content,
) {
    let mut textures = DecodedTextures::new();
    let mut wanted: std::collections::BTreeSet<String> = asset_game::T6_EFFECTS
        .iter()
        .map(|&name| name.to_owned())
        .collect();
    loop {
        let before = content.fx.len();
        for &load in zones {
            for asset in &load.assets {
                if asset.ty != fastfile_t6::AssetType::Fx {
                    continue;
                }
                let Some(name) = header_str(load, &asset.header, 0) else {
                    continue;
                };
                if !wanted.contains(name) || content.fx.iter().any(|fx| fx.name == name) {
                    continue;
                }
                match capture_effect(load, zones, asset, name, ipaks, &mut textures, content) {
                    Some(fx) => content.fx.push(fx),
                    None => content
                        .report
                        .push(format!("t6 effects: {name}: unreadable")),
                }
            }
        }
        for fx in &content.fx[before..] {
            for elem in &fx.elems {
                if elem.raw[FX_ELEM_TYPE] == FX_ELEM_RUNNER {
                    wanted.extend(elem.visuals.iter().filter(|v| !v.is_empty()).cloned());
                }
                for child in [
                    &elem.effect_on_impact,
                    &elem.effect_on_death,
                    &elem.effect_emitted,
                ] {
                    if !child.is_empty() {
                        wanted.insert(child.clone());
                    }
                }
            }
        }
        if content.fx.len() == before {
            break;
        }
    }
    let missing: Vec<_> = wanted
        .iter()
        .filter(|name| !content.fx.iter().any(|fx| fx.name == **name))
        .collect();
    content.report.push(format!(
        "t6 effects: {} captured, {} materials; in no zone read: {missing:?}",
        content.fx.len(),
        content.fx_materials.len()
    ));
}

fn capture_effect(
    load: &fastfile_t6::ZoneLoad,
    zones: &[&fastfile_t6::ZoneLoad],
    asset: &fastfile_t6::LoadedAsset,
    name: &str,
    ipaks: &[asset_transport::IPak],
    textures: &mut DecodedTextures,
    content: &mut T6Content,
) -> Option<asset_game::T6FxCapture> {
    let header = &asset.header;
    let count = |at: usize| u16::from_le_bytes([header[at], header[at + 1]]) as u32;
    let elem_count = count(8) + count(10) + count(12);
    let mut fx = asset_game::T6FxCapture {
        name: name.to_owned(),
        header: header.clone(),
        elems: Vec::new(),
    };
    if elem_count == 0 {
        return Some(fx);
    }
    let elems = decode_ptr(header_u32(header, FX_EFFECT_DEF_ELEMS)?)?;
    let blocks = &load.blocks;
    for i in 0..elem_count {
        let at = elems.at(i * FX_ELEM_DEF);
        let raw = blocks.bytes(at, FX_ELEM_DEF as usize).ok()?.to_vec();
        let elem_type = raw[FX_ELEM_TYPE];
        let visual_count = usize::from(raw[FX_ELEM_TYPE + 1]);
        let samples = |field: usize, n: usize, stride: usize| -> Vec<u8> {
            decode_ptr(header_u32(&raw, field).unwrap_or(0))
                .and_then(|p| blocks.bytes(p, n * stride).ok())
                .map_or_else(Vec::new, <[u8]>::to_vec)
        };
        let vel_samples = samples(
            FX_ELEM_VEL_SAMPLES,
            usize::from(raw[FX_ELEM_TYPE + 2]) + 1,
            FX_ELEM_VEL_SAMPLE,
        );
        let vis_samples = samples(
            FX_ELEM_VIS_SAMPLES,
            usize::from(raw[FX_ELEM_TYPE + 3]) + 1,
            FX_ELEM_VIS_SAMPLE,
        );
        let slots: Vec<fastfile_t6::Ptr> = if visual_count > 1 {
            decode_ptr(header_u32(&raw, FX_ELEM_VISUALS as usize).unwrap_or(0))
                .map_or_else(Vec::new, |arr| {
                    (0..visual_count as u32).map(|v| arr.at(v * 4)).collect()
                })
        } else if visual_count == 1 {
            vec![at.at(FX_ELEM_VISUALS)]
        } else {
            Vec::new()
        };
        let mut visuals = Vec::new();
        for slot in slots {
            let visual = match elem_type {
                0..=FX_ELEM_LAST_SPRITE => {
                    let material = load.asset_in(asset, slot);
                    let name = material.and_then(|m| header_str(load, &m.header, 0));
                    if let (Some(material), Some(name)) = (material, name)
                        && !content.fx_materials.contains_key(name)
                    {
                        let native = load.blocks.ptr_at(slot).ok().flatten().and_then(|address| {
                            capture_native(
                                load,
                                zones,
                                address,
                                material,
                                ColourMapAlpha::Mask,
                                ipaks,
                                textures,
                                &mut content.techsets,
                                &mut content.report,
                            )
                        });
                        content
                            .fx_materials
                            .insert(name.to_owned(), T6MaterialCapture { native });
                    }
                    name.map(str::to_owned)
                }
                FX_ELEM_MODEL => load
                    .asset_in(asset, slot)
                    .and_then(|m| header_str(load, &m.header, 0))
                    .map(asset_game::t6_model_name),
                FX_ELEM_SOUND | FX_ELEM_RUNNER => blocks
                    .bytes(slot, 4)
                    .ok()
                    .and_then(|b| header_str(load, b, 0))
                    .map(str::to_owned),
                _ => None,
            };
            visuals.push(visual.unwrap_or_else(String::new));
        }
        let child =
            |field: usize| header_str(load, &raw, field).map_or_else(String::new, str::to_owned);
        fx.elems.push(asset_game::T6FxElemCapture {
            vel_samples,
            vis_samples,
            visuals,
            effect_on_impact: child(FX_ELEM_CHILDREN[0]),
            effect_on_death: child(FX_ELEM_CHILDREN[1]),
            effect_emitted: child(FX_ELEM_CHILDREN[2]),
            raw,
        });
    }
    Some(fx)
}

fn native_material_seed(
    path: &Path,
    name: &str,
    material: &fastfile_t6::LoadedAsset,
    technique_set: &str,
    is_sky: bool,
    materials: &mut asset_material::MaterialCatalog,
) -> Result<usize, String> {
    let seed = materials.link_material(native_material_definition(
        path,
        name,
        &material.header,
        technique_set,
        is_sky,
    )?);
    Ok(seed)
}

fn native_material_definition(
    path: &Path,
    name: &str,
    h: &[u8],
    technique_set: &str,
    is_sky: bool,
) -> Result<asset_material::AuthoredMaterial, String> {
    use asset_world::world_t6::Reader;
    if h.len() < 89 {
        return Err(format!("T6 material {name}: truncated header"));
    }
    Ok(asset_material::AuthoredMaterial {
        name: asset_core::AssetRef::Real(name.to_owned()),
        namespace: asset_core::AssetNamespace::T6,
        technique_set: asset_core::AssetRef::Real(
            asset_material::t6_techset::T6Draw::Lit.technique_set_name(technique_set),
        ),
        technique_set_edge: Default::default(),
        draw_surf: u64::from_le_bytes(h[16..24].try_into().unwrap()),
        sort_key: if is_sky {
            asset_iw4::SORT_KEY_SKYBOX
        } else {
            h[9]
        },
        info_game_flags: h[4],
        texture_atlas: Some([h[10], h[11]]),
        surface_type_bits: Some(Reader::word(h, 24)?),
        t5_layered_surface_types: Some(Reader::word(h, 28)?),
        state_flags: h[87],
        camera_region: if is_sky {
            asset_iw4::CAMERA_REGION_LIT_OPAQUE
        } else {
            camera_region(h[88])
        },
        state_bits: Vec::new(),
        state_bits_entry: None,
        t5_state_bits_entry: None,
        iw5_state_bits_entry: None,
        technique_table: None,
        route: None,
        textures: Vec::new(),
        constants: Vec::new(),
        zone: asset_core::ZoneOwner::from_zone_path(path),
    })
}

fn camera_region(t6: u8) -> u8 {
    match t6 {
        0 | 2 | 6 => asset_iw4::CAMERA_REGION_LIT_OPAQUE,
        1 => asset_iw4::CAMERA_REGION_LIT_TRANS,
        3..=5 => asset_iw4::CAMERA_REGION_EMISSIVE,
        7 => asset_iw4::CAMERA_REGION_DEPTH_HACK,
        _ => asset_iw4::CAMERA_REGION_NONE,
    }
}

fn capture_bodies(
    path: &Path,
    factions: &[fastfile_t6::ZoneLoad],
    shared: &[fastfile_t6::ZoneLoad],
    ipaks: &[asset_transport::IPak],
    kits: asset_model::SoldierKits,
    materials: &mut asset_material::MaterialCatalog,
    report: &mut Vec<String>,
) -> Result<asset_model::BodyMeshBuild, String> {
    use asset_core::{AssetNamespace, WalkLocalMaterialIndex};
    use asset_material::t6_techset::T6Draw;
    let mut bodies = asset_model::BodyMeshBuild::default();
    let zones: Vec<_> = factions.iter().chain(shared).collect();
    let mut decoded = DecodedTextures::new();
    let mut techsets = BTreeMap::new();
    let mut bound = BTreeMap::new();
    for kit in [kits.allies.as_ref(), kits.axis.as_ref()]
        .into_iter()
        .flatten()
    {
        let name = &kit.body;
        if bodies.get(name).is_some() {
            continue;
        }
        let (load, asset, model) = factions
            .iter()
            .find_map(|load| {
                load.assets.iter().find_map(|asset| {
                    let model = asset_model::T6Model::new(load, asset)?;
                    (model.name() == Some(name.as_str())).then_some((load, asset, model))
                })
            })
            .ok_or_else(|| format!("T6 body {name}: model missing"))?;
        let mut rows = Vec::with_capacity(model.surface_count());
        for surface in 0..model.surface_count() {
            let slot = model
                .material_slot(surface)
                .ok_or("T6 body material slot missing")?;
            let material = load
                .asset_in(asset, slot)
                .ok_or("T6 body material missing")?;
            let raw_name =
                header_str(load, &material.header, 0).ok_or("T6 body material name missing")?;
            let material_name = asset_core::AssetRef::bare_name(raw_name);
            if let Some(&row) = bound.get(material_name) {
                rows.push(Some(row));
                continue;
            }
            let address = load
                .blocks
                .ptr_at(slot)
                .map_err(|e| format!("T6 body material pointer: {e:?}"))?
                .ok_or("T6 body material null")?;
            let (material_load, material) = if raw_name.starts_with(',') {
                zones
                    .iter()
                    .find_map(|zone| {
                        zone.assets
                            .iter()
                            .find(|asset| {
                                asset.ty == fastfile_t6::AssetType::Material
                                    && header_str(zone, &asset.header, 0) == Some(material_name)
                            })
                            .map(|asset| (*zone, asset))
                    })
                    .ok_or_else(|| {
                        format!("T6 body material {material_name}: definition missing")
                    })?
            } else {
                (load, material)
            };
            let native = capture_native(
                material_load,
                &zones,
                address,
                material,
                ColourMapAlpha::Mask,
                ipaks,
                &mut decoded,
                &mut techsets,
                report,
            )
            .ok_or_else(|| format!("T6 body material {material_name}: native capture failed"))?;
            let set = &techsets[&native.technique_set];
            materials.link_t6_technique_set(set, T6Draw::Lit, report);
            let seed =
                native_material_seed(path, material_name, material, &set.name, false, materials)?;
            let row = materials
                .t6_material(
                    seed,
                    material_name,
                    set,
                    &native.textures,
                    native.constants,
                    &native.state,
                    T6Draw::Lit,
                    report,
                )
                .map_err(|e| format!("T6 body material link failed: {e:?}"))?;
            let row = WalkLocalMaterialIndex::from_walk(row);
            bound.insert(material_name.to_owned(), row);
            rows.push(Some(row));
        }
        let skel = asset_model::capture_model_skel_t6(model, |surface| rows[surface])
            .ok_or_else(|| format!("T6 body {name}: skeleton capture failed"))?;
        report.push(format!(
            "T6 body {name}: {} bones, {} vertices, {} surfaces",
            skel.bones.len(),
            skel.positions.len(),
            skel.surface_materials.len()
        ));
        bodies.insert_in(AssetNamespace::T6, skel, Some(materials));
    }
    bodies.set_kits(kits);
    materials.resolve_technique_set_edges();
    Ok(bodies)
}

fn map_teams(
    path: &Path,
    report: &mut Vec<String>,
) -> Result<
    (
        asset_game::MapTeamSettings,
        Vec<fastfile_t6::ZoneLoad>,
        asset_model::SoldierKits,
    ),
    String,
> {
    use asset_core::{AssetKey, AssetKind, AssetNamespace};
    let patch = patch_zone(path, report).ok_or("T6 map table zone missing")?;
    let table = patch
        .assets
        .iter()
        .filter_map(|a| asset_game::capture_t6_string_table(&patch, a))
        .find(|t| t.name == "mp/mapstable.csv");
    let table = table.ok_or("T6 map faction table missing")?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("T6 map name missing")?;
    let row = (0..table.rows)
        .find(|&r| table.cell(r as i32, 0) == stem)
        .ok_or("T6 map absent from faction table")?;
    let mut settings = asset_game::MapTeamSettings::default();
    let mut kits = asset_model::SoldierKits::default();
    let mut loads = Vec::new();
    for (index, column) in [1, 2].into_iter().enumerate() {
        let faction = table.cell(row as i32, column);
        let faction_path = path.with_file_name(format!("faction_{faction}_mp.ff"));
        let image = asset_transport::open_t6_zone(&faction_path)
            .map_err(|e| format!("T6 faction {faction}: {e:?}"))?;
        let (load, walk) = fastfile_t6::load_zone(schema()?, &image.bytes, |_, _| true);
        walk.map_err(|e| format!("T6 faction {faction}: {e:?}"))?;
        let script_name = format!("maps/mp/teams/_teamset_{faction}.gsc");
        let script = load
            .assets
            .iter()
            .find(|a| {
                a.ty == fastfile_t6::AssetType::Script
                    && header_str(&load, &a.header, 0) == Some(script_name.as_str())
            })
            .ok_or("T6 faction script missing")?;
        let data = load
            .blocks
            .bytes(
                decode_ptr(header_u32(&script.header, 8).ok_or("T6 faction script pointer")?)
                    .ok_or("T6 faction script null")?,
                header_u32(&script.header, 4).ok_or("T6 faction script length")? as usize,
            )
            .map_err(|e| format!("T6 faction script: {e:?}"))?;
        let properties =
            asset_game::t6_team_properties(data).ok_or("T6 faction properties unreadable")?;
        let value = |key: &str| properties.get(key).cloned();
        let key = |name, kind| {
            value(name).and_then(|name| AssetKey::new(AssetNamespace::T6, kind, name).ok())
        };
        let color = value("g_TeamColor_").and_then(|s| {
            let values: Vec<f32> = s
                .split_whitespace()
                .filter_map(|v| v.parse().ok())
                .collect();
            values.try_into().ok()
        });
        let icon = key("icons", AssetKind::Material);
        let name = key("g_TeamName_", AssetKind::Localize);
        let strings = asset_game::FactionStrings {
            name: value("strings/_name"),
            eliminated: value("strings/_eliminated"),
            forfeited: value("strings/_forfeited"),
        };
        let music = asset_game::TeamMusic {
            spawn: value("music/spawn_").map(|s| format!("mus_{}", s.to_ascii_lowercase())),
            victory: value("music/victory_").map(|s| format!("mus_{}", s.to_ascii_lowercase())),
            defeat: Some("mus_loss".into()),
            ..Default::default()
        };
        if index == 0 {
            settings.allies = icon;
            settings.allies_name = name;
            settings.allies_color = color;
            settings.allies_strings = strings;
            settings.allies_voice = value("voice");
            settings.allies_music = music;
        } else {
            settings.axis = icon;
            settings.axis_name = name;
            settings.axis_color = color;
            settings.axis_strings = strings;
            settings.axis_voice = value("voice");
            settings.axis_music = music;
        }
        let team = if index == 0 { "allies" } else { "axis" };
        if properties.contains_key("attackers") {
            settings.attackers = Some(team.into());
        }
        if properties.contains_key("defenders") {
            settings.defenders = Some(team.into());
        }
        report.push(format!(
            "T6 faction {team}: {faction}, voice={:?}, icon={:?}",
            value("voice"),
            value("icons")
        ));
        let names: Vec<String> = load
            .assets
            .iter()
            .filter_map(|asset| {
                asset_model::T6Model::new(&load, asset)?
                    .name()
                    .map(str::to_owned)
            })
            .collect();
        let mut faction_kits = asset_model::soldier_kits(&names);
        let kit = faction_kits
            .allies
            .take()
            .or(faction_kits.axis.take())
            .ok_or_else(|| format!("T6 faction {faction}: soldier model missing"))?;
        if index == 0 {
            kits.allies = Some(kit);
        } else {
            kits.axis = Some(kit);
        }
        loads.push(load);
        if let Some(zone) = path.parent().and_then(Path::parent) {
            let localized = zone
                .join("english")
                .join(format!("en_faction_{faction}_mp.ff"));
            if localized.exists() {
                let image = asset_transport::open_t6_zone(&localized)
                    .map_err(|e| format!("T6 localized faction: {e:?}"))?;
                let (load, walk) = fastfile_t6::load_zone(schema()?, &image.bytes, |_, _| true);
                walk.map_err(|e| format!("T6 localized faction: {e:?}"))?;
                loads.push(load);
            }
        }
    }
    Ok((settings, loads, kits))
}

impl ZoneLane for T6Lane {
    fn game(&self) -> ZoneGame {
        Self::GAME
    }

    fn capabilities(&self) -> &'static [(PreparedCapability, LaneStatus)] {
        Self::CAPABILITIES
    }

    fn load_world(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,
        _shared_surfaces: asset_model::SharedXModelSurfaces,
        material_seed: asset_material::MaterialCatalog,
        _common_film_visions: &super::FilmVisionCatalog,
    ) -> LoadedWorld {
        let stage = progress.begin_scoped(StageId::MapAssets, "t6_world", None);
        let result = (|| -> Result<LoadedWorld, String> {
            use asset_core::AssetNamespace;
            use asset_material::t6_techset::T6Draw;
            use asset_world::world_t6::{
                Reader, build_clip_collision, build_world_draw, entity_string,
            };
            let (load, walked) = fastfile_t6::load_zone(schema()?, &image.bytes, |_, _| true);
            walked.map_err(|e| format!("T6 map walk: {e:?}"))?;
            let mut report = vec![format!("T6 map: {} assets", load.assets.len())];
            let world_asset = load
                .assets
                .iter()
                .find(|a| a.ty == fastfile_t6::AssetType::GfxWorld)
                .ok_or("T6 GfxWorld missing")?;
            let view = fastfile_t6::world::WorldView::new(&load, world_asset)
                .map_err(|e| format!("T6 GfxWorld: {e:?}"))?;
            let ipaks = open_ipaks(path, &mut report);
            let shared_loads = ["code_post_gfx_mp.ff", "common_mp.ff"]
                .into_iter()
                .map(|name| {
                    let image = asset_transport::open_t6_zone(path.with_file_name(name))
                        .map_err(|error| format!("T6 shared image zone {name}: {error:?}"))?;
                    let (load, result) =
                        fastfile_t6::load_zone(schema()?, &image.bytes, |_, _| true);
                    result.map_err(|error| format!("T6 shared image zone {name}: {error:?}"))?;
                    Ok(load)
                })
                .collect::<Result<Vec<_>, String>>()?;
            let image_loads: Vec<_> = std::iter::once(&load).chain(&shared_loads).collect();
            let mut materials = material_seed;
            let mut decoded = DecodedTextures::new();
            let mut techsets = BTreeMap::new();
            let mut material_rows = BTreeMap::new();
            let mut layer_formats = BTreeMap::new();
            let sky_name = header_str(&load, &world_asset.header, 36);
            let sky_model = sky_name
                .and_then(|name| {
                    load.assets.iter().find(|asset| {
                        asset.ty == fastfile_t6::AssetType::XModel
                            && header_str(&load, &asset.header, 0) == Some(name)
                    })
                })
                .and_then(|asset| asset_model::T6Model::new(&load, asset));
            let world_materials = view
                .surfaces()
                .map(|surface| surface.material().ok_or("T6 surface material missing"))
                .collect::<Result<Vec<_>, _>>()?;
            let mut sky_materials = Vec::new();
            if let Some(model) = sky_model {
                for surface in 0..model.surface_count() {
                    let material = model
                        .material_slot(surface)
                        .and_then(|slot| load.asset_at(slot))
                        .ok_or("T6 sky material missing")?;
                    sky_materials.push(material);
                }
            }
            let smodel_slots = match Reader::word(&world_asset.header, 784)? {
                0 => Vec::new(),
                count => {
                    let rows = Reader::ptr(&world_asset.header, 876)?;
                    (0..count).map(|i| rows.at(i * 152 + 56)).collect()
                }
            };
            let mut smodels: Vec<(&fastfile_t6::LoadedAsset, asset_model::T6Model)> = Vec::new();
            for &slot in &smodel_slots {
                let Some(asset) = load.asset_in(world_asset, slot) else {
                    continue;
                };
                if smodels.iter().any(|(seen, _)| std::ptr::eq(*seen, asset)) {
                    continue;
                }
                if let Some(model) = asset_model::T6Model::new(&load, asset) {
                    smodels.push((asset, model));
                }
            }
            let script_placements =
                asset_world::parse_script_model_placements(entity_string(&load)?);
            let mut script_xmodels: Vec<asset_model::T6Model> = Vec::new();
            for asset in &load.assets {
                let Some(model) = asset_model::T6Model::new(&load, asset) else {
                    continue;
                };
                let Some(name) = model.name() else {
                    continue;
                };
                if script_placements.iter().any(|p| p.model == name)
                    && !script_xmodels.iter().any(|seen| seen.name() == Some(name))
                {
                    script_xmodels.push(model);
                }
            }
            let smodel_materials: Vec<_> = smodels
                .iter()
                .map(|(_, model)| model)
                .chain(&script_xmodels)
                .flat_map(|model| {
                    (0..model.surface_count())
                        .filter_map(|surface| load.asset_at(model.material_slot(surface)?))
                })
                .collect();
            let mut surface_materials = Vec::with_capacity(world_materials.len());
            let mut surface_layer_formats = Vec::with_capacity(world_materials.len());
            for material in world_materials
                .iter()
                .chain(&sky_materials)
                .chain(&smodel_materials)
            {
                let material = *material;
                let name = header_str(&load, &material.header, 0)
                    .ok_or("T6 material name missing")?
                    .to_owned();
                let row = if let Some(row) = material_rows.get(&name) {
                    *row
                } else {
                    let native = capture_native(
                        &load,
                        &image_loads,
                        fastfile_t6::Ptr {
                            block: 0,
                            offset: 0,
                        },
                        material,
                        ColourMapAlpha::Gloss,
                        &ipaks,
                        &mut decoded,
                        &mut techsets,
                        &mut report,
                    )
                    .ok_or_else(|| format!("T6 material {name}: native capture failed"))?;
                    let set = &techsets[&native.technique_set];
                    materials.link_t6_technique_set(set, T6Draw::Lit, &mut report);
                    let is_sky = sky_materials.iter().any(|sky| std::ptr::eq(*sky, material));
                    let seed = native_material_seed(
                        path,
                        &name,
                        material,
                        &set.name,
                        is_sky,
                        &mut materials,
                    )?;
                    let row = materials
                        .t6_material(
                            seed,
                            &name,
                            set,
                            &native.textures,
                            native.constants,
                            &native.state,
                            T6Draw::Lit,
                            &mut report,
                        )
                        .map_err(|e| format!("T6 material link failed: {e:?}"))?;
                    layer_formats.insert(row, set.world_vert_format);
                    material_rows.insert(name, row);
                    row
                };
                surface_materials.push(Some(row));
                surface_layer_formats.push(layer_formats.get(&row).copied().unwrap_or(0));
            }
            surface_materials.truncate(world_materials.len());
            surface_layer_formats.truncate(world_materials.len());
            let mut draw = build_world_draw(
                &load,
                world_asset,
                &materials,
                surface_materials,
                &surface_layer_formats,
            )?;
            if let Some(model) = sky_model {
                let skel = asset_model::capture_model_skel_t6(model, |surface| {
                    let material = load.asset_at(model.material_slot(surface)?)?;
                    let name = header_str(&load, &material.header, 0)?;
                    material_rows
                        .get(name)
                        .copied()
                        .map(asset_core::WalkLocalMaterialIndex::from_walk)
                })
                .ok_or("T6 sky model geometry missing")?;
                draw.sky_model = Some(asset_world::world_t6::model_mesh(skel)?);
            }
            report.push(format!(
                "T6 skybox: name={sky_name:?} captured={}",
                draw.sky_model.is_some()
            ));
            let model_material = |model: asset_model::T6Model, surface| {
                let material = load.asset_at(model.material_slot(surface)?)?;
                let name = header_str(&load, &material.header, 0)?;
                material_rows
                    .get(name)
                    .copied()
                    .map(asset_core::WalkLocalMaterialIndex::from_walk)
            };
            let mut static_model_meshes = Vec::new();
            let mut smodel_mesh = Vec::with_capacity(smodels.len());
            for (asset, model) in &smodels {
                let mesh = asset_model::capture_model_skel_t6(*model, |surface| {
                    model_material(*model, surface)
                })
                .map(asset_world::world_t6::model_mesh);
                match mesh {
                    Some(Ok(mesh)) => {
                        smodel_mesh.push((*asset, static_model_meshes.len()));
                        static_model_meshes.push(mesh);
                    }
                    Some(Err(error)) => {
                        report.push(format!("T6 static model {:?}: {error}", model.name()))
                    }
                    None => report.push(format!(
                        "T6 static model {:?}: geometry missing",
                        model.name()
                    )),
                }
            }
            let static_model_instances =
                asset_world::world_t6::static_model_placements(&load, world_asset, |slot| {
                    let asset = load.asset_in(world_asset, slot)?;
                    smodel_mesh
                        .iter()
                        .find(|(seen, _)| std::ptr::eq(*seen, asset))
                        .map(|&(_, mesh)| mesh)
                })?;
            let mut map_xmodel_scene_assets = asset_world::MapXModelSceneCatalog::default();
            for model in &script_xmodels {
                let (Some(name), Some(skel)) = (
                    model.name(),
                    asset_model::capture_model_skel_t6(*model, |surface| {
                        model_material(*model, surface)
                    }),
                ) else {
                    continue;
                };
                map_xmodel_scene_assets.insert(
                    asset_world::MapXModelAssetKey(name.to_owned()),
                    asset_world::MapXModelSceneAsset::T6(Arc::new(skel)),
                );
            }
            let script_model_instances = script_placements
                .iter()
                .map(|placement| t6_script_model_instance(placement, &mut map_xmodel_scene_assets))
                .collect::<Vec<_>>();
            report.push(format!(
                "T6 script models: placements={} models={}",
                script_model_instances.len(),
                script_xmodels.len()
            ));
            report.push(format!(
                "T6 static models: slots={} meshes={} placed={}",
                static_model_instances.len(),
                static_model_meshes.len(),
                static_model_instances.iter().flatten().count()
            ));
            for zone in shared_loads.iter().chain(std::iter::once(&load)) {
                for def in zone
                    .assets
                    .iter()
                    .filter(|asset| asset.ty == fastfile_t6::AssetType::LightDef)
                {
                    let Some(name) = header_str(zone, &def.header, 0) else {
                        continue;
                    };
                    let Some(image) = def.field(4).and_then(|index| zone.assets.get(index)) else {
                        continue;
                    };
                    let image_name = header_str(zone, &image.header, IMAGE_NAME)
                        .ok_or("T6 attenuation image name missing")?;
                    let decoded = Arc::new(decode_map_image(zone, image, &ipaks)?);
                    let width = decoded.width() as u16;
                    materials.link_image(asset_material::AuthoredImage {
                        namespace: AssetNamespace::T6,
                        name: asset_core::AssetRef::Real(image_name.to_owned()),
                        map_type: image.header[4],
                        semantic: image.header[5],
                        category: image.header[6],
                        use_srgb_reads: false,
                        width,
                        height: decoded.height() as u16,
                        depth: 1,
                        level_count: decoded.texture_descriptor.mip_level_count as u8,
                        format: 0,
                        payload: Arc::new(Vec::new()),
                        decoded: Some(decoded),
                        common_owned: false,
                        decoded_variant: None,
                        decoded_by: None,
                        pending_decode: None,
                    });
                    draw.light_defs
                        .push(asset_world::world_draw::CapturedLightDef {
                            namespace: AssetNamespace::T6,
                            name: asset_core::AssetRef::decode(name),
                            attenuation_image_name: Some(image_name.to_owned()),
                            attenuation_width: Some(width),
                            attenuation_sampler: t6_sampler_state(def.header[8]),
                            lmap_lookup_start: Reader::word(&def.header, 12)? as i32,
                        });
                }
            }
            report.push(format!("T6 light definitions: {}", draw.light_defs.len()));
            let reader = Reader(&load);
            let probe_count = Reader::word(&world_asset.header, 396)?;
            if probe_count != 0 {
                let probes = Reader::ptr(&world_asset.header, 400)?;
                for i in 0..probe_count {
                    let row = probes.at(i * 76);
                    let image = load
                        .asset_in(world_asset, row.at(60))
                        .ok_or("T6 reflection image missing")?;
                    let decoded = Arc::new(decode_map_image(&load, image, &ipaks)?);
                    let name = header_str(&load, &image.header, IMAGE_NAME)
                        .ok_or("T6 probe name missing")?;
                    let index = materials.link_image(asset_material::AuthoredImage {
                        namespace: AssetNamespace::T6,
                        name: asset_core::AssetRef::Real(name.to_owned()),
                        map_type: 5,
                        semantic: image.header[5],
                        category: image.header[6],
                        use_srgb_reads: false,
                        width: decoded.width() as u16,
                        height: decoded.height() as u16,
                        depth: 1,
                        level_count: decoded.texture_descriptor.mip_level_count as u8,
                        format: u32::from_le_bytes(*b"DXT5"),
                        payload: Arc::new(Vec::new()),
                        decoded: Some(decoded),
                        common_owned: false,
                        decoded_variant: None,
                        decoded_by: None,
                        pending_decode: None,
                    });
                    draw.reflection_probes
                        .push(asset_world::WorldReflectionProbe {
                            image: Some(index),
                            origin: reader.xyz(row)?,
                            lighting_sh: Some([
                                reader.xyzw(row.at(12))?,
                                reader.xyzw(row.at(28))?,
                                reader.xyzw(row.at(44))?,
                            ]),
                        });
                }
            }
            let page_count = Reader::word(&world_asset.header, 408)?;
            if page_count != 0 {
                let pages = Reader::ptr(&world_asset.header, 412)?;
                let mut decoded_pages = Vec::new();
                for i in 0..page_count {
                    let row = pages.at(i * 8);
                    let primary = load.asset_in(world_asset, row);
                    let secondary = load
                        .asset_in(world_asset, row.at(4))
                        .ok_or("T6 secondary lightmap missing")?;
                    let primary_image = primary
                        .map(|asset| decode_map_image(&load, asset, &ipaks))
                        .transpose()?;
                    let secondary_image = decode_map_image(&load, secondary, &ipaks)?;
                    let secondary_size =
                        bevy::math::UVec2::new(secondary_image.width(), secondary_image.height());
                    let primary_size = primary_image
                        .as_ref()
                        .map(|image| bevy::math::UVec2::new(image.width(), image.height()))
                        .unwrap_or(secondary_size);
                    decoded_pages.push(Some(asset_world::WorldLightmap {
                        ambient_image: secondary_image.clone(),
                        directional_image: secondary_image.clone(),
                        sun_mask_image: primary_image.as_ref().unwrap_or(&secondary_image).clone(),
                        primary_image,
                        secondary_image: Some(secondary_image),
                        secondary_b_image: None,
                        ambient_source_name: header_str(&load, &secondary.header, IMAGE_NAME)
                            .unwrap_or("")
                            .to_owned(),
                        sun_mask_source_name: primary
                            .and_then(|asset| header_str(&load, &asset.header, IMAGE_NAME))
                            .unwrap_or("")
                            .to_owned(),
                        ambient_size: secondary_size,
                        sun_mask_size: primary_size,
                    }));
                }
                draw.lightmap = Ok(decoded_pages);
            }
            report.push(format!(
                "T6 lighting images: {probe_count} reflection probes, {page_count} lightmap pages"
            ));
            report.push(format!(
                "T6 world: {} vertices, {} triangles, {} surfaces, {} materials",
                draw.stats.vertices,
                draw.stats.triangles,
                draw.stats.surfaces,
                material_rows.len()
            ));
            let (team_settings, faction_loads, kits) = map_teams(path, &mut report)?;
            let bodies = capture_bodies(
                path,
                &faction_loads,
                &shared_loads,
                &ipaks,
                kits,
                &mut materials,
                &mut report,
            )?;
            let mut sound_loads: Vec<_> = std::iter::once(&load).chain(&faction_loads).collect();
            let mut sound_names = asset_audio::t6_sound_names(&sound_loads);
            sound_names.extend(
                [&team_settings.allies_music, &team_settings.axis_music]
                    .into_iter()
                    .flat_map(|music| {
                        [&music.spawn, &music.victory, &music.defeat]
                            .into_iter()
                            .filter_map(|alias| alias.clone())
                    }),
            );
            sound_names.sort();
            sound_names.dedup();
            sound_loads.extend(&shared_loads);
            let (banks, bank_report) = asset_audio::t6_sound_banks(path);
            report.extend(bank_report);
            let (sound_catalog, filled, sound_gaps) = asset_audio::capture_t6_sounds(
                path,
                &sound_loads,
                &banks,
                sound_names.iter().map(String::as_str),
            );
            report.push(format!(
                "T6 map/faction sounds: {} names, {} captured, {} gaps",
                sound_names.len(),
                filled.len(),
                sound_gaps.len()
            ));
            report.extend(sound_gaps);
            let mut sound_capture = asset_audio::ZoneSoundCapture::for_map(
                path,
                asset_core::FamilyId::T6,
                "T6 map/factions",
            );
            sound_capture.set_t6(sound_catalog);
            let sound = Some(sound_capture.finish(Ok(())));
            let collision = build_clip_collision(&load)?;
            report.push(format!(
                "T6 collision: {} brushes, {} BSP leaves, {} triangles",
                collision.brushes.len(),
                collision.leaves.len(),
                collision.mesh.tri_indices.len() / 3
            ));
            let entities = entity_string(&load)?;
            let spawns = asset_world::dm_spawn_points_treyarch(entities);
            let mut scripts = crate::ScriptSources::default();
            scripts.set_entities(asset_world::t5_entities_for_iw4_rules(entities));
            let light_grid = asset_world::world_t6::light_grid(&load, world_asset)
                .map_err(|e| report.push(format!("T6 light grid: {e}")))
                .ok();
            let mut smodel_lighting_samples = match &light_grid {
                Some(grid) => super::helpers::smodel_lighting_samples(
                    &mut report,
                    grid,
                    asset_world::world_t6::static_model_lighting_origins(&load, world_asset)?,
                    &static_model_instances,
                    Some(&collision),
                ),
                None => Vec::new(),
            };
            let mut vertex_lighting =
                asset_world::world_t6::static_model_vertex_lighting(&load, world_asset)?;
            for sample in &mut smodel_lighting_samples {
                sample.vertex_lighting = vertex_lighting
                    .get_mut(sample.authored_slot)
                    .and_then(Option::take)
                    .map(std::sync::Arc::new);
            }
            let vertex_lit_slots = vertex_lighting.iter().flatten().count();
            if vertex_lit_slots > 0 {
                report.push(format!(
                    "T6 static model vertex lighting: {vertex_lit_slots} slots without a lighting sample"
                ));
            }
            let reflection_probe_images =
                super::helpers::decode_reflection_probes(&mut report, &draw, &materials);
            let exp_fog = load
                .assets
                .iter()
                .filter(|a| a.ty == fastfile_t6::AssetType::Script)
                .filter(|a| {
                    header_str(&load, &a.header, 0).is_some_and(|n| {
                        n.starts_with("maps/mp/createart/") && n.ends_with("_art.gsc")
                    })
                })
                .find_map(|a| {
                    let data = load
                        .blocks
                        .bytes(
                            decode_ptr(header_u32(&a.header, 8)?)?,
                            header_u32(&a.header, 4)? as usize,
                        )
                        .ok()?;
                    asset_world::t6_set_vol_fog(data)
                });
            report.push(format!(
                "T6 createart fog: {}",
                if exp_fog.is_some() { "ready" } else { "none" }
            ));
            let vision_name = path
                .file_stem()
                .map(|stem| format!("vision/{}.vision", stem.to_string_lossy()));
            let t6_film_grade = load
                .assets
                .iter()
                .filter(|a| a.ty == fastfile_t6::AssetType::RawFile)
                .find(|a| header_str(&load, &a.header, 0) == vision_name.as_deref())
                .and_then(|a| {
                    let data = load
                        .blocks
                        .bytes(
                            decode_ptr(header_u32(&a.header, 8)?)?,
                            header_u32(&a.header, 4)? as usize,
                        )
                        .ok()?;
                    asset_world::parse_t6_film_grade(std::str::from_utf8(data).ok()?)
                });
            report.push(format!(
                "T6 vision film grade: {}",
                if t6_film_grade.is_some() {
                    "ready"
                } else {
                    "none"
                }
            ));
            let world = crate::session_load::PreparedWorld {
                min: draw.stats.min,
                max: draw.stats.max,
                world_bounds: draw.stats.bounds,
                draw: Some(draw),
                light_grid,
                reflection_probe_images,
                static_model_meshes,
                static_model_instances,
                map_xmodel_scene_assets,
                script_model_instances,
                smodel_lighting_samples,
                policy: WorldDrawPolicy::t6(),
                intermission_view: asset_world::parse_intermission_view(entities),
                exp_fog,
                t6_film_grade,
                ..crate::PreparedWorld::empty(WorldDrawPolicy::t6())
            };
            Ok(LoadedWorld {
                scripts,
                world,
                materials,
                sound,
                collision: Some(collision),
                bodies,
                spawns,
                facts: crate::MapFacts {
                    script_sound: asset_audio::MapScriptSoundFacts {
                        attackers: team_settings.attackers.clone(),
                        defenders: team_settings.defenders.clone(),
                        ..Default::default()
                    },
                    team_settings,
                    ..Default::default()
                },
                report,
                ..LoadedWorld::empty(WorldDrawPolicy::t6())
            })
        })();
        match result {
            Ok(loaded) => {
                stage.done();
                loaded
            }
            Err(error) => {
                stage.fail();
                LoadedWorld::with_gap(
                    WorldDrawPolicy::t6(),
                    PreparedCapability::PreparedWorld,
                    error,
                    None,
                )
            }
        }
    }

    fn load_common_mp(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,
        _decode_color_maps: bool,
        material_seed: asset_material::MaterialCatalog,
    ) -> CommonCensus {
        let mut report = vec![format!("common_mp (T6): {}", path.display())];
        // Claimed before the walk: the sound bank may ask for this zone while
        // the content is still decoding, and must wait for these aliases
        // rather than read the zone itself.
        let sound_claim = asset_audio::ZoneSoundCapture::claim_common(
            path,
            asset_audio::ZoneGame::T6,
            "t6 common",
        );
        let schema = match schema() {
            Ok(schema) => schema,
            Err(error) => {
                report.push(error);
                return CommonCensus {
                    material_population: material_seed,
                    report,
                    ..Default::default()
                };
            }
        };
        let stage = progress.begin_scoped(StageId::CommonAssets, "t6_walk", None);
        let (load, walked) = fastfile_t6::load_zone(schema, &image.bytes, |_, _| true);
        stage.done();
        if let Err(error) = walked {
            report.push(format!(
                "common_mp T6 walk: stopped after {} assets — {error:?}",
                load.assets.len()
            ));
        }
        let mut catalog = asset_game::WeaponCatalog::default();
        catalog.set_capture_ns(asset_core::AssetNamespace::T6);
        let mut seen = 0;
        for asset in &load.assets {
            if asset.ty != fastfile_t6::AssetType::Weapon {
                continue;
            }
            seen += 1;
            if let Some(view) = fastfile_t6::weapon::WeaponView::new(&load, asset) {
                catalog.capture_t6(view);
            }
        }
        let captured = catalog.len();
        let content_stage = progress.begin_scoped(StageId::CommonAssets, "t6_content", None);
        let patch = patch_zone(path, &mut report);
        let ipaks = open_ipaks(path, &mut report);
        let others: Vec<_> = patch
            .into_iter()
            .chain(hands_zone(path, &mut report))
            .collect();
        let mut content = capture_content(path, &load, &others, &ipaks);
        capture_xanims(&load, &others, &mut content);
        let icon_loads: Vec<_> = others.iter().chain(std::iter::once(&load)).collect();
        let icons = capture_weapon_icons(path, &icon_loads, &ipaks, &mut report);
        asset_material::store_zone_ui_images(asset_core::AssetNamespace::T6, icons);
        capture_sounds(path, &load, sound_claim, &mut content);
        content_stage.done();
        report.extend(content.report.iter().cloned());
        let weapons = catalog.into_build();
        report.push(format!(
            "common_mp T6: walked {} assets; weapons {seen} seen, {captured} with an IW4 stand-in",
            load.assets.len()
        ));
        CommonCensus {
            weapons,
            material_population: material_seed,
            report,
            preparation: Some(Box::new(common_compile::T6CommonCompiler {
                content,
                captured_weapons: captured,
            })),
            ..Default::default()
        }
    }

    fn load_material_population(
        &self,
        path: &Path,
        _image: &ZoneImage,
        _progress: &LoadProgress,
        material_seed: asset_material::MaterialCatalog,
    ) -> MaterialPopulation {
        MaterialPopulation {
            materials: material_seed,
            report: vec![format!(
                "startup materials: T6 materials are not read ({})",
                path.display()
            )],
            ..Default::default()
        }
    }
}
