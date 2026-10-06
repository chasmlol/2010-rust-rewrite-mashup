use bevy::prelude::*;
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::texture::GpuImage;
use render_material::{MaterialGenerationId, MaterialRefusal, rebind_stable_material};

use super::gpu_prepare::{ConstantPackRefusal, split_bind_layout};
use super::postfx::ExtractedBlood;
use super::postfx_dof::{POSTFX_VERTEX_TYPE, hud_2d_sources};
use super::sm3_wgsl::{PASS_FRAGMENT_ENTRY, PASS_VERTEX_ENTRY};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum BloodGpuRefusal {
    VertexLayout,
    UnsupportedState {
        fields: super::state::UnsupportedStateFields,
    },
    Projection,
    Execute(MaterialRefusal),
    PassCount,
    PortMismatch,
    Constants(ConstantPackRefusal),
    SamplerSlots,
}

pub(super) struct BloodPortGpu {
    generation: MaterialGenerationId,
    format: TextureFormat,
    blood: ExtractedBlood,
    pub(super) pipeline: CachedRenderPipelineId,
    pub(super) constants: BindGroup,
    textures_layout: BindGroupLayoutDescriptor,
    arena: Buffer,
    textures: Option<((TextureViewId, TextureViewId), BindGroup)>,
}

impl BloodPortGpu {
    pub(super) fn matches(&self, blood: &ExtractedBlood, format: TextureFormat) -> bool {
        self.generation == blood.film.generation && self.format == format
    }

    pub(super) fn create(
        blood: &ExtractedBlood,
        format: TextureFormat,
        device: &RenderDevice,
        cache: &PipelineCache,
    ) -> Result<Self, BloodGpuRefusal> {
        let port = &blood.film.port;
        let state = super::state::GfxPassState::from_prepared(blood.film.shell.passes()[0].state());
        if let Some(fields) = state.unsupported_host_fields() {
            return Err(BloodGpuRefusal::UnsupportedState { fields });
        }
        let vertex_layouts = super::colour_submit::vertex_layouts_from_contract(port.wgpu_layout());
        if port.abi().vertex_type != POSTFX_VERTEX_TYPE
            || vertex_layouts.len() != 1
            || port.wgpu_layout().vertex_buffers[0].stream != 0
            || vertex_layouts[0].array_stride != hud_iw4::GFX_TESS_VERTEX_STRIDE as u64
        {
            return Err(BloodGpuRefusal::VertexLayout);
        }
        if blood.texture_slots.len() != port.abi().samplers.len()
            || blood.texture_slots.iter().any(|&slot| slot > 1)
        {
            return Err(BloodGpuRefusal::SamplerSlots);
        }
        let size = (port.module().vertex_constant_len + port.module().pixel_constant_len) * 16
            + d3d9_sm3::texture_slot_rows(port.module().sampler_count) * 16;
        let (constant_entries, texture_entries) = split_bind_layout(port.wgpu_layout());
        let constants_layout = super::colour_submit::bind_group_layout_from_entries(
            "iw4_blood_constants",
            &constant_entries,
            false,
        );
        let textures_layout = super::colour_submit::bind_group_layout_from_entries(
            "iw4_blood_textures",
            &texture_entries,
            false,
        );
        let arena = device.create_buffer(&BufferDescriptor {
            label: Some("iw4_blood_constant_arena"),
            size: size as u64,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let constants = device.create_bind_group(
            "iw4_blood_constants",
            &cache.get_bind_group_layout(&constants_layout),
            &[BindGroupEntry {
                binding: 0,
                resource: BindingResource::Buffer(BufferBinding {
                    buffer: &arena,
                    offset: 0,
                    size: None,
                }),
            }],
        );
        let host_state = state.apply_change_state_0_host(AlphaMode::Blend, false);
        let pipeline = cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some("iw4_blood".into()),
            layout: vec![constants_layout, textures_layout.clone()],
            immediate_size: 0,
            vertex: VertexState {
                shader: blood.film.shader.clone(),
                shader_defs: Vec::new(),
                entry_point: Some(PASS_VERTEX_ENTRY.into()),
                buffers: vertex_layouts,
            },
            fragment: Some(FragmentState {
                shader: blood.film.shader.clone(),
                shader_defs: Vec::new(),
                entry_point: Some(PASS_FRAGMENT_ENTRY.into()),
                targets: vec![Some(ColorTargetState {
                    format,
                    blend: host_state.blend.blend_state(),
                    write_mask: host_state.colour_writes(),
                })],
            }),
            primitive: super::colour_submit::exact_primitive_state(
                host_state.cull,
                host_state.line_fill,
            ),
            depth_stencil: None,
            multisample: default(),
            zero_initialize_workgroup_memory: false,
        });
        Ok(Self {
            generation: blood.film.generation,
            format,
            blood: blood.clone(),
            pipeline,
            constants,
            textures_layout,
            arena,
            textures: None,
        })
    }

    pub(super) fn upload(
        &self,
        queue: &RenderQueue,
        surface_w: f32,
        surface_h: f32,
    ) -> Result<(), BloodGpuRefusal> {
        let port = &self.blood.film.port;
        let sources = hud_2d_sources(surface_w, surface_h).ok_or(BloodGpuRefusal::Projection)?;
        let execution =
            rebind_stable_material(&self.blood.film.shell, &self.blood.film.catalog, &sources)
                .map_err(BloodGpuRefusal::Execute)?;
        let Some(pass) = execution.pass(0).filter(|_| execution.pass_count() == 1) else {
            return Err(BloodGpuRefusal::PassCount);
        };
        if pass.port != port.id() {
            return Err(BloodGpuRefusal::PortMismatch);
        }
        let constants = port.pack_hit(pass).map_err(BloodGpuRefusal::Constants)?;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(bytemuck::cast_slice(&constants.vertex));
        bytes.extend_from_slice(bytemuck::cast_slice(&constants.pixel));
        let mut slots = vec![0u32; d3d9_sm3::texture_slot_rows(port.module().sampler_count) * 4];
        for (ordinal, &slot) in self.blood.texture_slots.iter().enumerate() {
            slots[ordinal] = d3d9_sm3::texture_slot_word(u16::from(slot), u16::from(slot));
        }
        bytes.extend_from_slice(bytemuck::cast_slice(&slots));
        queue.write_buffer(&self.arena, 0, &bytes);
        Ok(())
    }

    pub(super) fn textures(
        &mut self,
        table: &mut super::texture_table::ExactTextureTable,
        device: &RenderDevice,
        cache: &PipelineCache,
        color: &GpuImage,
        mask: &GpuImage,
    ) -> BindGroup {
        let key = (color.texture_view.id(), mask.texture_view.id());
        if self
            .textures
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
        {
            let group = table.views_bind_group(
                device,
                &cache.get_bind_group_layout(&self.textures_layout),
                "iw4_blood_textures",
                &[&color.texture_view, &mask.texture_view],
                &[&color.sampler, &mask.sampler],
            );
            self.textures = Some((key, group));
        }
        self.textures.as_ref().expect("built above").1.clone()
    }
}
