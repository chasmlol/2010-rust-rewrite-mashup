use crate::drawsurf::scene_depth::{SCENE_DEPTH_FORMAT, SceneDepthTexture};
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::Instant;

use bevy::core_pipeline::core_3d::main_opaque_pass_3d;
use bevy::core_pipeline::upscaling::ViewUpscalingPipeline;
use bevy::core_pipeline::{Core3d, Core3dSystems};
use bevy::mesh::VertexBufferLayout;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::render_phase::TrackedRenderPass;
use bevy::render::render_resource::binding_types::{
    sampler, storage_buffer_read_only, texture_2d, texture_3d, texture_cube, uniform_buffer_sized,
};
use bevy::render::render_resource::{
    BindGroup, BindGroupEntry, BindGroupLayoutDescriptor, BindingResource, Buffer, BufferBinding,
    BufferDescriptor, BufferInitDescriptor, BufferUsages, ColorTargetState, ColorWrites,
    CommandEncoder, CompareFunction, DepthBiasState, DepthStencilState, Face, FrontFace,
    IndexFormat, LoadOp, MultisampleState, Operations, PipelineCache, PolygonMode, PrimitiveState,
    RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPassDescriptor,
    SamplerBindingType, ShaderStages, SpecializedRenderPipelines, StoreOp, TextureFormat,
    TextureSampleType, TextureView, TextureViewDescriptor, VertexAttribute, VertexFormat,
    VertexStepMode,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::view::{ExtractedView, Msaa, ViewTarget};
use bevy::render::{Render, RenderSystems};

use super::ExtractedRenderFrameProducts;
use super::backend::{
    ModelIndexRingCopyRefuse, ModelIndexStream, PackDraw, PackedEmit, PackedListKind,
    SmodelRigidFlush, draw_spot_shadow_map, draw_sun_shadow_map_forced, draw_surf_list_work_colour,
    pack_sun_shadow_frontend, prim_args_from_world_flush, prim_args_u32_index_span,
};
use super::depth_range::{
    GFX_DEPTH_RANGE_VIEWMODEL, depth_range_type_for_draw, reverse_z_viewport_depth,
};
use super::exact_pipeline::{
    ExactModuleSource, ExactPipelinePlan, ExactPipelineRegistry, ExactPipelineSlot,
};
use super::floatz::{self, ExactFloatZResolve};
use super::gpu_contract::{
    WgpuBindLayoutEntry, WgpuBindingKind, WgpuShaderVisibility, WgpuVertexFormat,
};
use super::gpu_prepare::{
    ConstantPackRefusal, PassConstantBuffers, overlay_packed_code_on_banks, split_bind_layout,
};
use super::gpu_resources::{
    RuntimeProgramPortGpuExt, RuntimeUploadedImageRegistry, SamplerTable, TextureBindRefusal,
    UploadedTextureBind, padded_upload_len, write_buffer_padded,
};
use super::shadowmap_spot_gpu::{
    SHADOWMAP_SPOT_COLOR_FORMAT, SHADOWMAP_SPOT_DEPTH_FORMAT, ShadowmapSpotGpu,
    ensure_shadowmap_spot_targets,
};
use super::shadowmap_sun_gpu::{
    SHADOWMAP_SUN_COLOR_FORMAT, SHADOWMAP_SUN_DEPTH_FORMAT, ShadowmapSunGpu,
    ensure_shadowmap_sun_target, scissor_xywh,
};
use super::sm3_wgsl::{PASS_FRAGMENT_ENTRY, alpha_test_fragment_entry};
use super::smodel_cache_gpu::SmodelCacheGpu;
use super::smodel_cached::{
    cached_lighting_location, cached_lighting_vertex_buffers, inject_cached_lighting_attribute,
    lighting_texcoord_register,
};
use super::state::{ChangeState0Host, ChangeState1Host, GfxPassState};
use super::texture_table::{
    self, ExactTextureTable, SceneTextureTables, ShadowTextureTable, TableEpoch,
    TextureTableRefusal,
};
use crate::diag::render_frame_diag::{
    GPU_SPAN_COLOUR, GPU_SPAN_EMISSIVE, GPU_SPAN_FLOATZ, GPU_SPAN_SPOT, GPU_SPAN_SUN,
};
use asset_iw4::{D3DCMP_ALWAYS, D3DCMP_EQUAL, D3DCMP_LESS, D3DCMP_LESSEQUAL};
use render_backend::{MaterialExecView, MaterialRunExecutor, PlaceLanes};
use render_frame::MaterialExecFrame;
use render_frame::{BspCameraLane, RetainedDrawItem, RetainedDrawKind, SmodelPretessRange};
use render_frame::{CODE_MESH_INDEX_CAP, CODE_MESH_VERT_CAP, CODE_MESH_VERT_STRIDE};
use render_frame::{
    FrameProduct, FrameProductKind, FrameProductStatus, PackedFrontendLists, SunShadowForcedFrame,
    SunShadowPartition, SunShadowViewport, TextureBindIdentity,
};
use render_material::{
    ExecutablePassView, MaterialExecution, MaterialGenerationId, MaterialRefusal,
    PackedCodeConstantLane, PackedCodeConstants, PortId, PreparedMaterialTable, RuntimeCodeSources,
    RuntimeMaterialCatalog, RuntimeShaderPair, RuntimeShaderStage, RuntimeSortedMaterialTable,
    TechType, prepared_draw_technique,
};

pub const CODE_TEXTURE_FLOATZ: u32 = 0x0f;

pub const CODE_TEXTURE_RESOLVED_POST_SUN: u32 = 9;

pub const CODE_TEXTURE_SHADOWMAP_SUN: u32 = 6;

pub const CODE_TEXTURE_SHADOWMAP_SPOT: u32 = 7;

/// Geometry that survives frame replacement within a world generation.
#[derive(Clone, Debug, Default)]
pub struct ExtractedStaticGeometry {
    pub world_vertices: Arc<Vec<[u8; asset_iw4::size::GFX_WORLD_VERTEX]>>,
    pub world_layer: Arc<Vec<u8>>,
    pub world_indices: Arc<Vec<u32>>,
    pub world_surface_ranges: Arc<Vec<(u32, u32)>>,
    pub world_vertex_refusal: Option<render_frame::WorldVertexRefusal>,
    pub smodel_vertices: Arc<Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>>,
    pub smodel_indices: Arc<Vec<u32>>,
    pub smodel_surface_ranges: Arc<Vec<(u32, u32)>>,
    pub smodel_vertex_refusal: Option<render_frame::PackedVertexRefusal>,
    pub smodel_cached_vertices: Arc<Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>>,
    pub smodel_surface_verts: Arc<Vec<(u32, u32)>>,
    pub smodel_vertex_lighting: Arc<Vec<[u8; 4]>>,
}

/// Installed world and material resources. Longer-lived than a colour frame:
/// static geometry, ports, catalog and prepared tables stay here.
#[derive(Resource, Clone, Debug, Default)]
pub struct InstalledRenderWorld(Arc<RenderWorldData>);

impl InstalledRenderWorld {
    pub fn new(data: RenderWorldData) -> Self {
        Self(Arc::new(data))
    }
}

impl std::ops::Deref for InstalledRenderWorld {
    type Target = RenderWorldData;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Consumed by publication; no mutable access through an installed handle.
#[derive(Clone, Debug, Default)]
pub struct RenderWorldData {
    pub generation: MaterialGenerationId,
    pub world_generation: frame::WorldGeneration,
    pub world_products: frame::WorldProducts,
    pub smc_revision: Option<u64>,
    pub ports: Arc<Vec<super::AdmittedExactPort>>,
    pub static_geometry: Arc<ExtractedStaticGeometry>,
    pub smodel_pretess_indices: std::sync::Arc<Vec<u16>>,
    pub smodel_index_layout_revision: u64,
    pub smc_index_baked: Arc<Vec<u16>>,
    pub sampler_table: Option<SamplerTable>,
    pub image_handles: super::gpu_resources::RuntimeImageHandles,
    pub catalog: Option<Arc<RuntimeMaterialCatalog>>,
    pub prepared: Option<Arc<PreparedMaterialTable>>,
    pub sorted_material_names: Arc<Vec<String>>,
    pub shader_program_names: Arc<Vec<Option<String>>>,
    pub sun_effects: Option<render_frame::SunEffectsDef>,
}

/// Commands and changed data of the current frame. Names the installed world by
/// generation instead of carrying world/material tables again.
#[derive(Resource, Clone, Debug, Default)]
pub struct PublishedRenderFrame {
    world: InstalledRenderWorld,
    data: Arc<RenderFrameData>,
}

impl PublishedRenderFrame {
    pub fn seal(world: InstalledRenderWorld, data: RenderFrameData) -> Self {
        assert_eq!(world.generation, data.generation);
        assert_eq!(world.world_generation, data.world_generation);
        Self {
            world,
            data: Arc::new(data),
        }
    }
    pub fn world(&self) -> &InstalledRenderWorld {
        &self.world
    }
}

impl std::ops::Deref for PublishedRenderFrame {
    type Target = RenderFrameData;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

#[derive(Clone, Debug, Default)]
pub struct RenderFrameData {
    pub frame_products: ExtractedRenderFrameProducts,
    pub generation: MaterialGenerationId,
    pub world_generation: frame::WorldGeneration,
    pub sun_shadow: Option<SunShadowForcedFrame>,
    pub sun_effects: Option<render_frame::SunEffectsFrame>,
    pub warm_pipelines: bool,
    pub pipeline_world_materials: Arc<std::collections::HashSet<u16>>,
    pub pipeline_smodel_materials: Arc<std::collections::HashSet<u16>>,
    pub pipeline_demand_revision: u64,
    pub smc_vb_patches: Vec<(lighting_iw4::SmcPatchLock, Vec<u8>)>,
    pub smc_ib_patches: Vec<(u32, Vec<u8>)>,
    pub xmodel_vertices: Arc<Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>>,
    pub xmodel_indices: Arc<Vec<u32>>,
    pub xmodel_surface_ranges: Arc<Vec<(u32, u32)>>,
    pub xmodel_vertex_refusal: Option<render_frame::PackedVertexRefusal>,
    pub fx_vertices: Arc<Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>>,
    pub fx_indices: Arc<Vec<u32>>,
    pub fx_surface_ranges: Arc<Vec<(u32, u32)>>,
    pub fx_vertex_refusal: Option<render_frame::PackedVertexRefusal>,
    pub fx_revision: u64,
    pub xmodel_revision: u64,
    pub xmodel_topology_revision: u64,
    pub xmodel_packed_segments: render_frame::PackedSegments,
    pub particle_cloud_vertices: Arc<Vec<[u8; fx_iw4::GFX_POS_TEX_VERTEX_STRIDE]>>,
    pub particle_cloud_indices: Arc<Vec<u32>>,
    pub particle_cloud_surface_ranges: Arc<Vec<(u32, u32)>>,
    pub particle_cloud_template: (usize, usize),
    pub particle_cloud_revision: u64,
    pub mark_mesh_vertices: Arc<Vec<[u8; asset_iw4::size::GFX_WORLD_VERTEX]>>,
    pub mark_mesh_indices: Arc<Vec<u16>>,
    pub mark_mesh_surface_ranges: Arc<Vec<(u32, u32)>>,
    pub mark_mesh_revision: u64,
    pub glass_mesh_vertices: Arc<Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>>,
    pub glass_mesh_indices: Arc<Vec<u32>>,
    pub glass_mesh_surface_ranges: Arc<Vec<(u32, u32)>>,
    pub glass_mesh_revision: u64,
    pub glass_mesh_vertex_refusal: Option<render_frame::PackedVertexRefusal>,
    pub glass_mesh_bounds: Arc<Vec<[f32; 4]>>,
    pub exec_frame: MaterialExecFrame,
}

/// Borrowed view of the two extract resources. Frame does not own world tables.
#[derive(Clone, Copy)]
struct ExtractedColourRefs<'a> {
    world: &'a InstalledRenderWorld,
    frame: &'a PublishedRenderFrame,
}

impl<'a> ExtractedColourRefs<'a> {
    fn new(world: &'a InstalledRenderWorld, frame: &'a PublishedRenderFrame) -> Self {
        Self { world, frame }
    }
}

mod geometry;
mod indirect;
mod pipeline;
mod prepare_camera;
mod record;
mod residency;
mod smodel_skinned;

pub use pipeline::cached_lighting_port_variant;

#[derive(Resource, Default)]
struct ExactColourGeometry {
    generation: MaterialGenerationId,
    world_generation: frame::WorldGeneration,
    world_products: frame::WorldProducts,
    world_vertex: Option<Buffer>,
    world_layer: Option<Buffer>,
    world_effect: Option<Buffer>,
    world_index: Option<Buffer>,
    world_surface_ranges: Vec<(u32, u32)>,
    world_vertex_count: usize,
    world_layer_count: usize,
    world_index_count: usize,

    world_cpu_indices: Vec<u32>,
    smodel_vertex: Option<Buffer>,
    smodel_vertex_lighting: Option<Buffer>,
    smodel_index: Option<Buffer>,
    smodel_surface_ranges: Vec<(u32, u32)>,
    smodel_vertex_count: usize,
    smodel_index_count: usize,
    smodel_cached_vertex: Option<Buffer>,
    smodel_cached_index: Option<Buffer>,
    smodel_cached_vertex_count: usize,

    xmodel: residency::GpuMesh,
    xmodel_surface_ranges: Vec<(u32, u32)>,

    xmodel_resident_segments: render_frame::PackedSegments,

    xmodel_resident_allocation: u64,
    fx_vertex: Option<Buffer>,
    fx_index: Option<Buffer>,
    fx_surface_ranges: Vec<(u32, u32)>,
    fx_vertex_count: usize,
    fx_index_count: usize,
    fx_revision: u64,

    fx_copy_dst: bool,
    particle_cloud: residency::GpuMesh,
    particle_cloud_template_resident: Option<(u64, u64, (usize, usize))>,
    particle_cloud_surface_ranges: Vec<(u32, u32)>,
    mark_mesh: residency::GpuMesh,
    mark_mesh_surface_ranges: Vec<(u32, u32)>,
    glass_mesh: residency::GpuMesh,
    glass_mesh_surface_ranges: Vec<(u32, u32)>,
    last_xmodel_gpu_hash: Option<i64>,
    neutral_vertex_lighting: Option<Buffer>,
    neutral_vertex_lighting_count: usize,
}

pub const NEUTRAL_VERTEX_LIGHTING: [u8; 4] = [45, 45, 45, 255];

#[derive(Resource, Default)]
struct ExactColourPipeline {
    generation: MaterialGenerationId,
    ports: Vec<ExactColourPortGpu>,
    by_id: HashMap<PortId, usize>,
}

impl ExactColourPipeline {
    fn get(&self, id: PortId) -> Option<&ExactColourPortGpu> {
        self.by_id.get(&id).map(|&index| &self.ports[index])
    }

    fn rebuild_index(&mut self) {
        self.by_id.clear();
        self.by_id.reserve(self.ports.len());
        for (index, port) in self.ports.iter().enumerate() {
            self.by_id.insert(port.port.id(), index);
        }
    }
}

struct ExactColourPortGpu {
    constants_layout: BindGroupLayoutDescriptor,
    textures_layout: BindGroupLayoutDescriptor,
    vertex_buffers: Vec<VertexBufferLayout>,
    cached_source: Option<Arc<str>>,
    cached_vertex_buffers: Vec<VertexBufferLayout>,
    port: super::AdmittedExactPort,
}

#[derive(Resource, Default)]
struct ExactColourBindingCache {
    generation: MaterialGenerationId,
    views_revision: u64,
    textures: [HashMap<BoundTextureKey, Arc<[u32]>>; 4],
}

#[derive(Resource, Default)]
struct ExactShadowBindingCache {
    generation: MaterialGenerationId,
    views_revision: u64,
    textures: HashMap<BoundTextureKey, Arc<[u32]>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct BoundTextureKey {
    identity: TextureBindIdentity,
    spot_shadow_select: Option<u8>,
}

impl ExactColourBindingCache {
    fn open_epoch(&mut self, generation: MaterialGenerationId, views_revision: u64) {
        if self.generation == generation && self.views_revision == views_revision {
            return;
        }
        self.generation = generation;
        self.views_revision = views_revision;
        for slots in &mut self.textures {
            slots.clear();
        }
    }

    fn interned_n(&self) -> usize {
        self.textures.iter().map(HashMap::len).sum()
    }
}

impl ExactShadowBindingCache {
    fn open_epoch(&mut self, generation: MaterialGenerationId, views_revision: u64) {
        if self.generation == generation && self.views_revision == views_revision {
            return;
        }
        self.generation = generation;
        self.views_revision = views_revision;
        self.textures.clear();
    }

    fn clear_textures(&mut self) {
        self.textures.clear();
    }
}

fn open_scene_table_epoch(
    binding_cache: &mut ExactColourBindingCache,
    scene_tables: &mut SceneTextureTables,
    scratch: &mut ColourSubmitScratch,
    uploaded: &RuntimeUploadedImageRegistry,
    generation: MaterialGenerationId,
) {
    let epoch = TableEpoch {
        generation,
        replaced_revision: uploaded.replaced_revision(),
    };
    for table in &mut scene_tables.0 {
        table.open_epoch(epoch);
    }
    scratch.prepared_scene_epoch = epoch;
    binding_cache.open_epoch(generation, uploaded.views_revision());
}

fn open_shadow_table_epoch(
    binding_cache: &mut ExactShadowBindingCache,
    shadow_table: &mut ShadowTextureTable,
    scratch: &mut ShadowSubmitScratch,
    uploaded: &RuntimeUploadedImageRegistry,
    generation: MaterialGenerationId,
) {
    let epoch = TableEpoch {
        generation,
        replaced_revision: uploaded.replaced_revision(),
    };
    shadow_table.open_epoch(epoch);
    scratch.prepared_shadow_epoch = epoch;
    binding_cache.open_epoch(generation, uploaded.views_revision());
}

#[derive(Resource, Default)]
struct ExactPipelineKickCache {
    current: Vec<ExactPipelineSlot>,
    scheduled: HashSet<ExactColourPipelineKey>,
    generation: Option<MaterialGenerationId>,
    views: Vec<(TextureFormat, u32)>,
    demand_revision: u64,
}

#[derive(Default)]
struct GpuConstantArena {
    buffer: Option<Buffer>,
    capacity: u64,
    bind_group: Option<BindGroup>,

    uploaded_len: usize,

    dirty_scratch: Vec<(usize, usize)>,
}

#[derive(Resource, Default)]
struct ExactConstantArena {
    generation: MaterialGenerationId,
    gpu: [GpuConstantArena; COLOUR_PREPARE_LANES],
}

const COLOUR_PREPARE_LANES: usize = 4;

#[derive(Default)]
struct ColourPrepareLane {
    run_pack: RunPackCache,
    arena_pack: ArenaPack,
    executor: MaterialRunExecutor,
    prepared: Vec<PreparedExactDraw>,
    submitted_keys: Vec<u64>,
    pending_viewmodel_prepared: Vec<PreparedExactDraw>,
    pending_viewmodel_keys: Vec<u64>,
    world_exec_ready_keys: Vec<u64>,
}

#[derive(Resource, Default)]
struct ShadowmapSunArena {
    generation: MaterialGenerationId,
    gpu: [GpuConstantArena; super::SUN_SHADOW_PARTITION_COUNT as usize],
}

#[derive(Resource, Default)]
struct ShadowmapSpotArena {
    generation: MaterialGenerationId,
    gpu: GpuConstantArena,

    pack: ArenaPack,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct ExactColourPipelineKey {
    target: TextureFormat,
    depth_format: TextureFormat,
    samples: u32,
    state0: ChangeState0Host,
    state1: ChangeState1Host,

    pub(super) port: PortId,

    cached_lighting: bool,

    mark_mesh: bool,

    forward_z: bool,
}

fn exact_colour_target_format(target: TextureFormat, srgb_write: bool) -> TextureFormat {
    if srgb_write {
        target.add_srgb_suffix()
    } else {
        target
    }
}

struct ExactColourTargetViews {
    main_raw: TextureView,
    main_srgb: TextureView,
    sampled_raw: Option<TextureView>,
    sampled_srgb: Option<TextureView>,
}

impl ExactColourTargetViews {
    fn new(target: &ViewTarget) -> Self {
        let raw_format = target.main_texture_format();
        let srgb_format = raw_format.add_srgb_suffix();
        Self {
            main_raw: target.main_texture_view().clone(),
            main_srgb: target.main_texture().create_view(&TextureViewDescriptor {
                label: Some("iw4_exact_colour_main_srgb"),
                format: Some(srgb_format),
                ..Default::default()
            }),
            sampled_raw: target.sampled_main_texture_view().cloned(),
            sampled_srgb: target.sampled_main_texture().map(|texture| {
                texture.create_view(&TextureViewDescriptor {
                    label: Some("iw4_exact_colour_sampled_srgb"),
                    format: Some(srgb_format),
                    ..Default::default()
                })
            }),
        }
    }

    fn attachment_views(&self, srgb_write: bool) -> (&TextureView, Option<&TextureView>) {
        let (main, sampled) = if srgb_write {
            (&self.main_srgb, self.sampled_srgb.as_ref())
        } else {
            (&self.main_raw, self.sampled_raw.as_ref())
        };
        match sampled {
            Some(sampled) => (sampled, Some(main)),
            None => (main, None),
        }
    }
}

fn exact_fragment_entry(
    target: TextureFormat,
    forward_z: bool,
    alpha_test: Option<d3d9_state::AlphaTest>,
) -> String {
    if forward_z && target == TextureFormat::R32Float {
        PASS_FRAGMENT_ENTRY.to_owned()
    } else {
        alpha_test_fragment_entry(alpha_test)
    }
}

fn exact_pipeline_plan(
    ports: &ExactColourPipeline,
    registry: &ExactPipelineRegistry,
    device: &RenderDevice,
    key: ExactColourPipelineKey,
) -> (ExactModuleSource, ExactPipelinePlan) {
    let port = ports
        .get(key.port)
        .expect("ExactColourPipelineKey.port is an admitted PortId");
    let depth_compare = if key.state1.depth_test_enable {
        Some(if key.forward_z {
            d3d_zfunc(key.state1.depth_func)
        } else {
            d3d_zfunc_to_reverse_z(key.state1.depth_func)
        })
    } else {
        Some(CompareFunction::Always)
    };
    let cached_lighting = key.cached_lighting && port.cached_source.is_some();
    let source = match (cached_lighting, port.cached_source.as_ref()) {
        (true, Some(cached)) => ExactModuleSource::CachedLighting(cached.clone()),
        _ => ExactModuleSource::Port(port.port.shared_module()),
    };
    let plan = ExactPipelinePlan {
        label: format!(
            "iw4_exact_colour/{:016x}/{}",
            key.port.vertex_program_hash, key.port.vertex_type
        ),
        fragment_entry: exact_fragment_entry(key.target, key.forward_z, key.state0.alpha_test),
        vertex_buffers: if cached_lighting && !port.cached_vertex_buffers.is_empty() {
            port.cached_vertex_buffers.clone()
        } else {
            let mut buffers = port.vertex_buffers.clone();

            if key.mark_mesh
                && let Some(buffer) = buffers.first_mut()
            {
                buffer.array_stride = asset_iw4::size::GFX_WORLD_VERTEX as u64;
            }
            buffers
        },
        targets: vec![Some(ColorTargetState {
            format: key.target,

            blend: if key.target == TextureFormat::R32Float {
                None
            } else {
                key.state0.blend.blend_state()
            },
            write_mask: if key.target == TextureFormat::R32Float {
                key.state0.colour_writes() & ColorWrites::RED
            } else {
                key.state0.colour_writes()
            },
        })],
        primitive: exact_primitive_state(key.state0.cull, key.state0.line_fill),
        depth_stencil: DepthStencilState {
            format: key.depth_format,
            depth_write_enabled: Some(key.state1.depth_write),
            depth_compare,
            stencil: super::scene_depth::stencil_state(key.state1.stencil),
            bias: if key.forward_z {
                polygon_offset_bias_forward_z(key.state1.polyoffset_level)
            } else {
                polygon_offset_bias(key.state1.polyoffset_level)
            },
        },
        multisample: MultisampleState {
            count: key.samples,
            ..Default::default()
        },
        constants_layout: registry.bind_group_layout(device, &port.constants_layout),
        textures_layout: registry.bind_group_layout(device, &port.textures_layout),
        minecraft_layout: registry
            .bind_group_layout(device, &super::minecraft_light::layout_descriptor()),
    };
    (source, plan)
}

fn request_exact_pipeline(
    registry: &mut ExactPipelineRegistry,
    ports: &ExactColourPipeline,
    device: &RenderDevice,
    key: ExactColourPipelineKey,
) -> ExactPipelineSlot {
    if let Some(slot) = registry.slot(&key) {
        return slot;
    }
    let (source, plan) = exact_pipeline_plan(ports, registry, device, key);
    registry.request(key, source, plan)
}

fn d3d_zfunc(depth_func_index: u8) -> CompareFunction {
    let d3d = asset_iw4::S_DEPTH_TEST_TABLE
        .get(usize::from(depth_func_index))
        .copied()
        .unwrap_or(D3DCMP_LESSEQUAL);
    match d3d {
        D3DCMP_ALWAYS => CompareFunction::Always,
        D3DCMP_LESS => CompareFunction::Less,
        D3DCMP_EQUAL => CompareFunction::Equal,
        D3DCMP_LESSEQUAL => CompareFunction::LessEqual,
        _ => CompareFunction::LessEqual,
    }
}

fn d3d_zfunc_to_reverse_z(depth_func_index: u8) -> CompareFunction {
    let d3d = asset_iw4::S_DEPTH_TEST_TABLE
        .get(usize::from(depth_func_index))
        .copied()
        .unwrap_or(D3DCMP_LESSEQUAL);
    match d3d {
        D3DCMP_ALWAYS => CompareFunction::Always,
        D3DCMP_LESS => CompareFunction::Greater,
        D3DCMP_EQUAL => CompareFunction::Equal,
        D3DCMP_LESSEQUAL => CompareFunction::GreaterEqual,
        _ => CompareFunction::GreaterEqual,
    }
}

pub(super) fn exact_primitive_state(cull: u8, line_fill: bool) -> PrimitiveState {
    PrimitiveState {
        front_face: FrontFace::Cw,
        cull_mode: match cull {
            1 => Some(Face::Back),
            2 => Some(Face::Front),
            _ => None,
        },
        polygon_mode: if line_fill {
            PolygonMode::Line
        } else {
            PolygonMode::Fill
        },
        ..Default::default()
    }
}

fn polygon_offset_bias(level: u8) -> DepthBiasState {
    let (slope_scale, constant) = asset_iw4::polygon_offset_wgpu_defaults(u32::from(level));
    DepthBiasState {
        constant: -constant,
        slope_scale: -slope_scale,
        clamp: 0.0,
    }
}

fn polygon_offset_bias_forward_z(level: u8) -> DepthBiasState {
    let (slope_scale, constant) = asset_iw4::polygon_offset_wgpu_defaults(u32::from(level));
    DepthBiasState {
        constant,
        slope_scale,
        clamp: 0.0,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum GpuSubmitRefusal {
    NoExactPort {
        pair: RuntimeShaderPair,
    },
    ConstantPack(ConstantPackRefusal),
    TextureBind(TextureBindRefusal),
    TextureTable(TextureTableRefusal),
    ProductDependencyNotReady {
        product: FrameProductKind,
    },
    UnsupportedTessKind,

    MultipleCameraViews {
        views: u32,
    },
    WorldVertex(render_frame::WorldVertexRefusal),
    PackedVertex(render_frame::PackedVertexRefusal),
    MissingSurfaceRange {
        surf: u16,
    },
    MissingSmodelRange {
        surface: u32,
    },
    MissingXModelRange {
        surface: u32,
    },
    MissingCodeMeshRange {
        draw: u32,
    },
    MissingParticleCloudRange {
        draw: u32,
    },
    MissingMarkMeshRange {
        draw: u32,
    },
    MissingGlassMeshRange {
        draw: u32,
    },
    EmptyIndexRange {
        surf: u16,
    },
    EmptySmodelIndexRange {
        surface: u32,
    },
    EmptyXModelIndexRange {
        surface: u32,
    },
    EmptyCodeMeshIndexRange {
        draw: u32,
    },
    EmptyParticleCloudIndexRange {
        draw: u32,
    },
    EmptyMarkMeshIndexRange {
        draw: u32,
    },
    EmptyGlassMeshIndexRange {
        draw: u32,
    },
    PipelineNotReady,
    SmodelCacheIndexEmpty {
        placement: u32,
    },
    SmodelCacheIndicesMissing {
        cache_index: u16,
    },
    SmodelXSurfacePathUnread {
        placement: u32,
    },

    SmodelCachedWithoutDestRange {
        placement: u32,
    },
    SmodelSkinnedDestMissing {
        placement: u32,
    },
    ConstantArenaMissing,

    WorldPretessSpanBeyondLimit {
        start: u32,
        count: u32,
        logical_len: u32,
        epoch: u64,
    },

    WorldPretessEpochMismatch {
        span_epoch: u64,
        layout_epoch: u64,
    },
}

#[derive(Resource, Default)]
pub(super) struct ExactColourSubmitCensus {
    ready_draws: u32,
    refused_draws: u32,
    log_frame: u32,
    pub submitted_keys: Vec<u64>,
    pub world_exec_ready_keys: Vec<u64>,
    gpu_ready: Option<u32>,
    pub set_bind_group0_n: Option<u32>,
    pub set_bind_group1_n: Option<u32>,
    pub material_runs_n: Option<u32>,
    pub pass_setups_n: Option<u32>,
    pub obj_binds_n: Option<u32>,
    pub shell_hits_n: Option<u32>,
    pub shell_misses_n: Option<u32>,
    pub overlay_const_writes_n: Option<u32>,
    pub overlay_need_known_n: Option<u32>,
    code_mesh_gpu_kind: Option<i32>,
    sun_shadow_prepare_ms: Option<f32>,
    sun_shadow_patch_ms: Option<f32>,
    sun_shadow_arena_ms: Option<f32>,
    sun_shadow_record_ms: Option<f32>,
    sun_shadow_finish_ms: Option<f32>,
    sun_shadow_queue_ms: Option<f32>,
    sun_shadow_unnamed_ms: Option<f32>,
    sun_shadow_static_hit: Option<u32>,
    sun_shadow_static_n: Option<u32>,
    sun_shadow_dynamic_n: Option<u32>,
    sun_shadow_wvp_intern_hit: Option<u32>,
    sun_shadow_wvp_intern_miss: Option<u32>,
    sun_shadow_wvp_intern_n: Option<u32>,
    sun_shadow_wvp_unique_base: Option<u32>,
    sun_shadow_wvp_unique_wvp: Option<u32>,
    sun_shadow_state_pipe_n: Option<u32>,
    sun_shadow_state_tess_n: Option<u32>,
    sun_shadow_state_bind_n: Option<u32>,
    sun_shadow_state_off_n: Option<u32>,
    sun_shadow_state_group_n: Option<u32>,
    sun_shadow_state_run_n: Option<u32>,
    sun_shadow_state_run_max: Option<u32>,
    sun_shadow_state_top10: Option<u32>,
    frame: ExactColourFrameCensus,
}

#[derive(Default)]
struct ExactColourFrameCensus {
    pub bsp_submitted_surfaces: [u32; 4],
    pub bsp_submit_refused_surfaces: [u32; 4],
    pub bsp_drawn_surfaces: [u32; 4],
    pub bsp_draw_refused_surfaces: [u32; 4],
    submit_prepare_ms: Option<f32>,
    colour_submit_ms: Option<f32>,
    submit_encode_ms: Option<f32>,
    submit_gather_ms: Option<f32>,
    pass_end_ms: Option<f32>,
    encoder_finish_ms: Option<f32>,
    submit_arena_ms: Option<f32>,
    submit_record_ms: Option<f32>,
    pack_intern_hit_n: Option<u32>,
    pack_intern_miss_n: Option<u32>,
    pack_arena_share_n: Option<u32>,
    gpu_exec_reuse_n: Option<u32>,
    gpu_exec_unique_n: Option<u32>,
    pack_overlay_n: Option<u32>,
    pack_overlay_row_n: Option<u32>,
    pack_overlay_pixel_share_n: Option<u32>,
    pack_arena_vertex_n: Option<u32>,
    pack_arena_pixel_n: Option<u32>,
    pack_seed_n: Option<u32>,
    pack_walk_n: Option<u32>,
    tex_bind_hit_n: Option<u32>,
    tex_bind_miss_n: Option<u32>,
    markmesh_hits: Option<u32>,
    markmesh_prepared: Option<u32>,
    last_markmesh_refusal: Option<String>,
    last_markmesh_exec_skip: Option<String>,
    markmesh_missing_58: Option<u32>,
    last_mark_packed_custom: Option<u8>,
    last_mark_packed_scene_light: Option<u8>,
    last_mark_lmap_sampler: Option<u32>,
    glassmesh_hits: Option<u32>,
    glassmesh_prepared: Option<u32>,
    last_glassmesh_exec_skip: Option<String>,
    last_glassmesh_refusal: Option<String>,
    last_glass_packed_probe: Option<u8>,
    last_glass_probe_sampler: Option<u32>,
    set_bind_group_n: Option<u32>,
    pub set_state_n: Option<u32>,
    pub multi_draw_n: Option<u32>,
    pub multi_draw_commands_n: Option<u32>,
    gpu_prepared: Option<u32>,
    gpu_world_ready: Option<u32>,
    gpu_smodel_ready: Option<u32>,
    gpu_xmodel_ready: Option<u32>,
    end_depth_restore_n: Option<u32>,
    end_depth_range_type: Option<i32>,
    sun_shadow_gpu: Option<u32>,
    sun_shadow_gpu_miss: Option<u32>,
    sun_shadow_gpu_cause: Option<String>,
    sun_shadow_gpu_causes: Option<String>,
    spot_shadow_gpu: Option<u32>,
    spot_shadow_gpu_miss: Option<u32>,
    spot_shadow_gpu_cause: Option<String>,
    spot_shadow_slot_n: Option<u32>,
    sun_shadow_submit_ms: Option<f32>,
    sun_shadow_world_ib_n: Option<u32>,
    world_index_gaps: Option<u32>,
    world_run_indices_n: Option<u32>,
    world_material_runs: Option<u32>,
    world_material_runs_seq: Option<u32>,
    world_key_runs: Option<u32>,
    world_key_runs_seq: Option<u32>,
    world_mixed_breaks: Option<u32>,
    world_gathered: Option<u32>,
    world_ib_skip: Option<u32>,
    world_gpu_runs: Option<u32>,
    world_gpu_runs_seq: Option<u32>,
    world_sampler_runs_seq: Option<u32>,
    world_probe_runs_seq: Option<u32>,
    world_light_runs_seq: Option<u32>,
    smodel_reuse_n: Option<u32>,
    xmodel_reuse_n: Option<u32>,
    xmodel_material_runs: Option<u32>,
    smodel_index_gaps: Option<u32>,
    smodel_material_runs: Option<u32>,
    smodel_material_runs_seq: Option<u32>,
    smodel_material_run_max: Option<u32>,
    smodel_same_surface_n: Option<u32>,
    smodel_unique_surfaces: Option<u32>,
    smodel_hits: Option<u32>,
    smodel_lighting_runs: Option<u32>,
    smodel_lighting_run_max: Option<u32>,
    smodel_pretess_runs: Option<u32>,
    smodel_pretess_hits: Option<u32>,
    smodel_pretess_verts: Option<u32>,
    smodel_pretess_indices: Option<u32>,
    smodel_cached_lighting: Option<u32>,
    smodel_pretess_local: Option<u32>,
    smodel_pretess_length1: Option<u32>,
    smodel_pretess_skip: Option<u32>,
    submit_cause: Option<String>,
    submit_cause2: Option<String>,
    gpu_not_ready_n: Option<u32>,
    gpu_no_port_n: Option<u32>,
    pnr_smodel_mat: Option<String>,
    pnr_world_mat: Option<String>,
    pnr_smodel_ps: Option<String>,
    pnr_world_ps: Option<String>,
    pnr_smodel_key_n: Option<u32>,
    pnr_world_key_n: Option<u32>,
    pnr_port_n: Option<u32>,
    gpu_smodel_bind_mat: Option<String>,
}

fn reset_exact_colour_census(census: &mut ExactColourSubmitCensus) {
    census.submitted_keys.clear();
    census.frame = ExactColourFrameCensus::default();
}

fn exec_tables(
    extracted: ExtractedColourRefs<'_>,
) -> Option<(&RuntimeMaterialCatalog, &PreparedMaterialTable)> {
    Some((
        extracted.world.catalog.as_deref()?,
        extracted.world.prepared.as_deref()?,
    ))
}

fn colour_census_clock(on: bool) -> Option<Instant> {
    on.then(Instant::now)
}

fn colour_census_ms(start: Option<Instant>) -> Option<f32> {
    start.map(|t| t.elapsed().as_secs_f32() * 1000.0)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RecordCensus {
    group0: u32,
    group1: u32,

    state: u32,

    multi_draws: u32,
    multi_draw_commands: u32,
}

impl RecordCensus {
    fn total(self) -> u32 {
        self.group0.saturating_add(self.group1)
    }

    fn add(&mut self, other: Self) {
        self.group0 = self.group0.saturating_add(other.group0);
        self.group1 = self.group1.saturating_add(other.group1);
        self.state = self.state.saturating_add(other.state);
        self.multi_draws = self.multi_draws.saturating_add(other.multi_draws);
        self.multi_draw_commands = self
            .multi_draw_commands
            .saturating_add(other.multi_draw_commands);
    }
}

#[derive(Default)]
struct ShadowExecScratch {
    executor: MaterialRunExecutor,
    code_sources: RuntimeCodeSources,
    pack_draws: Vec<PackDraw>,
}

#[derive(Resource, Default)]
struct ColourSubmitScratch {
    lanes: [ColourPrepareLane; COLOUR_PREPARE_LANES],

    world_exec_ready_keys: Vec<u64>,
    prepared: Vec<PreparedExactDraw>,
    submitted_keys: Vec<u64>,
    pending_viewmodel_prepared: Vec<PreparedExactDraw>,
    pending_viewmodel_keys: Vec<u64>,
    pack_draws: Vec<PackDraw>,

    pack_plan: Option<ColourPackPlan>,

    skinned_tess: smodel_skinned::SmodelSkinnedTess,

    prepared_scene_epoch: TableEpoch,

    logged_skinned: Option<(usize, usize)>,
}

#[derive(Resource, Default)]
struct ShadowSubmitScratch {
    sun_exec: ShadowExecScratch,
    spot_exec: ShadowExecScratch,

    skinned_tess: smodel_skinned::SmodelSkinnedTess,

    sun_prepared: PreparedSunWork,
    spot_prepared: PreparedSpotWork,

    prepared_shadow_epoch: TableEpoch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WorldPretessKey {
    world_generation: frame::WorldGeneration,
    geometry_generation: MaterialGenerationId,
    world_run_revision: u64,
    colour_world: u64,
    light_world: u64,
    emissive_world: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ColourPackKey {
    colour_digest: u64,
    light_digest: u64,
    emissive_digest: u64,
    pretess: WorldPretessKey,
    smodel_index_count: usize,
    xmodel_topology_revision: u64,
    xmodel_index_count: usize,
}

struct ColourRowPlan {
    draw_index: u32,
    world_surface_override: Option<u16>,

    technique: TechType,
    index_span: Option<(u32, u32)>,

    layout_epoch: u64,
}

struct ColourPackPlan {
    key: ColourPackKey,
    packed: PackedFrontendLists,
    work: render_backend::ColourDrawListWork,

    world_rows: Vec<Option<WorldPackedRowMeta>>,

    row_plan: Vec<ColourRowPlan>,
}

fn colour_pack_key(
    colour: &FrameProduct,
    light: &FrameProduct,
    emissive: &FrameProduct,
    pretess: WorldPretessKey,
    smodel_index_count: usize,
    xmodel_topology_revision: u64,
    xmodel_index_count: usize,
) -> ColourPackKey {
    ColourPackKey {
        colour_digest: colour.list_digest,
        light_digest: light.list_digest,
        emissive_digest: emissive.list_digest,
        pretess,
        smodel_index_count,
        xmodel_topology_revision,
        xmodel_index_count,
    }
}

fn mix_u64(id: &mut u64, word: u64) {
    *id ^= word;
    *id = id.wrapping_mul(0x0000_0100_0000_01b3);
}

fn mix_surface_samplers_gpu(id: &mut u64, samplers: super::SurfaceSamplerInputs) {
    let encode = |value: Option<u8>| value.map_or(0, |value| u64::from(value) + 1);
    mix_u64(id, encode(samplers.reflection_probe.map(|value| value.0)));
    mix_u64(id, encode(samplers.primary_lightmap.map(|value| value.0)));
    mix_u64(id, encode(samplers.secondary_lightmap.map(|value| value.0)));
}

fn expanded_world_id(product: &FrameProduct, run_surfs: &[u16]) -> u64 {
    let mut id = 0xcbf2_9ce4_8422_2325;
    for draw in &product.ordered_draws {
        let RetainedDrawKind::World {
            surf, run, run_off, ..
        } = draw.kind
        else {
            continue;
        };
        mix_u64(&mut id, u64::from(surf));
        mix_u64(&mut id, u64::from(run));
        mix_u64(&mut id, u64::from(run_off));
        mix_u64(&mut id, draw.key);
        mix_surface_samplers_gpu(&mut id, draw.surface_samplers);
        let members = run.max(1);
        for offset in 0..members {
            let member = if run <= 1 {
                surf
            } else {
                run_surfs
                    .get(run_off as usize + usize::from(offset))
                    .copied()
                    .unwrap_or(u16::MAX)
            };
            mix_u64(&mut id, u64::from(member));
        }
    }
    id
}

fn world_pretess_key(
    colour: &FrameProduct,
    light: &FrameProduct,
    emissive: &FrameProduct,
    run_surfs: &[u16],
    world_generation: frame::WorldGeneration,
    geometry_generation: MaterialGenerationId,
    world_run_revision: u64,
) -> WorldPretessKey {
    WorldPretessKey {
        world_generation,
        geometry_generation,
        world_run_revision,
        colour_world: expanded_world_id(colour, run_surfs),
        light_world: expanded_world_id(light, run_surfs),
        emissive_world: expanded_world_id(emissive, run_surfs),
    }
}

#[derive(Resource, Default)]
struct CameraPrepareState {
    active: bool,
    samples: u32,
    needs_floatz: bool,
    needs_resolved_scene: bool,
    world_ib_skip: bool,
    ready_draws: u32,
    refused_draws: u32,
    pipeline_not_ready: u32,
    last_refusal: Option<GpuSubmitRefusal>,
    submit_refusals: BTreeMap<(&'static str, &'static str), u32>,
    exec_refused: u32,
    exec_refusals: BTreeMap<(&'static str, &'static str), u32>,
    unsupported_state: UnsupportedStateCensus,
    bsp_submit_refused_surfaces: [u32; 4],
    pnr_smodel_mats: BTreeMap<String, u32>,
    pnr_world_mats: BTreeMap<String, u32>,
    pnr_smodel_ps: BTreeMap<String, u32>,
    pnr_world_ps: BTreeMap<String, u32>,
    pnr_smodel_keys: HashSet<u64>,
    pnr_world_keys: HashSet<u64>,
    pnr_ports: HashSet<PortId>,
    bind_smodel_mats: BTreeMap<String, u32>,
    last_markmesh_refusal: Option<GpuSubmitRefusal>,
    last_markmesh_exec_skip: Option<&'static str>,
    markmesh_missing_58: u32,
    last_glassmesh_exec_skip: Option<&'static str>,
    last_glassmesh_refusal: Option<GpuSubmitRefusal>,
    last_mark_packed_custom: Option<u8>,
    last_mark_packed_scene_light: Option<u8>,
    last_mark_lmap_sampler: Option<u32>,
    last_glass_packed_probe: Option<u8>,
    last_glass_probe_sampler: Option<u32>,
    markmesh_hits: usize,
    glassmesh_hits: usize,
    prepared_hits: u32,
    prepare_cost: PrepareCost,
    colour_run_census: render_backend::MaterialRunCensus,
    focused_object_id: Option<u16>,
    viewmodel_held: usize,
}

pub fn bind_group_layout_from_entries(
    label: &'static str,
    entries: &[WgpuBindLayoutEntry],
    dynamic_uniforms: bool,
) -> BindGroupLayoutDescriptor {
    let mut built = Vec::with_capacity(entries.len());
    for entry in entries {
        let visibility = match entry.visibility {
            WgpuShaderVisibility::Vertex => ShaderStages::VERTEX,
            WgpuShaderVisibility::Fragment => ShaderStages::FRAGMENT,
            WgpuShaderVisibility::VertexFragment => ShaderStages::VERTEX_FRAGMENT,
        };
        let builder = match entry.kind {
            WgpuBindingKind::UniformBuffer { min_size } => {
                uniform_buffer_sized(dynamic_uniforms, NonZeroU64::new(min_size))
                    .visibility(visibility)
            }
            WgpuBindingKind::ReadOnlyStorageBuffer => {
                storage_buffer_read_only::<[u32; 4]>(false).visibility(visibility)
            }
            WgpuBindingKind::TextureArray { dimension, count } => {
                let sample = TextureSampleType::Float { filterable: true };
                match dimension {
                    super::SamplerTextureDimension::D2 => texture_2d(sample),
                    super::SamplerTextureDimension::Cube => texture_cube(sample),
                    super::SamplerTextureDimension::D3 => texture_3d(sample),
                }
                .visibility(visibility)
                .count(texture_table::array_count(count))
            }
            WgpuBindingKind::SamplerArray { count } => sampler(SamplerBindingType::Filtering)
                .visibility(visibility)
                .count(texture_table::array_count(count)),
        };
        built.push(builder.build(u32::from(entry.binding), visibility));
    }
    BindGroupLayoutDescriptor::new(label, &built)
}

pub fn vertex_layouts_from_contract(
    layout: &super::gpu_contract::WgpuPassLayout,
) -> Vec<VertexBufferLayout> {
    let last_stream = layout
        .vertex_buffers
        .iter()
        .map(|buffer| buffer.stream)
        .max();
    (0..last_stream.map_or(0, |stream| usize::from(stream) + 1))
        .map(|stream| {
            let Some(buffer) = layout
                .vertex_buffers
                .iter()
                .find(|buffer| usize::from(buffer.stream) == stream)
            else {
                return VertexBufferLayout {
                    array_stride: 0,
                    step_mode: VertexStepMode::Vertex,
                    attributes: Vec::new(),
                };
            };
            VertexBufferLayout {
                array_stride: buffer.array_stride,
                step_mode: VertexStepMode::Vertex,
                attributes: buffer
                    .attributes
                    .iter()
                    .map(|attribute| VertexAttribute {
                        format: match attribute.format {
                            WgpuVertexFormat::Float32x2 => VertexFormat::Float32x2,
                            WgpuVertexFormat::Float16x2 => VertexFormat::Float16x2,
                            WgpuVertexFormat::Float32x3 => VertexFormat::Float32x3,
                            WgpuVertexFormat::Float32x4 => VertexFormat::Float32x4,
                            WgpuVertexFormat::Unorm8x4 => VertexFormat::Unorm8x4,
                            WgpuVertexFormat::Uint8x4 => VertexFormat::Uint8x4,
                        },
                        offset: attribute.offset,
                        shader_location: attribute.location,
                    })
                    .collect(),
            }
        })
        .collect()
}

pub fn colour_ports_static(
    extracted_generation: MaterialGenerationId,
    extracted_port_len: usize,
    cpu_generation: MaterialGenerationId,
    cpu_port_len: usize,
) -> bool {
    extracted_generation == cpu_generation
        && extracted_port_len == cpu_port_len
        && extracted_port_len > 0
}

pub fn colour_world_smodel_static(
    (gpu_world_generation, gpu_world_products): (frame::WorldGeneration, frame::WorldProducts),
    gpu_world_verts: usize,
    gpu_world_indices: usize,
    gpu_world_layer: usize,
    gpu_smodel_verts: usize,
    gpu_smodel_indices: usize,
    (cpu_world_generation, cpu_world_products): (frame::WorldGeneration, frame::WorldProducts),
    cpu_world_verts: usize,
    cpu_world_indices: usize,
    cpu_world_layer: usize,
    cpu_smodel_verts: usize,
    cpu_smodel_indices: usize,
) -> bool {
    (gpu_world_generation == cpu_world_generation || gpu_world_products.same_as(cpu_world_products))
        && gpu_world_verts == cpu_world_verts
        && gpu_world_indices == cpu_world_indices
        && gpu_world_layer == cpu_world_layer
        && gpu_smodel_verts == cpu_smodel_verts
        && gpu_smodel_indices == cpu_smodel_indices
}

fn is_viewmodel_colour_draw(kind: &RetainedDrawKind, key: u64) -> bool {
    depth_range_type_for_draw(kind, key) == GFX_DEPTH_RANGE_VIEWMODEL
}

fn viewmodel_colour_submits_when_pipelines_ready(any_viewmodel_pipeline_not_ready: bool) -> bool {
    !any_viewmodel_pipeline_not_ready
}

fn sun_shadow_view_missing(view_ready: bool, binds_sun_shadow: bool) -> bool {
    binds_sun_shadow && !view_ready
}

fn sun_shadow_content_missing(sun_recorded: bool, binds_sun_shadow: bool) -> bool {
    binds_sun_shadow && !sun_recorded
}

fn retain_camera_draws_with_sun_content(
    prepared: &mut Vec<PreparedExactDraw>,
    sun_recorded: bool,
    spot_recorded: bool,
) -> u32 {
    let mut held = 0u32;
    prepared.retain(|draw| {
        if sun_shadow_content_missing(sun_recorded, draw.binds_sun_shadow)
            || (draw.binds_spot_shadow && !spot_recorded)
        {
            held = held.saturating_add(1);
            false
        } else {
            true
        }
    });
    held
}

fn publish_this_frame_sun_shadow_view(
    products: &ExtractedRenderFrameProducts,
    uploaded: &mut RuntimeUploadedImageRegistry,
    shadowmap: &mut ShadowmapSunGpu,
    device: &RenderDevice,
) -> bool {
    let sun = products.0.product(FrameProductKind::SunShadow);
    if sun.ordered_draws.is_empty() {
        uploaded.publish_frame_target(|registry| &mut registry.sun_shadow, None);
        return false;
    }

    let (color_view, _resized) = ensure_shadowmap_sun_target(shadowmap, device);
    uploaded.publish_frame_target(|registry| &mut registry.sun_shadow, color_view);
    uploaded.sun_shadow.is_some()
}

fn spot_rt_for_light(products: &ExtractedRenderFrameProducts, light_index: u8) -> Option<u8> {
    products
        .0
        .product(FrameProductKind::SpotShadow)
        .spot_slots
        .iter()
        .find(|slot| slot.emitted.light_index == light_index)
        .map(|slot| slot.emitted.plan.render_target_id)
}

fn spot_shadow_view_missing(
    uploaded: &RuntimeUploadedImageRegistry,
    binds: bool,
    select: Option<u8>,
) -> bool {
    if !binds {
        return false;
    }
    let Some(rt) = select else {
        return true;
    };
    match rt {
        lighting_iw4::GFX_SPOT_SHADOW_RT_LARGE => uploaded.spot_shadow_rt10.is_none(),
        lighting_iw4::GFX_SPOT_SHADOW_RT_SMALL => uploaded.spot_shadow_rt11.is_none(),
        _ => true,
    }
}

fn publish_this_frame_spot_shadow_views(
    products: &ExtractedRenderFrameProducts,
    uploaded: &mut RuntimeUploadedImageRegistry,
    shadowmap: &mut ShadowmapSpotGpu,
    device: &RenderDevice,
) -> bool {
    let spot = products.0.product(FrameProductKind::SpotShadow);
    if spot.spot_slots.is_empty() {
        uploaded.publish_frame_target(|registry| &mut registry.spot_shadow_rt10, None);
        uploaded.publish_frame_target(|registry| &mut registry.spot_shadow_rt11, None);
        return false;
    }
    ensure_shadowmap_spot_targets(shadowmap, device);

    let large = shadowmap
        .color_view(lighting_iw4::GFX_SPOT_SHADOW_RT_LARGE)
        .cloned();
    let small = shadowmap
        .color_view(lighting_iw4::GFX_SPOT_SHADOW_RT_SMALL)
        .cloned();
    uploaded.publish_frame_target(|registry| &mut registry.spot_shadow_rt10, large);
    uploaded.publish_frame_target(|registry| &mut registry.spot_shadow_rt11, small);
    uploaded.spot_shadow_rt10.is_some() || uploaded.spot_shadow_rt11.is_some()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum XModelUpload {
    Resident,

    Rewrite { vertices: bool, indices: bool },

    Grow,
}

struct XModelUploadQuery {
    uploaded_vertices: u64,
    uploaded_topology: u64,
    resident_verts: usize,
    resident_indices: usize,

    vertex_fits: bool,
    index_fits: bool,
    cpu_vertices_revision: u64,
    cpu_topology_revision: u64,
    cpu_verts: usize,
    cpu_indices: usize,
}

fn colour_xmodel_upload(q: XModelUploadQuery) -> XModelUpload {
    if !q.vertex_fits || !q.index_fits {
        return XModelUpload::Grow;
    }
    let vertices =
        q.uploaded_vertices != q.cpu_vertices_revision || q.resident_verts != q.cpu_verts;
    let indices =
        q.uploaded_topology != q.cpu_topology_revision || q.resident_indices != q.cpu_indices;
    if !vertices && !indices {
        return XModelUpload::Resident;
    }
    XModelUpload::Rewrite { vertices, indices }
}

fn colour_code_mesh_upload_kind(
    gpu_revision: u64,
    gpu_verts: usize,
    gpu_indices: usize,
    gpu_has_ring: bool,
    cpu_revision: u64,
    cpu_verts: usize,
    cpu_indices: usize,
) -> i32 {
    let over =
        cpu_verts > CODE_MESH_VERT_CAP as usize || cpu_indices > CODE_MESH_INDEX_CAP as usize;
    if over {
        return 0;
    }
    if cpu_verts == 0 || cpu_indices == 0 {
        return 0;
    }
    if gpu_has_ring
        && gpu_revision == cpu_revision
        && gpu_verts == cpu_verts
        && gpu_indices == cpu_indices
    {
        0
    } else if gpu_has_ring {
        1
    } else {
        2
    }
}

fn drawsurf_object_id(key: u64) -> u16 {
    dpvs_iw4::GfxDrawSurf::from_packed(key).object_id()
}

#[derive(Resource, Default)]
pub struct FocusedOwnerSubmitState {
    emitted_frame_id: u64,
}

pub fn emit_focused_owner_submit(
    products: &ExtractedRenderFrameProducts,
    state: &mut FocusedOwnerSubmitState,
    material_context: Option<(&[String], &super::gpu_resources::RuntimeImageHandles)>,
    tables: Option<(&RuntimeMaterialCatalog, &PreparedMaterialTable)>,
    terminal: &'static str,
    prepared_surfaces: u32,
    prepared_passes: u32,
    drawn_passes: u32,
) {
    let Some(focus) = products.0.focus() else {
        return;
    };
    if state.emitted_frame_id == products.0.frame_id {
        return;
    }
    let mut product_surfaces = 0u32;
    let mut execution_ready_surfaces = 0u32;
    let mut materials = BTreeSet::new();
    let mut material_textures = BTreeSet::new();
    if let Some(object_id) = focus.object_id {
        for product in [
            products.0.product(FrameProductKind::Colour),
            products.0.product(FrameProductKind::Light),
            products.0.product(FrameProductKind::Emissive),
        ] {
            for (index, item) in product.ordered_draws.iter().enumerate() {
                if drawsurf_object_id(item.key) != object_id {
                    continue;
                }
                product_surfaces = product_surfaces.saturating_add(1);
                let resolved = tables.and_then(|(catalog, prepared)| {
                    let tech = product.draw_tech.get(index).copied()?;
                    prepared_draw_technique(
                        catalog,
                        prepared,
                        render_material::MaterialDrawKey::new(item.key, item.material_rank)
                            .with_material_id(item.material_id),
                        tech,
                    )
                    .ok()
                });
                execution_ready_surfaces =
                    execution_ready_surfaces.saturating_add(u32::from(resolved.is_some()));
                let ordinal = world_material_sorted(item.key);
                let material = material_context
                    .and_then(|(names, _)| names.get(usize::from(ordinal)))
                    .map_or_else(|| ordinal.to_string(), |name| format!("{ordinal}:{name}"));
                materials.insert(material);
                if let Some((_, prepared_tech)) = resolved {
                    for (pass_index, pass) in prepared_tech.passes.iter().enumerate() {
                        let Some(local) = pass.local_samplers.as_ref() else {
                            continue;
                        };
                        for lane in &local.lanes {
                            let image_index = lane.texture.image.0 as usize;
                            let image = material_context.map_or_else(String::new, |(_, images)| {
                                let image_name = images
                                    .material_names
                                    .get(image_index)
                                    .map(String::as_str)
                                    .unwrap_or("<out-of-range>");
                                let retained = images
                                    .material_images
                                    .get(image_index)
                                    .is_some_and(Option::is_some);
                                format!(",image_name={image_name},retained={}", u8::from(retained))
                            });
                            material_textures.insert(format!(
                                "material={ordinal},pass={pass_index},register={},name_hash=0x{:08x},image={},semantic={},sampler=0x{:02x}{image}",
                                lane.register,
                                lane.name_hash,
                                lane.texture.image.0,
                                lane.texture.semantic,
                                lane.texture.sampler_state,
                            ));
                        }
                    }
                }
            }
        }
    }
    let materials =
        (!materials.is_empty()).then(|| materials.into_iter().collect::<Vec<_>>().join(";"));
    let material_textures = (!material_textures.is_empty())
        .then(|| material_textures.into_iter().collect::<Vec<_>>().join(";"));
    let outcome = if focus.outcome == "planned_empty" {
        "planned_empty"
    } else if focus.outcome != "planned" {
        "plan_refused"
    } else if terminal != "complete" {
        terminal
    } else if product_surfaces == 0 {
        "product_missing"
    } else if execution_ready_surfaces == 0 {
        "material_refused"
    } else if execution_ready_surfaces < product_surfaces {
        "material_partial"
    } else if prepared_surfaces == 0 {
        "gpu_prepare_refused"
    } else if prepared_surfaces < execution_ready_surfaces {
        "gpu_prepare_partial"
    } else if drawn_passes == 0 {
        "draw_refused"
    } else if drawn_passes < prepared_passes {
        "draw_partial"
    } else {
        "drawn"
    };
    perf::render_owner_submit(
        products.0.frame_id,
        "script_model",
        focus.owner_id,
        focus.object_id,
        outcome,
        materials.as_deref(),
        material_textures.as_deref(),
        product_surfaces,
        execution_ready_surfaces,
        prepared_surfaces,
        prepared_passes,
        drawn_passes,
    );
    state.emitted_frame_id = products.0.frame_id;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct UnsupportedStateCensus {
    unknown_blend_factor: u32,
    unknown_blend_operation: u32,
    stencil: u32,
}

impl UnsupportedStateCensus {
    fn absorb(&mut self, other: Self) {
        self.unknown_blend_factor = self
            .unknown_blend_factor
            .saturating_add(other.unknown_blend_factor);
        self.unknown_blend_operation = self
            .unknown_blend_operation
            .saturating_add(other.unknown_blend_operation);
        self.stencil = self.stencil.saturating_add(other.stencil);
    }

    fn note(&mut self, fields: super::state::UnsupportedStateFields) {
        self.unknown_blend_factor = self
            .unknown_blend_factor
            .saturating_add(u32::from(fields.unknown_blend_factor));
        self.unknown_blend_operation = self
            .unknown_blend_operation
            .saturating_add(u32::from(fields.unknown_blend_operation));
        self.stencil = self.stencil.saturating_add(u32::from(fields.stencil));
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct AuthoredStateCensus {
    non_add_blend: u32,
    independent_alpha_blend: u32,
    partial_colour_write: u32,
    line_fill: u32,
    stencil: u32,
}

impl AuthoredStateCensus {
    fn note(&mut self, fields: super::state::AuthoredStateFields) {
        self.non_add_blend = self
            .non_add_blend
            .saturating_add(u32::from(fields.non_add_blend));
        self.independent_alpha_blend = self
            .independent_alpha_blend
            .saturating_add(u32::from(fields.independent_alpha_blend));
        self.partial_colour_write = self
            .partial_colour_write
            .saturating_add(u32::from(fields.partial_colour_write));
        self.line_fill = self.line_fill.saturating_add(u32::from(fields.line_fill));
        self.stencil = self.stencil.saturating_add(u32::from(fields.stencil));
    }
}

fn as_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn world_pretess_dest_ib(
    device: &RenderDevice,
    queue: &RenderQueue,
    reuse: Option<Buffer>,
    indices: &[u32],
) -> Option<Buffer> {
    if indices.is_empty() {
        return None;
    }
    let bytes: &[u8] = bytemuck::cast_slice(indices);
    let need = padded_upload_len(bytes.len()) as u64;
    let buf = match reuse {
        Some(existing) if existing.size() >= need => existing,
        _ => device.create_buffer(&BufferDescriptor {
            label: Some("iw4_world_pretess_dest_ib"),
            size: need,
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }),
    };
    write_buffer_padded(queue, &buf, bytes);
    Some(buf)
}

fn smodel_pretess_submit_refusal(
    extracted: ExtractedColourRefs<'_>,
    placement: u32,
    cache_index: u16,
    range: SmodelPretessRange,
) -> Result<(ExactTessBind, u32, u32, u8), GpuSubmitRefusal> {
    if cache_index == 0 {
        return Err(GpuSubmitRefusal::SmodelCacheIndexEmpty { placement });
    }
    if !extracted.world.smc_index_baked.contains(&cache_index) {
        return Err(GpuSubmitRefusal::SmodelCacheIndicesMissing { cache_index });
    }
    let end = range
        .start
        .checked_add(range.count)
        .and_then(|end| usize::try_from(end).ok());
    if range.count == 0
        || range.count % 3 != 0
        || end.is_none_or(|end| end > extracted.world.smodel_pretess_indices.len())
    {
        return Err(GpuSubmitRefusal::SmodelCacheIndicesMissing { cache_index });
    }
    Ok((
        ExactTessBind::SmodelCached,
        range.start,
        range.count,
        asset_iw4::vertex_decl::STATICMODELCACHE_VERTEX_TYPE,
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExactTessBind {
    World,
    Smodel,
    SmodelCached,
    SmodelSkinned,
    XModel,
    CodeMesh,
    ParticleCloud,
    MarkMesh,
    Glass,
}

#[derive(Clone)]
struct PreparedExactDraw {
    pipeline: ExactPipelineSlot,
    port: PortId,

    constants: Option<Arc<PassConstantBuffers>>,

    constant_base: Option<u32>,

    texture_slots: Arc<[u32]>,
    start: u32,
    count: u32,
    tess: ExactTessBind,

    after_scene_resolve: bool,

    owner_object_id: Option<u16>,

    depth_min: f32,
    depth_max: f32,

    ring_epoch: u32,

    smc_stream_off: u64,

    state: super::state::GfxPassState,

    bsp_kind: Option<BspCameraLane>,
    bsp_run_first: u16,
    bsp_first_surf: u16,
    bsp_surf_count: u16,

    bsp_counted: bool,

    binds_sun_shadow: bool,

    binds_spot_shadow: bool,

    indirect_arg: Option<u32>,
    arena_lane: u8,

    /// The loaded map only stands in for a Minecraft world: this draw of
    /// its world is prepared but not submitted.
    minecraft_hidden: bool,
}

fn bsp_draw_source(kind: &RetainedDrawKind) -> (Option<BspCameraLane>, u16, u16, u16) {
    match *kind {
        RetainedDrawKind::World {
            surf,
            run,
            bsp_kind: Some(kind),
            bsp_run_first: Some(run_first),
            ..
        } => (Some(kind), run_first, surf, run.max(1)),
        RetainedDrawKind::World {
            bsp_kind: Some(_),
            bsp_run_first: None,
            ..
        } => panic!("BSP world draw is missing its producer run identity"),
        _ => (None, 0, 0, 0),
    }
}

const fn bsp_kind_index(kind: BspCameraLane) -> usize {
    match kind {
        BspCameraLane::LitOpaque => 0,
        BspCameraLane::LitTrans => 1,
        BspCameraLane::Emissive => 2,
        BspCameraLane::Decal => 3,
    }
}

fn submit_refusal_family(kind: &RetainedDrawKind, viewmodel: bool) -> &'static str {
    match kind {
        RetainedDrawKind::World { .. } => "world",
        RetainedDrawKind::Smodel { .. } => "smodel",
        RetainedDrawKind::XModel { .. } if viewmodel => "xmodel/fpv",
        RetainedDrawKind::XModel { .. } => "xmodel",
        RetainedDrawKind::CodeMesh { .. } => "codemesh",
        RetainedDrawKind::ParticleCloud { .. } => "particlecloud",
        RetainedDrawKind::MarkMesh { .. } => "markmesh",
        RetainedDrawKind::Glass { .. } => "glassmesh",
    }
}

fn submit_refusal_class(cause: &GpuSubmitRefusal) -> &'static str {
    match cause {
        GpuSubmitRefusal::NoExactPort { .. } => "NoExactPort",
        GpuSubmitRefusal::ConstantPack(_) => "ConstantPack",
        GpuSubmitRefusal::TextureBind(_) => "TextureBind",
        GpuSubmitRefusal::TextureTable(_) => "TextureTable",
        GpuSubmitRefusal::ProductDependencyNotReady { .. } => "ProductDependencyNotReady",
        GpuSubmitRefusal::UnsupportedTessKind => "UnsupportedTessKind",
        GpuSubmitRefusal::MultipleCameraViews { .. } => "MultipleCameraViews",
        GpuSubmitRefusal::WorldVertex(_) => "WorldVertex",
        GpuSubmitRefusal::PackedVertex(_) => "PackedVertex",
        GpuSubmitRefusal::MissingSurfaceRange { .. } => "MissingSurfaceRange",
        GpuSubmitRefusal::MissingSmodelRange { .. } => "MissingSmodelRange",
        GpuSubmitRefusal::MissingXModelRange { .. } => "MissingXModelRange",
        GpuSubmitRefusal::MissingCodeMeshRange { .. } => "MissingCodeMeshRange",
        GpuSubmitRefusal::MissingParticleCloudRange { .. } => "MissingParticleCloudRange",
        GpuSubmitRefusal::MissingMarkMeshRange { .. } => "MissingMarkMeshRange",
        GpuSubmitRefusal::MissingGlassMeshRange { .. } => "MissingGlassMeshRange",
        GpuSubmitRefusal::EmptyIndexRange { .. } => "EmptyIndexRange",
        GpuSubmitRefusal::EmptySmodelIndexRange { .. } => "EmptySmodelIndexRange",
        GpuSubmitRefusal::EmptyXModelIndexRange { .. } => "EmptyXModelIndexRange",
        GpuSubmitRefusal::EmptyCodeMeshIndexRange { .. } => "EmptyCodeMeshIndexRange",
        GpuSubmitRefusal::EmptyParticleCloudIndexRange { .. } => "EmptyParticleCloudIndexRange",
        GpuSubmitRefusal::EmptyMarkMeshIndexRange { .. } => "EmptyMarkMeshIndexRange",
        GpuSubmitRefusal::EmptyGlassMeshIndexRange { .. } => "EmptyGlassMeshIndexRange",
        GpuSubmitRefusal::PipelineNotReady => "PipelineNotReady",
        GpuSubmitRefusal::SmodelCacheIndexEmpty { .. } => "SmodelCacheIndexEmpty",
        GpuSubmitRefusal::SmodelCacheIndicesMissing { .. } => "SmodelCacheIndicesMissing",
        GpuSubmitRefusal::SmodelXSurfacePathUnread { .. } => "SmodelXSurfacePathUnread",
        GpuSubmitRefusal::SmodelCachedWithoutDestRange { .. } => "SmodelCachedWithoutDestRange",
        GpuSubmitRefusal::SmodelSkinnedDestMissing { .. } => "SmodelSkinnedDestMissing",
        GpuSubmitRefusal::ConstantArenaMissing => "ConstantArenaMissing",
        GpuSubmitRefusal::WorldPretessSpanBeyondLimit { .. } => "WorldPretessSpanBeyondLimit",
        GpuSubmitRefusal::WorldPretessEpochMismatch { .. } => "WorldPretessEpochMismatch",
    }
}

fn exec_refusal_row(
    kind: &RetainedDrawKind,
    key: u64,
    cause: &MaterialRefusal,
) -> (&'static str, &'static str) {
    let viewmodel = is_viewmodel_colour_draw(kind, key);
    (
        submit_refusal_family(kind, viewmodel),
        material_refusal_class(cause),
    )
}

struct DrawRefusalCensus {
    on: bool,
    submit: BTreeMap<(&'static str, &'static str), u32>,
    exec: BTreeMap<(&'static str, &'static str), u32>,
}

impl DrawRefusalCensus {
    fn new(on: bool) -> Self {
        Self {
            on,
            submit: BTreeMap::new(),
            exec: BTreeMap::new(),
        }
    }

    fn taking(
        on: bool,
        submit: BTreeMap<(&'static str, &'static str), u32>,
        exec: BTreeMap<(&'static str, &'static str), u32>,
    ) -> Self {
        Self { on, submit, exec }
    }

    fn note_exec(&mut self, kind: &RetainedDrawKind, key: u64, cause: &MaterialRefusal) {
        if !self.on {
            return;
        }
        *self
            .exec
            .entry(exec_refusal_row(kind, key, cause))
            .or_default() += 1;
    }

    fn note_submit(&mut self, kind: &RetainedDrawKind, viewmodel: bool, cause: &GpuSubmitRefusal) {
        if !self.on {
            return;
        }
        *self
            .submit
            .entry((
                submit_refusal_family(kind, viewmodel),
                submit_refusal_class(cause),
            ))
            .or_default() += 1;
    }

    fn note_submit_class(&mut self, family: &'static str, class: &'static str, n: u32) {
        if !self.on || n == 0 {
            return;
        }
        *self.submit.entry((family, class)).or_default() += n;
    }
}

fn material_refusal_class(cause: &MaterialRefusal) -> &'static str {
    match cause {
        MaterialRefusal::SortedMaterialTableMissing { .. } => "SortedMaterialTableMissing",
        MaterialRefusal::SortedMaterialOrdinalOutOfRange { .. } => {
            "SortedMaterialOrdinalOutOfRange"
        }
        MaterialRefusal::SortedMaterialBuildFailed { .. } => "SortedMaterialBuildFailed",
        MaterialRefusal::StaleMaterialGeneration { .. } => "StaleMaterialGeneration",
        MaterialRefusal::MissingLightBindings { .. } => "MissingLightBindings",
        MaterialRefusal::MaterialOutOfRange { .. } => "MaterialOutOfRange",
        MaterialRefusal::LocalTechniqueSetOutOfRange { .. } => "LocalTechniqueSetOutOfRange",
        MaterialRefusal::RemappedTechniqueSetOutOfRange { .. } => "RemappedTechniqueSetOutOfRange",
        MaterialRefusal::RemapMissing { .. } => "RemapMissing",
        MaterialRefusal::RemapCycle { .. } => "RemapCycle",
        MaterialRefusal::TechniqueAbsent { .. } => "TechniqueAbsent",
        MaterialRefusal::EmptyTechnique { .. } => "EmptyTechnique",
        MaterialRefusal::StateEntriesMissing => "StateEntriesMissing",
        MaterialRefusal::StateEntryOutOfRange { .. } => "StateEntryOutOfRange",
        MaterialRefusal::StateRowOutOfRange { .. } => "StateRowOutOfRange",
        MaterialRefusal::UnsupportedState { .. } => "UnsupportedState",
        MaterialRefusal::PassIndexOverflow { .. } => "PassIndexOverflow",
        MaterialRefusal::ShaderProgramMissing { .. } => "ShaderProgramMissing",
        MaterialRefusal::UnsupportedShaderPair { .. } => "UnsupportedShaderPair",
        MaterialRefusal::MissingTexture { .. } => "MissingTexture",
        MaterialRefusal::MissingMaterialConstant { .. } => "MissingMaterialConstant",
        MaterialRefusal::MissingCodeConstant { .. } => "MissingCodeConstant",
        MaterialRefusal::CodeConstantRowsOutOfRange { .. } => "CodeConstantRowsOutOfRange",
        MaterialRefusal::MissingCodeTexture { .. } => "MissingCodeTexture",
        MaterialRefusal::MissingLiteralConstant { .. } => "MissingLiteralConstant",
        MaterialRefusal::UnknownArgumentType { .. } => "UnknownArgumentType",
        MaterialRefusal::PassArgCountMismatch { .. } => "PassArgCountMismatch",
        MaterialRefusal::TechniqueSetNamespaceMismatch { .. } => "TechniqueSetNamespaceMismatch",
    }
}

pub fn dump_sorted_material_names(catalog: &RuntimeMaterialCatalog) -> Vec<String> {
    match &catalog.parts().sorted_materials {
        RuntimeSortedMaterialTable::Ready {
            asset_ids_by_ordinal,
            ..
        } => asset_ids_by_ordinal
            .iter()
            .map(|id| {
                catalog
                    .parts()
                    .materials
                    .get(usize::from(id.0))
                    .map(|material| material.name.clone())
                    .unwrap_or_else(|| format!("id{}", id.0))
            })
            .collect(),
        RuntimeSortedMaterialTable::Missing { .. } | RuntimeSortedMaterialTable::BuildFailed(_) => {
            Vec::new()
        }
    }
}

pub fn dump_shader_program_names(catalog: &RuntimeMaterialCatalog) -> Vec<Option<String>> {
    catalog
        .parts()
        .shader_programs
        .iter()
        .map(|slot| slot.as_ref().map(|program| program.name.clone()))
        .collect()
}

fn rank_count_map(map: &BTreeMap<String, u32>, take: usize) -> Option<String> {
    if map.is_empty() {
        return None;
    }
    let mut ranked: Vec<(&String, &u32)> = map.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    Some(
        ranked
            .into_iter()
            .take(take)
            .map(|(name, n)| format!("{name}:{n}"))
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn rank_pair_map(map: &BTreeMap<(&'static str, &'static str), u32>, take: usize) -> Option<String> {
    if map.is_empty() {
        return None;
    }
    let mut ranked: Vec<_> = map.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    Some(
        ranked
            .into_iter()
            .take(take)
            .map(|((family, cause), n)| format!("{family}:{cause}:{n}"))
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn record_pipeline_not_ready(
    kind: &RetainedDrawKind,
    viewmodel: bool,
    key: u64,
    execution: &MaterialExecution,
    extracted: ExtractedColourRefs<'_>,
    smodel_mats: &mut BTreeMap<String, u32>,
    world_mats: &mut BTreeMap<String, u32>,
    smodel_ps: &mut BTreeMap<String, u32>,
    world_ps: &mut BTreeMap<String, u32>,
    smodel_keys: &mut HashSet<u64>,
    world_keys: &mut HashSet<u64>,
    ports: &mut HashSet<PortId>,
) {
    let family = submit_refusal_family(kind, viewmodel);
    let ordinal = world_material_sorted(key);
    let name = extracted
        .world
        .sorted_material_names
        .get(usize::from(ordinal))
        .cloned()
        .unwrap_or_else(|| format!("ord{ordinal}"));
    let ps = execution
        .pass(0)
        .map(|pass| {
            extracted
                .world
                .shader_program_names
                .get(pass.shader_pair.pixel.asset_slot as usize)
                .and_then(|slot| slot.clone())
                .unwrap_or_else(|| format!("{:016x}", pass.port.pixel_program_hash))
        })
        .unwrap_or_else(|| "<no pass>".to_owned());
    for pass in execution.iter_passes() {
        ports.insert(pass.port);
    }
    match family {
        "smodel" => {
            *smodel_mats.entry(name).or_default() += 1;
            *smodel_ps.entry(ps).or_default() += 1;
            smodel_keys.insert(key);
        }
        "world" => {
            *world_mats.entry(name).or_default() += 1;
            *world_ps.entry(ps).or_default() += 1;
            world_keys.insert(key);
        }
        _ => {}
    }
}

fn execution_binds_code_texture(execution: &MaterialExecution, index: u32) -> bool {
    execution
        .iter_passes()
        .any(|pass| pass.code_samplers.iter().any(|lane| lane.index == index))
}

fn identity_placement_kind(kind: &RetainedDrawKind) -> bool {
    match *kind {
        RetainedDrawKind::World { .. }
        | RetainedDrawKind::CodeMesh { .. }
        | RetainedDrawKind::MarkMesh { .. }
        | RetainedDrawKind::Glass { .. } => true,
        RetainedDrawKind::Smodel {
            world_from_local, ..
        }
        | RetainedDrawKind::XModel {
            world_from_local, ..
        } => world_from_local == Mat4::IDENTITY,
        _ => false,
    }
}

#[derive(Default)]
struct RunPackCache {
    serial: u64,
    passes: Vec<Option<Arc<PassConstantBuffers>>>,

    pipelines: Vec<Option<(usize, ExactPipelineSlot)>>,
    intern: HashMap<PackedBankKey, (Arc<PassConstantBuffers>, u32)>,
    intern_stamp: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PackedBankKey {
    port: PortId,
    local: usize,
    code: u64,
}

fn packed_bank_key(
    port: PortId,
    local: Option<&Arc<render_material::PackedLocalBanks>>,
    code: u64,
) -> PackedBankKey {
    PackedBankKey {
        port,
        local: local.map(|banks| Arc::as_ptr(banks) as usize).unwrap_or(0),
        code,
    }
}

fn intern_packed_bank<E>(
    intern: &mut HashMap<PackedBankKey, (Arc<PassConstantBuffers>, u32)>,
    stamp: u32,
    key: PackedBankKey,
    miss: impl FnOnce() -> Result<Arc<PassConstantBuffers>, E>,
) -> Result<(Arc<PassConstantBuffers>, bool), E> {
    if let Some((existing, seen)) = intern.get_mut(&key) {
        *seen = stamp;
        return Ok((Arc::clone(existing), true));
    }
    let packed = miss()?;
    intern.insert(key, (Arc::clone(&packed), stamp));
    Ok((packed, false))
}

impl RunPackCache {
    fn begin(&mut self, serial: u64, pass_len: usize) {
        if self.serial != serial {
            self.serial = serial;
            self.passes.clear();
            self.pipelines.clear();
        }
        if self.passes.len() < pass_len {
            self.passes.resize(pass_len, None);
            self.pipelines.resize(pass_len, None);
        }
    }

    fn begin_pack_intern_frame(&mut self) {
        self.intern_stamp = self.intern_stamp.wrapping_add(1);
    }

    fn sweep_pack_intern(&mut self) {
        self.intern
            .retain(|_, (_, seen)| *seen == self.intern_stamp);
    }

    fn pack(
        &mut self,
        pass_index: usize,
        port: &super::AdmittedExactPort,
        executable: ExecutablePassView<'_>,
        prepare_cost: &mut PrepareCost,
    ) -> Result<Arc<PassConstantBuffers>, ConstantPackRefusal> {
        if let Some(Some(existing)) = self.passes.get(pass_index) {
            prepare_cost.note_intern_hit();
            return Ok(Arc::clone(existing));
        }
        let key = packed_bank_key(
            executable.port,
            executable.local_banks,
            executable.code_constant_id,
        );
        let (packed, hit) = intern_packed_bank(&mut self.intern, self.intern_stamp, key, || {
            port.pack_hit(executable).map(Arc::new)
        })?;
        if hit {
            prepare_cost.note_intern_hit();
        } else {
            prepare_cost.note_intern_miss();
            prepare_cost.note_pack_seed(executable.local_banks.is_some());
        }
        if let Some(slot) = self.passes.get_mut(pass_index) {
            *slot = Some(Arc::clone(&packed));
        }
        Ok(packed)
    }
}

impl ExactPrepare<'_> {
    fn bind_hit_textures(
        &mut self,
        executable: ExecutablePassView<'_>,
        surface: super::SurfaceSamplerInputs,
        after_scene_resolve: bool,
    ) -> Result<Arc<[u32]>, GpuSubmitRefusal> {
        let port_gpu = self
            .pipeline_res
            .get(executable.port)
            .expect("exact port was admitted before texture binding");
        let key = BoundTextureKey {
            identity: TextureBindIdentity {
                port: executable.port,
                local: executable
                    .local_samplers
                    .map(|packed| packed.id)
                    .unwrap_or(0),
                code: executable.code_sampler_id,
                surface,
            },
            spot_shadow_select: self.spot_shadow_select,
        };
        let scene_index = texture_table::scene_table_index(
            after_scene_resolve,
            GfxPassState::from_prepared(executable.state).srgb_write_enable(),
        );
        let extracted = self.extracted;
        let uploaded = self.uploaded;
        let sampler_table = self.sampler_table;
        let spot_shadow_select = self.spot_shadow_select;
        let device = self.device;
        let resolve = |table: &mut ExactTextureTable| -> Result<Arc<[u32]>, GpuSubmitRefusal> {
            let textures = port_gpu
                .port
                .resolve_uploaded_texture_binds(
                    executable,
                    surface,
                    &extracted.world.image_handles,
                    extracted.world.generation,
                    uploaded,
                    sampler_table,
                    spot_shadow_select,
                )
                .map_err(GpuSubmitRefusal::TextureBind)?;
            texture_slot_words(device, table, &textures)
        };
        let mut shared_guard;
        let (texture_slots, table) = match &mut self.textures {
            PrepareTextureTables::SceneShared(shared) => {
                shared_guard = shared
                    .lock()
                    .expect("scene texture tables are never poisoned");
                let refs = &mut *shared_guard;
                (&mut refs.slots[scene_index], &mut refs.tables[scene_index])
            }
            PrepareTextureTables::Shadow { slots, table } => (&mut **slots, &mut **table),
        };
        if let Some(slots) = texture_slots.get(&key) {
            self.cost.tex_bind_hit_n = self.cost.tex_bind_hit_n.saturating_add(1);
            return Ok(Arc::clone(slots));
        }
        self.cost.tex_bind_miss_n = self.cost.tex_bind_miss_n.saturating_add(1);
        let slots = resolve(table)?;
        texture_slots.insert(key, Arc::clone(&slots));
        Ok(slots)
    }
}

fn texture_slot_words(
    device: &RenderDevice,
    table: &mut ExactTextureTable,
    textures: &[UploadedTextureBind],
) -> Result<Arc<[u32]>, GpuSubmitRefusal> {
    textures
        .iter()
        .map(|lane| {
            table
                .slot_word(device, lane)
                .map_err(GpuSubmitRefusal::TextureTable)
        })
        .collect::<Result<Vec<u32>, _>>()
        .map(Arc::from)
}

#[derive(Default)]
struct PrepareCost {
    pack_seed_n: u32,
    pack_walk_n: u32,
    tex_bind_hit_n: u32,
    tex_bind_miss_n: u32,
    intern_hit_n: u32,
    intern_miss_n: u32,
}

#[derive(Default)]
struct SceneTextureState {
    slots: [HashMap<BoundTextureKey, Arc<[u32]>>; 4],
    tables: [ExactTextureTable; 4],
}

enum PrepareTextureTables<'a> {
    SceneShared(&'a std::sync::Mutex<SceneTextureState>),
    Shadow {
        slots: &'a mut HashMap<BoundTextureKey, Arc<[u32]>>,
        table: &'a mut ExactTextureTable,
    },
}

struct ExactPrepare<'a> {
    extracted: ExtractedColourRefs<'a>,
    geometry: &'a ExactColourGeometry,

    pretess: Option<&'a CameraWorldPretess>,
    pipeline_res: &'a ExactColourPipeline,
    registry: &'a ExactPipelineRegistry,
    device: &'a RenderDevice,
    uploaded: &'a RuntimeUploadedImageRegistry,

    spot_shadow_select: Option<u8>,
    sampler_table: &'a SamplerTable,

    textures: PrepareTextureTables<'a>,

    arena: Option<&'a mut ArenaPack>,
    run_pack: RunPackCache,
    cost: PrepareCost,
    skinned_tess: Option<&'a mut smodel_skinned::SmodelSkinnedTess>,
    skinned_shared: Option<&'a std::sync::Mutex<smodel_skinned::SmodelSkinnedTess>>,
}

#[derive(Clone, Copy)]
struct ExactPrepareTarget {
    color: TextureFormat,
    samples: u32,
    depth: TextureFormat,
    forward_z: bool,
    use_world_pretess: bool,
}

#[derive(Clone, Copy)]
struct SunFlush {
    kind: SunFlushKind,
    draw_start: u32,
    draw_count: u32,
    ring_epoch: u32,
}

impl PrepareCost {
    fn note_pack_seed(&mut self, seeded: bool) {
        if seeded {
            self.pack_seed_n = self.pack_seed_n.saturating_add(1);
        } else {
            self.pack_walk_n = self.pack_walk_n.saturating_add(1);
        }
    }
    fn absorb(&mut self, other: Self) {
        self.pack_seed_n = self.pack_seed_n.saturating_add(other.pack_seed_n);
        self.pack_walk_n = self.pack_walk_n.saturating_add(other.pack_walk_n);
        self.tex_bind_hit_n = self.tex_bind_hit_n.saturating_add(other.tex_bind_hit_n);
        self.tex_bind_miss_n = self.tex_bind_miss_n.saturating_add(other.tex_bind_miss_n);
        self.intern_hit_n = self.intern_hit_n.saturating_add(other.intern_hit_n);
        self.intern_miss_n = self.intern_miss_n.saturating_add(other.intern_miss_n);
    }

    fn note_intern_hit(&mut self) {
        self.intern_hit_n = self.intern_hit_n.saturating_add(1);
    }
    fn note_intern_miss(&mut self) {
        self.intern_miss_n = self.intern_miss_n.saturating_add(1);
    }
}

#[derive(Default)]
struct WorldRunGather {
    indices: Vec<u32>,
    ranges: Vec<(u32, u32)>,

    index_gaps: u32,
}

fn empty_world_run_gather(ranges: Vec<(u32, u32)>, index_gaps: u32) -> WorldRunGather {
    WorldRunGather {
        indices: Vec::new(),
        ranges,
        index_gaps,
    }
}

fn world_submit_ranges_for_pass<'a>(
    geometry: &'a ExactColourGeometry,
    pretess: Option<&'a CameraWorldPretess>,
    use_world_pretess: bool,
) -> &'a [(u32, u32)] {
    match pretess.filter(|_| use_world_pretess) {
        Some(pretess) => pretess.ranges(),
        None => &geometry.world_surface_ranges,
    }
}

fn gather_world_run_indices(
    hits: impl IntoIterator<Item = (u16, u64, super::SurfaceSamplerInputs)>,
    src_indices: &[u32],
    src_ranges: &[(u32, u32)],
) -> WorldRunGather {
    let mut ranges = vec![(0, 0); src_ranges.len()];
    let mut indices = Vec::new();
    let mut prev: Option<(u64, super::SurfaceSamplerInputs, u32)> = None;
    let mut index_gaps = 0u32;
    for (surf, key, samplers) in hits {
        let Some(&(start, count)) = src_ranges.get(usize::from(surf)) else {
            prev = None;
            continue;
        };
        if count == 0 {
            prev = None;
            continue;
        }
        let dest = indices.len() as u32;
        let Some(slice) = src_indices.get(start as usize..(start.saturating_add(count)) as usize)
        else {
            prev = None;
            continue;
        };
        if let Some((prev_key, prev_samplers, end)) = prev
            && prev_key == key
            && prev_samplers == samplers
            && end != start
        {
            index_gaps = index_gaps.saturating_add(1);
        }
        indices.extend_from_slice(slice);
        if let Some(slot) = ranges.get_mut(usize::from(surf)) {
            *slot = (dest, count);
        }
        prev = Some((key, samplers, start.saturating_add(count)));
    }
    WorldRunGather {
        indices,
        ranges,
        index_gaps,
    }
}

fn colour_tech_at(
    colour: &FrameProduct,
    light: &FrameProduct,
    emissive: &FrameProduct,
    di: usize,
) -> Option<TechType> {
    let colour_n = colour.ordered_draws.len();
    let light_n = light.ordered_draws.len();
    if di < colour_n {
        colour.draw_tech.get(di).copied()
    } else if di < colour_n + light_n {
        light.draw_tech.get(di - colour_n).copied()
    } else {
        emissive.draw_tech.get(di - colour_n - light_n).copied()
    }
}

fn colour_draw_at<'a>(
    colour: &'a FrameProduct,
    light: &'a FrameProduct,
    emissive: &'a FrameProduct,
    di: usize,
) -> Option<&'a RetainedDrawItem> {
    let colour_n = colour.ordered_draws.len();
    let light_n = light.ordered_draws.len();
    if di < colour_n {
        colour.ordered_draws.get(di)
    } else if di < colour_n + light_n {
        light.ordered_draws.get(di - colour_n)
    } else {
        emissive.ordered_draws.get(di - colour_n - light_n)
    }
}

fn packed_draw_index(packed: &render_frame::PackedFrontendLists, emit: PackedEmit) -> Option<u32> {
    let i = emit.entry as usize;
    match emit.list {
        PackedListKind::World => packed.world_draw_indices.get(i).copied(),
        PackedListKind::XModel => packed.xmodel_draw_indices.get(i).copied(),
        PackedListKind::Smodel => packed.smodel_draw_indices.get(i).copied(),
        PackedListKind::Cached => packed.smodel_cached_draw_indices.get(i).copied(),
        PackedListKind::Pretess => packed.smodel_pretess_draw_indices.get(i).copied(),
        PackedListKind::SmodelSkinned => packed.smodel_skinned_draw_indices.get(i).copied(),
    }
}

struct WorldPackedSurf<'a> {
    item: &'a RetainedDrawItem,
    world_surf: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WorldPackedRowMeta {
    di: u32,
    world_surf: Option<u16>,
}

impl WorldPackedSurf<'_> {
    fn world_surf(&self) -> Option<u16> {
        self.world_surf.or_else(|| match self.item.kind {
            RetainedDrawKind::World { surf, .. } => Some(surf),
            _ => None,
        })
    }
}

fn world_packed_row_meta(
    colour: &FrameProduct,
    light: &FrameProduct,
    emissive: &FrameProduct,
    packed: &render_frame::PackedFrontendLists,
    table: &[u16],
) -> Vec<Option<WorldPackedRowMeta>> {
    let skip_end = colour
        .ordered_draws
        .len()
        .saturating_add(light.ordered_draws.len())
        .saturating_add(emissive.ordered_draws.len());
    let mut prior = vec![0u16; skip_end];
    let mut rows = Vec::with_capacity(packed.world_draw_indices.len());
    for &di in &packed.world_draw_indices {
        let di_us = di as usize;
        let Some(item) = colour_draw_at(colour, light, emissive, di_us) else {
            rows.push(None);
            continue;
        };
        let seen = prior.get(di_us).copied().unwrap_or(0);
        let world_surf = match item.kind {
            RetainedDrawKind::World {
                surf, run, run_off, ..
            } if run > 1 => Some(
                table
                    .get(run_off as usize + usize::from(seen))
                    .copied()
                    .unwrap_or(surf),
            ),
            _ => None,
        };
        if let Some(slot) = prior.get_mut(di_us) {
            *slot = seen.saturating_add(1);
        }
        rows.push(Some(WorldPackedRowMeta { di, world_surf }));
    }
    rows
}

fn bind_world_packed_rows<'a>(
    colour: &'a FrameProduct,
    light: &'a FrameProduct,
    emissive: &'a FrameProduct,
    rows: &'a [Option<WorldPackedRowMeta>],
) -> impl Iterator<Item = Option<WorldPackedSurf<'a>>> + 'a {
    rows.iter().map(move |meta| {
        let meta = meta.as_ref()?;
        colour_draw_at(colour, light, emissive, meta.di as usize).map(|item| WorldPackedSurf {
            item,
            world_surf: meta.world_surf,
        })
    })
}

struct PreparedColourRow<'a> {
    item: &'a RetainedDrawItem,
    world_surf: Option<u16>,
}

impl PreparedColourRow<'_> {
    fn expanded_item(&self) -> Option<RetainedDrawItem> {
        let surf = self.world_surf?;
        let RetainedDrawKind::World {
            surf: first_surf,
            world_from_local,
            bsp_kind,
            bsp_run_first,
            setup_key_changed,
            ..
        } = self.item.kind
        else {
            return None;
        };
        let mut item = *self.item;
        item.kind = match bsp_kind {
            Some(kind) => RetainedDrawKind::World {
                surf,
                run: 1,
                run_off: 0,
                bsp_kind: Some(kind),
                bsp_run_first: Some(
                    bsp_run_first.expect("BSP world draw is missing its producer run identity"),
                ),
                setup_key_changed: setup_key_changed && surf == first_surf,
                world_from_local: Mat4::IDENTITY,
            },
            None => RetainedDrawKind::world_with_pose(surf, world_from_local),
        };
        Some(item)
    }
}

fn colour_entry_index_span(
    packed: &render_frame::PackedFrontendLists,
    emit: PackedEmit,
) -> Option<(u32, u32)> {
    let index = emit.entry as usize;
    if emit.list == PackedListKind::XModel {
        return packed
            .xmodel
            .get(index)
            .map(|entry| (entry.index_byte_offset / 2, u32::from(entry.tri_count) * 3));
    }
    let entries = match emit.list {
        PackedListKind::Smodel => &packed.smodel,
        PackedListKind::Cached => &packed.smodel_cached,
        PackedListKind::Pretess => &packed.smodel_pretess,
        PackedListKind::SmodelSkinned | PackedListKind::World | PackedListKind::XModel => {
            return None;
        }
    };
    entries
        .get(index)
        .map(|entry| (entry.index_byte_offset / 2, u32::from(entry.tri_count) * 3))
}

fn intended_colour_tech(
    colour: &FrameProduct,
    light: &FrameProduct,
    emissive: &FrameProduct,
    di: usize,
) -> TechType {
    colour_tech_at(colour, light, emissive, di).unwrap_or(TechType(0))
}

fn build_colour_row_plan(
    colour: &FrameProduct,
    light: &FrameProduct,
    emissive: &FrameProduct,
    packed: &render_frame::PackedFrontendLists,
    work: &super::backend::ColourDrawListWork,
    world_run_surfs: &[u16],
    pretess: &CameraWorldPretess,
) -> Vec<ColourRowPlan> {
    let layout_epoch = pretess.layout_epoch();
    let skip_end = colour
        .ordered_draws
        .len()
        .saturating_add(light.ordered_draws.len())
        .saturating_add(emissive.ordered_draws.len());
    let mut emitted = vec![false; skip_end];
    let mut rows = Vec::with_capacity(work.emit_order.len().max(skip_end));
    for emit in &work.emit_order {
        let Some(di) = packed_draw_index(packed, *emit) else {
            continue;
        };
        let already_emitted = emitted.get(di as usize).copied().unwrap_or(false);
        if emit.list == PackedListKind::World && already_emitted {
            continue;
        }
        if let Some(slot) = emitted.get_mut(di as usize) {
            *slot = true;
        }
        let tech = intended_colour_tech(colour, light, emissive, di as usize);
        if emit.list == PackedListKind::World {
            let Some(item) = colour_draw_at(colour, light, emissive, di as usize) else {
                continue;
            };
            if let Some(index_span) = world_emit_run_dest_span(item, world_run_surfs, pretess) {
                rows.push(ColourRowPlan {
                    draw_index: di,
                    world_surface_override: None,
                    technique: tech,
                    index_span: Some(index_span),
                    layout_epoch,
                });
            } else {
                push_expanded_world_row_plan(
                    item,
                    di,
                    tech,
                    world_run_surfs,
                    layout_epoch,
                    &mut rows,
                );
            }
            continue;
        }
        let span = colour_entry_index_span(packed, *emit);
        if colour_draw_at(colour, light, emissive, di as usize).is_none() {
            continue;
        }
        rows.push(ColourRowPlan {
            draw_index: di,
            world_surface_override: None,
            technique: tech,
            index_span: span,
            layout_epoch,
        });
    }
    for i in 0..skip_end {
        if emitted.get(i).copied().unwrap_or(false) {
            continue;
        }
        let Some(item) = colour_draw_at(colour, light, emissive, i) else {
            continue;
        };
        let tech = intended_colour_tech(colour, light, emissive, i);
        push_expanded_world_row_plan(
            item,
            i as u32,
            tech,
            world_run_surfs,
            layout_epoch,
            &mut rows,
        );
    }
    rows
}

fn world_emit_run_dest_span(
    item: &RetainedDrawItem,
    table: &[u16],
    pretess: &CameraWorldPretess,
) -> Option<(u32, u32)> {
    let RetainedDrawKind::World {
        surf, run, run_off, ..
    } = item.kind
    else {
        return None;
    };
    let run = run.max(1);
    let mut span: Option<(u32, u32)> = None;
    for offset in 0..run {
        let surf = if run == 1 {
            surf
        } else {
            *table.get(run_off as usize + usize::from(offset))?
        };
        let &(start, count) = pretess.ranges().get(usize::from(surf))?;
        if count == 0 {
            return None;
        }
        span = Some(match span {
            None => (start, count),
            Some((span_start, span_count)) => {
                if span_start.saturating_add(span_count) != start {
                    return None;
                }
                (span_start, span_count.saturating_add(count))
            }
        });
    }
    span
}

fn push_expanded_world_row_plan(
    item: &RetainedDrawItem,
    draw_index: u32,
    tech: TechType,
    table: &[u16],
    layout_epoch: u64,
    out: &mut Vec<ColourRowPlan>,
) {
    if let RetainedDrawKind::World { run, run_off, .. } = item.kind
        && run > 1
        && let Some(surfs) = table.get(run_off as usize..run_off as usize + usize::from(run))
        && surfs.len() == usize::from(run)
    {
        for &s in surfs {
            out.push(ColourRowPlan {
                draw_index,
                world_surface_override: Some(s),
                technique: tech,
                index_span: None,
                layout_epoch,
            });
        }
        return;
    }
    out.push(ColourRowPlan {
        draw_index,
        world_surface_override: None,
        technique: tech,
        index_span: None,
        layout_epoch,
    });
}

fn world_material_sorted(packed: u64) -> u16 {
    dpvs_iw4::GfxDrawSurf { packed }.material_sorted_index()
}

struct WorldPretessLayout {
    key: WorldPretessKey,
    index: Option<Buffer>,
    ranges: Vec<(u32, u32)>,
    index_gaps: u32,
    logical_index_count: u32,
    epoch: u64,
}

#[derive(Resource, Default)]
struct CameraWorldPretess {
    layout: Option<WorldPretessLayout>,

    epoch: u64,
}

impl CameraWorldPretess {
    fn ranges(&self) -> &[(u32, u32)] {
        self.layout
            .as_ref()
            .map(|layout| layout.ranges.as_slice())
            .unwrap_or(&[])
    }

    fn index(&self) -> Option<&Buffer> {
        self.layout
            .as_ref()
            .and_then(|layout| layout.index.as_ref())
    }

    fn logical_index_count(&self) -> u32 {
        self.layout
            .as_ref()
            .map(|layout| layout.logical_index_count)
            .unwrap_or(0)
    }

    fn layout_epoch(&self) -> u64 {
        self.layout.as_ref().map(|layout| layout.epoch).unwrap_or(0)
    }
}

fn exact_draw_run_continues(prev: &PreparedExactDraw, next: &PreparedExactDraw) -> bool {
    prev.after_scene_resolve == next.after_scene_resolve
        && prev.tess == next.tess
        && prev.pipeline == next.pipeline
        && prev.port == next.port
        && same_packed_constants(prev, next)
        && same_texture_slots(prev, next)
        && prev.depth_min == next.depth_min
        && prev.depth_max == next.depth_max
        && prev.smc_stream_off == next.smc_stream_off
        && bsp_prepared_runs_continue(prev, next)
        && prev.start.saturating_add(prev.count) == next.start
}

fn bsp_prepared_runs_continue(prev: &PreparedExactDraw, next: &PreparedExactDraw) -> bool {
    bsp_run_identity_continues(
        prev.bsp_kind,
        prev.bsp_run_first,
        prev.bsp_first_surf,
        prev.bsp_surf_count,
        next.bsp_kind,
        next.bsp_run_first,
        next.bsp_first_surf,
    )
}

fn bsp_run_identity_continues(
    previous_kind: Option<BspCameraLane>,
    previous_run_first: u16,
    previous_first: u16,
    previous_count: u16,
    current_kind: Option<BspCameraLane>,
    current_run_first: u16,
    current_first: u16,
) -> bool {
    match (previous_kind, current_kind) {
        (None, None) => true,
        (Some(previous), Some(current)) => {
            previous == current
                && previous_run_first == current_run_first
                && previous_first.checked_add(previous_count) == Some(current_first)
        }
        _ => false,
    }
}

fn same_texture_slots(prev: &PreparedExactDraw, next: &PreparedExactDraw) -> bool {
    Arc::ptr_eq(&prev.texture_slots, &next.texture_slots)
        || prev.texture_slots == next.texture_slots
}

fn same_packed_constants(prev: &PreparedExactDraw, next: &PreparedExactDraw) -> bool {
    match (&prev.constants, &next.constants) {
        (Some(left), Some(right)) => Arc::ptr_eq(left, right) || left == right,
        (None, None) => {
            prev.constant_base.is_some()
                && prev.constant_base == next.constant_base
                && prev.arena_lane == next.arena_lane
        }
        _ => false,
    }
}

fn coalesce_indexed_runs<T>(
    draws: &mut [T],
    can_extend: impl Fn(&T, &T) -> bool,
    extend: impl Fn(&mut T, &T),
) -> usize {
    let mut write = 0usize;
    let mut read = 0usize;
    while read < draws.len() {
        if write > 0 && can_extend(&draws[write - 1], &draws[read]) {
            let (head, tail) = draws.split_at_mut(read);
            extend(&mut head[write - 1], &tail[0]);
        } else {
            if write != read {
                draws.swap(write, read);
            }
            write += 1;
        }
        read += 1;
    }
    write
}

fn tess_flush_merge_slot<T>(
    out: &[T],
    draw: &T,
    continues: impl Fn(&T, &T) -> bool,
    look_past_sibling_pass: impl Fn(&T, &T) -> bool,
) -> Option<usize> {
    let mut i = out.len();
    while i > 0 {
        i -= 1;
        if continues(&out[i], draw) {
            return Some(i);
        }
        if look_past_sibling_pass(&out[i], draw) {
            continue;
        }
        break;
    }
    None
}

fn colour_look_past_sibling_pass(prev: &PreparedExactDraw, next: &PreparedExactDraw) -> bool {
    prev.after_scene_resolve == next.after_scene_resolve
        && prev.tess == next.tess
        && prev.pipeline != next.pipeline
        && (prev.start.saturating_add(prev.count) == next.start
            || (prev.start == next.start && prev.count == next.count))
}

fn coalesce_exact_draws(draws: &mut Vec<PreparedExactDraw>) {
    let mut len = 0usize;
    let mut read = 0usize;
    while read < draws.len() {
        let slot = {
            let (head, tail) = draws.split_at_mut(read);
            tess_flush_merge_slot(
                &head[..len],
                &tail[0],
                exact_draw_run_continues,
                colour_look_past_sibling_pass,
            )
        };
        match slot {
            Some(i) => {
                let count = draws[read].count;
                let bsp_surf_count = draws[read].bsp_surf_count;
                let bsp_counted = draws[read].bsp_counted;
                draws[i].count = draws[i].count.saturating_add(count);
                draws[i].bsp_surf_count = draws[i].bsp_surf_count.saturating_add(bsp_surf_count);
                draws[i].bsp_counted |= bsp_counted;
                draws[i].binds_sun_shadow |= draws[read].binds_sun_shadow;
                draws[i].binds_spot_shadow |= draws[read].binds_spot_shadow;
            }
            None => {
                if len != read {
                    draws.swap(len, read);
                }
                len += 1;
            }
        }
        read += 1;
    }
    draws.truncate(len);
}

fn shadow_gpu_state_continues(
    same_pipeline: bool,
    same_tess: bool,
    same_epoch: bool,
    same_texture: bool,
    prev_base: Option<u32>,
    next_base: Option<u32>,
    prev_depth: (f32, f32),
    next_depth: (f32, f32),
    prev_start: u32,
    prev_count: u32,
    next_start: u32,
) -> bool {
    same_pipeline
        && same_tess
        && same_epoch
        && same_texture
        && prev_base.is_some()
        && prev_base == next_base
        && prev_depth == next_depth
        && prev_start.saturating_add(prev_count) == next_start
}

fn shadow_draw_run_continues(prev: &PreparedExactDraw, next: &PreparedExactDraw) -> bool {
    shadow_gpu_state_continues(
        prev.pipeline == next.pipeline && prev.port == next.port,
        prev.tess == next.tess,
        prev.ring_epoch == next.ring_epoch,
        same_texture_slots(prev, next),
        prev.constant_base,
        next.constant_base,
        (prev.depth_min, prev.depth_max),
        (next.depth_min, next.depth_max),
        prev.start,
        prev.count,
        next.start,
    )
}

fn coalesce_shadow_draws(draws: &mut Vec<PreparedExactDraw>) {
    let live = coalesce_shadow_draws_prefix(draws);
    draws.truncate(live);
}

fn coalesce_shadow_draws_prefix(draws: &mut [PreparedExactDraw]) -> usize {
    coalesce_indexed_runs(draws, shadow_draw_run_continues, |prev, next| {
        prev.count = prev.count.saturating_add(next.count);
        prev.bsp_surf_count = prev.bsp_surf_count.saturating_add(next.bsp_surf_count);
        prev.bsp_counted |= next.bsp_counted;
    })
}

fn submit_exact_draws<'a>(
    encoder: &mut CommandEncoder,
    device: &RenderDevice,
    target: &ViewTarget,
    depth: &SceneDepthTexture,
    extracted_view: &ExtractedView,
    geometry: &ExactColourGeometry,
    smodel_cache_gpu: &SmodelCacheGpu,
    smodel_skinned_vertex: Option<&Buffer>,
    smodel_skinned_vertex_lighting: Option<&Buffer>,
    smodel_skinned_index: Option<&Buffer>,
    pretess: &CameraWorldPretess,
    indirect: &indirect::ExactIndirectDraws,
    registry: &ExactPipelineRegistry,
    constant_arena: &ExactConstantArena,
    textures_bind: [&BindGroup; 2],
    draws: impl IntoIterator<Item = &'a PreparedExactDraw>,
    label: &'static str,
    refused_draws: &mut u32,
    last_refusal: &mut Option<GpuSubmitRefusal>,
    encode_not_ready: &mut u32,
    focused_object_id: Option<u16>,
    minecraft: Option<&BindGroup>,
) -> (u32, f32, [u32; 4], [u32; 4], u32, RecordCensus) {
    let mut draws = draws.into_iter().peekable();
    if draws.peek().is_none() {
        return (0, 0.0, [0; 4], [0; 4], 0, RecordCensus::default());
    }
    let views = ExactColourTargetViews::new(target);
    let mut indexed = 0u32;
    let mut drop_ms = 0.0f32;
    let mut bsp_drawn_surfaces = [0u32; 4];
    let mut bsp_draw_refused_surfaces = [0u32; 4];
    let mut focused_drawn = 0u32;
    let mut record_n = RecordCensus::default();
    while let Some(first) = draws.peek() {
        let srgb_write = first.state.srgb_write_enable();
        let mut ops = target.get_color_attachment().ops;
        if srgb_write && matches!(ops.load, LoadOp::Clear(_)) {
            let (view, resolve_target) = views.attachment_views(false);
            let clear_attachments = [Some(RenderPassColorAttachment {
                view,
                resolve_target: resolve_target.map(|view| &**view),
                ops,
                depth_slice: None,
            })];
            let clear_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("iw4_exact_colour_raw_clear"),
                color_attachments: &clear_attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            drop(clear_pass);
            ops.load = LoadOp::Load;
        }
        let (view, resolve_target) = views.attachment_views(srgb_write);
        let attachment = RenderPassColorAttachment {
            view,
            resolve_target: resolve_target.map(|view| &**view),
            ops,
            depth_slice: None,
        };
        let (run_indexed, run_drop_ms, run_drawn, run_refused, run_focused_drawn, run_binds) =
            submit_exact_draw_run(
                encoder,
                device,
                attachment,
                depth,
                extracted_view,
                geometry,
                smodel_cache_gpu,
                smodel_skinned_vertex,
                smodel_skinned_vertex_lighting,
                smodel_skinned_index,
                pretess,
                indirect,
                registry,
                constant_arena,
                textures_bind[usize::from(srgb_write)],
                std::iter::from_fn(|| {
                    draws.next_if(|draw| draw.state.srgb_write_enable() == srgb_write)
                }),
                label,
                refused_draws,
                last_refusal,
                encode_not_ready,
                focused_object_id,
                minecraft,
            );
        indexed = indexed.saturating_add(run_indexed);
        focused_drawn = focused_drawn.saturating_add(run_focused_drawn);
        record_n.add(run_binds);
        drop_ms += run_drop_ms;
        for lane in 0..3 {
            bsp_drawn_surfaces[lane] = bsp_drawn_surfaces[lane].saturating_add(run_drawn[lane]);
            bsp_draw_refused_surfaces[lane] =
                bsp_draw_refused_surfaces[lane].saturating_add(run_refused[lane]);
        }
    }
    (
        indexed,
        drop_ms,
        bsp_drawn_surfaces,
        bsp_draw_refused_surfaces,
        focused_drawn,
        record_n,
    )
}

fn submit_exact_draw_run<'a>(
    encoder: &mut CommandEncoder,
    device: &RenderDevice,
    attachment: RenderPassColorAttachment<'_>,
    depth: &SceneDepthTexture,
    extracted_view: &ExtractedView,
    geometry: &ExactColourGeometry,
    smodel_cache_gpu: &SmodelCacheGpu,
    smodel_skinned_vertex: Option<&Buffer>,
    smodel_skinned_vertex_lighting: Option<&Buffer>,
    smodel_skinned_index: Option<&Buffer>,
    pretess: &CameraWorldPretess,
    indirect: &indirect::ExactIndirectDraws,
    registry: &ExactPipelineRegistry,
    constant_arena: &ExactConstantArena,
    textures_bind: &BindGroup,
    draws: impl IntoIterator<Item = &'a PreparedExactDraw>,
    label: &'static str,
    refused_draws: &mut u32,
    last_refusal: &mut Option<GpuSubmitRefusal>,
    encode_not_ready: &mut u32,
    focused_object_id: Option<u16>,
    minecraft: Option<&BindGroup>,
) -> (u32, f32, [u32; 4], [u32; 4], u32, RecordCensus) {
    let attachments = [Some(attachment)];
    let mut pass = TrackedRenderPass::new(
        device,
        encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some(label),
            color_attachments: &attachments,
            depth_stencil_attachment: Some(depth.get_attachment(StoreOp::Store)),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        }),
    );
    let mut bound_pipeline = None;
    let mut bound_tess = None;
    let mut bound_smc_off = None;
    let mut bound_depth = None;
    let mut bound_arena = None;
    let mut textures_bound = false;

    let indirect_args = indirect.buffer();
    let mut batch = indirect::IndirectBatch::default();
    let mut record_n = RecordCensus::default();

    macro_rules! issue_indirect_batch {
        () => {
            if let Some(done) = batch.take() {
                issue_indirect(&mut pass, indirect_args, done, &mut record_n);
            }
        };
    }
    let mut indexed = 0u32;
    let mut bsp_drawn_surfaces = [0u32; 4];
    let mut bsp_draw_refused_surfaces = [0u32; 4];
    let mut focused_drawn = 0u32;
    let viewport = extracted_view.viewport;
    let vp_x = viewport.x as f32;
    let vp_y = viewport.y as f32;
    let vp_w = viewport.z as f32;
    let vp_h = viewport.w as f32;

    for draw in draws {
        let Some(gpu_pipeline) = registry.ready(draw.pipeline) else {
            *refused_draws = refused_draws.saturating_add(1);
            *encode_not_ready = encode_not_ready.saturating_add(1);
            *last_refusal = Some(GpuSubmitRefusal::PipelineNotReady);
            if draw.bsp_counted
                && let Some(kind) = draw.bsp_kind
            {
                let lane = bsp_kind_index(kind);
                bsp_draw_refused_surfaces[lane] =
                    bsp_draw_refused_surfaces[lane].saturating_add(u32::from(draw.bsp_surf_count));
            }
            continue;
        };
        if draw.count == 0 {
            continue;
        }
        let (Some(constant_base), Some(constants_bind)) = (
            draw.constant_base,
            constant_arena
                .gpu
                .get(usize::from(draw.arena_lane))
                .and_then(|arena| arena.bind_group.as_ref()),
        ) else {
            *refused_draws = refused_draws.saturating_add(1);
            *last_refusal = Some(GpuSubmitRefusal::ConstantArenaMissing);
            continue;
        };
        let (vertex, index) = match draw.tess {
            ExactTessBind::World => (geometry.world_vertex.as_ref(), pretess.index()),
            ExactTessBind::Smodel => (
                geometry.smodel_vertex.as_ref(),
                geometry.smodel_index.as_ref(),
            ),
            ExactTessBind::SmodelCached => (
                smodel_cache_gpu.vertex_buffer(),
                smodel_cache_gpu.dynamic_index_buffer(),
            ),
            ExactTessBind::SmodelSkinned => (smodel_skinned_vertex, smodel_skinned_index),
            ExactTessBind::XModel => (
                geometry.xmodel.vertex.buffer(),
                geometry.xmodel.index.buffer(),
            ),
            ExactTessBind::CodeMesh => (geometry.fx_vertex.as_ref(), geometry.fx_index.as_ref()),
            ExactTessBind::ParticleCloud => (
                geometry.particle_cloud.vertex.buffer(),
                geometry.particle_cloud.index.buffer(),
            ),
            ExactTessBind::MarkMesh => (
                geometry.mark_mesh.vertex.buffer(),
                geometry.mark_mesh.index.buffer(),
            ),
            ExactTessBind::Glass => (
                geometry.glass_mesh.vertex.buffer(),
                geometry.glass_mesh.index.buffer(),
            ),
        };
        let (Some(vertex), Some(index)) = (vertex, index) else {
            *refused_draws = refused_draws.saturating_add(1);
            *last_refusal = Some(GpuSubmitRefusal::UnsupportedTessKind);
            if draw.bsp_counted
                && let Some(kind) = draw.bsp_kind
            {
                let lane = bsp_kind_index(kind);
                bsp_draw_refused_surfaces[lane] =
                    bsp_draw_refused_surfaces[lane].saturating_add(u32::from(draw.bsp_surf_count));
            }
            continue;
        };
        if bound_pipeline != Some(draw.pipeline)
            || bound_tess != Some(draw.tess)
            || (draw.tess == ExactTessBind::SmodelCached
                && bound_smc_off != Some(draw.smc_stream_off))
        {
            issue_indirect_batch!();
            pass.set_render_pipeline(gpu_pipeline);
            let vertex_slice = if draw.tess == ExactTessBind::SmodelCached {
                let end = draw
                    .smc_stream_off
                    .saturating_add(u64::from(lighting_iw4::SMC_BANK_VB_BYTES));
                vertex.slice(draw.smc_stream_off..end)
            } else {
                vertex.slice(..)
            };
            pass.set_vertex_buffer(0, vertex_slice);
            if let Some(lighting) =
                vertex_lighting_stream(draw.tess, geometry, smodel_skinned_vertex_lighting)
            {
                pass.set_vertex_buffer(1, lighting.slice(..));
            }
            if draw.tess == ExactTessBind::World
                && let Some(effect) = geometry.world_effect.as_ref()
            {
                pass.set_vertex_buffer(2, effect.slice(..));
            }
            record_n.state = record_n.state.saturating_add(2);
            if draw.tess == ExactTessBind::World {
                if let Some(layer) = geometry.world_layer.as_ref() {
                    pass.set_vertex_buffer(1, layer.slice(..));
                    record_n.state = record_n.state.saturating_add(1);
                }
            }
            let index_format = if matches!(
                draw.tess,
                ExactTessBind::SmodelCached | ExactTessBind::MarkMesh
            ) {
                IndexFormat::Uint16
            } else {
                IndexFormat::Uint32
            };
            pass.set_index_buffer(index.slice(..), index_format);
            record_n.state = record_n.state.saturating_add(1);
            bound_pipeline = Some(draw.pipeline);
            bound_tess = Some(draw.tess);
            bound_smc_off = Some(draw.smc_stream_off);
        }
        if vp_w > 0.0 && vp_h > 0.0 && bound_depth != Some((draw.depth_min, draw.depth_max)) {
            issue_indirect_batch!();
            pass.set_viewport(vp_x, vp_y, vp_w, vp_h, draw.depth_min, draw.depth_max);
            record_n.state = record_n.state.saturating_add(1);
            bound_depth = Some((draw.depth_min, draw.depth_max));
        }

        if bound_arena != Some(draw.arena_lane) {
            issue_indirect_batch!();
            pass.set_bind_group(0, constants_bind, &[]);
            if let Some(minecraft) = minecraft {
                pass.set_bind_group(super::minecraft_light::MINECRAFT_GROUP, minecraft, &[]);
            }
            bound_arena = Some(draw.arena_lane);
            record_n.group0 = record_n.group0.saturating_add(1);
        }
        if !textures_bound {
            pass.set_bind_group(1, textures_bind, &[]);
            textures_bound = true;
            record_n.group1 = record_n.group1.saturating_add(1);
        }
        if draw.tess == ExactTessBind::World {
            let logical = pretess.logical_index_count();
            let end = draw.start.saturating_add(draw.count);
            if end > logical {
                *refused_draws = refused_draws.saturating_add(1);
                *last_refusal = Some(GpuSubmitRefusal::WorldPretessSpanBeyondLimit {
                    start: draw.start,
                    count: draw.count,
                    logical_len: logical,
                    epoch: pretess.layout_epoch(),
                });
                continue;
            }
        }
        match (indirect_args, draw.indirect_arg) {
            (Some(_), Some(slot)) => {
                if let Some(done) = batch.push(slot) {
                    issue_indirect(&mut pass, indirect_args, done, &mut record_n);
                }
            }
            _ => pass.draw_indexed(
                draw.start..draw.start.saturating_add(draw.count),
                0,
                constant_base..constant_base.saturating_add(1),
            ),
        }
        indexed = indexed.saturating_add(1);
        if draw.owner_object_id == focused_object_id && focused_object_id.is_some() {
            focused_drawn = focused_drawn.saturating_add(1);
        }
        if draw.bsp_counted
            && let Some(kind) = draw.bsp_kind
        {
            let lane = bsp_kind_index(kind);
            bsp_drawn_surfaces[lane] =
                bsp_drawn_surfaces[lane].saturating_add(u32::from(draw.bsp_surf_count));
        }
    }
    issue_indirect_batch!();
    let drop_started = Instant::now();
    drop(pass);
    (
        indexed,
        drop_started.elapsed().as_secs_f32() * 1000.0,
        bsp_drawn_surfaces,
        bsp_draw_refused_surfaces,
        focused_drawn,
        record_n,
    )
}

fn issue_indirect<'a>(
    pass: &mut TrackedRenderPass<'a>,
    args: Option<&'a Buffer>,
    batch: indirect::IndirectBatch,
    record_n: &mut RecordCensus,
) {
    let Some(args) = args else {
        return;
    };
    pass.multi_draw_indexed_indirect(
        args,
        indirect::ExactIndirectDraws::byte_offset(batch.first()),
        batch.count(),
    );
    record_n.multi_draws = record_n.multi_draws.saturating_add(1);
    record_n.multi_draw_commands = record_n.multi_draw_commands.saturating_add(batch.count());
}

fn sun_flush_owner<'a>(
    draws: &'a [RetainedDrawItem],
    techs: &[TechType],
    draw_indices: &[u32],
    draw_offset: usize,
    kind: SunFlushKind,
) -> Result<(&'a RetainedDrawItem, TechType), String> {
    let missing = match kind {
        SunFlushKind::Smodel => "SmodelProvenanceMissing",
        SunFlushKind::XModel => "XModelDynamicProvenanceMissing",
        SunFlushKind::World => "WorldProvenanceMissing",
    };
    let Some(&draw_index) = draw_indices.first() else {
        return Err(missing.into());
    };
    for &owner in draw_indices {
        let index = (owner as usize).saturating_add(draw_offset);
        if draws.get(index).is_none() || techs.get(index).is_none() {
            return Err(missing.into());
        }
    }
    let draw_index = (draw_index as usize).saturating_add(draw_offset);
    let Some(item) = draws.get(draw_index) else {
        return Err(missing.into());
    };
    let matches_kind = match (kind, &item.kind) {
        (SunFlushKind::Smodel, RetainedDrawKind::Smodel { .. }) => true,
        (SunFlushKind::XModel, RetainedDrawKind::XModel { .. }) => true,
        (SunFlushKind::World, RetainedDrawKind::World { .. }) => true,
        _ => false,
    };
    if !matches_kind {
        return Err(missing.into());
    }
    let Some(&tech) = techs.get(draw_index) else {
        return Err("NoListTech".into());
    };
    Ok((item, tech))
}

fn sun_flush_world_from_local(kind: &RetainedDrawKind) -> Mat4 {
    match *kind {
        RetainedDrawKind::Smodel {
            stream: Some(lighting_iw4::SmodelSurfPath::Skinned),
            ..
        } => Mat4::IDENTITY,
        RetainedDrawKind::World {
            world_from_local, ..
        }
        | RetainedDrawKind::Smodel {
            world_from_local, ..
        }
        | RetainedDrawKind::XModel {
            world_from_local, ..
        } => world_from_local,
        _ => Mat4::IDENTITY,
    }
}

fn emit_local_rigid_shadow_flush(
    source_indices: &[u32],
    hit_start: u32,
    hit_count: u32,
    stream: &mut ModelIndexStream,
    family: &'static str,
) -> Result<(u32, u32, u32), String> {
    if hit_count == 0 {
        return Err(format!("{family}DynamicFlushEmpty"));
    }
    if hit_count % 3 != 0 {
        return Err(format!("{family}DynamicIndexCountNotTriangles"));
    }
    let start = hit_start as usize;
    let end = start.saturating_add(hit_count as usize);
    let indices = source_indices
        .get(start..end)
        .ok_or_else(|| format!("{family}DynamicIndexSourceMissing"))?;
    let (epoch, append) = stream
        .append_indices(indices)
        .map_err(|refuse| match refuse {
            ModelIndexRingCopyRefuse::SourceMissing => {
                format!("{family}DynamicIndexSourceMissing")
            }
            ModelIndexRingCopyRefuse::ExceedsCapacity => format!("{family}RingExceedsCapacity"),
        })?;
    Ok((epoch, append.base_index, hit_count))
}

fn emit_local_xmodel_shadow_flush(
    source_indices: &[u32],
    hit_start: u32,
    hit_count: u32,
    stream: &mut ModelIndexStream,
) -> Result<(u32, u32, u32), String> {
    emit_local_rigid_shadow_flush(source_indices, hit_start, hit_count, stream, "XModel")
}

fn emit_local_smodel_shadow_flush(
    source_indices: &[u32],
    hit_start: u32,
    hit_count: u32,
    stream: &mut ModelIndexStream,
) -> Result<(u32, u32, u32), String> {
    emit_local_rigid_shadow_flush(source_indices, hit_start, hit_count, stream, "Smodel")
}

#[derive(Clone)]
struct ShadowmapSunFlushGpu {
    draws: Vec<PreparedExactDraw>,
    world_from_local: Mat4,
    pass_code: Vec<PackedCodeConstants>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ShadowStaticOwner {
    World {
        key: u64,
        surf: u16,
        run: u16,
        run_off: u32,
        bsp_kind: Option<BspCameraLane>,
        bsp_run_first: Option<u16>,
        setup_key_changed: bool,
        world_from_local: [u32; 16],
        samplers: super::SurfaceSamplerInputs,
    },
    Smodel {
        key: u64,
        surface: u32,
        material: u32,
        world_from_local: [u32; 16],
        lighting_handle: u32,
        packed_lighting: Option<[u8; 4]>,
        placement: u32,
        stream: Option<lighting_iw4::SmodelSurfPath>,
        cache_index: Option<u16>,
        pretess: Option<render_frame::SmodelPretessRange>,
        samplers: super::SurfaceSamplerInputs,
    },
}

fn world_shadow_static_owner(item: &RetainedDrawItem) -> Option<ShadowStaticOwner> {
    let RetainedDrawKind::World {
        surf,
        run,
        run_off,
        bsp_kind,
        bsp_run_first,
        setup_key_changed,
        world_from_local,
    } = item.kind
    else {
        return None;
    };
    Some(ShadowStaticOwner::World {
        key: item.key,
        surf,
        run,
        run_off,
        bsp_kind,
        bsp_run_first,
        setup_key_changed,
        world_from_local: overlay_world_key(world_from_local),
        samplers: item.surface_samplers,
    })
}

fn smodel_shadow_static_owner(item: &RetainedDrawItem) -> Option<ShadowStaticOwner> {
    let RetainedDrawKind::Smodel {
        surface,
        material,
        world_from_local,
        lighting_handle,
        packed_lighting,
        placement,
        stream,
        cache_index,
        pretess,
    } = item.kind
    else {
        return None;
    };
    Some(ShadowStaticOwner::Smodel {
        key: item.key,
        surface,
        material,
        world_from_local: overlay_world_key(world_from_local),
        lighting_handle,
        packed_lighting,
        placement,
        stream,
        cache_index,
        pretess,
        samplers: item.surface_samplers,
    })
}

fn static_shadow_owners_answer(
    retained: &[Option<ShadowStaticOwner>],
    draws: &[RetainedDrawItem],
    draw_indices: &[u32],
    owner: fn(&RetainedDrawItem) -> Option<ShadowStaticOwner>,
) -> bool {
    retained.len() == draw_indices.len()
        && retained.iter().zip(draw_indices).all(|(expected, &index)| {
            draws.get(index as usize).and_then(owner).as_ref() == expected.as_ref()
        })
}

fn replace_static_shadow_owners(
    retained: &mut Vec<Option<ShadowStaticOwner>>,
    draws: &[RetainedDrawItem],
    draw_indices: &[u32],
    owner: fn(&RetainedDrawItem) -> Option<ShadowStaticOwner>,
) {
    retained.clear();
    retained.extend(
        draw_indices
            .iter()
            .map(|&index| draws.get(index as usize).and_then(owner)),
    );
}

#[derive(Resource, Default)]
struct ResidentShadowStaticDraws {
    generation: Option<u64>,
    world: [Vec<Option<ShadowmapSunFlushGpu>>; 2],
    smodel: [Vec<Option<ShadowmapSunFlushGpu>>; 2],
    world_owners: [Vec<Option<ShadowStaticOwner>>; 2],
    smodel_owners: [Vec<Option<ShadowStaticOwner>>; 2],
    smodel_entries: [Vec<render_frame::GfxSmodelRigidEntry>; 2],

    commands: [ResidentSunCommands; 2],
}

impl ResidentShadowStaticDraws {
    fn reset_unless_answering(
        &mut self,
        generation: u64,
        partition: usize,
        world_n: usize,
        smodel_n: usize,
        draws: &[RetainedDrawItem],
        packed: &render_frame::PackedFrontendLists,
    ) -> bool {
        let i = partition.min(1);
        let generation_changed = self.generation != Some(generation);
        if generation_changed {
            self.generation = Some(generation);
            self.world = [Vec::new(), Vec::new()];
            self.smodel = [Vec::new(), Vec::new()];
            self.world_owners = [Vec::new(), Vec::new()];
            self.smodel_owners = [Vec::new(), Vec::new()];
            self.smodel_entries = [Vec::new(), Vec::new()];
        }
        let world_changed = generation_changed
            || !static_shadow_owners_answer(
                &self.world_owners[i],
                draws,
                &packed.world_draw_indices,
                world_shadow_static_owner,
            );
        let smodel_changed = generation_changed
            || self.smodel_entries[i] != packed.smodel
            || !static_shadow_owners_answer(
                &self.smodel_owners[i],
                draws,
                &packed.smodel_draw_indices,
                smodel_shadow_static_owner,
            );
        if world_changed || self.world[i].len() != world_n {
            self.world[i].clear();
            self.world[i].resize_with(world_n, || None);
        }
        if smodel_changed || self.smodel[i].len() != smodel_n {
            self.smodel[i].clear();
            self.smodel[i].resize_with(smodel_n, || None);
        }
        if world_changed {
            replace_static_shadow_owners(
                &mut self.world_owners[i],
                draws,
                &packed.world_draw_indices,
                world_shadow_static_owner,
            );
        }
        if smodel_changed {
            self.smodel_entries[i].clone_from(&packed.smodel);
            replace_static_shadow_owners(
                &mut self.smodel_owners[i],
                draws,
                &packed.smodel_draw_indices,
                smodel_shadow_static_owner,
            );
        }
        smodel_changed
    }
}

#[derive(Clone, Copy)]
struct SunCodeSite {
    index: u16,
    byte_off: usize,
    rows: usize,
}

struct SunArenaSlot {
    /// Index into `ResidentSunCommands::placements`. Slots that share a
    /// placement share one world-view-projection, and the partition patch
    /// computes it once for the whole group.
    placement: u32,
    sites: Vec<SunCodeSite>,
}

#[derive(Default)]
struct ResidentSunCommands {
    generation: Option<u64>,
    world_n: usize,
    smodel_n: usize,
    world_draws: Vec<PreparedExactDraw>,
    smodel_draws: Vec<PreparedExactDraw>,
    slots: Vec<SunArenaSlot>,
    placements: Vec<Mat4>,
    placement_index: HashMap<[u32; 16], u32>,
    placement_wvp: Vec<[[u32; 4]; 4]>,
    bytes: Vec<u8>,

    dirty: Vec<ArenaDirty>,
    resident_slots: usize,
    resident_placements: usize,
    resident_bytes: usize,

    refused: u32,
}

impl ResidentSunCommands {
    fn rebuild(
        &mut self,
        generation: u64,
        world: &[Option<ShadowmapSunFlushGpu>],
        smodel: &[Option<ShadowmapSunFlushGpu>],
    ) {
        self.generation = Some(generation);
        self.world_n = world.len();
        self.smodel_n = smodel.len();
        self.slots.clear();
        self.placements.clear();
        self.placement_index.clear();
        self.bytes.clear();
        self.refused = 0;
        let mut seen = HashMap::new();
        let mut world_draws = Vec::new();
        let mut smodel_draws = Vec::new();
        for flush in world.iter().flatten() {
            self.push_flush(flush, &mut seen, &mut world_draws);
        }
        for flush in smodel.iter().flatten() {
            self.push_flush(flush, &mut seen, &mut smodel_draws);
        }
        coalesce_shadow_draws(&mut world_draws);
        coalesce_shadow_draws(&mut smodel_draws);
        self.world_draws = world_draws;
        self.smodel_draws = smodel_draws;
        self.resident_slots = self.slots.len();
        self.resident_placements = self.placements.len();
        self.resident_bytes = self.bytes.len();
    }

    fn open_frame(&mut self) {
        self.slots.truncate(self.resident_slots);
        self.placements.truncate(self.resident_placements);
        let resident = u32::try_from(self.resident_placements).unwrap_or(u32::MAX);
        self.placement_index.retain(|_, index| *index < resident);
        self.bytes.truncate(self.resident_bytes);
        self.dirty.clear();
    }

    /// Placements repeat: every world surface is at the identity, and a static
    /// model's surfaces share one transform. The index makes the patch below
    /// one matrix product per distinct placement instead of one per slot.
    fn placement_for(&mut self, world_key: [u32; 16], world_from_local: Mat4) -> u32 {
        if let Some(&index) = self.placement_index.get(&world_key) {
            return index;
        }
        let index = u32::try_from(self.placements.len()).expect("sun placement index fits u32");
        self.placements.push(world_from_local);
        self.placement_index.insert(world_key, index);
        index
    }

    fn push_flush(
        &mut self,
        flush: &ShadowmapSunFlushGpu,
        seen: &mut HashMap<(usize, usize, [u32; 16]), u32>,
        out: &mut Vec<PreparedExactDraw>,
    ) {
        let world_key = overlay_world_key(flush.world_from_local);
        for (draw, code) in flush.draws.iter().zip(flush.pass_code.iter()) {
            let Some(constants) = draw.constants.as_ref() else {
                out.push(draw.clone());
                continue;
            };
            let key = (
                Arc::as_ptr(constants) as usize,
                Arc::as_ptr(&draw.texture_slots).cast::<u32>() as usize,
                world_key,
            );
            let base = match seen.get(&key) {
                Some(&base) => base,
                None => {
                    let vertex_off = self.bytes.len();
                    let pixel_off = vertex_off + std::mem::size_of_val(constants.vertex.as_slice());
                    let Some(sites) = sun_code_sites(code, constants, vertex_off, pixel_off) else {
                        self.refused = self.refused.saturating_add(1);
                        continue;
                    };
                    let base =
                        u32::try_from(vertex_off / 16).expect("sun constant arena row fits u32");
                    append_constant_span(&mut self.bytes, constants, &draw.texture_slots);
                    self.dirty.push(ArenaDirty::Identity {
                        start: vertex_off,
                        end: self.bytes.len(),
                    });
                    let placement = self.placement_for(world_key, flush.world_from_local);
                    self.slots.push(SunArenaSlot { placement, sites });
                    seen.insert(key, base);
                    base
                }
            };
            let mut packed = draw.clone();

            packed.constants = None;
            packed.constant_base = Some(base);
            out.push(packed);
        }
    }

    fn patch_sun_registers(&mut self, partition: &SunShadowPartition) {
        let projection = super::code_transpose_matrix_row4(partition.projection);
        let polygon_offset = [partition.polygon_offset.map(f32::to_bits)];
        let mut wvp = std::mem::take(&mut self.placement_wvp);
        wvp.clear();
        wvp.extend(self.placements.iter().map(|world_from_local| {
            super::code_transpose_matrix_row4(partition.clip_from_world * *world_from_local)
        }));
        for slot in &self.slots {
            let Some(placement_wvp) = wvp.get(slot.placement as usize) else {
                continue;
            };
            for site in &slot.sites {
                let rows: &[[u32; 4]] = match site.index {
                    super::CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0 => &placement_wvp[..],
                    render_backend::overlay::CODE_TRANSPOSE_PROJECTION => &projection,
                    super::CODE_SHADOWMAP_POLYGON_OFFSET => &polygon_offset,
                    _ => continue,
                };
                let end = site.byte_off + site.rows * 16;
                let bytes = bytemuck::cast_slice(&rows[..site.rows]);
                if self.bytes[site.byte_off..end] != *bytes {
                    self.bytes[site.byte_off..end].copy_from_slice(bytes);
                    self.dirty.push(ArenaDirty::Identity {
                        start: site.byte_off,
                        end,
                    });
                }
            }
        }
        self.placement_wvp = wvp;
    }
}

fn sun_code_sites(
    code: &PackedCodeConstants,
    constants: &PassConstantBuffers,
    vertex_off: usize,
    pixel_off: usize,
) -> Option<Vec<SunCodeSite>> {
    let mut sites = Vec::new();
    for lane in &code.lanes {
        let rows = match lane.index {
            super::CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0
            | render_backend::overlay::CODE_TRANSPOSE_PROJECTION => 4usize,
            super::CODE_SHADOWMAP_POLYGON_OFFSET => 1usize,
            _ => continue,
        };
        let (bank_len, base) = match lane.stage {
            RuntimeShaderStage::Vertex => (constants.vertex.len(), vertex_off),
            RuntimeShaderStage::Pixel => (constants.pixel.len(), pixel_off),
        };
        let first = usize::from(lane.destination);
        if first.checked_add(rows)? > bank_len {
            return None;
        }
        sites.push(SunCodeSite {
            index: lane.index,
            byte_off: base + first * 16,
            rows,
        });
    }
    Some(sites)
}

fn sun_overlay_refusal(partition: usize) -> &'static str {
    if partition == 0 {
        "NearOverlayRefused"
    } else {
        "FarOverlayRefused"
    }
}

fn overlay_world_key(world_from_local: Mat4) -> [u32; 16] {
    world_from_local.to_cols_array().map(f32::to_bits)
}

type ShadowOverlayIntern = HashMap<(usize, u8, [u32; 16]), Arc<PassConstantBuffers>>;

fn intern_overlaid_shadow_banks(
    interned: &Arc<PassConstantBuffers>,
    overlay: &PackedCodeConstants,
    intern: &mut ShadowOverlayIntern,
    partition_i: u8,
    world_from_local: Mat4,
) -> Result<Arc<PassConstantBuffers>, ()> {
    let key = (
        Arc::as_ptr(interned) as usize,
        partition_i,
        overlay_world_key(world_from_local),
    );
    if let Some(hit) = intern.get(&key) {
        return Ok(Arc::clone(hit));
    }
    let mut banks = (**interned).clone();
    overlay_packed_code_on_banks(&mut banks.vertex, &mut banks.pixel, &overlay.lanes)
        .map_err(|_| ())?;
    let out = Arc::new(banks);
    intern.insert(key, Arc::clone(&out));
    Ok(out)
}

fn overlay_shadow_draw_constants_with(
    draw: &PreparedExactDraw,
    overlay: &PackedCodeConstants,
    world_from_local: Mat4,
    overlay_i: u8,
    intern: &mut ShadowOverlayIntern,
) -> Result<PreparedExactDraw, ()> {
    let mut out = draw.clone();
    let Some(constants) = draw.constants.as_ref() else {
        return Ok(out);
    };
    let key = (
        Arc::as_ptr(constants) as usize,
        overlay_i,
        overlay_world_key(world_from_local),
    );
    let overlaid = if let Some(hit) = intern.get(&key) {
        Arc::clone(hit)
    } else {
        intern_overlaid_shadow_banks(constants, overlay, intern, overlay_i, world_from_local)?
    };
    out.constants = Some(overlaid);
    out.constant_base = None;
    Ok(out)
}

fn spot_overlay_lane(
    template: &PackedCodeConstantLane,
    rows: Vec<[u32; 4]>,
) -> PackedCodeConstantLane {
    PackedCodeConstantLane {
        stage: template.stage,
        destination: template.destination,
        index: template.index,
        first_row: 0,
        row_count: u8::try_from(rows.len()).unwrap_or(u8::MAX),
        rows: Arc::from(rows.into_boxed_slice()),
    }
}

fn shadowmap_spot_overlay(
    template: &PackedCodeConstants,
    parms: &lighting_iw4::SpotShadowViewParms,
    world_from_local: Mat4,
) -> PackedCodeConstants {
    let clip_from_world = Mat4::from_cols_array(&parms.view_projection)
        * Mat4::from_translation(-Vec3::from_array(parms.origin));
    let wvp = super::code_transpose_matrix_rows(clip_from_world * world_from_local);
    let projection = super::code_transpose_matrix_rows(Mat4::from_cols_array(&parms.projection));
    let lanes = template
        .lanes
        .iter()
        .filter_map(|lane| match lane.index {
            super::CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0 => {
                Some(spot_overlay_lane(lane, wvp.clone()))
            }
            render_backend::overlay::CODE_TRANSPOSE_PROJECTION => {
                Some(spot_overlay_lane(lane, projection.clone()))
            }
            _ => None,
        })
        .collect();
    PackedCodeConstants::from_lanes(lanes)
}

#[derive(Clone, Copy)]
enum SunFlushKind {
    World,
    Smodel,
    XModel,
}

impl SunFlushKind {
    fn family(self) -> &'static str {
        match self {
            SunFlushKind::World => "World",
            SunFlushKind::Smodel => "Smodel",
            SunFlushKind::XModel => "XModel",
        }
    }
}

impl ExactPrepare<'_> {
    fn prepare_shadow_flush_draws(
        &mut self,
        executor: &mut MaterialRunExecutor,
        view: Option<MaterialExecView<'_>>,
        flush: SunFlush,
        owner_draws: &[u32],
        draw_offset: usize,
        product: &FrameProduct,
        target: ExactPrepareTarget,
    ) -> Result<ShadowmapSunFlushGpu, String> {
        let result: Result<ShadowmapSunFlushGpu, String> = (|| {
            if flush.draw_count == 0 {
                return Err("EmptyFlush".into());
            }
            let (item, tech) = sun_flush_owner(
                &product.ordered_draws,
                &product.draw_tech,
                owner_draws,
                draw_offset,
                flush.kind,
            )?;
            let view = view.ok_or_else(|| "MaterialTablesMissing".to_string())?;
            let vertex_type = MaterialRunExecutor::vertex_type(view, item, tech);
            executor
                .execute(view, item, tech, vertex_type, false)
                .map_err(|_| "ExecRefused".to_string())?;
            if matches!(flush.kind, SunFlushKind::Smodel)
                && vertex_type == asset_iw4::vertex_decl::STATICMODELCACHE_VERTEX_TYPE
            {
                return Err("SmodelCachedNotRigidStream".into());
            }
            let run_serial = executor.run_serial();
            let execution = executor.execution();
            let mut prepared = Vec::new();
            self.prepare_ready_hit(
                item,
                execution,
                run_serial,
                None,
                item.surface_samplers,
                target,
                false,
                false,
                &mut prepared,
            )
            .map_err(|cause| format!("{cause:?}"))?;
            for draw in &mut prepared {
                if draw.tess != ExactTessBind::SmodelSkinned {
                    draw.start = flush.draw_start;
                    draw.count = flush.draw_count;
                    draw.ring_epoch = flush.ring_epoch;
                }
            }
            Ok(ShadowmapSunFlushGpu {
                draws: prepared,
                world_from_local: sun_flush_world_from_local(&item.kind),
                pass_code: execution.code_constants().to_vec(),
            })
        })();
        result.map_err(|cause| format!("{}:{cause}", flush.kind.family()))
    }
}

fn prepare_smodel_skinned_shadow_gpu(
    prepare: &mut ExactPrepare<'_>,
    executor: &mut MaterialRunExecutor,
    view: Option<MaterialExecView<'_>>,
    packed: &PackedFrontendLists,
    flushes: &[SmodelRigidFlush],
    product: &FrameProduct,
    draw_offset: usize,
    target: ExactPrepareTarget,
    miss: &mut u32,
    miss_rows: &mut BTreeMap<String, u32>,
) -> Vec<ShadowmapSunFlushGpu> {
    let mut out = Vec::new();
    for flush in flushes {
        let owner_start = flush.entry_start as usize;
        let owner_end = owner_start.saturating_add(flush.entry_count as usize);
        let owners = packed
            .smodel_skinned_draw_indices
            .get(owner_start..owner_end)
            .unwrap_or(&[]);
        match prepare.prepare_shadow_flush_draws(
            executor,
            view,
            SunFlush {
                kind: SunFlushKind::Smodel,
                draw_start: 0,
                draw_count: 1,
                ring_epoch: 0,
            },
            owners,
            draw_offset,
            product,
            target,
        ) {
            Ok(gpu) => out.push(gpu),
            Err(cause) => {
                *miss = miss.saturating_add(1);
                *miss_rows.entry(cause).or_default() += 1;
            }
        }
    }
    out
}

fn vertex_lighting_stream<'a>(
    tess: ExactTessBind,
    geometry: &'a ExactColourGeometry,
    smodel_skinned: Option<&'a Buffer>,
) -> Option<&'a Buffer> {
    match tess {
        ExactTessBind::World => None,
        ExactTessBind::Smodel => geometry
            .smodel_vertex_lighting
            .as_ref()
            .or(geometry.neutral_vertex_lighting.as_ref()),
        ExactTessBind::SmodelSkinned => smodel_skinned,
        _ => geometry.neutral_vertex_lighting.as_ref(),
    }
}

fn record_shadowmap_draws<'a>(
    encoder: &mut CommandEncoder,
    device: &RenderDevice,
    registry: &ExactPipelineRegistry,
    geometry: &ExactColourGeometry,

    world_index: Option<&Buffer>,
    smodel_index_epochs: &[Buffer],
    xmodel_index_epochs: &[Buffer],
    smodel_vertex: Option<&Buffer>,
    xmodel_vertex: Option<&Buffer>,
    smodel_skinned_vertex: Option<&Buffer>,
    smodel_skinned_vertex_lighting: Option<&Buffer>,
    smodel_skinned_index: Option<&Buffer>,
    constants_bind: Option<&BindGroup>,
    textures_bind: &BindGroup,
    draws: impl IntoIterator<Item = &'a PreparedExactDraw>,
    color_view: &bevy::render::render_resource::TextureView,
    depth_view: &bevy::render::render_resource::TextureView,
    scissor: super::backend::D3dScissorRect,
    viewport: SunShadowViewport,
    cleared: bool,
    label: &'static str,
    miss: &mut u32,
    miss_rows: &mut BTreeMap<String, u32>,
    minecraft: Option<&BindGroup>,
) -> (u32, RecordCensus) {
    let (sx, sy, sw, sh) = scissor_xywh(scissor);
    let vp = viewport;
    let attachments = [Some(RenderPassColorAttachment {
        view: color_view,
        resolve_target: None,
        ops: Operations {
            load: if cleared {
                LoadOp::Clear(LinearRgba::WHITE.into())
            } else {
                LoadOp::Load
            },
            store: StoreOp::Store,
        },
        depth_slice: None,
    })];
    let mut pass = TrackedRenderPass::new(
        device,
        encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some(label),
            color_attachments: &attachments,
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(Operations {
                    load: if cleared {
                        LoadOp::Clear(super::backend::SHADOWMAP_CLEAR_Z)
                    } else {
                        LoadOp::Load
                    },
                    store: StoreOp::Store,
                }),
                stencil_ops: Some(Operations {
                    load: if cleared {
                        LoadOp::Clear(0)
                    } else {
                        LoadOp::Load
                    },
                    store: StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        }),
    );
    if sw > 0 && sh > 0 {
        pass.set_scissor_rect(sx, sy, sw, sh);
    }
    let mut record_n = RecordCensus {
        state: u32::from(sw > 0 && sh > 0),
        ..RecordCensus::default()
    };
    let mut bound_pipeline = None;
    let mut bound_tess = None;
    let mut bound_epoch = None;
    let mut bound_depth = None;
    let mut constants_bound = false;
    let mut indexed = 0u32;
    // A map standing in for a Minecraft world casts no shadows.
    for draw in draws.into_iter().filter(|draw| !draw.minecraft_hidden) {
        let Some(gpu_pipeline) = registry.ready(draw.pipeline) else {
            *miss = miss.saturating_add(1);
            *miss_rows.entry("PipelineNotReady".into()).or_default() += 1;
            continue;
        };
        let (Some(constant_base), Some(constants_bind)) = (draw.constant_base, constants_bind)
        else {
            *miss = miss.saturating_add(1);
            *miss_rows.entry("ConstantArenaMissing".into()).or_default() += 1;
            continue;
        };
        let (vertex, index) = match draw.tess {
            ExactTessBind::World => (geometry.world_vertex.as_ref(), world_index),
            ExactTessBind::Smodel => (
                smodel_vertex,
                smodel_index_epochs.get(draw.ring_epoch as usize),
            ),
            ExactTessBind::XModel => (
                xmodel_vertex,
                xmodel_index_epochs.get(draw.ring_epoch as usize),
            ),
            ExactTessBind::SmodelCached => (
                geometry.smodel_cached_vertex.as_ref(),
                geometry.smodel_cached_index.as_ref(),
            ),
            ExactTessBind::SmodelSkinned => (smodel_skinned_vertex, smodel_skinned_index),
            _ => {
                *miss = miss.saturating_add(1);
                *miss_rows.entry("UnsupportedTessKind".into()).or_default() += 1;
                continue;
            }
        };
        let (Some(vertex), Some(index)) = (vertex, index) else {
            *miss = miss.saturating_add(1);
            *miss_rows.entry("MissingTessBuffer".into()).or_default() += 1;
            continue;
        };
        if bound_pipeline != Some(draw.pipeline)
            || bound_tess != Some(draw.tess)
            || bound_epoch != Some(draw.ring_epoch)
        {
            pass.set_render_pipeline(gpu_pipeline);
            pass.set_vertex_buffer(0, vertex.slice(..));
            if let Some(lighting) =
                vertex_lighting_stream(draw.tess, geometry, smodel_skinned_vertex_lighting)
            {
                pass.set_vertex_buffer(1, lighting.slice(..));
            }
            if draw.tess == ExactTessBind::World
                && let Some(effect) = geometry.world_effect.as_ref()
            {
                pass.set_vertex_buffer(2, effect.slice(..));
            }
            record_n.state = record_n.state.saturating_add(2);
            if draw.tess == ExactTessBind::World
                && let Some(layer) = geometry.world_layer.as_ref()
            {
                pass.set_vertex_buffer(1, layer.slice(..));
                record_n.state = record_n.state.saturating_add(1);
            }
            pass.set_index_buffer(index.slice(..), IndexFormat::Uint32);
            record_n.state = record_n.state.saturating_add(1);
            bound_pipeline = Some(draw.pipeline);
            bound_tess = Some(draw.tess);
            bound_epoch = Some(draw.ring_epoch);
        }

        if bound_depth != Some((draw.depth_min, draw.depth_max)) {
            pass.set_viewport(
                vp.x as f32,
                vp.y as f32,
                vp.width as f32,
                vp.height as f32,
                draw.depth_min,
                draw.depth_max,
            );
            bound_depth = Some((draw.depth_min, draw.depth_max));
            record_n.state = record_n.state.saturating_add(1);
        }
        if !constants_bound {
            pass.set_bind_group(0, constants_bind, &[]);
            pass.set_bind_group(1, textures_bind, &[]);
            if let Some(minecraft) = minecraft {
                pass.set_bind_group(super::minecraft_light::MINECRAFT_GROUP, minecraft, &[]);
            }
            constants_bound = true;
            record_n.group0 = record_n.group0.saturating_add(1);
            record_n.group1 = record_n.group1.saturating_add(1);
        }
        pass.draw_indexed(
            draw.start..draw.start.saturating_add(draw.count),
            0,
            constant_base..constant_base.saturating_add(1),
        );
        indexed = indexed.saturating_add(1);
    }
    drop(pass);
    (indexed, record_n)
}

fn clear_empty_shadowmap_sun_partition_zero(
    encoder: &mut CommandEncoder,
    color_view: &bevy::render::render_resource::TextureView,
    depth_view: &bevy::render::render_resource::TextureView,
) {
    let attachments = [Some(RenderPassColorAttachment {
        view: color_view,
        resolve_target: None,
        ops: Operations {
            load: LoadOp::Clear(LinearRgba::WHITE.into()),
            store: StoreOp::Store,
        },
        depth_slice: None,
    })];
    let pass = encoder.begin_render_pass(&RenderPassDescriptor {
        label: Some("iw4_shadowmap_sun_partition_zero_empty"),
        color_attachments: &attachments,
        depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
            view: depth_view,
            depth_ops: Some(Operations {
                load: LoadOp::Clear(super::backend::SHADOWMAP_CLEAR_Z),
                store: StoreOp::Store,
            }),
            stencil_ops: Some(Operations {
                load: LoadOp::Clear(0),
                store: StoreOp::Store,
            }),
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    drop(pass);
}

#[derive(Default)]
struct SunShadowSubmit {
    gpu: u32,
    miss: u32,
    cause: Option<String>,
    causes: Option<String>,
    static_hit: u32,
    world_ib_n: u32,
    record_n: RecordCensus,

    emit_ms: f32,
    prepare_ms: f32,
    patch_ms: f32,
    arena_ms: f32,
    record_ms: f32,
    finish_ms: f32,
}

#[derive(Default)]
struct SpotShadowSubmit {
    gpu: u32,
    miss: u32,
    slots: u32,
    cause: Option<String>,
    record_n: RecordCensus,
}

struct PreparedSpotSlot {
    envelope: super::backend::SpotShadowMapPass,
    color_view: TextureView,
    depth_view: TextureView,
    smodel_index_epochs: Vec<Buffer>,
    xmodel_index_epochs: Vec<Buffer>,
    draw_range: std::ops::Range<usize>,
}

#[derive(Default)]
struct PreparedSpotWork {
    miss: u32,
    miss_rows: BTreeMap<String, u32>,
    all_prepared: Vec<PreparedExactDraw>,
    prepared_slots: Vec<PreparedSpotSlot>,
}

struct PreparedSunPartition {
    pi: usize,
    envelope: super::backend::SunShadowPartitionPass,
    xmodel_draws: Vec<PreparedExactDraw>,
    skinned_draws: Vec<PreparedExactDraw>,
}

#[derive(Default)]
struct PreparedSunWork {
    early: Option<SunShadowSubmit>,
    color_view: Option<TextureView>,
    depth_view: Option<TextureView>,
    clear_partition_zero: bool,
    partitions: Vec<PreparedSunPartition>,
    miss: u32,
    miss_rows: BTreeMap<String, u32>,
    any_flush: bool,
    smodel_reusable_all: bool,
    retry_n: u32,
    world_ib_n: u32,
    emit_ms: f32,
    prepare_ms: f32,
    patch_ms: f32,
    arena_ms: f32,
}

impl PreparedSunWork {
    fn done(submit: SunShadowSubmit) -> Self {
        Self {
            early: Some(submit),
            ..Self::default()
        }
    }
}

impl SunShadowSubmit {
    fn refused(cause: &str) -> Self {
        Self {
            miss: 1,
            cause: Some(format!("{cause}:1")),
            ..Self::default()
        }
    }
}

fn world_shadow_zone_span(start: u32, count: u32, src_index_n: usize) -> Option<(u32, u32)> {
    let end = (start as usize).saturating_add(count as usize);
    (end <= src_index_n).then_some((start, count))
}

fn prepare_shadowmap_spot(
    products: &ExtractedRenderFrameProducts,
    extracted: ExtractedColourRefs<'_>,
    geometry: &ExactColourGeometry,
    pipeline_res: &ExactColourPipeline,
    registry: &ExactPipelineRegistry,
    device: &RenderDevice,
    queue: &RenderQueue,
    uploaded: &RuntimeUploadedImageRegistry,
    binding_cache: &mut ExactShadowBindingCache,
    texture_table: &mut ExactTextureTable,
    shadow_arena: &mut ShadowmapSpotArena,
    shadowmap: &mut ShadowmapSpotGpu,
    sampler_table: &SamplerTable,
    shadow_exec: &mut ShadowExecScratch,
    skinned_tess: &mut smodel_skinned::SmodelSkinnedTess,
) -> PreparedSpotWork {
    let ShadowExecScratch {
        executor: shadow_exec_executor,
        ..
    } = shadow_exec;
    shadow_exec_executor.begin_list();
    let shadow_exec_view = exec_tables(extracted).map(|(catalog, prepared_table)| {
        MaterialExecView::camera(catalog, prepared_table, &extracted.frame.exec_frame)
    });
    let spot = products.0.product(FrameProductKind::SpotShadow);
    if spot.spot_slots.is_empty() {
        return PreparedSpotWork::default();
    }
    ensure_shadowmap_spot_targets(shadowmap, device);
    let mut slots: Vec<_> = spot.spot_slots.iter().collect();
    slots.sort_by_key(|slot| {
        (
            match slot.emitted.plan.render_target_id {
                lighting_iw4::GFX_SPOT_SHADOW_RT_SMALL => 0u8,
                lighting_iw4::GFX_SPOT_SHADOW_RT_LARGE => 1u8,
                _ => 2u8,
            },
            slot.emitted.slot_index,
        )
    });

    let mut prepare = ExactPrepare {
        extracted,
        geometry,
        pretess: None,
        pipeline_res,
        registry,
        device,
        uploaded,
        spot_shadow_select: None,
        sampler_table,
        textures: PrepareTextureTables::Shadow {
            slots: &mut binding_cache.textures,
            table: texture_table,
        },
        arena: None,
        run_pack: RunPackCache::default(),
        cost: PrepareCost::default(),
        skinned_tess: Some(skinned_tess),
        skinned_shared: None,
    };
    let target = ExactPrepareTarget {
        color: SHADOWMAP_SPOT_COLOR_FORMAT,
        samples: 1,
        depth: SHADOWMAP_SPOT_DEPTH_FORMAT,
        forward_z: true,
        use_world_pretess: false,
    };
    let mut miss = 0u32;
    let mut miss_rows = BTreeMap::<String, u32>::new();
    let mut overlay_intern = ShadowOverlayIntern::new();
    let mut all_prepared = Vec::new();
    let mut prepared_slots = Vec::new();

    for slot in slots {
        let emitted = slot.emitted;
        let Some(view_parms) = emitted.view_parms.as_ref() else {
            miss = miss.saturating_add(1);
            *miss_rows
                .entry("MissingSpotShadowViewParms".into())
                .or_default() += 1;
            continue;
        };
        let envelope = draw_spot_shadow_map(emitted.slot_index, emitted.plan, Some(&slot.packed));
        let Some(work) = envelope.work.as_ref() else {
            miss = miss.saturating_add(1);
            *miss_rows.entry("MissingSpotShadowWork".into()).or_default() += 1;
            continue;
        };
        let Some(color_view) = shadowmap.color_view(envelope.render_target_id).cloned() else {
            miss = miss.saturating_add(1);
            *miss_rows
                .entry("MissingSpotShadowColorTarget".into())
                .or_default() += 1;
            continue;
        };
        let Some(depth_view) = shadowmap.depth_view(envelope.render_target_id).cloned() else {
            miss = miss.saturating_add(1);
            *miss_rows
                .entry("MissingSpotShadowDepthTarget".into())
                .or_default() += 1;
            continue;
        };

        let mut flushes = Vec::<ShadowmapSunFlushGpu>::new();
        // Receivers sample every emitted slot, so a slot whose casters all
        // compacted away still clears its target to fully lit.
        let work = if spot.ordered_draws.is_empty() {
            &Default::default()
        } else {
            work
        };
        for flush in &work.world_flushes {
            let (start, count) = prim_args_u32_index_span(prim_args_from_world_flush(*flush));
            let Some((start, count)) =
                world_shadow_zone_span(start, count, geometry.world_cpu_indices.len())
            else {
                miss = miss.saturating_add(1);
                *miss_rows
                    .entry("WorldShadowSpanOutsideZoneIb".into())
                    .or_default() += 1;
                continue;
            };
            let owner_start = flush.entry_start as usize;
            let owner_end = owner_start.saturating_add(flush.entry_count as usize);
            let owners = slot
                .packed
                .world_draw_indices
                .get(owner_start..owner_end)
                .unwrap_or(&[]);
            match prepare.prepare_shadow_flush_draws(
                shadow_exec_executor,
                shadow_exec_view,
                SunFlush {
                    kind: SunFlushKind::World,
                    draw_start: start,
                    draw_count: count,
                    ring_epoch: 0,
                },
                owners,
                0,
                spot,
                target,
            ) {
                Ok(gpu) => flushes.push(gpu),
                Err(cause) => {
                    miss = miss.saturating_add(1);
                    *miss_rows.entry(cause).or_default() += 1;
                }
            }
        }

        let mut xmodel_stream = ModelIndexStream::new();
        let mut xmodel_spans = Vec::new();
        for flush in &work.xmodel_flushes {
            let owner_start = flush.entry_start as usize;
            let owner_end = owner_start.saturating_add(flush.entry_count as usize);
            let Some(entries) = slot.packed.xmodel.get(owner_start..owner_end) else {
                miss = miss.saturating_add(1);
                *miss_rows
                    .entry("XModelDynamicProvenanceMissing".into())
                    .or_default() += 1;
                continue;
            };
            for (entry_offset, entry) in entries.iter().enumerate() {
                let hit_start = entry.index_byte_offset / 2;
                let hit_count = u32::from(entry.tri_count).saturating_mul(3);
                match emit_local_xmodel_shadow_flush(
                    &extracted.frame.xmodel_indices,
                    hit_start,
                    hit_count,
                    &mut xmodel_stream,
                ) {
                    Ok((ring_epoch, draw_start, draw_count)) => xmodel_spans.push((
                        ring_epoch,
                        draw_start,
                        draw_count,
                        owner_start.saturating_add(entry_offset),
                    )),
                    Err(cause) => {
                        miss = miss.saturating_add(1);
                        *miss_rows.entry(cause).or_default() += 1;
                    }
                }
            }
        }
        let xmodel_epochs = xmodel_stream.finish();
        for (ring_epoch, draw_start, draw_count, owner) in xmodel_spans {
            let owners = slot
                .packed
                .xmodel_draw_indices
                .get(owner..owner + 1)
                .unwrap_or(&[]);
            match prepare.prepare_shadow_flush_draws(
                shadow_exec_executor,
                shadow_exec_view,
                SunFlush {
                    kind: SunFlushKind::XModel,
                    draw_start,
                    draw_count,
                    ring_epoch,
                },
                owners,
                0,
                spot,
                target,
            ) {
                Ok(gpu) => flushes.push(gpu),
                Err(cause) => {
                    miss = miss.saturating_add(1);
                    *miss_rows.entry(cause).or_default() += 1;
                }
            }
        }

        let mut smodel_stream = ModelIndexStream::new();
        let mut smodel_spans = Vec::new();
        for flush in &work.smodel_flushes {
            let owner_start = flush.entry_start as usize;
            let owner_end = owner_start.saturating_add(flush.entry_count as usize);
            let Some(entries) = slot.packed.smodel.get(owner_start..owner_end) else {
                miss = miss.saturating_add(1);
                *miss_rows
                    .entry("SmodelDynamicProvenanceMissing".into())
                    .or_default() += 1;
                continue;
            };
            for (entry_offset, entry) in entries.iter().enumerate() {
                let hit_start = entry.index_byte_offset / 2;
                let hit_count = u32::from(entry.tri_count).saturating_mul(3);
                match emit_local_smodel_shadow_flush(
                    extracted.world.static_geometry.smodel_indices.as_slice(),
                    hit_start,
                    hit_count,
                    &mut smodel_stream,
                ) {
                    Ok((ring_epoch, draw_start, draw_count)) => smodel_spans.push((
                        ring_epoch,
                        draw_start,
                        draw_count,
                        owner_start.saturating_add(entry_offset),
                    )),
                    Err(cause) => {
                        miss = miss.saturating_add(1);
                        *miss_rows.entry(cause).or_default() += 1;
                    }
                }
            }
        }
        let smodel_epochs = smodel_stream.finish();
        for (ring_epoch, draw_start, draw_count, owner) in smodel_spans {
            let owners = slot
                .packed
                .smodel_draw_indices
                .get(owner..owner + 1)
                .unwrap_or(&[]);
            match prepare.prepare_shadow_flush_draws(
                shadow_exec_executor,
                shadow_exec_view,
                SunFlush {
                    kind: SunFlushKind::Smodel,
                    draw_start,
                    draw_count,
                    ring_epoch,
                },
                owners,
                0,
                spot,
                target,
            ) {
                Ok(gpu) => flushes.push(gpu),
                Err(cause) => {
                    miss = miss.saturating_add(1);
                    *miss_rows.entry(cause).or_default() += 1;
                }
            }
        }

        let skinned_gpu = prepare_smodel_skinned_shadow_gpu(
            &mut prepare,
            shadow_exec_executor,
            shadow_exec_view,
            &slot.packed,
            &work.smodel_skinned_flushes,
            spot,
            0,
            target,
            &mut miss,
            &mut miss_rows,
        );
        flushes.extend(skinned_gpu);

        let start = all_prepared.len();
        for flush in &flushes {
            for (draw, code) in flush.draws.iter().zip(&flush.pass_code) {
                let overlay = shadowmap_spot_overlay(code, view_parms, flush.world_from_local);
                match overlay_shadow_draw_constants_with(
                    draw,
                    &overlay,
                    flush.world_from_local,
                    u8::try_from(emitted.slot_index).unwrap_or(u8::MAX),
                    &mut overlay_intern,
                ) {
                    Ok(draw) => all_prepared.push(draw),
                    Err(()) => {
                        miss = miss.saturating_add(1);
                        *miss_rows.entry("SpotOverlayRefused".into()).or_default() += 1;
                    }
                }
            }
        }
        let end = all_prepared.len();
        if start == end && !envelope.cleared {
            continue;
        }
        let smodel_index_epochs =
            shadowmap.upload_smodel_index_epochs(emitted.slot_index, device, queue, &smodel_epochs);
        let xmodel_index_epochs =
            shadowmap.upload_xmodel_index_epochs(emitted.slot_index, device, queue, &xmodel_epochs);
        prepared_slots.push(PreparedSpotSlot {
            envelope,
            color_view,
            depth_view,
            smodel_index_epochs,
            xmodel_index_epochs,
            draw_range: start..end,
        });
    }
    drop(prepare);

    if !all_prepared.is_empty() {
        let ShadowmapSpotArena { gpu, pack, .. } = shadow_arena;
        pack.begin_list();
        upload_constant_arena(
            &mut all_prepared,
            pack,
            gpu,
            pipeline_res,
            registry,
            device,
            queue,
            "iw4_shadowmap_spot_vs_arena",
            "iw4_shadowmap_spot_ps_arena",
        );
    }
    PreparedSpotWork {
        miss,
        miss_rows,
        all_prepared,
        prepared_slots,
    }
}

fn record_shadowmap_spot(
    mut work: PreparedSpotWork,
    pipeline_res: &ExactColourPipeline,
    registry: &ExactPipelineRegistry,
    geometry: &ExactColourGeometry,
    device: &RenderDevice,
    texture_table: &mut ExactTextureTable,
    shadow_arena: &ShadowmapSpotArena,
    context: &mut RenderContext,
    smodel_skinned_vertex: Option<&Buffer>,
    smodel_skinned_vertex_lighting: Option<&Buffer>,
    smodel_skinned_index: Option<&Buffer>,
    minecraft: Option<&BindGroup>,
) -> SpotShadowSubmit {
    if work.prepared_slots.is_empty() {
        return SpotShadowSubmit {
            miss: work.miss,
            cause: rank_count_map(&work.miss_rows, 1),
            ..SpotShadowSubmit::default()
        };
    }
    let Some(table_layout) = texture_table_layout(pipeline_res) else {
        return SpotShadowSubmit {
            miss: 1,
            cause: Some("TextureTableLayoutMissing:1".into()),
            ..SpotShadowSubmit::default()
        };
    };
    let table_binds = texture_table.binds(device, registry, table_layout);
    let diagnostics = context.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let encoder = context.command_encoder();
    let gpu_span = diagnostics.time_span(encoder, GPU_SPAN_SPOT);
    let mut indexed = 0u32;
    let mut record_n = RecordCensus::default();
    let world_index = geometry.world_index.as_ref();
    for slot in &work.prepared_slots {
        let draws = &mut work.all_prepared[slot.draw_range.clone()];
        let live = coalesce_shadow_draws_prefix(draws);
        let (run_indexed, run_binds) = record_shadowmap_draws(
            encoder,
            device,
            registry,
            geometry,
            world_index,
            &slot.smodel_index_epochs,
            &slot.xmodel_index_epochs,
            geometry.smodel_vertex.as_ref(),
            geometry.xmodel.vertex.buffer(),
            smodel_skinned_vertex,
            smodel_skinned_vertex_lighting,
            smodel_skinned_index,
            shadow_arena.gpu.bind_group.as_ref(),
            &table_binds.scene,
            &draws[..live],
            &slot.color_view,
            &slot.depth_view,
            slot.envelope.scissor,
            slot.envelope.viewport,
            slot.envelope.cleared,
            "iw4_shadowmap_spot_slot",
            &mut work.miss,
            &mut work.miss_rows,
            minecraft,
        );
        indexed = indexed.saturating_add(run_indexed);
        record_n.add(run_binds);
    }
    gpu_span.end(encoder);
    SpotShadowSubmit {
        gpu: indexed,
        miss: work.miss,
        slots: work.prepared_slots.len() as u32,
        cause: rank_count_map(&work.miss_rows, 1),
        record_n,
    }
}

fn sun_model_index_span(
    source_len: usize,
    start: u32,
    count: u32,
) -> Result<(u32, u32, u32), String> {
    if count == 0
        || !count.is_multiple_of(3)
        || (start as usize)
            .checked_add(count as usize)
            .is_none_or(|end| end > source_len)
    {
        return Err("SunModelIndexSourceMissing".into());
    }
    Ok((0, start, count))
}

#[derive(Clone, Copy)]
struct SmodelShadowSpan {
    draw_start: u32,
    draw_count: u32,
    ring_epoch: u32,
    entry_start: u32,
    entry_count: u32,
}

fn prepare_shadowmap_sun(
    products: &ExtractedRenderFrameProducts,
    extracted: ExtractedColourRefs<'_>,
    geometry: &ExactColourGeometry,
    pipeline_res: &ExactColourPipeline,
    registry: &ExactPipelineRegistry,
    device: &RenderDevice,
    queue: &RenderQueue,
    uploaded: &RuntimeUploadedImageRegistry,
    binding_cache: &mut ExactShadowBindingCache,
    texture_table: &mut ExactTextureTable,
    shadow_arena: &mut ShadowmapSunArena,
    shadowmap: &mut ShadowmapSunGpu,
    static_draws: &mut ResidentShadowStaticDraws,
    sampler_table: &SamplerTable,
    shadow_exec: &mut ShadowExecScratch,
    skinned_tess: &mut smodel_skinned::SmodelSkinnedTess,
) -> PreparedSunWork {
    let sun = products.0.product(FrameProductKind::SunShadow);
    if sun.ordered_draws.is_empty() {
        return PreparedSunWork::done(SunShadowSubmit::default());
    }
    let (color_view, resized) = ensure_shadowmap_sun_target(shadowmap, device);
    if resized {
        binding_cache.clear_textures();
    }
    let Some(color_view) = color_view else {
        return PreparedSunWork::done(SunShadowSubmit::refused("ShadowmapSunTargetMissing"));
    };
    let Some(depth_view) = shadowmap.depth_view().cloned() else {
        return PreparedSunWork::done(SunShadowSubmit::refused("ShadowmapSunDepthMissing"));
    };
    if draw_sun_shadow_map_forced(0, None).is_none() {
        return PreparedSunWork::done(SunShadowSubmit::refused("ShadowmapSunPartitionMissing"));
    }
    let generation = extracted.world.generation.get();

    let ShadowExecScratch {
        executor: shadow_exec_executor,
        code_sources: shadow_code_sources,
        pack_draws,
    } = shadow_exec;
    shadow_exec_executor.begin_list();
    shadow_code_sources.clone_from(&extracted.frame.exec_frame.code_sources);
    if let Some(sun_frame) = extracted.frame.sun_shadow.as_ref() {
        shadow_code_sources.set_constant(
            super::CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0,
            super::code_transpose_matrix_rows(sun_frame.partitions[0].clip_from_world),
        );
    }
    let shadow_exec_view = extracted.frame.sun_shadow.as_ref().and_then(|sun_frame| {
        let (catalog, prepared_table) = exec_tables(extracted)?;
        Some(MaterialExecView::shadow_partition(
            catalog,
            prepared_table,
            &extracted.frame.exec_frame,
            sun_frame.partitions[0].clip_from_world,
            sun_frame.partitions[0].view,
            shadow_code_sources,
        ))
    });
    let mut prepare = ExactPrepare {
        extracted,
        geometry,
        pretess: None,
        pipeline_res,
        registry,
        device,
        uploaded,
        spot_shadow_select: None,
        sampler_table,
        textures: PrepareTextureTables::Shadow {
            slots: &mut binding_cache.textures,
            table: texture_table,
        },
        arena: None,
        run_pack: RunPackCache::default(),
        cost: PrepareCost::default(),
        skinned_tess: Some(skinned_tess),
        skinned_shared: None,
    };
    let mut miss = 0u32;
    let mut miss_rows = BTreeMap::<String, u32>::new();
    let world_ib_n = 0u32;
    let mut retry_n = 0u32;
    if texture_table_layout(pipeline_res).is_none() {
        return PreparedSunWork::done(SunShadowSubmit::refused("TextureTableLayoutMissing"));
    }
    let mut smodel_reusable_all = true;
    let mut any_flush = false;
    let mut clear_partition_zero = false;
    let mut partitions = Vec::new();
    let mut ms_emit = 0.0f32;
    let mut ms_prepare = 0.0f32;
    let mut ms_patch = 0.0f32;
    let mut ms_arena = 0.0f32;
    let near_n = sun.sun_near_n.min(sun.ordered_draws.len());
    for partition in 0..super::SUN_SHADOW_PARTITION_COUNT {
        let pi = partition as usize;
        let draws = if pi == 0 {
            &sun.ordered_draws[..near_n]
        } else {
            &sun.ordered_draws[near_n..]
        };
        if draws.is_empty() {
            if pi == 0 {
                clear_partition_zero = true;
            }
            continue;
        }
        let packed_fallback;
        let packed = match &sun.sun_packed {
            Some(lists) => &lists[pi],
            None => {
                packed_fallback = super::backend::pack_sun_shadow_frontend(
                    draws,
                    &products.0.world_run_surfs,
                    &geometry.world_surface_ranges,
                    u32::try_from(geometry.world_vertex_count).unwrap_or(u32::MAX),
                    &geometry.smodel_surface_ranges,
                    &geometry.xmodel_surface_ranges,
                    &mut *pack_draws,
                );
                &packed_fallback
            }
        };
        let Some(envelope) = draw_sun_shadow_map_forced(partition, Some(&packed)) else {
            continue;
        };
        let Some(work) = envelope.work.as_ref() else {
            continue;
        };

        let cached_absent = packed.smodel_cached.len() + packed.smodel_pretess.len();
        if cached_absent > 0 {
            let n = u32::try_from(cached_absent).unwrap_or(u32::MAX);
            miss = miss.saturating_add(n);
            *miss_rows
                .entry("SmodelCachedStreamAbsentFromSunAtlas".into())
                .or_default() += n;
        }
        any_flush = true;
        let smodel_identity_changed = static_draws.reset_unless_answering(
            generation,
            pi,
            work.world_flushes.len(),
            packed.smodel.len(),
            draws,
            packed,
        );
        let emit_started = Instant::now();
        let mut xmodel_spans = Vec::with_capacity(packed.xmodel.len());
        for flush in &work.xmodel_flushes {
            let owner_start = flush.entry_start as usize;
            let owner_end = owner_start.saturating_add(flush.entry_count as usize);
            let Some(entries) = packed.xmodel.get(owner_start..owner_end) else {
                miss = miss.saturating_add(1);
                *miss_rows
                    .entry("XModelDynamicProvenanceMissing".into())
                    .or_default() += 1;
                continue;
            };
            if packed
                .xmodel_draw_indices
                .get(owner_start..owner_end)
                .is_none()
            {
                miss = miss.saturating_add(1);
                *miss_rows
                    .entry("XModelDynamicProvenanceMissing".into())
                    .or_default() += 1;
                continue;
            }
            for (entry_offset, entry) in entries.iter().enumerate() {
                let hit_start = entry.index_byte_offset / 2;
                let hit_count = u32::from(entry.tri_count).saturating_mul(3);
                match sun_model_index_span(
                    extracted.frame.xmodel_indices.len(),
                    hit_start,
                    hit_count,
                ) {
                    Ok((ring_epoch, draw_start, draw_count)) => {
                        xmodel_spans.push((
                            draw_start,
                            draw_count,
                            ring_epoch,
                            u32::try_from(owner_start.saturating_add(entry_offset))
                                .unwrap_or(u32::MAX),
                            1,
                        ));
                    }
                    Err(cause) => {
                        miss = miss.saturating_add(1);
                        *miss_rows.entry(cause).or_default() += 1;
                    }
                }
            }
        }
        smodel_reusable_all &= !smodel_identity_changed;
        let mut smodel_spans = Vec::with_capacity(packed.smodel.len());
        for flush in &work.smodel_flushes {
            let owner_start = flush.entry_start as usize;
            let owner_end = owner_start.saturating_add(flush.entry_count as usize);
            let Some(entries) = packed.smodel.get(owner_start..owner_end) else {
                smodel_spans.extend((owner_start..owner_end).map(|_| None));
                *miss_rows
                    .entry("SmodelDynamicProvenanceMissing".into())
                    .or_default() += 1;
                continue;
            };
            if packed
                .smodel_draw_indices
                .get(owner_start..owner_end)
                .is_none()
            {
                smodel_spans.extend(entries.iter().map(|_| None));
                *miss_rows
                    .entry("SmodelDynamicProvenanceMissing".into())
                    .or_default() += 1;
                continue;
            }
            for (entry_offset, entry) in entries.iter().enumerate() {
                let hit_start = entry.index_byte_offset / 2;
                let hit_count = u32::from(entry.tri_count).saturating_mul(3);
                match sun_model_index_span(
                    extracted.world.static_geometry.smodel_indices.len(),
                    hit_start,
                    hit_count,
                ) {
                    Ok((ring_epoch, draw_start, draw_count)) => {
                        smodel_spans.push(Some(SmodelShadowSpan {
                            draw_start,
                            draw_count,
                            ring_epoch,
                            entry_start: u32::try_from(owner_start.saturating_add(entry_offset))
                                .unwrap_or(u32::MAX),
                            entry_count: 1,
                        }));
                    }
                    Err(cause) => {
                        smodel_spans.push(None);
                        *miss_rows.entry(cause).or_default() += 1;
                    }
                }
            }
        }
        ms_emit += emit_started.elapsed().as_secs_f32() * 1000.0;
        let prepare_started = Instant::now();
        let src_index_n = geometry.world_cpu_indices.len();
        let world_spans: Vec<Option<(u32, u32)>> = work
            .world_flushes
            .iter()
            .map(|flush| {
                let (start, count) = prim_args_u32_index_span(prim_args_from_world_flush(*flush));
                world_shadow_zone_span(start, count, src_index_n)
            })
            .collect();
        let world_span_holes = as_u32(world_spans.iter().filter(|span| span.is_none()).count());
        let smodel_span_holes = as_u32(smodel_spans.iter().filter(|span| span.is_none()).count());
        if world_span_holes > 0 {
            miss = miss.saturating_add(world_span_holes);
            *miss_rows
                .entry("WorldShadowSpanOutsideZoneIb".into())
                .or_default() += world_span_holes;
        }
        if smodel_span_holes > 0 {
            miss = miss.saturating_add(smodel_span_holes);
            *miss_rows
                .entry("SmodelShadowStreamRefused".into())
                .or_default() += smodel_span_holes;
        }
        let retry_before = retry_n;
        for slot_i in 0..static_draws.world[pi].len() {
            if static_draws.world[pi][slot_i].is_some() {
                continue;
            }
            let Some((src_start, count)) = world_spans[slot_i] else {
                continue;
            };
            let flush = work.world_flushes[slot_i];
            let owner_start = flush.entry_start as usize;
            let owner_end = owner_start.saturating_add(flush.entry_count as usize);
            let owners = packed
                .world_draw_indices
                .get(owner_start..owner_end)
                .unwrap_or(&[]);
            retry_n = retry_n.saturating_add(1);
            match prepare.prepare_shadow_flush_draws(
                shadow_exec_executor,
                shadow_exec_view,
                SunFlush {
                    kind: SunFlushKind::World,
                    draw_start: src_start,
                    draw_count: count,
                    ring_epoch: 0,
                },
                owners,
                if pi == 0 { 0 } else { near_n },
                sun,
                ExactPrepareTarget {
                    color: SHADOWMAP_SUN_COLOR_FORMAT,
                    samples: 1,
                    depth: SHADOWMAP_SUN_DEPTH_FORMAT,
                    forward_z: true,
                    use_world_pretess: false,
                },
            ) {
                Ok(gpu) => static_draws.world[pi][slot_i] = Some(gpu),
                Err(cause) => {
                    miss = miss.saturating_add(1);
                    *miss_rows.entry(cause).or_default() += 1;
                }
            }
        }
        for slot_i in 0..static_draws.smodel[pi].len() {
            if static_draws.smodel[pi][slot_i].is_some() {
                continue;
            }
            let Some(span) = smodel_spans[slot_i] else {
                continue;
            };
            let owner_start = span.entry_start as usize;
            let owner_end = owner_start.saturating_add(span.entry_count as usize);
            let owners = packed
                .smodel_draw_indices
                .get(owner_start..owner_end)
                .unwrap_or(&[]);
            retry_n = retry_n.saturating_add(1);
            match prepare.prepare_shadow_flush_draws(
                shadow_exec_executor,
                shadow_exec_view,
                SunFlush {
                    kind: SunFlushKind::Smodel,
                    draw_start: span.draw_start,
                    draw_count: span.draw_count,
                    ring_epoch: span.ring_epoch,
                },
                owners,
                if pi == 0 { 0 } else { near_n },
                sun,
                ExactPrepareTarget {
                    color: SHADOWMAP_SUN_COLOR_FORMAT,
                    samples: 1,
                    depth: SHADOWMAP_SUN_DEPTH_FORMAT,
                    forward_z: true,
                    use_world_pretess: false,
                },
            ) {
                Ok(gpu) => static_draws.smodel[pi][slot_i] = Some(gpu),
                Err(cause) => {
                    miss = miss.saturating_add(1);
                    *miss_rows.entry(cause).or_default() += 1;
                }
            }
        }
        let mut xmodel_gpu = Vec::with_capacity(xmodel_spans.len());
        for (draw_start, draw_count, ring_epoch, entry_start, entry_count) in xmodel_spans {
            let start = entry_start as usize;
            let end = start.saturating_add(entry_count as usize);
            let owners = packed.xmodel_draw_indices.get(start..end).unwrap_or(&[]);
            match prepare.prepare_shadow_flush_draws(
                shadow_exec_executor,
                shadow_exec_view,
                SunFlush {
                    kind: SunFlushKind::XModel,
                    draw_start,
                    draw_count,
                    ring_epoch,
                },
                owners,
                if pi == 0 { 0 } else { near_n },
                sun,
                ExactPrepareTarget {
                    color: SHADOWMAP_SUN_COLOR_FORMAT,
                    samples: 1,
                    depth: SHADOWMAP_SUN_DEPTH_FORMAT,
                    forward_z: true,
                    use_world_pretess: false,
                },
            ) {
                Ok(gpu) => xmodel_gpu.push(gpu),
                Err(cause) => {
                    miss = miss.saturating_add(1);
                    *miss_rows.entry(cause).or_default() += 1;
                }
            }
        }
        let skinned_gpu = prepare_smodel_skinned_shadow_gpu(
            &mut prepare,
            shadow_exec_executor,
            shadow_exec_view,
            packed,
            &work.smodel_skinned_flushes,
            sun,
            if pi == 0 { 0 } else { near_n },
            ExactPrepareTarget {
                color: SHADOWMAP_SUN_COLOR_FORMAT,
                samples: 1,
                depth: SHADOWMAP_SUN_DEPTH_FORMAT,
                forward_z: true,
                use_world_pretess: false,
            },
            &mut miss,
            &mut miss_rows,
        );
        ms_prepare += prepare_started.elapsed().as_secs_f32() * 1000.0;
        let patch_started = Instant::now();
        let ResidentShadowStaticDraws {
            world: resident_world,
            smodel: resident_smodel,
            commands,
            ..
        } = &mut *static_draws;
        let plan = &mut commands[pi];
        plan.open_frame();

        if plan.generation != Some(generation)
            || plan.world_n != resident_world[pi].len()
            || plan.smodel_n != resident_smodel[pi].len()
            || retry_n != retry_before
            || smodel_identity_changed
        {
            plan.rebuild(generation, &resident_world[pi], &resident_smodel[pi]);
        }
        if plan.refused > 0 {
            miss = miss.saturating_add(plan.refused);
            *miss_rows.entry(sun_overlay_refusal(pi).into()).or_default() += plan.refused;
        }
        let refused_before = plan.refused;
        let mut xmodel_draws = Vec::with_capacity(xmodel_gpu.len());
        let mut xmodel_seen = HashMap::new();
        for flush in &xmodel_gpu {
            plan.push_flush(flush, &mut xmodel_seen, &mut xmodel_draws);
        }
        let dynamic_refused = plan.refused.saturating_sub(refused_before);
        plan.refused = refused_before;
        if dynamic_refused > 0 {
            miss = miss.saturating_add(dynamic_refused);
            *miss_rows.entry(sun_overlay_refusal(pi).into()).or_default() += dynamic_refused;
        }
        coalesce_shadow_draws(&mut xmodel_draws);
        let mut skinned_draws = Vec::with_capacity(skinned_gpu.len());
        let mut skinned_seen = HashMap::new();
        for flush in &skinned_gpu {
            plan.push_flush(flush, &mut skinned_seen, &mut skinned_draws);
        }
        coalesce_shadow_draws(&mut skinned_draws);
        if plan.world_draws.is_empty()
            && plan.smodel_draws.is_empty()
            && xmodel_draws.is_empty()
            && skinned_draws.is_empty()
        {
            continue;
        }

        let Some(frame) = extracted.frame.sun_shadow else {
            miss = miss.saturating_add(1);
            *miss_rows.entry("MissingSunShadowFrame".into()).or_default() += 1;
            continue;
        };
        plan.patch_sun_registers(&frame.partitions[pi]);
        ms_patch += patch_started.elapsed().as_secs_f32() * 1000.0;
        let arena_started = Instant::now();
        upload_packed_arena(
            &plan.bytes,
            &[],
            plan.bytes.len(),
            0,
            &plan.dirty,
            false,
            plan.world_draws
                .iter()
                .chain(xmodel_draws.iter())
                .chain(skinned_draws.iter())
                .chain(plan.smodel_draws.iter()),
            &mut shadow_arena.gpu[pi],
            pipeline_res,
            registry,
            device,
            queue,
            "iw4_shadowmap_sun_vs_arena",
            "iw4_shadowmap_sun_ps_arena",
        );
        ms_arena += arena_started.elapsed().as_secs_f32() * 1000.0;
        partitions.push(PreparedSunPartition {
            pi,
            envelope,
            xmodel_draws,
            skinned_draws,
        });
    }
    drop(prepare);
    PreparedSunWork {
        early: None,
        color_view: Some(color_view),
        depth_view: Some(depth_view),
        clear_partition_zero,
        partitions,
        miss,
        miss_rows,
        any_flush,
        smodel_reusable_all,
        retry_n,
        world_ib_n,
        emit_ms: ms_emit,
        prepare_ms: ms_prepare,
        patch_ms: ms_patch,
        arena_ms: ms_arena,
    }
}

fn finish_sun_submit(
    work: &PreparedSunWork,
    gpu: u32,
    record_n: RecordCensus,
    record_ms: f32,
) -> SunShadowSubmit {
    let static_hit = u32::from(work.any_flush && work.smodel_reusable_all && work.retry_n == 0);
    SunShadowSubmit {
        gpu,
        emit_ms: work.emit_ms,
        prepare_ms: work.prepare_ms,
        patch_ms: work.patch_ms,
        arena_ms: work.arena_ms,
        record_ms,
        finish_ms: 0.0,
        miss: work.miss,
        cause: rank_count_map(&work.miss_rows, 1),
        causes: rank_count_map(&work.miss_rows, 6),
        static_hit,
        world_ib_n: work.world_ib_n,
        record_n,
    }
}

fn record_shadowmap_sun(
    mut work: PreparedSunWork,
    pipeline_res: &ExactColourPipeline,
    registry: &ExactPipelineRegistry,
    geometry: &ExactColourGeometry,
    device: &RenderDevice,
    texture_table: &mut ExactTextureTable,
    shadow_arena: &ShadowmapSunArena,
    static_draws: &ResidentShadowStaticDraws,
    context: &mut RenderContext,
    smodel_skinned_vertex: Option<&Buffer>,
    smodel_skinned_vertex_lighting: Option<&Buffer>,
    smodel_skinned_index: Option<&Buffer>,
    minecraft: Option<&BindGroup>,
) -> SunShadowSubmit {
    if let Some(submit) = work.early.take() {
        return submit;
    }
    let Some(color_view) = work.color_view.take() else {
        return SunShadowSubmit::refused("ShadowmapSunTargetMissing");
    };
    let Some(depth_view) = work.depth_view.take() else {
        return SunShadowSubmit::refused("ShadowmapSunDepthMissing");
    };
    if !work.clear_partition_zero && work.partitions.is_empty() {
        return finish_sun_submit(&work, 0, RecordCensus::default(), 0.0);
    }
    let diagnostics = context.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let encoder = context.command_encoder();
    let gpu_span = diagnostics.time_span(encoder, GPU_SPAN_SUN);
    if work.clear_partition_zero {
        clear_empty_shadowmap_sun_partition_zero(encoder, &color_view, &depth_view);
    }
    let mut indexed = 0u32;
    let mut record_n = RecordCensus::default();
    let record_started = Instant::now();
    if !work.partitions.is_empty() {
        let Some(table_layout) = texture_table_layout(pipeline_res) else {
            gpu_span.end(encoder);
            return SunShadowSubmit::refused("TextureTableLayoutMissing");
        };
        let table_binds = texture_table.binds(device, registry, table_layout);
        let world_index = geometry.world_index.as_ref();
        for part in &work.partitions {
            let plan = &static_draws.commands[part.pi];
            let (run_indexed, run_binds) = record_shadowmap_draws(
                encoder,
                device,
                registry,
                geometry,
                world_index,
                geometry.smodel_index.as_slice(),
                geometry
                    .xmodel
                    .index
                    .buffer()
                    .map(std::slice::from_ref)
                    .unwrap_or_default(),
                geometry.smodel_vertex.as_ref(),
                geometry.xmodel.vertex.buffer(),
                smodel_skinned_vertex,
                smodel_skinned_vertex_lighting,
                smodel_skinned_index,
                shadow_arena.gpu[part.pi].bind_group.as_ref(),
                &table_binds.sun_caster,
                plan.world_draws
                    .iter()
                    .chain(part.xmodel_draws.iter())
                    .chain(part.skinned_draws.iter())
                    .chain(plan.smodel_draws.iter()),
                &color_view,
                &depth_view,
                part.envelope.scissor,
                part.envelope.viewport,
                part.envelope.cleared,
                "iw4_shadowmap_sun_partition",
                &mut work.miss,
                &mut work.miss_rows,
                minecraft,
            );
            indexed = indexed.saturating_add(run_indexed);
            record_n.add(run_binds);
        }
    }
    let ms_record = record_started.elapsed().as_secs_f32() * 1000.0;
    gpu_span.end(encoder);
    if indexed == 0 {
        return finish_sun_submit(&work, 0, RecordCensus::default(), ms_record);
    }

    finish_sun_submit(&work, indexed, record_n, ms_record)
}

fn append_with_rollback<T, E>(
    out: &mut Vec<T>,
    write: impl FnOnce(&mut Vec<T>) -> Result<(), E>,
) -> Result<usize, E> {
    let start = out.len();
    match write(out) {
        Ok(()) => Ok(out.len().saturating_sub(start)),
        Err(cause) => {
            out.truncate(start);
            Err(cause)
        }
    }
}

impl ExactPrepare<'_> {
    fn prepare_ready_hit(
        &mut self,
        item: &RetainedDrawItem,
        execution: &MaterialExecution,
        run_serial: u64,
        place_code: Option<PlaceLanes<'_>>,
        surface: super::SurfaceSamplerInputs,
        target: ExactPrepareTarget,
        binds_sun_shadow: bool,
        binds_spot_shadow: bool,
        out: &mut Vec<PreparedExactDraw>,
    ) -> Result<usize, GpuSubmitRefusal> {
        let after_scene_resolve = matches!(self.textures, PrepareTextureTables::SceneShared(_))
            && (item.camera_region == Some(asset_iw4::CAMERA_REGION_EMISSIVE)
                || matches!(item.kind, RetainedDrawKind::CodeMesh { .. })
                || matches!(item.kind, RetainedDrawKind::Glass { .. })
                || matches!(item.kind, RetainedDrawKind::MarkMesh { glass: true, .. })
                || execution_binds_code_texture(execution, CODE_TEXTURE_RESOLVED_POST_SUN)
                || execution_binds_code_texture(execution, CODE_TEXTURE_FLOATZ));
        let kind = item.kind;
        let key = item.key;
        let (tess, start, count, vertex_type) = match kind {
            RetainedDrawKind::World { surf, .. } => {
                if let Some(cause) = self.extracted.world.static_geometry.world_vertex_refusal {
                    return Err(GpuSubmitRefusal::WorldVertex(cause));
                }
                let &(start, count) = world_submit_ranges_for_pass(
                    self.geometry,
                    self.pretess,
                    target.use_world_pretess,
                )
                .get(usize::from(surf))
                .ok_or(GpuSubmitRefusal::MissingSurfaceRange { surf })?;
                if count == 0 {
                    return Err(GpuSubmitRefusal::EmptyIndexRange { surf });
                }
                if self.geometry.world_vertex.is_none() || self.geometry.world_index.is_none() {
                    return Err(GpuSubmitRefusal::WorldVertex(
                        render_frame::WorldVertexRefusal::ForeignLayout {
                            source_layout: "exact colour world buffers absent",
                        },
                    ));
                }
                (ExactTessBind::World, start, count, execution.vertex_type)
            }
            RetainedDrawKind::Smodel {
                surface,
                placement,
                stream,
                cache_index,
                pretess,
                world_from_local,
                ..
            } => match stream {
                None => return Err(GpuSubmitRefusal::SmodelXSurfacePathUnread { placement }),
                Some(lighting_iw4::SmodelSurfPath::Cached) => {
                    if let Some(range) = pretess {
                        smodel_pretess_submit_refusal(
                            self.extracted,
                            placement,
                            cache_index
                                .ok_or(GpuSubmitRefusal::SmodelCacheIndexEmpty { placement })?,
                            range,
                        )?
                    } else {
                        return Err(GpuSubmitRefusal::SmodelCachedWithoutDestRange { placement });
                    }
                }
                Some(lighting_iw4::SmodelSurfPath::Pretess) => {
                    let cache_index =
                        cache_index.ok_or(GpuSubmitRefusal::SmodelCacheIndexEmpty { placement })?;
                    let pretess = pretess
                        .ok_or(GpuSubmitRefusal::SmodelCacheIndicesMissing { cache_index })?;
                    smodel_pretess_submit_refusal(self.extracted, placement, cache_index, pretess)?
                }

                Some(lighting_iw4::SmodelSurfPath::Rigid) => {
                    if let Some(cause) = self.extracted.world.static_geometry.smodel_vertex_refusal
                    {
                        return Err(GpuSubmitRefusal::PackedVertex(cause));
                    }
                    let &(start, count) = self
                        .geometry
                        .smodel_surface_ranges
                        .get(surface as usize)
                        .ok_or(GpuSubmitRefusal::MissingSmodelRange { surface })?;
                    if count == 0 {
                        return Err(GpuSubmitRefusal::EmptySmodelIndexRange { surface });
                    }
                    if self.geometry.smodel_vertex.is_none() || self.geometry.smodel_index.is_none()
                    {
                        return Err(GpuSubmitRefusal::PackedVertex(
                            render_frame::PackedVertexRefusal::ForeignLayout {
                                source_layout: "exact colour smodel buffers absent",
                            },
                        ));
                    }
                    (
                        ExactTessBind::Smodel,
                        start,
                        count,
                        asset_iw4::vertex_decl::PACKED_VERTEX_TYPE,
                    )
                }
                Some(lighting_iw4::SmodelSurfPath::Skinned) => {
                    let (start, count) = match (self.skinned_tess.as_mut(), self.skinned_shared) {
                        (Some(tess), _) => {
                            tess.append_draw(self.extracted, placement, surface, world_from_local)?
                        }
                        (None, Some(shared)) => shared
                            .lock()
                            .expect("skinned static model cache is never poisoned")
                            .append_draw(self.extracted, placement, surface, world_from_local)?,
                        (None, None) => {
                            return Err(GpuSubmitRefusal::SmodelSkinnedDestMissing { placement });
                        }
                    };
                    if count == 0 {
                        return Err(GpuSubmitRefusal::EmptySmodelIndexRange { surface });
                    }
                    (
                        ExactTessBind::SmodelSkinned,
                        start,
                        count,
                        asset_iw4::vertex_decl::PACKED_VERTEX_TYPE,
                    )
                }
            },
            RetainedDrawKind::XModel { surface, .. } => {
                if let Some(cause) = self.extracted.frame.xmodel_vertex_refusal {
                    return Err(GpuSubmitRefusal::PackedVertex(cause));
                }
                let &(start, count) = self
                    .geometry
                    .xmodel_surface_ranges
                    .get(surface as usize)
                    .ok_or(GpuSubmitRefusal::MissingXModelRange { surface })?;
                if count == 0 {
                    return Err(GpuSubmitRefusal::EmptyXModelIndexRange { surface });
                }
                if !self.geometry.xmodel.drawable() {
                    return Err(GpuSubmitRefusal::PackedVertex(
                        render_frame::PackedVertexRefusal::ForeignLayout {
                            source_layout: "exact colour xmodel buffers absent",
                        },
                    ));
                }
                (
                    ExactTessBind::XModel,
                    start,
                    count,
                    asset_iw4::vertex_decl::PACKED_VERTEX_TYPE,
                )
            }
            RetainedDrawKind::CodeMesh { draw, .. } => {
                if let Some(cause) = self.extracted.frame.fx_vertex_refusal {
                    return Err(GpuSubmitRefusal::PackedVertex(cause));
                }
                let &(start, count) = self
                    .geometry
                    .fx_surface_ranges
                    .get(draw as usize)
                    .ok_or(GpuSubmitRefusal::MissingCodeMeshRange { draw })?;
                if count == 0 {
                    return Err(GpuSubmitRefusal::EmptyCodeMeshIndexRange { draw });
                }
                if self.geometry.fx_vertex.is_none() || self.geometry.fx_index.is_none() {
                    return Err(GpuSubmitRefusal::PackedVertex(
                        render_frame::PackedVertexRefusal::ForeignLayout {
                            source_layout: "exact colour code-mesh buffers absent",
                        },
                    ));
                }
                (
                    ExactTessBind::CodeMesh,
                    start,
                    count,
                    asset_iw4::vertex_decl::PACKED_VERTEX_TYPE,
                )
            }
            RetainedDrawKind::ParticleCloud { draw, .. } => {
                let &(start, count) = self
                    .geometry
                    .particle_cloud_surface_ranges
                    .get(draw as usize)
                    .ok_or(GpuSubmitRefusal::MissingParticleCloudRange { draw })?;
                if count == 0 {
                    return Err(GpuSubmitRefusal::EmptyParticleCloudIndexRange { draw });
                }
                if !self.geometry.particle_cloud.drawable() {
                    return Err(GpuSubmitRefusal::PackedVertex(
                        render_frame::PackedVertexRefusal::ForeignLayout {
                            source_layout: "exact colour particle-cloud buffers absent",
                        },
                    ));
                }
                (
                    ExactTessBind::ParticleCloud,
                    start,
                    count,
                    asset_iw4::vertex_decl::POS_TEX_VERTEX_TYPE,
                )
            }
            RetainedDrawKind::MarkMesh { draw, packed, .. } => {
                let &(start, count) = self
                    .geometry
                    .mark_mesh_surface_ranges
                    .get(draw as usize)
                    .ok_or(GpuSubmitRefusal::MissingMarkMeshRange { draw })?;
                if count == 0 {
                    return Err(GpuSubmitRefusal::EmptyMarkMeshIndexRange { draw });
                }
                if !self.geometry.mark_mesh.drawable() {
                    return Err(GpuSubmitRefusal::WorldVertex(
                        render_frame::WorldVertexRefusal::ForeignLayout {
                            source_layout: "exact colour mark-mesh buffers absent",
                        },
                    ));
                }
                (
                    ExactTessBind::MarkMesh,
                    start,
                    count,
                    if packed {
                        asset_iw4::vertex_decl::PACKED_VERTEX_TYPE
                    } else {
                        asset_iw4::vertex_decl::WORLD_VERTEX_TYPE
                    },
                )
            }
            RetainedDrawKind::Glass { draw, .. } => {
                if let Some(cause) = self.extracted.frame.glass_mesh_vertex_refusal {
                    return Err(GpuSubmitRefusal::PackedVertex(cause));
                }
                let &(start, count) = self
                    .geometry
                    .glass_mesh_surface_ranges
                    .get(draw as usize)
                    .ok_or(GpuSubmitRefusal::MissingGlassMeshRange { draw })?;
                if count == 0 {
                    return Err(GpuSubmitRefusal::EmptyGlassMeshIndexRange { draw });
                }
                if !self.geometry.glass_mesh.drawable() {
                    return Err(GpuSubmitRefusal::PackedVertex(
                        render_frame::PackedVertexRefusal::ForeignLayout {
                            source_layout: "exact colour glass-mesh buffers absent",
                        },
                    ));
                }
                (
                    ExactTessBind::Glass,
                    start,
                    count,
                    asset_iw4::vertex_decl::PACKED_VERTEX_TYPE,
                )
            }
        };

        let smc_stream_off = if tess == ExactTessBind::SmodelCached {
            if let RetainedDrawKind::Smodel {
                cache_index: Some(cache_index),
                ..
            } = kind
            {
                u64::from(
                    lighting_iw4::smc_stream_source_byte_offset(cache_index)
                        .ok_or(GpuSubmitRefusal::SmodelCacheIndicesMissing { cache_index })?,
                )
            } else {
                0
            }
        } else {
            0
        };

        let (bsp_kind, bsp_run_first, bsp_first_surf, bsp_surf_count) = bsp_draw_source(&kind);
        self.run_pack.begin(run_serial, execution.pass_count());
        append_with_rollback(out, |out| {
            for (pass_index, executable) in execution.iter_passes().enumerate() {
                debug_assert_eq!(
                    executable.port.vertex_type, vertex_type,
                    "ExecutablePass.port was admitted for a different tess vertex type"
                );
                let pipeline_res = self.pipeline_res;
                let (port_index, pipeline) = match self.run_pack.pipelines.get(pass_index).copied()
                {
                    Some(Some(hit)) => hit,
                    _ => {
                        let port_index = pipeline_res.by_id.get(&executable.port).copied().ok_or(
                            GpuSubmitRefusal::NoExactPort {
                                pair: executable.shader_pair,
                            },
                        )?;
                        let host_state = GfxPassState::from_prepared(executable.state);
                        let state0 = host_state.apply_change_state_0_host(AlphaMode::Opaque, false);
                        let state1 = host_state.apply_change_state_1_host();
                        let color = exact_colour_target_format(target.color, state0.srgb_write);
                        let key = ExactColourPipelineKey {
                            target: color,
                            depth_format: target.depth,
                            samples: target.samples,
                            state0,
                            state1,
                            port: executable.port,
                            cached_lighting: false,
                            mark_mesh: tess == ExactTessBind::MarkMesh
                                && vertex_type == asset_iw4::vertex_decl::PACKED_VERTEX_TYPE,
                            forward_z: target.forward_z,
                        };

                        let Some(pipeline) = self.registry.slot(&key) else {
                            self.registry.discover(key);
                            return Err(GpuSubmitRefusal::PipelineNotReady);
                        };
                        if !self.registry.is_ready(pipeline) {
                            return Err(GpuSubmitRefusal::PipelineNotReady);
                        }
                        if let Some(slot) = self.run_pack.pipelines.get_mut(pass_index) {
                            *slot = Some((port_index, pipeline));
                        }
                        (port_index, pipeline)
                    }
                };
                let port_gpu = &pipeline_res.ports[port_index];
                let texture_slots =
                    self.bind_hit_textures(executable, surface, after_scene_resolve)?;
                // Shadow placement is applied after preparation, independently for each
                // light and object. Its unplaced banks belong to the material run.
                let unplaced_shadow = matches!(self.textures, PrepareTextureTables::Shadow { .. })
                    && place_code.is_none();
                let (constants, constant_base) =
                    if identity_placement_kind(&kind) || unplaced_shadow {
                        let constants = self
                            .run_pack
                            .pack(pass_index, &port_gpu.port, executable, &mut self.cost)
                            .map_err(GpuSubmitRefusal::ConstantPack)?;
                        match self.arena.as_deref_mut() {
                            Some(arena) => (None, Some(arena.base_for(&constants, &texture_slots))),
                            None => (Some(constants), None),
                        }
                    } else if let Some(arena) = self.arena.as_deref_mut() {
                        let overlay = place_code.and_then(|c| c.pass(pass_index)).unwrap_or(&[]);
                        let (base, interned) = arena
                            .append_hit(
                                &port_gpu.port,
                                executable,
                                overlay,
                                &texture_slots,
                                key,
                                pass_index as u32,
                            )
                            .map_err(GpuSubmitRefusal::ConstantPack)?;
                        if interned {
                            self.cost.note_intern_hit();
                        } else {
                            self.cost.note_intern_miss();
                            self.cost.note_pack_seed(true);
                        }
                        (None, Some(base))
                    } else {
                        let mut packed = port_gpu
                            .port
                            .pack_hit(executable)
                            .map_err(GpuSubmitRefusal::ConstantPack)?;
                        if let Some(overlay) = place_code.and_then(|c| c.pass(pass_index)) {
                            overlay_packed_code_on_banks(
                                &mut packed.vertex,
                                &mut packed.pixel,
                                overlay,
                            )
                            .map_err(GpuSubmitRefusal::ConstantPack)?;
                        }
                        self.cost.note_pack_seed(executable.local_banks.is_some());
                        (Some(Arc::new(packed)), None)
                    };
                let (depth_min, depth_max) = if target.forward_z {
                    (0.0, 1.0)
                } else {
                    reverse_z_viewport_depth(depth_range_type_for_draw(&kind, key))
                };
                out.push(PreparedExactDraw {
                    after_scene_resolve,
                    pipeline,
                    port: executable.port,
                    constants,
                    constant_base,
                    texture_slots,
                    start,
                    count,
                    tess,
                    owner_object_id: matches!(kind, RetainedDrawKind::XModel { .. })
                        .then(|| drawsurf_object_id(key)),
                    minecraft_hidden: super::minecraft_world::hides_map()
                        && matches!(
                            kind,
                            RetainedDrawKind::World { .. }
                                | RetainedDrawKind::Smodel { .. }
                                | RetainedDrawKind::Glass { .. }
                                | RetainedDrawKind::MarkMesh { .. }
                        ),
                    depth_min,
                    depth_max,
                    state: GfxPassState::from_prepared(executable.state),
                    ring_epoch: 0,
                    smc_stream_off,
                    bsp_kind,
                    bsp_run_first,
                    bsp_first_surf,
                    bsp_surf_count,
                    bsp_counted: bsp_kind.is_some() && pass_index == 0,
                    binds_sun_shadow,
                    binds_spot_shadow,

                    indirect_arg: None,
                    arena_lane: 0,
                });
            }
            Ok(())
        })
    }
}

const PLACED_ARENA_MARK: u32 = 1 << 31;

fn mark_placed_base(placed_rows: u32) -> u32 {
    placed_rows | PLACED_ARENA_MARK
}

fn resolve_arena_base(base: u32, identity_rows: u32) -> u32 {
    if base & PLACED_ARENA_MARK == 0 {
        base
    } else {
        (base & !PLACED_ARENA_MARK).saturating_add(identity_rows)
    }
}

fn write_arena_span(
    queue: &RenderQueue,
    buffer: &Buffer,
    identity: &[u8],
    placed: &[u8],
    reserved: usize,
    start: usize,
    end: usize,
) {
    let Some(size) = NonZeroU64::new(end.saturating_sub(start) as u64) else {
        return;
    };
    let mut view = queue
        .write_buffer_with(buffer, start as u64, size)
        .expect("constant arena upload fits its buffer");
    let identity_end = end.min(identity.len());
    if start < identity_end {
        view.slice(0..identity_end - start)
            .copy_from_slice(&identity[start..identity_end]);
    }
    let pad_start = start.max(identity.len());
    let pad_end = end.min(reserved);
    if pad_start < pad_end {
        view.slice(pad_start - start..pad_end - start).fill(0);
    }
    let placed_start = start.max(reserved);
    let placed_end = end.min(reserved.saturating_add(placed.len()));
    if placed_start < placed_end {
        view.slice(placed_start - start..placed_end - start)
            .copy_from_slice(&placed[placed_start - reserved..placed_end - reserved]);
    }
    let tail_start = start.max(placed_end).max(pad_end);
    if tail_start < end {
        view.slice(tail_start - start..end - start).fill(0);
    }
}
fn grow_identity_reserved(current: usize, needed: usize) -> usize {
    if needed <= current {
        current
    } else {
        needed.next_power_of_two().max(needed)
    }
}

#[derive(Clone, Copy, Debug)]
enum ArenaDirty {
    Identity { start: usize, end: usize },
    Placed { start: usize, end: usize },
}

fn gpu_dirty_range(dirty: ArenaDirty, reserved: usize) -> (usize, usize) {
    match dirty {
        ArenaDirty::Identity { start, end } => (start, end),
        ArenaDirty::Placed { start, end } => {
            (reserved.saturating_add(start), reserved.saturating_add(end))
        }
    }
}

fn coalesce_gpu_dirty(ranges: &mut Vec<(usize, usize)>) {
    if ranges.is_empty() {
        return;
    }
    ranges.sort_unstable_by_key(|range| range.0);
    let mut merged = 0;
    for i in 1..ranges.len() {
        let (start, end) = ranges[i];
        // Queue writes allocate staging storage and record a copy per range.
        // Refill small clean gaps from the authoritative backing as well, trading
        // at most 16 KiB per eliminated write for fewer staging allocations.
        const MAX_UPLOAD_GAP: usize = 16 * 1024;
        if start.saturating_sub(ranges[merged].1) <= MAX_UPLOAD_GAP {
            ranges[merged].1 = ranges[merged].1.max(end);
        } else {
            merged += 1;
            ranges[merged] = (start, end);
        }
    }
    ranges.truncate(merged + 1);
}

fn aligned_backing_span(start: usize, end: usize, backing_len: usize) -> Option<(usize, usize)> {
    if start >= end || start >= backing_len {
        return None;
    }
    let end = end.min(backing_len);
    let start = start / 4 * 4;
    let end = end.next_multiple_of(4).min(backing_len);
    if start >= end || (end - start) % 4 != 0 {
        return None;
    }
    Some((start, end))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PlacedPackKey {
    port: PortId,
    local: usize,
    slots: usize,
    drawsurf_key: u64,
    pass_index: u32,
}

#[derive(Clone, Debug)]
struct PlacedSpan {
    base: u32,
    len: usize,
    stamp: u32,
    version: u64,

    vertex_len: u32,
    pixel_len: u32,

    lanes: Vec<SpanLane>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SpanLane {
    pixel_stage: bool,
    destination: u16,
    row_count: u8,
}

impl SpanLane {
    fn of(lane: &PackedCodeConstantLane) -> Self {
        Self {
            pixel_stage: matches!(lane.stage, RuntimeShaderStage::Pixel),
            destination: lane.destination,
            row_count: lane.row_count,
        }
    }
}

fn patch_span_lanes<'a>(
    span: &mut [u8],
    span_start: usize,
    recorded: &[SpanLane],
    lanes: impl Iterator<Item = SpanLaneWrite<'a>> + Clone,
    vertex_len: usize,
    pixel_len: usize,
    slots: &[u32],
    dirty: &mut Vec<(usize, usize)>,
) -> Option<()> {
    if recorded.len() != lanes.clone().count() {
        return None;
    }
    let mut touched: Option<(usize, usize)> = None;
    let mark = |at: usize, end: usize, touched: &mut Option<(usize, usize)>| match touched {
        Some((lo, hi)) => {
            *lo = (*lo).min(at);
            *hi = (*hi).max(end);
        }
        None => *touched = Some((at, end)),
    };
    for (site, write) in recorded.iter().zip(lanes) {
        if *site != write.site {
            return None;
        }
        let (bank_off, bank_len) = if site.pixel_stage {
            (vertex_len, pixel_len)
        } else {
            (0, vertex_len)
        };
        let first = usize::from(site.destination);
        let count = usize::from(site.row_count);
        if first.saturating_add(count) > bank_len || write.rows.len() != count {
            return None;
        }
        let at = (bank_off + first) * 16;
        let end = at + count * 16;
        if end > span.len() {
            return None;
        }
        let bytes: &[u8] = bytemuck::cast_slice(write.rows);
        if span[at..end] != *bytes {
            span[at..end].copy_from_slice(bytes);
            mark(at, end, &mut touched);
        }
    }
    let tail = (vertex_len + pixel_len) * 16;
    let bytes: &[u8] = bytemuck::cast_slice(slots);
    if tail + bytes.len() > span.len() {
        return None;
    }
    if span[tail..tail + bytes.len()] != *bytes {
        span[tail..tail + bytes.len()].copy_from_slice(bytes);
        mark(tail, tail + bytes.len(), &mut touched);
    }
    if let Some((lo, hi)) = touched {
        dirty.push((span_start + lo, span_start + hi));
    }
    Some(())
}

#[derive(Clone, Copy)]
struct SpanLaneWrite<'a> {
    site: SpanLane,
    rows: &'a [[u32; 4]],
}

fn span_lane_writes<'a>(
    lanes: impl Iterator<Item = &'a PackedCodeConstantLane> + Clone + 'a,
) -> impl Iterator<Item = SpanLaneWrite<'a>> + Clone + 'a {
    lanes.map(|lane| {
        let first = usize::from(lane.first_row);
        let end = first.saturating_add(usize::from(lane.row_count));
        SpanLaneWrite {
            site: SpanLane::of(lane),
            rows: lane.rows.get(first..end).unwrap_or(&[]),
        }
    })
}

fn placed_pack_key(
    port: PortId,
    local: Option<&Arc<render_material::PackedLocalBanks>>,
    slots: &Arc<[u32]>,
    drawsurf_key: u64,
    pass_index: u32,
) -> PlacedPackKey {
    PlacedPackKey {
        port,
        local: local.map(|banks| Arc::as_ptr(banks) as usize).unwrap_or(0),
        slots: Arc::as_ptr(slots).cast::<u32>() as usize,
        drawsurf_key,
        pass_index,
    }
}

fn placed_span_version(code: u64, overlay: &[PackedCodeConstantLane]) -> u64 {
    code ^ PackedCodeConstants::hash_lanes(overlay)
}

fn take_exact_free(free: &mut Vec<(u32, usize)>, need: usize) -> Option<u32> {
    let i = free.iter().position(|(_, len)| *len == need)?;
    Some(free.swap_remove(i).0)
}

struct SpanSource<'a, F> {
    lanes: &'a [PackedCodeConstantLane],
    overlay: &'a [PackedCodeConstantLane],
    vertex_len: usize,
    pixel_len: usize,
    slots: &'a [u32],
    pack: F,
}

impl<F> SpanSource<'_, F> {
    fn writes(&self) -> impl Iterator<Item = SpanLaneWrite<'_>> + Clone {
        span_lane_writes(self.lanes.iter().chain(self.overlay))
    }

    fn sites(&self) -> Vec<SpanLane> {
        self.writes().map(|write| write.site).collect()
    }
}

fn intern_placed_span<E, F>(
    intern: &mut HashMap<PlacedPackKey, PlacedSpan>,
    placed: &mut Vec<u8>,
    stamp: u32,
    key: PlacedPackKey,
    version: u64,
    dirty: &mut Vec<(usize, usize)>,
    free: &mut Vec<(u32, usize)>,
    scratch: &mut Vec<u8>,
    source: SpanSource<'_, F>,
) -> Result<(u32, bool), E>
where
    F: FnOnce(&mut Vec<u8>) -> Result<(), E>,
{
    if let Some(entry) = intern.get_mut(&key) {
        entry.stamp = stamp;
        if entry.version == version {
            return Ok((mark_placed_base(entry.base), true));
        }
        let start = entry.base as usize * 16;
        if start.saturating_add(entry.len) <= placed.len()
            && entry.vertex_len as usize == source.vertex_len
            && entry.pixel_len as usize == source.pixel_len
            && patch_span_lanes(
                &mut placed[start..start + entry.len],
                start,
                &entry.lanes,
                source.writes(),
                source.vertex_len,
                source.pixel_len,
                source.slots,
                dirty,
            )
            .is_some()
        {
            entry.version = version;
            return Ok((mark_placed_base(entry.base), true));
        }
        scratch.clear();
        let sites = source.sites();
        let packed = scratch;
        (source.pack)(packed)?;
        if packed.len() == entry.len && start.saturating_add(entry.len) <= placed.len() {
            placed[start..start + entry.len].copy_from_slice(packed);
            entry.version = version;
            entry.lanes = sites;
            entry.vertex_len = as_u32(source.vertex_len);
            entry.pixel_len = as_u32(source.pixel_len);
            dirty.push((start, start + entry.len));
            return Ok((mark_placed_base(entry.base), true));
        }
        free.push((entry.base, entry.len));
        let (base, start) = place_span(placed, free, packed);
        entry.base = base;
        entry.len = packed.len();
        entry.version = version;
        entry.lanes = sites;
        entry.vertex_len = as_u32(source.vertex_len);
        entry.pixel_len = as_u32(source.pixel_len);
        dirty.push((start, start + packed.len()));
        return Ok((mark_placed_base(base), false));
    }
    scratch.clear();
    let sites = source.sites();
    let vertex_len = source.vertex_len;
    let pixel_len = source.pixel_len;
    let packed = scratch;
    (source.pack)(packed)?;
    let (base, start) = place_span(placed, free, packed);
    intern.insert(
        key,
        PlacedSpan {
            base,
            len: packed.len(),
            stamp,
            version,
            vertex_len: as_u32(vertex_len),
            pixel_len: as_u32(pixel_len),
            lanes: sites,
        },
    );
    dirty.push((start, start + packed.len()));
    Ok((mark_placed_base(base), false))
}

fn place_span(placed: &mut Vec<u8>, free: &mut Vec<(u32, usize)>, packed: &[u8]) -> (u32, usize) {
    if let Some(base) = take_exact_free(free, packed.len()) {
        let start = base as usize * 16;
        placed[start..start + packed.len()].copy_from_slice(packed);
        (base, start)
    } else {
        let start = placed.len();
        placed.extend_from_slice(packed);
        (
            u32::try_from(start / 16).expect("exact constant arena row fits u32"),
            start,
        )
    }
}

#[derive(Default)]
struct ArenaPack {
    identity: Vec<u8>,

    identity_reserved: usize,

    seen: HashMap<(usize, usize), (u32, Arc<PassConstantBuffers>, Arc<[u32]>, u32)>,
    placed: Vec<u8>,

    placed_seen: HashMap<PlacedPackKey, PlacedSpan>,
    identity_free: Vec<(u32, usize)>,
    placed_free: Vec<(u32, usize)>,
    share_n: u32,
    stamp: u32,
    dirty: Vec<ArenaDirty>,

    pack_scratch: Vec<u8>,
    placed_dirty_scratch: Vec<(usize, usize)>,
    full_upload: bool,
}

impl ArenaPack {
    fn begin_list(&mut self) {
        let live = self.stamp;
        if self.seen.values().any(|(_, _, _, stamp)| *stamp != live) {
            let dead: Vec<_> = self
                .seen
                .iter()
                .filter(|(_, (_, _, _, stamp))| *stamp != live)
                .map(|(key, (base, constants, slots, _))| {
                    (
                        *key,
                        *base,
                        std::mem::size_of_val(constants.vertex.as_slice())
                            + std::mem::size_of_val(constants.pixel.as_slice())
                            + d3d9_sm3::texture_slot_rows(slots.len()) * 16,
                    )
                })
                .collect();
            for (key, base, len) in dead {
                self.seen.remove(&key);
                self.identity_free.push((base, len));
            }
        }
        if self.placed_seen.values().any(|span| span.stamp != live) {
            let dead: Vec<_> = self
                .placed_seen
                .iter()
                .filter(|(_, span)| span.stamp != live)
                .map(|(key, span)| (*key, span.base, span.len))
                .collect();
            for (key, base, len) in dead {
                self.placed_seen.remove(&key);
                self.placed_free.push((base, len));
            }
        }
        self.stamp = self.stamp.wrapping_add(1);
        self.share_n = 0;
        self.dirty.clear();
    }

    fn reserved_rows(&self) -> u32 {
        u32::try_from(self.identity_reserved.max(self.identity.len()) / 16)
            .expect("exact constant arena row fits u32")
    }

    fn base_for(&mut self, constants: &Arc<PassConstantBuffers>, slots: &Arc<[u32]>) -> u32 {
        let key = (
            Arc::as_ptr(constants) as usize,
            Arc::as_ptr(slots).cast::<u32>() as usize,
        );
        if let Some((base, _, _, stamp)) = self.seen.get_mut(&key) {
            *stamp = self.stamp;
            self.share_n = self.share_n.saturating_add(1);
            return *base;
        }
        let mut span = Vec::new();
        append_constant_span(&mut span, constants, slots);
        let (base, start) = if let Some(base) = take_exact_free(&mut self.identity_free, span.len())
        {
            let start = base as usize * 16;
            self.identity[start..start + span.len()].copy_from_slice(&span);
            (base, start)
        } else {
            let start = self.identity.len();
            let base = u32::try_from(start / 16).expect("exact constant arena row fits u32");
            self.identity.extend_from_slice(&span);
            (base, start)
        };
        self.seen.insert(
            key,
            (base, Arc::clone(constants), Arc::clone(slots), self.stamp),
        );
        self.dirty.push(ArenaDirty::Identity {
            start,
            end: start + span.len(),
        });
        base
    }

    fn append_hit(
        &mut self,
        port: &super::AdmittedExactPort,
        executable: ExecutablePassView<'_>,
        overlay: &[PackedCodeConstantLane],
        slots: &Arc<[u32]>,
        drawsurf_key: u64,
        pass_index: u32,
    ) -> Result<(u32, bool), ConstantPackRefusal> {
        let key = placed_pack_key(
            executable.port,
            executable.local_banks,
            slots,
            drawsurf_key,
            pass_index,
        );
        let version = placed_span_version(executable.code_constant_id, overlay);
        let Some(banks) = executable.local_banks else {
            return Err(ConstantPackRefusal::MissingLocalBanks);
        };
        self.placed_dirty_scratch.clear();
        let (base, hit) = intern_placed_span(
            &mut self.placed_seen,
            &mut self.placed,
            self.stamp,
            key,
            version,
            &mut self.placed_dirty_scratch,
            &mut self.placed_free,
            &mut self.pack_scratch,
            SpanSource {
                lanes: executable.code_constants,
                overlay,
                vertex_len: banks.vertex.len(),
                pixel_len: banks.pixel.len(),
                slots,
                pack: |placed: &mut Vec<u8>| {
                    port.pack_hit_into(executable, overlay, placed)?;
                    append_slot_rows(placed, slots);
                    Ok(())
                },
            },
        )?;
        for (start, end) in self.placed_dirty_scratch.drain(..) {
            self.dirty.push(ArenaDirty::Placed { start, end });
        }
        if hit {
            self.share_n = self.share_n.saturating_add(1);
        }
        Ok((base, hit))
    }
}

fn append_constant_span(bytes: &mut Vec<u8>, constants: &PassConstantBuffers, slots: &[u32]) {
    bytes.extend_from_slice(bytemuck::cast_slice(constants.vertex.as_slice()));
    bytes.extend_from_slice(bytemuck::cast_slice(constants.pixel.as_slice()));
    append_slot_rows(bytes, slots);
}

fn append_slot_rows(bytes: &mut Vec<u8>, slots: &[u32]) {
    bytes.extend_from_slice(bytemuck::cast_slice(slots));
    let padding = d3d9_sm3::texture_slot_rows(slots.len()) * 4 - slots.len();
    bytes.extend(std::iter::repeat_n(0u8, padding * 4));
}

fn texture_table_layout(pipeline: &ExactColourPipeline) -> Option<&BindGroupLayoutDescriptor> {
    pipeline.ports.first().map(|port| &port.textures_layout)
}

fn arena_capacity(required: usize) -> u64 {
    u64::try_from(required.max(256))
        .expect("exact constant arena size fits u64")
        .next_power_of_two()
}

fn upload_constant_arena(
    draws: &mut [PreparedExactDraw],
    packed: &mut ArenaPack,
    arena: &mut GpuConstantArena,
    pipeline: &ExactColourPipeline,
    registry: &ExactPipelineRegistry,
    device: &RenderDevice,
    queue: &RenderQueue,
    label: &'static str,
    _legacy_label: &'static str,
) -> (u32, usize, usize) {
    for draw in draws.iter_mut() {
        if draw.constant_base.is_none()
            && let Some(ref constants) = draw.constants
        {
            draw.constant_base = Some(packed.base_for(constants, &draw.texture_slots));
        }
    }
    let needed = packed.identity.len();
    let reserved = grow_identity_reserved(packed.identity_reserved, needed);
    if reserved != packed.identity_reserved {
        packed.identity_reserved = reserved;
        packed.full_upload = true;
    }
    for draw in draws.iter_mut() {
        if let Some(base) = draw.constant_base.as_mut() {
            *base = resolve_arena_base(*base, packed.reserved_rows());
        }
    }
    let full_upload = packed.full_upload;
    packed.full_upload = false;
    let mut dirty = std::mem::take(&mut packed.dirty);
    let result = upload_packed_arena(
        &packed.identity,
        &packed.placed,
        packed.identity_reserved,
        packed.share_n,
        &dirty,
        full_upload,
        draws.iter(),
        arena,
        pipeline,
        registry,
        device,
        queue,
        label,
        _legacy_label,
    );
    dirty.clear();
    packed.dirty = dirty;
    result
}

/// What `upload_packed_arena` has to move this frame. A changed logical
/// length is not a relocated layout: growth inside the same allocation keeps
/// its prefix and uploads only the tail, while a recreated buffer or an
/// explicit owner/generation/reservation reset restores every byte. Pure over
/// lengths so the decision is checkable without a device.
#[derive(Debug, PartialEq, Eq)]
enum ArenaUploadPlan {
    /// Same length, no dirty ranges: nothing to write.
    Idle,
    /// The GPU buffer or the layout moved: restore every byte.
    Full,
    /// Same allocation, same layout: write dirty ranges, plus the grown tail
    /// if the logical length moved.
    Ranges,
}

fn plan_arena_upload(
    old_len: usize,
    total_len: usize,
    resize: bool,
    full_upload: bool,
    dirty_empty: bool,
) -> ArenaUploadPlan {
    if resize || full_upload {
        return ArenaUploadPlan::Full;
    }
    if total_len == 0 || (old_len == total_len && dirty_empty) {
        return ArenaUploadPlan::Idle;
    }
    ArenaUploadPlan::Ranges
}

fn upload_packed_arena<'a>(
    identity: &[u8],
    placed: &[u8],
    identity_reserved: usize,
    share_n: u32,
    dirty: &[ArenaDirty],
    full_upload: bool,
    draws: impl IntoIterator<Item = &'a PreparedExactDraw>,
    arena: &mut GpuConstantArena,
    pipeline: &ExactColourPipeline,
    registry: &ExactPipelineRegistry,
    device: &RenderDevice,
    queue: &RenderQueue,
    label: &'static str,
    _legacy_label: &'static str,
) -> (u32, usize, usize) {
    let reserved = identity_reserved.max(identity.len());
    let total_len = reserved.saturating_add(placed.len());
    let required = padded_upload_len(total_len);
    let resize = arena.buffer.is_none()
        || u64::try_from(required).expect("constant arena size fits u64") > arena.capacity;
    if resize {
        arena.capacity = arena_capacity(required);
        arena.buffer = Some(device.create_buffer(&BufferDescriptor {
            label: Some(label),
            size: arena.capacity,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        arena.bind_group = None;
        arena.uploaded_len = 0;
        perf::Counter::CounterArenaReallocN.emit(1.0);
    }

    // Taken where the upload is chosen, before any branch below: sizes the pack
    // selected against what the queue was handed.
    let old_len = arena.uploaded_len;
    let plan = plan_arena_upload(old_len, total_len, resize, full_upload, dirty.is_empty());
    perf::Counter::CounterArenaUsedBytes.emit(total_len as f64);
    perf::Counter::CounterArenaCapacityBytes.emit(arena.capacity as f64);
    let logical_dirty_bytes: usize = dirty
        .iter()
        .copied()
        .map(|range| {
            let (start, end) = gpu_dirty_range(range, reserved);
            end.saturating_sub(start)
        })
        .sum();
    perf::Counter::CounterArenaDirtyBytes.emit(logical_dirty_bytes as f64);
    let mut uploaded_bytes: usize = 0;
    let mut upload_calls: u32 = 0;

    match plan {
        ArenaUploadPlan::Idle => {}
        ArenaUploadPlan::Full => {
            if total_len > 0 {
                write_arena_span(
                    queue,
                    arena.buffer.as_ref().expect("constant arena exists"),
                    identity,
                    placed,
                    reserved,
                    0,
                    padded_upload_len(total_len),
                );
                arena.uploaded_len = total_len;
                uploaded_bytes = total_len;
                upload_calls = 1;
                if resize {
                    perf::Counter::CounterArenaFullResize.emit(1.0);
                } else {
                    perf::Counter::CounterArenaFullFlag.emit(1.0);
                }
            }
        }
        ArenaUploadPlan::Ranges => {
            // Same allocation, same layout: the old prefix is still where it
            // was, so only the backing length moves. Growth extends it and the
            // new tail joins the upload ranges below even with an empty dirty
            // list; shrinkage truncates it, and the dropped tail is dead bytes
            // no draw addresses.
            let grew = total_len > old_len;
            arena.uploaded_len = total_len;
            let mut gpu_ranges = std::mem::take(&mut arena.dirty_scratch);
            gpu_ranges.clear();
            gpu_ranges.extend(
                dirty
                    .iter()
                    .copied()
                    .map(|range| gpu_dirty_range(range, reserved)),
            );
            if grew {
                gpu_ranges.push((old_len, total_len));
            }
            coalesce_gpu_dirty(&mut gpu_ranges);
            for &(start, end) in gpu_ranges.iter() {
                let Some((start, end)) = aligned_backing_span(start, end, total_len) else {
                    continue;
                };
                upload_calls = upload_calls.saturating_add(1);
                uploaded_bytes = uploaded_bytes.saturating_add(end.saturating_sub(start));
                write_arena_span(
                    queue,
                    arena.buffer.as_ref().expect("constant arena exists"),
                    identity,
                    placed,
                    reserved,
                    start,
                    end,
                );
            }
            gpu_ranges.clear();
            arena.dirty_scratch = gpu_ranges;
        }
    }
    perf::Counter::CounterArenaUploadedBytes.emit(uploaded_bytes as f64);
    perf::Counter::CounterArenaUploadCalls.emit(f64::from(upload_calls));
    if arena.bind_group.is_none() {
        let mut layout_source: Option<BindGroupLayoutDescriptor> = None;
        for draw in draws {
            if let Some(port) = pipeline.get(draw.port) {
                layout_source = Some(port.constants_layout.clone());
                break;
            }
        }
        if let Some(layout_source) = layout_source {
            let bgl = registry.bind_group_layout(device, &layout_source);
            let bind_group = device.create_bind_group(
                "iw4_exact_constant_arena",
                &bgl,
                &[BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::Buffer(BufferBinding {
                        buffer: arena.buffer.as_ref().expect("constant arena exists"),
                        offset: 0,
                        size: None,
                    }),
                }],
            );
            arena.bind_group = Some(bind_group);
        }
    }
    (share_n, total_len, 0)
}

pub(super) fn register(app: &mut App) {
    super::floatz::register(app);
    let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) else {
        return;
    };
    render_app
        .init_resource::<InstalledRenderWorld>()
        .init_resource::<PublishedRenderFrame>()
        .init_resource::<ExactColourGeometry>()
        .init_resource::<super::resolved_scene::ResolvedScene>()
        .init_resource::<SmodelCacheGpu>()
        .init_resource::<ExactColourPipeline>()
        .init_resource::<ExactColourBindingCache>()
        .init_resource::<ExactShadowBindingCache>()
        .init_resource::<ExactTextureTable>()
        .init_resource::<ShadowTextureTable>()
        .init_resource::<SceneTextureTables>()
        .init_resource::<ExactPipelineKickCache>()
        .init_resource::<ExactPipelineRegistry>()
        .init_resource::<ExactConstantArena>()
        .init_resource::<ExactColourSubmitCensus>()
        .init_resource::<ShadowmapSunGpu>()
        .init_resource::<ShadowmapSpotGpu>()
        .init_resource::<ShadowmapSunArena>()
        .init_resource::<ShadowmapSpotArena>()
        .init_resource::<ColourSubmitScratch>()
        .init_resource::<ShadowSubmitScratch>()
        .init_resource::<CameraPrepareState>()
        .init_resource::<CameraWorldPretess>()
        .init_resource::<prepare_camera::InstalledColourPass>()
        .init_resource::<indirect::ExactIndirectDraws>()
        .init_resource::<ResidentShadowStaticDraws>()
        .add_systems(
            Render,
            (
                pipeline::init_or_update_pipeline.in_set(RenderSystems::PrepareAssets),
                geometry::upload_exact_geometry.in_set(RenderSystems::PrepareResources),
                pipeline::kick_extracted_colour_pipelines.in_set(RenderSystems::Prepare),
                prepare_camera::install_shared_colour_pass
                    .in_set(RenderSystems::Prepare)
                    .after(geometry::upload_exact_geometry)
                    .after(super::scene_depth::prepare_scene_depth)
                    .after(pipeline::kick_extracted_colour_pipelines)
                    .after(super::gpu_resources::prepare_uploaded_image_registry),
                prepare_camera::prepare_colour_lanes
                    .in_set(RenderSystems::Prepare)
                    .after(prepare_camera::install_shared_colour_pass),
            ),
        )
        .add_systems(
            Core3d,
            (
                record::draw_exact_colour
                    .in_set(Core3dSystems::MainPass)
                    .in_set(super::draw::ExactColourDrawSet)
                    .after(main_opaque_pass_3d),
                record::copy_submit_prepare_ms.after(super::draw::ExactColourDrawSet),
            ),
        );
}
