use alloc::vec::Vec;
use core::fmt;

const CUSTOMDATA: u16 = 0x35;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramKind {
    Pixel,
    Vertex,
    Geometry,
    Hull,
    Domain,
    Compute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramError {
    UnalignedBytes(usize),
    Truncated { at_word: usize },
    UnknownKind(u32),
    ZeroLengthInstruction { at_word: usize },
}

impl fmt::Display for ProgramError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnalignedBytes(len) => write!(f, "program length {len} is not DWORD-aligned"),
            Self::Truncated { at_word } => write!(f, "program is truncated at word {at_word}"),
            Self::UnknownKind(kind) => write!(f, "unknown program type {kind}"),
            Self::ZeroLengthInstruction { at_word } => {
                write!(f, "instruction at word {at_word} has length 0")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Instruction<'a> {
    pub opcode: u16,
    pub tokens: &'a [u32],
}

impl Instruction<'_> {
    pub fn name(&self) -> OpcodeName {
        OpcodeName(self.opcode)
    }

    pub fn extended(&self) -> bool {
        self.tokens[0] & 0x8000_0000 != 0
    }

    pub fn controls(&self) -> u32 {
        (self.tokens[0] >> 11) & 0x1fff
    }
}

#[derive(Clone, Debug)]
pub struct Program {
    pub kind: ProgramKind,
    pub major: u8,
    pub minor: u8,
    pub words: Vec<u32>,
}

impl Program {
    pub fn parse(chunk: &[u8]) -> Result<Self, ProgramError> {
        if !chunk.len().is_multiple_of(4) {
            return Err(ProgramError::UnalignedBytes(chunk.len()));
        }
        let words: Vec<u32> = chunk
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| u32::from_le_bytes(*b))
            .collect();
        let version = *words
            .first()
            .ok_or(ProgramError::Truncated { at_word: 0 })?;
        let length = *words.get(1).ok_or(ProgramError::Truncated { at_word: 1 })? as usize;
        let kind = match version >> 16 {
            0 => ProgramKind::Pixel,
            1 => ProgramKind::Vertex,
            2 => ProgramKind::Geometry,
            3 => ProgramKind::Hull,
            4 => ProgramKind::Domain,
            5 => ProgramKind::Compute,
            other => return Err(ProgramError::UnknownKind(other)),
        };
        if length > words.len() {
            return Err(ProgramError::Truncated { at_word: length });
        }
        Ok(Self {
            kind,
            major: ((version >> 4) & 0xf) as u8,
            minor: (version & 0xf) as u8,
            words: words[..length].to_vec(),
        })
    }

    pub fn instructions(&self) -> Result<Vec<Instruction<'_>>, ProgramError> {
        let mut out = Vec::new();
        let mut at = 2;
        while at < self.words.len() {
            let token = self.words[at];
            let opcode = (token & 0x7ff) as u16;
            let length = if opcode == CUSTOMDATA {
                *self
                    .words
                    .get(at + 1)
                    .ok_or(ProgramError::Truncated { at_word: at + 1 })? as usize
            } else {
                ((token >> 24) & 0x7f) as usize
            };
            if length == 0 {
                return Err(ProgramError::ZeroLengthInstruction { at_word: at });
            }
            let tokens = self
                .words
                .get(at..at + length)
                .ok_or(ProgramError::Truncated { at_word: at })?;
            out.push(Instruction { opcode, tokens });
            at += length;
        }
        Ok(out)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpcodeName(pub u16);

const NAMES: [&str; 0x6b] = [
    "add",
    "and",
    "break",
    "breakc",
    "call",
    "callc",
    "case",
    "continue",
    "continuec",
    "cut",
    "default",
    "deriv_rtx",
    "deriv_rty",
    "discard",
    "div",
    "dp2",
    "dp3",
    "dp4",
    "else",
    "emit",
    "emitthencut",
    "endif",
    "endloop",
    "endswitch",
    "eq",
    "exp",
    "frc",
    "ftoi",
    "ftou",
    "ge",
    "iadd",
    "if",
    "ieq",
    "ige",
    "ilt",
    "imad",
    "imax",
    "imin",
    "imul",
    "ine",
    "ineg",
    "ishl",
    "ishr",
    "itof",
    "label",
    "ld",
    "ld_ms",
    "log",
    "loop",
    "lt",
    "mad",
    "min",
    "max",
    "customdata",
    "mov",
    "movc",
    "mul",
    "ne",
    "nop",
    "not",
    "or",
    "resinfo",
    "ret",
    "retc",
    "round_ne",
    "round_ni",
    "round_pi",
    "round_z",
    "rsq",
    "sample",
    "sample_c",
    "sample_c_lz",
    "sample_l",
    "sample_d",
    "sample_b",
    "sqrt",
    "switch",
    "sincos",
    "udiv",
    "ult",
    "uge",
    "umul",
    "umad",
    "umax",
    "umin",
    "ushr",
    "utof",
    "xor",
    "dcl_resource",
    "dcl_constantbuffer",
    "dcl_sampler",
    "dcl_index_range",
    "dcl_gs_output_topology",
    "dcl_gs_input_primitive",
    "dcl_max_output_vertex_count",
    "dcl_input",
    "dcl_input_sgv",
    "dcl_input_siv",
    "dcl_input_ps",
    "dcl_input_ps_sgv",
    "dcl_input_ps_siv",
    "dcl_output",
    "dcl_output_sgv",
    "dcl_output_siv",
    "dcl_temps",
    "dcl_indexable_temp",
    "dcl_global_flags",
];

impl fmt::Display for OpcodeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            op if (op as usize) < NAMES.len() => f.write_str(NAMES[op as usize]),
            0x6c => f.write_str("lod"),
            0x6d => f.write_str("gather4"),
            0x6e => f.write_str("sample_pos"),
            0x6f => f.write_str("sample_info"),
            0x7a => f.write_str("deriv_rtx_coarse"),
            0x7b => f.write_str("deriv_rtx_fine"),
            0x7c => f.write_str("deriv_rty_coarse"),
            0x7d => f.write_str("deriv_rty_fine"),
            0x7e => f.write_str("gather4_c"),
            0x7f => f.write_str("gather4_po"),
            0x80 => f.write_str("gather4_po_c"),
            0x81 => f.write_str("rcp"),
            0x82 => f.write_str("f32tof16"),
            0x83 => f.write_str("f16tof32"),
            0x84 => f.write_str("uaddc"),
            0x85 => f.write_str("usubb"),
            0x86 => f.write_str("countbits"),
            0x87 => f.write_str("firstbit_hi"),
            0x88 => f.write_str("firstbit_lo"),
            0x89 => f.write_str("firstbit_shi"),
            0x8a => f.write_str("ubfe"),
            0x8b => f.write_str("ibfe"),
            0x8c => f.write_str("bfi"),
            0x8d => f.write_str("bfrev"),
            0x8e => f.write_str("swapc"),
            op => write!(f, "op_{op:#x}"),
        }
    }
}
