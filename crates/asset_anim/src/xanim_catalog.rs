use std::collections::HashMap;
use std::ops::Deref;
use std::sync::{Arc, Mutex};

use asset_iw4::size as sz;
use fastfile_iw4::{
    AssetLinkSink, AssetType, Ptr, Result, ScriptStrings, XAnimPartsGeometry, ZoneStream,
};

use crate::asset_graph::ZoneOwner;
use crate::asset_key::AssetNamespace;
use crate::xanim_clip::{AnimClip, ClipNotify, RawDeltaTrans, RawXAnimParts};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct XAnimKey {
    pub namespace: AssetNamespace,
    pub name: String,
}

impl XAnimKey {
    pub fn new(namespace: AssetNamespace, name: &str) -> Self {
        Self {
            namespace,
            name: ascii_lower(name),
        }
    }

    pub fn display(&self) -> String {
        format!("{}:anim/{}", self.namespace.as_str(), self.name)
    }
}

#[derive(Clone, Debug)]
pub struct CapturedXAnim {
    pub namespace: AssetNamespace,
    pub parts: Arc<RawXAnimParts>,
}

impl CapturedXAnim {
    pub fn key(&self) -> XAnimKey {
        XAnimKey::new(self.namespace, &self.parts.name)
    }
}

#[derive(Debug)]
pub struct XAnimCatalog {
    entries: Vec<CapturedXAnim>,

    indices: HashMap<XAnimKey, usize>,

    order: Vec<XAnimKey>,

    zones: Vec<ZoneOwner>,

    decoded: Mutex<Vec<Option<Arc<AnimClip>>>>,
}

#[derive(Clone, Debug)]
pub struct XAnimBuild {
    catalog: XAnimCatalog,
    capture_zone: ZoneOwner,
    capture_ns: Option<AssetNamespace>,
    pub capture_gaps: usize,
    strings: ScriptStrings,
}

impl Default for XAnimCatalog {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            indices: HashMap::new(),
            order: Vec::new(),
            zones: Vec::new(),
            decoded: Mutex::new(Vec::new()),
        }
    }
}

impl Default for XAnimBuild {
    fn default() -> Self {
        Self {
            catalog: XAnimCatalog::default(),
            capture_zone: ZoneOwner::default(),
            capture_ns: None,
            capture_gaps: 0,
            strings: ScriptStrings::default(),
        }
    }
}

impl Clone for XAnimCatalog {
    fn clone(&self) -> Self {
        let decoded = self
            .decoded
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone();
        Self {
            entries: self.entries.clone(),
            indices: self.indices.clone(),
            order: self.order.clone(),
            zones: self.zones.clone(),
            decoded: Mutex::new(decoded),
        }
    }
}

impl Deref for XAnimBuild {
    type Target = XAnimCatalog;

    fn deref(&self) -> &Self::Target {
        &self.catalog
    }
}

impl XAnimCatalog {
    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn get(&self, ns: AssetNamespace, name: &str) -> Option<&CapturedXAnim> {
        self.entries.get(self.index_by_name(ns, name)?)
    }

    pub fn hint_edge(
        &self,
        hint: Option<&str>,
        ns: AssetNamespace,
    ) -> asset_core::AssetEdge<asset_core::XAnimSpace> {
        match hint.filter(|name| !name.is_empty()) {
            None => asset_core::AssetEdge::Absent,
            Some(name) => match self.index_by_name(ns, name) {
                Some(index) => asset_core::AssetEdge::bind_order(index, self.zone_of(index)),
                None => asset_core::AssetEdge::Unresolved(asset_core::AssetEdgeReason::CatalogMiss),
            },
        }
    }

    fn has_key(&self, key: &XAnimKey) -> bool {
        self.indices.contains_key(key)
    }

    pub fn index_by_name(&self, ns: AssetNamespace, name: &str) -> Option<usize> {
        self.indices.get(&XAnimKey::new(ns, name)).copied()
    }

    pub fn zone_of(&self, index: usize) -> ZoneOwner {
        self.zones.get(index).copied().unwrap_or_default()
    }

    pub fn name_at(&self, index: usize) -> Option<&str> {
        self.order.get(index).map(|k| k.name.as_str())
    }

    pub fn clip_at(&self, index: usize) -> Option<Arc<AnimClip>> {
        let captured = self.entries.get(index)?;
        {
            let decoded = self
                .decoded
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if let Some(clip) = decoded.get(index).and_then(Option::as_ref) {
                return Some(Arc::clone(clip));
            }
        }
        let arc = Arc::new(AnimClip::from_parts(&captured.parts).ok()?);
        let mut decoded = self
            .decoded
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let slot = decoded.get_mut(index)?;
        Some(Arc::clone(slot.get_or_insert(arc)))
    }

    pub fn clip(&self, ns: AssetNamespace, name: &str) -> Option<Arc<AnimClip>> {
        self.clip_at(self.index_by_name(ns, name)?)
    }

    pub fn decode(&self, ns: AssetNamespace, name: &str) -> Option<AnimClip> {
        let captured = self.get(ns, name)?;
        AnimClip::from_parts(&captured.parts).ok()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.order.iter().map(|k| k.name.as_str())
    }

    pub fn namespace_count(&self, ns: AssetNamespace) -> usize {
        self.order.iter().filter(|k| k.namespace == ns).count()
    }

    pub fn collide_name_count(&self) -> usize {
        let mut seen: std::collections::HashMap<&str, u8> = std::collections::HashMap::new();
        for k in &self.order {
            *seen.entry(k.name.as_str()).or_insert(0) |= match k.namespace {
                AssetNamespace::Iw4 => 1,
                AssetNamespace::T5 => 2,
                AssetNamespace::Iw5 => 4,
                AssetNamespace::T6 => 8,
            };
        }
        seen.values().filter(|bits| bits.count_ones() >= 2).count()
    }
}

impl XAnimBuild {
    pub fn publish(self) -> XAnimCatalog {
        self.catalog
    }

    pub fn set_strings(&mut self, strings: ScriptStrings) {
        self.strings = strings;
    }

    pub fn set_capture_ns(&mut self, ns: AssetNamespace) {
        self.capture_ns = Some(ns);
    }

    pub fn set_capture_zone(&mut self, zone: ZoneOwner) {
        self.capture_zone = zone;
    }

    pub fn insert_captured(&mut self, captured: CapturedXAnim) {
        self.insert_in(
            self.capture_ns
                .expect("asset capture requires an explicit family"),
            captured,
        );
    }

    pub fn insert_in(&mut self, ns: AssetNamespace, mut captured: CapturedXAnim) {
        captured.namespace = ns;
        let key = captured.key();
        self.retain(key, captured);
    }

    fn retain(&mut self, key: XAnimKey, captured: CapturedXAnim) {
        let mut decoded = self
            .catalog
            .decoded
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(&pos) = self.catalog.indices.get(&key) {
            self.catalog.zones[pos] = self.capture_zone;
            self.catalog.entries[pos] = captured;
            decoded[pos] = None;
        } else {
            self.catalog
                .indices
                .insert(key.clone(), self.catalog.entries.len());
            self.catalog.order.push(key.clone());
            self.catalog.zones.push(self.capture_zone);
            self.catalog.entries.push(captured);
            decoded.push(None);
        }
    }

    pub fn absorb(&mut self, mut local: Self) -> usize {
        self.capture_gaps = self.capture_gaps.saturating_add(local.capture_gaps);
        let saved_zone = self.capture_zone;
        let saved_ns = self.capture_ns;
        let mut added = 0;
        let order = std::mem::take(&mut local.catalog.order);
        let entries = std::mem::take(&mut local.catalog.entries);
        for (i, (key, captured)) in order.into_iter().zip(entries).enumerate() {
            let vacant = !self.catalog.has_key(&key);
            self.capture_zone = local
                .catalog
                .zones
                .get(i)
                .copied()
                .unwrap_or(local.capture_zone);
            self.capture_ns = Some(key.namespace);
            self.retain(key, captured);
            if vacant {
                added += 1;
            }
        }
        self.capture_zone = saved_zone;
        self.capture_ns = saved_ns;
        added
    }

    pub fn absorb_local(&mut self, local: Self) {
        let _ = self.absorb(local);
    }

    pub fn capture_xanim_iw5(
        &mut self,
        s: &fastfile_iw5::ZoneStream<'_>,
        strings: &fastfile_iw5::ScriptStrings,
        geometry: fastfile_iw5::XAnimPartsGeometry,
    ) {
        let Some(name_ptr) = geometry.name else {
            self.capture_gaps += 1;
            return;
        };
        let Ok(name) = s.cstr(name_ptr) else {
            self.capture_gaps += 1;
            return;
        };
        if name.is_empty() {
            self.capture_gaps += 1;
            return;
        }

        let track_count = geometry.bone_count[9] as usize;
        let mut names = Vec::with_capacity(track_count);
        if let Some(arr) = geometry.names {
            for i in 0..track_count {
                let sid = s.u16_at(arr, i * 2).unwrap_or(0);
                names.push(strings.get(s, sid).unwrap_or("").to_owned());
            }
        } else if track_count > 0 {
            self.capture_gaps += 1;
            return;
        }

        let mut notifies = Vec::with_capacity(geometry.notify_count);
        if let Some(arr) = geometry.notify {
            for i in 0..geometry.notify_count {
                let b = arr.at(i * fastfile_iw5::size::XANIM_NOTIFY_INFO);
                let sid = s.u16_at(b, 0).unwrap_or(0);
                let time = s.f32_at(b, 4).unwrap_or(0.0);
                notifies.push(ClipNotify {
                    name: strings.get(s, sid).unwrap_or("").to_owned(),
                    time,
                });
            }
        }

        let data_byte = copy_u8_iw5(s, geometry.data_byte, geometry.data_byte_count);
        let data_short = copy_u16_iw5(s, geometry.data_short, geometry.data_short_count);
        let data_int = copy_u32_iw5(s, geometry.data_int, geometry.data_int_count);
        let random_data_short = copy_u16_iw5(
            s,
            geometry.random_data_short,
            geometry.random_data_short_count,
        );
        let random_data_byte = copy_u8_iw5(
            s,
            geometry.random_data_byte,
            geometry.random_data_byte_count,
        );
        let random_data_int =
            copy_u32_iw5(s, geometry.random_data_int, geometry.random_data_int_count);
        let indices = if geometry.indices_are_bytes {
            copy_u8_iw5(s, geometry.indices, geometry.index_count)
                .into_iter()
                .map(u16::from)
                .collect()
        } else {
            copy_u16_iw5(s, geometry.indices, geometry.index_count)
        };

        self.insert_in(
            AssetNamespace::Iw5,
            CapturedXAnim {
                namespace: AssetNamespace::Iw5,
                parts: Arc::new(RawXAnimParts {
                    name: name.to_owned(),
                    data_byte,
                    data_short,
                    data_int,
                    random_data_byte,
                    random_data_short,
                    random_data_int,
                    numframes: geometry.numframes,
                    flags: geometry.flags,
                    bone_count: geometry.bone_count,
                    framerate: geometry.framerate,
                    names,
                    notifies,
                    indices,
                    delta_trans: None,
                }),
            },
        );
    }

    pub fn capture_xanim_t5(
        &mut self,
        s: &fastfile_t5::ZoneStream<'_>,
        strings: &fastfile_t5::ScriptStrings,
        geometry: fastfile_t5::XAnimPartsGeometry,
    ) {
        let Some(name_ptr) = geometry.name else {
            self.capture_gaps += 1;
            return;
        };
        let Ok(name) = s.cstr(name_ptr) else {
            self.capture_gaps += 1;
            return;
        };
        if name.is_empty() {
            self.capture_gaps += 1;
            return;
        }

        let track_count = geometry.bone_count[9] as usize;
        let mut names = Vec::with_capacity(track_count);
        if let Some(arr) = geometry.names {
            for i in 0..track_count {
                let sid = s.u16_at(arr, i * 2).unwrap_or(0);
                names.push(strings.get(s, sid).unwrap_or("").to_owned());
            }
        } else if track_count > 0 {
            self.capture_gaps += 1;
            return;
        }

        let mut notifies = Vec::with_capacity(geometry.notify_count);
        if let Some(arr) = geometry.notify {
            for i in 0..geometry.notify_count {
                let b = arr.at(i * fastfile_t5::size::XANIM_NOTIFY_INFO);
                let sid = s.u16_at(b, 0).unwrap_or(0);
                let time = s.f32_at(b, 4).unwrap_or(0.0);
                notifies.push(ClipNotify {
                    name: strings.get(s, sid).unwrap_or("").to_owned(),
                    time,
                });
            }
        }

        let data_byte = copy_u8_t5(s, geometry.data_byte, geometry.data_byte_count);
        let data_short = copy_u16_t5(s, geometry.data_short, geometry.data_short_count);
        let data_int = copy_u32_t5(s, geometry.data_int, geometry.data_int_count);
        let random_data_short = copy_u16_t5(
            s,
            geometry.random_data_short,
            geometry.random_data_short_count,
        );
        let random_data_byte = copy_u8_t5(
            s,
            geometry.random_data_byte,
            geometry.random_data_byte_count,
        );
        let random_data_int =
            copy_u32_t5(s, geometry.random_data_int, geometry.random_data_int_count);
        let indices = if geometry.indices_are_bytes {
            copy_u8_t5(s, geometry.indices, geometry.index_count)
                .into_iter()
                .map(u16::from)
                .collect()
        } else {
            copy_u16_t5(s, geometry.indices, geometry.index_count)
        };

        self.insert_in(
            AssetNamespace::T5,
            CapturedXAnim {
                namespace: AssetNamespace::T5,
                parts: Arc::new(RawXAnimParts {
                    name: name.to_owned(),
                    data_byte,
                    data_short,
                    data_int,
                    random_data_byte,
                    random_data_short,
                    random_data_int,
                    numframes: geometry.numframes,
                    flags: geometry.flags,
                    bone_count: geometry.bone_count,
                    framerate: geometry.framerate,
                    names,
                    notifies,
                    indices,
                    delta_trans: None,
                }),
            },
        );
    }
}

impl XAnimBuild {
    pub fn capture_xanim_t6(
        &mut self,
        ns: AssetNamespace,
        prefix: &str,
        load: &fastfile_t6::ZoneLoad,
        asset: &fastfile_t6::LoadedAsset,
    ) -> bool {
        const SIZE: usize = 104;
        let h = &asset.header;
        if asset.ty != fastfile_t6::AssetType::XAnimParts || h.len() < SIZE {
            return false;
        }
        let u16_at = |o: usize| u16::from_le_bytes([h[o], h[o + 1]]);
        let u32_at = |o: usize| u32::from_le_bytes(h[o..o + 4].try_into().unwrap());
        let ptr = |o: usize| {
            let raw = u32_at(o);
            (raw != 0 && raw < 0xFFFF_FFFE).then(|| fastfile_t6::Ptr {
                block: ((raw - 1) >> 29) as u8,
                offset: (raw - 1) & 0x1FFF_FFFF,
            })
        };
        let bytes = |o: usize, len: usize| -> Vec<u8> {
            ptr(o)
                .and_then(|p| load.blocks.bytes(p, len).ok())
                .map_or_else(Vec::new, <[u8]>::to_vec)
        };
        let words = |o: usize, count: usize| -> Vec<u16> {
            bytes(o, 2 * count)
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect()
        };
        let dwords = |o: usize, count: usize| -> Vec<u32> {
            bytes(o, 4 * count)
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_le_bytes(*c))
                .collect()
        };
        let Some(name) = ptr(0)
            .and_then(|p| load.blocks.cstr(p).ok())
            .and_then(|b| core::str::from_utf8(b).ok())
            .filter(|name| !name.is_empty())
        else {
            self.capture_gaps += 1;
            return false;
        };
        let numframes = u16_at(14);
        let mut bone_count = [0u8; 10];
        bone_count.copy_from_slice(&h[24..34]);
        let names = words(64, usize::from(bone_count[9]))
            .into_iter()
            .map(|id| load.script_string(id).unwrap_or("").to_owned())
            .collect();
        let notifies = bytes(96, 8 * usize::from(h[34]))
            .as_chunks::<8>()
            .0
            .iter()
            .map(|c| ClipNotify {
                name: load
                    .script_string(u16::from_le_bytes([c[0], c[1]]))
                    .unwrap_or("")
                    .to_owned(),
                time: f32::from_le_bytes([c[4], c[5], c[6], c[7]]),
            })
            .collect();
        let index_count = u32_at(44) as usize;
        let indices = if numframes < 256 {
            bytes(92, index_count).into_iter().map(u16::from).collect()
        } else {
            words(92, index_count)
        };
        let parts = RawXAnimParts {
            name: format!("{prefix}{}", name.to_ascii_lowercase()),
            data_byte: bytes(68, usize::from(u16_at(4))),
            data_short: words(72, usize::from(u16_at(6))),
            data_int: dwords(76, usize::from(u16_at(8))),
            random_data_short: words(80, u32_at(40) as usize),
            random_data_byte: bytes(84, usize::from(u16_at(10))),
            random_data_int: dwords(88, usize::from(u16_at(12))),
            numframes,
            flags: u8::from(h[16] != 0) | (u8::from(h[17] != 0) << 1),
            bone_count,
            framerate: f32::from_le_bytes(h[48..52].try_into().unwrap()),
            names,
            notifies,
            indices,
            delta_trans: None,
        };
        self.insert_in(
            ns,
            CapturedXAnim {
                namespace: ns,
                parts: Arc::new(parts),
            },
        );
        true
    }

    pub fn absorb_vacant(&mut self, mut local: Self) -> (Vec<String>, usize) {
        let order = std::mem::take(&mut local.catalog.order);
        let entries = std::mem::take(&mut local.catalog.entries);
        let (mut added, mut kept) = (Vec::new(), 0);
        for (key, captured) in order.into_iter().zip(entries) {
            if self.catalog.has_key(&key) {
                kept += 1;
            } else {
                added.push(key.name.clone());
                self.retain(key, captured);
            }
        }
        (added, kept)
    }
}

impl AssetLinkSink for XAnimBuild {
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

    fn capture_xanim(&mut self, s: &ZoneStream<'_>, geometry: XAnimPartsGeometry) -> Result<()> {
        let Some(name_ptr) = geometry.name else {
            self.capture_gaps += 1;
            return Ok(());
        };
        let Ok(name) = s.cstr(name_ptr) else {
            self.capture_gaps += 1;
            return Ok(());
        };
        if name.is_empty() {
            self.capture_gaps += 1;
            return Ok(());
        }

        let track_count = geometry.bone_count[9] as usize;
        let mut names = Vec::with_capacity(track_count);
        if let Some(arr) = geometry.names {
            for i in 0..track_count {
                let sid = s.u16_at(arr, i * 2).unwrap_or(0);
                names.push(self.strings.get(s, sid).unwrap_or("").to_owned());
            }
        } else if track_count > 0 {
            self.capture_gaps += 1;
            return Ok(());
        }

        let mut notifies = Vec::with_capacity(geometry.notify_count);
        if let Some(arr) = geometry.notify {
            for i in 0..geometry.notify_count {
                let b = arr.at(i * sz::XANIM_NOTIFY_INFO);
                let sid = s.u16_at(b, 0).unwrap_or(0);
                let time = s.f32_at(b, 4).unwrap_or(0.0);
                notifies.push(ClipNotify {
                    name: self.strings.get(s, sid).unwrap_or("").to_owned(),
                    time,
                });
            }
        }

        let data_byte = copy_u8(s, geometry.data_byte, geometry.data_byte_count);
        let data_short = copy_u16(s, geometry.data_short, geometry.data_short_count);
        let data_int = copy_u32(s, geometry.data_int, geometry.data_int_count);
        let random_data_short = copy_u16(
            s,
            geometry.random_data_short,
            geometry.random_data_short_count,
        );
        let random_data_byte = copy_u8(
            s,
            geometry.random_data_byte,
            geometry.random_data_byte_count,
        );
        let random_data_int = copy_u32(s, geometry.random_data_int, geometry.random_data_int_count);
        let indices = if geometry.indices_are_bytes {
            copy_u8(s, geometry.indices, geometry.index_count)
                .into_iter()
                .map(u16::from)
                .collect()
        } else {
            copy_u16(s, geometry.indices, geometry.index_count)
        };
        let delta_trans = copy_delta_trans(s, geometry.delta_trans);

        self.insert_captured(CapturedXAnim {
            namespace: AssetNamespace::Iw4,
            parts: Arc::new(RawXAnimParts {
                name: name.to_owned(),
                data_byte,
                data_short,
                data_int,
                random_data_byte,
                random_data_short,
                random_data_int,
                numframes: geometry.numframes,
                flags: geometry.flags,
                bone_count: geometry.bone_count,
                framerate: geometry.framerate,
                names,
                notifies,
                indices,
                delta_trans,
            }),
        });
        let _ = geometry.frequency;
        Ok(())
    }
}

fn ascii_lower(name: &str) -> String {
    name.to_ascii_lowercase()
}

fn copy_u8(s: &ZoneStream<'_>, ptr: Option<Ptr>, count: usize) -> Vec<u8> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    s.slice_at(p, 0, count)
        .map(|b| b.to_vec())
        .unwrap_or_default()
}

fn copy_u16(s: &ZoneStream<'_>, ptr: Option<Ptr>, count: usize) -> Vec<u16> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    let Ok(bytes) = s.slice_at(p, 0, count * 2) else {
        return Vec::new();
    };
    bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect()
}

fn copy_u32(s: &ZoneStream<'_>, ptr: Option<Ptr>, count: usize) -> Vec<u32> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    let Ok(bytes) = s.slice_at(p, 0, count * 4) else {
        return Vec::new();
    };
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn copy_f32_3(s: &ZoneStream<'_>, ptr: Ptr, off: usize) -> Option<[f32; 3]> {
    let bytes = s.slice_at(ptr, off, 12).ok()?;
    Some([
        f32::from_le_bytes(bytes[0..4].try_into().ok()?),
        f32::from_le_bytes(bytes[4..8].try_into().ok()?),
        f32::from_le_bytes(bytes[8..12].try_into().ok()?),
    ])
}

fn copy_delta_trans(
    s: &ZoneStream<'_>,
    geo: fastfile_iw4::XAnimDeltaTransGeometry,
) -> Option<RawDeltaTrans> {
    if let Some(constant) = geo.constant {
        let mins = copy_f32_3(s, constant, 0)?;
        return Some(RawDeltaTrans {
            size: 0,
            small: geo.small != 0,
            mins,
            step: [0.0; 3],
            indices: Vec::new(),
            packed: Vec::new(),
        });
    }
    let mins_step = geo.mins_step?;
    let mins = copy_f32_3(s, mins_step, 0)?;
    let step = copy_f32_3(s, mins_step, 12)?;
    let n = geo.size as usize + 1;
    let indices = if geo.indices_are_bytes {
        copy_u8(s, geo.indices, n)
            .into_iter()
            .map(u16::from)
            .collect()
    } else {
        copy_u16(s, geo.indices, n)
    };
    let packed_n = if geo.small != 0 { 3 * n } else { 6 * n };
    let packed = copy_u8(s, geo.frames, packed_n);
    Some(RawDeltaTrans {
        size: geo.size,
        small: geo.small != 0,
        mins,
        step,
        indices,
        packed,
    })
}

fn copy_u8_iw5(
    s: &fastfile_iw5::ZoneStream<'_>,
    ptr: Option<fastfile_iw5::Ptr>,
    count: usize,
) -> Vec<u8> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    s.slice_at(p, 0, count)
        .map(|b| b.to_vec())
        .unwrap_or_default()
}

fn copy_u16_iw5(
    s: &fastfile_iw5::ZoneStream<'_>,
    ptr: Option<fastfile_iw5::Ptr>,
    count: usize,
) -> Vec<u16> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    let Ok(bytes) = s.slice_at(p, 0, count * 2) else {
        return Vec::new();
    };
    bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect()
}

fn copy_u32_iw5(
    s: &fastfile_iw5::ZoneStream<'_>,
    ptr: Option<fastfile_iw5::Ptr>,
    count: usize,
) -> Vec<u32> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    let Ok(bytes) = s.slice_at(p, 0, count * 4) else {
        return Vec::new();
    };
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn copy_u8_t5(
    s: &fastfile_t5::ZoneStream<'_>,
    ptr: Option<fastfile_t5::Ptr>,
    count: usize,
) -> Vec<u8> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    s.slice_at(p, 0, count)
        .map(|b| b.to_vec())
        .unwrap_or_default()
}

fn copy_u16_t5(
    s: &fastfile_t5::ZoneStream<'_>,
    ptr: Option<fastfile_t5::Ptr>,
    count: usize,
) -> Vec<u16> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    let Ok(bytes) = s.slice_at(p, 0, count * 2) else {
        return Vec::new();
    };
    bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect()
}

fn copy_u32_t5(
    s: &fastfile_t5::ZoneStream<'_>,
    ptr: Option<fastfile_t5::Ptr>,
    count: usize,
) -> Vec<u32> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    let Ok(bytes) = s.slice_at(p, 0, count * 4) else {
        return Vec::new();
    };
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}
