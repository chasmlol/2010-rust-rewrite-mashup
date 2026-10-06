use super::WeaponRegistry;

pub(super) fn equipment(
    registry: &WeaponRegistry,
    id: u32,
) -> Option<weapon_iw4::EquipmentRuntimeFacts> {
    let f = registry.facts_of(id)?;
    Some(weapon_iw4::EquipmentRuntimeFacts {
        offhand_class: f.offhand_class,
        start_ammo: f.start_ammo_rounds(),
        clip_size: f.clip_size,
        impact_damage: f.damage,
        impact_payload_weapon: registry.impact_payload_of(id).unwrap_or(0),
        fuse_time_ms: f.fuse_time_ms,
        hold_fire_time_ms: f.hold_fire_time_ms,
        cook_off_hold: f.cook_off_hold,
        has_detonator: f.has_detonator,
        detonate_delay_ms: f.detonate_delay_ms,
        detonate_time_ms: f.detonate_time_ms,
        projectile_rotates: f.projectile_rotates
            || registry.host_namespace_of(id) != Some(asset_core::AssetNamespace::Iw4),
        stickiness: f.stickiness,
        timed_detonation: f.timed_detonation,
        proj_impact_explode: f.proj_impact_explode,
        stick_to_players: f.stick_to_players,
        ballistic_blade: registry.host_namespace_of(id) == Some(asset_core::AssetNamespace::T5)
            && registry.name_of(id) == "knife_ballistic",
        explosion_radius: f.explosion_radius,
        explosion_radius_min: f.explosion_radius_min,
        explosion_inner_damage: f.explosion_inner_damage,
        explosion_outer_damage: f.explosion_outer_damage,
        damage_cone_angle: f.damage_cone_angle,
        missile_guidance: f.missile_guidance,
        ignition_delay_ms: f.ignition_delay_ms,
        require_lock_to_fire: f.require_lock_to_fire,
        projectile_speed: f.projectile_speed,
        projectile_speed_up: f.projectile_speed_up,
        projectile_speed_forward: f.projectile_speed_forward,
        projectile_speed_relative_up: f.projectile_speed_relative_up,
        refuses_pickup: f.refuses_pickup,
        projectile_activate_dist: f.projectile_activate_dist,
        projectile_explosion_type: f.projectile_explosion_type,
        weap_type: f.weap_type,
        weap_class: f.weap_class,
        parallel_bounce: f.parallel_bounce,
        perpendicular_bounce: f.perpendicular_bounce,
    })
}

pub(super) fn penetration(
    registry: &WeaponRegistry,
    id: u32,
) -> Option<weapon_iw4::BulletPenFacts> {
    let f = registry.facts_of(id)?;
    Some(weapon_iw4::BulletPenFacts {
        penetrate_type: f.penetrate_type,
        penetrate_multiplier: f.penetrate_multiplier,
        rifle_bullet: f.rifle_bullet,
        ricochet_chance: f.ricochet_chance,
        explosive_bullet: f.explosive_bullet,
    })
}

impl WeaponRegistry {
    pub fn equipment_facts_of(&self, id: u32) -> Option<weapon_iw4::EquipmentRuntimeFacts> {
        self.rows.get(id as usize)?.equipment
    }
    pub fn penetration_facts_of(&self, id: u32) -> Option<weapon_iw4::BulletPenFacts> {
        self.rows.get(id as usize)?.penetration
    }
}
