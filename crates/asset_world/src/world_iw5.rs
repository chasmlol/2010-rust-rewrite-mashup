use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use dpvs_iw4::{AabbNodeView, Bounds, CPlane, SurfRange};
use fastfile_iw5::size as sz;
use fastfile_iw5::{GfxImageGeometry, GfxWorldGeometry, MAX_LIGHTMAP_PAGES, ZonePtr, ZoneStream};

use crate::world_draw::{
    CameraRangeKind, CameraSurfRange, CameraSurfRanges, CapturedLightDef, DpvsWorldData,
    OwnedPortal, SurfaceDrawFields, WorldDraw, WorldLightRegionHull, WorldLightmap,
    WorldLightmapGap, WorldPrimaryLight, WorldReflectionProbe, WorldVertexPayload,
    make_material_batches, primary_mask_payload, scatter_vertex_layer_type3,
    secondary_page_payloads,
};
use crate::world_mesh::{
    WorldMeshError, WorldMeshStats, normalize_or_up, unpack_color, unpack_unit_vec,
};
use crate::{SurfaceCastsSunShadow, world_capture_from_casters};
use asset_material::MaterialCatalog;

fn iw4_ptr(p: fastfile_iw5::Ptr) -> fastfile_iw4::Ptr {
    fastfile_iw4::Ptr {
        block: p.block,
        offset: p.offset,
    }
}

pub fn build_iw5_world_draw(
    s: &ZoneStream<'_>,
    geometry: GfxWorldGeometry,
    materials: MaterialCatalog,
) -> Result<(WorldDraw, MaterialCatalog), WorldMeshError> {
    let (Some(vertices), Some(indices)) = (geometry.vertices, geometry.indices) else {
        return Err(WorldMeshError::NoGeometry);
    };
    let Some(surfaces) = geometry.surfaces else {
        return Err(WorldMeshError::NoGeometry);
    };

    let mut packed_vertices = Vec::with_capacity(geometry.vertex_count);
    let mut positions = Vec::with_capacity(geometry.vertex_count);
    let mut normals = Vec::with_capacity(geometry.vertex_count);
    let mut tangents = Vec::with_capacity(geometry.vertex_count);
    let mut colors = Vec::with_capacity(geometry.vertex_count);
    let mut texture_uvs = Vec::with_capacity(geometry.vertex_count);
    let mut lightmap_uvs = Vec::with_capacity(geometry.vertex_count);
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];

    for i in 0..geometry.vertex_count {
        let v = vertices.at(i * sz::GFX_WORLD_VERTEX);
        let mut packed = [0u8; sz::GFX_WORLD_VERTEX];
        for (offset, byte) in packed.iter_mut().enumerate() {
            *byte = s.u8_at(v, offset).map_err(WorldMeshError::from)?;
        }
        packed_vertices.push(packed);
        let xyz = [
            s.f32_at(v, 0).map_err(WorldMeshError::from)?,
            s.f32_at(v, 4).map_err(WorldMeshError::from)?,
            s.f32_at(v, 8).map_err(WorldMeshError::from)?,
        ];
        for axis in 0..3 {
            min[axis] = min[axis].min(xyz[axis]);
            max[axis] = max[axis].max(xyz[axis]);
        }
        positions.push(xyz);
        normals.push(normalize_or_up(unpack_unit_vec(
            s.u32_at(v, 36).map_err(WorldMeshError::from)?,
        )));
        let tangent = normalize_or_up(unpack_unit_vec(
            s.u32_at(v, 40).map_err(WorldMeshError::from)?,
        ));
        tangents.push([
            tangent[0],
            tangent[1],
            tangent[2],
            s.f32_at(v, 12).map_err(WorldMeshError::from)?,
        ]);
        colors.push(unpack_color(s.u32_at(v, 16).map_err(WorldMeshError::from)?));
        texture_uvs.push([
            s.f32_at(v, 20).map_err(WorldMeshError::from)?,
            s.f32_at(v, 24).map_err(WorldMeshError::from)?,
        ]);
        lightmap_uvs.push([
            s.f32_at(v, 28).map_err(WorldMeshError::from)?,
            s.f32_at(v, 32).map_err(WorldMeshError::from)?,
        ]);
    }

    let mut packed_indices: Vec<u32> = Vec::new();
    let mut surface_index_ranges = Vec::with_capacity(geometry.surface_count);
    let mut authored_lightmapped = Vec::with_capacity(geometry.surface_count);
    let mut surface_lightmap_indices = Vec::with_capacity(geometry.surface_count);
    let mut surface_materials = Vec::with_capacity(geometry.surface_count);
    let mut surface_primary_lights = Vec::with_capacity(geometry.surface_count);
    let mut surface_reflection_probes = Vec::with_capacity(geometry.surface_count);
    let mut surface_casts_sun_shadow = SurfaceCastsSunShadow::with_len(geometry.surface_count);
    let mut surface_vertex_layer = Vec::with_capacity(geometry.surface_count);
    let mut surface_first_vertex = Vec::with_capacity(geometry.surface_count);
    let mut surface_vertex_count = Vec::with_capacity(geometry.surface_count);
    let mut surface_draw_fields = Vec::with_capacity(geometry.surface_count);
    let mut skipped_surfaces = 0usize;
    let mut skipped_sky_surfaces = 0usize;
    let mut skipped_shadowcaster_surfaces = 0usize;
    let mut unrouted_surfaces = 0usize;
    let mut undecided_state_bits_surfaces = 0usize;

    for i in 0..geometry.surface_count {
        let surface = surfaces.at(i * s.layout(sz::GFX_SURFACE, 32));
        let laf = s.layout(20, 24);
        let vertex_layer_data = s.i32_at(surface, 0).map_err(WorldMeshError::from)?;
        let first_vertex = s.u32_at(surface, 4).map_err(WorldMeshError::from)? as usize;
        let vertex_count = s.u16_at(surface, 8).map_err(WorldMeshError::from)? as usize;
        let tri_count = s.u16_at(surface, 10).map_err(WorldMeshError::from)? as usize;
        let base_index = s.u32_at(surface, 12).map_err(WorldMeshError::from)? as usize;
        let lightmap_index = s.u8_at(surface, laf).map_err(WorldMeshError::from)? as usize;
        let material = materials.material_index(iw4_ptr(surface.at(16)));
        let authored = material.and_then(|index| materials.materials.get(index.get()));
        let pass = crate::world_draw::surface_pass(&materials, authored);
        let is_sky = pass.sky;
        let is_shadowcaster = pass.shadow_only;
        if is_sky {
            skipped_sky_surfaces += 1;
        }
        unrouted_surfaces += usize::from(pass.unrouted);
        undecided_state_bits_surfaces += usize::from(pass.state_bits_undecided);
        authored_lightmapped
            .push(lightmap_index < geometry.lightmap_count && pass.takes_lightmap());
        surface_lightmap_indices.push(lightmap_index.min(255) as u8);
        surface_materials.push(material.map(|i| i.get()));
        surface_primary_lights.push(s.u8_at(surface, laf + 2).map_err(WorldMeshError::from)?);
        surface_reflection_probes.push(s.u8_at(surface, laf + 1).map_err(WorldMeshError::from)?);

        if s.u8_at(surface, laf + 3).map_err(WorldMeshError::from)? != 0 {
            surface_casts_sun_shadow.set(i);
        }
        surface_vertex_layer.push(vertex_layer_data);
        surface_first_vertex.push(first_vertex as u32);
        surface_vertex_count.push(vertex_count as u32);
        surface_draw_fields.push(SurfaceDrawFields {
            first_vertex: first_vertex as u32,
            tri_count: tri_count as u16,
            base_index: base_index as u32,
            lightmap_index: lightmap_index.min(255) as u8,
            reflection_probe_index: s.u8_at(surface, laf + 1).map_err(WorldMeshError::from)?,
            primary_light_index: s.u8_at(surface, laf + 2).map_err(WorldMeshError::from)?,
        });
        let range_start = packed_indices.len() as u32;

        if is_shadowcaster {
            skipped_shadowcaster_surfaces += 1;
            surface_index_ranges.push((range_start, 0));
            continue;
        }

        if first_vertex + vertex_count > geometry.vertex_count
            || base_index + tri_count * 3 > geometry.index_count
        {
            skipped_surfaces += 1;
            surface_index_ranges.push((range_start, 0));
            continue;
        }

        let mut ok = true;
        for t in 0..tri_count * 3 {
            let local = s
                .u16_at(indices, (base_index + t) * 2)
                .map_err(WorldMeshError::from)? as usize;
            let absolute = first_vertex + local;
            if absolute >= geometry.vertex_count {
                skipped_surfaces += 1;
                ok = false;
                break;
            }
            packed_indices.push(absolute as u32);
        }
        let count = if ok {
            packed_indices.len() as u32 - range_start
        } else {
            packed_indices.truncate(range_start as usize);
            0
        };
        surface_index_ranges.push((range_start, count));
    }

    let packed_layer = match (geometry.vertex_layer, geometry.vertex_layer_size) {
        (Some(ptr), n) if n > 0 => s
            .slice_at(ptr, 0, n)
            .map_err(WorldMeshError::from)?
            .to_vec(),
        _ => Vec::new(),
    };
    let vertex_layer = scatter_vertex_layer_type3(
        &packed_layer,
        geometry.vertex_count,
        &surface_first_vertex,
        &surface_vertex_count,
        &surface_vertex_layer,
    );

    let stats = WorldMeshStats {
        vertices: positions.len(),
        triangles: packed_indices.len() / 3,
        surfaces: geometry.surface_count,
        skipped_surfaces: skipped_surfaces + skipped_shadowcaster_surfaces,
        sky_surfaces: skipped_sky_surfaces,

        sky_material: None,
        unrouted_surfaces,
        undecided_state_bits_surfaces,
        min,
        max,

        bounds: geometry.bounds.map(|bits| bits.map(f32::from_bits)),
    };

    let lightmap = decode_lightmaps(s, geometry);
    let surface_lightmapped: Vec<bool> = match &lightmap {
        Ok(pages) => authored_lightmapped
            .into_iter()
            .zip(surface_lightmap_indices.iter().copied())
            .map(|(authored, index)| {
                authored && pages.get(index as usize).and_then(|p| p.as_ref()).is_some()
            })
            .collect(),
        Err(_) => vec![false; geometry.surface_count],
    };
    let (batches, surface_batch_ranges) = make_material_batches(
        &positions,
        &normals,
        &tangents,
        &colors,
        &texture_uvs,
        &lightmap_uvs,
        &packed_indices,
        &surface_index_ranges,
        &surface_materials,
        &surface_lightmapped,
        &surface_lightmap_indices,
        &surface_primary_lights,
        &surface_reflection_probes,
    );
    let primary_lights = extract_primary_lights(s, geometry.sun_primary_light_count)?;
    let light_defs = capture_iw5_light_defs(s, &materials);
    let light_region_hulls = extract_iw5_light_regions(s, geometry)?;
    let reflection_probes = extract_iw5_reflection_probes(s, geometry, &materials)?;
    let dpvs = extract_iw5_dpvs(s, geometry)?;

    let outdoor_image_name = geometry.outdoor_image.and_then(|slot| {
        materials.image_index(iw4_ptr(slot)).and_then(|index| {
            materials
                .images
                .get(index)
                .map(|image| image.name.to_string())
        })
    });
    Ok((
        WorldDraw {
            sky_model: None,
            batches,
            lightmap,
            stats,
            packed_vertices: WorldVertexPayload::Iw5(packed_vertices),
            vertex_layer,
            surface_vertex_layer,
            surface_first_vertex,
            surface_draw_fields,
            positions,
            normals,
            tangents,
            colors,
            texture_uvs,
            lightmap_uvs,
            packed_indices,
            surface_index_ranges,
            surface_batch_ranges,
            surface_lightmapped,
            surface_lightmap_indices,
            surface_reflection_probes,
            surface_primary_lights,
            sort_key_distortion: None,
            capture: world_capture_from_casters(surface_casts_sun_shadow),
            brush_models: Vec::new(),
            brush_model_bounds: Vec::new(),
            surface_materials,
            primary_lights,
            light_defs,
            sun_primary_light_count: geometry.sun_primary_light_count as u32,
            sun_stages: crate::sun_stages_iw5(s),
            light_region_hulls,
            shadow_geometry: Vec::new(),
            reflection_probes,
            dpvs,
            outdoor_image_name,
            outdoor_image: None,
            outdoor_lookup: geometry.outdoor_lookup,
            sun_effects: None,
            t6_exposure: None,
            t5_sun_parse_exposure: None,
            sky_dynamic_intensity: None,
            t5_sun_light: None,
            t5_tree_scatter_intensity: None,
            t5_tree_scatter_amount: None,
            t5_exposure_volume_count: 0,
        },
        materials,
    ))
}

fn aabb_children_offset(
    s: &fastfile_iw5::ZoneStream<'_>,
    node: fastfile_iw5::Ptr,
) -> Result<i32, WorldMeshError> {
    let offset = s
        .i32_at(node, s.layout(40, 48))
        .map_err(WorldMeshError::from)?;
    let stride = s.layout(sz::GFX_AABB_TREE, 56) as i32;
    if offset <= 0 {
        return Ok(offset);
    }
    if offset % stride != 0 {
        return Err(WorldMeshError::InvalidAabbChildrenOffset { offset, stride });
    }
    Ok(offset / stride * dpvs_iw4::AABB_NODE_STRIDE as i32)
}

fn extract_iw5_dpvs(
    s: &ZoneStream<'_>,
    g: GfxWorldGeometry,
) -> Result<DpvsWorldData, WorldMeshError> {
    let mut out = DpvsWorldData::new(CameraSurfRanges::new(
        CameraSurfRange {
            kind: CameraRangeKind::LitOpaque,
            begin: g.lit_surfs_begin,
            end: g.lit_surfs_end,
        },
        vec![
            CameraSurfRange {
                kind: CameraRangeKind::LitTrans,
                begin: g.lit_trans_surfs_begin,
                end: g.lit_trans_surfs_end,
            },
            CameraSurfRange {
                kind: CameraRangeKind::Emissive,
                begin: g.emissive_surfs_begin,
                end: g.emissive_surfs_end,
            },
        ],
    ));
    out.cell_count = g.cell_count;
    out.lit_opaque_begin = g.lit_surfs_begin;
    out.lit_opaque_end = g.lit_surfs_end;
    out.emissive_surfs_begin = g.emissive_surfs_begin;
    out.emissive_surfs_end = g.emissive_surfs_end;

    out.static_surface_count = g.static_surface_count;
    out.static_surface_count_no_decal = g.static_surface_count_no_decal;

    if let (Some(planes_ptr), Some(nodes_ptr)) = (g.planes, g.nodes) {
        out.planes.reserve(g.plane_count);
        for i in 0..g.plane_count {
            let p = planes_ptr.at(i * sz::CPLANE);
            out.planes.push(CPlane {
                normal: [
                    s.f32_at(p, 0).map_err(WorldMeshError::from)?,
                    s.f32_at(p, 4).map_err(WorldMeshError::from)?,
                    s.f32_at(p, 8).map_err(WorldMeshError::from)?,
                ],
                dist: s.f32_at(p, 12).map_err(WorldMeshError::from)?,
                r#type: s.u8_at(p, 16).map_err(WorldMeshError::from)?,
            });
        }
        out.nodes.reserve(g.node_count);
        for i in 0..g.node_count {
            out.nodes
                .push(s.u16_at(nodes_ptr, i * 2).map_err(WorldMeshError::from)?);
        }
    }

    if let Some(sorted_ptr) = g.sorted_surf_index {
        let n = g.static_surface_count + g.static_surface_count_no_decal;
        out.sorted_surf_index.reserve(n);
        for i in 0..n {
            out.sorted_surf_index
                .push(s.u16_at(sorted_ptr, i * 2).map_err(WorldMeshError::from)?);
        }
    }

    if let Some(bounds_ptr) = g.surfaces_bounds {
        out.surface_bounds.reserve(g.surface_count);
        for i in 0..g.surface_count {
            out.surface_bounds
                .push(read_bounds(s, bounds_ptr.at(i * sz::GFX_SURFACE_BOUNDS))?);
        }
    }
    if let Some(skies_ptr) = g.skies {
        for i in 0..g.sky_count {
            let sky = skies_ptr.at(i * s.layout(sz::GFX_SKY, 32));
            let count = s.i32_at(sky, 0).map_err(WorldMeshError::from)?.max(0) as usize;
            let ZonePtr::Offset(arr) = s
                .ptr_at(sky, s.layout(4, 8))
                .map_err(WorldMeshError::from)?
            else {
                continue;
            };
            for j in 0..count {
                out.sky_start_surfs
                    .push(s.u32_at(arr, j * 4).map_err(WorldMeshError::from)?);
            }
        }
    }
    if let Some(insts_ptr) = g.smodel_insts {
        out.smodel_bounds.reserve(g.smodel_count);
        for i in 0..g.smodel_count {
            out.smodel_bounds
                .push(read_bounds(s, insts_ptr.at(i * sz::GFX_STATIC_MODEL_INST))?);
        }
    }

    out.cell_roots = vec![SurfRange::default(); g.cell_count];
    out.aabb_trees = vec![Vec::new(); g.cell_count];
    out.aabb_smodel_indices = vec![Vec::new(); g.cell_count];
    if let (Some(counts_ptr), Some(trees_ptr)) = (g.aabb_tree_counts, g.aabb_trees) {
        for i in 0..g.cell_count {
            let count = s
                .i32_at(counts_ptr, i * 4)
                .map_err(WorldMeshError::from)?
                .max(0) as usize;
            if count == 0 {
                continue;
            }
            let tree = match s
                .ptr_at(trees_ptr, i * s.pointer_bytes())
                .map_err(WorldMeshError::from)?
            {
                ZonePtr::Offset(p) => p,
                _ => continue,
            };
            out.cell_roots[i] = SurfRange {
                start: s.u16_at(tree, 28).map_err(WorldMeshError::from)?,
                count: s.u16_at(tree, 26).map_err(WorldMeshError::from)?,
            };
            let mut nodes = Vec::with_capacity(count);
            let mut smodel_indices: Vec<u16> = Vec::with_capacity(count);
            for j in 0..count {
                let n = tree.at(j * s.layout(sz::GFX_AABB_TREE, 56));
                let smodel_count = s.u16_at(n, 34).map_err(WorldMeshError::from)? as usize;
                let smodel_index_start = smodel_indices.len() as u32;
                let mut smodel_index_count = 0u16;
                if let ZonePtr::Offset(indexes) = s
                    .ptr_at(n, s.layout(36, 40))
                    .map_err(WorldMeshError::from)?
                {
                    for k in 0..smodel_count {
                        smodel_indices
                            .push(s.u16_at(indexes, k * 2).map_err(WorldMeshError::from)?);
                        smodel_index_count += 1;
                    }
                }
                nodes.push(AabbNodeView {
                    bounds: Bounds::from_mid_half(
                        [
                            s.f32_at(n, 0).map_err(WorldMeshError::from)?,
                            s.f32_at(n, 4).map_err(WorldMeshError::from)?,
                            s.f32_at(n, 8).map_err(WorldMeshError::from)?,
                        ],
                        [
                            s.f32_at(n, 12).map_err(WorldMeshError::from)?,
                            s.f32_at(n, 16).map_err(WorldMeshError::from)?,
                            s.f32_at(n, 20).map_err(WorldMeshError::from)?,
                        ],
                    ),
                    child_count: s.u16_at(n, 24).map_err(WorldMeshError::from)?,
                    children_offset: aabb_children_offset(s, n)?,
                    start_surf: s.u16_at(n, 28).map_err(WorldMeshError::from)?,
                    surface_count: s.u16_at(n, 26).map_err(WorldMeshError::from)?,

                    start_surf_no_decal: 0,
                    surface_count_no_decal: 0,
                    smodel_index_start,
                    smodel_index_count,
                });
            }
            out.aabb_trees[i] = nodes;
            out.aabb_smodel_indices[i] = smodel_indices;
        }
    }

    if let Some(cells_ptr) = g.cells {
        out.portals_per_cell = Vec::with_capacity(g.cell_count);
        out.cell_reflection_probes = Vec::with_capacity(g.cell_count);
        for i in 0..g.cell_count {
            let cell = cells_ptr.at(i * s.layout(sz::GFX_CELL, 72));
            let portal_count = s.i32_at(cell, 0x18).map_err(WorldMeshError::from)?.max(0) as usize;
            let mut cell_portals = Vec::with_capacity(portal_count);
            let portals_ptr = match s
                .ptr_at(cell, s.layout(0x1c, 32))
                .map_err(WorldMeshError::from)?
            {
                ZonePtr::Offset(p) => Some(p),
                ZonePtr::Null => None,
                _ => None,
            };
            if let Some(arr) = portals_ptr {
                for j in 0..portal_count {
                    let portal = arr.at(j * s.layout(sz::GFX_PORTAL, 80));
                    let plane_off = s.layout(12, 24);
                    let plane = [
                        s.f32_at(portal, plane_off).map_err(WorldMeshError::from)?,
                        s.f32_at(portal, plane_off + 4)
                            .map_err(WorldMeshError::from)?,
                        s.f32_at(portal, plane_off + 8)
                            .map_err(WorldMeshError::from)?,
                        s.f32_at(portal, plane_off + 12)
                            .map_err(WorldMeshError::from)?,
                    ];
                    let neighbor = s
                        .u16_at(portal, s.layout(32, 48))
                        .map_err(WorldMeshError::from)?;
                    let vertex_count =
                        s.u8_at(portal, s.layout(0x22, 50))
                            .map_err(WorldMeshError::from)? as usize;
                    let vert_start = out.portal_verts.len();
                    if let ZonePtr::Offset(verts) = s
                        .ptr_at(portal, s.layout(0x1c, 40))
                        .map_err(WorldMeshError::from)?
                    {
                        for k in 0..vertex_count {
                            let v = verts.at(k * 12);
                            out.portal_verts.push([
                                s.f32_at(v, 0).map_err(WorldMeshError::from)?,
                                s.f32_at(v, 4).map_err(WorldMeshError::from)?,
                                s.f32_at(v, 8).map_err(WorldMeshError::from)?,
                            ]);
                        }
                    }
                    let axis_off = s.layout(sz::GFX_PORTAL_HULL_AXIS, 52);
                    let mut hull_axis = [[0.0f32; 3]; 2];
                    for (i, value) in hull_axis.as_flattened_mut().iter_mut().enumerate() {
                        *value = s
                            .f32_at(portal, axis_off + i * 4)
                            .map_err(WorldMeshError::from)?;
                    }
                    cell_portals.push(OwnedPortal {
                        plane,
                        neighbor,
                        vert_start,
                        vert_count: out.portal_verts.len() - vert_start,
                        hull_axis: Some(hull_axis),
                    });
                }
            }
            out.portals_per_cell.push(cell_portals);
            let probe_count = s
                .u8_at(cell, s.layout(0x20, 40))
                .map_err(WorldMeshError::from)? as usize;
            let mut probes = Vec::with_capacity(probe_count);
            if let ZonePtr::Offset(arr) = s
                .ptr_at(cell, s.layout(0x24, 48))
                .map_err(WorldMeshError::from)?
            {
                for k in 0..probe_count {
                    probes.push(s.u8_at(arr, k).map_err(WorldMeshError::from)?);
                }
            }
            out.cell_reflection_probes.push(probes);
        }
    }

    out.checked()
}

fn read_bounds(s: &ZoneStream<'_>, p: fastfile_iw5::Ptr) -> Result<Bounds, WorldMeshError> {
    Ok(Bounds::from_mid_half(
        [
            s.f32_at(p, 0).map_err(WorldMeshError::from)?,
            s.f32_at(p, 4).map_err(WorldMeshError::from)?,
            s.f32_at(p, 8).map_err(WorldMeshError::from)?,
        ],
        [
            s.f32_at(p, 12).map_err(WorldMeshError::from)?,
            s.f32_at(p, 16).map_err(WorldMeshError::from)?,
            s.f32_at(p, 20).map_err(WorldMeshError::from)?,
        ],
    ))
}

fn extract_iw5_reflection_probes(
    s: &ZoneStream<'_>,
    geometry: GfxWorldGeometry,
    materials: &MaterialCatalog,
) -> Result<Vec<WorldReflectionProbe>, WorldMeshError> {
    let (Some(images), Some(origins)) = (
        geometry.reflection_probes,
        geometry.reflection_probe_origins,
    ) else {
        return Ok(Vec::new());
    };
    let mut probes = Vec::with_capacity(geometry.reflection_probe_count);
    for i in 0..geometry.reflection_probe_count {
        let origin = origins.at(i * 12);
        probes.push(WorldReflectionProbe {
            image: materials.image_index(iw4_ptr(images.at(i * s.pointer_bytes()))),
            lighting_sh: None,
            origin: [
                s.f32_at(origin, 0).map_err(WorldMeshError::from)?,
                s.f32_at(origin, 4).map_err(WorldMeshError::from)?,
                s.f32_at(origin, 8).map_err(WorldMeshError::from)?,
            ],
        });
    }
    Ok(probes)
}

fn decode_lightmaps(
    s: &ZoneStream<'_>,
    geometry: GfxWorldGeometry,
) -> Result<Vec<Option<WorldLightmap>>, WorldLightmapGap> {
    if geometry.lightmap_count == 0 {
        return Err(WorldLightmapGap::Missing);
    }
    let retain = geometry.lightmap_count.min(MAX_LIGHTMAP_PAGES);
    let mut pages = Vec::with_capacity(retain);
    let mut decoded = 0usize;
    let mut last_err = None;
    for i in 0..retain {
        let pair = geometry.lightmaps[i];
        match decode_lightmap_pair(s, pair.primary, pair.secondary) {
            Ok(page) => {
                pages.push(Some(page));
                decoded += 1;
            }
            Err(err) => {
                pages.push(None);
                last_err = Some(err);
            }
        }
    }
    if decoded == 0 {
        return Err(last_err.unwrap_or(WorldLightmapGap::Missing));
    }
    Ok(pages)
}

fn decode_lightmap_pair(
    s: &ZoneStream<'_>,
    primary: Option<GfxImageGeometry>,
    secondary: Option<GfxImageGeometry>,
) -> Result<WorldLightmap, WorldLightmapGap> {
    let image = secondary.ok_or(WorldLightmapGap::MissingSecondary)?;
    let primary = primary.ok_or(WorldLightmapGap::MissingPrimary)?;
    decode_lightmap_images(s, primary, image)
}

fn decode_lightmap_images(
    s: &ZoneStream<'_>,
    primary: GfxImageGeometry,
    image: GfxImageGeometry,
) -> Result<WorldLightmap, WorldLightmapGap> {
    if image.width == 0 || image.height < 2 || image.height % 2 != 0 || image.depth != 1 {
        return Err(WorldLightmapGap::InvalidDimensions {
            width: image.width,
            height: image.height,
            depth: image.depth,
        });
    }
    if image.format != 21 {
        return Err(WorldLightmapGap::UnsupportedSecondaryFormat(image.format));
    }
    let page_height = usize::from(image.height / 2);
    let row_bytes = usize::from(image.width) * 4;
    let needed = row_bytes * usize::from(image.height);
    if image.source_len < needed {
        return Err(WorldLightmapGap::Truncated {
            needed,
            have: image.source_len,
        });
    }
    let source = s
        .source_slice(image.source_offset, image.source_len)
        .map_err(|_| WorldLightmapGap::Truncated {
            needed,
            have: image.source_len,
        })?;
    let (ambient_rgba, directional_rgba) = secondary_page_payloads(source, row_bytes, page_height)?;
    let mut secondary_rgba = Vec::with_capacity(needed);
    secondary_rgba.extend_from_slice(&ambient_rgba);
    secondary_rgba.extend_from_slice(&directional_rgba);
    let mut secondary_image = Image::new(
        Extent3d {
            width: u32::from(image.width),
            height: u32::from(image.height),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        secondary_rgba,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    secondary_image.sampler = ImageSampler::linear();
    let mut ambient_image = Image::new(
        Extent3d {
            width: u32::from(image.width),
            height: page_height as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        ambient_rgba,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    ambient_image.sampler = ImageSampler::linear();
    let mut directional_image = Image::new(
        Extent3d {
            width: u32::from(image.width),
            height: page_height as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        directional_rgba,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    directional_image.sampler = ImageSampler::linear();
    let ambient_source_name = image
        .name
        .and_then(|name| s.cstr(name).ok())
        .unwrap_or("<unnamed lightmap secondary>")
        .to_owned();
    if primary.format != 50 {
        return Err(WorldLightmapGap::UnsupportedPrimaryFormat(primary.format));
    }
    if primary.width == 0 || primary.height == 0 || primary.depth != 1 {
        return Err(WorldLightmapGap::InvalidDimensions {
            width: primary.width,
            height: primary.height,
            depth: primary.depth,
        });
    }
    let primary_size = UVec2::new(u32::from(primary.width), u32::from(primary.height));
    let secondary_size = UVec2::new(u32::from(image.width), page_height as u32);
    if primary_size.x * secondary_size.y != primary_size.y * secondary_size.x {
        return Err(WorldLightmapGap::MismatchedAspectRatio {
            primary: primary_size,
            secondary: secondary_size,
        });
    }
    let mask_source = s
        .source_slice(primary.source_offset, primary.source_len)
        .map_err(|_| WorldLightmapGap::Truncated {
            needed: usize::from(primary.width) * usize::from(primary.height),
            have: primary.source_len,
        })?;
    let mask_source = primary_mask_payload(mask_source, primary.width, primary.height)?;
    let mut primary_image = Image::new(
        Extent3d {
            width: u32::from(primary.width),
            height: u32::from(primary.height),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        mask_source.to_vec(),
        TextureFormat::R8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    primary_image.sampler = ImageSampler::linear();
    let sun_mask_image = primary_image.clone();
    let sun_mask_source_name = primary
        .name
        .and_then(|name| s.cstr(name).ok())
        .unwrap_or("<unnamed lightmap primary>")
        .to_owned();
    Ok(WorldLightmap {
        primary_image: Some(primary_image),
        secondary_image: Some(secondary_image),
        secondary_b_image: None,
        ambient_image,
        directional_image,
        sun_mask_image,
        ambient_source_name,
        sun_mask_source_name,
        ambient_size: secondary_size,
        sun_mask_size: primary_size,
    })
}

fn extract_iw5_light_regions(
    s: &ZoneStream<'_>,
    geometry: GfxWorldGeometry,
) -> Result<Option<Vec<Vec<WorldLightRegionHull>>>, WorldMeshError> {
    let Some(regions) = geometry.light_regions else {
        return Ok(None);
    };
    let mut out = Vec::with_capacity(geometry.primary_light_count);
    for i in 0..geometry.primary_light_count {
        let region = regions.at(i * s.layout(sz::GFX_LIGHT_REGION, 16));
        let hull_count = s.u32_at(region, 0)? as usize;
        let mut hulls = Vec::with_capacity(hull_count);
        if hull_count > 0 {
            let ZonePtr::Offset(arr) = s.ptr_at(region, s.layout(4, 8))? else {
                return Err(WorldMeshError::NoGeometry);
            };
            for j in 0..hull_count {
                let hull = arr.at(j * s.layout(sz::GFX_LIGHT_REGION_HULL, 88));
                let mut kdop_mid = [0.0_f32; 9];
                let mut kdop_half = [0.0_f32; 9];
                for k in 0..9 {
                    kdop_mid[k] = s.f32_at(hull, k * 4)?;
                    kdop_half[k] = s.f32_at(hull, 36 + k * 4)?;
                }
                let axis_count = s.u32_at(hull, 72)? as usize;
                let mut axes = Vec::with_capacity(axis_count);
                if axis_count > 0 {
                    let ZonePtr::Offset(axis_arr) = s.ptr_at(hull, s.layout(76, 80))? else {
                        return Err(WorldMeshError::NoGeometry);
                    };
                    for k in 0..axis_count {
                        let a = axis_arr.at(k * 20);
                        axes.push(lighting_iw4::LightRegionAxis {
                            dir: [s.f32_at(a, 0)?, s.f32_at(a, 4)?, s.f32_at(a, 8)?],
                            mid: s.f32_at(a, 12)?,
                            half: s.f32_at(a, 16)?,
                        });
                    }
                }
                hulls.push(WorldLightRegionHull {
                    kdop_mid,
                    kdop_half,
                    axes,
                });
            }
        }
        out.push(hulls);
    }
    Ok(Some(out))
}

fn extract_primary_lights(
    s: &ZoneStream<'_>,
    sun_primary_light_count: usize,
) -> Result<Vec<WorldPrimaryLight>, WorldMeshError> {
    let Some(world) = s.com_world() else {
        return Ok(Vec::new());
    };
    let Some(lights) = world.primary_lights else {
        return Ok(Vec::new());
    };
    let mut output = Vec::with_capacity(world.primary_light_count);
    for index in 0..world.primary_light_count {
        let light = lights.at(index * s.layout(sz::COM_PRIMARY_LIGHT, 88));
        output.push(WorldPrimaryLight {
            is_sun: index != 0 && index <= sun_primary_light_count,
            light_type: s.u8_at(light, 0).map_err(WorldMeshError::from)?,
            can_cast_shadow: s.u8_at(light, 1).map_err(WorldMeshError::from)? != 0,
            exponent: s.u8_at(light, 2).map_err(WorldMeshError::from)?,
            color: [
                s.f32_at(light, 4).map_err(WorldMeshError::from)?,
                s.f32_at(light, 8).map_err(WorldMeshError::from)?,
                s.f32_at(light, 12).map_err(WorldMeshError::from)?,
            ],
            direction: [
                s.f32_at(light, 16).map_err(WorldMeshError::from)?,
                s.f32_at(light, 20).map_err(WorldMeshError::from)?,
                s.f32_at(light, 24).map_err(WorldMeshError::from)?,
            ],

            origin: [
                s.f32_at(light, 40).map_err(WorldMeshError::from)?,
                s.f32_at(light, 44).map_err(WorldMeshError::from)?,
                s.f32_at(light, 48).map_err(WorldMeshError::from)?,
            ],
            radius: s.f32_at(light, 52).map_err(WorldMeshError::from)?,
            cos_outer: s.f32_at(light, 56).map_err(WorldMeshError::from)?,
            cos_inner: s.f32_at(light, 60).map_err(WorldMeshError::from)?,

            cos_half_fov_expanded: 0.0,
            def_name: def_name_from_light(s, light),
            falloff_image_width: None,
            lmap_lookup_start: 0,
            attenuation_image: None,
            attenuation_sampler: 0,
            t5_attenuation: None,
            t5_falloff: None,
            t5_a_ab_b: None,
            t5_angle: None,
            t5_cookie0: None,
            t5_cookie1: None,
            t5_cookie2: None,
            t5_diffuse_color: None,
            t5_specular_color: None,
        });
    }
    Ok(output)
}

fn def_name_from_light(s: &ZoneStream<'_>, light: fastfile_iw5::Ptr) -> Option<String> {
    let name_off = s.layout(sz::COM_PRIMARY_LIGHT_DEF_NAME_OFF, 80);
    let ZonePtr::Offset(name_ptr) = s.ptr_at(light, name_off).ok()? else {
        return None;
    };
    s.cstr(name_ptr).ok().map(str::to_owned)
}

pub fn capture_iw5_light_defs(
    s: &ZoneStream<'_>,
    materials: &MaterialCatalog,
) -> Vec<CapturedLightDef> {
    s.light_defs()
        .iter()
        .filter_map(|def| {
            let name = crate::AssetRef::decode(def.name.and_then(|p| s.cstr(p).ok())?);
            if name.is_empty() {
                return None;
            }
            let attenuation_image_name = def
                .attenuation_image
                .and_then(|ptr| {
                    materials
                        .image_index(iw4_ptr(ptr))
                        .and_then(|i| materials.images.get(i).map(|image| image.name.to_string()))
                        .or_else(|| image_name_at(s, ptr))
                })
                .or_else(|| {
                    def.attenuation_image_name
                        .and_then(|p| s.cstr(p).ok().map(str::to_owned))
                })
                .filter(|image| !image.is_empty());
            Some(CapturedLightDef {
                namespace: crate::AssetNamespace::Iw5,
                name,
                attenuation_image_name,
                attenuation_width: def.attenuation_width,
                attenuation_sampler: def.attenuation_sampler,
                lmap_lookup_start: def.lmap_lookup_start,
            })
        })
        .collect()
}

fn image_name_at(s: &ZoneStream<'_>, img: fastfile_iw5::Ptr) -> Option<String> {
    match s.ptr_at(img, s.layout(sz::GFX_IMAGE_NAME_OFF, 32)) {
        Ok(ZonePtr::Offset(n)) => s.cstr(s.resolve_alias(n)).ok().map(str::to_owned),
        _ => None,
    }
}
