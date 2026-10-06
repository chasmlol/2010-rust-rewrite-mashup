use std::sync::Arc;

use crate::transport::frame::{FRAME_SEGMENTS, FrameParts, FrameSegments, WireMeta};
use crate::transport::meta_wire::META_SEGMENTS;

const SAME: u8 = 0;
const PATCH: u8 = 1;
const RAW: u8 = 2;
const PREFIXED: u8 = 3;

const RUN_MERGE_GAP: usize = 8;
const PREFIX_MIN_BYTES: usize = 4096;
const OUTER_LEVEL: i32 = 3;

pub const MAX_RECONSTRUCTED_FRAME_BYTES: usize = 256 * 1024;
pub const MAX_DELTA_INSTRUCTION_BYTES: usize = 256 * 1024;
const MAX_ZSTD_WINDOW_LOG: u32 = 18;

fn reserve_output(out: &mut Vec<u8>, additional: usize) -> Option<()> {
    let end = out.len().checked_add(additional)?;
    if end > MAX_RECONSTRUCTED_FRAME_BYTES {
        return None;
    }
    out.try_reserve_exact(additional).ok()
}

fn segment_slices<'a>(bytes: &'a [u8], lens: &FrameSegments) -> Option<[&'a [u8]; FRAME_SEGMENTS]> {
    let mut out = [&bytes[..0]; FRAME_SEGMENTS];
    let mut at = 0usize;
    for (slot, len) in out.iter_mut().zip(lens) {
        let end = at.checked_add(*len as usize)?;
        *slot = bytes.get(at..end)?;
        at = end;
    }
    (at == bytes.len()).then_some(out)
}

#[derive(Default)]
pub struct MetaPatchCache {
    entries: Vec<(Arc<WireMeta>, Arc<WireMeta>, Vec<u8>)>,
}

pub fn encode(new: &FrameParts, old: &FrameParts, cache: &mut MetaPatchCache) -> Vec<u8> {
    let new_segments = new.segments();
    let old_segments = old.segments();
    let mut out = Vec::with_capacity(4096);
    put_segment(&mut out, new_segments[0], old_segments[0]);
    let cached = cache
        .entries
        .iter()
        .find(|(n, o, _)| Arc::ptr_eq(n, &new.meta) && Arc::ptr_eq(o, &old.meta));
    match cached {
        Some((_, _, patch)) => out.extend_from_slice(patch),
        None => {
            let start = out.len();
            for index in 1..META_SEGMENTS {
                put_segment(&mut out, new_segments[index], old_segments[index]);
            }
            cache.entries.push((
                Arc::clone(&new.meta),
                Arc::clone(&old.meta),
                out[start..].to_vec(),
            ));
        }
    }
    for index in META_SEGMENTS..FRAME_SEGMENTS {
        put_segment(&mut out, new_segments[index], old_segments[index]);
    }
    out
}

fn put_segment(out: &mut Vec<u8>, new: &[u8], old: &[u8]) {
    if new == old {
        out.push(SAME);
        return;
    }
    if new.len() == old.len() {
        let mark = out.len();
        out.push(PATCH);
        put_patch(out, new, old);
        if out.len() - mark <= new.len() / 4 || new.len() < PREFIX_MIN_BYTES {
            return;
        }
        out.truncate(mark);
    }
    if new.len() >= PREFIX_MIN_BYTES
        && !old.is_empty()
        && let Ok(packed) = compress_against(new, old)
    {
        out.push(PREFIXED);
        put_varint(out, new.len());
        put_varint(out, packed.len());
        out.extend_from_slice(&packed);
        return;
    }
    out.push(RAW);
    put_varint(out, new.len());
    out.extend_from_slice(new);
}

pub fn decode(delta: &[u8], base: &[u8], base_lens: &FrameSegments) -> Option<Vec<u8>> {
    if delta.len() > MAX_DELTA_INSTRUCTION_BYTES || base.len() > MAX_RECONSTRUCTED_FRAME_BYTES {
        return None;
    }
    let old = segment_slices(base, base_lens)?;
    let mut input = delta;
    let mut out = Vec::new();
    for old in old {
        let (&tag, rest) = input.split_first()?;
        input = rest;
        match tag {
            SAME => {
                reserve_output(&mut out, old.len())?;
                out.extend_from_slice(old);
            }
            PATCH => {
                reserve_output(&mut out, old.len())?;
                let start = out.len();
                out.extend_from_slice(old);
                let runs = get_varint(&mut input)?;
                let mut at = 0usize;
                for _ in 0..runs {
                    at = at.checked_add(get_varint(&mut input)?)?;
                    let len = get_varint(&mut input)?;
                    let bytes = take(&mut input, len)?;
                    let end = at.checked_add(len)?;
                    out.get_mut(start + at..start + end)?.copy_from_slice(bytes);
                    at = end;
                }
            }
            RAW => {
                let len = get_varint(&mut input)?;
                reserve_output(&mut out, len)?;
                out.extend_from_slice(take(&mut input, len)?);
            }
            PREFIXED => {
                let len = get_varint(&mut input)?;
                let packed_len = get_varint(&mut input)?;
                let packed = take(&mut input, packed_len)?;
                let start = out.len();
                reserve_output(&mut out, len)?;
                out.resize(start.checked_add(len)?, 0);
                let mut dctx = zstd::zstd_safe::DCtx::create();
                dctx.set_parameter(zstd::zstd_safe::DParameter::WindowLogMax(
                    MAX_ZSTD_WINDOW_LOG,
                ))
                .ok()?;
                dctx.ref_prefix(old).ok()?;
                let got = dctx.decompress(&mut out[start..], packed).ok()?;
                if got != len {
                    return None;
                }
            }
            _ => return None,
        }
    }
    input.is_empty().then_some(out)
}

pub fn compress(delta: &[u8]) -> std::io::Result<Vec<u8>> {
    if delta.len() > MAX_DELTA_INSTRUCTION_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "snapshot instruction budget exceeded",
        ));
    }
    thread_local! {
        static COMPRESSOR: std::cell::RefCell<Option<zstd::bulk::Compressor<'static>>> =
            const { std::cell::RefCell::new(None) };
    }
    COMPRESSOR.with_borrow_mut(|slot| {
        if slot.is_none() {
            *slot = Some(zstd::bulk::Compressor::new(OUTER_LEVEL)?);
        }
        slot.as_mut()
            .expect("compressor initialised")
            .compress(delta)
    })
}

pub fn decompress(packed: &[u8], len: usize) -> std::io::Result<Vec<u8>> {
    if len > MAX_DELTA_INSTRUCTION_BYTES
        || packed.len() > crate::transport::protocol::MAX_PACKET_BYTES as usize
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "snapshot decode budget exceeded",
        ));
    }
    thread_local! {
        static DECOMPRESSOR: std::cell::RefCell<Option<zstd::bulk::Decompressor<'static>>> =
            const { std::cell::RefCell::new(None) };
    }
    DECOMPRESSOR.with_borrow_mut(|slot| {
        if slot.is_none() {
            let mut decoder = zstd::bulk::Decompressor::new()?;
            decoder.set_parameter(zstd::zstd_safe::DParameter::WindowLogMax(
                MAX_ZSTD_WINDOW_LOG,
            ))?;
            *slot = Some(decoder);
        }
        slot.as_mut()
            .expect("decompressor initialised")
            .decompress(packed, len)
    })
}

fn first_difference(new: &[u8], old: &[u8], from: usize) -> Option<usize> {
    let mut at = from;
    while at + 8 <= new.len() {
        let a = u64::from_ne_bytes(new[at..at + 8].try_into().expect("8-byte window"));
        let b = u64::from_ne_bytes(old[at..at + 8].try_into().expect("8-byte window"));
        if a != b {
            break;
        }
        at += 8;
    }
    (at..new.len()).find(|&i| new[i] != old[i])
}

fn put_patch(out: &mut Vec<u8>, new: &[u8], old: &[u8]) {
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut at = 0usize;
    while let Some(first) = first_difference(new, old, at) {
        let mut last = first;
        let mut scan = first;
        while scan < new.len() && scan - last <= RUN_MERGE_GAP {
            if new[scan] != old[scan] {
                last = scan;
            }
            scan += 1;
        }
        match runs.last_mut() {
            Some((_, end)) if first - *end <= RUN_MERGE_GAP => *end = last + 1,
            _ => runs.push((first, last + 1)),
        }
        at = last + 1;
    }
    put_varint(out, runs.len());
    let mut cursor = 0usize;
    for (start, end) in runs {
        put_varint(out, start - cursor);
        put_varint(out, end - start);
        out.extend_from_slice(&new[start..end]);
        cursor = end;
    }
}

fn compress_against(
    payload: &[u8],
    baseline: &[u8],
) -> Result<Vec<u8>, zstd::zstd_safe::ErrorCode> {
    let mut cctx = zstd::zstd_safe::CCtx::create();
    cctx.set_parameter(zstd::zstd_safe::CParameter::CompressionLevel(3))?;
    cctx.ref_prefix(baseline)?;
    let mut out = vec![0u8; zstd::zstd_safe::compress_bound(payload.len())];
    let len = cctx.compress2(&mut out[..], payload)?;
    out.truncate(len);
    Ok(out)
}

fn put_varint(out: &mut Vec<u8>, mut value: usize) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn get_varint(input: &mut &[u8]) -> Option<usize> {
    let mut value = 0usize;
    for shift in (0..35).step_by(7) {
        let (&byte, rest) = input.split_first()?;
        *input = rest;
        value |= usize::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

fn take<'a>(input: &mut &'a [u8], len: usize) -> Option<&'a [u8]> {
    if len > input.len() {
        return None;
    }
    let (head, rest) = input.split_at(len);
    *input = rest;
    Some(head)
}
