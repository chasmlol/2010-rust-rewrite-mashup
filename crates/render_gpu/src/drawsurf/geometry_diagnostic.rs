use crate::drawsurf::scene_depth::{SCENE_DEPTH_FORMAT, SceneDepthTexture};
use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use bevy::core_pipeline::core_3d::main_opaque_pass_3d;
use bevy::core_pipeline::{Core3d, Core3dSystems};
use bevy::mesh::VertexBufferLayout;
use bevy::prelude::*;
use bevy::render::RenderStartup;
use bevy::render::render_resource::binding_types::uniform_buffer;
use bevy::render::render_resource::{
    BindGroup, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, Buffer,
    BufferInitDescriptor, BufferUsages, CachedRenderPipelineId, ColorTargetState, ColorWrites,
    CompareFunction, DepthStencilState, FragmentState, FrontFace, IndexFormat, MultisampleState,
    PipelineCache, PrimitiveState, RenderPipelineDescriptor, ShaderStages,
    SpecializedRenderPipeline, SpecializedRenderPipelines, StoreOp, TextureFormat, VertexAttribute,
    VertexFormat, VertexState, VertexStepMode,
};
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::view::{
    ExtractedView, Msaa, ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms,
};
use bevy::render::{Render, RenderSystems};
use bevy::shader::Shader;

use super::depth_range::{
    GFX_DEPTH_RANGE_VIEWMODEL, depth_range_type_for_draw, reverse_z_viewport_depth,
};
use crate::diag::render_frame_diag::{SharedRenderStages, SharedRenderStagesSlot};
use render_frame::{
    FrameProductKind, RetainedDrawItem, RetainedDrawKind, SmodelVertex, WorldVertex,
};
use render_material::MaterialGenerationId;

const SHADER_PATH: &str = "embedded://render_gpu/drawsurf/geometry_diagnostic.wgsl";

fn geometry_diagnostic_covers(kind: &RetainedDrawKind, key: u64) -> bool {
    !matches!(
        kind,
        RetainedDrawKind::CodeMesh { .. }
            | RetainedDrawKind::ParticleCloud { .. }
            | RetainedDrawKind::MarkMesh { .. }
            | RetainedDrawKind::Glass { .. }
    ) && depth_range_type_for_draw(kind, key) != GFX_DEPTH_RANGE_VIEWMODEL
}

struct DiagnosticStamp {
    started: Instant,
    slot: Option<Arc<Mutex<SharedRenderStages>>>,
    pass: u32,
    draw_n: u32,
    world_n: u32,
    smodel_n: u32,
    xmodel_n: u32,
}

impl Drop for DiagnosticStamp {
    fn drop(&mut self) {
        let Some(slot) = &self.slot else {
            return;
        };
        let Ok(mut guard) = slot.lock() else {
            return;
        };
        guard.diag_ms = Some(self.started.elapsed().as_secs_f32() * 1000.0);
        guard.diag_pass = Some(self.pass);
        guard.diag_draw_n = Some(self.draw_n);
        guard.diag_world_n = Some(self.world_n);
        guard.diag_smodel_n = Some(self.smodel_n);
        guard.diag_xmodel_n = Some(self.xmodel_n);
    }
}

static DIAGNOSTIC: OnceLock<bool> = OnceLock::new();

pub fn geometry_diagnostic_enabled() -> bool {
    *DIAGNOSTIC.get_or_init(|| {
        std::env::var_os("IW4L_GEOMETRY_DIAGNOSTIC").is_some()
            && std::env::var_os("IW4L_NO_DIAGNOSTIC").is_none()
    })
}

#[derive(Default)]
struct DiagnosticDrawScratch {
    submitted: HashSet<u64>,
    exec_ready: HashSet<u64>,
    draws: Vec<(RetainedDrawItem, bool)>,
    smodel_instances: Vec<DiagnosticSmodelInstance>,
}

fn diagnostic_would_draw(item: &RetainedDrawItem, ready: bool, submitted: &HashSet<u64>) -> bool {
    if ready && matches!(item.kind, RetainedDrawKind::World { .. }) {
        return false;
    }
    if submitted.contains(&item.key) {
        return false;
    }
    geometry_diagnostic_covers(&item.kind, item.key)
}

#[derive(Resource, Clone, Debug, Default)]
pub struct ExtractedDiagnosticGeometry {
    pub generation: MaterialGenerationId,
    pub overlay_gpu_wait: bool,
    pub world_vertices: Arc<Vec<WorldVertex>>,
    pub world_indices: Arc<Vec<u32>>,
    pub world_surface_ranges: Arc<Vec<(u32, u32)>>,
    pub smodel_vertices: Arc<Vec<SmodelVertex>>,
    pub smodel_indices: Arc<Vec<u32>>,
    pub smodel_surface_ranges: Arc<Vec<(u32, u32)>>,
    pub xmodel_vertices: Arc<Vec<SmodelVertex>>,
    pub xmodel_indices: Arc<Vec<u32>>,
    pub xmodel_surface_ranges: Arc<Vec<(u32, u32)>>,
    pub xmodel_revision: u64,
    pub g0_world_surfs: Vec<u16>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct DiagnosticSmodelInstance {
    world_from_local: [[f32; 4]; 4],
}

#[derive(Resource, Default)]
struct DiagnosticGeometry {
    generation: MaterialGenerationId,
    world_vertex: Option<Buffer>,
    world_index: Option<Buffer>,
    world_surface_ranges: Arc<Vec<(u32, u32)>>,
    smodel_vertex: Option<Buffer>,
    smodel_index: Option<Buffer>,
    smodel_surface_ranges: Arc<Vec<(u32, u32)>>,
    world_vertex_count: usize,
    world_index_count: usize,
    smodel_vertex_count: usize,
    smodel_index_count: usize,
    xmodel_vertex: Option<Buffer>,
    xmodel_index: Option<Buffer>,
    xmodel_surface_ranges: Arc<Vec<(u32, u32)>>,
    xmodel_vertex_count: usize,
    xmodel_index_count: usize,
    xmodel_revision: u64,

    g0_world_surfs: Vec<u16>,
}

#[derive(Resource)]
struct DiagnosticPipeline {
    view_layout: BindGroupLayoutDescriptor,
    shader: Handle<Shader>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct DiagnosticPipelineKey {
    target: TextureFormat,
    samples: u32,
    tess: DiagnosticTess,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum DiagnosticTess {
    World,
    Smodel,
    XModel,
}

impl SpecializedRenderPipeline for DiagnosticPipeline {
    type Key = DiagnosticPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        RenderPipelineDescriptor {
            label: Some("iw4_geometry_diagnostic".into()),
            layout: vec![self.view_layout.clone()],
            immediate_size: 0,
            vertex: VertexState {
                shader: self.shader.clone(),
                shader_defs: Vec::new(),
                entry_point: Some(match key.tess {
                    DiagnosticTess::World => "vertex".into(),
                    DiagnosticTess::Smodel | DiagnosticTess::XModel => "vertex_smodel".into(),
                }),
                buffers: match key.tess {
                    DiagnosticTess::World => vec![world_vertex_layout()],
                    DiagnosticTess::Smodel | DiagnosticTess::XModel => {
                        vec![smodel_vertex_layout(), smodel_instance_layout()]
                    }
                },
            },
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                shader_defs: Vec::new(),
                entry_point: Some("fragment".into()),
                targets: vec![Some(ColorTargetState {
                    format: key.target,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                front_face: FrontFace::Cw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(DepthStencilState {
                format: SCENE_DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(CompareFunction::GreaterEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: MultisampleState {
                count: key.samples,
                ..Default::default()
            },
            zero_initialize_workgroup_memory: false,
        }
    }
}

#[derive(Component)]
struct DiagnosticViewBindGroup {
    bind_group: BindGroup,
    world_pipeline: CachedRenderPipelineId,
    smodel_pipeline: CachedRenderPipelineId,
    xmodel_pipeline: CachedRenderPipelineId,
}

fn world_vertex_layout() -> VertexBufferLayout {
    VertexBufferLayout {
        array_stride: size_of::<WorldVertex>() as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: vec![
            VertexAttribute {
                format: VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            VertexAttribute {
                format: VertexFormat::Float32x3,
                offset: 12,
                shader_location: 1,
            },
            VertexAttribute {
                format: VertexFormat::Float32x4,
                offset: 24,
                shader_location: 2,
            },
            VertexAttribute {
                format: VertexFormat::Float32x4,
                offset: 40,
                shader_location: 3,
            },
            VertexAttribute {
                format: VertexFormat::Float32x2,
                offset: 56,
                shader_location: 4,
            },
            VertexAttribute {
                format: VertexFormat::Float32x2,
                offset: 64,
                shader_location: 5,
            },
        ],
    }
}

fn smodel_vertex_layout() -> VertexBufferLayout {
    VertexBufferLayout {
        array_stride: size_of::<SmodelVertex>() as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: vec![
            VertexAttribute {
                format: VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            VertexAttribute {
                format: VertexFormat::Float32x3,
                offset: 12,
                shader_location: 1,
            },
            VertexAttribute {
                format: VertexFormat::Float32x4,
                offset: 24,
                shader_location: 2,
            },
            VertexAttribute {
                format: VertexFormat::Float32x2,
                offset: 40,
                shader_location: 3,
            },
        ],
    }
}

fn smodel_instance_layout() -> VertexBufferLayout {
    VertexBufferLayout {
        array_stride: size_of::<DiagnosticSmodelInstance>() as u64,
        step_mode: VertexStepMode::Instance,
        attributes: (0..4)
            .map(|column| VertexAttribute {
                format: VertexFormat::Float32x4,
                offset: u64::from(column) * 16,
                shader_location: 6 + column,
            })
            .collect(),
    }
}

fn init_pipeline(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(DiagnosticPipeline {
        view_layout: BindGroupLayoutDescriptor::new(
            "iw4_geometry_diagnostic_view",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::VERTEX_FRAGMENT,
                (uniform_buffer::<ViewUniform>(true),),
            ),
        ),
        shader: asset_server.load(SHADER_PATH),
    });
}

fn upload_geometry(
    source: Res<ExtractedDiagnosticGeometry>,
    mut geometry: ResMut<DiagnosticGeometry>,
    device: Res<RenderDevice>,
) {
    if !geometry_diagnostic_enabled() {
        return;
    }
    if source.overlay_gpu_wait {
        use std::sync::atomic::{AtomicBool, Ordering};
        static SKIPPED: AtomicBool = AtomicBool::new(false);
        if !SKIPPED.swap(true, Ordering::Relaxed) {
            diag::info!(
                World,
                "geometry diagnostic GPU upload skipped: overlay GPU residency wait"
            );
        }
        return;
    }
    let geometry_matches = geometry.world_vertex_count == source.world_vertices.len()
        && geometry.world_index_count == source.world_indices.len()
        && geometry.smodel_vertex_count == source.smodel_vertices.len()
        && geometry.smodel_index_count == source.smodel_indices.len();
    if geometry.generation == source.generation && geometry_matches {
        geometry.world_surface_ranges = Arc::clone(&source.world_surface_ranges);
        geometry.smodel_surface_ranges = Arc::clone(&source.smodel_surface_ranges);
    } else {
        geometry.generation = source.generation;
        geometry.world_vertex = None;
        geometry.world_index = None;
        geometry.world_surface_ranges = Arc::new(Vec::new());
        geometry.smodel_vertex = None;
        geometry.smodel_index = None;
        geometry.smodel_surface_ranges = Arc::new(Vec::new());
        geometry.world_vertex_count = source.world_vertices.len();
        geometry.world_index_count = source.world_indices.len();
        geometry.smodel_vertex_count = source.smodel_vertices.len();
        geometry.smodel_index_count = source.smodel_indices.len();
        if !source.world_vertices.is_empty() && !source.world_indices.is_empty() {
            geometry.world_vertex = Some(device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("iw4_geometry_diagnostic_world_vb"),
                contents: bytemuck::cast_slice(source.world_vertices.as_slice()),
                usage: BufferUsages::VERTEX,
            }));
            geometry.world_index = Some(device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("iw4_geometry_diagnostic_world_ib"),
                contents: bytemuck::cast_slice(source.world_indices.as_slice()),
                usage: BufferUsages::INDEX,
            }));
            geometry.world_surface_ranges = Arc::clone(&source.world_surface_ranges);
        }
        if !source.smodel_vertices.is_empty() && !source.smodel_indices.is_empty() {
            geometry.smodel_vertex = Some(device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("iw4_geometry_diagnostic_smodel_vb"),
                contents: bytemuck::cast_slice(source.smodel_vertices.as_slice()),
                usage: BufferUsages::VERTEX,
            }));
            geometry.smodel_index = Some(device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("iw4_geometry_diagnostic_smodel_ib"),
                contents: bytemuck::cast_slice(source.smodel_indices.as_slice()),
                usage: BufferUsages::INDEX,
            }));
            geometry.smodel_surface_ranges = Arc::clone(&source.smodel_surface_ranges);
        }
    }

    let xmodel_matches = geometry.xmodel_revision == source.xmodel_revision
        && geometry.xmodel_vertex_count == source.xmodel_vertices.len()
        && geometry.xmodel_index_count == source.xmodel_indices.len();
    if xmodel_matches {
        geometry.xmodel_surface_ranges = Arc::clone(&source.xmodel_surface_ranges);
    } else {
        geometry.xmodel_vertex = None;
        geometry.xmodel_index = None;
        geometry.xmodel_surface_ranges = Arc::new(Vec::new());
        geometry.xmodel_vertex_count = source.xmodel_vertices.len();
        geometry.xmodel_index_count = source.xmodel_indices.len();
        geometry.xmodel_revision = source.xmodel_revision;
        if !source.xmodel_vertices.is_empty() && !source.xmodel_indices.is_empty() {
            geometry.xmodel_vertex = Some(device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("iw4_geometry_diagnostic_xmodel_vb"),
                contents: bytemuck::cast_slice(source.xmodel_vertices.as_slice()),
                usage: BufferUsages::VERTEX,
            }));
            geometry.xmodel_index = Some(device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("iw4_geometry_diagnostic_xmodel_ib"),
                contents: bytemuck::cast_slice(source.xmodel_indices.as_slice()),
                usage: BufferUsages::INDEX,
            }));
            geometry.xmodel_surface_ranges = Arc::clone(&source.xmodel_surface_ranges);
        }
    }
    geometry.g0_world_surfs.clone_from(&source.g0_world_surfs);
}

fn prepare_views(
    mut commands: Commands,
    pipeline: Option<Res<DiagnosticPipeline>>,
    mut specialized: ResMut<SpecializedRenderPipelines<DiagnosticPipeline>>,
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    uniforms: Res<ViewUniforms>,
    views: Query<(Entity, &ExtractedView, &Msaa)>,
) {
    if !geometry_diagnostic_enabled() {
        return;
    }
    let (Some(pipeline), Some(binding)) = (pipeline, uniforms.uniforms.binding()) else {
        return;
    };
    for (entity, view, msaa) in &views {
        let world_pipeline = specialized.specialize(
            &cache,
            &pipeline,
            DiagnosticPipelineKey {
                target: view.target_format,
                samples: msaa.samples(),
                tess: DiagnosticTess::World,
            },
        );
        let smodel_pipeline = specialized.specialize(
            &cache,
            &pipeline,
            DiagnosticPipelineKey {
                target: view.target_format,
                samples: msaa.samples(),
                tess: DiagnosticTess::Smodel,
            },
        );
        let xmodel_pipeline = specialized.specialize(
            &cache,
            &pipeline,
            DiagnosticPipelineKey {
                target: view.target_format,
                samples: msaa.samples(),
                tess: DiagnosticTess::XModel,
            },
        );
        let bind_group = device.create_bind_group(
            "iw4_geometry_diagnostic_view",
            &cache.get_bind_group_layout(&pipeline.view_layout),
            &BindGroupEntries::single(binding.clone()),
        );
        commands.entity(entity).insert(DiagnosticViewBindGroup {
            bind_group,
            world_pipeline,
            smodel_pipeline,
            xmodel_pipeline,
        });
    }
}

fn draw_geometry_diagnostic(
    view: ViewQuery<(
        &ViewTarget,
        &SceneDepthTexture,
        &ExtractedView,
        &ViewUniformOffset,
        &DiagnosticViewBindGroup,
    )>,
    colour_frame: Res<super::PublishedRenderFrame>,
    geometry: Res<DiagnosticGeometry>,
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    mut context: RenderContext,
    mut last_census: Local<Option<String>>,
    submitted: Option<Res<super::colour_submit::ExactColourSubmitCensus>>,
    extracted: Option<Res<ExtractedDiagnosticGeometry>>,
    slot: Option<Res<SharedRenderStagesSlot>>,
    mut scratch: Local<DiagnosticDrawScratch>,
) {
    let products = &colour_frame.frame_products;
    if extracted
        .as_ref()
        .is_some_and(|extracted| extracted.overlay_gpu_wait)
    {
        return;
    }
    if !geometry_diagnostic_enabled() {
        return;
    }
    let mut stamp = DiagnosticStamp {
        started: Instant::now(),
        slot: slot.as_ref().map(|s| Arc::clone(&s.0)),
        pass: 0,
        draw_n: 0,
        world_n: 0,
        smodel_n: 0,
        xmodel_n: 0,
    };
    scratch.submitted.clear();
    scratch.exec_ready.clear();
    scratch.draws.clear();

    if let Some(census) = submitted.as_ref() {
        scratch
            .submitted
            .extend(census.submitted_keys.iter().copied());
        scratch
            .exec_ready
            .extend(census.world_exec_ready_keys.iter().copied());
    }
    let colour = products.0.product(FrameProductKind::Colour);
    let emissive = products.0.product(FrameProductKind::Emissive);
    let DiagnosticDrawScratch {
        submitted,
        exec_ready,
        draws,
        smodel_instances,
    } = &mut *scratch;
    draws.extend(
        colour
            .ordered_draws
            .iter()
            .enumerate()
            .filter_map(|(_, item)| {
                let ready = exec_ready.contains(&item.key);
                diagnostic_would_draw(item, ready, submitted).then_some((*item, ready))
            })
            .chain(
                emissive
                    .ordered_draws
                    .iter()
                    .enumerate()
                    .filter_map(|(_, item)| {
                        let ready = exec_ready.contains(&item.key);
                        diagnostic_would_draw(item, ready, submitted).then_some((*item, ready))
                    }),
            ),
    );
    for &surf in &geometry.g0_world_surfs {
        let item = RetainedDrawItem {
            material_id: None,
            material_rank: 0,
            key: 0,
            kind: RetainedDrawKind::world(surf),
            surface_samplers: Default::default(),
            camera_region: None,
        };
        if diagnostic_would_draw(&item, false, submitted) {
            draws.push((item, false));
        }
    }
    if draws.is_empty() {
        return;
    }
    let (target, depth, extracted_view, view_offset, view_bind) = view.into_inner();
    let world_pipeline = cache.get_render_pipeline(view_bind.world_pipeline);
    let smodel_pipeline = cache.get_render_pipeline(view_bind.smodel_pipeline);
    let xmodel_pipeline = cache.get_render_pipeline(view_bind.xmodel_pipeline);
    if world_pipeline.is_none() && smodel_pipeline.is_none() && xmodel_pipeline.is_none() {
        return;
    }
    smodel_instances.clear();
    smodel_instances.extend(draws.iter().map(|(item, _)| match item.kind {
        RetainedDrawKind::Smodel {
            world_from_local, ..
        }
        | RetainedDrawKind::XModel {
            world_from_local, ..
        } => DiagnosticSmodelInstance {
            world_from_local: world_from_local.to_cols_array_2d(),
        },
        _ => DiagnosticSmodelInstance {
            world_from_local: Mat4::IDENTITY.to_cols_array_2d(),
        },
    }));
    let smodel_instance_buffer = (!smodel_instances.is_empty()).then(|| {
        device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("iw4_geometry_diagnostic_smodel_instances"),
            contents: bytemuck::cast_slice(smodel_instances.as_slice()),
            usage: BufferUsages::VERTEX,
        })
    });
    let attachments = [Some(target.get_color_attachment())];
    stamp.pass = 1;
    let mut pass =
        context.begin_tracked_render_pass(bevy::render::render_resource::RenderPassDescriptor {
            label: Some("iw4_geometry_diagnostic_pass"),
            color_attachments: &attachments,
            depth_stencil_attachment: Some(depth.get_attachment(StoreOp::Store)),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    pass.set_bind_group(0, &view_bind.bind_group, &[view_offset.offset]);
    let mut bound_tess = None;
    let mut bound_depth = None;
    let viewport = extracted_view.viewport;
    let vp_x = viewport.x as f32;
    let vp_y = viewport.y as f32;
    let vp_w = viewport.z as f32;
    let vp_h = viewport.w as f32;
    let mut world_draws = 0u32;
    let mut smodel_draws = 0u32;
    let mut xmodel_draws = 0u32;
    let fx_draws = 0u32;
    let mut world_skipped = 0u32;
    let mut smodel_skipped = 0u32;
    let mut xmodel_skipped = 0u32;
    let mut fx_skipped = 0u32;
    for (index, (item, ready)) in draws.iter().enumerate() {
        if *ready && matches!(item.kind, RetainedDrawKind::World { .. }) {
            continue;
        }
        if submitted.contains(&item.key) {
            continue;
        }
        if !geometry_diagnostic_covers(&item.kind, item.key) {
            xmodel_skipped = xmodel_skipped.saturating_add(1);
            continue;
        }
        let (tess, start, count, instances) = match item.kind {
            RetainedDrawKind::World { surf, .. } => {
                let (Some(vertex), Some(index), Some(pipeline), Some(&(start, count))) = (
                    geometry.world_vertex.as_ref(),
                    geometry.world_index.as_ref(),
                    world_pipeline,
                    geometry.world_surface_ranges.get(usize::from(surf)),
                ) else {
                    world_skipped = world_skipped.saturating_add(1);
                    continue;
                };
                if bound_tess != Some(DiagnosticTess::World) {
                    pass.set_render_pipeline(pipeline);
                    pass.set_vertex_buffer(0, vertex.slice(..));
                    pass.set_index_buffer(index.slice(..), IndexFormat::Uint32);
                    bound_tess = Some(DiagnosticTess::World);
                }
                world_draws = world_draws.saturating_add(1);
                (DiagnosticTess::World, start, count, 0..1)
            }
            RetainedDrawKind::Smodel { surface, .. } => {
                let (
                    Some(vertex),
                    Some(index_buffer),
                    Some(instance_buffer),
                    Some(pipeline),
                    Some(&(start, count)),
                ) = (
                    geometry.smodel_vertex.as_ref(),
                    geometry.smodel_index.as_ref(),
                    smodel_instance_buffer.as_ref(),
                    smodel_pipeline,
                    geometry.smodel_surface_ranges.get(surface as usize),
                )
                else {
                    smodel_skipped = smodel_skipped.saturating_add(1);
                    continue;
                };
                if bound_tess != Some(DiagnosticTess::Smodel) {
                    pass.set_render_pipeline(pipeline);
                    pass.set_vertex_buffer(0, vertex.slice(..));
                    pass.set_vertex_buffer(1, instance_buffer.slice(..));
                    pass.set_index_buffer(index_buffer.slice(..), IndexFormat::Uint32);
                    bound_tess = Some(DiagnosticTess::Smodel);
                }
                smodel_draws = smodel_draws.saturating_add(1);
                let instance = u32::try_from(index).unwrap_or(u32::MAX);
                (
                    DiagnosticTess::Smodel,
                    start,
                    count,
                    instance..instance.saturating_add(1),
                )
            }
            RetainedDrawKind::XModel { surface, .. } => {
                let (
                    Some(vertex),
                    Some(index_buffer),
                    Some(instance_buffer),
                    Some(pipeline),
                    Some(&(start, count)),
                ) = (
                    geometry.xmodel_vertex.as_ref(),
                    geometry.xmodel_index.as_ref(),
                    smodel_instance_buffer.as_ref(),
                    xmodel_pipeline,
                    geometry.xmodel_surface_ranges.get(surface as usize),
                )
                else {
                    xmodel_skipped = xmodel_skipped.saturating_add(1);
                    continue;
                };
                if bound_tess != Some(DiagnosticTess::XModel) {
                    pass.set_render_pipeline(pipeline);
                    pass.set_vertex_buffer(0, vertex.slice(..));
                    pass.set_vertex_buffer(1, instance_buffer.slice(..));
                    pass.set_index_buffer(index_buffer.slice(..), IndexFormat::Uint32);
                    bound_tess = Some(DiagnosticTess::XModel);
                }
                xmodel_draws = xmodel_draws.saturating_add(1);
                let instance = u32::try_from(index).unwrap_or(u32::MAX);
                (
                    DiagnosticTess::XModel,
                    start,
                    count,
                    instance..instance.saturating_add(1),
                )
            }
            RetainedDrawKind::CodeMesh { .. } | RetainedDrawKind::ParticleCloud { .. } => {
                fx_skipped = fx_skipped.saturating_add(1);
                continue;
            }
            RetainedDrawKind::MarkMesh { .. } | RetainedDrawKind::Glass { .. } => {
                fx_skipped = fx_skipped.saturating_add(1);
                continue;
            }
        };
        if count == 0 {
            match tess {
                DiagnosticTess::World => {
                    world_draws = world_draws.saturating_sub(1);
                    world_skipped = world_skipped.saturating_add(1);
                }
                DiagnosticTess::Smodel => {
                    smodel_draws = smodel_draws.saturating_sub(1);
                    smodel_skipped = smodel_skipped.saturating_add(1);
                }
                DiagnosticTess::XModel => {
                    xmodel_draws = xmodel_draws.saturating_sub(1);
                    xmodel_skipped = xmodel_skipped.saturating_add(1);
                }
            }
            continue;
        }
        if vp_w > 0.0 && vp_h > 0.0 {
            let (depth_min, depth_max) =
                reverse_z_viewport_depth(depth_range_type_for_draw(&item.kind, item.key));
            if bound_depth != Some((depth_min, depth_max)) {
                pass.set_viewport(vp_x, vp_y, vp_w, vp_h, depth_min, depth_max);
                bound_depth = Some((depth_min, depth_max));
            }
        }
        pass.draw_indexed(start..start.saturating_add(count), 0, instances);
    }
    drop(pass);
    stamp.draw_n = world_draws
        .saturating_add(smodel_draws)
        .saturating_add(xmodel_draws)
        .saturating_add(fx_draws);
    stamp.world_n = world_draws;
    stamp.smodel_n = smodel_draws;
    stamp.xmodel_n = xmodel_draws;
    let g0_queued = geometry.g0_world_surfs.len();
    let line = format!(
        "drawsurf geometry diagnostic: world_draws={world_draws} smodel_draws={smodel_draws} xmodel_draws={xmodel_draws} fx_draws={fx_draws} world_skipped={world_skipped} smodel_skipped={smodel_skipped} xmodel_skipped={xmodel_skipped} fx_skipped={fx_skipped} g0_queued={g0_queued}"
    );
    if last_census.as_ref() != Some(&line) {
        *last_census = Some(line.clone());
        diag::warn!(World, "{line}");
    }
}

pub(super) fn register(app: &mut App) {
    if !geometry_diagnostic_enabled() {
        return;
    }
    bevy::asset::embedded_asset!(app, "geometry_diagnostic.wgsl");
    let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) else {
        return;
    };
    render_app
        .init_resource::<ExtractedDiagnosticGeometry>()
        .init_resource::<DiagnosticGeometry>()
        .init_resource::<SpecializedRenderPipelines<DiagnosticPipeline>>()
        .add_systems(RenderStartup, init_pipeline)
        .add_systems(
            Render,
            (
                upload_geometry.in_set(RenderSystems::PrepareResources),
                prepare_views.in_set(RenderSystems::PrepareBindGroups),
            ),
        )
        .add_systems(
            Core3d,
            draw_geometry_diagnostic
                .in_set(Core3dSystems::MainPass)
                .after(main_opaque_pass_3d)
                .after(super::draw::ExactColourDrawSet),
        );
}
