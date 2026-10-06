use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
use crate::gaps::{GapCause, HudPresentationGaps};
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImages;
use asset_game::MenuCatalog;
use assets::PreparedWeapons;
use bevy::prelude::*;
use net::{LocalPresentClient, PresentedSnapshot};
use std::collections::HashMap;

#[derive(Component)]
pub(crate) struct UseHintRaster;

#[derive(Default)]
pub(crate) struct HintMemory {
    caption: Option<(
        String,
        String,
        Option<(asset_core::AssetNamespace, String, i32, bool)>,
    )>,
    last_seen: f64,
}

pub(crate) fn update(
    surface: Res<crate::surface::Hud2dSurface>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<assets::PreparedLocalizedStrings>>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    input: Res<frame::HudInputView>,
    mut pass: ResMut<HudTessPass>,
    weapons: Option<Res<PreparedWeapons>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    time: Res<Time>,
    mut memory: Local<HintMemory>,
    view: Option<Res<frame::ViewSubject>>,
) {
    pass.use_hint = TessJob::Hide;
    if !surface.is_ready()
        || input.script_menu_open
        || view.as_ref().is_some_and(|v| v.in_killcam())
    {
        memory.caption = None;
        return;
    }
    let Some(snapshot) = presented.snapshot() else {
        memory.caption = None;
        return;
    };
    if snapshot.meta.phase != sim::MatchPhase::Playing {
        memory.caption = None;
        return;
    }
    let Some(ps) = presented.alive_player(local.0) else {
        memory.caption = None;
        return;
    };
    let result = (|| {
        if ps.cursor_hint <= 0 {
            return Ok(None);
        }
        let bind = input
            .use_key
            .as_deref()
            .or_else(|| strings.as_ref()?.0.text(hud_iw4::KEY_UNBOUND))
            .ok_or_else(|| "KEY_UNBOUND localization missing".to_owned())?;
        if ps.cursor_hint <= 4 {
            if ps.cursor_hint_string < 0 {
                return Ok(None);
            }
            let strings = strings
                .as_ref()
                .ok_or_else(|| "localized strings missing".to_owned())?;
            let raw =
                sim::hud_string_in_occupied(&snapshot.meta.hud_strings, ps.cursor_hint_string)
                    .ok_or_else(|| {
                        format!("hint string {} not published", ps.cursor_hint_string)
                    })?;
            let localized = crate::hudelem::resolve_hud_text(strings, raw)
                .ok_or_else(|| format!("missing {raw}"))?;
            let unbound = strings
                .0
                .text(hud_iw4::KEY_UNBOUND)
                .ok_or_else(|| "KEY_UNBOUND localization missing".to_owned())?;
            let hint = hud_iw4::replace_directive(&localized, |cmd| {
                if matches!(cmd, "+activate" | "+usereload") {
                    bind.to_owned()
                } else {
                    hud_iw4::unbound_directive(unbound, cmd)
                }
            })
            .replace("&&1", bind);
            return Ok(Some((hint, raw.to_owned(), None)));
        }
        let weapon = (ps.cursor_hint - 4) as u32;
        let weapons = weapons
            .as_ref()
            .ok_or_else(|| "weapon catalog missing".to_owned())?;
        let bound = weapons
            .snapshot_weapon(presented.weapon_epoch(), weapon)
            .map_err(|refusal| format!("weapon binding refused: {refusal:?}"))?;
        let strings = strings
            .as_ref()
            .ok_or_else(|| "localized strings missing".to_owned())?;
        let primary_count = ps
            .weapons
            .iter()
            .filter(|&&w| {
                w > 0
                    && weapons
                        .snapshot_weapon(presented.weapon_epoch(), w as u32)
                        .ok()
                        .and_then(|weapon| weapon.hud_facts())
                        .is_some_and(|f| f.is_primary())
            })
            .count();
        let offhand = bound.hud_facts().is_some_and(|f| f.offhand_class != 0);
        let key = if offhand || primary_count < 2 {
            "PLATFORM_PICKUPNEWWEAPON"
        } else {
            "PLATFORM_SWAPWEAPONS"
        };
        let template = strings
            .0
            .text(key)
            .ok_or_else(|| format!("missing {key}"))?;
        let name_key = weapons
            .registry()
            .display_name_key_of(weapon)
            .ok_or_else(|| format!("weapon {weapon}: display name missing"))?;
        let name = weapons
            .registry()
            .identity_namespace_of(weapon)
            .map_or_else(
                || strings.0.text(name_key),
                |ns| strings.0.text_in(ns, name_key),
            )
            .ok_or_else(|| format!("missing {name_key}"))?;
        let text = format!("{} {}", template.replace("&&1", bind), name);
        let (image, ratio) = weapons
            .registry()
            .pickup_icon_of(weapon)
            .ok_or_else(|| format!("weapon {weapon}: pickup/hud icon unresolved"))?;
        let namespace = weapons
            .registry()
            .hud_icon_namespace_of(weapon)
            .ok_or_else(|| format!("weapon {weapon}: namespace missing"))?;
        Ok::<_, String>(Some((
            text,
            key.to_owned(),
            Some((
                namespace,
                image.to_owned(),
                ratio,
                ps.cursor_hint_dual_wield != 0,
            )),
        )))
    })();
    match result {
        Ok(Some(caption)) => {
            memory.caption = Some(caption);
            memory.last_seen = time.elapsed_secs_f64();
        }
        Ok(None) => {}
        Err(reason) => {
            gaps.raise(GapCause::CursorHintMissing { reason });
            memory.caption = None;
            return;
        }
    }

    let alpha = (1.0 - (time.elapsed_secs_f64() - memory.last_seen) / 0.1).clamp(0.0, 1.0) as f32;
    let Some((text, key, icon)) = memory.caption.as_ref().filter(|_| alpha > 0.0) else {
        return;
    };
    let Some(catalog) = catalog else {
        gaps.raise(GapCause::NoFontCatalog);
        return;
    };
    let Some(item) = catalog
        .get("hud_fullscreen")
        .and_then(|m| m.items.iter().find(|i| i.owner_draw == 72))
    else {
        gaps.raise(GapCause::CursorHintMissing {
            reason: "hud_fullscreen ownerDraw 72 missing".into(),
        });
        return;
    };
    let font_name = hud_iw4::ui_get_font_handle(
        item.font_enum,
        surface.scale_virtual_to_real()[1],
        item.text_scale,
    );
    let Some(font) = catalog.font(font_name) else {
        gaps.raise(GapCause::FontMissing {
            name: font_name.to_owned(),
        });
        return;
    };
    let nscale = hud_iw4::normalized_text_scale(font.pixel_height, item.text_scale);

    let width = crate::chrome::ui_text_width(font, text, item.text_scale)
        - if icon.is_some() {
            crate::chrome::ui_text_width(font, " ", item.text_scale)
        } else {
            0.0
        };
    let height = hud_iw4::ui_text_height(item.text_scale);
    let horz = i32::from(item.rect.horz_align);
    let vert = i32::from(item.rect.vert_align);
    let rect = surface.apply_rect(
        -width * 0.5,
        item.rect.y + height * 0.5,
        nscale,
        nscale,
        horz,
        vert,
    );
    let mut color = item.fore_color;
    color[3] *= alpha;
    let mut cmds = vec![Draw2dCmd {
        material_namespace: crate::images::HUD_CHROME_NAMESPACE,
        x: rect.x,
        y: rect.y,
        w: rect.w,
        h: rect.h,
        s0: 0.0,
        t0: 0.0,
        s1: 1.0,
        t1: 1.0,
        color,
        material: asset_core::AssetRef::bare_name(&font.material).to_owned(),
        op: Draw2dOp::TextRun {
            font: font_name.into(),
            scale: nscale,
            text: text.clone(),
            loc_key: key.clone(),
            style: item.text_style,
            fx: None,
            glow: None,
        },
        provenance: Draw2dProvenance::CgDraw { site: "use_hint" },
        layer: 1,
    }];
    if let Some((namespace, image, ratio, dual)) = icon {
        if hud_images.get(*namespace, image, &mut images).is_none() {
            gaps.raise(GapCause::CursorHintMissing {
                reason: format!("image {image}: {:?}", hud_images.miss_reason()),
            });
            return;
        }
        let (sx, sy) = match ratio {
            0 => (1.0, 1.0),
            1 => (2.0, 1.0),
            _ => (2.0, 0.5),
        };
        let (w, h) = (item.rect.w * sx, item.rect.h * sy);
        let y = item.rect.y - h * 0.5 + height * (if *dual { sx } else { 1.0 }) * 1.5;
        let placement = if *dual {
            let offset = (h - w) * 0.5;
            vec![(-h + offset, w), (h - offset, -w)]
        } else {
            vec![(-w * 0.5, w)]
        };
        for (x, w) in placement {
            let rect = surface.apply_rect(x, y, w, h, horz, vert);
            cmds.push(Draw2dCmd {
                material_namespace: *namespace,
                x: rect.x,
                y: rect.y,
                w: rect.w,
                h: rect.h,
                s0: 0.0,
                t0: 0.0,
                s1: 1.0,
                t1: 1.0,
                color,
                material: image.clone(),
                op: Draw2dOp::StretchPic,
                provenance: Draw2dProvenance::CgDraw { site: "use_hint" },
                layer: 1,
            });
        }
    }
    let mut fonts = HashMap::new();
    fonts.insert(font_name.to_owned(), font);
    let (mut quads, _) = tessellate_fonts(&Draw2dList { cmds }, &fonts);
    if icon.as_ref().is_some_and(|(_, _, _, dual)| *dual) {
        let start = quads.len().saturating_sub(2);
        for (q, sign) in quads[start..].iter_mut().zip([1.0, -1.0]) {
            let center = [
                (q.xy[0][0] + q.xy[2][0]) * 0.5,
                (q.xy[0][1] + q.xy[2][1]) * 0.5,
            ];
            for point in &mut q.xy {
                let x = point[0] - center[0];
                let y = point[1] - center[1];
                *point = [center[0] - sign * y, center[1] + sign * x];
            }
        }
    }
    if let TessJob::Quads(existing) = &mut pass.use_hint {
        existing.extend(quads);
    } else {
        pass.use_hint = TessJob::Quads(quads);
    }
}
