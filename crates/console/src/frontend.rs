use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use frame::{UiMenuDvars, UiMenuRequest, UiPartyState};
use ui::frontend::maps::{map_label, map_preview, pack_maps};
use ui::frontend::rules::{MATCH_CONFIG, host_rules, seed_rules, selected_game};

use crate::{CommandSpec, ConsoleCommand, ConsoleRegistry};

const PAGE_SIZE: usize = 10;
const PACK_SLOTS: usize = 16;

#[derive(Default)]
pub(crate) struct FrontendState {
    public: bool,
    map_pack: usize,
    map_page: usize,
    map_hover: usize,
    browser_page: usize,
    adverts: Vec<net::MasterAdvert>,
    rules_seeded: bool,
}

#[derive(SystemParam)]
pub(crate) struct LobbyServices<'w> {
    intent: Option<Res<'w, net::MasterLaunchIntent>>,
    browser: Option<Res<'w, net::MasterBrowser>>,
    bridge: Option<Res<'w, net::MasterBridge>>,
    action: Option<ResMut<'w, net::PendingMasterMenuAction>>,
}

impl LobbyServices<'_> {
    fn submit(&mut self, action: net::MasterMenuAction) -> Result<(), String> {
        if !self
            .intent
            .as_ref()
            .is_some_and(|intent| intent.configured())
        {
            return Err("Public lobbies require a configured master".into());
        }
        let pending = self.action.as_mut().ok_or("Lobby service is unavailable")?;
        if pending.0.is_some() {
            return Err("A lobby request is already pending".into());
        }
        pending.0 = Some(action);
        Ok(())
    }
}

pub(crate) fn register(registry: &mut ConsoleRegistry) {
    for name in [
        "set",
        "seta",
        "setfromdvar",
        "ui_create_lobby",
        "ui_leave_lobby",
        "ui_start_match",
        "ui_lobby_privacy",
        "ui_vote_skip",
        "ui_maps",
        "ui_map_pack",
        "ui_map_hover",
        "ui_select_map",
        "ui_select_mode",
        "ui_browser_refresh",
        "ui_browser_page",
        "ui_join_lobby",
        "ui_join_lobby_id",
    ] {
        if registry.resolve(name).is_none() {
            registry.register(CommandSpec::new(name));
        }
    }
}

fn change_page(page: &mut usize, command: &ConsoleCommand, len: usize) {
    let delta = command
        .args
        .first()
        .and_then(|arg| arg.parse::<isize>().ok())
        .unwrap_or(0);
    *page = page
        .saturating_add_signed(delta)
        .min(len.saturating_sub(1) / PAGE_SIZE);
}

pub(crate) fn route(
    mut events: MessageReader<ConsoleCommand>,
    mut returned: MessageReader<frame::ReturnedToMenu>,
    mut commands: Commands,
    mut dvars: ResMut<UiMenuDvars>,
    mut party: ResMut<UiPartyState>,
    mut menus: MessageWriter<UiMenuRequest>,
    mut transition: ResMut<session::SessionSwapRequest>,
    mut maps: ResMut<ui::MenuMapList>,
    settings: Res<frame::GameSettings>,
    localize: Option<Res<asset_game::LocalizeCatalog>>,
    catalog: Option<Res<asset_game::MenuCatalog>>,
    mut services: LobbyServices,
    mut state: Local<FrontendState>,
    mut echo: crate::feature_dispatch::ConsoleEcho,
) {
    let mut returned_from_world = false;
    let mut returned_in_menu = false;
    for fact in returned.read() {
        returned_from_world |= fact.had_world;
        returned_in_menu |= !fact.had_world;
    }
    if returned_from_world {
        commands.remove_resource::<frame::HostMatchRules>();
        *party = UiPartyState::default();
        state.public = false;
    } else if returned_in_menu && state.public {
        let reason = match services.bridge.as_ref().map(|bridge| bridge.state()) {
            Some(net::MasterBridgeState::Failed { error, .. }) => format!("{error:?}"),
            Some(net::MasterBridgeState::Closed { reason, .. }) => {
                format!("Lobby closed: {reason:?}")
            }
            _ => "Public lobby ended".into(),
        };
        state.public = false;
        if !party.is_host {
            *party = UiPartyState::default();
            menus.write(UiMenuRequest::Close("game_lobby".into()));
        }
        dvars.set("ui_frontend_status", reason);
    }
    if dvars.get("ui_mapname").is_none()
        && let Some(map) = maps.maps().find(|map| map.starts_with("iw4:"))
    {
        dvars.set("ui_mapname", map.clone());
    }
    if dvars.get("ui_gametype").is_none() {
        dvars.set(
            "ui_gametype",
            sim::HostGameModeSelection::from_env().token(),
        );
    }
    if !state.rules_seeded
        && let Some(config) = catalog.as_ref().and_then(|c| c.rawfile_text(MATCH_CONFIG))
    {
        seed_rules(&mut dvars, config);
        state.rules_seeded = true;
    }
    for command in events.read() {
        let result = (|| -> Result<(), String> {
            match command.name.as_str() {
                "set" | "seta" => {
                    if let [name, values @ ..] = command.args.as_slice() {
                        if values.is_empty() {
                            echo.write(format!("{name} = {}", dvars.get(name).unwrap_or_default()));
                        } else {
                            dvars.set(name, values.join(" "));
                        }
                    }
                }
                "setfromdvar" => {
                    if let [name, source] = command.args.as_slice()
                        && let Some(value) = dvars.get(source).map(str::to_owned)
                    {
                        dvars.set(name, value);
                    }
                }
                "ui_create_lobby" => {
                    selected_game(&dvars, &maps)?;
                    party.active = true;
                    party.in_lobby = true;
                    party.is_host = true;
                    state.public = false;
                    dvars.set("ui_frontend_status", "");
                    menus.write(UiMenuRequest::Open("game_lobby".into()));
                }
                "ui_leave_lobby" => {
                    if state.public {
                        services.submit(net::MasterMenuAction::LeaveLobby)?;
                    }
                    *party = UiPartyState::default();
                    state.public = false;
                    menus.write(UiMenuRequest::Close("game_lobby".into()));
                }
                "ui_lobby_privacy" => {
                    if !party.in_lobby || !party.is_host {
                        return Err("Only the host can change lobby privacy".into());
                    }
                    if state.public {
                        services.submit(net::MasterMenuAction::LeaveLobby)?;
                    } else {
                        let (map, mode) = selected_game(&dvars, &maps)?;
                        services.submit(net::MasterMenuAction::Host {
                            map,
                            mode: mode.token().into(),
                        })?;
                    }
                    state.public = !state.public;
                }
                "ui_start_match" => {
                    if !party.in_lobby || !party.is_host {
                        return Err("Only the lobby host can start a match".into());
                    }
                    let (map, mode) = selected_game(&dvars, &maps)?;
                    commands.insert_resource(host_rules(&dvars));
                    if state.public {
                        services.submit(net::MasterMenuAction::StartMatch {
                            map,
                            mode: mode.token().into(),
                        })?;
                    } else {
                        let id = transition
                            .request_zone(map.clone())
                            .map_err(|error| error.to_string())?;
                        commands.insert_resource(mode);
                        echo.write(format!(
                            "menu: starting {map} {} (swap #{id})",
                            mode.token()
                        ));
                    }
                }
                "ui_vote_skip" => services.submit(net::MasterMenuAction::VoteToSkip)?,
                "ui_maps" => {
                    maps.refresh_minecraft_worlds();
                    let map = dvars.get("ui_mapname").unwrap_or_default();
                    state.map_pack = maps.pack_of(map).unwrap_or(0);
                    let index = pack_maps(&maps, state.map_pack)
                        .iter()
                        .position(|installed| installed == map)
                        .unwrap_or(0);
                    state.map_page = index / PAGE_SIZE;
                    state.map_hover = index % PAGE_SIZE;
                }
                "ui_map_pack" => {
                    let slot = command
                        .args
                        .first()
                        .and_then(|arg| arg.parse::<usize>().ok())
                        .ok_or("Invalid map page")?;
                    let (pack, page, _) = map_pages(&maps)
                        .into_iter()
                        .nth(slot)
                        .ok_or("Invalid map page")?;
                    state.map_pack = pack;
                    state.map_page = page;
                    state.map_hover = 0;
                }
                "ui_map_hover" => {
                    state.map_hover = command
                        .args
                        .first()
                        .and_then(|s| s.parse().ok())
                        .filter(|row| *row < PAGE_SIZE)
                        .ok_or("Invalid map row")?;
                }
                "ui_select_map" | "ui_select_mode" => {
                    if !party.in_lobby || !party.is_host {
                        return Err("Only the lobby host can change game setup".into());
                    }
                    let (mut map, mut mode) = selected_game(&dvars, &maps)?;
                    let selecting_map = command.name == "ui_select_map";
                    if selecting_map {
                        let row = command
                            .args
                            .first()
                            .and_then(|arg| arg.parse::<usize>().ok())
                            .filter(|row| *row < PAGE_SIZE)
                            .ok_or("Invalid map row")?;
                        map = pack_maps(&maps, state.map_pack)
                            .get(state.map_page * PAGE_SIZE + row)
                            .ok_or("Map is unavailable")?
                            .clone();
                    } else {
                        mode = command
                            .args
                            .first()
                            .and_then(|arg| sim::HostGameModeSelection::from_token(arg))
                            .ok_or("Unsupported game mode")?;
                    }
                    if state.public {
                        services.submit(net::MasterMenuAction::UpdateLobby {
                            map: map.clone(),
                            mode: mode.token().into(),
                        })?;
                    }
                    dvars.set("ui_mapname", map);
                    dvars.set("ui_gametype", mode.token());
                    menus.write(UiMenuRequest::Close(
                        if selecting_map {
                            "game_map_select"
                        } else {
                            "game_mode_select"
                        }
                        .into(),
                    ));
                }
                "ui_browser_refresh" => {
                    services.submit(net::MasterMenuAction::Refresh)?;
                    state.browser_page = 0;
                    dvars.set("ui_browser_status", "Refreshing lobbies...");
                }
                "ui_browser_page" => {
                    let len = services
                        .browser
                        .as_ref()
                        .map_or(0, |browser| browser.snapshot().adverts.len());
                    change_page(&mut state.browser_page, command, len);
                }
                "ui_join_lobby" | "ui_join_lobby_id" => {
                    let (advert_id, map, mode) = if command.name == "ui_join_lobby_id" {
                        let advert_id = command
                            .args
                            .first()
                            .filter(|id| id.is_ascii())
                            .ok_or("usage: ui_join_lobby_id <hex ID>")?
                            .parse()
                            .map_err(|_| "Invalid lobby ID")?;
                        let (map, mode) = selected_game(&dvars, &maps)?;
                        (advert_id, map, mode.token().into())
                    } else {
                        let row = command
                            .args
                            .first()
                            .and_then(|arg| arg.parse::<usize>().ok())
                            .ok_or("Invalid lobby row")?;
                        let advert = state
                            .adverts
                            .get(row)
                            .ok_or("Lobby is no longer available")?;
                        if advert.locked
                            || advert.in_match
                            || advert.players >= advert.max_players
                            || !advert.missing.is_empty()
                        {
                            return Err("Lobby is unavailable or requires missing content".into());
                        }
                        (advert.id, advert.map.clone(), advert.mode.clone())
                    };
                    services.submit(net::MasterMenuAction::Join {
                        advert_id,
                        map,
                        mode,
                    })?;
                    state.public = true;
                    party.active = true;
                    party.in_lobby = true;
                    party.is_host = false;
                    dvars.set("ui_frontend_status", "Joining lobby...");
                    menus.write(UiMenuRequest::Open("game_lobby".into()));
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(error) = result {
            let status =
                if command.name.starts_with("ui_browser") || command.name == "ui_join_lobby" {
                    "ui_browser_status"
                } else {
                    "ui_frontend_status"
                };
            dvars.set(status, &error);
            echo.write(format!("menu: {error}"));
        }
    }
    let mut lobby_names = vec![settings.player_name.clone()];
    if state.public
        && let Some(bridge) = services.bridge.as_ref()
    {
        match bridge.state() {
            net::MasterBridgeState::Hosting {
                map,
                mode,
                members,
                member_names,
                max_players,
                ..
            }
            | net::MasterBridgeState::Joined {
                map,
                mode,
                members,
                member_names,
                max_players,
                ..
            } => {
                lobby_names = members
                    .iter()
                    .map(|id| {
                        member_names
                            .get(id)
                            .cloned()
                            .unwrap_or_else(|| "Player".into())
                    })
                    .collect();
                if dvars.get("ui_frontend_status") == Some("Joining lobby...") {
                    dvars.set("ui_frontend_status", "");
                }
                dvars.set("ui_mapname", map);
                dvars.set("ui_gametype", mode);
                dvars.set(
                    "ui_lobby_players",
                    format!("PLAYERS: {} / {max_players}", members.len()),
                );
            }
            net::MasterBridgeState::Failed { error, .. } => {
                dvars.set("ui_frontend_status", format!("{error:?}"))
            }
            net::MasterBridgeState::Closed { reason, .. } => {
                dvars.set("ui_frontend_status", format!("Lobby closed: {reason:?}"))
            }
            _ => {}
        }
    } else {
        dvars.set("ui_lobby_players", "PLAYERS: 1 / 18");
    }
    for row in 0..18 {
        let name = lobby_names.get(row);
        dvars.set(
            &format!("ui_lobby_member_{row}"),
            name.map(String::as_str).unwrap_or(""),
        );
        dvars.set(
            &format!("ui_lobby_member_{row}_visible"),
            if name.is_some() { "1" } else { "0" },
        );
    }
    dvars.set("ui_lobby_host", if party.is_host { "1" } else { "0" });
    dvars.set("ui_lobby_public", if state.public { "1" } else { "0" });
    dvars.set(
        "ui_lobby_privacy",
        localize
            .as_ref()
            .and_then(|loc| {
                loc.text(if state.public {
                    "MPUI_LOBBY"
                } else {
                    "MPUI_PRIVATE_MATCH_LOBBY"
                })
            })
            .unwrap_or(if state.public {
                "PUBLIC LOBBY"
            } else {
                "PRIVATE LOBBY"
            }),
    );
    let selected_label = map_label(dvars.get("ui_mapname").unwrap_or_default());
    dvars.set("ui_map_label", selected_label);
    let preview = map_preview(dvars.get("ui_mapname").unwrap_or_default());
    dvars.set("ui_lobby_preview", preview);
    let mode_label = dvars
        .get("ui_gametype")
        .and_then(sim::HostGameModeSelection::from_token)
        .map_or("", |mode| mode.display_name());
    dvars.set("ui_mode_label", mode_label);
    let pack = pack_maps(&maps, state.map_pack);
    let hovered_map = pack.get(state.map_page * PAGE_SIZE + state.map_hover);
    dvars.set(
        "ui_map_preview_title",
        hovered_map.map_or_else(String::new, |map| map_label(map)),
    );
    dvars.set(
        "ui_map_preview",
        hovered_map.map_or_else(String::new, |map| map_preview(map)),
    );
    for row in 0..PAGE_SIZE {
        dvars.set(
            &format!("ui_map_{row}"),
            pack.get(state.map_page * PAGE_SIZE + row)
                .map(|map| map_label(map))
                .unwrap_or_default(),
        );
    }
    let pages = map_pages(&maps);
    for slot in 0..PACK_SLOTS {
        let page = pages.get(slot);
        dvars.set(
            &format!("ui_map_pack_{slot}"),
            page.map_or("", |(_, _, label)| label.as_str()),
        );
        dvars.set(
            &format!("ui_map_pack_tint_{slot}"),
            if page
                .is_some_and(|(pack, page, _)| *pack == state.map_pack && *page == state.map_page)
            {
                "1"
            } else {
                "0.45"
            },
        );
    }
    if let Some(browser) = services.browser.as_ref() {
        let snapshot = browser.snapshot();
        state.browser_page = state
            .browser_page
            .min(snapshot.adverts.len().saturating_sub(1) / PAGE_SIZE);
        state.adverts = snapshot
            .adverts
            .into_iter()
            .skip(state.browser_page * PAGE_SIZE)
            .take(PAGE_SIZE)
            .collect();
        if let Some(error) = snapshot.error {
            dvars.set(
                "ui_browser_status",
                format!("Could not refresh lobbies: {error}"),
            );
        } else if !snapshot.loading {
            dvars.set(
                "ui_browser_status",
                if state.adverts.is_empty() {
                    "No lobbies found"
                } else {
                    ""
                },
            );
        }
    }
    for row in 0..PAGE_SIZE {
        let label = state
            .adverts
            .get(row)
            .map(|advert| {
                format!(
                    "{}  {}  {}/{}",
                    advert.name, advert.map, advert.players, advert.max_players
                )
            })
            .unwrap_or_default();
        dvars.set(&format!("ui_browser_{row}"), label);
    }
}

fn map_pages(maps: &ui::MenuMapList) -> Vec<(usize, usize, String)> {
    maps.0
        .iter()
        .enumerate()
        .flat_map(|(index, pack)| {
            let pages = pack.maps.len().div_ceil(PAGE_SIZE);
            (0..pages).map(move |page| {
                (
                    index,
                    page,
                    if pages > 1 {
                        format!("{} {}", pack.label, page + 1)
                    } else {
                        pack.label.clone()
                    },
                )
            })
        })
        .collect()
}
