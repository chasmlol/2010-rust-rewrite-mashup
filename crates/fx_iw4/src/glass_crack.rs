//! Transient crack graph for `FxGlass` fracture.
//!
//! The graph is a planar half-edge structure over one piece's outline. Pane edges
//! carry no twin; every crack segment is created as a forward/reverse twin pair.
//! Inserting a crack either splices an open chain into a loop, splits one loop in
//! two, or merges two loops into one. Faces of the finished graph are the emitted
//! shards, so the workspace is sized for the largest graph a pane may reach: 255
//! points, 512 directed edges, 32 loops and 32 pending branches.

use crate::glass::FX_GLASS_VERT_SCALE;
use crate::glass_shatter::{FX_GLASS_SHATTER_TWO_PI, glass_interior_branch_count};
use crate::random::{FxRandomChannel, sample_at};

pub const FX_GLASS_CRACK_PT_MAX: usize = 255;
pub const FX_GLASS_CRACK_EDGE_MAX: usize = 512;
pub const FX_GLASS_CRACK_LOOP_MAX: usize = 32;
pub const FX_GLASS_CRACK_BRANCH_MAX: usize = 32;

/// Null edge handle.
pub const FX_GLASS_EDGE_NONE: u16 = u16::MAX;

/// Edge class. Shard emission turns this class into a support bit, so only a
/// supported pane edge may carry it.
pub const FX_GLASS_EDGE_SUPPORTED: u8 = 0;
/// Crack segments created by the walk.
pub const FX_GLASS_EDGE_CRACK: u8 = 1;
/// Original pane edge that was not supported.
pub const FX_GLASS_EDGE_BORDER: u8 = 2;

/// Probe lookahead added to a segment before scanning and removed when nothing was hit,
/// as a fraction of the piece's bounding radius. Every length the walk works in is a
/// fraction of that radius, so a small pane breaks like a large one.
const CRACK_LOOKAHEAD_FRAC: f32 = 2.0 / 7.0;
/// Extra reach granted when a retry re-aims at an endpoint.
const CRACK_ENDPOINT_EPS: f32 = 0.01;
/// Distance under which the impact snaps onto the pane border instead of seeding an
/// interior star, again as a fraction of the bounding radius.
const CRACK_SNAP_FRAC: f32 = 1.0 / 14.0;
/// Fraction of the remaining deflection budget a single step may consume.
const CRACK_DEFLECT_MIX: f32 = 0.5;
/// Chance a step spawns a secondary branch at depth zero, halved at each further
/// depth. The halving is what keeps a star from filling the workspace.
const CRACK_BRANCH_CHANCE: f32 = 0.25;
/// Secondary branches may themselves branch only this deep.
const CRACK_BRANCH_DEPTH_MAX: u8 = 2;
/// Per-step length jitter, `len *= CRACK_STEP_MIN + rand * CRACK_STEP_SPAN`.
const CRACK_STEP_MIN: f32 = 0.75;
const CRACK_STEP_SPAN: f32 = 0.5;
/// Seed segment length, `(CRACK_SEED_MIN + rand * CRACK_SEED_SPAN) * lookahead`.
const CRACK_SEED_MIN: f32 = 0.5;
const CRACK_SEED_SPAN: f32 = 0.5;
/// Minimum wedge at a border vertex that still earns one bisecting crack.
const CRACK_BORDER_MIN_ANGLE: f32 = 1.046_150_4;
/// Quantisation grid: packed units per inch.
const CRACK_PACK_SCALE: f32 = 1.0 / FX_GLASS_VERT_SCALE;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxGlassCrackEdge {
    pub i0: u8,
    pub i1: u8,
    pub loop_index: u8,
    pub kind: u8,
    pub len: f32,
    pub dir: [f32; 2],
    pub twin: u16,
    pub next: u16,
}

impl Default for FxGlassCrackEdge {
    fn default() -> Self {
        Self {
            i0: 0,
            i1: 0,
            loop_index: 0,
            kind: FX_GLASS_EDGE_BORDER,
            len: 0.0,
            dir: [0.0, 0.0],
            twin: FX_GLASS_EDGE_NONE,
            next: FX_GLASS_EDGE_NONE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxGlassCrackLoop {
    pub first_edge: u16,
    pub mins: [f32; 2],
    pub maxs: [f32; 2],
}

impl Default for FxGlassCrackLoop {
    fn default() -> Self {
        Self {
            first_edge: FX_GLASS_EDGE_NONE,
            mins: [0.0, 0.0],
            maxs: [0.0, 0.0],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FxGlassCrackBranch {
    pub after_edge: u16,
    pub start_index: u8,
    pub dir: [f32; 2],
    pub len: f32,
    pub base_dir: [f32; 2],
    pub deflect_limit: f32,
    pub prior_crack_length: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FxGlassClipSegment {
    pub start_index: u8,
    pub len: f32,
    pub dir: [f32; 2],
    pub deflect_limit: f32,
    pub base_dir: [f32; 2],
    pub deflect_dir: [f32; 2],
    pub deflect_len: f32,
    pub is_bad: bool,
    pub was_deflected: bool,
    pub hit_at_vertex: bool,
    pub hit_edge: u16,
    pub hit_edge_prev: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FxGlassCrackWalk {
    pub cutoff_crack_length: f32,
    pub loop_index: u8,
    pub total_crack_length: f32,
    pub clip: FxGlassClipSegment,
    pub front_head: u16,
    pub back_head: u16,
    pub front_tail: u16,
    pub back_tail: u16,
    pub clipped_edge: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FxGlassCrackRand {
    key: u64,
    cursor: u64,
}

impl FxGlassCrackRand {
    pub fn from_seed(seed: u64) -> Self {
        Self {
            key: seed,
            cursor: 0,
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> f32 {
        let value = sample_at(self.key, FxRandomChannel::GlassCrack, self.cursor);
        self.cursor = self.cursor.wrapping_add(1);
        value
    }

    fn lerp(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.next()
    }
}

/// Scratch space for one piece's graph. Large enough that callers should box it.
#[derive(Clone, Debug)]
pub struct FxGlassCrackWork {
    pub packed_pts: [[i16; 2]; FX_GLASS_CRACK_PT_MAX],
    pub pts: [[f32; 2]; FX_GLASS_CRACK_PT_MAX],
    pub pt_count: u16,
    pub edges: [FxGlassCrackEdge; FX_GLASS_CRACK_EDGE_MAX],
    pub edge_count: u16,
    pub loops: [FxGlassCrackLoop; FX_GLASS_CRACK_LOOP_MAX],
    pub loop_count: u8,
    pub break_org_loop: u16,
    pub break_org_index: u8,
    pub crack_length_min: f32,
    pub crack_length_max: f32,
    pub original_radius: f32,
    pub impact_pos: [f32; 2],
    pub branch_stack: [FxGlassCrackBranch; FX_GLASS_CRACK_BRANCH_MAX],
    pub branch_stack_level: u8,
    /// Bookkeeping beside the branch records: how many branches deep each pending
    /// descriptor is.
    pub branch_depth: [u8; FX_GLASS_CRACK_BRANCH_MAX],
    pub rand: FxGlassCrackRand,
}

impl Default for FxGlassCrackWork {
    fn default() -> Self {
        Self {
            packed_pts: [[0; 2]; FX_GLASS_CRACK_PT_MAX],
            pts: [[0.0; 2]; FX_GLASS_CRACK_PT_MAX],
            pt_count: 0,
            edges: [FxGlassCrackEdge::default(); FX_GLASS_CRACK_EDGE_MAX],
            edge_count: 0,
            loops: [FxGlassCrackLoop::default(); FX_GLASS_CRACK_LOOP_MAX],
            loop_count: 0,
            break_org_loop: FX_GLASS_EDGE_NONE,
            break_org_index: 0,
            crack_length_min: 0.0,
            crack_length_max: 0.0,
            original_radius: 0.0,
            impact_pos: [0.0, 0.0],
            branch_stack: [FxGlassCrackBranch::default(); FX_GLASS_CRACK_BRANCH_MAX],
            branch_stack_level: 0,
            branch_depth: [0; FX_GLASS_CRACK_BRANCH_MAX],
            rand: FxGlassCrackRand::default(),
        }
    }
}

fn dot2(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross2(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[1] - a[1] * b[0]
}

/// Counter-clockwise angle from `u` to `v` in `[0, 2pi)`.
fn ccw_angle(u: [f32; 2], v: [f32; 2]) -> f32 {
    let a = libm::atan2f(cross2(u, v), dot2(u, v));
    if a < 0.0 {
        a + FX_GLASS_SHATTER_TWO_PI
    } else {
        a
    }
}

fn norm2(v: [f32; 2]) -> Option<([f32; 2], f32)> {
    let len_sq = dot2(v, v);
    if len_sq <= 1e-12 {
        return None;
    }
    let len = libm::sqrtf(len_sq);
    Some(([v[0] / len, v[1] / len], len))
}

/// Rotates `v` by a random angle whose cosine lies in `[max_cos, 1]`, with the sign
/// carried by `sign`. The draw is biased by `r*r`, so small deflections dominate.
fn random_rotate(rand: &mut FxGlassCrackRand, v: [f32; 2], max_cos: f32, sign: f32) -> [f32; 2] {
    let r = rand.next();
    let c = 1.0 + (max_cos - 1.0) * r * r;
    let c = c.clamp(-1.0, 1.0);
    let s = libm::sqrtf((1.0 - c * c).max(0.0)) * sign;
    [c * v[0] - s * v[1], c * v[1] + s * v[0]]
}

impl FxGlassCrackWork {
    fn edge(&self, e: u16) -> FxGlassCrackEdge {
        self.edges[usize::from(e)]
    }

    /// Probe reach for one clip, scaled off the piece the graph was seeded from.
    fn lookahead(&self) -> f32 {
        (self.original_radius * CRACK_LOOKAHEAD_FRAC).max(1e-3)
    }

    fn snap_dist(&self) -> f32 {
        (self.original_radius * CRACK_SNAP_FRAC).max(1e-3)
    }

    fn pt(&self, i: u8) -> [f32; 2] {
        self.pts[usize::from(i)]
    }

    /// Recomputes `dir`/`len` after an endpoint moved.
    fn refresh_edge(&mut self, e: u16) {
        let row = self.edge(e);
        let a = self.pt(row.i0);
        let b = self.pt(row.i1);
        let d = [b[0] - a[0], b[1] - a[1]];
        let len = libm::sqrtf(dot2(d, d));
        let slot = &mut self.edges[usize::from(e)];
        slot.dir = d;
        slot.len = len;
    }

    /// One directed edge off the end of the workspace.
    fn alloc_edge(&mut self, i0: u8, i1: u8, loop_index: u8, kind: u8) -> Option<u16> {
        if usize::from(self.edge_count) >= FX_GLASS_CRACK_EDGE_MAX {
            return None;
        }
        let e = self.edge_count;
        self.edge_count += 1;
        self.edges[usize::from(e)] = FxGlassCrackEdge {
            i0,
            i1,
            loop_index,
            kind,
            len: 0.0,
            dir: [0.0, 0.0],
            twin: FX_GLASS_EDGE_NONE,
            next: FX_GLASS_EDGE_NONE,
        };
        self.refresh_edge(e);
        Some(e)
    }

    /// The reverse edge, cross-linked, same loop and class.
    fn alloc_twin(&mut self, e: u16) -> Option<u16> {
        let row = self.edge(e);
        let t = self.alloc_edge(row.i1, row.i0, row.loop_index, row.kind)?;
        let slot = &mut self.edges[usize::from(t)];
        slot.len = row.len;
        slot.dir = [-row.dir[0], -row.dir[1]];
        slot.twin = e;
        self.edges[usize::from(e)].twin = t;
        Some(t)
    }

    fn add_point(&mut self, p: [f32; 2]) -> Option<u8> {
        if usize::from(self.pt_count) >= FX_GLASS_CRACK_PT_MAX {
            return None;
        }
        let i = self.pt_count as u8;
        // The point is quantised onto the packed i16 grid and read back, so every
        // generated vertex is exactly representable in geometry data.
        let px = pack_coord(p[0]);
        let py = pack_coord(p[1]);
        self.packed_pts[usize::from(i)] = [px, py];
        self.pts[usize::from(i)] = [
            f32::from(px) * FX_GLASS_VERT_SCALE,
            f32::from(py) * FX_GLASS_VERT_SCALE,
        ];
        self.pt_count += 1;
        Some(i)
    }

    /// Loop bounds from its circular edge chain.
    fn refresh_loop_bounds(&mut self, li: u8) {
        let first = self.loops[usize::from(li)].first_edge;
        if first == FX_GLASS_EDGE_NONE {
            return;
        }
        let start = self.pt(self.edge(first).i0);
        let mut mins = start;
        let mut maxs = start;
        let mut e = first;
        loop {
            let p = self.pt(self.edge(e).i0);
            mins[0] = mins[0].min(p[0]);
            mins[1] = mins[1].min(p[1]);
            maxs[0] = maxs[0].max(p[0]);
            maxs[1] = maxs[1].max(p[1]);
            e = self.edge(e).next;
            if e == first || e == FX_GLASS_EDGE_NONE {
                break;
            }
        }
        self.loops[usize::from(li)].mins = mins;
        self.loops[usize::from(li)].maxs = maxs;
    }

    fn relabel_loop(&mut self, first: u16, li: u8) {
        let mut e = first;
        loop {
            self.edges[usize::from(e)].loop_index = li;
            e = self.edge(e).next;
            if e == first || e == FX_GLASS_EDGE_NONE {
                break;
            }
        }
    }

    fn loop_prev(&self, first: u16, e: u16) -> u16 {
        let mut prev = first;
        while self.edge(prev).next != e {
            prev = self.edge(prev).next;
            if prev == first {
                break;
            }
        }
        prev
    }

    /// Seeds the workspace from one piece outline in packed parent-local units.
    pub fn init_from_loop(&mut self, verts: &[[i16; 2]], support_mask: u32) -> bool {
        *self = Self::default();
        let n = verts.len();
        if !(3..=FX_GLASS_CRACK_LOOP_MAX * 4).contains(&n) || n > FX_GLASS_CRACK_PT_MAX {
            return false;
        }
        // Authored contours can have either winding. Crack faces require the
        // interior on the left; keep vertex zero and reverse clockwise contours.
        let clockwise = crate::glass_geo::glass_contour_area_x2(verts) < 0;
        for i in 0..n {
            let source = if clockwise { (n - i) % n } else { i };
            let v = verts[source];
            self.packed_pts[i] = v;
            self.pts[i] = [
                f32::from(v[0]) * FX_GLASS_VERT_SCALE,
                f32::from(v[1]) * FX_GLASS_VERT_SCALE,
            ];
            self.pt_count += 1;
        }
        for i in 0..n {
            // Support belongs to the original edge, not its reordered vertex.
            let source_edge = if clockwise { n - 1 - i } else { i };
            let supported = source_edge < 32 && support_mask & (0x8000_0000u32 >> source_edge) != 0;
            let kind = if supported {
                FX_GLASS_EDGE_SUPPORTED
            } else {
                FX_GLASS_EDGE_BORDER
            };
            if self
                .alloc_edge(i as u8, ((i + 1) % n) as u8, 0, kind)
                .is_none()
            {
                return false;
            }
        }
        for i in 0..n {
            self.edges[i].next = ((i + 1) % n) as u16;
        }
        self.loop_count = 1;
        self.loops[0].first_edge = 0;
        self.refresh_loop_bounds(0);
        true
    }

    /// Returns the edge the first crack is spliced after when the impact snapped onto
    /// existing geometry, plus the outward seed direction for the interior case.
    fn pick_seed_dir(&mut self) -> (u16, [f32; 2]) {
        let p = self.impact_pos;
        let mut best_d2 = f32::INFINITY;
        let mut best_edge = FX_GLASS_EDGE_NONE;
        let mut best_t = 0.0f32;
        let mut away = [0.0f32, 0.0];
        for li in 0..self.loop_count {
            let first = self.loops[usize::from(li)].first_edge;
            if first == FX_GLASS_EDGE_NONE {
                continue;
            }
            let mut e = first;
            loop {
                let row = self.edge(e);
                let a = self.pt(row.i0);
                let ab = row.dir;
                let ab2 = dot2(ab, ab);
                let t = if ab2 <= 1e-8 {
                    0.0
                } else {
                    (dot2([p[0] - a[0], p[1] - a[1]], ab) / ab2).clamp(0.0, 1.0)
                };
                let q = [a[0] + ab[0] * t, a[1] + ab[1] * t];
                let d = [p[0] - q[0], p[1] - q[1]];
                let d2 = dot2(d, d);
                if d2 < best_d2 {
                    best_d2 = d2;
                    best_edge = e;
                    best_t = t * row.len;
                    away = d;
                }
                e = row.next;
                if e == first || e == FX_GLASS_EDGE_NONE {
                    break;
                }
            }
        }
        if best_edge == FX_GLASS_EDGE_NONE {
            return (FX_GLASS_EDGE_NONE, [1.0, 0.0]);
        }
        let snap = self.snap_dist();
        if best_d2 > snap * snap {
            self.break_org_index = match self.add_point(p) {
                Some(i) => i,
                None => return (FX_GLASS_EDGE_NONE, [1.0, 0.0]),
            };
            let dir = norm2(away).map(|(d, _)| d).unwrap_or([1.0, 0.0]);
            return (FX_GLASS_EDGE_NONE, dir);
        }
        (self.snap_to_edge(best_edge, best_t), [0.0, 0.0])
    }

    /// Puts the impact on the border, splitting the hit edge when the projection
    /// falls strictly inside it, and returns the edge it now follows.
    fn snap_to_edge(&mut self, e: u16, along: f32) -> u16 {
        let row = self.edge(e);
        let first = self.loops[usize::from(row.loop_index)].first_edge;
        let prev = self.loop_prev(first, e);
        if along <= CRACK_ENDPOINT_EPS {
            self.break_org_index = row.i0;
            return prev;
        }
        if along >= row.len - CRACK_ENDPOINT_EPS {
            self.break_org_index = row.i1;
            return e;
        }
        let a = self.pt(row.i0);
        let t = along / row.len.max(1e-6);
        let mid = [a[0] + row.dir[0] * t, a[1] + row.dir[1] * t];
        let Some(new_i) = self.add_point(mid) else {
            self.break_org_index = row.i0;
            return prev;
        };
        self.break_org_index = new_i;
        let Some(head) = self.alloc_edge(row.i0, new_i, row.loop_index, row.kind) else {
            self.break_org_index = row.i0;
            return prev;
        };
        self.edges[usize::from(head)].next = e;
        self.edges[usize::from(prev)].next = head;
        self.edges[usize::from(e)].i0 = new_i;
        self.refresh_edge(e);
        if row.twin != FX_GLASS_EDGE_NONE {
            self.split_twin(e, head);
        }
        head
    }

    /// Keeps the reverse side of a split edge consistent.
    fn split_twin(&mut self, e: u16, head: u16) {
        let row = self.edge(e);
        let twin = row.twin;
        if twin == FX_GLASS_EDGE_NONE {
            return;
        }
        // The existing twin becomes the reverse of the trailing half.
        {
            let t = &mut self.edges[usize::from(twin)];
            t.i0 = row.i1;
            t.i1 = row.i0;
        }
        self.edges[usize::from(twin)].i1 = row.i0;
        self.edges[usize::from(twin)].i0 = row.i1;
        self.refresh_edge(twin);
        self.edges[usize::from(twin)].twin = e;
        self.edges[usize::from(e)].twin = twin;
        // A fresh reverse edge covers the leading half and is spliced before the twin.
        let head_row = self.edge(head);
        let Some(head_twin) = self.alloc_edge(
            head_row.i1,
            head_row.i0,
            self.edge(twin).loop_index,
            head_row.kind,
        ) else {
            return;
        };
        self.edges[usize::from(head_twin)].twin = head;
        self.edges[usize::from(head)].twin = head_twin;
        self.edges[usize::from(head_twin)].next = self.edge(twin).next;
        self.edges[usize::from(twin)].next = head_twin;
        let li = self.edge(twin).loop_index;
        if self.loops[usize::from(li)].first_edge == FX_GLASS_EDGE_NONE {
            self.loops[usize::from(li)].first_edge = twin;
        }
    }

    /// Clips the pending segment against one candidate edge.
    fn trace_against_edge(&self, e: u16, prev: u16, clip: &mut FxGlassClipSegment) {
        let row = self.edge(e);
        if row.i0 == clip.start_index || row.i1 == clip.start_index {
            return;
        }
        let d = clip.dir;
        // Every geometric edge appears twice, once per face. A crack only ever meets
        // the half whose face it is travelling inside, which is the one it approaches
        // from the left; picking the other half would splice the chain into the face
        // on the far side and turn a hole inside out.
        if cross2(row.dir, d) >= 0.0 {
            return;
        }
        let p = self.pt(clip.start_index);
        let a = self.pt(row.i0);
        let b = self.pt(row.i1);
        let ab = [b[0] - a[0], b[1] - a[1]];
        let det = cross2(d, ab);
        if libm::fabsf(det) <= 1e-7 {
            return;
        }
        let ao = [a[0] - p[0], a[1] - p[1]];
        let t_ray = cross2(ao, ab) / det;
        let t_seg = cross2(ao, d) / det;
        if t_ray <= CRACK_ENDPOINT_EPS || t_ray >= clip.len {
            return;
        }
        if !(0.0..=1.0).contains(&t_seg) {
            return;
        }
        let edge_len = row.len.max(1e-6);
        let near_start = t_seg * edge_len <= CRACK_ENDPOINT_EPS;
        let near_end = (1.0 - t_seg) * edge_len <= CRACK_ENDPOINT_EPS;
        if near_start {
            // Already aiming exactly at this vertex: terminate on it.
            clip.hit_at_vertex = true;
            clip.hit_edge = e;
            clip.hit_edge_prev = prev;
            clip.len = t_ray;
            clip.was_deflected = false;
            return;
        }
        if near_end {
            // The crack would clip the far vertex of this edge. Re-aim at the vertex
            // and rescan rather than cutting a slither off the corner.
            self.propose_deflection(p, b, clip);
            return;
        }
        clip.hit_at_vertex = false;
        clip.hit_edge = e;
        clip.hit_edge_prev = prev;
        clip.len = t_ray;
        clip.was_deflected = false;
    }

    /// Accepts an endpoint re-aim only while it stays inside the branch's deflection
    /// budget and shortens the reach.
    fn propose_deflection(&self, from: [f32; 2], to: [f32; 2], clip: &mut FxGlassClipSegment) {
        let Some((dir, len)) = norm2([to[0] - from[0], to[1] - from[1]]) else {
            return;
        };
        if len >= clip.len {
            return;
        }
        if dot2(dir, clip.base_dir) < clip.deflect_limit {
            return;
        }
        if clip.was_deflected && clip.deflect_len <= len {
            return;
        }
        clip.was_deflected = true;
        clip.deflect_dir = dir;
        clip.deflect_len = len;
    }

    fn scan_loop(&self, li: u8, clip: &mut FxGlassClipSegment) {
        let first = self.loops[usize::from(li)].first_edge;
        if first == FX_GLASS_EDGE_NONE {
            return;
        }
        let mut prev = first;
        let mut e = self.edge(first).next;
        loop {
            self.trace_against_edge(e, prev, clip);
            if clip.hit_at_vertex {
                // A vertex hit is exact; nothing closer can exist on this heading.
                return;
            }
            prev = e;
            e = self.edge(e).next;
            if e == FX_GLASS_EDGE_NONE {
                return;
            }
            if prev == first {
                return;
            }
        }
    }

    /// Scans every loop the segment could reach for the first thing it meets,
    /// re-aiming and rescanning while a vertex deflects it.
    fn clip_segment(&self, clip: &mut FxGlassClipSegment) {
        let p = self.pt(clip.start_index);
        let lookahead = self.lookahead();
        clip.len += lookahead;
        clip.is_bad = false;
        let mut retries = 0u32;
        loop {
            clip.hit_edge = FX_GLASS_EDGE_NONE;
            clip.hit_edge_prev = FX_GLASS_EDGE_NONE;
            clip.hit_at_vertex = false;
            clip.was_deflected = false;
            let reach = clip.len;
            self.scan_loop(0, clip);
            if !clip.hit_at_vertex && self.loop_count > 1 {
                let end = [p[0] + clip.dir[0] * reach, p[1] + clip.dir[1] * reach];
                let mins = [p[0].min(end[0]), p[1].min(end[1])];
                let maxs = [p[0].max(end[0]), p[1].max(end[1])];
                for li in 1..self.loop_count {
                    let row = self.loops[usize::from(li)];
                    if row.first_edge == FX_GLASS_EDGE_NONE {
                        continue;
                    }
                    if row.maxs[0] < mins[0]
                        || row.mins[0] > maxs[0]
                        || row.maxs[1] < mins[1]
                        || row.mins[1] > maxs[1]
                    {
                        continue;
                    }
                    self.scan_loop(li, clip);
                    if clip.hit_at_vertex {
                        break;
                    }
                }
            }
            if !clip.was_deflected {
                break;
            }
            clip.dir = clip.deflect_dir;
            clip.len = clip.deflect_len + CRACK_ENDPOINT_EPS;
            retries += 1;
            if retries > u32::from(self.pt_count) {
                clip.is_bad = true;
                return;
            }
        }
        if clip.hit_edge == FX_GLASS_EDGE_NONE {
            clip.len -= lookahead;
        }
    }

    /// Walks one crack step by step. Returns true when it reached existing geometry,
    /// false when it ran out of budget and stays an open chain.
    fn do_crack_walk(&mut self, walk: &mut FxGlassCrackWalk, depth: u8) -> bool {
        walk.front_head = FX_GLASS_EDGE_NONE;
        walk.back_head = FX_GLASS_EDGE_NONE;
        walk.front_tail = FX_GLASS_EDGE_NONE;
        walk.back_tail = FX_GLASS_EDGE_NONE;
        walk.clipped_edge = FX_GLASS_EDGE_NONE;
        let mut clip = walk.clip;
        self.clip_segment(&mut clip);
        loop {
            if clip.is_bad {
                walk.clip = clip;
                return false;
            }
            if clip.hit_edge != FX_GLASS_EDGE_NONE
                && self.edge(clip.hit_edge).loop_index == walk.loop_index
                && usize::from(self.loop_count) >= FX_GLASS_CRACK_LOOP_MAX
            {
                walk.clip = clip;
                return false;
            }
            let tip = if clip.hit_at_vertex {
                self.edge(clip.hit_edge).i0
            } else {
                let p = self.pt(clip.start_index);
                let next = [p[0] + clip.dir[0] * clip.len, p[1] + clip.dir[1] * clip.len];
                match self.add_point(next) {
                    Some(i) => i,
                    None => {
                        walk.clip = clip;
                        return false;
                    }
                }
            };
            if tip == clip.start_index {
                walk.clip = clip;
                return false;
            }
            {
                let p = self.pt(tip);
                let row = &mut self.loops[usize::from(walk.loop_index)];
                row.mins[0] = row.mins[0].min(p[0]);
                row.mins[1] = row.mins[1].min(p[1]);
                row.maxs[0] = row.maxs[0].max(p[0]);
                row.maxs[1] = row.maxs[1].max(p[1]);
            }
            let Some(fwd) =
                self.alloc_edge(clip.start_index, tip, walk.loop_index, FX_GLASS_EDGE_CRACK)
            else {
                walk.clip = clip;
                return false;
            };
            let Some(rev) = self.alloc_twin(fwd) else {
                walk.clip = clip;
                return false;
            };
            if walk.front_head == FX_GLASS_EDGE_NONE {
                walk.front_head = fwd;
                walk.back_tail = rev;
            } else {
                self.edges[usize::from(walk.front_tail)].next = fwd;
                self.edges[usize::from(rev)].next = walk.back_head;
            }
            walk.front_tail = fwd;
            walk.back_head = rev;

            if clip.hit_edge != FX_GLASS_EDGE_NONE {
                walk.clipped_edge = if clip.hit_at_vertex {
                    clip.hit_edge_prev
                } else {
                    self.split_hit_edge(clip.hit_edge, clip.hit_edge_prev, tip)
                };
                walk.clip = clip;
                return walk.clipped_edge != FX_GLASS_EDGE_NONE;
            }

            walk.total_crack_length += clip.len;
            if walk.total_crack_length >= walk.cutoff_crack_length {
                walk.clip = clip;
                return false;
            }
            let branch_now = depth < CRACK_BRANCH_DEPTH_MAX
                && usize::from(self.branch_stack_level) < FX_GLASS_CRACK_BRANCH_MAX
                && self.rand.next() * f32::from(1u16 << depth) < CRACK_BRANCH_CHANCE;
            if branch_now {
                let branch = FxGlassCrackBranch {
                    after_edge: fwd,
                    start_index: tip,
                    dir: random_rotate(&mut self.rand, clip.base_dir, clip.deflect_limit, -1.0),
                    len: clip.len
                        * self
                            .rand
                            .lerp(CRACK_STEP_MIN, CRACK_STEP_MIN + CRACK_STEP_SPAN),
                    base_dir: clip.base_dir,
                    deflect_limit: clip.deflect_limit,
                    prior_crack_length: walk.total_crack_length,
                };
                self.branch_stack[usize::from(self.branch_stack_level)] = branch;
                self.branch_depth[usize::from(self.branch_stack_level)] = depth + 1;
                self.branch_stack_level += 1;
            }
            clip.start_index = tip;
            clip.len *= self
                .rand
                .lerp(CRACK_STEP_MIN, CRACK_STEP_MIN + CRACK_STEP_SPAN);
            clip.base_dir = clip.dir;
            clip.dir = random_rotate(
                &mut self.rand,
                clip.dir,
                CRACK_DEFLECT_MIX + CRACK_DEFLECT_MIX * clip.deflect_limit,
                1.0,
            );
            self.clip_segment(&mut clip);
        }
    }

    /// Splits the edge the crack landed on, keeping twins symmetric, and returns the
    /// leading half (`clippedEdge`).
    fn split_hit_edge(&mut self, hit: u16, prev: u16, tip: u8) -> u16 {
        let row = self.edge(hit);
        let Some(head) = self.alloc_edge(row.i0, tip, row.loop_index, row.kind) else {
            return FX_GLASS_EDGE_NONE;
        };
        self.edges[usize::from(head)].next = hit;
        if prev != FX_GLASS_EDGE_NONE {
            self.edges[usize::from(prev)].next = head;
        }
        self.edges[usize::from(hit)].i0 = tip;
        self.refresh_edge(hit);
        if self.loops[usize::from(row.loop_index)].first_edge == hit {
            self.loops[usize::from(row.loop_index)].first_edge = head;
        }
        if row.twin != FX_GLASS_EDGE_NONE {
            self.split_twin(hit, head);
        }
        head
    }

    /// The wedge at `v` whose face the heading `d` points into.
    ///
    /// Returns the edge that ends at `v` inside that wedge, which is the edge a new
    /// crack is spliced after, together with the loop that owns it.
    fn find_wedge(&self, v: u8, d: [f32; 2]) -> Option<(u16, u8)> {
        let mut best: Option<(u16, u8, f32)> = None;
        for li in 0..self.loop_count {
            let first = self.loops[usize::from(li)].first_edge;
            if first == FX_GLASS_EDGE_NONE {
                continue;
            }
            let mut e = first;
            loop {
                let row = self.edge(e);
                if row.i1 == v {
                    let next = self.edge(row.next);
                    let back = norm2([-row.dir[0], -row.dir[1]]);
                    let fwd = norm2(next.dir);
                    if let (Some((back, _)), Some((fwd, _))) = (back, fwd) {
                        let span = ccw_angle(fwd, back);
                        let at = ccw_angle(fwd, d);
                        if at <= span && best.is_none_or(|(_, _, s)| span < s) {
                            best = Some((e, li, span));
                        }
                    }
                }
                e = row.next;
                if e == first || e == FX_GLASS_EDGE_NONE {
                    break;
                }
            }
        }
        best.map(|(e, li, _)| (e, li))
    }

    /// Re-finds the splice edge at `v` inside `li` once a walk has finished.
    ///
    /// Splitting an edge rewrites its twin's end vertex, so the edge chosen before the
    /// walk may no longer end at `v`; the leading half that replaced it does.
    fn respot_after(&self, li: u8, v: u8, d: [f32; 2]) -> u16 {
        let first = self.loops[usize::from(li)].first_edge;
        if first == FX_GLASS_EDGE_NONE {
            return FX_GLASS_EDGE_NONE;
        }
        let mut any = FX_GLASS_EDGE_NONE;
        let mut best: Option<(u16, f32)> = None;
        let mut e = first;
        loop {
            let row = self.edge(e);
            if row.i1 == v {
                if any == FX_GLASS_EDGE_NONE {
                    any = e;
                }
                let next = self.edge(row.next);
                if let (Some((back, _)), Some((fwd, _))) =
                    (norm2([-row.dir[0], -row.dir[1]]), norm2(next.dir))
                {
                    let span = ccw_angle(fwd, back);
                    if ccw_angle(fwd, d) <= span && best.is_none_or(|(_, s)| span < s) {
                        best = Some((e, span));
                    }
                }
            }
            e = row.next;
            if e == first || e == FX_GLASS_EDGE_NONE {
                break;
            }
        }
        best.map_or(any, |(e, _)| e)
    }

    /// Walks one pending branch and re-links the topology according to the outcome.
    fn process_crack(&mut self, branch: FxGlassCrackBranch, depth: u8) {
        let placement = self.find_wedge(branch.start_index, branch.dir).or_else(|| {
            let e = branch.after_edge;
            (e != FX_GLASS_EDGE_NONE).then(|| (e, self.edge(e).loop_index))
        });
        let (after_edge, loop_index, fresh) = match placement {
            Some((e, li)) => (e, li, false),
            None => {
                // The impact vertex is not attached to anything yet: the first arm of
                // an interior star opens a loop component of its own, which later
                // becomes a hole boundary of whichever shard encloses it.
                if usize::from(self.loop_count) >= FX_GLASS_CRACK_LOOP_MAX {
                    return;
                }
                let li = self.loop_count;
                let p = self.pt(branch.start_index);
                self.loops[usize::from(li)] = FxGlassCrackLoop {
                    first_edge: FX_GLASS_EDGE_NONE,
                    mins: p,
                    maxs: p,
                };
                self.loop_count += 1;
                (FX_GLASS_EDGE_NONE, li, true)
            }
        };
        let cutoff = self.rand.lerp(self.crack_length_min, self.crack_length_max)
            - branch.prior_crack_length;
        if cutoff <= 0.0 {
            if fresh {
                self.rollback_fresh_loop(loop_index);
            }
            return;
        }
        let mut walk = FxGlassCrackWalk {
            cutoff_crack_length: cutoff,
            loop_index,
            total_crack_length: 0.0,
            clip: FxGlassClipSegment {
                start_index: branch.start_index,
                len: branch.len,
                dir: branch.dir,
                deflect_limit: branch.deflect_limit,
                base_dir: branch.base_dir,
                ..FxGlassClipSegment::default()
            },
            ..FxGlassCrackWalk::default()
        };
        let connected = self.do_crack_walk(&mut walk, depth);
        // Edge splitting during the walk can move the vertex the splice point was
        // chosen for, so the insertion edge is re-established against the final graph.
        let after_edge = if fresh {
            after_edge
        } else {
            self.respot_after(loop_index, branch.start_index, branch.dir)
        };
        if after_edge == FX_GLASS_EDGE_NONE && !fresh {
            return;
        }
        if walk.front_head == FX_GLASS_EDGE_NONE {
            if fresh {
                self.rollback_fresh_loop(loop_index);
            }
            return;
        }
        if !connected {
            self.splice_open_chain(after_edge, loop_index, &walk);
            return;
        }
        let hit_edge = walk.clip.hit_edge;
        let hit_loop = self.edge(walk.clipped_edge).loop_index;
        if fresh {
            // Nothing was spliced into the fresh component, so the chain is a slit
            // reaching into the loop it landed on rather than a loop of its own.
            self.rollback_fresh_loop(loop_index);
            let hit_loop = self.edge(walk.clipped_edge).loop_index;
            self.edges[usize::from(walk.clipped_edge)].next = walk.back_head;
            self.edges[usize::from(walk.back_tail)].next = walk.front_head;
            self.edges[usize::from(walk.front_tail)].next = hit_edge;
            self.relabel_loop(walk.front_head, hit_loop);
            self.break_org_loop = walk.back_tail;
            self.refresh_loop_bounds(hit_loop);
            return;
        }
        if hit_loop == loop_index {
            self.split_loop(after_edge, loop_index, hit_edge, &walk);
        } else {
            self.merge_loops(after_edge, loop_index, hit_loop, hit_edge, &walk);
        }
    }

    /// Drops a loop slot that was reserved for a chain which never landed in it.
    fn rollback_fresh_loop(&mut self, li: u8) {
        if li + 1 == self.loop_count {
            self.loops[usize::from(li)] = FxGlassCrackLoop::default();
            self.loop_count -= 1;
        }
    }

    fn splice_open_chain(&mut self, after_edge: u16, loop_index: u8, walk: &FxGlassCrackWalk) {
        if after_edge != FX_GLASS_EDGE_NONE {
            let after_next = self.edge(after_edge).next;
            self.edges[usize::from(walk.back_tail)].next = after_next;
            self.edges[usize::from(walk.front_tail)].next = walk.back_head;
            self.edges[usize::from(after_edge)].next = walk.front_head;
            self.relabel_loop(walk.front_head, loop_index);
            self.refresh_loop_bounds(loop_index);
            return;
        }
        // Nothing to splice into yet: the outward chain and its reverse close the
        // fresh component, which later branches at this vertex attach to.
        self.edges[usize::from(walk.front_tail)].next = walk.back_head;
        self.edges[usize::from(walk.back_tail)].next = walk.front_head;
        self.loops[usize::from(loop_index)].first_edge = walk.front_head;
        self.break_org_loop = walk.back_tail;
        self.relabel_loop(walk.front_head, loop_index);
        self.refresh_loop_bounds(loop_index);
    }

    fn split_loop(
        &mut self,
        after_edge: u16,
        loop_index: u8,
        hit_edge: u16,
        walk: &FxGlassCrackWalk,
    ) {
        let clipped = walk.clipped_edge;
        let after = after_edge;
        let after_next = self.edge(after).next;
        self.edges[usize::from(clipped)].next = walk.back_head;
        self.edges[usize::from(walk.back_tail)].next = after_next;
        self.edges[usize::from(after)].next = walk.front_head;
        self.edges[usize::from(walk.front_tail)].next = hit_edge;
        if usize::from(self.loop_count) >= FX_GLASS_CRACK_LOOP_MAX {
            self.relabel_loop(walk.front_head, loop_index);
            self.refresh_loop_bounds(loop_index);
            return;
        }
        let new_li = self.loop_count;
        self.loop_count += 1;
        self.loops[usize::from(loop_index)].first_edge = walk.front_head;
        self.loops[usize::from(new_li)].first_edge = clipped;
        self.relabel_loop(walk.front_head, loop_index);
        self.relabel_loop(clipped, new_li);
        self.refresh_loop_bounds(loop_index);
        self.refresh_loop_bounds(new_li);
    }

    fn merge_loops(
        &mut self,
        after_edge: u16,
        loop_index: u8,
        hit_loop: u8,
        hit_edge: u16,
        walk: &FxGlassCrackWalk,
    ) {
        let clipped = walk.clipped_edge;
        let keep = loop_index.min(hit_loop);
        let drop = loop_index.max(hit_loop);
        let after_next = self.edge(after_edge).next;
        self.edges[usize::from(walk.back_tail)].next = after_next;
        self.edges[usize::from(after_edge)].next = walk.front_head;
        self.edges[usize::from(clipped)].next = walk.back_head;
        self.edges[usize::from(walk.front_tail)].next = hit_edge;
        let a = self.loops[usize::from(keep)];
        let b = self.loops[usize::from(drop)];
        self.loops[usize::from(keep)] = FxGlassCrackLoop {
            first_edge: walk.front_head,
            mins: [a.mins[0].min(b.mins[0]), a.mins[1].min(b.mins[1])],
            maxs: [a.maxs[0].max(b.maxs[0]), a.maxs[1].max(b.maxs[1])],
        };
        self.relabel_loop(walk.front_head, keep);
        // Dense renumbering: the absorbed row is swap-removed and the moved loop relabelled.
        self.loop_count -= 1;
        let last = self.loop_count;
        if drop != last {
            self.loops[usize::from(drop)] = self.loops[usize::from(last)];
            let moved = self.loops[usize::from(drop)].first_edge;
            if moved != FX_GLASS_EDGE_NONE {
                self.relabel_loop(moved, drop);
            }
        }
        self.loops[usize::from(last)] = FxGlassCrackLoop::default();
        self.refresh_loop_bounds(keep);
    }

    /// Seeds the star (or the border wedge), then drains the LIFO branch stack.
    pub fn create_cracks(&mut self) -> bool {
        let (after_edge, seed_dir) = self.pick_seed_dir();
        let (count, step, mut dir) = if after_edge == FX_GLASS_EDGE_NONE {
            let n = glass_interior_branch_count(self.rand.next());
            (n, FX_GLASS_SHATTER_TWO_PI / n as f32, seed_dir)
        } else {
            let row = self.edge(after_edge);
            let next = self.edge(row.next);
            let incoming = [-row.dir[0], -row.dir[1]];
            let outgoing = next.dir;
            let (incoming, _) = norm2(incoming).unwrap_or(([1.0, 0.0], 1.0));
            let (outgoing, _) = norm2(outgoing).unwrap_or(([0.0, 1.0], 1.0));
            // Faces are wound CCW and their interior is to the left of every edge, so
            // the interior wedge at the break point is the turn from the edge leaving
            // it round to the edge arriving at it. Sweeping the other way would aim
            // the first arm out through the exterior, and a crack that leaves the face
            // splits off a loop that is not part of the pane at all.
            let mut angle = libm::atan2f(cross2(outgoing, incoming), dot2(outgoing, incoming));
            if angle < 0.0 {
                angle += FX_GLASS_SHATTER_TWO_PI;
            }
            let scaled = angle * (0.5 + 0.5 * self.rand.next()) / FX_GLASS_SHATTER_TWO_PI;
            let mut n = scaled as u32;
            if n == 0 {
                if angle <= CRACK_BORDER_MIN_ANGLE {
                    return false;
                }
                n = 1;
            }
            (n, angle / (n + 1) as f32, outgoing)
        };
        if count == 0 {
            return false;
        }
        self.break_org_loop = FX_GLASS_EDGE_NONE;
        let base = self.branch_stack_level;
        let c = libm::cosf(step);
        let s = libm::sinf(step);
        // Deflection budget narrows as the star gets denser so neighbouring arms
        // cannot cross before they reach the border.
        let deflect_limit = 0.5 * c + 0.5;
        let max_cos = CRACK_DEFLECT_MIX + CRACK_DEFLECT_MIX * deflect_limit;
        for _ in 0..count {
            if usize::from(self.branch_stack_level) >= FX_GLASS_CRACK_BRANCH_MAX {
                break;
            }
            dir = [c * dir[0] - s * dir[1], c * dir[1] + s * dir[0]];
            let len = (CRACK_SEED_MIN + CRACK_SEED_SPAN * self.rand.next()) * self.lookahead();
            let sign = if self.rand.next() < 0.5 { -1.0 } else { 1.0 };
            self.branch_depth[usize::from(self.branch_stack_level)] = 0;
            self.branch_stack[usize::from(self.branch_stack_level)] = FxGlassCrackBranch {
                after_edge,
                start_index: self.break_org_index,
                dir: random_rotate(&mut self.rand, dir, max_cos, sign),
                len,
                base_dir: dir,
                deflect_limit,
                prior_crack_length: 0.0,
            };
            self.branch_stack_level += 1;
        }
        while self.branch_stack_level > base {
            self.branch_stack_level -= 1;
            let branch = self.branch_stack[usize::from(self.branch_stack_level)];
            let depth = self.branch_depth[usize::from(self.branch_stack_level)];
            self.process_crack(branch, depth);
        }
        true
    }

    /// One traversal start per loop, preferring a true border edge and otherwise a
    /// spur tip so the contour walk begins outside an open crack.
    pub fn loop_start_edge(&self, li: u8) -> u16 {
        let first = self.loops[usize::from(li)].first_edge;
        if first == FX_GLASS_EDGE_NONE {
            return FX_GLASS_EDGE_NONE;
        }
        let mut best = FX_GLASS_EDGE_NONE;
        let mut e = first;
        loop {
            let row = self.edge(e);
            if row.twin == FX_GLASS_EDGE_NONE {
                return e;
            }
            if self.edge(row.next).i1 == row.i0 {
                best = e;
            }
            e = row.next;
            if e == first || e == FX_GLASS_EDGE_NONE {
                break;
            }
        }
        if best != FX_GLASS_EDGE_NONE {
            best
        } else {
            first
        }
    }
}

fn pack_coord(v: f32) -> i16 {
    let scaled = v * CRACK_PACK_SCALE;
    let r = libm::floorf(scaled + 0.5);
    r.clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}
