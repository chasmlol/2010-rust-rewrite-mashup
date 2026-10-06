mod host;
mod runtime;
mod script;
mod state;

use runtime::Runner;
use state::OpenMenu;
pub use state::ScriptMenus;

use std::collections::HashMap;

use asset_game::{MenuCatalog, MenuDef, SessionTeamSettings};
use assets::PreparedLocalizedStrings;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use frame::{
    AppScreen, MatchTornDown, RuntimeRole, UiExecCommand, UiMenuKey, UiMenuRequest, UiPlaySound,
};
use net::{
    ActionRequestIds, ClientActionInbox, ClientActionInput, LocalPresentClient, PresentedSnapshot,
};

use crate::chrome::{
    ChromeAssets, ChromeFrame, ChromeGapKind, ChromeMenuAnim, MenuVisOnError, OwnerDrawArgs,
    OwnerDrawPaint, execute_chrome_menu_ex, item_screen_rects, push_owner_pic,
};
use crate::draw2d::{Draw2dList, Draw2dOp, tessellate_fonts};
use crate::expr_cache::MenuExprCache;
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImages;
use crate::playercard::UiLocalVars;
use host::{MenuHost, MenuWorld};
const WINDOW_DECORATION: i32 = 0x0010_0000;
pub(super) const DVAR_FOCUS: i32 = 0x10;
pub(super) const MAX_SCRIPT_DEPTH: u32 = 16;
const MAIN_MENU_DVAR: &str = "g_scriptMainMenu";
const FALLBACK_MAIN_MENU: &str = "class";
const SCOREBOARD_MENU: &str = "scoreboard";

pub(super) fn is_focusable(item: &asset_game::MenuItem) -> bool {
    if item.static_flags & WINDOW_DECORATION != 0 {
        return false;
    }
    let h = &item.handlers;
    item.item_type != 0
        || !h.action.is_empty()
        || !h.accept.is_empty()
        || !h.focus.is_empty()
        || !h.mouse_enter.is_empty()
}

pub(super) fn item_rect(item: &asset_game::MenuItem) -> [f32; 4] {
    [item.rect.x, item.rect.y, item.rect.w, item.rect.h]
}

pub(super) fn choice_index(choices: &[(String, String)], value: &str) -> Option<usize> {
    let number = value.trim().parse::<f32>().ok();
    choices.iter().position(|(_, choice)| {
        choice == value || number.is_some_and(|n| choice.trim().parse::<f32>() == Ok(n))
    })
}

fn painted_def(def: &MenuDef, open: &OpenMenu) -> MenuDef {
    let (mut out, _) = crate::killcam_skip::inherit_shared_vis(def);
    for (item, state) in out.items.iter_mut().zip(&open.items) {
        if state.hidden {
            item.vis_exp = String::from("0");
        }
        if let Some(color) = state.fore {
            item.fore_color = color;
        }
        if let Some(color) = state.back {
            item.back_color = color;
        }
    }
    if let Some(color) = def.focus_color.filter(|color| color[3] > 0.0)
        && let Some(item) = open.focus.and_then(|index| out.items.get_mut(index))
        && (!item.text_key.is_empty() || !item.text_exp.is_empty())
    {
        item.fore_color = color;
    }
    out
}

#[derive(Component)]
pub(crate) struct ScriptMenuRaster;

pub(crate) fn spawn_script_menus(root: &mut ChildSpawnerCommands) {
    root.spawn((
        ScriptMenuRaster,
        crate::gpu_list::GpuListLatch::default(),
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        FocusPolicy::Pass,
    ));
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct MenuInputs<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    keyboard: MessageReader<'w, 's, bevy::input::keyboard::KeyboardInput>,
    mouse: Res<'w, ButtonInput<MouseButton>>,
    windows: Query<'w, 's, &'static Window, With<bevy::window::PrimaryWindow>>,
    hud_input: Option<ResMut<'w, frame::HudInputView>>,
    screen: Res<'w, AppScreen>,
    actions: Option<Res<'w, ClientActionInput>>,
    requests: MessageReader<'w, 's, UiMenuRequest>,
    classes: Option<Res<'w, frame::HostClassLoadouts>>,
    weapons: Option<Res<'w, assets::PreparedWeapons>>,
    compass: Option<Res<'w, assets::SessionCompass>>,
    party: Res<'w, frame::UiPartyState>,
    frontend_strings: Option<Res<'w, asset_game::LocalizeCatalog>>,
}

#[derive(Default)]
struct Pressed {
    escape: bool,
    enter: bool,
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    home: bool,
    end: bool,
    backspace: bool,
    delete: bool,
    text: Vec<String>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct MenuOutputs<'w> {
    dvars: ResMut<'w, frame::UiMenuDvars>,
    binding: ResMut<'w, frame::UiBindingCapture>,
    binds: MessageWriter<'w, frame::UiBindRequest>,
    sounds: MessageWriter<'w, UiPlaySound>,
    music: MessageWriter<'w, frame::UiPlayMusic>,
    stop_music: MessageWriter<'w, frame::UiStopMusic>,
    exec: MessageWriter<'w, UiExecCommand>,
    inbox: Option<ResMut<'w, ClientActionInbox>>,
    ids: Option<ResMut<'w, ActionRequestIds>>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_script_menus(
    surface: Res<crate::surface::Hud2dSurface>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    teams: Option<Res<SessionTeamSettings>>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    role: Option<Res<RuntimeRole>>,
    mut input: MenuInputs,
    mut torn: MessageReader<MatchTornDown>,
    mut menus: ResMut<ScriptMenus>,
    mut locals: ResMut<UiLocalVars>,
    mut exprs: ResMut<MenuExprCache>,
    mut out: MenuOutputs,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut pass: ResMut<HudTessPass>,
) {
    pass.script_menus = TessJob::Hide;
    if torn.read().count() > 0 {
        out.binding.command = None;
        *menus = ScriptMenus::default();
    }
    let requests: Vec<UiMenuRequest> = input.requests.read().cloned().collect();
    let Some(catalog) = catalog.as_deref() else {
        return;
    };
    let snapshot = presented.snapshot();
    let meta = snapshot.and_then(|s| s.meta.for_client(local.0));
    let in_game = matches!(*input.screen, AppScreen::InGame | AppScreen::ClassSelect);
    let frontend = *input.screen == AppScreen::MainMenu;
    if (!in_game && !frontend) || (in_game && meta.is_none()) {
        if menus.screen.is_some() {
            out.stop_music.write(frame::UiStopMusic);
            out.binding.command = None;
            *menus = ScriptMenus::default();
        }
        if let Some(view) = input.hud_input.as_mut()
            && view.script_menu_open
        {
            view.script_menu_open = false;
        }
        return;
    }
    let screen_changed = menus.screen.map(|s| s == AppScreen::MainMenu) != Some(frontend);
    if screen_changed {
        out.binding.command = None;
        *menus = ScriptMenus {
            screen: Some(*input.screen),
            ..default()
        };
        out.stop_music.write(frame::UiStopMusic);
    }
    let localize = if in_game {
        strings.as_ref().map(|s| &s.0)
    } else {
        input.frontend_strings.as_deref()
    };
    let world = MenuWorld {
        ms: crate::scorebar::milliseconds() as i32,
        in_game,
        party: &input.party,
        dvars: snapshot
            .filter(|_| in_game)
            .map(|s| s.meta.script_dvars(local.0)),
        sv_running: in_game && role.is_some_and(|r| r.runs_authority()),
        catalog: Some(catalog),
        localize,
        kind: snapshot.map(|s| s.meta.kind),
        team: meta.map_or(0, |m| m.client_state_team),
        team_scores: snapshot.map_or([0; 3], |s| s.meta.objectives.scores),
        player_score: meta.map_or(0, |m| m.score),
        time_left_s: snapshot.map_or(0, |s| {
            let now_ms = s.tick.0.saturating_mul(sim::MATCH_TICK_MS) as i32;
            s.meta.objectives.time_left_ms(now_ms).div_euclid(1000)
        }),
        teams: teams.as_ref().map(|t| &t.0),
        scores_open: in_game
            && input
                .actions
                .as_ref()
                .is_some_and(|a| a.client.kb.scores.active),
        classes: input.classes.as_deref(),
        weapons: input.weapons.as_ref().map(|w| w.registry().as_ref()),
        emp_jammed: in_game
            && presented
                .player(local.0)
                .is_some_and(|ps| ps.other_flags & playerstate_iw4::other_flags::EMP_JAMMED != 0),
        radar_blocked: in_game
            && presented
                .snapshot()
                .and_then(|snap| snap.meta.for_client(local.0))
                .is_some_and(|meta| meta.radar_blocked),
    };

    let mut runner = Runner {
        catalog,
        world: &world,
        menus: &mut menus,
        locals: &mut locals,
        exprs: &mut exprs,
        dvars: &mut out.dvars,
        depth: 0,
        binding: &mut out.binding,
    };

    if frontend && screen_changed {
        runner.open("iw4l_main");
        if let Some(def) = catalog.get("main")
            && !def.sound_name.is_empty()
        {
            out.music.write(frame::UiPlayMusic {
                alias: def.sound_name.clone(),
            });
        }
    }

    for command in meta
        .into_iter()
        .filter(|_| in_game)
        .flat_map(|m| &m.menu_commands)
    {
        if command.serial <= runner.menus.applied_serial {
            continue;
        }
        runner.menus.applied_serial = command.serial;
        match &command.kind {
            sim::MenuCommandKind::Open(name) => runner.open(name),
            sim::MenuCommandKind::ClosePopup => {
                if let Some(top) = runner.menus.input_menu().map(|m| m.name.clone()) {
                    runner.close(&top);
                }
            }
            sim::MenuCommandKind::CloseInGame => runner.close_all(),
        }
    }

    let console_open = input.hud_input.as_ref().is_some_and(|h| h.console_open);
    let mut pressed = Pressed::default();
    for request in requests {
        match request {
            UiMenuRequest::Toggle => pressed.escape = true,
            UiMenuRequest::Open(name) => runner.open(&name),
            UiMenuRequest::Close(name) => runner.close(&name),
            UiMenuRequest::Focus { menu, item } => {
                if let Some(index) = runner.catalog.get(&menu).and_then(|definition| {
                    definition
                        .items
                        .iter()
                        .position(|candidate| candidate.name == item)
                }) {
                    runner.set_focus(&menu, index);
                }
            }
            UiMenuRequest::Key(UiMenuKey::Escape) => pressed.escape = true,
            UiMenuRequest::Key(UiMenuKey::Enter) => pressed.enter = true,
            UiMenuRequest::Key(UiMenuKey::Up) => pressed.up = true,
            UiMenuRequest::Key(UiMenuKey::Down) => pressed.down = true,
            UiMenuRequest::Key(UiMenuKey::Left) => pressed.left = true,
            UiMenuRequest::Key(UiMenuKey::Right) => pressed.right = true,
            UiMenuRequest::Key(UiMenuKey::Home) => pressed.home = true,
            UiMenuRequest::Key(UiMenuKey::End) => pressed.end = true,
            UiMenuRequest::Key(UiMenuKey::Backspace) => pressed.backspace = true,
            UiMenuRequest::Key(UiMenuKey::Delete) => pressed.delete = true,
            UiMenuRequest::Text(text) => pressed.text.push(text),
        }
    }
    // Reopen after a teardown resets the stack; only acknowledgement clears
    // the notice. Opening last also keeps keyboard/mouse focus on the dialog.
    if frontend && runner.dvars.get("ui_connection_error") == Some("1") {
        runner.open("iw4l_connection_error");
    }
    if frontend && runner.dvars.get("ui_map_error") == Some("1") {
        runner.open("iw4l_map_error");
    }
    let pointer = !console_open;
    if pointer {
        let keys = &input.keys;
        pressed.escape |= keys.just_pressed(KeyCode::Escape);
        pressed.enter |=
            keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter);
        pressed.up |= keys.just_pressed(KeyCode::ArrowUp);
        pressed.down |= keys.just_pressed(KeyCode::ArrowDown);
        pressed.left |= keys.just_pressed(KeyCode::ArrowLeft);
        pressed.right |= keys.just_pressed(KeyCode::ArrowRight);
        pressed.home |= keys.just_pressed(KeyCode::Home);
        pressed.end |= keys.just_pressed(KeyCode::End);
        pressed.backspace |= keys.just_pressed(KeyCode::Backspace);
        pressed.delete |= keys.just_pressed(KeyCode::Delete);
    }
    for event in input.keyboard.read() {
        if pointer
            && event.state.is_pressed()
            && let Some(text) = &event.text
        {
            pressed.text.push(text.to_string());
        }
    }
    if runner.menus.binding_menu.as_ref().is_some_and(|name| {
        runner
            .menus
            .input_menu()
            .is_none_or(|top| &top.name != name)
    }) {
        runner.binding.command = None;
        runner.menus.binding_menu = None;
    }
    handle_input(&mut runner, &input, &surface, &pressed, pointer);

    flush_outputs(&mut menus, &mut out, local.0);
    if let Some(view) = input.hud_input.as_mut() {
        let open = menus.captures_input();
        if view.script_menu_open != open {
            view.script_menu_open = open;
        }
    }

    let scoreboard = snapshot
        .filter(|s| in_game && crate::scoreboard::displayed(world.scores_open, s, local.0))
        .and_then(|_| catalog.get(SCOREBOARD_MENU));
    if (menus.stack.is_empty() && scoreboard.is_none()) || !surface.is_ready() {
        return;
    }
    let open_names = menus.open_names();
    let mut list = Draw2dList::default();
    let mut reported = std::mem::take(&mut menus.reported);
    let mut layers: Vec<(String, std::borrow::Cow<MenuDef>, Option<[f32; 4]>)> = Vec::new();
    if let Some(def) = scoreboard {
        layers.push((
            SCOREBOARD_MENU.to_owned(),
            std::borrow::Cow::Borrowed(def),
            None,
        ));
    }
    let paint_start = menus
        .stack
        .iter()
        .rposition(|open| {
            catalog
                .get(&open.name)
                .is_some_and(|def| def.fullscreen != 0)
        })
        .unwrap_or(0);
    for open in &menus.stack[paint_start..] {
        let Some(def) = catalog.get(&open.name) else {
            continue;
        };
        let mut painted = painted_def(def, open);
        let mut slider_parts = Vec::new();
        for item in &mut painted.items {
            if item.item_type == 14 && !item.dvar.is_empty() {
                item.text_key = out
                    .dvars
                    .get(&format!("ui_bind_{}", item.dvar))
                    .unwrap_or("UNBOUND")
                    .to_owned();
                if out.binding.command.as_deref() == Some(item.dvar.as_str()) {
                    item.text_key = "... (ESC)".to_owned();
                }
                item.text_literal = true;
                item.text_exp.clear();
                item.text_align_mode = 10;
                item.text_align_x = -8.0;
                item.text_align_y = 0.0;
            }
            if item.item_type == 4 && !item.dvar.is_empty() {
                item.text_key = out.dvars.get(&item.dvar).unwrap_or("").to_owned();
                if item.edit_field.as_ref().is_some_and(|field| field.masked) {
                    item.text_key = "*".repeat(item.text_key.chars().count());
                }
                item.text_literal = true;
                item.text_exp.clear();
            }
            if let Some(slider) = &item.slider {
                let value = out
                    .dvars
                    .get(&item.dvar)
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(slider.min);
                let fraction = ((value - slider.min) / (slider.max - slider.min)).clamp(0.0, 1.0);
                for (x, y, w, h, color) in [
                    (0.0, 9.0, item.rect.w, 2.0, [0.6, 0.6, 0.6, 1.0]),
                    (
                        fraction * (item.rect.w - 6.0),
                        3.0,
                        6.0,
                        14.0,
                        [1.0, 1.0, 1.0, 1.0],
                    ),
                ] {
                    let mut part = item.clone();
                    part.slider = None;
                    part.dvar.clear();
                    part.text_key.clear();
                    part.text_exp.clear();
                    part.item_type = 0;
                    part.static_flags |= WINDOW_DECORATION;
                    part.style = 1;
                    part.background = "white".into();
                    part.back_color = color;
                    part.rect.x += x;
                    part.rect.y += y;
                    part.rect.w = w;
                    part.rect.h = h;
                    slider_parts.push(part);
                }
                let mut label = item.clone();
                label.slider = None;
                label.dvar.clear();
                label.item_type = 0;
                label.static_flags |= WINDOW_DECORATION;
                label.background.clear();
                label.back_color = [0.0; 4];
                label.text_exp.clear();
                label.text_literal = true;
                let displayed = slider
                    .display_range
                    .map_or(value, |[min, max]| min + fraction * (max - min));
                let decimals = usize::from(slider.decimals.min(6));
                label.text_key = format!("{displayed:.decimals$}{}", slider.suffix);
                label.rect.x += item.rect.w + 8.0;
                label.rect.w = 36.0;
                slider_parts.push(label);
                item.background.clear();
                item.text_key.clear();
                item.text_exp.clear();
            }
            if !item.choices.is_empty() {
                let value = out.dvars.get(&item.dvar).unwrap_or_default();
                item.text_key = choice_index(&item.choices, value)
                    .map(|at| item.choices[at].0.clone())
                    .unwrap_or_else(|| value.to_owned());
                item.text_exp.clear();
            }
        }
        painted.items.extend(slider_parts);
        if let Some(edit) = &menus.editing
            && edit.menu == open.name
            && let Some(item) = painted.items.get_mut(edit.item)
        {
            let mut text = if item.edit_field.as_ref().is_some_and(|field| field.masked) {
                vec!['*'; edit.buffer.len()]
            } else {
                edit.buffer.clone()
            };
            text.insert(edit.cursor.min(text.len()), '|');
            item.text_key = text.into_iter().collect();
            item.text_exp.clear();
        }
        let focus_rect = open.focus.and_then(|i| painted.items.get(i)).map(item_rect);
        layers.push((
            open.name.clone(),
            std::borrow::Cow::Owned(painted),
            focus_rect,
        ));
    }
    for (name, painted, focus_rect) in &layers {
        let host = MenuHost {
            world: &world,
            dvars: &out.dvars,
            menu: painted,
            locals: &locals,
            open: &open_names,
            focus_rect: *focus_rect,
        };
        let mut owner_draw = |args: OwnerDrawArgs<'_>, frame: &mut ChromeFrame| {
            if args.item.owner_draw == 181 {
                if let Some(map_namespace) = hud_images.map_namespace()
                    && let Some(image) = input
                        .compass
                        .as_ref()
                        .and_then(|c| c.declaration.image.as_ref())
                {
                    push_owner_pic(
                        &args,
                        image.clone(),
                        map_namespace,
                        args.color,
                        Draw2dOp::StretchPic,
                        frame,
                    );
                    return OwnerDrawPaint::Painted;
                }
            }
            OwnerDrawPaint::Gap(ChromeGapKind::OwnerDraw)
        };
        let frame = execute_chrome_menu_ex(
            painted,
            &host,
            &surface,
            ChromeAssets {
                catalog: Some(catalog),
                localize,
            },
            ChromeMenuAnim::IDENTITY,
            &mut exprs,
            MenuVisOnError::PaintAnyway,
            Some(&mut owner_draw),
        );
        if frame.coverage.typed_gap > 0 && !reported.contains(name) {
            reported.insert(name.clone());
            let gaps: Vec<String> = frame
                .coverage
                .gap_ids
                .iter()
                .map(|(index, kind)| {
                    let item = &painted.items[*index];
                    let error = frame
                        .vis_errors
                        .iter()
                        .find(|(i, _)| i == index)
                        .map_or("", |(_, e)| e.as_str());
                    let text = if item.text_exp.is_empty() {
                        item.text_key.clone()
                    } else {
                        format!("{:?}", exprs.evaluate_string(&item.text_exp, &host))
                    };
                    format!("{index}:{}:{kind:?}{error}[{text}]", item.name)
                })
                .collect();
            diag::info!(
                Ui,
                "menu {}: {} of {} items not painted: {}",
                name,
                gaps.len(),
                painted.items.len(),
                gaps.join(" ")
            );
        }
        list.cmds.extend(frame.list.cmds);
    }
    menus.reported = reported;
    let mut fonts: HashMap<String, &asset_game::FontDef> = HashMap::new();
    for cmd in &list.cmds {
        let _ = hud_images.get_native(cmd.material_namespace, &cmd.material, &mut images);
        if let Draw2dOp::TextRun { font, .. } = &cmd.op
            && !fonts.contains_key(font)
            && let Some(def) = catalog.font(font)
        {
            fonts.insert(font.clone(), def);
        }
    }
    let (quads, _) = tessellate_fonts(&list, &fonts);
    if !quads.is_empty() {
        pass.script_menus = TessJob::Quads(quads);
    }
}

fn handle_input(
    runner: &mut Runner<'_, '_>,
    input: &MenuInputs,
    surface: &crate::surface::Hud2dSurface,
    pressed: &Pressed,
    pointer: bool,
) {
    if runner.binding.command.is_some() || runner.binding.consumed_input {
        if pressed.escape {
            runner.binding.command = None;
            runner.binding.consumed_input = true;
        }
        return;
    }
    if let Some(mut edit) = runner.menus.editing.take() {
        if pressed.escape {
            return;
        }
        if pressed.enter {
            if let Some(item) = runner
                .catalog
                .get(&edit.menu)
                .and_then(|def| def.items.get(edit.item))
            {
                runner
                    .dvars
                    .set(&item.dvar, edit.buffer.iter().collect::<String>());
                let value: String = edit.buffer.iter().collect();
                let value = value.replace('\\', "\\\\").replace('"', "\\\"");
                if !item.edit_field.as_ref().is_some_and(|field| field.masked) {
                    runner
                        .menus
                        .exec
                        .push(format!("set {} \"{}\"", item.dvar, value));
                }
                runner.run_events(&edit.menu, Some(edit.item), &item.handlers.accept);
            }
            return;
        }
        if pressed.home {
            edit.cursor = 0;
        }
        if pressed.end {
            edit.cursor = edit.buffer.len();
        }
        if pressed.left {
            edit.cursor = edit.cursor.saturating_sub(1);
        }
        if pressed.right {
            edit.cursor = (edit.cursor + 1).min(edit.buffer.len());
        }
        if pressed.backspace && edit.cursor > 0 {
            edit.cursor -= 1;
            edit.buffer.remove(edit.cursor);
        }
        if pressed.delete && edit.cursor < edit.buffer.len() {
            edit.buffer.remove(edit.cursor);
        }
        for text in &pressed.text {
            for ch in text.chars().filter(|ch| !ch.is_control()) {
                if edit.buffer.len() >= edit.max_chars {
                    break;
                }
                edit.buffer.insert(edit.cursor, ch);
                edit.cursor += 1;
            }
        }
        runner.menus.editing = Some(edit);
        return;
    }
    if pressed.escape {
        match runner.menus.input_menu().map(|m| m.name.clone()) {
            Some(top) => runner.escape(&top),
            None if !runner.world.in_game => runner.open("iw4l_main"),
            None => {
                let main = runner
                    .world
                    .dvars
                    .as_ref()
                    .and_then(|d| d.string(MAIN_MENU_DVAR))
                    .filter(|name| !name.is_empty() && runner.catalog.get(name).is_some())
                    .map(str::to_owned);
                let name = main.unwrap_or_else(|| {
                    diag::warn!(
                        Ui,
                        "togglemenu: {MAIN_MENU_DVAR} names no loaded menu, opening {FALLBACK_MAIN_MENU}"
                    );
                    FALLBACK_MAIN_MENU.to_owned()
                });
                runner.open(&name);
            }
        }
        return;
    }
    let Some(top) = runner.menus.input_menu().map(|m| m.name.clone()) else {
        return;
    };

    if pressed.down {
        runner.focus_nav(&top, 0, 1);
    }
    if pressed.up {
        runner.focus_nav(&top, 0, -1);
    }
    if pressed.left || pressed.right {
        let step = if pressed.left { -1 } else { 1 };
        let focus = runner.menus.input_menu().and_then(|m| m.focus);
        if !focus.is_some_and(|focus| runner.adjust(&top, focus, step)) {
            runner.focus_nav(&top, step, 0);
        }
        return;
    }
    if pressed.enter {
        if let Some(focus) = runner.menus.input_menu().and_then(|m| m.focus) {
            runner.activate(&top, focus, true);
        }
        return;
    }
    if !pointer {
        return;
    }

    let cursor = input
        .windows
        .single()
        .ok()
        .and_then(|w| w.cursor_position());
    let Some(cursor) = cursor else {
        return;
    };
    let moved = runner.menus.cursor.is_some_and(|old| old != cursor);
    runner.menus.cursor = Some(cursor);
    if !input.mouse.pressed(MouseButton::Left) {
        runner.menus.slider_drag = None;
    }
    if !moved && !input.mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(def) = runner.catalog.get(&top) else {
        return;
    };
    let Some(open) = runner.menus.input_menu() else {
        return;
    };
    let painted = painted_def(def, open);
    let names = runner.menus.open_names();
    let host = MenuHost {
        world: runner.world,
        dvars: runner.dvars,
        menu: &painted,
        locals: runner.locals,
        open: &names,
        focus_rect: None,
    };
    let rects = item_screen_rects(&painted, &host, surface, runner.exprs);
    let visible = runner.visible_items(&top);
    if let Some((menu, index)) = runner.menus.slider_drag.clone() {
        if menu == top && input.mouse.pressed(MouseButton::Left) {
            if let Some((_, [x0, _, x1, _])) = rects.iter().find(|(i, _)| *i == index) {
                runner.slide(&top, index, (cursor.x - x0) / (x1 - x0));
            }
            return;
        }
        runner.menus.slider_drag = None;
    }
    let hit = rects.iter().rev().find_map(|&(index, [x0, y0, x1, y1])| {
        let inside = cursor.x >= x0 && cursor.x < x1 && cursor.y >= y0 && cursor.y < y1;
        (inside && is_focusable(&def.items[index]) && visible.contains(&index)).then_some(index)
    });
    runner.set_hover(&top, hit);
    if input.mouse.just_pressed(MouseButton::Left)
        && let Some(index) = hit
    {
        if let Some((_, [x0, _, x1, _])) = rects.iter().find(|(i, _)| *i == index)
            && runner.slide(&top, index, (cursor.x - x0) / (x1 - x0))
        {
            runner.set_focus(&top, index);
            runner.menus.slider_drag = Some((top.clone(), index));
        } else {
            runner.activate(&top, index, false);
        }
    }
}

fn flush_outputs(menus: &mut ScriptMenus, out: &mut MenuOutputs, local: sim::ClientId) {
    for command in menus.bind_requests.drain(..) {
        out.binds.write(frame::UiBindRequest { command });
    }
    for alias in menus.sounds.drain(..) {
        out.sounds.write(UiPlaySound { alias });
    }
    for text in menus.exec.drain(..) {
        diag::info!(Ui, "menu exec: {text}");
        out.exec.write(UiExecCommand { text });
    }
    let (Some(inbox), Some(ids)) = (out.inbox.as_mut(), out.ids.as_mut()) else {
        menus.responses.clear();
        return;
    };
    for (menu, response) in menus.responses.drain(..) {
        if menu.eq_ignore_ascii_case("changeclass")
            && let Some(slot) = response
                .strip_prefix("custom")
                .and_then(|n| n.parse::<usize>().ok())
                .and_then(|n| n.checked_sub(1))
                .filter(|slot| *slot < sim::match_state::PERSONAL_CLASS_SLOTS)
        {
            out.exec.write(UiExecCommand {
                text: format!("spawn {slot} &"),
            });
            continue;
        }
        let (Some(menu_field), Some(response_field)) = (
            sim::menu_response_field(&menu),
            sim::menu_response_field(&response),
        ) else {
            diag::warn!(
                Ui,
                "menu response `{menu}` `{response}` does not fit the wire"
            );
            continue;
        };
        let request_id = ids.allocate();
        match inbox.push(
            local,
            sim::ClientAction::MenuResponse {
                request_id,
                menu: menu_field,
                response: response_field,
            },
        ) {
            Ok(_) => diag::info!(Ui, "menu response: {menu} {response}"),
            Err(error) => diag::warn!(
                Ui,
                "menu response `{menu}` `{response}` not queued: {error}"
            ),
        }
    }
}
