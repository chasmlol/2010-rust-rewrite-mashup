use asset_iw4::size as sz;

use crate::asset_type::AssetType;
use crate::zone::{
    FxEffectDefGeometry, FxImpactTableGeometry, PhysPresetGeometry, Ptr, Result,
    XAnimPartsGeometry, XFILE_BLOCK_TEMP, XFILE_BLOCK_VIRTUAL, ZoneError, ZonePtr, ZoneStream,
};

mod clipmap;
mod fx;
mod gfxworld;
mod material;
mod menu;
mod model;
mod sound;
mod vehicle;
mod weapon;
mod world;
mod xanim;

pub use menu::{MenuChoiceValue, MenuDefCapture, MenuItemLayout, MenuRectCapture, MenuScriptKind};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphCapture {
    pub letter: u16,
    pub x0: i8,
    pub y0: i8,
    pub dx: u8,
    pub pixel_width: u8,
    pub pixel_height: u8,
    pub s0: f32,
    pub t0: f32,
    pub s1: f32,
    pub t1: f32,
}

impl GlyphCapture {
    pub fn from_row(row: &[u8; sz::GLYPH]) -> Self {
        Self {
            letter: u16::from_le_bytes([row[0], row[1]]),
            x0: row[2] as i8,
            y0: row[3] as i8,
            dx: row[4],
            pixel_width: row[5],
            pixel_height: row[6],
            s0: f32::from_bits(u32::from_le_bytes([row[8], row[9], row[10], row[11]])),
            t0: f32::from_bits(u32::from_le_bytes([row[12], row[13], row[14], row[15]])),
            s1: f32::from_bits(u32::from_le_bytes([row[16], row[17], row[18], row[19]])),
            t1: f32::from_bits(u32::from_le_bytes([row[20], row[21], row[22], row[23]])),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FontCapture<'a> {
    pub name: &'a str,

    pub pixel_height: i32,
    pub material: &'a str,
    pub glow_material: &'a str,
    pub glyphs: &'a [u8],
}

pub fn load_asset_body(s: &mut ZoneStream<'_>, ty: AssetType) -> Result<()> {
    load_asset_body_observed(s, ty, &mut IgnoreAssetLinks)
}

fn load_asset_body_observed(
    s: &mut ZoneStream<'_>,
    ty: AssetType,
    links: &mut dyn AssetLinkSink,
) -> Result<()> {
    use AssetType::*;
    if s.wire_format() == crate::Iw4WireFormat::X64 && ty == SndDriverGlobals {
        return Ok(());
    }
    if s.wire_format() == crate::Iw4WireFormat::X64
        && !matches!(
            ty,
            Weapon
                | ComWorld
                | FxWorld
                | GameWorldMp
                | GameWorldSp
                | MapEnts
                | AddonMapEnts
                | GfxWorld
                | ClipMapMp
                | ClipMapSp
                | Vehicle
                | Tracer
                | LightDef
                | ImpactFx
                | Fx
                | XModel
                | PhysPreset
                | PhysCollmap
                | Sound
                | LoadedSound
                | XAnimParts
                | StringTable
                | StructuredDataDef
                | LeaderBoard
                | SoundCurve
                | MenuList
                | Menu
                | Localize
                | RawFile
                | Font
                | TechniqueSet
                | Material
                | Image
                | PixelShader
                | VertexShader
                | VertexDecl
        )
    {
        return Err(ZoneError::UnsupportedAssetLayout {
            format: s.wire_format(),
            ty,
        });
    }
    match ty {
        StringTable => load_string_table(s, links),
        LeaderBoard => load_leaderboard_def(s),
        StructuredDataDef => load_structured_data_def_set(s),
        RawFile => load_raw_file(s, links),
        Localize => load_localize(s, links),
        Font => load_font(s, links),
        Sound => sound::load_sound(s, links),
        LoadedSound => sound::load_loaded_sound(s, links),
        SoundCurve => sound::load_snd_curve(s, links),
        TechniqueSet => material::load_technique_set(s, links),
        Material => material::load_material(s, links),
        Image => material::load_image(s),
        PixelShader | VertexShader => material::load_shader(s),
        VertexDecl => load_vertex_decl(s),
        PhysPreset => load_phys_preset(s),
        PhysCollmap => material::load_phys_collmap(s, links),
        XAnimParts => xanim::load_xanim(s, links),
        XModel => model::load_xmodel(s, links),
        Fx => fx::load_fx(s, links),
        ComWorld => world::load_comworld(s),
        FxWorld => world::load_fxworld(s, links),
        LightDef => world::load_light_def(s, links),
        GfxWorld => gfxworld::load_gfxworld(s, links),
        GameWorldMp => world::load_gameworld_mp(s),
        GameWorldSp => world::load_gameworld_sp(s),
        MapEnts => world::load_mapents(s),
        AddonMapEnts => world::load_addonmapents(s),
        ClipMapMp | ClipMapSp => clipmap::load_clipmap(s, links),
        ImpactFx => load_impact_fx(s, links),
        Weapon => weapon::load_weapon(s, links),
        Tracer => weapon::load_tracer(s, links),
        MenuList => menu::load_menu_list(s, links),
        Menu => menu::load_menu(s, links),
        Vehicle => vehicle::load_vehicle(s, links),
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

    fn capture_raw_file(&mut self, name: &str, data: &[u8], zlib_compressed: bool) -> Result<()> {
        let _ = (name, data, zlib_compressed);
        Ok(())
    }

    fn capture_string_table(&mut self, s: &ZoneStream<'_>, header: Ptr) -> Result<()> {
        let _ = (s, header);
        Ok(())
    }

    fn capture_menu_list(&mut self, name: &str, count: i32) -> Result<()> {
        let _ = (name, count);
        Ok(())
    }

    fn capture_menu(&mut self, name: &str) -> Result<()> {
        let _ = name;
        Ok(())
    }

    fn capture_menu_item(
        &mut self,
        menu: &str,
        item: &str,
        text: &[u8],
        owner_draw: i32,
        item_type: i32,
    ) -> Result<()> {
        let _ = (menu, item, text, owner_draw, item_type);
        Ok(())
    }

    fn capture_menu_def(&mut self, rec: &MenuDefCapture<'_>) -> Result<()> {
        let _ = rec;
        Ok(())
    }

    fn capture_menu_visible_exp(&mut self, menu: &str, dump: &str) -> Result<()> {
        let _ = (menu, dump);
        Ok(())
    }

    fn capture_menu_float_exp(&mut self, menu: &str, key: u32, dump: &str) -> Result<()> {
        let _ = (menu, key, dump);
        Ok(())
    }

    fn capture_menu_item_layout(&mut self, rec: &MenuItemLayout<'_>) -> Result<()> {
        let _ = rec;
        Ok(())
    }

    fn capture_item_visible_exp(&mut self, menu: &str, item: &str, dump: &str) -> Result<()> {
        let _ = (menu, item, dump);
        Ok(())
    }

    fn capture_item_float_exp(
        &mut self,
        menu: &str,
        item: &str,
        key: u32,
        dump: &str,
    ) -> Result<()> {
        let _ = (menu, item, key, dump);
        Ok(())
    }

    fn capture_item_disabled_exp(&mut self, menu: &str, item: &str, dump: &str) -> Result<()> {
        let _ = (menu, item, dump);
        Ok(())
    }

    fn capture_item_text_exp(&mut self, menu: &str, item: &str, dump: &str) -> Result<()> {
        let _ = (menu, item, dump);
        Ok(())
    }

    fn capture_item_choice(
        &mut self,
        menu: &str,
        item: &str,
        label: &str,
        value: MenuChoiceValue<'_>,
    ) -> Result<()> {
        let _ = (menu, item, label, value);
        Ok(())
    }

    fn capture_item_material_exp(&mut self, menu: &str, item: &str, dump: &str) -> Result<()> {
        let _ = (menu, item, dump);
        Ok(())
    }

    fn capture_menu_script(
        &mut self,
        menu: &str,
        item: &str,
        kind: MenuScriptKind,
        script: &str,
    ) -> Result<()> {
        let _ = (menu, item, kind, script);
        Ok(())
    }

    fn capture_menu_set_local_var(
        &mut self,
        menu: &str,
        item: &str,
        kind: MenuScriptKind,
        var_kind: i32,
        name: &str,
        expr: &str,
    ) -> Result<()> {
        let _ = (menu, item, kind, var_kind, name, expr);
        Ok(())
    }

    fn begin_menu_event_branch(
        &mut self,
        menu: &str,
        item: &str,
        kind: MenuScriptKind,
        condition: Option<&str>,
    ) -> Result<()> {
        let _ = (menu, item, kind, condition);
        Ok(())
    }

    fn end_menu_event_branch(
        &mut self,
        menu: &str,
        item: &str,
        kind: MenuScriptKind,
    ) -> Result<()> {
        let _ = (menu, item, kind);
        Ok(())
    }

    fn begin_menu_script_set(&mut self, body: Ptr) {
        let _ = body;
    }

    fn end_menu_script_set(&mut self, insert_slot: Option<Ptr>) {
        let _ = insert_slot;
    }

    fn reuse_menu_script_set(
        &mut self,
        body: Ptr,
        menu: &str,
        item: &str,
        kind: MenuScriptKind,
    ) -> Result<()> {
        let _ = (body, menu, item, kind);
        Ok(())
    }

    fn capture_xanim(&mut self, s: &ZoneStream<'_>, geometry: XAnimPartsGeometry) -> Result<()> {
        let _ = (s, geometry);
        Ok(())
    }

    fn capture_fx(&mut self, s: &ZoneStream<'_>, geometry: FxEffectDefGeometry) -> Result<()> {
        let _ = (s, geometry);
        Ok(())
    }

    fn capture_impact_fx(
        &mut self,
        s: &ZoneStream<'_>,
        geometry: FxImpactTableGeometry,
    ) -> Result<()> {
        let _ = (s, geometry);
        Ok(())
    }

    fn capture_tracer(
        &mut self,
        s: &ZoneStream<'_>,
        geometry: crate::TracerDefGeometry,
    ) -> Result<()> {
        let _ = (s, geometry);
        Ok(())
    }

    fn capture_fx_glass_def(
        &mut self,
        def_index: usize,
        material: &str,
        material_shattered: &str,
    ) -> Result<()> {
        let _ = (def_index, material, material_shattered);
        Ok(())
    }

    fn remember_xmodel_surfaces(&mut self, _slot: Ptr, _surfaces: Ptr) {}
    fn remember_xmodel_surface_name(&mut self, _slot: Ptr, _name: Ptr) {}
    fn xmodel_surface_name(&self, _slot: Ptr) -> Option<Ptr> {
        None
    }

    fn xmodel_surfaces(&self, _slot: Ptr) -> Option<Ptr> {
        None
    }

    fn xmodel_name_ptr(&self, _slot: Ptr) -> Option<Ptr> {
        None
    }

    fn remember_xmodel_name(&mut self, _slot: Ptr, _insert_slot: Option<Ptr>, _name: Ptr) {}

    fn linked_asset_name(&self, _slot: Ptr) -> Option<&str> {
        None
    }

    fn capture_font(&mut self, rec: &FontCapture<'_>) -> Result<()> {
        let _ = rec;
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

pub fn load_asset_at_with<F>(
    s: &mut ZoneStream<'_>,
    ty: AssetType,
    slot: Ptr,
    inspect: F,
) -> Result<()>
where
    F: FnOnce(&ZoneStream<'_>, AssetType) -> Result<()>,
{
    if s.wire_format() == crate::Iw4WireFormat::X64 && ty == AssetType::SndDriverGlobals {
        return Ok(());
    }
    s.push(XFILE_BLOCK_TEMP)?;
    if s.begin_body(slot)? {
        load_asset_body(s, ty)?;
        inspect(s, ty)?;
    }
    s.pop()
}

pub fn load_asset_at_observed(
    s: &mut ZoneStream<'_>,
    ty: AssetType,
    slot: Ptr,
    links: &mut dyn AssetLinkSink,
) -> Result<bool> {
    if s.wire_format() == crate::Iw4WireFormat::X64 && ty == AssetType::SndDriverGlobals {
        return Ok(false);
    }
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
            if ty == AssetType::XModel {
                if let Some(name) = s.xmodel().and_then(|g| g.name) {
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

pub(crate) fn asset_ptr_at_linked(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    ty: AssetType,
    slot: Ptr,
) -> Result<bool> {
    load_asset_at_observed(s, ty, slot, links)
}

pub(crate) fn follow_name(s: &mut ZoneStream<'_>, p: Ptr, field: usize) -> Result<()> {
    s.follow_string(p, field)?;
    Ok(())
}

fn load_localize(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::LOCALIZE_ENTRY, 16))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    let value = follow_name_ptr(s, p, 0)?;
    let name = follow_name_ptr(s, p, s.pointer_bytes())?;
    if let (Some(name), Some(value)) = (name, value) {
        let name = s.cstr(name)?;
        let value = s.cstr_bytes(value)?;
        links.capture_localize(name, value)?;
    }
    s.pop()
}

fn follow_name_ptr(s: &mut ZoneStream<'_>, p: Ptr, field: usize) -> Result<Option<Ptr>> {
    s.follow_string(p, field)
}

fn load_font(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::FONT, 40))?;
    let pixel_height = s.i32_at(p, s.layout(4, 8))?;
    let glyph_count = s.i32_at(p, s.layout(8, 12))?.max(0) as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    let mut name_buf = [0u8; 128];
    let name = copy_cstr_field(s, p, 0, &mut name_buf);

    let fresh_mat = asset_ptr_at_linked(s, links, AssetType::Material, p.at(s.layout(12, 16)))?;
    let mut mat_buf = [0u8; 128];
    let material = copy_linked_material(s, links, p.at(s.layout(12, 16)), fresh_mat, &mut mat_buf);

    let fresh_glow = asset_ptr_at_linked(s, links, AssetType::Material, p.at(s.layout(16, 24)))?;
    let mut glow_buf = [0u8; 128];
    let glow_material =
        copy_linked_material(s, links, p.at(s.layout(16, 24)), fresh_glow, &mut glow_buf);

    let glyphs_ptr = s.plain_array(p, s.layout(20, 32), 4, sz::GLYPH, glyph_count)?;
    let glyphs = match glyphs_ptr {
        Some(gp) if glyph_count > 0 => s.slice_at(gp, 0, glyph_count.saturating_mul(sz::GLYPH))?,
        _ => &[],
    };
    links.capture_font(&FontCapture {
        name,
        pixel_height,
        material,
        glow_material,
        glyphs,
    })?;
    s.pop()
}

fn copy_cstr_field<'a>(s: &ZoneStream<'_>, p: Ptr, field: usize, buf: &'a mut [u8]) -> &'a str {
    let raw = match s.ptr_at(p, field) {
        Ok(ZonePtr::Offset(q)) => s.cstr(s.resolve_alias(q)).unwrap_or(""),
        _ => "",
    };
    let n = raw.len().min(buf.len());
    buf[..n].copy_from_slice(raw.as_bytes());
    core::str::from_utf8(&buf[..n]).unwrap_or("")
}

pub(super) fn copy_linked_material<'a>(
    s: &ZoneStream<'_>,
    links: &dyn AssetLinkSink,
    slot: Ptr,
    fresh: bool,
    buf: &'a mut [u8],
) -> &'a str {
    let raw = if fresh {
        s.latest_material()
            .and_then(|g| g.name)
            .and_then(|n| s.cstr(n).ok())
            .unwrap_or("")
    } else {
        links
            .linked_asset_name(slot)
            .or_else(|| match s.ptr_at(slot, 0).ok() {
                Some(ZonePtr::Offset(p)) => links.linked_asset_name(s.resolve_alias(p)),
                _ => None,
            })
            .or_else(|| offset_material_name(s, slot))
            .unwrap_or("")
    };
    let n = raw.len().min(buf.len());
    buf[..n].copy_from_slice(raw.as_bytes());
    core::str::from_utf8(&buf[..n]).unwrap_or("")
}

fn offset_material_name<'a>(s: &'a ZoneStream<'_>, slot: Ptr) -> Option<&'a str> {
    let mat = match s.ptr_at(slot, 0).ok()? {
        ZonePtr::Offset(p) => s.resolve_alias(p),
        _ => return None,
    };
    match s.ptr_at(mat, 0).ok()? {
        ZonePtr::Offset(n) => s.cstr(s.resolve_alias(n)).ok().filter(|n| !n.is_empty()),
        _ => None,
    }
}

fn load_impact_fx(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(8, 16))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let mut entries_array = None;
    if s.begin_body(p.at(s.layout(4, 8)))? {
        let arr = s.alloc_load(4, s.layout(sz::FX_IMPACT_ENTRY, 280) * 15)?;
        entries_array = Some(arr);
        for i in 0..15 {
            let e = arr.at(i * s.layout(sz::FX_IMPACT_ENTRY, 280));
            for j in 0..sz::SURF_TYPE_NUM {
                asset_ptr_at_linked(s, links, AssetType::Fx, e.at(j * s.pointer_bytes()))?;
            }
            for j in 0..4 {
                asset_ptr_at_linked(
                    s,
                    links,
                    AssetType::Fx,
                    e.at(s.layout(124, 248) + j * s.pointer_bytes()),
                )?;
            }
        }
    }

    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };
    if let Some(entries) = entries_array {
        links.capture_impact_fx(
            s,
            FxImpactTableGeometry {
                header: p,
                name,
                entries,
                row_count: 15,
            },
        )?;
    }
    s.pop()
}

fn load_raw_file(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::RAW_FILE, 24))?;
    let compressed_len = s.i32_at(p, s.layout(4, 8))?;
    let len = s.i32_at(p, s.layout(8, 12))?;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let zlib_compressed = compressed_len > 0;
    let bytes = if zlib_compressed {
        compressed_len as usize
    } else if len >= 0 {
        (len as usize).saturating_add(1)
    } else {
        0
    };
    let data_ptr = s.plain_array(p, s.layout(12, 16), 1, 1, bytes)?;
    if let Some(px) = data_ptr {
        let raw = s.slice_at(px, 0, bytes).unwrap_or(&[]);
        let name = match s.ptr_at(p, 0).ok() {
            Some(ZonePtr::Offset(np)) => s.cstr(s.resolve_alias(np)).ok().unwrap_or(""),
            _ => "",
        };
        if !name.is_empty() {
            links.capture_raw_file(name, raw, zlib_compressed)?;
        }
    }
    s.pop()
}

fn load_string_table(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::STRING_TABLE, 24))?;
    let columns = s.i32_at(p, s.layout(4, 8))? as usize;
    let rows = s.i32_at(p, s.layout(8, 12))? as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let cells = columns.saturating_mul(rows);

    if let Some(arr) = s.follow_array(
        p,
        s.layout(12, 16),
        4,
        s.layout(sz::STRING_TABLE_CELL, 16),
        cells,
    )? {
        for i in 0..cells {
            s.follow_string(arr.at(i * s.layout(sz::STRING_TABLE_CELL, 16)), 0)?;
        }
    }
    links.capture_string_table(s, p)?;
    s.pop()
}

fn load_leaderboard_def(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::LEADERBOARD_DEF, 32))?;
    let column_count = s.i32_at(p, s.layout(8, 12))?.max(0) as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    if s.begin_body(p.at(s.layout(20, 24)))? {
        let columns = s.alloc_load(4, s.layout(sz::LB_COLUMN_DEF, 48) * column_count)?;
        for index in 0..column_count {
            let column = columns.at(index * s.layout(sz::LB_COLUMN_DEF, 48));
            s.follow_string(column, 0)?;
            s.follow_string(column, s.layout(16, 24))?;
        }
    }
    s.pop()
}

fn load_structured_data_def_set(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::STRUCTURED_DATA_DEF_SET, 24))?;
    let def_count = s.i32_at(p, s.layout(4, 8))?.max(0) as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    if let Some(defs) = s.follow_array(
        p,
        s.layout(8, 16),
        4,
        s.layout(sz::STRUCTURED_DATA_DEF, 88),
        def_count,
    )? {
        for index in 0..def_count {
            load_structured_data_def(s, defs.at(index * s.layout(sz::STRUCTURED_DATA_DEF, 88)))?;
        }
    }
    s.record_structured_data_def_set(p);
    s.pop()
}

fn load_structured_data_def(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    let enum_count = s.i32_at(p, 8)?.max(0) as usize;
    if let Some(enums) = s.follow_array(
        p,
        s.layout(12, 16),
        4,
        s.layout(sz::STRUCTURED_DATA_ENUM, 16),
        enum_count,
    )? {
        for index in 0..enum_count {
            let row = enums.at(index * s.layout(sz::STRUCTURED_DATA_ENUM, 16));
            let entry_count = s.i32_at(row, 0)?.max(0) as usize;
            if let Some(entries) = s.follow_array(
                row,
                8,
                4,
                s.layout(sz::STRUCTURED_DATA_ENUM_ENTRY, 16),
                entry_count,
            )? {
                for entry_index in 0..entry_count {
                    s.follow_string(
                        entries.at(entry_index * s.layout(sz::STRUCTURED_DATA_ENUM_ENTRY, 16)),
                        0,
                    )?;
                }
            }
        }
    }

    let struct_count = s.i32_at(p, s.layout(16, 24))?.max(0) as usize;
    if let Some(structs) = s.follow_array(
        p,
        s.layout(20, 32),
        4,
        s.layout(sz::STRUCTURED_DATA_STRUCT, 24),
        struct_count,
    )? {
        for index in 0..struct_count {
            let row = structs.at(index * s.layout(sz::STRUCTURED_DATA_STRUCT, 24));
            let property_count = s.i32_at(row, 0)?.max(0) as usize;
            if let Some(properties) = s.follow_array(
                row,
                s.layout(4, 8),
                4,
                s.layout(sz::STRUCTURED_DATA_STRUCT_PROPERTY, 24),
                property_count,
            )? {
                for property_index in 0..property_count {
                    s.follow_string(
                        properties
                            .at(property_index * s.layout(sz::STRUCTURED_DATA_STRUCT_PROPERTY, 24)),
                        0,
                    )?;
                }
            }
        }
    }

    let indexed_count = s.i32_at(p, s.layout(24, 40))?.max(0) as usize;
    s.plain_array(
        p,
        s.layout(28, 48),
        4,
        sz::STRUCTURED_DATA_ARRAY,
        indexed_count,
    )?;
    let enumed_count = s.i32_at(p, s.layout(32, 56))?.max(0) as usize;
    s.plain_array(
        p,
        s.layout(36, 64),
        4,
        sz::STRUCTURED_DATA_ARRAY,
        enumed_count,
    )?;
    Ok(())
}

fn load_vertex_decl(s: &mut ZoneStream<'_>) -> Result<()> {
    use asset_iw4::vertex_decl as vd;

    let p = s.alloc_load(4, s.layout(sz::VERTEX_DECL, 176))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;

    let stream_count = s.u8_at(p, s.layout(vd::OFFSET_STREAM_COUNT, 8))?;
    let has_optional_source = s.u8_at(p, s.layout(vd::OFFSET_HAS_OPTIONAL_SOURCE, 9))?;
    let mut routing = [[0u8; 2]; vd::ROUTING_COUNT];
    for (index, pair) in routing.iter_mut().enumerate() {
        let offset = s.layout(vd::OFFSET_ROUTING_DATA, 16) + index * 2;
        pair[0] = s.u8_at(p, offset)?;
        pair[1] = s.u8_at(p, offset + 1)?;
    }

    follow_name(s, p, 0)?;
    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };
    s.record_vertex_decl(crate::zone::VertexDeclGeometry {
        name,
        stream_count,
        has_optional_source,
        routing,
    });

    s.pop()
}

fn load_phys_preset(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::PHYS_PRESET, 56))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    let name = s.follow_string(p, 0)?;

    let snd_alias_prefix = s.follow_string(p, s.layout(28, 32))?;
    s.record_phys_preset(PhysPresetGeometry {
        header: p,
        name,
        snd_alias_prefix,
    });
    s.pop()
}

pub(crate) fn begin_temp_body(s: &mut ZoneStream<'_>, slot: Ptr) -> Result<bool> {
    s.push(XFILE_BLOCK_TEMP)?;
    s.begin_body(slot)
}
