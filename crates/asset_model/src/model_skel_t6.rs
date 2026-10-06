use bevy::prelude::{Quat, Vec3};
use fastfile_t6::{Ptr, ZoneLoad};

use crate::dobj::ModelPoseSrc;
use crate::model_skel::{BoneBind, ModelSkel, VertSkin};
use crate::packed_vertex::{
    normalize_or_up, repack_vertex_t6, unpack_color, unpack_packed_tex_coords, unpack_unit_vec,
};
use asset_core::WalkLocalMaterialIndex;
pub use xmodel_runtime::BoneCollision;

mod off {
    pub const NAME: u32 = 0;
    pub const NUM_BONES: u32 = 4;
    pub const NUM_ROOT_BONES: u32 = 5;
    pub const NUM_SURFS: u32 = 6;
    pub const BONE_NAMES: u32 = 8;
    pub const PARENT_LIST: u32 = 12;
    pub const QUATS: u32 = 16;
    pub const TRANS: u32 = 20;
    pub const PART_CLASSIFICATION: u32 = 24;
    pub const BASE_MAT: u32 = 28;
    pub const SURFS: u32 = 32;
    pub const MATERIAL_HANDLES: u32 = 36;
    pub const LOD_INFO: u32 = 40;
    pub const LOD_INFO_STRIDE: u32 = 28;
    pub const COLL_SURFS: u32 = 152;
    pub const NUM_COLL_SURFS: u32 = 156;
    pub const CONTENTS: u32 = 160;
    pub const BONE_INFO: u32 = 164;
    pub const RADIUS: u32 = 168;
    pub const NUM_LODS: u32 = 196;
    pub const COLL_LOD: u32 = 198;
    pub const SIZE: usize = 248;
}

const XSURFACE: u32 = 80;
const XSURFACE_PART_BITS: u32 = 48;
const XSURFACE_PART_BITS_WORDS: u32 = 5;
const DOBJ_ANIM_MAT: u32 = 32;
const GFX_PACKED_VERTEX: u32 = 32;
const XRIGID_VERT_LIST: u32 = 12;
const XMODEL_QUAT: u32 = 8;
const XBONE_INFO: u32 = 44;
const XMODEL_COLL_SURF: u32 = 44;
const XMODEL_COLL_TRI: u32 = 48;
const BONE_STRIDE: u16 = 64;

struct Reader<'z> {
    load: &'z ZoneLoad,
}

impl<'z> Reader<'z> {
    fn bytes(&self, p: Ptr, len: usize) -> Option<&'z [u8]> {
        self.load.blocks.bytes(p, len).ok()
    }
    fn u8(&self, p: Ptr) -> Option<u8> {
        Some(self.bytes(p, 1)?[0])
    }
    fn u16(&self, p: Ptr) -> Option<u16> {
        Some(u16::from_le_bytes(self.bytes(p, 2)?.try_into().ok()?))
    }
    fn i16(&self, p: Ptr) -> Option<i16> {
        Some(i16::from_le_bytes(self.bytes(p, 2)?.try_into().ok()?))
    }
    fn u32(&self, p: Ptr) -> Option<u32> {
        Some(u32::from_le_bytes(self.bytes(p, 4)?.try_into().ok()?))
    }
    fn i32(&self, p: Ptr) -> Option<i32> {
        Some(self.u32(p)? as i32)
    }
    fn f32(&self, p: Ptr) -> Option<f32> {
        Some(f32::from_bits(self.u32(p)?))
    }
    fn ptr(&self, p: Ptr) -> Option<Option<Ptr>> {
        self.load.blocks.ptr_at(p).ok()
    }
}

#[derive(Clone, Copy)]
pub struct T6Model<'z> {
    load: &'z ZoneLoad,
    header: &'z [u8],
}

impl<'z> T6Model<'z> {
    pub fn new(load: &'z ZoneLoad, asset: &'z fastfile_t6::LoadedAsset) -> Option<Self> {
        (asset.ty == fastfile_t6::AssetType::XModel && asset.header.len() >= off::SIZE).then_some(
            Self {
                load,
                header: &asset.header,
            },
        )
    }

    fn h_u8(&self, o: u32) -> u8 {
        self.header[o as usize]
    }
    fn h_u16(&self, o: u32) -> u16 {
        u16::from_le_bytes(self.header[o as usize..o as usize + 2].try_into().unwrap())
    }
    fn h_u32(&self, o: u32) -> u32 {
        u32::from_le_bytes(self.header[o as usize..o as usize + 4].try_into().unwrap())
    }
    fn h_f32(&self, o: u32) -> f32 {
        f32::from_bits(self.h_u32(o))
    }
    fn h_ptr(&self, o: u32) -> Option<Ptr> {
        let raw = self.h_u32(o);
        if raw == 0 || raw >= 0xFFFF_FFFE {
            return None;
        }
        let e = raw - 1;
        Some(Ptr {
            block: (e >> 29) as u8,
            offset: e & 0x1FFF_FFFF,
        })
    }

    pub fn name(&self) -> Option<&'z str> {
        let p = self.h_ptr(off::NAME)?;
        core::str::from_utf8(self.load.blocks.cstr(p).ok()?).ok()
    }

    pub fn surface_count(&self) -> usize {
        usize::from(self.h_u8(off::NUM_SURFS))
    }

    pub fn material_slot(&self, index: usize) -> Option<Ptr> {
        Some(self.h_ptr(off::MATERIAL_HANDLES)?.at(4 * index as u32))
    }
}

pub fn capture_model_skel_t6(
    model: T6Model<'_>,
    mut material_of: impl FnMut(usize) -> Option<WalkLocalMaterialIndex>,
) -> Option<ModelSkel> {
    let r = Reader { load: model.load };
    let name = model.name()?.to_owned();
    let num_bones = usize::from(model.h_u8(off::NUM_BONES));
    let num_root_bones = usize::from(model.h_u8(off::NUM_ROOT_BONES));
    let surface_count = model.surface_count();
    let surfaces = model.h_ptr(off::SURFS)?;
    let bone_names = model.h_ptr(off::BONE_NAMES)?;
    let base_mat = model.h_ptr(off::BASE_MAT)?;

    let mut bones = Vec::with_capacity(num_bones);
    let mut bone_name_strs = Vec::with_capacity(num_bones);
    let mut tag_view = None;
    let mut tag_weapon = None;
    for i in 0..num_bones {
        let id = r.u16(bone_names.at(2 * i as u32))?;
        let bone_name = model.load.script_string(id).unwrap_or("").to_owned();
        if bone_name == "tag_view" && tag_view.is_none() {
            tag_view = Some(i);
        }
        if bone_name == "tag_weapon" && tag_weapon.is_none() {
            tag_weapon = Some(i);
        }
        bone_name_strs.push(bone_name);
        let m = base_mat.at(i as u32 * DOBJ_ANIM_MAT);
        bones.push(BoneBind {
            quat: [
                r.f32(m)?,
                r.f32(m.at(4))?,
                r.f32(m.at(8))?,
                r.f32(m.at(12))?,
            ],
            trans: [r.f32(m.at(16))?, r.f32(m.at(20))?, r.f32(m.at(24))?],
        });
    }

    let num_child = num_bones.saturating_sub(num_root_bones);
    let pose = capture_pose_src(&r, &model, &name, &bone_name_strs, &bones, num_child);

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut colors = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    let mut surface_materials = Vec::with_capacity(surface_count);
    let mut surface_vertex_ranges = Vec::with_capacity(surface_count);
    let mut surface_index_ranges = Vec::with_capacity(surface_count);
    let mut vert_skin = Vec::new();
    let mut rigid_verts = 0usize;
    let mut blend_verts = 0usize;
    let mut packed_vertices = Vec::new();
    let mut surface_part_bits = Vec::with_capacity(surface_count);
    let mut surface_deformed = Vec::with_capacity(surface_count);
    let mut surface_vert_list_count = Vec::with_capacity(surface_count);

    for surface_index in 0..surface_count {
        let surface = surfaces.at(surface_index as u32 * XSURFACE);
        let vert_list_count = usize::from(r.u8(surface.at(1))?);
        surface_deformed.push(Some(r.u16(surface.at(2))? & 0x80 != 0));
        surface_vert_list_count.push(Some(vert_list_count as u32));
        let vertex_count = usize::from(r.u16(surface.at(4))?);
        let tri_count = usize::from(r.u16(surface.at(6))?);
        let vertices = r.ptr(surface.at(32))??;
        let triangles = r.ptr(surface.at(12))??;
        let mut part_bits = [0u32; 6];
        for (i, bits) in part_bits
            .iter_mut()
            .enumerate()
            .take(XSURFACE_PART_BITS_WORDS as usize)
        {
            *bits = r.u32(surface.at(XSURFACE_PART_BITS + 4 * i as u32))?;
        }

        let base = positions.len();
        let index_start = indices.len();
        let skins = decode_surface_skin(&r, surface, vertex_count, vert_list_count, num_bones)?;
        for (vertex_index, skin) in skins.into_iter().enumerate() {
            let vertex = vertices.at(vertex_index as u32 * GFX_PACKED_VERTEX);
            let packed = repack_vertex_t6(
                r.bytes(vertex, GFX_PACKED_VERTEX as usize)?
                    .try_into()
                    .ok()?,
            );
            let word = |o: usize| u32::from_le_bytes(packed[o..o + 4].try_into().unwrap());
            packed_vertices.push(packed);
            positions.push([r.f32(vertex)?, r.f32(vertex.at(4))?, r.f32(vertex.at(8))?]);
            normals.push(normalize_or_up(unpack_unit_vec(word(24))));
            colors.push(unpack_color(word(16)));
            uvs.push(unpack_packed_tex_coords(word(20)));
            if skin.weights[1] == 0.0 && skin.weights[0] == 1.0 {
                rigid_verts += 1;
            } else {
                blend_verts += 1;
            }
            vert_skin.push(skin);
        }
        for index_offset in 0..tri_count * 3 {
            let index = r.u16(triangles.at(2 * index_offset as u32))?;
            if usize::from(index) >= vertex_count {
                return None;
            }
            indices.push(base as u32 + u32::from(index));
        }
        surface_vertex_ranges.push((base, vertex_count));
        surface_index_ranges.push((index_start, indices.len() - index_start));
        surface_part_bits.push(part_bits);
        surface_materials.push(material_of(surface_index));
    }

    let mut lod_surf_span = [(0u16, 0u16); 4];
    let mut lod_dist = [0.0f32; 4];
    for lod in 0..4u32 {
        let record = off::LOD_INFO + lod * off::LOD_INFO_STRIDE;
        lod_surf_span[lod as usize] = (model.h_u16(record + 6), model.h_u16(record + 4));
        lod_dist[lod as usize] = model.h_f32(record);
    }

    let (bone_collision, coll_surfs) = capture_collision(&r, &model, num_bones)?;
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
        radius: Some(model.h_f32(off::RADIUS)),
        bounds: None,
        contents: Some(model.h_u32(off::CONTENTS)),
        coll_lod: model.h_u16(off::COLL_LOD) as i16,
        coll_surfs,
        movement_brushes: Vec::new(),
        mount_tag: None,
        lod: Some(crate::ModelLodSelector::T5 {
            num_lods: model.h_u16(off::NUM_LODS) as i16,
            lod_dist,
        }),
        lod_smc: None,
        lod_part_bits: None,
        lod_surf_span,
    })
}

fn capture_pose_src(
    r: &Reader<'_>,
    model: &T6Model<'_>,
    name: &str,
    bone_names: &[String],
    bones: &[BoneBind],
    num_child: usize,
) -> Option<ModelPoseSrc> {
    let (parent_list, quats, trans) = if num_child == 0 {
        (Vec::new(), Vec::new(), Vec::new())
    } else {
        let parents = model.h_ptr(off::PARENT_LIST)?;
        let quats = model.h_ptr(off::QUATS)?;
        let trans = model.h_ptr(off::TRANS)?;
        (
            (0..num_child as u32)
                .map(|i| r.u8(parents.at(i)))
                .collect::<Option<Vec<_>>>()?,
            (0..num_child as u32)
                .map(|i| {
                    let q = quats.at(i * XMODEL_QUAT);
                    Some([r.i16(q)?, r.i16(q.at(2))?, r.i16(q.at(4))?, r.i16(q.at(6))?])
                })
                .collect::<Option<Vec<_>>>()?,
            (0..num_child as u32)
                .map(|i| {
                    let t = trans.at(i * 12);
                    Some([r.f32(t)?, r.f32(t.at(4))?, r.f32(t.at(8))?])
                })
                .collect::<Option<Vec<_>>>()?,
        )
    };
    Some(ModelPoseSrc {
        name: name.to_owned(),
        num_bones: bones.len(),
        num_root_bones: bones.len() - num_child,
        scale: 1.0,
        no_scale_part_bits: [0; 6],
        bone_names: bone_names.to_vec(),
        parent_list,
        quats,
        trans,
        base_mat: bones
            .iter()
            .map(|b| {
                (
                    Quat::from_xyzw(b.quat[0], b.quat[1], b.quat[2], b.quat[3]),
                    Vec3::from_array(b.trans),
                )
            })
            .collect(),
        root_rest: None,
    })
}

fn decode_surface_skin(
    r: &Reader<'_>,
    surface: Ptr,
    vertex_count: usize,
    vert_list_count: usize,
    num_bones: usize,
) -> Option<Vec<VertSkin>> {
    let mut skins = vec![VertSkin::default(); vertex_count];
    let bone_at = |raw: u16| -> Option<u16> {
        if !raw.is_multiple_of(BONE_STRIDE) {
            return None;
        }
        let bone = raw / BONE_STRIDE;
        (usize::from(bone) < num_bones).then_some(bone)
    };
    let rigid = |bone| VertSkin {
        bones: [bone, 0, 0, 0],
        weights: [1.0, 0.0, 0.0, 0.0],
        ..Default::default()
    };

    if vert_list_count > 0
        && let Some(Some(list)) = r.ptr(surface.at(40))
    {
        let mut vertex = 0usize;
        for i in 0..vert_list_count as u32 {
            let entry = list.at(i * XRIGID_VERT_LIST);
            let bone = bone_at(r.u16(entry)?)?;
            let run = usize::from(r.u16(entry.at(2))?);
            for _ in 0..run {
                if vertex >= vertex_count {
                    break;
                }
                skins[vertex] = rigid(bone);
                vertex += 1;
            }
        }
        if vertex == vertex_count {
            return Some(skins);
        }
    }

    let vi = surface.at(16);
    let counts = [
        r.i16(vi)?.max(0) as usize,
        r.i16(vi.at(2))?.max(0) as usize,
        r.i16(vi.at(4))?.max(0) as usize,
        r.i16(vi.at(6))?.max(0) as usize,
    ];
    let Some(blend) = r.ptr(vi.at(8))? else {
        if counts.iter().all(|&c| c == 0) {
            skins.fill(rigid(0));
            return Some(skins);
        }
        return None;
    };

    const WEIGHT_SCALE: f32 = 1.0 / 65535.0;
    let mut cursor = 0u32;
    let mut vertex = 0usize;
    for (bucket, &count) in counts.iter().enumerate() {
        let influences = bucket + 1;
        for _ in 0..count {
            let words = 1 + (influences as u32 - 1) * 2;
            let start = cursor;
            cursor += words;
            let mut skin = VertSkin::default();
            skin.bones[0] = bone_at(r.u16(blend.at(start * 2))?)?;
            let mut remaining = 1.0f32;
            for extra in 1..influences {
                let o = start + 1 + (extra as u32 - 1) * 2;
                skin.bones[extra] = bone_at(r.u16(blend.at(o * 2))?)?;
                let w = f32::from(r.u16(blend.at(o * 2 + 2))?) * WEIGHT_SCALE;
                skin.weights[extra] = w;
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
    (vertex == vertex_count).then_some(skins)
}

#[allow(clippy::type_complexity)]
fn capture_collision(
    r: &Reader<'_>,
    model: &T6Model<'_>,
    num_bones: usize,
) -> Option<(
    Vec<Option<BoneCollision>>,
    Vec<xmodel_runtime::CollSurfCollision>,
)> {
    let bounds = |row: Ptr, o: u32| {
        let mut midpoint = [0.0; 3];
        let mut half_size = [0.0; 3];
        for axis in 0..3u32 {
            let min = r.f32(row.at(o + axis * 4))?;
            let max = r.f32(row.at(o + 12 + axis * 4))?;
            if !min.is_finite() || !max.is_finite() || min > max {
                return None;
            }
            midpoint[axis as usize] = (min + max) * 0.5;
            half_size[axis as usize] = (max - min) * 0.5;
        }
        Some((midpoint, half_size))
    };
    let mut bones = Vec::new();
    if let Some(info) = model.h_ptr(off::BONE_INFO) {
        let classes = model.h_ptr(off::PART_CLASSIFICATION)?;
        for bone in 0..num_bones as u32 {
            let row = info.at(bone * XBONE_INFO);
            let radius_sq = r.f32(row.at(36))?;
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
                part_classification: r.u8(classes.at(bone))?,
            }));
        }
    }
    let num_coll_surfs = usize::try_from(model.h_u32(off::NUM_COLL_SURFS) as i32).ok()?;
    let mut surfs = Vec::with_capacity(num_coll_surfs);
    if num_coll_surfs != 0 {
        let arr = model.h_ptr(off::COLL_SURFS)?;
        for i in 0..num_coll_surfs as u32 {
            let row = arr.at(i * XMODEL_COLL_SURF);
            let (midpoint, half_size) = bounds(row, 8)?;
            let bone = u16::try_from(r.i32(row.at(32))?).ok()?;
            if usize::from(bone) >= num_bones {
                return None;
            }
            let num = usize::try_from(r.i32(row.at(4))?).ok()?;
            let mut tris = Vec::with_capacity(num);
            if num != 0 {
                let tri_arr = r.ptr(row)??;
                for t in 0..num as u32 {
                    let tri = tri_arr.at(t * XMODEL_COLL_TRI);
                    let vec4 = |o: u32| {
                        let mut v = [0.0; 4];
                        for (j, value) in v.iter_mut().enumerate() {
                            *value = r.f32(tri.at(o + 4 * j as u32))?;
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
                contents: r.u32(row.at(36))?,
                surf_flags: r.u32(row.at(40))?,
                midpoint,
                half_size,
                tris,
            });
        }
    }
    Some((bones, surfs))
}
