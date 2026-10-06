use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use crate::asset_type::AssetType;
use crate::content::XFILE_HEADER_LEN;
use crate::envelope::MAX_XFILE_COUNT;
use crate::schema::{AssetSchema, BinOp, Eval, LoadFn, Op, ScalarKind, Schema, StartLoad};

pub const XFILE_BLOCK_TEMP: u8 = 0;
pub const XFILE_BLOCK_VIRTUAL: u8 = 5;

const BLOCK_SHIFT: u32 = 29;
const OFFSET_MASK: u32 = (1 << BLOCK_SHIFT) - 1;
const PTR_FOLLOWING: u32 = 0xFFFF_FFFF;
const PTR_INSERT: u32 = 0xFFFF_FFFE;
const INSERT_BLOCK: u8 = XFILE_BLOCK_VIRTUAL;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockKind {
    Temp,
    Runtime,
    Delay,
    Normal,
}

const BLOCK_KINDS: [BlockKind; MAX_XFILE_COUNT] = [
    BlockKind::Temp,
    BlockKind::Runtime,
    BlockKind::Runtime,
    BlockKind::Delay,
    BlockKind::Delay,
    BlockKind::Normal,
    BlockKind::Normal,
    BlockKind::Normal,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ptr {
    pub block: u8,
    pub offset: u32,
}

impl Ptr {
    pub fn at(self, delta: u32) -> Ptr {
        Ptr {
            block: self.block,
            offset: self.offset.wrapping_add(delta),
        }
    }

    fn encode(self) -> u32 {
        ((u32::from(self.block) << BLOCK_SHIFT) | self.offset) + 1
    }

    fn decode(raw: u32) -> Option<Ptr> {
        if raw == 0 || raw == PTR_FOLLOWING || raw == PTR_INSERT {
            return None;
        }
        let e = raw - 1;
        Some(Ptr {
            block: (e >> BLOCK_SHIFT) as u8,
            offset: e & OFFSET_MASK,
        })
    }
}

pub(crate) fn decode_ptr(raw: u32) -> Option<Ptr> {
    Ptr::decode(raw)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalkError {
    Truncated { at: usize },
    BlockOverflow { block: u8, end: usize, size: usize },
    BadPointer { raw: u32 },
    Misplaced { expected: Ptr, got: Ptr },
    DelayBlock,
    StackUnderflow,
    NegativeCount(i64),
    DivideByZero,
    UnknownAssetType(u32),
    NoLoader(AssetType),
    Schema,
}

pub type Result<T> = core::result::Result<T, WalkError>;

pub struct ZoneBlocks {
    blocks: [Vec<u8>; MAX_XFILE_COUNT],
}

impl ZoneBlocks {
    fn slice(&self, p: Ptr, len: usize) -> Result<&[u8]> {
        let block = self
            .blocks
            .get(p.block as usize)
            .ok_or(WalkError::BadPointer { raw: p.encode() })?;
        let start = p.offset as usize;
        block
            .get(start..start + len)
            .ok_or(WalkError::BlockOverflow {
                block: p.block,
                end: start + len,
                size: block.len(),
            })
    }

    pub fn bytes(&self, p: Ptr, len: usize) -> Result<&[u8]> {
        self.slice(p, len)
    }

    pub fn u32_at(&self, p: Ptr) -> Result<u32> {
        Ok(u32::from_le_bytes(self.slice(p, 4)?.try_into().unwrap()))
    }

    pub fn ptr_at(&self, p: Ptr) -> Result<Option<Ptr>> {
        let raw = self.u32_at(p)?;
        if raw == 0 {
            return Ok(None);
        }
        Ptr::decode(raw)
            .map(Some)
            .ok_or(WalkError::BadPointer { raw })
    }

    pub fn cstr(&self, p: Ptr) -> Result<&[u8]> {
        let block = &self.blocks[p.block as usize];
        let rest = block
            .get(p.offset as usize..)
            .ok_or(WalkError::BadPointer { raw: p.encode() })?;
        let len = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or(WalkError::BlockOverflow {
                block: p.block,
                end: block.len() + 1,
                size: block.len(),
            })?;
        Ok(&rest[..len])
    }

    fn write_u32(&mut self, p: Ptr, v: u32) -> Result<()> {
        let size = self.blocks[p.block as usize].len();
        let start = p.offset as usize;
        let dst = self.blocks[p.block as usize]
            .get_mut(start..start + 4)
            .ok_or(WalkError::BlockOverflow {
                block: p.block,
                end: start + 4,
                size,
            })?;
        dst.copy_from_slice(&v.to_le_bytes());
        Ok(())
    }
}

pub struct LoadedAsset {
    pub ty: AssetType,
    pub list_index: Option<usize>,
    pub header: Vec<u8>,
    pub fields: Vec<(u32, usize)>,
    pub bound: Vec<(Ptr, usize)>,
    pub image_data: Option<LoadedImage>,
}

pub struct LoadedImage {
    pub level_count: u8,
    pub flags: u8,
    pub format: u32,
    pub payload: Vec<u8>,
}

impl LoadedAsset {
    pub fn bound_at(&self, slot: Ptr) -> Option<usize> {
        self.bound
            .binary_search_by(|(at, _)| at.cmp(&slot))
            .ok()
            .map(|i| self.bound[i].1)
    }

    pub fn field(&self, offset: u32) -> Option<usize> {
        self.fields
            .iter()
            .find(|(at, _)| *at == offset)
            .map(|&(_, index)| index)
    }
}

pub struct ZoneLoad {
    pub blocks: ZoneBlocks,
    pub assets: Vec<LoadedAsset>,
    pub asset_slots: BTreeMap<Ptr, usize>,
    pub script_strings: Option<(Ptr, usize)>,
}

impl ZoneLoad {
    pub fn asset_at(&self, slot: Ptr) -> Option<&LoadedAsset> {
        self.asset_slots.get(&slot).map(|&i| &self.assets[i])
    }

    pub fn asset_in(&self, owner: &LoadedAsset, slot: Ptr) -> Option<&LoadedAsset> {
        owner
            .bound_at(slot)
            .map(|i| &self.assets[i])
            .or_else(|| self.asset_at(slot))
    }

    pub fn script_string(&self, id: u16) -> Option<&str> {
        let (arr, count) = self.script_strings?;
        if usize::from(id) >= count {
            return None;
        }
        match self.blocks.ptr_at(arr.at(4 * u32::from(id))).ok()? {
            Some(p) => core::str::from_utf8(self.blocks.cstr(p).ok()?).ok(),
            None => Some(""),
        }
    }
}

fn loader_for(ty: AssetType) -> Option<&'static str> {
    use AssetType as T;
    Some(match ty {
        T::PhysPreset => "PhysPreset",
        T::PhysConstraints => "PhysConstraints",
        T::DestructibleDef => "DestructibleDef",
        T::XAnimParts => "XAnimParts",
        T::XModel => "XModel",
        T::Material => "Material",
        T::TechniqueSet => "MaterialTechniqueSet",
        T::Image => "GfxImage",
        T::SoundBank => "SndBank",
        T::SoundPatch => "SndPatch",
        T::ClipMap | T::ClipMapPvs => "clipMap_t",
        T::ComWorld => "ComWorld",
        T::GameWorldSp => "GameWorldSp",
        T::GameWorldMp => "GameWorldMp",
        T::MapEnts => "MapEnts",
        T::GfxWorld => "GfxWorld",
        T::LightDef => "GfxLightDef",
        T::Font => "Font_s",
        T::FontIcon => "FontIcon",
        T::MenuList => "MenuList",
        T::Menu => "menuDef_t",
        T::LocalizeEntry => "LocalizeEntry",
        T::Weapon => "WeaponVariantDef",
        T::Attachment => "WeaponAttachment",
        T::AttachmentUnique => "WeaponAttachmentUnique",
        T::WeaponCamo => "WeaponCamo",
        T::SndDriverGlobals => "SndDriverGlobals",
        T::Fx => "FxEffectDef",
        T::ImpactFx => "FxImpactTable",
        T::RawFile => "RawFile",
        T::StringTable => "StringTable",
        T::Leaderboard => "LeaderboardDef",
        T::XGlobals => "XGlobals",
        T::Ddl => "ddlRoot_t",
        T::Glasses => "Glasses",
        T::EmblemSet => "EmblemSet",
        T::Script => "ScriptParseTree",
        T::KeyValuePairs => "KeyValuePairs",
        T::Vehicle => "VehicleDef",
        T::MemoryBlock => "MemoryBlock",
        T::AddonMapEnts => "AddonMapEnts",
        T::Tracer => "TracerDef",
        T::SkinnedVerts => "SkinnedVertsDef",
        T::Qdb => "Qdb",
        T::Slug => "Slug",
        T::FootstepTable => "FootstepTableDef",
        T::FootstepFxTable => "FootstepFXTableDef",
        T::ZBarrier => "ZBarrierDef",
        _ => return None,
    })
}

fn asset_type_for(name: &str) -> Option<AssetType> {
    (0..64)
        .filter_map(AssetType::from_u32)
        .find(|&t| loader_for(t) == Some(name))
}

struct Walker<'a> {
    schema: &'a Schema,
    src: &'a [u8],
    pos: usize,
    mem: ZoneBlocks,
    offsets: [usize; MAX_XFILE_COUNT],
    stack: Vec<u8>,
    temp_saved: Vec<usize>,
    assets: Vec<LoadedAsset>,
    asset_slots: BTreeMap<Ptr, usize>,
    slot_log: Vec<(Ptr, usize)>,
    addresses: BTreeMap<Ptr, usize>,
    script_strings: Option<(Ptr, usize)>,
}

struct Frame<'s> {
    asset: &'s AssetSchema,
    vars: Vec<Option<Ptr>>,
}

impl<'a> Walker<'a> {
    fn top(&self) -> Result<u8> {
        self.stack.last().copied().ok_or(WalkError::StackUnderflow)
    }

    fn cursor(&self) -> Result<Ptr> {
        let block = self.top()?;
        Ok(Ptr {
            block,
            offset: self.offsets[block as usize] as u32,
        })
    }

    fn push(&mut self, block: u8) {
        if BLOCK_KINDS[block as usize] == BlockKind::Temp {
            self.temp_saved.push(self.offsets[block as usize]);
        }
        self.stack.push(block);
    }

    fn pop(&mut self) -> Result<()> {
        let block = self.stack.pop().ok_or(WalkError::StackUnderflow)?;
        if BLOCK_KINDS[block as usize] == BlockKind::Temp {
            self.offsets[block as usize] =
                self.temp_saved.pop().ok_or(WalkError::StackUnderflow)?;
        }
        Ok(())
    }

    fn align_block(&mut self, block: u8, align: usize) -> Result<()> {
        let b = block as usize;
        if align > 1 {
            self.offsets[b] = self.offsets[b].next_multiple_of(align);
        }
        if self.offsets[b] > self.mem.blocks[b].len() {
            return Err(WalkError::BlockOverflow {
                block,
                end: self.offsets[b],
                size: self.mem.blocks[b].len(),
            });
        }
        Ok(())
    }

    fn alloc(&mut self, align: usize) -> Result<Ptr> {
        let block = self.top()?;
        self.align_block(block, align)?;
        self.cursor()
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let at = self.pos;
        let bytes = self
            .src
            .get(at..at + len)
            .ok_or(WalkError::Truncated { at })?;
        self.pos += len;
        Ok(bytes)
    }

    fn load(&mut self, dst: Ptr, len: usize) -> Result<()> {
        let cur = self.cursor()?;
        if cur != dst {
            return Err(WalkError::Misplaced {
                expected: cur,
                got: dst,
            });
        }
        let b = dst.block as usize;
        let start = dst.offset as usize;
        let size = self.mem.blocks[b].len();
        if start + len > size {
            return Err(WalkError::BlockOverflow {
                block: dst.block,
                end: start + len,
                size,
            });
        }
        match BLOCK_KINDS[b] {
            BlockKind::Temp | BlockKind::Normal => {
                let bytes = self.take(len)?;
                self.mem.blocks[b][start..start + len].copy_from_slice(bytes);
            }
            BlockKind::Runtime => self.mem.blocks[b][start..start + len].fill(0),
            BlockKind::Delay => return Err(WalkError::DelayBlock),
        }
        self.offsets[b] += len;
        Ok(())
    }

    fn load_null_terminated(&mut self) -> Result<Ptr> {
        let at = self.cursor()?;
        let b = at.block as usize;
        let rest = self
            .src
            .get(self.pos..)
            .ok_or(WalkError::Truncated { at: self.pos })?;
        let len = rest
            .iter()
            .position(|&c| c == 0)
            .ok_or(WalkError::Truncated { at: self.pos })?
            + 1;
        let start = at.offset as usize;
        let size = self.mem.blocks[b].len();
        if start + len > size {
            return Err(WalkError::BlockOverflow {
                block: at.block,
                end: start + len,
                size,
            });
        }
        self.mem.blocks[b][start..start + len].copy_from_slice(&rest[..len]);
        self.pos += len;
        self.offsets[b] += len;
        Ok(at)
    }

    fn insert_pointer(&mut self) -> Result<Ptr> {
        self.align_block(INSERT_BLOCK, 4)?;
        let b = INSERT_BLOCK as usize;
        let at = Ptr {
            block: INSERT_BLOCK,
            offset: self.offsets[b] as u32,
        };
        if self.offsets[b] + 4 > self.mem.blocks[b].len() {
            return Err(WalkError::BlockOverflow {
                block: INSERT_BLOCK,
                end: self.offsets[b] + 4,
                size: self.mem.blocks[b].len(),
            });
        }
        self.offsets[b] += 4;
        Ok(at)
    }

    fn deref(&self, p: Ptr) -> Result<Ptr> {
        let raw = self.mem.u32_at(p)?;
        Ptr::decode(raw).ok_or(WalkError::BadPointer { raw })
    }

    fn eval(&self, frame: &Frame<'_>, e: &Eval) -> Result<i64> {
        Ok(match e {
            Eval::Num(n) => *n,
            Eval::Op(op, a, b) => {
                let a = self.eval(frame, a)?;
                let b = self.eval(frame, b)?;
                match op {
                    BinOp::Add => a.wrapping_add(b),
                    BinOp::Sub => a.wrapping_sub(b),
                    BinOp::Mul => a.wrapping_mul(b),
                    BinOp::Div => a.checked_div(b).ok_or(WalkError::DivideByZero)?,
                    BinOp::Rem => a.checked_rem(b).ok_or(WalkError::DivideByZero)?,
                    BinOp::And => a & b,
                    BinOp::Xor => a ^ b,
                    BinOp::Or => a | b,
                    BinOp::Shl => a.wrapping_shl(b as u32),
                    BinOp::Shr => a.wrapping_shr(b as u32),
                    BinOp::Gt => i64::from(a > b),
                    BinOp::Ge => i64::from(a >= b),
                    BinOp::Lt => i64::from(a < b),
                    BinOp::Le => i64::from(a <= b),
                    BinOp::Eq => i64::from(a == b),
                    BinOp::Ne => i64::from(a != b),
                    BinOp::LogicAnd => i64::from(a != 0 && b != 0),
                    BinOp::LogicOr => i64::from(a != 0 || b != 0),
                }
            }
            Eval::Var {
                var,
                steps,
                indices,
                size,
                kind,
            } => {
                let mut p = frame.vars[*var].ok_or(WalkError::Schema)?;
                for &(off, deref) in steps {
                    p = p.at(off);
                    if deref {
                        p = self.deref(p)?;
                    }
                }
                for (idx, stride) in indices {
                    let i = self.eval(frame, idx)?;
                    p = p.at((i as u32).wrapping_mul(*stride));
                }
                let b = self.mem.bytes(p, *size as usize)?;
                match (kind, size) {
                    (ScalarKind::Float, 4) => f32::from_le_bytes(b.try_into().unwrap()) as i64,
                    (ScalarKind::Signed, 1) => i64::from(b[0] as i8),
                    (ScalarKind::Signed, 2) => i64::from(i16::from_le_bytes(b.try_into().unwrap())),
                    (ScalarKind::Signed, 4) => i64::from(i32::from_le_bytes(b.try_into().unwrap())),
                    (ScalarKind::Unsigned, 1) => i64::from(b[0]),
                    (ScalarKind::Unsigned, 2) => {
                        i64::from(u16::from_le_bytes(b.try_into().unwrap()))
                    }
                    (ScalarKind::Unsigned, 4) => {
                        i64::from(u32::from_le_bytes(b.try_into().unwrap()))
                    }
                    _ => return Err(WalkError::Schema),
                }
            }
        })
    }

    fn count(&self, frame: &Frame<'_>, e: &Eval) -> Result<usize> {
        let n = self.eval(frame, e)?;
        usize::try_from(n).map_err(|_| WalkError::NegativeCount(n))
    }

    fn access(&self, base: Ptr, off: u32, deref: bool) -> Result<Ptr> {
        let slot = base.at(off);
        if deref { self.deref(slot) } else { Ok(slot) }
    }

    fn run_load(&mut self, frame: &mut Frame<'a>, load: usize, start: bool) -> Result<()> {
        let f: &'a LoadFn = &frame.asset.loads[load];
        let at = frame.vars[f.var].ok_or(WalkError::Schema)?;
        if start {
            match f.start {
                StartLoad::Full => self.load(at, f.size as usize)?,
                StartLoad::Partial(n) => self.load(at, n as usize)?,
                StartLoad::None => {}
            }
        }
        self.run_body(frame, f.var, &f.body, false)
    }

    fn run_body(
        &mut self,
        frame: &mut Frame<'a>,
        var: usize,
        body: &'a [Op],
        insert: bool,
    ) -> Result<()> {
        for op in body {
            self.run_op(frame, var, op, insert)?;
        }
        Ok(())
    }

    fn run_op(
        &mut self,
        frame: &mut Frame<'a>,
        var: usize,
        op: &'a Op,
        insert: bool,
    ) -> Result<()> {
        let base = {
            let v = frame.vars[var];
            move || v.ok_or(WalkError::Schema)
        };
        match op {
            Op::Push(b) => self.push(*b),
            Op::Pop => self.pop()?,
            Op::Nop => {}
            Op::If(cond, body) => {
                if self.eval(frame, cond)? != 0 {
                    self.run_body(frame, var, body, insert)?;
                }
            }
            Op::Chain(cases) => {
                for (cond, body) in cases {
                    let take = match cond {
                        Some(c) => self.eval(frame, c)? != 0,
                        None => true,
                    };
                    if take {
                        self.run_body(frame, var, body, insert)?;
                        break;
                    }
                }
            }
            Op::Block(b, body) => {
                self.push(*b);
                self.run_body(frame, var, body, insert)?;
                self.pop()?;
            }
            Op::IfNonZero(off, body) => {
                if self.mem.u32_at(base()?.at(*off))? != 0 {
                    self.run_body(frame, var, body, insert)?;
                }
            }
            Op::Reuse { off, temp, body } => {
                let slot = base()?.at(*off);
                let raw = self.mem.u32_at(slot)?;
                let follows = raw == PTR_FOLLOWING || (*temp && raw == PTR_INSERT);
                if follows {
                    self.run_body(frame, var, body, raw == PTR_INSERT)?;
                } else if *temp {
                    let target = Ptr::decode(raw).ok_or(WalkError::BadPointer { raw })?;
                    let aliased = self.mem.u32_at(target)?;
                    self.mem.write_u32(slot, aliased)?;
                    if let Some(&i) = self.asset_slots.get(&target) {
                        self.bind_slot(slot, i);
                    }
                }
            }
            Op::Alloc {
                off,
                align,
                temp,
                body,
            } => {
                let slot = base()?.at(*off);
                let align = self.count(frame, align)?;
                let at = self.alloc(align)?;
                self.mem.write_u32(slot, at.encode())?;
                let insert_slot = if *temp && insert {
                    Some(self.insert_pointer()?)
                } else {
                    None
                };
                self.run_body(frame, var, body, false)?;
                if let Some(ins) = insert_slot {
                    let v = self.mem.u32_at(slot)?;
                    self.mem.write_u32(ins, v)?;
                }
            }
            Op::XString(off) => self.load_xstring(base()?.at(*off))?,
            Op::XStringArray {
                off,
                deref,
                start,
                count,
            } => {
                let arr = self.access(base()?, *off, *deref)?;
                let n = self.count(frame, count)?;
                if *start {
                    self.load(arr, 4 * n)?;
                }
                for i in 0..n {
                    self.load_xstring(arr.at(4 * i as u32))?;
                }
            }
            Op::AssetLoad { off, asset } => {
                let slot = base()?.at(*off);
                self.load_asset(*asset, slot, None)?;
            }
            Op::PtrArray {
                off,
                deref,
                func,
                start,
                count,
            } => {
                let arr = self.access(base()?, *off, *deref)?;
                let n = self.count(frame, count)?;
                self.run_ptr_array(frame, *func, arr, *start, n)?;
            }
            Op::Array {
                off,
                deref,
                func,
                start,
                count,
            } => {
                let arr = self.access(base()?, *off, *deref)?;
                let n = self.count(frame, count)?;
                self.run_array(frame, *func, arr, *start, n)?;
            }
            Op::Single { off, load } => {
                let p = self.deref(base()?.at(*off))?;
                let v = frame.asset.loads[*load].var;
                frame.vars[v] = Some(p);
                self.run_load(frame, *load, true)?;
            }
            Op::Embedded { off, load, start } => {
                let p = base()?.at(*off);
                let v = frame.asset.loads[*load].var;
                frame.vars[v] = Some(p);
                self.run_load(frame, *load, *start)?;
            }
            Op::Raw {
                off,
                deref,
                elem,
                count,
            } => {
                let p = self.access(base()?, *off, *deref)?;
                let n = self.count(frame, count)?;
                self.load(p, n * *elem as usize)?;
            }
        }
        Ok(())
    }

    fn run_array(
        &mut self,
        frame: &mut Frame<'a>,
        func: usize,
        arr: Ptr,
        start: bool,
        n: usize,
    ) -> Result<()> {
        let f = &frame.asset.arrays[func];
        let (size, load) = (f.size, f.load);
        if start {
            self.load(arr, size as usize * n)?;
        }
        let v = frame.asset.loads[load].var;
        for i in 0..n {
            frame.vars[v] = Some(arr.at(size * i as u32));
            self.run_load(frame, load, false)?;
        }
        Ok(())
    }

    fn run_ptr_array(
        &mut self,
        frame: &mut Frame<'a>,
        func: usize,
        arr: Ptr,
        start: bool,
        n: usize,
    ) -> Result<()> {
        let f = &frame.asset.ptr_arrays[func];
        if start {
            self.load(arr, 4 * n)?;
        }
        for i in 0..n {
            let slot = arr.at(4 * i as u32);
            let raw = self.mem.u32_at(slot)?;
            if raw == 0 {
                continue;
            }
            if let Some(asset) = f.asset {
                self.load_asset(asset, slot, None)?;
                continue;
            }
            if f.reusable && raw != PTR_FOLLOWING {
                continue;
            }
            let align = self.count(frame, &f.align)?;
            let at = self.alloc(align)?;
            self.mem.write_u32(slot, at.encode())?;
            match f.load {
                Some(load) => {
                    let v = frame.asset.loads[load].var;
                    frame.vars[v] = Some(at);
                    self.run_load(frame, load, true)?;
                }
                None => self.load(at, f.size as usize)?,
            }
        }
        Ok(())
    }

    fn load_xstring(&mut self, slot: Ptr) -> Result<()> {
        let raw = self.mem.u32_at(slot)?;
        if raw == PTR_FOLLOWING {
            self.alloc(1)?;
            let at = self.load_null_terminated()?;
            self.mem.write_u32(slot, at.encode())?;
        }
        Ok(())
    }

    fn bind_slot(&mut self, slot: Ptr, index: usize) {
        self.asset_slots.insert(slot, index);
        self.slot_log.push((slot, index));
    }

    fn load_asset(&mut self, asset: usize, slot: Ptr, list_index: Option<usize>) -> Result<()> {
        let schema: &'a AssetSchema = &self.schema.assets[asset];
        let log_start = self.slot_log.len();
        let in_temp = schema.root_in_temp;
        if in_temp {
            self.push(XFILE_BLOCK_TEMP);
        }
        let raw = self.mem.u32_at(slot)?;
        if raw != 0 {
            let follows = raw == PTR_FOLLOWING || (in_temp && raw == PTR_INSERT);
            if follows {
                let mut frame = Frame {
                    asset: schema,
                    vars: vec![None; schema.var_count],
                };
                let align = self.count(&frame, &schema.root_align)?;
                let at = self.alloc(align)?;
                self.mem.write_u32(slot, at.encode())?;
                let insert_slot = if raw == PTR_INSERT {
                    Some(self.insert_pointer()?)
                } else {
                    None
                };
                let root = &schema.loads[schema.root];
                frame.vars[root.var] = Some(at);
                self.run_load(&mut frame, schema.root, true)?;

                let ty = asset_type_for(&schema.name).ok_or(WalkError::Schema)?;
                let header = self.mem.bytes(at, root.size as usize)?.to_vec();
                let image_data = if ty == AssetType::Image {
                    match self.mem.ptr_at(at)? {
                        Some(data) => {
                            let definition = self.mem.bytes(data, 12)?;
                            let format = u32::from_le_bytes(definition[4..8].try_into().unwrap());
                            let len =
                                u32::from_le_bytes(definition[8..12].try_into().unwrap()) as usize;
                            Some(LoadedImage {
                                level_count: definition[0],
                                flags: definition[1],
                                format,
                                payload: self.mem.bytes(data.at(12), len)?.to_vec(),
                            })
                        }
                        None => None,
                    }
                } else {
                    None
                };
                let end = at.at(root.size);
                let mut fields: Vec<(u32, usize)> = Vec::new();
                let mut bound: BTreeMap<Ptr, usize> = BTreeMap::new();
                for &(slot, index) in &self.slot_log[log_start..] {
                    bound.insert(slot, index);
                    if (at..end).contains(&slot) {
                        let offset = slot.offset - at.offset;
                        fields.retain(|(o, _)| *o != offset);
                        fields.push((offset, index));
                    }
                }
                let bound = bound.into_iter().collect();
                let index = self.assets.len();
                self.assets.push(LoadedAsset {
                    ty,
                    list_index,
                    header,
                    fields,
                    bound,
                    image_data,
                });
                self.bind_slot(slot, index);
                self.addresses.insert(at, index);
                if let Some(ins) = insert_slot {
                    self.mem.write_u32(ins, at.encode())?;
                    self.bind_slot(ins, index);
                }
            } else if in_temp {
                let target = Ptr::decode(raw).ok_or(WalkError::BadPointer { raw })?;
                let aliased = self.mem.u32_at(target)?;
                self.mem.write_u32(slot, aliased)?;
                if let Some(&i) = self.asset_slots.get(&target) {
                    self.bind_slot(slot, i);
                }
            } else if let Some(&i) = Ptr::decode(raw).and_then(|at| self.addresses.get(&at)) {
                self.bind_slot(slot, i);
            }
        }
        if in_temp {
            self.pop()?;
        }
        Ok(())
    }
}

pub fn load_zone(
    schema: &Schema,
    image: &[u8],
    mut on_asset: impl FnMut(usize, AssetType) -> bool,
) -> (ZoneLoad, Result<()>) {
    let Some(header) = crate::content::ZoneHeader::parse(image) else {
        let empty = ZoneLoad {
            blocks: ZoneBlocks {
                blocks: Default::default(),
            },
            assets: Vec::new(),
            asset_slots: BTreeMap::new(),
            script_strings: None,
        };
        return (empty, Err(WalkError::Truncated { at: 0 }));
    };
    let mut w = Walker {
        schema,
        src: image,
        pos: XFILE_HEADER_LEN,
        mem: ZoneBlocks {
            blocks: core::array::from_fn(|i| vec![0u8; header.block_size[i] as usize]),
        },
        offsets: [0; MAX_XFILE_COUNT],
        stack: Vec::new(),
        temp_saved: Vec::new(),
        assets: Vec::new(),
        asset_slots: BTreeMap::new(),
        slot_log: Vec::new(),
        addresses: BTreeMap::new(),
        script_strings: None,
    };
    let result = walk_list(&mut w, &mut on_asset);
    let load = ZoneLoad {
        blocks: w.mem,
        assets: w.assets,
        asset_slots: w.asset_slots,
        script_strings: w.script_strings,
    };
    (load, result)
}

fn walk_list(
    w: &mut Walker<'_>,
    on_asset: &mut impl FnMut(usize, AssetType) -> bool,
) -> Result<()> {
    let head = w.take(24)?;
    let rd = |o: usize| u32::from_le_bytes(head[o..o + 4].try_into().unwrap());
    let (string_count, strings, depend_count, depends, asset_count, assets) = (
        rd(0) as usize,
        rd(4),
        rd(8) as usize,
        rd(12),
        rd(16) as usize,
        rd(20),
    );

    w.push(XFILE_BLOCK_VIRTUAL);
    for (list, (count, ptr)) in [(string_count, strings), (depend_count, depends)]
        .into_iter()
        .enumerate()
    {
        if ptr != 0 {
            let arr = w.alloc(4)?;
            if list == 0 {
                w.script_strings = Some((arr, count));
            }
            w.load(arr, 4 * count)?;
            for i in 0..count {
                w.load_xstring(arr.at(4 * i as u32))?;
            }
        }
    }
    if assets != 0 {
        let arr = w.alloc(4)?;
        w.load(arr, 8 * asset_count)?;
        for i in 0..asset_count {
            let entry = arr.at(8 * i as u32);
            let raw = w.mem.u32_at(entry)?;
            let ty = AssetType::from_u32(raw).ok_or(WalkError::UnknownAssetType(raw))?;
            let name = loader_for(ty).ok_or(WalkError::NoLoader(ty))?;
            let asset = w
                .schema
                .asset_by_name(name)
                .ok_or(WalkError::NoLoader(ty))?;
            w.load_asset(asset, entry.at(4), Some(i))?;
            if !on_asset(i, ty) {
                break;
            }
        }
    }
    w.pop()
}
