//! Session-only prop geometry and placement primitives.
//!
//! The catalog contains extracted local geometry and host-authored park pieces.
use bevy::prelude::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

const MAX_PROPS: usize = 100_000;
const MAX_TRIANGLES_PER_PROP: usize = 1_000_000;
const MAX_MESHES_PER_PROP: usize = 256;
const MAX_VERTICES_PER_MESH: usize = 1_000_000;
const MAX_INDICES_PER_MESH: usize = 3_000_000;
const MAX_PLACED_PROPS: usize = 128;
const MAX_MODEL_BYTES: u64 = 256 * 1024 * 1024;
const MAX_COLLISION_BYTES: u64 = 128 * 1024 * 1024;
pub const RETAIL_MODEL_UNIT_METERS: f32 = 0.0254;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PropCatalog {
    pub schema: u32,
    pub props: Vec<PropDefinition>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PropDefinition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub meshes: Vec<PropMesh>,
    pub triangles: Vec<[[f32; 3]; 3]>,
    #[serde(default)]
    pub grind_rails: Vec<[[f32; 3]; 2]>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PropMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    #[serde(default)]
    pub colors: Vec<[f32; 4]>,
}

impl PropCatalog {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path)
            .map_err(|e| format!("could not read prop catalog {}: {e}", path.display()))?;
        let manifest: DiskCatalog = serde_json::from_slice(&bytes)
            .map_err(|e| format!("invalid prop catalog {}: {e}", path.display()))?;
        if manifest.schema != 3 {
            return Err(format!(
                "unsupported prop catalog schema {}; rerun the Skate converter",
                manifest.schema
            ));
        }
        let root = path
            .parent()
            .ok_or("prop catalog path has no parent directory")?;
        let mut props = Vec::with_capacity(manifest.props.len());
        for entry in manifest.props {
            let model_path = safe_asset_path(root, &entry.model)?;
            let collision_path = safe_asset_path(root, &entry.collision)?;
            props.push(PropDefinition {
                id: entry.id,
                name: entry.name,
                meshes: read_runtime_model(&model_path)?,
                triangles: read_runtime_collision(&collision_path)?,
                grind_rails: entry.grind_rails,
            });
        }

        let catalog = Self { schema: 1, props };
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn with_basic_park_objects(mut self) -> Self {
        let ids: HashSet<_> = self.props.iter().map(|prop| prop.id.clone()).collect();
        self.props.extend(
            Self::basic_park_objects()
                .props
                .into_iter()
                .filter(|prop| !ids.contains(&prop.id)),
        );
        self
    }

    pub fn basic_park_objects() -> Self {
        let mut props = Vec::with_capacity(15);
        for (id, name, length, height, width, style) in [
            ("kicker_micro", "Micro kicker", 0.55, 0.18, 0.9, 0),
            ("kicker_small", "Small kicker", 0.8, 0.28, 1.15, 0),
            ("kicker_steep", "Steep kicker", 0.72, 0.42, 1.1, 0),
            ("kicker_wide", "Wide kicker", 1.0, 0.3, 1.8, 1),
            ("bank_low", "Low bank ramp", 1.25, 0.4, 1.5, 1),
            ("bank_high", "High bank ramp", 1.4, 0.62, 1.5, 1),
            ("spine_ramp", "Spine ramp", 1.0, 0.42, 1.35, 2),
            ("quarter_micro", "Micro quarter pipe", 0.95, 0.42, 1.25, 3),
            ("quarter_mini", "Mini quarter pipe", 1.35, 0.72, 1.5, 3),
            ("quarter_vert", "Vert quarter pipe", 1.8, 1.15, 1.8, 3),
        ] {
            props.push(make_ramp(id, name, length, height, width, style));
        }
        for (id, name, length, height, width, slope) in [
            (
                "rail_flat_short",
                "Low short grind rail",
                1.2,
                0.22,
                0.08,
                0.0,
            ),
            (
                "rail_flat_medium",
                "Medium grind rail",
                2.0,
                0.38,
                0.08,
                0.0,
            ),
            ("rail_flat_long", "Long grind rail", 3.0, 0.48, 0.08, 0.0),
            (
                "rail_hand_low",
                "Low sloped handrail",
                2.0,
                0.62,
                0.08,
                0.24,
            ),
            (
                "rail_hand_high",
                "High sloped handrail",
                2.6,
                0.92,
                0.08,
                0.36,
            ),
        ] {
            props.push(make_rail(id, name, length, height, width, slope));
        }
        let catalog = Self { schema: 1, props };
        catalog
            .validate()
            .expect("built-in park props must be valid");
        catalog
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1 {
            return Err(format!("unsupported prop catalog schema {}", self.schema));
        }
        if self.props.is_empty() || self.props.len() > MAX_PROPS {
            return Err("prop catalog must contain between 1 and 100000 entries".into());
        }
        let mut ids = HashSet::with_capacity(self.props.len());
        for prop in &self.props {
            if prop.id.is_empty()
                || prop.id.len() > 128
                || !prop
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                || prop.name.trim().is_empty()
                || prop.name.len() > 256
            {
                return Err(format!(
                    "prop {:?} has an invalid id or display name",
                    prop.id
                ));
            }
            if !ids.insert(&prop.id) {
                return Err(format!("duplicate prop id {:?}", prop.id));
            }
            if prop.triangles.is_empty() || prop.triangles.len() > MAX_TRIANGLES_PER_PROP {
                return Err(format!(
                    "prop {:?} has no collision geometry or exceeds the triangle limit",
                    prop.id
                ));
            }
            if prop.meshes.is_empty() || prop.meshes.len() > MAX_MESHES_PER_PROP {
                return Err(format!(
                    "prop {:?} has no render meshes or exceeds the mesh limit",
                    prop.id
                ));
            }
            for mesh in &prop.meshes {
                if mesh.positions.is_empty()
                    || mesh.positions.len() > MAX_VERTICES_PER_MESH
                    || mesh.indices.is_empty()
                    || mesh.indices.len() > MAX_INDICES_PER_MESH
                    || mesh.indices.len() % 3 != 0
                    || mesh.normals.len() != mesh.positions.len()
                    || mesh.uvs.len() != mesh.positions.len()
                    || (!mesh.colors.is_empty() && mesh.colors.len() != mesh.positions.len())
                    || mesh
                        .indices
                        .iter()
                        .any(|&i| i as usize >= mesh.positions.len())
                {
                    return Err(format!(
                        "prop {:?} contains invalid render mesh dimensions or indices",
                        prop.id
                    ));
                }
                if mesh
                    .positions
                    .iter()
                    .flatten()
                    .chain(mesh.normals.iter().flatten())
                    .chain(mesh.uvs.iter().flatten())
                    .chain(mesh.colors.iter().flatten())
                    .any(|v| !v.is_finite())
                {
                    return Err(format!(
                        "prop {:?} contains non-finite render geometry",
                        prop.id
                    ));
                }
            }
            if prop.grind_rails.iter().any(|rail| {
                rail.iter().flatten().any(|value| !value.is_finite())
                    || (Vec3::from_array(rail[1]) - Vec3::from_array(rail[0])).length_squared()
                        <= 1.0e-8
            }) {
                return Err(format!(
                    "prop {:?} contains invalid grind rail geometry",
                    prop.id
                ));
            }
            if prop
                .triangles
                .iter()
                .flatten()
                .flatten()
                .any(|v| !v.is_finite())
            {
                return Err(format!(
                    "prop {:?} contains non-finite collision geometry",
                    prop.id
                ));
            }
            if prop.triangles.iter().any(|t| {
                let a = Vec3::from_array(t[0]);
                let b = Vec3::from_array(t[1]);
                let c = Vec3::from_array(t[2]);
                (b - a).cross(c - a).length_squared() <= 1.0e-10
            }) {
                return Err(format!(
                    "prop {:?} contains degenerate collision triangles",
                    prop.id
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct PlacedProp {
    pub definition: usize,
    pub position: Vec3,
    pub yaw: f32,
}

impl PlacedProp {
    pub fn collision_triangles(&self, catalog: &PropCatalog) -> Result<Vec<[[f32; 3]; 3]>, String> {
        let prop = catalog
            .props
            .get(self.definition)
            .ok_or("placed prop references an unknown catalog entry")?;
        if !self.position.is_finite() || !self.yaw.is_finite() {
            return Err(format!(
                "placed prop {:?} has a non-finite transform",
                prop.id
            ));
        }
        let transform =
            Mat4::from_rotation_translation(Quat::from_rotation_y(self.yaw), self.position);
        let triangles: Vec<_> = prop
            .triangles
            .iter()
            .map(|triangle| {
                triangle.map(|p| {
                    transform
                        .transform_point3(Vec3::from_array(p) * RETAIL_MODEL_UNIT_METERS)
                        .to_array()
                })
            })
            .collect();
        if triangles.iter().flatten().flatten().any(|v| !v.is_finite()) {
            return Err(format!(
                "placed prop {:?} produced non-finite collision geometry",
                prop.id
            ));
        }
        Ok(triangles)
    }

    pub fn grind_rails(&self, catalog: &PropCatalog) -> Result<Vec<Vec<[f32; 3]>>, String> {
        let prop = catalog
            .props
            .get(self.definition)
            .ok_or("placed prop references an unknown catalog entry")?;
        if !self.position.is_finite() || !self.yaw.is_finite() {
            return Err(format!(
                "placed prop {:?} has a non-finite transform",
                prop.id
            ));
        }
        let rotation = Quat::from_rotation_y(self.yaw);
        Ok(prop
            .grind_rails
            .iter()
            .map(|rail| {
                rail.map(|point| {
                    (rotation * (Vec3::from_array(point) * RETAIL_MODEL_UNIT_METERS)
                        + self.position)
                        .to_array()
                })
                .to_vec()
            })
            .collect())
    }
}

struct ParkMeshBuilder {
    mesh: PropMesh,
    triangles: Vec<[[f32; 3]; 3]>,
}

impl ParkMeshBuilder {
    fn new() -> Self {
        Self {
            mesh: PropMesh {
                positions: Vec::new(),
                normals: Vec::new(),
                uvs: Vec::new(),
                indices: Vec::new(),
                colors: Vec::new(),
            },
            triangles: Vec::new(),
        }
    }

    fn triangle(&mut self, points: [[f32; 3]; 3], color: [f32; 4]) {
        let a = Vec3::from_array(points[0]);
        let b = Vec3::from_array(points[1]);
        let c = Vec3::from_array(points[2]);
        let normal = (b - a).cross(c - a);
        if normal.length_squared() <= 1.0e-10 {
            return;
        }
        let normal = normal.normalize().to_array();
        let start = self.mesh.positions.len() as u32;
        for point in points {
            self.mesh
                .positions
                .push((Vec3::from_array(point) / RETAIL_MODEL_UNIT_METERS).to_array());
            self.mesh.normals.push(normal);
            self.mesh.uvs.push([point[0], point[1]]);
            self.mesh.colors.push(color);
        }
        self.mesh.indices.extend([start, start + 1, start + 2]);
        self.triangles.push(
            points.map(|point| (Vec3::from_array(point) / RETAIL_MODEL_UNIT_METERS).to_array()),
        );
    }

    fn quad(&mut self, points: [[f32; 3]; 4], color: [f32; 4]) {
        self.triangle([points[0], points[1], points[2]], color);
        self.triangle([points[0], points[2], points[3]], color);
    }

    fn box_mesh(&mut self, min: Vec3, max: Vec3, color: [f32; 4]) {
        let [x0, y0, z0] = min.to_array();
        let [x1, y1, z1] = max.to_array();
        for face in [
            [[x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [x1, y0, z0]],
            [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
            [[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]],
            [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
            [[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]],
        ] {
            self.quad(face, color);
        }
    }

    fn cylinder(&mut self, start: Vec3, end: Vec3, radius: f32, color: [f32; 4]) {
        let axis = end - start;
        if axis.length_squared() <= 1.0e-8 {
            return;
        }
        let axis = axis.normalize();
        let u = axis.any_orthonormal_vector();
        let v = axis.cross(u).normalize();
        let segments = 10;
        for i in 0..segments {
            let angle_a = std::f32::consts::TAU * i as f32 / segments as f32;
            let angle_b = std::f32::consts::TAU * (i + 1) as f32 / segments as f32;
            let offset = |angle: f32| (u * angle.cos() + v * angle.sin()) * radius;
            let a = start + offset(angle_a);
            let b = start + offset(angle_b);
            let c = end + offset(angle_b);
            let d = end + offset(angle_a);
            self.quad(
                [a.to_array(), b.to_array(), c.to_array(), d.to_array()],
                color,
            );
        }
    }

    fn finish(self, id: &str, name: &str, grind_rails: Vec<[[f32; 3]; 2]>) -> PropDefinition {
        PropDefinition {
            id: format!("iw4l_{id}"),
            name: name.into(),
            meshes: vec![self.mesh],
            triangles: self.triangles,
            grind_rails: grind_rails
                .into_iter()
                .map(|rail| {
                    rail.map(|point| {
                        (Vec3::from_array(point) / RETAIL_MODEL_UNIT_METERS).to_array()
                    })
                })
                .collect(),
        }
    }
}

fn make_ramp(
    id: &str,
    name: &str,
    length: f32,
    height: f32,
    width: f32,
    style: u8,
) -> PropDefinition {
    let mut builder = ParkMeshBuilder::new();
    let depth = 0.12_f32.min(height * 0.4);
    let (mut profile, ride_segments) = if style == 3 {
        let radius = length;
        let segments = 10;
        let arc = (0..=segments)
            .map(|i| {
                let angle = std::f32::consts::FRAC_PI_2 * i as f32 / segments as f32;
                [radius * angle.sin(), height * (1.0 - angle.cos())]
            })
            .collect::<Vec<_>>();
        (arc, segments)
    } else if style == 2 {
        (
            vec![[-length * 0.5, 0.0], [0.0, height], [length * 0.5, 0.0]],
            2,
        )
    } else {
        (vec![[0.0, 0.0], [length, height]], 1)
    };
    profile.push([profile.last().unwrap()[0], -depth]);
    profile.push([profile[0][0], -depth]);
    let half = width * 0.5;
    let deck = match style {
        1 => [0.42, 0.31, 0.20, 1.0],
        3 => [0.34, 0.37, 0.40, 1.0],
        _ => [0.50, 0.52, 0.54, 1.0],
    };
    let dark = [0.24, 0.26, 0.28, 1.0];
    for edge in 0..profile.len() {
        let [x0, y0] = profile[edge];
        let [x1, y1] = profile[(edge + 1) % profile.len()];
        if edge < ride_segments {
            let panels = (width / 0.22).ceil().clamp(3.0, 12.0) as usize;
            for panel in 0..panels {
                let z0 = -half + width * panel as f32 / panels as f32;
                let z1 = -half + width * (panel + 1) as f32 / panels as f32;
                let shade = if (panel + edge) % 2 == 0 { 1.0 } else { 0.9 };
                let color = [deck[0] * shade, deck[1] * shade, deck[2] * shade, 1.0];
                builder.quad(
                    [[x0, y0, z0], [x1, y1, z0], [x1, y1, z1], [x0, y0, z1]],
                    color,
                );
            }
        } else {
            builder.quad(
                [
                    [x0, y0, -half],
                    [x1, y1, -half],
                    [x1, y1, half],
                    [x0, y0, half],
                ],
                dark,
            );
        }
    }
    for side in [-half, half] {
        let points: Vec<_> = profile.iter().map(|[x, y]| [*x, *y, side]).collect();
        for i in 1..points.len() - 1 {
            builder.triangle([points[0], points[i], points[i + 1]], dark);
        }
    }

    let lip = profile[if style == 2 { 1 } else { ride_segments }];
    let coping_radius = 0.025_f32;
    builder.cylinder(
        Vec3::new(lip[0], lip[1], -half),
        Vec3::new(lip[0], lip[1], half),
        coping_radius,
        [0.68, 0.70, 0.72, 1.0],
    );
    let grind_rails = vec![[
        [lip[0], lip[1] + coping_radius, -half],
        [lip[0], lip[1] + coping_radius, half],
    ]];
    builder.finish(id, name, grind_rails)
}

fn make_rail(
    id: &str,
    name: &str,
    length: f32,
    height: f32,
    thickness: f32,
    slope: f32,
) -> PropDefinition {
    let mut builder = ParkMeshBuilder::new();
    let start = Vec3::new(-length * 0.5, height, 0.0);
    let end = Vec3::new(length * 0.5, height + slope, 0.0);
    let radius = thickness * 0.5;
    let steel = [0.60, 0.64, 0.68, 1.0];
    let dark_steel = [0.31, 0.34, 0.37, 1.0];
    builder.cylinder(start, end, radius, steel);
    let support_count = (length / 0.8).ceil() as usize + 1;
    for i in 0..support_count {
        let t = i as f32 / (support_count - 1).max(1) as f32;
        let point = start.lerp(end, t);
        builder.box_mesh(
            Vec3::new(point.x - 0.045, 0.0, -0.045),
            Vec3::new(point.x + 0.045, point.y - radius, 0.045),
            dark_steel,
        );
        builder.box_mesh(
            Vec3::new(point.x - 0.11, 0.0, -0.12),
            Vec3::new(point.x + 0.11, 0.035, 0.12),
            [0.43, 0.45, 0.47, 1.0],
        );
    }
    builder.finish(
        id,
        name,
        vec![[
            [start.x, start.y + radius, start.z],
            [end.x, end.y + radius, end.z],
        ]],
    )
}

#[derive(Deserialize)]
struct DiskCatalog {
    schema: u32,
    props: Vec<DiskProp>,
}

#[derive(Deserialize)]
struct DiskProp {
    id: String,
    name: String,
    model: PathBuf,
    collision: PathBuf,
    #[serde(default)]
    grind_rails: Vec<[[f32; 3]; 2]>,
}

fn safe_asset_path(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!("unsafe prop asset path {:?}", relative));
    }
    Ok(root.join(relative))
}

fn read_runtime_model(path: &Path) -> Result<Vec<PropMesh>, String> {
    check_asset_size(path, MAX_MODEL_BYTES)?;
    let bytes = std::fs::read(path)
        .map_err(|e| format!("could not read prop model {}: {e}", path.display()))?;
    let mut cursor = BinaryCursor::new(&bytes, b"IW4LPM01", path)?;
    let mesh_count = cursor.u32()? as usize;
    if mesh_count == 0 || mesh_count > MAX_MESHES_PER_PROP {
        return Err(format!("invalid mesh count in {}", path.display()));
    }
    let mut meshes = Vec::with_capacity(mesh_count);
    for _ in 0..mesh_count {
        let vertex_count = cursor.u32()? as usize;
        let index_count = cursor.u32()? as usize;
        if vertex_count == 0
            || vertex_count > MAX_VERTICES_PER_MESH
            || index_count == 0
            || index_count > MAX_INDICES_PER_MESH
            || index_count % 3 != 0
        {
            return Err(format!("invalid mesh dimensions in {}", path.display()));
        }
        let mut positions = Vec::with_capacity(vertex_count);
        let mut normals = Vec::with_capacity(vertex_count);
        let mut uvs = Vec::with_capacity(vertex_count);
        for _ in 0..vertex_count {
            positions.push([cursor.f32()?, cursor.f32()?, cursor.f32()?]);
            normals.push([cursor.f32()?, cursor.f32()?, cursor.f32()?]);
            uvs.push([cursor.f32()?, cursor.f32()?]);
        }
        let mut indices = Vec::with_capacity(index_count);
        for _ in 0..index_count {
            indices.push(cursor.u32()?);
        }
        meshes.push(PropMesh {
            positions,
            normals,
            uvs,
            indices,
            colors: Vec::new(),
        });
    }
    cursor.finish()?;
    Ok(meshes)
}

fn read_runtime_collision(path: &Path) -> Result<Vec<[[f32; 3]; 3]>, String> {
    check_asset_size(path, MAX_COLLISION_BYTES)?;
    let bytes = std::fs::read(path)
        .map_err(|e| format!("could not read prop collision {}: {e}", path.display()))?;
    let mut cursor = BinaryCursor::new(&bytes, b"IW4LPC01", path)?;
    let triangle_count = cursor.u32()? as usize;
    if triangle_count == 0 || triangle_count > MAX_TRIANGLES_PER_PROP {
        return Err(format!(
            "invalid collision triangle count in {}",
            path.display()
        ));
    }
    let mut triangles = Vec::with_capacity(triangle_count);
    for _ in 0..triangle_count {
        triangles.push([
            [cursor.f32()?, cursor.f32()?, cursor.f32()?],
            [cursor.f32()?, cursor.f32()?, cursor.f32()?],
            [cursor.f32()?, cursor.f32()?, cursor.f32()?],
        ]);
    }
    cursor.finish()?;
    Ok(triangles)
}

fn check_asset_size(path: &Path, maximum: u64) -> Result<(), String> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("could not inspect prop asset {}: {e}", path.display()))?
        .len();
    if size > maximum {
        return Err(format!(
            "prop asset {} exceeds the {maximum}-byte size limit",
            path.display()
        ));
    }
    Ok(())
}

struct BinaryCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
    path: &'a Path,
}

impl<'a> BinaryCursor<'a> {
    fn new(bytes: &'a [u8], magic: &[u8; 8], path: &'a Path) -> Result<Self, String> {
        if bytes.get(..8) != Some(magic.as_slice()) {
            return Err(format!("invalid prop asset header in {}", path.display()));
        }
        Ok(Self {
            bytes,
            offset: 8,
            path,
        })
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N], String> {
        let end = self
            .offset
            .checked_add(N)
            .ok_or_else(|| format!("prop asset size overflow in {}", self.path.display()))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| format!("truncated prop asset {}", self.path.display()))?;
        self.offset = end;
        Ok(bytes.try_into().expect("fixed-size byte range"))
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take()?))
    }

    fn f32(&mut self) -> Result<f32, String> {
        let value = f32::from_le_bytes(self.take()?);
        if !value.is_finite() {
            return Err(format!(
                "non-finite prop asset value in {}",
                self.path.display()
            ));
        }
        Ok(value)
    }

    fn finish(self) -> Result<(), String> {
        if self.offset != self.bytes.len() {
            return Err(format!(
                "trailing bytes in prop asset {}",
                self.path.display()
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Previous,
    Next,
    RotateLeft,
    RotateRight,
    Select,
    DeleteLast,
    Close,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MenuInput {
    pub dpad_up: bool,
    pub dpad_down: bool,
    pub dpad_left: bool,
    pub dpad_right: bool,
    pub keyboard_up: bool,
    pub keyboard_down: bool,
    pub keyboard_left: bool,
    pub keyboard_right: bool,
    pub confirm: bool,
    pub delete: bool,
    pub cancel: bool,
}

impl MenuInput {
    pub fn action(self) -> Option<MenuAction> {
        if self.cancel {
            Some(MenuAction::Close)
        } else if self.confirm {
            Some(MenuAction::Select)
        } else if self.delete {
            Some(MenuAction::DeleteLast)
        } else if self.dpad_up || self.keyboard_up {
            Some(MenuAction::Previous)
        } else if self.dpad_down || self.keyboard_down {
            Some(MenuAction::Next)
        } else if self.dpad_left || self.keyboard_left {
            Some(MenuAction::RotateLeft)
        } else if self.dpad_right || self.keyboard_right {
            Some(MenuAction::RotateRight)
        } else {
            None
        }
    }
}

/// In-memory dropper state. Placed props intentionally have no persistence.
pub struct DropperState {
    catalog: std::sync::Arc<PropCatalog>,
    selected: usize,
    yaw: f32,
    placed: Vec<PlacedProp>,
}

impl DropperState {
    pub fn new(catalog: impl Into<std::sync::Arc<PropCatalog>>) -> Result<Self, String> {
        let catalog = catalog.into();
        catalog.validate()?;
        Ok(Self {
            catalog,
            selected: 0,
            yaw: 0.,
            placed: Vec::new(),
        })
    }

    pub fn selected(&self) -> &PropDefinition {
        &self.catalog.props[self.selected]
    }

    pub fn placed(&self) -> &[PlacedProp] {
        &self.placed
    }

    pub fn preview(&self, position: Vec3) -> PlacedProp {
        PlacedProp {
            definition: self.selected,
            position,
            yaw: self.yaw,
        }
    }

    pub fn catalog(&self) -> &PropCatalog {
        &self.catalog
    }

    pub fn apply(&mut self, input: MenuInput, position: Vec3) -> Result<bool, String> {
        match input.action() {
            Some(MenuAction::Previous) => {
                self.selected =
                    (self.selected + self.catalog.props.len() - 1) % self.catalog.props.len();
            }
            Some(MenuAction::Next) => {
                self.selected = (self.selected + 1) % self.catalog.props.len();
            }
            Some(MenuAction::RotateLeft) => self.rotate(-std::f32::consts::FRAC_PI_4),
            Some(MenuAction::RotateRight) => self.rotate(std::f32::consts::FRAC_PI_4),
            Some(MenuAction::Select) => {
                if !position.is_finite() {
                    return Err("cannot place a prop at a non-finite position".into());
                }
                if self.placed.len() >= MAX_PLACED_PROPS {
                    return Err(format!("session prop limit reached ({MAX_PLACED_PROPS})"));
                }
                self.placed.push(PlacedProp {
                    definition: self.selected,
                    position,
                    yaw: self.yaw,
                });
            }
            Some(MenuAction::DeleteLast) => {
                self.placed.pop();
            }
            Some(MenuAction::Close) => return Ok(false),
            None => return Ok(true),
        }
        Ok(true)
    }

    fn rotate(&mut self, delta: f32) {
        self.yaw = (self.yaw + delta).rem_euclid(std::f32::consts::TAU);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> PropCatalog {
        PropCatalog {
            schema: 1,
            props: vec![PropDefinition {
                id: "quarter_pipe".into(),
                name: "Quarter pipe".into(),
                meshes: vec![PropMesh {
                    positions: vec![[0., 0., 0.], [1., 0., 0.], [0., 0., 1.]],
                    normals: vec![[0., 1., 0.]; 3],
                    uvs: vec![[0., 0.]; 3],
                    indices: vec![0, 1, 2],
                    colors: vec![],
                }],
                triangles: vec![[[0., 0., 0.], [1., 0., 0.], [0., 0., 1.]]],
                grind_rails: vec![],
            }],
        }
    }

    #[test]
    fn catalog_rejects_duplicate_ids_non_finite_and_degenerate_geometry() {
        let mut data = catalog();
        data.props.push(data.props[0].clone());
        assert!(data.validate().unwrap_err().contains("duplicate"));
        let mut data = catalog();
        data.props[0].triangles[0][0][0] = f32::NAN;
        assert!(data.validate().unwrap_err().contains("non-finite"));
        let mut data = catalog();
        data.props[0].triangles[0][2] = [2., 0., 0.];
        assert!(data.validate().unwrap_err().contains("degenerate"));
    }

    #[test]
    fn placement_rotates_and_translates_local_collision_triangles() {
        let catalog = catalog();
        let placed = PlacedProp {
            definition: 0,
            position: Vec3::new(4., 5., 6.),
            yaw: std::f32::consts::FRAC_PI_2,
        };
        let triangle = placed.collision_triangles(&catalog).unwrap()[0];
        assert!(Vec3::from_array(triangle[0]).distance(Vec3::new(4., 5., 6.)) < 1e-5);
        assert!(Vec3::from_array(triangle[1]).distance(Vec3::new(4., 5., 6. - 0.0254)) < 1e-5);
    }

    #[test]
    fn menu_accepts_dpad_and_keyboard_equivalents() {
        assert_eq!(
            MenuInput {
                dpad_up: true,
                ..Default::default()
            }
            .action(),
            Some(MenuAction::Previous)
        );
        assert_eq!(
            MenuInput {
                keyboard_up: true,
                ..Default::default()
            }
            .action(),
            Some(MenuAction::Previous)
        );
        assert_eq!(
            MenuInput {
                dpad_down: true,
                ..Default::default()
            }
            .action(),
            Some(MenuAction::Next)
        );
        assert_eq!(
            MenuInput {
                keyboard_down: true,
                ..Default::default()
            }
            .action(),
            Some(MenuAction::Next)
        );
        assert_eq!(
            MenuInput {
                dpad_left: true,
                ..Default::default()
            }
            .action(),
            Some(MenuAction::RotateLeft)
        );
        assert_eq!(
            MenuInput {
                keyboard_left: true,
                ..Default::default()
            }
            .action(),
            Some(MenuAction::RotateLeft)
        );
    }

    #[test]
    fn dropper_navigates_places_rotates_and_deletes_in_memory() {
        let mut catalog = catalog();
        catalog.props.push(PropDefinition {
            id: "ramp".into(),
            name: "Ramp".into(),
            meshes: catalog.props[0].meshes.clone(),
            triangles: catalog.props[0].triangles.clone(),
            grind_rails: vec![],
        });
        let mut dropper = DropperState::new(catalog).unwrap();
        dropper
            .apply(
                MenuInput {
                    keyboard_down: true,
                    ..Default::default()
                },
                Vec3::ZERO,
            )
            .unwrap();
        assert_eq!(dropper.selected().id, "ramp");
        dropper
            .apply(
                MenuInput {
                    dpad_right: true,
                    ..Default::default()
                },
                Vec3::ZERO,
            )
            .unwrap();
        dropper
            .apply(
                MenuInput {
                    confirm: true,
                    ..Default::default()
                },
                Vec3::new(1., 2., 3.),
            )
            .unwrap();
        assert_eq!(dropper.placed().len(), 1);
        assert!((dropper.placed()[0].yaw - std::f32::consts::FRAC_PI_4).abs() < 1e-5);
        dropper
            .apply(
                MenuInput {
                    delete: true,
                    ..Default::default()
                },
                Vec3::ZERO,
            )
            .unwrap();
        assert!(dropper.placed().is_empty());
    }
}
