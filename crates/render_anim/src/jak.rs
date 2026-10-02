//! Jak Mode: the local player handed to `jak_mode`'s Jak, who moves, rides
//! and shoots through the map's own collision or the Minecraft world's
//! blocks. Rendering and match ownership stay with IW4L.
use std::sync::{Arc, Mutex, mpsc};

use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use frame::{AppScreen, JakMode};
use jak_mode::glam as jg;
use jak_mode::{
    ActorWorld, Basis, CollideWorld, Event, Jak, PadInput, Pat, Tri, TriangleGrid, button,
};

/// Map units per Jak meter: Jak's body (three spheres, 2.8 m) stands as tall
/// as the 70-unit soldier he replaces.
pub const MAP_PER_METER: f32 = 25.0;
const TO_JAK: f32 = jak_mode::METER / MAP_PER_METER;
/// Jak's health is 8 and a soldier's 100: his damage in MW2's terms.
const DAMAGE_TO_MW2: f32 = 100.0 / 8.0;
const STEP: f32 = 1.0 / 60.0;
/// The map's triangles are bucketed on this grid, in Jak units.
const GRID_CELL: f32 = jak_mode::METER * 4.0;

pub fn to_jak(p: Vec3) -> jg::Vec3 {
    jg::Vec3::new(p.x, p.z, -p.y) * TO_JAK
}

pub fn from_jak(p: jg::Vec3) -> Vec3 {
    Vec3::new(p.x, -p.z, p.y) / TO_JAK
}

fn dir_from_jak(v: jg::Vec3) -> Vec3 {
    Vec3::new(v.x, -v.z, v.y)
}

fn dir_to_jak(v: Vec3) -> jg::Vec3 {
    jg::Vec3::new(v.x, v.z, -v.y)
}

/// The Minecraft world's blocks, read straight from the block grid for
/// each box the movement asks about.
struct BlockWorld {
    scratch: Vec<[[f32; 3]; 3]>,
}

impl CollideWorld for BlockWorld {
    fn fill(&mut self, min: jg::Vec3, max: jg::Vec3, out: &mut Vec<Tri>) {
        let (a, b) = (from_jak(min), from_jak(max));
        self.scratch.clear();
        sim::voxel::collision_triangles_in(
            a.min(b).to_array(),
            a.max(b).to_array(),
            &mut self.scratch,
        );
        out.extend(self.scratch.iter().map(|t| Tri {
            v: t.map(|p| to_jak(Vec3::from_array(p))),
            pat: Pat::default(),
        }));
    }
}

/// The Minecraft world's mobs, as targets for a shot.
struct Mobs {
    boxes: Vec<(u64, jg::Vec3, jg::Vec3)>,
}

impl Mobs {
    fn gather() -> Self {
        let boxes = sim::voxel::mob_targets()
            .into_iter()
            .map(|(key, mins, maxs)| {
                let (a, b) = (
                    to_jak(Vec3::from_array(mins)),
                    to_jak(Vec3::from_array(maxs)),
                );
                (key, a.min(b), a.max(b))
            })
            .collect();
        Self { boxes }
    }

    /// The nearest mob box along the aim, for the gun to track.
    fn nearest_in_front(&self, from: jg::Vec3, forward: jg::Vec3, range: f32) -> Option<jg::Vec3> {
        self.boxes
            .iter()
            .map(|(_, lo, hi)| (*lo + *hi) * 0.5)
            .filter(|c| {
                let d = *c - from;
                d.length() <= range && d.normalize_or_zero().dot(forward) > 0.7
            })
            .min_by(|a, b| a.distance(from).total_cmp(&b.distance(from)))
    }
}

impl ActorWorld for Mobs {
    fn actor_hit(
        &mut self,
        start: jg::Vec3,
        motion: jg::Vec3,
        radius: f32,
    ) -> Option<(u64, f32, jg::Vec3)> {
        let mut best: Option<(u64, f32, jg::Vec3)> = None;
        for &(key, lo, hi) in &self.boxes {
            let (lo, hi) = (lo - jg::Vec3::splat(radius), hi + jg::Vec3::splat(radius));
            let (mut t0, mut t1) = (0.0f32, 1.0f32);
            let mut hit = true;
            for k in 0..3 {
                let (s, d) = (start[k], motion[k]);
                if d.abs() < 1e-9 {
                    if s < lo[k] || s > hi[k] {
                        hit = false;
                        break;
                    }
                    continue;
                }
                let (u, v) = ((lo[k] - s) / d, (hi[k] - s) / d);
                t0 = t0.max(u.min(v));
                t1 = t1.min(u.max(v));
                if t0 > t1 {
                    hit = false;
                    break;
                }
            }
            if hit && best.is_none_or(|(_, t, _)| t0 < t) {
                best = Some((key, t0, start + motion * t0));
            }
        }
        best
    }
}

enum Collision {
    None,
    Building(Mutex<mpsc::Receiver<TriangleGrid>>),
    Map(Box<TriangleGrid>),
}

/// The follow camera's orbit, in degrees, and how long it has been left
/// alone.
struct Camera {
    yaw: f32,
    pitch: f32,
    idle: f32,
}

#[derive(Resource)]
struct Host {
    jak: Option<Jak>,
    collision: Collision,
    clip: Option<Arc<asset_world::ClipCollision>>,
    accumulated: f32,
    latched: u32,
    held_last: u32,
    camera: Camera,
    starting: bool,
    animator: crate::jak_pose::Animator,
}

impl Default for Host {
    fn default() -> Self {
        Self {
            jak: None,
            collision: Collision::None,
            clip: None,
            accumulated: 0.0,
            latched: 0,
            held_last: 0,
            camera: Camera {
                yaw: 0.0,
                pitch: -12.0,
                idle: 0.0,
            },
            starting: false,
            animator: crate::jak_pose::Animator::new(),
        }
    }
}

pub fn register(app: &mut App) {
    app.init_resource::<JakMode>()
        .init_resource::<Host>()
        .add_systems(
            Update,
            update
                .after(frame::PresentedPublished)
                .before(crate::sync_camera_from_presented)
                .before(render_scene::GfxSceneAdd)
                .in_set(frame::ClientSet::Present),
        );
}

/// The map's collision as Jak-space triangles, bucketed for queries.
fn build_map_grid(clip: Arc<asset_world::ClipCollision>) -> mpsc::Receiver<TriangleGrid> {
    let (send, receive) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("iw4l-jak-collision".into())
        .spawn(move || {
            let start = std::time::Instant::now();
            let tris: Vec<Tri> = crate::clip_triangles::map_triangles(&clip)
                .into_iter()
                .map(|t| Tri {
                    v: t.map(to_jak),
                    pat: Pat::default(),
                })
                .collect();
            let n = tris.len();
            let grid = TriangleGrid::new(tris, GRID_CELL);
            diag::info!(
                World,
                "Jak collision: {n} triangles in {}ms",
                start.elapsed().as_millis()
            );
            let _ = send.send(grid);
        });
    if let Err(e) = spawned {
        diag::warn!(World, "Jak collision thread: {e}");
    }
    receive
}

fn stop(host: &mut Host, mode: &mut JakMode, authority: &mut net::AuthorityWorld) {
    authority
        .0
        .set_external_motion(sim::ClientId(mode.client), false);
    host.jak = None;
    host.starting = false;
    mode.active = false;
    mode.camera = None;
    mode.board = None;
    mode.skins = None;
    mode.shots.clear();
    mode.state.clear();
}

/// Keyboard, mouse and controller, in the PS2 pad's layout the gameplay
/// reads: A/Space jump (X), B/E zap (circle), X/C square, Y/Q triangle,
/// LB/Ctrl duck (L1), LT/Shift L2, RB/left mouse fire (R1), RT/F board (R2).
fn read_input(
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    pad: Option<&Gamepad>,
) -> (u32, [f32; 2], [f32; 2]) {
    use GamepadButton as B;
    let mut held = 0u32;
    let mut set = |down: bool, bit: u32| {
        if down {
            held |= bit;
        }
    };
    set(keys.pressed(KeyCode::Space), button::X);
    set(keys.pressed(KeyCode::KeyE), button::CIRCLE);
    set(keys.pressed(KeyCode::KeyC), button::SQUARE);
    set(keys.pressed(KeyCode::KeyQ), button::TRIANGLE);
    set(keys.pressed(KeyCode::ControlLeft), button::L1);
    set(keys.pressed(KeyCode::ShiftLeft), button::L2);
    set(mouse.pressed(MouseButton::Left), button::R1);
    set(
        keys.pressed(KeyCode::KeyF) || mouse.pressed(MouseButton::Right),
        button::R2,
    );
    let mut left = [0.0f32; 2];
    if keys.pressed(KeyCode::KeyW) {
        left[1] += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        left[1] -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        left[0] += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        left[0] -= 1.0;
    }
    let mut right = [0.0f32; 2];
    if let Some(pad) = pad {
        set(pad.pressed(B::South), button::X);
        set(pad.pressed(B::East), button::CIRCLE);
        set(pad.pressed(B::West), button::SQUARE);
        set(pad.pressed(B::North), button::TRIANGLE);
        set(pad.pressed(B::LeftTrigger), button::L1);
        set(pad.pressed(B::LeftTrigger2), button::L2);
        set(pad.pressed(B::RightTrigger), button::R1);
        set(pad.pressed(B::RightTrigger2), button::R2);
        set(pad.pressed(B::LeftThumb), button::L3);
        set(pad.pressed(B::RightThumb), button::R3);
        let (l, r) = (pad.left_stick(), pad.right_stick());
        if l.length() > left[0].hypot(left[1]) {
            left = [l.x, l.y];
        }
        right = [r.x, r.y];
    }
    (held, left, right)
}

/// The follow camera: orbits Jak by stick or mouse, drifts behind him when
/// left alone. Returns the view and the frame the stick is read through.
fn follow_camera(camera: &mut Camera, jak: &Jak, look: Vec2, dt: f32) -> (Transform, Basis) {
    let target = from_jak(jak.trans()) + Vec3::Z * 1.6 * MAP_PER_METER;
    if look.length_squared() > 0.0 {
        camera.yaw -= look.x;
        camera.pitch = (camera.pitch - look.y).clamp(-70.0, 45.0);
        camera.idle = 0.0;
    } else {
        camera.idle += dt;
    }
    let travel = dir_from_jak(jak.velocity());
    if camera.idle > 0.6 && travel.truncate().length() > 4.0 * MAP_PER_METER {
        let behind = travel.y.atan2(travel.x).to_degrees();
        let diff = (behind - camera.yaw + 540.0).rem_euclid(360.0) - 180.0;
        camera.yaw += diff * (1.5 * dt).min(1.0);
    }
    let (yaw, pitch) = (camera.yaw.to_radians(), camera.pitch.to_radians());
    let forward = Vec3::new(
        pitch.cos() * yaw.cos(),
        pitch.cos() * yaw.sin(),
        pitch.sin(),
    );
    let eye = target - forward * 6.0 * MAP_PER_METER;
    let view = Transform::from_translation(eye).looking_to(forward, Vec3::Z);
    let f = dir_to_jak(*view.forward()).normalize_or(jg::Vec3::Z);
    let u = dir_to_jak(*view.up()).normalize_or(jg::Vec3::Y);
    (
        view,
        Basis {
            r: u.cross(f).normalize_or_zero(),
            u,
            f,
        },
    )
}

/// Jak's frame in map space, laid out the way the soldier body faces: +x
/// ahead, +y to his left, +z up.
fn body_matrix(jak: &Jak, lift: f32) -> Mat4 {
    let q = jak.orientation();
    let f = dir_from_jak(q * jg::Vec3::Z).normalize_or(Vec3::X);
    let l = dir_from_jak(q * jg::Vec3::X).normalize_or(Vec3::Y);
    let u = dir_from_jak(q * jg::Vec3::Y).normalize_or(Vec3::Z);
    let at = from_jak(jak.trans()) + u * lift;
    Mat4::from_cols(f.extend(0.0), l.extend(0.0), u.extend(0.0), at.extend(1.0))
}

#[allow(clippy::too_many_arguments)]
fn update(
    time: Res<Time>,
    screen: Res<AppScreen>,
    local: Res<net::LocalPresentClient>,
    presented: Res<net::PresentedSnapshot>,
    clip: Res<crate::DynEntPhysClip>,
    mut authority: Option<ResMut<net::AuthorityWorld>>,
    mut mode: ResMut<JakMode>,
    mut host: ResMut<Host>,
    skate: Res<frame::SkateMode>,
    (keys, mouse, mut motion): (
        Res<ButtonInput<KeyCode>>,
        Res<ButtonInput<MouseButton>>,
        MessageReader<MouseMotion>,
    ),
    (gamepads, active): (Query<&Gamepad>, Option<Res<frame::ActivePad>>),
) {
    let Some(authority) = authority.as_deref_mut() else {
        return;
    };
    let host = &mut *host;
    let ps = presented.player(local.0);
    let alive = ps.is_some_and(|p| p.pm_type == 0) && *screen == AppScreen::InGame;
    let same_map = host
        .clip
        .as_ref()
        .is_none_or(|a| clip.0.as_ref().is_some_and(|b| Arc::ptr_eq(a, b)));
    if !same_map {
        if mode.active {
            stop(host, &mut mode, authority);
        }
        host.collision = Collision::None;
        host.clip = None;
    }
    if mode.active && !alive {
        stop(host, &mut mode, authority);
    }
    let toggle = std::mem::take(&mut mode.toggle_requested);
    if toggle && alive {
        if mode.active || host.starting {
            stop(host, &mut mode, authority);
            diag::info!(World, "Jak Mode stopped");
            return;
        }
        if skate.active || skate.entering {
            mode.status = "leave Skate first".into();
            return;
        }
        host.starting = true;
        mode.client = local.0.0;
        mode.status.clear();
    }
    if host.starting && !sim::voxel::active() && matches!(host.collision, Collision::None) {
        match clip.0.clone() {
            Some(geometry) => {
                host.clip = Some(geometry.clone());
                host.collision = Collision::Building(Mutex::new(build_map_grid(geometry)));
                mode.status = "building Jak's collision for this map".into();
            }
            None => {
                host.starting = false;
                mode.status = "this map has no collision loaded".into();
            }
        }
    }
    if let Collision::Building(receive) = &host.collision
        && let Ok(grid) = receive
            .lock()
            .map_err(|_| ())
            .and_then(|r| r.try_recv().map_err(|_| ()))
    {
        host.collision = Collision::Map(Box::new(grid));
        mode.status.clear();
    }
    let ready = sim::voxel::active() || matches!(host.collision, Collision::Map(_));
    if host.starting
        && ready
        && let Some(ps) = ps.filter(|_| alive)
    {
        host.starting = false;
        let origin = Vec3::from_array(ps.origin);
        let yaw = ps.viewangles[1].to_radians();
        let facing = dir_to_jak(Vec3::new(yaw.cos(), yaw.sin(), 0.0));
        let mut jak = Jak::new(to_jak(origin), jak_mode::math::y_angle(facing));
        jak.gun.endless_ammo = true;
        host.camera.yaw = ps.viewangles[1];
        host.camera.pitch = -12.0;
        host.accumulated = 0.0;
        host.latched = 0;
        host.held_last = 0;
        host.jak = Some(jak);
        mode.active = true;
        authority.0.set_external_motion(local.0, true);
        diag::info!(World, "Jak Mode started");
    }
    if !mode.active {
        return;
    }
    if host.jak.is_none() {
        return;
    }
    let pad = active
        .and_then(|active| active.0)
        .and_then(|entity| gamepads.get(entity).ok());
    let (held, left, right) = if mode.input_blocked {
        (0, [0.0; 2], [0.0; 2])
    } else {
        read_input(&keys, &mouse, pad)
    };
    host.latched |= held & !host.held_last;
    host.held_last = held;
    let mut look = Vec2::new(right[0], -right[1]) * 180.0 * time.delta_secs();
    for m in motion.read() {
        if !mode.input_blocked {
            look += m.delta * 0.15;
        }
    }
    let dt = time.delta_secs().min(0.1);
    host.accumulated = (host.accumulated + dt).min(0.25);
    let voxel = sim::voxel::active();
    let mut blocks = BlockWorld {
        scratch: Vec::new(),
    };
    let mut empty = jak_mode::EmptyWorld;
    let mut events = Vec::new();
    while host.accumulated >= STEP {
        host.accumulated -= STEP;
        let Some(jak) = host.jak.as_mut() else {
            break;
        };
        jak.camera = follow_camera(&mut host.camera, jak, look, STEP).1;
        let input = PadInput {
            held,
            pressed: std::mem::take(&mut host.latched),
            left,
            right,
        };
        let mut mobs = if voxel {
            Mobs::gather()
        } else {
            Mobs { boxes: Vec::new() }
        };
        let forward = jak.orientation() * jg::Vec3::Z;
        jak.gun.aim = mobs.nearest_in_front(
            jak.gun.fire_point,
            jg::Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero(),
            jak_mode::gun::TRACK_FIND_RANGE,
        );
        let world: &mut dyn CollideWorld = if voxel {
            &mut blocks
        } else if let Collision::Map(grid) = &mut host.collision {
            &mut **grid
        } else {
            &mut empty
        };
        jak.step(&input, world, Some(&mut mobs));
        events.extend(jak.take_events());
        look = Vec2::ZERO;
    }
    let Some(jak) = host.jak.as_ref() else {
        return;
    };
    for event in &events {
        match *event {
            Event::Impact(hit) if voxel => {
                let damage = hit.damage * DAMAGE_TO_MW2;
                match hit.actor {
                    Some(key) => {
                        sim::voxel::push_mob_shot(key, damage, from_jak(hit.pos).to_array())
                    }
                    None => sim::voxel::push_shot(
                        from_jak(hit.surface).to_array(),
                        dir_from_jak(hit.normal).normalize_or_zero().to_array(),
                        damage,
                    ),
                }
            }
            Event::BoardOn => diag::info!(World, "Jak: on the board"),
            Event::BoardOff => diag::info!(World, "Jak: off the board"),
            _ => {}
        }
    }
    let lift = jak.control.draw_offset_y / TO_JAK;
    mode.root = body_matrix(jak, lift);
    mode.board = jak
        .on_board()
        .then(|| mode.root.inverse() * body_matrix(jak, lift * 0.5));
    mode.skins = assets::jak_model::local_jak().map(|model| host.animator.pose(model, jak));
    mode.shots = jak
        .projectiles
        .iter()
        .map(|p| (from_jak(p.trans), from_jak(p.tail)))
        .collect();
    mode.state = jak.state.name().to_owned();
    mode.speed = jak.velocity().length() / jak_mode::METER;
    mode.ammo = (!jak.gun.endless_ammo).then_some(jak.gun.ammo);
    let (view, _) = follow_camera(&mut host.camera, jak, Vec2::ZERO, 0.0);
    mode.camera = Some((view, 70.0));
    authority
        .0
        .set_origin(local.0, from_jak(jak.trans()).to_array());
}

/// The JET-Board as a plain slab under Jak's feet, in the body's first
/// material, until the board's own model is drawn.
pub(crate) fn board_mesh(board: Mat4, geom: &mut crate::anim::remote_body::CpuBodyGeom) {
    let Some(material) = geom.surfaces.first().and_then(|s| s.name.clone()) else {
        return;
    };
    let (half_length, half_width, thickness) = (
        0.7 * MAP_PER_METER,
        0.25 * MAP_PER_METER,
        0.08 * MAP_PER_METER,
    );
    let corner = |x: f32, y: f32, z: f32| board.transform_point3(Vec3::new(x, y, z));
    let faces: [([Vec3; 4], Vec3); 6] = [
        (
            [
                corner(-half_length, -half_width, thickness),
                corner(half_length, -half_width, thickness),
                corner(half_length, half_width, thickness),
                corner(-half_length, half_width, thickness),
            ],
            Vec3::Z,
        ),
        (
            [
                corner(-half_length, half_width, 0.0),
                corner(half_length, half_width, 0.0),
                corner(half_length, -half_width, 0.0),
                corner(-half_length, -half_width, 0.0),
            ],
            -Vec3::Z,
        ),
        (
            [
                corner(half_length, -half_width, 0.0),
                corner(half_length, half_width, 0.0),
                corner(half_length, half_width, thickness),
                corner(half_length, -half_width, thickness),
            ],
            Vec3::X,
        ),
        (
            [
                corner(-half_length, half_width, 0.0),
                corner(-half_length, -half_width, 0.0),
                corner(-half_length, -half_width, thickness),
                corner(-half_length, half_width, thickness),
            ],
            -Vec3::X,
        ),
        (
            [
                corner(half_length, half_width, 0.0),
                corner(-half_length, half_width, 0.0),
                corner(-half_length, half_width, thickness),
                corner(half_length, half_width, thickness),
            ],
            Vec3::Y,
        ),
        (
            [
                corner(-half_length, -half_width, 0.0),
                corner(half_length, -half_width, 0.0),
                corner(half_length, -half_width, thickness),
                corner(-half_length, -half_width, thickness),
            ],
            -Vec3::Y,
        ),
    ];
    let pack = |n: Vec3| {
        [
            (n.x * 127. + 127.5) as u8,
            (n.y * 127. + 127.5) as u8,
            (n.z * 127. + 127.5) as u8,
            63,
        ]
    };
    let index_start = geom.indices.len() as u32;
    for (quad, local_normal) in faces {
        let normal = board.transform_vector3(local_normal).normalize_or(Vec3::Z);
        let base = geom.packed.len() as u32;
        for (k, p) in quad.iter().enumerate() {
            let mut packed = [0; asset_iw4::size::GFX_PACKED_VERTEX];
            for (i, f) in p.to_array().iter().enumerate() {
                packed[i * 4..i * 4 + 4].copy_from_slice(&f.to_le_bytes());
            }
            packed[12..16].copy_from_slice(&1f32.to_le_bytes());
            packed[16..20].fill(255);
            let uv = [(k == 1 || k == 2) as u8 as f32, (k >= 2) as u8 as f32];
            packed[20..22].copy_from_slice(&half::f16::from_f32(uv[0]).to_le_bytes());
            packed[22..24].copy_from_slice(&half::f16::from_f32(uv[1]).to_le_bytes());
            packed[24..28].copy_from_slice(&pack(normal));
            packed[28..32].copy_from_slice(&pack(normal.any_orthonormal_vector()));
            geom.packed.push(packed);
        }
        geom.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    geom.surfaces.push(crate::anim::remote_body::CpuSurfMeta {
        index_start,
        index_count: geom.indices.len() as u32 - index_start,
        name: Some(material),
    });
    geom.decoded_n = geom.packed.len();
}
