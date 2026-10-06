use bevy::prelude::*;

pub(crate) fn community_server_menu(
    mut events: MessageReader<crate::ConsoleCommand>,
    servers: Res<ui::CommunityServers>,
    (screen, party): (Res<frame::AppScreen>, Res<frame::UiPartyState>),
    mut dvars: ResMut<frame::UiMenuDvars>,
    mut exit: MessageWriter<AppExit>,
    (mut choice, mut error, mut restarting): (
        Local<Option<String>>,
        Local<Option<String>>,
        Local<bool>,
    ),
    mut echo: crate::feature_dispatch::ConsoleEcho,
) {
    let choice = choice.get_or_insert_with(|| servers.selected.clone());
    let in_menu = *screen == frame::AppScreen::MainMenu;
    let locked =
        !in_menu || party.in_lobby || servers.choices.iter().all(|(_, value)| value.is_empty());
    for command in events.read() {
        if !matches!(command.name.as_str(), "set" | "seta") {
            continue;
        }
        let [name, value, ..] = command.args.as_slice() else {
            continue;
        };
        match name.as_str() {
            "ui_community_server" if !locked => {
                if servers.choices.iter().any(|(_, option)| option == value) {
                    *choice = value.clone();
                    *error = None;
                }
            }
            "ui_community_apply" if value == "1" && !*restarting => {
                if locked {
                    echo.write(
                        "Leave the lobby and return to the main menu before switching servers.",
                    );
                } else if !choice.is_empty() && *choice != servers.selected {
                    match servers.restart(choice) {
                        Ok(()) => {
                            *restarting = true;
                            diag::lifecycle_boundary("quit_requested", " via=community_switch");
                            exit.write(AppExit::Success);
                        }
                        Err(cause) => {
                            echo.write(format!("Community switch: {cause}"));
                            *error = Some(cause);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    dvars.set("ui_community_menu", u8::from(in_menu).to_string());
    dvars.set("ui_community_server", choice.clone());
    dvars.set("ui_community_locked", u8::from(locked).to_string());
    dvars.set(
        "ui_community_apply_disabled",
        u8::from(locked || choice.is_empty() || *choice == servers.selected || *restarting)
            .to_string(),
    );
    let current = servers
        .choices
        .iter()
        .find(|(_, value)| *value == servers.selected && !value.is_empty())
        .map(|(label, _)| label.as_str())
        .unwrap_or("None");
    dvars.set("ui_community_current", format!("Current server: {current}"));
    let hint = if let Some(error) = error.as_deref() {
        error
    } else if party.in_lobby {
        "Leave your lobby before switching servers."
    } else if servers.choices.iter().all(|(_, value)| value.is_empty()) {
        "Add .iw4l-server files beside the game, then restart."
    } else {
        "Applying a server restarts the game and checks its updates."
    };
    dvars.set("ui_community_hint", hint);
}
