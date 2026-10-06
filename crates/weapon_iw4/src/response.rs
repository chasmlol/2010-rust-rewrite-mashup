const LN_2: f64 = core::f64::consts::LN_2;

fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}
fn frequency(time: f64, numerator: f64) -> Option<f64> {
    if !positive(time) {
        return None;
    }
    let value = numerator / time;
    if positive(value) { Some(value) } else { None }
}

fn decayed_product(a: f64, b: f64, c: f64, z: f64) -> f64 {
    if a == 0.0 || b == 0.0 || c == 0.0 {
        return 0.0;
    }
    let product = a * b * c;
    let decay = libm::exp(-z);
    if product.is_finite() && product != 0.0 && decay > 0.0 {
        return product * decay;
    }
    let magnitude = libm::exp(libm::log(a.abs()) + libm::log(b.abs()) + libm::log(c.abs()) - z);
    if a.is_sign_negative() ^ b.is_sign_negative() ^ c.is_sign_negative() {
        -magnitude
    } else {
        magnitude
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RecoilAxis {
    x: f64,
    v: f64,
}
impl RecoilAxis {
    pub fn new(x: f64, v: f64) -> Option<Self> {
        if x.is_finite() && v.is_finite() {
            Some(Self { x, v })
        } else {
            None
        }
    }
    pub fn state(&self) -> (f64, f64) {
        (self.x, self.v)
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn add_peak(&mut self, amplitude: f64, peak_seconds: f64) -> bool {
        let Some(w) = frequency(peak_seconds, 1.0) else {
            return false;
        };
        if !amplitude.is_finite() {
            return false;
        }
        let impulse = amplitude * w * core::f64::consts::E;
        let next = self.v + impulse;
        if !impulse.is_finite() || !next.is_finite() {
            return false;
        }
        self.v = next;
        true
    }
    pub fn advance(&mut self, dt: f64, peak_seconds: f64) -> bool {
        if !positive(dt) {
            return false;
        }
        let Some(w) = frequency(peak_seconds, 1.0) else {
            return false;
        };
        let z = w * dt;
        if !z.is_finite() || z > 4096.0 {
            self.reset();
            return true;
        }
        let x = decayed_product(self.x, 1.0 + z, 1.0, z) + decayed_product(self.v, dt, 1.0, z);
        let v = decayed_product(self.v, 1.0 - z, 1.0, z) - decayed_product(self.x, w, z, z);
        if !x.is_finite() || !v.is_finite() {
            return false;
        }
        self.x = x;
        self.v = v;
        true
    }
    pub fn bounded(&self, limit: f64) -> f64 {
        if !limit.is_finite() || limit <= 0.0 {
            return 0.0;
        }
        limit * libm::tanh(self.x / limit)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwayConfig {
    pub half_life_seconds: f64,
    pub reference_rate: [f64; 2],
}
impl SwayConfig {
    fn lambda(self) -> Option<f64> {
        if !self.reference_rate.iter().all(|r| positive(*r)) {
            return None;
        }
        frequency(self.half_life_seconds, LN_2)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LookSway {
    previous: Option<[f64; 2]>,
    q: [f64; 2],
}
impl LookSway {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn rate(&self) -> [f64; 2] {
        self.q
    }
    pub fn previous(&self) -> Option<[f64; 2]> {
        self.previous
    }
    pub fn rebase(&mut self, angles: [f64; 2]) -> bool {
        if !angles.iter().all(|a| a.is_finite()) {
            return false;
        }
        self.previous = Some(angles.map(positive_degrees));
        true
    }
    pub fn sample(&mut self, angles: [f64; 2], dt: f64, config: SwayConfig) -> bool {
        self.update(angles, dt, config, false)
    }
    pub fn observe_only(&mut self, angles: [f64; 2], dt: f64, config: SwayConfig) -> bool {
        self.update(angles, dt, config, true)
    }
    fn update(&mut self, angles: [f64; 2], dt: f64, config: SwayConfig, suppressed: bool) -> bool {
        if !positive(dt) || !angles.iter().all(|a| a.is_finite()) {
            return false;
        }
        let Some(lambda) = config.lambda() else {
            return false;
        };
        let current = angles.map(positive_degrees);
        let Some(previous) = self.previous else {
            self.previous = Some(current);
            return true;
        };
        let velocity = if suppressed {
            [0.0; 2]
        } else {
            core::array::from_fn(|i| {
                let delta = positive_degrees(current[i] - previous[i] + 180.0) - 180.0;
                delta / dt
            })
        };
        if !velocity.iter().all(|v| v.is_finite()) {
            return false;
        }
        let z = lambda * dt;
        let decay = libm::exp(-z);
        let weight = -libm::expm1(-z);
        let next = core::array::from_fn(|i| self.q[i] * decay + velocity[i] * weight);
        if !next.iter().all(|q: &f64| q.is_finite()) {
            return false;
        }
        self.q = next;
        self.previous = Some(current);
        true
    }
    pub fn drive(&self, config: SwayConfig) -> [f64; 2] {
        if config.lambda().is_none() {
            return [0.0; 2];
        }
        core::array::from_fn(|i| libm::tanh(self.q[i] / config.reference_rate[i]))
    }
}

fn positive_degrees(angle: f64) -> f64 {
    let rem = angle % 360.0;
    if rem < 0.0 { rem + 360.0 } else { rem }
}
