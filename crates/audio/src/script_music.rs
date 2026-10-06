use crate::aliases::namespace_alias;
use crate::cue::CueHandle;
use crate::cue_execution::CueTrigger;
use crate::{SoundBank, SoundClass};
use bevy::prelude::*;

#[derive(Resource, Default)]
pub(crate) struct ScriptMusicPlayback {
    epoch: u64,
    pending: Option<String>,
    active: Option<CueHandle>,
}

pub(crate) fn update_script_music(
    epoch: Res<crate::backend::MatchEpoch>,
    ready: Res<crate::AudioReady>,
    generation: Res<frame::WorldGeneration>,
    runtime: Res<crate::AudioRuntime>,
    loading: Option<Res<assets::LoadingScreen>>,
    mut events: MessageReader<net::SvcScriptAudio>,
    bank: Option<Res<SoundBank>>,
    mut playback: ResMut<ScriptMusicPlayback>,
    mut mix: ResMut<crate::script_mix::ScriptAudioMix>,
    mut feedback: ResMut<crate::clip_store::CueFeedback>,
) {
    mix.reset_epoch(epoch.0);
    if playback.epoch != epoch.0 {
        playback.epoch = epoch.0;
        playback.pending = None;
        playback.active = None;
    }
    if playback
        .active
        .as_ref()
        .is_some_and(|handle| !handle.active())
    {
        playback.active = None;
    }
    for event in events.read() {
        match &event.0 {
            sim::ScriptAudioCommand::ChannelVolumes { .. }
            | sim::ScriptAudioCommand::DeactivateChannelVolumes { .. } => {}
            sim::ScriptAudioCommand::MusicPlay(alias) => {
                if playback.pending.is_some() || playback.active.is_some() {
                    diag::warn!(
                        Audio,
                        "audio: music transport is busy; alias {alias} was not started"
                    );
                    continue;
                }
                playback.pending = Some(alias.clone());
            }
            sim::ScriptAudioCommand::SoundFade { volume, fade_ms } => {
                mix.fade(runtime.audio_frame(), *volume, *fade_ms);
            }
            sim::ScriptAudioCommand::MusicStop { fade_ms } => {
                playback.pending = None;
                if let Some(handle) = &playback.active {
                    handle.release(
                        runtime.audio_frame(),
                        (*fade_ms).max(0) as u64 * u64::from(crate::render_core::SAMPLE_RATE)
                            / 1000,
                    );
                }
                if *fade_ms <= 0 {
                    playback.active = None;
                }
            }
        }
    }
    if !ready.0.ready_for(*generation) || loading.is_some_and(|screen| !screen.is_complete()) {
        return;
    }
    let Some(bank) = bank else { return };
    let Some(alias) = playback.pending.take() else {
        return;
    };
    let (namespace, name) = namespace_alias(&alias);
    let handle = runtime.trigger_cue(CueTrigger {
        event: None,
        bank: bank.0.clone(),
        namespace,
        alias: name.into(),
        bound: None,
        origin_inches: None,
        emitter: None,
        class: SoundClass::Music,
        epoch: epoch.0,
        pitch_scale: 1.0,
        fallbacks: Vec::new(),
    });
    feedback.push(CueHandle(handle.0.clone()));
    playback.active = Some(handle);
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<ScriptMusicPlayback>()
        .add_systems(Update, update_script_music.in_set(net::ClientSet::Effects));
}
