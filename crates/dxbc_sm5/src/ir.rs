use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;

use crate::program::{Instruction, OpcodeName};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrError {
    Truncated {
        opcode: OpcodeName,
    },
    UnsupportedIndex {
        opcode: OpcodeName,
        representation: u32,
    },
    Trailing {
        opcode: OpcodeName,
        words: usize,
    },
}

impl fmt::Display for IrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { opcode } => write!(f, "{opcode}: operand tokens run out"),
            Self::UnsupportedIndex {
                opcode,
                representation,
            } => write!(f, "{opcode}: index representation {representation}"),
            Self::Trailing { opcode, words } => {
                write!(f, "{opcode}: {words} words left after its operands")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RegisterType {
    Temp,
    Input,
    Output,
    IndexableTemp,
    Immediate32,
    Immediate64,
    Sampler,
    Resource,
    ConstantBuffer,
    ImmediateConstantBuffer,
    Label,
    InputPrimitiveId,
    OutputDepth,
    Null,
    Other(u32),
}

impl RegisterType {
    fn from_raw(raw: u32) -> Self {
        match raw {
            0 => Self::Temp,
            1 => Self::Input,
            2 => Self::Output,
            3 => Self::IndexableTemp,
            4 => Self::Immediate32,
            5 => Self::Immediate64,
            6 => Self::Sampler,
            7 => Self::Resource,
            8 => Self::ConstantBuffer,
            9 => Self::ImmediateConstantBuffer,
            10 => Self::Label,
            11 => Self::InputPrimitiveId,
            12 => Self::OutputDepth,
            13 => Self::Null,
            other => Self::Other(other),
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            Self::Temp => "r",
            Self::Input => "v",
            Self::Output => "o",
            Self::IndexableTemp => "x",
            Self::Immediate32 | Self::Immediate64 => "l",
            Self::Sampler => "s",
            Self::Resource => "t",
            Self::ConstantBuffer => "cb",
            Self::ImmediateConstantBuffer => "icb",
            Self::Label => "label",
            Self::InputPrimitiveId => "vPrim",
            Self::OutputDepth => "oDepth",
            Self::Null => "null",
            Self::Other(_) => "?",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Components {
    None,
    One,
    Mask(u8),
    Swizzle([u8; 4]),
    Select(u8),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Index {
    Immediate(u32),
    Relative(Box<Operand>),
    ImmediatePlusRelative(u32, Box<Operand>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Modifier {
    pub negate: bool,
    pub absolute: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Operand {
    pub register: RegisterType,
    pub components: Components,
    pub indices: Vec<Index>,
    pub modifier: Modifier,
    pub immediate: Vec<u32>,
}

impl Operand {
    pub fn register_number(&self) -> Option<u32> {
        match self.indices.first()? {
            Index::Immediate(n) => Some(*n),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecodedInstruction {
    pub opcode: OpcodeName,
    pub controls: u32,
    pub extended: Vec<u32>,
    pub operands: Vec<Operand>,
    pub extra: Vec<u32>,
}

impl DecodedInstruction {
    pub fn saturate(&self) -> bool {
        self.controls & 0x4 != 0
    }

    pub fn test_nonzero(&self) -> bool {
        self.controls & 0x80 != 0
    }
}

struct Cursor<'a> {
    tokens: &'a [u32],
    at: usize,
    opcode: OpcodeName,
}

impl Cursor<'_> {
    fn next(&mut self) -> Result<u32, IrError> {
        let token = *self.tokens.get(self.at).ok_or(IrError::Truncated {
            opcode: self.opcode,
        })?;
        self.at += 1;
        Ok(token)
    }

    fn remaining(&self) -> usize {
        self.tokens.len().saturating_sub(self.at)
    }

    fn operand(&mut self) -> Result<Operand, IrError> {
        let token = self.next()?;
        let mut modifier = Modifier::default();
        let mut extended = token & 0x8000_0000 != 0;
        while extended {
            let ext = self.next()?;
            extended = ext & 0x8000_0000 != 0;
            if ext & 0x3f == 1 {
                match (ext >> 6) & 0xff {
                    1 => modifier.negate = true,
                    2 => modifier.absolute = true,
                    3 => {
                        modifier.negate = true;
                        modifier.absolute = true;
                    }
                    _ => {}
                }
            }
        }
        let count = token & 0x3;
        let components = match count {
            0 => Components::None,
            1 => Components::One,
            2 => match (token >> 2) & 0x3 {
                0 => Components::Mask(((token >> 4) & 0xf) as u8),
                1 => Components::Swizzle(core::array::from_fn(|i| {
                    ((token >> (4 + 2 * i)) & 0x3) as u8
                })),
                _ => Components::Select(((token >> 4) & 0x3) as u8),
            },
            _ => Components::None,
        };
        let register = RegisterType::from_raw((token >> 12) & 0xff);
        let dimension = ((token >> 20) & 0x3) as usize;
        let mut indices = Vec::with_capacity(dimension);
        for i in 0..dimension {
            let representation = (token >> (22 + 3 * i)) & 0x7;
            indices.push(match representation {
                0 => Index::Immediate(self.next()?),
                2 => Index::Relative(Box::new(self.operand()?)),
                3 => {
                    let base = self.next()?;
                    Index::ImmediatePlusRelative(base, Box::new(self.operand()?))
                }
                other => {
                    return Err(IrError::UnsupportedIndex {
                        opcode: self.opcode,
                        representation: other,
                    });
                }
            });
        }
        let mut immediate = Vec::new();
        match register {
            RegisterType::Immediate32 => {
                let n = if count == 1 { 1 } else { 4 };
                for _ in 0..n {
                    immediate.push(self.next()?);
                }
            }
            RegisterType::Immediate64 => {
                let n = if count == 1 { 2 } else { 4 };
                for _ in 0..n {
                    immediate.push(self.next()?);
                }
            }
            _ => {}
        }
        Ok(Operand {
            register,
            components,
            indices,
            modifier,
            immediate,
        })
    }
}

fn raw_layout(opcode: u16) -> Option<RawLayout> {
    Some(match opcode {
        0x35 => RawLayout::AllRaw { skip: 2 },
        0x68 => RawLayout::AllRaw { skip: 1 },
        0x6a => RawLayout::AllRaw { skip: 1 },
        0x69 => RawLayout::AllRaw { skip: 1 },
        0x58 => RawLayout::OperandThenRaw,
        0x60 | 0x61 | 0x63 | 0x64 | 0x66 | 0x67 => RawLayout::OperandThenRaw,
        _ => return None,
    })
}

enum RawLayout {
    AllRaw { skip: usize },
    OperandThenRaw,
}

pub fn decode_instruction(instruction: &Instruction<'_>) -> Result<DecodedInstruction, IrError> {
    let opcode = instruction.name();
    let tokens = instruction.tokens;
    let mut cursor = Cursor {
        tokens,
        at: 1,
        opcode,
    };
    let mut extended = Vec::new();
    if instruction.opcode != 0x35 {
        let mut more = instruction.extended();
        while more {
            let token = cursor.next()?;
            more = token & 0x8000_0000 != 0;
            extended.push(token);
        }
    }
    let mut operands = Vec::new();
    let mut extra = Vec::new();
    match raw_layout(instruction.opcode) {
        Some(RawLayout::AllRaw { skip }) => {
            extra.extend_from_slice(tokens.get(skip..).unwrap_or(&[]));
            cursor.at = tokens.len();
        }
        Some(RawLayout::OperandThenRaw) => {
            operands.push(cursor.operand()?);
            extra.extend_from_slice(&tokens[cursor.at..]);
            cursor.at = tokens.len();
        }
        None => {
            while cursor.remaining() > 0 {
                operands.push(cursor.operand()?);
            }
        }
    }
    if cursor.remaining() > 0 {
        return Err(IrError::Trailing {
            opcode,
            words: cursor.remaining(),
        });
    }
    Ok(DecodedInstruction {
        opcode,
        controls: instruction.controls(),
        extended,
        operands,
        extra,
    })
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let wrap_abs = self.modifier.absolute;
        if self.modifier.negate {
            f.write_str("-")?;
        }
        if wrap_abs {
            f.write_str("|")?;
        }
        if matches!(self.register, RegisterType::Immediate32) {
            f.write_str("l(")?;
            for (i, bits) in self.immediate.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                let value = f32::from_bits(*bits);
                if value.is_finite() && (value == 0.0 || value.abs() > 1e-6) && value.abs() < 1e9 {
                    write!(f, "{value}")?;
                } else {
                    write!(f, "{bits:#x}")?;
                }
            }
            f.write_str(")")?;
        } else {
            f.write_str(self.register.prefix())?;
            for (i, index) in self.indices.iter().enumerate() {
                let open = if i == 0
                    && !matches!(
                        self.register,
                        RegisterType::IndexableTemp
                            | RegisterType::ConstantBuffer
                            | RegisterType::ImmediateConstantBuffer
                    ) {
                    ""
                } else {
                    "["
                };
                let close = if open.is_empty() { "" } else { "]" };
                match index {
                    Index::Immediate(n) => write!(f, "{open}{n}{close}")?,
                    Index::Relative(op) => write!(f, "[{op}]")?,
                    Index::ImmediatePlusRelative(n, op) => write!(f, "[{op} + {n}]")?,
                }
            }
        }
        const XYZW: [char; 4] = ['x', 'y', 'z', 'w'];
        match self.components {
            Components::Mask(mask) if mask != 0xf => {
                f.write_str(".")?;
                for (i, c) in XYZW.iter().enumerate() {
                    if mask & (1 << i) != 0 {
                        write!(f, "{c}")?;
                    }
                }
            }
            Components::Swizzle(s) if s != [0, 1, 2, 3] => {
                f.write_str(".")?;
                for c in s {
                    write!(f, "{}", XYZW[c as usize])?;
                }
            }
            Components::Select(c) => write!(f, ".{}", XYZW[c as usize])?,
            _ => {}
        }
        if wrap_abs {
            f.write_str("|")?;
        }
        Ok(())
    }
}

impl fmt::Display for DecodedInstruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.opcode)?;
        if self.saturate() {
            f.write_str("_sat")?;
        }
        if matches!(self.opcode.0, 0x1f | 0x03 | 0x0d | 0x08 | 0x3f) {
            f.write_str(if self.test_nonzero() { "_nz" } else { "_z" })?;
        }
        for (i, operand) in self.operands.iter().enumerate() {
            f.write_str(if i == 0 { " " } else { ", " })?;
            write!(f, "{operand}")?;
        }
        if !self.extra.is_empty() && self.opcode.0 != 0x35 {
            write!(f, " {:x?}", self.extra)?;
        }
        Ok(())
    }
}
