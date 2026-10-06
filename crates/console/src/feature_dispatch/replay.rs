use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ::replay::{
    CLIP_DEMO_FILE, CLIP_DUMP_FILE, CLIP_MANIFEST_FILE, CLIP_MS, ClipRing, MatchRecordIdentity,
    Recording, ReplaySession,
};
use bevy::prelude::*;
use frame::{LaunchIdentity, RuntimeRole};
use net::{AuthorityClock, AuthorityInputGate, AuthorityWorld, PresentedSnapshot};
use render_frontend::prepare::scene::camera::SimCamera;

use crate::{ConsoleCommand, ConsoleDispatch, ConsoleLine, ConsoleSettings, ConsoleState};

use super::state_dump::{audio_dump_section, persist_bytes_atomic, state_dump_body};

pub(crate) fn route_replay_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut output: (
        ResMut<ConsoleState>,
        Res<ConsoleSettings>,
        ResMut<ConsoleLine>,
    ),
    identity: Option<Res<LaunchIdentity>>,
    mut recorder: ResMut<ReplaySession>,
    audio: (
        Option<Res<audio::AudioReady>>,
        Option<Res<audio::StartDecisions>>,
        Option<Res<audio::MissingAliasGaps>>,
        Option<Res<audio::ClipStore>>,
        Option<Res<audio::AudioRuntime>>,
    ),
    replay_inputs: (
        Option<Res<AuthorityWorld>>,
        Res<SimCamera>,
        Res<AuthorityInputGate>,
    ),
    mut lifecycle: (
        ResMut<::replay::PendingReplayArm>,
        ResMut<::session::SessionSwapRequest>,
        ResMut<ConsoleDispatch>,
    ),
    clip: (
        Res<ClipRing>,
        Res<RuntimeRole>,
        Res<net::ClientPredictionState>,
        Option<Res<AuthorityClock>>,
        Option<Res<PresentedSnapshot>>,
    ),
) {
    let (console, settings, line) = &mut output;
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };
    let (authority, sim_cam, input_gate) = replay_inputs;
    let (theater, map_transition, dispatch) = &mut lifecycle;
    let (ring, role, prediction, clip_clock, clip_presented) = clip;

    for cmd in events.read() {
        match cmd.name.as_str() {
            "demo" | "play" => match cmd.args.as_slice() {
                [name] => {
                    let Some(identity) = identity.as_ref() else {
                        dispatch.release();
                        echo(
                            "demo: launch identity missing (artifacts path unknown)".into(),
                            console,
                            line,
                        );
                        continue;
                    };
                    if let Some(open) = recorder.0.take() {
                        match open.stop() {
                            Ok((ticks, path)) => echo(
                                format!(
                                    "demo: stoprecord {ticks} ticks in {} before playback",
                                    path.display()
                                ),
                                console,
                                line,
                            ),
                            Err(error) => {
                                echo(format!("demo: stoprecord failed: {error}"), console, line)
                            }
                        }
                    }
                    match ::replay::Playback::open(&identity.artifacts, name) {
                        Ok(playback) => {
                            let Some(zone) = playback.identity().zone_name().map(str::to_owned)
                            else {
                                dispatch.release();
                                echo(
                                    format!(
                                        "demo: `{name}` has no zone in the header — re-record, or: make play {name} ZONE=<map>"
                                    ),
                                    console,
                                    line,
                                );
                                continue;
                            };
                            theater.playback = Some(playback);
                            theater.quit_on_end = false;
                            match map_transition.request_demo(name.clone(), zone.clone(), false) {
                                Ok(id) => echo(
                                    format!("demo: `{name}` zone `{zone}` (swap #{id})"),
                                    console,
                                    line,
                                ),
                                Err(error) => {
                                    theater.playback = None;
                                    dispatch.release();
                                    echo(format!("demo: {error}"), console, line);
                                }
                            }
                        }
                        Err(error) => {
                            dispatch.release();
                            echo(format!("demo: {error}"), console, line);
                        }
                    }
                }
                _ => {
                    dispatch.release();
                    echo("usage: demo <name>".into(), console, line);
                }
            },

            "record" => {
                if cmd.args.len() > 1 {
                    echo("usage: record [name]".into(), console, line);
                    continue;
                }
                if let Some(open) = recorder.0.as_ref() {
                    echo(
                        format!(
                            "record: already recording `{}` ({} ticks) — stoprecord first",
                            open.name(),
                            open.ticks()
                        ),
                        console,
                        line,
                    );
                    continue;
                }
                if !sim_cam.enabled || !input_gate.local_cmds_enabled {
                    echo(
                        "record: the simulation does not own the camera yet \
                         (no clip brushes, or the authored intermission view is active); \
                         there are no ticks to record"
                            .into(),
                        console,
                        line,
                    );
                    continue;
                }
                let Some(identity) = identity.as_ref() else {
                    echo(
                        "record: launch identity missing (artifacts path unknown)".into(),
                        console,
                        line,
                    );
                    continue;
                };
                let Some(authority) = authority.as_ref() else {
                    echo(
                        "record: authority world missing (match identity unavailable)".into(),
                        console,
                        line,
                    );
                    continue;
                };
                let requested = cmd.args.first().map(String::as_str).unwrap_or("");
                let record_identity =
                    MatchRecordIdentity::from_world_on_zone(&authority.0, &identity.zone);
                match Recording::start_with_identity(
                    &identity.artifacts,
                    requested,
                    record_identity,
                ) {
                    Ok(open) => {
                        echo(
                            format!("record: writing {}", open.path().display()),
                            console,
                            line,
                        );
                        recorder.0 = Some(open);
                    }
                    Err(error) => echo(format!("record: {error}"), console, line),
                }
            }

            "stoprecord" => {
                let Some(open) = recorder.0.take() else {
                    echo("stoprecord: not recording".into(), console, line);
                    continue;
                };
                match open.stop() {
                    Ok((ticks, path)) => echo(
                        format!("stoprecord: {ticks} ticks in {}", path.display()),
                        console,
                        line,
                    ),
                    Err(error) => echo(format!("stoprecord: {error}"), console, line),
                }
            }
            "clip" => {
                if parse_clip_args(&cmd.args).is_err() {
                    echo("usage: clip".into(), console, line);
                    continue;
                }
                if ring.is_empty() {
                    echo("clip: ring empty (0 ticks)".into(), console, line);
                    continue;
                }
                let Some(identity) = identity.as_ref() else {
                    echo(
                        "clip: launch identity missing (artifacts path unknown)".into(),
                        console,
                        line,
                    );
                    continue;
                };
                let audio_section = audio_dump_section(
                    audio.0.as_deref(),
                    audio.1.as_deref(),
                    audio.2.as_deref(),
                    audio.3.as_deref(),
                    audio.4.as_deref(),
                );
                match save_clip_package(
                    identity,
                    authority.as_deref().filter(|_| role.runs_authority()),
                    prediction.0.world(),
                    ring.as_ref(),
                    clip_clock.as_deref().filter(|_| role.runs_authority()),
                    clip_presented.as_deref(),
                    &audio_section,
                ) {
                    Ok(saved) => {
                        echo(
                            format!(
                                "clip: {}  {:.2}s ({} ticks) of {}s",
                                saved.id,
                                saved.duration_ms as f32 / 1000.0,
                                saved.ticks,
                                CLIP_MS / 1000
                            ),
                            console,
                            line,
                        );
                        echo(
                            format!("clip: demo → {}", saved.demo_path.display()),
                            console,
                            line,
                        );
                        echo(
                            format!("clip: dump → {}", saved.dump_path.display()),
                            console,
                            line,
                        );
                    }
                    Err(error) => echo(format!("clip: {error}"), console, line),
                }
            }
            _ => {}
        }
    }
}

fn parse_clip_args(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        Ok(())
    } else {
        Err("usage: clip".into())
    }
}

struct SavedClip {
    id: String,
    ticks: u64,
    duration_ms: u32,
    demo_path: PathBuf,
    dump_path: PathBuf,
}

fn allocate_clip_dir(artifacts: &Path) -> Result<(String, PathBuf), String> {
    for _ in 0..8 {
        let id = ::replay::new_ulid().map_err(|error| error.to_string())?;
        match ::replay::create_clip_dir(artifacts, &id) {
            Ok(dir) => return Ok((id, dir)),
            Err(::replay::ReplayError::Io(error)) if error.kind() == ErrorKind::AlreadyExists => {
                continue;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("could not allocate a unique ULID directory".into())
}

fn save_clip_package(
    identity: &LaunchIdentity,
    world: Option<&AuthorityWorld>,
    prediction: &sim::SimWorld,
    ring: &ClipRing,
    authority_clock: Option<&AuthorityClock>,
    presented: Option<&PresentedSnapshot>,
    audio: &str,
) -> Result<SavedClip, String> {
    let (id, dir) = allocate_clip_dir(&identity.artifacts)?;
    let captured_unix_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before UNIX epoch: {error}"))?
        .as_nanos();
    let result = (|| {
        let demo_path = dir.join(CLIP_DEMO_FILE);
        let dump_path = dir.join(CLIP_DUMP_FILE);
        let record_identity = MatchRecordIdentity::from_world_on_zone(
            world.map(|world| &world.0).unwrap_or(prediction),
            &identity.zone,
        );
        let mut recording = Recording::start_at_path(demo_path.clone(), record_identity)
            .map_err(|error| error.to_string())?;
        recording
            .record_clip_ring(ring)
            .map_err(|error| error.to_string())?;
        let (ticks, _) = recording.stop().map_err(|error| error.to_string())?;
        let body = state_dump_body(
            identity,
            captured_unix_ns,
            authority_clock,
            world,
            presented,
            Some(audio),
        );
        persist_bytes_atomic(&dump_path, &body)?;
        let mut manifest = ::replay::clip_manifest(
            &id,
            ticks,
            ring.duration_ms(),
            &identity.zone,
            captured_unix_ns,
        );
        let source = if world.is_some() {
            "authority"
        } else {
            "received"
        };
        manifest.push_str(&format!(
            "source = {source:?}\nrole = {:?}\n",
            identity.role_label
        ));
        persist_bytes_atomic(&dir.join(CLIP_MANIFEST_FILE), &manifest)?;
        ::replay::rewrite_latest_symlink(&identity.artifacts, &id)
            .map_err(|error| error.to_string())?;
        Ok(SavedClip {
            id,
            ticks,
            duration_ms: ring.duration_ms(),
            demo_path,
            dump_path,
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    result
}
