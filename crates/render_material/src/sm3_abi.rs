use std::collections::{BTreeMap, BTreeSet};

use asset_iw4::vertex_decl as vd;

use crate::argument::RuntimeArgumentBinding;
use crate::sm3::{Sm3InstructionBody, Sm3ProgramIr, Sm3Register, Sm3RegisterFile};
use crate::stage::RuntimeShaderStage;
use crate::vertex_decl::RuntimeVertexDecl;

const DCL_USAGE_MASK: u32 = 0x0000_000f;

const DCL_USAGE_INDEX_SHIFT: u32 = 16;
const DCL_USAGE_INDEX_MASK: u32 = 0x0000_000f;

const DCL_TEXTURE_TYPE_SHIFT: u32 = 27;
const DCL_TEXTURE_TYPE_MASK: u32 = 0x0000_000f;
const DCL_TEXTURE_TYPE_2D: u32 = 2;
const DCL_TEXTURE_TYPE_CUBE: u32 = 3;
const DCL_TEXTURE_TYPE_VOLUME: u32 = 4;

pub use d3d9_sm3::{SamplerTextureDimension, Semantic};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexAttribute {
    pub source: u8,

    pub dest: u8,
    pub semantic: Semantic,
    pub layout: vd::StreamSourceLayout,

    pub location: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexInputBinding {
    pub register: Sm3Register,
    pub attribute: Option<VertexAttribute>,
    pub semantic: Semantic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VaryingBinding {
    pub semantic: Semantic,
    pub vertex_register: Sm3Register,
    pub pixel_register: Option<Sm3Register>,

    pub location: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstantSource {
    Literal { words: Option<[u32; 4]> },

    Material { name_hash: u32 },

    Code { index: u16, row: u8 },

    ProgramDefined,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstantBinding {
    pub register: u16,
    pub source: ConstantSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SamplerSource {
    MaterialTexture { name_hash: u32 },

    CodeTexture { index: u32 },

    SurfaceReflectionProbe,

    SurfacePrimaryLightmap,

    SurfaceSecondaryLightmap,
    SurfaceSecondaryBLightmap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SamplerBinding {
    pub register: u16,
    pub source: SamplerSource,
    pub dimension: SamplerTextureDimension,

    pub depth_compare: bool,
}

const CODE_TEXTURE_SHADOWMAP_SUN: u32 = 6;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassProgramAbi {
    pub vertex_family: crate::VertexLayoutFamily,
    pub vertex_type: u8,

    pub attributes: Vec<VertexAttribute>,

    pub vertex_inputs: Vec<VertexInputBinding>,

    pub position: VaryingBinding,
    pub varyings: Vec<VaryingBinding>,
    pub vertex_constants: Vec<ConstantBinding>,
    pub pixel_constants: Vec<ConstantBinding>,
    pub samplers: Vec<SamplerBinding>,
}

pub fn format_vertex_semantic(semantic: Semantic) -> String {
    let index = semantic.usage_index;
    match semantic.usage {
        vd::D3DDECLUSAGE_POSITION => format!("POSITION{index}"),
        vd::D3DDECLUSAGE_NORMAL => format!("NORMAL{index}"),
        vd::D3DDECLUSAGE_TEXCOORD => format!("TEXCOORD{index}"),
        vd::D3DDECLUSAGE_COLOR => format!("COLOR{index}"),
        other => format!("{other}:{index}"),
    }
}

impl PassProgramAbi {
    pub fn missing_element_semantics(&self) -> Vec<Semantic> {
        self.vertex_inputs
            .iter()
            .filter(|binding| binding.attribute.is_none())
            .map(|binding| binding.semantic)
            .collect()
    }

    pub fn missing_element_census(&self) -> String {
        self.missing_element_semantics()
            .into_iter()
            .map(format_vertex_semantic)
            .collect::<Vec<_>>()
            .join(",")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassAbiRefusal {
    StreamCountOutOfRange {
        stream_count: u8,
    },

    RoutingDestinationUnknown {
        dest: u8,
    },

    RoutingSourceUnknown {
        source: u8,
    },

    VertexTypeMissingSource {
        vertex_type: u8,
        source: u8,
    },

    UnknownDeclType {
        source: u8,
        raw: u8,
    },

    VertexInputUndeclared {
        register: Sm3Register,
    },

    VertexInputUnrouted {
        register: Sm3Register,
        semantic: Semantic,
    },

    DuplicateRoutedSemantic {
        semantic: Semantic,
    },

    DeclarationWithoutUsage {
        register: Sm3Register,
    },

    MissingPositionOutput,

    DuplicatePositionOutput,

    VaryingUnbound {
        semantic: Semantic,
    },

    ConstantRegisterUnbound {
        stage: RuntimeShaderStage,
        register: u16,
    },

    ConstantRegisterConflict {
        stage: RuntimeShaderStage,
        register: u16,
    },

    SamplerRegisterUnbound {
        register: u16,
    },

    SamplerRegisterConflict {
        register: u16,
    },

    UnsupportedSamplerTextureType {
        register: u16,
        texture_type: u32,
    },

    SamplerDeclarationMissing {
        register: u16,
    },

    VertexStageSampler {
        register: u16,
    },

    UnknownArgumentType {
        argument_type: u16,
    },

    ConstantBandOverflow {
        stage: RuntimeShaderStage,
        destination: u16,
        row_count: u8,
    },

    DxbcProgram,

    DxbcVertexInputUnrouted {
        register: u32,
    },
}

pub(crate) fn build_pass_abi(
    vertex: &Sm3ProgramIr,
    pixel: &Sm3ProgramIr,
    decl: &RuntimeVertexDecl,
    vertex_type: u8,
    arguments: &[RuntimeArgumentBinding],
    custom_sampler_flags: u8,
    t5_custom_sampler_flags: u8,
    hardware_shadow_compare: bool,
) -> Result<PassProgramAbi, PassAbiRefusal> {
    let attributes = routed_attributes(decl, vertex_type)?;
    let vertex_inputs = bind_vertex_inputs(vertex, &attributes)?;
    let vertex_declarations = declarations(vertex);
    let pixel_declarations = declarations(pixel);

    let (position, varyings) = link_varyings(&vertex_declarations, &pixel_declarations, pixel)?;

    let vertex_constants = constant_bindings(vertex, RuntimeShaderStage::Vertex, arguments)?;
    let pixel_constants = constant_bindings(pixel, RuntimeShaderStage::Pixel, arguments)?;
    let samplers = sampler_bindings(
        vertex,
        pixel,
        arguments,
        custom_sampler_flags,
        t5_custom_sampler_flags,
        hardware_shadow_compare,
    )?;

    Ok(PassProgramAbi {
        vertex_family: decl.family,
        vertex_type,
        attributes,
        vertex_inputs,
        position,
        varyings,
        vertex_constants,
        pixel_constants,
        samplers,
    })
}

pub(crate) fn routed_attributes(
    decl: &RuntimeVertexDecl,
    vertex_type: u8,
) -> Result<Vec<VertexAttribute>, PassAbiRefusal> {
    if usize::from(decl.stream_count) > vd::ROUTING_COUNT {
        return Err(PassAbiRefusal::StreamCountOutOfRange {
            stream_count: decl.stream_count,
        });
    }
    let mut attributes = Vec::new();
    let mut seen = BTreeSet::new();
    for (location, pair) in decl.routed().iter().enumerate() {
        let [source, dest] = *pair;
        if usize::from(source) >= decl.family.source_count() {
            return Err(PassAbiRefusal::RoutingSourceUnknown { source });
        }
        let (usage, usage_index) = decl
            .family
            .destination_usage(dest)
            .ok_or(PassAbiRefusal::RoutingDestinationUnknown { dest })?;
        let layout = decl.family.source_layout(vertex_type, source).ok_or(
            PassAbiRefusal::VertexTypeMissingSource {
                vertex_type,
                source,
            },
        )?;
        if let vd::D3dDeclType::Unknown(raw) = layout.decl_type {
            return Err(PassAbiRefusal::UnknownDeclType { source, raw });
        }
        let semantic = Semantic { usage, usage_index };
        if !seen.insert(semantic) {
            return Err(PassAbiRefusal::DuplicateRoutedSemantic { semantic });
        }
        attributes.push(VertexAttribute {
            source,
            dest,
            semantic,
            layout,
            location: location as u32,
        });
    }
    Ok(attributes)
}

fn bind_vertex_inputs(
    vertex: &Sm3ProgramIr,
    attributes: &[VertexAttribute],
) -> Result<Vec<VertexInputBinding>, PassAbiRefusal> {
    let mut vertex_inputs = Vec::new();
    for (register, semantic) in declarations(vertex)
        .iter()
        .filter(|(register, _)| register.file == Sm3RegisterFile::Input)
    {
        let attribute = attributes
            .iter()
            .copied()
            .find(|attribute| attribute.semantic == *semantic);
        if attribute.is_none()
            && semantic.usage == vd::D3DDECLUSAGE_POSITION
            && semantic.usage_index == 0
        {
            return Err(PassAbiRefusal::VertexInputUnrouted {
                register: *register,
                semantic: *semantic,
            });
        }
        vertex_inputs.push(VertexInputBinding {
            register: *register,
            attribute,
            semantic: *semantic,
        });
    }
    for register in read_registers(vertex, Sm3RegisterFile::Input) {
        if !vertex_inputs
            .iter()
            .any(|binding| binding.register == register)
        {
            return Err(PassAbiRefusal::VertexInputUndeclared { register });
        }
    }
    Ok(vertex_inputs)
}

fn declarations(program: &Sm3ProgramIr) -> Vec<(Sm3Register, Semantic)> {
    let mut declarations = Vec::new();
    for instruction in &program.instructions {
        let Sm3InstructionBody::Declaration {
            usage_token: Some(token),
            destination,
        } = &instruction.body
        else {
            continue;
        };
        if destination.register.file == Sm3RegisterFile::Sampler {
            continue;
        }
        declarations.push((
            destination.register,
            Semantic {
                usage: (token & DCL_USAGE_MASK) as u8,
                usage_index: ((token >> DCL_USAGE_INDEX_SHIFT) & DCL_USAGE_INDEX_MASK) as u8,
            },
        ));
    }
    declarations
}

fn link_varyings(
    vertex_declarations: &[(Sm3Register, Semantic)],
    pixel_declarations: &[(Sm3Register, Semantic)],
    pixel: &Sm3ProgramIr,
) -> Result<(VaryingBinding, Vec<VaryingBinding>), PassAbiRefusal> {
    let outputs = vertex_declarations
        .iter()
        .filter(|(register, _)| is_vertex_output(register.file))
        .collect::<Vec<_>>();

    let mut position = None;
    for (register, semantic) in &outputs {
        if semantic.usage != vd::D3DDECLUSAGE_POSITION {
            continue;
        }
        if position.is_some() {
            return Err(PassAbiRefusal::DuplicatePositionOutput);
        }
        position = Some(VaryingBinding {
            semantic: *semantic,
            vertex_register: *register,
            pixel_register: None,
            location: 0,
        });
    }
    let position = position.ok_or(PassAbiRefusal::MissingPositionOutput)?;

    let pixel_inputs = pixel_declarations
        .iter()
        .filter(|(register, _)| register.file == Sm3RegisterFile::Input)
        .collect::<Vec<_>>();
    for register in read_registers(pixel, Sm3RegisterFile::Input) {
        if !pixel_inputs
            .iter()
            .any(|(declared, _)| *declared == register)
        {
            return Err(PassAbiRefusal::VertexInputUndeclared { register });
        }
    }

    let mut varyings = Vec::new();
    for (register, semantic) in &outputs {
        if semantic.usage == vd::D3DDECLUSAGE_POSITION {
            continue;
        }
        let pixel_register = pixel_inputs
            .iter()
            .find(|(_, pixel_semantic)| pixel_semantic == semantic)
            .map(|(register, _)| *register);
        varyings.push(VaryingBinding {
            semantic: *semantic,
            vertex_register: *register,
            pixel_register,
            location: 0,
        });
    }
    varyings.sort_by_key(|varying| (varying.semantic, varying.vertex_register));
    for (location, varying) in varyings.iter_mut().enumerate() {
        varying.location = location as u32;
    }

    for (_, semantic) in &pixel_inputs {
        if !varyings.iter().any(|varying| varying.semantic == *semantic) {
            return Err(PassAbiRefusal::VaryingUnbound {
                semantic: *semantic,
            });
        }
    }

    Ok((position, varyings))
}

fn is_vertex_output(file: Sm3RegisterFile) -> bool {
    matches!(
        file,
        Sm3RegisterFile::Output | Sm3RegisterFile::RasterOutput | Sm3RegisterFile::AttributeOutput
    )
}

fn read_registers(program: &Sm3ProgramIr, file: Sm3RegisterFile) -> BTreeSet<Sm3Register> {
    let mut registers = BTreeSet::new();
    for instruction in &program.instructions {
        let Sm3InstructionBody::Operation { sources, .. } = &instruction.body else {
            continue;
        };
        for source in sources {
            if source.register.file == file {
                registers.insert(source.register);
            }
        }
    }
    registers
}

fn program_defined_constants(program: &Sm3ProgramIr) -> BTreeSet<u16> {
    let mut defined = BTreeSet::new();
    for instruction in &program.instructions {
        if let Sm3InstructionBody::FloatDefinition { destination, .. } = &instruction.body {
            if destination.register.file == Sm3RegisterFile::FloatConstant {
                defined.insert(destination.register.index);
            }
        }
    }
    defined
}

fn constant_bindings(
    program: &Sm3ProgramIr,
    stage: RuntimeShaderStage,
    arguments: &[RuntimeArgumentBinding],
) -> Result<Vec<ConstantBinding>, PassAbiRefusal> {
    let mut bound = BTreeMap::<u16, ConstantSource>::new();
    let mut insert = |register: u16, source: ConstantSource| -> Result<(), PassAbiRefusal> {
        if bound.insert(register, source).is_some() {
            return Err(PassAbiRefusal::ConstantRegisterConflict { stage, register });
        }
        Ok(())
    };
    for argument in arguments {
        match argument {
            RuntimeArgumentBinding::LiteralConstant {
                stage: argument_stage,
                destination,
                words,
            } if *argument_stage == stage => {
                insert(*destination, ConstantSource::Literal { words: *words })?;
            }
            RuntimeArgumentBinding::MaterialConstant {
                stage: argument_stage,
                destination,
                name_hash,
            } if *argument_stage == stage => {
                insert(
                    *destination,
                    ConstantSource::Material {
                        name_hash: *name_hash,
                    },
                )?;
            }
            RuntimeArgumentBinding::CodeConstant {
                stage: argument_stage,
                destination,
                index,
                row_count,
                ..
            } if *argument_stage == stage => {
                for row in 0..*row_count {
                    let register = destination.checked_add(u16::from(row)).ok_or(
                        PassAbiRefusal::ConstantBandOverflow {
                            stage,
                            destination: *destination,
                            row_count: *row_count,
                        },
                    )?;
                    insert(register, ConstantSource::Code { index: *index, row })?;
                }
            }
            RuntimeArgumentBinding::Unknown { argument_type, .. } => {
                return Err(PassAbiRefusal::UnknownArgumentType {
                    argument_type: *argument_type,
                });
            }
            _ => {}
        }
    }
    for register in program_defined_constants(program) {
        bound
            .entry(register)
            .or_insert(ConstantSource::ProgramDefined);
    }

    let mut static_reads = BTreeSet::new();
    let mut uses_relative_float = false;
    for instruction in &program.instructions {
        let Sm3InstructionBody::Operation { sources, .. } = &instruction.body else {
            continue;
        };
        for source in sources {
            if source.register.file != Sm3RegisterFile::FloatConstant {
                continue;
            }
            if source.relative.is_some() {
                uses_relative_float = true;
            } else {
                static_reads.insert(source.register.index);
            }
        }
    }
    for register in &static_reads {
        bound
            .get(register)
            .ok_or(PassAbiRefusal::ConstantRegisterUnbound {
                stage,
                register: *register,
            })?;
    }
    let registers: BTreeSet<u16> = if uses_relative_float {
        bound.keys().copied().collect()
    } else {
        static_reads
    };
    let mut bindings = Vec::new();
    for register in registers {
        let source = bound[&register];
        bindings.push(ConstantBinding { register, source });
    }
    Ok(bindings)
}

fn sampler_bindings(
    vertex: &Sm3ProgramIr,
    pixel: &Sm3ProgramIr,
    arguments: &[RuntimeArgumentBinding],
    custom_sampler_flags: u8,
    t5_custom_sampler_flags: u8,
    hardware_shadow_compare: bool,
) -> Result<Vec<SamplerBinding>, PassAbiRefusal> {
    if let Some(register) = used_samplers(vertex).into_iter().next() {
        return Err(PassAbiRefusal::VertexStageSampler { register });
    }
    let mut dimensions = BTreeMap::<u16, SamplerTextureDimension>::new();
    for instruction in &pixel.instructions {
        let Sm3InstructionBody::Declaration {
            usage_token: Some(token),
            destination,
        } = &instruction.body
        else {
            continue;
        };
        if destination.register.file != Sm3RegisterFile::Sampler {
            continue;
        }
        let texture_type = (token >> DCL_TEXTURE_TYPE_SHIFT) & DCL_TEXTURE_TYPE_MASK;
        let dimension = match texture_type {
            DCL_TEXTURE_TYPE_2D => SamplerTextureDimension::D2,
            DCL_TEXTURE_TYPE_CUBE => SamplerTextureDimension::Cube,
            DCL_TEXTURE_TYPE_VOLUME => SamplerTextureDimension::D3,
            _ => {
                return Err(PassAbiRefusal::UnsupportedSamplerTextureType {
                    register: destination.register.index,
                    texture_type,
                });
            }
        };
        dimensions.insert(destination.register.index, dimension);
    }

    let mut bound = BTreeMap::<u16, SamplerSource>::new();
    for argument in arguments {
        let (destination, source) = match argument {
            RuntimeArgumentBinding::MaterialTexture {
                destination,
                name_hash,
            } => (
                *destination,
                SamplerSource::MaterialTexture {
                    name_hash: *name_hash,
                },
            ),
            RuntimeArgumentBinding::CodeTexture { destination, index } => {
                (*destination, SamplerSource::CodeTexture { index: *index })
            }
            _ => continue,
        };
        if bound.insert(destination, source).is_some() {
            return Err(PassAbiRefusal::SamplerRegisterConflict {
                register: destination,
            });
        }
    }

    let (flag_bits, stages) =
        custom_sampler_flag_stages(custom_sampler_flags, t5_custom_sampler_flags);
    for (bit, register, source) in stages {
        if flag_bits & bit != 0 && bound.insert(register, source).is_some() {
            return Err(PassAbiRefusal::SamplerRegisterConflict { register });
        }
    }

    let mut bindings = Vec::new();
    for register in used_samplers(pixel) {
        let source = bound
            .get(&register)
            .copied()
            .ok_or(PassAbiRefusal::SamplerRegisterUnbound { register })?;
        let dimension = dimensions
            .get(&register)
            .copied()
            .ok_or(PassAbiRefusal::SamplerDeclarationMissing { register })?;
        bindings.push(SamplerBinding {
            register,
            source,
            dimension,
            depth_compare: hardware_shadow_compare
                && dimension == SamplerTextureDimension::D2
                && source
                    == (SamplerSource::CodeTexture {
                        index: CODE_TEXTURE_SHADOWMAP_SUN,
                    }),
        });
    }
    Ok(bindings)
}

fn custom_sampler_flag_stages(
    custom_sampler_flags: u8,
    t5_custom_sampler_flags: u8,
) -> (u8, Vec<(u8, u16, SamplerSource)>) {
    if t5_custom_sampler_flags != 0 {
        (
            t5_custom_sampler_flags,
            vec![
                (0x01, 15, SamplerSource::SurfaceReflectionProbe),
                (0x02, 12, SamplerSource::SurfacePrimaryLightmap),
                (0x04, 13, SamplerSource::SurfaceSecondaryLightmap),
                (0x08, 14, SamplerSource::SurfaceSecondaryBLightmap),
            ],
        )
    } else {
        (
            custom_sampler_flags,
            vec![
                (0x01, 1, SamplerSource::SurfaceReflectionProbe),
                (0x02, 2, SamplerSource::SurfacePrimaryLightmap),
                (0x04, 3, SamplerSource::SurfaceSecondaryLightmap),
            ],
        )
    }
}

fn used_samplers(program: &Sm3ProgramIr) -> BTreeSet<u16> {
    let mut samplers = BTreeSet::new();
    for instruction in &program.instructions {
        match &instruction.body {
            Sm3InstructionBody::Operation { sources, .. } => {
                for source in sources {
                    if source.register.file == Sm3RegisterFile::Sampler {
                        samplers.insert(source.register.index);
                    }
                }
            }
            Sm3InstructionBody::Declaration { destination, .. } => {
                if destination.register.file == Sm3RegisterFile::Sampler {
                    samplers.insert(destination.register.index);
                }
            }
            _ => {}
        }
    }
    samplers
}
