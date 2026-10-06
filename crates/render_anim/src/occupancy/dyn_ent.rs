use bevy::prelude::*;
use dpvs_iw4::{
    Bounds, DpvsPlanes, dyn_ent_cell_bits_len, dyn_ent_client_word_count, dyn_ent_in_cell,
    dyn_ent_primary_light_vis_word_count, filter_dyn_ent_into_cells,
    link_dyn_ent_primary_light_bit, unfilter_dyn_ent_from_cells,
    unlink_dyn_ent_from_primary_lights,
};

use crate::anim::body_frustum::AdmittedCellVis;
use crate::anim::xmodel_pose::PosedModelSurface;
use crate::lighting_box_half;
use crate::occupancy::dyn_ent_phys;
use crate::occupancy::script_model::{DObjLodView, pose_script_dobj_with_materials};
use crate::{
    DynEntDrawPlan, DynEntOwnerDraw, XMODEL_OBJECT_ID_DYNENT_BASE, append_dynent_asset,
    stamp_plan_geometry, topology_fingerprint,
};
use anim_iw4::DOBJ_RADIUS_PARENT_ROOT;
use render_scene::{
    DynAtPointLookup, DynEntModelEntity, FpvLens, FrustumSpec, ModelLightingOwner,
    ModelLightingRequest, ModelLightingRequests, PublishedCellVis, TessMaterials, WorldDpvsCells,
    WorldDynEntInstance, WorldModelLightingAtlas, WorldPresentFacts, XModelColourRefusal,
    XModelSurfaceDraw, perspective_frustum_planes,
};
use render_scene::{SmodelPassMaterial, smodel_camera_lod};

#[derive(Resource, Clone, Debug, Default)]
pub struct DynEntCellBits {
    pub word_count: usize,
    pub cell_count: usize,
    pub client_count: u32,
    pub bits: Vec<u32>,
}

impl DynEntCellBits {
    pub fn is_ready(&self) -> bool {
        self.word_count > 0
            && self.cell_count > 0
            && self.bits.len() == dyn_ent_cell_bits_len(self.word_count, self.cell_count)
    }

    pub fn ensure(&mut self, client_count: u32, cell_count: usize) -> bool {
        let word_count = dyn_ent_client_word_count(client_count);
        let len = dyn_ent_cell_bits_len(word_count, cell_count);
        if self.word_count == word_count
            && self.cell_count == cell_count
            && self.client_count == client_count
            && self.bits.len() == len
        {
            return false;
        }
        self.word_count = word_count;
        self.cell_count = cell_count;
        self.client_count = client_count;
        self.bits = vec![0; len];
        true
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct DynEntPrimaryLightVis {
    pub words: Vec<u32>,
    pub closest: Vec<u8>,
    pub sun_primary: u32,
    pub primary_count: u32,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct DynEntBrushPrimaryLightVis {
    pub words: Vec<u32>,
    pub sun_primary: u32,
    pub primary_count: u32,
}

pub fn ensure_primary_light_words(
    words: &mut Vec<u32>,
    client_count: u32,
    sun: u32,
    primary_count: u32,
) {
    let n = dyn_ent_primary_light_vis_word_count(client_count, sun, primary_count);
    if words.len() != n {
        *words = vec![0; n];
    }
}

pub fn relink_dyn_ent_primary_lights(
    words: &mut Vec<u32>,
    closest: Option<&mut [u8]>,
    atpoint: &DynAtPointLookup,
    id: u32,
    bounds: Bounds,
    dead: bool,
) {
    let sun = atpoint.sun_primary();
    let count = atpoint.primary_count();
    if dead {
        unlink_dyn_ent_from_primary_lights(words, id, sun, count);
        if let Some(slot) = closest.and_then(|c| c.get_mut(id as usize)) {
            *slot = 0;
        }
        return;
    }
    let closest_byte = atpoint.link_dyn_ent_primary(bounds.mid(), bounds.half(), |light, set| {
        link_dyn_ent_primary_light_bit(words, id, light, sun, count, set);
    });
    if let Some(slot) = closest.and_then(|c| c.get_mut(id as usize)) {
        *slot = closest_byte;
    }
}

#[derive(Resource, Default)]
struct DynEntPoseProduct {
    assets: Vec<DynEntPosedAsset>,
    owners: Vec<DynEntPosedOwner>,
}

struct DynEntPosedAsset {
    key: asset_world::MapXModelAssetKey,
    camera_lod: Option<u8>,
    surfaces: Vec<PosedModelSurface>,
    authored: Vec<Option<assets::MaterialIndex>>,
}

struct DynEntPosedOwner {
    entity: Entity,
    asset_index: usize,
    model: String,
    world_from_local: Mat4,
    lighting_origin: [f32; 3],
    camera_hidden: bool,
    radius: Option<f32>,
}

impl DynEntPoseProduct {
    fn asset_index(
        &self,
        key: &asset_world::MapXModelAssetKey,
        camera_lod: Option<u8>,
    ) -> Option<usize> {
        self.assets
            .iter()
            .position(|asset| &asset.key == key && asset.camera_lod == camera_lod)
    }
}

pub fn register_dyn_ent_systems(app: &mut App) {
    dyn_ent_phys::register_dyn_ent_phys(app);
    app.init_resource::<DynEntDrawPlan>()
        .init_resource::<DynEntPoseProduct>()
        .init_resource::<DynEntCellBits>()
        .init_resource::<DynEntPrimaryLightVis>()
        .add_systems(
            Update,
            (
                link_dyn_ent_cells.after(dyn_ent_phys::step_phys_world0),
                cull_dyn_ent_cell_models
                    .after(link_dyn_ent_cells)
                    .after(frame::WorkerCmdSet::CellStatic),
                pose_dyn_ents.after(cull_dyn_ent_cell_models),
                append_dynent_draws.after(pose_dyn_ents),
            )
                .in_set(frame::RenderSet::Anim)
                .in_set(frame::WorkerCmdSet::CellDynModel),
        )
        .add_systems(
            Update,
            dyn_ent_phys::step_phys_world0.in_set(frame::WorkerCmdSet::Physics),
        );
}

pub use dyn_ent_phys::{DynEntPhysClip, DynEntPhysImpulse, DynEntPhysWorld};

pub fn sphere_behind_frustum(origin: [f32; 3], radius: f32, planes: &[[f32; 4]]) -> bool {
    render_scene::XModelCasterBound { origin, radius }.outside(planes)
}

pub fn fpv_frustum_planes(
    cameras: &Query<(&GlobalTransform, &Projection, &Camera), With<FpvLens>>,
) -> Vec<[f32; 4]> {
    if matches!(
        std::env::var("IW4L_NO_FRUSTUM").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    ) {
        return Vec::new();
    }
    let Ok((cam_xf, projection, camera)) = cameras.single() else {
        return Vec::new();
    };
    let (fov, near, far, mut aspect) = match projection {
        Projection::Perspective(p) => (p.fov, p.near, p.far, p.aspect_ratio),
        Projection::Orthographic(_) | Projection::Custom(_) => return Vec::new(),
    };
    if aspect < 1e-3
        && let Some(vw) = camera.logical_viewport_size()
        && vw.y > 0.0
    {
        aspect = vw.x / vw.y;
    }
    if aspect < 1e-3 {
        aspect = 16.0 / 9.0;
    }
    let (_scale, rot, _) = cam_xf.to_scale_rotation_translation();
    let mut planes = perspective_frustum_planes(FrustumSpec {
        eye: cam_xf.translation(),
        forward: rot * Vec3::NEG_Z,
        right: rot * Vec3::X,
        up: rot * Vec3::Y,
        fov_y_rad: fov,
        aspect,
        near,
        far,
    })
    .to_vec();
    if matches!(
        std::env::var("IW4L_FRUSTUM_NEARFAR").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    ) {
        planes.truncate(2);
    }
    planes
}

pub fn xmodel_radius(
    catalog: Option<&asset_world::MapXModelSceneCatalog>,
    key: &asset_world::MapXModelAssetKey,
) -> f32 {
    let Some(catalog) = catalog else {
        return 0.0;
    };
    match catalog.get(key) {
        Some(
            asset_world::MapXModelSceneAsset::Iw4(skel)
            | asset_world::MapXModelSceneAsset::Iw5(skel)
            | asset_world::MapXModelSceneAsset::T6(skel)
            | asset_world::MapXModelSceneAsset::T5(skel),
        ) => skel.radius.unwrap_or(0.0),
        Some(asset_world::MapXModelSceneAsset::Unavailable { .. }) | None => 0.0,
    }
}

pub fn xmodel_phys_hull(
    catalog: Option<&asset_world::MapXModelSceneCatalog>,
    key: &asset_world::MapXModelAssetKey,
) -> ([f32; 3], [f32; 3]) {
    let radius = xmodel_radius(catalog, key).max(1.0);
    let fallback = ([-radius; 3], [radius; 3]);
    let Some(catalog) = catalog else {
        return fallback;
    };
    match catalog.get(key) {
        Some(
            asset_world::MapXModelSceneAsset::Iw4(skel)
            | asset_world::MapXModelSceneAsset::Iw5(skel)
            | asset_world::MapXModelSceneAsset::T6(skel)
            | asset_world::MapXModelSceneAsset::T5(skel),
        ) => hull_from_bounds(skel.bounds).unwrap_or(fallback),
        Some(asset_world::MapXModelSceneAsset::Unavailable { .. }) | None => fallback,
    }
}

pub(crate) fn hull_from_bounds(
    bounds: Option<([f32; 3], [f32; 3])>,
) -> Option<([f32; 3], [f32; 3])> {
    let (mid, half) = bounds?;
    if !half.iter().all(|h| h.is_finite() && *h > 0.0) {
        return None;
    }
    Some((
        [mid[0] - half[0], mid[1] - half[1], mid[2] - half[2]],
        [mid[0] + half[0], mid[1] + half[1], mid[2] + half[2]],
    ))
}

fn xmodel_local_bounds(
    catalog: Option<&asset_world::MapXModelSceneCatalog>,
    key: &asset_world::MapXModelAssetKey,
) -> Option<([f32; 3], [f32; 3])> {
    let catalog = catalog?;
    match catalog.get(key) {
        Some(
            asset_world::MapXModelSceneAsset::Iw4(skel)
            | asset_world::MapXModelSceneAsset::Iw5(skel)
            | asset_world::MapXModelSceneAsset::T6(skel)
            | asset_world::MapXModelSceneAsset::T5(skel),
        ) => skel.bounds,
        Some(asset_world::MapXModelSceneAsset::Unavailable { .. }) | None => None,
    }
}

pub(crate) fn dyn_ent_world_bounds(
    transform: &Transform,
    local: Option<([f32; 3], [f32; 3])>,
    radius: f32,
) -> Bounds {
    if let Some((mid, half)) = local
        && half.iter().all(|h| h.is_finite() && *h > 0.0)
    {
        return transform_local_bounds(transform, mid, half);
    }
    Bounds::from_mid_half(transform.translation.to_array(), [radius.max(1.0); 3])
}

fn transform_local_bounds(
    transform: &Transform,
    local_mid: [f32; 3],
    local_half: [f32; 3],
) -> Bounds {
    let world_mid = transform.transform_point(Vec3::from_array(local_mid));
    let rot = Mat3::from_quat(transform.rotation);
    let world_half = rot.x_axis.abs() * local_half[0]
        + rot.y_axis.abs() * local_half[1]
        + rot.z_axis.abs() * local_half[2];
    Bounds::from_mid_half(world_mid.to_array(), world_half.to_array())
}

pub(crate) fn cell_admission_hides(
    dyn_ent_id: u32,
    membership: &DynEntCellBits,
    vis: AdmittedCellVis<'_>,
) -> bool {
    if !membership.is_ready() || vis.vis_all || vis.words.is_empty() || vis.cell_count == 0 {
        return false;
    }
    let last = vis.cell_count.min(membership.cell_count);
    for cell in 0..last {
        if vis.contains(cell)
            && dyn_ent_in_cell(&membership.bits, membership.word_count, cell, dyn_ent_id)
        {
            return false;
        }
    }
    true
}

fn admitted_vis(vis: &PublishedCellVis) -> AdmittedCellVis<'_> {
    AdmittedCellVis {
        words: &vis.words,
        cell_count: vis.cell_count,
        vis_all: vis.vis_all,
    }
}

fn link_dyn_ent_cells(
    cells: Res<WorldDpvsCells>,
    atpoint: Res<DynAtPointLookup>,
    catalog: Option<Res<asset_world::MapXModelSceneCatalog>>,
    mut membership: ResMut<DynEntCellBits>,
    mut vis: ResMut<DynEntPrimaryLightVis>,
    changed: Query<
        (&WorldDynEntInstance, &Transform),
        (
            With<DynEntModelEntity>,
            Or<(
                Changed<Transform>,
                Changed<WorldDynEntInstance>,
                Added<WorldDynEntInstance>,
            )>,
        ),
    >,
    all: Query<(&WorldDynEntInstance, &Transform), With<DynEntModelEntity>>,
) {
    if !cells.is_ready() {
        *membership = DynEntCellBits::default();
        *vis = DynEntPrimaryLightVis::default();
        return;
    }
    let dpvs = DpvsPlanes {
        planes: &cells.planes,
        nodes: &cells.nodes,
        cell_count: cells.cell_count as u32,
    };
    let catalog = catalog.as_deref();
    let client_count = all
        .iter()
        .map(|(inst, _)| inst.index)
        .max()
        .map(|index| u32::from(index) + 1)
        .unwrap_or(0);
    let wiped = membership.ensure(client_count, cells.cell_count);
    vis.sun_primary = atpoint.sun_primary();
    vis.primary_count = atpoint.primary_count();
    let sun = vis.sun_primary;
    let primary_count = vis.primary_count;
    ensure_primary_light_words(&mut vis.words, client_count, sun, primary_count);
    if vis.closest.len() != client_count as usize {
        vis.closest = vec![0; client_count as usize];
    }

    let relink = |membership: &mut DynEntCellBits,
                  vis: &mut DynEntPrimaryLightVis,
                  inst: &WorldDynEntInstance,
                  transform: &Transform| {
        let id = u32::from(inst.index);
        if inst.dead {
            unfilter_dyn_ent_from_cells(
                &mut membership.bits,
                membership.word_count,
                membership.cell_count,
                id,
            );
            relink_dyn_ent_primary_lights(
                &mut vis.words,
                Some(vis.closest.as_mut_slice()),
                &atpoint,
                id,
                Bounds::from_mid_half([0.0; 3], [0.0; 3]),
                true,
            );
            return;
        }
        let radius = xmodel_radius(catalog, &inst.current_model);
        let bounds = dyn_ent_world_bounds(
            transform,
            xmodel_local_bounds(catalog, &inst.current_model),
            radius,
        );
        filter_dyn_ent_into_cells(
            &dpvs,
            bounds,
            &mut membership.bits,
            membership.word_count,
            id,
        );
        relink_dyn_ent_primary_lights(
            &mut vis.words,
            Some(vis.closest.as_mut_slice()),
            &atpoint,
            id,
            bounds,
            false,
        );
    };

    if wiped {
        for (inst, transform) in &all {
            relink(&mut membership, &mut vis, inst, transform);
        }
        return;
    }
    for (inst, transform) in &changed {
        relink(&mut membership, &mut vis, inst, transform);
    }
}

fn cull_dyn_ent_cell_models(
    catalog: Option<Res<asset_world::MapXModelSceneCatalog>>,
    cameras: Query<(&GlobalTransform, &Projection, &Camera), With<FpvLens>>,
    cell_vis: Res<PublishedCellVis>,
    membership: Res<DynEntCellBits>,
    mut instances: Query<
        (&WorldDynEntInstance, &Transform, &mut Visibility),
        With<DynEntModelEntity>,
    >,
) {
    let planes = fpv_frustum_planes(&cameras);
    let vis = admitted_vis(&cell_vis);
    let catalog = catalog.as_deref();
    for (inst, transform, mut visibility) in &mut instances {
        if inst.dead {
            *visibility = Visibility::Hidden;
            continue;
        }
        if cell_admission_hides(u32::from(inst.index), &membership, vis) {
            *visibility = Visibility::Hidden;
            continue;
        }
        let origin = transform.translation.to_array();
        let radius = xmodel_radius(catalog, &inst.current_model);
        let culled = sphere_behind_frustum(origin, radius, &planes);
        if culled {
            *visibility = Visibility::Hidden;
        } else {
            *visibility = Visibility::Inherited;
        }
    }
}

fn pose_dyn_ents(
    catalog: Option<Res<asset_world::MapXModelSceneCatalog>>,
    instances: Query<(Entity, &WorldDynEntInstance, &Transform, &Visibility)>,
    cameras: Query<(&GlobalTransform, &Projection, &Camera), With<FpvLens>>,
    lod_skinned: Res<render_scene::LodRampSkinnedDvar>,
    prepared: Res<crate::anim::model_materials::PreparedModelMaterials>,
    mut product: ResMut<DynEntPoseProduct>,
) {
    product.owners.clear();
    if catalog.as_ref().is_some_and(|c| c.is_changed()) {
        product.assets.clear();
    }
    let Some(catalog) = catalog.as_deref() else {
        product.assets.clear();
        return;
    };
    let eye = cameras
        .single()
        .ok()
        .map(|(xf, _, _)| xf.translation().to_array());
    let skinned_ramp = lod_skinned.args();
    for (entity, inst, transform, visibility) in &instances {
        if inst.dead {
            continue;
        }

        let camera_hidden = *visibility == Visibility::Hidden;
        let skel = match catalog.get(&inst.current_model) {
            Some(
                asset_world::MapXModelSceneAsset::Iw4(skel)
                | asset_world::MapXModelSceneAsset::Iw5(skel)
                | asset_world::MapXModelSceneAsset::T6(skel)
                | asset_world::MapXModelSceneAsset::T5(skel),
            ) => skel.as_ref(),
            Some(asset_world::MapXModelSceneAsset::Unavailable { .. }) | None => {
                continue;
            }
        };

        let camera_lod = smodel_camera_lod(
            skel.lod,
            inst.lighting_origin,
            1.0,
            eye.map(Vec3::from_array),
            skinned_ramp,
        );
        if eye.is_some() && camera_lod.is_none() {
            continue;
        }
        let asset_index = if let Some(index) = product.asset_index(&inst.current_model, camera_lod)
        {
            index
        } else {
            let Some(dobj) = prepared.scene_dobj(&inst.current_model.0) else {
                continue;
            };
            let dobj_state =
                xmodel_runtime::DObjSemanticState::bind_pose(inst.current_model.0.clone(), 1, 1);
            let Ok(request) = dobj_state.resolve_request(|_| None) else {
                continue;
            };
            let Some((surfaces, authored)) = pose_script_dobj_with_materials(
                Some(catalog),
                &[skel],
                dobj,
                &request,
                Some(DObjLodView {
                    origin: inst.lighting_origin,
                    eye: eye.map(Vec3::from_array),
                    ramp: skinned_ramp,
                }),
                &[],
            ) else {
                continue;
            };
            let index = product.assets.len();
            product.assets.push(DynEntPosedAsset {
                key: inst.current_model.clone(),
                camera_lod,
                surfaces,
                authored,
            });
            index
        };
        product.owners.push(DynEntPosedOwner {
            entity,
            asset_index,
            model: inst.current_model.0.clone(),
            world_from_local: transform.to_matrix(),
            lighting_origin: inst.lighting_origin,
            camera_hidden,
            radius: skel.radius,
        });
    }
}

fn append_dynent_draws(
    catalog: Option<Res<asset_world::MapXModelSceneCatalog>>,
    atlas: Option<Res<WorldModelLightingAtlas>>,
    atpoint: Res<DynAtPointLookup>,
    tess: Option<Res<TessMaterials>>,
    facts: Res<WorldPresentFacts>,
    mut plan: ResMut<DynEntDrawPlan>,
    mut lighting_requests: ResMut<ModelLightingRequests>,
    product: Res<DynEntPoseProduct>,
    model_materials: Res<crate::anim::model_materials::PreparedModelMaterials>,
    mut last_material_generation: Local<Option<render_material::MaterialGenerationId>>,
    mut draws: Local<Vec<XModelSurfaceDraw>>,
    mut owners: Local<Vec<DynEntOwnerDraw>>,
) {
    let atlas_ref = atlas.as_deref();
    if catalog.is_none() || atlas_ref.is_none() || !facts.spawned || tess.is_none() {
        plan.publish_no_rows();
        return;
    }
    let catalog_changed = catalog.as_ref().is_some_and(|c| c.is_changed());
    let atlas_changed = atlas.as_ref().is_some_and(|a| a.is_changed());
    let tess_changed = tess.as_ref().is_some_and(|h| h.is_changed());
    let tess = tess.as_deref().expect("checked");
    let tess_catalog = std::sync::Arc::clone(&tess.catalog());
    let material_generation = tess.catalog().generation_id();
    let catalog_reset = catalog_changed
        || atlas_changed
        || tess_changed
        || *last_material_generation != Some(material_generation);
    if catalog_reset {
        let (revision, generation) = (plan.revision, plan.generation);
        *plan = DynEntDrawPlan::default();
        plan.revision = revision;
        plan.generation = generation;
        *last_material_generation = Some(material_generation);
    }
    draws.clear();
    owners.clear();

    let mut drawn = 0u16;
    let mut packed_assets: Vec<Option<Vec<(u32, u32)>>> = vec![None; product.assets.len()];

    for row in &product.owners {
        let posed = &product.assets[row.asset_index];
        let surfaces_idx = if let Some(asset) = plan.asset(&posed.key, posed.camera_lod) {
            asset.surfaces.clone()
        } else if let Some(surfaces) = packed_assets[row.asset_index].as_ref() {
            surfaces.clone()
        } else {
            let materials: Vec<Option<SmodelPassMaterial>> = posed
                .surfaces
                .iter()
                .zip(posed.authored.iter().copied())
                .map(|(_surface, authored)| {
                    model_materials.authored(&tess_catalog, authored?).cloned()
                })
                .collect();
            if materials.iter().all(Option::is_none) {
                continue;
            }
            let surfaces = append_dynent_asset(
                &mut plan,
                posed.key.clone(),
                posed.camera_lod,
                &posed.surfaces,
                &materials,
            );
            packed_assets[row.asset_index] = Some(surfaces.clone());
            surfaces
        };
        if surfaces_idx.is_empty() {
            continue;
        }
        let box_half = row
            .radius
            .and_then(|radius| lighting_box_half(&[radius], &[DOBJ_RADIUS_PARENT_ROOT]));
        let lookup_fallback = atpoint.fallback(row.lighting_origin, box_half);
        let pending_lighting = (!row.camera_hidden).then(|| {
            lighting_requests.request(ModelLightingRequest {
                owner: ModelLightingOwner::DynEnt(row.entity),
                origin: row.lighting_origin,
                lookup_fallback,
            })
        });
        let caster_bound = row.radius.map(|radius| render_scene::XModelCasterBound {
            origin: row.world_from_local.w_axis.truncate().to_array(),
            radius: radius.max(1.0),
        });
        let object_id = XMODEL_OBJECT_ID_DYNENT_BASE.saturating_add(drawn);
        drawn = drawn.saturating_add(1);
        owners.push(DynEntOwnerDraw {
            object_id,
            model: row.model.clone(),
        });
        for (surface, material) in surfaces_idx {
            draws.push(XModelSurfaceDraw {
                surface,
                material,
                world_from_local: row.world_from_local,
                lighting_handle: 0,
                pending_lighting,
                colour_refusal: row
                    .camera_hidden
                    .then_some(XModelColourRefusal::CameraFrustum),
                object_id,
                scene_light_index: 0,
                reflection_probe_index: 0,
                packed_lighting: None,
                is_scope: false,
                scene_entnum: None,
                body_client: None,
                caster_bound,
            });
        }
    }
    if !draws.is_empty() {
        let topology =
            topology_fingerprint(&plan.indices, &plan.surface_ranges, plan.vertices.len());
        let rev = plan.revision;
        plan.revision = stamp_plan_geometry(&mut plan.revisions, rev, topology);
    }
    plan.publish_frame_rows(&mut draws, &mut owners);
}
