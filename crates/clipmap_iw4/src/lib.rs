#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod area_entities;
mod capsule_query;
mod cmodel;
mod layouts;
mod mesh;
mod temp_box;
mod world_trace;
mod xmodel_trace;

use alloc::vec::Vec;
use trace_iw4::{BrushRef, Trace, trace_box};

pub use area_entities::{
    AREA_SECTOR_COUNT, AreaBounds, AreaEntityLink, AreaEntityWorld, AreaEntityWorldError,
    AreaSector,
};
pub use cmodel::{ClipCmodel, CmodelTransform, clip_handle_to_model, transformed_capsule_trace};
pub use layouts::{
    Cbrush, Cbrushside, Cleaf, CleafBrushNode, ClipMaterial, Cmodel, Cnode, CollisionAabbTree,
    CollisionPartition,
};
pub use mesh::{
    ClipAabbNode, ClipBorder, ClipMeshRef, ClipMeshTables, ClipPartition, MeshWalkCensus,
    VERTS_PER_SEGMENT, aabb_forest_roots, flatten_tri_material_index, flatten_tri_surface_flags,
    trace_through_mesh, trace_through_mesh_into, trace_through_mesh_with_census,
};
pub use temp_box::transformed_temp_capsule_trace;
pub use world_trace::{
    ClipWorldWinner, WorldTraceCensus, combine_brush_mesh, finish_mesh_forest_fallback,
    trace_brush_and_mesh, trace_brush_and_mesh_with_census, trace_brush_and_mesh_with_winner,
    trace_leaf_brushes_into, trace_leaf_mesh_into,
};
pub use xmodel_trace::{
    ClipStaticModel, RigidXform, StaticModelHit, StaticModelWalkStats, XModelAnimBone, XModelColl,
    XModelCollSurf, XModelCollTri, point_trace_static_models, point_trace_static_models_stats,
    point_trace_static_models_stats_keyed, trace_box_misses, trace_static_model, xmodel_trace_line,
    xmodel_trace_line_animated,
};

pub trait BrushView {
    fn planes(&self) -> &[[f32; 4]];
    fn contents(&self) -> u32;

    fn plane_surface_flags(&self) -> &[u32];

    fn glass_encoded(&self) -> u16 {
        0
    }
}

impl BrushView for BrushRef<'_> {
    fn planes(&self) -> &[[f32; 4]] {
        self.planes
    }
    fn contents(&self) -> u32 {
        self.contents
    }
    fn plane_surface_flags(&self) -> &[u32] {
        self.plane_surface_flags
    }
    fn glass_encoded(&self) -> u16 {
        self.glass_encoded
    }
}

pub fn brush_ref<B: BrushView>(brush: &B) -> BrushRef<'_> {
    BrushRef {
        planes: brush.planes(),
        contents: brush.contents(),
        plane_surface_flags: brush.plane_surface_flags(),
        glass_encoded: brush.glass_encoded(),
    }
}

#[must_use]
pub fn glass_is_solid(damage: u16, destroy_threshold: u16) -> bool {
    if damage == 0xffff {
        return false;
    }
    damage < destroy_threshold
}

pub(crate) fn brush_glass_allowed<B: BrushView>(
    brush: &B,
    glass_is_solid: &dyn Fn(u16) -> bool,
) -> bool {
    let encoded = brush.glass_encoded();
    if encoded == 0 {
        return true;
    }
    glass_is_solid(encoded - 1)
}

#[derive(Clone, Copy, Debug)]
pub struct TraceExtents {
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub mask: u32,
}

impl TraceExtents {
    pub fn new(start: [f32; 3], end: [f32; 3], mins: [f32; 3], maxs: [f32; 3], mask: u32) -> Self {
        Self {
            start,
            end,
            mins,
            maxs,
            mask,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ClipNode {
    pub plane: [f32; 4],
    pub children: [i32; 2],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClipLeaf {
    pub first_brush: u32,
    pub num_brushes: u16,

    pub first_coll_aabb_index: u16,

    pub coll_aabb_count: u16,
}

#[derive(Clone, Debug, Default)]
pub struct LeafHits {
    pub brush_ids: Vec<u16>,
    pub aabb_roots: Vec<u16>,
    pub leaves_visited: u32,
}

impl LeafHits {
    fn visit_leaf(&mut self, leaf: &ClipLeaf, leafbrushes: &[u16]) {
        self.leaves_visited = self.leaves_visited.saturating_add(1);
        let first = leaf.first_brush as usize;
        let last = first + leaf.num_brushes as usize;
        if let Some(slice) = leafbrushes.get(first..last) {
            self.brush_ids.extend_from_slice(slice);
        }
        if leaf.coll_aabb_count == 0 {
            return;
        }
        let start = leaf.first_coll_aabb_index;
        for k in 0..leaf.coll_aabb_count {
            self.aabb_roots.push(start.saturating_add(k));
        }
    }

    fn finish(&mut self) {
        self.brush_ids.sort_unstable();
        self.brush_ids.dedup();
        self.aabb_roots.sort_unstable();
        self.aabb_roots.dedup();
    }
}

#[must_use]
pub fn leaves_have_coll_aabb(leaves: &[ClipLeaf]) -> bool {
    leaves.iter().any(|leaf| leaf.coll_aabb_count != 0)
}

#[must_use]
pub fn mesh_aabb_roots_for_trace<'a>(
    hits: Option<&'a LeafHits>,
    forest: &'a [u16],
    leaves: &[ClipLeaf],
) -> (&'a [u16], bool) {
    if leaves_have_coll_aabb(leaves) {
        match hits {
            Some(h) => (&h.aabb_roots, false),
            None => (forest, true),
        }
    } else {
        (forest, true)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ClipMapRef<'a, B: BrushView> {
    pub nodes: &'a [ClipNode],
    pub leaves: &'a [ClipLeaf],
    pub leafbrushes: &'a [u16],
    pub brushes: &'a [B],
}

pub fn trace_linear_with_glass<B: BrushView>(
    map: &ClipMapRef<'_, B>,
    ext: &TraceExtents,
    glass_is_solid: &dyn Fn(u16) -> bool,
) -> Trace {
    trace_box(
        map.brushes.iter().filter_map(|b| {
            if !brush_glass_allowed(b, glass_is_solid) {
                return None;
            }
            Some(brush_ref(b))
        }),
        ext.start,
        ext.end,
        ext.mins,
        ext.maxs,
        ext.mask,
    )
}

pub fn collect_leaf_hits<B: BrushView>(map: &ClipMapRef<'_, B>, ext: &TraceExtents) -> LeafHits {
    let mut hits = LeafHits::default();
    if map.nodes.is_empty() || map.leaves.is_empty() {
        return hits;
    }
    let always_one = || 1.0_f32;
    let _ = walk_clip_tree(map, ext, &always_one, &mut |leaf| {
        hits.visit_leaf(leaf, map.leafbrushes);
    });
    hits.finish();
    hits
}

pub fn position_leaf_hits<B: BrushView>(
    map: &ClipMapRef<'_, B>,
    ext: &TraceExtents,
) -> Option<LeafHits> {
    if ext.start != ext.end || map.nodes.is_empty() || map.leaves.is_empty() {
        return None;
    }
    let walk = TreeWalk::new(ext);
    let mut hits = LeafHits::default();
    let mut seen = alloc::vec![false; map.nodes.len()];
    let mut stack = alloc::vec![0_i32];
    while let Some(id) = stack.pop() {
        if id < 0 {
            let leaf = map.leaves.get((!id) as usize)?;
            let first = leaf.first_brush as usize;
            let last = first.checked_add(usize::from(leaf.num_brushes))?;
            map.leafbrushes.get(first..last)?;
            hits.visit_leaf(leaf, map.leafbrushes);
            continue;
        }
        let index = id as usize;
        let node = map.nodes.get(index)?;
        if seen[index]
            || !node.plane.iter().all(|x| x.is_finite())
            || node.plane[..3].iter().all(|&x| x == 0.0)
        {
            return None;
        }
        seen[index] = true;
        let offset = walk.plane_offset([node.plane[0], node.plane[1], node.plane[2]]);
        let distance = plane_dist(node.plane, walk.start);
        if !offset.is_finite() || !distance.is_finite() {
            return None;
        }
        if distance >= offset {
            stack.push(node.children[0]);
        } else if distance <= -offset {
            stack.push(node.children[1]);
        } else {
            stack.extend(node.children);
        }
    }
    hits.finish();
    Some(hits)
}

pub fn trace_selected_brushes_with_glass<B: BrushView>(
    map: &ClipMapRef<'_, B>,
    brush_ids: &[u16],
    ext: &TraceExtents,
    glass_is_solid: &dyn Fn(u16) -> bool,
) -> Trace {
    let mut selected: Vec<BrushRef<'_>> = Vec::with_capacity(brush_ids.len());
    for &id in brush_ids {
        if let Some(b) = map.brushes.get(id as usize) {
            if !brush_glass_allowed(b, glass_is_solid) {
                continue;
            }
            selected.push(brush_ref(b));
        }
    }
    trace_box(
        selected.iter().copied(),
        ext.start,
        ext.end,
        ext.mins,
        ext.maxs,
        ext.mask,
    )
}

pub fn trace_through_tree_with_glass<B: BrushView>(
    map: &ClipMapRef<'_, B>,
    ext: &TraceExtents,
    glass_is_solid: &dyn Fn(u16) -> bool,
) -> Trace {
    if map.nodes.is_empty() || map.leaves.is_empty() {
        return trace_linear_with_glass(map, ext, glass_is_solid);
    }
    let mut best = Trace {
        fraction: 1.0,
        endpos: ext.end,
        ..Trace::default()
    };
    let frac = core::cell::Cell::new(1.0_f32);
    let _ = walk_clip_tree(map, ext, &|| frac.get(), &mut |leaf| {
        let _ = trace_leaf_brushes_into(map, leaf, ext, glass_is_solid, &mut best);
        frac.set(best.fraction);
    });
    best
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TreeWalkCensus {
    pub early_out_n: u32,
}

pub fn walk_clip_tree<B: BrushView>(
    map: &ClipMapRef<'_, B>,
    ext: &TraceExtents,
    fraction: &dyn Fn() -> f32,
    on_leaf: &mut dyn FnMut(&ClipLeaf),
) -> TreeWalkCensus {
    let mut census = TreeWalkCensus::default();
    if map.nodes.is_empty() || map.leaves.is_empty() {
        return census;
    }
    let walk = TreeWalk::new(ext);
    walk_node(
        map,
        0,
        walk.start,
        walk.end,
        0.0,
        1.0,
        &walk,
        fraction,
        on_leaf,
        &mut census.early_out_n,
    );
    census
}

struct TreeWalk {
    start: [f32; 3],
    end: [f32; 3],

    size: [f32; 3],

    bounding_radius: f32,
}

const TREE_PLANE_EPSILON: f32 = 0.125;

impl TreeWalk {
    fn new(ext: &TraceExtents) -> Self {
        let mut center = [0.0_f32; 3];
        let mut size = [0.0_f32; 3];
        let mut start = [0.0_f32; 3];
        let mut end = [0.0_f32; 3];
        for i in 0..3 {
            center[i] = (ext.mins[i] + ext.maxs[i]) * 0.5;
            size[i] = ext.maxs[i] - center[i];
            start[i] = ext.start[i] + center[i];
            end[i] = ext.end[i] + center[i];
        }
        let bounding_radius =
            libm::sqrtf(size[0] * size[0] + size[1] * size[1] + size[2] * size[2]);
        Self {
            start,
            end,
            size,
            bounding_radius,
        }
    }

    fn plane_offset(&self, normal: [f32; 3]) -> f32 {
        for (axis, component) in normal.iter().enumerate() {
            if libm::fabsf(libm::fabsf(*component) - 1.0) <= 1e-4 {
                return self.size[axis] + TREE_PLANE_EPSILON;
            }
        }
        self.bounding_radius + TREE_PLANE_EPSILON
    }
}

const TREE_SPLIT_EPS: f32 = 0.000_000_476_837_16;

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        (b[0] - a[0]) * t + a[0],
        (b[1] - a[1]) * t + a[1],
        (b[2] - a[2]) * t + a[2],
    ]
}

fn walk_node<B: BrushView>(
    map: &ClipMapRef<'_, B>,
    mut num: i32,
    mut p1: [f32; 3],
    p2: [f32; 3],
    mut p1t: f32,
    p2t: f32,
    walk: &TreeWalk,
    fraction: &dyn Fn() -> f32,
    on_leaf: &mut dyn FnMut(&ClipLeaf),
    early_out_n: &mut u32,
) {
    loop {
        if num < 0 {
            if p1t >= fraction() {
                *early_out_n = early_out_n.saturating_add(1);
                return;
            }
            let leaf_i = (!num) as usize;
            if let Some(leaf) = map.leaves.get(leaf_i) {
                on_leaf(leaf);
            }
            return;
        }
        let Some(node) = map.nodes.get(num as usize) else {
            return;
        };
        let normal = [node.plane[0], node.plane[1], node.plane[2]];
        let offset = walk.plane_offset(normal);
        let d1 = plane_dist(node.plane, p1);
        let d2 = plane_dist(node.plane, p2);
        let min_d = if d2 - d1 < 0.0 { d2 } else { d1 };
        if min_d >= offset {
            num = node.children[0];
            continue;
        }
        let max_d = if d1 - d2 < 0.0 { d2 } else { d1 };
        if max_d <= -offset {
            num = node.children[1];
            continue;
        }

        if p1t >= p2t {
            return;
        }

        if p1t >= fraction() {
            *early_out_n = early_out_n.saturating_add(1);
            return;
        }
        let diff = d2 - d1;
        let abs_diff = libm::fabsf(diff);
        let (side, mut frac, mut frac2) = if abs_diff <= TREE_SPLIT_EPS {
            (0usize, 1.0_f32, 0.0_f32)
        } else {
            let v7 = if diff < 0.0 { d1 } else { -d1 };
            let inv = 1.0 / abs_diff;
            (
                usize::from(diff >= 0.0),
                (v7 + offset) * inv,
                (v7 - offset) * inv,
            )
        };
        if frac < 0.0 {
            frac = 0.0;
        }
        if 1.0 - frac < 0.0 {
            frac = 1.0;
        }
        if frac2 < 0.0 {
            frac2 = 0.0;
        }
        let mid = lerp3(p1, p2, frac);
        let midt = (p2t - p1t) * frac + p1t;
        walk_node(
            map,
            node.children[side],
            p1,
            mid,
            p1t,
            midt,
            walk,
            fraction,
            on_leaf,
            early_out_n,
        );
        p1 = lerp3(p1, p2, frac2);
        p1t = (p2t - p1t) * frac2 + p1t;
        num = node.children[1 - side];
    }
}

fn plane_dist(plane: [f32; 4], p: [f32; 3]) -> f32 {
    plane[0] * p[0] + plane[1] * p[1] + plane[2] * p[2] - plane[3]
}
