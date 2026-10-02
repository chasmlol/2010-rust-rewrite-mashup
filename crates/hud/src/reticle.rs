use assets::PreparedWeapons;
use bevy::prelude::*;
use bevy::ui::Display;
use frame::{LifeStarted, ViewSubject};
use hud_iw4::{
    CG_CROSSHAIR_ALPHA_DEFAULT, CG_CROSSHAIR_ALPHA_MIN_DEFAULT, HipCrosshairGate,
    SCREEN_BLEND_BLURRED, WeaponAdsCrosshairFacts, WeaponReticleFacts, calc_reticle_alpha,
    calc_reticle_spread, hip_crosshair_trans_scale, hip_crosshair_visible, is_flashbanged,
    reticle_draw_size,
};
use movement_iw4::mantle::is_weapon_inactive;
use net::{FrameClock, LocalPresentClient, PresentedSnapshot, ViewweaponAim};
use playerstate_iw4::{KillCamMode, ThirdPersonViewInputs, is_third_person_view};
use weapon_iw4::{
    SpreadOverrideState, WeaponSpreadFacts, get_spread_for_weapon, get_viewmodel_weapon_index,
    perk_weap_spread_multiplier, should_apply_view_org_bob,
};

use crate::gaps::{GapCause, HudGap, HudPresentationGaps, ImageMiss, ReticleSlot};
use crate::images::HudImages;
use crate::presentation_scale::{PresentationScale, ScaleClass};
use crate::ui_write::adopt_display;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ReticleQuad {
    Side(u8),

    Center,
}

#[derive(Resource, Default)]
pub(crate) struct ReticleAdsLatch {
    last_frac: f32,
    pub(crate) position_to_ads: bool,
}

pub(crate) fn spawn_reticle(root: &mut ChildSpawnerCommands) {
    for quad in [
        ReticleQuad::Side(0),
        ReticleQuad::Side(1),
        ReticleQuad::Side(2),
        ReticleQuad::Side(3),
        ReticleQuad::Center,
    ] {
        root.spawn((
            quad,
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                ..default()
            },
            UiTransform::IDENTITY,
            ImageNode::default(),
        ));
    }
}

fn hide_all(quads: &mut Query<(&ReticleQuad, &mut Node, &mut ImageNode, &mut UiTransform)>) {
    for (_, mut node, _, _) in quads.iter_mut() {
        adopt_display(&mut node, Display::None);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_reticle(
    surface: Res<crate::surface::Hud2dSurface>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    weapons: Option<Res<PreparedWeapons>>,
    cameras: Query<&Projection, With<Camera3d>>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
    mut gaps: ResMut<HudPresentationGaps>,
    mut ads_latch: ResMut<ReticleAdsLatch>,
    aim: Res<ViewweaponAim>,
    cg_clock: Res<FrameClock>,
    mut quads: Query<(&ReticleQuad, &mut Node, &mut ImageNode, &mut UiTransform)>,
    life: (
        MessageReader<LifeStarted>,
        Res<ViewSubject>,
        Option<Res<frame::SkateMode>>,
        Option<Res<frame::MinecraftUi>>,
        Option<Res<frame::JakMode>>,
    ),
) {
    let (mut started, view, skate, minecraft, jak) = life;
    for ev in started.read() {
        if ev.client == local.0.0 {
            *ads_latch = ReticleAdsLatch::default();
        }
    }
    if skate.is_some_and(|mode| mode.active)
        || jak.is_some_and(|mode| mode.active)
        || minecraft.is_some_and(|ui| ui.active && ui.inventory_open)
    {
        hide_all(&mut quads);
        return;
    }
    if !surface.is_ready() {
        return;
    }
    let Some(ps) = presented.player(local.0) else {
        hide_all(&mut quads);
        return;
    };
    if ps.pm_type >= playerstate_iw4::PM_TYPE_DEAD {
        hide_all(&mut quads);
        return;
    }

    if ps.f_weapon_pos_frac > ads_latch.last_frac + 1e-4 {
        ads_latch.position_to_ads = true;
    } else if ps.f_weapon_pos_frac + 1e-4 < ads_latch.last_frac {
        ads_latch.position_to_ads = false;
    }
    ads_latch.last_frac = ps.f_weapon_pos_frac;

    let viewmodel_index = get_viewmodel_weapon_index(ps);
    let Some(weapons) = weapons.as_ref() else {
        gaps.raise(GapCause::ReticleNoWeaponCatalog);
        hide_all(&mut quads);
        return;
    };
    let Some(facts) = weapons.0.facts_of(viewmodel_index) else {
        gaps.raise(GapCause::ReticleWeaponNotInCatalog { viewmodel_index });
        hide_all(&mut quads);
        return;
    };
    gaps.clear(HudGap::ReticleWeaponDef);

    let reticle_facts = WeaponReticleFacts {
        i_reticle_min_ofs: facts.i_reticle_min_ofs,
        hip_reticle_side_pos: facts.hip_reticle_side_pos,
        i_reticle_side_size: facts.i_reticle_side_size,
    };
    let ads_xf = WeaponAdsCrosshairFacts {
        ads_aim_pitch: facts.ads_aim_pitch,
        ads_crosshair_in_frac: facts.ads_crosshair_in_frac,
        ads_crosshair_out_frac: facts.ads_crosshair_out_frac,
    };
    let spread_facts = WeaponSpreadFacts {
        stand_min: facts.hip_spread_stand_min,
        ducked_min: facts.hip_spread_ducked_min,
        prone_min: facts.hip_spread_prone_min,
        stand_max: facts.hip_spread_stand_max,
        ducked_max: facts.hip_spread_ducked_max,
        prone_max: facts.hip_spread_prone_max,
    };
    let aim_spread = ps.aim_spread_scale;

    let mantle_inactive = is_weapon_inactive(ps, true);
    let rendering_third_person = (view.in_killcam()
        && ps.kill_cam_entity != playerstate_iw4::ENTITYNUM_NONE)
        || is_third_person_view(ThirdPersonViewInputs {
            pm_type: ps.pm_type,
            other_flags: ps.other_flags,
            link_flags: ps.link_flags,
            cg_third_person: false,
            in_killcam: view.in_killcam(),
            killcam_mode: KillCamMode::Mode0,
        });
    let gate = HipCrosshairGate {
        rendering_third_person,
        e_flags: ps.e_flags,
        other_flags: ps.other_flags,
        viewmodel_weapon_index: viewmodel_index as i32,
        flashbanged: is_flashbanged(
            cg_clock.time(),
            ps.shellshock_time,
            ps.shellshock_duration,
            presented
                .shellshock(local.0)
                .map_or(SCREEN_BLEND_BLURRED, |shock| shock.screen_type),
        ) != 0,
        draw_hud: true,
        dvars_allow: true,
        f_weapon_pos_frac: ps.f_weapon_pos_frac,
        cg_draw_gun: true,

        bob_gate: should_apply_view_org_bob(
            false,
            ps.pm_type,
            ps.other_flags,
            ps.link_flags,
            ps.e_flags,
            ps.f_weapon_pos_frac,
            facts.overlay_reticle,
        ),
        weaponstate_primary: ps.weaponstate_primary,
        weaponstate_secondary: ps.weaponstate_secondary,
        last_weapon_hand: ps.last_weapon_hand,
        mantle_weapon_inactive: mantle_inactive,
    };
    if !hip_crosshair_visible(&gate) {
        hide_all(&mut quads);
        return;
    }

    let Some(assets) = weapons.0.reticle_of(viewmodel_index) else {
        gaps.raise(GapCause::ReticleNoAuthoredMaterials { viewmodel_index });
        hide_all(&mut quads);
        return;
    };

    let Some(tan_half) = cameras.iter().find_map(|p| match p {
        Projection::Perspective(persp) => Some((persp.fov * 0.5).tan()),
        _ => None,
    }) else {
        hide_all(&mut quads);
        return;
    };
    let trans_scale = hip_crosshair_trans_scale(
        ps.f_weapon_pos_frac,
        ads_latch.position_to_ads,
        &ads_xf,
        tan_half,
    );
    let side_size_v = reticle_draw_size(&reticle_facts, trans_scale);
    let cone = get_spread_for_weapon(
        ps.view_height_current,
        ps.spread_override,
        SpreadOverrideState::from_i32(ps.spread_override_state),
        &spread_facts,
        perk_weap_spread_multiplier(ps.perks[0]),
    );
    let gap_v = calc_reticle_spread(
        cone.min,
        cone.max,
        aim_spread,
        trans_scale,
        tan_half,
        &reticle_facts,
        side_size_v,
    );

    let center_size_v = assets.center_size as f32 * trans_scale;
    let alpha = calc_reticle_alpha(
        1.0,
        CG_CROSSHAIR_ALPHA_DEFAULT,
        CG_CROSSHAIR_ALPHA_MIN_DEFAULT,
        aim_spread,
    );

    let scale = PresentationScale::from_window(surface.width(), surface.height());
    let factor = scale.factor(ScaleClass::ProjectionBound);
    let cx = scale.width() * 0.5;
    let cy = scale.height() * 0.5;

    let xhair_px = if aim.live {
        (aim.xhair_x * factor, aim.xhair_y * factor)
    } else {
        (0.0, 0.0)
    };

    let weapon_ns = weapons
        .0
        .namespace_of(viewmodel_index)
        .unwrap_or(crate::images::HUD_CHROME_NAMESPACE);
    let center = resolve_slot(
        ReticleSlot::Center,
        assets.center_authored,
        &assets.center_image,
        weapon_ns,
        &mut hud_images,
        &mut images,
    );
    let side = resolve_slot(
        ReticleSlot::Side,
        assets.side_authored,
        &assets.side_image,
        weapon_ns,
        &mut hud_images,
        &mut images,
    );
    let center_handle = center.handle();
    let side_handle = side.handle();
    match material_gap_cause(&center, &side) {
        Some(cause) => gaps.raise(cause),
        None => gaps.clear(HudGap::ReticleMaterial),
    }

    for (quad, mut node, mut image_node, mut xform) in quads.iter_mut() {
        let (handle, size_v, offset) = match *quad {
            ReticleQuad::Center => (center_handle.clone(), center_size_v, None),
            ReticleQuad::Side(i) => (side_handle.clone(), side_size_v[0], Some(i)),
        };
        let Some(handle) = handle else {
            adopt_display(&mut node, Display::None);
            continue;
        };
        if size_v <= 0.0 {
            adopt_display(&mut node, Display::None);
            continue;
        }
        let size = size_v * factor;

        let (dx, dy, turns) = match offset {
            None => (0.0, 0.0, 0u8),
            Some(0) => (0.0, -(gap_v[1] * factor), 0),
            Some(1) => (gap_v[0] * factor, 0.0, 1),
            Some(2) => (0.0, gap_v[1] * factor, 2),
            Some(_) => (-(gap_v[0] * factor), 0.0, 3),
        };
        adopt_display(&mut node, Display::Flex);
        node.left = Val::Px(cx + xhair_px.0 + dx - size * 0.5);
        node.top = Val::Px(cy + xhair_px.1 + dy - size * 0.5);
        node.width = Val::Px(size);
        node.height = Val::Px(size);
        xform.rotation = Rot2::degrees(f32::from(turns) * 90.0);
        image_node.image = handle;
        image_node.color = Color::srgba(1.0, 1.0, 1.0, alpha);
    }
}

struct SlotResolution {
    slot: ReticleSlot,

    authored: bool,
    image: SlotImage,
}

enum SlotImage {
    Unnamed,

    Drawn(Handle<Image>),

    Missing { name: String, miss: ImageMiss },
}

impl SlotResolution {
    fn handle(&self) -> Option<Handle<Image>> {
        match &self.image {
            SlotImage::Drawn(handle) => Some(handle.clone()),
            SlotImage::Unnamed | SlotImage::Missing { .. } => None,
        }
    }
}

fn resolve_slot(
    slot: ReticleSlot,
    authored: bool,
    name: &Option<String>,
    namespace: asset_core::AssetNamespace,
    hud_images: &mut HudImages,
    images: &mut Assets<Image>,
) -> SlotResolution {
    let image = match name.as_deref() {
        None => SlotImage::Unnamed,
        Some(name) => match hud_images.get(namespace, name, images) {
            Some(handle) => SlotImage::Drawn(handle),
            None => SlotImage::Missing {
                name: name.to_owned(),
                miss: hud_images.miss_reason(),
            },
        },
    };
    SlotResolution {
        slot,
        authored,
        image,
    }
}

fn material_gap_cause(center: &SlotResolution, side: &SlotResolution) -> Option<GapCause> {
    slot_gap_cause(center).or_else(|| slot_gap_cause(side))
}

fn slot_gap_cause(resolution: &SlotResolution) -> Option<GapCause> {
    if !resolution.authored {
        return None;
    }
    match &resolution.image {
        SlotImage::Drawn(_) => None,
        SlotImage::Unnamed => Some(GapCause::ReticleSlotNamesNoImage {
            slot: resolution.slot,
        }),
        SlotImage::Missing { name, miss } => Some(GapCause::ReticleImageMissing {
            slot: resolution.slot,
            name: name.clone(),
            miss: *miss,
        }),
    }
}
