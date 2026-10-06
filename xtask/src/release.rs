use std::path::{Path, PathBuf};
use std::process::Command;

use master_protocol::{Channel, PROTOCOL_VERSION};
use serde_json::{Value, json};

use crate::dotenv::Env;
use crate::shell::{Res, Step, capture, require_tools, run};
use crate::windows;

const ZSTD_LEVEL: &str = "3";
const ZSTD_THREADS: &str = "0";

const ARCHIVE_PASSWORD: &[u8] = b"t.me/contextrot";

pub struct Git {
    pub rev: String,
    pub dirty: bool,
}

pub fn git_identity(root: &Path) -> Res<Git> {
    let rev =
        capture(
            Command::new("git")
                .current_dir(root)
                .args(["rev-parse", "--short=12", "HEAD"]),
        )?
        .trim()
        .to_string();
    let status = capture(
        Command::new("git")
            .current_dir(root)
            .args(["status", "--porcelain"]),
    )?;
    Ok(Git {
        rev,
        dirty: !status.trim().is_empty(),
    })
}

pub fn public_host(env: &Env) -> Res<String> {
    let target = env.require("IW4L_DEPLOY_HOST")?;
    let host = crate::shell::Ssh::new(&target)?.host().to_string();
    Ok(host)
}

pub fn file_sha256(path: &Path) -> Res<String> {
    use sha2::{Digest as _, Sha256};
    let mut file =
        std::fs::File::open(path).map_err(|error| format!("open {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    Ok(hex(&hasher.finalize()))
}

pub fn file_size(path: &Path) -> Res<u64> {
    std::fs::metadata(path)
        .map(|meta| meta.len())
        .map_err(|error| format!("stat {}: {error}", path.display()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    hex(&Sha256::digest(bytes))
}

fn write_json(path: &Path, value: &Value) -> Res<()> {
    let mut text = serde_json::to_string_pretty(value)
        .map_err(|error| format!("encoding {}: {error}", path.display()))?;
    text.push('\n');
    std::fs::write(path, text).map_err(|error| format!("writing {}: {error}", path.display()))
}

fn zstd_version() -> Res<String> {
    let text = capture(Command::new("zstd").arg("--version"))?;
    let version = text
        .split('v')
        .find_map(|chunk| {
            let digits: String = chunk
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            (digits.contains('.') && digits.starts_with(|c: char| c.is_ascii_digit()))
                .then_some(digits)
        })
        .ok_or("cannot parse zstd --version")?;
    Ok(version.trim_end_matches('.').to_string())
}

struct Scratch {
    path: PathBuf,
    keep: bool,
}

impl Scratch {
    fn new(under: &Path, tag: &str) -> Res<Self> {
        let path = under.join(format!(".stage-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)
            .map_err(|error| format!("creating {}: {error}", path.display()))?;
        Ok(Self { path, keep: false })
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

pub fn build_master(root: &Path, profile: &str) -> Res<PathBuf> {
    windows::require_profile(profile)?;
    require_tools(&["cargo"])?;
    let step = Step::start("build.master", &format!("profile={profile} crt-static"));
    let rustflags = format!(
        "{} -C target-feature=+crt-static",
        std::env::var("RUSTFLAGS").unwrap_or_else(|_| String::new())
    );
    let messages = capture(
        Command::new("cargo")
            .current_dir(root)
            .args(["build", "--profile", profile, "--locked", "--target"])
            .arg(windows::LINUX_TARGET)
            .args([
                "-p",
                "iw4l-master",
                "--message-format=json-render-diagnostics",
            ])
            .env("RUSTFLAGS", rustflags.trim()),
    )?;
    let bin = windows::cargo_json_bin(&messages, "iw4l-master")?;
    step.done(&format!("bin={}", bin.display()));
    Ok(bin)
}

struct GameBlob {
    name: String,
    exe_sha: String,
    exe_size: u64,
    blob_sha: String,
    blob_size: u64,
}

fn package_game(root: &Path, game_exe: &Path, dest_dir: &Path) -> Res<GameBlob> {
    require_tools(&["zstd"])?;
    let exe_sha = file_sha256(game_exe)?;
    let zstd_ver = zstd_version()?;
    let cache_key = format!("{exe_sha}.zstd-{zstd_ver}.{ZSTD_LEVEL}.T{ZSTD_THREADS}");
    let cache_dir = root.join("dist/cache/zstd");
    std::fs::create_dir_all(&cache_dir)
        .map_err(|error| format!("creating {}: {error}", cache_dir.display()))?;
    let cache_blob = cache_dir.join(format!("{cache_key}.zst"));
    if cache_blob.is_file() {
        println!(
            "package.game: reused sha256={exe_sha} zstd={zstd_ver} -{ZSTD_LEVEL} -T{ZSTD_THREADS}"
        );
    } else {
        let step = Step::start(
            "package.game",
            &format!("zstd -{ZSTD_LEVEL} -T{ZSTD_THREADS} sha256={exe_sha}"),
        );
        let part = cache_blob.with_extension("zst.part");
        run(Command::new("zstd")
            .arg(format!("-{ZSTD_LEVEL}"))
            .arg(format!("-T{ZSTD_THREADS}"))
            .arg("-o")
            .arg(&part)
            .arg(game_exe))?;
        std::fs::rename(&part, &cache_blob)
            .map_err(|error| format!("renaming {}: {error}", part.display()))?;
        step.done("");
    }
    let blob_sha = file_sha256(&cache_blob)?;
    let name = format!("iw4l-{exe_sha}.exe.zst");
    let dest = dest_dir.join(&name);
    if dest.is_file() {
        if file_sha256(&dest)? != blob_sha {
            return Err(format!(
                "existing blob {} does not match cache",
                dest.display()
            ));
        }
    } else {
        std::fs::copy(&cache_blob, &dest)
            .map_err(|error| format!("copying to {}: {error}", dest.display()))?;
    }
    Ok(GameBlob {
        exe_sha,
        exe_size: file_size(game_exe)?,
        blob_sha,
        blob_size: file_size(&dest)?,
        name,
    })
}

fn write_toml(path: &Path, value: &impl serde::Serialize) -> Res<()> {
    let text = toml::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

fn community(channel: Channel, host: &str, ca_path: &Path) -> Res<updater::Community> {
    Ok(updater::Community {
        schema: 1,
        name: format!("IW4L {channel}"),
        master: updater::Master {
            address: format!("{host}:{}", channel.port()),
            server_name: channel.server_name().into(),
        },
        updates: updater::Updates {
            url: format!("https://{host}:{}/updates/manifest.toml", channel.port()),
            ca_pem: std::fs::read_to_string(ca_path).map_err(|e| e.to_string())?,
        },
    })
}

fn entry(role: &str, local: &str, remote: String, stage: &Path, immutable: bool) -> Res<Value> {
    let path = stage.join(local);
    Ok(json!({
        "role": role,
        "local": local,
        "remote": remote,
        "sha256": file_sha256(&path)?,
        "size": file_size(&path)?,
        "immutable": immutable,
    }))
}

struct Meta<'a> {
    release_id: &'a str,
    channel: Channel,
    profile: &'a str,
    git: &'a Git,
    update_url: &'a str,
    master_sha: &'a str,
}

fn release_descriptor(stage: &Path, meta: &Meta<'_>, blob: &GameBlob) -> Res<Value> {
    let Meta {
        release_id,
        channel,
        profile,
        git,
        update_url,
        master_sha,
    } = *meta;
    let mut manifest = entry(
        "manifest",
        "server/updates/manifest.toml",
        format!("manifests/{release_id}.toml"),
        stage,
        false,
    )?;
    manifest["live"] = json!("manifest.toml");
    Ok(json!({
        "id": release_id,
        "channel": channel.as_str(),
        "profile": profile,
        "target": windows::TARGET,
        "git": git.rev,
        "dirty": git.dirty,
        "protocol": PROTOCOL_VERSION,
        "master_port": channel.port(),
        "update_url": update_url,
        "files": [
            entry(
                "game-blob",
                &format!("server/updates/{}", blob.name),
                blob.name.clone(),
                stage,
                true,
            )?,
            manifest,
            entry(
                "master",
                "server/iw4l-master",
                format!("masters/{master_sha}/iw4l-master"),
                stage,
                true,
            )?,
        ],
    }))
}

pub fn prepare(root: &Path, env: &Env, channel: Channel, profile: &str) -> Res<PathBuf> {
    windows::require_profile(profile)?;
    let host = public_host(env)?;
    let git = git_identity(root)?;
    let ca_cert = windows::public_ca(env)?;
    let update_url = format!("https://{host}:{}/updates/manifest.toml", channel.port());

    let bins = windows::build(profile, &ca_cert)?;
    let master_bin = build_master(root, profile)?;

    let releases = root.join("dist/releases").join(channel.as_str());
    std::fs::create_dir_all(&releases)
        .map_err(|error| format!("creating {}: {error}", releases.display()))?;
    let mut scratch = Scratch::new(&releases, channel.as_str())?;
    let stage = scratch.path.clone();
    let client_dir = stage.join("client");
    std::fs::create_dir_all(&client_dir)
        .map_err(|error| format!("creating {}: {error}", client_dir.display()))?;
    let updates_dir = stage.join("server/updates");
    std::fs::create_dir_all(&updates_dir)
        .map_err(|error| format!("creating {}: {error}", updates_dir.display()))?;

    let blob = package_game(root, &bins.game, &updates_dir)?;
    copy(&bins.game, &client_dir.join("iw4l.exe"))?;
    write_toml(
        &client_dir.join("community.iw4l-server"),
        &community(channel, &host, &ca_cert)?,
    )?;
    for (from, to) in LEGAL_FILES {
        copy(&root.join(from), &stage.join("server").join(to))?;
    }
    copy(&ca_cert, &client_dir.join("iw4l-ca.pem"))?;
    let staged_master = stage.join("server/iw4l-master");
    copy(&master_bin, &staged_master)?;
    set_mode(&staged_master, 0o755)?;
    let master_sha = file_sha256(&staged_master)?;

    let licenses = LEGAL_FILES
        .iter()
        .map(|(from, name)| Ok(json!({ "name": name, "sha256": file_sha256(&root.join(from))? })))
        .collect::<Res<Vec<_>>>()?;
    let identity = json!({
        "licenses": licenses,
        "channel": channel.as_str(),
        "dirty": git.dirty,
        "git": git.rev,
        "master_sha256": master_sha,
        "profile": profile,
        "protocol": PROTOCOL_VERSION,
        "target": windows::TARGET,
        "files": {
            "ca": file_sha256(&client_dir.join("iw4l-ca.pem"))?,
            "community": file_sha256(&client_dir.join("community.iw4l-server"))?,
            "game_blob": blob.blob_sha,
            "game_exe": blob.exe_sha,
        },
    });
    let compact =
        serde_json::to_string(&identity).map_err(|error| format!("encoding identity: {error}"))?;
    let release_id = sha256_hex(compact.as_bytes())[..16].to_string();

    let client_identity = json!({ "exe": blob.exe_sha, "size": blob.exe_size,
        "compressed_size": blob.blob_size, "protocol": PROTOCOL_VERSION });
    let manifest = updater::Manifest {
        schema: 1,
        release: sha256_hex(client_identity.to_string().as_bytes())[..16].into(),
        protocol: PROTOCOL_VERSION,
        file: updater::ManifestFile {
            name: "iw4l.exe".into(),
            path: blob.name.clone(),
            sha256: blob.exe_sha.clone(),
            size: blob.exe_size,
            compressed_size: blob.blob_size,
        },
    };
    write_toml(&updates_dir.join("manifest.toml"), &manifest)?;

    let dest = releases.join(&release_id);
    if dest.join("deployment.json").is_file() {
        println!("release: reused {}", dest.display());
    } else {
        let descriptor = release_descriptor(
            &stage,
            &Meta {
                release_id: &release_id,
                channel,
                profile,
                git: &git,
                update_url: &update_url,
                master_sha: &master_sha,
            },
            &blob,
        )?;
        write_json(&stage.join("deployment.json"), &descriptor)?;
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::rename(&stage, &dest)
            .map_err(|error| format!("renaming into {}: {error}", dest.display()))?;
        scratch.keep = true;
        world_readable(&dest)?;
        println!("release: wrote {}", dest.display());
    }
    std::fs::write(releases.join("LATEST"), format!("{release_id}\n"))
        .map_err(|error| format!("writing LATEST: {error}"))?;
    server_archive(
        &dest.join("server"),
        &dest.join(format!("iw4l-server-release-{release_id}.zip")),
    )?;
    player_archive(
        &dest.join("client"),
        &dest.join(format!("iw4l-windows-{channel}.zip")),
        true,
    )?;
    // The GitHub download: the descriptor names the master, which stays private.
    player_archive(&dest.join("client"), &dest.join("iw4l-windows.zip"), false)?;
    println!("release.done path=dist/releases/{channel}/{release_id}");
    Ok(dest)
}

const LEGAL_FILES: &[(&str, &str)] = &[
    ("LICENSE", "LICENSE"),
    ("NOTICE", "NOTICE"),
    ("crates/ui/assets/OFL-Oxanium.txt", "OFL-Oxanium.txt"),
    ("crates/console/assets/OFL-FiraMono.txt", "OFL-FiraMono.txt"),
];

pub fn bundles(root: &Path, env: &Env, profile: &str) -> Res<()> {
    windows::require_profile(profile)?;
    let host = public_host(env)?;
    let ca_cert = windows::public_ca(env)?;
    let bins = windows::build(profile, &ca_cert)?;

    let out = root.join("dist/windows");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out)
        .map_err(|error| format!("creating {}: {error}", out.display()))?;
    let scratch = Scratch::new(&out, "bundles")?;

    for channel in Channel::ALL {
        let stage = scratch.path.join(channel.as_str());
        std::fs::create_dir_all(&stage)
            .map_err(|error| format!("creating {}: {error}", stage.display()))?;
        copy(&bins.game, &stage.join("iw4l.exe"))?;
        write_toml(
            &stage.join("community.iw4l-server"),
            &community(channel, &host, &ca_cert)?,
        )?;
        let archive = out.join(format!("iw4l-windows-{channel}.zip"));
        player_archive(&stage, &archive, true)?;
        println!("[windows] {channel} archive: {}", archive.display());
    }
    Ok(())
}

fn player_archive(stage: &Path, archive: &Path, descriptor: bool) -> Res<()> {
    let mut names = vec!["iw4l.exe"];
    if descriptor {
        names.push("community.iw4l-server");
    }
    let files = names
        .into_iter()
        .map(|n| stage.join(n).display().to_string())
        .collect::<Vec<_>>();
    crate::bundle_zip::write_archive(archive, &files, ARCHIVE_PASSWORD)
}

fn server_archive(stage: &Path, archive: &Path) -> Res<()> {
    use zip::write::SimpleFileOptions;
    let mut writer =
        zip::ZipWriter::new(std::fs::File::create(archive).map_err(|e| e.to_string())?);
    let mut files = vec![stage.join("iw4l-master")];
    files.extend(LEGAL_FILES.iter().map(|(_, name)| stage.join(name)));
    for entry in std::fs::read_dir(stage.join("updates")).map_err(|e| e.to_string())? {
        files.push(entry.map_err(|e| e.to_string())?.path());
    }
    files.sort();
    for path in files {
        let name = path
            .strip_prefix(stage)
            .map_err(|e| e.to_string())?
            .to_string_lossy();
        let mode = if name == "iw4l-master" { 0o755 } else { 0o644 };
        writer
            .start_file(name, SimpleFileOptions::default().unix_permissions(mode))
            .map_err(|e| e.to_string())?;
        std::io::copy(
            &mut std::fs::File::open(path).map_err(|e| e.to_string())?,
            &mut writer,
        )
        .map_err(|e| e.to_string())?;
    }
    writer.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn copy(from: &Path, to: &Path) -> Res<()> {
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|error| format!("copy {} -> {}: {error}", from.display(), to.display()))
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Res<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|error| format!("chmod {mode:o} {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Res<()> {
    Ok(())
}

#[cfg(unix)]
fn world_readable(path: &Path) -> Res<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let meta = std::fs::symlink_metadata(path)
        .map_err(|error| format!("stat {}: {error}", path.display()))?;
    if meta.file_type().is_symlink() {
        return Ok(());
    }
    let mut mode = meta.permissions().mode();
    mode |= 0o444;
    if meta.is_dir() || mode & 0o100 != 0 {
        mode |= 0o111;
    }
    set_mode(path, mode)?;
    if meta.is_dir() {
        let entries = std::fs::read_dir(path)
            .map_err(|error| format!("reading {}: {error}", path.display()))?;
        for child in entries {
            let child = child.map_err(|error| format!("reading {}: {error}", path.display()))?;
            world_readable(&child.path())?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn world_readable(_path: &Path) -> Res<()> {
    Ok(())
}

pub fn run_cli(root: &Path, env: &Env, args: &[String]) -> Res<()> {
    let what = args
        .first()
        .map(String::as_str)
        .ok_or("usage: cargo xtask release <prod|dev|bundles>")?;
    let profile = windows::profile(env)?;
    crate::licenses::check(root)?;
    if what == "bundles" {
        return bundles(root, env, &profile);
    }
    let channel: Channel = what
        .parse()
        .map_err(|_| format!("usage: cargo xtask release <prod|dev|bundles> (got {what:?})"))?;
    prepare(root, env, channel, &profile).map(|_| ())
}
