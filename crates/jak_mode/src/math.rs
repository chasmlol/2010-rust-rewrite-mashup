//! Jak's units and the vector, angle and rotation helpers its movement code
//! is written in.
//!
//! Lengths are in Jak units (4096 per meter), time in ticks (300 per second,
//! five per 60 Hz frame) and angles in rotation units (65536 per turn), so a
//! constant reads the same here as in the game it comes from.
use glam::{Mat3, Quat, Vec3};

pub const METER: f32 = 4096.0;
pub const TICKS_PER_SECOND: i64 = 300;
/// One simulation frame: the game's movement runs one control iteration per
/// 60 Hz frame.
pub const FRAME_TICKS: i64 = 5;
pub const SECONDS_PER_FRAME: f32 = 1.0 / 60.0;
pub const TURN: f32 = 65536.0;

pub const fn meters(m: f32) -> f32 {
    m * METER
}

/// A duration in ticks, truncated the way the game's constants are.
pub const fn seconds(s: f32) -> i64 {
    (s * TICKS_PER_SECOND as f32) as i64
}

pub const fn degrees(d: f32) -> f32 {
    d * (TURN / 360.0)
}

pub fn to_radians(a: f32) -> f32 {
    a * (std::f32::consts::TAU / TURN)
}

pub fn from_radians(r: f32) -> f32 {
    r * (TURN / std::f32::consts::TAU)
}

pub fn sin(a: f32) -> f32 {
    to_radians(a).sin()
}

pub fn cos(a: f32) -> f32 {
    to_radians(a).cos()
}

pub fn atan(y: f32, x: f32) -> f32 {
    from_radians(y.atan2(x))
}

pub fn acos(c: f32) -> f32 {
    from_radians(c.clamp(-1.0, 1.0).acos())
}

/// `b - a`, wrapped into one signed half turn.
pub fn deg_diff(a: f32, b: f32) -> f32 {
    f32::from((b as i64).wrapping_sub(a as i64) as i16)
}

/// `a - b`, wrapped into one signed half turn.
pub fn deg_sub(a: f32, b: f32) -> f32 {
    f32::from((a as i64).wrapping_sub(b as i64) as i16)
}

/// An angle wrapped into one signed half turn.
pub fn wrap_angle(a: f32) -> f32 {
    f32::from(a as i64 as i16)
}

pub fn seek(x: f32, target: f32, step: f32) -> f32 {
    let d = target - x;
    if step >= d.abs() {
        target
    } else if d >= 0.0 {
        x + step
    } else {
        x - step
    }
}

/// `x` remapped from `[x0, x1]` to `[a, b]`, clamped to the ends.
pub fn lerp_scale(a: f32, b: f32, x: f32, x0: f32, x1: f32) -> f32 {
    let span = x1 - x0;
    let t = if span == 0.0 {
        if x >= x0 { 1.0 } else { 0.0 }
    } else {
        ((x - x0) / span).clamp(0.0, 1.0)
    };
    (1.0 - t) * a + t * b
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// A stick or trigger byte mapped to `[-out, out]` with a dead zone around
/// `center` and full travel at `max`.
pub fn analog_input(value: i32, offset: f32, center: f32, max: f32, out: f32) -> f32 {
    let v = value as f32 - offset;
    let past = v.abs() - center;
    let range = max - center;
    let out = if v < 0.0 { -out } else { out };
    if past <= 0.0 {
        0.0
    } else if past >= range {
        out
    } else {
        past * out / range
    }
}

pub fn normalize(v: Vec3, length: f32) -> Vec3 {
    v.normalize_or_zero() * length
}

/// `v` with its component along unit `n` removed.
pub fn flatten(v: Vec3, n: Vec3) -> Vec3 {
    v - n * n.dot(v)
}

/// The component of `v` along unit `n` replaced by `n` itself: what a
/// collision leaves of a velocity that ran into a surface.
pub fn reflect_flat(v: Vec3, n: Vec3) -> Vec3 {
    flatten(v, n) + n
}

/// `v` mirrored in the plane with unit normal `n`.
pub fn reflect(v: Vec3, n: Vec3) -> Vec3 {
    2.0 * flatten(v, n) - v
}

/// [`reflect_flat`] that keeps the velocity's heading across the ground: the
/// part into the surface is removed by moving along `gravity_up` rather than
/// along `n`, so riding onto a slope bends the path up the slope instead of
/// sideways.
pub fn reflect_flat_gravity(v: Vec3, n: Vec3, gravity_up: Vec3) -> Vec3 {
    let flat = reflect_flat(v, n);
    let length = v.length();
    let along = gravity_up.dot(n);
    if 0.0001 < along && 8192.0 < length {
        let into = v.normalize_or_zero().dot(n);
        let out = v + gravity_up * (length * -(into / along)) + n;
        normalize(out, flat.length())
    } else {
        flat
    }
}

pub fn xz_length(v: Vec3) -> f32 {
    (v.x * v.x + v.z * v.z).sqrt()
}

/// `v` with its xz part scaled to `length`; y is kept.
pub fn xz_normalize(v: Vec3, length: f32) -> Vec3 {
    let l = xz_length(v);
    if l == 0.0 {
        v
    } else {
        let k = length / l;
        Vec3::new(v.x * k, v.y, v.z * k)
    }
}

/// The yaw of `v` (0 along +z, a quarter turn along +x).
pub fn y_angle(v: Vec3) -> f32 {
    atan(v.x, v.z)
}

pub fn rotate_y(v: Vec3, a: f32) -> Vec3 {
    Quat::from_rotation_y(to_radians(a)) * v
}

/// Splits `v` along unit `up` and puts it back with the vertical part
/// replaced.
pub fn with_vertical(v: Vec3, up: Vec3, vertical: f32) -> Vec3 {
    flatten(v, up) + up * vertical
}

/// Moves `v` toward `target` by at most `step`.
pub fn vector_seek(v: Vec3, target: Vec3, step: f32) -> Vec3 {
    let d = target - v;
    let l = d.length();
    if l <= step {
        target
    } else {
        v + d * (step / l)
    }
}

pub fn x_axis(q: Quat) -> Vec3 {
    q * Vec3::X
}

pub fn y_axis(q: Quat) -> Vec3 {
    q * Vec3::Y
}

pub fn z_axis(q: Quat) -> Vec3 {
    q * Vec3::Z
}

pub fn quat_y_angle(q: Quat) -> f32 {
    y_angle(z_axis(q))
}

/// The yaw from `q`'s forward to `v`, each read across the xz plane.
pub fn quat_vector_y_angle(q: Quat, v: Vec3) -> f32 {
    deg_diff(quat_y_angle(q), y_angle(v))
}

/// A frame as its three axes: `r` (local +x), `u` (local +y, up) and `f`
/// (local +z, forward). `to_world` takes a local vector out, `to_local` back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Basis {
    pub r: Vec3,
    pub u: Vec3,
    pub f: Vec3,
}

impl Default for Basis {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Basis {
    pub const IDENTITY: Self = Self {
        r: Vec3::X,
        u: Vec3::Y,
        f: Vec3::Z,
    };

    /// Up exactly `up`, forward as close to `forward` as that allows: the
    /// pitch comes from `up`, not from `forward`.
    pub fn forward_up_nopitch(forward: Vec3, up: Vec3) -> Self {
        let u = up.normalize_or(Vec3::Y);
        let r = u.cross(forward).normalize_or_zero();
        let f = r.cross(u).normalize_or_zero();
        Self { r, u, f }
    }

    pub fn from_quat(q: Quat) -> Self {
        Self {
            r: x_axis(q),
            u: y_axis(q),
            f: z_axis(q),
        }
    }

    pub fn to_world(&self, v: Vec3) -> Vec3 {
        self.r * v.x + self.u * v.y + self.f * v.z
    }

    pub fn to_local(&self, v: Vec3) -> Vec3 {
        Vec3::new(self.r.dot(v), self.u.dot(v), self.f.dot(v))
    }

    pub fn quat(&self) -> Quat {
        Quat::from_mat3(&Mat3::from_cols(self.r, self.u, self.f)).normalize()
    }
}

pub fn forward_up_nopitch_quat(forward: Vec3, up: Vec3) -> Quat {
    Basis::forward_up_nopitch(forward, up).quat()
}

/// Rotation about `from × to` by `angle`, or about `fallback` when the two
/// are parallel and the cross product says nothing.
fn rotation_between(from: Vec3, to: Vec3, angle: f32, fallback: Vec3) -> Quat {
    let axis = from.cross(to);
    let axis = if axis.length_squared() > 1e-12 {
        axis.normalize()
    } else {
        fallback.normalize_or(Vec3::Y)
    };
    Quat::from_axis_angle(axis, to_radians(angle))
}

/// The rate limit every smoothed turn shares: at most `rate` per second, and
/// at most `5 / frames` of what is left, so the turn eases in at the end.
fn smooth_limit(angle: f32, rate: f32, frames: i32) -> f32 {
    let frames = if frames == 0 { 1e-11 } else { frames as f32 };
    let limit = (rate * SECONDS_PER_FRAME).min(5.0 * angle.abs() / frames);
    angle.min(limit).max(-limit)
}

/// One frame of turning unit `from` toward unit `to`.
pub fn smooth_rotation(from: Vec3, to: Vec3, rate: f32, frames: i32, fallback: Vec3) -> Quat {
    let angle = acos(from.dot(to));
    rotation_between(from, to, smooth_limit(angle, rate, frames), fallback)
}

/// [`smooth_rotation`] the other way round the circle.
pub fn smooth_rotation_long_way(
    from: Vec3,
    to: Vec3,
    rate: f32,
    frames: i32,
    fallback: Vec3,
) -> Quat {
    let angle = -acos(from.dot(to));
    rotation_between(from, to, smooth_limit(angle, rate, frames), fallback)
}

/// Unit `a` turned toward unit `b` by fraction `t` of the angle between them.
pub fn vector_deg_slerp(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    if t <= 0.0 {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    let (an, bn) = (a.normalize_or_zero(), b.normalize_or_zero());
    let c = an.dot(bn);
    if c.abs() > 0.9999 {
        return a;
    }
    let angle = t * acos(c);
    rotation_between(an, bn, angle, Vec3::Y) * a
}

/// Turns `q` toward `target`: its up axis toward the target's at `tilt_rate`,
/// then its heading about that up axis at `turn_rate`. `long_way` (signed)
/// asks for the heading turn through the far side when it points against the
/// short one.
pub fn rotate_toward_orientation(
    q: Quat,
    target: Quat,
    turn_rate: f32,
    tilt_rate: f32,
    turn_frames: i32,
    tilt_frames: i32,
    long_way: f32,
) -> Quat {
    let mut q = q;
    if tilt_rate > 0.0 {
        let tilt = smooth_rotation(y_axis(q), y_axis(target), tilt_rate, tilt_frames, x_axis(q));
        q = (tilt * q).normalize();
    }
    if turn_rate > 0.0 {
        let up = y_axis(q);
        let from = flatten(z_axis(q), up).normalize_or_zero();
        let to = flatten(z_axis(target), up).normalize_or_zero();
        let turn = if long_way != 0.0 && from.cross(to).dot(up) * long_way < 0.0 {
            smooth_rotation_long_way(from, to, turn_rate, turn_frames, up)
        } else {
            smooth_rotation(from, to, turn_rate, turn_frames, up)
        };
        q = (turn * q).normalize();
    }
    q
}
