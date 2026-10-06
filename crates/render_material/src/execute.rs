use std::sync::Arc;

use asset_core::AssetNamespace;
use dpvs_iw4::GfxDrawSurf;

use crate::catalog::{fnv1a64, fnv1a64_more};
use crate::prepared::{
    PackedCodeArg, PackedLocalBanks, PackedLocalSamplers, PreparedArgBelts, PreparedMaterialTable,
    PreparedTechnique,
};
use crate::{
    CatalogBuildError, CodeSourceLookup, CompiledPassState, MaterialAssetId, MaterialGenerationId,
    PortId, RemapResolution, RuntimeImageId, RuntimeMaterial, RuntimeMaterialCatalog,
    RuntimeShaderPair, RuntimeShaderStage, RuntimeSortedMaterialTable, RuntimeTechnique,
    RuntimeTechniqueSetId, SetupArm, TechType, UnsupportedStateFields,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutablePass {
    pub pass_index: u8,
    pub port: PortId,
    pub shader_pair: RuntimeShaderPair,
    pub state: CompiledPassState,
    pub local_banks: Option<Arc<PackedLocalBanks>>,
    pub local_samplers: Option<Arc<PackedLocalSamplers>>,
    pub code_constants: PackedCodeConstants,
    pub code_samplers: PackedCodeSamplers,
}

#[derive(Clone, Copy, Debug)]
pub struct ExecutablePassView<'a> {
    pub pass_index: u8,
    pub port: PortId,
    pub shader_pair: RuntimeShaderPair,
    pub state: CompiledPassState,
    pub local_banks: Option<&'a Arc<PackedLocalBanks>>,
    pub local_samplers: Option<&'a Arc<PackedLocalSamplers>>,
    pub code_constants: &'a [PackedCodeConstantLane],
    pub code_samplers: &'a [PackedCodeSamplerLane],

    pub code_constant_id: u64,

    pub code_sampler_id: u64,
}

impl ExecutablePass {
    pub fn view(&self) -> ExecutablePassView<'_> {
        ExecutablePassView {
            pass_index: self.pass_index,
            port: self.port,
            shader_pair: self.shader_pair,
            state: self.state,
            local_banks: self.local_banks.as_ref(),
            local_samplers: self.local_samplers.as_ref(),
            code_constants: &self.code_constants.lanes,
            code_samplers: &self.code_samplers.lanes,
            code_constant_id: self.code_constants.id,
            code_sampler_id: self.code_samplers.id,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MaterialExecution {
    pub tech_type: TechType,
    pub arm: SetupArm,
    pub vertex_type: u8,
    shell: Arc<StableMaterialShell>,
    code_constants: Vec<PackedCodeConstants>,
    code_samplers: Vec<PackedCodeSamplers>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PackedCodeSamplerLane {
    pub register: u16,
    pub index: u32,
    pub sampler_state: u8,
    pub image: Option<RuntimeImageId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PackedCodeSamplers {
    pub id: u64,
    pub lanes: Vec<PackedCodeSamplerLane>,
}

impl Default for PackedCodeSamplers {
    fn default() -> Self {
        Self::from_lanes(Vec::new())
    }
}

impl PackedCodeSamplers {
    pub fn from_lanes(lanes: Vec<PackedCodeSamplerLane>) -> Self {
        Self {
            id: hash_packed_code_lanes(&lanes),
            lanes,
        }
    }

    pub fn texture(&self, register: u16, index: u32) -> Option<PackedCodeSamplerLane> {
        self.lanes
            .iter()
            .copied()
            .find(|lane| lane.register == register && lane.index == index)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackedCodeConstantLane {
    pub stage: RuntimeShaderStage,
    pub destination: u16,
    pub index: u16,
    pub first_row: u8,
    pub row_count: u8,
    pub rows: Arc<[[u32; 4]]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackedCodeConstants {
    pub id: u64,
    pub lanes: Vec<PackedCodeConstantLane>,
}

impl Default for PackedCodeConstants {
    fn default() -> Self {
        Self::from_lanes(Vec::new())
    }
}

impl PackedCodeConstants {
    pub fn from_lanes(lanes: Vec<PackedCodeConstantLane>) -> Self {
        Self {
            id: Self::hash_lanes(&lanes),
            lanes,
        }
    }

    pub fn hash_lanes(lanes: &[PackedCodeConstantLane]) -> u64 {
        hash_packed_code_constants(lanes)
    }

    fn refresh_id(&mut self) {
        self.id = Self::hash_lanes(&self.lanes);
    }
}

fn hash_packed_code_constants(lanes: &[PackedCodeConstantLane]) -> u64 {
    // The id keys packed constant caches with no second comparison: keep a
    // full-strength mix.
    let mut hash = fold_mix(FOLD_SEED, lanes.len() as u64);
    for lane in lanes {
        let stage = match lane.stage {
            RuntimeShaderStage::Vertex => 0u64,
            RuntimeShaderStage::Pixel => 1u64,
        };
        let header = stage
            | u64::from(lane.destination) << 8
            | u64::from(lane.index) << 24
            | u64::from(lane.first_row) << 40
            | u64::from(lane.row_count) << 48;
        hash = fold_mix(hash, header);
        for row in lane.rows.iter() {
            let low = u64::from(row[0]) | u64::from(row[1]) << 32;
            let high = u64::from(row[2]) | u64::from(row[3]) << 32;
            hash = fold_mix(fold_mix(hash, low), high);
        }
    }
    fold_mix(hash, FOLD_SEED)
}

const FOLD_SEED: u64 = 0x243f_6a88_85a3_08d3;
const FOLD_MULTIPLIER: u64 = 0x9e37_79b9_7f4a_7c15;

fn fold_mix(hash: u64, word: u64) -> u64 {
    let product = u128::from(hash ^ word) * u128::from(FOLD_MULTIPLIER);
    (product as u64) ^ ((product >> 64) as u64)
}

fn hash_packed_code_lanes(lanes: &[PackedCodeSamplerLane]) -> u64 {
    let mut hash = fnv1a64(&(lanes.len() as u64).to_le_bytes());
    for lane in lanes {
        hash = fnv1a64_more(hash, &lane.register.to_le_bytes());
        hash = fnv1a64_more(hash, &lane.index.to_le_bytes());
        hash = fnv1a64_more(hash, &[lane.sampler_state]);
        match lane.image {
            Some(image) => {
                hash = fnv1a64_more(hash, &[1]);
                hash = fnv1a64_more(hash, &image.0.to_le_bytes());
            }
            None => {
                hash = fnv1a64_more(hash, &[0]);
            }
        }
    }
    hash
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterialRefusal {
    SortedMaterialTableMissing,
    SortedMaterialOrdinalOutOfRange {
        ordinal: u32,
        table_len: usize,
    },
    SortedMaterialBuildFailed {
        cause: CatalogBuildError,
    },
    StaleMaterialGeneration {
        retained: MaterialGenerationId,
        current: MaterialGenerationId,
    },
    MissingLightBindings {
        scene_light: u8,
    },
    MaterialOutOfRange {
        material: MaterialAssetId,
    },
    LocalTechniqueSetOutOfRange {
        set: RuntimeTechniqueSetId,
    },
    RemappedTechniqueSetOutOfRange {
        set: RuntimeTechniqueSetId,
    },
    RemapMissing {
        pointer_identity: u32,
    },
    RemapCycle {
        first_set: RuntimeTechniqueSetId,
    },
    TechniqueAbsent {
        tech_type: TechType,
    },
    EmptyTechnique {
        tech_type: TechType,
    },
    StateEntriesMissing,
    StateEntryOutOfRange {
        tech_type: TechType,
    },
    StateRowOutOfRange {
        row: usize,
        pass_index: u8,
    },
    UnsupportedState {
        pass_index: u8,
        fields: UnsupportedStateFields,
        word0: u32,
        word1: u32,
    },
    PassIndexOverflow {
        pass_index: usize,
    },
    ShaderProgramMissing {
        pass_index: u8,
    },
    UnsupportedShaderPair {
        pass_index: u8,
        pair: RuntimeShaderPair,
    },
    MissingTexture {
        pass_index: u8,
        name_hash: u32,
    },
    MissingMaterialConstant {
        pass_index: u8,
        name_hash: u32,
    },
    MissingCodeConstant {
        pass_index: u8,
        stage: RuntimeShaderStage,
        index: u16,
    },
    CodeConstantRowsOutOfRange {
        pass_index: u8,
        stage: RuntimeShaderStage,
        index: u16,
        first_row: u8,
        row_count: u8,
        available_rows: usize,
    },
    MissingCodeTexture {
        pass_index: u8,
        index: u32,
    },
    MissingLiteralConstant {
        pass_index: u8,
    },
    UnknownArgumentType {
        pass_index: u8,
        argument_type: u16,
    },
    PassArgCountMismatch {
        pass_index: u8,
        per_prim: u8,
        per_obj: u8,
        stable: u8,
        arguments: usize,
    },
    TechniqueSetNamespaceMismatch {
        want: AssetNamespace,
        got: AssetNamespace,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MaterialDrawKey {
    pub material_id: Option<MaterialAssetId>,
    pub packed: u64,
    pub rank: u32,
}

impl MaterialDrawKey {
    pub const fn new(packed: u64, rank: u32) -> Self {
        Self {
            packed,
            rank,
            material_id: None,
        }
    }

    pub const fn with_material_id(mut self, material_id: Option<MaterialAssetId>) -> Self {
        self.material_id = material_id;
        self
    }

    pub const fn draw_surf(self) -> GfxDrawSurf {
        GfxDrawSurf {
            packed: self.packed,
        }
    }
}

pub fn resolve_sorted_material(
    catalog: &RuntimeMaterialCatalog,
    key: MaterialDrawKey,
) -> Result<&RuntimeMaterial, MaterialRefusal> {
    if let Some(material_id) = key.material_id {
        return catalog
            .parts()
            .materials
            .get(usize::from(material_id.0))
            .filter(|material| material.asset_id == material_id)
            .ok_or(MaterialRefusal::MaterialOutOfRange {
                material: material_id,
            });
    }
    let ordinal = key.rank;
    let material_id = match &catalog.parts().sorted_materials {
        RuntimeSortedMaterialTable::Missing => {
            return Err(MaterialRefusal::SortedMaterialTableMissing);
        }
        RuntimeSortedMaterialTable::BuildFailed(cause) => {
            return Err(MaterialRefusal::SortedMaterialBuildFailed {
                cause: cause.clone(),
            });
        }
        RuntimeSortedMaterialTable::Ready {
            asset_ids_by_ordinal,
            ..
        } => *asset_ids_by_ordinal.get(ordinal as usize).ok_or(
            MaterialRefusal::SortedMaterialOrdinalOutOfRange {
                ordinal,
                table_len: asset_ids_by_ordinal.len(),
            },
        )?,
    };
    catalog
        .parts()
        .materials
        .get(usize::from(material_id.0))
        .ok_or(MaterialRefusal::MaterialOutOfRange {
            material: material_id,
        })
}

pub fn add_surf_has_technique(
    catalog: &RuntimeMaterialCatalog,
    key: MaterialDrawKey,
    tech_type: TechType,
) -> bool {
    let Ok(material) = resolve_sorted_material(catalog, key) else {
        return true;
    };
    let set_id = match material.remap {
        RemapResolution::SelfSet => material.local_technique_set,
        RemapResolution::Resolved(set) => set,
        RemapResolution::Missing { .. } | RemapResolution::Cycle { .. } => return true,
    };
    let Some(set) = catalog.parts().technique_sets.get(set_id.0 as usize) else {
        return true;
    };
    set.technique(tech_type).is_some()
}

pub fn resolve_material_technique(
    catalog: &RuntimeMaterialCatalog,
    key: MaterialDrawKey,
    tech_type: TechType,
) -> Result<(&RuntimeMaterial, &RuntimeTechnique), MaterialRefusal> {
    let material = resolve_sorted_material(catalog, key)?;

    let set_id = match material.remap {
        RemapResolution::SelfSet => material.local_technique_set,
        RemapResolution::Resolved(set) => set,
        RemapResolution::Missing { pointer_identity } => {
            return Err(MaterialRefusal::RemapMissing { pointer_identity });
        }
        RemapResolution::Cycle { first_set } => {
            return Err(MaterialRefusal::RemapCycle { first_set });
        }
    };
    let set = catalog
        .parts()
        .technique_sets
        .get(set_id.0 as usize)
        .ok_or_else(|| {
            if matches!(material.remap, RemapResolution::SelfSet) {
                MaterialRefusal::LocalTechniqueSetOutOfRange { set: set_id }
            } else {
                MaterialRefusal::RemappedTechniqueSetOutOfRange { set: set_id }
            }
        })?;
    if set.namespace != material.namespace {
        return Err(MaterialRefusal::TechniqueSetNamespaceMismatch {
            want: material.namespace,
            got: set.namespace,
        });
    }
    let technique = set
        .technique(tech_type)
        .ok_or(MaterialRefusal::TechniqueAbsent { tech_type })?;
    if technique.passes.is_empty() {
        return Err(MaterialRefusal::EmptyTechnique { tech_type });
    }
    Ok((material, technique))
}

pub fn world_tess_vertex_type(
    catalog: &RuntimeMaterialCatalog,
    key: MaterialDrawKey,
    tech_type: TechType,
) -> u8 {
    match resolve_material_technique(catalog, key, tech_type) {
        Ok((material, technique)) => {
            let authored = world_vert_decl_from_resolved(catalog, material, technique);
            if authored == asset_iw4::vertex_decl::WORLD_VERTEX_TYPE
                && technique_routes_optional_source(catalog, technique)
            {
                asset_iw4::vertex_decl::WORLD_VERTEX_TYPE.saturating_add(1)
            } else {
                authored
            }
        }
        Err(_) => asset_iw4::vertex_decl::WORLD_VERTEX_TYPE,
    }
}

pub fn world_tess_vertex_type_authored(
    catalog: &RuntimeMaterialCatalog,
    key: MaterialDrawKey,
    tech_type: TechType,
) -> u8 {
    match resolve_material_technique(catalog, key, tech_type) {
        Ok((material, technique)) => world_vert_decl_from_resolved(catalog, material, technique),
        Err(_) => asset_iw4::vertex_decl::WORLD_VERTEX_TYPE,
    }
}

fn world_vert_decl_from_resolved(
    catalog: &RuntimeMaterialCatalog,
    material: &RuntimeMaterial,
    technique: &RuntimeTechnique,
) -> u8 {
    let set_id = match material.remap {
        RemapResolution::SelfSet => material.local_technique_set,
        RemapResolution::Resolved(set) => set,
        RemapResolution::Missing { .. } | RemapResolution::Cycle { .. } => {
            return asset_iw4::vertex_decl::WORLD_VERTEX_TYPE;
        }
    };
    catalog
        .parts()
        .technique_sets
        .get(set_id.0 as usize)
        .map(|set| {
            asset_iw4::vertex_decl::world_vert_decl_type(technique.flags, set.world_vert_format)
        })
        .unwrap_or(asset_iw4::vertex_decl::WORLD_VERTEX_TYPE)
}

fn technique_routes_optional_source(
    catalog: &RuntimeMaterialCatalog,
    technique: &RuntimeTechnique,
) -> bool {
    let Some(pass) = technique.passes.first() else {
        return false;
    };
    let Some(pair) = pass.shader_pair else {
        return false;
    };
    let Some(decl) = catalog.vertex_decl(pair.vertex_decl_slot) else {
        return false;
    };
    decl.routed().iter().any(|route| route[0] >= 5)
}

pub fn smodel_tess_vertex_type(stream: Option<lighting_iw4::SmodelSurfPath>) -> u8 {
    match stream {
        Some(lighting_iw4::SmodelSurfPath::Cached | lighting_iw4::SmodelSurfPath::Pretess) => {
            asset_iw4::vertex_decl::STATICMODELCACHE_VERTEX_TYPE
        }
        _ => asset_iw4::vertex_decl::PACKED_VERTEX_TYPE,
    }
}

fn missing_prepared_technique(
    material: &RuntimeMaterial,
    technique: &RuntimeTechnique,
    tech_type: TechType,
) -> MaterialRefusal {
    let Some(entries) = material.state_bits_entry.as_ref() else {
        return MaterialRefusal::StateEntriesMissing;
    };
    match entries.get(usize::from(tech_type.0)).copied() {
        None => MaterialRefusal::StateEntryOutOfRange { tech_type },
        Some(0xff) => MaterialRefusal::TechniqueAbsent { tech_type },
        Some(base) => {
            for pass_index in 0..technique.passes.len() {
                let row = usize::from(base).saturating_add(pass_index);
                let Ok(pass_index) = u8::try_from(pass_index) else {
                    return MaterialRefusal::PassIndexOverflow { pass_index };
                };
                if material.pass_states.get(row).is_none() {
                    return MaterialRefusal::StateRowOutOfRange { row, pass_index };
                }
            }
            match technique.passes.first().and_then(|pass| pass.shader_pair) {
                Some(pair) => MaterialRefusal::UnsupportedShaderPair {
                    pass_index: 0,
                    pair,
                },
                None => MaterialRefusal::ShaderProgramMissing { pass_index: 0 },
            }
        }
    }
}

fn resolve_code_descriptors_into(
    code_sources: &impl CodeSourceLookup,
    pass_index: u8,
    descriptors: impl IntoIterator<Item = PackedCodeArg>,
    constants: &mut PackedCodeConstants,
    samplers: &mut PackedCodeSamplers,
) -> Result<(), MaterialRefusal> {
    constants.lanes.clear();
    samplers.lanes.clear();
    for argument in descriptors {
        match argument {
            PackedCodeArg::Constant {
                stage,
                destination,
                index,
                first_row,
                row_count,
            } => constants.lanes.push(resolve_code_constant(
                code_sources,
                pass_index,
                stage,
                destination,
                index,
                first_row,
                row_count,
            )?),
            PackedCodeArg::Sampler { destination, index } => {
                samplers.lanes.push(resolve_code_sampler(
                    code_sources,
                    pass_index,
                    destination,
                    index,
                )?);
            }
            PackedCodeArg::Unknown { argument_type } => {
                return Err(MaterialRefusal::UnknownArgumentType {
                    pass_index,
                    argument_type,
                });
            }
        }
    }
    samplers.id = hash_packed_code_lanes(&samplers.lanes);
    constants.refresh_id();
    Ok(())
}

fn resolve_code_constant(
    code_sources: &impl CodeSourceLookup,
    pass_index: u8,
    stage: RuntimeShaderStage,
    destination: u16,
    index: u16,
    first_row: u8,
    row_count: u8,
) -> Result<PackedCodeConstantLane, MaterialRefusal> {
    let source = CodeSourceLookup::constant_arc(code_sources, index).ok_or(
        MaterialRefusal::MissingCodeConstant {
            pass_index,
            stage,
            index,
        },
    )?;
    let start = usize::from(first_row);
    let end = start.saturating_add(usize::from(row_count));
    if source.get(start..end).is_none() {
        return Err(MaterialRefusal::CodeConstantRowsOutOfRange {
            pass_index,
            stage,
            index,
            first_row,
            row_count,
            available_rows: source.len(),
        });
    }
    Ok(PackedCodeConstantLane {
        stage,
        destination,
        index,
        first_row,
        row_count,
        rows: source,
    })
}

fn resolve_code_sampler(
    code_sources: &impl CodeSourceLookup,
    pass_index: u8,
    destination: u16,
    index: u32,
) -> Result<PackedCodeSamplerLane, MaterialRefusal> {
    let sampler_state = code_sources
        .texture(index)
        .ok_or(MaterialRefusal::MissingCodeTexture { pass_index, index })?;
    Ok(PackedCodeSamplerLane {
        register: destination,
        index,
        sampler_state,
        image: code_sources.texture_image(index),
    })
}

fn resolve_pass_code_into(
    belts: &PreparedArgBelts,
    code_sources: &impl CodeSourceLookup,
    pass_index: u8,
    constants: &mut PackedCodeConstants,
    samplers: &mut PackedCodeSamplers,
) -> Result<(), MaterialRefusal> {
    resolve_code_descriptors_into(
        code_sources,
        pass_index,
        belts
            .stable
            .code
            .iter()
            .copied()
            .chain(belts.per_prim.code.iter().copied())
            .chain(belts.per_obj.code.iter().copied()),
        constants,
        samplers,
    )
}

fn resolve_pass_code(
    belts: &PreparedArgBelts,
    code_sources: &impl CodeSourceLookup,
    pass_index: u8,
) -> Result<(PackedCodeConstants, PackedCodeSamplers), MaterialRefusal> {
    let mut constants = PackedCodeConstants::default();
    let mut samplers = PackedCodeSamplers::default();
    resolve_pass_code_into(
        belts,
        code_sources,
        pass_index,
        &mut constants,
        &mut samplers,
    )?;
    Ok((constants, samplers))
}

fn validate_prepared_generation(
    catalog: &RuntimeMaterialCatalog,
    prepared: &PreparedMaterialTable,
) -> Result<(), MaterialRefusal> {
    if prepared.generation_id() != catalog.generation_id() {
        return Err(MaterialRefusal::StaleMaterialGeneration {
            retained: prepared.generation_id(),
            current: catalog.generation_id(),
        });
    }
    Ok(())
}

pub fn prepared_draw_technique<'a>(
    catalog: &'a RuntimeMaterialCatalog,
    prepared: &'a PreparedMaterialTable,
    key: MaterialDrawKey,
    tech_type: TechType,
) -> Result<(&'a RuntimeMaterial, &'a PreparedTechnique), MaterialRefusal> {
    validate_prepared_generation(catalog, prepared)?;
    let (material, technique) = resolve_material_technique(catalog, key, tech_type)?;
    let prepared_tech = prepared
        .technique(material.asset_id, tech_type)
        .ok_or_else(|| missing_prepared_technique(material, technique, tech_type))?;
    Ok((material, prepared_tech))
}

pub fn draw_code_sampler_mask(
    catalog: &RuntimeMaterialCatalog,
    prepared: &PreparedMaterialTable,
    key: MaterialDrawKey,
    tech_type: TechType,
) -> Result<u64, MaterialRefusal> {
    prepared_draw_technique(catalog, prepared, key, tech_type)
        .map(|(_, prepared_tech)| prepared_tech.code_sampler_mask)
}

pub fn draw_binds_code_texture(
    catalog: &RuntimeMaterialCatalog,
    prepared: &PreparedMaterialTable,
    key: MaterialDrawKey,
    tech_type: TechType,
    index: u32,
) -> Result<bool, MaterialRefusal> {
    let (_, prepared_tech) = prepared_draw_technique(catalog, prepared, key, tech_type)?;
    Ok(prepared_tech.binds_code_texture(index))
}

pub fn execute_material(
    catalog: &RuntimeMaterialCatalog,
    prepared: &PreparedMaterialTable,
    code_sources: &impl CodeSourceLookup,
    key: MaterialDrawKey,
    tech_type: TechType,
    vertex_type: u8,
) -> Result<MaterialExecution, MaterialRefusal> {
    execute_material_with_shell(catalog, prepared, code_sources, key, tech_type, vertex_type)
}

pub fn execute_material_with_shell(
    catalog: &RuntimeMaterialCatalog,
    prepared: &PreparedMaterialTable,
    code_sources: &impl CodeSourceLookup,
    key: MaterialDrawKey,
    tech_type: TechType,
    vertex_type: u8,
) -> Result<MaterialExecution, MaterialRefusal> {
    validate_prepared_generation(catalog, prepared)?;
    let (material, technique) = resolve_material_technique(catalog, key, tech_type)?;
    let prepared_tech = prepared
        .technique(material.asset_id, tech_type)
        .ok_or_else(|| missing_prepared_technique(material, technique, tech_type))?;
    let mut shell_passes = Vec::with_capacity(technique.passes.len());
    let mut code_constants = Vec::with_capacity(technique.passes.len());
    let mut code_samplers = Vec::with_capacity(technique.passes.len());

    for (index, pass) in technique.passes.iter().enumerate() {
        let pass_index = u8::try_from(index)
            .map_err(|_| MaterialRefusal::PassIndexOverflow { pass_index: index })?;
        let pair = pass
            .shader_pair
            .ok_or(MaterialRefusal::ShaderProgramMissing { pass_index })?;
        let prepared_pass = prepared_tech
            .passes
            .get(index)
            .ok_or(MaterialRefusal::UnsupportedShaderPair { pass_index, pair })?;
        let port_id = prepared_pass
            .port
            .get(usize::from(vertex_type))
            .copied()
            .flatten()
            .ok_or(MaterialRefusal::UnsupportedShaderPair { pass_index, pair })?;
        let state = prepared_pass.state;
        let load_bits = state.authored_words();
        if let Some(fields) = state.unsupported_host_fields() {
            return Err(MaterialRefusal::UnsupportedState {
                pass_index,
                fields,
                word0: load_bits[0],
                word1: load_bits[1],
            });
        }
        let Some(belts) = prepared_pass.belts.as_ref() else {
            return Err(MaterialRefusal::PassArgCountMismatch {
                pass_index,
                per_prim: pass.per_prim_arg_count,
                per_obj: pass.per_obj_arg_count,
                stable: pass.stable_arg_count,
                arguments: pass.arguments.len(),
            });
        };
        let belts = Arc::new(belts.clone());
        let (pass_constants, pass_samplers) = resolve_pass_code(&belts, code_sources, pass_index)?;
        let local_banks = prepared_pass
            .local_banks
            .get(usize::from(vertex_type))
            .cloned()
            .flatten();
        let local_samplers = prepared_pass.local_samplers.clone();
        shell_passes.push(StablePassShell {
            pass_index,
            port: port_id,
            shader_pair: pair,
            state,
            local_banks,
            local_samplers,
            belts,
        });
        code_constants.push(pass_constants);
        code_samplers.push(pass_samplers);
    }

    let arm = SetupArm::classify(tech_type, key.packed);
    Ok(MaterialExecution {
        tech_type,
        arm,
        vertex_type,
        shell: Arc::new(StableMaterialShell {
            generation_id: catalog.generation_id(),
            material_id: material.asset_id,
            source_selection: technique.source_selection,
            tech_type,
            arm,
            vertex_type,
            passes: shell_passes,
        }),
        code_constants,
        code_samplers,
    })
}

#[derive(Clone, Debug)]
pub struct StablePassShell {
    pass_index: u8,
    port: PortId,
    shader_pair: RuntimeShaderPair,
    state: CompiledPassState,
    local_banks: Option<Arc<PackedLocalBanks>>,
    local_samplers: Option<Arc<PackedLocalSamplers>>,
    belts: Arc<PreparedArgBelts>,
}

#[derive(Clone, Debug)]
pub struct StableMaterialShell {
    generation_id: crate::MaterialGenerationId,
    material_id: MaterialAssetId,
    source_selection: Option<crate::SourceTechniqueSelection>,
    tech_type: TechType,
    arm: SetupArm,
    vertex_type: u8,
    passes: Vec<StablePassShell>,
}

impl StableMaterialShell {
    pub fn material_id(&self) -> MaterialAssetId {
        self.material_id
    }
    pub fn source_selection(&self) -> Option<crate::SourceTechniqueSelection> {
        self.source_selection
    }
    pub fn generation_id(&self) -> crate::MaterialGenerationId {
        self.generation_id
    }
    pub fn passes(&self) -> &[StablePassShell] {
        &self.passes
    }
}

impl StablePassShell {
    pub fn state(&self) -> CompiledPassState {
        self.state
    }
    pub fn bind<'a>(
        &'a self,
        constants: &'a PackedCodeConstants,
        samplers: &'a PackedCodeSamplers,
    ) -> ExecutablePassView<'a> {
        ExecutablePassView {
            pass_index: self.pass_index,
            port: self.port,
            shader_pair: self.shader_pair,
            state: self.state,
            local_banks: self.local_banks.as_ref(),
            local_samplers: self.local_samplers.as_ref(),
            code_constants: &constants.lanes,
            code_samplers: &samplers.lanes,
            code_constant_id: constants.id,
            code_sampler_id: samplers.id,
        }
    }
}

pub fn capture_stable_shell(
    catalog: &RuntimeMaterialCatalog,
    prepared: &PreparedMaterialTable,
    key: MaterialDrawKey,
    tech_type: TechType,
    vertex_type: u8,
    execution: &MaterialExecution,
) -> Option<StableMaterialShell> {
    validate_prepared_generation(catalog, prepared).ok()?;
    let (material, technique) = resolve_material_technique(catalog, key, tech_type).ok()?;
    let prepared_tech = prepared.technique(material.asset_id, tech_type)?;
    let shell = &execution.shell;
    if shell.generation_id != catalog.generation_id()
        || shell.material_id != material.asset_id
        || shell.tech_type != tech_type
        || shell.vertex_type != vertex_type
        || execution.tech_type != shell.tech_type
        || execution.arm != shell.arm
        || execution.vertex_type != shell.vertex_type
    {
        return None;
    }
    if execution.pass_count() != technique.passes.len() {
        return None;
    }
    if prepared_tech.passes.len() != execution.pass_count() {
        return None;
    }
    Some((*execution.shell).clone())
}

pub fn rebind_stable_material(
    shell: &StableMaterialShell,
    catalog: &RuntimeMaterialCatalog,
    code_sources: &impl CodeSourceLookup,
) -> Result<MaterialExecution, MaterialRefusal> {
    let mut execution = MaterialExecution::vacant();
    execution.rebind_into(&Arc::new(shell.clone()), catalog, code_sources)?;
    Ok(execution)
}

impl MaterialExecution {
    fn vacant_shell() -> Arc<StableMaterialShell> {
        use std::sync::OnceLock;
        static VACANT: OnceLock<Arc<StableMaterialShell>> = OnceLock::new();
        VACANT
            .get_or_init(|| {
                Arc::new(StableMaterialShell {
                    generation_id: crate::MaterialGenerationId::default(),
                    material_id: MaterialAssetId(u16::MAX),
                    source_selection: None,
                    tech_type: TechType(0),
                    arm: SetupArm::Generic,
                    vertex_type: 0,
                    passes: Vec::new(),
                })
            })
            .clone()
    }

    pub fn vacant() -> Self {
        Self {
            tech_type: TechType(0),
            arm: SetupArm::Generic,
            vertex_type: 0,
            shell: Self::vacant_shell(),
            code_constants: Vec::new(),
            code_samplers: Vec::new(),
        }
    }

    pub fn pass_count(&self) -> usize {
        self.shell.passes.len()
    }

    pub fn release_code_rows(&mut self) {
        for constants in &mut self.code_constants {
            constants.lanes.clear();
        }
        for samplers in &mut self.code_samplers {
            samplers.lanes.clear();
        }
    }

    pub fn shell_arc(&self) -> &Arc<StableMaterialShell> {
        &self.shell
    }

    pub fn code_constants(&self) -> &[PackedCodeConstants] {
        &self.code_constants
    }

    pub fn pass(&self, index: usize) -> Option<ExecutablePassView<'_>> {
        let pass = self.shell.passes.get(index)?;
        let constants = self.code_constants.get(index)?;
        let samplers = self.code_samplers.get(index)?;
        Some(pass.bind(constants, samplers))
    }

    pub fn iter_passes(&self) -> impl Iterator<Item = ExecutablePassView<'_>> + '_ {
        (0..self.pass_count()).filter_map(|index| self.pass(index))
    }

    pub fn rebind_into(
        &mut self,
        shell: &Arc<StableMaterialShell>,
        catalog: &RuntimeMaterialCatalog,
        code_sources: &impl CodeSourceLookup,
    ) -> Result<(), MaterialRefusal> {
        let generation = catalog.generation_id();
        if shell.generation_id != generation {
            return Err(MaterialRefusal::StaleMaterialGeneration {
                retained: shell.generation_id,
                current: generation,
            });
        }
        self.tech_type = shell.tech_type;
        self.arm = shell.arm;
        self.vertex_type = shell.vertex_type;
        if !Arc::ptr_eq(&self.shell, shell) {
            self.shell = Arc::clone(shell);
        }
        let n = shell.passes.len();
        if self.code_constants.len() > n {
            self.code_constants.truncate(n);
            self.code_samplers.truncate(n);
        }
        while self.code_constants.len() < n {
            self.code_constants.push(PackedCodeConstants::default());
            self.code_samplers.push(PackedCodeSamplers::default());
        }
        for (index, pass) in shell.passes.iter().enumerate() {
            if let Some(fields) = pass.state.unsupported_host_fields() {
                return Err(MaterialRefusal::UnsupportedState {
                    pass_index: pass.pass_index,
                    fields,
                    word0: pass.state.authored_words()[0],
                    word1: pass.state.authored_words()[1],
                });
            }
            resolve_pass_code_into(
                &pass.belts,
                code_sources,
                pass.pass_index,
                &mut self.code_constants[index],
                &mut self.code_samplers[index],
            )?;
        }
        Ok(())
    }
}
