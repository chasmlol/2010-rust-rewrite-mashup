use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::sync::Arc;

use asset_iw4::size as sz;
pub use asset_model::{ModelKind, model_kind};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use fastfile_iw4::{GfxWorldGeometry, Ptr, XModelGeometry, ZoneError, ZonePtr, ZoneStream};
use lighting_iw4::GFX_STATIC_MODEL_DRAW_INST_FLAGS;

use crate::world_mesh::{normalize_or_up, unpack_color, unpack_packed_tex_coords, unpack_unit_vec};
use asset_core::ZoneOwner;
use asset_material::MaterialCatalog;
use asset_model::ModelSkel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexColorStats {
    pub vertices: u64,
    pub white: u64,
    pub alpha_zero: u64,
    pub min_rgba: [u8; 4],
    pub max_rgba: [u8; 4],
    pub sum_rgba: [u64; 4],
}

impl Default for VertexColorStats {
    fn default() -> Self {
        Self::empty()
    }
}

impl VertexColorStats {
    pub const fn empty() -> Self {
        Self {
            vertices: 0,
            white: 0,
            alpha_zero: 0,
            min_rgba: [255; 4],
            max_rgba: [0; 4],
            sum_rgba: [0; 4],
        }
    }

    pub fn accumulate_rgba_u8(&mut self, rgba: [u8; 4]) {
        self.vertices += 1;
        if rgba == [255, 255, 255, 255] {
            self.white += 1;
        }
        if rgba[3] == 0 {
            self.alpha_zero += 1;
        }
        for i in 0..4 {
            self.min_rgba[i] = self.min_rgba[i].min(rgba[i]);
            self.max_rgba[i] = self.max_rgba[i].max(rgba[i]);
            self.sum_rgba[i] += u64::from(rgba[i]);
        }
    }

    pub fn accumulate_rgba_f32(&mut self, rgba: [f32; 4]) {
        self.accumulate_rgba_u8(rgba_f32_to_u8(rgba));
    }

    pub fn merge(&mut self, other: &Self) {
        if other.vertices == 0 {
            return;
        }
        if self.vertices == 0 {
            *self = *other;
            return;
        }
        self.vertices += other.vertices;
        self.white += other.white;
        self.alpha_zero += other.alpha_zero;
        for i in 0..4 {
            self.min_rgba[i] = self.min_rgba[i].min(other.min_rgba[i]);
            self.max_rgba[i] = self.max_rgba[i].max(other.max_rgba[i]);
            self.sum_rgba[i] += other.sum_rgba[i];
        }
    }

    pub fn finish(mut self) -> Self {
        if self.vertices == 0 {
            self.min_rgba = [0; 4];
            self.max_rgba = [0; 4];
        }
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceColorCensus {
    pub surface_index: usize,
    pub material: Option<usize>,
    pub stats: VertexColorStats,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelColorCensus {
    pub name: String,
    pub stats: VertexColorStats,
    pub surfaces: Vec<SurfaceColorCensus>,
}

fn rgba_f32_to_u8(rgba: [f32; 4]) -> [u8; 4] {
    [
        (rgba[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (rgba[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (rgba[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        (rgba[3].clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

pub fn census_rgba_f32(colors: &[[f32; 4]]) -> VertexColorStats {
    let mut stats = VertexColorStats::empty();
    for &c in colors {
        stats.accumulate_rgba_f32(c);
    }
    stats.finish()
}

pub fn census_mesh_vertex_colors(mesh: &Mesh) -> Option<VertexColorStats> {
    let values = mesh.attribute(Mesh::ATTRIBUTE_COLOR)?;
    let colors = match values {
        bevy::render::mesh::VertexAttributeValues::Float32x4(v) => v.as_slice(),
        _ => return None,
    };
    Some(census_rgba_f32(colors))
}

pub fn census_model_mesh(model: &ModelMesh) -> ModelColorCensus {
    let mut surfaces = Vec::with_capacity(model.lod_surfaces[0].len());
    let mut total = VertexColorStats::empty();
    for (surface_index, surface) in model.lod_surfaces[0].iter().enumerate() {
        let stats = census_mesh_vertex_colors(&surface.mesh).unwrap_or_default();
        total.merge(&stats);
        surfaces.push(SurfaceColorCensus {
            surface_index,
            material: surface.material,
            stats,
        });
    }
    ModelColorCensus {
        name: model.name.clone(),
        stats: total.finish(),
        surfaces,
    }
}

pub fn census_skel_vertex_colors(
    name: impl Into<String>,
    colors: &[[f32; 4]],
    surface_vertex_ranges: &[(usize, usize)],
    surface_materials: &[Option<usize>],
) -> ModelColorCensus {
    let mut surfaces = Vec::with_capacity(surface_vertex_ranges.len());
    let mut total = VertexColorStats::empty();
    if surface_vertex_ranges.is_empty() {
        let stats = census_rgba_f32(colors);
        return ModelColorCensus {
            name: name.into(),
            stats,
            surfaces: vec![SurfaceColorCensus {
                surface_index: 0,
                material: surface_materials.first().copied().flatten(),
                stats,
            }],
        };
    }
    for (surface_index, &(first, count)) in surface_vertex_ranges.iter().enumerate() {
        let end = first.saturating_add(count).min(colors.len());
        let start = first.min(colors.len());
        let stats = census_rgba_f32(&colors[start..end]);
        total.merge(&stats);
        surfaces.push(SurfaceColorCensus {
            surface_index,
            material: surface_materials.get(surface_index).copied().flatten(),
            stats,
        });
    }
    ModelColorCensus {
        name: name.into(),
        stats: total.finish(),
        surfaces,
    }
}

pub fn format_vertex_color_stats(stats: &VertexColorStats) -> String {
    format!(
        "verts={} white={} alpha0={} min=[{},{},{},{}] max=[{},{},{},{}] sum=[{},{},{},{}]",
        stats.vertices,
        stats.white,
        stats.alpha_zero,
        stats.min_rgba[0],
        stats.min_rgba[1],
        stats.min_rgba[2],
        stats.min_rgba[3],
        stats.max_rgba[0],
        stats.max_rgba[1],
        stats.max_rgba[2],
        stats.max_rgba[3],
        stats.sum_rgba[0],
        stats.sum_rgba[1],
        stats.sum_rgba[2],
        stats.sum_rgba[3],
    )
}

#[derive(Clone, Debug)]
pub struct ModelSurfaceDraw {
    pub mesh: Mesh,

    pub material: Option<usize>,

    pub packed_vertices: Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,

    pub xsurface_plus_1: Option<u8>,

    pub xsurface_base_index: u16,

    pub xsurface_vert_offset: u16,

    pub collision: XSurfaceCollisionPayload,
}

#[derive(Clone, Debug)]
pub enum XSurfaceCollisionPayload {
    Iw4(Vec<OwnedXRigidVertListCollision>),
    Unavailable { source_layout: &'static str },
}

#[derive(Clone, Debug)]
pub struct OwnedXRigidVertListCollision {
    pub tri_offset: u16,
    pub tri_count: u16,
    pub tree: Option<OwnedXSurfaceCollisionTree>,
}

#[derive(Clone, Debug)]
pub struct OwnedXSurfaceCollisionTree {
    pub trans: [f32; 3],
    pub scale: [f32; 3],
    pub nodes: Vec<asset_iw4::XSurfaceCollisionNode>,
    pub leafs: Vec<asset_iw4::XSurfaceCollisionLeaf>,
}

#[derive(Clone, Debug)]
pub enum PackedVertexPayload {
    Iw4(Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>),
    Unavailable { source_layout: &'static str },
}

impl Default for PackedVertexPayload {
    fn default() -> Self {
        Self::Unavailable {
            source_layout: "packed vertex payload not installed",
        }
    }
}

fn lod0_only_surfaces(draws: Vec<ModelSurfaceDraw>) -> [Vec<ModelSurfaceDraw>; 4] {
    let mut lods: [Vec<ModelSurfaceDraw>; 4] = Default::default();
    lods[0] = draws;
    lods
}

#[derive(Clone, Debug)]
pub struct ModelMesh {
    pub name: String,

    pub lod_surfaces: [Vec<ModelSurfaceDraw>; 4],
    pub vertices: usize,
    pub triangles: usize,

    pub lod_smc: Option<[[u8; 4]; 4]>,

    pub lod: Option<asset_model::ModelLodSelector>,
}

impl ModelMesh {
    pub fn surface_count(&self) -> usize {
        self.lod_surfaces.iter().map(|lod| lod.len()).sum()
    }

    pub fn surfaces(&self) -> &[ModelSurfaceDraw] {
        &self.lod_surfaces[0]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct StaticModelInstance {
    pub model_slot: Ptr,
    pub origin: [f32; 3],

    pub axis: [[f32; 3]; 3],
    pub scale: f32,
    pub cull_dist: u16,

    pub reflection_probe_index: u8,

    pub primary_light_index: u8,

    pub flags: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct StaticModelPlacement {
    pub mesh: usize,
    pub transform: Transform,

    pub origin: [f32; 3],
    pub axis: [[f32; 3]; 3],
    pub scale: f32,
    pub cull_dist: u16,

    pub reflection_probe_index: u8,

    pub primary_light_index: u8,

    pub flags: u8,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MapXModelAssetKey(pub String);

impl Borrow<str> for MapXModelAssetKey {
    fn borrow(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug)]
pub enum MapXModelSceneAsset {
    Iw4(Arc<ModelSkel>),

    T5(Arc<ModelSkel>),

    Iw5(Arc<ModelSkel>),

    T6(Arc<ModelSkel>),
    Unavailable { reason: &'static str },
}

#[derive(Clone, Debug, Default, Resource)]
pub struct MapXModelSceneCatalog {
    assets: BTreeMap<MapXModelAssetKey, MapXModelSceneAsset>,

    order: Vec<MapXModelAssetKey>,

    zones: Vec<ZoneOwner>,
    capture_zone: ZoneOwner,

    phys_preset_slot_n: usize,

    phys_preset_name_hint_n: usize,

    phys_preset_n: usize,

    surface_materials: BTreeMap<MapXModelAssetKey, Vec<Option<crate::MaterialIndex>>>,
    resolved: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapXModelMaterialCensus {
    pub models: usize,
    pub surfaces: usize,
    pub bound: usize,
    pub unbound: usize,

    pub absent: usize,
}

impl MapXModelSceneCatalog {
    pub fn get(&self, key: &MapXModelAssetKey) -> Option<&MapXModelSceneAsset> {
        self.assets.get(key)
    }

    pub fn get_name(&self, name: &str) -> Option<&MapXModelSceneAsset> {
        self.assets.get(name)
    }

    pub fn resolve_surface_materials(
        &mut self,
        walk_local_ids: &[Option<usize>],
    ) -> MapXModelMaterialCensus {
        let mut census = MapXModelMaterialCensus::default();
        self.surface_materials.clear();
        for (key, asset) in &self.assets {
            let skel = match asset {
                MapXModelSceneAsset::Iw4(skel)
                | MapXModelSceneAsset::Iw5(skel)
                | MapXModelSceneAsset::T6(skel)
                | MapXModelSceneAsset::T5(skel) => skel,
                MapXModelSceneAsset::Unavailable { .. } => continue,
            };
            census.models += 1;
            let mut row = Vec::with_capacity(skel.surface_materials.len());
            for local in &skel.surface_materials {
                census.surfaces += 1;
                let Some(local) = local else {
                    census.absent += 1;
                    row.push(None);
                    continue;
                };
                match walk_local_ids.get(local.get()).copied().flatten() {
                    Some(order) => {
                        census.bound += 1;
                        row.push(Some(crate::MaterialIndex::from_order(order)));
                    }
                    None => {
                        census.unbound += 1;
                        row.push(None);
                    }
                }
            }
            self.surface_materials.insert(key.clone(), row);
        }
        self.resolved = true;
        census
    }

    pub fn materials_resolved(&self) -> bool {
        self.resolved
    }

    pub fn surface_material(
        &self,
        key: &MapXModelAssetKey,
        surface_index: usize,
    ) -> Option<crate::MaterialIndex> {
        self.surface_materials
            .get(key)?
            .get(surface_index)
            .copied()
            .flatten()
    }

    pub fn insert(&mut self, key: MapXModelAssetKey, asset: MapXModelSceneAsset) {
        if !self.assets.contains_key(&key) {
            self.order.push(key.clone());
            self.zones.push(self.capture_zone);
        }
        self.assets.entry(key).or_insert(asset);
    }

    pub fn remap_walk_materials(&mut self, donor_to_host: &[usize]) {
        for asset in self.assets.values_mut() {
            let (MapXModelSceneAsset::Iw4(skel)
            | MapXModelSceneAsset::T5(skel)
            | MapXModelSceneAsset::Iw5(skel)
            | MapXModelSceneAsset::T6(skel)) = asset
            else {
                continue;
            };
            let skel = Arc::make_mut(skel);
            for slot in &mut skel.surface_materials {
                *slot = slot.and_then(|local| {
                    donor_to_host
                        .get(local.get())
                        .map(|&host| asset_core::WalkLocalMaterialIndex::from_walk(host))
                });
            }
        }
    }

    pub fn absorb_captured(&mut self, mut earlier: Self) {
        for (index, key) in earlier.order.into_iter().enumerate() {
            let asset = earlier
                .assets
                .remove(&key)
                .expect("catalog insertion order");
            if matches!(asset, MapXModelSceneAsset::Unavailable { .. }) {
                continue;
            }
            match self.assets.entry(key.clone()) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    self.order.push(key);
                    self.zones.push(earlier.zones[index]);
                    entry.insert(asset);
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    if matches!(entry.get(), MapXModelSceneAsset::Unavailable { .. }) {
                        entry.insert(asset);
                        if let Some(slot) = self.order.iter().position(|k| k == &key) {
                            self.zones[slot] = earlier.zones[index];
                        }
                    }
                }
            }
        }
    }

    pub fn retain_names(&mut self, names: &std::collections::BTreeSet<String>) {
        self.assets.retain(|key, _| names.contains(&key.0));
        self.surface_materials
            .retain(|key, _| names.contains(&key.0));
        let mut at = 0;
        self.order.retain(|key| {
            let keep = names.contains(&key.0);
            if keep {
                at += 1;
            } else {
                self.zones.remove(at);
            }
            keep
        });
    }

    pub fn set_capture_zone(&mut self, zone: ZoneOwner) {
        self.capture_zone = zone;
    }

    pub fn alias(&mut self, from: &str, to: &str) -> bool {
        let Some(asset) = self
            .assets
            .get(to)
            .filter(|asset| !matches!(asset, MapXModelSceneAsset::Unavailable { .. }))
            .cloned()
        else {
            return false;
        };
        let zone = self
            .index_by_name(to)
            .map(|index| self.zone_of(index))
            .unwrap_or_default();
        let key = MapXModelAssetKey(from.to_owned());
        match self.order.iter().position(|k| k == &key) {
            Some(slot) => self.zones[slot] = zone,
            None => {
                self.order.push(key.clone());
                self.zones.push(zone);
            }
        }
        self.assets.insert(key, asset);
        true
    }

    pub fn zone_of(&self, index: usize) -> ZoneOwner {
        self.zones.get(index).copied().unwrap_or_default()
    }

    pub fn index_by_name(&self, name: &str) -> Option<usize> {
        let key = MapXModelAssetKey(name.to_owned());
        self.order.iter().position(|k| k == &key)
    }

    pub fn name_at(&self, index: usize) -> Option<&str> {
        self.order.get(index).map(|k| k.0.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&MapXModelAssetKey, &MapXModelSceneAsset)> {
        self.assets.iter()
    }

    pub fn len(&self) -> usize {
        self.assets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }

    pub fn phys_preset_slot_n(&self) -> usize {
        self.phys_preset_slot_n
    }

    pub fn phys_preset_name_hint_n(&self) -> usize {
        self.phys_preset_name_hint_n
    }

    pub fn phys_preset_n(&self) -> usize {
        self.phys_preset_n
    }

    pub fn set_dynent_phys_preset_n(&mut self, n: usize) {
        self.phys_preset_n = n;
    }

    pub fn note_xmodel_phys_preset(&mut self, slot: bool, named: bool) {
        if slot {
            self.phys_preset_slot_n += 1;
        }
        if named {
            self.phys_preset_name_hint_n += 1;
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScriptModelMetadata {
    pub gameobject: String,
    pub targetname: String,
    pub script_noteworthy: String,
    pub destructible_type: String,

    pub destructible_def: String,
    pub t5_destructible: Option<std::sync::Arc<xmodel_runtime::T5DestructibleDef>>,
    pub target: String,

    pub script_exploder: String,
    pub brush_link: crate::ScriptBrushModelLink,

    pub script_accumulate: Option<i32>,

    pub script_threshold: Option<i32>,

    pub script_destructable_area: String,

    pub script_fxid: String,
}

#[derive(Clone, Debug)]
pub struct ScriptModelSceneInstance {
    pub id: crate::ScriptModelId,
    pub current_model: MapXModelAssetKey,
    pub transform: Transform,

    pub lighting_origin: [f32; 3],
    pub dobj_state: xmodel_runtime::DObjSemanticState,
    pub metadata: ScriptModelMetadata,
}

#[derive(Default)]
pub struct StaticModelDraw {
    pub meshes: Vec<ModelMesh>,

    pub placements: Vec<Option<StaticModelPlacement>>,

    pub gaps: usize,
}

impl std::fmt::Debug for StaticModelDraw {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StaticModelDraw")
            .field("meshes", &self.meshes.len())
            .field("authored_slots", &self.placements.len())
            .field("resolved", &self.resolved_count())
            .field("gaps", &self.gaps)
            .finish()
    }
}

impl StaticModelDraw {
    pub fn unresolved(smodel_count: usize) -> Self {
        Self {
            meshes: Vec::new(),
            placements: vec![None; smodel_count],
            gaps: smodel_count,
        }
    }

    pub fn resolved_count(&self) -> usize {
        self.placements.iter().flatten().count()
    }
}

#[derive(Debug, Default)]
pub struct PreparedMapModels {
    pub static_draw: StaticModelDraw,
    pub static_error: Option<StaticModelDrawError>,
    pub scene_assets: MapXModelSceneCatalog,
    pub script_instances: Vec<ScriptModelSceneInstance>,

    pub script_brush_models: Vec<crate::ScriptBrushModelPlacement>,

    pub flag_descriptors: Vec<crate::FlagDescriptor>,

    pub script_structs: Vec<crate::MapScriptStruct>,

    pub script_gaps: usize,
}

#[derive(Debug)]
pub enum StaticModelDrawError {
    InstancesUnavailable { smodel_count: usize },

    PlacementCountMismatch { decoded: usize, smodel_count: usize },
}

impl std::fmt::Display for StaticModelDrawError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InstancesUnavailable { smodel_count } => write!(
                f,
                "smodelDrawInsts did not decode ({smodel_count} authored slots)"
            ),
            Self::PlacementCountMismatch {
                decoded,
                smodel_count,
            } => write!(
                f,
                "decoded {decoded} placements but smodelCount is {smodel_count}"
            ),
        }
    }
}

#[derive(Debug)]
pub enum ModelMeshError {
    MissingName,
    MissingSurfaces,
    MissingStaticModelInstances,
    InvalidIndex {
        surface: usize,
        index: u16,
        vertices: usize,
    },
    InvalidCollisionTree {
        surface: usize,
        vert_list: usize,
        reason: asset_iw4::XSurfaceCollisionRangeError,
    },
    Zone(ZoneError),
    T5Zone(fastfile_t5::ZoneError),
}

impl std::fmt::Display for ModelMeshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingName => write!(f, "XModel has no resolved name"),
            Self::MissingSurfaces => write!(f, "XModel LOD0 has no resolved surfaces"),
            Self::MissingStaticModelInstances => {
                write!(f, "GfxWorld has no static-model instance tables")
            }
            Self::InvalidIndex {
                surface,
                index,
                vertices,
            } => write!(
                f,
                "XModel surface {surface} index {index} exceeds {vertices} vertices"
            ),
            Self::InvalidCollisionTree {
                surface,
                vert_list,
                reason,
            } => write!(
                f,
                "XModel surface {surface} rigid vert list {vert_list} has invalid collision data: {reason:?}"
            ),
            Self::Zone(error) => write!(f, "reading XModel geometry: {error}"),
            Self::T5Zone(error) => write!(f, "reading T5 XModel geometry: {error}"),
        }
    }
}

pub fn build_static_model_instances(
    stream: &ZoneStream<'_>,
    world: GfxWorldGeometry,
) -> Result<Vec<StaticModelInstance>, ModelMeshError> {
    let (Some(draw_insts), Some(_insts)) = (world.smodel_draw_insts, world.smodel_insts) else {
        return Err(ModelMeshError::MissingStaticModelInstances);
    };
    let mut out = Vec::with_capacity(world.smodel_count);
    for index in 0..world.smodel_count {
        let inst = draw_insts.at(index * stream.layout(sz::GFX_STATIC_MODEL_DRAW_INST, 88));
        let mut axis = [[0.0; 3]; 3];
        for (row, values) in axis.iter_mut().enumerate() {
            for (column, value) in values.iter_mut().enumerate() {
                *value = stream.f32_at(inst, 12 + (row * 3 + column) * 4)?;
            }
        }
        out.push(StaticModelInstance {
            model_slot: inst.at(stream.layout(0x34, 56)),
            origin: [
                stream.f32_at(inst, 0)?,
                stream.f32_at(inst, 4)?,
                stream.f32_at(inst, 8)?,
            ],
            axis,
            scale: stream.f32_at(inst, 0x30)?,
            cull_dist: stream.u16_at(inst, stream.layout(0x38, 64))?,
            reflection_probe_index: stream.u8_at(inst, stream.layout(0x3c, 68))?,
            primary_light_index: stream.u8_at(inst, stream.layout(0x3d, 69))?,
            flags: stream.u8_at(inst, stream.layout(GFX_STATIC_MODEL_DRAW_INST_FLAGS, 70))?,
        });
    }
    Ok(out)
}

pub fn build_iw5_static_model_instances(
    stream: &fastfile_iw5::ZoneStream<'_>,
    world: fastfile_iw5::GfxWorldGeometry,
) -> Result<Vec<StaticModelInstance>, ModelMeshError> {
    let (Some(draw_insts), Some(_insts)) = (world.smodel_draw_insts, world.smodel_insts) else {
        return Err(ModelMeshError::MissingStaticModelInstances);
    };
    let draw_inst = stream.layout(fastfile_iw5::size::GFX_STATIC_MODEL_DRAW_INST, 88);
    let model_off = stream.layout(0x34, 56);
    let mut out = Vec::with_capacity(world.smodel_count);
    for index in 0..world.smodel_count {
        let inst = draw_insts.at(index * draw_inst);
        let mut axis = [[0.0; 3]; 3];
        for (row, values) in axis.iter_mut().enumerate() {
            for (column, value) in values.iter_mut().enumerate() {
                *value = stream
                    .f32_at(inst, 12 + (row * 3 + column) * 4)
                    .map_err(|_| ModelMeshError::MissingStaticModelInstances)?;
            }
        }
        out.push(StaticModelInstance {
            model_slot: Ptr {
                block: inst.block,
                offset: inst.offset + model_off as u32,
            },
            origin: [
                stream
                    .f32_at(inst, 0)
                    .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
                stream
                    .f32_at(inst, 4)
                    .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
                stream
                    .f32_at(inst, 8)
                    .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
            ],
            axis,
            scale: stream
                .f32_at(inst, 0x30)
                .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
            cull_dist: stream
                .u16_at(inst, stream.layout(0x38, 64))
                .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
            reflection_probe_index: stream
                .u8_at(inst, stream.layout(0x3c, 68))
                .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
            primary_light_index: stream
                .u8_at(inst, stream.layout(0x3d, 69))
                .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
            flags: stream
                .u8_at(inst, stream.layout(GFX_STATIC_MODEL_DRAW_INST_FLAGS, 70))
                .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
        });
    }
    Ok(out)
}

pub fn build_t5_static_model_instances(
    stream: &fastfile_t5::ZoneStream<'_>,
    world: fastfile_t5::GfxWorldGeometry,
) -> Result<Vec<StaticModelInstance>, ModelMeshError> {
    let (Some(draw_insts), Some(_insts)) = (world.smodel_draw_insts, world.smodel_insts) else {
        return Err(ModelMeshError::MissingStaticModelInstances);
    };
    const DRAW_INST: usize = 76;

    const CULL_DIST_OFF: usize = 0x00;
    const PLACEMENT_OFF: usize = 0x04;
    const ORIGIN_OFF: usize = PLACEMENT_OFF;
    const AXIS_OFF: usize = PLACEMENT_OFF + 12;
    const SCALE_OFF: usize = PLACEMENT_OFF + 0x30;
    const MODEL_OFF: usize = 0x38;
    let mut out = Vec::with_capacity(world.smodel_count);
    for index in 0..world.smodel_count {
        let inst = draw_insts.at(index * DRAW_INST);
        let mut axis = [[0.0; 3]; 3];
        for (row, values) in axis.iter_mut().enumerate() {
            for (column, value) in values.iter_mut().enumerate() {
                *value = stream
                    .f32_at(inst, AXIS_OFF + (row * 3 + column) * 4)
                    .map_err(|_| ModelMeshError::MissingStaticModelInstances)?;
            }
        }
        let model = match stream
            .ptr_at(inst, MODEL_OFF)
            .map_err(|_| ModelMeshError::MissingStaticModelInstances)?
        {
            fastfile_t5::ZonePtr::Offset(p) => p,
            _ => {
                continue;
            }
        };
        out.push(StaticModelInstance {
            model_slot: Ptr {
                block: model.block,
                offset: model.offset,
            },
            origin: [
                stream
                    .f32_at(inst, ORIGIN_OFF)
                    .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
                stream
                    .f32_at(inst, ORIGIN_OFF + 4)
                    .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
                stream
                    .f32_at(inst, ORIGIN_OFF + 8)
                    .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
            ],
            axis,
            scale: stream
                .f32_at(inst, SCALE_OFF)
                .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,

            cull_dist: {
                let dist = stream
                    .f32_at(inst, CULL_DIST_OFF)
                    .map_err(|_| ModelMeshError::MissingStaticModelInstances)?;
                if dist.is_finite() && (0.0..=f32::from(u16::MAX)).contains(&dist) {
                    dist as u16
                } else {
                    0
                }
            },

            reflection_probe_index: stream
                .u8_at(inst, 0x4a)
                .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,
            primary_light_index: stream
                .u8_at(inst, 0x4b)
                .map_err(|_| ModelMeshError::MissingStaticModelInstances)?,

            flags: 0,
        });
    }
    Ok(out)
}

impl std::error::Error for ModelMeshError {}

impl From<fastfile_t5::ZoneError> for ModelMeshError {
    fn from(error: fastfile_t5::ZoneError) -> Self {
        Self::T5Zone(error)
    }
}

impl From<ZoneError> for ModelMeshError {
    fn from(error: ZoneError) -> Self {
        Self::Zone(error)
    }
}

fn decode_iw4_xsurfaces(
    stream: &ZoneStream<'_>,
    surfaces: Ptr,
    surface_count: usize,
    material_handles: Option<Ptr>,
    handle_count: usize,
    surf_index: u16,
    materials: Option<&MaterialCatalog>,
) -> Result<(Vec<ModelSurfaceDraw>, usize, usize), ModelMeshError> {
    let mut draws = Vec::with_capacity(surface_count);
    let mut total_vertices = 0usize;
    let mut total_triangles = 0usize;
    let handle_base = usize::from(surf_index);
    for surface_index in 0..surface_count {
        let surface = surfaces.at(surface_index * stream.layout(sz::XSURFACE, 88));
        let vertex_count = stream.u16_at(surface, 2)? as usize;
        let tri_count = stream.u16_at(surface, 4)? as usize;
        let vertices = resolved_ptr(stream, surface, stream.layout(28, 40))?;
        let triangles = resolved_ptr(stream, surface, stream.layout(12, 16))?;

        let mut positions = Vec::with_capacity(vertex_count);
        let mut normals = Vec::with_capacity(vertex_count);
        let mut colors = Vec::with_capacity(vertex_count);
        let mut uvs = Vec::with_capacity(vertex_count);
        let mut packed_vertices = Vec::with_capacity(vertex_count);
        let mut indices = Vec::with_capacity(tri_count * 3);

        for vertex_index in 0..vertex_count {
            let vertex = vertices.at(vertex_index * sz::GFX_PACKED_VERTEX);
            let mut packed = [0u8; sz::GFX_PACKED_VERTEX];
            for (offset, byte) in packed.iter_mut().enumerate() {
                *byte = stream.u8_at(vertex, offset)?;
            }
            packed_vertices.push(packed);
            positions.push([
                stream.f32_at(vertex, 0)?,
                stream.f32_at(vertex, 4)?,
                stream.f32_at(vertex, 8)?,
            ]);
            colors.push(unpack_color(stream.u32_at(vertex, 16)?));
            uvs.push(unpack_packed_tex_coords(stream.u32_at(vertex, 20)?));
            normals.push(normalize_or_up(unpack_unit_vec(stream.u32_at(vertex, 24)?)));
        }
        for index_offset in 0..tri_count * 3 {
            let index = stream.u16_at(triangles, index_offset * 2)?;
            if usize::from(index) >= vertex_count {
                return Err(ModelMeshError::InvalidIndex {
                    surface: surface_index,
                    index,
                    vertices: vertex_count,
                });
            }
            indices.push(u32::from(index));
        }

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        mesh.insert_indices(Indices::U32(indices));

        let handle_slot = handle_base.saturating_add(surface_index);
        let material = if handle_slot < handle_count {
            material_handles
                .and_then(|handles| {
                    materials?.material_index(handles.at(handle_slot * stream.pointer_bytes()))
                })
                .map(|i| i.get())
        } else {
            None
        };

        total_vertices += vertex_count;
        total_triangles += tri_count;
        draws.push(ModelSurfaceDraw {
            mesh,
            material,
            packed_vertices,
            xsurface_plus_1: Some(stream.u8_at(surface, 1)?),
            xsurface_base_index: stream.u16_at(surface, 8)?,
            xsurface_vert_offset: stream.u16_at(surface, 10)?,
            collision: capture_iw4_xsurface_collision(stream, surface, surface_index, tri_count)?,
        });
    }
    Ok((draws, total_vertices, total_triangles))
}

pub fn build_xmodel_mesh(
    stream: &ZoneStream<'_>,
    geometry: XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Result<ModelMesh, ModelMeshError> {
    let name = geometry
        .name
        .map(|ptr| stream.cstr(ptr))
        .transpose()?
        .ok_or(ModelMeshError::MissingName)?
        .to_owned();

    let mut lod_surfaces: [Vec<ModelSurfaceDraw>; 4] = Default::default();
    let mut total_vertices = 0usize;
    let mut total_triangles = 0usize;
    let mut any = false;
    for lod in 0..4 {
        let Some(surfaces) = geometry.lod_xsurfaces[lod] else {
            continue;
        };
        let count = geometry.lod_numsurfs[lod] as usize;
        if count == 0 {
            continue;
        }
        let (draws, verts, tris) = decode_iw4_xsurfaces(
            stream,
            surfaces,
            count,
            geometry.material_handles,
            geometry.material_handle_count,
            geometry.lod_surf_index[lod],
            materials,
        )?;
        any = any || !draws.is_empty();
        total_vertices += verts;
        total_triangles += tris;
        lod_surfaces[lod] = draws;
    }
    if !any {
        return Err(ModelMeshError::MissingSurfaces);
    }

    Ok(ModelMesh {
        name,
        lod_surfaces,
        vertices: total_vertices,
        triangles: total_triangles,
        lod_smc: Some(geometry.lod_smc),
        lod: Some(asset_model::ModelLodSelector::Iw4 {
            lod_start: geometry.lod_start,
            num_lods: geometry.num_lods,
            lod_dist: geometry.lod_dist,
        }),
    })
}

pub fn build_t5_xmodel_mesh(
    stream: &fastfile_t5::ZoneStream<'_>,
    geometry: fastfile_t5::XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Result<ModelMesh, ModelMeshError> {
    let name = geometry
        .name
        .map(|ptr| stream.cstr(ptr))
        .transpose()
        .map_err(|_| ModelMeshError::MissingName)?
        .ok_or(ModelMeshError::MissingName)?
        .to_owned();
    let surfaces = geometry.surfaces.ok_or(ModelMeshError::MissingSurfaces)?;

    let mut lod_surfaces: [Vec<ModelSurfaceDraw>; 4] = Default::default();
    let mut total_vertices = 0usize;
    let mut total_triangles = 0usize;
    for lod in 0..4 {
        let (first_surface, surface_count) = geometry.lod_surf_span[lod];
        let first_surface = usize::from(first_surface);
        if surface_count == 0 || first_surface + usize::from(surface_count) > geometry.surface_count
        {
            continue;
        }
        let (draws, verts, tris) = decode_t5_xsurfaces(
            stream,
            surfaces,
            first_surface,
            usize::from(surface_count),
            geometry.material_handles,
            materials,
        )?;
        total_vertices += verts;
        total_triangles += tris;
        lod_surfaces[lod] = draws;
    }
    if lod_surfaces.iter().all(|lod| lod.is_empty()) {
        return Err(ModelMeshError::MissingSurfaces);
    }

    Ok(ModelMesh {
        name,
        lod_surfaces,
        vertices: total_vertices,
        triangles: total_triangles,
        lod_smc: None,
        lod: Some(asset_model::ModelLodSelector::T5 {
            num_lods: geometry.num_lods,
            lod_dist: geometry.lod_dist,
        }),
    })
}

#[allow(clippy::type_complexity)]
fn decode_t5_xsurfaces(
    stream: &fastfile_t5::ZoneStream<'_>,
    surfaces: fastfile_t5::Ptr,
    first_surface: usize,
    surface_count: usize,
    material_handles: Option<fastfile_t5::Ptr>,
    materials: Option<&MaterialCatalog>,
) -> Result<(Vec<ModelSurfaceDraw>, usize, usize), ModelMeshError> {
    const XSURFACE: usize = 68;
    const VERT_COUNT_OFF: usize = 4;
    const TRI_COUNT_OFF: usize = 6;
    const TRIANGLES_OFF: usize = 12;
    const VERTICES_OFF: usize = 32;
    const PACKED_VERTEX: usize = 32;

    let end_surface = first_surface + surface_count;
    let mut draws = Vec::with_capacity(surface_count);
    let mut total_vertices = 0usize;
    let mut total_triangles = 0usize;

    for surface_index in first_surface..end_surface {
        let surface = surfaces.at(surface_index * XSURFACE);
        let vertex_count = stream
            .u16_at(surface, VERT_COUNT_OFF)
            .map_err(|_| ModelMeshError::MissingSurfaces)? as usize;
        let tri_count = stream
            .u16_at(surface, TRI_COUNT_OFF)
            .map_err(|_| ModelMeshError::MissingSurfaces)? as usize;
        let vertices = resolved_ptr_t5(stream, surface, VERTICES_OFF)?;
        let triangles = resolved_ptr_t5(stream, surface, TRIANGLES_OFF)?;

        let mut positions = Vec::with_capacity(vertex_count);
        let mut normals = Vec::with_capacity(vertex_count);
        let mut colors = Vec::with_capacity(vertex_count);
        let mut uvs = Vec::with_capacity(vertex_count);
        let mut packed_vertices = Vec::with_capacity(vertex_count);
        let mut indices = Vec::with_capacity(tri_count * 3);

        for vertex_index in 0..vertex_count {
            let vertex = vertices.at(vertex_index * PACKED_VERTEX);
            let mut packed = [0u8; PACKED_VERTEX];
            for (offset, byte) in packed.iter_mut().enumerate() {
                *byte = stream
                    .u8_at(vertex, offset)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?;
            }
            packed_vertices.push(packed);
            positions.push([
                stream
                    .f32_at(vertex, 0)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?,
                stream
                    .f32_at(vertex, 4)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?,
                stream
                    .f32_at(vertex, 8)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?,
            ]);
            colors.push(unpack_color(
                stream
                    .u32_at(vertex, 16)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?,
            ));
            uvs.push(unpack_packed_tex_coords(
                stream
                    .u32_at(vertex, 20)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?,
            ));
            normals.push(normalize_or_up(unpack_unit_vec(
                stream
                    .u32_at(vertex, 24)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?,
            )));
        }
        for index_offset in 0..tri_count * 3 {
            let index = stream
                .u16_at(triangles, index_offset * 2)
                .map_err(|_| ModelMeshError::MissingSurfaces)?;
            if usize::from(index) >= vertex_count {
                return Err(ModelMeshError::InvalidIndex {
                    surface: surface_index,
                    index,
                    vertices: vertex_count,
                });
            }
            indices.push(u32::from(index));
        }

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        mesh.insert_indices(Indices::U32(indices));

        let material = material_handles.and_then(|handles| {
            let slot = Ptr {
                block: handles.block,
                offset: handles.offset + (surface_index * 4) as u32,
            };
            materials?.material_index(slot).map(|i| i.get())
        });

        total_vertices += vertex_count;
        total_triangles += tri_count;
        draws.push(ModelSurfaceDraw {
            mesh,
            material,
            packed_vertices,

            xsurface_plus_1: Some(u8::from(
                stream
                    .u16_at(surface, 2)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?
                    & 0x80
                    != 0,
            )),
            xsurface_base_index: 0,
            xsurface_vert_offset: 0,
            collision: capture_t5_xsurface_collision(stream, surface, surface_index, tri_count)?,
        });
    }

    Ok((draws, total_vertices, total_triangles))
}

pub fn build_iw5_xmodel_mesh(
    stream: &fastfile_iw5::ZoneStream<'_>,
    geometry: fastfile_iw5::XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Result<ModelMesh, ModelMeshError> {
    use fastfile_iw5::size as iw5sz;

    let name = geometry
        .name
        .map(|ptr| stream.cstr(ptr))
        .transpose()
        .map_err(|_| ModelMeshError::MissingName)?
        .ok_or(ModelMeshError::MissingName)?
        .to_owned();
    let surfaces = geometry.surfaces.ok_or(ModelMeshError::MissingSurfaces)?;

    let mut draws = Vec::with_capacity(geometry.surface_count);
    let mut total_vertices = 0usize;
    let mut total_triangles = 0usize;

    for surface_index in 0..geometry.surface_count {
        let surface = surfaces.at(surface_index * stream.layout(iw5sz::XSURFACE, 88));
        let vertex_count = stream
            .u16_at(surface, 2)
            .map_err(|_| ModelMeshError::MissingSurfaces)? as usize;
        let tri_count = stream
            .u16_at(surface, 4)
            .map_err(|_| ModelMeshError::MissingSurfaces)? as usize;
        let vertices = resolved_ptr_iw5(
            stream,
            surface,
            stream.layout(iw5sz::XSURFACE_VERTS0_OFF, 40),
        )?;
        let triangles = resolved_ptr_iw5(stream, surface, iw5sz::XSURFACE_TRI_INDICES_OFF)?;
        let xsurface_flags = stream
            .u8_at(surface, iw5sz::XSURFACE_FLAGS_OFF)
            .map_err(|_| ModelMeshError::MissingSurfaces)?;

        let mut positions = Vec::with_capacity(vertex_count);
        let mut normals = Vec::with_capacity(vertex_count);
        let mut colors = Vec::with_capacity(vertex_count);
        let mut uvs = Vec::with_capacity(vertex_count);
        let mut packed_vertices = Vec::with_capacity(vertex_count);
        let mut indices = Vec::with_capacity(tri_count * 3);

        for vertex_index in 0..vertex_count {
            let vertex = vertices.at(vertex_index * iw5sz::GFX_PACKED_VERTEX);
            let mut packed = [0u8; iw5sz::GFX_PACKED_VERTEX];
            for (offset, byte) in packed.iter_mut().enumerate() {
                *byte = stream
                    .u8_at(vertex, offset)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?;
            }
            packed_vertices.push(packed);
            positions.push([
                stream
                    .f32_at(vertex, 0)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?,
                stream
                    .f32_at(vertex, 4)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?,
                stream
                    .f32_at(vertex, 8)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?,
            ]);
            colors.push(unpack_color(
                stream
                    .u32_at(vertex, 16)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?,
            ));
            uvs.push(unpack_packed_tex_coords(
                stream
                    .u32_at(vertex, 20)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?,
            ));
            normals.push(normalize_or_up(unpack_unit_vec(
                stream
                    .u32_at(vertex, 24)
                    .map_err(|_| ModelMeshError::MissingSurfaces)?,
            )));
        }
        for index_offset in 0..tri_count * 3 {
            let index = stream
                .u16_at(triangles, index_offset * 2)
                .map_err(|_| ModelMeshError::MissingSurfaces)?;
            if usize::from(index) >= vertex_count {
                return Err(ModelMeshError::InvalidIndex {
                    surface: surface_index,
                    index,
                    vertices: vertex_count,
                });
            }
            indices.push(u32::from(index));
        }

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        mesh.insert_indices(Indices::U32(indices));

        let material = geometry.material_handles.and_then(|handles| {
            let slot = Ptr {
                block: handles.block,
                offset: handles.offset + (surface_index * stream.pointer_bytes()) as u32,
            };
            materials?.material_index(slot).map(|i| i.get())
        });

        total_vertices += vertex_count;
        total_triangles += tri_count;
        draws.push(ModelSurfaceDraw {
            mesh,
            material,
            packed_vertices,
            xsurface_plus_1: Some(u8::from(
                xsurface_flags & iw5sz::XSURFACE_FLAG_DEFORMED != 0,
            )),
            xsurface_base_index: 0,
            xsurface_vert_offset: 0,
            collision: XSurfaceCollisionPayload::Unavailable {
                source_layout: "IW5 XSurface collision layout not retained",
            },
        });
    }

    Ok(ModelMesh {
        name,
        lod_surfaces: lod0_only_surfaces(draws),
        vertices: total_vertices,
        triangles: total_triangles,
        lod_smc: None,

        lod: None,
    })
}

fn resolved_ptr(
    stream: &ZoneStream<'_>,
    parent: fastfile_iw4::Ptr,
    field: usize,
) -> Result<fastfile_iw4::Ptr, ModelMeshError> {
    match stream.ptr_at(parent, field)? {
        ZonePtr::Offset(ptr) => Ok(stream.resolve_alias(ptr)),
        _ => Err(ModelMeshError::MissingSurfaces),
    }
}

fn capture_iw4_xsurface_collision(
    stream: &ZoneStream<'_>,
    surface: Ptr,
    surface_index: usize,
    surface_tri_count: usize,
) -> Result<XSurfaceCollisionPayload, ModelMeshError> {
    let count = stream.u32_at(surface, stream.layout(32, 48))? as usize;
    if count == 0 {
        return Ok(XSurfaceCollisionPayload::Iw4(Vec::new()));
    }
    let lists = resolved_ptr(stream, surface, stream.layout(36, 56))?;
    stream.slice_at(
        lists,
        0,
        count * stream.layout(asset_iw4::size::XRIGID_VERT_LIST, 16),
    )?;
    let mut trees = Vec::with_capacity(count);
    for list_index in 0..count {
        let list = lists.at(list_index * stream.layout(asset_iw4::size::XRIGID_VERT_LIST, 16));
        let tri_offset = stream.u16_at(list, 4)?;
        let tri_count = stream.u16_at(list, 6)?;
        let tri_end = usize::from(tri_offset)
            .checked_add(usize::from(tri_count))
            .filter(|end| *end <= surface_tri_count)
            .ok_or(ModelMeshError::InvalidCollisionTree {
                surface: surface_index,
                vert_list: list_index,
                reason: asset_iw4::XSurfaceCollisionRangeError::RigidTriangleRange,
            })?;
        let tree = match stream.ptr_at(list, 8)? {
            ZonePtr::Null => None,
            ZonePtr::Offset(tree) => {
                let tree = capture_iw4_collision_tree(stream, stream.resolve_alias(tree))?;
                asset_iw4::validate_xsurface_collision_ranges(
                    &tree.nodes,
                    &tree.leafs,
                    usize::from(tri_offset),
                    tri_end,
                )
                .map_err(|reason| ModelMeshError::InvalidCollisionTree {
                    surface: surface_index,
                    vert_list: list_index,
                    reason,
                })?;
                Some(tree)
            }
            ZonePtr::Following | ZonePtr::Insert => return Err(ModelMeshError::MissingSurfaces),
        };
        trees.push(OwnedXRigidVertListCollision {
            tri_offset,
            tri_count,
            tree,
        });
    }
    Ok(XSurfaceCollisionPayload::Iw4(trees))
}

fn capture_iw4_collision_tree(
    stream: &ZoneStream<'_>,
    tree: Ptr,
) -> Result<OwnedXSurfaceCollisionTree, ModelMeshError> {
    let mut trans = [0.0; 3];
    let mut scale = [0.0; 3];
    for axis in 0..3 {
        trans[axis] = stream.f32_at(tree, axis * 4)?;
        scale[axis] = stream.f32_at(tree, 12 + axis * 4)?;
    }
    let node_count = stream.u32_at(tree, 24)? as usize;
    let leaf_count = stream.u32_at(tree, stream.layout(32, 40))? as usize;
    let nodes_ptr = if node_count == 0 {
        None
    } else {
        let rows = resolved_ptr(stream, tree, stream.layout(28, 32))?;
        stream.slice_at(
            rows,
            0,
            node_count * asset_iw4::size::XSURFACE_COLLISION_NODE,
        )?;
        Some(rows)
    };
    let leafs_ptr = if leaf_count == 0 {
        None
    } else {
        let rows = resolved_ptr(stream, tree, stream.layout(36, 48))?;
        stream.slice_at(
            rows,
            0,
            leaf_count * asset_iw4::size::XSURFACE_COLLISION_LEAF,
        )?;
        Some(rows)
    };
    let mut nodes = Vec::with_capacity(node_count);
    if let Some(rows) = nodes_ptr {
        for index in 0..node_count {
            let row = rows.at(index * asset_iw4::size::XSURFACE_COLLISION_NODE);
            nodes.push(asset_iw4::XSurfaceCollisionNode {
                mins: [
                    stream.u16_at(row, 0)?,
                    stream.u16_at(row, 2)?,
                    stream.u16_at(row, 4)?,
                ],
                maxs: [
                    stream.u16_at(row, 6)?,
                    stream.u16_at(row, 8)?,
                    stream.u16_at(row, 10)?,
                ],
                child_begin_index: stream.u16_at(row, 12)?,
                child_count: stream.u16_at(row, 14)?,
            });
        }
    }
    let mut leafs = Vec::with_capacity(leaf_count);
    if let Some(rows) = leafs_ptr {
        for index in 0..leaf_count {
            leafs.push(asset_iw4::XSurfaceCollisionLeaf {
                triangle_begin_index: stream
                    .u16_at(rows, index * asset_iw4::size::XSURFACE_COLLISION_LEAF)?,
            });
        }
    }
    Ok(OwnedXSurfaceCollisionTree {
        trans,
        scale,
        nodes,
        leafs,
    })
}

fn capture_t5_xsurface_collision(
    stream: &fastfile_t5::ZoneStream<'_>,
    surface: fastfile_t5::Ptr,
    surface_index: usize,
    surface_tri_count: usize,
) -> Result<XSurfaceCollisionPayload, ModelMeshError> {
    let count = stream.u8_at(surface, 1)? as usize;
    if count == 0 {
        return Ok(XSurfaceCollisionPayload::Iw4(Vec::new()));
    }
    let lists = resolved_ptr_t5(stream, surface, 40)?;
    stream.slice_at(lists, 0, count * fastfile_t5::size::XRIGID_VERT_LIST)?;
    let mut trees = Vec::with_capacity(count);
    for list_index in 0..count {
        let list = lists.at(list_index * fastfile_t5::size::XRIGID_VERT_LIST);
        let tri_offset = stream.u16_at(list, 4)?;
        let tri_count = stream.u16_at(list, 6)?;
        let tri_end = usize::from(tri_offset)
            .checked_add(usize::from(tri_count))
            .filter(|end| *end <= surface_tri_count)
            .ok_or(ModelMeshError::InvalidCollisionTree {
                surface: surface_index,
                vert_list: list_index,
                reason: asset_iw4::XSurfaceCollisionRangeError::RigidTriangleRange,
            })?;
        let tree = match stream.ptr_at(list, 8)? {
            fastfile_t5::ZonePtr::Null => None,
            fastfile_t5::ZonePtr::Offset(tree) => {
                let tree = capture_t5_collision_tree(stream, stream.resolve_alias(tree))?;
                asset_iw4::validate_xsurface_collision_ranges(
                    &tree.nodes,
                    &tree.leafs,
                    usize::from(tri_offset),
                    tri_end,
                )
                .map_err(|reason| ModelMeshError::InvalidCollisionTree {
                    surface: surface_index,
                    vert_list: list_index,
                    reason,
                })?;
                Some(tree)
            }
            fastfile_t5::ZonePtr::Following | fastfile_t5::ZonePtr::Insert => {
                return Err(ModelMeshError::MissingSurfaces);
            }
        };
        trees.push(OwnedXRigidVertListCollision {
            tri_offset,
            tri_count,
            tree,
        });
    }
    Ok(XSurfaceCollisionPayload::Iw4(trees))
}

fn capture_t5_collision_tree(
    stream: &fastfile_t5::ZoneStream<'_>,
    tree: fastfile_t5::Ptr,
) -> Result<OwnedXSurfaceCollisionTree, ModelMeshError> {
    let mut trans = [0.0; 3];
    let mut scale = [0.0; 3];
    for axis in 0..3 {
        trans[axis] = stream.f32_at(tree, axis * 4)?;
        scale[axis] = stream.f32_at(tree, 12 + axis * 4)?;
    }
    let node_count = stream.u32_at(tree, 24)? as usize;
    let leaf_count = stream.u32_at(tree, 32)? as usize;
    let nodes_ptr = if node_count == 0 {
        None
    } else {
        let rows = resolved_ptr_t5(stream, tree, 28)?;
        stream.slice_at(
            rows,
            0,
            node_count * asset_iw4::size::XSURFACE_COLLISION_NODE,
        )?;
        Some(rows)
    };
    let leafs_ptr = if leaf_count == 0 {
        None
    } else {
        let rows = resolved_ptr_t5(stream, tree, 36)?;
        stream.slice_at(
            rows,
            0,
            leaf_count * asset_iw4::size::XSURFACE_COLLISION_LEAF,
        )?;
        Some(rows)
    };
    let mut nodes = Vec::with_capacity(node_count);
    if let Some(rows) = nodes_ptr {
        for index in 0..node_count {
            let row = rows.at(index * asset_iw4::size::XSURFACE_COLLISION_NODE);
            nodes.push(asset_iw4::XSurfaceCollisionNode {
                mins: [
                    stream.u16_at(row, 0)?,
                    stream.u16_at(row, 2)?,
                    stream.u16_at(row, 4)?,
                ],
                maxs: [
                    stream.u16_at(row, 6)?,
                    stream.u16_at(row, 8)?,
                    stream.u16_at(row, 10)?,
                ],
                child_begin_index: stream.u16_at(row, 12)?,
                child_count: stream.u16_at(row, 14)?,
            });
        }
    }
    let mut leafs = Vec::with_capacity(leaf_count);
    if let Some(rows) = leafs_ptr {
        for index in 0..leaf_count {
            leafs.push(asset_iw4::XSurfaceCollisionLeaf {
                triangle_begin_index: stream
                    .u16_at(rows, index * asset_iw4::size::XSURFACE_COLLISION_LEAF)?,
            });
        }
    }
    Ok(OwnedXSurfaceCollisionTree {
        trans,
        scale,
        nodes,
        leafs,
    })
}

fn resolved_ptr_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    parent: fastfile_t5::Ptr,
    field: usize,
) -> Result<fastfile_t5::Ptr, ModelMeshError> {
    match stream
        .ptr_at(parent, field)
        .map_err(|_| ModelMeshError::MissingSurfaces)?
    {
        fastfile_t5::ZonePtr::Offset(ptr) => Ok(stream.resolve_alias(ptr)),
        _ => Err(ModelMeshError::MissingSurfaces),
    }
}

fn resolved_ptr_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    parent: fastfile_iw5::Ptr,
    field: usize,
) -> Result<fastfile_iw5::Ptr, ModelMeshError> {
    match stream
        .ptr_at(parent, field)
        .map_err(|_| ModelMeshError::MissingSurfaces)?
    {
        fastfile_iw5::ZonePtr::Offset(ptr) => Ok(stream.resolve_alias(ptr)),
        _ => Err(ModelMeshError::MissingSurfaces),
    }
}
