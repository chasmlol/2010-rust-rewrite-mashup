use bevy::prelude::*;

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScreenEffectsView {
    pub ready: bool,
    pub default_killcam_view: bool,
    pub thermal_active: bool,
    pub thermal_scoped: bool,
    pub instant_thermal: bool,
    pub suppressed: bool,
    pub blend_ms: i32,
    pub flashed: bool,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScreenEffectsPublished;

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenEffectsDvars {
    pub draw_shellshock: bool,
    pub thermal_scope_ms: i32,
    pub thermal_no_scope_ms: i32,
}

impl Default for ScreenEffectsDvars {
    fn default() -> Self {
        Self {
            draw_shellshock: true,
            thermal_scope_ms: 250,
            thermal_no_scope_ms: 250,
        }
    }
}

impl ScreenEffectsDvars {
    pub fn apply(&mut self, name: &str, value: &str) -> bool {
        match name.to_ascii_lowercase().as_str() {
            "cg_drawshellshock" => self.draw_shellshock = decimal_prefix(value) != 0,
            "thermalblurfactorscope" | "thermalblurfactornoscope" => {
                let value = decimal_prefix(value);
                let slot = if name.eq_ignore_ascii_case("thermalBlurFactorScope") {
                    &mut self.thermal_scope_ms
                } else {
                    &mut self.thermal_no_scope_ms
                };
                *slot = value.clamp(0, 10000);
            }
            _ => return false,
        }
        true
    }
}

fn decimal_prefix(value: &str) -> i32 {
    let value = value.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let (negative, digits) = match value.as_bytes().first() {
        Some(b'-') => (true, &value[1..]),
        Some(b'+') => (false, &value[1..]),
        _ => (false, value),
    };
    let limit = if negative {
        i32::MAX as u32 + 1
    } else {
        i32::MAX as u32
    };
    let n = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0u32, |n, d| {
            n.saturating_mul(10)
                .saturating_add(u32::from(d - b'0'))
                .min(limit)
        });
    if negative {
        (-i64::from(n)) as i32
    } else {
        n as i32
    }
}
