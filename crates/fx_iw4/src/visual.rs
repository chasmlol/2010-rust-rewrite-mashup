use crate::origin::sample_float_range;
use crate::random::{FxRandomChannel, sample_f32};

pub const FX_ELEM_VIS_STATE_SAMPLE_SIZE: usize = 0x30;

pub const FX_ELEM_VISUAL_STATE_SIZE: usize = 0x18;

pub const FX_VIS_COLOR_OFF: usize = 0x00;

pub const FX_VIS_ROT_DELTA_OFF: usize = 0x04;

pub const FX_VIS_ROT_TOTAL_OFF: usize = 0x08;

pub const FX_VIS_SIZE0_OFF: usize = 0x0c;

pub const FX_VIS_SIZE1_OFF: usize = 0x10;

pub const FX_VIS_SCALE_OFF: usize = 0x14;

#[inline]
pub fn elem_norm_time(age_msec: i32, life_msec: i32) -> f32 {
    if life_msec <= 0 {
        return 1.0;
    }
    let t = age_msec as f32 / life_msec as f32;
    if t <= 0.0 {
        0.0
    } else if t >= 1.0 {
        1.0
    } else {
        t
    }
}

#[inline]
pub fn setup_visual_sample_point(interval_count: u8, norm_time: f32) -> (usize, f32) {
    let sample_point = (interval_count as f32) * norm_time;
    let floor = libm::floorf(sample_point).max(0.0) as usize;
    let frac = sample_point - floor as f32;
    (floor, frac)
}

fn sample_pair(samples: &[u8], floor: usize) -> Option<(&[u8], &[u8])> {
    let off0 = floor.checked_mul(FX_ELEM_VIS_STATE_SAMPLE_SIZE)?;
    let off1 = off0.checked_add(FX_ELEM_VIS_STATE_SAMPLE_SIZE)?;

    let need = off1.checked_add(FX_VIS_SCALE_OFF)?.checked_add(4)?;
    if samples.len() < need {
        return None;
    }
    let s0 = &samples[off0..off0 + FX_ELEM_VIS_STATE_SAMPLE_SIZE];
    let s1_end = (off1 + FX_ELEM_VIS_STATE_SAMPLE_SIZE).min(samples.len());
    let s1 = &samples[off1..s1_end];
    Some((s0, s1))
}

fn read_f32(bytes: &[u8], off: usize) -> Option<f32> {
    let b: [u8; 4] = bytes.get(off..off + 4)?.try_into().ok()?;
    Some(f32::from_le_bytes(b))
}

#[inline]
pub fn integrate_rotation_from_zero(
    s0: &[u8],
    s1: &[u8],
    seed: u64,
    sample_lerp: f32,
    life_msec: f32,
) -> f32 {
    let r = sample_f32(seed, FxRandomChannel::RotationDelta);
    let half_sq = sample_lerp * sample_lerp * 0.5;
    let base_delta0 = read_f32(s0, FX_VIS_ROT_DELTA_OFF).unwrap_or(0.0);
    let amp_delta0 = read_f32(s0, FX_ELEM_VISUAL_STATE_SIZE + FX_VIS_ROT_DELTA_OFF).unwrap_or(0.0);
    let base_total0 = read_f32(s0, FX_VIS_ROT_TOTAL_OFF).unwrap_or(0.0);
    let amp_total0 = read_f32(s0, FX_ELEM_VISUAL_STATE_SIZE + FX_VIS_ROT_TOTAL_OFF).unwrap_or(0.0);
    let base_delta1 = read_f32(s1, FX_VIS_ROT_DELTA_OFF).unwrap_or(0.0);
    let amp_delta1 = read_f32(s1, FX_ELEM_VISUAL_STATE_SIZE + FX_VIS_ROT_DELTA_OFF).unwrap_or(0.0);
    let term_next = (base_delta1 + amp_delta1 * r) * half_sq;
    let term_total = amp_total0 * r + base_total0;
    let term_delta = (sample_lerp - half_sq) * (base_delta0 + r * amp_delta0);
    (term_next + term_total + term_delta) * life_msec
}

#[inline]
pub fn evaluate_rotation_total(
    samples: &[u8],
    interval_count: u8,
    norm_time: f32,
    seed: u64,
    initial_rotation: [f32; 2],
    life_msec: f32,
) -> Option<f32> {
    let (floor, frac) = setup_visual_sample_point(interval_count, norm_time);
    let (s0, s1) = sample_pair(samples, floor)?;
    let initial = sample_float_range(
        initial_rotation[0],
        initial_rotation[1],
        sample_f32(seed, FxRandomChannel::InitialRotation),
    );
    Some(initial + integrate_rotation_from_zero(s0, s1, seed, frac, life_msec))
}

#[inline]
pub fn evaluate_size0(
    samples: &[u8],
    interval_count: u8,
    norm_time: f32,
    rand01: f32,
) -> Option<f32> {
    evaluate_size_channel(samples, interval_count, norm_time, rand01, FX_VIS_SIZE0_OFF)
}

#[inline]
pub fn evaluate_size1(
    samples: &[u8],
    interval_count: u8,
    norm_time: f32,
    rand01: f32,
) -> Option<f32> {
    evaluate_size_channel(samples, interval_count, norm_time, rand01, FX_VIS_SIZE1_OFF)
}

#[inline]
pub fn evaluate_scale(
    samples: &[u8],
    interval_count: u8,
    norm_time: f32,
    rand01: f32,
) -> Option<f32> {
    evaluate_size_channel(samples, interval_count, norm_time, rand01, FX_VIS_SCALE_OFF)
}

fn evaluate_size_channel(
    samples: &[u8],
    interval_count: u8,
    norm_time: f32,
    rand01: f32,
    off: usize,
) -> Option<f32> {
    let (floor, frac) = setup_visual_sample_point(interval_count, norm_time);
    let (s0, s1) = sample_pair(samples, floor)?;
    let base0 = read_f32(s0, off)?;
    let amp0 = read_f32(s0, FX_ELEM_VISUAL_STATE_SIZE + off).unwrap_or(0.0);
    let base1 = read_f32(s1, off)?;
    let amp1 = read_f32(s1, FX_ELEM_VISUAL_STATE_SIZE + off).unwrap_or(0.0);
    let v0 = base0 + amp0 * rand01;
    let v1 = base1 + amp1 * rand01;
    Some(v0 * (1.0 - frac) + v1 * frac)
}

#[inline]
pub fn evaluate_color_bgra(
    samples: &[u8],
    interval_count: u8,
    norm_time: f32,
    rand01: f32,
) -> Option<[u8; 4]> {
    let (floor, frac) = setup_visual_sample_point(interval_count, norm_time);
    let (s0, s1) = sample_pair(samples, floor)?;
    let mut out = [0u8; 4];
    for c in 0..4 {
        let base0 = *s0.get(FX_VIS_COLOR_OFF + c)? as f32;
        let amp0 = *s0
            .get(FX_ELEM_VISUAL_STATE_SIZE + FX_VIS_COLOR_OFF + c)
            .unwrap_or(&0) as f32;
        let base1 = *s1.get(FX_VIS_COLOR_OFF + c).unwrap_or(&0) as f32;
        let amp1 = *s1
            .get(FX_ELEM_VISUAL_STATE_SIZE + FX_VIS_COLOR_OFF + c)
            .unwrap_or(&0) as f32;
        let v0 = base0 * (1.0 - rand01) + amp0 * rand01;
        let v1 = base1 * (1.0 - rand01) + amp1 * rand01;
        let v = v0 * (1.0 - frac) + v1 * frac;
        out[c] = libm::roundf(v).clamp(0.0, 255.0) as u8;
    }
    Some(out)
}

#[inline]
pub fn evaluate_vis_alpha(
    samples: &[u8],
    interval_count: u8,
    norm_time: f32,
    rand01: f32,
) -> Option<u8> {
    Some(evaluate_color_bgra(samples, interval_count, norm_time, rand01)?[3])
}
