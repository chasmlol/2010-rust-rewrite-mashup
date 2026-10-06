use crate::aliases::namespace_alias;
use crate::ambient::{MAP_BED_SLOT, MapSources};
use crate::cue::CueHandle;
use crate::cue_execution::CueTrigger;
use crate::{SoundBank, SoundClass};
use bevy::prelude::*;

#[derive(Resource, Default)]
pub(crate) struct ScriptAmbientPlayback {
    epoch: u64,
    target: Option<(bool, sim::ScriptAmbient)>,
    started: bool,
    voices: Vec<CueHandle>,
}

pub(crate) fn update_script_ambient(
    epoch: Res<crate::backend::MatchEpoch>,
    ready: Res<crate::AudioReady>,
    generation: Res<frame::WorldGeneration>,
    loading: Option<Res<assets::LoadingScreen>>,
    identity: Option<Res<frame::LaunchIdentity>>,
    presented: Option<Res<net::PresentedSnapshot>>,
    clock: Option<Res<net::FrameClock>>,
    local: Res<net::LocalPresentClient>,
    bank: Option<Res<SoundBank>>,
    runtime: Res<crate::AudioRuntime>,
    mut playback: ResMut<ScriptAmbientPlayback>,
    mut map: Option<ResMut<MapSources>>,
    mut feedback: ResMut<crate::clip_store::CueFeedback>,
) {
    if playback.epoch != epoch.0 {
        playback.epoch = epoch.0;
        playback.target = None;
        playback.started = false;
        playback.voices.clear();
    }
    playback.voices.retain(CueHandle::active);
    if identity.is_some_and(|identity| assets::minecraft_map::is_minecraft_load(&identity.zone)) {
        for voice in playback.voices.drain(..) {
            voice.release(runtime.audio_frame(), 0);
        }
        return;
    }
    if !ready.0.ready_for(*generation) || loading.is_some_and(|screen| !screen.is_complete()) {
        return;
    }
    let Some(presented) = presented else { return };
    let Some(snapshot) = presented.snapshot() else {
        return;
    };
    let ac130 = presented
        .player(local.0)
        .is_some_and(|ps| ps.other_flags & playerstate_iw4::other_flags::AC130 != 0);
    let plan = if ac130 {
        &snapshot.meta.objectives.ac130_ambient
    } else {
        &snapshot.meta.objectives.ambient
    };
    let now = runtime.audio_frame();
    let Some(plan) = plan.as_ref().filter(|plan| plan.valid()) else {
        if playback.target.take().is_some() {
            for handle in &playback.voices {
                handle.release(now, 0);
            }
        }
        playback.started = false;
        return;
    };
    let Some(clock) = clock else { return };
    let frames = plan.end_ms.saturating_sub(clock.time()).max(0) as u64
        * u64::from(crate::render_core::SAMPLE_RATE)
        / 1000;
    if let Some(map) = map.as_mut() {
        map.desired.retain(|source| source.key.slot != MAP_BED_SLOT);
    }
    let selected = (ac130, plan.clone());
    if playback.target.as_ref() != Some(&selected) {
        let same_source = playback.target.as_ref().is_some_and(|(mode, old)| {
            *mode == ac130
                && old
                    .alias
                    .as_ref()
                    .zip(plan.alias.as_ref())
                    .is_some_and(|(old, new)| old.eq_ignore_ascii_case(new))
        });
        let retained = same_source
            && playback
                .voices
                .last()
                .is_some_and(|handle| handle.fade_to(now, 1.0, frames));
        if !retained {
            for handle in &playback.voices {
                handle.release(now, frames);
            }
        }
        playback.started = retained;
        playback.target = Some(selected);
    }
    let Some(alias) = plan.alias.as_ref().filter(|_| !playback.started) else {
        return;
    };
    let Some(bank) = bank else { return };
    if playback.voices.len() == 2 {
        let oldest = playback.voices.remove(0);
        oldest.release(now, 0);
    }
    let (namespace, name) = namespace_alias(alias);
    let handle = runtime.trigger_faded_cue(
        CueTrigger {
            event: None,
            bank: bank.0.clone(),
            namespace,
            alias: name.into(),
            bound: None,
            origin_inches: None,
            emitter: None,
            class: SoundClass::Ambience,
            epoch: epoch.0,
            pitch_scale: 1.0,
            fallbacks: Vec::new(),
        },
        frames,
    );
    feedback.push(CueHandle(handle.0.clone()));
    playback.voices.push(handle);
    playback.started = true;
}
