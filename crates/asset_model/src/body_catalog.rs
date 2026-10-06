use std::collections::HashMap;

use fastfile_iw4::{ScriptStrings, ZoneStream};

use crate::asset_graph::{
    AssetEdge, AssetEdgeCensus, capture_xmodel_material_slots, stamp_xmodel_material_edges,
};
use crate::model_skel::{
    ModelSkel, capture_body_skel, capture_body_skel_iw5, capture_body_skel_t5,
};
use crate::soldiers::{SoldierKits, body_has_tp_attach_bones, is_body_model, soldier_kits};
use asset_material::{MaterialCatalog, MaterialDefinitions};

pub const BODY_SPINE_BONES: &[&str] = &[
    "torso_stabilizer",
    "pelvis",
    "j_spinelower",
    "j_spineupper",
    "j_spine4",
    "j_head",
];

#[derive(Clone, Debug)]
pub struct BodyMeshEntry {
    pub skel: ModelSkel,
    pub namespace: asset_core::AssetNamespace,

    pub material_keys: Vec<Option<asset_core::MaterialKey>>,

    pub material_edges: Vec<AssetEdge<crate::MaterialSpace>>,
}

impl BodyMeshEntry {
    fn from_skel(
        skel: ModelSkel,
        materials: Option<&MaterialCatalog>,
        namespace: asset_core::AssetNamespace,
    ) -> Self {
        let (material_keys, material_edges) =
            capture_xmodel_material_slots(&skel.surface_materials, materials.map(|c| &**c));
        Self {
            skel,
            namespace,
            material_keys,
            material_edges,
        }
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

    pub fn bone_count(&self) -> usize {
        self.skel.bones.len()
    }

    pub fn has_spine_chain(&self) -> bool {
        BODY_SPINE_BONES
            .iter()
            .all(|want| self.skel.bone_names.iter().any(|n| n == want))
    }

    pub fn bone_names(&self) -> &[String] {
        &self.skel.bone_names
    }
}

#[derive(Clone, Debug, Default)]
pub struct BodyMeshCatalog {
    entries: HashMap<String, BodyMeshEntry>,
    kits_cache: std::sync::OnceLock<SoldierKits>,
}

#[derive(Clone, Debug, Default)]
pub struct BodyMeshBuild {
    catalog: BodyMeshCatalog,
    strings: ScriptStrings,
}

impl std::ops::Deref for BodyMeshBuild {
    type Target = BodyMeshCatalog;

    fn deref(&self) -> &Self::Target {
        &self.catalog
    }
}

impl BodyMeshBuild {
    pub fn publish(self) -> BodyMeshCatalog {
        self.catalog
    }

    pub fn set_strings(&mut self, strings: ScriptStrings) {
        self.strings = strings;
    }

    pub fn capture(&mut self, stream: &ZoneStream<'_>, materials: &MaterialCatalog) {
        let Some(geometry) = stream.xmodel() else {
            return;
        };
        let Some(skel) = capture_body_skel(stream, &self.strings, geometry, Some(materials)) else {
            return;
        };
        self.insert_entry(
            skel.name.clone(),
            BodyMeshEntry::from_skel(skel, Some(materials), asset_core::AssetNamespace::Iw4),
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
        let Some(skel) = capture_body_skel_t5(stream, strings, geometry, Some(materials)) else {
            return;
        };
        self.insert_entry(
            skel.name.clone(),
            BodyMeshEntry::from_skel(skel, Some(materials), asset_core::AssetNamespace::T5),
        );
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
        let Some(skel) = capture_body_skel_iw5(stream, strings, geometry, Some(materials)) else {
            return;
        };
        self.insert_entry(
            skel.name.clone(),
            BodyMeshEntry::from_skel(skel, Some(materials), asset_core::AssetNamespace::Iw5),
        );
    }

    fn insert_entry(&mut self, name: String, entry: BodyMeshEntry) {
        self.catalog.kits_cache = std::sync::OnceLock::new();
        self.catalog.entries.entry(name).or_insert(entry);
    }

    pub fn insert(&mut self, skel: crate::ModelSkel) {
        self.insert_captured(skel, None);
    }

    pub fn insert_captured(&mut self, skel: crate::ModelSkel, materials: Option<&MaterialCatalog>) {
        self.insert_in(asset_core::AssetNamespace::Iw4, skel, materials);
    }

    pub fn insert_in(
        &mut self,
        namespace: asset_core::AssetNamespace,
        skel: ModelSkel,
        materials: Option<&MaterialCatalog>,
    ) {
        self.insert_entry(
            skel.name.clone(),
            BodyMeshEntry::from_skel(skel, materials, namespace),
        );
    }

    pub fn set_kits(&mut self, kits: SoldierKits) {
        self.catalog.kits_cache = std::sync::OnceLock::from(kits);
    }

    pub fn resolve_materials(&mut self, materials: &MaterialDefinitions) {
        for entry in self.catalog.entries.values_mut() {
            entry.resolve_materials(materials);
        }
    }
}

impl BodyMeshCatalog {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&BodyMeshEntry> {
        self.entries.get(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    pub fn kits(&self) -> &SoldierKits {
        self.kits_cache.get_or_init(|| {
            let names: Vec<String> = self
                .entries
                .iter()
                .filter(|(name, entry)| {
                    !is_body_model(name) || body_has_tp_attach_bones(&entry.skel.bone_names)
                })
                .map(|(name, _)| name.clone())
                .collect();
            soldier_kits(&names)
        })
    }

    pub fn report_lines(&self) -> Vec<String> {
        let kits = self.kits();
        let census = self.material_edge_census();
        let mut lines = vec![format!(
            "bodies: {} captured; materialHandles bound={} unresolved={} absent={}",
            self.entries.len(),
            census.bound,
            census.unresolved,
            census.absent,
        )];
        for (side, kit) in [
            ("allies", kits.allies.as_ref()),
            ("axis", kits.axis.as_ref()),
        ] {
            let Some(kit) = kit else {
                continue;
            };
            if let Some(body) = self.entries.get(&kit.body) {
                lines.push(format!(
                    "bodies: {} bones={} verts={} (rigid={}, blend={}) [{side}]",
                    kit.body,
                    body.bone_count(),
                    body.skel.positions.len(),
                    body.skel.rigid_verts,
                    body.skel.blend_verts,
                ));
            } else {
                lines.push(format!("bodies: {} missing skel [{side}]", kit.body));
            }
            match &kit.head {
                Some(head) => match self.entries.get(head) {
                    Some(h) => lines.push(format!(
                        "  head={} bones={} verts={}",
                        head,
                        h.bone_count(),
                        h.skel.positions.len()
                    )),
                    None => lines.push(format!("  head={head} missing skel")),
                },
                None => lines.push("  head=(none)".into()),
            }
        }
        if kits.allies.is_none() && kits.axis.is_none() && !self.entries.is_empty() {
            lines.push(format!(
                "bodies: {} captured models, no kit resolved",
                self.entries.len()
            ));
        }
        lines
    }

    pub fn kits_have_spine(&self) -> bool {
        let kits = self.kits();
        for kit in [kits.allies.as_ref(), kits.axis.as_ref()]
            .into_iter()
            .flatten()
        {
            let Some(body) = self.entries.get(&kit.body) else {
                return false;
            };
            if !body.has_spine_chain() {
                return false;
            }
        }
        kits.allies.is_some() || kits.axis.is_some()
    }

    pub fn material_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for entry in self.entries.values() {
            for edge in &entry.material_edges {
                census.push(*edge);
            }
        }
        census
    }

    pub fn material_unresolved_hints(&self) -> Vec<&str> {
        let mut names = Vec::new();
        for entry in self.entries.values() {
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
                .values()
                .flat_map(|entry| entry.material_edges.iter()),
        )
    }
}
