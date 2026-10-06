use super::{AssetLinkSink, always_array, follow_name};
use crate::size as sz;
use crate::zone::{
    Ptr, Result, XFILE_BLOCK_LARGE, XFILE_BLOCK_PHYSICAL, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream,
};

pub(super) fn load_snd_bank(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, sz::SND_BANK)?;
    let alias_count = s.u32_at(p, 4)? as usize;
    let radverb_count = s.u32_at(p, 0x18)? as usize;
    let snapshot_count = s.u32_at(p, 0x20)? as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    if let Some(aliases) = always_array(s, p.at(8), 4, sz::SND_ALIAS_LIST * alias_count)? {
        for i in 0..alias_count {
            load_alias_list(s, links, aliases.at(i * sz::SND_ALIAS_LIST))?;
        }
    }
    always_array(s, p.at(12), 4, sz::SND_INDEX_ENTRY * alias_count)?;
    always_array(s, p.at(0x1c), 4, sz::SND_RADVERB * radverb_count)?;
    always_array(s, p.at(0x24), 4, sz::SND_SNAPSHOT * snapshot_count)?;
    s.pop()
}

pub(super) fn load_snd_driver_globals(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
) -> Result<()> {
    let p = s.alloc_load(4, sz::SND_DRIVER_GLOBALS)?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    for (count_offset, pointer_offset, stride) in sz::SND_DRIVER_GLOBAL_ARRAYS {
        let count = s.u32_at(p, count_offset)? as usize;
        let rows = always_array(s, p.at(pointer_offset), 4, stride * count)?;
        match (pointer_offset, rows) {
            (8, Some(rows)) => links.capture_snd_groups(s, rows, count)?,
            (16, Some(rows)) => links.capture_snd_curves(s, rows, count)?,
            _ => {}
        }
    }
    s.pop()
}

pub(super) fn load_snd_patch(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, sz::SND_PATCH)?;
    let element_count = s.u32_at(p, 4)? as usize;
    let file_count = s.u32_at(p, 12)? as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    always_array(s, p.at(8), 4, 4 * element_count)?;
    if s.begin_body(p.at(16))? {
        let files = s.alloc_load(4, sz::SOUND_FILE * file_count)?;
        s.fixup_slot(p.at(16), files)?;
        for i in 0..file_count {
            load_sound_file(s, links, files.at(i * sz::SOUND_FILE))?;
        }
    }
    s.pop()
}

fn load_alias_list(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    follow_name(s, p, sz::SND_ALIAS_LIST_NAME_OFF)?;
    let count = s.i32_at(p, sz::SND_ALIAS_LIST_COUNT_OFF)?.max(0) as usize;
    let mut head = None;
    if s.begin_body(p.at(sz::SND_ALIAS_LIST_HEAD_OFF))? {
        let arr = s.alloc_load(4, sz::SND_ALIAS * count)?;
        s.fixup_slot(p.at(sz::SND_ALIAS_LIST_HEAD_OFF), arr)?;
        head = Some(arr);
        for i in 0..count {
            load_alias(s, links, arr.at(i * sz::SND_ALIAS))?;
        }
    }
    links.capture_sound(s, p, count, head)
}

fn load_alias(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    follow_name(s, p, sz::SND_ALIAS_NAME_OFF)?;
    follow_name(s, p, sz::SND_ALIAS_SUBTITLE_OFF)?;
    follow_name(s, p, sz::SND_ALIAS_SECONDARY_OFF)?;
    if s.begin_body(p.at(sz::SND_ALIAS_SOUND_FILE_OFF))? {
        let file = s.alloc_load(4, sz::SOUND_FILE)?;
        s.fixup_slot(p.at(sz::SND_ALIAS_SOUND_FILE_OFF), file)?;
        load_sound_file(s, links, file)?;
    }
    Ok(())
}

fn load_sound_file(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    let ty = s.u8_at(p, sz::SOUND_FILE_TYPE_OFF)?;
    let slot = p.at(sz::SOUND_FILE_UNION_OFF);
    if !s.begin_body(slot)? {
        return Ok(());
    }
    if ty == sz::SAT_LOADED {
        let loaded = s.alloc_load(4, sz::LOADED_SOUND)?;
        s.fixup_slot(slot, loaded)?;
        load_loaded_sound(s, links, loaded)?;
        links.bind_last_loaded_to_sound_file(p)?;
    } else {
        let streamed = s.alloc_load(4, sz::STREAMED_SOUND)?;
        s.fixup_slot(slot, streamed)?;
        load_streamed_sound(s, streamed)?;
        let name = cstr_at(s, streamed, sz::STREAMED_SOUND_FILENAME_OFF).unwrap_or("");
        links.bind_streamed_sound_file(p, "", name)?;
    }
    Ok(())
}

fn load_loaded_sound(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    follow_name(s, p, sz::LOADED_SOUND_NAME_OFF)?;
    let asset = p.at(sz::LOADED_SOUND_ASSET_OFF);
    let pcm = load_snd_asset(s, asset)?;
    let data_len = s.u32_at(asset, sz::SND_ASSET_DATA_SIZE_OFF)? as usize;
    links.capture_loaded_sound(s, p, pcm.unwrap_or(p), pcm.map_or(0, |_| data_len))
}

fn load_snd_asset(s: &mut ZoneStream<'_>, p: Ptr) -> Result<Option<Ptr>> {
    let seek_count = s.u32_at(p, sz::SND_ASSET_SEEK_TABLE_COUNT_OFF)? as usize;
    let data_size = s.u32_at(p, sz::SND_ASSET_DATA_SIZE_OFF)? as usize;
    always_array(s, p.at(sz::SND_ASSET_SEEK_TABLE_OFF), 4, 4 * seek_count)?;

    s.push(XFILE_BLOCK_LARGE)?;
    s.push(XFILE_BLOCK_PHYSICAL)?;
    let pcm = always_array(s, p.at(sz::SND_ASSET_DATA_OFF), 2048, data_size)?;
    s.pop()?;
    s.pop()?;
    Ok(pcm)
}

fn load_streamed_sound(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    follow_name(s, p, sz::STREAMED_SOUND_FILENAME_OFF)?;
    if s.begin_body(p.at(4))? {
        let primed = s.alloc_load(4, sz::PRIMED_SOUND)?;
        s.fixup_slot(p.at(4), primed)?;
        load_primed_sound(s, primed)?;
    }
    Ok(())
}

fn load_primed_sound(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    follow_name(s, p, 0)?;
    let size = s.u32_at(p, 8)? as usize;
    s.push(XFILE_BLOCK_LARGE)?;
    always_array(s, p.at(4), 2048, size)?;
    s.pop()?;
    Ok(())
}

fn cstr_at<'a>(s: &'a ZoneStream<'_>, parent: Ptr, field: usize) -> Option<&'a str> {
    match s.ptr_at(parent, field).ok()? {
        ZonePtr::Offset(q) => s.cstr(s.resolve_alias(q)).ok(),
        ZonePtr::Null => Some(""),
        _ => None,
    }
}
