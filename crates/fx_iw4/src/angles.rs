use crate::origin::sample_float_range;
use crate::random::{FxRandomChannel, sample_f32};

#[inline]
pub fn sample_elem_angles(
    spawn_angles: [[f32; 2]; 3],
    angular_velocity: [[f32; 2]; 3],
    seed: u64,
    age_msec: f32,
) -> [f32; 3] {
    let ch_spawn = [
        FxRandomChannel::SpawnPitch,
        FxRandomChannel::SpawnAngleYaw,
        FxRandomChannel::SpawnRoll,
    ];
    let ch_vel = [
        FxRandomChannel::AngularPitch,
        FxRandomChannel::AngularYaw,
        FxRandomChannel::AngularRoll,
    ];
    let mut out = [0.0f32; 3];
    for i in 0..3 {
        let spawn = sample_float_range(
            spawn_angles[i][0],
            spawn_angles[i][1],
            sample_f32(seed, ch_spawn[i]),
        );
        let vel = sample_float_range(
            angular_velocity[i][0],
            angular_velocity[i][1],
            sample_f32(seed, ch_vel[i]),
        );
        out[i] = spawn + age_msec * vel;
    }
    out
}

#[inline]
pub fn angles_to_axis_radians(angles: [f32; 3]) -> [[f32; 3]; 3] {
    let pitch = angles[0];
    let yaw = angles[1];
    let roll = angles[2];
    let cy = libm::cosf(yaw);
    let sy = libm::sinf(yaw);
    let cp = libm::cosf(pitch);
    let sp = libm::sinf(pitch);
    let cr = libm::cosf(roll);
    let sr = libm::sinf(roll);
    [
        [cy * cp, sy * cp, -sp],
        [sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, sr * cp],
        [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp],
    ]
}

#[inline]
pub fn mat3_mul(a: [[f32; 3]; 3], b: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut out = [[0.0f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    out
}

#[inline]
pub fn get_elem_angles_axis(
    spawn_angles: [[f32; 2]; 3],
    angular_velocity: [[f32; 2]; 3],
    seed: u64,
    age_msec: f32,
    effect_axis: [[f32; 3]; 3],
) -> [[f32; 3]; 3] {
    let angles = sample_elem_angles(spawn_angles, angular_velocity, seed, age_msec);
    let local = angles_to_axis_radians(angles);
    mat3_mul(local, effect_axis)
}
