//! Local Skate gameplay adapter. Rendering and match ownership remain in IW4L.
pub mod collision;
pub mod rails;
pub mod rig;
use bevy::prelude::*;
use frame::{AppScreen, SkateMode};
use skate_host::{
    bridge::{CollisionBuilder, InputFrame, Pose, PreparedCollision, Session},
    object_dropper::{DropperState, MenuInput, PlacedProp, PropCatalog},
};
use std::sync::{Arc, Mutex, mpsc};

enum Job {
    Activate(u64, Vec3, f32, f32),
    Step(u64, f32, InputFrame, f32),
    SetProps(Vec<PlacedProp>),
    Suspend,
}
enum Reply {
    Ready(Option<Arc<PropCatalog>>),
    Activated(u64, Pose, u128),
    Pose(u64, Pose),
    DropperError(String),
    Error(String),
}
struct CollisionBuildRequest {
    id: u64,
    centre: Option<Vec3>,
    rebuild_blocks: bool,
    block_triangles: Vec<[[f32; 3]; 3]>,
    placed_props: Vec<PlacedProp>,
}
struct CollisionBuildResult {
    id: u64,
    revision: u64,
    centre: Option<Vec3>,
    block_triangles: Vec<[[f32; 3]; 3]>,
    prepared: Result<PreparedCollision, String>,
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
    prop_catalog: Option<Arc<PropCatalog>>,
    dropper: Option<DropperState>,
}

#[derive(Resource, Default)]
pub struct SkatePropDrawState {
    pub catalog: Option<Arc<PropCatalog>>,
    pub placed: Vec<PlacedProp>,
    pub preview: Option<PlacedProp>,
}

pub fn register(app: &mut App) {
    app.init_resource::<SkateMode>()
        .init_resource::<Host>()
        .init_resource::<SkatePropDrawState>()
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
fn block_collision_triangles(centre: Vec3) -> Result<(Vec<[[f32; 3]; 3]>, usize), String> {
    let triangles: Vec<[[f32; 3]; 3]> =
        sim::voxel::collision_triangles(centre.to_array(), BLOCK_RADIUS, BLOCK_DEPTH)
            .into_iter()
            .map(|t| t.map(|p| collision::to_skate(Vec3::from_array(p)).to_array()))
            .collect();
    if triangles.is_empty() {
        return Err("no blocks around the skater yet".into());
    }
    let n = triangles.len();
    Ok((triangles, n))
}

fn build_session_collision(
    builder: &CollisionBuilder,
    base: &[[[f32; 3]; 3]],
    rails: &[Vec<[f32; 3]>],
    blocks: &[[[f32; 3]; 3]],
    catalog: Option<&PropCatalog>,
    placed: &[PlacedProp],
) -> Result<PreparedCollision, String> {
    let mut triangles = base.to_vec();
    triangles.extend_from_slice(blocks);
    match catalog {
        Some(catalog) => builder.build_with_props(triangles, rails.to_vec(), catalog, placed),
        None if placed.is_empty() => builder.build(triangles, rails.to_vec()),
        None => Err("placed props exist but their local catalog is unavailable".into()),
    }
}

/// One retained session per map. Leaving skating only pauses this worker;
/// collision, decoded animation banks, graphs and the rig remain resident.
fn preload_map(host: &mut Host, clip: Arc<asset_world::ClipCollision>) -> Result<(), String> {
    let root =
        std::env::var_os("IW4L_SKATE_ASSETS").ok_or("IW4L_SKATE_ASSETS is not configured")?;
    rig::reference().ok_or("Skate rig.json could not be loaded")?;
    assets::bot_model::local_skate_board().ok_or("Skate board.json could not be loaded")?;
    let prop_catalog_path = std::path::Path::new(&root).join("private/park-props/catalog.json");
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
                let base_triangles = world.triangles.clone();
                let base_rails = world.rails.clone();
                let mut session = Session::new(
                    std::path::Path::new(&root),
                    base_triangles.clone(),
                    base_rails.clone(),
                    [0., 0., 0.],
                    0.,
                )?;
                let worker_catalog = if prop_catalog_path.is_file() {
                    match PropCatalog::load(&prop_catalog_path) {
                        Ok(catalog) => Some(Arc::new(catalog)),
                        Err(error) => {
                            diag::warn!(World, "Skate prop catalog: {error}");
                            None
                        }
                    }
                } else {
                    None
                };
                diag::info!(
                    World,
                    "Skate map session preloaded in {}ms",
                    start.elapsed().as_millis()
                );
                if publish.send(Reply::Ready(worker_catalog.clone())).is_err() {
                    return Ok(());
                }
                // On a Minecraft world the collision streams: built around
                // the skater off this thread and swapped in as they move or
                // the blocks change.
                let build_builder = session.collision_builder();
                let build_base_triangles = base_triangles.clone();
                let build_base_rails = base_rails.clone();
                let build_catalog = worker_catalog.clone();
                let (build_send, build_jobs) = mpsc::channel::<CollisionBuildRequest>();
                let (built_send, built) = mpsc::channel::<CollisionBuildResult>();
                std::thread::Builder::new()
                    .name("iw4l-skate-blocks".into())
                    .spawn(move || {
                        while let Ok(mut request) = build_jobs.recv() {
                            while let Ok(newer) = build_jobs.try_recv() {
                                request = newer;
                            }
                            let revision = sim::voxel::revision();
                            let centre = request.centre;
                            let block_triangles = if request.rebuild_blocks {
                                centre
                                    .ok_or_else(|| "block collision rebuild has no centre".into())
                                    .and_then(block_collision_triangles)
                                    .map(|(triangles, _)| triangles)
                            } else {
                                Ok(request.block_triangles)
                            };
                            let (block_triangles, prepared) = match block_triangles {
                                Ok(block_triangles) => {
                                    let prepared = build_session_collision(
                                        &build_builder,
                                        &build_base_triangles,
                                        &build_base_rails,
                                        &block_triangles,
                                        build_catalog.as_deref(),
                                        &request.placed_props,
                                    );
                                    (block_triangles, prepared)
                                }
                                Err(error) => (Vec::new(), Err(error)),
                            };
                            if built_send
                                .send(CollisionBuildResult {
                                    id: request.id,
                                    revision,
                                    centre,
                                    block_triangles,
                                    prepared,
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                    })
                    .map_err(|e| e.to_string())?;
                let mut blocks: Option<(u64, Vec3)> = None;
                let mut block_triangles = Vec::new();
                let mut placed_props = Vec::new();
                let mut building = false;
                let mut requested = std::time::Instant::now();
                let mut next_build_id = 0_u64;
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
                                match block_collision_triangles(spawn) {
                                    Ok((triangles, n)) => {
                                        block_triangles = triangles;
                                        let prepared = build_session_collision(
                                            &session.collision_builder(),
                                            &base_triangles,
                                            &base_rails,
                                            &block_triangles,
                                            worker_catalog.as_deref(),
                                            &placed_props,
                                        )?;
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
                        Job::SetProps(placed) => {
                            placed_props = placed;
                            let id = next_build_id;
                            next_build_id = next_build_id.wrapping_add(1);
                            let rebuild_blocks = sim::voxel::active() && skater_at.is_some();
                            if build_send
                                .send(CollisionBuildRequest {
                                    id,
                                    centre: if rebuild_blocks {
                                        skater_at
                                    } else {
                                        blocks.map(|(_, centre)| centre)
                                    },
                                    rebuild_blocks,
                                    block_triangles: block_triangles.clone(),
                                    placed_props: placed_props.clone(),
                                })
                                .is_err()
                            {
                                let _ = publish.send(Reply::DropperError(
                                    "collision builder disconnected".into(),
                                ));
                                building = false;
                                continue;
                            }
                            loop {
                                match built.recv() {
                                    Ok(result) => {
                                        let is_current = result.id == id;
                                        match result.prepared {
                                            Ok(prepared) => {
                                                block_triangles = result.block_triangles;
                                                if let Some(centre) = result.centre {
                                                    blocks = Some((result.revision, centre));
                                                }
                                                session.install_collision(prepared)?;
                                            }
                                            Err(error) if is_current => {
                                                let _ = publish.send(Reply::DropperError(error));
                                            }
                                            Err(error) => {
                                                diag::warn!(
                                                    World,
                                                    "Skate collision rebuild: {error}"
                                                );
                                            }
                                        }
                                        if is_current {
                                            building = false;
                                            break;
                                        }
                                    }
                                    Err(_) => {
                                        let _ = publish.send(Reply::DropperError(
                                            "collision builder disconnected".into(),
                                        ));
                                        building = false;
                                        break;
                                    }
                                }
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
                            if let Ok(result) = built.try_recv() {
                                building = false;
                                match result.prepared {
                                    Ok(prepared) => {
                                        block_triangles = result.block_triangles;
                                        if let Some(centre) = result.centre {
                                            blocks = Some((result.revision, centre));
                                        }
                                        session.install_collision(prepared)?;
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
                                if (far || changed)
                                    && build_send
                                        .send(CollisionBuildRequest {
                                            id: next_build_id,
                                            centre: Some(at),
                                            rebuild_blocks: true,
                                            block_triangles: Vec::new(),
                                            placed_props: placed_props.clone(),
                                        })
                                        .is_ok()
                                {
                                    next_build_id = next_build_id.wrapping_add(1);
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
    host.prop_catalog = None;
    host.dropper = None;
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
        if host
            .dropper
            .as_ref()
            .is_some_and(|dropper| !dropper.placed().is_empty())
        {
            let _ = send.send(Job::SetProps(Vec::new()));
        }
        let _ = send.send(Job::Suspend);
    }
    host.dropper = None;
    mode.active = false;
    mode.entering = false;
    mode.dropper_open = false;
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
    mut prop_draw: ResMut<SkatePropDrawState>,
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
    let mut opened_dropper = false;
    if mode.active
        && (keyboard.just_pressed(KeyCode::Tab)
            || just_pressed(input.buttons(), previous_buttons, 0x0020))
    {
        mode.dropper_open = !mode.dropper_open;
        opened_dropper = mode.dropper_open;
        if mode.dropper_open {
            mode.dropper_status = if let Some(catalog) = &host.prop_catalog {
                format!(
                    "{} props loaded. Enter/A places; arrows/D-pad choose and rotate; X deletes.",
                    catalog.props.len()
                )
            } else {
                dropper_catalog_status()
            };
            host.dropper = host
                .prop_catalog
                .clone()
                .and_then(|catalog| DropperState::new(catalog).ok());
        }
    }

    if mode.active && mode.dropper_open && !opened_dropper {
        let prop_send = host.send.clone();
        if let Some(dropper) = &mut host.dropper {
            let place_position = ps.map_or(mode.root.w_axis.truncate(), |player| {
                let yaw = player.viewangles[1].to_radians();
                Vec3::from_array(player.origin) + Vec3::new(yaw.cos(), yaw.sin(), 0.) * 128.
            });
            let menu_input = MenuInput {
                dpad_up: just_pressed(input.buttons(), previous_buttons, 0x0001),
                dpad_down: just_pressed(input.buttons(), previous_buttons, 0x0002),
                dpad_left: just_pressed(input.buttons(), previous_buttons, 0x0004),
                dpad_right: just_pressed(input.buttons(), previous_buttons, 0x0008),
                keyboard_up: keyboard.just_pressed(KeyCode::ArrowUp),
                keyboard_down: keyboard.just_pressed(KeyCode::ArrowDown),
                keyboard_left: keyboard.just_pressed(KeyCode::ArrowLeft),
                keyboard_right: keyboard.just_pressed(KeyCode::ArrowRight),
                confirm: just_pressed(input.buttons(), previous_buttons, 0x1000)
                    || keyboard.just_pressed(KeyCode::Enter),
                delete: just_pressed(input.buttons(), previous_buttons, 0x4000)
                    || keyboard.just_pressed(KeyCode::Backspace),
                cancel: keyboard.just_pressed(KeyCode::Escape)
                    || just_pressed(input.buttons(), previous_buttons, 0x0020),
            };
            let prior_count = dropper.placed().len();
            let mut menu_error = false;
            match dropper.apply(menu_input, collision::to_skate(place_position)) {
                Ok(true) => {}
                Ok(false) => mode.dropper_open = false,
                Err(error) => {
                    mode.dropper_status = error;
                    menu_error = true;
                }
            }
            if prior_count != dropper.placed().len()
                && let Some(send) = &prop_send
                && let Err(error) = send.send(Job::SetProps(dropper.placed().to_vec()))
            {
                mode.dropper_status = format!("Could not update prop collision: {error}");
                menu_error = true;
            }
            if mode.dropper_open && !menu_error {
                mode.dropper_status = format!(
                    "{} | {}/{} placed",
                    dropper.selected().name,
                    dropper.placed().len(),
                    128
                );
                prop_draw.preview = Some(dropper.preview(collision::to_skate(place_position)));
            } else {
                prop_draw.preview = None;
            }
        } else {
            mode.dropper_status = dropper_catalog_status();
        }
    } else if !mode.dropper_open {
        prop_draw.preview = None;
    }
    prop_draw.catalog = host.prop_catalog.clone();
    prop_draw.placed = host
        .dropper
        .as_ref()
        .map_or_else(Vec::new, |dropper| dropper.placed().to_vec());

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
            Reply::Ready(catalog) => {
                host.prop_catalog = catalog;
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
            Reply::DropperError(error) => {
                mode.dropper_status = format!("Prop collision update failed: {error}");
                diag::warn!(World, "Skate prop collision: {error}");
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
        prop_draw.preview = None;
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
        "The local prop catalog could not be loaded; rerun the Skate converter and check its log."
            .into()
    } else {
        let availability = catalog.with_file_name("availability.json");
        if let Ok(bytes) = std::fs::read(availability)
            && let Ok(report) = serde_json::from_slice::<serde_json::Value>(&bytes)
            && let Some(reason) = report.get("reason").and_then(serde_json::Value::as_str)
        {
            format!("Create-a-Park props unavailable: {reason}")
        } else {
            "No local prop catalog. Run the Skate converter with the game's parkassets.big first."
                .into()
        }
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
