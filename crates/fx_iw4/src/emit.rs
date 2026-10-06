use crate::random::{FxRandomChannel, sample_f32};

pub const FX_EMIT_RESIDUAL_TO_DIST: f64 = 0.003_906_25;

pub const FX_EMIT_DIST_TO_RESIDUAL: f64 = 256.0;

pub const FX_EMIT_CRT_RAND_SCALE: f64 = 3.051_757_812_5e-5;

pub const FX_EMIT_RESIDUAL_ROUND_BIAS: f64 = 9.313_225_746_154_785e-10;

pub const FX_ELEM_EMIT_ORIENT_AXIS: i32 = 0x8000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxEmitSpawn {
    pub lerp: f32,
    pub msec_at_spawn: i32,
}

pub const FX_EMIT_SPAWN_CAP: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxEmitSchedule {
    pub new_residual: u8,
    pub spawn_count: usize,
    pub spawns: [FxEmitSpawn; FX_EMIT_SPAWN_CAP],
}

impl FxEmitSchedule {
    #[inline]
    pub fn spawns(&self) -> &[FxEmitSpawn] {
        &self.spawns[..self.spawn_count]
    }
}

#[inline]
pub fn emit_dist_range(
    emit_dist_base: f32,
    emit_dist_amp: f32,
    emit_var_base: f32,
    emit_var_amp: f32,
    elem_random_seed: u64,
) -> (f32, f32) {
    let r = sample_f32(elem_random_seed, FxRandomChannel::EmitDistance);
    let base = emit_dist_base + emit_dist_amp * r + emit_var_base;
    let max = base + emit_var_amp;
    (base, max)
}

#[inline]
pub fn emit_pack_residual(residual_dist: f32, max_dist_per_emit: f32) -> u8 {
    if max_dist_per_emit <= 0.0 {
        return 0;
    }
    let v = (residual_dist as f64) * FX_EMIT_DIST_TO_RESIDUAL / (max_dist_per_emit as f64)
        + FX_EMIT_RESIDUAL_ROUND_BIAS;
    let rounded = libm::round(v);
    if rounded <= 0.0 {
        0
    } else if rounded >= 255.0 {
        255
    } else {
        rounded as u8
    }
}

#[inline]
pub fn emit_unpack_residual_start(emit_residual: u8, max_dist_per_emit: f32) -> f32 {
    -((emit_residual as f32) * max_dist_per_emit * (FX_EMIT_RESIDUAL_TO_DIST as f32))
}

pub fn process_emitting_schedule(
    emit_residual: u8,
    origin_begin: [f32; 3],
    origin_end: [f32; 3],
    msec_update_begin: i32,
    msec_update_end: i32,
    base_dist_per_emit: f32,
    max_dist_per_emit: f32,
    mut next_spacing01: impl FnMut() -> f32,
) -> FxEmitSchedule {
    let mut out = FxEmitSchedule {
        new_residual: emit_residual,
        spawn_count: 0,
        spawns: [FxEmitSpawn {
            lerp: 0.0,
            msec_at_spawn: 0,
        }; FX_EMIT_SPAWN_CAP],
    };
    let delta = [
        origin_end[0] - origin_begin[0],
        origin_end[1] - origin_begin[1],
        origin_end[2] - origin_begin[2],
    ];
    let dist_sq = delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2];
    if dist_sq <= 1e-12 || max_dist_per_emit <= 0.0 {
        return out;
    }
    let dist_in_update = libm::sqrtf(dist_sq);
    let mut dist_next = emit_unpack_residual_start(emit_residual, max_dist_per_emit);
    let mut dist_last;
    let msec_span = (msec_update_end.saturating_sub(msec_update_begin)).max(0) as f32;
    let var_amp = (max_dist_per_emit - base_dist_per_emit).max(0.0);

    loop {
        dist_last = dist_next;
        let u01 = next_spacing01();
        dist_next = u01 * var_amp + base_dist_per_emit + dist_next;
        if dist_next > dist_in_update {
            break;
        }
        let clamped = if dist_next < 0.0 { 0.0 } else { dist_next };
        dist_next = clamped;
        let lerp = clamped / dist_in_update;
        let msec_at_spawn = msec_update_begin.wrapping_add(libm::floorf(msec_span * lerp) as i32);
        if out.spawn_count >= FX_EMIT_SPAWN_CAP {
            break;
        }
        out.spawns[out.spawn_count] = FxEmitSpawn {
            lerp,
            msec_at_spawn,
        };
        out.spawn_count += 1;
    }

    let residual_dist = dist_in_update - dist_last;
    out.new_residual = emit_pack_residual(residual_dist, max_dist_per_emit);
    out
}

#[inline]
pub fn emit_lerp_origin(begin: [f32; 3], end: [f32; 3], lerp: f32) -> [f32; 3] {
    [
        begin[0] + (end[0] - begin[0]) * lerp,
        begin[1] + (end[1] - begin[1]) * lerp,
        begin[2] + (end[2] - begin[2]) * lerp,
    ]
}
