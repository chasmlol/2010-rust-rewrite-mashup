use bevy::prelude::*;
use bots::{
    BotAddQueue, BotFireQueue, BotHold, BotRoster, BotTpQueue, BotTpRequest, BotTpTarget,
    BotTpWhere,
};
use hud::PendingSplash;
use net::ClientActionInbox;
use render_frontend::adapters::anim::view_kick::PendingViewHurt;
use sim::{ClientAction, ClientId};

use crate::{ConsoleCommand, ConsoleDispatch, ConsoleLine, ConsoleSettings, ConsoleState};

mod capture;
mod echo;
mod hitvol;
mod process;
mod replay;
mod session;
mod state_dump;
mod ui;

pub(crate) use capture::route_capture_commands;
pub(crate) use echo::ConsoleEcho;
pub(crate) use hitvol::route_hitvol_commands;
pub(crate) use process::{exit_process, request_exit};
pub(crate) use replay::route_replay_commands;
pub(crate) use session::route_session_commands;
pub(crate) use state_dump::route_state_dump_commands;
pub(crate) use ui::route_ui_commands;

pub(crate) fn route_debug_feature_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut output: (
        ResMut<ConsoleState>,
        Res<ConsoleSettings>,
        ResMut<ConsoleLine>,
    ),
    mut bot_add: ResMut<BotAddQueue>,
    (mut bot_hold, mut bot_tp, mut bot_fire): (
        ResMut<BotHold>,
        ResMut<BotTpQueue>,
        ResMut<BotFireQueue>,
    ),
    (weapons, mut inbox, mut give_seq, roster): (
        Option<Res<assets::PreparedWeapons>>,
        Option<ResMut<ClientActionInbox>>,
        ResMut<net::ActionRequestIds>,
        Option<Res<BotRoster>>,
    ),
    (mut hurt, mut pending_splash, authority): (
        ResMut<PendingViewHurt>,
        ResMut<PendingSplash>,
        Option<Res<net::AuthorityWorld>>,
    ),
) {
    let (console, settings, line) = &mut output;
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };

    for cmd in events.read() {
        match cmd.name.as_str() {
            "bot" => match parse_bot_args(&cmd.args) {
                Err(msg) => echo(msg, console, line),
                Ok(BotVerb::Hold(_) | BotVerb::Tp(_) | BotVerb::Fire(_) | BotVerb::Give { .. })
                    if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) =>
                {
                    echo("bot: cheats are off".into(), console, line);
                }
                Ok(BotVerb::Add(n)) => {
                    bot_add.push(n);
                    echo(format!("bot: queued add {n}"), console, line);
                }
                Ok(BotVerb::Dummy(n)) => {
                    bot_add.push_dummy(n);
                    echo(format!("bot: queued dummy {n}"), console, line);
                }
                Ok(BotVerb::Hold(on)) => {
                    bot_hold.0 = on;
                    echo(
                        format!("bot: hold {}", if on { "on" } else { "off" }),
                        console,
                        line,
                    );
                }
                Ok(BotVerb::Tp(request)) => {
                    bot_tp.push(request);
                    echo("bot: queued tp".into(), console, line);
                }
                Ok(BotVerb::Fire(target)) => {
                    bot_fire.push(target);
                    echo("bot: queued fire".into(), console, line);
                }
                Ok(BotVerb::Give {
                    id,
                    weapon,
                    attachments,
                }) => {
                    let Some(weapons) = weapons.as_ref() else {
                        echo("bot give: weapon catalog not loaded".into(), console, line);
                        continue;
                    };
                    let Some(inbox) = inbox.as_mut() else {
                        echo("bot give: no action inbox".into(), console, line);
                        continue;
                    };
                    if !roster.as_ref().is_some_and(|r| r.is_bot(id)) {
                        echo(format!("bot give: {id:?} is not a bot"), console, line);
                        continue;
                    }
                    let (camo, attachments) = crate::weapon_dispatch::split_camo(&attachments);
                    match crate::weapon_dispatch::resolve_give_id(
                        weapons.registry(),
                        &weapon,
                        &attachments,
                    )
                    .and_then(|weapon_id| {
                        let model = camo.map_or(Ok(0), |camo| {
                            crate::weapon_dispatch::camo_slot(weapons.registry(), weapon_id, camo)
                        })?;
                        Ok((weapon_id, model))
                    }) {
                        Ok((weapon_id, model)) => {
                            let request_id = give_seq.allocate();
                            if let Err(error) = inbox.push(
                                id,
                                ClientAction::GiveWeapon {
                                    request_id,
                                    weapon: weapon_id,
                                    model,
                                },
                            ) {
                                echo(format!("bot give: {error}"), console, line);
                                continue;
                            }
                            echo(
                                format!(
                                    "bot give: queued {} id={weapon_id} on {} request_id={request_id}",
                                    weapons.registry().configuration_label(weapon_id),
                                    id.0
                                ),
                                console,
                                line,
                            );
                        }
                        Err(msg) => echo(format!("bot give: {msg}"), console, line),
                    }
                }
            },
            "hurt" => {
                hurt.0 = hurt.0.saturating_add(1);
                echo(
                    "hurt: queued undirected view punch (255/255 count=1)".into(),
                    console,
                    line,
                );
            }
            "splash" => match cmd.args.as_slice() {
                [] => echo(
                    "usage: splash <key> [optionalNumber] — splash slot 0 (mp/splashTable.csv)"
                        .into(),
                    console,
                    line,
                ),
                [key, rest @ ..] => {
                    let optional = rest
                        .first()
                        .and_then(|s| s.parse::<i32>().ok())
                        .unwrap_or(0);
                    pending_splash.key = Some(key.clone());
                    pending_splash.optional_number = optional;
                    echo(
                        format!("splash: queued `{key}` optional={optional} (slot 0)"),
                        console,
                        line,
                    );
                }
            },

            _ => {}
        }
    }
}

pub(crate) fn resume_lifecycle_commands(
    mut dispatch: ResMut<ConsoleDispatch>,
    mut transition: ResMut<::session::SessionSwapRequest>,
    mut echo: ConsoleEcho,
) {
    if let Some(completed) = transition.take_completed() {
        match completed.result {
            ::session::SessionSwapResult::Failed { zone, error } => {
                dispatch.release();
                echo.write(format!("map: `{zone}` failed: {error}"));
            }
            _ => dispatch.paused = false,
        }
    }
}

#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct DebugPosOverlay(pub bool);

pub fn register_feature_commands(registry: &mut crate::ConsoleRegistry, maps: &[String]) {
    if registry.resolve("map").is_none() {
        registry.register(
            crate::CommandSpec::new("map")
                .usage("map <zone> — tear down the current occupancy, then load a zone")
                .arg(crate::StaticCompleter::new(maps.iter().cloned())),
        );
    }
    for (name, usage) in [
        ("help", "help — list registered commands"),
        ("clear", "clear — clear console scrollback"),
        ("screenshot", "screenshot [name] — capture the backbuffer"),
        (
            "record",
            "record [name] — start a demo under iw4l-artifacts",
        ),
        ("stoprecord", "stoprecord — finish the open demo"),
        (
            "clip",
            "clip — save last ≤45s available to this client as iw4l-artifacts/clips/<ULID>/{clip.iw4ldemo, dump.txt} (always-on ring)",
        ),
        (
            "map_restart",
            "map_restart — a new match on the current map, on what the last one prepared",
        ),
        (
            "disconnect",
            "disconnect — leave the session: tear the world down, leave the room, back to the main menu",
        ),
        (
            "demo",
            "demo <name> — tear down the current occupancy, then play iw4l-artifacts/demos/<name>.iw4ldemo or clips/<name>/clip.iw4ldemo",
        ),
        ("play", "play <name> — launcher alias of demo"),
        (
            "exit",
            "exit — quit the process, abandoning unfinished screenshots",
        ),
        (
            "quit",
            "quit — quit the process, abandoning unfinished screenshots",
        ),
        (
            "finish_run",
            "finish_run — finish the run's screenshots, then quit",
        ),
        ("ui", "ui [0|1] — hide/show game UI; console Overlay stays"),
        (
            "thirdperson",
            "thirdperson [0|1|toggle] — switch the saved player camera view",
        ),
        (
            "cg_thirdPerson",
            "cg_thirdPerson [0|1|toggle] — switch the saved player camera view",
        ),
        (
            "togglemenu",
            "togglemenu — open the script main menu (g_scriptMainMenu), or escape the top menu",
        ),
        ("openmenu", "openmenu <menu> — open an in-game menuDef"),
        (
            "menutext",
            "menutext <text> — type into the active native menu field",
        ),
        (
            "closemenu",
            "closemenu <menu> — close an open in-game menuDef",
        ),
        (
            "menukey",
            "menukey escape|enter|up|down|left|right|home|end|backspace|delete — a key to the top in-game menu",
        ),
        (
            "dump",
            "dump [name] - atomically write the current authority + presented state to iw4l-artifacts/dumps/<timestamp>-<name>.txt (one shot; no history or timing)",
        ),
        (
            "hitvol",
            "hitvol — what the authority holds for a bullet to clip against: world tables, live player volumes, script-model clips, kit models",
        ),
        (
            "bot",
            "bot add [N] | dummy [N] | hold [on|off] | give <id> <weapon> [att...] [camo=<name>] | fire [all|<id>] | tp all|<id> above <h> | tp all|<id> <x> <y> <z>",
        ),
        (
            "menu",
            "menu [open <screen> | nav up|down | accept | back | dump] — shell surface; back/device remain typed gaps (S2/S4)",
        ),
        ("hurt", "hurt — stamp one undirected damage-feedback punch"),
        (
            "splash",
            "splash <key> [optionalNumber] — splash slot 0 from mp/splashTable.csv (one_shot_kill, longshot, capture, …)",
        ),
        (
            "wait",
            "wait [seconds|<n>t|world|spawn|torn|ambient] — pause the console FIFO; <n>t = n authority ticks; world = scene.spawned; spawn = AppScreen::InGame; torn = HasWorld false and scene.spawned false (hold after MatchTornDown); ambient = MapAmbientBooted (overlay finished, CreateFX sources published)",
        ),
    ] {
        if registry.resolve(name).is_none() {
            registry.register(crate::CommandSpec::new(name).usage(usage));
        }
    }
}

pub(crate) const BOT_USAGE: &str = "usage: bot add [N] | dummy [N] | hold [on|off] | give <id> <weapon> [att...] [camo=<name>] | fire [all|<id>] | tp all|<id> above <h> | tp all|<id> <x> <y> <z> [yaw] [pitch]";

#[derive(Debug, PartialEq)]
pub(crate) enum BotVerb {
    Add(u32),
    Dummy(u32),
    Hold(bool),
    Give {
        id: ClientId,
        weapon: String,
        attachments: Vec<String>,
    },
    Fire(BotTpTarget),
    Tp(BotTpRequest),
}

pub(crate) fn parse_bot_args(args: &[String]) -> Result<BotVerb, String> {
    let sub = args.first().map(String::as_str).unwrap_or("");
    match sub {
        "add" => {
            let n = args
                .get(1)
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(1)
                .clamp(1, 16);
            Ok(BotVerb::Add(n))
        }
        "dummy" => {
            if args.len() > 2 {
                return Err("usage: bot dummy [N]".into());
            }
            let n = args
                .get(1)
                .map(|s| s.parse::<u32>())
                .transpose()
                .map_err(|_| "usage: bot dummy [N]".to_owned())?
                .unwrap_or(1);
            if !(1..=16).contains(&n) {
                return Err("bot dummy: count must be 1..16".into());
            }
            Ok(BotVerb::Dummy(n))
        }
        "hold" => match args.get(1).map(String::as_str) {
            None | Some("on") | Some("1") => Ok(BotVerb::Hold(true)),
            Some("off") | Some("0") => Ok(BotVerb::Hold(false)),
            Some(other) => Err(format!("usage: bot hold [on|off] (got `{other}`)")),
        },
        "give" => {
            let usage = || "usage: bot give <id> <weapon> [attachment...]".to_owned();
            let id = args
                .get(1)
                .ok_or_else(usage)?
                .parse::<u32>()
                .map_err(|_| usage())?;
            let weapon = args.get(2).cloned().ok_or_else(usage)?;
            Ok(BotVerb::Give {
                id: ClientId(id),
                weapon,
                attachments: args[3..].to_vec(),
            })
        }
        "fire" => match args.get(1).map(String::as_str) {
            None | Some("all") => Ok(BotVerb::Fire(BotTpTarget::All)),
            Some(s) => {
                let id = s
                    .parse::<u32>()
                    .map_err(|_| "usage: bot fire [all|<id>]".to_owned())?;
                Ok(BotVerb::Fire(BotTpTarget::Id(ClientId(id))))
            }
        },
        "tp" => parse_bot_tp(&args[1..]),
        _ => Err(BOT_USAGE.into()),
    }
}

fn parse_bot_tp(args: &[String]) -> Result<BotVerb, String> {
    let target = match args.first().map(String::as_str) {
        Some("all") => BotTpTarget::All,
        Some(s) => {
            let id = s.parse::<u32>().map_err(|_| BOT_USAGE.to_owned())?;
            BotTpTarget::Id(sim::ClientId(id))
        }
        None => return Err(BOT_USAGE.into()),
    };
    let rest = &args[1..];
    if rest.first().map(String::as_str) == Some("above") {
        let height = rest
            .get(1)
            .ok_or_else(|| "usage: bot tp all|<id> above <h>".to_owned())
            .and_then(parse_finite)?;
        return Ok(BotVerb::Tp(BotTpRequest {
            target,
            where_: BotTpWhere::Above { height },
        }));
    }
    if rest.len() < 3 || rest.len() > 5 {
        return Err(BOT_USAGE.into());
    }
    let origin = [
        parse_finite(&rest[0])?,
        parse_finite(&rest[1])?,
        parse_finite(&rest[2])?,
    ];
    let yaw = rest.get(3).map(parse_finite).transpose()?;
    let pitch = rest.get(4).map(parse_finite).transpose()?;
    Ok(BotVerb::Tp(BotTpRequest {
        target,
        where_: BotTpWhere::Absolute { origin, yaw, pitch },
    }))
}

fn parse_finite(s: &String) -> Result<f32, String> {
    s.parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("bot: not a finite number `{s}`"))
}
