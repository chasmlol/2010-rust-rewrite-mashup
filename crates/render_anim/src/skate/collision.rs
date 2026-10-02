use bevy::prelude::*;

pub fn to_skate(p: Vec3) -> Vec3 {
    Vec3::new(p.x, p.z, -p.y) * 0.0254
}
pub fn from_skate(p: Vec3) -> Vec3 {
    Vec3::new(p.x, -p.z, p.y) / 0.0254
}
pub fn basis() -> Mat4 {
    Mat4::from_cols(Vec4::X, Vec4::Z, -Vec4::Y, Vec4::W)
}

pub struct World {
    pub triangles: Vec<[[f32; 3]; 3]>,
    /// Grind rails, each a polyline of two or more points.
    pub rails: Vec<Vec<[f32; 3]>>,
}

/// Use collision, not visible triangles: invisible player clips and solid props
/// must remain solid when the local character changes movement controller.
pub fn extract(clip: &asset_world::ClipCollision) -> World {
    let out = crate::clip_triangles::map_triangles(clip);
    let (found, census) = super::rails::find(&out);
    diag::info!(
        World,
        "skate rails: {} walkable edges, {} lips, {} runs, {} rails",
        census.candidates,
        census.lips,
        census.runs,
        census.rails,
    );
    let rails = found
        .into_iter()
        .map(|rail| rail.into_iter().map(|p| to_skate(p).to_array()).collect())
        .collect();
    World {
        triangles: out
            .into_iter()
            .map(|p| p.map(|v| to_skate(v).to_array()))
            .collect(),
        rails,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cube_collision_has_outward_faces_and_roundtrips_units() {
        let faces = crate::clip_triangles::brush_faces(&[
            [1., 0., 0., 16.],
            [-1., 0., 0., 16.],
            [0., 1., 0., 16.],
            [0., -1., 0., 16.],
            [0., 0., 1., 16.],
            [0., 0., -1., 16.],
        ]);
        assert_eq!(faces.len(), 6);
        for f in faces {
            assert_eq!(f.len(), 4);
            assert!((f[1] - f[0]).cross(f[2] - f[0]).dot(f[0]) > 0.);
        }
        let p = Vec3::new(1234., -456., 72.);
        assert!(from_skate(to_skate(p)).distance(p) < 0.001);
    }
}
