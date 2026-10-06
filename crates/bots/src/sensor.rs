use math_iw4::angle_vectors;
use sim::{ClientId, ClientLifecycle, Snapshot};

use crate::observation::{
    BotObservation, Contact, DEFAULT_OBJECTIVE_RADIUS, KnowledgeSource, ModeObjective, SelfState,
    Visibility, WeaponAction, WeaponSlot,
};
use crate::query::{QuerySubsystem, SightSample, WorldQuery};

pub const SIGHT_RANGE_IN: f32 = 5906.0;
pub const SIGHT_FOV_DEG: f32 = 90.0;
const PROBE_CHEST_Z: f32 = 48.0;
const PROBE_HEAD_Z: f32 = 64.0;
const PROBE_FEET_Z: f32 = 8.0;
/// Perception's share of the shared quota, so routing and firing keep theirs.
const SIGHT_PROBES: u32 = 6;

pub fn observe(
    snapshot: &Snapshot,
    bot: ClientId,
    world: &mut impl WorldQuery,
) -> Option<BotObservation> {
    observe_focused(snapshot, bot, world, None)
}

/// `focus` is the client the bot has already committed to. It is probed before
/// the rotation spends the same allowance on acquisition candidates, so a
/// committed target is the last thing perception gives up, not the first.
pub fn observe_focused(
    snapshot: &Snapshot,
    bot: ClientId,
    world: &mut impl WorldQuery,
    focus: Option<ClientId>,
) -> Option<BotObservation> {
    let meta = snapshot.meta.for_client(bot)?;
    let ps = snapshot
        .players
        .iter()
        .find(|(id, _)| *id == bot)
        .map(|(_, ps)| ps)?;
    let team = meta.client_state_team;
    world.enter(QuerySubsystem::Perception);
    let self_state = SelfState {
        life_sequence: meta.life_sequence,
        id: bot,
        lifecycle: meta.lifecycle,
        origin: ps.origin,
        viewangles: ps.viewangles,
        delta_angles: ps.delta_angles,
        view_height: ps.view_height_current,
        stance: crate::observation::Stance::from_view_height(ps.view_height_current),
        health: ps.health,
        weapon: ps.weapon as u16,
        weapon_class: world.weapon_class(ps.weapon as u16),
        weapon_action: WeaponAction::from_weaponstate(ps.weaponstate_primary),
        ammo_clip: meta.ammo_clip,
        ammo_stock: meta.ammo_stock,
        team,
    };
    let inventory = own_inventory(ps, meta, world);
    let mut seen = Vec::new();
    let mut unsensed = Vec::new();
    if meta.lifecycle != ClientLifecycle::Alive {
        // A bot that is not alive performs no probe, so it makes no negative
        // observation either.
        unsensed.extend(
            snapshot
                .players
                .iter()
                .map(|(id, _)| *id)
                .filter(|id| *id != bot),
        );
    } else {
        let eye = [
            ps.origin[0],
            ps.origin[1],
            ps.origin[2]
                + if ps.view_height_current > 1.0 {
                    ps.view_height_current
                } else {
                    60.0
                },
        ];
        let (forward, _, _) = angle_vectors(ps.viewangles);
        let half = (SIGHT_FOV_DEG * 0.5).to_radians().cos();
        // Rotate probes so occluded clients at the front of the snapshot cannot
        // monopolize perception.
        let mut probes = SIGHT_PROBES;
        let count = snapshot.players.len();
        let start = (snapshot.tick.0 as usize / 2 + bot.0 as usize) % count.max(1);
        for index in probe_order(snapshot, start, count, focus) {
            let (id, other) = &snapshot.players[index];
            if *id == bot {
                continue;
            }
            let Some(other_meta) = snapshot.meta.for_client(*id) else {
                continue;
            };
            if other_meta.lifecycle != ClientLifecycle::Alive {
                continue;
            }
            if snapshot.meta.kind.is_team() && other_meta.client_state_team == team {
                continue;
            }
            // Out of range or outside the cone is a complete answer on its own;
            // only a client that needed a probe it did not get is unsensed.
            if !in_range_and_fov(ps.origin, other.origin, forward, half) {
                continue;
            }
            if probes == 0 {
                unsensed.push(*id);
                continue;
            }
            match probe_visible(world, bot, eye, other.origin, *id, &mut probes) {
                Visibility::Seen => seen.push(Contact {
                    id: *id,
                    origin: other.origin,
                    source: KnowledgeSource::CurrentlySeen,
                    seen_tick: snapshot.tick.0,
                    confidence: 1.0,
                }),
                Visibility::NotSeen => {}
                Visibility::Unknown => unsensed.push(*id),
            }
        }
    }
    let objectives = public_objectives(snapshot, bot, team);
    Some(BotObservation {
        tick: snapshot.tick.0,
        time_ms: snapshot.tick.0 as i32 * 50,
        self_state,
        inventory,
        seen,
        unsensed,
        events: Vec::new(),
        objective: objectives
            .iter()
            .copied()
            .min_by(|a, b| dist2(ps.origin, a.origin).total_cmp(&dist2(ps.origin, b.origin))),
        objectives,
    })
}

fn in_range_and_fov(from: [f32; 3], to: [f32; 3], forward: [f32; 3], min_dot: f32) -> bool {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let len2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if len2 > SIGHT_RANGE_IN * SIGHT_RANGE_IN || len2 <= 0.0001 {
        return false;
    }
    let len = len2.sqrt();
    let dir = [d[0] / len, d[1] / len, d[2] / len];
    dir[0] * forward[0] + dir[1] * forward[1] + dir[2] * forward[2] >= min_dot
}

/// The bot's own carried weapons with the ammunition the match state holds for
/// them. Enemy inventories and positions are never read here.
fn own_inventory(
    ps: &playerstate_iw4::PlayerState,
    meta: &sim::ClientSnapshotMeta,
    world: &impl WorldQuery,
) -> Vec<WeaponSlot> {
    ps.weapons
        .iter()
        .copied()
        .filter(|&id| id > 0)
        .filter_map(|id| {
            let weapon = u16::try_from(id).ok()?;
            let facts = world.weapon_facts(weapon)?;
            let (clip, stock) = meta
                .ammo_by_weapon
                .iter()
                .find(|(owned, _, _)| *owned == id as u32)
                .map_or((0, 0), |(_, clip, stock)| (*clip, *stock));
            Some(WeaponSlot {
                weapon,
                class: world.weapon_class(weapon),
                facts,
                clip,
                stock,
            })
        })
        .collect()
}

/// Snapshot indices in the order perception spends its allowance: the committed
/// target first, then the tick's rotation.
fn probe_order(
    snapshot: &Snapshot,
    start: usize,
    count: usize,
    focus: Option<ClientId>,
) -> Vec<usize> {
    let mut order: Vec<usize> = (0..count).map(|offset| (start + offset) % count).collect();
    if let Some(focus) = focus
        && let Some(at) = order
            .iter()
            .position(|index| snapshot.players[*index].0 == focus)
    {
        let first = order.remove(at);
        order.insert(0, first);
    }
    order
}

fn probe_visible(
    world: &mut impl WorldQuery,
    bot: ClientId,
    eye: [f32; 3],
    origin: [f32; 3],
    target: ClientId,
    remaining: &mut u32,
) -> Visibility {
    let probes = [
        [origin[0], origin[1], origin[2] + PROBE_CHEST_Z],
        [origin[0], origin[1], origin[2] + PROBE_HEAD_Z],
        [origin[0], origin[1], origin[2] + PROBE_FEET_Z],
    ];
    // One positive probe settles the question; a negative one settles it only
    // when every probe actually ran and came back classified.
    let mut complete = true;
    for probe in probes {
        if *remaining == 0 {
            return Visibility::Unknown;
        }
        *remaining -= 1;
        match world.sight_ray(eye, probe, bot) {
            Ok(SightSample::Clear) => return Visibility::Seen,
            Ok(SightSample::HitPlayer { client, .. }) if client == target => {
                return Visibility::Seen;
            }
            Err(_) => {
                // The shared quota is gone; stop attributing denials to this bot.
                *remaining = 0;
                return Visibility::Unknown;
            }
            Ok(SightSample::Unknown) => complete = false,
            Ok(_) => {}
        }
    }
    if complete {
        Visibility::NotSeen
    } else {
        Visibility::Unknown
    }
}

fn public_objectives(snapshot: &Snapshot, bot: ClientId, team: i32) -> Vec<ModeObjective> {
    let Some(team) = gamemode_iw4::Team::from_packed_u8(team as u8) else {
        return Vec::new();
    };
    let feet = snapshot
        .players
        .iter()
        .find(|(id, _)| *id == bot)
        .map(|(_, p)| p.origin);
    snapshot
        .meta
        .objectives
        .compass
        .iter()
        .filter(|o| {
            o.shows_to(team, bot.0)
                && !o.icon.contains("defend")
                && !o.icon.starts_with("compassping")
        })
        .map(|o| ModeObjective {
            id: u32::from(o.index),
            origin: o.origin,
            touching: feet.is_some_and(|f| {
                dist2([f[0], f[1], 0.0], [o.origin[0], o.origin[1], 0.0])
                    <= DEFAULT_OBJECTIVE_RADIUS * DEFAULT_OBJECTIVE_RADIUS
            }),
            ..ModeObjective::at(o.origin)
        })
        .collect()
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}
