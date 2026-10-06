use bevy::prelude::*;
use bevy::render::render_resource::binding_types::{sampler, texture_2d, uniform_buffer_sized};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue};
use bevy::render::{RenderApp, RenderStartup};
use std::{collections::HashMap, num::NonZeroU64};

#[derive(Resource)]
pub(super) struct TemporalScreenPipeline {
    shader: Handle<Shader>,
    layout: BindGroupLayoutDescriptor,
    params: Buffer,
    sampler: Sampler,
    pipelines: HashMap<TextureFormat, CachedRenderPipelineId>,
}

impl TemporalScreenPipeline {
    fn pipeline(&mut self, format: TextureFormat, cache: &PipelineCache) -> CachedRenderPipelineId {
        *self.pipelines.entry(format).or_insert_with(|| {
            cache.queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("shellshock_screen".into()),
                layout: vec![self.layout.clone()],
                vertex: VertexState {
                    shader: self.shader.clone(),
                    entry_point: Some("vertex".into()),
                    ..default()
                },
                fragment: Some(FragmentState {
                    shader_defs: Vec::new(),
                    shader: self.shader.clone(),
                    entry_point: Some("fragment".into()),
                    targets: vec![Some(ColorTargetState {
                        format,
                        blend: Some(screen_blend()),
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                ..default()
            })
        })
    }
}

fn screen_blend() -> BlendState {
    let blend = BlendComponent {
        src_factor: BlendFactor::SrcAlpha,
        dst_factor: BlendFactor::OneMinusSrcAlpha,
        operation: BlendOperation::Add,
    };
    BlendState {
        color: blend,
        alpha: blend,
    }
}

struct ScreenHistory {
    texture: Texture,
    view: TextureView,
    format: TextureFormat,
    size: Extent3d,
    owner: (Option<u64>, u32, u32, u64, u32),
    saved_ms: i32,
}

#[derive(Resource, Default)]
pub(super) struct TemporalScreenGpu {
    history: Option<ScreenHistory>,
}

pub(super) struct TemporalScreenDraw<'a, 'w, 's> {
    pub frame: Option<hud::ShellshockScreen>,
    pub texture: &'a Texture,
    pub view: &'a TextureView,
    pub format: TextureFormat,
    pub device: &'a RenderDevice,
    pub queue: &'a RenderQueue,
    pub cache: &'a PipelineCache,
    pub pipeline: &'a mut TemporalScreenPipeline,
    pub context: &'a mut RenderContext<'w, 's>,
}

impl TemporalScreenGpu {
    pub(super) fn draw(&mut self, draw: TemporalScreenDraw<'_, '_, '_>) {
        let TemporalScreenDraw {
            frame,
            texture,
            view,
            format,
            device,
            queue,
            cache,
            pipeline,
            context,
        } = draw;
        let Some(frame) = frame.filter(|frame| frame.blend_ms > 0) else {
            self.history = None;
            return;
        };
        let size = texture.size();
        let owner = (
            frame.world_generation,
            frame.client,
            frame.view_client,
            frame.life_sequence,
            frame.authoritative_life,
        );
        if self.history.as_ref().is_some_and(|history| {
            history.format != format
                || history.size != size
                || history.owner != owner
                || frame.time_ms.wrapping_sub(history.saved_ms) < 0
        }) {
            self.history = None;
        }
        let id = pipeline.pipeline(format, cache);
        if let Some(history) = &self.history
            && let Some(alpha) =
                hud_iw4::shellshock_screen_alpha(frame.time_ms, history.saved_ms, frame.blend_ms)
            && let Some(render_pipeline) = cache.get_render_pipeline(id)
        {
            queue.write_buffer(
                &pipeline.params,
                0,
                bytemuck::cast_slice(&[f32::from(alpha) / 255.0, 0.0, 0.0, 0.0]),
            );
            let bind = device.create_bind_group(
                "shellshock_screen",
                &cache.get_bind_group_layout(&pipeline.layout),
                &BindGroupEntries::sequential((
                    &history.view,
                    &pipeline.sampler,
                    pipeline.params.as_entire_buffer_binding(),
                )),
            );
            let mut pass = context.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some("shellshock_screen"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                })],
                ..default()
            });
            pass.set_render_pipeline(render_pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        let history = self.history.get_or_insert_with(|| {
            let texture = device.create_texture(&TextureDescriptor {
                label: Some("shellshock_screen_history"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format,
                usage: TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&TextureViewDescriptor::default());
            ScreenHistory {
                texture,
                view,
                format,
                size,
                owner,
                saved_ms: frame.time_ms,
            }
        });
        context.command_encoder().copy_texture_to_texture(
            texture.as_image_copy(),
            history.texture.as_image_copy(),
            size,
        );
        history.saved_ms = frame.time_ms;
    }
}

impl TemporalScreenPipeline {
    fn new(device: &RenderDevice, shader: Handle<Shader>) -> Self {
        Self {
            shader,
            layout: BindGroupLayoutDescriptor::new(
                "shellshock_screen",
                &BindGroupLayoutEntries::sequential(
                    ShaderStages::FRAGMENT,
                    (
                        texture_2d(TextureSampleType::Float { filterable: true }),
                        sampler(SamplerBindingType::Filtering),
                        uniform_buffer_sized(false, NonZeroU64::new(16)),
                    ),
                ),
            ),
            params: device.create_buffer(&BufferDescriptor {
                label: Some("shellshock_screen_alpha"),
                size: 16,
                usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            sampler: device.create_sampler(&SamplerDescriptor {
                mag_filter: FilterMode::Linear,
                min_filter: FilterMode::Linear,
                ..default()
            }),
            pipelines: HashMap::new(),
        }
    }
}

fn init(mut commands: Commands, assets: Res<AssetServer>, device: Res<RenderDevice>) {
    commands.insert_resource(TemporalScreenPipeline::new(
        &device,
        assets.load("embedded://render_gpu/drawsurf/shellshock_screen.wgsl"),
    ));
}

pub(super) fn register(app: &mut App) {
    bevy::asset::embedded_asset!(app, "shellshock_screen.wgsl");
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app
            .init_resource::<TemporalScreenGpu>()
            .add_systems(RenderStartup, init);
    }
}
