use asset_game::WeaponKickFacts;
use weapon_iw4::{
    CAMERA_LIMIT_DEGREES, CAMERA_PEAK_SECONDS, FireRecoilPsScales, GunKickRange,
    GunRecoilPlacementState, GunRecoilResponse, MODEL_PEAK_SECONDS, RecoilAxis, ViewKickRange,
    calculate_weapon_position_gun_recoil, fire_recoil_gun_range, fire_recoil_view_range,
    recoil_ads_weight, weapon_fire_recoil,
};

pub type KickParams = WeaponKickFacts;

#[derive(Clone, Debug, PartialEq)]
pub struct ViewKickState {
    camera: [RecoilAxis; 3],
    pub kick_angles: [f32; 3],
    pub gun: GunRecoilPlacementState,
    rng: u32,
}
impl Default for ViewKickState {
    fn default() -> Self {
        Self {
            camera: Default::default(),
            kick_angles: [0.0; 3],
            gun: Default::default(),
            rng: 0xA341_316C,
        }
    }
}
impl ViewKickState {
    pub fn reset(&mut self) {
        let rng = self.rng;
        *self = Self::default();
        self.rng = rng;
    }
    pub fn seed_fire(
        &mut self,
        kick: &KickParams,
        frac: f32,
        weap_flags: u32,
        recoil_scale: i32,
        reduce_window_active: bool,
    ) {
        self.seed_fire_scaled(
            kick,
            frac,
            FireRecoilPsScales {
                reduce_window_active,
                reduced_percent: 0.0,
                weap_flags,
                recoil_scale,
            },
        );
    }
    pub fn seed_fire_scaled(&mut self, k: &KickParams, frac: f32, mut ps: FireRecoilPsScales) {
        let view = fire_recoil_view_range(
            frac,
            ViewKickRange {
                pitch_min: k.hip_view_kick_pitch_min,
                pitch_max: k.hip_view_kick_pitch_max,
                yaw_min: k.hip_view_kick_yaw_min,
                yaw_max: k.hip_view_kick_yaw_max,
            },
            ViewKickRange {
                pitch_min: k.ads_view_kick_pitch_min,
                pitch_max: k.ads_view_kick_pitch_max,
                yaw_min: k.ads_view_kick_yaw_min,
                yaw_max: k.ads_view_kick_yaw_max,
            },
        );
        let gun = fire_recoil_gun_range(
            frac,
            GunKickRange {
                pitch_min: k.hip_gun_kick_pitch_min,
                pitch_max: k.hip_gun_kick_pitch_max,
                yaw_min: k.hip_gun_kick_yaw_min,
                yaw_max: k.hip_gun_kick_yaw_max,
            },
            GunKickRange {
                pitch_min: k.ads_gun_kick_pitch_min,
                pitch_max: k.ads_gun_kick_pitch_max,
                yaw_min: k.ads_gun_kick_yaw_min,
                yaw_max: k.ads_gun_kick_yaw_max,
            },
        );
        if ps.reduce_window_active {
            let t = recoil_ads_weight(frac);
            ps.reduced_percent = k.hip_gun_kick_reduced_kick_percent * (1.0 - t)
                + k.ads_gun_kick_reduced_kick_percent * t;
        }
        let samples = [self.unit01(), self.unit01(), self.unit01(), self.unit01()];
        let impulse = weapon_fire_recoil(view, gun, ps, samples);
        for (axis, amplitude) in self.camera.iter_mut().zip(impulse.camera_peak) {
            axis.add_peak(f64::from(amplitude), CAMERA_PEAK_SECONDS);
        }
        for (axis, amplitude) in self.gun.axes.iter_mut().zip(impulse.model_peak) {
            axis.add_peak(f64::from(amplitude), MODEL_PEAK_SECONDS);
        }
    }
    pub fn advance(&mut self, kick: &KickParams, frac: f32, dt_secs: f32) {
        if !dt_secs.is_finite() || dt_secs <= 0.0 {
            return;
        }
        let dt = f64::from(dt_secs);
        for (axis, out) in self.camera.iter_mut().zip(&mut self.kick_angles) {
            axis.advance(dt, CAMERA_PEAK_SECONDS);
            *out = axis.bounded(CAMERA_LIMIT_DEGREES) as f32;
        }
        calculate_weapon_position_gun_recoil(
            &mut self.gun,
            dt as f32,
            frac,
            true,
            GunRecoilResponse::default(),
            GunRecoilResponse::default(),
            [kick.gun_max_pitch, kick.gun_max_yaw],
        );
    }
    fn unit01(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = if x == 0 { 1 } else { x };
        (x as f32) / (u32::MAX as f32)
    }
}

pub fn add_kick_to_viewangles(viewangles: [f32; 3], kick_angles: [f32; 3]) -> [f32; 3] {
    core::array::from_fn(|i| viewangles[i] + kick_angles[i])
}
