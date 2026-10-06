use std::path::PathBuf;

use asset_audio::load_mp_sound_bank;
use asset_game::{load_mp_localized_strings, load_ui_menu_catalog};
use asset_transport::{LoadProgress, find_runtime_common_mp};
// The Minecraft world is found and listed as a map of its own.
use assets::{find_zone_file, list_mp_map_packs};
use assets::{
    LoadingPreviewSource, LoadingScreen, MatchLoadRequest, NamespaceSoundIwd, NamespaceTrees,
};
use audio::{SoundBank, SoundIwd};
use bevy::prelude::*;
use bevy::window::PresentMode;
use render::diag::acceptance::{
    ACCEPTANCE_HEIGHT, ACCEPTANCE_PRESENT_MODE, ACCEPTANCE_WIDTH, AcceptanceRun,
};
use render::diag::capture::{CaptureQueue, CaptureRequest};
use render_frontend::prepare::scene::world::WorldScene;
use replay::{Playback, ReplayPlayback};
use session::StartupCommands;
use ui::{
    AppScreen, ClassLoadoutCatalog, LaunchIdentity, LaunchReport, MenuMapList, UiAssetRoot, UiDraw,
    UiLayer, UiLayers,
};

use crate::args::{AcceptanceLaunch, LaunchMode};
use crate::bench;
use crate::plugins::{add_runtime_plugins, add_runtime_plugins_with_role};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Listen,
    Dedicated,
    Client,
    Replay,
}

struct LaunchConfig {
    role: Role,
    zone: String,
    games_root: PathBuf,
    artifacts: PathBuf,
}

fn launch_report(
    zone: String,
    common_mp: Result<PathBuf, String>,
    zone_ff: Result<PathBuf, String>,
    zone_alias: Option<String>,
) -> LaunchReport {
    LaunchReport {
        zone,
        common_mp,
        zone_ff,
        zone_alias,
        sim_gap: "map loading has not reached clipmap yet",
        prediction_metrics: None,
        world_report: vec!["queued background zone load".into()],
    }
}

#[derive(Resource)]
struct ShellCommonTask {
    task: bevy::tasks::Task<assets::ShellCommon>,
    perk_table: Option<asset_game::CapturedStringTable>,
    started: std::time::Instant,
}

fn install_class_catalog(
    mut commands: Commands,
    shell: Option<ResMut<ShellCommonTask>>,
    mut strings: ResMut<asset_game::LocalizeCatalog>,
    menus: Res<asset_game::MenuCatalog>,
) {
    use bevy::tasks::futures_lite::future;
    let Some(mut shell) = shell else {
        return;
    };
    let Some(common) = future::block_on(future::poll_once(&mut shell.task)) else {
        return;
    };
    for line in &common.report {
        diag::info!(Launch, "{line}");
    }
    commands.insert_resource(ui::frontend::maps::MapPresentation::from_tables(
        &common.tables,
    ));
    let mut class_catalog =
        ClassLoadoutCatalog::from_weapon_registry(std::sync::Arc::new(common.weapons))
            .with_weapon_tables(&common.tables);
    if let Some(table) = shell.perk_table.as_ref() {
        class_catalog = class_catalog.with_perk_table(table);
    }
    diag::info!(
        Launch,
        "CAC menu: primary={} secondary={} lethal={} tactical={} excluded={} ({:.0}ms after the menu started)",
        class_catalog.primary.len(),
        class_catalog.secondary.len(),
        class_catalog.lethal.len(),
        class_catalog.tactical.len(),
        class_catalog.excluded.len(),
        shell.started.elapsed().as_secs_f32() * 1000.0,
    );
    for (key, preview) in &mut class_catalog.previews {
        if key.contains('+')
            && asset_core::AssetKey::parse(key)
                .is_ok_and(|key| key.namespace == asset_core::AssetNamespace::Iw4)
            && menus.material_images.contains_key(&preview.image)
        {
            preview.image = format!("iw4:material/{}", preview.image);
        }
    }
    strings.absorb(common.strings);
    commands.insert_resource(class_catalog);
    commands.remove_resource::<ShellCommonTask>();
}

fn launch_identity(config: &LaunchConfig) -> LaunchIdentity {
    LaunchIdentity {
        role_label: format!("{:?}", config.role),
        games_root: config.games_root.clone(),
        artifacts: config.artifacts.clone(),
        zone: config.zone.clone(),
    }
}

fn fatal(msg: &str) -> ! {
    diag::exit_launch_error(msg);
}

pub fn launch(
    games: asset_transport::GamesRoot,
    artifacts: PathBuf,
    mode: LaunchMode,
    acceptance: Option<AcceptanceLaunch>,
    cheats: sim::HostCheats,
) {
    asset_transport::set_game_folders(console::stored_game_folders(&artifacts));
    let steam = asset_transport::link_steam_games(&games);
    diag::info!(Launch, "{}", asset_transport::games_root_report(&games));

    if let Some(plan) = crate::frame_owner::prefer_performance_cores() {
        assets::publish_process_cpus(plan.allowed.clone());
        diag::info!(
            Launch,
            "frame owner: cpus {:?} of {:?} allowed (performance cores {:?})",
            plan.chosen,
            plan.allowed,
            plan.performance
        );
    }
    match mode {
        LaunchMode::Menu => {
            if acceptance.is_some() {
                fatal(&format!(
                    "render acceptance requires `iw4l map <zone>` (not menu); maps: {}",
                    render::diag::acceptance::ACCEPTANCE_MAPS.join(", ")
                ));
            }
            run_menu(games, artifacts, cheats, &steam);
        }
        LaunchMode::Map(zone) => run_map(
            games,
            artifacts,
            zone,
            acceptance,
            Role::Listen,
            None,
            cheats,
        ),
        LaunchMode::Serve(zone) => {
            run_map(games, artifacts, zone, None, Role::Dedicated, None, cheats)
        }
        LaunchMode::ExportGltf(zone) => {
            if acceptance.is_some() {
                fatal("render acceptance is not available for export-gltf");
            }
            run_export_gltf(games, artifacts, zone);
        }
        LaunchMode::Play {
            name,
            zone_override,
        } => run_play(games, artifacts, name, zone_override, acceptance),
    }
}

fn run_export_gltf(games: asset_transport::GamesRoot, artifacts: PathBuf, zone_arg: String) {
    let found = find_zone_file(&games, &zone_arg)
        .unwrap_or_else(|error| fatal(&format!("export-gltf: {error}")));
    let game = asset_transport::zone_game_for_path(&found.path)
        .unwrap_or_else(|| fatal("export-gltf: source game could not be identified"));
    if game != asset_core::ZoneGame::Iw4 {
        fatal(&format!(
            "export-gltf P1a supports native IW4 only; {} is {}",
            found.zone_name,
            game.prefix()
        ));
    }
    let common_mp = find_runtime_common_mp(&games, &found.path).map(|zone| zone.path);

    let prepared = match bevy::tasks::futures_lite::future::block_on(assets::load_prepared_match(
        Ok(found.path),
        common_mp,
        asset_transport::LoadProgress::default(),
    )) {
        assets::MatchLoadOutcome::Ready(prepared) => prepared,
        assets::MatchLoadOutcome::Canceled => fatal("export-gltf: map walk canceled"),
        assets::MatchLoadOutcome::Refused(reason) => {
            fatal(&format!("export-gltf: map walk refused: {reason}"))
        }
    };
    let summary = assets::export_prepared_world_gltf(
        &artifacts,
        &found.zone_name,
        prepared.world,
        &prepared.materials,
    )
    .unwrap_or_else(|error| fatal(&format!("export-gltf: {error}")));
    diag::announce_stdout(&summary.scene.display().to_string());
    diag::announce_stdout(&summary.report_line());
}

fn mw2_not_found(
    games: &asset_transport::GamesRoot,
    steam: &asset_transport::SteamProbe,
) -> String {
    use asset_transport::SteamCandidate;
    let instructions = if cfg!(windows) {
        "MW2 Multiplayer was not found.\n\n\
        Put a shortcut to the game folder next to iw4l.exe:\n\
        1. In Steam, right-click Call of Duty: Modern Warfare 2 > Manage > Browse local \
        files and copy the folder path from the address bar.\n\
        2. In the folder with iw4l.exe, right-click > New > Shortcut, paste the path and \
        name the shortcut Modern Warfare 2.\n\
        3. Launch iw4l.exe again.\n\nSearched:\n"
    } else {
        "MW2 Multiplayer was not found.\n\nCreate an iw4 symlink to the game's installation directory in the configured games root, then launch again.\n\nSearched:\n"
    };
    let mut text = instructions.to_owned();
    for folder in asset_transport::search_roots(&games.0) {
        text.push_str(&format!("  {}\n", folder.display()));
    }
    if games
        .0
        .join(asset_transport::MW2_SHORTCUT)
        .symlink_metadata()
        .is_ok()
    {
        text.push_str(&format!(
            "{} already exists, so Steam was not searched.\n",
            asset_transport::MW2_SHORTCUT
        ));
    } else if !steam.steam_found {
        text.push_str("Steam was not found on this PC.\n");
    } else {
        text.push_str("Tried in Steam:\n");
        for (_, folder, candidate) in steam
            .tried
            .iter()
            .filter(|(game, _, _)| *game == asset_core::ZoneGame::Iw4)
        {
            let outcome = match candidate {
                SteamCandidate::Missing => "not installed here".to_owned(),
                SteamCandidate::NoMultiplayerData => {
                    "multiplayer data is missing; verify the game files in Steam".to_owned()
                }
                SteamCandidate::Linked => "linked, but the game data could not be read".to_owned(),
                SteamCandidate::OneOfSeveral => {
                    "one of several installs; put a shortcut to the one to use".to_owned()
                }
                SteamCandidate::ShortcutFailed(error) => {
                    format!("found, but the shortcut was not created: {error}")
                }
            };
            text.push_str(&format!("  {}: {outcome}\n", folder.display()));
        }
    }
    text
}

fn run_menu(
    games: asset_transport::GamesRoot,
    artifacts: PathBuf,
    cheats: sim::HostCheats,
    steam: &asset_transport::SteamProbe,
) {
    start_perf(None, "menu");
    let ui_games = asset_game::ui_games_root(&games).unwrap_or_else(|error| {
        for line in asset_transport::games_content_report(&games) {
            diag::warn!(Launch, "{line}");
        }
        diag::warn!(Launch, "{error}");
        fatal(&mw2_not_found(&games, steam))
    });
    let shell_common = assets::load_pool().spawn(assets::load_shell_common(games.clone()));
    let (mut menus, menu_report) = load_ui_menu_catalog(&ui_games);
    ui::install_frontend_menus(&mut menus).unwrap_or_else(|error| fatal(&error));
    for line in &menu_report {
        diag::info!(Launch, "{line}");
    }
    let missing = ["main", "main_text"]
        .into_iter()
        .filter(|name| menus.get(name).is_none())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        fatal(&format!(
            "Cannot start the IW4 menu: {} not loaded from {}.\n\n\
             A missing menu can mean an absent file or a FastFile decoding failure. \
             Include the catalog details below when reporting this error.\n\n{}",
            missing.join(", "),
            ui_games.0.display(),
            menu_report.join("\n")
        ));
    }
    let maps = list_mp_map_packs(&games);
    diag::info!(
        Launch,
        "menu: {} maps in {} packs under {}",
        maps.iter().map(|pack| pack.maps.len()).sum::<usize>(),
        maps.len(),
        games.0.display()
    );
    let config = LaunchConfig {
        role: Role::Listen,
        zone: String::new(),
        games_root: games.0.clone(),
        artifacts,
    };
    let mut app = App::new();
    let namespace_trees = NamespaceTrees::discover(&games);
    let have = content_flags(&namespace_trees);
    let master_intent = if have.0 & net::CONTENT_IW4 == 0 {
        diag::warn!(
            Net,
            "master browser disabled: IW4 common_mp.ff is not installed"
        );
        net::MasterLaunchIntent::disabled()
    } else {
        net::MasterLaunchIntent::browser_from_env(have).unwrap_or_else(|error| {
            diag::warn!(Net, "master browser disabled: {error}");
            net::MasterLaunchIntent::disabled()
        })
    };
    app.insert_resource(master_intent);
    app.add_plugins(crate::plugins::default_plugins_with_quiet_log(
        WindowPlugin {
            primary_window: Some(Window {
                title: "iw4l".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        },
    ));
    app.insert_resource(ShellCommonTask {
        task: shell_common,
        perk_table: menus.string_table("mp/perkTable.csv").cloned(),
        started: std::time::Instant::now(),
    })
    .add_systems(Update, install_class_catalog);
    match load_mp_localized_strings(&ui_games, "iw4:code_post_gfx_mp") {
        Ok(loc) => {
            diag::info!(Launch, "menu: {} localize keys", loc.len());
            app.insert_resource(loc);
        }
        Err(error) => diag::warn!(Launch, "menu: localize: {error}"),
    }
    app.insert_resource(menus);
    let menu_bank = match load_mp_sound_bank(&ui_games, "iw4:code_post_gfx_mp") {
        Ok(loaded) => {
            diag::info!(Launch, "menu: sound bank ready");
            for line in loaded.gap_lines() {
                diag::warn!(Launch, "menu: {line}");
            }
            let bank = std::sync::Arc::new(loaded.catalog);
            app.insert_resource(SoundBank(std::sync::Arc::clone(&bank)));
            Some(bank)
        }
        Err(error) => {
            diag::warn!(Launch, "menu: sound bank: {error}");
            None
        }
    };

    let (menu_iwd, lines) = NamespaceSoundIwd::open(&namespace_trees);
    for line in lines {
        diag::info!(Launch, "menu: {line}");
    }
    let menu_iwd = std::sync::Arc::new(menu_iwd);
    app.insert_resource(SoundIwd(std::sync::Arc::clone(&menu_iwd)));
    if let Some(bank) = menu_bank {
        app.insert_resource(audio::ClipStore::start(
            std::sync::Arc::clone(&bank),
            Some(std::sync::Arc::clone(&menu_iwd)),
        ));
        app.insert_resource(audio::FrontendAudio {
            bank,
            iwd: menu_iwd,
        });
    }
    app.insert_resource(launch_identity(&config))
        .insert_resource(cheats)
        .insert_resource(MenuMapList(maps))
        .insert_resource(AppScreen::MainMenu)
        .insert_resource(UiAssetRoot(Some(ui_games.0)))
        .insert_resource(StartupCommands {
            lines: console::startup_commands(),
        })
        .insert_resource(WorldScene::default())
        .insert_resource(ClearColor(Color::srgb(0.04, 0.045, 0.06)))
        .insert_resource({
            let mut layers = UiLayers::default();
            layers.show_only([UiLayer::Shell, UiLayer::Overlay]);
            layers
        });
    add_runtime_plugins(&mut app);
    if let Some(capture) = CaptureRequest::from_env() {
        queue_launch_capture(
            &mut app,
            CaptureRequest {
                exit_after_capture: true,
                ..capture
            },
        );
    }
    app.run();
    let _ = flush_perf();
}

fn run_play(
    games: asset_transport::GamesRoot,
    artifacts: PathBuf,
    name: String,
    zone_override: Option<String>,
    acceptance: Option<AcceptanceLaunch>,
) {
    if acceptance.is_some() {
        fatal("render acceptance requires `iw4l map <zone>` (not play)");
    }
    let playback = Playback::open(&artifacts, &name).unwrap_or_else(|e| {
        fatal(&format!("play: {e}"));
    });
    let zone = zone_override
        .or_else(|| playback.identity().zone_name().map(str::to_owned))
        .unwrap_or_else(|| {
            fatal(&format!(
                "play: recording {} has no zone — re-record on this build, or: make play {name} ZONE=<map>",
                playback.path().display()
            ));
        });
    diag::info!(Launch, "play: {} zone={zone}", playback.path().display());
    let session = ReplayPlayback::new(playback);
    run_map(
        games,
        artifacts,
        zone,
        None,
        Role::Replay,
        Some(session),
        sim::HostCheats::default(),
    );
}

fn run_map(
    games: asset_transport::GamesRoot,
    artifacts: PathBuf,
    zone_arg: String,
    acceptance: Option<AcceptanceLaunch>,
    role: Role,
    playback: Option<ReplayPlayback>,
    cheats: sim::HostCheats,
) {
    let found = find_zone_file(&games, &zone_arg);
    let zone_alias = found.as_ref().ok().and_then(|z| z.alias_note.clone());
    let zone_arg_lc = zone_arg.trim().to_ascii_lowercase();
    let (parsed_game, requested_stem) = asset_transport::split_zone_key(&zone_arg_lc);
    let zone = found
        .as_ref()
        .ok()
        .map(|z| z.zone_name.clone())
        .unwrap_or_else(|| requested_stem.to_owned());
    let zone_ff = found
        .as_ref()
        .map(|z| z.path.clone())
        .map_err(|e| e.clone());
    let common_mp = match &zone_ff {
        Ok(path) => find_runtime_common_mp(&games, path).map(|z| z.path),
        Err(e) => Err(e.clone()),
    };
    let probe = launch_report(zone.clone(), common_mp.clone(), zone_ff.clone(), zone_alias);
    let game = parsed_game.or_else(|| {
        found
            .as_ref()
            .ok()
            .and_then(|z| asset_transport::zone_game_for_path(&z.path))
    });
    let loading_title = asset_transport::map_load_title(&zone_arg_lc, game);
    let namespace_trees = NamespaceTrees::discover(&games);
    let have = content_flags(&namespace_trees);
    let requires = net::content_required_by_map(&zone_arg_lc)
        .unwrap_or_else(|error| fatal(&format!("master content: {error}")));

    let master_intent = if role == Role::Replay {
        net::MasterLaunchIntent::disabled()
    } else if have.0 & net::CONTENT_IW4 == 0 {
        diag::warn!(
            Net,
            "master launch disabled: IW4 common_mp.ff is not installed"
        );
        net::MasterLaunchIntent::disabled()
    } else {
        net::MasterLaunchIntent::from_env_for_map(&zone, have, requires).unwrap_or_else(|error| {
            diag::warn!(Net, "master launch disabled: {error}");
            net::MasterLaunchIntent::disabled()
        })
    };

    let config = LaunchConfig {
        role: if master_intent.is_join() {
            Role::Client
        } else {
            role
        },
        zone: zone.clone(),
        games_root: games.0.clone(),
        artifacts,
    };
    start_perf(Some(zone.clone()), role_name(config.role));
    let (mut menus, menu_report) = load_ui_menu_catalog(&games);
    ui::install_frontend_menus(&mut menus).unwrap_or_else(|error| fatal(&error));
    for line in &menu_report {
        diag::info!(Launch, "{line}");
    }
    let progress = LoadProgress::default();
    let acceptance_run = acceptance
        .map(|a| AcceptanceRun {
            artifact_dir: a.dir,
            map: zone.clone(),
        })
        .or_else(|| AcceptanceRun::from_env(zone.clone()));
    let present_mode = launch_present_mode(acceptance_run.is_some());
    let mut app = App::new();
    if acceptance_run.is_some() || std::env::var_os("IW4L_PRESENT_MODE").is_some() {
        app.insert_resource(ui::PresentModeOverride(present_mode));
    }
    app.insert_resource(master_intent);
    let dedicated = config.role == Role::Dedicated;
    if dedicated {
        app.insert_resource(bevy::winit::WinitSettings::continuous())
            .insert_resource(frame::Headless);
    }
    app.add_plugins(crate::plugins::default_plugins_with_quiet_log(
        WindowPlugin {
            exit_condition: if dedicated {
                bevy::window::ExitCondition::DontExit
            } else {
                bevy::window::ExitCondition::OnAllClosed
            },
            primary_window: (!dedicated).then(|| Window {
                title: match config.role {
                    Role::Replay => format!("iw4l — play {}", config.zone),
                    Role::Listen | Role::Dedicated => format!("iw4l — {}", config.zone),
                    Role::Client => format!("iw4l — join {}", config.zone),
                },
                resolution: (ACCEPTANCE_WIDTH, ACCEPTANCE_HEIGHT).into(),
                present_mode,
                ..default()
            }),
            ..default()
        },
    ));
    if let Ok(path) = &zone_ff {
        app.insert_resource(LoadingPreviewSource {
            path: path.clone(),
            map_name: zone.clone(),
            request_id: 0,
        });
    }
    let ui_games_root = asset_game::ui_games_root(&games).ok().map(|root| root.0);
    app.insert_resource(launch_identity(&config))
        .insert_resource(cheats)
        .insert_resource(probe)
        .insert_resource(menus)
        .insert_resource(MatchLoadRequest {
            request_id: 0,
            load_key: Default::default(),
            zone: zone.clone(),
            zone_ff,
            common_mp,
            progress: progress.clone(),
        })
        .insert_resource(LoadingScreen::new(
            progress.clone(),
            loading_title,
            sim::host_game_mode_kind().display_name().to_owned(),
        ))
        .insert_resource(UiAssetRoot(ui_games_root))
        .insert_resource(StartupCommands {
            lines: console::startup_commands(),
        })
        .insert_resource(WorldScene::default())
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(AppScreen::Loading)
        .insert_resource({
            let mut layers = UiLayers::default();
            layers.show_only([UiLayer::Loading, UiLayer::Overlay]);
            layers
        });
    if let Some(run) = acceptance_run {
        diag::info!(
            Launch,
            "render acceptance: dir={} map={} {}x{} {:?}",
            run.artifact_dir.display(),
            run.map,
            ACCEPTANCE_WIDTH,
            ACCEPTANCE_HEIGHT,
            present_mode
        );

        app.insert_resource(UiDraw(false));
        app.insert_resource(run);
    }
    match config.role {
        Role::Replay => add_runtime_plugins_with_role(&mut app, frame::RuntimeRole::Replay),
        Role::Listen | Role::Dedicated => add_runtime_plugins(&mut app),
        Role::Client => add_runtime_plugins_with_role(&mut app, frame::RuntimeRole::Client),
    }
    // The demo, before the playback moves into the world: it names the workload
    // in the bench manifest, and "same demo" is what makes two runs comparable
    // at all. The whole path, not a stem — a clip is `clips/<id>/clip.iw4ldemo`,
    // whose stem is `clip` for every clip ever recorded.
    let demo = playback
        .as_ref()
        .map(|playback| playback.path().display().to_string());
    if let Some(playback) = playback {
        app.insert_resource(playback);
    }
    if let Some(capture) = CaptureRequest::from_env() {
        queue_launch_capture(&mut app, capture);
    }
    bench::announce_runtime(&mut app);
    let bench = bench::enabled();
    if bench {
        bench::insert(
            &mut app,
            &zone,
            demo.as_deref(),
            role_name(config.role),
            &config.artifacts,
            progress.clone(),
        );
    }
    app.run();
    let trace = flush_perf();
    if bench {
        bench::finish(&config.artifacts, trace);
    }
}

fn content_flags(trees: &NamespaceTrees) -> master_protocol::ContentFlags {
    net::content_inventory(
        trees.get(asset_core::AssetNamespace::Iw4).is_some(),
        trees.get(asset_core::AssetNamespace::Iw5).is_some(),
        trees.get(asset_core::AssetNamespace::T5).is_some(),
        trees.get(asset_core::AssetNamespace::T6).is_some(),
    )
}

fn start_perf(zone: Option<String>, role: &str) {
    if let Err(error) = perf::start(perf::RunMetadata {
        zone,
        role: role.to_owned(),
        focus: std::env::var("IW4L_PERF_FOCUS").ok(),
    }) {
        diag::error!(Launch, "perf: start failed: {error}");
    }
}

fn flush_perf() -> Option<PathBuf> {
    match perf::flush() {
        Ok(path) => path,
        Err(error) => {
            diag::error!(Launch, "perf: flush failed: {error}");
            None
        }
    }
}

fn queue_launch_capture(app: &mut App, request: CaptureRequest) {
    app.world_mut()
        .get_resource_mut::<CaptureQueue>()
        .expect("CaptureQueue: RenderPlugin must be added before a launch capture is queued")
        .push(request);
}

const fn role_name(role: Role) -> &'static str {
    match role {
        Role::Listen => "listen",
        Role::Dedicated => "dedicated",
        Role::Client => "client",
        Role::Replay => "replay",
    }
}

fn launch_present_mode(acceptance: bool) -> PresentMode {
    if acceptance {
        return ACCEPTANCE_PRESENT_MODE;
    }
    match std::env::var("IW4L_PRESENT_MODE").ok().as_deref() {
        Some("AutoNoVsync") => PresentMode::AutoNoVsync,
        Some("Immediate") => PresentMode::Immediate,
        Some("Mailbox") => PresentMode::Mailbox,
        Some("Fifo") => PresentMode::Fifo,
        Some(other) => {
            diag::warn!(Launch, "unknown IW4L_PRESENT_MODE={other}; using Fifo");
            PresentMode::default()
        }
        None => PresentMode::default(),
    }
}
