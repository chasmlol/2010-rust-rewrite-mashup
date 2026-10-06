use super::{WeaponBodyFacts, WeaponRegistry};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectileCameraPolicy {
    Missile,
    TopAttack,
    Remote,
    Rocket,
    MissileAlternate,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponHudFacts {
    pub ads_aim_pitch: f32,
    pub ads_crosshair_in_frac: f32,
    pub ads_crosshair_out_frac: f32,
    pub ads_overlay_height: f32,
    pub ads_overlay_width: f32,
    pub ads_zoom_in_frac: f32,
    pub ads_zoom_out_frac: f32,
    pub ammo_counter_clip: i32,
    pub ammo_index: i32,
    pub can_hold_breath: bool,
    pub clip_index: i32,
    pub clip_only: bool,
    pub clip_size: i32,
    pub flip_kill_icon: bool,
    pub hip_reticle_side_pos: f32,
    pub hip_spread_ducked_max: f32,
    pub hip_spread_ducked_min: f32,
    pub hip_spread_prone_max: f32,
    pub hip_spread_prone_min: f32,
    pub hip_spread_stand_max: f32,
    pub hip_spread_stand_min: f32,
    pub i_reticle_min_ofs: i32,
    pub i_reticle_side_size: i32,
    pub kill_icon_ratio: i32,
    pub low_ammo_warning_threshold: f32,
    pub thermal_scope: bool,
    pub ads_dof: Option<[f32; 2]>,
    pub offhand_class: i32,
    pub overlay_reticle: i32,
    primary: bool,
    alternate: bool,
    guided_overlay: bool,
}

impl WeaponHudFacts {
    pub fn is_primary(self) -> bool {
        self.primary
    }
    pub fn is_alternate(self) -> bool {
        self.alternate
    }
    pub fn guided_overlay(self) -> bool {
        self.guided_overlay
    }
    pub(super) fn prepare(f: WeaponBodyFacts) -> Self {
        Self {
            ads_aim_pitch: f.ads_aim_pitch,
            ads_crosshair_in_frac: f.ads_crosshair_in_frac,
            ads_crosshair_out_frac: f.ads_crosshair_out_frac,
            ads_overlay_height: f.ads_overlay_height,
            ads_overlay_width: f.ads_overlay_width,
            ads_zoom_in_frac: f.ads_zoom_in_frac,
            ads_zoom_out_frac: f.ads_zoom_out_frac,
            ammo_counter_clip: f.ammo_counter_clip,
            ammo_index: f.ammo_index,
            can_hold_breath: f.can_hold_breath,
            clip_index: f.clip_index,
            clip_only: f.clip_only,
            clip_size: f.clip_size,
            flip_kill_icon: f.flip_kill_icon,
            hip_reticle_side_pos: f.hip_reticle_side_pos,
            hip_spread_ducked_max: f.hip_spread_ducked_max,
            hip_spread_ducked_min: f.hip_spread_ducked_min,
            hip_spread_prone_max: f.hip_spread_prone_max,
            hip_spread_prone_min: f.hip_spread_prone_min,
            hip_spread_stand_max: f.hip_spread_stand_max,
            hip_spread_stand_min: f.hip_spread_stand_min,
            i_reticle_min_ofs: f.i_reticle_min_ofs,
            i_reticle_side_size: f.i_reticle_side_size,
            kill_icon_ratio: f.kill_icon_ratio,
            low_ammo_warning_threshold: f.low_ammo_warning_threshold,
            thermal_scope: f.thermal_scope,
            ads_dof: f.ads_dof,
            offhand_class: f.offhand_class,
            overlay_reticle: f.overlay_reticle,
            primary: f.inventory_type == 0,
            alternate: f.inventory_type == 3,
            guided_overlay: f.overlay_interface == 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponEventFacts {
    pub impact_type: i32,
    pub bolt_action: bool,
    pub ignition_delay_ms: i32,
    pub explosion_radius: i32,
    pub explosion_radius_min: i32,
    pub explosion_inner_damage: i32,
    pub explosion_outer_damage: i32,
    pub damage: i32,
    projectile_camera: ProjectileCameraPolicy,
    rests_on_ground: bool,
    hides_fire_ping: bool,
}

impl WeaponEventFacts {
    pub fn projectile_camera(self) -> ProjectileCameraPolicy {
        self.projectile_camera
    }
    pub fn rests_on_ground(self) -> bool {
        self.rests_on_ground
    }
    pub fn hides_fire_ping(self) -> bool {
        self.hides_fire_ping
    }
    pub(super) fn prepare(f: WeaponBodyFacts) -> Self {
        Self {
            impact_type: f.impact_type,
            bolt_action: f.bolt_action,
            ignition_delay_ms: f.ignition_delay_ms,
            explosion_radius: f.explosion_radius,
            explosion_radius_min: f.explosion_radius_min,
            explosion_inner_damage: f.explosion_inner_damage,
            explosion_outer_damage: f.explosion_outer_damage,
            damage: f.damage,
            projectile_camera: if f.missile_guidance == 3 {
                ProjectileCameraPolicy::TopAttack
            } else if f.missile_guidance == 2 {
                ProjectileCameraPolicy::Remote
            } else if f.weap_class == 7 {
                ProjectileCameraPolicy::Rocket
            } else if f.weap_type == 2 {
                ProjectileCameraPolicy::MissileAlternate
            } else {
                ProjectileCameraPolicy::Missile
            },
            rests_on_ground: matches!(f.stickiness, 3 | 4),
            hides_fire_ping: f.weap_type == weapon_iw4::WEAPTYPE_GRENADE || f.silenced,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponWorldFacts {
    shield: bool,
}

impl WeaponWorldFacts {
    pub fn is_shield(self) -> bool {
        self.shield
    }
    pub(super) fn prepare(f: WeaponBodyFacts) -> Self {
        Self {
            shield: f.weap_type == weapon_iw4::WEAPTYPE_SHIELD,
        }
    }
}

impl WeaponRegistry {
    pub(crate) fn hud_facts_of(&self, id: u32) -> Option<WeaponHudFacts> {
        self.rows.get(id as usize)?.hud
    }
    pub(crate) fn event_facts_of(&self, id: u32) -> Option<WeaponEventFacts> {
        self.rows.get(id as usize)?.events
    }
    pub(crate) fn world_facts_of(&self, id: u32) -> Option<WeaponWorldFacts> {
        self.rows.get(id as usize)?.world
    }
}
