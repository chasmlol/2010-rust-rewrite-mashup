use asset_core::AssetNamespace;
use bevy::prelude::*;
use frame::{AppScreen, LifeEnded, MatchTornDown};
use hud_iw4::shellshock_remaining_ms;
use net::{FrameClock, LocalPresentClient, PresentedSnapshot};

use crate::sources::{DesiredSource, SourceCueRequest, SourceKey};
use crate::{PlayAlias, SND_ENT_LOCAL, playback::SoundBank};

#[derive(Resource, Default)]
pub(crate) struct ShellshockSources {
    pub source: Option<DesiredSource>,
    context: Option<(u64, u32, u32)>,
    next_version: u64,
    was_active: bool,
}

pub(crate) fn update_shellshock_tinnitus(
    mut torn: MessageReader<MatchTornDown>,
    mut died: MessageReader<LifeEnded>,
    screen: Option<Res<AppScreen>>,
    cg_clock: Option<Res<FrameClock>>,
    presented: Option<Res<PresentedSnapshot>>,
    local: Option<Res<LocalPresentClient>>,
    bank: Option<Res<SoundBank>>,
    runtime: Res<crate::AudioRuntime>,
    mut sources: ResMut<ShellshockSources>,
    mut play: MessageWriter<crate::AliasCommand>,
    epoch: Res<crate::backend::MatchEpoch>,
) {
    let torn = torn.read().next().is_some();
    let local_id = local.as_ref().map(|l| l.0.0);
    let life = local.as_ref().and_then(|local| {
        presented
            .as_ref()?
            .snapshot()?
            .meta
            .for_client(local.0)
            .map(|meta| meta.life_sequence.0)
    });
    let died = died
        .read()
        .any(|ev| local_id == Some(ev.client) && life == Some(ev.life));
    if torn || !screen.is_some_and(|s| matches!(*s, AppScreen::InGame)) {
        sources.source = None;
        sources.context = None;
        sources.was_active = false;
        return;
    }
    let (Some(cg_clock), Some(presented), Some(local)) = (cg_clock, presented, local) else {
        sources.source = None;
        sources.context = None;
        sources.was_active = false;
        return;
    };
    let context = (epoch.0, local.0.0, life.unwrap_or(0));
    if sources.context != Some(context) {
        sources.source = None;
        sources.was_active = false;
        sources.context = Some(context);
    }
    let alive = presented.alive_player(local.0).is_some();
    let remaining = presented
        .player(local.0)
        .map(|ps| {
            shellshock_remaining_ms(cg_clock.time(), ps.shellshock_time, ps.shellshock_duration)
        })
        .unwrap_or(0);
    let Some(parms) = presented
        .shellshock(local.0)
        .map(|shock| shock.sound.clone())
    else {
        sources.source = None;
        sources.was_active = false;
        return;
    };
    let want = remaining > 0 && parms.affect && alive && !died;
    if want {
        let same = sources
            .source
            .as_ref()
            .is_some_and(|source| source.cue.alias == parms.loop_alias);
        if !same {
            sources.source = None;
            if let Some(bank) = bank
                && let Some(cue) = runtime.source_cue(SourceCueRequest {
                    bank: bank.0.clone(),
                    namespace: AssetNamespace::Iw4,
                    alias: parms.loop_alias.clone(),
                    emitter: Some(local.0.0),
                    scope: crate::backend::AudioScope::Match,
                    epoch: epoch.0,
                    group: None,
                })
            {
                sources.next_version = sources
                    .next_version
                    .checked_add(1)
                    .expect("source version exhausted");
                sources.source = Some(DesiredSource {
                    key: SourceKey {
                        scope: crate::backend::AudioScope::Match,
                        epoch: epoch.0,
                        object: u64::from(local.0.0),
                        slot: 5,
                    },
                    version: sources.next_version,
                    cue,
                    origin_inches: None,
                    start_frame: runtime.audio_frame(),
                    gain: 1.0,
                    rate: 1.0,
                    audible: true,
                });
            }
        }
        sources.was_active = true;
        return;
    }
    if sources.was_active {
        sources.source = None;
        let alias = if died || !alive {
            &parms.abort_alias
        } else {
            &parms.end_alias
        };
        play.write(crate::AliasCommand::Play(PlayAlias {
            event: None,
            namespace: AssetNamespace::Iw4,
            alias: alias.to_owned(),
            fallback: None,
            origin_inches: None,
            snd_ent: Some(SND_ENT_LOCAL),
        }));
    }
    sources.was_active = false;
}
