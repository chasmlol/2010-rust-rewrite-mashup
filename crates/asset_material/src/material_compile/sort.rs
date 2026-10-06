use core::cmp::Ordering;
use render_material::{CatalogBuildError, MaterialAssetId, SortedMaterialOrdinal};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
struct ComparatorPassKey {
    pixel_shader_name: String,
    vertex_shader_name: String,
    code_pixel_constants: Vec<u16>,
    pixel_constants: Vec<(u16, [u32; 4])>,
}

#[derive(Clone, Debug)]
struct ComparatorRecord {
    asset_id: MaterialAssetId,
    name: String,
    technique_set_name: String,
    sort_key: u8,
    info_game_flags: u8,
    state_flags: u8,
    prepass: u8,
    slot5: Option<ComparatorPassKey>,
    slot9: Option<ComparatorPassKey>,
}

fn shader_name<'a>(
    source: &'a crate::MaterialDefinitions,
    material: MaterialAssetId,
    slot: u8,
    reference: &crate::OwnedShaderRef,
) -> Result<&'a str, CatalogBuildError> {
    reference
        .shader
        .and_then(|index| source.shaders.get(index))
        .map(|shader| shader.name.as_str())
        .ok_or(CatalogBuildError::ShaderIdentityMissing { material, slot })
}

fn comparator_pass_key(
    source: &crate::MaterialDefinitions,
    material: &crate::AuthoredMaterial,
    asset_id: MaterialAssetId,
    slot: u8,
    technique: &crate::OwnedTechnique,
) -> Result<ComparatorPassKey, CatalogBuildError> {
    let pass = technique
        .passes
        .first()
        .ok_or(CatalogBuildError::TechniqueBodyMissing {
            material: asset_id,
            slot,
        })?;
    let stable_start =
        usize::from(pass.per_prim_arg_count).saturating_add(usize::from(pass.per_obj_arg_count));
    let stable_end = stable_start.saturating_add(usize::from(pass.stable_arg_count));
    let stable = pass.arguments.get(stable_start..stable_end).ok_or(
        CatalogBuildError::TechniqueBodyMissing {
            material: asset_id,
            slot,
        },
    )?;

    let argument_type = |argument: &crate::OwnedShaderArgument| match argument {
        crate::OwnedShaderArgument::MaterialVertexConstant { .. } => 0,
        crate::OwnedShaderArgument::LiteralVertexConstant { .. } => 1,
        crate::OwnedShaderArgument::MaterialPixelSampler { .. } => 2,
        crate::OwnedShaderArgument::CodeVertexConstant { .. } => 3,
        crate::OwnedShaderArgument::CodePixelSampler { .. } => 4,
        crate::OwnedShaderArgument::CodePixelConstant { .. } => 5,
        crate::OwnedShaderArgument::MaterialPixelConstant { .. } => 6,
        crate::OwnedShaderArgument::LiteralPixelConstant { .. } => 7,
        crate::OwnedShaderArgument::Unknown { argument_type, .. } => *argument_type,
    };
    let mut cursor = stable
        .iter()
        .position(|argument| argument_type(argument) >= 5)
        .unwrap_or(stable.len());
    let mut code_pixel_constants = Vec::new();
    while let Some(crate::OwnedShaderArgument::CodePixelConstant { index, .. }) = stable.get(cursor)
    {
        code_pixel_constants.push(*index);
        cursor += 1;
    }

    cursor = stable
        .iter()
        .position(|argument| argument_type(argument) >= 6)
        .unwrap_or(stable.len());
    let mut pixel_constants = Vec::new();
    while let Some(crate::OwnedShaderArgument::MaterialPixelConstant {
        destination,
        name_hash,
    }) = stable.get(cursor)
    {
        let words = match material
            .constants
            .iter()
            .find(|constant| constant.name_hash == *name_hash)
            .map(|constant| constant.literal.map(f32::to_bits))
        {
            Some(words) => words,
            None if leftover_x_token_aliased(source, material) => {
                cursor += 1;
                continue;
            }
            None => {
                return Err(CatalogBuildError::MaterialConstantMissing {
                    material: asset_id,
                    name_hash: *name_hash,
                });
            }
        };
        pixel_constants.push((*destination, words));
        cursor += 1;
    }
    while let Some(crate::OwnedShaderArgument::LiteralPixelConstant { destination, words }) =
        stable.get(cursor)
    {
        pixel_constants.push((
            *destination,
            words.ok_or(CatalogBuildError::LiteralPixelConstantMissing {
                material: asset_id,
                slot,
            })?,
        ));
        cursor += 1;
    }
    pixel_constants.sort_by_key(|(destination, _)| *destination);

    Ok(ComparatorPassKey {
        pixel_shader_name: shader_name(source, asset_id, slot, &pass.pixel_shader)?.to_owned(),
        vertex_shader_name: shader_name(source, asset_id, slot, &pass.vertex_shader)?.to_owned(),
        code_pixel_constants,
        pixel_constants,
    })
}

fn leftover_x_token_aliased(
    source: &crate::MaterialDefinitions,
    material: &crate::AuthoredMaterial,
) -> bool {
    if material.namespace != asset_core::AssetNamespace::T5 {
        return false;
    }
    let want = asset_core::AssetRef::bare_name(material.technique_set.as_str());
    let stripped = crate::t5_feature_token_stripped(want);
    stripped != want
        && source.technique_set_facts().iter().any(|facts| {
            facts.name.is_real() && facts.name.as_str() == stripped && facts.table.is_some()
        })
}

fn comparator_record(
    source: &crate::MaterialDefinitions,
    index: usize,
) -> Result<ComparatorRecord, CatalogBuildError> {
    let asset_id = MaterialAssetId(
        u16::try_from(index).map_err(|_| CatalogBuildError::MaterialAssetIdOverflow { index })?,
    );
    let material = &source.materials[index];
    let key = crate::TechsetKey::new(material.namespace, material.technique_set.as_str());
    let facts = match source.resolve_technique_set(key) {
        crate::TechsetResolve::Hit { facts, .. } => facts,
        crate::TechsetResolve::GraphMissing { .. } => {
            return Err(CatalogBuildError::TechniqueGraphMissing { material: asset_id });
        }
        crate::TechsetResolve::Foreign { got, .. } => {
            return Err(CatalogBuildError::TechniqueSetNamespaceMismatch {
                material: asset_id,
                want: material.namespace,
                got,
            });
        }
        crate::TechsetResolve::Missing => {
            return Err(CatalogBuildError::TechniqueSetMissing { material: asset_id });
        }
    };
    let table = facts
        .table
        .as_ref()
        .ok_or(CatalogBuildError::TechniqueGraphMissing { material: asset_id })?;
    let graph = table
        .graph
        .as_ref()
        .ok_or(CatalogBuildError::TechniqueGraphMissing { material: asset_id })?;
    let pass_key = |slot: usize| -> Result<Option<ComparatorPassKey>, CatalogBuildError> {
        if table.slots & (1 << slot) == 0 {
            return Ok(None);
        }
        let technique = graph.slots.get(slot).and_then(Option::as_ref).ok_or(
            CatalogBuildError::TechniqueBodyMissing {
                material: asset_id,
                slot: slot as u8,
            },
        )?;
        comparator_pass_key(source, material, asset_id, slot as u8, technique).map(Some)
    };
    Ok(ComparatorRecord {
        asset_id,
        name: material.name.to_string(),
        technique_set_name: facts.name.to_string(),
        sort_key: material.sort_key,
        info_game_flags: material.info_game_flags,
        state_flags: material.state_flags,
        prepass: dpvs_iw4::material_prepass(
            table.slots & 1 == 0,
            table.slots & 2 != 0,
            material.state_flags,
            table.technique0_flags,
        ),
        slot5: pass_key(5)?,
        slot9: pass_key(9)?,
    })
}

fn compare_float_words(
    a: [u32; 4],
    b: [u32; 4],
    material: MaterialAssetId,
    slot: u8,
) -> Result<Ordering, CatalogBuildError> {
    for (a, b) in a.into_iter().zip(b) {
        let (a, b) = (f32::from_bits(a), f32::from_bits(b));
        let order = a
            .partial_cmp(&b)
            .ok_or(CatalogBuildError::NonFiniteComparatorConstant { material, slot })?;
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    Ok(Ordering::Equal)
}

fn compare_pass_args(
    a: &ComparatorPassKey,
    b: &ComparatorPassKey,
    material: MaterialAssetId,
    slot: u8,
) -> Result<Ordering, CatalogBuildError> {
    let order = a.code_pixel_constants.cmp(&b.code_pixel_constants);
    if order != Ordering::Equal {
        return Ok(order);
    }
    let order = a.pixel_constants.len().cmp(&b.pixel_constants.len());
    if order != Ordering::Equal {
        return Ok(order);
    }
    for ((a_destination, a_words), (b_destination, b_words)) in
        a.pixel_constants.iter().zip(&b.pixel_constants)
    {
        let order = a_destination.cmp(b_destination);
        if order != Ordering::Equal {
            return Ok(order);
        }
        let order = compare_float_words(*a_words, *b_words, material, slot)?;
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    Ok(Ordering::Equal)
}

fn compare_sorted_materials(
    a: &ComparatorRecord,
    b: &ComparatorRecord,
    slot_gap: &mut u32,
) -> Result<Ordering, CatalogBuildError> {
    let order = a.sort_key.cmp(&b.sort_key);
    if order != Ordering::Equal {
        return Ok(order);
    }
    let order = if a.slot9.is_some() {
        ((b.info_game_flags >> 1) & 1).cmp(&((a.info_game_flags >> 1) & 1))
    } else {
        u8::from(b.slot5.is_some()).cmp(&u8::from(a.slot5.is_some()))
    };
    if order != Ordering::Equal {
        return Ok(order);
    }
    let order = a.prepass.cmp(&b.prepass);
    if order != Ordering::Equal {
        return Ok(order);
    }
    let order = ((b.state_flags >> 3) & 1).cmp(&((a.state_flags >> 3) & 1));
    if order != Ordering::Equal {
        return Ok(order);
    }

    let mut compare_slot = |slot: u8,
                            a_pass: Option<&ComparatorPassKey>,
                            b_pass: Option<&ComparatorPassKey>,
                            compare_args: bool|
     -> Result<Ordering, CatalogBuildError> {
        let (Some(a_pass), Some(b_pass)) = (a_pass, b_pass) else {
            if a_pass.is_none() && b_pass.is_none() {
                return Ok(Ordering::Equal);
            }
            *slot_gap = slot_gap.saturating_add(1);
            if *slot_gap == 1 {
                diag::info!(
                    World,
                    "drawsurf sorted materials: slot {slot} occupancy gap \
                     {} vs {} — present-first (not ComparatorInvariant)",
                    a.name,
                    b.name
                );
            }
            return Ok(u8::from(b_pass.is_some()).cmp(&u8::from(a_pass.is_some())));
        };
        let order = a_pass.pixel_shader_name.cmp(&b_pass.pixel_shader_name);
        if order != Ordering::Equal {
            return Ok(order);
        }
        if compare_args {
            let order = compare_pass_args(a_pass, b_pass, a.asset_id, slot)?;
            if order != Ordering::Equal {
                return Ok(order);
            }
        }
        Ok(a_pass.vertex_shader_name.cmp(&b_pass.vertex_shader_name))
    };

    if a.slot9.is_some() {
        let order = compare_slot(
            9,
            a.slot9.as_ref(),
            b.slot9.as_ref(),
            a.state_flags & 8 != 0,
        )?;
        if order != Ordering::Equal {
            return Ok(order);
        }
    } else if b.slot9.is_some() {
        let order = compare_slot(5, a.slot5.as_ref(), b.slot5.as_ref(), true)?;
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    let order = a.technique_set_name.cmp(&b.technique_set_name);
    if order != Ordering::Equal {
        return Ok(order);
    }
    Ok(a.name.cmp(&b.name))
}

fn comparator_skippable(cause: &CatalogBuildError) -> bool {
    matches!(
        cause,
        CatalogBuildError::TechniqueSetMissing { .. }
            | CatalogBuildError::TechniqueSetNamespaceMismatch { .. }
            | CatalogBuildError::TechniqueGraphMissing { .. }
            | CatalogBuildError::TechniqueBodyMissing { .. }
            | CatalogBuildError::ShaderIdentityMissing { .. }
            | CatalogBuildError::MaterialConstantMissing { .. }
            | CatalogBuildError::LiteralPixelConstantMissing { .. }
    )
}

fn skip_cause_key(cause: &CatalogBuildError) -> &'static str {
    match cause {
        CatalogBuildError::TechniqueSetMissing { .. } => "TechniqueSetMissing",
        CatalogBuildError::TechniqueSetNamespaceMismatch { .. } => "TechniqueSetNamespaceMismatch",
        CatalogBuildError::TechniqueGraphMissing { .. } => "TechniqueGraphMissing",
        CatalogBuildError::TechniqueBodyMissing { .. } => "TechniqueBodyMissing",
        CatalogBuildError::ShaderIdentityMissing { .. } => "ShaderIdentityMissing",
        CatalogBuildError::MaterialConstantMissing { .. } => "MaterialConstantMissing",
        CatalogBuildError::LiteralPixelConstantMissing { .. } => "LiteralPixelConstantMissing",
        CatalogBuildError::SortedMaterialCapacity { .. } => "SortedMaterialCapacity",
        _ => "other",
    }
}

fn leftover_t5_common_yields_capacity(material: &crate::AuthoredMaterial) -> bool {
    leftover_t5_common_yields_capacity_parts(
        material.t5_state_bits_entry.is_some(),
        material.zone.as_str(),
    )
}

fn leftover_t5_common_yields_capacity_parts(has_t5_state_bits: bool, zone: &str) -> bool {
    has_t5_state_bits && zone == asset_core::ZoneOwner::COMMON_MP.as_str()
}

fn leftover_iw5_common_yields_capacity(material: &crate::AuthoredMaterial) -> bool {
    leftover_iw5_common_yields_capacity_parts(
        material.iw5_state_bits_entry.is_some(),
        material.zone.as_str(),
    )
}

fn leftover_iw5_common_yields_capacity_parts(has_iw5_state_bits: bool, zone: &str) -> bool {
    has_iw5_state_bits && zone == asset_core::ZoneOwner::COMMON_MP.as_str()
}

fn leftover_t6_yields_capacity(material: &crate::AuthoredMaterial) -> bool {
    material.namespace == asset_core::AssetNamespace::T6
}

fn is_foreign_common_leftover(material: &crate::AuthoredMaterial) -> bool {
    leftover_t5_common_yields_capacity(material)
        || leftover_iw5_common_yields_capacity(material)
        || leftover_t6_yields_capacity(material)
}

fn insertion_sort_by_comparator(
    records: &mut [ComparatorRecord],
    slot_gap_n: &mut u32,
) -> Result<(), CatalogBuildError> {
    for index in 1..records.len() {
        let mut cursor = index;
        while cursor != 0
            && compare_sorted_materials(&records[cursor], &records[cursor - 1], slot_gap_n)?
                == Ordering::Less
        {
            records.swap(cursor, cursor - 1);
            cursor -= 1;
        }
    }
    Ok(())
}

fn append_foreign_leftover(
    prefix: Vec<ComparatorRecord>,
    foreign_leftover: Vec<ComparatorRecord>,
) -> Result<Vec<ComparatorRecord>, CatalogBuildError> {
    if prefix.len() > SortedMaterialOrdinal::RETAIL_LIMIT {
        return Err(CatalogBuildError::SortedMaterialCapacity {
            count: prefix.len(),
            capacity: SortedMaterialOrdinal::RETAIL_LIMIT,
        });
    }
    let mut keep = prefix;
    keep.extend(foreign_leftover);
    if keep.len() > SortedMaterialOrdinal::LIMIT {
        return Err(CatalogBuildError::SortedMaterialCapacity {
            count: keep.len(),
            capacity: SortedMaterialOrdinal::LIMIT,
        });
    }
    Ok(keep)
}

fn skip_tech_label(counts: &BTreeMap<String, u32>) -> Option<String> {
    if counts.is_empty() {
        return None;
    }
    let mut ranked: Vec<(&str, u32)> = counts
        .iter()
        .map(|(name, count)| (name.as_str(), *count))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    Some(
        ranked
            .into_iter()
            .take(6)
            .map(|(name, count)| format!("{name}:{count}"))
            .collect::<Vec<_>>()
            .join(","),
    )
}

pub(super) fn build_sorted_material_table(
    source: &crate::MaterialDefinitions,
) -> Result<
    (
        Vec<MaterialAssetId>,
        Vec<Option<SortedMaterialOrdinal>>,
        u32,
        u32,
        Option<String>,
        Option<String>,
        Option<String>,
    ),
    CatalogBuildError,
> {
    let mut records = Vec::new();
    let mut skipped_n = 0u32;
    let mut slot_gap_n = 0u32;
    let mut first_skip: Option<(usize, CatalogBuildError)> = None;
    let mut skip_tech_counts = BTreeMap::<String, u32>::new();
    let mut skip_cause_counts = BTreeMap::<String, u32>::new();
    let mut first_x0_skip: Option<String> = None;
    for index in 0..source.materials.len() {
        match comparator_record(source, index) {
            Ok(record) => records.push(record),
            Err(cause) if comparator_skippable(&cause) => {
                let tech = source.materials[index].technique_set.as_str();
                *skip_tech_counts.entry(tech.to_owned()).or_default() += 1;
                *skip_cause_counts
                    .entry(skip_cause_key(&cause).to_owned())
                    .or_default() += 1;
                if first_x0_skip.is_none() && tech.contains("x0") {
                    first_x0_skip = Some(format!(
                        "{cause:?} {} tech={tech}",
                        source.materials[index].name
                    ));
                }
                if first_skip.is_none() {
                    first_skip = Some((index, cause));
                }
                skipped_n = skipped_n.saturating_add(1);
            }
            Err(cause) => return Err(cause),
        }
    }
    let mut iw4_records = Vec::new();
    let mut leftover_records = Vec::new();
    for record in records {
        let leftover = source
            .materials
            .get(usize::from(record.asset_id.0))
            .expect("comparator record asset id is a catalog slot");
        if is_foreign_common_leftover(leftover) {
            leftover_records.push(record);
        } else {
            iw4_records.push(record);
        }
    }
    insertion_sort_by_comparator(&mut iw4_records, &mut slot_gap_n)?;
    insertion_sort_by_comparator(&mut leftover_records, &mut slot_gap_n)?;
    let records = append_foreign_leftover(iw4_records, leftover_records)?;
    let sorted_first_skip = first_skip.as_ref().map(|(index, cause)| {
        let name = source
            .materials
            .get(*index)
            .map(|material| material.name.as_str())
            .unwrap_or("<out-of-range>");
        format!("{cause:?} {name}")
    });
    let skip_tech = skip_tech_label(&skip_tech_counts);
    let skip_cause = skip_tech_label(&skip_cause_counts);
    if sorted_first_skip.is_some() {
        diag::info!(
            World,
            "drawsurf sorted materials: skipped={skipped_n} ranked={} first_skip={} skip_cause={} skip_tech={} first_x0={}",
            records.len(),
            sorted_first_skip.as_deref().unwrap_or("-"),
            skip_cause.as_deref().unwrap_or("-"),
            skip_tech.as_deref().unwrap_or("-"),
            first_x0_skip.as_deref().unwrap_or("-"),
        );
    }
    let asset_ids_by_ordinal = records
        .iter()
        .map(|record| record.asset_id)
        .collect::<Vec<_>>();
    let mut ordinals_by_asset_id = vec![None; source.materials.len()];
    for (ordinal, record) in records.iter().enumerate() {
        ordinals_by_asset_id[usize::from(record.asset_id.0)] = Some(SortedMaterialOrdinal::new(
            u32::try_from(ordinal).map_err(|_| CatalogBuildError::SortedMaterialCapacity {
                count: records.len(),
                capacity: SortedMaterialOrdinal::LIMIT,
            })?,
        )?);
    }
    Ok((
        asset_ids_by_ordinal,
        ordinals_by_asset_id,
        skipped_n,
        slot_gap_n,
        sorted_first_skip,
        skip_tech,
        skip_cause,
    ))
}
