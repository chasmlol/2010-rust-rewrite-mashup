#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
    pub fn dot(self, b: Self) -> f64 {
        self.x * b.x + self.y * b.y + self.z * b.z
    }
    pub fn cross(self, b: Self) -> Self {
        Self::new(
            self.y * b.z - self.z * b.y,
            self.z * b.x - self.x * b.z,
            self.x * b.y - self.y * b.x,
        )
    }
    pub fn length_squared(self) -> f64 {
        self.dot(self)
    }
    pub fn length(self) -> f64 {
        let scale = self.x.abs().max(self.y.abs()).max(self.z.abs());
        if scale == 0.0 {
            0.0
        } else {
            scale * libm::sqrt((self / scale).length_squared())
        }
    }
    pub fn finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
    pub fn normalized(self) -> Option<Self> {
        if !self.finite() {
            return None;
        }
        let scale = self.x.abs().max(self.y.abs()).max(self.z.abs());
        if scale == 0.0 {
            return None;
        }
        let scaled = self / scale;
        Some(scaled / libm::sqrt(scaled.length_squared()))
    }
    pub fn component(self, i: usize) -> f64 {
        match i {
            0 => self.x,
            1 => self.y,
            _ => self.z,
        }
    }
}
impl From<[f64; 3]> for Vec3 {
    fn from(v: [f64; 3]) -> Self {
        Self::new(v[0], v[1], v[2])
    }
}
impl From<Vec3> for [f64; 3] {
    fn from(v: Vec3) -> Self {
        [v.x, v.y, v.z]
    }
}
impl core::ops::Add for Vec3 {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self::new(self.x + b.x, self.y + b.y, self.z + b.z)
    }
}
impl core::ops::Sub for Vec3 {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self::new(self.x - b.x, self.y - b.y, self.z - b.z)
    }
}
impl core::ops::Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}
impl core::ops::Mul<f64> for Vec3 {
    type Output = Self;
    fn mul(self, b: f64) -> Self {
        Self::new(self.x * b, self.y * b, self.z * b)
    }
}
impl core::ops::Div<f64> for Vec3 {
    type Output = Self;
    fn div(self, b: f64) -> Self {
        Self::new(self.x / b, self.y / b, self.z / b)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Capsule {
    pub a: Vec3,
    pub b: Vec3,
    pub radius: f64,
}
impl Capsule {
    pub fn valid(self) -> bool {
        self.a.finite() && self.b.finite() && self.radius.is_finite() && self.radius >= 0.0
    }
    pub fn translated(self, d: Vec3) -> Self {
        Self {
            a: self.a + d,
            b: self.b + d,
            ..self
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Plane {
    pub normal: Vec3,
    pub d: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct Triangle {
    pub a: Vec3,
    pub b: Vec3,
    pub c: Vec3,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Contact {
    pub normal: Vec3,
    pub depth: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ContactResult {
    Clear,
    Contact(Contact),
    Invalid,
    Unsupported,
}

pub(crate) fn vec_cmp(a: Vec3, b: Vec3) -> core::cmp::Ordering {
    a.x.total_cmp(&b.x)
        .then(a.y.total_cmp(&b.y))
        .then(a.z.total_cmp(&b.z))
}
fn better(a: Contact, b: Contact) -> bool {
    a.depth
        .total_cmp(&b.depth)
        .then(vec_cmp(a.normal, b.normal))
        .is_lt()
}
fn finite_contact(c: Contact) -> ContactResult {
    if c.normal.finite() && c.depth.is_finite() && c.depth >= 0.0 {
        ContactResult::Contact(c)
    } else {
        ContactResult::Invalid
    }
}

pub fn capsule_plane_hull<I: IntoIterator<Item = Plane>>(
    moving: Capsule,
    planes: I,
) -> ContactResult {
    if !moving.valid() {
        return ContactResult::Invalid;
    }
    let mut count = 0usize;
    let mut excluded = false;
    let mut best: Option<Contact> = None;
    for p in planes {
        count += 1;
        if !p.d.is_finite() || !p.normal.finite() {
            return ContactResult::Invalid;
        }
        let scale = p.normal.x.abs().max(p.normal.y.abs()).max(p.normal.z.abs());
        if scale == 0.0 {
            return ContactResult::Invalid;
        }
        let scaled = p.normal / scale;
        let len = scaled.length();
        let normal = scaled / len;
        let offset = (p.d / scale) / len;
        let separation = normal.dot(moving.a).min(normal.dot(moving.b)) - offset - moving.radius;
        if !separation.is_finite() {
            return ContactResult::Invalid;
        }
        if separation > 0.0 {
            excluded = true;
        } else {
            let contact = Contact {
                normal,
                depth: (-separation).max(0.0),
            };
            if best.is_none_or(|b| better(contact, b)) {
                best = Some(contact);
            }
        }
    }
    if count == 0 {
        ContactResult::Invalid
    } else if excluded {
        ContactResult::Clear
    } else {
        finite_contact(best.unwrap())
    }
}

fn closest_segments(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> Option<(Vec3, Vec3)> {
    let u = b - a;
    let v = d - c;
    let w = a - c;
    let aa = u.dot(u);
    let bb = u.dot(v);
    let cc = v.dot(v);
    let dd = u.dot(w);
    let ee = v.dot(w);
    if ![aa, bb, cc, dd, ee].iter().all(|x| x.is_finite()) {
        return None;
    }
    let mut best = (a, c);
    let mut best_d = (a - c).length_squared();
    if !best_d.is_finite() {
        return None;
    }
    let mut consider = |s: f64, t: f64| {
        let p = a + u * s;
        let q = c + v * t;
        let dist = (p - q).length_squared();
        if dist.is_finite()
            && (dist < best_d
                || (dist == best_d && vec_cmp(p, best.0).then(vec_cmp(q, best.1)).is_lt()))
        {
            best = (p, q);
            best_d = dist;
        }
    };
    let clamp = |x: f64| x.clamp(0.0, 1.0);
    if cc > 0.0 {
        consider(0.0, clamp(ee / cc));
        consider(1.0, clamp((ee + bb) / cc));
    }
    if aa > 0.0 {
        consider(clamp(-dd / aa), 0.0);
        consider(clamp((bb - dd) / aa), 1.0);
    }
    let determinant = u.cross(v).length_squared();
    if determinant > 0.0 && determinant.is_finite() {
        let cross = u.cross(v);
        let s = v.cross(w).dot(cross) / determinant;
        let t = u.cross(w).dot(cross) / determinant;
        if s.is_finite() && t.is_finite() && (0.0..=1.0).contains(&s) && (0.0..=1.0).contains(&t) {
            consider(s, t);
        }
    }
    Some(best)
}

pub fn capsule_capsule(moving: Capsule, fixed: Capsule) -> ContactResult {
    if !moving.valid() || !fixed.valid() {
        return ContactResult::Invalid;
    }
    let radius = moving.radius + fixed.radius;
    if !radius.is_finite() {
        return ContactResult::Invalid;
    }
    let (p, q) = match closest_segments(moving.a, moving.b, fixed.a, fixed.b) {
        Some(v) => v,
        None => return ContactResult::Invalid,
    };
    let distance = (p - q).length();
    if !distance.is_finite() {
        return ContactResult::Invalid;
    }
    if distance >= radius {
        return ContactResult::Clear;
    }
    let axis_scale = (moving.b - moving.a)
        .length()
        .max((fixed.b - fixed.a).length())
        .max(1.0);
    if !axis_scale.is_finite() {
        return ContactResult::Invalid;
    }
    if distance > 1e-12 * axis_scale {
        return finite_contact(Contact {
            normal: (p - q) / distance,
            depth: radius - distance,
        });
    }
    let u = moving.b - moving.a;
    let v = fixed.b - fixed.a;
    let axes = [
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        u,
        v,
        u.cross(v),
        u.cross(Vec3::new(1.0, 0.0, 0.0)),
        u.cross(Vec3::new(0.0, 1.0, 0.0)),
        v.cross(Vec3::new(1.0, 0.0, 0.0)),
        v.cross(Vec3::new(0.0, 1.0, 0.0)),
    ];
    let mut best: Option<Contact> = None;
    for axis in axes {
        if let Some(n) = axis.normalized() {
            for normal in [n, -n] {
                let depth = normal.dot(fixed.a).max(normal.dot(fixed.b))
                    - normal.dot(moving.a).min(normal.dot(moving.b))
                    + radius;
                let c = Contact {
                    normal,
                    depth: depth.max(0.0),
                };
                if !depth.is_finite() {
                    return ContactResult::Invalid;
                }
                if best.is_none_or(|b| better(c, b)) {
                    best = Some(c);
                }
            }
        }
    }
    finite_contact(best.unwrap())
}

fn closest_point_segment(p: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let v = b - a;
    let vv = v.dot(v);
    if vv == 0.0 {
        a
    } else {
        a + v * ((p - a).dot(v) / vv).clamp(0.0, 1.0)
    }
}
fn inside_triangle(p: Vec3, t: Triangle, n: Vec3) -> bool {
    let s0 = (t.b - t.a).cross(p - t.a).dot(n);
    let s1 = (t.c - t.b).cross(p - t.b).dot(n);
    let s2 = (t.a - t.c).cross(p - t.c).dot(n);
    s0 >= 0.0 && s1 >= 0.0 && s2 >= 0.0
}
fn closest_point_triangle(p: Vec3, t: Triangle, n: Vec3) -> Vec3 {
    let projection = p - n * (p - t.a).dot(n);
    if inside_triangle(projection, t, n) {
        return projection;
    }
    let mut best = t.a;
    let mut dist = (p - best).length_squared();
    for (a, b) in [(t.a, t.b), (t.b, t.c), (t.c, t.a)] {
        let q = closest_point_segment(p, a, b);
        let d = (p - q).length_squared();
        if d < dist || (d == dist && vec_cmp(q, best).is_lt()) {
            best = q;
            dist = d;
        }
    }
    best
}

pub fn capsule_triangle(moving: Capsule, t: Triangle) -> ContactResult {
    if !moving.valid() || !t.a.finite() || !t.b.finite() || !t.c.finite() {
        return ContactResult::Invalid;
    }
    let ab = t.b - t.a;
    let ac = t.c - t.a;
    let cross = ab.cross(ac);
    if !ab.finite() || !ac.finite() || !cross.finite() {
        return ContactResult::Invalid;
    }
    let n = match cross.normalized() {
        Some(n) => n,
        None => return ContactResult::Unsupported,
    };
    let mut best = (moving.a, closest_point_triangle(moving.a, t, n));
    let mut dist = (best.0 - best.1).length_squared();
    if !dist.is_finite() {
        return ContactResult::Invalid;
    }
    let mut consider = |p: Vec3, q: Vec3| {
        let d = (p - q).length_squared();
        if d < dist || (d == dist && vec_cmp(p, best.0).then(vec_cmp(q, best.1)).is_lt()) {
            best = (p, q);
            dist = d;
        }
    };
    consider(moving.b, closest_point_triangle(moving.b, t, n));
    for (a, b) in [(t.a, t.b), (t.b, t.c), (t.c, t.a)] {
        match closest_segments(moving.a, moving.b, a, b) {
            Some((p, q)) => consider(p, q),
            None => return ContactResult::Invalid,
        }
    }
    let da = (moving.a - t.a).dot(n);
    let db = (moving.b - t.a).dot(n);
    if !da.is_finite() || !db.is_finite() {
        return ContactResult::Invalid;
    }
    if (da <= 0.0 && db >= 0.0) || (da >= 0.0 && db <= 0.0) {
        let denominator = da - db;
        if denominator != 0.0 {
            let p = moving.a + (moving.b - moving.a) * (da / denominator);
            if inside_triangle(p, t, n) {
                consider(p, p);
            }
        }
    }
    let distance = libm::sqrt(dist);
    if !distance.is_finite() {
        return ContactResult::Invalid;
    }
    if distance >= moving.radius {
        return ContactResult::Clear;
    }
    let scale = (moving.b - moving.a)
        .length()
        .max(ab.length())
        .max(ac.length())
        .max(1.0);
    if !scale.is_finite() {
        return ContactResult::Invalid;
    }
    if distance > 1e-12 * scale {
        return finite_contact(Contact {
            normal: (best.0 - best.1) / distance,
            depth: moving.radius - distance,
        });
    }
    let positive = Contact {
        normal: n,
        depth: (moving.radius - da.min(db)).max(0.0),
    };
    let negative = Contact {
        normal: -n,
        depth: (moving.radius + da.max(db)).max(0.0),
    };
    finite_contact(if better(positive, negative) {
        positive
    } else {
        negative
    })
}
