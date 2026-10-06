use std::collections::HashMap;

use bevy::prelude::*;
use render_fx::{FxWorldColorImages, PreparedFxCatalog, PreparedTracers};

use crate::assemble::drawsurf::{RuntimeImageHandles, RuntimeLightmapHandles, RuntimeMaterial};

use super::world::WorldScene;

fn log_image_asset_memory(images: &Assets<Image>) {
    use bevy::asset::RenderAssetUsages;
    let (mut count, mut main_bytes, mut render_bytes) = (0usize, 0usize, 0usize);
    for (_, image) in images.iter() {
        let bytes = image.data.as_ref().map_or(0, Vec::len);
        count += 1;
        if image.asset_usage.contains(RenderAssetUsages::MAIN_WORLD) {
            main_bytes += bytes;
        }
        if image.asset_usage.contains(RenderAssetUsages::RENDER_WORLD) {
            render_bytes += bytes;
        }
    }
    let mib = |bytes: usize| bytes as f64 / (1024.0 * 1024.0);
    diag::info!(
        World,
        "image asset memory: assets={count} main_world_retained={:.1}MiB uploaded={:.1}MiB rss={:.0}MiB",
        mib(main_bytes),
        mib(render_bytes),
        asset_transport::process_resident_bytes()
            .map_or(0.0, |bytes| bytes as f64 / (1024.0 * 1024.0)),
    );
}

fn exact_color_handle_for_runtime_mat(
    material: &RuntimeMaterial,
    exact_handles: &[Option<Handle<Image>>],
) -> Option<Handle<Image>> {
    let binding = material
        .texture_semantic(asset_material::TS_COLOR_MAP)
        .or_else(|| material.texture_semantic(asset_material::TS_2D))?;
    exact_handles.get(binding.0 as usize).cloned().flatten()
}

fn stitch_fx_color_by_asset(
    colors_by_asset: &mut HashMap<usize, Handle<Image>>,
    keys_by_asset: &mut HashMap<usize, (u8, u16)>,
    exact_handles: &[Option<Handle<Image>>],
    scene: &WorldScene,
    asset_id: usize,
) {
    let Some(material) = scene
        .runtime_material_catalog
        .derived(assets::MaterialIndex::from_order(asset_id))
    else {
        return;
    };
    keys_by_asset.insert(asset_id, (material.sort_key, 0));
    if let Some(handle) = exact_color_handle_for_runtime_mat(material, exact_handles) {
        colors_by_asset.insert(asset_id, handle);
    }
}

pub(crate) fn world_color_images(
    scene: &WorldScene,
    exact_material_handles: &[Option<Handle<Image>>],
    tracers: Option<&PreparedTracers>,
    fx_catalog: Option<&PreparedFxCatalog>,
) -> FxWorldColorImages {
    let mut fx_color_by_name = HashMap::<String, Handle<Image>>::new();
    let mut fx_keys = HashMap::<String, (u8, u16)>::new();
    let mut colors_by_asset = HashMap::<usize, Handle<Image>>::new();
    let mut keys_by_asset = HashMap::<usize, (u8, u16)>::new();
    let mut tracer_bound = 0usize;
    if let Some(tracers) = tracers {
        for def in tracers.0.defs() {
            let Some(asset_id) = def.material.bound_index() else {
                continue;
            };
            tracer_bound += 1;

            stitch_fx_color_by_asset(
                &mut colors_by_asset,
                &mut keys_by_asset,
                exact_material_handles,
                scene,
                asset_id,
            );
        }
    }
    let tracer_unique = colors_by_asset.len();
    let mut elem_bound = 0usize;
    if let Some(fx) = fx_catalog {
        for effect in fx.0.effects() {
            for elem in &effect.elems {
                for vis in &elem.visuals {
                    let Some(asset_id) = vis.bound_index() else {
                        continue;
                    };
                    elem_bound += 1;
                    stitch_fx_color_by_asset(
                        &mut colors_by_asset,
                        &mut keys_by_asset,
                        exact_material_handles,
                        scene,
                        asset_id,
                    );
                }
            }
        }
    }
    let mut mark_colors_by_asset = HashMap::<usize, Handle<Image>>::new();
    let mut mark_keys_by_asset = HashMap::<usize, (u8, u16)>::new();
    let mut mark_bound = 0usize;
    if let Some(fx) = fx_catalog {
        for (asset_id, _) in fx.0.unique_decal_mark_hints() {
            mark_bound += 1;
            stitch_fx_color_by_asset(
                &mut mark_colors_by_asset,
                &mut mark_keys_by_asset,
                exact_material_handles,
                scene,
                asset_id,
            );
        }
    }

    for (asset_id, material) in scene
        .runtime_material_catalog
        .parts()
        .materials
        .iter()
        .enumerate()
    {
        if crate::adapters::motion_tracker::MATERIALS.contains(&material.name.as_str()) {
            stitch_fx_color_by_asset(
                &mut colors_by_asset,
                &mut keys_by_asset,
                exact_material_handles,
                scene,
                asset_id,
            );
        }
    }

    for (asset_id, handle) in colors_by_asset.iter().chain(mark_colors_by_asset.iter()) {
        let Some(material) = scene
            .runtime_material_catalog
            .derived(assets::MaterialIndex::from_order(*asset_id))
        else {
            continue;
        };
        asset_game::insert_fx_color_image(&mut fx_color_by_name, &material.name, handle.clone());
        asset_game::insert_fx_color_image(&mut fx_keys, &material.name, (material.sort_key, 0));
    }
    let stub_aliases = asset_game::alias_fx_color_map_stubs(&mut fx_color_by_name);
    let _ = asset_game::alias_fx_color_map_stubs(&mut fx_keys);
    diag::info!(
        World,
        "fx color maps: {} bind keys (stub_aliases={stub_aliases}, material_keys={}) — handles from exact pool, not CPU clones",
        fx_color_by_name.len(),
        fx_keys.len()
    );
    diag::info!(
        World,
        "fx tracer color maps by asset: {tracer_unique} unique / {tracer_bound} bound defs",
    );
    diag::info!(
        World,
        "fx elem color maps by asset: {} unique / {elem_bound} bound visuals",
        colors_by_asset.len()
    );
    diag::info!(
        World,
        "fx mark color maps by asset: {} unique / {mark_bound} unique Bound — not sprite colors_by_asset",
        mark_colors_by_asset.len()
    );
    if let Some(fx) = fx_catalog {
        let missing: Vec<String> =
            fx.0.unique_bound_hints()
                .into_iter()
                .filter(|(asset_id, _)| !colors_by_asset.contains_key(asset_id))
                .map(|(asset_id, hint)| {
                    let catalog = scene
                        .runtime_material_catalog
                        .parts()
                        .materials
                        .get(asset_id)
                        .map(|material| material.name.as_str())
                        .unwrap_or(hint);
                    format!("{asset_id}:{catalog}")
                })
                .collect();
        if !missing.is_empty() {
            let (distortion, other): (Vec<_>, Vec<_>) = missing
                .into_iter()
                .partition(|row| row.contains("distortion"));
            diag::info!(
                World,
                "fx elem Bound without color map: {} of {} unique (distortion={}, other={}) other_sample: {}",
                distortion.len() + other.len(),
                fx.0.material_visual_unique_bound_count(),
                distortion.len(),
                other.len(),
                if other.is_empty() {
                    "-".to_string()
                } else {
                    other.join("; ")
                }
            );
        }
    }
    let motion_tracker = crate::adapters::motion_tracker::MATERIALS.map(|name| {
        let catalog = &scene.runtime_material_catalog;
        let ordinal = catalog.ordinal_for_material_name(name)?;
        let material = catalog
            .parts()
            .materials
            .iter()
            .find(|material| material.name == name)?;
        Some(render_fx::drawsurf::tess::fx::FxPassMaterial {
            color: fx_color_by_name.get(name).cloned(),
            sort_key: material
                .baked_draw_surf
                .map(|key| dpvs_iw4::GfxDrawSurf { packed: key }.primary_sort_key())?,
            material_sorted_index: Some(ordinal.get()),
        })
    });
    FxWorldColorImages {
        motion_tracker,
        colors: fx_color_by_name,
        keys: fx_keys,
        colors_by_asset,
        keys_by_asset,
        mark_colors_by_asset,
        mark_keys_by_asset,
    }
}

pub(crate) fn prepare_fx_model_geometry(
    scene: &WorldScene,
    models: Option<&render_fx::PreparedFxModels>,
) -> render_fx::PreparedFxModelGeometry {
    let mut plan = render_fx::FxModelDrawPlan::default();
    plan.generation = scene.runtime_material_catalog.generation_id().get();
    let Some(models) = models else {
        return render_fx::PreparedFxModelGeometry(plan);
    };
    let Some((image, dims)) = scene
        .model_lighting_image
        .clone()
        .zip(scene.model_lighting_dims)
    else {
        return render_fx::PreparedFxModelGeometry(plan);
    };
    let atlas = super::model_lighting_atlas::WorldModelLightingAtlas { image, dims };
    let models = &models.0;
    for index in 0..models.len() {
        let Some(entry) = models.get_at(index) else {
            continue;
        };
        let name = models.name_at(index).unwrap_or_default();
        let binding = (|| {
            let pose = entry.skel.pose.as_ref()?;
            let dobj = xmodel_runtime::DObj::build(&[(pose, None)]).ok()?;
            let state = xmodel_runtime::DObjSemanticState::bind_pose(name.to_owned(), 1, 1);
            let request = state.resolve_request(|_| None).ok()?;
            let world = xmodel_runtime::pose_dobj(&dobj, &request, Mat4::IDENTITY).ok()?;
            let skin = dobj.skin_matrices(&world);
            Some((dobj, request, skin))
        })();
        let Some((dobj, request, skin)) = binding else {
            diag::warn!(World, "FX model `{name}` refused: bind pose unavailable");
            continue;
        };
        for lod in 0..entry
            .skel
            .lod
            .map_or(1, |selector| selector.num_lods().clamp(0, 4) as u8)
        {
            let Some(lod_surfaces) = render_anim::anim::xmodel_pose::skin_model_filtered(
                &entry.skel,
                |bone| skin[bone],
                &[],
                |surface| {
                    dobj.surface_visible(
                        0,
                        &entry.skel.surface_part_bits[surface],
                        &request.hide_part_bits,
                    )
                },
                |surface| {
                    render_anim::anim::xmodel_pose::stream_lod_surface_rigid(
                        &entry.skel,
                        lod,
                        &[],
                        0,
                        surface,
                    )
                },
                lod,
            ) else {
                diag::warn!(
                    World,
                    "FX model `{name}` LOD {lod} refused: geometry unavailable"
                );
                continue;
            };
            let materials: Vec<_> = lod_surfaces
                .iter()
                .map(|surface| {
                    crate::assemble::drawsurf::tess::xmodel::bound_lit_xmodel_pass_material(
                        &atlas,
                        &scene.runtime_material_catalog,
                        entry.material_index(surface.surface_index)?,
                    )
                })
                .collect();
            if materials.iter().any(Option::is_none) {
                diag::warn!(
                    World,
                    "FX model `{name}` LOD {lod} refused: material binding unavailable"
                );
                continue;
            }
            render_fx::append_fx_model_asset(&mut plan, index, lod, &lod_surfaces, &materials);
        }
    }
    render_fx::PreparedFxModelGeometry(plan)
}

pub fn install(
    commands: &mut Commands,
    scene: &WorldScene,
    images: &Assets<Image>,
    exact_material_handles: Vec<Option<Handle<Image>>>,
    reflection_probe_handles: Vec<Option<Handle<Image>>>,
    lightmap_handles: Vec<Option<RuntimeLightmapHandles>>,
    tracers: Option<&PreparedTracers>,
    fx_catalog: Option<&PreparedFxCatalog>,
) {
    let fx_images = world_color_images(scene, &exact_material_handles, tracers, fx_catalog);
    commands.insert_resource(fx_images);
    let exact_material_views = exact_material_handles.iter().flatten().count();
    let exact_probe_views = reflection_probe_handles.iter().flatten().count();
    let exact_lightmap_views = lightmap_handles
        .iter()
        .flatten()
        .filter(|page| page.primary.is_some() && page.secondary.is_some())
        .count();
    commands.insert_resource(RuntimeImageHandles::from_pools(
        scene.runtime_material_catalog.generation_id(),
        exact_material_handles.clone(),
        scene.exact_material_names.clone(),
        reflection_probe_handles.clone(),
        lightmap_handles.clone(),
        None,
    ));
    log_image_asset_memory(images);
    diag::info!(
        World,
        "drawsurf typed image registry: material={exact_material_views}/{} probe={exact_probe_views}/{} lightmap_exact={exact_lightmap_views}/{}",
        exact_material_handles.len(),
        reflection_probe_handles.len(),
        lightmap_handles.len(),
    );
}
