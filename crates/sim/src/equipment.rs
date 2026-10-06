use crate::bullet_collision::{EntityCollisionEpoch, EntityCollisionTraceGeom};
use crate::damage::DamageAttempt;
use crate::frame::FrameWorld;
use crate::identities::{DamageSource, LifeSequence, PelletId};
use crate::match_state::{ClientLifecycle, EventAudience};
use crate::{
    BulletTraceQuery, ClientId, ColliderId, MASK_BULLET_WORLD, ProjectileId, Tick, TraceOutcome,
    bullet_trace_with_entity_models, level_time_ms,
};
use entity_iw4::{
    GLASS_PROJECTILE_PANE_HOPS, GRENADE_SPIN_PITCH_MAX, GRENADE_SPIN_PITCH_MIN,
    GRENADE_SPIN_ROLL_MAX, GRENADE_SPIN_ROLL_MIN, MISSILE_GLASS_SHATTER_VEL, MissileLandAnglesIn,
    TR_GRAVITY, TR_STATIONARY, Trajectory, evaluate_trajectory, evaluate_trajectory_delta,
    fire_grenade_no_draw_ms, fire_missile_apos, init_grenade_apos, init_grenade_pos,
    missile_land_angles,
};
use trace_iw4::surface_type_from_flags;

const SURF_TYPE_GLASS: u8 = 9;

pub use weapon_iw4::EquipmentRuntimeFacts;
use weapon_iw4::WEAPCLASS_THROWINGKNIFE;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileState {
    pub id: ProjectileId,
    pub owner: ClientId,
    pub owner_life: LifeSequence,
    pub weapon: u32,
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    pub pos: Trajectory,
    pub apos: Trajectory,
    pub entnum: i32,
    pub launch_time: i32,
    pub spawn_time_ms: i32,
    pub detonate_at_ms: Option<i32>,
    pub cleanup_at_ms: i32,
    pub travel_distance: f32,
    pub live: bool,
    pub stuck_pane: Option<u32>,
    pub grounded: bool,
    pub guide: crate::MissileGuide,
    pub attached_to: Option<crate::MissileTarget>,
}

impl ProjectileState {
    pub fn origin_at(&self, at_time_ms: i32) -> [f32; 3] {
        evaluate_trajectory(&self.pos, at_time_ms)
    }

    pub fn velocity_at(&self, at_time_ms: i32) -> [f32; 3] {
        evaluate_trajectory_delta(&self.pos, at_time_ms)
    }

    pub fn is_armed(&self, activate_dist: i32) -> bool {
        self.live && (activate_dist <= 0 || self.travel_distance >= activate_dist as f32)
    }
}

pub const GRENADE_FUSE_CAP_MS: i32 = 60_000;
const OFFHAND_CLASS_SMOKE: i32 = 2;
pub const GRENADE_DEFAULT_FUSE_MS: i32 = 30_000;
pub const ROCKET_CLEANUP_MS: i32 = 60_000;
pub const BOUNCE_EVENT_SPEED_DELTA: f32 = 100.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GrenadeLaunchKind {
    Thrown { remaining_fuse_ms: Option<i32> },
    Launcher,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum WeaponNote {
    Pullback {
        owner: ClientId,
        weapon: u32,
    },
    Fired {
        owner: ClientId,
    },
    DetonationRequested {
        owner: ClientId,
        weapon: u32,
    },
    ReloadStarted {
        owner: ClientId,
    },
    Detonated {
        id: crate::ProjectileId,
        origin: [f32; 3],
    },
    Stuck {
        id: crate::ProjectileId,
    },
}

pub fn projectile_birth_ms(projectile: &ProjectileState) -> i32 {
    projectile.spawn_time_ms
}

fn forward(angles: [f32; 3]) -> [f32; 3] {
    let pitch = angles[0].to_radians();
    let yaw = angles[1].to_radians();
    [
        pitch.cos() * yaw.cos(),
        pitch.cos() * yaw.sin(),
        -pitch.sin(),
    ]
}

fn vec3_length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn vec3_normalize(v: [f32; 3]) -> Option<[f32; 3]> {
    let len = vec3_length(v);
    if len < 1e-6 {
        None
    } else {
        Some([v[0] / len, v[1] / len, v[2] / len])
    }
}

fn flatten_xy(dir: [f32; 3]) -> Option<[f32; 3]> {
    vec3_normalize([dir[0], dir[1], 0.0])
}

fn project_owner_velocity(launch_vel: [f32; 3], owner_vel: [f32; 3]) -> [f32; 3] {
    let Some(dir) = vec3_normalize(launch_vel) else {
        return launch_vel;
    };
    let along = owner_vel[0] * dir[0] + owner_vel[1] * dir[1] + owner_vel[2] * dir[2];
    [
        launch_vel[0] + dir[0] * along,
        launch_vel[1] + dir[1] * along,
        launch_vel[2] + dir[2] * along,
    ]
}

fn grenade_launch_velocity(
    direction: [f32; 3],
    facts: &EquipmentRuntimeFacts,
    owner_vel: [f32; 3],
) -> [f32; 3] {
    let speed = facts.projectile_speed as f32;
    let mut velocity = [
        direction[0] * speed,
        direction[1] * speed,
        direction[2] * speed + facts.projectile_speed_up as f32,
    ];
    if facts.projectile_speed_forward != 0
        && let Some(flat) = flatten_xy(direction)
    {
        let extra = facts.projectile_speed_forward as f32;
        velocity[0] += flat[0] * extra;
        velocity[1] += flat[1] * extra;
        velocity[2] += flat[2] * extra;
    }
    if facts.projectile_speed_relative_up != 0 {
        let level = direction[0].hypot(direction[1]);
        let up = match flatten_xy(direction) {
            Some(flat) => [-flat[0] * direction[2], -flat[1] * direction[2], level],
            None => [0.0, 0.0, 1.0],
        };
        let extra = facts.projectile_speed_relative_up as f32;
        for (axis, along) in velocity.iter_mut().zip(up) {
            *axis += along * extra;
        }
    }
    project_owner_velocity(velocity, owner_vel)
}

fn grenade_fuse_due(now_ms: i32, detonate_at_ms: Option<i32>, live: bool) -> bool {
    detonate_at_ms.is_some_and(|deadline| live && deadline <= now_ms)
}

pub(crate) const AIRDROP_MARKER_WEAPON: &str = "airdrop_marker_mp";

// The airdrop marker's fuse would otherwise pop the flare midair on a steep throw.
fn waits_for_ground(world: &FrameWorld, facts: &EquipmentRuntimeFacts, weapon: u32) -> bool {
    facts.offhand_class == OFFHAND_CLASS_SMOKE
        || world.weapon_script_name(weapon) == AIRDROP_MARKER_WEAPON
}

fn grenade_deadlines(
    facts: &EquipmentRuntimeFacts,
    kind: GrenadeLaunchKind,
    now_ms: i32,
) -> (Option<i32>, i32) {
    let cap_at = now_ms.saturating_add(GRENADE_FUSE_CAP_MS);
    if facts.is_throwing_knife() {
        return (None, cap_at);
    }
    let cleanup_at_ms = cap_at;
    match kind {
        GrenadeLaunchKind::Launcher if facts.fuse_time_ms <= 0 => {
            if facts.projectile_activate_dist != 0 {
                (None, cleanup_at_ms)
            } else {
                (
                    Some(now_ms.saturating_add(GRENADE_DEFAULT_FUSE_MS).min(cap_at)),
                    cleanup_at_ms,
                )
            }
        }
        GrenadeLaunchKind::Thrown { remaining_fuse_ms } => {
            let authored = facts.fuse_time_ms;
            let fuse = if facts.timed_detonation {
                remaining_fuse_ms.filter(|&ms| ms > 0).unwrap_or(authored)
            } else {
                authored
            };
            let fuse = if fuse <= 0 {
                GRENADE_DEFAULT_FUSE_MS
            } else {
                fuse.min(GRENADE_FUSE_CAP_MS)
            };
            (Some(now_ms.saturating_add(fuse)), cleanup_at_ms)
        }
        GrenadeLaunchKind::Launcher => {
            let fuse = if facts.fuse_time_ms <= 0 {
                GRENADE_DEFAULT_FUSE_MS
            } else {
                facts.fuse_time_ms.min(GRENADE_FUSE_CAP_MS)
            };
            (Some(now_ms.saturating_add(fuse)), cleanup_at_ms)
        }
    }
}

pub(crate) fn spawn_grenade_projectile(
    world: &mut FrameWorld,
    owner: ClientId,
    weapon: u32,
    tick: Tick,
    origin: [f32; 3],
    angles: [f32; 3],
    owner_vel: [f32; 3],
    kind: GrenadeLaunchKind,
) -> bool {
    let Some(facts) = world.equipment_facts_for(weapon) else {
        return false;
    };
    if !facts.is_usable() {
        return false;
    }
    let owner_life = world.client_meta(owner).unwrap().life_sequence;
    let direction = forward(angles);
    let time_ms = level_time_ms(tick);
    let (pitch_rate, roll_rate) = match kind {
        GrenadeLaunchKind::Launcher => (0.0, 0.0),
        GrenadeLaunchKind::Thrown { .. } if facts.weap_class == WEAPCLASS_THROWINGKNIFE => {
            (entity_iw4::GRENADE_BLADE_SPIN_PITCH, 0.0)
        }
        GrenadeLaunchKind::Thrown { .. } if facts.projectile_rotates => grenade_spin_rates(world),
        GrenadeLaunchKind::Thrown { .. } => (0.0, 0.0),
    };
    let apos = if pitch_rate == 0.0 && roll_rate == 0.0 {
        fire_missile_apos(direction)
    } else {
        init_grenade_apos(direction, time_ms, pitch_rate, roll_rate)
    };
    let velocity = grenade_launch_velocity(direction, &facts, owner_vel);
    let pos = init_grenade_pos(origin, velocity, time_ms);
    let speed = vec3_length(velocity);
    let launch_time = time_ms + fire_grenade_no_draw_ms(speed);
    let (detonate_at_ms, cleanup_at_ms) = grenade_deadlines(&facts, kind, time_ms);
    let id_projectile = world.allocate_projectile_id();
    let entnum = match world.allocate_dynamic_entity(crate::gentity::EntityRunKind::Missile) {
        Ok(entity) => entity.number(),
        Err(error) => {
            diag::warn!(Sim, "grenade not spawned: {error:?}");
            return false;
        }
    };
    world.push_projectile(ProjectileState {
        id: id_projectile,
        owner,
        owner_life,
        weapon,
        origin,
        velocity: pos.tr_delta,
        pos,
        apos,
        entnum,
        launch_time,
        spawn_time_ms: time_ms,
        detonate_at_ms,
        cleanup_at_ms,
        travel_distance: 0.0,
        live: true,
        stuck_pane: None,
        grounded: false,
        guide: crate::MissileGuide::default(),
        attached_to: None,
    });
    true
}

pub(crate) fn spawn_offhand_projectile(
    world: &mut FrameWorld,
    id: ClientId,
    weapon: u32,
    tick: Tick,
    remaining_fuse_ms: Option<i32>,
) -> bool {
    let (origin, angles, owner_vel) = {
        let Some(ps) = world.player(id) else {
            return false;
        };
        (
            [
                ps.origin[0],
                ps.origin[1],
                ps.origin[2] + ps.view_height_current,
            ],
            ps.viewangles,
            ps.velocity,
        )
    };
    spawn_grenade_projectile(
        world,
        id,
        weapon,
        tick,
        origin,
        angles,
        owner_vel,
        GrenadeLaunchKind::Thrown { remaining_fuse_ms },
    )
}

pub(crate) fn explode_offhand_in_hand(
    world: &mut FrameWorld,
    id: ClientId,
    weapon: u32,
    tick: Tick,
) {
    let Some(facts) = world.equipment_facts_for(weapon) else {
        return;
    };
    let Some(ps) = world.player(id).copied() else {
        return;
    };
    let owner_life = world
        .client_meta(id)
        .map(|m| m.life_sequence)
        .unwrap_or_default();
    let origin = [
        ps.origin[0],
        ps.origin[1],
        ps.origin[2] + ps.view_height_current,
    ];
    let radius = facts
        .explosion_radius
        .max(facts.explosion_radius_min)
        .max(0) as f32;
    if facts.projectile_explosion_type == 2 {
        crate::damage::apply_flashbang_blast(
            world,
            origin,
            facts.explosion_radius.max(0) as f32,
            facts.explosion_radius_min.max(0) as f32,
            id,
        );
    }
    if radius <= 0.0 || facts.explosion_inner_damage <= 0 {
        return;
    }
    let blast = crate::damage::ExplosionBlast {
        cone: None,
        origin,
        radius,
        inner_damage: facts.explosion_inner_damage as f32,
        outer_damage: facts.explosion_outer_damage.max(0) as f32,
        weapon,
        source: DamageSource::Projectile(ProjectileId(0)),
        attacker: id,
        attacker_life: owner_life,
        killcam_entity_start_time: level_time_ms(tick),
    };
    crate::damage::apply_explosion_blast(world, tick, &blast);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectileHitGeometry {
    World,
    Player,
    Entity { epoch: EntityCollisionEpoch },
    Bounce,
    Fuse,
    Dud,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileImpact {
    pub id: ProjectileId,
    pub owner: ClientId,
    pub weapon: u32,
    pub origin: [f32; 3],
    pub geometry: ProjectileHitGeometry,
    pub terminal: Option<ColliderId>,
    pub amount: i32,
    pub fraction: Option<f32>,
    pub surface_flags: Option<u32>,
    pub surf_type: Option<u8>,
    pub entnum: i32,
}

fn grenade_spin_rates(world: &mut FrameWorld) -> (f32, f32) {
    let rng = world.combat_rng_mut();
    let pitch_sign = if rng.next_index(2) == 0 { 1.0 } else { -1.0 };
    let pitch = host_flrand(rng, GRENADE_SPIN_PITCH_MIN, GRENADE_SPIN_PITCH_MAX);
    let roll_sign = if rng.next_index(2) == 0 { 1.0 } else { -1.0 };
    let roll = host_flrand(rng, GRENADE_SPIN_ROLL_MIN, GRENADE_SPIN_ROLL_MAX);
    (pitch_sign * pitch, roll_sign * roll)
}

fn host_flrand(rng: &mut crate::MatchRng, min: f32, max: f32) -> f32 {
    let u = rng.next_u32() as f32 * (1.0 / 4_294_967_296.0);
    min + (max - min) * u
}

pub(crate) fn phase_offhand(
    world: &mut FrameWorld,
    _tick: Tick,
    cmds: &[(ClientId, playerstate_iw4::UserCmd)],
) {
    for (id, cmd) in cmds {
        if !world
            .client_meta(*id)
            .is_some_and(|meta| meta.lifecycle == ClientLifecycle::Alive)
        {
            continue;
        }
        let requested = u32::from(cmd.off_hand_index);

        if requested != 0 {
            if let Some(ps) = world.player_mut(*id) {
                ps.off_hand_index = requested as i32;
            }
        }
    }
}

pub(crate) fn predict_projectile(
    world: &FrameWorld,
    mut projectile: ProjectileState,
    now: i32,
    duration_ms: i32,
) -> Option<[f32; 3]> {
    let facts = required_projectile_facts(world, projectile.weapon);
    let attached = world.missile_collision_models(projectile.id);
    let models: Vec<_> = world
        .entity_collision_capabilities()
        .iter()
        .filter(|capabilities| {
            !capabilities
                .owner
                .script_model()
                .is_some_and(|id| attached.contains(&id))
        })
        .map(|capabilities| capabilities.trace_geom())
        .collect();
    let content = world.content();
    let (brushes, bsp, cmodels, mesh) = (
        content.clip_brushes(),
        content.clip_bsp(),
        content.clip_cmodels(),
        content.clip_mesh(),
    );
    let players = world.alive_collision_poses();
    let deadline = now
        .saturating_add(duration_ms)
        .min(projectile.cleanup_at_ms)
        .min(projectile.detonate_at_ms.unwrap_or(i32::MAX));
    let mut time = now;
    while time < deadline {
        if projectile.pos.tr_type == TR_STATIONARY {
            return Some(projectile.origin);
        }
        let next = time
            .saturating_add(crate::MATCH_TICK_MS as i32)
            .min(deadline);
        crate::missile_guidance::steer(world, &mut projectile, next, facts);
        let end = projectile.origin_at(next);
        match bullet_trace_with_entity_models(
            &brushes,
            &bsp,
            &cmodels,
            &mesh,
            &players,
            &models,
            &BulletTraceQuery {
                start: projectile.origin,
                end,
                mask: MASK_BULLET_WORLD,
                ignore: Some(projectile.owner),
                ignore_hit: None,
                ignore_model: None,
            },
            &|piece| world.world_objects().glass_is_solid(u32::from(piece)),
        ) {
            TraceOutcome::Hit {
                end,
                normal,
                collider,
                fraction,
            } => {
                projectile.travel_distance +=
                    vec3_length(core::array::from_fn(|i| end[i] - projectile.origin[i]));
                if (matches!(collider, ColliderId::Player { .. }) && facts.stick_to_players)
                    || (!matches!(collider, ColliderId::Player { .. })
                        && sticks_to_surface(facts, normal))
                    || (facts.proj_impact_explode
                        && projectile.is_armed(facts.projectile_activate_dist))
                {
                    return Some(end);
                }
                let surf_type = match collider {
                    ColliderId::World { surface_flags, .. }
                    | ColliderId::EntityDObjBone { surface_flags, .. }
                    | ColliderId::EntityLinkedBrush { surface_flags, .. } => {
                        surface_type_from_flags(surface_flags)
                    }
                    ColliderId::Player { .. } => 0,
                };
                let hit_time = time + ((next - time) as f32 * fraction) as i32;
                projectile.velocity = projectile.velocity_at(hit_time);
                bounce_velocity(&mut projectile, normal, &facts, surf_type);
                projectile.origin = end;
                projectile.pos.tr_base = end;
                projectile.pos.tr_time = next;
                projectile.pos.tr_delta = projectile.velocity;
            }
            TraceOutcome::Miss { .. } => {
                projectile.travel_distance +=
                    vec3_length(core::array::from_fn(|i| end[i] - projectile.origin[i]));
                projectile.origin = end;
            }
            TraceOutcome::StartSolid { .. } | TraceOutcome::Invalid { .. } => return None,
        }
        time = next;
    }
    Some(projectile.origin)
}

pub(crate) fn think_projectile(world: &mut FrameWorld, tick: Tick, entnum: i32) {
    struct PendingDetonation {
        projectile: ProjectileState,
        origin: [f32; 3],
        normal: [f32; 3],
        surf_type: u8,
        geometry: ProjectileHitGeometry,
        terminal: Option<ColliderId>,
        amount: i32,
        fraction: Option<f32>,
        surface_flags: Option<u32>,
        splash: bool,
    }
    let mut detonated = Vec::new();
    let mut direct_hits = Vec::new();
    let mut entity_hits = Vec::new();
    let mut impacts = Vec::new();
    let Some(mut projectile) = world.projectile_by_number(entnum) else {
        return;
    };
    let time = level_time_ms(tick);
    let prev = time.saturating_sub(crate::MATCH_TICK_MS as i32);
    if time >= projectile.cleanup_at_ms
        && projectile
            .detonate_at_ms
            .is_none_or(|deadline| deadline > projectile.cleanup_at_ms || !projectile.live)
    {
        let _ = world.remove_projectile_by_number(entnum);
        return;
    }
    let facts = required_projectile_facts(world, projectile.weapon);
    if let Some(target) = projectile.attached_to {
        if let Some(origin) = target.resolve(world) {
            projectile.origin = origin;
            projectile.pos.tr_base = origin;
            projectile.pos.tr_time = time;
        } else {
            projectile.attached_to = None;
        }
    }
    crate::missile_guidance::steer(world, &mut projectile, time, facts);
    let attached = world.missile_collision_models(projectile.id);
    let script_models: Vec<EntityCollisionTraceGeom> = world
        .entity_collision_capabilities()
        .iter()
        .filter(|capabilities| {
            !capabilities
                .owner
                .script_model()
                .is_some_and(|id| attached.contains(&id))
        })
        .map(|capabilities| capabilities.trace_geom())
        .collect();
    let content = world.content();
    let cmodels = content.clip_cmodels();
    let players = world.alive_collision_poses();
    let (brushes, bsp, mesh) = (
        content.clip_brushes(),
        content.clip_bsp(),
        content.clip_mesh(),
    );
    let start = projectile.origin;
    let mut eval_time = time;
    if let Some(deadline) = projectile
        .detonate_at_ms
        .filter(|&ms| projectile.live && ms <= time)
    {
        eval_time = eval_time.min(deadline);
    }
    if projectile.cleanup_at_ms > prev && projectile.cleanup_at_ms <= eval_time {
        eval_time = eval_time.min(projectile.cleanup_at_ms);
    }
    let end = projectile.origin_at(eval_time);
    let vel = projectile.velocity_at(eval_time);
    let speed = (vel[0] * vel[0] + vel[1] * vel[1] + vel[2] * vel[2]).sqrt();
    if let Some(pane) = projectile.stuck_pane {
        if world.world_objects().glass_is_solid(pane)
            && projectile
                .detonate_at_ms
                .is_none_or(|deadline| deadline > time)
        {
            if let Some(row) = world.projectile_mut_by_number(entnum) {
                *row = projectile;
            }
            return;
        }
        if !world.world_objects().glass_is_solid(pane) {
            unstick_missile(tick, &mut projectile);
            projectile.stuck_pane = None;
        }
    }
    if projectile.pos.tr_type == TR_STATIONARY
        && projectile
            .detonate_at_ms
            .is_none_or(|deadline| deadline > time)
    {
        if let Some(row) = world.projectile_mut_by_number(entnum) {
            *row = projectile;
        }
        return;
    }
    let mut trace_start = start;
    let mut hops = 0u32;
    let mut hop_capped = false;
    let outcome = loop {
        if projectile.pos.tr_type == TR_STATIONARY {
            break TraceOutcome::Miss { end };
        }
        let glass_pairs = world.world_objects().glass_damage_pairs();
        let outcome = bullet_trace_with_entity_models(
            &brushes,
            &bsp,
            &cmodels,
            &mesh,
            &players,
            &script_models,
            &BulletTraceQuery {
                start: trace_start,
                end,
                mask: MASK_BULLET_WORLD,
                ignore: Some(projectile.owner),
                ignore_hit: None,
                ignore_model: None,
            },
            &|piece| {
                crate::world_objects::glass_piece_is_solid(
                    glass_pairs
                        .iter()
                        .find(|(id, _)| *id == u32::from(piece))
                        .map(|(_, d)| *d)
                        .unwrap_or(0),
                )
            },
        );
        let punched = match outcome {
            TraceOutcome::Hit {
                end: hit_end,
                collider:
                    ColliderId::World {
                        surface_flags,
                        glass_encoded,
                        ..
                    },
                ..
            }
            | TraceOutcome::StartSolid {
                end: hit_end,
                collider:
                    Some(ColliderId::World {
                        surface_flags,
                        glass_encoded,
                        ..
                    }),
            } if missile_glass_punch_eligible(speed, surface_flags, glass_encoded) => {
                let pane = u32::from(glass_encoded).saturating_sub(1);
                if world.world_objects().glass_is_solid(pane) {
                    let _ = world.world_objects_mut().force_shatter_glass(
                        pane,
                        time,
                        hit_end,
                        vel,
                        entity_iw4::GlassCause::Impact,
                    );
                    hops = hops.saturating_add(1);
                    if hops >= GLASS_PROJECTILE_PANE_HOPS {
                        hop_capped = true;
                        park_missile_at(tick, &mut projectile, hit_end, vel);
                        None
                    } else {
                        let dx = end[0] - trace_start[0];
                        let dy = end[1] - trace_start[1];
                        let dz = end[2] - trace_start[2];
                        let len = (dx * dx + dy * dy + dz * dz).sqrt().max(1.0);
                        trace_start = [
                            hit_end[0] + dx / len * 0.25,
                            hit_end[1] + dy / len * 0.25,
                            hit_end[2] + dz / len * 0.25,
                        ];
                        Some(())
                    }
                } else {
                    None
                }
            }
            _ => None,
        };
        if punched.is_some() && !hop_capped {
            continue;
        }
        break outcome;
    };
    if world.publishes_snapshot()
        && projectile.live
        && crate::script_player::means(
            world,
            DamageSource::Projectile(projectile.id),
            projectile.weapon,
            0,
            false,
        ) == "MOD_GRENADE"
    {
        let touch_end = match outcome {
            TraceOutcome::Hit { end, .. } | TraceOutcome::StartSolid { end, .. } => Some(end),
            TraceOutcome::Miss { .. } => Some(end),
            TraceOutcome::Invalid { .. } => None,
        };
        if let Some(touch_end) = touch_end {
            crate::script::host::triggers::record_grenade_touch(
                world.ecs(),
                projectile,
                start,
                touch_end,
                facts.explosion_inner_damage.max(0),
            );
        }
    }
    if hop_capped {
        projectile.velocity = vel;
        if let Some(row) = world.projectile_mut_by_number(entnum) {
            *row = projectile;
        }
        return;
    }
    let mut pending_detonation = None;
    let mut hit = match outcome {
        TraceOutcome::Hit {
            fraction,
            end,
            normal,
            collider,
        } => Some((end, normal, Some(collider), fraction)),
        TraceOutcome::StartSolid { collider, end } => {
            Some((end, startsolid_normal(start, end), collider, 0.0))
        }
        TraceOutcome::Miss { .. } | TraceOutcome::Invalid { .. } => None,
    };
    let mut contact_origin = None;
    if facts.is_retrievable_knife()
        && let Some((end, normal, collider, _)) = hit
        && !matches!(collider, Some(ColliderId::Player { .. }))
    {
        let probe_start = core::array::from_fn(|i| end[i] + normal[i] * 0.135);
        let probe_end = core::array::from_fn(|i| end[i] - normal[i] * 1.5);
        if let TraceOutcome::Hit {
            end,
            normal,
            collider,
            fraction,
        } = bullet_trace_with_entity_models(
            &brushes,
            &bsp,
            &cmodels,
            &mesh,
            &players,
            &script_models,
            &BulletTraceQuery {
                start: probe_start,
                end: probe_end,
                mask: MASK_BULLET_WORLD,
                ignore: Some(projectile.owner),
                ignore_hit: None,
                ignore_model: None,
            },
            &|piece| world.world_objects().glass_is_solid(u32::from(piece)),
        ) {
            // Surface clearance precedes the knife's resting/embedded pose offsets.
            contact_origin = Some(core::array::from_fn(|i| end[i] + (end[i] - probe_end[i])));
            hit = Some((end, normal, Some(collider), fraction));
        }
    }
    let mut contact_time = eval_time;
    if let Some((_end, _normal, _collider, fraction)) = hit {
        contact_time = prev.saturating_add(((eval_time - prev) as f32 * fraction) as i32);
    }
    let mut fuse_due = grenade_fuse_due(time, projectile.detonate_at_ms, projectile.live);
    if fuse_due && !projectile.grounded && waits_for_ground(world, &facts, projectile.weapon) {
        fuse_due = false;
        projectile.detonate_at_ms = Some(time.saturating_add(crate::MATCH_TICK_MS as i32));
    }
    let cleanup_due = time >= projectile.cleanup_at_ms;
    let contact_before_fuse = hit.is_some()
        && !(projectile.pos.tr_type == TR_STATIONARY && fuse_due)
        && projectile
            .detonate_at_ms
            .is_none_or(|deadline| contact_time <= deadline);
    enum TickOutcome {
        Contact,
        Fuse,
        Cleanup,
        Fly,
    }
    let tick_outcome = if hit.is_some() && contact_before_fuse {
        TickOutcome::Contact
    } else if fuse_due {
        TickOutcome::Fuse
    } else if cleanup_due {
        TickOutcome::Cleanup
    } else {
        TickOutcome::Fly
    };
    match tick_outcome {
        TickOutcome::Cleanup => {
            let _ = world.remove_projectile_by_number(entnum);
            return;
        }
        TickOutcome::Fuse => {
            let fuse_time = projectile.detonate_at_ms.unwrap_or(eval_time);
            projectile.origin = projectile.origin_at(fuse_time);
            projectile.velocity = projectile.velocity_at(fuse_time);
            pending_detonation = Some(PendingDetonation {
                projectile,
                origin: projectile.origin,
                normal: [0.0, 0.0, 1.0],
                surf_type: 0,
                geometry: ProjectileHitGeometry::Fuse,
                terminal: None,
                amount: 0,
                fraction: None,
                surface_flags: None,
                splash: projectile.live,
            });
        }
        TickOutcome::Fly => {
            let traveled = vec3_length([end[0] - start[0], end[1] - start[1], end[2] - start[2]]);
            projectile.origin = end;
            projectile.travel_distance = projectile.travel_distance + traveled;
            projectile.velocity = projectile.velocity_at(time);
            if let Some(row) = world.projectile_mut_by_number(entnum) {
                *row = projectile;
            }
            return;
        }
        TickOutcome::Contact => {
            let (end, normal, collider, fraction) = hit.expect("contact");
            let traveled = vec3_length([end[0] - start[0], end[1] - start[1], end[2] - start[2]]);
            projectile.origin = contact_origin.unwrap_or(end);
            projectile.travel_distance = projectile.travel_distance + traveled;
            let armed = projectile.is_armed(facts.projectile_activate_dist);
            let (surface_flags, hit_surf) = match collider {
                Some(ColliderId::World { surface_flags, .. }) => (
                    Some(surface_flags),
                    Some(surface_type_from_flags(surface_flags)),
                ),
                _ => (None, None),
            };
            match collider {
                _ if facts.impact_payload_weapon != 0 => {
                    if let Some(ColliderId::Player { client, .. }) = collider {
                        direct_hits.push((projectile, client));
                    }
                    if let Some(
                        ColliderId::EntityDObjBone { owner, .. }
                        | ColliderId::EntityLinkedBrush { owner, .. },
                    ) = collider
                    {
                        if let Some(target) = owner.script_model() {
                            let bone = match collider {
                                Some(ColliderId::EntityDObjBone { bone, .. }) => {
                                    Some(usize::from(bone))
                                }
                                _ => None,
                            };
                            entity_hits.push((
                                DamageSource::Projectile(projectile.id),
                                crate::script::EntityHit {
                                    target,
                                    amount: facts.impact_damage.max(0),
                                    attacker: Some(projectile.owner),
                                    means: "",
                                    weapon: projectile.weapon,
                                    point: end,
                                    dir: projectile.velocity,
                                    bone,
                                    flags: 0,
                                },
                            ));
                        }
                    }
                    impacts.push(ProjectileImpact {
                        id: projectile.id,
                        owner: projectile.owner,
                        weapon: projectile.weapon,
                        origin: end,
                        geometry: if matches!(collider, Some(ColliderId::Player { .. })) {
                            ProjectileHitGeometry::Player
                        } else {
                            ProjectileHitGeometry::World
                        },
                        terminal: collider,
                        amount: facts.impact_damage.max(0),
                        fraction: Some(fraction),
                        surface_flags,
                        surf_type: hit_surf,
                        entnum: projectile.entnum,
                    });
                    arm_impact_payload(
                        world,
                        tick,
                        contact_time,
                        &mut projectile,
                        end,
                        normal,
                        collider,
                    );
                }
                _ if facts.is_retrievable_knife() => {
                    let player_hit = matches!(collider, Some(ColliderId::Player { .. }));
                    if let Some(ColliderId::Player { client, .. }) = collider {
                        direct_hits.push((projectile, client));
                    }
                    knife_impact(
                        world,
                        tick,
                        &mut projectile,
                        normal,
                        fraction,
                        hit_surf.unwrap_or(0),
                        player_hit,
                    );
                    if projectile.pos.tr_type == TR_STATIONARY
                        && let Some(ColliderId::World { glass_encoded, .. }) = collider
                        && glass_encoded != 0
                    {
                        projectile.stuck_pane = Some(u32::from(glass_encoded) - 1);
                    }
                    impacts.push(ProjectileImpact {
                        id: projectile.id,
                        owner: projectile.owner,
                        weapon: projectile.weapon,
                        origin: end,
                        geometry: if player_hit {
                            ProjectileHitGeometry::Player
                        } else {
                            ProjectileHitGeometry::Bounce
                        },
                        terminal: collider,
                        amount: if player_hit {
                            facts.impact_damage.max(0)
                        } else {
                            0
                        },
                        fraction: Some(fraction),
                        surface_flags,
                        surf_type: hit_surf,
                        entnum: projectile.entnum,
                    });
                }
                Some(ColliderId::Player { client, .. }) => {
                    if facts.stick_to_players {
                        if facts.impact_damage > 0 {
                            direct_hits.push((projectile, client));
                        }
                        projectile.attached_to = player_attachment(world, client, end);
                        stick_missile(tick, &mut projectile, end);
                        push_grenade_stick(world, tick, &projectile);
                    } else if !armed {
                        direct_hits.push((projectile, client));
                        projectile.live = false;
                        pending_detonation = Some(PendingDetonation {
                            projectile,
                            origin: end,
                            normal,
                            surf_type: 0,
                            geometry: ProjectileHitGeometry::Dud,
                            terminal: collider,
                            amount: facts.impact_damage.max(0),
                            fraction: Some(fraction),
                            surface_flags: None,
                            splash: false,
                        });
                    } else if facts.proj_impact_explode {
                        direct_hits.push((projectile, client));
                        pending_detonation = Some(PendingDetonation {
                            projectile,
                            origin: end,
                            normal,
                            surf_type: 0,
                            geometry: ProjectileHitGeometry::Player,
                            terminal: collider,
                            amount: facts.impact_damage.max(0),
                            fraction: Some(fraction),
                            surface_flags: None,
                            splash: true,
                        });
                    } else {
                        direct_hits.push((projectile, client));
                        bounce_missile(world, tick, &mut projectile, end, normal, fraction, 0);
                        impacts.push(ProjectileImpact {
                            id: projectile.id,
                            owner: projectile.owner,
                            weapon: projectile.weapon,
                            origin: end,
                            geometry: ProjectileHitGeometry::Bounce,
                            terminal: collider,
                            amount: 0,
                            fraction: Some(fraction),
                            surface_flags: None,
                            surf_type: None,
                            entnum: projectile.entnum,
                        });
                    }
                }
                Some(
                    collider @ (ColliderId::EntityDObjBone { .. }
                    | ColliderId::EntityLinkedBrush { .. }),
                ) => {
                    let epoch = entity_collision_epoch(collider, &script_models)
                        .unwrap_or(EntityCollisionEpoch::CurrentTick);
                    if armed && facts.proj_impact_explode {
                        if let Some(target) = match collider {
                            ColliderId::EntityDObjBone { owner, .. }
                            | ColliderId::EntityLinkedBrush { owner, .. } => owner.script_model(),
                            _ => None,
                        } {
                            let bone = match collider {
                                ColliderId::EntityDObjBone { bone, .. } => Some(usize::from(bone)),
                                _ => None,
                            };
                            entity_hits.push((
                                DamageSource::Projectile(projectile.id),
                                crate::script::EntityHit {
                                    target,
                                    amount: facts.impact_damage.max(0),
                                    attacker: Some(projectile.owner),
                                    means: "",
                                    weapon: projectile.weapon,
                                    point: end,
                                    dir: projectile.velocity,
                                    bone,
                                    flags: 0,
                                },
                            ));
                        }
                        pending_detonation = Some(PendingDetonation {
                            projectile,
                            origin: end,
                            normal,
                            surf_type: 0,
                            geometry: ProjectileHitGeometry::Entity { epoch },
                            terminal: Some(collider),
                            amount: facts.impact_damage.max(0),
                            fraction: Some(fraction),
                            surface_flags: None,
                            splash: true,
                        });
                    } else if sticks_to_surface(facts, normal) {
                        settle_equipment(world, tick, &mut projectile, end, normal, fraction);
                    } else if !armed {
                        projectile.live = false;
                        pending_detonation = Some(PendingDetonation {
                            projectile,
                            origin: end,
                            normal,
                            surf_type: 0,
                            geometry: ProjectileHitGeometry::Dud,
                            terminal: Some(collider),
                            amount: facts.impact_damage.max(0),
                            fraction: Some(fraction),
                            surface_flags: None,
                            splash: false,
                        });
                    } else {
                        bounce_missile(world, tick, &mut projectile, end, normal, fraction, 0);
                        impacts.push(ProjectileImpact {
                            id: projectile.id,
                            owner: projectile.owner,
                            weapon: projectile.weapon,
                            origin: end,
                            geometry: ProjectileHitGeometry::Bounce,
                            terminal: Some(collider),
                            amount: 0,
                            fraction: Some(fraction),
                            surface_flags: None,
                            surf_type: None,
                            entnum: projectile.entnum,
                        });
                    }
                }
                Some(world_collider @ ColliderId::World { .. }) => {
                    let surf_type = hit_surf.unwrap_or(0);
                    if armed && facts.proj_impact_explode {
                        pending_detonation = Some(PendingDetonation {
                            projectile,
                            origin: end,
                            normal,
                            surf_type,
                            geometry: ProjectileHitGeometry::World,
                            terminal: Some(world_collider),
                            amount: facts.impact_damage.max(0),
                            fraction: Some(fraction),
                            surface_flags,
                            splash: true,
                        });
                    } else if sticks_to_surface(facts, normal) {
                        if let ColliderId::World { glass_encoded, .. } = world_collider
                            && glass_encoded != 0
                        {
                            let pane = u32::from(glass_encoded).saturating_sub(1);
                            if world.world_objects().glass_is_solid(pane) {
                                projectile.stuck_pane = Some(pane);
                            }
                        }
                        settle_equipment(world, tick, &mut projectile, end, normal, fraction);
                    } else if !armed {
                        projectile.live = false;
                        pending_detonation = Some(PendingDetonation {
                            projectile,
                            origin: end,
                            normal,
                            surf_type,
                            geometry: ProjectileHitGeometry::Dud,
                            terminal: Some(world_collider),
                            amount: facts.impact_damage.max(0),
                            fraction: Some(fraction),
                            surface_flags,
                            splash: false,
                        });
                    } else {
                        bounce_missile(
                            world,
                            tick,
                            &mut projectile,
                            end,
                            normal,
                            fraction,
                            surf_type,
                        );
                        impacts.push(ProjectileImpact {
                            id: projectile.id,
                            owner: projectile.owner,
                            weapon: projectile.weapon,
                            origin: end,
                            geometry: ProjectileHitGeometry::Bounce,
                            terminal: Some(world_collider),
                            amount: 0,
                            fraction: Some(fraction),
                            surface_flags,
                            surf_type: hit_surf,
                            entnum: projectile.entnum,
                        });
                    }
                }
                None => {
                    if armed && facts.proj_impact_explode {
                        pending_detonation = Some(PendingDetonation {
                            projectile,
                            origin: end,
                            normal,
                            surf_type: 0,
                            geometry: ProjectileHitGeometry::World,
                            terminal: None,
                            amount: facts.impact_damage.max(0),
                            fraction: Some(fraction),
                            surface_flags: None,
                            splash: true,
                        });
                    } else if !armed {
                        projectile.live = false;
                        pending_detonation = Some(PendingDetonation {
                            projectile,
                            origin: end,
                            normal,
                            surf_type: 0,
                            geometry: ProjectileHitGeometry::Dud,
                            terminal: None,
                            amount: 0,
                            fraction: Some(fraction),
                            surface_flags: None,
                            splash: false,
                        });
                    }
                }
            }
        }
    }

    if let Some(info) = pending_detonation {
        if world.publishes_snapshot() && info.fraction.is_some() {
            let means = if matches!(info.geometry, ProjectileHitGeometry::Dud) {
                "MOD_IMPACT"
            } else {
                crate::script_player::means(
                    world,
                    DamageSource::Projectile(projectile.id),
                    projectile.weapon,
                    0,
                    false,
                )
            };
            crate::script::host::triggers::damage_line(
                world.ecs(),
                info.origin,
                info.origin,
                facts.impact_damage.max(0),
                projectile.owner,
                Some(projectile.id),
                means,
            );
        }
        detonated.push(info);
    }

    if detonated.is_empty() {
        projectile.velocity = projectile.velocity_at(time);
        if let Some(row) = world.projectile_mut_by_number(entnum) {
            *row = projectile;
        }
    } else {
        let _ = world.remove_projectile_by_number(entnum);
    }
    for info in &detonated {
        impacts.push(ProjectileImpact {
            id: info.projectile.id,
            owner: info.projectile.owner,
            weapon: info.projectile.weapon,
            origin: info.origin,
            geometry: info.geometry,
            terminal: info.terminal,
            amount: info.amount,
            fraction: info.fraction,
            surface_flags: info.surface_flags,
            surf_type: info.surface_flags.map(|_| info.surf_type),
            entnum: info.projectile.entnum,
        });
    }
    world.record_projectile_impacts(tick, &impacts);
    for info in &detonated {
        if info.splash {
            world.note_dying_missile(tick, info.projectile);
        }
    }
    let resolves_damage = world.publishes_snapshot();
    if resolves_damage {
        for (projectile, target) in direct_hits {
            let facts = required_projectile_facts(world, projectile.weapon);
            let Some(target_meta) = world.client_meta(target) else {
                continue;
            };
            let intent = DamageAttempt {
                splash: false,
                source: DamageSource::Projectile(projectile.id),
                pellet: PelletId(0),
                attacker: projectile.owner,
                attacker_life: projectile.owner_life,
                target,
                target_life: target_meta.life_sequence,
                weapon: projectile.weapon,
                amount: facts.impact_damage.max(0),
                killcam_entity_start_time: projectile_birth_ms(&projectile),
                inflictor_origin: Some(projectile.origin),
                hitloc: 0,
            };
            let _ = crate::damage::apply_damage_attempt(world, tick, &intent);
        }
        for (source, mut hit) in std::mem::take(&mut entity_hits) {
            hit.means = crate::script_player::means(world, source, hit.weapon, 0, false);
            crate::script::damage_entity(world.ecs(), &hit);
        }
    }
    for info in detonated {
        perf::projectile_phase(
            info.projectile.weapon,
            info.projectile.id.0,
            "detonated",
            time,
        );
        world.weapon_notes.push(WeaponNote::Detonated {
            id: info.projectile.id,
            origin: info.origin,
        });
        if !info.splash {
            continue;
        }
        let facts = required_projectile_facts(world, info.projectile.weapon);
        let event_kind = if facts.projectile_explosion_type == 2 {
            entity_iw4::EntityEventKind::FLASHBANG_EXPLODE
        } else {
            entity_iw4::EntityEventKind::GRENADE_EXPLODE
        };
        world.push_entity_event(
            tick,
            EventAudience::All,
            event_kind,
            crate::EntityEventPayload {
                number: info.projectile.entnum,
                attacker_entity_num: info.projectile.owner.0 as i32,
                weapon: info.projectile.weapon,
                correlation: info.projectile.id.0,
                origin: info.origin,
                direction: info.normal,
                surf_type: info.surf_type,
                ..Default::default()
            },
        );
        if !resolves_damage {
            continue;
        }
        if facts.projectile_explosion_type == 2 {
            crate::damage::apply_flashbang_blast(
                world,
                info.origin,
                facts.explosion_radius.max(0) as f32,
                facts.explosion_radius_min.max(0) as f32,
                info.projectile.owner,
            );
        }
        let radius = facts
            .explosion_radius
            .max(facts.explosion_radius_min)
            .max(0) as f32;
        if radius <= 0.0 || (facts.explosion_inner_damage <= 0 && facts.explosion_outer_damage <= 0)
        {
            continue;
        }
        let blast = crate::damage::ExplosionBlast {
            cone: (facts.damage_cone_angle > 0.0 && facts.damage_cone_angle < 180.0).then(|| {
                (
                    forward(info.projectile.apos.tr_base),
                    facts.damage_cone_angle.to_radians().cos(),
                )
            }),
            origin: info.origin,
            radius,
            inner_damage: facts.explosion_inner_damage as f32,
            outer_damage: facts.explosion_outer_damage.max(0) as f32,
            weapon: info.projectile.weapon,
            source: DamageSource::Projectile(info.projectile.id),
            attacker: info.projectile.owner,
            attacker_life: info.projectile.owner_life,
            killcam_entity_start_time: projectile_birth_ms(&info.projectile),
        };
        crate::damage::apply_explosion_blast(world, tick, &blast);
        crate::damage::apply_shared_glass_blast(
            world,
            tick,
            info.origin,
            facts.explosion_inner_damage,
            facts.explosion_outer_damage,
            radius,
        );
    }
}

fn push_grenade_bounce(
    world: &mut FrameWorld,
    tick: Tick,
    projectile: &ProjectileState,
    surf_type: u8,
) {
    world.push_entity_event(
        tick,
        EventAudience::All,
        entity_iw4::EntityEventKind::GRENADE_BOUNCE,
        crate::EntityEventPayload {
            number: projectile.entnum,
            weapon: projectile.weapon,
            correlation: projectile.id.0,
            origin: projectile.origin,
            event_parm: i32::from(surf_type),
            surf_type,
            ..Default::default()
        },
    );
}

fn player_attachment(
    world: &FrameWorld,
    client: ClientId,
    point: [f32; 3],
) -> Option<crate::MissileTarget> {
    let ps = world.player(client)?;
    let life = world.client_meta(client)?.life_sequence;
    let axis = math_iw4::angles_to_axis(ps.viewangles);
    let delta: [f32; 3] = core::array::from_fn(|i| point[i] - ps.origin[i]);
    let offset = axis.map(|row| (0..3).map(|i| row[i] * delta[i]).sum());
    Some(crate::MissileTarget::Player {
        client,
        life,
        offset,
    })
}

fn arm_impact_payload(
    world: &mut FrameWorld,
    tick: Tick,
    contact_time: i32,
    projectile: &mut ProjectileState,
    point: [f32; 3],
    normal: [f32; 3],
    collider: Option<ColliderId>,
) {
    let weapon = required_projectile_facts(world, projectile.weapon).impact_payload_weapon;
    let facts = required_projectile_facts(world, weapon);
    let origin = core::array::from_fn(|i| point[i] + normal[i] * 0.25);
    stick_missile(tick, projectile, origin);
    projectile.id = world.allocate_projectile_id();
    projectile.weapon = weapon;
    projectile.spawn_time_ms = contact_time;
    projectile.launch_time = contact_time;
    projectile.travel_distance = 0.0;
    projectile.guide = crate::MissileGuide::default();
    projectile.grounded = normal[2] > 0.7;
    projectile.detonate_at_ms = Some(contact_time.saturating_add(facts.fuse_time_ms.max(1)));
    projectile.cleanup_at_ms = contact_time.saturating_add(GRENADE_FUSE_CAP_MS);
    if let Some(ColliderId::Player { client, .. }) = collider {
        projectile.attached_to = player_attachment(world, client, origin);
    }
    if let Some(
        ColliderId::EntityDObjBone { owner, .. } | ColliderId::EntityLinkedBrush { owner, .. },
    ) = collider
    {
        projectile.attached_to = owner
            .script_model()
            .and_then(|id| world.gentity_number(id))
            .and_then(|number| {
                let entity = world.entity_kernel().current_ref(number).ok()?;
                let mover = world.script_mover_by_number(number)?;
                let delta: [f32; 3] = core::array::from_fn(|i| origin[i] - mover.state.tr_base[i]);
                let offset = math_iw4::angles_to_axis(mover.state.apos_tr_base)
                    .map(|row| (0..3).map(|i| row[i] * delta[i]).sum());
                Some(crate::MissileTarget::Entity { entity, offset })
            });
    }
    if let Some(ColliderId::World { glass_encoded, .. }) = collider {
        projectile.stuck_pane = (glass_encoded != 0).then(|| u32::from(glass_encoded) - 1);
    }
    perf::projectile_phase(
        weapon,
        projectile.id.0,
        "armed",
        projectile.detonate_at_ms.unwrap_or(0),
    );
    push_grenade_stick(world, tick, projectile);
}

fn push_grenade_stick(world: &mut FrameWorld, tick: Tick, projectile: &ProjectileState) {
    world
        .weapon_notes
        .push(WeaponNote::Stuck { id: projectile.id });
    world.push_entity_event(
        tick,
        EventAudience::All,
        entity_iw4::EntityEventKind::GRENADE_STICK,
        crate::EntityEventPayload {
            number: projectile.entnum,
            weapon: projectile.weapon,
            correlation: projectile.id.0,
            origin: projectile.origin,
            ..Default::default()
        },
    );
}

fn missile_glass_punch_eligible(speed: f32, surface_flags: u32, glass_encoded: u16) -> bool {
    speed >= MISSILE_GLASS_SHATTER_VEL
        && surface_type_from_flags(surface_flags) == SURF_TYPE_GLASS
        && glass_encoded != 0
}

fn park_missile_at(
    tick: Tick,
    projectile: &mut ProjectileState,
    origin: [f32; 3],
    velocity: [f32; 3],
) {
    let time = level_time_ms(tick);
    projectile.origin = origin;
    projectile.velocity = velocity;
    projectile.pos.tr_base = origin;
    projectile.pos.tr_time = time;
    projectile.pos.tr_delta = velocity;
}

fn sticks_to_surface(facts: EquipmentRuntimeFacts, normal: [f32; 3]) -> bool {
    matches!(facts.stickiness, 1 | 2) || (matches!(facts.stickiness, 3 | 4) && normal[2] > 0.7)
}

fn settle_equipment(
    world: &mut FrameWorld,
    tick: Tick,
    projectile: &mut ProjectileState,
    origin: [f32; 3],
    normal: [f32; 3],
    fraction: f32,
) {
    let facts = required_projectile_facts(world, projectile.weapon);
    let yaw = evaluate_trajectory(&projectile.apos, level_time_ms(tick))[1];
    if facts.stickiness == 3 {
        apply_missile_land_angles(world, tick, projectile, normal, fraction);
    }
    let origin = core::array::from_fn(|i| origin[i] + normal[i] * 0.25);
    stick_missile(tick, projectile, origin);
    if matches!(facts.stickiness, 2 | 4) {
        let mut angles = projectile.apos.tr_base;
        if facts.stickiness == 4 {
            angles = [math_iw4::pitch_for_yaw_on_normal(yaw, normal), yaw, 0.0];
        } else {
            let facing = forward(angles);
            let along: f32 = (0..3).map(|i| facing[i] * normal[i]).sum();
            angles =
                math_iw4::vect_to_angles(core::array::from_fn(|i| facing[i] - along * normal[i]));
        }
        let (_, right, up) = math_iw4::angle_vectors(angles);
        let side: f32 = (0..3).map(|i| normal[i] * right[i]).sum();
        let vertical: f32 = (0..3).map(|i| normal[i] * up[i]).sum();
        angles[2] = side.atan2(vertical).to_degrees();
        projectile.apos.tr_base = angles;
    }
    if !facts.timed_detonation {
        projectile.detonate_at_ms = None;
        projectile.cleanup_at_ms = i32::MAX;
    }
    push_grenade_stick(world, tick, projectile);
}

fn stick_missile(tick: Tick, projectile: &mut ProjectileState, origin: [f32; 3]) {
    let time = level_time_ms(tick);
    projectile.origin = origin;
    projectile.velocity = [0.0, 0.0, 0.0];
    projectile.pos.tr_type = TR_STATIONARY;
    projectile.pos.tr_time = time;
    projectile.pos.tr_duration = 0;
    projectile.pos.tr_base = origin;
    projectile.pos.tr_delta = [0.0, 0.0, 0.0];
    projectile.apos = Trajectory {
        tr_type: TR_STATIONARY,
        tr_time: time,
        tr_duration: 0,
        tr_base: evaluate_trajectory(&projectile.apos, time),
        tr_delta: [0.0; 3],
    };
}

fn knife_impact(
    world: &mut FrameWorld,
    tick: Tick,
    projectile: &mut ProjectileState,
    normal: [f32; 3],
    fraction: f32,
    surf_type: u8,
    player_hit: bool,
) {
    let time = level_time_ms(tick);
    let hit_time =
        time - crate::MATCH_TICK_MS as i32 + (crate::MATCH_TICK_MS as f32 * fraction) as i32;
    let facts = required_projectile_facts(world, projectile.weapon);
    if facts.ballistic_blade {
        let origin = core::array::from_fn(|i| projectile.origin[i] + normal[i] * 0.25);
        stick_missile(tick, projectile, origin);
        push_grenade_stick(world, tick, projectile);
        return;
    }
    projectile.velocity = projectile.velocity_at(hit_time);
    bounce_velocity(projectile, normal, &facts, surf_type);
    let speed = vec3_length(projectile.velocity);
    let direction = vec3_normalize(projectile.velocity).unwrap_or([0.0; 3]);
    let incidence: f32 = (0..3).map(|i| direction[i] * normal[i]).sum();
    let floor = normal[2] > 0.7;
    let stop = (player_hit && facts.stick_to_players)
        || (!player_hit && ((floor && speed < 20.0) || incidence > 0.7));
    if !stop {
        projectile.origin = core::array::from_fn(|i| {
            projectile.origin[i]
                + if i == 2 {
                    (normal[i] * 0.1).min(0.0)
                } else {
                    normal[i] * 0.1
                }
        });
        projectile.pos.tr_base = projectile.origin;
        projectile.pos.tr_time = time;
        projectile.pos.tr_delta = projectile.velocity;
        apply_missile_land_angles(world, tick, projectile, normal, fraction);
        push_grenade_bounce(world, tick, projectile, surf_type);
        return;
    }
    let mut angles = evaluate_trajectory(&projectile.apos, hit_time);
    let mut origin = projectile.origin;
    if !player_hit && floor && (speed < 20.0 || incidence < 0.7) {
        let forward = forward(angles);
        let dot: f32 = (0..3).map(|i| forward[i] * normal[i]).sum();
        angles = math_iw4::vect_to_angles(core::array::from_fn(|i| forward[i] - dot * normal[i]));
        let (_, right, up) = math_iw4::angle_vectors(angles);
        let side: f32 = (0..3).map(|i| normal[i] * right[i]).sum();
        let vertical: f32 = (0..3).map(|i| normal[i] * up[i]).sum();
        angles[2] = side.atan2(vertical).to_degrees() + 90.0;
        origin[2] -= 1.0;
    } else {
        angles[0] = normal[2].atan2(normal[0].hypot(normal[1])).to_degrees()
            + host_flrand(world.combat_rng_mut(), -15.0, 15.0);
        let forward = forward(angles);
        origin = core::array::from_fn(|i| origin[i] - normal[i] * 1.5 - forward[i] * 4.5);
    }
    stick_missile(tick, projectile, origin);
    projectile.apos.tr_base = angles;
    push_grenade_stick(world, tick, projectile);
}

fn unstick_missile(tick: Tick, projectile: &mut ProjectileState) {
    let time = level_time_ms(tick);
    projectile.velocity = [0.0, 0.0, 0.0];
    projectile.pos.tr_type = TR_GRAVITY;
    projectile.pos.tr_time = time;
    projectile.pos.tr_duration = 0;
    projectile.pos.tr_base = projectile.origin;
    projectile.pos.tr_delta = [0.0, 0.0, 0.0];
}

fn apply_missile_land_angles(
    world: &mut FrameWorld,
    tick: Tick,
    projectile: &mut ProjectileState,
    normal: [f32; 3],
    fraction: f32,
) {
    let time = level_time_ms(tick);
    let prev = time.saturating_sub(crate::MATCH_TICK_MS as i32);
    let hit_time = prev.saturating_add(((time - prev) as f32 * fraction) as i32);
    let (spin_random, wall_spin_addend) = {
        let rng = world.combat_rng_mut();
        let spin_random = rng.next_u32() as f32 * (1.0 / 4_294_967_296.0);
        let wall_spin_addend = ((rng.next_u32() & 0x7f) as i32 - 63) as f32;
        (spin_random, wall_spin_addend)
    };
    projectile.apos = missile_land_angles(MissileLandAnglesIn {
        apos: projectile.apos,
        normal,
        hit_time_ms: hit_time,
        force_align: false,
        spin_random,
        wall_spin_addend,
    })
    .apos;
}

fn bounce_missile(
    world: &mut FrameWorld,
    tick: Tick,
    projectile: &mut ProjectileState,
    origin: [f32; 3],
    normal: [f32; 3],
    fraction: f32,
    surf_type: u8,
) {
    let time = level_time_ms(tick);
    let prev = time.saturating_sub(crate::MATCH_TICK_MS as i32);
    let hit_time = prev.saturating_add(((time - prev) as f32 * fraction) as i32);
    projectile.grounded |= normal[2] > 0.7;
    projectile.velocity = projectile.velocity_at(hit_time);
    let incoming = projectile.velocity;
    let facts = required_projectile_facts(world, projectile.weapon);
    bounce_velocity(projectile, normal, &facts, surf_type);
    let outgoing = projectile.velocity;
    projectile.origin = origin;
    projectile.pos.tr_base = origin;
    projectile.pos.tr_time = time;
    projectile.pos.tr_delta = projectile.velocity;
    apply_missile_land_angles(world, tick, projectile, normal, fraction);
    let delta = vec3_length([
        outgoing[0] - incoming[0],
        outgoing[1] - incoming[1],
        outgoing[2] - incoming[2],
    ]);
    if delta > BOUNCE_EVENT_SPEED_DELTA {
        push_grenade_bounce(world, tick, projectile, surf_type);
    }
}

fn required_projectile_facts(world: &FrameWorld, weapon: u32) -> EquipmentRuntimeFacts {
    world
        .equipment_facts_for(weapon)
        .or_else(|| world.missile_launch_facts(weapon))
        .unwrap_or_else(|| {
            panic!(
                "a missile needs a validated equipment row; bounce/damage must not default to zero"
            );
        })
}

fn bounce_velocity(
    projectile: &mut ProjectileState,
    normal: [f32; 3],
    facts: &EquipmentRuntimeFacts,
    surf_type: u8,
) {
    let coefficients = facts
        .parallel_bounce
        .as_ref()
        .zip(facts.perpendicular_bounce.as_ref())
        .and_then(|(p, n)| p.get(surf_type as usize).zip(n.get(surf_type as usize)))
        .unwrap_or_else(|| panic!("missing projectile surface bounce coefficients"));
    let incoming = projectile.velocity;
    projectile.velocity =
        bounce_reflected_scale(incoming, normal, *coefficients.0, *coefficients.1);
}

fn bounce_reflected_scale(
    incoming: [f32; 3],
    normal: [f32; 3],
    parallel: f32,
    perpendicular: f32,
) -> [f32; 3] {
    let speed = vec3_length(incoming);
    let nlen = vec3_length(normal);
    if speed < 1e-6 || nlen < 1e-6 {
        return [0.0, 0.0, 0.0];
    }
    let n = [normal[0] / nlen, normal[1] / nlen, normal[2] / nlen];
    let d = incoming[0] * n[0] + incoming[1] * n[1] + incoming[2] * n[2];
    let reflected = [
        incoming[0] - 2.0 * d * n[0],
        incoming[1] - 2.0 * d * n[1],
        incoming[2] - 2.0 * d * n[2],
    ];
    let incidence = (-d / speed).clamp(0.0, 1.0);
    let factor = parallel + (perpendicular - parallel) * incidence;
    [
        reflected[0] * factor,
        reflected[1] * factor,
        reflected[2] * factor,
    ]
}

fn startsolid_normal(start: [f32; 3], end: [f32; 3]) -> [f32; 3] {
    let d = [start[0] - end[0], start[1] - end[1], start[2] - end[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len < 1e-6 {
        [0.0, 0.0, 1.0]
    } else {
        [d[0] / len, d[1] / len, d[2] / len]
    }
}

fn entity_collision_epoch(
    terminal: ColliderId,
    rows: &[EntityCollisionTraceGeom],
) -> Option<EntityCollisionEpoch> {
    let owner = match terminal {
        ColliderId::EntityDObjBone { owner, .. } | ColliderId::EntityLinkedBrush { owner, .. } => {
            owner
        }
        ColliderId::World { .. } | ColliderId::Player { .. } => return None,
    };
    rows.iter()
        .find(|row| row.owner == owner)
        .map(|row| row.epoch)
}
