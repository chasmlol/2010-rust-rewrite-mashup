use asset_iw4::size as sz;
use bevy::prelude::{Quat, Vec3};
use fastfile_iw4::{Ptr, ScriptStrings, XModelGeometry, ZonePtr, ZoneStream};

use crate::{
    ModelKind, dobj::ModelPoseSrc, fpv_catalog::TagViewBind, model_kind, normalize_or_up,
    unpack_color, unpack_packed_tex_coords, unpack_unit_vec,
};
use asset_material::MaterialCatalog;
pub use xmodel_runtime::BoneCollision;

const BONE_STRIDE: u16 = 64;

pub fn lod_surface_range(span: [(u16, u16); 4], total: usize, lod: u8) -> core::ops::Range<usize> {
    let i = usize::from(lod).min(3);
    if span.iter().all(|&(_, count)| count == 0) {
        return if i == 0 { 0..total } else { 0..0 };
    }
    let (start, count) = span[i];
    let start = usize::from(start).min(total);
    let end = start.saturating_add(usize::from(count)).min(total);
    start..end
}

pub fn xmodel_lod_for_dist(num_lods: u8, lod_dist: [f32; 4], dist: f32) -> Option<u8> {
    if num_lods == 0 {
        return Some(0);
    }
    let n = usize::from(num_lods).min(4);
    for lod in 0..n {
        let d = lod_dist[lod];
        if d == 0.0 || d > dist {
            return Some(lod as u8);
        }
    }
    None
}

pub fn has_lod_for_dist(submodels: impl IntoIterator<Item = (u8, [f32; 4])>, dist: f32) -> bool {
    let mut saw = false;
    for (num_lods, lod_dist) in submodels {
        saw = true;
        if xmodel_lod_for_dist(num_lods, lod_dist, dist).is_some() {
            return true;
        }
    }
    !saw
}

#[derive(Clone, Debug)]
pub struct ModelSkel {
    pub name: String,
    pub bones: Vec<BoneBind>,

    pub bone_collision: Vec<Option<BoneCollision>>,

    pub bone_names: Vec<String>,
    pub tag_view: Option<usize>,
    pub tag_weapon: Option<usize>,

    pub pose: Option<ModelPoseSrc>,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,

    pub surface_materials: Vec<Option<crate::WalkLocalMaterialIndex>>,

    pub surface_vertex_ranges: Vec<(usize, usize)>,

    pub surface_index_ranges: Vec<(usize, usize)>,

    pub surface_part_bits: Vec<[u32; 6]>,

    pub surface_deformed: Vec<Option<bool>>,

    pub surface_vert_list_count: Vec<Option<u32>>,

    pub vert_skin: Vec<VertSkin>,
    pub rigid_verts: usize,
    pub blend_verts: usize,

    pub packed_vertices: Vec<[u8; sz::GFX_PACKED_VERTEX]>,

    pub radius: Option<f32>,

    pub bounds: Option<([f32; 3], [f32; 3])>,

    pub contents: Option<u32>,

    pub coll_lod: i16,

    pub coll_surfs: Vec<xmodel_runtime::CollSurfCollision>,

    pub movement_brushes: Vec<xmodel_runtime::ModelMovementBrush>,

    pub mount_tag: Option<String>,

    pub lod: Option<crate::ModelLodSelector>,

    pub lod_smc: Option<[[u8; 4]; 4]>,

    pub lod_part_bits: Option<[[u32; 6]; 4]>,

    pub lod_surf_span: [(u16, u16); 4],
}

impl ModelSkel {
    pub fn retained_capability(&self) -> Option<xmodel_runtime::RetainedModelCapability> {
        Some(xmodel_runtime::RetainedModelCapability {
            key: self.name.clone(),
            pose: self.pose.clone()?,
            bone_collision: self.bone_collision.clone(),
            contents: self.contents,
            coll_lod: self.coll_lod,
            coll_surfs: self.coll_surfs.clone(),
            movement_brushes: self.movement_brushes.clone(),
            bounds: self.bounds,
            radius: self.radius,
        })
    }

    pub fn parent_of(&self, bone: usize) -> Option<usize> {
        let pose = self.pose.as_ref()?;
        if bone < pose.num_root_bones {
            return None;
        }
        let child = bone - pose.num_root_bones;
        let step = *pose.parent_list.get(child)? as usize;
        bone.checked_sub(step).filter(|p| *p < bone && step > 0)
    }

    pub fn tag_camera(&self) -> Option<usize> {
        self.bone_names
            .iter()
            .position(|n| n.eq_ignore_ascii_case("tag_camera"))
    }

    pub fn surfaces_for_lod(&self, lod: u8) -> core::ops::Range<usize> {
        lod_surface_range(self.lod_surf_span, self.surface_vertex_ranges.len(), lod)
    }

    #[must_use]
    pub fn posed_bounds_part_bits(&self, lod: u8) -> [u32; 6] {
        if let Some(rows) = self.lod_part_bits {
            return rows[usize::from(lod).min(3)];
        }
        let mut local = [0u32; 6];
        for surf in self.surfaces_for_lod(lod) {
            if let Some(bits) = self.surface_part_bits.get(surf) {
                for (dst, src) in local.iter_mut().zip(bits.iter()) {
                    *dst |= *src;
                }
            }
        }
        local
    }
}

pub type FpvSkel = ModelSkel;

#[derive(Clone, Copy, Debug, Default)]
pub struct VertSkin {
    pub bones: [u16; 4],
    pub weights: [f32; 4],

    pub weight_u16: [u16; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct BoneBind {
    pub quat: [f32; 4],
    pub trans: [f32; 3],
}

impl BoneBind {
    pub fn to_mat4(self) -> bevy::math::Mat4 {
        let q = Quat::from_xyzw(self.quat[0], self.quat[1], self.quat[2], self.quat[3]);
        bevy::math::Mat4::from_rotation_translation(q, Vec3::from_array(self.trans))
    }
}

impl From<BoneBind> for TagViewBind {
    fn from(b: BoneBind) -> Self {
        TagViewBind {
            quat: b.quat,
            trans: b.trans,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SharedXModelSurfaces(
    pub std::collections::BTreeMap<String, (std::sync::Arc<ModelSkel>, usize)>,
);

impl SharedXModelSurfaces {
    pub fn retain(
        &mut self,
        stream: &ZoneStream<'_>,
        geometry: XModelGeometry,
        skel: std::sync::Arc<ModelSkel>,
    ) {
        for lod in 0..4 {
            if geometry.lod_xsurfaces[lod].is_some()
                && let Some(name) =
                    geometry.lod_surface_names[lod].and_then(|p| stream.cstr(p).ok())
                && !name.starts_with(',')
            {
                self.0.insert(name.to_owned(), (skel.clone(), lod));
            }
        }
    }

    pub fn retain_iw5(
        &mut self,
        stream: &fastfile_iw5::ZoneStream<'_>,
        geometry: fastfile_iw5::XModelGeometry,
        skel: std::sync::Arc<ModelSkel>,
    ) {
        if geometry.surfaces.is_some()
            && let Some(name) = geometry.surfaces_name.and_then(|p| stream.cstr(p).ok())
            && !name.starts_with(',')
        {
            self.0.insert(name.to_owned(), (skel, 0));
        }
    }
}

pub fn capture_xmodel_skel_with_shared(
    stream: &ZoneStream<'_>,
    strings: &ScriptStrings,
    geometry: XModelGeometry,
    materials: Option<&MaterialCatalog>,
    shared: &SharedXModelSurfaces,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    capture_model_skel_iw4(stream, strings, geometry, name, materials, Some(shared))
}

pub fn capture_xmodel_skel(
    stream: &ZoneStream<'_>,
    strings: &ScriptStrings,
    geometry: XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    capture_model_skel_iw4(stream, strings, geometry, name, materials, None)
}

pub fn capture_fpv_skel(
    stream: &ZoneStream<'_>,
    strings: &ScriptStrings,
    geometry: XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    if model_kind(&name) != Some(ModelKind::Fpv) {
        return None;
    }
    capture_model_skel_iw4(stream, strings, geometry, name, materials, None)
}

pub fn capture_body_skel(
    stream: &ZoneStream<'_>,
    strings: &ScriptStrings,
    geometry: XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    if model_kind(&name) != Some(ModelKind::Soldier) {
        return None;
    }
    capture_model_skel_iw4(stream, strings, geometry, name, materials, None)
}

pub fn capture_world_weapon_skel(
    stream: &ZoneStream<'_>,
    strings: &ScriptStrings,
    geometry: XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    if model_kind(&name) != Some(ModelKind::WorldWeapon) {
        return None;
    }
    capture_model_skel_iw4(stream, strings, geometry, name, materials, None)
}

pub fn capture_untyped_skel(
    stream: &ZoneStream<'_>,
    strings: &ScriptStrings,
    geometry: XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    capture_model_skel_iw4(stream, strings, geometry, name, materials, None)
}

fn capture_model_skel_iw4(
    stream: &ZoneStream<'_>,
    strings: &ScriptStrings,
    geometry: XModelGeometry,
    name: String,
    materials: Option<&MaterialCatalog>,
    shared: Option<&SharedXModelSurfaces>,
) -> Option<ModelSkel> {
    let bone_names = geometry.bone_names?;
    let base_mat = geometry.base_mat?;

    let mut bones = Vec::with_capacity(geometry.num_bones);
    let mut bone_name_strs = Vec::with_capacity(geometry.num_bones);
    let mut tag_view = None;
    let mut tag_weapon = None;
    for i in 0..geometry.num_bones {
        let id = stream.u16_at(bone_names, i * 2).ok()?;
        let bone_name = strings.get(stream, id).unwrap_or("").to_owned();
        if bone_name == "tag_view" && tag_view.is_none() {
            tag_view = Some(i);
        }
        if bone_name == "tag_weapon" && tag_weapon.is_none() {
            tag_weapon = Some(i);
        }
        bone_name_strs.push(bone_name);
        let m = base_mat.at(i * sz::DOBJ_ANIM_MAT);
        bones.push(BoneBind {
            quat: [
                stream.f32_at(m, 0).ok()?,
                stream.f32_at(m, 4).ok()?,
                stream.f32_at(m, 8).ok()?,
                stream.f32_at(m, 12).ok()?,
            ],
            trans: [
                stream.f32_at(m, 16).ok()?,
                stream.f32_at(m, 20).ok()?,
                stream.f32_at(m, 24).ok()?,
            ],
        });
    }

    let num_child = geometry.num_bones.saturating_sub(geometry.num_root_bones);
    let pose = capture_pose_src(stream, &geometry, &bone_name_strs, &bones, num_child);
    let bone_collision = capture_bone_collision(stream, geometry, geometry.num_bones);

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut colors = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    let mut surface_materials = Vec::new();
    let mut surface_vertex_ranges = Vec::new();
    let mut surface_index_ranges = Vec::new();
    let mut surface_part_bits = Vec::new();
    let mut surface_deformed = Vec::new();
    let mut surface_vert_list_count = Vec::new();
    let mut vert_skin = Vec::new();
    let mut rigid_verts = 0usize;
    let mut blend_verts = 0usize;
    let mut packed_vertices = Vec::new();
    let mut lod_surf_span = [(0u16, 0u16); 4];
    let mut any_surf = false;

    for lod in 0..4 {
        let Some(surfaces) = geometry.lod_xsurfaces[lod] else {
            let count = usize::from(geometry.lod_numsurfs[lod]);
            if count == 0 {
                continue;
            }
            let name = stream
                .cstr(geometry.lod_surface_names[lod]?)
                .ok()?
                .trim_start_matches(',');
            let (source, source_lod) = shared?.0.get(name)?;
            let (first, source_count) = source.lod_surf_span[*source_lod];
            if usize::from(source_count) != count {
                return None;
            }
            let start_surface = surface_vertex_ranges.len();
            for offset in 0..count {
                let n = usize::from(first) + offset;
                let (v, vn) = source.surface_vertex_ranges[n];
                let (ix, ixn) = source.surface_index_ranges[n];
                if source.vert_skin[v..v + vn].iter().any(|skin| {
                    skin.bones.iter().zip(skin.weights).any(|(bone, weight)| {
                        weight != 0.0 && usize::from(*bone) >= geometry.num_bones
                    })
                }) {
                    return None;
                }
                let base = positions.len();
                let index_start = indices.len();
                positions.extend_from_slice(&source.positions[v..v + vn]);
                normals.extend_from_slice(&source.normals[v..v + vn]);
                colors.extend_from_slice(&source.colors[v..v + vn]);
                uvs.extend_from_slice(&source.uvs[v..v + vn]);
                packed_vertices.extend_from_slice(&source.packed_vertices[v..v + vn]);
                for skin in &source.vert_skin[v..v + vn] {
                    if skin.weights[1] == 0.0 && skin.weights[0] == 1.0 {
                        rigid_verts += 1;
                    } else {
                        blend_verts += 1;
                    }
                    vert_skin.push(*skin);
                }
                indices.extend(
                    source.indices[ix..ix + ixn]
                        .iter()
                        .map(|index| base as u32 + *index - v as u32),
                );
                surface_vertex_ranges.push((base, vn));
                surface_index_ranges.push((index_start, ixn));
                surface_part_bits.push(source.surface_part_bits[n]);
                surface_deformed.push(source.surface_deformed[n]);
                surface_vert_list_count.push(source.surface_vert_list_count[n]);
                let handle_slot = usize::from(geometry.lod_surf_index[lod]) + offset;
                surface_materials.push(geometry.material_handles.and_then(|handles| {
                    materials?.material_index(handles.at(handle_slot * stream.pointer_bytes()))
                }));
            }
            lod_surf_span[lod] = (u16::try_from(start_surface).ok()?, source_count);
            any_surf = true;
            continue;
        };
        let count = usize::from(geometry.lod_numsurfs[lod]);
        if count == 0 {
            continue;
        }
        let start = surface_vertex_ranges.len();
        let handle_base = usize::from(geometry.lod_surf_index[lod]);
        for surface_index in 0..count {
            let surface = surfaces.at(surface_index * stream.layout(sz::XSURFACE, 88));
            let vertex_count = stream.u16_at(surface, 2).ok()? as usize;
            let tri_count = stream.u16_at(surface, 4).ok()? as usize;
            let deformed = stream.u8_at(surface, 1).ok()? != 0;
            let vert_list_count = stream.u32_at(surface, stream.layout(32, 48)).ok()? as usize;
            let mut part_bits = [0u32; 6];
            for (i, word) in part_bits.iter_mut().enumerate() {
                *word = stream.u32_at(surface, stream.layout(40, 64) + i * 4).ok()?;
            }
            let vertices = match stream.ptr_at(surface, stream.layout(28, 40)).ok()? {
                ZonePtr::Offset(p) => stream.resolve_alias(p),
                _ => return None,
            };
            let triangles = match stream.ptr_at(surface, stream.layout(12, 16)).ok()? {
                ZonePtr::Offset(p) => stream.resolve_alias(p),
                _ => return None,
            };

            let base = positions.len();
            let index_start = indices.len();
            let surface_skins = decode_surface_skin(
                stream,
                surface,
                vertex_count,
                vert_list_count,
                geometry.num_bones,
            )?;

            for vertex_index in 0..vertex_count {
                let vertex = vertices.at(vertex_index * sz::GFX_PACKED_VERTEX);
                packed_vertices.push(copy_packed_vertex_iw4(stream, vertex)?);
                positions.push([
                    stream.f32_at(vertex, 0).ok()?,
                    stream.f32_at(vertex, 4).ok()?,
                    stream.f32_at(vertex, 8).ok()?,
                ]);
                colors.push(unpack_color(stream.u32_at(vertex, 16).ok()?));
                uvs.push(unpack_packed_tex_coords(stream.u32_at(vertex, 20).ok()?));
                normals.push(normalize_or_up(unpack_unit_vec(
                    stream.u32_at(vertex, 24).ok()?,
                )));
                let skin = surface_skins[vertex_index];
                if skin.weights[1] == 0.0 && skin.weights[0] == 1.0 {
                    rigid_verts += 1;
                } else {
                    blend_verts += 1;
                }
                vert_skin.push(skin);
            }
            for index_offset in 0..tri_count * 3 {
                let index = stream.u16_at(triangles, index_offset * 2).ok()?;
                if usize::from(index) >= vertex_count {
                    return None;
                }
                indices.push(base as u32 + u32::from(index));
            }
            surface_vertex_ranges.push((base, vertex_count));
            surface_index_ranges.push((index_start, indices.len() - index_start));
            surface_part_bits.push(part_bits);
            surface_deformed.push(Some(deformed));
            surface_vert_list_count.push(Some(u32::try_from(vert_list_count).unwrap_or(u32::MAX)));
            let handle_slot = handle_base.saturating_add(surface_index);
            surface_materials.push(if handle_slot < geometry.material_handle_count {
                geometry.material_handles.and_then(|handles| {
                    materials?.material_index(handles.at(handle_slot * stream.pointer_bytes()))
                })
            } else {
                None
            });
        }
        let n = surface_vertex_ranges.len().saturating_sub(start);
        lod_surf_span[lod] = (u16::try_from(start).ok()?, u16::try_from(n).ok()?);
        any_surf = any_surf || n > 0;
    }
    if !any_surf {
        return None;
    }

    Some(ModelSkel {
        name,
        bones,
        bone_collision,
        bone_names: bone_name_strs,
        tag_view,
        tag_weapon,
        pose,
        positions,
        normals,
        colors,
        uvs,
        indices,
        surface_materials,
        surface_vertex_ranges,
        surface_index_ranges,
        surface_part_bits,
        surface_deformed,
        surface_vert_list_count,
        vert_skin,
        rigid_verts,
        blend_verts,
        packed_vertices,
        radius: geometry.radius,
        bounds: match (geometry.bounds_mid, geometry.bounds_half) {
            (Some(mid), Some(half))
                if mid.iter().all(|v| v.is_finite())
                    && half.iter().all(|v| v.is_finite() && *v >= 0.0) =>
            {
                Some((mid, half))
            }
            _ => None,
        },
        contents: Some(geometry.contents),
        coll_lod: geometry.coll_lod,
        coll_surfs: capture_coll_surfs(stream, geometry),
        movement_brushes: Vec::new(),

        mount_tag: None,
        lod: Some(crate::ModelLodSelector::Iw4 {
            lod_start: geometry.lod_start,
            num_lods: geometry.num_lods,
            lod_dist: geometry.lod_dist,
        }),
        lod_smc: Some(geometry.lod_smc),
        lod_part_bits: Some(geometry.lod_part_bits),
        lod_surf_span,
    })
}

fn capture_bone_collision(
    stream: &ZoneStream<'_>,
    geometry: XModelGeometry,
    num_bones: usize,
) -> Vec<Option<BoneCollision>> {
    let (Some(info), Some(classes)) = (geometry.bone_info, geometry.part_classification) else {
        return Vec::new();
    };
    (0..num_bones)
        .map(|bone| {
            let row = info.at(bone * sz::XBONE_INFO);
            let radius_sq = stream.f32_at(row, 24).ok()?;
            if radius_sq == 0.0 {
                return None;
            }
            let midpoint = [
                stream.f32_at(row, 0).ok()?,
                stream.f32_at(row, 4).ok()?,
                stream.f32_at(row, 8).ok()?,
            ];
            let half_size = [
                stream.f32_at(row, 12).ok()?,
                stream.f32_at(row, 16).ok()?,
                stream.f32_at(row, 20).ok()?,
            ];
            let finite = radius_sq.is_finite()
                && radius_sq > 0.0
                && midpoint.iter().all(|value| value.is_finite())
                && half_size
                    .iter()
                    .all(|value| value.is_finite() && *value >= 0.0);
            let part_classification = stream.u8_at(classes, bone).ok()?;
            finite.then_some(BoneCollision {
                midpoint,
                half_size,
                radius_sq,
                part_classification,
            })
        })
        .collect()
}

fn capture_coll_surfs(
    stream: &ZoneStream<'_>,
    geometry: XModelGeometry,
) -> Vec<xmodel_runtime::CollSurfCollision> {
    let Some(arr) = geometry.coll_surfs else {
        return Vec::new();
    };
    (0..geometry.num_coll_surfs.max(0) as usize)
        .filter_map(|i| {
            let row = arr.at(i * stream.layout(sz::XMODEL_COLL_SURF, 48));
            let midpoint = [
                stream.f32_at(row, stream.layout(8, 12)).ok()?,
                stream.f32_at(row, stream.layout(12, 16)).ok()?,
                stream.f32_at(row, stream.layout(16, 20)).ok()?,
            ];
            let half_size = [
                stream.f32_at(row, stream.layout(20, 24)).ok()?,
                stream.f32_at(row, stream.layout(24, 28)).ok()?,
                stream.f32_at(row, stream.layout(28, 32)).ok()?,
            ];
            if !midpoint.iter().all(|v| v.is_finite())
                || !half_size.iter().all(|v| v.is_finite() && *v >= 0.0)
            {
                return None;
            }
            let bone = u16::try_from(stream.i32_at(row, stream.layout(32, 36)).ok()?).ok()?;
            let contents = stream.u32_at(row, stream.layout(36, 40)).ok()?;
            let surf_flags = stream.u32_at(row, stream.layout(40, 44)).ok()?;
            let tris = capture_coll_tris(stream, row)?;
            Some(xmodel_runtime::CollSurfCollision {
                bone,
                contents,
                surf_flags,
                midpoint,
                half_size,
                tris,
            })
        })
        .collect()
}

fn capture_coll_tris(stream: &ZoneStream<'_>, surf: Ptr) -> Option<Vec<xmodel_runtime::CollTri>> {
    let num = stream.i32_at(surf, stream.layout(4, 8)).ok()?.max(0) as usize;
    let arr = match stream.ptr_at(surf, 0).ok()? {
        ZonePtr::Offset(p) => stream.resolve_alias(p),
        ZonePtr::Null if num == 0 => return Some(Vec::new()),
        _ => return Some(Vec::new()),
    };
    (0..num)
        .map(|i| {
            let t = arr.at(i * sz::XMODEL_COLL_TRI);
            let f = |off: usize| stream.f32_at(t, off).ok();
            Some(xmodel_runtime::CollTri {
                plane: [f(0)?, f(4)?, f(8)?, f(12)?],
                svec: [f(16)?, f(20)?, f(24)?, f(28)?],
                tvec: [f(32)?, f(36)?, f(40)?, f(44)?],
            })
        })
        .collect()
}

fn copy_packed_vertex_iw4(
    stream: &ZoneStream<'_>,
    vertex: Ptr,
) -> Option<[u8; sz::GFX_PACKED_VERTEX]> {
    let bytes = stream.slice_at(vertex, 0, sz::GFX_PACKED_VERTEX).ok()?;
    let mut packed = [0u8; sz::GFX_PACKED_VERTEX];
    packed.copy_from_slice(bytes);
    Some(packed)
}

fn copy_packed_vertex_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    vertex: fastfile_t5::Ptr,
) -> Option<[u8; sz::GFX_PACKED_VERTEX]> {
    let bytes = stream.slice_at(vertex, 0, sz::GFX_PACKED_VERTEX).ok()?;
    let mut packed = [0u8; sz::GFX_PACKED_VERTEX];
    packed.copy_from_slice(bytes);
    Some(packed)
}

fn copy_packed_vertex_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    vertex: fastfile_iw5::Ptr,
) -> Option<[u8; sz::GFX_PACKED_VERTEX]> {
    let bytes = stream.slice_at(vertex, 0, sz::GFX_PACKED_VERTEX).ok()?;
    let mut packed = [0u8; sz::GFX_PACKED_VERTEX];
    packed.copy_from_slice(bytes);
    Some(packed)
}

fn capture_pose_src(
    stream: &ZoneStream<'_>,
    geometry: &XModelGeometry,
    bone_names: &[String],
    bones: &[BoneBind],
    num_child: usize,
) -> Option<ModelPoseSrc> {
    let parent_list = if num_child == 0 {
        Vec::new()
    } else {
        let arr = geometry.parent_list?;
        (0..num_child)
            .map(|i| stream.u8_at(arr, i).ok())
            .collect::<Option<Vec<_>>>()?
    };
    let quats = if num_child == 0 {
        Vec::new()
    } else {
        let arr = geometry.quats?;
        (0..num_child)
            .map(|i| {
                let o = i * sz::XMODEL_QUAT;
                Some([
                    stream.i16_at(arr, o).ok()?,
                    stream.i16_at(arr, o + 2).ok()?,
                    stream.i16_at(arr, o + 4).ok()?,
                    stream.i16_at(arr, o + 6).ok()?,
                ])
            })
            .collect::<Option<Vec<_>>>()?
    };
    let trans = if num_child == 0 {
        Vec::new()
    } else {
        let arr = geometry.trans?;
        (0..num_child)
            .map(|i| {
                let o = i * 12;
                Some([
                    stream.f32_at(arr, o).ok()?,
                    stream.f32_at(arr, o + 4).ok()?,
                    stream.f32_at(arr, o + 8).ok()?,
                ])
            })
            .collect::<Option<Vec<_>>>()?
    };
    let base_mat = bones
        .iter()
        .map(|b| {
            (
                Quat::from_xyzw(b.quat[0], b.quat[1], b.quat[2], b.quat[3]),
                Vec3::from_array(b.trans),
            )
        })
        .collect();
    Some(ModelPoseSrc {
        name: geometry
            .name
            .and_then(|p| stream.cstr(p).ok())
            .unwrap_or("")
            .to_owned(),
        num_bones: geometry.num_bones,
        num_root_bones: geometry.num_root_bones,
        scale: geometry.scale,
        no_scale_part_bits: geometry.no_scale_part_bits,
        bone_names: bone_names.to_vec(),
        parent_list,
        quats,
        trans,
        base_mat,
        root_rest: None,
    })
}

fn decode_surface_skin(
    stream: &ZoneStream<'_>,
    surface: fastfile_iw4::Ptr,
    vertex_count: usize,
    vert_list_count: usize,
    num_bones: usize,
) -> Option<Vec<VertSkin>> {
    let mut skins = vec![VertSkin::default(); vertex_count];
    let bone_at = |raw: u16| -> Option<u16> {
        if raw % BONE_STRIDE != 0 {
            return None;
        }
        let bone = raw / BONE_STRIDE;
        ((bone as usize) < num_bones).then_some(bone)
    };

    if vert_list_count > 0 {
        if let Ok(ZonePtr::Offset(list_ptr)) = stream.ptr_at(surface, stream.layout(36, 56)) {
            let list = stream.resolve_alias(list_ptr);
            let mut vertex = 0usize;
            for i in 0..vert_list_count {
                let entry = list.at(i * stream.layout(sz::XRIGID_VERT_LIST, 16));
                let bone_offset = stream.u16_at(entry, 0).ok()?;
                let run = stream.u16_at(entry, 2).ok()? as usize;
                let bone = bone_at(bone_offset)?;
                for _ in 0..run {
                    if vertex >= vertex_count {
                        break;
                    }
                    skins[vertex] = VertSkin {
                        bones: [bone, 0, 0, 0],
                        weights: [1.0, 0.0, 0.0, 0.0],
                        ..Default::default()
                    };
                    vertex += 1;
                }
            }
            if vertex == vertex_count {
                return Some(skins);
            }
        }
    }

    let vi = surface.at(stream.layout(16, 24));
    let counts = [
        stream.i16_at(vi, 0).ok()?.max(0) as usize,
        stream.i16_at(vi, 2).ok()?.max(0) as usize,
        stream.i16_at(vi, 4).ok()?.max(0) as usize,
        stream.i16_at(vi, 6).ok()?.max(0) as usize,
    ];
    let blend_ptr = match stream.ptr_at(vi, 8).ok()? {
        ZonePtr::Offset(p) => stream.resolve_alias(p),
        ZonePtr::Null if counts.iter().all(|&c| c == 0) => {
            if num_bones > 1 {
                diag::warn!(
                    Zone,
                    "xsurface skin: {vertex_count} verts, vertListCount={vert_list_count}, \
                     no blend buckets, {num_bones} bones — cannot resolve ownership (typed gap)"
                );
            }
            for skin in &mut skins {
                *skin = VertSkin {
                    bones: [0, 0, 0, 0],
                    weights: [1.0, 0.0, 0.0, 0.0],
                    ..Default::default()
                };
            }
            return Some(skins);
        }
        _ => return None,
    };

    let mut cursor = 0usize;
    let mut vertex = 0usize;
    let take = |cursor: &mut usize, n: usize| -> Option<()> {
        *cursor = cursor.checked_add(n)?;
        Some(())
    };
    for (bucket, count) in counts.iter().enumerate() {
        let influences = bucket + 1;
        for _ in 0..*count {
            let words = 1 + (influences - 1) * 2;
            let start = cursor;
            take(&mut cursor, words)?;
            let mut skin = VertSkin::default();
            let b0 = stream.u16_at(blend_ptr, start * 2).ok()?;
            skin.bones[0] = bone_at(b0)?;
            let mut remaining = dpvs_iw4::SKIN_BLEND_WEIGHT_ONE as f32;
            for extra in 1..influences {
                let off = start + 1 + (extra - 1) * 2;
                let b = stream.u16_at(blend_ptr, off * 2).ok()?;
                let raw = stream.u16_at(blend_ptr, off * 2 + 2).ok()?;
                let w = dpvs_iw4::skin_blend_weight(raw);
                skin.bones[extra] = bone_at(b)?;
                skin.weights[extra] = w;
                skin.weight_u16[extra] = raw;
                remaining -= w;
            }
            skin.weights[0] = remaining;
            if vertex >= vertex_count {
                return None;
            }
            skins[vertex] = skin;
            vertex += 1;
        }
    }
    if vertex != vertex_count {
        return None;
    }
    Some(skins)
}

pub fn capture_fpv_skel_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    strings: &fastfile_t5::ScriptStrings,
    geometry: fastfile_t5::XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    if model_kind(&name) != Some(ModelKind::Fpv) {
        return None;
    }
    capture_model_skel_t5(stream, strings, geometry, name, materials)
}

pub fn capture_body_skel_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    strings: &fastfile_t5::ScriptStrings,
    geometry: fastfile_t5::XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    if model_kind(&name) != Some(ModelKind::Soldier) {
        return None;
    }
    capture_model_skel_t5(stream, strings, geometry, name, materials)
}

pub fn capture_world_weapon_skel_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    strings: &fastfile_t5::ScriptStrings,
    geometry: fastfile_t5::XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    if model_kind(&name) != Some(ModelKind::WorldWeapon) {
        return None;
    }
    capture_model_skel_t5(stream, strings, geometry, name, materials)
}

pub fn capture_xmodel_skel_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    strings: &fastfile_t5::ScriptStrings,
    geometry: fastfile_t5::XModelGeometry,
    materials: &MaterialCatalog,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    capture_model_skel_t5(stream, strings, geometry, name, Some(materials))
}

fn capture_model_skel_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    strings: &fastfile_t5::ScriptStrings,
    geometry: fastfile_t5::XModelGeometry,
    name: String,
    materials: Option<&MaterialCatalog>,
) -> Option<ModelSkel> {
    const XSURFACE: usize = 68;
    const DOBJ_ANIM_MAT: usize = 32;
    const GFX_PACKED_VERTEX: usize = 32;
    const XRIGID_VERT_LIST: usize = 12;

    let surfaces = geometry.surfaces?;
    let bone_names = geometry.bone_names?;
    let base_mat = geometry.base_mat?;

    let mut bones = Vec::with_capacity(geometry.num_bones);
    let mut bone_name_strs = Vec::with_capacity(geometry.num_bones);
    let mut tag_view = None;
    let mut tag_weapon = None;
    for i in 0..geometry.num_bones {
        let id = stream.u16_at(bone_names, i * 2).ok()?;
        let bone_name = strings.get(stream, id).unwrap_or("").to_owned();
        if bone_name == "tag_view" && tag_view.is_none() {
            tag_view = Some(i);
        }
        if bone_name == "tag_weapon" && tag_weapon.is_none() {
            tag_weapon = Some(i);
        }
        bone_name_strs.push(bone_name);
        let m = base_mat.at(i * DOBJ_ANIM_MAT);
        bones.push(BoneBind {
            quat: [
                stream.f32_at(m, 0).ok()?,
                stream.f32_at(m, 4).ok()?,
                stream.f32_at(m, 8).ok()?,
                stream.f32_at(m, 12).ok()?,
            ],
            trans: [
                stream.f32_at(m, 16).ok()?,
                stream.f32_at(m, 20).ok()?,
                stream.f32_at(m, 24).ok()?,
            ],
        });
    }

    let num_child = geometry.num_bones.saturating_sub(geometry.num_root_bones);
    let pose = capture_pose_src_t5(stream, &geometry, &bone_name_strs, &bones, num_child);

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut colors = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    let mut surface_materials = Vec::with_capacity(geometry.surface_count);
    let mut surface_vertex_ranges = Vec::with_capacity(geometry.surface_count);
    let mut surface_index_ranges = Vec::with_capacity(geometry.surface_count);
    let mut vert_skin = Vec::new();
    let mut rigid_verts = 0usize;
    let mut blend_verts = 0usize;
    let mut packed_vertices = Vec::new();
    let mut surface_part_bits = Vec::with_capacity(geometry.surface_count);
    let mut surface_deformed = Vec::with_capacity(geometry.surface_count);
    let mut surface_vert_list_count = Vec::with_capacity(geometry.surface_count);

    for surface_index in 0..geometry.surface_count {
        let surface = surfaces.at(surface_index * XSURFACE);
        let vert_list_count = stream.u8_at(surface, 1).ok()? as usize;

        surface_deformed.push(Some(stream.u16_at(surface, 2).ok()? & 0x80 != 0));
        surface_vert_list_count.push(Some(vert_list_count as u32));
        let vertex_count = stream.u16_at(surface, 4).ok()? as usize;
        let tri_count = stream.u16_at(surface, 6).ok()? as usize;
        let vertices = match stream.ptr_at(surface, 32).ok()? {
            fastfile_t5::ZonePtr::Offset(p) => stream.resolve_alias(p),
            _ => return None,
        };
        let triangles = match stream.ptr_at(surface, 12).ok()? {
            fastfile_t5::ZonePtr::Offset(p) => stream.resolve_alias(p),
            _ => return None,
        };
        let mut part_bits = [0u32; 6];
        for i in 0..fastfile_t5::size::XSURFACE_PART_BITS_WORDS {
            part_bits[i] = stream
                .u32_at(surface, fastfile_t5::size::XSURFACE_PART_BITS_OFF + i * 4)
                .ok()?;
        }

        let base = positions.len();
        let index_start = indices.len();
        let surface_skins = decode_surface_skin_t5(
            stream,
            surface,
            vertex_count,
            vert_list_count,
            geometry.num_bones,
            XRIGID_VERT_LIST,
        )?;

        for vertex_index in 0..vertex_count {
            let vertex = vertices.at(vertex_index * GFX_PACKED_VERTEX);
            packed_vertices.push(copy_packed_vertex_t5(stream, vertex)?);
            positions.push([
                stream.f32_at(vertex, 0).ok()?,
                stream.f32_at(vertex, 4).ok()?,
                stream.f32_at(vertex, 8).ok()?,
            ]);
            normals.push(normalize_or_up(unpack_unit_vec(
                stream.u32_at(vertex, 24).ok()?,
            )));
            colors.push(unpack_color(stream.u32_at(vertex, 16).ok()?));
            uvs.push(unpack_packed_tex_coords(stream.u32_at(vertex, 20).ok()?));
            let skin = surface_skins[vertex_index];
            if skin.weights[1] == 0.0 && skin.weights[0] == 1.0 {
                rigid_verts += 1;
            } else {
                blend_verts += 1;
            }
            vert_skin.push(skin);
        }
        for index_offset in 0..tri_count * 3 {
            let index = stream.u16_at(triangles, index_offset * 2).ok()?;
            if usize::from(index) >= vertex_count {
                return None;
            }
            indices.push(base as u32 + u32::from(index));
        }
        surface_vertex_ranges.push((base, vertex_count));
        surface_index_ranges.push((index_start, indices.len() - index_start));
        surface_part_bits.push(part_bits);
        surface_materials.push(geometry.material_handles.and_then(|handles| {
            materials?.material_index(Ptr {
                block: handles.block,
                offset: handles.offset + (surface_index * 4) as u32,
            })
        }));
    }

    let (bone_collision, coll_surfs) = capture_collision_t5(stream, geometry).or_else(|| {
        diag::warn!(Zone, "T5 XModel {name}: malformed collision payload");
        None
    })?;
    Some(ModelSkel {
        name,
        bones,
        bone_collision,
        bone_names: bone_name_strs,
        tag_view,
        tag_weapon,
        pose,
        positions,
        normals,
        colors,
        uvs,
        indices,
        surface_materials,
        surface_vertex_ranges,
        surface_index_ranges,
        surface_part_bits,
        surface_deformed,
        surface_vert_list_count,
        vert_skin,
        rigid_verts,
        blend_verts,
        packed_vertices,
        radius: geometry.radius,
        bounds: None,
        contents: Some(geometry.contents),
        coll_lod: geometry.coll_lod,
        coll_surfs,
        movement_brushes: capture_movement_brushes_t5(stream, geometry)?,

        mount_tag: None,
        lod: Some(crate::ModelLodSelector::T5 {
            num_lods: geometry.num_lods,
            lod_dist: geometry.lod_dist,
        }),
        lod_smc: None,
        lod_part_bits: None,
        lod_surf_span: geometry.lod_surf_span,
    })
}

fn capture_movement_brushes_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    geometry: fastfile_t5::XModelGeometry,
) -> Option<Vec<xmodel_runtime::ModelMovementBrush>> {
    use fastfile_t5::{Ptr, ZonePtr, size};
    let pointer = |p: Ptr, offset| match stream.ptr_at(p, offset).ok()? {
        ZonePtr::Offset(p) => Some(stream.resolve_alias(p)),
        _ => None,
    };
    if geometry.collision_map_count == 0 {
        return Some(Vec::new());
    }
    let maps = geometry.collision_maps?;
    if stream.ptr_at(maps, 0).ok()? == ZonePtr::Null {
        return Some(Vec::new());
    }
    let list = pointer(maps, 0)?;
    let count = stream.u32_at(list, 0).ok()? as usize;
    if count == 0 {
        return Some(Vec::new());
    }
    let geoms = pointer(list, 4)?;
    let mut brushes = Vec::new();
    for i in 0..count {
        let geom = geoms.at(i * size::PHYS_GEOM_INFO);
        if stream.ptr_at(geom, 0).ok()? == ZonePtr::Null {
            continue;
        }
        let brush = pointer(geom, 0)?;
        let mut planes = Vec::new();
        let mut plane_surface_flags = Vec::new();
        for side in 0..2 {
            for axis in 0..3 {
                let mut plane = [0.0; 4];
                plane[axis] = if side == 0 { -1.0 } else { 1.0 };
                plane[3] = stream.f32_at(brush, side * 16 + axis * 4).ok()? * plane[axis];
                planes.push(plane);
                plane_surface_flags.push(stream.u32_at(brush, 60 + (side * 3 + axis) * 4).ok()?);
            }
        }
        let side_count = stream
            .u32_at(brush, size::BRUSH_WRAPPER_NUMSIDES_OFF)
            .ok()? as usize;
        if side_count != 0 {
            let sides = pointer(brush, size::BRUSH_WRAPPER_SIDES_OFF)?;
            for j in 0..side_count {
                let side = sides.at(j * size::CBRUSH_SIDE);
                let plane = pointer(side, 0)?;
                planes.push([
                    stream.f32_at(plane, 0).ok()?,
                    stream.f32_at(plane, 4).ok()?,
                    stream.f32_at(plane, 8).ok()?,
                    stream.f32_at(plane, 12).ok()?,
                ]);
                plane_surface_flags.push(stream.u32_at(side, 8).ok()?);
            }
        }
        brushes.push(xmodel_runtime::ModelMovementBrush {
            planes,
            contents: stream.u32_at(brush, 12).ok()?,
            plane_surface_flags,
        });
    }
    Some(brushes)
}

#[allow(clippy::type_complexity)]
fn capture_collision_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    geometry: fastfile_t5::XModelGeometry,
) -> Option<(
    Vec<Option<BoneCollision>>,
    Vec<xmodel_runtime::CollSurfCollision>,
)> {
    use fastfile_t5::{Ptr, ZonePtr, size as t5sz};
    let bounds = |row: Ptr, off| {
        let mut midpoint = [0.0; 3];
        let mut half_size = [0.0; 3];
        for axis in 0..3 {
            let min = stream.f32_at(row, off + axis * 4).ok()?;
            let max = stream.f32_at(row, off + 12 + axis * 4).ok()?;
            if !min.is_finite() || !max.is_finite() || min > max {
                return None;
            }
            midpoint[axis] = (min + max) * 0.5;
            half_size[axis] = (max - min) * 0.5;
        }
        Some((midpoint, half_size))
    };
    let mut bones = Vec::new();
    if let Some(info) = geometry.bone_info {
        let classes = geometry.part_classification?;
        for bone in 0..geometry.num_bones {
            let row = info.at(bone * t5sz::XBONE_INFO);

            let radius_sq = stream.f32_at(row, 36).ok()?;
            if radius_sq == 0.0 {
                bones.push(None);
                continue;
            }
            if !radius_sq.is_finite() || radius_sq < 0.0 {
                return None;
            }
            let (midpoint, half_size) = bounds(row, 0)?;
            bones.push(Some(BoneCollision {
                midpoint,
                half_size,
                radius_sq,
                part_classification: stream.u8_at(classes, bone).ok()?,
            }));
        }
    }
    let mut surfs = Vec::with_capacity(geometry.num_coll_surfs);
    if geometry.num_coll_surfs != 0 {
        let arr = geometry.coll_surfs?;
        for i in 0..geometry.num_coll_surfs {
            let row = arr.at(i * t5sz::XMODEL_COLL_SURF);
            let (midpoint, half_size) = bounds(row, 8)?;
            let bone = u16::try_from(stream.i32_at(row, 32).ok()?).ok()?;
            if usize::from(bone) >= geometry.num_bones {
                return None;
            }
            let num = usize::try_from(stream.i32_at(row, 4).ok()?).ok()?;
            let mut tris = Vec::with_capacity(num);
            if num != 0 {
                let ZonePtr::Offset(arr) = stream.ptr_at(row, 0).ok()? else {
                    return None;
                };
                let arr = stream.resolve_alias(arr);
                for i in 0..num {
                    let tri = arr.at(i * t5sz::XMODEL_COLL_TRI);
                    let vec4 = |off| {
                        let mut v = [0.0; 4];
                        for (j, value) in v.iter_mut().enumerate() {
                            *value = stream.f32_at(tri, off + j * 4).ok()?;
                            if !value.is_finite() {
                                return None;
                            }
                        }
                        Some(v)
                    };
                    tris.push(xmodel_runtime::CollTri {
                        plane: vec4(0)?,
                        svec: vec4(16)?,
                        tvec: vec4(32)?,
                    });
                }
            }
            surfs.push(xmodel_runtime::CollSurfCollision {
                bone,
                contents: stream.u32_at(row, 36).ok()?,
                surf_flags: stream.u32_at(row, 40).ok()?,
                midpoint,
                half_size,
                tris,
            });
        }
    }
    Some((bones, surfs))
}

fn capture_pose_src_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    geometry: &fastfile_t5::XModelGeometry,
    bone_names: &[String],
    bones: &[BoneBind],
    num_child: usize,
) -> Option<ModelPoseSrc> {
    use fastfile_t5::size as t5sz;

    let parent_list = if num_child == 0 {
        Vec::new()
    } else {
        let arr = geometry.parent_list?;
        (0..num_child)
            .map(|i| stream.u8_at(arr, i).ok())
            .collect::<Option<Vec<_>>>()?
    };
    let quats = if num_child == 0 {
        Vec::new()
    } else {
        let arr = geometry.quats?;
        (0..num_child)
            .map(|i| {
                let o = i * t5sz::XMODEL_QUAT;
                Some([
                    stream.i16_at(arr, o).ok()?,
                    stream.i16_at(arr, o + 2).ok()?,
                    stream.i16_at(arr, o + 4).ok()?,
                    stream.i16_at(arr, o + 6).ok()?,
                ])
            })
            .collect::<Option<Vec<_>>>()?
    };
    let trans = if num_child == 0 {
        Vec::new()
    } else {
        let arr = geometry.trans?;
        (0..num_child)
            .map(|i| {
                let o = i * t5sz::XMODEL_TRANS_STRIDE;
                Some([
                    stream.f32_at(arr, o).ok()?,
                    stream.f32_at(arr, o + 4).ok()?,
                    stream.f32_at(arr, o + 8).ok()?,
                ])
            })
            .collect::<Option<Vec<_>>>()?
    };
    let base_mat: Vec<(Quat, Vec3)> = bones
        .iter()
        .map(|b| {
            (
                Quat::from_xyzw(b.quat[0], b.quat[1], b.quat[2], b.quat[3]),
                Vec3::from_array(b.trans),
            )
        })
        .collect();

    Some(ModelPoseSrc {
        name: geometry
            .name
            .and_then(|p| stream.cstr(p).ok())
            .unwrap_or("")
            .to_owned(),
        num_bones: geometry.num_bones,
        num_root_bones: geometry.num_root_bones,
        scale: 1.0,
        no_scale_part_bits: [0; 6],
        bone_names: bone_names.to_vec(),
        parent_list,
        quats,
        trans,
        base_mat,
        root_rest: None,
    })
}

fn decode_surface_skin_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    surface: fastfile_t5::Ptr,
    vertex_count: usize,
    vert_list_count: usize,
    num_bones: usize,
    rigid_stride: usize,
) -> Option<Vec<VertSkin>> {
    let mut skins = vec![VertSkin::default(); vertex_count];
    let bone_at = |raw: u16| -> Option<u16> {
        if raw % BONE_STRIDE != 0 {
            return None;
        }
        let bone = raw / BONE_STRIDE;
        ((bone as usize) < num_bones).then_some(bone)
    };

    if vert_list_count > 0 {
        if let Ok(fastfile_t5::ZonePtr::Offset(list_ptr)) = stream.ptr_at(surface, 40) {
            let list = stream.resolve_alias(list_ptr);
            let mut vertex = 0usize;
            for i in 0..vert_list_count {
                let entry = list.at(i * rigid_stride);
                let bone_offset = stream.u16_at(entry, 0).ok()?;
                let run = stream.u16_at(entry, 2).ok()? as usize;
                let bone = bone_at(bone_offset)?;
                for _ in 0..run {
                    if vertex >= vertex_count {
                        break;
                    }
                    skins[vertex] = VertSkin {
                        bones: [bone, 0, 0, 0],
                        weights: [1.0, 0.0, 0.0, 0.0],
                        ..Default::default()
                    };
                    vertex += 1;
                }
            }
            if vertex == vertex_count {
                return Some(skins);
            }
        }
    }

    let vi = surface.at(16);
    let counts = [
        stream.i16_at(vi, 0).ok()?.max(0) as usize,
        stream.i16_at(vi, 2).ok()?.max(0) as usize,
        stream.i16_at(vi, 4).ok()?.max(0) as usize,
        stream.i16_at(vi, 6).ok()?.max(0) as usize,
    ];
    let blend_ptr = match stream.ptr_at(vi, 8).ok()? {
        fastfile_t5::ZonePtr::Offset(p) => stream.resolve_alias(p),
        fastfile_t5::ZonePtr::Null if counts.iter().all(|&c| c == 0) => {
            for skin in &mut skins {
                *skin = VertSkin {
                    bones: [0, 0, 0, 0],
                    weights: [1.0, 0.0, 0.0, 0.0],
                    ..Default::default()
                };
            }
            return Some(skins);
        }
        _ => return None,
    };

    let mut cursor = 0usize;
    let mut vertex = 0usize;
    let take = |cursor: &mut usize, n: usize| -> Option<()> {
        *cursor = cursor.checked_add(n)?;
        Some(())
    };
    for (bucket, count) in counts.iter().enumerate() {
        let influences = bucket + 1;
        for _ in 0..*count {
            let words = 1 + (influences - 1) * 2;
            let start = cursor;
            take(&mut cursor, words)?;
            let mut skin = VertSkin::default();
            let b0 = stream.u16_at(blend_ptr, start * 2).ok()?;
            skin.bones[0] = bone_at(b0)?;
            let mut remaining = 1.0f32;
            for extra in 1..influences {
                let off = start + 1 + (extra - 1) * 2;
                let b = stream.u16_at(blend_ptr, off * 2).ok()?;
                let raw = stream.u16_at(blend_ptr, off * 2 + 2).ok()?;
                let w = dpvs_iw4::skin_blend_weight(raw);
                skin.bones[extra] = bone_at(b)?;
                skin.weights[extra] = w;
                skin.weight_u16[extra] = raw;
                remaining -= w;
            }
            skin.weights[0] = remaining;
            if vertex >= vertex_count {
                return None;
            }
            skins[vertex] = skin;
            vertex += 1;
        }
    }
    if vertex != vertex_count {
        return None;
    }
    Some(skins)
}

pub fn capture_fpv_skel_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    strings: &fastfile_iw5::ScriptStrings,
    geometry: fastfile_iw5::XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Option<FpvSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    if model_kind(&name) != Some(ModelKind::Fpv) {
        return None;
    }
    capture_model_skel_iw5(stream, strings, geometry, name, materials, None)
}

pub fn capture_world_weapon_skel_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    strings: &fastfile_iw5::ScriptStrings,
    geometry: fastfile_iw5::XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    if model_kind(&name) != Some(ModelKind::WorldWeapon) {
        return None;
    }
    capture_model_skel_iw5(stream, strings, geometry, name, materials, None)
}

pub fn capture_xmodel_skel_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    strings: &fastfile_iw5::ScriptStrings,
    geometry: fastfile_iw5::XModelGeometry,
    materials: &MaterialCatalog,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    capture_model_skel_iw5(stream, strings, geometry, name, Some(materials), None)
}

pub fn capture_xmodel_skel_iw5_with_shared(
    stream: &fastfile_iw5::ZoneStream<'_>,
    strings: &fastfile_iw5::ScriptStrings,
    geometry: fastfile_iw5::XModelGeometry,
    materials: &MaterialCatalog,
    shared: &SharedXModelSurfaces,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    capture_model_skel_iw5(
        stream,
        strings,
        geometry,
        name,
        Some(materials),
        Some(shared),
    )
}

pub fn capture_body_skel_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    strings: &fastfile_iw5::ScriptStrings,
    geometry: fastfile_iw5::XModelGeometry,
    materials: Option<&MaterialCatalog>,
) -> Option<ModelSkel> {
    let name = geometry.name.and_then(|p| stream.cstr(p).ok())?.to_owned();
    if model_kind(&name) != Some(ModelKind::Soldier) {
        return None;
    }
    capture_model_skel_iw5(stream, strings, geometry, name, materials, None)
}

fn capture_model_skel_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    strings: &fastfile_iw5::ScriptStrings,
    geometry: fastfile_iw5::XModelGeometry,
    name: String,
    materials: Option<&MaterialCatalog>,
    shared: Option<&SharedXModelSurfaces>,
) -> Option<ModelSkel> {
    use fastfile_iw5::size as iw5sz;

    let bone_names = geometry.bone_names?;
    let base_mat = geometry.base_mat?;

    let mut bones = Vec::with_capacity(geometry.num_bones);
    let mut bone_name_strs = Vec::with_capacity(geometry.num_bones);
    let mut tag_view = None;
    let mut tag_weapon = None;
    for i in 0..geometry.num_bones {
        let id = stream.u16_at(bone_names, i * 2).ok()?;
        let bone_name = strings.get(stream, id).unwrap_or("").to_owned();
        if bone_name == "tag_view" && tag_view.is_none() {
            tag_view = Some(i);
        }
        if bone_name == "tag_weapon" && tag_weapon.is_none() {
            tag_weapon = Some(i);
        }
        bone_name_strs.push(bone_name);
        let m = base_mat.at(i * iw5sz::DOBJ_ANIM_MAT);
        bones.push(BoneBind {
            quat: [
                stream.f32_at(m, 0).ok()?,
                stream.f32_at(m, 4).ok()?,
                stream.f32_at(m, 8).ok()?,
                stream.f32_at(m, 12).ok()?,
            ],
            trans: [
                stream.f32_at(m, 16).ok()?,
                stream.f32_at(m, 20).ok()?,
                stream.f32_at(m, 24).ok()?,
            ],
        });
    }

    let num_child = geometry.num_bones.saturating_sub(geometry.num_root_bones);
    let pose = capture_pose_src_iw5(stream, &geometry, &bone_name_strs, &bones, num_child);
    let bone_collision = capture_bone_collision_iw5(stream, geometry);
    let material_at = |surface_index: usize| {
        geometry.material_handles.and_then(|handles| {
            materials?.material_index(Ptr {
                block: handles.block,
                offset: handles.offset + (surface_index * stream.pointer_bytes()) as u32,
            })
        })
    };

    let Some(surfaces) = geometry.surfaces else {
        let surfaces_name = stream
            .cstr(geometry.surfaces_name?)
            .ok()?
            .trim_start_matches(',');
        let (source, _) = shared?.0.get(surfaces_name)?;
        if source.surface_vertex_ranges.len() != geometry.surface_count
            || source.vert_skin.iter().any(|skin| {
                skin.bones
                    .iter()
                    .zip(skin.weights)
                    .any(|(bone, weight)| weight != 0.0 && usize::from(*bone) >= geometry.num_bones)
            })
        {
            return None;
        }
        return Some(ModelSkel {
            name,
            bones,
            bone_names: bone_name_strs,
            tag_view,
            tag_weapon,
            pose,
            bone_collision,
            surface_materials: (0..geometry.surface_count).map(material_at).collect(),
            radius: geometry.radius,
            ..(**source).clone()
        });
    };

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut colors = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    let mut packed_vertices = Vec::new();
    let mut surface_materials = Vec::with_capacity(geometry.surface_count);
    let mut surface_vertex_ranges = Vec::with_capacity(geometry.surface_count);
    let mut surface_index_ranges = Vec::with_capacity(geometry.surface_count);
    let mut surface_part_bits = Vec::with_capacity(geometry.surface_count);
    let mut surface_deformed = Vec::with_capacity(geometry.surface_count);
    let mut surface_vert_list_count = Vec::with_capacity(geometry.surface_count);
    let mut vert_skin = Vec::new();
    let mut rigid_verts = 0usize;
    let mut blend_verts = 0usize;

    for surface_index in 0..geometry.surface_count {
        let surface = surfaces.at(surface_index * stream.layout(iw5sz::XSURFACE, 88));
        let vertex_count = stream.u16_at(surface, 2).ok()? as usize;
        let tri_count = stream.u16_at(surface, 4).ok()? as usize;
        let vert_list_count = stream
            .u32_at(
                surface,
                stream.layout(iw5sz::XSURFACE_VERT_LIST_COUNT_OFF, 48),
            )
            .ok()? as usize;
        let flags = stream.u8_at(surface, iw5sz::XSURFACE_FLAGS_OFF).ok()?;
        surface_deformed.push(Some(flags & iw5sz::XSURFACE_FLAG_DEFORMED != 0));
        surface_vert_list_count.push(Some(u32::try_from(vert_list_count).ok()?));

        let part_bits_off = stream.layout(44, 64);
        let mut part_bits = [0u32; 6];
        for (i, word) in part_bits.iter_mut().enumerate() {
            *word = stream.u32_at(surface, part_bits_off + i * 4).ok()?;
        }
        surface_part_bits.push(part_bits);
        let vertices = match stream
            .ptr_at(surface, stream.layout(iw5sz::XSURFACE_VERTS0_OFF, 40))
            .ok()?
        {
            fastfile_iw5::ZonePtr::Offset(p) => stream.resolve_alias(p),
            _ => return None,
        };
        let triangles = match stream
            .ptr_at(surface, iw5sz::XSURFACE_TRI_INDICES_OFF)
            .ok()?
        {
            fastfile_iw5::ZonePtr::Offset(p) => stream.resolve_alias(p),
            _ => return None,
        };

        let base = positions.len();
        let index_start = indices.len();
        let surface_skins = decode_surface_skin_iw5(
            stream,
            surface,
            vertex_count,
            vert_list_count,
            geometry.num_bones,
        )?;

        for vertex_index in 0..vertex_count {
            let vertex = vertices.at(vertex_index * iw5sz::GFX_PACKED_VERTEX);
            positions.push([
                stream.f32_at(vertex, 0).ok()?,
                stream.f32_at(vertex, 4).ok()?,
                stream.f32_at(vertex, 8).ok()?,
            ]);
            packed_vertices.push(copy_packed_vertex_iw5(stream, vertex)?);
            normals.push(normalize_or_up(unpack_unit_vec(
                stream.u32_at(vertex, 24).ok()?,
            )));
            colors.push(unpack_color(stream.u32_at(vertex, 16).ok()?));
            uvs.push(unpack_packed_tex_coords(stream.u32_at(vertex, 20).ok()?));
            let skin = surface_skins[vertex_index];
            if skin.weights[1] == 0.0 && skin.weights[0] == 1.0 {
                rigid_verts += 1;
            } else {
                blend_verts += 1;
            }
            vert_skin.push(skin);
        }
        for index_offset in 0..tri_count * 3 {
            let index = stream.u16_at(triangles, index_offset * 2).ok()?;
            if usize::from(index) >= vertex_count {
                return None;
            }
            indices.push(base as u32 + u32::from(index));
        }
        surface_vertex_ranges.push((base, vertex_count));
        surface_index_ranges.push((index_start, indices.len() - index_start));
        surface_materials.push(material_at(surface_index));
    }

    Some(ModelSkel {
        name,
        bones,
        bone_collision,
        bone_names: bone_name_strs,
        tag_view,
        tag_weapon,
        pose,
        positions,
        normals,
        colors,
        uvs,
        indices,
        surface_materials,
        surface_vertex_ranges,
        surface_index_ranges,
        surface_part_bits,
        surface_deformed,
        surface_vert_list_count,
        vert_skin,
        rigid_verts,
        blend_verts,
        packed_vertices,
        radius: geometry.radius,
        bounds: None,
        contents: None,
        coll_lod: 0,
        coll_surfs: Vec::new(),
        movement_brushes: Vec::new(),

        mount_tag: None,

        lod: None,
        lod_smc: None,
        lod_part_bits: None,
        lod_surf_span: [(0, 0); 4],
    })
}

fn capture_bone_collision_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    geometry: fastfile_iw5::XModelGeometry,
) -> Vec<Option<BoneCollision>> {
    let (Some(info), Some(classes)) = (geometry.bone_info, geometry.part_classification) else {
        return Vec::new();
    };
    (0..geometry.num_bones)
        .map(|bone| {
            let row = info.at(bone * fastfile_iw5::size::XBONE_INFO);
            let radius_sq = stream.f32_at(row, 24).ok()?;
            let midpoint = [
                stream.f32_at(row, 0).ok()?,
                stream.f32_at(row, 4).ok()?,
                stream.f32_at(row, 8).ok()?,
            ];
            let half_size = [
                stream.f32_at(row, 12).ok()?,
                stream.f32_at(row, 16).ok()?,
                stream.f32_at(row, 20).ok()?,
            ];
            let finite = radius_sq.is_finite()
                && radius_sq > 0.0
                && midpoint.iter().all(|value| value.is_finite())
                && half_size
                    .iter()
                    .all(|value| value.is_finite() && *value >= 0.0);
            let part_classification = stream.u8_at(classes, bone).ok()?;
            finite.then_some(BoneCollision {
                midpoint,
                half_size,
                radius_sq,
                part_classification,
            })
        })
        .collect()
}

fn capture_pose_src_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    geometry: &fastfile_iw5::XModelGeometry,
    bone_names: &[String],
    bones: &[BoneBind],
    num_child: usize,
) -> Option<ModelPoseSrc> {
    use fastfile_iw5::size as iw5sz;

    let parent_list = if num_child == 0 {
        Vec::new()
    } else {
        let arr = geometry.parent_list?;
        (0..num_child)
            .map(|i| stream.u8_at(arr, i).ok())
            .collect::<Option<Vec<_>>>()?
    };
    let quats = if num_child == 0 {
        Vec::new()
    } else {
        let arr = geometry.quats?;
        (0..num_child)
            .map(|i| {
                let o = i * iw5sz::XMODEL_QUAT;
                Some([
                    stream.i16_at(arr, o).ok()?,
                    stream.i16_at(arr, o + 2).ok()?,
                    stream.i16_at(arr, o + 4).ok()?,
                    stream.i16_at(arr, o + 6).ok()?,
                ])
            })
            .collect::<Option<Vec<_>>>()?
    };
    let trans = if num_child == 0 {
        Vec::new()
    } else {
        let arr = geometry.trans?;
        (0..num_child)
            .map(|i| {
                let o = i * 12;
                Some([
                    stream.f32_at(arr, o).ok()?,
                    stream.f32_at(arr, o + 4).ok()?,
                    stream.f32_at(arr, o + 8).ok()?,
                ])
            })
            .collect::<Option<Vec<_>>>()?
    };
    let base_mat = bones
        .iter()
        .map(|b| {
            (
                Quat::from_xyzw(b.quat[0], b.quat[1], b.quat[2], b.quat[3]),
                Vec3::from_array(b.trans),
            )
        })
        .collect();
    Some(ModelPoseSrc {
        name: geometry
            .name
            .and_then(|p| stream.cstr(p).ok())
            .unwrap_or("")
            .to_owned(),
        num_bones: geometry.num_bones,
        num_root_bones: geometry.num_root_bones,
        scale: geometry.scale,
        no_scale_part_bits: geometry.no_scale_part_bits,
        bone_names: bone_names.to_vec(),
        parent_list,
        quats,
        trans,
        base_mat,
        root_rest: None,
    })
}

fn decode_surface_skin_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    surface: fastfile_iw5::Ptr,
    vertex_count: usize,
    vert_list_count: usize,
    num_bones: usize,
) -> Option<Vec<VertSkin>> {
    use fastfile_iw5::size as iw5sz;

    let mut skins = vec![VertSkin::default(); vertex_count];
    let bone_at = |raw: u16| -> Option<u16> {
        if raw % BONE_STRIDE != 0 {
            return None;
        }
        let bone = raw / BONE_STRIDE;
        ((bone as usize) < num_bones).then_some(bone)
    };

    let mut rigid_assigned = 0usize;
    if vert_list_count > 0 {
        if let Ok(fastfile_iw5::ZonePtr::Offset(list_ptr)) =
            stream.ptr_at(surface, stream.layout(iw5sz::XSURFACE_VERT_LIST_OFF, 56))
        {
            let list = stream.resolve_alias(list_ptr);
            let mut vertex = 0usize;
            for i in 0..vert_list_count {
                let entry = list.at(i * stream.layout(iw5sz::XRIGID_VERT_LIST, 16));
                let bone_offset = stream.u16_at(entry, 0).ok()?;
                let run = stream.u16_at(entry, 2).ok()? as usize;
                let bone = bone_at(bone_offset)?;
                for _ in 0..run {
                    if vertex >= vertex_count {
                        break;
                    }
                    skins[vertex] = VertSkin {
                        bones: [bone, 0, 0, 0],
                        weights: [1.0, 0.0, 0.0, 0.0],
                        ..Default::default()
                    };
                    vertex += 1;
                }
            }
            rigid_assigned = vertex;
            if vertex == vertex_count {
                return Some(skins);
            }
        }
    }

    let vi = surface.at(stream.layout(iw5sz::XSURFACE_VERT_INFO_OFF, 24));
    let counts = [
        stream.i16_at(vi, 0).ok()?.max(0) as usize,
        stream.i16_at(vi, 2).ok()?.max(0) as usize,
        stream.i16_at(vi, 4).ok()?.max(0) as usize,
        stream.i16_at(vi, 6).ok()?.max(0) as usize,
    ];
    let blend_ptr = match stream.ptr_at(vi, 8).ok()? {
        fastfile_iw5::ZonePtr::Offset(p) => stream.resolve_alias(p),
        fastfile_iw5::ZonePtr::Null if counts.iter().all(|&c| c == 0) => {
            if rigid_assigned > 0 {
                return Some(skins);
            }
            if num_bones > 1 {
                diag::warn!(
                    Zone,
                    "iw5 xsurface skin: {vertex_count} verts, vertListCount={vert_list_count}, \
                     no blend buckets, {num_bones} bones — cannot resolve ownership (typed gap)"
                );
            }
            for skin in &mut skins {
                *skin = VertSkin {
                    bones: [0, 0, 0, 0],
                    weights: [1.0, 0.0, 0.0, 0.0],
                    ..Default::default()
                };
            }
            return Some(skins);
        }
        _ => return None,
    };

    let mut cursor = 0usize;
    let mut vertex = rigid_assigned;
    let take = |cursor: &mut usize, n: usize| -> Option<()> {
        *cursor = cursor.checked_add(n)?;
        Some(())
    };
    for (bucket, count) in counts.iter().enumerate() {
        let influences = bucket + 1;
        for _ in 0..*count {
            let words = 1 + (influences - 1) * 2;
            let start = cursor;
            take(&mut cursor, words)?;
            let mut skin = VertSkin::default();
            let b0 = stream.u16_at(blend_ptr, start * 2).ok()?;
            skin.bones[0] = bone_at(b0)?;
            let mut remaining = 1.0f32;
            for extra in 1..influences {
                let off = start + 1 + (extra - 1) * 2;
                let b = stream.u16_at(blend_ptr, off * 2).ok()?;
                let raw = stream.u16_at(blend_ptr, off * 2 + 2).ok()?;
                let w = dpvs_iw4::skin_blend_weight(raw);
                skin.bones[extra] = bone_at(b)?;
                skin.weights[extra] = w;
                skin.weight_u16[extra] = raw;
                remaining -= w;
            }
            skin.weights[0] = remaining;
            if vertex >= vertex_count {
                return None;
            }
            skins[vertex] = skin;
            vertex += 1;
        }
    }
    if vertex != vertex_count {
        return None;
    }
    Some(skins)
}
