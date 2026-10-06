use std::collections::HashMap;

use asset_game::{CapturedStringTable, MenuCatalog};
use assets::{BoundWeapons, PreparedLocalizedStrings, PreparedWeapons, SessionCompass};
use bevy::prelude::*;
use bevy::ui::{Display, FocusPolicy};
use hud_iw4::{
    ExprError, ExprHost, LowAmmoWarningQuery, Operand, PERKS_INFO_HD_MENU, SCREEN_BLEND_BLURRED,
    SPECIALTY_NULL, WEAPON_NAME_FADE_DURATION_MS, WEAPON_NAME_FADE_TAIL_MS, WEAPONBAR_HD_MENU,
    draw_player_weapon_low_ammo_warning, fade_color, get_perk_slot_index, is_flashbanged,
    low_ammo_warning_color_pair, low_ammo_warning_pulse_frac, perk_code_key, vec4_lerp,
};
use net::{FrameClock, LocalPresentClient, PresentedSnapshot, WeaponSelect};
use playerstate_iw4::{ENTITYNUM_NONE, PM_TYPE_DEAD, PlayerState};
use weapon_iw4::{get_viewmodel_weapon_index, player_weapons_find_slot};

use crate::ammo::{
    OWNERDRAW_CLIP, OWNERDRAW_CLIP_LEFT, OWNERDRAW_COMPASS_RING, OWNERDRAW_LOW_AMMO,
    OWNERDRAW_OFFHAND_FRAG, OWNERDRAW_OFFHAND_SMOKE, OWNERDRAW_STOCK, OWNERDRAW_WEAPON_NAME,
    OWNERDRAW_WEAPON_NAME_KILLCAM, WeaponbarAmmo, offhand_ammo, offhand_weapon_index,
    paint_clip_pips, stock_digits, weaponbar_ammo,
};
use crate::chrome::{
    ChromeAssets, ChromeFrame, ChromeGapKind, ChromeMenuAnim, MenuVisOnError, OwnerDrawArgs,
    OwnerDrawPaint, execute_chrome_menu_ex, push_owner_pic, push_owner_text,
    push_owner_text_right_of_rect,
};
use crate::draw2d::{Draw2dOp, tessellate_fonts};
use crate::gaps::{GapCause, HudGap, HudPresentationGaps, ImageMiss};
use crate::gpu_list::{GpuListLatch, HudTessPass, TessJob};
use crate::images::HudImages;
use crate::playercard::UiLocalVars;
use crate::scorebar::milliseconds;
use crate::weapon_name::localized_weapon_name;

const EFLAGS_HIDE_AMMO_HUD: u32 = 0x100000;

const WEAPFLAGS_HIDE_AMMO_HUD: u32 = 0x80;

const WEAPON_NAME_RIGHT_INSET: f32 = 28.0;

#[derive(Component)]
pub(crate) struct WeaponbarRaster;

pub(crate) fn spawn_weaponbar(root: &mut ChildSpawnerCommands) {
    root.spawn((
        WeaponbarRaster,
        GpuListLatch::default(),
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

fn chrome_error(frame: &ChromeFrame) -> Option<String> {
    if let Some((item, error)) = frame.vis_errors.first() {
        return Some(format!("item {item}: {error}"));
    }
    frame
        .coverage
        .gap_ids
        .first()
        .map(|(item, kind)| format!("item {item}: {kind:?}"))
}

fn hide(pass: &mut HudTessPass) {
    pass.weaponbar = TessJob::Hide;
}

struct WeaponbarExprHost<'a> {
    weapons: Option<&'a BoundWeapons<'a>>,
    ps: Option<&'a PlayerState>,
    input: Option<&'a frame::HudInputView>,
    ms: i32,
    cg_time: i32,
    fov: f32,
    shock_screen_type: i32,
    in_killcam: bool,
    missilecam: bool,
    game_ended: bool,
    spectating_client: bool,
    dvars: sim::ScriptDvars<'a>,
    local_vars: &'a UiLocalVars,
    catalog: Option<&'a MenuCatalog>,
    menu: Option<&'a asset_game::MenuDef>,
    perk_slots: [u32; 8],
    weapon_script: String,

    lock: Option<hud_iw4::WeaponLockView>,
    frag_ammo: i32,
    smoke_ammo: i32,
    stock_ammo: i32,
    clip_ammo: i32,
}

impl WeaponbarExprHost<'_> {
    fn table(&self, name: &str) -> Option<&CapturedStringTable> {
        self.catalog.and_then(|c| c.string_table(name))
    }
}

impl ExprHost for WeaponbarExprHost<'_> {
    fn binding_label(&self, command: &str) -> Option<&str> {
        self.input?.binding_keys.get(command).map(String::as_str)
    }

    fn ui_active(&self) -> Result<i32, ExprError> {
        Ok(i32::from(self.input.is_some_and(|i| i.script_menu_open)))
    }
    fn action_slot_usable(&self, slot: i32) -> Result<i32, ExprError> {
        if self
            .ps
            .and_then(|ps| {
                usize::try_from(slot - 1)
                    .ok()
                    .and_then(|i| ps.action_slot_type.get(i))
            })
            .is_some_and(|kind| !matches!(kind, 0 | 1 | 2))
        {
            return Err(ExprError::Host("nightvision action slot not hosted"));
        }
        Ok(i32::from(
            self.ps
                .and_then(|ps| action_slot_weapon(ps, slot - 1, self.weapons))
                .is_some(),
        ))
    }
    fn key_binding(&self, command: &str) -> Result<Operand, ExprError> {
        let slot = command
            .strip_prefix("+actionslot ")
            .and_then(|s| s.parse::<usize>().ok())
            .and_then(|i| i.checked_sub(1));
        let text = slot
            .and_then(|i| self.input.and_then(|input| input.action_slot_keys.get(i)))
            .and_then(|key| key.as_deref())
            .unwrap_or("");
        Ok(Operand::Str(text.into()))
    }
    fn milliseconds(&self) -> i32 {
        self.ms
    }
    fn static_dvar_int(&self, index: i32) -> Result<i32, ExprError> {
        let name = self
            .menu
            .and_then(|menu| menu.static_dvar_name(index))
            .ok_or(ExprError::Host("static dvar name"))?;
        self.dvar_int(name)
    }
    fn team_field(&self, _field: &str) -> Result<Operand, ExprError> {
        Err(ExprError::Host("team field"))
    }
    fn player_field(&self, field: &str) -> Result<Operand, ExprError> {
        if field.eq_ignore_ascii_case("fragAmmo") {
            Ok(Operand::Int(self.frag_ammo))
        } else if field.eq_ignore_ascii_case("smokeAmmo") {
            Ok(Operand::Int(self.smoke_ammo))
        } else if field.eq_ignore_ascii_case("stockAmmo") {
            Ok(Operand::Int(self.stock_ammo))
        } else if field.eq_ignore_ascii_case("clipAmmo")
            || field.eq_ignore_ascii_case("clipAmmo_left")
        {
            Ok(Operand::Int(self.clip_ammo))
        } else {
            Err(ExprError::Host("player field"))
        }
    }
    fn other_team_field(&self, _field: &str) -> Result<Operand, ExprError> {
        Err(ExprError::Host("other team field"))
    }
    fn local_var_string(&self, name: &str) -> Result<Operand, ExprError> {
        Ok(Operand::Str(self.local_vars.int(name).to_string()))
    }
    fn local_var_int(&self, name: &str) -> Result<i32, ExprError> {
        Ok(self.local_vars.int(name))
    }
    fn time_left(&self) -> Result<i32, ExprError> {
        Err(ExprError::Host("timeleft"))
    }
    fn score_at_rank(&self, _rank: i32) -> Result<i32, ExprError> {
        Err(ExprError::Host("score"))
    }
    fn gametype_name(&self) -> Result<Operand, ExprError> {
        Err(ExprError::Host("gametype"))
    }
    fn weapon_lock(&self) -> Result<hud_iw4::WeaponLockView, ExprError> {
        self.lock.ok_or(ExprError::Host("weapon lock"))
    }
    fn emp_jammed(&self) -> Result<i32, ExprError> {
        Ok(i32::from(
            self.ps.is_some_and(|ps| ps.other_flags & 0x400 != 0),
        ))
    }
    fn dvar_int(&self, name: &str) -> Result<i32, ExprError> {
        if let Some(value) = self.dvars.int(name) {
            Ok(value)
        } else if name.eq_ignore_ascii_case("scr_gameended") {
            Ok(i32::from(self.game_ended))
        } else if name.eq_ignore_ascii_case("g_hardcore")
            || name.eq_ignore_ascii_case("onlinegame")
            || name.eq_ignore_ascii_case("xblive_privatematch")
            || name.eq_ignore_ascii_case("cg_thirdPersonSpectator")
        {
            Ok(0)
        } else if name.eq_ignore_ascii_case("scr_showperksonspawn") {
            Ok(1)
        } else {
            Err(ExprError::Host("dvarint"))
        }
    }
    fn dvar_bool(&self, name: &str) -> Result<i32, ExprError> {
        self.dvar_int(name)
    }
    fn dvar_float(&self, name: &str) -> Result<f32, ExprError> {
        self.dvars
            .string(name)
            .and_then(|value| value.parse().ok())
            .or_else(|| name.eq_ignore_ascii_case("cg_fov").then_some(self.fov))
            .ok_or(ExprError::Host("dvarfloat"))
    }
    fn table_lookup(
        &self,
        table: &str,
        col0: i32,
        key: &str,
        result_col: i32,
    ) -> Result<Operand, ExprError> {
        let Some(t) = self.table(table) else {
            return Ok(Operand::Str(String::new()));
        };
        match t.lookup_row_in_col(col0, key) {
            Some(row) => Ok(Operand::Str(String::from(t.cell(row, result_col)))),
            None => Ok(Operand::Str(String::new())),
        }
    }
    fn table_lookup_by_row(&self, table: &str, row: i32, col: i32) -> Result<Operand, ExprError> {
        let Some(t) = self.table(table) else {
            return Ok(Operand::Str(String::new()));
        };
        Ok(Operand::Str(String::from(t.cell(row, col))))
    }
    fn get_perk(&self, name: &str) -> Result<Operand, ExprError> {
        let Some(slot) = get_perk_slot_index(name) else {
            return Ok(Operand::Str(SPECIALTY_NULL.to_owned()));
        };
        let code = self.perk_slots.get(slot).copied().unwrap_or(0);
        let Some(key) = perk_code_key(code) else {
            return Ok(Operand::Str(SPECIALTY_NULL.to_owned()));
        };
        let Some(t) = self.table("mp/perkTable.csv") else {
            return Err(ExprError::Host("perkTable"));
        };
        match t.lookup_row_in_col(0, &key) {
            Some(row) => Ok(Operand::Str(String::from(t.cell(row, 1)))),
            None => Ok(Operand::Str(SPECIALTY_NULL.to_owned())),
        }
    }
    fn in_killcam(&self) -> Result<i32, ExprError> {
        Ok(i32::from(self.in_killcam))
    }
    fn in_killcam_npc(&self) -> Result<i32, ExprError> {
        Ok(i32::from(
            self.in_killcam
                && self
                    .ps
                    .is_some_and(|ps| ps.kill_cam_entity != ENTITYNUM_NONE),
        ))
    }
    fn is_reloading(&self) -> Result<i32, ExprError> {
        Ok(i32::from(self.ps.is_some_and(|ps| {
            [ps.weaponstate_primary, ps.weaponstate_secondary]
                .iter()
                .any(|state| (8..=12).contains(state))
        })))
    }
    fn missilecam(&self) -> Result<i32, ExprError> {
        Ok(i32::from(self.missilecam))
    }
    fn flashbanged(&self) -> Result<i32, ExprError> {
        let Some(ps) = self.ps else {
            return Ok(0);
        };
        Ok(is_flashbanged(
            self.cg_time,
            ps.shellshock_time,
            ps.shellshock_duration,
            self.shock_screen_type,
        ))
    }
    fn weapon_name(&self) -> Result<Operand, ExprError> {
        Ok(Operand::Str(self.weapon_script.clone()))
    }
    fn spectating_client(&self) -> Result<i32, ExprError> {
        Ok(i32::from(self.spectating_client))
    }
    fn is_item_unlocked(&self, _item: &str) -> Result<i32, ExprError> {
        Ok(0)
    }
}

struct OwnerDrawState<'a> {
    ps: &'a PlayerState,
    weapons: Option<&'a BoundWeapons<'a>>,
    ammo: Option<WeaponbarAmmo>,
    name: Option<String>,
    select_time: i32,
    cg_time: i32,
    yaw: f32,
    north_yaw: f32,
    ticker_stretch: f32,
    hide_ammo: bool,
}

fn paint_owner(
    state: &OwnerDrawState<'_>,
    args: OwnerDrawArgs<'_>,
    frame: &mut ChromeFrame,
) -> OwnerDrawPaint {
    match args.item.owner_draw {
        145 => paint_heading_tape(state, &args, frame),
        190 if state.ps.e_flags & 0x20000 == 0 => OwnerDrawPaint::Painted,
        171..=174 => paint_action_slot(state, args, frame),
        OWNERDRAW_STOCK => paint_stock(state, &args, frame),
        OWNERDRAW_CLIP => match state.ammo.as_ref() {
            Some(ammo) => paint_clip_pips(&args, ammo, 0, frame),
            None => OwnerDrawPaint::Painted,
        },
        OWNERDRAW_CLIP_LEFT => match state.ammo.as_ref() {
            Some(ammo) => paint_clip_pips(&args, ammo, 1, frame),
            None => OwnerDrawPaint::Painted,
        },
        OWNERDRAW_WEAPON_NAME => paint_weapon_name(state, &args, frame, true),
        OWNERDRAW_WEAPON_NAME_KILLCAM => paint_weapon_name(state, &args, frame, false),
        OWNERDRAW_OFFHAND_FRAG => paint_offhand(state, &args, frame, state.ps.offhand_primary),
        OWNERDRAW_OFFHAND_SMOKE => paint_offhand(state, &args, frame, state.ps.offhand_secondary),
        OWNERDRAW_COMPASS_RING => paint_compass_ring(state, &args, frame),
        OWNERDRAW_LOW_AMMO => paint_low_ammo(state, &args, frame),
        _ => OwnerDrawPaint::Gap(ChromeGapKind::OwnerDraw),
    }
}

fn paint_stock(
    state: &OwnerDrawState<'_>,
    args: &OwnerDrawArgs<'_>,
    frame: &mut ChromeFrame,
) -> OwnerDrawPaint {
    if state.hide_ammo {
        return OwnerDrawPaint::Painted;
    }
    let Some(ammo) = state.ammo.as_ref() else {
        return OwnerDrawPaint::Painted;
    };
    let Some(count) = ammo.stock else {
        return OwnerDrawPaint::Painted;
    };
    if matches!(
        ammo.kind,
        hud_iw4::AmmoCounterClipKind::None | hud_iw4::AmmoCounterClipKind::AltWeapon
    ) {
        return OwnerDrawPaint::Painted;
    }
    match push_owner_text(args, &stock_digits(count), args.color, frame) {
        Ok(()) => OwnerDrawPaint::Painted,
        Err(kind) => OwnerDrawPaint::Gap(kind),
    }
}

fn paint_low_ammo(
    state: &OwnerDrawState<'_>,
    args: &OwnerDrawArgs<'_>,
    frame: &mut ChromeFrame,
) -> OwnerDrawPaint {
    let Some(ammo) = state.ammo.as_ref() else {
        return OwnerDrawPaint::Painted;
    };
    let Some(clip) = ammo.clip else {
        return OwnerDrawPaint::Painted;
    };
    let Some(stock) = ammo.stock else {
        return OwnerDrawPaint::Painted;
    };
    let hands = if ammo.dual { 2 } else { 1 };
    let clip_alt = ammo.clip_alt.unwrap_or(0);
    let Some(kind) = draw_player_weapon_low_ammo_warning(LowAmmoWarningQuery {
        pm_type: state.ps.pm_type,
        e_flags: state.ps.e_flags,
        weapon: state.ps.weapon,
        ammo_counter_clip: ammo.ammo_counter_clip,
        weaponstate: [state.ps.weaponstate_primary, state.ps.weaponstate_secondary],
        hands,
        clip: [clip, clip_alt],
        clip_size: ammo.clip_size,
        stock,
        threshold: ammo.low_ammo_warning_threshold,
        clip_only: ammo.clip_only,
    }) else {
        return OwnerDrawPaint::Painted;
    };
    let Some(table) = args.assets.localize else {
        return OwnerDrawPaint::Gap(ChromeGapKind::Localize);
    };
    let Some(text) = table.text(kind.loc_key()) else {
        return OwnerDrawPaint::Gap(ChromeGapKind::Localize);
    };
    let (c1, c2) = low_ammo_warning_color_pair(kind);
    let pulse = low_ammo_warning_pulse_frac(state.cg_time);
    let lerped = vec4_lerp(c1, c2, pulse);
    let color = [
        lerped[0].clamp(0.0, 1.0),
        lerped[1].clamp(0.0, 1.0),
        lerped[2].clamp(0.0, 1.0),
        lerped[3].clamp(0.0, 1.0),
    ];
    match push_owner_text(args, text, color, frame) {
        Ok(()) => OwnerDrawPaint::Painted,
        Err(kind) => OwnerDrawPaint::Gap(kind),
    }
}

fn paint_weapon_name(
    state: &OwnerDrawState<'_>,
    args: &OwnerDrawArgs<'_>,
    frame: &mut ChromeFrame,
    fade: bool,
) -> OwnerDrawPaint {
    if state.hide_ammo {
        return OwnerDrawPaint::Painted;
    }
    let mut color = args.color;
    if fade {
        let Some(alpha) = fade_color(
            state.cg_time,
            state.select_time,
            WEAPON_NAME_FADE_DURATION_MS,
            WEAPON_NAME_FADE_TAIL_MS,
        ) else {
            return OwnerDrawPaint::Painted;
        };
        color[3] *= alpha;
    }
    let Some(name) = state.name.as_deref() else {
        return OwnerDrawPaint::Painted;
    };
    match push_owner_text_right_of_rect(args, name, WEAPON_NAME_RIGHT_INSET, color, frame) {
        Ok(()) => OwnerDrawPaint::Painted,
        Err(kind) => OwnerDrawPaint::Gap(kind),
    }
}

fn selected_weapon_index(ps: &PlayerState, selected: u32) -> u32 {
    let owned = i32::try_from(selected)
        .is_ok_and(|weapon| weapon != 0 && player_weapons_find_slot(&ps.weapons, weapon) >= 0);
    if owned { selected } else { ps.weapon }
}

fn paint_offhand(
    state: &OwnerDrawState<'_>,
    args: &OwnerDrawArgs<'_>,
    frame: &mut ChromeFrame,
    class: i32,
) -> OwnerDrawPaint {
    if state.hide_ammo || args.color[3] <= 0.0 {
        return OwnerDrawPaint::Painted;
    }
    if state.ps.pm_type >= PM_TYPE_DEAD {
        return OwnerDrawPaint::Painted;
    }
    let Some(weapons) = state.weapons else {
        return OwnerDrawPaint::Painted;
    };
    let Some(index) = offhand_weapon_index(state.ps, weapons, class) else {
        return OwnerDrawPaint::Painted;
    };
    let Some(image) = weapons.registry().hud_icon_image_of(index) else {
        return OwnerDrawPaint::Gap(ChromeGapKind::MaterialExp);
    };
    push_owner_pic(
        args,
        image.to_owned(),
        weapons
            .registry()
            .hud_icon_namespace_of(index)
            .expect("owned weapon has a namespace"),
        args.color,
        Draw2dOp::StretchPic,
        frame,
    );
    OwnerDrawPaint::Painted
}

fn paint_heading_tape(
    state: &OwnerDrawState<'_>,
    args: &OwnerDrawArgs<'_>,
    frame: &mut ChromeFrame,
) -> OwnerDrawPaint {
    let Some(material) = background_stem(&args.item.background) else {
        return OwnerDrawPaint::Gap(ChromeGapKind::MaterialExp);
    };
    let before = frame.list.cmds.len();
    push_owner_pic(
        args,
        material,
        crate::images::HUD_CHROME_NAMESPACE,
        args.color,
        Draw2dOp::StretchPic,
        frame,
    );
    if let Some(cmd) = frame.list.cmds.get_mut(before) {
        let center = -(state.yaw - state.north_yaw) / 360.0;
        cmd.s0 = center - state.ticker_stretch * 0.5;
        cmd.s1 = center + state.ticker_stretch * 0.5;
    }
    OwnerDrawPaint::Painted
}

fn paint_compass_ring(
    state: &OwnerDrawState<'_>,
    args: &OwnerDrawArgs<'_>,
    frame: &mut ChromeFrame,
) -> OwnerDrawPaint {
    let Some(material) = background_stem(&args.item.background) else {
        return OwnerDrawPaint::Gap(ChromeGapKind::MaterialExp);
    };
    let deg = -(state.yaw - state.north_yaw);
    push_owner_pic(
        args,
        material,
        crate::images::HUD_CHROME_NAMESPACE,
        args.color,
        Draw2dOp::RotateSt {
            center_s: 0.5,
            center_t: 0.5,
            radius_st: 0.5,
            scale_final_s: 1.0,
            scale_final_t: 1.0,
            deg,
        },
        frame,
    );
    OwnerDrawPaint::Painted
}

fn background_stem(background: &str) -> Option<String> {
    let stem = background.trim_start_matches(',').trim();
    if stem.is_empty() {
        None
    } else {
        Some(stem.to_owned())
    }
}

fn ads_javelin(ps: &PlayerState, weapons: &BoundWeapons<'_>) -> bool {
    let viewmodel = get_viewmodel_weapon_index(ps);
    viewmodel > 0
        && ps.f_weapon_pos_frac == 1.0
        && weapons
            .row(viewmodel)
            .and_then(|weapon| weapon.hud_facts())
            .is_some_and(|facts| facts.guided_overlay())
}

#[derive(Clone, Default)]
pub(crate) struct HudPlayerVis {
    pub ui_active: bool,
    pub flashbanged: bool,
    pub weapon_script: String,
    pub ads_javelin: bool,
    pub missilecam: bool,
    pub emp_jammed: bool,
    pub game_ended: bool,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct HudPlayerVisInput<'w> {
    input: Option<Res<'w, frame::HudInputView>>,
    weapons: Option<Res<'w, PreparedWeapons>>,
    cg_clock: Res<'w, FrameClock>,
}

impl HudPlayerVisInput<'_> {
    pub(crate) fn read(&self, presented: &PresentedSnapshot, local: sim::ClientId) -> HudPlayerVis {
        let ui_active = self.input.as_ref().is_some_and(|i| i.script_menu_open);
        let Some(ps) = presented.player(local) else {
            return HudPlayerVis {
                ui_active,
                ..HudPlayerVis::default()
            };
        };
        let snapshot = presented.snapshot();
        let bound = self
            .weapons
            .as_deref()
            .and_then(|weapons| weapons.for_snapshot(presented.weapon_epoch()).ok());
        let weapons = bound.as_ref();
        HudPlayerVis {
            ui_active,
            flashbanged: is_flashbanged(
                self.cg_clock.time(),
                ps.shellshock_time,
                ps.shellshock_duration,
                presented
                    .shellshock(local)
                    .map_or(SCREEN_BLEND_BLURRED, |shock| shock.screen_type),
            ) != 0,
            weapon_script: weapons
                .map(|w| {
                    w.registry()
                        .script_name_of(get_viewmodel_weapon_index(ps))
                        .to_owned()
                })
                .unwrap_or_default(),
            ads_javelin: weapons.is_some_and(|w| ads_javelin(ps, w)),
            missilecam: snapshot
                .and_then(|s| s.meta.for_client(local))
                .is_some_and(|m| m.remote_missile.is_some()),
            emp_jammed: ps.other_flags & 0x400 != 0,
            game_ended: snapshot.is_some_and(|s| {
                matches!(
                    s.meta.phase,
                    sim::MatchPhase::Intermission | sim::MatchPhase::PostGame
                )
            }),
        }
    }
}

fn weapon_lock_view(
    ps: &PlayerState,
    weapons: &BoundWeapons<'_>,
    meta: Option<&sim::ClientSnapshotMeta>,
    time_ms: i32,
    projection: Option<&Projection>,
    surface: &crate::surface::Hud2dSurface,
) -> hud_iw4::WeaponLockView {
    let ads_javelin = ads_javelin(ps, weapons);
    let lock = meta
        .map(|m| m.weapon_lock)
        .filter(|lock| lock.weapon == ps.weapon && ps.health > 0)
        .unwrap_or_default();
    let mut screen_pos = [0.0; 2];
    if lock.flags & 3 != 0 {
        if let Some(Projection::Perspective(projection)) = projection {
            let eye = Vec3::from_array(ps.origin) + Vec3::Z * ps.view_height_current;
            let delta = Vec3::from_array(lock.target) - eye;
            let (forward, right, up) = math_iw4::angle_vectors(ps.viewangles);
            let depth = delta.dot(Vec3::from_array(forward));
            if depth > 0.0 {
                let scale = surface.height() * 0.5 / ((projection.fov * 0.5).tan() * depth);
                screen_pos = [
                    surface.width() * 0.5 + delta.dot(Vec3::from_array(right)) * scale,
                    surface.height() * 0.5 - delta.dot(Vec3::from_array(up)) * scale,
                ];
            }
        }
    }
    hud_iw4::WeaponLockView {
        ads_javelin,
        time_ms,
        attack_top: lock.flags & 4 != 0,
        attack_direct: lock.flags & 8 != 0,
        locking: lock.locking(),
        locked: lock.locked(),
        too_close: lock.too_close(),
        screen_pos,
    }
}

fn ammo_hud_hidden(ps: &PlayerState) -> bool {
    (ps.e_flags & EFLAGS_HIDE_AMMO_HUD) != 0 || (ps.weap_flags & WEAPFLAGS_HIDE_AMMO_HUD) != 0
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct WeaponbarInput<'w, 's> {
    settings: Res<'w, frame::GameSettings>,
    select: Res<'w, WeaponSelect>,
    input: Option<Res<'w, frame::HudInputView>>,
    cameras: Query<'w, 's, &'static Projection, With<Camera3d>>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_weaponbar(
    surface: Res<crate::surface::Hud2dSurface>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    weapons: Option<Res<PreparedWeapons>>,
    compass: Option<Res<SessionCompass>>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut pass: ResMut<HudTessPass>,
    mut exprs: ResMut<crate::expr_cache::MenuExprCache>,
    local_vars: Res<UiLocalVars>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    view: Option<Res<frame::ViewSubject>>,
    cg_clock: Res<FrameClock>,
    client_input: WeaponbarInput,
) {
    let weapons = weapons
        .as_deref()
        .and_then(|weapons| weapons.for_snapshot(presented.weapon_epoch()).ok());
    if !surface.is_ready() {
        return;
    }
    let Some(ps) = presented.player(local.0) else {
        gaps.clear(HudGap::PerkDisplay);
        gaps.clear(HudGap::CompassRing);
        hide(&mut pass);
        return;
    };
    let Some(catalog) = catalog.as_ref() else {
        let perks_live = local_vars.int("ui_show_perks") != 0;
        if perks_live {
            gaps.raise(GapCause::PerkNoCatalog);
        } else {
            gaps.clear(HudGap::PerkDisplay);
        }
        gaps.raise(GapCause::CompassRingNoMenu {
            name: WEAPONBAR_HD_MENU.to_owned(),
        });
        hide(&mut pass);
        return;
    };

    let meta = presented
        .snapshot()
        .and_then(|s| s.meta.for_client(local.0));
    let ammo = weapons.as_ref().and_then(|w| weaponbar_ammo(ps, w, meta));
    let viewmodel = get_viewmodel_weapon_index(ps);
    let weapon_script = weapons
        .as_ref()
        .map(|w| w.registry().script_name_of(viewmodel))
        .unwrap_or_default();
    let name = localized_weapon_name(
        selected_weapon_index(ps, client_input.select.index),
        weapons.as_ref(),
        strings.as_deref(),
        &mut gaps,
    );
    let hide_ammo = ammo_hud_hidden(ps);
    let frag_ammo = weapons
        .as_ref()
        .map(|w| offhand_ammo(ps, w, ps.offhand_primary))
        .unwrap_or(0);
    let smoke_ammo = weapons
        .as_ref()
        .map(|w| offhand_ammo(ps, w, ps.offhand_secondary))
        .unwrap_or(0);
    let mut host = WeaponbarExprHost {
        weapons: weapons.as_ref(),
        ps: Some(ps),
        input: client_input.input.as_deref(),
        ms: milliseconds() as i32,
        cg_time: cg_clock.time(),
        fov: client_input.settings.fov,
        shock_screen_type: presented
            .shellshock(local.0)
            .map_or(SCREEN_BLEND_BLURRED, |shock| shock.screen_type),
        in_killcam: view.as_deref().is_some_and(|v| v.in_killcam()),
        missilecam: meta.is_some_and(|m| m.remote_missile.is_some()),
        game_ended: presented.snapshot().is_some_and(|s| {
            matches!(
                s.meta.phase,
                sim::MatchPhase::Intermission | sim::MatchPhase::PostGame
            )
        }),
        spectating_client: false,
        dvars: presented
            .snapshot()
            .map(|s| s.meta.script_dvars(local.0))
            .unwrap_or_default(),
        local_vars: &local_vars,
        catalog: Some(catalog),
        menu: None,
        perk_slots: ps.perk_slots,
        weapon_script,
        lock: Some(
            weapons
                .as_ref()
                .map(|weapons| {
                    weapon_lock_view(
                        ps,
                        weapons,
                        meta,
                        cg_clock.time(),
                        client_input.cameras.iter().next(),
                        &surface,
                    )
                })
                .unwrap_or_default(),
        ),
        frag_ammo,
        smoke_ammo,
        stock_ammo: ammo.as_ref().and_then(|a| a.stock).unwrap_or(0),
        clip_ammo: ammo.as_ref().and_then(|a| a.clip).unwrap_or(0),
    };
    let north_yaw = compass.as_ref().and_then(|c| c.north_yaw).unwrap_or(0.0);
    let owner_state = OwnerDrawState {
        ps,
        weapons: weapons.as_ref(),
        ammo,
        name,
        select_time: client_input.select.time,
        cg_time: cg_clock.time(),
        yaw: ps.viewangles[1],
        north_yaw,
        ticker_stretch: host
            .dvars
            .string("compassTickertapeStretch")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(0.5)
            .clamp(0.01, 1.0),
        hide_ammo,
    };

    let mut list = crate::draw2d::Draw2dList::default();
    let mut weaponbar_vis = false;
    let mut weaponbar_failed = false;
    let mut ring_cmds = 0usize;
    if let Some(menu) = catalog.get(WEAPONBAR_HD_MENU) {
        host.menu = Some(menu);
        let mut hook = |args: OwnerDrawArgs<'_>, frame: &mut ChromeFrame| {
            let ring = args.item.owner_draw == OWNERDRAW_COMPASS_RING;
            let before = frame.list.cmds.len();
            let out = paint_owner(&owner_state, args, frame);
            if ring {
                ring_cmds += frame.list.cmds.len().saturating_sub(before);
            }
            out
        };
        let frame = execute_chrome_menu_ex(
            menu,
            &host,
            &surface,
            ChromeAssets {
                catalog: Some(catalog),
                localize: strings.as_ref().map(|s| &s.0),
            },
            ChromeMenuAnim::IDENTITY,
            &mut exprs,
            MenuVisOnError::HideAll,
            Some(&mut hook),
        );
        if let Some(error) = chrome_error(&frame) {
            weaponbar_failed = true;
            gaps.raise(GapCause::WeaponbarPaint { error });
        }
        weaponbar_vis =
            frame.coverage.vis_false < frame.coverage.items_total || !frame.list.cmds.is_empty();
        list.cmds.extend(frame.list.cmds);
    } else {
        gaps.raise(GapCause::CompassRingNoMenu {
            name: WEAPONBAR_HD_MENU.to_owned(),
        });
    }

    for name in [
        "dpad_hd",
        "javelin_overlay_hd",
        "missilecam_hud_hd",
        "remote_chopper_overlay_hd",
        "ac130_hud_hd",
    ] {
        let Some(menu) = catalog.get(name) else {
            continue;
        };
        host.menu = Some(menu);
        let mut hook = |args: OwnerDrawArgs<'_>, frame: &mut ChromeFrame| {
            paint_owner(&owner_state, args, frame)
        };
        let frame = execute_chrome_menu_ex(
            menu,
            &host,
            &surface,
            ChromeAssets {
                catalog: Some(catalog),
                localize: strings.as_ref().map(|s| &s.0),
            },
            ChromeMenuAnim::IDENTITY,
            &mut exprs,
            MenuVisOnError::HideAll,
            Some(&mut hook),
        );
        if let Some(error) = chrome_error(&frame) {
            gaps.raise(GapCause::WeaponbarPaint { error });
            weaponbar_failed = true;
        }
        list.cmds.extend(frame.list.cmds);
    }

    let perks_live = {
        let show = local_vars.int("ui_show_perks");
        host.ms.wrapping_sub(show) < 5000 || host.in_killcam
    };
    let mut perk_painted = false;
    let mut perk_failed = false;
    if let Some(menu) = catalog.get(PERKS_INFO_HD_MENU) {
        host.menu = Some(menu);
        if host.table("mp/perkTable.csv").is_none() && perks_live {
            gaps.raise(GapCause::PerkTableMissing {
                name: String::from("mp/perkTable.csv"),
            });
        }
        let mut hook = |args: OwnerDrawArgs<'_>, frame: &mut ChromeFrame| {
            paint_owner(&owner_state, args, frame)
        };
        let frame = execute_chrome_menu_ex(
            menu,
            &host,
            &surface,
            ChromeAssets {
                catalog: Some(catalog),
                localize: strings.as_ref().map(|s| &s.0),
            },
            ChromeMenuAnim::IDENTITY,
            &mut exprs,
            MenuVisOnError::HideAll,
            Some(&mut hook),
        );
        if let Some(error) = chrome_error(&frame) {
            perk_failed = true;
            gaps.raise(GapCause::PerkPaint { error });
        }
        perk_painted = !frame.list.cmds.is_empty();
        if perks_live
            && frame.list.cmds.is_empty()
            && frame.coverage.vis_false < frame.coverage.items_total
        {
            gaps.raise(GapCause::PerkEmptyPaint {
                name: PERKS_INFO_HD_MENU.to_owned(),
            });
        }
        list.cmds.extend(frame.list.cmds);
    } else if perks_live {
        gaps.raise(GapCause::PerkNoMenu {
            name: PERKS_INFO_HD_MENU.to_owned(),
        });
    }

    let mut fonts: HashMap<String, &asset_game::FontDef> = HashMap::new();
    let mut ring_miss = false;
    let mut perk_miss = false;
    for cmd in &list.cmds {
        if hud_images
            .get(cmd.material_namespace, &cmd.material, &mut images)
            .is_none()
        {
            let miss = if hud_images.has_games_root() {
                ImageMiss::NotDecoded
            } else {
                ImageMiss::NoGamesRoot
            };
            if matches!(
                cmd.provenance,
                crate::draw2d::Draw2dProvenance::OwnerDraw(OWNERDRAW_COMPASS_RING)
            ) || cmd.material.contains("compass_letters")
            {
                ring_miss = true;
                gaps.raise(GapCause::CompassRingMaterialMissing {
                    name: cmd.material.clone(),
                    miss,
                });
            } else if matches!(&cmd.provenance, crate::Draw2dProvenance::MenuItem { menu, .. } if menu == PERKS_INFO_HD_MENU)
            {
                perk_miss = true;
                gaps.raise(GapCause::PerkMaterialMissing {
                    name: cmd.material.clone(),
                    miss,
                });
            } else {
                weaponbar_failed = true;
                gaps.raise(GapCause::WeaponbarPaint {
                    error: format!("image `{}` is {miss}", cmd.material),
                });
            }
        }
        if let Draw2dOp::TextRun { font, .. } = &cmd.op {
            if fonts.contains_key(font) {
                continue;
            }
            if let Some(def) = catalog.font(font) {
                fonts.insert(font.clone(), def);
            }
        }
    }

    if (!ring_miss && ring_cmds > 0) || !weaponbar_vis {
        gaps.clear(HudGap::CompassRing);
    }
    if !weaponbar_failed {
        gaps.clear(HudGap::Weaponbar);
    }
    if !perk_failed && ((perk_painted && !perk_miss) || !perks_live) {
        gaps.clear(HudGap::PerkDisplay);
    }

    let (quads, _) = tessellate_fonts(&list, &fonts);
    if quads.is_empty() {
        hide(&mut pass);
        return;
    }
    pass.weaponbar = TessJob::Quads(quads);
}

fn action_slot_weapon(
    ps: &PlayerState,
    slot: i32,
    weapons: Option<&BoundWeapons<'_>>,
) -> Option<u32> {
    let slot = usize::try_from(slot).ok()?;
    if ps.weap_flags & 2 != 0 {
        return None;
    }
    if ps.action_slot_type.get(slot) == Some(&2) {
        let weapons = weapons?;
        let weapon = if weapons.row(ps.weapon)?.hud_facts()?.is_alternate() {
            ps.weapon_primary
        } else {
            weapons.registry().alternate_of(ps.weapon)
        };
        return (weapon != 0).then_some(weapon);
    }
    if ps.action_slot_type.get(slot) != Some(&1) {
        return None;
    }
    let weapon = *ps.action_slot_param.get(slot)?;
    (weapon != 0 && ps.weapons.contains(&weapon)).then_some(weapon as u32)
}

fn action_slot_atlas_uv(atlas: [u8; 2], time_ms: i32) -> [f32; 4] {
    let rows = usize::from(atlas[0].max(1));
    let cols = usize::from(atlas[1].max(1));
    let frame = (time_ms.max(0) as usize / 50) % (rows * cols);
    let s0 = (frame % cols) as f32 / cols as f32;
    let t0 = (frame / cols) as f32 / rows as f32;
    [s0, t0, s0 + 1.0 / cols as f32, t0 + 1.0 / rows as f32]
}

fn paint_action_slot(
    state: &OwnerDrawState<'_>,
    mut args: OwnerDrawArgs<'_>,
    frame: &mut ChromeFrame,
) -> OwnerDrawPaint {
    if state
        .ps
        .action_slot_type
        .get((args.item.owner_draw - 171) as usize)
        .is_some_and(|kind| !matches!(kind, 0 | 1 | 2))
    {
        return OwnerDrawPaint::Gap(ChromeGapKind::OwnerDraw);
    }
    let Some(weapon) = action_slot_weapon(state.ps, args.item.owner_draw - 171, state.weapons)
    else {
        return OwnerDrawPaint::Painted;
    };
    let Some(weapons) = state.weapons else {
        return OwnerDrawPaint::Gap(ChromeGapKind::MaterialExp);
    };
    let Some((material, ratio)) = weapons.registry().dpad_icon_of(weapon) else {
        return OwnerDrawPaint::Gap(ChromeGapKind::MaterialExp);
    };

    match ratio {
        1 => args.rect.w *= 2.0,
        2 => {
            args.rect.w *= 2.0;
            args.rect.y += args.rect.h * 0.25;
            args.rect.h *= 0.5;
        }
        _ => {}
    }
    let cmd_index = frame.list.cmds.len();
    push_owner_pic(
        &args,
        material.to_owned(),
        weapons
            .registry()
            .component_namespace_of(weapon, asset_game::WeaponComponent::Material)
            .expect("owned weapon namespace"),
        args.color,
        Draw2dOp::StretchPic,
        frame,
    );
    if let Some(atlas) = weapons.registry().dpad_icon_atlas_of(weapon) {
        let [s0, t0, s1, t1] = action_slot_atlas_uv(atlas, state.cg_time);
        if let Some(cmd) = frame.list.cmds.get_mut(cmd_index) {
            cmd.s0 = s0;
            cmd.t0 = t0;
            cmd.s1 = s1;
            cmd.t1 = t1;
        }
    }
    OwnerDrawPaint::Painted
}
