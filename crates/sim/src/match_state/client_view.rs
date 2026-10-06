#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptSeat {
    pub spectator_client: i32,
    pub kill_cam_entity: i32,
    pub look_at_entity: i32,
    pub archive_ms: i32,
    pub ps_offset_ms: i32,
    pub length_ms: i32,
}

impl Default for ScriptSeat {
    fn default() -> Self {
        Self {
            spectator_client: -1,
            kill_cam_entity: -1,
            look_at_entity: -1,
            archive_ms: 0,
            ps_offset_ms: 0,
            length_ms: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KillcamHud {
    pub final_kill: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LocationSelection {
    pub material: String,
    pub choose_direction: bool,
    pub radius: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuCommand {
    pub serial: u32,
    pub kind: MenuCommandKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuCommandKind {
    Open(String),
    ClosePopup,
    CloseInGame,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisionChange {
    pub name: String,
    pub duration_ms: i32,
    pub set_ms: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewEffects {
    pub naked_vision: Option<VisionChange>,
    pub thermal_vision: Option<VisionChange>,
    pub missile_vision: Option<VisionChange>,
    pub night_vision: Option<crate::VisionChange>,
    pub pain_vision: Option<crate::VisionChange>,
    pub depth_of_field: ScriptDepthOfField,
    pub blur: Option<ScriptBlur>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScriptDepthOfField {
    pub near_start: f32,
    pub near_end: f32,
    pub far_start: f32,
    pub far_end: f32,
    pub near_blur: f32,
    pub far_blur: f32,
}

impl ScriptDepthOfField {
    pub fn overrides_scene(&self) -> bool {
        self.near_start != 0.0
            || self.near_end != 0.0
            || self.far_start != 0.0
            || self.far_end != 0.0
    }
}

pub const MENU_COMMAND_TAIL: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RadarMode {
    #[default]
    Off,
    Normal,
    Fast,
    Constant,
}

impl RadarMode {
    pub fn wire_tag(self) -> u8 {
        self as u8
    }

    pub fn from_wire_tag(tag: u8) -> Option<Self> {
        Some(match tag {
            0 => Self::Off,
            1 => Self::Normal,
            2 => Self::Fast,
            3 => Self::Constant,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LinkedWeaponView {
    pub entity_num: i32,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RemoteMissile {
    pub projectile: crate::ProjectileId,
    pub entnum: i32,
    pub angles: [f32; 3],
    pub armed: bool,
    pub boosted: bool,
    pub attack: bool,
    pub unlink_at_ms: Option<i32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScriptBlur {
    pub from: f32,
    pub to: f32,
    pub set_ms: i32,
    pub duration_ms: i32,
}

impl ScriptBlur {
    pub fn sample(self, now_ms: i32) -> f32 {
        let fraction = if self.duration_ms <= 0 {
            1.0
        } else {
            (now_ms.wrapping_sub(self.set_ms) as f32 / self.duration_ms as f32).clamp(0.0, 1.0)
        };
        self.from + (self.to - self.from) * fraction
    }
}

/// Rendering dvars authored by GSC are delivered to clients without console gates.
pub fn is_postfx_dvar(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("r_film")
        || name.starts_with("r_glow")
        || name.starts_with("r_dof")
        || matches!(
            name.as_str(),
            "r_blur"
                | "r_hue"
                | "r_brightness"
                | "r_contrast"
                | "r_saturation"
                | "r_gamma"
                | "r_exposure"
                | "nightvision"
                | "cg_drawshellshock"
                | "thermalblurfactorscope"
                | "thermalblurfactornoscope"
        )
}
