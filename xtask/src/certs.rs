use std::path::{Path, PathBuf};
use std::process::Command;

use master_protocol::Channel;

use crate::shell::{Res, capture, run};

const SERVER_DAYS: &str = "825";
const CA_DAYS: &str = "3650";

pub enum San {
    Labels,
    WithHost(String),
}

impl San {
    fn value(&self) -> String {
        let labels = Channel::ALL
            .iter()
            .map(|channel| format!("DNS:{}", channel.server_name()))
            .collect::<Vec<_>>()
            .join(",");
        match self {
            Self::Labels => labels,
            Self::WithHost(host) if is_ipv4(host) => format!("{labels},IP:{host}"),
            Self::WithHost(host) => format!("{labels},DNS:{host}"),
        }
    }

    fn host(&self) -> Option<&str> {
        match self {
            Self::Labels => None,
            Self::WithHost(host) => Some(host),
        }
    }
}

pub struct Ca {
    dir: PathBuf,
}

impl Ca {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn ca_key(&self) -> PathBuf {
        self.dir.join("ca-key.pem")
    }

    pub fn ca_cert(&self) -> PathBuf {
        self.dir.join("iw4l-ca.pem")
    }

    pub fn server_key(&self) -> PathBuf {
        self.dir.join("server-key.pem")
    }

    pub fn server_cert(&self) -> PathBuf {
        self.dir.join("server-cert.pem")
    }

    /// Make sure the directory holds a CA and a server identity covering `san`,
    /// minting only what is missing. An identity that is present but wrong is
    /// never replaced: that would silently invalidate every `ca.pem` already
    /// handed out.
    pub fn ensure(&self, san: &San) -> Res<()> {
        crate::shell::require_tools(&["openssl"])?;
        std::fs::create_dir_all(&self.dir)
            .map_err(|error| format!("creating {}: {error}", self.dir.display()))?;
        set_mode(&self.dir, 0o700)?;
        self.ensure_ca()?;
        self.ensure_server(san)?;
        run(Command::new("openssl")
            .arg("verify")
            .arg("-CAfile")
            .arg(self.ca_cert())
            .arg(self.server_cert())
            .stdout(std::process::Stdio::null()))?;
        println!("certificates ready: {}", self.dir.display());
        Ok(())
    }

    fn ensure_ca(&self) -> Res<()> {
        let (key, cert) = (self.ca_key(), self.ca_cert());
        if key.is_file() && cert.is_file() {
            return Ok(());
        }
        if key.exists() || cert.exists() {
            return Err(format!(
                "CA under {} is incomplete; refusing to replace it",
                self.dir.display()
            ));
        }
        gen_key(&key)?;
        run(Command::new("openssl")
            .args(["req", "-x509", "-new", "-sha256", "-days", CA_DAYS])
            .arg("-key")
            .arg(&key)
            .arg("-out")
            .arg(&cert)
            .args(["-subj", "/CN=IW4L Release CA"]))
    }

    fn ensure_server(&self, san: &San) -> Res<()> {
        let (key, cert) = (self.server_key(), self.server_cert());
        if key.is_file() && cert.is_file() {
            let Some(host) = san.host() else {
                return Ok(());
            };
            if cert_covers_host(&cert, host)? {
                return Ok(());
            }
            return Err(format!(
                "server cert under {} does not cover {host} ({}); refusing to replace it",
                self.dir.display(),
                san.value()
            ));
        }
        if key.exists() || cert.exists() {
            return Err(format!(
                "server identity under {} is incomplete; refusing to replace it",
                self.dir.display()
            ));
        }
        gen_key(&key)?;
        let csr = self.dir.join("server.csr");
        let ext = self.dir.join("server.ext");
        let subject = format!("/CN={}", Channel::Prod.server_name());
        run(Command::new("openssl")
            .args(["req", "-new", "-key"])
            .arg(&key)
            .arg("-out")
            .arg(&csr)
            .args(["-subj", &subject]))?;
        std::fs::write(
            &ext,
            format!(
                "basicConstraints=critical,CA:FALSE\n\
                 keyUsage=critical,digitalSignature,keyEncipherment\n\
                 extendedKeyUsage=serverAuth\n\
                 subjectAltName={}\n",
                san.value()
            ),
        )
        .map_err(|error| format!("writing {}: {error}", ext.display()))?;
        let signed = run(Command::new("openssl")
            .args(["x509", "-req", "-sha256", "-days", SERVER_DAYS, "-in"])
            .arg(&csr)
            .arg("-CA")
            .arg(self.ca_cert())
            .arg("-CAkey")
            .arg(self.ca_key())
            .arg("-CAserial")
            .arg(self.dir.join("ca.srl"))
            .arg("-CAcreateserial")
            .arg("-extfile")
            .arg(&ext)
            .arg("-out")
            .arg(&cert));
        let _ = std::fs::remove_file(&csr);
        let _ = std::fs::remove_file(&ext);
        signed
    }
}

fn gen_key(path: &Path) -> Res<()> {
    run(Command::new("openssl")
        .args([
            "genpkey",
            "-algorithm",
            "RSA",
            "-pkeyopt",
            "rsa_keygen_bits:3072",
            "-out",
        ])
        .arg(path))?;
    set_mode(path, 0o600)
}

pub fn is_ipv4(value: &str) -> bool {
    value.parse::<std::net::Ipv4Addr>().is_ok()
}

fn cert_covers_host(cert: &Path, host: &str) -> Res<bool> {
    let sans = capture(
        Command::new("openssl")
            .args(["x509", "-in"])
            .arg(cert)
            .args(["-noout", "-ext", "subjectAltName"]),
    )?;
    let entry = if is_ipv4(host) {
        format!("IP Address:{host}")
    } else {
        format!("DNS:{host}")
    };
    // openssl prints the extension name on its own line, then the
    // comma-separated entries; both separators have to split.
    Ok(sans.split(['\n', ',']).any(|item| item.trim() == entry))
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

/// `cargo xtask certs <host>` — the release CA named by `IW4L_RELEASE_KEY`.
pub fn run_cli(env: &crate::dotenv::Env, args: &[String]) -> Res<()> {
    let host = args
        .first()
        .ok_or("usage: cargo xtask certs <host>")?
        .clone();
    let dir = env.require("IW4L_RELEASE_KEY")?;
    Ca::new(PathBuf::from(dir)).ensure(&San::WithHost(host))
}
