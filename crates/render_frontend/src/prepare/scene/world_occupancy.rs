use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::render::render_resource::TextureUsages;
use bevy::render::view::Msaa;

use crate::assemble::drawsurf::RuntimeLightmapHandles;
use crate::prepare::scene::camera::{FlyCamera, FpvLens, transform_from_iw_view};
use crate::prepare::scene::world::{SMODEL_LIGHTING_MAX_CLIENT_VIEWS, WorldScene};
use hud_iw4::{CG_FOV_DEFAULT, horizontal_to_vertical_fov_deg};

pub(crate) const CAMERA_NEAR_INCHES: f32 = 2.0;

pub(crate) fn camera_far_inches(radius: f32) -> f32 {
    (radius * 8.0).max(CAMERA_NEAR_INCHES * 100.0)
}

pub fn place(
    commands: &mut Commands,
    scene: &mut WorldScene,
    images: &mut Assets<Image>,
    exact_material_handles: Vec<Option<Handle<Image>>>,
    reflection_probe_handles: Vec<Option<Handle<Image>>>,
    lightmap_handles: Vec<Option<RuntimeLightmapHandles>>,
) {
    let _batches = std::mem::take(&mut scene.batches);

    let static_model_instances = std::mem::take(&mut scene.static_model_instances);
    let map_xmodel_scene_assets = std::mem::take(&mut scene.map_xmodel_scene_assets);
    let script_model_instances = std::mem::take(&mut scene.script_model_instances);
    let dyn_ent_instances = std::mem::take(&mut scene.dyn_ent_instances);
    let dyn_ent_brush_n = scene.dyn_ent_brush_n;
    let model_lighting_atlas = if scene.light_grid.is_some() {
        lighting_iw4::model_lighting_atlas_dims(SMODEL_LIGHTING_MAX_CLIENT_VIEWS).map(|dims| {
            crate::prepare::scene::model_lighting_atlas::WorldModelLightingAtlas::new(images, dims)
        })
    } else {
        None
    };
    if !scene.static_model_meshes.is_empty() {
        for model in &scene.static_model_meshes {
            diag::info!(
                World,
                "static model mesh: {} ({} surfaces)",
                model.name,
                model.lod_surfaces.iter().map(|l| l.len()).sum::<usize>()
            );
        }
        scene.smodel_mesh_names = scene
            .static_model_meshes
            .iter()
            .map(|model| model.name.clone())
            .collect();
        let mesh_authored: Vec<Vec<Option<assets::MaterialIndex>>> = scene
            .static_model_meshes
            .iter()
            .map(|model| model.surface_materials().collect())
            .collect();

        let slot_count = static_model_instances.len();
        let mut smodel_lighting = model_lighting_atlas.as_ref().and_then(|atlas| {
            crate::prepare::scene::smodel_lighting::prepare_smodel_lighting(
                atlas.dims,
                &scene.smodel_lighting_samples,
                slot_count,
            )
        });

        let mut entities = vec![None; slot_count];
        let mut cull_dists = vec![0; slot_count];
        let mut unresolved_slots = 0usize;
        let mut lit_slots = 0usize;
        let mut sample_without_technique = 0usize;
        let mut technique_without_sample = 0usize;

        for (index, instance) in static_model_instances.iter().enumerate() {
            let Some(instance) = *instance else {
                unresolved_slots += 1;
                continue;
            };
            let Some(authored_row) = mesh_authored.get(instance.mesh) else {
                unresolved_slots += 1;
                continue;
            };
            if authored_row.is_empty() {
                unresolved_slots += 1;
                continue;
            }
            let wants_atlas = crate::prepare::scene::smodel_lighting::surfaces_take_model_lighting(
                authored_row.iter().copied(),
                &scene.runtime_material_catalog,
            );
            let has_sample = smodel_lighting.as_ref().is_some_and(|l| {
                crate::prepare::scene::smodel_lighting::slot_has_lighting_sample(l, index)
            });
            if has_sample && !wants_atlas {
                sample_without_technique += 1;
            }
            if wants_atlas && !has_sample {
                technique_without_sample += 1;
            }
            let lit = smodel_lighting.as_ref().is_some_and(|l| {
                crate::prepare::scene::smodel_lighting::slot_is_lit_candidate(l, index, wants_atlas)
            });

            let parent = commands
                .spawn((
                    instance.transform,
                    Visibility::Hidden,
                    crate::prepare::scene::cull::StaticModelEntity,
                ))
                .id();

            if lit {
                let Some(lighting) = smodel_lighting.as_mut() else {
                    unresolved_slots += 1;
                    commands.entity(parent).despawn();
                    continue;
                };
                lighting.parents[index] = Some(parent);
                lit_slots += 1;
            }

            entities[index] = Some(parent);
            cull_dists[index] = instance.cull_dist;
        }

        if let Some(mut lighting) = smodel_lighting {
            lighting.spawn_lit_n = lit_slots as u32;
            lighting.spawn_sample_without_technique = sample_without_technique as u32;
            diag::info!(
                World,
                "smodel lighting: atlas={}x{}x4 entry_limit={} resolved_samples={} \
                 lit_candidate_slots={} (technique+sample; sample_without_technique={} \
                 technique_without_sample={}; retained path — handled in immediates, no Mesh3d)",
                lighting_iw4::MODEL_LIGHTING_ATLAS_WIDTH,
                model_lighting_atlas
                    .as_ref()
                    .map(|atlas| atlas.dims.image_height)
                    .unwrap_or(0),
                model_lighting_atlas
                    .as_ref()
                    .map(|atlas| atlas.dims.smodel_entry_limit)
                    .unwrap_or(0),
                scene.smodel_lighting_samples.len(),
                lit_slots,
                sample_without_technique,
                technique_without_sample,
            );
            commands.insert_resource(lighting);
        } else if !scene.smodel_lighting_samples.is_empty() {
            let dims = lighting_iw4::model_lighting_atlas_dims(SMODEL_LIGHTING_MAX_CLIENT_VIEWS);
            diag::info!(
                World,
                "smodel lighting samples: resolved={} concurrent_entry_limit={:?} \
                 (IW4L_SMODEL_LIGHTING disabled — unlit gap)",
                scene.smodel_lighting_samples.len(),
                dims.map(|d| d.smodel_entry_limit)
            );
        }

        commands.insert_resource(
            crate::prepare::scene::smodel_geom_cache::WorldStaticModelCache::new(slot_count),
        );

        diag::info!(
            World,
            "static models: {} authored slots ({} unresolved); tess left for assemble",
            slot_count,
            unresolved_slots,
        );
        if let Some(cull) = scene.cull.as_mut() {
            cull.static_model_entities = entities;
            cull.static_model_cull_dists = cull_dists;
            cull.smodel_vis.resize(slot_count, 0);
        }
    }

    scene.static_model_instances = static_model_instances;

    let mut script_ready = 0usize;
    let mut script_unavailable = 0usize;
    let mut script_gameobject_tagged = 0usize;
    for instance in script_model_instances {
        match map_xmodel_scene_assets.get(&instance.current_model) {
            Some(asset_world::MapXModelSceneAsset::Iw4(_))
            | Some(
                asset_world::MapXModelSceneAsset::Iw5(_) | asset_world::MapXModelSceneAsset::T6(_),
            )
            | Some(asset_world::MapXModelSceneAsset::T5(_)) => script_ready += 1,
            Some(asset_world::MapXModelSceneAsset::Unavailable { .. }) | None => {
                script_unavailable += 1
            }
        }

        if !instance.metadata.gameobject.is_empty() {
            script_gameobject_tagged += 1;
        }
        let transform = instance.transform;
        let gameobject = instance.metadata.gameobject.clone();
        let visibility = Visibility::Inherited;
        let mut entity = commands.spawn((
            transform,
            visibility,
            crate::prepare::scene::cull::ScriptModelEntity,
            instance,
        ));
        if !gameobject.is_empty() {
            entity.insert(crate::prepare::scene::cull::ScriptModelGameObject(
                gameobject,
            ));
        }
    }
    diag::info!(
        World,
        "script models: ready={} unavailable={} gameobject-tagged={} script_smodel_placements=0",
        script_ready,
        script_unavailable,
        script_gameobject_tagged,
    );

    let mut dyn_ready = 0usize;
    let mut dyn_unavailable = 0usize;
    for instance in dyn_ent_instances {
        match map_xmodel_scene_assets.get(&instance.current_model) {
            Some(asset_world::MapXModelSceneAsset::Iw4(_))
            | Some(
                asset_world::MapXModelSceneAsset::Iw5(_) | asset_world::MapXModelSceneAsset::T6(_),
            )
            | Some(asset_world::MapXModelSceneAsset::T5(_)) => dyn_ready += 1,
            Some(asset_world::MapXModelSceneAsset::Unavailable { .. }) | None => {
                dyn_unavailable += 1
            }
        }
        let transform = instance.transform;
        commands.spawn((
            transform,
            Visibility::Inherited,
            crate::prepare::scene::cull::DynEntModelEntity,
            instance,
        ));
    }
    diag::info!(
        World,
        "dynents: spawned_ready={} catalog_miss={} brushes_undrawn={}",
        dyn_ready,
        dyn_unavailable,
        dyn_ent_brush_n,
    );

    commands.insert_resource(map_xmodel_scene_assets);
    if let Some(atlas) = model_lighting_atlas {
        let cache =
            crate::prepare::scene::model_lighting_cache::WorldModelLightingCache::new(atlas.dims);

        scene.model_lighting_image = Some(atlas.image.clone());
        scene.model_lighting_dims = Some(atlas.dims);
        commands.insert_resource(crate::assemble::drawsurf::RuntimeImageHandles::from_pools(
            scene.runtime_material_catalog.generation_id(),
            exact_material_handles,
            scene.exact_material_names.clone(),
            reflection_probe_handles,
            lightmap_handles,
            Some(atlas.image.clone()),
        ));
        diag::info!(
            World,
            "drawsurf typed image registry: model_lighting=READY code_texture=3 sampler=0xe2 volume=D3"
        );
        commands.insert_resource(cache);
        commands.insert_resource(atlas);
    }

    let transform = if let Some(view) = scene.intermission_view {
        transform_from_iw_view(view)
    } else {
        let eye = scene.center
            + Vec3::new(
                scene.radius * 0.15,
                -scene.radius * 0.45,
                scene.radius * 0.06,
            );
        let target = scene.center + Vec3::new(0.0, scene.radius * 0.2, 0.0);
        let mut transform = Transform::from_translation(eye);
        transform.look_at(target, Vec3::Z);
        transform
    };
    let (yaw, pitch, _) = transform.rotation.to_euler(EulerRot::ZYX);

    let near = CAMERA_NEAR_INCHES;
    let far = camera_far_inches(scene.radius);

    let host_id = commands
        .spawn((
            transform,
            FlyCamera {
                yaw,
                pitch,
                speed: if scene.intermission_view.is_some() {
                    320.0
                } else {
                    scene.radius * 0.25
                },
            },
            audio::AmbientListener,
            Visibility::Inherited,
        ))
        .id();
    let lens = commands.spawn((
        FpvLens,
        Camera3d {
            depth_texture_usages: (TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::TEXTURE_BINDING)
                .into(),
            ..default()
        },
        CompositingSpace::Srgb,
        Tonemapping::None,
        Msaa::Off,
        Transform::IDENTITY,
        Projection::Perspective(PerspectiveProjection {
            fov: horizontal_to_vertical_fov_deg(CG_FOV_DEFAULT).to_radians(),
            near,
            far,
            ..default()
        }),
    ));
    let lens_id = lens.id();
    commands.entity(host_id).add_child(lens_id);
}
