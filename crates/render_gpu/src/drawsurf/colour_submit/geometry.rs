use super::residency;
use super::*;

pub(super) fn upload_exact_geometry(
    world: Res<InstalledRenderWorld>,
    frame: Res<PublishedRenderFrame>,
    geometry: ResMut<ExactColourGeometry>,
    mut smodel_cache_gpu: ResMut<SmodelCacheGpu>,
    mut census: ResMut<ExactColourSubmitCensus>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let source = ExtractedColourRefs::new(&world, &frame);
    let geometry = geometry.into_inner();
    smodel_cache_gpu.ensure(&device);
    for (lock, bytes) in &source.frame.smc_vb_patches {
        let _ = smodel_cache_gpu.patch(&queue, *lock, bytes);
    }
    for (off, bytes) in &source.frame.smc_ib_patches {
        let _ = smodel_cache_gpu.patch_indices(&queue, *off, bytes);
    }
    if geometry.world_cpu_indices.is_empty()
        && !source.world.static_geometry.world_indices.is_empty()
    {
        geometry
            .world_cpu_indices
            .clone_from(source.world.static_geometry.world_indices.as_ref());
        if geometry.world_surface_ranges.is_empty() {
            geometry
                .world_surface_ranges
                .clone_from(source.world.static_geometry.world_surface_ranges.as_ref());
        }
    }
    let empty_source = source.world.static_geometry.world_vertices.is_empty()
        && source.world.static_geometry.smodel_vertices.is_empty();
    if empty_source && geometry.world_products.0.is_some() {
        return;
    }
    let static_matches = colour_world_smodel_static(
        (geometry.world_generation, geometry.world_products),
        geometry.world_vertex_count,
        geometry.world_index_count,
        geometry.world_layer_count,
        geometry.smodel_vertex_count,
        geometry.smodel_index_count,
        (source.world.world_generation, source.world.world_products),
        source.world.static_geometry.world_vertices.len(),
        source.world.static_geometry.world_indices.len(),
        source.world.static_geometry.world_layer.len(),
        source.world.static_geometry.smodel_vertices.len(),
        source.world.static_geometry.smodel_indices.len(),
    );
    geometry.world_generation = source.world.world_generation;
    if !static_matches {
        geometry.world_products = source.world.world_products;
        geometry.world_vertex = None;
        geometry.world_layer = None;
        geometry.world_effect = None;
        geometry.world_index = None;
        geometry.world_cpu_indices.clear();
        geometry.world_surface_ranges.clear();
        geometry.world_vertex_count = source.world.static_geometry.world_vertices.len();
        geometry.world_layer_count = source.world.static_geometry.world_layer.len();
        geometry.world_index_count = source.world.static_geometry.world_indices.len();
        geometry.smodel_vertex = None;
        geometry.smodel_vertex_lighting = None;
        geometry.smodel_index = None;
        geometry.smodel_surface_ranges.clear();
        geometry.smodel_vertex_count = source.world.static_geometry.smodel_vertices.len();
        geometry.smodel_index_count = source.world.static_geometry.smodel_indices.len();
        geometry.smodel_cached_vertex = None;
        geometry.smodel_cached_index = None;
        geometry.smodel_cached_vertex_count =
            source.world.static_geometry.smodel_cached_vertices.len();
        if !source.world.static_geometry.world_vertices.is_empty()
            && !source.world.static_geometry.world_indices.is_empty()
        {
            diag::info!(
                World,
                "exact geometry: upload static buffers for install {:?} — world {} + smodel {} vertices, products {:?}",
                source.world.world_generation.0,
                source.world.static_geometry.world_vertices.len(),
                source.world.static_geometry.smodel_vertices.len(),
                source.world.world_products.0,
            );
            geometry.world_vertex = Some(device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("iw4_exact_colour_world_vb"),
                contents: bytemuck::cast_slice(
                    source.world.static_geometry.world_vertices.as_slice(),
                ),
                usage: BufferUsages::VERTEX,
            }));
            // Dynamic impact updates can patch this independently of static vertices.
            geometry.world_effect = Some(device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("world_surface_effect_vb"),
                contents: bytemuck::cast_slice(&vec![0u32; geometry.world_vertex_count]),
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            }));
            geometry.world_index = Some(device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("iw4_exact_colour_world_ib"),
                contents: bytemuck::cast_slice(
                    source.world.static_geometry.world_indices.as_slice(),
                ),
                usage: BufferUsages::INDEX,
            }));
            geometry
                .world_surface_ranges
                .clone_from(source.world.static_geometry.world_surface_ranges.as_ref());
            geometry
                .world_cpu_indices
                .clone_from(source.world.static_geometry.world_indices.as_ref());
        }
        if !source.world.static_geometry.world_layer.is_empty() {
            geometry.world_layer = Some(device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("iw4_exact_colour_world_layer_vb"),
                contents: source.world.static_geometry.world_layer.as_slice(),
                usage: BufferUsages::VERTEX,
            }));
        }
        if !source.world.static_geometry.smodel_vertices.is_empty()
            && !source.world.static_geometry.smodel_indices.is_empty()
        {
            geometry.smodel_vertex = Some(device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("iw4_exact_colour_smodel_vb"),
                contents: bytemuck::cast_slice(
                    source.world.static_geometry.smodel_vertices.as_slice(),
                ),
                usage: BufferUsages::VERTEX,
            }));
            if source.world.static_geometry.smodel_vertex_lighting.len()
                == source.world.static_geometry.smodel_vertices.len()
            {
                geometry.smodel_vertex_lighting = Some(
                    device.create_buffer_with_data(&BufferInitDescriptor {
                        label: Some("iw4_exact_colour_smodel_vertex_lighting_vb"),
                        contents: bytemuck::cast_slice(
                            source
                                .world
                                .static_geometry
                                .smodel_vertex_lighting
                                .as_slice(),
                        ),
                        usage: BufferUsages::VERTEX,
                    }),
                );
            }
            geometry.smodel_index = Some(device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("iw4_exact_colour_smodel_ib"),
                contents: bytemuck::cast_slice(
                    source.world.static_geometry.smodel_indices.as_slice(),
                ),
                usage: BufferUsages::INDEX,
            }));
            geometry
                .smodel_surface_ranges
                .clone_from(source.world.static_geometry.smodel_surface_ranges.as_ref());
        }
    }
    geometry.generation = source.world.generation;

    upload_xmodel_streams(geometry, source, &device, &queue);
    upload_particle_cloud(geometry, source, &device, &queue);

    upload_retained_mesh(
        &mut geometry.mark_mesh,
        &mut geometry.mark_mesh_surface_ranges,
        RetainedMeshSource {
            labels: (
                "iw4_exact_colour_mark_mesh_vb",
                "iw4_exact_colour_mark_mesh_ib",
            ),
            revision: source.frame.mark_mesh_revision,
            vertices: bytemuck::cast_slice(source.frame.mark_mesh_vertices.as_slice()),
            vertex_count: source.frame.mark_mesh_vertices.len(),
            vertex_stride: asset_iw4::size::GFX_WORLD_VERTEX,
            indices: bytemuck::cast_slice(source.frame.mark_mesh_indices.as_slice()),
            index_count: source.frame.mark_mesh_indices.len(),
            index_stride: 2,
            surface_ranges: &source.frame.mark_mesh_surface_ranges,
        },
        &device,
        &queue,
    );

    upload_retained_mesh(
        &mut geometry.glass_mesh,
        &mut geometry.glass_mesh_surface_ranges,
        RetainedMeshSource {
            labels: (
                "iw4_exact_colour_glass_mesh_vb",
                "iw4_exact_colour_glass_mesh_ib",
            ),
            revision: source.frame.glass_mesh_revision,
            vertices: bytemuck::cast_slice(source.frame.glass_mesh_vertices.as_slice()),
            vertex_count: source.frame.glass_mesh_vertices.len(),
            vertex_stride: asset_iw4::size::GFX_PACKED_VERTEX,
            indices: bytemuck::cast_slice(source.frame.glass_mesh_indices.as_slice()),
            index_count: source.frame.glass_mesh_indices.len(),
            index_stride: 4,
            surface_ranges: &source.frame.glass_mesh_surface_ranges,
        },
        &device,
        &queue,
    );

    let fx_kind = colour_code_mesh_upload_kind(
        geometry.fx_revision,
        geometry.fx_vertex_count,
        geometry.fx_index_count,
        geometry.fx_vertex.is_some() && geometry.fx_index.is_some() && geometry.fx_copy_dst,
        source.frame.fx_revision,
        source.frame.fx_vertices.len(),
        source.frame.fx_indices.len(),
    );
    census.code_mesh_gpu_kind = Some(fx_kind);
    if fx_kind == 0 {
        if source.frame.fx_vertices.is_empty() || source.frame.fx_indices.is_empty() {
            geometry.fx_revision = source.frame.fx_revision;
            geometry.fx_vertex_count = source.frame.fx_vertices.len();
            geometry.fx_index_count = source.frame.fx_indices.len();
            geometry.fx_surface_ranges.clear();
        }
    } else {
        if fx_kind == 2 {
            geometry.fx_vertex = Some(device.create_buffer(&BufferDescriptor {
                label: Some("iw4_code_mesh_vb"),
                size: u64::from(CODE_MESH_VERT_CAP) * u64::from(CODE_MESH_VERT_STRIDE),
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            geometry.fx_index = Some(device.create_buffer(&BufferDescriptor {
                label: Some("iw4_code_mesh_ib"),
                size: u64::from(CODE_MESH_INDEX_CAP) * 4,
                usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            geometry.fx_copy_dst = true;
        }
        geometry.fx_revision = source.frame.fx_revision;
        geometry.fx_vertex_count = source.frame.fx_vertices.len();
        geometry.fx_index_count = source.frame.fx_indices.len();
        if !source.frame.fx_vertices.is_empty() && !source.frame.fx_indices.is_empty() {
            if let (Some(vb), Some(ib)) = (geometry.fx_vertex.as_ref(), geometry.fx_index.as_ref())
            {
                queue.write_buffer(
                    vb,
                    0,
                    bytemuck::cast_slice(source.frame.fx_vertices.as_slice()),
                );
                queue.write_buffer(
                    ib,
                    0,
                    bytemuck::cast_slice(source.frame.fx_indices.as_slice()),
                );
                geometry
                    .fx_surface_ranges
                    .clone_from(&*source.frame.fx_surface_ranges);
            }
        }
    }
    ensure_neutral_vertex_lighting(geometry, &device);
}

// Every packed stream that a T6 vertex-lit pipeline can draw from without
// its own baked colours reads this buffer at the same vertex index.
fn ensure_neutral_vertex_lighting(geometry: &mut ExactColourGeometry, device: &RenderDevice) {
    const VERTEX_BYTES: u64 = asset_iw4::size::GFX_PACKED_VERTEX as u64;
    let vertex_buffers = [
        geometry.smodel_vertex.as_ref(),
        geometry.smodel_cached_vertex.as_ref(),
        geometry.xmodel.vertex.buffer(),
        geometry.fx_vertex.as_ref(),
        geometry.particle_cloud.vertex.buffer(),
        geometry.mark_mesh.vertex.buffer(),
        geometry.glass_mesh.vertex.buffer(),
    ];
    let need = vertex_buffers
        .into_iter()
        .flatten()
        .map(|buffer| buffer.size() / VERTEX_BYTES)
        .chain([u64::from(lighting_iw4::SMC_BANK_VB_BYTES) / VERTEX_BYTES])
        .max()
        .unwrap_or(0) as usize;
    if geometry.neutral_vertex_lighting.is_some() && geometry.neutral_vertex_lighting_count >= need
    {
        return;
    }
    let count = need.next_power_of_two();
    geometry.neutral_vertex_lighting =
        Some(device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("iw4_exact_colour_neutral_vertex_lighting_vb"),
            contents: bytemuck::cast_slice(&vec![NEUTRAL_VERTEX_LIGHTING; count]),
            usage: BufferUsages::VERTEX,
        }));
    geometry.neutral_vertex_lighting_count = count;
}

// Only the custom tail changes frame to frame; the template ahead of it is
// written again only when a stream is reallocated.
fn upload_particle_cloud(
    geometry: &mut ExactColourGeometry,
    source: ExtractedColourRefs<'_>,
    device: &RenderDevice,
    queue: &RenderQueue,
) {
    let frame = source.frame;
    let vertex_count = frame.particle_cloud_vertices.len();
    let index_count = frame.particle_cloud_indices.len();
    let mesh = &mut geometry.particle_cloud;
    if vertex_count == 0 || index_count == 0 {
        mesh.vertex.set_len(0);
        mesh.index.set_len(0);
        geometry.particle_cloud_surface_ranges.clear();
        return;
    }
    let resident = mesh.uploaded_vertices == frame.particle_cloud_revision
        && mesh.vertex.len() == vertex_count
        && mesh.index.len() == index_count
        && mesh.drawable();
    if resident {
        return;
    }
    let vertex_stride = fx_iw4::GFX_POS_TEX_VERTEX_STRIDE;
    let index_stride = size_of::<u32>();
    mesh.vertex.reserve(
        device,
        "iw4_exact_colour_sparkcloud_vb",
        BufferUsages::VERTEX,
        vertex_count,
        vertex_stride,
    );
    mesh.index.reserve(
        device,
        "iw4_exact_colour_sparkcloud_ib",
        BufferUsages::INDEX,
        index_count,
        index_stride,
    );
    let template = frame.particle_cloud_template;
    let key = (mesh.vertex.allocation(), mesh.index.allocation(), template);
    let (vertex_from, index_from) = if geometry.particle_cloud_template_resident == Some(key) {
        (template.0 * vertex_stride, template.1 * index_stride)
    } else {
        (0, 0)
    };
    let vertices: &[u8] = bytemuck::cast_slice(frame.particle_cloud_vertices.as_slice());
    let indices: &[u8] = bytemuck::cast_slice(frame.particle_cloud_indices.as_slice());
    let written = write_stream_tail(&mesh.vertex, queue, vertices, vertex_from)
        && write_stream_tail(&mesh.index, queue, indices, index_from);
    if !written {
        diag::error!(
            World,
            "drawsurf geometry: sparkcloud upload skipped — revision {} is not resident",
            frame.particle_cloud_revision,
        );
        mesh.vertex.set_len(0);
        mesh.index.set_len(0);
        geometry.particle_cloud_template_resident = None;
        geometry.particle_cloud_surface_ranges.clear();
        return;
    }
    mesh.vertex.set_len(vertex_count);
    mesh.index.set_len(index_count);
    mesh.uploaded_vertices = frame.particle_cloud_revision;
    mesh.uploaded_topology = frame.particle_cloud_revision;
    geometry.particle_cloud_template_resident = Some(key);
    geometry
        .particle_cloud_surface_ranges
        .clone_from(&*frame.particle_cloud_surface_ranges);
}

#[must_use]
fn write_stream_tail(
    stream: &residency::GpuStream,
    queue: &RenderQueue,
    bytes: &[u8],
    from: usize,
) -> bool {
    let Some((start, end)) = residency::aligned_stream_span(from, bytes.len(), bytes.len()) else {
        return true;
    };
    stream.write_at(queue, start as u64, &bytes[start..end])
}

struct RetainedMeshSource<'a> {
    labels: (&'static str, &'static str),
    revision: u64,
    vertices: &'a [u8],
    vertex_count: usize,
    vertex_stride: usize,
    indices: &'a [u8],
    index_count: usize,
    index_stride: usize,
    surface_ranges: &'a [(u32, u32)],
}

fn upload_retained_mesh(
    mesh: &mut residency::GpuMesh,
    ranges: &mut Vec<(u32, u32)>,
    src: RetainedMeshSource<'_>,
    device: &RenderDevice,
    queue: &RenderQueue,
) {
    if src.vertex_count == 0 || src.index_count == 0 {
        mesh.vertex.set_len(0);
        mesh.index.set_len(0);
        mesh.uploaded_vertices = src.revision;
        mesh.uploaded_topology = src.revision;
        ranges.clear();
        return;
    }
    let resident = mesh.uploaded_vertices == src.revision
        && mesh.vertex.len() == src.vertex_count
        && mesh.index.len() == src.index_count
        && mesh.vertex.buffer().is_some()
        && mesh.index.buffer().is_some();
    if resident {
        return;
    }
    mesh.vertex.reserve(
        device,
        src.labels.0,
        BufferUsages::VERTEX,
        src.vertex_count,
        src.vertex_stride,
    );
    mesh.index.reserve(
        device,
        src.labels.1,
        BufferUsages::INDEX,
        src.index_count,
        src.index_stride,
    );
    mesh.vertex.set_len(src.vertex_count);
    mesh.index.set_len(src.index_count);
    let vertices_written = mesh.vertex.write_at(queue, 0, src.vertices);
    let indices_written = mesh.index.write_at(queue, 0, src.indices);
    if !(vertices_written && indices_written) {
        diag::error!(
            World,
            "drawsurf geometry: {} upload skipped (vertices={vertices_written} indices={indices_written}) — revision {} is not resident",
            src.labels.0,
            src.revision,
        );
        return;
    }
    mesh.uploaded_vertices = src.revision;
    mesh.uploaded_topology = src.revision;
    ranges.clear();
    ranges.extend_from_slice(src.surface_ranges);
}

fn upload_xmodel_streams(
    geometry: &mut ExactColourGeometry,
    source: ExtractedColourRefs<'_>,
    device: &RenderDevice,
    queue: &RenderQueue,
) {
    let verts = source.frame.xmodel_vertices.len();
    let indices = source.frame.xmodel_indices.len();
    if verts == 0 || indices == 0 {
        geometry.xmodel.vertex.set_len(0);
        geometry.xmodel.index.set_len(0);
        geometry.xmodel.uploaded_vertices = source.frame.xmodel_revision;
        geometry.xmodel.uploaded_topology = source.frame.xmodel_topology_revision;
        geometry.xmodel_surface_ranges.clear();
        geometry.xmodel_resident_segments.forget();
        geometry.last_xmodel_gpu_hash = None;
        return;
    }
    let policy = colour_xmodel_upload(XModelUploadQuery {
        uploaded_vertices: geometry.xmodel.uploaded_vertices,
        uploaded_topology: geometry.xmodel.uploaded_topology,
        resident_verts: geometry.xmodel.vertex.len(),
        resident_indices: geometry.xmodel.index.len(),
        vertex_fits: geometry.xmodel.vertex.holds(verts),
        index_fits: geometry.xmodel.index.holds(indices),
        cpu_vertices_revision: source.frame.xmodel_revision,
        cpu_topology_revision: source.frame.xmodel_topology_revision,
        cpu_verts: verts,
        cpu_indices: indices,
    });
    let counts_stable = geometry.xmodel.vertex.len() == verts;
    let (write_vertices, write_indices) = match policy {
        XModelUpload::Resident => return,
        XModelUpload::Rewrite { vertices, indices } => (vertices, indices),
        XModelUpload::Grow => {
            geometry.xmodel.vertex.reserve(
                device,
                "iw4_exact_colour_xmodel_vb",
                BufferUsages::VERTEX,
                verts,
                asset_iw4::size::GFX_PACKED_VERTEX,
            );
            geometry.xmodel.index.reserve(
                device,
                "iw4_exact_colour_xmodel_ib",
                BufferUsages::INDEX,
                indices,
                4,
            );
            (true, true)
        }
    };
    geometry.xmodel.vertex.set_len(verts);
    geometry.xmodel.index.set_len(indices);
    if write_vertices {
        upload_xmodel_vertices(
            geometry,
            source,
            queue,
            counts_stable,
            asset_iw4::size::GFX_PACKED_VERTEX,
        );
    }
    if write_indices {
        if geometry.xmodel.index.write_at(
            queue,
            0,
            bytemuck::cast_slice(source.frame.xmodel_indices.as_slice()),
        ) {
            geometry.xmodel.uploaded_topology = source.frame.xmodel_topology_revision;
            geometry
                .xmodel_surface_ranges
                .clone_from(&*source.frame.xmodel_surface_ranges);
        } else {
            diag::error!(
                World,
                "drawsurf geometry: xmodel index upload skipped — topology revision {} is not resident",
                source.frame.xmodel_topology_revision,
            );
        }
    }
    geometry.last_xmodel_gpu_hash = None;
}

fn upload_xmodel_vertices(
    geometry: &mut ExactColourGeometry,
    source: ExtractedColourRefs<'_>,
    queue: &RenderQueue,
    counts_stable: bool,
    stride: usize,
) {
    let bytes: &[u8] = bytemuck::cast_slice(source.frame.xmodel_vertices.as_slice());
    let published = source.frame.xmodel_packed_segments;
    let resident = geometry.xmodel_resident_segments;

    let allocation = geometry.xmodel.vertex.allocation();
    let segmented = counts_stable
        && published.live
        && resident.live
        && published.layout == resident.layout
        && geometry.xmodel_resident_allocation == allocation;
    if !segmented {
        if !geometry.xmodel.vertex.write_at(queue, 0, bytes) {
            diag::error!(
                World,
                "drawsurf geometry: xmodel vertex upload skipped — revision {} is not resident",
                source.frame.xmodel_revision,
            );
            return;
        }
        geometry.xmodel.uploaded_vertices = source.frame.xmodel_revision;
        geometry.xmodel_resident_segments = published;
        geometry.xmodel_resident_allocation = allocation;
        return;
    }
    let mut spans = [(0usize, 0usize); render_frame::PACKED_SEGMENT_OWNERS];
    let mut n = 0;
    for (was, now) in resident.owners.iter().zip(published.owners.iter()) {
        if was.revision == now.revision || now.rows == 0 {
            continue;
        }
        let start = now.start as usize * stride;
        let end = start + now.rows as usize * stride;
        if let Some(span) = residency::aligned_stream_span(start, end, bytes.len()) {
            spans[n] = span;
            n += 1;
        }
    }
    let spans = coalesce_stream_spans(&mut spans[..n]);
    for &(start, end) in spans.iter() {
        if !geometry
            .xmodel
            .vertex
            .write_at(queue, start as u64, &bytes[start..end])
        {
            diag::error!(
                World,
                "drawsurf geometry: xmodel vertex segment [{start}, {end}) upload skipped — revision {} is not resident",
                source.frame.xmodel_revision,
            );
            geometry.xmodel_resident_segments = render_frame::PackedSegments::default();
            return;
        }
    }
    geometry.xmodel.uploaded_vertices = source.frame.xmodel_revision;
    geometry.xmodel_resident_segments = published;
    geometry.xmodel_resident_allocation = allocation;
}

fn coalesce_stream_spans(spans: &mut [(usize, usize)]) -> &[(usize, usize)] {
    if spans.is_empty() {
        return spans;
    }
    spans.sort_unstable_by_key(|span| span.0);
    let mut merged = 0;
    for i in 1..spans.len() {
        let (start, end) = spans[i];
        if start <= spans[merged].1 {
            spans[merged].1 = spans[merged].1.max(end);
        } else {
            merged += 1;
            spans[merged] = (start, end);
        }
    }
    &spans[..merged + 1]
}
