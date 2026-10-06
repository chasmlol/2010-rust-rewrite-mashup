use crate::random::{FxRandomChannel, sample_f32};

pub const FX_ELEM_SPAWN_RELATIVE: i32 = 0x2;

pub const FX_ELEM_SPAWN_OFFSET_MASK: i32 = 0x30;

pub const FX_ELEM_SPAWN_OFFSET_SPHERE: i32 = 0x10;

pub const FX_ELEM_SPAWN_OFFSET_CYLINDER: i32 = 0x20;

pub const FX_TWO_PI: f64 = 6.283_185_482_025_146_5;

#[inline]
pub fn sample_float_range(base: f32, amplitude: f32, rand01: f32) -> f32 {
    base + amplitude * rand01
}

#[inline]
pub fn sample_spawn_origin_offset(spawn_origin: [[f32; 2]; 3], seed: u64) -> [f32; 3] {
    [
        sample_float_range(
            spawn_origin[0][0],
            spawn_origin[0][1],
            sample_f32(seed, FxRandomChannel::SpawnOriginX),
        ),
        sample_float_range(
            spawn_origin[1][0],
            spawn_origin[1][1],
            sample_f32(seed, FxRandomChannel::SpawnOriginY),
        ),
        sample_float_range(
            spawn_origin[2][0],
            spawn_origin[2][1],
            sample_f32(seed, FxRandomChannel::SpawnOriginZ),
        ),
    ]
}

#[inline]
pub fn random_dir(seed: u64) -> [f32; 3] {
    let height = 2.0 * sample_f32(seed, FxRandomChannel::SpawnHeight) - 1.0;
    let horiz = libm::sqrtf((1.0 - height * height).max(0.0));
    let yaw = sample_f32(seed, FxRandomChannel::SpawnYaw) * (FX_TWO_PI as f32);
    let (sin_yaw, cos_yaw) = (libm::sinf(yaw), libm::cosf(yaw));
    [horiz * cos_yaw, horiz * sin_yaw, height]
}

#[inline]
pub fn apply_spawn_origin(
    effect_origin: [f32; 3],
    axis: [[f32; 3]; 3],
    spawn_origin: [[f32; 2]; 3],
    seed: u64,
    spawn_relative: bool,
) -> [f32; 3] {
    let local = sample_spawn_origin_offset(spawn_origin, seed);
    if spawn_relative {
        [
            effect_origin[0]
                + local[0] * axis[0][0]
                + local[1] * axis[1][0]
                + local[2] * axis[2][0],
            effect_origin[1]
                + local[0] * axis[0][1]
                + local[1] * axis[1][1]
                + local[2] * axis[2][1],
            effect_origin[2]
                + local[0] * axis[0][2]
                + local[1] * axis[1][2]
                + local[2] * axis[2][2],
        ]
    } else {
        [
            effect_origin[0] + local[0],
            effect_origin[1] + local[1],
            effect_origin[2] + local[2],
        ]
    }
}

#[inline]
pub fn offset_spawn_origin(
    origin: &mut [f32; 3],
    axis: [[f32; 3]; 3],
    flags: i32,
    radius_base: f32,
    radius_amp: f32,
    height_base: f32,
    height_amp: f32,
    seed: u64,
) {
    let Some(mode) = elem_spawn_offset_mode(flags) else {
        return;
    };
    match mode {
        FxSpawnOffsetMode::None => {}
        FxSpawnOffsetMode::Sphere => {
            let dir = random_dir(seed);
            let radius = sample_float_range(
                radius_base,
                radius_amp,
                sample_f32(seed, FxRandomChannel::SpawnRadius),
            );
            origin[0] += radius * dir[0];
            origin[1] += radius * dir[1];
            origin[2] += radius * dir[2];
        }
        FxSpawnOffsetMode::Cylinder => {
            let radius = sample_float_range(
                radius_base,
                radius_amp,
                sample_f32(seed, FxRandomChannel::SpawnRadius),
            );
            let yaw = sample_f32(seed, FxRandomChannel::SpawnYaw) * (FX_TWO_PI as f32);
            let (sin_yaw, cos_yaw) = (libm::sinf(yaw), libm::cosf(yaw));
            let rx = radius * cos_yaw;
            let ry = radius * sin_yaw;

            origin[0] += rx * axis[1][0] + ry * axis[2][0];
            origin[1] += rx * axis[1][1] + ry * axis[2][1];
            origin[2] += rx * axis[1][2] + ry * axis[2][2];
            let height = sample_float_range(
                height_base,
                height_amp,
                sample_f32(seed, FxRandomChannel::SpawnHeight),
            );
            origin[0] += height * axis[0][0];
            origin[1] += height * axis[0][1];
            origin[2] += height * axis[0][2];
        }
    }
}

#[inline]
pub fn spawn_origin_world(
    effect_origin: [f32; 3],
    axis: [[f32; 3]; 3],
    spawn_origin: [[f32; 2]; 3],
    flags: i32,
    radius_base: f32,
    radius_amp: f32,
    height_base: f32,
    height_amp: f32,
    seed: u64,
) -> [f32; 3] {
    let mut origin = apply_spawn_origin(
        effect_origin,
        axis,
        spawn_origin,
        seed,
        elem_spawn_relative(flags),
    );
    offset_spawn_origin(
        &mut origin,
        axis,
        flags,
        radius_base,
        radius_amp,
        height_base,
        height_amp,
        seed,
    );
    origin
}

#[inline]
pub fn world_delta_to_local(
    world: [f32; 3],
    effect_origin: [f32; 3],
    axis: [[f32; 3]; 3],
) -> [f32; 3] {
    let dx = world[0] - effect_origin[0];
    let dy = world[1] - effect_origin[1];
    let dz = world[2] - effect_origin[2];
    [
        dx * axis[0][0] + dy * axis[0][1] + dz * axis[0][2],
        dx * axis[1][0] + dy * axis[1][1] + dz * axis[1][2],
        dx * axis[2][0] + dy * axis[2][1] + dz * axis[2][2],
    ]
}

#[inline]
pub fn elem_spawn_offset_mode(flags: i32) -> Option<FxSpawnOffsetMode> {
    match flags & FX_ELEM_SPAWN_OFFSET_MASK {
        0 => Some(FxSpawnOffsetMode::None),
        FX_ELEM_SPAWN_OFFSET_SPHERE => Some(FxSpawnOffsetMode::Sphere),
        FX_ELEM_SPAWN_OFFSET_CYLINDER => Some(FxSpawnOffsetMode::Cylinder),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxSpawnOffsetMode {
    None,
    Sphere,
    Cylinder,
}

#[inline]
pub const fn elem_spawn_relative(flags: i32) -> bool {
    (flags & FX_ELEM_SPAWN_RELATIVE) != 0
}
