use asset_game::{FontDef, LocalizeCatalog, MenuCatalog, MenuDef, MenuItem, MenuRect};
use hud_iw4::{
    ExprError, ExprHost, item_text_origin, item_text_paint_scale, next_letter,
    normalized_text_scale, ui_get_font_handle, ui_text_height, window_paint_scale_rect,
};

use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance};
use crate::expr_cache::MenuExprCache;

const FLOAT_RECT_X: u32 = 0;

const FLOAT_RECT_Y: u32 = 1;

const FLOAT_RECT_W: u32 = 2;

const FLOAT_RECT_H: u32 = 3;

const FLOAT_FORECOLOR: u32 = 4;

const FLOAT_GLOWCOLOR: u32 = 9;

const FLOAT_BACKCOLOR: u32 = 14;

const FLOAT_COLOR_SLOTS: u32 = 5;

const FLOAT_TARGET_COUNT: u32 = FLOAT_BACKCOLOR + FLOAT_COLOR_SLOTS;

fn assign_color_slot(color: &mut [f32; 4], slot: u32, value: f32) {
    match slot {
        0..=2 => color[slot as usize] = value,
        3 => color[..3].fill(value),
        _ => color[3] = value,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChromeGapKind {
    VisExp,
    FloatExp,

    FloatExpTarget,
    MaterialExp,
    OwnerDraw,
    AssetFont,
    Localize,
    TextExp,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct ChromeAssets<'a> {
    pub catalog: Option<&'a MenuCatalog>,
    pub localize: Option<&'a LocalizeCatalog>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ChromeCoverage {
    pub(crate) items_total: usize,
    pub(crate) painted: usize,
    pub(crate) vis_false: usize,
    pub(crate) typed_gap: usize,
    pub(crate) gap_ids: Vec<(usize, ChromeGapKind)>,
}

impl ChromeCoverage {
    fn painted(&mut self) {
        self.painted += 1;
    }

    fn vis_false(&mut self) {
        self.vis_false += 1;
    }

    fn gap(&mut self, index: usize, kind: ChromeGapKind) {
        self.typed_gap += 1;
        self.gap_ids.push((index, kind));
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum MenuVisOnError {
    #[default]
    PaintAnyway,
    HideAll,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnerDrawPaint {
    Painted,
    Gap(ChromeGapKind),
}

#[allow(dead_code)]
pub(crate) struct OwnerDrawArgs<'a> {
    pub menu: &'a MenuDef,
    pub index: usize,
    pub item: &'a MenuItem,
    pub rect: MenuRect,
    pub color: [f32; 4],
    pub surface: &'a crate::surface::Hud2dSurface,
    pub assets: ChromeAssets<'a>,
    pub anim: ChromeMenuAnim,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ChromeFrame {
    pub(crate) list: Draw2dList,
    pub(crate) coverage: ChromeCoverage,
    pub(crate) vis_errors: Vec<(usize, String)>,
}

pub(crate) type ChromeMenuAnim = hud_iw4::MenuAnim;

pub(crate) fn execute_chrome_menu(
    menu: &MenuDef,
    host: &impl ExprHost,
    surface: &crate::surface::Hud2dSurface,
    assets: ChromeAssets<'_>,
    exprs: &mut MenuExprCache,
) -> ChromeFrame {
    execute_chrome_menu_ex(
        menu,
        host,
        surface,
        assets,
        ChromeMenuAnim::IDENTITY,
        exprs,
        MenuVisOnError::PaintAnyway,
        None,
    )
}

pub(crate) fn execute_chrome_menu_with_anim(
    menu: &MenuDef,
    host: &impl ExprHost,
    surface: &crate::surface::Hud2dSurface,
    assets: ChromeAssets<'_>,
    anim: ChromeMenuAnim,
    exprs: &mut MenuExprCache,
) -> ChromeFrame {
    execute_chrome_menu_ex(
        menu,
        host,
        surface,
        assets,
        anim,
        exprs,
        MenuVisOnError::PaintAnyway,
        None,
    )
}

pub(crate) fn execute_chrome_menu_ex(
    menu: &MenuDef,
    host: &impl ExprHost,
    surface: &crate::surface::Hud2dSurface,
    assets: ChromeAssets<'_>,
    anim: ChromeMenuAnim,
    exprs: &mut MenuExprCache,
    vis_on_error: MenuVisOnError,
    mut owner_draw: Option<&mut dyn FnMut(OwnerDrawArgs<'_>, &mut ChromeFrame) -> OwnerDrawPaint>,
) -> ChromeFrame {
    let mut frame = ChromeFrame {
        coverage: ChromeCoverage {
            items_total: menu.items.len(),
            ..ChromeCoverage::default()
        },
        ..ChromeFrame::default()
    };
    if !menu.vis_exp.is_empty() {
        match exprs.is_true(&menu.vis_exp, host) {
            Ok(false) => {
                for _ in 0..menu.items.len() {
                    frame.coverage.vis_false();
                }
                return frame;
            }
            Ok(true) => {}
            Err(err) => {
                frame
                    .vis_errors
                    .push((usize::MAX, format!("menu vis {err:?}")));
                if vis_on_error == MenuVisOnError::HideAll {
                    for index in 0..menu.items.len() {
                        frame.coverage.gap(index, ChromeGapKind::VisExp);
                    }
                    return frame;
                }
            }
        }
    }
    let mut parent = match apply_menu_float_rect(&menu.rect, menu, host, exprs) {
        Ok(rect) => rect,
        Err(err) => {
            frame
                .vis_errors
                .push((usize::MAX, format!("menu floatexp {err:?}")));
            for index in 0..menu.items.len() {
                frame.coverage.gap(index, ChromeGapKind::FloatExp);
            }
            return frame;
        }
    };
    parent.x += anim.offset[0];
    parent.y += anim.offset[1];
    for (index, item) in menu.items.iter().enumerate() {
        let hook = owner_draw.as_mut().map(|h| {
            let h: &mut dyn FnMut(OwnerDrawArgs<'_>, &mut ChromeFrame) -> OwnerDrawPaint = &mut **h;
            h
        });
        paint_item(
            menu, index, item, host, surface, assets, anim, exprs, &parent, hook, &mut frame,
        );
    }
    frame
}

pub(crate) fn item_screen_rects(
    menu: &MenuDef,
    host: &impl ExprHost,
    surface: &crate::surface::Hud2dSurface,
    exprs: &mut MenuExprCache,
) -> Vec<(usize, [f32; 4])> {
    if !menu.vis_exp.is_empty() && !matches!(exprs.is_true(&menu.vis_exp, host), Ok(true)) {
        return Vec::new();
    }
    let Ok(parent) = apply_menu_float_rect(&menu.rect, menu, host, exprs) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (index, item) in menu.items.iter().enumerate() {
        if !matches!(exprs.is_true(&item.vis_exp, host), Ok(true)) {
            continue;
        }
        let Ok(style) = evaluate_item_style(
            &parent,
            &menu.rect,
            item,
            host,
            exprs,
            ChromeMenuAnim::IDENTITY,
        ) else {
            continue;
        };
        let r = style.rect;
        let a = surface.apply_rect(r.x, r.y, r.w, r.h, r.horz_align as i32, r.vert_align as i32);
        let (x0, x1) = if a.w < 0.0 {
            (a.x + a.w, a.x)
        } else {
            (a.x, a.x + a.w)
        };
        let (y0, y1) = if a.h < 0.0 {
            (a.y + a.h, a.y)
        } else {
            (a.y, a.y + a.h)
        };
        out.push((index, [x0, y0, x1, y1]));
    }
    out
}

fn paint_item(
    menu: &MenuDef,
    index: usize,
    item: &MenuItem,
    host: &impl ExprHost,
    surface: &crate::surface::Hud2dSurface,
    assets: ChromeAssets<'_>,
    anim: ChromeMenuAnim,
    exprs: &mut MenuExprCache,
    parent: &MenuRect,
    owner_draw: Option<&mut dyn FnMut(OwnerDrawArgs<'_>, &mut ChromeFrame) -> OwnerDrawPaint>,
    frame: &mut ChromeFrame,
) {
    match exprs.is_true(&item.vis_exp, host) {
        Ok(false) => {
            frame.coverage.vis_false();
            return;
        }
        Ok(true) => {}
        Err(err) => {
            frame.vis_errors.push((index, format!("{err:?}")));
            frame.coverage.gap(index, ChromeGapKind::VisExp);
            return;
        }
    }
    let style = match evaluate_item_style(parent, &menu.rect, item, host, exprs, anim) {
        Ok(style) => style,
        Err(err) => {
            frame.vis_errors.push((index, format!("floatexp {err:?}")));
            frame.coverage.gap(index, ChromeGapKind::FloatExp);
            return;
        }
    };
    if let Some(target) = style.unsupported {
        frame
            .vis_errors
            .push((index, format!("floatexp target {target}")));
        frame.coverage.gap(index, ChromeGapKind::FloatExpTarget);
    }
    let rect = style.rect;
    if item.owner_draw != 0 {
        let args = OwnerDrawArgs {
            menu,
            index,
            item,
            rect,
            color: style.fore_color,
            surface,
            assets,
            anim,
        };
        match owner_draw {
            Some(hook) => match hook(args, frame) {
                OwnerDrawPaint::Painted => frame.coverage.painted(),
                OwnerDrawPaint::Gap(kind) => frame.coverage.gap(index, kind),
            },
            None => frame.coverage.gap(index, ChromeGapKind::OwnerDraw),
        }
        return;
    }
    match resolved_material(item, host, exprs) {
        Ok(Some(material)) => {
            push_stretch(
                menu,
                index,
                item,
                &style,
                material,
                surface,
                assets.catalog,
                host.milliseconds(),
                anim,
                &mut frame.list,
            );

            if has_text(item) && item.style != 5 {
                paint_text(
                    menu, index, item, &style, host, surface, assets, anim, exprs, frame,
                );
            } else {
                frame.coverage.painted();
            }
        }
        Ok(None) => {
            if has_text(item) {
                paint_text(
                    menu, index, item, &style, host, surface, assets, anim, exprs, frame,
                );
            } else {
                frame.coverage.painted();
            }
        }
        Err(err) => {
            frame.vis_errors.push((index, format!("material {err:?}")));
            frame.coverage.gap(index, ChromeGapKind::MaterialExp);
        }
    }
}

fn has_text(item: &MenuItem) -> bool {
    !item.text_key.is_empty() || !item.text_exp.is_empty()
}

fn paint_text(
    menu: &MenuDef,
    index: usize,
    item: &MenuItem,
    style: &EvaluatedItemStyle,
    host: &impl ExprHost,
    surface: &crate::surface::Hud2dSurface,
    assets: ChromeAssets<'_>,
    anim: ChromeMenuAnim,
    exprs: &mut MenuExprCache,
    frame: &mut ChromeFrame,
) {
    let mut resolved = match resolve_text(item, host, assets.localize, exprs) {
        Ok(Some(text)) => text,
        Ok(None) => {
            frame.coverage.painted();
            return;
        }
        Err(kind) => {
            frame.coverage.gap(index, kind);
            return;
        }
    };
    let mut at = 0;
    while let Some(start) = resolved.text[at..].find("[{") {
        let start = start + at;
        let Some(end) = resolved.text[start + 2..].find("}]") else {
            break;
        };
        let end = start + 2 + end;
        if let Some(label) = host.binding_label(&resolved.text[start + 2..end]) {
            let replacement = format!("[{label}]");
            resolved.text.replace_range(start..end + 2, &replacement);
            at = start + replacement.len();
        } else {
            at = end + 2;
        }
    }
    if resolved.text.is_empty() {
        frame.coverage.painted();
        return;
    }
    let font_name = ui_get_font_handle(
        item.font_enum,
        surface.scale_virtual_to_real()[1],
        item.text_scale,
    );
    let Some(font) = assets.catalog.and_then(|c| c.font(font_name)) else {
        frame.coverage.gap(index, ChromeGapKind::AssetFont);
        return;
    };

    let draw_text_scale = item_text_paint_scale(item.text_scale, anim.scale);
    let scale = normalized_text_scale(font.pixel_height, draw_text_scale);
    let wrap_width = (style.rect.w.abs() - item.text_align_x.max(0.0)).max(1.0);
    let lines = menu_text_lines(
        &resolved.text,
        (item.static_flags & 0x0080_0000 != 0).then_some(wrap_width),
        |text| ui_text_width(font, text, item.text_scale),
    );
    for (line, text) in lines.into_iter().enumerate() {
        let measured_w = ui_text_width(font, &text, item.text_scale);
        let measured_h = ui_text_height(item.text_scale);
        let rect = &style.rect;
        let (x, y) = item_text_origin(
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            item.text_align_mode,
            item.text_align_x,
            item.text_align_y,
            measured_w,
            measured_h,
        );
        let applied = surface.apply_rect(
            x,
            y + line as f32 * measured_h,
            scale,
            scale,
            rect.horz_align as i32,
            rect.vert_align as i32,
        );
        frame.list.cmds.push(Draw2dCmd {
            material_namespace: crate::images::HUD_CHROME_NAMESPACE,
            x: (applied.x + 0.5).floor(),
            y: (applied.y + 0.5).floor(),
            w: applied.w,
            h: applied.h,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color: style.fore_color,
            material: asset_core::AssetRef::bare_name(&font.material).to_owned(),
            op: Draw2dOp::TextRun {
                font: font_name.to_owned(),
                scale,
                text,
                loc_key: resolved.loc_key.clone(),

                style: item.text_style,
                fx: None,
                glow: text_run_glow(font, style.glow_color),
            },
            provenance: Draw2dProvenance::MenuItem {
                menu: menu.name.clone(),
                index,
            },
            layer: 1,
        });
    }
    frame.coverage.painted();
}

fn menu_text_lines(text: &str, width: Option<f32>, measure: impl Fn(&str) -> f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let Some(width) = width else {
            lines.push(paragraph.to_owned());
            continue;
        };
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if measure(&candidate) <= width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            for ch in word.chars() {
                let mut candidate = line.clone();
                candidate.push(ch);
                if !line.is_empty() && measure(&candidate) > width {
                    lines.push(std::mem::take(&mut line));
                }
                line.push(ch);
            }
        }
        lines.push(line);
    }
    lines
}

pub(crate) fn text_run_glow(font: &FontDef, color: [f32; 4]) -> Option<crate::draw2d::TextRunGlow> {
    if color[3] <= 0.0 {
        return None;
    }
    let material = if font.glow_material.is_empty() {
        &font.material
    } else {
        &font.glow_material
    };
    Some(crate::draw2d::TextRunGlow {
        material: asset_core::AssetRef::bare_name(material).to_owned(),
        color,
    })
}

pub(crate) fn push_owner_text(
    args: &OwnerDrawArgs<'_>,
    text: &str,
    color: [f32; 4],
    frame: &mut ChromeFrame,
) -> Result<(), ChromeGapKind> {
    if text.is_empty() {
        return Ok(());
    }
    let font_name = ui_get_font_handle(
        args.item.font_enum,
        args.surface.scale_virtual_to_real()[1],
        args.item.text_scale,
    );
    let Some(font) = args.assets.catalog.and_then(|c| c.font(font_name)) else {
        return Err(ChromeGapKind::AssetFont);
    };
    let measured_w = ui_text_width(font, text, args.item.text_scale);
    let measured_h = ui_text_height(args.item.text_scale);
    let (x, y) = item_text_origin(
        args.rect.x,
        args.rect.y,
        args.rect.w,
        args.rect.h,
        args.item.text_align_mode,
        args.item.text_align_x,
        args.item.text_align_y,
        measured_w,
        measured_h,
    );
    push_owner_text_run(args, font_name, font, text, color, x, y, frame);
    Ok(())
}

pub(crate) fn push_owner_text_right_of_rect(
    args: &OwnerDrawArgs<'_>,
    text: &str,
    right_inset: f32,
    color: [f32; 4],
    frame: &mut ChromeFrame,
) -> Result<(), ChromeGapKind> {
    if text.is_empty() {
        return Ok(());
    }
    let font_name = ui_get_font_handle(
        args.item.font_enum,
        args.surface.scale_virtual_to_real()[1],
        args.item.text_scale,
    );
    let Some(font) = args.assets.catalog.and_then(|c| c.font(font_name)) else {
        return Err(ChromeGapKind::AssetFont);
    };
    let width = ui_text_width(font, text, args.item.text_scale).trunc();
    let x = args.rect.x + args.rect.w - width - right_inset;
    push_owner_text_run(args, font_name, font, text, color, x, args.rect.y, frame);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn push_owner_text_run(
    args: &OwnerDrawArgs<'_>,
    font_name: &str,
    font: &FontDef,
    text: &str,
    color: [f32; 4],
    x: f32,
    y: f32,
    frame: &mut ChromeFrame,
) {
    let draw_text_scale = item_text_paint_scale(args.item.text_scale, args.anim.scale);
    let scale = normalized_text_scale(font.pixel_height, draw_text_scale);
    let applied = args.surface.apply_rect(
        x,
        y,
        scale,
        scale,
        args.rect.horz_align as i32,
        args.rect.vert_align as i32,
    );
    frame.list.cmds.push(Draw2dCmd {
        material_namespace: crate::images::HUD_CHROME_NAMESPACE,
        x: (applied.x + 0.5).floor(),
        y: (applied.y + 0.5).floor(),
        w: applied.w,
        h: applied.h,
        s0: 0.0,
        t0: 0.0,
        s1: 1.0,
        t1: 1.0,
        color,
        material: asset_core::AssetRef::bare_name(&font.material).to_owned(),
        op: Draw2dOp::TextRun {
            font: font_name.to_owned(),
            scale,
            text: text.to_owned(),
            loc_key: String::new(),

            style: args.item.text_style,
            fx: None,
            glow: None,
        },
        provenance: Draw2dProvenance::OwnerDraw(args.item.owner_draw),
        layer: 1,
    });
}

pub(crate) fn push_owner_pic(
    args: &OwnerDrawArgs<'_>,
    material: String,
    material_namespace: asset_core::AssetNamespace,
    color: [f32; 4],
    op: Draw2dOp,
    frame: &mut ChromeFrame,
) {
    if args.rect.w.abs() <= f32::EPSILON || args.rect.h.abs() <= f32::EPSILON {
        return;
    }
    let (x, y, w, h) = window_paint_scale_rect(
        args.rect.x,
        args.rect.y,
        args.rect.w,
        args.rect.h,
        args.anim.scale,
    );
    if w.abs() <= f32::EPSILON || h.abs() <= f32::EPSILON {
        return;
    }
    let applied = args.surface.apply_rect(
        x,
        y,
        w,
        h,
        args.rect.horz_align as i32,
        args.rect.vert_align as i32,
    );
    frame.list.cmds.push(Draw2dCmd {
        material_namespace,
        x: applied.x,
        y: applied.y,
        w: applied.w,
        h: applied.h,
        s0: 0.0,
        t0: 0.0,
        s1: 1.0,
        t1: 1.0,
        color,
        material,
        op,
        provenance: Draw2dProvenance::OwnerDraw(args.item.owner_draw),
        layer: 1,
    });
}

struct ResolvedText {
    text: String,
    loc_key: String,
}

fn resolve_text(
    item: &MenuItem,
    host: &impl ExprHost,
    loc: Option<&LocalizeCatalog>,
    exprs: &mut MenuExprCache,
) -> Result<Option<ResolvedText>, ChromeGapKind> {
    let raw = if !item.text_key.is_empty() {
        item.text_key.clone()
    } else if !item.text_exp.is_empty() {
        exprs
            .evaluate_string(&item.text_exp, host)
            .map_err(|_| ChromeGapKind::TextExp)?
    } else {
        return Ok(None);
    };
    if raw.is_empty() {
        return Ok(Some(ResolvedText {
            text: String::new(),
            loc_key: String::new(),
        }));
    }
    if let Some(key) = raw
        .strip_prefix('@')
        .filter(|_| item.item_type != 4 && !item.text_literal)
    {
        let Some(table) = loc else {
            return Err(ChromeGapKind::Localize);
        };
        let Some(text) = table.text(key) else {
            return Err(ChromeGapKind::Localize);
        };
        return Ok(Some(ResolvedText {
            text: text.to_owned(),
            loc_key: key.to_owned(),
        }));
    }
    Ok(Some(ResolvedText {
        text: raw,
        loc_key: String::new(),
    }))
}

pub(crate) fn ui_text_width(font: &FontDef, text: &str, text_scale: f32) -> f32 {
    text_width(font, text) as f32 * normalized_text_scale(font.pixel_height, text_scale)
}

pub(crate) fn text_width(font: &FontDef, text: &str) -> i32 {
    let mut width = 0i32;
    let mut max_width = 0i32;
    let mut chars = text.chars().peekable();
    while let Some(letter) = next_letter(&mut chars) {
        if letter == 13 || letter == 10 {
            width = 0;
            continue;
        }
        let Some(glyph) = font.glyph(letter) else {
            continue;
        };
        let dx = i32::from(glyph.dx);
        width += dx;
        if max_width < width {
            max_width = width;
        }
    }
    max_width
}

fn resolved_material(
    item: &MenuItem,
    host: &impl ExprHost,
    exprs: &mut MenuExprCache,
) -> Result<Option<String>, ExprError> {
    if !item.material_exp.is_empty() {
        let name = exprs.evaluate_string(&item.material_exp, host)?;
        if name.is_empty() {
            return Ok(background_stem(&item.background));
        }
        return Ok(Some(name));
    }
    Ok(background_stem(&item.background))
}

fn background_stem(raw: &str) -> Option<String> {
    let raw = raw.trim().trim_start_matches(',').trim();
    if raw.is_empty() {
        None
    } else {
        Some(raw.to_owned())
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct EvaluatedItemStyle {
    pub rect: MenuRect,
    pub fore_color: [f32; 4],
    pub glow_color: [f32; 4],
    pub back_color: [f32; 4],

    pub unsupported: Option<u32>,
}

impl EvaluatedItemStyle {
    pub fn fill_color(&self, item: &MenuItem) -> [f32; 4] {
        if item.style == 1 {
            self.back_color
        } else {
            self.fore_color
        }
    }
}

fn evaluate_item_style(
    parent: &MenuRect,
    captured_parent: &MenuRect,
    item: &MenuItem,
    host: &impl ExprHost,
    exprs: &mut MenuExprCache,
    anim: ChromeMenuAnim,
) -> Result<EvaluatedItemStyle, ExprError> {
    let mut style = EvaluatedItemStyle {
        rect: item.rect,
        fore_color: item.fore_color,
        glow_color: item.glow_color,
        back_color: item.back_color,
        unsupported: None,
    };
    style.rect.x += parent.x - captured_parent.x;
    style.rect.y += parent.y - captured_parent.y;
    for &(key, ref dump) in &item.float_exp {
        if dump.is_empty() {
            continue;
        }
        if key >= FLOAT_TARGET_COUNT {
            style.unsupported.get_or_insert(key);
            continue;
        }
        let value = match exprs.evaluate_float(dump, host) {
            Ok(value) => value,
            Err(err) if key <= FLOAT_RECT_H => return Err(err),
            Err(_) => {
                style.unsupported.get_or_insert(key);
                continue;
            }
        };
        match key {
            FLOAT_RECT_X => style.rect.x = parent.x + value,
            FLOAT_RECT_Y => style.rect.y = parent.y + value,
            FLOAT_RECT_W => style.rect.w = value,
            FLOAT_RECT_H => style.rect.h = value,
            _ => {
                let (color, base) = if key < FLOAT_GLOWCOLOR {
                    (&mut style.fore_color, FLOAT_FORECOLOR)
                } else if key < FLOAT_BACKCOLOR {
                    (&mut style.glow_color, FLOAT_GLOWCOLOR)
                } else {
                    (&mut style.back_color, FLOAT_BACKCOLOR)
                };
                assign_color_slot(color, key - base, value);
            }
        }
    }
    for color in [
        &mut style.fore_color,
        &mut style.glow_color,
        &mut style.back_color,
    ] {
        color[3] *= anim.alpha;
    }
    Ok(style)
}

fn apply_menu_float_rect(
    base: &MenuRect,
    menu: &MenuDef,
    host: &impl ExprHost,
    exprs: &mut MenuExprCache,
) -> Result<MenuRect, String> {
    let mut rect = *base;
    for &(key, ref dump) in &menu.float_exp {
        if dump.is_empty() {
            continue;
        }
        match key {
            FLOAT_RECT_X | FLOAT_RECT_Y | FLOAT_RECT_W | FLOAT_RECT_H => {
                let value = exprs
                    .evaluate_float(dump, host)
                    .map_err(|err| format!("target {key}, expression `{dump}`: {err:?}"))?;
                match key {
                    FLOAT_RECT_X => rect.x = value,
                    FLOAT_RECT_Y => rect.y = value,
                    FLOAT_RECT_W => rect.w = value,
                    _ => rect.h = value,
                }
            }
            _ => return Err(format!("unsupported target {key}, expression `{dump}`")),
        }
    }
    Ok(rect)
}

pub(crate) fn atlas_frame_st(
    catalog: Option<&MenuCatalog>,
    material: &str,
    time_ms: i32,
) -> [f32; 4] {
    let [rows, columns] = catalog
        .and_then(|catalog| {
            catalog
                .material_2d_plans
                .get(&material.to_ascii_lowercase())
        })
        .map(|plan| plan.texture_atlas.map(|n| u32::from(n.max(1))))
        .unwrap_or([1, 1]);
    let frame = (time_ms.max(0) as u32 / 50) % (rows * columns);
    let column = frame % columns;
    let row = frame / columns;
    [
        column as f32 / columns as f32,
        row as f32 / rows as f32,
        (column + 1) as f32 / columns as f32,
        (row + 1) as f32 / rows as f32,
    ]
}

fn push_stretch(
    menu: &MenuDef,
    index: usize,
    item: &MenuItem,
    style: &EvaluatedItemStyle,
    material: String,
    surface: &crate::surface::Hud2dSurface,
    catalog: Option<&MenuCatalog>,
    time_ms: i32,
    anim: ChromeMenuAnim,
    list: &mut Draw2dList,
) {
    let rect = style.rect;
    if rect.w.abs() <= f32::EPSILON || rect.h.abs() <= f32::EPSILON {
        return;
    }
    let (x, y, w, h) =
        window_paint_scale_rect(rect.x, rect.y, rect.w.abs(), rect.h.abs(), anim.scale);
    if w.abs() <= f32::EPSILON || h.abs() <= f32::EPSILON {
        return;
    }
    let applied = surface.apply_rect(x, y, w, h, rect.horz_align as i32, rect.vert_align as i32);
    let color = style.fill_color(item);
    let (material_namespace, material) = match asset_core::AssetKey::parse(&material) {
        Ok(key) if key.kind == asset_core::AssetKind::Material => (key.namespace, key.name),
        _ => (crate::images::HUD_CHROME_NAMESPACE, material),
    };
    let [s0, t0, s1, t1] = atlas_frame_st(catalog, &material, time_ms);
    list.cmds.push(Draw2dCmd {
        material_namespace,
        x: applied.x,
        y: applied.y,
        w: applied.w,
        h: applied.h,
        s0: if rect.w < 0.0 { s1 } else { s0 },
        t0: if rect.h < 0.0 { t1 } else { t0 },
        s1: if rect.w < 0.0 { s0 } else { s1 },
        t1: if rect.h < 0.0 { t0 } else { t1 },
        color,
        material,
        op: Draw2dOp::StretchPic,
        provenance: Draw2dProvenance::MenuItem {
            menu: menu.name.clone(),
            index,
        },
        layer: 1,
    });
}
