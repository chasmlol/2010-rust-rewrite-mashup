use super::*;

pub(super) fn leftover_hip_spread_block(
    read: impl Fn(usize) -> f32,
    stand_min: usize,
) -> [f32; 12] {
    core::array::from_fn(|i| read(stand_min + i * 4))
}

pub(super) fn apply_leftover_hip_spread(facts: &mut WeaponBodyFacts, block: [f32; 12]) {
    facts.hip_spread_stand_min = block[0];
    facts.hip_spread_ducked_min = block[1];
    facts.hip_spread_prone_min = block[2];
    facts.hip_spread_stand_max = block[3];
    facts.hip_spread_ducked_max = block[4];
    facts.hip_spread_prone_max = block[5];
    facts.hip_spread_decay_rate = block[6];
    facts.hip_spread_fire_add = block[7];
    facts.hip_spread_turn_add = block[8];
    facts.hip_spread_move_add = block[9];
    facts.hip_spread_ducked_decay = block[10];
    facts.hip_spread_prone_decay = block[11];
}

pub(super) fn read_name(stream: &ZoneStream<'_>, ptr: Ptr) -> Option<String> {
    stream
        .cstr(ptr)
        .ok()
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

pub(super) fn apply_leftover_default_anim_overrides(
    sz: &mut [Option<String>; WEAPON_ANIM_SLOTS],
    overrides: &[LeftoverAnimOverride],
) {
    for ov in overrides {
        if ov.attachment1 != 0 || ov.attachment2 != 0 {
            continue;
        }
        let Some(slot) = iw5_anim_tree_type_to_iw4_slot(ov.anim_tree_type) else {
            continue;
        };
        if let Some(name) = ov.override_anim.clone() {
            sz[slot] = Some(name);
        }
    }
}

pub(super) fn apply_leftover_default_sound_overrides(sounds: &mut WeaponSoundAliases) {
    use fastfile_iw5::size as sz;
    for ov in &sounds.leftover_sound_overrides {
        if ov.attachment1 != 0 || ov.attachment2 != 0 {
            continue;
        }
        match ov.sound_type {
            x if x == sz::SND_OVERRIDE_TYPE_FIRE => {
                if sounds.fire.is_none() {
                    sounds.fire = ov.override_sound.clone();
                }
            }
            x if x == sz::SND_OVERRIDE_TYPE_PLAYER_FIRE => {
                if sounds.fire_player.is_none() {
                    sounds.fire_player = ov.override_sound.clone();
                }
            }
            x if x == sz::SND_OVERRIDE_TYPE_PLAYER_LASTSHOT => {
                if sounds.fire_last_player.is_none() {
                    sounds.fire_last_player = ov.override_sound.clone();
                }
            }
            _ => {}
        }
    }
}

pub(super) fn read_hide_tags(
    stream: &ZoneStream<'_>,
    strings: &ScriptStrings,
    arr: Option<Ptr>,
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

pub(super) fn read_script_string_map(
    stream: &ZoneStream<'_>,
    strings: &ScriptStrings,
    keys: Option<Ptr>,
    values: Option<Ptr>,
) -> Vec<(String, String)> {
    let (Some(keys), Some(values)) = (keys, values) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in 0..16usize {
        let Ok(key_id) = stream.u16_at(keys, i * 2) else {
            break;
        };
        if key_id == 0 {
            break;
        }
        let Ok(val_id) = stream.u16_at(values, i * 2) else {
            break;
        };
        let Some(key) = strings.get(stream, key_id).filter(|s| !s.is_empty()) else {
            continue;
        };
        let val = strings
            .get(stream, val_id)
            .filter(|s| !s.is_empty())
            .unwrap_or(key);
        out.push((key.to_owned(), val.to_owned()));
    }
    out
}

pub(super) fn read_sz_xanims(
    stream: &ZoneStream<'_>,
    arr: Ptr,
) -> [Option<String>; WEAPON_ANIM_SLOTS] {
    let mut out = [const { None }; WEAPON_ANIM_SLOTS];
    for (i, slot) in out.iter_mut().take(WEAPON_ANIM_COUNT).enumerate() {
        let name_ptr = match stream.ptr_at(arr, i * stream.pointer_bytes()) {
            Ok(ZonePtr::Offset(q)) => Some(stream.resolve_alias(q)),
            _ => None,
        };
        *slot = name_ptr
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
    }
    out
}

pub(super) fn merge_sz_xanims(
    dst: &mut [Option<String>; WEAPON_ANIM_SLOTS],
    src: [Option<String>; WEAPON_ANIM_SLOTS],
) {
    for (d, s) in dst.iter_mut().zip(src) {
        if d.is_none() {
            *d = s;
        }
    }
}

pub(super) fn xanims_idle(names: &[Option<String>; WEAPON_ANIM_SLOTS]) -> Option<&str> {
    names
        .get(weap_anim::IDLE)
        .and_then(|s| s.as_deref())
        .filter(|s| !s.is_empty())
}

pub(super) fn merge_sound_aliases(dst: &mut WeaponSoundAliases, src: &WeaponSoundAliases) {
    for (dst, src) in [
        (&mut dst.fire, &src.fire),
        (&mut dst.fire_player, &src.fire_player),
        (&mut dst.empty_fire, &src.empty_fire),
        (&mut dst.empty_fire_player, &src.empty_fire_player),
        (&mut dst.melee_swipe, &src.melee_swipe),
        (&mut dst.melee_swipe_player, &src.melee_swipe_player),
        (&mut dst.melee_hit, &src.melee_hit),
        (&mut dst.melee_miss, &src.melee_miss),
        (&mut dst.pickup, &src.pickup),
        (&mut dst.pickup_player, &src.pickup_player),
        (&mut dst.ammo_pickup, &src.ammo_pickup),
        (&mut dst.ammo_pickup_player, &src.ammo_pickup_player),
        (&mut dst.pullback, &src.pullback),
        (&mut dst.pullback_player, &src.pullback_player),
        (&mut dst.reload, &src.reload),
        (&mut dst.reload_player, &src.reload_player),
        (&mut dst.reload_empty, &src.reload_empty),
        (&mut dst.reload_empty_player, &src.reload_empty_player),
        (&mut dst.reload_start, &src.reload_start),
        (&mut dst.reload_start_player, &src.reload_start_player),
        (&mut dst.reload_end, &src.reload_end),
        (&mut dst.reload_end_player, &src.reload_end_player),
        (&mut dst.rechamber, &src.rechamber),
        (&mut dst.rechamber_player, &src.rechamber_player),
        (&mut dst.alt_switch, &src.alt_switch),
        (&mut dst.alt_switch_player, &src.alt_switch_player),
        (&mut dst.raise, &src.raise),
        (&mut dst.raise_player, &src.raise_player),
        (&mut dst.first_raise, &src.first_raise),
        (&mut dst.first_raise_player, &src.first_raise_player),
        (&mut dst.putaway, &src.putaway),
        (&mut dst.putaway_player, &src.putaway_player),
        (&mut dst.proj_explosion, &src.proj_explosion),
        (&mut dst.projectile, &src.projectile),
        (&mut dst.proj_ignition_sound, &src.proj_ignition_sound),
        (&mut dst.fire_player_akimbo, &src.fire_player_akimbo),
        (&mut dst.fire_loop, &src.fire_loop),
        (&mut dst.fire_loop_player, &src.fire_loop_player),
        (&mut dst.fire_stop, &src.fire_stop),
        (&mut dst.fire_stop_player, &src.fire_stop_player),
        (&mut dst.fire_last, &src.fire_last),
        (&mut dst.fire_last_player, &src.fire_last_player),
    ] {
        if dst.is_none() {
            *dst = src.clone();
        }
    }
    for (dst, src) in [
        (&mut dst.fire_ptr_kind, src.fire_ptr_kind),
        (&mut dst.fire_player_ptr_kind, src.fire_player_ptr_kind),
        (&mut dst.reload_player_ptr_kind, src.reload_player_ptr_kind),
    ] {
        if dst.is_none() {
            *dst = src;
        }
    }
    for (dst, src) in dst.bounce.iter_mut().zip(src.bounce.iter()) {
        if dst.is_none() {
            *dst = src.clone();
        }
    }
    if dst.notetrack_sound_map.is_empty() && !src.notetrack_sound_map.is_empty() {
        dst.notetrack_sound_map = src.notetrack_sound_map.clone();
    }
    if dst.notetrack_rumble_map.is_empty() && !src.notetrack_rumble_map.is_empty() {
        dst.notetrack_rumble_map = src.notetrack_rumble_map.clone();
    }
}

pub(super) fn merge_combat_fx(dst: &mut WeaponCombatFx, src: &WeaponCombatFx) {
    merge_fx_edge(
        &mut dst.view_flash,
        &mut dst.view_flash_hint,
        src.view_flash,
        &src.view_flash_hint,
    );
    merge_fx_edge(
        &mut dst.world_flash,
        &mut dst.world_flash_hint,
        src.world_flash,
        &src.world_flash_hint,
    );
    merge_fx_edge(
        &mut dst.view_shell_eject,
        &mut dst.view_shell_eject_hint,
        src.view_shell_eject,
        &src.view_shell_eject_hint,
    );
    merge_fx_edge(
        &mut dst.world_shell_eject,
        &mut dst.world_shell_eject_hint,
        src.world_shell_eject,
        &src.world_shell_eject_hint,
    );
    merge_fx_edge(
        &mut dst.view_last_shot_eject,
        &mut dst.view_last_shot_eject_hint,
        src.view_last_shot_eject,
        &src.view_last_shot_eject_hint,
    );
    merge_fx_edge(
        &mut dst.world_last_shot_eject,
        &mut dst.world_last_shot_eject_hint,
        src.world_last_shot_eject,
        &src.world_last_shot_eject_hint,
    );
    merge_fx_edge(
        &mut dst.explosion,
        &mut dst.explosion_hint,
        src.explosion,
        &src.explosion_hint,
    );
    if dst.tracer.is_absent() {
        dst.tracer = src.tracer;
        if dst.tracer_hint.is_none() {
            dst.tracer_hint = src.tracer_hint.clone();
        }
    }
    dst.last_shot_eject_pair_authored |= src.last_shot_eject_pair_authored;
}

pub(super) fn merge_combat_slots(dst: &mut CombatFxSlots, src: &CombatFxSlots) {
    if dst.view_flash.is_none() {
        dst.view_flash = src.view_flash;
    }
    if dst.world_flash.is_none() {
        dst.world_flash = src.world_flash;
    }
    if dst.view_shell_eject.is_none() {
        dst.view_shell_eject = src.view_shell_eject;
    }
    if dst.world_shell_eject.is_none() {
        dst.world_shell_eject = src.world_shell_eject;
    }
    if dst.view_last_shot_eject.is_none() {
        dst.view_last_shot_eject = src.view_last_shot_eject;
    }
    if dst.world_last_shot_eject.is_none() {
        dst.world_last_shot_eject = src.world_last_shot_eject;
    }
    if dst.explosion.is_none() {
        dst.explosion = src.explosion;
    }
    if dst.tracer.is_none() {
        dst.tracer = src.tracer;
    }
}

pub(super) fn merge_fx_edge(
    dst_edge: &mut AssetEdge<FxSpace>,
    dst_hint: &mut Option<String>,
    src_edge: AssetEdge<FxSpace>,
    src_hint: &Option<String>,
) {
    if dst_edge.is_absent() {
        *dst_edge = src_edge;
        if dst_hint.is_none() {
            *dst_hint = src_hint.clone();
        }
    }
}

pub(super) fn kick_body_captured(k: &WeaponKickFacts) -> bool {
    k.hip_view_kick_pitch_min != 0.0
        || k.hip_view_kick_pitch_max != 0.0
        || k.ads_view_kick_pitch_min != 0.0
        || k.ads_view_kick_pitch_max != 0.0
        || k.hip_gun_kick_pitch_min != 0.0
        || k.hip_gun_kick_pitch_max != 0.0
        || k.ads_gun_kick_pitch_min != 0.0
        || k.ads_gun_kick_pitch_max != 0.0
        || k.hip_gun_kick_accel != 0.0
        || k.ads_gun_kick_accel != 0.0
}

pub(super) fn sway_body_captured(s: &WeaponSwayFacts) -> bool {
    s.sway_max_angle != 0.0
        || s.sway_lerp_speed != 0.0
        || s.sway_pitch_scale != 0.0
        || s.sway_yaw_scale != 0.0
        || s.ads_sway_max_angle != 0.0
        || s.ads_sway_lerp_speed != 0.0
}

pub(super) fn stance_ofs_captured(duck: &[f32; 3], prone: &[f32; 3]) -> bool {
    duck.iter().any(|v| *v != 0.0) || prone.iter().any(|v| *v != 0.0)
}

pub(super) fn movement_ofs_captured(m: &WeaponMovementOfsInputs) -> bool {
    m.stand_move.iter().any(|v| *v != 0.0)
        || m.stand_rot.iter().any(|v| *v != 0.0)
        || m.strafe_move.iter().any(|v| *v != 0.0)
        || m.strafe_rot.iter().any(|v| *v != 0.0)
        || m.ducked_move.iter().any(|v| *v != 0.0)
        || m.ducked_rot.iter().any(|v| *v != 0.0)
        || m.prone_move.iter().any(|v| *v != 0.0)
        || m.prone_rot.iter().any(|v| *v != 0.0)
        || m.pos_move_rate != 0.0
        || m.pos_prone_move_rate != 0.0
        || m.stand_move_min_speed != 0.0
        || m.ducked_move_min_speed != 0.0
        || m.prone_move_min_speed != 0.0
        || m.pos_rot_rate != 0.0
        || m.pos_prone_rot_rate != 0.0
}

pub(super) fn movement_from_capture(c: WeaponMovementOfsCapture) -> WeaponMovementOfsInputs {
    WeaponMovementOfsInputs {
        stand_move: c.stand_move,
        stand_rot: c.stand_rot,
        strafe_move: c.strafe_move,
        strafe_rot: c.strafe_rot,
        ducked_move: c.ducked_move,
        ducked_rot: c.ducked_rot,
        prone_move: c.prone_move,
        prone_rot: c.prone_rot,
        pos_move_rate: c.pos_move_rate,
        pos_prone_move_rate: c.pos_prone_move_rate,
        stand_move_min_speed: c.stand_move_min_speed,
        ducked_move_min_speed: c.ducked_move_min_speed,
        prone_move_min_speed: c.prone_move_min_speed,
        pos_rot_rate: c.pos_rot_rate,
        pos_prone_rot_rate: c.pos_prone_rot_rate,
    }
}

pub(super) fn idle_captured(i: &WeaponIdleInputs) -> bool {
    i.ads_idle_amount != 0.0
        || i.hip_idle_amount != 0.0
        || i.ads_idle_speed != 0.0
        || i.hip_idle_speed != 0.0
        || i.idle_crouch_factor != 0.0
        || i.idle_prone_factor != 0.0
}

pub(super) fn idle_from_capture(c: WeaponIdleCapture) -> WeaponIdleInputs {
    WeaponIdleInputs {
        ads_idle_amount: c.ads_idle_amount,
        hip_idle_amount: c.hip_idle_amount,
        ads_idle_speed: c.ads_idle_speed,
        hip_idle_speed: c.hip_idle_speed,
        idle_crouch_factor: c.idle_crouch_factor,
        idle_prone_factor: c.idle_prone_factor,
    }
}

pub(super) fn merge_body_facts(dst: &mut WeaponBodyFacts, src: WeaponBodyFacts) {
    dst.dual_wield |= src.dual_wield;
    if dst.fire_time_ms == 0 {
        dst.fire_time_ms = src.fire_time_ms;
    }
    if dst.impact_type == 0 && src.impact_type != 0 {
        dst.impact_type = src.impact_type;
    }

    if !dst.body_resolved && src.body_resolved {
        let fire_time_ms = dst.fire_time_ms;
        let ads_zoom_fov = dst.ads_zoom_fov;
        let ads_dof = dst.ads_dof;
        let ads_cs = dst.kick.f_ads_view_kick_center_speed;
        let hip_cs = dst.kick.f_hip_view_kick_center_speed;
        *dst = src;
        dst.fire_time_ms = fire_time_ms;
        dst.ads_zoom_fov = ads_zoom_fov;
        dst.ads_dof = ads_dof;

        dst.kick.f_ads_view_kick_center_speed = ads_cs;
        dst.kick.f_hip_view_kick_center_speed = hip_cs;
        return;
    }

    if dst.raise_time_ms == 0 {
        dst.raise_time_ms = src.raise_time_ms;
    }

    if !kick_body_captured(&dst.kick) && kick_body_captured(&src.kick) {
        let ads_cs = dst.kick.f_ads_view_kick_center_speed;
        let hip_cs = dst.kick.f_hip_view_kick_center_speed;
        dst.kick = src.kick;
        if ads_cs != 0.0 {
            dst.kick.f_ads_view_kick_center_speed = ads_cs;
        }
        if hip_cs != 0.0 {
            dst.kick.f_hip_view_kick_center_speed = hip_cs;
        }
    }
    if dst.kick.f_ads_view_kick_center_speed == 0.0 {
        dst.kick.f_ads_view_kick_center_speed = src.kick.f_ads_view_kick_center_speed;
    }
    if dst.kick.f_hip_view_kick_center_speed == 0.0 {
        dst.kick.f_hip_view_kick_center_speed = src.kick.f_hip_view_kick_center_speed;
    }
    if !sway_body_captured(&dst.sway) && sway_body_captured(&src.sway) {
        dst.sway = src.sway;
    }
    if !stance_ofs_captured(&dst.ducked_ofs, &dst.prone_ofs)
        && stance_ofs_captured(&src.ducked_ofs, &src.prone_ofs)
    {
        dst.ducked_ofs = src.ducked_ofs;
        dst.prone_ofs = src.prone_ofs;
    }
    if dst.night_vision_wear_time == 0 {
        dst.night_vision_wear_time = src.night_vision_wear_time;
    }

    if !movement_ofs_captured(&dst.movement) && movement_ofs_captured(&src.movement) {
        dst.movement = src.movement;
    }
    if !idle_captured(&dst.idle) && idle_captured(&src.idle) {
        dst.idle = src.idle;
    }
    if dst.select_requires_ammo.is_none() {
        dst.select_requires_ammo = src.select_requires_ammo;
        dst.quick_drop_time_ms = src.quick_drop_time_ms;
    }
    if dst.offhand_hold_is_cancelable.is_none() {
        dst.offhand_hold_is_cancelable = src.offhand_hold_is_cancelable;
    }
    if dst.drop_time_ms == 0 {
        dst.drop_time_ms = src.drop_time_ms;
    }
    if dst.offhand_class == 0 && src.offhand_class != 0 {
        dst.offhand_class = src.offhand_class;
    }
    if dst.move_speed_scale == 0.0 {
        dst.move_speed_scale = src.move_speed_scale;
    }
    if dst.ads_move_speed_scale == 0.0 {
        dst.ads_move_speed_scale = src.ads_move_speed_scale;
    }
    if dst.sprint_duration_scale == 0.0 {
        dst.sprint_duration_scale = src.sprint_duration_scale;
    }

    if dst.clip_size == 0 {
        dst.clip_size = src.clip_size;
    }
    if dst.fire_type == 0 && src.fire_type != 0 {
        dst.fire_type = src.fire_type;
    }
    if dst.max_ammo == 0 {
        dst.max_ammo = src.max_ammo;
    }
    if dst.damage == 0 {
        dst.damage = src.damage;
    }
    if dst.rechamber_time_ms == 0 {
        dst.rechamber_time_ms = src.rechamber_time_ms;
    }
    if dst.reload_time_ms == 0 {
        dst.reload_time_ms = src.reload_time_ms;
    }
    if dst.reload_show_rocket_time_ms == 0 {
        dst.reload_show_rocket_time_ms = src.reload_show_rocket_time_ms;
    }
    if dst.reload_empty_time_ms == 0 {
        dst.reload_empty_time_ms = src.reload_empty_time_ms;
    }
    if dst.reload_add_time_ms == 0 {
        dst.reload_add_time_ms = src.reload_add_time_ms;
    }
    if dst.reload_empty_add_time_ms == 0 {
        dst.reload_empty_add_time_ms = src.reload_empty_add_time_ms;
    }
    if dst.reload_start_time_ms == 0 {
        dst.reload_start_time_ms = src.reload_start_time_ms;
    }
    if dst.reload_start_add_time_ms == 0 {
        dst.reload_start_add_time_ms = src.reload_start_add_time_ms;
    }
    if dst.reload_end_time_ms == 0 {
        dst.reload_end_time_ms = src.reload_end_time_ms;
    }
    if dst.reload_ammo_add == 0 {
        dst.reload_ammo_add = src.reload_ammo_add;
    }
    if dst.reload_start_add == 0 {
        dst.reload_start_add = src.reload_start_add;
    }
    if dst.fuse_time_ms == 0 {
        dst.fuse_time_ms = src.fuse_time_ms;
    }
    if dst.aim_assist_range == 0.0 {
        dst.auto_aim_range = src.auto_aim_range;
        dst.aim_assist_range = src.aim_assist_range;
        dst.aim_assist_range_ads = src.aim_assist_range_ads;
    }
    if dst.sprint_raise_time_ms == 0 {
        dst.sprint_raise_time_ms = src.sprint_raise_time_ms;
    }
    if dst.sprint_loop_time_ms == 0 {
        dst.sprint_loop_time_ms = src.sprint_loop_time_ms;
    }
    if dst.sprint_drop_time_ms == 0 {
        dst.sprint_drop_time_ms = src.sprint_drop_time_ms;
    }
    if dst.stunned_start_time_ms == 0 {
        dst.stunned_start_time_ms = src.stunned_start_time_ms;
    }
    if dst.stunned_end_time_ms == 0 {
        dst.stunned_end_time_ms = src.stunned_end_time_ms;
    }
    if dst.hold_fire_time_ms == 0 {
        dst.hold_fire_time_ms = src.hold_fire_time_ms;
    }
    if !dst.cook_off_hold {
        dst.cook_off_hold = src.cook_off_hold;
    }
    if dst.damage_cone_angle == 0.0 {
        dst.damage_cone_angle = src.damage_cone_angle;
    }
    dst.has_detonator |= src.has_detonator;
    dst.projectile_rotates |= src.projectile_rotates;
    if dst.detonate_delay_ms == 0 {
        dst.detonate_delay_ms = src.detonate_delay_ms;
    }
    if dst.detonate_time_ms == 0 {
        dst.detonate_time_ms = src.detonate_time_ms;
    }
    if !dst.timed_detonation {
        dst.timed_detonation = src.timed_detonation;
    }
    if !dst.clip_only {
        dst.clip_only = src.clip_only;
    }
    if !dst.proj_impact_explode {
        dst.proj_impact_explode = src.proj_impact_explode;
    }
    if !dst.stick_to_players {
        dst.stick_to_players = src.stick_to_players;
    }
    if !dst.ads_fire_only {
        dst.ads_fire_only = src.ads_fire_only;
    }
    if dst.explosion_radius == 0 {
        dst.explosion_radius = src.explosion_radius;
    }
    if dst.explosion_radius_min == 0 {
        dst.explosion_radius_min = src.explosion_radius_min;
    }
    if dst.explosion_inner_damage == 0 {
        dst.explosion_inner_damage = src.explosion_inner_damage;
    }
    if dst.explosion_outer_damage == 0 {
        dst.explosion_outer_damage = src.explosion_outer_damage;
    }
    if dst.missile_guidance == 0 {
        dst.missile_guidance = src.missile_guidance;
    }
    if dst.ignition_delay_ms == 0 {
        dst.ignition_delay_ms = src.ignition_delay_ms;
    }
    dst.require_lock_to_fire |= src.require_lock_to_fire;
    if dst.projectile_speed == 0 {
        dst.projectile_speed = src.projectile_speed;
    }
    if dst.projectile_speed_up == 0 {
        dst.projectile_speed_up = src.projectile_speed_up;
    }
    if dst.projectile_speed_forward == 0 {
        dst.projectile_speed_forward = src.projectile_speed_forward;
    }
    if dst.projectile_activate_dist == 0 {
        dst.projectile_activate_dist = src.projectile_activate_dist;
    }
    if dst.projectile_explosion_type == 0 {
        dst.projectile_explosion_type = src.projectile_explosion_type;
    }
    if dst.parallel_bounce.is_none() {
        dst.parallel_bounce = src.parallel_bounce;
    }
    if dst.perpendicular_bounce.is_none() {
        dst.perpendicular_bounce = src.perpendicular_bounce;
    }
    if dst.location_damage_mult.is_none() {
        dst.location_damage_mult = src.location_damage_mult;
    }
    if dst.penetrate_type == 0 && src.penetrate_type != 0 {
        dst.penetrate_type = src.penetrate_type;
    }
    if dst.penetrate_multiplier == 0.0 && src.penetrate_multiplier != 0.0 {
        dst.penetrate_multiplier = src.penetrate_multiplier;
    }
    if !dst.rifle_bullet && src.rifle_bullet {
        dst.rifle_bullet = true;
    }
    if dst.inventory_type == 0 && src.inventory_type != 0 {
        dst.inventory_type = src.inventory_type;
    }
    if dst.start_ammo == 0 {
        dst.start_ammo = src.start_ammo;
    }
    if !dst.ammo_count_clip_relative && src.ammo_count_clip_relative {
        dst.ammo_count_clip_relative = true;
    }
    if dst.min_damage == 0 {
        dst.min_damage = src.min_damage;
    }
    if dst.min_player_damage == 0 {
        dst.min_player_damage = src.min_player_damage;
    }
    if dst.max_damage_range == 0.0 {
        dst.max_damage_range = src.max_damage_range;
    }
    if dst.min_damage_range == 0.0 {
        dst.min_damage_range = src.min_damage_range;
    }
    if !dst.inherits_perks && src.inherits_perks {
        dst.inherits_perks = true;
    }
    if !dst.no_partial_reload && src.no_partial_reload {
        dst.no_partial_reload = true;
    }
    if !dst.segmented_reload && src.segmented_reload {
        dst.segmented_reload = true;
    }
    if dst.rechamber_bolt_delay_ms == 0 && src.rechamber_bolt_delay_ms != 0 {
        dst.rechamber_bolt_delay_ms = src.rechamber_bolt_delay_ms;
    }
    if !dst.rechamber_while_ads && src.rechamber_while_ads {
        dst.rechamber_while_ads = true;
    }
    if dst.dual_wield_view_model_offset == 0.0 {
        dst.dual_wield_view_model_offset = src.dual_wield_view_model_offset;
    }
    if dst.overlay_interface == 0 {
        dst.overlay_interface = src.overlay_interface;
    }
    if dst.overlay_reticle == 0 {
        dst.overlay_reticle = src.overlay_reticle;
        if dst.ads_overlay_width == 0.0 {
            dst.ads_overlay_width = src.ads_overlay_width;
        }
        if dst.ads_overlay_height == 0.0 {
            dst.ads_overlay_height = src.ads_overlay_height;
        }
    }
    if dst.i_reticle_side_size == 0 {
        dst.i_reticle_side_size = src.i_reticle_side_size;
    }
    if dst.i_reticle_min_ofs == 0 {
        dst.i_reticle_min_ofs = src.i_reticle_min_ofs;
    }
}
