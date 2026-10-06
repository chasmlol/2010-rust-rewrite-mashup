use assets::BoundWeapons;
use hud_iw4::{
    AmmoCounterClipKind, CLIP_PIP_EMPTY_ALPHA, CLIP_PIP_EMPTY_RGB, ammo_counter_clip_kind,
    clip_pip_belt_xy, clip_pip_grid_xy, clip_pip_metrics,
};
use playerstate_iw4::PlayerState;
use sim::ClientSnapshotMeta;
use weapon_iw4::{
    ammo_row_present, ammo_table_key, clip_row_present, clip_table_key, get_ammo_not_in_clip,
    get_clip_for_hand, get_viewmodel_weapon_index, num_hands_for_held,
};

use crate::chrome::{OwnerDrawArgs, OwnerDrawPaint};
use crate::draw2d::{Draw2dOp, Draw2dProvenance};
use crate::surface::Hud2dSurface;

pub(crate) const OWNERDRAW_STOCK: i32 = 119;

pub(crate) const OWNERDRAW_CLIP: i32 = 117;

pub(crate) const OWNERDRAW_CLIP_LEFT: i32 = 121;

pub(crate) const OWNERDRAW_OFFHAND_FRAG: i32 = 103;

pub(crate) const OWNERDRAW_OFFHAND_SMOKE: i32 = 104;

pub(crate) const OWNERDRAW_LOW_AMMO: i32 = 120;

pub(crate) const OWNERDRAW_WEAPON_NAME: i32 = 81;

pub(crate) const OWNERDRAW_WEAPON_NAME_KILLCAM: i32 = 83;

pub(crate) const OWNERDRAW_COMPASS_RING: i32 = 166;

const PIP_POOL: usize = 100;

pub(crate) struct WeaponbarAmmo {
    pub kind: AmmoCounterClipKind,
    pub stock: Option<i32>,
    pub clip: Option<i32>,
    pub clip_alt: Option<i32>,
    pub clip_size: i32,
    pub dual: bool,
    pub pip_image: Option<&'static str>,
    pub ammo_counter_clip: i32,
    pub low_ammo_warning_threshold: f32,
    pub clip_only: bool,
}

pub(crate) fn weaponbar_ammo(
    ps: &PlayerState,
    weapons: &BoundWeapons<'_>,
    meta: Option<&ClientSnapshotMeta>,
) -> Option<WeaponbarAmmo> {
    let viewmodel = get_viewmodel_weapon_index(ps);
    let facts = weapons
        .row(viewmodel)
        .and_then(|weapon| weapon.hud_facts())?;
    let kind = ammo_counter_clip_kind(facts.ammo_counter_clip)?;
    let clip_key = clip_table_key(facts.clip_index, viewmodel);
    let ammo_key = ammo_table_key(facts.ammo_index, viewmodel);
    let stock = match kind {
        AmmoCounterClipKind::None => Some(0),
        AmmoCounterClipKind::AltWeapon => None,
        _ => {
            if ammo_row_present(&ps.ammo, ammo_key) {
                Some(get_ammo_not_in_clip(&ps.ammo, ammo_key))
            } else {
                meta.map(|m| m.ammo_stock)
            }
        }
    };
    let clip = match kind {
        AmmoCounterClipKind::None => Some(0),
        AmmoCounterClipKind::AltWeapon => None,
        _ => {
            if clip_row_present(&ps.ammoclip, clip_key) {
                Some(get_clip_for_hand(&ps.ammoclip, clip_key, 0))
            } else {
                meta.map(|m| m.ammo_clip)
            }
        }
    };
    let dual = num_hands_for_held(&ps.weapons, &ps.weapon_data, viewmodel) >= 1;
    let clip_alt = if dual {
        Some(if clip_row_present(&ps.ammoclip, clip_key) {
            get_clip_for_hand(&ps.ammoclip, clip_key, 1)
        } else {
            0
        })
    } else {
        None
    };
    let pip_image = clip_pip_metrics(kind).map(|m| m.image);
    Some(WeaponbarAmmo {
        kind,
        stock,
        clip,
        clip_alt,
        clip_size: facts.clip_size,
        dual,
        pip_image,
        ammo_counter_clip: facts.ammo_counter_clip,
        low_ammo_warning_threshold: facts.low_ammo_warning_threshold,
        clip_only: facts.clip_only,
    })
}

pub(crate) fn stock_digits(count: i32) -> String {
    format!("{count:3}")
}

pub(crate) fn offhand_ammo(ps: &PlayerState, weapons: &BoundWeapons<'_>, class: i32) -> i32 {
    if class == 0 {
        return 0;
    }
    let mut total = 0;
    for &slot in &ps.weapons {
        if slot <= 0 {
            continue;
        }
        let index = slot as u32;
        let Some(facts) = weapons.row(index).and_then(|weapon| weapon.hud_facts()) else {
            continue;
        };
        if facts.offhand_class != class {
            continue;
        }
        let ammo_key = ammo_table_key(facts.ammo_index, index);
        let clip_key = clip_table_key(facts.clip_index, index);
        if ammo_row_present(&ps.ammo, ammo_key) {
            total += get_ammo_not_in_clip(&ps.ammo, ammo_key);
        }
        if clip_row_present(&ps.ammoclip, clip_key) {
            total += get_clip_for_hand(&ps.ammoclip, clip_key, 0);
        }
    }
    total
}

pub(crate) fn offhand_weapon_index(
    ps: &PlayerState,
    weapons: &BoundWeapons<'_>,
    class: i32,
) -> Option<u32> {
    if class == 0 {
        return None;
    }
    ps.weapons.iter().find_map(|&slot| {
        if slot <= 0 {
            return None;
        }
        let index = slot as u32;
        weapons
            .row(index)
            .and_then(|weapon| weapon.hud_facts())
            .filter(|facts| facts.offhand_class == class)
            .map(|_| index)
    })
}

fn weaponbar_scaled(
    metrics: hud_iw4::ClipPipMetrics,
    surface: &Hud2dSurface,
) -> hud_iw4::ClipPipMetrics {
    let [sx, sy] = surface.scale_virtual_to_real();
    hud_iw4::ClipPipMetrics {
        width: metrics.width * sx,
        height: metrics.height * sy,
        step_x: metrics.step_x * sx,
        step_y: metrics.step_y * sy,
        ..metrics
    }
}

fn pip_hand_local(pip: usize, clip_size: usize) -> Option<(i32, i32)> {
    if clip_size == 0 {
        return None;
    }
    if pip < clip_size {
        Some((0, pip as i32))
    } else {
        let local = pip - clip_size;
        (local < clip_size).then_some((1, local as i32))
    }
}

pub(crate) fn paint_clip_pips(
    args: &OwnerDrawArgs<'_>,
    ammo: &WeaponbarAmmo,
    hand: i32,
    frame: &mut crate::chrome::ChromeFrame,
) -> OwnerDrawPaint {
    let Some(clip_count) = ammo.clip else {
        return OwnerDrawPaint::Painted;
    };
    if hand == 1 && !ammo.dual {
        return OwnerDrawPaint::Painted;
    }
    let Some(metrics) = clip_pip_metrics(ammo.kind).map(|m| weaponbar_scaled(m, args.surface))
    else {
        return OwnerDrawPaint::Painted;
    };
    let Some(image) = ammo.pip_image else {
        return OwnerDrawPaint::Painted;
    };
    let zigzag = ammo.kind == AmmoCounterClipKind::Beltfed;
    let clip_size_n = ammo.clip_size.max(0) as usize;
    let applied = args.surface.apply_rect(
        args.rect.x,
        args.rect.y,
        0.0,
        0.0,
        args.rect.horz_align as i32,
        args.rect.vert_align as i32,
    );
    let base = [applied.x, applied.y];
    let align = args.rect.horz_align as i32;
    let hand_n = if ammo.dual { 2 } else { 1 };
    let pip_n = clip_size_n.saturating_mul(hand_n).min(PIP_POOL);
    for i in 0..pip_n {
        let Some((pip_hand, local)) = pip_hand_local(i, clip_size_n) else {
            continue;
        };
        if pip_hand != hand {
            continue;
        }
        let [x, y] = if zigzag {
            clip_pip_belt_xy(metrics, base, local, ammo.clip_size, align)
        } else {
            clip_pip_grid_xy(metrics, base, local, align)
        };
        let filled = if hand == 0 {
            local < clip_count
        } else {
            ammo.clip_alt.is_some_and(|n| local < n)
        };
        let color = if filled {
            args.color
        } else {
            [
                CLIP_PIP_EMPTY_RGB,
                CLIP_PIP_EMPTY_RGB,
                CLIP_PIP_EMPTY_RGB,
                CLIP_PIP_EMPTY_ALPHA * args.color[3],
            ]
        };
        frame.list.cmds.push(crate::draw2d::Draw2dCmd {
            material_namespace: crate::images::HUD_CHROME_NAMESPACE,
            x,
            y,
            w: metrics.width,
            h: metrics.height,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color,
            material: image.to_owned(),
            op: Draw2dOp::StretchPic,
            provenance: Draw2dProvenance::OwnerDraw(args.item.owner_draw),
            layer: 1,
        });
    }
    OwnerDrawPaint::Painted
}
