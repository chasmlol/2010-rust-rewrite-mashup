use asset_game::MenuCatalog;
use bevy::prelude::*;
use net::{LocalPresentClient, PresentedSnapshot};
use playerstate_iw4::other_flags;

use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate};
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::HudImages;

const EMP_STATIC_VIS: &str = "op 16 op 138 op 1";

#[derive(Component)]
pub(crate) struct EmpStaticRaster;

#[allow(clippy::too_many_arguments)]
pub(crate) fn update(
    surface: Res<crate::surface::Hud2dSurface>,
    catalog: Option<Res<MenuCatalog>>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    view: Option<Res<frame::ViewSubject>>,
    mut pass: ResMut<HudTessPass>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
) {
    pass.emp_static = TessJob::Hide;
    if !surface.is_ready() || view.as_ref().is_some_and(|v| v.in_killcam()) {
        return;
    }
    if !presented
        .player(local.0)
        .is_some_and(|ps| ps.other_flags & other_flags::EMP_JAMMED != 0)
    {
        return;
    }
    let Some(item) = catalog.as_ref().and_then(|catalog| {
        catalog
            .get("hud_fullscreen")?
            .items
            .iter()
            .find(|item| item.vis_exp == EMP_STATIC_VIS && !item.background.is_empty())
    }) else {
        return;
    };
    let rect = surface.apply_rect(
        item.rect.x,
        item.rect.y,
        item.rect.w,
        item.rect.h,
        i32::from(item.rect.horz_align),
        i32::from(item.rect.vert_align),
    );
    if hud_images
        .get(
            crate::images::HUD_CHROME_NAMESPACE,
            &item.background,
            &mut images,
        )
        .is_none()
    {
        return;
    }
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
            material: item.background.clone(),
            op: Draw2dOp::StretchPic,
            provenance: Draw2dProvenance::CgDraw { site: "emp_static" },
            layer: 0,
        }],
    };
    let (quads, _) = tessellate(&list);
    if !quads.is_empty() {
        pass.emp_static = TessJob::Quads(quads);
    }
}
