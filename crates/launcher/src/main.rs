use std::path::PathBuf;

use asset_transport::{ensure_artifacts_dir, games_root_from_env};

#[cfg(windows)]
mod first_run;

#[global_allocator]
static PROCESS_ALLOCATOR: diag::ProcessCountingAllocator = diag::ProcessCountingAllocator;

fn main() {
    let (mode, acceptance) = bootstrap::parse_cli(std::env::args().skip(1))
        .unwrap_or_else(|e| diag::exit_launch_error(&e));
    if mode == bootstrap::LaunchMode::Help {
        println!("{}", bootstrap::args::USAGE);
        return;
    }
    bootstrap::bench::arm();
    prepare_process_root().unwrap_or_else(|e| {
        diag::exit_launch_error(&e);
    });
    #[cfg(windows)]
    first_run::prepare().unwrap_or_else(|e| first_run::fail(&e));
    let artifacts = ensure_artifacts_dir().unwrap_or_else(|e| diag::exit_launch_error(&e));
    announce_log(diag::init_log(&artifacts));
    let games = games_root_from_env().unwrap_or_else(|e| diag::exit_launch_error(&e));
    prepare_skate_assets();
    bootstrap::launch(games, artifacts, mode, acceptance);
}

fn prepare_skate_assets() {
    if std::env::var("IW4L_SKATE").is_ok_and(|value| value == "off") {
        return;
    }
    let Some(assets) = std::env::var_os("IW4L_SKATE_ASSETS") else {
        return;
    };
    if let Err(error) = assets::skate_board::ensure(std::path::Path::new(&assets)) {
        diag::warn!(Launch, "Could not prepare the skateboard: {error}");
    }
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
