use crate::geometry::{append_mesh, f32x2, f32x3, f32x4, install_retained_packed};
use bevy::mesh::Indices;
use bevy::prelude::*;
use render_frame::SmodelVertex;
use render_material::{RuntimeMaterialCatalog, SortedMaterialOrdinal};
use render_scene::{SmodelPassMaterial, WorldModelLightingAtlas};

use crate::anim::fpv_pose::PosedModelSurface;
use crate::draw::{
    BODY_PACKED_UNAVAILABLE, DynEntAssetDraw, DynEntDrawPlan, FPV_PACKED_EMPTY_PLAN,
    FPV_PACKED_UNAVAILABLE, FpvDrawPlan, ItemDrawPlan, MissileDrawPlan, RemoteBodyDrawPlan,
    RemoteBodySurfaceDraw, ScriptModelAssetDraw, ScriptModelDrawPlan, stamp_plan_geometry,
    topology_fingerprint,
};

pub const XMODEL_PACKED_UNAVAILABLE: &str =
    "xmodel merge GfxPackedVertex missing or count-mismatched; decoded float is not packed VB";
pub const XMODEL_PACKED_EMPTY_PLAN: &str = "xmodel plan has no vertices";

pub fn body_lit_pass_material(
    lighting: &WorldModelLightingAtlas,
    catalog: &RuntimeMaterialCatalog,
    material_key: &asset_core::MaterialKey,
) -> Option<SmodelPassMaterial> {
    let material_sorted_index = catalog
        .material_for_key(material_key)
        .and_then(|material| {
            catalog
                .parts()
                .sorted_materials
                .ordinal_for_asset_id(usize::from(material.asset_id.0))
        })
        .map(SortedMaterialOrdinal::get);
    let material_sorted_index = material_sorted_index?;
    lit_xmodel_pass_material(lighting, material_sorted_index)
}

pub fn bound_lit_xmodel_pass_material(
    lighting: &WorldModelLightingAtlas,
    catalog: &RuntimeMaterialCatalog,
    material_index: assets::MaterialIndex,
) -> Option<SmodelPassMaterial> {
    let ordinal = catalog.ordinal_for_asset_id(material_index)?.get();
    lit_xmodel_pass_material(lighting, ordinal)
}

pub fn authored_lit_xmodel_pass_material(
    lighting: &WorldModelLightingAtlas,
    catalog: &RuntimeMaterialCatalog,
    image_handles: &[Option<Handle<Image>>],
    authored: assets::MaterialIndex,
) -> Option<SmodelPassMaterial> {
    use lighting_iw4::{
        MODEL_LIGHTING_INV_ATLAS_WIDTH, MODEL_LIGHTING_VOLUME_W, model_lighting_inv_image_height,
        model_lighting_lookup_scale,
    };
    let world_material = catalog.derived(authored)?;
    let ordinal = catalog.ordinal_for_asset_id(authored)?;
    let maps = render_scene::runtime_maps(Some(authored), catalog, image_handles);
    let inv_h = model_lighting_inv_image_height(lighting.dims.image_height)?;
    let scale = model_lighting_lookup_scale(inv_h);
    Some(SmodelPassMaterial {
        model_lighting_required: true,
        color: maps.color,
        specular: maps.specular,
        probe: None,
        atlas: Some(lighting.image.clone()),
        alpha_mode: maps.alpha_mode,
        draw_mode: maps.draw_mode,
        cull_mode: maps.cull_mode,
        env_map_parms: maps.env_map_parms,
        lighting_lookup_scale: [scale.u, scale.v, scale.w, scale.q],
        atlas_lookup: [
            MODEL_LIGHTING_INV_ATLAS_WIDTH as f32,
            inv_h,
            MODEL_LIGHTING_VOLUME_W,
            0.0,
        ],
        sort_key: world_material.sort_key,
        material_sorted_index: Some(ordinal.get()),
    })
}

fn lit_xmodel_pass_material(
    lighting: &WorldModelLightingAtlas,
    material_sorted_index: u32,
) -> Option<SmodelPassMaterial> {
    use lighting_iw4::{
        MODEL_LIGHTING_INV_ATLAS_WIDTH, MODEL_LIGHTING_VOLUME_W, model_lighting_inv_image_height,
        model_lighting_lookup_scale,
    };
    let inv_h = model_lighting_inv_image_height(lighting.dims.image_height)?;
    let scale = model_lighting_lookup_scale(inv_h);
    Some(SmodelPassMaterial {
        model_lighting_required: true,
        color: None,
        specular: None,
        probe: None,
        atlas: Some(lighting.image.clone()),
        alpha_mode: AlphaMode::Opaque,
        draw_mode: None,
        cull_mode: None,
        env_map_parms: [0.0; 4],
        lighting_lookup_scale: [scale.u, scale.v, scale.w, scale.q],
        atlas_lookup: [
            MODEL_LIGHTING_INV_ATLAS_WIDTH as f32,
            inv_h,
            MODEL_LIGHTING_VOLUME_W,
            0.0,
        ],
        sort_key: 0,
        material_sorted_index: Some(material_sorted_index),
    })
}

pub struct BodyPackedSession {
    packed_ok: bool,
    packed: Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
}

impl BodyPackedSession {
    pub fn reserve_packed(&mut self, n: usize) {
        self.packed.reserve(n);
    }
}

pub fn take_body_packed_session(plan: &mut RemoteBodyDrawPlan) -> BodyPackedSession {
    let vertices_empty = plan.decoded_n == 0 && plan.vertices.is_empty();
    let packed_ok = vertices_empty
        || matches!(
            plan.packed_vertices,
            asset_world::PackedVertexPayload::Iw4(_)
        );
    let packed = match std::mem::replace(
        &mut plan.packed_vertices,
        asset_world::PackedVertexPayload::Unavailable {
            source_layout: BODY_PACKED_UNAVAILABLE,
        },
    ) {
        asset_world::PackedVertexPayload::Iw4(rows) => rows,
        asset_world::PackedVertexPayload::Unavailable { .. } => Vec::new(),
    };
    BodyPackedSession { packed_ok, packed }
}

pub fn install_body_packed_session(plan: &mut RemoteBodyDrawPlan, session: BodyPackedSession) {
    plan.packed_vertices = install_retained_packed(
        session.packed_ok,
        session.packed,
        plan.decoded_n,
        XMODEL_PACKED_EMPTY_PLAN,
        BODY_PACKED_UNAVAILABLE,
    );
}

pub fn append_remote_body_cpu_blob(
    plan: &mut RemoteBodyDrawPlan,
    session: &mut BodyPackedSession,
    packed_rows: &[[u8; asset_iw4::size::GFX_PACKED_VERTEX]],
    indices: &[u32],
    decoded_n: usize,
) -> Option<(u32, u32)> {
    if decoded_n == 0 || indices.is_empty() {
        return None;
    }
    let vert_base = plan.decoded_n as u32;
    let index_base = plan.indices.len() as u32;
    plan.decoded_n = plan.decoded_n.saturating_add(decoded_n);
    plan.indices
        .extend(indices.iter().map(|&index| vert_base + index));
    if session.packed_ok && packed_rows.len() == decoded_n {
        session.packed.extend_from_slice(packed_rows);
    } else {
        session.packed_ok = false;
        session.packed.clear();
    }
    Some((vert_base, index_base))
}

pub fn push_remote_body_cpu_draw(
    plan: &mut RemoteBodyDrawPlan,
    index_start: u32,
    index_count: u32,
    world_from_local: Mat4,
    lighting_handle: u32,
    scene_light_index: u8,
    reflection_probe_index: u8,
    material: u32,
    scene_entnum: Option<u32>,
) {
    if lighting_handle == 0 || index_count == 0 {
        return;
    }
    let range_idx = plan.surface_ranges.len() as u32;
    plan.surface_ranges.push((index_start, index_count));
    plan.draws.push(RemoteBodySurfaceDraw {
        surface: range_idx,
        material,
        world_from_local,
        lighting_handle,
        scene_light_index,
        reflection_probe_index,
        scene_entnum,
    });
}

pub fn finish_remote_body_draw_plan(plan: &mut RemoteBodyDrawPlan) {
    let topology = topology_fingerprint(&plan.indices, &plan.surface_ranges, plan.decoded_n);
    if plan.revisions.topology != topology {
        plan.revisions.topology = topology;
    }
}

pub fn append_script_model_asset(
    plan: &mut ScriptModelDrawPlan,
    key: asset_world::MapXModelAssetKey,
    dobj_state: xmodel_runtime::DObjSemanticState,
    camera_lods: Vec<Option<u8>>,
    surfaces: &[PosedModelSurface],
    materials: &[Option<SmodelPassMaterial>],
) -> usize {
    let mut asset_surfaces = Vec::new();

    let vertices_empty = plan.vertices.is_empty();
    let mut packed_ok = vertices_empty
        || matches!(
            plan.packed_vertices,
            asset_world::PackedVertexPayload::Iw4(_)
        );
    let mut packed = match std::mem::replace(
        &mut plan.packed_vertices,
        asset_world::PackedVertexPayload::Unavailable {
            source_layout: XMODEL_PACKED_UNAVAILABLE,
        },
    ) {
        asset_world::PackedVertexPayload::Iw4(rows) => rows,
        asset_world::PackedVertexPayload::Unavailable { .. } => Vec::new(),
    };
    for (surface, material) in surfaces.iter().zip(materials) {
        let Some(material) = material else { continue };
        let before = plan.vertices.len();
        let Some((start, count)) =
            append_mesh(&surface.mesh, &mut plan.vertices, &mut plan.indices)
        else {
            continue;
        };
        let decoded = plan.vertices.len() - before;
        if packed_ok && surface.packed_vertices.len() == decoded {
            packed.extend_from_slice(&surface.packed_vertices);
        } else {
            packed_ok = false;
            packed.clear();
        }
        let surface_index = plan.surface_ranges.len() as u32;
        plan.surface_ranges.push((start, count));
        let material_index = plan.materials.len() as u32;
        plan.materials.push(material.clone());
        asset_surfaces.push((surface_index, material_index));
    }
    plan.assets.push(ScriptModelAssetDraw {
        key,
        dobj_state,
        camera_lods,
        surfaces: asset_surfaces,
    });
    plan.packed_vertices = install_retained_packed(
        packed_ok,
        packed,
        plan.vertices.len(),
        XMODEL_PACKED_EMPTY_PLAN,
        XMODEL_PACKED_UNAVAILABLE,
    );

    plan.revisions.bump_surfaces();
    let topology = topology_fingerprint(&plan.indices, &plan.surface_ranges, plan.vertices.len());
    let rev = plan.revision;
    plan.revision = stamp_plan_geometry(&mut plan.revisions, rev, topology);
    plan.assets.len() - 1
}

pub fn overwrite_script_model_asset(
    plan: &mut ScriptModelDrawPlan,
    asset_index: usize,
    dobj_state: xmodel_runtime::DObjSemanticState,
    surfaces: &[PosedModelSurface],
    materials: &[Option<SmodelPassMaterial>],
) -> bool {
    let Some(asset) = plan.assets.get(asset_index) else {
        return false;
    };
    let incoming: Vec<(&PosedModelSurface, &SmodelPassMaterial)> = surfaces
        .iter()
        .zip(materials)
        .filter_map(|(surface, material)| material.as_ref().map(|material| (surface, material)))
        .collect();
    if incoming.len() != asset.surfaces.len() {
        return false;
    }
    let slots: Vec<(u32, u32)> = asset.surfaces.clone();
    let mut ranges = Vec::with_capacity(incoming.len());
    for ((surface, _), &(surf_i, mat_i)) in incoming.iter().zip(&slots) {
        let Some(&(index_start, count)) = plan.surface_ranges.get(surf_i as usize) else {
            return false;
        };
        let start = index_start as usize;
        let end = start.saturating_add(count as usize);
        let Some(slice) = plan.indices.get(start..end) else {
            return false;
        };
        let (Some(&vmin), Some(&vmax)) = (slice.iter().min(), slice.iter().max()) else {
            return false;
        };
        let n = (vmax.saturating_sub(vmin)).saturating_add(1) as usize;
        let Some(positions) = surface
            .mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(f32x3)
        else {
            return false;
        };
        if positions.len() != n || surface.packed_vertices.len() != n {
            return false;
        }
        let index_len = match surface.mesh.indices() {
            Some(Indices::U32(ix)) => ix.len(),
            Some(Indices::U16(ix)) => ix.len(),
            None => return false,
        };
        if index_len != count as usize {
            return false;
        }
        ranges.push((vmin as usize, n, mat_i as usize));
    }
    for ((base, n, mat_i), (surface, material)) in ranges.into_iter().zip(incoming.iter()) {
        let Some(positions) = surface
            .mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(f32x3)
        else {
            return false;
        };
        let normals = surface
            .mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .and_then(f32x3);
        let uvs = surface.mesh.attribute(Mesh::ATTRIBUTE_UV_0).and_then(f32x2);
        let colors = surface
            .mesh
            .attribute(Mesh::ATTRIBUTE_COLOR)
            .and_then(f32x4);
        let Some(dst) = plan.vertices.get_mut(base..base + n) else {
            return false;
        };
        for (i, vertex) in dst.iter_mut().enumerate() {
            *vertex = SmodelVertex {
                position: positions[i],
                normal: normals
                    .and_then(|a| a.get(i).copied())
                    .unwrap_or([0.0, 0.0, 1.0]),
                color: colors.and_then(|a| a.get(i).copied()).unwrap_or([1.0; 4]),
                uv0: uvs.and_then(|a| a.get(i).copied()).unwrap_or([0.0; 2]),
            };
        }
        if let asset_world::PackedVertexPayload::Iw4(rows) = &mut plan.packed_vertices {
            let Some(dst) = rows.get_mut(base..base + n) else {
                return false;
            };
            dst.copy_from_slice(&surface.packed_vertices);
        }
        if let Some(slot) = plan.materials.get_mut(mat_i) {
            *slot = (*material).clone();
        }
    }
    if let Some(asset) = plan.assets.get_mut(asset_index) {
        asset.dobj_state = dobj_state;
    }
    plan.revisions.bump_surfaces();
    plan.revision = plan.revision.wrapping_add(1);
    true
}

pub fn retain_script_model_assets(plan: &mut ScriptModelDrawPlan, keep: &[bool]) {
    debug_assert_eq!(keep.len(), plan.assets.len());
    if keep.iter().all(|k| *k) {
        return;
    }
    let packed_src = match &plan.packed_vertices {
        asset_world::PackedVertexPayload::Iw4(rows) => Some(rows.as_slice()),
        asset_world::PackedVertexPayload::Unavailable { .. } => None,
    };
    let mut next = ScriptModelDrawPlan::default();

    next.vertices.reserve(plan.vertices.len());
    next.indices.reserve(plan.indices.len());
    next.surface_ranges.reserve(plan.surface_ranges.len());
    next.materials.reserve(plan.materials.len());
    next.assets.reserve(plan.assets.len());
    let mut packed_dst = Vec::with_capacity(packed_src.map_or(0, <[_]>::len));
    let mut packed_ok = packed_src.is_some();
    for (asset_index, asset) in plan.assets.iter().enumerate() {
        if !keep.get(asset_index).copied().unwrap_or(false) {
            continue;
        }
        let mut new_surfaces = Vec::new();
        for &(surf_i, mat_i) in &asset.surfaces {
            let Some(&(index_start, count)) = plan.surface_ranges.get(surf_i as usize) else {
                continue;
            };
            let start = index_start as usize;
            let end = start.saturating_add(count as usize);
            let Some(slice) = plan.indices.get(start..end) else {
                continue;
            };
            let (vmin, vmax) = match (slice.iter().min(), slice.iter().max()) {
                (Some(min), Some(max)) => (*min, *max),
                _ => continue,
            };
            let vbase = next.vertices.len() as u32;
            for vi in vmin..=vmax {
                let Some(vertex) = plan.vertices.get(vi as usize) else {
                    packed_ok = false;
                    packed_dst.clear();
                    break;
                };
                next.vertices.push(*vertex);
                if packed_ok {
                    if let Some(src) = packed_src {
                        if let Some(row) = src.get(vi as usize) {
                            packed_dst.push(*row);
                        } else {
                            packed_ok = false;
                            packed_dst.clear();
                        }
                    }
                }
            }
            let index_start_new = next.indices.len() as u32;
            next.indices
                .extend(slice.iter().map(|index| vbase + (*index - vmin)));
            let surface_index = next.surface_ranges.len() as u32;
            next.surface_ranges.push((index_start_new, count));
            let Some(material) = plan.materials.get(mat_i as usize) else {
                continue;
            };
            let material_index = next.materials.len() as u32;
            next.materials.push(material.clone());
            new_surfaces.push((surface_index, material_index));
        }
        next.assets.push(ScriptModelAssetDraw {
            key: asset.key.clone(),
            dobj_state: asset.dobj_state.clone(),
            camera_lods: asset.camera_lods.clone(),
            surfaces: new_surfaces,
        });
    }
    next.packed_vertices = install_retained_packed(
        packed_ok,
        packed_dst,
        next.vertices.len(),
        XMODEL_PACKED_EMPTY_PLAN,
        XMODEL_PACKED_UNAVAILABLE,
    );

    // Carry the plan's revisions across the retain: a retain reshuffles surface
    // indices, it does not make this a different plan, and a revision that
    // restarts at zero is one a consumer can mistake for the one it last saw.
    next.revisions = plan.revisions;
    next.revisions.bump_surfaces();
    next.generation = plan.generation.wrapping_add(1);
    next.revision = plan.revision;
    next.revisions.bump_admission();
    let topology = topology_fingerprint(&next.indices, &next.surface_ranges, next.vertices.len());
    let rev = next.revision;
    next.revision = stamp_plan_geometry(&mut next.revisions, rev, topology);
    *plan = next;
}

pub fn append_missile_surfaces(
    plan: &mut MissileDrawPlan,
    surfaces: &[PosedModelSurface],
    materials: &[Option<SmodelPassMaterial>],
) -> Vec<(u32, u32)> {
    let mut asset_surfaces = Vec::new();
    let mut packed = match &plan.packed_vertices {
        asset_world::PackedVertexPayload::Iw4(rows) => rows.clone(),
        asset_world::PackedVertexPayload::Unavailable { .. } if plan.vertices.is_empty() => {
            Vec::new()
        }
        asset_world::PackedVertexPayload::Unavailable { .. } => Vec::new(),
    };
    let mut packed_ok = plan.vertices.is_empty()
        || matches!(
            plan.packed_vertices,
            asset_world::PackedVertexPayload::Iw4(_)
        );
    for (surface, material) in surfaces.iter().zip(materials) {
        let Some(material) = material else { continue };
        let before = plan.vertices.len();
        let Some((start, count)) =
            append_mesh(&surface.mesh, &mut plan.vertices, &mut plan.indices)
        else {
            continue;
        };
        let decoded = plan.vertices.len() - before;
        if packed_ok && surface.packed_vertices.len() == decoded {
            packed.extend_from_slice(&surface.packed_vertices);
        } else {
            packed_ok = false;
            packed.clear();
        }
        let surface_index = plan.surface_ranges.len() as u32;
        plan.surface_ranges.push((start, count));
        let material_index = plan.materials.len() as u32;
        plan.materials.push(material.clone());
        asset_surfaces.push((surface_index, material_index));
    }
    plan.packed_vertices = install_retained_packed(
        packed_ok,
        packed,
        plan.vertices.len(),
        XMODEL_PACKED_EMPTY_PLAN,
        XMODEL_PACKED_UNAVAILABLE,
    );

    plan.revisions.bump_surfaces();
    asset_surfaces
}

pub fn append_item_surfaces(
    plan: &mut ItemDrawPlan,
    surfaces: &[PosedModelSurface],
    materials: &[Option<SmodelPassMaterial>],
) -> Vec<(u32, u32)> {
    let mut asset_surfaces = Vec::new();
    let mut packed = match &plan.packed_vertices {
        asset_world::PackedVertexPayload::Iw4(rows) => rows.clone(),
        asset_world::PackedVertexPayload::Unavailable { .. } if plan.vertices.is_empty() => {
            Vec::new()
        }
        asset_world::PackedVertexPayload::Unavailable { .. } => Vec::new(),
    };
    let mut packed_ok = plan.vertices.is_empty()
        || matches!(
            plan.packed_vertices,
            asset_world::PackedVertexPayload::Iw4(_)
        );
    for (surface, material) in surfaces.iter().zip(materials) {
        let Some(material) = material else { continue };
        let before = plan.vertices.len();
        let Some((start, count)) =
            append_mesh(&surface.mesh, &mut plan.vertices, &mut plan.indices)
        else {
            continue;
        };
        let decoded = plan.vertices.len() - before;
        if packed_ok && surface.packed_vertices.len() == decoded {
            packed.extend_from_slice(&surface.packed_vertices);
        } else {
            packed_ok = false;
            packed.clear();
        }
        let surface_index = plan.surface_ranges.len() as u32;
        plan.surface_ranges.push((start, count));
        let material_index = plan.materials.len() as u32;
        plan.materials.push(material.clone());
        asset_surfaces.push((surface_index, material_index));
    }
    plan.packed_vertices = install_retained_packed(
        packed_ok,
        packed,
        plan.vertices.len(),
        XMODEL_PACKED_EMPTY_PLAN,
        XMODEL_PACKED_UNAVAILABLE,
    );

    plan.revisions.bump_surfaces();
    asset_surfaces
}

pub fn append_dynent_surfaces(
    plan: &mut DynEntDrawPlan,
    surfaces: &[PosedModelSurface],
    materials: &[Option<SmodelPassMaterial>],
) -> Vec<(u32, u32)> {
    let mut asset_surfaces = Vec::new();
    let vertices_empty = plan.vertices.is_empty();
    let mut packed_ok = vertices_empty
        || matches!(
            plan.packed_vertices,
            asset_world::PackedVertexPayload::Iw4(_)
        );
    let mut packed = match std::mem::replace(
        &mut plan.packed_vertices,
        asset_world::PackedVertexPayload::Unavailable {
            source_layout: XMODEL_PACKED_UNAVAILABLE,
        },
    ) {
        asset_world::PackedVertexPayload::Iw4(rows) => rows,
        asset_world::PackedVertexPayload::Unavailable { .. } => Vec::new(),
    };
    for (surface, material) in surfaces.iter().zip(materials) {
        let Some(material) = material else { continue };
        let before = plan.vertices.len();
        let Some((start, count)) =
            append_mesh(&surface.mesh, &mut plan.vertices, &mut plan.indices)
        else {
            continue;
        };
        let decoded = plan.vertices.len() - before;
        if packed_ok && surface.packed_vertices.len() == decoded {
            packed.extend_from_slice(&surface.packed_vertices);
        } else {
            packed_ok = false;
            packed.clear();
        }
        let surface_index = plan.surface_ranges.len() as u32;
        plan.surface_ranges.push((start, count));
        let material_index = plan.materials.len() as u32;
        plan.materials.push(material.clone());
        asset_surfaces.push((surface_index, material_index));
    }
    plan.packed_vertices = install_retained_packed(
        packed_ok,
        packed,
        plan.vertices.len(),
        XMODEL_PACKED_EMPTY_PLAN,
        XMODEL_PACKED_UNAVAILABLE,
    );

    plan.revisions.bump_surfaces();
    asset_surfaces
}

pub fn append_dynent_asset(
    plan: &mut DynEntDrawPlan,
    key: asset_world::MapXModelAssetKey,
    camera_lod: Option<u8>,
    surfaces: &[PosedModelSurface],
    materials: &[Option<SmodelPassMaterial>],
) -> Vec<(u32, u32)> {
    let asset_surfaces = append_dynent_surfaces(plan, surfaces, materials);
    plan.assets.push(DynEntAssetDraw {
        key,
        camera_lod,
        surfaces: asset_surfaces.clone(),
    });
    asset_surfaces
}

/// Publish the rows a prepared rig settled: its indices, its surface ranges,
/// its materials and its draws, plus a packed vertex buffer sized to the layout
/// it laid out. This runs when the composition changes. A pose writes into the
/// buffer below and touches nothing else here.
pub fn install_prepared_fpv_plan(
    plan: &mut FpvDrawPlan,
    geometry: &crate::anim::fpv_rig::PreparedFpvGeometry,
    camo: Option<&std::collections::HashMap<usize, SmodelPassMaterial>>,
    lighting_handle: u32,
) {
    geometry.write_indices(&mut plan.indices);
    plan.surface_ranges.clear();
    plan.surface_ranges
        .extend_from_slice(&geometry.surface_ranges);
    plan.materials.clear();
    plan.materials.extend(
        geometry
            .materials
            .iter()
            .zip(&geometry.material_authored)
            .map(|(material, authored)| {
                camo.and_then(|swaps| swaps.get(authored))
                    .unwrap_or(material)
                    .clone()
            }),
    );
    plan.draws.clear();
    plan.draws.extend_from_slice(&geometry.draws);
    plan.decoded_n = geometry.dest_n;
    plan.lighting_handle = lighting_handle;
    plan.geometry_ok = false;
    plan.packed_vertices = install_retained_packed(
        geometry.packed_ok,
        vec![[0u8; asset_iw4::size::GFX_PACKED_VERTEX]; geometry.dest_n],
        geometry.dest_n,
        FPV_PACKED_EMPTY_PLAN,
        FPV_PACKED_UNAVAILABLE,
    );
    plan.hands_plan_n = Some(geometry.hands_plan_n);
    plan.gun_plan_n = Some(geometry.gun_plan_n);
    plan.scope_plan_n = Some(geometry.scope_plan_n);
    plan.scope_house_plan_n = Some(geometry.scope_house_plan_n);
    plan.scope_lens_plan_n = Some(geometry.scope_lens_plan_n);
    plan.plan_draw_n = Some(geometry.plan_draw_n);
    plan.plan_skip_n = Some(geometry.plan_skip_n);
    plan.revisions.bump_surfaces();
    let topology = topology_fingerprint(&plan.indices, &plan.surface_ranges, plan.decoded_n);
    let revision = plan.revision;
    plan.revision = stamp_plan_geometry(&mut plan.revisions, revision, topology);
}

/// Nothing to draw. A plan that was already empty publishes no new revision:
/// a hidden viewmodel is not a reason for the merge downstream to re-copy a
/// vertex bank every frame.
pub fn clear_fpv_draw_plan(plan: &mut FpvDrawPlan, lighting_handle: u32) {
    let was_empty = plan.draws.is_empty() && plan.decoded_n == 0;
    plan.lighting_handle = lighting_handle;
    plan.geometry_ok = false;
    plan.settle_visible();
    // The rows are gone, so the rig that published them has to publish them
    // again before this plan draws anything: a viewmodel hidden without a
    // respawn behind it comes back to the same composition, not to an empty
    // plan.
    plan.rig_generation = 0;
    if was_empty {
        return;
    }
    plan.indices.clear();
    plan.surface_ranges.clear();
    plan.materials.clear();
    plan.draws.clear();
    plan.decoded_n = 0;
    plan.packed_vertices = asset_world::PackedVertexPayload::Unavailable {
        source_layout: FPV_PACKED_EMPTY_PLAN,
    };
    plan.hands_plan_n = None;
    plan.gun_plan_n = None;
    plan.scope_plan_n = Some(0);
    plan.scope_house_plan_n = Some(0);
    plan.scope_lens_plan_n = Some(0);
    plan.plan_draw_n = Some(0);
    plan.plan_skip_n = Some(0);
    plan.revisions.bump_surfaces();
    let topology = topology_fingerprint(&plan.indices, &plan.surface_ranges, 0);
    let revision = plan.revision;
    plan.revision = stamp_plan_geometry(&mut plan.revisions, revision, topology);
}
