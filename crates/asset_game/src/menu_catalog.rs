use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;

use bevy::prelude::Resource;
use fastfile_iw4::{
    AssetLinkSink, AssetSink, AssetType, FontCapture, GlyphCapture, MenuDefCapture, MenuItemLayout,
    MenuRectCapture, MenuScriptKind, Ptr, ScriptStrings, ZonePtr, ZoneStream, block_is_aliasable,
    load_asset_at_observed, load_zone,
};

use crate::localize::decode_localized_text;
use asset_core::AssetRef;
use asset_iw4::size as sz;
use asset_material::{TS_2D, TS_COLOR_MAP, decode_zone_image_rgba};
use asset_transport::{GamesRoot, ZoneMemory, find_zone_file, game_root_for_zone, open_zone};

pub const ITEM_TYPE_BUTTON: i32 = 1;

pub const ITEM_TYPE_TEXT: i32 = 0;

pub const ITEM_TYPE_SLIDER: i32 = 10;

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MenuRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub horz_align: u8,
    pub vert_align: u8,
}

impl From<MenuRectCapture> for MenuRect {
    fn from(r: MenuRectCapture) -> Self {
        Self {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
            horz_align: r.horz_align,
            vert_align: r.vert_align,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum MenuEvent {
    Script(String),
    If {
        condition: String,
        then: Vec<MenuEvent>,
    },
    Else(Vec<MenuEvent>),
    SetLocalVar {
        kind: i32,
        name: String,
        expr: String,
    },
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MenuHandlers {
    pub open: Vec<MenuEvent>,
    pub close: Vec<MenuEvent>,
    pub close_request: Vec<MenuEvent>,
    pub esc: Vec<MenuEvent>,
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ItemHandlers {
    pub action: Vec<MenuEvent>,
    pub accept: Vec<MenuEvent>,
    pub mouse_enter: Vec<MenuEvent>,
    pub mouse_exit: Vec<MenuEvent>,
    pub focus: Vec<MenuEvent>,
    pub leave_focus: Vec<MenuEvent>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MenuEditField {
    pub max_chars: usize,
    pub masked: bool,
}

impl Default for MenuEditField {
    fn default() -> Self {
        Self {
            max_chars: 32,
            masked: false,
        }
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct MenuSlider {
    pub min: f32,
    pub max: f32,
    pub step: f32,
    #[serde(default)]
    pub display_range: Option<[f32; 2]>,
    #[serde(default)]
    pub decimals: u8,
    #[serde(default)]
    pub suffix: String,
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MenuItem {
    pub name: String,
    pub handlers: ItemHandlers,

    pub text_key: String,
    pub text_literal: bool,
    pub item_type: i32,

    pub style: i32,
    pub owner_draw: i32,

    pub rect: MenuRect,
    pub fore_color: [f32; 4],
    pub back_color: [f32; 4],
    pub glow_color: [f32; 4],
    pub text_scale: f32,

    pub font_enum: i32,

    pub text_align_mode: i32,
    pub text_align_x: f32,
    pub text_align_y: f32,
    pub text_style: i32,
    pub game_msg_window_index: i32,
    pub background: String,
    pub focus_sound: String,
    pub dvar: String,
    pub dvar_test: String,
    pub enable_dvar: String,
    pub local_var: String,
    pub edit_field: Option<MenuEditField>,
    pub choices: Vec<(String, String)>,
    pub slider: Option<MenuSlider>,
    pub bg_ptr: u8,
    pub vis_ptr: u8,
    pub mat_ptr: u8,
    pub vis_exp: String,
    pub text_exp: String,
    pub material_exp: String,
    pub disabled_exp: String,

    pub float_exp: Vec<(u32, String)>,
    pub sound_ptr: u8,
    pub mouse_enter_ptr: u8,
    pub on_focus_ptr: u8,
    pub static_flags: i32,
    pub dvar_flags: i32,
    pub action: Vec<String>,
    pub mouse_enter: Vec<String>,
    pub on_focus: Vec<String>,
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MenuDef {
    pub name: String,
    pub sound_name: String,
    pub window_background: String,
    pub expr_dvars: String,
    pub fullscreen: i32,
    pub static_flags: i32,
    pub focus_color: Option<[f32; 4]>,

    pub rect: MenuRect,
    pub items: Vec<MenuItem>,
    pub on_open: Vec<String>,

    pub on_close: Vec<String>,

    pub on_close_request: Vec<String>,
    pub on_esc: Vec<String>,
    pub handlers: MenuHandlers,

    pub vis_exp: String,

    pub float_exp: Vec<(u32, String)>,

    pub on_open_local_vars: Vec<MenuSetLocalVar>,

    #[serde(skip)]
    expr_dvar_rows: std::sync::OnceLock<Vec<(i32, std::ops::Range<usize>)>>,
}

impl MenuDef {
    pub fn static_dvar_name(&self, index: i32) -> Option<&str> {
        self.expr_dvar_rows
            .get_or_init(|| expr_dvar_rows(&self.expr_dvars))
            .iter()
            .find(|(row, _)| *row == index)
            .map(|(_, name)| &self.expr_dvars[name.clone()])
    }

    pub fn set_expr_dvars(&mut self, expr_dvars: String) {
        self.expr_dvars = expr_dvars;
        self.expr_dvar_rows = std::sync::OnceLock::new();
    }
}

fn expr_dvar_rows(list: &str) -> Vec<(i32, std::ops::Range<usize>)> {
    let base = list.as_ptr() as usize;
    let mut tokens = list.split_whitespace();
    let mut rows = Vec::new();
    while let (Some("d"), Some(row), Some(name)) = (tokens.next(), tokens.next(), tokens.next()) {
        let start = name.as_ptr() as usize - base;
        if let Ok(row) = row.parse::<i32>() {
            rows.push((row, start..start + name.len()));
        }
    }
    rows
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MenuSetLocalVar {
    pub kind: i32,
    pub name: String,

    pub expr: String,
}

#[derive(Clone, Debug, Default)]
pub struct FontDef {
    pub name: String,
    pub pixel_height: i32,
    pub material: String,
    pub glow_material: String,
    pub glyphs: Vec<GlyphCapture>,
}

impl FontDef {
    pub fn glyph(&self, letter: u32) -> Option<&GlyphCapture> {
        const ASCII_BASE: u32 = 0x20;
        const ASCII_COUNT: u32 = 0x60;
        const MISS_INDEX: usize = 0x150 / 0x18;
        if letter.wrapping_sub(ASCII_BASE) < ASCII_COUNT {
            return self.glyphs.get((letter - ASCII_BASE) as usize);
        }
        let last = self.glyphs.len().saturating_sub(1);
        if last < ASCII_COUNT as usize {
            return self.glyphs.get(MISS_INDEX);
        }
        let mut lo = ASCII_COUNT as usize;
        let mut hi = last;
        while lo <= hi {
            let mid = lo.saturating_add(hi) / 2;
            let found = self.glyphs.get(mid)?.letter as u32;
            if found == letter {
                return self.glyphs.get(mid);
            }
            if found < letter {
                lo = mid.saturating_add(1);
            } else if mid == 0 {
                break;
            } else {
                hi = mid - 1;
            }
        }
        self.glyphs.get(MISS_INDEX)
    }
}

#[derive(Clone, Debug)]
pub struct ZoneUiImage {
    pub material: String,
    pub image: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,

    pub source: &'static str,
}

#[derive(Clone, Debug, Default)]
pub struct CapturedStringTable {
    pub name: String,
    pub columns: usize,
    pub rows: usize,
    pub cells: Vec<String>,
}

impl CapturedStringTable {
    #[must_use]
    pub fn cell(&self, row: i32, col: i32) -> &str {
        if row < 0 || col < 0 {
            return "";
        }
        let row = row as usize;
        let col = col as usize;
        if col >= self.columns || row >= self.rows {
            return "";
        }
        self.cells
            .get(row.saturating_mul(self.columns).saturating_add(col))
            .map(String::as_str)
            .unwrap_or("")
    }

    #[must_use]
    pub fn lookup_col(&self, key: &str, col: i32) -> &str {
        match self.lookup_row(key) {
            Some(row) => self.cell(row, col),
            None => "",
        }
    }

    #[must_use]
    pub fn lookup_row(&self, key: &str) -> Option<i32> {
        self.lookup_row_in_col(0, key)
    }

    #[must_use]
    pub fn lookup_row_in_col(&self, col: i32, key: &str) -> Option<i32> {
        for row in 0..self.rows {
            if self.cell(row as i32, col).eq_ignore_ascii_case(key) {
                return Some(row as i32);
            }
        }
        None
    }
}

#[derive(Clone, Debug, Default, Resource)]
pub struct MenuCatalog {
    pub menus: BTreeMap<String, MenuDef>,
    pub fonts: BTreeMap<String, FontDef>,

    pub string_tables: BTreeMap<String, CapturedStringTable>,

    pub rawfiles: BTreeMap<String, String>,

    pub zone_images: BTreeMap<String, ZoneUiImage>,

    pub material_state_bits: BTreeMap<String, asset_iw4::ColorPassAgreement<[u32; 2]>>,

    pub material_2d_plans: BTreeMap<String, HudMaterialPlan>,

    pub material_images: BTreeMap<String, String>,
    pub material_srgb_reads: BTreeMap<String, bool>,
    pub lists: Vec<(String, i32)>,
    pub walked: usize,

    pub font_headers: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HudMaterialPlan {
    pub texture_atlas: [u8; 2],
    pub technique_slots: Option<u64>,
    pub state_bits_entry: Option<[u8; asset_iw4::size::TECHNIQUE_SLOT_COUNT]>,
    pub state_rows: Vec<[u32; 2]>,
    pub unlit_pass_states: Vec<[u32; 2]>,
    pub unlit_pass_count: Option<u8>,
    pub textures: Vec<HudMaterialTextureBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HudMaterialTextureBinding {
    pub name_hash: u32,
    pub name_start: u8,
    pub name_end: u8,
    pub sampler_state: u8,
    pub semantic: u8,
    pub image: Option<String>,
}

impl MenuCatalog {
    pub fn load_definitions(&mut self, source: &str) -> Result<(), String> {
        let definitions = crate::menu_source::load(source, self)?;
        self.menus
            .extend(definitions.into_iter().map(|def| (def.name.clone(), def)));
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&MenuDef> {
        self.menus.get(name).or_else(|| {
            self.menus
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, def)| def)
        })
    }

    pub fn played_sound_aliases(&self) -> std::collections::BTreeSet<&str> {
        fn scan<'a>(script: &'a str, out: &mut std::collections::BTreeSet<&'a str>) {
            for command in script.split(';') {
                let mut words = command
                    .split_whitespace()
                    .map(|word| word.trim_matches('"'));
                if words
                    .next()
                    .is_some_and(|verb| verb.eq_ignore_ascii_case("play"))
                    && let Some(alias) = words.next().filter(|alias| !alias.is_empty())
                {
                    out.insert(alias);
                }
            }
        }
        let mut out = std::collections::BTreeSet::new();
        for menu in self.menus.values() {
            if !menu.sound_name.is_empty() {
                out.insert(menu.sound_name.as_str());
            }
            for script in menu
                .on_open
                .iter()
                .chain(&menu.on_close)
                .chain(&menu.on_close_request)
                .chain(&menu.on_esc)
            {
                scan(script, &mut out);
            }
            for item in &menu.items {
                if !item.focus_sound.is_empty() {
                    out.insert(item.focus_sound.as_str());
                }
                for script in item
                    .action
                    .iter()
                    .chain(&item.mouse_enter)
                    .chain(&item.on_focus)
                {
                    scan(script, &mut out);
                }
            }
        }
        out
    }

    pub fn font(&self, name: &str) -> Option<&FontDef> {
        self.fonts.get(&name.to_ascii_lowercase())
    }

    pub fn material_image(&self, material: &str) -> Option<&str> {
        self.material_images
            .get(&AssetRef::bare_name(material).to_ascii_lowercase())
            .map(String::as_str)
    }

    pub fn zone_image(&self, name: &str) -> Option<&ZoneUiImage> {
        self.zone_images
            .get(&AssetRef::bare_name(name).to_ascii_lowercase())
    }

    pub fn zone_image_n(&self) -> usize {
        let mut seen = BTreeMap::new();
        for atlas in self.zone_images.values() {
            seen.insert((atlas.material.as_str(), atlas.image.as_str()), ());
        }
        seen.len()
    }

    pub fn string_table(&self, name: &str) -> Option<&CapturedStringTable> {
        if let Some(table) = self.string_tables.get(name) {
            return Some(table);
        }
        self.string_tables
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, t)| t)
    }

    #[must_use]
    pub fn rawfile_text(&self, name: &str) -> Option<&str> {
        if let Some(text) = self.rawfiles.get(name) {
            return Some(text.as_str());
        }
        self.rawfiles
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, t)| t.as_str())
    }

    pub fn absorb(&mut self, other: MenuCatalog) {
        self.walked += other.walked;
        self.font_headers += other.font_headers;
        self.lists.extend(other.lists);
        for (name, def) in other.menus {
            self.menus.insert(name, def);
        }
        for (name, font) in other.fonts {
            self.fonts.insert(name, font);
        }
        for (name, table) in other.string_tables {
            self.string_tables.insert(name, table);
        }
        for (name, text) in other.rawfiles {
            self.rawfiles.insert(name, text);
        }
        self.material_state_bits.extend(other.material_state_bits);
        self.material_srgb_reads.extend(other.material_srgb_reads);
        self.material_2d_plans.extend(other.material_2d_plans);
        for (name, image) in other.material_images {
            self.material_images.insert(name, image);
        }
        for (name, image) in other.zone_images {
            self.zone_images.insert(name, image);
        }
    }
}

pub const UI_MENU_ZONES: &[&str] = &[
    "code_post_gfx_mp",
    "localized_code_post_gfx_mp",
    "localized_ui_mp",
    "common_mp",
    "patch_mp",
];

pub const HUD_CHROME_MENUS: &[&str] = &[
    "scorebar_hd",
    "scorebar_sd",
    "minimap_fullscreen",
    "weaponbar_hd",
    "weaponbar_sd",
    "hud_fullscreen",
    "dpad_hd",
    "javelin_overlay_hd",
    "missilecam_hud_hd",
    "remote_chopper_overlay_hd",
    "ac130_hud_hd",
    "dpad_sd",
    "splash",
    "challenge",
    "killstreak",
    "promotion",
    "scoreboard",
    "playercard_splash",
    "playercard_youkilled_hd",
    "playercard_killedby_hd",
    "perks_info_hd",
];

pub fn ui_games_root(games: &GamesRoot) -> Result<GamesRoot, String> {
    let anchor = find_zone_file(games, "iw4:common_mp")?;
    game_root_for_zone(&anchor.path).map(GamesRoot)
}

pub fn load_ui_menu_catalog(games: &GamesRoot) -> (MenuCatalog, Vec<String>) {
    let mut catalog = MenuCatalog::default();
    let mut report = Vec::new();
    let games = match ui_games_root(games) {
        Ok(root) => root,
        Err(error) => return (catalog, vec![format!("menu catalog unavailable: {error}")]),
    };
    report.push(format!("menu asset root (IW4): {}", games.0.display()));
    for zone in UI_MENU_ZONES {
        match find_zone_file(&games, &format!("iw4:{zone}")) {
            Ok(found) => match load_menu_catalog_with_iwd(&found.path, Some(&games.0)) {
                Ok(part) => {
                    report.push(format!(
                        "menu catalog: {zone} {} menus ({} lists, {} assets, {} font headers, {} fonts; main={}, main_text={}) from {}",
                        part.menus.len(),
                        part.lists.len(),
                        part.walked,
                        part.font_headers,
                        part.fonts.len(),
                        part.get("main").is_some(),
                        part.get("main_text").is_some(),
                        found.path.display()
                    ));
                    catalog.absorb(part);
                }
                Err(error) => report.push(format!(
                    "menu catalog gap: {zone} at {}: {error}",
                    found.path.display()
                )),
            },
            Err(error) => report.push(format!("menu catalog gap: {zone}: {error}")),
        }
    }
    report.push(format!(
        "menu catalog: {} menus after merge, {} zone font atlases",
        catalog.menus.len(),
        catalog.zone_image_n()
    ));
    (catalog, report)
}

pub fn load_menu_catalog(path: &Path) -> Result<MenuCatalog, String> {
    load_menu_catalog_with_iwd(path, None)
}

fn load_menu_catalog_with_iwd(path: &Path, games: Option<&Path>) -> Result<MenuCatalog, String> {
    let image = open_zone(path).map_err(|e| e.to_string())?;
    let header = image.header().map_err(|e| e.to_string())?;
    let mut memory = ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
    let mut sink = MenuSink {
        catalog: MenuCatalog::default(),
        russian_fonts: path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name.eq_ignore_ascii_case("russian")),
        names: HashMap::new(),
        script_sets: HashMap::new(),
        script_set_stack: Vec::new(),
        branches: Vec::new(),
        image_links: HashMap::new(),
        images: Vec::new(),
        technique_links: HashMap::new(),
        material_ts2d: HashMap::new(),
        loading_asset: None,
    };
    load_zone(&mut stream, &mut sink).map_err(|error| match sink.loading_asset {
        Some((index, ty, slot)) => format!("asset #{index} {} at {slot:?}: {error:?}", ty.name()),
        None => format!("asset table: {error:?}"),
    })?;
    Ok(sink.finish_and_take(games))
}

#[derive(Clone, Copy)]
enum ImageLink {
    Direct(usize),
    Alias(Ptr),
}

#[derive(Clone, Copy)]
enum TechniqueLink {
    Direct(fastfile_iw4::TechniqueSetGeometry),
    Alias(Ptr),
}

struct CapturedZoneImage {
    name: String,
    use_srgb_reads: bool,
    width: u16,
    height: u16,
    format: u32,
    payload: Vec<u8>,
}

#[derive(Clone)]
enum RecordedEvent {
    Script(String),
    Begin(Option<String>),
    End,
    SetLocalVar(i32, String, String),
}

struct MenuSink {
    russian_fonts: bool,
    catalog: MenuCatalog,
    names: HashMap<(u8, u32), String>,
    script_sets: HashMap<(u8, u32), Vec<RecordedEvent>>,
    script_set_stack: Vec<Ptr>,
    branches: Vec<(Option<String>, Vec<MenuEvent>)>,
    image_links: HashMap<Ptr, ImageLink>,
    images: Vec<CapturedZoneImage>,

    technique_links: HashMap<Ptr, TechniqueLink>,

    material_ts2d: HashMap<String, usize>,

    loading_asset: Option<(usize, AssetType, Ptr)>,
}

fn ptr_key(p: Ptr) -> (u8, u32) {
    (p.block, p.offset)
}

fn hud_pass_state_bits(
    state_bits_entry: Option<&[u8; asset_iw4::size::TECHNIQUE_SLOT_COUNT]>,
    table: &[[u32; 2]],
    technique_slots: Option<u64>,
) -> asset_iw4::ColorPassAgreement<[u32; 2]> {
    let technique_slots = technique_slots.filter(|&slots| slots != 0).or_else(|| {
        state_bits_entry.map(|entries| {
            entries
                .iter()
                .enumerate()
                .filter(|&(_, &entry)| entry != u8::MAX)
                .fold(0u64, |slots, (i, _)| slots | 1 << i)
        })
    });
    if let Some(slots) = technique_slots
        && let Some(row) = asset_iw4::color_pass_row_for_tech_type_pass(
            state_bits_entry,
            table,
            slots,
            asset_iw4::TECHNIQUE_UNLIT,
            0,
        )
    {
        return asset_iw4::ColorPassAgreement::Agreed(row);
    }
    asset_iw4::color_pass_agreement(state_bits_entry, table, technique_slots, |bits| bits)
}

fn hud_unlit_pass_states(
    state_bits_entry: Option<&[u8; asset_iw4::size::TECHNIQUE_SLOT_COUNT]>,
    table: &[[u32; 2]],
    technique_set: Option<fastfile_iw4::TechniqueSetGeometry>,
) -> (Option<u8>, Vec<[u32; 2]>) {
    let count = technique_set
        .filter(|set| set.technique_slots_scanned & (1 << asset_iw4::TECHNIQUE_UNLIT) != 0)
        .map(|set| set.pass_count_by_slot[asset_iw4::TECHNIQUE_UNLIT]);
    let states = (0..usize::from(count.unwrap_or(0)))
        .map(|pass| {
            asset_iw4::color_pass_row_for_tech_type_pass(
                state_bits_entry,
                table,
                technique_set.map_or(0, |set| set.technique_slots),
                asset_iw4::TECHNIQUE_UNLIT,
                pass,
            )
        })
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default();
    (count, states)
}

impl MenuSink {
    fn bind_name(&mut self, slot: Ptr, name: &str) {
        if name.is_empty() {
            return;
        }
        self.names.insert(ptr_key(slot), name.to_owned());
    }

    fn record_script_set(&mut self, event: RecordedEvent) {
        for body in &self.script_set_stack {
            self.script_sets
                .entry(ptr_key(*body))
                .or_default()
                .push(event.clone());
        }
    }

    fn push_event(&mut self, menu: &str, kind: MenuScriptKind, event: MenuEvent) {
        if let Some((_, body)) = self.branches.last_mut() {
            body.push(event);
            return;
        }
        let Some(def) = self.catalog.menus.get_mut(menu) else {
            return;
        };
        let list = match kind {
            MenuScriptKind::OnOpen => &mut def.handlers.open,
            MenuScriptKind::OnClose => &mut def.handlers.close,
            MenuScriptKind::OnCloseRequest => &mut def.handlers.close_request,
            MenuScriptKind::OnEsc => &mut def.handlers.esc,
            MenuScriptKind::ExecKey => return,
            item_kind => {
                let Some(row) = def.items.last_mut() else {
                    return;
                };
                match item_kind {
                    MenuScriptKind::Action => &mut row.handlers.action,
                    MenuScriptKind::Accept => &mut row.handlers.accept,
                    MenuScriptKind::MouseEnter | MenuScriptKind::MouseEnterText => {
                        &mut row.handlers.mouse_enter
                    }
                    MenuScriptKind::MouseExit | MenuScriptKind::MouseExitText => {
                        &mut row.handlers.mouse_exit
                    }
                    MenuScriptKind::OnFocus => &mut row.handlers.focus,
                    _ => &mut row.handlers.leave_focus,
                }
            }
        };
        list.push(event);
    }

    fn bind_image(&mut self, slot: Ptr, link: ImageLink) {
        if block_is_aliasable(slot.block) {
            self.image_links.insert(slot, link);
        }
    }

    fn resolve_image(&self, mut slot: Ptr) -> Option<usize> {
        for _ in 0..32 {
            match self.image_links.get(&slot).copied()? {
                ImageLink::Direct(index) => return Some(index),
                ImageLink::Alias(target) => slot = target,
            }
        }
        None
    }

    fn bind_technique(&mut self, slot: Ptr, link: TechniqueLink) {
        if block_is_aliasable(slot.block) {
            self.technique_links.insert(slot, link);
        }
    }

    fn material_technique_set(
        &self,
        s: &ZoneStream<'_>,
        field: Ptr,
    ) -> Option<fastfile_iw4::TechniqueSetGeometry> {
        match s.ptr_at(field, 0).ok()? {
            ZonePtr::Null => None,
            ZonePtr::Offset(target) => self.resolve_technique_set(target),
            ZonePtr::Following | ZonePtr::Insert => s.latest_technique_set(),
        }
    }

    fn resolve_technique_set(&self, mut slot: Ptr) -> Option<fastfile_iw4::TechniqueSetGeometry> {
        for _ in 0..32 {
            match self.technique_links.get(&slot).copied()? {
                TechniqueLink::Direct(set) => return Some(set),
                TechniqueLink::Alias(target) => slot = target,
            }
        }
        None
    }

    fn capture_zone_image(&mut self, s: &ZoneStream<'_>) -> Option<usize> {
        let geometry = s.latest_image()?;
        let name = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)?;
        let payload = if geometry.source_len == 0 {
            Vec::new()
        } else {
            s.source_slice(geometry.source_offset, geometry.source_len)
                .ok()?
                .to_vec()
        };
        let index = self.images.len();
        self.images.push(CapturedZoneImage {
            name: name.as_str().to_owned(),
            use_srgb_reads: geometry.use_srgb_reads,
            width: geometry.width,
            height: geometry.height,
            format: geometry.format,
            payload,
        });
        Some(index)
    }

    fn capture_material_ts2d(&mut self, s: &ZoneStream<'_>) {
        let Some(geometry) = s.latest_material() else {
            return;
        };
        let Some(name) = geometry
            .name
            .and_then(|name| s.cstr(name).ok())
            .map(AssetRef::decode)
        else {
            return;
        };
        let key = name.as_str().to_ascii_lowercase();
        if key.is_empty() {
            return;
        }

        let bits: Vec<_> = (0..geometry.state_bits_count)
            .filter_map(|i| {
                let p = geometry.state_bits?.at(i * sz::GFX_STATE_BITS);
                Some([s.u32_at(p, 0).ok()?, s.u32_at(p, 4).ok()?])
            })
            .collect();
        let technique_set = geometry
            .technique_set
            .and_then(|field| self.material_technique_set(s, field));
        let technique_slots = technique_set.map(|set| set.technique_slots);
        if geometry.state_bits_count > 0 {
            self.catalog.material_state_bits.insert(
                key.clone(),
                hud_pass_state_bits(geometry.state_bits_entry.as_ref(), &bits, technique_slots),
            );
        }
        let (unlit_pass_count, unlit_pass_states) =
            hud_unlit_pass_states(geometry.state_bits_entry.as_ref(), &bits, technique_set);
        let mut ts2d = None;
        let mut color = None;
        let mut bindings = Vec::with_capacity(geometry.texture_count);
        if let Some(table) = geometry.textures {
            for i in 0..geometry.texture_count {
                let texture = table.at(i * geometry.texture_stride);
                let Ok(semantic) = s.u8_at(texture, 7) else {
                    continue;
                };
                let image = (semantic != 11)
                    .then(|| self.resolve_image(texture.at(8)))
                    .flatten();
                bindings.push(HudMaterialTextureBinding {
                    name_hash: s.u32_at(texture, 0).unwrap_or(0),
                    name_start: s.u8_at(texture, 4).unwrap_or(0),
                    name_end: s.u8_at(texture, 5).unwrap_or(0),
                    sampler_state: s.u8_at(texture, 6).unwrap_or(0),
                    semantic,
                    image: image.and_then(|index| self.images.get(index).map(|i| i.name.clone())),
                });
                if let Some(image) = image {
                    if semantic == TS_2D {
                        ts2d = Some(image);
                    } else if semantic == TS_COLOR_MAP {
                        color = Some(image);
                    }
                }
            }
        }
        self.catalog.material_2d_plans.insert(
            key.clone(),
            HudMaterialPlan {
                texture_atlas: geometry.texture_atlas,
                technique_slots,
                state_bits_entry: geometry.state_bits_entry,
                state_rows: bits,
                unlit_pass_states,
                unlit_pass_count,
                textures: bindings,
            },
        );
        if let Some(image) = ts2d.or(color) {
            if let Some(captured) = self.images.get(image) {
                self.catalog
                    .material_images
                    .insert(key.clone(), captured.name.clone());
                self.catalog
                    .material_srgb_reads
                    .insert(key.clone(), captured.use_srgb_reads);
            }
            self.material_ts2d.insert(key, image);
        }
    }

    fn finish_ui_images(&mut self, games: Option<&Path>) {
        let mut materials = Vec::new();
        for font in self.catalog.fonts.values() {
            if !font.material.is_empty() {
                materials.push(font.material.clone());
            }
            if !font.glow_material.is_empty() {
                materials.push(font.glow_material.clone());
            }
        }
        for name in ["main", "main_text", "menu_xboxlive_lobby"]
            .into_iter()
            .chain(HUD_CHROME_MENUS.iter().copied())
        {
            if let Some(menu) = self.catalog.get(name) {
                if !menu.window_background.is_empty() {
                    materials.push(menu.window_background.clone());
                }
                materials.extend(
                    menu.items
                        .iter()
                        .map(|item| item.background.clone())
                        .filter(|material| !material.is_empty()),
                );
            }
        }

        materials.extend(
            [
                "progress_bar_bg",
                "progress_bar_fill",
                "hud_suitcase_bomb",
                "compassping_friendly_mp",
                "compassping_enemyfiring",
                "hud_fofbox_self",
                "hud_fofbox_hostile",
            ]
            .map(str::to_owned),
        );
        materials.extend(
            self.material_ts2d
                .keys()
                .filter(|name| {
                    name.starts_with("rank_")
                        || name.starts_with("killicon")
                        || name.starts_with("hud_minimap_")
                })
                .cloned(),
        );
        if let Some(table) = self.catalog.string_table("mp/rankIconTable.csv") {
            for row in 0..table.rows {
                // Column zero identifies the rank; the rest are prestige variants.
                for col in 1..table.columns {
                    let material = table.cell(row as i32, col as i32);
                    if !material.is_empty() {
                        materials.push(material.to_owned());
                    }
                }
            }
        }
        // These icons are selected through expressions, so static backgrounds do
        // not enumerate them. Some (including ACOG) only live in the fastfile.
        for (name, column) in [
            ("mp/statsTable.csv", 6),
            ("mp/attachmentTable.csv", 6),
            ("mp/perkTable.csv", 3),
        ] {
            if let Some(table) = self.catalog.string_table(name) {
                for row in 0..table.rows {
                    let material = table.cell(row as i32, column);
                    if !material.is_empty() {
                        materials.push(material.to_owned());
                    }
                }
            }
        }
        materials.sort_unstable();
        materials.dedup();
        for material in materials {
            self.install_ui_image(&material, games);
        }
    }

    fn finish_and_take(mut self, games: Option<&Path>) -> MenuCatalog {
        self.finish_ui_images(games);
        self.catalog
    }

    fn install_ui_image(&mut self, material: &str, games: Option<&Path>) {
        let bare = AssetRef::bare_name(material);
        if bare.is_empty() {
            return;
        }
        let key = bare.to_ascii_lowercase();
        if self.catalog.zone_images.contains_key(&key) {
            return;
        }
        let Some(&index) = self.material_ts2d.get(&key) else {
            diag::info!(
                Zone,
                "menu catalog: material `{bare}` has no TS_2D / colorMap image"
            );
            return;
        };
        let Some(image) = self.images.get(index) else {
            return;
        };
        let decoded = if image.payload.is_empty() {
            let Some(root) = games else {
                diag::info!(
                    Zone,
                    "menu catalog: material `{bare}` image `{}` is an IWD stub; no games root",
                    image.name
                );
                return;
            };
            match crate::decode_ui_image(root, &image.name) {
                Ok(Some(rgba)) => Some((rgba, "iwd")),
                Ok(None) => {
                    diag::warn!(
                        Zone,
                        "menu catalog: material `{bare}` image `{}` is not in the IWD",
                        image.name
                    );
                    None
                }
                Err(error) => {
                    diag::warn!(
                        Zone,
                        "menu catalog: IWD decode `{bare}` / `{}`: {error}",
                        image.name
                    );
                    None
                }
            }
        } else {
            match decode_zone_image_rgba(
                u32::from(image.width),
                u32::from(image.height),
                image.format,
                &image.payload,
            ) {
                Ok(rgba) => Some((rgba, "zone")),
                Err(error) => {
                    diag::warn!(
                        Zone,
                        "menu catalog: decode font atlas `{bare}` / `{}`: {error}",
                        image.name
                    );
                    None
                }
            }
        };
        let Some(((width, height, rgba), source)) = decoded else {
            return;
        };
        let rec = ZoneUiImage {
            material: bare.to_owned(),
            image: image.name.clone(),
            width,
            height,
            rgba: Arc::new(rgba),
            source,
        };
        diag::info!(
            Zone,
            "menu catalog: UI image {bare} -> {} {width}x{height} source={source}",
            rec.image
        );
        let image_key = rec.image.to_ascii_lowercase();
        self.catalog.zone_images.insert(key, rec.clone());
        if image_key != rec.material.to_ascii_lowercase() {
            self.catalog.zone_images.insert(image_key, rec);
        }
    }
}

impl AssetSink for MenuSink {
    fn set_script_strings(&mut self, _strings: ScriptStrings) {}

    fn load_asset(
        &mut self,
        s: &mut ZoneStream<'_>,
        index: usize,
        ty: AssetType,
        slot: Ptr,
    ) -> fastfile_iw4::Result<()> {
        self.loading_asset = Some((index, ty, slot));
        self.catalog.walked += 1;
        if ty == AssetType::Font {
            self.catalog.font_headers += 1;
        }
        load_asset_at_observed(s, ty, slot, self)?;
        self.loading_asset = None;
        Ok(())
    }
}

impl AssetLinkSink for MenuSink {
    fn loaded(
        &mut self,
        s: &ZoneStream<'_>,
        ty: AssetType,
        slot: Ptr,
        insert_slot: Option<Ptr>,
    ) -> fastfile_iw4::Result<()> {
        let name = match ty {
            AssetType::Image => {
                if let Some(index) = self.capture_zone_image(s) {
                    self.bind_image(slot, ImageLink::Direct(index));
                    if let Some(ins) = insert_slot {
                        self.bind_image(ins, ImageLink::Direct(index));
                    }
                }
                return Ok(());
            }
            AssetType::TechniqueSet => {
                if let Some(set) = s.latest_technique_set() {
                    self.bind_technique(slot, TechniqueLink::Direct(set));
                    if let Some(ins) = insert_slot {
                        self.bind_technique(ins, TechniqueLink::Direct(set));
                    }
                }
                return Ok(());
            }
            AssetType::Material => s
                .latest_material()
                .and_then(|g| g.name)
                .and_then(|n| s.cstr(n).ok())
                .unwrap_or(""),
            AssetType::Sound => s
                .latest_sound_name()
                .and_then(|n| s.cstr(n).ok())
                .unwrap_or(""),
            _ => return Ok(()),
        };
        self.bind_name(slot, name);
        if let Some(ins) = insert_slot {
            self.bind_name(ins, name);
        }
        if ty == AssetType::Material {
            if let Some(header) = s.latest_material().and_then(|g| g.header) {
                self.bind_name(header, name);
            }
            self.capture_material_ts2d(s);
        }
        Ok(())
    }

    fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> fastfile_iw4::Result<()> {
        if ty == AssetType::Image {
            self.bind_image(slot, ImageLink::Alias(target));
        }
        if ty == AssetType::TechniqueSet {
            self.bind_technique(slot, TechniqueLink::Alias(target));
        }
        if let Some(name) = self.names.get(&ptr_key(target)).cloned() {
            self.bind_name(slot, &name);
        }
        Ok(())
    }

    fn linked_asset_name(&self, slot: Ptr) -> Option<&str> {
        self.names.get(&ptr_key(slot)).map(String::as_str)
    }

    fn capture_menu_list(&mut self, name: &str, count: i32) -> fastfile_iw4::Result<()> {
        self.catalog.lists.push((name.to_owned(), count));
        Ok(())
    }

    fn capture_menu(&mut self, name: &str) -> fastfile_iw4::Result<()> {
        self.catalog.menus.insert(
            name.to_owned(),
            MenuDef {
                name: name.to_owned(),
                ..MenuDef::default()
            },
        );
        Ok(())
    }

    fn capture_menu_def(&mut self, rec: &MenuDefCapture<'_>) -> fastfile_iw4::Result<()> {
        let def = self
            .catalog
            .menus
            .entry(rec.name.to_owned())
            .or_insert_with(|| MenuDef {
                name: rec.name.to_owned(),
                ..MenuDef::default()
            });
        def.sound_name = rec.sound_name.to_owned();
        def.window_background = rec.window_background.to_owned();
        def.set_expr_dvars(rec.expr_dvars.to_owned());
        def.fullscreen = rec.fullscreen;
        def.static_flags = rec.static_flags;
        def.focus_color = Some(rec.focus_color);
        def.rect = MenuRect::from(rec.rect);
        Ok(())
    }

    fn capture_menu_visible_exp(&mut self, menu: &str, dump: &str) -> fastfile_iw4::Result<()> {
        let Some(def) = self.catalog.menus.get_mut(menu) else {
            return Ok(());
        };
        def.vis_exp = dump.to_owned();
        Ok(())
    }

    fn capture_menu_float_exp(
        &mut self,
        menu: &str,
        key: u32,
        dump: &str,
    ) -> fastfile_iw4::Result<()> {
        let Some(def) = self.catalog.menus.get_mut(menu) else {
            return Ok(());
        };
        def.float_exp.push((key, dump.to_owned()));
        Ok(())
    }

    fn capture_menu_item(
        &mut self,
        menu: &str,
        item: &str,
        text: &[u8],
        owner_draw: i32,
        item_type: i32,
    ) -> fastfile_iw4::Result<()> {
        let def = self
            .catalog
            .menus
            .entry(menu.to_owned())
            .or_insert_with(|| MenuDef {
                name: menu.to_owned(),
                ..MenuDef::default()
            });
        def.items.push(MenuItem {
            name: item.to_owned(),
            text_key: decode_localized_text(text),
            item_type,
            owner_draw,
            ..MenuItem::default()
        });
        Ok(())
    }

    fn capture_menu_item_layout(&mut self, rec: &MenuItemLayout<'_>) -> fastfile_iw4::Result<()> {
        let Some(def) = self.catalog.menus.get_mut(rec.menu) else {
            return Ok(());
        };
        if let Some(item) = def.items.last_mut() {
            item.rect = MenuRect::from(rec.rect);
            item.style = rec.style;
            item.fore_color = rec.fore_color;
            item.back_color = rec.back_color;
            item.glow_color = rec.glow_color;
            item.text_scale = rec.text_scale;
            item.font_enum = rec.font_enum;
            item.text_align_mode = rec.text_align_mode;
            item.text_align_x = rec.text_align_x;
            item.text_align_y = rec.text_align_y;
            item.text_style = rec.text_style;
            item.game_msg_window_index = rec.game_msg_window_index;
            item.background = rec.background.to_owned();
            item.focus_sound = rec.focus_sound.to_owned();
            item.dvar = rec.dvar.to_owned();
            item.dvar_test = rec.dvar_test.to_owned();
            item.enable_dvar = rec.enable_dvar.to_owned();
            item.local_var = rec.local_var.to_owned();
            item.bg_ptr = rec.bg_ptr;
            item.vis_ptr = rec.vis_ptr;
            item.mat_ptr = rec.mat_ptr;
            item.sound_ptr = rec.sound_ptr;
            item.mouse_enter_ptr = rec.mouse_enter_ptr;
            item.on_focus_ptr = rec.on_focus_ptr;
            item.static_flags = rec.static_flags;
            item.dvar_flags = rec.dvar_flags;
        }
        Ok(())
    }

    fn capture_item_visible_exp(
        &mut self,
        menu: &str,
        _item: &str,
        dump: &str,
    ) -> fastfile_iw4::Result<()> {
        let Some(def) = self.catalog.menus.get_mut(menu) else {
            return Ok(());
        };
        if let Some(item) = def.items.last_mut() {
            item.vis_exp = dump.to_owned();
        }
        Ok(())
    }

    fn capture_item_float_exp(
        &mut self,
        menu: &str,
        _item: &str,
        key: u32,
        dump: &str,
    ) -> fastfile_iw4::Result<()> {
        let Some(def) = self.catalog.menus.get_mut(menu) else {
            return Ok(());
        };
        if let Some(item) = def.items.last_mut() {
            item.float_exp.push((key, dump.to_owned()));
        }
        Ok(())
    }

    fn capture_item_disabled_exp(
        &mut self,
        menu: &str,
        _item: &str,
        dump: &str,
    ) -> fastfile_iw4::Result<()> {
        let Some(def) = self.catalog.menus.get_mut(menu) else {
            return Ok(());
        };
        if let Some(item) = def.items.last_mut() {
            item.disabled_exp = dump.to_owned();
        }
        Ok(())
    }

    fn capture_item_text_exp(
        &mut self,
        menu: &str,
        _item: &str,
        dump: &str,
    ) -> fastfile_iw4::Result<()> {
        let Some(def) = self.catalog.menus.get_mut(menu) else {
            return Ok(());
        };
        if let Some(item) = def.items.last_mut() {
            item.text_exp = dump.to_owned();
        }
        Ok(())
    }

    fn capture_item_choice(
        &mut self,
        menu: &str,
        _item: &str,
        label: &str,
        value: fastfile_iw4::MenuChoiceValue<'_>,
    ) -> fastfile_iw4::Result<()> {
        let Some(def) = self.catalog.menus.get_mut(menu) else {
            return Ok(());
        };
        if label.is_empty() {
            return Ok(());
        }
        if let Some(item) = def.items.last_mut() {
            let value = match value {
                fastfile_iw4::MenuChoiceValue::Str(text) => text.to_owned(),
                fastfile_iw4::MenuChoiceValue::Float(number) => number.to_string(),
            };
            item.choices.push((label.to_owned(), value));
        }
        Ok(())
    }

    fn capture_item_material_exp(
        &mut self,
        menu: &str,
        _item: &str,
        dump: &str,
    ) -> fastfile_iw4::Result<()> {
        let Some(def) = self.catalog.menus.get_mut(menu) else {
            return Ok(());
        };
        if let Some(item) = def.items.last_mut() {
            item.material_exp = dump.to_owned();
        }
        Ok(())
    }

    fn capture_menu_script(
        &mut self,
        menu: &str,
        item: &str,
        kind: MenuScriptKind,
        script: &str,
    ) -> fastfile_iw4::Result<()> {
        self.record_script_set(RecordedEvent::Script(script.to_owned()));
        self.push_event(menu, kind, MenuEvent::Script(script.to_owned()));
        let Some(def) = self.catalog.menus.get_mut(menu) else {
            return Ok(());
        };
        if item.is_empty() {
            match kind {
                MenuScriptKind::OnOpen => def.on_open.push(script.to_owned()),
                MenuScriptKind::OnClose => def.on_close.push(script.to_owned()),
                MenuScriptKind::OnCloseRequest => def.on_close_request.push(script.to_owned()),
                MenuScriptKind::OnEsc => def.on_esc.push(script.to_owned()),
                _ => {}
            }
            return Ok(());
        }
        let Some(row) = def.items.last_mut() else {
            return Ok(());
        };
        match kind {
            MenuScriptKind::Action | MenuScriptKind::Accept => row.action.push(script.to_owned()),
            MenuScriptKind::MouseEnter | MenuScriptKind::MouseEnterText => {
                row.mouse_enter.push(script.to_owned())
            }
            MenuScriptKind::OnFocus => row.on_focus.push(script.to_owned()),
            _ => {}
        }
        Ok(())
    }

    fn capture_menu_set_local_var(
        &mut self,
        menu: &str,
        item: &str,
        kind: MenuScriptKind,
        var_kind: i32,
        name: &str,
        expr: &str,
    ) -> fastfile_iw4::Result<()> {
        self.record_script_set(RecordedEvent::SetLocalVar(
            var_kind,
            name.to_owned(),
            expr.to_owned(),
        ));
        self.push_event(
            menu,
            kind,
            MenuEvent::SetLocalVar {
                kind: var_kind,
                name: name.to_owned(),
                expr: expr.to_owned(),
            },
        );
        if !item.is_empty() || kind != MenuScriptKind::OnOpen {
            return Ok(());
        }
        let Some(def) = self.catalog.menus.get_mut(menu) else {
            return Ok(());
        };
        def.on_open_local_vars.push(MenuSetLocalVar {
            kind: var_kind,
            name: name.to_owned(),
            expr: expr.to_owned(),
        });
        Ok(())
    }

    fn begin_menu_script_set(&mut self, body: Ptr) {
        self.script_set_stack.push(body);
        self.script_sets.entry(ptr_key(body)).or_default();
    }

    fn end_menu_script_set(&mut self, insert_slot: Option<Ptr>) {
        let Some(body) = self.script_set_stack.pop() else {
            return;
        };
        if let Some(slot) = insert_slot {
            if let Some(scripts) = self.script_sets.get(&ptr_key(body)).cloned() {
                self.script_sets.insert(ptr_key(slot), scripts);
            }
        }
    }

    fn reuse_menu_script_set(
        &mut self,
        body: Ptr,
        menu: &str,
        item: &str,
        kind: MenuScriptKind,
    ) -> fastfile_iw4::Result<()> {
        let Some(events) = self.script_sets.get(&ptr_key(body)).cloned() else {
            return Ok(());
        };
        for event in events {
            match event {
                RecordedEvent::Script(script) => {
                    self.capture_menu_script(menu, item, kind, &script)?
                }
                RecordedEvent::Begin(condition) => {
                    self.begin_menu_event_branch(menu, item, kind, condition.as_deref())?
                }
                RecordedEvent::End => self.end_menu_event_branch(menu, item, kind)?,
                RecordedEvent::SetLocalVar(var_kind, name, expr) => {
                    self.capture_menu_set_local_var(menu, item, kind, var_kind, &name, &expr)?
                }
            }
        }
        Ok(())
    }

    fn begin_menu_event_branch(
        &mut self,
        _menu: &str,
        _item: &str,
        _kind: MenuScriptKind,
        condition: Option<&str>,
    ) -> fastfile_iw4::Result<()> {
        self.record_script_set(RecordedEvent::Begin(condition.map(str::to_owned)));
        self.branches
            .push((condition.map(str::to_owned), Vec::new()));
        Ok(())
    }

    fn end_menu_event_branch(
        &mut self,
        menu: &str,
        _item: &str,
        kind: MenuScriptKind,
    ) -> fastfile_iw4::Result<()> {
        self.record_script_set(RecordedEvent::End);
        let Some((condition, body)) = self.branches.pop() else {
            return Ok(());
        };
        let event = match condition {
            Some(condition) => MenuEvent::If {
                condition,
                then: body,
            },
            None => MenuEvent::Else(body),
        };
        self.push_event(menu, kind, event);
        Ok(())
    }

    fn capture_font(&mut self, rec: &FontCapture<'_>) -> fastfile_iw4::Result<()> {
        let mut glyphs = Vec::new();
        for chunk in rec.glyphs.chunks_exact(sz::GLYPH) {
            let mut row = [0u8; sz::GLYPH];
            row.copy_from_slice(chunk);
            glyphs.push(GlyphCapture::from_row(&row));
        }
        // Russian zone atlases index Cyrillic by Windows-1251 bytes. Keep
        // those entries for legacy localized strings and add Unicode aliases
        // for UTF-8 user text. Other language atlases must not reinterpret them.
        if self.russian_fonts {
            let aliases: Vec<_> = glyphs
                .iter()
                .filter_map(|glyph| {
                    let letter = match glyph.letter {
                        0xc0..=0xff => 0x0410 + glyph.letter - 0xc0,
                        0x80 => 0x0402,
                        0x81 => 0x0403,
                        0x83 => 0x0453,
                        0x8a => 0x0409,
                        0x8c => 0x040a,
                        0x8d => 0x040c,
                        0x8e => 0x040b,
                        0x8f => 0x040f,
                        0x90 => 0x0452,
                        0x9a => 0x0459,
                        0x9c => 0x045a,
                        0x9d => 0x045c,
                        0x9e => 0x045b,
                        0x9f => 0x045f,
                        0xa1 => 0x040e,
                        0xa2 => 0x045e,
                        0xa3 => 0x0408,
                        0xa5 => 0x0490,
                        0xa8 => 0x0401,
                        0xaa => 0x0404,
                        0xaf => 0x0407,
                        0xb2 => 0x0406,
                        0xb3 => 0x0456,
                        0xb4 => 0x0491,
                        0xb8 => 0x0451,
                        0xba => 0x0454,
                        0xbc => 0x0458,
                        0xbd => 0x0405,
                        0xbe => 0x0455,
                        0xbf => 0x0457,
                        _ => return None,
                    };
                    if glyphs.iter().any(|existing| existing.letter == letter) {
                        return None;
                    }
                    let mut alias = *glyph;
                    alias.letter = letter;
                    Some(alias)
                })
                .collect();
            glyphs.extend(aliases);
            if let Some(extended) = glyphs.get_mut(0x60..) {
                extended.sort_by_key(|glyph| glyph.letter);
            }
        }
        self.catalog.fonts.insert(
            rec.name.to_ascii_lowercase(),
            FontDef {
                name: rec.name.to_owned(),
                pixel_height: rec.pixel_height,
                material: rec.material.to_owned(),
                glow_material: rec.glow_material.to_owned(),
                glyphs,
            },
        );
        Ok(())
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) -> fastfile_iw4::Result<()> {
        let n = name.replace('\\', "/");
        let is_arena = n.eq_ignore_ascii_case("mp/basemaps.arena")
            || n.rsplit('/')
                .next()
                .is_some_and(|leaf| leaf.eq_ignore_ascii_case("basemaps.arena"));
        let lower = n.to_ascii_lowercase();
        let is_config = lower.ends_with(".cfg");
        let is_shock = lower.starts_with("shock/") && lower.ends_with(".shock");
        if !is_arena && !is_config && !is_shock {
            return Ok(());
        }
        let bytes = if zlib_compressed {
            asset_transport::inflate_zlib(data).unwrap_or_else(|_| data.to_vec())
        } else {
            let mut bytes = data.to_vec();
            if bytes.last() == Some(&0) {
                bytes.pop();
            }
            bytes
        };
        if let Ok(text) = String::from_utf8(bytes) {
            self.catalog.rawfiles.insert(n, text);
        }
        Ok(())
    }

    fn capture_string_table(
        &mut self,
        s: &ZoneStream<'_>,
        header: Ptr,
    ) -> fastfile_iw4::Result<()> {
        let name = match s.ptr_at(header, 0).ok() {
            Some(ZonePtr::Offset(p)) => s.cstr(s.resolve_alias(p)).ok().unwrap_or("").to_owned(),
            _ => return Ok(()),
        };
        if name.is_empty() {
            return Ok(());
        }
        let columns = s.i32_at(header, s.layout(4, 8)).unwrap_or(0).max(0) as usize;
        let rows = s.i32_at(header, s.layout(8, 12)).unwrap_or(0) as usize;
        let cells_n = columns.saturating_mul(rows);
        let arr = match s.ptr_at(header, s.layout(12, 16)).ok() {
            Some(ZonePtr::Offset(p)) => s.resolve_alias(p),
            _ => {
                self.catalog.string_tables.insert(
                    name.clone(),
                    CapturedStringTable {
                        name,
                        columns,
                        rows,
                        cells: Vec::new(),
                    },
                );
                return Ok(());
            }
        };
        let mut cells = Vec::with_capacity(cells_n);
        for i in 0..cells_n {
            let cell = arr.at(i * s.layout(sz::STRING_TABLE_CELL, 16));
            let value = match s.ptr_at(cell, 0).ok() {
                Some(ZonePtr::Offset(p)) => {
                    s.cstr(s.resolve_alias(p)).ok().unwrap_or("").to_owned()
                }
                _ => String::new(),
            };
            cells.push(value);
        }
        self.catalog.string_tables.insert(
            name.clone(),
            CapturedStringTable {
                name,
                columns,
                rows,
                cells,
            },
        );
        Ok(())
    }
}
