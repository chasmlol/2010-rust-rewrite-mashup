use crate::response::SwayConfig;

pub const SWAY_HALF_LIFE_SECONDS: f64 = 0.050;
pub const SWAY_REFERENCE_RATE: f64 = 180.0;
pub const SWAY_ASSET_GAIN: f32 = 0.25;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponSwayParams {
    pub max_angle: f32,
    pub pitch_scale: f32,
    pub yaw_scale: f32,
    pub horiz_scale: f32,
    pub vert_scale: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SwaySpringState {
    pub horiz: f32,
    pub vert: f32,
    pub pitch: f32,
    pub yaw: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SwayContribution {
    pub origin: [f32; 3],
    pub angles: [f32; 3],
}

pub fn sway_config() -> SwayConfig {
    SwayConfig {
        half_life_seconds: SWAY_HALF_LIFE_SECONDS,
        reference_rate: [SWAY_REFERENCE_RATE; 2],
    }
}

pub fn lerp_sway_params(
    hip: WeaponSwayParams,
    ads: WeaponSwayParams,
    frac: f32,
) -> WeaponSwayParams {
    let t = crate::recoil_ads_weight(frac);
    let blend = |a: f32, b: f32| a * (1.0 - t) + b * t;
    WeaponSwayParams {
        max_angle: blend(hip.max_angle, ads.max_angle),
        pitch_scale: blend(hip.pitch_scale, ads.pitch_scale),
        yaw_scale: blend(hip.yaw_scale, ads.yaw_scale),
        horiz_scale: blend(hip.horiz_scale, ads.horiz_scale),
        vert_scale: blend(hip.vert_scale, ads.vert_scale),
    }
}

pub fn sway_from_drive(
    drive: [f64; 2],
    params: WeaponSwayParams,
    landing_scale: f32,
) -> SwaySpringState {
    let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
    let limit = finite(params.max_angle).abs() * SWAY_ASSET_GAIN;
    let land = finite(landing_scale).clamp(0.0, 4.0);
    let gain = |rate: f64, scale: f32| finite(rate as f32 * limit * finite(scale) * land);
    SwaySpringState {
        horiz: gain(drive[1], params.horiz_scale),
        vert: gain(drive[0], params.vert_scale),
        pitch: gain(drive[0], params.pitch_scale),
        yaw: gain(drive[1], params.yaw_scale),
    }
}

pub fn sway_shellshock_landing_scale(remaining_ms: i32, duration_ms: i32, scale: f32) -> f32 {
    if remaining_ms <= 0 || duration_ms <= 0 || !scale.is_finite() {
        return 1.0;
    }
    let weight = (remaining_ms as f32 / duration_ms as f32).clamp(0.0, 1.0);
    1.0 + (scale.clamp(0.0, 4.0) - 1.0) * weight
}
pub fn sway_contribution(s: SwaySpringState) -> SwayContribution {
    SwayContribution {
        origin: [0.0, -s.horiz, s.vert],
        angles: [-s.pitch, -s.yaw, 0.0],
    }
}
