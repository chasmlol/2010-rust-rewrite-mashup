//! Collision against a Minecraft world. While a Minecraft map is loaded, world
//! traces against the map it stands in for test block collision shapes
//! instead of that map's brushes and meshes.
//!
//! Minecraft space is blocks with Y up. A map unit point `(X, Y, Z)` is block
//! point `origin + (X, Z, -Y) / BLOCK`: X stays east, Z up becomes Y up, and
//! the left-handed-looking swap of Y and Z is a proper rotation.
use std::collections::HashMap;
use std::sync::RwLock;

use crate::world::SimBrush;

/// Map units per block: the 70-unit soldier stands about two blocks tall,
/// and a block is below the 39-unit jump, so one block can be jumped onto.
pub const BLOCK: f32 = 36.0;
/// Pulled back from every hit, as IW4 traces keep off surfaces.
const SURFACE_CLIP_EPSILON: f32 = 0.125;
const SOLID: u32 = 1;
/// `SURF_TYPE` bits for a stone-like surface.
const STONE_SURFACE: u32 = 17 << 20;

/// One chunk column of shape ids, x fastest then z then y.
pub struct VoxelChunk {
    pub min_y: i32,
    pub height: i32,
    pub shapes: Vec<u16>,
}

/// The block world traces run against, and where it sits in map space.
#[derive(Default)]
pub struct VoxelWorld {
    brushes: usize,
    origin: [f64; 3],
    chunks: HashMap<(i32, i32), VoxelChunk>,
    /// Collision boxes of each shape id, in block space `[min, max]`; id 0 is
    /// empty.
    shapes: Vec<Vec<[f32; 6]>>,
}

static WORLD: RwLock<Option<VoxelWorld>> = RwLock::new(None);
/// Bumped whenever the block world's collision changes.
static REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn bump() {
    REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Changes with every change to the block world's collision.
pub fn revision() -> u64 {
    REVISION.load(std::sync::atomic::Ordering::Relaxed)
}

/// What the authoritative game did to the block world this tick, for the
/// world's owner to apply.
#[derive(Clone, Copy, Debug)]
pub enum VoxelEvent {
    /// A bullet struck this block, with the damage it would have done: the
    /// weapon's, at that range, after penetration.
    Shot { block: [i32; 3], damage: f32 },
    /// An explosion went off here, in blocks.
    Explosion { center: [f64; 3] },
    /// A bullet struck the mob with this key, with the damage it would have
    /// done at that range, from this block point.
    MobShot { key: u64, damage: f32, from: [f64; 3] },
    /// A bullet's path through the air, in blocks, up to what stopped it:
    /// blocks without collision on it (grass, flowers) take its damage.
    Ray { from: [f64; 3], to: [f64; 3], damage: f32 },
}

static EVENTS: std::sync::Mutex<Vec<VoxelEvent>> = std::sync::Mutex::new(Vec::new());

/// A bullet impact at map point `end` on a surface facing `normal`: the
/// block behind the surface.
pub fn push_shot(end: [f32; 3], normal: [f32; 3], damage: f32) {
    let Ok(world) = WORLD.read() else {
        return;
    };
    let Some(world) = world.as_ref() else {
        return;
    };
    let p = to_block(world.origin, end);
    // Into the surface, past the trace's pull-back.
    let n = [f64::from(normal[0]), f64::from(normal[2]), -f64::from(normal[1])];
    let block = std::array::from_fn(|k| (p[k] - n[k] * 0.05).floor() as i32);
    if let Ok(mut events) = EVENTS.lock() {
        events.push(VoxelEvent::Shot { block, damage });
    }
}

/// An explosion at map point `origin`.
pub fn push_explosion(origin: [f32; 3]) {
    let Ok(world) = WORLD.read() else {
        return;
    };
    let Some(world) = world.as_ref() else {
        return;
    };
    let center = to_block(world.origin, origin);
    if let Ok(mut events) = EVENTS.lock() {
        events.push(VoxelEvent::Explosion { center });
    }
}

/// The world's mobs as bullets see them: a key and a block-space box
/// `[min, max]` each.
static MOB_BOXES: RwLock<Vec<(u64, [f64; 6])>> = RwLock::new(Vec::new());

pub fn set_mob_boxes(boxes: Vec<(u64, [f64; 6])>) {
    if let Ok(mut held) = MOB_BOXES.write() {
        *held = boxes;
    }
}

/// The nearest mob on the map-space segment `start`..`end`: its key, the
/// distance to it in map units and how far up its box the bullet struck
/// (0 at the feet, 1 at the top).
pub(crate) fn mob_on_segment(start: [f32; 3], end: [f32; 3]) -> Option<(u64, f32, f32)> {
    let world = WORLD.read().ok()?;
    let origin = world.as_ref()?.origin;
    let a = to_block(origin, start);
    let b = to_block(origin, end);
    let d: [f64; 3] = std::array::from_fn(|k| b[k] - a[k]);
    let boxes = MOB_BOXES.read().ok()?;
    let mut best: Option<(u64, f64, f64)> = None;
    for &(key, bb) in boxes.iter() {
        let (mut t0, mut t1) = (0.0f64, 1.0f64);
        let mut hit = true;
        for k in 0..3 {
            if d[k].abs() < 1e-12 {
                if a[k] < bb[k] || a[k] > bb[k + 3] {
                    hit = false;
                    break;
                }
                continue;
            }
            let (u, v) = ((bb[k] - a[k]) / d[k], (bb[k + 3] - a[k]) / d[k]);
            t0 = t0.max(u.min(v));
            t1 = t1.min(u.max(v));
            if t0 > t1 {
                hit = false;
                break;
            }
        }
        if hit && best.is_none_or(|(_, t, _)| t0 < t) {
            let y = a[1] + d[1] * t0;
            let up = ((y - bb[1]) / (bb[4] - bb[1]).max(1e-6)).clamp(0.0, 1.0);
            best = Some((key, t0, up));
        }
    }
    let length = f64::from(
        ((end[0] - start[0]).powi(2) + (end[1] - start[1]).powi(2) + (end[2] - start[2]).powi(2)).sqrt(),
    );
    best.map(|(key, t, up)| (key, (t * length) as f32, up as f32))
}

/// A bullet's path from map point `start` to `end`.
pub(crate) fn push_ray(start: [f32; 3], end: [f32; 3], damage: f32) {
    let Ok(world) = WORLD.read() else {
        return;
    };
    let Some(world) = world.as_ref() else {
        return;
    };
    let (from, to) = (to_block(world.origin, start), to_block(world.origin, end));
    if let Ok(mut events) = EVENTS.lock() {
        events.push(VoxelEvent::Ray { from, to, damage });
    }
}

/// MW2's hit location for a strike this far up a mob's box, as a standing
/// soldier's body divides: head, neck, upper and lower torso, legs.
pub(crate) fn mob_hitloc(up: f32) -> u8 {
    match up {
        u if u > 0.87 => 2,
        u if u > 0.8 => 3,
        u if u > 0.62 => 4,
        u if u > 0.45 => 5,
        u if u > 0.22 => 12,
        _ => 14,
    }
}

/// The world's mobs as aim assist sees them: each box's centre in map
/// units and its half width.
/// Each mob's key and box in map space, mins then maxs.
pub fn mob_targets() -> Vec<(u64, [f32; 3], [f32; 3])> {
    let Ok(world) = WORLD.read() else {
        return Vec::new();
    };
    let Some(origin) = world.as_ref().map(|w| w.origin) else {
        return Vec::new();
    };
    let Ok(boxes) = MOB_BOXES.read() else {
        return Vec::new();
    };
    boxes
        .iter()
        .map(|(key, b)| {
            let a = to_map(origin, [b[0], b[1], b[2]]);
            let c = to_map(origin, [b[3], b[4], b[5]]);
            let mins = [a[0].min(c[0]), a[1].min(c[1]), a[2].min(c[2])];
            let maxs = [a[0].max(c[0]), a[1].max(c[1]), a[2].max(c[2])];
            (*key, mins, maxs)
        })
        .collect()
}

/// A strike on the mob with this key from map point `from`, with the damage
/// it does.
pub fn push_mob_shot(key: u64, damage: f32, from: [f32; 3]) {
    let Ok(world) = WORLD.read() else {
        return;
    };
    let Some(world) = world.as_ref() else {
        return;
    };
    let from = to_block(world.origin, from);
    if let Ok(mut events) = EVENTS.lock() {
        events.push(VoxelEvent::MobShot { key, damage, from });
    }
}

/// Damage the world's mobs dealt the players: client, amount, and where it
/// came from in map space.
static PLAYER_DAMAGE: std::sync::Mutex<Vec<(u32, i32, Option<[f32; 3]>)>> = std::sync::Mutex::new(Vec::new());

pub fn push_player_damage(client: u32, amount: i32, from: Option<[f32; 3]>) {
    if let Ok(mut damage) = PLAYER_DAMAGE.lock() {
        damage.push((client, amount, from));
    }
}

pub(crate) fn take_player_damage() -> Vec<(u32, i32, Option<[f32; 3]>)> {
    PLAYER_DAMAGE.lock().map(|mut d| std::mem::take(&mut *d)).unwrap_or_default()
}

pub fn take_events() -> Vec<VoxelEvent> {
    EVENTS.lock().map(|mut events| std::mem::take(&mut *events)).unwrap_or_default()
}

/// Sets one block's collision shape id, as `set_chunk` lays them out.
pub fn set_block_shape(x: i32, y: i32, z: i32, shape: u16) {
    if let Ok(mut world) = WORLD.write()
        && let Some(world) = world.as_mut()
        && let Some(chunk) = world.chunks.get_mut(&(x >> 4, z >> 4))
    {
        let ly = y - chunk.min_y;
        if ly >= 0 && ly < chunk.height {
            let index = ((ly * 16 + (z & 15)) * 16 + (x & 15)) as usize;
            if let Some(slot) = chunk.shapes.get_mut(index) {
                *slot = shape;
                bump();
            }
        }
    }
}

/// Starts replacing world collision for the map whose brush table is
/// `brushes`, with the block world's `origin` block at map origin.
pub fn activate(brushes: &[SimBrush], origin: [f64; 3], shapes: Vec<Vec<[f32; 6]>>) {
    if let Ok(mut world) = WORLD.write() {
        *world = Some(VoxelWorld {
            brushes: brushes.as_ptr() as usize,
            origin,
            chunks: HashMap::new(),
            shapes,
        });
    }
    bump();
}

pub fn deactivate() {
    if let Ok(mut world) = WORLD.write() {
        *world = None;
    }
    let _ = take_events();
    let _ = take_player_damage();
    set_mob_boxes(Vec::new());
}

/// Adds shape ids to the table and returns the first new id.
pub fn add_shapes(more: Vec<Vec<[f32; 6]>>) -> Option<u16> {
    let mut world = WORLD.write().ok()?;
    let world = world.as_mut()?;
    let first = world.shapes.len() as u16;
    world.shapes.extend(more);
    Some(first)
}

pub fn set_chunk(x: i32, z: i32, chunk: VoxelChunk) {
    if let Ok(mut world) = WORLD.write()
        && let Some(world) = world.as_mut()
    {
        world.chunks.insert((x, z), chunk);
    }
    bump();
}

pub fn remove_chunk(x: i32, z: i32) {
    if let Ok(mut world) = WORLD.write()
        && let Some(world) = world.as_mut()
    {
        world.chunks.remove(&(x, z));
    }
    bump();
}

/// The faces of the block world's collision boxes within `radius` blocks
/// across and `depth` blocks up or down of map point `centre`, as triangles
/// in map units wound counterclockwise seen from outside. A face against a
/// full neighbouring block is left out; blocks without collision (grass,
/// flowers) have none.
pub fn collision_triangles(centre: [f32; 3], radius: i32, depth: i32) -> Vec<[[f32; 3]; 3]> {
    let Ok(world) = WORLD.read() else {
        return Vec::new();
    };
    let Some(world) = world.as_ref() else {
        return Vec::new();
    };
    let c = to_block(world.origin, centre);
    let (cx, cy, cz) = (c[0].floor() as i32, c[1].floor() as i32, c[2].floor() as i32);
    let mut out = Vec::new();
    block_triangles(world, [cx - radius, cy - depth, cz - radius], [cx + radius, cy + depth, cz + radius], &mut out);
    out
}

/// [`collision_triangles`] for the blocks the map-space box `min..max`
/// touches.
pub fn collision_triangles_in(min: [f32; 3], max: [f32; 3], out: &mut Vec<[[f32; 3]; 3]>) {
    let Ok(world) = WORLD.read() else {
        return;
    };
    let Some(world) = world.as_ref() else {
        return;
    };
    let a = to_block(world.origin, min);
    let b = to_block(world.origin, max);
    let lo: [i32; 3] = std::array::from_fn(|k| a[k].min(b[k]).floor() as i32);
    let hi: [i32; 3] = std::array::from_fn(|k| a[k].max(b[k]).floor() as i32);
    if (0..3).map(|k| i64::from(hi[k] - lo[k] + 1)).product::<i64>() > 32 * 32 * 32 {
        return;
    }
    block_triangles(world, lo, hi, out);
}

/// The block-box faces of the blocks from `lo` to `hi` inclusive, in map
/// units, wound counterclockwise seen from outside.
fn block_triangles(world: &VoxelWorld, lo: [i32; 3], hi: [i32; 3], out: &mut Vec<[[f32; 3]; 3]>) {
    let full: Vec<bool> = world.shapes.iter().map(|b| b.len() == 1 && b[0] == [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]).collect();
    let is_full = |x: i32, y: i32, z: i32| -> bool {
        let Some(chunk) = world.chunks.get(&(x >> 4, z >> 4)) else {
            return false;
        };
        let ly = y - chunk.min_y;
        if ly < 0 || ly >= chunk.height {
            return false;
        }
        let id = chunk.shapes[((ly * 16 + (z & 15)) * 16 + (x & 15)) as usize];
        full.get(usize::from(id)).copied().unwrap_or(false)
    };
    for x in lo[0]..=hi[0] {
        for z in lo[2]..=hi[2] {
            for y in lo[1]..=hi[1] {
                let boxes = world.shape_at(x, y, z);
                if boxes.is_empty() {
                    continue;
                }
                let base = [f64::from(x), f64::from(y), f64::from(z)];
                for b in boxes {
                    let lo = [f64::from(b[0]), f64::from(b[1]), f64::from(b[2])];
                    let hi = [f64::from(b[3]), f64::from(b[4]), f64::from(b[5])];
                    for axis in 0..3 {
                        // Tangents whose cross product is the axis.
                        let (u, v) = [(1, 2), (2, 0), (0, 1)][axis];
                        for high in [false, true] {
                            let at = if high { hi[axis] } else { lo[axis] };
                            let mut step = [0; 3];
                            step[axis] = if high { 1 } else { -1 };
                            let on_edge = if high { at >= 1.0 } else { at <= 0.0 };
                            if on_edge && is_full(x + step[0], y + step[1], z + step[2]) {
                                continue;
                            }
                            let corner = |a: f64, b: f64| {
                                let mut p = [0.0; 3];
                                p[axis] = at;
                                p[u] = a;
                                p[v] = b;
                                to_map(world.origin, std::array::from_fn(|k| base[k] + p[k]))
                            };
                            let mut quad = [
                                corner(lo[u], lo[v]),
                                corner(hi[u], lo[v]),
                                corner(hi[u], hi[v]),
                                corner(lo[u], hi[v]),
                            ];
                            if !high {
                                quad.reverse();
                            }
                            out.push([quad[0], quad[1], quad[2]]);
                            out.push([quad[0], quad[2], quad[3]]);
                        }
                    }
                }
            }
        }
    }
}

/// Whether traces against `brushes` go to the block world.
pub(crate) fn active_for(brushes: &[SimBrush]) -> bool {
    WORLD
        .read()
        .ok()
        .is_some_and(|world| world.as_ref().is_some_and(|w| w.brushes == brushes.as_ptr() as usize))
}

/// Whether any block world is standing in for a map.
pub fn active() -> bool {
    WORLD.read().ok().is_some_and(|world| world.is_some())
}

/// Map point to block point.
pub fn to_block(origin: [f64; 3], p: [f32; 3]) -> [f64; 3] {
    let s = f64::from(BLOCK);
    [
        origin[0] + f64::from(p[0]) / s,
        origin[1] + f64::from(p[2]) / s,
        origin[2] - f64::from(p[1]) / s,
    ]
}

/// Block point to map point.
pub fn to_map(origin: [f64; 3], b: [f64; 3]) -> [f32; 3] {
    let s = f64::from(BLOCK);
    [
        ((b[0] - origin[0]) * s) as f32,
        (-(b[2] - origin[2]) * s) as f32,
        ((b[1] - origin[1]) * s) as f32,
    ]
}

impl VoxelWorld {
    fn shape_at(&self, x: i32, y: i32, z: i32) -> &[[f32; 6]] {
        let Some(chunk) = self.chunks.get(&(x >> 4, z >> 4)) else {
            return &[];
        };
        let ly = y - chunk.min_y;
        if ly < 0 || ly >= chunk.height {
            return &[];
        }
        let index = ((ly * 16 + (z & 15)) * 16 + (x & 15)) as usize;
        let id = chunk.shapes.get(index).copied().unwrap_or(0);
        self.shapes.get(usize::from(id)).map_or(&[], Vec::as_slice)
    }
}

struct Sweep {
    first: Option<(f64, [f64; 3])>,
    start_solid: bool,
    end_solid: bool,
}

impl VoxelWorld {
    /// A block-space box `[lo, hi]` about `a` moved to `b`: the first hit as
    /// a fraction of the move and its normal, or whether it starts inside.
    fn sweep(&self, a: [f64; 3], b: [f64; 3], lo: [f64; 3], hi: [f64; 3], test_start: bool) -> Sweep {
        const INSIDE: f64 = 1e-4;
        let delta: [f64; 3] = std::array::from_fn(|k| b[k] - a[k]);
        let reach_lo: [i32; 3] = std::array::from_fn(|k| (a[k].min(b[k]) + lo[k] - 1.0).floor() as i32);
        let reach_hi: [i32; 3] = std::array::from_fn(|k| (a[k].max(b[k]) + hi[k] + 1.0).floor() as i32);
        let mut out = Sweep {
            first: None,
            start_solid: false,
            end_solid: false,
        };
        let mut best_t = f64::MAX;
        for y in reach_lo[1]..=reach_hi[1] {
            for z in reach_lo[2]..=reach_hi[2] {
                for x in reach_lo[0]..=reach_hi[0] {
                    for shape in self.shape_at(x, y, z) {
                        // The shape grown by the moving box.
                        let bmin = [
                            f64::from(x) + f64::from(shape[0]) - hi[0],
                            f64::from(y) + f64::from(shape[1]) - hi[1],
                            f64::from(z) + f64::from(shape[2]) - hi[2],
                        ];
                        let bmax = [
                            f64::from(x) + f64::from(shape[3]) - lo[0],
                            f64::from(y) + f64::from(shape[4]) - lo[1],
                            f64::from(z) + f64::from(shape[5]) - lo[2],
                        ];
                        let inside = |p: [f64; 3]| {
                            (0..3).all(|k| p[k] > bmin[k] + INSIDE && p[k] < bmax[k] - INSIDE)
                        };
                        if inside(a) {
                            if test_start {
                                out.start_solid = true;
                                out.end_solid |= inside(b);
                            }
                            continue;
                        }
                        let (mut t_in, mut t_out, mut axis) = (0.0f64, 1.0f64, None);
                        let mut hit = true;
                        for k in 0..3 {
                            if delta[k].abs() < 1e-12 {
                                if a[k] <= bmin[k] || a[k] >= bmax[k] {
                                    hit = false;
                                    break;
                                }
                                continue;
                            }
                            let (t0, t1) = ((bmin[k] - a[k]) / delta[k], (bmax[k] - a[k]) / delta[k]);
                            let (near, far) = (t0.min(t1), t0.max(t1));
                            if near > t_in {
                                t_in = near;
                                axis = Some(k);
                            }
                            t_out = t_out.min(far);
                            if t_in >= t_out {
                                hit = false;
                                break;
                            }
                        }
                        if hit
                            && let Some(k) = axis
                            && t_in < best_t
                        {
                            best_t = t_in;
                            let mut normal = [0.0; 3];
                            normal[k] = -delta[k].signum();
                            out.first = Some((t_in, normal));
                        }
                    }
                }
            }
        }
        out
    }
}

/// A box swept from `start` to `end` in map space against the block world.
pub(crate) fn trace(
    start: [f32; 3],
    end: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
) -> trace_iw4::Trace {
    let open = trace_iw4::Trace {
        fraction: 1.0,
        endpos: end,
        ..trace_iw4::Trace::default()
    };
    let Ok(guard) = WORLD.read() else {
        return open;
    };
    let Some(world) = guard.as_ref() else {
        return open;
    };
    let s = f64::from(BLOCK);
    let a = to_block(world.origin, start);
    let b = to_block(world.origin, end);
    // The box in block space: map X, Z and -Y extents.
    let lo = [
        f64::from(mins[0]) / s,
        f64::from(mins[2]) / s,
        -f64::from(maxs[1]) / s,
    ];
    let hi = [
        f64::from(maxs[0]) / s,
        f64::from(maxs[2]) / s,
        -f64::from(mins[1]) / s,
    ];
    let delta = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let length = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
    // Long traces (bullets) are tested in short segments, nearest first, so
    // the cells tested stay near the ray.
    let segments = (length / 4.0).ceil().max(1.0) as usize;
    let mut best_t = 1.0f64;
    let mut best_normal = [0.0f64; 3];
    let mut start_solid = false;
    let mut end_solid = false;
    for segment in 0..segments {
        let (t0, t1) = (
            segment as f64 / segments as f64,
            (segment + 1) as f64 / segments as f64,
        );
        let sa: [f64; 3] = std::array::from_fn(|k| a[k] + delta[k] * t0);
        let sb: [f64; 3] = std::array::from_fn(|k| a[k] + delta[k] * t1);
        let hit = world.sweep(sa, sb, lo, hi, segment == 0);
        if segment == 0 && hit.start_solid {
            start_solid = true;
            end_solid = segments == 1 && hit.end_solid;
            break;
        }
        if let Some((t, normal)) = hit.first {
            best_t = t0 + (t1 - t0) * t;
            best_normal = normal;
            break;
        }
    }
    if start_solid {
        return trace_iw4::Trace {
            fraction: 0.0,
            endpos: start,
            startsolid: 1,
            allsolid: u8::from(end_solid),
            contents: SOLID,
            surface_flags: STONE_SURFACE,
            ..trace_iw4::Trace::default()
        };
    }
    if best_t >= 1.0 {
        return open;
    }
    let pulled = if length > 0.0 {
        (best_t - f64::from(SURFACE_CLIP_EPSILON) / (length * s)).max(0.0)
    } else {
        0.0
    };
    let fraction = pulled as f32;
    let endpos = std::array::from_fn(|k| start[k] + (end[k] - start[k]) * fraction);
    let n = best_normal;
    trace_iw4::Trace {
        fraction,
        endpos,
        normal: [n[0] as f32, -(n[2] as f32), n[1] as f32],
        contents: SOLID,
        surface_flags: STONE_SURFACE,
        walkable: u8::from(n[1] > 0.7),
        ..trace_iw4::Trace::default()
    }
}
