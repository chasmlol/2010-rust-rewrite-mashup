use ::ui::UiDraw;
use bevy::prelude::*;
use frame::LaunchIdentity;
use net::{AuthorityClock, AuthorityWorld, PresentedSnapshot};

use crate::{ConsoleCommand, ConsoleLine, ConsoleSettings, ConsoleState};

use super::state_dump::write_current_state_dump;

pub(crate) fn route_ui_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut output: (
        ResMut<ConsoleState>,
        Res<ConsoleSettings>,
        ResMut<ConsoleLine>,
    ),
    mut ui_draw: ResMut<UiDraw>,
    mut game_settings: ResMut<frame::GameSettings>,
    menus: Res<hud::ScriptMenus>,
    identity: Option<Res<LaunchIdentity>>,
    (authority, authority_clock, presented): (
        Option<Res<AuthorityWorld>>,
        Option<Res<AuthorityClock>>,
        Option<Res<PresentedSnapshot>>,
    ),
    mut menu_requests: MessageWriter<frame::UiMenuRequest>,
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
            "ui" => match cmd.args.as_slice() {
                [] => echo(
                    format!("ui = {}", if ui_draw.0 { 1 } else { 0 }),
                    console,
                    line,
                ),
                [value] if value == "0" || value.eq_ignore_ascii_case("off") => {
                    ui_draw.0 = false;
                    echo("ui 0".into(), console, line);
                }
                [value] if value == "1" || value.eq_ignore_ascii_case("on") => {
                    ui_draw.0 = true;
                    echo("ui 1".into(), console, line);
                }
                _ => echo("usage: ui [0|1]".into(), console, line),
            },

            "thirdperson" | "cg_thirdPerson" => {
                let next = match cmd.args.as_slice() {
                    [] => Some(!game_settings.third_person),
                    [value] if value.eq_ignore_ascii_case("toggle") => {
                        Some(!game_settings.third_person)
                    }
                    [value] if value == "1" || value.eq_ignore_ascii_case("on") => Some(true),
                    [value] if value == "0" || value.eq_ignore_ascii_case("off") => Some(false),
                    _ => None,
                };
                if let Some(next) = next {
                    game_settings.third_person = next;
                    game_settings.touch();
                    echo(format!("thirdperson {}", u8::from(next)), console, line);
                } else {
                    echo("usage: thirdperson [0|1|toggle]".into(), console, line);
                }
            }
            "togglemenu" => {
                menu_requests.write(frame::UiMenuRequest::Toggle);
            }
            "openmenu" | "closemenu" => match cmd.args.as_slice() {
                [name] => {
                    menu_requests.write(if cmd.name == "openmenu" {
                        frame::UiMenuRequest::Open(name.clone())
                    } else {
                        frame::UiMenuRequest::Close(name.clone())
                    });
                }
                _ => echo(format!("usage: {} <menu>", cmd.name), console, line),
            },
            "menutext" => {
                menu_requests.write(frame::UiMenuRequest::Text(cmd.args.join(" ")));
            }
            "menukey" => {
                let key = match cmd.args.first().map(|a| a.to_ascii_lowercase()).as_deref() {
                    Some("escape") => Some(frame::UiMenuKey::Escape),
                    Some("enter") => Some(frame::UiMenuKey::Enter),
                    Some("up") => Some(frame::UiMenuKey::Up),
                    Some("down") => Some(frame::UiMenuKey::Down),
                    Some("left") => Some(frame::UiMenuKey::Left),
                    Some("right") => Some(frame::UiMenuKey::Right),
                    Some("home") => Some(frame::UiMenuKey::Home),
                    Some("end") => Some(frame::UiMenuKey::End),
                    Some("backspace") => Some(frame::UiMenuKey::Backspace),
                    Some("delete") => Some(frame::UiMenuKey::Delete),
                    _ => None,
                };
                match key {
                    Some(key) => {
                        menu_requests.write(frame::UiMenuRequest::Key(key));
                    }
                    None => echo(
                        "usage: menukey escape|enter|up|down|left|right|home|end|backspace|delete"
                            .into(),
                        console,
                        line,
                    ),
                }
            }

            "menu" => match parse_menu_args(&cmd.args) {
                Err(msg) => echo(msg, console, line),
                Ok(MenuVerb::Status) => {
                    echo(
                        format!(
                            "menu: stack=[{}] focus={:?}",
                            menus.open_names().join(","),
                            menus.focused_item()
                        ),
                        console,
                        line,
                    );
                }
                Ok(MenuVerb::Dump) => match write_current_state_dump(
                    identity.as_deref(),
                    "menu",
                    authority_clock.as_deref(),
                    authority.as_deref(),
                    presented.as_deref(),
                    None,
                ) {
                    Ok(path) => echo(
                        format!("menu dump: wrote {}", path.display()),
                        console,
                        line,
                    ),
                    Err(error) => echo(format!("menu dump: {error}"), console, line),
                },
                Ok(MenuVerb::Open(name)) => {
                    menu_requests.write(frame::UiMenuRequest::Open(name));
                }
                Ok(MenuVerb::Nav(dir)) => {
                    let key = match dir.as_str() {
                        "up" => frame::UiMenuKey::Up,
                        "down" => frame::UiMenuKey::Down,
                        _ => continue,
                    };
                    menu_requests.write(frame::UiMenuRequest::Key(key));
                }
                Ok(MenuVerb::Accept) => {
                    menu_requests.write(frame::UiMenuRequest::Key(frame::UiMenuKey::Enter));
                }
                Ok(MenuVerb::Back) => {
                    menu_requests.write(frame::UiMenuRequest::Key(frame::UiMenuKey::Escape));
                }
            },
            _ => {}
        }
    }
}

const MENU_USAGE: &str = "usage: menu [open <screen> | nav up|down | accept | back | dump]";

#[derive(Debug, PartialEq)]
enum MenuVerb {
    Status,
    Dump,
    Open(String),
    Nav(String),
    Accept,
    Back,
}

fn parse_menu_args(args: &[String]) -> Result<MenuVerb, String> {
    let sub = args.first().map(String::as_str).unwrap_or("");
    match sub {
        "" => Ok(MenuVerb::Status),
        "dump" => {
            if args.len() != 1 {
                return Err("usage: menu dump".into());
            }
            Ok(MenuVerb::Dump)
        }
        "open" => {
            let name = args
                .get(1)
                .cloned()
                .ok_or_else(|| "usage: menu open <screen>".to_owned())?;
            if args.len() != 2 {
                return Err("usage: menu open <screen>".into());
            }
            Ok(MenuVerb::Open(name))
        }
        "nav" => {
            let dir = args.get(1).map(String::as_str).unwrap_or("");
            match dir {
                "up" | "down" if args.len() == 2 => Ok(MenuVerb::Nav(dir.to_owned())),
                _ => Err("usage: menu nav up|down".into()),
            }
        }
        "accept" if args.len() == 1 => Ok(MenuVerb::Accept),
        "back" if args.len() == 1 => Ok(MenuVerb::Back),
        "accept" | "back" => Err(MENU_USAGE.into()),
        _ => Err(MENU_USAGE.into()),
    }
}
