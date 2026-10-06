use std::collections::HashMap;

use bevy::prelude::*;
use net::{FrameClock, LocalPresentClient, PresentedSnapshot};
use render_anim::{FpvBoltTargets, PreparedFpv};
use render_fx::{FxCodeMeshPlan, FxWorldColorImages};

pub(crate) const MATERIALS: [&str; 4] = [
    "motiontracker3d_bg",
    "motiontracker3d_sweep",
    "motiontracker3d_ping_enemy_mp",
    "motiontracker3d_ping_friendly_mp",
];

const RANGE: f32 = 1600.0;
const SWEEP_MS: i32 = 3000;
const SWEEP_SPEED: f32 = 2000.0;
const FADE_MS: i32 = 3000;
const CENTER_Y: f32 = -0.37;
const PING_SIZE: f32 = 0.2;

#[derive(Clone, Copy)]
struct Ping {
    distance: f32,
    yaw: f32,
    start_angle: f32,
    end_angle: f32,
    time: i32,
    enemy: bool,
}

#[derive(Resource, Default)]
pub(crate) struct MotionTracker {
    owner: Option<(u64, sim::ClientId, sim::LifeSequence)>,
    last_time: Option<i32>,
    previous_origin: Vec2,
    phase_ms: i32,
    active: bool,
    ping_played: bool,
    changed_at: i32,
    contacts: HashMap<sim::ClientId, [Option<Ping>; 2]>,
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<MotionTracker>();
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_motion_tracker(
    mut tracker: ResMut<MotionTracker>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    clock: Res<FrameClock>,
    mut sound: MessageWriter<audio::AliasCommand>,
    prepared: Res<PreparedFpv>,
    bolts: Res<FpvBoltTargets>,
    colors: Res<FxWorldColorImages>,
    mut plan: ResMut<FxCodeMeshPlan>,
) {
    let Some(snapshot) = presented.snapshot() else {
        *tracker = MotionTracker::default();
        return;
    };
    let Some(ps) = presented.player(local.0) else {
        *tracker = MotionTracker::default();
        return;
    };
    let Some(meta) = snapshot
        .meta
        .clients
        .iter()
        .find(|(id, _)| *id == local.0)
        .map(|(_, m)| m)
    else {
        return;
    };
    let Some(table) = prepared.table() else {
        *tracker = MotionTracker::default();
        return;
    };
    let now = clock.time();
    let owner = (table.catalog_id(), local.0, meta.life_sequence);
    if tracker.owner != Some(owner) || tracker.last_time.is_some_and(|t| now < t) {
        *tracker = MotionTracker {
            owner: Some(owner),
            ..Default::default()
        };
    }
    let weapon = weapon_iw4::get_viewmodel_weapon_index(ps);
    let active = table.motion_tracker(weapon, ps.weapon_primary)
        && ps.other_flags & (1 << 10) == 0
        && meta.lifecycle == sim::ClientLifecycle::Alive;
    let origin = Vec2::new(ps.origin[0], ps.origin[1]);
    let dt = tracker.last_time.map_or(0, |t| now.saturating_sub(t));
    tracker.last_time = Some(now);
    if active != tracker.active {
        tracker.active = active;
        tracker.changed_at = now;
        if active {
            tracker.phase_ms = 0;
            tracker.ping_played = false;
            tracker.previous_origin = origin;
        }
    }
    let mut play = |alias: &str, pitch: f32| {
        sound.write(audio::AliasCommand::PlayPitched {
            sound: audio::PlayAlias {
                event: None,
                namespace: asset_core::AssetNamespace::Iw4,
                alias: alias.to_owned(),
                fallback: None,
                origin_inches: None,
                snd_ent: Some(audio::SND_ENT_LOCAL),
            },
            pitch,
        });
    };
    if dt > 0 && (active || now - tracker.changed_at < 500) {
        let view_yaw = ps.viewangles[1].to_radians();
        for history in tracker.contacts.values_mut() {
            if let Some(ping) = &mut history[0] {
                let current = (ping.yaw - view_yaw).rem_euclid(std::f32::consts::TAU);
                let span = (ping.end_angle - ping.start_angle).rem_euclid(std::f32::consts::TAU);
                if span <= (current - ping.start_angle).rem_euclid(std::f32::consts::TAU) {
                    let backwards = (ping.start_angle - current).rem_euclid(std::f32::consts::TAU);
                    let forwards = (current - ping.end_angle).rem_euclid(std::f32::consts::TAU);
                    if forwards <= backwards {
                        ping.end_angle += forwards;
                    } else {
                        ping.start_angle -= backwards;
                    }
                }
            }
        }
        let previous = tracker.phase_ms;
        let elapsed = previous.saturating_add(dt);
        tracker.phase_ms = elapsed.rem_euclid(SWEEP_MS);
        if elapsed >= SWEEP_MS {
            tracker.ping_played = false;
            if active {
                play("motiontracker_ping", 1.0);
            } else {
                tracker.phase_ms = 0;
            }
        }
        let radius = tracker.phase_ms as f32 * SWEEP_SPEED / 1000.0;
        let previous_radius = previous as f32 * SWEEP_SPEED / 1000.0;
        let yaw = ps.viewangles[1].to_radians();
        let forward = Vec2::new(yaw.cos(), yaw.sin());
        for (id, state) in snapshot
            .players
            .iter()
            .filter(|_| active || elapsed < SWEEP_MS)
        {
            if now.saturating_sub(sim::level_time_ms(snapshot.tick)) > 500
                || *id == local.0
                || state.health <= 0
                || state.perks[0] & (1 << 28) != 0
            {
                continue;
            }
            let Some(actor) = snapshot
                .meta
                .clients
                .iter()
                .find(|(c, _)| c == id)
                .map(|(_, m)| m)
            else {
                continue;
            };
            if actor.lifecycle != sim::ClientLifecycle::Alive || actor.client_state_team == 3 {
                continue;
            }
            let position = Vec2::new(state.origin[0], state.origin[1]);
            let delta = position - origin;
            if delta.length_squared() > radius * radius
                || (elapsed < SWEEP_MS
                    && (position - tracker.previous_origin).length_squared()
                        <= previous_radius * previous_radius)
                || delta.dot(forward) < 0.0
            {
                continue;
            }
            let ping = Ping {
                distance: delta.length(),
                yaw: delta.y.atan2(delta.x),
                start_angle: (delta.y.atan2(delta.x) - yaw).rem_euclid(std::f32::consts::TAU),
                end_angle: (delta.y.atan2(delta.x) - yaw).rem_euclid(std::f32::consts::TAU),
                time: now,
                enemy: !matches!(meta.client_state_team, 1 | 2)
                    || meta.client_state_team != actor.client_state_team,
            };
            if ping.enemy && !tracker.ping_played {
                if let Some([tl, bl, br]) = bolts.tracker_screen {
                    let aspect = (br - bl).length() / (tl - bl).length();
                    let angle = ping.yaw - yaw;
                    let x = -angle.sin() * ping.distance / RANGE / aspect;
                    let y = angle.cos() * ping.distance / RANGE + CENTER_Y;
                    if x.abs() <= 0.5 && y.abs() <= 0.5 {
                        let max_distance = RANGE * Vec2::new(0.5 * aspect, 0.5 - CENTER_Y).length();
                        play("motiontracker_pong", 2.0 - ping.distance / max_distance);
                        tracker.ping_played = true;
                    }
                }
            }
            let history = tracker.contacts.entry(*id).or_insert([None, None]);
            history[1] = history[0];
            history[0] = Some(ping);
        }
        tracker.previous_origin = origin;
    }
    tracker
        .contacts
        .retain(|_, history| history.iter().flatten().any(|p| now - p.time < FADE_MS));
    let fade = ((now - tracker.changed_at) as f32 / 500.0).clamp(0.0, 1.0);
    let alpha = if active { fade } else { 1.0 - fade };
    if alpha <= 0.0 {
        return;
    }
    let Some([tl, bl, br]) = bolts.tracker_screen else {
        return;
    };
    let right = br - bl;
    let up = tl - bl;
    let width = right.length();
    let height = up.length();
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let right = right / width;
    let up = up / height;
    let normal = right.cross(up).normalize_or_zero();
    let rotation = Quat::from_mat3(&Mat3::from_cols(right, up, normal));
    let center = (tl + br) * 0.5;
    let mut quad = |material: usize, offset: Vec2, size: Vec2, opacity: f32| {
        let Some(binding) = &colors.motion_tracker[material] else {
            return;
        };
        let slot = plan.begin_material_draw(
            binding.color.clone(),
            binding.sort_key,
            binding.material_sorted_index,
        );
        plan.draws[slot as usize].viewmodel = true;
        plan.push_quad(
            Transform {
                translation: center + right * offset.x + up * offset.y,
                rotation,
                scale: Vec3::new(size.x, size.y, 1.0),
            },
            [255, 255, 255, (opacity.clamp(0.0, 1.0) * 255.0) as u8],
            fx_iw4::FxSpriteAtlasUv::FULL,
        );
        plan.end_material_draw(slot);
    };
    quad(0, Vec2::ZERO, Vec2::new(width, height), alpha);
    let visible = tracker
        .contacts
        .values()
        .flatten()
        .flatten()
        .filter(|ping| (0..FADE_MS).contains(&(now - ping.time)))
        .count();
    let max_points = if visible == 0 {
        32
    } else {
        (634 / visible).min(32).max(1)
    };
    for history in tracker.contacts.values() {
        for ping in history.iter().flatten() {
            let age = now - ping.time;
            if !(0..FADE_MS).contains(&age) {
                continue;
            }
            let span = (ping.end_angle - ping.start_angle).rem_euclid(std::f32::consts::TAU);
            let length = ping.distance / RANGE * span;
            let mut points = (length / (PING_SIZE * 0.25) + 0.5).floor().max(1.0) as usize;
            if length > 0.0 {
                points = points.max(3);
            }
            points = points.min(max_points);
            let edge = if length < 0.15 { length * 2.0 } else { 0.3 };
            let opacity =
                (alpha * (1.0 - age as f32 / FADE_MS as f32) * (age as f32 / 100.0).min(1.0)
                    / points as f32
                    / (1.0 - edge))
                    .min(1.0);
            for point in 0..points {
                let fraction = (point as f32 + 0.5) / points as f32;
                let angle = ping.start_angle + fraction * span;
                let position = Vec2::new(
                    -angle.sin() * ping.distance / RANGE / (width / height),
                    angle.cos() * ping.distance / RANGE + CENTER_Y,
                );
                if position.x.abs() > 0.5 + PING_SIZE || position.y.abs() > 0.5 + PING_SIZE {
                    continue;
                }
                let taper = if fraction < edge {
                    fraction / edge
                } else if fraction > 1.0 - edge {
                    (1.0 - fraction) / edge
                } else {
                    1.0
                };
                quad(
                    if ping.enemy { 2 } else { 3 },
                    position * Vec2::new(width, height),
                    Vec2::splat(PING_SIZE * height),
                    opacity * taper,
                );
            }
        }
    }
    let phase = tracker.phase_ms as f32 / SWEEP_MS as f32;
    if phase < 0.25 {
        let prior = SWEEP_SPEED * (phase + 1.0) * SWEEP_MS as f32 / 1000.0 / RANGE;
        quad(
            1,
            Vec2::new(0.0, (CENTER_Y + 0.5 * prior) * height),
            Vec2::new(2.0 * prior * height, prior * height),
            alpha * (1.0 - 4.0 * phase),
        );
    }
    let sweep_range = SWEEP_SPEED * phase * SWEEP_MS as f32 / 1000.0 / RANGE;
    quad(
        1,
        Vec2::new(0.0, (CENTER_Y + 0.5 * sweep_range) * height),
        Vec2::new(2.0 * sweep_range * height, sweep_range * height),
        alpha,
    );
    plan.bump();
    plan.publish_share();
}
