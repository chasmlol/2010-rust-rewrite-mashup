//! Session-only prop geometry and placement primitives.
//!
//! The catalog format deliberately contains only local geometry. The game-data
//! converter must provide this data from the user's extracted game before it
//! can be presented as an in-game prop catalog.
use bevy::prelude::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path};

const MAX_PROPS: usize = 100_000;
const MAX_TRIANGLES_PER_PROP: usize = 1_000_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PropCatalog {
    pub schema: u32,
    pub props: Vec<PropDefinition>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PropDefinition {
    pub id: String,
    pub name: String,
    pub triangles: Vec<[[f32; 3]; 3]>,
}

impl PropCatalog {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path)
            .map_err(|e| format!("could not read prop catalog {}: {e}", path.display()))?;
        let catalog: Self = serde_json::from_slice(&bytes)
            .map_err(|e| format!("invalid prop catalog {}: {e}", path.display()))?;
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
                triangle.map(|p| transform.transform_point3(Vec3::from_array(p)).to_array())
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
    catalog: PropCatalog,
    selected: usize,
    yaw: f32,
    placed: Vec<PlacedProp>,
}

impl DropperState {
    pub fn new(catalog: PropCatalog) -> Result<Self, String> {
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
        assert!(Vec3::from_array(triangle[1]).distance(Vec3::new(4., 5., 5.)) < 1e-5);
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
