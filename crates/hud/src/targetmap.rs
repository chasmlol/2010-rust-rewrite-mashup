use std::collections::HashMap;

use asset_game::MenuCatalog;
use assets::PreparedLocalizedStrings;
use bevy::prelude::*;
use frame::ViewSubject;
use hud_iw4::{ExprError, ExprHost, Operand};
use net::{LocalPresentClient, LocationCursor, PresentedSnapshot};

use crate::chrome::{
    ChromeAssets, ChromeFrame, ChromeMenuAnim, MenuVisOnError, OwnerDrawArgs, OwnerDrawPaint,
    execute_chrome_menu_ex,
};
use crate::compass::DrawableCompass;
use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
use crate::font_overlay;
use crate::gaps::{GapCause, HudPresentationGaps};
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::{HUD_CHROME_NAMESPACE, HudImages};
use crate::killcam_skip::inherit_shared_vis;
use crate::scorebar::milliseconds;

const MENU: &str = "targetmap_fullscreen";
const OWNER_DRAW_MAP: i32 = 181;
const OWNER_DRAW_PLAYER: i32 = 183;
const OWNER_DRAW_SELECTOR: i32 = 186;
const OWNER_DRAW_UNITS: [i32; 4] = [182, 185, 188, 189];
const MAP_BORDER: f32 = 2.0;
const PLAYER_SIZE: f32 = 20.0;
const DEFAULT_RADIUS_FRACTION: f32 = 0.15;
const ARROW_MATERIAL: &str = "map_location_selector_arrow";
const ARROW_SIZE: f32 = 24.0;

#[derive(Component)]
pub(crate) struct TargetmapRaster;
pub(crate) fn spawn_targetmap(root: &mut ChildSpawnerCommands) {
    font_overlay::spawn_overlay(root, TargetmapRaster);
}

fn hide(pass: &mut HudTessPass) {
    pass.targetmap = TessJob::Hide;
}

struct TargetmapExprHost<'a> {
    menu: &'a asset_game::MenuDef,
    ms: i32,
    directing: i32,
    dvars: sim::ScriptDvars<'a>,
    kind: gamemode_iw4::GameModeKind,
    localize: Option<&'a asset_game::LocalizeCatalog>,
}

impl ExprHost for TargetmapExprHost<'_> {
    fn milliseconds(&self) -> i32 {
        self.ms
    }
    fn static_dvar_int(&self, index: i32) -> Result<i32, ExprError> {
        let name = self
            .menu
            .static_dvar_name(index)
            .ok_or(ExprError::Host("static dvar name"))?;
        self.dvar_int(name)
    }
    fn team_field(&self, _field: &str) -> Result<Operand, ExprError> {
        Err(ExprError::Host("team field"))
    }
    fn player_field(&self, _field: &str) -> Result<Operand, ExprError> {
        Err(ExprError::Host("player field"))
    }
    fn other_team_field(&self, _field: &str) -> Result<Operand, ExprError> {
        Err(ExprError::Host("other team field"))
    }
    fn local_var_string(&self, _name: &str) -> Result<Operand, ExprError> {
        Ok(Operand::Str(String::new()))
    }
    fn time_left(&self) -> Result<i32, ExprError> {
        Ok(0)
    }
    fn score_at_rank(&self, _rank: i32) -> Result<i32, ExprError> {
        Ok(0)
    }
    fn gametype_name(&self) -> Result<Operand, ExprError> {
        crate::scorebar::gametype_display_name(self.kind, self.localize)
    }
    fn weapon_lock(&self) -> Result<hud_iw4::WeaponLockView, ExprError> {
        Err(ExprError::Host("weapon lock"))
    }
    fn dvar_int(&self, name: &str) -> Result<i32, ExprError> {
        self.dvars.int(name).ok_or(ExprError::Host("dvarint"))
    }
    fn selecting_location(&self) -> Result<i32, ExprError> {
        Ok(1)
    }
    fn selecting_direction(&self) -> Result<i32, ExprError> {
        Ok(self.directing)
    }
    fn radar_jam_intensity(&self) -> Result<f32, ExprError> {
        Ok(0.0)
    }
}

fn full_map_rect(rect: &asset_game::MenuRect, world_size: [f32; 2]) -> [f32; 4] {
    let rect_aspect = rect.w / rect.h;
    let map_aspect = world_size[0] / world_size[1];
    let [mut x, mut y, mut w, mut h] = if rect_aspect >= map_aspect {
        let w = map_aspect / rect_aspect * rect.w;
        [rect.x + (rect.w - w) * 0.5, rect.y, w, rect.h]
    } else {
        let h = rect_aspect / map_aspect * rect.h;
        [rect.x, rect.y + (rect.h - h) * 0.5, rect.w, h]
    };
    let border = MAP_BORDER.min(w * 0.25).min(h * 0.25);
    x += border;
    y += border;
    w -= border * 2.0;
    h -= border * 2.0;
    [x, y, w, h]
}

fn world_to_map(compass: &DrawableCompass, map: [f32; 4], world: [f32; 2]) -> [f32; 2] {
    let b = compass.bounds;
    let d = [world[0] - b.upper_left[0], world[1] - b.upper_left[1]];
    let east = b.north[1] * d[0] - b.north[0] * d[1];
    let south = -b.north[1] * d[1] - b.north[0] * d[0];
    [
        map[0] + map[2] * (east / b.world_size[0]),
        map[1] + map[3] * (south / b.world_size[1]),
    ]
}

fn pic(
    args: &OwnerDrawArgs<'_>,
    rect: [f32; 4],
    st: [f32; 4],
    material: &str,
    namespace: asset_core::AssetNamespace,
    color: [f32; 4],
) -> Draw2dCmd {
    let applied = args.surface.apply_rect(
        rect[0],
        rect[1],
        rect[2],
        rect[3],
        args.rect.horz_align as i32,
        args.rect.vert_align as i32,
    );
    Draw2dCmd {
        x: applied.x,
        y: applied.y,
        w: applied.w,
        h: applied.h,
        s0: st[0],
        t0: st[1],
        s1: st[2],
        t1: st[3],
        color,
        material: material.to_owned(),
        material_namespace: namespace,
        op: Draw2dOp::StretchPic,
        provenance: Draw2dProvenance::OwnerDraw(args.item.owner_draw),
        layer: 1,
    }
}

/// Positive `deg` turns the picture clockwise on screen.
fn rotated_pic(
    args: &OwnerDrawArgs<'_>,
    center: [f32; 2],
    size: f32,
    deg: f32,
    material: &str,
    color: [f32; 4],
) -> crate::draw2d::Draw2dQuad {
    let (sin, cos) = deg.to_radians().sin_cos();
    let xy = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]].map(|p: [f32; 2]| {
        let x = p[0] * size;
        let y = p[1] * size;
        let r = args.surface.apply_rect(
            center[0] + x * cos - y * sin,
            center[1] + x * sin + y * cos,
            0.0,
            0.0,
            args.rect.horz_align as i32,
            args.rect.vert_align as i32,
        );
        [r.x, r.y]
    });
    crate::draw2d::Draw2dQuad {
        xy,
        st: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        color,
        material: material.to_owned(),
        material_namespace: HUD_CHROME_NAMESPACE,
        provenance: Draw2dProvenance::OwnerDraw(args.item.owner_draw),
        layer: 1,
        clip: None,
    }
}

fn selector_rect(map: [f32; 4], at: [f32; 2], fraction: f32) -> Option<([f32; 4], [f32; 4])> {
    let side = fraction * map[3];
    if side <= 0.0 {
        return None;
    }
    let center = [map[0] + map[2] * at[0], map[1] + map[3] * at[1]];
    let lo = [center[0] - side * 0.5, center[1] - side * 0.5];
    let x0 = lo[0].max(map[0]);
    let y0 = lo[1].max(map[1]);
    let x1 = (lo[0] + side).min(map[0] + map[2]);
    let y1 = (lo[1] + side).min(map[1] + map[3]);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some((
        [x0, y0, x1 - x0, y1 - y0],
        [
            (x0 - lo[0]) / side,
            (y0 - lo[1]) / side,
            (x1 - lo[0]) / side,
            (y1 - lo[1]) / side,
        ],
    ))
}

struct Scene<'a> {
    compass: &'a DrawableCompass,
    map_namespace: asset_core::AssetNamespace,
    selection: &'a sim::LocationSelection,
    cursor: LocationCursor,
    origin: [f32; 2],
    view_yaw: f32,
}

fn owner_draw(
    scene: &Scene<'_>,
    rotated: &mut Vec<crate::draw2d::Draw2dQuad>,
    args: OwnerDrawArgs<'_>,
    frame: &mut ChromeFrame,
) -> OwnerDrawPaint {
    let owner = args.item.owner_draw;
    if OWNER_DRAW_UNITS.contains(&owner) {
        return OwnerDrawPaint::Painted;
    }
    if args.rect.w <= 0.0 || args.rect.h <= 0.0 {
        return OwnerDrawPaint::Painted;
    }
    let map = full_map_rect(&args.rect, scene.compass.bounds.world_size);
    match owner {
        OWNER_DRAW_MAP => {
            frame.list.cmds.push(pic(
                &args,
                map,
                [0.0, 0.0, 1.0, 1.0],
                &scene.compass.image_name,
                scene.map_namespace,
                args.color,
            ));
        }
        OWNER_DRAW_PLAYER => {
            let at = world_to_map(scene.compass, map, scene.origin);
            let deg = scene.compass.north_yaw - scene.view_yaw;
            rotated.push(rotated_pic(
                &args,
                at,
                PLAYER_SIZE,
                deg,
                &args.item.background,
                args.color,
            ));
        }
        OWNER_DRAW_SELECTOR => {
            let radius = scene.selection.radius;
            let fraction = if radius > 0.0 {
                (radius / scene.compass.bounds.world_size[1]).clamp(0.0, 1.0)
            } else {
                DEFAULT_RADIUS_FRACTION
            };
            if let Some((rect, st)) = selector_rect(map, scene.cursor.at, fraction) {
                frame.list.cmds.push(pic(
                    &args,
                    rect,
                    st,
                    &scene.selection.material,
                    HUD_CHROME_NAMESPACE,
                    args.color,
                ));
            }
            if scene.selection.choose_direction {
                for point in [scene.cursor.at, scene.cursor.aim] {
                    let center = [map[0] + map[2] * point[0], map[1] + map[3] * point[1]];
                    rotated.push(rotated_pic(
                        &args,
                        center,
                        ARROW_SIZE,
                        -scene.cursor.yaw,
                        ARROW_MATERIAL,
                        args.color,
                    ));
                    if scene.cursor.aim == scene.cursor.at {
                        break;
                    }
                }
            }
        }
        _ => return OwnerDrawPaint::Painted,
    }
    OwnerDrawPaint::Painted
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_targetmap(
    surface: Res<crate::surface::Hud2dSurface>,
    catalog: Option<Res<MenuCatalog>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    view: Option<Res<ViewSubject>>,
    local: Res<LocalPresentClient>,
    presented: Res<PresentedSnapshot>,
    cursor: Res<LocationCursor>,
    compass: Option<Res<assets::SessionCompass>>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut pass: ResMut<HudTessPass>,
    mut exprs: ResMut<crate::expr_cache::MenuExprCache>,
) {
    if !surface.is_ready() || view.as_deref().is_some_and(|v| v.in_killcam()) {
        hide(&mut pass);
        return;
    }
    let Some(snapshot) = presented.snapshot() else {
        hide(&mut pass);
        return;
    };
    let Some(selection) = snapshot
        .meta
        .for_client(local.0)
        .and_then(|meta| meta.location_selection.as_ref())
    else {
        hide(&mut pass);
        return;
    };
    let Some(ps) = presented.player(local.0) else {
        hide(&mut pass);
        return;
    };
    let Some(catalog) = catalog.as_ref() else {
        gaps.raise(GapCause::NoFontCatalog);
        hide(&mut pass);
        return;
    };
    let Some(menu) = catalog.get(MENU) else {
        hide(&mut pass);
        return;
    };
    let Some(drawable) = crate::compass::resolve(compass.as_deref(), &mut hud_images, &mut gaps)
    else {
        hide(&mut pass);
        return;
    };
    let host = TargetmapExprHost {
        menu,
        ms: milliseconds() as i32,
        directing: i32::from(cursor.directing),
        dvars: snapshot.meta.script_dvars(local.0),
        kind: snapshot.meta.kind,
        localize: strings.as_ref().map(|s| &s.0),
    };
    let (mut menu, _) = inherit_shared_vis(menu);
    menu.items
        .retain(|item| !item.background.to_ascii_lowercase().contains("stencil"));
    let Some(map_namespace) = hud_images.map_namespace() else {
        return;
    };
    let scene = Scene {
        compass: &drawable,
        map_namespace,
        selection,
        cursor: *cursor,
        origin: [ps.origin[0], ps.origin[1]],
        view_yaw: ps.viewangles[1],
    };
    let mut rotated = Vec::new();
    let frame = {
        let mut hook = |args: OwnerDrawArgs<'_>, frame: &mut ChromeFrame| {
            owner_draw(&scene, &mut rotated, args, frame)
        };
        execute_chrome_menu_ex(
            &menu,
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
        )
    };
    let mut fonts: HashMap<String, &asset_game::FontDef> = HashMap::new();
    for cmd in &frame.list.cmds {
        let _ = hud_images.get(cmd.material_namespace, &cmd.material, &mut images);
        if let Draw2dOp::TextRun { font, .. } = &cmd.op
            && !fonts.contains_key(font)
            && let Some(def) = catalog.font(font)
        {
            fonts.insert(font.clone(), def);
        }
    }
    for quad in &rotated {
        let _ = hud_images.get(quad.material_namespace, &quad.material, &mut images);
    }
    let list = Draw2dList {
        cmds: frame.list.cmds,
    };
    let (mut quads, _) = tessellate_fonts(&list, &fonts);
    quads.extend(rotated);
    if quads.is_empty() {
        hide(&mut pass);
        return;
    }
    pass.targetmap = TessJob::Quads(quads);
}
