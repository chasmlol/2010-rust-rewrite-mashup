//! The Blaster (the yellow gun's first form): drawing it, the fire timing
//! and the shot it fires.
use glam::Vec3;

use crate::control::Control;
use crate::math::*;
use crate::pad::{Pad, button};
use crate::projectile::Projectile;
use crate::{Event, Jak, State};

/// Ticks between shots.
pub const FIRE_DELAY: i64 = 96;
pub const AMMO_MAX: f32 = 100.0;
pub const AMMO_PER_SHOT: f32 = 1.0;
/// How far ahead an auto-aimed target may be.
pub const TRACK_FIND_RANGE: f32 = 286720.0;
pub const FIRE_RANGE: f32 = 409600.0;
/// Where the muzzle sits relative to Jak's origin while he holds the gun:
/// up, then ahead.
pub const MUZZLE_UP: f32 = meters(1.2);
pub const MUZZLE_AHEAD: f32 = meters(0.6);

#[derive(Clone, Debug)]
pub struct Gun {
    pub out: bool,
    pub latch: bool,
    pub get_on_time: i64,
    gun_time: i64,
    pub fire_time: i64,
    pub fire_pending: i32,
    pub fire_pending_time: i64,
    pub active: bool,
    pub active_time: i64,
    pub ammo: f32,
    pub endless_ammo: bool,
    pub turn_blend: f32,
    /// A point the host's auto-aim picked, in Jak space; the gun fires at it
    /// instead of straight ahead.
    pub aim: Option<Vec3>,
    pub fire_point: Vec3,
    pub fire_dir: Vec3,
}

const LONG_AGO: i64 = i64::MIN / 4;

impl Default for Gun {
    fn default() -> Self {
        Self {
            out: false,
            latch: false,
            get_on_time: LONG_AGO,
            gun_time: LONG_AGO,
            fire_time: LONG_AGO,
            fire_pending: 0,
            fire_pending_time: LONG_AGO,
            active: false,
            active_time: LONG_AGO,
            ammo: AMMO_MAX,
            endless_ammo: false,
            turn_blend: 0.0,
            aim: None,
            fire_point: Vec3::ZERO,
            fire_dir: Vec3::Z,
        }
    }
}

impl Gun {
    pub(crate) fn end_mode(&mut self) {
        self.out = false;
        self.active = false;
        self.latch = false;
        self.fire_pending = 0;
    }

    pub(crate) fn time_since_use(&self, now: i64) -> i64 {
        now.saturating_sub(self.gun_time)
    }

    /// Muzzle and aim from Jak's pose.
    fn compute_pos(&mut self, c: &Control) {
        let forward = flatten(z_axis(c.quat), crate::math::y_axis(c.quat)).normalize_or(Vec3::Z);
        self.fire_point = c.trans + Vec3::new(0.0, MUZZLE_UP, 0.0) + forward * MUZZLE_AHEAD;
        self.fire_dir = match self.aim {
            Some(target) if target.distance(self.fire_point) <= TRACK_FIND_RANGE => {
                (target - self.fire_point).normalize_or(forward)
            }
            _ => forward,
        };
    }

    /// One frame of the gun on foot: draw it on R1, queue a shot on each
    /// press, fire the queue as the delay allows. Returns the shot fired.
    pub(crate) fn check(
        &mut self,
        now: i64,
        pad: &Pad,
        c: &Control,
        board_latch: bool,
        events: &mut Vec<Event>,
    ) -> Option<Projectile> {
        let elapsed = |since: i64, d: i64| now - since >= d;
        if !self.out {
            if pad.hold(button::R1) {
                self.latch = true;
            }
            if self.latch && !board_latch && elapsed(self.gun_time, seconds(0.1)) {
                self.out = true;
                self.latch = false;
                self.get_on_time = now;
                self.active = false;
            } else {
                return None;
            }
        }
        if board_latch && elapsed(self.get_on_time, seconds(0.1)) {
            self.end_mode();
            return None;
        }
        self.compute_pos(c);
        self.active = elapsed(self.get_on_time, seconds(0.1));
        if self.active {
            self.active_time = now;
        }
        if !self.active
            && elapsed(self.fire_pending_time, seconds(0.2))
            && elapsed(self.active_time, seconds(0.2))
            && self.fire_pending == 1
        {
            self.fire_pending = 0;
        }
        if pad.pressed(button::R1) && FIRE_DELAY - 30 < now - self.fire_time {
            self.fire_pending_time = now;
            if self.fire_pending == 0 {
                self.fire_pending += 1;
            }
        }
        let mut shot = None;
        if elapsed(self.fire_time, FIRE_DELAY) && self.fire_pending > 0 && self.active {
            self.fire_pending -= 1;
            if self.endless_ammo || self.ammo >= AMMO_PER_SHOT {
                if !self.endless_ammo {
                    self.ammo -= AMMO_PER_SHOT;
                }
                self.fire_time = now;
                events.push(Event::Fire {
                    from: self.fire_point,
                    dir: self.fire_dir,
                });
                shot = Some(Projectile::yellow(self.fire_point, self.fire_dir, now));
            } else {
                self.fire_time = 0;
            }
        }
        self.gun_time = now;
        shot
    }
}

impl Jak {
    /// The gun's part of a frame, after Jak has moved.
    pub(crate) fn gun_frame(&mut self, world: &mut dyn crate::collide::CollideWorld) {
        if self.state.is_board() {
            if self.gun.out {
                self.gun.end_mode();
            }
            return;
        }
        if matches!(self.state, State::BoardGetOff) {
            return;
        }
        if let Some(mut shot) = self.gun.check(
            self.time,
            &self.pad,
            &self.control,
            self.board.latch,
            &mut self.events,
        ) {
            if let Some(hit) = shot.point_blank(world, &mut self.probe_cache) {
                self.events.push(Event::Impact(hit));
            } else {
                self.projectiles.push(shot);
            }
        }
    }
}
