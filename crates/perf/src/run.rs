//! One run, one directory.
//!
//! The trace, its manifest and the bench report are three views of a single
//! execution, and a reader comparing two runs should not have to pair them up
//! by timestamp. So the id is minted here rather than by whichever recorder
//! happens to start first, and a run has the same name whether `IW4L_PERF`,
//! `IW4L_BENCH` or both are set.
//!
//! Nothing is created until something asks. A process that records neither
//! leaves no directory behind.

use std::path::PathBuf;
use std::sync::OnceLock;

static RUN: OnceLock<Result<(PathBuf, String), String>> = OnceLock::new();

fn slot() -> &'static Result<(PathBuf, String), String> {
    RUN.get_or_init(|| {
        let id = uuid::Uuid::new_v4().to_string();
        let dir = std::env::var_os("IW4L_ARTIFACTS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("iw4l-artifacts"))
            .join("runs")
            .join(&id);
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("create {}: {error}", dir.display()))?;
        Ok((dir, id))
    })
}

/// This run's directory, created on the first ask, or the error that stopped it.
pub fn dir() -> Result<PathBuf, String> {
    match slot() {
        Ok((dir, _)) => Ok(dir.clone()),
        Err(error) => Err(error.clone()),
    }
}

/// This run's UUID — the name of [`dir`] — or `None` if it could not be made.
pub fn id() -> Option<String> {
    slot().as_ref().ok().map(|(_, id)| id.clone())
}
