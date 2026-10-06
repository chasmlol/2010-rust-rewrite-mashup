mod capture_types;
pub use capture_types::{
    CapturedAlias, CapturedSndCurve, CapturedSound, ChannelKey, LoadedSoundPcm, MSS_PCM,
    VoicePriority,
};
use capture_types::{capture_loaded_edge, is_null_sound_name};
mod capture_iw4;
pub(crate) use capture_iw4::capture_stereo_speaker_gains;
use capture_iw4::*;
mod binding;
pub use binding::{BoundSound, BoundSoundMedia, SoundBindingRefusal, SoundHandle};
use std::collections::HashMap;

use asset_iw4::size as sz;
use asset_iw4::snd_alias::{
    SND_ALIAS_ALIAS_NAME, SND_ALIAS_CENTER_PERCENTAGE, SND_ALIAS_CHAIN, SND_ALIAS_DIST_MAX,
    SND_ALIAS_DIST_MIN, SND_ALIAS_ENVELOP_MAX, SND_ALIAS_ENVELOP_MIN, SND_ALIAS_ENVELOP_PERCENTAGE,
    SND_ALIAS_FLAGS, SND_ALIAS_LFE_PERCENTAGE, SND_ALIAS_MIXER_GROUP, SND_ALIAS_PITCH_MAX,
    SND_ALIAS_PITCH_MIN, SND_ALIAS_PROBABILITY, SND_ALIAS_SECONDARY, SND_ALIAS_SEQUENCE,
    SND_ALIAS_SLAVE_PERCENTAGE, SND_ALIAS_SOUND_FILE, SND_ALIAS_SPEAKER_MAP, SND_ALIAS_START_DELAY,
    SND_ALIAS_SUBTITLE, SND_ALIAS_VELOCITY_MIN, SND_ALIAS_VOL_MAX, SND_ALIAS_VOL_MIN,
    SND_ALIAS_VOLUME_FALLOFF_CURVE, SND_CURVE_DEFAULT_ASSET_NAME, SND_CURVE_KNOT_COUNT,
    SND_CURVE_KNOT_STRIDE, SND_CURVE_KNOTS, SND_CURVE_MAX_KNOTS, SND_ENTCHANNEL_FILE,
    SndAliasFlags,
};
use fastfile_iw4::{AssetLinkSink, AssetType, Ptr, Result, ZonePtr, ZoneStream};

use crate::asset_graph::{AssetEdge, AssetEdgeCensus, AssetEdgeReason, ZoneOwner};
use crate::ent_channel::{EntChannel, parse_ent_channel_file};
use crate::{AssetNamespace, ZoneGame};

pub use asset_iw4::{advance_lcg, lerp_range, pick_weighted_variant_index, unit_random};

pub type LoadedSoundEdge = crate::asset_graph::AssetEdge<crate::asset_graph::LoadedSoundSpace>;

pub type LoadedSoundEdgeReason = AssetEdgeReason;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SoundAliasKey {
    pub namespace: AssetNamespace,
    pub name: String,
}

impl SoundAliasKey {
    pub fn new(namespace: AssetNamespace, name: impl Into<String>) -> Self {
        Self {
            namespace,
            name: name.into(),
        }
    }

    pub fn host(name: impl Into<String>) -> Self {
        Self::new(AssetNamespace::Iw4, name)
    }
}

fn mint_revision() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn ns_of(game: ZoneGame) -> AssetNamespace {
    AssetNamespace::from_zone_game(game)
}

#[derive(Clone, Debug, Default)]
pub struct SoundCatalog {
    pub(crate) sounds: Vec<CapturedSound>,
    pub(crate) loaded: Vec<LoadedSoundPcm>,

    pub(crate) curves: HashMap<(AssetNamespace, String), CapturedSndCurve>,

    pub(crate) rawfiles: HashMap<(AssetNamespace, String), Vec<u8>>,

    pub(crate) ent_channels: HashMap<AssetNamespace, Vec<EntChannel>>,

    group_volumes: HashMap<AssetNamespace, Vec<std::result::Result<f32, crate::MixerGroupError>>>,
    mixer_groups: HashMap<AssetNamespace, Vec<crate::MixerGroup>>,
    by_alias: HashMap<(AssetNamespace, String), usize>,

    by_alias_ci: HashMap<(AssetNamespace, String), usize>,
    by_loaded: HashMap<(AssetNamespace, String), usize>,

    file_to_loaded: HashMap<(u8, u32), String>,

    file_to_streamed: HashMap<(u8, u32), (String, String)>,

    loaded_by_insert: HashMap<(u8, u32), String>,

    curve_by_ptr: HashMap<(u8, u32), String>,
    last_loaded_name: Option<String>,
    last_curve_name: Option<String>,
    capture_game: Option<ZoneGame>,
    pub capture_gaps: usize,

    pub alias_flags_missing: usize,

    pub curve_capture_gaps: usize,
    capture_zone: ZoneOwner,

    revision: u64,
    playback: Vec<Vec<crate::AliasPlaybackPolicy>>,
    selection_weights: Vec<Vec<f32>>,
}

impl SoundCatalog {
    pub fn sounds(&self) -> &[CapturedSound] {
        &self.sounds
    }
    pub fn loaded_sounds(&self) -> &[LoadedSoundPcm] {
        &self.loaded
    }
    pub fn channels_in(&self, namespace: AssetNamespace) -> Option<&[EntChannel]> {
        self.ent_channels.get(&namespace).map(Vec::as_slice)
    }

    pub fn absorb(&mut self, other: SoundCatalog) {
        self.absorb_unresolved(other);
        self.finalize();
    }

    pub fn absorb_unresolved(&mut self, other: SoundCatalog) {
        self.playback.clear();
        self.selection_weights.clear();
        for s in other.sounds {
            self.register_sound(s);
        }
        for l in other.loaded {
            self.register_loaded(l);
        }
        for (key, data) in other.rawfiles {
            self.rawfiles.entry(key).or_insert(data);
        }
        for (namespace, channels) in other.ent_channels {
            self.ent_channels
                .entry(namespace)
                .and_modify(|existing| {
                    if existing.is_empty() {
                        *existing = channels.clone();
                    }
                })
                .or_insert(channels);
        }
        for (ns, volumes) in other.group_volumes {
            self.group_volumes.entry(ns).or_insert(volumes);
        }
        for (ns, groups) in other.mixer_groups {
            self.mixer_groups.entry(ns).or_insert(groups);
        }
        for (name, curve) in other.curves {
            match self.curves.get_mut(&name) {
                Some(existing) if existing.knots.is_empty() && !curve.knots.is_empty() => {
                    *existing = curve;
                }
                None => {
                    self.curves.insert(name, curve);
                }
                Some(_) => {}
            }
        }
        for (k, v) in other.loaded_by_insert {
            self.loaded_by_insert.entry(k).or_insert(v);
        }
        for (k, v) in other.curve_by_ptr {
            self.curve_by_ptr.entry(k).or_insert(v);
        }
        self.capture_gaps += other.capture_gaps;
        self.alias_flags_missing += other.alias_flags_missing;
        self.curve_capture_gaps += other.curve_capture_gaps;

        self.resolve_ent_channels();
    }

    pub fn finalize(&mut self) {
        self.resolve_volume_mod_groups();
        self.resolve_loaded_edges();
        self.resolve_curve_knots();
        self.resolve_ent_channels();
        self.publish();
    }

    pub fn absorb_missing_aliases(&mut self, other: SoundCatalog) {
        self.absorb_missing_aliases_unresolved(other);
        self.finalize();
    }

    pub fn absorb_missing_aliases_unresolved(&mut self, other: SoundCatalog) {
        self.playback.clear();
        self.selection_weights.clear();
        for sound in other.sounds {
            if self.index_in(ns_of(sound.game), &sound.name).is_none() {
                self.register_sound(sound);
            }
        }
        for loaded in other.loaded {
            if self
                .loaded_index_in(ns_of(loaded.game), &loaded.name)
                .is_none()
            {
                self.register_loaded(loaded);
            }
        }
        for (key, data) in other.rawfiles {
            self.rawfiles.entry(key).or_insert(data);
        }
        for (namespace, channels) in other.ent_channels {
            self.ent_channels
                .entry(namespace)
                .and_modify(|existing| {
                    if existing.is_empty() {
                        *existing = channels.clone();
                    }
                })
                .or_insert(channels);
        }
        for (ns, volumes) in other.group_volumes {
            self.group_volumes.entry(ns).or_insert(volumes);
        }
        for (ns, groups) in other.mixer_groups {
            self.mixer_groups.entry(ns).or_insert(groups);
        }
        for (name, curve) in other.curves {
            match self.curves.get_mut(&name) {
                Some(existing) if existing.knots.is_empty() && !curve.knots.is_empty() => {
                    *existing = curve;
                }
                None => {
                    self.curves.insert(name, curve);
                }
                Some(_) => {}
            }
        }
        self.capture_gaps += other.capture_gaps;
    }

    fn resolve_volume_mod_groups(&mut self) {
        if self.group_volumes.contains_key(&AssetNamespace::Iw5) {
            return;
        }
        let Some(file) = self.rawfiles.get(&(
            AssetNamespace::Iw5,
            "soundaliases/volumemodgroups.svmod".to_owned(),
        )) else {
            return;
        };
        let volumes = String::from_utf8_lossy(file)
            .lines()
            .filter_map(|line| {
                let mut fields = line.split(',').map(str::trim);
                let name = fields.next()?;
                if name.is_empty() || name.starts_with('#') {
                    return None;
                }
                Some(fields.next().and_then(|value| value.parse::<f32>().ok()))
            })
            .enumerate()
            .map(|(group, value)| match value {
                Some(gain) if gain.is_finite() && gain >= 0.0 => Ok(gain),
                _ => Err(crate::MixerGroupError::InvalidAttenuation { group }),
            })
            .collect();
        self.group_volumes.insert(AssetNamespace::Iw5, volumes);
    }

    pub fn ingest_mixer_groups(
        &mut self,
        namespace: AssetNamespace,
        groups: Vec<crate::MixerGroup>,
    ) {
        self.mixer_groups.insert(namespace, groups);
        self.group_volumes.remove(&namespace);
        self.playback.clear();
        self.selection_weights.clear();
    }

    pub fn playback_policy(
        &self,
        index: usize,
        variant: usize,
    ) -> Option<&crate::AliasPlaybackPolicy> {
        self.playback.get(index)?.get(variant)
    }

    pub fn publish(&mut self) {
        for (&namespace, groups) in &self.mixer_groups {
            self.group_volumes
                .insert(namespace, crate::compile_mixer_groups(groups));
        }
        self.playback = (0..self.sounds.len())
            .map(|alias| {
                (0..self.sounds[alias].aliases.len())
                    .map(|variant| {
                        crate::AliasPlaybackPolicy::compile(self, alias, variant)
                            .expect("captured alias variant")
                    })
                    .collect()
            })
            .collect();
        self.selection_weights = self
            .sounds
            .iter()
            .map(|sound| {
                sound
                    .aliases
                    .iter()
                    .map(|row| {
                        if row.probability > 0.0 {
                            row.probability
                        } else {
                            1.0
                        }
                    })
                    .collect()
            })
            .collect();
        self.revision = mint_revision();
    }

    pub(crate) fn mixer_group_volumes_in(
        &self,
        namespace: AssetNamespace,
    ) -> Option<&[std::result::Result<f32, crate::MixerGroupError>]> {
        self.group_volumes.get(&namespace).map(Vec::as_slice)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn ingest_rawfile(&mut self, name: &str, data: &[u8], zlib_compressed: bool) {
        let bytes = if zlib_compressed {
            asset_transport::inflate_zlib(data).unwrap_or_else(|_| data.to_vec())
        } else {
            let mut bytes = data.to_vec();
            if bytes.last() == Some(&0) {
                bytes.pop();
            }
            bytes
        };
        if !name.is_empty() {
            self.rawfiles.insert(
                (
                    ns_of(
                        self.capture_game
                            .expect("asset capture requires an explicit family"),
                    ),
                    name.to_owned(),
                ),
                bytes,
            );
        }
    }

    pub(crate) fn capture_zone_for_ingest(&self) -> ZoneOwner {
        self.capture_zone
    }

    pub(crate) fn ingest_loaded(&mut self, loaded: LoadedSoundPcm) {
        self.register_loaded(loaded);
    }

    pub(crate) fn ingest_sound(&mut self, mut sound: CapturedSound) {
        sound.game = self
            .capture_game
            .expect("asset capture requires an explicit family");
        sound.zone = self.capture_zone;
        self.register_sound(sound);
    }

    pub(crate) fn ingest_curve(&mut self, curve: CapturedSndCurve) {
        self.insert_curve(
            ns_of(
                self.capture_game
                    .expect("asset capture requires an explicit family"),
            ),
            curve,
        );
    }

    pub fn resolve_loaded_edges(&mut self) {
        self.playback.clear();
        self.selection_weights.clear();
        for i in 0..self.sounds.len() {
            let ns = ns_of(self.sounds[i].game);
            let alias_name = self.sounds[i].name.clone();
            for j in 0..self.sounds[i].aliases.len() {
                let null_file = self.sounds[i].aliases[j].is_null_file();
                let kind = self.sounds[i].aliases[j].file_kind();
                let hint = self.sounds[i].aliases[j].loaded_name.clone();
                let edge = self.sounds[i].aliases[j].loaded;
                self.sounds[i].aliases[j].loaded_binding_origin =
                    crate::LoadedBindingOrigin::Unresolved;
                if null_file {
                    match self.conventional_loaded_index(ns, &alias_name) {
                        Ok(Some((idx, origin))) => {
                            self.sounds[i].aliases[j].loaded =
                                AssetEdge::bind_order(idx, self.zone_of_loaded(idx));
                            self.sounds[i].aliases[j].loaded_binding_origin = origin;
                        }
                        Ok(None) => {
                            self.sounds[i].aliases[j].loaded = AssetEdge::Absent;
                            self.sounds[i].aliases[j].loaded_binding_origin =
                                crate::LoadedBindingOrigin::MissingConvention;
                        }
                        Err(matches) => {
                            self.sounds[i].aliases[j].loaded = AssetEdge::Absent;
                            self.sounds[i].aliases[j].loaded_binding_origin =
                                crate::LoadedBindingOrigin::AmbiguousConvention { matches };
                        }
                    }
                    continue;
                }

                if matches!(
                    edge,
                    AssetEdge::Unresolved(AssetEdgeReason::TempFieldNotAliasable)
                ) {
                    continue;
                }
                if hint.as_deref().is_some_and(|name| !name.is_empty()) {
                    self.sounds[i].aliases[j].loaded_binding_origin =
                        crate::LoadedBindingOrigin::AuthoredName;
                }
                if let Some(idx) = hint
                    .as_deref()
                    .filter(|name| !name.is_empty())
                    .and_then(|name| self.loaded_index_in(ns, name))
                {
                    self.sounds[i].aliases[j].loaded =
                        AssetEdge::bind_order(idx, self.zone_of_loaded(idx));
                    continue;
                }
                if kind == "streamed" || kind == "primed" {
                    self.sounds[i].aliases[j].loaded = AssetEdge::Absent;
                    self.sounds[i].aliases[j].loaded_binding_origin =
                        crate::LoadedBindingOrigin::NotLoaded;
                    continue;
                }
                if self.sounds[i].aliases[j].loaded.is_bound() {
                    self.sounds[i].aliases[j].loaded =
                        AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss);
                }
            }
        }
    }

    pub fn loaded_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for sound in &self.sounds {
            for row in &sound.aliases {
                census.push(row.loaded);
            }
        }
        census
    }

    pub fn loaded_unresolved_hints(&self) -> Vec<&str> {
        self.sounds
            .iter()
            .filter(|sound| sound.aliases.iter().any(|row| row.loaded.is_unresolved()))
            .map(|sound| sound.name.as_str())
            .collect()
    }

    pub fn loaded_unresolved_reason_counts(&self) -> (usize, usize) {
        let mut temp = 0;
        let mut miss = 0;
        for sound in &self.sounds {
            for row in &sound.aliases {
                match row.loaded {
                    AssetEdge::Unresolved(AssetEdgeReason::TempFieldNotAliasable) => temp += 1,
                    AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss) => miss += 1,
                    _ => {}
                }
            }
        }
        (temp, miss)
    }

    pub fn pcm_at(&self, index: usize) -> Option<&LoadedSoundPcm> {
        self.loaded.get(index)
    }

    pub fn set_capture_zone(&mut self, zone: ZoneOwner) {
        self.capture_zone = zone;
    }

    pub fn set_capture_game(&mut self, game: ZoneGame) {
        self.capture_game = Some(game);
    }

    pub fn zone_of_loaded(&self, index: usize) -> ZoneOwner {
        self.loaded
            .get(index)
            .map(|pcm| pcm.zone)
            .unwrap_or_default()
    }

    fn register_sound(&mut self, sound: CapturedSound) {
        if sound.name.is_empty() {
            return;
        }
        self.playback.clear();
        self.selection_weights.clear();
        let ns = ns_of(sound.game);
        let idx = self.sounds.len();
        self.by_alias.insert((ns, sound.name.clone()), idx);
        self.by_alias_ci
            .insert((ns, sound.name.to_ascii_lowercase()), idx);
        self.sounds.push(sound);
    }

    fn register_loaded(&mut self, loaded: LoadedSoundPcm) {
        if loaded.name.is_empty() {
            return;
        }
        let ns = ns_of(loaded.game);
        let idx = self.loaded.len();
        self.by_loaded.insert((ns, loaded.name.clone()), idx);
        self.loaded.push(loaded);
    }

    pub fn script_alias_looping(&self) -> std::collections::BTreeMap<String, Option<bool>> {
        self.sounds
            .iter()
            .flat_map(|sound| {
                let namespace = ns_of(sound.game);
                let name = sound.name.to_ascii_lowercase();
                let looping = sound.aliases.first().and_then(CapturedAlias::is_looping);
                let qualified = format!("{}:{name}", namespace.as_str());
                std::iter::once((qualified, looping))
                    .chain((namespace == AssetNamespace::Iw4).then_some((name, looping)))
            })
            .collect()
    }

    pub fn index_in(&self, ns: AssetNamespace, alias: &str) -> Option<usize> {
        if let Some(&i) = self.by_alias.get(&(ns, alias.to_owned())) {
            return Some(i);
        }
        self.by_alias_ci
            .get(&(ns, alias.to_ascii_lowercase()))
            .copied()
    }

    pub fn index_by_name(&self, alias: &str) -> Option<usize> {
        self.index_in(AssetNamespace::Iw4, alias)
    }

    pub fn index_unique(&self, alias: &str) -> Option<usize> {
        let mut found = None;
        for ns in AssetNamespace::ALL {
            if let Some(i) = self.index_in(ns, alias) {
                if found.is_some() {
                    return None;
                }
                found = Some(i);
            }
        }
        found
    }

    pub fn sound_at(&self, index: usize) -> Option<&CapturedSound> {
        self.sounds.get(index)
    }

    pub fn sound_in(&self, ns: AssetNamespace, alias: &str) -> Option<&CapturedSound> {
        self.index_in(ns, alias).map(|i| &self.sounds[i])
    }

    pub fn sound(&self, alias: &str) -> Option<&CapturedSound> {
        self.sound_in(AssetNamespace::Iw4, alias)
    }

    pub fn loaded_index_in(&self, ns: AssetNamespace, name: &str) -> Option<usize> {
        let bare = crate::AssetRef::bare_name(name);
        self.by_loaded.get(&(ns, bare.to_owned())).copied()
    }

    fn conventional_loaded_index(
        &self,
        ns: AssetNamespace,
        alias: &str,
    ) -> std::result::Result<Option<(usize, crate::LoadedBindingOrigin)>, usize> {
        if let Some(idx) = self.loaded_index_in(ns, alias) {
            return Ok(Some((
                idx,
                crate::LoadedBindingOrigin::NullAliasExactCompatibility,
            )));
        }
        let wav = format!("/{alias}.wav");
        let mut found = None;
        let mut matches = 0;
        for ((namespace, name), &index) in &self.by_loaded {
            if *namespace == ns && name.ends_with(&wav) {
                found = Some(index);
                matches += 1;
            }
        }
        match matches {
            0 => Ok(None),
            1 => Ok(found.map(|index| {
                (
                    index,
                    crate::LoadedBindingOrigin::NullAliasSuffixCompatibility,
                )
            })),
            _ => Err(matches),
        }
    }

    pub fn name_at(&self, index: usize) -> Option<&str> {
        self.sounds.get(index).map(|s| s.name.as_str())
    }

    pub fn namespace_of_alias(&self, index: usize) -> Option<AssetNamespace> {
        self.sounds.get(index).map(|s| ns_of(s.game))
    }

    pub fn zone_of_alias(&self, index: usize) -> ZoneOwner {
        self.sounds.get(index).map(|s| s.zone).unwrap_or_default()
    }

    fn loaded_name_for_offset(&self, s: &ZoneStream<'_>, p: Ptr) -> Option<String> {
        self.loaded_by_insert
            .get(&file_key(p))
            .cloned()
            .or_else(|| {
                self.loaded_by_insert
                    .get(&file_key(s.resolve_alias(p)))
                    .cloned()
            })
    }

    fn curve_with_knots(&self, namespace: AssetNamespace, key: &str) -> Option<&CapturedSndCurve> {
        self.curves
            .get(&(namespace, key.to_owned()))
            .filter(|c| !c.knots.is_empty())
    }

    fn curve_lookup(&self, namespace: AssetNamespace, name: &str) -> Option<CapturedSndCurve> {
        if let Some(curve) = self.curve_with_knots(namespace, name) {
            return Some(curve.clone());
        }
        let bare = crate::AssetRef::bare_name(name);
        if let Some(curve) = self.curve_with_knots(namespace, bare) {
            let mut out = curve.clone();
            out.name = name.to_owned();
            return Some(out);
        }

        if bare == "$default" {
            if let Some(curve) = self.curve_with_knots(namespace, SND_CURVE_DEFAULT_ASSET_NAME) {
                let mut out = curve.clone();
                out.name = name.to_owned();
                return Some(out);
            }
        }
        None
    }

    pub fn ent_channel(&self, key: ChannelKey) -> Option<&EntChannel> {
        self.ent_channels.get(&key.namespace)?.get(key.id as usize)
    }

    pub fn resolve_ent_channels(&mut self) {
        for namespace in AssetNamespace::ALL {
            if self.ent_channels.contains_key(&namespace) {
                continue;
            }
            let Some(bytes) = self.rawfile_named(namespace, SND_ENTCHANNEL_FILE) else {
                continue;
            };
            let Ok(text) = std::str::from_utf8(bytes) else {
                continue;
            };
            match parse_ent_channel_file(text) {
                Ok(rows) => {
                    self.playback.clear();
                    self.selection_weights.clear();
                    self.ent_channels.insert(namespace, rows);
                }
                Err(error) => {
                    diag::warn!(Zone, "entchannel parse namespace={namespace:?}: {error}")
                }
            }
        }
    }

    fn rawfile_named(&self, namespace: AssetNamespace, want: &str) -> Option<&[u8]> {
        if let Some(data) = self.rawfiles.get(&(namespace, want.to_owned())) {
            return Some(data);
        }
        let mut candidates = self.rawfiles.iter().filter(|((ns, name), _)| {
            let normalized = name.replace('\\', "/");
            *ns == namespace
                && (normalized.eq_ignore_ascii_case(want)
                    || normalized
                        .rsplit('/')
                        .next()
                        .is_some_and(|leaf| leaf.eq_ignore_ascii_case("channels.def")))
        });
        let (_, data) = candidates.next()?;
        if candidates.any(|(_, other)| other != data) {
            diag::warn!(Zone, "ambiguous channel definition namespace={namespace:?}");
            return None;
        }
        Some(data)
    }

    fn insert_curve(&mut self, namespace: AssetNamespace, curve: CapturedSndCurve) {
        if curve.knots.is_empty() {
            let bare = crate::AssetRef::bare_name(&curve.name);
            if self
                .curves
                .get(&(namespace, bare.to_owned()))
                .is_some_and(|c| !c.knots.is_empty())
                || self
                    .curves
                    .get(&(namespace, curve.name.clone()))
                    .is_some_and(|c| !c.knots.is_empty())
            {
                return;
            }
        }
        self.curves.insert((namespace, curve.name.clone()), curve);
    }

    fn curve_name_for_offset(&self, s: &ZoneStream<'_>, p: Ptr) -> Option<String> {
        self.curve_by_ptr.get(&file_key(p)).cloned().or_else(|| {
            self.curve_by_ptr
                .get(&file_key(s.resolve_alias(p)))
                .cloned()
        })
    }

    fn curve_for_row(&mut self, s: &ZoneStream<'_>, row: Ptr) -> Option<CapturedSndCurve> {
        let field = row.at(s.layout(SND_ALIAS_VOLUME_FALLOFF_CURVE, 104));
        let named = self.curve_by_ptr.get(&file_key(field)).cloned();
        if let Some(name) = named.as_deref()
            && let Some(curve) = self.curve_lookup(
                ns_of(
                    self.capture_game
                        .expect("asset capture requires an explicit family"),
                ),
                name,
            )
        {
            return Some(curve);
        }
        match s
            .ptr_at(row, s.layout(SND_ALIAS_VOLUME_FALLOFF_CURVE, 104))
            .ok()
        {
            None | Some(ZonePtr::Null) => None,
            Some(ZonePtr::Offset(p)) => {
                let header = s.resolve_alias(p);
                if let Some(curve) = read_curve_header(s, header) {
                    if let Some(filled) = self.curve_lookup(
                        ns_of(
                            self.capture_game
                                .expect("asset capture requires an explicit family"),
                        ),
                        &curve.name,
                    ) {
                        return Some(filled);
                    }
                    self.insert_curve(
                        ns_of(
                            self.capture_game
                                .expect("asset capture requires an explicit family"),
                        ),
                        curve.clone(),
                    );
                    return Some(curve);
                }
                let name = self.curve_name_for_offset(s, p).or(named);
                match name.as_deref().and_then(|n| {
                    self.curve_lookup(
                        ns_of(
                            self.capture_game
                                .expect("asset capture requires an explicit family"),
                        ),
                        n,
                    )
                }) {
                    Some(curve) => Some(curve),
                    None => name.map(|name| CapturedSndCurve {
                        name,
                        knots: Vec::new(),
                    }),
                }
            }
            Some(_) => named.map(|name| CapturedSndCurve {
                name,
                knots: Vec::new(),
            }),
        }
    }

    pub fn resolve_curve_knots(&mut self) {
        let mut remaining = 0usize;
        for i in 0..self.sounds.len() {
            let namespace = ns_of(self.sounds[i].game);
            for j in 0..self.sounds[i].aliases.len() {
                if self.sounds[i].aliases[j].volume_falloff.is_none()
                    && let Some([dry, near]) = self.sounds[i].aliases[j].t5_distance_curves
                {
                    let dry = self.curves.get(&(namespace, format!("t5/curve/{dry}")));
                    let near = self.curves.get(&(namespace, format!("t5/curve/{near}")));

                    if let (Some(dry), Some(near)) = (dry, near) {
                        self.sounds[i].aliases[j].near_falloff = Some(near.clone());
                        self.sounds[i].aliases[j].volume_falloff = Some(dry.clone());
                    } else {
                        remaining += 1;
                    }
                }
                let Some(cur) = self.sounds[i].aliases[j].volume_falloff.clone() else {
                    continue;
                };
                if !cur.knots.is_empty() {
                    continue;
                }
                match self.curve_lookup(namespace, &cur.name) {
                    Some(filled) => {
                        self.sounds[i].aliases[j].volume_falloff = Some(filled);
                    }
                    None => remaining += 1,
                }
            }
        }
        self.curve_capture_gaps = remaining;
    }

    fn offset_deref_kind(s: &ZoneStream<'_>, p: Ptr) -> Option<&'static str> {
        let v = s.u32_at(p, 0).ok()?;
        zone_ptr_kind(Some(ZonePtr::decode(v)))
    }

    pub fn pcm_for_variant(
        &self,
        ns: AssetNamespace,
        alias: &str,
        variant: usize,
    ) -> Option<&LoadedSoundPcm> {
        let sound = self.sound_in(ns, alias)?;
        let row = sound.aliases.get(variant)?;
        row.loaded
            .bound_index()
            .and_then(|index| self.pcm_at(index))
    }

    pub fn streamed_for_variant(
        &self,
        ns: AssetNamespace,
        alias: &str,
        variant: usize,
    ) -> Option<(AssetNamespace, String, String)> {
        let sound = self.sound_in(ns, alias)?;
        let row = sound.aliases.get(variant)?;
        self.streamed_from_row(ns, row)
    }

    pub fn streamed_for_variant_at(
        &self,
        index: usize,
        variant: usize,
    ) -> Option<(AssetNamespace, String, String)> {
        let sound = self.sounds.get(index)?;
        let row = sound.aliases.get(variant)?;
        self.streamed_from_row(ns_of(sound.game), row)
    }

    fn streamed_from_row(
        &self,
        ns: AssetNamespace,
        row: &CapturedAlias,
    ) -> Option<(AssetNamespace, String, String)> {
        let (dir, name) = row.streamed.as_ref()?;
        if name.is_empty() || name == ",null.wav" {
            return None;
        }
        if ns == AssetNamespace::T5 && dir.is_empty() {
            let path = name.replace('\\', "/");
            let relative = path.strip_prefix("sound/").or_else(|| {
                let (_, rest) = path.split_once('/')?;
                rest.strip_prefix("sound/")
            });
            if let Some(relative) = relative
                && let Some((directory, file)) = relative.rsplit_once('/')
            {
                return Some((ns, directory.to_owned(), file.to_owned()));
            }
            return None;
        }
        if dir.is_empty() {
            return None;
        }
        Some((ns, dir.clone(), name.clone()))
    }

    fn rawfile_text_in(&self, ns: AssetNamespace, name: &str) -> Option<&str> {
        let data = self.rawfiles.get(&(ns, name.to_owned()))?;
        std::str::from_utf8(data).ok()
    }

    fn rawfile_text_unique(&self, name: &str) -> Option<&str> {
        let mut found = None;
        for ns in AssetNamespace::ALL {
            if let Some(text) = self.rawfile_text_in(ns, name) {
                if found.is_some() {
                    return None;
                }
                found = Some(text);
            }
        }
        found
    }

    pub fn rawfile_text(&self, name: &str) -> Option<&str> {
        std::str::from_utf8(self.rawfile_bytes(name)?).ok()
    }

    pub fn rawfile_bytes_in(&self, namespace: AssetNamespace, name: &str) -> Option<&[u8]> {
        self.rawfiles
            .get(&(namespace, name.to_owned()))
            .map(Vec::as_slice)
    }

    pub fn rawfile_bytes(&self, name: &str) -> Option<&[u8]> {
        self.rawfiles
            .get(&(AssetNamespace::Iw4, name.to_owned()))
            .map(Vec::as_slice)
            .or_else(|| {
                let mut found = None;
                for ns in [AssetNamespace::T5, AssetNamespace::Iw5] {
                    if let Some(data) = self.rawfiles.get(&(ns, name.to_owned())) {
                        if found.is_some() {
                            return None;
                        }
                        found = Some(data.as_slice());
                    }
                }
                found
            })
    }

    fn rawfile_text_for_map(&self, ns: AssetNamespace, name: &str) -> Option<&str> {
        self.rawfile_text_in(ns, name)
            .or_else(|| self.rawfile_text_unique(name))
    }

    pub fn createfx_loop_sounds(
        &self,
        ns: AssetNamespace,
        map: &str,
    ) -> Vec<crate::createfx::CreateFxLoopSound> {
        let stem = map.strip_prefix("maps/mp/").unwrap_or(map);
        let stem = stem.strip_suffix(".d3dbsp").unwrap_or(stem);
        let path = format!("maps/createfx/{stem}_fx.gsc");
        let Some(src) = self.rawfile_text_for_map(ns, &path) else {
            let alt = format!("maps/mp/{stem}_fx.gsc");
            return self
                .rawfile_text_for_map(ns, &alt)
                .map(crate::createfx::parse_createfx_loop_sounds)
                .unwrap_or_default();
        };
        crate::createfx::parse_createfx_loop_sounds(src)
    }

    pub fn createfx_oneshots(
        &self,
        ns: AssetNamespace,
        map: &str,
    ) -> Vec<crate::createfx::CreateFxOneshot> {
        let stem = map.strip_prefix("maps/mp/").unwrap_or(map);
        let stem = stem.strip_suffix(".d3dbsp").unwrap_or(stem);
        let createfx_path = format!("maps/createfx/{stem}_fx.gsc");
        let mp_fx_path = format!("maps/mp/{stem}_fx.gsc");
        let src = self
            .rawfile_text_for_map(ns, &createfx_path)
            .or_else(|| self.rawfile_text_for_map(ns, &mp_fx_path));
        let Some(src) = src else {
            return Vec::new();
        };
        let mut shots = crate::createfx::parse_createfx_oneshots(src);
        if let Some(alias_src) = self.rawfile_text_for_map(ns, &mp_fx_path) {
            let aliases = crate::createfx::parse_createfx_effect_aliases(alias_src);
            crate::createfx::apply_createfx_effect_aliases(&mut shots, &aliases);
        }
        shots
    }

    pub fn pick_variant_at(
        &self,
        index: usize,
        rng: &mut u32,
        avoid: Option<usize>,
    ) -> Option<usize> {
        let weights = self
            .selection_weights
            .get(index)
            .filter(|weights| !weights.is_empty())?;
        Some(pick_weighted_variant_index(weights, rng, avoid))
    }
}
