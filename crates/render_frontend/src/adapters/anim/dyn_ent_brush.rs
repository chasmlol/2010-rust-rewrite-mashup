use bevy::platform::collections::HashSet;

use bevy::prelude::*;
use dpvs_iw4::{
    Bounds, DpvsPlanes, GFX_CFG_ENT_COUNT, dyn_brush_scene_list_admits, dyn_ent_in_cell,
    filter_bmodel_into_cells, filter_dyn_ent_into_cells, msb_iter, scene_ent_cell_bits_len,
    scene_ent_cell_row, scene_ent_cell_row_second_pass, scene_ent_cell_walk_bits,
    scene_ent_inner_planes, write_dyn_brush_vis,
};
use render_frontend::{
    CellFrustumWorkerCmd, DpvsEntWorkerCmd, SCENE_INDEX_EMPTY, WORKER_CMD_CELL_DYN_BRUSH,
    WORKER_CMD_CELL_SCENE_ENT, WORKER_CMD_DPVS_ENT, WorkerCmdBusyInput, scene_info_entnum,
};

use crate::adapters::anim::dyn_ent::{
    DynEntBrushPrimaryLightVis, DynEntCellBits, ensure_primary_light_words,
    relink_dyn_ent_primary_lights,
};
use crate::prepare::scene::cull::{
    DpvsFrameStats, append_bmodel_colour_span, draw_item_rebinds, refresh_draw_items_id,
};
use crate::prepare::scene::gfx_scene::HostGfxScene;
use crate::prepare::scene::smodel_geom_cache::FrontendWorkerCmds;
use crate::prepare::scene::world::WorldScene;

#[derive(Resource, Clone, Debug, Default)]
pub struct DynEntBrushCellBits {
    pub membership: DynEntCellBits,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct SceneEntCellBits {
    pub cell_count: usize,
    pub bits: Vec<u32>,

    pub ent_info: Vec<Option<Bounds>>,
}

impl SceneEntCellBits {
    fn ensure(&mut self, cell_count: usize) {
        let len = scene_ent_cell_bits_len(cell_count);
        let n = GFX_CFG_ENT_COUNT as usize;
        if self.cell_count == cell_count && self.bits.len() == len && self.ent_info.len() == n {
            return;
        }
        self.cell_count = cell_count;
        self.bits = vec![0; len];
        self.ent_info = vec![None; n];
    }

    fn is_ready(&self) -> bool {
        self.cell_count > 0
            && self.bits.len() == scene_ent_cell_bits_len(self.cell_count)
            && self.ent_info.len() == GFX_CFG_ENT_COUNT as usize
    }
}

pub fn register_dyn_ent_brush_systems(app: &mut App) {
    app.init_resource::<DynEntBrushCellBits>()
        .init_resource::<DynEntBrushPrimaryLightVis>()
        .init_resource::<SceneEntCellBits>()
        .add_systems(
            Update,
            (
                link_dyn_ent_brush_cells,
                exec_cell_dyn_brush_cmds
                    .after(link_dyn_ent_brush_cells)
                    .after(frame::WorkerCmdSet::CellStatic),
            )
                .in_set(frame::RenderSet::Anim)
                .in_set(frame::WorkerCmdSet::CellDynBrush),
        )
        .add_systems(
            Update,
            (
                size_scene_ent_cell_bits,
                link_scene_ents
                    .after(size_scene_ent_cell_bits)
                    .after(crate::prepare::scene::gfx_scene::GfxSceneAdd),
                exec_cell_scene_ent_cmds
                    .after(link_scene_ents)
                    .after(frame::WorkerCmdSet::CellStatic),
            )
                .in_set(frame::RenderSet::Anim)
                .in_set(frame::WorkerCmdSet::CellSceneEnt),
        )
        .add_systems(
            Update,
            drain_dpvs_ent_cmds
                .after(frame::WorkerCmdSet::CellSceneEnt)
                .in_set(frame::WorkerCmdSet::DpvsEnt),
        );
}

fn link_dyn_ent_brush_cells(
    scene: Option<Res<WorldScene>>,
    atpoint: Res<render_scene::DynAtPointLookup>,
    mut membership: ResMut<DynEntBrushCellBits>,
    mut vis: ResMut<DynEntBrushPrimaryLightVis>,
) {
    let Some(scene) = scene.as_ref() else {
        membership.membership = DynEntCellBits::default();
        *vis = DynEntBrushPrimaryLightVis::default();
        return;
    };
    let Some(cull) = scene.cull.as_ref() else {
        membership.membership = DynEntCellBits::default();
        *vis = DynEntBrushPrimaryLightVis::default();
        return;
    };
    if cull.dpvs.nodes.is_empty() || cull.dpvs.cell_count == 0 {
        membership.membership = DynEntCellBits::default();
        *vis = DynEntBrushPrimaryLightVis::default();
        return;
    }
    let dpvs = DpvsPlanes {
        planes: &cull.dpvs.planes,
        nodes: &cull.dpvs.nodes,
        cell_count: cull.dpvs.cell_count as u32,
    };
    let client_count = scene
        .dyn_ent_brushes
        .iter()
        .map(|b| u32::from(b.index) + 1)
        .max()
        .unwrap_or(0);
    let _ = membership
        .membership
        .ensure(client_count, cull.dpvs.cell_count);
    membership.membership.bits.fill(0);
    vis.sun_primary = scene.sun_primary_light_count;
    vis.primary_count = scene.primary_light_cull.len() as u32;
    let sun = vis.sun_primary;
    let primary_count = vis.primary_count;
    ensure_primary_light_words(&mut vis.words, client_count, sun, primary_count);
    vis.words.fill(0);
    for brush in &scene.dyn_ent_brushes {
        let Some(bounds) = brush.bounds else {
            continue;
        };
        let word_count = membership.membership.word_count;
        filter_dyn_ent_into_cells(
            &dpvs,
            bounds,
            &mut membership.membership.bits,
            word_count,
            u32::from(brush.index),
        );
        relink_dyn_ent_primary_lights(
            &mut vis.words,
            None,
            &atpoint,
            u32::from(brush.index),
            bounds,
            false,
        );
    }
}

fn exec_cell_dyn_brush_cmds(
    mut scene: Option<ResMut<WorldScene>>,
    mut stats: Option<ResMut<DpvsFrameStats>>,
    membership: Res<DynEntBrushCellBits>,
    mut worker_cmds: ResMut<FrontendWorkerCmds>,
) {
    let planes: Vec<[f32; 4]> = stats
        .as_ref()
        .map(|s| s.frustum_planes.clone())
        .unwrap_or_default();
    let brushes: Vec<_> = scene
        .as_ref()
        .map(|s| s.dyn_ent_brushes.clone())
        .unwrap_or_default();
    let bits = &membership.membership;
    let vis_len = brushes
        .iter()
        .map(|brush| usize::from(brush.index) + 1)
        .max()
        .unwrap_or(0);
    let mut vis = vec![0u8; vis_len];
    let _ = worker_cmds.queues.wait_of_type(
        WORKER_CMD_CELL_DYN_BRUSH,
        WorkerCmdBusyInput::default(),
        |data| {
            let Some(cmd) = CellFrustumWorkerCmd::from_bytes(data) else {
                return;
            };
            if !bits.is_ready() {
                return;
            }
            let cell = cmd.cell as usize;
            for brush in &brushes {
                let id = u32::from(brush.index);
                if !dyn_ent_in_cell(&bits.bits, bits.word_count, cell, id) {
                    continue;
                }
                let Some(bounds) = brush.bounds else {
                    continue;
                };
                let Some(slot) = vis.get_mut(usize::from(brush.index)) else {
                    continue;
                };
                let _ = write_dyn_brush_vis(slot, bounds, &planes);
            }
        },
    );
    let Some(scene) = scene.as_mut() else {
        return;
    };
    let admitted: Vec<u16> = scene
        .dyn_ent_brushes
        .iter()
        .filter_map(|brush| {
            let vis_byte = vis.get(usize::from(brush.index)).copied().unwrap_or(0);
            if dyn_brush_scene_list_admits(vis_byte, brush.surface_count) {
                Some(brush.brush_model)
            } else {
                None
            }
        })
        .collect();
    if admitted.iter().all(|&brush_model| brush_model == 0) {
        return;
    }
    let baked_material_keys: Vec<Option<u64>> = scene
        .runtime_material_catalog
        .parts()
        .materials
        .iter()
        .map(|material| material.baked_draw_surf)
        .collect();
    let batch_primary_lights: Vec<u8> = scene
        .batches
        .iter()
        .map(|batch| batch.primary_light_index)
        .collect();
    let Some(cull) = scene.cull.as_mut() else {
        return;
    };
    let mut already = HashSet::new();
    for item in &cull.draw_items {
        for offset in 0..item.run {
            already.insert(item.surf.saturating_add(offset));
        }
    }
    let mut added = 0u32;
    for brush_model in admitted {
        if brush_model == 0 {
            continue;
        }
        let Some(model) = cull.brush_models.get(usize::from(brush_model)).copied() else {
            continue;
        };
        let (span_added, _) = append_bmodel_colour_span(
            cull,
            model.start_surf,
            model.surface_count,
            &mut already,
            &baked_material_keys,
            &batch_primary_lights,
        );
        added = added.saturating_add(span_added);
    }
    if added == 0 {
        return;
    }
    refresh_draw_items_id(cull);
    if let Some(stats) = stats.as_mut() {
        stats.keys = cull.draw_items.len() as u32;
        stats.rebinds = draw_item_rebinds(&cull.draw_items);
    }
}

fn size_scene_ent_cell_bits(scene: Option<Res<WorldScene>>, mut bits: ResMut<SceneEntCellBits>) {
    let Some(scene) = scene.as_ref() else {
        *bits = SceneEntCellBits::default();
        return;
    };
    let Some(cull) = scene.cull.as_ref() else {
        *bits = SceneEntCellBits::default();
        return;
    };
    bits.ensure(cull.dpvs.cell_count);
}

#[derive(Default)]
struct SceneEntCellCache {
    tree: (usize, usize),
    by_ent: bevy::platform::collections::HashMap<u32, ([u32; 6], Vec<u32>)>,
    linked: HashSet<u32>,
}

impl SceneEntCellCache {
    fn link(&mut self, dpvs: &DpvsPlanes<'_>, bits: &mut [u32], bounds: Bounds, ent_id: u32) {
        let (mid, half) = (bounds.mid(), bounds.half());
        let key = [
            mid[0].to_bits(),
            mid[1].to_bits(),
            mid[2].to_bits(),
            half[0].to_bits(),
            half[1].to_bits(),
            half[2].to_bits(),
        ];
        if !self.linked.insert(ent_id) {
            dpvs_iw4::unfilter_scene_ent_from_cells_view0(bits, dpvs.cell_count as usize, ent_id);
        }
        let entry = self
            .by_ent
            .entry(ent_id)
            .or_insert_with(|| (key, Vec::new()));
        if entry.0 != key || entry.1.is_empty() {
            entry.0 = key;
            entry.1.clear();
            let cells = &mut entry.1;
            dpvs_iw4::scene_ent_cells(dpvs, bounds, &mut |cell| cells.push(cell));
        }
        for &cell in &entry.1 {
            dpvs_iw4::add_scene_ent_to_cell(bits, cell as usize, ent_id);
        }
    }
}

fn link_scene_ents(
    scene: Option<Res<WorldScene>>,
    gfx: Res<HostGfxScene>,
    mut bits: ResMut<SceneEntCellBits>,
    mut cache: Local<SceneEntCellCache>,
) {
    if !bits.is_ready() {
        return;
    }
    bits.bits.fill(0);
    bits.ent_info.fill(None);
    let Some(scene) = scene.as_ref() else {
        return;
    };
    let Some(cull) = scene.cull.as_ref() else {
        return;
    };
    if cull.dpvs.nodes.is_empty() || cull.dpvs.cell_count == 0 {
        return;
    }
    let dpvs = DpvsPlanes {
        planes: &cull.dpvs.planes,
        nodes: &cull.dpvs.nodes,
        cell_count: cull.dpvs.cell_count as u32,
    };
    let tree = (cull.dpvs.nodes.as_ptr() as usize, cull.dpvs.nodes.len());
    if cache.tree != tree {
        *cache = SceneEntCellCache {
            tree,
            ..SceneEntCellCache::default()
        };
    }
    cache.linked.clear();
    for model in &gfx.scene.scene_models {
        if let Some(bounds) = scene_ent_pose_bounds(model.origin, model.radius, model.posed_bounds)
        {
            cache.link(&dpvs, &mut bits.bits, bounds, scene_info_entnum(model.info));
        }
    }
    for dobj in &gfx.scene.scene_dobjs {
        if let Some(bounds) = scene_ent_pose_bounds(dobj.origin, dobj.radius, dobj.posed_bounds) {
            cache.link(&dpvs, &mut bits.bits, bounds, scene_info_entnum(dobj.info));
        }
    }
    for brush in &gfx.scene.scene_brushes {
        let Some(ent_id) = brush.param_4.map(u32::from) else {
            continue;
        };
        let Some(local) = cull
            .brush_model_bounds
            .get(brush.model_index as usize)
            .copied()
        else {
            continue;
        };
        let quat = brush.quat.unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let bounds = crate::prepare::scene::world::posed_brush_bounds(brush.origin, quat, local);
        filter_bmodel_into_cells(&dpvs, bounds, &mut bits.bits, ent_id);
        if let Some(slot) = bits.ent_info.get_mut(ent_id as usize) {
            *slot = Some(bounds);
        }
    }
}

fn scene_ent_pose_bounds(
    origin: [f32; 3],
    radius: Option<f32>,
    posed_bounds: Option<Bounds>,
) -> Option<Bounds> {
    posed_bounds.or_else(|| radius.map(|r| Bounds::from_mid_half(origin, [r, r, r])))
}

fn reaches_cell(dpvs: Option<&DpvsPlanes<'_>>, bounds: Bounds, cell: usize) -> bool {
    match dpvs {
        Some(dpvs) => dpvs_iw4::scene_ent_box_reaches_cell(dpvs, bounds, cell),
        None => true,
    }
}

fn skin_scene_ent_and_mark(
    scene: &mut render_frontend::GfxScene,
    skin_inputs: &mut crate::prepare::scene::gfx_scene::SceneEntSkinInputs,
    surface_cache: &mut crate::prepare::scene::gfx_scene::SceneEntSurfaceCache,
    entnum: u32,
) {
    let gate = scene.scene_dobj(entnum).map(|dobj| dobj.cull_gate);
    if let Some(mut gate) = gate {
        if dpvs_iw4::scene_dobj_gate_skin_begin(&mut gate) {
            let ready = skin_inputs.by_ent.get(&entnum).cloned();
            let expanded = ready.or_else(|| {
                skin_inputs.pending.get(&entnum).map(|pending| {
                    crate::prepare::scene::gfx_scene::expand_scene_ent_pending(
                        pending,
                        surface_cache,
                    )
                })
            });
            let mut entries = Vec::new();
            let summary = match expanded.as_ref() {
                Some(input) => {
                    let models: Vec<dpvs_iw4::PreSkinModel<'_>> = input
                        .models
                        .iter()
                        .map(|model| dpvs_iw4::PreSkinModel {
                            lod: model.lod,
                            bone_count: model.bone_count,
                            surfaces: &model.surfaces,
                        })
                        .collect();
                    dpvs_iw4::pre_skin_scene_ent(
                        &models,
                        &input.hide_part_bits,
                        skin_inputs.frame_bytes_used,
                        |entry| entries.push(entry),
                    )
                }

                None => dpvs_iw4::PreSkinSummary::default(),
            };
            skin_inputs.frame_bytes_used =
                skin_inputs.frame_bytes_used.saturating_add(summary.bytes);
            scene.store_scene_ent_skin(
                entnum,
                render_frontend::SceneEntSkinnedSurfs { entries, summary },
            );
        }
    }
    scene.mark_scene_ent_visible(entnum);
}

fn exec_cell_scene_ent_cmds(
    bits: Res<SceneEntCellBits>,
    mut gfx: ResMut<HostGfxScene>,
    mut skin_inputs: ResMut<crate::prepare::scene::gfx_scene::SceneEntSkinInputs>,
    mut surface_cache: ResMut<crate::prepare::scene::gfx_scene::SceneEntSurfaceCache>,
    world: Option<Res<WorldScene>>,
    stats: Option<Res<DpvsFrameStats>>,
    mut worker_cmds: ResMut<FrontendWorkerCmds>,
) {
    let _ = worker_cmds.queues.reset_type(WORKER_CMD_DPVS_ENT);
    let ready = bits.is_ready();
    gfx.scene.scene_ent_walked = false;
    let cell_count = bits.cell_count;
    let planes: Vec<[f32; 4]> = stats
        .as_ref()
        .map(|s| s.frustum_planes.clone())
        .unwrap_or_default();
    let mut pending: Vec<DpvsEntWorkerCmd> = Vec::new();
    let cull = world.as_ref().and_then(|w| w.cull.as_ref());
    let dpvs = cull.map(|cull| DpvsPlanes {
        planes: &cull.dpvs.planes,
        nodes: &cull.dpvs.nodes,
        cell_count: cull.dpvs.cell_count as u32,
    });
    let inputs = &mut *skin_inputs;
    let cache = &mut *surface_cache;
    let scene = &mut gfx.scene;
    let _ = worker_cmds.queues.wait_of_type(
        WORKER_CMD_CELL_SCENE_ENT,
        WorkerCmdBusyInput::default(),
        |data| {
            if !ready {
                return;
            }
            let Some(cmd) = CellFrustumWorkerCmd::from_bytes(data) else {
                return;
            };
            let cell = cmd.cell as usize;
            if cell >= cell_count {
                return;
            }
            let row0 = scene_ent_cell_row(&bits.bits, cell);
            let row1 = scene_ent_cell_row_second_pass(&bits.bits, cell_count, cell);

            scene.scene_ent_walked = true;
            let bit_count = scene_ent_cell_walk_bits(GFX_CFG_ENT_COUNT);
            let cell_planes: Vec<[f32; 4]> = stats
                .as_ref()
                .and_then(|s| {
                    s.cell_clips
                        .get(cell)
                        .filter(|c| c.plane_count > 0)
                        .map(|c| c.as_slice().to_vec())
                })
                .unwrap_or_else(|| planes.clone());
            let sphere_planes =
                scene_ent_inner_planes(&cell_planes, cmd.plane_begin, cmd.plane_count);
            for (second_pass, row) in [(false, row0), (true, row1)] {
                for entnum in msb_iter(row, bit_count) {
                    let id = entnum as u32;

                    if scene.scene_ent_visible(id) {
                        continue;
                    }
                    let dobj_slot = scene
                        .scene_dobj_index
                        .get(entnum)
                        .copied()
                        .filter(|slot| *slot != SCENE_INDEX_EMPTY);
                    if second_pass {
                        let Some(bounds) = bits.ent_info.get(entnum).copied().flatten() else {
                            continue;
                        };
                        if dpvs_iw4::scene_ent_frustum_hides(bounds, sphere_planes) {
                            continue;
                        }
                        scene.mark_scene_ent_visible(id);
                        continue;
                    } else if let Some(slot) = dobj_slot {
                        let Some(dobj) = scene.scene_dobjs.get(usize::from(slot)) else {
                            continue;
                        };
                        let (Some(radius), Some(bounds)) = (dobj.radius, dobj.posed_bounds) else {
                            pending.push(DpvsEntWorkerCmd {
                                scene_dobj: u32::from(slot),
                                plane_count: u16::from(cmd.plane_count),
                                cell: u16::try_from(cmd.cell).unwrap_or(u16::MAX),
                            });
                            continue;
                        };
                        if dpvs_iw4::scene_ent_sphere_hides(dobj.origin, radius, sphere_planes) {
                            continue;
                        }
                        if dpvs_iw4::scene_ent_frustum_hides(bounds, &cell_planes) {
                            continue;
                        }
                        if dpvs_iw4::scene_ent_needs_bound_worker(dobj.cull_gate) {
                            pending.push(DpvsEntWorkerCmd {
                                scene_dobj: u32::from(slot),
                                plane_count: u16::from(cmd.plane_count),
                                cell: u16::try_from(cmd.cell).unwrap_or(u16::MAX),
                            });
                            continue;
                        }

                        if dobj.cull_gate != dpvs_iw4::SCENE_DOBJ_GATE_FAILED {
                            if !reaches_cell(dpvs.as_ref(), bounds, cell) {
                                continue;
                            }
                            skin_scene_ent_and_mark(scene, inputs, cache, id);
                        }
                        continue;
                    }
                    let model_slot = scene
                        .scene_model_index
                        .get(entnum)
                        .copied()
                        .filter(|slot| *slot != SCENE_INDEX_EMPTY);
                    let Some(slot) = model_slot else {
                        continue;
                    };
                    let Some(model) = scene.scene_models.get(usize::from(slot)) else {
                        continue;
                    };
                    let origin = model.origin;
                    let radius = model.radius.unwrap_or(0.0);
                    if model.radius.is_some()
                        && dpvs_iw4::scene_ent_sphere_hides(origin, radius, sphere_planes)
                    {
                        continue;
                    }

                    scene.mark_scene_ent_visible(id);
                }
            }
        },
    );
    for cmd in &pending {
        let _ = worker_cmds.queues.add(WORKER_CMD_DPVS_ENT, &cmd.to_bytes());
    }
}

fn drain_dpvs_ent_cmds(
    mut worker_cmds: ResMut<FrontendWorkerCmds>,
    mut gfx: ResMut<HostGfxScene>,
    mut skin_inputs: ResMut<crate::prepare::scene::gfx_scene::SceneEntSkinInputs>,
    mut surface_cache: ResMut<crate::prepare::scene::gfx_scene::SceneEntSurfaceCache>,
    world: Option<Res<WorldScene>>,
    stats: Option<Res<DpvsFrameStats>>,
) {
    let planes: Vec<[f32; 4]> = stats
        .as_ref()
        .map(|s| s.frustum_planes.clone())
        .unwrap_or_default();
    let cull = world.as_ref().and_then(|w| w.cull.as_ref());
    let dpvs = cull.map(|cull| DpvsPlanes {
        planes: &cull.dpvs.planes,
        nodes: &cull.dpvs.nodes,
        cell_count: cull.dpvs.cell_count as u32,
    });
    let inputs = &mut *skin_inputs;
    let cache = &mut *surface_cache;
    let scene = &mut gfx.scene;
    let _ = worker_cmds.queues.wait_of_type(
        WORKER_CMD_DPVS_ENT,
        WorkerCmdBusyInput::default(),
        |data| {
            let Some(cmd) = DpvsEntWorkerCmd::from_bytes(data) else {
                return;
            };
            let Some(dobj) = scene.scene_dobjs.get_mut(cmd.scene_dobj as usize) else {
                return;
            };
            if !dpvs_iw4::scene_dobj_gate_begin(&mut dobj.cull_gate) {
                return;
            }
            let Some(bounds) = dobj.posed_bounds else {
                dobj.cull_gate = dpvs_iw4::SCENE_DOBJ_GATE_FAILED;
                return;
            };
            dobj.cull_gate = dpvs_iw4::SCENE_DOBJ_GATE_BOUNDED;
            let entnum = scene_info_entnum(dobj.info);
            if dpvs_iw4::scene_ent_frustum_hides(bounds, &planes) {
                return;
            }

            if !reaches_cell(dpvs.as_ref(), bounds, usize::from(cmd.cell)) {
                return;
            }
            skin_scene_ent_and_mark(scene, inputs, cache, entnum);
        },
    );
}
