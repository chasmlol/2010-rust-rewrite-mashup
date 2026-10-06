use std::collections::HashMap;

use asset_game::MenuCatalog;
use assets::{PreparedLocalizedStrings, PreparedWeapons};
use bevy::prelude::*;
use net::{LocalPresentClient, PresentedSnapshot};
use playerstate_iw4::{breath_hold_time_ms, weap_flags};

use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImages;

#[derive(Component)]
pub(crate) struct BreathHintRaster;

#[allow(clippy::too_many_arguments)]
pub(crate) fn update(
    surface: Res<crate::surface::Hud2dSurface>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    weapons: Option<Res<PreparedWeapons>>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    input: Res<frame::HudInputView>,
    view: Option<Res<frame::ViewSubject>>,
    mut pass: ResMut<HudTessPass>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
) {
    pass.breath_hint = TessJob::Hide;
    if !surface.is_ready()
        || input.script_menu_open
        || input.console_open
        || view.as_ref().is_some_and(|v| v.in_killcam())
    {
        return;
    }
    let Some(ps) = presented.alive_player(local.0) else {
        return;
    };
    let weapon = playerstate_iw4::get_viewmodel_weapon_index(ps);
    if ps.f_weapon_pos_frac != 1.0
        || !weapons
            .as_ref()
            .and_then(|w| {
                w.snapshot_weapon(presented.weapon_epoch(), weapon)
                    .ok()
                    .and_then(|weapon| weapon.hud_facts())
            })
            .is_some_and(|f| f.can_hold_breath)
    {
        return;
    }
    let Some(catalog) = catalog.as_ref() else {
        return;
    };
    let Some(item) = catalog
        .get("hud_fullscreen")
        .and_then(|m| m.items.iter().find(|i| i.owner_draw == 113))
    else {
        return;
    };
    let font_name = hud_iw4::ui_get_font_handle(
        item.font_enum,
        surface.scale_virtual_to_real()[1],
        item.text_scale,
    );
    let Some(font) = catalog.font(font_name) else {
        return;
    };
    let binding = ["+holdbreath", "+melee_breath", "+breath_sprint"]
        .iter()
        .find_map(|cmd| input.binding_keys.get(*cmd))
        .map(String::as_str)
        .unwrap_or("UNBOUND");
    let template = strings
        .as_ref()
        .and_then(|s| s.0.text("PLATFORM_HOLD_BREATH"));
    let text = if ps.weap_flags & weap_flags::HOLD_BREATH != 0 {
        format!(
            "Holding breath  {:.1}s",
            (breath_hold_time_ms(ps) - ps.hold_breath_timer).max(0) as f32 * 0.001
        )
    } else if ps.hold_breath_timer > 0 {
        format!(
            "Recovering breath  {:.1}s",
            ps.hold_breath_timer as f32 * 0.001
        )
    } else {
        template
            .map(|t| hud_iw4::hint_replace_bind(t, binding))
            .unwrap_or_else(|| format!("Hold [{binding}] to steady aim"))
    };
    let nscale = hud_iw4::normalized_text_scale(font.pixel_height, item.text_scale);
    let width = crate::chrome::ui_text_width(font, &text, item.text_scale);
    let rect = surface.apply_rect(
        item.rect.x - width * 0.5,
        item.rect.y,
        nscale,
        nscale,
        i32::from(item.rect.horz_align),
        i32::from(item.rect.vert_align),
    );
    let material = asset_core::AssetRef::bare_name(&font.material).to_owned();
    let list = Draw2dList {
        cmds: vec![Draw2dCmd {
            material_namespace: crate::images::HUD_CHROME_NAMESPACE,
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color: item.fore_color,
            material: material.clone(),
            op: Draw2dOp::TextRun {
                font: font_name.to_owned(),
                scale: nscale,
                text,
                loc_key: "PLATFORM_HOLD_BREATH".to_owned(),
                style: item.text_style,
                fx: None,
                glow: None,
            },
            provenance: Draw2dProvenance::CgDraw {
                site: "breath_hint",
            },
            layer: 1,
        }],
    };
    let mut fonts = HashMap::new();
    fonts.insert(font_name.to_owned(), font);
    let (quads, _) = tessellate_fonts(&list, &fonts);
    if !quads.is_empty()
        && hud_images
            .get(crate::images::HUD_CHROME_NAMESPACE, &material, &mut images)
            .is_some()
    {
        pass.breath_hint = TessJob::Quads(quads);
    }
}
