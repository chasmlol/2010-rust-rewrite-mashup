use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
use crate::gpu_list::{HudTessPass, TessJob};
use asset_game::MenuCatalog;
use bevy::prelude::*;
use hud_iw4::*;
use net::{CEntity, CEntityRuntime, FrameClock, LocalPresentClient, PresentedSnapshot};

use std::collections::HashMap;

use crate::gaps::{GapCause, HudGap, HudPresentationGaps};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OverheadPosedHead {
    ExactWorld([f32; 3]),
    NoDObjOrHead,
}

#[derive(Resource, Debug, Default)]
pub struct OverheadPosedPlayerFrame {
    by_ent: HashMap<u16, OverheadPosedHead>,
}

impl OverheadPosedPlayerFrame {
    pub fn replace(&mut self, rows: impl IntoIterator<Item = (u16, OverheadPosedHead)>) {
        self.by_ent.clear();
        self.by_ent.extend(rows);
    }

    fn get(&self, entnum: u16) -> Option<OverheadPosedHead> {
        self.by_ent.get(&entnum).copied()
    }
}

#[derive(Resource, Debug, Default)]
pub struct OverheadPosedModelFrame {
    models: std::collections::HashSet<(sim::ScriptModelId, u16)>,
}
impl OverheadPosedModelFrame {
    pub fn replace(&mut self, rows: impl IntoIterator<Item = (sim::ScriptModelId, u16)>) {
        self.models.clear();
        self.models.extend(rows);
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct OverheadPosedPlayerFramePublished;

#[derive(Component)]
pub(crate) struct OverheadNamesRaster;

#[derive(Default)]
pub(crate) struct NameMemory {
    last_time: Option<i32>,
    seen: HashMap<u16, (i32, i32, bool)>,
}

#[derive(Clone, Copy, Debug)]
struct TargetBoxSettings {
    client: Option<sim::ClientId>,
    scale: f32,
    min_size: f32,
    delay_seconds: f32,
    fade_seconds: f32,
}

impl Default for TargetBoxSettings {
    fn default() -> Self {
        Self {
            client: None,
            scale: sim::TargetBoxDvar::Scale.default_value(),
            min_size: sim::TargetBoxDvar::MinSize.default_value(),
            delay_seconds: sim::TargetBoxDvar::SpawnDelay.default_value(),
            fade_seconds: sim::TargetBoxDvar::SpawnFade.default_value(),
        }
    }
}

impl TargetBoxSettings {
    fn update(&mut self, client: sim::ClientId, dvars: sim::ScriptDvars<'_>) {
        if self.client != Some(client) {
            *self = Self {
                client: Some(client),
                ..Self::default()
            };
        }
        for (setting, slot) in [
            (sim::TargetBoxDvar::Scale, &mut self.scale),
            (sim::TargetBoxDvar::MinSize, &mut self.min_size),
            (sim::TargetBoxDvar::SpawnDelay, &mut self.delay_seconds),
            (sim::TargetBoxDvar::SpawnFade, &mut self.fade_seconds),
        ] {
            match dvars.float(setting.name()) {
                Some(value) if setting.accepts(value) => *slot = value,
                None => *slot = setting.default_value(),
                _ => {}
            }
        }
    }
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<OverheadPosedModelFrame>()
        .init_resource::<OverheadPosedPlayerFrame>()
        .add_systems(
            Update,
            update_overhead_names
                .after(OverheadPosedPlayerFramePublished)
                .after(frame::ScreenEffectsPublished)
                .after(crate::surface::update_hud_surface)
                .before(crate::plugin::flush_overhead_names_tess)
                .before(crate::gaps::report_hud_gaps)
                .in_set(frame::LifeFrontPublished),
        );
}

fn name_color(local_team: i32, target_team: i32) -> [f32; 4] {
    if target_team == 3 {
        [0.65, 0.65, 0.65, 1.0]
    } else if local_team != 0 && local_team == target_team {
        [0.6, 0.8, 0.6, 1.0]
    } else {
        [0.75, 0.25, 0.25, 1.0]
    }
}

// Player-supplied formatting must not override friend/enemy identification.
fn plain_name(name: &str) -> String {
    let mut chars = name.chars().peekable();
    let mut plain = String::new();
    while let Some(c) = chars.next() {
        if c == '^' && chars.peek().is_some_and(char::is_ascii_digit) {
            chars.next();
        } else if !c.is_control() {
            // Removing an escape can join another caret and digit ("^^12").
            if c.is_ascii_digit() && plain.ends_with('^') {
                plain.pop();
            } else {
                plain.push(c);
            }
        }
    }
    plain
}

// Intersect the view ray with the presented player's bounds, including crouch/prone.
fn aimed_distance(eye: Vec3, forward: Vec3, origin: Vec3, head: Vec3) -> Option<f32> {
    let mins = origin.min(head) - Vec3::new(15.0, 15.0, 0.0);
    let maxs = origin.max(head) + Vec3::new(15.0, 15.0, 8.0);
    let mut near: f32 = 0.0;
    let mut far = CROSSHAIR_SCAN_DISTANCE;
    for axis in 0..3 {
        if forward[axis].abs() < 1e-6 {
            if eye[axis] < mins[axis] || eye[axis] > maxs[axis] {
                return None;
            }
        } else {
            let a = (mins[axis] - eye[axis]) / forward[axis];
            let b = (maxs[axis] - eye[axis]) / forward[axis];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
        }
    }
    (near <= far).then_some(near)
}

#[allow(clippy::too_many_arguments)]
fn update_overhead_names(
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    posed: Res<OverheadPosedPlayerFrame>,
    posed_models: Res<OverheadPosedModelFrame>,
    players: Query<(&CEntity, &CEntityRuntime)>,
    cg_clock: Res<FrameClock>,
    surface: Res<crate::surface::Hud2dSurface>,
    catalog: Option<Res<MenuCatalog>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    prediction: Option<Res<net::ClientPredictionState>>,
    view: Res<frame::ViewSubject>,
    effects: Res<frame::ScreenEffectsView>,
    mut pass: ResMut<HudTessPass>,
    mut memory: Local<NameMemory>,
    mut target_settings: Local<TargetBoxSettings>,
    mut gaps: ResMut<HudPresentationGaps>,
) {
    pass.overhead_names = TessJob::Hide;
    gaps.clear(HudGap::OverheadNames);
    let now = cg_clock.time();
    if memory.last_time.is_some_and(|last| now < last) {
        memory.seen.clear();
    }
    memory.last_time = Some(now);
    let Some(snapshot) = presented.snapshot() else {
        *target_settings = TargetBoxSettings::default();
        memory.seen.clear();
        return;
    };
    target_settings.update(local.0, snapshot.meta.script_dvars(local.0));
    if !surface.is_ready() {
        memory.seen.clear();
        return;
    }
    let Some(ps) = presented.player(local.0) else {
        memory.seen.clear();
        return;
    };
    if ps.other_flags & 0x10 != 0 {
        memory.seen.clear();
        let dvars = snapshot.meta.script_dvars(local.0);
        if matches!(ps.pm_type, 5 | 6)
            || (view.in_killcam() && (!effects.ready || effects.default_killcam_view))
            || dvars.int("cg_draw2D") == Some(0)
            || dvars.int("net_showprofile").is_some_and(|value| value != 0)
        {
            return;
        }
        let Some((camera, transform)) = cameras.iter().find(|(c, _)| c.is_active) else {
            return;
        };
        let mut quads = thermal_target_quads(
            snapshot,
            local.0,
            now,
            &surface,
            (camera, transform),
            &players,
            &target_settings,
        );
        quads.extend(vehicle_target_quads(
            snapshot,
            local.0,
            &surface,
            (camera, transform),
            &players,
            &posed_models,
            &target_settings,
        ));
        if !quads.is_empty() {
            pass.overhead_names = TessJob::Quads(quads);
        }
        return;
    }
    if view.in_killcam() || presented.alive_player(local.0).is_none() {
        memory.seen.clear();
        return;
    }
    let Some(local_meta) = snapshot.meta.for_client(local.0) else {
        return;
    };
    if is_flashbanged(
        now,
        ps.shellshock_time,
        ps.shellshock_duration,
        presented
            .shellshock(local.0)
            .map_or(SCREEN_BLEND_BLURRED, |shock| shock.screen_type),
    ) != 0
    {
        memory.seen.clear();
        return;
    }
    let Some((camera, transform)) = cameras.iter().find(|(c, _)| c.is_active) else {
        return;
    };
    let Some(world) = prediction
        .as_ref()
        .filter(|p| p.0.is_armed() && p.0.world().has_world_clip())
        .map(|p| p.0.world())
    else {
        gaps.raise(GapCause::OverheadVisibilityUnavailable);
        return;
    };
    let font_name = ui_get_font_handle(2, surface.scale_virtual_to_real()[1], 1.0);
    let Some(font) = catalog.as_ref().and_then(|c| c.font(font_name)) else {
        gaps.raise(GapCause::OverheadFontUnavailable);
        return;
    };
    let eye = transform.translation();
    let forward = *transform.forward();
    let mut candidates = Vec::new();
    for (identity, runtime) in &players {
        if !runtime.in_next_snap()
            || runtime.next_state.e_type != entity_iw4::ET_PLAYER
            || (runtime.next_state.e_flags & (0x20 | 0x20000)) != 0
        {
            continue;
        }
        let Some(client) = identity.client() else {
            continue;
        };
        if client == local.0 {
            continue;
        }
        let Some(meta) = snapshot.meta.for_client(client) else {
            continue;
        };
        if meta.lifecycle != sim::ClientLifecycle::Alive {
            continue;
        }
        let enemy = local_meta.client_state_team == 0
            || local_meta.client_state_team != meta.client_state_team;
        if enemy
            && snapshot
                .players
                .iter()
                .any(|(id, ps)| *id == client && ps.perks[1] & playerstate_iw4::PERK1_SPYGAME != 0)
        {
            continue;
        }
        let Some(name) = entity_iw4::client_state_name(&meta.name) else {
            continue;
        };
        let origin = Vec3::from_array(runtime.origin);
        let head = match posed.get(identity.number()) {
            Some(OverheadPosedHead::ExactWorld(head)) => Vec3::from_array(head),
            Some(OverheadPosedHead::NoDObjOrHead) => {
                origin + Vec3::Z * (OVERHEAD_ORIGIN_FALLBACK_Z - OVERHEAD_HEAD_LIFT)
            }
            None => {
                gaps.raise(GapCause::OverheadHeadUnavailable {
                    entnum: identity.number(),
                });
                continue;
            }
        };
        if eye.distance_squared(head) > OVERHEAD_MAX_DISTANCE_DEFAULT.powi(2) {
            continue;
        }
        let anchor = head + Vec3::Z * OVERHEAD_HEAD_LIFT;
        let Ok(pixel) = camera.world_to_viewport(transform, anchor) else {
            continue;
        };
        if !pixel.is_finite()
            || pixel.x < 0.0
            || pixel.y < 0.0
            || pixel.x > surface.width()
            || pixel.y > surface.height()
        {
            continue;
        }
        let hit = world.trace_world(
            eye.to_array(),
            head.to_array(),
            [0.0; 3],
            [0.0; 3],
            OVERHEAD_TRACE_MASK,
        );
        let visible = hit.fraction >= 1.0 && hit.startsolid == 0;
        let aim = aimed_distance(eye, forward, origin, head).filter(|_| visible);
        candidates.push((
            identity.number(),
            meta.client_state_team,
            plain_name(name),
            anchor,
            pixel,
            visible,
            aim,
            (meta.rank, meta.prestige, client),
        ));
    }
    let aimed = candidates
        .iter()
        .filter_map(|c| c.6.map(|d| (c.0, d)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|c| c.0);
    memory
        .seen
        .retain(|ent, _| candidates.iter().any(|c| c.0 == *ent));
    let mut list = Draw2dList::default();
    for (ent, team, name, anchor, pixel, visible, _, (rank, prestige, client)) in candidates {
        let friendly = local_meta.client_state_team != 0 && local_meta.client_state_team == team;
        let active = visible && (friendly || aimed == Some(ent));
        if active {
            let seen = memory.seen.entry(ent).or_insert((now, now, friendly));
            if seen.2 != friendly {
                *seen = (now, now, friendly);
            }
            seen.1 = now;
        }
        let Some(&(start, last, was_friendly)) = memory.seen.get(&ent) else {
            continue;
        };
        if was_friendly != friendly {
            memory.seen.remove(&ent);
            continue;
        }
        let fade = if friendly {
            FRIENDLY_NAME_FADE_OUT_DEFAULT_MS
        } else {
            ENEMY_NAME_FADE_MS
        };
        if now.wrapping_sub(last) >= fade {
            memory.seen.remove(&ent);
            continue;
        }
        let alpha = overhead_fade_alpha(
            now,
            start,
            last,
            if friendly { 0 } else { ENEMY_NAME_FADE_MS },
            fade,
        );
        if alpha <= 0.0 {
            continue;
        }
        let distance = overhead_distance_scale(
            eye.to_array(),
            anchor.to_array(),
            OVERHEAD_NEAR_DISTANCE_DEFAULT,
            OVERHEAD_FAR_DISTANCE_DEFAULT,
            OVERHEAD_FAR_SCALE_DEFAULT,
        );
        let scale = normalized_text_scale(font.pixel_height, OVERHEAD_NAME_SIZE_DEFAULT * distance);
        let rank_scale =
            normalized_text_scale(font.pixel_height, OVERHEAD_RANK_SIZE_DEFAULT * distance);
        let x = (pixel.x - crate::chrome::text_width(font, &name) as f32 * scale * 0.5).round();
        let mut color = name_color(local_meta.client_state_team, team);
        color[3] = alpha;
        let mut text_runs = vec![(name, x, pixel.y.round(), scale, color)];
        if let Some((icon, level)) = catalog
            .as_ref()
            .and_then(|c| rank_presentation(c, rank, prestige))
        {
            let text_size = font.pixel_height as f32 * scale;
            let icon_size = OVERHEAD_ICON_SIZE_DEFAULT * text_size;
            let level_width = crate::chrome::text_width(font, level) as f32 * rank_scale;
            let icon_x = x - level_width - icon_size - 2.0 * distance;
            list.cmds.push(Draw2dCmd {
                x: icon_x,
                y: pixel.y.round() - (icon_size + text_size) * 0.5,
                w: icon_size,
                h: icon_size,
                s0: 0.0,
                t0: 0.0,
                s1: 1.0,
                t1: 1.0,
                color: [1.0, 1.0, 1.0, alpha],
                material: icon.to_owned(),
                material_namespace: crate::images::HUD_CHROME_NAMESPACE,
                op: Draw2dOp::StretchPic,
                provenance: Draw2dProvenance::CgDraw {
                    site: "overhead_names",
                },
                layer: 1,
            });
            text_runs.push((
                level.to_owned(),
                icon_x + icon_size,
                pixel.y.round() + font.pixel_height as f32 * rank_scale * 0.25,
                rank_scale,
                [1.0, 1.0, 1.0, alpha],
            ));
        } else {
            gaps.raise(GapCause::OverheadRankUnavailable { client: client.0 });
        }
        for (text, x, y, scale, color) in text_runs {
            list.cmds.push(Draw2dCmd {
                x,
                y,
                w: scale,
                h: scale,
                s0: 0.0,
                t0: 0.0,
                s1: 1.0,
                t1: 1.0,
                color,
                material: asset_core::AssetRef::bare_name(&font.material).to_owned(),
                material_namespace: crate::images::HUD_CHROME_NAMESPACE,
                op: Draw2dOp::TextRun {
                    font: font_name.to_owned(),
                    scale,
                    text,
                    loc_key: String::new(),
                    style: 3,
                    fx: None,
                    glow: None,
                },
                provenance: Draw2dProvenance::CgDraw {
                    site: "overhead_names",
                },
                layer: 1,
            });
        }
    }
    let fonts = HashMap::from([(font_name.to_owned(), font)]);
    let (quads, _) = tessellate_fonts(&list, &fonts);
    if !quads.is_empty() {
        pass.overhead_names = TessJob::Quads(quads);
    }
}

pub(crate) fn rank_presentation(
    catalog: &MenuCatalog,
    rank: i32,
    prestige: i32,
) -> Option<(&str, &str)> {
    if rank < 0 || prestige < 0 {
        return None;
    }
    let key = rank.to_string();
    let icon = catalog
        .string_table("mp/rankIconTable.csv")?
        .lookup_col(&key, prestige.checked_add(1)?);
    let level = catalog
        .string_table("mp/rankTable.csv")?
        .lookup_col(&key, 14);
    (!icon.is_empty() && !level.is_empty()).then_some((icon, level))
}

fn thermal_target_quads(
    snapshot: &sim::Snapshot,
    local: sim::ClientId,
    now: i32,
    surface: &crate::surface::Hud2dSurface,
    view: (&Camera, &GlobalTransform),
    players: &Query<(&CEntity, &CEntityRuntime)>,
    settings: &TargetBoxSettings,
) -> Vec<crate::draw2d::Draw2dQuad> {
    let Some(local_meta) = snapshot.meta.for_client(local) else {
        return Vec::new();
    };
    let (camera, transform) = view;
    let scale = settings.scale;
    let virtual_scale = surface.scale_virtual_to_real()[0];
    let min_size = settings.min_size * virtual_scale;
    let delay = (settings.delay_seconds * 1000.0) as i32;
    let fade = settings.fade_seconds * 1000.0;
    let mut quads = Vec::new();
    for (identity, runtime) in players {
        if !runtime.in_next_snap() || runtime.next_state.e_type != entity_iw4::ET_PLAYER {
            continue;
        }
        let Some(client) = identity.client() else {
            continue;
        };
        let Some(meta) = snapshot.meta.for_client(client) else {
            continue;
        };
        let Some((_, ps)) = snapshot.players.iter().find(|(id, _)| *id == client) else {
            continue;
        };
        if meta.lifecycle != sim::ClientLifecycle::Alive {
            continue;
        }
        let own = client == local;
        if own {
            if ps.other_flags & 0x20 != 0 {
                continue;
            }
        } else if (local_meta.client_state_team != 0
            && local_meta.client_state_team == meta.client_state_team)
            || ps.perks[0] & playerstate_iw4::PERK_COLDBLOODED != 0
        {
            continue;
        }
        let alpha = if own {
            1.0
        } else {
            let age = now.wrapping_sub(meta.item_use_spawn_ms);
            if age <= delay {
                continue;
            }
            if fade > 0.0 {
                (age.wrapping_sub(delay) as f32 / fade).min(1.0)
            } else {
                1.0
            }
        };
        let origin = Vec3::from_array(runtime.origin);
        let top = origin + Vec3::Z * 60.0;
        let (Ok(base), Ok(head)) = (
            camera.world_to_viewport(transform, origin),
            camera.world_to_viewport(transform, top),
        ) else {
            continue;
        };
        let center = (base + head) * 0.5;
        if !center.is_finite() {
            continue;
        }
        let size = (base.distance(head) * scale).max(min_size);
        let half = size * 0.5;
        let half_height = half * surface.display_pixel_aspect();
        quads.push(crate::draw2d::Draw2dQuad {
            xy: [
                [center.x - half, center.y - half_height],
                [center.x + half, center.y - half_height],
                [center.x + half, center.y + half_height],
                [center.x - half, center.y + half_height],
            ],
            st: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            color: [1.0, 1.0, 1.0, alpha],
            material: if own {
                "hud_fofbox_self"
            } else {
                "hud_fofbox_hostile"
            }
            .into(),
            material_namespace: crate::images::HUD_CHROME_NAMESPACE,
            provenance: Draw2dProvenance::CgDraw {
                site: "thermal_targets",
            },
            layer: 1,
            clip: None,
        });
    }
    quads
}

fn vehicle_target_quads(
    snapshot: &sim::Snapshot,
    local: sim::ClientId,
    surface: &crate::surface::Hud2dSurface,
    view: (&Camera, &GlobalTransform),
    entities: &Query<(&CEntity, &CEntityRuntime)>,
    posed: &OverheadPosedModelFrame,
    settings: &TargetBoxSettings,
) -> Vec<crate::draw2d::Draw2dQuad> {
    let Some(local_meta) = snapshot.meta.for_client(local) else {
        return Vec::new();
    };
    let (camera, transform) = view;
    let mut quads = Vec::new();
    for target in &snapshot.meta.objectives.vehicle_targets {
        if target.owner == local
            || (local_meta.client_state_team != 0
                && snapshot
                    .meta
                    .for_client(target.owner)
                    .is_some_and(|owner| owner.client_state_team == local_meta.client_state_team))
        {
            continue;
        }
        if !posed.models.contains(&(target.model, target.entity)) {
            continue;
        }
        let Some((_, runtime)) = entities
            .iter()
            .find(|(identity, _)| identity.number() == target.entity)
        else {
            continue;
        };
        if !runtime.in_next_snap()
            || runtime.next_state.e_type != entity_iw4::ET_SCRIPTMOVER
            || runtime.next_state.e_flags & entity_iw4::CG_SCRIPT_MOVER_NODRAW != 0
        {
            continue;
        }
        let origin = Vec3::from_array(runtime.origin);
        let (Ok(a), Ok(b)) = (
            camera.world_to_viewport(transform, origin + Vec3::new(-60.0, -60.0, -160.0)),
            camera.world_to_viewport(transform, origin + Vec3::new(60.0, 60.0, -40.0)),
        ) else {
            continue;
        };
        let center = (a + b) * 0.5;
        if !center.is_finite() {
            continue;
        }
        let half = (a.distance(b) * settings.scale)
            .max(settings.min_size * surface.scale_virtual_to_real()[0])
            * 0.5;
        let height = half * surface.display_pixel_aspect();
        quads.push(crate::draw2d::Draw2dQuad {
            xy: [
                [center.x - half, center.y - height],
                [center.x + half, center.y - height],
                [center.x + half, center.y + height],
                [center.x - half, center.y + height],
            ],
            st: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            color: [1.0; 4],
            material: "hud_fofbox_hostile".into(),
            material_namespace: crate::images::HUD_CHROME_NAMESPACE,
            provenance: Draw2dProvenance::CgDraw {
                site: "thermal_vehicle_targets",
            },
            layer: 1,
            clip: None,
        });
    }
    quads
}
