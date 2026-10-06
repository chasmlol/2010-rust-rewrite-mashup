use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::{self, Write};

use crate::ir::{Components, DecodedInstruction, Index, Operand, RegisterType};
use crate::program::{Program, ProgramKind};
use crate::reflection::Reflection;
use crate::signature::Signature;
use crate::{Container, decode_instruction};

pub const VERTEX_ENTRY: &str = "sm3_vertex_main";
pub const FRAGMENT_ENTRY: &str = "sm3_fragment_main";
pub const TEXTURE_TABLE_GROUP: u32 = 1;
pub const TEXTURE_TABLE_BINDING_2D: u32 = 0;
pub const TEXTURE_TABLE_BINDING_CUBE: u32 = 1;
pub const TEXTURE_TABLE_BINDING_3D: u32 = 2;
pub const TEXTURE_TABLE_BINDING_SAMPLERS: u32 = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WgslError {
    Container(String),
    Decode(String),
    WrongStage {
        expected: ProgramKind,
        found: ProgramKind,
    },
    UnsupportedOpcode(String),
    UnsupportedOperand(String),
    UnassignedConstant {
        buffer: u32,
        row: u32,
    },
    UnassignedTexture {
        texture: u32,
        sampler: u32,
    },
    UndeclaredResource(u32),
    MissingPosition,
    UnsupportedPixelOutput(u32),
    UnbalancedControlFlow,
}

impl fmt::Display for WgslError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Container(e) => write!(f, "container: {e}"),
            Self::Decode(e) => write!(f, "decode: {e}"),
            Self::WrongStage { expected, found } => {
                write!(f, "expected a {expected:?} program, found {found:?}")
            }
            Self::UnsupportedOpcode(op) => write!(f, "unsupported instruction `{op}`"),
            Self::UnsupportedOperand(op) => write!(f, "unsupported operand `{op}`"),
            Self::UnassignedConstant { buffer, row } => {
                write!(f, "cb{buffer}[{row}] has no arena row")
            }
            Self::UnassignedTexture { texture, sampler } => {
                write!(f, "t{texture}/s{sampler} has no texture slot")
            }
            Self::UndeclaredResource(t) => write!(f, "t{t} has no dcl_resource"),
            Self::MissingPosition => f.write_str("vertex shader writes no SV_Position"),
            Self::UnsupportedPixelOutput(r) => write!(f, "pixel output o{r} is not supported"),
            Self::UnbalancedControlFlow => f.write_str("if/else/loop nesting is unbalanced"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TextureDimension {
    D2,
    Cube,
    D3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConstantRow {
    pub buffer: u32,
    pub row: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureSlot {
    pub texture: u32,
    pub sampler: u32,
    pub dimension: TextureDimension,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VertexInput {
    pub register: u32,
    pub location: u32,
    pub attribute_type: String,
    pub expression: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampleAdapter {
    pub slot: usize,
    pub opaque_alpha: bool,
    pub square_rgb: bool,
    pub rgb_scale: f32,
    pub scale_row: Option<ConstantRow>,
    pub alpha_row: Option<ConstantRow>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PassAbi {
    pub vertex_inputs: Vec<VertexInput>,
    pub vertex_constants: Vec<ConstantRow>,
    pub pixel_constants: Vec<ConstantRow>,
    pub textures: Vec<TextureSlot>,
    pub sample_adapters: Vec<SampleAdapter>,
    pub alpha_tests: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub struct Shader {
    pub kind: ProgramKind,
    pub reflection: Reflection,
    pub input: Signature,
    pub output: Signature,
    pub instructions: Vec<DecodedInstruction>,
    pub temps: u32,
    pub resources: BTreeMap<u32, TextureDimension>,
}

impl Shader {
    pub fn parse(bytes: &[u8]) -> Result<Self, WgslError> {
        let container = Container::parse(bytes).map_err(|e| WgslError::Container(e.to_string()))?;
        let chunk = |fourcc: &[u8; 4]| container.chunk(fourcc);
        let reflection = chunk(b"RDEF")
            .map(Reflection::parse)
            .transpose()
            .map_err(|e| WgslError::Container(e.to_string()))?
            .unwrap_or_else(Reflection::default);
        let signature = |fourcc: &[u8; 4]| {
            chunk(fourcc)
                .map(|data| Signature::parse(data, 24))
                .transpose()
                .map_err(|e| WgslError::Container(e.to_string()))
                .map(|signature| signature.unwrap_or_else(Signature::default))
        };
        let input = signature(b"ISGN")?;
        let output = signature(b"OSGN")?;
        let program = Program::parse(
            container
                .program()
                .ok_or_else(|| WgslError::Container("no SHDR/SHEX chunk".into()))?,
        )
        .map_err(|e| WgslError::Decode(e.to_string()))?;
        let mut instructions = Vec::new();
        let mut temps = 0;
        let mut resources = BTreeMap::new();
        for instruction in program
            .instructions()
            .map_err(|e| WgslError::Decode(e.to_string()))?
        {
            let decoded =
                decode_instruction(&instruction).map_err(|e| WgslError::Decode(e.to_string()))?;
            match decoded.opcode.0 {
                0x68 => temps = decoded.extra.first().copied().unwrap_or(0),
                0x58 => {
                    let dimension = match (decoded.controls) & 0x1f {
                        3 => TextureDimension::D2,
                        5 => TextureDimension::D3,
                        6 => TextureDimension::Cube,
                        other => {
                            return Err(WgslError::UnsupportedOperand(format!(
                                "resource dimension {other}"
                            )));
                        }
                    };
                    if let Some(t) = decoded.operands.first().and_then(Operand::register_number) {
                        resources.insert(t, dimension);
                    }
                }
                _ => {}
            }
            instructions.push(decoded);
        }
        Ok(Self {
            kind: program.kind,
            reflection,
            input,
            output,
            instructions,
            temps,
            resources,
        })
    }

    pub fn constant_rows(&self) -> Result<BTreeSet<ConstantRow>, WgslError> {
        let mut rows = BTreeSet::new();
        for instruction in self
            .instructions
            .iter()
            .filter(|i| !is_declaration(i.opcode.0))
        {
            for operand in &instruction.operands {
                collect_rows(operand, &mut rows)?;
            }
        }
        Ok(rows)
    }

    pub fn texture_slots(&self) -> Result<BTreeSet<TextureSlot>, WgslError> {
        let mut slots = BTreeSet::new();
        for instruction in &self.instructions {
            if let Some((t, s)) = sample_registers(instruction) {
                let dimension = *self
                    .resources
                    .get(&t)
                    .ok_or(WgslError::UndeclaredResource(t))?;
                slots.insert(TextureSlot {
                    texture: t,
                    sampler: s,
                    dimension,
                });
            }
        }
        Ok(slots)
    }
}

fn collect_rows(operand: &Operand, rows: &mut BTreeSet<ConstantRow>) -> Result<(), WgslError> {
    for index in &operand.indices {
        if let Index::Relative(inner) | Index::ImmediatePlusRelative(_, inner) = index {
            collect_rows(inner, rows)?;
        }
    }
    if operand.register == RegisterType::ConstantBuffer {
        match operand.indices.as_slice() {
            [Index::Immediate(buffer), Index::Immediate(row)] => {
                rows.insert(ConstantRow {
                    buffer: *buffer,
                    row: *row,
                });
            }
            _ => return Err(WgslError::UnsupportedOperand(format!("relative {operand}"))),
        }
    }
    Ok(())
}

fn sample_registers(instruction: &DecodedInstruction) -> Option<(u32, u32)> {
    match instruction.opcode.0 {
        0x45..=0x4a => Some((
            instruction.operands.get(2)?.register_number()?,
            instruction.operands.get(3)?.register_number()?,
        )),
        _ => None,
    }
}

fn is_declaration(opcode: u16) -> bool {
    (0x58..=0x6a).contains(&opcode) || opcode == 0x35
}

const XYZW: [char; 4] = ['x', 'y', 'z', 'w'];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Vertex,
    Pixel,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Float,
    Int,
    Uint,
}

struct Lowering<'a> {
    stage: Stage,
    out: String,
    indent: usize,
    shader: &'a Shader,
    constants: BTreeMap<ConstantRow, usize>,
    textures: &'a [TextureSlot],
    sample_adapters: &'a [SampleAdapter],
    epilogue: String,
}

impl Lowering<'_> {
    fn line(&mut self, text: &str) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn bits(&self, operand: &Operand) -> Result<String, WgslError> {
        let base = match operand.register {
            RegisterType::Temp => format!("r{}", reg(operand)?),
            RegisterType::Input => format!("v{}", reg(operand)?),
            RegisterType::Output => format!("o{}", reg(operand)?),
            RegisterType::ConstantBuffer => match operand.indices.as_slice() {
                [Index::Immediate(buffer), Index::Immediate(row)] => {
                    let row = ConstantRow {
                        buffer: *buffer,
                        row: *row,
                    };
                    if !self.constants.contains_key(&row) {
                        return Err(WgslError::UnassignedConstant {
                            buffer: row.buffer,
                            row: row.row,
                        });
                    }
                    format!("cb{}_{}", row.buffer, row.row)
                }
                _ => return Err(WgslError::UnsupportedOperand(operand.to_string())),
            },
            RegisterType::Immediate32 => {
                let v: Vec<u32> = if operand.immediate.len() == 1 {
                    alloc::vec![operand.immediate[0]; 4]
                } else {
                    operand.immediate.clone()
                };
                return Ok(format!(
                    "vec4<u32>({:#x}u, {:#x}u, {:#x}u, {:#x}u)",
                    v[0], v[1], v[2], v[3]
                ));
            }
            _ => return Err(WgslError::UnsupportedOperand(operand.to_string())),
        };
        Ok(match operand.components {
            Components::Swizzle(s) if s != [0, 1, 2, 3] => format!(
                "{base}.{}{}{}{}",
                XYZW[s[0] as usize], XYZW[s[1] as usize], XYZW[s[2] as usize], XYZW[s[3] as usize]
            ),
            Components::Select(c) => {
                let c = XYZW[c as usize];
                format!("{base}.{c}{c}{c}{c}")
            }
            Components::One => format!("{base}.xxxx"),
            _ => base,
        })
    }

    fn source(&self, operand: &Operand, kind: Kind) -> Result<String, WgslError> {
        let bits = self.bits(operand)?;
        let (ty, mut value) = match kind {
            Kind::Float => ("f32", format!("bitcast<vec4<f32>>({bits})")),
            Kind::Int => ("i32", format!("bitcast<vec4<i32>>({bits})")),
            Kind::Uint => ("u32", bits),
        };
        if operand.modifier.absolute && ty != "u32" {
            value = format!("abs({value})");
        }
        if operand.modifier.negate {
            value = match ty {
                "u32" => format!("(~{value} + vec4<u32>(1u))"),
                _ => format!("(-{value})"),
            };
        }
        Ok(value)
    }

    fn store(
        &mut self,
        destination: &Operand,
        value: &str,
        kind: Kind,
        saturate: bool,
    ) -> Result<(), WgslError> {
        if destination.register == RegisterType::Null {
            return Ok(());
        }
        let name = match destination.register {
            RegisterType::Temp => format!("r{}", reg(destination)?),
            RegisterType::Output => format!("o{}", reg(destination)?),
            _ => return Err(WgslError::UnsupportedOperand(destination.to_string())),
        };
        let mask = match destination.components {
            Components::Mask(mask) => mask,
            _ => 0xf,
        };
        let mut value = String::from(value);
        if saturate && kind == Kind::Float {
            value = format!("clamp({value}, vec4<f32>(0.0), vec4<f32>(1.0))");
        }
        let bits = match kind {
            Kind::Uint => value,
            _ => format!("bitcast<vec4<u32>>({value})"),
        };
        let mut text = format!("{{ let t = {bits};");
        for (i, c) in XYZW.iter().enumerate() {
            if mask & (1 << i) != 0 {
                write!(text, " {name}.{c} = t.{c};").unwrap();
            }
        }
        text.push_str(" }");
        self.line(&text);
        Ok(())
    }

    fn texture_slot(&self, t: u32, s: u32) -> Result<(usize, TextureDimension), WgslError> {
        self.textures
            .iter()
            .position(|slot| slot.texture == t && slot.sampler == s)
            .map(|i| (i, self.textures[i].dimension))
            .ok_or(WgslError::UnassignedTexture {
                texture: t,
                sampler: s,
            })
    }

    fn instruction(&mut self, i: &DecodedInstruction) -> Result<(), WgslError> {
        let op = |n: usize| {
            i.operands
                .get(n)
                .ok_or_else(|| WgslError::Decode(i.to_string()))
        };
        let sat = i.saturate();
        let float2 = |this: &Self, f: &dyn Fn(&str, &str) -> String| -> Result<String, WgslError> {
            Ok(f(
                &this.source(op(1)?, Kind::Float)?,
                &this.source(op(2)?, Kind::Float)?,
            ))
        };
        let mask_of =
            |cond: String| format!("select(vec4<u32>(0u), vec4<u32>(0xffffffffu), {cond})");
        match i.opcode.0 {
            _ if is_declaration(i.opcode.0) => {}
            0x00 => {
                let v = float2(self, &|a, b| format!("({a} + {b})"))?;
                self.store(op(0)?, &v, Kind::Float, sat)?;
            }
            0x38 => {
                let v = float2(self, &|a, b| format!("({a} * {b})"))?;
                self.store(op(0)?, &v, Kind::Float, sat)?;
            }
            0x0e => {
                let v = float2(self, &|a, b| format!("({a} / {b})"))?;
                self.store(op(0)?, &v, Kind::Float, sat)?;
            }
            0x33 => {
                let v = float2(self, &|a, b| format!("min({a}, {b})"))?;
                self.store(op(0)?, &v, Kind::Float, sat)?;
            }
            0x34 => {
                let v = float2(self, &|a, b| format!("max({a}, {b})"))?;
                self.store(op(0)?, &v, Kind::Float, sat)?;
            }
            0x32 => {
                let v = format!(
                    "({} * {} + {})",
                    self.source(op(1)?, Kind::Float)?,
                    self.source(op(2)?, Kind::Float)?,
                    self.source(op(3)?, Kind::Float)?
                );
                self.store(op(0)?, &v, Kind::Float, sat)?;
            }
            0x0f..=0x11 => {
                let n = match i.opcode.0 {
                    0x0f => "xy",
                    0x10 => "xyz",
                    _ => "xyzw",
                };
                let v = float2(self, &|a, b| format!("vec4<f32>(dot({a}.{n}, {b}.{n}))"))?;
                self.store(op(0)?, &v, Kind::Float, sat)?;
            }
            0x36 => {
                let source = op(1)?;
                if sat || source.modifier != Default::default() {
                    let v = self.source(source, Kind::Float)?;
                    self.store(op(0)?, &v, Kind::Float, sat)?;
                } else {
                    let v = self.bits(source)?;
                    self.store(op(0)?, &v, Kind::Uint, false)?;
                }
            }
            0x37 => {
                let cond = self.bits(op(1)?)?;
                let floaty = sat
                    || [2, 3].iter().any(|&n| {
                        i.operands
                            .get(n)
                            .is_some_and(|o| o.modifier != Default::default())
                    });
                let kind = if floaty { Kind::Float } else { Kind::Uint };
                let a = self.source(op(2)?, kind)?;
                let b = self.source(op(3)?, kind)?;
                let v = format!("select({b}, {a}, {cond} != vec4<u32>(0u))");
                self.store(op(0)?, &v, kind, sat)?;
            }
            0x18 | 0x1d | 0x31 | 0x39 => {
                let cmp = match i.opcode.0 {
                    0x18 => "==",
                    0x1d => ">=",
                    0x31 => "<",
                    _ => "!=",
                };
                let v = float2(self, &|a, b| mask_of(format!("{a} {cmp} {b}")))?;
                self.store(op(0)?, &v, Kind::Uint, false)?;
            }
            0x19
            | 0x1a
            | 0x2f
            | 0x44
            | 0x4b
            | 0x40
            | 0x41
            | 0x42
            | 0x43
            | 0x81
            | 0x0b
            | 0x0c
            | 0x7a..=0x7d => {
                let a = self.source(op(1)?, Kind::Float)?;
                let v = match i.opcode.0 {
                    0x19 => format!("exp2({a})"),
                    0x1a => format!("fract({a})"),
                    0x2f => format!("log2({a})"),
                    0x44 => format!("inverseSqrt({a})"),
                    0x4b => format!("sqrt({a})"),
                    0x40 => format!("round({a})"),
                    0x41 => format!("floor({a})"),
                    0x42 => format!("ceil({a})"),
                    0x43 => format!("trunc({a})"),
                    0x81 => format!("(vec4<f32>(1.0) / {a})"),
                    0x0b | 0x7a => format!("dpdx({a})"),
                    0x7b => format!("dpdxFine({a})"),
                    0x0c | 0x7c => format!("dpdy({a})"),
                    0x7d => format!("dpdyFine({a})"),
                    _ => unreachable!(),
                };
                self.store(op(0)?, &v, Kind::Float, sat)?;
            }
            0x4d => {
                let a = self.source(op(2)?, Kind::Float)?;
                self.store(op(0)?, &format!("sin({a})"), Kind::Float, sat)?;
                self.store(op(1)?, &format!("cos({a})"), Kind::Float, sat)?;
            }
            0x1e | 0x24 | 0x25 | 0x29 | 0x2a | 0x01 | 0x3c | 0x57 | 0x55 | 0x53 | 0x54 => {
                let kind = match i.opcode.0 {
                    0x1e | 0x24 | 0x25 | 0x2a => Kind::Int,
                    _ => Kind::Uint,
                };
                let a = self.source(op(1)?, kind)?;
                let b = self.source(op(2)?, Kind::Uint)?;
                let b_typed = self.source(op(2)?, kind)?;
                let v = match i.opcode.0 {
                    0x1e => format!("({a} + {b_typed})"),
                    0x24 => format!("max({a}, {b_typed})"),
                    0x25 => format!("min({a}, {b_typed})"),
                    0x29 => format!("({a} << ({b} & vec4<u32>(31u)))"),
                    0x2a => format!("({a} >> ({b} & vec4<u32>(31u)))"),
                    0x55 => format!("({a} >> ({b} & vec4<u32>(31u)))"),
                    0x01 => format!("({a} & {b})"),
                    0x3c => format!("({a} | {b})"),
                    0x57 => format!("({a} ^ {b})"),
                    0x53 => format!("max({a}, {b})"),
                    0x54 => format!("min({a}, {b})"),
                    _ => unreachable!(),
                };
                let v = if kind == Kind::Int {
                    format!("bitcast<vec4<u32>>({v})")
                } else {
                    v
                };
                self.store(op(0)?, &v, Kind::Uint, false)?;
            }
            0x23 => {
                let v = format!(
                    "bitcast<vec4<u32>>({} * {} + {})",
                    self.source(op(1)?, Kind::Int)?,
                    self.source(op(2)?, Kind::Int)?,
                    self.source(op(3)?, Kind::Int)?
                );
                self.store(op(0)?, &v, Kind::Uint, false)?;
            }
            0x3b => {
                let v = format!("(~{})", self.source(op(1)?, Kind::Uint)?);
                self.store(op(0)?, &v, Kind::Uint, false)?;
            }
            0x28 => {
                let v = format!("bitcast<vec4<u32>>(-{})", self.source(op(1)?, Kind::Int)?);
                self.store(op(0)?, &v, Kind::Uint, false)?;
            }
            0x20 | 0x21 | 0x22 | 0x27 | 0x4f | 0x50 => {
                let (kind, cmp) = match i.opcode.0 {
                    0x20 => (Kind::Int, "=="),
                    0x21 => (Kind::Int, ">="),
                    0x22 => (Kind::Int, "<"),
                    0x27 => (Kind::Int, "!="),
                    0x4f => (Kind::Uint, "<"),
                    _ => (Kind::Uint, ">="),
                };
                let v = mask_of(format!(
                    "{} {cmp} {}",
                    self.source(op(1)?, kind)?,
                    self.source(op(2)?, kind)?
                ));
                self.store(op(0)?, &v, Kind::Uint, false)?;
            }
            0x2b => {
                let v = format!("vec4<f32>({})", self.source(op(1)?, Kind::Int)?);
                self.store(op(0)?, &v, Kind::Float, false)?;
            }
            0x56 => {
                let v = format!("vec4<f32>({})", self.source(op(1)?, Kind::Uint)?);
                self.store(op(0)?, &v, Kind::Float, false)?;
            }
            0x1b => {
                let v = format!(
                    "bitcast<vec4<u32>>(vec4<i32>({}))",
                    self.source(op(1)?, Kind::Float)?
                );
                self.store(op(0)?, &v, Kind::Uint, false)?;
            }
            0x1c => {
                let v = format!("vec4<u32>({})", self.source(op(1)?, Kind::Float)?);
                self.store(op(0)?, &v, Kind::Uint, false)?;
            }
            0x1f => {
                let c = self.condition(op(0)?, i.test_nonzero())?;
                self.line(&format!("if ({c}) {{"));
                self.indent += 1;
            }
            0x12 => {
                self.indent = self
                    .indent
                    .checked_sub(1)
                    .ok_or(WgslError::UnbalancedControlFlow)?;
                self.line("} else {");
                self.indent += 1;
            }
            0x15 | 0x16 => {
                self.indent = self
                    .indent
                    .checked_sub(1)
                    .ok_or(WgslError::UnbalancedControlFlow)?;
                self.line("}");
            }
            0x30 => {
                self.line("loop {");
                self.indent += 1;
            }
            0x02 => self.line("break;"),
            0x07 => self.line("continue;"),
            0x03 | 0x08 | 0x3f | 0x0d => {
                let c = self.condition(op(0)?, i.test_nonzero())?;
                let body = match i.opcode.0 {
                    0x03 => "break;".to_string(),
                    0x08 => "continue;".to_string(),
                    0x0d => "discard;".to_string(),
                    _ => self.epilogue.clone(),
                };
                self.line(&format!("if ({c}) {{ {body} }}"));
            }
            0x3e => {
                if self.indent > 1 {
                    let epilogue = self.epilogue.clone();
                    self.line(&epilogue);
                }
            }
            0x45..=0x4a => self.sample(i)?,
            _ => return Err(WgslError::UnsupportedOpcode(i.opcode.to_string())),
        }
        Ok(())
    }

    fn condition(&self, operand: &Operand, nonzero: bool) -> Result<String, WgslError> {
        let bits = self.bits(operand)?;
        Ok(format!("{bits}.x {} 0u", if nonzero { "!=" } else { "==" }))
    }

    fn sample(&mut self, i: &DecodedInstruction) -> Result<(), WgslError> {
        let op = |n: usize| {
            i.operands
                .get(n)
                .ok_or_else(|| WgslError::Decode(i.to_string()))
        };
        let resource = op(2)?;
        let t = reg(resource)?;
        let s = reg(op(3)?)?;
        let (slot, dimension) = self.texture_slot(t, s)?;
        let coord = self.source(op(1)?, Kind::Float)?;
        let (array, coord) = match dimension {
            TextureDimension::D2 => ("dx_textures_2d", format!("{coord}.xy")),
            TextureDimension::Cube => ("dx_textures_cube", format!("{coord}.xyz")),
            TextureDimension::D3 => ("dx_textures_3d", format!("{coord}.xyz")),
        };
        let texture = format!("{array}[dx_slot_{slot} & 0xffffu]");
        let sampler = format!("dx_samplers[dx_slot_{slot} >> 16u]");
        let pixel = self.stage == Stage::Pixel;
        let sampled = match i.opcode.0 {
            0x45 if pixel => format!("textureSample({texture}, {sampler}, {coord})"),
            0x45 => format!("textureSampleLevel({texture}, {sampler}, {coord}, 0.0)"),
            0x48 => format!(
                "textureSampleLevel({texture}, {sampler}, {coord}, {}.x)",
                self.source(op(4)?, Kind::Float)?
            ),
            0x4a if pixel => format!(
                "textureSampleBias({texture}, {sampler}, {coord}, {}.x)",
                self.source(op(4)?, Kind::Float)?
            ),
            0x4a => format!("textureSampleLevel({texture}, {sampler}, {coord}, 0.0)"),
            0x49 => {
                let n = if dimension == TextureDimension::D2 {
                    "xy"
                } else {
                    "xyz"
                };
                format!(
                    "textureSampleGrad({texture}, {sampler}, {coord}, {}.{n}, {}.{n})",
                    self.source(op(4)?, Kind::Float)?,
                    self.source(op(5)?, Kind::Float)?
                )
            }
            0x46 | 0x47 => {
                let reference = self.source(op(4)?, Kind::Float)?;
                format!(
                    "vec4<f32>(select(0.0, 1.0, {reference}.x <= textureSampleLevel({texture}, {sampler}, {coord}, 0.0).x))"
                )
            }
            _ => unreachable!(),
        };
        let sampled = match self.sample_adapters.iter().find(|a| a.slot == slot) {
            Some(adapter) => {
                let row_lane = |row: Option<ConstantRow>, lane: &str| match row {
                    Some(row) if !self.constants.contains_key(&row) => {
                        Err(WgslError::UnassignedConstant {
                            buffer: row.buffer,
                            row: row.row,
                        })
                    }
                    Some(row) => Ok(Some(format!(
                        "bitcast<f32>(cb{}_{}.{lane})",
                        row.buffer, row.row
                    ))),
                    None => Ok(None),
                };
                let scale = match row_lane(adapter.scale_row, "x")? {
                    Some(lane) => format!("{:?} * {lane}", adapter.rgb_scale),
                    None => format!("{:?}", adapter.rgb_scale),
                };
                let alpha_weight = row_lane(adapter.alpha_row, "y")?.unwrap_or_else(|| {
                    String::from(if adapter.opaque_alpha { "0.0" } else { "1.0" })
                });
                format!(
                    "dx_adapt({sampled}, {alpha_weight}, {}, {scale})",
                    adapter.square_rgb
                )
            }
            None => sampled,
        };
        let swizzled = match resource.components {
            Components::Swizzle(s) if s != [0, 1, 2, 3] => format!(
                "({sampled}).{}{}{}{}",
                XYZW[s[0] as usize], XYZW[s[1] as usize], XYZW[s[2] as usize], XYZW[s[3] as usize]
            ),
            _ => sampled,
        };
        self.store(op(0)?, &swizzled, Kind::Float, i.saturate())
    }
}

fn reg(operand: &Operand) -> Result<u32, WgslError> {
    operand
        .register_number()
        .ok_or_else(|| WgslError::UnsupportedOperand(operand.to_string()))
}

fn output_registers(shader: &Shader) -> BTreeSet<u32> {
    shader
        .instructions
        .iter()
        .filter(|i| matches!(i.opcode.0, 0x65..=0x67))
        .filter_map(|i| i.operands.first()?.register_number())
        .collect()
}

fn input_registers(shader: &Shader) -> BTreeSet<u32> {
    shader
        .instructions
        .iter()
        .filter(|i| (0x5f..=0x64).contains(&i.opcode.0))
        .filter_map(|i| i.operands.first()?.register_number())
        .collect()
}

fn texture_tables(out: &mut String, textures: &[TextureSlot]) {
    for (dimension, binding, array, ty) in [
        (
            TextureDimension::D2,
            TEXTURE_TABLE_BINDING_2D,
            "dx_textures_2d",
            "texture_2d<f32>",
        ),
        (
            TextureDimension::Cube,
            TEXTURE_TABLE_BINDING_CUBE,
            "dx_textures_cube",
            "texture_cube<f32>",
        ),
        (
            TextureDimension::D3,
            TEXTURE_TABLE_BINDING_3D,
            "dx_textures_3d",
            "texture_3d<f32>",
        ),
    ] {
        if textures.iter().any(|slot| slot.dimension == dimension) {
            writeln!(
                out,
                "@group({TEXTURE_TABLE_GROUP}) @binding({binding}) var {array}: binding_array<{ty}>;"
            )
            .unwrap();
        }
    }
    writeln!(
        out,
        "@group({TEXTURE_TABLE_GROUP}) @binding({TEXTURE_TABLE_BINDING_SAMPLERS}) var dx_samplers: binding_array<sampler>;"
    )
    .unwrap();
}

fn stage_body(
    shader: &Shader,
    stage: Stage,
    abi: &PassAbi,
    epilogue: String,
    prologue: &str,
) -> Result<String, WgslError> {
    let (rows, row_base) = match stage {
        Stage::Vertex => (&abi.vertex_constants, 0),
        Stage::Pixel => (&abi.pixel_constants, abi.vertex_constants.len()),
    };
    let constants: BTreeMap<ConstantRow, usize> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| (*row, row_base + i))
        .collect();
    let mut lowering = Lowering {
        stage,
        out: String::new(),
        indent: 1,
        shader,
        constants,
        textures: &abi.textures,
        sample_adapters: &abi.sample_adapters,
        epilogue,
    };
    lowering.out.push_str(prologue);
    let mut declared = shader.constant_rows()?;
    declared.extend(
        abi.sample_adapters
            .iter()
            .flat_map(|adapter| [adapter.scale_row, adapter.alpha_row])
            .flatten()
            .filter(|row| lowering.constants.contains_key(row)),
    );
    for row in declared {
        let arena = *lowering
            .constants
            .get(&row)
            .ok_or(WgslError::UnassignedConstant {
                buffer: row.buffer,
                row: row.row,
            })?;
        lowering.line(&format!(
            "let cb{}_{} = bitcast<vec4<u32>>(sm3_constants.c[sm3_constant_base + {arena}u]);",
            row.buffer, row.row
        ));
    }
    let slot_base = abi.vertex_constants.len() + abi.pixel_constants.len();
    for slot in shader.texture_slots()? {
        let (index, _) = lowering.texture_slot(slot.texture, slot.sampler)?;
        lowering.line(&format!(
            "let dx_slot_{index} = bitcast<vec4<u32>>(sm3_constants.c[sm3_constant_base + {}u]).{};",
            slot_base + index / 4,
            XYZW[index % 4]
        ));
    }
    for t in 0..lowering.shader.temps {
        lowering.line(&format!("var r{t}: vec4<u32> = vec4<u32>(0u);"));
    }
    for o in output_registers(shader) {
        lowering.line(&format!("var o{o}: vec4<u32> = vec4<u32>(0u);"));
    }
    for instruction in &shader.instructions {
        lowering.instruction(instruction)?;
    }
    if lowering.indent != 1 {
        return Err(WgslError::UnbalancedControlFlow);
    }
    let epilogue = lowering.epilogue.clone();
    lowering.line(&epilogue);
    Ok(lowering.out)
}

fn varying_locations(vertex: &Shader) -> BTreeMap<u32, u32> {
    vertex
        .output
        .elements
        .iter()
        .filter(|e| e.system_value != 1)
        .map(|e| e.register)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .enumerate()
        .map(|(location, register)| (register, location as u32))
        .collect()
}

pub fn lower_pass(abi: &PassAbi, vertex: &Shader, pixel: &Shader) -> Result<String, WgslError> {
    if vertex.kind != ProgramKind::Vertex {
        return Err(WgslError::WrongStage {
            expected: ProgramKind::Vertex,
            found: vertex.kind,
        });
    }
    if pixel.kind != ProgramKind::Pixel {
        return Err(WgslError::WrongStage {
            expected: ProgramKind::Pixel,
            found: pixel.kind,
        });
    }
    let position = vertex
        .output
        .elements
        .iter()
        .find(|e| e.system_value == 1)
        .map(|e| e.register)
        .ok_or(WgslError::MissingPosition)?;
    let locations = varying_locations(vertex);
    let flat_location = locations.len();

    let mut out = String::new();
    out.push_str("diagnostic(off, derivative_uniformity);\n\n");
    out.push_str("struct Sm3ConstantArena { c: array<vec4<f32>> }\n");
    out.push_str("@group(0) @binding(0) var<storage, read> sm3_constants: Sm3ConstantArena;\n");
    texture_tables(&mut out, &abi.textures);
    if !abi.sample_adapters.is_empty() {
        out.push_str(
            "\nfn dx_adapt(s: vec4<f32>, alpha_weight: f32, square_rgb: bool, scale: f32) -> vec4<f32> {\n    \
             let rgb = select(s.xyz, s.xyz * s.xyz, square_rgb) * scale;\n    \
             return vec4<f32>(rgb, mix(1.0, s.w, alpha_weight));\n}\n",
        );
    }
    out.push_str("\nstruct DxVaryings {\n    @builtin(position) @invariant position: vec4<f32>,\n");
    for location in locations.values() {
        writeln!(
            out,
            "    @location({location}) varying_{location}: vec4<f32>,"
        )
        .unwrap();
    }
    writeln!(
        out,
        "    @interpolate(flat) @location({flat_location}) sm3_constant_base: u32,\n}}\n"
    )
    .unwrap();

    writeln!(out, "@vertex\nfn {VERTEX_ENTRY}(").unwrap();
    for input in &abi.vertex_inputs {
        writeln!(
            out,
            "    @location({}) attribute_{}: {},",
            input.location, input.location, input.attribute_type
        )
        .unwrap();
    }
    out.push_str("    @builtin(instance_index) sm3_constant_base: u32,\n) -> DxVaryings {\n");
    let mut prologue = String::new();
    for register in input_registers(vertex) {
        let expression = abi
            .vertex_inputs
            .iter()
            .find(|input| input.register == register)
            .map_or_else(
                || "vec4<f32>(0.0)".to_string(),
                |input| input.expression.clone(),
            );
        writeln!(
            prologue,
            "    let v{register} = bitcast<vec4<u32>>({expression});"
        )
        .unwrap();
    }
    let mut epilogue = format!("return DxVaryings(bitcast<vec4<f32>>(o{position})");
    for register in locations.keys() {
        write!(epilogue, ", bitcast<vec4<f32>>(o{register})").unwrap();
    }
    epilogue.push_str(", sm3_constant_base);");
    out.push_str(&stage_body(
        vertex,
        Stage::Vertex,
        abi,
        epilogue,
        &prologue,
    )?);
    out.push_str("}\n\n");

    let pixel_outputs = output_registers(pixel);
    if let Some(&other) = pixel_outputs.iter().find(|&&r| r != 0) {
        return Err(WgslError::UnsupportedPixelOutput(other));
    }
    let mut prologue = String::from("    let sm3_constant_base = varyings.sm3_constant_base;\n");
    for register in input_registers(pixel) {
        let element = pixel.input.elements.iter().find(|e| e.register == register);
        let expression = match element {
            Some(e) if e.system_value == 1 => "varyings.position".to_string(),
            Some(e) => vertex
                .output
                .elements
                .iter()
                .find(|o| {
                    o.semantic.eq_ignore_ascii_case(&e.semantic)
                        && o.semantic_index == e.semantic_index
                })
                .and_then(|o| locations.get(&o.register))
                .map_or_else(
                    || "vec4<f32>(0.0)".to_string(),
                    |location| format!("varyings.varying_{location}"),
                ),
            None => "vec4<f32>(0.0)".to_string(),
        };
        writeln!(
            prologue,
            "    let v{register} = bitcast<vec4<u32>>({expression});"
        )
        .unwrap();
    }
    let colour = if pixel_outputs.contains(&0) {
        "bitcast<vec4<f32>>(o0)"
    } else {
        "vec4<f32>(varyings.position.z)"
    };
    let entries = core::iter::once((FRAGMENT_ENTRY.to_string(), String::new()))
        .chain(abi.alpha_tests.iter().cloned());
    for (entry, test) in entries {
        writeln!(
            out,
            "\n@fragment\nfn {entry}(varyings: DxVaryings) -> @location(0) vec4<f32> {{"
        )
        .unwrap();
        let epilogue = format!("{{ let dx_colour = {colour}; {test} return dx_colour; }}");
        out.push_str(&stage_body(pixel, Stage::Pixel, abi, epilogue, &prologue)?);
        out.push_str("}\n");
    }
    Ok(out)
}
