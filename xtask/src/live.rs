use std::path::{Path, PathBuf};

use crate::perfetto_query::{
    Table, cell, cell_i64, ensure_trace_health, resolve_processor, resolve_trace, run_sql_file,
};
use crate::scenario::Claim;

const BONEYARD_AMBIENT: &str = "ambient_mp_desert";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Recipe {
    Swap,
    Replace,
    PlayIn,
    DemoOut,
    DemoMap,
}

impl Recipe {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "swap" | "lifecycle-swap" => Some(Self::Swap),
            "replace" | "lifecycle-replace" => Some(Self::Replace),
            "play-in" | "lifecycle-play-in" => Some(Self::PlayIn),
            "demo-out" | "lifecycle-demo-out" => Some(Self::DemoOut),
            "demo-map" | "lifecycle-demo-map" => Some(Self::DemoMap),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Swap => "swap",
            Self::Replace => "replace",
            Self::PlayIn => "play-in",
            Self::DemoOut => "demo-out",
            Self::DemoMap => "demo-map",
        }
    }
}

struct Row {
    ts: i64,
    name: String,
    reason: Option<String>,
    zone: Option<String>,
    spawned: Option<i64>,
    gpu_plan: Option<i64>,
    glass_n: Option<i64>,
    running: Option<i64>,
    movers: Option<i64>,
    loopback_pending: Option<i64>,
    booted: Option<i64>,
    alias: Option<String>,
    phase: Option<String>,
    target: Option<String>,
    present: Option<i64>,
    quit_on_end: Option<i64>,
    present_choice: Option<String>,
    presented_has_ps: Option<i64>,
}

pub fn live_gate(root: &Path, recipe: Option<String>, trace: Option<PathBuf>) -> bool {
    let Some(recipe_s) = recipe else {
        println!(
            "usage: cargo xtask live <swap|replace|play-in|demo-out|demo-map> [trace.pftrace]"
        );
        println!("  make lifecycle-swap");
        return false;
    };
    let Some(recipe) = Recipe::parse(&recipe_s) else {
        println!("unknown lifecycle recipe: {recipe_s}");
        println!("usage: cargo xtask live <swap|replace|play-in|demo-out|demo-map> [trace]");
        return false;
    };
    crate::hr(&format!("live — lifecycle {} on .pftrace", recipe.label()));
    let path = match resolve_trace(root, trace.as_deref()) {
        Ok(path) => path,
        Err(error) => {
            println!("  make lifecycle-{}", recipe.label());
            return crate::scenario::harness_failure(&error);
        }
    };
    let rows = match load(root, &path) {
        Ok(rows) => rows,
        Err(error) => {
            return crate::scenario::harness_failure(&error);
        }
    };
    let claims = check(recipe, &rows);
    let mut ok = true;
    for claim in &claims {
        ok &= claim.report();
    }
    ok
}

fn load(root: &Path, trace: &Path) -> Result<Vec<Row>, String> {
    let processor = resolve_processor(root)?;
    ensure_trace_health(root, &processor, trace)?;
    let table = run_sql_file(root, &processor, trace, "xtask/perfetto/session.sql")?;
    Ok(parse_rows(&table))
}

fn check(recipe: Recipe, rows: &[Row]) -> Vec<Claim> {
    match recipe {
        Recipe::Swap => vec![
            torn_claim(rows, "Disconnect"),
            world_hold_claim(rows),
            sim_hold_claim(rows),
            ambient_hold_claim(rows),
            rust_after_torn_claim(rows),
            world_ready_after_rust_claim(rows),
            rust_ambient_not_desert_claim(rows),
        ],
        Recipe::Replace => vec![
            torn_claim(rows, "Replaced"),
            world_hold_claim(rows),
            sim_hold_claim(rows),
            rust_after_torn_claim(rows),
            world_ready_after_rust_claim(rows),
        ],
        Recipe::PlayIn => vec![
            swap_demo_claim(rows, "swap_in"),
            theater_on_claim(rows, Some(0)),
        ],
        Recipe::DemoOut => vec![
            swap_demo_claim(rows, "swap_out"),
            theater_on_claim(rows, None),
            torn_after_theater_claim(rows, "Disconnect"),
            cgame_hold_claim(rows),
        ],
        Recipe::DemoMap => vec![
            swap_demo_claim(rows, "swap_map"),
            theater_on_claim(rows, None),
            rust_after_theater_claim(rows),
        ],
    }
}

fn torn_claim(rows: &[Row], reason: &str) -> Claim {
    let mut claim = Claim::new("L1", "match_torn occupancy left");
    let n = rows
        .iter()
        .filter(|r| r.name == "match_torn" && r.reason.as_deref() == Some(reason))
        .count();
    claim.check(n > 0, format!("match_torn {reason} events={n}"));
    claim
}

fn world_hold_claim(rows: &[Row]) -> Claim {
    let mut claim = Claim::new("L2", "world shutdown emptied spawned / gpu_plan / glass");
    let hits = rows
        .iter()
        .filter(|r| {
            r.name == "world_hold"
                && r.spawned == Some(0)
                && r.gpu_plan == Some(0)
                && r.glass_n == Some(0)
        })
        .count();
    claim.check(
        hits > 0,
        format!("world_hold spawned=0 gpu_plan=0 glass_n=0 events={hits}"),
    );
    claim
}

fn sim_hold_claim(rows: &[Row]) -> Claim {
    let mut claim = Claim::new("L3", "game shutdown + loopback reset");
    let hits = rows
        .iter()
        .filter(|r| {
            r.name == "sim_hold"
                && r.running == Some(0)
                && r.movers == Some(0)
                && r.loopback_pending == Some(0)
        })
        .count();
    claim.check(
        hits > 0,
        format!("sim_hold running=0 movers=0 loopback_pending=0 events={hits}"),
    );
    claim
}

fn ambient_hold_claim(rows: &[Row]) -> Claim {
    let mut claim = Claim::new("L4", "ambient stop cleared MapAmbientBooted");
    let hits = rows
        .iter()
        .filter(|r| r.name == "ambient_hold" && r.booted == Some(0))
        .count();
    claim.check(hits > 0, format!("ambient_hold booted=0 events={hits}"));
    claim
}

fn rust_after_torn_claim(rows: &[Row]) -> Claim {
    let mut claim = Claim::new(
        "L5",
        "match_installed mp_rust after a torn (D187, not leftover argv)",
    );
    let rust = rows
        .iter()
        .filter(|r| r.name == "match_installed" && r.zone.as_deref() == Some("mp_rust"))
        .last();
    let Some(rust) = rust else {
        claim.check(false, "no match_installed zone=mp_rust");
        return claim;
    };
    let torn_before = rows
        .iter()
        .filter(|r| r.name == "match_torn" && r.ts <= rust.ts)
        .count();
    claim.check(
        torn_before > 0,
        format!(
            "mp_rust install ts={} after {torn_before} match_torn",
            rust.ts
        ),
    );
    claim
}

fn world_ready_after_rust_claim(rows: &[Row]) -> Claim {
    let mut claim = Claim::new("L6", "wait world: WorldScene.spawned after rust install");
    let Some(rust) = rows
        .iter()
        .filter(|r| r.name == "match_installed" && r.zone.as_deref() == Some("mp_rust"))
        .last()
    else {
        claim.check(false, "no rust install to order world_ready against");
        return claim;
    };
    let ready = rows
        .iter()
        .filter(|r| r.name == "world_ready" && r.spawned == Some(1) && r.ts >= rust.ts)
        .count();
    claim.check(
        ready > 0,
        format!(
            "world_ready spawned=1 after rust ts={} events={ready}",
            rust.ts
        ),
    );
    claim
}

fn rust_ambient_not_desert_claim(rows: &[Row]) -> Claim {
    let mut claim = Claim::new("L7", "ambient boot is not leftover boneyard desert");
    let Some(rust) = rows
        .iter()
        .filter(|r| r.name == "match_installed" && r.zone.as_deref() == Some("mp_rust"))
        .last()
    else {
        claim.check(false, "no rust install");
        return claim;
    };
    let boots: Vec<&Row> = rows
        .iter()
        .filter(|r| r.name == "ambient_boot" && r.ts >= rust.ts)
        .collect();
    if boots.is_empty() {
        claim.check(false, "no ambient_boot after rust install (wait ambient)");
        return claim;
    }
    let leftover = boots
        .iter()
        .filter(|r| r.alias.as_deref() == Some(BONEYARD_AMBIENT))
        .count();
    claim.check(
        leftover == 0,
        format!(
            "ambient_boot after rust={} leftover {BONEYARD_AMBIENT}={leftover}",
            boots.len()
        ),
    );
    if let Some(last) = boots.last() {
        claim.check(
            last.zone.as_deref() == Some("mp_rust"),
            format!(
                "last ambient_boot zone={:?} alias={:?}",
                last.zone, last.alias
            ),
        );
    }
    claim
}

fn swap_demo_claim(rows: &[Row], stem: &str) -> Claim {
    let needle = format!("demo:{stem}");
    let mut claim = Claim::new("L8", "swap requested theater demo");
    let n = rows
        .iter()
        .filter(|r| {
            r.name == "swap"
                && r.phase.as_deref() == Some("requested")
                && r.target.as_deref() == Some(needle.as_str())
        })
        .count();
    claim.check(n > 0, format!("swap requested {needle} events={n}"));
    claim
}

fn theater_on_claim(rows: &[Row], quit: Option<i64>) -> Claim {
    let mut claim = Claim::new("L9", "ReplayPlayback inserted (D208 theater=1)");
    let hits = rows
        .iter()
        .filter(|r| {
            r.name == "theater"
                && r.present == Some(1)
                && quit.is_none_or(|q| r.quit_on_end == Some(q))
        })
        .count();
    claim.check(
        hits > 0,
        format!("theater present=1 quit_on_end={quit:?} events={hits}"),
    );
    claim
}

fn torn_after_theater_claim(rows: &[Row], reason: &str) -> Claim {
    let mut claim = Claim::new("L10", "match_torn after theater occupancy");
    let Some(theater) = rows
        .iter()
        .filter(|r| r.name == "theater" && r.present == Some(1))
        .map(|r| r.ts)
        .min()
    else {
        claim.check(false, "no theater present=1 to order torn against");
        return claim;
    };
    let n = rows
        .iter()
        .filter(|r| {
            r.name == "match_torn" && r.reason.as_deref() == Some(reason) && r.ts >= theater
        })
        .count();
    claim.check(
        n > 0,
        format!("match_torn {reason} after theater ts={theater} events={n}"),
    );
    claim
}

fn rust_after_theater_claim(rows: &[Row]) -> Claim {
    let mut claim = Claim::new("L11", "match_installed mp_rust after theater (demo→map)");
    let Some(theater) = rows
        .iter()
        .filter(|r| r.name == "theater" && r.present == Some(1))
        .map(|r| r.ts)
        .min()
    else {
        claim.check(false, "no theater present=1");
        return claim;
    };
    let n = rows
        .iter()
        .filter(|r| {
            r.name == "match_installed" && r.zone.as_deref() == Some("mp_rust") && r.ts >= theater
        })
        .count();
    claim.check(
        n > 0,
        format!("mp_rust install after theater ts={theater} events={n}"),
    );
    claim
}

fn cgame_hold_claim(rows: &[Row]) -> Claim {
    let mut claim = Claim::new(
        "L12",
        "cgame_hold no_next / presented_has_ps=0 (D211 leftover Alive)",
    );
    let hits = rows
        .iter()
        .filter(|r| {
            r.name == "cgame_hold"
                && r.present_choice.as_deref() == Some("no_next")
                && r.presented_has_ps == Some(0)
        })
        .count();
    claim.check(
        hits > 0,
        format!("cgame_hold no_next presented_has_ps=0 events={hits}"),
    );
    claim
}

fn parse_rows(table: &Table) -> Vec<Row> {
    table
        .rows
        .iter()
        .map(|cells| Row {
            ts: cell_i64(&table.headers, cells, "ts").unwrap_or(0),
            name: cell(&table.headers, cells, "name").unwrap_or("").to_owned(),
            reason: cell(&table.headers, cells, "reason").map(str::to_owned),
            zone: cell(&table.headers, cells, "zone").map(str::to_owned),
            spawned: cell_i64(&table.headers, cells, "spawned"),
            gpu_plan: cell_i64(&table.headers, cells, "gpu_plan"),
            glass_n: cell_i64(&table.headers, cells, "glass_n"),
            running: cell_i64(&table.headers, cells, "running"),
            movers: cell_i64(&table.headers, cells, "movers"),
            loopback_pending: cell_i64(&table.headers, cells, "loopback_pending"),
            booted: cell_i64(&table.headers, cells, "booted"),
            alias: cell(&table.headers, cells, "alias").map(str::to_owned),
            phase: cell(&table.headers, cells, "phase").map(str::to_owned),
            target: cell(&table.headers, cells, "target").map(str::to_owned),
            present: cell_i64(&table.headers, cells, "present"),
            quit_on_end: cell_i64(&table.headers, cells, "quit_on_end"),
            present_choice: cell(&table.headers, cells, "present_choice").map(str::to_owned),
            presented_has_ps: cell_i64(&table.headers, cells, "presented_has_ps"),
        })
        .collect()
}
