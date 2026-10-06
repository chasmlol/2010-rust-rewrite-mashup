use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::perfetto_query::{cell_f32, cell_i32, cell_i64};

const AUTHORITY_SECONDS: f32 = 0.05;

const GRAVITY: f32 = 800.0;

const BUTTON_ATTACK: i64 = 0x1;

const BUTTON_RELOAD: i64 = 0x10;

const RELOAD_STATES: &[i32] = &[0x8, 0x9, 0xA, 0xB, 0xC];

const JUMP_LAUNCH_VZ: f32 = 249.799_92;

const JUMP_PEAK_VZ: f32 = JUMP_LAUNCH_VZ - GRAVITY * AUTHORITY_SECONDS;

const JUMP_VZ_TOLERANCE: f32 = 1.0;

const JUMP_MIN_RISE: f32 = 20.0;

const MIN_FORWARD_TRAVEL: f32 = 100.0;

const BOT_MIN_YAW_SPREAD: f32 = 5.0;

const BOTS_REQUIRED: usize = 2;

struct Row {
    sequence: i64,
    time_ms: i32,
    client_id: Option<i64>,
    is_bot: bool,
    origin_x: Option<f32>,
    origin_y: Option<f32>,
    origin_z: Option<f32>,
    vz: Option<f32>,
    yaw: Option<f32>,
    jump_time: Option<i32>,
    buttons: Option<i64>,
    weaponstate: Option<i32>,
    ammo_clip: Option<i32>,

    legs_anim: Option<i32>,

    torso_anim: Option<i32>,

    walking: Option<i32>,
}

#[derive(Debug)]
pub struct Claim {
    pub id: &'static str,
    pub title: &'static str,
    outcome: Option<Outcome>,
    pub evidence: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Fail,
    NotExercised,
    HarnessError,
}

impl Outcome {
    fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::NotExercised => "NOT_EXERCISED",
            Self::HarnessError => "HARNESS_ERROR",
        }
    }
}

impl Claim {
    pub(crate) fn new(id: &'static str, title: &'static str) -> Self {
        Self {
            id,
            title,
            outcome: None,
            evidence: Vec::new(),
        }
    }

    pub(crate) fn check(&mut self, ok: bool, line: impl Into<String>) {
        if !ok && self.outcome != Some(Outcome::HarnessError) {
            self.outcome = Some(Outcome::Fail);
        } else if self.outcome.is_none() {
            self.outcome = Some(Outcome::Pass);
        }
        self.evidence.push(format!(
            "{} {}",
            if ok { "ok  " } else { "FAIL" },
            line.into()
        ));
    }

    pub(crate) fn require(&mut self, present: bool, reason: impl Into<String>) -> bool {
        if !present {
            if self.outcome != Some(Outcome::HarnessError) {
                self.outcome = Some(Outcome::NotExercised);
            }
            self.evidence
                .push(format!("NOT_EXERCISED {}", reason.into()));
        }
        present
    }

    pub(crate) fn harness_error(&mut self, reason: impl Into<String>) {
        self.outcome = Some(Outcome::HarnessError);
        self.evidence
            .push(format!("HARNESS_ERROR {}", reason.into()));
    }

    pub(crate) fn report(&self) -> bool {
        let outcome = self.outcome.unwrap_or(Outcome::NotExercised);
        println!("\n[{}] {} — {}", self.id, self.title, outcome.label());
        for line in &self.evidence {
            println!("  {line}");
        }
        println!(
            "gate: {}",
            serde_json::json!({
                "id": self.id,
                "title": self.title,
                "outcome": outcome.label(),
                "evidence": self.evidence,
            })
        );
        outcome == Outcome::Pass
    }

    fn note(&mut self, line: impl Into<String>) {
        self.evidence.push(format!("--   {}", line.into()));
    }
}

pub(crate) fn harness_failure(reason: &str) -> bool {
    let mut claim = Claim::new("TRACE", "trace collection and decoding");
    claim.harness_error(reason);
    claim.report()
}

fn check(rows: &[Row]) -> Vec<Claim> {
    vec![
        reload_claim(rows),
        claim(rows),
        bots_claim(rows),
        anim_claim(rows),
    ]
}

fn local_rows(rows: &[Row]) -> Vec<&Row> {
    rows.iter().filter(|r| !r.is_bot).collect()
}

fn reload_claim(rows: &[Row]) -> Claim {
    let mut claim = Claim::new("A", "empty magazine triggers an automatic reload");
    let local = local_rows(rows);
    if local.is_empty() {
        claim.check(false, "no non-bot client in the dump");
        return claim;
    }

    let clips: Vec<(usize, i32)> = local
        .iter()
        .enumerate()
        .filter_map(|(i, r)| r.ammo_clip.map(|c| (i, c)))
        .collect();
    if clips.is_empty() {
        claim.check(false, "no frame carries ammo_clip");
        return claim;
    }

    let full = clips.iter().map(|(_, c)| *c).max().unwrap_or(0);
    let empty_at = clips.iter().find(|(_, c)| *c == 0).map(|(i, _)| *i);
    claim.check(
        full > 0,
        format!("magazine was loaded at some point (max ammo_clip={full})"),
    );
    let Some(empty_at) = empty_at else {
        claim.check(
            false,
            format!(
                "magazine never reached 0 (min ammo_clip={})",
                clips.iter().map(|(_, c)| *c).min().unwrap_or(0)
            ),
        );
        return claim;
    };
    claim.check(
        true,
        format!(
            "magazine emptied at t={}ms (frame {})",
            local[empty_at].time_ms, local[empty_at].sequence
        ),
    );

    let reload_at = local
        .iter()
        .enumerate()
        .skip(empty_at)
        .find(|(_, r)| r.weaponstate.is_some_and(|w| RELOAD_STATES.contains(&w)))
        .map(|(i, _)| i);
    let Some(reload_at) = reload_at else {
        let seen: Vec<i32> = {
            let mut v: Vec<i32> = local.iter().filter_map(|r| r.weaponstate).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        claim.check(
            false,
            format!("weaponstate never entered a reload state after empty; saw {seen:?}"),
        );
        return claim;
    };
    claim.check(
        true,
        format!(
            "weaponstate={:#x} at t={}ms",
            local[reload_at].weaponstate.unwrap_or(0),
            local[reload_at].time_ms
        ),
    );

    let refilled = local
        .iter()
        .skip(reload_at)
        .find(|r| r.ammo_clip.is_some_and(|c| c > 0));
    match refilled {
        Some(r) => claim.check(
            true,
            format!(
                "magazine refilled to {} at t={}ms",
                r.ammo_clip.unwrap_or(0),
                r.time_ms
            ),
        ),
        None => claim.check(
            false,
            "reload started but the magazine never came back (dump window too short, \
             or the reload never finished)",
        ),
    }

    let pressed_reload = local
        .iter()
        .any(|r| r.buttons.is_some_and(|b| b & BUTTON_RELOAD != 0));
    claim.check(
        !pressed_reload,
        "the reload was automatic (+reload was never in usercmd buttons)",
    );
    claim
}

fn claim(rows: &[Row]) -> Claim {
    let mut claim = Claim::new("B", "press +gostand launches, and hold +forward travels");
    let local = local_rows(rows);
    if local.is_empty() {
        claim.check(false, "no non-bot client in the dump");
        return claim;
    }

    let first_jump_time = local.iter().find_map(|r| r.jump_time);
    let launch = local
        .iter()
        .enumerate()
        .find(|(_, r)| match (r.jump_time, first_jump_time) {
            (Some(t), Some(first)) => t != first,
            _ => false,
        });
    let Some((launch_at, launch_row)) = launch else {
        claim.check(
            false,
            format!("jump_time never changed from {first_jump_time:?} — no jump was stamped"),
        );
        return claim;
    };
    claim.check(
        true,
        format!(
            "jump_time {:?} → {:?} at t={}ms",
            first_jump_time, launch_row.jump_time, launch_row.time_ms
        ),
    );

    let arc: Vec<&&Row> = local.iter().skip(launch_at).collect();
    let peak_vz = arc.iter().filter_map(|r| r.vz).fold(f32::MIN, f32::max);
    claim.check(
        (peak_vz - JUMP_PEAK_VZ).abs() <= JUMP_VZ_TOLERANCE,
        format!(
            "launch vz={peak_vz:.2} = {JUMP_LAUNCH_VZ:.2} less one tick of gravity              (expect {JUMP_PEAK_VZ:.2} ±{JUMP_VZ_TOLERANCE:.1})"
        ),
    );

    walking_cleared(&mut claim, &arc);

    let launch_z = launch_row.origin_z.unwrap_or(0.0);
    let arc_z: Vec<f32> = arc.iter().filter_map(|r| r.origin_z).collect();
    let peak_z = arc_z.iter().copied().fold(f32::MIN, f32::max);
    let peak_at = arc_z.iter().position(|z| *z == peak_z).unwrap_or(0);
    claim.check(
        peak_z - launch_z >= JUMP_MIN_RISE,
        format!(
            "origin_z rose {:.2} from {launch_z:.2} (need {JUMP_MIN_RISE:.0})",
            peak_z - launch_z
        ),
    );

    let landed = arc_z
        .iter()
        .skip(peak_at)
        .any(|z| peak_z - *z >= JUMP_MIN_RISE);
    claim.check(
        landed,
        format!("came back down from the apex at {peak_z:.2}"),
    );

    let travel = horizontal_travel(&local);
    claim.check(
        travel >= MIN_FORWARD_TRAVEL,
        format!("horizontal travel {travel:.1} units (need {MIN_FORWARD_TRAVEL:.0})"),
    );
    claim
}

fn walking_cleared(claim: &mut Claim, arc: &[&&Row]) {
    let sampled = arc.iter().filter_map(|r| r.walking).count();
    if sampled == 0 {
        claim.note("player_tick.walking unsampled — skip (host did not copy pml.walking)");
        return;
    }
    let airborne = arc.iter().filter(|r| r.walking == Some(0)).count();
    claim.check(
        airborne > 0,
        format!(
            "player_tick.walking: some post-launch row has pml.walking=0 \
             (player_tick 0-count={airborne})"
        ),
    );
}

fn bots_claim(rows: &[Row]) -> Claim {
    let mut claim = Claim::new("C", "bots join and shoot");
    let mut ids: Vec<i64> = rows
        .iter()
        .filter(|r| r.is_bot)
        .filter_map(|r| r.client_id)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    claim.check(
        ids.len() >= BOTS_REQUIRED,
        format!("{} bot client(s) in the dump: {ids:?}", ids.len()),
    );
    if ids.is_empty() {
        return claim;
    }

    let mut firing = Vec::new();
    let mut draining = Vec::new();
    let mut aiming = Vec::new();
    for id in &ids {
        let bot: Vec<&Row> = rows
            .iter()
            .filter(|r| r.is_bot && r.client_id == Some(*id))
            .collect();
        if bot
            .iter()
            .any(|r| r.buttons.is_some_and(|b| b & BUTTON_ATTACK != 0))
        {
            firing.push(*id);
        }
        let clips: Vec<i32> = bot.iter().filter_map(|r| r.ammo_clip).collect();
        if clips.windows(2).any(|w| w[1] < w[0]) {
            draining.push(*id);
        }
        let yaws: Vec<f32> = bot.iter().filter_map(|r| r.yaw).collect();
        if yaw_spread(&yaws) > BOT_MIN_YAW_SPREAD {
            aiming.push(*id);
        }
    }

    claim.check(
        firing.len() >= BOTS_REQUIRED,
        format!("{} bot(s) held the fire button: {firing:?}", firing.len()),
    );
    claim.check(
        draining.len() >= BOTS_REQUIRED,
        format!(
            "{} bot(s) actually spent rounds (ammo_clip fell): {draining:?}",
            draining.len()
        ),
    );
    claim.check(
        aiming.len() >= BOTS_REQUIRED,
        format!(
            "{} bot(s) aimed around rather than at one fixed yaw: {aiming:?}",
            aiming.len()
        ),
    );
    claim
}

fn anim_claim(rows: &[Row]) -> Claim {
    let mut claim = Claim::new(
        "D",
        "player_tick samples playerState legsAnim/torsoAnim (0 is the AnimScript gap)",
    );
    let local = local_rows(rows);
    if local.is_empty() {
        claim.check(false, "no non-bot client in the dump");
        return claim;
    }
    let sampled = local
        .iter()
        .filter(|r| r.legs_anim.is_some() && r.torso_anim.is_some())
        .count();
    claim.check(
        sampled > 0,
        format!(
            "{sampled}/{} local rows carry both legs_anim and torso_anim (NULL = no PS that tick, never invent 0)",
            local.len()
        ),
    );
    if sampled == 0 {
        return claim;
    }
    let legs: BTreeSet<i32> = local.iter().filter_map(|r| r.legs_anim).collect();
    let torso: BTreeSet<i32> = local.iter().filter_map(|r| r.torso_anim).collect();
    let all_zero = legs.iter().all(|&v| v == 0) && torso.iter().all(|&v| v == 0);
    if all_zero {
        claim.note(
            "all sampled values are 0 — the anim script is not in pmove; this is the gap, not a passing 0x155 codec round-trip",
        );
        claim.check(
            true,
            "values are 0: dump sampling is not the lie; AnimScript is the remaining gap",
        );
    } else {
        claim.check(
            true,
            format!("distinct legs_anim={legs:#x?} torso_anim={torso:#x?}"),
        );
    }
    let bots: Vec<&Row> = rows.iter().filter(|r| r.is_bot).collect();
    if !bots.is_empty() {
        let bot_sampled = bots
            .iter()
            .filter(|r| r.legs_anim.is_some() && r.torso_anim.is_some())
            .count();
        let bot_legs: BTreeSet<i32> = bots.iter().filter_map(|r| r.legs_anim).collect();
        claim.note(format!(
            "bots: {bot_sampled}/{} rows sampled, distinct legs_anim={bot_legs:#x?}",
            bots.len()
        ));
    }
    claim
}

fn horizontal_travel(rows: &[&Row]) -> f32 {
    let mut total = 0.0;
    let mut prev: Option<(f32, f32)> = None;
    for row in rows {
        let (Some(x), Some(y)) = (row.origin_x, row.origin_y) else {
            continue;
        };
        if let Some((px, py)) = prev {
            let (dx, dy) = (x - px, y - py);

            let step = (dx * dx + dy * dy).sqrt();
            if step < 200.0 {
                total += step;
            }
        }
        prev = Some((x, y));
    }
    total
}

fn yaw_spread(yaws: &[f32]) -> f32 {
    if yaws.len() < 2 {
        return 0.0;
    }
    let min = yaws.iter().copied().fold(f32::MAX, f32::min);
    let max = yaws.iter().copied().fold(f32::MIN, f32::max);
    max - min
}

fn load(root: &Path, trace: &Path) -> Result<Vec<Row>, String> {
    let processor = crate::perfetto_query::resolve_processor(root)?;
    crate::perfetto_query::ensure_trace_health(root, &processor, trace)?;
    let table = crate::perfetto_query::run_sql_file(
        root,
        &processor,
        trace,
        "xtask/perfetto/player_ticks.sql",
    )?;
    if table.rows.is_empty() {
        return Err(
            "no player_tick events — recording off (need IW4L_PERF=1) or PublishSnapshot never ran"
                .into(),
        );
    }
    let rows = parse_player_ticks(&table);
    if rows
        .iter()
        .all(|r| r.ammo_clip.is_none() && r.client_id.is_none())
    {
        let keys = arg_keys(&processor, trace)?;
        return Err(format!(
            "player_tick rows exist but EXTRACT_ARG(debug.*) is empty. arg keys: {keys:?}"
        ));
    }
    Ok(rows)
}

fn parse_player_ticks(table: &crate::perfetto_query::Table) -> Vec<Row> {
    let mut rows = Vec::with_capacity(table.rows.len());
    for (i, cells) in table.rows.iter().enumerate() {
        rows.push(Row {
            sequence: i as i64,
            time_ms: cell_i32(&table.headers, cells, "time_ms").unwrap_or(0),
            client_id: cell_i64(&table.headers, cells, "client_id"),
            is_bot: cell_i64(&table.headers, cells, "is_bot").unwrap_or(0) != 0,
            origin_x: cell_f32(&table.headers, cells, "origin_x"),
            origin_y: cell_f32(&table.headers, cells, "origin_y"),
            origin_z: cell_f32(&table.headers, cells, "origin_z"),
            vz: cell_f32(&table.headers, cells, "vz"),
            yaw: cell_f32(&table.headers, cells, "yaw"),
            jump_time: cell_i32(&table.headers, cells, "jump_time"),
            buttons: cell_i64(&table.headers, cells, "buttons"),
            weaponstate: cell_i32(&table.headers, cells, "weaponstate"),
            ammo_clip: cell_i32(&table.headers, cells, "ammo_clip"),
            legs_anim: cell_i32(&table.headers, cells, "legs_anim"),
            torso_anim: cell_i32(&table.headers, cells, "torso_anim"),
            walking: cell_i32(&table.headers, cells, "walking"),
        });
    }
    rows
}

fn arg_keys(processor: &std::path::Path, trace: &Path) -> Result<Vec<String>, String> {
    let table = crate::perfetto_query::run_sql(
        processor,
        trace,
        "SELECT DISTINCT key FROM args \
         JOIN slice ON args.arg_set_id = slice.arg_set_id \
         WHERE slice.name = 'player_tick' LIMIT 40",
    )?;
    Ok(table
        .rows
        .into_iter()
        .filter_map(|r| r.into_iter().next())
        .collect())
}

pub fn scenario_gate(root: &Path, trace_arg: Option<PathBuf>) -> bool {
    crate::hr("scenario — a scripted live match, read back out of .pftrace");
    let path = match crate::perfetto_query::resolve_trace(root, trace_arg.as_deref()) {
        Ok(path) => path,
        Err(error) => {
            println!("  make scenario");
            return crate::scenario::harness_failure(&error);
        }
    };
    let rows = match load(root, &path) {
        Ok(loaded) => loaded,
        Err(error) => {
            return crate::scenario::harness_failure(&error);
        }
    };
    println!("player_tick rows: {}", rows.len());
    let claims = check(&rows);
    let mut ok = true;
    for claim in &claims {
        ok &= claim.report();
    }
    ok
}
