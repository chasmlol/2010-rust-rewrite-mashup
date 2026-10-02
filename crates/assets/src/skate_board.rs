//! Derives `rig.json` and `board.json` from the skater GLB that Skate 3 setup
//! writes into the player's own asset folder. Nothing here ships converted
//! content: both files are generated on the player's machine.
use std::path::Path;

use bevy::math::DMat4;
use serde::Serialize;

use crate::glb::{Glb, Texture as BoardTexture, index};

const TEXTURE_LIMIT: u32 = 512;

/// Writes the two files unless both are already present.
pub fn ensure(assets: &Path) -> Result<(), String> {
    if assets.join("rig.json").is_file() && assets.join("board.json").is_file() {
        return Ok(());
    }
    export(assets)
}

pub fn export(assets: &Path) -> Result<(), String> {
    let glb_path = assets.join("private").join("skater.glb");
    let bytes = std::fs::read(&glb_path)
        .map_err(|error| format!("cannot read {}: {error}", glb_path.display()))?;
    let glb = Glb::parse(&bytes, "skater.glb")?;

    let skin = glb.at(&["skins", "0"])?;
    let joints = skin["joints"].as_array().ok_or("skin has no joints")?;
    let inverse_binds = glb.floats(index(&skin["inverseBindMatrices"])?)?;
    let names = joints
        .iter()
        .map(|joint| {
            let node = index(joint)?;
            glb.json["nodes"][node]["name"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("joint node {node} has no name"))
        })
        .collect::<Result<Vec<_>, String>>()?;

    // The GLB carries Blender's bone basis; the soldier retarget wants the
    // native bind frame, the board keeps the inverse binds as written.
    let basis_inverse = DMat4::from_cols_array(&[
        1., 0., 0., 0., 0., 0., -1., 0., 0., 1., 0., 0., 0., 0., 0., 1.,
    ])
    .inverse();
    let rig = names
        .iter()
        .zip(inverse_binds.as_chunks::<16>().0)
        .map(|(name, inverse)| {
            let columns = inverse.map(f64::from);
            let bind = DMat4::from_cols_array(&columns).inverse() * basis_inverse;
            RigBone {
                name,
                bind: bind.to_cols_array(),
                inverse_bind: *inverse,
            }
        })
        .collect::<Vec<_>>();

    let mut surfaces = Vec::new();
    let mut textures = Vec::new();
    let primitives = glb.at(&["meshes", "0", "primitives"])?;
    for primitive in primitives.as_array().ok_or("mesh has no primitives")? {
        let material = &glb.json["materials"][index(&primitive["material"])?];
        let material_name = material["name"].as_str().unwrap_or("");
        if !material_name.contains("Skate") {
            continue;
        }
        let texture = index(&material["pbrMetallicRoughness"]["baseColorTexture"]["index"])?;
        let image = index(&glb.json["textures"][texture]["source"])?;
        textures.push(glb.texture(image, TEXTURE_LIMIT)?);

        let attributes = &primitive["attributes"];
        let positions = glb.floats(index(&attributes["POSITION"])?)?;
        let normals = glb.floats(index(&attributes["NORMAL"])?)?;
        let uvs = glb.floats(index(&attributes["TEXCOORD_0"])?)?;
        let bone_ids = glb.integers(index(&attributes["JOINTS_0"])?)?;
        let weights = glb.floats(index(&attributes["WEIGHTS_0"])?)?;
        let count = positions.len() / 3;
        if normals.len() != count * 3
            || uvs.len() != count * 2
            || bone_ids.len() != count * 4
            || weights.len() != count * 4
        {
            return Err(format!("{material_name}: vertex streams disagree"));
        }
        let vertices = (0..count)
            .map(|v| {
                let u = half::f16::from_f32(uvs[v * 2]).to_bits();
                let w = half::f16::from_f32(uvs[v * 2 + 1]).to_bits();
                BoardVertex {
                    position: [positions[v * 3], positions[v * 3 + 1], positions[v * 3 + 2]],
                    normal: [normals[v * 3], normals[v * 3 + 1], normals[v * 3 + 2]],
                    uv: (u32::from(u) << 16) | u32::from(w),
                    joints: std::array::from_fn(|i| bone_ids[v * 4 + i]),
                    weights: std::array::from_fn(|i| weights[v * 4 + i]),
                }
            })
            .collect();
        let indices = glb
            .integers(index(&primitive["indices"])?)?
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[a, b, c]| [a, c, b])
            .collect();
        surfaces.push(BoardSurface {
            material: format!("iw4l_skate/{material_name}"),
            texture: textures.len() - 1,
            vertices,
            indices,
        });
    }
    if surfaces.is_empty() {
        return Err(format!("{} has no skateboard surfaces", glb_path.display()));
    }
    let board = Board {
        joints: names
            .iter()
            .map(|name| BoardJoint {
                target: name,
                origin: [0.; 3],
            })
            .collect(),
        surfaces,
        textures,
    };

    write_json(&assets.join("rig.json"), &rig)?;
    write_json(&assets.join("board.json"), &board)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let data = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, data)
        .and_then(|()| std::fs::rename(&partial, path))
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

#[derive(Serialize)]
struct RigBone<'a> {
    name: &'a str,
    bind: [f64; 16],
    inverse_bind: [f32; 16],
}

#[derive(Serialize)]
struct Board<'a> {
    joints: Vec<BoardJoint<'a>>,
    surfaces: Vec<BoardSurface>,
    textures: Vec<BoardTexture>,
}

#[derive(Serialize)]
struct BoardJoint<'a> {
    target: &'a str,
    origin: [f32; 3],
}

#[derive(Serialize)]
struct BoardSurface {
    material: String,
    texture: usize,
    vertices: Vec<BoardVertex>,
    indices: Vec<u32>,
}

#[derive(Serialize)]
struct BoardVertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: u32,
    joints: [u32; 4],
    weights: [f32; 4],
}

