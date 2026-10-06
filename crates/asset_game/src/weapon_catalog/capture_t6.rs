use super::*;

impl WeaponCatalog {
    pub fn capture_t6(&mut self, weapon: fastfile_t6::weapon::WeaponView<'_>) {
        use fastfile_t6::weapon::variant as v;
        let Some(name) = weapon.name().filter(|name| !name.is_empty()) else {
            return;
        };
        self.entries.push(CatalogWeapon {
            namespace: self
                .capture_ns
                .expect("asset capture requires an explicit family"),
            name: name.to_owned(),
            alternate_weapon: weapon
                .variant_str(v::ALT_WEAPON_NAME)
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            weap_def: None,
            display_name_key: weapon
                .variant_str(v::DISPLAY_NAME)
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            reticle: WeaponReticleAssets::default(),
            hud_material_edges: WeaponHudMaterialEdges::default(),
            reticle_center_slot: None,
            reticle_side_slot: None,
            overlay_material: weapon
                .variant_asset_name(v::OVERLAY_MATERIAL)
                .map(str::to_owned),
            overlay_image: weapon
                .variant_asset_name(v::OVERLAY_MATERIAL)
                .map(str::to_owned),
            overlay_material_slot: None,
            iw5_attachment_slots: std::array::from_fn(|_| None),
            attached_models: [true, false].map(|view| t6_attached_models(weapon, view)),
            t6_clip_models: [true, false].map(|view| {
                weapon
                    .attached_model(T6_CLIP_SLOT, view)
                    .map(|(name, _, _)| t6_model_name(name))
            }),
            t6_attachments: weapon
                .attachment_uniques()
                .filter_map(|unique| capture_t6_attachment(unique))
                .collect(),
            t6_attachment_stats: weapon
                .attachments()
                .map(capture_t6_attachment_stats)
                .collect(),
            camo_models: WeaponCamoModels::default(),
            skin_parent: None,
            iw5_reload_overrides: Vec::new(),
            iw5_anim_overrides: Vec::new(),
            iw5_fx_overrides: Vec::new(),
            iw5_notetrack_overrides: Vec::new(),
            hud_icon: weapon
                .def_loaded_asset_name(fastfile_t6::weapon::def::HUD_ICON)
                .map(t6_model_name),
            hud_icon_slot: None,
            pickup_icon: None,
            pickup_icon_slot: None,
            pickup_icon_image: None,
            pickup_icon_ratio: 0,
            hud_icon_ratio: 0,
            hud_icon_image: None,
            dpad_icon: None,
            dpad_icon_image: None,
            dpad_icon_atlas: None,
            dpad_icon_ratio: 0,
            kill_icon: None,
            kill_icon_slot: None,
            kill_icon_image: None,
            proj_trail: None,
            proj_trail_slot: None,
            proj_beacon: None,
            proj_beacon_slot: None,
            proj_ignition: None,
            proj_ignition_slot: None,
            projectile_fx: WeaponProjectileFx::default(),
            gun_xmodel: weapon
                .def_asset_array_name(fastfile_t6::weapon::def::GUN_XMODEL, 0)
                .map(t6_model_name),
            hand_xmodel: None,
            knife_xmodel: None,
            world_model: weapon
                .def_asset_array_name(fastfile_t6::weapon::def::WORLD_MODEL, 0)
                .map(t6_model_name),
            projectile_model: weapon
                .def_asset_name(fastfile_t6::weapon::def::PROJECTILE_MODEL)
                .map(t6_model_name)
                .or_else(|| crate::weapon_t6::planted_model(name).map(str::to_owned)),
            rocket_model: None,
            sz_xanims: t6_sz_xanims(weapon),
            dual_wield_weapon: None,
            impact_payload: None,
            sz_xanims_right: [const { None }; WEAPON_ANIM_SLOTS],
            sz_xanims_left: t6_sz_xanims_left(weapon),
            hide_tags: Vec::new(),
            sounds: capture_t6_sounds(weapon),
            combat_fx: WeaponCombatFx {
                explosion_hint: weapon
                    .def_asset_name(fastfile_t6::weapon::def::PROJ_EXPLOSION_EFFECT)
                    .map(str::to_owned),
                ..WeaponCombatFx::empty(crate::AssetNamespace::T6)
            },
            combat_slots: CombatFxSlots::default(),
            facts: capture_t6_body_facts(weapon),
        });
    }
}

pub(super) fn t6_sz_xanims(
    w: fastfile_t6::weapon::WeaponView<'_>,
) -> [Option<String>; WEAPON_ANIM_SLOTS] {
    t6_sz_xanims_by(|slot| w.xanim(slot))
}

pub(super) fn t6_sz_xanims_by<'a>(
    xanim: impl Fn(u32) -> Option<&'a str>,
) -> [Option<String>; WEAPON_ANIM_SLOTS] {
    use fastfile_t6::weapon::weap_anim as t6_anim;
    const PAIRS: [(usize, usize); 33] = [
        (t6_anim::IDLE, weap_anim::IDLE),
        (t6_anim::EMPTY_IDLE, weap_anim::EMPTY_IDLE),
        (t6_anim::FIRE, weap_anim::FIRE),
        (t6_anim::HOLD_FIRE, weap_anim::HOLD_FIRE),
        (t6_anim::LASTSHOT, weap_anim::LASTSHOT),
        (t6_anim::RECHAMBER, weap_anim::RECHAMBER),
        (t6_anim::MELEE, weap_anim::MELEE),
        (t6_anim::MELEE_CHARGE, weap_anim::MELEE_CHARGE),
        (t6_anim::RELOAD, weap_anim::RELOAD),
        (t6_anim::RELOAD_EMPTY, weap_anim::RELOAD_EMPTY),
        (t6_anim::RELOAD_START, weap_anim::RELOAD_START),
        (t6_anim::RELOAD_END, weap_anim::RELOAD_END),
        (t6_anim::RELOAD_QUICK, weap_anim_extra::RELOAD_QUICK),
        (
            t6_anim::RELOAD_QUICK_EMPTY,
            weap_anim_extra::RELOAD_QUICK_EMPTY,
        ),
        (t6_anim::RAISE, weap_anim::RAISE),
        (t6_anim::FIRST_RAISE, weap_anim::FIRST_RAISE),
        (t6_anim::DROP, weap_anim::DROP),
        (t6_anim::ALT_RAISE, weap_anim::ALT_RAISE),
        (t6_anim::ALT_DROP, weap_anim::ALT_DROP),
        (t6_anim::QUICK_RAISE, weap_anim::QUICK_RAISE),
        (t6_anim::QUICK_DROP, weap_anim::QUICK_DROP),
        (t6_anim::EMPTY_RAISE, weap_anim::EMPTY_RAISE),
        (t6_anim::EMPTY_DROP, weap_anim::EMPTY_DROP),
        (t6_anim::SPRINT_IN, weap_anim::SPRINT_IN),
        (t6_anim::SPRINT_LOOP, weap_anim::SPRINT_LOOP),
        (t6_anim::SPRINT_OUT, weap_anim::SPRINT_OUT),
        (t6_anim::DETONATE, weap_anim::DETONATE),
        (t6_anim::ADS_FIRE, weap_anim::ADS_FIRE),
        (t6_anim::ADS_LASTSHOT, weap_anim::ADS_LASTSHOT),
        (t6_anim::ADS_RECHAMBER, weap_anim::ADS_RECHAMBER),
        (t6_anim::ADS_UP, weap_anim::ADS_UP),
        (t6_anim::ADS_DOWN, weap_anim::ADS_DOWN),
        (t6_anim::FIRE_INTRO, weap_anim::FIRE),
    ];
    let mut out = [const { None }; WEAPON_ANIM_SLOTS];
    for (src, dst) in PAIRS {
        if out[dst].is_none() {
            out[dst] = xanim(src as u32)
                .filter(|name| !name.is_empty())
                .map(|name| format!("{T6_XANIM_PREFIX}{}", name.to_ascii_lowercase()));
        }
    }
    out
}

pub(super) fn t6_sz_xanims_left(
    w: fastfile_t6::weapon::WeaponView<'_>,
) -> [Option<String>; WEAPON_ANIM_SLOTS] {
    use fastfile_t6::weapon::weap_anim as t6_anim;
    const LEFT: [(usize, usize); 6] = [
        (t6_anim::DW_LEFT_IDLE, weap_anim::IDLE),
        (t6_anim::DW_LEFT_EMPTY_IDLE, weap_anim::EMPTY_IDLE),
        (t6_anim::DW_LEFT_FIRE, weap_anim::FIRE),
        (t6_anim::DW_LEFT_LASTSHOT, weap_anim::LASTSHOT),
        (t6_anim::DW_LEFT_RELOAD, weap_anim::RELOAD),
        (t6_anim::DW_LEFT_RELOAD_EMPTY, weap_anim::RELOAD_EMPTY),
    ];
    let clip = |slot: usize| {
        w.xanim(slot as u32)
            .filter(|name| !name.is_empty())
            .map(|name| format!("{T6_XANIM_PREFIX}{}", name.to_ascii_lowercase()))
    };
    if clip(t6_anim::DW_LEFT_IDLE).is_none() {
        return [const { None }; WEAPON_ANIM_SLOTS];
    }
    let mut out = t6_sz_xanims(w);
    for (src, dst) in LEFT {
        out[dst] = clip(src);
    }
    out
}

pub fn t6_weapon_xanim_names(w: fastfile_t6::weapon::WeaponView<'_>) -> Vec<String> {
    (0..fastfile_t6::weapon::variant::XANIM_COUNT)
        .filter_map(|slot| w.xanim(slot).filter(|name| !name.is_empty()))
        .map(str::to_ascii_lowercase)
        .collect()
}

pub(super) fn remap_t6_weap_class(raw: i32) -> i32 {
    use fastfile_t6::weapon::weap_class as t6;
    match raw {
        t6::RIFLE => 0,
        t6::MG => 2,
        t6::SMG => 3,
        t6::SPREAD | t6::PISTOL_SPREAD => weapon_iw4::WEAPCLASS_SPREAD,
        t6::PISTOL => weapon_iw4::WEAPCLASS_PISTOL,
        t6::GRENADE => weapon_iw4::WEAPCLASS_GRENADE,
        t6::ROCKETLAUNCHER => 7,
        t6::TURRET => weapon_iw4::WEAPCLASS_TURRET,
        t6::ITEM => 11,
        _ => 10,
    }
}

pub(super) fn remap_t6_weap_type(raw: i32) -> i32 {
    use fastfile_t6::weapon::weap_type as t6;
    match raw {
        t6::GRENADE | t6::GAS | t6::BOMB | t6::MINE => weapon_iw4::WEAPTYPE_GRENADE,
        t6::PROJECTILE => weapon_iw4::WEAPTYPE_PROJECTILE,
        t6::RIOTSHIELD => 3,
        _ => weapon_iw4::WEAPTYPE_BULLET,
    }
}

pub(super) fn capture_t6_body_facts(w: fastfile_t6::weapon::WeaponView<'_>) -> WeaponBodyFacts {
    use fastfile_t6::weapon::{def as d, variant as v};
    let ads_in_ms = w.variant_i32(v::ADS_TRANS_IN_TIME);
    let ads_out_ms = w.variant_i32(v::ADS_TRANS_OUT_TIME);
    let curve = w.damage_curve();
    let (near, near_range) = curve[0];
    let (far, far_range) = curve[fastfile_t6::weapon::def::DAMAGE_STEPS - 1];
    let mut facts = WeaponBodyFacts {
        body_resolved: w.has_def(),
        fire_time_ms: w.def_i32(d::FIRE_TIME),
        clip_size: w.variant_i32(v::CLIP_SIZE),
        weap_type: remap_t6_weap_type(w.def_i32(d::WEAP_TYPE)),
        weap_class: remap_t6_weap_class(w.def_i32(d::WEAP_CLASS)),
        player_anim_type: w.def_i32(d::PLAYER_ANIM_TYPE),
        fire_type: w.def_i32(d::FIRE_TYPE),
        inventory_type: w.def_i32(d::INVENTORY_TYPE),
        penetrate_type: w.def_i32(d::PENETRATE_TYPE),
        impact_type: w.def_i32(d::IMPACT_TYPE),
        move_speed_scale: w.def_f32(d::MOVE_SPEED_SCALE),
        ads_move_speed_scale: w.def_f32(d::ADS_MOVE_SPEED_SCALE),
        rechamber_time_ms: w.def_i32(d::RECHAMBER_TIME),
        rechamber_bolt_time_ms: w.def_i32(d::RECHAMBER_BOLT_TIME),
        drop_time_ms: w.def_i32(d::DROP_TIME),
        raise_time_ms: w.def_i32(d::RAISE_TIME),
        alternate_raise_time_ms: w.variant_i32(v::ALT_RAISE_TIME),
        alternate_drop_time_ms: w.def_i32(d::ALT_DROP_TIME),
        first_raise_time_ms: w.def_i32(d::FIRST_RAISE_TIME),
        quick_drop_time_ms: w.def_i32(d::QUICK_DROP_TIME),
        quick_raise_time_ms: w.def_i32(d::QUICK_RAISE_TIME),
        bolt_action: w.def_bool(d::BOLT_ACTION),
        select_requires_ammo: Some(false),
        offhand_hold_is_cancelable: Some(w.def_bool(d::OFFHAND_HOLD_IS_CANCELABLE)),
        inherits_perks: true,
        reload_time_ms: w.variant_i32(v::RELOAD_TIME),
        reload_empty_time_ms: w.variant_i32(v::RELOAD_EMPTY_TIME),
        ads_in_rate: if ads_in_ms > 0 {
            1.0 / ads_in_ms as f32
        } else {
            0.0
        },
        ads_out_rate: if ads_out_ms > 0 {
            1.0 / ads_out_ms as f32
        } else {
            0.0
        },
        ads_zoom_fov: w.variant_f32(v::ADS_ZOOM_FOV1),
        ads_zoom_in_frac: w.variant_f32(v::ADS_ZOOM_IN_FRAC),
        ads_zoom_out_frac: w.variant_f32(v::ADS_ZOOM_OUT_FRAC),
        ammo_counter_clip: w.def_i32(d::AMMO_COUNTER_CLIP),
        start_ammo: w.def_i32(d::START_AMMO),
        max_ammo: w.def_i32(d::MAX_AMMO),
        ammo_count_clip_relative: w.def_bool(d::AMMO_COUNT_CLIP_RELATIVE),
        shots_per_fire: w.def_i32(d::SHOT_COUNT).max(1),
        damage: near,
        max_damage_range: near_range,
        min_damage: far,
        min_damage_range: far_range,
        min_player_damage: w.def_i32(d::MIN_PLAYER_DAMAGE),
        explosion_radius: w.def_i32(d::EXPLOSION_RADIUS),
        explosion_radius_min: w.def_i32(d::EXPLOSION_RADIUS_MIN),
        explosion_inner_damage: w.def_i32(d::EXPLOSION_INNER_DAMAGE),
        explosion_outer_damage: w.def_i32(d::EXPLOSION_OUTER_DAMAGE),
        damage_cone_angle: w.def_f32(d::DAMAGE_CONE_ANGLE),
        projectile_speed: w.def_i32(d::PROJECTILE_SPEED),
        projectile_speed_up: w.def_i32(d::PROJECTILE_SPEED_UP),
        projectile_speed_relative_up: w.def_i32(d::PROJECTILE_SPEED_RELATIVE_UP),
        projectile_speed_forward: w.def_i32(d::PROJECTILE_SPEED_FORWARD),
        projectile_activate_dist: w.def_i32(d::PROJECTILE_ACTIVATE_DIST),
        projectile_explosion_type: w.def_i32(d::PROJ_EXPLOSION),
        proj_impact_explode: w.def_bool(d::PROJ_IMPACT_EXPLODE),
        stickiness: w.def_i32(d::STICKINESS),
        timed_detonation: w.def_bool(d::TIMED_DETONATION),
        has_detonator: w.def_bool(d::HAS_DETONATOR),
        refuses_pickup: !w.def_bool(d::RETRIEVABLE),
        detonate_delay_ms: w.def_i32(d::DETONATE_DELAY),
        detonate_time_ms: w.def_i32(d::DETONATE_TIME),
        offhand_class: leftover_t5_offhand_class(w.def_i32(d::OFFHAND_CLASS)),
        hold_fire_time_ms: w.def_i32(d::HOLD_FIRE_TIME),
        fuse_time_ms: w.def_i32(d::FUSE_TIME),
        cook_off_hold: w.def_bool(d::COOK_OFF_HOLD),
        fire_delay_ms: w.def_i32(d::FIRE_DELAY),
        melee_damage: w.def_i32(d::MELEE_DAMAGE),
        melee_time_ms: w.def_i32(d::MELEE_TIME),
        melee_delay_ms: w.def_i32(d::MELEE_DELAY),
        melee_charge_time_ms: w.def_i32(d::MELEE_CHARGE_TIME),
        melee_charge_delay_ms: w.def_i32(d::MELEE_CHARGE_DELAY),
        use_as_melee: w.def_bool(d::USE_AS_MELEE),
        reload_show_rocket_time_ms: w.def_i32(d::RELOAD_SHOW_ROCKET_TIME),
        reload_add_time_ms: w.def_i32(d::RELOAD_ADD_TIME),
        reload_empty_add_time_ms: w.def_i32(d::RELOAD_EMPTY_ADD_TIME),
        reload_start_time_ms: w.def_i32(d::RELOAD_START_TIME),
        reload_start_add_time_ms: w.def_i32(d::RELOAD_START_ADD_TIME),
        reload_end_time_ms: w.def_i32(d::RELOAD_END_TIME),
        reload_ammo_add: w.def_i32(d::RELOAD_AMMO_ADD),
        reload_start_add: w.def_i32(d::RELOAD_START_ADD),
        overlay_reticle: w.def_i32(d::OVERLAY_RETICLE),
        overlay_interface: w.def_i32(d::OVERLAY_INTERFACE),
        ads_overlay_width: w.def_f32(d::OVERLAY_WIDTH),
        ads_overlay_height: w.def_f32(d::OVERLAY_HEIGHT),
        hip_reticle_side_pos: w.def_f32(d::HIP_RETICLE_SIDE_POS),
        no_ads_when_mag_empty: w.def_bool(d::NO_ADS_WHEN_MAG_EMPTY),
        aim_down_sight: w.def_bool(d::AIM_DOWN_SIGHT),
        rechamber_while_ads: w.def_bool(d::RECHAMBER_WHILE_ADS),
        ads_fire_only: w.def_bool(d::ADS_FIRE_ONLY),
        no_partial_reload: w.def_bool(d::NO_PARTIAL_RELOAD),
        segmented_reload: w.def_bool(d::SEGMENTED_RELOAD),
        ads_spread: w.def_f32(d::ADS_SPREAD),
        kill_icon_ratio: w.def_i32(d::KILL_ICON_RATIO),
        flip_kill_icon: w.def_bool(d::FLIP_KILL_ICON),
        idle: WeaponIdleInputs {
            ads_idle_amount: w.def_f32(d::ADS_IDLE_AMOUNT),
            hip_idle_amount: w.def_f32(d::HIP_IDLE_AMOUNT),
            ads_idle_speed: w.def_f32(d::ADS_IDLE_SPEED),
            hip_idle_speed: w.def_f32(d::HIP_IDLE_SPEED),
            idle_crouch_factor: w.def_f32(d::IDLE_CROUCH_FACTOR),
            idle_prone_factor: w.def_f32(d::IDLE_PRONE_FACTOR),
        },
        kick: capture_t6_kick(w),
        ..WeaponBodyFacts::default()
    };
    apply_leftover_hip_spread(
        &mut facts,
        core::array::from_fn(|i| w.def_f32(d::HIP_SPREAD_STAND_MIN + 4 * i as u32)),
    );
    facts.parallel_bounce = w
        .def_f32_array::<{ d::SURF_TYPE_COUNT }>(d::PARALLEL_BOUNCE)
        .map(t6_surface_table);
    facts.perpendicular_bounce = w
        .def_f32_array::<{ d::SURF_TYPE_COUNT }>(d::PERPENDICULAR_BOUNCE)
        .map(t6_surface_table);
    if w.variant_bool(v::DUAL_MAG) {
        facts.dual_mag = Some(weapon_iw4::DualMagTimes {
            reload_ms: w.variant_i32(v::RELOAD_QUICK_TIME),
            reload_empty_ms: w.variant_i32(v::RELOAD_QUICK_EMPTY_TIME),
            add_ms: w.def_i32(d::RELOAD_QUICK_ADD_TIME),
            empty_add_ms: w.def_i32(d::RELOAD_QUICK_EMPTY_ADD_TIME),
        });
    }
    facts
}

pub(super) fn t6_surface_table(t6: [f32; fastfile_t6::weapon::def::SURF_TYPE_COUNT]) -> [f32; 31] {
    const T6_RIOT_SHIELD: usize = 31;
    const T6_SNOW: usize = 19;
    core::array::from_fn(|iw4| match iw4 {
        0..=28 => t6[iw4],
        29 => t6[T6_RIOT_SHIELD],
        _ => t6[T6_SNOW],
    })
}

pub(super) fn capture_t6_sounds(w: fastfile_t6::weapon::WeaponView<'_>) -> WeaponSoundAliases {
    use fastfile_t6::weapon::def as d;
    let name = |off| w.def_str(off).filter(|s| !s.is_empty()).map(str::to_owned);
    let fire = name(d::FIRE_SOUND);
    let fire_player = match (name(d::FIRE_SOUND_PLAYER), &fire) {
        (Some(player), Some(npc)) if player.to_ascii_lowercase().ends_with("_lfe") => npc
            .strip_suffix("_fire_npc")
            .map_or(Some(player), |base| Some(format!("{base}_fire_plr"))),
        (player, _) => player,
    };
    WeaponSoundAliases {
        fire,
        fire_player,
        fire_last: name(d::FIRE_LAST_SOUND),
        fire_last_player: name(d::FIRE_LAST_SOUND_PLAYER),
        empty_fire: name(d::EMPTY_FIRE_SOUND),
        empty_fire_player: name(d::EMPTY_FIRE_SOUND_PLAYER),
        melee_swipe: name(d::MELEE_SWIPE_SOUND),
        melee_swipe_player: name(d::MELEE_SWIPE_SOUND_PLAYER),
        melee_hit: name(d::MELEE_HIT_SOUND),
        melee_miss: name(d::MELEE_MISS_SOUND),
        pullback: name(d::PULLBACK_SOUND),
        pullback_player: name(d::PULLBACK_SOUND_PLAYER),
        raise: name(d::RAISE_SOUND),
        raise_player: name(d::RAISE_SOUND_PLAYER),
        first_raise: name(d::FIRST_RAISE_SOUND),
        first_raise_player: name(d::FIRST_RAISE_SOUND_PLAYER),
        putaway: name(d::PUTAWAY_SOUND),
        putaway_player: name(d::PUTAWAY_SOUND_PLAYER),
        proj_explosion: name(d::PROJ_EXPLOSION_SOUND),
        ..WeaponSoundAliases::default()
    }
}

pub(super) fn capture_t6_attachment_stats(
    a: fastfile_t6::weapon::AttachmentView<'_>,
) -> T6AttachmentStats {
    use fastfile_t6::weapon::attachment as at;
    let scale = |off| Some(a.f32_at(off)).filter(|v| *v > 0.0).unwrap_or(1.0);
    let set = |off| Some(a.f32_at(off)).filter(|v| *v > 0.0);
    T6AttachmentStats {
        kind: a.attachment_type(),
        clip_size_scale: scale(at::CLIP_SIZE_SCALE),
        fire_time_scale: scale(at::FIRE_TIME_SCALE),
        reload_time_scales: std::array::from_fn(|i| scale(at::RELOAD_TIME_SCALES + 4 * i as u32)),
        ads_in_time_scale: scale(at::ADS_TRANS_IN_TIME_SCALE),
        ads_out_time_scale: scale(at::ADS_TRANS_OUT_TIME_SCALE),
        ads_zoom_fov: set(at::ADS_ZOOM_FOV).filter(|fov| *fov > 1.0),
        ads_zoom_in_frac: set(at::ADS_ZOOM_IN_FRAC),
        ads_zoom_out_frac: set(at::ADS_ZOOM_OUT_FRAC),
        damage_range_scale: scale(at::DAMAGE_RANGE_SCALE),
        hip_spread_min_scale: scale(at::HIP_SPREAD_MIN_SCALE),
        hip_spread_max_scale: scale(at::HIP_SPREAD_MAX_SCALE),
        ads_move_speed_scale: scale(at::ADS_MOVE_SPEED_SCALE),
        ads_view_kick_center_speed_scale: scale(at::ADS_VIEW_KICK_CENTER_SPEED_SCALE),
        ads_idle_amount_scale: scale(at::ADS_IDLE_AMOUNT_SCALE),
        penetrating: a.u32_at(at::PERKS) != 0,
        dual_mag: a.flag(at::DUAL_MAG),
        shared_ammo: a.flag(at::SHARED_AMMO),
    }
}

pub(super) fn apply_t6_attachment_stats(facts: &mut WeaponBodyFacts, stats: &T6AttachmentStats) {
    let ms = |value: i32, scale: f32| (value as f32 * scale).round() as i32;
    if stats.clip_size_scale != 1.0 && facts.clip_size > 0 {
        facts.clip_size = (facts.clip_size as f32 * stats.clip_size_scale).round() as i32;
    }
    facts.fire_time_ms = ms(facts.fire_time_ms, stats.fire_time_scale);
    let [reload, empty, add, _, _] = stats.reload_time_scales;
    facts.reload_time_ms = ms(facts.reload_time_ms, reload);
    facts.reload_empty_time_ms = ms(facts.reload_empty_time_ms, empty);
    facts.reload_add_time_ms = ms(facts.reload_add_time_ms, add);
    facts.reload_empty_add_time_ms = ms(facts.reload_empty_add_time_ms, empty);
    facts.reload_start_time_ms = ms(facts.reload_start_time_ms, reload);
    facts.reload_end_time_ms = ms(facts.reload_end_time_ms, reload);
    facts.ads_in_rate /= stats.ads_in_time_scale;
    facts.ads_out_rate /= stats.ads_out_time_scale;
    if let Some(fov) = stats.ads_zoom_fov {
        facts.ads_zoom_fov = fov;
    }
    if let Some(frac) = stats.ads_zoom_in_frac {
        facts.ads_zoom_in_frac = frac;
    }
    if let Some(frac) = stats.ads_zoom_out_frac {
        facts.ads_zoom_out_frac = frac;
    }
    facts.max_damage_range *= stats.damage_range_scale;
    facts.min_damage_range *= stats.damage_range_scale;
    for min in [
        &mut facts.hip_spread_stand_min,
        &mut facts.hip_spread_ducked_min,
        &mut facts.hip_spread_prone_min,
    ] {
        *min *= stats.hip_spread_min_scale;
    }
    for max in [
        &mut facts.hip_spread_stand_max,
        &mut facts.hip_spread_ducked_max,
        &mut facts.hip_spread_prone_max,
    ] {
        *max *= stats.hip_spread_max_scale;
    }
    if stats.ads_move_speed_scale != 1.0 {
        facts.ads_move_speed_scale = (facts.ads_move_speed_scale * stats.ads_move_speed_scale)
            .min(1.0 / ADS_WALK_SPEED_SCALE);
    }
    facts.kick.f_ads_view_kick_center_speed *= stats.ads_view_kick_center_speed_scale;
    facts.idle.ads_idle_amount *= stats.ads_idle_amount_scale;
    if stats.dual_mag && facts.segmented_reload {
        facts.reload_ammo_add = facts.reload_ammo_add.max(1) * 2;
    }
    if stats.penetrating {
        facts.penetrate_multiplier *= 2.0;
    }
}

pub fn t6_attachment_models(
    unique: fastfile_t6::weapon::AttachmentUniqueView<'_>,
    view: bool,
) -> Vec<T6AttachmentModel> {
    let Some(owner) = unique.name() else {
        return Vec::new();
    };
    unique
        .models(view)
        .into_iter()
        .flatten()
        .map(|(model, tag, offset, angles)| {
            let model = t6_model_name(model);
            T6AttachmentModel {
                copy: format!("{model}@{owner}"),
                model,
                tag: tag.map(str::to_owned),
                offset,
                angles,
            }
        })
        .collect()
}

pub fn t6_attachment_ads_model(
    unique: fastfile_t6::weapon::AttachmentUniqueView<'_>,
) -> Option<T6AttachmentModel> {
    let owner = unique.name()?;
    let (model, tag, offset, angles) = unique.ads_model()?;
    let model = t6_model_name(model);
    Some(T6AttachmentModel {
        copy: format!("{model}@{owner}"),
        model,
        tag: tag.map(str::to_owned),
        offset,
        angles,
    })
}

pub(super) fn capture_t6_attachment(
    unique: fastfile_t6::weapon::AttachmentUniqueView<'_>,
) -> Option<T6Attachment> {
    use fastfile_t6::weapon::unique as u;
    unique.name()?;
    Some(T6Attachment {
        kind: unique.attachment_type(),
        mask: unique.combined_mask(),
        alt_weapon: unique.alt_weapon().map(str::to_owned),
        models: [true, false].map(|view| {
            t6_attachment_models(unique, view)
                .into_iter()
                .map(|model| model.copy)
                .collect()
        }),
        view_ads_model: t6_attachment_models(unique, true)
            .into_iter()
            .next()
            .zip(t6_attachment_ads_model(unique))
            .map(|(main, ads)| (main.copy, ads.copy)),
        overlay: unique
            .asset_field_name(u::OVERLAY_MATERIAL)
            .map(str::to_owned),
        hide_tags: unique.hide_tags().map(str::to_owned).collect(),
        xanims: t6_sz_xanims_by(|slot| unique.xanim(slot)),
        fire_sound: unique.sound(u::FIRE_SOUND).map(str::to_owned),
        fire_sound_player: unique.sound(u::FIRE_SOUND_PLAYER).map(str::to_owned),
        disable_base_attachment: unique.flag(u::DISABLE_BASE_ATTACHMENT),
        disable_base_clip: unique.flag(u::DISABLE_BASE_CLIP),
    })
}

pub fn t6_attachment_xanim_names(w: fastfile_t6::weapon::WeaponView<'_>) -> Vec<String> {
    w.attachment_uniques()
        .flat_map(|unique| {
            (0..fastfile_t6::weapon::variant::XANIM_COUNT)
                .filter_map(move |slot| unique.xanim(slot))
                .map(str::to_ascii_lowercase)
        })
        .collect()
}

pub fn t6_attachment_sound_names(w: fastfile_t6::weapon::WeaponView<'_>) -> Vec<String> {
    use fastfile_t6::weapon::unique as u;
    w.attachment_uniques()
        .flat_map(|unique| [u::FIRE_SOUND, u::FIRE_SOUND_PLAYER].map(|off| unique.sound(off)))
        .flatten()
        .map(str::to_owned)
        .collect()
}

pub(super) fn t6_attached_models(
    weapon: fastfile_t6::weapon::WeaponView<'_>,
    view: bool,
) -> Vec<String> {
    (0..fastfile_t6::weapon::variant::ATTACH_MODEL_COUNT)
        .filter_map(|slot| weapon.attached_model(slot, view))
        .map(|(name, _, _)| t6_model_name(name))
        .collect()
}

pub fn t6_model_name(name: &str) -> String {
    name.strip_prefix(',').unwrap_or(name).to_owned()
}

pub fn t6_weapon_sound_names(w: fastfile_t6::weapon::WeaponView<'_>) -> Vec<String> {
    let s = capture_t6_sounds(w);
    [
        s.fire,
        s.fire_player,
        s.fire_last,
        s.fire_last_player,
        s.empty_fire,
        s.empty_fire_player,
        s.melee_swipe,
        s.melee_swipe_player,
        s.melee_hit,
        s.melee_miss,
        s.pullback,
        s.pullback_player,
        s.raise,
        s.raise_player,
        s.first_raise,
        s.first_raise_player,
        s.putaway,
        s.putaway_player,
        s.proj_explosion,
    ]
    .into_iter()
    .flatten()
    .collect()
}

pub(super) fn capture_t6_kick(w: fastfile_t6::weapon::WeaponView<'_>) -> WeaponKickFacts {
    use fastfile_t6::weapon::{def as d, variant as v};
    WeaponKickFacts {
        f_ads_view_kick_center_speed: w.variant_f32(v::ADS_VIEW_KICK_CENTER_SPEED),
        f_hip_view_kick_center_speed: w.variant_f32(v::HIP_VIEW_KICK_CENTER_SPEED),
        gun_max_pitch: w.def_f32(d::GUN_MAX_PITCH),
        gun_max_yaw: w.def_f32(d::GUN_MAX_YAW),
        ads_gun_kick_reduced_kick_bullets: w.def_i32(d::ADS_GUN_KICK_REDUCED_KICK_BULLETS),
        ads_gun_kick_reduced_kick_percent: w.def_f32(d::ADS_GUN_KICK_REDUCED_KICK_PERCENT),
        ads_gun_kick_pitch_min: w.def_f32(d::ADS_GUN_KICK_PITCH_MIN),
        ads_gun_kick_pitch_max: w.def_f32(d::ADS_GUN_KICK_PITCH_MAX),
        ads_gun_kick_yaw_min: w.def_f32(d::ADS_GUN_KICK_YAW_MIN),
        ads_gun_kick_yaw_max: w.def_f32(d::ADS_GUN_KICK_YAW_MAX),
        ads_gun_kick_accel: w.def_f32(d::ADS_GUN_KICK_ACCEL),
        ads_gun_kick_speed_max: w.def_f32(d::ADS_GUN_KICK_SPEED_MAX),
        ads_gun_kick_speed_decay: w.def_f32(d::ADS_GUN_KICK_SPEED_DECAY),
        ads_gun_kick_static_decay: w.def_f32(d::ADS_GUN_KICK_STATIC_DECAY),
        ads_view_kick_pitch_min: w.def_f32(d::ADS_VIEW_KICK_PITCH_MIN),
        ads_view_kick_pitch_max: w.def_f32(d::ADS_VIEW_KICK_PITCH_MAX),
        ads_view_kick_yaw_min: w.def_f32(d::ADS_VIEW_KICK_YAW_MIN),
        ads_view_kick_yaw_max: w.def_f32(d::ADS_VIEW_KICK_YAW_MAX),
        hip_gun_kick_reduced_kick_bullets: w.def_i32(d::HIP_GUN_KICK_REDUCED_KICK_BULLETS),
        hip_gun_kick_reduced_kick_percent: w.def_f32(d::HIP_GUN_KICK_REDUCED_KICK_PERCENT),
        hip_gun_kick_pitch_min: w.def_f32(d::HIP_GUN_KICK_PITCH_MIN),
        hip_gun_kick_pitch_max: w.def_f32(d::HIP_GUN_KICK_PITCH_MAX),
        hip_gun_kick_yaw_min: w.def_f32(d::HIP_GUN_KICK_YAW_MIN),
        hip_gun_kick_yaw_max: w.def_f32(d::HIP_GUN_KICK_YAW_MAX),
        hip_gun_kick_accel: w.def_f32(d::HIP_GUN_KICK_ACCEL),
        hip_gun_kick_speed_max: w.def_f32(d::HIP_GUN_KICK_SPEED_MAX),
        hip_gun_kick_speed_decay: w.def_f32(d::HIP_GUN_KICK_SPEED_DECAY),
        hip_gun_kick_static_decay: w.def_f32(d::HIP_GUN_KICK_STATIC_DECAY),
        hip_view_kick_pitch_min: w.def_f32(d::HIP_VIEW_KICK_PITCH_MIN),
        hip_view_kick_pitch_max: w.def_f32(d::HIP_VIEW_KICK_PITCH_MAX),
        hip_view_kick_yaw_min: w.def_f32(d::HIP_VIEW_KICK_YAW_MIN),
        hip_view_kick_yaw_max: w.def_f32(d::HIP_VIEW_KICK_YAW_MAX),
    }
}
