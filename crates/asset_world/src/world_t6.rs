use crate::world_draw::*;
use crate::{SurfaceCastsSunShadow, WorldMeshStats, world_capture_from_casters};
use fastfile_t6::{LoadedAsset, Ptr, ZoneLoad};
use std::collections::HashMap;

pub struct Reader<'a>(pub &'a ZoneLoad);
impl<'a> Reader<'a> {
    pub fn bytes(&self, p: Ptr, len: usize) -> Result<&'a [u8], String> {
        self.0
            .blocks
            .bytes(p, len)
            .map_err(|e| format!("T6 data: {e:?}"))
    }
    pub fn word(b: &[u8], at: usize) -> Result<u32, String> {
        Ok(u32::from_le_bytes(
            b.get(at..at + 4)
                .ok_or("short T6 record")?
                .try_into()
                .unwrap(),
        ))
    }
    pub fn ptr(b: &[u8], at: usize) -> Result<Ptr, String> {
        let raw = Self::word(b, at)?;
        if raw == 0 || raw >= 0xffff_fffe {
            return Err("missing T6 pointer".into());
        }
        let encoded = raw - 1;
        Ok(Ptr {
            block: (encoded >> 29) as u8,
            offset: encoded & 0x1fff_ffff,
        })
    }
    pub fn u32(&self, p: Ptr) -> Result<u32, String> {
        Self::word(self.bytes(p, 4)?, 0)
    }
    pub fn f32(&self, p: Ptr) -> Result<f32, String> {
        Ok(f32::from_bits(self.u32(p)?))
    }
    pub fn u16(&self, p: Ptr) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.bytes(p, 2)?.try_into().unwrap()))
    }
    pub fn xyz(&self, p: Ptr) -> Result<[f32; 3], String> {
        Ok([self.f32(p)?, self.f32(p.at(4))?, self.f32(p.at(8))?])
    }
    pub fn xyzw(&self, p: Ptr) -> Result<[f32; 4], String> {
        let [x, y, z] = self.xyz(p)?;
        Ok([x, y, z, self.f32(p.at(12))?])
    }
    pub fn text(&self, p: Ptr) -> Result<&'a str, String> {
        core::str::from_utf8(
            self.0
                .blocks
                .cstr(p)
                .map_err(|e| format!("T6 string: {e:?}"))?,
        )
        .map_err(|e| e.to_string())
    }
}

pub fn build_world_draw(
    load: &ZoneLoad,
    asset: &LoadedAsset,
    materials: &asset_material::MaterialCatalog,
    surface_materials: Vec<Option<usize>>,
    surface_layer_formats: &[u8],
) -> Result<WorldDraw, String> {
    use asset_model::{
        half_to_f32, normalize_or_up, repack_vertex_t6, unpack_color, unpack_packed_tex_coords,
        unpack_unit_vec,
    };
    let view =
        fastfile_t6::world::WorldView::new(load, asset).map_err(|e| format!("T6 world: {e:?}"))?;
    let mut physical = HashMap::new();
    let mut packed_vertices = Vec::new();
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut tangents = Vec::new();
    let mut colors = Vec::new();
    let mut texture_uvs = Vec::new();
    let mut lightmap_uvs = Vec::new();
    let mut vertex_layer = Vec::new();
    let mut packed_indices = Vec::new();
    let mut surface_index_ranges = Vec::new();
    let mut surface_first_vertex = Vec::new();
    let mut surface_draw_fields = Vec::new();
    let mut surface_lightmap_indices = Vec::new();
    let mut surface_reflection_probes = Vec::new();
    let mut surface_primary_lights = Vec::new();
    let mut surface_vertex_layer = Vec::new();
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    let mut caster = SurfaceCastsSunShadow::with_len(view.surfaces().len());
    for (ordinal, surface) in view.surfaces().enumerate() {
        let start = packed_indices.len() as u32;
        let (layer_uvs, layer_normals) =
            layer_extras(surface_layer_formats.get(ordinal).copied().unwrap_or(0))?;
        let layer_stride = 4 * (layer_uvs + layer_normals);
        let layer_base = surface.stream_offset(1).filter(|_| layer_stride != 0);
        for bytes in surface
            .local_indices()
            .map_err(|e| format!("T6 indices: {e:?}"))?
            .as_chunks::<2>()
            .0
        {
            let local = u16::from_le_bytes(*bytes);
            let vertex = surface
                .vertex(local)
                .map_err(|e| format!("T6 vertex: {e:?}"))?;
            let layer_at = layer_base.map(|base| base + usize::from(local) * layer_stride);
            let key = (vertex.stream_offset, layer_at, layer_uvs, layer_normals);
            let index = if let Some(&index) = physical.get(&key) {
                index
            } else {
                let index = positions.len() as u32;
                let pos = vertex.position();
                if pos.iter().any(|x| !x.is_finite()) {
                    return Err("non-finite T6 position".into());
                }
                for axis in 0..3 {
                    min[axis] = min[axis].min(pos[axis]);
                    max[axis] = max[axis].max(pos[axis]);
                }
                let model = repack_vertex_t6(vertex.bytes[..32].try_into().unwrap());
                let word = |at| Reader::word(&model, at).unwrap();
                let uv = unpack_packed_tex_coords(word(20));
                let lmap = Reader::word(vertex.bytes, 32)?;
                let luv = [
                    (lmap & 0xffff) as f32 / 65535.0,
                    (lmap >> 16) as f32 / 65535.0,
                ];
                let normal = normalize_or_up(unpack_unit_vec(word(24)));
                let tangent = normalize_or_up(unpack_unit_vec(word(28)));
                let sign = f32::from_bits(word(12));
                let mut host = [0u8; asset_iw4::size::GFX_WORLD_VERTEX];
                host[..20].copy_from_slice(&model[..20]);
                for (at, value) in [(20, uv[0]), (24, uv[1]), (28, luv[0]), (32, luv[1])] {
                    host[at..at + 4].copy_from_slice(&value.to_le_bytes());
                }
                host[36..40].copy_from_slice(&model[24..28]);
                host[40..44].copy_from_slice(&model[28..32]);
                packed_vertices.push(host);
                positions.push(pos);
                normals.push(normal);
                tangents.push([tangent[0], tangent[1], tangent[2], sign]);
                colors.push(unpack_color(word(16)));
                texture_uvs.push(uv);
                lightmap_uvs.push(luv);
                let mut layer = [0u8; LAYER_HOST_STRIDE];
                if let Some(at) = layer_at {
                    let raw = view
                        .stream1
                        .get(at..at + layer_stride)
                        .ok_or("T6 layer vertex out of range")?;
                    for (i, word) in raw.as_chunks::<4>().0.iter().enumerate() {
                        if i < layer_uvs {
                            for (half, bits) in word.as_chunks::<2>().0.iter().enumerate() {
                                let at = 8 * i + 4 * half;
                                let value = half_to_f32(u16::from_le_bytes(*bits));
                                layer[at..at + 4].copy_from_slice(&value.to_le_bytes());
                            }
                        } else {
                            let at = 24 + 4 * (i - layer_uvs);
                            layer[at..at + 4].copy_from_slice(word);
                        }
                    }
                }
                vertex_layer.extend_from_slice(&layer);
                physical.insert(key, index);
                index
            };
            packed_indices.push(index);
        }
        surface_index_ranges.push((start, packed_indices.len() as u32 - start));
        surface_first_vertex.push(0);
        surface_vertex_layer.push(surface.stream_offset(1).map_or(-1, |x| x as i32));
        surface_draw_fields.push(SurfaceDrawFields {
            first_vertex: 0,
            tri_count: surface.triangle_count() as u16,
            base_index: start,
            lightmap_index: surface.lightmap_index(),
            reflection_probe_index: surface.reflection_probe_index(),
            primary_light_index: surface.primary_light_index(),
        });
        surface_lightmap_indices.push(surface.lightmap_index());
        surface_reflection_probes.push(surface.reflection_probe_index());
        surface_primary_lights.push(surface.primary_light_index());
        if surface.flags() & 1 != 0 {
            caster.set(surface.ordinal());
        }
    }
    let n = surface_index_ranges.len();
    if surface_materials.len() != n {
        return Err("T6 surface material count mismatch".into());
    }
    let lightmap_count = Reader::word(&asset.header, 408)?;
    let surface_lightmapped = surface_lightmap_indices
        .iter()
        .map(|&index| u32::from(index) < lightmap_count)
        .collect::<Vec<_>>();
    let (batches, surface_batch_ranges) = crate::world_t5::make_material_batches(
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
    let dpvs = read_dpvs(load, asset)?;
    let stats = WorldMeshStats {
        vertices: positions.len(),
        triangles: packed_indices.len() / 3,
        surfaces: n,
        min,
        max,
        bounds: Some([min[0], min[1], min[2], max[0], max[1], max[2]]),
        unrouted_surfaces: surface_materials
            .iter()
            .filter(|index| index.and_then(|i| materials.materials.get(i)).is_none())
            .count(),
        ..Default::default()
    };
    Ok(WorldDraw {
        batches,
        sky_model: None,
        lightmap: Err(WorldLightmapGap::Missing),
        stats,
        packed_vertices: WorldVertexPayload::T6(packed_vertices),
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
        capture: world_capture_from_casters(caster),
        brush_models: Vec::new(),
        brush_model_bounds: Vec::new(),
        surface_materials,
        primary_lights: primary_lights(load, &asset.header, Reader::word(&asset.header, 260)?)?,
        light_defs: Vec::new(),
        sun_primary_light_count: Reader::word(&asset.header, 260)?,
        sun_stages: Vec::new(),
        light_region_hulls: None,
        shadow_geometry: Vec::new(),
        reflection_probes: Vec::new(),
        dpvs,
        outdoor_image_name: None,
        outdoor_image: None,
        outdoor_lookup: [0; 16],
        sun_effects: None,
        t5_sun_parse_exposure: None,
        t6_exposure: Some(f32::from_bits(Reader::word(&asset.header, 184)?)),
        sky_dynamic_intensity: Some(core::array::from_fn(|i| {
            f32::from_bits(Reader::word(&asset.header, 356 + 4 * i).unwrap())
        })),
        t5_sun_light: None,
        t5_tree_scatter_intensity: None,
        t5_tree_scatter_amount: None,
        t5_exposure_volume_count: 0,
    })
}

const LAYER_HOST_STRIDE: usize = 32;

fn layer_extras(world_vert_format: u8) -> Result<(usize, usize), String> {
    match world_vert_format {
        0..=8 => {
            let (uvs, normals) = [
                (0, 0),
                (1, 0),
                (1, 1),
                (2, 0),
                (2, 1),
                (2, 2),
                (3, 0),
                (3, 1),
                (3, 2),
            ][usize::from(world_vert_format)];
            Ok((uvs, normals))
        }
        _ => Err(format!("T6 world vertex format {world_vert_format}")),
    }
}

fn read_dpvs(load: &ZoneLoad, asset: &LoadedAsset) -> Result<DpvsWorldData, String> {
    use dpvs_iw4::{AabbNodeView, Bounds, CPlane, SurfRange};
    let r = Reader(load);
    let h = &asset.header;
    let count = |at| Reader::word(h, at).map(|n| n as usize);
    let ptr = |at| Reader::ptr(h, at);
    let mut out = DpvsWorldData::new(CameraSurfRanges::new(
        CameraSurfRange {
            kind: CameraRangeKind::LitOpaque,
            begin: Reader::word(h, 792)?,
            end: Reader::word(h, 796)?,
        },
        vec![
            CameraSurfRange {
                kind: CameraRangeKind::LitTrans,
                begin: Reader::word(h, 800)?,
                end: Reader::word(h, 804)?,
            },
            CameraSurfRange {
                kind: CameraRangeKind::Emissive,
                begin: Reader::word(h, 808)?,
                end: Reader::word(h, 812)?,
            },
            CameraSurfRange {
                kind: CameraRangeKind::Emissive,
                begin: Reader::word(h, 816)?,
                end: Reader::word(h, 820)?,
            },
        ],
    ));
    out.lit_opaque_begin = Reader::word(h, 792)?;
    out.lit_opaque_end = Reader::word(h, 796)?;
    out.emissive_surfs_begin = Reader::word(h, 808)?;
    out.emissive_surfs_end = Reader::word(h, 820)?;
    out.static_surface_count = count(788)?;
    out.static_surface_count_no_decal = out.static_surface_count;
    out.cell_count = count(372)?;
    let planes = ptr(376)?;
    for i in 0..count(8)? {
        let p = planes.at(i as u32 * 20);
        out.planes.push(CPlane {
            normal: r.xyz(p)?,
            dist: r.f32(p.at(12))?,
            r#type: r.bytes(p.at(16), 1)?[0],
        });
    }
    let nodes = ptr(380)?;
    for i in 0..count(12)? {
        out.nodes.push(r.u16(nodes.at(i as u32 * 2))?);
    }
    if out.static_surface_count > 0 {
        let sorted = ptr(864)?;
        for i in 0..out.static_surface_count {
            out.sorted_surf_index.push(r.u16(sorted.at(i as u32 * 2))?);
        }
    }
    let surfaces = ptr(872)?;
    for i in 0..count(16)? {
        let p = surfaces.at(i as u32 * 80 + 56);
        out.surface_bounds
            .push(Bounds::from_mins_maxs(r.xyz(p)?, r.xyz(p.at(12))?));
    }
    let cells = ptr(392)?;
    for i in 0..out.cell_count {
        let p = cells.at(i as u32 * 48);
        let nc = r.u32(p.at(24))? as usize;
        let trees = Reader::ptr(r.bytes(p, 48)?, 28).ok();
        let mut rows = Vec::new();
        let mut smodel_ids = Vec::new();
        for j in 0..nc {
            let t = trees.ok_or("missing T6 cell trees")?.at(j as u32 * 40);
            let b = r.bytes(t, 40)?;
            let native_offset = Reader::word(b, 36)? as i32;
            if native_offset > 0 && native_offset % 40 != 0 {
                return Err("T6 visibility child offset is unaligned".into());
            }
            let child_offset = if native_offset > 0 {
                native_offset / 40 * dpvs_iw4::AABB_NODE_STRIDE as i32
            } else {
                native_offset
            };
            let sm_count = r.u16(t.at(30))? as usize;
            let sm_start = smodel_ids.len();
            if sm_count > 0 {
                let ids = Reader::ptr(b, 32)?;
                for k in 0..sm_count {
                    smodel_ids.push(r.u16(ids.at(k as u32 * 2))?);
                }
            }
            let (mins, maxs) = (r.xyz(t)?, r.xyz(t.at(12))?);
            let bounds = if (0..3).all(|k| mins[k] <= maxs[k]) {
                Bounds::from_mins_maxs(mins, maxs)
            } else {
                Bounds::default()
            };
            rows.push(AabbNodeView {
                bounds,
                child_count: r.u16(t.at(24))?,
                surface_count: r.u16(t.at(26))?,
                start_surf: r.u16(t.at(28))?,
                smodel_index_count: sm_count as u16,
                start_surf_no_decal: r.u16(t.at(28))?,
                surface_count_no_decal: r.u16(t.at(26))?,
                smodel_index_start: sm_start as u32,
                children_offset: child_offset,
            });
        }
        let root = rows.first().map_or(SurfRange::default(), |a| SurfRange {
            start: a.start_surf,
            count: a.surface_count,
        });
        out.cell_roots.push(root);
        out.aabb_trees.push(rows);
        out.aabb_smodel_indices.push(smodel_ids);
        let pc = r.u32(p.at(32))? as usize;
        let portals = Reader::ptr(r.bytes(p, 48)?, 36).ok();
        let mut owned = Vec::new();
        for j in 0..pc {
            let q = portals.ok_or("missing T6 portals")?.at(j as u32 * 92);
            let bytes = r.bytes(q, 92)?;
            let vertices = Reader::ptr(bytes, 36)?;
            let vc = usize::from(bytes[40]);
            let start = out.portal_verts.len();
            for k in 0..vc {
                out.portal_verts.push(r.xyz(vertices.at(k as u32 * 12))?);
            }
            let neighbor = r.u32(q.at(32))?;
            let cell_start = cells.offset;
            let neighbor = ((neighbor.wrapping_sub(1) & 0x1fff_ffff)
                .checked_sub(cell_start)
                .ok_or("T6 portal cell pointer")?)
                / 48;
            owned.push(OwnedPortal {
                plane: [
                    r.f32(q.at(12))?,
                    r.f32(q.at(16))?,
                    r.f32(q.at(20))?,
                    r.f32(q.at(24))?,
                ],
                neighbor: neighbor as u16,
                vert_start: start,
                vert_count: vc,
                hull_axis: Some([r.xyz(q.at(44))?, r.xyz(q.at(56))?]),
            });
        }
        out.portals_per_cell.push(owned);
        out.cell_reflection_probes.push(Vec::new());
    }
    if count(784)? > 0 {
        let instances = ptr(868)?;
        for i in 0..count(784)? {
            let p = instances.at(i as u32 * 36);
            out.smodel_bounds
                .push(Bounds::from_mins_maxs(r.xyz(p)?, r.xyz(p.at(12))?));
        }
    }
    out.checked().map_err(|e| e.to_string())
}

pub fn entity_string(load: &ZoneLoad) -> Result<&str, String> {
    let asset = load
        .assets
        .iter()
        .find(|a| a.ty == fastfile_t6::AssetType::MapEnts)
        .ok_or("T6 map entities missing")?;
    Reader(load).text(Reader::ptr(&asset.header, 4)?)
}

pub fn build_clip_collision(load: &ZoneLoad) -> Result<crate::ClipCollision, String> {
    use crate::{ClipBrush, ClipBspLeaf, ClipBspNode, ClipCmodel, ClipMapMaterial};
    let asset = load
        .assets
        .iter()
        .find(|a| {
            matches!(
                a.ty,
                fastfile_t6::AssetType::ClipMap | fastfile_t6::AssetType::ClipMapPvs
            )
        })
        .ok_or("T6 clipmap missing")?;
    let r = Reader(load);
    let h = &asset.header;
    let word = |at| Reader::word(h, at);
    let ptr = |at| Reader::ptr(h, at);
    let mut out = crate::ClipCollision::default();
    let material_count = word(16)? as usize;
    if material_count > 0 {
        let table = ptr(20)?;
        for i in 0..material_count {
            let p = table.at(i as u32 * 12);
            let b = r.bytes(p, 12)?;
            out.materials.push(ClipMapMaterial {
                name: r.text(Reader::ptr(b, 0)?)?.to_owned(),
                surface_flags: Reader::word(b, 4)?,
                content_flags: Reader::word(b, 8)?,
            });
        }
    }
    let brush_count = u16::from_le_bytes(h[64..66].try_into().unwrap()) as usize;
    if brush_count > 0 {
        let table = ptr(68)?;
        for i in 0..brush_count {
            let p = table.at(i as u32 * 96);
            let b = r.bytes(p, 96)?;
            let mins = r.xyz(p)?;
            let maxs = r.xyz(p.at(16))?;
            let mut planes = Vec::new();
            let mut flags = Vec::new();
            for side in 0..2 {
                for axis in 0..3 {
                    let mut plane = [0.0; 4];
                    plane[axis] = if side == 0 { -1.0 } else { 1.0 };
                    plane[3] = if side == 0 { -mins[axis] } else { maxs[axis] };
                    planes.push(plane);
                    flags.push(Reader::word(b, 60 + side * 12 + axis * 4)?);
                }
            }
            let count = Reader::word(b, 28)? as usize;
            if count > 0 {
                let sides = Reader::ptr(b, 32)?;
                for j in 0..count {
                    let q = sides.at(j as u32 * 12);
                    let sb = r.bytes(q, 12)?;
                    let plane = Reader::ptr(sb, 0)?;
                    planes.push([
                        r.f32(plane)?,
                        r.f32(plane.at(4))?,
                        r.f32(plane.at(8))?,
                        r.f32(plane.at(12))?,
                    ]);
                    flags.push(Reader::word(sb, 8)?);
                }
            }
            out.brushes.push(ClipBrush {
                planes,
                contents: Reader::word(b, 12)?,
                plane_surface_flags: flags,
                glass_encoded: 0,
            });
        }
    }
    let leaf_nodes = ptr(36).ok();
    let leaf_node_count = word(32)? as usize;
    fn leaf_brushes(
        r: &Reader<'_>,
        nodes: Ptr,
        node_count: usize,
        index: usize,
        out: &mut Vec<u16>,
        budget: &mut usize,
    ) -> Result<(), String> {
        if index >= node_count || *budget == 0 {
            return Err("invalid T6 leaf brush tree".into());
        }
        *budget -= 1;
        let p = nodes.at(index as u32 * 20);
        let b = r.bytes(p, 20)?;
        let count = i16::from_le_bytes(b[2..4].try_into().unwrap());
        if count > 0 {
            let ids = Reader::ptr(b, 8)?;
            for i in 0..count as u32 {
                out.push(r.u16(ids.at(i * 2))?);
            }
        } else {
            if count < 0 {
                leaf_brushes(r, nodes, node_count, index + 1, out, budget)?;
            }
            for child in [r.u16(p.at(16))?, r.u16(p.at(18))?] {
                if child != 0 {
                    leaf_brushes(
                        r,
                        nodes,
                        node_count,
                        index + usize::from(child),
                        out,
                        budget,
                    )?;
                }
            }
        }
        Ok(())
    }
    let mut read_leaf = |bytes: &[u8]| -> Result<ClipBspLeaf, String> {
        let index = Reader::word(bytes, 36)? as usize;
        let start = out.leafbrushes.len();
        if index > 0 {
            leaf_brushes(
                &r,
                leaf_nodes.ok_or("missing T6 leaf brush nodes")?,
                leaf_node_count,
                index,
                &mut out.leafbrushes,
                &mut (leaf_node_count + 1),
            )?;
        }
        let count = out.leafbrushes.len() - start;
        Ok(ClipBspLeaf {
            first_brush: start as u32,
            num_brushes: u16::try_from(count).map_err(|_| "T6 leaf has too many brushes")?,
            first_coll_aabb_index: u16::from_le_bytes(bytes[0..2].try_into().unwrap()),
            coll_aabb_count: u16::from_le_bytes(bytes[2..4].try_into().unwrap()),
        })
    };
    let leaf_count = word(100)? as usize;
    if leaf_count > 0 {
        let table = ptr(104)?;
        for i in 0..leaf_count {
            out.leaves
                .push(read_leaf(r.bytes(table.at(i as u32 * 44), 44)?)?);
        }
    }
    let model_count = word(144)? as usize;
    if model_count > 0 {
        let table = ptr(148)?;
        for i in 0..model_count {
            let p = table.at(i as u32 * 76);
            let b = r.bytes(p, 76)?;
            let leaf = read_leaf(&b[32..76])?;
            out.cmodels.push(ClipCmodel {
                mins: r.xyz(p)?,
                maxs: r.xyz(p.at(12))?,
                radius: r.f32(p.at(24))?,
                first_brush: leaf.first_brush,
                num_brushes: leaf.num_brushes,
            });
        }
    }
    let node_count = word(92)? as usize;
    if node_count > 0 {
        let table = ptr(96)?;
        for i in 0..node_count {
            let p = table.at(i as u32 * 8);
            let b = r.bytes(p, 8)?;
            let plane = Reader::ptr(b, 0)?;
            out.nodes.push(ClipBspNode {
                plane: [
                    r.f32(plane)?,
                    r.f32(plane.at(4))?,
                    r.f32(plane.at(8))?,
                    r.f32(plane.at(12))?,
                ],
                children: [
                    i16::from_le_bytes(b[4..6].try_into().unwrap()) as i32,
                    i16::from_le_bytes(b[6..8].try_into().unwrap()) as i32,
                ],
            });
        }
    }
    let mut mesh = clipmap_iw4::ClipMeshTables::default();
    let vertex_count = word(108)? as usize;
    if vertex_count > 0 {
        let table = ptr(112)?;
        for i in 0..vertex_count {
            mesh.verts.push(r.xyz(table.at(i as u32 * 12))?);
        }
    }
    let tri_count = word(116)? as usize;
    if tri_count > 0 {
        let table = ptr(120)?;
        for i in 0..tri_count * 3 {
            mesh.tri_indices.push(r.u16(table.at(i as u32 * 2))?);
        }
        let edge = ptr(124)?;
        let bits = r.bytes(edge, (tri_count * 3).div_ceil(32) * 4)?;
        for i in 0..tri_count {
            let mut value = 0;
            for e in 0..3 {
                let bit = i * 3 + e;
                value |= ((bits[bit / 8] >> (bit % 8)) & 1) << e;
            }
            mesh.tri_edge_is_walkable.push(value);
        }
    }
    let partitions = word(128)? as usize;
    if partitions > 0 {
        let table = ptr(132)?;
        for i in 0..partitions {
            let b = r.bytes(table.at(i as u32 * 16), 16)?;
            mesh.partitions.push(clipmap_iw4::ClipPartition {
                tri_count: b[0],
                first_tri: Reader::word(b, 4)? as i32,
                ..Default::default()
            });
        }
    }
    let aabb_count = word(136)? as usize;
    if aabb_count > 0 {
        let table = ptr(140)?;
        for i in 0..aabb_count {
            let p = table.at(i as u32 * 32);
            mesh.aabb_trees.push(clipmap_iw4::ClipAabbNode {
                origin: r.xyz(p)?,
                half_size: r.xyz(p.at(16))?,
                material_index: r.u16(p.at(12))?,
                child_count: r.u16(p.at(14))?,
                u: r.u32(p.at(28))? as i32,
            });
        }
    }
    mesh.aabb_roots = out
        .leaves
        .iter()
        .flat_map(|l| {
            l.first_coll_aabb_index..l.first_coll_aabb_index.saturating_add(l.coll_aabb_count)
        })
        .collect();
    mesh.aabb_roots.sort_unstable();
    mesh.aabb_roots.dedup();
    mesh.tri_surface_flags.resize(tri_count, 0);
    mesh.tri_content_flags.resize(tri_count, 0);
    out.tri_material_index.resize(tri_count, 0);
    for tree in &mesh.aabb_trees {
        if tree.child_count != 0 {
            continue;
        }
        let partition = mesh
            .partitions
            .get(tree.u as usize)
            .ok_or("T6 collision partition index")?;
        let material = out
            .materials
            .get(tree.material_index as usize)
            .ok_or("T6 collision material index")?;
        let first =
            usize::try_from(partition.first_tri).map_err(|_| "negative T6 triangle index")?;
        let end = first + usize::from(partition.tri_count);
        if end > tri_count {
            return Err("T6 collision triangles out of range".into());
        }
        for i in first..end {
            mesh.tri_surface_flags[i] = material.surface_flags;
            mesh.tri_content_flags[i] = material.content_flags;
            out.tri_material_index[i] = tree.material_index;
        }
    }
    out.mesh = std::sync::Arc::new(mesh);
    Ok(out)
}

fn sun_diffuse(world: &[u8]) -> Result<[f32; 4], String> {
    let f = |at: usize| -> Result<f32, String> {
        Ok(f32::from_le_bytes(
            world
                .get(at..at + 4)
                .ok_or("T6 sun parse params missing")?
                .try_into()
                .unwrap(),
        ))
    };
    let light = f(148)?;
    Ok([f(136)? * light, f(140)? * light, f(144)? * light, 0.0])
}

fn primary_lights(
    load: &ZoneLoad,
    world_header: &[u8],
    sun_count: u32,
) -> Result<Vec<crate::WorldPrimaryLight>, String> {
    let Some(world) = load
        .assets
        .iter()
        .find(|asset| asset.ty == fastfile_t6::AssetType::ComWorld)
    else {
        return Ok(Vec::new());
    };
    let r = Reader(load);
    let count = Reader::word(&world.header, 8)?;
    if count == 0 {
        return Ok(Vec::new());
    }
    let table = Reader::ptr(&world.header, 12)?;
    let mut lights = Vec::with_capacity(count as usize);
    for i in 0..count {
        let p = table.at(i * 196);
        let h = r.bytes(p, 196)?;
        let v4 = |off| -> Result<[f32; 4], String> {
            Ok([
                r.f32(p.at(off))?,
                r.f32(p.at(off + 4))?,
                r.f32(p.at(off + 8))?,
                r.f32(p.at(off + 12))?,
            ])
        };
        let is_sun = i != 0 && i <= sun_count;
        lights.push(crate::WorldPrimaryLight {
            is_sun,
            light_type: h[0],
            can_cast_shadow: h[1] != 0,
            exponent: h[2],
            color: r.xyz(p.at(8))?,
            direction: r.xyz(p.at(20))?,
            origin: r.xyz(p.at(32))?,
            radius: r.f32(p.at(44))?,
            cos_outer: r.f32(p.at(48))?,
            cos_inner: r.f32(p.at(52))?,
            cos_half_fov_expanded: r.f32(p.at(56))?,
            def_name: Reader::ptr(h, 192)
                .ok()
                .and_then(|p| r.text(p).ok())
                .map(str::to_owned),
            falloff_image_width: None,
            lmap_lookup_start: 0,
            attenuation_image: None,
            attenuation_sampler: 0,
            t5_attenuation: None,
            t5_falloff: Some(v4(96)?),
            t5_angle: Some(v4(112)?),
            t5_a_ab_b: Some(v4(128)?),
            t5_cookie0: Some(v4(144)?),
            t5_cookie1: Some(v4(160)?),
            t5_cookie2: Some(v4(176)?),
            t5_diffuse_color: Some(if is_sun {
                sun_diffuse(world_header)?
            } else {
                v4(80)?
            }),
            t5_specular_color: None,
        });
    }
    Ok(lights)
}

pub fn light_grid(
    load: &ZoneLoad,
    world: &LoadedAsset,
) -> Result<asset_model::OwnedLightGrid, String> {
    let r = Reader(load);
    let grid = world
        .header
        .get(464..536)
        .ok_or("T6 light grid header missing")?;
    let half = |at| u16::from_le_bytes(grid[at..at + 2].try_into().unwrap());
    let mins = [half(4), half(6), half(8)];
    let maxs = [half(10), half(12), half(14)];
    let row_axis = Reader::word(grid, 20)? as usize;
    let col_axis = Reader::word(grid, 24)? as usize;
    if row_axis > 2 || col_axis > 2 || row_axis == col_axis {
        return Err("T6 light grid axes invalid".into());
    }
    let rows = usize::from(
        maxs[row_axis]
            .checked_sub(mins[row_axis])
            .ok_or("T6 light grid bounds inverted")?,
    ) + 1;
    let raw_count = Reader::word(grid, 32)? as usize;
    let entry_count = Reader::word(grid, 40)? as usize;
    let coefficient_count = Reader::word(grid, 56)?;
    let (color_count, color_ptr, color_stride, color_encoding) = if coefficient_count != 0 {
        (
            coefficient_count,
            60,
            fastfile_t6::light_grid::COEFFICIENT_ROW_BYTES,
            asset_model::LightGridColorEncoding::T6Coefficients,
        )
    } else {
        (
            Reader::word(grid, 48)?,
            52,
            lighting_iw4::LIGHT_GRID_COLORS_BYTE_COUNT,
            asset_model::LightGridColorEncoding::Rgb8,
        )
    };
    if color_count == 0 {
        return Err("T6 light grid palette empty".into());
    }
    let data = |at, len| r.bytes(Reader::ptr(grid, at)?, len).map(<[u8]>::to_vec);
    let entries = data(44, entry_count * 4)?;
    if entries.as_chunks::<4>().0.iter().any(|entry| {
        let index = u16::from_le_bytes([entry[0], entry[1]]);
        index != u16::MAX && u32::from(index) >= color_count
    }) {
        return Err("T6 light grid color index invalid".into());
    }
    Ok(asset_model::OwnedLightGrid {
        mins,
        maxs,
        row_axis,
        col_axis,
        color_count,
        has_light_regions: false,
        sun_primary_light_index: Reader::word(grid, 0)?,
        row_data_start: data(28, rows * 2)?,
        raw_row_data: data(36, raw_count)?,
        entries,
        colors: data(color_ptr, color_count as usize * color_stride)?,
        color_encoding,
    })
}

pub fn static_model_placements(
    load: &ZoneLoad,
    world: &LoadedAsset,
    mut mesh_of: impl FnMut(Ptr) -> Option<usize>,
) -> Result<Vec<Option<crate::StaticModelPlacement>>, String> {
    use bevy::math::{Mat3, Quat, Vec3};
    let r = Reader(load);
    let count = Reader::word(&world.header, 784)?;
    if count == 0 {
        return Ok(Vec::new());
    }
    let rows = Reader::ptr(&world.header, 876)?;
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count {
        let row = rows.at(i * 152);
        let Some(mesh) = mesh_of(row.at(56)) else {
            out.push(None);
            continue;
        };
        let origin = r.xyz(row.at(4))?;
        let axis = [r.xyz(row.at(16))?, r.xyz(row.at(28))?, r.xyz(row.at(40))?];
        let scale = r.f32(row.at(52))?;
        let cull_dist = r.f32(row.at(0))?;
        let tail = r.bytes(row.at(96), 3)?;
        let basis = Mat3::from_cols(
            Vec3::from_array(axis[0]),
            Vec3::from_array(axis[1]),
            Vec3::from_array(axis[2]),
        );
        out.push(Some(crate::StaticModelPlacement {
            mesh,
            transform: bevy::prelude::Transform {
                translation: Vec3::from_array(origin),
                rotation: Quat::from_mat3(&basis),
                scale: Vec3::splat(scale),
            },
            origin,
            axis,
            scale,
            cull_dist: if cull_dist.is_finite() {
                cull_dist.clamp(0.0, f32::from(u16::MAX)) as u16
            } else {
                0
            },
            reflection_probe_index: tail[2],
            primary_light_index: tail[0],
            flags: 0,
        }));
    }
    Ok(out)
}

pub fn static_model_vertex_lighting(
    load: &ZoneLoad,
    world: &LoadedAsset,
) -> Result<Vec<Option<[Vec<[u8; 4]>; 4]>>, String> {
    let r = Reader(load);
    let count = Reader::word(&world.header, 784)?;
    if count == 0 {
        return Ok(Vec::new());
    }
    let rows = Reader::ptr(&world.header, 876)?;
    (0..count)
        .map(|i| {
            let mut lods: [Vec<[u8; 4]>; 4] = Default::default();
            for (lod, colors) in lods.iter_mut().enumerate() {
                let info = r.bytes(rows.at(i * 152 + 104 + lod as u32 * 12), 12)?;
                let n = usize::from(u16::from_le_bytes([info[8], info[9]]));
                if let (Ok(p), true) = (Reader::ptr(info, 0), n > 0) {
                    *colors = r
                        .bytes(p, n * 4)?
                        .chunks_exact(4)
                        .map(|c| [c[0], c[1], c[2], c[3]])
                        .collect();
                }
            }
            Ok(lods.iter().any(|lod| !lod.is_empty()).then_some(lods))
        })
        .collect()
}

pub fn static_model_lighting_origins(
    load: &ZoneLoad,
    world: &LoadedAsset,
) -> Result<Vec<(usize, [f32; 3])>, String> {
    let r = Reader(load);
    let count = Reader::word(&world.header, 784)?;
    if count == 0 {
        return Ok(Vec::new());
    }
    let instances = Reader::ptr(&world.header, 868)?;
    (0..count)
        .map(|i| Ok((i as usize, r.xyz(instances.at(i * 36 + 24))?)))
        .collect()
}

pub fn model_mesh(skel: asset_model::ModelSkel) -> Result<crate::ModelMesh, String> {
    use bevy::asset::RenderAssetUsages;
    use bevy::prelude::Mesh;
    use bevy::render::mesh::{Indices, PrimitiveTopology};
    let mut surfaces = Vec::new();
    for (surface, &(first, count)) in skel.surface_vertex_ranges.iter().enumerate() {
        let end = first
            .checked_add(count)
            .ok_or("T6 model vertex range overflow")?;
        let &(start, length) = skel
            .surface_index_ranges
            .get(surface)
            .ok_or("T6 model index range missing")?;
        let mut indices = Vec::new();
        for &index in skel
            .indices
            .get(start..start + length)
            .ok_or("T6 model indices missing")?
        {
            let local = (index as usize)
                .checked_sub(first)
                .filter(|&index| index < count)
                .ok_or("T6 model index outside surface")?;
            indices.push(local as u32);
        }
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            skel.positions[first..end].to_vec(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, skel.normals[first..end].to_vec());
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, skel.colors[first..end].to_vec());
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, skel.uvs[first..end].to_vec());
        mesh.insert_indices(Indices::U32(indices));
        surfaces.push(crate::model_mesh::ModelSurfaceDraw {
            mesh,
            material: skel
                .surface_materials
                .get(surface)
                .copied()
                .flatten()
                .map(|index| index.get()),
            packed_vertices: skel.packed_vertices[first..end].to_vec(),
            xsurface_plus_1: Some(0),
            xsurface_base_index: 0,
            xsurface_vert_offset: 0,
            collision: crate::model_mesh::XSurfaceCollisionPayload::Unavailable {
                source_layout: "T6 model collision not retained",
            },
        });
    }
    let mut lod_surfaces: [Vec<crate::model_mesh::ModelSurfaceDraw>; 4] = Default::default();
    for (lod, &(first, count)) in skel.lod_surf_span.iter().enumerate() {
        let first = usize::from(first);
        let end = first + usize::from(count);
        if end > surfaces.len() {
            return Err("T6 model LOD surface range invalid".into());
        }
        lod_surfaces[lod] = surfaces[first..end].to_vec();
    }
    Ok(crate::ModelMesh {
        name: skel.name,
        lod_surfaces,
        vertices: skel.positions.len(),
        triangles: skel.indices.len() / 3,
        lod_smc: skel.lod_smc,
        lod: skel.lod,
    })
}
