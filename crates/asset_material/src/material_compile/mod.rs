mod sort;
mod state;
pub use state::compile_material_state;
mod technique;

use render_material::{
    MaterialAssetId, RemapResolution, RuntimeImageId, RuntimeMaterial, RuntimeMaterialBuild,
    RuntimeMaterialCatalog, RuntimeShaderProgram, RuntimeShaderProgramId, RuntimeShaderStage,
    RuntimeSortedMaterialTable, RuntimeTechniqueSet, RuntimeTechniqueSetId, RuntimeTextureBinding,
    RuntimeVertexDecl, TECHNIQUE_SLOT_COUNT,
};
use sort::build_sorted_material_table;
use std::collections::BTreeMap;

fn program_hash(program: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in program {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub fn compile_material_catalog(source: &crate::MaterialDefinitions) -> RuntimeMaterialCatalog {
    let mut technique_sets = Vec::new();
    let mut vertex_decls = Vec::<(u32, RuntimeVertexDecl)>::new();
    for facts in source.technique_set_facts() {
        let compiler = technique::compiler_for(facts.namespace);
        let mut slots = vec![None; TECHNIQUE_SLOT_COUNT];
        if let Some(graph) = facts.table.as_ref().and_then(|table| table.graph.as_ref()) {
            for (slot_index, technique) in graph.slots.iter().enumerate().take(TECHNIQUE_SLOT_COUNT)
            {
                let Some(technique) = technique else {
                    continue;
                };
                slots[slot_index] = Some(technique::compile_technique(
                    compiler,
                    source,
                    slot_index as u8,
                    technique,
                    &mut vertex_decls,
                ));
            }
        }
        technique_sets.push(
            RuntimeTechniqueSet::new(slots)
                .expect("capture technique set is TECHNIQUE_SLOT_COUNT slots")
                .with_identity(
                    facts.world_vert_format,
                    facts.namespace,
                    facts.name.to_string(),
                ),
        );
    }

    let mut sorted_first_skip = None;
    let mut skip_tech = None;
    let mut skip_cause = None;
    let sorted_materials = match build_sorted_material_table(source) {
        Ok((
            asset_ids_by_ordinal,
            ordinals_by_asset_id,
            skipped_n,
            slot_gap_n,
            first,
            tech,
            cause,
        )) => {
            sorted_first_skip = first;
            skip_tech = tech;
            skip_cause = cause;
            RuntimeSortedMaterialTable::Ready {
                asset_ids_by_ordinal,
                ordinals_by_asset_id,
                skipped_n,
                slot_gap_n,
            }
        }
        Err(cause) => RuntimeSortedMaterialTable::BuildFailed(cause),
    };

    let materials: Vec<RuntimeMaterial> = source
        .materials
        .iter()
        .enumerate()
        .map(|(material_index, material)| {
            let key = crate::TechsetKey::new(material.namespace, material.technique_set.as_str());
            let local_index = match source.resolve_technique_set(key) {
                crate::TechsetResolve::Hit { index, .. } => Some(index),
                crate::TechsetResolve::GraphMissing { .. }
                | crate::TechsetResolve::Foreign { .. }
                | crate::TechsetResolve::Missing => None,
            };
            let local_technique_set = RuntimeTechniqueSetId(
                local_index
                    .and_then(|index| u32::try_from(index).ok())
                    .unwrap_or(u32::MAX),
            );
            let asset_id = MaterialAssetId(
                u16::try_from(material_index)
                    .expect("material catalog index exceeds the u16 asset-id domain"),
            );
            let baked_draw_surf =
                sorted_materials
                    .ordinal_for_asset_id(material_index)
                    .map(|ordinal| {
                        let table = material.technique_table.as_ref();
                        dpvs_iw4::bake_material_draw_surf_key(dpvs_iw4::MaterialDrawSurfBakeInput {
                            sort_key: material.sort_key,
                            info_game_flags: material.info_game_flags,
                            material_sorted_index: ordinal.sort_band(),
                            technique0_absent: table.is_none_or(|table| table.slots & 1 == 0),
                            technique1_present: table.is_some_and(|table| table.slots & 2 != 0),
                            material_byte_4b: material.state_flags,
                            technique0_flags: table.map_or(0, |table| table.technique0_flags),
                        })
                        .packed
                    });
            let [uv_anim, falloff_parms, falloff_begin, falloff_end] =
                crate::MaterialDefinitions::material_animation(material);
            RuntimeMaterial {
                asset_id,
                name: material.name.to_string(),
                namespace: material.namespace,
                technique_set: material.technique_set.to_string(),
                baked_draw_surf,
                local_technique_set,
                remap: RemapResolution::SelfSet,
                state_bits_entry: material.state_bits_entry,
                pass_states: material
                    .state_bits
                    .iter()
                    .map(|words| technique::compiler_for(material.namespace).compile_state(*words))
                    .collect(),
                camera_region: material.camera_region,
                draw_rules: technique::compiler_for(material.namespace)
                    .draw_rules(material, source.is_unlit(material).unwrap_or(false)),
                sort_key: material.sort_key,
                info_game_flags: material.info_game_flags,
                state_flags: material.state_flags,
                surface_type_bits: material.surface_type_bits,
                unlit: source.is_unlit(material).unwrap_or(false),
                takes_model_lighting: source.takes_model_lighting(material) == Some(true),
                uses_model_lighting_const: material
                    .route
                    .is_some_and(|route| route.uses_model_lighting_const),
                square_color_map: source.color_map_transform(material)
                    == crate::ColorMapTransform::Square,
                shadow_only: source.is_shadowcaster(material),
                cull_mode: match source.cull_face(material) {
                    Some(crate::MaterialCullFace::Back) => Some(0),
                    Some(crate::MaterialCullFace::Front) => Some(1),
                    Some(crate::MaterialCullFace::None) | None => None,
                },
                uv_anim_bits: uv_anim.map(f32::to_bits),
                falloff_parms_bits: falloff_parms.map(f32::to_bits),
                falloff_begin_bits: falloff_begin.map(f32::to_bits),
                falloff_end_bits: falloff_end.map(f32::to_bits),
                env_map_parms_bits: material
                    .constants
                    .iter()
                    .find(|constant| constant.name.starts_with(b"envMapParms"))
                    .map(|constant| constant.literal.map(f32::to_bits))
                    .unwrap_or([0; 4]),
                textures: {
                    let mut textures = material
                        .textures
                        .iter()
                        .map(|texture| {
                            (
                                texture.name_hash,
                                texture.image.and_then(|index| {
                                    u32::try_from(index)
                                        .ok()
                                        .map(|image| RuntimeTextureBinding {
                                            image: RuntimeImageId(image),
                                            sampler_state: texture.sampler_state,
                                            semantic: texture.semantic,
                                        })
                                }),
                            )
                        })
                        .collect::<Vec<_>>();
                    textures.sort_by_key(|(hash, _)| *hash);
                    textures
                },
                constants: {
                    let mut constants = material
                        .constants
                        .iter()
                        .map(|constant| (constant.name_hash, constant.literal.map(f32::to_bits)))
                        .collect::<Vec<_>>();
                    constants.sort_by_key(|(hash, _)| *hash);
                    constants
                },
            }
        })
        .collect();
    let mut material_indices_by_name = BTreeMap::new();
    for (index, material) in materials.iter().enumerate() {
        material_indices_by_name
            .entry(material.name.clone())
            .or_insert(index);
    }
    let material_indices_by_name = material_indices_by_name.into_iter().collect();
    let material_indices_by_key = materials
        .iter()
        .enumerate()
        .map(|(index, material)| {
            (
                asset_core::MaterialKey {
                    namespace: material.namespace,
                    name: material.name.clone(),
                },
                index,
            )
        })
        .collect();

    let shader_programs = source
        .shaders
        .iter()
        .enumerate()
        .map(|(asset_slot, shader)| {
            let stage = if shader.is_vertex() {
                RuntimeShaderStage::Vertex
            } else if shader.is_pixel() {
                RuntimeShaderStage::Pixel
            } else {
                panic!(
                    "material shader corpus contains non-shader asset type {:?} at slot {asset_slot}",
                    shader.kind
                );
            };
            let asset_slot = u32::try_from(asset_slot)
                .expect("shader catalog index exceeds the runtime u32 asset-slot domain");

            if shader.program.is_empty() {
                return None;
            }
            Some(RuntimeShaderProgram {
                id: RuntimeShaderProgramId {
                    asset_slot,
                    program_hash: program_hash(&shader.program),
                },
                stage,
                name: shader.name.to_string(),
                program: shader.program.clone(),
            })
        })
        .collect();

    RuntimeMaterialBuild {
        materials,
        material_indices_by_name,
        material_indices_by_key,
        technique_sets,
        shader_programs,
        vertex_decls,
        sorted_materials,
        iw5_remap: crate::iw5_tech_map::leftover_selector_census(
            source
                .materials
                .iter()
                .filter_map(|material| material.iw5_state_bits_entry.as_ref()),
        ),
        t5_remap: crate::t5_tech_map::leftover_t5_selector_census(
            source
                .materials
                .iter()
                .filter_map(|material| material.t5_state_bits_entry.as_ref()),
        ),
        iw5_fallback_n: source
            .technique_set_facts()
            .iter()
            .filter(|facts| facts.iw5_fallback_table.is_some())
            .count() as u32,
        t5_fallback_n: source
            .technique_set_facts()
            .iter()
            .filter(|facts| facts.t5_fallback_table.is_some())
            .count() as u32,
        sorted_first_skip,
        skip_tech,
        skip_cause,
        leftover_iw5_arg_n: source.leftover_iw5_arg_n,
        leftover_iw5_arg: source.leftover_iw5_arg_top(),
        leftover_iw5_arg2: source.leftover_iw5_arg_ranked(1),
        leftover_t5_arg_n: source.leftover_t5_arg_n,
        leftover_t5_arg: source.leftover_t5_arg_top(),
        leftover_t5_arg2: source.leftover_t5_arg_ranked(1),
        leftover_t5_arg3: source.leftover_t5_arg_ranked(2),
        leftover_t5_arg4: source.leftover_t5_arg_ranked(3),
        leftover_t5_arg5: source.leftover_t5_arg_ranked(4),
        leftover_t5_dest6: source.leftover_t5_arg_dest(6),
        leftover_t5_dest7: source.leftover_t5_arg_dest(7),
        leftover_t5_dest17: source.leftover_t5_arg_dest(17),
        leftover_t5_dest18: source.leftover_t5_arg_dest(18),
        leftover_t5_dest19: source.leftover_t5_arg_dest(19),
        leftover_t5_dest9: source.leftover_t5_arg_dest(9),
        leftover_t5_dest10: source.leftover_t5_arg_dest(10),
        leftover_t5_dest11: source.leftover_t5_arg_dest(11),
        leftover_t5_dest20: source.leftover_t5_arg_dest(20),
        leftover_t5_dest24: source.leftover_t5_arg_dest(24),
        leftover_t5_dest25: source.leftover_t5_arg_dest(25),
        leftover_t5_dest26: source.leftover_t5_arg_dest(26),
        leftover_t5_dest27: source.leftover_t5_arg_dest(27),
        leftover_unknown_n: remaining_unknown_arg_n(source),
    }
    .publish()
}

fn remaining_unknown_arg_n(source: &crate::MaterialDefinitions) -> u32 {
    let mut n = 0u32;
    for facts in source.technique_set_facts() {
        for table in facts
            .table
            .iter()
            .chain(facts.iw5_fallback_table.iter())
            .chain(facts.t5_fallback_table.iter())
        {
            let Some(graph) = &table.graph else {
                continue;
            };
            for technique in graph.slots.iter().flatten() {
                for pass in &technique.passes {
                    for argument in &pass.arguments {
                        if matches!(argument, crate::OwnedShaderArgument::Unknown { .. }) {
                            n = n.saturating_add(1);
                        }
                    }
                }
            }
        }
    }
    n
}
