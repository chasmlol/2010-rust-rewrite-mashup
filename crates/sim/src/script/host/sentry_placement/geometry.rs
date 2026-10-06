pub type Vec3 = [f64; 3];
#[derive(Clone, Copy, Debug)]
pub struct Request {
    pub player_origin: Vec3,
    pub eye: Vec3,
    pub forward: Vec3,
}
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub reach: f64,
    pub footprint_radius: f64,
    pub support_up: f64,
    pub support_down: f64,
    pub max_slope_radians: f64,
    pub max_foot_gap: f64,
    pub clearance: f64,
    pub frame_radius: f64,
    pub body_radius: f64,
    pub body_bottom: f64,
    pub body_top: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub origin: Vec3,
    pub axis: [Vec3; 3],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub valid: bool,
    pub pose: Pose,
}
#[derive(Clone, Copy, Debug)]
pub struct Support {
    pub point: Vec3,
    pub normal: Vec3,
}
pub trait Backend {
    fn support(&mut self, start: Vec3, end: Vec3) -> Option<Support>;
    fn sphere_clear(&mut self, start: Vec3, end: Vec3, radius: f64) -> bool;
}
fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: Vec3, s: f64) -> Vec3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn finite(v: Vec3) -> bool {
    v.iter().all(|x| x.is_finite())
}
fn unit(v: Vec3) -> Option<Vec3> {
    let scale = v.iter().fold(0.0_f64, |s, x| s.max(x.abs()));
    if !scale.is_finite() || scale == 0.0 {
        return None;
    }
    let v = v.map(|x| x / scale);
    let n = dot(v, v).sqrt();
    let v = mul(v, 1.0 / n);
    if finite(v) { Some(v) } else { None }
}
fn pads(p: Pose, radius: f64) -> [Vec3; 8] {
    core::array::from_fn(|i| {
        let angle = i as f64 * std::f64::consts::FRAC_PI_4;
        add(
            p.origin,
            add(
                mul(p.axis[0], radius * angle.cos()),
                mul(p.axis[1], radius * angle.sin()),
            ),
        )
    })
}
fn config_valid(c: Config) -> bool {
    [
        c.reach,
        c.footprint_radius,
        c.support_up,
        c.support_down,
        c.max_foot_gap,
        c.clearance,
        c.frame_radius,
        c.body_radius,
    ]
    .iter()
    .all(|v| v.is_finite() && *v > 0.0)
        && c.max_slope_radians.is_finite()
        && c.max_slope_radians > 0.0
        && c.max_slope_radians < std::f64::consts::FRAC_PI_2
        && c.body_top.is_finite()
        && c.body_bottom.is_finite()
        && c.body_top >= c.body_bottom
        && c.body_bottom >= c.body_radius + c.frame_radius
}
fn tolerance(c: Config) -> f64 {
    1.0e-7
        * c.footprint_radius
            .max(c.support_up)
            .max(c.support_down)
            .max(1.0)
}
#[derive(Clone, Copy)]
struct Samples {
    observed: [Option<Vec3>; 8],
    supporting: [Option<Vec3>; 8],
}
fn sample<B: Backend>(b: &mut B, p: Pose, req: Request, c: Config) -> Samples {
    let tol = tolerance(c);
    let mut observed = [None; 8];
    let mut supporting = [None; 8];
    for (i, pad) in pads(p, c.footprint_radius).into_iter().enumerate() {
        let start = [pad[0], pad[1], req.player_origin[2] + c.support_up];
        let end = [pad[0], pad[1], req.player_origin[2] - c.support_down];
        if !finite(start) || !finite(end) {
            continue;
        }
        let Some(h) = b.support(start, end) else {
            continue;
        };
        if !finite(h.point)
            || !finite(h.normal)
            || (h.point[0] - pad[0]).abs() > tol
            || (h.point[1] - pad[1]).abs() > tol
            || h.point[2] >= start[2] - tol
            || h.point[2] < end[2] - tol
        {
            continue;
        }
        let point = [pad[0], pad[1], h.point[2]];
        observed[i] = Some(point);
        let Some(n) = unit(h.normal) else {
            continue;
        };
        if n[2] <= 0.0 || n[2] + 1e-12 < c.max_slope_radians.cos() {
            continue;
        }
        supporting[i] = Some(point);
    }
    Samples {
        observed,
        supporting,
    }
}
fn fit(points: [Option<Vec3>; 8], preview: Pose, c: Config) -> Option<Pose> {
    let mut count = 0.0;
    let mut mean = [0.0; 3];
    for p in points.iter().flatten() {
        count += 1.0;
        mean = add(mean, sub(*p, preview.origin));
    }
    if count < 3.0 {
        return None;
    }
    mean = mul(mean, 1.0 / count);
    let (mut xx, mut xy, mut yy, mut xz, mut yz) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for p in points.iter().flatten() {
        let v = sub(sub(*p, preview.origin), mean);
        xx += v[0] * v[0];
        xy += v[0] * v[1];
        yy += v[1] * v[1];
        xz += v[0] * v[2];
        yz += v[1] * v[2];
    }
    let det = xx * yy - xy * xy;
    if !det.is_finite() || det <= 1e-12 * (xx + yy) * (xx + yy) {
        return None;
    }
    let a = (xz * yy - yz * xy) / det;
    let slope_b = (yz * xx - xz * xy) / det;
    let up = unit([-a, -slope_b, 1.0])?;
    if up[2] + 1e-12 < c.max_slope_radians.cos() {
        return None;
    }
    let forward = unit(sub(preview.axis[0], mul(up, dot(preview.axis[0], up))))?;
    let left = unit(cross(up, forward))?;
    let mut elevation = f64::NEG_INFINITY;
    for p in points.iter().flatten() {
        let v = sub(*p, preview.origin);
        elevation = elevation.max(v[2] - a * v[0] - slope_b * v[1]);
    }
    let pose = Pose {
        origin: add(preview.origin, [0.0, 0.0, elevation]),
        axis: [forward, left, up],
    };
    if finite(pose.origin) && pose.axis.iter().all(|v| finite(*v)) {
        Some(pose)
    } else {
        None
    }
}
fn verified(p: Pose, points: Samples, c: Config) -> bool {
    let actual = pads(p, c.footprint_radius);
    let tol = tolerance(c);
    let mut indices = [0usize; 8];
    let mut n = 0;
    for (i, pad) in actual.iter().enumerate() {
        if let Some(hit) = points.observed[i] {
            let gap = pad[2] - hit[2];
            if !gap.is_finite() || gap < -tol {
                return false;
            }
            if points.supporting[i].is_some() && gap <= c.max_foot_gap + tol {
                indices[n] = i;
                n += 1;
            }
        }
    }
    if n < 3 {
        return false;
    }
    let load = add(
        p.origin,
        mul(p.axis[2], c.body_bottom * 0.5 + c.body_top * 0.5),
    );
    if !finite(load) {
        return false;
    }
    for j in 0..n {
        let a = sub(actual[indices[j]], p.origin);
        let b = sub(actual[indices[(j + 1) % n]], p.origin);
        let q = sub(load, p.origin);
        let edge = [b[0] - a[0], b[1] - a[1]];
        let side = edge[0] * (q[1] - a[1]) - edge[1] * (q[0] - a[0]);
        let len = edge[0].hypot(edge[1]);
        if !side.is_finite() || side <= tol * len {
            return false;
        }
    }
    true
}
pub fn solve<B: Backend>(req: Request, c: Config, backend: &mut B) -> Placement {
    let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let mut preview = Pose {
        origin: if finite(req.player_origin) {
            req.player_origin
        } else {
            [0.0; 3]
        },
        axis: identity,
    };
    let fail = |pose| Placement { valid: false, pose };
    if !finite(req.player_origin) || !finite(req.eye) || !finite(req.forward) || !config_valid(c) {
        return fail(preview);
    }
    let Some(yaw) = unit([req.forward[0], req.forward[1], 0.0]) else {
        return fail(preview);
    };
    preview.origin = add(req.player_origin, mul(yaw, c.reach));
    preview.axis = [yaw, [-yaw[1], yaw[0], 0.0], [0.0, 0.0, 1.0]];
    if !finite(preview.origin) {
        return fail(Pose {
            origin: req.player_origin,
            axis: identity,
        });
    }
    let mut points = sample(backend, preview, req, c);
    for _ in 0..4 {
        let Some(pose) = fit(points.supporting, preview, c) else {
            return fail(preview);
        };
        let final_points = sample(backend, pose, req, c);
        if verified(pose, final_points, c) {
            let feet = pads(pose, c.footprint_radius);
            let centers = feet.map(|p| add(p, mul(pose.axis[2], c.frame_radius + c.clearance)));
            let bottom = add(pose.origin, mul(pose.axis[2], c.body_bottom));
            let top = add(pose.origin, mul(pose.axis[2], c.body_top));
            if !finite(bottom) || !finite(top) || !centers.iter().all(|p| finite(*p)) {
                return fail(pose);
            }
            let mut clear = true;
            for i in 0..8 {
                clear &= backend.sphere_clear(centers[i], centers[(i + 1) % 8], c.frame_radius);
            }
            clear &= backend.sphere_clear(bottom, top, c.body_radius);
            if !clear {
                return fail(pose);
            }
            let midpoint = add(
                pose.origin,
                mul(pose.axis[2], c.body_bottom * 0.5 + c.body_top * 0.5),
            );
            if !finite(midpoint) {
                return fail(pose);
            }
            return Placement {
                valid: backend.sphere_clear(req.eye, midpoint, 0.0),
                pose,
            };
        }
        points = final_points;
    }
    fail(preview)
}
