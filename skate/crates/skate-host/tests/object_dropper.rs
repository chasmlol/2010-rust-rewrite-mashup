use bevy::prelude::Vec3;
use skate_host::object_dropper::{
    DropperState, MenuAction, MenuInput, PlacedProp, PropCatalog, PropDefinition,
};
use std::path::Path;

fn catalog() -> PropCatalog {
    PropCatalog {
        schema: 1,
        props: vec![
            PropDefinition {
                id: "quarter_pipe".into(),
                name: "Quarter pipe".into(),
                meshes: vec![skate_host::object_dropper::PropMesh {
                    positions: vec![[0., 0., 0.], [1., 0., 0.], [0., 0., 1.]],
                    normals: vec![[0., 1., 0.]; 3],
                    uvs: vec![[0., 0.]; 3],
                    indices: vec![0, 1, 2],
                }],
                triangles: vec![[[0., 0., 0.], [1., 0., 0.], [0., 0., 1.]]],
            },
            PropDefinition {
                id: "ramp".into(),
                name: "Ramp".into(),
                meshes: vec![skate_host::object_dropper::PropMesh {
                    positions: vec![[0., 0., 0.], [1., 0., 0.], [0., 0., 1.]],
                    normals: vec![[0., 1., 0.]; 3],
                    uvs: vec![[0., 0.]; 3],
                    indices: vec![0, 1, 2],
                }],
                triangles: vec![[[0., 0., 0.], [1., 0., 0.], [0., 0., 1.]]],
            },
        ],
    }
}

#[test]
fn validates_catalogs_and_transforms_collision_geometry() {
    let catalog = catalog();
    catalog.validate().unwrap();
    let placed = PlacedProp {
        definition: 0,
        position: Vec3::new(4., 5., 6.),
        yaw: std::f32::consts::FRAC_PI_2,
    };
    let triangle = placed.collision_triangles(&catalog).unwrap()[0];
    assert!(Vec3::from_array(triangle[0]).distance(Vec3::new(4., 5., 6.)) < 1e-5);
    assert!(
        (Vec3::from_array(triangle[1]) - Vec3::new(4., 5., 6.))
            .distance(Vec3::new(0., 0., -0.0254))
            < 1e-5
    );

    let mut malformed = catalog;
    malformed.props[1].id = malformed.props[0].id.clone();
    assert!(malformed.validate().unwrap_err().contains("duplicate"));
}

#[test]
fn dpad_and_keyboard_menu_actions_place_rotate_and_delete_session_props() {
    let mut dropper = DropperState::new(catalog()).unwrap();
    for input in [
        MenuInput {
            keyboard_down: true,
            ..Default::default()
        },
        MenuInput {
            dpad_right: true,
            ..Default::default()
        },
        MenuInput {
            confirm: true,
            ..Default::default()
        },
    ] {
        assert!(dropper.apply(input, Vec3::new(1., 2., 3.)).unwrap());
    }

    assert_eq!(dropper.selected().id, "ramp");
    assert_eq!(dropper.placed().len(), 1);
    assert!((dropper.placed()[0].yaw - std::f32::consts::FRAC_PI_4).abs() < 1e-5);

    assert!(
        !dropper
            .apply(
                MenuInput {
                    cancel: true,
                    ..Default::default()
                },
                Vec3::ZERO,
            )
            .unwrap()
    );
    assert!(
        dropper
            .apply(
                MenuInput {
                    delete: true,
                    ..Default::default()
                },
                Vec3::ZERO,
            )
            .unwrap()
    );
    assert!(dropper.placed().is_empty());
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
}

#[test]
fn loads_converter_runtime_mesh_and_collision_files() {
    let root = std::env::temp_dir().join(format!(
        "iw4l-prop-catalog-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let models = root.join("models");
    let collision = root.join("collision");
    std::fs::create_dir_all(&models).unwrap();
    std::fs::create_dir_all(&collision).unwrap();

    let mut model = b"IW4LPM01".to_vec();
    model.extend_from_slice(&1_u32.to_le_bytes());
    model.extend_from_slice(&3_u32.to_le_bytes());
    model.extend_from_slice(&3_u32.to_le_bytes());
    let vertices: [[f32; 8]; 3] = [
        [0., 0., 0., 0., 1., 0., 0., 0.],
        [1., 0., 0., 0., 1., 0., 1., 0.],
        [0., 0., 1., 0., 1., 0., 0., 1.],
    ];
    for vertex in vertices {
        for value in vertex {
            model.extend_from_slice(&value.to_le_bytes());
        }
    }
    for index in [0_u32, 1, 2] {
        model.extend_from_slice(&index.to_le_bytes());
    }
    std::fs::write(models.join("ramp.mesh"), model).unwrap();

    let mut mesh_collision = b"IW4LPC01".to_vec();
    mesh_collision.extend_from_slice(&1_u32.to_le_bytes());
    let collision_values: [f32; 9] = [0., 0., 0., 1., 0., 0., 0., 0., 1.];
    for value in collision_values {
        mesh_collision.extend_from_slice(&value.to_le_bytes());
    }
    std::fs::write(collision.join("ramp.collision"), mesh_collision).unwrap();
    let manifest = serde_json::json!({
        "schema": 3,
        "props": [{
            "id": "ramp",
            "name": "Ramp",
            "model": "models/ramp.mesh",
            "collision": "collision/ramp.collision"
        }]
    });
    let path = root.join("catalog.json");
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();

    let catalog = PropCatalog::load(&path).unwrap();
    assert_eq!(catalog.props.len(), 1);
    assert_eq!(catalog.props[0].meshes[0].positions[1], [1., 0., 0.]);
    assert_eq!(catalog.props[0].triangles.len(), 1);
    catalog.validate().unwrap();

    let unsafe_manifest = serde_json::json!({
        "schema": 3,
        "props": [{
            "id": "ramp",
            "name": "Ramp",
            "model": "../outside.mesh",
            "collision": "collision/ramp.collision"
        }]
    });
    std::fs::write(&path, serde_json::to_vec(&unsafe_manifest).unwrap()).unwrap();
    assert!(
        PropCatalog::load(Path::new(&path))
            .unwrap_err()
            .contains("unsafe prop asset path")
    );
    std::fs::remove_dir_all(root).unwrap();
}
