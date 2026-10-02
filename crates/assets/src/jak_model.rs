//! Jak, his board and his gun, read from the GLB files the player exported
//! from their own copy of Jak 3 into the folder `IW4L_JAK_ASSETS` names, or
//! `jak-assets` beside the game.
//! Nothing here ships game content; Jak Mode falls back to the soldier
//! without them.
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use bevy::math::{Mat4, Quat, Vec3};
use serde_json::Value;

use crate::bot_model::{BotJoint, BotModel, BotSurface, BotTexture, BotVertex};
use crate::glb::{Glb, index};

/// The body, best first: the in-game Jak, then the variants that share his
/// skeleton.
const BODY_FILES: [&str; 3] = ["jakb-normal-lod0.glb", "jakb-c-lod0.glb", "jakb-lod0.glb"];
/// Every animation of Jak's skeleton travels with this model.
const CLIP_FILE: &str = "jakb-lod0.glb";
const BOARD_FILE: &str = "board-lod0.glb";
const GUN_FILE: &str = "gun-lod0.glb";
const TEXTURE_LIMIT: u32 = 512;
const DEFAULT_FOLDER: &str = "jak-assets";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trs {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Trs {
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        Self {
            translation: self.translation.lerp(other.translation, t),
            rotation: self.rotation.slerp(other.rotation, t),
            scale: self.scale.lerp(other.scale, t),
        }
    }
}

pub struct Joint {
    pub name: String,
    pub parent: Option<usize>,
    pub rest: Trs,
}

/// A skinned model: its skeleton, mesh and the animations of that skeleton,
/// in the file's meters with y up.
pub struct Rig {
    pub joints: Vec<Joint>,
    /// Joints ordered so every parent comes before its children.
    order: Vec<usize>,
    /// One per joint: the skin's joints are the skeleton.
    pub inverse_binds: Vec<Mat4>,
    pub mesh: BotModel,
    pub clips: HashMap<String, Clip>,
}

pub struct Clip {
    pub duration: f32,
    tracks: Vec<Track>,
}

struct Track {
    joint: usize,
    path: Channel,
    times: Vec<f32>,
    values: Vec<[f32; 4]>,
}

#[derive(Clone, Copy, PartialEq)]
enum Channel {
    Translation,
    Rotation,
    Scale,
}

impl Clip {
    /// The clip's pose at `time` seconds, clamped to its length, written over
    /// `pose`; joints it does not animate keep what `pose` holds.
    pub fn sample(&self, time: f32, pose: &mut [Trs]) {
        for track in &self.tracks {
            let Some(slot) = pose.get_mut(track.joint) else {
                continue;
            };
            let value = track.sample(time);
            match track.path {
                Channel::Translation => slot.translation = Vec3::new(value[0], value[1], value[2]),
                Channel::Scale => slot.scale = Vec3::new(value[0], value[1], value[2]),
                Channel::Rotation => {
                    slot.rotation = Quat::from_array(value).normalize();
                }
            }
        }
    }
}

impl Track {
    fn sample(&self, time: f32) -> [f32; 4] {
        let last = self.times.len() - 1;
        let next = self.times.partition_point(|&t| t <= time);
        if next == 0 {
            return self.values[0];
        }
        if next > last {
            return self.values[last];
        }
        let (a, b) = (next - 1, next);
        let span = self.times[b] - self.times[a];
        let t = if span > 0.0 {
            (time - self.times[a]) / span
        } else {
            0.0
        };
        let (va, vb) = (self.values[a], self.values[b]);
        if self.path == Channel::Rotation {
            Quat::from_array(va)
                .slerp(Quat::from_array(vb), t)
                .to_array()
        } else {
            std::array::from_fn(|k| va[k] + (vb[k] - va[k]) * t)
        }
    }
}

impl Rig {
    pub fn rest_pose(&self) -> Vec<Trs> {
        self.joints.iter().map(|j| j.rest).collect()
    }

    pub fn joint(&self, name: &str) -> Option<usize> {
        self.joints.iter().position(|j| j.name == name)
    }

    /// Every joint's matrix in the model's frame for `pose`.
    pub fn globals(&self, pose: &[Trs]) -> Vec<Mat4> {
        let mut out = vec![Mat4::IDENTITY; self.joints.len()];
        for &j in &self.order {
            let local = pose.get(j).copied().unwrap_or(self.joints[j].rest).matrix();
            out[j] = match self.joints[j].parent {
                Some(p) => out[p] * local,
                None => local,
            };
        }
        out
    }

    /// Each joint's skinning matrix: `frame * global * inverse bind`.
    pub fn skin_matrices(&self, globals: &[Mat4], frame: Mat4) -> Vec<Mat4> {
        globals
            .iter()
            .zip(&self.inverse_binds)
            .map(|(global, inverse)| frame * *global * *inverse)
            .collect()
    }
}

pub struct JakAssets {
    pub body: Rig,
    pub board: Option<Rig>,
    pub gun: Option<Rig>,
}

impl JakAssets {
    pub fn meshes(&self) -> impl Iterator<Item = &BotModel> {
        std::iter::once(&self.body.mesh)
            .chain(self.board.as_ref().map(|r| &r.mesh))
            .chain(self.gun.as_ref().map(|r| &r.mesh))
    }
}

pub fn local_jak() -> Option<&'static JakAssets> {
    static ASSETS: OnceLock<Option<JakAssets>> = OnceLock::new();
    ASSETS
        .get_or_init(|| {
            let root = std::env::var_os("IW4L_JAK_ASSETS")
                .map(std::path::PathBuf::from)
                .or_else(|| Some(Path::new(DEFAULT_FOLDER).to_path_buf()).filter(|p| p.is_dir()))?;
            match load(&root) {
                Ok(assets) => {
                    diag::info!(
                        World,
                        "Jak model: {} clips, board {}, gun {}",
                        assets.body.clips.len(),
                        assets.board.is_some(),
                        assets.gun.is_some()
                    );
                    Some(assets)
                }
                Err(error) => {
                    diag::warn!(World, "Jak model: {error}");
                    None
                }
            }
        })
        .as_ref()
}

pub fn load(root: &Path) -> Result<JakAssets, String> {
    let body_file = BODY_FILES
        .iter()
        .map(|name| root.join(name))
        .find(|path| path.is_file())
        .ok_or_else(|| format!("no {} in {}", BODY_FILES[0], root.display()))?;
    let mut body = rig(&body_file, "body")?;
    let clip_file = root.join(CLIP_FILE);
    if clip_file.is_file() {
        let bytes = read(&clip_file)?;
        let glb = Glb::parse(&bytes, CLIP_FILE)?;
        body.clips = clips(&glb, &body.joints)?;
    }
    let optional = |name: &str, label: &str| -> Result<Option<Rig>, String> {
        let path = root.join(name);
        path.is_file().then(|| rig(&path, label)).transpose()
    };
    Ok(JakAssets {
        body,
        board: optional(BOARD_FILE, "board")?,
        gun: optional(GUN_FILE, "gun")?,
    })
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))
}

fn rig(path: &Path, label: &str) -> Result<Rig, String> {
    let bytes = read(path)?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("model");
    let glb = Glb::parse(&bytes, name)?;
    let skin = glb.at(&["skins", "0"])?;
    let skin_nodes = skin["joints"]
        .as_array()
        .ok_or("skin has no joints")?
        .iter()
        .map(index)
        .collect::<Result<Vec<_>, _>>()?;
    let inverse_binds = glb
        .floats(index(&skin["inverseBindMatrices"])?)?
        .as_chunks::<16>()
        .0
        .iter()
        .map(Mat4::from_cols_array)
        .collect::<Vec<_>>();
    if inverse_binds.len() != skin_nodes.len() {
        return Err(format!("{name}: inverse binds and joints disagree"));
    }

    let nodes = glb.json["nodes"].as_array().ok_or("no nodes")?;
    let mut parent_node = vec![None; nodes.len()];
    for (n, node) in nodes.iter().enumerate() {
        for child in node["children"].as_array().into_iter().flatten() {
            if let Some(child) = child.as_u64().and_then(|c| parent_node.get_mut(c as usize)) {
                *child = Some(n);
            }
        }
    }
    let joint_of_node: HashMap<usize, usize> = skin_nodes
        .iter()
        .enumerate()
        .map(|(j, &n)| (n, j))
        .collect();
    let joints = skin_nodes
        .iter()
        .map(|&n| {
            let node = &nodes[n];
            Ok(Joint {
                name: node["name"].as_str().unwrap_or("").to_owned(),
                parent: parent_node[n].and_then(|p| joint_of_node.get(&p).copied()),
                rest: node_trs(node)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let order = parents_first(&joints)?;

    let mesh = mesh(&glb, name, label, &joints)?;
    let clips = clips(&glb, &joints)?;
    Ok(Rig {
        joints,
        order,
        inverse_binds,
        mesh,
        clips,
    })
}

fn node_trs(node: &Value) -> Result<Trs, String> {
    let floats = |key: &str| -> Option<Vec<f32>> {
        node[key]
            .as_array()
            .map(|a| a.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect())
    };
    if let Some(m) = floats("matrix") {
        let m: [f32; 16] = m.try_into().map_err(|_| "bad node matrix")?;
        let (scale, rotation, translation) =
            Mat4::from_cols_array(&m).to_scale_rotation_translation();
        return Ok(Trs {
            translation,
            rotation,
            scale,
        });
    }
    let vec3 = |key: &str, default: Vec3| {
        floats(key)
            .filter(|v| v.len() == 3)
            .map_or(default, |v| Vec3::new(v[0], v[1], v[2]))
    };
    Ok(Trs {
        translation: vec3("translation", Vec3::ZERO),
        rotation: floats("rotation")
            .filter(|v| v.len() == 4)
            .map_or(Quat::IDENTITY, |v| {
                Quat::from_xyzw(v[0], v[1], v[2], v[3]).normalize()
            }),
        scale: vec3("scale", Vec3::ONE),
    })
}

fn parents_first(joints: &[Joint]) -> Result<Vec<usize>, String> {
    let mut depth = vec![0usize; joints.len()];
    for (j, d) in depth.iter_mut().enumerate() {
        let mut at = joints[j].parent;
        while let Some(p) = at {
            *d += 1;
            if *d > joints.len() {
                return Err("joint hierarchy has a cycle".into());
            }
            at = joints[p].parent;
        }
    }
    let mut order: Vec<usize> = (0..joints.len()).collect();
    order.sort_by_key(|&j| depth[j]);
    Ok(order)
}

/// The mesh's textured primitives, each its own surface with only the
/// vertices it uses, wound for IW4.
fn mesh(glb: &Glb, name: &str, label: &str, joints: &[Joint]) -> Result<BotModel, String> {
    let mut surfaces = Vec::new();
    let mut textures = Vec::new();
    let mut texture_of_image = HashMap::new();
    let primitives = glb.at(&["meshes", "0", "primitives"])?;
    for (p, primitive) in primitives
        .as_array()
        .ok_or("mesh has no primitives")?
        .iter()
        .enumerate()
    {
        let Ok(indices) = index(&primitive["indices"]).and_then(|i| glb.integers(i)) else {
            continue;
        };
        if indices.is_empty() {
            continue;
        }
        let material = primitive["material"]
            .as_u64()
            .map(|m| &glb.json["materials"][m as usize]);
        let factor = material
            .and_then(|m| m["pbrMetallicRoughness"]["baseColorFactor"].as_array())
            .map(|f| {
                std::array::from_fn::<f32, 4, _>(|k| {
                    f.get(k).and_then(Value::as_f64).unwrap_or(1.0) as f32
                })
            })
            .unwrap_or([1.0; 4]);
        let image = material
            .and_then(|m| m["pbrMetallicRoughness"]["baseColorTexture"]["index"].as_u64())
            .and_then(|t| glb.json["textures"][t as usize]["source"].as_u64())
            .map(|i| i as usize);
        let texture = match texture_of_image.get(&image) {
            Some(&t) => t,
            None => {
                let texture = match image {
                    Some(i) => {
                        let t = glb.texture(i, TEXTURE_LIMIT)?;
                        tinted(t.width, t.height, t.rgba, factor)?
                    }
                    None => tinted(1, 1, vec![255; 4], factor)?,
                };
                textures.push(texture);
                texture_of_image.insert(image, textures.len() - 1);
                textures.len() - 1
            }
        };

        let attributes = &primitive["attributes"];
        let positions = glb.floats(index(&attributes["POSITION"])?)?;
        let normals = glb.floats(index(&attributes["NORMAL"])?)?;
        let uvs = glb.floats(index(&attributes["TEXCOORD_0"])?)?;
        let bones = glb.integers(index(&attributes["JOINTS_0"])?)?;
        let weights = glb.floats(index(&attributes["WEIGHTS_0"])?)?;
        let count = positions.len() / 3;
        if normals.len() != count * 3
            || uvs.len() != count * 2
            || bones.len() != count * 4
            || weights.len() != count * 4
        {
            return Err(format!("{name}: vertex streams disagree"));
        }
        let mut remap = HashMap::new();
        let mut vertices = Vec::new();
        let mut local = Vec::with_capacity(indices.len());
        for &i in &indices {
            let i = i as usize;
            if i >= count {
                return Err(format!("{name}: index outside the mesh"));
            }
            let at = *remap.entry(i).or_insert_with(|| {
                let u = half::f16::from_f32(uvs[i * 2]).to_bits();
                let v = half::f16::from_f32(uvs[i * 2 + 1]).to_bits();
                vertices.push(BotVertex {
                    position: [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]],
                    normal: [normals[i * 3], normals[i * 3 + 1], normals[i * 3 + 2]],
                    uv: (u32::from(u) << 16) | u32::from(v),
                    joints: std::array::from_fn(|k| {
                        (bones[i * 4 + k] as usize).min(joints.len() - 1)
                    }),
                    weights: std::array::from_fn(|k| weights[i * 4 + k]),
                });
                (vertices.len() - 1) as u32
            });
            local.push(at);
        }
        surfaces.push(BotSurface {
            material: format!("iw4l_jak/{label}/{p}"),
            texture,
            vertices,
            indices: local
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|&[a, b, c]| [a, c, b])
                .collect(),
        });
    }
    if surfaces.is_empty() {
        return Err(format!("{name} has no textured surfaces"));
    }
    Ok(BotModel {
        lighting_gain: 1.0,
        joints: joints
            .iter()
            .map(|j| BotJoint {
                target: j.name.clone(),
                origin: [0.0; 3],
                end: None,
                target_child: None,
            })
            .collect(),
        surfaces,
        textures,
    })
}

/// The texture times its material's colour factor; the exporter keeps the
/// console's half-range colours and doubles them back through the factor.
fn tinted(
    width: u32,
    height: u32,
    mut rgba: Vec<u8>,
    factor: [f32; 4],
) -> Result<BotTexture, String> {
    for pixel in rgba.chunks_exact_mut(4) {
        for (c, f) in pixel.iter_mut().zip(factor) {
            *c = (f32::from(*c) * f).round().clamp(0.0, 255.0) as u8;
        }
    }
    Ok(BotTexture {
        width: u16::try_from(width).map_err(|_| "texture too wide")?,
        height: u16::try_from(height).map_err(|_| "texture too tall")?,
        rgba,
    })
}

/// Every animation in `glb`, its channels matched to `joints` by node name.
fn clips(glb: &Glb, joints: &[Joint]) -> Result<HashMap<String, Clip>, String> {
    let nodes = &glb.json["nodes"];
    let mut out = HashMap::new();
    for animation in glb.json["animations"].as_array().into_iter().flatten() {
        let Some(name) = animation["name"].as_str() else {
            continue;
        };
        let samplers = &animation["samplers"];
        let mut tracks = Vec::new();
        let mut duration = 0.0f32;
        for channel in animation["channels"].as_array().into_iter().flatten() {
            let target = &channel["target"];
            let path = match target["path"].as_str() {
                Some("translation") => Channel::Translation,
                Some("rotation") => Channel::Rotation,
                Some("scale") => Channel::Scale,
                _ => continue,
            };
            let Some(node_name) = target["node"]
                .as_u64()
                .and_then(|n| nodes[n as usize]["name"].as_str())
            else {
                continue;
            };
            let Some(joint) = joints.iter().position(|j| j.name == node_name) else {
                continue;
            };
            let sampler = &samplers[index(&channel["sampler"])?];
            let times = glb.floats(index(&sampler["input"])?)?;
            let raw = glb.floats(index(&sampler["output"])?)?;
            let width = if path == Channel::Rotation { 4 } else { 3 };
            if times.is_empty() || raw.len() != times.len() * width {
                continue;
            }
            let values = raw
                .chunks_exact(width)
                .map(|c| std::array::from_fn(|k| c.get(k).copied().unwrap_or(0.0)))
                .collect();
            duration = duration.max(*times.last().unwrap_or(&0.0));
            tracks.push(Track {
                joint,
                path,
                times,
                values,
            });
        }
        if !tracks.is_empty() {
            out.insert(name.to_owned(), Clip { duration, tracks });
        }
    }
    Ok(out)
}
