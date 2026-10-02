//! The owner-approved end-to-end scenarios; see README.md.

#[cfg(test)]
mod jak_parity;
mod report;
mod runner;
mod scenario;
mod scenarios {
    pub mod heavy_gameplay_lifecycle;
}

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

use scenario::{MapB, Place, ResolvedPlace, ResolvedScene};
use scenarios::heavy_gameplay_lifecycle::{self as hgl, SCENARIO};

#[derive(Clone, Copy, PartialEq)]
enum CacheMode {
    Cold,
    Shared,
}

struct Args {
    scenario: String,
    seed: Option<u64>,
    replay: Option<PathBuf>,
    cache: CacheMode,
    bin: PathBuf,
}

fn main() {
    let root = repo_root();
    let args = match parse_args(&root) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            eprintln!(
                "usage: approved_tests heavy_gameplay_lifecycle [--seed N | --replay run.json] \
                 [--cache cold|shared] [--bin path]"
            );
            std::process::exit(2);
        }
    };
    if args.scenario != SCENARIO.name {
        eprintln!(
            "approved scenarios: {} (asked for {:?})",
            SCENARIO.name, args.scenario
        );
        std::process::exit(2);
    }
    std::process::exit(match run(&root, &args) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(message) => {
            eprintln!("approved_tests: {message}");
            1
        }
    });
}

fn repo_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .unwrap_or(manifest)
        .to_path_buf()
}

fn parse_args(root: &Path) -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let scenario = it.next().ok_or("which scenario?")?;
    let mut args = Args {
        scenario,
        seed: None,
        replay: None,
        cache: CacheMode::Cold,
        bin: root.join("target/play/iw4l"),
    };
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--seed" => {
                let raw = value()?;
                args.seed = Some(
                    raw.parse()
                        .map_err(|_| format!("--seed {raw}: not a u64"))?,
                );
            }
            "--replay" => args.replay = Some(PathBuf::from(value()?)),
            "--cache" => {
                args.cache = match value()?.as_str() {
                    "cold" => CacheMode::Cold,
                    "shared" => CacheMode::Shared,
                    other => return Err(format!("--cache {other}: cold or shared")),
                }
            }
            "--bin" => args.bin = PathBuf::from(value()?),
            other => return Err(format!("unknown flag {other}")),
        }
    }
    if args.seed.is_some() && args.replay.is_some() {
        return Err("--seed and --replay are exclusive: a replay keeps its own".into());
    }
    Ok(args)
}

struct Replay {
    source: PathBuf,
    seed: u64,
    map_b: String,
    landed: Vec<(String, [f32; 3], f32)>,
}

fn read_replay(path: &Path) -> Result<Replay, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let run: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let seed = run["seed"].as_u64().ok_or("replay: no seed")?;
    let map_b = run["maps"]["b"]
        .as_str()
        .ok_or("replay: no maps.b")?
        .to_owned();
    let mut landed = Vec::new();
    for scene in run["scenes"].as_array().into_iter().flatten() {
        let phase = scene["phase"].as_str().unwrap_or("").to_owned();
        let spawn = &scene["observed"]["spawn"];
        let origin = spawn["origin"].as_array().map(|a| {
            let f = |i: usize| a.get(i).and_then(Value::as_f64).unwrap_or(0.0) as f32;
            [f(0), f(1), f(2)]
        });
        if let (Some(origin), Some(yaw)) = (origin, spawn["yaw"].as_f64()) {
            landed.push((phase, origin, yaw as f32));
        }
    }
    Ok(Replay {
        source: path.to_path_buf(),
        seed,
        map_b,
        landed,
    })
}

fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn fresh_seed() -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    mix(now ^ (u64::from(std::process::id()) << 32))
}

fn resolve_scenes(
    tag: &'static str,
    map: &str,
    map_index: u64,
    seed: u64,
    replay: Option<&Replay>,
) -> Vec<ResolvedScene> {
    SCENARIO
        .scenes
        .iter()
        .enumerate()
        .map(|(index, scene)| {
            let phase = format!("{tag}.scene_{}", index + 1);
            let replayed = replay
                .and_then(|r| r.landed.iter().find(|(p, ..)| *p == phase))
                .map(|(_, origin, yaw)| ResolvedPlace::At {
                    origin: *origin,
                    yaw: *yaw,
                });
            let place = replayed.unwrap_or(match scene.place {
                Place::At { origin, yaw } => ResolvedPlace::At { origin, yaw },
                Place::Random => {
                    ResolvedPlace::Seeded(mix(seed ^ mix(map_index * 16 + index as u64 + 1)))
                }
            });
            ResolvedScene {
                tag,
                map: map.to_owned(),
                index,
                name: scene.name,
                place,
                weapon: scene.weapon.as_ref(),
                commands: scenario::scene_commands(scene, place),
            }
        })
        .collect()
}

fn run(root: &Path, args: &Args) -> Result<bool, String> {
    asset_transport::load_dotenv();
    let replay = args.replay.as_deref().map(read_replay).transpose()?;
    let seed = replay
        .as_ref()
        .map(|r| r.seed)
        .or(args.seed)
        .unwrap_or_else(fresh_seed);

    if !args.bin.is_file() {
        return Err(format!(
            "{} is not built (make approved builds it)",
            args.bin.display()
        ));
    }

    let games = asset_transport::games_root_from_env()?;
    let installed = asset_transport::list_mp_maps(&games);
    let (map_b, map_b_rule, candidates): (Option<String>, String, Vec<String>) =
        match (&replay, &SCENARIO.map_b) {
            (Some(r), _) => (Some(r.map_b.clone()), "replay".into(), Vec::new()),
            (None, MapB::Fixed(zone)) => (Some((*zone).to_owned()), "fixed".into(), Vec::new()),
            (None, MapB::FromInstalled { game }) => {
                let prefix = format!("{game}:");
                let candidates: Vec<String> = installed
                    .iter()
                    .filter_map(|m| m.strip_prefix(&prefix))
                    .filter(|m| *m != SCENARIO.map_a)
                    .map(str::to_owned)
                    .collect();
                let pick = (!candidates.is_empty()).then(|| {
                    candidates[(mix(seed ^ 0xB) % candidates.len() as u64) as usize].clone()
                });
                (
                    pick,
                    format!(
                        "seeded pick among installed {game} mp zones, {} excluded",
                        SCENARIO.map_a
                    ),
                    candidates,
                )
            }
        };
    let map_a_installed = installed
        .iter()
        .any(|m| m.split_once(':').is_some_and(|(_, z)| z == SCENARIO.map_a));

    let run_id = format!("{}-{seed:016x}", utc_stamp());
    let run_dir = root.join("iw4l-artifacts/approved-tests").join(&run_id);
    std::fs::create_dir_all(run_dir.parent().expect("has parent"))
        .map_err(|e| format!("create run root: {e}"))?;
    std::fs::create_dir(&run_dir).map_err(|e| format!("create {}: {e}", run_dir.display()))?;

    let mut manifest = json!({
        "scenario": SCENARIO.name,
        "run_id": run_id,
        "result": "running",
        "seed": seed,
        "replay_of": replay.as_ref().map(|r| r.source.display().to_string()),
        "build": report::build_facts(root, &args.bin),
        "platform": report::platform_facts(),
        "content": { "games_root": games.0.display().to_string() },
        "gametype": SCENARIO.gametype,
        "class": SCENARIO.class,
        "maps": {
            "a": SCENARIO.map_a,
            "a_installed": map_a_installed,
            "b": map_b,
            "b_rule": map_b_rule,
            "b_candidates": candidates,
        },
        "bots": { "players_target": SCENARIO.players, "bots_requested": SCENARIO.players - 1 },
        "timeouts_secs": { "phase": SCENARIO.phase_timeout_secs, "quit": SCENARIO.quit_timeout_secs },
    });

    let Some(map_b) = map_b else {
        manifest["result"] = "blocked".into();
        manifest["failure"] = "no second installed map for the gametype: a one-map run is not the lifecycle this scenario tests".into();
        report::write(&run_dir, &manifest)?;
        println!("blocked: no map B; {}", run_dir.join("run.json").display());
        return Ok(false);
    };
    if !map_a_installed {
        manifest["result"] = "blocked".into();
        manifest["failure"] =
            format!("{} is not installed under the games root", SCENARIO.map_a).into();
        report::write(&run_dir, &manifest)?;
        return Ok(false);
    }

    let scenes_a = resolve_scenes("a", SCENARIO.map_a, 0, seed, replay.as_ref());
    let scenes_b = resolve_scenes("b", &map_b, 1, seed, replay.as_ref());
    let phases = hgl::phases(&map_b, &scenes_a, &scenes_b);
    let script = scenario::script(&phases);
    let child_args = vec![
        "map".to_owned(),
        SCENARIO.map_a.to_owned(),
        "--cmds".to_owned(),
        script.clone(),
    ];

    let cache = prepare_cache(root, &run_dir, args.cache)?;
    manifest["cache"] = cache;
    let bin = stage_binary(&args.bin, &run_dir)?;
    manifest["command_line"] = json!([bin.display().to_string(), child_args]);
    manifest["script"] = script.into();
    manifest["phases_declared"] = phases
        .iter()
        .map(|p| p.name.clone())
        .collect::<Vec<_>>()
        .into();
    manifest["scenes"] = scenes_a
        .iter()
        .chain(&scenes_b)
        .map(ResolvedScene::to_json)
        .collect::<Vec<_>>()
        .into();
    report::write(&run_dir, &manifest)?;
    println!("run: {}", run_dir.display());

    let outcome = runner::run(runner::Launch {
        bin: &bin,
        cwd: &run_dir,
        args: child_args,
        env: vec![
            ("IW4L_PERF", "1".into()),
            ("IW4L_GAMETYPE", SCENARIO.gametype.into()),
        ],
        phase_timeout: Duration::from_secs(SCENARIO.phase_timeout_secs),
        quit_timeout: Duration::from_secs(SCENARIO.quit_timeout_secs),
    });

    let passed = report::finish(&run_dir, &mut manifest, &phases, &outcome);
    report::write(&run_dir, &manifest)?;
    println!(
        "{}: {}",
        manifest["result"].as_str().unwrap_or("?"),
        run_dir.join("run.json").display()
    );
    Ok(passed)
}

/// On Windows the launcher enters the folder holding its executable, so the binary must sit in the run directory.
fn stage_binary(bin: &Path, run_dir: &Path) -> Result<PathBuf, String> {
    if !cfg!(windows) {
        return Ok(bin.to_path_buf());
    }
    let exe = bin.with_extension("exe");
    let bin = if bin.exists() { bin } else { exe.as_path() };
    let name = bin
        .file_name()
        .ok_or_else(|| format!("--bin {} names no file", bin.display()))?;
    let staged = run_dir.join(name);
    std::fs::hard_link(bin, &staged)
        .or_else(|_| std::fs::copy(bin, &staged).map(drop))
        .map_err(|e| format!("stage {} into {}: {e}", bin.display(), run_dir.display()))?;
    Ok(staged)
}

fn prepare_cache(root: &Path, run_dir: &Path, mode: CacheMode) -> Result<Value, String> {
    let child_artifacts = run_dir.join("iw4l-artifacts");
    let child_cache = child_artifacts.join("cache");
    let uncontrolled = json!([
        "OS page cache",
        "GPU driver shader caches (e.g. ~/.cache/mesa_shader_cache)",
        "games install on disk",
    ]);
    match mode {
        CacheMode::Cold => {
            let cold = !child_cache.exists();
            Ok(json!({
                "mode": "cold",
                "cold_load": cold,
                "root": child_cache.display().to_string(),
                "reset": ["every IW4L-managed loading product: iw4l-artifacts/cache (localize, mips, nav, wgsl, xwma_pcm) starts empty in the run directory"],
                "kept": ["the repository's iw4l-artifacts/cache, untouched", ".env and the games root"],
                "not_controlled": uncontrolled,
            }))
        }
        CacheMode::Shared => {
            let shared = root.join("iw4l-artifacts/cache");
            std::fs::create_dir_all(&child_artifacts)
                .map_err(|e| format!("create {}: {e}", child_artifacts.display()))?;
            #[cfg(unix)]
            let linked = std::os::unix::fs::symlink(&shared, &child_cache);
            #[cfg(windows)]
            let linked = std::os::windows::fs::symlink_dir(&shared, &child_cache);
            linked.map_err(|e| format!("link {}: {e}", child_cache.display()))?;
            Ok(json!({
                "mode": "shared",
                "cold_load": false,
                "root": shared.display().to_string(),
                "reset": [],
                "kept": ["the repository's iw4l-artifacts/cache, reused and written to"],
                "not_controlled": uncontrolled,
            }))
        }
    }
}

fn utc_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}
