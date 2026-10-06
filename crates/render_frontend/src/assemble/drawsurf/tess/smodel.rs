use std::sync::Arc;

use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;

use crate::prepare::scene::world::WorldStaticModelMesh;

pub use render_frame::{PackedVertexRefusal, SmodelVertex};
pub use render_scene::{
    AuthoredMaps, LodRampArgs, SmodelPassMaterial, runtime_maps, smodel_camera_lod,
};

#[derive(Clone, Debug, Default)]
pub struct SmodelMeshSurfaces {
    pub surfaces: Vec<(u32, Option<assets::MaterialIndex>)>,
    pub surfaces_by_lod: [Vec<(u32, Option<assets::MaterialIndex>)>; 4],

    pub vert_start: u32,
    pub vert_count: u32,
    pub vert_count_by_lod: [u32; 4],

    pub lod_smc: Option<[u8; 4]>,
    pub lod_smc_rows: Option<[[u8; 4]; 4]>,
    pub lod: Option<asset_model::ModelLodSelector>,

    pub smc_surfs: Vec<SmodelCachedSurfSrc>,
    pub smc_surfs_by_lod: [Vec<SmodelCachedSurfSrc>; 4],

    pub xsurface_plus_1_by_lod: [Vec<Option<u8>>; 4],
}

#[derive(Clone, Debug)]
pub struct SmodelCachedSurfSrc {
    pub range_idx: u32,
    pub material: Option<assets::MaterialIndex>,
    pub packed_off: u32,
    pub packed_n: u32,
    pub vert_base: u32,
    pub index_start: u32,
    pub index_count: u32,
    pub xsurface_base_index: u16,
    pub xsurface_vert_offset: u16,
}

#[derive(Clone, Debug)]
pub struct SmodelPlacement {
    pub mesh: usize,

    pub lit: bool,

    pub reflection_probe_index: u8,

    pub primary_light_index: u8,

    pub flags: u8,

    pub lighting_slot: Option<usize>,

    pub packed_lighting: Option<[u8; 4]>,

    pub entity: Entity,

    pub world_from_local: Mat4,

    pub origin: [f32; 3],
    pub axis: [[f32; 3]; 3],
    pub scale: f32,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct SmodelGpuPlan {
    pub packed_vertices: asset_world::PackedVertexPayload,

    pub packed_match_surfaces: u32,

    pub packed_skip_surfaces: u32,

    pub packed_skip_first: Option<String>,

    pub cached_vertices: Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
    pub cached_indices: Vec<u32>,
    pub cached_placement_n: u32,
    pub cached_skip_n: u32,
    pub materials: Vec<SmodelPassMaterial>,
    pub meshes: Vec<SmodelMeshSurfaces>,

    pub material_state_flags: std::collections::HashMap<assets::MaterialIndex, u8>,
    pub placements: Vec<SmodelPlacement>,
    pub lighting_sh: Arc<Vec<Option<[[f32; 4]; 3]>>>,

    pub authored_placement_indices: Vec<Option<usize>>,

    pub shadow_draw_insts: Vec<dpvs_iw4::GfxStaticModelDrawInstShadow>,

    pub lit_material_key: std::collections::HashMap<(Option<assets::MaterialIndex>, u8), u32>,

    pub unlit_material_key: std::collections::HashMap<Option<assets::MaterialIndex>, u32>,
    pub upload_pending: bool,
    pub packed_share: Option<Arc<Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>>>,
    pub index_share: Option<Arc<Vec<u32>>>,
    pub range_share: Option<Arc<Vec<(u32, u32)>>>,
    pub vert_range_share: Option<Arc<Vec<(u32, u32)>>>,
    pub cached_share: Option<Arc<Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>>>,
    pub decoded_share: Option<Arc<Vec<SmodelVertex>>>,
    pub vertex_lighting_share: Option<Arc<Vec<[u8; 4]>>>,
}

impl SmodelGpuPlan {
    pub fn exact_packed_vertices(
        &self,
    ) -> Result<&[[u8; asset_iw4::size::GFX_PACKED_VERTEX]], PackedVertexRefusal> {
        let table_stride =
            asset_iw4::vertex_decl::stream_extent(asset_iw4::vertex_decl::PACKED_VERTEX_TYPE, 0);
        if table_stride != Some(asset_iw4::size::GFX_PACKED_VERTEX as u16) {
            return Err(PackedVertexRefusal::StrideMismatch { table_stride });
        }
        let vertices = if let Some(share) = self.packed_share.as_ref() {
            share.as_slice()
        } else {
            match &self.packed_vertices {
                asset_world::PackedVertexPayload::Iw4(vertices) => vertices.as_slice(),
                asset_world::PackedVertexPayload::Unavailable { source_layout } => {
                    return Err(PackedVertexRefusal::ForeignLayout { source_layout });
                }
            }
        };
        if vertices.len() != self.decoded_vertices().len() {
            return Err(PackedVertexRefusal::VertexCountMismatch {
                packed: vertices.len(),
                decoded: self.decoded_vertices().len(),
            });
        }
        Ok(vertices)
    }

    pub fn indices(&self) -> &[u32] {
        super::published_rows(&self.index_share)
    }

    pub fn surface_ranges(&self) -> &[(u32, u32)] {
        super::published_rows(&self.range_share)
    }

    pub fn decoded_vertices(&self) -> &[SmodelVertex] {
        super::published_rows(&self.decoded_share)
    }

    pub fn publish_extract_shares(&mut self) {
        if self.packed_share.is_none() {
            match std::mem::replace(
                &mut self.packed_vertices,
                asset_world::PackedVertexPayload::Unavailable {
                    source_layout: "packed payload published for extract",
                },
            ) {
                asset_world::PackedVertexPayload::Iw4(rows) => {
                    self.packed_share = Some(Arc::new(rows));
                }
                unavailable => self.packed_vertices = unavailable,
            }
        }
        if self.cached_share.is_none() {
            self.cached_share = Some(Arc::new(std::mem::take(&mut self.cached_vertices)));
        }
    }

    pub fn packed_ok(&self) -> bool {
        self.packed_share.is_some()
            || matches!(
                self.packed_vertices,
                asset_world::PackedVertexPayload::Iw4(_)
            )
    }

    pub fn packed_row_count(&self) -> Option<usize> {
        if let Some(share) = self.packed_share.as_ref() {
            return Some(share.len());
        }
        match &self.packed_vertices {
            asset_world::PackedVertexPayload::Iw4(rows) => Some(rows.len()),
            asset_world::PackedVertexPayload::Unavailable { .. } => None,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SmodelDrawImmediate {
    pub world_from_local: [[f32; 4]; 4],
    pub lighting_handle: u32,
    pub _pad: [u32; 3],
}

impl SmodelDrawImmediate {
    pub const SIZE: u32 = 80;

    pub fn new(world_from_local: Mat4, lighting_handle: u32) -> Self {
        debug_assert_eq!(std::mem::size_of::<Self>(), Self::SIZE as usize);
        Self {
            world_from_local: world_from_local.to_cols_array_2d(),
            lighting_handle,
            _pad: [0; 3],
        }
    }
}

fn f32x3(values: &VertexAttributeValues) -> Option<&[[f32; 3]]> {
    match values {
        VertexAttributeValues::Float32x3(v) => Some(v.as_slice()),
        _ => None,
    }
}

fn f32x2(values: &VertexAttributeValues) -> Option<&[[f32; 2]]> {
    match values {
        VertexAttributeValues::Float32x2(v) => Some(v.as_slice()),
        _ => None,
    }
}

fn f32x4(values: &VertexAttributeValues) -> Option<&[[f32; 4]]> {
    match values {
        VertexAttributeValues::Float32x4(v) => Some(v.as_slice()),
        _ => None,
    }
}

fn append_mesh_surface(
    mesh: &Mesh,
    vertices: &mut Vec<SmodelVertex>,
    indices: &mut Vec<u32>,
    surface_ranges: &mut Vec<(u32, u32)>,
) -> Option<u32> {
    let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).and_then(f32x3)?;
    let normals = mesh.attribute(Mesh::ATTRIBUTE_NORMAL).and_then(f32x3);
    let uvs = mesh.attribute(Mesh::ATTRIBUTE_UV_0).and_then(f32x2);
    let colors = mesh.attribute(Mesh::ATTRIBUTE_COLOR).and_then(f32x4);
    let mesh_indices = mesh.indices()?;
    let index_count = match mesh_indices {
        Indices::U32(ix) => ix.len(),
        Indices::U16(ix) => ix.len(),
    };
    let n = positions.len();
    if n == 0 || index_count == 0 {
        return None;
    }
    let indices_in_range = match mesh_indices {
        Indices::U32(ix) => ix.iter().all(|&index| (index as usize) < n),
        Indices::U16(ix) => ix.iter().all(|&index| usize::from(index) < n),
    };
    if !indices_in_range {
        return None;
    }
    let base = vertices.len() as u32;
    vertices.reserve(n);
    for i in 0..n {
        vertices.push(SmodelVertex {
            position: positions[i],
            normal: normals
                .and_then(|a| a.get(i).copied())
                .unwrap_or([0.0, 0.0, 1.0]),
            color: colors.and_then(|a| a.get(i).copied()).unwrap_or([1.0; 4]),
            uv0: uvs.and_then(|a| a.get(i).copied()).unwrap_or([0.0; 2]),
        });
    }
    let index_start = indices.len() as u32;
    match mesh_indices {
        Indices::U32(ix) => indices.extend(ix.iter().map(|&i| base.saturating_add(i))),
        Indices::U16(ix) => indices.extend(ix.iter().map(|&i| base.saturating_add(u32::from(i)))),
    }
    let range_idx = surface_ranges.len() as u32;
    surface_ranges.push((index_start, index_count as u32));
    Some(range_idx)
}

pub const UNIT_VERTEX_LIGHTING: [u8; 4] = [45, 45, 45, 255];

pub struct SmodelVertexLitCopy<'a> {
    pub mesh: usize,
    pub lighting: &'a [Vec<[u8; 4]>; 4],
}

pub fn pack_smodel_meshes(
    meshes: &[WorldStaticModelMesh],
    copies: &[SmodelVertexLitCopy<'_>],
) -> (
    SmodelGpuPlan,
    Vec<Vec<Option<assets::MaterialIndex>>>,
    Vec<Vec<asset_world::XSurfaceCollisionPayload>>,
    Vec<Option<usize>>,
) {
    let mut plan = SmodelGpuPlan::default();
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut surface_ranges = Vec::new();

    let mut mesh_authored: Vec<Vec<Option<assets::MaterialIndex>>> =
        Vec::with_capacity(meshes.len());
    let mut mesh_collision = Vec::with_capacity(meshes.len());
    let mut packed = Vec::new();
    let mut packed_ok = true;
    let mut match_surfaces = 0u32;
    let mut skip_surfaces = 0u32;
    let mut mark_collision_ready = 0u32;
    let mut mark_collision_missing = 0u32;
    let mut mark_collision_unavailable = 0u32;
    let mut skip_first: Option<String> = None;
    let mut vertex_lighting = Vec::new();
    let mut copy_meshes = Vec::with_capacity(copies.len());

    let jobs = meshes.iter().map(|model| (model, None)).chain(
        copies
            .iter()
            .filter_map(|copy| Some((meshes.get(copy.mesh)?, Some(copy.lighting)))),
    );
    for (model, lighting) in jobs {
        let rollback = (
            vertices.len(),
            indices.len(),
            surface_ranges.len(),
            packed.len(),
            match_surfaces,
            skip_surfaces,
        );
        let mut packed_row = SmodelMeshSurfaces::default();
        packed_row.vert_start = packed.len() as u32;
        packed_row.lod_smc = model.lod_smc.map(|rows| rows[0]);
        packed_row.lod_smc_rows = model.lod_smc;
        packed_row.lod = model.lod;
        let mut authored_row = Vec::new();
        let mut collision_row = Vec::new();
        for lod in 0..4 {
            let lod_packed_start = packed.len() as u32;
            for surface in &model.lod_surfaces[lod] {
                packed_row.xsurface_plus_1_by_lod[lod].push(surface.xsurface_plus_1);
                authored_row.push(surface.material);
                let decoded_before = vertices.len();
                let packed_off = packed.len() as u32;
                let Some(range_idx) = append_mesh_surface(
                    &surface.mesh,
                    &mut vertices,
                    &mut indices,
                    &mut surface_ranges,
                ) else {
                    skip_surfaces = skip_surfaces.saturating_add(1);
                    if skip_first.is_none() {
                        skip_first = Some(format!("empty_ib:{}", model.name));
                    }
                    continue;
                };
                let decoded_count = vertices.len() - decoded_before;
                if surface.packed_vertices.len() != decoded_count {
                    vertices.truncate(decoded_before);
                    indices.truncate(surface_ranges[range_idx as usize].0 as usize);
                    surface_ranges.pop();
                    skip_surfaces = skip_surfaces.saturating_add(1);
                    if skip_first.is_none() {
                        skip_first = Some(format!(
                            "{} packed={} decoded={}",
                            model.name,
                            surface.packed_vertices.len(),
                            decoded_count
                        ));
                    }
                    continue;
                }
                let (index_start, index_count) = surface_ranges[range_idx as usize];
                let src = SmodelCachedSurfSrc {
                    range_idx,
                    material: surface.material,
                    packed_off,
                    packed_n: decoded_count as u32,
                    vert_base: decoded_before as u32,
                    index_start,
                    index_count,
                    xsurface_base_index: surface.xsurface_base_index,
                    xsurface_vert_offset: surface.xsurface_vert_offset,
                };
                packed_row.surfaces_by_lod[lod].push((range_idx, surface.material));
                packed_row.smc_surfs_by_lod[lod].push(src.clone());
                if lod == 0 {
                    packed_row.surfaces.push((range_idx, surface.material));
                    packed_row.smc_surfs.push(src);
                    match &surface.collision {
                        asset_world::XSurfaceCollisionPayload::Iw4(lists)
                            if lists.iter().all(|list| list.tree.is_some()) =>
                        {
                            mark_collision_ready = mark_collision_ready.saturating_add(1);
                        }
                        asset_world::XSurfaceCollisionPayload::Iw4(_) => {
                            mark_collision_missing = mark_collision_missing.saturating_add(1);
                        }
                        asset_world::XSurfaceCollisionPayload::Unavailable { .. } => {
                            mark_collision_unavailable =
                                mark_collision_unavailable.saturating_add(1);
                        }
                    }
                    collision_row.push(surface.collision.clone());
                }
                packed.extend_from_slice(&surface.packed_vertices);
                match_surfaces = match_surfaces.saturating_add(1);
            }
            packed_row.vert_count_by_lod[lod] =
                (packed.len() as u32).saturating_sub(lod_packed_start);
        }
        packed_row.vert_count = (packed.len() as u32).saturating_sub(packed_row.vert_start);
        match lighting {
            None => vertex_lighting.resize(packed.len(), UNIT_VERTEX_LIGHTING),
            Some(lighting)
                if (0..4).all(|lod| {
                    lighting[lod].len() == packed_row.vert_count_by_lod[lod] as usize
                }) =>
            {
                vertex_lighting.extend(lighting.iter().flatten().copied());
                copy_meshes.push(Some(plan.meshes.len()));
            }
            Some(_) => {
                vertices.truncate(rollback.0);
                indices.truncate(rollback.1);
                surface_ranges.truncate(rollback.2);
                packed.truncate(rollback.3);
                match_surfaces = rollback.4;
                skip_surfaces = rollback.5;
                copy_meshes.push(None);
                continue;
            }
        }
        plan.meshes.push(packed_row);
        mesh_authored.push(authored_row);
        mesh_collision.push(collision_row);
    }
    packed_ok = packed_ok && packed.len() == vertices.len() && !packed.is_empty();
    plan.packed_match_surfaces = match_surfaces;
    plan.packed_skip_surfaces = skip_surfaces;
    plan.packed_skip_first = skip_first.clone();
    plan.packed_vertices = if packed_ok {
        asset_world::PackedVertexPayload::Iw4(packed)
    } else if vertices.is_empty() {
        asset_world::PackedVertexPayload::Unavailable {
            source_layout: "smodel plan has no vertices",
        }
    } else {
        asset_world::PackedVertexPayload::Unavailable {
            source_layout: "smodel packed vertex records missing or count-mismatched",
        }
    };
    plan.upload_pending = !vertices.is_empty() && !indices.is_empty();
    diag::info!(
        World,
        "smodel packed: verts={} packed={} match_surf={} skip_surf={} first_skip={} upload={} mark_collision=ready:{} missing:{} unavailable:{}",
        vertices.len(),
        match &plan.packed_vertices {
            asset_world::PackedVertexPayload::Iw4(rows) => rows.len(),
            asset_world::PackedVertexPayload::Unavailable { .. } => 0,
        },
        match_surfaces,
        skip_surfaces,
        skip_first.as_deref().unwrap_or("-"),
        u8::from(plan.upload_pending),
        mark_collision_ready,
        mark_collision_missing,
        mark_collision_unavailable,
    );
    plan.decoded_share = Some(Arc::new(vertices));
    plan.index_share = Some(Arc::new(indices));
    plan.range_share = Some(Arc::new(surface_ranges));
    let mut vert_ranges =
        vec![(0u32, 0u32); plan.range_share.as_ref().map_or(0, |rows| rows.len())];
    for mesh in &plan.meshes {
        for lod in 0..4 {
            for src in &mesh.smc_surfs_by_lod[lod] {
                if let Some(slot) = vert_ranges.get_mut(src.range_idx as usize) {
                    *slot = (src.packed_off, src.packed_n);
                }
            }
        }
    }
    plan.vert_range_share = Some(Arc::new(vert_ranges));
    plan.vertex_lighting_share = Some(Arc::new(vertex_lighting));
    (plan, mesh_authored, mesh_collision, copy_meshes)
}

pub fn expand_smodel_cached_vertices(
    plan: &mut SmodelGpuPlan,
    _lighting_for_slot: impl Fn(usize) -> Option<[u8; 4]>,
) {
    plan.cached_vertices.clear();
    plan.cached_indices.clear();
    plan.cached_placement_n = 0;
    plan.cached_skip_n = 0;
}

pub(crate) fn build_smodel_gpu_plan(
    mut scene: Option<ResMut<crate::prepare::scene::world::WorldScene>>,
    job: Res<crate::prepare::scene::spawn::WorldSpawnJob>,
    existing: Option<Res<SmodelGpuPlan>>,
    mut commands: Commands,
) {
    if !matches!(
        job.phase,
        crate::prepare::scene::spawn::WorldSpawnPhase::Gpu
            | crate::prepare::scene::spawn::WorldSpawnPhase::Done
    ) {
        return;
    }
    if existing.is_some() {
        return;
    }
    let Some(scene) = scene.as_mut() else {
        return;
    };
    if scene.static_model_meshes.is_empty() {
        return;
    }
    let slot_count = scene.static_model_instances.len();
    if !scene
        .cull
        .as_ref()
        .is_some_and(|cull| cull.static_model_entities.len() == slot_count)
    {
        return;
    }
    let meshes = std::mem::take(&mut scene.static_model_meshes);
    let mut slot_vertex_lighting = vec![None; slot_count];
    for sample in &scene.smodel_lighting_samples {
        if let Some(slot) = slot_vertex_lighting.get_mut(sample.authored_slot) {
            slot.clone_from(&sample.vertex_lighting);
        }
    }
    let copy_slots: Vec<usize> = (0..slot_count)
        .filter(|&slot| {
            slot_vertex_lighting[slot].is_some()
                && scene.static_model_instances[slot]
                    .is_some_and(|instance| instance.mesh < meshes.len())
        })
        .collect();
    let copies: Vec<SmodelVertexLitCopy> = copy_slots
        .iter()
        .filter_map(|&slot| {
            Some(SmodelVertexLitCopy {
                mesh: scene.static_model_instances[slot]?.mesh,
                lighting: slot_vertex_lighting[slot].as_deref()?,
            })
        })
        .collect();
    let (mut plan, mesh_authored, mesh_collision, copy_meshes) =
        pack_smodel_meshes(&meshes, &copies);
    drop(meshes);
    let mut slot_mesh = vec![None; slot_count];
    for (&slot, mesh) in copy_slots.iter().zip(&copy_meshes) {
        slot_mesh[slot] = *mesh;
    }
    diag::info!(
        World,
        "static model vertex lighting: {} placements, {} mesh copies",
        copy_slots.len(),
        copy_meshes.iter().flatten().count(),
    );

    for material in plan
        .meshes
        .iter()
        .flat_map(|mesh| mesh.smc_surfs_by_lod.iter().flatten())
        .filter_map(|surface| surface.material)
    {
        if let Some(derived) = scene.runtime_material_catalog.derived(material) {
            plan.material_state_flags
                .insert(material, derived.state_flags);
        }
    }

    plan.authored_placement_indices.resize(slot_count, None);
    let (exact, _, probes) = job.tess_image_handles();
    let sorted_ordinals: Vec<Option<u32>> =
        (0..scene.runtime_material_catalog.parts().materials.len())
            .map(|id| {
                scene
                    .runtime_material_catalog
                    .parts()
                    .sorted_materials
                    .ordinal_for_asset_id(id)
                    .map(crate::assemble::drawsurf::SortedMaterialOrdinal::get)
            })
            .collect();
    let sorted_ordinal = |asset_id: Option<assets::MaterialIndex>| {
        asset_id.and_then(|id| sorted_ordinals.get(id.order()).copied().flatten())
    };
    let lighting = scene.model_lighting_dims.and_then(|dims| {
        crate::prepare::scene::smodel_lighting::prepare_smodel_lighting(
            dims,
            &scene.smodel_lighting_samples,
            slot_count,
        )
    });
    let atlas_image = scene.model_lighting_image.clone();
    let mut slot_lighting_sh = vec![None; slot_count];
    for sample in &scene.smodel_lighting_samples {
        if let Some(slot) = slot_lighting_sh.get_mut(sample.authored_slot) {
            *slot = sample.lighting_sh;
        }
    }
    let mut placement_lighting_sh = Vec::new();

    for (index, instance) in scene.static_model_instances.iter().enumerate() {
        let Some(mut instance) = *instance else {
            continue;
        };
        if let Some(mesh) = slot_mesh[index] {
            instance.mesh = mesh;
        }
        let Some(parent) = scene
            .cull
            .as_ref()
            .and_then(|cull| cull.static_model_entities.get(index).copied().flatten())
        else {
            continue;
        };
        let Some(authored_row) = mesh_authored.get(instance.mesh) else {
            continue;
        };
        if authored_row.is_empty() {
            continue;
        }
        let wants_atlas = crate::prepare::scene::smodel_lighting::surfaces_take_model_lighting(
            authored_row.iter().copied(),
            &scene.runtime_material_catalog,
        );
        let lit = lighting.as_ref().is_some_and(|l| {
            crate::prepare::scene::smodel_lighting::slot_is_lit_candidate(l, index, wants_atlas)
        });
        if lit {
            let probe_index = instance.reflection_probe_index;
            for &authored in authored_row {
                let key = (authored, probe_index);
                if plan.lit_material_key.contains_key(&key) {
                    continue;
                }
                if authored
                    .and_then(|id| scene.runtime_material_catalog.derived(id))
                    .is_some_and(|m| m.shadow_only)
                {
                    continue;
                }
                let maps = runtime_maps(authored, &scene.runtime_material_catalog, exact);
                if maps.color.is_none() {
                    diag::info!(
                        World,
                        "static model material gap: {:?} has no decoded color map",
                        authored
                            .and_then(|id| scene.runtime_material_catalog.derived(id))
                            .map(|m| m.name.as_str())
                            .unwrap_or("<none>")
                    );
                }
                let probe = probes.get(usize::from(probe_index)).and_then(Clone::clone);
                let atlas = atlas_image
                    .as_ref()
                    .expect("smodel lighting requires the shared atlas")
                    .clone();
                let dims = scene
                    .model_lighting_dims
                    .expect("smodel lighting requires atlas dims");
                let inv_h = lighting_iw4::model_lighting_inv_image_height(dims.image_height)
                    .expect("mcv=1 atlas height is non-zero");
                let scale = lighting_iw4::model_lighting_lookup_scale(inv_h);
                let mat_idx = plan.materials.len() as u32;
                plan.materials.push(SmodelPassMaterial {
                    model_lighting_required: true,
                    color: maps.color,
                    specular: maps.specular,
                    probe,
                    atlas: Some(atlas),
                    alpha_mode: maps.alpha_mode,
                    draw_mode: maps.draw_mode,
                    cull_mode: maps.cull_mode,
                    env_map_parms: maps.env_map_parms,
                    lighting_lookup_scale: [scale.u, scale.v, scale.w, scale.q],
                    atlas_lookup: [
                        lighting_iw4::MODEL_LIGHTING_INV_ATLAS_WIDTH as f32,
                        inv_h,
                        lighting_iw4::MODEL_LIGHTING_VOLUME_W,
                        0.0,
                    ],
                    sort_key: authored
                        .and_then(|id| scene.runtime_material_catalog.derived(id))
                        .map(|m| m.sort_key)
                        .unwrap_or(0),
                    material_sorted_index: sorted_ordinal(authored),
                });
                plan.lit_material_key.insert(key, mat_idx);
            }
        } else {
            for &authored in authored_row {
                if plan.unlit_material_key.contains_key(&authored) {
                    continue;
                }
                if authored
                    .and_then(|id| scene.runtime_material_catalog.derived(id))
                    .is_some_and(|m| m.shadow_only)
                {
                    continue;
                }
                let maps = runtime_maps(authored, &scene.runtime_material_catalog, exact);
                let mat_idx = plan.materials.len() as u32;
                plan.materials.push(SmodelPassMaterial {
                    model_lighting_required: false,
                    color: maps.color,
                    specular: None,
                    probe: None,
                    atlas: None,
                    alpha_mode: maps.alpha_mode,
                    draw_mode: maps.draw_mode,
                    cull_mode: maps.cull_mode,
                    env_map_parms: maps.env_map_parms,
                    lighting_lookup_scale: [0.0; 4],
                    atlas_lookup: [0.0; 4],
                    sort_key: authored
                        .and_then(|id| scene.runtime_material_catalog.derived(id))
                        .map(|m| m.sort_key)
                        .unwrap_or(0),
                    material_sorted_index: sorted_ordinal(authored),
                });
                plan.unlit_material_key.insert(authored, mat_idx);
            }
        }

        let placement_i = plan.placements.len();
        placement_lighting_sh.push(slot_lighting_sh[index]);
        plan.placements.push(SmodelPlacement {
            mesh: instance.mesh,
            lit,
            reflection_probe_index: instance.reflection_probe_index,
            primary_light_index: instance.primary_light_index,
            flags: instance.flags,
            lighting_slot: Some(index),
            packed_lighting: None,
            entity: parent,
            world_from_local: instance.transform.to_matrix(),
            origin: instance.origin,
            axis: instance.axis,
            scale: instance.scale,
        });
        plan.authored_placement_indices[index] = Some(placement_i);
    }

    plan.lighting_sh = Arc::new(placement_lighting_sh);
    let cull_dists = scene
        .cull
        .as_ref()
        .map(|cull| cull.static_model_cull_dists.as_slice())
        .unwrap_or(&[]);
    let mut shadow_draw_insts = Vec::new();
    super::super::retained_list::fill_smodel_draw_inst_shadow(
        &mut shadow_draw_insts,
        cull_dists,
        &plan,
    );
    plan.shadow_draw_insts = shadow_draw_insts;

    expand_smodel_cached_vertices(&mut plan, |_| None);
    scene.smodel_mark_cpu = Some(crate::prepare::scene::world::SmodelMarkCpu {
        positions: plan.decoded_vertices().iter().map(|v| v.position).collect(),
        normals: plan.decoded_vertices().iter().map(|v| v.normal).collect(),
        indices: plan.indices().to_vec(),
        meshes: plan
            .meshes
            .iter()
            .enumerate()
            .map(|(mesh_index, mesh)| {
                mesh.surfaces_by_lod[0]
                    .iter()
                    .zip(mesh_collision.get(mesh_index).into_iter().flatten())
                    .filter_map(|(&(range_idx, authored), collision)| {
                        plan.surface_ranges().get(range_idx as usize).copied().map(
                            |(start, count)| crate::prepare::scene::world::SmodelMarkSurface {
                                index_start: start,
                                index_count: count,
                                material: authored,
                                collision: collision.clone(),
                            },
                        )
                    })
                    .collect()
            })
            .collect(),
    });
    diag::info!(
        World,
        "static models: {} authored slots; retained verts={} indices={} mats={}",
        slot_count,
        plan.decoded_vertices().len(),
        plan.indices().len(),
        plan.materials.len(),
    );
    plan.publish_extract_shares();
    commands.insert_resource(plan);
}
