use std::sync::Arc;

use bevy::input::gamepad::{GamepadRumbleIntensity, GamepadRumbleRequest};
use bevy::prelude::*;
use net::{LocalPresentClient, PresentedSnapshot};

#[derive(Clone, Debug)]
pub(crate) struct Rumble {
    duration_ms: i32,
    low: Vec<(f32, f32)>,
    high: Vec<(f32, f32)>,
    range: f32,
    fade_with_distance: bool,
    broadcast: bool,
}

impl Rumble {
    pub(crate) fn prepare(
        bank: &asset_audio::SoundCatalog,
        namespace: asset_core::AssetNamespace,
        name: &str,
    ) -> Result<Arc<Self>, String> {
        let text = |name: &str| -> Result<&str, String> {
            let key = format!("rumble/{name}");
            let bytes = bank
                .rawfile_bytes_in(namespace, &key)
                .ok_or_else(|| format!("missing {key}"))?;
            std::str::from_utf8(bytes).map_err(|_| format!("invalid text in {key}"))
        };
        let definition = text(name)?.trim_end_matches('\0');
        let mut fields = definition.split('\\');
        if fields.next() != Some("RUMBLE") {
            return Err(format!("invalid rumble header: {name}"));
        }
        let mut duration = None;
        let mut low = None;
        let mut high = None;
        let mut range = 0.0;
        let mut fade_with_distance = false;
        let mut broadcast = false;
        while let Some(key) = fields.next() {
            let value = fields
                .next()
                .ok_or_else(|| format!("missing {key} value: {name}"))?;
            match key {
                "duration" => duration = value.parse::<f32>().ok(),
                "lowRumbleFile" => low = Some(value),
                "highRumbleFile" => high = Some(value),
                "range" => range = value.parse::<f32>().map_err(|_| "invalid rumble range")?,
                "fadeWithDistance" => {
                    fade_with_distance = value
                        .parse::<i32>()
                        .map_err(|_| "invalid rumble distance flag")?
                        != 0
                }
                "broadcast" => {
                    broadcast = value
                        .parse::<i32>()
                        .map_err(|_| "invalid rumble broadcast flag")?
                        != 0
                }
                _ => {}
            }
        }
        let duration = duration
            .filter(|d| d.is_finite() && *d > 0.0)
            .ok_or_else(|| format!("invalid rumble duration: {name}"))?;
        if !range.is_finite() || range < 0.0 || (broadcast && fade_with_distance && range <= 0.0) {
            return Err(format!("invalid rumble range: {name}"));
        }
        let graph = |name: Option<&str>| -> Result<Vec<(f32, f32)>, String> {
            let name = name
                .filter(|name| !name.is_empty())
                .ok_or_else(|| "missing rumble graph name".to_owned())?;
            let mut tokens = text(name)?.trim_end_matches('\0').split_whitespace();
            if tokens.next() != Some("RUMBLEGRAPHFILE") {
                return Err(format!("invalid rumble graph header: {name}"));
            }
            let count = tokens
                .next()
                .and_then(|n| n.parse::<usize>().ok())
                .filter(|&n| n > 0 && n <= 16)
                .ok_or_else(|| format!("invalid rumble graph count: {name}"))?;
            let mut points = Vec::with_capacity(count);
            for _ in 0..count {
                let mut number = || {
                    tokens
                        .next()
                        .and_then(|n| n.parse::<f32>().ok())
                        .filter(|v| v.is_finite())
                        .ok_or_else(|| format!("invalid rumble graph point: {name}"))
                };
                let x = number()?;
                let y = number()?;
                if points.last().is_some_and(|&(previous, _)| previous > x) {
                    return Err(format!("unordered rumble graph: {name}"));
                }
                points.push((x, y));
            }
            Ok(points)
        };
        Ok(Arc::new(Self {
            duration_ms: (duration * 1000.0).max(1.0) as i32,
            low: graph(low)?,
            high: graph(high)?,
            range,
            fade_with_distance,
            broadcast,
        }))
    }

    fn intensity(&self, elapsed: i32) -> GamepadRumbleIntensity {
        let fraction = (elapsed as f32 / self.duration_ms as f32).clamp(0.0, 1.0);
        GamepadRumbleIntensity {
            strong_motor: sample(&self.low, fraction),
            weak_motor: sample(&self.high, fraction),
        }
    }
}

fn sample(points: &[(f32, f32)], fraction: f32) -> f32 {
    let mut previous = points[0];
    if fraction <= previous.0 {
        return previous.1.clamp(0.0, 1.0);
    }
    for &next in &points[1..] {
        if fraction <= next.0 {
            let t = if next.0 > previous.0 {
                (fraction - previous.0) / (next.0 - previous.0)
            } else {
                1.0
            };
            return (previous.1 + (next.1 - previous.1) * t).clamp(0.0, 1.0);
        }
        previous = next;
    }
    previous.1.clamp(0.0, 1.0)
}

#[derive(Message)]
pub(crate) struct PlayRumble {
    pub bank_revision: u64,
    pub alias: Option<String>,
    pub rumble: Arc<Rumble>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ScriptSource {
    Entity { number: i32, origin: [f32; 3] },
    Position([f32; 3]),
}

#[derive(Message)]
struct ScriptRumbleRequest {
    revision: u64,
    alias: String,
    source: ScriptSource,
    stop: bool,
    rumble: Option<Arc<Rumble>>,
}

struct ActiveRumble {
    start: i32,
    rumble: Arc<Rumble>,
    script: Option<(String, ScriptSource)>,
}

#[derive(Default)]
struct RumblePlayback {
    owner: Option<(
        frame::WorldGeneration,
        sim::ClientId,
        sim::LifeSequence,
        u64,
    )>,
    last_time: i32,
    active: Vec<ActiveRumble>,
    output: Option<(Entity, GamepadRumbleIntensity)>,
    selected: Option<Entity>,
    next_refresh_ms: i32,
}

fn push_rumble(active: &mut Vec<ActiveRumble>, request: ActiveRumble) {
    if active.len() >= 32
        && let Some((index, _)) = active
            .iter()
            .enumerate()
            .min_by_key(|(_, row)| row.start.saturating_add(row.rumble.duration_ms))
    {
        active.swap_remove(index);
    }
    active.push(request);
}

fn canonical_alias(alias: &str) -> String {
    alias.strip_prefix("iw4:").unwrap_or(alias).to_owned()
}

fn script_rumble(
    event: On<net::EntityRumble>,
    adopted: Option<Res<net::LastAdoptedSnapshot>>,
    bank: Option<Res<crate::SoundBank>>,
    mut requests: MessageWriter<ScriptRumbleRequest>,
    mut gaps: ResMut<crate::MissingAliasGaps>,
) {
    let (Some(snapshot), Some(bank)) = (
        adopted.and_then(|s| s.next_snap.clone().or_else(|| s.snap.clone())),
        bank,
    ) else {
        return;
    };
    let record = event.event;
    let Some((_, alias)) = snapshot
        .meta
        .objectives
        .rumble_aliases
        .iter()
        .find(|(index, _)| *index == record.payload.event_parm)
    else {
        gaps.record(&format!("rumble index {}", record.payload.event_parm));
        return;
    };
    let source = if record.event == entity_iw4::EntityEventKind::PLAY_RUMBLE_ON_POS {
        ScriptSource::Position(record.payload.origin)
    } else {
        ScriptSource::Entity {
            number: record.payload.number,
            origin: record.payload.origin,
        }
    };
    let stop = record.event == entity_iw4::EntityEventKind::STOP_RUMBLE;
    let (namespace, name) = alias
        .split_once(':')
        .and_then(|(ns, name)| asset_core::AssetNamespace::parse(ns).map(|ns| (ns, name)))
        .unwrap_or((asset_core::AssetNamespace::Iw4, alias.as_str()));
    let rumble = if stop {
        None
    } else {
        match Rumble::prepare(&bank.0, namespace, name) {
            Ok(rumble) => Some(rumble),
            Err(error) => {
                gaps.record(&format!("rumble {alias}: {error}"));
                return;
            }
        }
    };
    requests.write(ScriptRumbleRequest {
        revision: bank.0.revision(),
        alias: canonical_alias(alias),
        source,
        stop,
        rumble,
    });
}

fn spatial_gain(
    rumble: &Rumble,
    source: ScriptSource,
    local: sim::ClientId,
    receiver: [f32; 3],
    entity_origin: impl FnOnce(i32) -> Option<[f32; 3]>,
) -> f32 {
    if !rumble.broadcast {
        return match source {
            ScriptSource::Entity { number, .. } if u32::try_from(number).ok() != Some(local.0) => {
                0.0
            }
            _ => 1.0,
        };
    }
    let origin = match source {
        ScriptSource::Entity { number, .. } if u32::try_from(number).ok() == Some(local.0) => {
            receiver
        }
        ScriptSource::Entity { number, origin } => entity_origin(number).unwrap_or(origin),
        ScriptSource::Position(origin) => origin,
    };
    let distance = receiver
        .iter()
        .zip(origin)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f32>()
        .sqrt();
    if !distance.is_finite() || distance > rumble.range {
        return 0.0;
    }
    if rumble.fade_with_distance {
        (1.0 - distance / rumble.range).clamp(0.0, 1.0)
    } else {
        1.0
    }
}

pub(crate) fn register(app: &mut App) {
    app.add_message::<PlayRumble>()
        .add_message::<ScriptRumbleRequest>()
        .add_observer(script_rumble)
        .add_systems(
            Update,
            update
                .in_set(net::ClientSet::Effects)
                .after(crate::entity_events::play_viewmodel_notetrack_messages),
        );
}

fn update(
    mut requests: MessageReader<PlayRumble>,
    mut scripted: MessageReader<ScriptRumbleRequest>,
    entities: Query<(&net::CEntity, &Transform)>,
    bank: Option<Res<crate::SoundBank>>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    generation: Res<frame::WorldGeneration>,
    time: Res<Time>,
    devices: Res<frame::InputDevices>,
    mut tests: MessageReader<frame::TestControllerRumble>,
    gamepads: Query<Entity, With<Gamepad>>,
    mut state: Local<RumblePlayback>,
    mut output: MessageWriter<GamepadRumbleRequest>,
    settings: Res<frame::GameSettings>,
    active: Option<Res<frame::ActivePad>>,
) {
    let now = (time.elapsed_secs_f64() * 1000.0) as i32;
    let selected = active
        .and_then(|active| active.0)
        .filter(|entity| gamepads.contains(*entity));
    if state.selected != selected {
        state.active.clear();
        if let Some((gamepad, _)) = state.output.take() {
            output.write(GamepadRumbleRequest::Stop { gamepad });
        }
        state.selected = selected;
        state.next_refresh_ms = 0;
    }
    if !settings.pad_vibration || !devices.focused || selected.is_none() {
        requests.clear();
        scripted.clear();
        tests.clear();
        state.active.clear();
        if let Some((gamepad, _)) = state.output.take() {
            output.write(GamepadRumbleRequest::Stop { gamepad });
        }
        return;
    }
    let owner = bank.as_ref().and_then(|bank| {
        let meta = presented.snapshot()?.meta.for_client(local.0)?;
        (meta.lifecycle == sim::ClientLifecycle::Alive).then_some((
            *generation,
            local.0,
            meta.life_sequence,
            bank.0.revision(),
        ))
    });
    if owner != state.owner || now < state.last_time {
        state.active.clear();
        if let Some((gamepad, _)) = state.output.take() {
            output.write(GamepadRumbleRequest::Stop { gamepad });
        }
        state.owner = owner;
    }
    state.last_time = now;
    state
        .active
        .retain(|row| now.saturating_sub(row.start) < row.rumble.duration_ms);
    for request in requests.read() {
        if owner.is_none_or(|owner| owner.3 != request.bank_revision) {
            continue;
        }
        push_rumble(
            &mut state.active,
            ActiveRumble {
                start: now,
                rumble: Arc::clone(&request.rumble),
                script: request.alias.as_ref().map(|alias| {
                    (
                        canonical_alias(alias),
                        ScriptSource::Entity {
                            number: local.0.0 as i32,
                            origin: [0.0; 3],
                        },
                    )
                }),
            },
        );
    }
    for request in scripted.read() {
        if owner.is_none_or(|owner| owner.3 != request.revision) {
            continue;
        }
        if request.stop {
            state.active.retain(|row| !row.script.as_ref().is_some_and(|(alias, source)|
                alias == &request.alias && matches!((*source, request.source),
                    (ScriptSource::Entity { number: a, .. }, ScriptSource::Entity { number: b, .. }) if a == b)));
        } else if let Some(rumble) = &request.rumble {
            push_rumble(
                &mut state.active,
                ActiveRumble {
                    start: now,
                    rumble: Arc::clone(rumble),
                    script: Some((request.alias.clone(), request.source)),
                },
            );
        }
    }
    if tests.read().next().is_some() {
        push_rumble(
            &mut state.active,
            ActiveRumble {
                start: now,
                script: None,
                rumble: Arc::new(Rumble {
                    duration_ms: 400,
                    low: vec![(0.0, 0.6), (1.0, 0.0)],
                    high: vec![(0.0, 0.4), (1.0, 0.0)],
                    range: 0.0,
                    fade_with_distance: false,
                    broadcast: false,
                }),
            },
        );
        tests.clear();
    }
    let mut intensity = GamepadRumbleIntensity {
        strong_motor: 0.0,
        weak_motor: 0.0,
    };
    let receiver = presented
        .player(local.0)
        .map(|ps| ps.origin)
        .unwrap_or([0.0; 3]);
    for row in &state.active {
        let mut current = row.rumble.intensity(now.saturating_sub(row.start));
        if let Some((_, source)) = &row.script {
            let gain = spatial_gain(&row.rumble, *source, local.0, receiver, |number| {
                entities
                    .iter()
                    .find(|(identity, _)| i32::from(identity.number()) == number)
                    .map(|(_, transform)| crate::space::transform_inches(transform.translation))
            });
            current.strong_motor *= gain;
            current.weak_motor *= gain;
        }
        intensity.strong_motor = intensity.strong_motor.max(current.strong_motor);
        intensity.weak_motor = intensity.weak_motor.max(current.weak_motor);
    }
    if let Some(gamepad) = selected {
        let zero = intensity.strong_motor == 0.0 && intensity.weak_motor == 0.0;
        if zero {
            if state.output.take().is_some() {
                output.write(GamepadRumbleRequest::Stop { gamepad });
            }
            state.next_refresh_ms = 0;
        } else if state.output.is_none() || now >= state.next_refresh_ms {
            output.write(GamepadRumbleRequest::Stop { gamepad });
            output.write(GamepadRumbleRequest::Add {
                gamepad,
                duration: std::time::Duration::from_millis(160),
                intensity,
            });
            state.output = Some((gamepad, intensity));
            state.next_refresh_ms = now.saturating_add(80);
        }
    }
}
