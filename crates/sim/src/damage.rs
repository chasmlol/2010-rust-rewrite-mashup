use crate::frame::FrameWorld;
use crate::identities::{DamageSource, LifeSequence, PelletId};
use crate::match_state::ClientLifecycle;
use crate::world::{ClientId, Tick};
use crate::world_objects::{GlassPaneBasis, GlassPieceId};
use gamemode_iw4::{
    G_CAN_DAMAGE_CONTENTS_MASK, can_damage_player_vis_scale, radius_damage_amount,
    radius_damage_distance_to_aabb,
};

const AREA_ENTITY_CAPACITY: usize = 0x800;

const _: () = assert!(crate::MATCH_TICK_MS == 50);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageAttempt {
    pub splash: bool,
    pub source: crate::DamageSource,
    pub pellet: PelletId,
    pub attacker: ClientId,
    pub attacker_life: LifeSequence,
    pub target: ClientId,
    pub target_life: LifeSequence,
    pub weapon: u32,
    pub amount: i32,

    pub killcam_entity_start_time: i32,

    pub inflictor_origin: Option<[f32; 3]>,

    pub hitloc: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageRefusal {
    FriendlyFire,
    NonPositive,
    MissingTarget,
    TargetNotAlive,
    StaleLife,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeathCommit {
    pub victim: ClientId,
    pub victim_life: LifeSequence,
    pub attacker: ClientId,
    pub attacker_life: LifeSequence,
    pub source: crate::DamageSource,
    pub pellet: PelletId,
    pub weapon: u32,
    pub amount: i32,
    pub killcam_entity_start_time: i32,
    pub inflictor_origin: Option<[f32; 3]>,
    pub hitloc: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DamageOutcome {
    Refused(DamageRefusal),
    Nonlethal { health_after: i32 },
    Died(DeathCommit),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ExplosionBlast {
    pub cone: Option<([f32; 3], f32)>,
    pub origin: [f32; 3],
    pub radius: f32,
    pub inner_damage: f32,
    pub outer_damage: f32,
    pub weapon: u32,
    pub source: DamageSource,
    pub attacker: ClientId,
    pub attacker_life: LifeSequence,
    pub killcam_entity_start_time: i32,
}

impl ExplosionBlast {
    fn contains(&self, point: [f32; 3]) -> bool {
        self.cone.is_none_or(|(forward, cosine)| {
            let delta: [f32; 3] = std::array::from_fn(|i| point[i] - self.origin[i]);
            let len = delta.iter().map(|v| v * v).sum::<f32>().sqrt();
            len <= f32::EPSILON
                || delta.iter().zip(forward).map(|(a, b)| a * b).sum::<f32>() >= cosine * len
        })
    }
}

struct GlassBlastHit {
    id: GlassPieceId,
    amount: u32,
    hit: [f32; 3],
    dir: [f32; 3],
}

pub(crate) fn apply_explosion_blast(world: &mut FrameWorld, tick: Tick, blast: &ExplosionBlast) {
    if !world.publishes_snapshot() {
        return;
    }
    if crate::voxel::active() {
        crate::voxel::push_explosion(blast.origin);
    }
    let attempts = radius_player_attempts(world, blast);
    let glass = radius_glass_hits(world, blast);
    for attempt in &attempts {
        let _ = apply_damage_attempt(world, tick, attempt);
    }
    apply_glass_blast_hits(world, tick, glass);
    apply_entity_blast(world, blast);
    crate::t5_destructible::apply_radius(
        world,
        tick,
        &crate::t5_destructible::RadiusDamage {
            origin: blast.origin,
            radius: blast.radius,
            inner: blast.inner_damage,
            outer: blast.outer_damage,
            attacker: Some(blast.attacker),
            exclude: None,
            cone: blast.cone,
        },
    );
    let means = crate::script_player::means(world, blast.source, blast.weapon, 0, true);
    let ignore_model = world
        .ecs()
        .get_resource::<crate::script::Runtime>()
        .and_then(|runtime| match blast.source {
            DamageSource::Projectile(id) => runtime
                .missiles
                .get(&id)
                .and_then(|object| runtime.entities.get(object))
                .and_then(|entity| entity.presence),
            DamageSource::Radius(id) => Some(id),
            _ => None,
        });
    crate::script::host::triggers::damage_blast(
        world.ecs(),
        &crate::script::host::triggers::TriggerBlast {
            origin: blast.origin,
            radius: blast.radius,
            max: blast.inner_damage,
            min: blast.outer_damage,
            client: Some(blast.attacker),
            missile: match blast.source {
                DamageSource::Projectile(id) => Some(id),
                _ => None,
            },
            means,
            ignore_model,
            cone: blast.cone,
        },
    );
}

/// Damage the Minecraft world's mobs dealt players this tick, as world
/// damage the player does to itself.
pub(crate) fn apply_block_world_damage(world: &mut FrameWorld, tick: Tick) {
    if !world.publishes_snapshot() {
        return;
    }
    for (client, amount, from) in crate::voxel::take_player_damage() {
        let target = ClientId(client);
        let Some(meta) = world.client_meta(target) else {
            continue;
        };
        if meta.lifecycle != ClientLifecycle::Alive || amount <= 0 {
            continue;
        }
        let life = meta.life_sequence;
        let attempt = DamageAttempt {
            source: crate::DamageSource::Melee,
            pellet: crate::PelletId(0),
            attacker: target,
            attacker_life: life,
            target,
            target_life: life,
            weapon: 0,
            amount,
            killcam_entity_start_time: 0,
            inflictor_origin: from,
            hitloc: 0,
            splash: false,
        };
        let _ = apply_damage_attempt(world, tick, &attempt);
    }
}

fn apply_entity_blast(world: &mut FrameWorld, blast: &ExplosionBlast) {
    if blast.radius <= 0.0 {
        return;
    }
    let means = crate::script_player::means(world, blast.source, blast.weapon, 0, true);
    for (target, mid, dist) in
        crate::script::radius_targets(world.ecs(), blast.origin, blast.radius)
    {
        if !blast.contains(mid) {
            continue;
        }
        let amount = radius_damage_amount(
            blast.inner_damage,
            blast.outer_damage,
            blast.radius,
            dist,
            1.0,
        );
        if amount <= 0 {
            continue;
        }
        crate::script::damage_entity(
            world.ecs(),
            &crate::script::EntityHit {
                target,
                amount,
                attacker: Some(blast.attacker),
                means,
                weapon: blast.weapon,
                point: mid,
                dir: std::array::from_fn(|i| mid[i] - blast.origin[i]),
                bone: None,
                flags: IDFLAGS_RADIUS,
            },
        );
    }
}

const IDFLAGS_RADIUS: i32 = 1;

pub(crate) fn apply_script_blast(
    world: &mut FrameWorld,
    tick: Tick,
    blast: &crate::script::ScriptBlast,
) {
    if blast.radius <= 0.0 {
        return;
    }
    if world.publishes_snapshot() {
        crate::script::host::triggers::damage_blast(
            world.ecs(),
            &crate::script::host::triggers::TriggerBlast {
                origin: blast.origin,
                radius: blast.radius,
                max: blast.max,
                min: blast.min,
                client: blast.attacker,
                missile: None,
                means: blast.means,
                ignore_model: blast.inflictor,
                cone: None,
            },
        );
    }
    for target in radius_player_candidates(world, blast.origin, blast.radius) {
        let Some(meta) = world.client_meta(target) else {
            continue;
        };
        if meta.lifecycle != ClientLifecycle::Alive {
            continue;
        }
        let victim_life = meta.life_sequence;
        let Some(bounds) = world.player_area_bounds(target) else {
            continue;
        };
        let dist = radius_damage_distance_to_aabb(blast.origin, bounds.mid(), bounds.half());
        let vis_scale = player_radius_vis_scale(
            world,
            blast.origin,
            target,
            blast.inflictor.map(crate::AuthorityModelOwner::ScriptModel),
        );
        let amount = radius_damage_amount(blast.max, blast.min, blast.radius, dist, vis_scale);
        if amount <= 0 {
            continue;
        }
        let victim_origin = world.player(target).map_or(bounds.mid(), |ps| ps.origin);
        let dir: [f32; 3] = std::array::from_fn(|i| victim_origin[i] - blast.origin[i]);
        let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        let commit = blast.attacker.and_then(|attacker| {
            Some(DeathCommit {
                victim: target,
                victim_life,
                attacker,
                attacker_life: world.client_meta(attacker)?.life_sequence,
                source: DamageSource::Radius(
                    blast
                        .inflictor
                        .unwrap_or(crate::ScriptModelId::from_wire(u32::MAX)),
                ),
                pellet: PelletId(0),
                weapon: blast.weapon,
                amount,
                killcam_entity_start_time: 0,
                inflictor_origin: Some(blast.origin),
                hitloc: 0,
            })
        });
        let hit = crate::script_player::Hit {
            victim: target,
            attacker: blast.attacker,
            amount,
            flags: IDFLAGS_RADIUS,
            means: blast.means,
            weapon: blast.weapon,
            point: blast.origin,
            dir: if len > 0.0 {
                dir.map(|c| c / len)
            } else {
                [0.0; 3]
            },
            hitloc: 0,
            inflictor: None,
            commit,
        };
        crate::script::player_damage(world.ecs(), tick, &hit);
    }
    crate::t5_destructible::apply_radius(
        world,
        tick,
        &crate::t5_destructible::RadiusDamage {
            origin: blast.origin,
            radius: blast.radius,
            inner: blast.max,
            outer: blast.min,
            attacker: blast.attacker,
            exclude: blast.inflictor,
            cone: None,
        },
    );
    apply_shared_glass_blast(
        world,
        tick,
        blast.origin,
        blast.max as i32,
        blast.min as i32,
        blast.radius,
    );
}

pub(crate) fn apply_script_hit(world: &mut FrameWorld, tick: Tick, hit: &crate::script::ScriptHit) {
    let crate::script::HitTarget::Player(target) = hit.target else {
        return;
    };
    if hit.amount <= 0 {
        return;
    }
    let Some(meta) = world.client_meta(target) else {
        return;
    };
    if meta.lifecycle != ClientLifecycle::Alive {
        return;
    }
    let victim_life = meta.life_sequence;
    let Some(victim_origin) = world.player(target).map(|ps| ps.origin) else {
        return;
    };
    let dir: [f32; 3] = std::array::from_fn(|i| victim_origin[i] - hit.origin[i]);
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    let commit = hit.attacker.and_then(|attacker| {
        Some(DeathCommit {
            victim: target,
            victim_life,
            attacker,
            attacker_life: world.client_meta(attacker)?.life_sequence,
            source: DamageSource::Radius(
                hit.inflictor
                    .unwrap_or(crate::ScriptModelId::from_wire(u32::MAX)),
            ),
            pellet: PelletId(0),
            weapon: hit.weapon,
            amount: hit.amount,
            killcam_entity_start_time: 0,
            inflictor_origin: Some(hit.origin),
            hitloc: hit.hitloc,
        })
    });
    let player_hit = crate::script_player::Hit {
        victim: target,
        attacker: hit.attacker,
        amount: hit.amount,
        flags: hit.flags,
        means: hit.means,
        weapon: hit.weapon,
        point: hit.origin,
        dir: if len > 0.0 {
            dir.map(|c| c / len)
        } else {
            [0.0; 3]
        },
        hitloc: hit.hitloc,
        inflictor: None,
        commit,
    };
    crate::script::player_damage(world.ecs(), tick, &player_hit);
}

fn radius_player_attempts(world: &FrameWorld, blast: &ExplosionBlast) -> Vec<DamageAttempt> {
    let mut intents = Vec::new();
    if blast.radius <= 0.0 {
        return intents;
    }
    for target in radius_player_candidates(world, blast.origin, blast.radius) {
        let Some(meta) = world.client_meta(target) else {
            continue;
        };
        let Some(bounds) = world.player_area_bounds(target) else {
            continue;
        };
        if !blast.contains(bounds.mid()) {
            continue;
        }
        let dist = radius_damage_distance_to_aabb(blast.origin, bounds.mid(), bounds.half());
        let vis_scale = player_radius_vis_scale(world, blast.origin, target, None);
        let amount = radius_damage_amount(
            blast.inner_damage,
            blast.outer_damage,
            blast.radius,
            dist,
            vis_scale,
        );
        if amount <= 0 {
            continue;
        }
        intents.push(DamageAttempt {
            splash: true,
            source: blast.source,
            pellet: PelletId(0),
            attacker: blast.attacker,
            attacker_life: blast.attacker_life,
            target,
            target_life: meta.life_sequence,
            weapon: blast.weapon,
            amount,
            killcam_entity_start_time: blast.killcam_entity_start_time,
            inflictor_origin: Some(blast.origin),
            hitloc: 0,
        });
    }
    intents
}

fn radius_glass_hits(world: &FrameWorld, blast: &ExplosionBlast) -> Vec<GlassBlastHit> {
    let mut hits = Vec::new();
    if blast.radius <= 0.0 {
        return hits;
    }
    for (id, pane) in world.world_objects().glass_radius_targets() {
        let (mid, half) = glass_pane_aabb(pane);
        if !blast.contains(mid) {
            continue;
        }
        let dist = radius_damage_distance_to_aabb(blast.origin, mid, half);
        let amount = radius_damage_amount(
            blast.inner_damage,
            blast.outer_damage,
            blast.radius,
            dist,
            1.0,
        );
        if amount <= 0 {
            continue;
        }
        let dir = [
            mid[0] - blast.origin[0],
            mid[1] - blast.origin[1],
            mid[2] - blast.origin[2],
        ];
        hits.push(GlassBlastHit {
            id,
            amount: amount as u32,
            hit: mid,
            dir,
        });
    }
    hits
}

fn apply_glass_blast_hits(world: &mut FrameWorld, tick: Tick, hits: Vec<GlassBlastHit>) {
    if hits.is_empty() {
        return;
    }
    let at_time_ms = i32::try_from(tick.0.saturating_mul(crate::MATCH_TICK_MS)).unwrap_or(i32::MAX);
    let mut holdrand = *world.stuck_holdrand_mut();
    for hit in hits {
        world.world_objects_mut().apply_glass_hit(
            hit.id,
            hit.amount,
            at_time_ms,
            hit.hit,
            hit.dir,
            &mut || crate::item::random_unit(&mut holdrand),
        );
    }
    *world.stuck_holdrand_mut() = holdrand;
}

fn glass_pane_aabb(pane: GlassPaneBasis) -> ([f32; 3], [f32; 3]) {
    let mut min = pane.origin;
    let mut max = pane.origin;
    for corner in [
        [
            pane.origin[0] + pane.axis_s[0],
            pane.origin[1] + pane.axis_s[1],
            pane.origin[2] + pane.axis_s[2],
        ],
        [
            pane.origin[0] + pane.axis_t[0],
            pane.origin[1] + pane.axis_t[1],
            pane.origin[2] + pane.axis_t[2],
        ],
        [
            pane.origin[0] + pane.axis_s[0] + pane.axis_t[0],
            pane.origin[1] + pane.axis_s[1] + pane.axis_t[1],
            pane.origin[2] + pane.axis_s[2] + pane.axis_t[2],
        ],
    ] {
        for i in 0..3 {
            min[i] = min[i].min(corner[i]);
            max[i] = max[i].max(corner[i]);
        }
    }
    let mid = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    let half = [
        (max[0] - min[0]) * 0.5,
        (max[1] - min[1]) * 0.5,
        (max[2] - min[2]) * 0.5,
    ];
    (mid, half)
}

pub(crate) fn radius_player_candidates(
    world: &FrameWorld,
    origin: [f32; 3],
    radius: f32,
) -> Vec<ClientId> {
    let area_half = gamemode_iw4::radius_damage_area_half_extent(radius);
    let Ok(query) = clipmap_iw4::AreaBounds::from_mid_half(origin, [area_half; 3]) else {
        diag::warn!(
            Sim,
            "radius damage at {origin:?} r={radius} has no valid area"
        );
        return Vec::new();
    };
    world
        .area_entity_candidates(query, u32::MAX, AREA_ENTITY_CAPACITY)
        .into_iter()
        .map(|entity_num| ClientId(u32::from(entity_num)))
        .collect()
}

pub(crate) fn apply_shared_glass_blast(
    world: &mut FrameWorld,
    tick: Tick,
    origin: [f32; 3],
    inner_damage: i32,
    outer_damage: i32,
    radius: f32,
) {
    let time = crate::level_time_ms(tick);
    let mut holdrand = *world.stuck_holdrand_mut();
    world.world_objects_mut().apply_glass_blast(
        origin,
        inner_damage,
        outer_damage,
        radius,
        time,
        &mut || crate::item::random_unit(&mut holdrand),
    );
    *world.stuck_holdrand_mut() = holdrand;
}

fn player_radius_vis_scale(
    world: &FrameWorld,
    inflictor: [f32; 3],
    target: ClientId,
    exclude: Option<crate::AuthorityModelOwner>,
) -> f32 {
    let Some(ps) = world.player(target) else {
        return 1.0;
    };
    let (_, right, _) = math_iw4::angle_vectors(ps.viewangles);
    can_damage_player_vis_scale(
        ps.origin,
        ps.view_height_current,
        right,
        inflictor,
        |start, end| {
            let trace = world.trace_world_except(start, end, G_CAN_DAMAGE_CONTENTS_MASK, exclude);
            t_trace_passed(&trace)
        },
    )
}

fn t_trace_passed(trace: &trace_iw4::Trace) -> bool {
    trace.fraction >= 1.0 && trace.startsolid == 0
}

pub(crate) fn apply_flashbang_blast(
    world: &mut FrameWorld,
    origin: [f32; 3],
    radius_max: f32,
    radius_min: f32,
    attacker: ClientId,
) {
    let min_r = radius_min.max(1.0);
    let max_r = if radius_max < min_r {
        min_r
    } else {
        radius_max
    };
    let mut hits = Vec::new();
    for target in radius_player_candidates(world, origin, max_r) {
        let Some(meta) = world.client_meta(target) else {
            continue;
        };
        if meta.lifecycle != ClientLifecycle::Alive {
            continue;
        }
        let Some(ps) = world.player(target) else {
            continue;
        };
        let dist = {
            let dx = ps.origin[0] - origin[0];
            let dy = ps.origin[1] - origin[1];
            let dz = ps.origin[2] - origin[2];
            (dx * dx + dy * dy + dz * dz).sqrt()
        };
        if dist > max_r {
            continue;
        }
        if player_radius_vis_scale(world, origin, target, None) <= 0.0 {
            continue;
        }
        let amount_distance = flashbang_amount_distance(dist, min_r, max_r);
        let view_origin = [
            ps.origin[0],
            ps.origin[1],
            ps.origin[2] + ps.view_height_current,
        ];
        let (forward, _, _) = math_iw4::angle_vectors(ps.viewangles);
        let to_blast = [
            origin[0] - view_origin[0],
            origin[1] - view_origin[1],
            origin[2] - view_origin[2],
        ];
        let len =
            (to_blast[0] * to_blast[0] + to_blast[1] * to_blast[1] + to_blast[2] * to_blast[2])
                .sqrt();
        let amount_angle = if len > 0.0 {
            flashbang_amount_angle(
                (to_blast[0] * forward[0] + to_blast[1] * forward[1] + to_blast[2] * forward[2])
                    / len,
            )
        } else {
            flashbang_amount_angle(1.0)
        };
        hits.push((target, amount_distance, amount_angle));
    }
    for (target, amount_distance, amount_angle) in hits {
        crate::script::flashbang(
            world.ecs(),
            target,
            origin,
            amount_distance,
            amount_angle,
            attacker,
        );
    }
}

fn flashbang_amount_distance(dist: f32, min_r: f32, max_r: f32) -> f32 {
    if dist <= min_r || max_r <= min_r {
        1.0
    } else {
        1.0 - (dist - min_r) / (max_r - min_r)
    }
}

fn flashbang_amount_angle(dot: f32) -> f32 {
    (dot + 1.0) * 0.5
}

pub(crate) fn apply_damage_attempt(
    world: &mut FrameWorld,
    tick: Tick,
    intent: &DamageAttempt,
) -> DamageOutcome {
    if intent.amount <= 0 {
        return DamageOutcome::Refused(DamageRefusal::NonPositive);
    }
    let Some(meta) = world.client_meta(intent.target) else {
        return DamageOutcome::Refused(DamageRefusal::MissingTarget);
    };
    if meta.lifecycle != ClientLifecycle::Alive {
        return DamageOutcome::Refused(DamageRefusal::TargetNotAlive);
    }
    if meta.life_sequence != intent.target_life {
        return DamageOutcome::Refused(DamageRefusal::StaleLife);
    }
    let mut amount = intent.amount;
    if intent.hitloc != crate::shield::HITLOC
        && !matches!(intent.source, crate::DamageSource::Melee)
    {
        let scale = world
            .combat_facts_for(intent.weapon)
            .map(|facts| facts.location_scale(intent.hitloc))
            .unwrap_or(1.0);
        amount = (amount as f32 * scale) as i32;
    }
    if amount <= 0 {
        return DamageOutcome::Refused(DamageRefusal::NonPositive);
    }
    crate::script_player::damage(world, tick, intent, amount)
}

pub(crate) fn play_death(
    world: &mut FrameWorld,
    victim: ClientId,
    attacker: Option<ClientId>,
    commit: Option<&DeathCommit>,
) -> bool {
    let Some(self_ps) = world.player(victim).copied() else {
        return false;
    };
    let attacker_origin = attacker
        .filter(|&id| id != victim)
        .and_then(|id| world.player(id).map(|p| p.origin));
    let yaw =
        playerstate_iw4::look_at_killer_yaw(attacker_origin, self_ps.origin, self_ps.viewangles[1]);

    let dead_viewangles = [0.0, self_ps.viewangles[1], 0.0];

    let inflictor_origin = commit.and_then(|death| death.inflictor_origin);
    let hitscan_origin = match commit.map(|death| death.source) {
        Some(crate::DamageSource::Shot(_)) | Some(crate::DamageSource::Melee) => attacker_origin,
        _ => None,
    };
    let mt =
        crate::player_anim_script::event_anim_movetype(&self_ps, world.last_anim_movetype(victim));
    let (view, primary) = crate::pmove_anim_weapon_ids(&self_ps);
    let mut conds = crate::anim_conditions_from_pmove(
        &self_ps,
        world.combat_facts_for(view),
        world.combat_facts_for(primary),
        Some(mt),
        world.last_anim_strafing(victim),
        world.anim_command_buttons(victim),
    );
    if let Some(commit) = commit {
        let blast_origin = inflictor_origin.or(hitscan_origin);
        let dist_sq = blast_origin.map(|origin| {
            let dx = origin[0] - self_ps.origin[0];
            let dy = origin[1] - self_ps.origin[1];
            let dz = origin[2] - self_ps.origin[2];
            dx * dx + dy * dy + dz * dz
        });
        let weap_class = world.combat_facts_for(commit.weapon).map(|f| f.weap_class);
        let offhand_class = world
            .equipment_facts_for(commit.weapon)
            .map(|f| f.offhand_class);
        conds.set_value(
            anim_iw4::ANIM_COND_DAMAGETYPE,
            anim_script_damagetype(commit.source, weap_class, offhand_class, dist_sq),
        );
        conds.set_value(
            anim_iw4::ANIM_COND_HITLOCATION,
            u32::from(anim_iw4::anim_script_hit_location(commit.hitloc)),
        );

        let hitdir = match blast_origin {
            Some(origin) => {
                let v_dir = [self_ps.origin[0] - origin[0], self_ps.origin[1] - origin[1]];
                let (forward, _, _) = math_iw4::angle_vectors(self_ps.viewangles);
                anim_iw4::anim_script_hit_direction([forward[0], forward[1]], v_dir)
            }
            None => 0,
        };
        conds.set_value(anim_iw4::ANIM_COND_HITDIRECTION, u32::from(hitdir));
    }

    let script = world.player_anim_script();
    let mut seed = world.anim_event_seed();
    if let Some(script) = script.as_ref() {
        if let Some(ps) = world.player_mut(victim) {
            ps.viewangles = dead_viewangles;
            ps.pm_type = playerstate_iw4::PM_TYPE_DEAD;
            let _ = script.apply_event(ps, anim_iw4::ANIM_ET_DEATH, &conds, &mut seed, true);
        }
    } else if let Some(ps) = world.player_mut(victim) {
        ps.viewangles = dead_viewangles;
        ps.pm_type = playerstate_iw4::PM_TYPE_DEAD;
    }
    world.set_anim_event_seed(seed);
    world.client_meta_mut(victim).look_at_killer_yaw = yaw;
    true
}

pub(crate) fn force_death_anim(
    world: &mut FrameWorld,
    victim: ClientId,
    weapon: Option<u32>,
    explosive_mod: bool,
    inflictor_origin: Option<[f32; 3]>,
    hitloc: u8,
    v_dir: [f32; 3],
) {
    let Some(self_ps) = world.player(victim).copied() else {
        return;
    };
    let mt =
        crate::player_anim_script::event_anim_movetype(&self_ps, world.last_anim_movetype(victim));
    let (view, primary) = crate::pmove_anim_weapon_ids(&self_ps);
    let mut conds = crate::anim_conditions_from_pmove(
        &self_ps,
        world.combat_facts_for(view),
        world.combat_facts_for(primary),
        Some(mt),
        world.last_anim_strafing(victim),
        world.anim_command_buttons(victim),
    );
    let weap_class = weapon.and_then(|w| world.combat_facts_for(w).map(|f| f.weap_class));
    let offhand_class = weapon.and_then(|w| world.equipment_facts_for(w).map(|f| f.offhand_class));
    let damagetype = if offhand_class == Some(1) {
        let near = inflictor_origin.is_some_and(|o| {
            let d = [
                o[0] - self_ps.origin[0],
                o[1] - self_ps.origin[1],
                o[2] - self_ps.origin[2],
            ];
            d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < anim_iw4::ANIM_DAMAGE_EXPLOSION_NEAR_DIST_SQ
        });
        if near { 2 } else { 1 }
    } else if explosive_mod {
        2
    } else if weap_class == Some(4) {
        1
    } else {
        0
    };
    conds.set_value(anim_iw4::ANIM_COND_DAMAGETYPE, damagetype);
    conds.set_value(
        anim_iw4::ANIM_COND_HITLOCATION,
        u32::from(anim_iw4::anim_script_hit_location(hitloc)),
    );
    let (forward, _, _) = math_iw4::angle_vectors(self_ps.viewangles);
    conds.set_value(
        anim_iw4::ANIM_COND_HITDIRECTION,
        u32::from(anim_iw4::anim_script_hit_direction(
            [forward[0], forward[1]],
            [v_dir[0], v_dir[1]],
        )),
    );
    let script = world.player_anim_script();
    let mut seed = world.anim_event_seed();
    if let Some(script) = script.as_ref()
        && let Some(ps) = world.player_mut(victim)
    {
        let _ = script.apply_event(ps, anim_iw4::ANIM_ET_DEATH, &conds, &mut seed, true);
    }
    world.set_anim_event_seed(seed);
}

fn anim_script_damagetype(
    source: crate::DamageSource,
    weap_class: Option<i32>,
    offhand_class: Option<i32>,
    dist_sq: Option<f32>,
) -> u32 {
    match source {
        crate::DamageSource::Projectile(_) | crate::DamageSource::Radius(_) => {
            if offhand_class == Some(1) {
                if dist_sq.is_some_and(|d| d < anim_iw4::ANIM_DAMAGE_EXPLOSION_NEAR_DIST_SQ) {
                    2
                } else {
                    1
                }
            } else {
                2
            }
        }
        crate::DamageSource::Shot(_) => {
            if weap_class == Some(4) {
                1
            } else {
                0
            }
        }
        crate::DamageSource::Melee => 0,
    }
}
