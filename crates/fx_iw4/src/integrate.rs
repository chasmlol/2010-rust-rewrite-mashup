#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct FxElemVec3Range {
    pub base: [f32; 3],

    pub amplitude: [f32; 3],
}

impl FxElemVec3Range {
    fn sampled(self, seed: u64) -> [f32; 3] {
        core::array::from_fn(|axis| {
            self.base[axis]
                + self.amplitude[axis]
                    * crate::random::sample_f32(
                        seed,
                        [
                            crate::random::FxRandomChannel::VelocityX,
                            crate::random::FxRandomChannel::VelocityY,
                            crate::random::FxRandomChannel::VelocityZ,
                        ][axis],
                    )
        })
    }
}

pub fn integrate_velocity_graph(
    samples: &[FxElemVec3Range],
    start_age01: f32,
    end_age01: f32,
    life_ms: f32,
    seed: u64,
) -> [f32; 3] {
    if samples.len() < 2 || life_ms <= 0.0 {
        return [0.0, 0.0, 0.0];
    }
    let start = clamp01(start_age01);
    let end = clamp01(end_age01).max(start);
    if end <= start {
        return [0.0, 0.0, 0.0];
    }
    let segment_count = samples.len() - 1;

    let segment_span_ms = life_ms * segment_count as f32;
    let mut delta = [0.0f32; 3];
    let first = (libm::floorf(start * segment_count as f32) as usize).min(segment_count - 1);
    let end_excl =
        (libm::ceilf(end * segment_count as f32) as usize).clamp(first + 1, segment_count);
    for segment in first..end_excl {
        let seg_start = segment as f32 / segment_count as f32;
        let seg_end = (segment + 1) as f32 / segment_count as f32;
        let lo = start.max(seg_start);
        let hi = end.min(seg_end);
        if hi <= lo {
            continue;
        }
        let v0 = sample_lerp(samples, lo, seed);
        let v1 = sample_lerp(samples, hi, seed);
        let w = 0.5 * (hi - lo) * segment_span_ms;
        delta[0] += (v0[0] + v1[0]) * w;
        delta[1] += (v0[1] + v1[1]) * w;
        delta[2] += (v0[2] + v1[2]) * w;
    }
    delta
}

#[inline]
pub fn sample_vel_graph_at_age(samples: &[FxElemVec3Range], age01: f32, seed: u64) -> [f32; 3] {
    sample_lerp(samples, age01, seed)
}

fn clamp01(x: f32) -> f32 {
    if x < 0.0 {
        0.0
    } else if x > 1.0 {
        1.0
    } else {
        x
    }
}

fn sample_lerp(samples: &[FxElemVec3Range], age01: f32, seed: u64) -> [f32; 3] {
    let last = samples.len() - 1;
    let t = clamp01(age01);
    let seg = t * last as f32;
    let i = (libm::floorf(seg) as usize).min(last);
    let j = (i + 1).min(last);
    let f = if i == j { 0.0 } else { seg - i as f32 };
    let a = samples[i].sampled(seed);
    let b = samples[j].sampled(seed);
    [
        a[0] + (b[0] - a[0]) * f,
        a[1] + (b[1] - a[1]) * f,
        a[2] + (b[2] - a[2]) * f,
    ]
}

pub const FX_SPARKCLOUD_HISTORY_NEAR_MS: f64 = 500.0;

pub const FX_SPARKCLOUD_HISTORY_FAR_MS: f64 = 1000.0;

#[inline]
pub fn sparkcloud_history_lookback_ms(size1: f32, far: bool) -> f32 {
    let k = if far {
        FX_SPARKCLOUD_HISTORY_FAR_MS
    } else {
        FX_SPARKCLOUD_HISTORY_NEAR_MS
    };
    size1 * (k as f32)
}

#[inline]
pub const fn particle_cloud_cell_count(flags: i32) -> usize {
    1024usize >> (((flags as u32) >> 29) & 3)
}
