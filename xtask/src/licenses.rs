use std::path::Path;
use std::process::Command;

use crate::shell::{Res, capture, require_tools};

const OUTPUT: &str = "THIRD-PARTY-LICENSES.txt";

fn generate(root: &Path) -> Res<String> {
    require_tools(&["cargo-about"])
        .map_err(|error| format!("{error} (cargo install cargo-about --locked --features cli)"))?;
    capture(
        Command::new("cargo")
            .current_dir(root)
            .args(["about", "generate", "--locked", "--fail"])
            .args(["-m", "crates/launcher/Cargo.toml"])
            .args(["-c", "xtask/about/about.toml"])
            .arg("xtask/about/licenses.hbs"),
    )
}

/// `iw4l licenses` prints the committed file, so a release built from a stale
/// one would ship the wrong crate list.
pub fn check(root: &Path) -> Res<()> {
    let committed = std::fs::read_to_string(root.join(OUTPUT))
        .map_err(|error| format!("reading {OUTPUT}: {error}"))?;
    if generate(root)? != committed {
        return Err(format!(
            "{OUTPUT} does not match Cargo.lock; run cargo xtask licenses"
        ));
    }
    Ok(())
}

pub fn run_cli(root: &Path) -> Res<()> {
    std::fs::write(root.join(OUTPUT), generate(root)?)
        .map_err(|error| format!("writing {OUTPUT}: {error}"))
}
