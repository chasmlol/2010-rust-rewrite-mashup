use crate::response::RecoilAxis;

pub const CAMERA_PEAK_SECONDS: f64 = 0.080;
pub const MODEL_PEAK_SECONDS: f64 = 0.060;
pub const CAMERA_LIMIT_DEGREES: f64 = 8.0;
pub const CAMERA_ASSET_AMPLITUDE: f32 = 0.020;
pub const MODEL_ASSET_AMPLITUDE: f32 = 0.015;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewKickRange {
    pub pitch_min: f32,
    pub pitch_max: f32,
    pub yaw_min: f32,
    pub yaw_max: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GunKickRange {
    pub pitch_min: f32,
    pub pitch_max: f32,
    pub yaw_min: f32,
    pub yaw_max: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GunRecoilPlacementState {
    pub axes: [RecoilAxis; 2],
    pub angles: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GunRecoilResponse {
    pub peak_seconds: f64,
}
impl Default for GunRecoilResponse {
    fn default() -> Self {
        Self {
            peak_seconds: MODEL_PEAK_SECONDS,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FireRecoilImpulse {
    pub camera_peak: [f32; 3],
    pub model_peak: [f32; 2],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FireRecoilPsScales {
    pub reduce_window_active: bool,
    pub reduced_percent: f32,
    pub weap_flags: u32,
    pub recoil_scale: i32,
}

pub fn recoil_ads_weight(frac: f32) -> f32 {
    if frac.is_finite() {
        frac.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub fn fire_recoil_view_range(frac: f32, hip: ViewKickRange, ads: ViewKickRange) -> ViewKickRange {
    let t = recoil_ads_weight(frac);
    ViewKickRange {
        pitch_min: blend(hip.pitch_min, ads.pitch_min, t),
        pitch_max: blend(hip.pitch_max, ads.pitch_max, t),
        yaw_min: blend(hip.yaw_min, ads.yaw_min, t),
        yaw_max: blend(hip.yaw_max, ads.yaw_max, t),
    }
}
pub fn fire_recoil_gun_range(frac: f32, hip: GunKickRange, ads: GunKickRange) -> GunKickRange {
    let t = recoil_ads_weight(frac);
    GunKickRange {
        pitch_min: blend(hip.pitch_min, ads.pitch_min, t),
        pitch_max: blend(hip.pitch_max, ads.pitch_max, t),
        yaw_min: blend(hip.yaw_min, ads.yaw_min, t),
        yaw_max: blend(hip.yaw_max, ads.yaw_max, t),
    }
}

pub fn weapon_fire_recoil(
    view: ViewKickRange,
    gun: GunKickRange,
    ps: FireRecoilPsScales,
    samples: [f32; 4],
) -> FireRecoilImpulse {
    let mut gain = if ps.reduce_window_active {
        finite(ps.reduced_percent).max(0.0) * 0.01
    } else {
        1.0
    };
    if ps.weap_flags & playerstate_iw4::weap_flags::RECOIL_SCALE != 0 {
        gain *= (ps.recoil_scale as f32).max(0.0) * 0.01;
    }
    let pitch_gain = if ps.weap_flags & playerstate_iw4::weap_flags::DOUBLEBARREL_RECOIL != 0 {
        2.0
    } else {
        1.0
    };
    let sample = |lo, hi, u| blend(lo, hi, recoil_ads_weight(u));
    FireRecoilImpulse {
        camera_peak: [
            -finite(
                sample(view.pitch_min, view.pitch_max, samples[0])
                    * gain
                    * pitch_gain
                    * CAMERA_ASSET_AMPLITUDE,
            ),
            finite(sample(view.yaw_min, view.yaw_max, samples[1]) * gain * CAMERA_ASSET_AMPLITUDE),
            0.0,
        ],
        model_peak: [
            finite(
                sample(gun.pitch_min, gun.pitch_max, samples[2])
                    * gain
                    * pitch_gain
                    * MODEL_ASSET_AMPLITUDE,
            ),
            finite(sample(gun.yaw_min, gun.yaw_max, samples[3]) * gain * MODEL_ASSET_AMPLITUDE),
        ],
    }
}

pub fn gun_recoil_angle_contribution(state: GunRecoilPlacementState) -> [f32; 3] {
    [state.angles[0], state.angles[1], 0.0]
}

pub fn calculate_weapon_position_gun_recoil(
    state: &mut GunRecoilPlacementState,
    dt: f32,
    frac: f32,
    enabled: bool,
    hip: GunRecoilResponse,
    ads: GunRecoilResponse,
    limits: [f32; 2],
) {
    if !enabled || !dt.is_finite() || dt <= 0.0 {
        return;
    }
    let t = f64::from(recoil_ads_weight(frac));
    let peak = hip.peak_seconds * (1.0 - t) + ads.peak_seconds * t;
    for (axis, limit) in limits.into_iter().enumerate() {
        state.axes[axis].advance(f64::from(dt), peak);
        state.angles[axis] = state.axes[axis].bounded(f64::from(finite(limit).abs())) as f32;
    }
}

pub fn start_firing_restrict_kick_time(
    frac: f32,
    ads_bullets: i32,
    hip_bullets: i32,
    fire_ms: i32,
    delay_ms: i32,
) -> i32 {
    let bullets = if frac == 1.0 {
        ads_bullets
    } else {
        hip_bullets
    };
    bullets.saturating_mul(fire_ms).saturating_add(delay_ms)
}

fn finite(x: f32) -> f32 {
    if x.is_finite() { x } else { 0.0 }
}
fn blend(a: f32, b: f32, t: f32) -> f32 {
    finite(a) * (1.0 - t) + finite(b) * t
}
