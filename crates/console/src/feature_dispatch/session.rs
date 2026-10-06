use bevy::prelude::*;
use frame::{HasWorld, LaunchIdentity};

use crate::{ConsoleCommand, ConsoleDispatch};

use super::echo::ConsoleEcho;

pub(crate) fn route_session_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut echo: ConsoleEcho,
    mut transition: ResMut<::session::SessionSwapRequest>,
    has_world: Res<HasWorld>,
    playback: Option<Res<::replay::ReplayPlayback>>,
    bridge: Option<Res<net::MasterBridge>>,
    identity: Option<Res<LaunchIdentity>>,
    mut dispatch: ResMut<ConsoleDispatch>,
) {
    for cmd in events.read() {
        match cmd.name.as_str() {
            "map_restart" => {
                let zone = identity
                    .as_ref()
                    .map(|identity| identity.zone.clone())
                    .filter(|zone| has_world.0 && !zone.is_empty());
                match (cmd.args.is_empty(), zone) {
                    (false, _) => {
                        dispatch.release();
                        echo.write("usage: map_restart");
                    }
                    (true, None) => {
                        dispatch.release();
                        echo.write("map_restart: no map to restart");
                    }
                    (true, Some(zone)) => match transition.request_zone(zone.clone()) {
                        Ok(id) => {
                            echo.write(format!("map_restart: requested `{zone}` (swap #{id})"))
                        }
                        Err(error) => {
                            dispatch.release();
                            echo.write(format!("map_restart: {error}"));
                        }
                    },
                }
            }
            "map" => match cmd.args.as_slice() {
                [zone] => match transition.request_zone(zone.clone()) {
                    Ok(id) => echo.write(format!("map: requested `{zone}` (swap #{id})")),
                    Err(error) => {
                        dispatch.release();
                        echo.write(format!("map: {error}"));
                    }
                },
                _ => {
                    dispatch.release();
                    echo.write("usage: map <zone>");
                }
            },
            "disconnect" => {
                let in_session = has_world.0
                    || playback.is_some()
                    || transition.dump_id().is_some()
                    || bridge.is_some();
                if !cmd.args.is_empty() {
                    dispatch.release();
                    echo.write("usage: disconnect");
                } else if !in_session {
                    dispatch.release();
                    echo.write("disconnect: no session to leave");
                } else {
                    match transition.request_leave() {
                        Ok(id) => {
                            diag::lifecycle_boundary(
                                "disconnect_requested",
                                &format!(" swap={id}"),
                            );
                            echo.write(format!(
                                "disconnect: waiting for session teardown (swap #{id})"
                            ))
                        }
                        Err(error) => {
                            dispatch.release();
                            echo.write(format!("disconnect: {error}"));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
