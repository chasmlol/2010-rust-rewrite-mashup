use std::collections::HashMap;
use std::path::Path;

use crate::sound_catalog::{
    CapturedAlias, CapturedSndCurve, CapturedSound, LoadedSoundPcm, MSS_PCM, SoundCatalog,
};
use crate::zone::{Iw5ZoneMemory, open_zone_shared};
use crate::{AssetNamespace, ZoneGame, ZoneOwner};
use asset_iw4::snd_alias::{SND_CURVE_KNOT_COUNT, SND_CURVE_KNOT_STRIDE, SND_CURVE_KNOTS};
use fastfile_iw5::size as sz;
use fastfile_iw5::{
    AssetLinkSink, AssetSink, AssetType, Ptr, ScriptStrings, ZonePtr, ZoneStream,
    load_asset_at_observed, load_zone,
};

#[derive(Default)]
pub struct Iw5SoundCapture {
    catalog: SoundCatalog,
    last_loaded_name: Option<String>,
    last_curve_name: Option<String>,
    file_to_loaded: HashMap<(u8, u32), String>,
    file_to_streamed: HashMap<(u8, u32), (String, String)>,
    loaded_by_ptr: HashMap<(u8, u32), String>,
    curve_by_ptr: HashMap<(u8, u32), String>,
}

impl Iw5SoundCapture {
    pub fn for_zone(path: &Path) -> Self {
        let mut capture = Self::default();
        capture
            .catalog
            .set_capture_zone(ZoneOwner::from_zone_path(path));
        capture.catalog.set_capture_game(ZoneGame::Iw5);
        capture
    }

    pub(crate) fn catalog_mut(&mut self) -> &mut SoundCatalog {
        &mut self.catalog
    }

    pub fn finish(mut self) -> SoundCatalog {
        self.catalog.resolve_loaded_edges();
        self.catalog.resolve_curve_knots();
        self.catalog.resolve_ent_channels();
        self.catalog.publish();
        self.catalog
    }
}

struct Iw5SoundSink {
    capture: Iw5SoundCapture,
    walked: usize,
    stopped_at: Option<(usize, &'static str)>,
}

impl AssetSink for Iw5SoundSink {
    fn set_script_strings(&mut self, _strings: ScriptStrings) {}

    fn load_asset(
        &mut self,
        s: &mut ZoneStream<'_>,
        index: usize,
        ty: AssetType,
        slot: Ptr,
    ) -> fastfile_iw5::Result<()> {
        self.stopped_at = Some((index, ty.name()));
        load_asset_at_observed(s, ty, slot, &mut self.capture)?;
        self.walked += 1;
        self.stopped_at = None;
        Ok(())
    }
}

impl AssetLinkSink for Iw5SoundCapture {
    fn loaded(
        &mut self,
        _s: &ZoneStream<'_>,
        ty: AssetType,
        slot: Ptr,
        insert_slot: Option<Ptr>,
    ) -> fastfile_iw5::Result<()> {
        if ty == AssetType::LoadedSound
            && let Some(name) = self.last_loaded_name.clone()
        {
            self.loaded_by_ptr.insert(file_key(slot), name.clone());
            if let Some(ins) = insert_slot {
                self.loaded_by_ptr.insert(file_key(ins), name);
            }
        }
        if ty == AssetType::SoundCurve
            && let Some(name) = self.last_curve_name.clone()
        {
            self.curve_by_ptr.insert(file_key(slot), name.clone());
            if let Some(ins) = insert_slot {
                self.curve_by_ptr.insert(file_key(ins), name);
            }
        }
        Ok(())
    }

    fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> fastfile_iw5::Result<()> {
        if ty == AssetType::LoadedSound
            && let Some(name) = self.loaded_by_ptr.get(&file_key(target)).cloned()
        {
            self.loaded_by_ptr.insert(file_key(slot), name);
        }
        if ty == AssetType::SoundCurve
            && let Some(name) = self.curve_by_ptr.get(&file_key(target)).cloned()
        {
            self.curve_by_ptr.insert(file_key(slot), name);
        }
        Ok(())
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) -> fastfile_iw5::Result<()> {
        self.catalog.ingest_rawfile(name, data, zlib_compressed);
        Ok(())
    }

    fn capture_loaded_sound(
        &mut self,
        s: &ZoneStream<'_>,
        header: Ptr,
        pcm: Ptr,
        data_len: usize,
    ) -> fastfile_iw5::Result<()> {
        let name = name_at(s, header, 0).unwrap_or_default();

        let (format, rate, bits, channels, samples, block_size) =
            if s.wire_format() == fastfile_iw5::Iw5WireFormat::X64 {
                let format = i32::from(s.u16_at(header, 8).unwrap_or(0));
                let channels = i32::from(s.u16_at(header, 10).unwrap_or(0));
                let rate = s.u32_at(header, 12).unwrap_or(0);
                let block_size = u32::from(s.u16_at(header, 20).unwrap_or(0));
                let bits = i32::from(s.u16_at(header, 22).unwrap_or(0));
                let samples = if format == MSS_PCM && block_size != 0 {
                    (data_len / block_size as usize) as u32
                } else {
                    0
                };
                (format, rate, bits, channels, samples, block_size)
            } else {
                (
                    s.i32_at(header, 4).unwrap_or(0),
                    s.u32_at(header, 4 + 12).unwrap_or(0),
                    s.i32_at(header, 4 + 16).unwrap_or(0),
                    s.i32_at(header, 4 + 20).unwrap_or(0),
                    s.u32_at(header, 4 + 24).unwrap_or(0),
                    s.u32_at(header, 4 + 28).unwrap_or(0),
                )
            };
        let pcm_bytes = if data_len == 0 {
            Vec::new()
        } else {
            s.slice_at(pcm, 0, data_len)
                .map(|b| b.to_vec())
                .unwrap_or_default()
        };
        if name.is_empty() {
            self.catalog.capture_gaps += 1;
            self.last_loaded_name = None;
            return Ok(());
        }
        self.last_loaded_name = Some(name.clone());
        if pcm_bytes.is_empty() {
            return Ok(());
        }
        self.catalog.ingest_loaded(LoadedSoundPcm {
            name,
            game: ZoneGame::Iw5,
            format,
            rate,
            bits,
            channels,
            samples,
            block_size,
            pcm: pcm_bytes.into(),
            zone: self.catalog.capture_zone_for_ingest(),
            seek_table: Vec::new(),
            sab_media: None,
        });
        Ok(())
    }

    fn bind_last_loaded_to_sound_file(&mut self, file: Ptr) -> fastfile_iw5::Result<()> {
        if let Some(name) = self.last_loaded_name.clone() {
            self.file_to_loaded.insert(file_key(file), name);
        } else {
            self.catalog.capture_gaps += 1;
        }
        Ok(())
    }

    fn bind_streamed_sound_file(
        &mut self,
        file: Ptr,
        dir: &str,
        name: &str,
    ) -> fastfile_iw5::Result<()> {
        self.file_to_streamed
            .insert(file_key(file), (dir.to_owned(), name.to_owned()));
        Ok(())
    }

    fn capture_snd_curve(&mut self, s: &ZoneStream<'_>, header: Ptr) -> fastfile_iw5::Result<()> {
        let name = name_at(s, header, 0).unwrap_or_default();
        let count = s
            .u16_at(header, s.layout(SND_CURVE_KNOT_COUNT, 8))
            .unwrap_or(0) as usize;
        let n = count.min(16);
        let mut knots = Vec::with_capacity(n);
        let knots_off = s.layout(SND_CURVE_KNOTS, 12);
        for i in 0..n {
            let off = knots_off + i * SND_CURVE_KNOT_STRIDE;
            let x = s.f32_at(header, off).unwrap_or(0.0);
            let y = s.f32_at(header, off + 4).unwrap_or(0.0);
            knots.push((x, y));
        }
        self.last_curve_name = Some(name.clone());
        self.curve_by_ptr.insert(file_key(header), name.clone());
        if !name.is_empty() {
            self.catalog.ingest_curve(CapturedSndCurve { name, knots });
        }
        Ok(())
    }

    fn capture_sound(
        &mut self,
        s: &ZoneStream<'_>,
        list: Ptr,
        count: usize,
        head: Option<Ptr>,
    ) -> fastfile_iw5::Result<()> {
        let name = name_at(s, list, 0).unwrap_or_default();
        let Some(arr) = head else {
            if count > 0 {
                self.catalog.capture_gaps += 1;
            }
            return Ok(());
        };
        let mut aliases = Vec::with_capacity(count);
        for i in 0..count {
            let row = arr.at(i * s.layout(sz::SND_ALIAS, 152));
            aliases.push(self.capture_alias_row(s, row));
        }
        if name.is_empty() {
            self.catalog.capture_gaps += 1;
            return Ok(());
        }
        self.catalog.ingest_sound(CapturedSound {
            name,
            aliases,
            game: ZoneGame::Iw5,
            zone: self.catalog.capture_zone_for_ingest(),
        });
        Ok(())
    }
}

impl Iw5SoundCapture {
    fn capture_alias_row(&self, s: &ZoneStream<'_>, row: Ptr) -> CapturedAlias {
        let alias_name =
            name_at(s, row, s.layout(sz::SND_ALIAS_ALIAS_NAME_OFF, 0)).unwrap_or_default();
        let (
            loaded_name,
            streamed,
            file_type,
            file_exists,
            file_name,
            file_u,
            file_u_ptr,
            file_u_deref,
        ) = match s
            .ptr_at(row, s.layout(sz::SND_ALIAS_SOUND_FILE_OFF, 40))
            .ok()
        {
            Some(ZonePtr::Offset(p)) => {
                let file = s.resolve_alias(p);
                let key = file_key(file);
                let loaded = self.file_to_loaded.get(&key).cloned();
                let streamed = self.file_to_streamed.get(&key).cloned();
                let inspected = inspect_sound_file(s, file);
                let u = s.ptr_at(file, s.layout(4, 8)).ok();
                let loaded = loaded.or_else(|| match u {
                    Some(ZonePtr::Offset(q)) => {
                        self.loaded_by_ptr.get(&file_key(q)).cloned().or_else(|| {
                            self.loaded_by_ptr
                                .get(&file_key(s.resolve_alias(q)))
                                .cloned()
                        })
                    }
                    _ => None,
                });
                let file_u_deref = match u {
                    Some(ZonePtr::Offset(q)) => offset_deref_kind(s, q),
                    _ => None,
                };
                let loaded = if file_u_deref == Some("following")
                    || file_u_deref == Some("insert")
                    || matches!(u, Some(ZonePtr::Following) | Some(ZonePtr::Insert))
                {
                    loaded
                } else {
                    loaded.or(inspected.loaded_name.clone())
                };
                let streamed = streamed.or(inspected.streamed);
                let file_name = loaded.clone().or(inspected.file_name.clone()).or_else(|| {
                    streamed.as_ref().map(|(dir, name)| {
                        if dir.is_empty() {
                            name.clone()
                        } else {
                            format!("{dir}/{name}")
                        }
                    })
                });
                (
                    loaded,
                    streamed,
                    inspected.file_type,
                    inspected.file_exists,
                    file_name,
                    zone_ptr_kind(u),
                    zone_ptr_label(u),
                    file_u_deref,
                )
            }
            Some(ZonePtr::Null) => (None, None, None, None, None, Some("null"), None, None),
            other => (
                None,
                None,
                Some(0),
                None,
                None,
                zone_ptr_kind(other),
                zone_ptr_label(other),
                None,
            ),
        };
        let sound_file = s
            .ptr_at(row, s.layout(sz::SND_ALIAS_SOUND_FILE_OFF, 40))
            .ok();
        let loaded = capture_loaded_edge(
            sound_file,
            file_type,
            file_u,
            file_u_deref,
            loaded_name.as_deref(),
        );
        let curve_field = row.at(s.layout(sz::SND_ALIAS_VOLUME_FALLOFF_CURVE_OFF, 120));
        let volume_falloff = self
            .curve_by_ptr
            .get(&file_key(curve_field))
            .and_then(|name| {
                self.catalog
                    .curves
                    .get(&(AssetNamespace::Iw5, name.clone()))
            })
            .cloned()
            .or_else(|| {
                match s
                    .ptr_at(row, s.layout(sz::SND_ALIAS_VOLUME_FALLOFF_CURVE_OFF, 120))
                    .ok()
                {
                    Some(ZonePtr::Offset(p)) => {
                        let target = s.resolve_alias(p);
                        self.curve_by_ptr
                            .get(&file_key(target))
                            .or_else(|| self.curve_by_ptr.get(&file_key(p)))
                            .cloned()
                            .and_then(|name| {
                                self.catalog
                                    .curves
                                    .get(&(AssetNamespace::Iw5, name))
                                    .cloned()
                            })
                    }
                    _ => None,
                }
            });
        CapturedAlias {
            alias_name,
            subtitle: optional_name(s, row, s.layout(sz::SND_ALIAS_SUBTITLE_OFF, 8)),
            secondary: optional_name(s, row, s.layout(sz::SND_ALIAS_SECONDARY_OFF, 16)),
            chain: optional_name(s, row, s.layout(sz::SND_ALIAS_CHAIN_OFF, 24)),
            mixer_group: optional_name(s, row, s.layout(sz::SND_ALIAS_MIXER_GROUP_OFF, 32)),
            loaded_name,
            loaded,
            loaded_binding_origin: crate::LoadedBindingOrigin::Unresolved,
            streamed,
            file_type,
            file_exists,
            file_name,
            file_u,
            file_u_ptr,
            file_u_deref,
            sequence: s
                .i32_at(row, s.layout(sz::SND_ALIAS_SEQUENCE_OFF, 48))
                .unwrap_or(0),
            vol_min: s
                .f32_at(row, s.layout(sz::SND_ALIAS_VOL_MIN_OFF, 52))
                .unwrap_or(0.0),
            vol_max: s
                .f32_at(row, s.layout(sz::SND_ALIAS_VOL_MAX_OFF, 56))
                .unwrap_or(0.0),
            vol_mod_index: s
                .u32_at(row, s.layout(sz::SND_ALIAS_VOL_MOD_INDEX_OFF, 60))
                .ok(),
            pitch_min: s
                .f32_at(row, s.layout(sz::SND_ALIAS_PITCH_MIN_OFF, 64))
                .unwrap_or(0.0),
            pitch_max: s
                .f32_at(row, s.layout(sz::SND_ALIAS_PITCH_MAX_OFF, 68))
                .unwrap_or(0.0),
            dist_min: s
                .f32_at(row, s.layout(sz::SND_ALIAS_DIST_MIN_OFF, 72))
                .unwrap_or(0.0),
            dist_max: s
                .f32_at(row, s.layout(sz::SND_ALIAS_DIST_MAX_OFF, 76))
                .unwrap_or(0.0),
            velocity_min: s
                .f32_at(row, s.layout(sz::SND_ALIAS_VELOCITY_MIN_OFF, 80))
                .unwrap_or(0.0),
            flags: s.u32_at(row, s.layout(sz::SND_ALIAS_FLAGS_OFF, 84)).ok(),
            looping: None,
            slave_percentage: s
                .f32_at(row, s.layout(sz::SND_ALIAS_SLAVE_PERCENTAGE_OFF, 96))
                .unwrap_or(0.0),
            probability: s
                .f32_at(row, s.layout(sz::SND_ALIAS_PROBABILITY_OFF, 100))
                .unwrap_or(1.0),
            lfe_percentage: s
                .f32_at(row, s.layout(sz::SND_ALIAS_LFE_PERCENTAGE_OFF, 104))
                .unwrap_or(0.0),
            center_percentage: s
                .f32_at(row, s.layout(sz::SND_ALIAS_CENTER_PERCENTAGE_OFF, 108))
                .unwrap_or(0.0),
            start_delay: s
                .i32_at(row, s.layout(sz::SND_ALIAS_START_DELAY_OFF, 112))
                .unwrap_or(0),
            volume_falloff,
            t5_distance_curves: None,
            near_falloff: None,
            voice_priority: None,
            envelop_min: s
                .f32_at(row, s.layout(sz::SND_ALIAS_ENVELOP_MIN_OFF, 128))
                .unwrap_or(0.0),
            envelop_max: s
                .f32_at(row, s.layout(sz::SND_ALIAS_ENVELOP_MAX_OFF, 132))
                .unwrap_or(0.0),
            envelop_percentage: s
                .f32_at(row, s.layout(sz::SND_ALIAS_ENVELOP_PERCENTAGE_OFF, 136))
                .unwrap_or(0.0),
            speaker_map: speaker_map_name(s, row),
            stereo_speaker_gains: stereo_speaker_gains(s, row),
            limit_count: None,
            entity_limit_count: None,
        }
    }
}

struct InspectedSoundFile {
    file_type: Option<u8>,
    file_exists: Option<u8>,
    loaded_name: Option<String>,
    streamed: Option<(String, String)>,
    file_name: Option<String>,
}

fn inspect_sound_file(s: &ZoneStream<'_>, file: Ptr) -> InspectedSoundFile {
    let file_type = s.u8_at(file, 0).ok();
    let file_exists = s.u8_at(file, 1).ok();
    match file_type {
        Some(1) => {
            let loaded_name = match s.ptr_at(file, s.layout(4, 8)).ok() {
                Some(ZonePtr::Offset(p)) => {
                    let target = s.resolve_alias(p);
                    match s.u32_at(target, 0).ok().map(ZonePtr::decode) {
                        Some(ZonePtr::Following) | Some(ZonePtr::Insert) => None,
                        _ => name_at(s, target, 0),
                    }
                }
                _ => None,
            };
            InspectedSoundFile {
                file_type,
                file_exists,
                file_name: loaded_name.clone(),
                loaded_name,
                streamed: None,
            }
        }
        Some(2) | Some(3) => {
            let dir = name_at(s, file, s.layout(4, 8)).unwrap_or_default();
            let name = name_at(s, file, s.layout(8, 16)).unwrap_or_default();
            let file_name = if dir.is_empty() {
                name.clone()
            } else {
                format!("{dir}/{name}")
            };
            let streamed = (!name.is_empty()).then_some((dir, name));
            InspectedSoundFile {
                file_type,
                file_exists,
                loaded_name: None,
                streamed,
                file_name: (!file_name.is_empty()).then_some(file_name),
            }
        }
        _ => InspectedSoundFile {
            file_type,
            file_exists,
            loaded_name: None,
            streamed: None,
            file_name: None,
        },
    }
}

fn capture_loaded_edge(
    sound_file: Option<ZonePtr>,
    file_type: Option<u8>,
    file_u: Option<&str>,
    file_u_deref: Option<&str>,
    loaded_name: Option<&str>,
) -> crate::LoadedSoundEdge {
    use crate::asset_graph::{AssetEdge, AssetEdgeReason};
    match sound_file {
        None | Some(ZonePtr::Null) => AssetEdge::Absent,
        Some(ZonePtr::Following) | Some(ZonePtr::Insert) => {
            AssetEdge::Unresolved(AssetEdgeReason::TempFieldNotAliasable)
        }
        Some(ZonePtr::Offset(_)) => match file_type {
            Some(2) | Some(3) => AssetEdge::Absent,
            Some(1) => {
                if loaded_name.is_some_and(|n| {
                    n.rsplit(['/', '\\'])
                        .next()
                        .unwrap_or(n)
                        .eq_ignore_ascii_case("null.wav")
                }) {
                    return AssetEdge::Absent;
                }
                match file_u {
                    Some("null") | None => AssetEdge::Absent,
                    Some("following") | Some("insert") => {
                        if loaded_name.is_some_and(|name| !name.is_empty()) {
                            AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss)
                        } else {
                            AssetEdge::Unresolved(AssetEdgeReason::TempFieldNotAliasable)
                        }
                    }
                    Some("offset") => {
                        let stub =
                            file_u_deref == Some("following") || file_u_deref == Some("insert");
                        if stub && !loaded_name.is_some_and(|name| !name.is_empty()) {
                            AssetEdge::Unresolved(AssetEdgeReason::TempFieldNotAliasable)
                        } else {
                            AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss)
                        }
                    }
                    _ => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
                }
            }
            _ => AssetEdge::Absent,
        },
    }
}

fn name_at(s: &ZoneStream<'_>, parent: Ptr, field: usize) -> Option<String> {
    match s.ptr_at(parent, field).ok()? {
        ZonePtr::Offset(q) => s
            .cstr(s.resolve_alias(q))
            .ok()
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        _ => None,
    }
}

fn optional_name(s: &ZoneStream<'_>, parent: Ptr, field: usize) -> Option<String> {
    name_at(s, parent, field)
}

fn speaker_map_name(s: &ZoneStream<'_>, row: Ptr) -> Option<String> {
    match s
        .ptr_at(row, s.layout(sz::SND_ALIAS_SPEAKER_MAP_OFF, 144))
        .ok()?
    {
        ZonePtr::Offset(p) => name_at(s, s.resolve_alias(p), s.layout(4, 8)),
        _ => None,
    }
}

fn stereo_speaker_gains(s: &ZoneStream<'_>, row: Ptr) -> Option<[[f32; 2]; 2]> {
    if s.wire_format() != fastfile_iw5::Iw5WireFormat::X86 {
        return None;
    }
    let ZonePtr::Offset(map) = s.ptr_at(row, sz::SND_ALIAS_SPEAKER_MAP_OFF).ok()? else {
        return None;
    };
    let map = s.resolve_alias(map);
    crate::sound_catalog::capture_stereo_speaker_gains(|offset| s.u32_at(map, offset).ok())
}

fn file_key(p: Ptr) -> (u8, u32) {
    (p.block, p.offset)
}

fn zone_ptr_kind(p: Option<ZonePtr>) -> Option<&'static str> {
    Some(match p? {
        ZonePtr::Null => "null",
        ZonePtr::Offset(_) => "offset",
        ZonePtr::Following => "following",
        ZonePtr::Insert => "insert",
    })
}

fn zone_ptr_label(p: Option<ZonePtr>) -> Option<String> {
    match p? {
        ZonePtr::Offset(q) => Some(format!("{}:{}", q.block, q.offset)),
        _ => None,
    }
}

fn offset_deref_kind(s: &ZoneStream<'_>, p: Ptr) -> Option<&'static str> {
    match s.u32_at(p, 0).ok().map(ZonePtr::decode) {
        Some(ZonePtr::Following) => Some("following"),
        Some(ZonePtr::Insert) => Some("insert"),
        Some(ZonePtr::Null) => Some("null"),
        Some(ZonePtr::Offset(_)) => Some("offset"),
        None => None,
    }
}

pub fn load_sound_catalog_iw5(path: &Path) -> Result<SoundCatalog, String> {
    let image = open_zone_shared(path).map_err(|e| e.to_string())?;
    if image.game != ZoneGame::Iw5 {
        return Err(format!(
            "load_sound_catalog_iw5: {} is {:?}, not IW5",
            path.display(),
            image.game
        ));
    }
    let header = image.iw5_header().map_err(|e| e.to_string())?;
    let mut memory = Iw5ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
    let mut sink = Iw5SoundSink {
        capture: Iw5SoundCapture::for_zone(path),
        walked: 0,
        stopped_at: None,
    };
    if let Err(e) = load_zone(&mut stream, &mut sink) {
        if let Some((idx, name)) = sink.stopped_at {
            diag::info!(
                Zone,
                "iw5 sound catalog: stopped at asset {idx} ({name}) after {} walked: {e}",
                sink.walked
            );
        }
    }
    Ok(sink.capture.finish())
}
