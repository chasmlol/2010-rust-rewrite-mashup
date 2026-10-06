use std::sync::Arc;

use crate::catalog::{
    MaterialAssetId, PortId, RemapResolution, RuntimeImageId, RuntimeMaterial,
    RuntimeMaterialCatalog, RuntimePass, RuntimeShaderPair, RuntimeTextureBinding,
    TECHNIQUE_SLOT_COUNT,
};
use crate::{RuntimeArgumentBinding, RuntimeShaderStage, TechType};

const VERTEX_TYPE_COUNT: usize = asset_iw4::vertex_decl::VERTEX_TYPE_COUNT;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdmittedPortFacts {
    pub id: PortId,
    pub vertex_constant_len: usize,
    pub pixel_constant_len: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutableArgumentBinding {
    MaterialTexture {
        destination: u16,
        name_hash: u32,
        texture: RuntimeTextureBinding,
    },
    CodeTexture {
        destination: u16,
        index: u32,
        sampler_state: u8,

        image: Option<RuntimeImageId>,
    },
    MaterialConstant {
        stage: RuntimeShaderStage,
        destination: u16,
        name_hash: u32,
        words: [u32; 4],
    },
    LiteralConstant {
        stage: RuntimeShaderStage,
        destination: u16,
        words: [u32; 4],
    },
    CodeConstant {
        stage: RuntimeShaderStage,
        destination: u16,
        index: u16,
        first_row: u8,
        row_count: u8,

        rows: Arc<[[u32; 4]]>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PackedCodeArg {
    Constant {
        stage: RuntimeShaderStage,
        destination: u16,
        index: u16,
        first_row: u8,
        row_count: u8,
    },
    Sampler {
        destination: u16,
        index: u32,
    },
    Unknown {
        argument_type: u16,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedArgSlice {
    pub local: Vec<Option<ExecutableArgumentBinding>>,
    pub code: Vec<PackedCodeArg>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedArgBelts {
    pub per_prim: PreparedArgSlice,
    pub per_obj: PreparedArgSlice,
    pub stable: PreparedArgSlice,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PackedLocalBanks {
    pub vertex: Vec<[u32; 4]>,
    pub pixel: Vec<[u32; 4]>,
    pub written_vertex: Vec<bool>,
    pub written_pixel: Vec<bool>,
}

pub fn pack_local_banks(
    vertex_len: usize,
    pixel_len: usize,
    belts: &PreparedArgBelts,
) -> Option<PackedLocalBanks> {
    let mut vertex = vec![[0u32; 4]; vertex_len];
    let mut pixel = vec![[0u32; 4]; pixel_len];
    let mut written_vertex = vec![false; vertex_len];
    let mut written_pixel = vec![false; pixel_len];
    for slice in [&belts.per_prim, &belts.per_obj, &belts.stable] {
        for bound in slice.local.iter().flatten() {
            match bound {
                ExecutableArgumentBinding::MaterialConstant {
                    stage,
                    destination,
                    words,
                    ..
                }
                | ExecutableArgumentBinding::LiteralConstant {
                    stage,
                    destination,
                    words,
                } => seed_write(
                    *stage,
                    *destination,
                    std::slice::from_ref(words),
                    &mut vertex,
                    &mut pixel,
                    &mut written_vertex,
                    &mut written_pixel,
                )?,
                ExecutableArgumentBinding::MaterialTexture { .. }
                | ExecutableArgumentBinding::CodeTexture { .. }
                | ExecutableArgumentBinding::CodeConstant { .. } => {}
            }
        }
    }
    Some(PackedLocalBanks {
        vertex,
        pixel,
        written_vertex,
        written_pixel,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PackedLocalSamplerLane {
    pub register: u16,
    pub name_hash: u32,
    pub texture: RuntimeTextureBinding,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PackedLocalSamplers {
    pub id: u64,
    pub lanes: Vec<PackedLocalSamplerLane>,
}

impl Default for PackedLocalSamplers {
    fn default() -> Self {
        Self {
            id: hash_packed_local_lanes(&[]),
            lanes: Vec::new(),
        }
    }
}

impl PackedLocalSamplers {
    pub fn texture(&self, register: u16, name_hash: u32) -> Option<RuntimeTextureBinding> {
        self.lanes
            .iter()
            .find(|lane| lane.register == register && lane.name_hash == name_hash)
            .map(|lane| lane.texture)
    }
}

pub fn pack_local_samplers(belts: &PreparedArgBelts) -> PackedLocalSamplers {
    let mut lanes = Vec::new();
    for slice in [&belts.per_prim, &belts.per_obj, &belts.stable] {
        for bound in slice.local.iter().flatten() {
            if let ExecutableArgumentBinding::MaterialTexture {
                destination,
                name_hash,
                texture,
            } = bound
            {
                lanes.push(PackedLocalSamplerLane {
                    register: *destination,
                    name_hash: *name_hash,
                    texture: *texture,
                });
            }
        }
    }
    PackedLocalSamplers {
        id: hash_packed_local_lanes(&lanes),
        lanes,
    }
}

fn hash_packed_local_lanes(lanes: &[PackedLocalSamplerLane]) -> u64 {
    let mut hash = crate::catalog::fnv1a64(&(lanes.len() as u64).to_le_bytes());
    for lane in lanes {
        hash = crate::catalog::fnv1a64_more(hash, &lane.register.to_le_bytes());
        hash = crate::catalog::fnv1a64_more(hash, &lane.name_hash.to_le_bytes());
        hash = crate::catalog::fnv1a64_more(hash, &lane.texture.image.0.to_le_bytes());
        hash = crate::catalog::fnv1a64_more(
            hash,
            &[lane.texture.sampler_state, lane.texture.semantic],
        );
    }
    hash
}

fn seed_write(
    stage: RuntimeShaderStage,
    destination: u16,
    rows: &[[u32; 4]],
    vertex: &mut [[u32; 4]],
    pixel: &mut [[u32; 4]],
    written_vertex: &mut [bool],
    written_pixel: &mut [bool],
) -> Option<()> {
    let (bank, written) = match stage {
        RuntimeShaderStage::Vertex => (&mut *vertex, &mut *written_vertex),
        RuntimeShaderStage::Pixel => (&mut *pixel, &mut *written_pixel),
    };
    for (row, words) in rows.iter().enumerate() {
        let register = destination.checked_add(u16::try_from(row).ok()?)?;
        let index = usize::from(register);
        let slot = bank.get_mut(index)?;
        *slot = *words;
        *written.get_mut(index)? = true;
    }
    Some(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedPass {
    pub port: Arc<[Option<PortId>; VERTEX_TYPE_COUNT]>,

    pub shader_pair: Option<RuntimeShaderPair>,
    pub state: crate::CompiledPassState,

    pub per_prim_arg_count: u8,
    pub per_obj_arg_count: u8,
    pub stable_arg_count: u8,
    pub belts: Option<PreparedArgBelts>,

    pub local_banks: Arc<BankRow>,

    pub local_samplers: Option<Arc<PackedLocalSamplers>>,
}

impl PreparedArgBelts {
    fn from_pass(material: &RuntimeMaterial, pass_index: u8, pass: &RuntimePass) -> Option<Self> {
        let prim = usize::from(pass.per_prim_arg_count);
        let obj = usize::from(pass.per_obj_arg_count);
        let stable = usize::from(pass.stable_arg_count);
        if prim.saturating_add(obj).saturating_add(stable) != pass.arguments.len() {
            return None;
        }
        Some(Self {
            per_prim: PreparedArgSlice::bind_local(material, pass_index, &pass.arguments[..prim]),
            per_obj: PreparedArgSlice::bind_local(
                material,
                pass_index,
                &pass.arguments[prim..prim + obj],
            ),
            stable: PreparedArgSlice::bind_local(
                material,
                pass_index,
                &pass.arguments[prim + obj..],
            ),
        })
    }
}

impl PreparedArgSlice {
    fn bind_local(
        material: &RuntimeMaterial,
        pass_index: u8,
        args: &[RuntimeArgumentBinding],
    ) -> Self {
        Self {
            local: prebind_local_slice(material, pass_index, args),
            code: pack_code_args(args),
        }
    }
}

fn pack_code_args(args: &[RuntimeArgumentBinding]) -> Vec<PackedCodeArg> {
    args.iter()
        .filter_map(|argument| match argument {
            RuntimeArgumentBinding::CodeConstant {
                stage,
                destination,
                index,
                first_row,
                row_count,
            } => Some(PackedCodeArg::Constant {
                stage: *stage,
                destination: *destination,
                index: *index,
                first_row: *first_row,
                row_count: *row_count,
            }),
            RuntimeArgumentBinding::CodeTexture { destination, index } => {
                Some(PackedCodeArg::Sampler {
                    destination: *destination,
                    index: *index,
                })
            }
            RuntimeArgumentBinding::Unknown { argument_type, .. } => Some(PackedCodeArg::Unknown {
                argument_type: *argument_type,
            }),
            RuntimeArgumentBinding::MaterialTexture { .. }
            | RuntimeArgumentBinding::MaterialConstant { .. }
            | RuntimeArgumentBinding::LiteralConstant { .. } => None,
        })
        .collect()
}

fn is_material_local(argument: &RuntimeArgumentBinding) -> bool {
    matches!(
        argument,
        RuntimeArgumentBinding::MaterialTexture { .. }
            | RuntimeArgumentBinding::MaterialConstant { .. }
            | RuntimeArgumentBinding::LiteralConstant { .. }
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedTechnique {
    pub source_selection: Option<crate::SourceTechniqueSelection>,
    pub flags: u16,
    pub passes: Vec<PreparedPass>,

    pub code_sampler_mask: u64,
}

impl PreparedTechnique {
    pub fn binds_code_texture(&self, index: u32) -> bool {
        if index < 64 {
            self.code_sampler_mask & (1u64 << index) != 0
        } else {
            self.passes.iter().any(|pass| {
                pass.belts.as_ref().is_some_and(|belts| {
                    [&belts.per_prim, &belts.per_obj, &belts.stable]
                        .into_iter()
                        .any(|slice| {
                            slice.code.iter().any(|arg| {
                                matches!(arg, PackedCodeArg::Sampler { index: slot, .. } if *slot == index)
                            })
                        })
                })
            })
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedMaterial {
    pub tech: Vec<Option<PreparedTechnique>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreparedMaterialTable {
    generation_id: crate::MaterialGenerationId,
    materials: Vec<PreparedMaterial>,

    census: PreparedTableCensus,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PreparedTableCensus {
    pub port_slot_n: usize,
    pub local_bank_n: usize,
    pub local_sampler_n: usize,
}

impl PreparedMaterialTable {
    pub fn from_catalog(
        catalog: &RuntimeMaterialCatalog,
        mut admit: impl FnMut(&RuntimePass, u8) -> Option<AdmittedPortFacts>,
    ) -> Self {
        let mut materials = Vec::with_capacity(catalog.parts().materials.len());
        let mut shared = SharedPacks::default();
        for material in &catalog.parts().materials {
            materials.push(prepare_one_material(
                catalog,
                &mut admit,
                &mut shared,
                material,
            ));
        }
        let census = count_prepared_table(&materials);
        Self {
            generation_id: catalog.generation_id(),
            materials,
            census,
        }
    }

    pub fn generation_id(&self) -> crate::MaterialGenerationId {
        self.generation_id
    }

    pub fn material(&self, id: MaterialAssetId) -> Option<&PreparedMaterial> {
        self.materials.get(usize::from(id.0))
    }

    pub fn technique(
        &self,
        id: MaterialAssetId,
        tech_type: TechType,
    ) -> Option<&PreparedTechnique> {
        self.material(id)?
            .tech
            .get(usize::from(tech_type.0))?
            .as_ref()
    }

    pub fn port(
        &self,
        id: MaterialAssetId,
        tech_type: TechType,
        pass_index: u8,
        vertex_type: u8,
    ) -> Option<PortId> {
        self.technique(id, tech_type)?
            .passes
            .get(usize::from(pass_index))?
            .port
            .get(usize::from(vertex_type))
            .copied()
            .flatten()
    }

    pub fn passes_for_tech(
        &self,
        tech: TechType,
    ) -> impl Iterator<Item = (MaterialAssetId, &PreparedPass)> {
        self.materials
            .iter()
            .enumerate()
            .filter_map(move |(index, material)| {
                material
                    .tech
                    .get(usize::from(tech.0))
                    .and_then(Option::as_ref)
                    .map(|technique| (MaterialAssetId(index as u16), technique))
            })
            .flat_map(|(id, technique)| technique.passes.iter().map(move |pass| (id, pass)))
    }

    pub fn len(&self) -> usize {
        self.materials.len()
    }

    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }

    pub fn port_slot_n(&self) -> usize {
        self.census.port_slot_n
    }

    pub fn local_bank_n(&self) -> usize {
        self.census.local_bank_n
    }

    pub fn local_sampler_n(&self) -> usize {
        self.census.local_sampler_n
    }
}

fn count_prepared_table(materials: &[PreparedMaterial]) -> PreparedTableCensus {
    let passes = || {
        materials
            .iter()
            .flat_map(|material| material.tech.iter().flatten())
            .flat_map(|technique| technique.passes.iter())
    };
    PreparedTableCensus {
        port_slot_n: passes()
            .flat_map(|pass| pass.port.iter())
            .filter(|port| port.is_some())
            .count(),
        local_bank_n: passes()
            .flat_map(|pass| pass.local_banks.iter())
            .filter(|bank| bank.is_some())
            .count(),
        local_sampler_n: passes()
            .filter(|pass| pass.local_samplers.is_some())
            .count(),
    }
}

type BankRow = [Option<Arc<PackedLocalBanks>>; VERTEX_TYPE_COUNT];

#[derive(Default)]
struct SharedPacks {
    banks: std::collections::HashSet<Arc<PackedLocalBanks>>,
    samplers: std::collections::HashSet<Arc<PackedLocalSamplers>>,
    ports: std::collections::HashSet<Arc<[Option<PortId>; VERTEX_TYPE_COUNT]>>,
    bank_rows: std::collections::HashMap<[Option<usize>; VERTEX_TYPE_COUNT], Arc<BankRow>>,
}

impl SharedPacks {
    fn bank(&mut self, bank: PackedLocalBanks) -> Arc<PackedLocalBanks> {
        if let Some(shared) = self.banks.get(&bank) {
            return Arc::clone(shared);
        }
        let bank = Arc::new(bank);
        self.banks.insert(Arc::clone(&bank));
        bank
    }

    fn ports(
        &mut self,
        ports: [Option<PortId>; VERTEX_TYPE_COUNT],
    ) -> Arc<[Option<PortId>; VERTEX_TYPE_COUNT]> {
        if let Some(shared) = self.ports.get(&ports) {
            return Arc::clone(shared);
        }
        let ports = Arc::new(ports);
        self.ports.insert(Arc::clone(&ports));
        ports
    }

    fn bank_row(&mut self, row: BankRow) -> Arc<BankRow> {
        let key =
            std::array::from_fn(|index| row[index].as_ref().map(|bank| Arc::as_ptr(bank) as usize));
        Arc::clone(self.bank_rows.entry(key).or_insert_with(|| Arc::new(row)))
    }

    fn samplers(&mut self, samplers: PackedLocalSamplers) -> Arc<PackedLocalSamplers> {
        if let Some(shared) = self.samplers.get(&samplers) {
            return Arc::clone(shared);
        }
        let samplers = Arc::new(samplers);
        self.samplers.insert(Arc::clone(&samplers));
        samplers
    }
}

fn prepare_one_material(
    catalog: &RuntimeMaterialCatalog,
    admit: &mut impl FnMut(&RuntimePass, u8) -> Option<AdmittedPortFacts>,
    shared: &mut SharedPacks,
    material: &RuntimeMaterial,
) -> PreparedMaterial {
    let mut tech = vec![None; TECHNIQUE_SLOT_COUNT];
    let set_id = match material.remap {
        RemapResolution::SelfSet => material.local_technique_set,
        RemapResolution::Resolved(set) => set,
        RemapResolution::Missing { .. } | RemapResolution::Cycle { .. } => {
            return PreparedMaterial { tech };
        }
    };
    let Some(set) = catalog.parts().technique_sets.get(set_id.0 as usize) else {
        return PreparedMaterial { tech };
    };
    let Some(entries) = material.state_bits_entry.as_ref() else {
        return PreparedMaterial { tech };
    };
    for slot in 0..TECHNIQUE_SLOT_COUNT {
        let tech_type = TechType(u8::try_from(slot).unwrap_or(u8::MAX));
        let Some(technique) = set.technique(tech_type) else {
            continue;
        };
        if technique.passes.is_empty() {
            continue;
        }
        let base_row = entries.get(slot).copied().unwrap_or(0xff);
        if base_row == 0xff
            || usize::from(base_row)
                .checked_add(technique.passes.len())
                .is_none_or(|end| end > material.pass_states.len())
        {
            continue;
        }
        let mut passes = Vec::with_capacity(technique.passes.len());
        for (pass_index, pass) in technique.passes.iter().enumerate() {
            let row = usize::from(base_row).saturating_add(pass_index);
            let state = material.pass_states[row];
            let mut port = [None; VERTEX_TYPE_COUNT];
            let mut bank_lens = [None; VERTEX_TYPE_COUNT];

            if pass.shader_pair.is_some() {
                for vertex_type in 0..VERTEX_TYPE_COUNT {
                    let vertex_type_u8 = u8::try_from(vertex_type).unwrap_or(u8::MAX);
                    if let Some(facts) = admit(pass, vertex_type_u8) {
                        port[vertex_type] = Some(facts.id);
                        bank_lens[vertex_type] =
                            Some((facts.vertex_constant_len, facts.pixel_constant_len));
                    }
                }
            }
            let pass_index_u8 = u8::try_from(pass_index).unwrap_or(u8::MAX);
            let belts = PreparedArgBelts::from_pass(material, pass_index_u8, pass);
            let mut local_banks = [const { None }; VERTEX_TYPE_COUNT];
            if let Some(belts) = belts.as_ref() {
                for (vertex_type, lens) in bank_lens.iter().copied().enumerate() {
                    let Some((vertex_len, pixel_len)) = lens else {
                        continue;
                    };
                    local_banks[vertex_type] = pack_local_banks(vertex_len, pixel_len, belts)
                        .map(|bank| shared.bank(bank));
                }
            }
            let local_samplers = belts
                .as_ref()
                .map(|belts| shared.samplers(pack_local_samplers(belts)));
            passes.push(PreparedPass {
                port: shared.ports(port),
                shader_pair: pass.shader_pair,
                state,
                per_prim_arg_count: pass.per_prim_arg_count,
                per_obj_arg_count: pass.per_obj_arg_count,
                stable_arg_count: pass.stable_arg_count,
                belts,
                local_banks: shared.bank_row(local_banks),
                local_samplers,
            });
        }
        if !passes.is_empty() {
            let code_sampler_mask = code_sampler_mask_from_passes(&passes);
            tech[slot] = Some(PreparedTechnique {
                source_selection: technique.source_selection,
                flags: technique.flags,
                passes,
                code_sampler_mask,
            });
        }
    }
    PreparedMaterial { tech }
}

struct HashWalk {
    tex: usize,
    kst: usize,
    cmp: u32,
}

fn hashed_lookup<T: Copy>(
    table: &[(u32, T)],
    cursor: &mut usize,
    name_hash: u32,
    cmp: &mut u32,
) -> Option<T> {
    // Remapped code samplers can request hashes outside the authored walk order.
    if table.get(*cursor).is_none_or(|(hash, _)| *hash > name_hash) {
        *cursor = 0;
    }
    while *cursor < table.len() {
        *cmp = cmp.saturating_add(1);
        let hash = table[*cursor].0;
        if hash == name_hash {
            return Some(table[*cursor].1);
        }
        if hash > name_hash {
            return None;
        }
        *cursor += 1;
    }
    None
}

fn bind_material_texture_cursor(
    material: &RuntimeMaterial,
    walk: &mut HashWalk,
    destination: u16,
    name_hash: u32,
) -> Option<ExecutableArgumentBinding> {
    let texture =
        hashed_lookup(&material.textures, &mut walk.tex, name_hash, &mut walk.cmp).flatten()?;
    Some(ExecutableArgumentBinding::MaterialTexture {
        destination,
        name_hash,
        texture,
    })
}

fn bind_material_constant_cursor(
    material: &RuntimeMaterial,
    walk: &mut HashWalk,
    stage: RuntimeShaderStage,
    destination: u16,
    name_hash: u32,
) -> Option<ExecutableArgumentBinding> {
    let words = hashed_lookup(&material.constants, &mut walk.kst, name_hash, &mut walk.cmp)?;
    Some(ExecutableArgumentBinding::MaterialConstant {
        stage,
        destination,
        name_hash,
        words,
    })
}

fn bind_local_argument(
    material: &RuntimeMaterial,
    argument: &RuntimeArgumentBinding,
    walk: &mut HashWalk,
) -> Option<ExecutableArgumentBinding> {
    match argument {
        RuntimeArgumentBinding::MaterialTexture {
            destination,
            name_hash,
        } => bind_material_texture_cursor(material, walk, *destination, *name_hash),
        RuntimeArgumentBinding::MaterialConstant {
            stage,
            destination,
            name_hash,
        } => bind_material_constant_cursor(material, walk, *stage, *destination, *name_hash),
        RuntimeArgumentBinding::LiteralConstant {
            stage,
            destination,
            words,
        } => Some(ExecutableArgumentBinding::LiteralConstant {
            stage: *stage,
            destination: *destination,
            words: (*words)?,
        }),
        RuntimeArgumentBinding::CodeTexture { .. }
        | RuntimeArgumentBinding::CodeConstant { .. }
        | RuntimeArgumentBinding::Unknown { .. } => None,
    }
}

fn prebind_local_slice(
    material: &RuntimeMaterial,
    _pass_index: u8,
    args: &[RuntimeArgumentBinding],
) -> Vec<Option<ExecutableArgumentBinding>> {
    let n = args.len();
    let mut local = vec![None; n];
    let mut walk = HashWalk {
        tex: 0,
        kst: 0,
        cmp: 0,
    };
    let mut i = 0usize;
    let mut take_type = |ty: u16, walk: &mut HashWalk, i: &mut usize| {
        while *i < n && args[*i].mtl_arg_type() == ty {
            if is_material_local(&args[*i]) {
                local[*i] = bind_local_argument(material, &args[*i], walk);
            }
            *i += 1;
        }
    };
    take_type(
        asset_iw4::size::mtl_arg::MATERIAL_VERTEX_CONST,
        &mut walk,
        &mut i,
    );
    take_type(
        asset_iw4::size::mtl_arg::LITERAL_VERTEX_CONST,
        &mut walk,
        &mut i,
    );
    take_type(
        asset_iw4::size::mtl_arg::MATERIAL_PIXEL_SAMPLER,
        &mut walk,
        &mut i,
    );
    take_type(
        asset_iw4::size::mtl_arg::CODE_VERTEX_CONST,
        &mut walk,
        &mut i,
    );
    take_type(
        asset_iw4::size::mtl_arg::CODE_PIXEL_SAMPLER,
        &mut walk,
        &mut i,
    );
    take_type(
        asset_iw4::size::mtl_arg::CODE_PIXEL_CONST,
        &mut walk,
        &mut i,
    );
    walk.kst = 0;
    take_type(
        asset_iw4::size::mtl_arg::MATERIAL_PIXEL_CONST,
        &mut walk,
        &mut i,
    );
    take_type(
        asset_iw4::size::mtl_arg::LITERAL_PIXEL_CONST,
        &mut walk,
        &mut i,
    );
    if i < n {
        while i < n {
            if is_material_local(&args[i]) {
                local[i] = bind_local_argument(material, &args[i], &mut walk);
            }
            i += 1;
        }
    }
    local
}

fn code_sampler_mask_from_passes(passes: &[PreparedPass]) -> u64 {
    let mut mask = 0u64;
    for pass in passes {
        let Some(belts) = pass.belts.as_ref() else {
            continue;
        };
        for slice in [&belts.per_prim, &belts.per_obj, &belts.stable] {
            for arg in &slice.code {
                if let PackedCodeArg::Sampler { index, .. } = arg
                    && *index < 64
                {
                    mask |= 1u64 << index;
                }
            }
        }
    }
    mask
}
