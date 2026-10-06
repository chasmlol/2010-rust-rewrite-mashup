pub(crate) type Vector = [f32; 3];
#[derive(Clone, Copy)]
pub(crate) struct Motion {
    pub(crate) origin: Vector,
    pub(crate) velocity: Vector,
}
#[derive(Clone, Copy)]
pub(crate) struct Contact {
    pub(crate) fraction: f32,
    pub(crate) end: Vector,
    pub(crate) normal: Vector,
    pub(crate) blocked: bool,
    pub(crate) walkable: bool,
}
#[derive(Clone, Copy)]
pub(crate) struct Settings {
    pub(crate) dt: f32,
    pub(crate) gravity: Option<f32>,
    pub(crate) ground: Option<Vector>,
    pub(crate) step_height: f32,
    pub(crate) snap_down: f32,
    pub(crate) landing_normal_z: f32,
}
const BUDGET: usize = 12;
// A projected target can round inside a contact plane and stop the trace.
// Keep the retry just outside that plane.
const SKIN: f64 = 1. / 32.;
fn finite(v: Vector) -> bool {
    v.iter().all(|x| x.is_finite())
}
fn dot(a: Vector, b: Vector) -> f64 {
    (0..3).map(|i| a[i] as f64 * b[i] as f64).sum()
}
fn normal(v: Vector) -> Option<Vector> {
    if !finite(v) {
        return None;
    }
    let len = libm::sqrt(dot(v, v));
    if len < 1e-12 {
        None
    } else {
        Some(v.map(|x| (x as f64 / len) as f32))
    }
}
fn clean(mut m: Motion) -> Motion {
    if !finite(m.origin) {
        m.origin = [0.; 3];
    }
    if !finite(m.velocity) {
        m.velocity = [0.; 3];
    }
    m
}
fn project(v: Vector, ns: &[Vector]) -> Vector {
    if !finite(v) {
        return [0.; 3];
    }
    let feasible = |x: [f64; 3]| {
        ns.iter().all(|n| {
            let value = (0..3).map(|j| x[j] * n[j] as f64).sum::<f64>();
            let magnitude = (0..3).map(|j| (x[j] * n[j] as f64).abs()).sum::<f64>();
            value >= -1e-12 * magnitude
        })
    };
    let d = v.map(|x| x as f64);
    if float_feasible(v, ns) {
        return v;
    }
    if feasible(d) {
        return representable(d, v, ns);
    }
    let mut best = [0f64; 3];
    let mut cost = dot(v, v);
    for a in 0..ns.len() {
        consider(d, ns, &[a, a, a], 1, &feasible, &mut best, &mut cost);
        for b in a + 1..ns.len() {
            consider(d, ns, &[a, b, b], 2, &feasible, &mut best, &mut cost);
        }
    }
    representable(best, v, ns)
}
fn float_feasible(v: Vector, ns: &[Vector]) -> bool {
    finite(v)
        && ns
            .iter()
            .all(|n| dot(v, *n) >= 0. && v[0] * n[0] + v[1] * n[1] + v[2] * n[2] >= 0.)
}
fn representable(best: [f64; 3], desired: Vector, ns: &[Vector]) -> Vector {
    let rounded = best.map(|x| x as f32);
    if float_feasible(rounded, ns) && dot(rounded, rounded) <= dot(desired, desired) {
        return rounded;
    }
    let center = best.map(|x| (x * (1. - 8. * f32::EPSILON as f64)) as f32);
    if !finite(center) {
        return [0.; 3];
    }
    let adjacent: [[f32; 3]; 3] = core::array::from_fn(|i| {
        [
            libm::nextafterf(center[i], f32::NEG_INFINITY),
            center[i],
            libm::nextafterf(center[i], f32::INFINITY),
        ]
    });
    let mut result = [0.; 3];
    let mut distance = dot(desired, desired);
    for a in adjacent[0] {
        for b in adjacent[1] {
            for c in adjacent[2] {
                let candidate = [a, b, c];
                if !float_feasible(candidate, ns)
                    || dot(candidate, candidate) > dot(desired, desired)
                {
                    continue;
                }
                let delta = core::array::from_fn(|i| candidate[i] - desired[i]);
                let score = dot(delta, delta);
                if score < distance {
                    distance = score;
                    result = candidate;
                }
            }
        }
    }
    result
}
fn consider(
    d: [f64; 3],
    ns: &[Vector],
    ids: &[usize; 3],
    rank: usize,
    feasible: &impl Fn([f64; 3]) -> bool,
    best: &mut [f64; 3],
    cost: &mut f64,
) {
    if rank == 2 {
        let a = ns[ids[0]].map(|v| v as f64);
        let b = ns[ids[1]].map(|v| v as f64);
        let cross = |a: [f64; 3], b: [f64; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let product = |a: [f64; 3], b: [f64; 3]| (0..3).map(|k| a[k] * b[k]).sum::<f64>();
        let line = cross(a, b);
        let squared = product(line, line);
        if squared == 0. {
            return;
        }
        let scale = product(d, line) / squared;
        let x = line.map(|v| v * scale);
        let correction = core::array::from_fn(|k| x[k] - d[k]);
        let first = product(cross(correction, b), line) / squared;
        let second = product(cross(a, correction), line) / squared;
        if first < -1e-9
            || second < -1e-9
            || !first.is_finite()
            || !second.is_finite()
            || !feasible(x)
        {
            return;
        }
        let dist = (0..3).map(|k| (x[k] - d[k]) * (x[k] - d[k])).sum();
        if dist < *cost {
            *cost = dist;
            *best = x;
        }
        return;
    }
    let n = ns[ids[0]];
    let lambda = -(0..3).map(|k| n[k] as f64 * d[k]).sum::<f64>() / dot(n, n);
    if lambda < 0. || !lambda.is_finite() {
        return;
    }
    let x = core::array::from_fn(|k| d[k] + lambda * n[k] as f64);
    if !feasible(x) {
        return;
    }
    let dist = (0..3).map(|k| (x[k] - d[k]) * (x[k] - d[k])).sum();
    if dist < *cost {
        *cost = dist;
        *best = x;
    }
}
fn sweep(
    start: Vector,
    end: Vector,
    trace: &impl Fn(Vector, Vector) -> Contact,
) -> Option<(Contact, Vector)> {
    if !finite(start) || !finite(end) {
        return None;
    }
    let hit = trace(start, end);
    if hit.blocked
        || !hit.fraction.is_finite()
        || hit.fraction < 0.
        || hit.fraction > 1.
        || !finite(hit.end)
    {
        return None;
    }
    let p = core::array::from_fn(|i| {
        (start[i] as f64 + (end[i] as f64 - start[i] as f64) * hit.fraction as f64) as f32
    });
    if finite(p) { Some((hit, p)) } else { None }
}
pub(crate) fn slide(
    m: Motion,
    s: Settings,
    trace: &impl Fn(Vector, Vector) -> Contact,
) -> (Motion, bool) {
    let mut m = clean(m);
    if !s.dt.is_finite() || s.dt <= 0. {
        return (m, false);
    }
    let g = s.gravity.filter(|x| x.is_finite()).unwrap_or(0.);
    let initial = m.velocity;
    let mut ns = [[0.; 3]; BUDGET + 1];
    let mut count = 0;
    if let Some(n) = s.ground.filter(|n| normal(*n).is_some()) {
        ns[count] = n;
        count += 1;
    }
    let mut elapsed = 0f64;
    let total = s.dt as f64;
    let mut obstructed = false;
    let mut lifted = false;
    for _ in 0..BUDGET {
        let remaining = total - elapsed;
        if remaining <= 0. {
            return (m, obstructed);
        }
        let desired = |t: f64| {
            [
                initial[0],
                initial[1],
                (initial[2] as f64 - g as f64 * t) as f32,
            ]
        };
        let wanted = desired(elapsed + remaining * 0.5);
        let average = project(wanted, &ns[..count]);
        let mut target: [f64; 3] =
            core::array::from_fn(|i| m.origin[i] as f64 + average[i] as f64 * remaining);
        if lifted {
            for n in ns[..count].iter().filter(|n| dot(wanted, **n) < 0.) {
                (0..3).for_each(|i| target[i] += n[i] as f64 * SKIN);
            }
        }
        let target = target.map(|x| x as f32);
        let Some((hit, p)) = sweep(m.origin, target, trace) else {
            m.velocity = [0.; 3];
            return (m, true);
        };
        m.origin = p;
        elapsed += remaining * hit.fraction as f64;
        if hit.fraction == 1. {
            m.velocity = project(desired(total), &ns[..count]);
            return (m, obstructed);
        }
        obstructed = true;
        if normal(hit.normal).is_none() {
            m.velocity = [0.; 3];
            return (m, true);
        }
        let n = hit.normal;
        let duplicate = ns[..count].contains(&n);
        if !duplicate {
            ns[count] = n;
            count += 1;
        }
        m.velocity = project(desired(elapsed), &ns[..count]);
        if hit.fraction == 0. && duplicate {
            if !lifted {
                lifted = true;
                continue;
            }
            return (m, true);
        }
    }
    (m, obstructed)
}
fn land(
    m: Motion,
    distance: f32,
    s: Settings,
    trace: &impl Fn(Vector, Vector) -> Contact,
) -> Option<Motion> {
    if !distance.is_finite() || distance <= 0. {
        return None;
    }
    let mut target = m.origin;
    target[2] -= distance;
    let (hit, p) = sweep(m.origin, target, trace)?;
    if hit.fraction == 1. {
        return None;
    }
    let n = normal(hit.normal)?;
    let threshold = if s.landing_normal_z.is_finite() {
        s.landing_normal_z
    } else {
        1.
    };
    if n[2] <= 0. || !(hit.walkable || n[2] >= threshold) {
        return None;
    }
    Some(Motion {
        origin: p,
        velocity: project(m.velocity, &[hit.normal]),
    })
}
pub(crate) fn traverse(
    m: Motion,
    s: Settings,
    trace: &impl Fn(Vector, Vector) -> Contact,
) -> Motion {
    let m = clean(m);
    if !s.dt.is_finite() || s.dt <= 0. {
        return m;
    }
    let all_solid = core::cell::Cell::new(false);
    let baseline_trace = |a, b| {
        let hit = trace(a, b);
        if hit.blocked {
            all_solid.set(true);
        }
        hit
    };
    let (mut baseline, obstructed) = slide(m, s, &baseline_trace);
    if all_solid.get() {
        return baseline;
    }
    if s.ground.and_then(normal).is_some()
        && let Some(snapped) = land(baseline, s.snap_down, s, trace)
    {
        baseline = snapped;
    }
    if !obstructed
        || !s.dt.is_finite()
        || s.dt <= 0.
        || !s.step_height.is_finite()
        || s.step_height <= 0.
    {
        return baseline;
    }
    let horizontal = libm::sqrt(dot(
        [m.velocity[0], m.velocity[1], 0.],
        [m.velocity[0], m.velocity[1], 0.],
    ));
    if horizontal <= 1e-12 {
        return baseline;
    }
    let mut up = m.origin;
    up[2] += s.step_height;
    let Some((_, raised)) = sweep(m.origin, up, trace) else {
        return baseline;
    };
    let rise = raised[2] - m.origin[2];
    if rise <= 0. {
        return baseline;
    }
    let elevated = Motion {
        origin: raised,
        velocity: m.velocity,
    };
    let rejected = core::cell::Cell::new(false);
    let checked_trace = |a, b| {
        let hit = trace(a, b);
        if hit.blocked || (hit.fraction < 1. && normal(hit.normal).is_none_or(|n| n[2] < -0.001)) {
            rejected.set(true);
        }
        hit
    };
    let mut airborne = s;
    airborne.ground = None;
    let (advanced, _) = slide(elevated, airborne, &checked_trace);
    if rejected.get() {
        return baseline;
    }
    let search = rise
        + if s.snap_down.is_finite() {
            s.snap_down.max(0.)
        } else {
            0.
        };
    let Some(landed) = land(advanced, search, s, trace) else {
        return baseline;
    };
    if landed.origin[2] - m.origin[2] > s.step_height {
        return baseline;
    }
    let progress = |p: Vector| {
        ((p[0] as f64 - m.origin[0] as f64) * m.velocity[0] as f64
            + (p[1] as f64 - m.origin[1] as f64) * m.velocity[1] as f64)
            / horizontal
    };
    if progress(landed.origin) - progress(baseline.origin) > 0.001 {
        landed
    } else {
        baseline
    }
}
pub(crate) fn project_ground(v: &mut Vector, n: &Vector) {
    if !finite(*v) || !finite(*n) || n[2] < 0.001 {
        return;
    }
    let h = libm::sqrt(v[0] as f64 * v[0] as f64 + v[1] as f64 * v[1] as f64);
    if h <= 1e-12 {
        return;
    }
    let speed = libm::sqrt(dot(*v, *v));
    let z = -(v[0] as f64 * n[0] as f64 + v[1] as f64 * n[1] as f64) / n[2] as f64;
    let scale = speed / libm::sqrt(h * h + z * z);
    let out = [
        (v[0] as f64 * scale) as f32,
        (v[1] as f64 * scale) as f32,
        (z * scale) as f32,
    ];
    if finite(out) {
        *v = out;
    }
}
