use bevy::prelude::*;
use render_frame::SmodelVertex;
use render_scene::{SmodelPassMaterial, XModelSurfaceDraw};

pub use render_frame::SourceRevisions;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FpvSurfaceDraw {
    pub surface: u32,
    pub material: u32,

    pub is_scope: bool,
    /// A surface of the hands model rather than the gun's.
    pub hands: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RemoteBodySurfaceDraw {
    pub surface: u32,
    pub material: u32,
    pub world_from_local: Mat4,

    pub lighting_handle: u32,

    pub scene_light_index: u8,

    pub reflection_probe_index: u8,

    pub scene_entnum: Option<u32>,
}

pub const FPV_PACKED_UNAVAILABLE: &str =
    "fpv GfxPackedVertex missing or count-mismatched; decoded float is not packed VB";
pub const FPV_PACKED_EMPTY_PLAN: &str = "fpv plan has no vertices";

pub const BODY_PACKED_UNAVAILABLE: &str = "remote body GfxPackedVertex not retained";

pub const XMODEL_OBJECT_ID_BODY_BASE: u16 = 2;

pub const XMODEL_OBJECT_ID_MISSILE_BASE: u16 = 0x400;

pub const XMODEL_OBJECT_ID_ITEM_BASE: u16 = 0x500;

pub const XMODEL_OBJECT_ID_DYNENT_BASE: u16 = 0x600;

#[derive(Resource, Clone, Debug, Default)]
pub struct RemoteBodyDrawPlan {
    pub(crate) vertices: Vec<SmodelVertex>,

    pub(crate) decoded_n: usize,
    pub(crate) indices: Vec<u32>,
    pub(crate) surface_ranges: Vec<(u32, u32)>,
    pub(crate) materials: Vec<SmodelPassMaterial>,
    pub(crate) draws: Vec<RemoteBodySurfaceDraw>,

    pub revision: u64,

    pub generation: u64,

    pub revisions: SourceRevisions,

    pub(crate) packed_vertices: asset_world::PackedVertexPayload,

    pub last_packed_id: Option<u64>,
    pub last_draw_id: Option<u64>,
}

impl RemoteBodyDrawPlan {
    /// Drops everything this plan published. Dropping rows is a change like any
    /// other: a consumer that kept last frame's revision would go on drawing
    /// bodies this plan no longer has.
    pub fn clear_geometry(&mut self) {
        let had_rows = !self.draws.is_empty() || self.decoded_n != 0;
        self.vertices.clear();
        self.decoded_n = 0;
        self.indices.clear();
        self.surface_ranges.clear();
        self.materials.clear();
        self.draws.clear();
        self.packed_vertices = asset_world::PackedVertexPayload::Unavailable {
            source_layout: BODY_PACKED_UNAVAILABLE,
        };
        self.last_packed_id = None;
        self.last_draw_id = None;
        if had_rows {
            self.revisions.set_topology_from(&[], &[], 0);
            self.revisions.bump_surfaces();
            self.revisions.bump_draws();
            self.revisions.bump_admission();
            self.revision = self.revision.wrapping_add(1);
        }
    }
}

#[derive(Clone, Debug)]
pub struct ScriptModelAssetDraw {
    pub key: asset_world::MapXModelAssetKey,
    pub dobj_state: xmodel_runtime::DObjSemanticState,

    pub camera_lods: Vec<Option<u8>>,
    pub surfaces: Vec<(u32, u32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScriptModelOwnerDraw {
    pub entity: Entity,
    pub current_model: asset_world::MapXModelAssetKey,
    pub object_id: u16,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct ScriptModelDrawPlan {
    pub(crate) vertices: Vec<SmodelVertex>,
    pub(crate) indices: Vec<u32>,
    pub(crate) surface_ranges: Vec<(u32, u32)>,
    pub(crate) materials: Vec<SmodelPassMaterial>,
    pub(crate) assets: Vec<ScriptModelAssetDraw>,
    pub(crate) owners: Vec<ScriptModelOwnerDraw>,
    pub(crate) draws: Vec<XModelSurfaceDraw>,
    pub revision: u64,
    pub generation: u64,
    pub revisions: SourceRevisions,
    pub(crate) packed_vertices: asset_world::PackedVertexPayload,
}

impl ScriptModelDrawPlan {
    pub fn asset_index(
        &self,
        key: &asset_world::MapXModelAssetKey,
        dobj_state: &xmodel_runtime::DObjSemanticState,
        camera_lods: &[Option<u8>],
    ) -> Option<usize> {
        self.assets.iter().position(|asset| {
            &asset.key == key && &asset.dobj_state == dobj_state && asset.camera_lods == camera_lods
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MissileOwnerDraw {
    pub object_id: u16,
    pub model: String,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct MissileDrawPlan {
    pub(crate) vertices: Vec<SmodelVertex>,
    pub(crate) indices: Vec<u32>,
    pub(crate) surface_ranges: Vec<(u32, u32)>,
    pub(crate) materials: Vec<SmodelPassMaterial>,
    pub(crate) owners: Vec<MissileOwnerDraw>,
    pub(crate) draws: Vec<XModelSurfaceDraw>,
    pub revision: u64,
    pub generation: u64,
    pub revisions: SourceRevisions,
    pub(crate) packed_vertices: asset_world::PackedVertexPayload,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemOwnerDraw {
    pub object_id: u16,
    pub model: String,
}

#[derive(Clone, Debug)]
pub struct ItemAssetDraw {
    pub model: String,
    pub surfaces: Vec<(u32, u32)>,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct ItemDrawPlan {
    pub(crate) vertices: Vec<SmodelVertex>,
    pub(crate) indices: Vec<u32>,
    pub(crate) surface_ranges: Vec<(u32, u32)>,
    pub(crate) materials: Vec<SmodelPassMaterial>,
    pub(crate) assets: Vec<ItemAssetDraw>,
    pub(crate) owners: Vec<ItemOwnerDraw>,
    pub(crate) draws: Vec<XModelSurfaceDraw>,
    pub revision: u64,
    pub generation: u64,
    pub revisions: SourceRevisions,
    pub(crate) packed_vertices: asset_world::PackedVertexPayload,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DynEntOwnerDraw {
    pub object_id: u16,
    pub model: String,
}

#[derive(Clone, Debug)]
pub struct DynEntAssetDraw {
    pub key: asset_world::MapXModelAssetKey,

    pub camera_lod: Option<u8>,
    pub surfaces: Vec<(u32, u32)>,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct DynEntDrawPlan {
    pub(crate) vertices: Vec<SmodelVertex>,
    pub(crate) indices: Vec<u32>,
    pub(crate) surface_ranges: Vec<(u32, u32)>,
    pub(crate) materials: Vec<SmodelPassMaterial>,
    pub(crate) assets: Vec<DynEntAssetDraw>,
    pub(crate) owners: Vec<DynEntOwnerDraw>,
    pub(crate) draws: Vec<XModelSurfaceDraw>,
    pub revision: u64,
    pub generation: u64,
    pub revisions: SourceRevisions,
    pub(crate) packed_vertices: asset_world::PackedVertexPayload,
}

impl DynEntDrawPlan {
    pub fn asset(
        &self,
        key: &asset_world::MapXModelAssetKey,
        camera_lod: Option<u8>,
    ) -> Option<&DynEntAssetDraw> {
        self.assets
            .iter()
            .find(|asset| &asset.key == key && asset.camera_lod == camera_lod)
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct FpvDrawPlan {
    /// How many vertices the packed rows below carry.
    pub(crate) decoded_n: usize,
    pub(crate) indices: Vec<u32>,
    pub(crate) surface_ranges: Vec<(u32, u32)>,
    pub(crate) materials: Vec<SmodelPassMaterial>,
    pub(crate) draws: Vec<FpvSurfaceDraw>,
    /// The draws were cut down to the hands alone.
    pub(crate) hands_only: bool,

    pub world_from_local: Mat4,
    pub lighting_handle: u32,
    pub visible: bool,

    pub hands_plan_n: Option<u32>,
    pub gun_plan_n: Option<u32>,

    pub gun_colormap_skip_n: Option<u32>,

    pub gun_ordinal_skip_n: Option<u32>,

    pub gun_colormap_skip_names: Option<String>,

    pub scope_plan_n: Option<u32>,

    pub scope_house_plan_n: Option<u32>,

    pub scope_lens_plan_n: Option<u32>,

    pub plan_draw_n: Option<u32>,

    pub plan_mat_hints: Option<String>,

    pub plan_skip_n: Option<u32>,

    pub drawgun: Option<i32>,

    pub scene_light_index: u8,

    pub reflection_probe_index: u8,

    pub revision: u64,
    pub generation: u64,
    pub revisions: SourceRevisions,

    /// Whether this frame's placement matrix is real. Placement and geometry
    /// are published by two independent systems, each stating its own half for
    /// `settle_visible` to read.
    pub placement_ok: bool,

    /// Whether the rig skinned rows into this plan this frame.
    pub geometry_ok: bool,

    /// Which prepared rig the rows below came from. The rig publishes them
    /// once; a frame that still answers this generation publishes vertices.
    pub rig_generation: u64,

    pub camo: u8,

    pub(crate) packed_vertices: asset_world::PackedVertexPayload,
}

impl FpvDrawPlan {
    #[must_use]
    pub fn admits_colour(&self) -> bool {
        self.visible && self.lighting_handle != 0 && self.drawgun != Some(0)
    }

    pub fn decoded_n(&self) -> usize {
        self.decoded_n
    }

    /// The rows this frame skinned into. `None` once the plan has nothing
    /// packed to write — an empty plan, or a composition whose models never
    /// carried packed vertices.
    pub fn packed_rows_mut(&mut self) -> Option<&mut [[u8; asset_iw4::size::GFX_PACKED_VERTEX]]> {
        match &mut self.packed_vertices {
            asset_world::PackedVertexPayload::Iw4(rows) => Some(rows.as_mut_slice()),
            asset_world::PackedVertexPayload::Unavailable { .. } => None,
        }
    }

    /// Both halves have to hold: a plan with rows nobody can place is no more
    /// drawable than a placement with nothing to draw.
    pub fn settle_visible(&mut self) {
        self.visible = self.geometry_ok && self.placement_ok && self.drawgun != Some(0);
    }
}

pub fn topology_fingerprint(indices: &[u32], ranges: &[(u32, u32)], decoded_n: usize) -> u64 {
    let mut revisions = SourceRevisions::default();
    revisions.set_topology_from(indices, ranges, decoded_n);
    revisions.topology
}

pub fn stamp_plan_geometry(revisions: &mut SourceRevisions, revision: u64, topology: u64) -> u64 {
    if revisions.topology == topology {
        return revision;
    }
    revisions.topology = topology;
    revisions.bump_draws();
    revision.wrapping_add(1)
}

fn apply_plan<O: HasObjectId>(
    resolved: &render_scene::model_lighting::ResolvedModelLightingTable,
    draws: &mut Vec<XModelSurfaceDraw>,
    owners: &mut Vec<O>,
    revisions: &mut SourceRevisions,
) -> Vec<u16> {
    use render_scene::model_lighting::ResolvedModelLighting;
    let mut failed = Vec::new();
    let mut changed = false;
    for draw in draws.iter_mut() {
        let Some(request) = draw.pending_lighting.take() else {
            continue;
        };
        match resolved.get(request.owner) {
            Some(ResolvedModelLighting::Seated {
                handle,
                scene_light_index,
                reflection_probe_index,
                packed_lighting,
            }) => {
                changed |= (
                    draw.lighting_handle,
                    draw.scene_light_index,
                    draw.reflection_probe_index,
                    draw.packed_lighting,
                ) != (
                    handle,
                    scene_light_index,
                    reflection_probe_index,
                    packed_lighting,
                );
                draw.lighting_handle = handle;
                draw.scene_light_index = scene_light_index;
                draw.reflection_probe_index = reflection_probe_index;
                draw.packed_lighting = packed_lighting;
            }
            Some(ResolvedModelLighting::Failed) | None => failed.push(draw.object_id),
        }
    }
    if !failed.is_empty() {
        revisions.bump_admission();
        changed = true;
        draws.retain(|draw| !failed.contains(&draw.object_id));
        owners.retain(|owner| !failed.contains(&owner.object_id()));
    }
    if changed {
        revisions.bump_draws();
    }
    failed
}

fn apply_fpv_plan(
    resolved: &render_scene::model_lighting::ResolvedModelLightingTable,
    plan: &mut FpvDrawPlan,
) {
    use render_scene::model_lighting::{ModelLightingOwner, ResolvedModelLighting};
    let previous = (
        plan.lighting_handle,
        plan.scene_light_index,
        plan.reflection_probe_index,
    );
    let was_visible = plan.visible;
    let hide = |plan: &mut FpvDrawPlan| {
        plan.lighting_handle = 0;
        plan.visible = false;
        plan.scene_light_index = 0;
        plan.reflection_probe_index = 0;
    };
    match resolved.get(ModelLightingOwner::Eye) {
        Some(ResolvedModelLighting::Seated {
            handle,
            scene_light_index,
            reflection_probe_index,
            ..
        }) => {
            plan.lighting_handle = handle;
            plan.scene_light_index = scene_light_index;
            plan.reflection_probe_index = reflection_probe_index;

            if !plan.draws.is_empty() {
                plan.settle_visible();
            }
        }
        Some(ResolvedModelLighting::Failed) | None => hide(plan),
    }
    if was_visible != plan.visible {
        plan.revisions.bump_admission();
    }
    if was_visible != plan.visible
        || previous
            != (
                plan.lighting_handle,
                plan.scene_light_index,
                plan.reflection_probe_index,
            )
    {
        plan.revisions.bump_draws();
    }
}

/// The rows a producer rebuilds every frame, handed to the plan that publishes
/// them. The plan answers whether anything moved; no consumer re-hashes the
/// rows to find out for itself.
macro_rules! publish_frame_rows {
    ($plan:ty, $draw:ty, $owner:ty) => {
        impl $plan {
            pub fn publish_frame_rows(&mut self, draws: &mut Vec<$draw>, owners: &mut Vec<$owner>) {
                let mut changed = render_frame::publish_rows(&mut self.draws, draws);
                changed |= render_frame::publish_rows(&mut self.owners, owners);
                if changed {
                    self.revisions.bump_draws();
                    self.revision = self.revision.wrapping_add(1);
                }
            }

            /// The producer found nothing to draw this frame. Same contract as a
            /// rebuild that came back empty — an early return may not leave last
            /// frame's rows published under this frame's revision.
            pub fn publish_no_rows(&mut self) {
                if !self.draws.is_empty() || !self.owners.is_empty() {
                    self.draws.clear();
                    self.owners.clear();
                    self.revisions.bump_draws();
                    self.revisions.bump_admission();
                    self.revision = self.revision.wrapping_add(1);
                }
            }
        }
    };
}

/// Read-only views of what a producer published: a consumer that could write
/// the rows would make the published revision less than the whole truth.
macro_rules! published_rows {
    ($plan:ty, $draw:ty) => {
        impl $plan {
            pub fn draws(&self) -> &[$draw] {
                &self.draws
            }

            pub fn materials(&self) -> &[SmodelPassMaterial] {
                &self.materials
            }

            pub fn vertices(&self) -> &[SmodelVertex] {
                &self.vertices
            }

            pub fn indices(&self) -> &[u32] {
                &self.indices
            }

            pub fn surface_ranges(&self) -> &[(u32, u32)] {
                &self.surface_ranges
            }

            pub fn packed_vertices(&self) -> &asset_world::PackedVertexPayload {
                &self.packed_vertices
            }
        }
    };
}

published_rows!(RemoteBodyDrawPlan, RemoteBodySurfaceDraw);
published_rows!(ScriptModelDrawPlan, XModelSurfaceDraw);
published_rows!(MissileDrawPlan, XModelSurfaceDraw);
published_rows!(ItemDrawPlan, XModelSurfaceDraw);
published_rows!(DynEntDrawPlan, XModelSurfaceDraw);

impl FpvDrawPlan {
    pub fn draws(&self) -> &[FpvSurfaceDraw] {
        &self.draws
    }

    pub fn materials(&self) -> &[SmodelPassMaterial] {
        &self.materials
    }

    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    pub fn surface_ranges(&self) -> &[(u32, u32)] {
        &self.surface_ranges
    }

    pub fn packed_vertices(&self) -> &asset_world::PackedVertexPayload {
        &self.packed_vertices
    }
}

impl RemoteBodyDrawPlan {
    pub fn decoded_n(&self) -> usize {
        self.decoded_n
    }
}

impl MissileDrawPlan {
    /// Empties a staging plan for this frame's rebuild, keeping its allocations.
    pub fn clear_rebuild(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.surface_ranges.clear();
        self.materials.clear();
        self.packed_vertices = asset_world::PackedVertexPayload::Unavailable {
            source_layout: crate::XMODEL_PACKED_UNAVAILABLE,
        };
    }

    /// Takes the rebuild and reports what moved. Vertex bytes are not compared:
    /// these models are posed by `world_from_local`, so a different mesh always
    /// shows up in the index layout, the surface ranges, the materials or the
    /// owner list.
    pub fn publish_rebuild(
        &mut self,
        staged: &mut Self,
        draws: &mut Vec<XModelSurfaceDraw>,
        owners: &mut Vec<MissileOwnerDraw>,
    ) {
        let mut geometry = render_frame::publish_rows(&mut self.indices, &mut staged.indices);
        geometry |=
            render_frame::publish_rows(&mut self.surface_ranges, &mut staged.surface_ranges);
        let materials = render_frame::publish_rows(&mut self.materials, &mut staged.materials);
        geometry |= materials;
        geometry |= self.vertices.len() != staged.vertices.len();
        if materials {
            self.revisions.bump_materials();
        }
        if geometry {
            std::mem::swap(&mut self.vertices, &mut staged.vertices);
            self.packed_vertices = std::mem::replace(
                &mut staged.packed_vertices,
                asset_world::PackedVertexPayload::Unavailable {
                    source_layout: crate::XMODEL_PACKED_UNAVAILABLE,
                },
            );
            self.revisions.bump_vertices();
            self.revisions.set_topology_from(
                &self.indices,
                &self.surface_ranges,
                self.vertices.len(),
            );
        }
        staged.vertices.clear();
        let mut rows = render_frame::publish_rows(&mut self.draws, draws);
        rows |= render_frame::publish_rows(&mut self.owners, owners);
        if geometry || rows {
            self.revisions.bump_draws();
            self.revision = self.revision.wrapping_add(1);
        }
    }
}

publish_frame_rows!(ScriptModelDrawPlan, XModelSurfaceDraw, ScriptModelOwnerDraw);
publish_frame_rows!(ItemDrawPlan, XModelSurfaceDraw, ItemOwnerDraw);
publish_frame_rows!(DynEntDrawPlan, XModelSurfaceDraw, DynEntOwnerDraw);

impl ScriptModelDrawPlan {
    pub fn finalize_lighting(
        &mut self,
        resolved: &render_scene::ResolvedModelLightingTable,
    ) -> Vec<u16> {
        apply_plan(
            resolved,
            &mut self.draws,
            &mut self.owners,
            &mut self.revisions,
        )
    }
}

impl ItemDrawPlan {
    pub fn finalize_lighting(
        &mut self,
        resolved: &render_scene::ResolvedModelLightingTable,
    ) -> Vec<u16> {
        apply_plan(
            resolved,
            &mut self.draws,
            &mut self.owners,
            &mut self.revisions,
        )
    }
}

impl MissileDrawPlan {
    pub fn finalize_lighting(
        &mut self,
        resolved: &render_scene::ResolvedModelLightingTable,
    ) -> Vec<u16> {
        apply_plan(
            resolved,
            &mut self.draws,
            &mut self.owners,
            &mut self.revisions,
        )
    }
}

impl DynEntDrawPlan {
    pub fn finalize_lighting(
        &mut self,
        resolved: &render_scene::ResolvedModelLightingTable,
    ) -> Vec<u16> {
        apply_plan(
            resolved,
            &mut self.draws,
            &mut self.owners,
            &mut self.revisions,
        )
    }
}

impl FpvDrawPlan {
    pub fn finalize_lighting(&mut self, resolved: &render_scene::ResolvedModelLightingTable) {
        apply_fpv_plan(resolved, self);
    }
}
trait HasObjectId {
    fn object_id(&self) -> u16;
}

impl HasObjectId for ScriptModelOwnerDraw {
    fn object_id(&self) -> u16 {
        self.object_id
    }
}

impl HasObjectId for ItemOwnerDraw {
    fn object_id(&self) -> u16 {
        self.object_id
    }
}

impl HasObjectId for MissileOwnerDraw {
    fn object_id(&self) -> u16 {
        self.object_id
    }
}

impl HasObjectId for DynEntOwnerDraw {
    fn object_id(&self) -> u16 {
        self.object_id
    }
}
