use std::path::{Path, PathBuf};
use std::process::Command;

use master_protocol::Channel;

use crate::certs::{Ca, San};
use crate::dotenv::Env;
use crate::release::build_master;
use crate::shell::{Res, Ssh, Step, capture, require_tools};
use crate::windows;

/// Where the binary and its certificates land on the VPS. `/usr/local/lib`
/// rather than `/usr/local/bin`: nobody runs this by hand, systemd does.
fn remote_lib(channel: Channel) -> &'static str {
    match channel {
        Channel::Prod => "/usr/local/lib/iw4l",
        Channel::Dev => "/usr/local/lib/iw4l-dev",
    }
}

fn remote_etc(channel: Channel) -> &'static str {
    match channel {
        Channel::Prod => "/etc/iw4l",
        Channel::Dev => "/etc/iw4l-dev",
    }
}

const DEFAULT_SINCE: &str = "2h";

struct Args {
    ssh: Ssh,
    channel: Channel,
    ca: Ca,
    since: String,
}

fn parse(env: &Env, args: &[String]) -> Res<Args> {
    let mut target = None;
    let mut channel = Channel::Prod;
    let mut since = DEFAULT_SINCE.to_string();
    let mut ca_dir = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--channel" => {
                channel = rest
                    .next()
                    .ok_or("--channel needs prod or dev")?
                    .parse()
                    .map_err(|_| "--channel needs prod or dev".to_string())?;
            }
            "--since" => {
                since = rest.next().ok_or("--since needs a journal window")?.clone();
            }
            "--ca" => {
                ca_dir = Some(PathBuf::from(rest.next().ok_or("--ca needs a directory")?));
            }
            flag if flag.starts_with('-') => return Err(format!("unknown option {flag}")),
            value if target.is_none() => target = Some(value.to_string()),
            extra => return Err(format!("unexpected argument {extra}")),
        }
    }
    check_since(&since)?;
    let target = target.ok_or("usage: cargo xtask master <verb> user@host")?;
    let ca_dir = match ca_dir {
        Some(dir) => dir,
        None => default_ca_dir(env)?,
    };
    Ok(Args {
        ssh: Ssh::new(&target)?,
        channel,
        ca: Ca::new(ca_dir),
        since,
    })
}

/// `~/.iw4l/ca` unless told otherwise. Deliberately outside the clone: the
/// private key of everybody's trust anchor is not a repository file.
fn default_ca_dir(env: &Env) -> Res<PathBuf> {
    if let Some(dir) = env.get("IW4L_MASTER_CA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var("HOME").map_err(|_| "HOME is unset; pass --ca DIR")?;
    Ok(PathBuf::from(home).join(".iw4l/ca"))
}

pub fn check_since(since: &str) -> Res<()> {
    let digits = since.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let unit = &since[digits.len()..];
    let ok = !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && matches!(
            unit,
            "min" | "mins" | "h" | "hour" | "hours" | "day" | "days" | "week" | "weeks"
        );
    if ok {
        Ok(())
    } else {
        Err(format!(
            "--since must look like 30min, 2h, 1day or 1week (got {since:?})"
        ))
    }
}

/// The unit text, printed by the binary that parses `serve`. Asking the binary
/// is the whole point: an `ExecStart` written here could drift from the flags
/// `serve` actually accepts, and this one cannot.
pub fn unit_text(
    root: &Path,
    channel: Channel,
    exec: &str,
    cert: &str,
    key: &str,
    updates: &str,
    owner: (&str, &str),
) -> Res<String> {
    let (user, group) = owner;
    capture(
        Command::new("cargo")
            .current_dir(root)
            .args(["run", "--quiet", "-p", "iw4l-master", "--", "print-unit"])
            .args(["--channel", channel.as_str()])
            .args(["--exec", exec])
            .args(["--cert", cert])
            .args(["--key", key])
            .args(["--updates", updates])
            .args(["--user", user])
            .args(["--group", group]),
    )
}

fn remote_bin(channel: Channel) -> String {
    format!("{}/iw4l-master", remote_lib(channel))
}

fn remote_ca(channel: Channel) -> String {
    format!("{}/iw4l-ca.pem", remote_etc(channel))
}

/// Build the static binary and put it on the VPS. `crt-static` is why the
/// glibc version over there does not have to match this machine's.
fn upload_binary(root: &Path, env: &Env, ssh: &Ssh, channel: Channel) -> Res<()> {
    let remote_lib = remote_lib(channel);
    let profile = windows::profile(env)?;
    let bin = build_master(root, &profile)?;
    let step = Step::start("master.upload", ssh.target());
    ssh.run(&format!("install -d -m 0755 '{remote_lib}'"))?;
    ssh.rsync(&["--chmod=F755"], &bin, &remote_bin(channel))?;
    step.done("");
    Ok(())
}

fn upload_certs(ca: &Ca, ssh: &Ssh, channel: Channel) -> Res<()> {
    let remote_etc = remote_etc(channel);
    ssh.run(&format!("install -d -m 0755 '{remote_etc}'"))?;
    ssh.rsync(&["--chmod=F644"], &ca.ca_cert(), &remote_ca(channel))?;
    ssh.rsync(
        &["--chmod=F644"],
        &ca.server_cert(),
        &format!("{remote_etc}/server-cert.pem"),
    )?;
    ssh.rsync(
        &["--chmod=F640"],
        &ca.server_key(),
        &format!("{remote_etc}/server-key.pem"),
    )?;
    // Readable by the service account and by nobody else on the box.
    ssh.run(&format!(
        "chown root:iw4l '{remote_etc}/server-key.pem' && chmod 0640 '{remote_etc}/server-key.pem'"
    ))
}

pub fn install(root: &Path, env: &Env, args: &[String]) -> Res<()> {
    let Args {
        ssh, channel, ca, ..
    } = parse(env, args)?;
    let remote_lib = remote_lib(channel);
    let remote_etc = remote_etc(channel);
    require_tools(&["cargo", "rsync", "ssh"])?;
    ca.ensure(&match channel {
        Channel::Prod => San::Labels,
        Channel::Dev => San::WithHost(ssh.host().to_owned()),
    })?;

    let step = Step::start(
        "master.install",
        &format!("host={} channel={channel}", ssh.target()),
    );
    ssh.run(
        "set -eu
        getent group iw4l >/dev/null || groupadd --system iw4l
        id iw4l >/dev/null 2>&1 || useradd --system --gid iw4l --home-dir /nonexistent --shell /usr/sbin/nologin iw4l",
    )?;
    upload_binary(root, env, &ssh, channel)?;
    upload_certs(&ca, &ssh, channel)?;

    let unit = unit_text(
        root,
        channel,
        &remote_bin(channel),
        &format!("{remote_etc}/server-cert.pem"),
        &format!("{remote_etc}/server-key.pem"),
        &format!("{remote_lib}/updates/{channel}"),
        ("iw4l", "iw4l"),
    )?;
    ssh.feed(
        &format!("cat >'/etc/systemd/system/{}'", channel.unit()),
        &unit,
    )?;
    ssh.run(&format!(
        "set -eu
        if command -v ufw >/dev/null && ufw status | grep -q '^Status: active'; then
          ufw allow {port}/udp
          ufw allow {port}/tcp
        fi
        systemctl daemon-reload
        systemctl enable '{unit}'
        systemctl restart '{unit}'",
        port = channel.port(),
        unit = channel.unit(),
    ))?;
    step.done("");
    report(&ssh, channel)?;
    hand_out(&ssh, channel, &ca)?;
    Ok(())
}

/// Rebuild, upload, restart. The certificates and the unit are left alone.
pub fn update(root: &Path, env: &Env, args: &[String]) -> Res<()> {
    let Args { ssh, channel, .. } = parse(env, args)?;
    require_tools(&["cargo", "rsync", "ssh"])?;
    upload_binary(root, env, &ssh, channel)?;
    ssh.run(&format!("systemctl restart '{}'", channel.unit()))?;
    report(&ssh, channel)
}

pub fn status(env: &Env, args: &[String]) -> Res<()> {
    let Args { ssh, channel, .. } = parse(env, args)?;
    report(&ssh, channel)
}

pub fn logs(env: &Env, args: &[String]) -> Res<()> {
    let Args {
        ssh,
        channel,
        since,
        ..
    } = parse(env, args)?;
    journal(&ssh, channel, &since)
}

/// Stop and forget the service: its unit and its binary go. The certificates
/// stay on both ends — a reinstall has to keep the trust anchor every player
/// already pinned.
pub fn uninstall(env: &Env, args: &[String]) -> Res<()> {
    let Args { ssh, channel, .. } = parse(env, args)?;
    let remote_lib = remote_lib(channel);
    let remote_etc = remote_etc(channel);
    ssh.run(&format!(
        "set -eu
        systemctl disable --now '{unit}' 2>/dev/null || true
        rm -f '/etc/systemd/system/{unit}'
        systemctl daemon-reload
        rm -f '{bin}'
        rmdir '{remote_lib}' 2>/dev/null || true",
        unit = channel.unit(),
        bin = remote_bin(channel),
    ))?;
    println!(
        "master: {} removed from {}. Certificates under {remote_etc} and the local CA were kept.",
        channel.unit(),
        ssh.target()
    );
    Ok(())
}

pub fn journal(ssh: &Ssh, channel: Channel, since: &str) -> Res<()> {
    ssh.run(&format!(
        "journalctl -u '{unit}' --since '-{since}' --no-pager",
        unit = channel.unit(),
    ))
}

fn report(ssh: &Ssh, channel: Channel) -> Res<()> {
    ssh.run(&format!(
        "systemctl --no-pager --full status '{unit}' || true",
        unit = channel.unit()
    ))?;
    ssh.run(&format!(
        "'{bin}' status --connect 127.0.0.1:{port} --server-name '{name}' --ca-cert '{ca}'",
        bin = remote_bin(channel),
        port = channel.port(),
        name = channel.server_name(),
        ca = remote_ca(channel),
    ))
}

fn hand_out(ssh: &Ssh, channel: Channel, ca: &Ca) -> Res<()> {
    let remote_lib = remote_lib(channel);
    let descriptor = updater::Community {
        schema: 1,
        name: format!("IW4L {channel}"),
        master: updater::Master {
            address: format!("{}:{}", ssh.host(), channel.port()),
            server_name: match channel {
                Channel::Prod => channel.server_name().into(),
                Channel::Dev => ssh.host().into(),
            },
        },
        updates: updater::Updates {
            url: format!(
                "https://{}:{}/updates/manifest.toml",
                ssh.host(),
                channel.port()
            ),
            ca_pem: std::fs::read_to_string(ca.ca_cert()).map_err(|e| e.to_string())?,
        },
    };
    let path = ca.dir().join(format!("community-{channel}.iw4l-server"));
    std::fs::write(
        &path,
        toml::to_string_pretty(&descriptor).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!(
        "Upload client updates to {remote_lib}/updates/{channel}, then give {} and iw4l.exe to players over a trusted channel.",
        path.display()
    );
    Ok(())
}

pub fn run_cli(root: &Path, env: &Env, args: &[String]) -> Res<()> {
    let (verb, rest) = args
        .split_first()
        .ok_or("usage: cargo xtask master <install|update|status|logs|uninstall> user@host")?;
    match verb.as_str() {
        "install" => install(root, env, rest),
        "update" => update(root, env, rest),
        "status" => status(env, rest),
        "logs" => logs(env, rest),
        "uninstall" => uninstall(env, rest),
        other => Err(format!(
            "unknown master verb {other}; expected install|update|status|logs|uninstall"
        )),
    }
}
