use std::sync::Arc;

use bevy::prelude::*;
use bevy::render::render_resource::Face;

use crate::assemble::drawsurf::{
    SurfaceLightmapId, SurfaceReflectionProbeId, SurfaceSamplerInputs,
};
use crate::prepare::scene::world::{WorldScene, WorldSun};

pub use render_frame::WorldVertex;

#[derive(Clone, Debug)]
pub struct WorldPassMaterial {
    pub color: Option<Handle<Image>>,
    pub normal: Option<Handle<Image>>,
    pub specular: Option<Handle<Image>>,
    pub probe: Option<Handle<Image>>,
    pub ambient: Option<Handle<Image>>,
    pub directional: Option<Handle<Image>>,
    pub sun_mask: Option<Handle<Image>>,
    pub sun: Option<WorldSun>,
    pub alpha_mode: AlphaMode,

    pub draw_mode: Option<asset_material::MaterialDrawMode>,
    pub square_color_map: bool,
    pub env_map_parms: [f32; 4],
    pub uv_anim: [f32; 4],
    pub falloff_parms: [f32; 4],
    pub falloff_begin: [f32; 4],
    pub falloff_end: [f32; 4],
    pub cull_mode: Option<Face>,
}

pub use render_frame::WorldVertexRefusal;

#[derive(Resource, Clone, Debug, Default)]
pub struct WorldDrawGpuPlan {
    pub packed_vertices: asset_world::WorldVertexPayload,

    pub surface_material: Vec<u32>,

    pub surface_sampler_inputs: Vec<SurfaceSamplerInputs>,
    pub materials: Vec<WorldPassMaterial>,

    pub upload_pending: bool,
    pub vertex_share: Option<Arc<Vec<[u8; asset_iw4::size::GFX_WORLD_VERTEX]>>>,
    pub index_share: Option<Arc<Vec<u32>>>,
    pub range_share: Option<Arc<Vec<(u32, u32)>>>,
    pub layer_share: Option<Arc<Vec<u8>>>,
    pub decoded_share: Option<Arc<Vec<WorldVertex>>>,
}

impl WorldDrawGpuPlan {
    pub fn exact_packed_vertices(
        &self,
    ) -> Result<&[[u8; asset_iw4::size::GFX_WORLD_VERTEX]], WorldVertexRefusal> {
        let table_stride =
            asset_iw4::vertex_decl::stream_extent(asset_iw4::vertex_decl::WORLD_VERTEX_TYPE, 0);
        if table_stride != Some(asset_iw4::size::GFX_WORLD_VERTEX as u16) {
            return Err(WorldVertexRefusal::StrideMismatch { table_stride });
        }
        let vertices = if let Some(share) = self.vertex_share.as_ref() {
            share.as_slice()
        } else {
            match self.packed_vertices.type2_stream0() {
                Ok(vertices) => vertices,
                Err(source_layout) => {
                    return Err(WorldVertexRefusal::ForeignLayout { source_layout });
                }
            }
        };
        if vertices.len() != self.decoded_vertices().len() {
            return Err(WorldVertexRefusal::VertexCountMismatch {
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

    pub fn decoded_vertices(&self) -> &[WorldVertex] {
        super::published_rows(&self.decoded_share)
    }

    pub fn vertex_layer_rows(&self) -> &[u8] {
        super::published_rows(&self.layer_share)
    }

    pub fn publish_extract_shares(&mut self) {
        if self.vertex_share.is_none() {
            match std::mem::replace(
                &mut self.packed_vertices,
                asset_world::WorldVertexPayload::Unavailable {
                    source_layout: "packed payload published for extract",
                },
            ) {
                asset_world::WorldVertexPayload::Iw4(rows)
                | asset_world::WorldVertexPayload::Iw5(rows)
                | asset_world::WorldVertexPayload::T5(rows)
                | asset_world::WorldVertexPayload::T6(rows) => {
                    self.vertex_share = Some(Arc::new(rows));
                }
                unavailable => self.packed_vertices = unavailable,
            }
        }
    }

    pub fn from_scene(
        scene: &mut WorldScene,
        exact_handles: &[Option<Handle<Image>>],
        lightmap_handles: &[Option<crate::assemble::drawsurf::RuntimeLightmapHandles>],
        reflection_probe_handles: &[Option<Handle<Image>>],
    ) -> Self {
        let packed_vertices = std::mem::replace(
            &mut scene.retained_packed_vertices,
            asset_world::WorldVertexPayload::Unavailable {
                source_layout: "packed payload consumed by WorldDrawGpuPlan",
            },
        );
        let vertex_layer = scene.retained_vertex_layer.clone();
        let n = scene.retained_positions.len();
        let mut vertices = Vec::with_capacity(n);
        for i in 0..n {
            vertices.push(WorldVertex {
                position: scene.retained_positions[i],
                normal: scene
                    .retained_normals
                    .get(i)
                    .copied()
                    .unwrap_or([0.0, 0.0, 1.0]),
                tangent: scene
                    .retained_tangents
                    .get(i)
                    .copied()
                    .unwrap_or([1.0, 0.0, 0.0, 1.0]),
                color: scene.retained_colors.get(i).copied().unwrap_or([1.0; 4]),
                uv0: scene
                    .retained_texture_uvs
                    .get(i)
                    .copied()
                    .unwrap_or([0.0; 2]),
                uv1: scene
                    .retained_lightmap_uvs
                    .get(i)
                    .copied()
                    .unwrap_or([0.0; 2]),
            });
        }

        scene.retained_tangents.clear();
        scene.retained_colors.clear();
        scene.retained_texture_uvs.clear();

        let mut materials = Vec::new();
        let mut batch_to_mat = vec![u32::MAX; scene.batches.len()];

        for (batch_i, batch) in scene.batches.iter().enumerate() {
            let runtime = batch
                .material
                .and_then(|id| scene.runtime_material_catalog.derived(id));
            let color = runtime.and_then(|m| {
                m.texture_semantic(asset_material::TS_COLOR_MAP)
                    .and_then(|id| exact_handles.get(id.0 as usize).and_then(Clone::clone))
            });
            let normal = runtime.and_then(|m| {
                m.texture_semantic(asset_material::TS_NORMAL_MAP)
                    .and_then(|id| exact_handles.get(id.0 as usize).and_then(Clone::clone))
            });
            let specular = runtime.and_then(|m| {
                m.texture_semantic(asset_material::TS_SPECULAR_MAP)
                    .and_then(|id| exact_handles.get(id.0 as usize).and_then(Clone::clone))
            });
            let probe = reflection_probe_handles
                .get(usize::from(batch.reflection_probe_index))
                .and_then(Clone::clone);

            let (ambient, directional, sun_mask) = if batch.lightmapped {
                lightmap_handles
                    .get(usize::from(batch.lightmap_index))
                    .and_then(|page| page.as_ref())
                    .map(|page| {
                        (
                            Some(page.ambient_diagnostic.clone()),
                            Some(page.directional_diagnostic.clone()),
                            Some(page.sun_mask_diagnostic.clone()),
                        )
                    })
                    .unwrap_or((None, None, None))
            } else {
                (None, None, None)
            };

            let mat = WorldPassMaterial {
                color,
                normal,
                specular,
                probe,
                ambient,
                directional,
                sun_mask,
                sun: batch.sun,
                alpha_mode: AlphaMode::Opaque,
                draw_mode: None,
                square_color_map: runtime.is_some_and(|m| m.square_color_map),
                env_map_parms: runtime.map(|m| m.env_map_parms()).unwrap_or([0.0; 4]),
                uv_anim: runtime.map(|m| m.uv_anim()).unwrap_or([0.0; 4]),
                falloff_parms: runtime.map(|m| m.falloff_parms()).unwrap_or([0.0; 4]),
                falloff_begin: runtime.map(|m| m.falloff_begin()).unwrap_or([0.0; 4]),
                falloff_end: runtime.map(|m| m.falloff_end()).unwrap_or([0.0; 4]),
                cull_mode: runtime.and_then(|m| super::super::runtime_cull_face(m.cull_mode)),
            };
            let idx = materials.len() as u32;
            materials.push(mat);
            if let Some(slot) = batch_to_mat.get_mut(batch_i) {
                *slot = idx;
            }
        }

        let Some(cull) = scene.cull.as_ref() else {
            let upload_pending = !vertices.is_empty();
            let mut plan = Self {
                packed_vertices,
                surface_material: Vec::new(),
                surface_sampler_inputs: Vec::new(),
                materials,
                upload_pending,
                vertex_share: None,
                index_share: Some(Arc::new(Vec::new())),
                range_share: Some(Arc::new(Vec::new())),
                layer_share: Some(Arc::new(vertex_layer)),
                decoded_share: Some(Arc::new(vertices)),
            };
            plan.publish_extract_shares();
            return plan;
        };

        let indices = cull.packed_indices.clone();
        let surface_ranges = cull.surface_index_ranges.clone();

        let surface_material: Vec<u32> = cull
            .surface_batch_ranges
            .iter()
            .zip(surface_ranges.iter())
            .map(|(&(batch, _, _), &(_, count))| {
                if count == 0 {
                    return u32::MAX;
                }
                batch_to_mat.get(batch).copied().unwrap_or(u32::MAX)
            })
            .collect();
        let surface_sampler_inputs = cull
            .surface_batch_ranges
            .iter()
            .map(|&(batch, _, _)| {
                let Some(batch) = scene.batches.get(batch) else {
                    return SurfaceSamplerInputs::default();
                };
                SurfaceSamplerInputs {
                    reflection_probe: Some(SurfaceReflectionProbeId(batch.reflection_probe_index)),
                    primary_lightmap: batch
                        .lightmapped
                        .then_some(SurfaceLightmapId(batch.lightmap_index)),
                    secondary_lightmap: batch
                        .lightmapped
                        .then_some(SurfaceLightmapId(batch.lightmap_index)),
                }
            })
            .collect();

        let upload_pending = !vertices.is_empty() && !indices.is_empty();
        let mut plan = Self {
            packed_vertices,
            surface_material,
            surface_sampler_inputs,
            materials,
            upload_pending,
            vertex_share: None,
            index_share: Some(Arc::new(indices)),
            range_share: Some(Arc::new(surface_ranges)),
            layer_share: Some(Arc::new(vertex_layer)),
            decoded_share: Some(Arc::new(vertices)),
        };
        plan.publish_extract_shares();
        plan
    }
}

pub(crate) fn build_world_draw_gpu_plan(
    mut scene: Option<ResMut<WorldScene>>,
    job: Res<crate::prepare::scene::spawn::WorldSpawnJob>,
    existing: Option<Res<WorldDrawGpuPlan>>,
    mut commands: Commands,
) {
    if job.phase != crate::prepare::scene::spawn::WorldSpawnPhase::WorldTess {
        return;
    }
    if existing.is_some() {
        return;
    }
    let Some(scene) = scene.as_mut() else {
        return;
    };
    let (exact, lightmaps, probes) = job.tess_image_handles();
    let plan = WorldDrawGpuPlan::from_scene(scene, exact, lightmaps, probes);
    for batch in &scene.batches {
        let runtime = batch
            .material
            .and_then(|material| scene.runtime_material_catalog.derived(material));
        if runtime.is_some_and(|m| m.unlit)
            && runtime.is_some_and(|m| m.uv_anim() != [0.0; 4] || m.falloff_begin()[3] >= 0.5)
        {
            diag::info!(
                World,
                "world unlit anim: {} uvAnim={:?} falloffParms={:?} falloffBegin={:?} falloffEnd={:?}",
                runtime.map(|m| m.name.as_str()).unwrap_or("<none>"),
                runtime.map(|m| m.uv_anim()),
                runtime.map(|m| m.falloff_parms()),
                runtime.map(|m| m.falloff_begin()),
                runtime.map(|m| m.falloff_end()),
            );
        }
    }
    let exact_vertex_status = match plan.exact_packed_vertices() {
        Ok(vertices) => format!("READY records={} stride=0x2c", vertices.len()),
        Err(refusal) => format!("REFUSED({refusal:?})"),
    };
    let probe_surfaces = plan
        .surface_sampler_inputs
        .iter()
        .filter(|inputs| inputs.reflection_probe.is_some())
        .count();
    let lightmap_surfaces = plan
        .surface_sampler_inputs
        .iter()
        .filter(|inputs| inputs.secondary_lightmap.is_some())
        .count();
    diag::info!(
        World,
        "world drawsurf tess plan: batches={} verts={} indices={} materials={} retail_vb={} surface_inputs={}/{} probe/lightmap (material execution deferred to Colour product); mark_clip_verts pos={} lmap={} nrm={}",
        scene.batches.len(),
        plan.decoded_vertices().len(),
        plan.indices().len(),
        plan.materials.len(),
        exact_vertex_status,
        probe_surfaces,
        lightmap_surfaces,
        scene.retained_positions.len(),
        scene.retained_lightmap_uvs.len(),
        scene.retained_normals.len(),
    );
    commands.insert_resource(plan);
}
