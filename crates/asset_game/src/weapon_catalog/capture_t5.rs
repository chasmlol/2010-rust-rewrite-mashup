use super::*;

impl WeaponCatalog {
    pub fn capture_t5(
        &mut self,
        stream: &fastfile_t5::ZoneStream<'_>,
        strings: &fastfile_t5::ScriptStrings,
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
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let sz_xanims = geometry
            .sz_xanims
            .map(|arr| read_sz_xanims_t5(stream, arr, false))
            .unwrap_or([const { None }; WEAPON_ANIM_SLOTS]);
        self.entries.push(CatalogWeapon {
            namespace: self
                .capture_ns
                .expect("asset capture requires an explicit family"),
            impact_payload: geometry.weap_def.and_then(|body| {
                leftover_t5_cstr(
                    stream,
                    body,
                    fastfile_t5::size::WEAPON_DEF_IMPACT_PAYLOAD_OFF,
                )
            }),
            alternate_weapon: geometry
                .alternate_weapon_name
                .and_then(|p| stream.cstr(p).ok())
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            reticle_center_slot: leftover_t5_asset_slot(
                stream,
                geometry.weap_def,
                fastfile_t5::size::WEAPON_DEF_RETICLE_CENTER_OFF,
            ),
            reticle_side_slot: leftover_t5_asset_slot(
                stream,
                geometry.weap_def,
                fastfile_t5::size::WEAPON_DEF_RETICLE_SIDE_OFF,
            ),
            name: name.to_owned(),
            weap_def: geometry.weap_def.map(|p| (p.block, p.offset)),
            display_name_key: geometry
                .display_name
                .and_then(|ptr| stream.cstr(ptr).ok())
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            reticle: {
                let mut reticle = leftover_t5_reticle(stream, geometry.weap_def);
                reticle.center_material = geometry
                    .reticle_center_name
                    .and_then(|p| stream.cstr(p).ok())
                    .map(str::to_owned)
                    .or(reticle.center_material);
                reticle.side_material = geometry
                    .reticle_side_name
                    .and_then(|p| stream.cstr(p).ok())
                    .map(str::to_owned)
                    .or(reticle.side_material);
                reticle
            },
            hud_material_edges: WeaponHudMaterialEdges::default(),
            overlay_material: leftover_t5_overlay_name(stream, &geometry),
            overlay_image: None,
            overlay_material_slot: leftover_t5_overlay_slot(stream, &geometry),
            iw5_attachment_slots: std::array::from_fn(|_| None),
            attached_models: Default::default(),
            t6_clip_models: Default::default(),
            t6_attachments: Vec::new(),
            t6_attachment_stats: Vec::new(),
            iw5_reload_overrides: Vec::new(),
            iw5_anim_overrides: Vec::new(),
            iw5_fx_overrides: Vec::new(),
            iw5_notetrack_overrides: Vec::new(),
            hud_icon: leftover_t5_material_name_opt(
                stream,
                geometry.weap_def,
                fastfile_t5::size::WEAPON_DEF_HUD_ICON_OFF,
            ),
            hud_icon_slot: leftover_t5_asset_slot(
                stream,
                geometry.weap_def,
                fastfile_t5::size::WEAPON_DEF_HUD_ICON_OFF,
            ),

            pickup_icon: None,
            pickup_icon_slot: None,
            pickup_icon_image: None,
            pickup_icon_ratio: 0,
            hud_icon_ratio: geometry
                .weap_def
                .map_or(0, |body| i32_at_t5(stream, body, 0x324)),
            hud_icon_image: None,
            dpad_icon: None,
            dpad_icon_image: None,
            dpad_icon_atlas: None,
            dpad_icon_ratio: 0,
            kill_icon: leftover_t5_material_name_opt(
                stream,
                geometry.weap_def,
                fastfile_t5::size::WEAPON_DEF_KILL_ICON_OFF,
            ),
            kill_icon_slot: leftover_t5_asset_slot(
                stream,
                geometry.weap_def,
                fastfile_t5::size::WEAPON_DEF_KILL_ICON_OFF,
            ),
            kill_icon_image: None,
            proj_trail: None,
            proj_trail_slot: leftover_t5_asset_slot(
                stream,
                geometry.weap_def,
                fastfile_t5::size::WEAPON_DEF_PROJ_TRAIL_EFFECT_OFF,
            ),
            proj_beacon: None,
            proj_beacon_slot: None,
            proj_ignition: None,
            proj_ignition_slot: leftover_t5_asset_slot(
                stream,
                geometry.weap_def,
                fastfile_t5::size::WEAPON_DEF_PROJ_IGNITION_EFFECT_OFF,
            ),
            projectile_fx: WeaponProjectileFx::default(),
            gun_xmodel,
            hand_xmodel,

            camo_models: WeaponCamoModels::default(),
            skin_parent: geometry.weap_def.and_then(|body| {
                leftover_t5_cstr(
                    stream,
                    body,
                    fastfile_t5::size::WEAPON_DEF_PARENT_WEAPON_NAME_OFF,
                )
            }),
            world_model: geometry
                .world_model_name
                .and_then(|ptr| stream.cstr(ptr).ok())
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            projectile_model: geometry
                .projectile_model_name
                .and_then(|ptr| stream.cstr(ptr).ok())
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            knife_xmodel: None,
            rocket_model: geometry
                .rocket_model_name
                .and_then(|ptr| stream.cstr(ptr).ok())
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            sz_xanims,
            dual_wield_weapon: geometry.weap_def.and_then(|body| {
                leftover_t5_cstr(
                    stream,
                    body,
                    fastfile_t5::size::WEAPON_DEF_DUAL_WIELD_WEAPON_NAME_OFF,
                )
            }),
            sz_xanims_right: [const { None }; WEAPON_ANIM_SLOTS],
            sz_xanims_left: geometry
                .sz_xanims
                .map(|arr| read_sz_xanims_t5(stream, arr, true))
                .unwrap_or([const { None }; WEAPON_ANIM_SLOTS]),
            hide_tags: read_hide_tags_t5(stream, strings, geometry.hide_tags),
            sounds: leftover_t5_sounds(stream, strings, &geometry),
            combat_fx: WeaponCombatFx::empty(crate::AssetNamespace::T5),
            combat_slots: CombatFxSlots::default(),
            facts: capture_t5_body_facts(stream, &geometry),
        });
        let last = self.entries.last_mut().expect("just pushed");
        let (fx, slots) = leftover_t5_combat_fx(stream, &geometry);
        last.combat_fx = fx;
        last.combat_slots = slots;
        if normalize_weapon_name(&last.name) == "hatchet" {
            last.facts.weap_class = 9;
            last.facts.stick_to_players = true;
        }
    }
}

pub fn t5_inline_note_alias<'a>(note: &'a str, prefix: &str) -> Option<&'a str> {
    let (head, tail) = note.split_at_checked(prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then_some(tail)
        .filter(|alias| !alias.is_empty())
}

pub(super) fn remap_t5_weap_class(raw: i32) -> i32 {
    match raw {
        0 => 0,
        1 => 2,
        2 => 3,
        3 => 4,
        4 => 5,
        5 => 6,
        6 => 7,
        7 => 8,
        8 => 10,
        10 => 11,
        other => other,
    }
}

pub(super) fn read_sz_xanims_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    arr: fastfile_t5::Ptr,
    left: bool,
) -> [Option<String>; WEAPON_ANIM_SLOTS] {
    let mut t5 = [const { None }; fastfile_t5::size::WEAPON_XANIM_COUNT];
    for (i, slot) in t5.iter_mut().enumerate() {
        let name_ptr = match stream.ptr_at(arr, i * 4) {
            Ok(fastfile_t5::ZonePtr::Offset(q)) => Some(stream.resolve_alias(q)),
            _ => None,
        };
        *slot = name_ptr
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
    }
    let mut out = remap_t5_sz_xanims(&t5);
    if left {
        use fastfile_t5::size::weap_anim as a;
        for (src, dst) in [
            (a::IDLE_LEFT, weap_anim::IDLE),
            (a::EMPTY_IDLE_LEFT, weap_anim::EMPTY_IDLE),
            (a::FIRE_LEFT, weap_anim::FIRE),
            (a::LASTSHOT_LEFT, weap_anim::LASTSHOT),
            (a::RELOAD_LEFT, weap_anim::RELOAD),
            (a::RELOAD_EMPTY_LEFT, weap_anim::RELOAD_EMPTY),
        ] {
            out[dst] = t5[src].clone();
        }
    }
    out
}

pub(super) fn remap_t5_sz_xanims(t5: &[Option<String>]) -> [Option<String>; WEAPON_ANIM_SLOTS] {
    use fastfile_t5::size::weap_anim as t5_anim;
    const PAIRS: [(usize, usize); 34] = [
        (t5_anim::IDLE, weap_anim::IDLE),
        (t5_anim::EMPTY_IDLE, weap_anim::EMPTY_IDLE),
        (t5_anim::FIRE, weap_anim::FIRE),
        (t5_anim::HOLD_FIRE, weap_anim::HOLD_FIRE),
        (t5_anim::LASTSHOT, weap_anim::LASTSHOT),
        (t5_anim::RECHAMBER, weap_anim::RECHAMBER),
        (t5_anim::MELEE, weap_anim::MELEE),
        (t5_anim::MELEE_CHARGE, weap_anim::MELEE_CHARGE),
        (t5_anim::RELOAD, weap_anim::RELOAD),
        (t5_anim::RELOAD_EMPTY, weap_anim::RELOAD_EMPTY),
        (t5_anim::RELOAD_START, weap_anim::RELOAD_START),
        (t5_anim::RELOAD_END, weap_anim::RELOAD_END),
        (t5_anim::RELOAD_QUICK, weap_anim_extra::RELOAD_QUICK),
        (
            t5_anim::RELOAD_QUICK_EMPTY,
            weap_anim_extra::RELOAD_QUICK_EMPTY,
        ),
        (t5_anim::RAISE, weap_anim::RAISE),
        (t5_anim::FIRST_RAISE, weap_anim::FIRST_RAISE),
        (t5_anim::DROP, weap_anim::DROP),
        (t5_anim::ALT_RAISE, weap_anim::ALT_RAISE),
        (t5_anim::ALT_DROP, weap_anim::ALT_DROP),
        (t5_anim::QUICK_RAISE, weap_anim::QUICK_RAISE),
        (t5_anim::QUICK_DROP, weap_anim::QUICK_DROP),
        (t5_anim::EMPTY_RAISE, weap_anim::EMPTY_RAISE),
        (t5_anim::EMPTY_DROP, weap_anim::EMPTY_DROP),
        (t5_anim::SPRINT_IN, weap_anim::SPRINT_IN),
        (t5_anim::SPRINT_LOOP, weap_anim::SPRINT_LOOP),
        (t5_anim::SPRINT_OUT, weap_anim::SPRINT_OUT),
        (t5_anim::DETONATE, weap_anim::DETONATE),
        (t5_anim::NIGHTVISION_WEAR, weap_anim::NIGHTVISION_WEAR),
        (t5_anim::NIGHTVISION_REMOVE, weap_anim::NIGHTVISION_REMOVE),
        (t5_anim::ADS_FIRE, weap_anim::ADS_FIRE),
        (t5_anim::ADS_LASTSHOT, weap_anim::ADS_LASTSHOT),
        (t5_anim::ADS_RECHAMBER, weap_anim::ADS_RECHAMBER),
        (t5_anim::ADS_UP, weap_anim::ADS_UP),
        (t5_anim::ADS_DOWN, weap_anim::ADS_DOWN),
    ];
    let mut out = [const { None }; WEAPON_ANIM_SLOTS];
    for (src, dst) in PAIRS {
        if src < t5.len() {
            out[dst] = t5[src].clone();
        }
    }
    out
}

pub(super) fn t5_to_iw4_ptr(p: fastfile_t5::Ptr) -> Ptr {
    Ptr {
        block: p.block,
        offset: p.offset,
    }
}

pub(super) fn i32_at_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    body: fastfile_t5::Ptr,
    off: usize,
) -> i32 {
    stream.i32_at(body, off).unwrap_or(0)
}

pub(super) fn u8_at_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    body: fastfile_t5::Ptr,
    off: usize,
) -> u8 {
    stream.u8_at(body, off).unwrap_or(0)
}

pub(super) fn f32_at_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    body: fastfile_t5::Ptr,
    off: usize,
) -> f32 {
    stream.f32_at(body, off).unwrap_or(0.0)
}

pub(super) fn read_bounce_array_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    body: fastfile_t5::Ptr,
    off: usize,
) -> Option<[f32; 31]> {
    let arr = match stream.ptr_at(body, off) {
        Ok(fastfile_t5::ZonePtr::Offset(q)) => stream.resolve_alias(q),
        _ => return None,
    };
    let mut values = [0.0; 31];
    for (i, value) in values.iter_mut().enumerate() {
        *value = stream.f32_at(arr, i * 4).ok()?;
    }
    Some(values)
}

pub(super) fn leftover_t5_offhand_class(raw: i32) -> i32 {
    match raw {
        4 => 5,
        other => other,
    }
}

pub(super) fn leftover_t5_ads_rate(trans_ms: i32, stored: f32) -> f32 {
    if stored > 0.0 {
        stored
    } else if trans_ms > 0 {
        1.0 / trans_ms as f32
    } else {
        0.0
    }
}

pub(super) fn leftover_t5_cstr(
    stream: &fastfile_t5::ZoneStream<'_>,
    body: fastfile_t5::Ptr,
    off: usize,
) -> Option<String> {
    let name_ptr = match stream.ptr_at(body, off).ok()? {
        fastfile_t5::ZonePtr::Offset(q) => stream.resolve_alias(q),
        _ => return None,
    };
    stream
        .cstr(name_ptr)
        .ok()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

pub(super) fn leftover_t5_asset_slot(
    stream: &fastfile_t5::ZoneStream<'_>,
    body: Option<fastfile_t5::Ptr>,
    off: usize,
) -> Option<Ptr> {
    let body = body?;
    match stream.ptr_at(body, off).ok()? {
        fastfile_t5::ZonePtr::Null => None,
        _ => Some(t5_to_iw4_ptr(body.at(off))),
    }
}

pub(super) fn leftover_t5_reticle(
    stream: &fastfile_t5::ZoneStream<'_>,
    weap_def: Option<fastfile_t5::Ptr>,
) -> WeaponReticleAssets {
    use fastfile_t5::size as sz;
    WeaponReticleAssets {
        center_material: leftover_t5_material_name_opt(
            stream,
            weap_def,
            sz::WEAPON_DEF_RETICLE_CENTER_OFF,
        ),
        side_material: leftover_t5_material_name_opt(
            stream,
            weap_def,
            sz::WEAPON_DEF_RETICLE_SIDE_OFF,
        ),
        center_authored: leftover_t5_asset_slot(
            stream,
            weap_def,
            sz::WEAPON_DEF_RETICLE_CENTER_OFF,
        )
        .is_some(),
        side_authored: leftover_t5_asset_slot(stream, weap_def, sz::WEAPON_DEF_RETICLE_SIDE_OFF)
            .is_some(),
        center_size: weap_def
            .map(|body| i32_at_t5(stream, body, sz::WEAPON_DEF_RETICLE_CENTER_SIZE_OFF))
            .unwrap_or(0),
        side_size: weap_def
            .map(|body| i32_at_t5(stream, body, sz::WEAPON_DEF_RETICLE_SIDE_SIZE_OFF))
            .unwrap_or(0),
        ..WeaponReticleAssets::default()
    }
}

pub(super) fn leftover_t5_header_name(
    stream: &fastfile_t5::ZoneStream<'_>,
    body: fastfile_t5::Ptr,
    off: usize,
) -> Option<String> {
    leftover_t5_material_name(stream, body, off)
}

pub(super) fn leftover_t5_material_name(
    stream: &fastfile_t5::ZoneStream<'_>,
    body: fastfile_t5::Ptr,
    off: usize,
) -> Option<String> {
    let mat = match stream.ptr_at(body, off).ok()? {
        fastfile_t5::ZonePtr::Offset(q) => stream.resolve_alias(q),
        _ => return None,
    };
    leftover_t5_cstr(stream, mat, 0)
}

pub(super) fn leftover_t5_material_name_opt(
    stream: &fastfile_t5::ZoneStream<'_>,
    body: Option<fastfile_t5::Ptr>,
    off: usize,
) -> Option<String> {
    leftover_t5_material_name(stream, body?, off)
}

pub(super) fn leftover_t5_overlay_name(
    stream: &fastfile_t5::ZoneStream<'_>,
    geometry: &fastfile_t5::WeaponGeometry,
) -> Option<String> {
    leftover_t5_overlay_pick(stream, geometry).and_then(|(name, _)| name)
}

pub(super) fn leftover_t5_overlay_slot(
    stream: &fastfile_t5::ZoneStream<'_>,
    geometry: &fastfile_t5::WeaponGeometry,
) -> Option<Ptr> {
    leftover_t5_overlay_pick(stream, geometry).and_then(|(_, slot)| slot)
}

pub(super) fn leftover_t5_overlay_pick(
    stream: &fastfile_t5::ZoneStream<'_>,
    geometry: &fastfile_t5::WeaponGeometry,
) -> Option<(Option<String>, Option<Ptr>)> {
    let variant = geometry.variant?;
    use fastfile_t5::size as sz;
    let hi = geometry
        .overlay_material_name
        .and_then(|p| stream.cstr(p).ok())
        .map(str::to_owned)
        .or_else(|| {
            leftover_t5_material_name(stream, variant, sz::WEAPON_VARIANT_OVERLAY_SHADER_OFF)
        });
    let lo = geometry
        .overlay_material_lowres_name
        .and_then(|p| stream.cstr(p).ok())
        .map(str::to_owned)
        .or_else(|| {
            leftover_t5_material_name(
                stream,
                variant,
                sz::WEAPON_VARIANT_OVERLAY_SHADER_LOWRES_OFF,
            )
        });
    let hi_slot =
        leftover_t5_asset_slot(stream, Some(variant), sz::WEAPON_VARIANT_OVERLAY_SHADER_OFF);
    let lo_slot = leftover_t5_asset_slot(
        stream,
        Some(variant),
        sz::WEAPON_VARIANT_OVERLAY_SHADER_LOWRES_OFF,
    );
    if hi.as_deref().is_some_and(overlay_name_is_hud_iris) {
        return Some((hi, hi_slot));
    }
    if lo.as_deref().is_some_and(overlay_name_is_hud_iris) {
        return Some((lo, lo_slot));
    }
    Some((hi.or(lo), hi_slot.or(lo_slot)))
}

pub(super) fn leftover_t5_zoom_fov(
    stream: &fastfile_t5::ZoneStream<'_>,
    variant: fastfile_t5::Ptr,
) -> f32 {
    use fastfile_t5::size as sz;
    leftover_t5_first_positive_fov(
        f32_at_t5(stream, variant, sz::WEAPON_VARIANT_ADS_ZOOM_FOV1_OFF),
        f32_at_t5(stream, variant, sz::WEAPON_VARIANT_ADS_ZOOM_FOV2_OFF),
        f32_at_t5(stream, variant, sz::WEAPON_VARIANT_ADS_ZOOM_FOV3_OFF),
    )
}

pub(super) fn leftover_t5_first_positive_fov(fov1: f32, fov2: f32, fov3: f32) -> f32 {
    for fov in [fov1, fov2, fov3] {
        if fov > 0.0 {
            return fov;
        }
    }
    0.0
}

pub(super) fn leftover_t5_combat_fx(
    stream: &fastfile_t5::ZoneStream<'_>,
    geometry: &fastfile_t5::WeaponGeometry,
) -> (WeaponCombatFx, CombatFxSlots) {
    use fastfile_t5::size as sz;
    let body = geometry.weap_def;
    let slots = CombatFxSlots {
        view_flash: leftover_t5_asset_slot(stream, body, sz::WEAPON_DEF_VIEW_FLASH_OFF),
        world_flash: leftover_t5_asset_slot(stream, body, sz::WEAPON_DEF_WORLD_FLASH_OFF),
        view_shell_eject: leftover_t5_asset_slot(stream, body, sz::WEAPON_DEF_VIEW_SHELL_EJECT_OFF),
        world_shell_eject: leftover_t5_asset_slot(
            stream,
            body,
            sz::WEAPON_DEF_WORLD_SHELL_EJECT_OFF,
        ),
        view_last_shot_eject: leftover_t5_asset_slot(
            stream,
            body,
            sz::WEAPON_DEF_VIEW_LAST_SHOT_EJECT_OFF,
        ),
        world_last_shot_eject: leftover_t5_asset_slot(
            stream,
            body,
            sz::WEAPON_DEF_WORLD_LAST_SHOT_EJECT_OFF,
        ),
        explosion: leftover_t5_asset_slot(stream, body, sz::WEAPON_DEF_PROJ_EXPLOSION_EFFECT_OFF),
        tracer: None,
    };
    let fx = WeaponCombatFx {
        view_flash_hint: body
            .and_then(|b| leftover_t5_header_name(stream, b, sz::WEAPON_DEF_VIEW_FLASH_OFF)),
        world_flash_hint: body
            .and_then(|b| leftover_t5_header_name(stream, b, sz::WEAPON_DEF_WORLD_FLASH_OFF)),
        view_shell_eject_hint: body
            .and_then(|b| leftover_t5_header_name(stream, b, sz::WEAPON_DEF_VIEW_SHELL_EJECT_OFF)),
        world_shell_eject_hint: body
            .and_then(|b| leftover_t5_header_name(stream, b, sz::WEAPON_DEF_WORLD_SHELL_EJECT_OFF)),
        view_last_shot_eject_hint: body.and_then(|b| {
            leftover_t5_header_name(stream, b, sz::WEAPON_DEF_VIEW_LAST_SHOT_EJECT_OFF)
        }),
        world_last_shot_eject_hint: body.and_then(|b| {
            leftover_t5_header_name(stream, b, sz::WEAPON_DEF_WORLD_LAST_SHOT_EJECT_OFF)
        }),
        explosion_hint: body.and_then(|b| {
            leftover_t5_header_name(stream, b, sz::WEAPON_DEF_PROJ_EXPLOSION_EFFECT_OFF)
        }),
        last_shot_eject_pair_authored: slots.last_shot_pair_authored(),
        ..WeaponCombatFx::empty(crate::AssetNamespace::T5)
    };
    (fx, slots)
}

pub(super) fn leftover_t5_sounds(
    stream: &fastfile_t5::ZoneStream<'_>,
    strings: &fastfile_t5::ScriptStrings,
    geometry: &fastfile_t5::WeaponGeometry,
) -> WeaponSoundAliases {
    use fastfile_t5::size as sz;
    let Some(body) = geometry.weap_def else {
        return WeaponSoundAliases::default();
    };
    WeaponSoundAliases {
        proj_explosion: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_PROJ_EXPLOSION_SOUND_OFF),
        proj_ignition_sound: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_PROJ_IGNITION_SOUND_OFF),
        notetrack_convention: NotetrackConvention::InlinePrefix,
        fire: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_FIRE_OFF),
        fire_player: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_FIRE_PLAYER_OFF),
        empty_fire: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_EMPTY_FIRE_OFF),
        empty_fire_player: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_EMPTY_FIRE_PLAYER_OFF),
        rechamber: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_RECHAMBER_OFF),
        rechamber_player: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_RECHAMBER_PLAYER_OFF),
        reload: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_RELOAD_OFF),
        reload_player: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_RELOAD_PLAYER_OFF),
        reload_empty: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_RELOAD_EMPTY_OFF),
        reload_empty_player: leftover_t5_cstr(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_EMPTY_PLAYER_OFF,
        ),
        reload_start: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_RELOAD_START_OFF),
        reload_start_player: leftover_t5_cstr(
            stream,
            body,
            sz::WEAPON_DEF_SND_RELOAD_START_PLAYER_OFF,
        ),
        reload_end: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_RELOAD_END_OFF),
        reload_end_player: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_RELOAD_END_PLAYER_OFF),
        raise_player: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_RAISE_PLAYER_OFF),
        putaway_player: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_PUTAWAY_PLAYER_OFF),
        melee_swipe: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_MELEE_SWIPE_OFF),
        melee_swipe_player: leftover_t5_cstr(
            stream,
            body,
            sz::WEAPON_DEF_SND_MELEE_SWIPE_PLAYER_OFF,
        ),
        melee_hit: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_MELEE_HIT_OFF),
        melee_miss: leftover_t5_cstr(stream, body, sz::WEAPON_DEF_SND_MELEE_MISS_OFF),
        notetrack_sound_map: leftover_t5_script_string_map(
            stream,
            strings,
            body,
            sz::WEAPON_DEF_NOTE_SOUND_KEYS_OFF,
            sz::WEAPON_DEF_NOTE_SOUND_VALUES_OFF,
        ),
        ..WeaponSoundAliases::default()
    }
}

pub(super) fn leftover_t5_script_string_map(
    stream: &fastfile_t5::ZoneStream<'_>,
    strings: &fastfile_t5::ScriptStrings,
    body: fastfile_t5::Ptr,
    keys_off: usize,
    values_off: usize,
) -> Vec<(String, String)> {
    let keys = match stream.ptr_at(body, keys_off) {
        Ok(fastfile_t5::ZonePtr::Offset(q)) => stream.resolve_alias(q),
        _ => return Vec::new(),
    };
    let values = match stream.ptr_at(body, values_off) {
        Ok(fastfile_t5::ZonePtr::Offset(q)) => stream.resolve_alias(q),
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for i in 0..fastfile_t5::size::WEAPON_NOTETRACK_COUNT {
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
        let Some(val) = strings.get(stream, val_id).filter(|s| !s.is_empty()) else {
            continue;
        };
        out.push((key.to_owned(), val.to_owned()));
    }
    out
}

pub(super) fn capture_t5_body_facts(
    stream: &fastfile_t5::ZoneStream<'_>,
    geometry: &fastfile_t5::WeaponGeometry,
) -> WeaponBodyFacts {
    use fastfile_t5::size as sz;
    let mut facts = WeaponBodyFacts {
        body_resolved: geometry.weap_def.is_some(),
        fire_time_ms: geometry.fire_time_ms,
        clip_size: geometry.clip_size,
        weap_type: geometry.weap_type,
        weap_class: remap_t5_weap_class(geometry.weap_class),
        fire_type: geometry.fire_type,
        move_speed_scale: geometry.move_speed_scale,
        ads_move_speed_scale: geometry.ads_move_speed_scale,
        rechamber_time_ms: geometry.rechamber_time_ms,
        drop_time_ms: geometry.drop_time_ms,
        alternate_raise_time_ms: geometry.alternate_raise_time_ms,
        alternate_drop_time_ms: geometry.alternate_drop_time_ms,
        raise_time_ms: geometry.raise_time_ms,
        bolt_action: geometry.bolt_action,
        select_requires_ammo: Some(leftover_t5_select_requires_ammo()),
        ..WeaponBodyFacts::default()
    };
    if let Some(body) = geometry.weap_def {
        facts.kill_icon_ratio = i32_at_t5(stream, body, sz::WEAPON_DEF_KILL_ICON_RATIO_OFF);
        facts.flip_kill_icon = u8_at_t5(stream, body, sz::WEAPON_DEF_FLIP_KILL_ICON_OFF) != 0;
    }
    if let Some(variant) = geometry.variant {
        facts.silenced = u8_at_t5(stream, variant, sz::WEAPON_VARIANT_SILENCED_OFF) != 0;
        if u8_at_t5(stream, variant, sz::WEAPON_VARIANT_RAPID_FIRE_OFF) != 0 {
            facts.fire_time_ms = (facts.fire_time_ms as f32 * 0.75) as i32;
        }
        facts.reload_time_ms = i32_at_t5(stream, variant, sz::WEAPON_VARIANT_RELOAD_TIME_OFF);
        facts.reload_empty_time_ms =
            i32_at_t5(stream, variant, sz::WEAPON_VARIANT_RELOAD_EMPTY_TIME_OFF);
        let ads_in_ms = i32_at_t5(stream, variant, sz::WEAPON_VARIANT_ADS_TRANS_IN_OFF);
        let ads_out_ms = i32_at_t5(stream, variant, sz::WEAPON_VARIANT_ADS_TRANS_OUT_OFF);
        let stored_in = f32_at_t5(stream, variant, sz::WEAPON_VARIANT_ADS_IN_RATE_OFF);
        let stored_out = f32_at_t5(stream, variant, sz::WEAPON_VARIANT_ADS_OUT_RATE_OFF);
        facts.ads_in_rate = leftover_t5_ads_rate(ads_in_ms, stored_in);
        facts.ads_out_rate = leftover_t5_ads_rate(ads_out_ms, stored_out);
        facts.ads_zoom_fov = leftover_t5_zoom_fov(stream, variant);
        facts.ads_zoom_in_frac =
            f32_at_t5(stream, variant, sz::WEAPON_VARIANT_ADS_ZOOM_IN_FRAC_OFF);
        facts.ads_zoom_out_frac =
            f32_at_t5(stream, variant, sz::WEAPON_VARIANT_ADS_ZOOM_OUT_FRAC_OFF);
    }
    let Some(body) = geometry.weap_def else {
        return facts;
    };
    facts.dual_wield = u8_at_t5(stream, body, sz::WEAPON_DEF_DUAL_WIELD_OFF) != 0;
    facts.impact_type = i32_at_t5(stream, body, sz::WEAPON_DEF_IMPACT_TYPE_OFF);
    facts.ammo_counter_clip = i32_at_t5(stream, body, sz::WEAPON_DEF_AMMO_COUNTER_CLIP_OFF);
    facts.start_ammo = i32_at_t5(stream, body, sz::WEAPON_DEF_START_AMMO_OFF);
    facts.max_ammo = i32_at_t5(stream, body, sz::WEAPON_DEF_MAX_AMMO_OFF);
    facts.ammo_count_clip_relative =
        u8_at_t5(stream, body, sz::WEAPON_DEF_AMMO_COUNT_CLIP_RELATIVE_OFF) != 0;
    facts.shots_per_fire = i32_at_t5(stream, body, sz::WEAPON_DEF_SHOT_COUNT_OFF);
    apply_leftover_hip_spread(
        &mut facts,
        leftover_hip_spread_block(
            |off| f32_at_t5(stream, body, off),
            sz::WEAPON_DEF_HIP_SPREAD_STAND_MIN_OFF,
        ),
    );
    facts.damage = i32_at_t5(stream, body, sz::WEAPON_DEF_DAMAGE_OFF);
    facts.min_damage = i32_at_t5(stream, body, sz::WEAPON_DEF_MIN_DAMAGE_OFF);
    facts.max_damage_range = f32_at_t5(stream, body, sz::WEAPON_DEF_MAX_DAMAGE_RANGE_OFF);
    facts.min_damage_range = f32_at_t5(stream, body, sz::WEAPON_DEF_MIN_DAMAGE_RANGE_OFF);
    facts.explosion_radius = i32_at_t5(stream, body, sz::WEAPON_DEF_EXPLOSION_RADIUS_OFF);
    facts.explosion_radius_min = i32_at_t5(stream, body, sz::WEAPON_DEF_EXPLOSION_RADIUS_MIN_OFF);
    facts.explosion_inner_damage =
        i32_at_t5(stream, body, sz::WEAPON_DEF_EXPLOSION_INNER_DAMAGE_OFF);
    facts.explosion_outer_damage =
        i32_at_t5(stream, body, sz::WEAPON_DEF_EXPLOSION_OUTER_DAMAGE_OFF);
    facts.projectile_speed = i32_at_t5(stream, body, sz::WEAPON_DEF_PROJECTILE_SPEED_OFF);
    facts.projectile_speed_up = i32_at_t5(stream, body, sz::WEAPON_DEF_PROJECTILE_SPEED_UP_OFF);
    facts.projectile_activate_dist =
        i32_at_t5(stream, body, sz::WEAPON_DEF_PROJECTILE_ACTIVATE_DIST_OFF);
    facts.projectile_explosion_type =
        i32_at_t5(stream, body, sz::WEAPON_DEF_PROJ_EXPLOSION_TYPE_OFF);
    facts.missile_guidance = i32_at_t5(stream, body, sz::WEAPON_DEF_MISSILE_GUIDANCE_OFF);
    facts.ignition_delay_ms = i32_at_t5(stream, body, sz::WEAPON_DEF_IGNITION_DELAY_OFF);
    facts.proj_impact_explode = u8_at_t5(stream, body, sz::WEAPON_DEF_PROJ_IMPACT_EXPLODE_OFF) != 0;
    facts.offhand_class =
        leftover_t5_offhand_class(i32_at_t5(stream, body, sz::WEAPON_DEF_OFFHAND_CLASS_OFF));
    facts.hold_fire_time_ms = i32_at_t5(stream, body, sz::WEAPON_DEF_HOLD_FIRE_TIME_OFF);
    facts.fuse_time_ms = i32_at_t5(stream, body, sz::WEAPON_DEF_FUSE_TIME_OFF);
    facts.require_lock_to_fire =
        u8_at_t5(stream, body, sz::WEAPON_DEF_REQUIRE_LOCK_TO_FIRE_OFF) != 0;
    facts.stickiness = i32_at_t5(stream, body, sz::WEAPON_DEF_STICKINESS_OFF);
    facts.stick_to_players = matches!(facts.stickiness, 1 | 5);
    facts.has_detonator = u8_at_t5(stream, body, sz::WEAPON_DEF_HAS_DETONATOR_OFF) != 0;
    facts.timed_detonation = u8_at_t5(stream, body, sz::WEAPON_DEF_TIMED_DETONATION_OFF) != 0;
    facts.projectile_rotates = u8_at_t5(stream, body, sz::WEAPON_DEF_ROTATE_OFF) != 0;
    facts.cook_off_hold = u8_at_t5(stream, body, sz::WEAPON_DEF_COOK_OFF_HOLD_OFF) != 0;
    facts.offhand_hold_is_cancelable =
        Some(u8_at_t5(stream, body, sz::WEAPON_DEF_OFFHAND_HOLD_IS_CANCELABLE_OFF) != 0);
    facts.parallel_bounce = read_bounce_array_t5(stream, body, sz::WEAPON_DEF_PARALLEL_BOUNCE_OFF);
    facts.perpendicular_bounce =
        read_bounce_array_t5(stream, body, sz::WEAPON_DEF_PERPENDICULAR_BOUNCE_OFF);
    facts.fire_delay_ms = i32_at_t5(stream, body, sz::WEAPON_DEF_FIRE_DELAY_OFF);
    facts.quick_drop_time_ms = i32_at_t5(stream, body, sz::WEAPON_QUICK_DROP_TIME_OFF);
    facts.quick_raise_time_ms = i32_at_t5(stream, body, sz::WEAPON_QUICK_RAISE_TIME_OFF);
    facts.melee_damage = i32_at_t5(stream, body, sz::WEAPON_DEF_MELEE_DAMAGE_OFF);
    facts.melee_time_ms = i32_at_t5(stream, body, sz::WEAPON_DEF_MELEE_TIME_OFF);
    facts.melee_delay_ms = i32_at_t5(stream, body, sz::WEAPON_DEF_MELEE_DELAY_OFF);
    facts.melee_charge_time_ms = i32_at_t5(stream, body, sz::WEAPON_DEF_MELEE_CHARGE_TIME_OFF);
    facts.melee_charge_delay_ms = i32_at_t5(stream, body, sz::WEAPON_DEF_MELEE_CHARGE_DELAY_OFF);
    facts.use_as_melee = u8_at_t5(stream, body, sz::WEAPON_DEF_USE_AS_MELEE_OFF) != 0;
    facts.rechamber_bolt_delay_ms = i32_at_t5(stream, body, sz::WEAPON_DEF_RECHAMBER_BOLT_TIME_OFF);

    facts.reload_show_rocket_time_ms = i32_at_t5(stream, body, 0x3d4);
    facts.reload_add_time_ms = i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_ADD_TIME_OFF);
    facts.reload_empty_add_time_ms =
        i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_EMPTY_ADD_TIME_OFF);
    facts.reload_start_time_ms = i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_START_TIME_OFF);
    facts.reload_start_add_time_ms =
        i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_START_ADD_TIME_OFF);
    facts.reload_end_time_ms = i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_END_TIME_OFF);
    if let Some(variant) = geometry.variant
        && u8_at_t5(stream, variant, sz::WEAPON_VARIANT_DUAL_MAG_OFF) != 0
    {
        facts.dual_mag = Some(weapon_iw4::DualMagTimes {
            reload_ms: i32_at_t5(stream, variant, sz::WEAPON_VARIANT_RELOAD_QUICK_TIME_OFF),
            reload_empty_ms: i32_at_t5(
                stream,
                variant,
                sz::WEAPON_VARIANT_RELOAD_QUICK_EMPTY_TIME_OFF,
            ),
            add_ms: i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_QUICK_ADD_TIME_OFF),
            empty_add_ms: i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_QUICK_EMPTY_ADD_TIME_OFF),
        });
    }
    facts.reload_ammo_add = i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_AMMO_ADD_OFF);
    facts.reload_start_add = i32_at_t5(stream, body, sz::WEAPON_DEF_RELOAD_START_ADD_OFF);
    facts.overlay_reticle = i32_at_t5(stream, body, sz::WEAPON_DEF_ADS_OVERLAY_RETICLE_OFF);
    facts.can_hold_breath = facts.overlay_reticle != 0 && facts.weap_class != 11;
    facts.overlay_interface = i32_at_t5(stream, body, sz::WEAPON_DEF_ADS_OVERLAY_INTERFACE_OFF);
    facts.ads_overlay_width = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_OVERLAY_WIDTH_OFF);
    facts.ads_overlay_height = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_OVERLAY_HEIGHT_OFF);
    facts.i_reticle_side_size = i32_at_t5(stream, body, sz::WEAPON_DEF_RETICLE_SIDE_SIZE_OFF);
    facts.i_reticle_min_ofs = i32_at_t5(stream, body, sz::WEAPON_DEF_RETICLE_MIN_OFS_OFF);
    facts.hip_reticle_side_pos = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_RETICLE_SIDE_POS_OFF);
    facts.no_ads_when_mag_empty =
        u8_at_t5(stream, body, sz::WEAPON_DEF_NO_ADS_WHEN_MAG_EMPTY_OFF) != 0;
    facts.aim_down_sight = u8_at_t5(stream, body, sz::WEAPON_DEF_AIM_DOWN_SIGHT_OFF) != 0;
    facts.rechamber_while_ads = u8_at_t5(stream, body, sz::WEAPON_DEF_RECHAMBER_WHILE_ADS_OFF) != 0;
    facts.ads_fire_only = u8_at_t5(stream, body, sz::WEAPON_DEF_ADS_FIRE_ONLY_OFF) != 0;
    facts.no_partial_reload = u8_at_t5(stream, body, sz::WEAPON_DEF_NO_PARTIAL_RELOAD_OFF) != 0;
    facts.segmented_reload = u8_at_t5(stream, body, sz::WEAPON_DEF_SEGMENTED_RELOAD_OFF) != 0;
    facts.idle = WeaponIdleInputs {
        ads_idle_amount: f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_IDLE_AMOUNT_OFF),
        hip_idle_amount: f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_IDLE_AMOUNT_OFF),
        ads_idle_speed: f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_IDLE_SPEED_OFF),
        hip_idle_speed: f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_IDLE_SPEED_OFF),
        idle_crouch_factor: f32_at_t5(stream, body, sz::WEAPON_DEF_IDLE_CROUCH_FACTOR_OFF),
        idle_prone_factor: f32_at_t5(stream, body, sz::WEAPON_DEF_IDLE_PRONE_FACTOR_OFF),
    };
    facts.inherits_perks = leftover_t5_inherits_host_perks();
    facts.kick = leftover_t5_kick(stream, geometry);
    facts
}

pub(super) fn leftover_t5_inherits_host_perks() -> bool {
    true
}

pub(super) fn leftover_t5_select_requires_ammo() -> bool {
    false
}

pub(super) fn leftover_t5_kick(
    stream: &fastfile_t5::ZoneStream<'_>,
    geometry: &fastfile_t5::WeaponGeometry,
) -> WeaponKickFacts {
    use fastfile_t5::size as sz;
    let mut k = WeaponKickFacts::default();
    if let Some(variant) = geometry.variant {
        k.f_ads_view_kick_center_speed = f32_at_t5(
            stream,
            variant,
            sz::WEAPON_VARIANT_ADS_VIEW_KICK_CENTER_SPEED_OFF,
        );
        k.f_hip_view_kick_center_speed = f32_at_t5(
            stream,
            variant,
            sz::WEAPON_VARIANT_HIP_VIEW_KICK_CENTER_SPEED_OFF,
        );
    }
    let Some(body) = geometry.weap_def else {
        return k;
    };
    k.gun_max_pitch = f32_at_t5(stream, body, sz::WEAPON_DEF_GUN_MAX_PITCH_OFF);
    k.gun_max_yaw = f32_at_t5(stream, body, sz::WEAPON_DEF_GUN_MAX_YAW_OFF);
    k.ads_gun_kick_reduced_kick_bullets = i32_at_t5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_REDUCED_BULLETS_OFF,
    );
    k.ads_gun_kick_reduced_kick_percent = f32_at_t5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_REDUCED_PERCENT_OFF,
    );
    k.ads_gun_kick_pitch_min = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_PITCH_MIN_OFF);
    k.ads_gun_kick_pitch_max = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_PITCH_MAX_OFF);
    k.ads_gun_kick_yaw_min = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_YAW_MIN_OFF);
    k.ads_gun_kick_yaw_max = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_YAW_MAX_OFF);
    k.ads_gun_kick_accel = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_ACCEL_OFF);
    k.ads_gun_kick_speed_max = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_SPEED_MAX_OFF);
    k.ads_gun_kick_speed_decay =
        f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_SPEED_DECAY_OFF);
    k.ads_gun_kick_static_decay =
        f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_STATIC_DECAY_OFF);
    k.ads_view_kick_pitch_min = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_VIEW_KICK_PITCH_MIN_OFF);
    k.ads_view_kick_pitch_max = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_VIEW_KICK_PITCH_MAX_OFF);
    k.ads_view_kick_yaw_min = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_VIEW_KICK_YAW_MIN_OFF);
    k.ads_view_kick_yaw_max = f32_at_t5(stream, body, sz::WEAPON_DEF_ADS_VIEW_KICK_YAW_MAX_OFF);
    k.hip_gun_kick_reduced_kick_bullets = i32_at_t5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_REDUCED_BULLETS_OFF,
    );
    k.hip_gun_kick_reduced_kick_percent = f32_at_t5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_REDUCED_PERCENT_OFF,
    );
    k.hip_gun_kick_pitch_min = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_PITCH_MIN_OFF);
    k.hip_gun_kick_pitch_max = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_PITCH_MAX_OFF);
    k.hip_gun_kick_yaw_min = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_YAW_MIN_OFF);
    k.hip_gun_kick_yaw_max = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_YAW_MAX_OFF);
    k.hip_gun_kick_accel = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_ACCEL_OFF);
    k.hip_gun_kick_speed_max = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_SPEED_MAX_OFF);
    k.hip_gun_kick_speed_decay =
        f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_SPEED_DECAY_OFF);
    k.hip_gun_kick_static_decay =
        f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_STATIC_DECAY_OFF);
    k.hip_view_kick_pitch_min = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_VIEW_KICK_PITCH_MIN_OFF);
    k.hip_view_kick_pitch_max = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_VIEW_KICK_PITCH_MAX_OFF);
    k.hip_view_kick_yaw_min = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_VIEW_KICK_YAW_MIN_OFF);
    k.hip_view_kick_yaw_max = f32_at_t5(stream, body, sz::WEAPON_DEF_HIP_VIEW_KICK_YAW_MAX_OFF);
    k
}

pub(super) fn read_hide_tags_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    strings: &fastfile_t5::ScriptStrings,
    arr: Option<fastfile_t5::Ptr>,
) -> Vec<String> {
    let Some(arr) = arr else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in 0..fastfile_t5::size::WEAPON_HIDE_TAG_COUNT {
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
