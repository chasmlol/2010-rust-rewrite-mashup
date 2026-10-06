pub const WEAPCLASS_THROWINGKNIFE: i32 = 9;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EquipmentRuntimeFacts {
    pub offhand_class: i32,
    pub start_ammo: i32,
    pub clip_size: i32,
    pub impact_damage: i32,
    pub impact_payload_weapon: u32,
    pub fuse_time_ms: i32,

    pub hold_fire_time_ms: i32,

    pub cook_off_hold: bool,

    pub has_detonator: bool,
    pub detonate_delay_ms: i32,
    pub detonate_time_ms: i32,
    pub projectile_rotates: bool,
    pub stickiness: i32,
    pub timed_detonation: bool,

    pub proj_impact_explode: bool,

    pub stick_to_players: bool,
    pub ballistic_blade: bool,
    pub explosion_radius: i32,
    pub explosion_radius_min: i32,
    pub explosion_inner_damage: i32,
    pub explosion_outer_damage: i32,
    pub damage_cone_angle: f32,
    pub missile_guidance: i32,
    pub ignition_delay_ms: i32,
    pub require_lock_to_fire: bool,
    pub projectile_speed: i32,
    pub projectile_speed_up: i32,
    pub projectile_speed_forward: i32,
    pub projectile_speed_relative_up: i32,
    pub refuses_pickup: bool,
    pub projectile_activate_dist: i32,
    pub projectile_explosion_type: i32,
    pub weap_type: i32,
    pub weap_class: i32,
    pub parallel_bounce: Option<[f32; 31]>,
    pub perpendicular_bounce: Option<[f32; 31]>,
}

impl EquipmentRuntimeFacts {
    pub fn is_usable(self) -> bool {
        (self.projectile_speed > 0 || self.stickiness != 0)
            && (self.fuse_time_ms > 0 || self.impact_damage > 0 || self.explosion_inner_damage > 0)
    }

    pub fn is_throwing_knife(self) -> bool {
        self.weap_class == WEAPCLASS_THROWINGKNIFE
    }

    pub fn is_retrievable_knife(self) -> bool {
        self.is_throwing_knife() || self.ballistic_blade
    }

    pub fn is_offhand(self) -> bool {
        self.offhand_class != 0
    }

    pub fn spawn_clip_count(self) -> i32 {
        self.start_ammo.max(self.clip_size).max(1)
    }
}
