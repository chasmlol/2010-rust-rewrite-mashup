use super::snd::follow_snd_alias_custom;
use super::{AssetLinkSink, asset_ptr_at, follow_name};
use crate::asset_type::AssetType;
use crate::attachment::read_attachment_facts;
use crate::size as sz;
use crate::zone::{AttachmentGeometry, Ptr, Result, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream};

pub(super) fn load_attachment(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    s.walk_stage = "attachment";
    let p = s.alloc_load(4, s.layout(sz::WEAPON_ATTACHMENT, 264))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    follow_name(s, p, s.layout(4, 8))?;
    follow_xmodel_array(
        s,
        links,
        p,
        s.layout(sz::ATTACH_WORLD_MODELS_OFF, 32),
        sz::ATTACH_MODEL_COUNT,
    )?;
    follow_xmodel_array(
        s,
        links,
        p,
        s.layout(sz::ATTACH_VIEW_MODELS_OFF, 40),
        sz::ATTACH_MODEL_COUNT,
    )?;
    follow_xmodel_array(
        s,
        links,
        p,
        s.layout(sz::ATTACH_RETICLE_OFF, 48),
        sz::ATTACH_RETICLE_COUNT,
    )?;
    let ammo_general = s.layout(32, 56);
    if s.begin_body(p.at(ammo_general))? {
        let body = s.alloc_load(4, s.layout(sz::ATT_AMMO_GENERAL, 32))?;
        s.fixup_slot(p.at(ammo_general), body)?;
        asset_ptr_at(s, links, AssetType::Tracer, body.at(16))?;
    }
    let sight = s.layout(36, 64);
    if s.begin_body(p.at(sight))? {
        let body = s.alloc_load(4, sz::ATT_SIGHT)?;
        s.fixup_slot(p.at(sight), body)?;
    }
    let reload = s.layout(40, 72);
    if s.begin_body(p.at(reload))? {
        let body = s.alloc_load(1, sz::ATT_RELOAD)?;
        s.fixup_slot(p.at(reload), body)?;
    }
    let addons = s.layout(44, 80);
    if s.begin_body(p.at(addons))? {
        let body = s.alloc_load(1, sz::ATT_ADDONS)?;
        s.fixup_slot(p.at(addons), body)?;
    }
    let general = s.layout(48, 88);
    if s.begin_body(p.at(general))? {
        let body = s.alloc_load(4, s.layout(sz::ATT_GENERAL, 40))?;
        s.fixup_slot(p.at(general), body)?;
        asset_ptr_at(s, links, AssetType::Material, body.at(8))?;
        asset_ptr_at(s, links, AssetType::Material, body.at(s.layout(12, 16)))?;
    }
    for (field, bytes) in [
        (s.layout(52, 96), sz::ATT_AIM_ASSIST),
        (s.layout(56, 104), sz::ATT_AMMUNITION),
        (s.layout(60, 112), sz::ATT_DAMAGE),
        (s.layout(64, 120), sz::ATT_LOCATION_DAMAGE),
        (s.layout(68, 128), sz::ATT_IDLE_SETTINGS),
        (s.layout(72, 136), sz::ATT_ADS_SETTINGS),
        (s.layout(76, 144), sz::ATT_ADS_SETTINGS),
        (s.layout(80, 152), sz::ATT_HIP_SPREAD),
        (s.layout(84, 160), sz::ATT_GUN_KICK),
        (s.layout(88, 168), sz::ATT_VIEW_KICK),
    ] {
        if s.begin_body(p.at(field))? {
            let body = s.alloc_load(4, bytes)?;
            s.fixup_slot(p.at(field), body)?;
        }
    }
    let (overlay_names, overlay_width, overlay_height, overlay_reticle, thermal) =
        load_ads_overlay(s, links, p)?;
    let ui = s.layout(96, 184);
    if s.begin_body(p.at(ui))? {
        let body = s.alloc_load(4, s.layout(sz::ATT_UI, 32))?;
        s.fixup_slot(p.at(ui), body)?;
        asset_ptr_at(s, links, AssetType::Material, body.at(0))?;
        asset_ptr_at(s, links, AssetType::Material, body.at(s.layout(4, 8)))?;
    }
    let rumbles = s.layout(100, 192);
    if s.begin_body(p.at(rumbles))? {
        let body = s.alloc_load(4, s.layout(sz::ATT_RUMBLES, 16))?;
        s.fixup_slot(p.at(rumbles), body)?;
        follow_name(s, body, 0)?;
        follow_name(s, body, s.layout(4, 8))?;
    }
    let projectile = s.layout(104, 200);
    if s.begin_body(p.at(projectile))? {
        let body = s.alloc_load(4, s.layout(sz::ATT_PROJECTILE, 136))?;
        s.fixup_slot(p.at(projectile), body)?;
        asset_ptr_at(s, links, AssetType::XModel, body.at(32))?;
        asset_ptr_at(s, links, AssetType::Fx, body.at(s.layout(40, 48)))?;
        follow_snd_alias_custom(s, body.at(s.layout(48, 64)))?;
        asset_ptr_at(s, links, AssetType::Fx, body.at(s.layout(52, 72)))?;
        follow_snd_alias_custom(s, body.at(s.layout(56, 80)))?;
        asset_ptr_at(s, links, AssetType::Fx, body.at(s.layout(76, 104)))?;
        asset_ptr_at(s, links, AssetType::Fx, body.at(s.layout(84, 120)))?;
        follow_snd_alias_custom(s, body.at(s.layout(88, 128)))?;
    }
    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    s.set_latest_attachment_name(name);
    let geometry = AttachmentGeometry {
        header: p,
        name,
        view_model_name: first_model_name(s, links, p, s.layout(sz::ATTACH_VIEW_MODELS_OFF, 40)),
        world_model_name: first_model_name(s, links, p, s.layout(sz::ATTACH_WORLD_MODELS_OFF, 32)),
        view_model_names: core::array::from_fn(|i| {
            model_name_at(s, links, p, s.layout(sz::ATTACH_VIEW_MODELS_OFF, 40), i)
        }),
        world_model_names: core::array::from_fn(|i| {
            model_name_at(s, links, p, s.layout(sz::ATTACH_WORLD_MODELS_OFF, 32), i)
        }),
        reticle_model_names: core::array::from_fn(|i| {
            model_name_at(s, links, p, s.layout(sz::ATTACH_RETICLE_OFF, 48), i)
        }),
        overlay_names,
        overlay_width,
        overlay_height,
        overlay_reticle,
        thermal,
        facts: read_attachment_facts(s, p),
    };
    links.capture_attachment(s, &geometry)?;
    s.pop()
}

fn load_ads_overlay(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    att: Ptr,
) -> Result<([Option<Ptr>; 4], f32, f32, i32, bool)> {
    let slot = att.at(s.layout(sz::ATTACH_ADS_OVERLAY_OFF, 176));
    let offs = [
        sz::ADS_OVERLAY_SHADER_OFF,
        s.layout(sz::ADS_OVERLAY_SHADER_LOWRES_OFF, 8),
        s.layout(sz::ADS_OVERLAY_SHADER_EMP_OFF, 16),
        s.layout(sz::ADS_OVERLAY_SHADER_EMP_LOWRES_OFF, 24),
    ];
    let mut names = [None; 4];
    let mut body = None;
    if s.begin_body(slot)? {
        let loaded = s.alloc_load(4, s.layout(sz::ATT_ADS_OVERLAY, 64))?;
        s.fixup_slot(slot, loaded)?;
        body = Some(loaded);
        for (off, dest) in offs.into_iter().zip(&mut names) {
            asset_ptr_at(s, links, AssetType::Material, loaded.at(off))?;
            *dest = super::material_name_at(s, links, loaded.at(off));
        }
    } else if let Ok(ZonePtr::Offset(q)) = s.ptr_at(slot, 0) {
        let loaded = s.resolve_alias(q);
        body = Some(loaded);
        for (off, dest) in offs.into_iter().zip(&mut names) {
            *dest = super::material_name_at(s, links, loaded.at(off));
        }
    }
    let width = body
        .and_then(|p| s.f32_at(p, s.layout(sz::ADS_OVERLAY_WIDTH_OFF, 36)).ok())
        .unwrap_or(0.0);
    let height = body
        .and_then(|p| s.f32_at(p, s.layout(sz::ADS_OVERLAY_HEIGHT_OFF, 40)).ok())
        .unwrap_or(0.0);
    let reticle = body
        .and_then(|p| s.i32_at(p, s.layout(sz::ADS_OVERLAY_RETICLE_OFF, 32)).ok())
        .unwrap_or(0);
    let thermal = body
        .and_then(|p| s.u8_at(p, s.layout(sz::ADS_OVERLAY_THERMAL_OFF, 56)).ok())
        .unwrap_or(0)
        != 0;
    Ok((names, width, height, reticle, thermal))
}

fn first_model_name(
    s: &ZoneStream<'_>,
    links: &dyn AssetLinkSink,
    att: Ptr,
    field: usize,
) -> Option<Ptr> {
    model_name_at(s, links, att, field, 0)
}

fn model_name_at(
    s: &ZoneStream<'_>,
    links: &dyn AssetLinkSink,
    att: Ptr,
    field: usize,
    index: usize,
) -> Option<Ptr> {
    let slot = match s.ptr_at(att, field).ok()? {
        ZonePtr::Offset(q) => s.resolve_alias(q),
        _ => return None,
    };
    let slot = slot.at(index * s.pointer_bytes());
    match s.ptr_at(slot, 0).ok()? {
        ZonePtr::Offset(q) => {
            let body = s.resolve_alias(q);
            match s.ptr_at(body, 0) {
                Ok(ZonePtr::Offset(n)) => Some(s.resolve_alias(n)),
                _ => links
                    .xmodel_name_ptr(body)
                    .or_else(|| links.xmodel_name_ptr(q)),
            }
        }
        _ => links.xmodel_name_ptr(slot),
    }
}

fn follow_xmodel_array(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
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
            let width = s.pointer_bytes();
            let arr = s.alloc_load(4, width * count)?;
            s.fixup_slot(p.at(field), arr)?;
            for i in 0..count {
                asset_ptr_at(s, links, AssetType::XModel, arr.at(i * width))?;
            }
            Ok(())
        }
    }
}

pub(super) fn follow_attachment_array(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
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
            let width = s.pointer_bytes();
            let arr = s.alloc_load(4, width * count)?;
            s.fixup_slot(p.at(field), arr)?;
            for i in 0..count {
                asset_ptr_at(s, links, AssetType::Attachment, arr.at(i * width))?;
            }
            Ok(())
        }
    }
}
