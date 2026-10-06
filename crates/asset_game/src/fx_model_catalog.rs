use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use asset_material::MaterialDefinitions;
use fastfile_iw4::{ScriptStrings, ZoneStream};

use crate::{
    AssetEdge, AssetEdgeCensus, MaterialCatalog, MaterialIndex, MaterialSpace, ModelSkel,
    ZoneOwner, capture_xmodel_skel,
};
use asset_model::link::{capture_xmodel_material_slots, stamp_xmodel_material_edges};

#[derive(Clone, Debug)]
pub struct FxModelEntry {
    pub skel: Arc<ModelSkel>,
    material_keys: Vec<Option<asset_core::MaterialKey>>,
    material_edges: Vec<AssetEdge<MaterialSpace>>,
}

impl FxModelEntry {
    fn capture(skel: Arc<ModelSkel>, materials: &MaterialCatalog) -> Self {
        let (material_keys, material_edges) =
            capture_xmodel_material_slots(&skel.surface_materials, Some(materials));
        Self {
            skel,
            material_keys,
            material_edges,
        }
    }

    fn resolve_materials(&mut self, materials: &MaterialDefinitions) {
        stamp_xmodel_material_edges(
            &mut self.material_keys,
            &mut self.material_edges,
            &self.skel.surface_materials,
            materials,
        );
    }

    pub fn material_index(&self, surface: usize) -> Option<MaterialIndex> {
        self.material_edges
            .get(surface)?
            .bound_index()
            .map(MaterialIndex::from_order)
    }
}

type FxModelKey = (crate::AssetNamespace, String);

#[derive(Clone, Debug, Default)]
pub struct FxModelCatalog {
    entries: HashMap<FxModelKey, FxModelEntry>,
    order: Vec<FxModelKey>,
    zones: Vec<ZoneOwner>,
    capture_zone: ZoneOwner,
    capture_ns: Option<crate::AssetNamespace>,
    strings: ScriptStrings,
}

impl FxModelCatalog {
    pub fn set_capture_zone(&mut self, zone: ZoneOwner) {
        self.capture_zone = zone;
    }

    pub fn set_capture_ns(&mut self, ns: crate::AssetNamespace) {
        self.capture_ns = Some(ns);
    }

    pub fn set_strings(&mut self, strings: ScriptStrings) {
        self.strings = strings;
    }

    pub fn capture(&mut self, stream: &ZoneStream<'_>, materials: &MaterialCatalog) {
        let Some(geometry) = stream.xmodel() else {
            return;
        };
        let Some(skel) = capture_xmodel_skel(stream, &self.strings, geometry, Some(materials))
        else {
            return;
        };
        self.capture_shared(Arc::new(skel), materials);
    }

    pub fn capture_shared(&mut self, skel: Arc<ModelSkel>, materials: &MaterialCatalog) {
        let key = (
            self.capture_ns
                .expect("asset capture requires an explicit family"),
            skel.name.clone(),
        );
        if !self.entries.contains_key(&key) {
            self.order.push(key.clone());
            self.zones.push(self.capture_zone);
        }
        self.entries
            .entry(key)
            .or_insert_with(|| FxModelEntry::capture(skel, materials));
    }

    pub fn absorb(&mut self, other: Self) -> usize {
        let before = self.entries.len();
        for (index, key) in other.order.into_iter().enumerate() {
            let Some(entry) = other.entries.get(&key).cloned() else {
                continue;
            };
            if self.entries.contains_key(&key) {
                continue;
            }
            self.order.push(key.clone());
            self.zones
                .push(other.zones.get(index).copied().unwrap_or_default());
            self.entries.insert(key, entry);
        }
        self.entries.len().saturating_sub(before)
    }

    pub fn keep_referenced(&mut self, hints: &HashSet<FxModelKey>) {
        let old_order = std::mem::take(&mut self.order);
        let old_zones = std::mem::take(&mut self.zones);
        for (index, key) in old_order.into_iter().enumerate() {
            if hints.contains(&key) {
                self.order.push(key);
                self.zones
                    .push(old_zones.get(index).copied().unwrap_or_default());
            }
        }
        self.entries.retain(|key, _| hints.contains(key));
    }

    pub fn resolve_materials(&mut self, materials: &MaterialDefinitions) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for entry in self.entries.values_mut() {
            entry.resolve_materials(materials);
            for edge in &entry.material_edges {
                census.push(*edge);
            }
        }
        census
    }

    pub fn index_in(&self, ns: crate::AssetNamespace, name: &str) -> Option<usize> {
        self.order
            .iter()
            .position(|(key_ns, key_name)| *key_ns == ns && key_name == name)
    }

    pub fn get_at(&self, index: usize) -> Option<&FxModelEntry> {
        self.entries.get(self.order.get(index)?)
    }

    pub fn name_at(&self, index: usize) -> Option<&str> {
        self.order.get(index).map(|(_, name)| name.as_str())
    }

    pub fn zone_of(&self, index: usize) -> ZoneOwner {
        self.zones.get(index).copied().unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
