use crate::{
    PassAbiRefusal, PassProgramAbi, PortId, RuntimeMaterialCatalog, RuntimePass, RuntimeShaderPair,
    RuntimeShaderProgram, RuntimeShaderStage, Sm3IrError, Sm3ProgramIr,
};

pub struct PreparedProgramAbi<'a> {
    id: PortId,
    pair: RuntimeShaderPair,
    vertex_name: &'a str,
    pixel_name: &'a str,
    abi: PassProgramAbi,
    programs: PreparedShaderPrograms,
}

pub enum PreparedShaderPrograms {
    Sm3 {
        vertex: Sm3ProgramIr,
        pixel: Sm3ProgramIr,
    },
    Dxbc {
        vertex: Box<dxbc_sm5::wgsl::Shader>,
        pixel: Box<dxbc_sm5::wgsl::Shader>,
        lowering: dxbc_sm5::wgsl::PassAbi,
    },
}

impl<'a> PreparedProgramAbi<'a> {
    pub fn id(&self) -> PortId {
        self.id
    }
    pub fn pair(&self) -> RuntimeShaderPair {
        self.pair
    }
    pub fn names(&self) -> (&'a str, &'a str) {
        (self.vertex_name, self.pixel_name)
    }
    pub fn into_programs(self) -> (PassProgramAbi, PreparedShaderPrograms) {
        (self.abi, self.programs)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgramAbiRefusal {
    PassShaderPairMissing,
    MissingExactSource {
        pair: RuntimeShaderPair,
        stage: RuntimeShaderStage,
    },
    StageMismatch {
        pair: RuntimeShaderPair,
        expected: RuntimeShaderStage,
        actual: RuntimeShaderStage,
    },
    MissingVertexDeclaration {
        pointer_identity: u32,
    },
    Decode {
        stage: RuntimeShaderStage,
        cause: Sm3IrError,
    },
    Abi(PassAbiRefusal),
    DxbcStageMismatch {
        expected: dxbc_sm5::ProgramKind,
        actual: dxbc_sm5::ProgramKind,
    },
}

impl ProgramAbiRefusal {
    pub fn census_key(&self) -> String {
        match self {
            Self::StageMismatch { .. } => "StageMismatch".into(),
            Self::Abi(refusal) => abi_census_key(*refusal),
            Self::Decode { cause, .. } => format!("Decode({cause:?})")
                .split(['{', '('])
                .next()
                .unwrap_or("Decode")
                .trim()
                .to_owned(),
            other => {
                let text = format!("{other:?}");
                text.split(['{', '('])
                    .next()
                    .unwrap_or(&text)
                    .trim()
                    .to_owned()
            }
        }
    }
}

pub fn prepare_program_abi<'a>(
    catalog: &'a RuntimeMaterialCatalog,
    pass: &RuntimePass,
    vertex_type: u8,
) -> Result<PreparedProgramAbi<'a>, ProgramAbiRefusal> {
    let pair = pass
        .shader_pair
        .ok_or(ProgramAbiRefusal::PassShaderPairMissing)?;
    let vertex_source = exact_stage_source(catalog, pair, RuntimeShaderStage::Vertex)?;
    let pixel_source = exact_stage_source(catalog, pair, RuntimeShaderStage::Pixel)?;
    let decl = || {
        catalog.vertex_decl(pair.vertex_decl_slot).ok_or(
            ProgramAbiRefusal::MissingVertexDeclaration {
                pointer_identity: pair.vertex_decl_slot,
            },
        )
    };
    let (abi, programs) = if crate::dxbc_abi::is_dxbc_program(&vertex_source.program) {
        let parse = |source: &RuntimeShaderProgram| {
            dxbc_sm5::wgsl::Shader::parse(&source.program)
                .map_err(|_| ProgramAbiRefusal::Abi(PassAbiRefusal::DxbcProgram))
        };
        let (vertex, pixel) = (parse(vertex_source)?, parse(pixel_source)?);
        for (shader, expected) in [
            (&vertex, dxbc_sm5::ProgramKind::Vertex),
            (&pixel, dxbc_sm5::ProgramKind::Pixel),
        ] {
            if shader.kind != expected {
                return Err(ProgramAbiRefusal::DxbcStageMismatch {
                    expected,
                    actual: shader.kind,
                });
            }
        }
        let (abi, lowering) = crate::dxbc_abi::build_dxbc_pass_abi(
            &vertex,
            &pixel,
            decl()?,
            vertex_type,
            &pass.arguments,
            pass.t5_custom_sampler_flags | pass.custom_sampler_flags,
        )
        .map_err(ProgramAbiRefusal::Abi)?;
        (
            abi,
            PreparedShaderPrograms::Dxbc {
                vertex: Box::new(vertex),
                pixel: Box::new(pixel),
                lowering,
            },
        )
    } else {
        let decode = |source: &RuntimeShaderProgram, stage| {
            crate::decode_sm3_program(&source.program, stage)
                .map_err(|cause| ProgramAbiRefusal::Decode { stage, cause })
        };
        let (vertex, pixel) = (
            decode(vertex_source, RuntimeShaderStage::Vertex)?,
            decode(pixel_source, RuntimeShaderStage::Pixel)?,
        );
        let abi = crate::sm3_abi::build_pass_abi(
            &vertex,
            &pixel,
            decl()?,
            vertex_type,
            &pass.arguments,
            pass.custom_sampler_flags,
            pass.t5_custom_sampler_flags,
            pass.hardware_shadow_compare,
        )
        .map_err(ProgramAbiRefusal::Abi)?;
        (abi, PreparedShaderPrograms::Sm3 { vertex, pixel })
    };
    let id =
        PortId::from_pass(pass, vertex_type).ok_or(ProgramAbiRefusal::PassShaderPairMissing)?;
    Ok(PreparedProgramAbi {
        id,
        pair,
        vertex_name: &vertex_source.name,
        pixel_name: &pixel_source.name,
        abi,
        programs,
    })
}

fn exact_stage_source(
    catalog: &RuntimeMaterialCatalog,
    pair: RuntimeShaderPair,
    expected: RuntimeShaderStage,
) -> Result<&RuntimeShaderProgram, ProgramAbiRefusal> {
    let id = match expected {
        RuntimeShaderStage::Vertex => pair.vertex,
        RuntimeShaderStage::Pixel => pair.pixel,
    };
    let program = catalog
        .shader_program(id)
        .ok_or(ProgramAbiRefusal::MissingExactSource {
            pair,
            stage: expected,
        })?;
    if program.stage != expected {
        return Err(ProgramAbiRefusal::StageMismatch {
            pair,
            expected,
            actual: program.stage,
        });
    }
    Ok(program)
}

fn abi_census_key(refusal: crate::sm3_abi::PassAbiRefusal) -> String {
    match refusal {
        crate::sm3_abi::PassAbiRefusal::UnsupportedSamplerTextureType { texture_type, .. } => {
            format!("Abi/UnsupportedSamplerTextureType({texture_type})")
        }
        crate::sm3_abi::PassAbiRefusal::UnknownArgumentType { argument_type } => {
            format!("Abi/UnknownArgumentType({argument_type})")
        }
        crate::sm3_abi::PassAbiRefusal::VertexTypeMissingSource {
            vertex_type,
            source,
        } => format!("Abi/VertexTypeMissingSource({vertex_type},{source})"),
        crate::sm3_abi::PassAbiRefusal::ConstantRegisterUnbound { stage, register } => {
            format!("Abi/ConstantRegisterUnbound({stage:?},{register})")
        }
        crate::sm3_abi::PassAbiRefusal::VertexInputUnrouted { semantic, .. } => {
            format!("Abi/VertexInputUnrouted({semantic:?})")
        }
        crate::sm3_abi::PassAbiRefusal::SamplerRegisterConflict { register } => {
            format!("Abi/SamplerRegisterConflict({register})")
        }
        crate::sm3_abi::PassAbiRefusal::SamplerRegisterUnbound { register } => {
            format!("Abi/SamplerRegisterUnbound({register})")
        }
        other => {
            let text = format!("{other:?}");
            let name = text.split(['{', '(']).next().unwrap_or(&text).trim();
            format!("Abi/{name}")
        }
    }
}
