use bevy::prelude::Vec3;
use skate_host::object_dropper::{
    DropperState, MenuAction, MenuInput, PlacedProp, PropCatalog, PropDefinition,
};

fn catalog() -> PropCatalog {
    PropCatalog {
        schema: 1,
        props: vec![
            PropDefinition {
                id: "quarter_pipe".into(),
                name: "Quarter pipe".into(),
                triangles: vec![[[0., 0., 0.], [1., 0., 0.], [0., 0., 1.]]],
            },
            PropDefinition {
                id: "ramp".into(),
                name: "Ramp".into(),
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
    assert!(Vec3::from_array(triangle[1]).distance(Vec3::new(4., 5., 5.)) < 1e-5);

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
