use super::math::*;
use super::*;
use crate::math::Vector3;
fn settings() -> Settings {
    Settings {
        height: 0.9,
        sphere_radius: 0.3,
        start_index: 3,
    }
}
fn packet() -> Packet {
    let mut p = Packet::initialized(0.);
    p.position_32 = [0., 2., 0., 0.];
    p.velocity_0 = [0., 3., 4., 0.];
    p.forward_64 = [0., 0., 1., 0.];
    p.scalar_100 = 0.2;
    p
}
fn context() -> Context {
    Context {
        selection_flags_2948: 0,
        matching_group_2952: -1,
        up_544: UP,
        forward_224: [0., 0., 1., 0.],
    }
}
fn hit(frame: i32, position: Vector) -> QueryResult {
    QueryResult {
        contact_frame: frame,
        contact_time: frame as f32 * DT,
        contact_position: position,
        surface: 7 << 7,
        ..QueryResult::miss()
    }
}
fn launched() -> Selector {
    let mut s = Selector::default();
    s.begin_launch(packet(), [0., -9.8, 0., 0.], settings())
        .unwrap();
    s
}
fn selected() -> Selector {
    let mut s = launched();
    s.observe_launch(&[hit(60, [0., 0., 4., 0.])]).unwrap();
    s.select_launch(context(), s.candidates[0], None).unwrap();
    s
}
fn close(a: Vector, b: Vector) {
    for i in 0..4 {
        assert!((a[i] - b[i]).abs() < 0.0001, "{a:?} != {b:?}");
    }
}
#[test]
fn constructor_reset_and_exit_have_distinct_native_effects() {
    let mut s = Selector::default();
    assert!(!s.sampling.restart_allowed_8493);
    assert_eq!(s.sampling.selection.landing_frame_8480, 0);
    s.launch = packet();
    s.reset();
    assert_eq!(s.launch, packet());
    assert!(s.sampling.restart_allowed_8493);
    assert_eq!(s.sampling.selection.landing_frame_8480, 1000);
    let mut s = selected();
    let before = s.predictions.clone();
    let candidate = s.selected_candidate.trajectory;
    s.exit();
    assert!(!s.sampling.pending_8492 && !s.sampling.preinitialized_8494);
    assert_eq!(s.predictions, before);
    assert_eq!(s.selected_candidate.trajectory, candidate);
}
#[test]
fn launch_requests_are_native_valid_and_index_shifted() {
    let s = launched();
    let p = packet();
    let trajectory = s.candidates[0].trajectory;
    let original = Trajectory {
        position: add(p.position_32, s.offset_8272),
        velocity: sub(p.velocity_0, [0., 0.1, 0., 0.]),
        acceleration: [0., -9.8, 0., 0.],
        duration: 2.,
    };
    close(trajectory.position, original.position_at(3. * DT));
    close(trajectory.velocity, original.velocity_at(3. * DT));
    let q = s.predictions[0].request;
    assert_eq!(
        (q.trajectory.duration, q.radius, q.start_error, q.end_error),
        (2., 0.3, 0.25, 1.2)
    );
    assert!(validate_request(q).is_ok());
}
#[test]
fn invalid_authoring_is_not_capped_or_padded() {
    let mut s = Selector::default();
    let mut p = packet();
    p.kind_108 = 17;
    assert!(s.begin_launch(p, [0., -9.8, 0., 0.], settings()).is_err());
    p.kind_108 = 0;
    assert!(s.begin_launch(p, [0., -9.8, 0., 0.], settings()).is_err());
    let mut bad = settings();
    bad.sphere_radius = 0.;
    assert!(s.begin_launch(packet(), [0., -9.8, 0., 0.], bad).is_err());
    assert!(s.candidates.is_empty());
}
#[test]
fn pending_sample_uses_original_fallback_without_consuming() {
    let mut s = launched();
    let fallback = s.sampling.fallback_8208;
    let graph = PointGraph {
        x: [0.; 8],
        y: [1.; 8],
    };
    let mut out = TrajectoryResult::default();
    s.sample(10, DT, &graph, &mut out);
    close(out.position_272, fallback.position_at(10. * DT));
    assert!(!out.valid_404);
    assert_eq!(out.normal_304, UP);
    assert!(s.sampling.pending_8492);
    assert_eq!(out.scalar_392, packet().scalar_100);
}
#[test]
fn scoring_keeps_first_tie_and_secondary_low_height_penalty() {
    let mut p = packet();
    p.kind_108 = 2;
    p.kind_112 = 1;
    p.secondary_velocity_16 = [0., 0., 100., 0.];
    let mut s = Selector::default();
    s.begin_launch(p, [0., -9.8, 0., 0.], settings()).unwrap();
    let results: Vec<_> = s
        .candidates
        .iter()
        .map(|c| hit(60, add(c.trajectory.position, [0., 0.1, 2., 0.])))
        .collect();
    s.observe_launch(&results).unwrap();
    s.select_launch(context(), s.candidates[0], None).unwrap();
    assert_eq!(s.selected_index, Some(0));
    assert!(s.scores[2] < -9000.);
    assert_eq!(recovered::selection::selected_index(&[2., 2., 1.]), Some(0));
}
#[test]
fn selection_preserves_special_surface_and_unshifted_candidate() {
    let mut s = launched();
    s.observe_launch(&[hit(60, [0., 0., 4., 0.])]).unwrap();
    let first = s.candidates[0];
    let edge = crate::player::offboard::ground_query::Edge {
        start: Vector3::new(-1., 0., 4.),
        end: Vector3::new(1., 0., 4.),
    };
    let adjustment = ledge::Adjustment {
        edge,
        point: [0., 0., 4., 0.],
        lowered_trajectory: first.trajectory,
        landing_frame: 55,
        radius: 0.3,
    };
    s.select_launch(context(), first, Some(adjustment)).unwrap();
    assert_eq!(s.sampling.selection.word_8396, 2);
    assert_eq!(s.predictions[0].result.contact_frame, 63);
    assert_eq!(s.sampling.selection.landing_frame_8480, 55); //S3, not S2 index sum.
    assert_eq!(
        s.selected_candidate.trajectory.velocity,
        first.trajectory.velocity
    );
    assert_ne!(
        s.sampling.selection.trajectory_8144.position,
        s.selected_candidate.trajectory.position
    );
}
#[test]
fn animation_adjusts_sample_only_and_preserves_current_point() {
    let mut s = selected();
    let arc = s.sampling.selection.trajectory_8144;
    let candidate = s.selected_candidate.trajectory;
    let prediction = s.predictions[0];
    let frame = 10;
    let before = arc.position_at(frame as f32 * DT);
    let axes = [[1., 0., 0., 0.], UP, [0., 0., 1., 0.], [0.; 4]];
    s.adjust_animation(frame, [0., 0.9, 0.3, 0.], axes, 0.3);
    close(
        s.sampling
            .selection
            .trajectory_8144
            .position_at(frame as f32 * DT),
        before,
    );
    assert_eq!(s.selected_candidate.trajectory, candidate);
    assert_eq!(s.predictions[0], prediction);
}
#[test]
fn animation_correction_retains_native_fused_up_plus_forward_operation() {
    let mut s = selected();
    //Finite orthogonal axes deliberately exercise cancellation. This is an
    //arithmetic regression for82D6DF40, not a stock gameplay tuning fixture.
    let axes = [[0., 0., 1., 0.], [0.8, 0.6, 0., 0.],
        [0.6, -0.8, 0., 0.], [0.; 4]];
    s.adjust_animation(10, [0., 0.6, -0.8, 0.], axes, 0.);
    let forward = 0.6_f32 * -0.8;
    let expected = -0.8_f32.mul_add(0.6, forward);
    assert_ne!(expected, -(0.8_f32 * 0.6 + forward));
    assert_eq!(s.correction_8304[0].to_bits(), expected.to_bits());
}

#[test]
fn requery_gates_first_hit_changed_hit_and_miss_retention() {
    let mut s = selected();
    let old = s.sampling.selection;
    assert!(s.begin_requery(0x10000000, 0, 0.3).unwrap().is_none());
    assert!(s.sampling.restart_allowed_8493);
    let request = s.begin_requery(0, 0, 0.3).unwrap().unwrap();
    assert_eq!(
        request.trajectory.position,
        s.selected_candidate.trajectory.position
    );
    s.complete_requery(Prediction {
        request,
        result: hit(41, [3., 2., 1., 0.]),
    })
    .unwrap();
    assert_eq!(s.requery_count_8488, 1);
    assert_eq!(s.sampling.selection.position_6176, old.position_6176);
    s.sampling.restart_allowed_8493 = true;
    let request = s.begin_requery(0, 0, 0.3).unwrap().unwrap();
    s.complete_requery(Prediction {
        request,
        result: hit(42, [4., 2., 1., 0.]),
    })
    .unwrap();
    assert_eq!(s.sampling.selection.landing_frame_8480, 42);
    assert_eq!(s.sampling.selection.trajectory_8144, old.trajectory_8144);
    let retained = s.sampling.selection;
    s.sampling.restart_allowed_8493 = true;
    let request = s.begin_requery(0, 0, 0.3).unwrap().unwrap();
    s.complete_requery(Prediction {
        request,
        result: QueryResult::miss(),
    })
    .unwrap();
    assert_eq!(s.requery_count_8488, 2);
    assert!(!s.requery_pending_8499);
    assert_eq!(s.sampling.selection.position_6176, retained.position_6176);
}
#[test]
fn greatest_plane_root_is_not_limited_to_query_duration() {
    let t = Trajectory {
        position: [0., 1., 0., 0.],
        velocity: [0., 2., 0., 0.],
        acceleration: [0., -2., 0., 0.],
        duration: 0.1,
    };
    let time = ledge::plane_time(t, [0.; 4], UP).unwrap();
    assert!((time - (1. + 2f32.sqrt())).abs() < 0.0001);
    let tangent = Trajectory {
        position: [0., -1., 0., 0.],
        ..t
    };
    assert!((ledge::plane_time(tangent, [0.; 4], UP).unwrap() - 1.).abs() < 0.0001);
}
#[test]
fn air_six_lines_consume_even_when_all_miss() {
    let edge = crate::player::offboard::ground_query::Edge {
        start: Vector3::new(-1., 0., 0.),
        end: Vector3::new(1., 0., 0.),
    };
    let adjustment = ledge::Adjustment {
        edge,
        point: [0.; 4],
        lowered_trajectory: selected().selected_candidate.trajectory,
        landing_frame: 60,
        radius: 0.3,
    };
    let lines = ledge::lines(adjustment, 0.4).unwrap();
    assert_eq!(lines.len(), 6);
    assert_eq!(lines[5].radius, 0.001);
    assert!(ledge::consume_lines(&[None; 6]).is_ok());
    assert!(ledge::consume_lines(&[None; 7]).is_err());
}
#[test]
fn nonfinite_launch_packet_names_the_offending_field() {
    let gravity = [0., -9.8, 0., 0.];
    let cases: [(&str, fn(&mut Packet)); 4] = [
        ("scalar_100", |p| p.scalar_100 = f32::NAN),
        ("velocity_0", |p| p.velocity_0[1] = f32::NAN),
        ("position_32", |p| p.position_32[2] = f32::INFINITY),
        ("forward_64", |p| p.forward_64[0] = f32::NAN),
    ];
    for (field, corrupt) in cases {
        let mut s = Selector::default();
        let mut p = packet();
        corrupt(&mut p);
        let error = s.begin_launch(p, gravity, settings()).unwrap_err();
        assert!(
            error.contains(field),
            "error should name {field}, got {error:?}"
        );
    }
}
