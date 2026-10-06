use d3d9_sm3::{
    ConstantSlot, DeclType, PassLoweringAbi, PassWgsl, SamplerSlot, Sm3ProgramIr, Sm3Wgsl,
    VaryingLink, VertexInput, lower_pass_to_wgsl,
};
use render_material::MATERIAL_ALPHA_TESTS;

use super::sm3_abi::{ConstantSource, PassProgramAbi};

pub use d3d9_sm3::{PASS_FRAGMENT_ENTRY, PASS_VERTEX_ENTRY};

pub type ValidatedSm3Wgsl = Sm3Wgsl;

pub type ValidatedPassWgsl = PassWgsl;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sm3WgslError {
    Emit(d3d9_sm3::Sm3WgslError),
    WgslParse(String),
    WgslValidation(String),
}

impl From<d3d9_sm3::Sm3WgslError> for Sm3WgslError {
    fn from(value: d3d9_sm3::Sm3WgslError) -> Self {
        Self::Emit(value)
    }
}

fn known_decl_type(decl_type: DeclType) -> DeclType {
    match decl_type {
        DeclType::Unknown(_) => unreachable!("the ABI refuses unknown declaration types"),
        known => known,
    }
}

pub fn pass_lowering_abi(abi: &PassProgramAbi) -> PassLoweringAbi {
    PassLoweringAbi {
        vertex_inputs: abi
            .vertex_inputs
            .iter()
            .map(|input| VertexInput {
                register: input.register,
                location: input.attribute.map(|attribute| attribute.location),
                decl_type: input
                    .attribute
                    .map(|attribute| known_decl_type(attribute.layout.decl_type)),
                semantic: input.semantic,
            })
            .collect(),
        position: VaryingLink {
            semantic: abi.position.semantic,
            vertex_register: abi.position.vertex_register,
            pixel_register: abi.position.pixel_register,
            location: abi.position.location,
        },
        varyings: abi
            .varyings
            .iter()
            .map(|varying| VaryingLink {
                semantic: varying.semantic,
                vertex_register: varying.vertex_register,
                pixel_register: varying.pixel_register,
                location: varying.location,
            })
            .collect(),
        vertex_constants: abi
            .vertex_constants
            .iter()
            .map(|binding| ConstantSlot {
                register: binding.register,
                program_defined: binding.source == ConstantSource::ProgramDefined,
            })
            .collect(),
        pixel_constants: abi
            .pixel_constants
            .iter()
            .map(|binding| ConstantSlot {
                register: binding.register,
                program_defined: binding.source == ConstantSource::ProgramDefined,
            })
            .collect(),
        samplers: abi
            .samplers
            .iter()
            .map(|binding| SamplerSlot {
                register: binding.register,
                dimension: binding.dimension,
                depth_compare: binding.depth_compare,
            })
            .collect(),
        alpha_tests: MATERIAL_ALPHA_TESTS.to_vec(),
    }
}

pub(crate) fn validate_wgsl(source: &str) -> Result<(), Sm3WgslError> {
    let module = naga::front::wgsl::parse_str(source)
        .map_err(|error| Sm3WgslError::WgslParse(error.emit_to_string(source)))?;
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map_err(|error| Sm3WgslError::WgslValidation(error.to_string()))?;
    Ok(())
}

pub fn lower_pass_to_validated_wgsl(
    abi: &PassProgramAbi,
    vertex: &Sm3ProgramIr,
    pixel: &Sm3ProgramIr,
) -> Result<ValidatedPassWgsl, Sm3WgslError> {
    let lowered = lower_pass_to_wgsl(&pass_lowering_abi(abi), vertex, pixel)?;
    validate_wgsl(&lowered.source)?;
    Ok(lowered)
}

pub(crate) fn dxbc_alpha_tests() -> Vec<(String, String)> {
    MATERIAL_ALPHA_TESTS
        .iter()
        .enumerate()
        .map(|(index, test)| {
            let operator = match test.func {
                d3d9_sm3::CompareFunc::Never => return (index, Some("discard;".to_owned())),
                d3d9_sm3::CompareFunc::Always => return (index, Some(String::new())),
                d3d9_sm3::CompareFunc::Less => "<",
                d3d9_sm3::CompareFunc::Equal => "==",
                d3d9_sm3::CompareFunc::LessEqual => "<=",
                d3d9_sm3::CompareFunc::Greater => ">",
                d3d9_sm3::CompareFunc::NotEqual => "!=",
                d3d9_sm3::CompareFunc::GreaterEqual => ">=",
                d3d9_sm3::CompareFunc::Unknown(_) => return (index, None),
            };
            (
                index,
                Some(format!(
                    "if !(round(clamp(dx_colour.a, 0.0, 1.0) * {:.1}) {operator} {:.1}) {{ discard; }}",
                    d3d9_sm3::abi::ALPHA_REF_SCALE,
                    f32::from(test.reference)
                )),
            )
        })
        .filter_map(|(index, test)| {
            Some((d3d9_sm3::pass_fragment_alpha_test_entry(index), test?))
        })
        .collect()
}
