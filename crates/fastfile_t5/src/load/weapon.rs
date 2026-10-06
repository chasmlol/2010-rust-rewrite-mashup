use super::{AssetLinkSink, asset_ptr_at, follow_name, load_asset_at_observed};
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::zone::{Ptr, Result, WeaponGeometry, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream};

pub(super) fn load_weapon(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, sz::WEAPON_VARIANT_DEF)?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let (body_combat, weap_def, assets) = match s.ptr_at(p, 0x8)? {
        ZonePtr::Offset(q) => {
            s.note_offset(q);
            let body = s.resolve_alias(q);
            (
                read_body_combat(s, body)?,
                Some(body),
                WeaponAssets {
                    reticle_center: material_name_at(s, links, body.at(0x1a0)),
                    reticle_side: material_name_at(s, links, body.at(0x1a4)),
                    ..WeaponAssets::default()
                },
            )
        }
        _ if s.begin_body(p.at(0x8))? => {
            let def = s.alloc_load(4, sz::WEAPON_DEF)?;
            s.fixup_slot(p.at(0x8), def)?;
            let assets = load_weapon_def(s, links, def)?;
            (read_body_combat(s, def)?, Some(def), assets)
        }
        _ => (BodyCombat::default(), None, WeaponAssets::default()),
    };

    follow_name(s, p, 0xc)?;

    let alternate_weapon_name = s.follow_string(p, 0x14)?;

    let sz_xanims = follow_string_array(s, p, 0x10, sz::WEAPON_XANIM_COUNT)?;

    let _ = s.plain_array(
        p,
        sz::WEAPON_VARIANT_HIDE_TAGS_OFF,
        2,
        2,
        sz::WEAPON_HIDE_TAG_COUNT,
    )?;
    let hide_tags = match s.ptr_at(p, sz::WEAPON_VARIANT_HIDE_TAGS_OFF)? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };

    follow_name(s, p, 0x40)?;
    follow_name(s, p, 0x48)?;
    let overlay_material_name = follow_material_name(s, links, p.at(0x8c))?;
    let overlay_material_lowres_name = follow_material_name(s, links, p.at(0x90))?;
    asset_ptr_at(s, links, AssetType::Material, p.at(0x94))?;

    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    let clip_size = s.i32_at(p, sz::WEAPON_VARIANT_CLIP_SIZE_OFF)?;
    let display_name = match s.ptr_at(p, 12)? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    s.record_weapon(WeaponGeometry {
        overlay_material_name,
        overlay_material_lowres_name,
        reticle_center_name: assets.reticle_center,
        reticle_side_name: assets.reticle_side,
        alternate_weapon_name,
        alternate_raise_time_ms: s.i32_at(p, 0x3c)?,
        alternate_drop_time_ms: weap_def
            .map(|body| s.i32_at(body, 0x400))
            .transpose()?
            .unwrap_or_default(),
        name,
        display_name,
        weap_def,
        gun_xmodel_name: assets.gun,
        hand_xmodel_name: assets.hand,
        rocket_model_name: assets.rocket,
        projectile_model_name: assets.projectile,
        world_model_name: assets.world,
        move_speed_scale: body_combat.move_speed_scale,
        ads_move_speed_scale: body_combat.ads_move_speed_scale,
        weap_type: body_combat.weap_type,
        weap_class: body_combat.weap_class,
        fire_type: body_combat.fire_type,
        fire_time_ms: body_combat.fire_time_ms,
        rechamber_time_ms: body_combat.rechamber_time_ms,
        drop_time_ms: body_combat.drop_time_ms,
        raise_time_ms: body_combat.raise_time_ms,
        bolt_action: body_combat.bolt_action,
        clip_size,
        sz_xanims,
        hide_tags,
        variant: Some(p),
    });

    s.pop()
}

fn load_weapon_def(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    p: Ptr,
) -> Result<WeaponAssets> {
    follow_name(s, p, 0x0)?;

    let gun0 = follow_xmodel_array(s, links, p, 0x4)?;
    let hand0 = follow_xmodel_ptr(s, links, p.at(0x8))?;
    follow_name(s, p, 0xc)?;
    let _ = s.plain_array(p, 0x10, 2, 2, sz::WEAPON_NOTETRACK_COUNT)?;
    let _ = s.plain_array(p, 0x14, 2, 2, sz::WEAPON_NOTETRACK_COUNT)?;
    follow_name(s, p, 0x3c)?;

    asset_ptr_at(s, links, AssetType::Fx, p.at(0x74))?;
    asset_ptr_at(s, links, AssetType::Fx, p.at(0x78))?;

    for off in [
        0x07c, 0x080, 0x084, 0x088, 0x08c, 0x090, 0x094, 0x098, 0x09c, 0x0a0, 0x0a4, 0x0a8, 0x0ac,
        0x0b0, 0x0b4, 0x0b8, 0x0bc, 0x0c0, 0x0c4, 0x0c8, 0x0cc, 0x0d0, 0x0d4, 0x0d8, 0x0dc, 0x0e0,
        0x0e4, 0x0e8, 0x0ec, 0x0f0, 0x0f4, 0x0f8, 0x0fc, 0x100, 0x104, 0x108, 0x10c, 0x110, 0x114,
        0x118, 0x11c, 0x120, 0x124, 0x128, 0x12c, 0x130, 0x134, 0x138, 0x13c, 0x140, 0x144, 0x148,
        0x14c, 0x150, 0x154, 0x158, 0x15c, 0x160, 0x164, 0x168, 0x16c, 0x170,
    ] {
        follow_name(s, p, off)?;
    }

    let _ = follow_string_array(s, p, 0x174, sz::WEAPON_BOUNCE_SOUND_COUNT)?;

    follow_name(s, p, 0x178)?;
    follow_name(s, p, 0x17c)?;
    follow_name(s, p, 0x180)?;

    for off in [0x190, 0x194, 0x198, 0x19c] {
        asset_ptr_at(s, links, AssetType::Fx, p.at(off))?;
    }
    let reticle_center = follow_material_name(s, links, p.at(0x1a0))?;
    let reticle_side = follow_material_name(s, links, p.at(0x1a4))?;

    let world0 = follow_xmodel_array(s, links, p, 0x30c)?;
    asset_ptr_at(s, links, AssetType::XModel, p.at(0x310))?;
    let rocket = follow_xmodel_ptr(s, links, p.at(0x314))?;
    for off in [0x318, 0x31c] {
        asset_ptr_at(s, links, AssetType::XModel, p.at(off))?;
    }

    asset_ptr_at(s, links, AssetType::Material, p.at(0x320))?;
    asset_ptr_at(s, links, AssetType::Material, p.at(0x330))?;
    follow_name(s, p, 0x34c)?;

    for off in [0x394, 0x398, 0x39c, 0x3a0, 0x3a4, 0x3a8, 0x48c] {
        follow_name(s, p, off)?;
    }
    asset_ptr_at(s, links, AssetType::Material, p.at(0x578))?;
    asset_ptr_at(s, links, AssetType::Material, p.at(0x328))?;
    follow_name(s, p, 0x58c)?;
    follow_name(s, p, 0x590)?;
    let projectile = follow_xmodel_ptr(s, links, p.at(0x5e8))?;

    for off in [0x5f0, 0x5f8, 0x600, 0x608, 0x610, 0x618] {
        asset_ptr_at(s, links, AssetType::Fx, p.at(off))?;
    }
    for off in [0x61c, 0x620, 0x624, 0x628] {
        follow_name(s, p, off)?;
    }

    let _ = s.plain_array(
        p,
        sz::WEAPON_DEF_PARALLEL_BOUNCE_OFF,
        4,
        4,
        sz::WEAPON_BOUNCE_COEFFICIENT_COUNT,
    )?;
    let _ = s.plain_array(
        p,
        sz::WEAPON_DEF_PERPENDICULAR_BOUNCE_OFF,
        4,
        4,
        sz::WEAPON_BOUNCE_COEFFICIENT_COUNT,
    )?;

    asset_ptr_at(s, links, AssetType::Fx, p.at(0x658))?;
    asset_ptr_at(s, links, AssetType::Fx, p.at(0x674))?;
    follow_name(s, p, 0x678)?;

    follow_name(s, p, 0x714)?;
    let knot0 = s.i32_at(p, 0x72c)?.max(0) as usize;
    let _ = s.plain_array(p, 0x71c, 4, 8, knot0)?;
    let _ = s.plain_array(p, 0x724, 4, 8, knot0)?;

    follow_name(s, p, 0x718)?;
    let knot1 = s.i32_at(p, 0x730)?.max(0) as usize;
    let _ = s.plain_array(p, 0x720, 4, 8, knot1)?;
    let _ = s.plain_array(p, 0x728, 4, 8, knot1)?;

    follow_name(s, p, 0x784)?;
    follow_name(s, p, 0x788)?;
    follow_name(s, p, 0x79c)?;
    let _ = s.plain_array(p, 0x7bc, 4, 4, sz::WEAPON_LOCATION_DAMAGE_COUNT)?;

    for off in [0x7c0, 0x7c4, 0x7c8, 0x7e8, 0x7ec] {
        follow_name(s, p, off)?;
    }

    if s.begin_body(p.at(0x7f0))? {
        let ft = s.alloc_load(4, sz::FLAME_TABLE)?;
        s.fixup_slot(p.at(0x7f0), ft)?;
        load_flame_table(s, links, ft)?;
    }
    if s.begin_body(p.at(0x7f4))? {
        let ft = s.alloc_load(4, sz::FLAME_TABLE)?;
        s.fixup_slot(p.at(0x7f4), ft)?;
        load_flame_table(s, links, ft)?;
    }

    asset_ptr_at(s, links, AssetType::Fx, p.at(0x7f8))?;
    asset_ptr_at(s, links, AssetType::Fx, p.at(0x7fc))?;
    Ok(WeaponAssets {
        gun: gun0,
        hand: hand0,
        rocket,
        projectile,
        world: world0,
        reticle_center,
        reticle_side,
    })
}

fn follow_string_array(
    s: &mut ZoneStream<'_>,
    p: Ptr,
    field: usize,
    count: usize,
) -> Result<Option<Ptr>> {
    match s.ptr_at(p, field)? {
        ZonePtr::Null => Ok(None),
        ZonePtr::Offset(q) => {
            s.note_offset(q);
            Ok(Some(s.resolve_alias(q)))
        }
        _ => {
            if !s.begin_body(p.at(field))? {
                return Ok(None);
            }
            let arr = s.alloc_load(4, 4 * count)?;
            s.fixup_slot(p.at(field), arr)?;
            for i in 0..count {
                s.follow_string(arr, i * 4)?;
            }
            Ok(Some(arr))
        }
    }
}

fn follow_xmodel_array(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    p: Ptr,
    field: usize,
) -> Result<Option<Ptr>> {
    match s.ptr_at(p, field)? {
        ZonePtr::Null => Ok(None),
        ZonePtr::Offset(q) => {
            s.note_offset(q);
            let arr = s.resolve_alias(q);
            Ok(match s.ptr_at(arr, 0)? {
                ZonePtr::Offset(m) => {
                    let body = s.resolve_alias(m);
                    links
                        .xmodel_name_ptr(body)
                        .or_else(|| links.xmodel_name_ptr(m))
                }
                _ => None,
            })
        }
        _ => {
            if !s.begin_body(p.at(field))? {
                return Ok(None);
            }
            let arr = s.alloc_load(4, 4 * sz::WEAPON_GUN_MODEL_COUNT)?;
            s.fixup_slot(p.at(field), arr)?;
            let mut first_name = None;
            for i in 0..sz::WEAPON_GUN_MODEL_COUNT {
                let slot = arr.at(i * 4);
                let loaded = load_asset_at_observed(s, AssetType::XModel, slot, links)?;
                if i == 0 {
                    first_name = if loaded {
                        s.latest_xmodel().and_then(|g| g.name)
                    } else {
                        match s.ptr_at(slot, 0)? {
                            ZonePtr::Offset(q) => {
                                let body = s.resolve_alias(q);
                                links
                                    .xmodel_name_ptr(body)
                                    .or_else(|| links.xmodel_name_ptr(q))
                                    .or_else(|| links.xmodel_name_ptr(slot))
                            }
                            _ => None,
                        }
                    };
                }
            }
            Ok(first_name)
        }
    }
}

fn follow_xmodel_ptr(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    slot: Ptr,
) -> Result<Option<Ptr>> {
    let loaded = load_asset_at_observed(s, AssetType::XModel, slot, links)?;
    if loaded {
        if let Some(name) = s.latest_xmodel().and_then(|g| g.name) {
            return Ok(Some(name));
        }
    }
    Ok(links
        .xmodel_name_ptr(slot)
        .or_else(|| match s.ptr_at(slot, 0).ok()? {
            ZonePtr::Offset(q) => {
                let body = s.resolve_alias(q);
                links
                    .xmodel_name_ptr(body)
                    .or_else(|| links.xmodel_name_ptr(q))
            }
            _ => None,
        }))
}

fn load_flame_table(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    follow_name(s, p, 0x1a8)?;
    for off in [0x1ac, 0x1b0, 0x1b4, 0x1b8, 0x1bc, 0x1c0, 0x1c4, 0x1c8] {
        asset_ptr_at(s, links, AssetType::Material, p.at(off))?;
    }
    for off in [0x1cc, 0x1d0, 0x1d4, 0x1d8] {
        follow_name(s, p, off)?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default)]
struct BodyCombat {
    move_speed_scale: f32,
    ads_move_speed_scale: f32,
    weap_type: i32,
    weap_class: i32,
    fire_type: i32,
    fire_time_ms: i32,
    rechamber_time_ms: i32,
    drop_time_ms: i32,
    raise_time_ms: i32,
    bolt_action: bool,
}

fn read_body_combat(s: &ZoneStream<'_>, body: Ptr) -> Result<BodyCombat> {
    Ok(BodyCombat {
        move_speed_scale: s.f32_at(body, sz::WEAPON_MOVE_SPEED_SCALE_OFF)?,
        ads_move_speed_scale: s.f32_at(body, sz::WEAPON_ADS_MOVE_SPEED_SCALE_OFF)?,
        weap_type: s.i32_at(body, sz::WEAPON_TYPE_OFF)?,
        weap_class: s.i32_at(body, sz::WEAPON_CLASS_OFF)?,
        fire_type: s.i32_at(body, sz::WEAPON_FIRE_TYPE_OFF)?,
        fire_time_ms: s.i32_at(body, sz::WEAPON_FIRE_TIME_OFF)?,
        rechamber_time_ms: s.i32_at(body, sz::WEAPON_RECHAMBER_TIME_OFF)?,
        drop_time_ms: s.i32_at(body, sz::WEAPON_DROP_TIME_OFF)?,
        raise_time_ms: s.i32_at(body, sz::WEAPON_RAISE_TIME_OFF)?,
        bolt_action: s.u8_at(body, sz::WEAPON_BOLT_ACTION_OFF)? != 0,
    })
}

fn material_name_at(s: &ZoneStream<'_>, links: &dyn AssetLinkSink, slot: Ptr) -> Option<Ptr> {
    match s.ptr_at(slot, 0).ok()? {
        ZonePtr::Null => None,
        ZonePtr::Offset(target) => links
            .material_name_ptr(target)
            .or_else(|| links.material_name_ptr(slot)),
        ZonePtr::Following | ZonePtr::Insert => links.material_name_ptr(slot),
    }
}

fn follow_material_name(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    slot: Ptr,
) -> Result<Option<Ptr>> {
    if load_asset_at_observed(s, AssetType::Material, slot, links)? {
        Ok(s.latest_material().and_then(|g| g.name))
    } else {
        Ok(material_name_at(s, links, slot))
    }
}

#[derive(Default)]
struct WeaponAssets {
    gun: Option<Ptr>,
    hand: Option<Ptr>,
    rocket: Option<Ptr>,
    projectile: Option<Ptr>,
    world: Option<Ptr>,
    reticle_center: Option<Ptr>,
    reticle_side: Option<Ptr>,
}
