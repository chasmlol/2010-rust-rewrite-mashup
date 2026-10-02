use bevy::prelude::*;
use net::{
    ClientActionInbox, LocalPresentClient, LookState, PresentedSnapshot, look_angles_from_degrees,
};
use sim::{ClientAction, ClientLifecycle, MatchPhase, SpawnPick};
use ui::UiLayer;

use crate::feature_dispatch::DebugPosOverlay;
use crate::{
    ConsoleCommand, ConsoleDispatch, ConsoleLine, ConsoleRegistry, ConsoleSettings, ConsoleState,
    plugin::WaitMovePose,
};

#[derive(Component)]
pub(crate) struct ShowposHud;
#[derive(Component)]
pub(crate) struct SkateHud;
#[derive(Component)]
pub(crate) struct JakHud;

pub(crate) fn spawn_showpos_hud(commands: &mut Commands, font: Handle<Font>) {
    commands.spawn((JakHud, UiLayer::Overlay, Visibility::Hidden,
        Node { position_type: PositionType::Absolute, bottom:px(24), left:px(24),padding:UiRect::all(px(8)), ..default() },
        BackgroundColor(Color::srgba(0.02,0.03,0.04,0.7)),GlobalZIndex(19000),Text::new(""),
        TextFont{font:font.clone().into(),font_size:FontSize::Px(18.),..default()},TextColor(Color::srgb(0.98,0.88,0.35))));
    commands.spawn((SkateHud, UiLayer::Overlay, Visibility::Hidden,
        Node { position_type: PositionType::Absolute, bottom:px(24), left:px(24),padding:UiRect::all(px(8)), ..default() },
        BackgroundColor(Color::srgba(0.02,0.03,0.04,0.7)),GlobalZIndex(19000),Text::new(""),
        TextFont{font:font.clone().into(),font_size:FontSize::Px(18.),..default()},TextColor(Color::WHITE)));

    commands.spawn((
        ShowposHud,
        UiLayer::Overlay,
        Visibility::Hidden,
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            top: px(12),
            padding: UiRect::all(px(6)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.03, 0.04, 0.72)),
        GlobalZIndex(19_000),
        Text::new(""),
        TextFont {
            font: font.into(),
            font_size: FontSize::Px(14.0),
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.85, 0.40)),
    ));
}

pub(crate) fn register_debug_move_commands(registry: &mut ConsoleRegistry) {
    registry.register(crate::CommandSpec::new("skate").usage("skate [on|off|status] - local Skate gameplay (J toggles)"));
    if registry.resolve("showpos").is_none() {
        registry.register(
            crate::CommandSpec::new("showpos")
                .alias("debug_pos")
                .usage("showpos [on|off] — origin overlay (bare also prints once)"),
        );
    }
    if registry.resolve("move").is_none() {
        registry.register(
            crate::CommandSpec::new("move")
                .alias("tp")
                .usage("move <x> <y> <z> [yaw] [pitch] — write player origin (needs cheats)"),
        );
    }
    if registry.resolve("kill").is_none() {
        registry
            .register(crate::CommandSpec::new("kill").usage(
                "kill — ForceDeath the local player (needs cheats; invented, not COMMANDS)",
            ));
    }
    registry.register(
        crate::CommandSpec::new("jak")
            .usage("jak [on|off|status] - Jak Mode: Jak 3's JET-Board and Blaster (K toggles)"),
    );
    if registry.resolve("force_spawn").is_none() {
        registry.register(crate::CommandSpec::new("force_spawn").usage(
            "force_spawn [random <seed> | at <x> <y> <z> [yaw]] — respawn from any state through spawn resolution (needs cheats)",
        ));
    }
    if registry.resolve("damage").is_none() {
        registry.register(
            crate::CommandSpec::new("damage").usage(
                "damage [amount] — subtract Alive health (default 40; needs cheats; invented)",
            ),
        );
    }
    if registry.resolve("look").is_none() {
        registry.register(
            crate::CommandSpec::new("look")
                .usage("look <yaw> <pitch> — write LookState + authority angles (needs cheats)"),
        );
    }
    if registry.resolve("name").is_none() {
        registry.register(
            crate::CommandSpec::new("name")
                .usage("name [string] — write clientState.name[16] for the local client"),
        );
    }
    if registry.resolve("nudge").is_none() {
        registry.register(crate::CommandSpec::new("nudge").usage(
            "nudge <dx> <dy> <dz> — authority-only origin shift, no teleport bit (needs cheats)",
        ));
    }
    if registry.resolve("force_match_start").is_none() {
        registry.register(crate::CommandSpec::new("force_match_start").usage(
            "force_match_start — skip waitForPlayers and matchStartTimer (needs cheats; invented)",
        ));
    }
}

pub(crate) fn route_debug_move_commands(
    (mut skate, mut jak): (ResMut<frame::SkateMode>, ResMut<frame::JakMode>),
    mut events: MessageReader<ConsoleCommand>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    mut debug_pos: ResMut<DebugPosOverlay>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    mut authority: Option<ResMut<net::AuthorityWorld>>,
    mut inbox: Option<ResMut<ClientActionInbox>>,
    mut seq: ResMut<net::ActionRequestIds>,
    mut look: ResMut<LookState>,
    mut dispatch: ResMut<ConsoleDispatch>,
) {
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };

    for cmd in events.read() {
        match cmd.name.as_str() {
            "skate" => {
                match cmd.args.first().map(String::as_str) {
                    Some("status") => {},
                    Some("on") => { skate.toggle_requested = !skate.active && !skate.entering; },
                    Some("off") => { skate.toggle_requested = skate.active || skate.entering; },
                    None => { skate.toggle_requested = true; },
                    _ => { echo("usage: skate [on|off|status]".into(), &mut console, &mut line); continue; }
                }
                echo(format!("skate active={} ready={} controller={:?} tick={} {}",skate.active,skate.preloaded,skate.controller,skate.tick,skate.status),&mut console,&mut line);
            }

            "jak" => {
                match cmd.args.first().map(String::as_str) {
                    Some("status") => {}
                    Some("on") => jak.toggle_requested = !jak.active,
                    Some("off") => jak.toggle_requested = jak.active,
                    None => jak.toggle_requested = true,
                    _ => {
                        echo("usage: jak [on|off|status]".into(), &mut console, &mut line);
                        continue;
                    }
                }
                echo(
                    format!(
                        "jak active={} state={} speed={:.1}m/s {}",
                        jak.active, jak.state, jak.speed, jak.status
                    ),
                    &mut console,
                    &mut line,
                );
            }

            "showpos" | "debug_pos" => match cmd.args.first().map(String::as_str) {
                None => {
                    debug_pos.0 = true;
                    echo(format_showpos(&presented, local.0), &mut console, &mut line);
                }
                Some("on") | Some("1") => {
                    debug_pos.0 = true;
                    echo("showpos on".into(), &mut console, &mut line);
                }
                Some("off") | Some("0") => {
                    debug_pos.0 = false;
                    echo("showpos off".into(), &mut console, &mut line);
                }
                Some(other) => echo(
                    format!("usage: showpos [on|off] (got `{other}`)"),
                    &mut console,
                    &mut line,
                ),
            },
            "move" | "tp" => match parse_move(&cmd.args, &presented, local.0) {
                Err(msg) => echo(msg, &mut console, &mut line),
                Ok((origin, angles)) => {
                    if !alive(&presented, local.0) {
                        echo(
                            "move: not Alive — spawn a class first".into(),
                            &mut console,
                            &mut line,
                        );
                        continue;
                    }
                    if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) {
                        echo("move: cheats are off".into(), &mut console, &mut line);
                        continue;
                    }
                    let Some(inbox) = inbox.as_deref_mut() else {
                        echo(
                            "move: no action inbox (not a listen host)".into(),
                            &mut console,
                            &mut line,
                        );
                        continue;
                    };
                    let request_id = seq.allocate();
                    if let Err(error) = inbox.push(
                        local.0,
                        ClientAction::Move {
                            request_id,
                            origin,
                            angles,
                        },
                    ) {
                        echo(format!("move: {error}"), &mut console, &mut line);
                        continue;
                    }

                    look.angles = look_angles_from_degrees(angles);
                    arm_wait_move(
                        &mut dispatch,
                        cmd.background,
                        local.0,
                        origin,
                        angles,
                        "move",
                    );
                    echo(
                        format!(
                            "move: queued ({:.1} {:.1} {:.1}) yaw={:.0} pitch={:.0} request_id={request_id}{}",
                            origin[0],
                            origin[1],
                            origin[2],
                            angles[1],
                            angles[0],
                            if cmd.background {
                                " (async)"
                            } else {
                                " (sync until presented pose)"
                            }
                        ),
                        &mut console,
                        &mut line,
                    );
                }
            },
            "look" => match parse_look(&cmd.args, &presented, local.0) {
                Err(msg) => echo(msg, &mut console, &mut line),
                Ok((origin, angles)) => {
                    if !alive(&presented, local.0) {
                        echo(
                            "look: not Alive — spawn a class first".into(),
                            &mut console,
                            &mut line,
                        );
                        continue;
                    }
                    if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) {
                        echo("look: cheats are off".into(), &mut console, &mut line);
                        continue;
                    }
                    let Some(inbox) = inbox.as_deref_mut() else {
                        echo(
                            "look: no action inbox (not a listen host)".into(),
                            &mut console,
                            &mut line,
                        );
                        continue;
                    };
                    let request_id = seq.allocate();
                    if let Err(error) = inbox.push(
                        local.0,
                        ClientAction::Move {
                            request_id,
                            origin,
                            angles,
                        },
                    ) {
                        echo(format!("look: {error}"), &mut console, &mut line);
                        continue;
                    }
                    look.angles = look_angles_from_degrees(angles);
                    arm_wait_move(
                        &mut dispatch,
                        cmd.background,
                        local.0,
                        origin,
                        angles,
                        "look",
                    );
                    echo(
                        format!(
                            "look: queued yaw={:.0} pitch={:.0} request_id={request_id}{}",
                            angles[1],
                            angles[0],
                            if cmd.background {
                                " (async)"
                            } else {
                                " (sync until presented pose)"
                            }
                        ),
                        &mut console,
                        &mut line,
                    );
                }
            },
            "kill" => {
                if !alive(&presented, local.0) {
                    echo(
                        "kill: not Alive — spawn a class first".into(),
                        &mut console,
                        &mut line,
                    );
                    continue;
                }
                if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) {
                    echo("kill: cheats are off".into(), &mut console, &mut line);
                    continue;
                }
                let Some(inbox) = inbox.as_deref_mut() else {
                    echo(
                        "kill: no action inbox (not a listen host)".into(),
                        &mut console,
                        &mut line,
                    );
                    continue;
                };
                let request_id = seq.allocate();
                if let Err(error) = inbox.push(local.0, ClientAction::ForceDeath { request_id }) {
                    echo(format!("kill: {error}"), &mut console, &mut line);
                    continue;
                }
                echo(
                    format!("kill: queued ForceDeath request_id={request_id}"),
                    &mut console,
                    &mut line,
                );
            }
            "force_spawn" => {
                let pick = match parse_force_spawn(&cmd.args) {
                    Ok(pick) => pick,
                    Err(error) => {
                        echo(format!("force_spawn: {error}"), &mut console, &mut line);
                        continue;
                    }
                };
                if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) {
                    echo(
                        "force_spawn: cheats are off".into(),
                        &mut console,
                        &mut line,
                    );
                    continue;
                }
                let Some(inbox) = inbox.as_deref_mut() else {
                    echo(
                        "force_spawn: no action inbox (not a listen host)".into(),
                        &mut console,
                        &mut line,
                    );
                    continue;
                };
                let request_id = seq.allocate();
                if let Err(error) =
                    inbox.push(local.0, ClientAction::ForceSpawn { request_id, pick })
                {
                    echo(format!("force_spawn: {error}"), &mut console, &mut line);
                    continue;
                }
                if !cmd.background {
                    let life = authority
                        .as_ref()
                        .and_then(|a| a.0.client_meta(local.0))
                        .map(|m| m.life_sequence)
                        .unwrap_or_default();
                    dispatch.wait_alive = Some((local.0, life));
                    dispatch.wait_alive_elapsed = 0.0;
                }
                echo(
                    format!("force_spawn: queued {pick:?} request_id={request_id}"),
                    &mut console,
                    &mut line,
                );
            }
            "damage" => {
                let amount = match cmd.args.first() {
                    None => Ok(40),
                    Some(raw) => raw
                        .parse::<i32>()
                        .map_err(|_| format!("usage: damage [amount] (got `{raw}`)")),
                };
                let amount = match amount {
                    Ok(n) if n > 0 => n,
                    Ok(_) => {
                        echo("damage: amount must be > 0".into(), &mut console, &mut line);
                        continue;
                    }
                    Err(msg) => {
                        echo(msg, &mut console, &mut line);
                        continue;
                    }
                };
                if !alive(&presented, local.0) {
                    echo(
                        "damage: not Alive — spawn a class first".into(),
                        &mut console,
                        &mut line,
                    );
                    continue;
                }
                if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) {
                    echo("damage: cheats are off".into(), &mut console, &mut line);
                    continue;
                }
                let Some(inbox) = inbox.as_deref_mut() else {
                    echo(
                        "damage: no action inbox (not a listen host)".into(),
                        &mut console,
                        &mut line,
                    );
                    continue;
                };
                let request_id = seq.allocate();
                if let Err(error) =
                    inbox.push(local.0, ClientAction::DebugDamage { request_id, amount })
                {
                    echo(format!("damage: {error}"), &mut console, &mut line);
                    continue;
                }
                echo(
                    format!("damage: queued {amount} request_id={request_id}"),
                    &mut console,
                    &mut line,
                );
            }
            "nudge" => {
                if cmd.args.len() != 3 {
                    echo(
                        "usage: nudge <dx> <dy> <dz>".into(),
                        &mut console,
                        &mut line,
                    );
                    continue;
                }
                if !alive(&presented, local.0) {
                    echo(
                        "nudge: not Alive — spawn a class first".into(),
                        &mut console,
                        &mut line,
                    );
                    continue;
                }
                if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) {
                    echo("nudge: cheats are off".into(), &mut console, &mut line);
                    continue;
                }
                let parse = |s: &str| {
                    s.parse::<f32>()
                        .ok()
                        .filter(|v| v.is_finite())
                        .ok_or_else(|| format!("nudge: not a finite number `{s}`"))
                };
                let delta = match (
                    parse(&cmd.args[0]),
                    parse(&cmd.args[1]),
                    parse(&cmd.args[2]),
                ) {
                    (Ok(x), Ok(y), Ok(z)) => [x, y, z],
                    (Err(msg), _, _) | (_, Err(msg), _) | (_, _, Err(msg)) => {
                        echo(msg, &mut console, &mut line);
                        continue;
                    }
                };
                let Some(authority) = authority.as_deref_mut() else {
                    echo(
                        "nudge: no authority world (not a listen host)".into(),
                        &mut console,
                        &mut line,
                    );
                    continue;
                };
                authority.0.gate_nudge_origin(local.0, delta);
                echo(
                    format!(
                        "nudge: authority origin += ({:.1} {:.1} {:.1})",
                        delta[0], delta[1], delta[2]
                    ),
                    &mut console,
                    &mut line,
                );
            }
            "name" => match cmd.args.as_slice() {
                [] => {
                    let shown = presented
                        .snapshot()
                        .and_then(|s| s.meta.for_client(local.0))
                        .and_then(|m| entity_iw4::client_state_name(&m.name).map(str::to_owned));
                    match shown {
                        Some(n) => echo(format!("name is \"{n}\""), &mut console, &mut line),
                        None => echo("name is \"\"".into(), &mut console, &mut line),
                    }
                }
                [raw] => {
                    let Some(inbox) = inbox.as_deref_mut() else {
                        echo(
                            "name: no action inbox (not a listen host)".into(),
                            &mut console,
                            &mut line,
                        );
                        continue;
                    };
                    let packed = entity_iw4::pack_client_state_name(raw);
                    let request_id = seq.allocate();
                    if let Err(error) = inbox.push(
                        local.0,
                        ClientAction::SetName {
                            request_id,
                            name: packed,
                        },
                    ) {
                        echo(format!("name: {error}"), &mut console, &mut line);
                        continue;
                    }
                    echo(
                        format!("name: queued `{raw}` request_id={request_id}"),
                        &mut console,
                        &mut line,
                    );
                }
                _ => echo("usage: name [string]".into(), &mut console, &mut line),
            },
            "force_match_start" => {
                if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) {
                    echo(
                        "force_match_start: cheats are off".into(),
                        &mut console,
                        &mut line,
                    );
                    continue;
                }
                let Some(world) = authority.as_ref() else {
                    echo(
                        "force_match_start: no authority world (not a listen host)".into(),
                        &mut console,
                        &mut line,
                    );
                    continue;
                };
                match world.0.phase() {
                    MatchPhase::Playing => {
                        echo(
                            "force_match_start: already playing".into(),
                            &mut console,
                            &mut line,
                        );
                        continue;
                    }
                    MatchPhase::Warmup => {}
                    other => {
                        echo(
                            format!("force_match_start: match is not in warmup (phase={other:?})"),
                            &mut console,
                            &mut line,
                        );
                        continue;
                    }
                }
                let Some(inbox) = inbox.as_deref_mut() else {
                    echo(
                        "force_match_start: no action inbox (not a listen host)".into(),
                        &mut console,
                        &mut line,
                    );
                    continue;
                };
                let request_id = seq.allocate();
                if let Err(error) = inbox.push(
                    local.0,
                    ClientAction::SetMatchPhase {
                        request_id,
                        phase: MatchPhase::Playing,
                    },
                ) {
                    echo(
                        format!("force_match_start: {error}"),
                        &mut console,
                        &mut line,
                    );
                    continue;
                }
                if !cmd.background {
                    dispatch.wait_playing = true;
                    dispatch.wait_playing_elapsed = 0.0;
                    echo(
                        format!(
                            "force_match_start: queued SetMatchPhase Playing request_id={request_id} (sync until Playing)"
                        ),
                        &mut console,
                        &mut line,
                    );
                } else {
                    echo(
                        format!(
                            "force_match_start &: queued SetMatchPhase Playing request_id={request_id} (async)"
                        ),
                        &mut console,
                        &mut line,
                    );
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn update_showpos_overlay(
    debug_pos: Res<DebugPosOverlay>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    clock: Res<net::AuthorityClock>,
    time: Res<Time>,
    actions: Res<net::ClientActionInput>,
    mut line: ResMut<ConsoleLine>,
    mut hud: Query<(&mut Text, &mut Visibility), With<ShowposHud>>,
) {
    if !debug_pos.0 {
        for (_, mut vis) in &mut hud {
            *vis = Visibility::Hidden;
        }
        return;
    }
    let text = format_showpos_live(&presented, local.0, clock.tick, &time, &actions);
    line.0 = text.clone();
    for (mut hud_text, mut vis) in &mut hud {
        *vis = Visibility::Visible;
        **hud_text = text.clone();
    }
}

fn alive(presented: &PresentedSnapshot, id: sim::ClientId) -> bool {
    presented
        .snapshot()
        .and_then(|s| s.meta.for_client(id))
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
}

fn format_showpos(presented: &PresentedSnapshot, id: sim::ClientId) -> String {
    match presented.alive_player(id) {
        Some(ps) => {
            let eye_z = ps.origin[2] + ps.view_height_current;
            format!(
                "showpos origin={:.1} {:.1} {:.1}  eye={:.1} {:.1} {:.1}  yaw={:.0} pitch={:.0}",
                ps.origin[0],
                ps.origin[1],
                ps.origin[2],
                ps.origin[0],
                ps.origin[1],
                eye_z,
                ps.viewangles[1],
                ps.viewangles[0]
            )
        }
        None => "showpos: not Alive".into(),
    }
}

fn format_showpos_live(
    presented: &PresentedSnapshot,
    id: sim::ClientId,
    tick: u32,
    time: &Time,
    actions: &net::ClientActionInput,
) -> String {
    let life = presented
        .snapshot()
        .and_then(|s| s.meta.for_client(id))
        .map(|m| format!("{:?}", m.lifecycle))
        .unwrap_or_else(|| "—".into());
    let (origin, eye, yaw, pitch, vz, ground) = match presented.alive_player(id) {
        Some(ps) => {
            let eye_z = ps.origin[2] + ps.view_height_current;
            (
                format!(
                    "{:.0} {:.0} {:.0}",
                    ps.origin[0], ps.origin[1], ps.origin[2]
                ),
                format!("{:.0} {:.0} {:.0}", ps.origin[0], ps.origin[1], eye_z),
                ps.viewangles[1],
                ps.viewangles[0],
                ps.velocity[2],
                ps.ground_entity_num,
            )
        }
        None => ("—".into(), "—".into(), 0.0, 0.0, 0.0, -1),
    };
    let fps = if time.delta_secs() > 0.0 {
        1.0 / time.delta_secs()
    } else {
        0.0
    };
    let mut held_names = Vec::new();
    actions.client.kb.visit_active_names(|n| held_names.push(n));
    let held = if held_names.is_empty() {
        "—".into()
    } else {
        held_names.join(",")
    };
    format!(
        "showpos tick={tick} life={life} origin={origin} eye={eye} yaw={yaw:.0} pitch={pitch:.0} vz={vz:.0} ground={ground} held=[{held}] fps={fps:.0}"
    )
}

fn parse_move(
    args: &[String],
    presented: &PresentedSnapshot,
    id: sim::ClientId,
) -> Result<([f32; 3], [f32; 3]), String> {
    if args.len() < 3 || args.len() > 5 {
        return Err("usage: move <x> <y> <z> [yaw] [pitch]".into());
    }
    let parse = |s: &str| {
        s.parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("move: not a finite number `{s}`"))
    };
    let origin = [parse(&args[0])?, parse(&args[1])?, parse(&args[2])?];
    let current = presented
        .alive_player(id)
        .map(|ps| ps.viewangles)
        .unwrap_or([0.0, 0.0, 0.0]);
    let mut angles = current;
    if args.len() >= 4 {
        angles[1] = parse(&args[3])?;
    }
    if args.len() == 5 {
        angles[0] = parse(&args[4])?;
    }
    Ok((origin, angles))
}

fn parse_look(
    args: &[String],
    presented: &PresentedSnapshot,
    id: sim::ClientId,
) -> Result<([f32; 3], [f32; 3]), String> {
    if args.len() != 2 {
        return Err("usage: look <yaw> <pitch>".into());
    }
    let parse = |s: &str| {
        s.parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("look: not a finite number `{s}`"))
    };
    let Some(ps) = presented.alive_player(id) else {
        return Err("look: not Alive — spawn a class first".into());
    };
    let mut angles = ps.viewangles;
    angles[1] = parse(&args[0])?;
    angles[0] = parse(&args[1])?;
    Ok((ps.origin, angles))
}

fn arm_wait_move(
    dispatch: &mut ConsoleDispatch,
    background: bool,
    client: sim::ClientId,
    origin: [f32; 3],
    angles: [f32; 3],
    verb: &str,
) {
    if background {
        diag::info!(Console, "{verb} &: async — FIFO not blocked");
        return;
    }
    dispatch.wait_move = Some(WaitMovePose {
        client,
        origin,
        angles,
    });
    dispatch.wait_move_elapsed = 0.0;
}

pub(crate) fn presented_matches_move(presented: &PresentedSnapshot, pose: WaitMovePose) -> bool {
    let Some(ps) = presented.alive_player(pose.client) else {
        return false;
    };
    pose_matches_move(ps.origin, ps.viewangles, pose.origin, pose.angles)
}

fn pose_matches_move(
    have_origin: [f32; 3],
    have_angles: [f32; 3],
    want_origin: [f32; 3],
    want_angles: [f32; 3],
) -> bool {
    (have_origin[0] - want_origin[0]).abs() < 1.0
        && (have_origin[1] - want_origin[1]).abs() < 1.0
        && angle_abs_delta(have_angles[0], want_angles[0]) < 0.5
        && angle_abs_delta(have_angles[1], want_angles[1]) < 0.5
}

fn angle_abs_delta(a: f32, b: f32) -> f32 {
    let mut d = (a - b) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    d.abs()
}

fn parse_force_spawn(args: &[String]) -> Result<SpawnPick, String> {
    const USAGE: &str = "usage: force_spawn [random <seed> | at <x> <y> <z> [yaw]]";
    let num = |raw: &String| {
        raw.parse::<f32>()
            .map_err(|_| format!("{USAGE} (got `{raw}`)"))
    };
    match args.first().map(String::as_str) {
        None => Ok(SpawnPick::Seeded(0)),
        Some("random") => {
            let [_, seed] = args else {
                return Err(USAGE.into());
            };
            seed.parse::<u64>()
                .map(SpawnPick::Seeded)
                .map_err(|_| format!("{USAGE} (got `{seed}`)"))
        }
        Some("at") if matches!(args.len(), 4 | 5) => Ok(SpawnPick::At {
            origin: [num(&args[1])?, num(&args[2])?, num(&args[3])?],
            yaw: args.get(4).map(num).transpose()?.unwrap_or(0.0),
        }),
        Some(_) => Err(USAGE.into()),
    }
}

pub(crate) fn update_skate_overlay(mode:Res<frame::SkateMode>,mut hud:Query<(&mut Text,&mut Visibility),With<SkateHud>>) {
    for (mut text,mut visibility) in &mut hud {
        let failed = !mode.preloaded && !mode.preload_pending && !mode.status.is_empty();
        *visibility=if mode.active || mode.entering || failed {Visibility::Visible}else{Visibility::Hidden};
        **text=if failed {format!("Skate unavailable: {}", mode.status)}
        else if mode.entering && !mode.preloaded {"Skate is finishing map preparation... | J: cancel".into()}
        else if mode.controller.is_none() {"SKATE | Connect an Xbox / XInput controller | J: return to MW2".into()}
        else {"SKATE | Original controller controls | Start: pause | J: return to MW2".into()};
    }
}

pub(crate) fn update_jak_overlay(
    mode: Res<frame::JakMode>,
    mut hud: Query<(&mut Text, &mut Visibility), With<JakHud>>,
) {
    for (mut text, mut visibility) in &mut hud {
        *visibility = if mode.active || !mode.status.is_empty() {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        **text = if !mode.active {
            format!("Jak Mode: {}", mode.status)
        } else {
            let ammo = mode
                .ammo
                .map_or("endless".to_owned(), |a| format!("{a:.0}"));
            format!(
                "JAK | {} | {:.1} m/s | Blaster ammo {ammo}\nWASD/stick move  Space/A jump  F/RMB/RT board  LMB/RB fire, on the board flip  Ctrl/LB duck, charge jump  Ctrl or Shift/LT + direction trick  E/B zap  mouse/right stick camera | K: return to MW2",
                mode.state, mode.speed
            )
        };
    }
}
