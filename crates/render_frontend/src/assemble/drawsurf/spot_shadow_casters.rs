use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use lighting_iw4::{
    GFX_LIGHT_TYPE_SPOT, SPOT_SHADOW_ENT_MARK_LEN, SPOT_SHADOW_SCORE_LUMA,
    SPOT_SHADOW_SM_LIGHT_CAP, SpotShadowChooseDvars, SpotShadowHistory, SpotShadowableLight,
    set_used_bit, spot_shadow_choose, spot_shadow_dyn_brush_tess_admits,
    spot_shadow_dyn_brush_vis_bit_index, spot_shadow_emit_frontend, spot_shadow_fill_occupancy,
    spot_shadow_fill_scene_dobj_vis, spot_shadow_fill_scene_model_vis,
    spot_shadow_filter_smodel_ids, spot_shadow_link_dyn_brush_vis, spot_shadow_link_entity,
    spot_shadow_packed_lists_ready, spot_shadow_primary_vis_bit,
    spot_shadow_primary_vis_word_count, spot_shadow_tess_bsp_surfs,
    spot_shadow_tess_scene_dobj_indices, spot_shadow_tess_scene_model_indices,
    spot_shadow_walk_casters, spot_shadow_xmodel_is_caster,
};
use render_frame::PackedFrontendLists;
pub use render_frame::SpotShadowFrameSlot;
use std::ops::Range;
use std::sync::Arc;

use super::list;
use super::retained_list::{RetainedDrawItem, RetainedDrawKind};
use super::tess::smodel::{LodRampArgs, SmodelGpuPlan, smodel_camera_lod};
use super::tess::xmodel::{XModelDrawPlan, XModelSurfaceDraw};
use crate::pack_spot_shadow_frontend;
use crate::prepare::scene::gfx_scene::SpotShadowSceneOccupancy;
use crate::prepare::scene::view_parms::PreparedSceneView;
use crate::prepare::scene::world::WorldScene;
use dpvs_iw4::smodel_cull_dist_hides;

#[derive(Resource, Clone, Debug, Default)]
pub struct SpotShadowCasterPlan {
    pub history: SpotShadowHistory,
    pub packed: Vec<PackedFrontendLists>,
    emitted: Vec<lighting_iw4::SpotShadowEmittedSlot>,
    input_ranges: Vec<Range<usize>>,
    pub items: Vec<RetainedDrawItem>,
    pub marked_n: u32,
}

fn remap_sidecar(indices: &mut [u32], input: &Range<usize>, compact: &[u32]) {
    for index in indices {
        let local = usize::try_from(*index).expect("spot packed provenance fits usize");
        let source = input
            .start
            .checked_add(local)
            .filter(|&source| source < input.end)
            .expect("spot packed provenance belongs to its emitted slot");
        *index = *compact
            .get(source)
            .expect("spot product compaction covers every slot input");
    }
}

impl SpotShadowCasterPlan {
    pub(crate) fn clear_frame(&mut self) {
        self.packed.clear();
        self.emitted.clear();
        self.input_ranges.clear();
        self.items.clear();
        self.marked_n = 0;
    }

    pub(crate) fn shadowed_light_indices(&self) -> Vec<u8> {
        self.emitted.iter().map(|slot| slot.light_index).collect()
    }

    pub(crate) fn receivers_by_light_index(&self) -> Vec<Option<render_frame::SpotShadowReceiver>> {
        let mut out = Vec::new();
        for slot in &self.emitted {
            let Some(lookup) = slot.lookup else {
                continue;
            };
            let index = usize::from(slot.light_index);
            if out.len() <= index {
                out.resize(index + 1, None);
            }
            out[index] = Some(render_frame::SpotShadowReceiver {
                lookup,
                pixel_adjust: slot.plan.pixel_adjust(),
            });
        }
        out
    }

    pub(crate) fn take_frame_slots(&mut self, compact: &[u32]) -> Vec<SpotShadowFrameSlot> {
        assert_eq!(self.packed.len(), self.emitted.len());
        assert_eq!(self.packed.len(), self.input_ranges.len());
        let packed = std::mem::take(&mut self.packed);
        let emitted = std::mem::take(&mut self.emitted);
        let ranges = std::mem::take(&mut self.input_ranges);
        packed
            .into_iter()
            .zip(emitted)
            .zip(ranges)
            .map(|((mut packed, emitted), input)| {
                remap_sidecar(&mut packed.world_draw_indices, &input, compact);
                remap_sidecar(&mut packed.xmodel_draw_indices, &input, compact);
                remap_sidecar(&mut packed.smodel_draw_indices, &input, compact);
                remap_sidecar(&mut packed.smodel_cached_draw_indices, &input, compact);
                remap_sidecar(&mut packed.smodel_pretess_draw_indices, &input, compact);
                remap_sidecar(&mut packed.smodel_skinned_draw_indices, &input, compact);
                SpotShadowFrameSlot {
                    emitted,
                    packed: Arc::new(packed),
                }
            })
            .collect()
    }

    pub(crate) fn packed_sidecar_identity(&self, compact: &[u32]) -> u64 {
        let mut id = list::CONTENT_ID_SEED;
        list::mix_content_id(&mut id, compact.len() as u64);
        for &slot in compact {
            list::mix_content_id(&mut id, u64::from(slot));
        }
        list::mix_content_id(&mut id, self.packed.len() as u64);
        for ((packed, emitted), input) in self
            .packed
            .iter()
            .zip(self.emitted.iter())
            .zip(self.input_ranges.iter())
        {
            list::mix_content_id(&mut id, u64::from(emitted.slot_index));
            list::mix_content_id(&mut id, u64::from(emitted.light_index));
            list::mix_content_id(&mut id, input.start as u64);
            list::mix_content_id(&mut id, input.end as u64);
            mix_sidecar(&mut id, &packed.world_draw_indices);
            mix_sidecar(&mut id, &packed.xmodel_draw_indices);
            mix_sidecar(&mut id, &packed.smodel_draw_indices);
            mix_sidecar(&mut id, &packed.smodel_cached_draw_indices);
            mix_sidecar(&mut id, &packed.smodel_pretess_draw_indices);
            mix_sidecar(&mut id, &packed.smodel_skinned_draw_indices);
        }
        id
    }

    pub(crate) fn adopt_frame_slots(
        &mut self,
        compact: &[u32],
        last_id: &mut Option<u64>,
        last_packed: &mut Vec<Arc<PackedFrontendLists>>,
    ) -> Vec<SpotShadowFrameSlot> {
        let id = self.packed_sidecar_identity(compact);
        if *last_id == Some(id) && last_packed.len() == self.emitted.len() {
            let emitted = std::mem::take(&mut self.emitted);
            self.packed.clear();
            self.input_ranges.clear();
            return emitted
                .into_iter()
                .zip(last_packed.iter())
                .map(|(emitted, packed)| SpotShadowFrameSlot {
                    emitted,
                    packed: Arc::clone(packed),
                })
                .collect();
        }
        let slots = self.take_frame_slots(compact);
        *last_id = Some(id);
        last_packed.clear();
        last_packed.extend(slots.iter().map(|slot| Arc::clone(&slot.packed)));
        slots
    }
}

fn mix_sidecar(id: &mut u64, indices: &[u32]) {
    list::mix_content_id(id, indices.len() as u64);
    for &index in indices {
        list::mix_content_id(id, u64::from(index));
    }
}

fn host_dvars(sun_sample_size_near: f32) -> SpotShadowChooseDvars {
    SpotShadowChooseDvars {
        spot_enable: true,
        spot_limit: 4,
        max_lights: 4,
        min_score: 0.0,
        fade_time: 1.0,
        eye_project_dist: 0.0,
        spot_project_frac: 0.0,
        quality_spot_shadow: false,
        spot_dist_cull: false,
        sun_sample_size_near,
    }
}

struct RetainedIdentityIndex<'a> {
    retained: &'a [RetainedDrawItem],
    xmodel_retained: &'a [RetainedDrawItem],
    smodel: HashMap<(u32, u32), usize>,
    world: HashMap<u16, usize>,
    xmodel: HashMap<(u32, u16), usize>,
}

impl<'a> RetainedIdentityIndex<'a> {
    fn new(retained: &'a [RetainedDrawItem]) -> Self {
        Self::with_xmodels(retained, retained)
    }

    fn with_xmodels(
        retained: &'a [RetainedDrawItem],
        xmodel_retained: &'a [RetainedDrawItem],
    ) -> Self {
        let mut smodel = HashMap::new();
        let mut world = HashMap::new();
        let mut xmodel = HashMap::new();
        for (i, item) in retained.iter().enumerate() {
            match item.kind {
                RetainedDrawKind::Smodel {
                    placement, surface, ..
                } => {
                    smodel.entry((placement, surface)).or_insert(i);
                }
                RetainedDrawKind::World { surf, .. } => {
                    world.entry(surf).or_insert(i);
                }
                _ => {}
            }
        }
        for (i, item) in xmodel_retained.iter().enumerate() {
            if let RetainedDrawKind::XModel { surface, .. } = item.kind {
                let object_id = dpvs_iw4::GfxDrawSurf::from_packed(item.key).object_id();
                xmodel.entry((surface, object_id)).or_insert(i);
            }
        }
        Self {
            retained,
            xmodel_retained,
            smodel,
            world,
            xmodel,
        }
    }

    fn smodel(&self, placement: u32, surface: u32) -> Option<&'a RetainedDrawItem> {
        self.smodel
            .get(&(placement, surface))
            .map(|&i| &self.retained[i])
    }

    fn world(&self, surf: u16) -> Option<&'a RetainedDrawItem> {
        self.world.get(&surf).map(|&i| &self.retained[i])
    }

    fn xmodel(&self, surface: u32, object_id: u16) -> Option<&'a RetainedDrawItem> {
        self.xmodel
            .get(&(surface, object_id))
            .map(|&i| &self.xmodel_retained[i])
    }
}

#[must_use]
pub fn retained_xmodel_for_marked(
    mark_row: &[u8],
    plan_draws: &[XModelSurfaceDraw],
    retained: &[RetainedDrawItem],
) -> Vec<RetainedDrawItem> {
    retained_xmodel_for_marked_in(mark_row, plan_draws, &RetainedIdentityIndex::new(retained))
}

fn retained_xmodel_for_marked_in(
    mark_row: &[u8],
    plan_draws: &[XModelSurfaceDraw],
    index: &RetainedIdentityIndex<'_>,
) -> Vec<RetainedDrawItem> {
    let mut out = Vec::new();
    if index.xmodel.is_empty() {
        return out;
    }
    for draw in plan_draws {
        if !spot_shadow_xmodel_is_caster(mark_row, draw.scene_entnum) {
            continue;
        }
        if let Some(&item) = index.xmodel(draw.surface, draw.object_id) {
            out.push(item);
        }
    }
    out
}

fn retained_smodel_for_lod_in(
    ids: &[u16],
    index: &RetainedIdentityIndex<'_>,
    plan: Option<&SmodelGpuPlan>,
    eye: Vec3,
    ramp: LodRampArgs,
) -> Vec<RetainedDrawItem> {
    let Some(plan) = plan else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for &want in ids {
        let Some(placement) = plan.placements.get(usize::from(want)) else {
            continue;
        };
        let Some(mesh) = plan.meshes.get(placement.mesh) else {
            continue;
        };
        let Some(lod) =
            smodel_camera_lod(mesh.lod, placement.origin, placement.scale, Some(eye), ramp)
        else {
            continue;
        };
        let lod_surfs = mesh
            .surfaces_by_lod
            .get(usize::from(lod))
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        for &(surface, _) in lod_surfs {
            if let Some(&item) = index.smodel(u32::from(want), surface) {
                out.push(item);
            }
        }
    }
    out
}

fn retained_world_for_surfs(
    surfs: &[u16],
    index: &RetainedIdentityIndex<'_>,
) -> Vec<RetainedDrawItem> {
    let mut out = Vec::new();
    for &want in surfs {
        if let Some(&item) = index.world(want) {
            out.push(item);
        }
    }
    out
}

fn spot_lights(world: &WorldScene) -> Vec<SpotShadowableLight> {
    let n = world
        .primary_light_cull
        .len()
        .max(world.primary_light_pack.len());
    let mut lights = Vec::with_capacity(n);
    for i in 0..n {
        let cull = world.primary_light_cull.get(i);
        let pack = world.primary_light_pack.get(i);
        let light_type = cull
            .map(|c| c.light_type)
            .or_else(|| pack.map(|p| p.light_type))
            .unwrap_or(0);
        lights.push(SpotShadowableLight {
            can_use_shadow_map: light_type == GFX_LIGHT_TYPE_SPOT,
            color: pack.map(|p| p.color).unwrap_or([1.0; 3]),
            dir: cull.map(|c| c.direction).unwrap_or([0.0, 0.0, 1.0]),
            origin: cull.map(|c| c.origin).unwrap_or([0.0; 3]),
            radius: cull.map(|c| c.radius).unwrap_or(0.0),
            light_type,
            cos_half_fov_expanded: cull.map(|c| c.cos_half_fov_expanded).unwrap_or(0.0),
        });
    }
    lights
}

fn used_from_xmodel(draws: &[XModelSurfaceDraw]) -> [u32; 8] {
    let mut used = [0u32; 8];
    for draw in draws {
        set_used_bit(&mut used, u32::from(draw.scene_light_index));
    }
    used
}

fn used_from_shadow_geom_and_xmodel(world: &WorldScene, draws: &[XModelSurfaceDraw]) -> [u32; 8] {
    let mut used = used_from_xmodel(draws);
    for (i, geom) in world.shadow_geometry.iter().enumerate() {
        if !geom.surfaces.is_empty() || !geom.smodels.is_empty() {
            set_used_bit(&mut used, i as u32);
        }
    }
    used
}

pub fn fill_spot_shadow_caster_plan(
    plan: &mut SpotShadowCasterPlan,
    occ: &SpotShadowSceneOccupancy,
    gfx: Option<&render_frontend::GfxScene>,
    world: &WorldScene,
    view: Option<&PreparedSceneView>,
    xmodel: Option<&XModelDrawPlan>,
    catalog: &super::RuntimeMaterialCatalog,
    retained: &[RetainedDrawItem],
    world_run_surfs: &[u16],
    world_ranges: &[(u32, u32)],
    world_vertex_count: u32,
    smodel_ranges: &[(u32, u32)],
    smodel: Option<&SmodelGpuPlan>,
    lod_ramp: LodRampArgs,
    scene_time: u32,
    sm_enable: Option<bool>,
    sm_sun_enable: Option<bool>,
) {
    plan.clear_frame();
    let lights = spot_lights(world);
    if lights.is_empty() {
        return;
    }
    if !lighting_iw4::generate_runs_choose(sm_enable, None) {
        return;
    }
    let Some(view) = view.filter(|v| v.ready) else {
        return;
    };
    let draws = xmodel.map(|p| p.draws.as_slice()).unwrap_or(&[]);
    let mut used = used_from_shadow_geom_and_xmodel(world, draws);
    if lighting_iw4::generate_clears_used_bits_through_sun(
        sm_sun_enable.unwrap_or(lighting_iw4::SM_SUN_ENABLE_DEFAULT),
        world.sun_primary_light_count,
    ) {
        lighting_iw4::clear_used_bits_1_through_sun(&mut used, world.sun_primary_light_count);
    }
    let Some(choose) = spot_shadow_choose(
        &mut plan.history,
        used,
        &lights,
        view.eye.to_array(),
        view.forward.to_array(),
        scene_time,
        world.sun_primary_light_count,
        lights.len() as u32,
        lights.len() as u32,
        lighting_iw4::SpotShadowUsedForce::None,
        false,
        host_dvars(world.sun_sample_size_near),
        SPOT_SHADOW_SCORE_LUMA,
        None,
    ) else {
        return;
    };
    let fe = spot_shadow_emit_frontend(&choose, &lights, lights.len() as u32, 0.0, false);
    // Selection updates history even when no light emits a shadow this frame.
    // Visibility and caster indices only have consumers in emitted slots.
    if fe
        .slots
        .iter()
        .take(SPOT_SHADOW_SM_LIGHT_CAP)
        .all(Option::is_none)
    {
        return;
    }
    let word_n = spot_shadow_primary_vis_word_count(
        1,
        lighting_iw4::SPOT_SHADOW_CFG_INDEX_ENTS,
        world.sun_primary_light_count,
        lights.len() as u32,
    );
    if word_n == 0 {
        return;
    }
    let mut vis = vec![0u32; word_n];
    if let Some(scene) = gfx {
        for dobj in &scene.scene_dobjs {
            let entnum = render_frontend::scene_info_entnum(dobj.info);
            spot_shadow_link_entity(
                &mut vis,
                0,
                entnum,
                dobj.origin,
                dobj.radius.unwrap_or(0.0),
                &world.primary_light_cull,
                world.sun_primary_light_count,
            );
        }
        for model in &scene.scene_models {
            let entnum = render_frontend::scene_info_entnum(model.info);
            spot_shadow_link_entity(
                &mut vis,
                0,
                entnum,
                model.origin,
                model.radius.unwrap_or(0.0),
                &world.primary_light_cull,
                world.sun_primary_light_count,
            );
        }
    }
    let mut brush_vis = vec![0u32; 512];
    for brush in &world.dyn_ent_brushes {
        spot_shadow_link_dyn_brush_vis(
            &mut brush_vis,
            u32::from(brush.index),
            brush.origin,
            0.0,
            &world.primary_light_cull,
            world.sun_primary_light_count,
        );
    }
    let mut dobj_in = vec![
        lighting_iw4::SpotShadowCasterIn {
            info: 0,
            vis_lit: false,
            bounds_ok: false,
            box_mid: [0.0; 3],
            box_half: [0.0; 3],
        };
        occ.dobjs.len()
    ];
    let mut model_in = vec![
        lighting_iw4::SpotShadowCasterIn {
            info: 0,
            vis_lit: false,
            bounds_ok: false,
            box_mid: [0.0; 3],
            box_half: [0.0; 3],
        };
        occ.models.len()
    ];
    let shadow_materials = xmodel.map_or_else(Vec::new, |plan| {
        super::retained_list::xmodel_shadow_materials(plan, catalog)
    });
    let identity = RetainedIdentityIndex::with_xmodels(retained, &shadow_materials);
    // A scene entity can own many surfaces. Resolve them once in draw order;
    // each shadow slot then visits only that entity's retained surfaces.
    let mut xmodel_by_entity = HashMap::<u32, Vec<RetainedDrawItem>>::new();
    if !identity.xmodel.is_empty() {
        for draw in draws {
            if let Some(entnum) = draw.scene_entnum
                && let Some(&item) = identity.xmodel(draw.surface, draw.object_id)
            {
                xmodel_by_entity.entry(entnum).or_default().push(item);
            }
        }
    }
    for slot in fe.slots.iter().take(SPOT_SHADOW_SM_LIGHT_CAP) {
        let Some(slot) = slot else {
            continue;
        };
        let Some(light) = lights.get(usize::from(slot.light_index)) else {
            continue;
        };
        let Ok(_) = spot_shadow_fill_occupancy(
            &occ.dobjs,
            Some(vis.as_slice()),
            0,
            u32::from(slot.light_index),
            world.sun_primary_light_count,
            lights.len() as u32,
            &mut dobj_in,
        ) else {
            continue;
        };
        let Ok(_) = spot_shadow_fill_occupancy(
            &occ.models,
            Some(vis.as_slice()),
            0,
            u32::from(slot.light_index),
            world.sun_primary_light_count,
            lights.len() as u32,
            &mut model_in,
        ) else {
            continue;
        };
        let mut mark = vec![0u8; SPOT_SHADOW_ENT_MARK_LEN];
        spot_shadow_walk_casters(
            slot.walks_casters,
            light,
            &dobj_in,
            &model_in,
            u32::MAX,
            &mut mark,
        );
        plan.marked_n = plan
            .marked_n
            .saturating_add(mark.iter().filter(|&&b| b != 0).count() as u32);
        let mut bsp_ids = [0u16; 512];
        let geom = world.shadow_geometry.get(usize::from(slot.light_index));
        let bsp_n = spot_shadow_tess_bsp_surfs(
            slot.walks_casters,
            geom.map(|g| g.surfaces.as_slice()).unwrap_or(&[]),
            &mut bsp_ids,
        );
        let mut items = retained_world_for_surfs(&bsp_ids[..bsp_n], &identity);
        let mut smodel_ids = [0u16; 512];
        let sm_n = spot_shadow_filter_smodel_ids(
            slot.walks_casters,
            geom.map(|g| g.smodels.as_slice()).unwrap_or(&[]),
            |id| {
                let inst = world
                    .static_model_instances
                    .get(usize::from(id))
                    .and_then(|inst| inst.as_ref())?;
                if !lighting_iw4::spot_shadow_smodel_casts(inst.flags) {
                    return None;
                }
                let eye = view.eye.to_array();
                let dx = inst.origin[0] - eye[0];
                let dy = inst.origin[1] - eye[1];
                let dz = inst.origin[2] - eye[2];
                let dist_sq = dx * dx + dy * dy + dz * dz;
                if smodel_cull_dist_hides(inst.cull_dist, dist_sq, lod_ramp.scale_last) {
                    return None;
                }
                let mesh = world.static_model_meshes.get(inst.mesh);
                if smodel_camera_lod(
                    mesh.and_then(|m| m.lod),
                    inst.origin,
                    inst.scale,
                    Some(view.eye),
                    lod_ramp,
                )
                .is_none()
                {
                    return None;
                }
                Some(inst.flags)
            },
            &mut smodel_ids,
        );
        items.extend(retained_smodel_for_lod_in(
            &smodel_ids[..sm_n],
            &identity,
            smodel,
            view.eye,
            lod_ramp,
        ));
        for brush in &world.dyn_ent_brushes {
            if brush.brush_model == 0 {
                continue;
            }
            let Some(bit) = spot_shadow_dyn_brush_vis_bit_index(
                u32::from(brush.index),
                u32::from(slot.light_index),
                world.sun_primary_light_count,
                lights.len() as u32,
            ) else {
                continue;
            };
            if !spot_shadow_dyn_brush_tess_admits(
                slot.walks_casters,
                spot_shadow_primary_vis_bit(&brush_vis, bit),
            ) {
                continue;
            }
            let Some(model) = world
                .cull
                .as_ref()
                .and_then(|cull| cull.brush_models.get(usize::from(brush.brush_model)))
            else {
                continue;
            };
            let start = u32::from(model.start_surf);
            let count = u32::from(model.surface_count);
            let ids: Vec<u16> = (start..start.saturating_add(count))
                .filter_map(|s| u16::try_from(s).ok())
                .collect();
            items.extend(retained_world_for_surfs(&ids, &identity));
        }
        let mut dobj_vis = [0u8; lighting_iw4::SPOT_SHADOW_SCENE_DOBJ_VIS_STRIDE
            * lighting_iw4::SPOT_SHADOW_SM_LIGHT_CAP];
        let cam = gfx
            .filter(|s| s.scene_ent_walked)
            .map(|s| s.scene_ent_visible.as_slice());
        let entnums: Vec<u32> = gfx
            .map(|s| {
                s.scene_dobjs
                    .iter()
                    .map(|d| render_frontend::scene_info_entnum(d.info))
                    .collect()
            })
            .unwrap_or_default();
        let pose_ok: Vec<bool> = gfx
            .map(|s| {
                s.scene_dobjs
                    .iter()
                    .map(|d| lighting_iw4::spot_shadow_175d0_pose_ok(d.cull_gate))
                    .collect()
            })
            .unwrap_or_default();
        spot_shadow_fill_scene_dobj_vis(
            &mut dobj_vis,
            slot.slot_index,
            &entnums,
            cam,
            Some(pose_ok.as_slice()),
        );
        if let (Some(scene), Some(_)) = (gfx, xmodel) {
            let mut idxs = [0u32; 512];
            let vis_n = spot_shadow_tess_scene_dobj_indices(
                &dobj_vis,
                slot.slot_index,
                scene.scene_dobjs.len() as u32,
                &mut idxs,
            );
            for &si in &idxs[..vis_n] {
                let Some(dobj) = scene.scene_dobjs.get(si as usize) else {
                    continue;
                };
                let entnum = render_frontend::scene_info_entnum(dobj.info);
                if entnum < SPOT_SHADOW_ENT_MARK_LEN as u32
                    && let Some(draws) = xmodel_by_entity.get(&entnum)
                {
                    items.extend_from_slice(draws);
                }
            }
            let mut model_vis = [0u8; lighting_iw4::SPOT_SHADOW_SCENE_MODEL_VIS_STRIDE
                * lighting_iw4::SPOT_SHADOW_SM_LIGHT_CAP];
            let model_entnums: Vec<u32> = scene
                .scene_models
                .iter()
                .map(|m| render_frontend::scene_info_entnum(m.info))
                .collect();
            spot_shadow_fill_scene_model_vis(
                &mut model_vis,
                slot.slot_index,
                &model_entnums,
                cam,
                None,
            );
            let vis_n = spot_shadow_tess_scene_model_indices(
                &model_vis,
                slot.slot_index,
                scene.scene_models.len() as u32,
                &mut idxs,
            );
            for &si in &idxs[..vis_n] {
                let Some(model) = scene.scene_models.get(si as usize) else {
                    continue;
                };
                let entnum = render_frontend::scene_info_entnum(model.info);
                if entnum < SPOT_SHADOW_ENT_MARK_LEN as u32
                    && let Some(draws) = xmodel_by_entity.get(&entnum)
                {
                    items.extend_from_slice(draws);
                }
            }
        }
        if let Some(xmodel) = xmodel {
            items.extend(retained_xmodel_for_marked_in(
                &mark,
                &xmodel.draws,
                &identity,
            ));
        }
        let mut model_surfaces = HashSet::new();
        items.retain(|item| match item.kind {
            RetainedDrawKind::XModel {
                surface, object_id, ..
            } => model_surfaces.insert((surface, object_id)),
            _ => true,
        });
        let xmodel_ranges = xmodel.map(|p| p.range_rows()).unwrap_or(&[]);
        let pack_draws = items
            .iter()
            .map(super::retained_list::pack_draw)
            .collect::<Vec<_>>();
        let packed = pack_spot_shadow_frontend(
            &pack_draws,
            world_run_surfs,
            world_ranges,
            world_vertex_count,
            smodel_ranges,
            xmodel_ranges,
        );
        if spot_shadow_packed_lists_ready(packed.work_entry_n()) {
            let start = plan.items.len();
            plan.items.extend(items);
            plan.input_ranges.push(start..plan.items.len());
            plan.emitted.push(*slot);
            plan.packed.push(packed);
        }
    }
}
