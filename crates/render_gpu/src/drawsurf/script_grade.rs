use bevy::prelude::*;
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderDevice, RenderQueue};

#[derive(Resource)]
pub(super) struct GradeShader(pub Handle<Shader>);

pub(super) struct GradeGpu {
    pub pipeline: CachedRenderPipelineId,
    layout: BindGroupLayoutDescriptor,
    uniform: Buffer,
    sampler: Sampler,
}

impl GradeGpu {
    pub fn new(device: &RenderDevice, cache: &PipelineCache, shader: Handle<Shader>) -> Self {
        let layout = BindGroupLayoutDescriptor::new(
            "gsc_grade",
            &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        );
        let pipeline = cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some("gsc_grade".into()),
            layout: vec![layout.clone()],
            vertex: VertexState {
                shader: shader.clone(),
                entry_point: Some("vertex".into()),
                ..default()
            },
            fragment: Some(FragmentState {
                shader,
                entry_point: Some("fragment".into()),
                targets: vec![Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
                ..default()
            }),
            ..default()
        });
        Self {
            pipeline,
            layout,
            uniform: device.create_buffer(&BufferDescriptor {
                label: Some("gsc_grade"),
                size: 256,
                usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            sampler: device.create_sampler(&SamplerDescriptor {
                mag_filter: FilterMode::Linear,
                min_filter: FilterMode::Linear,
                ..default()
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        device: &RenderDevice,
        queue: &RenderQueue,
        cache: &PipelineCache,
        encoder: &mut CommandEncoder,
        source: &TextureView,
        target: &TextureView,
        grading: [f32; 4],
        t6_film: Option<asset_world::T6FilmGrade>,
    ) {
        let mut rows = [[0.0f32; 4]; 16];
        rows[0] = grading;
        if let Some(film) = t6_film {
            rows[1][0] = 1.0;
            rows[2..].copy_from_slice(&film.controls);
        }
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&rows));
        let group = device.create_bind_group(
            "gsc_grade",
            &cache.get_bind_group_layout(&self.layout),
            &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(source),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&self.sampler),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: self.uniform.as_entire_binding(),
                },
            ],
        );
        let attachments = [Some(RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(LinearRgba::BLACK.into()),
                store: StoreOp::Store,
            },
            depth_slice: None,
        })];
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("gsc_grade"),
            color_attachments: &attachments,
            ..default()
        });
        pass.set_pipeline(
            cache
                .get_render_pipeline(self.pipeline)
                .expect("grade preflight"),
        );
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}
