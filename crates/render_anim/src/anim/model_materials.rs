use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use render_material::RuntimeMaterialCatalog;
use render_scene::{SmodelPassMaterial, TessMaterials, WorldModelLightingAtlas};

type ModelMaterialsOwner = (
    Arc<RuntimeMaterialCatalog>,
    Option<bevy::asset::AssetId<Image>>,
    Option<usize>,
    Option<u64>,
);

#[derive(Resource, Default)]
pub struct PreparedModelMaterials {
    owner: Option<ModelMaterialsOwner>,
    by_name: HashMap<String, SmodelPassMaterial>,
    by_authored: HashMap<assets::MaterialIndex, SmodelPassMaterial>,
    scene_dobjs: HashMap<String, Arc<xmodel_runtime::DObj>>,
    projectile_materials: HashMap<assets::MaterialIndex, SmodelPassMaterial>,
    projectile_dobjs: HashMap<(asset_core::AssetNamespace, String), Arc<xmodel_runtime::DObj>>,
}

impl PreparedModelMaterials {
    fn owns(&self, owner: &ModelMaterialsOwner) -> bool {
        self.owner
            .as_ref()
            .is_some_and(|(catalog, atlas, bodies, world)| {
                Arc::ptr_eq(catalog, &owner.0)
                    && *atlas == owner.1
                    && *bodies == owner.2
                    && *world == owner.3
            })
    }

    pub fn settled_for(&self, catalog: &Arc<RuntimeMaterialCatalog>) -> bool {
        self.owner
            .as_ref()
            .is_some_and(|(owned, ..)| Arc::ptr_eq(owned, catalog))
    }

    pub fn material(
        &self,
        catalog: &Arc<RuntimeMaterialCatalog>,
        name: &str,
    ) -> Option<&SmodelPassMaterial> {
        if !self.settled_for(catalog) {
            return None;
        }
        self.by_name.get(name)
    }

    pub fn authored(
        &self,
        catalog: &Arc<RuntimeMaterialCatalog>,
        authored: assets::MaterialIndex,
    ) -> Option<&SmodelPassMaterial> {
        if !self.settled_for(catalog) {
            return None;
        }
        self.by_authored.get(&authored)
    }

    pub fn scene_dobj(&self, model: &str) -> Option<&Arc<xmodel_runtime::DObj>> {
        self.scene_dobjs.get(model)
    }

    pub fn projectile_material(
        &self,
        catalog: &Arc<RuntimeMaterialCatalog>,
        authored: assets::MaterialIndex,
    ) -> Option<&SmodelPassMaterial> {
        if !self.settled_for(catalog) {
            return None;
        }
        self.projectile_materials.get(&authored)
    }

    pub fn projectile_dobj(
        &self,
        namespace: asset_core::AssetNamespace,
        model: &str,
    ) -> Option<&Arc<xmodel_runtime::DObj>> {
        self.projectile_dobjs.get(&(namespace, model.to_owned()))
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

fn admit_names<'a>(
    names: impl IntoIterator<Item = &'a str>,
    atlas: &WorldModelLightingAtlas,
    catalog: &RuntimeMaterialCatalog,
    by_name: &mut HashMap<String, SmodelPassMaterial>,
    refused: &mut Vec<String>,
) {
    for name in names {
        if by_name.contains_key(name) || refused.iter().any(|seen| seen == name) {
            continue;
        }
        match crate::body_lit_pass_material(atlas, catalog, name) {
            Some(material) => {
                by_name.insert(name.to_owned(), material);
            }
            None => refused.push(name.to_owned()),
        }
    }
}

pub fn scene_lit_pass_material(
    atlas: &WorldModelLightingAtlas,
    tess: &TessMaterials,
    authored: assets::MaterialIndex,
) -> Option<SmodelPassMaterial> {
    let world_material = tess.catalog.derived(authored)?;
    let ordinal = tess
        .catalog
        .sorted_materials
        .ordinal_for_asset_id(authored.order())?;
    let maps =
        render_scene::runtime_maps(Some(authored), &tess.catalog, tess.material_images.as_ref());
    let inv_h = lighting_iw4::model_lighting_inv_image_height(atlas.dims.image_height)?;
    let scale = lighting_iw4::model_lighting_lookup_scale(inv_h);
    Some(SmodelPassMaterial {
        model_lighting_required: true,
        color: maps.color,
        specular: maps.specular,
        probe: None,
        atlas: Some(atlas.image.clone()),
        alpha_mode: maps.alpha_mode,
        draw_mode: maps.draw_mode,
        cull_mode: maps.cull_mode,
        env_map_parms: maps.env_map_parms,
        lighting_lookup_scale: [scale.u, scale.v, scale.w, scale.q],
        atlas_lookup: [
            lighting_iw4::MODEL_LIGHTING_INV_ATLAS_WIDTH as f32,
            inv_h,
            lighting_iw4::MODEL_LIGHTING_VOLUME_W,
            0.0,
        ],
        sort_key: world_material.sort_key,
        material_sorted_index: Some(ordinal.get()),
    })
}

fn present_names<'a>(
    keys: &'a [Option<asset_core::MaterialKey>],
    edges: &'a [assets::AssetEdge<assets::MaterialSpace>],
) -> impl Iterator<Item = &'a str> {
    keys.iter()
        .zip(edges)
        .filter(|(_, edge)| edge.is_bound())
        .filter_map(|(key, _)| Some(key.as_ref()?.name.as_str()))
}

pub fn prepare_model_materials(
    tess: Res<TessMaterials>,
    atlas: Option<Res<WorldModelLightingAtlas>>,
    bodies: Option<Res<assets::PreparedBodies>>,
    world_weapons: Option<Res<assets::PreparedWorldWeapons>>,
    scene_models: Option<Res<asset_world::MapXModelSceneCatalog>>,
    projectiles: Option<Res<assets::PreparedProjectileMeshes>>,
    mut prepared: ResMut<PreparedModelMaterials>,
) {
    if tess.material_images.is_empty() {
        return;
    }
    let owner: ModelMaterialsOwner = (
        Arc::clone(&tess.catalog),
        atlas.as_ref().map(|atlas| atlas.image.id()),
        bodies
            .as_ref()
            .map(|bodies| Arc::as_ptr(&bodies.0) as usize),
        world_weapons.as_ref().map(|world| world.0.identity()),
    );
    if prepared.owns(&owner) {
        return;
    }
    let (Some(atlas), Some(bodies), Some(world)) = (atlas, bodies, world_weapons) else {
        diag::info!(
            World,
            "model materials: nothing to admit (lighting atlas or model catalogs missing)"
        );
        *prepared = PreparedModelMaterials {
            owner: Some(owner),
            ..Default::default()
        };
        return;
    };
    let started = std::time::Instant::now();
    let mut by_name = HashMap::new();
    let mut refused = Vec::new();
    for model in [
        assets::bot_model::local_bot_model(),
        assets::bot_model::local_skate_board(),
    ]
    .into_iter()
    .flatten()
    .chain(
        assets::jak_model::local_jak()
            .into_iter()
            .flat_map(|jak| jak.meshes()),
    ) {
        admit_names(
            model.surfaces.iter().map(|s| s.material.as_str()),
            &atlas,
            &tess.catalog,
            &mut by_name,
            &mut refused,
        );
    }
    for name in bodies.0.names() {
        if let Some(entry) = bodies.0.get(name) {
            admit_names(
                present_names(&entry.material_keys, &entry.material_edges),
                &atlas,
                &tess.catalog,
                &mut by_name,
                &mut refused,
            );
        }
    }
    for index in 0..world.0.len() {
        if let Some(entry) = world.0.get_at(index) {
            admit_names(
                present_names(&entry.material_keys, &entry.material_edges),
                &atlas,
                &tess.catalog,
                &mut by_name,
                &mut refused,
            );
        }
    }
    let mut by_authored = HashMap::new();
    let mut scene_dobjs = HashMap::new();
    if let Some(scene_models) = scene_models.as_deref() {
        for (key, asset) in scene_models.iter() {
            let skel = match asset {
                asset_world::MapXModelSceneAsset::Iw4(skel)
                | asset_world::MapXModelSceneAsset::Iw5(skel)
                | asset_world::MapXModelSceneAsset::T5(skel) => skel,
                asset_world::MapXModelSceneAsset::Unavailable { .. } => continue,
            };
            for surface in 0..skel.surface_materials.len() {
                let Some(authored) = scene_models.surface_material(key, surface) else {
                    continue;
                };
                if by_authored.contains_key(&authored) {
                    continue;
                }
                if let Some(material) = scene_lit_pass_material(&atlas, &tess, authored) {
                    by_authored.insert(authored, material);
                }
            }
            if let Some(pose) = skel.pose.as_ref()
                && let Ok(dobj) = xmodel_runtime::DObj::build(&[(pose, None)])
            {
                scene_dobjs.insert(key.0.clone(), Arc::new(dobj));
            }
        }
    }
    let mut projectile_materials = HashMap::new();
    let mut projectile_dobjs = HashMap::new();
    if let Some(projectiles) = projectiles.as_deref() {
        for index in 0..projectiles.0.len() {
            let Some(entry) = projectiles.0.get_at(index) else {
                continue;
            };
            let key =
                projectiles
                    .0
                    .key_at(index)
                    .cloned()
                    .unwrap_or(asset_model::ProjectileMeshKey::new(
                        entry.namespace,
                        &entry.skel.name,
                    ));
            for surface in 0..entry.material_edges.len() {
                let Some(authored) = entry.material_index(surface) else {
                    continue;
                };
                if projectile_materials.contains_key(&authored) {
                    continue;
                }
                if let Some(material) = crate::authored_lit_xmodel_pass_material(
                    &atlas,
                    &tess.catalog,
                    tess.material_images.as_ref(),
                    authored,
                ) {
                    projectile_materials.insert(authored, material);
                }
            }
            if let Some(pose) = entry.skel.pose.as_ref()
                && let Ok(dobj) = xmodel_runtime::DObj::build(&[(pose, None)])
            {
                projectile_dobjs.insert((key.namespace, key.name), Arc::new(dobj));
            }
        }
    }
    diag::info!(
        World,
        "model materials prepared before Ready: admitted={} map_model_materials={} map_model_dobjs={} projectile_materials={} projectile_dobjs={} refused={} elapsed={:.1}ms{}",
        by_name.len(),
        by_authored.len(),
        scene_dobjs.len(),
        projectile_materials.len(),
        projectile_dobjs.len(),
        refused.len(),
        started.elapsed().as_secs_f64() * 1000.0,
        if refused.is_empty() {
            String::new()
        } else {
            format!(
                " first_refused=[{}]",
                refused
                    .iter()
                    .take(8)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
    );
    *prepared = PreparedModelMaterials {
        owner: Some(owner),
        by_name,
        by_authored,
        scene_dobjs,
        projectile_materials,
        projectile_dobjs,
    };
}
