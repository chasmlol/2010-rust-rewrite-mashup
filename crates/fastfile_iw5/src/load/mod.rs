use crate::asset_type::AssetType;
use crate::wire::Iw5WireFormat;
use crate::zone::{Ptr, Result, XFILE_BLOCK_TEMP, ZoneError, ZonePtr, ZoneStream};

mod attachment;
mod clipmap;
mod fx;
mod gfxworld;
mod material;
mod menu;
mod model;
mod simple;
mod snd;
mod sound;
mod tracer;
mod vehicle;
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
        PixelShader | VertexShader => material::load_shader(s),
        VertexDecl => material::load_vertex_decl(s),
        XAnimParts => xanim::load_xanim_parts(s, links),
        XModel => model::load_xmodel(s, links),
        XModelSurfs => model::load_xmodel_surfs(s, links),
        PhysCollMap => model::load_phys_collmap(s, links),
        RawFile => simple::load_raw_file(s, links),
        StringTable => simple::load_string_table(s, links),
        ScriptFile => simple::load_script_file(s, links),
        PhysPreset => simple::load_phys_preset(s),
        Localize => simple::load_localize(s, links),
        Fx => fx::load_fx(s, links),
        ImpactFx => fx::load_impact_fx(s, links),
        SurfaceFx => fx::load_surface_fx(s, links),
        LightDef => world::load_light_def(s, links),
        ComWorld => world::load_comworld(s),
        FxWorld => world::load_fxworld(s, links),
        GlassWorld => world::load_glass_world(s),
        GfxWorld => gfxworld::load_gfxworld(s, links),
        ClipMap => clipmap::load_clipmap(s, links),
        MapEnts => clipmap::load_mapents(s),
        AddonMapEnts => clipmap::load_addonmapents(s),
        PathData => clipmap::load_pathdata(s),
        VehicleTrack => clipmap::load_vehicletrack(s),
        Sound => sound::load_sound(s, links),
        LoadedSound => sound::load_loaded_sound(s, links),
        SoundCurve => sound::load_snd_curve(s, links),
        MenuList => menu::load_menu_list(s, links),
        Menu => menu::load_menu_def_asset(s, links),
        Font => simple::load_font(s, links),
        Attachment => attachment::load_attachment(s, links),
        Weapon => weapon::load_weapon(s, links),
        Tracer => tracer::load_tracer(s, links),
        Vehicle => vehicle::load_vehicle(s, links),
        Leaderboard => simple::load_leaderboard_def(s),
        StructuredDataDef => simple::load_structured_data_def_set(s),
        other => Err(ZoneError::NoAssetLoader(other)),
    }
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

    fn capture_xanim(
        &mut self,
        s: &ZoneStream<'_>,
        geometry: crate::zone::XAnimPartsGeometry,
    ) -> Result<()> {
        let _ = (s, geometry);
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

    fn capture_attachment(
        &mut self,
        s: &ZoneStream<'_>,
        geometry: &crate::zone::AttachmentGeometry,
    ) -> Result<()> {
        let _ = (s, geometry);
        Ok(())
    }

    fn remember_xmodel_surfaces(&mut self, _slot: Ptr, _surfaces: Option<Ptr>, _name: Option<Ptr>) {
    }

    fn xmodel_surfaces(&self, _slot: Ptr) -> (Option<Ptr>, Option<Ptr>) {
        (None, None)
    }

    fn xmodel_name_ptr(&self, _slot: Ptr) -> Option<Ptr> {
        None
    }

    fn remember_material_name(&mut self, _slot: Ptr, _insert_slot: Option<Ptr>, _name: Ptr) {}

    fn material_name_ptr(&self, _slot: Ptr) -> Option<Ptr> {
        None
    }

    fn remember_xmodel_name(&mut self, _slot: Ptr, _insert_slot: Option<Ptr>, _name: Ptr) {}

    fn capture_script_file(&mut self, name: &str, stack: &[u8], bytecode: &[u8]) -> Result<()> {
        let _ = (name, stack, bytecode);
        Ok(())
    }

    fn capture_raw_file(&mut self, name: &str, data: &[u8], zlib_compressed: bool) -> Result<()> {
        let _ = (name, data, zlib_compressed);
        Ok(())
    }

    fn capture_string_table(&mut self, s: &ZoneStream<'_>, header: Ptr) -> Result<()> {
        let _ = (s, header);
        Ok(())
    }

    fn capture_localize(&mut self, name: &str, value: &[u8]) -> Result<()> {
        let _ = (name, value);
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

    fn capture_snd_curve(&mut self, s: &ZoneStream<'_>, header: Ptr) -> Result<()> {
        let _ = (s, header);
        Ok(())
    }

    fn bind_streamed_sound_file(&mut self, file: Ptr, dir: &str, name: &str) -> Result<()> {
        let _ = (file, dir, name);
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
    load_asset_at_durable_slot(s, ty, slot, links).map(|loaded| loaded.is_some())
}

pub fn load_asset_at_durable_slot(
    s: &mut ZoneStream<'_>,
    ty: AssetType,
    slot: Ptr,
    links: &mut dyn AssetLinkSink,
) -> Result<Option<Option<Ptr>>> {
    if s.wire_format() == Iw5WireFormat::X64 && ty == AssetType::SndDriverGlobals {
        return Ok(None);
    }
    let pointer = s.ptr_at(slot, 0)?;
    s.push(XFILE_BLOCK_TEMP)?;
    let result = match pointer {
        ZonePtr::Null => Ok(None),
        ZonePtr::Offset(target) => {
            s.note_offset(target);
            links.alias(ty, slot, target)?;
            if ty == AssetType::Attachment {
                s.alias_attachment_name(slot, target);
            }
            Ok(None)
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
            if ty == AssetType::XModelSurfs {
                let surfaces = s.take_latest_xmodel_surfs_array();
                let name = s.take_latest_xmodel_surfs_name();
                links.remember_xmodel_surfaces(slot, surfaces, name);
                if let Some(insert_slot) = insert_slot {
                    links.remember_xmodel_surfaces(insert_slot, surfaces, name);
                }
                s.record_xmodel_surfs_array(surfaces);
                s.record_xmodel_surfs_name(name);
            }
            if ty == AssetType::Attachment {
                s.commit_attachment_name(slot, insert_slot);
            }
            links.loaded(s, ty, slot, insert_slot)?;
            Ok(Some(insert_slot))
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

pub(crate) fn begin_temp_body(s: &mut ZoneStream<'_>, slot: Ptr) -> Result<bool> {
    s.push(XFILE_BLOCK_TEMP)?;
    s.begin_body(slot)
}

pub(crate) fn always_alloc(s: &mut ZoneStream<'_>, slot: Ptr) -> Result<bool> {
    match s.ptr_at(slot, 0)? {
        ZonePtr::Null => Ok(false),
        ZonePtr::Offset(p) => {
            s.note_offset(p);
            Ok(false)
        }
        ZonePtr::Following | ZonePtr::Insert => Ok(true),
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

pub(super) fn material_name_at(
    s: &ZoneStream<'_>,
    links: &dyn AssetLinkSink,
    slot: Ptr,
) -> Option<Ptr> {
    match s.ptr_at(slot, 0).ok()? {
        ZonePtr::Null => None,
        ZonePtr::Offset(target) => links
            .material_name_ptr(s.resolve_alias(target))
            .or_else(|| links.material_name_ptr(target))
            .or_else(|| links.material_name_ptr(slot)),
        ZonePtr::Following | ZonePtr::Insert => links.material_name_ptr(slot),
    }
}
