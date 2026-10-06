use bevy::platform::collections::HashSet;

use bevy::prelude::*;
use dpvs_iw4::{
    AabbCullStats, AabbTreeCull, BspDrawSurfCensus, BspDrawSurfRun, CellClipPlanes,
    CellPortalGraph, DpvsPlanes, DpvsVisData, GfxDrawSurf, PortalView, VisBits, WalkScratch,
    add_aabb_tree_surfaces_in_frustum, add_bsp_draw_surfs_camera, add_sky_surfaces_dpvs,
    admit_cell_root_span, cell_caster_row_words, msb_get, or_caster_rows_for_visible,
    smodel_cull_dist_hides, visit_cells, words_for_bits,
};

use crate::prepare::scene::view_parms::PreparedSceneView;
use crate::prepare::scene::world::{WorldDrawItem, WorldDrawItemKind, WorldScene};

#[derive(Resource, Clone, Debug, Default)]
pub struct DpvsFrameStats {
    pub eye_cell: Option<usize>,

    pub eye_cell_unresolved: u32,
    pub visible_cells: u32,

    pub portals_frustum_skipped: u32,

    pub child_planes_unported: u32,

    pub aabb_nodes_visited: u32,

    pub aabb_planes_dropped: u32,

    pub surfaces_rejected_by_bounds: u32,

    pub surfaces_admitted_unbounded: u32,
    pub smodels_rejected_by_bounds: u32,
    pub smodels_admitted_unbounded: u32,

    pub bsp_visible_surfaces: u32,

    pub bsp_admitted_surfaces: u32,
    pub bsp_run_n: u32,

    pub bsp_input_gap_n: u32,

    pub bsp_ranges_unavailable: u32,
    pub keys: u32,

    pub material_ordinal_refused: u32,
    pub rebinds: u32,
    pub surfaces: u32,
    pub triangles: u32,
    pub lightmapped_triangles: u32,
    pub fallback_triangles: u32,
    pub unculled_triangles: u32,
    pub single_cell: bool,
    pub aspect: f32,
    pub fov_deg: f32,

    pub cell_static_started: Option<std::time::Instant>,

    pub submitted_batches: u32,

    pub rewritten_index_bytes: u64,

    pub visibility_changes: u32,

    pub sky_surf_n: u32,

    pub sky_mesh_n: u32,

    pub sky_vis_n: u32,

    pub sky_drawn_n: u32,

    pub sky_admitted: u32,

    pub view_prepared: u8,

    pub lock_pvs: u8,

    pub smodel_vis: Vec<u8>,

    pub smodel_vis_id: u64,

    pub g0_world_surfs: Vec<u16>,

    pub frustum_planes: Vec<[f32; 4]>,

    pub cell_vis: Vec<u32>,
    pub cell_vis_count: usize,

    pub cell_clips: Vec<CellClipPlanes>,

    pub cell_vis_all: bool,
}

const DRAW_DECALS: bool = true;

fn single_cell_from_env() -> bool {
    matches!(
        std::env::var("IW4L_SINGLE_CELL").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

#[derive(Component)]
pub struct WorldMeshEntity;

#[derive(Component)]
pub struct StaticModelEntity;

#[derive(Component)]
pub struct ScriptModelEntity;

pub use render_scene::DynEntModelEntity;

#[derive(Component)]
pub struct ScriptModelGameObject(pub String);

pub(crate) fn smodel_cull_dist_skips_slot(
    cull_dists: &[u16],
    slot: usize,
    origin: [f32; 3],
    eye: Option<Vec3>,
    scale_last: Option<f32>,
) -> bool {
    let Some(eye) = eye else {
        return false;
    };
    let cull_dist = cull_dists.get(slot).copied().unwrap_or(0);
    let dx = eye.x - origin[0];
    let dy = eye.y - origin[1];
    let dz = eye.z - origin[2];
    smodel_cull_dist_hides(cull_dist, dx * dx + dy * dy + dz * dz, scale_last)
}

pub fn apply_dpvs_cull(
    mut scene: ResMut<WorldScene>,
    mut stats: ResMut<DpvsFrameStats>,
    mut published_vis: ResMut<render_scene::PublishedCellVis>,
    prepared: Res<PreparedSceneView>,
    lock_pvs: Res<crate::prepare::scene::view_parms::RLockPvs>,
) {
    let _cull = perf::Span::RenderCullCpuMs.enter();
    stats.frustum_planes.clear();
    stats.smodel_vis.clear();
    stats.smodel_vis_id = 0;
    stats.cell_vis.clear();
    stats.cell_vis_count = 0;
    stats.cell_clips.clear();
    stats.cell_vis_all = false;
    stats.view_prepared = 0;
    stats.lock_pvs = 0;
    *published_vis = render_scene::PublishedCellVis::default();
    let Some(cull) = scene.cull.as_mut() else {
        return;
    };

    if cull.batch_count == 0 && cull.static_model_entities.is_empty() {
        return;
    }
    if !prepared.ready {
        return;
    }
    stats.view_prepared = 1;
    stats.lock_pvs = u8::from(lock_pvs.enabled && lock_pvs.frozen.is_some());

    let eye_pos = lock_pvs.dpvs_eye(&prepared);
    let eye = [eye_pos.x, eye_pos.y, eye_pos.z];
    let (fov, aspect) = lock_pvs.dpvs_fov_aspect(&prepared);
    let dpvs = &cull.dpvs;
    if dpvs.cell_count == 0 {
        return;
    }

    let frustum = lock_pvs.dpvs_frustum(&prepared).to_vec();
    stats.frustum_planes.clone_from(&frustum);

    let planes = DpvsPlanes {
        planes: &dpvs.planes,
        nodes: &dpvs.nodes,
        cell_count: dpvs.cell_count as u32,
    };

    let eye_cell = planes.cell_for_point(eye);
    let single_cell = single_cell_from_env();

    let mut per_cell: Vec<Vec<PortalView<'_>>> = Vec::with_capacity(dpvs.cell_count);
    for cell_portals in &dpvs.portals_per_cell {
        let mut views = Vec::with_capacity(cell_portals.len());
        for p in cell_portals {
            let end = p.vert_start + p.vert_count;
            let verts = dpvs.portal_verts.get(p.vert_start..end).unwrap_or(&[]);
            views.push(PortalView {
                plane: p.plane,
                neighbor: p.neighbor,
                vertices: verts,
                hull_axis: p.hull_axis,
            });
        }
        per_cell.push(views);
    }
    let slices: Vec<&[PortalView<'_>]> = per_cell.iter().map(|v| v.as_slice()).collect();
    let graph = CellPortalGraph { portals: &slices };

    let mut words = vec![0u32; words_for_bits(dpvs.cell_count.max(1))];
    let mut vis = VisBits::new(&mut words);
    let mut scratch = WalkScratch::new();
    let mut walk_stats = dpvs_iw4::WalkStats::default();
    match eye_cell {
        Some(cell) if single_cell => vis.set(cell),

        Some(cell) => {
            let mut cell_clips = vec![CellClipPlanes::EMPTY; dpvs.cell_count];
            let bevels = lock_pvs.dpvs_portal_bevels(&prepared);
            walk_stats = visit_cells(
                &graph,
                cell,
                eye,
                dpvs_iw4::dpvs_view_plane(lock_pvs.dpvs_forward(&prepared).to_array(), eye),
                &frustum,
                0,
                &mut vis,
                &mut scratch,
                Some(&mut cell_clips),
                bevels.as_ref(),
            );
            stats.cell_clips = cell_clips;
        }

        None => {
            for cell in 0..dpvs.cell_count {
                vis.set(cell);
            }
        }
    }

    let surf_count = cull.surface_batch_ranges.len();
    cull.surface_vis.clear();
    cull.surface_vis.resize(surf_count, 0);
    cull.smodel_vis.clear();
    cull.smodel_vis.resize(cull.static_model_entities.len(), 0);
    cull.draw_items.clear();
    cull.g0_surfs.clear();
    stats.g0_world_surfs.clear();

    let mut aabb_stats = AabbCullStats::default();
    {
        let mut vis_data = DpvsVisData {
            surface_vis: &mut cull.surface_vis,
            smodel_vis: &mut cull.smodel_vis,
        };
        for cell in 0..dpvs.cell_count {
            if !vis.get(cell) {
                continue;
            }
            let Some(tree) = dpvs.aabb_trees.get(cell) else {
                continue;
            };
            if tree.is_empty() {
                if let Some(root) = dpvs.cell_roots.get(cell)
                    && root.count > 0
                {
                    admit_cell_root_span(
                        &dpvs.sorted_surf_index,
                        root.start,
                        root.count,
                        &mut vis_data,
                        &mut aabb_stats,
                    );
                }
                continue;
            }
            let cull_input = AabbTreeCull {
                nodes: tree,
                smodel_indexes: dpvs
                    .aabb_smodel_indices
                    .get(cell)
                    .map_or(&[][..], Vec::as_slice),
                sorted_surf_index: &dpvs.sorted_surf_index,
                surfaces_bounds: &dpvs.surface_bounds,
                smodel_bounds: &dpvs.smodel_bounds,
                draw_decals: DRAW_DECALS,
            };
            add_aabb_tree_surfaces_in_frustum(
                &cull_input,
                &frustum,
                &mut vis_data,
                &mut aabb_stats,
            );
        }
    }

    let mut planes_without_far = frustum.clone();
    if planes_without_far.len() >= 2 {
        planes_without_far.remove(1);
    }
    let sky_admit = add_sky_surfaces_dpvs(
        &dpvs.sky_start_surfs,
        &dpvs.surface_bounds,
        &planes_without_far,
        &mut cull.surface_vis,
    );
    stats.sky_surf_n = dpvs.sky_start_surfs.len() as u32;
    stats.sky_mesh_n = dpvs
        .sky_start_surfs
        .iter()
        .filter(|&&surf| {
            cull.surface_index_ranges
                .get(surf as usize)
                .is_some_and(|&(_, n)| n > 0)
        })
        .count() as u32;
    stats.sky_vis_n = dpvs
        .sky_start_surfs
        .iter()
        .filter(|&&surf| cull.surface_vis.get(surf as usize).copied().unwrap_or(0) != 0)
        .count() as u32;
    stats.sky_admitted = sky_admit.admitted;
    stats.material_ordinal_refused = 0;

    let mut bsp_census = BspDrawSurfCensus::default();
    for range in dpvs.camera_ranges.iter() {
        let range = append_camera_bsp_range(
            range.kind,
            range.begin,
            range.end,
            &mut cull.surface_vis,
            &cull.surface_draw_fields,
            &cull.capture.packed_draw_surfs,
            &cull.surface_materials,
            &mut cull.bsp_run_scratch,
            &mut cull.draw_items,
            &mut cull.g0_surfs,
        );
        bsp_census.range_n = bsp_census.range_n.saturating_add(range.range_n);
        bsp_census.visible_n = bsp_census.visible_n.saturating_add(range.visible_n);
        bsp_census.admitted_n = bsp_census.admitted_n.saturating_add(range.admitted_n);
        bsp_census.run_n = bsp_census.run_n.saturating_add(range.run_n);
        bsp_census.input_gap_n = bsp_census.input_gap_n.saturating_add(range.input_gap_n);
        bsp_census.output_overflow_n = bsp_census
            .output_overflow_n
            .saturating_add(range.output_overflow_n);
    }
    stats.bsp_visible_surfaces = bsp_census.visible_n;
    stats.bsp_admitted_surfaces = bsp_census.admitted_n;
    stats.bsp_run_n = bsp_census.run_n;
    stats.bsp_input_gap_n = bsp_census
        .input_gap_n
        .saturating_add(bsp_census.output_overflow_n);
    stats.material_ordinal_refused = stats
        .material_ordinal_refused
        .saturating_add(bsp_census.input_gap_n);

    let mut draw_items_id = crate::assemble::drawsurf::list::CONTENT_ID_SEED;
    crate::assemble::drawsurf::list::mix_content_id(
        &mut draw_items_id,
        cull.draw_items.len() as u64,
    );
    stats.sky_drawn_n = cull
        .draw_items
        .iter()
        .map(|item| {
            (0..item.run)
                .filter(|offset| {
                    let surf = item.surf.saturating_add(*offset);
                    dpvs.sky_start_surfs.iter().any(|&s| s == u32::from(surf))
                })
                .count() as u32
        })
        .sum();

    let batch_count = cull.batch_count as usize;
    let mut batch_visible = vec![false; batch_count];
    let mut surfaces = 0u32;
    let mut prev_mat: Option<u16> = None;
    let mut rebinds = 0u32;
    let mut lightmapped_tris = 0u32;
    let mut fallback_tris = 0u32;
    for item in &cull.draw_items {
        crate::assemble::drawsurf::list::mix_content_id(&mut draw_items_id, item.key);
        crate::assemble::drawsurf::list::mix_content_id(&mut draw_items_id, u64::from(item.surf));
        crate::assemble::drawsurf::list::mix_content_id(&mut draw_items_id, u64::from(item.run));
        crate::assemble::drawsurf::list::mix_content_id(
            &mut draw_items_id,
            match item.kind {
                WorldDrawItemKind::Bsp(asset_world::CameraRangeKind::LitOpaque) => 0,
                WorldDrawItemKind::Bsp(asset_world::CameraRangeKind::LitTrans) => 1,
                WorldDrawItemKind::Bsp(asset_world::CameraRangeKind::Emissive) => 2,
                WorldDrawItemKind::BModel => 3,
                WorldDrawItemKind::Bsp(asset_world::CameraRangeKind::Decal) => 4,
            },
        );
        crate::assemble::drawsurf::list::mix_content_id(
            &mut draw_items_id,
            u64::from(item.setup_key_changed),
        );
        let mat = ((item.key >> 30) & 0xfff) as u16;
        if prev_mat != Some(mat) {
            rebinds += 1;
            prev_mat = Some(mat);
        }
        for offset in 0..item.run {
            let surf_usize = usize::from(item.surf.saturating_add(offset));
            let Some(&(batch, _s0, sc)) = cull.surface_batch_ranges.get(surf_usize) else {
                continue;
            };
            if sc == 0 {
                continue;
            }
            surfaces = surfaces.saturating_add(1);
            if let Some(slot) = batch_visible.get_mut(batch) {
                *slot = true;
            }
            let tris = sc / 3;
            if cull.batch_lightmapped.get(batch).copied().unwrap_or(false) {
                lightmapped_tris = lightmapped_tris.saturating_add(tris);
            } else {
                fallback_tris = fallback_tris.saturating_add(tris);
            }
        }
    }
    let tris = lightmapped_tris.saturating_add(fallback_tris);
    stats.eye_cell = eye_cell;
    if eye_cell.is_none() {
        stats.eye_cell_unresolved = stats.eye_cell_unresolved.saturating_add(1);
    }
    stats.visible_cells = vis.count_ones();
    drop(vis);
    stats.cell_vis.clone_from(&words);
    stats.cell_vis_count = dpvs.cell_count;
    stats.cell_vis_all = eye_cell.is_none();
    cull.cell_vis.clone_from(&words);
    cull.cell_vis_all = eye_cell.is_none();
    *published_vis = render_scene::PublishedCellVis {
        words: words.clone(),
        cell_count: dpvs.cell_count,
        vis_all: eye_cell.is_none(),
    };
    stats.portals_frustum_skipped = walk_stats.skip_clip;
    stats.child_planes_unported = walk_stats.child_planes_unported;
    stats.aabb_nodes_visited = aabb_stats.nodes_visited;
    stats.aabb_planes_dropped = aabb_stats.planes_dropped;
    stats.surfaces_rejected_by_bounds = aabb_stats.surfaces_rejected_by_bounds;
    stats.surfaces_admitted_unbounded = aabb_stats.surfaces_admitted_unbounded;
    stats.smodels_rejected_by_bounds = aabb_stats.smodels_rejected_by_bounds;
    stats.smodels_admitted_unbounded = aabb_stats.smodels_admitted_unbounded;
    stats.keys = cull.draw_items.len() as u32;
    cull.draw_items_id = draw_items_id;
    stats.g0_world_surfs.clone_from(&cull.g0_surfs);
    stats.rebinds = rebinds;
    stats.surfaces = surfaces;
    stats.triangles = tris;
    stats.lightmapped_triangles = lightmapped_tris;
    stats.fallback_triangles = fallback_tris;
    stats.unculled_triangles = (cull.packed_indices.len() / 3) as u32;
    stats.single_cell = single_cell;
    stats.aspect = aspect;
    stats.fov_deg = fov.to_degrees();

    let submitted_batches = batch_visible.iter().filter(|&&v| v).count() as u32;
    perf::Counter::CounterSubmittedBatches.emit(f64::from(submitted_batches));

    stats.submitted_batches = submitted_batches;
    stats.rewritten_index_bytes = 0;
    stats.visibility_changes = 0;
    stats.smodel_vis.clear();
    stats.smodel_vis.extend_from_slice(&cull.smodel_vis);
    let mut smodel_vis_id = crate::assemble::drawsurf::list::CONTENT_ID_SEED;
    crate::assemble::drawsurf::list::mix_content_bytes(&mut smodel_vis_id, &cull.smodel_vis);
    stats.smodel_vis_id = smodel_vis_id;
}

pub(crate) fn log_script_model_gaps_once(
    props: Query<&ScriptModelGameObject>,
    mut done: Local<bool>,
) {
    if *done || props.is_empty() {
        return;
    }
    *done = true;
    let mut names: Vec<&str> = props.iter().map(|prop| prop.0.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    diag::info!(
        World,
        "script_model gameobjects after _gameobjects::main: {} props kept, tags={:?}",
        props.iter().count(),
        names
    );
}

pub(crate) fn log_dpvs_stats_once(stats: Res<DpvsFrameStats>, mut done: Local<bool>) {
    if *done || stats.unculled_triangles == 0 {
        return;
    }
    *done = true;
    diag::info!(
        World,
        "dpvs frame: eye_cell={:?} unresolved={} visible_cells={} portals_frustum_skipped={} \
         child_planes_unported={} aabb_nodes={} planes_dropped={} \
         surf_rejected_by_bounds={} surf_unbounded={} smodel_rejected_by_bounds={} \
         smodel_unbounded={} draw_decals={} bsp_visible={} bsp_admitted={} bsp_runs={} \
         bsp_input_gap={} bsp_ranges_unavailable={} keys={} ordinal_refused={} g0_world={} rebinds={} surfaces={} \
         tris={}/{} (lightmapped={} fallback={}) single_cell={} batches={} rewrite_bytes={} \
         vis_chg={} sky_surf={} sky_mesh={} sky_vis={} sky_drawn={} sky_admit={} \
         aspect={:.3} fov_deg={:.1}",
        stats.eye_cell,
        stats.eye_cell_unresolved,
        stats.visible_cells,
        stats.portals_frustum_skipped,
        stats.child_planes_unported,
        stats.aabb_nodes_visited,
        stats.aabb_planes_dropped,
        stats.surfaces_rejected_by_bounds,
        stats.surfaces_admitted_unbounded,
        stats.smodels_rejected_by_bounds,
        stats.smodels_admitted_unbounded,
        DRAW_DECALS,
        stats.bsp_visible_surfaces,
        stats.bsp_admitted_surfaces,
        stats.bsp_run_n,
        stats.bsp_input_gap_n,
        stats.bsp_ranges_unavailable,
        stats.keys,
        stats.material_ordinal_refused,
        stats.g0_world_surfs.len(),
        stats.rebinds,
        stats.surfaces,
        stats.triangles,
        stats.unculled_triangles,
        stats.lightmapped_triangles,
        stats.fallback_triangles,
        stats.single_cell,
        stats.submitted_batches,
        stats.rewritten_index_bytes,
        stats.visibility_changes,
        stats.sky_surf_n,
        stats.sky_mesh_n,
        stats.sky_vis_n,
        stats.sky_drawn_n,
        stats.sky_admitted,
        stats.aspect,
        stats.fov_deg,
    );
}

pub(crate) fn add_world_surfaces_frustum_only(
    dpvs: &crate::prepare::scene::world::WorldDpvs,
    frustum: &[[f32; 4]],
    camera_cell_vis_lsb: &[u32],
    camera_vis_all: bool,
    surface_vis: &mut [u8],
    smodel_vis: &mut [u8],
    draw_msb: &mut Vec<u32>,
) -> (AabbCullStats, u32) {
    let mut stats = AabbCullStats::default();
    let mut vis_data = DpvsVisData {
        surface_vis,
        smodel_vis,
    };
    let words = cell_caster_row_words(dpvs.cell_count);
    if draw_msb.len() != words {
        draw_msb.clear();
        draw_msb.resize(words, 0);
    } else {
        draw_msb.fill(0);
    }
    let draw_n = if dpvs.cell_caster_bits.is_empty() {
        for cell in 0..dpvs.cell_count {
            add_one_cell_frustum_only(dpvs, cell, frustum, &mut vis_data, &mut stats);
        }
        dpvs.cell_count as u32
    } else {
        let n = or_caster_rows_for_visible(
            &dpvs.cell_caster_bits,
            dpvs.cell_count,
            camera_cell_vis_lsb,
            camera_vis_all,
            &mut draw_msb[..],
        );
        for cell in 0..dpvs.cell_count {
            if !msb_get(&draw_msb, cell) {
                continue;
            }
            add_one_cell_frustum_only(dpvs, cell, frustum, &mut vis_data, &mut stats);
        }
        n
    };
    (stats, draw_n)
}

fn append_camera_bsp_range(
    kind: asset_world::CameraRangeKind,
    begin: u32,
    end: u32,
    surface_vis: &mut [u8],
    surfaces: &[asset_world::SurfaceDrawFields],
    draw_surfs: &[GfxDrawSurf],
    surface_materials: &[Option<assets::MaterialIndex>],
    scratch: &mut Vec<BspDrawSurfRun<asset_world::CameraRangeKind>>,
    draw_items: &mut Vec<WorldDrawItem>,
    g0_surfs: &mut Vec<u16>,
) -> BspDrawSurfCensus {
    let range_n = end.saturating_sub(begin);
    if end < begin {
        return BspDrawSurfCensus {
            input_gap_n: 1,
            ..BspDrawSurfCensus::default()
        };
    }
    let (Ok(begin_i), Ok(end_i)) = (usize::try_from(begin), usize::try_from(end)) else {
        return BspDrawSurfCensus {
            range_n,
            input_gap_n: range_n.max(1),
            ..BspDrawSurfCensus::default()
        };
    };
    if end_i > surface_vis.len() || end_i > surfaces.len() || end_i > draw_surfs.len() {
        let visible_n = surface_vis
            .get(begin_i..surface_vis.len().min(end_i))
            .map_or(0, |bytes| {
                bytes.iter().filter(|byte| **byte != 0).count() as u32
            });
        return BspDrawSurfCensus {
            range_n,
            visible_n,
            input_gap_n: range_n.max(1),
            ..BspDrawSurfCensus::default()
        };
    }

    let mut host_input_gap_n = 0u32;
    for surf in begin_i..end_i {
        if surface_vis[surf] == 0 || draw_surfs[surf].packed != 0 {
            continue;
        }
        surface_vis[surf] = 0;
        host_input_gap_n = host_input_gap_n.saturating_add(1);
        if let Ok(surf) = u16::try_from(surf) {
            g0_surfs.push(surf);
        }
    }

    let placeholder = BspDrawSurfRun {
        kind,
        first_surf: 0,
        surf_count: 0,
        draw_surf: GfxDrawSurf::from_packed(0),
        setup_key_changed: false,
    };
    scratch.clear();
    scratch.resize(end_i - begin_i, placeholder);

    let mut census = BspDrawSurfCensus {
        range_n,
        ..BspDrawSurfCensus::default()
    };
    let material = |surf: usize| surface_materials.get(surf).copied().flatten();
    let mut span_begin = begin;
    let mut previous_key = None;
    while (span_begin as usize) < end_i {
        let head = span_begin as usize;
        let visible = surface_vis[head] != 0;
        let span_len = surface_vis[head..end_i]
            .iter()
            .enumerate()
            .position(|(offset, &vis)| {
                (vis != 0) != visible || (visible && material(head + offset) != material(head))
            })
            .unwrap_or(end_i - head);
        let span_end = span_begin + span_len as u32;
        if visible {
            let output = &mut scratch[census.run_n as usize..];
            let part = add_bsp_draw_surfs_camera(
                kind,
                span_begin,
                span_end,
                surface_vis,
                surfaces,
                draw_surfs,
                output,
            );
            let emitted = part.run_n.saturating_sub(part.output_overflow_n) as usize;
            if let Some(first) = output.get_mut(..emitted).and_then(|runs| runs.first_mut()) {
                let key = dpvs_iw4::bsp_draw_surf_setup_key(first.draw_surf);
                first.setup_key_changed = previous_key.is_some_and(|previous| previous != key);
                previous_key = Some(dpvs_iw4::bsp_draw_surf_setup_key(
                    output[emitted - 1].draw_surf,
                ));
            }
            census.visible_n += part.visible_n;
            census.admitted_n += part.admitted_n;
            census.run_n += part.run_n;
            census.input_gap_n += part.input_gap_n;
            census.output_overflow_n += part.output_overflow_n;
        }
        span_begin = span_end;
    }
    census.visible_n = census.visible_n.saturating_add(host_input_gap_n);
    census.input_gap_n = census.input_gap_n.saturating_add(host_input_gap_n);
    let emitted_n = census.run_n.saturating_sub(census.output_overflow_n) as usize;
    for run in scratch.iter().take(emitted_n) {
        draw_items.push(WorldDrawItem {
            key: run.draw_surf.packed,
            surf: run.first_surf,
            run: run.surf_count,
            kind: WorldDrawItemKind::Bsp(run.kind),
            setup_key_changed: run.setup_key_changed,
        });
    }
    census
}

fn add_one_cell_frustum_only(
    dpvs: &crate::prepare::scene::world::WorldDpvs,
    cell: usize,
    frustum: &[[f32; 4]],
    vis_data: &mut DpvsVisData<'_>,
    stats: &mut AabbCullStats,
) {
    let Some(tree) = dpvs.aabb_trees.get(cell) else {
        return;
    };
    if tree.is_empty() {
        if let Some(root) = dpvs.cell_roots.get(cell)
            && root.count > 0
        {
            admit_cell_root_span(
                &dpvs.sorted_surf_index,
                root.start,
                root.count,
                vis_data,
                stats,
            );
        }
        return;
    }
    let cull_input = AabbTreeCull {
        nodes: tree,
        smodel_indexes: dpvs
            .aabb_smodel_indices
            .get(cell)
            .map_or(&[][..], Vec::as_slice),
        sorted_surf_index: &dpvs.sorted_surf_index,
        surfaces_bounds: &dpvs.surface_bounds,
        smodel_bounds: &dpvs.smodel_bounds,
        draw_decals: DRAW_DECALS,
    };
    add_aabb_tree_surfaces_in_frustum(&cull_input, frustum, vis_data, stats);
}

pub(crate) fn packed_world_colour_key(
    cull: &crate::prepare::scene::world::WorldCull,
    surf: usize,
    baked_material_keys: &[Option<u64>],
    batch_primary_lights: &[u8],
) -> Option<u64> {
    let material_asset_id = cull.surface_materials.get(surf).copied().flatten()?;
    if let Some(word) = cull
        .capture
        .packed_draw_surfs
        .get(surf)
        .copied()
        .filter(|word| word.packed != 0)
    {
        return Some(word.packed);
    }
    let packed = baked_material_keys
        .get(material_asset_id.order())
        .copied()
        .flatten()?;
    let scene_light_index = if surf < cull.surface_primary_lights.len() {
        cull.surface_primary_lights[surf]
    } else {
        cull.surface_batch_ranges
            .get(surf)
            .and_then(|&(batch, _, _)| batch_primary_lights.get(batch).copied())
            .unwrap_or(0)
    };
    Some(crate::assemble::drawsurf::with_scene_light_index(
        packed,
        scene_light_index,
    ))
}

pub(crate) fn append_bmodel_colour_span(
    cull: &mut crate::prepare::scene::world::WorldCull,
    start_surf: u16,
    surface_count: u16,
    already: &mut HashSet<u16>,
    baked_material_keys: &[Option<u64>],
    batch_primary_lights: &[u8],
) -> (u32, u32) {
    let mut added = 0u32;
    let mut refused = 0u32;
    let start = usize::from(start_surf);
    let end = start.saturating_add(usize::from(surface_count));
    for surf in start..end {
        let Ok(surf_u16) = u16::try_from(surf) else {
            continue;
        };
        if !already.insert(surf_u16) {
            continue;
        }
        match packed_world_colour_key(cull, surf, baked_material_keys, batch_primary_lights) {
            Some(packed) => {
                cull.draw_items.push(WorldDrawItem {
                    key: packed,
                    surf: surf_u16,
                    run: 1,
                    kind: WorldDrawItemKind::BModel,
                    setup_key_changed: false,
                });
                added = added.saturating_add(1);
            }
            None => refused = refused.saturating_add(1),
        }
    }
    (added, refused)
}

pub(crate) fn refresh_draw_items_id(cull: &mut crate::prepare::scene::world::WorldCull) {
    let mut draw_items_id = crate::assemble::drawsurf::list::CONTENT_ID_SEED;
    crate::assemble::drawsurf::list::mix_content_id(
        &mut draw_items_id,
        cull.draw_items.len() as u64,
    );
    for item in &cull.draw_items {
        crate::assemble::drawsurf::list::mix_content_id(&mut draw_items_id, item.key);
        crate::assemble::drawsurf::list::mix_content_id(&mut draw_items_id, u64::from(item.surf));
        crate::assemble::drawsurf::list::mix_content_id(&mut draw_items_id, u64::from(item.run));
        crate::assemble::drawsurf::list::mix_content_id(
            &mut draw_items_id,
            match item.kind {
                WorldDrawItemKind::Bsp(asset_world::CameraRangeKind::LitOpaque) => 0,
                WorldDrawItemKind::Bsp(asset_world::CameraRangeKind::LitTrans) => 1,
                WorldDrawItemKind::Bsp(asset_world::CameraRangeKind::Emissive) => 2,
                WorldDrawItemKind::BModel => 3,
                WorldDrawItemKind::Bsp(asset_world::CameraRangeKind::Decal) => 4,
            },
        );
        crate::assemble::drawsurf::list::mix_content_id(
            &mut draw_items_id,
            u64::from(item.setup_key_changed),
        );
    }
    cull.draw_items_id = draw_items_id;
}

#[must_use]
pub(crate) fn draw_item_rebinds(items: &[WorldDrawItem]) -> u32 {
    let mut prev_mat: Option<u16> = None;
    let mut rebinds = 0u32;
    for item in items {
        let mat = ((item.key >> 30) & 0xfff) as u16;
        if prev_mat != Some(mat) {
            rebinds = rebinds.saturating_add(1);
            prev_mat = Some(mat);
        }
    }
    rebinds
}
