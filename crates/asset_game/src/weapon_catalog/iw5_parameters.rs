use super::*;

pub(super) fn iw5_primary_ads(
    assets: &[&Iw5ScopeRow],
) -> (Option<fastfile_iw5::AttachmentAdsSettings>, f32) {
    let mut chosen = None;
    let mut scale_product = 1.0;
    for asset in assets {
        let settings = if asset.share_ammo_with_alt {
            asset.ads_settings_main
        } else {
            asset.ads_settings
        };
        if chosen.is_none() {
            chosen = settings;
        }
        let scale = if asset.share_ammo_with_alt {
            (asset.ads_settings_main.is_none()
                && asset.scales.ads_settings_main != 0.0
                && asset.scales.ads_settings_main != 1.0)
                .then_some(asset.scales.ads_settings_main)
        } else {
            (asset.scales.ads_settings > 0.0).then_some(asset.scales.ads_settings)
        };
        if let Some(scale) = scale {
            scale_product *= scale;
        }
    }
    (chosen, scale_product)
}

pub(super) fn iw5_default_scope_models(
    slots: &[Option<String>; fastfile_iw5::size::WEAPON_ATTACHMENT_SLOT_COUNT],
    attachments: &HashMap<String, Iw5ScopeRow>,
) -> Option<(Option<String>, Option<String>)> {
    slots[..6]
        .iter()
        .filter_map(Option::as_deref)
        .find_map(|name| {
            if !name.ends_with("scope") || name.ends_with("vzscope") {
                return None;
            }
            let asset = attachments.get(name)?;
            let view = asset.view_models[0].clone();
            let world = asset.world_models[0].clone();
            (view.is_some() || world.is_some()).then_some((view, world))
        })
}

pub(super) fn iw5_first_block<T: Copy>(
    assets: &[&Iw5ScopeRow],
    block: impl Fn(&Iw5ScopeRow) -> Option<T>,
) -> Option<T> {
    assets.iter().find_map(|asset| block(asset))
}

pub(super) fn iw5_scale_product(
    assets: &[&Iw5ScopeRow],
    scale: impl Fn(&fastfile_iw5::AttachmentScales) -> f32,
) -> f32 {
    assets
        .iter()
        .map(|asset| scale(&asset.scales))
        .filter(|scale| *scale > 0.0)
        .product()
}

pub(super) fn scale_i32(value: i32, scale: f32) -> i32 {
    if scale == 1.0 {
        value
    } else {
        (value as f32 * scale) as i32
    }
}

pub(super) fn apply_iw5_parameter_blocks(facts: &mut WeaponBodyFacts, assets: &[&Iw5ScopeRow]) {
    let (ads, ads_scale) = iw5_primary_ads(assets);
    if let Some(ads) = ads {
        facts.ads_spread = ads.ads_spread;
        facts.ads_aim_pitch = ads.ads_aim_pitch;
        facts.ads_crosshair_in_frac = ads.ads_crosshair_in_frac;
        facts.ads_crosshair_out_frac = ads.ads_crosshair_out_frac;
        facts.ads_zoom_fov = ads.ads_zoom_fov;
        facts.ads_zoom_in_frac = ads.ads_zoom_in_frac;
        facts.ads_zoom_out_frac = ads.ads_zoom_out_frac;
        facts.ads_bob_factor = ads.ads_bob_factor;
        facts.ads_view_bob_mult = ads.ads_view_bob_mult;
        // Attachment transition times are seconds; the rates are per millisecond.
        if ads.ads_trans_in_time > 0.0 {
            facts.ads_in_rate = 1.0 / (ads.ads_trans_in_time * 1000.0);
        }
        if ads.ads_trans_out_time > 0.0 {
            facts.ads_out_rate = 1.0 / (ads.ads_trans_out_time * 1000.0);
        }
    }
    if ads_scale != 1.0 {
        facts.ads_spread *= ads_scale;
        facts.ads_aim_pitch *= ads_scale;
        facts.ads_zoom_fov *= ads_scale;
        facts.ads_in_rate *= ads_scale;
        facts.ads_out_rate *= ads_scale;
    }

    if let Some(sight) = iw5_first_block(assets, |a| a.sight) {
        facts.aim_down_sight = sight.aim_down_sight;
        facts.can_hold_breath = sight.can_hold_breath;
        facts.ads_fire_only = sight.ads_fire;
        facts.rechamber_while_ads = sight.rechamber_while_ads;
        facts.no_ads_when_mag_empty = sight.no_ads_when_mag_empty;
    }
    if let Some(general) = iw5_first_block(assets, |a| a.ammo_general) {
        facts.penetrate_type = general.penetrate_type;
        facts.penetrate_multiplier = general.penetrate_multiplier;
        facts.impact_type = general.impact_type;
        facts.fire_type = general.fire_type;
        facts.rifle_bullet = general.rifle_bullet;
    }
    if let Some(reload) = iw5_first_block(assets, |a| a.reload) {
        facts.no_partial_reload = reload.no_partial_reload;
        facts.segmented_reload = reload.segmented_reload;
    }
    if let Some(add_ons) = iw5_first_block(assets, |a| a.add_ons) {
        facts.motion_tracker = add_ons.motion_tracker;
        facts.silenced = add_ons.silenced;
    }
    if let Some(general) = iw5_first_block(assets, |a| a.general) {
        facts.bolt_action = general.bolt_action;
        facts.inherits_perks = general.inherits_perks;
        facts.move_speed_scale = general.move_speed_scale;
        facts.ads_move_speed_scale = general.ads_move_speed_scale;
    }

    if let Some(ammo) = iw5_first_block(assets, |a| a.ammunition) {
        facts.max_ammo = ammo.max_ammo;
        facts.start_ammo = ammo.start_ammo;
        facts.clip_size = ammo.clip_size;
        facts.shots_per_fire = ammo.shot_count;
        facts.reload_ammo_add = ammo.reload_ammo_add;
        facts.reload_start_add = ammo.reload_start_add;
    }
    let ammo_scale = iw5_scale_product(assets, |s| s.ammunition);
    facts.max_ammo = scale_i32(facts.max_ammo, ammo_scale);
    facts.start_ammo = scale_i32(facts.start_ammo, ammo_scale);
    facts.clip_size = scale_i32(facts.clip_size, ammo_scale);

    if let Some(damage) = iw5_first_block(assets, |a| a.damage) {
        facts.damage = damage.damage;
        facts.min_damage = damage.min_damage;
        facts.melee_damage = damage.melee_damage;
        facts.max_damage_range = damage.max_damage_range;
        facts.min_damage_range = damage.min_damage_range;
        facts.min_player_damage = damage.min_player_damage;
    }
    facts.damage = scale_i32(facts.damage, iw5_scale_product(assets, |s| s.damage));
    let damage_min = iw5_scale_product(assets, |s| s.damage_min);
    facts.min_damage = scale_i32(facts.min_damage, damage_min);
    facts.min_player_damage = scale_i32(facts.min_player_damage, damage_min);

    if let Some(location) = iw5_first_block(assets, |a| a.location_damage) {
        let mut mult = facts.location_damage_mult.unwrap_or([1.0; 20]);
        mult[..19].copy_from_slice(&location);
        facts.location_damage_mult = Some(mult);
    }

    if let Some(idle) = iw5_first_block(assets, |a| a.idle_settings) {
        facts.idle.hip_idle_amount = idle.hip_idle_amount;
        facts.idle.hip_idle_speed = idle.hip_idle_speed;
        facts.idle.idle_crouch_factor = idle.idle_crouch_factor;
        facts.idle.idle_prone_factor = idle.idle_prone_factor;
    }
    let idle_scale = iw5_scale_product(assets, |s| s.idle_settings);
    facts.idle.hip_idle_amount *= idle_scale;
    facts.idle.ads_idle_amount *= idle_scale;

    if let Some(spread) = iw5_first_block(assets, |a| a.hip_spread) {
        let v = spread.values;
        apply_leftover_hip_spread(
            facts,
            [
                v[0], v[1], v[2], v[3], v[4], v[5], v[9], v[6], v[7], v[8], v[10], v[11],
            ],
        );
    }
    let spread_scale = iw5_scale_product(assets, |s| s.hip_spread);
    if spread_scale != 1.0 {
        for value in [
            &mut facts.hip_spread_stand_min,
            &mut facts.hip_spread_ducked_min,
            &mut facts.hip_spread_prone_min,
            &mut facts.hip_spread_stand_max,
            &mut facts.hip_spread_ducked_max,
            &mut facts.hip_spread_prone_max,
        ] {
            *value *= spread_scale;
        }
    }

    let kick = &mut facts.kick;
    if let Some(gun) = iw5_first_block(assets, |a| a.gun_kick) {
        kick.hip_gun_kick_reduced_kick_bullets = gun.hip_reduced_kick_bullets;
        [
            kick.hip_gun_kick_reduced_kick_percent,
            kick.hip_gun_kick_pitch_min,
            kick.hip_gun_kick_pitch_max,
            kick.hip_gun_kick_yaw_min,
            kick.hip_gun_kick_yaw_max,
            kick.hip_gun_kick_accel,
            kick.hip_gun_kick_speed_max,
            kick.hip_gun_kick_speed_decay,
            kick.hip_gun_kick_static_decay,
        ] = gun.hip;
        kick.ads_gun_kick_reduced_kick_bullets = gun.ads_reduced_kick_bullets;
        [
            kick.ads_gun_kick_reduced_kick_percent,
            kick.ads_gun_kick_pitch_min,
            kick.ads_gun_kick_pitch_max,
            kick.ads_gun_kick_yaw_min,
            kick.ads_gun_kick_yaw_max,
            kick.ads_gun_kick_accel,
            kick.ads_gun_kick_speed_max,
            kick.ads_gun_kick_speed_decay,
            kick.ads_gun_kick_static_decay,
        ] = gun.ads;
    }
    let gun_scale = iw5_scale_product(assets, |s| s.gun_kick);
    if gun_scale != 1.0 {
        for value in [
            &mut kick.hip_gun_kick_pitch_min,
            &mut kick.hip_gun_kick_pitch_max,
            &mut kick.hip_gun_kick_yaw_min,
            &mut kick.hip_gun_kick_yaw_max,
            &mut kick.ads_gun_kick_pitch_min,
            &mut kick.ads_gun_kick_pitch_max,
            &mut kick.ads_gun_kick_yaw_min,
            &mut kick.ads_gun_kick_yaw_max,
        ] {
            *value *= gun_scale;
        }
    }
    if let Some(view) = iw5_first_block(assets, |a| a.view_kick) {
        [
            kick.hip_view_kick_pitch_min,
            kick.hip_view_kick_pitch_max,
            kick.hip_view_kick_yaw_min,
            kick.hip_view_kick_yaw_max,
            kick.f_hip_view_kick_center_speed,
            kick.ads_view_kick_pitch_min,
            kick.ads_view_kick_pitch_max,
            kick.ads_view_kick_yaw_min,
            kick.ads_view_kick_yaw_max,
            kick.f_ads_view_kick_center_speed,
        ] = view;
    }
    let view_scale = iw5_scale_product(assets, |s| s.view_kick);
    if view_scale != 1.0 {
        for value in [
            &mut kick.hip_view_kick_pitch_min,
            &mut kick.hip_view_kick_pitch_max,
            &mut kick.hip_view_kick_yaw_min,
            &mut kick.hip_view_kick_yaw_max,
            &mut kick.ads_view_kick_pitch_min,
            &mut kick.ads_view_kick_pitch_max,
            &mut kick.ads_view_kick_yaw_min,
            &mut kick.ads_view_kick_yaw_max,
        ] {
            *value *= view_scale;
        }
    }
    let center_scale = iw5_scale_product(assets, |s| s.view_center);
    kick.f_hip_view_kick_center_speed *= center_scale;
    kick.f_ads_view_kick_center_speed *= center_scale;

    facts.fire_time_ms = scale_i32(
        facts.fire_time_ms,
        iw5_scale_product(assets, |s| s.fire_timers),
    );
    let state = iw5_scale_product(assets, |s| s.state_timers);
    if state != 1.0 {
        for value in [
            &mut facts.fire_delay_ms,
            &mut facts.melee_delay_ms,
            &mut facts.melee_charge_delay_ms,
            &mut facts.rechamber_time_ms,
            &mut facts.rechamber_bolt_time_ms,
            &mut facts.hold_fire_time_ms,
            &mut facts.melee_time_ms,
            &mut facts.melee_charge_time_ms,
            &mut facts.reload_time_ms,
            &mut facts.reload_show_rocket_time_ms,
            &mut facts.reload_empty_time_ms,
            &mut facts.reload_add_time_ms,
            &mut facts.reload_start_time_ms,
            &mut facts.reload_start_add_time_ms,
            &mut facts.reload_end_time_ms,
            &mut facts.drop_time_ms,
            &mut facts.raise_time_ms,
            &mut facts.quick_drop_time_ms,
            &mut facts.quick_raise_time_ms,
            &mut facts.sprint_raise_time_ms,
            &mut facts.sprint_loop_time_ms,
            &mut facts.sprint_drop_time_ms,
        ] {
            *value = scale_i32(*value, state);
        }
    }
}

pub(super) fn iw5_anim_timer(facts: &mut WeaponBodyFacts, slot: usize) -> Option<&mut i32> {
    Some(match slot {
        weap_anim::FIRE => &mut facts.fire_time_ms,
        weap_anim::RECHAMBER => &mut facts.rechamber_time_ms,
        weap_anim::MELEE => &mut facts.melee_time_ms,
        weap_anim::MELEE_CHARGE => &mut facts.melee_charge_time_ms,
        weap_anim::RELOAD => &mut facts.reload_time_ms,
        weap_anim::RELOAD_EMPTY => &mut facts.reload_empty_time_ms,
        weap_anim::RELOAD_START => &mut facts.reload_start_time_ms,
        weap_anim::RELOAD_END => &mut facts.reload_end_time_ms,
        weap_anim::ALT_RAISE => &mut facts.alternate_raise_time_ms,
        weap_anim::ALT_DROP => &mut facts.alternate_drop_time_ms,
        weap_anim::RAISE => &mut facts.raise_time_ms,
        weap_anim::DROP => &mut facts.drop_time_ms,
        weap_anim::QUICK_RAISE => &mut facts.quick_raise_time_ms,
        weap_anim::QUICK_DROP => &mut facts.quick_drop_time_ms,
        weap_anim::SPRINT_IN => &mut facts.sprint_raise_time_ms,
        weap_anim::SPRINT_LOOP => &mut facts.sprint_loop_time_ms,
        weap_anim::SPRINT_OUT => &mut facts.sprint_drop_time_ms,
        _ => return None,
    })
}
