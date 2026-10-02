//! Local Skate gameplay adapter. Rendering and match ownership remain in IW4L.
pub mod collision;
pub mod rails;
pub mod rig;
use bevy::prelude::*;
use frame::{AppScreen, SkateMode};
use skate_host::bridge::{CollisionBuilder, InputFrame, Pose, PreparedCollision, Session};
use std::sync::{Arc, Mutex, mpsc};

enum Job {
    Activate(u64, Vec3, f32, f32),
    Step(u64, f32, InputFrame, f32),
    Suspend,
}
enum Reply {
    Ready,
    Activated(u64, Pose, u128),
    Pose(u64, Pose),
    Error(String),
}
#[derive(Resource, Default)]
struct Host {
    send: Option<mpsc::Sender<Job>>,
    receive: Option<Mutex<mpsc::Receiver<Reply>>>,
    clip: Option<Arc<asset_world::ClipCollision>>,
    ready: bool,
    enter_requested: bool,
    activating: bool,
    epoch: u64,
    pad_packet: u32,
    previous_buttons: u16,
    input_suspended: bool,
    logged_tick: u64,
}

pub fn register(app: &mut App) {
    app.init_resource::<SkateMode>()
        .init_resource::<Host>()
        .add_systems(Startup, preload_assets)
        .add_systems(
            Update,
            update
                .after(frame::PresentedPublished)
                .before(crate::sync_camera_from_presented)
                .before(render_scene::GfxSceneAdd)
                .in_set(frame::ClientSet::Present),
        );
}

fn preload_assets(mut mode: ResMut<SkateMode>) {
    let Some(root) = std::env::var_os("IW4L_SKATE_ASSETS") else {
        return;
    };
    mode.preload_pending = true;
    if let Err(e) = std::thread::Builder::new()
        .name("skate-preload".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            let start = std::time::Instant::now();
            match Session::preload(std::path::Path::new(&root)) {
                Ok(()) => diag::info!(
                    World,
                    "Skate animation banks preloaded in {}ms",
                    start.elapsed().as_millis()
                ),
                Err(e) => diag::warn!(World, "Skate preload: {e}"),
            }
        })
    {
        diag::warn!(World, "Skate preload thread: {e}");
    }
}

/// Blocks across either way of the skater that a Minecraft world's collision
/// covers, blocks up and down, and how far the skater goes before it is
/// rebuilt around them.
const BLOCK_RADIUS: i32 = 40;
const BLOCK_DEPTH: i32 = 20;
const BLOCK_RECENTRE: f32 = 14.0;

/// A Minecraft world's collision around map point `centre`, for Skate.
fn block_collision(
    builder: &CollisionBuilder,
    centre: Vec3,
) -> Result<(PreparedCollision, usize), String> {
    let triangles: Vec<[[f32; 3]; 3]> =
        sim::voxel::collision_triangles(centre.to_array(), BLOCK_RADIUS, BLOCK_DEPTH)
            .into_iter()
            .map(|t| t.map(|p| collision::to_skate(Vec3::from_array(p)).to_array()))
            .collect();
    if triangles.is_empty() {
        return Err("no blocks around the skater yet".into());
    }
    let n = triangles.len();
    Ok((builder.build(triangles, Vec::new())?, n))
}

/// One retained session per map. Leaving skating only pauses this worker;
/// collision, decoded animation banks, graphs and the rig remain resident.
fn preload_map(host: &mut Host, clip: Arc<asset_world::ClipCollision>) -> Result<(), String> {
    let root =
        std::env::var_os("IW4L_SKATE_ASSETS").ok_or("IW4L_SKATE_ASSETS is not configured")?;
    rig::reference().ok_or("Skate rig.json could not be loaded")?;
    assets::bot_model::local_skate_board().ok_or("Skate board.json could not be loaded")?;
    let (send, receive) = mpsc::channel();
    let (publish, results) = mpsc::channel();
    let geometry = clip.clone();
    std::thread::Builder::new()
        .name("iw4l-skate".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            let result = (|| -> Result<(), String> {
                let start = std::time::Instant::now();
                let world = collision::extract(&geometry);
                let mut session = Session::new(
                    std::path::Path::new(&root),
                    world.triangles,
                    world.rails,
                    [0., 0., 0.],
                    0.,
                )?;
                diag::info!(
                    World,
                    "Skate map session preloaded in {}ms",
                    start.elapsed().as_millis()
                );
                if publish.send(Reply::Ready).is_err() {
                    return Ok(());
                }
                // On a Minecraft world the collision streams: built around
                // the skater off this thread and swapped in as they move or
                // the blocks change.
                let builder = session.collision_builder();
                let (build_send, build_jobs) = mpsc::channel::<Vec3>();
                let (built_send, built) =
                    mpsc::channel::<(u64, Vec3, Result<(PreparedCollision, usize), String>)>();
                std::thread::Builder::new()
                    .name("iw4l-skate-blocks".into())
                    .spawn(move || {
                        while let Ok(mut centre) = build_jobs.recv() {
                            while let Ok(newer) = build_jobs.try_recv() {
                                centre = newer;
                            }
                            let revision = sim::voxel::revision();
                            let prepared = block_collision(&builder, centre);
                            if built_send.send((revision, centre, prepared)).is_err() {
                                break;
                            }
                        }
                    })
                    .map_err(|e| e.to_string())?;
                let mut blocks: Option<(u64, Vec3)> = None;
                let mut building = false;
                let mut requested = std::time::Instant::now();
                let mut skater_at: Option<Vec3> = None;
                let mut accumulated = 0.;
                let mut epoch = 0;
                while let Ok(job) = receive.recv() {
                    match job {
                        Job::Activate(new_epoch, spawn, yaw, aspect_ratio) => {
                            epoch = new_epoch;
                            accumulated = 0.;
                            let start = std::time::Instant::now();
                            session.set_aspect_ratio(aspect_ratio);
                            if sim::voxel::active() {
                                let revision = sim::voxel::revision();
                                match block_collision(&session.collision_builder(), spawn) {
                                    Ok((prepared, n)) => {
                                        session.install_collision(prepared)?;
                                        blocks = Some((revision, spawn));
                                        diag::info!(
                                            World,
                                            "Skate: {n} block collision triangles around the spawn"
                                        );
                                    }
                                    Err(e) => diag::warn!(World, "Skate block collision: {e}"),
                                }
                                skater_at = Some(spawn);
                            }
                            let p = session.activate(
                                collision::to_skate(spawn).to_array(),
                                yaw.to_radians() + std::f32::consts::FRAC_PI_2,
                            )?;
                            if publish
                                .send(Reply::Activated(epoch, p, start.elapsed().as_millis()))
                                .is_err()
                            {
                                break;
                            }
                        }
                        Job::Suspend => {
                            accumulated = 0.;
                            session.suspend_input();
                        }
                        Job::Step(request, dt, input, aspect_ratio) => {
                            if request != epoch {
                                continue;
                            }
                            if let Ok((revision, centre, prepared)) = built.try_recv() {
                                building = false;
                                match prepared {
                                    Ok((prepared, _)) => {
                                        session.install_collision(prepared)?;
                                        blocks = Some((revision, centre));
                                    }
                                    Err(e) => diag::warn!(World, "Skate block collision: {e}"),
                                }
                            }
                            if sim::voxel::active()
                                && !building
                                && let Some(at) = skater_at
                            {
                                let far = blocks.is_none_or(|(_, centre)| {
                                    let d = (at - centre) / sim::voxel::BLOCK;
                                    d.truncate().length() > BLOCK_RECENTRE
                                        || d.z.abs() > BLOCK_DEPTH as f32 * 0.5
                                });
                                let changed = blocks.is_some_and(|(revision, _)| {
                                    revision != sim::voxel::revision()
                                }) && requested.elapsed().as_secs_f32() > 0.25;
                                if (far || changed) && build_send.send(at).is_ok() {
                                    building = true;
                                    requested = std::time::Instant::now();
                                }
                            }
                            session.set_aspect_ratio(aspect_ratio);
                            session.collect(input, dt);
                            accumulated = (accumulated + dt).min(0.15);
                            let mut advanced = false;
                            // The native camera can change the simulation period.
                            while accumulated >= session.period() {
                                accumulated -= session.period();
                                session.advance()?;
                                advanced = true;
                            }
                            if advanced {
                                let p = session.pose();
                                if !p.root.is_finite() || p.bones.iter().any(|b| !b.is_finite()) {
                                    return Err("Skate published a non-finite pose".into());
                                }
                                skater_at = Some(collision::from_skate(p.root.w_axis.truncate()));
                                if publish.send(Reply::Pose(epoch, p)).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
                Ok(())
            })();
            if let Err(e) = result {
                let _ = publish.send(Reply::Error(e));
            }
        })
        .map_err(|e| e.to_string())?;
    host.send = Some(send);
    host.receive = Some(Mutex::new(results));
    host.clip = Some(clip);
    host.ready = false;
    Ok(())
}

fn stop(host: &mut Host, mode: &mut SkateMode, authority: &mut net::AuthorityWorld) {
    authority
        .0
        .set_external_motion(sim::ClientId(mode.client), false);
    host.enter_requested = false;
    host.activating = false;
    host.epoch = host.epoch.wrapping_add(1);
    if let Some(send) = &host.send {
        let _ = send.send(Job::Suspend);
    }
    mode.active = false;
    mode.entering = false;
    mode.camera = None;
    mode.bones.clear();
    mode.status.clear();
    diag::info!(World, "Skate mode stopped; map session retained");
}

fn present(mode: &mut SkateMode, p: Pose, authority: &mut net::AuthorityWorld) {
    let b = collision::basis();
    let mut root = b * p.root * b.inverse();
    root.w_axis = collision::from_skate(p.root.w_axis.truncate()).extend(1.);
    mode.root = root;
    mode.bones = p.bones;
    mode.names = p.names;
    mode.tick = p.tick;
    mode.status = p.state;
    mode.marker_placed = p.marker.placed;
    mode.marker_can_return = p.marker.can_return;
    mode.marker_progress = p.marker.progress;
    mode.camera = p.camera.map(|(position, basis, fov)| {
        (
            Transform::from_translation(collision::from_skate(position)).looking_to(
                b.transform_vector3(basis.z_axis).normalize(),
                b.transform_vector3(basis.y_axis).normalize(),
            ),
            fov,
        )
    });
    authority.0.set_origin(
        sim::ClientId(mode.client),
        root.w_axis.truncate().to_array(),
    );
}

fn update(
    time: Res<Time>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    screen: Res<AppScreen>,
    local: Res<net::LocalPresentClient>,
    presented: Res<net::PresentedSnapshot>,
    clip: Res<crate::DynEntPhysClip>,
    mut authority: Option<ResMut<net::AuthorityWorld>>,
    mut mode: ResMut<SkateMode>,
    mut host: ResMut<Host>,
    (gamepads, active): (
        Query<&bevy::input::gamepad::Gamepad>,
        Option<Res<frame::ActivePad>>,
    ),
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    let Some(authority) = authority.as_deref_mut() else {
        return;
    };
    let aspect_ratio = windows
        .single()
        .map(|w| w.width() / w.height().max(1.))
        .unwrap_or(16. / 9.);
    let ps = presented.player(local.0);
    let alive = ps.is_some_and(|p| p.pm_type == 0) && *screen == AppScreen::InGame;
    let same_map = host
        .clip
        .as_ref()
        .is_none_or(|a| clip.0.as_ref().is_some_and(|b| Arc::ptr_eq(a, b)));
    if (mode.active || host.enter_requested || host.activating) && (!alive || !same_map) {
        stop(&mut host, &mut mode, authority);
    }
    if !same_map {
        host.send = None;
        host.receive = None;
        host.clip = None;
        host.ready = false;
        mode.preloaded = false;
        mode.preload_pending = std::env::var_os("IW4L_SKATE_ASSETS").is_some();
    }
    // This runs during map preparation/class selection, without waiting for J.
    if host.clip.is_none()
        && std::env::var_os("IW4L_SKATE_ASSETS").is_some()
        && let Some(geometry) = clip.0.clone()
    {
        mode.preload_pending = true;
        host.clip = Some(geometry.clone()); // A failed load retries on a new map, never every frame.
        if let Err(e) = preload_map(&mut host, geometry) {
            diag::warn!(World, "Skate map preload: {e}");
            mode.preload_pending = false;
            mode.status = e;
        }
    }
    // Skating reads the same controller as the rest of the game, whatever
    // kind it is, converted to the Xbox layout the skate input expects.
    let pad = active
        .and_then(|active| active.0)
        .and_then(|entity| gamepads.get(entity).ok());
    host.pad_packet = host.pad_packet.wrapping_add(1);
    let input = pad.map_or_else(InputFrame::neutral, |pad| {
        let (marker_set, marker_return) = marker_keyboard_actions(
            !mode.input_blocked && !mode.dropper_open,
            keyboard.just_pressed(KeyCode::F6),
            keyboard.pressed(KeyCode::F7),
            keyboard.pressed(KeyCode::ShiftLeft),
            keyboard.just_pressed(KeyCode::ArrowDown),
            keyboard.pressed(KeyCode::ArrowUp),
        );
        pad_frame(pad, host.pad_packet, marker_set, marker_return)
    });
    mode.controller = input.controller();
    let previous_buttons = host.previous_buttons;
    host.previous_buttons = input.buttons();
    if mode.active
        && (keyboard.just_pressed(KeyCode::Tab)
            || just_pressed(input.buttons(), previous_buttons, 0x0020))
    {
        mode.dropper_open = !mode.dropper_open;
        if mode.dropper_open {
            mode.dropper_status = dropper_catalog_status();
        }
    }

    let mut replies = Vec::new();
    if let Some(receiver) = &host.receive {
        let receiver = receiver.lock().unwrap();
        loop {
            match receiver.try_recv() {
                Ok(reply) => replies.push(reply),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    replies.push(Reply::Error("Skate worker disconnected".into()));
                    break;
                }
            }
        }
    }
    for reply in replies {
        match reply {
            Reply::Ready => {
                host.ready = true;
                mode.preloaded = true;
                mode.preload_pending = false;
                diag::info!(World, "Skate ready before toggle");
            }
            Reply::Activated(epoch, p, ms) if epoch == host.epoch && host.activating && alive => {
                host.activating = false;
                mode.entering = false;
                mode.active = true;
                host.input_suspended = false;
                authority.0.set_external_motion(local.0, true);
                present(&mut mode, p, authority);
                diag::info!(World, "Skate activation from retained session: {ms}ms");
            }
            Reply::Pose(epoch, p) if epoch == host.epoch && mode.active => {
                if p.tick / 120 != host.logged_tick / 120 {
                    diag::info!(
                        World,
                        "Skate tick={} speed={:.2} state={}",
                        p.tick,
                        p.velocity.length(),
                        p.state
                    );
                    host.logged_tick = p.tick;
                }
                present(&mut mode, p, authority);
            }
            Reply::Error(e) => {
                stop(&mut host, &mut mode, authority);
                host.send = None;
                host.receive = None;
                host.ready = false;
                mode.preloaded = false;
                mode.preload_pending = false;
                diag::warn!(World, "Skate stopped: {e}");
                mode.status = e;
                return;
            }
            _ => {}
        }
    }
    if std::mem::take(&mut mode.toggle_requested) && alive {
        if mode.active || host.enter_requested || host.activating {
            stop(&mut host, &mut mode, authority);
            return;
        }
        if host.send.is_none() {
            diag::warn!(World, "Skate session unavailable: {}", mode.status);
            return;
        }
        host.enter_requested = true;
        mode.entering = true;
        mode.client = local.0.0;
    }
    if host.enter_requested
        && host.ready
        && let Some(ps) = ps.filter(|_| alive)
    {
        host.epoch = host.epoch.wrapping_add(1);
        host.enter_requested = false;
        host.activating = true;
        if let Some(send) = &host.send {
            let _ = send.send(Job::Activate(
                host.epoch,
                Vec3::from_array(ps.origin) + Vec3::Z * 2.,
                ps.viewangles[1],
                aspect_ratio,
            ));
        }
    }
    if !mode.active {
        return;
    }
    if mode.input_blocked || mode.dropper_open {
        if !host.input_suspended {
            if let Some(send) = &host.send {
                let _ = send.send(Job::Suspend);
            }
        }
        host.input_suspended = true;
        return;
    }
    host.input_suspended = false;
    if let Some(send) = &host.send {
        if send
            .send(Job::Step(
                host.epoch,
                time.delta_secs().min(0.1),
                input,
                aspect_ratio,
            ))
            .is_err()
        {
            stop(&mut host, &mut mode, authority);
        }
    }
}

fn just_pressed(buttons: u16, previous: u16, button: u16) -> bool {
    buttons & button != 0 && previous & button == 0
}

fn dropper_catalog_status() -> String {
    let Some(root) = std::env::var_os("IW4L_SKATE_ASSETS") else {
        return "Skate assets are not configured.".into();
    };
    let catalog = std::path::Path::new(&root).join("private/park-props/catalog.json");
    if catalog.is_file() {
        "Prop catalog found; in-game model rendering and placement are not wired yet.".into()
    } else {
        "No playable prop catalog. The converter currently extracts source files only.".into()
    }
}

/// A controller's state in XInput's layout, for the skate input.
fn pad_frame(
    pad: &bevy::input::gamepad::Gamepad,
    packet: u32,
    marker_set: bool,
    marker_return: bool,
) -> InputFrame {
    use bevy::input::gamepad::GamepadButton as B;
    const BITS: [(B, u16); 14] = [
        (B::DPadUp, 0x0001),
        (B::DPadDown, 0x0002),
        (B::DPadLeft, 0x0004),
        (B::DPadRight, 0x0008),
        (B::Start, 0x0010),
        (B::Select, 0x0020),
        (B::LeftThumb, 0x0040),
        (B::RightThumb, 0x0080),
        (B::LeftTrigger, 0x0100),
        (B::RightTrigger, 0x0200),
        (B::South, 0x1000),
        (B::East, 0x2000),
        (B::West, 0x4000),
        (B::North, 0x8000),
    ];
    let mut buttons = BITS
        .iter()
        .filter(|(button, _)| pad.pressed(*button))
        .fold(0, |bits, (_, bit)| bits | bit);
    buttons = with_marker_buttons(buttons, marker_set, marker_return);
    // An analog trigger reports its travel; a digital one only pressed.
    let trigger = |button: B| {
        let value = pad
            .get(button)
            .unwrap_or(if pad.pressed(button) { 1.0 } else { 0.0 });
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    let axis = |v: f32| (v.clamp(-1.0, 1.0) * 32767.0).round() as i16;
    let (left, right) = (pad.left_stick(), pad.right_stick());
    InputFrame::from_pad(
        buttons,
        [trigger(B::LeftTrigger2), trigger(B::RightTrigger2)],
        [axis(left.x), axis(left.y)],
        [axis(right.x), axis(right.y)],
        packet,
    )
}

fn with_marker_buttons(buttons: u16, set: bool, return_to_marker: bool) -> u16 {
    let set_buttons = if set { 0x0100 | 0x0002 } else { 0 };
    let return_buttons = if return_to_marker { 0x0100 | 0x0001 } else { 0 };
    buttons | set_buttons | return_buttons
}

fn marker_keyboard_actions(
    enabled: bool,
    f6_pressed: bool,
    f7_held: bool,
    shift_held: bool,
    down_pressed: bool,
    up_held: bool,
) -> (bool, bool) {
    if !enabled {
        return (false, false);
    }
    (
        f6_pressed || (shift_held && down_pressed),
        f7_held || (shift_held && up_held),
    )
}

#[cfg(test)]
mod tests {
    use super::{just_pressed, marker_keyboard_actions, with_marker_buttons};

    #[test]
    fn marker_shortcuts_preserve_pad_input_and_synthesize_native_combos() {
        assert_eq!(with_marker_buttons(0x4000, false, false), 0x4000);
        assert_eq!(with_marker_buttons(0x4000, true, false), 0x4102);
        assert_eq!(with_marker_buttons(0x4000, false, true), 0x4101);
        assert_eq!(with_marker_buttons(0x4000, true, true), 0x4103);
    }

    #[test]
    fn marker_keyboard_arrows_mimic_native_dpad_combos_and_respect_blocking() {
        assert_eq!(
            marker_keyboard_actions(true, false, false, true, true, false),
            (true, false)
        );
        assert_eq!(
            marker_keyboard_actions(true, false, false, true, false, true),
            (false, true)
        );
        assert_eq!(
            marker_keyboard_actions(false, true, true, true, true, true),
            (false, false)
        );
    }

    #[test]
    fn dropper_back_button_toggles_only_on_the_press_edge() {
        assert!(just_pressed(0x0020, 0, 0x0020));
        assert!(!just_pressed(0x0020, 0x0020, 0x0020));
        assert!(!just_pressed(0, 0x0020, 0x0020));
    }
}
