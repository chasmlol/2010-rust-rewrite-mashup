use bevy::prelude::*;
use net::{
    ClientActionInbox, LocalPresentClient, LookState, PresentedSnapshot, look_angles_from_degrees,
};
use sim::ClientAction;

use crate::{ConsoleCommand, ConsoleLine, ConsoleRegistry, ConsoleSettings, ConsoleState};

#[derive(Resource, Default)]
pub(crate) struct SavedPosition {
    saved: Option<([f32; 3], [f32; 3])>,
    on_spawn: bool,
}

pub(crate) fn register_saved_position_commands(registry: &mut ConsoleRegistry) {
    for (name, usage) in [
        ("save", "save — remember the player's position and view"),
        (
            "load",
            "load — move the player to the saved position (needs cheats)",
        ),
        (
            "loadspawn",
            "loadspawn [on|off] — move the player to the saved position on every respawn (needs cheats)",
        ),
    ] {
        if registry.resolve(name).is_none() {
            registry.register(crate::CommandSpec::new(name).usage(usage));
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn route_saved_position_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut lives: MessageReader<frame::LifeStarted>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    mut position: ResMut<SavedPosition>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    authority: Option<Res<net::AuthorityWorld>>,
    mut inbox: Option<ResMut<ClientActionInbox>>,
    mut seq: ResMut<net::ActionRequestIds>,
    mut look: ResMut<LookState>,
) {
    let capacity = settings.log_capacity;
    let mut echo = |msg: String| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };
    let mut load =
        |command: &str, saved: Option<([f32; 3], [f32; 3])>, echo: &mut dyn FnMut(String)| {
            let Some((origin, angles)) = saved else {
                echo(format!("{command}: nothing saved — use save first"));
                return;
            };
            if authority.as_ref().is_some_and(|a| !a.0.cheats_enabled()) {
                echo(format!("{command}: cheats are off"));
                return;
            }
            let Some(inbox) = inbox.as_deref_mut() else {
                echo(format!("{command}: no action inbox (not a listen host)"));
                return;
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
                echo(format!("{command}: {error}"));
                return;
            }
            look.angles = look_angles_from_degrees(angles);
            echo(format!(
                "{command}: moved to {:.1} {:.1} {:.1} yaw={:.0} pitch={:.0}",
                origin[0], origin[1], origin[2], angles[1], angles[0]
            ));
        };

    for cmd in events.read() {
        match cmd.name.as_str() {
            "save" => match presented.alive_player(local.0) {
                Some(ps) => {
                    position.saved = Some((ps.origin, ps.viewangles));
                    echo(format!(
                        "save: {:.1} {:.1} {:.1} yaw={:.0} pitch={:.0}",
                        ps.origin[0],
                        ps.origin[1],
                        ps.origin[2],
                        ps.viewangles[1],
                        ps.viewangles[0]
                    ));
                }
                None => echo("save: not Alive — spawn a class first".into()),
            },
            "load" => {
                if presented.alive_player(local.0).is_none() {
                    echo("load: not Alive — spawn a class first".into());
                    continue;
                }
                load("load", position.saved, &mut echo);
            }
            "loadspawn" => {
                let on = match cmd.args.first().map(String::as_str) {
                    None => !position.on_spawn,
                    Some("on" | "1") => true,
                    Some("off" | "0") => false,
                    Some(other) => {
                        echo(format!("usage: loadspawn [on|off] (got `{other}`)"));
                        continue;
                    }
                };
                position.on_spawn = on;
                echo(match (on, position.saved.is_some()) {
                    (true, true) => "loadspawn on: respawns move to the saved position".into(),
                    (true, false) => "loadspawn on: nothing saved yet — use save".into(),
                    (false, _) => "loadspawn off".into(),
                });
            }
            _ => {}
        }
    }

    let respawned = lives.read().any(|life| life.client == local.0.0);
    if respawned && position.on_spawn && position.saved.is_some() {
        load("loadspawn", position.saved, &mut echo);
    }
}
