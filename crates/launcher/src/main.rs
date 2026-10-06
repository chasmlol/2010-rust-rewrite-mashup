use std::path::PathBuf;

use asset_transport::{ensure_artifacts_dir, games_root_from_env};

#[cfg(windows)]
mod first_run;

#[global_allocator]
static PROCESS_ALLOCATOR: diag::ProcessCountingAllocator = diag::ProcessCountingAllocator;

const LICENSES: [(&str, &str); 5] = [
    ("LICENSE", include_str!("../../../LICENSE")),
    ("NOTICE", include_str!("../../../NOTICE")),
    (
        "OFL-Oxanium.txt",
        include_str!("../../ui/assets/OFL-Oxanium.txt"),
    ),
    (
        "OFL-FiraMono.txt",
        include_str!("../../console/assets/OFL-FiraMono.txt"),
    ),
    (
        "THIRD-PARTY-LICENSES.txt",
        include_str!("../../../THIRD-PARTY-LICENSES.txt"),
    ),
];

fn main() {
    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "licenses")
    {
        for (name, text) in LICENSES {
            println!("==> {name} <==\n\n{text}\n");
        }
        return;
    }
    let mut args = match updater::startup().unwrap_or_else(|e| diag::exit_launch_error(&e)) {
        Some(args) => args,
        None => return,
    };
    if args.is_empty() {
        args.push("menu".into());
    }
    bootstrap::bench::arm();
    prepare_process_root().unwrap_or_else(|e| {
        diag::exit_launch_error(&e);
    });
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(windows)]
    {
        // Also for a shortcut that names a map, so it works on first launch.
        first_run::prepare().unwrap_or_else(|e| first_run::fail(&e));
        if args.is_empty() {
            args.push("menu".into());
        }
    }
    let artifacts = ensure_artifacts_dir().unwrap_or_else(|e| diag::exit_launch_error(&e));
    announce_log(diag::init_log(&artifacts));
    let (mode, acceptance, cheats) =
        bootstrap::parse_cli(args.into_iter()).unwrap_or_else(|e| diag::exit_launch_error(&e));
    let games = games_root_from_env().unwrap_or_else(|e| diag::exit_launch_error(&e));
    bootstrap::launch(games, artifacts, mode, acceptance, cheats);
}

fn prepare_process_root() -> Result<(), String> {
    #[cfg(windows)]
    {
        let exe =
            std::env::current_exe().map_err(|error| format!("cannot locate iw4l.exe: {error}"))?;
        let root = exe
            .parent()
            .ok_or_else(|| format!("iw4l.exe has no parent directory: {}", exe.display()))?;
        std::env::set_current_dir(root).map_err(|error| {
            format!(
                "cannot enter launcher directory {}: {error}",
                root.display()
            )
        })?;
    }
    Ok(())
}

fn announce_log(path: PathBuf) {
    diag::announce_log_stdout(&path, diag::latest_log_path().as_deref());
    diag::info!(Launch, "log: {}", path.display());
}
