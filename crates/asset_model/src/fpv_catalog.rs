use std::collections::HashMap;

use fastfile_iw4::{ScriptStrings, ZoneStream};

use crate::asset_graph::{
    AssetEdge, AssetEdgeCensus, capture_xmodel_material_slots, stamp_xmodel_material_edges,
};
use crate::model_skel::{FpvSkel, capture_fpv_skel, capture_fpv_skel_iw5, capture_fpv_skel_t5};
use crate::{ModelKind, model_kind};
use asset_core::AssetNamespace;
use asset_core::FpvMeshIndex;
use asset_material::{MaterialCatalog, MaterialDefinitions};

pub const VIEWHANDS_NAME: &str = "viewmodel_base_viewhands";

pub const VIEWHANDS_NAME_T5: &str = "viewhands_usmc";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FpvMeshKey {
    pub namespace: AssetNamespace,
    pub name: String,
}

impl FpvMeshKey {
    pub fn new(namespace: AssetNamespace, name: &str) -> Self {
        Self {
            namespace,
            name: ascii_lower(name),
        }
    }

    pub fn display(&self) -> String {
        format!("{}:xmodel/{}", self.namespace.as_str(), self.name)
    }
}

const fn game_default_hands_name(ns: AssetNamespace) -> &'static str {
    match ns {
        AssetNamespace::Iw4 | AssetNamespace::Iw5 | AssetNamespace::T6 => VIEWHANDS_NAME,
        AssetNamespace::T5 => VIEWHANDS_NAME_T5,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FpvHands {
    FromKit {
        namespace: AssetNamespace,
        name: String,
    },

    FromWeaponDef {
        namespace: AssetNamespace,
        name: String,
    },

    GameDefault {
        namespace: AssetNamespace,
        name: String,
    },
    Unresolved,
}

impl FpvHands {
    pub fn key(&self) -> Option<(AssetNamespace, &str)> {
        match self {
            Self::FromKit { namespace, name }
            | Self::FromWeaponDef { namespace, name }
            | Self::GameDefault { namespace, name } => Some((*namespace, name.as_str())),
            Self::Unresolved => None,
        }
    }

    pub fn game_default(ns: AssetNamespace) -> Self {
        Self::GameDefault {
            namespace: ns,
            name: game_default_hands_name(ns).to_owned(),
        }
    }

    pub fn resolve(
        catalog: &FpvMeshCatalog,
        map_ns: AssetNamespace,
        kit: Option<&crate::SoldierKit>,
        weapon_hand: Option<&str>,
        weapon_ns: AssetNamespace,
    ) -> Self {
        if let Some(kit) = kit {
            if let Some(name) = kit.arms.as_deref() {
                if catalog.contains(map_ns, name) {
                    return Self::FromKit {
                        namespace: map_ns,
                        name: name.to_owned(),
                    };
                }
            }
            let arm_names: Vec<&str> = catalog
                .names_in(map_ns)
                .filter(|n| crate::is_arms_model(n))
                .collect();
            if let Some(name) = crate::arms_for_body(&kit.body, &arm_names) {
                if catalog.contains(map_ns, &name) {
                    return Self::FromKit {
                        namespace: map_ns,
                        name,
                    };
                }
            }
        }
        if let Some(name) = weapon_hand {
            if weapon_ns == map_ns && catalog.contains(weapon_ns, name) {
                return Self::FromWeaponDef {
                    namespace: weapon_ns,
                    name: name.to_owned(),
                };
            }
        }
        let def = Self::game_default(map_ns);
        if let Some((ns, name)) = def.key() {
            if catalog.contains(ns, name) {
                return def;
            }
        }
        Self::Unresolved
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TagViewBind {
    pub quat: [f32; 4],
    pub trans: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct FpvMeshEntry {
    pub namespace: AssetNamespace,
    pub skel: std::sync::Arc<FpvSkel>,

    pub material_keys: Vec<Option<asset_core::MaterialKey>>,

    pub material_edges: Vec<AssetEdge<crate::MaterialSpace>>,
}

impl FpvMeshEntry {
    fn from_skel(
        namespace: AssetNamespace,
        skel: std::sync::Arc<FpvSkel>,
        materials: Option<&MaterialCatalog>,
    ) -> Self {
        let (material_keys, material_edges) =
            capture_xmodel_material_slots(&skel.surface_materials, materials.map(|c| &**c));
        Self {
            namespace,
            skel,
            material_keys,
            material_edges,
        }
    }

    pub fn key(&self) -> FpvMeshKey {
        FpvMeshKey::new(self.namespace, &self.skel.name)
    }

    pub(crate) fn resolve_materials(&mut self, materials: &MaterialDefinitions) {
        stamp_xmodel_material_edges(
            &mut self.material_keys,
            &mut self.material_edges,
            &self.skel.surface_materials,
            materials,
        );
    }

    pub fn material_present_name(&self, surface: usize) -> Option<&str> {
        self.material_edges
            .get(surface)?
            .is_bound()
            .then(|| Some(self.material_keys.get(surface)?.as_ref()?.name.as_str()))
            .flatten()
    }

    pub fn tag_view(&self) -> Option<TagViewBind> {
        let i = self.skel.tag_view?;
        let b = self.skel.bones.get(i)?;
        Some(TagViewBind {
            quat: b.quat,
            trans: b.trans,
        })
    }

    pub fn tag_weapon(&self) -> Option<TagViewBind> {
        let i = self.skel.tag_weapon?;
        let b = self.skel.bones.get(i)?;
        Some(TagViewBind {
            quat: b.quat,
            trans: b.trans,
        })
    }

    pub fn bone_count(&self) -> usize {
        self.skel.bones.len()
    }
}

#[derive(Clone, Debug, Default)]
pub struct FpvMeshCatalog {
    identity: u64,
    entries: Vec<FpvMeshEntry>,
    indices: HashMap<FpvMeshKey, usize>,
    order: Vec<FpvMeshKey>,

    zones: Vec<crate::ZoneOwner>,

    pub map_namespace: Option<AssetNamespace>,
}

#[derive(Clone, Debug)]
pub struct FpvMeshBuild {
    catalog: FpvMeshCatalog,
    capture_zone: crate::ZoneOwner,
    strings: ScriptStrings,
    capture_ns: Option<AssetNamespace>,
}

impl Default for FpvMeshBuild {
    fn default() -> Self {
        Self {
            catalog: FpvMeshCatalog::default(),
            capture_zone: crate::ZoneOwner::default(),
            strings: ScriptStrings::default(),
            capture_ns: None,
        }
    }
}

impl std::ops::Deref for FpvMeshBuild {
    type Target = FpvMeshCatalog;

    fn deref(&self) -> &Self::Target {
        &self.catalog
    }
}

impl FpvMeshBuild {
    pub fn publish(self) -> FpvMeshCatalog {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let mut catalog = self.catalog;
        catalog.identity = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        catalog
    }

    pub fn set_strings(&mut self, strings: ScriptStrings) {
        self.strings = strings;
    }

    pub fn set_capture_ns(&mut self, ns: AssetNamespace) {
        self.capture_ns = Some(ns);
    }

    pub fn set_capture_zone(&mut self, zone: crate::ZoneOwner) {
        self.capture_zone = zone;
    }

    pub fn set_map_namespace(&mut self, ns: Option<AssetNamespace>) {
        self.catalog.map_namespace = ns;
    }

    pub fn capture(&mut self, stream: &ZoneStream<'_>, materials: &MaterialCatalog) {
        let Some(geometry) = stream.xmodel() else {
            return;
        };
        let Some(name_ptr) = geometry.name else {
            return;
        };
        let Ok(name) = stream.cstr(name_ptr) else {
            return;
        };
        if model_kind(name) != Some(ModelKind::Fpv) {
            return;
        }
        let Some(skel) = capture_fpv_skel(stream, &self.strings, geometry, Some(materials)) else {
            return;
        };
        self.insert_captured(skel, Some(materials));
    }

    pub fn capture_t5(
        &mut self,
        stream: &fastfile_t5::ZoneStream<'_>,
        strings: &fastfile_t5::ScriptStrings,
        materials: &MaterialCatalog,
    ) {
        let Some(geometry) = stream.latest_xmodel() else {
            return;
        };
        let Some(name_ptr) = geometry.name else {
            return;
        };
        let Ok(name) = stream.cstr(name_ptr) else {
            return;
        };
        if model_kind(name) != Some(ModelKind::Fpv) {
            return;
        }
        let Some(skel) = capture_fpv_skel_t5(stream, strings, geometry, Some(materials)) else {
            return;
        };
        self.insert_in(AssetNamespace::T5, skel, Some(materials));
    }

    pub fn capture_iw5(
        &mut self,
        stream: &fastfile_iw5::ZoneStream<'_>,
        strings: &fastfile_iw5::ScriptStrings,
        materials: &MaterialCatalog,
    ) {
        let Some(geometry) = stream.latest_xmodel() else {
            return;
        };
        let Some(name_ptr) = geometry.name else {
            return;
        };
        let Ok(name) = stream.cstr(name_ptr) else {
            return;
        };
        if model_kind(name) != Some(ModelKind::Fpv) {
            return;
        }
        let Some(skel) = capture_fpv_skel_iw5(stream, strings, geometry, Some(materials)) else {
            return;
        };
        self.insert_in(AssetNamespace::Iw5, skel, Some(materials));
    }

    pub fn capture_shared(
        &mut self,
        ns: Option<AssetNamespace>,
        skel: &std::sync::Arc<FpvSkel>,
        materials: &MaterialCatalog,
    ) {
        if model_kind(&skel.name) == Some(ModelKind::Fpv) {
            self.insert_in(
                ns.or(self.capture_ns)
                    .expect("asset capture requires an explicit family"),
                skel.clone(),
                Some(materials),
            );
        }
    }

    pub fn insert_captured(&mut self, skel: FpvSkel, materials: Option<&MaterialCatalog>) {
        self.insert_in(
            self.capture_ns
                .expect("asset capture requires an explicit family"),
            skel,
            materials,
        );
    }

    pub fn insert_in(
        &mut self,
        ns: AssetNamespace,
        skel: impl Into<std::sync::Arc<FpvSkel>>,
        materials: Option<&MaterialCatalog>,
    ) {
        let entry = FpvMeshEntry::from_skel(ns, skel.into(), materials);
        self.retain(entry.key(), entry);
    }

    fn retain(&mut self, key: FpvMeshKey, entry: FpvMeshEntry) {
        if let Some(&pos) = self.catalog.indices.get(&key) {
            self.catalog.zones[pos] = self.capture_zone;
            self.catalog.entries[pos] = entry;
        } else {
            self.catalog
                .indices
                .insert(key.clone(), self.catalog.entries.len());
            self.catalog.order.push(key.clone());
            self.catalog.zones.push(self.capture_zone);
            self.catalog.entries.push(entry);
        }
    }

    pub fn absorb(&mut self, mut other: Self) -> usize {
        let saved = self.capture_zone;
        let mut added = 0;
        let order = std::mem::take(&mut other.catalog.order);
        let entries = std::mem::take(&mut other.catalog.entries);
        for (i, (key, entry)) in order.into_iter().zip(entries).enumerate() {
            self.capture_zone = other
                .catalog
                .zones
                .get(i)
                .copied()
                .unwrap_or(other.capture_zone);
            let vacant = !self.catalog.indices.contains_key(&key);
            self.retain(key, entry);
            if vacant {
                added += 1;
            }
        }
        self.capture_zone = saved;
        added
    }

    pub fn resolve_materials(&mut self, materials: &MaterialDefinitions) {
        for entry in &mut self.catalog.entries {
            entry.resolve_materials(materials);
        }
    }
}

impl FpvMeshCatalog {
    pub fn identity(&self) -> u64 {
        self.identity
    }

    pub fn index_by_name(&self, ns: AssetNamespace, name: &str) -> Option<usize> {
        let key = FpvMeshKey::new(ns, name);
        self.indices.get(&key).copied()
    }

    pub fn zone_of(&self, index: usize) -> crate::ZoneOwner {
        self.zones.get(index).copied().unwrap_or_default()
    }

    pub fn get_at(&self, index: usize) -> Option<&FpvMeshEntry> {
        self.entries.get(index)
    }

    pub fn name_at(&self, index: usize) -> Option<&str> {
        self.order.get(index).map(|k| k.name.as_str())
    }

    pub fn material_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for entry in &self.entries {
            for edge in &entry.material_edges {
                census.push(*edge);
            }
        }
        census
    }

    pub fn material_unresolved_hints(&self) -> Vec<&str> {
        let mut names = Vec::new();
        for entry in &self.entries {
            for (edge, key) in entry.material_edges.iter().zip(entry.material_keys.iter()) {
                if edge.is_unresolved()
                    && let Some(name) = key.as_ref().map(|key| key.name.as_str())
                    && !name.is_empty()
                    && !names.contains(&name)
                {
                    names.push(name);
                }
            }
        }
        names.sort_unstable();
        names
    }

    pub fn material_bound_zones(&self) -> String {
        crate::bound_zone_names(
            self.entries
                .iter()
                .flat_map(|entry| entry.material_edges.iter()),
        )
    }

    pub fn get(&self, ns: AssetNamespace, name: &str) -> Option<&FpvMeshEntry> {
        self.index_by_name(ns, name)
            .and_then(|index| self.get_at(index))
    }

    pub fn bound_material_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.entries.iter().flat_map(|entry| {
            entry
                .material_edges
                .iter()
                .filter_map(|edge| edge.bound_index())
        })
    }

    pub fn hands(&self) -> Option<&FpvMeshEntry> {
        self.hands_in(AssetNamespace::Iw4)
    }

    pub fn hands_in(&self, ns: AssetNamespace) -> Option<&FpvMeshEntry> {
        self.get_hands(&FpvHands::game_default(ns))
    }

    pub fn get_hands(&self, hands: &FpvHands) -> Option<&FpvMeshEntry> {
        let (ns, name) = hands.key()?;
        self.get(ns, name)
    }

    pub fn names_in(&self, ns: AssetNamespace) -> impl Iterator<Item = &str> {
        self.order
            .iter()
            .filter(move |k| k.namespace == ns)
            .map(|k| k.name.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, ns: AssetNamespace, name: &str) -> bool {
        self.indices.contains_key(&FpvMeshKey::new(ns, name))
    }

    pub fn namespace_count(&self, ns: AssetNamespace) -> usize {
        self.order.iter().filter(|k| k.namespace == ns).count()
    }

    pub fn collide_name_count(&self) -> usize {
        let mut seen: HashMap<&str, u8> = HashMap::new();
        for k in &self.order {
            *seen.entry(k.name.as_str()).or_insert(0) |= match k.namespace {
                AssetNamespace::Iw4 => 1,
                AssetNamespace::T5 => 2,
                AssetNamespace::Iw5 => 4,
                AssetNamespace::T6 => 8,
            };
        }
        seen.values().filter(|bits| bits.count_ones() >= 2).count()
    }

    pub fn tag_view_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.skel.tag_view.is_some())
            .count()
    }
}

fn ascii_lower(name: &str) -> String {
    name.to_ascii_lowercase()
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PoseStats {
    pub hands_rigid: usize,
    pub hands_blend: usize,
    pub gun_rigid: usize,
    pub gun_blend: usize,
    pub idle_sampled: bool,
}

const SCOPE_ATTACH_TAGS: &[&str] = &[
    "tag_scope",
    "tag_acog",
    "tag_red_dot",
    "tag_reflex",
    "tag_hybrid",
    "tag_thermal",
    "tag_thermal_scope",
    "tag_eotech",
];

#[derive(Clone, Debug)]
pub struct FpvMount {
    pub model: FpvMeshIndex,
    pub parent_model: usize,
    pub tag: String,
}

#[derive(Clone, Debug)]
pub struct FpvMountPlan {
    pub gun: FpvMeshIndex,
    pub secondary_gun: Option<FpvMeshIndex>,
    pub attachments: Vec<FpvMount>,
    pub rocket: Option<FpvMount>,
    pub ads_swaps: Vec<(usize, FpvMeshIndex)>,
}

impl FpvMountPlan {
    pub fn attachment_models(&self, ads: bool) -> impl Iterator<Item = FpvMeshIndex> + '_ {
        self.attachments.iter().enumerate().map(move |(at, mount)| {
            ads.then(|| self.ads_swaps.iter().find(|(swap, _)| *swap == at))
                .flatten()
                .map_or(mount.model, |&(_, model)| model)
        })
    }
}

#[derive(Clone, Debug)]
pub struct FpvMountError {
    pub model: String,
    pub detail: &'static str,
}

impl core::fmt::Display for FpvMountError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "FPV model `{}`: {}", self.model, self.detail)
    }
}

fn scope_attach_tag_name<'a>(gun_bones: &'a [String], scope_bones: &[String]) -> Option<&'a str> {
    if let Some(root) = scope_bones.first()
        && let Some(hit) = gun_bones
            .iter()
            .find(|name| name.eq_ignore_ascii_case(root))
    {
        return Some(hit.as_str());
    }
    for tag in SCOPE_ATTACH_TAGS {
        if !scope_bones
            .iter()
            .any(|name| name.eq_ignore_ascii_case(tag))
        {
            continue;
        }
        if let Some(hit) = gun_bones.iter().find(|name| name.eq_ignore_ascii_case(tag)) {
            return Some(hit.as_str());
        }
    }
    gun_bones.iter().find_map(|name| {
        SCOPE_ATTACH_TAGS
            .iter()
            .copied()
            .find(|tag| name.eq_ignore_ascii_case(tag))
            .map(|_| name.as_str())
    })
}

pub fn plan_fpv_mounts(
    catalog: &FpvMeshCatalog,
    gun: FpvMeshIndex,
    attachments: &[FpvMeshIndex],
    rocket: Option<FpvMeshIndex>,
    on_gun_root: bool,
) -> Result<FpvMountPlan, FpvMountError> {
    let gun_entry = catalog.get_at(gun.order()).ok_or_else(|| FpvMountError {
        model: format!("#{}", gun.order()),
        detail: "gun missing from FPV catalog",
    })?;
    let gun_skel = &gun_entry.skel;
    if gun_skel.pose.is_none() {
        return Err(FpvMountError {
            model: gun_skel.name.clone(),
            detail: "gun has no pose source",
        });
    }
    let mut selected: Vec<FpvMount> = Vec::with_capacity(attachments.len());
    for &model in attachments {
        let entry = catalog.get_at(model.order()).ok_or_else(|| FpvMountError {
            model: format!("#{}", model.order()),
            detail: "attachment missing from FPV catalog",
        })?;
        let skel = &entry.skel;
        if skel.pose.is_none() {
            return Err(FpvMountError {
                model: skel.name.clone(),
                detail: "attachment has no pose source",
            });
        }
        let on_attachment = skel.bone_names.first().and_then(|root| {
            selected.iter().enumerate().rev().find_map(|(slot, mount)| {
                let bones = &catalog.get_at(mount.model.order())?.skel.bone_names;
                let hit = bones.iter().find(|name| name.eq_ignore_ascii_case(root))?;
                Some((slot + 2, hit.as_str()))
            })
        });
        let on_gun =
            || scope_attach_tag_name(&gun_skel.bone_names, &skel.bone_names).map(|tag| (1, tag));
        let root_on_gun = skel.bone_names.first().is_some_and(|root| {
            gun_skel
                .bone_names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(root))
        });
        let gun_root = || {
            let tag = skel.mount_tag.as_deref().and_then(|tag| {
                gun_skel
                    .bone_names
                    .iter()
                    .find(|name| name.eq_ignore_ascii_case(tag))
            });
            on_gun_root
                .then(|| {
                    tag.or(gun_skel.bone_names.first())
                        .map(|bone| (1, bone.as_str()))
                })
                .flatten()
        };
        let joint = if on_gun_root {
            gun_root()
        } else if root_on_gun {
            on_gun()
        } else {
            on_attachment.or_else(on_gun)
        };
        let (parent_model, tag) = joint.ok_or_else(|| FpvMountError {
            model: skel.name.clone(),
            detail: "no compatible mount on gun or earlier attachment",
        })?;
        selected.push(FpvMount {
            model,
            parent_model,
            tag: tag.to_owned(),
        });
    }
    let rocket = rocket
        .map(|model| {
            let entry = catalog.get_at(model.order()).ok_or_else(|| FpvMountError {
                model: format!("#{}", model.order()),
                detail: "rocket missing from FPV catalog",
            })?;
            if entry.skel.pose.is_none() {
                return Err(FpvMountError {
                    model: entry.skel.name.clone(),
                    detail: "rocket has no pose source",
                });
            }
            let tag = gun_skel
                .bone_names
                .iter()
                .find(|name| name.eq_ignore_ascii_case("tag_clip"))
                .ok_or_else(|| FpvMountError {
                    model: entry.skel.name.clone(),
                    detail: "gun has no tag_clip rocket mount",
                })?;
            Ok(FpvMount {
                model,
                parent_model: 1,
                tag: tag.clone(),
            })
        })
        .transpose()?;
    Ok(FpvMountPlan {
        gun,
        secondary_gun: None,
        attachments: selected,
        rocket,
        ads_swaps: Vec::new(),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyFpvMesh<F: asset_core::Family> {
    index: crate::FpvMeshIndex,
    owner: u64,
    family: core::marker::PhantomData<F>,
}

impl<F: asset_core::Family> FamilyFpvMesh<F> {
    pub fn index(self) -> crate::FpvMeshIndex {
        self.index
    }
    pub fn owner(self) -> u64 {
        self.owner
    }
}

impl FpvMeshCatalog {
    pub fn family_mesh<F: asset_core::Family>(
        &self,
        index: crate::FpvMeshIndex,
    ) -> Option<FamilyFpvMesh<F>> {
        (self.identity() != 0 && self.get_at(index.order())?.namespace == F::ID).then_some(
            FamilyFpvMesh {
                index,
                owner: self.identity(),
                family: core::marker::PhantomData,
            },
        )
    }
}
