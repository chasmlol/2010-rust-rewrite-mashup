use asset_core::{AssetNamespace, AssetRef, MaterialIndex};

use crate::{RuntimeArgumentBinding, RuntimeShaderStage, RuntimeVertexDecl, TechType};

const FNV1A64_OFFSET: u64 = 0xcbf29ce484222325;
const FNV1A64_PRIME: u64 = 0x100_0000_01b3;

pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    fnv1a64_more(FNV1A64_OFFSET, bytes)
}

pub(crate) fn fnv1a64_more(mut hash: u64, bytes: &[u8]) -> u64 {
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV1A64_PRIME);
    }
    hash
}

pub const TECHNIQUE_SLOT_COUNT: usize = asset_iw4::size::TECHNIQUE_SLOT_COUNT;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaterialAssetId(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SortedMaterialOrdinal(u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MaterialGenerationId(u64);

impl MaterialGenerationId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

pub const fn sort_band(rank: u32) -> u16 {
    if (rank as usize) < SortedMaterialOrdinal::RETAIL_LIMIT {
        rank as u16
    } else {
        (SortedMaterialOrdinal::RETAIL_LIMIT - 1) as u16
    }
}

impl SortedMaterialOrdinal {
    pub const LIMIT: usize = 0x8000;

    pub const RETAIL_LIMIT: usize = 0x1000;

    pub fn new(value: u32) -> Result<Self, CatalogBuildError> {
        if value as usize >= Self::LIMIT {
            return Err(CatalogBuildError::SortedMaterialCapacity {
                count: value as usize + 1,
                capacity: Self::LIMIT,
            });
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    pub const fn sort_band(self) -> u16 {
        if (self.0 as usize) < Self::RETAIL_LIMIT {
            self.0 as u16
        } else {
            (Self::RETAIL_LIMIT - 1) as u16
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RuntimeTechniqueSetId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RuntimeImageId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RuntimeTextureBinding {
    pub image: RuntimeImageId,
    pub sampler_state: u8,

    pub semantic: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RuntimeShaderProgramId {
    pub asset_slot: u32,
    pub program_hash: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RuntimeShaderPair {
    pub vertex: RuntimeShaderProgramId,
    pub pixel: RuntimeShaderProgramId,
    pub vertex_decl_slot: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeProgramIdentity {
    pub vertex_program_hash: u64,
    pub pixel_program_hash: u64,
    pub vertex_decl_slot: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PortId {
    pub vertex_program_hash: u64,
    pub pixel_program_hash: u64,
    pub vertex_decl_slot: u32,
    pub vertex_type: u8,
    pub custom_sampler_flags: u8,
    pub arguments_hash: u64,
}

impl PortId {
    pub fn matches_shader_pair(self, pair: RuntimeShaderPair) -> bool {
        self.vertex_program_hash == pair.vertex.program_hash
            && self.pixel_program_hash == pair.pixel.program_hash
            && self.vertex_decl_slot == pair.vertex_decl_slot
    }

    pub fn from_pass(pass: &RuntimePass, vertex_type: u8) -> Option<Self> {
        let mut id = Self::from_parts(
            pass.shader_pair?,
            port_custom_sampler_flags(pass),
            &pass.arguments,
            vertex_type,
        );
        id.arguments_hash ^= pass.color_space.port_mix();
        if pass.hardware_shadow_compare {
            id.arguments_hash ^= 0x5AD0_C0DE;
        }
        Some(id)
    }

    pub fn from_parts(
        pair: RuntimeShaderPair,
        custom_sampler_flags: u8,
        arguments: &[RuntimeArgumentBinding],
        vertex_type: u8,
    ) -> Self {
        Self {
            vertex_program_hash: pair.vertex.program_hash,
            pixel_program_hash: pair.pixel.program_hash,
            vertex_decl_slot: pair.vertex_decl_slot,
            vertex_type,
            custom_sampler_flags,
            arguments_hash: hash_runtime_arguments(arguments),
        }
    }
}

fn port_custom_sampler_flags(pass: &RuntimePass) -> u8 {
    pass.custom_sampler_flags | pass.t5_custom_sampler_flags.wrapping_shl(4)
}

fn hash_runtime_arguments(arguments: &[RuntimeArgumentBinding]) -> u64 {
    let mut hash = fnv1a64(&(arguments.len() as u64).to_le_bytes());
    for argument in arguments {
        hash = hash_one_runtime_argument(hash, argument);
    }
    hash
}

fn hash_one_runtime_argument(hash: u64, argument: &RuntimeArgumentBinding) -> u64 {
    match argument {
        RuntimeArgumentBinding::MaterialTexture {
            destination,
            name_hash,
        } => {
            let hash = fnv1a64_more(hash, &[1]);
            let hash = fnv1a64_more(hash, &destination.to_le_bytes());
            fnv1a64_more(hash, &name_hash.to_le_bytes())
        }
        RuntimeArgumentBinding::CodeTexture { destination, index } => {
            let hash = fnv1a64_more(hash, &[2]);
            let hash = fnv1a64_more(hash, &destination.to_le_bytes());
            fnv1a64_more(hash, &index.to_le_bytes())
        }
        RuntimeArgumentBinding::MaterialConstant {
            stage,
            destination,
            name_hash,
        } => {
            let hash = fnv1a64_more(hash, &[3, runtime_stage_tag(*stage)]);
            let hash = fnv1a64_more(hash, &destination.to_le_bytes());
            fnv1a64_more(hash, &name_hash.to_le_bytes())
        }
        RuntimeArgumentBinding::LiteralConstant {
            stage,
            destination,
            words,
        } => {
            let hash = fnv1a64_more(hash, &[4, runtime_stage_tag(*stage)]);
            let hash = fnv1a64_more(hash, &destination.to_le_bytes());
            match words {
                Some(words) => {
                    let mut hash = fnv1a64_more(hash, &[1]);
                    for word in words {
                        hash = fnv1a64_more(hash, &word.to_le_bytes());
                    }
                    hash
                }
                None => fnv1a64_more(hash, &[0]),
            }
        }
        RuntimeArgumentBinding::CodeConstant {
            stage,
            destination,
            index,
            first_row,
            row_count,
        } => {
            let hash = fnv1a64_more(hash, &[5, runtime_stage_tag(*stage)]);
            let hash = fnv1a64_more(hash, &destination.to_le_bytes());
            let hash = fnv1a64_more(hash, &index.to_le_bytes());
            fnv1a64_more(hash, &[*first_row, *row_count])
        }
        RuntimeArgumentBinding::Unknown { argument_type, raw } => {
            let hash = fnv1a64_more(hash, &[6]);
            let hash = fnv1a64_more(hash, &argument_type.to_le_bytes());
            fnv1a64_more(hash, raw)
        }
    }
}

fn runtime_stage_tag(stage: RuntimeShaderStage) -> u8 {
    match stage {
        RuntimeShaderStage::Vertex => 0,
        RuntimeShaderStage::Pixel => 1,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemapResolution {
    SelfSet,
    Resolved(RuntimeTechniqueSetId),
    Missing { pointer_identity: u32 },
    Cycle { first_set: RuntimeTechniqueSetId },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeShaderProgram {
    pub id: RuntimeShaderProgramId,
    pub stage: RuntimeShaderStage,
    pub name: String,
    pub program: Vec<u8>,
}

pub fn sort_pass_args(pass: &mut RuntimePass) {
    let prim = usize::from(pass.per_prim_arg_count);
    let obj = usize::from(pass.per_obj_arg_count);
    let stable = usize::from(pass.stable_arg_count);
    if prim.saturating_add(obj).saturating_add(stable) != pass.arguments.len() {
        return;
    }
    let sort_tier = |slice: &mut [RuntimeArgumentBinding]| {
        slice.sort_by(|a, b| a.runtime_sort_key().cmp(&b.runtime_sort_key()));
    };
    sort_tier(&mut pass.arguments[..prim]);
    sort_tier(&mut pass.arguments[prim..prim + obj]);
    sort_tier(&mut pass.arguments[prim + obj..]);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimePass {
    pub shader_pair: Option<RuntimeShaderPair>,

    pub custom_sampler_flags: u8,

    pub t5_custom_sampler_flags: u8,

    pub per_prim_arg_count: u8,
    pub per_obj_arg_count: u8,
    pub stable_arg_count: u8,
    pub arguments: Vec<RuntimeArgumentBinding>,

    pub color_space: crate::PassColorSpace,

    pub hardware_shadow_compare: bool,
}

impl RuntimePass {
    pub fn same_compile_identity(&self, other: &Self) -> bool {
        self.shader_pair == other.shader_pair
            && self.custom_sampler_flags == other.custom_sampler_flags
            && self.t5_custom_sampler_flags == other.t5_custom_sampler_flags
            && self.arguments == other.arguments
            && self.color_space == other.color_space
            && self.hardware_shadow_compare == other.hardware_shadow_compare
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeTechnique {
    pub source_selection: Option<SourceTechniqueSelection>,
    pub flags: u16,
    pub passes: Vec<RuntimePass>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TechniqueSelectionPolicy {
    Exact,
    DfogCompatibility,
    UnshadowedCompatibility,
    LitFallbackCompatibility,
    EmissiveCompatibility,
    DepthToColourCompatibility,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceTechniqueSelection {
    pub namespace: AssetNamespace,
    pub slot: u8,
    pub policy: TechniqueSelectionPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeTechniqueSet {
    slots: Vec<Option<RuntimeTechnique>>,

    pub world_vert_format: u8,

    pub namespace: AssetNamespace,

    pub name: String,
}

impl RuntimeTechniqueSet {
    pub fn new(slots: Vec<Option<RuntimeTechnique>>) -> Result<Self, CatalogBuildError> {
        if slots.len() != TECHNIQUE_SLOT_COUNT {
            return Err(CatalogBuildError::TechniqueSlotCount {
                expected: TECHNIQUE_SLOT_COUNT,
                actual: slots.len(),
            });
        }
        Ok(Self {
            slots,
            world_vert_format: 0,
            namespace: AssetNamespace::Iw4,
            name: String::new(),
        })
    }

    pub fn techniques(&self) -> impl Iterator<Item = &RuntimeTechnique> {
        self.slots.iter().flatten()
    }

    pub fn technique(&self, tech_type: TechType) -> Option<&RuntimeTechnique> {
        self.slots.get(usize::from(tech_type.0))?.as_ref()
    }

    pub fn with_identity(
        mut self,
        world_vert_format: u8,
        namespace: AssetNamespace,
        name: impl Into<String>,
    ) -> Self {
        self.world_vert_format = world_vert_format;
        self.namespace = namespace;
        self.name = name.into();
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialDrawRules {
    pub colour_camera_region: u8,
    pub smodel_colour_emits: bool,
    pub unlit_sky: bool,
    pub postfx_host_supported: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeMaterial {
    pub asset_id: MaterialAssetId,

    pub name: String,

    pub namespace: AssetNamespace,

    pub technique_set: String,
    pub baked_draw_surf: Option<u64>,
    pub local_technique_set: RuntimeTechniqueSetId,
    pub remap: RemapResolution,
    pub state_bits_entry: Option<[u8; TECHNIQUE_SLOT_COUNT]>,
    pub pass_states: Vec<crate::CompiledPassState>,

    pub camera_region: u8,
    pub draw_rules: MaterialDrawRules,

    pub sort_key: u8,

    pub info_game_flags: u8,

    pub state_flags: u8,

    pub surface_type_bits: Option<u32>,

    pub unlit: bool,

    pub takes_model_lighting: bool,

    pub uses_model_lighting_const: bool,

    pub square_color_map: bool,

    pub shadow_only: bool,

    pub cull_mode: Option<u8>,
    pub uv_anim_bits: [u32; 4],
    pub falloff_parms_bits: [u32; 4],
    pub falloff_begin_bits: [u32; 4],
    pub falloff_end_bits: [u32; 4],
    pub env_map_parms_bits: [u32; 4],
    pub textures: Vec<(u32, Option<RuntimeTextureBinding>)>,
    pub constants: Vec<(u32, [u32; 4])>,
}

impl RuntimeMaterial {
    pub fn uv_anim(&self) -> [f32; 4] {
        self.uv_anim_bits.map(f32::from_bits)
    }

    pub fn falloff_parms(&self) -> [f32; 4] {
        self.falloff_parms_bits.map(f32::from_bits)
    }

    pub fn falloff_begin(&self) -> [f32; 4] {
        self.falloff_begin_bits.map(f32::from_bits)
    }

    pub fn falloff_end(&self) -> [f32; 4] {
        self.falloff_end_bits.map(f32::from_bits)
    }

    pub fn env_map_parms(&self) -> [f32; 4] {
        self.env_map_parms_bits.map(f32::from_bits)
    }

    pub fn texture_semantic(&self, semantic: u8) -> Option<RuntimeImageId> {
        self.textures.iter().find_map(|(_, texture)| {
            texture
                .as_ref()
                .filter(|binding| binding.semantic == semantic)
                .map(|binding| binding.image)
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeSortedMaterialTable {
    Missing,
    BuildFailed(CatalogBuildError),
    Ready {
        asset_ids_by_ordinal: Vec<MaterialAssetId>,

        ordinals_by_asset_id: Vec<Option<SortedMaterialOrdinal>>,

        skipped_n: u32,

        slot_gap_n: u32,
    },
}

impl RuntimeSortedMaterialTable {
    pub fn ordinal_for_asset_id(&self, asset_id: usize) -> Option<SortedMaterialOrdinal> {
        match self {
            Self::Ready {
                ordinals_by_asset_id,
                ..
            } => ordinals_by_asset_id.get(asset_id).copied().flatten(),
            Self::Missing { .. } | Self::BuildFailed(_) => None,
        }
    }

    pub fn dump_label(&self) -> &'static str {
        match self {
            Self::Ready { .. } => "ready",
            Self::BuildFailed(_) => "failed",
            Self::Missing { .. } => "missing",
        }
    }

    pub fn skipped_n(&self) -> Option<i64> {
        match self {
            Self::Ready { skipped_n, .. } => Some(i64::from(*skipped_n)),
            Self::BuildFailed(_) | Self::Missing { .. } => None,
        }
    }

    pub fn slot_gap_n(&self) -> Option<i64> {
        match self {
            Self::Ready { slot_gap_n, .. } => Some(i64::from(*slot_gap_n)),
            Self::BuildFailed(_) | Self::Missing { .. } => None,
        }
    }
}

impl Default for RuntimeSortedMaterialTable {
    fn default() -> Self {
        Self::Missing
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeMaterialBuild {
    pub materials: Vec<RuntimeMaterial>,

    pub material_indices_by_name: Vec<(String, usize)>,
    pub material_indices_by_key: std::collections::HashMap<asset_core::MaterialKey, usize>,
    pub technique_sets: Vec<RuntimeTechniqueSet>,

    pub shader_programs: Vec<Option<RuntimeShaderProgram>>,

    pub vertex_decls: Vec<(u32, RuntimeVertexDecl)>,
    pub sorted_materials: RuntimeSortedMaterialTable,

    pub iw5_remap: Option<String>,

    pub t5_remap: Option<String>,

    pub iw5_fallback_n: u32,

    pub t5_fallback_n: u32,

    pub sorted_first_skip: Option<String>,

    pub skip_tech: Option<String>,

    pub skip_cause: Option<String>,

    pub leftover_iw5_arg_n: u32,

    pub leftover_iw5_arg: Option<String>,

    pub leftover_iw5_arg2: Option<String>,

    pub leftover_t5_arg_n: u32,

    pub leftover_t5_arg: Option<String>,

    pub leftover_t5_arg2: Option<String>,

    pub leftover_t5_arg3: Option<String>,

    pub leftover_t5_arg4: Option<String>,

    pub leftover_t5_arg5: Option<String>,

    pub leftover_t5_dest6: Option<String>,

    pub leftover_t5_dest7: Option<String>,

    pub leftover_t5_dest17: Option<String>,

    pub leftover_t5_dest18: Option<String>,

    pub leftover_t5_dest19: Option<String>,

    pub leftover_t5_dest9: Option<String>,

    pub leftover_t5_dest10: Option<String>,

    pub leftover_t5_dest11: Option<String>,

    pub leftover_t5_dest20: Option<String>,

    pub leftover_t5_dest24: Option<String>,

    pub leftover_t5_dest25: Option<String>,

    pub leftover_t5_dest26: Option<String>,

    pub leftover_t5_dest27: Option<String>,

    pub leftover_unknown_n: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeMaterialCatalog {
    generation_id: MaterialGenerationId,
    parts: RuntimeMaterialBuild,
}

impl RuntimeMaterialBuild {
    pub fn publish(self) -> RuntimeMaterialCatalog {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        RuntimeMaterialCatalog {
            generation_id: MaterialGenerationId(
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            ),
            parts: self,
        }
    }
}

impl Default for RuntimeMaterialCatalog {
    fn default() -> Self {
        RuntimeMaterialBuild::default().publish()
    }
}

impl RuntimeMaterialCatalog {
    pub fn generation_id(&self) -> MaterialGenerationId {
        self.generation_id
    }

    pub fn parts(&self) -> &RuntimeMaterialBuild {
        &self.parts
    }

    pub fn derived(&self, id: MaterialIndex) -> Option<&RuntimeMaterial> {
        let row = self.parts.materials.get(id.order())?;
        (usize::from(row.asset_id.0) == id.order()).then_some(row)
    }

    pub fn ordinal_for_asset_id(&self, id: MaterialIndex) -> Option<SortedMaterialOrdinal> {
        self.derived(id)?;
        self.parts.sorted_materials.ordinal_for_asset_id(id.order())
    }

    pub fn material_for_sorted_ordinal(&self, ordinal: u32) -> Option<&RuntimeMaterial> {
        let RuntimeSortedMaterialTable::Ready {
            asset_ids_by_ordinal,
            ..
        } = &self.parts.sorted_materials
        else {
            return None;
        };
        let asset_id = *asset_ids_by_ordinal.get(ordinal as usize)?;
        self.parts
            .materials
            .get(usize::from(asset_id.0))
            .filter(|material| material.asset_id == asset_id)
    }

    pub fn ordinal_for_material_name(
        &self,
        name: impl AsRef<str>,
    ) -> Option<SortedMaterialOrdinal> {
        let material = self.material_for_name(name)?;
        self.parts
            .sorted_materials
            .ordinal_for_asset_id(usize::from(material.asset_id.0))
    }

    pub fn material_for_key(&self, key: &asset_core::MaterialKey) -> Option<&RuntimeMaterial> {
        self.parts
            .materials
            .get(*self.parts.material_indices_by_key.get(key)?)
    }

    pub fn material_for_name(&self, name: impl AsRef<str>) -> Option<&RuntimeMaterial> {
        let want = AssetRef::bare_name(name.as_ref());
        let index = self
            .parts
            .material_indices_by_name
            .binary_search_by(|(candidate, _)| candidate.as_str().cmp(want))
            .ok()
            .and_then(|index| self.parts.material_indices_by_name.get(index))
            .map(|(_, material)| *material)?;
        self.parts
            .materials
            .get(index)
            .filter(|material| material.name == want)
    }

    pub fn shader_program(&self, id: RuntimeShaderProgramId) -> Option<&RuntimeShaderProgram> {
        self.parts
            .shader_programs
            .get(usize::try_from(id.asset_slot).ok()?)?
            .as_ref()
            .filter(|program| program.id == id)
    }

    pub fn vertex_decl(&self, pointer_identity: u32) -> Option<&RuntimeVertexDecl> {
        self.parts
            .vertex_decls
            .binary_search_by_key(&pointer_identity, |(identity, _)| *identity)
            .ok()
            .map(|index| &self.parts.vertex_decls[index].1)
    }

    pub fn opcode_surface_coverage(&self) -> crate::OpcodeSurfaceCoverage {
        let programs = self.parts.shader_programs.iter().filter_map(|slot| {
            let program = slot.as_ref()?;
            Some((program.program.as_slice(), program.stage))
        });
        crate::measure_shader_surface_coverage(programs)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogBuildError {
    TechniqueSlotCount {
        expected: usize,
        actual: usize,
    },
    MaterialAssetIdOverflow {
        index: usize,
    },
    SortedMaterialCapacity {
        count: usize,
        capacity: usize,
    },
    TechniqueSetMissing {
        material: MaterialAssetId,
    },
    TechniqueSetNamespaceMismatch {
        material: MaterialAssetId,
        want: AssetNamespace,
        got: AssetNamespace,
    },
    TechniqueGraphMissing {
        material: MaterialAssetId,
    },
    TechniqueBodyMissing {
        material: MaterialAssetId,
        slot: u8,
    },
    ShaderIdentityMissing {
        material: MaterialAssetId,
        slot: u8,
    },
    MaterialConstantMissing {
        material: MaterialAssetId,
        name_hash: u32,
    },
    LiteralPixelConstantMissing {
        material: MaterialAssetId,
        slot: u8,
    },
    ComparatorInvariant {
        material: MaterialAssetId,
        other: MaterialAssetId,
        slot: u8,
    },
    NonFiniteComparatorConstant {
        material: MaterialAssetId,
        slot: u8,
    },
}
