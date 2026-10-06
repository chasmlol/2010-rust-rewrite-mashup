use crate::fire_weapon_kind;
use crate::weaponstate::{FireType, WeaponDecodeError, WeaponState};

pub const BURST_COOLDOWN_DEFAULT_MS: i32 = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeaponHostRules {
    pub burst_cooldown_ms: i32,
}

impl Default for WeaponHostRules {
    fn default() -> Self {
        Self {
            burst_cooldown_ms: BURST_COOLDOWN_DEFAULT_MS,
        }
    }
}

pub const PERK_FASTRELOAD: u32 = 4;

pub const PERK_WEAP_RELOAD_MULTIPLIER_DEFAULT: f32 = 0.5;

pub fn perk_fastreload_eligible(perks0: u32, inherits_perks: bool) -> bool {
    (perks0 & PERK_FASTRELOAD) != 0 && inherits_perks
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissingCombatFacts {
    BurstCooldown,

    SegmentedReloadAmmoAdd,

    BulletRange,

    UnknownFireType,

    LocationDamage,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CapturedCombatInput {
    pub dual_wield: bool,
    pub fire_melees: bool,
    pub fire_time_ms: i32,
    pub fire_delay_ms: i32,
    pub raise_time_ms: i32,
    pub drop_time_ms: i32,
    pub alternate_weapon: u32,
    pub alternate_raise_time_ms: i32,
    pub alternate_drop_time_ms: i32,
    pub first_raise_time_ms: i32,
    pub reload_time_ms: i32,
    pub reload_empty_time_ms: i32,
    pub clip_size: i32,
    pub start_ammo: i32,
    pub max_ammo: i32,
    pub ammo_index: i32,
    pub clip_index: i32,
    pub fire_type: i32,

    pub weap_type: i32,

    pub weap_class: i32,

    pub player_anim_type: i32,

    pub inventory_type: i32,

    pub impact_type: i32,
    pub shots_per_fire: i32,
    pub burst_cooldown_ms: i32,
    pub bolt_action: bool,
    pub rechamber_time_ms: i32,
    pub rechamber_bolt_time_ms: i32,

    pub rechamber_bolt_delay_ms: i32,
    pub segmented_reload: bool,
    pub reload_start_time_ms: i32,
    pub reload_end_time_ms: i32,
    pub reload_ammo_add: i32,

    pub reload_add_time_ms: i32,

    pub reload_empty_add_time_ms: i32,

    pub reload_start_add_time_ms: i32,

    pub reload_start_add: i32,
    pub no_partial_reload: bool,
    pub sprint_raise_time_ms: i32,
    pub sprint_drop_time_ms: i32,
    pub stunned_start_time_ms: i32,
    pub stunned_end_time_ms: i32,
    pub damage: i32,
    pub min_damage: i32,
    pub max_damage_range: f32,
    pub min_damage_range: f32,
    pub hip_spread_stand_min: f32,
    pub hip_spread_ducked_min: f32,
    pub hip_spread_prone_min: f32,
    pub hip_spread_stand_max: f32,
    pub hip_spread_ducked_max: f32,
    pub hip_spread_prone_max: f32,
    pub hip_spread_decay_rate: f32,
    pub hip_spread_fire_add: f32,
    pub hip_spread_turn_add: f32,
    pub hip_spread_move_add: f32,
    pub hip_spread_ducked_decay: f32,
    pub hip_spread_prone_decay: f32,
    pub ads_spread: f32,

    pub aim_down_sight: bool,

    pub no_ads_when_mag_empty: bool,

    pub inherits_perks: bool,

    pub ads_reload_trans_time_ms: i32,
    pub ads_in_rate: f32,

    pub ads_out_rate: f32,

    pub rechamber_while_ads: bool,

    pub ads_fire_only: bool,

    pub melee_damage: i32,

    pub can_hold_breath: bool,
    pub overlay_reticle: i32,

    pub melee_time_ms: i32,

    pub melee_delay_ms: i32,

    pub melee_charge_time_ms: i32,

    pub melee_charge_delay_ms: i32,

    pub melee_charge_anim: bool,

    pub knife_model: u32,

    pub quick_raise_time_ms: i32,

    pub quick_drop_time_ms: i32,

    pub select_requires_ammo: Option<bool>,

    pub offhand_hold_is_cancelable: Option<bool>,

    pub ads_gun_kick_reduced_kick_bullets: i32,

    pub hip_gun_kick_reduced_kick_bullets: i32,

    pub location_damage: [f32; crate::HITLOC_COUNT],

    pub dual_mag: Option<crate::reload::DualMagTimes>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AimAssistRanges {
    pub auto_aim: f32,
    pub hip: f32,
    pub ads: f32,
}

impl AimAssistRanges {
    pub const NONE: Self = Self {
        auto_aim: 0.0,
        hip: 0.0,
        ads: 0.0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponCombatFacts {
    pub aim_assist: AimAssistRanges,
    pub dual_wield: bool,
    pub fire_melees: bool,
    pub fire_time_ms: i32,
    pub fire_delay_ms: i32,
    pub raise_time_ms: i32,
    pub drop_time_ms: i32,
    pub alternate_weapon: u32,
    pub alternate_raise_time_ms: i32,
    pub alternate_drop_time_ms: i32,
    pub first_raise_time_ms: i32,
    pub reload_time_ms: i32,
    pub reload_empty_time_ms: i32,
    pub clip_size: i32,
    pub start_ammo: i32,
    pub max_ammo: i32,

    pub ammo_index: i32,

    pub clip_index: i32,
    pub fire_type: i32,

    pub weap_type: i32,

    pub weap_class: i32,

    pub player_anim_type: i32,

    pub inventory_type: i32,

    pub impact_type: i32,

    pub shots_per_fire: i32,

    pub burst_cooldown_ms: i32,

    pub bolt_action: bool,

    pub rechamber_time_ms: i32,

    pub rechamber_bolt_time_ms: i32,

    pub rechamber_bolt_delay_ms: i32,

    pub segmented_reload: bool,
    pub reload_start_time_ms: i32,
    pub reload_end_time_ms: i32,

    pub reload_ammo_add: i32,

    pub reload_add_time_ms: i32,

    pub reload_empty_add_time_ms: i32,

    pub reload_start_add_time_ms: i32,

    pub reload_start_add: i32,

    pub no_partial_reload: bool,

    pub inherits_perks: bool,

    pub sprint_raise_time_ms: i32,

    pub sprint_drop_time_ms: i32,
    pub stunned_start_time_ms: i32,
    pub stunned_end_time_ms: i32,
    pub damage: i32,
    pub min_damage: i32,
    pub max_damage_range: f32,
    pub min_damage_range: f32,
    pub hip_spread_stand_min: f32,
    pub hip_spread_ducked_min: f32,
    pub hip_spread_prone_min: f32,
    pub hip_spread_stand_max: f32,
    pub hip_spread_ducked_max: f32,
    pub hip_spread_prone_max: f32,
    pub hip_spread_decay_rate: f32,
    pub hip_spread_fire_add: f32,
    pub hip_spread_turn_add: f32,
    pub hip_spread_move_add: f32,
    pub hip_spread_ducked_decay: f32,
    pub hip_spread_prone_decay: f32,
    pub ads_spread: f32,

    pub aim_down_sight: bool,

    pub no_ads_when_mag_empty: bool,

    pub ads_reload_trans_time_ms: i32,
    pub ads_in_rate: f32,

    pub ads_out_rate: f32,

    pub rechamber_while_ads: bool,

    pub ads_fire_only: bool,

    pub melee_damage: i32,

    pub can_hold_breath: bool,
    pub overlay_reticle: i32,

    pub melee_time_ms: i32,

    pub melee_delay_ms: i32,

    pub melee_charge_time_ms: i32,

    pub melee_charge_delay_ms: i32,

    pub melee_charge_anim: bool,

    pub knife_model: u32,

    pub quick_raise_time_ms: i32,

    pub quick_drop_time_ms: i32,

    pub select_requires_ammo: Option<bool>,

    pub offhand_hold_is_cancelable: Option<bool>,

    pub ads_gun_kick_reduced_kick_bullets: i32,

    pub hip_gun_kick_reduced_kick_bullets: i32,

    pub location_damage: [f32; crate::HITLOC_COUNT],

    pub dual_mag: Option<crate::reload::DualMagTimes>,
}

impl Default for WeaponCombatFacts {
    fn default() -> Self {
        Self::none()
    }
}

impl WeaponCombatFacts {
    pub const fn none() -> Self {
        Self {
            aim_assist: AimAssistRanges::NONE,
            dual_wield: false,
            fire_melees: false,
            fire_time_ms: 0,
            fire_delay_ms: 0,
            raise_time_ms: 0,
            drop_time_ms: 0,
            alternate_weapon: 0,
            alternate_raise_time_ms: 0,
            alternate_drop_time_ms: 0,
            first_raise_time_ms: 0,
            reload_time_ms: 0,
            reload_empty_time_ms: 0,
            clip_size: 0,
            start_ammo: 0,
            max_ammo: 0,
            ammo_index: 0,
            clip_index: 0,
            fire_type: 0,
            weap_type: 0,
            weap_class: 0,
            player_anim_type: 0,
            inventory_type: 0,
            impact_type: 0,
            shots_per_fire: 0,
            burst_cooldown_ms: 0,
            bolt_action: false,
            rechamber_time_ms: 0,
            rechamber_bolt_time_ms: 0,
            rechamber_bolt_delay_ms: 0,
            segmented_reload: false,
            reload_start_time_ms: 0,
            reload_end_time_ms: 0,
            reload_ammo_add: 0,
            reload_add_time_ms: 0,
            reload_empty_add_time_ms: 0,
            reload_start_add_time_ms: 0,
            reload_start_add: 0,
            no_partial_reload: false,
            sprint_raise_time_ms: 0,
            sprint_drop_time_ms: 0,
            stunned_start_time_ms: 0,
            stunned_end_time_ms: 0,
            damage: 0,
            min_damage: 0,
            max_damage_range: 0.0,
            min_damage_range: 0.0,
            hip_spread_stand_min: 0.0,
            hip_spread_ducked_min: 0.0,
            hip_spread_prone_min: 0.0,
            hip_spread_stand_max: 0.0,
            hip_spread_ducked_max: 0.0,
            hip_spread_prone_max: 0.0,
            hip_spread_decay_rate: 0.0,
            hip_spread_fire_add: 0.0,
            hip_spread_turn_add: 0.0,
            hip_spread_move_add: 0.0,
            hip_spread_ducked_decay: 0.0,
            hip_spread_prone_decay: 0.0,
            ads_spread: 0.0,
            aim_down_sight: false,
            no_ads_when_mag_empty: false,
            inherits_perks: false,
            ads_reload_trans_time_ms: 0,
            ads_in_rate: 0.0,
            ads_out_rate: 0.0,
            rechamber_while_ads: true,
            ads_fire_only: false,
            melee_damage: 0,
            can_hold_breath: false,
            overlay_reticle: 0,
            melee_time_ms: 0,
            melee_delay_ms: 0,
            melee_charge_time_ms: 0,
            melee_charge_delay_ms: 0,
            melee_charge_anim: false,
            knife_model: 0,
            quick_raise_time_ms: 0,
            quick_drop_time_ms: 0,
            select_requires_ammo: Some(false),
            offhand_hold_is_cancelable: Some(false),
            ads_gun_kick_reduced_kick_bullets: 0,
            hip_gun_kick_reduced_kick_bullets: 0,
            location_damage: crate::LOCATION_DAMAGE_IDENTITY,
            dual_mag: None,
        }
    }

    pub fn test_fixture(_reason: &'static str, facts: Self) -> Self {
        facts
    }

    pub fn try_from_captured(input: CapturedCombatInput) -> Result<Self, MissingCombatFacts> {
        if input.fire_time_ms <= 0
            && input.raise_time_ms <= 0
            && input.damage <= 0
            && input.clip_size <= 0
        {
            return Ok(Self::none());
        }
        let fire_ty =
            FireType::from_i32(input.fire_type).map_err(|_| MissingCombatFacts::UnknownFireType)?;
        if fire_ty.is_burst() && input.burst_cooldown_ms <= 0 {
            return Err(MissingCombatFacts::BurstCooldown);
        }
        if input.segmented_reload && input.reload_ammo_add <= 0 {
            return Err(MissingCombatFacts::SegmentedReloadAmmoAdd);
        }
        if input.damage > 0 && input.max_damage_range <= 0.0 && input.min_damage_range <= 0.0 {
            return Err(MissingCombatFacts::BulletRange);
        }
        if !crate::location_damage_is_valid(&input.location_damage) {
            return Err(MissingCombatFacts::LocationDamage);
        }
        Ok(Self {
            aim_assist: AimAssistRanges::NONE,
            dual_wield: input.dual_wield,
            fire_melees: input.fire_melees,
            fire_time_ms: input.fire_time_ms,
            fire_delay_ms: input.fire_delay_ms,
            raise_time_ms: input.raise_time_ms,
            drop_time_ms: input.drop_time_ms,
            alternate_weapon: input.alternate_weapon,
            alternate_raise_time_ms: input.alternate_raise_time_ms,
            alternate_drop_time_ms: input.alternate_drop_time_ms,
            first_raise_time_ms: input.first_raise_time_ms,
            reload_time_ms: input.reload_time_ms,
            reload_empty_time_ms: input.reload_empty_time_ms,
            clip_size: input.clip_size,
            start_ammo: input.start_ammo,
            max_ammo: input.max_ammo,
            ammo_index: input.ammo_index,
            clip_index: input.clip_index,
            fire_type: input.fire_type,
            weap_type: input.weap_type,
            weap_class: input.weap_class,
            player_anim_type: input.player_anim_type,
            inventory_type: input.inventory_type,
            impact_type: input.impact_type,
            shots_per_fire: input.shots_per_fire,
            burst_cooldown_ms: input.burst_cooldown_ms,
            bolt_action: input.bolt_action,
            rechamber_time_ms: input.rechamber_time_ms,
            rechamber_bolt_time_ms: input.rechamber_bolt_time_ms,
            rechamber_bolt_delay_ms: input.rechamber_bolt_delay_ms,
            segmented_reload: input.segmented_reload,
            reload_start_time_ms: input.reload_start_time_ms,
            reload_end_time_ms: input.reload_end_time_ms,
            reload_ammo_add: input.reload_ammo_add,
            reload_add_time_ms: input.reload_add_time_ms,
            reload_empty_add_time_ms: input.reload_empty_add_time_ms,
            reload_start_add_time_ms: input.reload_start_add_time_ms,
            reload_start_add: input.reload_start_add,
            no_partial_reload: input.no_partial_reload,
            inherits_perks: input.inherits_perks,
            sprint_raise_time_ms: input.sprint_raise_time_ms,
            sprint_drop_time_ms: input.sprint_drop_time_ms,
            stunned_start_time_ms: input.stunned_start_time_ms,
            stunned_end_time_ms: input.stunned_end_time_ms,
            damage: input.damage,
            min_damage: input.min_damage,
            max_damage_range: input.max_damage_range,
            min_damage_range: input.min_damage_range,
            hip_spread_stand_min: input.hip_spread_stand_min,
            hip_spread_ducked_min: input.hip_spread_ducked_min,
            hip_spread_prone_min: input.hip_spread_prone_min,
            hip_spread_stand_max: input.hip_spread_stand_max,
            hip_spread_ducked_max: input.hip_spread_ducked_max,
            hip_spread_prone_max: input.hip_spread_prone_max,
            hip_spread_decay_rate: input.hip_spread_decay_rate,
            hip_spread_fire_add: input.hip_spread_fire_add,
            hip_spread_turn_add: input.hip_spread_turn_add,
            hip_spread_move_add: input.hip_spread_move_add,
            hip_spread_ducked_decay: input.hip_spread_ducked_decay,
            hip_spread_prone_decay: input.hip_spread_prone_decay,
            ads_spread: input.ads_spread,
            aim_down_sight: input.aim_down_sight,
            no_ads_when_mag_empty: input.no_ads_when_mag_empty,
            ads_reload_trans_time_ms: input.ads_reload_trans_time_ms,
            ads_in_rate: input.ads_in_rate,
            ads_out_rate: input.ads_out_rate,
            rechamber_while_ads: input.rechamber_while_ads,
            ads_fire_only: input.ads_fire_only,
            melee_damage: input.melee_damage,
            can_hold_breath: input.can_hold_breath,
            overlay_reticle: input.overlay_reticle,
            melee_time_ms: input.melee_time_ms,
            melee_delay_ms: input.melee_delay_ms,
            melee_charge_time_ms: input.melee_charge_time_ms,
            melee_charge_delay_ms: input.melee_charge_delay_ms,
            melee_charge_anim: input.melee_charge_anim,
            knife_model: input.knife_model,
            quick_raise_time_ms: input.quick_raise_time_ms,
            quick_drop_time_ms: input.quick_drop_time_ms,
            select_requires_ammo: input.select_requires_ammo,
            offhand_hold_is_cancelable: input.offhand_hold_is_cancelable,
            ads_gun_kick_reduced_kick_bullets: input.ads_gun_kick_reduced_kick_bullets,
            hip_gun_kick_reduced_kick_bullets: input.hip_gun_kick_reduced_kick_bullets,
            location_damage: input.location_damage,
            dual_mag: input.dual_mag,
        })
    }

    pub fn is_none(self) -> bool {
        self.fire_time_ms <= 0 && self.raise_time_ms <= 0 && self.damage <= 0 && self.clip_size <= 0
    }

    pub fn is_usable(self) -> bool {
        !self.is_none()
    }

    pub fn fire_type_enum(self) -> Result<FireType, WeaponDecodeError> {
        FireType::from_i32(self.fire_type)
    }

    pub fn ammo_per_shot(self) -> i32 {
        1
    }

    pub fn pellet_count(self) -> i32 {
        if self.weap_class == crate::WEAPCLASS_SPREAD {
            if self.shots_per_fire <= 0 {
                1
            } else {
                self.shots_per_fire
            }
        } else {
            1
        }
    }

    pub fn reload_duration_ms(self, clip_empty: bool) -> i32 {
        if clip_empty && self.reload_empty_time_ms > 0 {
            self.reload_empty_time_ms
        } else {
            self.reload_time_ms
        }
    }

    pub fn burst_cooldown(self) -> i32 {
        self.burst_cooldown_ms
    }

    pub fn shells_per_reload_add(self) -> i32 {
        self.reload_ammo_add
    }

    pub fn bullet_range(self) -> f32 {
        if self.weap_class == crate::WEAPCLASS_SPREAD {
            self.min_damage_range
        } else {
            crate::BULLET_MAX_RANGE
        }
    }

    pub fn location_scale(self, hitloc: u8) -> f32 {
        crate::location_damage_scale(&self.location_damage, hitloc)
    }

    pub fn spread_facts(self) -> crate::WeaponSpreadFacts {
        crate::WeaponSpreadFacts {
            stand_min: self.hip_spread_stand_min,
            ducked_min: self.hip_spread_ducked_min,
            prone_min: self.hip_spread_prone_min,
            stand_max: self.hip_spread_stand_max,
            ducked_max: self.hip_spread_ducked_max,
            prone_max: self.hip_spread_prone_max,
        }
    }

    pub fn aim_spread_decay_facts(self) -> crate::WeaponAimSpreadDecayFacts {
        crate::WeaponAimSpreadDecayFacts {
            decay_rate: self.hip_spread_decay_rate,
            fire_add: self.hip_spread_fire_add,
            turn_add: self.hip_spread_turn_add,
            move_add: self.hip_spread_move_add,
            ducked_decay: self.hip_spread_ducked_decay,
            prone_decay: self.hip_spread_prone_decay,
        }
    }

    pub fn ads_allow_facts(self) -> crate::AdsAllowWeaponFacts {
        crate::AdsAllowWeaponFacts {
            aim_down_sight: self.aim_down_sight,
            no_ads_when_mag_empty: self.no_ads_when_mag_empty,
            clip_index: self.clip_index,
        }
    }

    pub fn ads_frac_context(self) -> (f32, f32, bool, bool) {
        (
            self.ads_in_rate,
            self.ads_out_rate,
            self.rechamber_while_ads,
            self.ads_fire_only,
        )
    }

    pub fn melee_facts(self) -> crate::MeleeWeaponFacts {
        crate::MeleeWeaponFacts::from_combat(&self)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WeaponHandState {
    pub weapon: u32,
    pub weaponstate: i32,
    pub weapon_time: i32,
    pub weapon_delay: i32,

    pub weap_anim: i32,

    pub hand_index: u8,
    pub clip: i32,
    pub stock: i32,

    pub shot_count: u8,

    pub burst_latch: bool,

    pub rechamber_pending: bool,

    pub delayed_rechamber: bool,

    pub weapon_restrict_kick_time: i32,

    pub quick_reload: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct WeaponCmd {
    pub msec: i32,
    pub server_time: i32,
    pub stun_time: i32,
    pub buttons: u32,
    pub old_buttons: u32,

    pub cmd_weapon: u16,

    pub pm_flags: u32,

    pub weap_flags: u32,

    pub pm_type: i32,

    pub e_flags: u32,

    pub last_weapon_hand: i32,

    pub f_weapon_pos_frac: f32,

    pub melee_charge_yaw: f32,

    pub melee_charge_dist: u8,

    pub player_melee_range: f32,

    pub is_in_air: bool,

    pub melee_charge: crate::MeleeChargeState,

    pub melee_started: Option<bool>,

    pub mantle_weapon_inactive: bool,

    pub mantle_quick_raise: bool,

    pub cmd_weapon_owned: bool,

    pub cmd_weapon_pistol_quick: bool,

    pub switch_raise_time_ms: i32,
    pub switch_alternate_raise_time_ms: i32,
    pub alternate_switch: bool,

    pub switch_quick_raise_time_ms: i32,
    pub switch_first_raise_time_ms: i32,

    pub offhand: crate::offhand::OffhandCmd,

    pub perks0: u32,

    pub perk_weap_reload_multiplier: f32,
}

impl Default for WeaponCmd {
    fn default() -> Self {
        Self {
            msec: 0,
            server_time: 0,
            stun_time: 0,
            buttons: 0,
            old_buttons: 0,
            cmd_weapon: 0,
            pm_flags: 0,
            weap_flags: 0,
            pm_type: 0,
            e_flags: 0,
            last_weapon_hand: 0,
            f_weapon_pos_frac: 0.0,
            melee_charge_yaw: 0.0,
            melee_charge_dist: 0,
            player_melee_range: crate::PLAYER_MELEE_RANGE_DEFAULT,
            is_in_air: false,
            melee_charge: crate::MeleeChargeState::default(),
            melee_started: None,
            mantle_weapon_inactive: false,
            mantle_quick_raise: false,
            cmd_weapon_owned: false,
            cmd_weapon_pistol_quick: false,
            switch_raise_time_ms: 0,
            switch_alternate_raise_time_ms: 0,
            alternate_switch: false,
            switch_quick_raise_time_ms: 0,
            switch_first_raise_time_ms: 0,
            offhand: crate::offhand::OffhandCmd::default(),
            perks0: 0,
            perk_weap_reload_multiplier: PERK_WEAP_RELOAD_MULTIPLIER_DEFAULT,
        }
    }
}

pub const BUTTON_RELOAD: u32 = playerstate_iw4::buttons::RELOAD;

pub const BUTTON_ATTACK: u32 = playerstate_iw4::buttons::ATTACK;

pub const CHECK_FIRING_AMMO_DRY_FIRE_MS: i32 = 500;

pub const BUTTON_THROW: u32 = playerstate_iw4::buttons::THROW;

pub fn get_weapon_fire_button(last_weapon_hand: i32, hand_index: i32) -> u32 {
    let left = hand_index != 0;
    if last_weapon_hand == 1 {
        if left { BUTTON_ATTACK } else { BUTTON_THROW }
    } else if left {
        BUTTON_THROW
    } else {
        BUTTON_ATTACK
    }
}

pub fn weapon_time_adjust(
    hand: &WeaponHandState,
    facts: &WeaponCombatFacts,
    cmd: &WeaponCmd,
) -> i32 {
    let reload_family = matches!(hand.weaponstate, 0x8 | 0x9 | 0xA | 0xB | 0xC);
    if !(reload_family && perk_fastreload_eligible(cmd.perks0, facts.inherits_perks)) {
        return cmd.msec;
    }
    let multiplier = cmd.perk_weap_reload_multiplier;
    if multiplier == 0.0 {
        return hand.weapon_time.max(hand.weapon_delay);
    }

    ((cmd.msec as f32 / multiplier) + 0.5) as i32
}

pub fn ads_fire_only_delay_ms(frac: f32, ads_in_rate: f32) -> i32 {
    if ads_in_rate <= 0.0 {
        return 0;
    }
    let remaining = (1.0 - frac).max(0.0);
    (remaining / ads_in_rate) as i32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponTickEvent {
    StunnedStarted,
    Detonated {
        weapon: u32,
    },
    ShotAccepted {
        ammo_used: i32,
    },
    EmptyClick,
    ReloadStarted,

    ReloadInsert,

    ReloadEnded,

    ReloadAmmoAdded {
        shells: i32,
    },

    RechamberWeapon,

    EjectBrass,
    RaiseFinished,
    DropFinished,

    PutawayStarted,
    AlternateStarted,

    RaiseStarted,

    OffhandUsed {
        weapon: u32,
        remaining_fuse_ms: Option<i32>,
    },

    OffhandCookedOff {
        weapon: u32,
    },

    OffhandPrepare {
        weapon: u32,
    },

    MeleeFired,
}

fn burst_pending(hand: &WeaponHandState, fire_ty: FireType) -> bool {
    fire_ty
        .burst_limit()
        .is_some_and(|lim| hand.shot_count > 0 && hand.shot_count < lim)
}

fn decay_offhand_family_timers(hand: &mut WeaponHandState, msec: i32) -> bool {
    if !crate::offhand::in_offhand_family(hand.weaponstate) {
        return false;
    }
    let delay_before = hand.weapon_delay;
    if hand.weapon_time > 0 {
        hand.weapon_time = (hand.weapon_time - msec).max(0);
    }
    if hand.weapon_delay > 0 {
        hand.weapon_delay = (hand.weapon_delay - msec).max(0);
    }
    delay_before > 0 && hand.weapon_delay == 0
}

#[derive(Clone, Copy)]
struct OrdinaryWeaponTick {
    fire_ty: FireType,
    attack: bool,
    was_attack: bool,
    delayed_action: bool,
}

#[derive(Clone, Copy)]
enum PreparedWeaponTick {
    Complete(Option<WeaponTickEvent>),
    Ordinary(OrdinaryWeaponTick),
}

pub fn weapon_ordinary(
    hand: &mut WeaponHandState,
    facts: &WeaponCombatFacts,
    cmd: &mut WeaponCmd,
) -> Option<WeaponTickEvent> {
    match prepare_weapon_tick(hand, facts, cmd) {
        PreparedWeaponTick::Complete(event) => event,
        PreparedWeaponTick::Ordinary(tick) => {
            crate::sprint::weapon_check_for_sprint(hand, facts, cmd.pm_flags);
            finish_weapon_tick(hand, facts, cmd, tick)
        }
    }
}

fn prepare_weapon_tick(
    hand: &mut WeaponHandState,
    facts: &WeaponCombatFacts,
    cmd: &mut WeaponCmd,
) -> PreparedWeaponTick {
    if let Some(cooked) = crate::offhand::weapon_update_grenade_throw(hand, cmd) {
        return PreparedWeaponTick::Complete(Some(cooked));
    }

    if cmd.cmd_weapon == 0 && hand.weapon != 0 {
        cmd.cmd_weapon = hand.weapon as u16;
    }

    if facts.fire_time_ms <= 0 && hand.weapon != 0 && facts.weap_type != crate::WEAPTYPE_SHIELD {
        if matches!(
            WeaponState::from_i32(hand.weaponstate),
            Ok(WeaponState::Raising | WeaponState::RaisingAltswitch)
        ) {
            hand.weapon_time = (hand.weapon_time - cmd.msec).max(0);
            if hand.weapon_time == 0 {
                hand.weaponstate = WeaponState::Ready as i32;
                crate::weap_anim::weapon_idle_weap_anim(&mut hand.weap_anim, cmd.pm_type);
            }
        }
        let delayed_action = decay_offhand_family_timers(hand, cmd.msec);
        if let Some(prepare) = crate::offhand::weapon_check_for_offhand(hand, cmd) {
            return PreparedWeaponTick::Complete(Some(prepare));
        }
        return PreparedWeaponTick::Complete(crate::offhand::weapon_advance_offhand(
            hand,
            cmd,
            delayed_action,
        ));
    }
    let fire_ty = match facts.fire_type_enum() {
        Ok(ty) => ty,

        Err(_) => return PreparedWeaponTick::Complete(None),
    };

    let detonator = cmd
        .offhand
        .inventory
        .iter()
        .find(|row| row.weapon == hand.weapon && row.has_detonator)
        .copied();
    if hand.weaponstate == WeaponState::Detonating as i32 {
        let before = hand.weapon_delay;
        hand.weapon_time = (hand.weapon_time - cmd.msec).max(0);
        hand.weapon_delay = (hand.weapon_delay - cmd.msec).max(0);
        if before > 0 && hand.weapon_delay == 0 {
            return PreparedWeaponTick::Complete(Some(WeaponTickEvent::Detonated {
                weapon: hand.weapon,
            }));
        }
        if hand.weapon_time == 0 {
            hand.weaponstate = WeaponState::Ready as i32;
            crate::weap_anim::weapon_idle_weap_anim(&mut hand.weap_anim, cmd.pm_type);
        }
        return PreparedWeaponTick::Complete(None);
    }
    if let Some(row) = detonator.filter(|_| {
        hand.weaponstate == WeaponState::Ready as i32
            && cmd.buttons & playerstate_iw4::buttons::ATTACK != 0
    }) {
        hand.weaponstate = WeaponState::Detonating as i32;
        hand.weapon_time = row.detonate_time_ms.max(1);
        hand.weapon_delay = row.detonate_delay_ms.max(1);
        crate::weap_anim::start_weapon_anim(
            &mut hand.weap_anim,
            crate::weap_anim::weap_anim_event::DETONATE,
        );
        return PreparedWeaponTick::Complete(None);
    }
    let fire_mask = if fire_ty == FireType::DoubleBarrel && cmd.last_weapon_hand != 1 {
        BUTTON_ATTACK | BUTTON_THROW
    } else if detonator.is_some() {
        playerstate_iw4::buttons::THROW
    } else {
        get_weapon_fire_button(cmd.last_weapon_hand, i32::from(hand.hand_index))
    };
    let attack = cmd.buttons & fire_mask != 0;
    let was_attack = cmd.old_buttons & fire_mask != 0;
    if fire_ty.is_single() && !attack {
        hand.shot_count = 0;
    }
    let time_before = hand.weapon_time;
    let delay_before = hand.weapon_delay;
    let decay_ms = weapon_time_adjust(hand, facts, cmd);
    if hand.weapon_time > 0 {
        hand.weapon_time = (hand.weapon_time - decay_ms).max(0);
    }

    if time_before != 0 && hand.weapon_time < 1 && hand.weapon_delay == 0 {
        weapon_decay_hold_interrupt(hand, fire_ty, cmd, attack);
    }
    if hand.weapon_delay > 0 {
        hand.weapon_delay = (hand.weapon_delay - decay_ms).max(0);
    }
    let delayed_action = delay_before > 0 && hand.weapon_delay == 0;

    if hand.weapon_restrict_kick_time > 0 {
        hand.weapon_restrict_kick_time = (hand.weapon_restrict_kick_time - cmd.msec).max(0);
    }

    PreparedWeaponTick::Ordinary(OrdinaryWeaponTick {
        fire_ty,
        attack,
        was_attack,
        delayed_action,
    })
}

fn finish_weapon_tick(
    hand: &mut WeaponHandState,
    facts: &WeaponCombatFacts,
    cmd: &mut WeaponCmd,
    tick: OrdinaryWeaponTick,
) -> Option<WeaponTickEvent> {
    let OrdinaryWeaponTick {
        fire_ty,
        attack,
        was_attack,
        delayed_action,
    } = tick;
    crate::sprint::weapon_advance_sprint(hand, &mut cmd.weap_flags, &mut cmd.pm_flags, cmd.pm_type);
    if let Some(melee) = crate::melee::weapon_advance_melee(
        hand,
        &facts.melee_facts(),
        &mut cmd.weap_flags,
        &mut cmd.pm_flags,
        cmd.pm_type,
        delayed_action,
    ) {
        return Some(melee);
    }

    if let Some(prepare) = crate::offhand::weapon_check_for_offhand(hand, cmd) {
        return Some(prepare);
    }
    let mut event = crate::weapon_change::weapon_check_for_change(hand, facts, cmd);

    let delayed = crate::reload::weapon_reload_delayed_action(hand, facts, delayed_action);
    let ammo_credited = delayed.shells;
    let rechamber_ev = delayed.rechamber_event;
    hand.delayed_rechamber = rechamber_ev;

    if fire_ty.is_burst() && attack && !was_attack {
        hand.burst_latch = true;
    }

    let reload = cmd.buttons & BUTTON_RELOAD != 0;
    if crate::reload::weapon_process_input_wants_reload(hand, facts, reload, cmd.pm_flags)
        && begin_weapon_reload(hand, facts)
    {
        return Some(WeaponTickEvent::ReloadStarted);
    }

    match WeaponState::from_i32(hand.weaponstate) {
        Ok(WeaponState::Raising) | Ok(WeaponState::RaisingAltswitch) => {
            if hand.weapon_time <= 0 {
                hand.weaponstate = WeaponState::Ready as i32;
                crate::weap_anim::weapon_idle_weap_anim(&mut hand.weap_anim, cmd.pm_type);
                event = Some(WeaponTickEvent::RaiseFinished);
            }
        }
        Ok(WeaponState::Dropping)
        | Ok(WeaponState::DroppingQuick)
        | Ok(WeaponState::DroppingAltswitch) => {
            if hand.weapon_time <= 0 {
                if crate::weapon_change::traversal_forces_holster(cmd) {
                    crate::weapon_change::finish_putaway_while_holstered(hand);
                    event = Some(WeaponTickEvent::DropFinished);
                } else {
                    crate::weapon_change::finish_putaway_to_cmd(hand, cmd);
                    if event != Some(WeaponTickEvent::AlternateStarted) {
                        event = if hand.weapon != 0 {
                            Some(WeaponTickEvent::RaiseStarted)
                        } else {
                            Some(WeaponTickEvent::DropFinished)
                        };
                    }
                }
            }
        }
        Ok(WeaponState::Firing) => {
            if hand.weapon_time <= 0 && hand.weapon_delay <= 0 && !delayed_action {
                if fire_ty.is_burst() {
                    if burst_pending(hand, fire_ty) {
                    } else {
                        crate::weap_anim::continue_weapon_anim(
                            &mut hand.weap_anim,
                            crate::weap_anim::weap_anim_event::IDLE,
                            cmd.pm_type,
                        );
                        hand.weaponstate = WeaponState::Ready as i32;
                        hand.weapon_time = facts.burst_cooldown().max(1);
                        hand.shot_count = 0;
                    }
                } else {
                    if !attack && !hand.burst_latch {
                        crate::weap_anim::continue_weapon_anim(
                            &mut hand.weap_anim,
                            crate::weap_anim::weap_anim_event::IDLE,
                            cmd.pm_type,
                        );
                    }
                    hand.weaponstate = WeaponState::Ready as i32;
                }
            }
        }
        Ok(WeaponState::Rechambering) => {}
        Ok(WeaponState::ReloadStart) | Ok(WeaponState::ReloadStartInterrupt) => {
            if hand.weapon_time <= 0 && hand.weapon_delay <= 0 {
                if facts.segmented_reload && attack {
                    hand.weaponstate = WeaponState::ReloadStartInterrupt as i32;
                }
                let interrupt_with_shells =
                    hand.weaponstate == WeaponState::ReloadStartInterrupt as i32 && hand.clip > 0;
                if interrupt_with_shells || !crate::reload::weapon_allow_reload(hand, facts) {
                    event = begin_reload_end(hand, facts, cmd.pm_type);
                } else {
                    event = begin_reload_loop(hand, facts, cmd.pm_type);
                }
            }
        }
        Ok(WeaponState::Reloading) | Ok(WeaponState::ReloadingInterrupt) => {
            if hand.weapon_time <= 0 && hand.weapon_delay <= 0 {
                if facts.segmented_reload {
                    if attack {
                        hand.weaponstate = WeaponState::ReloadingInterrupt as i32;
                    }
                    if hand.weaponstate == WeaponState::ReloadingInterrupt as i32
                        || !crate::reload::weapon_allow_reload(hand, facts)
                    {
                        event = begin_reload_end(hand, facts, cmd.pm_type);
                    } else {
                        event = begin_reload_loop(hand, facts, cmd.pm_type);
                    }
                } else {
                    hand.weaponstate = WeaponState::Ready as i32;
                    hand.weapon_delay = 0;
                    hand.rechamber_pending = false;
                    crate::weap_anim::weapon_idle_weap_anim(&mut hand.weap_anim, cmd.pm_type);
                }
            }
        }
        Ok(WeaponState::ReloadEnd) => {
            if hand.weapon_time <= 0 {
                hand.weaponstate = WeaponState::Ready as i32;
                crate::weap_anim::weapon_idle_weap_anim(&mut hand.weap_anim, cmd.pm_type);
            }
        }
        Ok(WeaponState::Ready) => {}
        Ok(
            WeaponState::OffhandInit
            | WeaponState::OffhandPrepare
            | WeaponState::OffhandHold
            | WeaponState::OffhandStart
            | WeaponState::Offhand
            | WeaponState::OffhandEnd,
        ) => {
            event = crate::offhand::weapon_advance_offhand(hand, cmd, delayed_action);
        }

        Ok(
            WeaponState::MeleeInit
            | WeaponState::MeleeFire
            | WeaponState::MeleeEnd
            | WeaponState::Detonating
            | WeaponState::StunnedStart
            | WeaponState::StunnedLoop
            | WeaponState::StunnedEnd
            | WeaponState::NightVisionWear
            | WeaponState::NightVisionRemove,
        ) => {}

        Ok(WeaponState::SprintIn | WeaponState::SprintLoop | WeaponState::SprintOut) => {}

        Err(_) => return None,
    }

    if event.is_none() {
        event = weapon_check_for_rechamber(hand, facts, cmd, delayed_action);
    }

    if event.is_some() {
        return event;
    }

    let pending = burst_pending(hand, fire_ty);

    let ready_idle = hand.weaponstate == WeaponState::Ready as i32 && hand.weapon_time <= 0;
    let mid_burst_continue =
        hand.weaponstate == WeaponState::Firing as i32 && hand.weapon_time <= 0 && pending;
    let delayed_fire =
        delayed_action && hand.weaponstate == WeaponState::Firing as i32 && hand.weapon_delay == 0;

    if ready_idle || mid_burst_continue || delayed_fire {
        let trigger = match fire_ty {
            FireType::FullAuto => attack || delayed_fire,
            FireType::SingleShot | FireType::DoubleBarrel => {
                (attack && hand.shot_count == 0) || delayed_fire
            }
            FireType::BurstFire2 | FireType::BurstFire3 | FireType::BurstFire4 => {
                attack || pending || delayed_fire
            }
        };

        if trigger && !facts.fire_melees {
            if hand.clip <= 0 {
                hand.shot_count = 0;
                if hand.stock > 0 && begin_weapon_reload(hand, facts) {
                    return Some(WeaponTickEvent::ReloadStarted);
                }

                crate::weap_anim::continue_weapon_anim(
                    &mut hand.weap_anim,
                    crate::weap_anim::weap_anim_event::IDLE,
                    cmd.pm_type,
                );
                if facts.weap_type != crate::WEAPTYPE_GRENADE {
                    hand.weapon_time += CHECK_FIRING_AMMO_DRY_FIRE_MS;
                    if facts.clip_size > 0 {
                        return Some(WeaponTickEvent::EmptyClick);
                    }
                }
                return None;
            }

            if !delayed_fire {
                if ready_idle {
                    hand.weapon_restrict_kick_time = crate::start_firing_restrict_kick_time(
                        cmd.f_weapon_pos_frac,
                        facts.ads_gun_kick_reduced_kick_bullets,
                        facts.hip_gun_kick_reduced_kick_bullets,
                        facts.fire_time_ms,
                        facts.fire_delay_ms,
                    );
                }
                hand.weaponstate = WeaponState::Firing as i32;
                hand.weapon_time = facts.fire_time_ms.max(1);
                if facts.ads_fire_only {
                    let ads_in_rate = if cmd.perks0 & playerstate_iw4::PERK_QUICKDRAW != 0 {
                        facts.ads_in_rate * playerstate_iw4::PERK_QUICKDRAW_SPEED_SCALE
                    } else {
                        facts.ads_in_rate
                    };
                    hand.weapon_delay = ads_fire_only_delay_ms(cmd.f_weapon_pos_frac, ads_in_rate);
                } else if facts.fire_delay_ms > 0 {
                    hand.weapon_delay = facts.fire_delay_ms;
                }

                if !fire_ty.is_full_auto() {
                    if fire_ty.is_burst() && hand.shot_count == 0 {
                        hand.burst_latch = false;
                    }
                    hand.shot_count = hand.shot_count.saturating_add(1).min(4);
                }
                if hand.weapon_delay != 0 {
                    return None;
                }
            }
            if fire_weapon_kind(facts.weap_type, facts.weap_class).is_none() {
                return Some(WeaponTickEvent::EmptyClick);
            }
            let double = fire_ty == FireType::DoubleBarrel
                && cmd.last_weapon_hand != 1
                && cmd.buttons & (BUTTON_ATTACK | BUTTON_THROW) == (BUTTON_ATTACK | BUTTON_THROW)
                && hand.clip > 1;
            cmd.weap_flags = (cmd.weap_flags & !0x200) | if double { 0x200 } else { 0 };
            let used = if double { 2 } else { facts.ammo_per_shot() }.min(hand.clip);
            hand.clip -= used;
            if facts.bolt_action {
                hand.rechamber_pending = true;
            }
            crate::weap_anim::set_fps_fire_anim(
                &mut hand.weap_anim,
                cmd.f_weapon_pos_frac > 0.0,
                hand.clip <= 0,
            );
            return Some(WeaponTickEvent::ShotAccepted { ammo_used: used });
        }
    }

    if rechamber_ev {
        Some(WeaponTickEvent::RechamberWeapon)
    } else if ammo_credited > 0 {
        Some(WeaponTickEvent::ReloadAmmoAdded {
            shells: ammo_credited,
        })
    } else {
        None
    }
}

fn weapon_finish_rechamber(hand: &mut WeaponHandState, pm_type: i32) {
    crate::weap_anim::continue_weapon_anim(
        &mut hand.weap_anim,
        crate::weap_anim::weap_anim_event::IDLE,
        pm_type,
    );
    hand.weaponstate = WeaponState::Ready as i32;
}

fn weapon_check_for_rechamber(
    hand: &mut WeaponHandState,
    facts: &WeaponCombatFacts,
    cmd: &WeaponCmd,
    delayed_action: bool,
) -> Option<WeaponTickEvent> {
    if !facts.bolt_action {
        return None;
    }

    let mut delayed_brass = None;
    if delayed_action
        && hand.rechamber_pending
        && hand.weaponstate == WeaponState::Rechambering as i32
    {
        hand.rechamber_pending = false;
        delayed_brass = Some(WeaponTickEvent::EjectBrass);
        if hand.weapon_time != 0 {
            return delayed_brass;
        }
    }
    if !hand.rechamber_pending {
        if hand.weaponstate == WeaponState::Rechambering as i32
            && hand.weapon_time == 0
            && hand.weapon_delay == 0
        {
            weapon_finish_rechamber(hand, cmd.pm_type);
        }
        return delayed_brass;
    }
    if hand.weaponstate != WeaponState::Ready as i32 || hand.weapon_time > 0 {
        return None;
    }
    let rechamber = if cmd.last_weapon_hand == 1 {
        facts.rechamber_bolt_time_ms
    } else {
        facts.rechamber_time_ms
    };
    let rechamber = if rechamber > 0 { rechamber } else { 1 };
    hand.weaponstate = WeaponState::Rechambering as i32;
    hand.weapon_time = rechamber;

    let bolt = facts.rechamber_bolt_delay_ms;
    hand.weapon_delay = if bolt != 0 && bolt < rechamber {
        bolt
    } else {
        1
    };
    crate::weap_anim::set_rechamber_anim(&mut hand.weap_anim, cmd.f_weapon_pos_frac > 0.0);

    Some(WeaponTickEvent::RechamberWeapon)
}

fn weapon_decay_hold_interrupt(
    hand: &mut WeaponHandState,
    fire_ty: FireType,
    cmd: &WeaponCmd,
    attack: bool,
) {
    let ws = hand.weaponstate;
    let rechamber = ws == WeaponState::Rechambering as i32;
    let firing = ws == WeaponState::Firing as i32;
    if firing && fire_ty.is_burst() && !burst_pending(hand, fire_ty) {
        return;
    }
    if !rechamber && !firing {
        return;
    }

    if !fire_ty.shot_limit_reached(hand.shot_count) {
        return;
    }
    if !attack {
        return;
    }
    if u32::from(cmd.cmd_weapon) != hand.weapon {
        return;
    }

    if hand.clip <= 0 {
        return;
    }
    hand.weapon_time = 1;
    if rechamber {
        weapon_finish_rechamber(hand, cmd.pm_type);
    } else {
        crate::weap_anim::continue_weapon_anim(
            &mut hand.weap_anim,
            crate::weap_anim::weap_anim_event::IDLE,
            cmd.pm_type,
        );
        hand.weaponstate = WeaponState::Ready as i32;
    }
}

fn begin_weapon_reload(hand: &mut WeaponHandState, facts: &WeaponCombatFacts) -> bool {
    let ws = hand.weaponstate;
    if !(ws == 0 || ws == 6 || ws == 7 || (0x16 < ws && ws < 0x1a)) {
        return false;
    }
    hand.shot_count = 0;
    hand.burst_latch = false;
    hand.weapon_delay = 0;
    if facts.segmented_reload {
        let start = if facts.reload_start_time_ms > 0 {
            facts.reload_start_time_ms
        } else {
            facts.reload_duration_ms(true)
        };
        let start = start.max(1);
        hand.weaponstate = WeaponState::ReloadStart as i32;
        hand.weapon_time = start;
        crate::weap_anim::start_weapon_anim(
            &mut hand.weap_anim,
            crate::weap_anim::weap_anim_event::RELOAD_START,
        );
        crate::reload::weapon_arm_reload_add_delay(hand, facts, start);
        return true;
    }
    let empty = reload_from_empty_clip(hand, facts);
    let (full, anim) = crate::reload::reload_segment(hand, facts, empty);
    hand.weaponstate = WeaponState::Reloading as i32;
    hand.weapon_time = full;
    crate::weap_anim::start_weapon_anim(&mut hand.weap_anim, anim);
    crate::reload::weapon_arm_reload_add_delay(hand, facts, full);
    true
}

pub fn weapon_hands(
    hands: &mut [WeaponHandState],
    facts: &WeaponCombatFacts,
    cmd: &mut WeaponCmd,
    last_hand: i32,
) -> [Option<(u8, WeaponTickEvent)>; 2] {
    let mut out = [None, None];
    let last = last_hand.clamp(0, 1) as usize;
    let n = hands.len().min(last + 1);
    let transition = check_stunned(&mut hands[..n], facts, cmd);
    let started = transition == Some(WeaponState::StunnedStart);
    if started
        || hands[..n]
            .iter()
            .any(|hand| matches!(hand.weaponstate, 26..=28))
    {
        for hand in &mut hands[..n] {
            if transition.is_none() {
                hand.weapon_time = (hand.weapon_time - cmd.msec).max(0);
                hand.weapon_delay = (hand.weapon_delay - cmd.msec).max(0);
            }
            hand.weapon_restrict_kick_time = (hand.weapon_restrict_kick_time - cmd.msec).max(0);
            if hand.weapon_time == 0 && hand.weapon_delay == 0 {
                match WeaponState::from_i32(hand.weaponstate) {
                    Ok(WeaponState::StunnedStart) => {
                        set_stunned_state(hand, WeaponState::StunnedLoop, 0, cmd.pm_type)
                    }
                    Ok(WeaponState::StunnedEnd) => {
                        hand.weaponstate = WeaponState::Ready as i32;
                        crate::weap_anim::weapon_idle_weap_anim(&mut hand.weap_anim, cmd.pm_type);
                    }
                    _ => {}
                }
            }
        }
        if started {
            out[0] = Some((0, WeaponTickEvent::StunnedStarted));
        }
        return out;
    }

    cmd.melee_charge.pm_flags = cmd.pm_flags;
    cmd.melee_charge.pm_type = cmd.pm_type;
    cmd.melee_charge.e_flags = cmd.e_flags;
    let melee_buttons = |buttons: u32| {
        if facts.fire_melees && buttons & BUTTON_ATTACK != 0 {
            buttons | crate::BUTTON_MELEE
        } else {
            buttons
        }
    };
    cmd.melee_started = crate::melee::weapon_try_melee(
        hands,
        &facts.melee_facts(),
        melee_buttons(cmd.buttons),
        melee_buttons(cmd.old_buttons),
        cmd.f_weapon_pos_frac,
        cmd.last_weapon_hand,
        &mut cmd.melee_charge,
        cmd.melee_charge_yaw,
        cmd.melee_charge_dist,
        cmd.is_in_air,
        cmd.player_melee_range,
    );
    cmd.pm_flags = cmd.melee_charge.pm_flags;
    let mut prepared = [PreparedWeaponTick::Complete(None); 2];
    for i in 0..n {
        hands[i].hand_index = i as u8;
        prepared[i] = prepare_weapon_tick(&mut hands[i], facts, cmd);
    }
    if prepared[..n]
        .iter()
        .all(|tick| matches!(tick, PreparedWeaponTick::Ordinary(_)))
    {
        crate::sprint::weapon_check_hands_for_sprint(&mut hands[..n], facts, cmd);
    }
    for i in 0..n {
        let event = match prepared[i] {
            PreparedWeaponTick::Complete(event) => event,
            PreparedWeaponTick::Ordinary(tick) => {
                finish_weapon_tick(&mut hands[i], facts, cmd, tick)
            }
        };
        out[i] = event.map(|event| (i as u8, event));
    }
    if n > 1 {
        let stock = hands[0].stock.min(hands[1].stock);
        hands[0].stock = stock;
        hands[1].stock = stock;
    }
    out
}

fn reload_from_empty_clip(hand: &WeaponHandState, facts: &WeaponCombatFacts) -> bool {
    hand.clip <= 0 && facts.weap_type == 0
}

fn begin_reload_loop(
    hand: &mut WeaponHandState,
    facts: &WeaponCombatFacts,
    pm_type: i32,
) -> Option<WeaponTickEvent> {
    let empty = reload_from_empty_clip(hand, facts);
    let (full, anim) = crate::reload::reload_segment(hand, facts, empty);
    if pm_type < 8 {
        crate::weap_anim::start_weapon_anim(&mut hand.weap_anim, anim);
    }

    hand.weaponstate = if hand.weaponstate == WeaponState::ReloadStartInterrupt as i32 {
        WeaponState::ReloadingInterrupt as i32
    } else {
        WeaponState::Reloading as i32
    };
    hand.weapon_time = full;
    hand.weapon_delay = 0;
    crate::reload::weapon_arm_reload_add_delay(hand, facts, full);
    Some(WeaponTickEvent::ReloadInsert)
}

fn begin_reload_end(
    hand: &mut WeaponHandState,
    facts: &WeaponCombatFacts,
    pm_type: i32,
) -> Option<WeaponTickEvent> {
    hand.weapon_delay = 0;

    hand.rechamber_pending = false;
    if facts.reload_end_time_ms > 0 {
        hand.weaponstate = WeaponState::ReloadEnd as i32;
        hand.weapon_time = facts.reload_end_time_ms;
        if pm_type < 8 {
            crate::weap_anim::start_weapon_anim(
                &mut hand.weap_anim,
                crate::weap_anim::weap_anim_event::RELOAD_END,
            );
        }
        Some(WeaponTickEvent::ReloadEnded)
    } else {
        hand.weaponstate = WeaponState::Ready as i32;
        hand.weapon_time = 0;
        crate::weap_anim::weapon_idle_weap_anim(&mut hand.weap_anim, pm_type);
        None
    }
}

pub fn spawn_clip_stock(facts: &WeaponCombatFacts, last_hand: i32) -> (i32, i32, i32) {
    let total = facts.start_ammo.max(0);
    let mag = facts.clip_size.max(0);
    let clip0 = if mag > 0 { total.min(mag) } else { 0 };
    let rest = (total - clip0).max(0);
    let clip1 = if last_hand >= 1 && mag > 0 {
        rest.min(mag)
    } else {
        0
    };
    let stock = (rest - clip1).max(0);
    (clip0, clip1, stock)
}

pub fn spawn_weapon_hand(weapon: u32, facts: &WeaponCombatFacts, first: bool) -> WeaponHandState {
    let total = facts.start_ammo.max(0);
    let clip = if facts.clip_size > 0 {
        total.min(facts.clip_size)
    } else {
        0
    };
    let stock = (total - clip).max(0);
    let first = first && facts.first_raise_time_ms > 0;
    let raise = if first {
        facts.first_raise_time_ms
    } else {
        facts.raise_time_ms.max(0)
    };
    let mut weap_anim = 0;
    if raise > 0 {
        crate::weap_anim::start_weapon_anim(
            &mut weap_anim,
            if first {
                crate::weap_anim::weap_anim_event::FIRST_RAISE
            } else {
                crate::weap_anim::weap_anim_event::RAISE
            },
        );
    }
    WeaponHandState {
        weapon,
        weaponstate: if raise > 0 {
            WeaponState::Raising as i32
        } else {
            WeaponState::Ready as i32
        },
        weapon_time: raise,
        weapon_delay: 0,
        weap_anim,
        hand_index: 0,
        clip,
        stock,
        shot_count: 0,
        burst_latch: false,
        rechamber_pending: false,
        delayed_rechamber: false,
        weapon_restrict_kick_time: 0,
        quick_reload: facts.dual_mag.is_some(),
    }
}

fn check_stunned(
    hands: &mut [WeaponHandState],
    facts: &WeaponCombatFacts,
    cmd: &WeaponCmd,
) -> Option<WeaponState> {
    let Some(primary) = hands.first() else {
        return None;
    };
    if cmd.cmd_weapon == 0 || matches!(primary.weaponstate, 1..=5 | 16..=21 | 29..=30) {
        return None;
    }
    let active = cmd.server_time <= cmd.stun_time;
    let ongoing = matches!(primary.weaponstate, 26 | 27);
    if active && !ongoing {
        for hand in hands {
            set_stunned_state(
                hand,
                WeaponState::StunnedStart,
                facts.stunned_start_time_ms,
                cmd.pm_type,
            );
        }
        Some(WeaponState::StunnedStart)
    } else {
        if !active && ongoing {
            for hand in hands {
                set_stunned_state(
                    hand,
                    WeaponState::StunnedEnd,
                    facts.stunned_end_time_ms,
                    cmd.pm_type,
                );
            }
            return Some(WeaponState::StunnedEnd);
        }
        None
    }
}

fn set_stunned_state(hand: &mut WeaponHandState, state: WeaponState, time: i32, pm_type: i32) {
    hand.weaponstate = state as i32;
    hand.weapon_time = time;
    hand.weapon_delay = 0;
    if pm_type < 8 {
        crate::weap_anim::start_weapon_anim(&mut hand.weap_anim, state as u32);
    }
}
