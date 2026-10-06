use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;

use crate::{hr, repo_root};

const WRAPPER_URL: &str = "https://get.perfetto.dev/trace_processor";
const SUMMARY: &[&str] = &[
    "wall",
    "FixedUpdate",
    "Update",
    "PreUpdate",
    "PostUpdate",
    "Present",
    "Effects",
];
const TOP_N: usize = 16;
const HOT_SHARE: f64 = 0.15;
const HOT_ABS_MS: f64 = 2.0;
const ENVELOPES: &[&str] = &[
    "wall",
    "FixedUpdate",
    "Update",
    "PreUpdate",
    "PostUpdate",
    "Present",
    "Predict",
    "Effects",
    "Diag",
    "receive_render_world",
    "extract_body",
    "dispatch_render_world",
    "render_thread",
    "render_render",
    "post_execute",
    "post_rebuild",
    "colour_submit",
    "skin_model",
    "fx_update",
    "fx_present",
    "static_sun_fx",
    "static_sun",
    "cull",
];

pub fn bench_gate(trace_arg: Option<PathBuf>) -> bool {
    let root = repo_root();
    hr("bench — Trace Processor SQL on .pftrace");
    match bench_inner(&root, trace_arg.as_deref()) {
        Ok(()) => true,
        Err(error) => {
            println!("{error}");
            false
        }
    }
}

pub fn query_gate(args: Vec<String>) -> bool {
    let root = repo_root();
    hr("query — ad-hoc Trace Processor SQL");
    match query_inner(&root, &args) {
        Ok(()) => true,
        Err(error) => {
            println!("{error}");
            false
        }
    }
}

fn bench_inner(root: &Path, trace_arg: Option<&Path>) -> Result<(), String> {
    let trace = resolve_trace(root, trace_arg)?;
    refuse_sqlite(&trace)?;
    let processor = resolve_processor(root)?;
    println!("processor: {}", processor.display());
    ensure_trace_health(root, &processor, &trace)?;

    let counters = run_sql_file(root, &processor, &trace, "xtask/perfetto/counters.sql")?;
    let hot = run_sql_file(root, &processor, &trace, "xtask/perfetto/hot_durs.sql")?;

    if hot.rows.is_empty() {
        return Err(
            "no typed duration spans in this file — empty TrackEventConfig or a recording that never reached a frame"
                .into(),
        );
    }

    let present: BTreeSet<&str> = hot
        .rows
        .iter()
        .filter_map(|row| row.first().map(String::as_str))
        .collect();
    let missing: Vec<&str> = ENVELOPES
        .iter()
        .copied()
        .filter(|name| !present.contains(name))
        .collect();
    if !missing.is_empty() {
        println!(
            "missing spans (not reached in active window): {}",
            missing.join(", ")
        );
    }

    println!("\nactive gameplay frame summary (ms)");
    print_hot_percentiles(&hot)?;
    println!("\ncounters");
    print_table(&counters);
    Ok(())
}

fn query_inner(root: &Path, args: &[String]) -> Result<(), String> {
    let (trace_arg, sql) = split_query_args(args)?;
    let trace = resolve_trace(root, trace_arg.as_deref())?;
    refuse_sqlite(&trace)?;
    let processor = resolve_processor(root)?;
    ensure_trace_health(root, &processor, &trace)?;
    let focused_owner_pack = Path::new(&sql)
        .file_name()
        .is_some_and(|name| name == "render_owner.sql");
    if focused_owner_pack {
        require_manifest_focus(&trace)?;
    }
    let table = if sql_looks_like_file(&sql) {
        let path = resolve_sql_file(root, &sql)?;
        println!("sql: {}", path.display());
        run_sql_file(
            root,
            &processor,
            &trace,
            path.to_str()
                .ok_or_else(|| "sql path is not utf-8".to_owned())?,
        )?
    } else {
        println!("sql: (inline)");
        run_sql(&processor, &trace, &sql)?
    };
    if focused_owner_pack {
        validate_render_owner_table(&table)?;
    }
    print_table(&table);
    Ok(())
}

fn validate_render_owner_table(table: &Table) -> Result<(), String> {
    if table.rows.is_empty() {
        return Err(
            "MISSING render-owner events: selector was recorded but no plan/submit pair reached this trace"
                .into(),
        );
    }
    let closure = table
        .headers
        .iter()
        .position(|header| header == "closure")
        .ok_or_else(|| "render_owner.sql must return a closure column".to_owned())?;
    let mut closed = 0usize;
    let mut trace_tail = 0usize;
    let mut failures = BTreeMap::<String, usize>::new();
    for row in &table.rows {
        match row.get(closure).map(String::as_str) {
            Some("closed") => closed += 1,
            Some("trace_tail_not_presented") => trace_tail += 1,
            Some(other) => *failures.entry(other.to_owned()).or_default() += 1,
            None => *failures.entry("missing_closure_value".into()).or_default() += 1,
        }
    }
    if closed == 0 {
        return Err("MISSING render-owner closure: no plan reached a submit terminal event".into());
    }
    if !failures.is_empty() {
        let details = failures
            .into_iter()
            .map(|(kind, count)| format!("{kind}={count}"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "render-owner plan/submit contract is open or duplicated: {details}"
        ));
    }
    if trace_tail > 1 {
        return Err(format!(
            "render-owner trace tail has {trace_tail} plans that never reached render extraction"
        ));
    }
    if trace_tail == 1 {
        println!("render-owner: one final plan did not reach render extraction before trace end");
    }
    Ok(())
}

fn split_query_args(args: &[String]) -> Result<(Option<PathBuf>, String), String> {
    match args {
        [] => Err(usage()),
        [only] if sql_looks_like_file(only) || looks_like_sql(only) => Ok((None, only.clone())),
        [trace] => Err(format!(
            "query needs SQL after the trace.\n{}\ntrace was: {trace}",
            usage()
        )),
        [trace, rest @ ..] => Ok((Some(PathBuf::from(trace)), rest.join(" "))),
    }
}

fn usage() -> String {
    "usage: cargo xtask query [trace.pftrace|runs/<uuid>] <file.sql | SELECT ...>\n\
     cargo xtask bench [trace.pftrace|runs/<uuid>]"
        .into()
}

fn looks_like_sql(s: &str) -> bool {
    let t = s.trim_start();
    t.len() >= 6 && t[..6].eq_ignore_ascii_case("select")
}

fn sql_looks_like_file(s: &str) -> bool {
    s.ends_with(".sql") || (Path::new(s).exists() && !looks_like_sql(s) && !is_trace_arg(s))
}

fn is_trace_arg(s: &str) -> bool {
    s.ends_with(".pftrace") || Path::new(s).is_dir()
}

fn resolve_sql_file(root: &Path, spec: &str) -> Result<PathBuf, String> {
    let as_path = Path::new(spec);
    if as_path.is_file() {
        return Ok(as_path.to_path_buf());
    }
    let under_root = root.join(spec);
    if under_root.is_file() {
        return Ok(under_root);
    }
    let pack = root.join("xtask/perfetto").join(spec);
    if pack.is_file() {
        return Ok(pack);
    }
    Err(format!("sql file not found: {spec}"))
}

pub fn resolve_trace(root: &Path, arg: Option<&Path>) -> Result<PathBuf, String> {
    let trace = resolve_trace_path(root, arg)?;
    println!("trace: {}", trace.display());
    println!("{}", manifest_summary(&trace));
    Ok(trace)
}

fn resolve_trace_path(root: &Path, arg: Option<&Path>) -> Result<PathBuf, String> {
    let trace = match arg {
        Some(path) => {
            refuse_sqlite(path)?;
            let abs = if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| root.to_path_buf())
                    .join(path)
            };
            if abs.is_dir() {
                let trace = abs.join("trace.pftrace");
                if trace.is_file() {
                    return Ok(trace);
                }
                return Err(format!("no trace.pftrace in {}", abs.display()));
            }
            if abs.is_file() {
                return Ok(abs);
            }
            let as_run = artifacts_dir(root)
                .join("runs")
                .join(path)
                .join("trace.pftrace");
            if as_run.is_file() {
                return Ok(as_run);
            }
            Err(format!("no such trace: {}", path.display()))
        }
        None => newest_trace(root).ok_or_else(|| {
            "no trace.pftrace under iw4l-artifacts/runs — record with IW4L_PERF=1 (docs/PERF.md)"
                .into()
        }),
    }?;
    Ok(trace)
}

pub fn ensure_trace_health(root: &Path, processor: &Path, trace: &Path) -> Result<(), String> {
    let health = run_sql_file(root, processor, trace, "xtask/perfetto/trace_health.sql")?;
    if health.rows.is_empty() {
        return Ok(());
    }
    let details = health
        .rows
        .iter()
        .map(|row| row.join("/"))
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "trace is incomplete: Perfetto reported errors, drops, or overwrites: {details}"
    ))
}

fn newest_trace(root: &Path) -> Option<PathBuf> {
    let dir = artifacts_dir(root).join("runs");
    let mut best: Option<(SystemTime, PathBuf)> = None;
    for run in std::fs::read_dir(dir).ok()?.flatten() {
        let trace = run.path().join("trace.pftrace");
        let Ok(modified) = trace.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        if best.as_ref().is_none_or(|(t, _)| modified > *t) {
            best = Some((modified, trace));
        }
    }
    best.map(|(_, p)| p)
}

fn artifacts_dir(root: &Path) -> PathBuf {
    std::env::var_os("IW4L_ARTIFACTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("iw4l-artifacts"))
}

fn refuse_sqlite(path: &Path) -> Result<(), String> {
    if path.extension().and_then(|e| e.to_str()) == Some("sqlite") {
        return Err(
            "refusing SQLite — performance and observation truth is .pftrace (docs/PERF.md)".into(),
        );
    }
    Ok(())
}

fn manifest_summary(trace: &Path) -> String {
    let Some(parent) = trace.parent() else {
        return "manifest: MISSING - trace path has no parent; record a new run before citing results"
            .into();
    };
    let path = parent.join("manifest.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return format!(
            "manifest: MISSING {} - provenance is unknown; record a new run before citing results",
            path.display()
        );
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return format!(
            "manifest: MALFORMED {} - provenance is unknown; record a new run before citing results",
            path.display()
        );
    };
    let field = |key: &str| {
        value
            .get(key)
            .filter(|value| !value.is_null())
            .map_or_else(|| "MISSING".into(), serde_json::Value::to_string)
    };
    let workload = value
        .get("command_line")
        .and_then(serde_json::Value::as_array)
        .map(|args| {
            args.iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|command| !command.is_empty())
        .unwrap_or_else(|| "MISSING".into());
    format!(
        "manifest: {} zone={} role={} git={} focus={} workload={workload:?} elapsed_ms={} buffer_size_kb={}",
        path.display(),
        field("zone"),
        field("role"),
        field("git"),
        field("focus"),
        field("elapsed_ms"),
        field("buffer_size_kb")
    )
}

fn require_manifest_focus(trace: &Path) -> Result<(), String> {
    let path = trace
        .parent()
        .ok_or_else(|| "trace path has no parent".to_owned())?
        .join("manifest.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
    match value.get("focus").and_then(serde_json::Value::as_str) {
        Some(focus) if !focus.is_empty() => Ok(()),
        _ => Err(format!(
            "MISSING manifest focus: render_owner.sql requires a run recorded with IW4L_PERF_FOCUS=script_model:<id> ({})",
            path.display()
        )),
    }
}

pub fn resolve_processor(root: &Path) -> Result<PathBuf, String> {
    if let Ok(explicit) = std::env::var("TRACE_PROCESSOR") {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!(
            "TRACE_PROCESSOR is set but not a file: {}",
            path.display()
        ));
    }
    for name in ["trace_processor", "trace_processor_shell"] {
        if let Some(path) = which(name) {
            return Ok(path);
        }
    }
    let cached = root.join("iw4l-artifacts/tools/trace_processor");
    if cached.is_file() {
        return Ok(cached);
    }
    if let Some(prebuilt) = newest_prebuilt() {
        return Ok(prebuilt);
    }
    fetch_wrapper(&cached)?;
    Ok(cached)
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn newest_prebuilt() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let dir = PathBuf::from(home).join(".local/share/perfetto/prebuilts");
    let mut best: Option<(SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let name = path.file_name()?.to_string_lossy();
        if !name.starts_with("trace_processor_shell") {
            continue;
        }
        let Ok(modified) = path.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        if best.as_ref().is_none_or(|(t, _)| modified > *t) {
            best = Some((modified, path));
        }
    }
    best.map(|(_, p)| p)
}

fn fetch_wrapper(dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    println!("fetching {WRAPPER_URL} -> {}", dest.display());
    let status = Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(dest)
        .arg(WRAPPER_URL)
        .status()
        .map_err(|error| {
            format!(
                "curl failed ({error}). Install Trace Processor:\n  \
                 curl -LO {WRAPPER_URL} && chmod +x trace_processor\n  \
                 or set TRACE_PROCESSOR to the binary"
            )
        })?;
    if !status.success() {
        return Err(format!(
            "curl {WRAPPER_URL} exited {status}. Set TRACE_PROCESSOR or install the wrapper."
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(dest)
            .map_err(|error| format!("stat {}: {error}", dest.display()))?
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(dest, permissions)
            .map_err(|error| format!("chmod {}: {error}", dest.display()))?;
    }
    Ok(())
}

pub(crate) fn run_sql_file(
    root: &Path,
    processor: &Path,
    trace: &Path,
    rel: &str,
) -> Result<Table, String> {
    let path = if Path::new(rel).is_absolute() {
        PathBuf::from(rel)
    } else {
        root.join(rel)
    };
    invoke_processor(processor, trace, Some(&path), None)
}

pub fn run_sql(processor: &Path, trace: &Path, sql: &str) -> Result<Table, String> {
    invoke_processor(processor, trace, None, Some(sql))
}

fn invoke_processor(
    processor: &Path,
    trace: &Path,
    sql_file: Option<&Path>,
    sql_text: Option<&str>,
) -> Result<Table, String> {
    let mut command = Command::new(processor);
    command.arg("query").arg("-f");
    if let Some(path) = sql_file {
        command.arg(path).arg(trace);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        return finish_processor(processor, command.output());
    }
    command.arg("-").arg(trace);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("run {}: {error}", processor.display()))?;
    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "trace_processor stdin closed".to_owned())?;
        stdin
            .write_all(sql_text.unwrap_or("").as_bytes())
            .map_err(|error| format!("write SQL: {error}"))?;
    }
    finish_processor(processor, child.wait_with_output())
}

fn finish_processor(
    processor: &Path,
    output: std::io::Result<std::process::Output>,
) -> Result<Table, String> {
    let output = output.map_err(|error| format!("run {}: {error}", processor.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!(
            "trace_processor exited {}:\n{stderr}{stdout}",
            output.status
        ));
    }
    parse_csv(&stdout).ok_or_else(|| format!("no CSV in trace_processor stdout:\n{stdout}"))
}

#[derive(Debug)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

pub(crate) fn cell<'a>(headers: &[String], row: &'a [String], name: &str) -> Option<&'a str> {
    let i = headers.iter().position(|h| h == name)?;
    let s = row.get(i)?.as_str().trim();
    if s.is_empty() || s.eq_ignore_ascii_case("null") || s == "[NULL]" {
        None
    } else {
        Some(s)
    }
}

pub(crate) fn cell_i64(headers: &[String], row: &[String], name: &str) -> Option<i64> {
    cell(headers, row, name)?.parse().ok()
}

pub(crate) fn cell_i32(headers: &[String], row: &[String], name: &str) -> Option<i32> {
    cell(headers, row, name)?.parse().ok()
}

pub(crate) fn cell_f32(headers: &[String], row: &[String], name: &str) -> Option<f32> {
    cell(headers, row, name)?.parse().ok()
}

pub(crate) fn cell_f64(headers: &[String], row: &[String], name: &str) -> Option<f64> {
    cell(headers, row, name)?.parse().ok()
}

fn parse_csv(text: &str) -> Option<Table> {
    let mut headers = None;
    let mut rows = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if !line.starts_with('"') && !line.contains(',') {
            continue;
        }
        let cells = split_csv(line);
        if headers.is_none() {
            headers = Some(cells);
        } else {
            rows.push(cells);
        }
    }
    Some(Table {
        headers: headers?,
        rows,
    })
}

fn split_csv(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut rest = line;
    while !rest.is_empty() {
        if let Some(stripped) = rest.strip_prefix('"') {
            let Some(end) = stripped.find('"') else {
                break;
            };
            cells.push(stripped[..end].replace("\"\"", "\""));
            rest = &stripped[end + 1..];
            if let Some(next) = rest.strip_prefix(',') {
                rest = next;
            } else {
                break;
            }
            continue;
        }
        match rest.find(',') {
            Some(end) => {
                cells.push(rest[..end].to_owned());
                rest = &rest[end + 1..];
            }
            None => {
                cells.push(rest.to_owned());
                break;
            }
        }
    }
    cells
}

fn print_table(table: &Table) {
    if table.headers.is_empty() {
        println!("(empty)");
        return;
    }
    let widths: Vec<usize> = (0..table.headers.len())
        .map(|i| {
            table
                .headers
                .get(i)
                .into_iter()
                .chain(table.rows.iter().filter_map(|row| row.get(i)))
                .map(|s| s.len())
                .max()
                .unwrap_or(0)
        })
        .collect();
    print_row(&table.headers, &widths);
    if table.rows.is_empty() {
        println!("(no rows)");
        return;
    }
    for row in &table.rows {
        print_row(row, &widths);
    }
}

fn print_row(cells: &[String], widths: &[usize]) {
    let mut line = String::new();
    for (i, width) in widths.iter().enumerate() {
        if i > 0 {
            line.push_str("  ");
        }
        let cell = cells.get(i).map(String::as_str).unwrap_or("");
        line.push_str(&format!("{cell:<width$}"));
    }
    println!("{line}");
}

#[derive(Clone)]
struct DurationStats {
    name: String,
    n: usize,
    p50: f64,
    p90: f64,
    p95: f64,
    p99: f64,
    max: f64,
    avg: f64,
}

fn print_hot_percentiles(table: &Table) -> Result<(), String> {
    let mut by_name: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let name_i = column(&table.headers, "name");
    let dur_i = column(&table.headers, "dur");
    let mut incomplete: BTreeMap<String, usize> = BTreeMap::new();
    for row in &table.rows {
        let Some(name) = row.get(name_i) else {
            continue;
        };
        let Some(dur) = row.get(dur_i).and_then(|s| s.parse::<f64>().ok()) else {
            continue;
        };
        if dur < 0.0 {
            *incomplete.entry(name.clone()).or_default() += 1;
            continue;
        }
        by_name.entry(name.clone()).or_default().push(dur / 1e6);
    }

    let bad_incomplete: Vec<String> = incomplete
        .iter()
        .filter(|(name, n)| name.as_str() != "wall" || **n > 1)
        .map(|(name, n)| format!("{name}={n}"))
        .collect();
    if !bad_incomplete.is_empty() {
        return Err(format!(
            "unclosed spans in active window: {}. Begin/end must use the same explicit track",
            bad_incomplete.join(", ")
        ));
    }

    let mut stats: Vec<DurationStats> = by_name
        .into_iter()
        .filter_map(|(name, mut values)| {
            if values.is_empty() {
                return None;
            }
            values.sort_by(|a, b| a.total_cmp(b));
            let n = values.len();
            Some(DurationStats {
                name,
                n,
                p50: percentile(&values, 0.50),
                p90: percentile(&values, 0.90),
                p95: percentile(&values, 0.95),
                p99: percentile(&values, 0.99),
                max: values[n - 1],
                avg: values.iter().sum::<f64>() / n as f64,
            })
        })
        .collect();
    let wall = stats.iter().find(|row| row.name == "wall");
    let wall_p90 = wall.map(|row| row.p90);

    println!(
        "{:<16} {:>6} {:>8} {:>8} {:>8} {:>8} {:>8}",
        "name", "n", "p50", "p90", "p95", "p99", "max"
    );
    for name in SUMMARY {
        match stats.iter().find(|row| row.name == *name) {
            Some(row) => print_duration_row(row, wall_p90, *name == "wall", false),
            _ => println!("{name:<16}  (not in this trace)"),
        }
    }

    if let Some(wall) = wall {
        let values = table
            .rows
            .iter()
            .filter(|row| row.get(name_i).is_some_and(|name| name == "wall"))
            .filter_map(|row| row.get(dur_i)?.parse::<f64>().ok())
            .filter(|dur| *dur >= 0.0)
            .map(|dur| dur / 1e6)
            .collect::<Vec<_>>();
        let hitch_30 = values.iter().filter(|ms| **ms >= 1000.0 / 30.0).count();
        let hitch_20 = values.iter().filter(|ms| **ms >= 50.0).count();
        println!(
            "wall: fps_p50={:.1} hitches>=33.3ms={} >=50ms={} avg={:.3}ms",
            1000.0 / wall.p50.max(f64::EPSILON),
            hitch_30,
            hitch_20,
            wall.avg
        );
    }

    stats.retain(|row| row.name != "wall");
    stats.sort_by(|a, b| b.p90.total_cmp(&a.p90));
    println!("\nhot paths by p90 (active gameplay)");
    println!(
        "{:<20} {:>6} {:>8} {:>8} {:>8} {:>8} {:>7}",
        "name", "n", "p50", "p90", "p99", "max", "%wall"
    );
    for row in stats.iter().filter(|row| row.p90 >= 0.05).take(TOP_N) {
        print_duration_row(row, wall_p90, false, true);
    }
    Ok(())
}

fn print_duration_row(row: &DurationStats, wall_p90: Option<f64>, is_wall: bool, with_share: bool) {
    let hot = is_hot(row.p90, wall_p90, is_wall);
    if with_share {
        let share = wall_p90
            .filter(|wall| *wall > 0.0)
            .map(|wall| format!("{:.0}%", row.p90 * 100.0 / wall))
            .unwrap_or_else(|| "-".into());
        println!(
            "{:<20} {:>6} {:>8.3} {:>8.3} {:>8.3} {:>8.3} {:>7}{}",
            row.name,
            row.n,
            row.p50,
            row.p90,
            row.p99,
            row.max,
            share,
            if hot { "  << HOT" } else { "" }
        );
    } else {
        println!(
            "{:<16} {:>6} {:>8.3} {:>8.3} {:>8.3} {:>8.3} {:>8.3}{}",
            row.name,
            row.n,
            row.p50,
            row.p90,
            row.p95,
            row.p99,
            row.max,
            if hot { "  << HOT" } else { "" }
        );
    }
}

fn is_hot(value: f64, wall: Option<f64>, is_wall: bool) -> bool {
    if is_wall {
        return value > 1000.0 / 60.0;
    }
    value
        >= wall
            .map(|ms| ms * HOT_SHARE)
            .unwrap_or(HOT_ABS_MS)
            .max(HOT_ABS_MS)
}

fn column(headers: &[String], name: &str) -> usize {
    headers.iter().position(|h| h == name).unwrap_or(0)
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}
