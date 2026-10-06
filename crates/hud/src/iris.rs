use assets::PreparedWeapons;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::{Display, FocusPolicy, ZIndex};
use hud_iw4::{WeaponAdsOverlayFacts, draw_ads_overlay_layout, draw_weap_reticle};
use net::{LocalPresentClient, PresentedSnapshot, ViewweaponAim};
use weapon_iw4::get_viewmodel_weapon_index;

use crate::gaps::{GapCause, HudGap, HudPresentationGaps};
use crate::images::HudImages;
use crate::presentation_scale::{PresentationScale, ScaleClass};
use crate::reticle::ReticleAdsLatch;
use crate::ui_write::adopt_display;

#[derive(Resource, Default)]
pub(crate) struct IrisLetterboxFill {
    source: Option<String>,
    handle: Option<Handle<Image>>,
    rgba: Option<[u8; 4]>,
}

#[derive(Component, Clone, Copy)]
pub(crate) struct AdsIrisOverlay(u8);

#[derive(Component, Clone, Copy)]
pub(crate) struct AdsIrisLetterbox(u8);

fn hidden_image_node() -> (Node, ImageNode, FocusPolicy) {
    (
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            overflow: Overflow::visible(),
            ..default()
        },
        ImageNode::default(),
        FocusPolicy::Pass,
    )
}

pub(crate) fn spawn_iris(root: &mut ChildSpawnerCommands) {
    for strip in 0..4_u8 {
        let (node, image, focus) = hidden_image_node();
        root.spawn((
            AdsIrisLetterbox(strip),
            node,
            image,
            focus,
            BackgroundColor(Color::NONE),
            ZIndex(0),
        ));
    }
    for quad in 0..4_u8 {
        let (node, image, focus) = hidden_image_node();
        root.spawn((AdsIrisOverlay(quad), node, image, focus, ZIndex(1)));
    }
}

fn hide_overlay(
    overlay: &mut Query<
        (&mut Node, &mut ImageNode, &AdsIrisOverlay),
        (With<AdsIrisOverlay>, Without<AdsIrisLetterbox>),
    >,
) {
    for (mut node, mut image_node, _) in overlay.iter_mut() {
        adopt_display(&mut node, Display::None);
        image_node.flip_x = false;
        image_node.flip_y = false;
    }
}

fn hide_letterbox(
    letterbox: &mut Query<
        (
            &mut Node,
            &mut ImageNode,
            &mut BackgroundColor,
            &AdsIrisLetterbox,
        ),
        Without<AdsIrisOverlay>,
    >,
) {
    for (mut node, _, mut bg, _) in letterbox.iter_mut() {
        adopt_display(&mut node, Display::None);
        *bg = BackgroundColor(Color::NONE);
    }
}

pub(crate) fn overlay_frame_texel(width: u32, height: u32, rgba: &[u8]) -> Option<[u8; 4]> {
    if width == 0 || height == 0 {
        return None;
    }
    let y = height / 2;
    let mut opaque = None;
    for x in 0..width {
        let i = ((y * width + x) * 4) as usize;
        if i + 3 >= rgba.len() {
            return opaque;
        }
        let p = [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]];
        if p[3] < 250 {
            continue;
        }
        if opaque.is_none() {
            opaque = Some(p);
        }
        if p[0] <= 16 && p[1] <= 16 && p[2] <= 16 {
            return Some(p);
        }
    }
    opaque
}

fn fill_handle(rgba: [u8; 4], images: &mut Assets<Image>) -> Handle<Image> {
    images.add(Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![rgba[0], rgba[1], rgba[2], rgba[3]],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    ))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_iris(
    surface: Res<crate::surface::Hud2dSurface>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    weapons: Option<Res<PreparedWeapons>>,
    ads_latch: Res<ReticleAdsLatch>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut fill: ResMut<IrisLetterboxFill>,
    aim: Res<ViewweaponAim>,
    mut overlay: Query<
        (&mut Node, &mut ImageNode, &AdsIrisOverlay),
        (With<AdsIrisOverlay>, Without<AdsIrisLetterbox>),
    >,
    mut letterbox: Query<
        (
            &mut Node,
            &mut ImageNode,
            &mut BackgroundColor,
            &AdsIrisLetterbox,
        ),
        Without<AdsIrisOverlay>,
    >,
) {
    let Some(ps) = presented.player(local.0) else {
        gaps.clear(HudGap::AdsOverlay);
        hide_overlay(&mut overlay);
        hide_letterbox(&mut letterbox);
        return;
    };

    let viewmodel_index = get_viewmodel_weapon_index(ps);
    let Some(weapons) = weapons.as_ref() else {
        gaps.clear(HudGap::AdsOverlay);
        hide_overlay(&mut overlay);
        hide_letterbox(&mut letterbox);
        return;
    };
    let Some(facts) = weapons
        .snapshot_weapon(presented.weapon_epoch(), viewmodel_index)
        .ok()
        .and_then(|weapon| weapon.hud_facts())
    else {
        gaps.clear(HudGap::AdsOverlay);
        hide_overlay(&mut overlay);
        hide_letterbox(&mut letterbox);
        return;
    };

    let hud_iris = weapons.registry().overlay_is_hud_iris(viewmodel_index);
    let weap = WeaponAdsOverlayFacts {
        ads_zoom_in_frac: facts.ads_zoom_in_frac,
        ads_zoom_out_frac: facts.ads_zoom_out_frac,
        overlay_material: u32::from(hud_iris),
        overlay_reticle: if hud_iris {
            if facts.overlay_reticle != 0 {
                facts.overlay_reticle
            } else {
                1
            }
        } else {
            0
        },
        ads_overlay_width: facts.ads_overlay_width,
        ads_overlay_height: facts.ads_overlay_height,
        ..WeaponAdsOverlayFacts::default()
    };
    if !surface.is_ready() {
        gaps.clear(HudGap::AdsOverlay);
        hide_overlay(&mut overlay);
        hide_letterbox(&mut letterbox);
        return;
    }
    let frame = draw_weap_reticle(
        ps.f_weapon_pos_frac,
        ads_latch.position_to_ads,
        surface.height(),
        ps.other_flags,
        &weap,
    );
    let gate = frame.overlay_alpha.is_some();

    if !gate {
        gaps.clear(HudGap::AdsOverlay);
        hide_overlay(&mut overlay);
        hide_letterbox(&mut letterbox);
        return;
    }
    if weap.overlay_material == 0 {
        gaps.clear(HudGap::AdsOverlay);
        hide_overlay(&mut overlay);
        hide_letterbox(&mut letterbox);
        return;
    }
    if facts.ads_overlay_width <= 0.0 || facts.ads_overlay_height <= 0.0 {
        gaps.raise(GapCause::AdsOverlayNoSize);
        hide_overlay(&mut overlay);
        hide_letterbox(&mut letterbox);
        return;
    }
    let Some(image_name) = weapons.registry().overlay_image_of(viewmodel_index) else {
        gaps.raise(GapCause::AdsOverlayNamesNoImage {
            material: weapons
                .registry()
                .overlay_material_of(viewmodel_index)
                .map(str::to_owned),
        });
        hide_overlay(&mut overlay);
        hide_letterbox(&mut letterbox);
        return;
    };

    let weapon_ns = weapons
        .registry()
        .component_namespace_of(viewmodel_index, asset_game::WeaponComponent::OverlayImage)
        .unwrap_or(crate::images::HUD_CHROME_NAMESPACE);
    let Some(handle) = hud_images.get(weapon_ns, image_name, &mut images) else {
        gaps.raise(GapCause::AdsOverlayImageMissing {
            name: image_name.to_owned(),
            miss: hud_images.miss_reason(),
        });
        hide_overlay(&mut overlay);
        hide_letterbox(&mut letterbox);
        return;
    };

    let scale = PresentationScale::from_window(surface.width(), surface.height());
    let factor = scale.factor(ScaleClass::ProjectionBound);
    let cx = scale.width() * 0.5;
    let cy = scale.height() * 0.5;
    let xhair = if aim.live {
        (aim.xhair_x * factor, aim.xhair_y * factor)
    } else {
        (0.0, 0.0)
    };
    let Some(alpha) = frame.overlay_alpha else {
        gaps.clear(HudGap::AdsOverlay);
        hide_overlay(&mut overlay);
        hide_letterbox(&mut letterbox);
        return;
    };
    let layout = draw_ads_overlay_layout(facts.ads_overlay_width, facts.ads_overlay_height);
    gaps.clear(HudGap::AdsOverlay);

    let inner_left = (cx + xhair.0 + layout.inner_x * factor).round();
    let inner_top = (cy + xhair.1 + layout.inner_y * factor).round();
    let inner_right = (cx + xhair.0 + (layout.inner_x + layout.inner_w) * factor).round();
    let inner_bottom = (cy + xhair.1 + (layout.inner_y + layout.inner_h) * factor).round();
    for (mut node, mut image_node, slot) in overlay.iter_mut() {
        let Some(quad) = layout.live_quads().get(usize::from(slot.0)) else {
            adopt_display(&mut node, Display::None);
            image_node.flip_x = false;
            image_node.flip_y = false;
            continue;
        };
        adopt_display(&mut node, Display::Flex);
        let left = (cx + xhair.0 + quad.x * factor).round();
        let top = (cy + xhair.1 + quad.y * factor).round();
        let right = (cx + xhair.0 + (quad.x + quad.w) * factor).round();
        let bottom = (cy + xhair.1 + (quad.y + quad.h) * factor).round();
        node.left = Val::Px(left);
        node.top = Val::Px(top);
        node.width = Val::Px(right - left);
        node.height = Val::Px(bottom - top);
        image_node.image = handle.clone();
        image_node.color = Color::srgba(1.0, 1.0, 1.0, alpha);
        image_node.rect = None;
        image_node.flip_x = quad.flip_s;
        image_node.flip_y = quad.flip_t;
    }

    let fill = match fill_for_overlay(&mut fill, image_name, &handle, &mut images) {
        Some(sampled) => sampled,
        None => {
            hide_letterbox(&mut letterbox);
            return;
        }
    };

    let fill_color = Color::srgba(
        f32::from(fill.rgba[0]) / 255.0,
        f32::from(fill.rgba[1]) / 255.0,
        f32::from(fill.rgba[2]) / 255.0,
        alpha,
    );
    let image = images.get(&handle).expect("overlay image was loaded");
    let quad = layout.quads[0];
    // Back the outer texture texel and its linear-filter footprint with the frame fill.
    let frame_x = (quad.w * factor * 1.5 / image.width() as f32).ceil();
    let frame_y = (quad.h * factor * 1.5 / image.height() as f32).ceil();
    let fill_left = (inner_left + frame_x).clamp(0.0, scale.width());
    let fill_right = (inner_right - frame_x).clamp(fill_left, scale.width());
    let fill_top = (inner_top + frame_y).clamp(0.0, scale.height());
    let fill_bottom = (inner_bottom - frame_y).clamp(fill_top, scale.height());
    let fill_width = fill_right - fill_left;
    let screen_w = scale.width().round();
    let screen_h = scale.height().round();
    for (mut node, mut image_node, mut bg, strip) in letterbox.iter_mut() {
        let placed = match strip.0 {
            0 if fill_left > 0.0 => Some((0.0, 0.0, fill_left, screen_h)),
            1 if screen_w > fill_right => Some((fill_right, 0.0, screen_w - fill_right, screen_h)),
            2 if fill_top > 0.0 => Some((fill_left, 0.0, fill_width, fill_top)),
            3 if screen_h > fill_bottom => {
                Some((fill_left, fill_bottom, fill_width, screen_h - fill_bottom))
            }
            _ => None,
        };
        let Some((left, top, width, height)) = placed else {
            adopt_display(&mut node, Display::None);
            *bg = BackgroundColor(Color::NONE);
            continue;
        };
        if width <= 0.0 || height <= 0.0 {
            adopt_display(&mut node, Display::None);
            *bg = BackgroundColor(Color::NONE);
            continue;
        }
        adopt_display(&mut node, Display::Flex);
        node.left = Val::Px(left);
        node.top = Val::Px(top);
        node.width = Val::Px(width);
        node.height = Val::Px(height);
        image_node.image = fill.handle.clone();
        image_node.color = Color::srgba(1.0, 1.0, 1.0, alpha);
        image_node.rect = None;
        *bg = BackgroundColor(fill_color);
    }
}

struct SampledFill {
    handle: Handle<Image>,
    rgba: [u8; 4],
}

fn fill_for_overlay(
    fill: &mut IrisLetterboxFill,
    image_name: &str,
    overlay: &Handle<Image>,
    images: &mut Assets<Image>,
) -> Option<SampledFill> {
    if fill.source.as_deref() == Some(image_name) {
        let handle = fill.handle.clone()?;
        let rgba = fill.rgba?;
        return Some(SampledFill { handle, rgba });
    }
    fill.source = Some(image_name.to_owned());
    fill.handle = None;
    fill.rgba = None;
    let (width, height, data) = {
        let image = images.get(overlay)?;
        (image.width(), image.height(), image.data.clone()?)
    };
    let rgba = overlay_frame_texel(width, height, &data)?;
    let handle = fill_handle(rgba, images);
    fill.handle = Some(handle.clone());
    fill.rgba = Some(rgba);
    Some(SampledFill { handle, rgba })
}
