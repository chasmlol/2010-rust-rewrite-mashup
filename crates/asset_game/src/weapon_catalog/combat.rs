use super::{WeaponBodyFacts, WeaponRegistry};
use weapon_iw4::{
    CapturedCombatInput, HITLOC_COUNT, LOCATION_DAMAGE_IDENTITY, MissingCombatFacts,
    WeaponCombatFacts, WeaponHostRules, bake_location_damage, location_damage_is_valid,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponCombatRefusal {
    UnknownWeapon,
    Unpublished,
    UnsupportedConfiguration,
    MissingFacts(MissingCombatFacts),
}

#[derive(Clone, Debug)]
pub(super) struct WeaponCombatProjection {
    input: CapturedCombatInput,
    location_damage: Option<[f32; HITLOC_COUNT]>,
    aim_assist: weapon_iw4::AimAssistRanges,
    melee_only: bool,
}

fn captured_input(f: WeaponBodyFacts, melee_charge_anim: bool) -> CapturedCombatInput {
    let segmented_reload = f.segmented_reload && f.reload_start_time_ms > 0;
    let reload_ammo_add = if f.reload_ammo_add > 0 {
        f.reload_ammo_add
    } else if segmented_reload {
        1
    } else {
        0
    };
    CapturedCombatInput {
        dual_wield: f.dual_wield,
        fire_melees: f.fire_melees,
        fire_time_ms: f.fire_time_ms,
        fire_delay_ms: f.fire_delay_ms,
        raise_time_ms: f.raise_time_ms,
        drop_time_ms: f.drop_time_ms,
        alternate_weapon: 0,
        alternate_raise_time_ms: f.alternate_raise_time_ms,
        alternate_drop_time_ms: f.alternate_drop_time_ms,
        first_raise_time_ms: f.first_raise_time_ms,
        reload_time_ms: f.reload_time_ms,
        reload_empty_time_ms: f.reload_empty_time_ms,
        clip_size: f.clip_size,
        start_ammo: f.start_ammo_rounds(),
        max_ammo: f.max_ammo_rounds(),
        ammo_index: f.ammo_index,
        clip_index: f.clip_index,
        fire_type: f.fire_type,
        weap_type: f.weap_type,
        weap_class: f.weap_class,
        player_anim_type: f.player_anim_type,
        inventory_type: f.inventory_type,

        impact_type: f.impact_type,
        shots_per_fire: f.shots_per_fire,

        burst_cooldown_ms: 0,

        bolt_action: f.bolt_action,
        rechamber_time_ms: f.rechamber_time_ms,
        rechamber_bolt_time_ms: f.rechamber_bolt_time_ms,
        rechamber_bolt_delay_ms: f.rechamber_bolt_delay_ms,
        segmented_reload,
        reload_start_time_ms: f.reload_start_time_ms,
        reload_end_time_ms: f.reload_end_time_ms,
        reload_ammo_add,
        reload_add_time_ms: f.reload_add_time_ms,
        reload_empty_add_time_ms: f.reload_empty_add_time_ms,
        reload_start_add_time_ms: f.reload_start_add_time_ms,
        reload_start_add: f.reload_start_add,
        no_partial_reload: f.no_partial_reload,
        dual_mag: f.dual_mag,
        inherits_perks: f.inherits_perks,
        sprint_raise_time_ms: f.sprint_raise_time_ms,
        sprint_drop_time_ms: f.sprint_drop_time_ms,
        stunned_start_time_ms: f.stunned_start_time_ms,
        stunned_end_time_ms: f.stunned_end_time_ms,
        damage: f.damage,
        min_damage: f.min_damage,
        max_damage_range: f.max_damage_range,
        min_damage_range: f.min_damage_range,
        hip_spread_stand_min: f.hip_spread_stand_min,
        hip_spread_ducked_min: f.hip_spread_ducked_min,
        hip_spread_prone_min: f.hip_spread_prone_min,
        hip_spread_stand_max: f.hip_spread_stand_max,
        hip_spread_ducked_max: f.hip_spread_ducked_max,
        hip_spread_prone_max: f.hip_spread_prone_max,
        hip_spread_decay_rate: f.hip_spread_decay_rate,
        hip_spread_fire_add: f.hip_spread_fire_add,
        hip_spread_turn_add: f.hip_spread_turn_add,
        hip_spread_move_add: f.hip_spread_move_add,
        hip_spread_ducked_decay: f.hip_spread_ducked_decay,
        hip_spread_prone_decay: f.hip_spread_prone_decay,
        ads_spread: f.ads_spread,
        aim_down_sight: f.aim_down_sight,
        no_ads_when_mag_empty: f.no_ads_when_mag_empty,
        ads_reload_trans_time_ms: f.ads_reload_trans_time_ms,
        ads_in_rate: f.ads_in_rate,
        ads_out_rate: f.ads_out_rate,
        rechamber_while_ads: f.rechamber_while_ads,
        ads_fire_only: f.ads_fire_only,
        melee_damage: f.melee_damage,
        can_hold_breath: f.can_hold_breath,
        overlay_reticle: f.overlay_reticle,
        melee_time_ms: f.melee_time_ms,
        melee_delay_ms: f.melee_delay_ms,
        melee_charge_time_ms: f.melee_charge_time_ms,
        melee_charge_delay_ms: f.melee_charge_delay_ms,
        melee_charge_anim,
        knife_model: f.knife_model,
        quick_raise_time_ms: f.quick_raise_time_ms,
        quick_drop_time_ms: f.quick_drop_time_ms,
        select_requires_ammo: f.select_requires_ammo,
        offhand_hold_is_cancelable: f.offhand_hold_is_cancelable,
        ads_gun_kick_reduced_kick_bullets: f.kick.ads_gun_kick_reduced_kick_bullets,
        hip_gun_kick_reduced_kick_bullets: f.kick.hip_gun_kick_reduced_kick_bullets,
        location_damage: LOCATION_DAMAGE_IDENTITY,
    }
}

impl WeaponCombatProjection {
    pub(super) fn prepare(registry: &WeaponRegistry, id: u32) -> Option<Self> {
        let mut f = registry.facts_of(id)?;
        let charge_anim = |weapon| {
            registry
                .anim_of(weapon, asset_iw4::size::weap_anim::MELEE_CHARGE)
                .is_some_and(|name| !name.is_empty())
        };
        let mut melee_charge_anim = charge_anim(id);
        let melee_weapon = registry.melee_weapon_of(id);
        if melee_weapon != id
            && let Some(melee) = registry.facts_of(melee_weapon)
        {
            f.melee_damage = melee.melee_damage;
            f.melee_time_ms = melee.melee_time_ms;
            f.melee_delay_ms = melee.melee_delay_ms;
            f.melee_charge_time_ms = melee.melee_charge_time_ms;
            f.melee_charge_delay_ms = melee.melee_charge_delay_ms;
            melee_charge_anim = charge_anim(melee_weapon);
            f.knife_model = melee_weapon;
        }
        let mut input = captured_input(f, melee_charge_anim);
        input.alternate_weapon = registry.alternate_of(id);
        Some(Self {
            input,
            location_damage: f.location_damage_mult,
            aim_assist: weapon_iw4::AimAssistRanges {
                auto_aim: f.auto_aim_range,
                hip: f.aim_assist_range,
                ads: f.aim_assist_range_ads,
            },
            melee_only: f.weap_type == 7
                && f.fire_time_ms <= 0
                && f.raise_time_ms <= 0
                && f.clip_size <= 0,
        })
    }

    fn compile(
        &self,
        rules: WeaponHostRules,
        global_location: Option<[f32; HITLOC_COUNT]>,
    ) -> Result<WeaponCombatFacts, MissingCombatFacts> {
        let mut input = self.input;
        let fire_type = weapon_iw4::FireType::from_i32(input.fire_type)
            .map_err(|_| MissingCombatFacts::UnknownFireType)?;
        input.burst_cooldown_ms = if fire_type.is_burst() {
            rules.burst_cooldown_ms
        } else {
            0
        };
        if self
            .location_damage
            .is_some_and(|table| !location_damage_is_valid(&table))
        {
            return Err(MissingCombatFacts::LocationDamage);
        }
        input.location_damage = bake_location_damage(
            input.weap_type,
            input.weap_class,
            global_location.unwrap_or(LOCATION_DAMAGE_IDENTITY),
            self.location_damage,
        );
        let mut facts = WeaponCombatFacts::try_from_captured(input)?;
        facts.alternate_weapon = input.alternate_weapon;
        facts.aim_assist = self.aim_assist;
        Ok(facts)
    }
}

impl WeaponRegistry {
    pub(crate) fn combat_facts_of(
        &self,
        id: u32,
        rules: WeaponHostRules,
        global_location: Option<[f32; HITLOC_COUNT]>,
    ) -> Result<WeaponCombatFacts, WeaponCombatRefusal> {
        if id == 0 {
            return Ok(WeaponCombatFacts::none());
        }
        let row = self
            .rows
            .get(id as usize)
            .ok_or(WeaponCombatRefusal::UnknownWeapon)?;
        if !self.configuration_supported(id) {
            return Err(WeaponCombatRefusal::UnsupportedConfiguration);
        }
        row.combat
            .as_ref()
            .ok_or(WeaponCombatRefusal::Unpublished)?
            .compile(rules, global_location)
            .map_err(WeaponCombatRefusal::MissingFacts)
    }

    pub fn melee_only_of(&self, id: u32) -> bool {
        self.rows
            .get(id as usize)
            .and_then(|row| row.combat.as_ref())
            .is_some_and(|projection| projection.melee_only)
    }
}
