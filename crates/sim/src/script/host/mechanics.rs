use super::entities::Link;
use crate::script::runtime::raise;
use crate::script::{BTreeMap, Resource, Runtime, Value};
use bevy_ecs::prelude::World;

/// Script-driven entity mechanics: timed moves, launched physics bodies and
/// entity/player links. They run before the VM each tick and settle native
/// collision, so a thread woken by movedone or physics_finished already
/// queries the final pose.
#[derive(Resource, Clone, Default)]
pub(crate) struct Mechanics {
    motions: BTreeMap<u64, Vec<Motion>>,
    bodies: BTreeMap<u64, Body>,
    slides: BTreeMap<u64, Slide>,
    finished: Vec<(u64, &'static str)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Body {
    velocity: [f32; 3],
    ticks: u32,
    offset: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    active: bool,
}

const TICK_S: f32 = crate::MATCH_TICK_MS as f32 / 1000.0;
const SETTLE_TICKS: u32 = 200;
const BODY_MINS: [f32; 3] = [-12.0, -12.0, 0.0];
const BODY_MAXS: [f32; 3] = [12.0, 12.0, 24.0];
const EXPLOSION_FORCE: f32 = 12500.0;
const EXPLOSION_UPBIAS: f32 = 0.5;
const EXPLOSION_MIN_FORCE: f32 = 40.0;

impl Mechanics {
    pub(crate) fn start(&mut self, object: u64, motion: Motion) {
        if motion.field == "origin" {
            self.bodies.remove(&object);
            self.slides.remove(&object);
        }
        let motions = self.motions.entry(object).or_default();
        motions.retain(|m| m.field != motion.field);
        motions.push(motion);
    }

    pub(crate) fn stop(&mut self, object: u64, field: &str) {
        if let Some(motions) = self.motions.get_mut(&object) {
            motions.retain(|m| m.field != field);
        }
    }

    pub(crate) fn launch(&mut self, object: u64, velocity: [f32; 3]) {
        self.motions.remove(&object);
        self.slides.remove(&object);
        self.bodies.insert(
            object,
            Body {
                velocity,
                ticks: 0,
                offset: [0.0; 3],
                mins: BODY_MINS,
                maxs: BODY_MAXS,
                active: true,
            },
        );
    }

    pub(crate) fn launch_piece(
        &mut self,
        object: u64,
        velocity: [f32; 3],
        offset: [f32; 3],
        half: [f32; 3],
    ) {
        self.bodies.insert(
            object,
            Body {
                velocity,
                ticks: 0,
                offset,
                mins: half.map(|h| -h),
                maxs: half,
                active: true,
            },
        );
    }

    pub(crate) fn cancel(&mut self, object: u64) {
        self.motions.remove(&object);
        self.bodies.remove(&object);
        self.slides.remove(&object);
    }

    pub(crate) fn slide(&mut self, object: u64, slide: Slide) {
        self.stop(object, "origin");
        self.bodies.remove(&object);
        self.slides.insert(object, slide);
    }

    pub(crate) fn sliding(&self, object: u64) -> bool {
        self.slides.contains_key(&object)
    }

    pub(crate) fn stop_slide(&mut self, object: u64) {
        self.slides.remove(&object);
    }

    pub(crate) fn velocity(&self, object: u64, now: i64) -> [f32; 3] {
        if let Some(slide) = self.slides.get(&object) {
            return slide.velocity;
        }
        if let Some(body) = self.bodies.get(&object) {
            return body.velocity;
        }
        self.motions
            .get(&object)
            .and_then(|motions| motions.iter().find(|m| m.field == "origin"))
            .map_or([0.0; 3], |m| m.sample(now).1)
    }

    pub(crate) fn explode(
        &mut self,
        runtime: &mut Runtime,
        center: [f32; 3],
        outer: f32,
        inner: f32,
        magnitude: f32,
    ) {
        for (object, body) in &mut self.bodies {
            if !runtime.entities.contains_key(object) {
                continue;
            }
            let Value::Vector(origin) = runtime.object_field(*object, "origin") else {
                continue;
            };
            if origin.iter().any(|v| !v.is_finite()) {
                continue;
            }
            let delta = glam::Vec3::from_array(origin) - glam::Vec3::from_array(center);
            let distance = delta.length();
            if outer <= 0.0 || distance >= outer {
                continue;
            }
            let fraction = if distance <= inner {
                1.0
            } else {
                (outer - distance) / (outer - inner)
            };
            let force = magnitude * fraction * EXPLOSION_FORCE;
            if !force.is_finite() || force < EXPLOSION_MIN_FORCE {
                continue;
            }
            let mut direction = delta.try_normalize().unwrap_or(glam::Vec3::Z);
            direction.z += EXPLOSION_UPBIAS;
            let impulse = direction.normalize() * force;
            body.velocity = (glam::Vec3::from_array(body.velocity) + impulse).to_array();
            body.ticks = 0;
            body.active = true;
        }
    }
}

pub(crate) fn advance_mechanics(world: &mut World) {
    let request = world.resource::<crate::step::StepRequest>();
    if !request.reason.advances_authority_world() {
        return;
    }
    let now = i64::from(request.tick.0) * i64::from(crate::MATCH_TICK_MS);
    let runtime = world.resource::<Runtime>();
    if runtime.fault.is_some() || runtime.program.is_none() {
        return;
    }
    advance_motions(world, now);
    advance_bodies(world);
    advance_slides(world);
    apply_entity_links(world);
    super::players::apply_player_links(world);
    super::presence::settle_collision(world);
}

pub(crate) fn deliver_finished(world: &mut World) {
    let finished = std::mem::take(&mut world.resource_mut::<Mechanics>().finished);
    for (object, name) in finished {
        raise(world, Value::Object(object), name, Vec::new());
    }
}

fn advance_motions(world: &mut World, now: i64) {
    world.resource_scope::<Mechanics, _>(|world, mut mechanics| {
        let mut runtime = world.resource_mut::<Runtime>();
        let Mechanics {
            motions, finished, ..
        } = &mut *mechanics;
        motions.retain(|object, _| runtime.entities.contains_key(object));
        for (object, list) in motions.iter_mut() {
            list.retain(|motion| {
                let (value, _) = motion.sample(now);
                runtime.set_object_field(*object, motion.field, Value::Vector(value));
                let done = now - motion.start_ms >= motion.duration_ms;
                if done {
                    finished.push((*object, motion.done));
                }
                !done
            });
        }
        motions.retain(|_, list| !list.is_empty());
    });
}

fn advance_bodies(world: &mut World) {
    let mut crushed = Vec::new();
    world.resource_scope::<Mechanics, _>(|world, mut mechanics| {
        let Mechanics {
            bodies, finished, ..
        } = &mut *mechanics;
        bodies.retain(|object, body| {
            if !world.resource::<Runtime>().entities.contains_key(object) {
                return false;
            }
            if !body.active {
                return true;
            }
            let (origin, angles) = {
                let mut runtime = world.resource_mut::<Runtime>();
                if !runtime.entities.contains_key(object) {
                    return false;
                }
                match (
                    runtime.object_field(*object, "origin"),
                    runtime.object_field(*object, "angles"),
                ) {
                    (Value::Vector(o), Value::Vector(a)) => (o, a),
                    (Value::Vector(o), _) => (o, [0.0; 3]),
                    _ => return true,
                }
            };
            body.velocity[2] -= GRAVITY * TICK_S;
            let start: [f32; 3] = std::array::from_fn(|i| origin[i] + body.offset[i]);
            let end: [f32; 3] = std::array::from_fn(|i| start[i] + body.velocity[i] * TICK_S);
            let trace = crate::frame::FrameWorld::from_world(world).trace_static_world(
                start,
                end,
                body.mins,
                body.maxs,
                crate::bullet_collision::MASK_PHYS_WORLD,
            );
            let fraction = if trace.startsolid != 0 {
                0.0
            } else {
                trace.fraction
            };
            let at: [f32; 3] =
                std::array::from_fn(|i| origin[i] + body.velocity[i] * TICK_S * fraction);
            if body.velocity[2] < 0.0 {
                crushed.extend(
                    crush_victims(world, *object, origin, at, body)
                        .into_iter()
                        .map(|(victim, point)| (victim, *object, point)),
                );
            }
            body.ticks += 1;
            let rested = fraction < 1.0 || body.ticks >= SETTLE_TICKS;
            let mut runtime = world.resource_mut::<Runtime>();
            runtime.set_object_field(*object, "origin", Value::Vector(at));
            if rested {
                runtime.set_object_field(*object, "angles", Value::Vector([0.0, angles[1], 0.0]));
                finished.push((*object, "physics_finished"));
                body.velocity = [0.0; 3];
                body.active = false;
            }
            true
        });
    });
    for (victim, pusher, point) in crushed {
        super::players::crush_player(world, victim, pusher, point);
    }
}

fn crush_victims(
    world: &mut World,
    object: u64,
    origin: [f32; 3],
    at: [f32; 3],
    body: &Body,
) -> Vec<(crate::ClientId, [f32; 3])> {
    let presence = world.resource::<Runtime>().entities[&object].presence;
    let frame = crate::frame::FrameWorld::from_world(world);
    let (mins, maxs) = presence
        .and_then(|id| {
            let row = frame
                .entity_collision_capabilities()
                .iter()
                .find(|row| row.owner.script_model() == Some(id))?;
            let brush = row.linked_brushes.first()?;
            let model = frame
                .clip_cmodels()
                .models
                .get(brush.cmodel_handle as usize)?;
            let reach = (0..2)
                .map(|i| model.mins[i].abs().max(model.maxs[i].abs()))
                .fold(0.0f32, f32::max);
            let offset: [f32; 3] = std::array::from_fn(|i| brush.origin[i] - origin[i]);
            Some((
                [
                    offset[0] - reach,
                    offset[1] - reach,
                    offset[2] + model.mins[2],
                ],
                [
                    offset[0] + reach,
                    offset[1] + reach,
                    offset[2] + model.maxs[2],
                ],
            ))
        })
        .unwrap_or((body.mins, body.maxs));
    let lo: [f32; 3] = std::array::from_fn(|i| origin[i].min(at[i]) + mins[i]);
    let hi: [f32; 3] = std::array::from_fn(|i| origin[i].max(at[i]) + maxs[i]);
    let (pmins, pmaxs) = (
        crate::bullet_collision::PLAYER_MINS,
        crate::bullet_collision::PLAYER_MAXS,
    );
    let mut victims = Vec::new();
    frame.visit_players(|id, ps| {
        if ps.pm_type >= playerstate_iw4::PM_TYPE_DEAD {
            return;
        }
        let inside =
            (0..3).all(|i| ps.origin[i] + pmins[i] < hi[i] && ps.origin[i] + pmaxs[i] > lo[i]);
        if inside {
            victims.push((id, ps.origin));
        }
    });
    victims
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Slide {
    pub center: [f32; 3],
    pub radius: f32,
    pub velocity: [f32; 3],
}

pub(crate) fn load_slide_field(world: &World, object: u64, name: &str) -> Option<Value> {
    (name == "slidevelocity" && world.resource::<Runtime>().entities.contains_key(&object)).then(
        || {
            world
                .resource::<Mechanics>()
                .slides
                .get(&object)
                .map_or(Value::Vector([0.0; 3]), |slide| {
                    Value::Vector(slide.velocity)
                })
        },
    )
}

pub(crate) fn store_slide_field(
    world: &mut World,
    object: u64,
    name: &str,
    value: &Value,
) -> Result<bool, String> {
    if name != "slidevelocity" || !world.resource::<Runtime>().entities.contains_key(&object) {
        return Ok(false);
    }
    let Value::Vector(velocity) = value else {
        return Err("slideVelocity requires a vector".into());
    };
    if velocity.iter().any(|v| !v.is_finite()) {
        return Err("slideVelocity must be finite".into());
    }
    let mut mechanics = world.resource_mut::<Mechanics>();
    let slide = mechanics
        .slides
        .get_mut(&object)
        .ok_or("cannot set slideVelocity on an entity that is not sliding")?;
    slide.velocity = *velocity;
    Ok(true)
}

fn advance_slides(world: &mut World) {
    world.resource_scope::<Mechanics, _>(|world, mut mechanics| {
        mechanics.slides.retain(|object, slide| {
            let origin = {
                let mut runtime = world.resource_mut::<Runtime>();
                if !runtime.entities.contains_key(object) {
                    return false;
                }
                let Value::Vector(origin) = runtime.object_field(*object, "origin") else {
                    return false;
                };
                origin
            };
            if origin.iter().any(|v| !v.is_finite()) {
                return false;
            }
            super::presence::settle_collision(world);
            let at = crate::step::script_slide(world, *object, origin, slide);
            world
                .resource_mut::<Runtime>()
                .set_object_field(*object, "origin", Value::Vector(at));
            true
        });
    });
}

impl Slide {
    pub(crate) fn advance<C: movement_iw4::CollisionBackend>(
        &mut self,
        origin: [f32; 3],
        collision: &C,
        mask: u32,
    ) -> [f32; 3] {
        use movement_iw4::GroundTraceInput;
        let mins = self.center.map(|v| v - self.radius);
        let maxs = self.center.map(|v| v + self.radius);
        let trace = |start, end| {
            collision.trace(GroundTraceInput {
                start,
                end,
                mins,
                maxs,
                tracemask: mask,
            })
        };
        let start_velocity = self.velocity;
        let mut at = origin;
        if !self.sweep(&mut at, collision, mask, mins, maxs) {
            return at;
        }
        let down = trace(origin, [origin[0], origin[1], origin[2] - 18.0]);
        if start_velocity[2] > 0.0 && (down.fraction == 1.0 || down.normal[2] < 0.7) {
            return at;
        }
        let up = trace(origin, [origin[0], origin[1], origin[2] + 18.0]);
        if up.startsolid != 0 {
            return at;
        }
        at = up.endpos;
        let elevation = at[2] - origin[2];
        self.velocity = start_velocity;
        self.sweep(&mut at, collision, mask, mins, maxs);
        let down = trace(at, [at[0], at[1], at[2] - elevation]);
        if down.startsolid == 0 {
            at = down.endpos;
        }
        if down.fraction < 1.0 {
            self.velocity = clip_slide(self.velocity, down.normal);
        }
        at
    }

    fn sweep<C: movement_iw4::CollisionBackend>(
        &mut self,
        origin: &mut [f32; 3],
        collision: &C,
        mask: u32,
        mins: [f32; 3],
        maxs: [f32; 3],
    ) -> bool {
        let mut end_velocity = self.velocity;
        end_velocity[2] -= GRAVITY * TICK_S;
        self.velocity[2] = (self.velocity[2] + end_velocity[2]) * 0.5;
        let mut planes = [[0.0; 3]; 5];
        planes[0] = glam::Vec3::from_array(self.velocity)
            .normalize_or_zero()
            .to_array();
        let mut plane_count = 1;
        let mut time = TICK_S;
        let mut bumped = false;
        for _ in 0..4 {
            let end = std::array::from_fn(|i| origin[i] + self.velocity[i] * time);
            if end.iter().any(|v: &f32| !v.is_finite()) {
                self.velocity = [0.0; 3];
                return true;
            }
            let hit = collision.trace(movement_iw4::GroundTraceInput {
                start: *origin,
                end,
                mins,
                maxs,
                tracemask: mask,
            });
            if hit.allsolid != 0 {
                self.velocity[2] = 0.0;
                return true;
            }
            if hit.fraction > 0.0 {
                *origin = hit.endpos;
            }
            if hit.fraction == 1.0 {
                break;
            }
            bumped = true;
            time *= 1.0 - hit.fraction;
            if plane_count == planes.len() {
                self.velocity = [0.0; 3];
                return true;
            }
            if planes[..plane_count]
                .iter()
                .any(|p| slide_dot(hit.normal, *p) > 0.99)
            {
                self.velocity = std::array::from_fn(|i| self.velocity[i] + hit.normal[i]);
                continue;
            }
            planes[plane_count] = hit.normal;
            plane_count += 1;
            for i in 0..plane_count {
                if slide_dot(self.velocity, planes[i]) >= 0.1 {
                    continue;
                }
                let mut velocity = clip_slide(self.velocity, planes[i]);
                let mut final_velocity = clip_slide(end_velocity, planes[i]);
                for j in 0..plane_count {
                    if i == j || slide_dot(velocity, planes[j]) >= 0.1 {
                        continue;
                    }
                    velocity = clip_slide(velocity, planes[j]);
                    final_velocity = clip_slide(final_velocity, planes[j]);
                    if slide_dot(velocity, planes[i]) >= 0.0 {
                        continue;
                    }
                    let crease = glam::Vec3::from_array(planes[i])
                        .cross(glam::Vec3::from_array(planes[j]))
                        .normalize_or_zero();
                    velocity =
                        (crease * crease.dot(glam::Vec3::from_array(self.velocity))).to_array();
                    final_velocity =
                        (crease * crease.dot(glam::Vec3::from_array(end_velocity))).to_array();
                    if planes[..plane_count]
                        .iter()
                        .enumerate()
                        .any(|(k, p)| k != i && k != j && slide_dot(velocity, *p) < 0.1)
                    {
                        self.velocity = [0.0; 3];
                        return true;
                    }
                }
                self.velocity = velocity;
                end_velocity = final_velocity;
                break;
            }
        }
        self.velocity = end_velocity;
        bumped
    }
}

fn slide_dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn clip_slide(velocity: [f32; 3], normal: [f32; 3]) -> [f32; 3] {
    let dot = slide_dot(velocity, normal);
    let backoff = -(dot - dot.abs() * (1.001 - 1.0));
    std::array::from_fn(|i| velocity[i] + backoff * normal[i])
}

fn apply_entity_links(world: &mut World) {
    let mut runtime = world.resource_mut::<Runtime>();
    let linked: Vec<(u64, Link)> = runtime
        .entities
        .iter()
        .filter_map(|(id, e)| Some((*id, e.linked_to.clone()?)))
        .collect();
    for (id, link) in linked {
        if !runtime.live(&link.parent) {
            runtime.entities.get_mut(&id).unwrap().linked_to = None;
            continue;
        }
        let field = |runtime: &mut Runtime, name| match runtime.object_field(link.parent, name) {
            Value::Vector(v) => v,
            _ => [0.0; 3],
        };
        let (base, base_angles) = (field(&mut runtime, "origin"), field(&mut runtime, "angles"));
        let axis = math_iw4::angles_to_axis(base_angles);
        let offset = link.tag_offset.unwrap_or([0.0; 3]);
        let local = std::array::from_fn(|i| offset[i] + link.origin[i]);
        let (child_axis, origin) =
            math_iw4::matrix_multiply43(math_iw4::angles_to_axis(link.angles), local, axis, base);
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        runtime.set_object_field(
            id,
            "angles",
            Value::Vector(math_iw4::axis_to_angles(child_axis)),
        );
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Motion {
    pub field: &'static str,
    pub path: MotionPath,
    pub start_ms: i64,
    pub duration_ms: i64,
    pub done: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MotionPath {
    Linear {
        from: [f32; 3],
        to: [f32; 3],
        accel: f32,
        decel: f32,
    },
    Ballistic {
        from: [f32; 3],
        velocity: [f32; 3],
    },
}

const GRAVITY: f32 = 800.0;

impl Motion {
    pub(crate) fn sample(&self, now: i64) -> ([f32; 3], [f32; 3]) {
        let elapsed = (now - self.start_ms).clamp(0, self.duration_ms);
        let t = elapsed as f32 / 1000.0;
        match self.path {
            MotionPath::Linear {
                from,
                to,
                accel,
                decel,
            } => {
                let total = self.duration_ms as f32 / 1000.0;
                let peak = 2.0 / (2.0 * total - accel - decel);
                let cruise_end = total - decel;
                let (fraction, rate) = if t < accel {
                    (0.5 * peak * t * t / accel, peak * t / accel)
                } else if t <= cruise_end {
                    (0.5 * peak * accel + peak * (t - accel), peak)
                } else {
                    let u = t - cruise_end;
                    (
                        0.5 * peak * accel + peak * (cruise_end - accel) + peak * u
                            - 0.5 * peak * u * u / decel,
                        peak * (1.0 - u / decel),
                    )
                };
                let fraction = if elapsed >= self.duration_ms {
                    1.0
                } else {
                    fraction
                };
                let rate = if elapsed >= self.duration_ms {
                    0.0
                } else {
                    rate
                };
                (
                    std::array::from_fn(|i| from[i] + (to[i] - from[i]) * fraction),
                    std::array::from_fn(|i| (to[i] - from[i]) * rate),
                )
            }
            MotionPath::Ballistic { from, velocity } => {
                let mut p: [f32; 3] = std::array::from_fn(|i| from[i] + velocity[i] * t);
                p[2] -= 0.5 * GRAVITY * t * t;
                let mut v = velocity;
                v[2] -= GRAVITY * t;
                (p, v)
            }
        }
    }
}
