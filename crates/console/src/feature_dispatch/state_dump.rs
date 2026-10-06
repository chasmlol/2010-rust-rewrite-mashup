use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use frame::LaunchIdentity;
use net::{AuthorityClock, AuthorityWorld, PresentedSnapshot};

use crate::ConsoleCommand;

use super::echo::ConsoleEcho;
use super::hitvol::hitvol_report;

pub(crate) fn route_state_dump_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut echo: ConsoleEcho,
    identity: Option<Res<LaunchIdentity>>,
    (authority, authority_clock, presented): (
        Option<Res<AuthorityWorld>>,
        Option<Res<AuthorityClock>>,
        Option<Res<PresentedSnapshot>>,
    ),
    (audio_ready, decisions, gaps, clips, runtime): (
        Option<Res<audio::AudioReady>>,
        Option<Res<audio::StartDecisions>>,
        Option<Res<audio::MissingAliasGaps>>,
        Option<Res<audio::ClipStore>>,
        Option<Res<audio::AudioRuntime>>,
    ),
) {
    for cmd in events.read() {
        if cmd.name != "dump" {
            continue;
        }
        let name = match parse_state_dump_name(&cmd.args) {
            Ok(name) => name,
            Err(error) => {
                echo.write(format!("dump: {error}"));
                continue;
            }
        };
        let audio = audio_dump_section(
            audio_ready.as_deref(),
            decisions.as_deref(),
            gaps.as_deref(),
            clips.as_deref(),
            runtime.as_deref(),
        );
        match write_current_state_dump(
            identity.as_deref(),
            &name,
            authority_clock.as_deref(),
            authority.as_deref(),
            presented.as_deref(),
            Some(&audio),
        ) {
            Ok(path) => echo.write(format!("dump: wrote {}", path.display())),
            Err(error) => echo.write(format!("dump: {error}")),
        }
    }
}

fn parse_state_dump_name(args: &[String]) -> Result<String, String> {
    let raw = match args {
        [] => "snapshot",
        [name] => name.strip_suffix(".txt").unwrap_or(name),
        _ => return Err("usage: dump [name]".into()),
    };
    if raw.is_empty()
        || raw.len() > 96
        || !raw
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        || matches!(raw, "." | "..")
    {
        return Err("name must be 1-96 ASCII letters, digits, '.', '-' or '_'".into());
    }
    Ok(raw.to_owned())
}

pub(super) fn write_current_state_dump(
    identity: Option<&LaunchIdentity>,
    name: &str,
    authority_clock: Option<&AuthorityClock>,
    authority: Option<&AuthorityWorld>,
    presented: Option<&PresentedSnapshot>,
    audio: Option<&str>,
) -> Result<PathBuf, String> {
    let identity = identity.ok_or("launch identity missing (artifacts path unknown)")?;
    let captured_unix_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before UNIX epoch: {error}"))?
        .as_nanos();
    let body = state_dump_body(
        identity,
        captured_unix_ns,
        authority_clock,
        authority,
        presented,
        audio,
    );
    persist_state_dump(&identity.artifacts, name, captured_unix_ns, &body)
}

pub(super) fn audio_dump_section(
    ready: Option<&audio::AudioReady>,
    decisions: Option<&audio::StartDecisions>,
    gaps: Option<&audio::MissingAliasGaps>,
    clips: Option<&audio::ClipStore>,
    runtime: Option<&audio::AudioRuntime>,
) -> String {
    let report = ready.map(|ready| ready.0);
    let ready = report.is_some_and(|report| report.ready_for(report.generation));
    let generation = report.and_then(|report| report.generation.0);
    let state = report.map_or("Missing", |report| match report.state {
        frame::ReadinessState::Pending => "Pending",
        frame::ReadinessState::Ready => "Ready",
        frame::ReadinessState::Silent => "Silent",
        frame::ReadinessState::Failed => "Failed",
    });
    let late = clips.map(audio::ClipStore::late_prepares).unwrap_or(0);
    let mut out = format!(
        "[audio]\nAudioReady = {ready}\naudio_generation = {generation:?}\naudio_state = {state}\nlate_prepares = {late}\n"
    );
    if let Some(runtime) = runtime {
        out.push_str(&format!("transport = {:?}\n", runtime.diagnostics()));
    }
    match gaps {
        Some(gaps) if !gaps.is_empty() => {
            out.push_str("missing_aliases =\n");
            for alias in &gaps.aliases {
                out.push_str("  ");
                out.push_str(alias);
                out.push('\n');
            }
        }
        _ => out.push_str("missing_aliases = []\n"),
    }
    out.push_str("starts =\n");
    match decisions {
        Some(decisions) => {
            let mut n = 0usize;
            for line in decisions.lines() {
                n += 1;
                out.push_str("  ");
                out.push_str(&line);
                out.push('\n');
            }
            if n == 0 {
                out.push_str("  (none)\n");
            }
        }
        None => out.push_str("  Unavailable { reason: \"StartDecisions resource absent\" }\n"),
    }
    out
}

pub(super) fn state_dump_body(
    identity: &LaunchIdentity,
    captured_unix_ns: u128,
    authority_clock: Option<&AuthorityClock>,
    authority: Option<&AuthorityWorld>,
    presented: Option<&PresentedSnapshot>,
    audio: Option<&str>,
) -> String {
    let authority_snapshot = match (authority_clock, authority) {
        (Some(clock), Some(world)) => format!("{:#?}", world.0.snapshot(sim::Tick(clock.tick))),
        (None, Some(_)) => "Unavailable { reason: \"AuthorityClock resource absent\" }".to_owned(),
        (Some(_), None) => "Unavailable { reason: \"AuthorityWorld resource absent\" }".to_owned(),
        (None, None) => {
            "Unavailable { reason: \"AuthorityClock and AuthorityWorld resources absent\" }"
                .to_owned()
        }
    };
    let presented_snapshot = presented
        .map(|snapshot| format!("{snapshot:#?}"))
        .unwrap_or_else(|| {
            "Unavailable { reason: \"PresentedSnapshot resource absent\" }".to_owned()
        });
    let audio =
        audio.unwrap_or("[audio]\nUnavailable { reason: \"not captured with this dump\" }\n");
    let hitvol = match authority {
        Some(world) => {
            let mut out = String::from("[hitvol]\n");
            for line in hitvol_report(&world.0) {
                out.push_str(&line);
                out.push('\n');
            }
            out.push_str("rows =\n");
            for row in world.0.hitvol_dump() {
                out.push_str(&format!("  {row:?}\n"));
            }
            out
        }
        None => "[hitvol]\nUnavailable { reason: \"AuthorityWorld resource absent\" }\n".to_owned(),
    };
    format!(
        "format = \"iw4l-state-dump-1\"\n\
         captured_unix_ns = {captured_unix_ns}\n\
         role = {:?}\n\
         zone = {:?}\n\
         authority_clock = {authority_clock:#?}\n\
         \n[authority_snapshot]\n{authority_snapshot}\n\
         \n[presented_snapshot]\n{presented_snapshot}\n\
         \n{hitvol}\n{audio}",
        identity.role_label, identity.zone,
    )
}

fn persist_state_dump(
    artifacts: &Path,
    name: &str,
    captured_unix_ns: u128,
    body: &str,
) -> Result<PathBuf, String> {
    let directory = artifacts.join("dumps");
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("create {}: {error}", directory.display()))?;
    let file_name = format!("{captured_unix_ns}-{name}.txt");
    let path = directory.join(&file_name);
    persist_bytes_atomic(&path, body)?;
    Ok(path)
}

pub(super) fn persist_bytes_atomic(path: &Path, body: &str) -> Result<(), String> {
    let directory = path
        .parent()
        .ok_or_else(|| format!("{} has no parent", path.display()))?;
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("create {}: {error}", directory.display()))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| format!("{} has no file name", path.display()))?
        .to_string_lossy();
    let temporary = directory.join(format!(".{file_name}.tmp"));
    let result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("create {}: {error}", temporary.display()))?;
        file.write_all(body.as_bytes())
            .map_err(|error| format!("write {}: {error}", temporary.display()))?;
        file.flush()
            .map_err(|error| format!("flush {}: {error}", temporary.display()))?;
        drop(file);
        std::fs::hard_link(&temporary, path).map_err(|error| {
            format!(
                "link {} to {}: {error}",
                temporary.display(),
                path.display()
            )
        })?;
        std::fs::remove_file(&temporary)
            .map_err(|error| format!("remove {}: {error}", temporary.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
