use asset_game::{MenuCatalog, MenuDef, MenuEvent};
use bevy::prelude::*;

use super::host::{MenuHost, MenuWorld};
use super::script::{self, Command};
use super::state::{EditState, ItemState, OpenMenu, ScriptMenus};
use super::{DVAR_FOCUS, MAX_SCRIPT_DEPTH, choice_index, is_focusable, item_rect};
use crate::expr_cache::MenuExprCache;
use crate::playercard::UiLocalVars;
pub(crate) struct Runner<'a, 'w> {
    pub(crate) catalog: &'a MenuCatalog,
    pub(crate) world: &'a MenuWorld<'w>,
    pub(crate) menus: &'a mut ScriptMenus,
    pub(crate) locals: &'a mut UiLocalVars,
    pub(crate) exprs: &'a mut MenuExprCache,
    pub(crate) dvars: &'a mut frame::UiMenuDvars,
    pub(crate) depth: u32,
    pub(crate) binding: &'a mut frame::UiBindingCapture,
}

impl Runner<'_, '_> {
    pub(crate) fn def(&self, name: &str) -> Option<&MenuDef> {
        self.catalog.get(name)
    }

    pub(crate) fn eval(&mut self, menu: &str, dump: &str) -> Option<hud_iw4::Operand> {
        let def = self.catalog.get(menu)?;
        let open = self.menus.open_names();
        let focus_rect = self
            .menus
            .stack
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(menu))
            .and_then(|m| m.focus)
            .and_then(|i| def.items.get(i))
            .map(item_rect);
        let host = MenuHost {
            world: self.world,
            dvars: self.dvars,
            menu: def,
            locals: self.locals,
            open: &open,
            focus_rect,
        };
        match self.exprs.evaluate(dump, &host) {
            Ok(value) => Some(value),
            Err(err) => {
                diag::debug!(Ui, "menu {menu}: expression failed {err:?}");
                None
            }
        }
    }

    pub(crate) fn run_events(&mut self, menu: &str, item: Option<usize>, events: &[MenuEvent]) {
        if self.depth >= MAX_SCRIPT_DEPTH {
            diag::warn!(Ui, "menu {menu}: script nesting too deep");
            return;
        }
        self.depth += 1;
        let mut last_if: Option<bool> = None;
        for event in events {
            match event {
                MenuEvent::Script(text) => {
                    last_if = None;
                    for command in script::parse(text) {
                        self.command(menu, item, command);
                    }
                }
                MenuEvent::If { condition, then } => {
                    let pass = self
                        .eval(menu, condition)
                        .is_some_and(|v| hud_iw4::expr::source_int(&v) != 0);
                    last_if = Some(pass);
                    if pass {
                        self.run_events(menu, item, then);
                    }
                }
                MenuEvent::Else(body) => {
                    if last_if == Some(false) {
                        self.run_events(menu, item, body);
                    }
                    last_if = None;
                }
                MenuEvent::SetLocalVar { kind, name, expr } => {
                    last_if = None;
                    let Some(value) = self.eval(menu, expr) else {
                        continue;
                    };
                    match kind {
                        3 | 4 => self.locals.set_int(name, hud_iw4::expr::source_int(&value)),
                        5 => self
                            .locals
                            .set_float(name, hud_iw4::expr::source_float(&value)),
                        _ => self
                            .locals
                            .set_string(name, hud_iw4::expr::source_str(&value)),
                    }
                }
            }
        }
        self.depth -= 1;
    }

    pub(crate) fn command(&mut self, menu: &str, item: Option<usize>, command: Command) {
        let target = |name: &str| {
            if name.eq_ignore_ascii_case("self") {
                menu.to_owned()
            } else {
                name.to_owned()
            }
        };
        match command {
            Command::Open(name) => self.open(&name),
            Command::Close(name) => self.close(&target(&name)),
            Command::Escape(name) => self.escape(&target(&name)),
            Command::SetFocus(name) => {
                let Some(def) = self.def(menu) else {
                    return;
                };
                if let Some(index) = def
                    .items
                    .iter()
                    .position(|i| i.name.eq_ignore_ascii_case(&name) && is_focusable(i))
                {
                    self.set_focus(menu, index);
                }
            }
            Command::FocusFirst => self.focus_step(menu, 1, true),
            Command::SetItemColor {
                item: name,
                field,
                color,
            } => {
                self.each_item(menu, item, &name, |state| match field.as_str() {
                    "backcolor" => state.back = Some(color),
                    "forecolor" => state.fore = Some(color),
                    _ => {}
                });
            }
            Command::Show(name) => self.each_item(menu, item, &name, |state| state.hidden = false),
            Command::Hide(name) => self.each_item(menu, item, &name, |state| state.hidden = true),
            Command::Play(alias) => self.menus.sounds.push(alias),
            Command::ScriptMenuResponse(response) => {
                self.menus.responses.push((menu.to_owned(), response));
            }
            Command::Exec(text) => self.exec(text),
            Command::CopyDvar(name, source) => {
                let value = self.dvar(&source).unwrap_or_default().to_owned();
                self.dvars.set(&name, value);
            }
            Command::SetDvar(name, value) => self.set_dvar(&name, &value),
            Command::ExecOnDvarIntValue {
                dvar,
                value,
                command,
            } => {
                let current = self
                    .dvar(&dvar)
                    .and_then(|s| s.parse::<f32>().ok())
                    .unwrap_or(0.0) as i32;
                if current == value {
                    self.exec(command);
                }
            }
            Command::ExecOnDvarStringValue {
                dvar,
                value,
                command,
            } => {
                let current = self.dvar(&dvar);
                if current.is_some_and(|c| c.eq_ignore_ascii_case(&value)) {
                    self.exec(command);
                }
            }
            Command::OpenForGameType(format) => {
                let name = self.for_game_type(menu, item, &format);
                self.open(&name);
            }
            Command::CloseForGameType(format) => {
                let name = self.for_game_type(menu, item, &format);
                self.close(&name);
            }
            Command::SetFocusByDvar(dvar) => {
                let Some(def) = self.def(menu) else {
                    return;
                };
                let found = def.items.iter().position(|i| {
                    i.dvar_flags & DVAR_FOCUS != 0
                        && i.dvar_test.eq_ignore_ascii_case(&dvar)
                        && self.enable_via_dvar(i, DVAR_FOCUS)
                        && is_focusable(i)
                });
                if let Some(index) = found {
                    self.set_focus(menu, index);
                }
            }
            Command::MenuOnDvar {
                open,
                want_match,
                dvar,
                value,
                menu: target_menu,
            } => {
                let Some(current) = self.dvar(&dvar) else {
                    diag::debug!(Ui, "menu {menu}: menu-on-dvar cannot find dvar {dvar}");
                    return;
                };
                if current.eq_ignore_ascii_case(&value) == want_match {
                    if open {
                        self.open(&target_menu);
                    } else {
                        self.close(&target(&target_menu));
                    }
                }
            }
            Command::Unhandled(name) => {
                diag::debug!(Ui, "menu {menu}: script command `{name}` not run");
            }
        }
    }

    /// `toggle` and cfg `exec` land in the menu dvars before the next script
    /// command runs, so a following `execNowOnDvarIntValue` sees the result.
    pub(crate) fn exec(&mut self, text: String) {
        let words: Vec<&str> = text
            .split_whitespace()
            .map(|word| word.trim_matches('"'))
            .collect();
        match words.as_slice() {
            ["toggle", name, values @ ..] => {
                let current = self.dvar(name).unwrap_or("0");
                let next = match values {
                    [] => {
                        if current.parse::<f32>().is_ok_and(|n| n != 0.0) {
                            "0"
                        } else {
                            "1"
                        }
                    }
                    _ => {
                        let choices: Vec<_> = values
                            .iter()
                            .map(|v| (String::new(), (*v).to_owned()))
                            .collect();
                        values
                            [choice_index(&choices, current).map_or(0, |i| (i + 1) % values.len())]
                    }
                };
                self.set_dvar(name, next);
            }
            ["exec", file] if file.ends_with(".cfg") => {
                let Some(config) = self.catalog.rawfile_text(file) else {
                    self.menus.exec.push(text);
                    return;
                };
                let defaults = self
                    .catalog
                    .rawfile_text("default_xboxlive.cfg")
                    .unwrap_or("");
                for line in config.lines() {
                    let mut words = line.split_whitespace();
                    match (words.next(), words.next(), words.next()) {
                        (Some("set" | "seta"), Some(name), value) => {
                            self.set_dvar(name, value.unwrap_or("").trim_matches('"'));
                        }
                        (Some("reset"), Some(name), _) => {
                            if let Some(value) = defaults.lines().find_map(|line| {
                                let mut words = line.split_whitespace();
                                (words.next() == Some("set")
                                    && words.next().is_some_and(|n| n.eq_ignore_ascii_case(name)))
                                .then(|| words.next().unwrap_or("").trim_matches('"'))
                            }) {
                                self.set_dvar(name, value);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => self.menus.exec.push(text),
        }
    }

    pub(crate) fn set_dvar(&mut self, name: &str, value: &str) {
        self.dvars.set(name, value);
        self.menus.exec.push(format!("set {name} \"{value}\""));
    }

    pub(crate) fn dvar(&self, name: &str) -> Option<&str> {
        self.dvars
            .get(name)
            .or_else(|| self.world.dvars.as_ref().and_then(|d| d.string(name)))
    }

    pub(crate) fn for_game_type(&self, menu: &str, item: Option<usize>, format: &str) -> String {
        let value = item
            .and_then(|i| self.def(menu)?.items.get(i))
            .and_then(|i| self.dvar(&i.dvar))
            .unwrap_or("");
        format.replacen("%s", value, 1)
    }

    pub(crate) fn enable_via_dvar(&self, item: &asset_game::MenuItem, flag: i32) -> bool {
        if item.enable_dvar.is_empty() || item.dvar_test.is_empty() {
            return true;
        }
        let value = self.dvar(&item.dvar_test).unwrap_or("");
        let listed = script::tokens(&item.enable_dvar)
            .iter()
            .any(|v| v.eq_ignore_ascii_case(value));
        listed == (item.dvar_flags & flag != 0)
    }

    pub(crate) fn each_item(
        &mut self,
        menu: &str,
        current: Option<usize>,
        name: &str,
        mut apply: impl FnMut(&mut ItemState),
    ) {
        let Some(def) = self.catalog.get(menu) else {
            return;
        };
        let Some(open) = self.menus.get_mut(menu) else {
            return;
        };
        if name.eq_ignore_ascii_case("self") {
            if let Some(state) = current.and_then(|i| open.items.get_mut(i)) {
                apply(state);
            }
            return;
        }
        for (index, item) in def.items.iter().enumerate() {
            if item.name.eq_ignore_ascii_case(name)
                && let Some(state) = open.items.get_mut(index)
            {
                apply(state);
            }
        }
    }

    pub(crate) fn open(&mut self, name: &str) {
        let catalog = self.catalog;
        let Some(def) = catalog.get(name) else {
            diag::warn!(Ui, "menu open: `{name}` is not loaded");
            return;
        };
        if let Some(pos) = self.menus.position(name) {
            let open = self.menus.stack.remove(pos);
            self.menus.stack.push(open);
            return;
        }
        self.menus.stack.push(OpenMenu {
            name: def.name.clone(),
            captures_input: def.static_flags & super::WINDOW_DECORATION == 0,
            focus: None,
            hover: None,
            items: vec![ItemState::default(); def.items.len()],
        });
        diag::info!(Ui, "menu open: {}", def.name);
        let name = def.name.clone();
        self.run_events(&name, None, &def.handlers.open);
        if self
            .menus
            .get_mut(&name)
            .is_some_and(|open| open.focus.is_none())
        {
            self.focus_step(&name, 1, true);
        }
    }

    pub(crate) fn close(&mut self, name: &str) {
        if self
            .menus
            .editing
            .as_ref()
            .is_some_and(|edit| edit.menu.eq_ignore_ascii_case(name))
        {
            self.menus.editing = None;
        }
        let catalog = self.catalog;
        if self.menus.position(name).is_none() {
            return;
        }
        if let Some(def) = catalog.get(name) {
            self.run_events(name, None, &def.handlers.close);
        }
        if let Some(pos) = self.menus.position(name) {
            self.menus.stack.remove(pos);
            diag::info!(Ui, "menu close: {name}");
        }
    }

    pub(crate) fn close_all(&mut self) {
        while let Some(top) = self.menus.stack.last().map(|m| m.name.clone()) {
            self.close(&top);
        }
    }

    pub(crate) fn escape(&mut self, name: &str) {
        let catalog = self.catalog;
        let Some(def) = catalog.get(name) else {
            return;
        };
        if def.handlers.esc.is_empty() {
            self.close(name);
        } else {
            self.run_events(name, None, &def.handlers.esc);
        }
    }

    pub(crate) fn set_focus(&mut self, menu: &str, index: usize) {
        let catalog = self.catalog;
        let Some(def) = catalog.get(menu) else {
            return;
        };
        let Some(open) = self.menus.get_mut(menu) else {
            return;
        };
        let old = open.focus;
        if old == Some(index) {
            return;
        }
        open.focus = Some(index);
        if let Some(old) = old.and_then(|i| def.items.get(i).map(|item| (i, item))) {
            self.run_events(menu, Some(old.0), &old.1.handlers.leave_focus);
        }
        if let Some(item) = def.items.get(index) {
            if !item.focus_sound.is_empty() {
                self.menus.sounds.push(item.focus_sound.clone());
            }
            self.run_events(menu, Some(index), &item.handlers.focus);
        }
    }

    pub(crate) fn set_hover(&mut self, menu: &str, index: Option<usize>) {
        let catalog = self.catalog;
        let Some(def) = catalog.get(menu) else {
            return;
        };
        let Some(open) = self.menus.get_mut(menu) else {
            return;
        };
        let old = open.hover;
        if old == index {
            if let Some(index) = index {
                self.set_focus(menu, index);
            }
            return;
        }
        open.hover = index;
        if let Some(old) = old.and_then(|i| def.items.get(i).map(|item| (i, item))) {
            self.run_events(menu, Some(old.0), &old.1.handlers.mouse_exit);
        }
        if let Some(index) = index {
            if let Some(item) = def.items.get(index) {
                self.run_events(menu, Some(index), &item.handlers.mouse_enter);
            }
            self.set_focus(menu, index);
        }
    }

    pub(crate) fn focus_step(&mut self, menu: &str, step: i32, first: bool) {
        let catalog = self.catalog;
        let Some(def) = catalog.get(menu) else {
            return;
        };
        let visible = self.visible_items(menu);
        let candidates: Vec<usize> = (0..def.items.len())
            .filter(|&i| is_focusable(&def.items[i]) && visible.contains(&i))
            .collect();
        if candidates.is_empty() {
            return;
        }
        let current = self.menus.get_mut(menu).and_then(|m| m.focus);
        let next = match (
            first,
            current.and_then(|c| candidates.iter().position(|&i| i == c)),
        ) {
            (false, Some(at)) => {
                let n = candidates.len() as i32;
                candidates[((at as i32 + step).rem_euclid(n)) as usize]
            }
            _ => {
                if step < 0 {
                    candidates[candidates.len() - 1]
                } else {
                    candidates[0]
                }
            }
        };
        self.set_focus(menu, next);
    }

    pub(crate) fn focus_nav(&mut self, menu: &str, dx: i32, dy: i32) -> bool {
        let catalog = self.catalog;
        let Some(def) = catalog.get(menu) else {
            return false;
        };
        let current = self.menus.get_mut(menu).and_then(|m| m.focus);
        let visible = self.visible_items(menu);
        let candidates: Vec<(usize, [f32; 4])> = (0..def.items.len())
            .filter(|&i| is_focusable(&def.items[i]) && visible.contains(&i))
            .map(|i| (i, nav_rect(&def.items[i])))
            .collect();
        let Some(&(at, here)) = current.and_then(|c| candidates.iter().find(|(i, _)| *i == c))
        else {
            if dx != 0 {
                return false;
            }
            self.focus_step(menu, dy, true);
            return true;
        };
        let centre = |r: [f32; 4]| [(r[0] + r[2]) * 0.5, (r[1] + r[3]) * 0.5];
        let c = centre(here);
        let same_column = |r: [f32; 4]| {
            let o = centre(r);
            (r[0] <= c[0] && c[0] <= r[2]) || (here[0] <= o[0] && o[0] <= here[2])
        };
        let others = candidates.iter().filter(|(i, _)| *i != at);
        let next = if dy != 0 {
            let dir = dy.signum() as f32;
            let column: Vec<_> = others.filter(|(_, r)| same_column(*r)).collect();
            let ahead = column
                .iter()
                .filter(|(_, r)| (centre(*r)[1] - c[1]) * dir > 0.5)
                .min_by(|a, b| {
                    let da = (centre(a.1)[1] - c[1]).abs();
                    let db = (centre(b.1)[1] - c[1]).abs();
                    da.total_cmp(&db)
                });
            let wrapped = || {
                column
                    .iter()
                    .min_by(|a, b| (centre(a.1)[1] * dir).total_cmp(&(centre(b.1)[1] * dir)))
            };
            match ahead.or_else(wrapped) {
                Some((index, _)) => *index,
                None => {
                    self.focus_step(menu, dy, false);
                    return true;
                }
            }
        } else {
            let dir = dx.signum() as f32;
            let Some((index, _)) = others
                .filter(|(_, r)| !same_column(*r) && (centre(*r)[0] - c[0]) * dir > 0.0)
                .min_by(|a, b| {
                    let score = |r: [f32; 4]| {
                        let o = centre(r);
                        (o[1] - c[1]).abs() * 4.0 + (o[0] - c[0]).abs()
                    };
                    score(a.1).total_cmp(&score(b.1))
                })
            else {
                return false;
            };
            *index
        };
        self.set_focus(menu, next);
        true
    }

    pub(crate) fn visible_items(&mut self, menu: &str) -> Vec<usize> {
        let Some(def) = self.catalog.get(menu) else {
            return Vec::new();
        };
        let hidden: Vec<bool> = self
            .menus
            .stack
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(menu))
            .map(|m| m.items.iter().map(|s| s.hidden).collect())
            .unwrap_or_default();
        let (def, _) = crate::killcam_skip::inherit_shared_vis(def);
        let open = self.menus.open_names();
        let host = MenuHost {
            world: self.world,
            dvars: self.dvars,
            menu: &def,
            locals: self.locals,
            open: &open,
            focus_rect: None,
        };
        let mut out = Vec::new();
        for (index, item) in def.items.iter().enumerate() {
            if hidden.get(index).copied().unwrap_or(false) {
                continue;
            }
            let enabled = item.disabled_exp.is_empty()
                || matches!(self.exprs.is_true(&item.disabled_exp, &host), Ok(false));
            if enabled && matches!(self.exprs.is_true(&item.vis_exp, &host), Ok(true)) {
                out.push(index);
            }
        }
        out
    }

    pub(crate) fn adjust(&mut self, menu: &str, index: usize, step: i32) -> bool {
        if !self.visible_items(menu).contains(&index) {
            return false;
        }
        let Some(item) = self.catalog.get(menu).and_then(|d| d.items.get(index)) else {
            return false;
        };
        if item.dvar.is_empty() {
            return false;
        }
        let current = self.dvar(&item.dvar).unwrap_or_default();
        let value = if let Some(slider) = &item.slider {
            let current = current.parse::<f32>().unwrap_or(slider.min);
            (current + step as f32 * slider.step)
                .clamp(slider.min, slider.max)
                .to_string()
        } else if !item.choices.is_empty() {
            let at = choice_index(&item.choices, current).unwrap_or(0);
            let next = (at as i32 + step).rem_euclid(item.choices.len() as i32) as usize;
            item.choices[next].1.clone()
        } else {
            return false;
        };
        self.dvars.set(&item.dvar, &value);
        self.menus
            .exec
            .push(format!("set {} \"{}\"", item.dvar, value));
        self.run_events(menu, Some(index), &item.handlers.action);
        true
    }

    pub(crate) fn slide(&mut self, menu: &str, index: usize, fraction: f32) -> bool {
        if !self.visible_items(menu).contains(&index) {
            return false;
        }
        let Some(item) = self.catalog.get(menu).and_then(|d| d.items.get(index)) else {
            return false;
        };
        let Some(slider) = &item.slider else {
            return false;
        };
        let value = slider.min + fraction.clamp(0.0, 1.0) * (slider.max - slider.min);
        let value = (slider.min + ((value - slider.min) / slider.step).round() * slider.step)
            .clamp(slider.min, slider.max)
            .to_string();
        self.dvars.set(&item.dvar, &value);
        self.menus
            .exec
            .push(format!("set {} \"{}\"", item.dvar, value));
        true
    }

    pub(crate) fn activate(&mut self, menu: &str, index: usize, accept: bool) {
        if self.adjust(menu, index, 1) {
            return;
        }
        if !self.visible_items(menu).contains(&index) {
            return;
        }
        let catalog = self.catalog;
        let Some(item) = catalog.get(menu).and_then(|d| d.items.get(index)) else {
            return;
        };
        if item.item_type == 14 && !item.dvar.is_empty() {
            self.binding.command = Some(item.dvar.clone());
            self.menus.binding_menu = Some(menu.to_owned());
            self.menus.bind_requests.push(item.dvar.clone());
            self.run_events(menu, Some(index), &item.handlers.action);
            return;
        }
        if item.item_type == 4 && !item.dvar.is_empty() {
            let max_chars = item
                .edit_field
                .as_ref()
                .map_or(32, |field| field.max_chars)
                .min(4096);
            let buffer: Vec<char> = self
                .dvars
                .get(&item.dvar)
                .unwrap_or_default()
                .chars()
                .take(max_chars)
                .collect();
            self.menus.editing = Some(EditState {
                menu: menu.to_owned(),
                item: index,
                cursor: buffer.len(),
                buffer,
                max_chars,
            });
            return;
        }
        let events = if accept && !item.handlers.accept.is_empty() {
            &item.handlers.accept
        } else {
            &item.handlers.action
        };
        self.run_events(menu, Some(index), events);
    }
}

fn nav_rect(item: &asset_game::MenuItem) -> [f32; 4] {
    let r = &item.rect;
    let ox = match r.horz_align {
        2 => 320.0,
        3 => 640.0,
        _ => 0.0,
    };
    let oy = match r.vert_align {
        2 => 240.0,
        3 => 480.0,
        _ => 0.0,
    };
    let (x0, x1) = (r.x + ox, r.x + ox + r.w);
    let (y0, y1) = (r.y + oy, r.y + oy + r.h);
    [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)]
}
