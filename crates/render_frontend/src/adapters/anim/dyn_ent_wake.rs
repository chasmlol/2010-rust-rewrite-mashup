use std::collections::HashMap;
use std::time::Instant;

use bevy::prelude::*;
use entity_iw4::EntityEventKind;

use crate::adapters::anim::dyn_ent::xmodel_radius;
use crate::adapters::anim::dyn_ent_phys::{DynEntPhysImpulse, DynEntPhysWorld};
use crate::adapters::fx::present::{FxElemInfoCache, play_named_oriented_in_world};
use crate::adapters::fx::world_mark::FrontendFxScene;
use crate::prepare::scene::cull::DynEntModelEntity;
use crate::prepare::scene::world::{WorldDynEntInstance, WorldScene};
use assets::{BoundWeapons, PreparedWeapons};
use net::{
    ClientPredictionState, EntityBulletHit, EntityEventSound, EntityExplosion, EntityPhysicsSphere,
};
use render_fx::{EntityMarks, HostFxSystem, PreparedFxCatalog};

pub const DYNENT_BULLET_FORCE: f32 = 1000.0;

pub const DYNENT_EXPLODE_FORCE: f32 = 12500.0;

pub const DYNENT_EXPLODE_UPBIAS: f32 = 0.5;

pub const DYNENT_EXPLODE_MIN_FORCE: f32 = 40.0;

pub const DYNENT_EXPLODING_BULLET_FORCE: f32 = 3500.0;

pub const DYNENT_EXPLODING_BULLET_MIN_FORCE: f32 = 5.0;

const PLAYER_MINS: Vec3 = Vec3::new(-15.0, -15.0, 0.0);
const PLAYER_MAXS: Vec3 = Vec3::new(15.0, 15.0, 70.0);

const PLAYER_WAKE_SPEED: f32 = 12.0;

#[derive(Resource, Debug, Default)]
pub struct DynEntWakeBroadphase {
    entries: Vec<DynEntWakeEntry>,
    cells: HashMap<(i32, i32, i32), Vec<usize>>,
    marks: Vec<u32>,
    candidate_indices: Vec<usize>,
    candidates: Vec<Entity>,
    generation: u32,
    built: bool,
    pub entry_n: u32,
    pub cell_n: u32,
    pub query_n: u32,
    pub candidate_n: u32,
    pub full_scan_n: u32,
    pub fallback_n: u32,
    pub query_ms: f32,
}

#[derive(Clone, Copy, Debug)]
struct DynEntWakeEntry {
    entity: Entity,
    center: Vec3,
    radius: f32,
}

const DYNENT_WAKE_CELL_SIZE: f32 = 256.0;

const DYNENT_WAKE_MAX_QUERY_CELLS: i64 = 4096;

fn wake_cell(v: f32) -> Option<i32> {
    v.is_finite()
        .then(|| (v / DYNENT_WAKE_CELL_SIZE).floor() as i32)
}

fn wake_cell_bounds(mins: Vec3, maxs: Vec3) -> Option<((i32, i32, i32), (i32, i32, i32))> {
    if !mins.is_finite() || !maxs.is_finite() {
        return None;
    }
    Some((
        (wake_cell(mins.x)?, wake_cell(mins.y)?, wake_cell(mins.z)?),
        (wake_cell(maxs.x)?, wake_cell(maxs.y)?, wake_cell(maxs.z)?),
    ))
}

fn wake_query_cell_n(lo: (i32, i32, i32), hi: (i32, i32, i32)) -> i64 {
    let dx = i64::from(hi.0) - i64::from(lo.0) + 1;
    let dy = i64::from(hi.1) - i64::from(lo.1) + 1;
    let dz = i64::from(hi.2) - i64::from(lo.2) + 1;
    dx.max(0)
        .saturating_mul(dy.max(0))
        .saturating_mul(dz.max(0))
}

impl DynEntWakeBroadphase {
    fn rebuild(&mut self, entries: impl IntoIterator<Item = DynEntWakeEntry>) {
        self.entries.clear();
        self.cells.clear();
        self.candidate_indices.clear();
        self.candidates.clear();
        self.query_n = 0;
        self.candidate_n = 0;
        self.full_scan_n = 0;
        self.fallback_n = 0;
        self.query_ms = 0.0;
        self.built = true;

        for entry in entries {
            let index = self.entries.len();
            self.entries.push(entry);
            let radius = Vec3::splat(entry.radius.max(0.0));
            let Some((lo, hi)) = wake_cell_bounds(entry.center - radius, entry.center + radius)
            else {
                self.built = false;
                continue;
            };
            for z in lo.2..=hi.2 {
                for y in lo.1..=hi.1 {
                    for x in lo.0..=hi.0 {
                        self.cells.entry((x, y, z)).or_default().push(index);
                    }
                }
            }
        }
        self.marks.resize(self.entries.len(), 0);
        self.marks.fill(0);
        self.generation = 0;
        self.entry_n = self.entries.len() as u32;
        self.cell_n = self.cells.len() as u32;
    }

    fn query_aabb(&mut self, mins: Vec3, maxs: Vec3) -> Option<&[Entity]> {
        self.query_n = self.query_n.saturating_add(1);
        self.full_scan_n = self.full_scan_n.saturating_add(self.entries.len() as u32);
        let Some((lo, hi)) = wake_cell_bounds(mins, maxs) else {
            self.fallback_n = self.fallback_n.saturating_add(1);
            return None;
        };
        if !self.built || wake_query_cell_n(lo, hi) > DYNENT_WAKE_MAX_QUERY_CELLS {
            self.fallback_n = self.fallback_n.saturating_add(1);
            return None;
        }

        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.marks.fill(0);
            self.generation = 1;
        }
        self.candidate_indices.clear();
        for z in lo.2..=hi.2 {
            for y in lo.1..=hi.1 {
                for x in lo.0..=hi.0 {
                    let Some(indices) = self.cells.get(&(x, y, z)) else {
                        continue;
                    };
                    for &index in indices {
                        if self.marks[index] == self.generation {
                            continue;
                        }
                        self.marks[index] = self.generation;
                        self.candidate_indices.push(index);
                    }
                }
            }
        }
        self.candidate_indices.sort_unstable();
        self.candidates.clear();
        self.candidates.extend(
            self.candidate_indices
                .iter()
                .map(|&index| self.entries[index].entity),
        );
        self.candidate_n = self
            .candidate_n
            .saturating_add(self.candidates.len() as u32);
        Some(&self.candidates)
    }

    fn segment_candidates(&mut self, start: Vec3, end: Vec3) -> Option<&[Entity]> {
        self.query_aabb(start.min(end), start.max(end))
    }

    fn sphere_candidates(&mut self, center: Vec3, radius: f32) -> Option<&[Entity]> {
        let extent = Vec3::splat(radius.max(0.0));
        self.query_aabb(center - extent, center + extent)
    }
}

pub(crate) fn rebuild_dyn_ent_wake_broadphase(
    catalog: Option<Res<asset_world::MapXModelSceneCatalog>>,
    instances: Query<(Entity, &WorldDynEntInstance, &Transform), With<DynEntModelEntity>>,
    mut broadphase: ResMut<DynEntWakeBroadphase>,
) {
    let catalog = catalog.as_deref();
    broadphase.rebuild(instances.iter().map(|(entity, inst, transform)| {
        let radius = can_wake(inst)
            .map(|_| xmodel_radius(catalog, &inst.current_model).max(1.0))
            .unwrap_or(0.0);
        DynEntWakeEntry {
            entity,
            center: transform.translation,
            radius,
        }
    }));
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ExplosionDvars {
    force: f32,
    upbias: f32,
    min_force: f32,
}

const GRENADE_EXPLODE_DVARS: ExplosionDvars = ExplosionDvars {
    force: DYNENT_EXPLODE_FORCE,
    upbias: DYNENT_EXPLODE_UPBIAS,
    min_force: DYNENT_EXPLODE_MIN_FORCE,
};

const EXPLODING_BULLET_DVARS: ExplosionDvars = ExplosionDvars {
    force: DYNENT_EXPLODING_BULLET_FORCE,
    upbias: DYNENT_EXPLODE_UPBIAS,
    min_force: DYNENT_EXPLODING_BULLET_MIN_FORCE,
};

pub(crate) fn explosion_falloff(
    dist: f32,
    inner_radius: f32,
    outer_radius: f32,
    in_scale: f32,
) -> Option<f32> {
    if outer_radius <= 0.0 || dist >= outer_radius {
        return None;
    }
    let mut scale = in_scale;
    if dist > inner_radius && inner_radius < outer_radius {
        scale *= (dist - outer_radius) / (inner_radius - outer_radius);
        if scale < 0.0 {
            scale = 0.0;
        }
    }
    Some(scale)
}

pub(crate) fn explosion_damage(inner_damage: i32, outer_damage: i32, scale: f32) -> i32 {
    ((inner_damage - outer_damage) as f32 * scale + outer_damage as f32) as i32
}

pub(crate) fn apply_health(health: &mut i32, damage: i32) -> bool {
    if damage <= 0 || *health <= 0 {
        return false;
    }
    *health -= damage;
    if *health <= 0 {
        *health = 0;
        true
    } else {
        false
    }
}

fn is_destroyable(ty: asset_world::DynEntType) -> bool {
    asset_world::dyn_ent_props(ty).is_some_and(|props| props.destroyable)
}

pub(crate) fn explosion_impulse(
    origin: Vec3,
    pose: Vec3,
    inner_radius: f32,
    outer_radius: f32,
    in_scale: f32,
    explosive_scale: f32,
    explicit: Vec3,
    cylinder: bool,
    dvars: ExplosionDvars,
) -> Option<Vec3> {
    if !origin.is_finite() || !pose.is_finite() {
        return None;
    }
    let dist = pose.distance(origin);
    let scale = explosion_falloff(dist, inner_radius, outer_radius, in_scale)?;
    let force = scale * explosive_scale * dvars.force;
    if !force.is_finite() {
        return None;
    }
    let dir = if explicit.length_squared() > 0.0 {
        explicit
    } else {
        if force < dvars.min_force {
            return None;
        }
        let mut diff = pose - origin;
        if cylinder {
            diff.z = 0.0;
        }
        let mut n = diff.try_normalize().unwrap_or(Vec3::Z);
        n.z += dvars.upbias;
        n.try_normalize()?
    };
    if force <= 0.0 {
        return None;
    }
    Some(dir * force)
}

pub(crate) fn bullet_impulse(hit_dir: Vec3, bullet_scale: f32) -> Option<Vec3> {
    let dir = hit_dir.try_normalize()?;
    Some(dir * DYNENT_BULLET_FORCE * bullet_scale)
}

fn closest_t_on_segment(start: Vec3, end: Vec3, point: Vec3) -> f32 {
    let d = end - start;
    let len2 = d.length_squared();
    if len2 < 1e-12 {
        return 0.0;
    }
    ((point - start).dot(d) / len2).clamp(0.0, 1.0)
}

pub(crate) fn segment_hits_sphere(
    start: Vec3,
    end: Vec3,
    center: Vec3,
    radius: f32,
) -> Option<f32> {
    let t = closest_t_on_segment(start, end, center);
    let p = start + (end - start) * t;
    if p.distance_squared(center) <= radius * radius {
        Some(t)
    } else {
        None
    }
}

fn aabb_sphere_overlap(mins: Vec3, maxs: Vec3, center: Vec3, radius: f32) -> bool {
    let q = center.clamp(mins, maxs);
    q.distance_squared(center) <= radius * radius
}

fn can_wake(inst: &WorldDynEntInstance) -> Option<&asset_world::OwnedPhysPreset> {
    if inst.dead {
        return None;
    }
    let props = asset_world::dyn_ent_props(inst.ty)?;
    if !props.use_physics {
        return None;
    }
    let preset = inst.phys_preset.as_ref()?;
    if preset.mass <= 0.0 {
        return None;
    }
    Some(preset)
}

fn weapon_radii(weapons: Option<&BoundWeapons<'_>>, weapon: u32) -> Option<(f32, f32)> {
    let facts = weapons?.row(weapon)?.event_facts()?;
    let outer = facts.explosion_radius.max(0) as f32;
    if outer <= 0.0 {
        return None;
    }
    let inner = (facts.explosion_radius_min.max(0) as f32).min(outer);
    Some((inner, outer))
}

fn weapon_explosion_damage(weapons: Option<&BoundWeapons<'_>>, weapon: u32) -> (i32, i32) {
    weapons
        .and_then(|w| w.row(weapon).and_then(|weapon| weapon.event_facts()))
        .map(|f| (f.explosion_inner_damage, f.explosion_outer_damage))
        .unwrap_or((0, 0))
}

fn weapon_hit_damage(weapons: Option<&BoundWeapons<'_>>, weapon: u32) -> i32 {
    weapons
        .and_then(|w| w.row(weapon).and_then(|weapon| weapon.event_facts()))
        .map(|f| f.damage)
        .unwrap_or(0)
}

fn axis_from_rotation(rot: Quat) -> [[f32; 3]; 3] {
    let m = Mat3::from_quat(rot);
    [
        m.x_axis.to_array(),
        m.y_axis.to_array(),
        m.z_axis.to_array(),
    ]
}

fn kill_dyn_ent(
    entity: Entity,
    inst: &mut WorldDynEntInstance,
    transform: &Transform,
    visibility: &mut Visibility,
    phys: &mut DynEntPhysWorld,
    host: Option<&mut HostFxSystem>,
    fx: Option<&PreparedFxCatalog>,
    scene: Option<&WorldScene>,
    marks: &EntityMarks,
) {
    inst.dead = true;
    inst.health = 0;
    *visibility = Visibility::Hidden;
    phys.destroy_body(entity);
    let Some(name) = inst.destroy_fx.as_deref() else {
        return;
    };
    let Some(host) = host else {
        return;
    };
    let Some(fx) = fx else {
        return;
    };
    let cache = FxElemInfoCache::default();
    let _ = play_named_oriented_in_world(
        &mut host.0,
        &fx.0,
        &cache,
        fx.0.map_fx_name(name),
        transform.translation.to_array(),
        axis_from_rotation(transform.rotation),
        FrontendFxScene::wrap(scene, marks)
            .as_ref()
            .map(|s| s as &dyn render_fx::present::FxScene),
    );
}

fn apply_radius_hit(
    entity: Entity,
    inst: &mut WorldDynEntInstance,
    transform: &Transform,
    visibility: &mut Visibility,
    origin: Vec3,
    inner: f32,
    outer: f32,
    explicit: Vec3,
    dvars: ExplosionDvars,
    inner_damage: i32,
    outer_damage: i32,
    impulses: &mut MessageWriter<DynEntPhysImpulse>,
    phys: &mut DynEntPhysWorld,
    host: Option<&mut HostFxSystem>,
    fx: Option<&PreparedFxCatalog>,
    scene: Option<&WorldScene>,
    marks: &EntityMarks,
) {
    if inst.dead {
        return;
    }
    let dist = transform.translation.distance(origin);
    let Some(scale) = explosion_falloff(dist, inner, outer, 1.0) else {
        return;
    };
    if let Some(preset) = can_wake(inst) {
        if let Some(impulse) = explosion_impulse(
            origin,
            transform.translation,
            inner,
            outer,
            1.0,
            preset.explosive_force_scale,
            explicit,
            false,
            dvars,
        ) {
            impulses.write(DynEntPhysImpulse { entity, impulse });
        }
    }
    if !is_destroyable(inst.ty) {
        return;
    }
    let damage = explosion_damage(inner_damage, outer_damage, scale);
    if apply_health(&mut inst.health, damage) {
        kill_dyn_ent(
            entity, inst, transform, visibility, phys, host, fx, scene, marks,
        );
    }
}

pub(crate) fn on_entity_explosion(
    explosion: On<EntityExplosion>,
    weapons: Option<Res<PreparedWeapons>>,
    mut broadphase: ResMut<DynEntWakeBroadphase>,
    mut instances: Query<
        (
            Entity,
            &mut WorldDynEntInstance,
            &Transform,
            &mut Visibility,
        ),
        With<DynEntModelEntity>,
    >,
    mut impulses: MessageWriter<DynEntPhysImpulse>,
    mut phys: ResMut<DynEntPhysWorld>,
    mut host: Option<ResMut<HostFxSystem>>,
    fx: Option<Res<PreparedFxCatalog>>,
    scene: Option<Res<WorldScene>>,
    entity_marks: Res<EntityMarks>,
) {
    let weapons = weapons
        .as_deref()
        .and_then(|weapons| weapons.for_event(explosion.event.world).ok());
    let payload = explosion.event.payload;
    let Some((inner, outer)) = weapon_radii(weapons.as_ref(), payload.weapon) else {
        return;
    };
    let (inner_damage, outer_damage) = weapon_explosion_damage(weapons.as_ref(), payload.weapon);
    let origin = Vec3::from_array(payload.origin);
    let explicit = if matches!(
        explosion.event.event,
        EntityEventKind::GRENADE_EXPLODE
            | EntityEventKind::ROCKET_EXPLODE
            | EntityEventKind::ROCKET_EXPLODE_NOMARKS
            | EntityEventKind::FLASHBANG_EXPLODE
    ) {
        Vec3::ZERO
    } else {
        Vec3::from_array(payload.direction)
    };
    let mut host = host.as_deref_mut();
    let fx = fx.as_deref();
    let scene = scene.as_deref();
    let marks = entity_marks.as_ref();
    let started = Instant::now();
    if let Some(candidates) = broadphase.sphere_candidates(origin, outer) {
        for &entity in candidates {
            let Ok((entity, mut inst, transform, mut visibility)) = instances.get_mut(entity)
            else {
                continue;
            };
            apply_radius_hit(
                entity,
                &mut inst,
                transform,
                &mut visibility,
                origin,
                inner,
                outer,
                explicit,
                GRENADE_EXPLODE_DVARS,
                inner_damage,
                outer_damage,
                &mut impulses,
                phys.as_mut(),
                host.as_deref_mut(),
                fx,
                scene,
                marks,
            );
        }
    } else {
        for (entity, mut inst, transform, mut visibility) in &mut instances {
            apply_radius_hit(
                entity,
                &mut inst,
                transform,
                &mut visibility,
                origin,
                inner,
                outer,
                explicit,
                GRENADE_EXPLODE_DVARS,
                inner_damage,
                outer_damage,
                &mut impulses,
                phys.as_mut(),
                host.as_deref_mut(),
                fx,
                scene,
                marks,
            );
        }
    }
    broadphase.query_ms += started.elapsed().as_secs_f32() * 1000.0;
}

fn closest_segment_hit(
    start: Vec3,
    end: Vec3,
    catalog: Option<&asset_world::MapXModelSceneCatalog>,
    broadphase: &mut DynEntWakeBroadphase,
    instances: &Query<
        (
            Entity,
            &mut WorldDynEntInstance,
            &Transform,
            &mut Visibility,
        ),
        With<DynEntModelEntity>,
    >,
) -> Option<(Entity, Vec3, f32)> {
    let mut best: Option<(f32, Entity, Vec3, f32)> = None;
    let started = Instant::now();
    if let Some(candidates) = broadphase.segment_candidates(start, end) {
        for &entity in candidates {
            let Ok((entity, inst, transform, _)) = instances.get(entity) else {
                continue;
            };
            let Some(preset) = can_wake(inst) else {
                continue;
            };
            let radius = xmodel_radius(catalog, &inst.current_model).max(1.0);
            let Some(t) = segment_hits_sphere(start, end, transform.translation, radius) else {
                continue;
            };
            if best.is_none_or(|(best_t, _, _, _)| t < best_t) {
                best = Some((t, entity, transform.translation, preset.bullet_force_scale));
            }
        }
    } else {
        for (entity, inst, transform, _) in instances {
            let Some(preset) = can_wake(inst) else {
                continue;
            };
            let radius = xmodel_radius(catalog, &inst.current_model).max(1.0);
            let Some(t) = segment_hits_sphere(start, end, transform.translation, radius) else {
                continue;
            };
            if best.is_none_or(|(best_t, _, _, _)| t < best_t) {
                best = Some((t, entity, transform.translation, preset.bullet_force_scale));
            }
        }
    }
    broadphase.query_ms += started.elapsed().as_secs_f32() * 1000.0;
    best.map(|(_, entity, pose, scale)| (entity, pose, scale))
}

fn hit_one_dyn_ent(
    entity: Entity,
    pose: Vec3,
    start: Vec3,
    end: Vec3,
    bullet_scale: f32,
    damage: i32,
    instances: &mut Query<
        (
            Entity,
            &mut WorldDynEntInstance,
            &Transform,
            &mut Visibility,
        ),
        With<DynEntModelEntity>,
    >,
    impulses: &mut MessageWriter<DynEntPhysImpulse>,
    phys: &mut DynEntPhysWorld,
    host: Option<&mut HostFxSystem>,
    fx: Option<&PreparedFxCatalog>,
    scene: Option<&WorldScene>,
    marks: &EntityMarks,
) {
    let mut dir = end - start;
    if dir.length_squared() < 1e-12 {
        dir = pose - start;
    }
    if let Some(impulse) = bullet_impulse(dir, bullet_scale) {
        impulses.write(DynEntPhysImpulse { entity, impulse });
    }
    let Ok((_, mut inst, transform, mut visibility)) = instances.get_mut(entity) else {
        return;
    };
    if !is_destroyable(inst.ty) {
        return;
    }
    if apply_health(&mut inst.health, damage) {
        kill_dyn_ent(
            entity,
            &mut inst,
            transform,
            &mut visibility,
            phys,
            host,
            fx,
            scene,
            marks,
        );
    }
}

pub(crate) fn on_entity_bullet_hit(
    hit: On<EntityBulletHit>,
    weapons: Option<Res<PreparedWeapons>>,
    catalog: Option<Res<asset_world::MapXModelSceneCatalog>>,
    mut broadphase: ResMut<DynEntWakeBroadphase>,
    mut instances: Query<
        (
            Entity,
            &mut WorldDynEntInstance,
            &Transform,
            &mut Visibility,
        ),
        With<DynEntModelEntity>,
    >,
    mut impulses: MessageWriter<DynEntPhysImpulse>,
    mut phys: ResMut<DynEntPhysWorld>,
    mut host: Option<ResMut<HostFxSystem>>,
    fx: Option<Res<PreparedFxCatalog>>,
    scene: Option<Res<WorldScene>>,
    entity_marks: Res<EntityMarks>,
) {
    let weapons = weapons
        .as_deref()
        .and_then(|weapons| weapons.for_event(hit.event.world).ok());
    let payload = hit.event.payload;
    let start = Vec3::from_array(payload.origin2);
    let end = Vec3::from_array(payload.origin);
    let catalog = catalog.as_deref();
    let mut host = host.as_deref_mut();
    let fx = fx.as_deref();
    let scene = scene.as_deref();
    let marks = entity_marks.as_ref();
    if let Some((entity, pose, bullet_scale)) =
        closest_segment_hit(start, end, catalog, &mut broadphase, &instances)
    {
        hit_one_dyn_ent(
            entity,
            pose,
            start,
            end,
            bullet_scale,
            weapon_hit_damage(weapons.as_ref(), payload.weapon),
            &mut instances,
            &mut impulses,
            phys.as_mut(),
            host.as_deref_mut(),
            fx,
            scene,
            marks,
        );
    }
    if matches!(
        hit.event.event,
        EntityEventKind::BULLET_HIT_EXPLODE | EntityEventKind::BULLET_HIT_CLIENT_EXPLODE
    ) {
        let Some((inner, outer)) = weapon_radii(weapons.as_ref(), payload.weapon) else {
            return;
        };
        let (inner_damage, outer_damage) =
            weapon_explosion_damage(weapons.as_ref(), payload.weapon);
        let origin = end;
        let started = Instant::now();
        if let Some(candidates) = broadphase.sphere_candidates(origin, outer) {
            for &entity in candidates {
                let Ok((entity, mut inst, transform, mut visibility)) = instances.get_mut(entity)
                else {
                    continue;
                };
                apply_radius_hit(
                    entity,
                    &mut inst,
                    transform,
                    &mut visibility,
                    origin,
                    inner,
                    outer,
                    Vec3::ZERO,
                    EXPLODING_BULLET_DVARS,
                    inner_damage,
                    outer_damage,
                    &mut impulses,
                    phys.as_mut(),
                    host.as_deref_mut(),
                    fx,
                    scene,
                    marks,
                );
            }
        } else {
            for (entity, mut inst, transform, mut visibility) in &mut instances {
                apply_radius_hit(
                    entity,
                    &mut inst,
                    transform,
                    &mut visibility,
                    origin,
                    inner,
                    outer,
                    Vec3::ZERO,
                    EXPLODING_BULLET_DVARS,
                    inner_damage,
                    outer_damage,
                    &mut impulses,
                    phys.as_mut(),
                    host.as_deref_mut(),
                    fx,
                    scene,
                    marks,
                );
            }
        }
        broadphase.query_ms += started.elapsed().as_secs_f32() * 1000.0;
    }
}

pub(crate) fn on_entity_event_sound(
    sound: On<EntityEventSound>,
    weapons: Option<Res<PreparedWeapons>>,
    catalog: Option<Res<asset_world::MapXModelSceneCatalog>>,
    mut broadphase: ResMut<DynEntWakeBroadphase>,
    mut instances: Query<
        (
            Entity,
            &mut WorldDynEntInstance,
            &Transform,
            &mut Visibility,
        ),
        With<DynEntModelEntity>,
    >,
    mut impulses: MessageWriter<DynEntPhysImpulse>,
    mut phys: ResMut<DynEntPhysWorld>,
    mut host: Option<ResMut<HostFxSystem>>,
    fx: Option<Res<PreparedFxCatalog>>,
    scene: Option<Res<WorldScene>>,
    entity_marks: Res<EntityMarks>,
) {
    if sound.event.event != EntityEventKind::MELEE_HIT {
        return;
    }
    let weapons = weapons
        .as_deref()
        .and_then(|weapons| weapons.for_event(sound.event.world).ok());
    let payload = sound.event.payload;
    let start = Vec3::from_array(payload.origin2);
    let end = Vec3::from_array(payload.origin);
    let catalog = catalog.as_deref();
    let mut host = host.as_deref_mut();
    let fx = fx.as_deref();
    let scene = scene.as_deref();
    let marks = entity_marks.as_ref();
    let Some((entity, pose, bullet_scale)) =
        closest_segment_hit(start, end, catalog, &mut broadphase, &instances)
    else {
        return;
    };
    hit_one_dyn_ent(
        entity,
        pose,
        start,
        end,
        bullet_scale,
        weapon_hit_damage(weapons.as_ref(), payload.weapon),
        &mut instances,
        &mut impulses,
        phys.as_mut(),
        host.as_deref_mut(),
        fx,
        scene,
        marks,
    );
}

pub(crate) fn wake_player_overlap(
    prediction: Option<Res<ClientPredictionState>>,
    world: Res<DynEntPhysWorld>,
    catalog: Option<Res<asset_world::MapXModelSceneCatalog>>,
    instances: Query<(Entity, &WorldDynEntInstance, &Transform), With<DynEntModelEntity>>,
    mut impulses: MessageWriter<DynEntPhysImpulse>,
) {
    let Some(ps) = prediction.as_ref().and_then(|p| p.0.predicted_local()) else {
        return;
    };
    let vel = Vec3::from_array(ps.velocity);
    if vel.length() < PLAYER_WAKE_SPEED {
        return;
    }
    let origin = Vec3::from_array(ps.origin);
    let mins = origin + PLAYER_MINS;
    let maxs = origin + PLAYER_MAXS;
    let catalog = catalog.as_deref();
    for (entity, inst, transform) in &instances {
        if world.is_awake(entity) {
            continue;
        }
        let Some(preset) = can_wake(inst) else {
            continue;
        };
        let radius = xmodel_radius(catalog, &inst.current_model).max(1.0);
        if !aabb_sphere_overlap(mins, maxs, transform.translation, radius) {
            continue;
        }
        impulses.write(DynEntPhysImpulse {
            entity,
            impulse: vel * preset.mass,
        });
    }
}

fn on_physics_sphere(
    sphere: On<EntityPhysicsSphere>,
    instances: Query<(Entity, &WorldDynEntInstance, &Transform), With<DynEntModelEntity>>,
    mut impulses: MessageWriter<DynEntPhysImpulse>,
) {
    if !sphere.origin.iter().all(|v| v.is_finite())
        || !sphere.outer_radius.is_finite()
        || !sphere.inner_radius.is_finite()
        || !sphere.magnitude.is_finite()
        || sphere.inner_radius < 0.0
        || sphere.outer_radius < sphere.inner_radius
    {
        return;
    }
    let origin = Vec3::from_array(sphere.origin);
    for (entity, inst, transform) in &instances {
        let Some(preset) = can_wake(inst) else {
            continue;
        };
        if let Some(impulse) = explosion_impulse(
            origin,
            transform.translation,
            sphere.inner_radius,
            sphere.outer_radius,
            sphere.magnitude,
            preset.explosive_force_scale,
            Vec3::ZERO,
            false,
            GRENADE_EXPLODE_DVARS,
        ) {
            impulses.write(DynEntPhysImpulse { entity, impulse });
        }
    }
}

pub fn register_dyn_ent_wake(app: &mut App) {
    app.init_resource::<DynEntWakeBroadphase>()
        .add_observer(on_entity_explosion)
        .add_observer(on_physics_sphere)
        .add_observer(on_entity_bullet_hit)
        .add_observer(on_entity_event_sound)
        .add_systems(
            Update,
            rebuild_dyn_ent_wake_broadphase.in_set(net::ClientSet::Receive),
        )
        .add_systems(
            Update,
            wake_player_overlap
                .in_set(frame::WorkerCmdSet::Physics)
                .before(render_anim::occupancy::dyn_ent_phys::step_phys_world0),
        );
}
