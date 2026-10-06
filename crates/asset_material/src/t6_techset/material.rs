use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum T6MaterialRefusal {
    MissingSource,
    WrongSourceFamily,
    MissingTechniqueGraph,
}

impl MaterialCatalog {
    #[allow(clippy::too_many_arguments)]
    pub fn t6_material(
        &mut self,
        source: usize,
        name: &str,
        set: &T6TechniqueSet,
        textures: &[T6Texture],
        constants: Vec<crate::MaterialConstant>,
        state: &T6MaterialState,
        draw: T6Draw,
        report: &mut Vec<String>,
    ) -> Result<usize, T6MaterialRefusal> {
        let seed = self
            .materials
            .get(source)
            .ok_or(T6MaterialRefusal::MissingSource)?;
        if seed.namespace != crate::AssetNamespace::T6 {
            return Err(T6MaterialRefusal::WrongSourceFamily);
        }
        let mut material = seed.clone();
        let technique_set = draw.technique_set_name(&set.name);
        let technique_set = technique_set.as_str();
        let graph = self
            .technique_set_facts()
            .iter()
            .find(|facts| {
                facts.namespace == crate::AssetNamespace::T6 && facts.name.as_str() == technique_set
            })
            .ok_or(T6MaterialRefusal::MissingTechniqueGraph)?
            .table
            .as_ref()
            .ok_or(T6MaterialRefusal::MissingTechniqueGraph)?
            .graph
            .as_ref()
            .ok_or(T6MaterialRefusal::MissingTechniqueGraph)?;
        let bindings = state.bind_techniques(graph);
        let selected_sources: BTreeSet<_> = graph
            .slots
            .iter()
            .enumerate()
            .filter_map(|(slot, technique)| {
                (bindings.entries[slot] != u8::MAX)
                    .then_some(technique.as_ref()?.source_selection?.slot)
            })
            .collect();
        let selected_techniques: Vec<_> = selected_sources
            .into_iter()
            .filter_map(|slot| set.techniques.get(usize::from(slot))?.as_ref())
            .collect();
        for refusal in bindings.refusals {
            report.push(format!("t6 material {name}: {refusal:?}"));
        }
        let sampled: BTreeSet<u32> = selected_techniques
            .iter()
            .flat_map(|technique| &technique.passes)
            .flat_map(|pass| &pass.arguments)
            .filter(|a| a.kind == argument_type::MATERIAL_PIXEL_SAMPLER)
            .map(|a| a.def)
            .chain(
                selected_techniques
                    .iter()
                    .flat_map(|technique| &technique.passes)
                    .any(reads_float_z)
                    .then_some(FLOAT_Z_HASH),
            )
            .collect();
        let defaults: Vec<T6Texture> = sampled
            .into_iter()
            .filter(|hash| !textures.iter().any(|t| t.name_hash == *hash))
            .map(|hash| {
                if let Some(texture) = sampler_aliases(hash)
                    .iter()
                    .find_map(|alias| textures.iter().find(|t| t.name_hash == *alias))
                {
                    return T6Texture {
                        name_hash: hash,
                        ..texture.clone()
                    };
                }
                let (image, rgba) = match hash {
                    NORMAL_MAP_HASH | ARM_NORMAL_MAP_HASH => {
                        ("$t6_flat_normal", [128, 128, 255, 255])
                    }
                    SPECULAR_MAP_HASH | ARM_SPECULAR_MAP_HASH | FLOAT_Z_HASH => {
                        ("$t6_black", [0, 0, 0, 0])
                    }
                    _ => ("$t6_white", [255; 4]),
                };
                T6Texture {
                    name_hash: hash,
                    sampler_state: 0x12,
                    semantic: crate::TS_2D,
                    image: image.to_owned(),
                    texels: std::sync::Arc::new(crate::solid_texture(rgba, false)),
                }
            })
            .collect();
        let textures: Vec<&T6Texture> = textures.iter().chain(&defaults).collect();
        let mut constants = constants;
        for constant in &mut constants {
            if constant.name_hash == OCCLUSION_AMOUNT_HASH {
                constant.literal[0] *= T6_SPECULAR_SCALE;
                constant.literal[1] *= T6_SPECULAR_SCALE;
            }
        }
        for pass in selected_techniques
            .iter()
            .flat_map(|technique| &technique.passes)
        {
            for kind in [
                argument_type::MATERIAL_VERTEX_CONST,
                argument_type::MATERIAL_PIXEL_CONST,
            ] {
                let rows: BTreeSet<(u32, u32)> = pass
                    .arguments
                    .iter()
                    .filter(|a| a.kind == kind)
                    .map(|a| (u32::from(a.buffer), u32::from(a.offset) / 16))
                    .collect();
                for (buffer, row) in rows {
                    let in_row = material_arguments_in_row(&pass.arguments, kind, buffer, row);
                    let Some(hash) = packed_row_hash(&in_row) else {
                        continue;
                    };
                    if constants.iter().any(|c| c.name_hash == hash) {
                        continue;
                    }
                    let mut literal = [0.0f32; 4];
                    for a in &in_row {
                        let Some(source) = constants.iter().find(|c| c.name_hash == a.def) else {
                            continue;
                        };
                        let first = (usize::from(a.offset) % 16) / 4;
                        let count = usize::from(a.size).div_ceil(4).clamp(1, 4 - first);
                        literal[first..first + count].copy_from_slice(&source.literal[..count]);
                    }
                    constants.push(crate::MaterialConstant {
                        name_hash: hash,
                        name: *b"t6_packed\0\0\0",
                        literal,
                    });
                }
            }
        }
        material.name = AssetRef::Real(name.to_owned());
        material.namespace = crate::AssetNamespace::T6;
        material.technique_set = AssetRef::Real(technique_set.to_owned());
        material.technique_set_edge = Default::default();
        material.technique_table = None;
        material.constants = constants;
        if draw == T6Draw::Emissive {
            material.sort_key = material.sort_key.saturating_add(1);
        }
        material.state_bits = bindings.rows;
        material.state_bits_entry = Some(bindings.entries);
        material.t5_state_bits_entry = None;
        material.iw5_state_bits_entry = None;
        let namespace = material.namespace;
        material.textures = textures
            .into_iter()
            .map(|texture| {
                let texels = &texture.texels;
                let image = self.link_image(crate::AuthoredImage {
                    namespace,
                    name: AssetRef::Real(texture.image.clone()),
                    map_type: 3,
                    semantic: texture.semantic,
                    category: 0,
                    use_srgb_reads: false,
                    width: texels.width() as u16,
                    height: texels.height() as u16,
                    depth: 1,
                    level_count: texels.texture_descriptor.mip_level_count as u8,
                    format: 0,
                    payload: std::sync::Arc::new(Vec::new()),
                    decoded: Some(texels.clone()),
                    common_owned: false,
                    decoded_variant: None,
                    decoded_by: None,
                    pending_decode: None,
                });
                crate::MaterialTextureBinding {
                    name_hash: texture.name_hash,
                    name_start: 0,
                    name_end: 0,
                    sampler_state: texture.sampler_state,
                    semantic: texture.semantic,
                    image: Some(image),
                }
            })
            .collect();
        Ok(self.link_material(material))
    }
}
