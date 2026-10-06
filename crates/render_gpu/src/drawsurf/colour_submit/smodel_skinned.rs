use std::sync::Arc;

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::{Buffer, BufferDescriptor, BufferUsages};
use bevy::render::renderer::{RenderDevice, RenderQueue};

use super::{ExtractedColourRefs, GpuSubmitRefusal};
use crate::drawsurf::gpu_resources::padded_upload_len;

#[derive(Default)]
pub(super) struct SmodelSkinnedTess {
    verts: Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
    vertex_lighting: Vec<[u8; 4]>,
    indices: Vec<u32>,
    spans: HashMap<(u32, u32), SkinnedSpan>,
    geometry: Option<Arc<super::ExtractedStaticGeometry>>,
    vertex: Option<Buffer>,
    vertex_lighting_buffer: Option<Buffer>,
    index: Option<Buffer>,
    vertex_cap: usize,
    index_cap: usize,
    uploaded_verts: usize,
    uploaded_indices: usize,
}

struct SkinnedSpan {
    world_from_local: Mat4,
    span: (u32, u32),
}

impl SmodelSkinnedTess {
    pub(super) fn begin_frame(&mut self) {}

    fn forget(&mut self) {
        self.verts.clear();
        self.vertex_lighting.clear();
        self.indices.clear();
        self.spans.clear();
        self.uploaded_verts = 0;
        self.uploaded_indices = 0;
    }

    pub(super) fn append_draw(
        &mut self,
        extracted: ExtractedColourRefs<'_>,
        placement: u32,
        surface: u32,
        world_from_local: Mat4,
    ) -> Result<(u32, u32), GpuSubmitRefusal> {
        let geometry = &extracted.world.static_geometry;
        if !self
            .geometry
            .as_ref()
            .is_some_and(|kept| Arc::ptr_eq(kept, geometry))
        {
            self.forget();
            self.geometry = Some(Arc::clone(geometry));
        }
        if let Some(kept) = self.spans.get(&(placement, surface))
            && kept.world_from_local == world_from_local
        {
            return Ok(kept.span);
        }
        let geom = geometry.as_ref();
        let &(packed_off, packed_n) = geom
            .smodel_surface_verts
            .get(surface as usize)
            .ok_or(GpuSubmitRefusal::MissingSmodelRange { surface })?;
        if packed_n == 0 {
            return Err(GpuSubmitRefusal::EmptySmodelIndexRange { surface });
        }
        let packed_off_us = packed_off as usize;
        let packed_n_us = packed_n as usize;
        let src = geom
            .smodel_vertices
            .get(packed_off_us..packed_off_us.saturating_add(packed_n_us))
            .ok_or(GpuSubmitRefusal::SmodelSkinnedDestMissing { placement })?;
        let &(index_start, index_count) = geom
            .smodel_surface_ranges
            .get(surface as usize)
            .ok_or(GpuSubmitRefusal::MissingSmodelRange { surface })?;
        if index_count == 0 {
            return Err(GpuSubmitRefusal::EmptySmodelIndexRange { surface });
        }
        let index_start_us = index_start as usize;
        let index_count_us = index_count as usize;
        let src_ix = geom
            .smodel_indices
            .get(index_start_us..index_start_us.saturating_add(index_count_us))
            .ok_or(GpuSubmitRefusal::SmodelSkinnedDestMissing { placement })?;
        let packed_end = packed_off.saturating_add(packed_n);
        if src_ix
            .iter()
            .any(|&idx| idx < packed_off || idx >= packed_end)
        {
            return Err(GpuSubmitRefusal::SmodelSkinnedDestMissing { placement });
        }
        let dest_base = self.verts.len();
        self.verts
            .resize(dest_base.saturating_add(packed_n_us), [0u8; 32]);
        let m = world_from_local.to_cols_array();
        let fixed = lighting_iw4::setup_transform_unit_vec(&m);
        if lighting_iw4::skin_xsurface_unique_verts(&mut self.verts[dest_base..], src, &m, &fixed)
            .is_err()
        {
            self.verts.truncate(dest_base);
            return Err(GpuSubmitRefusal::SmodelSkinnedDestMissing { placement });
        }
        match geom
            .smodel_vertex_lighting
            .get(packed_off_us..packed_off_us.saturating_add(packed_n_us))
        {
            Some(lighting) => self.vertex_lighting.extend_from_slice(lighting),
            None => self
                .vertex_lighting
                .resize(self.verts.len(), super::NEUTRAL_VERTEX_LIGHTING),
        }
        let dest_index_start = self.indices.len() as u32;
        let dest_base = dest_base as u32;
        self.indices.extend(
            src_ix
                .iter()
                .map(|&idx| idx.saturating_sub(packed_off).saturating_add(dest_base)),
        );
        let span = (dest_index_start, index_count);
        self.spans.insert(
            (placement, surface),
            SkinnedSpan {
                world_from_local,
                span,
            },
        );
        Ok(span)
    }

    pub(super) fn upload(&mut self, device: &RenderDevice, queue: &RenderQueue) {
        if self.verts.is_empty() || self.indices.is_empty() {
            return;
        }
        let vbytes: &[u8] = bytemuck::cast_slice(self.verts.as_slice());
        let ibytes: &[u8] = bytemuck::cast_slice(self.indices.as_slice());
        let vneed = padded_upload_len(vbytes.len());
        let ineed = padded_upload_len(ibytes.len());
        if self.vertex.is_none() || self.vertex_cap < vneed {
            self.vertex_cap = vneed.next_power_of_two();
            self.vertex = Some(device.create_buffer(&BufferDescriptor {
                label: Some("iw4_smodel_skinned_unique_vb"),
                size: self.vertex_cap as u64,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.vertex_lighting_buffer = Some(device.create_buffer(&BufferDescriptor {
                label: Some("iw4_smodel_skinned_unique_vertex_lighting_vb"),
                size: (self.vertex_cap / asset_iw4::size::GFX_PACKED_VERTEX * 4) as u64,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.uploaded_verts = 0;
        }
        if self.index.is_none() || self.index_cap < ineed {
            self.index_cap = ineed.next_power_of_two();
            self.index = Some(device.create_buffer(&BufferDescriptor {
                label: Some("iw4_smodel_skinned_unique_ib"),
                size: self.index_cap as u64,
                usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.uploaded_indices = 0;
        }
        const VERTEX_BYTES: usize = asset_iw4::size::GFX_PACKED_VERTEX;
        if let Some(buffer) = self.vertex.as_ref()
            && self.uploaded_verts < self.verts.len()
        {
            let from = self.uploaded_verts * VERTEX_BYTES;
            queue.write_buffer(buffer, from as u64, &vbytes[from..]);
            if let Some(lighting) = self.vertex_lighting_buffer.as_ref() {
                let lighting_bytes: &[u8] = bytemuck::cast_slice(self.vertex_lighting.as_slice());
                let from = self.uploaded_verts * 4;
                queue.write_buffer(lighting, from as u64, &lighting_bytes[from..]);
            }
            self.uploaded_verts = self.verts.len();
        }
        if let Some(buffer) = self.index.as_ref()
            && self.uploaded_indices < self.indices.len()
        {
            let from = self.uploaded_indices * 4;
            queue.write_buffer(buffer, from as u64, &ibytes[from..]);
            self.uploaded_indices = self.indices.len();
        }
    }

    pub(super) fn vertex_buffer(&self) -> Option<&Buffer> {
        self.vertex.as_ref()
    }

    pub(super) fn vertex_lighting_buffer(&self) -> Option<&Buffer> {
        self.vertex_lighting_buffer.as_ref()
    }

    pub(super) fn index_buffer(&self) -> Option<&Buffer> {
        self.index.as_ref()
    }
}
