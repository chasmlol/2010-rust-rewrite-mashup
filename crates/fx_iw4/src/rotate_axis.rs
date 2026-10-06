use crate::random::{FxRandomChannel, sample_f32};

pub const FX_RAND_ROT_DEGREES: f64 = 360.0;

pub const FX_DEG_TO_RAD: f64 = 0.017_453_292_384_743_69;

pub const FX_RAD_TO_DEG: f64 = 57.295_776_367_187_5;

#[inline]
pub fn runner_rand_rot_degrees(random_seed: u64) -> f32 {
    (f64::from(sample_f32(random_seed, FxRandomChannel::InitialRotation)) * FX_RAND_ROT_DEGREES)
        as f32
}

#[inline]
pub fn rotate_point_around_vector(dir: [f32; 3], point: [f32; 3], degrees: f32) -> [f32; 3] {
    let rad = (f64::from(degrees) * FX_DEG_TO_RAD) as f32;
    let c = libm::cosf(rad);
    let s = libm::sinf(rad);
    let t = 1.0 - c;
    let x = dir[0];
    let y = dir[1];
    let z = dir[2];
    let r00 = t * x * x + c;
    let r01 = t * y * x - s * z;
    let r02 = y * s + t * x * z;
    let r10 = s * z + t * y * x;
    let r11 = c + y * y * t;
    let r12 = t * y * z - s * x;
    let r20 = t * x * z - y * s;
    let r21 = t * y * z + s * x;
    let r22 = t * z * z + c;
    [
        r00 * point[0] + r01 * point[1] + r02 * point[2],
        r10 * point[0] + r11 * point[1] + r12 * point[2],
        r20 * point[0] + r21 * point[1] + r22 * point[2],
    ]
}

#[inline]
pub fn impact_mark_axis(axis_in: [[f32; 3]; 3], orientation_radians: f32) -> [[f32; 3]; 3] {
    let forward = axis_in[0];
    let rotated = rotate_point_around_vector(
        forward,
        axis_in[1],
        (f64::from(orientation_radians) * FX_RAD_TO_DEG) as f32,
    );
    let tex_coord_axis = [
        forward[1] * rotated[2] - forward[2] * rotated[1],
        forward[2] * rotated[0] - forward[0] * rotated[2],
        forward[0] * rotated[1] - forward[1] * rotated[0],
    ];
    [forward, tex_coord_axis, rotated]
}

#[inline]
pub fn randomly_rotate_axis(axis_in: [[f32; 3]; 3], random_seed: u64) -> [[f32; 3]; 3] {
    let degrees = runner_rand_rot_degrees(random_seed);
    let forward = axis_in[0];
    let right = rotate_point_around_vector(forward, axis_in[1], degrees);
    let up = [
        forward[1] * right[2] - forward[2] * right[1],
        forward[2] * right[0] - forward[0] * right[2],
        forward[0] * right[1] - forward[1] * right[0],
    ];
    [forward, right, up]
}
