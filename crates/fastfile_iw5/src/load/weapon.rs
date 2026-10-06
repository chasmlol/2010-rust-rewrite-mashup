use super::attachment::follow_attachment_array;
use super::snd::{follow_snd_alias_array, follow_snd_alias_custom};
use super::{
    AssetLinkSink, asset_ptr_at, follow_name, load_asset_at_durable_slot, load_asset_at_observed,
};
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::zone::{Ptr, Result, WeaponGeometry, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream};

pub(super) fn load_weapon(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    s.walk_stage = "weapon";
    let p = s.alloc_load(4, s.layout(sz::WEAPON_COMPLETE, 288))?;
    let ai_knots = s.u16_at(p, s.layout(168, 260))? as usize;
    let player_knots = s.u16_at(p, s.layout(170, 262))? as usize;
    let weap_def_off = s.layout(4, 8);

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let (
        weap_def,
        gun_xmodel_name,
        hand_xmodel_name,
        world_model_name,
        knife_xmodel_name,
        move_speed_scale,
        ads_move_speed_scale,
    ) = match s.ptr_at(p, weap_def_off)? {
        ZonePtr::Offset(q) => {
            s.note_offset(q);
            let body = s.resolve_alias(q);
            (
                Some(body),
                xmodel_array_name(s, links, body, s.layout(4, 8)),
                xmodel_name(s, links, body.at(s.layout(8, 16))),
                xmodel_array_name(
                    s,
                    links,
                    body,
                    s.layout(sz::WEAPON_DEF_WORLD_MODEL_OFF, 752),
                ),
                xmodel_name(
                    s,
                    links,
                    body.at(s.layout(sz::WEAPON_DEF_KNIFE_MODEL_OFF, 776)),
                ),
                s.f32_at(body, s.layout(sz::WEAPON_DEF_MOVE_SPEED_OFF, 1332))?,
                s.f32_at(body, s.layout(sz::WEAPON_DEF_ADS_MOVE_SPEED_OFF, 1336))?,
            )
        }
        _ if s.begin_body(p.at(weap_def_off))? => {
            let (body, gun0, hand0, world0, knife0) =
                load_weapon_def(s, links, ai_knots, player_knots)?;
            (
                Some(body),
                gun0,
                hand0,
                world0,
                knife0,
                s.f32_at(body, s.layout(sz::WEAPON_DEF_MOVE_SPEED_OFF, 1332))?,
                s.f32_at(body, s.layout(sz::WEAPON_DEF_ADS_MOVE_SPEED_OFF, 1336))?,
            )
        }
        _ => (None, None, None, None, None, 0.0, 0.0),
    };

    s.walk_stage = "weapon.complete";
    follow_name(s, p, s.layout(8, 16))?;
    let hide_tags_off = s.layout(sz::WEAPON_COMPLETE_HIDE_TAGS_OFF, 24);
    follow_script_string_array(s, p, hide_tags_off, sz::WEAPON_HIDE_TAG_COUNT)?;
    let hide_tags = match s.ptr_at(p, hide_tags_off)? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    follow_attachment_array(s, links, p, s.layout(16, 32), sz::WEAPON_SCOPE_COUNT)?;
    follow_attachment_array(s, links, p, s.layout(20, 40), sz::WEAPON_UNDERBARREL_COUNT)?;
    follow_attachment_array(s, links, p, s.layout(24, 48), sz::WEAPON_OTHER_ATTACH_COUNT)?;
    let attachments = attachment_slot_names(s, p);
    let sz_xanims = follow_string_array(s, p, s.layout(28, 56), sz::WEAPON_ANIM_COUNT)?;

    let anim_overrides_off = s.layout(sz::WEAPON_COMPLETE_ANIM_OVERRIDES_OFF, 72);
    let anim_overrides = s
        .i32_at(p, s.layout(sz::WEAPON_COMPLETE_ANIM_OVERRIDE_COUNT_OFF, 64))?
        .max(0) as usize;
    if s.begin_body(p.at(anim_overrides_off))? {
        let entry = s.layout(sz::ANIM_OVERRIDE_ENTRY, 40);
        let arr = s.alloc_load(4, entry * anim_overrides)?;
        s.fixup_slot(p.at(anim_overrides_off), arr)?;
        for i in 0..anim_overrides {
            load_anim_override(s, arr.at(i * entry))?;
        }
    }
    let anim_override_arr = match s.ptr_at(p, anim_overrides_off)? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    let sound_overrides_off = s.layout(sz::WEAPON_COMPLETE_SOUND_OVERRIDES_OFF, 88);
    let sound_overrides = s
        .i32_at(
            p,
            s.layout(sz::WEAPON_COMPLETE_SOUND_OVERRIDE_COUNT_OFF, 80),
        )?
        .max(0) as usize;
    if s.begin_body(p.at(sound_overrides_off))? {
        let entry = s.layout(sz::SOUND_OVERRIDE_ENTRY, 32);
        let arr = s.alloc_load(4, entry * sound_overrides)?;
        s.fixup_slot(p.at(sound_overrides_off), arr)?;
        for i in 0..sound_overrides {
            load_sound_override(s, arr.at(i * entry))?;
        }
    }
    let sound_override_arr = match s.ptr_at(p, sound_overrides_off)? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    let fx_overrides_off = s.layout(52, 104);
    let overrides = s.i32_at(p, s.layout(48, 96))?.max(0) as usize;
    if s.begin_body(p.at(fx_overrides_off))? {
        let entry = s.layout(sz::FX_OVERRIDE_ENTRY, 32);
        let arr = s.alloc_load(4, entry * overrides)?;
        s.fixup_slot(p.at(fx_overrides_off), arr)?;
        for i in 0..overrides {
            load_fx_override(s, links, arr.at(i * entry))?;
        }
    }
    let reload_overrides_off = s.layout(sz::WEAPON_COMPLETE_RELOAD_OVERRIDES_OFF, 120);
    let reload_overrides = s
        .i32_at(
            p,
            s.layout(sz::WEAPON_COMPLETE_RELOAD_OVERRIDE_COUNT_OFF, 112),
        )?
        .max(0) as usize;
    if s.begin_body(p.at(reload_overrides_off))? {
        let arr = s.alloc_load(4, sz::RELOAD_STATE_TIMER_ENTRY * reload_overrides)?;
        s.fixup_slot(p.at(reload_overrides_off), arr)?;
    }
    let note_overrides_off = s.layout(68, 136);
    let note_overrides = s.i32_at(p, s.layout(64, 128))?.max(0) as usize;
    if s.begin_body(p.at(note_overrides_off))? {
        let entry = s.layout(sz::NOTE_TRACK_SOUND_ENTRY, 24);
        let arr = s.alloc_load(4, entry * note_overrides)?;
        s.fixup_slot(p.at(note_overrides_off), arr)?;
        for i in 0..note_overrides {
            load_notetrack_override(s, arr.at(i * entry))?;
        }
    }

    let alternate_weapon_name = s.follow_string(p, s.layout(116, 192))?;
    // The complete def lives in TEMP and is overwritten by the next asset: keep a slot that outlives it.
    let kill_icon_cell = p.at(s.layout(sz::WEAPON_COMPLETE_KILL_ICON_OFF, 216));
    let referenced = match s.ptr_at(kill_icon_cell, 0)? {
        ZonePtr::Offset(q) => Some(q),
        _ => None,
    };
    let (kill_icon_slot, kill_icon) =
        match load_asset_at_durable_slot(s, AssetType::Material, kill_icon_cell, links)? {
            Some(Some(insert_slot)) => (Some(insert_slot), None),
            Some(None) => (None, s.latest_material().and_then(|m| m.header)),
            None => (referenced, None),
        };
    asset_ptr_at(s, links, AssetType::Material, p.at(s.layout(136, 224)))?;
    s.plain_array(p, s.layout(172, 264), 4, 8, ai_knots)?;
    s.plain_array(p, s.layout(176, 272), 4, 8, player_knots)?;

    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    let (weap_type, weap_class, fire_type) = match weap_def {
        Some(body) => (
            s.i32_at(body, s.layout(sz::WEAPON_DEF_WEAP_TYPE_OFF, 84))?,
            s.i32_at(body, s.layout(sz::WEAPON_DEF_WEAP_CLASS_OFF, 88))?,
            s.i32_at(body, s.layout(sz::WEAPON_DEF_FIRE_TYPE_OFF, 100))?,
        ),
        None => (0, 0, 0),
    };
    let (ads_zoom_in_frac, ads_zoom_out_frac, ads_in_rate, ads_out_rate) = match weap_def {
        Some(body) => (
            s.f32_at(body, s.layout(sz::WEAPON_DEF_ADS_ZOOM_IN_FRAC_OFF, 1344))
                .unwrap_or(0.0),
            s.f32_at(body, s.layout(sz::WEAPON_DEF_ADS_ZOOM_OUT_FRAC_OFF, 1348))
                .unwrap_or(0.0),
            s.f32_at(body, s.layout(sz::WEAPON_DEF_ADS_IN_RATE_OFF, 2136))
                .unwrap_or(0.0),
            s.f32_at(body, s.layout(sz::WEAPON_DEF_ADS_OUT_RATE_OFF, 2140))
                .unwrap_or(0.0),
        ),
        None => (0.0, 0.0, 0.0, 0.0),
    };
    let ads_zoom_fov = s.f32_at(p, s.layout(sz::WEAPON_COMPLETE_ADS_ZOOM_FOV_OFF, 144))?;
    let display_name = match s.ptr_at(p, s.layout(8, 16))? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };

    let (ads_overlay_width, ads_overlay_height, overlay_reticle, overlay_interface) = match weap_def
    {
        Some(body) => (
            s.f32_at(body, s.layout(sz::WEAPON_DEF_OVERLAY_WIDTH_OFF, 1388))
                .unwrap_or(0.0),
            s.f32_at(body, s.layout(sz::WEAPON_DEF_OVERLAY_HEIGHT_OFF, 1392))
                .unwrap_or(0.0),
            s.i32_at(body, s.layout(sz::WEAPON_DEF_OVERLAY_RETICLE_OFF, 1384))
                .unwrap_or(0),
            s.i32_at(body, s.layout(sz::WEAPON_DEF_OVERLAY_INTERFACE_OFF, 1404))
                .unwrap_or(0),
        ),
        None => (0.0, 0.0, 0, 0),
    };
    let array_at = |s: &ZoneStream<'_>, off: usize| match s.ptr_at(p, off) {
        Ok(ZonePtr::Offset(q)) => Some(s.resolve_alias(q)),
        _ => None,
    };
    let rocket_model_name = match weap_def {
        Some(body) => xmodel_name(s, links, body.at(s.layout(488, 768))),
        None => None,
    };
    let projectile_model_name = match weap_def {
        Some(body) => xmodel_name(s, links, body.at(s.layout(1304, 1672))),
        None => None,
    };
    let overlay_material_names = core::array::from_fn(|index| {
        let body = weap_def?;
        super::material_name_at(
            s,
            links,
            body.at(s.layout(sz::WEAPON_DEF_OVERLAY_SHADER_OFF, 1352) + index * s.pointer_bytes()),
        )
    });
    s.record_weapon(WeaponGeometry {
        overlay_material_names,
        rocket_model_name,
        projectile_model_name,
        alternate_weapon_name,
        alternate_raise_time_ms: s.i32_at(p, s.layout(124, 208))?,
        alternate_drop_time_ms: weap_def
            .map(|body| s.i32_at(body, s.layout(0x298, 1000)))
            .transpose()?
            .unwrap_or_default(),
        name,
        kill_icon_slot,
        kill_icon,
        display_name,
        weap_def,
        gun_xmodel_name,
        hand_xmodel_name,
        world_model_name,
        knife_xmodel_name,
        hide_tags,
        sz_xanims,
        ads_view_kick_center_speed: s.f32_at(
            p,
            s.layout(sz::WEAPON_COMPLETE_ADS_VIEW_KICK_CENTER_SPEED_OFF, 180),
        )?,
        hip_view_kick_center_speed: s.f32_at(
            p,
            s.layout(sz::WEAPON_COMPLETE_HIP_VIEW_KICK_CENTER_SPEED_OFF, 184),
        )?,
        ads_zoom_fov,
        ads_zoom_in_frac,
        ads_zoom_out_frac,
        ads_in_rate,
        ads_out_rate,
        ads_trans_in_time_ms: s
            .i32_at(p, s.layout(sz::WEAPON_COMPLETE_ADS_TRANS_IN_TIME_OFF, 148))?,
        ads_trans_out_time_ms: s
            .i32_at(p, s.layout(sz::WEAPON_COMPLETE_ADS_TRANS_OUT_TIME_OFF, 152))?,
        penetrate_multiplier: s.f32_at(
            p,
            s.layout(sz::WEAPON_COMPLETE_PENETRATE_MULTIPLIER_OFF, 176),
        )?,
        motion_tracker: s.u8_at(p, s.layout(sz::WEAPON_COMPLETE_MOTION_TRACKER_OFF, 280))? != 0,
        fire_time_ms: s.i32_at(p, s.layout(sz::WEAPON_COMPLETE_FIRE_TIME_OFF, 164))?,
        clip_size: s.i32_at(p, s.layout(sz::WEAPON_COMPLETE_CLIP_OFF, 156))?,
        impact_type: s.i32_at(p, s.layout(sz::WEAPON_COMPLETE_IMPACT_TYPE_OFF, 160))?,
        weap_type,
        weap_class,
        fire_type,
        move_speed_scale,
        ads_move_speed_scale,
        ads_overlay_width,
        ads_overlay_height,
        overlay_reticle,
        overlay_interface,
        attachments,
        anim_override_count: anim_overrides as i32,
        anim_overrides: anim_override_arr,
        sound_override_count: sound_overrides as i32,
        sound_overrides: sound_override_arr,
        fx_override_count: overrides as i32,
        overrides: array_at(s, fx_overrides_off),
        reload_override_count: reload_overrides as i32,
        reload_overrides: array_at(s, reload_overrides_off),
        note_track_override_count: note_overrides as i32,
        note_track_overrides: array_at(s, note_overrides_off),
    });
    s.pop()
}

fn attachment_slot_names(
    s: &ZoneStream<'_>,
    complete: Ptr,
) -> [Option<Ptr>; sz::WEAPON_ATTACHMENT_SLOT_COUNT] {
    let mut names = [None; sz::WEAPON_ATTACHMENT_SLOT_COUNT];
    let width = s.pointer_bytes();
    let groups = [
        (s.layout(16, 32), sz::WEAPON_SCOPE_COUNT),
        (s.layout(20, 40), sz::WEAPON_UNDERBARREL_COUNT),
        (s.layout(24, 48), sz::WEAPON_OTHER_ATTACH_COUNT),
    ];
    let mut out = names.iter_mut();
    for (field, count) in groups {
        let arr = match s.ptr_at(complete, field) {
            Ok(ZonePtr::Offset(q)) => Some(s.resolve_alias(q)),
            _ => None,
        };
        for i in 0..count {
            let Some(dest) = out.next() else {
                break;
            };
            let Some(arr) = arr else {
                continue;
            };
            let cell = arr.at(i * width);
            *dest = s
                .attachment_name(cell)
                .or_else(|| match s.ptr_at(cell, 0) {
                    Ok(ZonePtr::Offset(q)) => s
                        .attachment_name(q)
                        .or_else(|| s.attachment_name(s.resolve_alias(q))),
                    _ => None,
                })
                .flatten();
        }
    }
    names
}

fn load_weapon_def(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    ai_knots: usize,
    player_knots: usize,
) -> Result<(Ptr, Option<Ptr>, Option<Ptr>, Option<Ptr>, Option<Ptr>)> {
    s.walk_stage = "weapon.def";
    let width = s.pointer_bytes();
    let p = s.alloc_load(4, s.layout(sz::WEAPON_DEF, 2496))?;
    follow_name(s, p, 0)?;
    let gun0 = follow_xmodel_array(s, links, p, s.layout(4, 8))?;
    let hand0 = follow_xmodel_ptr(s, links, p.at(s.layout(8, 16)))?;
    follow_string_array(s, p, s.layout(12, 24), sz::WEAPON_ANIM_COUNT)?;
    follow_string_array(s, p, s.layout(16, 32), sz::WEAPON_ANIM_COUNT)?;
    follow_name(s, p, s.layout(20, 40))?;
    for (field, count) in [
        (s.layout(24, 48), 24),
        (s.layout(28, 56), 24),
        (s.layout(32, 64), 16),
        (s.layout(36, 72), 16),
    ] {
        follow_script_string_array(s, p, field, count)?;
    }
    asset_ptr_at(s, links, AssetType::Fx, p.at(s.layout(72, 112)))?;
    asset_ptr_at(s, links, AssetType::Fx, p.at(s.layout(76, 120)))?;

    let snd_first = s.layout(80, 128);
    for i in 0..48 {
        follow_snd_alias_custom(s, p.at(snd_first + i * width))?;
    }
    follow_snd_alias_array(s, p, s.layout(272, 512), sz::SURF_TYPE_COUNT)?;
    follow_snd_alias_array(s, p, s.layout(276, 520), sz::SURF_TYPE_COUNT)?;
    for off in [
        s.layout(280, 528),
        s.layout(284, 536),
        s.layout(288, 544),
        s.layout(292, 552),
    ] {
        asset_ptr_at(s, links, AssetType::Fx, p.at(off))?;
    }
    for off in [s.layout(296, 560), s.layout(300, 568)] {
        asset_ptr_at(s, links, AssetType::Material, p.at(off))?;
    }
    let world0 = follow_xmodel_array(s, links, p, s.layout(sz::WEAPON_DEF_WORLD_MODEL_OFF, 752))?;
    for off in [s.layout(484, 760), s.layout(488, 768)] {
        asset_ptr_at(s, links, AssetType::XModel, p.at(off))?;
    }
    let knife0 = follow_xmodel_ptr(
        s,
        links,
        p.at(s.layout(sz::WEAPON_DEF_KNIFE_MODEL_OFF, 776)),
    )?;
    asset_ptr_at(s, links, AssetType::XModel, p.at(s.layout(496, 784)))?;
    asset_ptr_at(s, links, AssetType::Material, p.at(s.layout(500, 792)))?;
    asset_ptr_at(s, links, AssetType::Material, p.at(s.layout(508, 808)))?;
    asset_ptr_at(s, links, AssetType::Material, p.at(s.layout(516, 824)))?;
    for off in [s.layout(532, 848), s.layout(540, 864), s.layout(556, 888)] {
        follow_name(s, p, off)?;
    }
    let overlay = p.at(s.layout(sz::WEAPON_DEF_OVERLAY_SHADER_OFF, 1352));
    for i in 0..4 {
        asset_ptr_at(s, links, AssetType::Material, overlay.at(i * width))?;
    }
    asset_ptr_at(s, links, AssetType::PhysCollMap, p.at(s.layout(1216, 1576)))?;
    asset_ptr_at(s, links, AssetType::XModel, p.at(s.layout(1304, 1672)))?;
    asset_ptr_at(s, links, AssetType::Fx, p.at(s.layout(1312, 1688)))?;
    asset_ptr_at(s, links, AssetType::Fx, p.at(s.layout(1316, 1696)))?;
    follow_snd_alias_custom(s, p.at(s.layout(1320, 1704)))?;
    follow_snd_alias_custom(s, p.at(s.layout(1324, 1712)))?;
    if s.begin_body(p.at(s.layout(1352, 1744)))? {
        s.alloc_load(4, 4 * sz::SURF_TYPE_COUNT)?;
    }
    if s.begin_body(p.at(s.layout(1356, 1752)))? {
        s.alloc_load(4, 4 * sz::SURF_TYPE_COUNT)?;
    }
    asset_ptr_at(s, links, AssetType::Fx, p.at(s.layout(1360, 1760)))?;
    asset_ptr_at(s, links, AssetType::Fx, p.at(s.layout(1364, 1768)))?;
    asset_ptr_at(s, links, AssetType::Fx, p.at(s.layout(1392, 1800)))?;
    follow_snd_alias_custom(s, p.at(s.layout(1396, 1808)))?;
    follow_name(s, p, s.layout(1552, 1968))?;
    if s.begin_body(p.at(s.layout(1560, 1984)))? {
        s.alloc_load(4, 8 * ai_knots)?;
    }
    follow_name(s, p, s.layout(1556, 1976))?;
    if s.begin_body(p.at(s.layout(1564, 1992)))? {
        s.alloc_load(4, 8 * player_knots)?;
    }
    for off in [
        s.layout(1644, 2080),
        s.layout(1648, 2088),
        s.layout(1680, 2128),
    ] {
        follow_name(s, p, off)?;
    }
    if s.begin_body(p.at(s.layout(1720, 2176)))? {
        s.alloc_load(4, 4 * sz::HITLOC_COUNT)?;
    }
    follow_name(s, p, s.layout(1724, 2184))?;
    follow_name(s, p, s.layout(1728, 2192))?;
    asset_ptr_at(s, links, AssetType::Tracer, p.at(s.layout(1732, 2200)))?;
    follow_snd_alias_custom(s, p.at(s.layout(1776, 2248)))?;
    asset_ptr_at(s, links, AssetType::Fx, p.at(s.layout(1780, 2256)))?;
    follow_name(s, p, s.layout(1784, 2264))?;
    follow_snd_alias_custom(s, p.at(s.layout(1800, 2288)))?;
    let spin_up = s.layout(1804, 2296);
    for i in 0..4 {
        follow_snd_alias_custom(s, p.at(spin_up + i * width))?;
    }
    let spin_down = s.layout(1820, 2328);
    for i in 0..4 {
        follow_snd_alias_custom(s, p.at(spin_down + i * width))?;
    }
    follow_snd_alias_custom(s, p.at(s.layout(1836, 2360)))?;
    follow_snd_alias_custom(s, p.at(s.layout(1840, 2368)))?;
    asset_ptr_at(s, links, AssetType::XModel, p.at(s.layout(1952, 2488)))?;
    Ok((p, gun0, hand0, world0, knife0))
}

fn load_anim_override(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    follow_name(s, p, s.layout(sz::ANIM_OVERRIDE_OVERRIDE_ANIM_OFF, 8))?;
    follow_name(s, p, s.layout(sz::ANIM_OVERRIDE_ALTMODE_ANIM_OFF, 16))
}

fn load_sound_override(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    follow_snd_alias_custom(s, p.at(s.layout(4, 8)))?;
    follow_snd_alias_custom(s, p.at(s.layout(8, 16)))
}

fn load_fx_override(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    asset_ptr_at(s, links, AssetType::Fx, p.at(s.layout(4, 8)))?;
    asset_ptr_at(s, links, AssetType::Fx, p.at(s.layout(8, 16)))
}

fn load_notetrack_override(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    follow_script_string_array(s, p, s.layout(4, 8), 24)?;
    follow_script_string_array(s, p, s.layout(8, 16), 24)
}

fn follow_script_string_array(
    s: &mut ZoneStream<'_>,
    p: Ptr,
    field: usize,
    count: usize,
) -> Result<()> {
    match s.ptr_at(p, field)? {
        ZonePtr::Null => Ok(()),
        ZonePtr::Offset(q) => {
            s.note_offset(q);
            Ok(())
        }
        _ => {
            if !s.begin_body(p.at(field))? {
                return Ok(());
            }
            let arr = s.alloc_load(2, 2 * count)?;
            s.fixup_slot(p.at(field), arr)
        }
    }
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
            let width = s.pointer_bytes();
            let arr = s.alloc_load(4, width * count)?;
            s.fixup_slot(p.at(field), arr)?;
            for i in 0..count {
                s.follow_string(arr, i * width)?;
            }
            Ok(Some(arr))
        }
    }
}

fn xmodel_name(s: &ZoneStream<'_>, links: &dyn AssetLinkSink, slot: Ptr) -> Option<Ptr> {
    match s.ptr_at(slot, 0).ok()? {
        ZonePtr::Null => None,
        ZonePtr::Offset(ptr) => links
            .xmodel_name_ptr(s.resolve_alias(ptr))
            .or_else(|| links.xmodel_name_ptr(ptr))
            .or_else(|| links.xmodel_name_ptr(slot)),
        ZonePtr::Following | ZonePtr::Insert => links.xmodel_name_ptr(slot),
    }
}

fn xmodel_array_name(
    s: &ZoneStream<'_>,
    links: &dyn AssetLinkSink,
    body: Ptr,
    field: usize,
) -> Option<Ptr> {
    let ZonePtr::Offset(ptr) = s.ptr_at(body, field).ok()? else {
        return None;
    };
    xmodel_name(s, links, s.resolve_alias(ptr))
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
            Ok(xmodel_name(s, links, arr))
        }
        _ => {
            if !s.begin_body(p.at(field))? {
                return Ok(None);
            }
            let width = s.pointer_bytes();
            let arr = s.alloc_load(4, width * sz::ATTACH_MODEL_COUNT)?;
            s.fixup_slot(p.at(field), arr)?;
            let mut first_name = None;
            for i in 0..sz::ATTACH_MODEL_COUNT {
                let slot = arr.at(i * width);
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
