use std::path::{Path, PathBuf};
use std::process::Command;

use crate::dotenv::Env;
use crate::shell::{Res, Step, capture, require_tools, run};

pub const TARGET: &str = "x86_64-pc-windows-msvc";
pub const LINUX_TARGET: &str = "x86_64-unknown-linux-gnu";

const WANT_STACK_RESERVE: u64 = 8 * 1024 * 1024;

pub struct WindowsBins {
    pub game: PathBuf,
}

pub fn require_profile(profile: &str) -> Res<()> {
    match profile {
        "play" | "release" | "perf" => Ok(()),
        other => Err(format!(
            "PROFILE must be play, release, or perf (got {other:?})"
        )),
    }
}

pub fn profile(env: &Env) -> Res<String> {
    let profile = env.get("PROFILE").unwrap_or_else(|| "play".to_string());
    require_profile(&profile)?;
    Ok(profile)
}

pub fn public_ca(env: &Env) -> Res<PathBuf> {
    let ca = env
        .get("IW4L_UPDATER_CA_CERT")
        .or_else(|| {
            env.get("IW4L_RELEASE_KEY")
                .map(|dir| format!("{dir}/iw4l-ca.pem"))
        })
        .ok_or("set IW4L_UPDATER_CA_CERT to the public CA pem (or IW4L_RELEASE_KEY)")?;
    let ca = PathBuf::from(ca);
    if !ca.is_file() {
        return Err(format!(
            "missing public CA {}; create it with `cargo xtask certs HOST` — do not mint a new CA to paper over this",
            ca.display()
        ));
    }
    Ok(ca)
}

pub fn setup() -> Res<()> {
    require_tools(&["cargo", "rustup", "llvm-lib"])?;
    run(Command::new("rustup").args(["target", "add", TARGET]))?;
    if !crate::shell::tool_on_path("cargo-xwin") {
        run(Command::new("cargo").args(["install", "cargo-xwin", "--locked"]))?;
    }
    println!("setup-windows: rustup target {TARGET} and cargo-xwin ready");
    Ok(())
}

fn require_windows_tools() -> Res<()> {
    for tool in ["cargo", "rustup", "llvm-lib", "cargo-xwin"] {
        if !crate::shell::tool_on_path(tool) {
            return Err(format!(
                "missing tool: {tool}; run: cargo xtask windows setup"
            ));
        }
    }
    let installed = capture(Command::new("rustup").args(["target", "list", "--installed"]))?;
    if !installed.lines().any(|line| line.trim() == TARGET) {
        return Err(format!(
            "missing rustup target {TARGET}; run: cargo xtask windows setup"
        ));
    }
    Ok(())
}

pub fn build(profile: &str, ca_cert: &Path) -> Res<WindowsBins> {
    require_profile(profile)?;
    require_windows_tools()?;
    if !ca_cert.is_file() {
        return Err(format!("missing public CA {}", ca_cert.display()));
    }
    let cache = std::env::var("XWIN_CACHE_DIR").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        format!("{home}/.cache/cargo-xwin")
    });
    std::fs::create_dir_all(&cache).map_err(|error| format!("creating {cache}: {error}"))?;

    let step = Step::start("build.windows", &format!("profile={profile}"));
    let messages = capture(
        Command::new("cargo")
            .args([
                "xwin",
                "build",
                "--profile",
                profile,
                "--locked",
                "--target",
            ])
            .arg(TARGET)
            .args(["-p", "launcher", "--message-format=json-render-diagnostics"])
            .env("XWIN_CACHE_DIR", &cache),
    )?;
    let bins = WindowsBins {
        game: cargo_json_bin(&messages, "iw4l")?,
    };
    assert_stack_reserve(&bins.game)?;
    step.done(&format!("game={}", bins.game.display()));
    Ok(bins)
}

pub fn cargo_json_bin(messages: &str, bin_name: &str) -> Res<PathBuf> {
    let mut found = None;
    for line in messages.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if message["reason"] != "compiler-artifact" {
            continue;
        }
        let target = &message["target"];
        if target["name"] != bin_name {
            continue;
        }
        let is_bin = target["kind"]
            .as_array()
            .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"));
        if !is_bin {
            continue;
        }
        if let Some(exe) = message["executable"].as_str() {
            found = Some(PathBuf::from(exe));
        }
    }
    let path = found.ok_or_else(|| format!("cargo JSON has no bin executable named {bin_name}"))?;
    if !path.is_file() {
        return Err(format!(
            "cargo named {} but it is not there",
            path.display()
        ));
    }
    Ok(path)
}

fn assert_stack_reserve(exe: &Path) -> Res<()> {
    let data = std::fs::read(exe).map_err(|error| format!("reading {}: {error}", exe.display()))?;
    let pe = usize::try_from(read_u32(&data, 0x3C)?).map_err(|_| "PE offset out of range")?;
    if data.get(pe..pe + 4) != Some(b"PE\0\0") {
        return Err(format!("{}: not a PE image", exe.display()));
    }
    let magic = read_u16(&data, pe + 24)?;
    let at = pe + 24 + 72;
    let reserve = if magic == 0x20b {
        read_u64(&data, at)?
    } else {
        u64::from(read_u32(&data, at)?)
    };
    if reserve < WANT_STACK_RESERVE {
        return Err(format!(
            "{}: main-thread stack reserve {reserve} < {WANT_STACK_RESERVE}; the /STACK link arg from crates/launcher/build.rs is gone",
            exe.display()
        ));
    }
    println!(
        "[windows] {}: main-thread stack reserve {} MiB",
        exe.display(),
        reserve >> 20
    );
    Ok(())
}

fn slice<const N: usize>(data: &[u8], at: usize) -> Res<[u8; N]> {
    data.get(at..at + N)
        .and_then(|bytes| <[u8; N]>::try_from(bytes).ok())
        .ok_or_else(|| format!("PE image truncated at {at:#x}"))
}

fn read_u16(data: &[u8], at: usize) -> Res<u16> {
    Ok(u16::from_le_bytes(slice::<2>(data, at)?))
}

fn read_u32(data: &[u8], at: usize) -> Res<u32> {
    Ok(u32::from_le_bytes(slice::<4>(data, at)?))
}

fn read_u64(data: &[u8], at: usize) -> Res<u64> {
    Ok(u64::from_le_bytes(slice::<8>(data, at)?))
}

pub fn run_cli(env: &Env, args: &[String]) -> Res<()> {
    match args.first().map(String::as_str).unwrap_or("build") {
        "setup" => setup(),
        "build" => {
            let profile = profile(env)?;
            let ca = public_ca(env)?;
            let bins = build(&profile, &ca)?;
            println!("iw4l.exe={}", bins.game.display());
            Ok(())
        }
        other => Err(format!(
            "usage: cargo xtask windows [build|setup] (got {other:?})"
        )),
    }
}
