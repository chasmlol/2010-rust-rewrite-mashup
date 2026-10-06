use asset_iw4::size as sz;

use super::{AssetLinkSink, asset_ptr_at, asset_ptr_at_linked, follow_name};
use crate::asset_type::AssetType;
use crate::zone::{Ptr, Result, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream};

#[derive(Clone, Copy, Debug, Default)]
pub struct MenuRectCapture {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub horz_align: u8,
    pub vert_align: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuScriptKind {
    OnOpen,
    OnClose,
    OnCloseRequest,
    OnEsc,
    MouseEnterText,
    MouseExitText,
    MouseEnter,
    MouseExit,
    Action,
    Accept,
    OnFocus,
    LeaveFocus,
    ExecKey,
}

#[derive(Clone, Copy, Debug)]
pub struct MenuDefCapture<'a> {
    pub name: &'a str,
    pub sound_name: &'a str,
    pub window_background: &'a str,
    pub expr_dvars: &'a str,
    pub fullscreen: i32,
    pub static_flags: i32,
    pub item_count: i32,
    pub focus_color: [f32; 4],

    pub rect: MenuRectCapture,
}

#[derive(Clone, Copy, Debug)]
pub enum MenuChoiceValue<'a> {
    Str(&'a str),
    Float(f32),
}

#[derive(Clone, Copy, Debug)]
pub struct MenuItemLayout<'a> {
    pub menu: &'a str,
    pub name: &'a str,
    pub text: &'a [u8],
    pub item_type: i32,

    pub style: i32,
    pub owner_draw: i32,
    pub rect: MenuRectCapture,
    pub fore_color: [f32; 4],

    pub back_color: [f32; 4],

    pub glow_color: [f32; 4],
    pub text_scale: f32,

    pub font_enum: i32,

    pub text_align_mode: i32,

    pub text_align_x: f32,

    pub text_align_y: f32,

    pub text_style: i32,
    pub game_msg_window_index: i32,
    pub background: &'a str,
    pub focus_sound: &'a str,

    pub dvar: &'a str,

    pub dvar_test: &'a str,

    pub enable_dvar: &'a str,

    pub local_var: &'a str,

    pub bg_ptr: u8,

    pub vis_ptr: u8,

    pub mat_ptr: u8,

    pub sound_ptr: u8,

    pub mouse_enter_ptr: u8,

    pub on_focus_ptr: u8,
    pub static_flags: i32,
    pub dvar_flags: i32,
}

fn name_at<'s>(s: &'s ZoneStream<'_>, p: Ptr) -> Result<Option<&'s str>> {
    match s.ptr_at(p, 0)? {
        ZonePtr::Offset(q) => Ok(s.cstr(s.resolve_alias(q)).ok()),
        _ => Ok(None),
    }
}

fn string_at<'s>(s: &'s ZoneStream<'_>, p: Ptr, field: usize) -> &'s str {
    match s.ptr_at(p, field) {
        Ok(ZonePtr::Offset(q)) => s.cstr(s.resolve_alias(q)).unwrap_or(""),
        _ => "",
    }
}

fn asset_name_at<'s>(s: &'s ZoneStream<'_>, p: Ptr, field: usize) -> &'s str {
    match s.ptr_at(p, field) {
        Ok(ZonePtr::Offset(header)) => string_at(s, s.resolve_alias(header), 0),
        _ => "",
    }
}

fn ptr_kind(s: &ZoneStream<'_>, p: Ptr, field: usize) -> u8 {
    match s.ptr_at(p, field) {
        Ok(ZonePtr::Null) => 0,
        Ok(ZonePtr::Following) => 1,
        Ok(ZonePtr::Insert) => 2,
        Ok(ZonePtr::Offset(_)) => 3,
        Err(_) => 255,
    }
}

fn cstr_opt<'s>(s: &'s ZoneStream<'_>, p: Option<Ptr>) -> &'s str {
    let Some(p) = p else {
        return "";
    };
    s.cstr(s.resolve_alias(p)).unwrap_or("")
}

fn name_after_link<'s>(
    s: &'s ZoneStream<'_>,
    parent: Ptr,
    field: usize,
    fresh: bool,
    latest_name: Option<Ptr>,
) -> &'s str {
    if fresh {
        cstr_opt(s, latest_name)
    } else {
        asset_name_at(s, parent, field)
    }
}

fn read_rect(s: &ZoneStream<'_>, p: Ptr, off: usize) -> Result<MenuRectCapture> {
    Ok(MenuRectCapture {
        x: s.f32_at(p, off)?,
        y: s.f32_at(p, off + 4)?,
        w: s.f32_at(p, off + 8)?,
        h: s.f32_at(p, off + 12)?,
        horz_align: s.u8_at(p, off + 16)?,
        vert_align: s.u8_at(p, off + 17)?,
    })
}

fn read_vec4(s: &ZoneStream<'_>, p: Ptr, off: usize) -> Result<[f32; 4]> {
    Ok([
        s.f32_at(p, off)?,
        s.f32_at(p, off + 4)?,
        s.f32_at(p, off + 8)?,
        s.f32_at(p, off + 12)?,
    ])
}

fn copy_into<'a>(src: &str, buf: &'a mut [u8]) -> &'a str {
    let raw = src.as_bytes();
    let n = raw.len().min(buf.len());
    buf[..n].copy_from_slice(&raw[..n]);
    core::str::from_utf8(&buf[..n]).unwrap_or("")
}

fn copy_bytes<'a>(src: &[u8], buf: &'a mut [u8]) -> &'a [u8] {
    let n = src.len().min(buf.len());
    buf[..n].copy_from_slice(&src[..n]);
    &buf[..n]
}

fn copy_ascii_name<'a>(s: &ZoneStream<'_>, p: Ptr, buf: &'a mut [u8]) -> Result<&'a str> {
    let raw = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(q) => s.cstr_bytes(s.resolve_alias(q)).unwrap_or(b""),
        _ => b"",
    };
    let n = raw.len().min(buf.len());
    buf[..n].copy_from_slice(&raw[..n]);
    Ok(core::str::from_utf8(&buf[..n]).unwrap_or(""))
}

pub(super) fn load_menu_list(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::MENU_LIST, 24))?;
    let count = s.i32_at(p, s.layout(4, 8))? as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    if let Some(name) = name_at(s, p)? {
        links.capture_menu_list(name, count as i32)?;
    }
    if s.begin_body(p.at(s.layout(8, 16)))? {
        let arr = s.alloc_load(4, s.pointer_bytes() * count)?;
        for i in 0..count {
            asset_ptr_at(s, links, AssetType::Menu, arr.at(i * s.pointer_bytes()))?;
        }
    }
    s.pop()
}

pub(super) fn load_menu(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::MENU_DEF, 488))?;
    let item_count = s.i32_at(p, s.layout(172, 188))? as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;

    let mut cache = StatementCache::default();
    let mut dvar_buf = [0u8; 4096];
    let dvar_len = {
        let mut dump = ExpDump {
            buf: &mut dvar_buf,
            used: 0,
        };
        if s.begin_body(p.at(s.layout(396, 480)))? {
            let support = load_expression_supporting_data(s, &mut cache)?;
            s.fixup_slot(p.at(s.layout(396, 480)), support)?;
        }

        if let ZonePtr::Offset(support) = s.ptr_at(p, s.layout(396, 480))? {
            dump_static_dvars(s, s.resolve_alias(support), &mut dump)?;
        }
        dump.used
    };
    let expr_dvars = core::str::from_utf8(&dvar_buf[..dvar_len]).unwrap_or("");

    let mut window_bg_buf = [0u8; 128];
    let window_background = load_window_def(s, links, p, &mut window_bg_buf)?;
    if let Some(name) = name_at(s, p)? {
        links.capture_menu(name)?;
    }

    let mut menu_buf = [0u8; 128];
    let menu_name = copy_ascii_name(s, p, &mut menu_buf)?;
    s.follow_string(p, s.layout(164, 176))?;
    let fullscreen = s.i32_at(p, s.layout(168, 184)).unwrap_or(0);

    for field in [204, 212, 208, 216] {
        let kind = match field {
            204 => MenuScriptKind::OnOpen,
            212 => MenuScriptKind::OnClose,
            208 => MenuScriptKind::OnCloseRequest,
            _ => MenuScriptKind::OnEsc,
        };
        follow_handler_set(
            s,
            links,
            p,
            s.layout(field, 224 + (field - 204) * 2),
            menu_name,
            "",
            kind,
            &mut cache,
        )?;
    }
    if s.begin_body(p.at(s.layout(220, 256)))? {
        load_item_key_handler(s, links, menu_name, &mut cache)?;
    }
    let mut vis_buf = [0u8; ITEM_STATEMENT_DUMP];
    let mut vis_dump = "";
    if s.begin_body(p.at(s.layout(224, 264)))? {
        vis_dump = dump_item_statement(s, &mut vis_buf, &mut cache)?;
    }
    s.follow_string(p, s.layout(228, 272))?;
    s.follow_string(p, s.layout(232, 280))?;
    let rect = read_rect(s, p, s.layout(4, 8))?;
    links.capture_menu_def(&MenuDefCapture {
        name: menu_name,
        sound_name: string_at(s, p, s.layout(232, 280)),
        window_background,
        expr_dvars,
        fullscreen,
        static_flags: s.i32_at(p, s.layout(68, 76))?,
        item_count: item_count as i32,
        focus_color: read_vec4(s, p, s.layout(240, 292)).unwrap_or([1.0; 4]),
        rect,
    })?;
    if !vis_dump.is_empty() {
        links.capture_menu_visible_exp(menu_name, vis_dump)?;
    }
    for field in [256, 260, 264, 268] {
        if s.begin_body(p.at(s.layout(field, 312 + (field - 256) * 2)))? {
            let mut buf = [0u8; ITEM_STATEMENT_DUMP];
            let dump = dump_item_statement(s, &mut buf, &mut cache)?;
            let key = ((field - 256) / 4) as u32;
            links.capture_menu_float_exp(menu_name, key, dump)?;
        }
    }
    for field in [272, 276] {
        if s.begin_body(p.at(s.layout(field, 312 + (field - 256) * 2)))? {
            load_statement(s, &mut cache)?;
        }
    }

    if s.begin_body(p.at(s.layout(280, 360)))? {
        let arr = s.alloc_load(4, s.pointer_bytes() * item_count)?;
        for i in 0..item_count {
            if s.begin_body(arr.at(i * s.pointer_bytes()))? {
                load_item_def(s, links, menu_name, &mut cache)?;
            }
        }
    }

    s.pop()
}

fn load_window_def<'a>(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    p: Ptr,
    bg_buf: &'a mut [u8],
) -> Result<&'a str> {
    follow_name(s, p, 0)?;
    s.follow_string(p, s.layout(44, 48))?;
    let fresh = asset_ptr_at_linked(s, links, AssetType::Material, p.at(s.layout(160, 168)))?;
    let latest = s.latest_material().and_then(|g| g.name);
    let from_stream = name_after_link(s, p, s.layout(160, 168), fresh, latest);
    let name = links
        .linked_asset_name(p.at(s.layout(160, 168)))
        .unwrap_or(from_stream);
    Ok(copy_into(name, bg_buf))
}

fn load_expression_supporting_data(
    s: &mut ZoneStream<'_>,
    cache: &mut StatementCache,
) -> Result<Ptr> {
    let p = s.alloc_load(4, s.layout(sz::EXPRESSION_SUPPORTING_DATA, 48))?;

    let fn_count = s.i32_at(p, 0)? as usize;
    if let Some(arr) = s.follow_array(p, s.layout(4, 8), 4, s.pointer_bytes(), fn_count)? {
        for i in 0..fn_count {
            if s.begin_body(arr.at(i * s.pointer_bytes()))? {
                let mut buf = [0u8; 1024];
                let mut inner = ExpDump {
                    buf: &mut buf,
                    used: 0,
                };
                let stmt = load_statement_rec(s, &mut inner, cache)?;
                s.fixup_slot(arr.at(i * s.pointer_bytes()), stmt)?;
                cache.insert(stmt, inner.as_str());
                s.expr_stmt_insert(stmt, inner.as_str());
            }
        }
    }

    let dvar_count = s.i32_at(p, s.layout(8, 16))? as usize;
    if let Some(arr) = s.follow_array(p, s.layout(12, 24), 4, s.pointer_bytes(), dvar_count)? {
        for i in 0..dvar_count {
            if s.begin_body(arr.at(i * s.pointer_bytes()))? {
                let d = s.alloc_load(4, s.layout(sz::STATIC_DVAR, 16))?;
                s.fixup_slot(arr.at(i * s.pointer_bytes()), d)?;
                s.follow_string(d, s.layout(4, 8))?;
            }
        }
    }

    let str_count = s.i32_at(p, s.layout(16, 32))? as usize;
    follow_string_array(s, p, s.layout(20, 40), str_count)?;
    Ok(p)
}

fn dump_static_dvars(s: &ZoneStream<'_>, support: Ptr, dump: &mut ExpDump<'_>) -> Result<()> {
    let count = s.i32_at(support, s.layout(8, 16))?.max(0) as usize;
    let ZonePtr::Offset(array) = s.ptr_at(support, s.layout(12, 24))? else {
        return Ok(());
    };
    let array = s.resolve_alias(array);
    for index in 0..count {
        let ZonePtr::Offset(dvar) = s.ptr_at(array, index * s.pointer_bytes())? else {
            continue;
        };
        let name = string_at(s, s.resolve_alias(dvar), s.layout(4, 8));
        if !name.is_empty() {
            dump.push("d");
            dump.push_i32(index as i32);
            dump.push(name);
        }
    }
    Ok(())
}

struct ExpDump<'a> {
    buf: &'a mut [u8],
    used: usize,
}

impl ExpDump<'_> {
    fn push(&mut self, part: &str) {
        if self.buf.is_empty() {
            return;
        }
        if self.used >= self.buf.len() {
            return;
        }
        if self.used > 0 {
            self.buf[self.used] = b' ';
            self.used += 1;
            if self.used >= self.buf.len() {
                return;
            }
        }
        let bytes = part.as_bytes();
        let n = bytes.len().min(self.buf.len() - self.used);
        self.buf[self.used..self.used + n].copy_from_slice(&bytes[..n]);
        self.used += n;
    }

    fn push_string(&mut self, value: &str) {
        self.push("s:");
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for byte in value.bytes() {
            if self.used + 2 > self.buf.len() {
                return;
            }
            self.buf[self.used] = HEX[(byte >> 4) as usize];
            self.buf[self.used + 1] = HEX[(byte & 15) as usize];
            self.used += 2;
        }
    }

    fn push_i32(&mut self, value: i32) {
        let mut tmp = [0u8; 16];
        self.push(i32_to_str(value, &mut tmp));
    }

    fn push_f32(&mut self, value: f32) {
        const HEX: &[u8] = b"0123456789abcdef";
        let bits = value.to_bits();
        let mut tmp = [0u8; 10];
        tmp[0] = b'f';
        tmp[1] = b':';
        for i in 0..8 {
            tmp[2 + i] = HEX[((bits >> (28 - 4 * i)) & 0xf) as usize];
        }
        self.push(core::str::from_utf8(&tmp).unwrap_or("f:00000000"));
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.used]).unwrap_or("")
    }
}

const STMT_CACHE_N: usize = 64;
const STMT_CACHE_BLOB: usize = 1024;

struct StatementCache {
    keys: [u32; STMT_CACHE_N],
    blobs: [[u8; STMT_CACHE_BLOB]; STMT_CACHE_N],
    lens: [u16; STMT_CACHE_N],
    n: usize,
}

impl Default for StatementCache {
    fn default() -> Self {
        Self {
            keys: [0; STMT_CACHE_N],
            blobs: [[0; STMT_CACHE_BLOB]; STMT_CACHE_N],
            lens: [0; STMT_CACHE_N],
            n: 0,
        }
    }
}

impl StatementCache {
    fn key_of(p: Ptr) -> u32 {
        ZonePtr::encode_offset(p)
    }

    fn insert(&mut self, p: Ptr, text: &str) {
        if text.is_empty() {
            return;
        }
        let k = Self::key_of(p);
        for i in 0..self.n {
            if self.keys[i] == k {
                return;
            }
        }
        if self.n >= STMT_CACHE_N {
            return;
        }
        let bytes = text.as_bytes();
        let n = bytes.len().min(STMT_CACHE_BLOB);
        let i = self.n;
        self.keys[i] = k;
        self.blobs[i][..n].copy_from_slice(&bytes[..n]);
        self.lens[i] = n as u16;
        self.n += 1;
    }

    fn get(&self, p: Ptr) -> Option<&str> {
        let k = Self::key_of(p);
        for i in 0..self.n {
            if self.keys[i] == k {
                let text = core::str::from_utf8(&self.blobs[i][..self.lens[i] as usize]).ok()?;
                if text.is_empty() {
                    return None;
                }
                return Some(text);
            }
        }
        None
    }
}

fn i32_to_str(value: i32, tmp: &mut [u8; 16]) -> &str {
    if value == 0 {
        tmp[0] = b'0';
        return "0";
    }
    let neg = value < 0;
    let mut v = if value == i32::MIN {
        1u32 << 31
    } else {
        value.unsigned_abs()
    };
    let mut i = tmp.len();
    while v > 0 {
        i -= 1;
        tmp[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    if neg {
        i -= 1;
        tmp[i] = b'-';
    }
    core::str::from_utf8(&tmp[i..]).unwrap_or("")
}

fn load_statement(s: &mut ZoneStream<'_>, cache: &mut StatementCache) -> Result<Ptr> {
    let mut buf = [0u8; 1024];
    let mut dump = ExpDump {
        buf: &mut buf,
        used: 0,
    };
    let stmt = load_statement_rec(s, &mut dump, cache)?;
    cache.insert(stmt, dump.as_str());
    s.expr_stmt_insert(stmt, dump.as_str());
    Ok(stmt)
}

fn load_statement_into(
    s: &mut ZoneStream<'_>,
    buf: &mut [u8],
    cache: &mut StatementCache,
) -> Result<usize> {
    let mut dump = ExpDump { buf, used: 0 };
    let stmt = load_statement_rec(s, &mut dump, cache)?;
    cache.insert(stmt, dump.as_str());
    s.expr_stmt_insert(stmt, dump.as_str());
    Ok(dump.used)
}

const ITEM_STATEMENT_DUMP: usize = 2048;

fn dump_item_statement<'a>(
    s: &mut ZoneStream<'_>,
    buf: &'a mut [u8],
    cache: &mut StatementCache,
) -> Result<&'a str> {
    let n = load_statement_into(s, buf, cache)?;
    Ok(core::str::from_utf8(&buf[..n]).unwrap_or(""))
}

#[allow(clippy::collapsible_match)]
fn load_statement_rec(
    s: &mut ZoneStream<'_>,
    dump: &mut ExpDump<'_>,
    cache: &mut StatementCache,
) -> Result<Ptr> {
    const EET_OPERAND: u8 = 1;
    const VAL_INT: u8 = 0;
    const VAL_FLOAT: u8 = 1;
    const VAL_STRING: u8 = 2;
    const VAL_FUNCTION: u8 = 3;

    let p = s.alloc_load(4, s.layout(sz::STATEMENT, 48))?;
    let num_entries = s.i32_at(p, 0)? as usize;

    if let Some(arr) = s.follow_array(
        p,
        s.layout(4, 8),
        4,
        s.layout(sz::EXPRESSION_ENTRY, 24),
        num_entries,
    )? {
        for i in 0..num_entries {
            let e = arr.at(i * s.layout(sz::EXPRESSION_ENTRY, 24));
            if s.i32_at(e, 0)? as u8 == EET_OPERAND {
                let operand = e.at(s.layout(4, 8));
                match s.i32_at(operand, 0)? as u8 {
                    VAL_INT => dump.push_i32(s.i32_at(operand, s.layout(4, 8)).unwrap_or(0)),
                    VAL_FLOAT => dump.push_f32(s.f32_at(operand, s.layout(4, 8)).unwrap_or(0.0)),
                    VAL_STRING => {
                        s.follow_string(operand, s.layout(4, 8))?;
                        dump.push_string(string_at(s, operand, s.layout(4, 8)));
                    }
                    VAL_FUNCTION => dump_val_function(s, dump, cache, operand.at(s.layout(4, 8)))?,
                    other => dump.push_i32(other as i32),
                }
            } else {
                dump.push("op");
                dump.push_i32(s.i32_at(e, s.layout(4, 8)).unwrap_or(0));
            }
        }
    }

    if s.begin_body(p.at(s.layout(8, 16)))? {
        let support = load_expression_supporting_data(s, cache)?;
        s.fixup_slot(p.at(s.layout(8, 16)), support)?;
    }

    rebuild_statement_dump(s, p, dump, cache);
    Ok(p)
}

fn rebuild_statement_dump(
    s: &ZoneStream<'_>,
    p: Ptr,
    dump: &mut ExpDump<'_>,
    cache: &StatementCache,
) {
    let mut buf = [0u8; ITEM_STATEMENT_DUMP];
    let mut rebuilt = ExpDump {
        buf: &mut buf,
        used: 0,
    };
    if dump_loaded_statement(s, p, &mut rebuilt, cache, 0, None).is_err() || rebuilt.used == 0 {
        return;
    }
    let new = rebuilt.as_str();
    let old = dump.as_str();
    if dump_holes(new) < dump_holes(old)
        || (dump_holes(new) == dump_holes(old) && new.len() > old.len())
    {
        let n = rebuilt.used.min(dump.buf.len());
        dump.buf[..n].copy_from_slice(&rebuilt.buf[..n]);
        dump.used = n;
    }
}

fn dump_holes(text: &str) -> usize {
    text.matches("fnmiss").count() + text.matches("{ }").count() + text.matches("{  }").count()
}

fn dump_val_function(
    s: &mut ZoneStream<'_>,
    dump: &mut ExpDump<'_>,
    cache: &mut StatementCache,
    slot: Ptr,
) -> Result<()> {
    match s.ptr_at(slot, 0)? {
        ZonePtr::Null => Ok(()),
        ZonePtr::Offset(p) => {
            s.note_offset(p);
            let body = s.resolve_alias(p);
            dump.push("{");
            push_nested_body(s, dump, cache, body);
            dump.push("}");
            Ok(())
        }
        ZonePtr::Following | ZonePtr::Insert => {
            if s.begin_body(slot)? {
                let mut buf = [0u8; 1024];
                let mut inner = ExpDump {
                    buf: &mut buf,
                    used: 0,
                };
                let stmt = load_statement_rec(s, &mut inner, cache)?;
                cache.insert(stmt, inner.as_str());
                s.expr_stmt_insert(stmt, inner.as_str());
                dump.push("{");
                dump.push(inner.as_str());
                dump.push("}");
            } else if let Ok(ZonePtr::Offset(p)) = s.ptr_at(slot, 0) {
                dump.push("{");
                push_nested_body(s, dump, cache, s.resolve_alias(p));
                dump.push("}");
            }
            Ok(())
        }
    }
}

fn push_nested_body(s: &ZoneStream<'_>, dump: &mut ExpDump<'_>, cache: &StatementCache, body: Ptr) {
    if let Some(inner) = s.expr_stmt_get(body) {
        dump.push(inner);
        return;
    }
    if let Some(inner) = cache.get(body) {
        dump.push(inner);
        return;
    }
    let mut buf = [0u8; 1024];
    let mut inner = ExpDump {
        buf: &mut buf,
        used: 0,
    };
    if dump_loaded_statement(s, body, &mut inner, cache, 1, None).is_ok() && inner.used > 0 {
        if dump_holes(inner.as_str()) == 0 {
            dump.push(inner.as_str());
            return;
        }
    }
    dump.push("fnmiss");
    dump.push("cache");
}

fn dump_loaded_statement(
    s: &ZoneStream<'_>,
    p: Ptr,
    dump: &mut ExpDump<'_>,
    cache: &StatementCache,
    depth: u8,
    inherited_support: Option<Ptr>,
) -> Result<()> {
    if depth > 12 {
        dump.push("fnmiss");
        dump.push("depth");
        return Ok(());
    }
    let support = stmt_support(s, p).or(inherited_support);
    let num_entries = match s.i32_at(p, 0) {
        Ok(n) if n >= 0 => n as usize,
        _ => {
            dump.push("fnmiss");
            dump.push("nent");
            return Ok(());
        }
    };
    let arr = match s.ptr_at(p, s.layout(4, 8)) {
        Ok(ZonePtr::Offset(arr)) => arr,
        other => {
            dump.push("fnmiss");
            match other {
                Ok(ZonePtr::Following) => dump.push("entries-follow"),
                Ok(ZonePtr::Insert) => dump.push("entries-insert"),
                Ok(ZonePtr::Null) => dump.push("entries-null"),
                _ => dump.push("entries"),
            }
            return Ok(());
        }
    };
    const EET_OPERAND: u8 = 1;
    const VAL_INT: u8 = 0;
    const VAL_FLOAT: u8 = 1;
    const VAL_STRING: u8 = 2;
    const VAL_FUNCTION: u8 = 3;
    for i in 0..num_entries {
        let e = arr.at(i * s.layout(sz::EXPRESSION_ENTRY, 24));
        if s.i32_at(e, 0).unwrap_or(-1) as u8 == EET_OPERAND {
            let operand = e.at(s.layout(4, 8));
            match s.i32_at(operand, 0).unwrap_or(-1) as u8 {
                VAL_INT => dump.push_i32(s.i32_at(operand, s.layout(4, 8)).unwrap_or(0)),
                VAL_FLOAT => dump.push_f32(s.f32_at(operand, s.layout(4, 8)).unwrap_or(0.0)),
                VAL_STRING => dump.push_string(string_at(s, operand, s.layout(4, 8))),
                VAL_FUNCTION => match s.ptr_at(operand, s.layout(4, 8)) {
                    Ok(ZonePtr::Offset(q)) => {
                        dump.push("{");
                        let body = s.resolve_alias(q);
                        let mut buf = [0u8; 1024];
                        let mut inner = ExpDump {
                            buf: &mut buf,
                            used: 0,
                        };
                        let dumped = dump_loaded_statement(
                            s,
                            body,
                            &mut inner,
                            cache,
                            depth.saturating_add(1),
                            support,
                        )
                        .is_ok()
                            && inner.used > 0;
                        let inner_text = inner.as_str();
                        if dumped && dump_holes(inner_text) == 0 {
                            dump.push(inner_text);
                        } else if let Some(cached) =
                            s.expr_stmt_get(body).or_else(|| cache.get(body))
                        {
                            dump.push(cached);
                        } else if dumped {
                            dump.push(inner_text);
                        } else {
                            dump.push("fnmiss");
                            dump.push("nested");
                        }
                        dump.push("}");
                    }
                    Ok(ZonePtr::Null) => {}
                    Ok(ZonePtr::Following) => {
                        dump.push("fnmiss");
                        dump.push("valfn-follow");
                    }
                    _ => {
                        dump.push("fnmiss");
                        dump.push("valfn");
                    }
                },
                other => dump.push_i32(other as i32),
            }
        } else {
            let op = s.i32_at(e, s.layout(4, 8)).unwrap_or(0);
            dump.push("op");
            dump.push_i32(op);
        }
    }
    Ok(())
}

fn stmt_support(s: &ZoneStream<'_>, stmt: Ptr) -> Option<Ptr> {
    match s.ptr_at(stmt, s.layout(8, 16)).ok()? {
        ZonePtr::Offset(p) => Some(s.resolve_alias(p)),
        _ => None,
    }
}

fn follow_handler_set(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    parent: Ptr,
    field: usize,
    menu: &str,
    item: &str,
    kind: MenuScriptKind,
    cache: &mut StatementCache,
) -> Result<()> {
    let (load, insert_slot) = s.begin_body_with_insert(parent.at(field))?;
    if load {
        load_event_handler_set(s, links, menu, item, kind, insert_slot, cache)?;
    } else if let Ok(ZonePtr::Offset(q)) = s.ptr_at(parent, field) {
        links.reuse_menu_script_set(s.resolve_alias(q), menu, item, kind)?;
    }
    Ok(())
}

fn load_event_handler_set(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    menu: &str,
    item: &str,
    kind: MenuScriptKind,
    insert_slot: Option<Ptr>,
    cache: &mut StatementCache,
) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::MENU_EVENT_HANDLER_SET, 16))?;
    links.begin_menu_script_set(p);
    let count = s.i32_at(p, 0)? as usize;

    if s.begin_body(p.at(s.layout(4, 8)))? {
        let arr = s.alloc_load(4, s.pointer_bytes() * count)?;
        for i in 0..count {
            if s.begin_body(arr.at(i * s.pointer_bytes()))? {
                load_event_handler(s, links, menu, item, kind, cache)?;
            }
        }
    }
    links.end_menu_script_set(insert_slot);
    Ok(())
}

#[allow(clippy::collapsible_match)]
fn load_event_handler(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    menu: &str,
    item: &str,
    kind: MenuScriptKind,
    cache: &mut StatementCache,
) -> Result<()> {
    const EVENT_UNCONDITIONAL: i32 = 0;
    const EVENT_IF: i32 = 1;
    const EVENT_ELSE: i32 = 2;
    const EVENT_SET_LOCAL_VAR_FIRST: i32 = 3;
    const EVENT_SET_LOCAL_VAR_LAST: i32 = 6;

    let p = s.alloc_load(4, s.layout(sz::MENU_EVENT_HANDLER, 16))?;
    let event_type = i32::from(s.u8_at(p, s.layout(4, 8))?);
    let data = p;

    match event_type {
        EVENT_UNCONDITIONAL => {
            s.follow_string(data, 0)?;
            let script = string_at(s, data, 0);
            if !script.is_empty() {
                links.capture_menu_script(menu, item, kind, script)?;
            }
        }
        EVENT_IF => {
            if s.begin_body(data.at(0))? {
                let c = s.alloc_load(4, s.layout(sz::CONDITIONAL_SCRIPT, 16))?;
                let mut condition_buf = [0u8; ITEM_STATEMENT_DUMP];
                let mut condition_len = 0;
                capture_expr_body_or_alias(s, c, s.layout(4, 8), cache, |dump| {
                    condition_len = copy_into(dump, &mut condition_buf).len();
                })?;
                let condition = core::str::from_utf8(&condition_buf[..condition_len]).unwrap_or("");
                links.begin_menu_event_branch(menu, item, kind, Some(condition))?;
                follow_handler_set(s, links, c, 0, menu, item, kind, cache)?;
                links.end_menu_event_branch(menu, item, kind)?;
            }
        }
        EVENT_ELSE => {
            links.begin_menu_event_branch(menu, item, kind, None)?;
            follow_handler_set(s, links, data, 0, menu, item, kind, cache)?;
            links.end_menu_event_branch(menu, item, kind)?;
        }
        t if (EVENT_SET_LOCAL_VAR_FIRST..=EVENT_SET_LOCAL_VAR_LAST).contains(&t) => {
            if s.begin_body(data.at(0))? {
                let v = s.alloc_load(4, s.layout(sz::SET_LOCAL_VAR_DATA, 16))?;
                s.follow_string(v, 0)?;
                let mut name_buf = [0u8; 64];
                let name = copy_into(string_at(s, v, 0), &mut name_buf);

                let mut captured = Ok(());
                capture_expr_body_or_alias(s, v, s.layout(4, 8), cache, |expr| {
                    captured = links.capture_menu_set_local_var(menu, item, kind, t, name, expr);
                })?;
                captured?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn load_item_key_handler(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    menu: &str,
    cache: &mut StatementCache,
) -> Result<()> {
    let mut more = true;
    while more {
        let p = s.alloc_load(4, s.layout(sz::ITEM_KEY_HANDLER, 24))?;
        follow_handler_set(
            s,
            links,
            p,
            s.layout(4, 8),
            menu,
            "",
            MenuScriptKind::ExecKey,
            cache,
        )?;
        more = s.begin_body(p.at(s.layout(8, 16)))?;
    }
    Ok(())
}

fn load_item_def(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    menu_name: &str,
    cache: &mut StatementCache,
) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::ITEM_DEF, 488))?;
    let item_type = s.i32_at(p, s.layout(184, 196))?;
    let style = s.i32_at(p, s.layout(0x30, 56))?;
    let float_expression_count = s.i32_at(p, s.layout(316, 404))? as usize;

    let owner_draw = s.i32_at(p, s.layout(0x38, 64))?;
    let static_flags = s.i32_at(p, s.layout(68, 76)).unwrap_or(0);
    let rect = read_rect(s, p, s.layout(4, 8))?;
    let fore_color = read_vec4(s, p, s.layout(80, 88)).unwrap_or([1.0, 1.0, 1.0, 1.0]);
    let back_color = read_vec4(s, p, s.layout(0x60, 104)).unwrap_or([0.0, 0.0, 0.0, 0.0]);
    let glow_color = read_vec4(s, p, s.layout(0x154, 448)).unwrap_or([0.0, 0.0, 0.0, 0.0]);
    let text_scale = s.f32_at(p, s.layout(212, 224)).unwrap_or(1.0);
    let font_enum = s.i32_at(p, s.layout(0xc4, 208)).unwrap_or(0);
    let text_align_mode = s.i32_at(p, s.layout(0xc8, 212)).unwrap_or(0);
    let text_align_x = s.f32_at(p, s.layout(0xcc, 216)).unwrap_or(0.0);
    let text_align_y = s.f32_at(p, s.layout(0xd0, 220)).unwrap_or(0.0);
    let text_style = s.i32_at(p, s.layout(0xd8, 228)).unwrap_or(0);
    let game_msg_window_index = s.i32_at(p, s.layout(220, 232))?;

    let bg_ptr = ptr_kind(s, p, s.layout(160, 168));
    let sound_ptr = ptr_kind(s, p, s.layout(296, 376));
    let vis_ptr = ptr_kind(s, p, s.layout(324, 416));
    let mat_ptr = ptr_kind(s, p, s.layout(336, 440));
    let mouse_enter_ptr = ptr_kind(s, p, s.layout(248, 280));
    let on_focus_ptr = ptr_kind(s, p, s.layout(264, 312));
    let mut bg_buf = [0u8; 128];
    let background = load_window_def(s, links, p, &mut bg_buf)?;
    let mut text_buf = [0u8; 256];
    let text = {
        let raw = match s.follow_string(p, s.layout(228, 240))? {
            Some(q) => s.cstr_bytes(q).unwrap_or(b""),
            None => b"",
        };
        copy_bytes(raw, &mut text_buf)
    };
    let mut name_buf = [0u8; 128];
    let item_name = copy_into(name_at(s, p)?.unwrap_or(""), &mut name_buf);
    links.capture_menu_item(menu_name, item_name, text, owner_draw, item_type)?;

    const ITEM_SCRIPT_FIELDS: [(usize, MenuScriptKind); 8] = [
        (240, MenuScriptKind::MouseEnterText),
        (244, MenuScriptKind::MouseExitText),
        (248, MenuScriptKind::MouseEnter),
        (252, MenuScriptKind::MouseExit),
        (256, MenuScriptKind::Action),
        (260, MenuScriptKind::Accept),
        (264, MenuScriptKind::OnFocus),
        (268, MenuScriptKind::LeaveFocus),
    ];
    for (field, kind) in ITEM_SCRIPT_FIELDS {
        follow_handler_set(
            s,
            links,
            p,
            s.layout(field, 264 + (field - 240) * 2),
            menu_name,
            item_name,
            kind,
            cache,
        )?;
    }
    s.follow_string(p, s.layout(272, 328))?;
    s.follow_string(p, s.layout(276, 336))?;
    if s.begin_body(p.at(s.layout(280, 344)))? {
        load_item_key_handler(s, links, menu_name, cache)?;
    }
    s.follow_string(p, s.layout(284, 352))?;
    s.follow_string(p, s.layout(288, 360))?;
    let mut dvar_buf = [0u8; 128];
    let dvar = copy_into(string_at(s, p, s.layout(272, 328)), &mut dvar_buf);
    let mut dvar_test_buf = [0u8; 128];
    let dvar_test = copy_into(string_at(s, p, s.layout(276, 336)), &mut dvar_test_buf);
    let mut enable_dvar_buf = [0u8; 128];
    let enable_dvar = copy_into(string_at(s, p, s.layout(284, 352)), &mut enable_dvar_buf);
    let mut local_var_buf = [0u8; 128];
    let local_var = copy_into(string_at(s, p, s.layout(288, 360)), &mut local_var_buf);

    let fresh_sound = asset_ptr_at_linked(s, links, AssetType::Sound, p.at(s.layout(296, 376)))?;
    let from_stream = name_after_link(s, p, s.layout(296, 376), fresh_sound, s.latest_sound_name());
    let focus_raw = links
        .linked_asset_name(p.at(s.layout(296, 376)))
        .unwrap_or(from_stream);
    let mut focus_buf = [0u8; 128];
    let focus_sound = copy_into(focus_raw, &mut focus_buf);
    links.capture_menu_item_layout(&MenuItemLayout {
        menu: menu_name,
        name: item_name,
        text,
        item_type,
        style,
        owner_draw,
        rect,
        fore_color,
        back_color,
        glow_color,
        text_scale,
        font_enum,
        text_align_mode,
        text_align_x,
        text_align_y,
        text_style,
        game_msg_window_index,
        background,
        focus_sound,
        dvar,
        dvar_test,
        enable_dvar,
        local_var,
        bg_ptr,
        vis_ptr,
        mat_ptr,
        sound_ptr,
        mouse_enter_ptr,
        on_focus_ptr,
        static_flags,
        dvar_flags: s.i32_at(p, s.layout(292, 368)).unwrap_or(0),
    })?;

    load_item_type_data(
        s,
        links,
        menu_name,
        item_name,
        p.at(s.layout(308, 392)),
        item_type,
        cache,
    )?;

    if s.begin_body(p.at(s.layout(320, 408)))? {
        let arr = s.alloc_load(
            4,
            s.layout(sz::ITEM_FLOAT_EXPRESSION, 16) * float_expression_count,
        )?;
        for i in 0..float_expression_count {
            let e = arr.at(i * s.layout(sz::ITEM_FLOAT_EXPRESSION, 16));
            let key = s.u32_at(e, 0).unwrap_or(0);
            capture_expr_body_or_alias(s, e, s.layout(4, 8), cache, |dump| {
                let _ = links.capture_item_float_exp(menu_name, item_name, key, dump);
            })?;
        }
    }
    capture_expr_body_or_alias(s, p, s.layout(324, 416), cache, |dump| {
        let _ = links.capture_item_visible_exp(menu_name, item_name, dump);
    })?;
    capture_expr_body_or_alias(s, p, s.layout(328, 424), cache, |dump| {
        let _ = links.capture_item_disabled_exp(menu_name, item_name, dump);
    })?;
    capture_expr_body_or_alias(s, p, s.layout(332, 432), cache, |dump| {
        let _ = links.capture_item_text_exp(menu_name, item_name, dump);
    })?;
    capture_expr_body_or_alias(s, p, s.layout(336, 440), cache, |dump| {
        let _ = links.capture_item_material_exp(menu_name, item_name, dump);
    })?;
    Ok(())
}

fn capture_expr_body_or_alias(
    s: &mut ZoneStream<'_>,
    parent: Ptr,
    field: usize,
    cache: &mut StatementCache,
    mut on_dump: impl FnMut(&str),
) -> Result<()> {
    if s.begin_body(parent.at(field))? {
        let mut buf = [0u8; ITEM_STATEMENT_DUMP];
        let dump = dump_item_statement(s, &mut buf, cache)?;
        if !dump.is_empty() {
            on_dump(dump);
        }
        return Ok(());
    }
    let Ok(ZonePtr::Offset(stmt)) = s.ptr_at(parent, field) else {
        return Ok(());
    };
    let body = s.resolve_alias(stmt);
    if let Some(cached) = s.expr_stmt_get(body).or_else(|| cache.get(body)) {
        if !cached.is_empty() {
            on_dump(cached);
        }
        return Ok(());
    }
    let mut buf = [0u8; ITEM_STATEMENT_DUMP];
    let mut dump = ExpDump {
        buf: &mut buf,
        used: 0,
    };
    let _ = dump_loaded_statement(s, body, &mut dump, cache, 0, None);
    if dump.used > 0 {
        on_dump(dump.as_str());
    }
    Ok(())
}

fn load_item_type_data(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    menu: &str,
    item: &str,
    p: Ptr,
    item_type: i32,
    cache: &mut StatementCache,
) -> Result<()> {
    const ITEM_TYPE_LISTBOX: i32 = 6;
    const ITEM_TYPE_MULTI: i32 = 12;
    const ITEM_TYPE_DVARENUM: i32 = 13;
    const ITEM_TYPE_NEWS_TICKER: i32 = 20;
    const ITEM_TYPE_TEXT_SCROLL: i32 = 21;
    const EDIT_FIELD_TYPES: [i32; 11] = [0, 4, 9, 10, 11, 14, 16, 17, 18, 22, 23];

    if item_type == ITEM_TYPE_LISTBOX {
        if s.begin_body(p.at(0))? {
            let b = s.alloc_load(4, s.layout(sz::LIST_BOX_DEF, 336))?;
            follow_handler_set(s, links, b, 288, menu, item, MenuScriptKind::Action, cache)?;
            asset_ptr_at(s, links, AssetType::Material, b.at(s.layout(320, 328)))?;
        }
    } else if EDIT_FIELD_TYPES.contains(&item_type) {
        if s.begin_body(p.at(0))? {
            s.alloc_load(4, sz::EDIT_FIELD_DEF)?;
        }
    } else if item_type == ITEM_TYPE_MULTI {
        if s.begin_body(p.at(0))? {
            let m = s.alloc_load(4, s.layout(sz::MULTI_DEF, 648))?;
            let labels = 0;
            let strings = s.layout(128, 256);
            for i in 0..32 {
                s.follow_string(m, labels + i * s.pointer_bytes())?;
            }
            for i in 0..32 {
                s.follow_string(m, strings + i * s.pointer_bytes())?;
            }
            let floats = s.layout(256, 512);
            let count = s.i32_at(m, s.layout(384, 640))?.clamp(0, 32) as usize;
            let string_values = s.i32_at(m, s.layout(388, 644))? != 0;
            for i in 0..count {
                let label = string_at(s, m, labels + i * s.pointer_bytes());
                let value = if string_values {
                    MenuChoiceValue::Str(string_at(s, m, strings + i * s.pointer_bytes()))
                } else {
                    MenuChoiceValue::Float(s.f32_at(m, floats + i * 4)?)
                };
                links.capture_item_choice(menu, item, label, value)?;
            }
        }
    } else if item_type == ITEM_TYPE_DVARENUM {
        s.follow_string(p, 0)?;
    } else if item_type == ITEM_TYPE_NEWS_TICKER {
        if s.begin_body(p.at(0))? {
            s.alloc_load(4, sz::NEWS_TICKER_DEF)?;
        }
    } else if item_type == ITEM_TYPE_TEXT_SCROLL && s.begin_body(p.at(0))? {
        s.alloc_load(4, sz::TEXT_SCROLL_DEF)?;
    }
    Ok(())
}

fn follow_string_array(s: &mut ZoneStream<'_>, p: Ptr, field: usize, count: usize) -> Result<()> {
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
            let arr = s.alloc_load(4, s.pointer_bytes() * count)?;
            for i in 0..count {
                s.follow_string(arr, i * s.pointer_bytes())?;
            }
            Ok(())
        }
    }
}
