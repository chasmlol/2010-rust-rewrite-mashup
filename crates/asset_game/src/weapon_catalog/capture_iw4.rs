use super::*;

impl WeaponCatalog {
    pub fn capture(&mut self, stream: &ZoneStream<'_>) {
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
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let world_model = geometry
            .world_model_name
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let camo = |names: &[Option<Ptr>; 16]| -> Vec<(u8, String)> {
            (1..16u8)
                .filter_map(|slot| {
                    let name = stream.cstr(names[usize::from(slot)]?).ok()?;
                    (!name.is_empty()).then(|| (slot, name.to_owned()))
                })
                .collect()
        };
        let camo_models = WeaponCamoModels {
            view: camo(&geometry.gun_xmodel_names),
            world: camo(&geometry.world_model_names),
        };
        let projectile_model = geometry
            .projectile_model_name
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let rocket_model = geometry
            .rocket_model_name
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let sz_xanims = geometry
            .sz_xanims
            .map(|arr| read_sz_xanims(stream, arr))
            .unwrap_or([const { None }; WEAPON_ANIM_SLOTS]);
        let sz_xanims_right = geometry
            .sz_xanims_right
            .map(|arr| read_sz_xanims(stream, arr))
            .unwrap_or([const { None }; WEAPON_ANIM_SLOTS]);
        let sz_xanims_left = geometry
            .sz_xanims_left
            .map(|arr| read_sz_xanims(stream, arr))
            .unwrap_or([const { None }; WEAPON_ANIM_SLOTS]);
        let hide_tags = read_hide_tags(stream, &self.strings, geometry.hide_tags);
        self.entries.push(CatalogWeapon {
            namespace: self
                .capture_ns
                .expect("asset capture requires an explicit family"),
            impact_payload: None,
            alternate_weapon: geometry
                .alternate_weapon_name
                .and_then(|p| read_name(stream, p)),
            name: name.to_owned(),
            weap_def: geometry.weap_def.map(ptr_key),
            display_name_key: geometry.display_name.and_then(|ptr| read_name(stream, ptr)),
            reticle: WeaponReticleAssets {
                center_material: None,
                side_material: None,
                center_edge: AssetEdge::Absent,
                side_edge: AssetEdge::Absent,
                center_image: None,
                side_image: None,
                center_size: geometry.reticle_center_size,
                side_size: geometry.i_reticle_side_size,
                center_authored: geometry.reticle_center_material_slot.is_some(),
                side_authored: geometry.reticle_side_material_slot.is_some(),
            },
            hud_material_edges: WeaponHudMaterialEdges::default(),
            overlay_material: None,
            overlay_image: None,
            reticle_center_slot: geometry.reticle_center_material_slot,
            reticle_side_slot: geometry.reticle_side_material_slot,
            overlay_material_slot: geometry.overlay_material_slot,
            iw5_attachment_slots: std::array::from_fn(|_| None),
            attached_models: Default::default(),
            t6_clip_models: Default::default(),
            t6_attachments: Vec::new(),
            t6_attachment_stats: Vec::new(),
            iw5_reload_overrides: Vec::new(),
            iw5_anim_overrides: Vec::new(),
            iw5_fx_overrides: Vec::new(),
            iw5_notetrack_overrides: Vec::new(),
            hud_icon: None,
            hud_icon_slot: geometry.hud_icon_slot,
            pickup_icon: None,
            pickup_icon_slot: geometry.pickup_icon_slot,
            pickup_icon_image: None,
            pickup_icon_ratio: geometry.pickup_icon_ratio,
            hud_icon_ratio: geometry.hud_icon_ratio,
            hud_icon_image: None,
            dpad_icon: geometry.dpad_icon_name.and_then(|p| read_name(stream, p)),
            dpad_icon_image: None,
            dpad_icon_atlas: None,
            dpad_icon_ratio: geometry.dpad_icon_ratio,
            kill_icon: geometry
                .kill_icon_name
                .and_then(|ptr| read_name(stream, ptr)),
            kill_icon_slot: geometry.kill_icon_slot,
            kill_icon_image: None,
            proj_trail: None,
            proj_trail_slot: geometry.proj_trail_slot,
            proj_beacon: None,
            proj_beacon_slot: geometry.proj_beacon_slot,
            proj_ignition: None,
            proj_ignition_slot: geometry.proj_ignition_slot,
            projectile_fx: WeaponProjectileFx::default(),
            gun_xmodel,
            hand_xmodel,
            world_model,
            camo_models,
            skin_parent: None,
            projectile_model,
            rocket_model,
            knife_xmodel: None,
            dual_wield_weapon: None,
            sz_xanims,
            sz_xanims_right,
            sz_xanims_left,
            hide_tags,
            sounds: WeaponSoundAliases {
                notetrack_convention: NotetrackConvention::SoundMap,
                fire: geometry
                    .fire_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                fire_player: geometry
                    .fire_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                empty_fire: geometry
                    .empty_fire_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                empty_fire_player: geometry
                    .empty_fire_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                melee_swipe: geometry
                    .melee_swipe_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                melee_swipe_player: geometry
                    .melee_swipe_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                melee_hit: geometry
                    .melee_hit_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                melee_miss: geometry
                    .melee_miss_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                pickup: geometry
                    .pickup_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                pickup_player: geometry
                    .pickup_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                ammo_pickup: geometry
                    .ammo_pickup_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                ammo_pickup_player: geometry
                    .ammo_pickup_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                pullback: geometry
                    .pullback_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                pullback_player: geometry
                    .pullback_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                reload: geometry
                    .reload_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                reload_player: geometry
                    .reload_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                reload_empty: geometry
                    .reload_empty_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                reload_empty_player: geometry
                    .reload_empty_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                reload_start: geometry
                    .reload_start_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                reload_start_player: geometry
                    .reload_start_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                reload_end: geometry
                    .reload_end_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                reload_end_player: geometry
                    .reload_end_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                rechamber: geometry
                    .rechamber_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                rechamber_player: geometry
                    .rechamber_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                alt_switch: geometry
                    .alt_switch_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                alt_switch_player: geometry
                    .alt_switch_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                raise: geometry
                    .raise_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                raise_player: geometry
                    .raise_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                first_raise: geometry
                    .first_raise_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                first_raise_player: geometry
                    .first_raise_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                putaway: geometry
                    .putaway_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                putaway_player: geometry
                    .putaway_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                proj_explosion: geometry
                    .proj_explosion_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                projectile: geometry
                    .projectile_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                proj_ignition_sound: geometry
                    .proj_ignition_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                bounce: geometry
                    .bounce_sound_names
                    .map(|slot| slot.and_then(|ptr| read_name(stream, ptr))),
                notetrack_sound_map: read_script_string_map(
                    stream,
                    &self.strings,
                    geometry.notetrack_sound_keys,
                    geometry.notetrack_sound_values,
                ),
                notetrack_rumble_map: read_script_string_map(
                    stream,
                    &self.strings,
                    geometry.notetrack_rumble_keys,
                    geometry.notetrack_rumble_values,
                ),
                fire_player_akimbo: None,
                fire_loop: None,
                fire_loop_player: None,
                fire_stop: None,
                fire_stop_player: None,
                fire_last: geometry
                    .fire_last_sound_name
                    .and_then(|ptr| read_name(stream, ptr)),
                fire_last_player: geometry
                    .fire_last_sound_player_name
                    .and_then(|ptr| read_name(stream, ptr)),
                leftover_sound_overrides: Vec::new(),
                fire_ptr_kind: None,
                fire_player_ptr_kind: None,
                reload_player_ptr_kind: None,
            },
            combat_slots: CombatFxSlots {
                view_flash: geometry.view_flash_slot,
                world_flash: geometry.world_flash_slot,
                view_shell_eject: geometry.view_shell_eject_slot,
                world_shell_eject: geometry.world_shell_eject_slot,
                view_last_shot_eject: geometry.view_last_shot_eject_slot,
                world_last_shot_eject: geometry.world_last_shot_eject_slot,
                explosion: geometry.explosion_slot,
                tracer: geometry.tracer_slot,
            },
            combat_fx: WeaponCombatFx {
                last_shot_eject_pair_authored: geometry.view_last_shot_eject_slot.is_some()
                    && geometry.world_last_shot_eject_slot.is_some(),
                ..WeaponCombatFx::empty(crate::AssetNamespace::Iw4)
            },
            facts: WeaponBodyFacts {
                body_resolved: geometry.weap_def.is_some(),
                fire_time_ms: geometry.fire_time_ms,
                impact_type: geometry.impact_type,
                raise_time_ms: geometry.raise_time_ms,
                drop_time_ms: geometry.drop_time_ms,
                alternate_raise_time_ms: geometry.alternate_raise_time_ms,
                alternate_drop_time_ms: geometry.alternate_drop_time_ms,
                first_raise_time_ms: geometry.first_raise_time_ms,
                fire_delay_ms: geometry.fire_delay_ms,
                hold_fire_time_ms: geometry.hold_fire_time_ms,
                weap_type: geometry.weap_type,
                weap_class: geometry.weap_class,
                player_anim_type: geometry.player_anim_type,
                offhand_class: geometry.offhand_class,
                shots_per_fire: geometry.shots_per_fire,
                ammo_index: geometry.ammo_index,
                clip_index: geometry.clip_index,
                ammo_counter_clip: geometry.ammo_counter_clip,
                low_ammo_warning_threshold: geometry.low_ammo_warning_threshold,
                hip_spread_stand_min: geometry.hip_spread_stand_min,
                hip_spread_ducked_min: geometry.hip_spread_ducked_min,
                hip_spread_prone_min: geometry.hip_spread_prone_min,
                hip_spread_stand_max: geometry.hip_spread_stand_max,
                hip_spread_ducked_max: geometry.hip_spread_ducked_max,
                hip_spread_prone_max: geometry.hip_spread_prone_max,
                hip_spread_decay_rate: geometry.hip_spread_decay_rate,
                hip_spread_fire_add: geometry.hip_spread_fire_add,
                hip_spread_turn_add: geometry.hip_spread_turn_add,
                hip_spread_move_add: geometry.hip_spread_move_add,
                hip_spread_ducked_decay: geometry.hip_spread_ducked_decay,
                hip_spread_prone_decay: geometry.hip_spread_prone_decay,
                i_reticle_side_size: geometry.i_reticle_side_size,
                i_reticle_min_ofs: geometry.i_reticle_min_ofs,
                hip_reticle_side_pos: geometry.hip_reticle_side_pos,
                ads_aim_pitch: geometry.ads_aim_pitch,
                ads_crosshair_in_frac: geometry.ads_crosshair_in_frac,
                ads_crosshair_out_frac: geometry.ads_crosshair_out_frac,
                ads_spread: geometry.ads_spread,
                can_hold_breath: geometry.overlay_reticle != 0 && geometry.weap_class != 11,
                aim_down_sight: geometry.aim_down_sight,
                thermal_scope: geometry.thermal_scope,
                silenced: geometry.weap_def.is_some_and(|body| {
                    stream.u8_at(body, stream.layout(0x670, 2168)).unwrap_or(0) != 0
                }),
                ads_zoom_fov: geometry.ads_zoom_fov,
                ads_dof: Some(geometry.ads_dof),
                ads_zoom_in_frac: geometry.ads_zoom_in_frac,
                ads_zoom_out_frac: geometry.ads_zoom_out_frac,
                no_ads_when_mag_empty: geometry.no_ads_when_mag_empty,
                inherits_perks: geometry.inherits_perks,
                ads_reload_trans_time_ms: geometry.ads_reload_trans_time_ms,
                ads_in_rate: geometry.ads_in_rate,
                ads_out_rate: geometry.ads_out_rate,
                rechamber_while_ads: geometry.rechamber_while_ads,
                ads_fire_only: geometry.ads_fire_only,
                melee_damage: geometry.melee_damage,
                overlay_reticle: geometry.overlay_reticle,
                overlay_interface: geometry.overlay_interface,
                ads_overlay_width: geometry.ads_overlay_width,
                ads_overlay_height: geometry.ads_overlay_height,
                melee_time_ms: geometry.melee_time_ms,
                melee_delay_ms: geometry.melee_delay_ms,
                melee_charge_time_ms: geometry.melee_charge_time_ms,
                melee_charge_delay_ms: geometry.melee_charge_delay_ms,
                knife_model: geometry.knife_model,
                use_as_melee: false,
                quick_raise_time_ms: geometry.quick_raise_time_ms,
                quick_drop_time_ms: geometry.quick_drop_time_ms,
                select_requires_ammo: geometry.select_requires_ammo,
                offhand_hold_is_cancelable: geometry.offhand_hold_is_cancelable,
                move_speed_scale: geometry.move_speed_scale,
                ads_move_speed_scale: geometry.ads_move_speed_scale,
                sprint_duration_scale: geometry.sprint_duration_scale,
                ducked_ofs: geometry.ducked_ofs,
                prone_ofs: geometry.prone_ofs,
                night_vision_wear_time: geometry.night_vision_wear_time,
                ads_bob_factor: geometry.ads_bob_factor,
                ads_view_bob_mult: geometry.ads_view_bob_mult,
                movement: movement_from_capture(geometry.movement),
                idle: idle_from_capture(geometry.idle),
                clip_size: geometry.clip_size,
                penetrate_type: geometry.penetrate_type,
                penetrate_multiplier: geometry.penetrate_multiplier,
                motion_tracker: geometry.motion_tracker,
                rifle_bullet: geometry.rifle_bullet,
                ricochet_chance: geometry.ricochet_chance,
                explosive_bullet: geometry.explosive_bullet,
                inventory_type: geometry.inventory_type,
                fire_type: geometry.fire_type,
                max_ammo: geometry.max_ammo,
                damage: geometry.damage,
                rechamber_time_ms: geometry.rechamber_time_ms,
                rechamber_bolt_time_ms: geometry.rechamber_bolt_time_ms,
                rechamber_bolt_delay_ms: geometry.rechamber_bolt_delay_ms,
                reload_time_ms: geometry.reload_time_ms,
                reload_show_rocket_time_ms: geometry.reload_show_rocket_time_ms,
                reload_empty_time_ms: geometry.reload_empty_time_ms,
                reload_add_time_ms: geometry.reload_add_time_ms,
                reload_empty_add_time_ms: 0,
                reload_start_time_ms: geometry.reload_start_time_ms,
                reload_start_add_time_ms: geometry.reload_start_add_time_ms,
                reload_end_time_ms: geometry.reload_end_time_ms,
                dual_mag: None,
                kill_icon_ratio: geometry.kill_icon_ratio,
                flip_kill_icon: geometry.flip_kill_icon,
                reload_ammo_add: geometry.reload_ammo_add,
                reload_start_add: geometry.reload_start_add,
                no_partial_reload: geometry.no_partial_reload,
                bolt_action: geometry.bolt_action,
                segmented_reload: geometry.segmented_reload,
                sprint_raise_time_ms: geometry.sprint_raise_time_ms,
                sprint_loop_time_ms: geometry.sprint_loop_time_ms,
                sprint_drop_time_ms: geometry.sprint_drop_time_ms,
                stunned_start_time_ms: geometry.stunned_start_time_ms,
                stunned_end_time_ms: geometry.stunned_end_time_ms,
                fuse_time_ms: geometry.fuse_time_ms,
                auto_aim_range: geometry.auto_aim_range,
                aim_assist_range: geometry.aim_assist_range,
                aim_assist_range_ads: geometry.aim_assist_range_ads,
                cook_off_hold: geometry.cook_off_hold,
                clip_only: geometry.clip_only,
                has_detonator: geometry.has_detonator,
                detonate_delay_ms: geometry.detonate_delay_ms,
                detonate_time_ms: geometry.detonate_time_ms,
                projectile_rotates: geometry.projectile_rotates,
                timed_detonation: geometry.timed_detonation,
                proj_impact_explode: geometry.proj_impact_explode,
                stick_to_players: geometry.stick_to_players,
                explosion_radius: geometry.explosion_radius,
                explosion_radius_min: geometry.explosion_radius_min,
                explosion_inner_damage: geometry.explosion_inner_damage,
                explosion_outer_damage: geometry.explosion_outer_damage,
                damage_cone_angle: geometry.damage_cone_angle,
                missile_guidance: geometry.missile_guidance,
                ignition_delay_ms: geometry.ignition_delay_ms,
                require_lock_to_fire: geometry.require_lock_to_fire,
                stickiness: geometry.stickiness,
                projectile_speed: geometry.projectile_speed,
                projectile_speed_up: geometry.projectile_speed_up,
                projectile_speed_forward: geometry.projectile_speed_forward,
                projectile_speed_relative_up: 0,
                refuses_pickup: false,
                projectile_activate_dist: geometry.projectile_activate_dist,
                projectile_explosion_type: geometry.projectile_explosion_type,
                parallel_bounce: geometry.parallel_bounce,
                perpendicular_bounce: geometry.perpendicular_bounce,
                location_damage_mult: geometry.location_damage_mult,
                start_ammo: geometry.start_ammo,
                ammo_count_clip_relative: false,
                min_damage: geometry.min_damage,
                min_player_damage: geometry.min_player_damage,
                max_damage_range: geometry.max_damage_range,
                min_damage_range: geometry.min_damage_range,
                kick: WeaponKickFacts::from_capture(geometry.kick),
                sway: WeaponSwayFacts::from_capture(geometry.sway),
                dual_wield_view_model_offset: geometry.dual_wield_view_model_offset,
                dual_wield: false,
                fire_melees: false,
                no_dual_wield: geometry.no_dual_wield,
            },
        });
    }
}
