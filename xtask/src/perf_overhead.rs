use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const MIN_PAIRS: usize = 5;

struct Config {
    pairs: usize,
    warmup_pairs: usize,
    binary: PathBuf,
    workdir: PathBuf,
    zone: String,
    commands: String,
    focus: Option<String>,
    /// The recorder the on-arm turns on. `IW4L_PERF` is the Perfetto session;
    /// `IW4L_BENCH` is the in-process recorder that feeds `make bench`, whose
    /// cost is a different question with a different answer.
    var: Recorder,
}

/// Which recorder is being measured. A closed set rather than a free-form
/// variable name: a typo would otherwise run both arms identically and report
/// its own noise as the overhead of something.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Recorder {
    Perf,
    Bench,
}

impl Recorder {
    const fn env(self) -> &'static str {
        match self {
            Self::Perf => "IW4L_PERF",
            Self::Bench => "IW4L_BENCH",
        }
    }

    /// Whether an on-arm of this recorder leaves a `trace.pftrace` behind.
    /// Only the Perfetto session writes one; the bench recorder writes a report
    /// and a manifest, and counting traces would expect a file it never makes.
    const fn writes_trace(self) -> bool {
        matches!(self, Self::Perf)
    }
}

struct Pair {
    index: usize,
    order: String,
    on_ms: f64,
    off_ms: f64,
}

pub fn run(root: &Path, args: Vec<String>) -> bool {
    crate::hr("perf-overhead — paired native Perfetto process benchmark");
    match run_inner(root, parse(root, args)) {
        Ok(()) => true,
        Err(error) => {
            println!("{error}");
            false
        }
    }
}

fn parse(root: &Path, args: Vec<String>) -> Result<Config, String> {
    let mut pairs = 10;
    let mut warmup_pairs = 1;
    let mut binary = root.join("target/play/iw4l");
    let mut workdir = root.to_owned();
    let mut zone = "mp_boneyard".to_owned();
    let mut commands = None;
    let mut focus = None;
    let mut var = Recorder::Perf;
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value after {flag}\n{}", usage()))?;
        match flag.as_str() {
            "--pairs" => {
                pairs = value
                    .parse()
                    .map_err(|_| format!("invalid --pairs value: {value}"))?;
            }
            "--warmup-pairs" => {
                warmup_pairs = value
                    .parse()
                    .map_err(|_| format!("invalid --warmup-pairs value: {value}"))?;
            }
            "--bin" => binary = PathBuf::from(value),
            "--workdir" => workdir = PathBuf::from(value),
            "--zone" => zone = value,
            "--cmds" => commands = Some(value),
            "--focus" => focus = Some(value),
            "--var" => {
                var = match value.as_str() {
                    "IW4L_PERF" => Recorder::Perf,
                    "IW4L_BENCH" => Recorder::Bench,
                    other => {
                        return Err(format!(
                            "unknown --var {other}: expected IW4L_PERF or IW4L_BENCH"
                        ));
                    }
                }
            }
            _ => return Err(format!("unknown flag: {flag}\n{}", usage())),
        }
    }
    if pairs < MIN_PAIRS {
        return Err(format!(
            "--pairs must be at least {MIN_PAIRS}; fewer pairs are a smoke comparison, not an overhead measurement"
        ));
    }
    if !binary.is_file() {
        return Err(format!(
            "benchmark binary not found: {}. Build it once before measuring",
            binary.display()
        ));
    }
    if !workdir.is_dir() {
        return Err(format!(
            "benchmark workdir not found: {}",
            workdir.display()
        ));
    }
    let commands = commands.ok_or_else(usage)?;
    Ok(Config {
        pairs,
        warmup_pairs,
        binary,
        workdir,
        zone,
        commands,
        focus,
        var,
    })
}

fn usage() -> String {
    "usage: cargo xtask perf-overhead --pairs N --warmup-pairs N --bin PATH [--workdir PATH] --zone ZONE --cmds SCRIPT [--focus SELECTOR] [--var IW4L_PERF|IW4L_BENCH]"
        .to_owned()
}

fn run_inner(root: &Path, config: Result<Config, String>) -> Result<(), String> {
    let config = config?;
    println!("binary: {}", config.binary.display());
    println!("workdir: {}", config.workdir.display());
    println!("zone: {}", config.zone);
    if let Some(focus) = &config.focus {
        println!("comparison: focus={focus} / no-focus (IW4L_PERF=1 in both arms)");
    } else {
        println!("comparison: {} on / off", config.var.env());
    }
    println!(
        "design: {} warmup pair(s), {} measured AB/BA pair(s)",
        config.warmup_pairs, config.pairs
    );

    let total = config.warmup_pairs + config.pairs;
    let mut measured = Vec::with_capacity(config.pairs);
    for ordinal in 0..total {
        let on_first = ordinal % 2 == 0;
        let phase = if ordinal < config.warmup_pairs {
            "warmup"
        } else {
            "measure"
        };
        println!(
            "{phase} pair {}/{} order={}",
            ordinal + 1,
            total,
            order_label(&config, on_first)
        );
        let (on_ms, off_ms) = if on_first {
            (run_once(&config, true)?, run_once(&config, false)?)
        } else {
            let off_ms = run_once(&config, false)?;
            let on_ms = run_once(&config, true)?;
            (on_ms, off_ms)
        };
        println!(
            "  on={on_ms:.3} ms off={off_ms:.3} ms delta={:.3} ms",
            on_ms - off_ms
        );
        if ordinal >= config.warmup_pairs {
            measured.push(Pair {
                index: ordinal - config.warmup_pairs + 1,
                order: order_label(&config, on_first),
                on_ms,
                off_ms,
            });
        }
    }

    report(root, &config, &measured)
}

fn order_label(config: &Config, on_first: bool) -> String {
    let on = arm_label(config, true);
    let off = arm_label(config, false);
    if on_first {
        format!("{on}/{off}")
    } else {
        format!("{off}/{on}")
    }
}

fn arm_label(config: &Config, enabled: bool) -> &'static str {
    match (config.focus.is_some(), enabled) {
        (true, true) => "focus",
        (true, false) => "no-focus",
        (false, true) => "on",
        (false, false) => "off",
    }
}

fn run_once(config: &Config, enabled: bool) -> Result<f64, String> {
    let before = trace_runs(&config.workdir)?;
    let mut command = Command::new(&config.binary);
    command
        .current_dir(&config.workdir)
        .args(["--cheats", "map", &config.zone, "--cmds", &config.commands])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Both recorders are cleared first: whichever one the caller inherited
    // would otherwise sit in the off-arm and be measured as no overhead at all.
    command.env_remove("IW4L_PERF_FOCUS");
    command.env_remove("IW4L_PERF");
    command.env_remove("IW4L_BENCH");
    if let Some(focus) = &config.focus {
        command.env("IW4L_PERF", "1");
        if enabled {
            command.env("IW4L_PERF_FOCUS", focus);
        }
    } else if enabled {
        command.env(config.var.env(), "1");
    }
    let started = Instant::now();
    let output = command
        .output()
        .map_err(|error| format!("run {}: {error}", config.binary.display()))?;
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    if !output.status.success() {
        return Err(format!(
            "{} run exited {}\nstdout:\n{}\nstderr:\n{}",
            arm_label(config, enabled),
            output.status,
            output_tail(&output.stdout),
            output_tail(&output.stderr)
        ));
    }
    let after = trace_runs(&config.workdir)?;
    let added = after.difference(&before).count();
    let expected = usize::from(config.focus.is_some() || (enabled && config.var.writes_trace()));
    if added != expected {
        return Err(format!(
            "{} run created {added} trace(s), expected {expected}; do not benchmark alongside another recorder",
            arm_label(config, enabled)
        ));
    }
    Ok(elapsed_ms)
}

fn trace_runs(root: &Path) -> Result<BTreeSet<String>, String> {
    let dir = root.join("iw4l-artifacts/runs");
    if !dir.exists() {
        return Ok(BTreeSet::new());
    }
    let entries =
        std::fs::read_dir(&dir).map_err(|error| format!("read {}: {error}", dir.display()))?;
    Ok(entries
        .flatten()
        .filter(|entry| entry.path().join("trace.pftrace").is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect())
}

fn output_tail(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .rev()
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}

fn report(root: &Path, config: &Config, pairs: &[Pair]) -> Result<(), String> {
    let deltas: Vec<f64> = pairs.iter().map(|pair| pair.on_ms - pair.off_ms).collect();
    let on_mean = mean(pairs.iter().map(|pair| pair.on_ms));
    let off_mean = mean(pairs.iter().map(|pair| pair.off_ms));
    let delta_mean = mean(deltas.iter().copied());
    let mut sorted = deltas.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let median = if sorted.len().is_multiple_of(2) {
        (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
    } else {
        sorted[sorted.len() / 2]
    };
    let variance = deltas
        .iter()
        .map(|delta| (delta - delta_mean).powi(2))
        .sum::<f64>()
        / (deltas.len() - 1) as f64;
    let margin = t_critical_95(deltas.len() - 1) * (variance / deltas.len() as f64).sqrt();
    let low = delta_mean - margin;
    let high = delta_mean + margin;
    let ab = mean_or_none(
        pairs
            .iter()
            .filter(|pair| pair.order == order_label(config, true))
            .map(|pair| pair.on_ms - pair.off_ms),
    );
    let ba = mean_or_none(
        pairs
            .iter()
            .filter(|pair| pair.order == order_label(config, false))
            .map(|pair| pair.on_ms - pair.off_ms),
    );

    let (on, off) = if config.focus.is_some() {
        ("focus", "no-focus")
    } else {
        ("on", "off")
    };

    println!("\npaired result");
    println!("pairs: {}", pairs.len());
    println!("{on} mean: {on_mean:.3} ms");
    println!("{off} mean: {off_mean:.3} ms");
    println!("paired delta mean: {delta_mean:.3} ms");
    println!("paired delta median: {median:.3} ms");
    println!("paired delta 95% CI: [{low:.3}, {high:.3}] ms");
    println!(
        "paired overhead: {:.3}% of {off} mean (95% CI [{:.3}%, {:.3}%])",
        delta_mean * 100.0 / off_mean,
        low * 100.0 / off_mean,
        high * 100.0 / off_mean
    );
    println!(
        "order check: {on}/{off} mean={} ms, {off}/{on} mean={} ms",
        format_optional(ab),
        format_optional(ba)
    );
    println!(
        "inference: {}",
        if low <= 0.0 && high >= 0.0 {
            "the interval includes zero; this run does not resolve overhead from process noise"
        } else {
            "the interval excludes zero for this binary, script, and machine"
        }
    );

    let dir = root.join("iw4l-artifacts/perf-overhead");
    std::fs::create_dir_all(&dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock: {error}"))?
        .as_secs();
    let path = dir.join(format!("{stamp}.csv"));
    let mut csv = format!("pair,order,{on}_ms,{off}_ms,delta_ms\n");
    for pair in pairs {
        csv.push_str(&format!(
            "{},{},{:.6},{:.6},{:.6}\n",
            pair.index,
            pair.order,
            pair.on_ms,
            pair.off_ms,
            pair.on_ms - pair.off_ms
        ));
    }
    std::fs::write(&path, csv).map_err(|error| format!("write {}: {error}", path.display()))?;
    println!("samples: {}", path.display());
    Ok(())
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let values: Vec<f64> = values.collect();
    values.iter().sum::<f64>() / values.len() as f64
}

fn mean_or_none(values: impl Iterator<Item = f64>) -> Option<f64> {
    let values: Vec<f64> = values.collect();
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

fn format_optional(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |value| format!("{value:.3}"))
}

fn t_critical_95(df: usize) -> f64 {
    const TABLE: [f64; 30] = [
        12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160,
        2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056,
        2.052, 2.048, 2.045, 2.042,
    ];
    TABLE.get(df.saturating_sub(1)).copied().unwrap_or(1.96)
}
