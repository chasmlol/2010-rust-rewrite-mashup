use bevy::prelude::*;
use dpvs_iw4::{
    GfxDrawSurf, GfxDrawSurfFields, SF_XMODEL_RIGID_SKINNED, material_sort_key_row, pack,
    pack_code_mesh_draw_surf, pack_glass_mesh_draw_surf, pack_mark_mesh_draw_surf,
    pack_particle_cloud_draw_surf, pack_xmodel_rigid_skinned_draw_surf,
    with_reflection_probe_index,
};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use super::list::DrawSurfList;
use super::tess::fx::FxCodeMeshPlan;
use super::tess::glass::GfxGlassMeshPlan;
use super::tess::mark::{GfxMarkMeshPlan, mark_mesh_surface_samplers};
use super::tess::particle_cloud::FxParticleCloudPlan;
use super::tess::smodel::{
    LodRampArgs, SmodelGpuPlan, SmodelMeshSurfaces, SmodelPlacement, smodel_camera_lod,
};
use super::tess::xmodel::{XMODEL_OBJECT_ID_VIEWMODEL, XModelDrawPlan, merge_xmodel_draw_plan};
use crate::assemble::pack::{PackDraw, PackKind};
use crate::prepare::scene::cull::{DpvsFrameStats, smodel_cull_dist_skips_slot};
use crate::prepare::scene::smodel_geom_cache::{
    LodRampDvar, PretessDvar, SmcEnableDvar, WorldStaticModelCache,
};
use crate::prepare::scene::smodel_lighting::WorldSmodelLighting;
use crate::prepare::scene::view_parms::PreparedSceneView;
use crate::prepare::scene::world::WorldScene;
use frame::WorldGeneration;

pub(crate) fn pack_draw(draw: &RetainedDrawItem) -> PackDraw {
    let kind = match draw.kind {
        RetainedDrawKind::World {
            surf, run, run_off, ..
        } => PackKind::World { surf, run, run_off },
        RetainedDrawKind::Smodel {
            surface,
            lighting_handle,
            stream: Some(lighting_iw4::SmodelSurfPath::Rigid),
            ..
        } => PackKind::SmodelRigid {
            surface,
            lighting_handle,
        },
        RetainedDrawKind::Smodel {
            surface,
            lighting_handle,
            stream: Some(lighting_iw4::SmodelSurfPath::Skinned),
            ..
        } => PackKind::SmodelSkinned {
            surface,
            lighting_handle,
        },
        RetainedDrawKind::Smodel {
            lighting_handle,
            stream: Some(lighting_iw4::SmodelSurfPath::Pretess),
            pretess: Some(dest),
            ..
        } => PackKind::SmodelPretess {
            lighting_handle,
            dest,
        },
        RetainedDrawKind::Smodel {
            lighting_handle,
            stream: Some(lighting_iw4::SmodelSurfPath::Cached),
            pretess: Some(dest),
            ..
        } => PackKind::SmodelCached {
            lighting_handle,
            dest,
        },
        RetainedDrawKind::XModel {
            surface,
            lighting_handle,
            ..
        } => PackKind::XModel {
            surface,
            lighting_handle,
        },
        _ => PackKind::Skip,
    };
    PackDraw {
        key: draw.key,
        material_rank: draw.material_rank,
        kind,
    }
}

pub use render_frame::{BspCameraLane, RetainedDrawItem, RetainedDrawKind};

const fn bsp_lane(kind: asset_world::CameraRangeKind) -> BspCameraLane {
    match kind {
        asset_world::CameraRangeKind::LitOpaque => BspCameraLane::LitOpaque,
        asset_world::CameraRangeKind::LitTrans => BspCameraLane::LitTrans,
        asset_world::CameraRangeKind::Decal => BspCameraLane::Decal,
        asset_world::CameraRangeKind::Emissive => BspCameraLane::Emissive,
    }
}

pub use render_frame::SmodelPretessRange;

// The packed key holds only a 12-bit sort band; maps with more materials alias in it.
fn world_surface_rank(
    cull: Option<&crate::prepare::scene::world::WorldCull>,
    surf: usize,
    packed: u64,
    catalog: &super::material_runtime::RuntimeMaterialCatalog,
) -> u32 {
    cull.and_then(|cull| cull.surface_materials.get(surf).copied().flatten())
        .and_then(|id| catalog.ordinal_for_asset_id(id))
        .map_or_else(
            || u32::from(GfxDrawSurf::from_packed(packed).material_sorted_index()),
            |ordinal| ordinal.get(),
        )
}

fn with_catalog(
    key: u64,
    material_rank: u32,
    kind: RetainedDrawKind,
    surface_samplers: super::SurfaceSamplerInputs,
    catalog: &super::material_runtime::RuntimeMaterialCatalog,
) -> RetainedDrawItem {
    RetainedDrawItem {
        material_id: catalog
            .material_for_sorted_ordinal(material_rank)
            .map(|material| material.asset_id),
        key,
        material_rank,
        kind,
        surface_samplers,
        camera_region: super::material_runtime::resolve_sorted_material(
            catalog,
            render_material::MaterialDrawKey::new(key, material_rank),
        )
        .ok()
        .map(|material| material.draw_rules.colour_camera_region),
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RetainedRebuildCensus {
    pub world_n: u32,
    pub smodel_n: u32,

    pub smodel_hidden_n: u32,

    pub smodel_query_n: u32,

    pub smodel_vis_n: u32,

    pub smodel_vis_ready: u8,
    pub sort_us: u32,

    pub smodel_probe59_n: u32,

    pub smodel_miss59_n: u32,

    pub rebuild_skip: u8,

    pub lod_hold: u8,

    pub world_run_n: u32,

    pub smodel_bucket_flush_n: u32,
    pub smodel_bucket_rigid_n: u32,
    pub smodel_bucket_skinned_n: u32,
    pub smodel_bucket_cached_n: u32,
    pub smodel_bucket_unread_n: u32,

    pub smodel_bucket_consume_n: u32,

    pub smodel_bucket_context_refused_n: u32,
}

impl RetainedRebuildCensus {
    fn note_smodel_bucket(&mut self, bucket: Option<i32>, full: bool) {
        let Some(bucket) = bucket else {
            self.smodel_bucket_unread_n = self.smodel_bucket_unread_n.saturating_add(1);
            return;
        };
        match bucket & 3 {
            lighting_iw4::SMODEL_BUCKET_CACHED => {
                self.smodel_bucket_cached_n = self.smodel_bucket_cached_n.saturating_add(1);
            }
            lighting_iw4::SMODEL_BUCKET_SKINNED => {
                self.smodel_bucket_skinned_n = self.smodel_bucket_skinned_n.saturating_add(1);
            }
            _ => {
                self.smodel_bucket_rigid_n = self.smodel_bucket_rigid_n.saturating_add(1);
            }
        }
        if full {
            self.smodel_bucket_flush_n = self.smodel_bucket_flush_n.saturating_add(1);
        }
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct StaticDrawLane {
    pub generation_id: super::MaterialGenerationId,

    logged_buckets: Option<[u32; 5]>,

    pub world_generation: WorldGeneration,
    pub colour: Vec<RetainedDrawItem>,
    pub emissive: Vec<RetainedDrawItem>,
    pub distortion: Vec<RetainedDrawItem>,
    pub census: RetainedRebuildCensus,

    last_static: Option<(u64, u64, EyeLodReuseKey)>,

    static_items: Vec<RetainedDrawItem>,

    world_items: Vec<RetainedDrawItem>,

    smodel_items: Vec<RetainedDrawItem>,

    last_smodel_picks: Vec<SmodelLodPick>,

    pub world_run_surfs: Vec<u16>,

    pub membership_revision: u64,

    pub smodel_surf_lists: lighting_iw4::SmodelSurfBucketLists,

    pub smodel_pretess_indices: Arc<Vec<u16>>,

    pub smodel_index_layout_revision: u64,
}

impl StaticDrawLane {
    pub fn live_for(
        &self,
        current: WorldGeneration,
    ) -> (
        &[RetainedDrawItem],
        &[RetainedDrawItem],
        &[RetainedDrawItem],
        &[RetainedDrawItem],
        &[u16],
    ) {
        if self.world_generation == current {
            (
                &self.static_items,
                &self.colour,
                &self.emissive,
                &self.distortion,
                self.world_run_surfs.as_slice(),
            )
        } else {
            (&[], &[], &[], &[], &[])
        }
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct XModelDrawLane {
    pub colour: Vec<RetainedDrawItem>,
    pub emissive: Vec<RetainedDrawItem>,
    pub distortion: Vec<RetainedDrawItem>,

    colour_source: Vec<u32>,
    emissive_source: Vec<u32>,
    distortion_source: Vec<u32>,

    pub membership_revision: u64,
    membership_hash: u64,

    pub payload_revision: u64,
    pub merge_packed_n: Option<u32>,
    pub sorted: u32,
    pub object_id_standin: u32,
    pub fx_object_id_exhausted: u32,
    pub skipped_no_ordinal: u32,
    pub skipped_no_baked_key: u32,
    pub skipped_camera_frustum: u32,
    pub skipped_no_lighting: u32,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct FxDrawLane {
    pub colour: Vec<RetainedDrawItem>,
    pub emissive: Vec<RetainedDrawItem>,
    pub distortion: Vec<RetainedDrawItem>,

    pub membership_revision: u64,
    membership_hash: u64,

    pub payload_revision: u64,
    pub code_sorted: u32,
    pub particle_cloud_sorted: u32,
    pub mark_mesh_sorted: u32,
    pub glass_mesh_sorted: u32,
    pub code_skipped_no_ordinal: u32,
    pub particle_cloud_skipped_no_ordinal: u32,
    pub mark_mesh_skipped_no_ordinal: u32,
    pub mark_mesh_skipped_no_lighting: u32,
    pub glass_mesh_skipped_no_ordinal: u32,
}

fn item_uses_distortion_lane(mat_sort_key: u8, world_distortion_key: Option<u32>) -> bool {
    world_distortion_key == Some(u32::from(material_sort_key_row(mat_sort_key)))
}

fn push_direct_lane_item(
    colour: &mut Vec<RetainedDrawItem>,
    emissive: &mut Vec<RetainedDrawItem>,
    distortion: Option<&mut Vec<RetainedDrawItem>>,
    item: RetainedDrawItem,
) {
    if let Some(distortion) = distortion {
        distortion.push(item);
    }
    match super::frame_product_kind_for_camera_region(item.camera_region) {
        Some(super::FrameProductKind::Emissive) => emissive.push(item),
        Some(super::FrameProductKind::Colour) => colour.push(item),
        Some(_) | None => {}
    }
}

pub(crate) fn retained_draw_order_tie(kind: &RetainedDrawKind) -> u32 {
    match kind {
        RetainedDrawKind::World { surf, .. } => u32::from(*surf),
        RetainedDrawKind::Smodel { surface, .. } | RetainedDrawKind::XModel { surface, .. } => {
            *surface
        }
        RetainedDrawKind::CodeMesh { draw, .. }
        | RetainedDrawKind::ParticleCloud { draw, .. }
        | RetainedDrawKind::MarkMesh { draw, .. }
        | RetainedDrawKind::Glass { draw, .. } => *draw,
    }
}

fn glass_depth_order(
    item: &RetainedDrawItem,
    eye: [f32; 3],
    glass: Option<&GfxGlassMeshPlan>,
) -> u32 {
    let RetainedDrawKind::Glass { draw, .. } = item.kind else {
        return 0;
    };
    let Some(origin) = glass
        .and_then(|plan| plan.draws.get(draw as usize))
        .map(|d| d.origin)
    else {
        return 0;
    };
    let dx = origin[0] - eye[0];
    let dy = origin[1] - eye[1];
    let dz = origin[2] - eye[2];
    !(dx * dx + dy * dy + dz * dz).to_bits()
}

fn sort_fx_draw_lane(lane: &mut FxDrawLane, eye: [f32; 3], glass: Option<&GfxGlassMeshPlan>) {
    let order = |item: &RetainedDrawItem| {
        (
            item.host_sort_key(),
            glass_depth_order(item, eye, glass),
            retained_draw_order_tie(&item.kind),
        )
    };
    lane.colour.sort_unstable_by_key(order);
    lane.emissive.sort_unstable_by_key(order);
    lane.distortion.sort_unstable_by_key(order);
}

pub(crate) fn mix_draw_membership(id: &mut u64, item: &RetainedDrawItem) {
    super::list::mix_content_id(id, item.key);
    super::list::mix_content_id(id, u64::from(item.material_rank));
    super::list::mix_content_id(id, item.material_id.map_or(0, |id| u64::from(id.0) + 1));
    let encode = |value: Option<u8>| value.map_or(0, |value| u64::from(value) + 1);
    super::list::mix_content_id(
        id,
        encode(item.surface_samplers.reflection_probe.map(|value| value.0)),
    );
    super::list::mix_content_id(
        id,
        encode(item.surface_samplers.primary_lightmap.map(|value| value.0)),
    );
    super::list::mix_content_id(
        id,
        encode(
            item.surface_samplers
                .secondary_lightmap
                .map(|value| value.0),
        ),
    );
    let (kind_tag, a, b, c) = match item.kind {
        RetainedDrawKind::World {
            surf, run, run_off, ..
        } => (
            0u64,
            u64::from(surf) | (u64::from(run) << 16),
            u64::from(run_off),
            0,
        ),
        RetainedDrawKind::Smodel {
            placement,
            surface,
            lighting_handle,
            stream,
            pretess,
            ..
        } => {
            let stream_tag = match stream {
                Some(lighting_iw4::SmodelSurfPath::Rigid) => 1,
                Some(lighting_iw4::SmodelSurfPath::Skinned) => 2,
                Some(lighting_iw4::SmodelSurfPath::Cached) => 3,
                Some(lighting_iw4::SmodelSurfPath::Pretess) => 4,
                None => 0,
            };
            let dest = pretess.map_or(0, |range| {
                u64::from(range.start).wrapping_shl(32) ^ u64::from(range.count)
            });
            (
                1,
                u64::from(placement),
                u64::from(surface) ^ u64::from(lighting_handle).rotate_left(16),
                stream_tag ^ dest,
            )
        }
        RetainedDrawKind::XModel {
            surface, object_id, ..
        } => (2, u64::from(surface), u64::from(object_id), 0),
        RetainedDrawKind::CodeMesh {
            draw,
            arg_count,
            viewmodel,
            ..
        } => (
            3,
            u64::from(draw),
            u64::from(arg_count),
            u64::from(viewmodel),
        ),
        RetainedDrawKind::ParticleCloud { draw, .. } => (4, u64::from(draw), 0, 0),
        RetainedDrawKind::MarkMesh { draw, .. } => (5, u64::from(draw), 0, 0),
        RetainedDrawKind::Glass {
            draw,
            lighting_handle,
            ..
        } => (6, u64::from(draw), u64::from(lighting_handle), 0),
    };
    super::list::mix_content_id(id, kind_tag);
    super::list::mix_content_id(id, a);
    super::list::mix_content_id(id, b);
    super::list::mix_content_id(id, c);
}

fn xmodel_camera_material(
    xmodel: &XModelDrawPlan,
    draw: &super::tess::xmodel::XModelSurfaceDraw,
    catalog: &super::RuntimeMaterialCatalog,
    thermal: &super::thermal_body::ThermalBodySelection,
) -> Option<(u32, bool)> {
    if let Some(ordinal) = thermal.for_draw(draw) {
        let ordinal = *ordinal.as_ref().ok()?;
        let material = catalog.material_for_sorted_ordinal(ordinal)?;
        Some((ordinal, material.takes_model_lighting))
    } else {
        let material = xmodel.materials.get(draw.material as usize)?;
        Some((
            material.material_sorted_index?,
            material.model_lighting_required,
        ))
    }
}

fn xmodel_lane_layout_hash(
    xmodel: &XModelDrawPlan,
    catalog: &super::material_runtime::RuntimeMaterialCatalog,
    thermal: &super::thermal_body::ThermalBodySelection,
) -> u64 {
    let mut id = super::list::CONTENT_ID_SEED;
    super::list::mix_content_id(&mut id, catalog.generation_id().get());
    super::list::mix_content_id(&mut id, xmodel.topology_revision);
    super::list::mix_content_id(&mut id, xmodel.draws.len() as u64);
    for draw in &xmodel.draws {
        super::list::mix_content_id(&mut id, u64::from(draw.surface));
        super::list::mix_content_id(&mut id, u64::from(draw.material));
        super::list::mix_content_id(&mut id, u64::from(draw.object_id));
        super::list::mix_content_id(&mut id, u64::from(draw.scene_light_index));
        super::list::mix_content_id(&mut id, u64::from(draw.reflection_probe_index));
        let refusal = match draw.colour_refusal {
            Some(super::tess::xmodel::XModelColourRefusal::CameraFrustum) => 1,
            None => 0,
        };
        super::list::mix_content_id(&mut id, refusal);
        let material = xmodel_camera_material(xmodel, draw, catalog, thermal);
        let lighting_skip =
            material.is_some_and(|(_, required)| required && draw.lighting_handle == 0);
        super::list::mix_content_id(&mut id, u64::from(lighting_skip));
        let ordinal = material
            .map(|(ordinal, _)| u64::from(ordinal))
            .unwrap_or(u64::MAX);
        super::list::mix_content_id(&mut id, ordinal);
    }
    id
}

fn lane_layout_hash(
    fx: Option<&FxCodeMeshPlan>,
    particle_cloud: Option<&FxParticleCloudPlan>,
    mark_mesh: Option<&GfxMarkMeshPlan>,
    glass_mesh: Option<&GfxGlassMeshPlan>,
) -> u64 {
    let mut id = super::list::CONTENT_ID_SEED;
    if let Some(plan) = fx {
        super::list::mix_content_id(&mut id, plan.draws.len() as u64);
        for (i, draw) in plan.draws.iter().enumerate() {
            if draw.index_count == 0 {
                continue;
            }
            super::list::mix_content_id(&mut id, i as u64);
            super::list::mix_content_id(&mut id, u64::from(draw.material));
            super::list::mix_content_id(&mut id, u64::from(draw.viewmodel));
            let mat = plan.materials.get(draw.material as usize);
            super::list::mix_content_id(&mut id, u64::from(mat.map(|m| m.sort_key).unwrap_or(0)));
            super::list::mix_content_id(
                &mut id,
                mat.and_then(|m| m.material_sorted_index)
                    .map(u64::from)
                    .unwrap_or(u64::MAX),
            );
        }
    }
    if let Some(plan) = particle_cloud {
        super::list::mix_content_id(&mut id, plan.draws.len() as u64);
        for (i, draw) in plan.draws.iter().enumerate() {
            if draw.index_count == 0 {
                continue;
            }
            super::list::mix_content_id(&mut id, i as u64);
            super::list::mix_content_id(&mut id, u64::from(draw.material));
        }
    }
    if let Some(plan) = mark_mesh {
        super::list::mix_content_id(&mut id, plan.draws.len() as u64);
        for (i, draw) in plan.draws.iter().enumerate() {
            if draw.index_count == 0 {
                continue;
            }
            super::list::mix_content_id(&mut id, i as u64);
            super::list::mix_content_id(&mut id, u64::from(draw.material));
            super::list::mix_content_id(&mut id, u64::from(draw.sub_key.lmap));
            super::list::mix_content_id(
                &mut id,
                u64::from(draw.sub_key.entity.unwrap_or(u16::MAX)),
            );
            super::list::mix_content_id(
                &mut id,
                u64::from(draw.sub_key.smodel.unwrap_or(u16::MAX)),
            );
            super::list::mix_content_id(&mut id, u64::from(draw.sub_key.glass.unwrap_or(u16::MAX)));
            super::list::mix_content_id(&mut id, u64::from(draw.sub_key.primary_light));
            super::list::mix_content_id(&mut id, u64::from(draw.sub_key.probe));
        }
    }
    if let Some(plan) = glass_mesh {
        super::list::mix_content_id(&mut id, plan.draws.len() as u64);
        for (i, draw) in plan.draws.iter().enumerate() {
            if draw.index_count == 0 {
                continue;
            }
            super::list::mix_content_id(&mut id, i as u64);
            super::list::mix_content_id(&mut id, u64::from(draw.material));
            super::list::mix_content_id(&mut id, u64::from(draw.reflection_probe_index));
        }
    }
    id
}

fn overlay_xmodel_lane_payload(xmodel: &XModelDrawPlan, lane: &mut XModelDrawLane) -> bool {
    let XModelDrawLane {
        colour,
        emissive,
        distortion,
        colour_source,
        emissive_source,
        distortion_source,
        ..
    } = lane;
    if colour.len() != colour_source.len()
        || emissive.len() != emissive_source.len()
        || distortion.len() != distortion_source.len()
    {
        return false;
    }
    let items = colour
        .iter_mut()
        .chain(emissive.iter_mut())
        .chain(distortion.iter_mut());
    let sources = colour_source
        .iter()
        .chain(emissive_source.iter())
        .chain(distortion_source.iter());
    for (item, &source) in items.zip(sources) {
        let RetainedDrawKind::XModel {
            world_from_local,
            lighting_handle,
            packed_lighting,
            is_scope,
            scene_entnum,
            ..
        } = &mut item.kind
        else {
            continue;
        };
        let Some(draw) = xmodel.draws.get(source as usize) else {
            return false;
        };
        *world_from_local = draw.world_from_local;
        *lighting_handle = draw.lighting_handle;
        *packed_lighting = draw.packed_lighting;
        *is_scope = draw.is_scope;
        *scene_entnum = draw.scene_entnum;
    }
    true
}

fn sort_xmodel_lane(items: &mut [RetainedDrawItem], sources: &mut [u32], order: &mut Vec<u32>) {
    debug_assert_eq!(items.len(), sources.len());
    order.clear();
    order.extend(0..u32::try_from(items.len()).unwrap_or(u32::MAX));
    order.sort_unstable_by_key(|&i| {
        let item = &items[i as usize];
        (item.host_sort_key(), retained_draw_order_tie(&item.kind))
    });
    for start in 0..order.len() {
        let mut at = start;
        loop {
            let from = order[at] as usize;
            order[at] = u32::try_from(at).unwrap_or(u32::MAX);
            if from == start {
                break;
            }
            items.swap(at, from);
            sources.swap(at, from);
            at = from;
        }
    }
}

fn overlay_fx_lane_payload(
    fx: Option<&FxCodeMeshPlan>,
    particle_cloud: Option<&FxParticleCloudPlan>,
    mark_mesh: Option<&GfxMarkMeshPlan>,
    glass_mesh: Option<&GfxGlassMeshPlan>,
    lane: &mut FxDrawLane,
) -> bool {
    for item in lane
        .colour
        .iter_mut()
        .chain(lane.emissive.iter_mut())
        .chain(lane.distortion.iter_mut())
    {
        match &mut item.kind {
            RetainedDrawKind::CodeMesh {
                draw,
                arg_count,
                args,
                ..
            } => {
                let Some(plan) = fx else {
                    return false;
                };
                let Some(src) = plan.draws.get(*draw as usize) else {
                    return false;
                };
                let n = (src.arg_count as usize).min(2);
                *arg_count = n as u8;
                *args = [[0.0; 4]; 2];
                let start = src.arg_start as usize;
                for (ai, slot) in args.iter_mut().enumerate().take(n) {
                    if let Some(row) = plan.args.get(start + ai) {
                        *slot = *row;
                    }
                }
            }
            RetainedDrawKind::ParticleCloud { draw, clouds, .. } => {
                let Some(plan) = particle_cloud else {
                    return false;
                };
                let Some(src) = plan.draws.get(*draw as usize) else {
                    return false;
                };
                *clouds = src.clouds;
            }
            RetainedDrawKind::MarkMesh {
                glass,
                draw,
                packed,
                lighting_handle,
                ..
            } => {
                let Some(plan) = mark_mesh else {
                    return false;
                };
                let Some(src) = plan.draws.get(*draw as usize) else {
                    return false;
                };
                *packed = src.sub_key.packed;
                *glass = src.sub_key.glass.is_some();
                if let Some(piece) = src.sub_key.glass {
                    let Some(glass_draw) = glass_mesh
                        .and_then(|plan| plan.draws.iter().find(|draw| draw.piece == piece))
                    else {
                        return false;
                    };
                    *lighting_handle = glass_draw.lighting_handle;
                }
            }
            RetainedDrawKind::Glass {
                draw,
                lighting_handle,
                ..
            } => {
                let Some(plan) = glass_mesh else {
                    return false;
                };
                let Some(src) = plan.draws.get(*draw as usize) else {
                    return false;
                };
                *lighting_handle = src.lighting_handle;
            }
            _ => {}
        }
    }
    true
}

fn mix_fx_payload_revision(
    fx: Option<&FxCodeMeshPlan>,
    particle_cloud: Option<&FxParticleCloudPlan>,
    mark_mesh: Option<&GfxMarkMeshPlan>,
    glass_mesh: Option<&GfxGlassMeshPlan>,
) -> u64 {
    let mut id = super::list::CONTENT_ID_SEED;
    super::list::mix_content_id(&mut id, fx.map(|plan| plan.revision).unwrap_or(0));
    super::list::mix_content_id(
        &mut id,
        particle_cloud.map(|plan| plan.revision).unwrap_or(0),
    );
    super::list::mix_content_id(&mut id, mark_mesh.map(|plan| plan.revision).unwrap_or(0));
    super::list::mix_content_id(&mut id, glass_mesh.map(|plan| plan.revision).unwrap_or(0));
    id
}

fn smodel_drawsurf_key(
    sort_key: u8,
    material_sorted_index: u32,
    object_id: u16,
    reflection_probe_index: u8,
    scene_light_index: u8,
    stream: lighting_iw4::SmodelSurfPath,
) -> u64 {
    pack(GfxDrawSurfFields {
        object_id,
        reflection_probe_index,
        scene_light_index,
        surf_type: lighting_iw4::smodel_surf_type(stream),
        material_sorted_index: render_material::sort_band(material_sorted_index),
        primary_sort_key: material_sort_key_row(sort_key),
        ..Default::default()
    })
    .packed
}

fn smodel_colour_emits(
    pass: SmodelDestinationPass,
    catalog: &super::RuntimeMaterialCatalog,
    authored: Option<assets::MaterialIndex>,
) -> bool {
    if pass != SmodelDestinationPass::Colour {
        return true;
    }
    authored
        .and_then(|id| catalog.derived(id))
        .is_none_or(|material| material.draw_rules.smodel_colour_emits)
}

fn smodel_sun_shadow_emits(
    pass: SmodelDestinationPass,
    catalog: &super::RuntimeMaterialCatalog,
    authored: Option<assets::MaterialIndex>,
) -> bool {
    if pass != SmodelDestinationPass::SunShadow {
        return true;
    }
    let Some(id) = authored else {
        return true;
    };
    catalog
        .derived(id)
        .map(|material| lighting_iw4::smodel_surf_sun_shadow_emits(material.info_game_flags))
        .unwrap_or(true)
}

#[derive(Clone, Copy, Default)]
struct SmodelExpandStats {
    emitted: u32,
    skipped_custom: u32,
}

fn note_custom_only_skip(
    stats: SmodelExpandStats,
    placement: u32,
    custom_skip: Option<&mut HashSet<u32>>,
) {
    if stats.emitted == 0
        && stats.skipped_custom > 0
        && let Some(set) = custom_skip
    {
        set.insert(placement);
    }
}

pub(crate) fn static_list_reusable(
    last: Option<(u64, u64, EyeLodReuseKey)>,
    world_id: u64,
    vis_id: u64,
    eye_key: EyeLodReuseKey,
    generation_hold: bool,
    has_static: bool,
) -> bool {
    generation_hold
        && has_static
        && last
            .is_some_and(|(world, vis, eye)| world == world_id && vis == vis_id && eye == eye_key)
}

pub(crate) fn world_static_reusable(
    last_world_id: Option<u64>,
    world_id: u64,
    generation_hold: bool,
) -> bool {
    generation_hold && last_world_id == Some(world_id)
}

pub(crate) fn smodel_descriptors_reusable(
    last: Option<&[SmodelLodPick]>,
    next: &[SmodelLodPick],
) -> bool {
    last.is_some_and(|last| last == next)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EyeLodReuseKey {
    eye_bits: Option<[u32; 3]>,
    ramp_bits: [Option<u32>; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SmodelLodPick {
    placement: u32,
    mesh: u32,
    lod: u8,
    cache_index: u16,
    lighting_handle: u32,
    packed_lighting: Option<[u8; 4]>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SmodelLodWalk {
    picks: Vec<SmodelLodPick>,
    hidden_n: u32,
    query_n: u32,
}

fn eye_lod_reuse_key(eye: Option<Vec3>, ramp: LodRampArgs) -> EyeLodReuseKey {
    EyeLodReuseKey {
        eye_bits: eye.map(|eye| [eye.x.to_bits(), eye.y.to_bits(), eye.z.to_bits()]),
        ramp_bits: [
            ramp.scale_mid.map(f32::to_bits),
            ramp.bias_mid.map(f32::to_bits),
            ramp.scale_last.map(f32::to_bits),
        ],
    }
}

fn note_smodel_vis(census: &mut RetainedRebuildCensus, smodel_vis: &[u8]) {
    if !smodel_vis.is_empty() {
        census.smodel_vis_ready = 1;
        census.smodel_vis_n = smodel_vis.iter().filter(|byte| **byte != 0).count() as u32;
    }
}

fn collect_smodel_lod_picks(
    plan: Option<&SmodelGpuPlan>,
    smodel_vis: &[u8],
    cull_dists: &[u16],
    eye: Option<Vec3>,
    lod_args: LodRampArgs,
    lighting: Option<&WorldSmodelLighting>,
    smc_cache: Option<&WorldStaticModelCache>,
) -> SmodelLodWalk {
    let Some(plan) = plan else {
        return SmodelLodWalk::default();
    };
    let mut walk = SmodelLodWalk::default();
    for (placement_i, placement) in plan.placements.iter().enumerate() {
        let vis_slot = placement.lighting_slot.unwrap_or(placement_i);
        if smodel_vis_skips_slot(smodel_vis, vis_slot) {
            walk.hidden_n = walk.hidden_n.saturating_add(1);
            continue;
        }
        if smodel_cull_dist_skips_slot(
            cull_dists,
            vis_slot,
            placement.origin,
            eye,
            lod_args.scale_last,
        ) {
            walk.hidden_n = walk.hidden_n.saturating_add(1);
            continue;
        }
        walk.query_n = walk.query_n.saturating_add(1);
        if let Some(pick) = smodel_descriptor_pick(
            placement_i,
            placement,
            plan,
            lighting,
            smc_cache,
            eye,
            lod_args,
        ) {
            walk.picks.push(pick);
        }
    }
    walk
}

fn smodel_descriptor_pick(
    placement_i: usize,
    placement: &SmodelPlacement,
    plan: &SmodelGpuPlan,
    lighting: Option<&WorldSmodelLighting>,
    smc_cache: Option<&WorldStaticModelCache>,
    eye: Option<Vec3>,
    lod_args: LodRampArgs,
) -> Option<SmodelLodPick> {
    let handle = if placement.lit {
        let slot = placement.lighting_slot?;
        let h = lighting
            .and_then(|l| l.handles.get(slot).copied())
            .unwrap_or(0);
        if h == 0 {
            return None;
        }
        u32::from(h)
    } else {
        0
    };
    let packed_lighting = placement.packed_lighting.or_else(|| {
        placement
            .lighting_slot
            .and_then(|slot| lighting.and_then(|l| l.packed_lighting_for_slot(slot)))
    });
    let mesh = plan.meshes.get(placement.mesh)?;
    let lod = smodel_camera_lod(mesh.lod, placement.origin, placement.scale, eye, lod_args)?;
    let lod_surfs = mesh
        .surfaces_by_lod
        .get(usize::from(lod))
        .map(|v| v.as_slice())
        .unwrap_or(&[]);
    if lod_surfs.is_empty() {
        return None;
    }
    let vis_slot = placement.lighting_slot.unwrap_or(placement_i);
    let _smodel_index = u16::try_from(vis_slot).ok()?;
    let lod_i = usize::from(lod);
    Some(SmodelLodPick {
        placement: placement_i as u32,
        mesh: placement.mesh as u32,
        lod,
        cache_index: smodel_cache_index_u16(smc_cache, placement.lighting_slot, lod_i),
        lighting_handle: handle,
        packed_lighting,
    })
}

fn merge_presorted_retained(
    static_items: &[RetainedDrawItem],
    dynamic: &[RetainedDrawItem],
) -> Vec<RetainedDrawItem> {
    let mut out = Vec::with_capacity(static_items.len() + dynamic.len());
    let mut i = 0;
    let mut j = 0;
    while i < static_items.len() && j < dynamic.len() {
        let a = (
            static_items[i].host_sort_key(),
            retained_draw_order_tie(&static_items[i].kind),
        );
        let b = (
            dynamic[j].host_sort_key(),
            retained_draw_order_tie(&dynamic[j].kind),
        );
        if a <= b {
            out.push(static_items[i]);
            i += 1;
        } else {
            out.push(dynamic[j]);
            j += 1;
        }
    }
    out.extend_from_slice(&static_items[i..]);
    out.extend_from_slice(&dynamic[j..]);
    out
}

fn materialize_world_runs(items: &mut Vec<RetainedDrawItem>, table: &mut Vec<u16>) -> u32 {
    table.clear();
    let mut run_n = 0u32;
    for item in items {
        let RetainedDrawKind::World {
            surf,
            run,
            world_from_local,
            bsp_kind,
            bsp_run_first,
            setup_key_changed,
            ..
        } = item.kind
        else {
            continue;
        };
        let run = run.max(1);
        let run_off = table.len() as u32;
        for offset in 0..run {
            table.push(
                surf.checked_add(offset)
                    .unwrap_or_else(|| panic!("world draw run exceeds the u16 surface domain")),
            );
        }
        item.kind = RetainedDrawKind::World {
            surf,
            run,
            run_off,
            bsp_kind,
            bsp_run_first,
            setup_key_changed,
            world_from_local,
        };
        run_n = run_n.saturating_add(1);
    }
    run_n
}

pub(crate) fn smodel_vis_skips_slot(smodel_vis: &[u8], slot: usize) -> bool {
    if smodel_vis.is_empty() {
        return false;
    }
    smodel_vis.get(slot).copied().unwrap_or(0) == 0
}

fn packed_lighting_dword_nonzero(bytes: Option<[u8; 4]>) -> bool {
    bytes.is_some_and(|b| u32::from_le_bytes(b) != 0)
}

fn smodel_lod_smc_flag(mesh: &SmodelMeshSurfaces, lod: usize) -> u8 {
    mesh.lod_smc_rows
        .and_then(|rows| rows.get(lod).copied())
        .or_else(|| (lod == 0).then_some(mesh.lod_smc).flatten())
        .map(|row| row[1])
        .unwrap_or(0)
}

fn smodel_cache_index_u16(
    cache: Option<&WorldStaticModelCache>,
    lighting_slot: Option<usize>,
    lod: usize,
) -> u16 {
    lighting_slot
        .and_then(|slot| cache.and_then(|c| c.cache_index.get(slot)))
        .and_then(|row| row.get(lod).copied())
        .unwrap_or(0)
}

fn smodel_lod_is_rigid(mesh: &SmodelMeshSurfaces, lod: usize) -> Option<bool> {
    let bytes = mesh.xsurface_plus_1_by_lod.get(lod)?;
    let mut raw = Vec::with_capacity(bytes.len());
    for byte in bytes {
        raw.push((*byte)?);
    }
    Some(lighting_iw4::smodel_lod_is_rigid(&raw))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SmodelDestinationPass {
    Colour,
    SunShadow,
}

#[derive(Clone, Copy, Debug)]
struct SmodelBucketEmitSource {
    placement: u32,
    lod: u8,
    payload: u16,
    world_from_local: Mat4,
    lighting_handle: u32,
    packed_lighting: Option<[u8; 4]>,
    pass: SmodelDestinationPass,
}

struct SmodelBucketEmitQueues {
    rows: [Vec<SmodelBucketEmitSource>; lighting_iw4::SMODEL_BUCKET_LIST_N],
}

impl Default for SmodelBucketEmitQueues {
    fn default() -> Self {
        Self {
            rows: std::array::from_fn(|_| Vec::new()),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SmodelDestinationRecord {
    key: u64,
    surface: u32,
    material: u32,
    world_from_local: Mat4,
    lighting_handle: u32,
    packed_lighting: Option<[u8; 4]>,
    placement: u32,
    stream: Option<lighting_iw4::SmodelSurfPath>,
    cache_index: Option<u16>,
    pretess: Option<SmodelPretessRange>,
    surface_samplers: super::SurfaceSamplerInputs,
    material_rank: u32,
}

impl SmodelDestinationRecord {
    fn into_item(
        self,
        catalog: &super::material_runtime::RuntimeMaterialCatalog,
    ) -> RetainedDrawItem {
        let world_from_local = match self.stream {
            Some(lighting_iw4::SmodelSurfPath::Cached | lighting_iw4::SmodelSurfPath::Pretess) => {
                Mat4::IDENTITY
            }
            Some(lighting_iw4::SmodelSurfPath::Rigid)
            | Some(lighting_iw4::SmodelSurfPath::Skinned)
            | None => self.world_from_local,
        };
        with_catalog(
            self.key,
            self.material_rank,
            RetainedDrawKind::Smodel {
                surface: self.surface,
                material: self.material,
                world_from_local,
                lighting_handle: self.lighting_handle,
                packed_lighting: self.packed_lighting,
                placement: self.placement,
                stream: self.stream,
                cache_index: self.cache_index,
                pretess: self.pretess,
            },
            self.surface_samplers,
            catalog,
        )
    }
}

struct SmodelPretessBuilder {
    enabled: bool,
    cmd_used: usize,
    cmd_cap: usize,
    indices: Vec<u16>,
}

impl SmodelPretessBuilder {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            cmd_used: 0,
            cmd_cap: usize::MAX,
            indices: Vec::new(),
        }
    }

    fn dest_used(&self) -> u32 {
        u32::try_from(self.indices.len()).unwrap_or(u32::MAX)
    }

    fn decide(&self, cache_indices: &[u16], runs: &[&[u16]]) -> lighting_iw4::SmodelCachedCmdPlan {
        lighting_iw4::smodel_cached_cmd_plan(
            lighting_iw4::SmodelSurfPath::Cached,
            cache_indices,
            self.enabled,
            false,
            self.cmd_used,
            self.cmd_cap,
            self.dest_used(),
            render_frame::DYNAMIC_INDEX_BUFFER_CAPACITY,
            runs,
        )
    }

    fn commit_pretess(
        &mut self,
        alloc: lighting_iw4::SmodelPretessAlloc,
        runs: &[&[u16]],
    ) -> Option<SmodelPretessRange> {
        let start = usize::try_from(alloc.first_index).ok()?;
        let count = usize::try_from(alloc.index_count).ok()?;
        if start != self.indices.len() {
            return None;
        }
        self.indices.resize(start.saturating_add(count), 0);
        if !lighting_iw4::smodel_pretess_indices_copy(&mut self.indices, alloc, runs) {
            self.indices.truncate(start);
            return None;
        }
        self.cmd_used = self.cmd_used.saturating_add(alloc.cmd.len());
        Some(SmodelPretessRange {
            start: alloc.first_index,
            count: alloc.index_count,
        })
    }

    fn append_cached_fallback(&mut self, source: &[u16]) -> Option<SmodelPretessRange> {
        if source.is_empty() || !source.len().is_multiple_of(3) {
            return None;
        }
        let start = u32::try_from(self.indices.len()).ok()?;
        let count = u32::try_from(source.len()).ok()?;
        if start.checked_add(count)? > render_frame::DYNAMIC_INDEX_BUFFER_CAPACITY {
            return None;
        }
        self.indices.extend_from_slice(source);
        Some(SmodelPretessRange { start, count })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SmodelConsumeResult {
    buckets: u32,
    context_refused: u32,
    skipped_custom: u32,
}

fn push_smodel_surf_bucket(
    lists: &mut lighting_iw4::SmodelSurfBucketLists,
    queues: &mut SmodelBucketEmitQueues,
    lod: i32,
    smc_enable: bool,
    lighting_nonzero: bool,
    lodinfo_29: u8,
    cache_index: u16,
    smodel_index: u16,
    lod_is_rigid: Option<bool>,
    mut source: SmodelBucketEmitSource,
) -> (Option<i32>, bool) {
    let Some(lod_is_rigid) = lod_is_rigid else {
        return (None, false);
    };
    let bucket = lighting_iw4::add_static_model_surf_to_bucket(
        lod,
        smc_enable,
        lighting_nonzero,
        lodinfo_29,
        cache_index,
        lod_is_rigid,
    );
    let payload = lighting_iw4::smodel_bucket_store_payload(bucket, smodel_index, cache_index);
    let Some(push) = lighting_iw4::smodel_surf_bucket_push(lists, bucket, payload) else {
        return (None, false);
    };
    let Some(queue) = usize::try_from(bucket)
        .ok()
        .and_then(|bucket| queues.rows.get_mut(bucket))
    else {
        return (None, false);
    };
    source.payload = payload;
    queue.push(source);
    (Some(bucket), push == lighting_iw4::SmodelBucketPush::Full)
}

fn expand_smodel_destination(
    source: SmodelBucketEmitSource,
    source_path: Option<lighting_iw4::SmodelSurfPath>,
    smodel_plan: &SmodelGpuPlan,
    catalog: &super::RuntimeMaterialCatalog,
    _cache: Option<&WorldStaticModelCache>,
    _pretess: &mut SmodelPretessBuilder,
    out: &mut Vec<SmodelDestinationRecord>,
    custom_skip: Option<&mut HashSet<u32>>,
) -> SmodelExpandStats {
    let mut stats = SmodelExpandStats::default();
    let Some(placement) = smodel_plan.placements.get(source.placement as usize) else {
        return stats;
    };
    let Some(mesh) = smodel_plan.meshes.get(placement.mesh) else {
        return stats;
    };
    let lod_surfs = mesh
        .surfaces_by_lod
        .get(usize::from(source.lod))
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let Some(object_id) = source
        .placement
        .checked_add(1)
        .and_then(|id| u16::try_from(id).ok())
    else {
        return stats;
    };
    for &(surface, authored) in lod_surfs {
        if !smodel_colour_emits(source.pass, catalog, authored) {
            continue;
        }
        if !smodel_sun_shadow_emits(source.pass, catalog, authored) {
            stats.skipped_custom = stats.skipped_custom.saturating_add(1);
            continue;
        }
        let material = match source.pass {
            SmodelDestinationPass::Colour if placement.lit => smodel_plan
                .lit_material_key
                .get(&(authored, placement.reflection_probe_index))
                .copied(),
            SmodelDestinationPass::Colour => smodel_plan.unlit_material_key.get(&authored).copied(),
            SmodelDestinationPass::SunShadow => smodel_plan
                .lit_material_key
                .get(&(authored, placement.reflection_probe_index))
                .copied()
                .or_else(|| smodel_plan.unlit_material_key.get(&authored).copied()),
        };
        let Some(material) = material else {
            continue;
        };
        let Some(mat) = smodel_plan.materials.get(material as usize) else {
            continue;
        };
        if source.pass == SmodelDestinationPass::Colour
            && placement.lit != mat.model_lighting_required
        {
            continue;
        }
        let Some(material_sorted_index) = mat.material_sorted_index else {
            continue;
        };
        let Some(source_path) = source_path else {
            continue;
        };
        let stream = lighting_iw4::smodel_dest_path(source_path, false);
        let pretess_range = None;
        let cache_index = matches!(
            stream,
            lighting_iw4::SmodelSurfPath::Cached | lighting_iw4::SmodelSurfPath::Pretess
        )
        .then_some(source.payload);
        out.push(SmodelDestinationRecord {
            material_rank: material_sorted_index,
            key: smodel_drawsurf_key(
                mat.sort_key,
                material_sorted_index,
                object_id,
                placement.reflection_probe_index,
                placement.primary_light_index,
                stream,
            ),
            surface,
            material,
            world_from_local: source.world_from_local,
            lighting_handle: source.lighting_handle,
            packed_lighting: source.packed_lighting,
            placement: source.placement,
            stream: Some(stream),
            cache_index,
            pretess: pretess_range,
            surface_samplers: super::SurfaceSamplerInputs {
                reflection_probe: Some(super::SurfaceReflectionProbeId(
                    placement.reflection_probe_index,
                )),
                ..Default::default()
            },
        });
        stats.emitted = stats.emitted.saturating_add(1);
    }
    note_custom_only_skip(stats, source.placement, custom_skip);
    stats
}

fn cached_index_run<'a>(
    cache: Option<&'a WorldStaticModelCache>,
    placement: &SmodelPlacement,
    surface: u32,
) -> Option<&'a [u16]> {
    let slot = u32::try_from(placement.lighting_slot?).ok()?;
    let run = cache?.index_runs.get(&(slot, surface))?;
    (!run.is_empty() && run.len().is_multiple_of(3)).then_some(run.as_slice())
}

fn push_smodel_destination(
    source: SmodelBucketEmitSource,
    surface: u32,
    authored: Option<assets::MaterialIndex>,
    stream: Option<lighting_iw4::SmodelSurfPath>,
    pretess: Option<SmodelPretessRange>,
    smodel_plan: &SmodelGpuPlan,
    catalog: &super::RuntimeMaterialCatalog,
    out: &mut Vec<SmodelDestinationRecord>,
) -> SmodelExpandStats {
    let mut stats = SmodelExpandStats::default();
    if !smodel_colour_emits(source.pass, catalog, authored) {
        return stats;
    }
    if !smodel_sun_shadow_emits(source.pass, catalog, authored) {
        stats.skipped_custom = 1;
        return stats;
    }
    let Some(placement) = smodel_plan.placements.get(source.placement as usize) else {
        return stats;
    };
    let Some(object_id) = source
        .placement
        .checked_add(1)
        .and_then(|id| u16::try_from(id).ok())
    else {
        return stats;
    };
    let material = match source.pass {
        SmodelDestinationPass::Colour if placement.lit => smodel_plan
            .lit_material_key
            .get(&(authored, placement.reflection_probe_index))
            .copied(),
        SmodelDestinationPass::Colour => smodel_plan.unlit_material_key.get(&authored).copied(),
        SmodelDestinationPass::SunShadow => smodel_plan
            .lit_material_key
            .get(&(authored, placement.reflection_probe_index))
            .copied()
            .or_else(|| smodel_plan.unlit_material_key.get(&authored).copied()),
    };
    let Some(material) = material else {
        return stats;
    };
    let Some(mat) = smodel_plan.materials.get(material as usize) else {
        return stats;
    };
    if source.pass == SmodelDestinationPass::Colour && placement.lit != mat.model_lighting_required
    {
        return stats;
    }
    let Some(material_sorted_index) = mat.material_sorted_index else {
        return stats;
    };
    let Some(stream) = stream else {
        return stats;
    };
    let cache_index = matches!(
        stream,
        lighting_iw4::SmodelSurfPath::Cached | lighting_iw4::SmodelSurfPath::Pretess
    )
    .then_some(source.payload);
    out.push(SmodelDestinationRecord {
        material_rank: material_sorted_index,
        key: smodel_drawsurf_key(
            mat.sort_key,
            material_sorted_index,
            object_id,
            placement.reflection_probe_index,
            placement.primary_light_index,
            stream,
        ),
        surface,
        material,
        world_from_local: source.world_from_local,
        lighting_handle: source.lighting_handle,
        packed_lighting: source.packed_lighting,
        placement: source.placement,
        stream: Some(stream),
        cache_index,
        pretess,
        surface_samplers: super::SurfaceSamplerInputs {
            reflection_probe: Some(super::SurfaceReflectionProbeId(
                placement.reflection_probe_index,
            )),
            ..Default::default()
        },
    });
    stats.emitted = 1;
    stats
}

fn expand_cached_destination_batch(
    sources: &[SmodelBucketEmitSource],
    smodel_plan: &SmodelGpuPlan,
    catalog: &super::RuntimeMaterialCatalog,
    cache: Option<&WorldStaticModelCache>,
    pretess: &mut SmodelPretessBuilder,
    out: &mut Vec<SmodelDestinationRecord>,
    mut custom_skip: Option<&mut HashSet<u32>>,
) -> SmodelExpandStats {
    let mut stats = SmodelExpandStats::default();
    let Some(&first) = sources.first() else {
        return stats;
    };
    let Some(placement) = smodel_plan.placements.get(first.placement as usize) else {
        return stats;
    };
    let Some(mesh) = smodel_plan.meshes.get(placement.mesh) else {
        return stats;
    };
    let lod_surfs = mesh
        .surfaces_by_lod
        .get(usize::from(first.lod))
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if lod_surfs.is_empty() {
        return stats;
    }
    let ids: Vec<u16> = sources.iter().map(|source| source.payload).collect();
    let mut runs: Vec<&[u16]> = Vec::new();
    let mut complete = true;
    for &(surface, _) in lod_surfs {
        for source in sources {
            let Some(place) = smodel_plan.placements.get(source.placement as usize) else {
                complete = false;
                continue;
            };
            match cached_index_run(cache, place, surface) {
                Some(run) => runs.push(run),
                None => complete = false,
            }
        }
    }
    let decision = if complete {
        pretess.decide(&ids, &runs)
    } else {
        pretess.decide(&ids, &[])
    };
    if decision.cmd_kind == lighting_iw4::SmodelCmdKind::Unchanged && decision.consumed == 0 {
        for &source in sources {
            let one = expand_smodel_destination(
                source,
                Some(lighting_iw4::SmodelSurfPath::Cached),
                smodel_plan,
                catalog,
                cache,
                pretess,
                out,
                custom_skip.as_mut().map(|set| &mut **set),
            );
            stats.emitted = stats.emitted.saturating_add(one.emitted);
            stats.skipped_custom = stats.skipped_custom.saturating_add(one.skipped_custom);
        }
        return stats;
    }
    if let Some(alloc) = decision.pretess
        && decision.dest == lighting_iw4::SmodelSurfPath::Pretess
        && let Some(batch_range) = pretess.commit_pretess(alloc, &runs)
    {
        let mut start = batch_range.start;
        for &(surface, authored) in lod_surfs {
            let mut count = 0u32;
            for source in sources {
                if let Some(place) = smodel_plan.placements.get(source.placement as usize)
                    && let Some(run) = cached_index_run(cache, place, surface)
                {
                    count = count.saturating_add(run.len() as u32);
                }
            }
            let range = (count > 0).then_some(SmodelPretessRange { start, count });
            start = start.saturating_add(count);
            let one = push_smodel_destination(
                first,
                surface,
                authored,
                Some(lighting_iw4::SmodelSurfPath::Pretess),
                range,
                smodel_plan,
                catalog,
                out,
            );
            stats.emitted = stats.emitted.saturating_add(one.emitted);
            stats.skipped_custom = stats.skipped_custom.saturating_add(one.skipped_custom);
        }
        note_custom_only_skip(
            stats,
            first.placement,
            custom_skip.as_mut().map(|set| &mut **set),
        );
        return stats;
    }
    for &source in sources {
        let Some(place) = smodel_plan.placements.get(source.placement as usize) else {
            continue;
        };
        let mut one = SmodelExpandStats::default();
        for &(surface, authored) in lod_surfs {
            let range = cached_index_run(cache, place, surface)
                .and_then(|run| pretess.append_cached_fallback(run));
            let pushed = push_smodel_destination(
                source,
                surface,
                authored,
                Some(lighting_iw4::SmodelSurfPath::Cached),
                range,
                smodel_plan,
                catalog,
                out,
            );
            one.emitted = one.emitted.saturating_add(pushed.emitted);
            one.skipped_custom = one.skipped_custom.saturating_add(pushed.skipped_custom);
        }
        note_custom_only_skip(
            one,
            source.placement,
            custom_skip.as_mut().map(|set| &mut **set),
        );
        stats.emitted = stats.emitted.saturating_add(one.emitted);
        stats.skipped_custom = stats.skipped_custom.saturating_add(one.skipped_custom);
    }
    stats
}

fn smodel_cached_batch_state(placement: &SmodelPlacement) -> (usize, bool, u8, u8) {
    (
        placement.mesh,
        placement.lit,
        placement.reflection_probe_index,
        placement.primary_light_index,
    )
}

fn consume_smodel_buckets(
    lists: &mut lighting_iw4::SmodelSurfBucketLists,
    queues: &mut SmodelBucketEmitQueues,
    active_mask: u32,
    smodel_plan: &SmodelGpuPlan,
    catalog: &super::RuntimeMaterialCatalog,
    cache: Option<&WorldStaticModelCache>,
    pretess: &mut SmodelPretessBuilder,
    out: &mut Vec<SmodelDestinationRecord>,
    mut custom_skip: Option<&mut HashSet<u32>>,
) -> SmodelConsumeResult {
    let mut consumed = [None; lighting_iw4::SMODEL_BUCKET_LIST_N];
    let n = lighting_iw4::smodel_bucket_lists_consume(lists, active_mask, &mut consumed);
    let mut result = SmodelConsumeResult {
        buckets: n as u32,
        context_refused: 0,
        skipped_custom: 0,
    };
    for bucket in consumed.into_iter().take(n).flatten() {
        let sources = std::mem::take(&mut queues.rows[bucket.bucket as usize]);
        if sources.len() != usize::from(bucket.count) {
            result.context_refused = result.context_refused.saturating_add(
                u32::try_from(sources.len().abs_diff(usize::from(bucket.count)))
                    .unwrap_or(u32::MAX),
            );
        }
        let count = usize::from(bucket.count).min(sources.len());
        let mut i = 0usize;
        while i < count {
            let source = sources[i];
            if source.payload != bucket.ids[i] || source.lod != bucket.lod {
                result.context_refused = result.context_refused.saturating_add(1);
                i += 1;
                continue;
            }
            if bucket.source != lighting_iw4::SmodelSurfPath::Cached {
                let stats = expand_smodel_destination(
                    source,
                    Some(bucket.source),
                    smodel_plan,
                    catalog,
                    cache,
                    pretess,
                    out,
                    custom_skip.as_mut().map(|set| &mut **set),
                );
                result.skipped_custom = result.skipped_custom.saturating_add(stats.skipped_custom);
                i += 1;
                continue;
            }
            let ids = &bucket.ids[i..count];
            let rest = &sources[i..count];
            let bank_n = lighting_iw4::smodel_same_bank_count(ids).min(rest.len());
            let batch_state = smodel_plan
                .placements
                .get(source.placement as usize)
                .map(smodel_cached_batch_state);
            let mut n = 1usize;
            while n < bank_n {
                let next = rest[n];
                if next.payload != ids[n]
                    || next.lod != bucket.lod
                    || smodel_plan
                        .placements
                        .get(next.placement as usize)
                        .map(smodel_cached_batch_state)
                        != batch_state
                {
                    break;
                }
                n += 1;
            }
            let stats = expand_cached_destination_batch(
                &rest[..n],
                smodel_plan,
                catalog,
                cache,
                pretess,
                out,
                custom_skip.as_mut().map(|set| &mut **set),
            );
            result.skipped_custom = result.skipped_custom.saturating_add(stats.skipped_custom);
            i += n;
        }
    }
    result
}

fn consume_smodel_bucket_tail(
    lists: &mut lighting_iw4::SmodelSurfBucketLists,
    queues: &mut SmodelBucketEmitQueues,
    smodel_plan: &SmodelGpuPlan,
    catalog: &super::RuntimeMaterialCatalog,
    cache: Option<&WorldStaticModelCache>,
    pretess: &mut SmodelPretessBuilder,
    out: &mut Vec<SmodelDestinationRecord>,
    custom_skip: Option<&mut HashSet<u32>>,
) -> SmodelConsumeResult {
    let mut mask = 0u32;
    for (bucket, count) in lists.count.iter().enumerate() {
        if *count > 0
            && let Some(bucket_mask) = lighting_iw4::smodel_bucket_mask(bucket as u8)
        {
            mask |= bucket_mask;
        }
    }
    consume_smodel_buckets(
        lists,
        queues,
        mask,
        smodel_plan,
        catalog,
        cache,
        pretess,
        out,
        custom_skip,
    )
}

pub(crate) fn rebuild_xmodel_draw_lane(
    plans: (
        Option<Res<super::tess::xmodel::FpvDrawPlan>>,
        Option<Res<super::tess::xmodel::RemoteBodyDrawPlan>>,
        Option<Res<super::tess::xmodel::ScriptModelDrawPlan>>,
        Option<Res<super::tess::xmodel::MissileDrawPlan>>,
        Option<Res<super::tess::xmodel::ItemDrawPlan>>,
        Option<Res<super::tess::xmodel::FxModelDrawPlan>>,
        Option<Res<super::tess::xmodel::DynEntDrawPlan>>,
        Option<Res<crate::prepare::scene::gfx_scene::HostGfxScene>>,
    ),
    mut xmodel: ResMut<XModelDrawPlan>,
    sky: Option<Res<super::tess::sky::SkyModelDrawPlan>>,
    prepared: Option<Res<PreparedSceneView>>,
    runtime: Res<super::MaterialGeneration>,
    mut lane: ResMut<XModelDrawLane>,
    presentation: (
        Res<net::PresentedSnapshot>,
        Res<frame::ScreenEffectsView>,
        Res<render_anim::gaps::RenderPresentationGaps>,
    ),
) {
    let (fpv, bodies, scripts, missiles, items, fx_models, dynents, gfx_scene) = plans;
    let empty_fpv = super::tess::xmodel::FpvDrawPlan::default();
    let empty_body = super::tess::xmodel::RemoteBodyDrawPlan::default();
    let empty_scripts = super::tess::xmodel::ScriptModelDrawPlan::default();
    let empty_missiles = super::tess::xmodel::MissileDrawPlan::default();
    let empty_items = super::tess::xmodel::ItemDrawPlan::default();
    let empty_fx_models = super::tess::xmodel::FxModelDrawPlan::default();
    let empty_dynents = super::tess::xmodel::DynEntDrawPlan::default();
    merge_xmodel_draw_plan(
        &mut xmodel,
        fpv.as_deref().unwrap_or(&empty_fpv),
        bodies.as_deref().unwrap_or(&empty_body),
        scripts.as_deref().unwrap_or(&empty_scripts),
        missiles.as_deref().unwrap_or(&empty_missiles),
        items.as_deref().unwrap_or(&empty_items),
        fx_models.as_deref().unwrap_or(&empty_fx_models),
        dynents.as_deref().unwrap_or(&empty_dynents),
        gfx_scene.as_ref().map(|g| &g.scene),
        sky.as_deref()
            .zip(prepared.as_ref().filter(|v| v.ready).map(|v| v.eye)),
    );

    let (presented, effects, gaps) = presentation;
    let thermal =
        super::thermal_body::ThermalBodySelection::new(&presented, &effects, &runtime.catalog);
    if let Some(name) = xmodel
        .draws
        .iter()
        .find_map(|draw| match thermal.for_draw(draw) {
            Some(Err(name)) => Some(name),
            _ => None,
        })
    {
        gaps.raise(
            render_anim::gaps::RenderGapCause::ThermalBodyMaterialMissing { name: name.clone() },
        );
    } else {
        gaps.clear(render_anim::gaps::RenderGap::ThermalBodyMaterial);
    }
    let layout = xmodel_lane_layout_hash(&xmodel, &runtime.catalog, &thermal);
    lane.merge_packed_n = xmodel.packed_rows().map(|rows| rows.len() as u32);
    lane.fx_object_id_exhausted = xmodel.fx_object_id_exhausted;
    if layout == lane.membership_hash && overlay_xmodel_lane_payload(&xmodel, &mut lane) {
        lane.payload_revision = xmodel.revision;
        perf::Counter::XmodelLayoutOverlay.emit(1.0);
        return;
    }
    perf::Counter::XmodelLayoutOverlay.emit(0.0);

    lane.colour.clear();
    lane.emissive.clear();
    lane.distortion.clear();
    lane.colour_source.clear();
    lane.emissive_source.clear();
    lane.distortion_source.clear();
    lane.merge_packed_n = xmodel.packed_rows().map(|rows| rows.len() as u32);
    lane.sorted = 0;
    lane.object_id_standin = 0;
    lane.fx_object_id_exhausted = xmodel.fx_object_id_exhausted;
    lane.skipped_no_ordinal = 0;
    lane.skipped_no_baked_key = 0;
    lane.skipped_camera_frustum = 0;
    lane.skipped_no_lighting = 0;

    for (source, draw) in xmodel.draws.iter().enumerate() {
        let source = u32::try_from(source).unwrap_or(u32::MAX);
        if matches!(
            draw.colour_refusal,
            Some(super::tess::xmodel::XModelColourRefusal::CameraFrustum)
        ) {
            lane.skipped_camera_frustum = lane.skipped_camera_frustum.saturating_add(1);
            continue;
        }
        let Some((material_sorted_index, model_lighting_required)) =
            xmodel_camera_material(&xmodel, draw, &runtime.catalog, &thermal)
        else {
            lane.skipped_no_ordinal = lane.skipped_no_ordinal.saturating_add(1);
            continue;
        };
        if model_lighting_required && draw.lighting_handle == 0 {
            lane.skipped_no_lighting = lane.skipped_no_lighting.saturating_add(1);
            continue;
        }
        let Some(baked) = runtime
            .catalog
            .material_for_sorted_ordinal(material_sorted_index)
            .and_then(|material| material.baked_draw_surf)
        else {
            lane.skipped_no_baked_key = lane.skipped_no_baked_key.saturating_add(1);
            continue;
        };
        let key = with_reflection_probe_index(
            pack_xmodel_rigid_skinned_draw_surf(GfxDrawSurf::from_packed(baked), draw.object_id),
            draw.reflection_probe_index,
        )
        .packed;
        let key = crate::assemble::drawsurf::with_scene_light_index(key, draw.scene_light_index);
        let packed_key = dpvs_iw4::GfxDrawSurf::from_packed(key);
        debug_assert_eq!(packed_key.surf_type(), SF_XMODEL_RIGID_SKINNED);
        let item = with_catalog(
            key,
            material_sorted_index,
            RetainedDrawKind::XModel {
                surface: draw.surface,
                material: draw.material,
                object_id: draw.object_id,
                world_from_local: draw.world_from_local,
                lighting_handle: draw.lighting_handle,
                packed_lighting: draw.packed_lighting,
                is_scope: draw.is_scope,
                scene_entnum: draw.scene_entnum,
            },
            super::SurfaceSamplerInputs {
                reflection_probe: Some(super::SurfaceReflectionProbeId(
                    packed_key.reflection_probe_index(),
                )),
                ..Default::default()
            },
            &runtime.catalog,
        );
        lane.distortion.push(item);
        lane.distortion_source.push(source);
        match super::frame_product_kind_for_camera_region(item.camera_region) {
            Some(super::FrameProductKind::Emissive) => {
                lane.emissive.push(item);
                lane.emissive_source.push(source);
            }
            Some(super::FrameProductKind::Colour) => {
                lane.colour.push(item);
                lane.colour_source.push(source);
            }
            Some(_) | None => {}
        }
        lane.sorted = lane.sorted.saturating_add(1);
        if draw.object_id >= XMODEL_OBJECT_ID_VIEWMODEL {
            lane.object_id_standin = lane.object_id_standin.saturating_add(1);
        }
    }
    let mut order = Vec::new();
    {
        let XModelDrawLane {
            colour,
            emissive,
            distortion,
            colour_source,
            emissive_source,
            distortion_source,
            ..
        } = &mut *lane;
        sort_xmodel_lane(colour, colour_source, &mut order);
        sort_xmodel_lane(emissive, emissive_source, &mut order);
        sort_xmodel_lane(distortion, distortion_source, &mut order);
    }
    lane.membership_hash = layout;
    lane.membership_revision = lane.membership_revision.wrapping_add(1);
    lane.payload_revision = xmodel.revision;
    perf::Counter::XmodelColourCameraFrustum.emit(f64::from(lane.skipped_camera_frustum));
    perf::Counter::XmodelColourNoLighting.emit(f64::from(lane.skipped_no_lighting));
}

pub(crate) fn rebuild_fx_draw_lane(
    plans: (
        Option<Res<FxCodeMeshPlan>>,
        Option<Res<FxParticleCloudPlan>>,
        Option<Res<GfxMarkMeshPlan>>,
        Option<Res<GfxGlassMeshPlan>>,
    ),
    runtime: Res<super::MaterialGeneration>,
    mut lane: ResMut<FxDrawLane>,
    mark_owners: Query<(
        Entity,
        &crate::prepare::scene::world::WorldScriptModelInstance,
    )>,
    lighting: Res<crate::prepare::scene::model_lighting_cache::ResolvedModelLightingTable>,
    smodel_lighting: Option<Res<crate::prepare::scene::smodel_lighting::WorldSmodelLighting>>,
    camera_origin: Option<Res<render_fx::FxCameraOrigin>>,
    scene: Option<Res<WorldScene>>,
) {
    let (fx, particle_cloud, mark_mesh, glass_mesh) = plans;
    let eye = camera_origin.map(|c| c.0).unwrap_or([0.0; 3]);
    let glass_plan = glass_mesh.as_deref();
    let distortion_key = scene
        .as_deref()
        .and_then(|scene| scene.cull.as_ref())
        .and_then(|cull| cull.sort_key_distortion);
    let lane = &mut *lane;
    let layout = lane_layout_hash(
        fx.as_deref(),
        particle_cloud.as_deref(),
        mark_mesh.as_deref(),
        glass_mesh.as_deref(),
    );
    let payload = mix_fx_payload_revision(
        fx.as_deref(),
        particle_cloud.as_deref(),
        mark_mesh.as_deref(),
        glass_mesh.as_deref(),
    );
    if layout == lane.membership_hash
        && overlay_fx_lane_payload(
            fx.as_deref(),
            particle_cloud.as_deref(),
            mark_mesh.as_deref(),
            glass_mesh.as_deref(),
            lane,
        )
    {
        sort_fx_draw_lane(lane, eye, glass_plan);
        lane.payload_revision = payload;
        perf::Counter::FxLayoutOverlay.emit(1.0);
        return;
    }
    perf::Counter::FxLayoutOverlay.emit(0.0);
    lane.colour.clear();
    lane.emissive.clear();
    lane.distortion.clear();
    lane.code_sorted = 0;
    lane.particle_cloud_sorted = 0;
    lane.mark_mesh_sorted = 0;
    lane.glass_mesh_sorted = 0;
    lane.code_skipped_no_ordinal = 0;
    lane.particle_cloud_skipped_no_ordinal = 0;
    lane.mark_mesh_skipped_no_ordinal = 0;
    lane.mark_mesh_skipped_no_lighting = 0;
    lane.glass_mesh_skipped_no_ordinal = 0;

    if let Some(fx) = fx.as_ref() {
        for (i, draw) in fx.draws.iter().enumerate() {
            if draw.index_count == 0 {
                continue;
            }
            let Some(mat) = fx.materials.get(draw.material as usize) else {
                continue;
            };
            let Some(material_sorted_index) = mat.material_sorted_index else {
                lane.code_skipped_no_ordinal = lane.code_skipped_no_ordinal.saturating_add(1);
                continue;
            };
            let key = pack_code_mesh_draw_surf(
                mat.sort_key,
                render_material::sort_band(material_sorted_index),
                i as u16,
            )
            .packed;
            let mut mesh_args = [[0.0f32; 4]; 2];
            let n = (draw.arg_count as usize).min(2);
            let start = draw.arg_start as usize;
            for (ai, slot) in mesh_args.iter_mut().enumerate().take(n) {
                if let Some(row) = fx.args.get(start + ai) {
                    *slot = *row;
                }
            }
            let item = with_catalog(
                key,
                material_sorted_index,
                RetainedDrawKind::CodeMesh {
                    viewmodel: draw.viewmodel,
                    draw: i as u32,
                    material: draw.material,
                    arg_count: n as u8,
                    args: mesh_args,
                },
                super::SurfaceSamplerInputs::default(),
                &runtime.catalog,
            );
            push_direct_lane_item(
                &mut lane.colour,
                &mut lane.emissive,
                Some(&mut lane.distortion),
                item,
            );
            lane.code_sorted = lane.code_sorted.saturating_add(1);
        }
    }

    if let Some(clouds) = particle_cloud.as_ref() {
        for (i, draw) in clouds.draws.iter().enumerate() {
            if draw.index_count == 0 {
                continue;
            }
            let Some(mat) = clouds.materials.get(draw.material as usize) else {
                continue;
            };
            let Some(material_sorted_index) = mat.material_sorted_index else {
                lane.particle_cloud_skipped_no_ordinal =
                    lane.particle_cloud_skipped_no_ordinal.saturating_add(1);
                continue;
            };
            let key = pack_particle_cloud_draw_surf(
                mat.sort_key,
                render_material::sort_band(material_sorted_index),
                i as u16,
            )
            .packed;
            let item = with_catalog(
                key,
                material_sorted_index,
                RetainedDrawKind::ParticleCloud {
                    draw: i as u32,
                    material: draw.material,
                    clouds: draw.clouds,
                },
                super::SurfaceSamplerInputs::default(),
                &runtime.catalog,
            );
            push_direct_lane_item(&mut lane.colour, &mut lane.emissive, None, item);
            lane.particle_cloud_sorted = lane.particle_cloud_sorted.saturating_add(1);
        }
    }

    if let Some(marks) = mark_mesh.as_ref() {
        for (i, draw) in marks.draws.iter().enumerate() {
            if draw.index_count == 0 {
                continue;
            }
            let Some(mat) = marks.materials.get(draw.material as usize) else {
                continue;
            };
            let Some(material_sorted_index) = mat.material_sorted_index else {
                lane.mark_mesh_skipped_no_ordinal =
                    lane.mark_mesh_skipped_no_ordinal.saturating_add(1);
                continue;
            };
            let mut sub_key = draw.sub_key;
            let lighting_handle = if let Some(number) = sub_key.entity {
                use crate::prepare::scene::model_lighting_cache::{
                    ModelLightingOwner, ResolvedModelLighting,
                };
                let result = mark_owners
                    .iter()
                    .find(|(_, owner)| owner.gentity_number == Some(number))
                    .and_then(|(entity, _)| lighting.get(ModelLightingOwner::ScriptModel(entity)));
                let Some(ResolvedModelLighting::Seated {
                    handle,
                    scene_light_index,
                    reflection_probe_index,
                    ..
                }) = result
                else {
                    lane.mark_mesh_skipped_no_lighting += 1;
                    continue;
                };
                sub_key.primary_light = scene_light_index;
                sub_key.probe = reflection_probe_index;
                handle
            } else if let Some(piece) = sub_key.glass {
                use crate::prepare::scene::model_lighting_cache::{
                    ModelLightingOwner, ResolvedModelLighting,
                };
                let Some(ResolvedModelLighting::Seated {
                    handle,
                    scene_light_index,
                    reflection_probe_index,
                    ..
                }) = lighting.get(ModelLightingOwner::Glass(piece))
                else {
                    lane.mark_mesh_skipped_no_lighting += 1;
                    continue;
                };
                sub_key.primary_light = scene_light_index;
                sub_key.probe = reflection_probe_index;
                handle
            } else if let Some(index) = sub_key.smodel {
                let Some(handle) = smodel_lighting
                    .as_ref()
                    .and_then(|l| l.handles.get(index as usize))
                    .copied()
                else {
                    lane.mark_mesh_skipped_no_lighting += 1;
                    continue;
                };
                u32::from(handle)
            } else {
                0
            };
            let key = pack_mark_mesh_draw_surf(
                mat.sort_key,
                render_material::sort_band(material_sorted_index),
                i as u16,
                sub_key.lmap,
                sub_key.primary_light,
                sub_key.probe,
            )
            .packed;
            let item = with_catalog(
                key,
                material_sorted_index,
                RetainedDrawKind::MarkMesh {
                    glass: sub_key.glass.is_some(),
                    draw: i as u32,
                    material: draw.material,
                    packed: sub_key.packed,
                    lighting_handle,
                },
                mark_mesh_surface_samplers(sub_key),
                &runtime.catalog,
            );
            push_direct_lane_item(&mut lane.colour, &mut lane.emissive, None, item);
            lane.mark_mesh_sorted = lane.mark_mesh_sorted.saturating_add(1);
        }
    }

    if let Some(glass) = glass_mesh.as_ref() {
        for (i, draw) in glass.draws.iter().enumerate() {
            if draw.index_count == 0 {
                continue;
            }
            let Some(mat) = glass.materials.get(draw.material as usize) else {
                continue;
            };
            let Some(material_sorted_index) = mat.material_sorted_index else {
                lane.glass_mesh_skipped_no_ordinal =
                    lane.glass_mesh_skipped_no_ordinal.saturating_add(1);
                continue;
            };
            let key = pack_glass_mesh_draw_surf(
                mat.sort_key,
                render_material::sort_band(material_sorted_index),
                i as u16,
                draw.reflection_probe_index,
            )
            .packed;
            let packed_key = dpvs_iw4::GfxDrawSurf::from_packed(key);
            let item = with_catalog(
                key,
                material_sorted_index,
                RetainedDrawKind::Glass {
                    draw: i as u32,
                    material: draw.material,
                    lighting_handle: draw.lighting_handle,
                },
                super::SurfaceSamplerInputs {
                    reflection_probe: Some(super::SurfaceReflectionProbeId(
                        packed_key.reflection_probe_index(),
                    )),
                    ..Default::default()
                },
                &runtime.catalog,
            );
            let distortion = if item_uses_distortion_lane(mat.sort_key, distortion_key) {
                Some(&mut lane.distortion)
            } else {
                None
            };
            push_direct_lane_item(&mut lane.colour, &mut lane.emissive, distortion, item);
            lane.glass_mesh_sorted = lane.glass_mesh_sorted.saturating_add(1);
        }
    }

    sort_fx_draw_lane(lane, eye, glass_plan);
    lane.membership_hash = layout;
    lane.membership_revision = lane.membership_revision.wrapping_add(1);
    lane.payload_revision = payload;
}

fn emit_world_static_lane(
    list: &mut StaticDrawLane,
    world_list: &DrawSurfList,
    world_plan: Option<&super::WorldDrawGpuPlan>,
    scene: Option<&WorldScene>,
    catalog: &super::material_runtime::RuntimeMaterialCatalog,
) {
    for item in &world_list.items {
        let surface_samplers = world_plan
            .and_then(|plan| plan.surface_sampler_inputs.get(usize::from(item.surf)))
            .copied()
            .unwrap_or_default();
        let world_from_local =
            scene
                .and_then(|scene| scene.cull.as_ref())
                .map_or(Mat4::IDENTITY, |cull| {
                    bmodel_world_from_local_for_surf(
                        item.surf,
                        &cull.brush_models,
                        &cull.bmodel_world_from_local,
                    )
                });
        let kind = match item.kind {
            crate::prepare::scene::world::WorldDrawItemKind::Bsp(kind) => {
                RetainedDrawKind::bsp_world(
                    item.surf,
                    item.run,
                    bsp_lane(kind),
                    item.surf,
                    item.setup_key_changed,
                )
            }
            crate::prepare::scene::world::WorldDrawItemKind::BModel => {
                RetainedDrawKind::world_with_pose(item.surf, world_from_local)
            }
        };

        let world_rank = world_surface_rank(
            scene.and_then(|scene| scene.cull.as_ref()),
            usize::from(item.surf),
            item.key,
            catalog,
        );
        list.world_items.push(with_catalog(
            item.key,
            world_rank,
            kind,
            surface_samplers,
            catalog,
        ));
        list.census.world_n = list.census.world_n.saturating_add(u32::from(item.run));
    }
}

fn emit_smodel_static_lane(
    list: &mut StaticDrawLane,
    plan: &SmodelGpuPlan,
    walk: &SmodelLodWalk,
    smc_cache: Option<&WorldStaticModelCache>,
    smc_on: bool,
    pretess_enabled: bool,
    catalog: &super::material_runtime::RuntimeMaterialCatalog,
) {
    let mut queues = SmodelBucketEmitQueues::default();
    let mut destinations = Vec::new();
    let mut pretess = SmodelPretessBuilder::new(pretess_enabled);
    list.census.smodel_hidden_n = walk.hidden_n;
    list.census.smodel_query_n = walk.query_n;
    list.last_smodel_picks.clone_from(&walk.picks);
    for pick in &walk.picks {
        let placement_i = pick.placement as usize;
        let Some(placement) = plan.placements.get(placement_i) else {
            continue;
        };
        let vis_slot = placement.lighting_slot.unwrap_or(placement_i);
        let world_from_local = placement.world_from_local;
        let handle = pick.lighting_handle;
        let packed_lighting = pick.packed_lighting;
        let Some(mesh) = plan.meshes.get(placement.mesh) else {
            continue;
        };
        let lod = pick.lod;
        let Some(smodel_index) = u16::try_from(vis_slot).ok() else {
            list.census.smodel_bucket_context_refused_n = list
                .census
                .smodel_bucket_context_refused_n
                .saturating_add(1);
            continue;
        };
        let lod_i = usize::from(lod);
        let cache_index = pick.cache_index;
        let source = SmodelBucketEmitSource {
            placement: placement_i as u32,
            lod,
            payload: 0,
            world_from_local,
            lighting_handle: handle,
            packed_lighting,
            pass: SmodelDestinationPass::Colour,
        };
        let (bucket, full) = push_smodel_surf_bucket(
            &mut list.smodel_surf_lists,
            &mut queues,
            i32::from(lod),
            smc_on,
            packed_lighting_dword_nonzero(packed_lighting),
            smodel_lod_smc_flag(mesh, lod_i),
            cache_index,
            smodel_index,
            smodel_lod_is_rigid(mesh, lod_i),
            source,
        );
        list.census.note_smodel_bucket(bucket, full);
        if bucket.is_none() {
            expand_smodel_destination(
                source,
                None,
                plan,
                catalog,
                smc_cache,
                &mut pretess,
                &mut destinations,
                None,
            );
        } else if full
            && let Some(mask) = bucket
                .and_then(|bucket| u8::try_from(bucket).ok())
                .and_then(lighting_iw4::smodel_bucket_mask)
        {
            let consumed = consume_smodel_buckets(
                &mut list.smodel_surf_lists,
                &mut queues,
                mask,
                plan,
                catalog,
                smc_cache,
                &mut pretess,
                &mut destinations,
                None,
            );
            list.census.smodel_bucket_consume_n = list
                .census
                .smodel_bucket_consume_n
                .saturating_add(consumed.buckets);
            list.census.smodel_bucket_context_refused_n = list
                .census
                .smodel_bucket_context_refused_n
                .saturating_add(consumed.context_refused);
        }
    }
    if list.census.smodel_bucket_unread_n > 0 {
        diag::warn!(
            World,
            "smodel arm unknown: {} of {} admitted placements carry no XSurface+1 byte (rigid/skinned undecidable) — dropped, not drawn",
            list.census.smodel_bucket_unread_n,
            list.census.smodel_query_n
        );
    }
    let consumed = consume_smodel_bucket_tail(
        &mut list.smodel_surf_lists,
        &mut queues,
        plan,
        catalog,
        smc_cache,
        &mut pretess,
        &mut destinations,
        None,
    );
    list.census.smodel_bucket_consume_n = list
        .census
        .smodel_bucket_consume_n
        .saturating_add(consumed.buckets);
    list.census.smodel_bucket_context_refused_n = list
        .census
        .smodel_bucket_context_refused_n
        .saturating_add(consumed.context_refused);
    let buckets = [
        list.census.smodel_bucket_rigid_n,
        list.census.smodel_bucket_skinned_n,
        list.census.smodel_bucket_cached_n,
        list.census.smodel_bucket_unread_n,
        list.census.smodel_bucket_consume_n,
    ];
    if list.logged_buckets != Some(buckets) {
        list.logged_buckets = Some(buckets);
        let [rigid, skinned, cached, unread, consume] = buckets;
        diag::info!(
            World,
            "smodel buckets: rigid={rigid} skinned={skinned} cached={cached} unread={unread} consume={consume}",
        );
    }
    if list.smodel_pretess_indices.as_slice() != pretess.indices.as_slice() {
        list.smodel_index_layout_revision = list.smodel_index_layout_revision.wrapping_add(1);
        list.smodel_pretess_indices = Arc::new(pretess.indices);
    }
    for destination in destinations {
        list.census.smodel_n = list.census.smodel_n.saturating_add(1);
        if destination.packed_lighting.is_some() {
            list.census.smodel_probe59_n = list.census.smodel_probe59_n.saturating_add(1);
        } else {
            list.census.smodel_miss59_n = list.census.smodel_miss59_n.saturating_add(1);
        }
        list.smodel_items.push(destination.into_item(catalog));
    }
}

fn compose_static_lanes(list: &mut StaticDrawLane) {
    list.static_items.clear();
    list.static_items.extend_from_slice(&list.world_items);
    list.static_items.extend_from_slice(&list.smodel_items);
    let sort_started = Instant::now();
    list.static_items
        .sort_unstable_by_key(|i| (i.host_sort_key(), retained_draw_order_tie(&i.kind)));
    list.census.world_run_n =
        materialize_world_runs(&mut list.static_items, &mut list.world_run_surfs);
    let mut colour = std::mem::take(&mut list.colour);
    let mut emissive = std::mem::take(&mut list.emissive);
    let mut distortion = std::mem::take(&mut list.distortion);
    colour.clear();
    emissive.clear();
    distortion.clear();
    for &item in &list.static_items {
        let distortion_out =
            matches!(item.kind, RetainedDrawKind::World { .. }).then_some(&mut distortion);
        push_direct_lane_item(&mut colour, &mut emissive, distortion_out, item);
    }
    list.colour = colour;
    list.emissive = emissive;
    list.distortion = distortion;
    list.census.sort_us = sort_started.elapsed().as_micros() as u32;
    list.membership_revision = list.membership_revision.wrapping_add(1);
}

fn apply_lod_hold_census(list: &mut StaticDrawLane, walk: SmodelLodWalk, smodel_vis: &[u8]) {
    list.census.rebuild_skip = 0;
    list.census.lod_hold = 1;
    list.census.smodel_query_n = walk.query_n;
    list.census.smodel_hidden_n = walk.hidden_n;
    list.census.sort_us = 0;
    note_smodel_vis(&mut list.census, smodel_vis);
    list.last_smodel_picks = walk.picks;
}

fn restore_held_smodel_census(dst: &mut RetainedRebuildCensus, src: &RetainedRebuildCensus) {
    dst.smodel_n = src.smodel_n;
    dst.smodel_probe59_n = src.smodel_probe59_n;
    dst.smodel_miss59_n = src.smodel_miss59_n;
    dst.smodel_bucket_flush_n = src.smodel_bucket_flush_n;
    dst.smodel_bucket_rigid_n = src.smodel_bucket_rigid_n;
    dst.smodel_bucket_skinned_n = src.smodel_bucket_skinned_n;
    dst.smodel_bucket_cached_n = src.smodel_bucket_cached_n;
    dst.smodel_bucket_unread_n = src.smodel_bucket_unread_n;
    dst.smodel_bucket_consume_n = src.smodel_bucket_consume_n;
    dst.smodel_bucket_context_refused_n = src.smodel_bucket_context_refused_n;
}

pub(crate) fn rebuild_static_draw_lane(
    mut list: ResMut<StaticDrawLane>,
    world_list: Res<DrawSurfList>,
    world_geom: (
        Option<Res<super::WorldDrawGpuPlan>>,
        Option<Res<WorldScene>>,
    ),
    smodel: (
        Option<Res<SmodelGpuPlan>>,
        Option<Res<WorldSmodelLighting>>,
        Option<Res<DpvsFrameStats>>,
        Option<Res<WorldStaticModelCache>>,
        Res<SmcEnableDvar>,
        Res<PretessDvar>,
    ),
    prepared: Option<Res<PreparedSceneView>>,
    lod_ramp: Res<LodRampDvar>,
    runtime: Res<super::MaterialGeneration>,
    world_generation: Option<Res<WorldGeneration>>,
) {
    let _post_rebuild = perf::Span::HostPostRebuildMs.enter();
    let (world_plan, scene) = world_geom;
    let (smodel_plan, lighting, dpvs, smc_cache, smc_enable, pretess_dvar) = smodel;
    let smc_on = smc_enable.enabled != Some(false);
    let smodel_vis = dpvs
        .as_ref()
        .map(|stats| stats.smodel_vis.as_slice())
        .unwrap_or(&[]);
    let world_id = world_list.draw_items_id;
    let vis_id = dpvs.as_ref().map(|stats| stats.smodel_vis_id).unwrap_or(0);
    let world_generation = world_generation.map(|g| *g).unwrap_or_default();
    let eye = prepared.as_ref().filter(|v| v.ready).map(|v| v.eye);
    let lod_args = lod_ramp.args();
    let eye_key = eye_lod_reuse_key(eye, lod_args);
    let generation_hold = list.generation_id == runtime.catalog.generation_id()
        && list.world_generation == world_generation;
    let reuse = static_list_reusable(
        list.last_static,
        world_id,
        vis_id,
        eye_key,
        generation_hold,
        !list.static_items.is_empty(),
    );
    list.generation_id = runtime.catalog.generation_id();
    list.world_generation = world_generation;
    if reuse {
        list.census.rebuild_skip = 1;
        list.census.lod_hold = 0;
        list.census.smodel_query_n = 0;
        list.census.sort_us = 0;
        note_smodel_vis(&mut list.census, smodel_vis);
    } else {
        let world_hold = world_static_reusable(
            list.last_static.map(|(world, _, _)| world),
            world_id,
            generation_hold,
        );
        let cull_dists = scene
            .as_ref()
            .and_then(|scene| scene.cull.as_ref())
            .map(|cull| cull.static_model_cull_dists.as_slice())
            .unwrap_or(&[]);
        let smodel_walk = collect_smodel_lod_picks(
            smodel_plan.as_deref(),
            smodel_vis,
            cull_dists,
            eye,
            lod_args,
            lighting.as_deref(),
            smc_cache.as_deref(),
        );
        let smodel_hold = generation_hold
            && list.last_static.is_some()
            && smodel_descriptors_reusable(
                Some(list.last_smodel_picks.as_slice()),
                &smodel_walk.picks,
            );
        if world_hold && smodel_hold {
            apply_lod_hold_census(list.as_mut(), smodel_walk, smodel_vis);
        } else {
            let prev = list.census;
            if !world_hold {
                list.world_items.clear();
                list.world_run_surfs.clear();
            }
            if !smodel_hold {
                list.smodel_items.clear();
                list.smodel_surf_lists = lighting_iw4::SmodelSurfBucketLists::default();
                list.smodel_pretess_indices = Arc::new(Vec::new());
                list.smodel_index_layout_revision =
                    list.smodel_index_layout_revision.wrapping_add(1);
                list.last_smodel_picks.clear();
            }
            list.static_items.clear();
            list.colour.clear();
            list.emissive.clear();
            list.distortion.clear();
            list.census = RetainedRebuildCensus::default();
            note_smodel_vis(&mut list.census, smodel_vis);
            if world_hold {
                list.census.world_n = prev.world_n;
            }
            if smodel_hold {
                restore_held_smodel_census(&mut list.census, &prev);
                list.census.smodel_query_n = smodel_walk.query_n;
                list.census.smodel_hidden_n = smodel_walk.hidden_n;
                list.last_smodel_picks.clone_from(&smodel_walk.picks);
            }
            if !world_hold {
                emit_world_static_lane(
                    list.as_mut(),
                    &world_list,
                    world_plan.as_deref(),
                    scene.as_deref(),
                    &runtime.catalog,
                );
            }
            if !smodel_hold && let Some(plan) = smodel_plan.as_deref() {
                emit_smodel_static_lane(
                    list.as_mut(),
                    plan,
                    &smodel_walk,
                    smc_cache.as_deref(),
                    smc_on,
                    pretess_dvar.enabled,
                    &runtime.catalog,
                );
            }
            compose_static_lanes(list.as_mut());
        }
    }
    list.last_static = Some((world_id, vis_id, eye_key));
}

#[derive(Resource, Clone, Debug, Default)]
pub struct SunShadowCasterPlan {
    pub generation_id: super::MaterialGenerationId,
    pub items: Vec<RetainedDrawItem>,

    pub surface_vis_sun: [Vec<u8>; 2],

    pub smodel_vis_sun: [Vec<u8>; 2],
    pub world_eligible: u32,
    pub world_missing_key: u32,
    pub smodel_eligible: u32,
    pub smodel_excluded: u32,
    pub smodel_missing_key: u32,

    pub cutout_plus23: u32,

    pub cutout_missing_key: u32,

    pub cutout_empty_ib: u32,

    pub cutout_custom0: u32,

    pub bsp_ids: Vec<u16>,
    pub smodel_ids: Vec<u16>,
    pub bsp_ids_far: Vec<u16>,
    pub smodel_ids_far: Vec<u16>,

    pub xmodel_eligible: u32,

    pub xmodel_skipped_viewmodel: u32,

    pub xmodel_missing_key: u32,

    pub xmodel_no_technique: u32,

    pub smodel_no_custom: u32,

    pub lists: render_frame::SunShadowCasterLists,

    pub sun_near_n: usize,

    pub smodel_surf_lists: lighting_iw4::SmodelSurfBucketLists,
    pub smodel_bucket_flush_n: u32,
    pub smodel_bucket_rigid_n: u32,
    pub smodel_bucket_skinned_n: u32,
    pub smodel_bucket_cached_n: u32,
    pub smodel_bucket_unread_n: u32,
    pub smodel_bucket_consume_n: u32,
    pub smodel_bucket_context_refused_n: u32,
    pub smodel_pretess_indices: Vec<u16>,
}

impl SunShadowCasterPlan {
    fn note_smodel_bucket(&mut self, bucket: Option<i32>, full: bool) {
        let Some(bucket) = bucket else {
            self.smodel_bucket_unread_n = self.smodel_bucket_unread_n.saturating_add(1);
            return;
        };
        match bucket & 3 {
            lighting_iw4::SMODEL_BUCKET_CACHED => {
                self.smodel_bucket_cached_n = self.smodel_bucket_cached_n.saturating_add(1);
            }
            lighting_iw4::SMODEL_BUCKET_SKINNED => {
                self.smodel_bucket_skinned_n = self.smodel_bucket_skinned_n.saturating_add(1);
            }
            _ => {
                self.smodel_bucket_rigid_n = self.smodel_bucket_rigid_n.saturating_add(1);
            }
        }
        if full {
            self.smodel_bucket_flush_n = self.smodel_bucket_flush_n.saturating_add(1);
        }
    }
}

#[must_use]
pub(crate) fn bmodel_world_from_local_for_surf(
    surf: u16,
    models: &[asset_world::GfxBrushModelSurfs],
    poses: &[Mat4],
) -> Mat4 {
    let s = usize::from(surf);
    for (index, model) in models.iter().enumerate().skip(1) {
        let start = usize::from(model.start_surf);
        let end = start.saturating_add(usize::from(model.surface_count));
        if s >= start && s < end {
            return poses.get(index).copied().unwrap_or(Mat4::IDENTITY);
        }
    }
    Mat4::IDENTITY
}

pub(crate) fn extra_bmodel_surfs_with_pose(
    models: &[asset_world::GfxBrushModelSurfs],
    already_bit0: &asset_world::SurfaceCastsSunShadow,
    world_from_local: &[Mat4],
) -> Vec<(usize, Mat4)> {
    let mut extra = Vec::new();
    for (index, model) in models.iter().enumerate().skip(1) {
        let pose = world_from_local
            .get(index)
            .copied()
            .unwrap_or(Mat4::IDENTITY);
        let start = usize::from(model.start_surf);
        let end = start.saturating_add(usize::from(model.surface_count));
        let last = end.min(already_bit0.len());
        for surf in start..last {
            if !already_bit0.get(surf) {
                extra.push((surf, pose));
            }
        }
    }
    extra
}

fn emit_world_sun_shadow_surf(
    surf: usize,
    cull: &crate::prepare::scene::world::WorldCull,
    world_plan: &super::WorldDrawGpuPlan,
    catalog: &super::RuntimeMaterialCatalog,
    plan: &mut SunShadowCasterPlan,
    world_from_local: Mat4,
) {
    plan.world_eligible = plan.world_eligible.saturating_add(1);
    let material = cull
        .surface_materials
        .get(surf)
        .copied()
        .flatten()
        .and_then(|id| catalog.derived(id));
    let cutout = material
        .map(|material| super::sun_shadow_cutout_name(&material.name))
        .unwrap_or(false);
    if cutout {
        plan.cutout_plus23 = plan.cutout_plus23.saturating_add(1);
    }
    let empty_ib = world_plan
        .surface_ranges()
        .get(surf)
        .map(|&(_, count)| count == 0)
        .unwrap_or(true);
    if cutout && empty_ib {
        plan.cutout_empty_ib = plan.cutout_empty_ib.saturating_add(1);
    }
    let Some(surf_u16) = u16::try_from(surf).ok() else {
        plan.world_missing_key = plan.world_missing_key.saturating_add(1);
        if cutout {
            plan.cutout_missing_key = plan.cutout_missing_key.saturating_add(1);
        }
        return;
    };
    let packed = if let Some(word) = cull
        .capture
        .packed_draw_surfs
        .get(surf)
        .copied()
        .filter(|word| word.packed != 0)
    {
        word.packed
    } else {
        let Some(material_id) = cull.surface_materials.get(surf).copied().flatten() else {
            plan.world_missing_key = plan.world_missing_key.saturating_add(1);
            if cutout {
                plan.cutout_missing_key = plan.cutout_missing_key.saturating_add(1);
            }
            return;
        };
        let Some(packed) = catalog
            .derived(material_id)
            .and_then(|material| material.baked_draw_surf)
        else {
            plan.world_missing_key = plan.world_missing_key.saturating_add(1);
            if cutout {
                plan.cutout_missing_key = plan.cutout_missing_key.saturating_add(1);
            }
            return;
        };
        let scene_light = cull.surface_primary_lights.get(surf).copied().unwrap_or(0);
        super::with_scene_light_index(packed, scene_light)
    };
    if cutout && GfxDrawSurf::from_packed(packed).custom_index() == 0 {
        plan.cutout_custom0 = plan.cutout_custom0.saturating_add(1);
    }
    let samplers = world_plan
        .surface_sampler_inputs
        .get(surf)
        .copied()
        .unwrap_or_default();

    let world_rank = world_surface_rank(Some(cull), surf, packed, catalog);
    plan.items.push(with_catalog(
        packed,
        world_rank,
        RetainedDrawKind::world_with_pose(surf_u16, world_from_local),
        samplers,
        catalog,
    ));
}

pub(crate) fn sun_shadow_bsp_range(
    dpvs: &crate::prepare::scene::world::WorldDpvs,
    surf_count: usize,
) -> (u32, u32) {
    let begin = dpvs.lit_opaque_begin;
    let end = if dpvs.emissive_surfs_end > begin {
        dpvs.emissive_surfs_end
    } else if dpvs.lit_opaque_end > begin {
        dpvs.lit_opaque_end
    } else {
        surf_count as u32
    };
    (begin, end.min(surf_count as u32))
}

fn emit_world_sun_shadow_from_vis(
    vis: &[u8],
    cull: &crate::prepare::scene::world::WorldCull,
    world_plan: &super::WorldDrawGpuPlan,
    catalog: &super::RuntimeMaterialCatalog,
    plan: &mut SunShadowCasterPlan,
    bsp_ids: &mut Vec<u16>,
) {
    let n = cull.surface_materials.len();
    let (begin, end) = sun_shadow_bsp_range(&cull.dpvs, n);
    if !cull.capture.packed_draw_surfs.is_empty() {
        let cap = end.saturating_sub(begin) as usize;
        if bsp_ids.len() < cap {
            bsp_ids.resize(cap, 0);
        }
        let got = render_frontend::add_bsp_sun_shadow_partition(
            begin,
            end,
            vis,
            cull.capture.casters.words(),
            &cull.capture.packed_draw_surfs,
            &mut bsp_ids[..cap],
        );
        for &surf in &bsp_ids[..got] {
            emit_world_sun_shadow_surf(
                usize::from(surf),
                cull,
                world_plan,
                catalog,
                plan,
                Mat4::IDENTITY,
            );
        }
    } else {
        let last = (end as usize).min(n);
        for surf in begin as usize..last {
            if vis.get(surf).copied().unwrap_or(0) == 0 {
                continue;
            }
            if !cull.capture.casters.get(surf) {
                continue;
            }
            emit_world_sun_shadow_surf(surf, cull, world_plan, catalog, plan, Mat4::IDENTITY);
        }
    }
    for (surf, pose) in extra_bmodel_surfs_with_pose(
        &cull.brush_models,
        &cull.capture.casters,
        &cull.bmodel_world_from_local,
    ) {
        if vis.get(surf).copied().unwrap_or(0) == 0 {
            continue;
        }
        emit_world_sun_shadow_surf(surf, cull, world_plan, catalog, plan, pose);
    }
}

fn emit_smodel_sun_shadow_one(
    placement_i: usize,
    placement: &SmodelPlacement,
    smodel_plan: &SmodelGpuPlan,
    catalog: &super::RuntimeMaterialCatalog,
    plan: &mut SunShadowCasterPlan,
    eye: Option<Vec3>,
    ramp: LodRampArgs,
    buckets: SmodelBucketBakeSrc<'_>,
    queues: &mut SmodelBucketEmitQueues,
    pretess: &mut SmodelPretessBuilder,
    destinations: &mut Vec<SmodelDestinationRecord>,
    custom_skip: &mut HashSet<u32>,
) -> Option<u32> {
    if !super::smodel_casts_sun_shadow(placement.flags) {
        plan.smodel_excluded = plan.smodel_excluded.saturating_add(1);
        return None;
    }
    plan.smodel_eligible = plan.smodel_eligible.saturating_add(1);
    let Some(mesh) = smodel_plan.meshes.get(placement.mesh) else {
        plan.smodel_missing_key = plan.smodel_missing_key.saturating_add(1);
        return None;
    };
    let Some(lod) = smodel_camera_lod(mesh.lod, placement.origin, placement.scale, eye, ramp)
    else {
        plan.smodel_missing_key = plan.smodel_missing_key.saturating_add(1);
        return None;
    };
    let lod_surfs = mesh
        .surfaces_by_lod
        .get(usize::from(lod))
        .map(|v| v.as_slice())
        .unwrap_or(&[]);
    if lod_surfs.is_empty() {
        plan.smodel_missing_key = plan.smodel_missing_key.saturating_add(1);
        return None;
    }
    let lod_i = usize::from(lod);
    let packed = placement.packed_lighting.or_else(|| {
        placement.lighting_slot.and_then(|slot| {
            buckets
                .lighting
                .and_then(|l| l.packed_lighting_for_slot(slot))
        })
    });
    let Some(smodel_index) = u16::try_from(placement.lighting_slot.unwrap_or(placement_i)).ok()
    else {
        plan.smodel_bucket_context_refused_n =
            plan.smodel_bucket_context_refused_n.saturating_add(1);
        return None;
    };
    let source = SmodelBucketEmitSource {
        placement: placement_i as u32,
        lod,
        payload: 0,
        world_from_local: placement.world_from_local,
        lighting_handle: 0,
        packed_lighting: packed,
        pass: SmodelDestinationPass::SunShadow,
    };

    let (bucket, full) = push_smodel_surf_bucket(
        &mut plan.smodel_surf_lists,
        queues,
        i32::from(lod),
        false,
        packed_lighting_dword_nonzero(packed),
        smodel_lod_smc_flag(mesh, lod_i),
        smodel_cache_index_u16(buckets.cache, placement.lighting_slot, lod_i),
        smodel_index,
        smodel_lod_is_rigid(mesh, lod_i),
        source,
    );
    plan.note_smodel_bucket(bucket, full);
    if bucket.is_none() {
        let stats = expand_smodel_destination(
            source,
            None,
            smodel_plan,
            catalog,
            buckets.cache,
            pretess,
            destinations,
            Some(custom_skip),
        );
        plan.smodel_no_custom = plan.smodel_no_custom.saturating_add(stats.skipped_custom);
    } else if full
        && let Some(mask) = bucket
            .and_then(|bucket| u8::try_from(bucket).ok())
            .and_then(lighting_iw4::smodel_bucket_mask)
    {
        let consumed = consume_smodel_buckets(
            &mut plan.smodel_surf_lists,
            queues,
            mask,
            smodel_plan,
            catalog,
            buckets.cache,
            pretess,
            destinations,
            Some(custom_skip),
        );
        plan.smodel_bucket_consume_n = plan
            .smodel_bucket_consume_n
            .saturating_add(consumed.buckets);
        plan.smodel_bucket_context_refused_n = plan
            .smodel_bucket_context_refused_n
            .saturating_add(consumed.context_refused);
        plan.smodel_no_custom = plan
            .smodel_no_custom
            .saturating_add(consumed.skipped_custom);
    }
    Some(source.placement)
}

#[derive(Clone, Copy, Default)]
pub struct SmodelBucketBakeSrc<'a> {
    pub pretess_enable: bool,
    pub cache: Option<&'a WorldStaticModelCache>,
    pub lighting: Option<&'a WorldSmodelLighting>,
}

pub fn bake_sun_shadow_caster_plan(
    cull: &crate::prepare::scene::world::WorldCull,
    smodel_draw_insts: &[dpvs_iw4::GfxStaticModelDrawInstShadow],
    world_plan: &super::WorldDrawGpuPlan,
    smodel_plan: &SmodelGpuPlan,
    catalog: &super::RuntimeMaterialCatalog,
    world_vis: Option<&[u8]>,
    smodel_vis: Option<&[u8]>,
    eye: Option<Vec3>,
    ramp: LodRampArgs,
    buckets: SmodelBucketBakeSrc<'_>,
    bsp_ids: &mut Vec<u16>,
    smodel_ids: &mut Vec<u16>,
) -> SunShadowCasterPlan {
    let mut plan = SunShadowCasterPlan {
        generation_id: catalog.generation_id(),
        ..Default::default()
    };
    let mut queues = SmodelBucketEmitQueues::default();
    let mut pretess = SmodelPretessBuilder::new(buckets.pretess_enable);
    let mut destinations = Vec::new();
    let mut destination_sources = Vec::new();
    let mut custom_skip = HashSet::new();
    if let Some(vis) = world_vis {
        emit_world_sun_shadow_from_vis(vis, cull, world_plan, catalog, &mut plan, bsp_ids);
    } else {
        let n = cull.capture.casters.len().max(cull.surface_materials.len());
        for surf in 0..n {
            if !cull.capture.casters.get(surf) {
                continue;
            }
            emit_world_sun_shadow_surf(surf, cull, world_plan, catalog, &mut plan, Mat4::IDENTITY);
        }
        for (surf, pose) in extra_bmodel_surfs_with_pose(
            &cull.brush_models,
            &cull.capture.casters,
            &cull.bmodel_world_from_local,
        ) {
            emit_world_sun_shadow_surf(surf, cull, world_plan, catalog, &mut plan, pose);
        }
    }
    if let Some(vis) = smodel_vis {
        let cap = vis.len().min(smodel_draw_insts.len());
        if smodel_ids.len() < cap {
            smodel_ids.resize(cap, 0);
        }
        let got =
            dpvs_iw4::add_smodel_range_sun_shadow(vis, smodel_draw_insts, &mut smodel_ids[..cap]);
        for &id in &smodel_ids[..got] {
            let authored_slot = usize::from(id);
            let Some(placement_i) = smodel_plan
                .authored_placement_indices
                .get(authored_slot)
                .copied()
                .flatten()
            else {
                plan.smodel_missing_key = plan.smodel_missing_key.saturating_add(1);
                continue;
            };
            let Some(placement) = smodel_plan.placements.get(placement_i) else {
                plan.smodel_missing_key = plan.smodel_missing_key.saturating_add(1);
                continue;
            };
            if let Some(source) = emit_smodel_sun_shadow_one(
                placement_i,
                placement,
                smodel_plan,
                catalog,
                &mut plan,
                eye,
                ramp,
                buckets,
                &mut queues,
                &mut pretess,
                &mut destinations,
                &mut custom_skip,
            ) {
                destination_sources.push(source);
            }
        }
    } else {
        for (placement_i, placement) in smodel_plan.placements.iter().enumerate() {
            if let Some(source) = emit_smodel_sun_shadow_one(
                placement_i,
                placement,
                smodel_plan,
                catalog,
                &mut plan,
                eye,
                ramp,
                buckets,
                &mut queues,
                &mut pretess,
                &mut destinations,
                &mut custom_skip,
            ) {
                destination_sources.push(source);
            }
        }
    }
    let consumed = consume_smodel_bucket_tail(
        &mut plan.smodel_surf_lists,
        &mut queues,
        smodel_plan,
        catalog,
        buckets.cache,
        &mut pretess,
        &mut destinations,
        Some(&mut custom_skip),
    );
    plan.smodel_bucket_consume_n = plan
        .smodel_bucket_consume_n
        .saturating_add(consumed.buckets);
    plan.smodel_bucket_context_refused_n = plan
        .smodel_bucket_context_refused_n
        .saturating_add(consumed.context_refused);
    plan.smodel_no_custom = plan
        .smodel_no_custom
        .saturating_add(consumed.skipped_custom);
    let destination_placements: HashSet<u32> =
        destinations.iter().map(|record| record.placement).collect();
    plan.smodel_missing_key = plan.smodel_missing_key.saturating_add(
        destination_sources
            .into_iter()
            .filter(|source| {
                !destination_placements.contains(source) && !custom_skip.contains(source)
            })
            .count() as u32,
    );
    plan.smodel_pretess_indices = pretess.indices;
    plan.items.extend(
        destinations
            .into_iter()
            .map(|record| record.into_item(catalog)),
    );
    plan.items
        .sort_unstable_by_key(|item| (item.host_sort_key(), retained_draw_order_tie(&item.kind)));
    plan
}

pub(crate) fn fill_smodel_draw_inst_shadow(
    out: &mut Vec<dpvs_iw4::GfxStaticModelDrawInstShadow>,
    cull_dists: &[u16],
    smodel_plan: &SmodelGpuPlan,
) {
    let n = smodel_plan.authored_placement_indices.len();
    if out.len() != n {
        out.clear();
        out.resize(
            n,
            dpvs_iw4::GfxStaticModelDrawInstShadow {
                flags: 0,
                cull_dist: 0,
                origin: [0.0; 3],
            },
        );
    }
    out.fill(dpvs_iw4::GfxStaticModelDrawInstShadow {
        flags: 0,
        cull_dist: 0,
        origin: [0.0; 3],
    });
    for (authored_slot, placement_i) in smodel_plan.authored_placement_indices.iter().enumerate() {
        let Some(placement) = placement_i.and_then(|i| smodel_plan.placements.get(i)) else {
            continue;
        };
        out[authored_slot] = dpvs_iw4::GfxStaticModelDrawInstShadow {
            flags: placement.flags,
            cull_dist: cull_dists.get(authored_slot).copied().unwrap_or(0),
            origin: placement.world_from_local.w_axis.truncate().to_array(),
        };
    }
}

/// One dynamic caster and the sphere the partition test reads. A caster with no
/// sphere is admitted to both partitions, which is what the collector did for
/// every caster before the bound existed.
struct DynamicSunCaster {
    item: RetainedDrawItem,
    bound: Option<render_scene::XModelCasterBound>,
}

/// The dynamic casters a partition keeps, in the order `merge_presorted_retained`
/// needs. Filtering a sorted slice preserves the order, so the sort happens once
/// for both partitions.
fn partition_dynamic_casters(
    dynamic: &[DynamicSunCaster],
    planes: &[[f32; 4]],
) -> Vec<RetainedDrawItem> {
    dynamic
        .iter()
        .filter(|caster| dynamic_caster_kept(caster.bound, planes))
        .map(|caster| caster.item)
        .collect()
}

/// A caster is kept unless its own sphere is wholly outside the partition.
/// No planes and no bound both mean "kept": the partition that states nothing
/// and the producer that states nothing each widen the volume rather than
/// dropping a shadow.
fn dynamic_caster_kept(
    bound: Option<render_scene::XModelCasterBound>,
    planes: &[[f32; 4]],
) -> bool {
    if planes.is_empty() {
        return true;
    }
    bound.is_none_or(|bound| !bound.outside(planes))
}

pub(crate) fn merge_sun_shadow_caster_partitions(
    near: &mut SunShadowCasterPlan,
    far: &mut SunShadowCasterPlan,
    xmodel: Option<&XModelDrawPlan>,
    catalog: &super::RuntimeMaterialCatalog,
    partition_planes: [&[[f32; 4]]; 2],
) -> (Vec<RetainedDrawItem>, Vec<RetainedDrawItem>) {
    near.xmodel_eligible = 0;
    near.xmodel_skipped_viewmodel = 0;
    near.xmodel_missing_key = 0;
    near.xmodel_no_technique = 0;
    let mut dynamic = match xmodel {
        Some(xmodel) => collect_xmodel_sun_shadow_casters(xmodel, catalog, near),
        None => Vec::new(),
    };
    far.xmodel_eligible = near.xmodel_eligible;
    far.xmodel_skipped_viewmodel = near.xmodel_skipped_viewmodel;
    far.xmodel_missing_key = near.xmodel_missing_key;
    far.xmodel_no_technique = near.xmodel_no_technique;
    if dynamic.is_empty() {
        return (
            std::mem::take(&mut near.items),
            std::mem::take(&mut far.items),
        );
    }
    dynamic.sort_unstable_by_key(|caster| {
        (
            caster.item.host_sort_key(),
            retained_draw_order_tie(&caster.item.kind),
        )
    });
    let near_dynamic = partition_dynamic_casters(&dynamic, partition_planes[0]);
    let far_dynamic = partition_dynamic_casters(&dynamic, partition_planes[1]);
    (
        merge_presorted_retained(&near.items, &near_dynamic),
        merge_presorted_retained(&far.items, &far_dynamic),
    )
}

fn collect_xmodel_sun_shadow_casters(
    xmodel: &XModelDrawPlan,
    catalog: &super::RuntimeMaterialCatalog,
    plan: &mut SunShadowCasterPlan,
) -> Vec<DynamicSunCaster> {
    let mut items = Vec::new();
    for draw in &xmodel.draws {
        if draw.object_id == XMODEL_OBJECT_ID_VIEWMODEL || draw.is_scope {
            plan.xmodel_skipped_viewmodel = plan.xmodel_skipped_viewmodel.saturating_add(1);
            continue;
        }
        plan.xmodel_eligible = plan.xmodel_eligible.saturating_add(1);
        let Some(item) = xmodel_shadow_draw(xmodel, draw, catalog) else {
            plan.xmodel_missing_key = plan.xmodel_missing_key.saturating_add(1);
            continue;
        };
        if !super::material_runtime::add_surf_has_technique(
            catalog,
            render_material::MaterialDrawKey::new(item.key, item.material_rank),
            super::TechType(super::SUN_SHADOW_CASTER_TECH),
        ) {
            plan.xmodel_no_technique = plan.xmodel_no_technique.saturating_add(1);
            continue;
        }
        items.push(DynamicSunCaster {
            item,
            bound: draw.caster_bound,
        });
    }
    items
}

pub(super) fn xmodel_shadow_materials(
    xmodel: &XModelDrawPlan,
    catalog: &super::RuntimeMaterialCatalog,
) -> Vec<RetainedDrawItem> {
    xmodel
        .draws
        .iter()
        .filter(|draw| draw.object_id != XMODEL_OBJECT_ID_VIEWMODEL && !draw.is_scope)
        .filter_map(|draw| xmodel_shadow_draw(xmodel, draw, catalog))
        .collect()
}

fn xmodel_shadow_draw(
    xmodel: &XModelDrawPlan,
    draw: &super::tess::xmodel::XModelSurfaceDraw,
    catalog: &super::RuntimeMaterialCatalog,
) -> Option<RetainedDrawItem> {
    let material = xmodel.materials.get(draw.material as usize)?;
    let ordinal = material.material_sorted_index?;
    let baked = catalog
        .material_for_sorted_ordinal(ordinal)?
        .baked_draw_surf?;
    let key =
        pack_xmodel_rigid_skinned_draw_surf(GfxDrawSurf::from_packed(baked), draw.object_id).packed;
    Some(with_catalog(
        key,
        ordinal,
        RetainedDrawKind::XModel {
            surface: draw.surface,
            material: draw.material,
            object_id: draw.object_id,
            world_from_local: draw.world_from_local,
            lighting_handle: draw.lighting_handle,
            packed_lighting: draw.packed_lighting,
            is_scope: draw.is_scope,
            scene_entnum: draw.scene_entnum,
        },
        super::SurfaceSamplerInputs {
            reflection_probe: Some(super::SurfaceReflectionProbeId(draw.reflection_probe_index)),
            ..Default::default()
        },
        catalog,
    ))
}
