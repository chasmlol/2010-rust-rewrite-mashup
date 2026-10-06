use std::process::ExitCode;

use xtask::chaos;
use xtask::dotenv::Env;
use xtask::frame_budget;
use xtask::live;
use xtask::net_feel;
use xtask::repo_root;
use xtask::scenario;
use xtask::shell::Res;

const TRACE_TOOLS: &[&str] = &[
    "scenario [trace.pftrace]",
    "chaos [trace.pftrace]",
    "net-feel [trace.pftrace]",
    "live <swap|replace|play-in|demo-out|demo-map> [trace.pftrace]",
    "bench [trace.pftrace]",
    "perf-overhead --pairs N --bin PATH [--workdir PATH] --zone ZONE --cmds SCRIPT [--focus SELECTOR]",
    "query [trace] <sql>",
    "frame-budget [trace.pftrace]",
    "bundle-zip ARCHIVE FILE...",
];

/// The clone lifecycle and the formatter. These touch git in this working
/// tree and nothing outside it.
const REPO_TOOLS: &[&str] = &[
    "mr new <name>",
    "mr ship <name>",
    "mr ls",
    "mr fmt FILE.rs...",
    "publish-check",
    "licenses",
];

/// Everything that leaves this machine. These read `.env` for the host, the
/// root and the CA; none of them touch an engine crate.
const SHIP_TOOLS: &[&str] = &[
    "windows [build|setup]",
    "certs <host>",
    "release <prod|dev|bundles>",
    "publish <prod|dev>",
    "provision",
    "logs <prod|dev> [--since 2h]",
    "master <install|update|status|logs|uninstall> user@host [--channel prod] [--since 10min]",
    "loc",
];

fn print_tools() {
    println!("xtask trace tools:");
    println!("  {}", TRACE_TOOLS.join(" | "));
    println!("xtask repo tools:");
    for tool in REPO_TOOLS {
        println!("  {tool}");
    }
    println!("xtask ship tools:");
    for tool in SHIP_TOOLS {
        println!("  {tool}");
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().cloned().unwrap_or_else(|| "all".to_string());
    let rest = args.get(1..).unwrap_or(&[]);

    if cmd == "all" || cmd == "help" || cmd == "-h" || cmd == "--help" {
        print_tools();
        return ExitCode::SUCCESS;
    }
    // The repo tools and the ship commands report their own failure and carry no pass/fail verdict;
    // the trace gates below print "ok"/"failed" because that verdict is the point.
    if let Some(result) = repo(&cmd, rest).or_else(|| ship(&cmd, rest)) {
        return match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{cmd}: {error}");
                ExitCode::FAILURE
            }
        };
    }
    let ok = match cmd.as_str() {
        "scenario" => {
            scenario::scenario_gate(&repo_root(), rest.first().map(std::path::PathBuf::from))
        }
        "chaos" => chaos::chaos_gate(&repo_root(), rest.first().map(std::path::PathBuf::from)),
        "net-feel" => {
            net_feel::net_feel_gate(&repo_root(), rest.first().map(std::path::PathBuf::from))
        }
        "live" => live::live_gate(
            &repo_root(),
            rest.first().cloned(),
            rest.get(1).map(std::path::PathBuf::from),
        ),
        "bench" => xtask::perfetto_query::bench_gate(rest.first().map(std::path::PathBuf::from)),
        "perf-overhead" => xtask::perf_overhead::run(&repo_root(), rest.to_vec()),
        "query" => xtask::perfetto_query::query_gate(rest.to_vec()),
        "frame-budget" => {
            frame_budget::frame_budget(rest.iter().map(std::path::PathBuf::from).collect())
        }
        other => {
            println!("unknown command: {other}");
            print_tools();
            false
        }
    };
    finish(ok)
}

/// `None` when `cmd` is not a repo tool. These read no `.env`: a clone is made
/// and landed with nothing but git and rustfmt.
fn repo(cmd: &str, rest: &[String]) -> Option<Res<()>> {
    let root = repo_root();
    match cmd {
        "licenses" => Some(xtask::licenses::run_cli(&root)),
        "mr" => Some(xtask::mrs::run_cli(&root, rest)),
        "publish-check" => Some(xtask::publish_check::run_cli(&root)),
        _ => None,
    }
}

/// `None` when `cmd` is not a ship command, so the trace gates get their turn.
fn ship(cmd: &str, rest: &[String]) -> Option<Res<()>> {
    if cmd == "bundle-zip" {
        return Some(xtask::bundle_zip::run(rest.to_vec()));
    }
    if !matches!(
        cmd,
        "windows" | "certs" | "release" | "publish" | "provision" | "logs" | "master" | "loc"
    ) {
        return None;
    }
    let root = repo_root();
    Some(Env::load(&root).and_then(|env| match cmd {
        "windows" => xtask::windows::run_cli(&env, rest),
        "certs" => xtask::certs::run_cli(&env, rest),
        "release" => xtask::release::run_cli(&root, &env, rest),
        "publish" => xtask::publish::run_cli(&root, &env, rest),
        "provision" => xtask::provision::run_cli(&root, &env, rest),
        "logs" => xtask::publish::logs(&env, rest),
        "master" => xtask::master::run_cli(&root, &env, rest),
        "loc" => xtask::loc::run_cli(&root),
        other => Err(format!("unknown command: {other}")),
    }))
}

fn finish(ok: bool) -> ExitCode {
    println!();
    if ok {
        println!("ok");
        ExitCode::SUCCESS
    } else {
        println!("failed");
        ExitCode::FAILURE
    }
}
