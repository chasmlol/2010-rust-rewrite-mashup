use bevy::prelude::*;

use super::dof::GlowDvars;

#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct FilmVisionView {
    pub current: Option<asset_world::FilmVision>,
    pub script_forced: bool,
    /// Hue in radians, gamma, exposure in stops, saturation.
    pub grading: [f32; 4],
    pub blur: f32,
    from: hud_iw4::VisionSetVars,
    to: hud_iw4::VisionSetVars,
    result: hud_iw4::VisionSetVars,
    lerp: hud_iw4::VisionSetLerpData,
    last_map: Option<asset_world::FilmVision>,
    override_vision: Option<asset_world::FilmVision>,
}

impl Default for FilmVisionView {
    fn default() -> Self {
        Self {
            current: None,
            script_forced: false,
            grading: [0.0, 1.0, 0.0, 1.0],
            blur: 0.0,
            from: hud_iw4::VisionSetVars::default(),
            to: hud_iw4::VisionSetVars::default(),
            result: hud_iw4::VisionSetVars::default(),
            lerp: hud_iw4::VisionSetLerpData::default(),
            last_map: None,
            override_vision: None,
        }
    }
}

impl FilmVisionView {
    /// Select a preset, or restore the map with None. Call only for a ready view.
    pub fn select(
        &mut self,
        map: Option<asset_world::FilmVision>,
        preset: Option<asset_world::FilmVision>,
        now_ms: i32,
        duration_ms: i32,
        allowed: bool,
        script_forced: bool,
    ) {
        if self.last_map.is_none() {
            self.result = pack_film_vision(map.unwrap_or_default());
            self.lerp.style = hud_iw4::VISION_SET_LERP_HOLD;
        }
        let (current, _) = hud_iw4::vision_sets_update(
            now_ms,
            self.from,
            self.to,
            self.lerp,
            self.result,
            allowed,
            script_forced,
        );
        let target = preset.or(map).unwrap_or_default();
        (self.from, self.to, self.lerp) = hud_iw4::vision_set_start(
            now_ms,
            duration_ms,
            hud_iw4::VISION_SET_LERP_TO_SMOOTH,
            self.lerp.style,
            current,
            pack_film_vision(target),
        );
        self.result = if duration_ms <= 0 { self.to } else { current };
        self.last_map = Some(target);
        self.override_vision = preset;
    }
}

pub fn pack_film_vision(vision: asset_world::FilmVision) -> hud_iw4::VisionSetVars {
    hud_iw4::VisionSetVars {
        r_glow: vision.glow_enable,
        r_glow_bloom_cutoff: vision.glow_bloom_cutoff,
        r_glow_bloom_desaturation: vision.glow_bloom_desaturation,
        r_glow_bloom_intensity0: vision.glow_bloom_intensity,
        r_glow_radius0: vision.glow_radius,
        r_film_enable: vision.enable,
        r_film_brightness: vision.brightness,
        r_film_contrast: vision.contrast,
        r_film_desaturation: vision.desaturation,
        r_film_desaturation_dark: vision.desaturation_dark,
        r_film_invert: vision.invert,
        r_film_light_tint: vision.light_tint,
        r_film_medium_tint: vision.medium_tint,
        r_film_dark_tint: vision.dark_tint,
        ..hud_iw4::VisionSetVars::default()
    }
}

pub fn unpack_film_vision(vars: hud_iw4::VisionSetVars) -> asset_world::FilmVision {
    asset_world::FilmVision {
        enable: vars.r_film_enable,
        contrast: vars.r_film_contrast,
        brightness: vars.r_film_brightness,
        desaturation: vars.r_film_desaturation,
        desaturation_dark: vars.r_film_desaturation_dark,
        invert: vars.r_film_invert,
        light_tint: vars.r_film_light_tint,
        medium_tint: vars.r_film_medium_tint,
        dark_tint: vars.r_film_dark_tint,
        glow_enable: vars.r_glow,
        glow_radius: vars.r_glow_radius0,
        glow_bloom_cutoff: vars.r_glow_bloom_cutoff,
        glow_bloom_desaturation: vars.r_glow_bloom_desaturation,
        glow_bloom_intensity: vars.presented_glow_intensity0(),
    }
}

#[must_use]
pub fn presented_film_vision(
    view_ready: bool,
    map_vision: Option<asset_world::FilmVision>,
) -> Option<asset_world::FilmVision> {
    if view_ready { map_vision } else { None }
}

#[must_use]
pub fn presented_film_vision_with_glow_tweaks(
    view_ready: bool,
    map_vision: Option<asset_world::FilmVision>,
    use_tweaks: bool,
    tweaks: lighting_iw4::GlowViewInfo,
) -> Option<asset_world::FilmVision> {
    if !view_ready {
        return None;
    }
    if !use_tweaks {
        return map_vision;
    }
    let mut vision = map_vision.unwrap_or_default();
    let selected = lighting_iw4::select_glow_view_info(
        lighting_iw4::GlowViewInfo {
            enable: vision.glow_enable,
            cutoff: vision.glow_bloom_cutoff,
            desaturation: vision.glow_bloom_desaturation,
            intensity: vision.glow_bloom_intensity,
            radius: vision.glow_radius,
        },
        true,
        tweaks,
    );
    vision.glow_enable = selected.enable;
    vision.glow_bloom_cutoff = selected.cutoff;
    vision.glow_bloom_desaturation = selected.desaturation;
    vision.glow_bloom_intensity = selected.intensity;
    vision.glow_radius = selected.radius;
    Some(vision)
}

#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn presented_film_vision_with_lerp(
    view_ready: bool,
    map_vision: Option<asset_world::FilmVision>,
    now_ms: i32,
    duration_ms: i32,
    slot: &mut FilmVisionView,
    use_tweaks: bool,
    tweaks: lighting_iw4::GlowViewInfo,
    allowed: bool,
    script_forced: bool,
) -> Option<asset_world::FilmVision> {
    if !view_ready {
        *slot = FilmVisionView::default();
        return None;
    }
    let Some(map) = map_vision else {
        *slot = FilmVisionView::default();
        return None;
    };
    if slot.last_map != Some(map) {
        let (from, to, lerp) = hud_iw4::vision_set_start(
            now_ms,
            duration_ms,
            hud_iw4::VISION_SET_LERP_TO_LINEAR,
            slot.lerp.style,
            slot.result,
            pack_film_vision(map),
        );
        slot.from = from;
        slot.to = to;
        slot.lerp = lerp;
        slot.last_map = Some(map);
        if slot.lerp.style == hud_iw4::VISION_SET_LERP_HOLD {
            slot.result = slot.to;
        }
    }
    let (vars, lerp) = hud_iw4::vision_sets_update(
        now_ms,
        slot.from,
        slot.to,
        slot.lerp,
        slot.result,
        allowed,
        script_forced,
    );
    slot.lerp = lerp;
    slot.result = vars;
    let mixed = unpack_film_vision(vars);
    presented_film_vision_with_glow_tweaks(true, Some(mixed), use_tweaks, tweaks)
}

#[derive(Resource, Default)]
struct AppliedVision {
    vision: Option<Option<sim::VisionChange>>,
    instant_thermal: bool,
    pain: Option<Option<sim::VisionChange>>,
    pain_slot: FilmVisionView,
    pain_strength: f32,
}

pub fn register(app: &mut App) {
    app.init_resource::<AppliedVision>();
    app.init_resource::<FilmVisionView>().add_systems(
        Update,
        update_film_vision_view
            .after(crate::prepare::scene::view_parms::stamp_prepared_scene_view)
            .after(frame::ScreenEffectsPublished)
            .in_set(net::ClientSet::Present),
    );
}

#[allow(clippy::too_many_arguments)]
fn update_film_vision_view(
    view: Res<crate::prepare::scene::view_parms::PreparedSceneView>,
    scene: Res<crate::prepare::scene::world::WorldScene>,
    glow: Res<GlowDvars>,
    clock: Res<net::FrameClock>,
    mut film: ResMut<FilmVisionView>,
    presented: Res<net::PresentedSnapshot>,
    local: Res<net::LocalPresentClient>,
    mut applied: ResMut<AppliedVision>,
    settings: Res<frame::GameSettings>,
    screen_effects: Res<frame::ScreenEffectsView>,
) {
    if !view.ready {
        applied.vision = None;
    } else {
        let wanted = presented.snapshot().and_then(|snapshot| {
            let meta = snapshot.meta.for_client(local.0)?;
            let effects = &meta.view_effects;
            let global = &snapshot.meta.objectives;
            if meta.remote_missile.is_some() {
                effects
                    .missile_vision
                    .clone()
                    .or_else(|| global.missile_vision.clone())
            } else if screen_effects.thermal_active {
                effects
                    .thermal_vision
                    .clone()
                    .or_else(|| global.thermal_vision.clone())
            } else if presented
                .player(local.0)
                .is_some_and(|ps| ps.weap_flags & playerstate_iw4::weap_flags::NIGHT_VISION != 0)
                || meta
                    .client_dvars
                    .iter()
                    .rev()
                    .chain(global.server_info.iter().rev())
                    .find(|(name, _)| name.eq_ignore_ascii_case("nightvision"))
                    .is_some_and(|(_, value)| value == "1")
            {
                effects
                    .night_vision
                    .clone()
                    .or_else(|| global.night_vision.clone())
            } else {
                effects
                    .naked_vision
                    .clone()
                    .or_else(|| global.naked_vision.clone())
            }
        });
        film.script_forced = wanted.is_some()
            || presented
                .snapshot()
                .and_then(|s| {
                    s.meta.for_client(local.0).map(|meta| {
                        meta.client_dvars
                            .iter()
                            .chain(s.meta.objectives.server_info.iter())
                            .any(|(name, _)| {
                                sim::is_postfx_dvar(name)
                                    && !matches!(
                                        name.to_ascii_lowercase().as_str(),
                                        "cg_drawshellshock"
                                            | "thermalblurfactorscope"
                                            | "thermalblurfactornoscope"
                                    )
                            })
                    })
                })
                .unwrap_or(false);
        if applied.vision.as_ref() != Some(&wanted) {
            let preset = wanted
                .as_ref()
                .and_then(|vision| loaded_script_vision(&scene, vision));
            let duration_ms = if screen_effects.instant_thermal || applied.instant_thermal {
                0
            } else {
                match (&applied.vision, &wanted) {
                    (Some(_), Some(vision)) => vision.duration_ms,
                    _ => 0,
                }
            };
            film.select(
                scene.film_vision,
                preset,
                clock.time(),
                duration_ms,
                glow.allowed || wanted.is_some(),
                glow.allowed_script_forced || wanted.is_some(),
            );
            applied.vision = Some(wanted);
        }
        applied.instant_thermal = screen_effects.instant_thermal;
    }
    let script_forced = film.script_forced;
    let mixed = presented_film_vision_with_lerp(
        view.ready,
        film.override_vision.or(scene.film_vision),
        clock.time(),
        0,
        &mut film,
        glow.use_tweaks && !script_forced,
        glow.tweak_view_info(),
        glow.allowed || script_forced,
        glow.allowed_script_forced || script_forced,
    );
    let mut mixed = mixed;
    if !view.ready {
        *applied = AppliedVision::default();
    } else if let Some(snapshot) = presented.snapshot() {
        let wanted = snapshot
            .meta
            .for_client(local.0)
            .and_then(|meta| meta.view_effects.pain_vision.clone())
            .or_else(|| snapshot.meta.objectives.pain_vision.clone());
        if applied.pain.as_ref() != Some(&wanted) {
            let preset = wanted
                .as_ref()
                .and_then(|vision| loaded_script_vision(&scene, vision));
            let duration = if applied.pain.is_some() {
                wanted.as_ref().map_or(0, |v| v.duration_ms)
            } else {
                0
            };
            applied.pain_slot.select(
                scene.film_vision,
                preset,
                clock.time(),
                duration,
                true,
                true,
            );
            applied.pain = Some(wanted.clone());
        }
        let health = presented
            .player(local.0)
            .filter(|ps| ps.health > 0 && ps.max_health > 0 && ps.pm_type < 5)
            .map(|ps| ps.health as f32 / ps.max_health as f32);
        if let Some(health) = health.filter(|_| wanted.is_some()) {
            if health <= 0.5 || applied.pain_strength > 0.0 {
                let target = (1.0 - health).clamp(0.0, 1.0);
                applied.pain_strength = target.max(applied.pain_strength - clock.frametime_secs());
            }
        } else {
            applied.pain_strength = 0.0;
        }
        let pain_map = applied.pain_slot.override_vision.or(scene.film_vision);
        let pain = presented_film_vision_with_lerp(
            true,
            pain_map,
            clock.time(),
            0,
            &mut applied.pain_slot,
            false,
            glow.tweak_view_info(),
            true,
            true,
        );
        if let Some(pain) = pain.filter(|_| applied.pain_strength > 0.0) {
            let base = pack_film_vision(match mixed {
                Some(vision) => vision,
                None => asset_world::FilmVision::default(),
            });
            mixed = Some(unpack_film_vision(hud_iw4::vision_lerp_vars(
                base,
                pack_film_vision(pain),
                applied.pain_strength,
                hud_iw4::VISION_SET_LERP_TO_LINEAR,
            )));
            film.script_forced = true;
        }
    }
    film.current = if view.ready && !film.script_forced && settings.brightness != 0.0 {
        let mut vision = mixed.unwrap_or(asset_world::FilmVision::default());
        vision.enable = true;
        vision.brightness += settings.brightness;
        Some(vision)
    } else {
        mixed
    };
    film.grading = [0.0, 1.0, 0.0, 1.0];
    film.blur = 0.0;
    if !view.ready {
        film.script_forced = false;
        return;
    }
    if let Some(snapshot) = presented.snapshot() {
        if let Some(meta) = snapshot.meta.for_client(local.0) {
            film.blur = meta
                .view_effects
                .blur
                .map_or(0.0, |blur| blur.sample(clock.time()));
            let mut vision = match film.current {
                Some(vision) => vision,
                None => asset_world::FilmVision::default(),
            };
            let mut grading = film.grading;
            let mut blur = film.blur;
            let mut enable_override = None;
            for (name, value) in snapshot
                .meta
                .objectives
                .server_info
                .iter()
                .chain(meta.client_dvars.iter())
            {
                if matches!(
                    name.to_ascii_lowercase().as_str(),
                    "r_filmenable" | "r_filmtweakenable"
                ) {
                    if let Ok(value) = value.parse::<f32>() {
                        if value.is_finite() {
                            enable_override = Some(value != 0.0);
                        }
                    }
                }
                if apply_script_film_dvar(&mut vision, &mut grading, &mut blur, name, value) {
                    film.script_forced = true;
                }
            }
            if let Some(enable) = enable_override {
                vision.enable = enable;
            }
            film.grading = grading;
            film.blur = blur;
            film.current = Some(vision);
        }
    }
}

fn loaded_script_vision(
    scene: &crate::prepare::scene::world::WorldScene,
    vision: &sim::VisionChange,
) -> Option<asset_world::FilmVision> {
    if vision.name.is_empty() {
        return None;
    }
    let key = format!("vision/{}.vision", vision.name.to_ascii_lowercase());
    match scene.film_visions.get(&key) {
        Some(Ok(preset)) => Some(*preset),
        Some(Err(error)) => {
            diag::warn!(World, "vision {key}: {error:?}");
            None
        }
        None => {
            diag::warn!(World, "vision {key} is not loaded");
            None
        }
    }
}

/// Values come from snapshot metadata, never from the console permission path.
fn apply_script_film_dvar(
    vision: &mut asset_world::FilmVision,
    grading: &mut [f32; 4],
    blur: &mut f32,
    name: &str,
    value: &str,
) -> bool {
    let name = name.to_ascii_lowercase().replace("r_filmtweak", "r_film");
    let value = value.trim().trim_matches('"');
    if matches!(
        name.as_str(),
        "r_filmlighttint" | "r_filmmediumtint" | "r_filmdarktint"
    ) {
        let Ok(values) = value
            .split_whitespace()
            .map(str::parse::<f32>)
            .collect::<Result<Vec<_>, _>>()
        else {
            return false;
        };
        if values.len() != 3 || !values.iter().all(|v| v.is_finite()) {
            return false;
        }
        let tint = [values[0], values[1], values[2]];
        vision.enable = true;
        match name.as_str() {
            "r_filmlighttint" => vision.light_tint = tint,
            "r_filmmediumtint" => vision.medium_tint = tint,
            _ => vision.dark_tint = tint,
        }
        return true;
    }
    let Ok(v) = value.parse::<f32>() else {
        return false;
    };
    if !v.is_finite() {
        return false;
    }
    match name.as_str() {
        "r_filmenable" => vision.enable = v != 0.0,
        "r_filmusetweaks" | "r_glowusetweaks" => {}
        "r_filmbrightness" | "r_brightness" => {
            vision.enable = true;
            vision.brightness = v;
        }
        "r_filmcontrast" | "r_contrast" => {
            vision.enable = true;
            vision.contrast = v;
        }
        "r_filmdesaturation" => {
            vision.enable = true;
            vision.desaturation = v;
        }
        "r_filmdesaturationdark" => {
            vision.enable = true;
            vision.desaturation_dark = v;
        }
        "r_filminvert" => {
            vision.enable = true;
            vision.invert = v != 0.0;
        }
        "r_glow" | "r_glowtweakenable" => vision.glow_enable = v != 0.0,
        "r_glowradius0" | "r_glowtweakradius0" => vision.glow_radius = v,
        "r_glowbloomintensity0" | "r_glowtweakbloomintensity0" => vision.glow_bloom_intensity = v,
        "r_glowbloomcutoff" | "r_glowtweakbloomcutoff" => vision.glow_bloom_cutoff = v,
        "r_glowbloomdesaturation" | "r_glowtweakbloomdesaturation" => {
            vision.glow_bloom_desaturation = v
        }
        "r_hue" | "r_filmhue" => grading[0] = v.to_radians(),
        "r_gamma" => grading[1] = v.max(0.001),
        "r_exposure" => grading[2] = v,
        "r_saturation" => grading[3] = v,
        "r_blur" => *blur = v.max(0.0),
        _ => return false,
    }
    true
}
