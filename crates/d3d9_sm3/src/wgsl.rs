use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::abi::{
    ALPHA_REF_SCALE, AlphaTest, CompareFunc, DeclType, PassLoweringAbi, SamplerTextureDimension,
    missing_vertex_element,
};
use crate::bytecode::ShaderStage;
use crate::ir::{
    Sm3Instruction, Sm3InstructionBody, Sm3LoweringRefusal, Sm3Opcode, Sm3ProgramIr, Sm3Register,
    Sm3RegisterFile, Sm3Source, Sm3SourceModifier, TEXLD_PROJECT,
    validate_supported_opcode_surface,
};

pub const PASS_VERTEX_ENTRY: &str = "sm3_vertex_main";
pub const PASS_FRAGMENT_ENTRY: &str = "sm3_fragment_main";

pub const TEXTURE_TABLE_GROUP: u32 = 1;
pub const TEXTURE_TABLE_BINDING_2D: u32 = 0;
pub const TEXTURE_TABLE_BINDING_CUBE: u32 = 1;
pub const TEXTURE_TABLE_BINDING_3D: u32 = 2;
pub const TEXTURE_TABLE_BINDING_SAMPLERS: u32 = 3;

pub const fn texture_slot_word(texture: u16, sampler: u16) -> u32 {
    (texture as u32) | ((sampler as u32) << 16)
}

pub const fn texture_slot_rows(sampler_count: usize) -> usize {
    sampler_count.div_ceil(4)
}

fn texture_array_name(dimension: SamplerTextureDimension) -> &'static str {
    match dimension {
        SamplerTextureDimension::D2 => "sm3_textures_2d",
        SamplerTextureDimension::Cube => "sm3_textures_cube",
        SamplerTextureDimension::D3 => "sm3_textures_3d",
    }
}

pub fn pass_fragment_alpha_test_entry(index: usize) -> String {
    format!("{PASS_FRAGMENT_ENTRY}_atest{index}")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sm3Wgsl {
    pub source: String,

    pub output_masks: BTreeMap<Sm3Register, u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassWgsl {
    pub source: String,

    pub attribute_count: usize,

    pub varying_count: usize,

    pub vertex_constant_len: usize,
    pub pixel_constant_len: usize,
    pub sampler_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sm3WgslError {
    LoweringSurface(Sm3LoweringRefusal),

    RelativeConstantsNeedPassAbi,

    SamplersNeedPassAbi,

    UnboundSamplerRegister {
        register: u16,
    },
    UnsupportedInstructionControls {
        at_word: usize,
        opcode: Sm3Opcode,
        controls: u8,
    },
    UnsupportedDestinationFile {
        at_word: usize,
        file: Sm3RegisterFile,
    },
    UnsupportedSourceFile {
        at_word: usize,
        file: Sm3RegisterFile,
    },
    InvalidSamplerOperand {
        at_word: usize,
    },
    InvalidNormaliseDestination {
        at_word: usize,
        component: u8,
    },

    InvalidSincosDestination {
        at_word: usize,
        component: u8,
    },
    RegisterReadBeforeWrite {
        at_word: usize,
        register: Sm3Register,
        component: u8,
    },

    PartialConstantDefinition {
        at_word: usize,
        register: Sm3Register,
        write_mask: u8,
    },
    NoProgramOutput,

    DeclaredOutputNeverWritten {
        register: Sm3Register,
    },

    MissingColourOutput,

    UnsupportedAlphaTestFunc {
        raw_func: Option<u32>,
    },

    UnsupportedPixelOutput {
        register: Sm3Register,
    },

    UnlinkedVertexOutput {
        register: Sm3Register,
    },

    UnboundConstantRegister {
        register: Sm3Register,
    },
    SamplerDeclarationMissing {
        register: u16,
    },
    UnsupportedSamplerTextureType {
        register: u16,
        texture_type: u32,
    },

    PixelOnlyOpcode {
        at_word: usize,
        opcode: Sm3Opcode,
    },

    VertexOnlyOpcode {
        at_word: usize,
        opcode: Sm3Opcode,
    },

    SaturateOnAddress {
        at_word: usize,
    },

    UnbalancedControlFlow {
        at_word: usize,
        kind: &'static str,
    },
}

#[derive(Clone, Debug, Default)]
struct LoweringPlan {
    external: BTreeSet<Sm3Register>,

    external_read_masks: BTreeMap<Sm3Register, u8>,

    writable: BTreeSet<Sm3Register>,

    float_definitions: BTreeMap<Sm3Register, [u32; 4]>,
    integer_definitions: BTreeMap<Sm3Register, [u32; 4]>,
    samplers: BTreeMap<u16, SamplerTextureDimension>,
    output_masks: BTreeMap<Sm3Register, u8>,

    uses_relative_float: bool,
}

fn instruction_controls_supported(opcode: Sm3Opcode, controls: u8) -> bool {
    match opcode {
        Sm3Opcode::TexLd if controls == TEXLD_PROJECT => true,

        Sm3Opcode::Ifc if (1..=6).contains(&controls) => true,
        _ => controls == 0,
    }
}

fn plan_program(program: &Sm3ProgramIr) -> Result<LoweringPlan, Sm3WgslError> {
    validate_supported_opcode_surface(program).map_err(Sm3WgslError::LoweringSurface)?;
    let mut plan = LoweringPlan::default();
    for instruction in &program.instructions {
        if !instruction_controls_supported(instruction.opcode, instruction.controls) {
            return Err(Sm3WgslError::UnsupportedInstructionControls {
                at_word: instruction.at_word,
                opcode: instruction.opcode,
                controls: instruction.controls,
            });
        }
        if matches!(
            instruction.opcode,
            Sm3Opcode::TexKill | Sm3Opcode::Dsx | Sm3Opcode::Dsy | Sm3Opcode::TexLdD
        ) && program.stage != ShaderStage::Pixel
        {
            return Err(Sm3WgslError::PixelOnlyOpcode {
                at_word: instruction.at_word,
                opcode: instruction.opcode,
            });
        }
        if instruction.opcode == Sm3Opcode::Mova && program.stage != ShaderStage::Vertex {
            return Err(Sm3WgslError::VertexOnlyOpcode {
                at_word: instruction.at_word,
                opcode: instruction.opcode,
            });
        }
        match &instruction.body {
            Sm3InstructionBody::Declaration {
                usage_token,
                destination,
            } => {
                if destination.register.file == Sm3RegisterFile::Sampler {
                    let texture_type = usage_token.map(|token| (token >> 27) & 0x0f).ok_or(
                        Sm3WgslError::SamplerDeclarationMissing {
                            register: destination.register.index,
                        },
                    )?;
                    let dimension = match texture_type {
                        2 => SamplerTextureDimension::D2,
                        3 => SamplerTextureDimension::Cube,
                        4 => SamplerTextureDimension::D3,
                        _ => {
                            return Err(Sm3WgslError::UnsupportedSamplerTextureType {
                                register: destination.register.index,
                                texture_type,
                            });
                        }
                    };
                    plan.samplers.insert(destination.register.index, dimension);
                } else if is_external(destination.register.file) {
                    plan.external.insert(destination.register);
                }
            }
            Sm3InstructionBody::FloatDefinition {
                destination,
                values,
            } => {
                if destination.register.file != Sm3RegisterFile::FloatConstant {
                    return Err(Sm3WgslError::UnsupportedDestinationFile {
                        at_word: instruction.at_word,
                        file: destination.register.file,
                    });
                }
                if destination.write_mask != 0x0f {
                    return Err(Sm3WgslError::PartialConstantDefinition {
                        at_word: instruction.at_word,
                        register: destination.register,
                        write_mask: destination.write_mask,
                    });
                }
                plan.float_definitions.insert(destination.register, *values);
            }
            Sm3InstructionBody::IntegerDefinition {
                destination,
                values,
            } => {
                if destination.register.file != Sm3RegisterFile::IntegerConstant {
                    return Err(Sm3WgslError::UnsupportedDestinationFile {
                        at_word: instruction.at_word,
                        file: destination.register.file,
                    });
                }
                if destination.write_mask != 0x0f {
                    return Err(Sm3WgslError::PartialConstantDefinition {
                        at_word: instruction.at_word,
                        register: destination.register,
                        write_mask: destination.write_mask,
                    });
                }
                plan.integer_definitions
                    .insert(destination.register, *values);
            }
            Sm3InstructionBody::Operation {
                destination,
                sources,
            } => {
                for (source_index, source) in sources.iter().enumerate() {
                    if instruction.opcode == Sm3Opcode::Rep {
                        if source.register.file != Sm3RegisterFile::IntegerConstant
                            || source.modifier != Sm3SourceModifier::None
                            || source.relative.is_some()
                        {
                            return Err(Sm3WgslError::UnsupportedSourceFile {
                                at_word: instruction.at_word,
                                file: source.register.file,
                            });
                        }
                    } else if source.register.file == Sm3RegisterFile::IntegerConstant {
                        return Err(Sm3WgslError::UnsupportedSourceFile {
                            at_word: instruction.at_word,
                            file: source.register.file,
                        });
                    }
                    if source.register.file == Sm3RegisterFile::Sampler {
                        if !plan.samplers.contains_key(&source.register.index) {
                            return Err(Sm3WgslError::SamplerDeclarationMissing {
                                register: source.register.index,
                            });
                        }
                    } else if is_external(source.register.file) {
                        if source.register.file == Sm3RegisterFile::Misc
                            && (program.stage != ShaderStage::Pixel || source.register.index > 1)
                        {
                            return Err(Sm3WgslError::UnsupportedSourceFile {
                                at_word: instruction.at_word,
                                file: source.register.file,
                            });
                        }

                        if source.relative.is_some()
                            && source.register.file == Sm3RegisterFile::FloatConstant
                        {
                            continue;
                        }
                        plan.external.insert(source.register);
                        let destination_mask = destination.map_or(0x0f, |value| value.write_mask);
                        let texture_dimension = sources.get(1).and_then(|sampler| {
                            (sampler.register.file == Sm3RegisterFile::Sampler)
                                .then(|| plan.samplers.get(&sampler.register.index).copied())
                                .flatten()
                        });
                        let operand_mask = operation_source_mask(
                            instruction.opcode,
                            instruction.controls,
                            source_index,
                            destination_mask,
                            source,
                            texture_dimension,
                        );
                        *plan.external_read_masks.entry(source.register).or_insert(0) |=
                            operand_mask;
                    }
                }
                if instruction.opcode == Sm3Opcode::TexKill {
                    let Some(destination) = *destination else {
                        continue;
                    };
                    if is_external(destination.register.file) {
                        plan.external.insert(destination.register);
                        *plan
                            .external_read_masks
                            .entry(destination.register)
                            .or_insert(0) |= 0x0f;
                    } else if !matches!(
                        destination.register.file,
                        Sm3RegisterFile::Temporary | Sm3RegisterFile::Float16Temporary
                    ) {
                        return Err(Sm3WgslError::UnsupportedDestinationFile {
                            at_word: instruction.at_word,
                            file: destination.register.file,
                        });
                    }
                    continue;
                }
                if let Some(destination) = destination {
                    if instruction.opcode == Sm3Opcode::Mova
                        && destination.register.file != Sm3RegisterFile::Address
                    {
                        return Err(Sm3WgslError::UnsupportedDestinationFile {
                            at_word: instruction.at_word,
                            file: destination.register.file,
                        });
                    }
                    require_writable(instruction.at_word, destination.register)?;
                    plan.writable.insert(destination.register);
                    if is_output(destination.register.file) {
                        *plan.output_masks.entry(destination.register).or_insert(0) |=
                            destination.write_mask;
                    }
                }
            }
            Sm3InstructionBody::Unmodeled { .. } => {
                unreachable!("lowering surface rejects unknown opcodes")
            }
        }
    }

    for register in plan
        .float_definitions
        .keys()
        .chain(plan.integer_definitions.keys())
    {
        plan.external.remove(register);
        plan.writable.remove(register);
    }
    plan.uses_relative_float = program.instructions.iter().any(|instruction| {
        let Sm3InstructionBody::Operation { sources, .. } = &instruction.body else {
            return false;
        };
        sources.iter().any(|source| {
            source.relative.is_some() && source.register.file == Sm3RegisterFile::FloatConstant
        })
    });
    Ok(plan)
}

fn operation_source_mask(
    opcode: Sm3Opcode,
    controls: u8,
    source_index: usize,
    destination_mask: u8,
    source: &Sm3Source,
    texture_dimension: Option<SamplerTextureDimension>,
) -> u8 {
    let logical_components = match opcode {
        Sm3Opcode::Rcp
        | Sm3Opcode::Rsq
        | Sm3Opcode::Exp
        | Sm3Opcode::Log
        | Sm3Opcode::Pow
        | Sm3Opcode::Sincos => 0x01,
        Sm3Opcode::Dp3 => 0x07,
        Sm3Opcode::Dp4 => 0x0f,
        Sm3Opcode::Nrm => 0x07,
        Sm3Opcode::Dp2Add if source_index < 2 => 0x03,
        Sm3Opcode::Dp2Add => 0x01,
        Sm3Opcode::TexLd => match texture_dimension {
            Some(SamplerTextureDimension::D2) if controls == TEXLD_PROJECT => 0x0b,
            Some(SamplerTextureDimension::D2) => 0x03,
            Some(SamplerTextureDimension::D3) if controls == TEXLD_PROJECT => 0x0f,
            Some(SamplerTextureDimension::Cube | SamplerTextureDimension::D3) => 0x07,
            None => 0x0f,
        },
        Sm3Opcode::TexLdL => match texture_dimension {
            Some(SamplerTextureDimension::D2) => 0x0b,
            Some(SamplerTextureDimension::Cube | SamplerTextureDimension::D3) | None => 0x0f,
        },

        Sm3Opcode::TexLdD if source_index == 1 => 0,
        Sm3Opcode::TexLdD => match texture_dimension {
            Some(SamplerTextureDimension::D2) => 0x03,
            Some(SamplerTextureDimension::Cube | SamplerTextureDimension::D3) => 0x07,
            None => 0x0f,
        },

        Sm3Opcode::Ifc | Sm3Opcode::Rep => 0x01,
        _ => destination_mask,
    };
    let mut register_mask = 0u8;
    for component in 0..4 {
        if logical_components & (1 << component) != 0 {
            register_mask |= 1 << source.swizzle[component];
        }
    }
    register_mask
}

pub fn lower_sm3_to_wgsl(program: &Sm3ProgramIr) -> Result<Sm3Wgsl, Sm3WgslError> {
    let plan = plan_program(program)?;
    if plan.uses_relative_float {
        return Err(Sm3WgslError::RelativeConstantsNeedPassAbi);
    }
    if plan.output_masks.is_empty() {
        return Err(Sm3WgslError::NoProgramOutput);
    }
    if !plan.samplers.is_empty() {
        return Err(Sm3WgslError::SamplersNeedPassAbi);
    }

    let mut source = String::new();
    source.push_str("struct Sm3RegisterOutputs {\n");
    for register in plan.output_masks.keys() {
        writeln!(source, "    {}: vec4<f32>,", register_name(*register)).unwrap();
    }
    source.push_str("}\n\nfn sm3_main(\n");
    for register in &plan.external {
        writeln!(source, "    {}: vec4<f32>,", register_name(*register)).unwrap();
    }
    source.push_str(") -> Sm3RegisterOutputs {\n");

    let mut defined = BTreeMap::new();
    emit_prologue(&mut source, &plan, &mut defined);
    emit_body(
        &mut source,
        &program.instructions,
        &plan.external,
        &plan.samplers,
        &BTreeSet::new(),
        &mut defined,
        None,
    )?;
    source.push_str("    return Sm3RegisterOutputs(\n");
    for register in plan.output_masks.keys() {
        writeln!(source, "        {},", register_name(*register)).unwrap();
    }
    source.push_str("    );\n}\n");

    Ok(Sm3Wgsl {
        source,
        output_masks: plan.output_masks,
    })
}

pub fn lower_pass_to_wgsl(
    abi: &PassLoweringAbi,
    vertex: &Sm3ProgramIr,
    pixel: &Sm3ProgramIr,
) -> Result<PassWgsl, Sm3WgslError> {
    let vertex_plan = plan_program(vertex)?;
    let pixel_plan = plan_program(pixel)?;
    let compare: BTreeSet<u16> = abi
        .samplers
        .iter()
        .filter(|slot| slot.depth_compare)
        .map(|slot| slot.register)
        .collect();

    let vertex_constant_len = constant_block_len(abi, &vertex_plan, Stage::Vertex)?;
    let pixel_constant_len = constant_block_len(abi, &pixel_plan, Stage::Pixel)?;

    let mut linked_outputs = vec![abi.position];
    linked_outputs.extend(abi.varyings.iter().copied());
    for linked in &linked_outputs {
        if !vertex_plan
            .output_masks
            .contains_key(&linked.vertex_register)
        {
            return Err(Sm3WgslError::DeclaredOutputNeverWritten {
                register: linked.vertex_register,
            });
        }
    }
    for register in vertex_plan.output_masks.keys() {
        if !linked_outputs
            .iter()
            .any(|linked| linked.vertex_register == *register)
        {
            return Err(Sm3WgslError::UnlinkedVertexOutput {
                register: *register,
            });
        }
    }

    let colour_output = Sm3Register {
        file: Sm3RegisterFile::ColourOutput,
        index: 0,
    };
    for register in pixel_plan.output_masks.keys() {
        if *register != colour_output {
            return Err(Sm3WgslError::UnsupportedPixelOutput {
                register: *register,
            });
        }
    }
    if !pixel_plan.output_masks.contains_key(&colour_output) {
        return Err(Sm3WgslError::MissingColourOutput);
    }

    let mut source = String::new();
    source.push_str("struct Sm3ConstantArena { c: array<vec4<f32>> }\n");
    source.push_str("@group(0) @binding(0) var<storage, read> sm3_constants: Sm3ConstantArena;\n");
    if vertex_plan.uses_relative_float {
        emit_vs_relative_const_loader(&mut source, vertex_constant_len);
    }
    write_texture_table_bindings(&mut source, abi);
    let slot_row_base = vertex_constant_len + pixel_constant_len;

    // Depth-equal passes need identical position evaluation across pipelines,
    // including the passes that originally write the depth buffer.
    source.push_str(
        "\nstruct Sm3Varyings {\n    @builtin(position) @invariant position: vec4<f32>,\n",
    );
    for varying in &abi.varyings {
        writeln!(
            source,
            "    @location({}) varying_{}: vec4<f32>,",
            varying.location, varying.location
        )
        .unwrap();
    }
    writeln!(
        source,
        "    @interpolate(flat) @location({}) sm3_constant_base: u32,",
        abi.varyings.len()
    )
    .unwrap();
    source.push_str("}\n\n@vertex\nfn ");
    source.push_str(PASS_VERTEX_ENTRY);
    source.push_str("(\n");
    for input in &abi.vertex_inputs {
        let (Some(location), Some(decl_type)) = (input.location, input.decl_type) else {
            continue;
        };
        writeln!(
            source,
            "    @location({}) attribute_{}: {},",
            location,
            location,
            attribute_wgsl_type(decl_type)
        )
        .unwrap();
    }
    source.push_str("    @builtin(instance_index) sm3_constant_base: u32,\n");
    source.push_str(") -> Sm3Varyings {\n");
    for input in &abi.vertex_inputs {
        let expansion = match (input.location, input.decl_type) {
            (Some(location), Some(decl_type)) => attribute_expansion(decl_type, location),
            _ => {
                let [x, y, z, w] = missing_vertex_element(input.semantic);
                format!("vec4<f32>({x:?}, {y:?}, {z:?}, {w:?})")
            }
        };
        writeln!(
            source,
            "    let {} = {};",
            register_name(input.register),
            expansion
        )
        .unwrap();
    }
    emit_constant_prologue(
        &mut source,
        abi,
        &vertex_plan,
        Stage::Vertex,
        "sm3_constant_base",
    )?;
    emit_texture_slot_prologue(
        &mut source,
        abi,
        &vertex_plan,
        "sm3_constant_base",
        slot_row_base,
    )?;
    let mut defined = BTreeMap::new();
    emit_prologue(&mut source, &vertex_plan, &mut defined);
    let vertex_relative = vertex_plan.uses_relative_float.then_some("sm3_vs_c");
    emit_body(
        &mut source,
        &vertex.instructions,
        &vertex_plan.external,
        &vertex_plan.samplers,
        &BTreeSet::new(),
        &mut defined,
        vertex_relative,
    )?;
    source.push_str("    return Sm3Varyings(\n");
    writeln!(
        source,
        "        {},",
        register_name(abi.position.vertex_register)
    )
    .unwrap();
    for varying in &abi.varyings {
        writeln!(
            source,
            "        {},",
            register_name(varying.vertex_register)
        )
        .unwrap();
    }
    source.push_str("        sm3_constant_base,\n");
    source.push_str("    );\n}\n");

    let front_facing = pixel_plan
        .external
        .iter()
        .any(|r| r.file == Sm3RegisterFile::Misc && r.index == 1);
    source.push_str("\nfn sm3_pixel_main");
    source.push_str("(varyings: Sm3Varyings");
    if front_facing {
        source.push_str(", sm3_front_facing: bool");
    }
    source.push_str(") -> vec4<f32> {\n");
    for register in &pixel_plan.external {
        if register.file == Sm3RegisterFile::Misc {
            match register.index {
                0 => {
                    if pixel_plan
                        .external_read_masks
                        .get(register)
                        .copied()
                        .unwrap_or(0)
                        & !3
                        != 0
                    {
                        return Err(Sm3WgslError::UnsupportedSourceFile {
                            at_word: 0,
                            file: register.file,
                        });
                    }
                    source.push_str("    let misc0 = varyings.position.xy - vec2<f32>(0.5);\n");
                }
                1 => {
                    source.push_str(
                        "    let misc1 = vec4<f32>(select(-1.0, 1.0, sm3_front_facing));\n",
                    );
                }
                _ => {
                    return Err(Sm3WgslError::UnsupportedSourceFile {
                        at_word: 0,
                        file: register.file,
                    });
                }
            }
        }
    }
    for varying in &abi.varyings {
        let Some(register) = varying.pixel_register else {
            continue;
        };
        writeln!(
            source,
            "    let {} = varyings.varying_{};",
            register_name(register),
            varying.location
        )
        .unwrap();
    }
    emit_constant_prologue(
        &mut source,
        abi,
        &pixel_plan,
        Stage::Pixel,
        &format!("varyings.sm3_constant_base + {}u", vertex_constant_len),
    )?;
    emit_texture_slot_prologue(
        &mut source,
        abi,
        &pixel_plan,
        "varyings.sm3_constant_base",
        slot_row_base,
    )?;
    let mut defined = BTreeMap::new();
    emit_prologue(&mut source, &pixel_plan, &mut defined);
    emit_body(
        &mut source,
        &pixel.instructions,
        &pixel_plan.external,
        &pixel_plan.samplers,
        &compare,
        &mut defined,
        None,
    )?;
    writeln!(source, "    return {};", register_name(colour_output)).unwrap();
    source.push_str("}\n");

    for index in 0..=abi.alpha_tests.len() {
        let alpha_test = index.checked_sub(1).map(|slot| abi.alpha_tests[slot]);
        source.push_str("\n@fragment\nfn ");
        match index.checked_sub(1) {
            None => source.push_str(PASS_FRAGMENT_ENTRY),
            Some(slot) => source.push_str(&pass_fragment_alpha_test_entry(slot)),
        }
        source.push_str("(varyings: Sm3Varyings");
        if front_facing {
            source.push_str(", @builtin(front_facing) sm3_front_facing: bool");
        }
        source.push_str(") -> @location(0) vec4<f32> {\n");
        writeln!(
            source,
            "    let {} = sm3_pixel_main(varyings{});",
            register_name(colour_output),
            if front_facing {
                ", sm3_front_facing"
            } else {
                ""
            }
        )
        .unwrap();
        if let Some(alpha_test) = alpha_test {
            emit_alpha_test(&mut source, alpha_test, colour_output)?;
        }
        writeln!(source, "    return {};", register_name(colour_output)).unwrap();
        source.push_str("}\n");
    }

    Ok(PassWgsl {
        source,
        attribute_count: abi
            .vertex_inputs
            .iter()
            .filter(|input| input.location.is_some())
            .count(),
        varying_count: abi.varyings.len(),
        vertex_constant_len,
        pixel_constant_len,
        sampler_count: abi.samplers.len(),
    })
}

fn emit_alpha_test(
    source: &mut String,
    alpha_test: AlphaTest,
    colour_output: Sm3Register,
) -> Result<(), Sm3WgslError> {
    let operator = match alpha_test.func {
        CompareFunc::Never => {
            source.push_str("    discard;\n");
            return Ok(());
        }
        CompareFunc::Always => return Ok(()),
        CompareFunc::Less => "<",
        CompareFunc::Equal => "==",
        CompareFunc::LessEqual => "<=",
        CompareFunc::Greater => ">",
        CompareFunc::NotEqual => "!=",
        CompareFunc::GreaterEqual => ">=",
        CompareFunc::Unknown(raw) => {
            return Err(Sm3WgslError::UnsupportedAlphaTestFunc {
                raw_func: Some(raw),
            });
        }
    };
    writeln!(
        source,
        "    let sm3_alpha_ref = round(clamp({}.a, 0.0, 1.0) * {:.1});",
        register_name(colour_output),
        ALPHA_REF_SCALE
    )
    .unwrap();
    writeln!(
        source,
        "    if !(sm3_alpha_ref {} {:.1}) {{ discard; }}",
        operator,
        f32::from(alpha_test.reference)
    )
    .unwrap();
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Vertex,
    Pixel,
}

fn stage_constant_registers(plan: &LoweringPlan) -> Vec<Sm3Register> {
    plan.external
        .iter()
        .copied()
        .filter(|register| register.file == Sm3RegisterFile::FloatConstant)
        .collect()
}

fn constant_block_len(
    abi: &PassLoweringAbi,
    plan: &LoweringPlan,
    stage: Stage,
) -> Result<usize, Sm3WgslError> {
    let bindings = match stage {
        Stage::Vertex => &abi.vertex_constants,
        Stage::Pixel => &abi.pixel_constants,
    };
    let mut len = 0usize;
    for register in stage_constant_registers(plan) {
        let bound = bindings
            .iter()
            .find(|binding| binding.register == register.index)
            .ok_or(Sm3WgslError::UnboundConstantRegister { register })?;

        if bound.program_defined {
            continue;
        }
        len = len.max(usize::from(register.index) + 1);
    }
    if plan.uses_relative_float {
        for binding in bindings {
            if binding.program_defined {
                continue;
            }
            len = len.max(usize::from(binding.register) + 1);
        }
    }
    Ok(len)
}

fn emit_constant_prologue(
    wgsl: &mut String,
    abi: &PassLoweringAbi,
    plan: &LoweringPlan,
    stage: Stage,
    arena_base: &str,
) -> Result<(), Sm3WgslError> {
    let bindings = match stage {
        Stage::Vertex => &abi.vertex_constants,
        Stage::Pixel => &abi.pixel_constants,
    };
    for register in stage_constant_registers(plan) {
        let bound = bindings
            .iter()
            .find(|binding| binding.register == register.index)
            .ok_or(Sm3WgslError::UnboundConstantRegister { register })?;
        if bound.program_defined {
            continue;
        }
        writeln!(
            wgsl,
            "    let {} = sm3_constants.c[{} + {}u];",
            register_name(register),
            arena_base,
            register.index
        )
        .unwrap();
    }
    Ok(())
}

fn emit_prologue(wgsl: &mut String, plan: &LoweringPlan, defined: &mut BTreeMap<Sm3Register, u8>) {
    for (register, values) in &plan.float_definitions {
        writeln!(
            wgsl,
            "    let {} = vec4<f32>(bitcast<f32>(0x{:08x}u), bitcast<f32>(0x{:08x}u), bitcast<f32>(0x{:08x}u), bitcast<f32>(0x{:08x}u));",
            register_name(*register),
            values[0],
            values[1],
            values[2],
            values[3]
        )
        .unwrap();
        defined.insert(*register, 0x0f);
    }
    for (register, values) in &plan.integer_definitions {
        writeln!(
            wgsl,
            "    let {} = vec4<i32>(bitcast<i32>(0x{:08x}u), bitcast<i32>(0x{:08x}u), bitcast<i32>(0x{:08x}u), bitcast<i32>(0x{:08x}u));",
            register_name(*register),
            values[0],
            values[1],
            values[2],
            values[3]
        )
        .unwrap();
        defined.insert(*register, 0x0f);
    }
    for register in &plan.writable {
        let init = if register.file == Sm3RegisterFile::Address {
            "vec4<i32>(0)"
        } else {
            "vec4<f32>(0.0)"
        };
        writeln!(wgsl, "    var {} = {init};", register_name(*register)).unwrap();
    }
}

fn write_texture_table_bindings(wgsl: &mut String, abi: &PassLoweringAbi) {
    if abi.samplers.is_empty() {
        return;
    }
    for (dimension, binding, texture_type) in [
        (
            SamplerTextureDimension::D2,
            TEXTURE_TABLE_BINDING_2D,
            "texture_2d<f32>",
        ),
        (
            SamplerTextureDimension::Cube,
            TEXTURE_TABLE_BINDING_CUBE,
            "texture_cube<f32>",
        ),
        (
            SamplerTextureDimension::D3,
            TEXTURE_TABLE_BINDING_3D,
            "texture_3d<f32>",
        ),
    ] {
        if abi.samplers.iter().any(|slot| slot.dimension == dimension) {
            writeln!(
                wgsl,
                "@group({TEXTURE_TABLE_GROUP}) @binding({binding}) var {}: binding_array<{texture_type}>;",
                texture_array_name(dimension)
            )
            .unwrap();
        }
    }
    writeln!(
        wgsl,
        "@group({TEXTURE_TABLE_GROUP}) @binding({TEXTURE_TABLE_BINDING_SAMPLERS}) var sm3_samplers: binding_array<sampler>;"
    )
    .unwrap();
    if abi.samplers.iter().any(|slot| slot.depth_compare) {
        wgsl.push_str(SHADOW_COMPARE_FN);
    }
}

// D3D9 depth-format shadow maps answer a fetch with a bilinear-filtered
// `z <= stored` test, not the stored depth; programs written for them average
// those answers, so a plain fetch of the float atlas yields garbage edges.
const SHADOW_COMPARE_FN: &str = "fn sm3_shadow_compare(slot: u32, uv: vec2<f32>, z: f32) -> f32 {
    let size = vec2<f32>(textureDimensions(sm3_textures_2d[slot & 0xffffu]));
    let stored = textureGather(0, sm3_textures_2d[slot & 0xffffu], sm3_samplers[slot >> 16u], uv);
    let lit = select(vec4<f32>(0.0), vec4<f32>(1.0), stored >= vec4<f32>(z));
    let f = fract(uv * size - 0.5);
    return mix(mix(lit.w, lit.z, f.x), mix(lit.x, lit.y, f.x), f.y);
}
";

fn emit_texture_slot_prologue(
    wgsl: &mut String,
    abi: &PassLoweringAbi,
    plan: &LoweringPlan,
    arena_base: &str,
    slot_row_base: usize,
) -> Result<(), Sm3WgslError> {
    for register in plan.samplers.keys() {
        let slot = abi
            .samplers
            .iter()
            .position(|bound| bound.register == *register)
            .ok_or(Sm3WgslError::UnboundSamplerRegister {
                register: *register,
            })?;
        writeln!(
            wgsl,
            "    let sm3_slot_s{register} = bitcast<vec4<u32>>(sm3_constants.c[{arena_base} + {}u]).{};",
            slot_row_base + slot / 4,
            component_name((slot % 4) as u8)
        )
        .unwrap();
    }
    Ok(())
}

fn attribute_wgsl_type(decl_type: DeclType) -> &'static str {
    match decl_type {
        DeclType::Float2 | DeclType::Float16x2 => "vec2<f32>",
        DeclType::Float3 => "vec3<f32>",

        DeclType::Float4 | DeclType::D3dColor | DeclType::UByte4N => "vec4<f32>",
        DeclType::UByte4 => "vec4<u32>",
        DeclType::Unknown(_) => {
            unreachable!("unknown declaration types are refused before emission")
        }
    }
}

fn attribute_expansion(decl_type: DeclType, location: u32) -> String {
    match decl_type {
        DeclType::Float2 | DeclType::Float16x2 => {
            format!("vec4<f32>(attribute_{location}.x, attribute_{location}.y, 0.0, 1.0)")
        }
        DeclType::Float3 => format!("vec4<f32>(attribute_{location}, 1.0)"),
        DeclType::Float4 | DeclType::UByte4N => format!("attribute_{location}"),
        DeclType::D3dColor => format!(
            "vec4<f32>(attribute_{location}.z, attribute_{location}.y, attribute_{location}.x, attribute_{location}.w)"
        ),
        DeclType::UByte4 => format!("vec4<f32>(attribute_{location})"),
        DeclType::Unknown(_) => {
            unreachable!("unknown declaration types are refused before emission")
        }
    }
}

fn is_external(file: Sm3RegisterFile) -> bool {
    matches!(
        file,
        Sm3RegisterFile::Input
            | Sm3RegisterFile::FloatConstant
            | Sm3RegisterFile::Texture
            | Sm3RegisterFile::Misc
    )
}

fn is_output(file: Sm3RegisterFile) -> bool {
    matches!(
        file,
        Sm3RegisterFile::RasterOutput
            | Sm3RegisterFile::AttributeOutput
            | Sm3RegisterFile::Output
            | Sm3RegisterFile::ColourOutput
            | Sm3RegisterFile::DepthOutput
    )
}

fn require_writable(at_word: usize, register: Sm3Register) -> Result<(), Sm3WgslError> {
    if matches!(
        register.file,
        Sm3RegisterFile::Temporary
            | Sm3RegisterFile::FloatConstant
            | Sm3RegisterFile::RasterOutput
            | Sm3RegisterFile::AttributeOutput
            | Sm3RegisterFile::Output
            | Sm3RegisterFile::ColourOutput
            | Sm3RegisterFile::DepthOutput
            | Sm3RegisterFile::Float16Temporary
            | Sm3RegisterFile::Address
    ) {
        Ok(())
    } else {
        Err(Sm3WgslError::UnsupportedDestinationFile {
            at_word,
            file: register.file,
        })
    }
}

enum FlowFrame {
    If {
        at_word: usize,
        defined_at_if: BTreeMap<Sm3Register, u8>,
        then_end: Option<BTreeMap<Sm3Register, u8>>,
    },
    Rep {
        at_word: usize,
        defined_before: BTreeMap<Sm3Register, u8>,
    },
}

impl FlowFrame {
    fn at_word(&self) -> usize {
        match self {
            Self::If { at_word, .. } | Self::Rep { at_word, .. } => *at_word,
        }
    }

    fn unclosed_kind(&self) -> &'static str {
        match self {
            Self::If { .. } => "unclosed-ifc",
            Self::Rep { .. } => "unclosed-rep",
        }
    }
}

fn merge_defined(
    before: &BTreeMap<Sm3Register, u8>,
    then_end: &BTreeMap<Sm3Register, u8>,
    else_end: &BTreeMap<Sm3Register, u8>,
) -> BTreeMap<Sm3Register, u8> {
    let mut out = before.clone();
    let mut registers: BTreeSet<Sm3Register> = BTreeSet::new();
    registers.extend(then_end.keys().copied());
    registers.extend(else_end.keys().copied());
    for register in registers {
        let then_mask = then_end.get(&register).copied().unwrap_or(0);
        let else_mask = else_end.get(&register).copied().unwrap_or(0);
        let before_mask = before.get(&register).copied().unwrap_or(0);
        let mask = before_mask | (then_mask & else_mask);
        if mask != 0 {
            out.insert(register, mask);
        } else {
            out.remove(&register);
        }
    }
    out
}

fn ifc_rel_op(controls: u8) -> Option<&'static str> {
    Some(match controls {
        1 => ">",
        2 => "==",
        3 => ">=",
        4 => "<",
        5 => "!=",
        6 => "<=",
        _ => return None,
    })
}

fn emit_vs_relative_const_loader(source: &mut String, len: usize) {
    source.push_str("fn sm3_vs_c(base: u32, i: i32) -> vec4<f32> {\n");
    if len == 0 {
        source.push_str("    return vec4<f32>(0.0);\n}\n");
        return;
    }
    writeln!(
        source,
        "    if i < 0 || i >= {len} {{ return vec4<f32>(0.0); }}"
    )
    .unwrap();
    source.push_str("    return sm3_constants.c[base + u32(i)];\n}\n");
}

fn emit_body(
    wgsl: &mut String,
    instructions: &[Sm3Instruction],
    external: &BTreeSet<Sm3Register>,
    samplers: &BTreeMap<u16, SamplerTextureDimension>,
    compare: &BTreeSet<u16>,
    defined: &mut BTreeMap<Sm3Register, u8>,
    relative_c: Option<&str>,
) -> Result<(), Sm3WgslError> {
    let mut flow = Vec::new();
    for instruction in instructions {
        emit_instruction(
            wgsl,
            instruction,
            external,
            samplers,
            compare,
            defined,
            &mut flow,
            relative_c,
        )?;
    }
    if let Some(frame) = flow.last() {
        return Err(Sm3WgslError::UnbalancedControlFlow {
            at_word: frame.at_word(),
            kind: frame.unclosed_kind(),
        });
    }
    Ok(())
}

fn emit_texkill(
    wgsl: &mut String,
    at_word: usize,
    register: Sm3Register,
    external: &BTreeSet<Sm3Register>,
    defined: &BTreeMap<Sm3Register, u8>,
    relative_c: Option<&str>,
) -> Result<(), Sm3WgslError> {
    let identity = Sm3Source {
        register,
        swizzle: [0, 1, 2, 3],
        modifier: Sm3SourceModifier::None,
        relative: None,
    };
    let x = source_component(at_word, &identity, 0, external, defined, relative_c)?;
    let y = source_component(at_word, &identity, 1, external, defined, relative_c)?;
    let z = source_component(at_word, &identity, 2, external, defined, relative_c)?;
    let w = source_component(at_word, &identity, 3, external, defined, relative_c)?;
    writeln!(
        wgsl,
        "    if ({x} < 0.0 || {y} < 0.0 || {z} < 0.0 || {w} < 0.0) {{ discard; }}"
    )
    .unwrap();
    Ok(())
}

fn emit_instruction(
    wgsl: &mut String,
    instruction: &Sm3Instruction,
    external: &BTreeSet<Sm3Register>,
    samplers: &BTreeMap<u16, SamplerTextureDimension>,
    compare: &BTreeSet<u16>,
    defined: &mut BTreeMap<Sm3Register, u8>,
    flow: &mut Vec<FlowFrame>,
    relative_c: Option<&str>,
) -> Result<(), Sm3WgslError> {
    match &instruction.body {
        Sm3InstructionBody::Declaration { .. }
        | Sm3InstructionBody::FloatDefinition { .. }
        | Sm3InstructionBody::IntegerDefinition { .. } => Ok(()),
        Sm3InstructionBody::Operation { sources, .. } if instruction.opcode == Sm3Opcode::Rep => {
            let source = &sources[0];
            let component = source.swizzle[0];
            match defined.get(&source.register) {
                Some(mask) if mask & (1 << component) != 0 => {}
                _ => {
                    return Err(Sm3WgslError::RegisterReadBeforeWrite {
                        at_word: instruction.at_word,
                        register: source.register,
                        component,
                    });
                }
            }
            writeln!(
                wgsl,
                "    var sm3_rep_{} = bitcast<u32>({}.{});",
                instruction.at_word,
                register_name(source.register),
                component_name(component)
            )
            .unwrap();
            writeln!(wgsl, "    loop {{").unwrap();
            writeln!(
                wgsl,
                "    if sm3_rep_{} == 0u {{ break; }}",
                instruction.at_word
            )
            .unwrap();
            writeln!(wgsl, "    sm3_rep_{} -= 1u;", instruction.at_word).unwrap();
            flow.push(FlowFrame::Rep {
                at_word: instruction.at_word,
                defined_before: defined.clone(),
            });
            Ok(())
        }
        Sm3InstructionBody::Operation { .. } if instruction.opcode == Sm3Opcode::EndRep => {
            let frame = flow.pop().ok_or(Sm3WgslError::UnbalancedControlFlow {
                at_word: instruction.at_word,
                kind: "endrep-without-rep",
            })?;
            let FlowFrame::Rep { defined_before, .. } = frame else {
                return Err(Sm3WgslError::UnbalancedControlFlow {
                    at_word: instruction.at_word,
                    kind: "endrep-crosses-ifc",
                });
            };
            *defined = defined_before;
            wgsl.push_str("    }\n");
            Ok(())
        }
        Sm3InstructionBody::Operation { sources, .. } if instruction.opcode == Sm3Opcode::Ifc => {
            let op = ifc_rel_op(instruction.controls).ok_or(
                Sm3WgslError::UnsupportedInstructionControls {
                    at_word: instruction.at_word,
                    opcode: instruction.opcode,
                    controls: instruction.controls,
                },
            )?;
            let left = source_component(
                instruction.at_word,
                &sources[0],
                0,
                external,
                defined,
                relative_c,
            )?;
            let right = source_component(
                instruction.at_word,
                &sources[1],
                0,
                external,
                defined,
                relative_c,
            )?;
            writeln!(wgsl, "    if ({left} {op} {right}) {{").unwrap();
            flow.push(FlowFrame::If {
                at_word: instruction.at_word,
                defined_at_if: defined.clone(),
                then_end: None,
            });
            Ok(())
        }
        Sm3InstructionBody::Operation { .. } if instruction.opcode == Sm3Opcode::Else => {
            let frame = flow.last_mut().ok_or(Sm3WgslError::UnbalancedControlFlow {
                at_word: instruction.at_word,
                kind: "else-without-ifc",
            })?;
            let FlowFrame::If {
                defined_at_if,
                then_end,
                ..
            } = frame
            else {
                return Err(Sm3WgslError::UnbalancedControlFlow {
                    at_word: instruction.at_word,
                    kind: "else-crosses-rep",
                });
            };
            if then_end.is_some() {
                return Err(Sm3WgslError::UnbalancedControlFlow {
                    at_word: instruction.at_word,
                    kind: "double-else",
                });
            }
            *then_end = Some(defined.clone());
            *defined = defined_at_if.clone();
            wgsl.push_str("    } else {\n");
            Ok(())
        }
        Sm3InstructionBody::Operation { .. } if instruction.opcode == Sm3Opcode::Endif => {
            let frame = flow.pop().ok_or(Sm3WgslError::UnbalancedControlFlow {
                at_word: instruction.at_word,
                kind: "endif-without-ifc",
            })?;
            let FlowFrame::If {
                defined_at_if,
                then_end,
                ..
            } = frame
            else {
                return Err(Sm3WgslError::UnbalancedControlFlow {
                    at_word: instruction.at_word,
                    kind: "endif-crosses-rep",
                });
            };
            let had_else = then_end.is_some();
            let then_end = then_end.unwrap_or_else(|| defined.clone());
            let else_end = if had_else {
                defined.clone()
            } else {
                defined_at_if.clone()
            };
            *defined = merge_defined(&defined_at_if, &then_end, &else_end);
            wgsl.push_str("    }\n");
            Ok(())
        }
        Sm3InstructionBody::Operation {
            destination: None, ..
        } => Ok(()),
        Sm3InstructionBody::Operation {
            destination: Some(destination),
            sources: _,
        } if instruction.opcode == Sm3Opcode::TexKill => emit_texkill(
            wgsl,
            instruction.at_word,
            destination.register,
            external,
            defined,
            relative_c,
        ),
        Sm3InstructionBody::Operation {
            destination: Some(destination),
            sources,
        } => {
            let aliases_destination = sources
                .iter()
                .any(|source| source.register == destination.register);
            for component in 0..4 {
                if destination.write_mask & (1 << component) == 0 {
                    continue;
                }
                let mut expression = operation_component(
                    instruction.at_word,
                    instruction.opcode,
                    instruction.controls,
                    sources,
                    component,
                    external,
                    samplers,
                    compare,
                    defined,
                    relative_c,
                )?;
                if destination.saturate {
                    if instruction.opcode == Sm3Opcode::Mova {
                        return Err(Sm3WgslError::SaturateOnAddress {
                            at_word: instruction.at_word,
                        });
                    }
                    expression = format!("clamp({expression}, 0.0, 1.0)");
                }
                if aliases_destination {
                    writeln!(
                        wgsl,
                        "    let sm3_value_{}_{} = {};",
                        instruction.at_word,
                        component_name(component),
                        expression
                    )
                    .unwrap();
                } else {
                    writeln!(
                        wgsl,
                        "    {}.{} = {};",
                        register_name(destination.register),
                        component_name(component),
                        expression
                    )
                    .unwrap();
                }
            }
            if aliases_destination {
                for component in 0..4 {
                    if destination.write_mask & (1 << component) == 0 {
                        continue;
                    }
                    writeln!(
                        wgsl,
                        "    {}.{} = sm3_value_{}_{};",
                        register_name(destination.register),
                        component_name(component),
                        instruction.at_word,
                        component_name(component)
                    )
                    .unwrap();
                }
            }
            *defined.entry(destination.register).or_insert(0) |= destination.write_mask;
            Ok(())
        }
        Sm3InstructionBody::Unmodeled { .. } => {
            unreachable!("lowering surface rejects unknown opcodes")
        }
    }
}

fn operation_component(
    at_word: usize,
    opcode: Sm3Opcode,
    controls: u8,
    sources: &[Sm3Source],
    component: u8,
    external: &BTreeSet<Sm3Register>,
    samplers: &BTreeMap<u16, SamplerTextureDimension>,
    compare: &BTreeSet<u16>,
    defined: &BTreeMap<Sm3Register, u8>,
    relative_c: Option<&str>,
) -> Result<String, Sm3WgslError> {
    let scalar = |source: &Sm3Source, output_component: u8| {
        source_component(
            at_word,
            source,
            output_component,
            external,
            defined,
            relative_c,
        )
    };
    let expression = match opcode {
        Sm3Opcode::Mov => scalar(&sources[0], component)?,

        Sm3Opcode::Mova => format!("i32(round({}))", scalar(&sources[0], component)?),
        Sm3Opcode::Add => format!(
            "({} + {})",
            scalar(&sources[0], component)?,
            scalar(&sources[1], component)?
        ),

        Sm3Opcode::Sub => format!(
            "({} - {})",
            scalar(&sources[0], component)?,
            scalar(&sources[1], component)?
        ),
        Sm3Opcode::Mad => format!(
            "({} * {} + {})",
            scalar(&sources[0], component)?,
            scalar(&sources[1], component)?,
            scalar(&sources[2], component)?
        ),
        Sm3Opcode::Mul => format!(
            "({} * {})",
            scalar(&sources[0], component)?,
            scalar(&sources[1], component)?
        ),
        Sm3Opcode::Rcp => format!("(1.0 / {})", scalar(&sources[0], 0)?),
        Sm3Opcode::Rsq => format!("inverseSqrt(abs({}))", scalar(&sources[0], 0)?),
        Sm3Opcode::Dp3 => dot(
            at_word,
            &sources[0],
            &sources[1],
            3,
            external,
            defined,
            relative_c,
        )?,
        Sm3Opcode::Dp4 => dot(
            at_word,
            &sources[0],
            &sources[1],
            4,
            external,
            defined,
            relative_c,
        )?,

        Sm3Opcode::Min => format!(
            "min({}, {})",
            scalar(&sources[0], component)?,
            scalar(&sources[1], component)?
        ),
        Sm3Opcode::Max => format!(
            "max({}, {})",
            scalar(&sources[0], component)?,
            scalar(&sources[1], component)?
        ),

        Sm3Opcode::Slt => format!(
            "select(0.0, 1.0, {} < {})",
            scalar(&sources[0], component)?,
            scalar(&sources[1], component)?
        ),

        Sm3Opcode::Sge => format!(
            "select(0.0, 1.0, {} >= {})",
            scalar(&sources[0], component)?,
            scalar(&sources[1], component)?
        ),

        Sm3Opcode::Dsx => format!("dpdx({})", scalar(&sources[0], component)?),
        Sm3Opcode::Dsy => format!("dpdy({})", scalar(&sources[0], component)?),
        Sm3Opcode::Exp => format!("exp2({})", scalar(&sources[0], 0)?),

        Sm3Opcode::Log => format!("log2(abs({}))", scalar(&sources[0], 0)?),

        Sm3Opcode::Frc => format!("fract({})", scalar(&sources[0], component)?),

        Sm3Opcode::Abs => format!("abs({})", scalar(&sources[0], component)?),

        Sm3Opcode::Sincos => {
            if component >= 2 {
                return Err(Sm3WgslError::InvalidSincosDestination { at_word, component });
            }
            let angle = scalar(&sources[0], 0)?;
            if component == 0 {
                format!("cos({angle})")
            } else {
                format!("sin({angle})")
            }
        }

        Sm3Opcode::Cmp => {
            let value = scalar(&sources[0], component)?;
            let if_nonneg = scalar(&sources[1], component)?;
            let if_neg = scalar(&sources[2], component)?;
            format!("select({if_neg}, {if_nonneg}, {value} >= 0.0)")
        }
        Sm3Opcode::Lrp => {
            let a = scalar(&sources[0], component)?;
            let b = scalar(&sources[1], component)?;
            let c = scalar(&sources[2], component)?;
            format!("({a} * {b} + (1.0 - {a}) * {c})")
        }
        Sm3Opcode::Nrm => {
            if component == 3 {
                return Err(Sm3WgslError::InvalidNormaliseDestination { at_word, component });
            }
            let values = (0..3)
                .map(|index| scalar(&sources[0], index))
                .collect::<Result<Vec<_>, _>>()?;
            format!(
                "normalize(vec3<f32>({}, {}, {}))[{}]",
                values[0], values[1], values[2], component
            )
        }
        Sm3Opcode::Pow => format!(
            "pow(abs({}), {})",
            scalar(&sources[0], 0)?,
            scalar(&sources[1], 0)?
        ),
        Sm3Opcode::Dp2Add => {
            let x0 = scalar(&sources[0], 0)?;
            let y0 = scalar(&sources[0], 1)?;
            let x1 = scalar(&sources[1], 0)?;
            let y1 = scalar(&sources[1], 1)?;
            format!("({x0} * {x1} + {y0} * {y1} + {})", scalar(&sources[2], 0)?)
        }
        Sm3Opcode::TexLd | Sm3Opcode::TexLdL | Sm3Opcode::TexLdD => texture_component(
            at_word, opcode, controls, sources, component, external, samplers, compare, defined,
            relative_c,
        )?,
        Sm3Opcode::Nop
        | Sm3Opcode::Dcl
        | Sm3Opcode::Def
        | Sm3Opcode::DefI
        | Sm3Opcode::Rep
        | Sm3Opcode::EndRep
        | Sm3Opcode::TexKill
        | Sm3Opcode::Ifc
        | Sm3Opcode::Else
        | Sm3Opcode::Endif
        | Sm3Opcode::Unknown(_) => {
            unreachable!("non-operation opcode reached expression lowering")
        }
    };
    Ok(expression)
}

fn dot(
    at_word: usize,
    left: &Sm3Source,
    right: &Sm3Source,
    count: u8,
    external: &BTreeSet<Sm3Register>,
    defined: &BTreeMap<Sm3Register, u8>,
    relative_c: Option<&str>,
) -> Result<String, Sm3WgslError> {
    let left = (0..count)
        .map(|component| source_component(at_word, left, component, external, defined, relative_c))
        .collect::<Result<Vec<_>, _>>()?;
    let right = (0..count)
        .map(|component| source_component(at_word, right, component, external, defined, relative_c))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((0..usize::from(count))
        .map(|index| format!("{} * {}", left[index], right[index]))
        .collect::<Vec<_>>()
        .join(" + "))
}

fn texture_component(
    at_word: usize,
    opcode: Sm3Opcode,
    controls: u8,
    sources: &[Sm3Source],
    component: u8,
    external: &BTreeSet<Sm3Register>,
    samplers: &BTreeMap<u16, SamplerTextureDimension>,
    compare: &BTreeSet<u16>,
    defined: &BTreeMap<Sm3Register, u8>,
    relative_c: Option<&str>,
) -> Result<String, Sm3WgslError> {
    let sampler = &sources[1];
    if sampler.register.file != Sm3RegisterFile::Sampler
        || sampler.modifier != Sm3SourceModifier::None
        || sampler.relative.is_some()
    {
        return Err(Sm3WgslError::InvalidSamplerOperand { at_word });
    }
    let projected = opcode == Sm3Opcode::TexLd && controls == TEXLD_PROJECT;
    let x = source_component(at_word, &sources[0], 0, external, defined, relative_c)?;
    let y = source_component(at_word, &sources[0], 1, external, defined, relative_c)?;
    let dimension = samplers.get(&sampler.register.index).copied().ok_or(
        Sm3WgslError::SamplerDeclarationMissing {
            register: sampler.register.index,
        },
    )?;
    if projected && dimension == SamplerTextureDimension::Cube {
        return Err(Sm3WgslError::UnsupportedInstructionControls {
            at_word,
            opcode,
            controls,
        });
    }
    let w = if projected {
        Some(source_component(
            at_word,
            &sources[0],
            3,
            external,
            defined,
            relative_c,
        )?)
    } else {
        None
    };
    let coordinate = match dimension {
        SamplerTextureDimension::D2 => match &w {
            Some(w) => format!("vec2<f32>({x} / {w}, {y} / {w})"),
            None => format!("vec2<f32>({x}, {y})"),
        },
        SamplerTextureDimension::Cube | SamplerTextureDimension::D3 => {
            let z = source_component(at_word, &sources[0], 2, external, defined, relative_c)?;
            match &w {
                Some(w) => format!("vec3<f32>({x} / {w}, {y} / {w}, {z} / {w})"),
                None => format!("vec3<f32>({x}, {y}, {z})"),
            }
        }
    };
    let slot = format!("sm3_slot_s{}", sampler.register.index);
    if compare.contains(&sampler.register.index)
        && dimension == SamplerTextureDimension::D2
        && opcode != Sm3Opcode::TexLdD
    {
        let z = source_component(at_word, &sources[0], 2, external, defined, relative_c)?;
        let z = match &w {
            Some(w) => format!("{z} / {w}"),
            None => z,
        };
        return Ok(format!("sm3_shadow_compare({slot}, {coordinate}, {z})"));
    }
    let texture = format!("{}[{slot} & 0xffffu]", texture_array_name(dimension));
    let sampler_expr = format!("sm3_samplers[{slot} >> 16u]");
    let sample = if opcode == Sm3Opcode::TexLdL {
        let level = source_component(at_word, &sources[0], 3, external, defined, relative_c)?;
        format!("textureSampleLevel({texture}, {sampler_expr}, {coordinate}, {level})")
    } else if opcode == Sm3Opcode::TexLdD {
        if sources.len() < 4 {
            return Err(Sm3WgslError::InvalidSamplerOperand { at_word });
        }
        let ddx = gradient_vec(
            at_word,
            &sources[2],
            dimension,
            external,
            defined,
            relative_c,
        )?;
        let ddy = gradient_vec(
            at_word,
            &sources[3],
            dimension,
            external,
            defined,
            relative_c,
        )?;
        format!("textureSampleGrad({texture}, {sampler_expr}, {coordinate}, {ddx}, {ddy})")
    } else {
        format!("textureSample({texture}, {sampler_expr}, {coordinate})")
    };
    Ok(format!("{sample}.{}", component_name(component)))
}

fn gradient_vec(
    at_word: usize,
    source: &Sm3Source,
    dimension: SamplerTextureDimension,
    external: &BTreeSet<Sm3Register>,
    defined: &BTreeMap<Sm3Register, u8>,
    relative_c: Option<&str>,
) -> Result<String, Sm3WgslError> {
    let x = source_component(at_word, source, 0, external, defined, relative_c)?;
    let y = source_component(at_word, source, 1, external, defined, relative_c)?;
    match dimension {
        SamplerTextureDimension::D2 => Ok(format!("vec2<f32>({x}, {y})")),
        SamplerTextureDimension::Cube | SamplerTextureDimension::D3 => {
            let z = source_component(at_word, source, 2, external, defined, relative_c)?;
            Ok(format!("vec3<f32>({x}, {y}, {z})"))
        }
    }
}

fn source_component(
    at_word: usize,
    source: &Sm3Source,
    output_component: u8,
    external: &BTreeSet<Sm3Register>,
    defined: &BTreeMap<Sm3Register, u8>,
    relative_c: Option<&str>,
) -> Result<String, Sm3WgslError> {
    if source.register.file == Sm3RegisterFile::Sampler {
        return Err(Sm3WgslError::InvalidSamplerOperand { at_word });
    }
    let component = source.swizzle[usize::from(output_component)];
    let base = if let Some(relative) = source.relative {
        let helper = relative_c.ok_or(Sm3WgslError::RelativeConstantsNeedPassAbi)?;
        if source.register.file != Sm3RegisterFile::FloatConstant {
            return Err(Sm3WgslError::UnsupportedSourceFile {
                at_word,
                file: source.register.file,
            });
        }
        let rel_component = relative.component;
        match defined.get(&relative.register) {
            Some(mask) if mask & (1 << rel_component) != 0 => {}
            _ => {
                return Err(Sm3WgslError::RegisterReadBeforeWrite {
                    at_word,
                    register: relative.register,
                    component: rel_component,
                });
            }
        }
        format!(
            "{helper}(sm3_constant_base, i32({}) + {}.{}).{}",
            source.register.index,
            register_name(relative.register),
            component_name(rel_component),
            component_name(component)
        )
    } else if let Some(mask) = defined.get(&source.register) {
        if mask & (1 << component) == 0 {
            return Err(Sm3WgslError::RegisterReadBeforeWrite {
                at_word,
                register: source.register,
                component,
            });
        }
        format!(
            "{}.{}",
            register_name(source.register),
            component_name(component)
        )
    } else if external.contains(&source.register) {
        format!(
            "{}.{}",
            register_name(source.register),
            component_name(component)
        )
    } else if matches!(
        source.register.file,
        Sm3RegisterFile::Temporary | Sm3RegisterFile::Float16Temporary
    ) {
        return Err(Sm3WgslError::RegisterReadBeforeWrite {
            at_word,
            register: source.register,
            component,
        });
    } else {
        return Err(Sm3WgslError::UnsupportedSourceFile {
            at_word,
            file: source.register.file,
        });
    };
    Ok(match source.modifier {
        Sm3SourceModifier::None => base,
        Sm3SourceModifier::Negate => format!("(-{base})"),
        Sm3SourceModifier::Bias => format!("({base} - 0.5)"),
        Sm3SourceModifier::BiasNegate => format!("(0.5 - {base})"),
        Sm3SourceModifier::Sign => format!("({base} * 2.0 - 1.0)"),
        Sm3SourceModifier::SignNegate => format!("(1.0 - {base} * 2.0)"),
        Sm3SourceModifier::Complement => format!("(1.0 - {base})"),
        Sm3SourceModifier::Double => format!("({base} * 2.0)"),
        Sm3SourceModifier::DoubleNegate => format!("(-{base} * 2.0)"),
        Sm3SourceModifier::DivideByZ | Sm3SourceModifier::DivideByW => {
            let divisor_component = if source.modifier == Sm3SourceModifier::DivideByZ {
                2
            } else {
                3
            };
            let mut divisor = *source;
            divisor.modifier = Sm3SourceModifier::None;
            let divisor = source_component(
                at_word,
                &divisor,
                divisor_component,
                external,
                defined,
                relative_c,
            )?;
            format!("({base} / {divisor})")
        }
        Sm3SourceModifier::Absolute => format!("abs({base})"),
        Sm3SourceModifier::AbsoluteNegate => format!("(-abs({base}))"),
        Sm3SourceModifier::LogicalNot => format!("select(0.0, 1.0, {base} == 0.0)"),
        Sm3SourceModifier::Unknown(_) => unreachable!("lowering surface rejects unknown modifiers"),
    })
}

fn register_name(register: Sm3Register) -> String {
    let prefix = match register.file {
        Sm3RegisterFile::Temporary => "r",
        Sm3RegisterFile::Input => "v",
        Sm3RegisterFile::FloatConstant => "c",
        Sm3RegisterFile::Texture => "t",
        Sm3RegisterFile::Address => "a",
        Sm3RegisterFile::RasterOutput => "or",
        Sm3RegisterFile::AttributeOutput => "oa",
        Sm3RegisterFile::Output => "o",
        Sm3RegisterFile::IntegerConstant => "i",
        Sm3RegisterFile::ColourOutput => "oc",
        Sm3RegisterFile::DepthOutput => "od",
        Sm3RegisterFile::BooleanConstant => "b",
        Sm3RegisterFile::Loop => "al",
        Sm3RegisterFile::Float16Temporary => "h",
        Sm3RegisterFile::Misc => "misc",
        Sm3RegisterFile::Label => "label",
        Sm3RegisterFile::Predicate => "p",
        Sm3RegisterFile::Sampler => "s",
        Sm3RegisterFile::Unknown(value) => return format!("unknown{value}_{}", register.index),
    };
    format!("{prefix}{}", register.index)
}

fn component_name(component: u8) -> char {
    ['x', 'y', 'z', 'w'][usize::from(component)]
}
