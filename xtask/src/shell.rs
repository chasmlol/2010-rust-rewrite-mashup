//! Process plumbing for the ship commands: tool checks, timed steps, ssh and
//! rsync.

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

pub type Res<T> = Result<T, String>;

/// `-o BatchMode=yes`: a deploy never stops to ask for a password. The
/// keepalive triple bounds a hung TCP session the way the shell version did.
const SSH_BASE: [&str; 8] = [
    "-o",
    "BatchMode=yes",
    "-o",
    "ConnectTimeout=10",
    "-o",
    "ServerAliveInterval=5",
    "-o",
    "ServerAliveCountMax=3",
];

pub const RSYNC_TIMEOUT: &str = "--timeout=60";

pub fn tool_on_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    let exe = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(&path)
        .any(|dir| is_executable(&dir.join(name)) || is_executable(&dir.join(&exe)))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

pub fn require_tools(tools: &[&str]) -> Res<()> {
    for tool in tools {
        if !tool_on_path(tool) {
            return Err(format!("missing tool: {tool}"));
        }
    }
    Ok(())
}

fn describe(cmd: &Command) -> String {
    let mut out = cmd.get_program().to_string_lossy().into_owned();
    for arg in cmd.get_args() {
        out.push(' ');
        out.push_str(&arg.to_string_lossy());
    }
    out
}

/// Run to completion with inherited stdio. A non-zero exit is an error, which
/// is what `set -e` bought every line of the scripts.
pub fn run(cmd: &mut Command) -> Res<()> {
    let label = describe(cmd);
    let status = cmd
        .status()
        .map_err(|error| format!("cannot run {label}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{label}: {status}"))
    }
}

/// stdout as a string; stderr still reaches the terminal.
pub fn capture(cmd: &mut Command) -> Res<String> {
    let label = describe(cmd);
    let output = cmd
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("cannot run {label}: {error}"))?;
    if !output.status.success() {
        return Err(format!("{label}: {}", output.status));
    }
    String::from_utf8(output.stdout).map_err(|_| format!("{label}: output is not UTF-8"))
}

/// stdout as a string, after writing `input` to stdin.
pub fn feed(cmd: &mut Command, input: &str) -> Res<String> {
    let label = describe(cmd);
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("cannot run {label}: {error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| format!("{label}: no stdin"))?
        .write_all(input.as_bytes())
        .map_err(|error| format!("{label}: writing stdin: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("{label}: {error}"))?;
    if !output.status.success() {
        return Err(format!("{label}: {}", output.status));
    }
    String::from_utf8(output.stdout).map_err(|_| format!("{label}: output is not UTF-8"))
}

/// `name: start …` / `name: done elapsed=Ns …`, the progress shape the deploy
/// log has always had.
pub struct Step {
    name: &'static str,
    t0: Instant,
}

impl Step {
    pub fn start(name: &'static str, detail: &str) -> Self {
        println!("{name}: start {detail}");
        Self {
            name,
            t0: Instant::now(),
        }
    }

    pub fn done(self, detail: &str) {
        let elapsed = self.t0.elapsed().as_secs();
        let line = format!("{}: done elapsed={elapsed}s {detail}", self.name);
        println!("{}", line.trim_end());
    }
}

/// An ssh target that has already been checked to be `user@host`.
pub struct Ssh {
    target: String,
}

impl Ssh {
    pub fn new(target: &str) -> Res<Self> {
        let Some((user, host)) = target.split_once('@') else {
            return Err(format!("deploy host must be user@host (got {target:?})"));
        };
        let user_ok = !user.is_empty()
            && user
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        if !user_ok || !is_hostish(host) {
            return Err(format!("deploy host must be user@host (got {target:?})"));
        }
        Ok(Self {
            target: target.to_string(),
        })
    }

    /// The bare host, for URLs and for the certificate SAN.
    pub fn host(&self) -> &str {
        self.target.split_once('@').map_or("", |(_, host)| host)
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    fn command(&self, stdin_closed: bool) -> Command {
        let mut cmd = Command::new("ssh");
        if stdin_closed {
            cmd.arg("-n");
        }
        cmd.args(SSH_BASE).arg(&self.target);
        cmd
    }

    /// Run `script` remotely with stdin closed. Every caller that is inside a
    /// loop over a list needs this: ssh without `-n` eats the rest of the list.
    pub fn run(&self, script: &str) -> Res<()> {
        run(self.command(true).arg(script))
    }

    /// Same, but the exit status is the answer rather than a failure.
    pub fn try_run(&self, script: &str) -> Res<bool> {
        let mut cmd = self.command(true);
        cmd.arg(script);
        let label = describe(&cmd);
        let status = cmd
            .status()
            .map_err(|error| format!("cannot run {label}: {error}"))?;
        Ok(status.success())
    }

    pub fn capture(&self, script: &str) -> Res<String> {
        capture(self.command(true).arg(script))
    }

    /// stdout of a remote script fed from a local string.
    pub fn feed(&self, script: &str, input: &str) -> Res<String> {
        feed(self.command(false).arg(script), input)
    }

    pub fn rsync(&self, extra: &[&str], local: &Path, remote: &str) -> Res<()> {
        let mut cmd = Command::new("rsync");
        cmd.arg("-a").arg(RSYNC_TIMEOUT).args(extra).arg(local);
        cmd.arg(format!("{}:{remote}", self.target));
        run(&mut cmd)
    }
}

fn is_hostish(host: &str) -> bool {
    !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
}

/// `$IW4L_DEPLOY_ROOT`: absolute, no traversal, nothing that needs quoting in
/// the remote shell snippets that interpolate it.
pub fn check_deploy_root(root: &str) -> Res<()> {
    let simple = root.starts_with('/')
        && !root.contains("..")
        && root
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'-'));
    if simple {
        Ok(())
    } else {
        Err(format!(
            "IW4L_DEPLOY_ROOT must be a simple absolute path (got {root:?})"
        ))
    }
}
