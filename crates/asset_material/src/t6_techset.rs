mod material;
pub use material::T6MaterialRefusal;
use std::collections::BTreeSet;

use dxbc_sm5::wgsl::{ConstantRow, Shader, TextureDimension, TextureSlot};
use dxbc_sm5::{BindingKind, Reflection};
use fastfile_iw4::AssetType;

use crate::asset_graph::AssetRef;
use crate::material_catalog::{
    AssetPointerIdentity, AuthoredShader, AuthoredVertexDecl, MaterialCatalog, OwnedMaterialPass,
    OwnedShaderArgument, OwnedShaderRef, OwnedTechnique, OwnedTechniqueGraph, TechniqueSetFacts,
    TechniqueTable,
};
use crate::t5_code_remap::{iw4_code_const_index, iw4_code_texture_index};
use crate::t5_tech_map::{IW4_TECHNIQUE_TYPE_COUNT, IW4_TECHNIQUE_TYPE_NAMES};

pub mod argument_type {
    pub const MATERIAL_VERTEX_CONST: u16 = 0;
    pub const LITERAL_VERTEX_CONST: u16 = 1;
    pub const MATERIAL_PIXEL_SAMPLER: u16 = 2;
    pub const CODE_VERTEX_CONST: u16 = 3;
    pub const CODE_PIXEL_SAMPLER: u16 = 4;
    pub const CODE_PIXEL_CONST: u16 = 5;
    pub const MATERIAL_PIXEL_CONST: u16 = 6;
    pub const LITERAL_PIXEL_CONST: u16 = 7;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct T6Argument {
    pub kind: u16,
    pub offset: u16,
    pub size: u8,
    pub buffer: u16,
    pub def: u32,
    pub literal: Option<[u32; 4]>,
}

#[derive(Clone, Debug)]
pub struct T6Pass {
    pub vertex_name: String,
    pub vertex: Vec<u8>,
    pub pixel_name: String,
    pub pixel: Vec<u8>,
    pub custom_sampler_flags: u8,
    pub arguments: Vec<T6Argument>,
    pub layer_routing: Vec<[u8; 2]>,
}

#[derive(Clone, Debug)]
pub struct T6Technique {
    pub flags: u16,
    pub passes: Vec<T6Pass>,
}

#[derive(Clone, Debug)]
pub struct T6TechniqueSet {
    pub name: String,
    pub world_vert_format: u8,
    pub techniques: Vec<Option<T6Technique>>,
}

pub const T6_TECHNIQUE_LIT: usize = 4;
pub const T6_TECHNIQUE_EMISSIVE: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum T6Draw {
    Lit,
    Emissive,
}

impl T6Draw {
    pub fn technique_set_name(self, set: &str) -> String {
        match self {
            Self::Lit => set.to_owned(),
            Self::Emissive => format!("{set}$emissive"),
        }
    }
}

const GFXS0_COLORWRITE_RGB: u32 = 0x0800_0000;

pub const T6_TECHNIQUE_TYPE_NAMES: [&str; 36] = [
    "depth prepass",
    "build shadowmap depth",
    "unlit",
    "emissive",
    "lit",
    "lit sun",
    "lit sun shadow",
    "lit spot",
    "lit spot shadow",
    "lit spot square",
    "lit spot square shadow",
    "lit spot round",
    "lit spot round shadow",
    "lit omni",
    "lit omni shadow",
    "lit dlight glight",
    "lit sun dlight glight",
    "lit sun shadow dlight glight",
    "lit spot dlight glight",
    "lit spot shadow dlight glight",
    "lit spot square dlight glight",
    "lit spot square shadow dlight glight",
    "lit spot round dlight glight",
    "lit spot round shadow dlight glight",
    "lit omni dlight glight",
    "lit omni shadow dlight glight",
    "light spot",
    "light omni",
    "fakelight normal",
    "fakelight view",
    "sunlight preview",
    "case texture",
    "solid wireframe",
    "shaded wireframe",
    "debug bumpmap",
    "debug performance",
];

fn t6_techniques_for_iw4_slot(iw4: &str) -> Vec<usize> {
    let name = iw4.strip_suffix(" dfog").unwrap_or(iw4);
    let mut names = vec![name.to_owned()];
    if let Some(unshadowed) = name.strip_suffix(" shadow") {
        names.push(unshadowed.to_owned());
    }
    if name == "build shadowmap color" {
        names.push("build shadowmap depth".to_owned());
    }
    if name.starts_with("lit") && !name.contains("instanced") {
        names.push("lit".to_owned());
        names.extend(
            T6_TECHNIQUE_TYPE_NAMES
                .iter()
                .filter(|t6| t6.starts_with("lit"))
                .map(|t6| (*t6).to_owned()),
        );
    }
    names
        .iter()
        .filter_map(|name| T6_TECHNIQUE_TYPE_NAMES.iter().position(|t6| t6 == name))
        .collect()
}

fn select_t6_technique(
    set: &T6TechniqueSet,
    draw: T6Draw,
    requested: &str,
) -> Option<render_material::SourceTechniqueSelection> {
    use render_material::TechniqueSelectionPolicy as Policy;
    let candidates = match draw {
        T6Draw::Lit => t6_techniques_for_iw4_slot(requested),
        T6Draw::Emissive if requested.starts_with("depth") || requested.starts_with("build") => {
            Vec::new()
        }
        T6Draw::Emissive => vec![T6_TECHNIQUE_EMISSIVE],
    };
    let slot = candidates.into_iter().find(|&slot| {
        set.techniques
            .get(slot)
            .and_then(Option::as_ref)
            .is_some_and(|technique| {
                !technique.passes.is_empty() && technique.passes.len() <= usize::from(u8::MAX)
            })
    })?;
    let source = T6_TECHNIQUE_TYPE_NAMES[slot];
    let without_fog = requested.strip_suffix(" dfog").unwrap_or(requested);
    let policy = if source == requested {
        Policy::Exact
    } else if source == without_fog {
        Policy::DfogCompatibility
    } else if without_fog == "build shadowmap color" && source == "build shadowmap depth" {
        Policy::DepthToColourCompatibility
    } else if draw == T6Draw::Emissive {
        Policy::EmissiveCompatibility
    } else if without_fog.strip_suffix(" shadow") == Some(source) {
        Policy::UnshadowedCompatibility
    } else {
        Policy::LitFallbackCompatibility
    };
    Some(render_material::SourceTechniqueSelection {
        namespace: crate::AssetNamespace::T6,
        slot: u8::try_from(slot).ok()?,
        policy,
    })
}

#[derive(Clone, Debug)]
pub struct T6MaterialState {
    pub entries: [u8; 36],
    pub rows: Vec<[u32; 2]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum T6StateRefusal {
    MissingSourceSelection {
        target_slot: usize,
    },
    MissingEntry {
        target_slot: usize,
        source_slot: u8,
    },
    IncompleteRows {
        target_slot: usize,
        source_slot: u8,
        base: u8,
        passes: usize,
    },
    AdaptedTableCapacity {
        target_slot: usize,
    },
}

pub struct T6StateBindings {
    pub entries: [u8; IW4_TECHNIQUE_TYPE_COUNT],
    pub rows: Vec<[u32; 2]>,
    pub refusals: Vec<T6StateRefusal>,
}

impl T6MaterialState {
    pub fn first_bits(&self, source_slot: usize) -> Option<[u32; 2]> {
        let &base = self.entries.get(source_slot)?;
        (base != u8::MAX)
            .then(|| self.rows.get(usize::from(base)).copied())
            .flatten()
    }

    pub fn bind_techniques(&self, graph: &OwnedTechniqueGraph) -> T6StateBindings {
        use render_material::TechniqueSelectionPolicy;
        let mut entries = [u8::MAX; IW4_TECHNIQUE_TYPE_COUNT];
        let mut rows = self.rows.clone();
        let mut refusals = Vec::new();
        for (target_slot, technique) in graph.slots.iter().enumerate().take(entries.len()) {
            let Some(technique) = technique else { continue };
            let Some(selection) = technique
                .source_selection
                .filter(|selection| selection.namespace == crate::AssetNamespace::T6)
            else {
                refusals.push(T6StateRefusal::MissingSourceSelection { target_slot });
                continue;
            };
            let base = self
                .entries
                .get(usize::from(selection.slot))
                .copied()
                .unwrap_or(u8::MAX);
            if base == u8::MAX {
                refusals.push(T6StateRefusal::MissingEntry {
                    target_slot,
                    source_slot: selection.slot,
                });
                continue;
            }
            let passes = technique.passes.len();
            let end = usize::from(base)
                .checked_add(passes)
                .filter(|&end| passes > 0 && end <= self.rows.len());
            let Some(end) = end else {
                refusals.push(T6StateRefusal::IncompleteRows {
                    target_slot,
                    source_slot: selection.slot,
                    base,
                    passes,
                });
                continue;
            };
            if selection.policy == TechniqueSelectionPolicy::DepthToColourCompatibility {
                let Ok(adapted_base) = u8::try_from(rows.len()) else {
                    refusals.push(T6StateRefusal::AdaptedTableCapacity { target_slot });
                    continue;
                };
                if adapted_base == u8::MAX
                    || rows.len().checked_add(passes).is_none_or(|end| end > 256)
                {
                    refusals.push(T6StateRefusal::AdaptedTableCapacity { target_slot });
                    continue;
                }
                rows.extend(
                    self.rows[usize::from(base)..end]
                        .iter()
                        .map(|&[bits, more]| [bits | GFXS0_COLORWRITE_RGB, more]),
                );
                entries[target_slot] = adapted_base;
            } else {
                entries[target_slot] = base;
            }
        }
        T6StateBindings {
            entries,
            rows,
            refusals,
        }
    }
}

pub const CODE_T6_HDR_CONTROL_0: u16 = 0x300;
pub const CODE_T6_HDR_CONTROL_1: u16 = 0x301;
pub const CODE_T6_SKY_COLOR_MULTIPLIER: u16 = 0x302;
pub const CODE_T6_FOG: [u16; 6] = [0x306, 0x307, 0x308, 0x309, 0x30a, 0x30b];
const T6_FOG_NAMES: [&str; 6] = [
    "fogColor",
    "fogConsts",
    "fogConsts2",
    "sunFogDir",
    "sunFogColor",
    "sunFog",
];
pub use render_material::{CODE_T6_GRID_SH, CODE_T6_REFLECTION_SH, CODE_T6_SAMPLE_DECODE};

enum EngineValue {
    Code(&'static str),
    Literal([f32; 4]),
}

fn engine_value(name: &str) -> EngineValue {
    use EngineValue::{Code, Literal};
    match name {
        "viewProjectionMatrix" => Code("TRANSPOSE_VIEW_PROJECTION_MATRIX"),
        "projectionMatrix" => Code("TRANSPOSE_PROJECTION_MATRIX"),
        "inverseViewMatrix" => Code("INVERSE_TRANSPOSE_VIEW_MATRIX"),
        "inverseViewProjectionMatrix" => Code("INVERSE_TRANSPOSE_VIEW_PROJECTION_MATRIX"),
        "shadowLookupMatrix" => Code("TRANSPOSE_SHADOW_LOOKUP_MATRIX"),
        "worldOutdoorLookupMatrix" => Code("TRANSPOSE_WORLD_OUTDOOR_LOOKUP_MATRIX"),
        "worldMatrix" => Code("TRANSPOSE_WORLD_MATRIX0"),
        "worldViewMatrix" => Code("TRANSPOSE_WORLD_VIEW_MATRIX0"),
        "worldViewProjectionMatrix" => Code("TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX0"),
        "inverseWorldViewMatrix" => Code("INVERSE_TRANSPOSE_WORLD_VIEW_MATRIX0"),
        "inverseTransposeWorldViewMatrix" => Code("INVERSE_WORLD_VIEW_MATRIX0"),
        "inverseTransposeWorldMatrix" => Code("INVERSE_WORLD_MATRIX0"),
        "sunPosition" | "lightPosition" => Code("LIGHT_POSITION"),
        "sunDiffuse" | "lightDiffuse" => Code("LIGHT_DIFFUSE"),
        "lightSpotDir" => Code("LIGHT_SPOTDIR"),
        "lightSpotFactors" => Code("LIGHT_SPOTFACTORS"),
        "shadowmapSwitchPartition" => Code("SHADOWMAP_SWITCH_PARTITION"),
        "sunShadowmapPixelSize" => Code("SUN_SHADOWMAP_PIXEL_ADJUST"),
        "spotShadowmapPixelAdjust" => Code("SPOT_SHADOWMAP_PIXEL_ADJUST"),
        "shadowmapPolygonOffset" => Code("SHADOWMAP_POLYGON_OFFSET"),
        "zNear" => Code("ZNEAR"),
        "materialColor" => Code("MATERIAL_COLOR"),
        "renderTargetSize" => Code("RENDER_TARGET_SIZE"),
        "gameTime" => Code("GAMETIME"),
        "clipSpaceLookupScale" => Code("CLIP_SPACE_LOOKUP_SCALE"),
        "clipSpaceLookupOffset" => Code("CLIP_SPACE_LOOKUP_OFFSET"),
        "colorMatrixR" => Code("COLOR_MATRIX_R"),
        "colorMatrixG" => Code("COLOR_MATRIX_G"),
        "colorMatrixB" => Code("COLOR_MATRIX_B"),
        "outdoorFeatherParms" => Code("OUTDOOR_FEATHER_PARMS"),
        "particleCloudColor" => Code("PARTICLE_CLOUD_COLOR"),
        "hdrControl0" => Code("T6_HDR_CONTROL_0"),
        "hdrControl1" => Code("T6_HDR_CONTROL_1"),
        "skyColorMultiplier" => Code("T6_SKY_COLOR_MULTIPLIER"),
        "fogColor" | "fogConsts" | "fogConsts2" | "sunFogDir" | "sunFogColor" | "sunFog" => {
            Code("T6_FOG")
        }
        "colorTint" | "lightHeroScale" | "occlusionAmount" => Literal([1.0; 4]),
        "heroLightingR" => Literal([1.0, 0.0, 0.0, 0.0]),
        "heroLightingG" => Literal([0.0, 1.0, 0.0, 0.0]),
        "heroLightingB" => Literal([0.0, 0.0, 1.0, 0.0]),
        "weaponParam0" => Literal([0.0, 0.0, 0.0, 1.0]),
        "gridLightingCoordsAndVis" => Code("BASE_LIGHTING_COORDS"),
        "lightingLookupScale" => Code("LIGHTING_LOOKUP_SCALE"),
        "reflectionLightingSH0" => Code("T6_REFLECTION_SH0"),
        "reflectionLightingSH1" => Code("T6_REFLECTION_SH1"),
        "reflectionLightingSH2" => Code("T6_REFLECTION_SH2"),
        "gridLightingSH0" => Code("T6_GRID_SH0"),
        "gridLightingSH1" => Code("T6_GRID_SH1"),
        "gridLightingSH2" => Code("T6_GRID_SH2"),
        _ => Literal([0.0; 4]),
    }
}

fn variable_at(reflection: &Reflection, row: ConstantRow) -> Option<(&str, u32)> {
    let binding = reflection
        .bindings
        .iter()
        .find(|b| b.kind == BindingKind::ConstantBuffer && b.bind_point == row.buffer)?;
    let buffer = reflection
        .constant_buffers
        .iter()
        .find(|buffer| buffer.name == binding.name)?;
    let at = row.row * 16;
    buffer
        .variables
        .iter()
        .find(|v| v.start_offset <= at && at < v.start_offset + v.size.max(1))
        .map(|v| (v.name.as_str(), v.start_offset / 16))
}

fn texture_name(reflection: &Reflection, texture: u32) -> Option<&str> {
    reflection
        .bindings
        .iter()
        .find(|b| b.kind == BindingKind::Texture && b.bind_point == texture)
        .map(|b| b.name.as_str())
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Primitive,
    Object,
    Stable,
}

fn code_tier(index: u16) -> Tier {
    let world = iw4_code_const_index("WORLD_MATRIX0").unwrap_or(u16::MAX);
    let last = iw4_code_const_index("INVERSE_TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX2").unwrap_or(0);
    if (world..=last).contains(&index) {
        Tier::Primitive
    } else {
        Tier::Object
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Vertex,
    Pixel,
}

fn material_arguments_in_row(
    arguments: &[T6Argument],
    kind: u16,
    buffer: u32,
    row: u32,
) -> Vec<&T6Argument> {
    arguments
        .iter()
        .filter(|a| {
            a.kind == kind
                && u32::from(a.buffer) == buffer
                && u32::from(a.offset) / 16 <= row
                && row * 16 < u32::from(a.offset) + u32::from(a.size).max(1)
        })
        .collect()
}

fn packed_row_hash(arguments: &[&T6Argument]) -> Option<u32> {
    const FNV_OFFSET: u32 = 0x811c_9dc5;
    const FNV_PRIME: u32 = 0x0100_0193;
    let packed = arguments.len() > 1 || arguments.iter().any(|a| a.offset % 16 != 0 || a.size < 16);
    packed.then(|| {
        arguments.iter().fold(FNV_OFFSET, |hash, a| {
            [a.def, u32::from(a.offset % 16), u32::from(a.size)]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .fold(hash, |hash, byte| {
                    (hash ^ u32::from(byte)).wrapping_mul(FNV_PRIME)
                })
        })
    })
}

fn constant_argument(
    stage: Stage,
    destination: u16,
    row: ConstantRow,
    reflection: &Reflection,
    arguments: &[T6Argument],
    gaps: &mut BTreeSet<String>,
) -> (Tier, OwnedShaderArgument) {
    use argument_type as t;
    let (material_kind, literal_kind, code_kind) = match stage {
        Stage::Vertex => (
            t::MATERIAL_VERTEX_CONST,
            t::LITERAL_VERTEX_CONST,
            t::CODE_VERTEX_CONST,
        ),
        Stage::Pixel => (
            t::MATERIAL_PIXEL_CONST,
            t::LITERAL_PIXEL_CONST,
            t::CODE_PIXEL_CONST,
        ),
    };
    let covering = arguments.iter().find(|a| {
        [material_kind, literal_kind, code_kind].contains(&a.kind)
            && u32::from(a.buffer) == row.buffer
            && u32::from(a.offset) / 16 <= row.row
            && row.row * 16 < u32::from(a.offset) + u32::from(a.size).max(16)
    });
    match covering {
        Some(a) if a.kind == material_kind => {
            let in_row = material_arguments_in_row(arguments, material_kind, row.buffer, row.row);
            let name_hash = packed_row_hash(&in_row).unwrap_or(a.def);
            let argument = match stage {
                Stage::Vertex => OwnedShaderArgument::MaterialVertexConstant {
                    destination,
                    name_hash,
                },
                Stage::Pixel => OwnedShaderArgument::MaterialPixelConstant {
                    destination,
                    name_hash,
                },
            };
            return (Tier::Stable, argument);
        }
        Some(a) if a.kind == literal_kind => {
            return (
                Tier::Stable,
                literal_argument(stage, destination, a.literal),
            );
        }
        _ => {}
    }
    let Some((name, first)) = variable_at(reflection, row) else {
        gaps.insert(format!("cb{}[{}]", row.buffer, row.row));
        return (
            Tier::Stable,
            literal_argument(stage, destination, Some([0; 4])),
        );
    };
    match engine_value(name) {
        EngineValue::Code(code) => {
            let index = match code {
                "T6_HDR_CONTROL_0" => Some(CODE_T6_HDR_CONTROL_0),
                "T6_HDR_CONTROL_1" => Some(CODE_T6_HDR_CONTROL_1),
                "T6_SKY_COLOR_MULTIPLIER" => Some(CODE_T6_SKY_COLOR_MULTIPLIER),
                "T6_REFLECTION_SH0" => Some(CODE_T6_REFLECTION_SH[0]),
                "T6_REFLECTION_SH1" => Some(CODE_T6_REFLECTION_SH[1]),
                "T6_REFLECTION_SH2" => Some(CODE_T6_REFLECTION_SH[2]),
                "T6_GRID_SH0" => Some(CODE_T6_GRID_SH[0]),
                "T6_GRID_SH1" => Some(CODE_T6_GRID_SH[1]),
                "T6_GRID_SH2" => Some(CODE_T6_GRID_SH[2]),
                "T6_FOG" => T6_FOG_NAMES
                    .iter()
                    .position(|fog| *fog == name)
                    .map(|i| CODE_T6_FOG[i]),
                _ => iw4_code_const_index(code),
            };
            let Some(index) = index else {
                gaps.insert(format!("{name}: no IW4 {code}"));
                return (
                    Tier::Stable,
                    literal_argument(stage, destination, Some([0; 4])),
                );
            };
            let first_row = (row.row - first) as u8;
            let argument = match stage {
                Stage::Vertex => OwnedShaderArgument::CodeVertexConstant {
                    destination,
                    index,
                    first_row,
                    row_count: 1,
                },
                Stage::Pixel => OwnedShaderArgument::CodePixelConstant {
                    destination,
                    index,
                    first_row,
                    row_count: 1,
                },
            };
            (code_tier(index), argument)
        }
        EngineValue::Literal(value) => (
            Tier::Stable,
            literal_argument(stage, destination, Some(value.map(f32::to_bits))),
        ),
    }
}

fn literal_argument(
    stage: Stage,
    destination: u16,
    words: Option<[u32; 4]>,
) -> OwnedShaderArgument {
    match stage {
        Stage::Vertex => OwnedShaderArgument::LiteralVertexConstant { destination, words },
        Stage::Pixel => OwnedShaderArgument::LiteralPixelConstant { destination, words },
    }
}

fn sampler_argument(
    destination: u16,
    slot: TextureSlot,
    reflection: &Reflection,
    arguments: &[T6Argument],
    gaps: &mut BTreeSet<String>,
) -> Option<(Tier, OwnedShaderArgument)> {
    use argument_type as t;
    let bound = arguments.iter().find(|a| {
        (a.kind == t::MATERIAL_PIXEL_SAMPLER || a.kind == t::CODE_PIXEL_SAMPLER)
            && u32::from(a.offset & 0xff) == slot.texture
    });
    if let Some(a) = bound.filter(|a| a.kind == t::MATERIAL_PIXEL_SAMPLER) {
        return Some((
            Tier::Stable,
            OwnedShaderArgument::MaterialPixelSampler {
                destination,
                name_hash: a.def,
            },
        ));
    }
    let name = texture_name(reflection, slot.texture).unwrap_or("");
    let lower = name.to_ascii_lowercase();
    let code = if lower.contains("reflectionprobe") {
        return None;
    } else if lower.contains("lightmapsampler") {
        return None;
    } else if lower.contains("modellighting") {
        "MODEL_LIGHTING"
    } else if lower.contains("shadowmap") && lower.contains("sun") {
        "SHADOWMAP_SUN"
    } else if lower.contains("shadowmap") {
        "SHADOWMAP_SPOT"
    } else if lower.contains("attenuation") {
        "LIGHT_ATTENUATION"
    } else if lower.contains("outdoor") {
        "OUTDOOR"
    } else if lower.contains("floatz") {
        return Some((
            Tier::Stable,
            OwnedShaderArgument::MaterialPixelSampler {
                destination,
                name_hash: FLOAT_Z_HASH,
            },
        ));
    } else {
        gaps.insert(format!("t{} {name}", slot.texture));
        if slot.dimension == TextureDimension::D2 {
            "WHITE"
        } else {
            "BLACK"
        }
    };
    let index = iw4_code_texture_index(code)?;
    Some((
        Tier::Object,
        OwnedShaderArgument::CodePixelSampler { destination, index },
    ))
}

fn reads_float_z(pass: &T6Pass) -> bool {
    let Ok(pixel) = Shader::parse(&pass.pixel) else {
        return false;
    };
    pixel.texture_slots().is_ok_and(|slots| {
        slots.iter().any(|slot| {
            texture_name(&pixel.reflection, slot.texture)
                .is_some_and(|name| name.to_ascii_lowercase().contains("floatz"))
        })
    })
}

pub const T6_LAYER_WORLD_VERT_FORMAT: u8 = 8;

fn iw4_layer_source(t6: u8) -> Option<u8> {
    match t6 {
        6..=9 => Some(t6 - 1),
        _ => None,
    }
}

fn t6_vertex_decl(layer_routing: &[[u8; 2]]) -> AuthoredVertexDecl {
    let mut routing = [[0u8; 2]; asset_iw4::vertex_decl::ROUTING_COUNT];
    let base = [
        [0, 0],
        [1, 2],
        [2, 5],
        [3, 1],
        [4, 7],
        [crate::T6_VERTEX_LIGHTING_SOURCE, 6],
    ];
    let layer = layer_routing
        .iter()
        .filter_map(|&[source, dest]| Some([iw4_layer_source(source)?, dest]));
    let mut stream_count = 0;
    for (slot, pair) in routing.iter_mut().zip(base.into_iter().chain(layer)) {
        *slot = pair;
        stream_count += 1;
    }
    AuthoredVertexDecl {
        family: crate::VertexLayoutFamily::T6,
        name: AssetRef::Real(if layer_routing.is_empty() {
            "$t6_packed_vertex".to_owned()
        } else {
            format!("$t6_packed_vertex{layer_routing:?}")
        }),
        stream_count,
        has_optional_source: 0,
        routing,
    }
}

impl MaterialCatalog {
    fn t6_pass(
        &mut self,
        pass: &T6Pass,
        pass_index: u8,
        vertex_decl: usize,
        gaps: &mut BTreeSet<String>,
        namespace: crate::AssetNamespace,
    ) -> Result<OwnedMaterialPass, String> {
        let vertex =
            Shader::parse(&pass.vertex).map_err(|e| format!("{}: {e}", pass.vertex_name))?;
        let pixel = Shader::parse(&pass.pixel).map_err(|e| format!("{}: {e}", pass.pixel_name))?;
        let rows = |shader: &Shader| -> Result<Vec<ConstantRow>, String> {
            Ok(shader
                .constant_rows()
                .map_err(|e| e.to_string())?
                .into_iter()
                .collect())
        };
        let mut arguments = Vec::new();
        for (stage, shader) in [(Stage::Vertex, &vertex), (Stage::Pixel, &pixel)] {
            for (destination, row) in (0u16..).zip(rows(shader)?) {
                arguments.push(constant_argument(
                    stage,
                    destination,
                    row,
                    &shader.reflection,
                    &pass.arguments,
                    gaps,
                ));
            }
        }
        let samples_decoded_texture = pixel
            .texture_slots()
            .map_err(|e| e.to_string())?
            .iter()
            .any(|slot| {
                texture_name(&pixel.reflection, slot.texture).is_some_and(|name| {
                    let name = name.to_ascii_lowercase();
                    name.contains("modellighting") || name.contains("reflectionprobe")
                })
            });
        if samples_decoded_texture {
            arguments.push((
                Tier::Object,
                OwnedShaderArgument::CodePixelConstant {
                    destination: rows(&pixel)?.len() as u16,
                    index: CODE_T6_SAMPLE_DECODE,
                    first_row: 0,
                    row_count: 1,
                },
            ));
        }
        let mut slots = vertex.texture_slots().map_err(|e| e.to_string())?;
        slots.extend(pixel.texture_slots().map_err(|e| e.to_string())?);
        for (destination, slot) in (0u16..).zip(slots) {
            arguments.extend(sampler_argument(
                destination,
                slot,
                &pixel.reflection,
                &pass.arguments,
                gaps,
            ));
        }
        arguments.sort_by_key(|(tier, _)| *tier);
        let count = |tier| arguments.iter().filter(|(t, _)| *t == tier).count() as u8;
        let (per_prim, per_obj, stable) = (
            count(Tier::Primitive),
            count(Tier::Object),
            count(Tier::Stable),
        );
        let shader_ref =
            |this: &mut Self, name: &str, kind, program: &[u8], tag: u32| OwnedShaderRef {
                pointer_identity: AssetPointerIdentity {
                    block: u8::MAX,
                    offset: tag,
                },
                shader: Some(this.link_shader(AuthoredShader {
                    namespace,
                    name: AssetRef::Real(name.to_owned()),
                    kind,
                    program: program.to_vec(),
                })),
            };
        Ok(OwnedMaterialPass {
            pass_index,
            vertex_decl_identity: AssetPointerIdentity {
                block: u8::MAX,
                offset: vertex_decl as u32,
            },
            vertex_decl: Some(vertex_decl),
            vertex_shader: shader_ref(
                self,
                &pass.vertex_name,
                AssetType::VertexShader,
                &pass.vertex,
                0,
            ),
            pixel_shader: shader_ref(
                self,
                &pass.pixel_name,
                AssetType::PixelShader,
                &pass.pixel,
                1,
            ),
            per_prim_arg_count: per_prim,
            per_obj_arg_count: per_obj,
            stable_arg_count: stable,
            custom_sampler_flags: 0,
            t5_custom_sampler_flags: pass.custom_sampler_flags,
            arguments: arguments
                .into_iter()
                .map(|(_, argument)| argument)
                .collect(),
            arguments_truncated: false,
        })
    }

    pub fn link_t6_technique_set(
        &mut self,
        set: &T6TechniqueSet,
        draw: T6Draw,
        report: &mut Vec<String>,
    ) -> usize {
        let namespace = crate::AssetNamespace::T6;
        let layered = set.world_vert_format != 0;
        let mut gaps = BTreeSet::new();
        let mut slots = vec![None; IW4_TECHNIQUE_TYPE_COUNT];
        let mut table = TechniqueTable::default();
        for (iw4_slot, iw4_name) in IW4_TECHNIQUE_TYPE_NAMES.iter().enumerate() {
            let Some(selection) = select_t6_technique(set, draw, iw4_name) else {
                continue;
            };
            let technique = set.techniques[usize::from(selection.slot)]
                .as_ref()
                .unwrap();
            let passes: Result<Vec<_>, String> = (0u8..)
                .zip(&technique.passes)
                .map(|(index, pass)| {
                    let layer_routing = if layered {
                        &pass.layer_routing[..]
                    } else {
                        &[]
                    };
                    let vertex_decl = self.link_vertex_decl(t6_vertex_decl(layer_routing));
                    self.t6_pass(pass, index, vertex_decl, &mut gaps, namespace)
                })
                .collect();
            match passes {
                Ok(passes) => {
                    table.slots |= 1 << iw4_slot;
                    table.scanned |= 1 << iw4_slot;
                    table.pass_count_by_slot[iw4_slot] = passes.len() as u8;
                    table.max_pass_count = table.max_pass_count.max(passes.len() as u16);
                    slots[iw4_slot] = Some(OwnedTechnique {
                        source_selection: Some(selection),
                        flags: if layered {
                            asset_iw4::vertex_decl::TECHNIQUE_FLAG_VERTEX_TYPE_FROM_SURFACE
                        } else {
                            0
                        },
                        passes,
                        body_scanned: true,
                    });
                }
                Err(error) => report.push(format!("t6 techset {} {iw4_name}: {error}", set.name)),
            }
        }
        report.push(format!(
            "t6 techset {}: T6 techniques {:?}, IW4 slots filled {}",
            set.name,
            set.techniques
                .iter()
                .enumerate()
                .filter(|(_, t)| t.is_some())
                .map(|(i, _)| T6_TECHNIQUE_TYPE_NAMES.get(i).copied().unwrap_or("?"))
                .collect::<Vec<_>>(),
            table.slots.count_ones()
        ));
        if !gaps.is_empty() {
            report.push(format!(
                "t6 techset {}: no IW4 source for {:?}",
                set.name, gaps
            ));
        }
        table.model_lighting_const =
            Some(crate::material_catalog::graph_slots_bind_model_lighting_const(&slots));
        table.graph = Some(OwnedTechniqueGraph {
            slots,
            rows_truncated: 0,
            arguments_truncated: 0,
        });
        self.link_techset(TechniqueSetFacts {
            namespace,
            name: AssetRef::Real(draw.technique_set_name(&set.name)),
            table: Some(table),
            world_vert_format: if layered {
                T6_LAYER_WORLD_VERT_FORMAT
            } else {
                0
            },
            zone: Default::default(),
            t5_occupancy: None,
            iw5_fallback_table: None,
            t5_fallback_table: None,
        })
    }
}

const COLOR_MAP_HASHES: [u32; 3] = [0xa0ab_1041, 0xf039_ec2d, 0xb607_c0fe];
const NORMAL_MAP_HASH: u32 = 0x59d3_0d0f;
const ARM_NORMAL_MAP_HASH: u32 = 0x942c_bff0;
const SPECULAR_MAP_HASH: u32 = 0x34ec_ccb3;
const FLOAT_Z_HASH: u32 = 0x666c_745a;
const ARM_SPECULAR_MAP_HASH: u32 = 0x8c29_7e80;

fn sampler_aliases(hash: u32) -> &'static [u32] {
    const NORMAL: [u32; 2] = [NORMAL_MAP_HASH, ARM_NORMAL_MAP_HASH];
    const SPECULAR: [u32; 2] = [SPECULAR_MAP_HASH, ARM_SPECULAR_MAP_HASH];
    [&COLOR_MAP_HASHES[..], &NORMAL[..], &SPECULAR[..]]
        .into_iter()
        .find(|group| group.contains(&hash))
        .unwrap_or(&[])
}

const OCCLUSION_AMOUNT_HASH: u32 = 0x9027_e5c1;
const T6_SPECULAR_SCALE: f32 = 0.5;

#[derive(Clone, Debug)]
pub struct T6Texture {
    pub name_hash: u32,
    pub sampler_state: u8,
    pub semantic: u8,
    pub image: String,
    pub texels: std::sync::Arc<bevy::prelude::Image>,
}
