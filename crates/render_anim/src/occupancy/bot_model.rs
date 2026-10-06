use crate::anim::remote_body::{CpuBodyGeom, CpuSurfMeta};
use bevy::prelude::*;

/// Retarget each source influence to the soldier bind skeleton, then use the
/// same animated skin matrices as its weapon and game-side pose.
pub(super) fn skin(
    model: &assets::bot_model::BotModel,
    skel: &asset_model::ModelSkel,
    matrices: &[Mat4],
    geom: &mut CpuBodyGeom,
) -> Result<(), String> {
    let bone = |name: &str| skel.bone_names.iter().position(|n| n == name);
    let mut transforms = Vec::with_capacity(model.joints.len());
    for joint in &model.joints {
        let index = bone(&joint.target)
            .ok_or_else(|| format!("bot model missing bone {}", joint.target))?;
        let origin = Vec3::from_array(joint.origin);
        let target = Vec3::from_array(skel.bones[index].trans);
        let rotation = joint
            .end
            .zip(joint.target_child.as_deref())
            .and_then(|(end, child)| {
                let child = bone(child)?;
                let from = (Vec3::from_array(end) - origin).try_normalize()?;
                let to = (Vec3::from_array(skel.bones[child].trans) - target).try_normalize()?;
                Some(Quat::from_rotation_arc(from, to))
            })
            .unwrap_or(Quat::IDENTITY);
        let bind = Mat4::from_rotation_translation(rotation, target - rotation * origin);
        transforms.push(*matrices.get(index).ok_or("bot skin matrix missing")? * bind);
    }
    for surface in &model.surfaces {
        let vertex_base = geom.packed.len() as u32;
        let index_start = geom.indices.len() as u32;
        for v in &surface.vertices {
            let mut position = Vec3::ZERO;
            let mut normal = Vec3::ZERO;
            let sum: f32 = v.weights.iter().sum();
            for i in 0..4 {
                let weight = v.weights[i] / sum.max(1e-8);
                let matrix = transforms[v.joints[i]];
                position += matrix.transform_point3(Vec3::from_array(v.position)) * weight;
                normal += matrix.transform_vector3(Vec3::from_array(v.normal)) * weight;
            }
            normal = normal.normalize_or(Vec3::Z);
            let tangent = normal.any_orthonormal_vector();
            let mut packed = [0u8; asset_iw4::size::GFX_PACKED_VERTEX];
            for (i, f) in position.to_array().iter().enumerate() {
                packed[i * 4..i * 4 + 4].copy_from_slice(&f.to_le_bytes());
            }
            packed[12..16].copy_from_slice(&1.0f32.to_le_bytes());
            packed[16..20].fill(255);
            packed[20..24].copy_from_slice(&v.uv.to_le_bytes());
            packed[24..28].copy_from_slice(&pack_normal(normal));
            packed[28..32].copy_from_slice(&pack_normal(tangent));
            geom.packed.push(packed);
        }
        geom.indices
            .extend(surface.indices.iter().map(|i| vertex_base + i));
        geom.surfaces.push(CpuSurfMeta {
            index_start,
            index_count: surface.indices.len() as u32,
            material: Some(asset_core::MaterialKey {
                namespace: asset_core::AssetNamespace::Iw4,
                name: surface.material.clone(),
            }),
        });
    }
    geom.decoded_n = geom.packed.len();
    Ok(())
}

fn pack_normal(n: Vec3) -> [u8; 4] {
    [
        (n.x * 127.0 + 127.5) as u8,
        (n.y * 127.0 + 127.5) as u8,
        (n.z * 127.0 + 127.5) as u8,
        63,
    ]
}
