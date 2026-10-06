use super::*;

impl WeaponCatalog {
    pub fn capture_iw5_attachment(
        &mut self,
        stream: &fastfile_iw5::ZoneStream<'_>,
        geometry: &fastfile_iw5::AttachmentGeometry,
        fx_name_at_slot: &dyn Fn(fastfile_iw5::Ptr) -> Option<String>,
    ) {
        let Some(name) = geometry.name.and_then(|ptr| leftover_cstr_iw5(stream, ptr)) else {
            return;
        };
        let facts = &geometry.facts;
        let projectile_fx = |x86, x64| {
            let body = facts.projectile?.body;
            match stream.ptr_at(body, stream.layout(x86, x64)).ok()? {
                fastfile_iw5::ZonePtr::Offset(slot) => fx_name_at_slot(stream.resolve_alias(slot)),
                _ => None,
            }
        };
        let projectile_sound =
            |x86, x64| leftover_iw5_snd_alias(stream, facts.projectile?.body, x86, x64);
        self.iw5_attachments.insert(
            name,
            Iw5ScopeRow {
                display_name: facts
                    .display_name
                    .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
                weapon_type: facts.weapon_type,
                weapon_class: facts.weapon_class,
                overlay: geometry.overlay_names[0].and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
                view_models: geometry
                    .view_model_names
                    .map(|ptr| ptr.and_then(|p| leftover_cstr_iw5(stream, p))),
                world_models: geometry
                    .world_model_names
                    .map(|ptr| ptr.and_then(|p| leftover_cstr_iw5(stream, p))),
                reticle_models: geometry
                    .reticle_model_names
                    .map(|ptr| ptr.and_then(|p| leftover_cstr_iw5(stream, p))),
                overlay_reticle: geometry.overlay_reticle,
                thermal: geometry.thermal,
                width: geometry.overlay_width,
                height: geometry.overlay_height,
                sight: facts.sight,
                ammo_general: facts.ammo_general,
                reload: facts.reload,
                add_ons: facts.add_ons,
                general: facts.general,
                reticle: facts.general.and_then(|general| general.body).map(|body| {
                    let center = leftover_iw5_material_name(stream, Some(body), 8, 8);
                    let side = leftover_iw5_material_name(stream, Some(body), 12, 16);
                    WeaponReticleAssets {
                        center_authored: center.is_some(),
                        side_authored: side.is_some(),
                        center_material: center,
                        side_material: side,
                        center_size: stream.i32_at(body, stream.layout(16, 24)).unwrap_or(0),
                        side_size: stream.i32_at(body, stream.layout(20, 28)).unwrap_or(0),
                        ..Default::default()
                    }
                }),
                ammunition: facts.ammunition,
                damage: facts.damage,
                projectile: facts.projectile,
                projectile_explosion_fx: projectile_fx(40, 48),
                projectile_trail_fx: projectile_fx(76, 104),
                projectile_ignition_fx: projectile_fx(84, 120),
                projectile_explosion_sound: projectile_sound(48, 64),
                projectile_ignition_sound: projectile_sound(88, 128),
                projectile_model: facts.projectile.and_then(|p| p.model).and_then(|model| {
                    match stream.ptr_at(model, 0).ok()? {
                        fastfile_iw5::ZonePtr::Offset(name) => {
                            leftover_cstr_iw5(stream, stream.resolve_alias(name))
                        }
                        _ => None,
                    }
                }),
                location_damage: facts.location_damage,
                idle_settings: facts.idle_settings,
                ads_settings: facts.ads_settings,
                ads_settings_main: facts.ads_settings_main,
                hip_spread: facts.hip_spread,
                gun_kick: facts.gun_kick,
                view_kick: facts.view_kick,
                scales: facts.scales,
                share_ammo_with_alt: facts.share_ammo_with_alt,
                ..Default::default()
            },
        );
    }

    pub fn capture_iw5(
        &mut self,
        stream: &fastfile_iw5::ZoneStream<'_>,
        strings: &fastfile_iw5::ScriptStrings,
        fx_name_at_slot: &dyn Fn(fastfile_iw5::Ptr) -> Option<String>,
    ) {
        let Some(geometry) = stream.weapon() else {
            return;
        };
        let Some(name_ptr) = geometry.name else {
            return;
        };
        let Ok(name) = stream.cstr(name_ptr) else {
            return;
        };
        if name.is_empty() {
            return;
        }
        let gun_xmodel = geometry
            .gun_xmodel_name
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let hand_xmodel = geometry
            .hand_xmodel_name
            .and_then(|ptr| leftover_cstr_iw5(stream, ptr));
        let world_model = geometry
            .world_model_name
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let overlay_material = leftover_iw5_overlay_name(stream, &geometry);
        let overlay_image = None;
        let overlay_material_slot = leftover_iw5_weapdef_overlay_slot(stream, geometry.weap_def);
        let iw5_attachment_slots = geometry
            .attachments
            .map(|name| name.and_then(|ptr| leftover_cstr_iw5(stream, ptr)));
        let iw5_reload_overrides = geometry.reload_overrides(stream).collect();
        let iw5_fx_overrides = read_iw5_fx_overrides(stream, &geometry, fx_name_at_slot);
        let iw5_notetrack_overrides = read_iw5_notetrack_overrides(stream, strings, &geometry);
        let mut sz_xanims = geometry
            .sz_xanims
            .map(|arr| read_sz_xanims_iw5(stream, arr))
            .unwrap_or([const { None }; WEAPON_ANIM_SLOTS]);
        let leftover_anim_overrides = leftover_iw5_anim_overrides(stream, &geometry);
        apply_leftover_default_anim_overrides(&mut sz_xanims, &leftover_anim_overrides);
        self.entries.push(CatalogWeapon {
            namespace: self
                .capture_ns
                .expect("asset capture requires an explicit family"),
            impact_payload: None,
            alternate_weapon: geometry
                .alternate_weapon_name
                .and_then(|p| leftover_cstr_iw5(stream, p)),
            reticle_center_slot: leftover_iw5_asset_slot(
                stream,
                geometry.weap_def,
                fastfile_iw5::size::WEAPON_DEF_RETICLE_CENTER_OFF,
                560,
            ),
            reticle_side_slot: leftover_iw5_asset_slot(
                stream,
                geometry.weap_def,
                fastfile_iw5::size::WEAPON_DEF_RETICLE_SIDE_OFF,
                568,
            ),
            name: name.to_owned(),
            weap_def: geometry.weap_def.map(iw5_ptr_key),
            display_name_key: geometry
                .display_name
                .and_then(|ptr| stream.cstr(ptr).ok())
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            reticle: leftover_iw5_reticle(stream, geometry.weap_def),
            hud_material_edges: WeaponHudMaterialEdges::default(),
            overlay_material,
            overlay_image,
            overlay_material_slot,
            iw5_attachment_slots,
            attached_models: Default::default(),
            t6_clip_models: Default::default(),
            t6_attachments: Vec::new(),
            t6_attachment_stats: Vec::new(),
            iw5_reload_overrides,
            iw5_anim_overrides: leftover_anim_overrides,
            iw5_fx_overrides,
            iw5_notetrack_overrides,
            hud_icon: leftover_iw5_material_name(
                stream,
                geometry.weap_def,
                fastfile_iw5::size::WEAPON_DEF_HUD_ICON_OFF,
                792,
            ),
            hud_icon_slot: leftover_iw5_asset_slot(
                stream,
                geometry.weap_def,
                fastfile_iw5::size::WEAPON_DEF_HUD_ICON_OFF,
                792,
            ),

            pickup_icon: None,
            pickup_icon_slot: leftover_iw5_asset_slot(stream, geometry.weap_def, 508, 808),
            pickup_icon_image: None,
            pickup_icon_ratio: geometry
                .weap_def
                .map_or(0, |body| i32_at_iw5(stream, body, 512, 816)),
            hud_icon_ratio: geometry
                .weap_def
                .map_or(0, |body| i32_at_iw5(stream, body, 504, 800)),
            hud_icon_image: None,
            dpad_icon: None,
            dpad_icon_image: None,
            dpad_icon_atlas: None,
            dpad_icon_ratio: 0,
            kill_icon: geometry
                .kill_icon
                .and_then(|mat| leftover_xstring_at_iw5(stream, mat, 0, 0)),
            kill_icon_slot: geometry.kill_icon_slot.map(|slot| Ptr {
                block: slot.block,
                offset: slot.offset,
            }),
            kill_icon_image: None,
            proj_trail: None,
            proj_trail_slot: None,
            proj_beacon: None,
            proj_beacon_slot: None,
            proj_ignition: None,
            proj_ignition_slot: None,
            projectile_fx: WeaponProjectileFx::default(),
            gun_xmodel,
            hand_xmodel,
            world_model,
            camo_models: WeaponCamoModels::default(),
            skin_parent: None,
            projectile_model: geometry
                .projectile_model_name
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            rocket_model: geometry
                .rocket_model_name
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            knife_xmodel: geometry
                .knife_xmodel_name
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            sz_xanims,
            dual_wield_weapon: None,
            sz_xanims_right: [const { None }; WEAPON_ANIM_SLOTS],
            sz_xanims_left: [const { None }; WEAPON_ANIM_SLOTS],
            hide_tags: read_hide_tags_iw5(stream, strings, geometry.hide_tags),
            sounds: leftover_iw5_sounds(stream, strings, &geometry),
            combat_fx: read_iw5_combat_fx(stream, &geometry, fx_name_at_slot),
            combat_slots: CombatFxSlots::default(),
            facts: capture_iw5_body_facts(stream, &geometry),
        });
    }
}

pub(super) fn iw5_best_pair_override<'a, T>(
    rows: &'a [T],
    selection: Iw5AttachmentSelection,
    override_type: u32,
    fields: impl Fn(&T) -> (u16, u16, u32),
) -> Option<&'a T> {
    let candidates = selection.override_candidates();
    let mut best = None;
    let mut best_score = 0;
    for row in rows {
        let (first, second, row_type) = fields(row);
        if row_type != override_type {
            continue;
        }
        let score = u8::from(first != 0 && candidates.contains(&first))
            + u8::from(second != 0 && candidates.contains(&second));
        if score > best_score {
            best = Some(row);
            best_score = score;
        }
    }
    best
}

pub(super) fn remap_iw5_weap_type(raw: i32) -> i32 {
    match raw {
        1 => 0,
        2 => 1,
        3 => 2,
        4 => 3,
        other => other,
    }
}

pub(super) fn i32_at_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    body: fastfile_iw5::Ptr,
    x86: usize,
    x64: usize,
) -> i32 {
    stream.i32_at(body, stream.layout(x86, x64)).unwrap_or(0)
}

pub(super) fn u8_at_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    body: fastfile_iw5::Ptr,
    x86: usize,
    x64: usize,
) -> u8 {
    stream.u8_at(body, stream.layout(x86, x64)).unwrap_or(0)
}

pub(super) fn f32_at_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    body: fastfile_iw5::Ptr,
    x86: usize,
    x64: usize,
) -> f32 {
    stream.f32_at(body, stream.layout(x86, x64)).unwrap_or(0.0)
}

pub(super) fn capture_iw5_body_facts(
    stream: &fastfile_iw5::ZoneStream<'_>,
    geometry: &fastfile_iw5::WeaponGeometry,
) -> WeaponBodyFacts {
    use fastfile_iw5::size as sz;
    let mut facts = WeaponBodyFacts {
        body_resolved: geometry.weap_def.is_some(),
        fire_time_ms: geometry.fire_time_ms,
        clip_size: geometry.clip_size,
        weap_type: remap_iw5_weap_type(geometry.weap_type),
        weap_class: geometry.weap_class,
        fire_type: geometry.fire_type,
        move_speed_scale: geometry.move_speed_scale,
        ads_move_speed_scale: geometry.ads_move_speed_scale,
        ads_overlay_width: geometry.ads_overlay_width,
        ads_overlay_height: geometry.ads_overlay_height,
        overlay_reticle: geometry.overlay_reticle,
        overlay_interface: geometry.overlay_interface,
        ads_zoom_fov: geometry.ads_zoom_fov,
        ads_zoom_in_frac: geometry.ads_zoom_in_frac,
        ads_zoom_out_frac: geometry.ads_zoom_out_frac,
        ads_in_rate: geometry.ads_in_rate,
        ads_out_rate: geometry.ads_out_rate,
        impact_type: geometry.impact_type,
        penetrate_multiplier: geometry.penetrate_multiplier,
        motion_tracker: geometry.motion_tracker,
        kick: WeaponKickFacts {
            f_ads_view_kick_center_speed: geometry.ads_view_kick_center_speed,
            f_hip_view_kick_center_speed: geometry.hip_view_kick_center_speed,
            ..Default::default()
        },
        ..WeaponBodyFacts::default()
    };
    let Some(body) = geometry.weap_def else {
        return facts;
    };
    facts.kill_icon_ratio = i32_at_iw5(stream, body, sz::WEAPON_DEF_KILL_ICON_RATIO_OFF, 1588);
    facts.silenced = u8_at_iw5(stream, body, sz::WEAPON_DEF_SILENCED_OFF, 2459) != 0;
    facts.flip_kill_icon = u8_at_iw5(stream, body, sz::WEAPON_DEF_FLIP_KILL_ICON_OFF, 2455) != 0;
    facts.ads_aim_pitch = f32_at_iw5(stream, body, 1400, 1816);
    facts.ads_crosshair_in_frac = f32_at_iw5(stream, body, 1404, 1820);
    facts.ads_crosshair_out_frac = f32_at_iw5(stream, body, 1408, 1824);
    facts.kick.gun_max_pitch = f32_at_iw5(stream, body, sz::WEAPON_DEF_GUN_MAX_PITCH_OFF, 1496);
    facts.kick.gun_max_yaw = f32_at_iw5(stream, body, sz::WEAPON_DEF_GUN_MAX_YAW_OFF, 1500);
    facts.kick.ads_gun_kick_reduced_kick_bullets = i32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_REDUCED_KICK_BULLETS_OFF,
        1828,
    );
    facts.kick.ads_gun_kick_reduced_kick_percent = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_REDUCED_KICK_PERCENT_OFF,
        1832,
    );
    facts.kick.ads_gun_kick_pitch_min = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_PITCH_MIN_OFF,
        1836,
    );
    facts.kick.ads_gun_kick_pitch_max = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_PITCH_MAX_OFF,
        1840,
    );
    facts.kick.ads_gun_kick_yaw_min =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_YAW_MIN_OFF, 1844);
    facts.kick.ads_gun_kick_yaw_max =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_YAW_MAX_OFF, 1848);
    facts.kick.ads_gun_kick_accel =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_ACCEL_OFF, 1852);
    facts.kick.ads_gun_kick_speed_max = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_SPEED_MAX_OFF,
        1856,
    );
    facts.kick.ads_gun_kick_speed_decay = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_SPEED_DECAY_OFF,
        1860,
    );
    facts.kick.ads_gun_kick_static_decay = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_STATIC_DECAY_OFF,
        1864,
    );
    facts.kick.ads_view_kick_pitch_min = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_VIEW_KICK_PITCH_MIN_OFF,
        1868,
    );
    facts.kick.ads_view_kick_pitch_max = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_VIEW_KICK_PITCH_MAX_OFF,
        1872,
    );
    facts.kick.ads_view_kick_yaw_min =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_VIEW_KICK_YAW_MIN_OFF, 1876);
    facts.kick.ads_view_kick_yaw_max =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_VIEW_KICK_YAW_MAX_OFF, 1880);
    facts.kick.hip_gun_kick_reduced_kick_bullets = i32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_REDUCED_KICK_BULLETS_OFF,
        1896,
    );
    facts.kick.hip_gun_kick_reduced_kick_percent = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_REDUCED_KICK_PERCENT_OFF,
        1900,
    );
    facts.kick.hip_gun_kick_pitch_min = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_PITCH_MIN_OFF,
        1904,
    );
    facts.kick.hip_gun_kick_pitch_max = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_PITCH_MAX_OFF,
        1908,
    );
    facts.kick.hip_gun_kick_yaw_min =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_YAW_MIN_OFF, 1912);
    facts.kick.hip_gun_kick_yaw_max =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_YAW_MAX_OFF, 1916);
    facts.kick.hip_gun_kick_accel =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_ACCEL_OFF, 1920);
    facts.kick.hip_gun_kick_speed_max = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_SPEED_MAX_OFF,
        1924,
    );
    facts.kick.hip_gun_kick_speed_decay = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_SPEED_DECAY_OFF,
        1928,
    );
    facts.kick.hip_gun_kick_static_decay = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_STATIC_DECAY_OFF,
        1932,
    );
    facts.kick.hip_view_kick_pitch_min = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_VIEW_KICK_PITCH_MIN_OFF,
        1936,
    );
    facts.kick.hip_view_kick_pitch_max = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_VIEW_KICK_PITCH_MAX_OFF,
        1940,
    );
    facts.kick.hip_view_kick_yaw_min =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_VIEW_KICK_YAW_MIN_OFF, 1944);
    facts.kick.hip_view_kick_yaw_max =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_VIEW_KICK_YAW_MAX_OFF, 1948);
    facts.ammo_counter_clip = i32_at_iw5(stream, body, sz::WEAPON_DEF_AMMO_COUNTER_CLIP_OFF, 836);
    facts.start_ammo = i32_at_iw5(stream, body, sz::WEAPON_DEF_START_AMMO_OFF, 840);
    facts.i_reticle_side_size = i32_at_iw5(stream, body, sz::WEAPON_DEF_RETICLE_SIDE_SIZE_OFF, 580);
    facts.i_reticle_min_ofs = i32_at_iw5(stream, body, sz::WEAPON_DEF_RETICLE_MIN_OFS_OFF, 584);
    facts.ammo_index = i32_at_iw5(stream, body, sz::WEAPON_DEF_AMMO_INDEX_OFF, 856);
    facts.clip_index = i32_at_iw5(stream, body, sz::WEAPON_DEF_CLIP_INDEX_OFF, 872);
    facts.max_ammo = i32_at_iw5(stream, body, sz::WEAPON_DEF_MAX_AMMO_OFF, 876);
    facts.shots_per_fire = i32_at_iw5(stream, body, sz::WEAPON_DEF_SHOTS_PER_FIRE_OFF, 880);
    apply_leftover_hip_spread(
        &mut facts,
        leftover_hip_spread_block(
            |off| stream.f32_at(body, off).unwrap_or(0.0),
            stream.layout(sz::WEAPON_DEF_HIP_SPREAD_STAND_MIN_OFF, 1420),
        ),
    );
    facts.melee_damage = i32_at_iw5(stream, body, sz::WEAPON_DEF_MELEE_DAMAGE_OFF, 912);
    facts.melee_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_MELEE_TIME_OFF, 956);
    facts.melee_delay_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_MELEE_DELAY_OFF, 924);
    facts.melee_charge_time_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_MELEE_CHARGE_TIME_OFF, 960);
    facts.melee_charge_delay_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_MELEE_CHARGE_DELAY_OFF, 928);
    facts.knife_model = u32::from(matches!(
        stream.ptr_at(body, stream.layout(sz::WEAPON_DEF_KNIFE_MODEL_OFF, 776)),
        Ok(fastfile_iw5::ZonePtr::Offset(_))
            | Ok(fastfile_iw5::ZonePtr::Following)
            | Ok(fastfile_iw5::ZonePtr::Insert)
    ));
    facts.fire_delay_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_FIRE_DELAY_OFF, 920);
    facts.rechamber_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_RECHAMBER_TIME_OFF, 936);
    facts.rechamber_bolt_time_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_RECHAMBER_BOLT_TIME_OFF, 940);
    facts.rechamber_bolt_delay_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_RECHAMBER_BOLT_DELAY_OFF, 944);
    facts.reload_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_TIME_OFF, 964);
    facts.reload_empty_time_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_EMPTY_TIME_OFF, 972);
    facts.reload_add_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_ADD_TIME_OFF, 976);
    facts.reload_start_time_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_START_TIME_OFF, 980);
    facts.reload_start_add_time_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_START_ADD_TIME_OFF, 984);
    facts.reload_end_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_END_TIME_OFF, 988);
    facts.alternate_raise_time_ms = geometry.alternate_raise_time_ms;
    facts.alternate_drop_time_ms = geometry.alternate_drop_time_ms;
    facts.drop_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_DROP_TIME_OFF, 992);
    facts.raise_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_RAISE_TIME_OFF, 996);
    facts.quick_drop_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_QUICK_DROP_TIME_OFF, 1004);
    facts.quick_raise_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_QUICK_RAISE_TIME_OFF, 1008);
    facts.sprint_raise_time_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_SPRINT_RAISE_TIME_OFF, 1024);
    facts.sprint_loop_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_SPRINT_LOOP_TIME_OFF, 1028);
    facts.sprint_drop_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_SPRINT_DROP_TIME_OFF, 1032);
    facts.damage = i32_at_iw5(stream, body, sz::WEAPON_DEF_DAMAGE_OFF, 904);
    facts.min_damage = i32_at_iw5(stream, body, sz::WEAPON_DEF_MIN_DAMAGE_OFF, 2144);
    facts.min_player_damage = i32_at_iw5(stream, body, sz::WEAPON_DEF_MIN_PLAYER_DAMAGE_OFF, 2148);
    facts.max_damage_range = f32_at_iw5(stream, body, sz::WEAPON_DEF_MAX_DAMAGE_RANGE_OFF, 2152);
    facts.min_damage_range = f32_at_iw5(stream, body, sz::WEAPON_DEF_MIN_DAMAGE_RANGE_OFF, 2156);
    facts.inherits_perks = u8_at_iw5(stream, body, sz::WEAPON_DEF_INHERITS_PERKS_OFF, 2435) != 0;
    facts.rifle_bullet = u8_at_iw5(stream, body, sz::WEAPON_DEF_RIFLE_BULLET_OFF, 2437) != 0;
    facts.ricochet_chance = f32_at_iw5(stream, body, sz::WEAPON_DEF_RICOCHET_CHANCE_OFF, 1728);
    facts.explosive_bullet =
        u8_at_iw5(stream, body, sz::WEAPON_DEF_EXPLOSIVE_BULLET_OFF, 2444) != 0;
    facts.bolt_action = u8_at_iw5(stream, body, sz::WEAPON_DEF_BOLT_ACTION_OFF, 2439) != 0;
    facts.aim_down_sight = u8_at_iw5(stream, body, sz::WEAPON_DEF_AIM_DOWN_SIGHT_OFF, 2440) != 0;
    facts.can_hold_breath = u8_at_iw5(stream, body, sz::WEAPON_DEF_CAN_HOLD_BREATH_OFF, 2441) != 0;
    facts.rechamber_while_ads =
        u8_at_iw5(stream, body, sz::WEAPON_DEF_RECHAMBER_WHILE_ADS_OFF, 2443) != 0;
    facts.ads_fire_only = u8_at_iw5(stream, body, sz::WEAPON_DEF_ADS_FIRE_ONLY_OFF, 2448) != 0;
    facts.no_partial_reload =
        u8_at_iw5(stream, body, sz::WEAPON_DEF_NO_PARTIAL_RELOAD_OFF, 2456) != 0;
    facts.segmented_reload =
        u8_at_iw5(stream, body, sz::WEAPON_DEF_SEGMENTED_RELOAD_OFF, 2457) != 0;
    facts.select_requires_ammo = Some(
        u8_at_iw5(
            stream,
            body,
            sz::WEAPON_DEF_DISABLE_SWITCH_TO_WHEN_EMPTY_OFF,
            2450,
        ) != 0,
    );
    facts.offhand_hold_is_cancelable = Some(
        u8_at_iw5(
            stream,
            body,
            sz::WEAPON_DEF_OFFHAND_HOLD_IS_CANCELABLE_OFF,
            2478,
        ) != 0,
    );
    facts.idle = WeaponIdleInputs {
        ads_idle_amount: f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_IDLE_AMOUNT_OFF, 1472),
        hip_idle_amount: f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_IDLE_AMOUNT_OFF, 1476),
        ads_idle_speed: f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_IDLE_SPEED_OFF, 1480),
        hip_idle_speed: f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_IDLE_SPEED_OFF, 1484),
        idle_crouch_factor: f32_at_iw5(stream, body, sz::WEAPON_DEF_IDLE_CROUCH_FACTOR_OFF, 1488),
        idle_prone_factor: f32_at_iw5(stream, body, sz::WEAPON_DEF_IDLE_PRONE_FACTOR_OFF, 1492),
    };
    facts.offhand_class = i32_at_iw5(stream, body, sz::WEAPON_DEF_OFFHAND_CLASS_OFF, 104);
    facts.hold_fire_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_HOLD_FIRE_TIME_OFF, 948);
    facts.fuse_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_FUSE_TIME_OFF, 1072);
    facts.stickiness = i32_at_iw5(stream, body, sz::WEAPON_DEF_STICKINESS_OFF, 1720);
    facts.has_detonator = u8_at_iw5(stream, body, sz::WEAPON_DEF_HAS_DETONATOR_OFF, 2466) != 0;
    facts.timed_detonation =
        u8_at_iw5(stream, body, sz::WEAPON_DEF_TIMED_DETONATION_OFF, 2468) != 0;
    facts.projectile_rotates = u8_at_iw5(stream, body, sz::WEAPON_DEF_ROTATE_OFF, 2469) != 0;
    facts.cook_off_hold = u8_at_iw5(stream, body, sz::WEAPON_DEF_COOK_OFF_HOLD_OFF, 2445) != 0;
    facts.explosion_radius = i32_at_iw5(stream, body, sz::WEAPON_DEF_EXPLOSION_RADIUS_OFF, 1612);
    facts.explosion_radius_min =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_EXPLOSION_RADIUS_MIN_OFF, 1616);
    facts.explosion_inner_damage = i32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_EXPLOSION_INNER_DAMAGE_OFF,
        1620,
    );
    facts.explosion_outer_damage = i32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_EXPLOSION_OUTER_DAMAGE_OFF,
        1624,
    );
    facts.projectile_speed = i32_at_iw5(stream, body, sz::WEAPON_DEF_PROJECTILE_SPEED_OFF, 1640);
    facts.projectile_speed_up =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_PROJECTILE_SPEED_UP_OFF, 1644);
    facts.projectile_activate_dist = i32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_PROJECTILE_ACTIVATE_DIST_OFF,
        1652,
    );
    facts.missile_guidance = i32_at_iw5(stream, body, 1380, 1788);
    facts.ignition_delay_ms = i32_at_iw5(stream, body, 1388, 1796);
    facts.require_lock_to_fire = u8_at_iw5(stream, body, 1898, 2430) != 0;
    facts.projectile_explosion_type =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_PROJ_EXPLOSION_TYPE_OFF, 1680);
    facts.proj_impact_explode =
        u8_at_iw5(stream, body, sz::WEAPON_DEF_PROJ_IMPACT_EXPLODE_OFF, 2462) != 0;
    facts.stick_to_players =
        u8_at_iw5(stream, body, sz::WEAPON_DEF_STICK_TO_PLAYERS_OFF, 2463) != 0;
    facts
}

pub(super) fn iw5_ptr_key(p: fastfile_iw5::Ptr) -> (u8, u32) {
    (p.block, p.offset)
}

pub(super) fn read_sz_xanims_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    arr: fastfile_iw5::Ptr,
) -> [Option<String>; WEAPON_ANIM_SLOTS] {
    let mut iw5 = [const { None }; fastfile_iw5::size::WEAPON_ANIM_COUNT];

    let step = stream.pointer_bytes();
    for (i, slot) in iw5.iter_mut().enumerate() {
        let name_ptr = match stream.ptr_at(arr, i * step) {
            Ok(fastfile_iw5::ZonePtr::Offset(q)) => Some(stream.resolve_alias(q)),
            _ => None,
        };
        *slot = name_ptr
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
    }
    remap_iw5_sz_xanims(&iw5)
}

pub(super) fn remap_iw5_sz_xanims(iw5: &[Option<String>]) -> [Option<String>; WEAPON_ANIM_SLOTS] {
    let mut out = [const { None }; WEAPON_ANIM_SLOTS];
    for (i, name) in iw5.iter().enumerate() {
        let Some(slot) = iw5_anim_tree_type_to_iw4_slot(i as u32) else {
            continue;
        };
        out[slot] = name.clone();
    }
    out
}

pub(super) fn iw5_anim_tree_type_to_iw4_slot(ty: u32) -> Option<usize> {
    let ty = ty as usize;
    if ty <= weap_anim::ADS_RECHAMBER {
        Some(ty)
    } else if ty == fastfile_iw5::size::WEAPON_ANIM_ADS_UP {
        Some(weap_anim::ADS_UP)
    } else if ty == fastfile_iw5::size::WEAPON_ANIM_ADS_DOWN {
        Some(weap_anim::ADS_DOWN)
    } else {
        None
    }
}

pub(super) fn leftover_iw5_overlay_name(
    stream: &fastfile_iw5::ZoneStream<'_>,
    geometry: &fastfile_iw5::WeaponGeometry,
) -> Option<String> {
    if let Some(name) = geometry
        .overlay_material_names
        .iter()
        .flatten()
        .filter_map(|&p| leftover_cstr_iw5(stream, p))
        .find(|name| overlay_name_is_hud_iris(name))
        .or_else(|| {
            leftover_iw5_material_name(
                stream,
                geometry.weap_def,
                fastfile_iw5::size::WEAPON_DEF_OVERLAY_SHADER_OFF,
                1352,
            )
        })
    {
        if overlay_name_is_hud_iris(&name) {
            return Some(name);
        }
    }
    None
}

pub(super) fn leftover_cstr_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    ptr: fastfile_iw5::Ptr,
) -> Option<String> {
    stream
        .cstr(ptr)
        .ok()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

pub(super) fn leftover_iw5_weapdef_overlay_slot(
    stream: &fastfile_iw5::ZoneStream<'_>,
    weap_def: Option<fastfile_iw5::Ptr>,
) -> Option<Ptr> {
    leftover_iw5_asset_slot(
        stream,
        weap_def,
        fastfile_iw5::size::WEAPON_DEF_OVERLAY_SHADER_OFF,
        1352,
    )
}

pub(super) fn leftover_iw5_material_name(
    stream: &fastfile_iw5::ZoneStream<'_>,
    weap_def: Option<fastfile_iw5::Ptr>,
    x86: usize,
    x64: usize,
) -> Option<String> {
    let body = weap_def?;
    let field = stream.layout(x86, x64);
    let mat = match stream.ptr_at(body, field).ok()? {
        fastfile_iw5::ZonePtr::Offset(q) => stream.resolve_alias(q),
        _ => return None,
    };
    leftover_xstring_at_iw5(stream, mat, 0, 0)
}

pub(super) fn leftover_iw5_asset_slot(
    stream: &fastfile_iw5::ZoneStream<'_>,
    weap_def: Option<fastfile_iw5::Ptr>,
    x86: usize,
    x64: usize,
) -> Option<Ptr> {
    let body = weap_def?;
    let field = stream.layout(x86, x64);
    match stream.ptr_at(body, field).ok()? {
        fastfile_iw5::ZonePtr::Null => None,
        _ => {
            let cell = body.at(field);
            Some(Ptr {
                block: cell.block,
                offset: cell.offset,
            })
        }
    }
}

pub(super) fn leftover_iw5_reticle(
    stream: &fastfile_iw5::ZoneStream<'_>,
    weap_def: Option<fastfile_iw5::Ptr>,
) -> WeaponReticleAssets {
    use fastfile_iw5::size as sz;
    WeaponReticleAssets {
        center_material: leftover_iw5_material_name(
            stream,
            weap_def,
            sz::WEAPON_DEF_RETICLE_CENTER_OFF,
            560,
        ),
        side_material: leftover_iw5_material_name(
            stream,
            weap_def,
            sz::WEAPON_DEF_RETICLE_SIDE_OFF,
            568,
        ),
        center_authored: leftover_iw5_asset_slot(
            stream,
            weap_def,
            sz::WEAPON_DEF_RETICLE_CENTER_OFF,
            560,
        )
        .is_some(),
        side_authored: leftover_iw5_asset_slot(
            stream,
            weap_def,
            sz::WEAPON_DEF_RETICLE_SIDE_OFF,
            568,
        )
        .is_some(),
        center_size: weap_def
            .map(|body| i32_at_iw5(stream, body, sz::WEAPON_DEF_RETICLE_CENTER_SIZE_OFF, 576))
            .unwrap_or(0),
        side_size: weap_def
            .map(|body| i32_at_iw5(stream, body, sz::WEAPON_DEF_RETICLE_SIDE_SIZE_OFF, 580))
            .unwrap_or(0),
        ..WeaponReticleAssets::default()
    }
}

pub(super) fn leftover_iw5_sounds(
    stream: &fastfile_iw5::ZoneStream<'_>,
    strings: &fastfile_iw5::ScriptStrings,
    geometry: &fastfile_iw5::WeaponGeometry,
) -> WeaponSoundAliases {
    use fastfile_iw5::size as sz;
    let Some(body) = geometry.weap_def else {
        return WeaponSoundAliases::default();
    };
    let mut sounds = WeaponSoundAliases {
        notetrack_convention: NotetrackConvention::SoundMap,
        pickup: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_PICKUP_OFF, 128),
        pickup_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_PICKUP_PLAYER_OFF,
            136,
        ),
        ammo_pickup: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_AMMO_PICKUP_OFF, 144),
        ammo_pickup_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_AMMO_PICKUP_PLAYER_OFF,
            152,
        ),
        pullback: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_PULLBACK_OFF, 168),
        pullback_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_PULLBACK_PLAYER_OFF,
            176,
        ),
        fire: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_FIRE_OFF, 184),
        fire_player: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_FIRE_PLAYER_OFF, 192),
        empty_fire: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_EMPTY_FIRE_OFF, 256),
        empty_fire_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_EMPTY_FIRE_PLAYER_OFF,
            264,
        ),
        melee_swipe: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_MELEE_SWIPE_OFF, 272),
        melee_swipe_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_MELEE_SWIPE_PLAYER_OFF,
            280,
        ),
        melee_hit: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_MELEE_HIT_OFF, 288),
        melee_miss: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_MELEE_MISS_OFF, 296),
        rechamber: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_RECHAMBER_OFF, 304),
        rechamber_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RECHAMBER_PLAYER_OFF,
            312,
        ),
        reload: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_RELOAD_OFF, 320),
        reload_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_PLAYER_OFF,
            328,
        ),
        reload_empty: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_EMPTY_OFF,
            336,
        ),
        reload_empty_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_EMPTY_PLAYER_OFF,
            344,
        ),
        reload_start: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_START_OFF,
            352,
        ),
        reload_start_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_START_PLAYER_OFF,
            360,
        ),
        reload_end: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_RELOAD_END_OFF, 368),
        reload_end_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_END_PLAYER_OFF,
            376,
        ),
        alt_switch: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_ALT_SWITCH_OFF, 432),
        alt_switch_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_ALT_SWITCH_PLAYER_OFF,
            440,
        ),
        raise: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_RAISE_OFF, 448),
        raise_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_RAISE_PLAYER_OFF,
            456,
        ),
        first_raise: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_FIRST_RAISE_OFF, 464),
        first_raise_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_FIRST_RAISE_PLAYER_OFF,
            472,
        ),
        putaway: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_PUTAWAY_OFF, 480),
        putaway_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_PUTAWAY_PLAYER_OFF,
            488,
        ),
        proj_explosion: None,
        projectile: None,
        proj_ignition_sound: None,
        bounce: Default::default(),
        notetrack_sound_map: leftover_iw5_script_string_map(
            stream,
            strings,
            body,
            stream.layout(sz::WEAPON_DEF_NOTE_SOUND_KEYS_OFF, 48),
            stream.layout(sz::WEAPON_DEF_NOTE_SOUND_VALUES_OFF, 56),
            sz::WEAPON_DEF_NOTE_SOUND_MAP_COUNT,
        ),
        notetrack_rumble_map: leftover_iw5_script_string_map(
            stream,
            strings,
            body,
            stream.layout(sz::WEAPON_DEF_NOTE_RUMBLE_KEYS_OFF, 64),
            stream.layout(sz::WEAPON_DEF_NOTE_RUMBLE_VALUES_OFF, 72),
            sz::WEAPON_DEF_NOTE_RUMBLE_MAP_COUNT,
        ),
        fire_player_akimbo: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_FIRE_PLAYER_AKIMBO_OFF,
            200,
        ),
        fire_loop: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_FIRE_LOOP_OFF, 208),
        fire_loop_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_FIRE_LOOP_PLAYER_OFF,
            216,
        ),
        fire_stop: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_FIRE_STOP_OFF, 224),
        fire_stop_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_FIRE_STOP_PLAYER_OFF,
            232,
        ),
        fire_last: leftover_iw5_snd_alias(stream, body, sz::WEAPON_DEF_SND_FIRE_LAST_OFF, 240),
        fire_last_player: leftover_iw5_snd_alias(
            stream,
            body,
            sz::WEAPON_DEF_SND_FIRE_LAST_PLAYER_OFF,
            248,
        ),
        leftover_sound_overrides: leftover_iw5_sound_overrides(stream, geometry),
        fire_ptr_kind: leftover_iw5_snd_ptr_kind(stream, body, sz::WEAPON_DEF_SND_FIRE_OFF, 184),
        fire_player_ptr_kind: leftover_iw5_snd_ptr_kind(
            stream,
            body,
            sz::WEAPON_DEF_SND_FIRE_PLAYER_OFF,
            192,
        ),
        reload_player_ptr_kind: leftover_iw5_snd_ptr_kind(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_PLAYER_OFF,
            328,
        ),
    };
    apply_leftover_default_sound_overrides(&mut sounds);
    sounds
}

pub(super) fn leftover_iw5_anim_overrides(
    stream: &fastfile_iw5::ZoneStream<'_>,
    geometry: &fastfile_iw5::WeaponGeometry,
) -> Vec<LeftoverAnimOverride> {
    geometry
        .anim_overrides(stream)
        .map(|row| LeftoverAnimOverride {
            attachment1: row.attachment1,
            attachment2: row.attachment2,
            anim_tree_type: row.anim_tree_type,
            override_anim: row
                .override_anim
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            altmode_anim: row
                .altmode_anim
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            anim_time_ms: row.anim_time_ms,
            alt_time_ms: row.alt_time_ms,
        })
        .collect()
}

pub(super) fn leftover_xstring_at_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    row: fastfile_iw5::Ptr,
    x86: usize,
    x64: usize,
) -> Option<String> {
    match stream.ptr_at(row, stream.layout(x86, x64)) {
        Ok(fastfile_iw5::ZonePtr::Offset(q)) => leftover_cstr_iw5(stream, stream.resolve_alias(q)),
        _ => None,
    }
}

pub(super) fn leftover_iw5_sound_overrides(
    stream: &fastfile_iw5::ZoneStream<'_>,
    geometry: &fastfile_iw5::WeaponGeometry,
) -> Vec<LeftoverSoundOverride> {
    geometry
        .sound_overrides(stream)
        .map(|row| LeftoverSoundOverride {
            attachment1: row.attachment1,
            attachment2: row.attachment2,
            sound_type: row.sound_type,
            override_sound: row
                .override_sound
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            altmode_sound: row
                .altmode_sound
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
        })
        .collect()
}

pub(super) fn read_iw5_combat_fx(
    stream: &fastfile_iw5::ZoneStream<'_>,
    geometry: &fastfile_iw5::WeaponGeometry,
    fx_name_at_slot: &dyn Fn(fastfile_iw5::Ptr) -> Option<String>,
) -> WeaponCombatFx {
    use fastfile_iw5::size as sz;
    let Some(body) = geometry.weap_def else {
        return WeaponCombatFx::empty(crate::AssetNamespace::Iw5);
    };
    let name = |x86, x64| match stream.ptr_at(body, stream.layout(x86, x64)) {
        Ok(fastfile_iw5::ZonePtr::Offset(q)) => fx_name_at_slot(stream.resolve_alias(q)),
        _ => None,
    };
    let view_last_shot_eject_hint = name(sz::WEAPON_DEF_VIEW_LAST_SHOT_EJECT_OFF, 544);
    let world_last_shot_eject_hint = name(sz::WEAPON_DEF_WORLD_LAST_SHOT_EJECT_OFF, 552);
    WeaponCombatFx {
        view_flash_hint: name(sz::WEAPON_DEF_VIEW_FLASH_OFF, 112),
        world_flash_hint: name(sz::WEAPON_DEF_WORLD_FLASH_OFF, 120),
        view_shell_eject_hint: name(sz::WEAPON_DEF_VIEW_SHELL_EJECT_OFF, 528),
        world_shell_eject_hint: name(sz::WEAPON_DEF_WORLD_SHELL_EJECT_OFF, 536),
        last_shot_eject_pair_authored: view_last_shot_eject_hint.is_some()
            && world_last_shot_eject_hint.is_some(),
        view_last_shot_eject_hint,
        world_last_shot_eject_hint,
        ..WeaponCombatFx::empty(crate::AssetNamespace::Iw5)
    }
}

pub(super) fn read_iw5_fx_overrides(
    stream: &fastfile_iw5::ZoneStream<'_>,
    geometry: &fastfile_iw5::WeaponGeometry,
    fx_name_at_slot: &dyn Fn(fastfile_iw5::Ptr) -> Option<String>,
) -> Vec<Iw5FxOverride> {
    geometry
        .overrides(stream)
        .map(|row| Iw5FxOverride {
            attachment1: row.attachment1,
            attachment2: row.attachment2,
            fx_type: row.fx_type,
            override_fx: row.override_fx.and_then(fx_name_at_slot),
            altmode_fx: row.altmode_fx.and_then(fx_name_at_slot),
        })
        .collect()
}

pub(super) fn read_iw5_notetrack_overrides(
    stream: &fastfile_iw5::ZoneStream<'_>,
    strings: &fastfile_iw5::ScriptStrings,
    geometry: &fastfile_iw5::WeaponGeometry,
) -> Vec<Iw5NotetrackOverride> {
    geometry
        .note_track_overrides(stream)
        .map(|row| Iw5NotetrackOverride {
            attachment: row.attachment,
            sound_map: row
                .sound_map
                .map(|map| iw5_script_string_pairs(stream, strings, map, 24))
                .unwrap_or_default(),
        })
        .collect()
}

pub(super) fn leftover_iw5_snd_alias(
    stream: &fastfile_iw5::ZoneStream<'_>,
    body: fastfile_iw5::Ptr,
    x86: usize,
    x64: usize,
) -> Option<String> {
    leftover_iw5_snd_alias_at(stream, body, x86, x64)
}

pub(super) fn leftover_iw5_snd_ptr_kind(
    stream: &fastfile_iw5::ZoneStream<'_>,
    body: fastfile_iw5::Ptr,
    x86: usize,
    x64: usize,
) -> Option<&'static str> {
    match stream.ptr_at(body, stream.layout(x86, x64)) {
        Ok(fastfile_iw5::ZonePtr::Null) => Some("null"),
        Ok(fastfile_iw5::ZonePtr::Following) => Some("follow"),
        Ok(fastfile_iw5::ZonePtr::Insert) => Some("insert"),
        Ok(fastfile_iw5::ZonePtr::Offset(_)) => Some("offset"),
        Err(_) => Some("err"),
    }
}

pub(super) fn leftover_iw5_snd_alias_at(
    stream: &fastfile_iw5::ZoneStream<'_>,
    body: fastfile_iw5::Ptr,
    x86: usize,
    x64: usize,
) -> Option<String> {
    let wrapper = match stream.ptr_at(body, stream.layout(x86, x64)).ok()? {
        fastfile_iw5::ZonePtr::Offset(q) => stream.resolve_alias(q),
        _ => return None,
    };
    let name_ptr = match stream.ptr_at(wrapper, 0).ok()? {
        fastfile_iw5::ZonePtr::Offset(n) => stream.resolve_alias(n),
        _ => wrapper,
    };
    leftover_cstr_iw5(stream, name_ptr)
}

pub(super) fn leftover_iw5_script_string_map(
    stream: &fastfile_iw5::ZoneStream<'_>,
    strings: &fastfile_iw5::ScriptStrings,
    body: fastfile_iw5::Ptr,
    keys_off: usize,
    values_off: usize,
    cap: usize,
) -> Vec<(String, String)> {
    fastfile_iw5::ScriptStringMap::at(stream, body, keys_off, values_off)
        .map(|map| iw5_script_string_pairs(stream, strings, map, cap))
        .unwrap_or_default()
}

pub(super) fn iw5_script_string_pairs(
    stream: &fastfile_iw5::ZoneStream<'_>,
    strings: &fastfile_iw5::ScriptStrings,
    map: fastfile_iw5::ScriptStringMap,
    cap: usize,
) -> Vec<(String, String)> {
    map.pairs(stream, cap)
        .filter_map(|(key_id, val_id)| {
            let key = strings.get(stream, key_id).filter(|s| !s.is_empty())?;
            let val = strings
                .get(stream, val_id)
                .filter(|s| !s.is_empty())
                .unwrap_or(key);
            Some((key.to_owned(), val.to_owned()))
        })
        .collect()
}

pub(super) fn read_hide_tags_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    strings: &fastfile_iw5::ScriptStrings,
    arr: Option<fastfile_iw5::Ptr>,
) -> Vec<String> {
    let Some(arr) = arr else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in 0..32usize {
        let Ok(id) = stream.u16_at(arr, i * 2) else {
            break;
        };
        if id == 0 {
            continue;
        }
        if let Some(name) = strings.get(stream, id) {
            if !name.is_empty() {
                out.push(name.to_owned());
            }
        }
    }
    out
}
