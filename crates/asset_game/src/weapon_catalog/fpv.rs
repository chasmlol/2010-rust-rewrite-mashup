use super::{WeaponBodyFacts, WeaponRegistry};
use crate::{WeaponKickFacts, WeaponSwayFacts};
use weapon_iw4::{WeaponIdleInputs, WeaponMovementOfsInputs};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponFpvFacts {
    pub ads_aim_pitch: f32,
    pub ads_bob_factor: f32,
    pub ads_overlay_height: f32,
    pub ads_overlay_width: f32,
    pub ads_view_bob_mult: f32,
    pub ads_zoom_fov: f32,
    pub ads_zoom_in_frac: f32,
    pub ads_zoom_out_frac: f32,
    pub aim_down_sight: bool,
    pub body_resolved: bool,
    pub can_hold_breath: bool,
    pub clip_index: i32,
    pub dual_wield: bool,
    pub dual_wield_view_model_offset: f32,
    pub ducked_ofs: [f32; 3],
    pub idle: WeaponIdleInputs,
    pub kick: WeaponKickFacts,
    pub movement: WeaponMovementOfsInputs,
    pub night_vision_wear_time: i32,
    pub overlay_reticle: i32,
    pub prone_ofs: [f32; 3],
    pub reload_show_rocket_time_ms: i32,
    pub reload_time_ms: i32,
    pub sway: WeaponSwayFacts,
    pub motion_tracker: bool,
    pub inherits_perks: bool,
    ads_overlay: crate::AdsOverlayConvention,
    alternate: bool,
    dual_animation: bool,
}

impl WeaponFpvFacts {
    pub fn is_alternate(self) -> bool {
        self.alternate
    }
    pub fn dual_animation(self) -> bool {
        self.dual_animation
    }

    pub fn ads_overlay_convention(self) -> crate::AdsOverlayConvention {
        self.ads_overlay
    }

    pub(super) fn prepare(
        f: WeaponBodyFacts,
        right_idle_bound: bool,
        ads_overlay: crate::AdsOverlayConvention,
    ) -> Self {
        Self {
            ads_aim_pitch: f.ads_aim_pitch,
            ads_bob_factor: f.ads_bob_factor,
            ads_overlay_height: f.ads_overlay_height,
            ads_overlay_width: f.ads_overlay_width,
            ads_view_bob_mult: f.ads_view_bob_mult,
            ads_zoom_fov: f.ads_zoom_fov,
            ads_zoom_in_frac: f.ads_zoom_in_frac,
            ads_zoom_out_frac: f.ads_zoom_out_frac,
            aim_down_sight: f.aim_down_sight,
            body_resolved: f.body_resolved,
            can_hold_breath: f.can_hold_breath,
            clip_index: f.clip_index,
            dual_wield: f.dual_wield,
            dual_wield_view_model_offset: f.dual_wield_view_model_offset,
            ducked_ofs: f.ducked_ofs,
            idle: f.idle,
            kick: f.kick,
            movement: f.movement,
            night_vision_wear_time: f.night_vision_wear_time,
            overlay_reticle: f.overlay_reticle,
            prone_ofs: f.prone_ofs,
            reload_show_rocket_time_ms: f.reload_show_rocket_time_ms,
            reload_time_ms: f.reload_time_ms,
            sway: f.sway,
            motion_tracker: f.motion_tracker,
            inherits_perks: f.inherits_perks,
            ads_overlay,
            alternate: f.inventory_type == 3,
            dual_animation: !f.no_dual_wield && right_idle_bound,
        }
    }
}

impl WeaponRegistry {
    pub(crate) fn fpv_facts_of(&self, id: u32) -> Option<WeaponFpvFacts> {
        self.rows.get(id as usize)?.fpv
    }
}
