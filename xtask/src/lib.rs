pub mod bundle_zip;
pub mod certs;
pub mod chaos;
pub mod dotenv;
pub mod fmt;
pub mod frame_budget;
pub mod licenses;
pub mod live;
pub mod loc;
pub mod master;
pub mod mrs;
pub mod net_feel;
pub mod perf_overhead;
pub mod perfetto_query;
pub mod provision;
pub mod publish;
pub mod publish_check;
pub mod release;
pub mod scenario;
pub mod shell;
pub mod windows;

use std::path::{Path, PathBuf};

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a parent")
        .to_path_buf()
}

pub fn hr(title: &str) {
    println!("\n== {title} ==");
}
