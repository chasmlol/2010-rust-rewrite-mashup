use std::path::Path;

use master_protocol::Channel;

use crate::certs::{Ca, San};
use crate::dotenv::Env;
use crate::master::unit_text;
use crate::shell::{Res, Ssh, Step, require_tools};

const JOURNALD_CONF: &str = "[Journal]\nSystemMaxUse=300M\nMaxRetentionSec=14day\n";

pub fn run_cli(root: &Path, env: &Env, _args: &[String]) -> Res<()> {
    require_tools(&["cargo", "openssl", "rsync", "ssh"])?;
    let ssh = Ssh::new(&env.require("IW4L_DEPLOY_HOST")?)?;
    let deploy_root = env.require("IW4L_DEPLOY_ROOT")?;
    crate::shell::check_deploy_root(&deploy_root)?;
    let ca = Ca::new(env.require("IW4L_RELEASE_KEY")?.into());

    ca.ensure(&San::Labels)?;

    let step = Step::start(
        "provision",
        &format!("host={} root={deploy_root}", ssh.target()),
    );
    ssh.run(&format!(
        "set -eu
        if ! command -v rsync >/dev/null; then
          apt-get update
          DEBIAN_FRONTEND=noninteractive apt-get install -y rsync
        fi
        getent group iw4l-release >/dev/null || groupadd --system iw4l-release
        id iw4l >/dev/null 2>&1 || useradd --system --gid iw4l-release --home-dir '{deploy_root}' --shell /usr/sbin/nologin iw4l
        install -d -m 0755 \
          '{deploy_root}/bin' \
          '{deploy_root}/masters' \
          '{deploy_root}/releases/dev/manifests' \
          '{deploy_root}/releases/prod/manifests' \
          '{deploy_root}/staging/dev' \
          '{deploy_root}/staging/prod' \
          '{deploy_root}/locks' \
          /etc/iw4l /etc/systemd/system /etc/systemd/journald.conf.d"
    ))?;

    for channel in Channel::ALL {
        let unit = unit_text(
            root,
            channel,
            &format!("{deploy_root}/bin/iw4l-master-{channel}"),
            "/etc/iw4l/server-cert.pem",
            "/etc/iw4l/server-key.pem",
            &format!("{deploy_root}/releases/{channel}"),
            ("iw4l", "iw4l-release"),
        )?;
        ssh.feed(
            &format!("cat >'/etc/systemd/system/{}'", channel.unit()),
            &unit,
        )?;
    }
    ssh.feed("cat >/etc/systemd/journald.conf.d/iw4l.conf", JOURNALD_CONF)?;
    ssh.rsync(&["--chmod=F644"], &ca.ca_cert(), "/etc/iw4l/iw4l-ca.pem")?;
    ssh.rsync(
        &["--chmod=F644"],
        &ca.server_cert(),
        "/etc/iw4l/server-cert.pem",
    )?;
    ssh.rsync(
        &["--chmod=F640"],
        &ca.server_key(),
        "/etc/iw4l/server-key.pem",
    )?;

    let ufw_ports = Channel::ALL
        .iter()
        .map(|channel| {
            format!(
                "ufw allow {}/udp\n          ufw allow {}/tcp",
                channel.port(),
                channel.port()
            )
        })
        .collect::<Vec<_>>()
        .join("\n          ");
    let units = Channel::ALL
        .iter()
        .map(|channel| format!("'{}'", channel.unit()))
        .collect::<Vec<_>>()
        .join(" ");
    ssh.run(&format!(
        "set -eu
        systemctl disable --now iw4l-dedicated@prod.service iw4l-dedicated@dev.service 2>/dev/null || true
        rm -f /etc/systemd/system/iw4l-dedicated@.service \
          '{deploy_root}/bin/iw4l-dedicated-prod' \
          '{deploy_root}/bin/iw4l-dedicated-dev' \
          '{deploy_root}/config/prod.env' \
          '{deploy_root}/config/dev.env'
        chown root:iw4l-release /etc/iw4l/server-key.pem
        chmod 640 /etc/iw4l/server-key.pem
        systemctl daemon-reload
        systemctl restart systemd-journald
        if command -v ufw >/dev/null && ufw status | grep -q '^Status: active'; then
          {ufw_ports}
        fi
        systemctl enable {units} >/dev/null
        # Do not start a master here: its executable symlink appears on first publish."
    ))?;

    step.done("");
    println!(
        "provision: journald/units updated; a master is started by `cargo xtask publish` when its binary changes"
    );
    Ok(())
}
