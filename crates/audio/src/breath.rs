use asset_core::AssetNamespace;
use bevy::prelude::*;
use net::{LocalPresentClient, PresentedSnapshot};
use playerstate_iw4::{breath_hold_time_ms, weap_flags};

use crate::sources::{DesiredSource, SourceCueRequest, SourceKey};
use crate::{AliasCommand, PlayAlias};

#[derive(Default)]
pub(crate) struct BreathAudio {
    active: bool,
    holding: bool,
    selection: Option<(AssetNamespace, asset_game::BreathCuePolicy)>,
    heartbeat_at: i32,
}

#[derive(Resource, Default)]
pub(crate) struct BreathSources {
    pub source: Option<DesiredSource>,
    context: Option<(u64, u32, u32)>,
    next_version: u64,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update(
    presented: Option<Res<PresentedSnapshot>>,
    local: Option<Res<LocalPresentClient>>,
    weapons: Option<Res<assets::PreparedWeapons>>,
    view: Option<Res<frame::ViewSubject>>,
    screen: Option<Res<frame::AppScreen>>,
    bank: Option<Res<crate::SoundBank>>,
    mut torn: MessageReader<frame::MatchTornDown>,
    mut died: MessageReader<frame::LifeEnded>,
    mut state: Local<BreathAudio>,
    mut sources: ResMut<BreathSources>,
    runtime: Res<crate::AudioRuntime>,
    epoch: Res<crate::backend::MatchEpoch>,
    mut play: MessageWriter<AliasCommand>,
) {
    let client = local.as_ref().map(|local| local.0);
    let life = client.and_then(|client| {
        presented
            .as_ref()?
            .snapshot()?
            .meta
            .for_client(client)
            .map(|meta| meta.life_sequence.0)
    });
    let context = client
        .zip(life)
        .map(|(client, life)| (epoch.0, client.0, life));
    if sources.context != context {
        if state.active {
            stop_aliases(
                &mut play,
                state.selection,
                sources.context.map(|(_, client, _)| client),
            );
        }
        sources.source = None;
        sources.context = context;
        *state = BreathAudio::default();
    }
    let torn_down = torn.read().count() != 0;
    let died = died.read().any(|event| {
        client.is_some_and(|client| event.client == client.0) && life == Some(event.life)
    });
    let ps = client
        .and_then(|client| presented.as_ref()?.alive_player(client))
        .filter(|_| !view.as_ref().is_some_and(|view| view.in_killcam()));
    if torn_down
        || died
        || !screen.is_some_and(|screen| matches!(*screen, frame::AppScreen::InGame))
        || ps.is_none()
    {
        if state.active {
            stop_aliases(&mut play, state.selection, client.map(|client| client.0));
        }
        sources.source = None;
        sources.context = None;
        *state = BreathAudio::default();
        return;
    }
    let holding = ps.is_some_and(|p| p.weap_flags & weap_flags::HOLD_BREATH != 0);
    let selection = ps.and_then(|ps| {
        let policy = weapons
            .as_ref()?
            .registry()
            .semantic_policy_of(playerstate_iw4::get_viewmodel_weapon_index(ps))?;
        Some((policy.cue_namespace.namespace(), policy.breath_cues))
    });
    let Some((ns, cues)) = selection else {
        if state.active {
            stop_aliases(&mut play, state.selection, client.map(|client| client.0));
        }
        sources.source = None;
        *state = BreathAudio::default();
        return;
    };
    let changed = state.selection != selection;
    if state.active && changed {
        stop_aliases(&mut play, state.selection, client.map(|client| client.0));
        sources.source = None;
    }
    if state.holding && !holding && !changed {
        sources.source = None;
        for alias in [cues.inhale, cues.heartbeat] {
            play.write(AliasCommand::Stop {
                namespace: ns,
                alias: alias.to_owned(),
                snd_ent: Some(crate::SND_ENT_LOCAL),
            });
        }
        if let Some(ps) = ps {
            let alias = if ps.hold_breath_timer > breath_hold_time_ms(ps) {
                cues.gasp
            } else {
                cues.exhale
            };
            play.write(AliasCommand::Play(sound(ns, alias)));
        }
    }
    if holding && (!state.holding || changed) {
        play.write(AliasCommand::Play(sound(ns, cues.inhale)));
        state.heartbeat_at = ps.map_or(0, |p| p.command_time).saturating_add(1000);
    }
    if holding
        && let Some(ps) = ps
        && sources.source.is_none()
        && ps.command_time >= state.heartbeat_at
    {
        let looping = bank.as_ref().is_some_and(|bank| {
            bank.0.index_in(ns, cues.heartbeat).is_some_and(|index| {
                bank.0.sound_at(index).is_some_and(|sound| {
                    (0..sound.aliases.len()).any(|variant| {
                        bank.0
                            .playback_policy(index, variant)
                            .is_some_and(|policy| policy.looping().is_looping())
                    })
                })
            })
        });
        if looping {
            if let Some(bank) = bank.as_ref()
                && let Some(cue) = runtime.source_cue(SourceCueRequest {
                    bank: bank.0.clone(),
                    namespace: ns,
                    alias: cues.heartbeat.into(),
                    emitter: client.map(|client| client.0),
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
                        object: u64::from(client.expect("alive client").0),
                        slot: 6,
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
        } else {
            play.write(AliasCommand::Play(sound(ns, cues.heartbeat)));
            state.heartbeat_at = ps.command_time.saturating_add(1000);
        }
    }
    state.active = true;
    state.holding = holding;
    state.selection = selection;
}

fn stop_aliases(
    play: &mut MessageWriter<AliasCommand>,
    selection: Option<(AssetNamespace, asset_game::BreathCuePolicy)>,
    client: Option<u32>,
) {
    let Some((namespace, cues)) = selection else {
        return;
    };
    for alias in cues.aliases() {
        play.write(AliasCommand::Stop {
            namespace,
            alias: alias.to_owned(),
            snd_ent: client,
        });
    }
}

fn sound(namespace: AssetNamespace, alias: &str) -> PlayAlias {
    PlayAlias {
        event: None,
        namespace,
        alias: alias.to_owned(),
        fallback: None,
        origin_inches: None,
        snd_ent: Some(crate::SND_ENT_LOCAL),
    }
}
