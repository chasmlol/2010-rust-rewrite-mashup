use std::collections::{HashMap, HashSet, VecDeque};

use asset_game::MenuCatalog;
use assets::{PreparedLocalizedStrings, PreparedWeapons};
use bevy::prelude::*;
use hud_iw4::{
    GAME_MSG_WIN0_FADE_IN_TIME_MS, GAME_MSG_WIN0_FADE_OUT_TIME_MS, GAME_MSG_WIN0_LINE_COUNT,
    GAME_MSG_WIN0_MSG_TIME_MS, GAME_MSG_WIN0_SCROLL_TIME_MS, KILLICON_DIED, gamenotify_line,
    killicon_stretch_uv, killicon_virtual_size, normalized_text_scale, obituary_mod,
    obituary_mod_killicon,
};
use net::{FrameClock, LocalPresentClient};

use crate::chrome::text_width;
use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
use crate::font_overlay::HUD_SMALL_FONT;
use crate::gaps::{GapCause, HudGap, HudPresentationGaps};
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImages;

#[derive(Clone, Debug)]
enum KillfeedLine {
    Obituary {
        start_ms: i32,
        end_ms: i32,
        icon: String,
        icon_namespace: asset_core::AssetNamespace,
        attacker: String,
        has_attacker: bool,
        victim: String,
        attacker_team: i32,
        victim_team: i32,

        kill_icon_ratio: i32,

        flip_kill_icon: bool,
    },
    Notify {
        start_ms: i32,
        end_ms: i32,
        text: String,
        name_empty: bool,
    },
}

impl KillfeedLine {
    fn start_ms(&self) -> i32 {
        match self {
            Self::Obituary { start_ms, .. } | Self::Notify { start_ms, .. } => *start_ms,
        }
    }

    fn end_ms(&self) -> i32 {
        match self {
            Self::Obituary { end_ms, .. } | Self::Notify { end_ms, .. } => *end_ms,
        }
    }

    fn fade_out(&mut self, now: i32) {
        match self {
            Self::Obituary { end_ms, .. } | Self::Notify { end_ms, .. } => {
                *end_ms = (*end_ms).min(now.saturating_add(GAME_MSG_WIN0_FADE_OUT_TIME_MS));
            }
        }
    }
}

struct KillIconPick {
    stem: String,

    namespace: asset_core::AssetNamespace,
    ratio: i32,
    flip: bool,
}

#[derive(Resource, Default)]
pub(crate) struct KillfeedWindow {
    lines: VecDeque<KillfeedLine>,
    bold: VecDeque<(i32, String)>,
    seen: HashSet<u32>,
}

impl KillfeedWindow {
    fn push(&mut self, line: KillfeedLine, now: i32) {
        self.lines.retain(|line| now < line.end_ms());
        self.lines.push_back(line);
        while self.lines.len() > GAME_MSG_WIN0_LINE_COUNT + 3 {
            self.lines.pop_front();
        }
        if self.lines.len() > GAME_MSG_WIN0_LINE_COUNT {
            let fading = self.lines.len() - GAME_MSG_WIN0_LINE_COUNT - 1;
            self.lines[fading].fade_out(now);
        }
    }
}

const BOLD_LINE_COUNT: usize = 3;
const BOLD_MSG_TIME_MS: i32 = 3000;
const BOLD_TEXT_SCALE: f32 = 0.5;
const BOLD_Y: f32 = 100.0;

#[derive(Component)]
pub(crate) struct KillfeedRaster;

pub(crate) fn spawn_killfeed(root: &mut ChildSpawnerCommands) {
    crate::font_overlay::spawn_overlay(root, KillfeedRaster);
}

fn hide(pass: &mut HudTessPass) {
    pass.killfeed = TessJob::Hide;
}

fn pick_kill_icon(
    payload: &sim::EntityEventPayload,
    weapons: Option<&assets::BoundWeapons<'_>>,
) -> KillIconPick {
    let chrome = crate::images::HUD_CHROME_NAMESPACE;

    if let Some(mod_) = obituary_mod(payload.event_parm) {
        let Some(stem) = obituary_mod_killicon(mod_) else {
            return KillIconPick {
                stem: String::new(),
                namespace: chrome,
                ratio: 0,
                flip: false,
            };
        };
        return KillIconPick {
            stem: stem.to_owned(),
            namespace: chrome,
            ratio: 0,
            flip: false,
        };
    }
    let weapon = payload.event_parm as u32;
    let (ratio, flip) =
        match weapons.and_then(|reg| reg.row(weapon).and_then(|weapon| weapon.hud_facts())) {
            Some(facts) => (facts.kill_icon_ratio, facts.flip_kill_icon),
            None => (0, false),
        };
    let (stem, namespace) = if let Some(reg) = weapons {
        let ns = reg
            .registry()
            .component_namespace_of(weapon, asset_game::WeaponComponent::Material)
            .unwrap_or(chrome);
        if let Some(image) = reg.registry().kill_icon_image_of(weapon) {
            (image.to_owned(), ns)
        } else if let Some(name) = reg.registry().kill_icon_of(weapon) {
            (name.to_owned(), ns)
        } else {
            (KILLICON_DIED.to_owned(), chrome)
        }
    } else {
        (KILLICON_DIED.to_owned(), chrome)
    };
    KillIconPick {
        stem,
        namespace,
        ratio,
        flip,
    }
}

fn snapshot_client_team(presented: &net::PresentedSnapshot, client: i32) -> i32 {
    if client < 0 {
        return 0;
    }
    presented
        .snapshot()
        .and_then(|snap| snap.meta.for_client(sim::ClientId(client as u32)))
        .map_or(0, |meta| meta.client_state_team)
}

fn obituary_name_color(local_team: i32, team: i32) -> [f32; 4] {
    if !matches!(local_team, 1 | 2) || !matches!(team, 1 | 2) {
        return [1.0, 1.0, 1.0, 1.0];
    }
    let rgb = if team == local_team {
        gamemode_iw4::TEAM_COLOR_MY_TEAM
    } else {
        gamemode_iw4::TEAM_COLOR_ENEMY_TEAM
    };
    let mut color = [1.0; 4];
    for (slot, value) in color.iter_mut().zip(rgb.split_whitespace()) {
        *slot = value.parse().unwrap_or(1.0);
    }
    color
}

fn snapshot_client_name(presented: &net::PresentedSnapshot, client: i32) -> String {
    if client < 0 {
        return String::new();
    }
    let Some(snap) = presented.snapshot() else {
        return String::new();
    };
    let Some(meta) = snap.meta.for_client(sim::ClientId(client as u32)) else {
        return String::new();
    };
    match entity_iw4::client_state_name(&meta.name) {
        Some(s) => s.to_owned(),
        None => String::new(),
    }
}

pub(crate) fn obituary(
    obituary: On<net::EntityObituary>,
    mut window: ResMut<KillfeedWindow>,
    weapons: Option<Res<PreparedWeapons>>,
    presented: Res<net::PresentedSnapshot>,
    clock: Res<FrameClock>,
) {
    if obituary.in_killcam {
        return;
    }
    let payload = obituary.event.payload;
    let bound = weapons
        .as_deref()
        .and_then(|weapons| weapons.for_event(obituary.event.world).ok());
    let pick = pick_kill_icon(&payload, bound.as_ref());
    let attacker = snapshot_client_name(&presented, payload.attacker_entity_num);
    let victim = snapshot_client_name(&presented, payload.other_entity_num);
    let attacker_team = snapshot_client_team(&presented, payload.attacker_entity_num);
    let victim_team = snapshot_client_team(&presented, payload.other_entity_num);
    let now = clock.time();
    window.push(
        KillfeedLine::Obituary {
            start_ms: now,
            end_ms: now.saturating_add(GAME_MSG_WIN0_MSG_TIME_MS),
            icon: pick.stem,
            icon_namespace: pick.namespace,
            attacker,
            has_attacker: (0..18).contains(&payload.attacker_entity_num)
                && payload.attacker_entity_num != payload.other_entity_num,
            victim,
            attacker_team,
            victim_team,
            kill_icon_ratio: pick.ratio,
            flip_kill_icon: pick.flip,
        },
        now,
    );
}

#[allow(clippy::too_many_arguments)]
fn text_cmd(
    font_name: &str,
    text_style: i32,
    x: f32,
    y: f32,
    cmd_w: f32,
    cmd_h: f32,
    material: String,
    text: String,
    color: [f32; 4],
    site: &'static str,
) -> Draw2dCmd {
    Draw2dCmd {
        material_namespace: crate::images::HUD_CHROME_NAMESPACE,
        x: (x + 0.5).floor(),
        y: (y + 0.5).floor(),
        w: cmd_w,
        h: cmd_h,
        s0: 0.0,
        t0: 0.0,
        s1: 1.0,
        t1: 1.0,
        color,
        material,
        op: Draw2dOp::TextRun {
            font: font_name.to_owned(),
            scale: cmd_w,
            text,
            loc_key: String::new(),

            style: text_style,
            fx: None,
            glow: None,
        },
        provenance: Draw2dProvenance::CgDraw { site },
        layer: 1,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_killfeed(
    surface: Res<crate::surface::Hud2dSurface>,
    presented: Res<net::PresentedSnapshot>,
    clock: Res<FrameClock>,
    local: Res<LocalPresentClient>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut window: ResMut<KillfeedWindow>,
    mut pass: ResMut<HudTessPass>,
    mut notifies: MessageReader<net::SvcGameNotify>,
    view: Option<Res<frame::ViewSubject>>,
) {
    if !surface.is_ready() {
        hide(&mut pass);
        return;
    }
    if presented.player(local.0).is_none() {
        window.lines.clear();
        window.bold.clear();
        window.seen.clear();
        gaps.clear(HudGap::Obituary);
        hide(&mut pass);
        return;
    }

    let now = clock.time();
    for cmd in notifies.read() {
        if !window.seen.insert(cmd.id) {
            continue;
        }
        let Some(template) = strings
            .as_ref()
            .and_then(|s| crate::hudelem::resolve_hud_text(s, &cmd.key))
        else {
            gaps.raise(GapCause::LocalizedRowMissing {
                key: cmd.key.clone(),
            });
            continue;
        };
        let args: Vec<String> = cmd
            .name
            .split(sim::HUD_PRINT_ARG_SEPARATOR)
            .map(|arg| {
                strings
                    .as_ref()
                    .and_then(|s| crate::hudelem::resolve_hud_text(s, arg))
                    .unwrap_or_else(|| arg.to_owned())
            })
            .collect();
        let text = if template.contains("&&2") {
            args.iter().enumerate().fold(template, |line, (i, arg)| {
                line.replace(&format!("&&{}", i + 1), arg)
            })
        } else {
            gamenotify_line(&template, &args[0])
        };
        if cmd.tag == net::SVC_PRINT_BOLD {
            window.bold.push_back((now, text));
            while window.bold.len() > BOLD_LINE_COUNT {
                window.bold.pop_front();
            }
            continue;
        }
        window.push(
            KillfeedLine::Notify {
                start_ms: now,
                end_ms: now.saturating_add(GAME_MSG_WIN0_MSG_TIME_MS),
                text,
                name_empty: cmd.tag == net::SVC_DISCONNECT_NOTIFY && cmd.name.is_empty(),
            },
            now,
        );
    }
    window.lines.retain(|line| now < line.end_ms());
    window
        .bold
        .retain(|(start, _)| now.saturating_sub(*start) < BOLD_MSG_TIME_MS);
    if window.lines.is_empty() && window.bold.is_empty() {
        gaps.clear(HudGap::Obituary);
        hide(&mut pass);
        return;
    }
    if view.is_some_and(|v| v.in_killcam()) {
        hide(&mut pass);
        return;
    }

    let names_ok = match window.lines.back() {
        None => true,
        Some(KillfeedLine::Obituary {
            has_attacker,
            attacker,
            victim,
            ..
        }) => (!*has_attacker || !attacker.is_empty()) && !victim.is_empty(),
        Some(KillfeedLine::Notify { text, .. }) => !text.is_empty(),
    };
    match window.lines.back() {
        None => {}
        Some(KillfeedLine::Obituary { .. }) => {
            if !names_ok {
                gaps.raise(GapCause::ObituaryNoClientInfo);
            } else {
                gaps.clear(HudGap::Obituary);
            }
        }
        Some(KillfeedLine::Notify { name_empty, .. }) => {
            if *name_empty {
                gaps.raise(GapCause::GameNotifyNoClientInfo);
            } else if names_ok {
                gaps.clear(HudGap::Obituary);
            }
        }
    }

    let local_team = snapshot_client_team(&presented, local.0.0 as i32);
    let Some(item) = catalog
        .as_ref()
        .and_then(|c| c.get("hud_fullscreen"))
        .and_then(|menu| {
            menu.items.iter().find(|item| {
                item.item_type == hud_iw4::ITEM_TYPE_GAME_MESSAGE_WINDOW
                    && item.game_msg_window_index == 0
            })
        })
    else {
        hide(&mut pass);
        return;
    };
    let font_name = hud_iw4::ui_get_font_handle(
        item.font_enum,
        surface.scale_virtual_to_real()[1],
        item.text_scale,
    );
    let font = catalog.as_ref().and_then(|c| c.font(font_name));
    let (nscale, font_material) = match font {
        Some(def) => (
            normalized_text_scale(def.pixel_height, item.text_scale),
            asset_core::AssetRef::bare_name(&def.material).to_owned(),
        ),
        None => (0.0, String::new()),
    };
    if names_ok && font.is_none() {
        gaps.raise(GapCause::ObituaryNoClientInfo);
    }

    let miss = hud_images.miss_reason();
    let mut cmds = Vec::new();
    let mut fonts = HashMap::new();
    let font_tex_ok = if let Some(def) = font {
        fonts.insert(font_name.to_owned(), def);
        hud_images
            .get(
                crate::images::HUD_CHROME_NAMESPACE,
                &font_material,
                &mut images,
            )
            .is_some()
    } else {
        false
    };
    if names_ok && font.is_some() && !font_tex_ok {
        let image = hud_images
            .zone_image_name(&font_material)
            .map(str::to_owned);
        gaps.raise(GapCause::FontAtlasMissing {
            material: font_material.clone(),
            image,
        });
    }

    let line_height = (item.text_scale * hud_iw4::GAME_MSG_CHAR_EM + 0.5).floor();
    let icon_spacing = font.map_or(0.0, |def| text_width(def, " ") as f32 * nscale);
    let scroll_offset: f32 = window
        .lines
        .iter()
        .map(|line| {
            let age = now.saturating_sub(line.start_ms());
            let fraction = (1.0 - age as f32 / GAME_MSG_WIN0_SCROLL_TIME_MS as f32).clamp(0.0, 1.0);
            (line_height * fraction + 0.5).floor()
        })
        .sum();
    for (i, line) in window.lines.iter().rev().enumerate() {
        let first_cmd = cmds.len();
        let age = now.saturating_sub(line.start_ms());

        let alpha = (age as f32 / GAME_MSG_WIN0_FADE_IN_TIME_MS as f32).clamp(0.0, 1.0)
            * ((line.end_ms() - now) as f32 / GAME_MSG_WIN0_FADE_OUT_TIME_MS as f32)
                .clamp(0.0, 1.0)
            * item.fore_color[3];
        let y_virtual = item.rect.y + scroll_offset - line_height * (i as f32 + 1.0);
        match line {
            KillfeedLine::Notify { text, .. } => {
                if font.is_some() && font_tex_ok {
                    let applied = surface.apply_rect(
                        item.rect.x,
                        y_virtual,
                        nscale,
                        nscale,
                        i32::from(item.rect.horz_align),
                        i32::from(item.rect.vert_align),
                    );
                    cmds.push(text_cmd(
                        font_name,
                        item.text_style,
                        applied.x,
                        applied.y,
                        applied.w,
                        applied.h,
                        font_material.clone(),
                        text.clone(),
                        [
                            item.fore_color[0],
                            item.fore_color[1],
                            item.fore_color[2],
                            1.0,
                        ],
                        "killfeed_game_msg",
                    ));
                }
            }
            KillfeedLine::Obituary {
                icon,
                icon_namespace,
                attacker,
                has_attacker,
                victim,
                attacker_team,
                victim_team,
                kill_icon_ratio,
                flip_kill_icon,
                ..
            } => {
                let mut x_virtual = item.rect.x;
                let line_names = (!*has_attacker || !attacker.is_empty())
                    && !victim.is_empty()
                    && font.is_some()
                    && font_tex_ok;
                if line_names && *has_attacker {
                    if let Some(def) = font {
                        let attacker_w = text_width(def, attacker) as f32 * nscale;
                        let applied = surface.apply_rect(
                            x_virtual,
                            y_virtual,
                            nscale,
                            nscale,
                            i32::from(item.rect.horz_align),
                            i32::from(item.rect.vert_align),
                        );
                        cmds.push(text_cmd(
                            font_name,
                            item.text_style,
                            applied.x,
                            applied.y,
                            applied.w,
                            applied.h,
                            font_material.clone(),
                            attacker.clone(),
                            obituary_name_color(local_team, *attacker_team),
                            "killfeed_obituary",
                        ));
                        x_virtual += attacker_w + icon_spacing;
                    }
                }

                let (icon_vw, icon_vh) = killicon_virtual_size(*kill_icon_ratio);
                let icon_scale = item.text_scale / hud_iw4::GAME_MSG_WIN0_TEXT_SCALE;
                let (icon_vw, icon_vh) = (icon_vw * icon_scale, icon_vh * icon_scale);
                let (s0, s1) = killicon_stretch_uv(*flip_kill_icon);
                let placed = surface.apply_rect(
                    x_virtual,
                    y_virtual - icon_vh,
                    icon_vw,
                    icon_vh,
                    i32::from(item.rect.horz_align),
                    i32::from(item.rect.vert_align),
                );
                let icon_ok = hud_images.get(*icon_namespace, icon, &mut images).is_some();
                if !icon_ok {
                    gaps.raise(GapCause::ObituaryKillIconMissing {
                        name: icon.clone(),
                        miss,
                    });
                } else {
                    cmds.push(Draw2dCmd {
                        material_namespace: *icon_namespace,
                        x: (placed.x + 0.5).floor(),
                        y: (placed.y + 0.5).floor(),
                        w: placed.w,
                        h: placed.h,
                        s0,
                        t0: 0.0,
                        s1,
                        t1: 1.0,
                        color: [1.0, 1.0, 1.0, 1.0],
                        material: icon.clone(),
                        op: Draw2dOp::StretchPic,
                        provenance: Draw2dProvenance::CgDraw {
                            site: "killfeed_obituary",
                        },
                        layer: 1,
                    });
                }
                if line_names {
                    x_virtual += icon_vw + icon_spacing;
                    let applied = surface.apply_rect(
                        x_virtual,
                        y_virtual,
                        nscale,
                        nscale,
                        i32::from(item.rect.horz_align),
                        i32::from(item.rect.vert_align),
                    );
                    cmds.push(text_cmd(
                        font_name,
                        item.text_style,
                        applied.x,
                        applied.y,
                        applied.w,
                        applied.h,
                        font_material.clone(),
                        victim.clone(),
                        obituary_name_color(local_team, *victim_team),
                        "killfeed_obituary",
                    ));
                }
            }
        }
        for cmd in &mut cmds[first_cmd..] {
            cmd.color[3] *= alpha;
        }
    }
    if let Some(def) = catalog
        .as_ref()
        .and_then(|c| c.font(HUD_SMALL_FONT))
        .filter(|_| !window.bold.is_empty())
    {
        let font_material = asset_core::AssetRef::bare_name(&def.material).to_owned();
        fonts.insert(HUD_SMALL_FONT.to_owned(), def);
        let bold_font_tex_ok = hud_images
            .get(
                crate::images::HUD_CHROME_NAMESPACE,
                &font_material,
                &mut images,
            )
            .is_some();
        let scale = normalized_text_scale(def.pixel_height, BOLD_TEXT_SCALE);
        for (i, (start, text)) in window.bold.iter().enumerate().filter(|_| bold_font_tex_ok) {
            let age = now.saturating_sub(*start);
            let alpha = ((BOLD_MSG_TIME_MS - age) as f32 / 500.0).clamp(0.0, 1.0);
            let width = text_width(def, text) as f32 * scale;
            let line_h = def.pixel_height as f32 * scale;
            let applied = surface.apply_rect(
                -width / 2.0,
                BOLD_Y + line_h * i as f32,
                scale,
                scale,
                hud_iw4::ALIGN_CENTER,
                hud_iw4::ALIGN_VIEWABLE,
            );
            cmds.push(text_cmd(
                HUD_SMALL_FONT,
                hud_iw4::GAME_MSG_WIN0_TEXT_STYLE,
                applied.x,
                applied.y,
                applied.w,
                applied.h,
                font_material.clone(),
                text.clone(),
                [1.0, 1.0, 1.0, alpha],
                "killfeed_bold_msg",
            ));
        }
    }

    let list = Draw2dList { cmds };
    let (quads, _) = tessellate_fonts(&list, &fonts);
    if quads.is_empty() {
        hide(&mut pass);
        return;
    }
    pass.killfeed = TessJob::Quads(quads);
}
