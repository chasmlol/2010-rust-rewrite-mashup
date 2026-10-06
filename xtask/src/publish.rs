//! Publish an already prepared release. Compiles nothing and compresses
//! nothing: every byte and every hash was frozen by `cargo xtask release`.
//!
//! The shape is content-addressed and resumable — inventory the remote by
//! sha256, upload only what differs into a staging directory, verify it landed
//! intact, then promote by rename. The master is switched before the client
//! manifest, so a player never learns about a release the relay cannot serve.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use master_protocol::Channel;
use serde_json::Value;

use crate::dotenv::Env;
use crate::release::file_sha256;
use crate::shell::{Res, Ssh, capture, check_deploy_root, require_tools, run};

/// How long the restarted master gets to answer `status` with the protocol the
/// release was built against, before its predecessor is put back.
const HEALTH_DEADLINE: Duration = Duration::from_secs(15);

const MISSING: &str = "MISSING";

/// One shipped file, as `deployment.json` describes it.
struct Row {
    local: String,
    /// Relative to `IW4L_DEPLOY_ROOT`.
    remote: String,
    sha256: String,
    size: u64,
    immutable: bool,
    role: String,
}

impl Row {
    fn is_manifest(&self) -> bool {
        self.role == "manifest"
    }
}

/// Holds the per-channel publish lock for as long as it is alive.
struct Lock<'a> {
    ssh: &'a Ssh,
    dir: String,
}

impl Drop for Lock<'_> {
    fn drop(&mut self) {
        let _ = self
            .ssh
            .try_run(&format!("rmdir '{}' 2>/dev/null || true", self.dir));
    }
}

/// stdin: one absolute path per line. stdout: `path<TAB>sha256`, or
/// `path<TAB>MISSING`. One ssh for the whole list — sequentially hashing over
/// ssh cost about ten seconds per file.
const REMOTE_SHA256_LIST: &str = r#"set -eu
while IFS= read -r path; do
  [ -n "$path" ] || continue
  if [ -f "$path" ]; then
    printf "%s\t%s\n" "$path" "$(sha256sum "$path" | cut -d" " -f1)"
  else
    printf "%s\t%s\n" "$path" "MISSING"
  fi
done
"#;

/// stdin: `staged<TAB>final<TAB>octal-mode`.
const REMOTE_PROMOTE: &str = r#"set -eu
while IFS="$(printf "\t")" read -r staged final mode; do
  [ -n "$staged" ] || continue
  mkdir -p "$(dirname "$final")"
  mv -f "$staged" "$final"
  chmod "$mode" "$final"
done
"#;

fn read_json(path: &Path) -> Res<Value> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("reading {}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("parsing {}: {error}", path.display()))
}

fn field<'a>(value: &'a Value, key: &str, path: &Path) -> Res<&'a Value> {
    value
        .get(key)
        .ok_or_else(|| format!("{} has no {key}", path.display()))
}

fn str_field(value: &Value, key: &str, path: &Path) -> Res<String> {
    field(value, key, path)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("{}: {key} is not a string", path.display()))
}

fn u64_field(value: &Value, key: &str, path: &Path) -> Res<u64> {
    field(value, key, path)?
        .as_u64()
        .ok_or_else(|| format!("{}: {key} is not a number", path.display()))
}

/// Where a file goes under `IW4L_DEPLOY_ROOT`. Only the master lives outside
/// the channel's release directory: it is shared and addressed by its own SHA.
fn rows_of(descriptor: &Value, channel: Channel, path: &Path) -> Res<Vec<Row>> {
    let files = field(descriptor, "files", path)?
        .as_array()
        .ok_or_else(|| format!("{}: files is not an array", path.display()))?;
    files
        .iter()
        .map(|entry| {
            let role = str_field(entry, "role", path)?;
            let remote = str_field(entry, "remote", path)?;
            let remote = if role == "master" {
                remote
            } else {
                format!("releases/{channel}/{remote}")
            };
            Ok(Row {
                local: str_field(entry, "local", path)?,
                remote,
                sha256: str_field(entry, "sha256", path)?,
                size: u64_field(entry, "size", path)?,
                immutable: field(entry, "immutable", path)?
                    .as_bool()
                    .ok_or_else(|| format!("{}: immutable is not a bool", path.display()))?,
                role,
            })
        })
        .collect()
}

fn release_dir(root: &Path, channel: Channel) -> Res<PathBuf> {
    if let Ok(explicit) = std::env::var("RELEASE")
        && !explicit.is_empty()
    {
        let dir = PathBuf::from(&explicit);
        return Ok(if dir.is_absolute() {
            dir
        } else {
            root.join(explicit)
        });
    }
    let channel_dir = root.join("dist/releases").join(channel.as_str());
    let latest = channel_dir.join("LATEST");
    let id = std::fs::read_to_string(&latest).map_err(|_| {
        format!(
            "no RELEASE= and no {}; run `cargo xtask release {channel}` first",
            latest.display()
        )
    })?;
    Ok(channel_dir.join(id.trim()))
}

fn remote_inventory(ssh: &Ssh, paths: &[String]) -> Res<Vec<(String, String)>> {
    let input = paths.join("\n");
    let output = ssh.feed(REMOTE_SHA256_LIST, &format!("{input}\n"))?;
    Ok(output
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(path, hash)| (path.to_string(), hash.to_string()))
        .collect())
}

fn lookup(inventory: &[(String, String)], path: &str) -> String {
    inventory
        .iter()
        .find(|(known, _)| known == path)
        .map_or(MISSING.to_string(), |(_, hash)| hash.clone())
}

pub fn run_cli(root: &Path, env: &Env, args: &[String]) -> Res<()> {
    let channel: Channel = args
        .first()
        .ok_or("usage: cargo xtask publish <prod|dev>")?
        .parse()
        .map_err(|_| "usage: cargo xtask publish <prod|dev>".to_string())?;
    publish(root, env, channel)
}

pub fn publish(root: &Path, env: &Env, channel: Channel) -> Res<()> {
    require_tools(&["curl", "rsync", "ssh"])?;
    let ssh = Ssh::new(&env.require("IW4L_DEPLOY_HOST")?)?;
    let deploy_root = env.require("IW4L_DEPLOY_ROOT")?;
    check_deploy_root(&deploy_root)?;

    let dir = release_dir(root, channel)?;
    let descriptor_path = dir.join("deployment.json");
    if !descriptor_path.is_file() {
        return Err(format!("not a prepared release: {}", dir.display()));
    }
    let descriptor = read_json(&descriptor_path)?;
    let release_id = str_field(&descriptor, "id", &descriptor_path)?;
    let protocol = u64_field(&descriptor, "protocol", &descriptor_path)?;
    let desc_channel = str_field(&descriptor, "channel", &descriptor_path)?;
    let desc_port = u64_field(&descriptor, "master_port", &descriptor_path)?;
    if desc_channel != channel.as_str() {
        return Err(format!("release channel {desc_channel} != {channel}"));
    }
    if desc_port != u64::from(channel.port()) {
        return Err(format!(
            "release master_port {desc_port} != {}",
            channel.port()
        ));
    }

    let rows = rows_of(&descriptor, channel, &descriptor_path)?;
    for row in &rows {
        let local = dir.join(&row.local);
        if !local.is_file() {
            return Err(format!("missing {}", local.display()));
        }
        if file_sha256(&local)? != row.sha256 {
            return Err(format!("local sha256 mismatch: {}", local.display()));
        }
    }

    // The client and the master inside one release have to agree, or the
    // handshake ALPN would reject every player the moment the manifest flips.
    let manifest_path = dir.join("server/updates/manifest.toml");
    let manifest =
        updater::Manifest::parse(&std::fs::read(&manifest_path).map_err(|e| e.to_string())?)?;
    if u64::from(manifest.protocol) != protocol {
        return Err("client/master protocol mismatch inside the release".to_string());
    }

    let master_local = dir.join("server/iw4l-master");
    let master_sha = file_sha256(&master_local)?;
    let bin_link = format!("{deploy_root}/bin/iw4l-master-{channel}");
    let new_master = format!("{deploy_root}/masters/{master_sha}/iw4l-master");
    let staging = format!("{deploy_root}/staging/{channel}/{release_id}");
    let remote_releases = format!("{deploy_root}/releases/{channel}");

    ssh.run(&format!("install -d -m 0755 '{deploy_root}/locks'"))?;
    let lock_dir = format!("{deploy_root}/locks/{channel}");
    if !ssh.try_run(&format!("mkdir '{lock_dir}'"))? {
        return Err(format!(
            "publish: channel {channel} already being published"
        ));
    }
    let _lock = Lock {
        ssh: &ssh,
        dir: lock_dir,
    };

    ssh.run(&format!(
        "install -d -m 0755 '{staging}' '{remote_releases}/manifests' '{deploy_root}/masters/{master_sha}' '{deploy_root}/bin'"
    ))?;

    let inventory_started = Instant::now();
    let finals: Vec<String> = rows
        .iter()
        .map(|row| format!("{deploy_root}/{}", row.remote))
        .collect();
    let inventory = remote_inventory(&ssh, &finals)?;

    let mut need = Vec::new();
    let (mut upload_bytes, mut skipped) = (0_u64, 0_usize);
    for row in &rows {
        let final_path = format!("{deploy_root}/{}", row.remote);
        let existing = lookup(&inventory, &final_path);
        if existing == row.sha256 {
            skipped += 1;
            continue;
        }
        if existing != MISSING && row.immutable {
            return Err(format!("immutable collision at {final_path}"));
        }
        upload_bytes += row.size;
        need.push(row);
    }
    println!(
        "upload: start files={} bytes={upload_bytes} skipped={skipped} inventory={}s",
        need.len(),
        inventory_started.elapsed().as_secs()
    );

    let upload_started = Instant::now();
    if !need.is_empty() {
        for row in &need {
            let remote = format!("{staging}/{}", row.local);
            let parent = remote.rsplit_once('/').map_or("", |(head, _)| head);
            ssh.run(&format!("install -d -m 0755 '{parent}'"))?;
            ssh.rsync(
                &["--info=progress2", "--stats", "--chmod=F644"],
                &dir.join(&row.local),
                &remote,
            )?;
        }
        let staged: Vec<String> = need
            .iter()
            .map(|row| format!("{staging}/{}", row.local))
            .collect();
        let landed = remote_inventory(&ssh, &staged)?;
        for (row, path) in need.iter().zip(&staged) {
            if lookup(&landed, path) != row.sha256 {
                return Err(format!("staged sha256 mismatch: {path}"));
            }
        }
    }
    println!(
        "upload: done elapsed={}s",
        upload_started.elapsed().as_secs()
    );

    // The manifest is not promoted with the rest: it is the switch, and it is
    // thrown only after the master answers on the new protocol.
    let promote: String = need
        .iter()
        .filter(|row| !row.is_manifest())
        .map(|row| {
            let mode = if row.role == "master" { "755" } else { "644" };
            format!(
                "{staging}/{}\t{deploy_root}/{}\t{mode}\n",
                row.local, row.remote
            )
        })
        .collect();
    if !promote.is_empty() {
        ssh.feed(REMOTE_PROMOTE, &promote)?;
    }

    let master = MasterSwitch {
        ssh: &ssh,
        channel,
        deploy_root: &deploy_root,
        bin_link: &bin_link,
        new_master: &new_master,
        master_sha: &master_sha,
        protocol,
    };
    master.activate()?;
    // `activate` returns early when the binary is unchanged, and an unchanged
    // binary can still be a stale process. Ask before throwing the switch.
    let running = master.running_protocol();
    if running != Some(protocol) {
        return Err(format!(
            "publish: running master protocol {} != client {protocol}",
            running.map_or("none".to_string(), |value| value.to_string())
        ));
    }

    activate_manifest(&ssh, &dir, &staging, &remote_releases, &release_id)?;

    let ca_pem = dir.join("client/iw4l-ca.pem");
    verify_published_manifest(&ssh, channel, &ca_pem, &manifest_path)?;
    println!("verify.manifest: ok release={release_id}");

    println!("[deploy] master health (on {})", ssh.host());
    ssh.run(&format!(
        "'{bin_link}' status --connect 127.0.0.1:{port} --server-name '{name}' --ca-cert /etc/iw4l/iw4l-ca.pem",
        port = channel.port(),
        name = channel.server_name(),
    ))?;

    local_reachability(&ssh, channel, &master_local, &ca_pem)?;
    println!(
        "[deploy] {channel} healthy: master udp/{port}, releases https://{host}:{port}/updates/",
        port = channel.port(),
        host = ssh.host(),
    );
    ssh.run(&format!("rm -rf '{staging}'"))
}

struct MasterSwitch<'a> {
    ssh: &'a Ssh,
    channel: Channel,
    deploy_root: &'a str,
    bin_link: &'a str,
    new_master: &'a str,
    master_sha: &'a str,
    protocol: u64,
}

impl MasterSwitch<'_> {
    fn probe(&self) -> Res<String> {
        self.ssh.capture(&format!(
            "'{link}' status --connect 127.0.0.1:{port} --server-name '{name}' --ca-cert /etc/iw4l/iw4l-ca.pem",
            link = self.bin_link,
            port = self.channel.port(),
            name = self.channel.server_name(),
        ))
    }

    fn running_protocol(&self) -> Option<u64> {
        let status = self.probe().ok()?;
        parse_protocol(&status)
    }

    fn point_at(&self, sha: &str) -> Res<()> {
        self.ssh.run(&format!(
            "set -eu
            ln -sfn '../masters/{sha}/iw4l-master' '{link}.new'
            mv -T '{link}.new' '{link}'
            systemctl restart '{unit}'",
            link = self.bin_link,
            unit = self.channel.unit(),
        ))
    }

    fn activate(&self) -> Res<()> {
        let unit = self.channel.unit();
        let old_pid = self
            .ssh
            .capture(&format!("systemctl show -p MainPID --value '{unit}'"))
            .unwrap_or_else(|_| "0".to_string());
        let old_pid = old_pid.trim().to_string();
        let current_sha = self
            .ssh
            .capture(&format!(
                "if [ -e '{link}' ]; then sha256sum '{link}' | cut -d' ' -f1; fi",
                link = self.bin_link
            ))
            .unwrap_or_else(|_| String::new());
        let current_sha = current_sha.trim().to_string();
        if current_sha == self.master_sha {
            println!("activate.master: unchanged pid={old_pid}");
            return Ok(());
        }
        let named = if current_sha.is_empty() {
            "none"
        } else {
            &current_sha
        };
        println!(
            "activate.master: start old_sha={named} new_sha={}",
            self.master_sha
        );
        // Keep the binary that is running now, so a failed health check has
        // something to go back to even if it was never published from here.
        self.ssh.run(&format!(
            "set -eu
            if [ -n '{current_sha}' ] && [ ! -f '{root}/masters/{current_sha}/iw4l-master' ] && [ -e '{link}' ]; then
              install -d -m 0755 '{root}/masters/{current_sha}'
              cp -L '{link}' '{root}/masters/{current_sha}/iw4l-master'
            fi
            test -f '{new}'",
            root = self.deploy_root,
            link = self.bin_link,
            new = self.new_master,
        ))?;
        self.point_at(self.master_sha)?;

        let deadline = Instant::now() + HEALTH_DEADLINE;
        loop {
            let seen = self.running_protocol();
            if seen == Some(self.protocol) {
                break;
            }
            if let Some(other) = seen {
                println!(
                    "activate.master: waiting for protocol={} (saw {other})",
                    self.protocol
                );
            }
            if Instant::now() >= deadline {
                println!(
                    "activate.master: health deadline protocol={}; restoring previous master",
                    seen.map_or("none".to_string(), |value| value.to_string())
                );
                if !current_sha.is_empty() {
                    let _ = self.point_at(&current_sha);
                }
                return Err(
                    "publish: master activation failed; client manifest not switched".to_string(),
                );
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        let new_pid = self
            .ssh
            .capture(&format!("systemctl show -p MainPID --value '{unit}'"))?;
        println!(
            "activate.master: done pid={} protocol={}",
            new_pid.trim(),
            self.protocol
        );
        println!(
            "[deploy] note: replacing master is not a seamless keep of existing QUIC sessions"
        );
        Ok(())
    }
}

fn parse_protocol(status: &str) -> Option<u64> {
    status
        .split_whitespace()
        .find_map(|token| token.strip_prefix("protocol="))
        .and_then(|value| value.parse().ok())
}

fn activate_manifest(
    ssh: &Ssh,
    dir: &Path,
    staging: &str,
    remote_releases: &str,
    release_id: &str,
) -> Res<()> {
    let local = dir.join("server/updates/manifest.toml");
    let local_sha = file_sha256(&local)?;
    let live = format!("{remote_releases}/manifest.toml");
    let archived = format!("{remote_releases}/manifests/{release_id}.toml");
    let remote_live = ssh
        .capture(&format!(
            "if [ -f '{live}' ]; then sha256sum '{live}' | cut -d' ' -f1; fi"
        ))?
        .trim()
        .to_string();
    if remote_live == local_sha {
        println!("activate.manifest: unchanged release={release_id}");
        return Ok(());
    }
    // The manifest may have been skipped above — its archived copy can already
    // match while the live one does not, which is exactly a rollback. Put it in
    // staging unconditionally; it is one small file.
    let staged = format!("{staging}/server/updates/manifest.toml");
    ssh.run(&format!("install -d -m 0755 '{staging}/server/updates'"))?;
    ssh.rsync(&["--chmod=F644"], &local, &staged)?;
    ssh.run(&format!(
        "set -eu
        install -d -m 0755 '{remote_releases}/manifests'
        cp -f '{staged}' '{archived}'
        chmod 644 '{archived}'
        mv -T '{staged}' '{live}.new'
        chmod 644 '{live}.new'
        mv -T '{live}.new' '{live}'"
    ))?;
    println!("activate.manifest: done release={release_id}");
    Ok(())
}

fn verify_published_manifest(
    ssh: &Ssh,
    channel: Channel,
    ca_pem: &Path,
    local_manifest: &Path,
) -> Res<()> {
    let got = std::env::temp_dir().join(format!("iw4l-manifest-{}.toml", std::process::id()));
    run(Command::new("curl")
        .args([
            "--fail",
            "--show-error",
            "--silent",
            "--connect-timeout",
            "3",
            "--max-time",
            "10",
            "--cacert",
        ])
        .arg(ca_pem)
        .args(["--noproxy", "*", "--connect-to"])
        .arg(format!(
            "{}:{}:{}:{}",
            channel.server_name(),
            channel.port(),
            ssh.host(),
            channel.port()
        ))
        .arg("-o")
        .arg(&got)
        .arg(format!(
            "https://{name}:{port}/updates/manifest.toml",
            name = channel.server_name(),
            port = channel.port()
        )))?;
    let matches = file_sha256(&got)? == file_sha256(local_manifest)?;
    let _ = std::fs::remove_file(&got);
    if matches {
        Ok(())
    } else {
        Err("published manifest does not match the prepared release".to_string())
    }
}

/// A relay the VPS can reach but this machine cannot is usually a tun/VPN
/// device that forwards TCP and drops UDP — not a broken deploy.
fn local_reachability(ssh: &Ssh, channel: Channel, master_bin: &Path, ca_pem: &Path) -> Res<()> {
    println!("[deploy] master reachability (from this machine)");
    let reachable = Command::new("timeout")
        .arg("5")
        .arg(master_bin)
        .arg("status")
        .args(["--connect", &format!("{}:{}", ssh.host(), channel.port())])
        .args(["--server-name", channel.server_name()])
        .arg("--ca-cert")
        .arg(ca_pem)
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if reachable {
        return Ok(());
    }
    let host_ip = capture(Command::new("getent").args(["ahostsv4", ssh.host()]))
        .ok()
        .and_then(|text| {
            text.lines()
                .next()
                .and_then(|line| line.split_whitespace().next().map(str::to_string))
        });
    let egress = host_ip
        .as_deref()
        .filter(|_| crate::shell::tool_on_path("ip"))
        .and_then(|ip| capture(Command::new("ip").args(["-o", "route", "get", ip])).ok())
        .and_then(|route| {
            let mut tokens = route.split_whitespace();
            while let Some(token) = tokens.next() {
                if token == "dev" {
                    return tokens.next().map(str::to_string);
                }
            }
            None
        })
        .unwrap_or_else(|| "unknown".to_string());
    eprintln!(
        "[deploy] WARNING: udp/{port} unreachable from this machine, though the host answers itself.",
        port = channel.port()
    );
    eprintln!(
        "[deploy]   route to {host} leaves via {egress}; a tun/VPN device forwards TCP but often drops UDP.",
        host = ssh.host()
    );
    eprintln!(
        "[deploy]   this is not automatically a VPS fault; set IW4L_DEPLOY_REQUIRE_LOCAL_UDP=1 to make this fatal."
    );
    if std::env::var("IW4L_DEPLOY_REQUIRE_LOCAL_UDP").as_deref() == Ok("1") {
        return Err(format!(
            "udp/{} unreachable from this machine",
            channel.port()
        ));
    }
    Ok(())
}

/// `cargo xtask logs <prod|dev> [--since 2h]` — the bounded master journal on
/// our release host. A master of your own is `cargo xtask master logs`.
pub fn logs(env: &Env, args: &[String]) -> Res<()> {
    let (channel, rest) = args
        .split_first()
        .ok_or("usage: cargo xtask logs <prod|dev> [--since 2h]")?;
    let channel: Channel = channel
        .parse()
        .map_err(|_| format!("usage: cargo xtask logs <prod|dev> (got {channel:?})"))?;
    let since = match rest {
        // `make logs prod` forwards an empty SINCE rather than unsetting it.
        [] => std::env::var("SINCE")
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "2h".to_string()),
        [flag, value] if flag == "--since" => value.clone(),
        _ => return Err("usage: cargo xtask logs <prod|dev> [--since 2h]".to_string()),
    };
    crate::master::check_since(&since)?;
    let ssh = Ssh::new(&env.require("IW4L_DEPLOY_HOST")?)?;
    crate::master::journal(&ssh, channel, &since)
}
