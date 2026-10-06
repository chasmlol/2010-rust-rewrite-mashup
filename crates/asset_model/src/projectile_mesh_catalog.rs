use std::collections::{HashMap, HashSet};

use fastfile_iw4::{ScriptStrings, ZoneStream};

use crate::{
    asset_graph::{
        AssetEdge, AssetEdgeCensus, capture_xmodel_material_slots, stamp_xmodel_material_edges,
    },
    model_kind,
    model_skel::{ModelSkel, capture_untyped_skel},
};
use asset_core::AssetNamespace;
use asset_material::{MaterialCatalog, MaterialDefinitions};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProjectileMeshKey {
    pub namespace: AssetNamespace,
    pub name: String,
}

impl ProjectileMeshKey {
    pub fn new(namespace: AssetNamespace, name: &str) -> Self {
        Self {
            namespace,
            name: name.to_ascii_lowercase(),
        }
    }

    pub fn display(&self) -> String {
        format!("{}:xmodel/{}", self.namespace.as_str(), self.name)
    }
}

#[derive(Clone, Debug)]
pub struct ProjectileMeshEntry {
    pub namespace: AssetNamespace,
    pub skel: std::sync::Arc<ModelSkel>,
    pub material_keys: Vec<Option<asset_core::MaterialKey>>,
    pub material_edges: Vec<AssetEdge<crate::MaterialSpace>>,
}

impl ProjectileMeshEntry {
    fn from_skel(
        namespace: AssetNamespace,
        skel: std::sync::Arc<ModelSkel>,
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

    pub fn key(&self) -> ProjectileMeshKey {
        ProjectileMeshKey::new(self.namespace, &self.skel.name)
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

    pub fn material_index(&self, surface: usize) -> Option<crate::MaterialIndex> {
        self.material_edges.get(surface)?.bound()
    }
}

#[derive(Clone, Debug, Default)]
pub struct ProjectileMeshCatalog {
    entries: HashMap<ProjectileMeshKey, ProjectileMeshEntry>,
    order: Vec<ProjectileMeshKey>,
}

#[derive(Clone, Debug, Default)]
pub struct ProjectileMeshBuild {
    catalog: ProjectileMeshCatalog,
    capture_ns: Option<AssetNamespace>,
    strings: ScriptStrings,
}

impl std::ops::Deref for ProjectileMeshBuild {
    type Target = ProjectileMeshCatalog;

    fn deref(&self) -> &Self::Target {
        &self.catalog
    }
}

impl ProjectileMeshBuild {
    pub fn publish(self) -> ProjectileMeshCatalog {
        self.catalog
    }

    pub fn set_strings(&mut self, strings: ScriptStrings) {
        self.strings = strings;
    }

    pub fn set_capture_ns(&mut self, ns: AssetNamespace) {
        self.capture_ns = Some(ns);
    }

    pub fn capture_unclassified(&mut self, stream: &ZoneStream<'_>, materials: &MaterialCatalog) {
        let Some(geometry) = stream.xmodel() else {
            return;
        };
        let Some(name_ptr) = geometry.name else {
            return;
        };
        let Ok(name) = stream.cstr(name_ptr) else {
            return;
        };
        if model_kind(name).is_some() {
            return;
        }
        let Some(skel) = capture_untyped_skel(stream, &self.strings, geometry, Some(materials))
        else {
            return;
        };
        self.insert_in(
            self.capture_ns
                .expect("asset capture requires an explicit family"),
            skel,
            Some(materials),
        );
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
        let Some(skel) =
            crate::model_skel::capture_xmodel_skel_t5(stream, strings, geometry, materials)
        else {
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
        let Some(skel) =
            crate::model_skel::capture_xmodel_skel_iw5(stream, strings, geometry, materials)
        else {
            return;
        };
        self.insert_in(AssetNamespace::Iw5, skel, Some(materials));
    }

    pub fn absorb(&mut self, mut other: Self) {
        for key in std::mem::take(&mut other.catalog.order) {
            if !self.catalog.entries.contains_key(&key) {
                if let Some(entry) = other.catalog.entries.remove(&key) {
                    self.catalog.order.push(key.clone());
                    self.catalog.entries.insert(key, entry);
                }
            }
        }
    }

    pub fn insert_captured(&mut self, skel: ModelSkel, materials: Option<&MaterialCatalog>) {
        self.insert_in(
            self.capture_ns
                .expect("asset capture requires an explicit family"),
            skel,
            materials,
        );
    }

    pub fn capture_shared_unclassified(
        &mut self,
        skel: &std::sync::Arc<ModelSkel>,
        materials: &MaterialCatalog,
    ) {
        if model_kind(&skel.name).is_none() {
            self.insert_in(
                self.capture_ns
                    .expect("asset capture requires an explicit family"),
                skel.clone(),
                Some(materials),
            );
        }
    }

    pub fn capture_shared(
        &mut self,
        ns: AssetNamespace,
        skel: &std::sync::Arc<ModelSkel>,
        materials: &MaterialCatalog,
    ) {
        self.insert_in(ns, skel.clone(), Some(materials));
    }

    fn insert_in(
        &mut self,
        ns: AssetNamespace,
        skel: impl Into<std::sync::Arc<ModelSkel>>,
        materials: Option<&MaterialCatalog>,
    ) {
        let entry = ProjectileMeshEntry::from_skel(ns, skel.into(), materials);
        let key = entry.key();
        if self.catalog.entries.contains_key(&key) {
            return;
        }
        self.catalog.order.push(key.clone());
        self.catalog.entries.insert(key, entry);
    }

    pub fn keep_referenced(&mut self, hints: &HashSet<ProjectileMeshKey>) {
        self.catalog.order.retain(|key| hints.contains(key));
        self.catalog.entries.retain(|key, _| hints.contains(key));
    }

    pub fn absorb_world_weapon(&mut self, entry: &crate::WorldWeaponEntry) {
        let projectile = ProjectileMeshEntry {
            namespace: entry.namespace,
            skel: entry.skel.clone(),
            material_keys: entry.material_keys.clone(),
            material_edges: entry.material_edges.clone(),
        };
        let key = projectile.key();
        if self.catalog.entries.contains_key(&key) {
            return;
        }
        self.catalog.order.push(key.clone());
        self.catalog.entries.insert(key, projectile);
    }

    pub fn resolve_materials(&mut self, materials: &MaterialDefinitions) {
        for entry in self.catalog.entries.values_mut() {
            entry.resolve_materials(materials);
        }
    }
}

impl ProjectileMeshCatalog {
    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn get(&self, ns: AssetNamespace, name: &str) -> Option<&ProjectileMeshEntry> {
        self.entries.get(&ProjectileMeshKey::new(ns, name))
    }

    pub fn index_by_name(&self, ns: AssetNamespace, name: &str) -> Option<usize> {
        self.order
            .iter()
            .position(|k| *k == ProjectileMeshKey::new(ns, name))
    }

    pub fn get_at(&self, index: usize) -> Option<&ProjectileMeshEntry> {
        self.order.get(index).and_then(|key| self.entries.get(key))
    }

    pub fn name_at(&self, index: usize) -> Option<&str> {
        self.order.get(index).map(|k| k.name.as_str())
    }

    pub fn key_at(&self, index: usize) -> Option<&ProjectileMeshKey> {
        self.order.get(index)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.order.iter().map(|k| k.name.as_str())
    }

    pub fn names_in(&self, ns: AssetNamespace) -> impl Iterator<Item = &str> {
        self.order
            .iter()
            .filter(move |k| k.namespace == ns)
            .map(|k| k.name.as_str())
    }

    pub fn contains(&self, ns: AssetNamespace, name: &str) -> bool {
        self.entries.contains_key(&ProjectileMeshKey::new(ns, name))
    }

    pub fn namespace_count(&self, ns: AssetNamespace) -> usize {
        self.order.iter().filter(|k| k.namespace == ns).count()
    }

    pub fn material_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for key in &self.order {
            if let Some(entry) = self.entries.get(key) {
                for edge in &entry.material_edges {
                    census.push(*edge);
                }
            }
        }
        census
    }

    pub fn report_line(&self) -> String {
        let mut names: Vec<String> = self.order.iter().map(|k| k.display()).collect();
        names.sort_unstable();
        format!(
            "common_mp projectile meshes: {} retained ({})",
            self.order.len(),
            if names.is_empty() {
                "none".to_owned()
            } else {
                names.join(",")
            }
        )
    }
}
