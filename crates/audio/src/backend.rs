use bevy::prelude::*;
use frame::ClientSet;

pub(crate) use crate::render_core::AudioScope;
use crate::runtime::AudioRuntime;

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MatchEpoch(pub u64);

impl MatchEpoch {
    pub fn bump(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<net::FireVerdictState>();
    let verdicts = app.world().resource::<net::FireVerdictState>().clone();
    app.init_resource::<AudioRuntime>();
    app.world()
        .resource::<AudioRuntime>()
        .set_fire_verdicts(verdicts);
    app.init_resource::<MatchEpoch>()
        .add_systems(
            Update,
            publish_audio_context
                .in_set(ClientSet::Predict)
                .after(frame::OwnerEventsPublished),
        )
        .add_systems(
            PostUpdate,
            (
                submit_presented_audio,
                cancel_audio_on_exit.after(submit_presented_audio),
            ),
        );
}

pub(crate) fn publish_audio_context(
    epoch: Res<MatchEpoch>,
    runtime: Res<AudioRuntime>,
    verdicts: Option<Res<net::FireVerdictState>>,
    clips: Option<Res<crate::ClipStore>>,
    mut mix: Option<ResMut<crate::script_mix::ScriptAudioMix>>,
    mut channels: Option<ResMut<crate::script_mix::ChannelAudioMix>>,
    listeners: Query<&Transform, With<crate::AmbientListener>>,
    generation: Option<Res<frame::WorldGeneration>>,
    presented: Option<Res<net::PresentedSnapshot>>,
    events: Option<Res<net::EntityEventCursor>>,
    local: Option<Res<net::LocalPresentClient>>,
    adopted: Option<Res<net::LastAdoptedSnapshot>>,
    prediction: Option<Res<net::ClientPredictionState>>,
    view: Option<Res<frame::ViewSubject>>,
) {
    if let Some(verdicts) = verdicts {
        runtime.set_fire_verdicts(verdicts.clone());
    }
    runtime.set_match_epoch(epoch.0);
    runtime.set_event_context(
        generation
            .and_then(|generation| generation.0)
            .zip(presented.as_ref().and_then(|presented| {
                presented.snapshot().map(|snapshot| {
                    if view.as_ref().is_some_and(|view| view.in_killcam()) {
                        snapshot.tick.0
                    } else {
                        adopted
                            .as_ref()
                            .and_then(|adopted| adopted.next())
                            .map_or(snapshot.tick.0, |latest| latest.tick.0.max(snapshot.tick.0))
                    }
                })
            }))
            .zip(events.as_ref().map(|events| events.timeline()))
            .map(|((world, tick), timeline)| crate::event::EventContext {
                local_life: local.as_ref().and_then(|local| {
                    let snapshot = if view.as_ref().is_some_and(|view| view.in_killcam()) {
                        presented.as_ref()?.snapshot()?
                    } else {
                        adopted
                            .as_ref()
                            .and_then(|adopted| adopted.next())
                            .or_else(|| presented.as_ref()?.snapshot())?
                    };
                    let meta = snapshot.meta.for_client(local.0)?;
                    (meta.lifecycle == sim::ClientLifecycle::Alive)
                        .then_some((local.0.0, meta.life_sequence.0))
                }),
                world,
                timeline,
                tick,
                owner_tick: prediction
                    .as_ref()
                    .and_then(|prediction| prediction.0.history().newest())
                    .map(|latest| latest.tick.0),
            }),
    );
    runtime.set_media_service(clips.as_ref().map(|clips| clips.service()));
    if let Some(mix) = mix.as_mut() {
        mix.reset_epoch(epoch.0);
    }
    if let Some(channels) = channels.as_mut() {
        channels.reset_epoch(epoch.0);
    }
    runtime.set_cue_mix(
        mix.zip(channels)
            .map(|(mix, channels)| crate::cue_execution::CueMix {
                epoch: epoch.0,
                gain: mix.gain.clone(),
                channels: channels.bindings(),
            }),
    );
    let mut listener = listeners.iter();
    let pose = listener.next();
    assert!(listener.next().is_none(), "more than one ambient listener");
    runtime.set_listener(pose.map(|pose| crate::spatial::ListenerSnapshot {
        origin_inches: crate::transform_inches(pose.translation),
        right: (pose.rotation * Vec3::X).to_array(),
    }));
}

fn cancel_audio_on_exit(mut exit: MessageReader<AppExit>, runtime: Res<AudioRuntime>) {
    if exit.read().count() == 0 {
        return;
    }
    runtime.cancel_all();
    diag::info!(
        Audio,
        "audio: admissions queue_full={} logical_budget={} physical_budget={} concurrency={} cancelled={} stale_scope={} output_unavailable={}",
        runtime.rejection_count(crate::AdmissionFailure::QueueFull),
        runtime.rejection_count(crate::AdmissionFailure::LogicalBudget),
        runtime.rejection_count(crate::AdmissionFailure::PhysicalBudget),
        runtime.rejection_count(crate::AdmissionFailure::Concurrency),
        runtime.rejection_count(crate::AdmissionFailure::Cancelled),
        runtime.rejection_count(crate::AdmissionFailure::StaleScope),
        runtime.rejection_count(crate::AdmissionFailure::OutputUnavailable)
    );
    let stats = runtime.diagnostics();
    diag::info!(
        Audio,
        "audio: exit frames={} device_blocks={} null_blocks={} busy_blocks={} underruns={} peak={:.6} source_revision={} sources={} source_pending_layers={} source_rendered={} source_virtual={} source_dropped={}",
        stats.audio_frame,
        stats.device_blocks,
        stats.null_blocks,
        stats.busy_blocks,
        stats.device_underruns,
        stats.peak,
        stats.source_revision,
        stats.logical_sources,
        stats.pending_source_layers,
        stats.rendered_sources,
        stats.virtual_sources,
        stats.dropped_sources
    );
}

fn submit_presented_audio(
    mut runtime: ResMut<AudioRuntime>,
    epoch: Res<MatchEpoch>,
    settings: Option<Res<frame::GameSettings>>,
    listeners: Query<&Transform, With<crate::AmbientListener>>,
    destructibles: Option<Res<crate::destructible_loops::DestructibleSources>>,
    map: Option<Res<crate::ambient::MapSources>>,
    menu: Option<Res<crate::frontend::MenuSources>>,
    shellshock: Option<Res<crate::shellshock::ShellshockSources>>,
    breath: Option<Res<crate::breath::BreathSources>>,
) {
    runtime.set_match_epoch(epoch.0);
    let mut listener = listeners.iter();
    let pose = listener.next();
    assert!(listener.next().is_none(), "more than one ambient listener");
    runtime.set_listener(pose.map(|pose| crate::spatial::ListenerSnapshot {
        origin_inches: crate::transform_inches(pose.translation),
        right: (pose.rotation * Vec3::X).to_array(),
    }));
    if let Some(settings) = settings {
        runtime.set_master_volume(settings.master_volume);
    }
    let mut desired = Vec::new();
    if let Some(destructibles) = destructibles {
        desired.extend(destructibles.desired.iter().cloned());
    }
    if let Some(map) = map {
        desired.extend(map.desired.iter().cloned());
    }
    if let Some(menu) = menu {
        desired.extend(menu.source.iter().cloned());
    }
    if let Some(shellshock) = shellshock {
        desired.extend(shellshock.source.iter().cloned());
    }
    if let Some(breath) = breath {
        desired.extend(breath.source.iter().cloned());
    }
    runtime.set_sources(desired);
}
