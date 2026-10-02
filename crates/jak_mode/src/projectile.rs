//! Shots in flight. The Blaster's shot flies straight and fast, a small
//! sphere against the world and a large one against creatures, trailing a
//! beam up to 16 m long, and stops at the first thing it touches.
use glam::Vec3;

use crate::collide::{CollideCache, CollideWorld};
use crate::math::*;
use crate::{Event, Jak};

pub const YELLOW_SPEED: f32 = 819200.0;
pub const YELLOW_TIMEOUT: i64 = seconds(3.0);
pub const YELLOW_DAMAGE: f32 = 2.0;
/// The sphere the shot sweeps through the world with.
pub const YELLOW_RADIUS: f32 = 819.2;
/// The sphere it hits creatures with.
pub const YELLOW_ACTOR_RADIUS: f32 = 4096.0;
const TAIL_MAX: f32 = 65536.0;
const STOP_SPEED: f32 = 204.8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileHit {
    /// Where the shot came to rest, pulled back along its path.
    pub pos: Vec3,
    /// The point on the surface it struck, and that surface's normal.
    pub surface: Vec3,
    pub normal: Vec3,
    pub damage: f32,
    /// It struck a creature rather than the world.
    pub actor: Option<u64>,
    pub dir: Vec3,
}

#[derive(Clone, Debug)]
pub struct Projectile {
    pub trans: Vec3,
    pub transv: Vec3,
    /// The far end of the beam drawn behind the shot.
    pub tail: Vec3,
    pub dir: Vec3,
    pub spawn_time: i64,
    pub timeout: i64,
    pub damage: f32,
    old_dist: [f32; 16],
    old_dist_count: usize,
}

/// What a sweep of the shot met first.
struct Contact {
    u: f32,
    surface: Vec3,
    normal: Vec3,
    actor: Option<u64>,
}

/// Creatures a shot can hit, from the host: the earliest along the move.
pub trait ActorWorld {
    fn actor_hit(&mut self, start: Vec3, motion: Vec3, radius: f32) -> Option<(u64, f32, Vec3)>;
}

impl Projectile {
    pub fn yellow(pos: Vec3, dir: Vec3, now: i64) -> Self {
        let dir = dir.normalize_or(Vec3::Z);
        Self {
            trans: pos,
            transv: dir * YELLOW_SPEED,
            tail: pos,
            dir,
            spawn_time: now,
            timeout: YELLOW_TIMEOUT,
            damage: YELLOW_DAMAGE,
            old_dist: [4095996000.0; 16],
            old_dist_count: 0,
        }
    }

    fn sweep(
        &self,
        world: &mut dyn CollideWorld,
        cache: &mut CollideCache,
        start: Vec3,
        motion: Vec3,
    ) -> Option<Contact> {
        cache.fill_line_sphere(world, start, motion, YELLOW_RADIUS);
        cache
            .resolve_moving_sphere(start, YELLOW_RADIUS, motion, -1.0, true)
            .map(|h| Contact {
                u: h.u,
                surface: h.intersect,
                normal: h.normal,
                actor: None,
            })
    }

    fn hit(&self, contact: &Contact) -> ProjectileHit {
        ProjectileHit {
            pos: self.trans,
            surface: contact.surface,
            normal: contact.normal,
            damage: self.damage,
            actor: contact.actor,
            dir: self.dir,
        }
    }

    /// The shot fired into something within reach of the muzzle: it is
    /// stopped where it spawns.
    pub(crate) fn point_blank(
        &mut self,
        world: &mut dyn CollideWorld,
        cache: &mut CollideCache,
    ) -> Option<ProjectileHit> {
        let (back, ahead, keep) = (-10240.0, 12697.6, -4096.0);
        let start = self.trans + self.dir * back;
        let motion = self.dir * (ahead - back);
        let contact = self.sweep(world, cache, start, motion)?;
        let at = start + motion * contact.u;
        if self.dir.dot(at - self.trans) < keep {
            self.trans += self.dir * keep;
        } else {
            self.trans = at;
        }
        Some(self.hit(&contact))
    }

    /// One frame of flight. `Some` when the shot ends: the hit, or `None`
    /// inside for a shot that ran out of time.
    fn step(
        &mut self,
        now: i64,
        world: &mut dyn CollideWorld,
        actors: Option<&mut dyn ActorWorld>,
        cache: &mut CollideCache,
    ) -> Option<Option<ProjectileHit>> {
        if now - self.spawn_time >= self.timeout {
            return Some(None);
        }
        let before = self.trans;
        let motion = self.transv * SECONDS_PER_FRAME;
        let mut contact = self.sweep(world, cache, self.trans, motion);
        if let Some(actors) = actors
            && let Some((key, u, point)) = actors.actor_hit(self.trans, motion, YELLOW_ACTOR_RADIUS)
            && contact.as_ref().is_none_or(|c| u < c.u)
        {
            contact = Some(Contact {
                u,
                surface: point,
                normal: -self.dir,
                actor: Some(key),
            });
        }
        match &contact {
            Some(c) => {
                self.trans += motion * c.u;
                self.transv = Vec3::ZERO;
            }
            None => self.trans += motion,
        }
        let trail = self.tail - self.trans;
        if TAIL_MAX < trail.length() {
            self.tail = self.trans + normalize(trail, TAIL_MAX);
        }
        if let Some(c) = contact {
            let back = normalize(self.tail - self.trans, 2048.0);
            self.trans += back;
            return Some(Some(self.hit(&c)));
        }
        self.old_dist[self.old_dist_count] = 0.0625 * before.distance(self.trans);
        self.old_dist_count = (self.old_dist_count + 1) & 15;
        let moved: f32 = self.old_dist.iter().sum();
        if moved < STOP_SPEED {
            return Some(None);
        }
        None
    }
}

impl Jak {
    /// Shots in flight against the world and, when given, the creatures.
    pub fn step_projectiles_with(
        &mut self,
        world: &mut dyn CollideWorld,
        mut actors: Option<&mut dyn ActorWorld>,
    ) {
        let now = self.time;
        let mut cache = std::mem::take(&mut self.probe_cache);
        let mut i = 0;
        while i < self.projectiles.len() {
            let creatures = actors.as_mut().map(|a| &mut **a as &mut dyn ActorWorld);
            let ended = self.projectiles[i].step(now, world, creatures, &mut cache);
            match ended {
                Some(hit) => {
                    if let Some(hit) = hit {
                        self.events.push(Event::Impact(hit));
                    }
                    self.projectiles.swap_remove(i);
                }
                None => i += 1,
            }
        }
        self.probe_cache = cache;
    }
}
