use weapon_iw4::{
    LookSway, SwaySpringState, WeaponSwayParams, lerp_sway_params, sway_config, sway_from_drive,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewSwayState {
    look: LookSway,
    contribution: SwaySpringState,
}
impl ViewSwayState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn springs(&self) -> SwaySpringState {
        self.contribution
    }
    pub fn advance(
        &mut self,
        hip: WeaponSwayParams,
        ads: WeaponSwayParams,
        angles: [f32; 3],
        frac: f32,
        ads_enabled: bool,
        overlay: bool,
        landing_scale: f32,
        dt: f32,
    ) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let config = sway_config();
        let sample = [f64::from(angles[0]), f64::from(angles[1])];
        if overlay && frac > 0.0 {
            if self.look.observe_only(sample, f64::from(dt), config) {
                self.contribution = SwaySpringState::default();
            }
            return;
        }
        if !self.look.sample(sample, f64::from(dt), config) {
            return;
        }
        let params = if ads_enabled {
            lerp_sway_params(hip, ads, frac)
        } else {
            hip
        };
        self.contribution = sway_from_drive(self.look.drive(config), params, landing_scale);
    }
}
