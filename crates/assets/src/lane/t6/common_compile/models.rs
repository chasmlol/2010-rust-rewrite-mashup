use crate::lane::CommonDependencyRefusal;
use asset_material::MaterialCatalog;
use asset_model::{FpvMeshBuild, WorldWeaponBuild};

pub(super) fn bind_native_materials(
    path: &std::path::Path,
    captures: &std::collections::BTreeMap<String, super::super::T6MaterialCapture>,
    techsets: &std::collections::BTreeMap<String, asset_material::t6_techset::T6TechniqueSet>,
    materials: &mut MaterialCatalog,
    refusals: &mut Vec<CommonDependencyRefusal>,
) -> std::collections::BTreeMap<String, usize> {
    let mut bound = std::collections::BTreeMap::new();
    for (name, capture) in captures {
        let result = (|| {
            let native = capture
                .native
                .as_ref()
                .ok_or("native technique capture missing")?;
            let set = techsets
                .get(&native.technique_set)
                .ok_or("native technique set missing")?;
            let draw = if native
                .state
                .first_bits(asset_material::t6_techset::T6_TECHNIQUE_LIT)
                .is_none()
                && native
                    .state
                    .first_bits(asset_material::t6_techset::T6_TECHNIQUE_EMISSIVE)
                    .is_some()
            {
                asset_material::t6_techset::T6Draw::Emissive
            } else {
                asset_material::t6_techset::T6Draw::Lit
            };
            let mut report = Vec::new();
            materials.link_t6_technique_set(set, draw, &mut report);
            let definition = super::super::native_material_definition(
                path,
                name,
                &native.header,
                &set.name,
                false,
            )?;
            let seed = materials.link_material(definition);
            materials
                .t6_material(
                    seed,
                    name,
                    set,
                    &native.textures,
                    native.constants.clone(),
                    &native.state,
                    draw,
                    &mut report,
                )
                .map_err(|e| format!("{e:?}"))
        })();
        match result {
            Ok(index) => {
                bound.insert(name.clone(), index);
            }
            Err(reason) => refusals.push(CommonDependencyRefusal::NativeMaterial {
                family: asset_core::FamilyId::T6,
                name: name.clone(),
                reason,
            }),
        }
    }
    bound
}

pub(super) fn bind_t6_content(
    content: super::super::T6Content,
    materials: &mut MaterialCatalog,
    fpv: &mut FpvMeshBuild,
    world: &mut WorldWeaponBuild,
    refusals: &mut Vec<CommonDependencyRefusal>,
) -> String {
    let bound = bind_native_materials(
        &content.path,
        &content.materials,
        &content.techsets,
        materials,
        refusals,
    );
    let count = content.models.len();
    for mut model in content.models {
        model.skel.surface_materials = model
            .surface_materials
            .iter()
            .map(|name| {
                name.as_ref()
                    .and_then(|name| bound.get(name))
                    .copied()
                    .map(asset_core::WalkLocalMaterialIndex::from_walk)
            })
            .collect();
        if model.view {
            fpv.insert_in(asset_core::FamilyId::T6, model.skel, Some(materials));
        } else {
            world.insert_in(asset_core::FamilyId::T6, model.skel, Some(materials));
        }
    }
    materials.resolve_technique_set_edges();
    format!(
        "t6 native content: {count} models, {} materials",
        bound.len()
    )
}
