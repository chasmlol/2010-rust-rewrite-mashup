//! Session-only prop geometry and placement primitives.
//!
//! The catalog format deliberately contains only local geometry. The game-data
//! converter must provide this data from the user's extracted game before it
//! can be presented as an in-game prop catalog.
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
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PropMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
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
            });
        }
        let catalog = Self { schema: 1, props };
        catalog.validate()?;
        Ok(catalog)
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
                    .any(|v| !v.is_finite())
                {
                    return Err(format!(
                        "prop {:?} contains non-finite render geometry",
                        prop.id
                    ));
                }
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
                        .transform_point3(Vec3::from_array(p) * 0.0254)
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
                }],
                triangles: vec![[[0., 0., 0.], [1., 0., 0.], [0., 0., 1.]]],
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
