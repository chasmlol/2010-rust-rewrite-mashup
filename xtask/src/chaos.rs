use std::path::{Path, PathBuf};

use crate::perfetto_query::{
    Table, cell, cell_f32, cell_i32, cell_i64, resolve_processor, resolve_trace, run_sql_file,
};
use crate::scenario::Claim;

const TRUCK_ROOF: [f32; 3] = [-1066.0, 1391.0, 7.0];

const ROOF_XY_SLACK: f32 = 200.0;
const ROOF_Z_MIN: f32 = -50.0;
const ROOF_Z_MAX: f32 = 80.0;

const PITCH_DOWN_MIN: f32 = 80.0;
const PITCH_DOWN_MAX: f32 = 90.0;

const MIN_DEAD_MS: i32 = 1500;

const AUTHORITY_MS: i32 = 50;

const ALIVE_SPLIT_UNITS: f32 = 100.0;

const CORPSE_Z_FLOOR: f32 = -10_000.0;
const TRUCK_IDS: [i64; 3] = [234, 179, 163];

struct Trace {
    ticks: Vec<TickRow>,
    deaths: Vec<DeathRow>,
    projectiles: usize,
    trucks: Vec<TruckRow>,
    feel: Vec<FeelRow>,
    corpses: Vec<CorpseRow>,
    items: Vec<ItemRow>,
    pickups: Vec<PickupRow>,
    remotes: Vec<RemoteRow>,
    lighting_fail: i64,
}

struct TickRow {
    client_id: i64,
    is_bot: bool,
    time_ms: i32,
    level_time_ms: Option<i32>,
    origin_x: Option<f32>,
    origin_y: Option<f32>,
    origin_z: Option<f32>,
    pitch: Option<f32>,
    lifecycle: Option<String>,
}

struct DeathRow {
    victim: i64,
    attacker: Option<i64>,
    suicide: i64,
}

struct TruckRow {
    script_model_id: Option<i64>,
    state: Option<i64>,
    health: Option<i64>,
    death_clip: Option<String>,
    present_gap: Option<String>,
}

struct FeelRow {
    time_ms: Option<i32>,
    lifecycle: Option<String>,
    fanout_seat_applied: Option<i64>,
    seat_lookup_tick: Option<i64>,
    present_choice: Option<String>,
    present_snapshot_delta_time: Option<i64>,
    authority_origin_x: Option<f32>,
    authority_origin_y: Option<f32>,
    adopted_origin_x: Option<f32>,
    adopted_origin_y: Option<f32>,
}

struct CorpseRow {
    occupied: Option<i64>,
    origin_z: Option<f32>,
    pose_e_type: Option<i64>,
    legs_leaf_name: Option<String>,
}

struct ItemRow {
    e_type: Option<i64>,
    clip_r: Option<i64>,
    scavenger: Option<i64>,
    present_gap: Option<String>,
}

struct PickupRow {
    event: Option<String>,
    picker_pm_type: Option<i64>,
}

struct RemoteRow {
    client: Option<i64>,
    time_ms: Option<i32>,
    pose_e_type: Option<i64>,
    origin_x: Option<f32>,
    origin_y: Option<f32>,
    snap_origin_x: Option<f32>,
    snap_origin_y: Option<f32>,
    proxy_outcome: Option<String>,
}

pub fn chaos_gate(root: &Path, trace: Option<PathBuf>) -> bool {
    crate::hr("chaos — truck + RPG into the floor, .pftrace");
    let path = match resolve_trace(root, trace.as_deref()) {
        Ok(path) => path,
        Err(error) => {
            println!("  make chaos");
            return crate::scenario::harness_failure(&error);
        }
    };
    let loaded = match load(root, &path) {
        Ok(loaded) => loaded,
        Err(error) => {
            return crate::scenario::harness_failure(&error);
        }
    };
    let claims = check(&loaded);
    let mut ok = true;
    for claim in &claims {
        ok &= claim.report();
    }
    ok
}

fn load(root: &Path, trace: &Path) -> Result<Trace, String> {
    let processor = resolve_processor(root)?;
    crate::perfetto_query::ensure_trace_health(root, &processor, trace)?;
    let ticks = parse_ticks(&run_sql_file(
        root,
        &processor,
        trace,
        "xtask/perfetto/player_ticks.sql",
    )?);
    let deaths = parse_deaths(&run_sql_file(
        root,
        &processor,
        trace,
        "xtask/perfetto/deaths.sql",
    )?);
    let projectiles = run_sql_file(root, &processor, trace, "xtask/perfetto/projectiles.sql")?
        .rows
        .len();
    let trucks = parse_trucks(&run_sql_file(
        root,
        &processor,
        trace,
        "xtask/perfetto/trucks.sql",
    )?);
    let feel = parse_feel(&run_sql_file(
        root,
        &processor,
        trace,
        "xtask/perfetto/feel.sql",
    )?);
    let corpses = parse_corpses(&run_sql_file(
        root,
        &processor,
        trace,
        "xtask/perfetto/corpses.sql",
    )?);
    let items = parse_items(&run_sql_file(
        root,
        &processor,
        trace,
        "xtask/perfetto/items.sql",
    )?);
    let pickups = parse_pickups(&run_sql_file(
        root,
        &processor,
        trace,
        "xtask/perfetto/pickups.sql",
    )?);
    let remotes = parse_remotes(&run_sql_file(
        root,
        &processor,
        trace,
        "xtask/perfetto/remotes.sql",
    )?);
    let lighting = run_sql_file(root, &processor, trace, "xtask/perfetto/lighting_fail.sql")?;
    let lighting_fail = lighting
        .rows
        .first()
        .and_then(|row| cell(&lighting.headers, row, "n"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    Ok(Trace {
        ticks,
        deaths,
        projectiles,
        trucks,
        feel,
        corpses,
        items,
        pickups,
        remotes,
        lighting_fail,
    })
}

fn check(t: &Trace) -> Vec<Claim> {
    let mut claims = vec![
        pose_claim(t),
        bot_pitch_claim(t),
        rpg_claim(t),
        truck_claim(t),
        truck_death_clip_claim(t),
        combat_kill_claim(t),
        dead_claim(t),
        seat_claim(t),
        seat_delta_claim(t),
        seat_lookup_walk_claim(t),
        suicide_wait_claim(t),
        alive_split_claim(t),
        lighting_claim(t),
        corpse_claim(t),
        corpse_clip_claim(t),
        items_present_claim(t),
        items_drawn_claim(t),
        items_ammo_claim(t),
        items_dead_pickup_claim(t),
        items_scavenger_claim(t),
        proxy_claim(t),
        starve_claim(t),
    ];
    for claim in &mut claims {
        let observed = match claim.id {
            "K0" | "I5" => t.deaths.iter().any(|d| d.suicide == 0),
            "K1" => t
                .ticks
                .iter()
                .any(|r| r.client_id == 0 && r.lifecycle.as_deref() == Some("Dead")),
            "K2" | "K4" | "K6" => t
                .feel
                .iter()
                .any(|r| r.lifecycle.as_deref() == Some("Dead")),
            "K3" => t.feel.iter().any(|r| {
                r.lifecycle.as_deref() == Some("Alive")
                    && r.authority_origin_x.is_some()
                    && r.adopted_origin_x.is_some()
            }),
            "L1" => t
                .remotes
                .iter()
                .any(|r| r.pose_e_type == Some(1) && r.client.is_some_and(|c| c != 0)),
            "C1" | "C2" => t.corpses.iter().any(|r| r.occupied == Some(1)),
            "I2" | "I3" => t.items.iter().any(|r| r.e_type == Some(3)),
            _ => true,
        };
        claim.require(
            observed,
            "required scenario entity or lifecycle was not observed",
        );
    }
    claims
}

fn pose_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new("T1", "local on truck 234 roof looking down (pitch 80..90)");
    let rows: Vec<&TickRow> = t
        .ticks
        .iter()
        .filter(|r| r.client_id == 0 && !r.is_bot && r.origin_x.is_some() && r.pitch.is_some())
        .collect();
    if rows.is_empty() {
        claim.check(false, "no local origin+pitch rows");
        return claim;
    }
    let down: Vec<&&TickRow> = rows
        .iter()
        .filter(|r| {
            r.pitch
                .is_some_and(|p| (PITCH_DOWN_MIN..=PITCH_DOWN_MAX).contains(&p))
        })
        .collect();
    let min_p = rows.iter().filter_map(|r| r.pitch).fold(f32::MAX, f32::min);
    let max_p = rows.iter().filter_map(|r| r.pitch).fold(f32::MIN, f32::max);
    claim.check(
        !down.is_empty(),
        format!(
            "pitch in [{PITCH_DOWN_MIN},{PITCH_DOWN_MAX}]: {} / {} local rows (min={min_p:.1} max={max_p:.1})",
            down.len(),
            rows.len()
        ),
    );
    let on_roof = down.iter().any(|r| {
        on_truck_roof(
            r.origin_x.unwrap_or(0.0),
            r.origin_y.unwrap_or(0.0),
            r.origin_z.unwrap_or(0.0),
        )
    });
    let nearest = rows
        .iter()
        .filter_map(|r| {
            Some(xy_dist(
                r.origin_x?,
                r.origin_y?,
                TRUCK_ROOF[0],
                TRUCK_ROOF[1],
            ))
        })
        .fold(f32::MAX, f32::min);
    claim.check(
        on_roof,
        format!(
            "Alive-shaped origin within {ROOF_XY_SLACK:.0}u XY of truck 234 roof {:?} while looking down (nearest XY={nearest:.1})",
            TRUCK_ROOF
        ),
    );
    claim
}

fn bot_pitch_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new(
        "R1",
        "bots look down (pitch 80..90) — RPG into the floor, not the horizon",
    );
    let bot_ids: std::collections::BTreeSet<i64> = t
        .ticks
        .iter()
        .filter(|r| r.is_bot)
        .map(|r| r.client_id)
        .collect();
    claim.check(
        bot_ids.len() >= 7,
        format!("bot client_ids in trace={} (need ≥7)", bot_ids.len()),
    );
    let bot_rows = t.ticks.iter().filter(|r| r.is_bot).count();
    let down = t
        .ticks
        .iter()
        .filter(|r| {
            r.is_bot
                && r.pitch
                    .is_some_and(|p| (PITCH_DOWN_MIN..=PITCH_DOWN_MAX).contains(&p))
        })
        .count();
    claim.check(
        down > 0,
        format!("bot rows with pitch in [80,90]: {down} / {bot_rows}"),
    );
    claim
}

fn rpg_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new(
        "R2",
        "an RPG missile existed — not assault hitscan pretending to be a rocket",
    );
    claim.check(
        t.projectiles > 0,
        format!(
            "missile fire events={} (chaos recipe gives RPG)",
            t.projectiles
        ),
    );
    claim
}

fn truck_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new("T6", "a boneyard vehicle left state 0 / full health");
    let targets: Vec<&TruckRow> = t
        .trucks
        .iter()
        .filter(|row| {
            row.script_model_id
                .is_some_and(|id| TRUCK_IDS.contains(&id))
                && row.state.is_some()
        })
        .collect();
    if !claim.require(
        !targets.is_empty(),
        "no authoritative target vehicle rows were observed",
    ) {
        return claim;
    }
    let max_state = targets.iter().filter_map(|r| r.state).max();
    let min_health = targets.iter().filter_map(|r| r.health).min();
    let damaged = max_state.unwrap_or(0) > 0 || min_health.is_some_and(|h| h < 300);
    claim.check(
        damaged,
        format!("trucks.max(state)={max_state:?} min(health)={min_health:?}"),
    );
    claim
}

fn truck_death_clip_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new(
        "T7",
        "truck 234/179/163 published death_clip and Present posed it (machine B, not player DEATH)",
    );
    if !claim.require(
        t.trucks.iter().any(|r| {
            r.script_model_id.is_some_and(|id| TRUCK_IDS.contains(&id))
                && r.state.is_some_and(|s| s >= 5)
        }),
        "no destroyed vehicle state was observed",
    ) {
        return claim;
    }
    let destroyed: std::collections::BTreeSet<(i64, &str)> = t
        .trucks
        .iter()
        .filter(|r| {
            r.script_model_id.is_some_and(|id| TRUCK_IDS.contains(&id)) && r.state.unwrap_or(0) >= 5
        })
        .filter_map(|r| Some((r.script_model_id?, r.death_clip.as_deref()?)))
        .collect();
    let named = destroyed.len();
    let posed = t
        .trucks
        .iter()
        .filter(|r| {
            r.present_gap.as_deref() == Some("posed")
                && r.script_model_id
                    .zip(r.death_clip.as_deref())
                    .is_some_and(|pair| destroyed.contains(&pair))
        })
        .count();
    claim.check(
        named > 0,
        format!("rows with death_clip after state>=5 = {named}"),
    );
    claim.check(
        posed > 0,
        format!("posed rows matching a destroyed vehicle's published animation = {posed}"),
    );
    claim
}

fn combat_kill_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new(
        "K0",
        "local died to someone else (not suicide) — the killcam path, not a suicide",
    );
    let local_deaths = t.deaths.iter().filter(|d| d.victim == 0).count();
    claim.check(
        local_deaths > 0,
        format!("local (victim=0) death rows={local_deaths}"),
    );
    let combat = t
        .deaths
        .iter()
        .filter(|d| d.victim == 0 && d.suicide == 0 && d.attacker.is_some_and(|a| a != 0))
        .count();
    let suicides = t
        .deaths
        .iter()
        .filter(|d| d.victim == 0 && d.suicide == 1)
        .count();
    claim.check(
        combat > 0,
        format!(
            "local combat deaths (suicide=0, attacker≠0)={combat}; suicides={suicides} (GSC #L638 handleSuicideDeath skips killcam)"
        ),
    );
    claim
}

fn dead_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new(
        "K1",
        "local Dead lasts ≥1.5s then Alive (respawn after the killcam)",
    );
    let lives = local_lives(&t.ticks);
    if lives.is_empty() {
        claim.check(false, "no local lifecycle rows");
        return claim;
    }
    let (longest, then_alive) = longest_dead(&lives);
    claim.check(
        longest >= MIN_DEAD_MS,
        format!("longest local Dead stretch={longest}ms (need ≥{MIN_DEAD_MS})"),
    );
    claim.check(
        then_alive,
        "Alive follows that Dead stretch (spawnClient, not stuck Dead)",
    );
    claim
}

fn seat_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new("K2", "Fanout applied a killcam seat while local was Dead");
    let applied = t
        .feel
        .iter()
        .filter(|f| f.fanout_seat_applied == Some(1))
        .count();
    claim.check(
        applied > 0,
        format!("feel.fanout_seat_applied=1 on {applied} rows"),
    );
    let on_dead = t
        .feel
        .iter()
        .filter(|f| f.fanout_seat_applied == Some(1) && f.lifecycle.as_deref() == Some("Dead"))
        .count();
    claim.check(
        on_dead > 0,
        format!("seat_applied=1 on local Dead={on_dead} (an Alive seat is refused)"),
    );
    claim
}

fn seat_delta_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new(
        "K4",
        "presented local deltaTime is nonzero while Fanout seated (N24/N28)",
    );
    let seated: Vec<&FeelRow> = t
        .feel
        .iter()
        .filter(|f| {
            f.fanout_seat_applied == Some(1)
                && f.lifecycle.as_deref() == Some("Dead")
                && f.present_choice.is_some()
        })
        .collect();
    if seated.is_empty() {
        claim.check(false, "no Dead+seat rows to read deltaTime");
        return claim;
    }
    let live = seated
        .iter()
        .skip(1)
        .filter(|f| {
            f.present_choice.as_deref() != Some("archived")
                || f.present_snapshot_delta_time.is_none()
                || f.present_snapshot_delta_time == Some(0)
        })
        .count();
    claim.check(
        live == 0,
        format!(
            "Dead+seat present_choice=archived and delta_time=0 = {live} / {} (present painted the corpse)",
            seated.len()
        ),
    );
    claim
}

fn seat_lookup_walk_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new(
        "K6",
        "Fanout killcam lookup tick walks (N27: fixed archivetime + moving now)",
    );
    let ticks: Vec<i64> = t
        .feel
        .iter()
        .filter(|f| {
            f.fanout_seat_applied == Some(1)
                && f.lifecycle.as_deref() == Some("Dead")
                && f.seat_lookup_tick.is_some()
        })
        .filter_map(|f| f.seat_lookup_tick)
        .collect();
    if ticks.is_empty() {
        claim.check(false, "no Dead+seat rows with a lookup tick");
        return claim;
    }
    let unique: std::collections::BTreeSet<i64> = ticks.iter().copied().collect();
    let span = ticks.iter().max().copied().unwrap_or(0) - ticks.iter().min().copied().unwrap_or(0);
    let walks = unique.len() >= 10 || span >= 10;
    claim.check(
        walks,
        format!(
            "Dead+seat lookup ticks unique={} span={span} over {} rows \
             (one tick is a still photo, not resample)",
            unique.len(),
            ticks.len()
        ),
    );
    claim
}

fn suicide_wait_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new(
        "K7",
        "suicide/world victims stay Dead ≥1.5s (GSC postDeathDelay, then spawnClient)",
    );
    let victims: std::collections::BTreeSet<i64> = t
        .deaths
        .iter()
        .filter(|d| d.suicide == 1)
        .map(|d| d.victim)
        .collect();
    if victims.is_empty() {
        claim.require(false, "no suicide/world death was exercised");
        return claim;
    }
    for victim in victims {
        let lives: Vec<(i32, String)> = t
            .ticks
            .iter()
            .filter(|r| r.client_id == victim)
            .filter_map(|r| {
                r.lifecycle
                    .clone()
                    .map(|l| (r.level_time_ms.unwrap_or(r.time_ms), l))
            })
            .collect();
        if lives.is_empty() {
            claim.check(
                false,
                format!("suicide victim {victim} has no player_tick.lifecycle rows"),
            );
            continue;
        }
        let (ms, _) = longest_dead(&lives);
        claim.check(
            ms >= MIN_DEAD_MS,
            format!(
                "suicide victim {victim} longest Dead={ms}ms want ≥{MIN_DEAD_MS} \
                 (one-tick spawn was the 5068 disease)"
            ),
        );
    }
    claim
}

fn alive_split_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new(
        "K3",
        "Alive listener is not seated on an archived origin (|auth−adopted|<100)",
    );
    let tail = tail_start(t);
    let splits = t
        .feel
        .iter()
        .filter(|f| f.time_ms.unwrap_or(0) >= tail)
        .filter(|f| {
            f.lifecycle.as_deref() == Some("Alive")
                && f.authority_origin_x.is_some()
                && f.adopted_origin_x.is_some()
        })
        .filter(|f| {
            let dx = f.authority_origin_x.unwrap() - f.adopted_origin_x.unwrap();
            let dy = f.authority_origin_y.unwrap_or(0.0) - f.adopted_origin_y.unwrap_or(0.0);
            dx * dx + dy * dy > ALIVE_SPLIT_UNITS * ALIVE_SPLIT_UNITS
        })
        .count();
    claim.check(
        splits == 0,
        format!("Alive |auth−adopted| XY > {ALIVE_SPLIT_UNITS} on {splits} rows in last 20s (I2)"),
    );
    claim
}

fn lighting_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new(
        "L1",
        "remote bodies are not skipped for lighting alloc failed",
    );
    claim.check(
        t.lighting_fail == 0,
        format!(
            "lighting_fail events={} (PLAN #2 ring drain)",
            t.lighting_fail
        ),
    );
    claim
}

fn corpse_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new("C1", "corpses are not at z≈−1.7e5 (uninit trajectory)");
    let bad_slots = t
        .corpses
        .iter()
        .filter(|c| {
            c.occupied == Some(1)
                && c.pose_e_type.is_none()
                && c.origin_z.is_some_and(|z| z < CORPSE_Z_FLOOR)
        })
        .count();
    claim.check(
        bad_slots == 0,
        format!("corpses occupied with origin_z < {CORPSE_Z_FLOOR} = {bad_slots}"),
    );
    let bad_centity = t
        .corpses
        .iter()
        .filter(|c| {
            c.pose_e_type == Some(2)
                && c.legs_leaf_name.is_none()
                && c.origin_z.is_some_and(|z| z < CORPSE_Z_FLOOR)
        })
        .count();
    claim.check(
        bad_centity == 0,
        format!("corpse eType 2 with origin_z < {CORPSE_Z_FLOOR} = {bad_centity}"),
    );
    claim
}

fn corpse_clip_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new(
        "C2",
        "occupied corpse dump has legs_leaf_name (instrument) and a DEATH clip (not locomotion)",
    );
    let occupied = t.corpses.iter().filter(|c| c.occupied == Some(1)).count();
    claim.check(occupied > 0, format!("occupied corpse rows={occupied}"));
    let named = t
        .corpses
        .iter()
        .filter(|c| c.occupied == Some(1) && c.legs_leaf_name.is_some())
        .count();
    claim.check(
        named > 0,
        format!("occupied with legs_leaf_name={named} (NULL is Present not sampled, never 0)"),
    );
    let death = t
        .corpses
        .iter()
        .filter(|c| {
            c.occupied == Some(1)
                && c.legs_leaf_name
                    .as_deref()
                    .is_some_and(|n| n.to_ascii_lowercase().contains("death"))
        })
        .count();
    claim.check(
        death > 0,
        format!("occupied DEATH-named clips={death} (locomotion idle/run is not a death clip)"),
    );
    claim
}

fn items_present_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new(
        "I1",
        "ET_ITEM dump has e_type=3 rows — drop is a gentity, not gun_attached on the corpse",
    );
    let n = t.items.iter().filter(|i| i.e_type == Some(3)).count();
    claim.check(
        n > 0,
        format!("ET_ITEM rows={n} (empty-clip RPG deaths still hold the gun; assault with clip must drop)"),
    );
    claim
}

fn items_drawn_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new(
        "I2",
        "Present posed the dropped worldModel, not only an authority row",
    );
    let n = t
        .items
        .iter()
        .filter(|i| i.present_gap.as_deref() == Some("posed"))
        .count();
    claim.check(
        n > 0,
        format!(
            "posed ET_ITEM rows={n} (authority e_type=3 without present_gap=posed is a picture gap)"
        ),
    );
    claim
}

fn items_ammo_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new(
        "I3",
        "ET_ITEM carries ItemWeaponSetAmmo (clip_r), not a mesh-only prop",
    );
    let n = t.items.iter().filter(|i| i.clip_r.unwrap_or(0) > 0).count();
    claim.check(
        n > 0,
        format!(
            "ItemWeaponSetAmmo clip_r>0 rows={n} (authority drop without ammo is a pickup lie)"
        ),
    );
    claim
}

fn items_dead_pickup_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new(
        "I4",
        "Dead walkers do not consume ET_ITEM (health/pm_type gate)",
    );
    if t.pickups
        .iter()
        .any(|p| p.event.is_none() || p.picker_pm_type.is_none())
    {
        claim.harness_error("pickup events are missing event kind or picker movement type");
        return claim;
    }
    let rejected = t
        .pickups
        .iter()
        .filter(|p| {
            p.event.as_deref() == Some("pickup_rejected")
                && p.picker_pm_type.is_some_and(|pm| pm >= 8)
        })
        .count();
    if !claim.require(
        rejected > 0,
        "no dead player overlapped a dropped item to exercise rejection",
    ) {
        return claim;
    }
    let dead = t
        .pickups
        .iter()
        .filter(|p| {
            p.event.as_deref() == Some("pickup") && p.picker_pm_type.is_some_and(|pm| pm >= 8)
        })
        .count();
    claim.check(
        dead == 0,
        format!("Dead pickup grants={dead}; rejected dead-player overlaps={rejected}"),
    );
    claim
}

fn items_scavenger_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new(
        "I5",
        "Non-suicide deaths drop scavenger_bag_mp (GSC dropScavengerForDeath before the gun)",
    );
    let n = t.items.iter().filter(|i| i.scavenger == Some(1)).count();
    claim.check(
        n > 0,
        format!(
            "scavenger bags={n} (GSC drops a bag on every non-suicide death; missing scavenger_bag_mp catalog name stays 0)"
        ),
    );
    claim
}

fn proxy_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new(
        "P1",
        "remote ET_PLAYER centity origin tracks the applied snapshot (PLAN #5 freeze)",
    );
    let remotes: Vec<&RemoteRow> = t
        .remotes
        .iter()
        .filter(|r| r.pose_e_type == Some(1) && r.client.is_some_and(|c| c != 0))
        .collect();
    if !claim.require(
        !remotes.is_empty(),
        "no remote ET_PLAYER rows were observed",
    ) {
        return claim;
    }
    if remotes.iter().any(|r| {
        r.origin_x.is_none()
            || r.origin_y.is_none()
            || r.snap_origin_x.is_none()
            || r.snap_origin_y.is_none()
    }) {
        claim.harness_error("remote pose or applied snapshot XY coordinates are missing");
        return claim;
    }
    let n = remotes.len() as i64;
    let split = remotes
        .iter()
        .filter(|r| {
            let dx = r.origin_x.unwrap_or(0.0) - r.snap_origin_x.unwrap_or(0.0);
            let dy = r.origin_y.unwrap_or(0.0) - r.snap_origin_y.unwrap_or(0.0);
            dx * dx + dy * dy > ALIVE_SPLIT_UNITS * ALIVE_SPLIT_UNITS
        })
        .count() as i64;
    claim.check(
        split == 0,
        format!("remote eType 1 |centity−snap| XY > {ALIVE_SPLIT_UNITS} = {split} / {n}"),
    );
    claim
}

fn starve_claim(t: &Trace) -> Claim {
    let mut claim = Claim::new(
        "P2",
        "remote proxy starve is dump-window start, not an eaten 4-tick ring",
    );
    let remotes: Vec<&RemoteRow> = t
        .remotes
        .iter()
        .filter(|r| r.pose_e_type == Some(1) && r.client.is_some_and(|c| c != 0))
        .collect();
    if !claim.require(
        !remotes.is_empty(),
        "no remote ET_PLAYER rows were observed",
    ) {
        return claim;
    }
    if remotes.iter().any(|r| {
        r.time_ms.is_none()
            || !matches!(
                r.proxy_outcome.as_deref(),
                Some("starved" | "exact" | "interpolated")
            )
    }) {
        claim.harness_error("remote sample time or proxy outcome is missing or invalid");
        return claim;
    }
    let starved = remotes
        .iter()
        .filter(|r| r.proxy_outcome.as_deref() == Some("starved"))
        .count();
    let first_ok = remotes
        .iter()
        .filter(|r| r.proxy_outcome.as_deref() != Some("starved"))
        .filter_map(|r| r.time_ms)
        .min();
    if !claim.require(
        first_ok.is_some(),
        "no usable remote proxy sample was observed",
    ) {
        return claim;
    }
    let later = remotes
        .iter()
        .filter(|r| {
            r.proxy_outcome.as_deref() == Some("starved")
                && first_ok.is_some_and(|first| r.time_ms.is_some_and(|ms| ms > first))
        })
        .count();
    claim.check(
        later == 0,
        format!("remote starved after first frame = {later} (all starved {starved})"),
    );
    claim
}

fn tail_start(t: &Trace) -> i32 {
    t.ticks
        .iter()
        .filter_map(|r| r.level_time_ms)
        .max()
        .unwrap_or(0)
        .saturating_sub(20_000)
}

fn local_lives(ticks: &[TickRow]) -> Vec<(i32, String)> {
    ticks
        .iter()
        .filter(|r| r.client_id == 0)
        .filter_map(|r| {
            r.lifecycle
                .clone()
                .map(|l| (r.level_time_ms.unwrap_or(r.time_ms), l))
        })
        .collect()
}

fn longest_dead(lives: &[(i32, String)]) -> (i32, bool) {
    let mut best = 0i32;
    let mut best_end_idx: Option<usize> = None;
    let mut run_start: Option<i32> = None;
    let mut run_end = 0i32;
    let mut run_end_idx = 0usize;
    for (i, (t, life)) in lives.iter().enumerate() {
        if life == "Dead" {
            if run_start.is_none() {
                run_start = Some(*t);
            }
            run_end = *t;
            run_end_idx = i;
        } else if let Some(start) = run_start.take() {
            let dur = (run_end - start) + AUTHORITY_MS;
            if dur >= best {
                best = dur;
                best_end_idx = Some(run_end_idx);
            }
        }
    }
    if let Some(start) = run_start {
        let dur = (run_end - start) + AUTHORITY_MS;
        if dur >= best {
            best = dur;
            best_end_idx = Some(run_end_idx);
        }
    }
    let then_alive =
        best_end_idx.is_some_and(|i| lives.iter().skip(i + 1).any(|(_, l)| l == "Alive"));
    (best, then_alive)
}

fn on_truck_roof(x: f32, y: f32, z: f32) -> bool {
    xy_dist(x, y, TRUCK_ROOF[0], TRUCK_ROOF[1]) <= ROOF_XY_SLACK
        && (ROOF_Z_MIN..=ROOF_Z_MAX).contains(&z)
}

fn xy_dist(x: f32, y: f32, ox: f32, oy: f32) -> f32 {
    let dx = x - ox;
    let dy = y - oy;
    (dx * dx + dy * dy).sqrt()
}

fn parse_ticks(table: &Table) -> Vec<TickRow> {
    table
        .rows
        .iter()
        .map(|cells| TickRow {
            client_id: cell_i64(&table.headers, cells, "client_id").unwrap_or(0),
            is_bot: cell_i64(&table.headers, cells, "is_bot").unwrap_or(0) != 0,
            time_ms: cell_i32(&table.headers, cells, "time_ms").unwrap_or(0),
            level_time_ms: cell_i32(&table.headers, cells, "level_time_ms"),
            origin_x: cell_f32(&table.headers, cells, "origin_x"),
            origin_y: cell_f32(&table.headers, cells, "origin_y"),
            origin_z: cell_f32(&table.headers, cells, "origin_z"),
            pitch: cell_f32(&table.headers, cells, "pitch"),
            lifecycle: cell(&table.headers, cells, "lifecycle").map(str::to_owned),
        })
        .collect()
}

fn parse_deaths(table: &Table) -> Vec<DeathRow> {
    table
        .rows
        .iter()
        .map(|cells| DeathRow {
            victim: cell_i64(&table.headers, cells, "victim").unwrap_or(0),
            attacker: cell_i64(&table.headers, cells, "attacker"),
            suicide: cell_i64(&table.headers, cells, "suicide").unwrap_or(0),
        })
        .collect()
}

fn parse_trucks(table: &Table) -> Vec<TruckRow> {
    table
        .rows
        .iter()
        .map(|cells| TruckRow {
            script_model_id: cell_i64(&table.headers, cells, "script_model_id"),
            state: cell_i64(&table.headers, cells, "state"),
            health: cell_i64(&table.headers, cells, "health"),
            death_clip: cell(&table.headers, cells, "death_clip").map(str::to_owned),
            present_gap: cell(&table.headers, cells, "present_gap").map(str::to_owned),
        })
        .collect()
}

fn parse_feel(table: &Table) -> Vec<FeelRow> {
    table
        .rows
        .iter()
        .map(|cells| FeelRow {
            time_ms: cell_i32(&table.headers, cells, "time_ms"),
            lifecycle: cell(&table.headers, cells, "lifecycle").map(str::to_owned),
            fanout_seat_applied: cell_i64(&table.headers, cells, "fanout_seat_applied"),
            seat_lookup_tick: cell_i64(&table.headers, cells, "seat_lookup_tick"),
            present_choice: cell(&table.headers, cells, "present_choice").map(str::to_owned),
            present_snapshot_delta_time: cell_i64(
                &table.headers,
                cells,
                "present_snapshot_delta_time",
            ),
            authority_origin_x: cell_f32(&table.headers, cells, "authority_origin_x"),
            authority_origin_y: cell_f32(&table.headers, cells, "authority_origin_y"),
            adopted_origin_x: cell_f32(&table.headers, cells, "adopted_origin_x"),
            adopted_origin_y: cell_f32(&table.headers, cells, "adopted_origin_y"),
        })
        .collect()
}

fn parse_corpses(table: &Table) -> Vec<CorpseRow> {
    table
        .rows
        .iter()
        .map(|cells| CorpseRow {
            occupied: cell_i64(&table.headers, cells, "occupied"),
            origin_z: cell_f32(&table.headers, cells, "origin_z"),
            pose_e_type: cell_i64(&table.headers, cells, "pose_e_type"),
            legs_leaf_name: cell(&table.headers, cells, "legs_leaf_name").map(str::to_owned),
        })
        .collect()
}

fn parse_items(table: &Table) -> Vec<ItemRow> {
    table
        .rows
        .iter()
        .map(|cells| ItemRow {
            e_type: cell_i64(&table.headers, cells, "e_type"),
            clip_r: cell_i64(&table.headers, cells, "clip_r"),
            scavenger: cell_i64(&table.headers, cells, "scavenger"),
            present_gap: cell(&table.headers, cells, "present_gap").map(str::to_owned),
        })
        .collect()
}

fn parse_pickups(table: &Table) -> Vec<PickupRow> {
    table
        .rows
        .iter()
        .map(|cells| PickupRow {
            event: cell(&table.headers, cells, "event").map(str::to_owned),
            picker_pm_type: cell_i64(&table.headers, cells, "picker_pm_type"),
        })
        .collect()
}

fn parse_remotes(table: &Table) -> Vec<RemoteRow> {
    table
        .rows
        .iter()
        .map(|cells| RemoteRow {
            client: cell_i64(&table.headers, cells, "client"),
            time_ms: cell_i32(&table.headers, cells, "time_ms"),
            pose_e_type: cell_i64(&table.headers, cells, "pose_e_type"),
            origin_x: cell_f32(&table.headers, cells, "origin_x"),
            origin_y: cell_f32(&table.headers, cells, "origin_y"),
            snap_origin_x: cell_f32(&table.headers, cells, "snap_origin_x"),
            snap_origin_y: cell_f32(&table.headers, cells, "snap_origin_y"),
            proxy_outcome: cell(&table.headers, cells, "proxy_outcome").map(str::to_owned),
        })
        .collect()
}
