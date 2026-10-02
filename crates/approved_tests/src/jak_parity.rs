//! Jak Mode against Jak 3's own numbers: each expectation is a tuning value
//! or the closed form of a formula from the game's code, written out here
//! rather than read back from `jak_mode`, so a transcription slip fails.
use jak_mode::glam::Vec3;
use jak_mode::*;

const M: f32 = 4096.0;
const TICKS_PER_FRAME: i64 = 5;

fn quad(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> [Tri; 2] {
    [
        Tri {
            v: [a, b, c],
            pat: Pat::default(),
        },
        Tri {
            v: [a, c, d],
            pat: Pat::default(),
        },
    ]
}

/// A floor at y = 0, and a wall facing -z at `wall` meters when given.
fn ground(wall: Option<f32>) -> TriangleGrid {
    let s = 2000.0 * M;
    let mut tris = quad(
        Vec3::new(-s, 0.0, -s),
        Vec3::new(-s, 0.0, s),
        Vec3::new(s, 0.0, s),
        Vec3::new(s, 0.0, -s),
    )
    .to_vec();
    if let Some(z) = wall {
        let (z, h) = (z * M, 20.0 * M);
        tris.extend(quad(
            Vec3::new(-s, 0.0, z),
            Vec3::new(-s, h, z),
            Vec3::new(s, h, z),
            Vec3::new(s, 0.0, z),
        ));
    }
    TriangleGrid::new(tris, 4.0 * M)
}

fn idle() -> PadInput {
    PadInput::default()
}

fn press(b: u32) -> PadInput {
    PadInput {
        held: b,
        pressed: b,
        ..Default::default()
    }
}

fn hold(b: u32) -> PadInput {
    PadInput {
        held: b,
        ..Default::default()
    }
}

fn forward() -> PadInput {
    PadInput {
        left: [0.0, 1.0],
        ..Default::default()
    }
}

fn run(jak: &mut Jak, world: &mut TriangleGrid, input: PadInput, frames: usize) {
    for _ in 0..frames {
        jak.step(&input, world, None);
    }
}

fn standing(world: &mut TriangleGrid) -> Jak {
    let mut jak = Jak::new(Vec3::ZERO, 0.0);
    run(&mut jak, world, idle(), 60);
    assert_eq!(jak.state, State::Stance);
    jak
}

fn boarding(world: &mut TriangleGrid) -> Jak {
    let mut jak = standing(world);
    jak.request_board();
    run(&mut jak, world, idle(), 60);
    assert_eq!(jak.state, State::BoardStance);
    jak
}

/// Highest the root rises over where it took off, in meters, for a jump
/// whose button stays down for `held` frames after the press.
fn jump_apex(jak: &mut Jak, world: &mut TriangleGrid, held: usize) -> f32 {
    let base = jak.trans().y;
    jak.step(&press(button::X), world, None);
    let mut top = jak.trans().y;
    for frame in 0..240 {
        let input = if frame < held {
            hold(button::X)
        } else {
            idle()
        };
        jak.step(&input, world, None);
        top = top.max(jak.trans().y);
    }
    (top - base) / M
}

fn xz_speed(v: Vec3) -> f32 {
    (v.x * v.x + v.z * v.z).sqrt()
}

#[test]
fn units() {
    assert_eq!(math::meters(1.0), 4096.0);
    assert_eq!(math::seconds(1.0), 300);
    assert_eq!(math::degrees(360.0), 65536.0);
    assert_eq!(math::FRAME_TICKS, TICKS_PER_FRAME);
}

#[test]
fn board_surfaces_multiply_into_stone() {
    let walk = surface::Surface::mult(&surface::board::WALK, &surface::STONE);
    assert_eq!(walk.seek0, 0.5 * 153600.0);
    assert_eq!(walk.seek90, 0.5 * 153600.0);
    assert_eq!(walk.seek180, 0.5 * 256000.0);
    assert_eq!(walk.target_speed, 102400.0);
    assert_eq!(walk.transv_max, 143360.0);
}

/// Full forward reads 127/128 off the pad. Each grounded frame the board
/// adds `seek * lerp(0.4, 1, stick, 0.3, 1)` along its heading, then drags
/// by `1 - seek / target` of a frame, with seek 76800 and target 102400.
#[test]
fn board_thrust_follows_its_seek_curve() {
    let mut world = ground(None);
    let mut jak = boarding(&mut world);
    let seek_step = 0.5 * 153600.0 / 60.0;
    let drag = 1.0 - seek_step / 102400.0;
    let push = 0.4 + 0.6 * ((127.0 / 128.0 - 0.3) / 0.7);
    let mut speed = xz_speed(jak.velocity());
    run(&mut jak, &mut world, forward(), 1);
    speed = speed.max(xz_speed(jak.velocity()));
    for _ in 0..600 {
        jak.step(&forward(), &mut world, None);
        let expected = (speed + seek_step * push) * drag;
        speed = xz_speed(jak.velocity());
        assert!((speed - expected).abs() < 0.5, "{speed} vs {expected}");
    }
    let terminal = push * seek_step * drag / (1.0 - drag);
    assert!(
        (speed - terminal).abs() < 0.01 * terminal,
        "{speed} vs {terminal}"
    );
}

/// Left alone the board still pushes at 0.4 of its seek: it cruises.
#[test]
fn board_cruises_with_the_stick_released() {
    let mut world = ground(None);
    let mut jak = boarding(&mut world);
    run(&mut jak, &mut world, idle(), 900);
    let cruise = 0.4 * 102400.0 * (1.0 - 1280.0 / 102400.0);
    let speed = xz_speed(jak.velocity());
    assert!(
        (speed - cruise).abs() < 0.005 * cruise,
        "{speed} vs {cruise}"
    );
}

#[test]
fn board_jump_reaches_its_heights() {
    let mut world = ground(None);
    let mut jak = boarding(&mut world);
    let held = jump_apex(&mut jak, &mut world, 60);
    assert!((held - 3.5).abs() < 0.01, "{held}");
    run(&mut jak, &mut world, idle(), 60);
    let tapped = jump_apex(&mut jak, &mut world, 0);
    assert!((tapped - 1.01).abs() < 0.01, "{tapped}");
}

/// On foot the same heights, less the 0.7 m the jump raises the collision
/// by (and 0.1 m back at the low end).
#[test]
fn foot_jump_reaches_its_heights() {
    let mut world = ground(None);
    let mut jak = standing(&mut world);
    let held = jump_apex(&mut jak, &mut world, 60);
    assert!((held - (3.5 - 0.7)).abs() < 0.01, "{held}");
    run(&mut jak, &mut world, idle(), 60);
    let tapped = jump_apex(&mut jak, &mut world, 0);
    assert!((tapped - (1.01 - 0.6)).abs() < 0.01, "{tapped}");
}

/// The hop onto the board is timed to land 0.66 s after it starts.
#[test]
fn board_get_on_lands_after_its_hop() {
    let mut world = ground(None);
    let mut jak = standing(&mut world);
    jak.request_board();
    jak.step(&idle(), &mut world, None);
    assert_eq!(jak.state, State::BoardGetOn);
    let start = jak.time;
    while jak.state == State::BoardGetOn && jak.time - start < 600 {
        jak.step(&idle(), &mut world, None);
    }
    let took = jak.time - start;
    assert!(
        (took - math::seconds(0.66)).abs() <= 2 * TICKS_PER_FRAME,
        "{took}"
    );
}

fn fire_times(jak: &mut Jak, world: &mut TriangleGrid, frames: usize) -> Vec<i64> {
    let mut times = Vec::new();
    for _ in 0..frames {
        jak.step(&press(button::R1), world, None);
        for event in jak.take_events() {
            if let Event::Fire { .. } = event {
                times.push(jak.time);
            }
        }
    }
    times
}

/// The Blaster is up 0.1 s after the first press and fires then; held
/// presses fire again every 96 ticks, on the first frame past them.
#[test]
fn blaster_fires_on_its_delay() {
    let mut world = ground(None);
    let mut jak = standing(&mut world);
    let pressed = jak.time + TICKS_PER_FRAME;
    let times = fire_times(&mut jak, &mut world, 120);
    assert_eq!(times.first().copied(), Some(pressed + math::seconds(0.1)));
    let frame_after_delay = (96 + TICKS_PER_FRAME - 1) / TICKS_PER_FRAME * TICKS_PER_FRAME;
    assert!(times.len() >= 5);
    for pair in times.windows(2) {
        assert_eq!(pair[1] - pair[0], frame_after_delay);
    }
}

/// 200 m/s, gone after 3 s with nothing in the way.
#[test]
fn yellow_shot_flies_and_times_out() {
    let mut world = ground(None);
    let mut jak = standing(&mut world);
    jak.step(&press(button::R1), &mut world, None);
    while jak.projectiles.is_empty() {
        jak.step(&idle(), &mut world, None);
    }
    let spawned = jak.projectiles[0].spawn_time;
    let mut last = jak.projectiles[0].trans;
    let step = 819200.0 / 60.0;
    while !jak.projectiles.is_empty() {
        jak.step(&idle(), &mut world, None);
        if let Some(shot) = jak.projectiles.first() {
            assert!((shot.trans.distance(last) - step).abs() < 1.0);
            assert!(shot.tail.distance(shot.trans) <= 16.0 * M + 1.0);
            last = shot.trans;
        }
    }
    assert_eq!(jak.time - spawned, math::seconds(3.0));
}

/// The shot stops on the wall it meets and settles half a meter back along
/// its tail, its 0.2 m sphere touching the wall.
#[test]
fn yellow_shot_stops_on_a_wall() {
    let wall = 30.0;
    let mut world = ground(Some(wall));
    let mut jak = standing(&mut world);
    let mut hit = None;
    for frame in 0..120 {
        let input = if frame == 0 {
            press(button::R1)
        } else {
            idle()
        };
        jak.step(&input, &mut world, None);
        for event in jak.take_events() {
            if let Event::Impact(h) = event {
                hit = Some(h);
            }
        }
        if hit.is_some() {
            break;
        }
    }
    let hit = hit.expect("the shot reaches the wall");
    assert!((hit.surface.z - wall * M).abs() < 1.0);
    assert!(hit.normal.distance(Vec3::NEG_Z) < 1e-4);
    assert!(
        (hit.pos.z - (wall * M - 819.2 - 2048.0)).abs() < 1.0,
        "{}",
        hit.pos.z
    );
    assert_eq!(hit.damage, 2.0);
    assert_eq!(hit.actor, None);
}

/// A wall inside the muzzle's reach takes the shot on the frame it fires.
#[test]
fn yellow_shot_point_blank() {
    let mut world = ground(Some(2.0));
    let mut jak = standing(&mut world);
    let mut fired = None;
    let mut struck = None;
    for _ in 0..60 {
        jak.step(&press(button::R1), &mut world, None);
        for event in jak.take_events() {
            match event {
                Event::Fire { .. } => fired = fired.or(Some(jak.time)),
                Event::Impact(_) => struck = struck.or(Some(jak.time)),
                _ => {}
            }
        }
        if struck.is_some() {
            break;
        }
    }
    assert!(fired.is_some());
    assert_eq!(fired, struck);
    assert!(jak.projectiles.is_empty());
}

/// A sphere swept into a face meets it one radius out; past an edge it meets
/// the edge; a solid sweep passes through a face from behind.
#[test]
fn sphere_sweeps_meet_faces_edges_and_skip_backs() {
    let floor = quad(
        Vec3::new(-M, 0.0, -M),
        Vec3::new(-M, 0.0, M),
        Vec3::new(M, 0.0, M),
        Vec3::new(M, 0.0, -M),
    );
    let mut world = TriangleGrid::new(floor.to_vec(), 4.0 * M);
    let mut cache = CollideCache::default();
    let r = 0.7 * M;
    let start = Vec3::new(0.0, 2.0 * M, 0.0);
    let down = Vec3::new(0.0, -3.0 * M, 0.0);
    cache.fill_line_sphere(&mut world, start, down, r);
    let face = cache.probe_line_sphere(start, down, r).expect("face");
    assert!((face.u - (2.0 * M - r) / (3.0 * M)).abs() < 1e-5);
    assert!(face.normal.distance(Vec3::Y) < 1e-6);
    assert!(face.intersect.y.abs() < 1e-3);

    let beside = Vec3::new(M + 0.5 * r, 2.0 * M, 0.0);
    cache.fill_line_sphere(&mut world, beside, down, r);
    let edge = cache.probe_line_sphere(beside, down, r).expect("edge");
    let drop = 2.0 * M - (r * r - 0.25 * r * r).sqrt();
    assert!((edge.u - drop / (3.0 * M)).abs() < 1e-4, "{}", edge.u);
    assert!((edge.intersect.x - M).abs() < 1e-2);

    let below = Vec3::new(0.0, -2.0 * M, 0.0);
    let up = Vec3::new(0.0, 3.0 * M, 0.0);
    cache.fill_line_sphere(&mut world, below, up, r);
    assert!(cache.probe_line_sphere(below, up, r).is_none());
}

/// A turn is limited to the rate's share of a frame, or 5/frames of what is
/// left when that is less.
#[test]
fn smooth_rotation_limits_the_turn() {
    let from = Vec3::Z;
    let to = Vec3::X;
    let rate = math::degrees(180.0);
    let q = math::smooth_rotation(from, to, rate, 30, Vec3::Y);
    let turned = math::to_radians(rate / 60.0);
    assert!(((q * from).angle_between(from) - turned).abs() < 1e-4);
    let q = math::smooth_rotation(from, to, math::degrees(3600.0), 30, Vec3::Y);
    let share = std::f32::consts::FRAC_PI_2 * 5.0 / 30.0;
    assert!(((q * from).angle_between(from) - share).abs() < 1e-4);
}

/// Riding along a wall at a shallow angle glances off it: the board turns
/// to run along the wall and never passes it.
#[test]
fn board_glances_off_a_wall() {
    let wall = 30.0;
    let mut world = ground(Some(wall));
    let mut jak = boarding(&mut world);
    let slant = PadInput {
        left: [-0.3, 0.95],
        ..Default::default()
    };
    let mut glanced = false;
    for _ in 0..600 {
        jak.step(&slant, &mut world, None);
        glanced |= jak.take_events().contains(&Event::BoardGlance);
        assert!(jak.trans().z < wall * M);
    }
    assert!(glanced);
}
