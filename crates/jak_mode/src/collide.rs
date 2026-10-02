//! Collision against the world as triangles: a sphere swept along a move,
//! the earliest triangle it touches, and the cache of nearby triangles each
//! query reads. The world itself is whatever the host hands over through
//! [`CollideWorld`].
use glam::Vec3;

/// How a triangle behaves underfoot. `wall_angle` is the steepest the
/// contact normal may lean from vertical and still be ground.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PatMode {
    #[default]
    Ground,
    Wall,
    Obstacle,
    Halfpipe,
}

impl PatMode {
    pub fn wall_angle(self) -> f32 {
        match self {
            PatMode::Ground => 0.2,
            PatMode::Wall => 2.0,
            PatMode::Obstacle => 0.82,
            PatMode::Halfpipe => -2.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PatMaterial {
    #[default]
    Stone,
    Ice,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PatEvent {
    #[default]
    None,
    Rail,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pat {
    pub mode: PatMode,
    pub material: PatMaterial,
    pub event: PatEvent,
}

/// A world triangle, wound counterclockwise seen from the side it blocks.
#[derive(Clone, Copy, Debug)]
pub struct Tri {
    pub v: [Vec3; 3],
    pub pat: Pat,
}

/// The host's world: every triangle that may touch the box `min..max`.
pub trait CollideWorld {
    fn fill(&mut self, min: Vec3, max: Vec3, out: &mut Vec<Tri>);
}

/// A world with nothing in it.
pub struct EmptyWorld;

impl CollideWorld for EmptyWorld {
    fn fill(&mut self, _min: Vec3, _max: Vec3, _out: &mut Vec<Tri>) {}
}

#[derive(Clone, Copy, Debug)]
struct CachedTri {
    v: [Vec3; 3],
    normal: Vec3,
    min: Vec3,
    max: Vec3,
    pat: Pat,
}

/// What a sweep hit: how far along the move (0..1), the contact point on the
/// triangle, the triangle's face normal and its surface.
#[derive(Clone, Copy, Debug)]
pub struct TriHit {
    pub u: f32,
    pub intersect: Vec3,
    pub normal: Vec3,
    pub pat: Pat,
    pub verts: [Vec3; 3],
}

/// The triangles near one query, gathered from the world once and swept
/// against many times.
#[derive(Default)]
pub struct CollideCache {
    tris: Vec<CachedTri>,
    scratch: Vec<Tri>,
}

impl CollideCache {
    pub fn clear(&mut self) {
        self.tris.clear();
    }

    pub fn len(&self) -> usize {
        self.tris.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }

    pub fn fill_box(&mut self, world: &mut dyn CollideWorld, min: Vec3, max: Vec3) {
        self.tris.clear();
        self.scratch.clear();
        world.fill(min, max, &mut self.scratch);
        for t in self.scratch.drain(..) {
            let n = (t.v[2] - t.v[1]).cross(t.v[0] - t.v[1]);
            let l = n.length();
            if l.is_nan() || l <= 1e-6 || !n.is_finite() {
                continue;
            }
            let tmin = t.v[0].min(t.v[1]).min(t.v[2]);
            let tmax = t.v[0].max(t.v[1]).max(t.v[2]);
            if tmax.cmplt(min).any() || tmin.cmpgt(max).any() {
                continue;
            }
            self.tris.push(CachedTri {
                v: t.v,
                normal: n / l,
                min: tmin,
                max: tmax,
                pat: t.pat,
            });
        }
    }

    /// The cache for a sphere of `radius` moving from `start` by `motion`.
    pub fn fill_line_sphere(
        &mut self,
        world: &mut dyn CollideWorld,
        start: Vec3,
        motion: Vec3,
        radius: f32,
    ) {
        let end = start + motion;
        let r = Vec3::splat(radius);
        self.fill_box(world, start.min(end) - r, start.max(end) + r);
    }

    /// The earliest triangle a sphere at `center` meets moving by `motion`,
    /// earlier than `best` (a fraction; negative means no limit). A solid
    /// sweep ignores triangles it moves away from or starts behind.
    pub fn resolve_moving_sphere(
        &self,
        center: Vec3,
        radius: f32,
        motion: Vec3,
        best: f32,
        solid: bool,
    ) -> Option<TriHit> {
        let mut best_u = if best >= 0.0 { best } else { 2.0 };
        let mut found = None;
        let lo = center.min(center + motion) - Vec3::splat(radius);
        let hi = center.max(center + motion) + Vec3::splat(radius);
        for t in &self.tris {
            if t.max.cmplt(lo).any() || t.min.cmpgt(hi).any() {
                continue;
            }
            let Some((u, intersect)) = moving_sphere_triangle(center, radius, motion, t) else {
                continue;
            };
            if u < 0.0 || best_u <= u {
                continue;
            }
            if solid && (motion.dot(t.normal) >= 0.0 || (center - intersect).dot(t.normal) < 0.0) {
                continue;
            }
            best_u = u;
            found = Some(TriHit {
                u,
                intersect,
                normal: t.normal,
                pat: t.pat,
                verts: t.v,
            });
        }
        found
    }

    /// Whether a sphere at rest touches any cached triangle.
    pub fn overlaps_sphere(&self, center: Vec3, radius: f32) -> bool {
        self.resolve_moving_sphere(center, radius, Vec3::ZERO, -1.0, false)
            .is_some()
    }

    /// A solid sphere probe along `motion`.
    pub fn probe_line_sphere(&self, start: Vec3, motion: Vec3, radius: f32) -> Option<TriHit> {
        self.resolve_moving_sphere(start, radius, motion, -1.0, true)
    }
}

/// Fills a private cache along the probe and sweeps it.
pub fn fill_and_probe_line_sphere(
    world: &mut dyn CollideWorld,
    cache: &mut CollideCache,
    start: Vec3,
    motion: Vec3,
    radius: f32,
) -> Option<TriHit> {
    cache.fill_line_sphere(world, start, motion, radius);
    cache.probe_line_sphere(start, motion, radius)
}

/// The fraction along `dir` at which a ray from `origin` enters the sphere
/// at `sphere` of `radius`; 0 when it starts inside.
fn ray_sphere(origin: Vec3, dir: Vec3, sphere: Vec3, radius: f32) -> Option<f32> {
    let o = origin - sphere;
    let c = o.length_squared() - radius * radius;
    if c < 0.0 {
        return Some(0.0);
    }
    let a = dir.length_squared();
    if a == 0.0 {
        return None;
    }
    let b = dir.dot(o);
    if b >= 0.0 {
        return None;
    }
    let disc = b * b - a * c;
    if disc < 0.0 {
        return None;
    }
    let u = (-b - disc.sqrt()) / a;
    if u > 1.0 { None } else { Some(u.max(0.0)) }
}

/// The fraction along `dir` at which a ray meets the finite cylinder from
/// `base` along unit `axis` for `length`, and the point on the axis there.
fn ray_cylinder(
    origin: Vec3,
    dir: Vec3,
    base: Vec3,
    axis: Vec3,
    radius: f32,
    length: f32,
) -> Option<(f32, Vec3)> {
    let rel = origin - base;
    let s0 = rel.dot(axis);
    let ds = dir.dot(axis);
    let s1 = s0 + ds;
    if s0 < 0.0 && s1 < 0.0 {
        return None;
    }
    if s0 >= length && s1 >= length {
        return None;
    }
    let perp_o = rel - axis * s0;
    let perp_d = dir - axis * ds;
    let u = ray_sphere(perp_o, perp_d, Vec3::ZERO, radius)?;
    let s = s0 + ds * u;
    if s < 0.0 || s >= length {
        return None;
    }
    Some((u, base + axis * s))
}

fn inside_triangle(p: Vec3, t: &CachedTri) -> bool {
    let (a, b, c) = (t.v[0], t.v[1], t.v[2]);
    let n = t.normal;
    (b - a).cross(p - a).dot(n) >= 0.0
        && (c - b).cross(p - b).dot(n) >= 0.0
        && (a - c).cross(p - c).dot(n) >= 0.0
}

/// A sphere swept against one triangle: the face first, then its three
/// corners and three edges. Returns the fraction of the move and the contact
/// point on the triangle; 0 when the sphere already overlaps it.
fn moving_sphere_triangle(
    center: Vec3,
    radius: f32,
    motion: Vec3,
    t: &CachedTri,
) -> Option<(f32, Vec3)> {
    let n = t.normal;
    let d = (center - t.v[1]).dot(n);
    let m = motion.dot(n);
    if d.abs() < radius {
        let p = center - n * d;
        if inside_triangle(p, t) {
            return Some((0.0, p));
        }
        return edges_and_corners(center, radius, motion, t);
    }
    if m == 0.0 {
        return None;
    }
    let ta = (radius - d) / m;
    let tb = (-radius - d) / m;
    if ta < 0.0 && tb < 0.0 {
        return None;
    }
    let u = ta.min(tb);
    if u >= 1.0 {
        return None;
    }
    let at = center + motion * u;
    let p = at - n * (at - t.v[1]).dot(n);
    if inside_triangle(p, t) {
        return Some((u, p));
    }
    edges_and_corners(center, radius, motion, t)
}

fn edges_and_corners(
    center: Vec3,
    radius: f32,
    motion: Vec3,
    t: &CachedTri,
) -> Option<(f32, Vec3)> {
    let mut best: Option<(f32, Vec3)> = None;
    let mut keep = |hit: Option<(f32, Vec3)>| {
        if let Some((u, p)) = hit
            && u >= 0.0
            && best.is_none_or(|(b, _)| u < b)
        {
            best = Some((u, p));
        }
    };
    for v in t.v {
        keep(ray_sphere(center, motion, v, radius).map(|u| (u, v)));
    }
    for (a, b) in [(t.v[0], t.v[1]), (t.v[1], t.v[2]), (t.v[2], t.v[0])] {
        let edge = b - a;
        let length = edge.length();
        if length > 0.0 {
            keep(ray_cylinder(
                center,
                motion,
                a,
                edge / length,
                radius,
                length,
            ));
        }
    }
    best.filter(|(u, _)| *u <= 1.0)
}

/// A static soup of triangles bucketed on a grid, for a world that does not
/// change while Jak moves through it.
pub struct TriangleGrid {
    cell: f32,
    tris: Vec<Tri>,
    cells: std::collections::HashMap<(i32, i32, i32), Vec<u32>>,
    stamp: Vec<u32>,
    epoch: u32,
}

impl TriangleGrid {
    pub fn new(tris: Vec<Tri>, cell: f32) -> Self {
        let mut cells: std::collections::HashMap<(i32, i32, i32), Vec<u32>> = Default::default();
        let key = |p: Vec3| {
            (
                (p.x / cell).floor() as i32,
                (p.y / cell).floor() as i32,
                (p.z / cell).floor() as i32,
            )
        };
        for (i, t) in tris.iter().enumerate() {
            let lo = key(t.v[0].min(t.v[1]).min(t.v[2]));
            let hi = key(t.v[0].max(t.v[1]).max(t.v[2]));
            for x in lo.0..=hi.0 {
                for y in lo.1..=hi.1 {
                    for z in lo.2..=hi.2 {
                        cells.entry((x, y, z)).or_default().push(i as u32);
                    }
                }
            }
        }
        let stamp = vec![0; tris.len()];
        Self {
            cell,
            tris,
            cells,
            stamp,
            epoch: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.tris.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }
}

impl CollideWorld for TriangleGrid {
    fn fill(&mut self, min: Vec3, max: Vec3, out: &mut Vec<Tri>) {
        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            self.stamp.fill(0);
            self.epoch = 1;
        }
        let cell = self.cell;
        let key = |p: Vec3| {
            (
                (p.x / cell).floor() as i32,
                (p.y / cell).floor() as i32,
                (p.z / cell).floor() as i32,
            )
        };
        let (lo, hi) = (key(min), key(max));
        let span = (hi.0 - lo.0 + 1) as i64 * (hi.1 - lo.1 + 1) as i64 * (hi.2 - lo.2 + 1) as i64;
        if span > 4096 {
            return;
        }
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                for z in lo.2..=hi.2 {
                    let Some(list) = self.cells.get(&(x, y, z)) else {
                        continue;
                    };
                    for &i in list {
                        let slot = &mut self.stamp[i as usize];
                        if *slot != self.epoch {
                            *slot = self.epoch;
                            out.push(self.tris[i as usize]);
                        }
                    }
                }
            }
        }
    }
}
