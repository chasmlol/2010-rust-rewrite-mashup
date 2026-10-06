use asset_world::ClipCollision;
use assets::PreparedWeapons;
use bevy::prelude::*;
use entity_iw4::{EntityState, Trajectory, evaluate_trajectory};
use net::PresentedSnapshot;
use playerstate_iw4::{ENTITYNUM_NONE, KillCamMode};
use render_scene::WorldCameraPose;
use sim::ClientId;

#[derive(Default)]
pub struct KillcamCamera {
    entity: Option<i32>,
    mode: Option<KillCamMode>,
    origin: Vec3,
    angles: [f32; 3],
    target: Vec3,
    rest_ground: bool,
    previous_origin: Option<(i32, Vec3)>,
    stop: Option<(i32, f32, Vec3, Vec3)>,
    entered_at: i32,
    last_time: i32,
    last_pose: Option<WorldCameraPose>,
    blend_until: i32,
}

fn entity_origin(es: &EntityState, now: i32) -> Vec3 {
    Vec3::from_array(evaluate_trajectory(
        &Trajectory {
            tr_time: es.tr_time,
            tr_type: es.tr_type,
            tr_duration: es.tr_duration,
            tr_base: es.tr_base,
            tr_delta: es.tr_delta,
        },
        now,
    ))
}

fn look_angles(direction: Vec3) -> [f32; 3] {
    let forward = direction.normalize_or_zero();
    let left = Vec3::new(-forward.y, forward.x, 0.0)
        .try_normalize()
        .unwrap_or(Vec3::X);
    math_iw4::axis_to_angles([
        forward.to_array(),
        left.to_array(),
        forward.cross(left).to_array(),
    ])
}

fn pull_back(clip: Option<&ClipCollision>, mut start: Vec3, mut end: Vec3) -> Vec3 {
    let Some(clip) = clip else {
        return end;
    };
    let trace =
        |a: Vec3, b: Vec3| clip.sweep_box(a.to_array(), b.to_array(), [-5.0; 3], [5.0; 3], 0x811);
    let mut hit = trace(start, end);
    for _ in 0..3 {
        if !hit.startsolid {
            break;
        }
        start += if hit.fraction == 1.0 {
            Vec3::Z * 20.0
        } else {
            Vec3::from_array(hit.normal) * 20.0
        };
        hit = trace(start, end);
    }
    if hit.startsolid {
        return start;
    }
    for _ in 0..3 {
        if hit.fraction >= 1.0 {
            return end;
        }
        start = start.lerp(end, hit.fraction);
        let normal = Vec3::from_array(hit.normal);
        let remaining = end - start;
        end = start + remaining + normal * (0.1 - remaining.dot(normal));
        hit = trace(start, end);
    }
    start.lerp(end, hit.fraction)
}

fn look_at_both(target: Vec3, entity: Vec3, camera: Vec3) -> Vec3 {
    let first = (target - camera).normalize_or_zero();
    let second = (entity - camera).normalize_or_zero();
    let mut direction = (first + second).normalize_or_zero();
    let dot = direction.dot(first);
    if dot < 0.9848077893257141 {
        direction = if dot < 0.8660253882408142 {
            first
        } else {
            let axis = first.cross(second).normalize_or_zero();
            let rotated = Quat::from_axis_angle(axis, 10.0_f32.to_radians()) * first;
            rotated.lerp(
                first,
                (dot - 0.9848077893257141) / (0.8660253882408142 - 0.9848077893257141),
            )
        };
    }
    if direction.z < -0.949999988079071 {
        direction = if direction.z < -0.9800000190734863 {
            first
        } else {
            first.lerp(
                direction,
                (direction.z + 0.9800000190734863) / 0.030000030994415283,
            )
        };
    }
    direction.normalize_or_zero()
}

pub(super) fn classify_view(
    presented: &PresentedSnapshot,
    viewer: ClientId,
    in_killcam: bool,
    weapons: Option<&PreparedWeapons>,
) -> Option<KillCamMode> {
    if !in_killcam {
        return Some(KillCamMode::Mode0);
    }
    let ps = presented.player(viewer)?;
    if ps.kill_cam_entity == ENTITYNUM_NONE {
        return Some(KillCamMode::Mode0);
    }
    let snapshot = presented.snapshot()?;
    let number = ps.kill_cam_entity;
    let entity = snapshot.meta.entities.iter().find(|e| e.number == number);
    let projectile = snapshot.projectiles.iter().find(|p| p.entnum == number);
    let mode = if let Some(p) = projectile {
        let facts = weapons.and_then(|w| {
            w.snapshot_weapon(presented.weapon_epoch(), p.weapon)
                .ok()
                .and_then(|weapon| weapon.event_facts())
        });
        match facts
            .map(|f| f.projectile_camera())
            .unwrap_or(asset_game::ProjectileCameraPolicy::Missile)
        {
            asset_game::ProjectileCameraPolicy::TopAttack => KillCamMode::Mode7Javelin,
            asset_game::ProjectileCameraPolicy::Remote => KillCamMode::Mode8Remote,
            asset_game::ProjectileCameraPolicy::Rocket => KillCamMode::Mode5Rocket,
            asset_game::ProjectileCameraPolicy::MissileAlternate => KillCamMode::Mode4MissileAlt,
            asset_game::ProjectileCameraPolicy::Missile => KillCamMode::Mode3Missile,
        }
    } else {
        match entity?.e_type {
            12 => KillCamMode::Mode1Heli,
            11 => KillCamMode::Mode6Turret,
            _ => KillCamMode::Mode2Airstrike,
        }
    };
    Some(mode)
}

impl KillcamCamera {
    pub fn update(
        &mut self,
        presented: &PresentedSnapshot,
        viewer: ClientId,
        now: i32,
        in_killcam: bool,
        weapons: Option<&PreparedWeapons>,
        clip: Option<&ClipCollision>,
    ) -> Option<(WorldCameraPose, f32, Option<f32>)> {
        let ps = presented.player(viewer)?;
        if !in_killcam || now < self.last_time {
            *self = Self::default();
        }
        let previous_time = self.last_time;
        self.last_time = now;
        if !in_killcam {
            return None;
        }
        let snapshot = presented.snapshot()?;
        let trajectory_time = presented.trajectory_time_ms(now);
        let number = ps.kill_cam_entity;
        if number == ENTITYNUM_NONE {
            self.entity = None;
            self.mode = None;
            self.stop = None;
            self.previous_origin = None;
            self.last_pose = Some(WorldCameraPose {
                origin: [
                    ps.origin[0],
                    ps.origin[1],
                    ps.origin[2] + ps.view_height_current,
                ],
                angles: ps.viewangles,
            });
            return None;
        }
        let entity = snapshot.meta.entities.iter().find(|e| e.number == number);
        let projectile = snapshot.projectiles.iter().find(|p| p.entnum == number);
        if self.entity != Some(number) {
            let mode = classify_view(presented, viewer, in_killcam, weapons)?;
            self.rest_ground = false;
            self.entity = Some(number);
            self.mode = Some(mode);
            self.stop = None;
            self.previous_origin = None;
            self.entered_at = now;
            self.blend_until = playerstate_iw4::killcam_lerp_deadline_ms(
                self.last_pose.is_none(),
                true,
                mode,
                now,
            );
        }
        if let Some(p) = projectile {
            self.origin = Vec3::from_array(presented.projectile_origin_at(p, now));
            self.angles = evaluate_trajectory(&p.apos, trajectory_time);
        } else if let Some(es) = entity {
            self.origin = entity_origin(es, trajectory_time);
            self.angles = evaluate_trajectory(
                &Trajectory {
                    tr_time: es.apos_tr_time,
                    tr_type: es.apos_tr_type,
                    tr_duration: es.apos_tr_duration,
                    tr_base: es.apos_tr_base,
                    tr_delta: es.apos_tr_delta,
                },
                trajectory_time,
            );
        }
        if let Some(p) = projectile {
            self.rest_ground = entity.is_some_and(|e| e.ground_entity_num != ENTITYNUM_NONE)
                && weapons
                    .and_then(|w| {
                        w.snapshot_weapon(presented.weapon_epoch(), p.weapon)
                            .ok()
                            .and_then(|weapon| weapon.event_facts())
                    })
                    .is_some_and(|f| f.rests_on_ground());
        }
        let look_at = if ps.kill_cam_look_at_entity == ENTITYNUM_NONE {
            viewer.0 as i32
        } else {
            ps.kill_cam_look_at_entity
        };
        if let Some(es) = snapshot.meta.entities.iter().find(|e| e.number == look_at) {
            self.target = entity_origin(es, trajectory_time) + Vec3::Z * 60.0;
        } else if look_at != viewer.0 as i32
            && let Some(target) = presented.player(ClientId(look_at as u32))
        {
            self.target = Vec3::from_array(target.origin) + Vec3::Z * target.view_height_current;
        }
        let mode = self.mode?;
        let mut anchor = self.origin;
        if let Some((start, duration, origin, velocity)) = self.stop {
            let elapsed = (now - start).max(0) as f32;
            let t = elapsed.min(duration);
            anchor = origin
                + velocity
                    * if duration > 0.0 {
                        t * (1.0 - 0.5 * t / duration)
                    } else {
                        0.0
                    };
        }
        let (forward, right, up) = math_iw4::angle_vectors(self.angles);
        let forward = Vec3::from_array(forward);
        let up = Vec3::from_array(up);
        let mut toward = self.target - anchor;
        toward.z = toward.z.min(0.0);
        toward = toward.normalize_or_zero();
        let side = Vec3::new(-toward.y, toward.x, 0.0)
            .try_normalize()
            .unwrap_or(Vec3::X);
        let camera_up = toward.cross(side);
        let (desired, angles, fov) = match mode {
            KillCamMode::Mode0 => return None,
            KillCamMode::Mode1Heli => return None,
            KillCamMode::Mode2Airstrike => {
                let origin = anchor.with_z(anchor.z.max(self.target.z + 24.0));
                (origin, look_angles(self.target - origin), 50.0)
            }
            KillCamMode::Mode6Turret => (anchor - forward * 10.0 + up * 10.0, self.angles, 50.0),
            KillCamMode::Mode7Javelin => {
                let age = projectile
                    .map(|p| (trajectory_time - p.launch_time).max(0))
                    .unwrap_or(now - self.entered_at) as f32
                    / 1000.0;
                let origin = if projectile.is_some_and(|p| p.pos.tr_type == entity_iw4::TR_GRAVITY)
                {
                    anchor
                } else if forward.z > 0.0 {
                    let pass = (age * 2.0 * std::f32::consts::PI / 5.0)
                        .min(4.71238899230957)
                        .sin();
                    anchor - forward * (pass * 200.0) + up * (50.0 * (1.0 - pass.abs()))
                } else {
                    anchor + Vec3::Z * (175.0 + 25.0 * forward.z)
                };
                let blend = (1.0 - (anchor.z - self.target.z) / 3000.0).clamp(0.0, 1.0);
                let gravity = projectile.is_some_and(|p| p.pos.tr_type == entity_iw4::TR_GRAVITY);
                let axis_forward = if gravity {
                    forward
                } else {
                    (anchor - origin).normalize_or_zero()
                };
                let left = -Vec3::from_array(right);
                let axis = Mat3::from_cols(axis_forward, left, axis_forward.cross(left));
                let mut rotation = Quat::from_mat3(&axis);
                if !gravity && forward.z <= 0.0 {
                    let target_forward = (self.target - origin).normalize_or_zero();
                    let target_left = Vec3::new(-target_forward.y, target_forward.x, 0.0)
                        .try_normalize()
                        .unwrap_or(Vec3::X);
                    let target_axis = Mat3::from_cols(
                        target_forward,
                        target_left,
                        target_forward.cross(target_left),
                    );
                    rotation = rotation.slerp(Quat::from_mat3(&target_axis), blend);
                }
                let angles = if projectile.is_some_and(|p| p.pos.tr_type == entity_iw4::TR_GRAVITY)
                {
                    self.angles
                } else {
                    math_iw4::axis_to_angles(Mat3::from_quat(rotation).to_cols_array_2d())
                };
                let origin = if !gravity && forward.z <= 0.0 {
                    origin.with_z(origin.z.max(self.target.z + 128.0))
                } else {
                    origin
                };
                (origin, angles, 60.0)
            }
            _ => {
                let (height, back) = if self.rest_ground {
                    if matches!(
                        mode,
                        KillCamMode::Mode3Missile | KillCamMode::Mode4MissileAlt
                    ) {
                        (15.0, 30.0)
                    } else {
                        (0.0, 0.0)
                    }
                } else {
                    match mode {
                        KillCamMode::Mode5Rocket => (10.0, 70.0),
                        KillCamMode::Mode8Remote => (60.0, 300.0),
                        _ => (5.0, 35.0),
                    }
                };
                let origin = pull_back(
                    clip,
                    self.origin,
                    anchor + camera_up * height - toward * back,
                );
                let direction = look_at_both(self.target, self.origin, origin);
                (origin, look_angles(direction), 50.0)
            }
        };
        if self.stop.is_none()
            && matches!(
                mode,
                KillCamMode::Mode4MissileAlt | KillCamMode::Mode5Rocket | KillCamMode::Mode8Remote
            )
            && ((self.origin - self.target).length() < 350.0
                || (mode == KillCamMode::Mode8Remote && self.origin.z - self.target.z < 350.0))
            && let Some((time, previous)) = self.previous_origin.filter(|(time, _)| *time < now)
        {
            let velocity = (self.origin - previous) / (now - time) as f32;
            let speed = velocity.length();
            let fraction = clip
                .map(|c| {
                    c.sweep_box(
                        self.origin.to_array(),
                        (self.origin + velocity.normalize_or_zero() * 100.0).to_array(),
                        [0.0; 3],
                        [0.0; 3],
                        0x811,
                    )
                    .fraction
                })
                .unwrap_or(1.0);
            let duration = if speed > 0.0 {
                200.0 * fraction / speed
            } else {
                0.0
            };
            self.stop = Some((now, duration, self.origin, velocity));
        }
        let mut pose = WorldCameraPose {
            origin: desired.to_array(),
            angles,
        };
        if now < self.blend_until
            && let Some(from) = self.last_pose
        {
            let dt = (now - previous_time).max(0) as f32;
            let t = (dt / (self.blend_until - previous_time).max(1) as f32).clamp(0.0, 1.0);
            let velocity = self
                .previous_origin
                .filter(|(time, _)| *time < now && (projectile.is_some() || entity.is_some()))
                .map(|(time, origin)| (self.origin - origin) / (now - time) as f32)
                .unwrap_or(Vec3::ZERO);
            let mut projected = desired + velocity * (self.blend_until - now) as f32;
            if let Some(clip) = clip {
                let hit = clip.sweep_box(
                    desired.to_array(),
                    projected.to_array(),
                    [-5.0; 3],
                    [5.0; 3],
                    0x811,
                );
                projected = if hit.startsolid {
                    desired
                } else {
                    desired.lerp(projected, hit.fraction)
                };
            }
            pose.origin = Vec3::from_array(from.origin).lerp(projected, t).to_array();
            for (i, angle) in pose.angles.iter_mut().enumerate() {
                *angle = from.angles[i] + math_iw4::angle_subtract(*angle, from.angles[i]) * t;
            }
        }
        self.previous_origin = Some((now, self.origin));
        self.last_pose = Some(pose);
        let focus_distance = matches!(
            mode,
            KillCamMode::Mode2Airstrike
                | KillCamMode::Mode3Missile
                | KillCamMode::Mode4MissileAlt
                | KillCamMode::Mode5Rocket
                | KillCamMode::Mode8Remote
        )
        .then_some((self.target - desired).length());
        Some((pose, fov, focus_distance))
    }
}
