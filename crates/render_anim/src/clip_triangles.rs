//! The solid triangles of a map's collision, for the movement modes that run
//! their own collision against it.
use bevy::prelude::*;
use std::collections::HashSet;

/// Use collision, not visible triangles: invisible player clips and solid props
/// must remain solid when the local character changes movement controller.
pub fn map_triangles(clip: &asset_world::ClipCollision) -> Vec<[Vec3; 3]> {
    let mut out = Vec::<[Vec3; 3]>::new();
    let mut push = |p: [Vec3; 3]| {
        if p.iter().all(|v| v.is_finite())
            && (p[1] - p[0]).cross(p[2] - p[0]).length_squared() > 0.001
        {
            out.push(p);
        }
    };
    let mesh = &clip.mesh;
    let mut visited = HashSet::new();
    for part in &mesh.partitions {
        for tri in
            part.first_tri.max(0) as usize..part.first_tri.max(0) as usize + part.tri_count as usize
        {
            if !visited.insert(tri)
                || mesh.tri_content_flags.get(tri).copied().unwrap_or(1) & 0x10001 == 0
            {
                continue;
            }
            let Some(indices) = mesh.tri_indices.get(tri * 3..tri * 3 + 3) else {
                continue;
            };
            let base = part.first_vert_segment as usize * 1024;
            if let (Some(a), Some(b), Some(c)) = (
                mesh.verts.get(base + indices[0] as usize),
                mesh.verts.get(base + indices[1] as usize),
                mesh.verts.get(base + indices[2] as usize),
            ) {
                // IW4 traces clockwise faces; Skate uses counterclockwise faces.
                push([
                    Vec3::from_array(*a),
                    Vec3::from_array(*c),
                    Vec3::from_array(*b),
                ]);
            }
        }
    }
    for brush in &clip.brushes {
        if brush.contents & 0x10001 == 0 {
            continue;
        }
        for face in brush_faces(&brush.planes) {
            for i in 1..face.len() - 1 {
                push([face[0], face[i], face[i + 1]]);
            }
        }
    }
    for placed in &clip.static_models {
        let model = &placed.model;
        let inverse = Mat3::from_cols_array_2d(&model.inv_scaled_axis);
        if inverse.determinant().abs() < 1e-10 {
            continue;
        }
        let axis = inverse.inverse();
        for surf in &model.coll.surfs {
            if surf.contents & 0x10001 == 0 {
                continue;
            }
            for t in &surf.tris {
                let n = Vec3::from_slice(&t.plane[..3]);
                let m = Mat3::from_cols(
                    n,
                    Vec3::from_slice(&t.svec[..3]),
                    Vec3::from_slice(&t.tvec[..3]),
                )
                .transpose();
                if m.determinant().abs() < 1e-12 {
                    continue;
                }
                let inv = m.inverse();
                let p = |s, tv| {
                    axis * (inv * Vec3::new(t.plane[3], t.svec[3] + s, t.tvec[3] + tv))
                        + Vec3::from_array(model.origin)
                };
                let mut pts = [p(0., 0.), p(1., 0.), p(0., 1.)];
                if (pts[1] - pts[0]).cross(pts[2] - pts[0]).dot(axis * n) < 0. {
                    pts.swap(1, 2);
                }
                push(pts);
            }
        }
    }
    let mut seen = HashSet::new();
    out.retain(|p| {
        let mut k = p.map(|v| v.to_array().map(|x| (x * 8.).round() as i32));
        k.sort();
        seen.insert(k)
    });
    out
}

pub(crate) fn brush_faces(planes: &[[f32; 4]]) -> Vec<Vec<Vec3>> {
    let mut points = Vec::<Vec3>::new();
    for a in 0..planes.len() {
        for b in a + 1..planes.len() {
            for c in b + 1..planes.len() {
                let m = Mat3::from_cols(
                    Vec3::from_slice(&planes[a][..3]),
                    Vec3::from_slice(&planes[b][..3]),
                    Vec3::from_slice(&planes[c][..3]),
                )
                .transpose();
                if m.determinant().abs() < 1e-6 {
                    continue;
                }
                let p = m.inverse() * Vec3::new(planes[a][3], planes[b][3], planes[c][3]);
                if p.is_finite()
                    && planes
                        .iter()
                        .all(|v| Vec3::from_slice(&v[..3]).dot(p) <= v[3] + 0.05)
                    && points.iter().all(|v| v.distance_squared(p) > 0.01)
                {
                    points.push(p);
                }
            }
        }
    }
    planes
        .iter()
        .filter_map(|plane| {
            let n = Vec3::from_slice(&plane[..3]);
            let mut face: Vec<_> = points
                .iter()
                .copied()
                .filter(|p| (n.dot(*p) - plane[3]).abs() < 0.1)
                .collect();
            if face.len() < 3 {
                return None;
            }
            let center = face.iter().copied().sum::<Vec3>() / face.len() as f32;
            let x = n.normalize().any_orthonormal_vector();
            let y = n.cross(x).normalize();
            face.sort_by(|a, b| {
                let a = *a - center;
                let b = *b - center;
                a.dot(y)
                    .atan2(a.dot(x))
                    .total_cmp(&b.dot(y).atan2(b.dot(x)))
            });
            Some(face)
        })
        .collect()
}
