use asset_iw4::size as sz;

use super::{
    AssetLinkSink, asset_ptr_at, asset_ptr_at_linked, follow_name, load_asset_at_observed,
};
use crate::asset_type::AssetType;
use crate::zone::{
    Ptr, Result, WeaponGeometry, WeaponIdleCapture, WeaponKickCapture, WeaponMovementOfsCapture,
    WeaponSwayCapture, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream,
};

fn authored_material_slot(
    s: &ZoneStream<'_>,
    body: Option<Ptr>,
    field: usize,
) -> Result<Option<Ptr>> {
    let Some(body) = body else {
        return Ok(None);
    };
    Ok(match s.ptr_at(body, field)? {
        ZonePtr::Null => None,
        _ => Some(body.at(field)),
    })
}

pub(super) fn load_weapon(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::WEAPON_COMPLETE_DEF, 160))?;
    let ai_vs_ai_knots = s.u16_at(p, s.layout(100, 132))? as usize;
    let ai_vs_player_knots = s.u16_at(p, s.layout(102, 134))? as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let (
        weap_def,
        gun_xmodel_names,
        hand_xmodel_name,
        world_model_names,
        projectile_model_name,
        rocket_model_name,
        sound_names,
        sz_xanims_right,
        sz_xanims_left,
    ) = match s.ptr_at(p, s.layout(4, 8))? {
        ZonePtr::Offset(q) => {
            s.note_offset(q);
            (
                Some(s.resolve_alias(q)),
                [None; 16],
                None,
                [None; 16],
                None,
                None,
                WeaponSoundNames::default(),
                None,
                None,
            )
        }
        _ if s.begin_body(p.at(s.layout(4, 8)))? => {
            let (body, gun0, hand0, world0, proj0, rocket0, sounds, anim_r, anim_l) =
                load_weapon_def(s, links, ai_vs_ai_knots, ai_vs_player_knots)?;
            (
                Some(body),
                gun0,
                hand0,
                world0,
                proj0,
                rocket0,
                sounds,
                anim_r,
                anim_l,
            )
        }
        _ => (
            None,
            [None; 16],
            None,
            [None; 16],
            None,
            None,
            WeaponSoundNames::default(),
            None,
            None,
        ),
    };

    let display_name = s.follow_string(p, s.layout(8, 16))?;
    let hide_tags = s.plain_array(p, s.layout(12, 24), 2, 2, 32)?;
    let sz_xanims = follow_string_array(s, p, s.layout(16, 32), sz::WEAPON_ANIM_COUNT)?;
    let alternate_weapon_name = s.follow_string(p, s.layout(60, 80))?;

    let kill_icon_fresh =
        asset_ptr_at_linked(s, links, AssetType::Material, p.at(s.layout(72, 96)))?;
    let kill_icon_name = if kill_icon_fresh {
        s.latest_material().and_then(|g| g.name)
    } else {
        // The icon target can dangle (unsettled TEMP alias, e.g. SP turret
        // kill icons): the name is catalog garnish, so a bad read resolves to
        // `None` instead of aborting the walk. Stream position is untouched.
        match s.ptr_at(p, s.layout(72, 96)) {
            Ok(ZonePtr::Offset(q)) => match s.ptr_at(s.resolve_alias(q), 0) {
                Ok(ZonePtr::Offset(n)) => Some(s.resolve_alias(n)),
                _ => None,
            },
            _ => None,
        }
    };
    let dpad_icon_fresh =
        asset_ptr_at_linked(s, links, AssetType::Material, p.at(s.layout(76, 104)))?;
    let dpad_icon_name = if dpad_icon_fresh {
        s.latest_material().and_then(|g| g.name)
    } else {
        match s.ptr_at(p, s.layout(76, 104)) {
            Ok(ZonePtr::Offset(q)) => match s.ptr_at(s.resolve_alias(q), 0) {
                Ok(ZonePtr::Offset(n)) => Some(s.resolve_alias(n)),
                _ => None,
            },
            _ => None,
        }
    };

    s.plain_array(p, s.layout(104, 136), 4, 8, ai_vs_ai_knots)?;
    s.plain_array(p, s.layout(108, 144), 4, 8, ai_vs_player_knots)?;

    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };

    let fire_time_ms = s.i32_at(p, s.layout(0x28, 60))?;
    let ads_zoom_fov = s.f32_at(p, s.layout(0x14, 40))?;
    let impact_type = s.i32_at(p, s.layout(0x24, 56))?;
    let clip_size = s.i32_at(p, s.layout(0x20, 52))?;
    let penetrate_multiplier = s.f32_at(p, s.layout(0x30, 68))?;

    let f_ads_view_kick_center_speed = s.f32_at(p, s.layout(0x34, 72))?;
    let f_hip_view_kick_center_speed = s.f32_at(p, s.layout(0x38, 76))?;
    let body_facts = match weap_def {
        Some(body) => WeapDefScalars {
            raise_time_ms: s.i32_at(body, s.layout(0x28c, 980))?,
            drop_time_ms: s.i32_at(body, s.layout(0x288, 976))?,
            fire_delay_ms: s.i32_at(body, s.layout(0x240, 904))?,
            hold_fire_time_ms: s.i32_at(body, s.layout(0x25c, 932))?,
            weap_type: s.i32_at(body, s.layout(0x2c, 84))?,
            weap_class: s.i32_at(body, s.layout(0x30, 88))?,
            player_anim_type: s.i32_at(body, s.layout(0x28, 80))?,
            offhand_class: s.i32_at(body, s.layout(0x40, 104))?,
            shots_per_fire: s.i32_at(body, s.layout(0x220, 864))?,
            ammo_index: s.i32_at(body, s.layout(0x210, 840))?,
            clip_index: s.i32_at(body, s.layout(0x218, 856))?,
            ammo_counter_clip: s.i32_at(body, s.layout(0x204, 820))?,
            low_ammo_warning_threshold: s.f32_at(body, s.layout(0x43c, 1460))?,
            hip_spread_stand_min: s.f32_at(body, s.layout(0x338, 1168))?,
            hip_spread_ducked_min: s.f32_at(body, s.layout(0x33c, 1172))?,
            hip_spread_prone_min: s.f32_at(body, s.layout(0x340, 1176))?,
            hip_spread_stand_max: s.f32_at(body, s.layout(0x344, 1180))?,
            hip_spread_ducked_max: s.f32_at(body, s.layout(0x348, 1184))?,
            hip_spread_prone_max: s.f32_at(body, s.layout(0x34c, 1188))?,
            hip_spread_decay_rate: s.f32_at(body, s.layout(0x350, 1192))?,
            hip_spread_fire_add: s.f32_at(body, s.layout(0x354, 1196))?,
            hip_spread_turn_add: s.f32_at(body, s.layout(0x358, 1200))?,
            hip_spread_move_add: s.f32_at(body, s.layout(0x35c, 1204))?,
            hip_spread_ducked_decay: s.f32_at(body, s.layout(0x360, 1208))?,
            hip_spread_prone_decay: s.f32_at(body, s.layout(0x364, 1212))?,
            reticle_center_size: s.i32_at(body, s.layout(0x128, 560))?,
            i_reticle_side_size: s.i32_at(body, s.layout(0x12c, 564))?,
            i_reticle_min_ofs: s.i32_at(body, s.layout(0x130, 568))?,
            hip_reticle_side_pos: s.f32_at(body, s.layout(0x368, 1216))?,
            ads_aim_pitch: s.f32_at(body, s.layout(0x474, 1544))?,
            ads_crosshair_in_frac: s.f32_at(body, s.layout(0x478, 1548))?,
            ads_crosshair_out_frac: s.f32_at(body, s.layout(0x47c, 1552))?,
            ads_spread: s.f32_at(body, s.layout(0x4c0, 1620))?,
            aim_down_sight: s.u8_at(body, s.layout(0x65f, 2151))? != 0,
            thermal_scope: s.u8_at(body, s.layout(0x67b, 2179))? != 0,
            no_ads_when_mag_empty: s.u8_at(body, s.layout(0x658, 2144))? != 0,
            inherits_perks: s.u8_at(body, s.layout(0x65a, 2146))? != 0,
            ads_reload_trans_time_ms: s.i32_at(body, s.layout(0x520, 1736))?,
            ads_in_rate: s.f32_at(body, s.layout(0x590, 1864))?,
            ads_out_rate: s.f32_at(body, s.layout(0x594, 1868))?,
            rechamber_while_ads: s.u8_at(body, s.layout(0x660, 2152))? != 0,
            ads_fire_only: s.u8_at(body, s.layout(0x665, 2157))? != 0,
            dual_wield_view_model_offset: s.f32_at(body, s.layout(0x3cc, 1320))?,
            no_dual_wield: s.u8_at(body, s.layout(0x66b, 2163))? != 0,
            melee_damage: s.i32_at(body, s.layout(0x238, 896))?,
            overlay_reticle: s.i32_at(body, s.layout(0x318, 1136))?,
            overlay_interface: s.i32_at(body, s.layout(0x31c, 1140))?,
            ads_zoom_in_frac: s.f32_at(body, s.layout(0x300, 1096))?,
            ads_zoom_out_frac: s.f32_at(body, s.layout(0x304, 1100))?,
            ads_overlay_width: s.f32_at(body, s.layout(0x320, 1144))?,
            ads_overlay_height: s.f32_at(body, s.layout(0x324, 1148))?,
            melee_time_ms: s.i32_at(body, s.layout(0x264, 940))?,
            melee_delay_ms: s.i32_at(body, s.layout(0x244, 908))?,
            melee_charge_time_ms: s.i32_at(body, s.layout(0x268, 944))?,
            melee_charge_delay_ms: s.i32_at(body, s.layout(0x248, 912))?,
            knife_model: match s.ptr_at(body, s.layout(0x1e4, 760))? {
                ZonePtr::Null => 0,
                ZonePtr::Following => u32::MAX,
                ZonePtr::Insert => u32::MAX - 1,
                ZonePtr::Offset(p) => ZonePtr::encode_offset(p),
            },
            quick_raise_time_ms: s.i32_at(body, s.layout(0x298, 992))?,
            quick_drop_time_ms: s.i32_at(body, s.layout(0x294, 988))?,
            select_requires_ammo: Some(s.u8_at(body, s.layout(0x667, 2159))? != 0),
            offhand_hold_is_cancelable: Some(s.u8_at(body, s.layout(0x681, 2185))? != 0),
            move_speed_scale: s.f32_at(body, s.layout(0x2f4, 1084))?,
            ads_move_speed_scale: s.f32_at(body, s.layout(0x2f8, 1088))?,
            sprint_duration_scale: s.f32_at(body, s.layout(0x2fc, 1092))?,
            ducked_ofs: [
                s.f32_at(body, s.layout(0x168, 624))?,
                s.f32_at(body, s.layout(0x16c, 628))?,
                s.f32_at(body, s.layout(0x170, 632))?,
            ],
            prone_ofs: [
                s.f32_at(body, s.layout(0x18c, 660))?,
                s.f32_at(body, s.layout(0x190, 664))?,
                s.f32_at(body, s.layout(0x194, 668))?,
            ],
            night_vision_wear_time: s.i32_at(body, s.layout(0x2c0, 1032))?,
            ads_bob_factor: s.f32_at(body, s.layout(0x330, 1160))?,
            ads_view_bob_mult: s.f32_at(body, s.layout(0x334, 1164))?,
            movement: load_movement_ofs(s, body)?,
            idle: load_idle(s, body)?,
            penetrate_type: s.i32_at(body, s.layout(0x34, 92))?,
            inventory_type: s.i32_at(body, s.layout(0x38, 96))?,
            fire_type: s.i32_at(body, s.layout(0x3c, 100))?,
            max_ammo: s.i32_at(body, s.layout(0x21c, 860))?,
            damage: s.i32_at(body, s.layout(0x230, 888))?,
            rechamber_time_ms: s.i32_at(body, s.layout(0x250, 920))?,
            rechamber_bolt_time_ms: s.i32_at(body, s.layout(0x254, 924))?,
            rechamber_bolt_delay_ms: s.i32_at(body, s.layout(0x258, 928))?,
            reload_time_ms: s.i32_at(body, s.layout(0x26c, 948))?,
            reload_show_rocket_time_ms: s.i32_at(body, s.layout(0x270, 952))?,
            reload_empty_time_ms: s.i32_at(body, s.layout(0x274, 956))?,
            reload_add_time_ms: s.i32_at(body, s.layout(0x278, 960))?,
            reload_start_time_ms: s.i32_at(body, s.layout(0x27c, 964))?,
            reload_start_add_time_ms: s.i32_at(body, s.layout(0x280, 968))?,
            reload_end_time_ms: s.i32_at(body, s.layout(0x284, 972))?,
            kill_icon_ratio: s.i32_at(body, s.layout(0x3d0, 1324))?,
            flip_kill_icon: s.u8_at(body, s.layout(0x66c, 2164))? != 0,
            reload_ammo_add: s.i32_at(body, s.layout(0x3d4, 1328))?,
            reload_start_add: s.i32_at(body, s.layout(0x3d8, 1332))?,
            no_partial_reload: s.u8_at(body, s.layout(0x66d, 2165))? != 0,
            bolt_action: s.u8_at(body, s.layout(0x65e, 2150))? != 0,
            rifle_bullet: s.u8_at(body, s.layout(0x65c, 2148))? != 0,
            ricochet_chance: s.f32_at(body, s.layout(0x440, 1464))?,
            explosive_bullet: s.u8_at(body, s.layout(0x661, 2153))? != 0,
            segmented_reload: s.u8_at(body, s.layout(0x66e, 2166))? != 0,
            cook_off_hold: s.u8_at(body, s.layout(0x662, 2154))? != 0,
            clip_only: s.u8_at(body, s.layout(0x663, 2155))? != 0,
            has_detonator: s.u8_at(body, s.layout(0x675, 2173))? != 0,
            detonate_delay_ms: s.i32_at(body, s.layout(0x24c, 916))?,
            detonate_time_ms: s.i32_at(body, s.layout(0x260, 936))?,
            projectile_rotates: s.u8_at(body, s.layout(0x678, 2176))? != 0,
            timed_detonation: s.u8_at(body, s.layout(0x677, 2175))? != 0,
            proj_impact_explode: s.u8_at(body, s.layout(0x673, 2171))? != 0,
            stick_to_players: s.u8_at(body, s.layout(0x674, 2172))? != 0,
            sprint_raise_time_ms: s.i32_at(body, s.layout(0x2a8, 1008))?,
            sprint_loop_time_ms: s.i32_at(body, s.layout(0x2ac, 1012))?,
            sprint_drop_time_ms: s.i32_at(body, s.layout(0x2b0, 1016))?,
            stunned_start_time_ms: s.i32_at(body, s.layout(0x2b4, 1020))?,
            stunned_end_time_ms: s.i32_at(body, s.layout(0x2bc, 1028))?,
            fuse_time_ms: s.i32_at(body, s.layout(0x2d8, 1056))?,
            auto_aim_range: s.f32_at(body, s.layout(0x2e0, 1064))?,
            aim_assist_range: s.f32_at(body, s.layout(0x2e4, 1068))?,
            aim_assist_range_ads: s.f32_at(body, s.layout(0x2e8, 1072))?,
            explosion_radius: s.i32_at(body, s.layout(0x3e8, 1348))?,
            explosion_radius_min: s.i32_at(body, s.layout(0x3ec, 1352))?,
            explosion_inner_damage: s.i32_at(body, s.layout(0x3f0, 1356))?,
            explosion_outer_damage: s.i32_at(body, s.layout(0x3f4, 1360))?,
            damage_cone_angle: s.f32_at(body, s.layout(0x3f8, 1364))?,
            projectile_speed: s.i32_at(body, s.layout(0x404, 1376))?,
            projectile_speed_up: s.i32_at(body, s.layout(0x408, 1380))?,
            projectile_speed_forward: s.i32_at(body, s.layout(0x40c, 1384))?,
            projectile_activate_dist: s.i32_at(body, s.layout(0x410, 1388))?,
            missile_guidance: s.i32_at(body, s.layout(0x460, 1516))?,
            ignition_delay_ms: s.i32_at(body, s.layout(0x468, 1524))?,
            require_lock_to_fire: s.u8_at(body, s.layout(0x656, 2142))? != 0,
            stickiness: s.i32_at(body, s.layout(0x438, 1456))?,
            projectile_explosion_type: s.i32_at(body, s.layout(0x424, 1416))?,
            parallel_bounce: read_f32_array(s, body, s.layout(0x444, 1472))?,
            perpendicular_bounce: read_f32_array(s, body, s.layout(0x448, 1480))?,
            location_damage_mult: read_f32_array(s, body, s.layout(0x5b4, 1904))?,
            start_ammo: s.i32_at(body, s.layout(0x208, 824))?,
            min_damage: s.i32_at(body, s.layout(0x598, 1872))?,
            min_player_damage: s.i32_at(body, s.layout(0x59c, 1876))?,
            max_damage_range: s.f32_at(body, s.layout(0x5a0, 1880))?,
            min_damage_range: s.f32_at(body, s.layout(0x5a4, 1884))?,
            kick: WeaponKickCapture {
                f_ads_view_kick_center_speed,
                f_hip_view_kick_center_speed,
                gun_max_pitch: s.f32_at(body, s.layout(0x384, 1244))?,
                gun_max_yaw: s.f32_at(body, s.layout(0x388, 1248))?,
                ads_gun_kick_reduced_kick_bullets: s.i32_at(body, s.layout(0x480, 1556))?,
                ads_gun_kick_reduced_kick_percent: s.f32_at(body, s.layout(0x484, 1560))?,
                ads_gun_kick_pitch_min: s.f32_at(body, s.layout(0x488, 1564))?,
                ads_gun_kick_pitch_max: s.f32_at(body, s.layout(0x48c, 1568))?,
                ads_gun_kick_yaw_min: s.f32_at(body, s.layout(0x490, 1572))?,
                ads_gun_kick_yaw_max: s.f32_at(body, s.layout(0x494, 1576))?,
                ads_gun_kick_accel: s.f32_at(body, s.layout(0x498, 1580))?,
                ads_gun_kick_speed_max: s.f32_at(body, s.layout(0x49c, 1584))?,
                ads_gun_kick_speed_decay: s.f32_at(body, s.layout(0x4a0, 1588))?,
                ads_gun_kick_static_decay: s.f32_at(body, s.layout(0x4a4, 1592))?,
                ads_view_kick_pitch_min: s.f32_at(body, s.layout(0x4a8, 1596))?,
                ads_view_kick_pitch_max: s.f32_at(body, s.layout(0x4ac, 1600))?,
                ads_view_kick_yaw_min: s.f32_at(body, s.layout(0x4b0, 1604))?,
                ads_view_kick_yaw_max: s.f32_at(body, s.layout(0x4b4, 1608))?,
                hip_gun_kick_reduced_kick_bullets: s.i32_at(body, s.layout(0x4c4, 1624))?,
                hip_gun_kick_reduced_kick_percent: s.f32_at(body, s.layout(0x4c8, 1628))?,
                hip_gun_kick_pitch_min: s.f32_at(body, s.layout(0x4cc, 1632))?,
                hip_gun_kick_pitch_max: s.f32_at(body, s.layout(0x4d0, 1636))?,
                hip_gun_kick_yaw_min: s.f32_at(body, s.layout(0x4d4, 1640))?,
                hip_gun_kick_yaw_max: s.f32_at(body, s.layout(0x4d8, 1644))?,
                hip_gun_kick_accel: s.f32_at(body, s.layout(0x4dc, 1648))?,
                hip_gun_kick_speed_max: s.f32_at(body, s.layout(0x4e0, 1652))?,
                hip_gun_kick_speed_decay: s.f32_at(body, s.layout(0x4e4, 1656))?,
                hip_gun_kick_static_decay: s.f32_at(body, s.layout(0x4e8, 1660))?,
                hip_view_kick_pitch_min: s.f32_at(body, s.layout(0x4ec, 1664))?,
                hip_view_kick_pitch_max: s.f32_at(body, s.layout(0x4f0, 1668))?,
                hip_view_kick_yaw_min: s.f32_at(body, s.layout(0x4f4, 1672))?,
                hip_view_kick_yaw_max: s.f32_at(body, s.layout(0x4f8, 1676))?,
            },
            sway: WeaponSwayCapture {
                sway_max_angle: s.f32_at(body, s.layout(0x38c, 1252))?,
                sway_lerp_speed: s.f32_at(body, s.layout(0x390, 1256))?,
                sway_pitch_scale: s.f32_at(body, s.layout(0x394, 1260))?,
                sway_yaw_scale: s.f32_at(body, s.layout(0x398, 1264))?,
                sway_horiz_scale: s.f32_at(body, s.layout(0x39c, 1268))?,
                sway_vert_scale: s.f32_at(body, s.layout(0x3a0, 1272))?,
                sway_shell_shock_scale: s.f32_at(body, s.layout(0x3a4, 1276))?,
                ads_sway_max_angle: s.f32_at(body, s.layout(0x3a8, 1280))?,
                ads_sway_lerp_speed: s.f32_at(body, s.layout(0x3ac, 1284))?,
                ads_sway_pitch_scale: s.f32_at(body, s.layout(0x3b0, 1288))?,
                ads_sway_yaw_scale: s.f32_at(body, s.layout(0x3b4, 1292))?,
                ads_sway_horiz_scale: s.f32_at(body, s.layout(0x3b8, 1296))?,
                ads_sway_vert_scale: s.f32_at(body, s.layout(0x3bc, 1300))?,
            },
        },
        None => WeapDefScalars {
            kick: WeaponKickCapture {
                f_ads_view_kick_center_speed,
                f_hip_view_kick_center_speed,
                ..WeaponKickCapture::default()
            },
            ..WeapDefScalars::default()
        },
    };
    s.record_weapon(WeaponGeometry {
        alternate_weapon_name,
        alternate_raise_time_ms: s.i32_at(p, s.layout(68, 92))?,
        first_raise_time_ms: s.i32_at(p, s.layout(84, 116))?,
        alternate_drop_time_ms: weap_def
            .map(|body| s.i32_at(body, s.layout(0x290, 984)))
            .transpose()?
            .unwrap_or_default(),
        view_flash_slot: authored_material_slot(s, weap_def, s.layout(0x48, 112))?,
        world_flash_slot: authored_material_slot(s, weap_def, s.layout(0x4c, 120))?,
        view_shell_eject_slot: authored_material_slot(s, weap_def, s.layout(0x110, 512))?,
        world_shell_eject_slot: authored_material_slot(s, weap_def, s.layout(0x114, 520))?,
        view_last_shot_eject_slot: authored_material_slot(s, weap_def, s.layout(0x118, 528))?,
        world_last_shot_eject_slot: authored_material_slot(s, weap_def, s.layout(0x11c, 536))?,
        explosion_slot: authored_material_slot(s, weap_def, s.layout(0x428, 1424))?,
        tracer_slot: authored_material_slot(s, weap_def, s.layout(0x5c0, 1928))?,
        name,
        weap_def,
        display_name,

        reticle_center_material_slot: authored_material_slot(s, weap_def, s.layout(0x120, 544))?,
        reticle_side_material_slot: authored_material_slot(s, weap_def, s.layout(0x124, 552))?,
        overlay_material_slot: authored_material_slot(s, weap_def, s.layout(0x308, 1104))?,
        hud_icon_slot: authored_material_slot(s, weap_def, s.layout(0x1ec, 776))?,
        pickup_icon_slot: authored_material_slot(s, weap_def, s.layout(0x1f4, 792))?,
        pickup_icon_ratio: weap_def
            .map(|body| s.i32_at(body, s.layout(0x1f8, 800)))
            .transpose()?
            .unwrap_or(0),
        hud_icon_ratio: weap_def
            .map(|body| s.i32_at(body, s.layout(0x1f0, 784)))
            .transpose()?
            .unwrap_or(0),

        kill_icon_slot: match s.ptr_at(p, s.layout(72, 96))? {
            ZonePtr::Null => None,
            ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
            _ => Some(p.at(s.layout(72, 96))),
        },
        kill_icon_name,
        dpad_icon_name,
        dpad_icon_ratio: s.i32_at(p, s.layout(0x2c, 64))?,
        motion_tracker: s.u8_at(p, s.layout(112, 152))? != 0,
        proj_trail_slot: authored_material_slot(s, weap_def, s.layout(0x44c, 1488))?,
        proj_beacon_slot: authored_material_slot(s, weap_def, s.layout(0x450, 1496))?,
        proj_ignition_slot: authored_material_slot(s, weap_def, s.layout(0x46c, 1528))?,
        reticle_center_size: body_facts.reticle_center_size,
        gun_xmodel_name: gun_xmodel_names[0],
        gun_xmodel_names,
        hand_xmodel_name,
        world_model_name: world_model_names[0],
        world_model_names,
        projectile_model_name,
        rocket_model_name,
        sz_xanims,
        sz_xanims_right,
        sz_xanims_left,
        hide_tags,
        notetrack_sound_keys: sound_names.notetrack_sound_keys,
        notetrack_sound_values: sound_names.notetrack_sound_values,
        notetrack_rumble_keys: sound_names.notetrack_rumble_keys,
        notetrack_rumble_values: sound_names.notetrack_rumble_values,
        fire_sound_name: sound_names.fire,
        fire_sound_player_name: sound_names.fire_player,
        fire_last_sound_name: sound_names.fire_last,
        fire_last_sound_player_name: sound_names.fire_last_player,
        empty_fire_sound_name: sound_names.empty_fire,
        empty_fire_sound_player_name: sound_names.empty_fire_player,
        melee_swipe_sound_name: sound_names.melee_swipe,
        melee_swipe_sound_player_name: sound_names.melee_swipe_player,
        melee_hit_sound_name: sound_names.melee_hit,
        melee_miss_sound_name: sound_names.melee_miss,
        pickup_sound_name: sound_names.pickup,
        pickup_sound_player_name: sound_names.pickup_player,
        ammo_pickup_sound_name: sound_names.ammo_pickup,
        ammo_pickup_sound_player_name: sound_names.ammo_pickup_player,
        pullback_sound_name: sound_names.pullback,
        pullback_sound_player_name: sound_names.pullback_player,
        reload_sound_name: sound_names.reload,
        reload_sound_player_name: sound_names.reload_player,
        reload_empty_sound_name: sound_names.reload_empty,
        reload_empty_sound_player_name: sound_names.reload_empty_player,
        reload_start_sound_name: sound_names.reload_start,
        reload_start_sound_player_name: sound_names.reload_start_player,
        reload_end_sound_name: sound_names.reload_end,
        reload_end_sound_player_name: sound_names.reload_end_player,
        rechamber_sound_name: sound_names.rechamber,
        rechamber_sound_player_name: sound_names.rechamber_player,
        alt_switch_sound_name: sound_names.alt_switch,
        alt_switch_sound_player_name: sound_names.alt_switch_player,
        raise_sound_name: sound_names.raise,
        raise_sound_player_name: sound_names.raise_player,
        first_raise_sound_name: sound_names.first_raise,
        first_raise_sound_player_name: sound_names.first_raise_player,
        putaway_sound_name: sound_names.putaway,
        putaway_sound_player_name: sound_names.putaway_player,
        proj_explosion_sound_name: sound_names.proj_explosion,
        projectile_sound_name: sound_names.projectile,
        proj_ignition_sound_name: sound_names.proj_ignition_sound,
        bounce_sound_names: sound_names.bounce,
        fire_time_ms,
        ads_zoom_fov,
        ads_dof: [
            s.f32_at(p, s.layout(0x5c, 124))?,
            s.f32_at(p, s.layout(0x60, 128))?,
        ],
        impact_type,
        raise_time_ms: body_facts.raise_time_ms,
        drop_time_ms: body_facts.drop_time_ms,
        fire_delay_ms: body_facts.fire_delay_ms,
        hold_fire_time_ms: body_facts.hold_fire_time_ms,
        weap_type: body_facts.weap_type,
        weap_class: body_facts.weap_class,
        player_anim_type: body_facts.player_anim_type,
        offhand_class: body_facts.offhand_class,
        shots_per_fire: body_facts.shots_per_fire,
        ammo_index: body_facts.ammo_index,
        clip_index: body_facts.clip_index,
        ammo_counter_clip: body_facts.ammo_counter_clip,
        low_ammo_warning_threshold: body_facts.low_ammo_warning_threshold,
        hip_spread_stand_min: body_facts.hip_spread_stand_min,
        hip_spread_ducked_min: body_facts.hip_spread_ducked_min,
        hip_spread_prone_min: body_facts.hip_spread_prone_min,
        hip_spread_stand_max: body_facts.hip_spread_stand_max,
        hip_spread_ducked_max: body_facts.hip_spread_ducked_max,
        hip_spread_prone_max: body_facts.hip_spread_prone_max,
        hip_spread_decay_rate: body_facts.hip_spread_decay_rate,
        hip_spread_fire_add: body_facts.hip_spread_fire_add,
        hip_spread_turn_add: body_facts.hip_spread_turn_add,
        hip_spread_move_add: body_facts.hip_spread_move_add,
        hip_spread_ducked_decay: body_facts.hip_spread_ducked_decay,
        hip_spread_prone_decay: body_facts.hip_spread_prone_decay,
        i_reticle_side_size: body_facts.i_reticle_side_size,
        i_reticle_min_ofs: body_facts.i_reticle_min_ofs,
        hip_reticle_side_pos: body_facts.hip_reticle_side_pos,
        ads_aim_pitch: body_facts.ads_aim_pitch,
        ads_crosshair_in_frac: body_facts.ads_crosshair_in_frac,
        ads_crosshair_out_frac: body_facts.ads_crosshair_out_frac,
        ads_spread: body_facts.ads_spread,
        aim_down_sight: body_facts.aim_down_sight,
        thermal_scope: body_facts.thermal_scope,
        no_ads_when_mag_empty: body_facts.no_ads_when_mag_empty,
        inherits_perks: body_facts.inherits_perks,
        ads_reload_trans_time_ms: body_facts.ads_reload_trans_time_ms,
        ads_in_rate: body_facts.ads_in_rate,
        ads_out_rate: body_facts.ads_out_rate,
        rechamber_while_ads: body_facts.rechamber_while_ads,
        ads_fire_only: body_facts.ads_fire_only,
        dual_wield_view_model_offset: body_facts.dual_wield_view_model_offset,
        no_dual_wield: body_facts.no_dual_wield,
        melee_damage: body_facts.melee_damage,
        overlay_reticle: body_facts.overlay_reticle,
        overlay_interface: body_facts.overlay_interface,
        ads_zoom_in_frac: body_facts.ads_zoom_in_frac,
        ads_zoom_out_frac: body_facts.ads_zoom_out_frac,
        ads_overlay_width: body_facts.ads_overlay_width,
        ads_overlay_height: body_facts.ads_overlay_height,
        melee_time_ms: body_facts.melee_time_ms,
        melee_delay_ms: body_facts.melee_delay_ms,
        melee_charge_time_ms: body_facts.melee_charge_time_ms,
        melee_charge_delay_ms: body_facts.melee_charge_delay_ms,
        knife_model: body_facts.knife_model,
        quick_raise_time_ms: body_facts.quick_raise_time_ms,
        quick_drop_time_ms: body_facts.quick_drop_time_ms,
        select_requires_ammo: body_facts.select_requires_ammo,
        offhand_hold_is_cancelable: body_facts.offhand_hold_is_cancelable,
        move_speed_scale: body_facts.move_speed_scale,
        ads_move_speed_scale: body_facts.ads_move_speed_scale,
        sprint_duration_scale: body_facts.sprint_duration_scale,
        ducked_ofs: body_facts.ducked_ofs,
        prone_ofs: body_facts.prone_ofs,
        night_vision_wear_time: body_facts.night_vision_wear_time,
        ads_bob_factor: body_facts.ads_bob_factor,
        ads_view_bob_mult: body_facts.ads_view_bob_mult,
        movement: body_facts.movement,
        idle: body_facts.idle,
        clip_size,
        penetrate_type: body_facts.penetrate_type,
        penetrate_multiplier,
        rifle_bullet: body_facts.rifle_bullet,
        ricochet_chance: body_facts.ricochet_chance,
        explosive_bullet: body_facts.explosive_bullet,
        inventory_type: body_facts.inventory_type,
        fire_type: body_facts.fire_type,
        max_ammo: body_facts.max_ammo,
        damage: body_facts.damage,
        rechamber_time_ms: body_facts.rechamber_time_ms,
        rechamber_bolt_time_ms: body_facts.rechamber_bolt_time_ms,
        rechamber_bolt_delay_ms: body_facts.rechamber_bolt_delay_ms,
        reload_time_ms: body_facts.reload_time_ms,
        reload_show_rocket_time_ms: body_facts.reload_show_rocket_time_ms,
        reload_empty_time_ms: body_facts.reload_empty_time_ms,
        reload_add_time_ms: body_facts.reload_add_time_ms,
        reload_start_time_ms: body_facts.reload_start_time_ms,
        reload_start_add_time_ms: body_facts.reload_start_add_time_ms,
        reload_end_time_ms: body_facts.reload_end_time_ms,
        kill_icon_ratio: body_facts.kill_icon_ratio,
        flip_kill_icon: body_facts.flip_kill_icon,
        reload_ammo_add: body_facts.reload_ammo_add,
        reload_start_add: body_facts.reload_start_add,
        no_partial_reload: body_facts.no_partial_reload,
        bolt_action: body_facts.bolt_action,
        segmented_reload: body_facts.segmented_reload,
        sprint_raise_time_ms: body_facts.sprint_raise_time_ms,
        sprint_loop_time_ms: body_facts.sprint_loop_time_ms,
        sprint_drop_time_ms: body_facts.sprint_drop_time_ms,
        stunned_start_time_ms: body_facts.stunned_start_time_ms,
        stunned_end_time_ms: body_facts.stunned_end_time_ms,
        fuse_time_ms: body_facts.fuse_time_ms,
        auto_aim_range: body_facts.auto_aim_range,
        aim_assist_range: body_facts.aim_assist_range,
        aim_assist_range_ads: body_facts.aim_assist_range_ads,
        cook_off_hold: body_facts.cook_off_hold,
        clip_only: body_facts.clip_only,
        has_detonator: body_facts.has_detonator,
        detonate_delay_ms: body_facts.detonate_delay_ms,
        detonate_time_ms: body_facts.detonate_time_ms,
        projectile_rotates: body_facts.projectile_rotates,
        timed_detonation: body_facts.timed_detonation,
        proj_impact_explode: body_facts.proj_impact_explode,
        stick_to_players: body_facts.stick_to_players,
        explosion_radius: body_facts.explosion_radius,
        explosion_radius_min: body_facts.explosion_radius_min,
        explosion_inner_damage: body_facts.explosion_inner_damage,
        explosion_outer_damage: body_facts.explosion_outer_damage,
        damage_cone_angle: body_facts.damage_cone_angle,
        missile_guidance: body_facts.missile_guidance,
        ignition_delay_ms: body_facts.ignition_delay_ms,
        require_lock_to_fire: body_facts.require_lock_to_fire,
        stickiness: body_facts.stickiness,
        projectile_speed: body_facts.projectile_speed,
        projectile_speed_up: body_facts.projectile_speed_up,
        projectile_speed_forward: body_facts.projectile_speed_forward,
        projectile_activate_dist: body_facts.projectile_activate_dist,
        projectile_explosion_type: body_facts.projectile_explosion_type,
        parallel_bounce: body_facts.parallel_bounce,
        perpendicular_bounce: body_facts.perpendicular_bounce,
        location_damage_mult: body_facts.location_damage_mult,
        start_ammo: body_facts.start_ammo,
        min_damage: body_facts.min_damage,
        min_player_damage: body_facts.min_player_damage,
        max_damage_range: body_facts.max_damage_range,
        min_damage_range: body_facts.min_damage_range,
        kick: body_facts.kick,
        sway: body_facts.sway,
    });

    s.pop()
}

#[derive(Clone, Copy, Default)]
struct WeapDefScalars {
    raise_time_ms: i32,
    drop_time_ms: i32,
    fire_delay_ms: i32,

    hold_fire_time_ms: i32,
    weap_type: i32,
    weap_class: i32,

    player_anim_type: i32,
    offhand_class: i32,
    shots_per_fire: i32,
    ammo_index: i32,
    clip_index: i32,

    ammo_counter_clip: i32,

    low_ammo_warning_threshold: f32,
    hip_spread_stand_min: f32,
    hip_spread_ducked_min: f32,
    hip_spread_prone_min: f32,
    hip_spread_stand_max: f32,
    hip_spread_ducked_max: f32,
    hip_spread_prone_max: f32,
    hip_spread_decay_rate: f32,
    hip_spread_fire_add: f32,
    hip_spread_turn_add: f32,
    hip_spread_move_add: f32,
    hip_spread_ducked_decay: f32,
    hip_spread_prone_decay: f32,
    reticle_center_size: i32,
    i_reticle_side_size: i32,
    i_reticle_min_ofs: i32,
    hip_reticle_side_pos: f32,
    ads_aim_pitch: f32,
    ads_crosshair_in_frac: f32,
    ads_crosshair_out_frac: f32,
    ads_spread: f32,
    aim_down_sight: bool,
    thermal_scope: bool,
    no_ads_when_mag_empty: bool,
    inherits_perks: bool,
    ads_reload_trans_time_ms: i32,
    ads_in_rate: f32,
    ads_out_rate: f32,
    rechamber_while_ads: bool,
    ads_fire_only: bool,
    dual_wield_view_model_offset: f32,
    no_dual_wield: bool,
    melee_damage: i32,
    overlay_reticle: i32,
    overlay_interface: i32,
    ads_zoom_in_frac: f32,
    ads_zoom_out_frac: f32,
    ads_overlay_width: f32,
    ads_overlay_height: f32,
    melee_time_ms: i32,
    melee_delay_ms: i32,
    melee_charge_time_ms: i32,
    melee_charge_delay_ms: i32,
    knife_model: u32,
    quick_raise_time_ms: i32,
    quick_drop_time_ms: i32,
    select_requires_ammo: Option<bool>,
    offhand_hold_is_cancelable: Option<bool>,
    move_speed_scale: f32,
    ads_move_speed_scale: f32,
    sprint_duration_scale: f32,
    ducked_ofs: [f32; 3],
    prone_ofs: [f32; 3],
    night_vision_wear_time: i32,
    ads_bob_factor: f32,
    ads_view_bob_mult: f32,
    movement: WeaponMovementOfsCapture,
    idle: WeaponIdleCapture,
    penetrate_type: i32,
    rifle_bullet: bool,
    ricochet_chance: f32,
    explosive_bullet: bool,
    inventory_type: i32,
    fire_type: i32,
    max_ammo: i32,
    damage: i32,
    rechamber_time_ms: i32,
    rechamber_bolt_time_ms: i32,
    rechamber_bolt_delay_ms: i32,
    reload_time_ms: i32,

    reload_show_rocket_time_ms: i32,
    reload_empty_time_ms: i32,
    reload_add_time_ms: i32,
    reload_start_time_ms: i32,
    reload_start_add_time_ms: i32,
    reload_end_time_ms: i32,

    kill_icon_ratio: i32,

    flip_kill_icon: bool,
    reload_ammo_add: i32,
    reload_start_add: i32,
    no_partial_reload: bool,

    bolt_action: bool,

    segmented_reload: bool,
    sprint_raise_time_ms: i32,
    sprint_loop_time_ms: i32,
    sprint_drop_time_ms: i32,
    stunned_start_time_ms: i32,
    stunned_end_time_ms: i32,
    fuse_time_ms: i32,
    auto_aim_range: f32,
    aim_assist_range: f32,
    aim_assist_range_ads: f32,

    cook_off_hold: bool,

    clip_only: bool,

    has_detonator: bool,
    detonate_delay_ms: i32,
    detonate_time_ms: i32,
    projectile_rotates: bool,
    timed_detonation: bool,

    proj_impact_explode: bool,

    stick_to_players: bool,
    explosion_radius: i32,
    explosion_radius_min: i32,
    explosion_inner_damage: i32,
    explosion_outer_damage: i32,
    damage_cone_angle: f32,
    missile_guidance: i32,
    ignition_delay_ms: i32,
    require_lock_to_fire: bool,
    stickiness: i32,
    projectile_speed: i32,
    projectile_speed_up: i32,
    projectile_speed_forward: i32,
    projectile_activate_dist: i32,
    projectile_explosion_type: i32,
    parallel_bounce: Option<[f32; 31]>,
    perpendicular_bounce: Option<[f32; 31]>,
    location_damage_mult: Option<[f32; 20]>,
    start_ammo: i32,
    min_damage: i32,
    min_player_damage: i32,
    max_damage_range: f32,
    min_damage_range: f32,
    kick: WeaponKickCapture,
    sway: WeaponSwayCapture,
}

#[derive(Clone, Copy, Debug, Default)]
struct WeaponSoundNames {
    projectile: Option<Ptr>,
    proj_ignition_sound: Option<Ptr>,
    fire: Option<Ptr>,
    fire_player: Option<Ptr>,
    fire_last: Option<Ptr>,
    fire_last_player: Option<Ptr>,
    empty_fire: Option<Ptr>,
    empty_fire_player: Option<Ptr>,
    melee_swipe: Option<Ptr>,
    melee_swipe_player: Option<Ptr>,
    melee_hit: Option<Ptr>,
    melee_miss: Option<Ptr>,
    pickup: Option<Ptr>,
    pickup_player: Option<Ptr>,
    ammo_pickup: Option<Ptr>,
    ammo_pickup_player: Option<Ptr>,
    pullback: Option<Ptr>,
    pullback_player: Option<Ptr>,
    reload: Option<Ptr>,
    reload_player: Option<Ptr>,
    reload_empty: Option<Ptr>,
    reload_empty_player: Option<Ptr>,
    reload_start: Option<Ptr>,
    reload_start_player: Option<Ptr>,
    reload_end: Option<Ptr>,
    reload_end_player: Option<Ptr>,
    rechamber: Option<Ptr>,
    rechamber_player: Option<Ptr>,
    alt_switch: Option<Ptr>,
    alt_switch_player: Option<Ptr>,
    raise: Option<Ptr>,
    raise_player: Option<Ptr>,
    first_raise: Option<Ptr>,
    first_raise_player: Option<Ptr>,
    putaway: Option<Ptr>,
    putaway_player: Option<Ptr>,
    proj_explosion: Option<Ptr>,
    bounce: [Option<Ptr>; sz::SURF_TYPE_NUM],
    notetrack_sound_keys: Option<Ptr>,
    notetrack_sound_values: Option<Ptr>,
    notetrack_rumble_keys: Option<Ptr>,
    notetrack_rumble_values: Option<Ptr>,
}

fn f32x3_at(s: &ZoneStream<'_>, body: Ptr, off: usize) -> Result<[f32; 3]> {
    Ok([
        s.f32_at(body, off)?,
        s.f32_at(body, off + 4)?,
        s.f32_at(body, off + 8)?,
    ])
}

fn load_movement_ofs(s: &ZoneStream<'_>, body: Ptr) -> Result<WeaponMovementOfsCapture> {
    Ok(WeaponMovementOfsCapture {
        stand_move: f32x3_at(s, body, s.layout(0x138, 576))?,
        stand_rot: f32x3_at(s, body, s.layout(0x144, 588))?,
        strafe_move: f32x3_at(s, body, s.layout(0x150, 600))?,
        strafe_rot: f32x3_at(s, body, s.layout(0x15c, 612))?,
        ducked_move: f32x3_at(s, body, s.layout(0x174, 636))?,
        ducked_rot: f32x3_at(s, body, s.layout(0x180, 648))?,
        prone_move: f32x3_at(s, body, s.layout(0x198, 672))?,
        prone_rot: f32x3_at(s, body, s.layout(0x1a4, 684))?,
        pos_move_rate: s.f32_at(body, s.layout(0x1b0, 696))?,
        pos_prone_move_rate: s.f32_at(body, s.layout(0x1b4, 700))?,
        stand_move_min_speed: s.f32_at(body, s.layout(0x1b8, 704))?,
        ducked_move_min_speed: s.f32_at(body, s.layout(0x1bc, 708))?,
        prone_move_min_speed: s.f32_at(body, s.layout(0x1c0, 712))?,
        pos_rot_rate: s.f32_at(body, s.layout(0x1c4, 716))?,
        pos_prone_rot_rate: s.f32_at(body, s.layout(0x1c8, 720))?,
    })
}

fn load_idle(s: &ZoneStream<'_>, body: Ptr) -> Result<WeaponIdleCapture> {
    Ok(WeaponIdleCapture {
        ads_idle_amount: s.f32_at(body, s.layout(0x36c, 1220))?,
        hip_idle_amount: s.f32_at(body, s.layout(0x370, 1224))?,
        ads_idle_speed: s.f32_at(body, s.layout(0x374, 1228))?,
        hip_idle_speed: s.f32_at(body, s.layout(0x378, 1232))?,
        idle_crouch_factor: s.f32_at(body, s.layout(0x37c, 1236))?,
        idle_prone_factor: s.f32_at(body, s.layout(0x380, 1240))?,
    })
}

fn load_weapon_def(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    ai_vs_ai_knots: usize,
    ai_vs_player_knots: usize,
) -> Result<(
    Ptr,
    [Option<Ptr>; 16],
    Option<Ptr>,
    [Option<Ptr>; 16],
    Option<Ptr>,
    Option<Ptr>,
    WeaponSoundNames,
    Option<Ptr>,
    Option<Ptr>,
)> {
    let p = s.alloc_load(4, s.layout(sz::WEAPON_DEF, 2192))?;

    follow_name(s, p, 0)?;
    let gun0 = follow_xmodel_array(s, links, p, s.layout(4, 8))?;
    let hand0 = follow_xmodel_ptr(s, links, p.at(s.layout(8, 16)))?;
    let sz_xanims_right = follow_string_array(s, p, s.layout(12, 24), sz::WEAPON_ANIM_COUNT)?;
    let sz_xanims_left = follow_string_array(s, p, s.layout(16, 32), sz::WEAPON_ANIM_COUNT)?;
    follow_name(s, p, s.layout(20, 40))?;

    let notetrack_sound_keys = s.plain_array(p, s.layout(24, 48), 2, 2, 16)?;
    let notetrack_sound_values = s.plain_array(p, s.layout(28, 56), 2, 2, 16)?;
    let notetrack_rumble_keys = s.plain_array(p, s.layout(32, 64), 2, 2, 16)?;
    let notetrack_rumble_values = s.plain_array(p, s.layout(36, 72), 2, 2, 16)?;

    asset_ptr_at(s, links, AssetType::Fx, p.at(s.layout(0x48, 112)))?;
    asset_ptr_at(s, links, AssetType::Fx, p.at(s.layout(0x4c, 120)))?;

    let mut sound_names = WeaponSoundNames::default();
    for field in (80..=264).step_by(4) {
        let name = follow_snd_alias_custom(s, p.at(s.layout(field, 128 + (field - 80) * 2)))?;
        match field {
            0x50 => sound_names.pickup = name,
            0x54 => sound_names.pickup_player = name,
            0x58 => sound_names.ammo_pickup = name,
            0x5c => sound_names.ammo_pickup_player = name,
            0x60 => sound_names.projectile = name,
            0x64 => sound_names.pullback = name,
            0x68 => sound_names.pullback_player = name,
            0x6c => sound_names.fire = name,
            0x70 => sound_names.fire_player = name,
            0x88 => sound_names.fire_last = name,
            0x8c => sound_names.fire_last_player = name,
            0x90 => sound_names.empty_fire = name,
            0x94 => sound_names.empty_fire_player = name,
            0x98 => sound_names.melee_swipe = name,
            0x9c => sound_names.melee_swipe_player = name,
            0xa0 => sound_names.melee_hit = name,
            0xa4 => sound_names.melee_miss = name,
            0xa8 => sound_names.rechamber = name,
            0xac => sound_names.rechamber_player = name,
            0xb0 => sound_names.reload = name,
            0xb4 => sound_names.reload_player = name,
            0xb8 => sound_names.reload_empty = name,
            0xbc => sound_names.reload_empty_player = name,
            0xc0 => sound_names.reload_start = name,
            0xc4 => sound_names.reload_start_player = name,
            0xc8 => sound_names.reload_end = name,
            0xcc => sound_names.reload_end_player = name,
            0xe8 => sound_names.alt_switch = name,
            0xec => sound_names.alt_switch_player = name,
            0xf0 => sound_names.raise = name,
            0xf4 => sound_names.raise_player = name,
            0xf8 => sound_names.first_raise = name,
            0xfc => sound_names.first_raise_player = name,
            0x100 => sound_names.putaway = name,
            0x104 => sound_names.putaway_player = name,
            _ => {}
        }
    }
    sound_names.notetrack_sound_keys = notetrack_sound_keys;
    sound_names.notetrack_sound_values = notetrack_sound_values;
    sound_names.notetrack_rumble_keys = notetrack_rumble_keys;
    sound_names.notetrack_rumble_values = notetrack_rumble_values;
    if s.begin_body(p.at(s.layout(268, 504)))? {
        let arr = s.alloc_load(4, s.layout(sz::SND_ALIAS_CUSTOM, 8) * sz::SURF_TYPE_NUM)?;
        for i in 0..sz::SURF_TYPE_NUM {
            sound_names.bounce[i] =
                follow_snd_alias_custom(s, arr.at(i * s.layout(sz::SND_ALIAS_CUSTOM, 8)))?;
        }
    }

    for (field_x86, field_x64) in [(272, 512), (276, 520), (280, 528), (284, 536)] {
        let field = s.layout(field_x86, field_x64);
        asset_ptr_at(s, links, AssetType::Fx, p.at(field))?;
    }
    for (field_x86, field_x64) in [(288, 544), (292, 552)] {
        let field = s.layout(field_x86, field_x64);
        asset_ptr_at(s, links, AssetType::Material, p.at(field))?;
    }

    let world0 = follow_xmodel_array(s, links, p, s.layout(472, 736))?;
    asset_ptr_at(s, links, AssetType::XModel, p.at(s.layout(476, 744)))?;
    let rocket0 = follow_xmodel_ptr(s, links, p.at(s.layout(480, 752)))?;
    asset_ptr_at(s, links, AssetType::XModel, p.at(s.layout(484, 760)))?;
    asset_ptr_at(s, links, AssetType::XModel, p.at(s.layout(488, 768)))?;
    asset_ptr_at(s, links, AssetType::Material, p.at(s.layout(492, 776)))?;
    asset_ptr_at(s, links, AssetType::Material, p.at(s.layout(500, 792)))?;
    asset_ptr_at(s, links, AssetType::Material, p.at(s.layout(508, 808)))?;
    for (field_x86, field_x64) in [(524, 832), (532, 848), (548, 872)] {
        let field = s.layout(field_x86, field_x64);
        follow_name(s, p, field)?;
    }
    for (field_x86, field_x64) in [(0x308, 1104), (0x30c, 1112), (0x310, 1120), (0x314, 1128)] {
        let field = s.layout(field_x86, field_x64);
        asset_ptr_at(s, links, AssetType::Material, p.at(field))?;
    }

    asset_ptr_at(s, links, AssetType::PhysCollmap, p.at(s.layout(968, 1312)))?;
    let projectile0 = follow_xmodel_ptr(s, links, p.at(s.layout(0x420, 1408)))?;
    asset_ptr_at(s, links, AssetType::Fx, p.at(s.layout(0x428, 1424)))?;
    asset_ptr_at(s, links, AssetType::Fx, p.at(s.layout(0x42c, 1432)))?;
    sound_names.proj_explosion = follow_snd_alias_custom(s, p.at(s.layout(0x430, 1440)))?;
    follow_snd_alias_custom(s, p.at(s.layout(0x434, 1448)))?;
    s.plain_array(p, s.layout(1092, 1472), 4, 4, sz::SURF_TYPE_NUM)?;
    s.plain_array(p, s.layout(1096, 1480), 4, 4, sz::SURF_TYPE_NUM)?;
    for (field_x86, field_x64) in [(0x44c, 1488), (0x450, 1496), (0x46c, 1528)] {
        let field = s.layout(field_x86, field_x64);
        asset_ptr_at(s, links, AssetType::Fx, p.at(field))?;
    }
    sound_names.proj_ignition_sound = follow_snd_alias_custom(s, p.at(s.layout(1136, 1536)))?;

    follow_name(s, p, s.layout(1292, 1696))?;
    s.plain_array(p, s.layout(1300, 1712), 4, 8, ai_vs_ai_knots)?;
    follow_name(s, p, s.layout(1296, 1704))?;
    s.plain_array(p, s.layout(1304, 1720), 4, 8, ai_vs_player_knots)?;

    for (field_x86, field_x64) in [(1384, 1808), (1388, 1816), (1420, 1856)] {
        let field = s.layout(field_x86, field_x64);
        follow_name(s, p, field)?;
    }
    s.plain_array(p, s.layout(1460, 1904), 4, 4, 20)?;
    for (field_x86, field_x64) in [(1464, 1912), (1468, 1920)] {
        let field = s.layout(field_x86, field_x64);
        follow_name(s, p, field)?;
    }

    asset_ptr_at(s, links, AssetType::Tracer, p.at(s.layout(0x5c0, 1928)))?;
    follow_snd_alias_custom(s, p.at(s.layout(1500, 1960)))?;
    asset_ptr_at(s, links, AssetType::Fx, p.at(s.layout(1504, 1968)))?;
    follow_name(s, p, s.layout(1508, 1976))?;
    follow_snd_alias_custom(s, p.at(s.layout(1524, 2000)))?;
    for i in 0..4 {
        follow_snd_alias_custom(s, p.at(s.layout(1528, 2008) + i * s.pointer_bytes()))?;
    }
    for i in 0..4 {
        follow_snd_alias_custom(s, p.at(s.layout(1544, 2040) + i * s.pointer_bytes()))?;
    }
    follow_snd_alias_custom(s, p.at(s.layout(1560, 2072)))?;
    follow_snd_alias_custom(s, p.at(s.layout(1564, 2080)))?;

    Ok((
        p,
        gun0,
        hand0,
        world0,
        projectile0,
        rocket0,
        sound_names,
        sz_xanims_right,
        sz_xanims_left,
    ))
}

pub(super) fn load_tracer(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::TRACER_DEF, 120))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let material_fresh = asset_ptr_at_linked(s, links, AssetType::Material, p.at(s.layout(4, 8)))?;
    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    let material_slot = match s.ptr_at(p, s.layout(4, 8))? {
        ZonePtr::Null => None,
        _ => Some(p.at(s.layout(4, 8))),
    };
    let material_name = if material_fresh {
        s.latest_material().and_then(|g| g.name)
    } else {
        None
    };
    let mut colors = [[0f32; 4]; 5];
    for (i, row) in colors.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = s.f32_at(p, s.layout(0x20, 40) + i * 16 + c * 4)?;
        }
    }
    links.capture_tracer(
        s,
        crate::TracerDefGeometry {
            header: Some(p),
            name,
            material_slot,
            material_name,
            material_fresh,
            draw_interval: s.u32_at(p, s.layout(8, 16))?,
            speed: s.f32_at(p, s.layout(0xc, 20))?,
            beam_length: s.f32_at(p, s.layout(0x10, 24))?,
            beam_width: s.f32_at(p, s.layout(0x14, 28))?,
            screw_radius: s.f32_at(p, s.layout(0x18, 32))?,
            screw_dist: s.f32_at(p, s.layout(0x1c, 36))?,
            colors,
        },
    )?;
    s.pop()
}

fn follow_string_array(
    s: &mut ZoneStream<'_>,
    p: Ptr,
    field: usize,
    count: usize,
) -> Result<Option<Ptr>> {
    match s.ptr_at(p, field)? {
        ZonePtr::Null => Ok(None),
        ZonePtr::Offset(q) => {
            s.note_offset(q);
            Ok(Some(s.resolve_alias(q)))
        }
        _ => {
            if !s.begin_body(p.at(field))? {
                return Ok(None);
            }
            let arr = s.alloc_load(4, s.pointer_bytes() * count)?;
            for i in 0..count {
                s.follow_string(arr, i * s.pointer_bytes())?;
            }
            Ok(Some(arr))
        }
    }
}

fn follow_xmodel_array(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    p: Ptr,
    field: usize,
) -> Result<[Option<Ptr>; 16]> {
    let mut names = [None; 16];
    match s.ptr_at(p, field)? {
        ZonePtr::Null => {}
        ZonePtr::Offset(q) => {
            s.note_offset(q);
            let arr = s.resolve_alias(q);
            for (i, name) in names.iter_mut().enumerate() {
                let slot = arr.at(i * s.pointer_bytes());
                if i == 0
                    && let Some(found) = links.xmodel_name_ptr(arr)
                {
                    *name = Some(found);
                    continue;
                }
                *name = match s.ptr_at(slot, 0)? {
                    ZonePtr::Offset(m) => {
                        let body = s.resolve_alias(m);
                        links
                            .xmodel_name_ptr(body)
                            .or_else(|| links.xmodel_name_ptr(m))
                    }
                    _ => None,
                }
                .or_else(|| links.xmodel_name_ptr(slot));
            }
        }
        _ => {
            if !s.begin_body(p.at(field))? {
                return Ok(names);
            }
            let arr = s.alloc_load(4, s.pointer_bytes() * 16)?;
            for (i, name) in names.iter_mut().enumerate() {
                let slot = arr.at(i * s.pointer_bytes());
                let loaded = load_asset_at_observed(s, AssetType::XModel, slot, links)?;
                *name = if loaded {
                    s.xmodel().and_then(|g| g.name)
                } else {
                    match s.ptr_at(slot, 0)? {
                        ZonePtr::Offset(q) => {
                            let body = s.resolve_alias(q);
                            links
                                .xmodel_name_ptr(body)
                                .or_else(|| links.xmodel_name_ptr(q))
                                .or_else(|| links.xmodel_name_ptr(slot))
                        }
                        _ => None,
                    }
                };
            }
        }
    }
    Ok(names)
}

fn follow_xmodel_ptr(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    slot: Ptr,
) -> Result<Option<Ptr>> {
    let loaded = load_asset_at_observed(s, AssetType::XModel, slot, links)?;
    if loaded {
        if let Some(name) = s.xmodel().and_then(|g| g.name) {
            return Ok(Some(name));
        }
    }
    Ok(links
        .xmodel_name_ptr(slot)
        .or_else(|| match s.ptr_at(slot, 0).ok()? {
            ZonePtr::Offset(q) => {
                let body = s.resolve_alias(q);
                links
                    .xmodel_name_ptr(body)
                    .or_else(|| links.xmodel_name_ptr(q))
            }
            _ => None,
        }))
}

fn follow_snd_alias_custom(s: &mut ZoneStream<'_>, slot: Ptr) -> Result<Option<Ptr>> {
    match s.ptr_at(slot, 0)? {
        ZonePtr::Null => Ok(None),
        ZonePtr::Offset(q) => {
            s.note_offset(q);
            let wrapper = s.resolve_alias(q);
            Ok(match s.ptr_at(wrapper, 0)? {
                ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
                _ => None,
            })
        }
        _ => {
            if !s.begin_body(slot)? {
                return Ok(None);
            }
            let n = s.alloc_load(4, s.layout(sz::SND_ALIAS_CUSTOM, 8))?;
            follow_name(s, n, 0)?;
            Ok(match s.ptr_at(n, 0)? {
                ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
                _ => None,
            })
        }
    }
}

fn read_f32_array<const N: usize>(
    s: &ZoneStream<'_>,
    body: Ptr,
    field: usize,
) -> Result<Option<[f32; N]>> {
    let arr = match s.ptr_at(body, field)? {
        ZonePtr::Null => return Ok(None),
        ZonePtr::Offset(p) => s.resolve_alias(p),
        _ => return Err(crate::ZoneError::UnresolvedPointer(body.at(field))),
    };
    let mut values = [0.0; N];
    for (i, value) in values.iter_mut().enumerate() {
        *value = s.f32_at(arr, i * 4)?;
    }
    Ok(Some(values))
}
