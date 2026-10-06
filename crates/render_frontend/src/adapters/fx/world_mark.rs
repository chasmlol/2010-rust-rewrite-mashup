use std::cell::RefCell;

use fx::FxSystemHost;
use marks_iw4::{FxAllocMarkRequest, MarkFragmentsAgainst, impact_mark_material};

use crate::prepare::scene::world::WorldScene;
use render_fx::EntityMarks;

pub struct FrontendFxScene<'a> {
    world: &'a WorldScene,
    marks: &'a EntityMarks,
}

impl<'a> FrontendFxScene<'a> {
    pub fn wrap(scene: Option<&'a WorldScene>, marks: &'a EntityMarks) -> Option<Self> {
        scene.map(|world| Self { world, marks })
    }
}

struct MarkBoxScratch {
    cell_bits: Vec<u32>,
    surf_bits: Vec<u32>,
    smodel_bits: Vec<u32>,
    list_n: usize,
    smodel_n: usize,
}

impl MarkBoxScratch {
    fn prepare(&mut self, cell_n: usize, list_n: usize, smodel_n: usize) {
        fn words(v: &mut Vec<u32>, n: usize) {
            let need = marks_iw4::mark_box_surfaces_words(n as u32);
            if v.len() != need {
                v.resize(need, 0);
            } else {
                v.fill(0);
            }
        }
        words(&mut self.cell_bits, cell_n);
        words(&mut self.surf_bits, list_n);
        words(&mut self.smodel_bits, smodel_n);
        self.list_n = list_n;
        self.smodel_n = smodel_n;
    }

    fn surf_any(&self) -> bool {
        self.surf_bits.iter().any(|&word| word != 0)
    }

    fn smodel_any(&self) -> bool {
        self.smodel_n > 0 && self.smodel_bits.iter().any(|&word| word != 0)
    }
}

struct MarkClipScratch {
    mids: Vec<[f32; 3]>,
    halves: Vec<[f32; 3]>,
    ranges: Vec<(u32, u32)>,
    contexts: Vec<[u8; 7]>,
    tris: Vec<marks_iw4::FxMarkStagingTri>,
    points: Vec<marks_iw4::FxMarkStagingPoint>,
    hit_smodels: Vec<usize>,
}

impl MarkClipScratch {
    fn ensure_stage_bufs(&mut self) {
        let tri_n = marks_iw4::R_MARK_FRAGMENTS_MAX_TRIS as usize;
        let point_n = marks_iw4::R_MARK_FRAGMENTS_MAX_POINTS as usize;
        if self.tris.len() != tri_n {
            self.tris.clear();
            self.tris.resize(tri_n, marks_iw4::FxMarkStagingTri::ZERO);
        }
        if self.points.len() != point_n {
            self.points.clear();
            self.points
                .resize(point_n, marks_iw4::FxMarkStagingPoint::ZERO);
        }
    }

    fn clear_filter(&mut self) {
        self.mids.clear();
        self.halves.clear();
        self.ranges.clear();
        self.contexts.clear();
    }
}

thread_local! {
    static MARK_BOX_SCRATCH: RefCell<MarkBoxScratch> = const {
        RefCell::new(MarkBoxScratch {
            cell_bits: Vec::new(),
            surf_bits: Vec::new(),
            smodel_bits: Vec::new(),
            list_n: 0,
            smodel_n: 0,
        })
    };
    static MARK_CLIP_SCRATCH: RefCell<MarkClipScratch> = const {
        RefCell::new(MarkClipScratch {
            mids: Vec::new(),
            halves: Vec::new(),
            ranges: Vec::new(),
            contexts: Vec::new(),
            tris: Vec::new(),
            points: Vec::new(),
            hit_smodels: Vec::new(),
        })
    };
}

fn pose_material<'a>(host: &'a FxSystemHost, against: MarkFragmentsAgainst) -> Option<&'a str> {
    impact_mark_material(
        [
            host.last_decal_mat0.as_deref(),
            host.last_decal_mat1.as_deref(),
        ],
        against,
    )
}

pub(crate) struct WorldMarkPose<'a> {
    pub origin: [f32; 3],
    pub radius: f32,
    pub axis: Option<[[f32; 3]; 3]>,

    pub material: Option<&'a str>,
}

struct SurfaceMarkLookup {
    lmap: u8,
    primary: u8,
    probe: u8,
}

pub(crate) fn runtime_material_by_name<'a>(
    scene: &'a WorldScene,
    name: &str,
) -> Option<&'a crate::assemble::drawsurf::RuntimeMaterial> {
    let want = asset_core::AssetRef::bare_name(name);
    scene
        .runtime_material_catalog
        .parts()
        .materials
        .iter()
        .find(|material| asset_core::AssetRef::bare_name(&material.name) == want)
}

fn mark_material_surface_bits(scene: &WorldScene, name: &str) -> Option<u32> {
    runtime_material_by_name(scene, name).and_then(|material| material.surface_type_bits)
}

pub(crate) fn receiver_allow_inputs(
    scene: &WorldScene,
    _surf: usize,
    mat_slot: Option<assets::MaterialIndex>,
) -> (Option<u8>, Option<u32>) {
    let Some(material) = mat_slot.and_then(|id| scene.runtime_material_catalog.derived(id)) else {
        return (None, None);
    };
    (Some(material.info_game_flags), material.surface_type_bits)
}

fn mark_cell_trees(
    dpvs: &crate::prepare::scene::world::WorldDpvs,
) -> Vec<marks_iw4::MarkCellAabbTree<'_>> {
    let empty: &[u16] = &[];
    dpvs.aabb_trees
        .iter()
        .enumerate()
        .map(|(i, nodes)| marks_iw4::MarkCellAabbTree {
            nodes,
            smodel_indexes: dpvs
                .aabb_smodel_indices
                .get(i)
                .map(|v| v.as_slice())
                .unwrap_or(empty),
        })
        .collect()
}

fn run_mark_box_surfaces(
    scene: &WorldScene,
    origin: [f32; 3],
    radius: f32,
) -> Option<marks_iw4::MarkBoxSurfacesCensus> {
    let cull = scene.cull.as_ref()?;
    let dpvs = &cull.dpvs;
    if dpvs.nodes.is_empty() || dpvs.cell_count == 0 || dpvs.surface_bounds.is_empty() {
        return None;
    }
    let list_n = dpvs.static_surface_count;
    if list_n == 0 || dpvs.sorted_surf_index.is_empty() {
        return None;
    }
    let trees = mark_cell_trees(dpvs);
    if trees.len() < dpvs.cell_count {
        return None;
    }
    let smodel_n = dpvs.smodel_bounds.len();
    MARK_BOX_SCRATCH.with(|scratch| {
        let mut scratch = scratch.borrow_mut();
        scratch.prepare(dpvs.cell_count, list_n, smodel_n);
        let MarkBoxScratch {
            cell_bits,
            surf_bits,
            smodel_bits,
            ..
        } = &mut *scratch;
        let smodel_slice = if smodel_n > 0 {
            Some(smodel_bits.as_mut_slice())
        } else {
            None
        };
        let planes = dpvs_iw4::DpvsPlanes {
            planes: &dpvs.planes,
            nodes: &dpvs.nodes,
            cell_count: dpvs.cell_count as u32,
        };
        let mut box_q = marks_iw4::MarkBoxSurfaces {
            planes,
            origin,
            radius,
            trees: &trees,
            surf_bits,
            surf_bit_count: list_n as u32,
            smodel_bits: smodel_slice,
            smodel_bit_count: smodel_n as u32,
            cell_bits,
        };
        Some(marks_iw4::mark_box_surfaces(&mut box_q))
    })
}

struct WorldFilterMeta {
    allow: marks_iw4::MarkWorldAllowCensus,
    mark_surface_type_bits: Option<u32>,
    unknown_receiver_material: Option<String>,
}

fn fill_world_filter(scene: &WorldScene, pose: &WorldMarkPose<'_>) -> Option<WorldFilterMeta> {
    let cull = scene.cull.as_ref()?;
    let dpvs = &cull.dpvs;
    let mark_bits = pose
        .material
        .and_then(|name| mark_material_surface_bits(scene, name));
    MARK_CLIP_SCRATCH.with(|clip_cell| {
        let mut clip = clip_cell.borrow_mut();
        clip.clear_filter();
        MARK_BOX_SCRATCH.with(|box_cell| {
            let bits = box_cell.borrow();
            let list_n = bits.list_n;
            if list_n == 0 {
                return None;
            }
            let radius_sq = pose.radius * pose.radius;
            let mut allow = marks_iw4::MarkWorldAllowCensus {
                decal_list_n: list_n as u32,
                ..marks_iw4::MarkWorldAllowCensus::default()
            };
            let mut unknown_receiver_material = None;
            for bit in dpvs_iw4::msb_iter(&bits.surf_bits, list_n) {
                let Some(surf) = marks_iw4::mark_sorted_bit_surf(&dpvs.sorted_surf_index, bit)
                else {
                    continue;
                };
                let Some(bounds) = dpvs.surface_bounds.get(surf) else {
                    continue;
                };
                if !marks_iw4::mark_sphere_hits_bounds(
                    pose.origin,
                    radius_sq,
                    bounds.mid(),
                    bounds.half(),
                ) {
                    continue;
                }
                allow.sphere_hit = allow.sphere_hit.saturating_add(1);
                let mat_slot = cull.surface_materials.get(surf).copied().flatten();
                let (flags, recv_bits) = receiver_allow_inputs(scene, surf, mat_slot);
                let decision = marks_iw4::mark_allow(flags, recv_bits, mark_bits);
                match decision {
                    marks_iw4::FxMarkAllow::Keep => allow.keep = allow.keep.saturating_add(1),
                    marks_iw4::FxMarkAllow::Reject => allow.reject = allow.reject.saturating_add(1),
                    marks_iw4::FxMarkAllow::Unknown => {
                        allow.unknown = allow.unknown.saturating_add(1)
                    }
                }
                if decision == marks_iw4::FxMarkAllow::Unknown
                    && unknown_receiver_material.is_none()
                {
                    unknown_receiver_material = Some(
                        mat_slot
                            .and_then(|id| scene.runtime_material_catalog.derived(id))
                            .map(|material| material.name.clone())
                            .unwrap_or_else(|| "<missing>".to_owned()),
                    );
                }
                if !marks_iw4::mark_include_in_world_clip(decision) {
                    continue;
                }
                let Some(&(start, count)) = cull.surface_index_ranges.get(surf) else {
                    continue;
                };
                clip.mids.push(bounds.mid());
                clip.halves.push(bounds.half());
                clip.ranges.push((start, count));
                let lookup = lookup_world_surface_mark(scene, surf);
                clip.contexts
                    .push(marks_iw4::mark_context_from_world_surface(
                        lookup.lmap,
                        lookup.primary,
                        lookup.probe,
                    ));
            }
            Some(WorldFilterMeta {
                allow,
                mark_surface_type_bits: mark_bits,
                unknown_receiver_material,
            })
        })
    })
}

fn lookup_world_surface_mark(scene: &WorldScene, surf: usize) -> SurfaceMarkLookup {
    let none = SurfaceMarkLookup {
        lmap: marks_iw4::GFX_SURFACE_LIGHTMAP_NONE,
        primary: 0,
        probe: 0,
    };
    let Some(cull) = scene.cull.as_ref() else {
        return none;
    };
    let primary = cull.surface_primary_lights.get(surf).copied().unwrap_or(0);
    let lmap = cull
        .surface_lightmap_indices
        .get(surf)
        .copied()
        .unwrap_or(marks_iw4::GFX_SURFACE_LIGHTMAP_NONE);
    let probe = cull
        .surface_reflection_probes
        .get(surf)
        .copied()
        .unwrap_or(0);
    SurfaceMarkLookup {
        lmap,
        primary,
        probe,
    }
}

fn first_tri_context(tri: &marks_iw4::FxMarkStagingTri) -> u32 {
    u32::from_le_bytes([
        tri.context[0],
        tri.context[1],
        tri.context[2],
        tri.context[3],
    ])
}

fn impact_mark_axis(host: &FxSystemHost) -> Option<[[f32; 3]; 3]> {
    Some(fx_iw4::impact_mark_axis(
        host.last_decal_axis?,
        host.last_decal_rotation?,
    ))
}

fn pose_from_fx_host(host: &FxSystemHost) -> Option<WorldMarkPose<'_>> {
    if host.last_decal_against_world != Some(true) {
        return None;
    }
    Some(WorldMarkPose {
        origin: host.last_decal_origin?,
        radius: host.last_decal_size0?,
        axis: Some(impact_mark_axis(host)?),
        material: pose_material(host, MarkFragmentsAgainst::WorldBrushes),
    })
}

pub(crate) fn box_surfaces_prelude_ran(host: &FxSystemHost, scene: &WorldScene) -> bool {
    let Some(pose) = (|| {
        Some(WorldMarkPose {
            origin: host.last_decal_origin?,
            radius: host.last_decal_size0?,
            axis: Some(impact_mark_axis(host)?),
            material: pose_material(host, MarkFragmentsAgainst::WorldBrushes),
        })
    })() else {
        return false;
    };
    run_mark_box_surfaces(scene, pose.origin, pose.radius).is_some()
}

pub(crate) fn finish_impact_marks(
    host: &mut FxSystemHost,
    scene: &WorldScene,
    marks: &EntityMarks,
    def_index: u8,
    against_world: bool,
    against_models: bool,
) {
    if against_models {
        super::entity_mark::queue(host, marks);
    }
    if against_world {
        complete_glass_marks(host, scene);
    }
    let want_world = against_world && pose_from_fx_host(host).is_some();
    let want_models = against_models && pose_from_fx_host_models(host).is_some();
    let spatial = pose_from_fx_host(host).or_else(|| pose_from_fx_host_models(host));
    let Some(spatial) = spatial else {
        if against_world {
            host.note_box_surfaces_skip();
            host.note_world_go_skip(def_index);
        }
        if against_models {
            host.note_models_go_skip(def_index);
        }
        return;
    };
    let origin = spatial.origin;
    let radius = spatial.radius;
    if run_mark_box_surfaces(scene, origin, radius).is_none() {
        if against_world {
            host.note_box_surfaces_skip();
            host.note_world_go_skip(def_index);
        }
        if against_models {
            host.note_models_go_skip(def_index);
        }
        return;
    }
    let surf_any = MARK_BOX_SCRATCH.with(|s| s.borrow().surf_any());
    let smodel_any = MARK_BOX_SCRATCH.with(|s| s.borrow().smodel_any());
    if want_world {
        if surf_any {
            complete_world_generate(host, scene, def_index);
        } else {
            host.note_box_surfaces_run();
            host.note_world_go_skip(def_index);
        }
    } else if against_world {
        host.note_box_surfaces_skip();
        host.note_world_go_skip(def_index);
    } else {
        host.note_box_surfaces_run();
    }
    if want_models {
        if smodel_any {
            complete_models_generate(host, scene, def_index);
        } else {
            host.note_models_go_skip(def_index);
        }
    } else if against_models {
        host.note_models_go_skip(def_index);
    }
}

impl render_fx::present::FxScene for FrontendFxScene<'_> {
    fn sample_atpoint_rgb(&self, origin: [f32; 3]) -> Option<[u8; 3]> {
        self.world
            .light_grid
            .as_ref()
            .and_then(|grid| asset_model::sample_light_grid(&grid.view(), origin).ok())
            .map(|sample| sample.compressed)
    }

    fn finish_impact_marks(
        &self,
        host: &mut FxSystemHost,
        def_index: u8,
        against_world: bool,
        against_models: bool,
    ) {
        finish_impact_marks(
            host,
            self.world,
            self.marks,
            def_index,
            against_world,
            against_models,
        );
    }

    fn box_surfaces_prelude_ran(&self, host: &FxSystemHost) -> bool {
        box_surfaces_prelude_ran(host, self.world)
    }
}

fn complete_world_generate(host: &mut FxSystemHost, scene: &WorldScene, def_index: u8) {
    let Some(pose) = pose_from_fx_host(host) else {
        host.note_box_surfaces_skip();
        host.note_world_go_skip(def_index);
        return;
    };
    let origin = pose.origin;
    let radius = pose.radius;
    let Some(axis) = pose.axis else {
        host.note_world_go_skip(def_index);
        return;
    };
    let Some(meta) = fill_world_filter(scene, &pose) else {
        host.note_box_surfaces_skip();
        host.note_world_go_skip(def_index);
        return;
    };
    let material = pose.material.map(str::to_owned);
    let Some(native_color) = host.last_decal_color else {
        host.note_world_go_skip(def_index);
        return;
    };
    host.note_box_surfaces_run();
    let Some(staging) = stage_clip_retained(scene, origin, radius, axis) else {
        host.note_world_go_skip(def_index);
        return;
    };
    if staging.used_tri == 0 || staging.overflow {
        if meta.allow.unknown > 0 {
            diag::event!(
                World,
                Info,
                "mark_allow_skip",
                "mark material={} bits={:?} unknown_receiver={} unknown={} reject={} keep={}",
                material.as_deref().unwrap_or("<missing>"),
                meta.mark_surface_type_bits,
                meta.unknown_receiver_material
                    .as_deref()
                    .unwrap_or("<missing>"),
                meta.allow.unknown,
                meta.allow.reject,
                meta.allow.keep,
            );
        }
        host.note_world_go_skip(def_index);
        return;
    }
    let used_t = staging.used_tri as usize;
    let used_p = staging.used_point as usize;
    let (first_ctx, tris, points) = MARK_CLIP_SCRATCH.with(|clip_cell| {
        let clip = clip_cell.borrow();
        let ctx = clip.tris.first().map(first_tri_context);
        (
            ctx,
            clip.tris[..used_t.min(clip.tris.len())].to_vec(),
            clip.points[..used_p.min(clip.points.len())].to_vec(),
        )
    });
    let Some(first_ctx) = first_ctx else {
        host.note_world_go_skip(def_index);
        return;
    };
    let req = FxAllocMarkRequest {
        any_marks: true,
        tri_count: staging.used_tri,
        point_count: staging.used_point,
        origin,
        radius,
        tex_coord_axis: axis[1],
        native_color,
        material: 0,
        frame_count: host.frame_stamp,
        first_tri_context: first_ctx,
    };
    match host.marks.alloc_mark_from_go_callback(req, &tris, &points) {
        Some(handle) => {
            host.marks.set_material_name(handle, material.as_deref());
            host.last_mark_alloc_slot = Some(handle);
            host.note_world_go_fire();
        }
        None => host.note_world_go_skip(def_index),
    }
}

fn complete_glass_marks(host: &mut FxSystemHost, scene: &WorldScene) {
    use fx_iw4::{FX_GLASS_STATE_FLAG_SIMPLE, glass_state_def_index, glass_state_flags};

    let Some(glass) = scene.fx_glass.as_ref() else {
        return;
    };
    let Some(pose) = pose_from_fx_host(host) else {
        return;
    };
    let (origin, radius) = (pose.origin, pose.radius);
    let Some(axis) = pose.axis else { return };
    let Some(material) = pose_material(host, MarkFragmentsAgainst::Models).map(str::to_owned)
    else {
        return;
    };
    let Some(native_color) = host.last_decal_color else {
        return;
    };
    let mark_bits = mark_material_surface_bits(scene, &material);
    let planes = marks_iw4::mark_fragment_clip_planes(origin, axis, radius);
    let mut vertices = [fx_iw4::FxGlassIntactVert {
        xyz: [0.0; 3],
        uv: [0.0; 2],
    }; 32];
    let mut fragment = [marks_iw4::FxWorldMarkPoint {
        xyz: [0.0; 3],
        weights: [0.0; 3],
    }; marks_iw4::R_MARK_CHOP_MAX_POINTS];
    let mut tris = Vec::new();
    let mut points = Vec::new();
    let mut def_allows: Vec<Option<bool>> = vec![None; glass.defs.len()];
    for piece in 0..host.glass.init_piece_count as usize {
        if !host.glass.is_in_use(piece as u32) {
            continue;
        }
        let mut context = [0u8; 7];
        context[0] = 4;
        context[1] = marks_iw4::GFX_SURFACE_LIGHTMAP_NONE;
        context[2..4].copy_from_slice(&(piece as u16).to_le_bytes());
        let state = &host.glass.piece_states[piece];
        if glass_state_flags(state) & FX_GLASS_STATE_FLAG_SIMPLE != 0 {
            continue;
        }
        let place = &host.glass.piece_places[piece];
        let pane_axis = fx_iw4::unit_quat_to_axis(fx_iw4::glass_place_quat(place));
        let pane_origin = fx_iw4::glass_place_origin(place);
        let plane_distance: f32 = (0..3)
            .map(|k| (origin[k] - pane_origin[k]) * pane_axis[2][k])
            .sum();
        let thickness = host.glass.half_thickness.get(piece).copied().unwrap_or(0.0);
        // Exact only while the clip volume is the box `origin ± radius` along
        // the mark axes (`mark_fragment_clip_planes`).
        let reach = radius
            * axis
                .iter()
                .map(|a| (0..3).map(|k| a[k] * pane_axis[2][k]).sum::<f32>().abs())
                .sum::<f32>();
        if plane_distance.abs() - thickness.abs() > reach + 0.01 {
            continue;
        }
        let def_index = glass_state_def_index(state) as usize;
        let Some(def) = glass.defs.get(def_index) else {
            continue;
        };
        let allows = *def_allows[def_index].get_or_insert_with(|| {
            glass
                .def_materials
                .get(def_index)
                .and_then(|(name, _)| runtime_material_by_name(scene, name))
                .is_some_and(|receiver| {
                    marks_iw4::mark_include_in_world_clip(marks_iw4::mark_allow(
                        Some(receiver.info_game_flags),
                        receiver.surface_type_bits,
                        mark_bits,
                    ))
                })
        });
        if !allows {
            continue;
        }
        let Some(n) =
            fx_iw4::glass_intact_verts(place, state, &host.glass.geo_data, def, &mut vertices)
        else {
            continue;
        };
        if tris.is_empty() {
            tris = vec![
                marks_iw4::FxMarkStagingTri::ZERO;
                marks_iw4::R_MARK_FRAGMENTS_MAX_TRIS as usize
            ];
            points = vec![
                marks_iw4::FxMarkStagingPoint::ZERO;
                marks_iw4::R_MARK_FRAGMENTS_MAX_POINTS as usize
            ];
        }
        let facing = pane_axis[2]
            .iter()
            .zip(axis[0])
            .map(|(a, b)| a * b)
            .sum::<f32>();
        let side = if plane_distance.abs() > 0.001 {
            plane_distance
        } else {
            facing
        };
        let normal = pane_axis[2].map(|v| if side < 0.0 { -v } else { v });
        for vertex in &mut vertices[..n] {
            for k in 0..3 {
                vertex.xyz[k] += normal[k] * thickness;
            }
        }
        let mut used_tri = 0;
        let mut used_point = 0;
        let mut overflow = false;
        for i in 1..n - 1 {
            let v0 = vertices[0].xyz;
            let mut v1 = vertices[i].xyz;
            let mut v2 = vertices[i + 1].xyz;
            if marks_iw4::mark_is_triangle_rejected(normal, v0, v1, v2) {
                std::mem::swap(&mut v1, &mut v2);
            }
            if marks_iw4::mark_is_triangle_rejected(normal, v0, v1, v2) {
                continue;
            }
            let count =
                marks_iw4::mark_chop_world_triangle_points(&planes, v0, v1, v2, &mut fragment);
            if count < 3 {
                continue;
            }
            match marks_iw4::mark_emit_brush_fragment(
                used_tri,
                used_point,
                marks_iw4::R_MARK_FRAGMENTS_MAX_TRIS,
                marks_iw4::R_MARK_FRAGMENTS_MAX_POINTS,
                &fragment[..count as usize],
                [0.0; 2],
                [0.0; 2],
                [0.0; 2],
                normal,
                normal,
                normal,
                context,
                &mut tris,
                &mut points,
            ) {
                Ok((t, p)) => {
                    used_tri = t;
                    used_point = p;
                }
                Err(_) => {
                    overflow = true;
                    break;
                }
            }
        }
        if used_tri == 0 || overflow {
            continue;
        }
        let req = FxAllocMarkRequest {
            any_marks: true,
            tri_count: used_tri,
            point_count: used_point,
            origin,
            radius,
            tex_coord_axis: axis[1],
            native_color,
            material: 0,
            frame_count: host.frame_stamp,
            first_tri_context: first_tri_context(&tris[0]),
        };
        if let Some(handle) = host.marks.alloc_mark_from_go_callback(
            req,
            &tris[..used_tri as usize],
            &points[..used_point as usize],
        ) {
            host.marks.set_material_name(handle, Some(&material));
            host.last_mark_alloc_slot = Some(handle);
        }
    }
}

fn stage_clip_retained(
    scene: &WorldScene,
    origin: [f32; 3],
    radius: f32,
    axis: [[f32; 3]; 3],
) -> Option<marks_iw4::MarkWorldStaging> {
    if scene.retained_positions.is_empty() {
        return None;
    }
    if scene.retained_lightmap_uvs.len() != scene.retained_positions.len()
        || scene.retained_normals.len() != scene.retained_positions.len()
    {
        return None;
    }
    let packed = match scene.cull.as_ref() {
        Some(cull) => cull.packed_indices.as_slice(),
        None => &[],
    };
    MARK_CLIP_SCRATCH.with(|clip_cell| {
        let mut clip = clip_cell.borrow_mut();
        if clip.ranges.is_empty() {
            return Some(marks_iw4::MarkWorldStaging::default());
        }
        clip.ensure_stage_bufs();
        let MarkClipScratch {
            mids,
            halves,
            ranges,
            contexts,
            tris,
            points,
            ..
        } = &mut *clip;
        Some(marks_iw4::mark_stage_world_surfaces(
            origin,
            radius,
            axis,
            mids,
            halves,
            &scene.retained_positions,
            &scene.retained_lightmap_uvs,
            &scene.retained_normals,
            packed,
            ranges,
            contexts,
            marks_iw4::R_MARK_FRAGMENTS_MAX_TRIS,
            marks_iw4::R_MARK_FRAGMENTS_MAX_POINTS,
            tris,
            points,
            false,
        ))
    })
}

fn pose_from_fx_host_models(host: &FxSystemHost) -> Option<WorldMarkPose<'_>> {
    if host.last_decal_against_models != Some(true) {
        return None;
    }
    Some(WorldMarkPose {
        origin: host.last_decal_origin?,
        radius: host.last_decal_size0?,
        axis: Some(impact_mark_axis(host)?),
        material: pose_material(host, MarkFragmentsAgainst::Models),
    })
}

fn complete_models_generate(host: &mut FxSystemHost, scene: &WorldScene, def_index: u8) {
    let Some(pose) = pose_from_fx_host_models(host) else {
        host.note_models_go_skip(def_index);
        return;
    };
    let origin = pose.origin;
    let radius = pose.radius;
    let Some(axis) = pose.axis else {
        host.note_models_go_skip(def_index);
        return;
    };
    let Some(native_color) = host.last_decal_color else {
        host.note_models_go_skip(def_index);
        return;
    };
    let material = pose.material.map(str::to_owned);
    let mark_bits = material
        .as_deref()
        .and_then(|name| mark_material_surface_bits(scene, name));
    let pose = WorldMarkPose {
        origin,
        radius,
        axis: Some(axis),
        material: material.as_deref(),
    };
    let Some(cull) = scene.cull.as_ref() else {
        host.note_models_go_skip(def_index);
        return;
    };
    let cpu = scene.smodel_mark_cpu.as_ref();
    let mut walked = 0u32;
    let mut clip_kept = 0u32;
    let mut fired = 0u32;
    let mut no_instance = 0u32;
    let mut no_cpu = 0u32;
    let mut no_mesh = 0u32;
    let mut surf_keep = 0u32;
    MARK_CLIP_SCRATCH.with(|clip_cell| {
        let mut clip = clip_cell.borrow_mut();
        clip.hit_smodels.clear();
        MARK_BOX_SCRATCH.with(|box_cell| {
            let bits = box_cell.borrow();
            for bit in dpvs_iw4::msb_iter(&bits.smodel_bits, bits.smodel_n) {
                clip.hit_smodels.push(bit);
            }
        });
        let hit_n = clip.hit_smodels.len();
        for i in 0..hit_n {
            let bit = clip.hit_smodels[i];
            walked = walked.saturating_add(1);
            let Some(instance) = scene
                .static_model_instances
                .get(bit)
                .and_then(|slot| slot.as_ref())
            else {
                no_instance = no_instance.saturating_add(1);
                continue;
            };
            let Some(cpu) = cpu else {
                no_cpu = no_cpu.saturating_add(1);
                continue;
            };
            let Some(mesh_surfs) = cpu.meshes.get(instance.mesh) else {
                no_mesh = no_mesh.saturating_add(1);
                continue;
            };
            let bounds = cull.dpvs.smodel_bounds.get(bit).copied();
            let Some(staging) = stage_one_smodel(
                scene, cpu, mesh_surfs, &pose, axis, instance, bounds, bit as u16, mark_bits,
                &mut clip,
            ) else {
                continue;
            };
            surf_keep = surf_keep.saturating_add(staging.1);
            clip_kept = clip_kept.saturating_add(staging.0.census.clip_kept);
            if staging.0.used_tri == 0 || staging.0.overflow {
                continue;
            }
            let used_t = staging.0.used_tri as usize;
            let used_p = staging.0.used_point as usize;
            let Some(first_tri) = clip.tris.first() else {
                continue;
            };
            let req = FxAllocMarkRequest {
                any_marks: true,
                tri_count: staging.0.used_tri,
                point_count: staging.0.used_point,
                origin: pose.origin,
                radius: pose.radius,
                tex_coord_axis: axis[1],
                native_color,
                material: 0,
                frame_count: host.frame_stamp,
                first_tri_context: first_tri_context(first_tri),
            };
            if let Some(handle) = host.marks.alloc_mark_from_go_callback(
                req,
                &clip.tris[..used_t],
                &clip.points[..used_p],
            ) {
                host.marks.set_material_name(handle, material.as_deref());
                if host.last_mark_alloc_slot.is_none() {
                    host.last_mark_alloc_slot = Some(handle);
                }
                host.note_models_go_fire();
                fired = fired.saturating_add(1);
            }
        }
    });
    host.last_models_smodel_n = Some(walked);
    host.last_models_clip_kept = Some(clip_kept);
    host.last_models_no_instance = Some(no_instance);
    host.last_models_no_cpu = Some(no_cpu);
    host.last_models_no_mesh = Some(no_mesh);
    host.last_models_surf_keep = Some(surf_keep);
    if fired == 0 && walked == 0 {
        host.note_models_go_skip(def_index);
    }
}

fn stage_one_smodel(
    scene: &WorldScene,
    cpu: &crate::prepare::scene::world::SmodelMarkCpu,
    mesh_surfs: &[crate::prepare::scene::world::SmodelMarkSurface],
    pose: &WorldMarkPose<'_>,
    axis: [[f32; 3]; 3],
    instance: &crate::prepare::scene::world::WorldStaticModelInstance,
    _bounds: Option<dpvs_iw4::Bounds>,
    smodel_index: u16,
    mark_bits: Option<u32>,
    clip: &mut MarkClipScratch,
) -> Option<(marks_iw4::MarkWorldStaging, u32)> {
    use bevy::prelude::Vec3;

    let (mins, maxs) = marks_iw4::mark_model_local_box(
        pose.origin,
        pose.radius,
        instance.origin,
        instance.axis,
        instance.scale,
    )?;
    clip.ensure_stage_bufs();
    let planes = marks_iw4::mark_fragment_clip_planes(pose.origin, axis, pose.radius);
    let mark_dir = axis[0];
    let mut fragment = [marks_iw4::FxWorldMarkPoint {
        xyz: [0.0; 3],
        weights: [0.0; 3],
    }; marks_iw4::R_MARK_CHOP_MAX_POINTS];
    let mut staging = marks_iw4::MarkWorldStaging::default();
    let mut surf_keep = 0u32;
    let tf = instance.transform;
    let zero_uv = [0.0f32, 0.0];
    let MarkClipScratch { tris, points, .. } = clip;
    for (surf_i, surface) in mesh_surfs.iter().enumerate() {
        if surf_i > 63 {
            break;
        }
        let (flags, recv_bits) = receiver_allow_inputs(scene, 0, surface.material);
        let decision = marks_iw4::mark_allow(flags, recv_bits, mark_bits);
        if !marks_iw4::mark_include_in_world_clip(decision) {
            continue;
        }
        let start_i = surface.index_start as usize;
        let end_i = start_i.saturating_add(surface.index_count as usize);
        if surface.index_count < 3 || end_i > cpu.indices.len() {
            continue;
        }
        let asset_world::XSurfaceCollisionPayload::Iw4(lists) = &surface.collision else {
            continue;
        };
        if lists.iter().any(|list| list.tree.is_none()) {
            continue;
        }
        surf_keep = surf_keep.saturating_add(1);
        let context = marks_iw4::mark_context_from_smodel(
            surf_i as u8,
            smodel_index,
            instance.primary_light_index,
            instance.reflection_probe_index,
        );
        let mut visit_triangle = |triangle: u32| {
            if staging.overflow {
                return false;
            }
            let Some(t) = start_i.checked_add(triangle as usize * 3) else {
                return true;
            };
            if t + 2 >= end_i {
                return true;
            }
            let i0 = cpu.indices[t] as usize;
            let i1 = cpu.indices[t + 1] as usize;
            let i2 = cpu.indices[t + 2] as usize;
            let Some(&l0) = cpu.positions.get(i0) else {
                return true;
            };
            let Some(&l1) = cpu.positions.get(i1) else {
                return true;
            };
            let Some(&l2) = cpu.positions.get(i2) else {
                return true;
            };
            staging.census.tri_seen = staging.census.tri_seen.saturating_add(1);
            let v0 = tf.transform_point(Vec3::from(l0)).to_array();
            let v1 = tf.transform_point(Vec3::from(l1)).to_array();
            let v2 = tf.transform_point(Vec3::from(l2)).to_array();
            if marks_iw4::mark_is_triangle_rejected(mark_dir, v0, v1, v2) {
                staging.census.tri_rejected = staging.census.tri_rejected.saturating_add(1);
                return true;
            }
            let pts =
                marks_iw4::mark_chop_world_triangle_points(&planes, v0, v1, v2, &mut fragment);
            if pts < 3 {
                staging.census.clip_zero = staging.census.clip_zero.saturating_add(1);
                return true;
            }
            staging.census.clip_kept = staging.census.clip_kept.saturating_add(pts - 2);
            let n0 = xform_normal(tf, cpu.normals.get(i0).copied());
            let n1 = xform_normal(tf, cpu.normals.get(i1).copied());
            let n2 = xform_normal(tf, cpu.normals.get(i2).copied());
            match marks_iw4::mark_emit_brush_fragment(
                staging.used_tri,
                staging.used_point,
                marks_iw4::R_MARK_FRAGMENTS_MAX_TRIS,
                marks_iw4::R_MARK_FRAGMENTS_MAX_POINTS,
                &fragment[..pts as usize],
                zero_uv,
                zero_uv,
                zero_uv,
                n0,
                n1,
                n2,
                context,
                tris,
                points,
            ) {
                Ok((used_tri, used_point)) => {
                    staging.used_tri = used_tri;
                    staging.used_point = used_point;
                }
                Err(marks_iw4::FxMarkEmitRefuse::Overflow) => {
                    staging.overflow = true;
                    return false;
                }
                Err(marks_iw4::FxMarkEmitRefuse::FragmentTooSmall) => {}
            }
            true
        };
        let mut walk_ok = true;
        for list in lists {
            let Some(tree) = &list.tree else {
                unreachable!("missing collision trees were refused above");
            };
            let result = marks_iw4::xsurface_visit_triangles_in_aabb(
                marks_iw4::XSurfaceCollisionTree {
                    trans: tree.trans,
                    scale: tree.scale,
                    nodes: &tree.nodes,
                    leafs: &tree.leafs,
                },
                mins,
                maxs,
                &mut visit_triangle,
            );
            match result {
                Ok(true) => {}
                Ok(false) => break,
                Err(_) => {
                    walk_ok = false;
                    break;
                }
            }
        }
        drop(visit_triangle);
        if !walk_ok {
            return None;
        }
    }
    if surf_keep == 0 {
        return None;
    }
    Some((staging, surf_keep))
}

fn xform_normal(tf: bevy::prelude::Transform, n: Option<[f32; 3]>) -> [f32; 3] {
    use bevy::prelude::Vec3;
    let nrm = n.unwrap_or([0.0, 0.0, 1.0]);
    let w = tf.rotation.mul_vec3(Vec3::from(nrm));
    let len = w.length();
    if len > 0.0 {
        (w / len).to_array()
    } else {
        [0.0, 0.0, 1.0]
    }
}
