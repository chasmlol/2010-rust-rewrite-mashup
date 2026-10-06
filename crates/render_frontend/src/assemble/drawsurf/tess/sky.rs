use super::smodel::{SmodelGpuPlan, SmodelPassMaterial, pack_smodel_meshes, runtime_maps};
use super::xmodel::XModelSurfaceDraw;
use crate::prepare::scene::{spawn::WorldSpawnJob, world::WorldScene};
use bevy::prelude::*;
use render_frame::SourceRevisions;

#[derive(Resource, Default)]
pub struct SkyModelDrawPlan {
    pub geometry: SmodelGpuPlan,
    pub draws: Vec<XModelSurfaceDraw>,
    pub refusals: Vec<SkyModelRefusal>,
    pub generation: u64,
    pub revisions: SourceRevisions,
}

#[derive(Debug)]
pub enum SkyModelRefusal {
    SurfaceMaterialMissing { surface: u32 },
    MaterialNotSorted { material: assets::MaterialIndex },
    MaterialUnavailable { material: assets::MaterialIndex },
}

pub fn build_sky_model_draw_plan(
    mut commands: Commands,
    scene: Option<Res<WorldScene>>,
    job: Option<Res<WorldSpawnJob>>,
    existing: Option<Res<SkyModelDrawPlan>>,
) {
    if existing.is_some() {
        return;
    }
    let (Some(scene), Some(job)) = (scene, job) else {
        return;
    };
    let Some(model) = &scene.sky_model else {
        return;
    };
    let (exact, _, _) = job.tess_image_handles();
    if exact.is_empty() {
        return;
    }
    let (mut geometry, _, _, _) = pack_smodel_meshes(std::slice::from_ref(model), &[]);
    let mut draws = Vec::new();
    let mut refusals = Vec::new();
    for &(surface, authored) in &geometry.meshes[0].surfaces_by_lod[0] {
        let Some(authored) = authored else {
            refusals.push(SkyModelRefusal::SurfaceMaterialMissing { surface });
            continue;
        };
        let Some(derived) = scene.runtime_material_catalog.derived(authored) else {
            refusals.push(SkyModelRefusal::MaterialUnavailable { material: authored });
            continue;
        };
        let Some(ordinal) = scene
            .runtime_material_catalog
            .parts()
            .sorted_materials
            .ordinal_for_asset_id(authored.order())
        else {
            refusals.push(SkyModelRefusal::MaterialNotSorted { material: authored });
            continue;
        };
        let maps = runtime_maps(Some(authored), &scene.runtime_material_catalog, exact);
        let material = geometry.materials.len() as u32;
        geometry.materials.push(SmodelPassMaterial {
            model_lighting_required: false,
            color: maps.color,
            specular: None,
            probe: None,
            atlas: None,
            alpha_mode: maps.alpha_mode,
            draw_mode: maps.draw_mode,
            cull_mode: maps.cull_mode,
            env_map_parms: maps.env_map_parms,
            lighting_lookup_scale: [0.; 4],
            atlas_lookup: [0.; 4],
            sort_key: derived.sort_key,
            material_sorted_index: Some(ordinal.get()),
        });
        draws.push(XModelSurfaceDraw {
            surface,
            material,
            world_from_local: Mat4::IDENTITY,
            lighting_handle: 0,
            pending_lighting: None,
            colour_refusal: None,
            object_id: u16::MAX,
            scene_light_index: 0,
            reflection_probe_index: 0,
            packed_lighting: None,
            is_scope: false,
            scene_entnum: None,
            body_client: None,
            caster_bound: None,
        });
    }
    diag::info!(
        World,
        "skybox tess: model={} surfaces={} skipped={}",
        model.name,
        draws.len(),
        geometry.packed_skip_surfaces
    );
    if !refusals.is_empty() {
        diag::warn!(
            World,
            "skybox tess refusals: model={} {refusals:?}",
            model.name
        );
    }
    let mut revisions = SourceRevisions::default();
    revisions.set_topology_from(
        &geometry.indices(),
        &geometry.surface_ranges(),
        geometry.decoded_vertices().len(),
    );
    revisions.bump_surfaces();
    revisions.bump_draws();
    commands.insert_resource(SkyModelDrawPlan {
        geometry,
        draws,
        refusals,
        generation: 1,
        revisions,
    });
}
