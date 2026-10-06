use std::num::NonZeroU64;

use bevy::core_pipeline::{Core3d, Core3dSystems};
use bevy::mesh::VertexBufferLayout;
use bevy::prelude::*;
use bevy::render::Extract;
use bevy::render::RenderStartup;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::binding_types::{sampler, texture_2d, uniform_buffer_sized};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, BlendComponent,
    BlendFactor, BlendOperation, BlendState, Buffer, BufferDescriptor, BufferUsages,
    ColorTargetState, ColorWrites, Extent3d, FragmentState, FrontFace, IndexFormat, PipelineCache,
    PrimitiveState, RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor,
    ShaderStages, SpecializedRenderPipeline, SpecializedRenderPipelines, Texture,
    TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
    TextureView, TextureViewDescriptor, VertexAttribute, VertexFormat, VertexState, VertexStepMode,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::texture::GpuImage;
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{ExtractSchedule, Render, RenderApp, RenderSystems};
use bevy::shader::Shader;
use hud::{HudTessBatch, HudTessGpuFrame, HudTessTechnique, HudTessVertex};
use hud_iw4::{cmd_buf_set_2d_projection, set_2d_clip_coeffs};

use super::backend::{
    DYNAMIC_INDEX_BUFFER_CAPACITY, DYNAMIC_TESSELLATION_VB_CAPACITY, GfxCmdBufStreams,
    GfxDrawPrimArgs, GfxDynamicIndexBuffer, GfxDynamicVertexBuffer, GfxStreamSource0,
    copy_tess_vertex_bytes, copy_u16_indices_into_ring, draw_tess_technique, gfx_tess_stream0,
    set_stream_source,
};
use crate::diag::render_frame_diag::SharedRenderStagesSlot;

const SHADER_PATH: &str = "embedded://render_gpu/drawsurf/iw_tess.wgsl";
const PARAMS_SIZE: u64 = 16;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct IwTessVertex {
    xyzw: [f32; 4],
    color_bgra: [u8; 4],
    uv: [f32; 2],
    packed_normal: u32,
}

#[derive(Resource)]
struct IwTessMeta {
    vb: Buffer,
    ib: Buffer,
    cpu_vb: GfxDynamicVertexBuffer,
    cpu_ib: GfxDynamicIndexBuffer,
    cpu_vb_bytes: Vec<u8>,
    cpu_ib_indices: Vec<u16>,
    vb_token: u32,
    retired_vb: Vec<Buffer>,
    retired_ib: Vec<Buffer>,
    vb_wrote: bool,
    ib_wrote: bool,
    draws: Vec<PreparedTessGeom>,
}

struct PreparedTessGeom {
    vb: Buffer,
    ib: Buffer,
    stream: GfxStreamSource0,
    first_index: u32,
    index_count: u32,
    batch_i: usize,
}

fn create_tess_vb(device: &RenderDevice) -> Buffer {
    device.create_buffer(&BufferDescriptor {
        label: Some("iw_tess_dynamic_vb"),
        size: u64::from(DYNAMIC_TESSELLATION_VB_CAPACITY),
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_tess_ib(device: &RenderDevice) -> Buffer {
    device.create_buffer(&BufferDescriptor {
        label: Some("iw_tess_dynamic_ib"),
        size: u64::from(DYNAMIC_INDEX_BUFFER_CAPACITY) * 2,
        usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[derive(Clone, Debug, Default)]
struct ExtractedTessFrame {
    vertices: Vec<IwTessVertex>,
    indices: Vec<u16>,
    batches: Vec<HudTessBatch>,
    surface_w: f32,
    surface_h: f32,
    visible: bool,
    saved_screen_sequence: u64,
    shellshock_screen: Option<hud::ShellshockScreen>,
}

#[derive(Resource, Default)]
struct ExtractedIwTess(ExtractedTessFrame);

struct SavedScreenCopy {
    texture: Texture,
    view: TextureView,
    sampler: Sampler,
    format: TextureFormat,
    size: Extent3d,
}

#[derive(Resource, Default)]
struct SavedScreenGpu {
    copy: Option<SavedScreenCopy>,
    sequence: u64,
    captured: bool,
}

#[derive(Resource)]
struct IwTessPipeline {
    shader: Handle<Shader>,
    modulate_layout: BindGroupLayoutDescriptor,
    params: Buffer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct IwTessPipelineKey {
    target: TextureFormat,
    samples: u32,
    state: Option<render_material::CompiledPassState>,
}

impl SpecializedRenderPipeline for IwTessPipeline {
    type Key = IwTessPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let blend = BlendComponent {
            src_factor: BlendFactor::SrcAlpha,
            dst_factor: BlendFactor::OneMinusSrcAlpha,
            operation: BlendOperation::Add,
        };
        let state = key.state.map(|bits| {
            crate::GfxPassState::from_prepared(bits)
                .apply_change_state_0_host(AlphaMode::Blend, false)
        });
        RenderPipelineDescriptor {
            label: Some("iw_tess_stretchpic".into()),
            layout: vec![self.modulate_layout.clone()],
            immediate_size: 0,
            vertex: VertexState {
                shader: self.shader.clone(),
                shader_defs: Vec::new(),
                entry_point: Some("vs_tess".into()),
                buffers: vec![VertexBufferLayout {
                    array_stride: 32,
                    step_mode: VertexStepMode::Vertex,
                    attributes: vec![
                        VertexAttribute {
                            format: VertexFormat::Float32x4,
                            offset: 0,
                            shader_location: 0,
                        },
                        VertexAttribute {
                            format: VertexFormat::Unorm8x4,
                            offset: 16,
                            shader_location: 1,
                        },
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 20,
                            shader_location: 2,
                        },
                    ],
                }],
            },
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                shader_defs: Vec::new(),
                entry_point: Some("fs_tess".into()),
                targets: vec![Some(ColorTargetState {
                    format: key.target,
                    blend: match state {
                        Some(state) => state.blend.blend_state(),
                        None => Some(BlendState {
                            color: blend,
                            alpha: blend,
                        }),
                    },
                    write_mask: state.map_or(ColorWrites::ALL, |state| state.colour_writes()),
                })],
            }),
            primitive: PrimitiveState {
                front_face: FrontFace::Ccw,
                cull_mode: None,
                ..default()
            },
            depth_stencil: None,
            multisample: bevy::render::render_resource::MultisampleState {
                count: key.samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            zero_initialize_workgroup_memory: false,
        }
    }
}

fn init_pipeline(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    device: Res<RenderDevice>,
) {
    let params = device.create_buffer(&bevy::render::render_resource::BufferDescriptor {
        label: Some("iw_tess_params"),
        size: PARAMS_SIZE,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    commands.insert_resource(IwTessPipeline {
        shader: asset_server.load(SHADER_PATH),
        modulate_layout: BindGroupLayoutDescriptor::new(
            "iw_tess_modulate_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::VERTEX_FRAGMENT,
                (
                    uniform_buffer_sized(false, NonZeroU64::new(PARAMS_SIZE)),
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                ),
            ),
        ),
        params,
    });
    commands.insert_resource(IwTessMeta {
        vb: create_tess_vb(&device),
        ib: create_tess_ib(&device),
        cpu_vb: GfxDynamicVertexBuffer::default(),
        cpu_ib: GfxDynamicIndexBuffer {
            cur_index_count: 0,
            capacity: DYNAMIC_INDEX_BUFFER_CAPACITY,
        },
        cpu_vb_bytes: vec![0u8; DYNAMIC_TESSELLATION_VB_CAPACITY as usize],
        cpu_ib_indices: vec![0u16; DYNAMIC_INDEX_BUFFER_CAPACITY as usize],
        vb_token: 1,
        retired_vb: Vec::new(),
        retired_ib: Vec::new(),
        vb_wrote: false,
        ib_wrote: false,
        draws: Vec::new(),
    });
}

fn extract_iw_tess(mut extracted: ResMut<ExtractedIwTess>, frame: Extract<Res<HudTessGpuFrame>>) {
    extracted.0 = ExtractedTessFrame {
        vertices: frame
            .vertices
            .iter()
            .map(|v: &HudTessVertex| IwTessVertex {
                xyzw: v.xyzw,
                color_bgra: v.color_bgra,
                uv: v.uv,
                packed_normal: v.packed_normal,
            })
            .collect(),
        indices: frame.indices.clone(),
        batches: frame.batches.clone(),
        surface_w: frame.surface_w,
        surface_h: frame.surface_h,
        visible: frame.visible,
        saved_screen_sequence: frame.saved_screen_sequence,
        shellshock_screen: frame.shellshock_screen,
    };
}

// Batches append to the same backing until rollover. Stage that contiguous
// range once, before its buffer is retired or the frame starts drawing it.
fn extend_tess_upload(dirty: &mut Option<core::ops::Range<usize>>, start: usize, len: usize) {
    match dirty {
        Some(range) => {
            range.start = range.start.min(start);
            range.end = range.end.max(start + len);
        }
        None => *dirty = Some(start..start + len),
    }
}

fn flush_tess_upload(
    queue: &RenderQueue,
    buffer: &Buffer,
    backing: &[u8],
    dirty: &mut Option<core::ops::Range<usize>>,
) {
    if let Some(range) = dirty.take() {
        queue.write_buffer(buffer, range.start as u64, &backing[range]);
    }
}

fn prepare_iw_tess(
    extracted: Res<ExtractedIwTess>,
    mut meta: ResMut<IwTessMeta>,
    pipeline: Res<IwTessPipeline>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    meta.draws.clear();
    meta.retired_vb.clear();
    meta.retired_ib.clear();
    meta.vb_wrote = false;
    meta.ib_wrote = false;
    if !extracted.0.visible {
        return;
    }
    let mut vb_dirty = None;
    let mut ib_dirty = None;
    for (batch_i, batch) in extracted.0.batches.iter().enumerate() {
        if batch.index_count == 0 || batch.vertex_count == 0 {
            continue;
        }
        if batch.index_count % 3 != 0 {
            continue;
        }
        let fv = batch.first_vertex as usize;
        let vc = batch.vertex_count as usize;
        let fi = batch.first_index as usize;
        let ic = batch.index_count as usize;
        let Some(verts) = extracted.0.vertices.get(fv..fv.saturating_add(vc)) else {
            continue;
        };
        if verts.len() != vc {
            continue;
        }
        let Some(indices) = extracted.0.indices.get(fi..fi.saturating_add(ic)) else {
            continue;
        };
        if indices.len() != ic {
            continue;
        }
        let need_vb = batch
            .vertex_count
            .saturating_mul(super::backend::GFX_TESS_VERTEX_STRIDE);
        if meta.cpu_vb.capacity < meta.cpu_vb.used_bytes.saturating_add(need_vb) && meta.vb_wrote {
            flush_tess_upload(&queue, &meta.vb, &meta.cpu_vb_bytes, &mut vb_dirty);
            let next = create_tess_vb(&device);
            let old = core::mem::replace(&mut meta.vb, next);
            meta.retired_vb.push(old);
            meta.cpu_vb.used_bytes = 0;
            meta.vb_token = meta.vb_token.wrapping_add(1).max(1);
            meta.cpu_vb_bytes.fill(0);
        }
        let tess_draw = draw_tess_technique(
            GfxDrawPrimArgs {
                vertex_count: batch.vertex_count,
                tri_count: batch.index_count / 3,
                base_index: 0,
            },
            &mut meta.cpu_vb,
            1,
        );
        let packed = bytemuck::cast_slice(verts);
        if !copy_tess_vertex_bytes(&mut meta.cpu_vb_bytes, tess_draw.vertex, packed) {
            continue;
        }
        let vb_gpu = meta.vb.clone();
        extend_tess_upload(
            &mut vb_dirty,
            tess_draw.vertex.lock_byte_offset as usize,
            packed.len(),
        );
        meta.vb_wrote = true;
        let tri_count = batch.index_count / 3;
        if meta
            .cpu_ib
            .cur_index_count
            .saturating_add(batch.index_count)
            > meta.cpu_ib.capacity
            && meta.ib_wrote
        {
            flush_tess_upload(
                &queue,
                &meta.ib,
                bytemuck::cast_slice(&meta.cpu_ib_indices),
                &mut ib_dirty,
            );
            let next = create_tess_ib(&device);
            let old = core::mem::replace(&mut meta.ib, next);
            meta.retired_ib.push(old);
            meta.cpu_ib.cur_index_count = 0;
            meta.cpu_ib_indices.fill(0);
        }
        let index_append = meta.cpu_ib.set_index_data(tri_count);
        if index_append.lock_byte_offset % 4 != 0 {
            continue;
        }
        copy_u16_indices_into_ring(&mut meta.cpu_ib_indices, index_append.base_index, indices);
        let ib_gpu = meta.ib.clone();
        let ib_start = index_append.base_index as usize;
        let ib_end = ib_start.saturating_add(indices.len());
        let Some(ib_bytes) = meta.cpu_ib_indices.get(ib_start..ib_end) else {
            continue;
        };
        extend_tess_upload(
            &mut ib_dirty,
            index_append.lock_byte_offset as usize,
            core::mem::size_of_val(ib_bytes),
        );
        meta.ib_wrote = true;
        let stream = gfx_tess_stream0(meta.vb_token, tess_draw.vertex);
        meta.draws.push(PreparedTessGeom {
            vb: vb_gpu,
            ib: ib_gpu,
            stream,
            first_index: index_append.base_index,
            index_count: batch.index_count,
            batch_i,
        });
    }

    flush_tess_upload(&queue, &meta.vb, &meta.cpu_vb_bytes, &mut vb_dirty);
    flush_tess_upload(
        &queue,
        &meta.ib,
        bytemuck::cast_slice(&meta.cpu_ib_indices),
        &mut ib_dirty,
    );

    let Some(projection) =
        cmd_buf_set_2d_projection(extracted.0.surface_w as i32, extracted.0.surface_h as i32)
    else {
        return;
    };
    let coeffs = set_2d_clip_coeffs(&projection);
    queue.write_buffer(&pipeline.params, 0, bytemuck::bytes_of(&coeffs));
}

fn draw_iw_tess(
    view: ViewQuery<(&ViewTarget, &ExtractedView)>,
    extracted: Res<ExtractedIwTess>,
    meta: Res<IwTessMeta>,
    pipeline: Res<IwTessPipeline>,
    mut specialized: ResMut<SpecializedRenderPipelines<IwTessPipeline>>,
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    images: Res<RenderAssets<GpuImage>>,
    mut texture_table: ResMut<super::texture_table::ExactTextureTable>,
    mut blood: ResMut<HudBloodGpu>,
    mut saved: ResMut<SavedScreenGpu>,
    mut temporal: ResMut<super::shellshock_screen::TemporalScreenGpu>,
    mut temporal_pipeline: ResMut<super::shellshock_screen::TemporalScreenPipeline>,
    queue: Res<RenderQueue>,
    mut context: RenderContext,
    stages: Option<Res<SharedRenderStagesSlot>>,
) {
    let (target, _extracted_view) = view.into_inner();
    let format = target.main_texture_format();
    let scene_size = target.main_texture().size();
    temporal.draw(super::shellshock_screen::TemporalScreenDraw {
        frame: extracted.0.shellshock_screen,
        texture: target.main_texture(),
        view: target.main_texture_view(),
        format,
        device: &device,
        queue: &queue,
        cache: &cache,
        pipeline: &mut temporal_pipeline,
        context: &mut context,
    });
    if extracted.0.saved_screen_sequence != saved.sequence {
        saved.captured = false;
        if let Some(copy) = saved
            .copy
            .as_ref()
            .filter(|copy| copy.format == format && copy.size == scene_size)
        {
            context.command_encoder().copy_texture_to_texture(
                target.main_texture().as_image_copy(),
                copy.texture.as_image_copy(),
                scene_size,
            );
            saved.sequence = extracted.0.saved_screen_sequence;
            saved.captured = true;
        }
    }
    if !extracted.0.visible {
        return;
    }
    if meta.draws.is_empty() {
        if let Some(slot) = stages.as_ref()
            && let Ok(mut guard) = slot.0.lock()
        {
            guard.tess_stream_bind_n = Some(0);
            guard.tess_stream_skip_n = Some(0);
        }
        return;
    }
    let samples = 1;
    let saved_copy = saved
        .copy
        .as_ref()
        .filter(|copy| saved.captured && copy.format == format && copy.size == scene_size);
    let blood_ready = blood.uploaded;
    let mut prepared = Vec::with_capacity(meta.draws.len());
    for geom in &meta.draws {
        let Some(batch) = extracted.0.batches.get(geom.batch_i) else {
            continue;
        };
        let Some(gpu_image) = images.get(&batch.image) else {
            continue;
        };
        let (id, bind, textures) = match batch.technique {
            HudTessTechnique::Modulate => {
                let id = specialized.specialize(
                    &cache,
                    &pipeline,
                    IwTessPipelineKey {
                        target: format,
                        samples,
                        state: batch.state,
                    },
                );
                let layout = cache.get_bind_group_layout(&pipeline.modulate_layout);
                let bind = device.create_bind_group(
                    "iw_tess_modulate",
                    &layout,
                    &BindGroupEntries::sequential((
                        pipeline.params.as_entire_buffer_binding(),
                        &gpu_image.texture_view,
                        &gpu_image.sampler,
                    )),
                );
                (id, bind, None)
            }
            HudTessTechnique::SavedScreen => {
                let Some(copy) = saved_copy else {
                    continue;
                };
                let id = specialized.specialize(
                    &cache,
                    &pipeline,
                    IwTessPipelineKey {
                        target: format,
                        samples,
                        state: batch.state,
                    },
                );
                let layout = cache.get_bind_group_layout(&pipeline.modulate_layout);
                let bind = device.create_bind_group(
                    "iw_tess_saved_screen",
                    &layout,
                    &BindGroupEntries::sequential((
                        pipeline.params.as_entire_buffer_binding(),
                        &copy.view,
                        &copy.sampler,
                    )),
                );
                (id, bind, None)
            }
            HudTessTechnique::SplatterAlt => {
                let Some(gpu_mask) = batch.mask.as_ref().and_then(|mask| images.get(mask)) else {
                    continue;
                };
                let Some(port) = blood.port.as_mut().filter(|_| blood_ready) else {
                    continue;
                };
                let textures =
                    port.textures(&mut texture_table, &device, &cache, gpu_image, gpu_mask);
                (port.pipeline, port.constants.clone(), Some(textures))
            }
        };
        if cache.get_render_pipeline(id).is_none() {
            continue;
        }
        prepared.push((id, bind, textures, geom));
    }
    if prepared.is_empty() {
        if let Some(slot) = stages.as_ref()
            && let Ok(mut guard) = slot.0.lock()
        {
            guard.tess_stream_bind_n = Some(0);
            guard.tess_stream_skip_n = Some(0);
        }
        return;
    }
    let attachments = [Some(target.get_unsampled_color_attachment())];
    let mut pass =
        context.begin_tracked_render_pass(bevy::render::render_resource::RenderPassDescriptor {
            label: Some("iw_tess_stretchpic_pass"),
            color_attachments: &attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    let mut streams = GfxCmdBufStreams::default();
    let mut bind_n = 0u32;
    let mut skip_n = 0u32;
    for (id, bind, textures, geom) in &prepared {
        let Some(gpu_pipeline) = cache.get_render_pipeline(*id) else {
            continue;
        };
        pass.set_render_pipeline(gpu_pipeline);
        pass.set_bind_group(0, bind, &[]);
        if let Some(textures) = textures {
            pass.set_bind_group(1, textures, &[]);
        }
        let action = set_stream_source(
            &mut streams,
            geom.stream.buffer,
            geom.stream.offset,
            geom.stream.stride,
        );
        if action.bind_stream0 {
            pass.set_vertex_buffer(0, geom.vb.slice(u64::from(geom.stream.offset)..));
            bind_n = bind_n.saturating_add(1);
        } else {
            skip_n = skip_n.saturating_add(1);
        }
        pass.set_index_buffer(geom.ib.slice(..), IndexFormat::Uint16);
        let start = geom.first_index;
        let end = start.saturating_add(geom.index_count);
        pass.draw_indexed(start..end, 0, 0..1);
    }
    drop(pass);
    if let Some(slot) = stages.as_ref()
        && let Ok(mut guard) = slot.0.lock()
    {
        guard.tess_stream_bind_n = Some(bind_n);
        guard.tess_stream_skip_n = Some(skip_n);
    }
}

#[derive(Resource, Default)]
pub(super) struct HudBloodGpu {
    port: Option<super::hud_blood::BloodPortGpu>,
    refusal: Option<super::hud_blood::BloodGpuRefusal>,
    uploaded: bool,
}

impl HudBloodGpu {
    fn refuse(&mut self, cause: super::hud_blood::BloodGpuRefusal) {
        if self.refusal.as_ref() != Some(&cause) {
            diag::warn!(World, "hud blood port: RED cause={cause:?}");
            self.refusal = Some(cause);
        }
    }
}

fn prepare_saved_screen(
    views: Query<&ViewTarget, With<Camera3d>>,
    device: Res<RenderDevice>,
    cache: Res<PipelineCache>,
    pipeline: Res<IwTessPipeline>,
    mut specialized: ResMut<SpecializedRenderPipelines<IwTessPipeline>>,
    mut saved: ResMut<SavedScreenGpu>,
) {
    let Some(target) = views.iter().next() else {
        return;
    };
    let format = target.main_texture_format();
    let size = target.main_texture().size();
    specialized.specialize(
        &cache,
        &pipeline,
        IwTessPipelineKey {
            target: format,
            samples: 1,
            state: None,
        },
    );
    if saved
        .copy
        .as_ref()
        .is_some_and(|copy| copy.format == format && copy.size == size)
    {
        return;
    }
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("iw_tess_saved_screen"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&TextureViewDescriptor::default());
    let sampler = device.create_sampler(&SamplerDescriptor::default());
    // Resizing invalidates the frozen image; only a new flash may capture again.
    saved.captured = false;
    saved.copy = Some(SavedScreenCopy {
        texture,
        view,
        sampler,
        format,
        size,
    });
}

fn prepare_hud_blood(
    postfx: Res<super::postfx::ExtractedPostFx>,
    extracted: Res<ExtractedIwTess>,
    views: Query<&ViewTarget, With<Camera3d>>,
    device: Res<RenderDevice>,
    cache: Res<PipelineCache>,
    queue: Res<RenderQueue>,
    mut gpu: ResMut<HudBloodGpu>,
) {
    gpu.uploaded = false;
    let Some(blood) = postfx.blood.as_ref() else {
        gpu.port = None;
        return;
    };
    let Some(format) = views.iter().next().map(ViewTarget::main_texture_format) else {
        return;
    };
    if gpu
        .port
        .as_ref()
        .is_none_or(|port| !port.matches(blood, format))
    {
        gpu.port = None;
        match super::hud_blood::BloodPortGpu::create(blood, format, &device, &cache) {
            Ok(port) => {
                gpu.port = Some(port);
                gpu.refusal = None;
            }
            Err(cause) => {
                gpu.refuse(cause);
                return;
            }
        }
    }
    let drawn = extracted.0.visible
        && extracted
            .0
            .batches
            .iter()
            .any(|batch| batch.technique == HudTessTechnique::SplatterAlt);
    if !drawn {
        return;
    }
    let uploaded = gpu
        .port
        .as_ref()
        .map(|port| port.upload(&queue, extracted.0.surface_w, extracted.0.surface_h));
    match uploaded {
        Some(Ok(())) => gpu.uploaded = true,
        Some(Err(cause)) => gpu.refuse(cause),
        None => {}
    }
}

pub(super) fn register(app: &mut App) {
    super::shellshock_screen::register(app);
    bevy::asset::embedded_asset!(app, "iw_tess.wgsl");
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app
        .init_resource::<ExtractedIwTess>()
        .init_resource::<HudBloodGpu>()
        .init_resource::<SavedScreenGpu>()
        .init_resource::<SpecializedRenderPipelines<IwTessPipeline>>()
        .add_systems(RenderStartup, init_pipeline)
        .add_systems(ExtractSchedule, extract_iw_tess)
        .add_systems(
            Render,
            (
                prepare_iw_tess.in_set(RenderSystems::PrepareResources),
                prepare_hud_blood.in_set(RenderSystems::PrepareResources),
                prepare_saved_screen.in_set(RenderSystems::PrepareResources),
            ),
        )
        .add_systems(
            Core3d,
            draw_iw_tess
                .in_set(Core3dSystems::PostProcess)
                .after(super::postfx::PostFxSet),
        );
}
