use std::collections::HashMap;

use asset_iw4::size as iw4_sz;
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use dpvs_iw4::{AABB_NODE_STRIDE, AabbNodeView, Bounds, CPlane, SurfRange};
use fastfile_iw4::Ptr as Iw4Ptr;
use fastfile_t5::size as t5_sz;
use fastfile_t5::{GfxWorldGeometry, ZonePtr, ZoneStream};

use crate::world_draw::{
    CameraRangeKind, CameraSurfRange, CameraSurfRanges, DpvsWorldData, OwnedPortal,
    SurfaceDrawFields, WorldBatch, WorldDraw, WorldLightmap, WorldLightmapGap, WorldPrimaryLight,
    WorldReflectionProbe, WorldVertexPayload,
};
use crate::world_mesh::{
    WorldMeshError, WorldMeshStats, normalize_or_up, unpack_color, unpack_unit_vec,
};
use crate::{SurfaceCastsSunShadow, world_capture_from_casters};
use asset_material::MaterialCatalog;

const D3DFMT_R5G6B5: u32 = 23;

const FOURCC_DXT1: u32 = u32::from_le_bytes(*b"DXT1");

const GFX_SURFACE: usize = 80;

const VERTEX_LAYER_DATA_OFF: usize = 12;
const FIRST_VERTEX_OFF: usize = 28;
const VERTEX_COUNT_OFF: usize = 32;
const TRI_COUNT_OFF: usize = 34;
const BASE_INDEX_OFF: usize = 36;
const MATERIAL_OFF: usize = 0x30;
const LIGHTMAP_INDEX_OFF: usize = 0x34;
const REFLECTION_PROBE_OFF: usize = 0x35;
const PRIMARY_LIGHT_OFF: usize = 0x36;
const SURFACE_FLAGS_OFF: usize = 0x37;
const DRAW_SURF_CASTS_SUN_SHADOW: u64 = 1 << 20;

const T5_CPLANE: usize = 20;
const T5_GFX_CELL: usize = 56;
const T5_GFX_AABB_TREE: usize = 40;
const T5_GFX_PORTAL: usize = 68;
const T5_CELL_AABB_COUNT: usize = 0x18;
const T5_CELL_AABB_TREES: usize = 0x1c;
const T5_CELL_PORTAL_COUNT: usize = 0x20;
const T5_CELL_PORTALS: usize = 0x24;
const T5_CELL_PROBE_COUNT: usize = 0x30;
const T5_CELL_PROBES: usize = 0x34;

const T5_SURFACE_BOUNDS: usize = 0x38;

const T5_SMODEL_INST_BOUNDS: usize = 0;
const T5_AABB_CHILD_COUNT: usize = 24;
const T5_AABB_SURFACE_COUNT: usize = 26;
const T5_AABB_START_SURF: usize = 28;
const T5_AABB_SMODEL_COUNT: usize = 0x1e;
const T5_AABB_SMODEL_INDEXES: usize = 0x20;
const T5_AABB_CHILDREN_OFFSET: usize = 0x24;
const T5_PORTAL_PLANE: usize = 12;
const T5_PORTAL_CELL: usize = 0x20;
const T5_PORTAL_VERTS: usize = 0x24;
const T5_PORTAL_VERT_COUNT: usize = 0x28;

const T5_PORTAL_HULL_AXIS: usize = 0x2c;

const T5_COM_PRIMARY_LIGHT: usize = t5_sz::COM_PRIMARY_LIGHT;
const T5_LIGHT_COLOR: usize = 8;
const T5_LIGHT_DIR: usize = 20;
const T5_LIGHT_ORIGIN: usize = 32;
const T5_LIGHT_RADIUS: usize = 44;
const T5_LIGHT_COS_OUTER: usize = 48;
const T5_LIGHT_COS_INNER: usize = 52;

const T5_GFX_REFLECTION_PROBE: usize = 24;
const T5_PROBE_IMAGE: usize = 12;

fn iw4_ptr(p: fastfile_t5::Ptr) -> Iw4Ptr {
    Iw4Ptr {
        block: p.block,
        offset: p.offset,
    }
}

pub fn build_t5_world_draw(
    s: &ZoneStream<'_>,
    geometry: GfxWorldGeometry,
    mut materials: MaterialCatalog,
) -> Result<(WorldDraw, MaterialCatalog), WorldMeshError> {
    let Some(vertices) = geometry.vertices else {
        return Err(WorldMeshError::NoGeometry);
    };
    let Some(indices) = geometry.indices else {
        return Err(WorldMeshError::NoGeometry);
    };
    let Some(surfaces) = geometry.surfaces else {
        return Err(WorldMeshError::NoGeometry);
    };

    if let Some(table) = geometry.terrain_scorch_images {
        let images: [Option<usize>; 31] =
            std::array::from_fn(|index| materials.image_index(iw4_ptr(table.at(index * 4))));
        let white = materials
            .image_index_by_key(asset_material::AssetNamespace::T5, "$white")
            .unwrap_or_else(|| {
                let index = materials.images.len();
                materials.images.push(asset_material::AuthoredImage {
                    namespace: asset_material::AssetNamespace::T5,
                    name: asset_material::AssetRef::decode("$white"),
                    map_type: 3,
                    semantic: 2,
                    category: 1,
                    use_srgb_reads: false,
                    width: 1,
                    height: 1,
                    depth: 1,
                    level_count: 1,
                    format: 21,
                    payload: std::sync::Arc::new(vec![255; 4]),
                    decoded: None,
                    common_owned: false,
                    decoded_variant: None,
                    decoded_by: None,
                    pending_decode: None,
                });
                index
            });
        for material in &mut materials.materials {
            let Some(types) = material.t5_layered_surface_types else {
                continue;
            };
            for layer in 0..((types >> 29) as usize).min(4) {
                let surface = ((types >> (layer * 6)) & 0x3f) as usize;
                let image = images
                    .get(surface)
                    .copied()
                    .flatten()
                    .or(images[0])
                    .or(Some(white));
                material
                    .textures
                    .push(asset_material::MaterialTextureBinding {
                        name_hash: asset_material::t5_code_remap::terrain_scorch_binding_hash(
                            layer,
                        ),
                        name_start: b'i',
                        name_end: b'0' + layer as u8,
                        sampler_state: 0x62,
                        semantic: 2,
                        image,
                    });
            }
        }
    }

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
        let v = vertices.at(i * iw4_sz::GFX_WORLD_VERTEX);
        let mut packed = [0u8; fastfile_t5::size::GFX_WORLD_VERTEX];
        for (offset, byte) in packed.iter_mut().enumerate() {
            *byte = s.u8_at(v, offset).map_err(|_| WorldMeshError::NoGeometry)?;
        }
        packed_vertices.push(packed);
        let xyz = [
            s.f32_at(v, 0).map_err(|_| WorldMeshError::NoGeometry)?,
            s.f32_at(v, 4).map_err(|_| WorldMeshError::NoGeometry)?,
            s.f32_at(v, 8).map_err(|_| WorldMeshError::NoGeometry)?,
        ];
        for axis in 0..3 {
            min[axis] = min[axis].min(xyz[axis]);
            max[axis] = max[axis].max(xyz[axis]);
        }
        positions.push(xyz);
        normals.push(normalize_or_up(unpack_unit_vec(
            s.u32_at(v, 36).map_err(|_| WorldMeshError::NoGeometry)?,
        )));
        let tangent = normalize_or_up(unpack_unit_vec(
            s.u32_at(v, 40).map_err(|_| WorldMeshError::NoGeometry)?,
        ));
        tangents.push([
            tangent[0],
            tangent[1],
            tangent[2],
            s.f32_at(v, 12).map_err(|_| WorldMeshError::NoGeometry)?,
        ]);
        colors.push(unpack_color(
            s.u32_at(v, 16).map_err(|_| WorldMeshError::NoGeometry)?,
        ));
        texture_uvs.push([
            s.f32_at(v, 20).map_err(|_| WorldMeshError::NoGeometry)?,
            s.f32_at(v, 24).map_err(|_| WorldMeshError::NoGeometry)?,
        ]);
        lightmap_uvs.push([
            s.f32_at(v, 28).map_err(|_| WorldMeshError::NoGeometry)?,
            s.f32_at(v, 32).map_err(|_| WorldMeshError::NoGeometry)?,
        ]);
    }

    let vertex_layer = match (geometry.vertex_layer, geometry.vertex_layer_size) {
        (Some(ptr), n) if n > 0 => s
            .slice_at(ptr, 0, n)
            .map_err(|_| WorldMeshError::NoGeometry)?
            .to_vec(),
        _ => Vec::new(),
    };

    let mut packed_indices: Vec<u32> = Vec::new();
    let mut surface_index_ranges = Vec::with_capacity(geometry.surface_count);
    let mut surface_vertex_layer = Vec::with_capacity(geometry.surface_count);
    let mut surface_first_vertex = Vec::with_capacity(geometry.surface_count);
    let mut surface_vertex_count = Vec::with_capacity(geometry.surface_count);
    let mut authored_lightmapped = Vec::with_capacity(geometry.surface_count);
    let mut surface_materials = Vec::with_capacity(geometry.surface_count);
    let mut surface_primary_lights = Vec::with_capacity(geometry.surface_count);
    let mut surface_reflection_probes = Vec::with_capacity(geometry.surface_count);
    let mut surface_lightmap_indices = Vec::with_capacity(geometry.surface_count);
    let mut surface_draw_fields = Vec::with_capacity(geometry.surface_count);
    let mut surface_casts_sun_shadow = SurfaceCastsSunShadow::with_len(geometry.surface_count);
    let mut skipped_surfaces = 0usize;
    let mut skipped_sky_surfaces = 0usize;
    let mut sky_material: Option<usize> = None;
    let mut skipped_shadowcaster_surfaces = 0usize;
    let mut unrouted_surfaces = 0usize;
    let mut undecided_state_bits_surfaces = 0usize;

    for i in 0..geometry.surface_count {
        let surface = surfaces.at(i * GFX_SURFACE);
        let first_vertex = s
            .u32_at(surface, FIRST_VERTEX_OFF)
            .map_err(|_| WorldMeshError::NoGeometry)? as usize;
        let vertex_layer_data = s
            .i32_at(surface, VERTEX_LAYER_DATA_OFF)
            .map_err(|_| WorldMeshError::NoGeometry)?;
        let vertex_count = s
            .u16_at(surface, VERTEX_COUNT_OFF)
            .map_err(|_| WorldMeshError::NoGeometry)? as usize;
        surface_vertex_layer.push(vertex_layer_data);
        surface_first_vertex.push(first_vertex as u32);
        surface_vertex_count.push(vertex_count as u32);
        let tri_count = s
            .u16_at(surface, TRI_COUNT_OFF)
            .map_err(|_| WorldMeshError::NoGeometry)? as usize;
        let base_index = s
            .u32_at(surface, BASE_INDEX_OFF)
            .map_err(|_| WorldMeshError::NoGeometry)? as usize;
        let lightmap_index = s
            .u8_at(surface, LIGHTMAP_INDEX_OFF)
            .map_err(|_| WorldMeshError::NoGeometry)? as usize;
        let material = materials.material_index(iw4_ptr(surface.at(MATERIAL_OFF)));
        let authored = material.and_then(|index| materials.materials.get(index.get()));
        let pass = crate::world_draw::surface_pass(&materials, authored);

        let is_sky = pass.sky
            || authored.is_some_and(|m| m.info_game_flags & T5_MATERIAL_GAME_FLAG_SKY != 0);
        if is_sky && sky_material.is_none() {
            sky_material = material.map(|index| index.get());
        }
        let is_shadowcaster = pass.shadow_only;
        unrouted_surfaces += usize::from(pass.unrouted);
        undecided_state_bits_surfaces += usize::from(pass.state_bits_undecided);

        authored_lightmapped.push(lightmap_index < geometry.lightmap_count);
        surface_materials.push(material.map(|i| i.get()));
        surface_primary_lights.push(
            s.u8_at(surface, PRIMARY_LIGHT_OFF)
                .map_err(|_| WorldMeshError::NoGeometry)?,
        );
        surface_reflection_probes.push(
            s.u8_at(surface, REFLECTION_PROBE_OFF)
                .map_err(|_| WorldMeshError::NoGeometry)?,
        );
        surface_lightmap_indices.push(lightmap_index.min(255) as u8);
        let surface_flags = s
            .u8_at(surface, SURFACE_FLAGS_OFF)
            .map_err(|_| WorldMeshError::NoGeometry)?;
        if surface_flags & 1 != 0
            && authored.is_some_and(|m| m.draw_surf & DRAW_SURF_CASTS_SUN_SHADOW != 0)
        {
            surface_casts_sun_shadow.set(i);
        }
        surface_draw_fields.push(SurfaceDrawFields {
            first_vertex: first_vertex as u32,
            tri_count: tri_count as u16,
            base_index: base_index as u32,
            lightmap_index: lightmap_index.min(255) as u8,
            reflection_probe_index: s
                .u8_at(surface, REFLECTION_PROBE_OFF)
                .map_err(|_| WorldMeshError::NoGeometry)?,
            primary_light_index: s
                .u8_at(surface, PRIMARY_LIGHT_OFF)
                .map_err(|_| WorldMeshError::NoGeometry)?,
        });

        let range_start = packed_indices.len() as u32;

        if is_sky {
            skipped_sky_surfaces += 1;

            if geometry.sky_box_model.is_some() {
                surface_index_ranges.push((range_start, 0));
                continue;
            }
        }
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
                .map_err(|_| WorldMeshError::NoGeometry)? as usize;
            let absolute = first_vertex + local;
            if absolute >= geometry.vertex_count {
                ok = false;
                break;
            }
            packed_indices.push(absolute as u32);
        }
        let count = if ok {
            (tri_count * 3) as u32
        } else {
            packed_indices.truncate(range_start as usize);
            skipped_surfaces += 1;
            0
        };
        surface_index_ranges.push((range_start, count));
    }

    let stats = WorldMeshStats {
        vertices: positions.len(),
        triangles: packed_indices.len() / 3,
        surfaces: geometry.surface_count,
        skipped_surfaces: skipped_surfaces
            + if geometry.sky_box_model.is_some() {
                skipped_sky_surfaces
            } else {
                0
            }
            + skipped_shadowcaster_surfaces,
        sky_surfaces: skipped_sky_surfaces,
        sky_material,
        unrouted_surfaces,
        undecided_state_bits_surfaces,
        min,
        max,

        bounds: geometry.bounds.map(|bits| bits.map(f32::from_bits)),
    };

    let lightmap = if geometry.lightmap_count == 0 {
        Err(WorldLightmapGap::Missing)
    } else if geometry.lightmap_count > geometry.lightmaps.len() {
        Err(WorldLightmapGap::MultiplePages(geometry.lightmap_count))
    } else {
        geometry
            .lightmaps
            .iter()
            .take(geometry.lightmap_count)
            .map(|page| decode_t5_lightmap(s, *page).map(Some))
            .collect::<Result<Vec<_>, _>>()
    };
    let surface_lightmapped = if lightmap.is_ok() {
        authored_lightmapped
    } else {
        vec![false; geometry.surface_count]
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

    let mut dpvs = extract_t5_dpvs(s, geometry)?;
    if let Some(ptr) = geometry.sky_start_surfs {
        dpvs.sky_start_surfs = (0..geometry.sky_surf_count)
            .map(|i| s.u32_at(ptr, i * 4).map_err(|_| WorldMeshError::NoGeometry))
            .collect::<Result<Vec<_>, _>>()?;
    }
    let primary_lights = extract_t5_primary_lights(s, geometry.sun_primary_light_index)?;
    let reflection_probes = extract_t5_reflection_probes(s, geometry, &materials)?;

    let mut expanded =
        vec![0u8; geometry.vertex_count * asset_material::T5_WORLD_LAYER_HOST_STRIDE];
    let mut spans = std::collections::BTreeSet::new();
    let mut owners = vec![None; geometry.vertex_count];
    for i in 0..geometry.surface_count {
        let Some(material) = surface_materials[i].and_then(|id| materials.materials.get(id)) else {
            continue;
        };
        let Some(set) = materials.technique_set_facts().iter().find(|set| {
            set.namespace == material.namespace && set.name.same_name(&material.technique_set)
        }) else {
            continue;
        };
        let vertex_type = usize::from(set.world_vert_format) + 2;
        let stride = *fastfile_t5::vertex_decl::LAYER_DATA_STRIDE
            .get(vertex_type)
            .ok_or(WorldMeshError::NoGeometry)? as usize;
        if stride == 0 {
            continue;
        }
        let first = surface_first_vertex[i] as usize;
        let count = surface_vertex_count[i] as usize;
        let offset =
            usize::try_from(surface_vertex_layer[i]).map_err(|_| WorldMeshError::NoGeometry)?;
        if !spans.insert((first, count, offset, stride)) {
            continue;
        }
        for local in 0..count {
            let vertex = first + local;
            let src = offset + local * stride;
            let bytes = vertex_layer
                .get(src..src + stride)
                .ok_or(WorldMeshError::NoGeometry)?;
            let dst = vertex * asset_material::T5_WORLD_LAYER_HOST_STRIDE;
            let slot = expanded
                .get_mut(dst..dst + stride)
                .ok_or(WorldMeshError::NoGeometry)?;
            if owners[vertex].is_some_and(|owner| owner != (src, stride)) && slot != bytes {
                return Err(WorldMeshError::ConflictingVertexLayer { vertex });
            }
            owners[vertex] = Some((src, stride));
            slot.copy_from_slice(bytes);
        }
    }
    let vertex_layer = expanded;

    let t5_sun_light = geometry
        .sun_light
        .map(|p| {
            let read = |off| s.f32_at(p, off).map_err(|_| WorldMeshError::NoGeometry);
            Ok::<_, WorldMeshError>(crate::world_draw::WorldSunLight {
                direction: [read(16)?, read(20)?, read(24)?],
                diffuse: [read(76)?, read(80)?, read(84)?, read(88)?],
                specular: [read(92)?, read(96)?, read(100)?, read(104)?],
            })
        })
        .transpose()?;

    Ok((
        WorldDraw {
            sky_model: None,
            t5_sun_light,
            batches,
            lightmap,
            stats,
            packed_vertices: WorldVertexPayload::T5(packed_vertices),
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
            light_defs: s
                .light_defs()
                .iter()
                .filter_map(|def| {
                    Some(crate::world_draw::CapturedLightDef {
                        namespace: crate::AssetNamespace::T5,
                        name: crate::AssetRef::decode(s.cstr(def.name?).ok()?),
                        attenuation_image_name: def
                            .attenuation_image_name
                            .and_then(|p| s.cstr(p).ok())
                            .map(str::to_owned),
                        attenuation_width: def.attenuation_width,
                        attenuation_sampler: def.attenuation_sampler,
                        lmap_lookup_start: def.lmap_lookup_start,
                    })
                })
                .collect(),
            sun_primary_light_count: geometry.sun_primary_light_index as u32,
            sun_stages: Vec::new(),
            light_region_hulls: None,
            shadow_geometry: Vec::new(),
            reflection_probes,
            dpvs,
            outdoor_image_name: None,
            outdoor_image: None,
            outdoor_lookup: [0; 16],
            sun_effects: None,
            sky_dynamic_intensity: geometry
                .sky_dynamic_intensity_bits
                .map(|v| v.map(f32::from_bits))
                .filter(|v| v.iter().all(|f| f.is_finite())),
            t6_exposure: None,
            t5_sun_parse_exposure: geometry
                .sun_parse_exposure_bits
                .map(f32::from_bits)
                .filter(|v| v.is_finite()),
            t5_tree_scatter_intensity: geometry
                .sun_parse_tree_scatter_intensity_bits
                .map(f32::from_bits)
                .filter(|v| v.is_finite()),
            t5_tree_scatter_amount: geometry
                .sun_parse_tree_scatter_amount_bits
                .map(f32::from_bits)
                .filter(|v| v.is_finite()),
            t5_exposure_volume_count: geometry.exposure_volume_count as u32,
        },
        materials,
    ))
}

const T5_MATERIAL_GAME_FLAG_SKY: u8 = 8;

pub fn build_t5_world_mesh(
    s: &ZoneStream<'_>,
    geometry: GfxWorldGeometry,
) -> Result<(Mesh, WorldMeshStats), WorldMeshError> {
    let (draw, _) = build_t5_world_draw(s, geometry, MaterialCatalog::default())?;
    let Some(batch) = draw.batches.into_iter().next() else {
        return Err(WorldMeshError::NoGeometry);
    };
    Ok((batch.mesh, draw.stats))
}

fn read_t5_mins_maxs(s: &ZoneStream<'_>, p: fastfile_t5::Ptr) -> Result<Bounds, WorldMeshError> {
    Ok(Bounds::from_mins_maxs(
        [
            s.f32_at(p, 0).map_err(|_| WorldMeshError::NoGeometry)?,
            s.f32_at(p, 4).map_err(|_| WorldMeshError::NoGeometry)?,
            s.f32_at(p, 8).map_err(|_| WorldMeshError::NoGeometry)?,
        ],
        [
            s.f32_at(p, 12).map_err(|_| WorldMeshError::NoGeometry)?,
            s.f32_at(p, 16).map_err(|_| WorldMeshError::NoGeometry)?,
            s.f32_at(p, 20).map_err(|_| WorldMeshError::NoGeometry)?,
        ],
    ))
}

fn read_t5_aabb_bounds(s: &ZoneStream<'_>, p: fastfile_t5::Ptr) -> Result<Bounds, WorldMeshError> {
    let mut empty = true;
    for offset in [
        T5_AABB_CHILD_COUNT,
        T5_AABB_SURFACE_COUNT,
        T5_AABB_SMODEL_COUNT,
    ] {
        empty &= s
            .u16_at(p, offset)
            .map_err(|_| WorldMeshError::NoGeometry)?
            == 0;
    }
    for axis in 0..3 {
        empty &= s
            .f32_at(p, axis * 4)
            .map_err(|_| WorldMeshError::NoGeometry)?
            == f32::MAX;
        empty &= s
            .f32_at(p, 12 + axis * 4)
            .map_err(|_| WorldMeshError::NoGeometry)?
            == -f32::MAX;
    }
    if empty {
        Ok(Bounds::CLEARED)
    } else {
        read_t5_mins_maxs(s, p)
    }
}

fn extract_t5_dpvs(
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
                kind: CameraRangeKind::Decal,
                begin: g.decal_surfs_begin,
                end: g.decal_surfs_end,
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
    out.static_surface_count_no_decal = 0;

    if let (Some(planes_ptr), Some(nodes_ptr)) = (g.planes, g.nodes) {
        out.planes.reserve(g.plane_count);
        for i in 0..g.plane_count {
            let p = planes_ptr.at(i * T5_CPLANE);
            out.planes.push(CPlane {
                normal: [
                    s.f32_at(p, 0).map_err(|_| WorldMeshError::NoGeometry)?,
                    s.f32_at(p, 4).map_err(|_| WorldMeshError::NoGeometry)?,
                    s.f32_at(p, 8).map_err(|_| WorldMeshError::NoGeometry)?,
                ],
                dist: s.f32_at(p, 12).map_err(|_| WorldMeshError::NoGeometry)?,
                r#type: s.u8_at(p, 16).map_err(|_| WorldMeshError::NoGeometry)?,
            });
        }
        out.nodes.reserve(g.node_count);
        for i in 0..g.node_count {
            out.nodes.push(
                s.u16_at(nodes_ptr, i * 2)
                    .map_err(|_| WorldMeshError::NoGeometry)?,
            );
        }
    }

    if let Some(sorted_ptr) = g.sorted_surf_index {
        out.sorted_surf_index.reserve(g.static_surface_count);
        for i in 0..g.static_surface_count {
            out.sorted_surf_index.push(
                s.u16_at(sorted_ptr, i * 2)
                    .map_err(|_| WorldMeshError::NoGeometry)?,
            );
        }
    }

    if let Some(surfaces_ptr) = g.surfaces {
        out.surface_bounds.reserve(g.surface_count);
        for i in 0..g.surface_count {
            let surf = surfaces_ptr.at(i * GFX_SURFACE + T5_SURFACE_BOUNDS);
            out.surface_bounds.push(read_t5_mins_maxs(s, surf)?);
        }
    }
    if let Some(insts_ptr) = g.smodel_insts {
        out.smodel_bounds.reserve(g.smodel_count);
        for i in 0..g.smodel_count {
            let inst = insts_ptr.at(i * t5_sz::GFX_STATIC_MODEL_INST + T5_SMODEL_INST_BOUNDS);
            out.smodel_bounds.push(read_t5_mins_maxs(s, inst)?);
        }
    }

    out.cell_roots = vec![SurfRange::default(); g.cell_count];
    out.aabb_trees = vec![Vec::new(); g.cell_count];
    out.aabb_smodel_indices = vec![Vec::new(); g.cell_count];
    out.portals_per_cell = Vec::with_capacity(g.cell_count);
    out.cell_reflection_probes = Vec::with_capacity(g.cell_count);

    let Some(cells_ptr) = g.cells else {
        return out.checked();
    };

    for i in 0..g.cell_count {
        let cell = cells_ptr.at(i * T5_GFX_CELL);
        let aabb_count = s
            .i32_at(cell, T5_CELL_AABB_COUNT)
            .map_err(|_| WorldMeshError::NoGeometry)?
            .max(0) as usize;
        let portal_count = s
            .i32_at(cell, T5_CELL_PORTAL_COUNT)
            .map_err(|_| WorldMeshError::NoGeometry)?
            .max(0) as usize;

        if aabb_count > 0 {
            if let ZonePtr::Offset(tree) = s
                .ptr_at(cell, T5_CELL_AABB_TREES)
                .map_err(|_| WorldMeshError::NoGeometry)?
            {
                out.cell_roots[i] = SurfRange {
                    start: s
                        .u16_at(tree, T5_AABB_START_SURF)
                        .map_err(|_| WorldMeshError::NoGeometry)?,
                    count: s
                        .u16_at(tree, T5_AABB_SURFACE_COUNT)
                        .map_err(|_| WorldMeshError::NoGeometry)?,
                };
                let mut nodes = Vec::with_capacity(aabb_count);
                let mut smodel_indices: Vec<u16> = Vec::with_capacity(aabb_count);
                for j in 0..aabb_count {
                    let n = tree.at(j * T5_GFX_AABB_TREE);
                    let t5_children = s
                        .i32_at(n, T5_AABB_CHILDREN_OFFSET)
                        .map_err(|_| WorldMeshError::NoGeometry)?;

                    let children_offset = if t5_children > 0 {
                        ((t5_children as usize) / T5_GFX_AABB_TREE * AABB_NODE_STRIDE) as i32
                    } else {
                        t5_children
                    };
                    let smodel_count = s
                        .u16_at(n, T5_AABB_SMODEL_COUNT)
                        .map_err(|_| WorldMeshError::NoGeometry)?
                        as usize;
                    let smodel_index_start = smodel_indices.len() as u32;
                    let mut smodel_index_count = 0u16;
                    if let ZonePtr::Offset(indexes) = s
                        .ptr_at(n, T5_AABB_SMODEL_INDEXES)
                        .map_err(|_| WorldMeshError::NoGeometry)?
                    {
                        for k in 0..smodel_count {
                            smodel_indices.push(
                                s.u16_at(indexes, k * 2)
                                    .map_err(|_| WorldMeshError::NoGeometry)?,
                            );
                            smodel_index_count += 1;
                        }
                    }
                    nodes.push(AabbNodeView {
                        bounds: read_t5_aabb_bounds(s, n)?,
                        child_count: s
                            .u16_at(n, T5_AABB_CHILD_COUNT)
                            .map_err(|_| WorldMeshError::NoGeometry)?,
                        children_offset,
                        start_surf: s
                            .u16_at(n, T5_AABB_START_SURF)
                            .map_err(|_| WorldMeshError::NoGeometry)?,
                        surface_count: s
                            .u16_at(n, T5_AABB_SURFACE_COUNT)
                            .map_err(|_| WorldMeshError::NoGeometry)?,

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

        let mut cell_portals = Vec::with_capacity(portal_count);
        if portal_count > 0 {
            if let ZonePtr::Offset(arr) = s
                .ptr_at(cell, T5_CELL_PORTALS)
                .map_err(|_| WorldMeshError::NoGeometry)?
            {
                for j in 0..portal_count {
                    let portal = arr.at(j * T5_GFX_PORTAL);
                    let plane = [
                        s.f32_at(portal, T5_PORTAL_PLANE)
                            .map_err(|_| WorldMeshError::NoGeometry)?,
                        s.f32_at(portal, T5_PORTAL_PLANE + 4)
                            .map_err(|_| WorldMeshError::NoGeometry)?,
                        s.f32_at(portal, T5_PORTAL_PLANE + 8)
                            .map_err(|_| WorldMeshError::NoGeometry)?,
                        s.f32_at(portal, T5_PORTAL_PLANE + 12)
                            .map_err(|_| WorldMeshError::NoGeometry)?,
                    ];
                    let neighbor = match s
                        .ptr_at(portal, T5_PORTAL_CELL)
                        .map_err(|_| WorldMeshError::NoGeometry)?
                    {
                        ZonePtr::Offset(neighbor_cell)
                            if neighbor_cell.block == cells_ptr.block
                                && neighbor_cell.offset >= cells_ptr.offset =>
                        {
                            let delta = (neighbor_cell.offset - cells_ptr.offset) as usize;
                            if delta % T5_GFX_CELL == 0 {
                                (delta / T5_GFX_CELL) as u16
                            } else {
                                u16::MAX
                            }
                        }
                        _ => u16::MAX,
                    };
                    let vertex_count = s
                        .u8_at(portal, T5_PORTAL_VERT_COUNT)
                        .map_err(|_| WorldMeshError::NoGeometry)?
                        as usize;
                    let vert_start = out.portal_verts.len();
                    if let ZonePtr::Offset(verts) = s
                        .ptr_at(portal, T5_PORTAL_VERTS)
                        .map_err(|_| WorldMeshError::NoGeometry)?
                    {
                        for k in 0..vertex_count {
                            let v = verts.at(k * 12);
                            out.portal_verts.push([
                                s.f32_at(v, 0).map_err(|_| WorldMeshError::NoGeometry)?,
                                s.f32_at(v, 4).map_err(|_| WorldMeshError::NoGeometry)?,
                                s.f32_at(v, 8).map_err(|_| WorldMeshError::NoGeometry)?,
                            ]);
                        }
                    }
                    let hull_axis = [
                        [
                            s.f32_at(portal, T5_PORTAL_HULL_AXIS)
                                .map_err(|_| WorldMeshError::NoGeometry)?,
                            s.f32_at(portal, T5_PORTAL_HULL_AXIS + 4)
                                .map_err(|_| WorldMeshError::NoGeometry)?,
                            s.f32_at(portal, T5_PORTAL_HULL_AXIS + 8)
                                .map_err(|_| WorldMeshError::NoGeometry)?,
                        ],
                        [
                            s.f32_at(portal, T5_PORTAL_HULL_AXIS + 12)
                                .map_err(|_| WorldMeshError::NoGeometry)?,
                            s.f32_at(portal, T5_PORTAL_HULL_AXIS + 16)
                                .map_err(|_| WorldMeshError::NoGeometry)?,
                            s.f32_at(portal, T5_PORTAL_HULL_AXIS + 20)
                                .map_err(|_| WorldMeshError::NoGeometry)?,
                        ],
                    ];
                    cell_portals.push(OwnedPortal {
                        plane,
                        neighbor,
                        vert_start,
                        vert_count: out.portal_verts.len() - vert_start,
                        hull_axis: Some(hull_axis),
                    });
                }
            }
        }
        out.portals_per_cell.push(cell_portals);
        let probe_count = s
            .u8_at(cell, T5_CELL_PROBE_COUNT)
            .map_err(|_| WorldMeshError::NoGeometry)? as usize;
        let mut probes = Vec::with_capacity(probe_count);
        if let ZonePtr::Offset(arr) = s
            .ptr_at(cell, T5_CELL_PROBES)
            .map_err(|_| WorldMeshError::NoGeometry)?
        {
            for k in 0..probe_count {
                probes.push(s.u8_at(arr, k).map_err(|_| WorldMeshError::NoGeometry)?);
            }
        }
        out.cell_reflection_probes.push(probes);
    }

    out.checked()
}

fn extract_t5_primary_lights(
    s: &ZoneStream<'_>,
    sun_primary_light_index: usize,
) -> Result<Vec<WorldPrimaryLight>, WorldMeshError> {
    let Some(world) = s.com_world() else {
        return Ok(Vec::new());
    };
    let Some(lights) = world.primary_lights else {
        return Ok(Vec::new());
    };
    let mut output = Vec::with_capacity(world.primary_light_count);
    for index in 0..world.primary_light_count {
        let light = lights.at(index * T5_COM_PRIMARY_LIGHT);
        output.push(WorldPrimaryLight {
            is_sun: index != 0 && index <= sun_primary_light_index,
            light_type: s.u8_at(light, 0).map_err(|_| WorldMeshError::NoGeometry)?,
            can_cast_shadow: s.u8_at(light, 1).map_err(|_| WorldMeshError::NoGeometry)? != 0,
            exponent: s.u8_at(light, 2).map_err(|_| WorldMeshError::NoGeometry)?,
            color: [
                s.f32_at(light, T5_LIGHT_COLOR)
                    .map_err(|_| WorldMeshError::NoGeometry)?,
                s.f32_at(light, T5_LIGHT_COLOR + 4)
                    .map_err(|_| WorldMeshError::NoGeometry)?,
                s.f32_at(light, T5_LIGHT_COLOR + 8)
                    .map_err(|_| WorldMeshError::NoGeometry)?,
            ],
            direction: [
                s.f32_at(light, T5_LIGHT_DIR)
                    .map_err(|_| WorldMeshError::NoGeometry)?,
                s.f32_at(light, T5_LIGHT_DIR + 4)
                    .map_err(|_| WorldMeshError::NoGeometry)?,
                s.f32_at(light, T5_LIGHT_DIR + 8)
                    .map_err(|_| WorldMeshError::NoGeometry)?,
            ],
            origin: [
                s.f32_at(light, T5_LIGHT_ORIGIN)
                    .map_err(|_| WorldMeshError::NoGeometry)?,
                s.f32_at(light, T5_LIGHT_ORIGIN + 4)
                    .map_err(|_| WorldMeshError::NoGeometry)?,
                s.f32_at(light, T5_LIGHT_ORIGIN + 8)
                    .map_err(|_| WorldMeshError::NoGeometry)?,
            ],
            radius: s
                .f32_at(light, T5_LIGHT_RADIUS)
                .map_err(|_| WorldMeshError::NoGeometry)?,
            cos_outer: s
                .f32_at(light, T5_LIGHT_COS_OUTER)
                .map_err(|_| WorldMeshError::NoGeometry)?,
            cos_inner: s
                .f32_at(light, T5_LIGHT_COS_INNER)
                .map_err(|_| WorldMeshError::NoGeometry)?,
            cos_half_fov_expanded: 0.0,
            def_name: t5_def_name(s, light),
            falloff_image_width: None,
            lmap_lookup_start: 0,
            attenuation_image: None,
            attenuation_sampler: 0,
            t5_attenuation: t5_vec4(s, light, t5_sz::COM_PRIMARY_LIGHT_ATTENUATION_OFF),
            t5_falloff: t5_vec4(s, light, t5_sz::COM_PRIMARY_LIGHT_FALLOFF_OFF),
            t5_a_ab_b: t5_vec4(s, light, t5_sz::COM_PRIMARY_LIGHT_A_AB_B_OFF),
            t5_angle: t5_vec4(s, light, t5_sz::COM_PRIMARY_LIGHT_ANGLE_OFF),
            t5_cookie0: t5_vec4(s, light, t5_sz::COM_PRIMARY_LIGHT_COOKIE0_OFF),
            t5_cookie1: t5_vec4(s, light, t5_sz::COM_PRIMARY_LIGHT_COOKIE1_OFF),
            t5_cookie2: t5_vec4(s, light, t5_sz::COM_PRIMARY_LIGHT_COOKIE2_OFF),
            t5_diffuse_color: t5_vec4(s, light, t5_sz::COM_PRIMARY_LIGHT_DIFFUSE_COLOR_OFF),
            t5_specular_color: t5_vec4(s, light, t5_sz::COM_PRIMARY_LIGHT_SPECULAR_COLOR_OFF),
        });
    }
    Ok(output)
}

fn t5_vec4(s: &ZoneStream<'_>, light: fastfile_t5::Ptr, off: usize) -> Option<[f32; 4]> {
    Some([
        s.f32_at(light, off).ok()?,
        s.f32_at(light, off + 4).ok()?,
        s.f32_at(light, off + 8).ok()?,
        s.f32_at(light, off + 12).ok()?,
    ])
}

fn t5_def_name(s: &ZoneStream<'_>, light: fastfile_t5::Ptr) -> Option<String> {
    let ZonePtr::Offset(name) = s
        .ptr_at(light, t5_sz::COM_PRIMARY_LIGHT_DEF_NAME_OFF)
        .ok()?
    else {
        return None;
    };
    s.cstr(name).ok().map(str::to_owned)
}

fn extract_t5_reflection_probes(
    s: &ZoneStream<'_>,
    geometry: GfxWorldGeometry,
    materials: &MaterialCatalog,
) -> Result<Vec<WorldReflectionProbe>, WorldMeshError> {
    let Some(probes) = geometry.reflection_probes else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(geometry.reflection_probe_count);
    for i in 0..geometry.reflection_probe_count {
        let probe = probes.at(i * T5_GFX_REFLECTION_PROBE);
        out.push(WorldReflectionProbe {
            image: materials.image_index(iw4_ptr(probe.at(T5_PROBE_IMAGE))),
            lighting_sh: None,
            origin: [
                s.f32_at(probe, 0).map_err(|_| WorldMeshError::NoGeometry)?,
                s.f32_at(probe, 4).map_err(|_| WorldMeshError::NoGeometry)?,
                s.f32_at(probe, 8).map_err(|_| WorldMeshError::NoGeometry)?,
            ],
        });
    }
    Ok(out)
}

#[derive(Default)]
struct BatchBuilder {
    material: Option<usize>,
    lightmapped: bool,
    lightmap_index: u8,
    primary_light_index: u8,
    reflection_probe_index: u8,
    vertices: HashMap<u32, u32>,
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    tangents: Vec<[f32; 4]>,
    colors: Vec<[f32; 4]>,
    texture_uvs: Vec<[f32; 2]>,
    lightmap_uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn make_material_batches(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    tangents: &[[f32; 4]],
    colors: &[[f32; 4]],
    texture_uvs: &[[f32; 2]],
    lightmap_uvs: &[[f32; 2]],
    packed_indices: &[u32],
    surface_index_ranges: &[(u32, u32)],
    surface_materials: &[Option<usize>],
    surface_lightmapped: &[bool],
    surface_lightmap_indices: &[u8],
    surface_primary_lights: &[u8],
    surface_reflection_probes: &[u8],
) -> (Vec<WorldBatch>, Vec<(usize, u32, u32)>) {
    let mut builders: Vec<BatchBuilder> = Vec::new();
    let mut surface_batch_ranges = Vec::with_capacity(surface_index_ranges.len());

    for (i, &(start, count)) in surface_index_ranges.iter().enumerate() {
        if count == 0 {
            surface_batch_ranges.push((0, 0, 0));
            continue;
        }
        let material = surface_materials.get(i).copied().flatten();
        let lightmapped = surface_lightmapped.get(i).copied().unwrap_or(false);
        let lightmap_index = surface_lightmap_indices[i];
        let primary_light_index = surface_primary_lights.get(i).copied().unwrap_or(0);
        let reflection_probe_index = surface_reflection_probes.get(i).copied().unwrap_or(0);

        let batch_index = builders
            .iter()
            .position(|batch| {
                batch.material == material
                    && batch.lightmapped == lightmapped
                    && batch.lightmap_index == lightmap_index
                    && batch.primary_light_index == primary_light_index
                    && batch.reflection_probe_index == reflection_probe_index
            })
            .unwrap_or_else(|| {
                let index = builders.len();
                builders.push(BatchBuilder {
                    material,
                    lightmapped,
                    lightmap_index,
                    primary_light_index,
                    reflection_probe_index,
                    ..Default::default()
                });
                index
            });

        let batch = &mut builders[batch_index];
        let local_start = batch.indices.len() as u32;
        for &absolute in &packed_indices[start as usize..(start + count) as usize] {
            let local = *batch.vertices.entry(absolute).or_insert_with(|| {
                let local = batch.positions.len() as u32;
                batch.positions.push(positions[absolute as usize]);
                batch.normals.push(normals[absolute as usize]);
                batch.tangents.push(tangents[absolute as usize]);
                batch.colors.push(colors[absolute as usize]);
                batch.texture_uvs.push(texture_uvs[absolute as usize]);
                batch.lightmap_uvs.push(lightmap_uvs[absolute as usize]);
                local
            });
            batch.indices.push(local);
        }
        let local_count = batch.indices.len() as u32 - local_start;
        surface_batch_ranges.push((batch_index, local_start, local_count));
    }

    let batches = builders
        .into_iter()
        .map(|batch| {
            let packed_indices = batch.indices.clone();
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, batch.positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, batch.normals);
            mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, batch.tangents);
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, batch.colors);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, batch.texture_uvs);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, batch.lightmap_uvs);
            mesh.insert_indices(Indices::U32(batch.indices));
            WorldBatch {
                mesh,
                packed_indices,
                material: batch.material,
                lightmapped: batch.lightmapped,
                lightmap_index: batch.lightmap_index,
                primary_light_index: batch.primary_light_index,
                reflection_probe_index: batch.reflection_probe_index,
            }
        })
        .collect();

    (batches, surface_batch_ranges)
}

fn t5_lightmap_flat() -> Option<(u8, u8)> {
    let raw = std::env::var("IW4L_T5_LIGHTMAP_FLAT").ok()?;
    let mut parts = raw.split(',');
    let primary = parts.next().and_then(|v| v.parse().ok()).unwrap_or(8u8);
    let secondary = parts.next().and_then(|v| v.parse().ok()).unwrap_or(128u8);
    Some((primary, secondary))
}

fn decode_t5_lightmap(
    s: &ZoneStream<'_>,
    page: fastfile_t5::GfxLightmapImages,
) -> Result<WorldLightmap, WorldLightmapGap> {
    let secondary = page.secondary.ok_or(WorldLightmapGap::MissingSecondary)?;
    let primary = page.primary.ok_or(WorldLightmapGap::MissingPrimary)?;

    if secondary.format != D3DFMT_R5G6B5 {
        return Err(WorldLightmapGap::UnsupportedSecondaryFormat(
            secondary.format,
        ));
    }
    if secondary.width == 0
        || secondary.height < 2
        || secondary.height % 2 != 0
        || secondary.depth != 1
    {
        return Err(WorldLightmapGap::InvalidDimensions {
            width: secondary.width,
            height: secondary.height,
            depth: secondary.depth,
        });
    }
    let page_height = usize::from(secondary.height / 2);
    let row_pixels = usize::from(secondary.width);
    let needed = row_pixels * usize::from(secondary.height) * 2;
    if secondary.source_len < needed {
        return Err(WorldLightmapGap::Truncated {
            needed,
            have: secondary.source_len,
        });
    }
    let source = s
        .source_slice(secondary.source_offset, secondary.source_len)
        .map_err(|_| WorldLightmapGap::Truncated {
            needed,
            have: secondary.source_len,
        })?;
    let (mut ambient_rgba, mut directional_rgba) =
        r5g6b5_page_payloads(source, row_pixels, page_height)?;

    if let Some((_, secondary_flat)) = t5_lightmap_flat() {
        for (index, byte) in ambient_rgba
            .iter_mut()
            .chain(directional_rgba.iter_mut())
            .enumerate()
        {
            *byte = if index % 4 == 3 { 255 } else { secondary_flat };
        }
    }

    let mut secondary_rgba = Vec::with_capacity(ambient_rgba.len() + directional_rgba.len());
    secondary_rgba.extend_from_slice(&ambient_rgba);
    secondary_rgba.extend_from_slice(&directional_rgba);
    let mut secondary_image = Image::new(
        Extent3d {
            width: u32::from(secondary.width),
            height: u32::from(secondary.height),
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
            width: u32::from(secondary.width),
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
            width: u32::from(secondary.width),
            height: page_height as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        directional_rgba,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    directional_image.sampler = ImageSampler::linear();
    let ambient_source_name = secondary
        .name
        .and_then(|name| s.cstr(name).ok())
        .unwrap_or("<unnamed lightmap secondary>")
        .to_owned();

    if primary.format != FOURCC_DXT1 {
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
    let secondary_size = UVec2::new(u32::from(secondary.width), page_height as u32);
    if primary_size.x * secondary_size.y != primary_size.y * secondary_size.x {
        return Err(WorldLightmapGap::MismatchedAspectRatio {
            primary: primary_size,
            secondary: secondary_size,
        });
    }
    let dxt_needed =
        usize::from(primary.width).div_ceil(4) * usize::from(primary.height).div_ceil(4) * 8;
    if primary.source_len < dxt_needed {
        return Err(WorldLightmapGap::Truncated {
            needed: dxt_needed,
            have: primary.source_len,
        });
    }
    let dxt = s
        .source_slice(primary.source_offset, primary.source_len)
        .map_err(|_| WorldLightmapGap::Truncated {
            needed: dxt_needed,
            have: primary.source_len,
        })?;
    let mut rgba = decode_bc1_level(dxt, primary.width.into(), primary.height.into())
        .map_err(|_| WorldLightmapGap::UnsupportedPrimaryFormat(primary.format))?;
    if let Some((primary_flat, _)) = t5_lightmap_flat() {
        for (index, byte) in rgba.iter_mut().enumerate() {
            *byte = if index % 4 == 3 { 255 } else { primary_flat };
        }
    }

    let mut primary_image = Image::new(
        Extent3d {
            width: u32::from(primary.width),
            height: u32::from(primary.height),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba.clone(),
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    primary_image.sampler = ImageSampler::linear();

    let luma: Vec<u8> = rgba
        .chunks_exact(4)
        .map(|px| {
            ((u16::from(px[0]) * 77 + u16::from(px[1]) * 150 + u16::from(px[2]) * 29) / 256) as u8
        })
        .collect();
    let mut sun_mask_image = Image::new(
        Extent3d {
            width: u32::from(primary.width),
            height: u32::from(primary.height),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        luma,
        TextureFormat::R8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    sun_mask_image.sampler = ImageSampler::linear();
    let sun_mask_source_name = primary
        .name
        .and_then(|name| s.cstr(name).ok())
        .unwrap_or("<unnamed lightmap primary>")
        .to_owned();

    let secondary_b = page
        .secondary_b
        .ok_or(WorldLightmapGap::MissingSecondaryB)?;

    if secondary_b.format != 34 {
        return Err(WorldLightmapGap::UnsupportedSecondaryBFormat(
            secondary_b.format,
        ));
    }
    if secondary_b.width == 0 || secondary_b.height == 0 || secondary_b.depth != 1 {
        return Err(WorldLightmapGap::InvalidDimensions {
            width: secondary_b.width,
            height: secondary_b.height,
            depth: secondary_b.depth,
        });
    }
    let needed = usize::from(secondary_b.width) * usize::from(secondary_b.height) * 4;
    if secondary_b.source_len < needed {
        return Err(WorldLightmapGap::Truncated {
            needed,
            have: secondary_b.source_len,
        });
    }
    let bytes = s
        .source_slice(secondary_b.source_offset, needed)
        .map_err(|_| WorldLightmapGap::Truncated {
            needed,
            have: secondary_b.source_len,
        })?;
    let mut secondary_b_image = Image::new(
        Extent3d {
            width: secondary_b.width.into(),
            height: secondary_b.height.into(),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        bytes.to_vec(),
        TextureFormat::Rg16Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    secondary_b_image.sampler = ImageSampler::linear();

    Ok(WorldLightmap {
        primary_image: Some(primary_image),
        secondary_image: Some(secondary_image),
        secondary_b_image: Some(secondary_b_image),
        ambient_image,
        directional_image,
        sun_mask_image,
        ambient_source_name,
        sun_mask_source_name,
        ambient_size: secondary_size,
        sun_mask_size: primary_size,
    })
}

fn r5g6b5_page_payloads(
    source: &[u8],
    width: usize,
    page_height: usize,
) -> Result<(Vec<u8>, Vec<u8>), WorldLightmapGap> {
    let page_pixels = width * page_height;
    let needed = page_pixels * 2 * 2;
    let source = source.get(..needed).ok_or(WorldLightmapGap::Truncated {
        needed,
        have: source.len(),
    })?;
    let mut ambient = Vec::with_capacity(page_pixels * 4);
    let mut directional = Vec::with_capacity(page_pixels * 4);
    for (page, out) in [(0, &mut ambient), (1, &mut directional)] {
        let base = page * page_pixels * 2;
        for i in 0..page_pixels {
            let o = base + i * 2;
            let packed = u16::from_le_bytes([source[o], source[o + 1]]);
            let r = ((packed >> 11) & 0x1f) as u8;
            let g = ((packed >> 5) & 0x3f) as u8;
            let b = (packed & 0x1f) as u8;
            out.extend_from_slice(&[
                (r << 3) | (r >> 2),
                (g << 2) | (g >> 4),
                (b << 3) | (b >> 2),
                255,
            ]);
        }
    }
    Ok((ambient, directional))
}

fn decode_bc1_level(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let blocks_wide = width.div_ceil(4) as usize;
    let blocks_high = height.div_ceil(4) as usize;
    let needed = blocks_wide * blocks_high * 8;
    if data.len() < needed {
        return Err("truncated BC1 lightmap".into());
    }
    let mut out = vec![0; width as usize * height as usize * 4];
    let mut tile = [0u8; 64];
    for block_y in 0..blocks_high {
        for block_x in 0..blocks_wide {
            let offset = (block_y * blocks_wide + block_x) * 8;
            bcdec_rs::bc1(&data[offset..offset + 8], &mut tile, 16);
            for y in 0..4 {
                for x in 0..4 {
                    let dx = block_x * 4 + x;
                    let dy = block_y * 4 + y;
                    if dx < width as usize && dy < height as usize {
                        let destination = (dy * width as usize + dx) * 4;
                        let source = (y * 4 + x) * 4;
                        out[destination..destination + 4]
                            .copy_from_slice(&tile[source..source + 4]);
                    }
                }
            }
        }
    }
    Ok(out)
}
