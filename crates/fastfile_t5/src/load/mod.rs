use crate::asset_type::AssetType;
use crate::size as sz;
use crate::zone::{Ptr, Result, XFILE_BLOCK_TEMP, ZoneError, ZonePtr, ZoneStream};

mod clipmap;
mod destructible;
mod fx;
mod gfxworld;
mod glass;
mod material;
mod menu;
mod model;
mod simple;
mod sound;
mod weapon;
mod world;
mod xanim;

pub fn load_asset_body(s: &mut ZoneStream<'_>, ty: AssetType) -> Result<()> {
    load_asset_body_observed(s, ty, &mut IgnoreAssetLinks)
}

fn load_asset_body_observed(
    s: &mut ZoneStream<'_>,
    ty: AssetType,
    links: &mut dyn AssetLinkSink,
) -> Result<()> {
    use AssetType::*;
    match ty {
        TechniqueSet => material::load_technique_set(s, links),
        Material => material::load_material(s, links),
        Image => material::load_image(s),
        Fx => fx::load_fx(s, links),
        XModel => model::load_xmodel(s, links),
        PhysConstraints => model::load_phys_constraints(s, links),
        DestructibleDef => destructible::load_destructible_def(s, links),
        ComWorld => world::load_comworld(s),
        LightDef => world::load_light_def(s, links),
        GfxWorld => gfxworld::load_gfxworld(s, links),
        Glasses => glass::load_glasses(s, links),
        GameWorldMp => world::load_game_world_mp(s),
        GameWorldSp => world::load_game_world_sp(s),
        XGlobals => simple::load_xglobals(s),
        EmblemSet => simple::load_emblem_set(s, links),
        ClipMapMp | ClipMapSp => clipmap::load_clip_map(s, links),
        MapEnts => simple::load_map_ents(s),
        XModelPieces => model::load_xmodel_pieces(s, links),
        Sound => sound::load_snd_bank(s, links),
        SoundPatch => sound::load_snd_patch(s, links),
        SndDriverGlobals => sound::load_snd_driver_globals(s, links),
        Localize => simple::load_localize(s, links),
        RawFile => simple::load_raw_file(s, links),
        PhysPreset => simple::load_phys_preset(s),
        XAnimParts => xanim::load_xanim_parts(s, links),
        MenuList => menu::load_menu_list(s, links),
        Font => simple::load_font(s, links),
        Weapon => weapon::load_weapon(s, links),
        ImpactFx => load_impact_fx(s, links),
        StringTable => simple::load_string_table(s, links),
        Ddl => simple::load_ddl(s),
        other => Err(ZoneError::NoAssetLoader(other)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NestedShaderKind {
    Vertex,
    Pixel,
}

pub trait AssetLinkSink {
    fn loaded(
        &mut self,
        s: &ZoneStream<'_>,
        ty: AssetType,
        slot: Ptr,
        insert_slot: Option<Ptr>,
    ) -> Result<()>;

    fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> Result<()>;

    fn capture_impact_fx(&mut self, _s: &ZoneStream<'_>, _name: Ptr, _entries: Ptr) -> Result<()> {
        Ok(())
    }

    fn capture_destructible(&mut self, _s: &ZoneStream<'_>, _header: Ptr) -> Result<()> {
        Ok(())
    }

    fn remember_material_name(&mut self, _slot: Ptr, _insert_slot: Option<Ptr>, _name: Ptr) {}

    fn material_name_ptr(&self, _slot: Ptr) -> Option<Ptr> {
        None
    }

    fn remember_xmodel_name(&mut self, _slot: Ptr, _insert_slot: Option<Ptr>, _name: Ptr) {}

    fn xmodel_name_ptr(&self, _slot: Ptr) -> Option<Ptr> {
        None
    }

    fn capture_xanim(
        &mut self,
        s: &ZoneStream<'_>,
        geometry: crate::zone::XAnimPartsGeometry,
    ) -> Result<()> {
        let _ = (s, geometry);
        Ok(())
    }

    fn capture_snd_curves(&mut self, s: &ZoneStream<'_>, rows: Ptr, count: usize) -> Result<()> {
        let _ = (s, rows, count);
        Ok(())
    }

    fn capture_snd_groups(&mut self, s: &ZoneStream<'_>, rows: Ptr, count: usize) -> Result<()> {
        let _ = (s, rows, count);
        Ok(())
    }

    fn capture_raw_file(&mut self, name: &str, data: &[u8], zlib_compressed: bool) -> Result<()> {
        let _ = (name, data, zlib_compressed);
        Ok(())
    }

    fn capture_localize(&mut self, name: &str, value: &[u8]) -> Result<()> {
        let _ = (name, value);
        Ok(())
    }

    fn capture_string_table(&mut self, s: &ZoneStream<'_>, header: Ptr) -> Result<()> {
        let _ = (s, header);
        Ok(())
    }

    fn nested_shader(
        &mut self,
        s: &ZoneStream<'_>,
        kind: NestedShaderKind,
        slot: Ptr,
    ) -> Result<()> {
        let _ = (s, kind, slot);
        Ok(())
    }

    fn nested_shader_alias(
        &mut self,
        kind: NestedShaderKind,
        slot: Ptr,
        target: Ptr,
    ) -> Result<()> {
        let _ = (kind, slot, target);
        Ok(())
    }

    fn nested_vertex_decl(&mut self, s: &ZoneStream<'_>, slot: Ptr) -> Result<()> {
        let _ = (s, slot);
        Ok(())
    }

    fn nested_vertex_decl_alias(&mut self, slot: Ptr, target: Ptr) -> Result<()> {
        let _ = (slot, target);
        Ok(())
    }

    fn capture_loaded_sound(
        &mut self,
        s: &ZoneStream<'_>,
        header: Ptr,
        pcm: Ptr,
        data_len: usize,
    ) -> Result<()> {
        let _ = (s, header, pcm, data_len);
        Ok(())
    }

    fn bind_last_loaded_to_sound_file(&mut self, file: Ptr) -> Result<()> {
        let _ = file;
        Ok(())
    }

    fn bind_streamed_sound_file(&mut self, file: Ptr, dir: &str, name: &str) -> Result<()> {
        let _ = (file, dir, name);
        Ok(())
    }

    fn capture_sound(
        &mut self,
        s: &ZoneStream<'_>,
        list: Ptr,
        count: usize,
        head: Option<Ptr>,
    ) -> Result<()> {
        let _ = (s, list, count, head);
        Ok(())
    }

    fn capture_fx(
        &mut self,
        s: &ZoneStream<'_>,
        geometry: crate::zone::FxEffectDefGeometry,
    ) -> Result<()> {
        let _ = (s, geometry);
        Ok(())
    }
}

struct IgnoreAssetLinks;

impl AssetLinkSink for IgnoreAssetLinks {
    fn loaded(
        &mut self,
        _s: &ZoneStream<'_>,
        _ty: AssetType,
        _slot: Ptr,
        _insert_slot: Option<Ptr>,
    ) -> Result<()> {
        Ok(())
    }

    fn alias(&mut self, _ty: AssetType, _slot: Ptr, _target: Ptr) -> Result<()> {
        Ok(())
    }
}

pub fn load_asset_at(s: &mut ZoneStream<'_>, ty: AssetType, slot: Ptr) -> Result<()> {
    load_asset_at_observed(s, ty, slot, &mut IgnoreAssetLinks).map(|_| ())
}

pub fn load_asset_at_observed(
    s: &mut ZoneStream<'_>,
    ty: AssetType,
    slot: Ptr,
    links: &mut dyn AssetLinkSink,
) -> Result<bool> {
    let pointer = s.ptr_at(slot, 0)?;
    s.push(XFILE_BLOCK_TEMP)?;
    let result = match pointer {
        ZonePtr::Null => Ok(false),
        ZonePtr::Offset(target) => {
            s.note_offset(target);
            links.alias(ty, slot, target)?;
            Ok(false)
        }
        ZonePtr::Following | ZonePtr::Insert => {
            let (load, insert_slot) = s.begin_body_with_insert(slot)?;
            debug_assert!(load);
            load_asset_body_observed(s, ty, links)?;
            if ty == AssetType::Material
                && let Some(name) = s.latest_material().and_then(|g| g.name)
            {
                links.remember_material_name(slot, insert_slot, name);
            }
            if ty == AssetType::XModel {
                if let Some(name) = s.latest_xmodel().and_then(|g| g.name) {
                    links.remember_xmodel_name(slot, insert_slot, name);
                }
            }
            links.loaded(s, ty, slot, insert_slot)?;
            Ok(true)
        }
    };
    s.pop()?;
    result
}

pub(crate) fn asset_ptr_at(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    ty: AssetType,
    slot: Ptr,
) -> Result<()> {
    load_asset_at_observed(s, ty, slot, links).map(|_| ())
}

pub(crate) fn follow_name(s: &mut ZoneStream<'_>, p: Ptr, field: usize) -> Result<()> {
    s.follow_string(p, field)?;
    Ok(())
}

fn load_impact_fx(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, sz::FX_IMPACT_TABLE)?;
    s.push(crate::zone::XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    if s.begin_body(p.at(4))? {
        let arr = s.alloc_load(4, sz::FX_IMPACT_ENTRY * sz::FX_IMPACT_ENTRY_COUNT)?;
        for i in 0..sz::FX_IMPACT_ENTRY_COUNT {
            let e = arr.at(i * sz::FX_IMPACT_ENTRY);
            for j in 0..sz::SURF_TYPE_NUM {
                asset_ptr_at(s, links, AssetType::Fx, e.at(j * 4))?;
            }
            for j in 0..sz::FX_IMPACT_FLESH_COUNT {
                asset_ptr_at(s, links, AssetType::Fx, e.at(124 + j * 4))?;
            }
        }
        if let ZonePtr::Offset(name) = s.ptr_at(p, 0)? {
            links.capture_impact_fx(s, s.resolve_alias(name), arr)?;
        }
    }
    s.pop()
}

pub(crate) fn begin_temp_body(s: &mut ZoneStream<'_>, slot: Ptr) -> Result<bool> {
    s.push(XFILE_BLOCK_TEMP)?;
    s.begin_body(slot)
}

pub(crate) fn always_alloc(s: &mut ZoneStream<'_>, slot: Ptr) -> Result<bool> {
    match s.ptr_at(slot, 0)? {
        ZonePtr::Null => Ok(false),
        ZonePtr::Offset(_) | ZonePtr::Following | ZonePtr::Insert => Ok(true),
    }
}

pub(crate) fn always_array(
    s: &mut ZoneStream<'_>,
    slot: Ptr,
    align: usize,
    bytes: usize,
) -> Result<Option<Ptr>> {
    if !always_alloc(s, slot)? {
        return Ok(None);
    }
    let body = s.alloc_load(align, bytes)?;
    s.fixup_slot(slot, body)?;
    Ok(Some(body))
}

pub(crate) fn runtime_array(
    s: &mut ZoneStream<'_>,
    slot: Ptr,
    align: usize,
    bytes: usize,
) -> Result<Option<Ptr>> {
    s.push(crate::zone::XFILE_BLOCK_RUNTIME)?;
    let body = always_array(s, slot, align, bytes)?;
    s.pop()?;
    Ok(body)
}
