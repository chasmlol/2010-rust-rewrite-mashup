use std::path::Path;
use std::sync::Arc;

use fastfile_iw4::{
    AssetLinkSink, AssetSink, AssetType, Ptr, ScriptStrings, ZoneStream, load_asset_at_observed,
    load_zone,
};

use crate::discover::{GamesRoot, find_runtime_zone, find_zone_file, find_zone_file_version};
use crate::sound_catalog::SoundCatalog;
use crate::zone::{ZoneMemory, open_zone_shared};
use crate::zone_sound::{ZoneSoundOrigin, ensure_zone_sound};
use crate::{AssetNamespace, ZoneGame};

struct SoundSink {
    catalog: SoundCatalog,
    walked: usize,
    stopped_at: Option<(usize, &'static str)>,
}

impl AssetSink for SoundSink {
    fn set_script_strings(&mut self, _strings: ScriptStrings) {}

    fn load_asset(
        &mut self,
        s: &mut ZoneStream<'_>,
        index: usize,
        ty: AssetType,
        slot: Ptr,
    ) -> fastfile_iw4::Result<()> {
        self.stopped_at = Some((index, ty.name()));
        load_asset_at_observed(s, ty, slot, self)?;
        self.walked += 1;
        self.stopped_at = None;
        Ok(())
    }
}

impl AssetLinkSink for SoundSink {
    fn loaded(
        &mut self,
        s: &ZoneStream<'_>,
        ty: AssetType,
        slot: Ptr,
        insert_slot: Option<Ptr>,
    ) -> fastfile_iw4::Result<()> {
        self.catalog.loaded(s, ty, slot, insert_slot)
    }

    fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> fastfile_iw4::Result<()> {
        self.catalog.alias(ty, slot, target)
    }

    fn capture_loaded_sound(
        &mut self,
        s: &ZoneStream<'_>,
        header: Ptr,
        pcm: Ptr,
        data_len: usize,
    ) -> fastfile_iw4::Result<()> {
        self.catalog.capture_loaded_sound(s, header, pcm, data_len)
    }

    fn bind_last_loaded_to_sound_file(&mut self, file: Ptr) -> fastfile_iw4::Result<()> {
        self.catalog.bind_last_loaded_to_sound_file(file)
    }

    fn bind_streamed_sound_file(
        &mut self,
        file: Ptr,
        dir: &str,
        name: &str,
    ) -> fastfile_iw4::Result<()> {
        self.catalog.bind_streamed_sound_file(file, dir, name)
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) -> fastfile_iw4::Result<()> {
        self.catalog.ingest_rawfile(name, data, zlib_compressed);
        Ok(())
    }

    fn capture_sound(
        &mut self,
        s: &ZoneStream<'_>,
        list: Ptr,
        count: usize,
        head: Option<Ptr>,
    ) -> fastfile_iw4::Result<()> {
        self.catalog.capture_sound(s, list, count, head)
    }

    fn capture_snd_curve(&mut self, s: &ZoneStream<'_>, header: Ptr) -> fastfile_iw4::Result<()> {
        self.catalog.capture_snd_curve(s, header)
    }
}

pub fn load_sound_catalog(path: &Path) -> Result<SoundCatalog, String> {
    let image = open_zone_shared(path).map_err(|e| e.to_string())?;
    let header = image.header().map_err(|e| e.to_string())?;
    let mut memory = ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
    let mut sink = SoundSink {
        catalog: SoundCatalog::default(),
        walked: 0,
        stopped_at: None,
    };
    sink.catalog
        .set_capture_zone(crate::ZoneOwner::from_zone_path(path));
    sink.catalog.set_capture_game(crate::ZoneGame::Iw4);
    load_zone(&mut stream, &mut sink).map_err(|e| {
        format!(
            "sound catalog failed after {} assets at {:?}: {e}",
            sink.walked, sink.stopped_at
        )
    })?;
    sink.catalog.resolve_curve_knots();
    sink.catalog.resolve_ent_channels();
    sink.catalog.publish();
    Ok(sink.catalog)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoundZoneGap {
    pub namespace: AssetNamespace,

    pub zone: String,
    pub reason: String,
}

#[derive(Default)]
pub struct LoadedSoundBank {
    pub catalog: SoundCatalog,
    pub gaps: Vec<SoundZoneGap>,
}

fn namespace_of_game(game: ZoneGame) -> AssetNamespace {
    AssetNamespace::from_zone_game(game)
}

pub(crate) fn walk_zone_sound(path: &Path) -> Result<SoundCatalog, String> {
    let game = crate::zone_game_for_path(path)
        .ok_or_else(|| format!("{} is not a readable IWff envelope", path.display()))?;
    match game {
        ZoneGame::Iw4 => load_sound_catalog(path),
        ZoneGame::Iw5 => crate::sound_load_iw5::load_sound_catalog_iw5(path),
        ZoneGame::T5 => crate::sound_load_t5::load_sound_catalog_t5(path),
        ZoneGame::T6 => Err(format!("{}: T6 sound banks are not read", path.display())),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Merge {
    Override,
    Missing,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Absent {
    Skip,
    Gap,
}

struct SoundSource {
    namespace: AssetNamespace,
    name: &'static str,
    found: Result<std::path::PathBuf, String>,
    merge: Merge,
    absent: Absent,
}

fn sound_sources(games: &GamesRoot, map: &Path) -> (Vec<SoundSource>, Vec<SoundSource>) {
    let runtime = |name: &'static str, merge, absent| SoundSource {
        namespace: AssetNamespace::Iw4,
        name,
        found: find_runtime_zone(games, map, name).map(|zone| zone.path),
        merge,
        absent,
    };
    let donor = |namespace, name: &'static str, version| SoundSource {
        namespace,
        name,
        found: find_zone_file_version(games, name, version).map(|zone| zone.path),
        merge: Merge::Missing,
        absent: Absent::Skip,
    };
    let before_map = vec![
        runtime("code_post_gfx_mp", Merge::Override, Absent::Skip),
        runtime("localized_code_post_gfx_mp", Merge::Override, Absent::Skip),
        runtime("patch_mp", Merge::Override, Absent::Skip),
        runtime("common_mp", Merge::Override, Absent::Gap),
        runtime("localized_common_mp", Merge::Override, Absent::Gap),
    ];
    let after_map = vec![
        donor(
            AssetNamespace::Iw5,
            "code_post_gfx_mp",
            fastfile_iw5::ZONE_VERSION_PC,
        ),
        donor(
            AssetNamespace::Iw5,
            "common_mp",
            fastfile_iw5::ZONE_VERSION_PC,
        ),
        donor(
            AssetNamespace::Iw5,
            "localized_common_mp",
            fastfile_iw5::ZONE_VERSION_PC,
        ),
        donor(
            AssetNamespace::T5,
            "code_post_gfx_mp",
            fastfile_t5::ZONE_VERSION_PC,
        ),
        donor(
            AssetNamespace::T5,
            "common_mp",
            fastfile_t5::ZONE_VERSION_PC,
        ),
        donor(
            AssetNamespace::T5,
            "localized_common_mp",
            fastfile_t5::ZONE_VERSION_PC,
        ),
        donor(
            AssetNamespace::T6,
            "common_mp",
            fastfile_t6::ZONE_VERSION_PC,
        ),
    ];
    (before_map, after_map)
}

pub struct SoundSources {
    before_map: SoundCatalog,
    after_map: Vec<(SoundSource, Arc<SoundCatalog>)>,
    gaps: Vec<SoundZoneGap>,
}

fn origin_label(origin: ZoneSoundOrigin) -> String {
    match origin {
        ZoneSoundOrigin::Shared(walk) => format!("{walk} walk"),
        ZoneSoundOrigin::AudioOnly => "audio-only walk".to_owned(),
    }
}

pub fn gather_sound_sources(games: &GamesRoot, map: &Path) -> SoundSources {
    let (before, after) = sound_sources(games, map);
    let mut sources = SoundSources {
        before_map: SoundCatalog::default(),
        after_map: Vec::new(),
        gaps: Vec::new(),
    };
    for source in before {
        if let Some(catalog) = sources.take(&source) {
            sources.before_map.absorb_unresolved((*catalog).clone());
        }
    }
    for source in after {
        if let Some(catalog) = sources.take(&source) {
            sources.after_map.push((source, catalog));
        }
    }
    sources
}

impl SoundSources {
    fn take(&mut self, source: &SoundSource) -> Option<Arc<SoundCatalog>> {
        let path = match &source.found {
            Ok(path) => path,
            Err(error) => {
                match source.absent {
                    Absent::Skip => diag::info!(
                        Zone,
                        "sound bank: {} {} missing ({error})",
                        source.namespace.as_str(),
                        source.name
                    ),
                    Absent::Gap => self.gap(source.namespace, source.name, error.clone()),
                }
                return None;
            }
        };
        let (catalog, origin) = ensure_zone_sound(path);
        match catalog {
            Ok(catalog) => {
                diag::info!(
                    Zone,
                    "sound bank: {} {} from the {} — {} aliases {} loaded",
                    source.namespace.as_str(),
                    source.name,
                    origin_label(origin),
                    catalog.sounds.len(),
                    catalog.loaded.len()
                );
                Some(catalog)
            }
            Err(error) => {
                self.gap(source.namespace, source.name, error);
                None
            }
        }
    }

    fn gap(&mut self, namespace: AssetNamespace, zone: &str, reason: String) {
        self.gaps.push(SoundZoneGap {
            namespace,
            zone: zone.to_owned(),
            reason,
        });
    }
}

pub fn compose_sound_bank(
    sources: SoundSources,
    zone: &str,
    namespace: AssetNamespace,
    map: Result<SoundCatalog, String>,
) -> LoadedSoundBank {
    let SoundSources {
        before_map,
        after_map,
        gaps,
    } = sources;
    let mut bank = LoadedSoundBank {
        catalog: before_map,
        gaps,
    };
    match map {
        Ok(map) => {
            diag::info!(
                Zone,
                "sound bank: map zone {zone} ({}) — {} rawfiles, {} aliases",
                namespace.as_str(),
                map.rawfiles.len(),
                map.sounds.len()
            );
            match namespace {
                AssetNamespace::Iw4 => bank.catalog.absorb_unresolved(map),
                _ => bank.catalog.absorb_missing_aliases_unresolved(map),
            }
        }
        Err(error) => bank.gap(namespace, zone, error),
    }
    for (source, catalog) in after_map {
        let catalog = (*catalog).clone();
        match source.merge {
            Merge::Override => bank.catalog.absorb_unresolved(catalog),
            Merge::Missing => bank.catalog.absorb_missing_aliases_unresolved(catalog),
        }
    }
    bank.catalog.finalize();
    for gap in &bank.gaps {
        diag::warn!(
            Zone,
            "sound bank gap: {} `{}` — {}",
            gap.namespace.as_str(),
            gap.zone,
            gap.reason
        );
    }
    bank
}

pub fn load_mp_sound_bank(games: &GamesRoot, zone: &str) -> Result<LoadedSoundBank, String> {
    let zone_ff = find_zone_file(games, zone)?;
    let sources = gather_sound_sources(games, &zone_ff.path);
    let namespace =
        crate::zone_game_for_path(&zone_ff.path).map_or(AssetNamespace::Iw4, namespace_of_game);
    let (map, _) = ensure_zone_sound(&zone_ff.path);
    let map = map.map(|catalog| (*catalog).clone());
    Ok(compose_sound_bank(sources, zone, namespace, map))
}

impl LoadedSoundBank {
    fn gap(&mut self, namespace: AssetNamespace, zone: &str, reason: String) {
        self.gaps.push(SoundZoneGap {
            namespace,
            zone: zone.to_owned(),
            reason,
        });
    }

    pub fn gap_lines(&self) -> Vec<String> {
        self.gaps
            .iter()
            .map(|g| {
                format!(
                    "sound bank gap: {} `{}` — {}",
                    g.namespace.as_str(),
                    g.zone,
                    g.reason
                )
            })
            .collect()
    }
}
