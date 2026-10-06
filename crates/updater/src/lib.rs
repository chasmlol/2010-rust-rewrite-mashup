use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub type Result<T> = std::result::Result<T, String>;
const MAX_BINARY: u64 = 512 * 1024 * 1024;
const MAX_MANIFEST: u64 = 64 * 1024;
static SELECTED: OnceLock<Community> = OnceLock::new();
static COMMUNITIES: OnceLock<CommunityFiles> = OnceLock::new();
const SELECTION_FILE: &str = "iw4l.community.json";

#[derive(Clone, Debug)]
pub struct CommunityFile {
    pub path: PathBuf,
    pub name: String,
}

struct CommunityFiles {
    entries: Vec<CommunityFile>,
    selected: Option<PathBuf>,
}

pub fn communities() -> &'static [CommunityFile] {
    COMMUNITIES.get().map_or(&[], |files| &files.entries)
}

pub fn selected_path() -> Option<&'static Path> {
    COMMUNITIES.get()?.selected.as_deref()
}

fn read_community(path: &Path) -> Result<Community> {
    let bytes = read_bounded(
        File::open(path).map_err(|e| format!("{}: {e}", path.display()))?,
        MAX_MANIFEST,
    )?;
    let community: Community =
        toml::from_str(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)
            .map_err(|e| format!("{}: {e}", path.display()))?;
    if community.schema != 1
        || community.name.trim().is_empty()
        || community.master.server_name.trim().is_empty()
        || community.master.address.trim().is_empty()
    {
        return Err("unsupported or incomplete community descriptor".into());
    }
    trust_roots(&community.updates.ca_pem)?;
    update_url(&community)?;
    Ok(community)
}

fn discover_communities(root: &Path, explicit: Option<PathBuf>) -> Result<CommunityFiles> {
    let mut paths: Vec<_> = fs::read_dir(root)
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map(|entry| entry.path()).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "iw4l-server"))
        .collect();
    let saved = match File::open(root.join(SELECTION_FILE)) {
        Ok(file) => match read_bounded(file, MAX_MANIFEST)
            .and_then(|bytes| serde_json::from_slice::<PathBuf>(&bytes).map_err(|e| e.to_string()))
        {
            Ok(path) => Some(root.join(path)),
            Err(error) => {
                println!("[iw4l] cannot read community selection: {error}");
                None
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            println!("[iw4l] cannot read community selection: {error}");
            None
        }
    };
    let explicit = explicit.map(|path| root.join(path));
    if let Some(path) = &explicit {
        read_community(path)?;
        if !paths.contains(path) {
            paths.push(path.clone());
        }
    }
    if let Some(path) = &saved
        && !paths.contains(path)
        && path.is_file()
    {
        paths.push(path.clone());
    }
    let mut entries = Vec::new();
    for path in paths {
        match read_community(&path) {
            Ok(community) => entries.push(CommunityFile {
                path,
                name: community.name,
            }),
            Err(error) => println!("[iw4l] skipping {}: {error}", path.display()),
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));
    let selected = explicit
        .or_else(|| saved.filter(|path| entries.iter().any(|entry| entry.path == *path)))
        .or_else(|| (entries.len() == 1).then(|| entries[0].path.clone()));
    Ok(CommunityFiles { entries, selected })
}

pub fn restart_with_community(path: &Path) -> Result<()> {
    if !communities().iter().any(|entry| entry.path == path) {
        return Err("community is not in the installed server list".into());
    }
    read_community(path)?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let root = exe.parent().ok_or("executable has no directory")?;
    let selection = root.join(SELECTION_FILE);
    let previous = match fs::read(&selection) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("read community selection: {error}")),
    };
    let payload =
        serde_json::to_vec(path.strip_prefix(root).unwrap_or(path)).map_err(|e| e.to_string())?;
    save_selection(root, &payload)?;
    if let Err(error) = Command::new(&exe)
        .arg("menu")
        .env("IW4L_COMMUNITY", path)
        .env_remove("IW4L_MASTER_HOST_NAME")
        .env_remove("IW4L_MASTER_JOIN")
        .env_remove("IW4L_CMDS")
        .stdin(std::process::Stdio::null())
        .spawn()
    {
        let rollback = match previous {
            Some(bytes) => save_selection(root, &bytes),
            None => fs::remove_file(selection).map_err(|e| e.to_string()),
        };
        rollback.map_err(|rollback| format!("restart: {error}; restore selection: {rollback}"))?;
        return Err(format!("restart community: {error}"));
    }
    Ok(())
}

fn save_selection(root: &Path, payload: &[u8]) -> Result<()> {
    let temp = root.join(format!("iw4l.community.{}.tmp", std::process::id()));
    let result = write_synced(&temp, payload)
        .and_then(|()| fs::rename(&temp, root.join(SELECTION_FILE)).map_err(|e| e.to_string()));
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result.map_err(|e| format!("save community selection: {e}"))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Community {
    pub schema: u32,
    pub name: String,
    pub master: Master,
    pub updates: Updates,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Master {
    pub address: String,
    pub server_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Updates {
    pub url: String,
    pub ca_pem: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub release: String,
    pub protocol: u16,
    pub file: ManifestFile,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestFile {
    pub name: String,
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub compressed_size: u64,
}

pub fn selected() -> Option<&'static Community> {
    SELECTED.get()
}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() as u64 > MAX_MANIFEST {
            return Err("update manifest exceeds limit".into());
        }
        let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
        let manifest: Self = toml::from_str(text).map_err(|e| format!("update manifest: {e}"))?;
        let file = &manifest.file;
        if manifest.schema != 1 || manifest.release.trim().is_empty() || manifest.protocol == 0 {
            return Err("unsupported or incomplete update manifest".into());
        }
        if file.name != "iw4l.exe"
            || file.sha256.len() != 64
            || !file.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || file.path != format!("iw4l-{}.exe.zst", file.sha256.to_ascii_lowercase())
            || file.size == 0
            || file.size > MAX_BINARY
            || file.compressed_size == 0
            || file.compressed_size > MAX_BINARY
        {
            return Err("invalid update file identity or size".into());
        }
        Ok(manifest)
    }
}

pub fn startup() -> Result<Option<Vec<OsString>>> {
    let mut args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--iw4l-replace") {
        if args.len() != 2 {
            return Err("replacement helper requires one job path".into());
        }
        replace(Path::new(&args[1]))?;
        return Ok(None);
    }
    let restarted = args.first().is_some_and(|a| a == "--iw4l-updated");
    if restarted {
        args.remove(0);
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let root = exe.parent().ok_or("executable has no directory")?;
    let files = discover_communities(root, std::env::var_os("IW4L_COMMUNITY").map(PathBuf::from))?;
    let descriptor = files.selected.clone();
    COMMUNITIES
        .set(files)
        .map_err(|_| "communities already loaded")?;
    let Some(path) = descriptor else {
        return Ok(Some(args));
    };
    let community = read_community(&path)?;
    if !cfg!(windows) {
        SELECTED
            .set(community)
            .map_err(|_| "community already selected")?;
        return Ok(Some(args));
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("iw4l.update.lock"))
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if lock.try_lock_exclusive().is_ok() {
            break;
        }
        if !restarted || Instant::now() >= deadline {
            return Err("another iw4l process is checking or applying an update".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    cleanup_helper(root)?;
    let (client, url) = client(&community)?;
    let manifest = Manifest::parse(&get(&client, url.clone(), MAX_MANIFEST)?)?;
    if fs::metadata(&exe).map_err(|e| e.to_string())?.len() == manifest.file.size
        && file_sha256(&exe)? == manifest.file.sha256.to_ascii_lowercase()
    {
        let _ = fs::remove_file(root.join("iw4l.previous.exe"));
        SELECTED
            .set(community)
            .map_err(|_| "community already selected")?;
        return Ok(Some(args));
    }
    println!("[iw4l] downloading release {}", manifest.release);
    let blob_url = url.join(&manifest.file.path).map_err(|e| e.to_string())?;
    let blob = get(&client, blob_url, manifest.file.compressed_size)?;
    if blob.len() as u64 != manifest.file.compressed_size {
        return Err("compressed update size mismatch".into());
    }
    let mut decoder =
        zstd::stream::read::Decoder::new(blob.as_slice()).map_err(|e| e.to_string())?;
    decoder.window_log_max(29).map_err(|e| e.to_string())?;
    let decoded = read_bounded(decoder, manifest.file.size)?;
    if decoded.len() as u64 != manifest.file.size
        || hash(&decoded) != manifest.file.sha256.to_ascii_lowercase()
    {
        return Err("update executable size or SHA-256 mismatch".into());
    }
    let staged = root.join("iw4l.update.exe");
    write_synced(&staged, &decoded)?;
    fs::set_permissions(
        &staged,
        fs::metadata(&exe).map_err(|e| e.to_string())?.permissions(),
    )
    .map_err(|e| e.to_string())?;
    let temp = std::env::temp_dir().join(format!(
        "iw4l-update-{}-{}",
        std::process::id(),
        &manifest.file.sha256[..16]
    ));
    fs::create_dir(&temp).map_err(|e| format!("create helper directory: {e}"))?;
    let helper = temp.join("iw4l-update-helper.exe");
    fs::copy(&exe, &helper).map_err(|e| e.to_string())?;
    let job = Job {
        pid: std::process::id(),
        target: exe.clone(),
        source: staged,
        sha256: manifest.file.sha256,
        size: manifest.file.size,
        args: args
            .iter()
            .map(|a| {
                a.to_str()
                    .map(str::to_owned)
                    .ok_or("non-UTF8 launch argument")
            })
            .collect::<std::result::Result<_, _>>()?,
        cwd: std::env::current_dir().map_err(|e| e.to_string())?,
    };
    let job_path = temp.join("job.json");
    write_synced(
        &job_path,
        &serde_json::to_vec(&job).map_err(|e| e.to_string())?,
    )?;
    write_synced(
        &root.join("iw4l.update-helper"),
        temp.to_str().ok_or("non-UTF8 helper directory")?.as_bytes(),
    )?;
    let mut child = Command::new(helper)
        .arg("--iw4l-replace")
        .arg(&job_path)
        .stdin(std::process::Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !temp.join("ready").is_file() {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("update helper exited before handoff".into());
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            return Err("update helper handoff timeout".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(None)
}

fn cleanup_helper(root: &Path) -> Result<()> {
    let marker = root.join("iw4l.update-helper");
    if !marker.exists() {
        return Ok(());
    }
    let bytes = read_bounded(
        File::open(&marker).map_err(|e| e.to_string())?,
        MAX_MANIFEST,
    )?;
    let temp = PathBuf::from(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?);
    if temp.parent() != Some(std::env::temp_dir().as_path())
        || !temp
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("iw4l-update-"))
    {
        return Err("invalid update helper cleanup directory".into());
    }
    for _ in 0..30 {
        if fs::remove_dir_all(&temp).is_ok() || !temp.exists() {
            fs::remove_file(&marker).map_err(|e| e.to_string())?;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

fn update_url(community: &Community) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(&community.updates.url).map_err(|e| e.to_string())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.path().ends_with("/manifest.toml")
    {
        return Err(
            "update URL must be an HTTPS manifest.toml without credentials, query or fragment"
                .into(),
        );
    }
    Ok(url)
}

fn client(community: &Community) -> Result<(reqwest::blocking::Client, reqwest::Url)> {
    let mut url = update_url(community)?;
    let port = url
        .port_or_known_default()
        .ok_or("update URL has no port")?;
    let addresses: Vec<_> = (url.host_str().ok_or("missing update host")?, port)
        .to_socket_addrs()
        .map_err(|e| format!("update address: {e}"))?
        .collect();
    if addresses.is_empty() {
        return Err("update host resolved no addresses".into());
    }
    let roots = trust_roots(&community.updates.ca_pem)?;
    let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| e.to_string())?
    .with_root_certificates(roots)
    .with_no_client_auth();
    url.set_host(Some(&community.master.server_name))
        .map_err(|e| e.to_string())?;
    let client = reqwest::blocking::Client::builder()
        .use_preconfigured_tls(tls)
        .https_only(true)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(&community.master.server_name, &addresses)
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(180))
        .user_agent("iw4l-updater/1")
        .build()
        .map_err(|e| e.to_string())?;
    Ok((client, url))
}

pub fn trust_roots(pem: &str) -> Result<rustls::RootCertStore> {
    let certs = rustls_pemfile::certs(&mut std::io::Cursor::new(pem.as_bytes()))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    if certs.is_empty() {
        return Err("community CA contains no certificates".into());
    }
    let mut roots = rustls::RootCertStore::empty();
    for cert in certs {
        roots.add(cert).map_err(|e| e.to_string())?;
    }
    Ok(roots)
}

fn get(client: &reqwest::blocking::Client, url: reqwest::Url, cap: u64) -> Result<Vec<u8>> {
    let response = client
        .get(url)
        .header(reqwest::header::ACCEPT_ENCODING, "identity")
        .send()
        .map_err(|e| e.to_string())?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(format!("update HTTP status {}", response.status()));
    }
    if response.content_length().is_some_and(|size| size > cap) {
        return Err("update response exceeds limit".into());
    }
    read_bounded(response, cap)
}

fn read_bounded(reader: impl Read, cap: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(cap + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > cap {
        return Err("update input exceeds limit".into());
    }
    Ok(bytes)
}

pub fn file_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    std::io::copy(&mut file, &mut digest).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", digest.finalize()))
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn write_synced(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = File::create(path).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())
}

#[derive(Serialize, Deserialize)]
struct Job {
    pid: u32,
    target: PathBuf,
    source: PathBuf,
    sha256: String,
    size: u64,
    args: Vec<String>,
    cwd: PathBuf,
}

fn replace(path: &Path) -> Result<()> {
    let job: Job = serde_json::from_slice(&read_bounded(
        File::open(path).map_err(|e| e.to_string())?,
        MAX_MANIFEST,
    )?)
    .map_err(|e| e.to_string())?;
    let temp = path.parent().ok_or("helper job has no directory")?;
    let root = job
        .target
        .parent()
        .ok_or("update target has no directory")?;
    if job.source != root.join("iw4l.update.exe") || job.source == job.target {
        return Err("invalid replacement source".into());
    }
    wait_parent(job.pid, &temp.join("ready"))?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("iw4l.update.lock"))
        .map_err(|e| e.to_string())?;
    lock.try_lock_exclusive()
        .map_err(|_| "another process interrupted replacement")?;
    if fs::metadata(&job.source).map_err(|e| e.to_string())?.len() != job.size
        || file_sha256(&job.source)? != job.sha256.to_ascii_lowercase()
    {
        return Err("staged replacement verification failed".into());
    }
    let backup = root.join("iw4l.previous.exe");
    if backup.exists() {
        fs::remove_file(&backup).map_err(|e| e.to_string())?;
    }
    fs::rename(&job.target, &backup).map_err(|e| format!("backup executable: {e}"))?;
    if let Err(e) = fs::rename(&job.source, &job.target) {
        fs::rename(&backup, &job.target)
            .map_err(|rollback| format!("install: {e}; rollback: {rollback}"))?;
        return Err(format!("install update: {e}"));
    }
    // Keep the lock through spawning and rollback; the restarted process waits for this handoff.
    if let Err(e) = Command::new(&job.target)
        .arg("--iw4l-updated")
        .args(&job.args)
        .current_dir(&job.cwd)
        .spawn()
    {
        fs::remove_file(&job.target)
            .map_err(|rollback| format!("restart: {e}; remove failed update: {rollback}"))?;
        fs::rename(&backup, &job.target)
            .map_err(|rollback| format!("restart: {e}; restore: {rollback}"))?;
        return Err(format!("restart update: {e}; previous executable restored"));
    }
    Ok(())
}

#[cfg(windows)]
fn wait_parent(pid: u32, ready: &Path) -> Result<()> {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };
    // Open the handle before acknowledging handoff so PID reuse cannot change the process we wait for.
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return Err(format!(
            "open update parent: {}",
            std::io::Error::last_os_error()
        ));
    }
    let result = (|| {
        write_synced(ready, b"ready")?;
        if unsafe { WaitForSingleObject(handle, 120_000) } != WAIT_OBJECT_0 {
            return Err("update parent wait failed or timed out".into());
        }
        Ok(())
    })();
    unsafe {
        CloseHandle(handle);
    }
    result
}

#[cfg(not(windows))]
fn wait_parent(pid: u32, ready: &Path) -> Result<()> {
    write_synced(ready, b"ready")?;
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let running = fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .is_some_and(|stat| {
                !stat
                    .rsplit_once(')')
                    .is_some_and(|(_, rest)| rest.trim_start().starts_with('Z'))
            });
        if !running {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("update parent wait timeout".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
