use std::sync::Arc;

use crate::runtime::AudioRuntime;
use asset_audio::SoundCatalog;
use asset_core::AssetNamespace;
use bevy::prelude::*;
use frame::{ClientSet, FxSoundPublished, MatchTornDown, SessionSwapApplied};
use net::{LastAdoptedSnapshot, SvcLocalSound};

use crate::backend::MatchEpoch;
use crate::clip_store::CueFeedback;
use crate::messages::{
    AliasCommand, BoundWeaponSound, Footstep, LandSound, PlayAlias, SND_ENT_LOCAL,
    ViewmodelNotetracks, WeaponSound,
};
use crate::start::{SoundClass, StartDecision, StartDecisions, StartFailure, StartOutcome};

#[derive(Component, Default)]
pub struct AmbientListener;

#[derive(Resource, Clone)]
pub struct SoundBank(pub Arc<SoundCatalog>);

#[derive(Resource, Default, Debug)]
pub struct MissingAliasGaps {
    pub aliases: Vec<String>,
}

impl MissingAliasGaps {
    pub fn record(&mut self, alias: &str) {
        if !self.aliases.iter().any(|a| a == alias) {
            diag::warn!(Audio, "audio: missing alias `{alias}` (typed gap)");
            self.aliases.push(alias.to_owned());
        }
    }

    pub fn len(&self) -> usize {
        self.aliases.len()
    }

    pub fn is_empty(&self) -> bool {
        self.aliases.is_empty()
    }
}

pub(crate) struct PlayerSoundPlugin;

impl Plugin for PlayerSoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::clip_store::ResidentClipCache>()
            .init_resource::<MissingAliasGaps>()
            .init_resource::<StartDecisions>()
            .init_resource::<CueFeedback>()
            .init_resource::<crate::ambient::MapAmbientBooted>()
            .init_resource::<crate::ambient::MapSources>()
            .init_resource::<crate::script_ambient::ScriptAmbientPlayback>()
            .init_resource::<crate::destructible_loops::DestructibleSources>()
            .init_resource::<crate::ambient::SoundBankLoadAttempted>()
            .init_resource::<crate::ambient::ResidentSoundBank>()
            .init_resource::<crate::BobCycleTracker>()
            .init_resource::<crate::shellshock::ShellshockSources>()
            .init_resource::<crate::breath::BreathSources>()
            .add_message::<AliasCommand>()
            .add_message::<Footstep>()
            .add_message::<WeaponSound>()
            .add_message::<BoundWeaponSound>()
            .add_message::<ViewmodelNotetracks>()
            .init_resource::<crate::entity_events::NotetrackSoundTable>()
            .add_message::<LandSound>()
            .add_systems(
                Update,
                play_weapon_sound_messages
                    .in_set(ClientSet::Predict)
                    .after(frame::OwnerEventsPublished)
                    .after(crate::backend::publish_audio_context),
            )
            .add_systems(
                Update,
                (
                    crate::ambient::boot_map_ambient_once,
                    crate::script_ambient::update_script_ambient
                        .after(crate::ambient::boot_map_ambient_once),
                    collect_cue_decisions
                        .after(play_alias_messages)
                        .before(play_footstep_messages)
                        .before(play_land_sound_messages),
                    apply_svc_local_sound
                        .before(play_alias_messages)
                        .run_if(resource_exists::<LastAdoptedSnapshot>),
                    crate::shellshock::update_shellshock_tinnitus.before(play_alias_messages),
                    crate::breath::update.before(play_alias_messages),
                    play_alias_messages.after(FxSoundPublished),
                    play_footstep_messages,
                    crate::entity_events::play_viewmodel_notetrack_messages
                        .before(play_bound_weapon_sounds),
                    play_bound_weapon_sounds.after(collect_cue_decisions),
                    play_land_sound_messages,
                    crate::destructible_loops::update,
                )
                    .in_set(ClientSet::Effects),
            )
            .add_systems(
                Update,
                (
                    crate::ambient::start_sound_bank_compose
                        .after(crate::ambient::stop_map_ambient_on_match_end),
                    crate::ambient::install_sound_bank
                        .after(crate::ambient::start_sound_bank_compose),
                    crate::entity_events::bind_notetrack_sounds
                        .after(crate::ambient::install_sound_bank),
                    crate::ambient::stop_map_ambient_on_match_end.after(SessionSwapApplied),
                    reset_clip_prep_on_match_torn_down,
                )
                    .in_set(ClientSet::Load),
            );
    }
}

fn reset_clip_prep_on_match_torn_down(
    mut torn: MessageReader<MatchTornDown>,
    mut pending: ResMut<CueFeedback>,
) {
    if torn.read().count() == 0 {
        return;
    }
    pending.clear();
}

fn apply_svc_local_sound(
    mut cmds: MessageReader<SvcLocalSound>,
    adopted: Res<LastAdoptedSnapshot>,
    announcer: Res<crate::match_voices::AnnouncerRoutes>,
    mut play: MessageWriter<crate::AliasCommand>,
) {
    for cmd in cmds.read() {
        let Some((namespace, alias)) = adopted
            .sound_alias_name(cmd.index)
            .map(|alias| announcer.route(alias))
            .map(|(namespace, alias)| (namespace, alias.to_owned()))
        else {
            diag::warn!(
                Audio,
                "audio: svc local sound CS index {} is unresolved (typed gap)",
                cmd.index
            );
            continue;
        };
        if crate::diagnostics::enabled() {
            crate::diagnostics::emit(format!(
                "audio diag: authority_local_sound alias={alias} stop={} index={} server_tick={:?}",
                cmd.stop,
                cmd.index,
                adopted.next().map(|snapshot| snapshot.tick.0)
            ));
        }
        if cmd.stop {
            play.write(AliasCommand::Stop {
                namespace,
                alias,
                snd_ent: Some(SND_ENT_LOCAL),
            });
        } else {
            play.write(crate::AliasCommand::Play(PlayAlias {
                event: None,
                namespace,
                alias,
                fallback: None,
                origin_inches: None,
                snd_ent: Some(SND_ENT_LOCAL),
            }));
        }
    }
}

fn sound_entity(number: Option<u32>, local: sim::ClientId) -> Option<u32> {
    number.map(|number| {
        if number == SND_ENT_LOCAL {
            local.0
        } else {
            number
        }
    })
}

fn play_alias_messages(
    mut events: MessageReader<AliasCommand>,
    runtime: Res<AudioRuntime>,
    mut pending: ResMut<CueFeedback>,
    mut decisions: ResMut<StartDecisions>,
    bank: Option<Res<SoundBank>>,
    epoch: Res<MatchEpoch>,
    local: Res<net::LocalPresentClient>,
) {
    for command in events.read() {
        let (event, pitch_scale) = match command {
            AliasCommand::StopEntity { snd_ent } => {
                runtime.stop_emitter_cues(*snd_ent, epoch.0);
                continue;
            }
            AliasCommand::Play(event) => (event, 1.0),
            AliasCommand::PlayPitched { sound, pitch } => (sound, *pitch),
            AliasCommand::Stop {
                namespace,
                alias,
                snd_ent,
            } => {
                let snd_ent = sound_entity(*snd_ent, local.0);
                runtime.stop_cue(*namespace, alias, snd_ent, epoch.0);
                continue;
            }
        };
        let Some(bank) = bank.as_ref() else {
            drop_without_bank(
                std::iter::once((event.alias.as_str(), event.event)),
                &mut decisions,
            );
            continue;
        };
        play_oneshot_recorded(
            &bank.0,
            event.namespace,
            &event.alias,
            None,
            event.origin_inches,
            &runtime,
            &mut pending,
            &mut decisions,
            sound_entity(event.snd_ent, local.0),
            SoundClass::World,
            epoch.0,
            pitch_scale,
            event.fallback.iter().cloned().collect(),
            event.event,
        );
    }
}

fn play_footstep_messages(
    mut events: MessageReader<Footstep>,
    runtime: Res<AudioRuntime>,
    mut gaps: ResMut<MissingAliasGaps>,
    mut pending: ResMut<CueFeedback>,
    mut decisions: ResMut<StartDecisions>,
    bank: Option<Res<SoundBank>>,
    epoch: Res<MatchEpoch>,
) {
    let Some(bank) = bank else {
        drop_without_bank(events.read().map(|e| (e.alias, e.event)), &mut decisions);
        return;
    };
    for event in events.read() {
        let outcome = play_surface_alias_chain(
            &bank.0,
            event.alias,
            event.fallback,
            event.origin_inches,
            &runtime,
            &mut pending,
            &mut decisions,
            event.snd_ent,
            SoundClass::World,
            epoch.0,
            event.event,
        );
        if outcome.allows_binding_fallback() {
            gaps.record(event.alias);
            gaps.record(event.fallback);
        }
    }
}

fn play_weapon_sound_messages(
    mut events: MessageReader<WeaponSound>,
    runtime: Res<AudioRuntime>,
    mut gaps: ResMut<MissingAliasGaps>,
    mut pending: ResMut<CueFeedback>,
    mut decisions: ResMut<StartDecisions>,
    bank: Option<Res<SoundBank>>,
    epoch: Res<MatchEpoch>,
) {
    let Some(bank) = bank else {
        drop_without_bank(
            events.read().map(|e| (e.alias.as_str(), e.event)),
            &mut decisions,
        );
        return;
    };
    for event in events.read() {
        let outcome = play_oneshot_recorded(
            &bank.0,
            event.namespace,
            &event.alias,
            None,
            event.origin_inches,
            &runtime,
            &mut pending,
            &mut decisions,
            event.snd_ent,
            SoundClass::Weapon,
            epoch.0,
            1.0,
            Vec::new(),
            event.event,
        );
        if outcome.allows_binding_fallback() {
            gaps.record(&event.alias);
        }
    }
}

fn play_bound_weapon_sounds(
    mut events: MessageReader<BoundWeaponSound>,
    runtime: Res<AudioRuntime>,
    mut gaps: ResMut<MissingAliasGaps>,
    mut pending: ResMut<CueFeedback>,
    mut decisions: ResMut<StartDecisions>,
    bank: Option<Res<SoundBank>>,
    epoch: Res<MatchEpoch>,
    local: Res<net::LocalPresentClient>,
) {
    let Some(bank) = bank else {
        for _ in events.read() {}
        return;
    };
    for event in events.read() {
        if event.bank_revision != bank.0.revision() {
            continue;
        }
        let Some(alias) = bank.0.name_at(event.index) else {
            continue;
        };
        let Some(namespace) = bank.0.namespace_of_alias(event.index) else {
            continue;
        };
        let outcome = play_oneshot_recorded(
            &bank.0,
            namespace,
            alias,
            Some(event.index),
            event.origin_inches,
            &runtime,
            &mut pending,
            &mut decisions,
            sound_entity(event.snd_ent, local.0),
            SoundClass::Weapon,
            epoch.0,
            1.0,
            Vec::new(),
            event.event,
        );
        if outcome.allows_binding_fallback() {
            gaps.record(alias);
        }
    }
}

fn play_land_sound_messages(
    mut events: MessageReader<LandSound>,
    runtime: Res<AudioRuntime>,
    mut gaps: ResMut<MissingAliasGaps>,
    mut pending: ResMut<CueFeedback>,
    mut decisions: ResMut<StartDecisions>,
    bank: Option<Res<SoundBank>>,
    epoch: Res<MatchEpoch>,
) {
    let Some(bank) = bank else {
        drop_without_bank(events.read().map(|e| (e.alias, e.event)), &mut decisions);
        return;
    };
    for event in events.read() {
        let outcome = play_surface_alias_chain(
            &bank.0,
            event.alias,
            event.fallback,
            event.origin_inches,
            &runtime,
            &mut pending,
            &mut decisions,
            event.snd_ent,
            SoundClass::World,
            epoch.0,
            event.event,
        );
        if outcome.allows_binding_fallback() {
            gaps.record(event.alias);
            gaps.record(event.fallback);
        }
    }
}

fn drop_without_bank<'a>(
    aliases: impl Iterator<Item = (&'a str, Option<crate::AudioEvent>)>,
    decisions: &mut StartDecisions,
) {
    for (alias, event) in aliases {
        if !crate::AudioSilent::active() {
            diag::warn!(
                Audio,
                "audio: alias `{alias}` dropped — no SoundBank (typed gap)"
            );
        }
        decisions.record(StartDecision {
            event,
            namespace: AssetNamespace::Iw4,
            alias: alias.to_owned(),
            variant: None,
            loaded_binding_origin: None,
            outcome: StartOutcome::Failed(StartFailure::BankMissing),
            secondary: None,
            detail: None,
        });
    }
}

fn play_surface_alias_chain(
    bank: &Arc<SoundCatalog>,
    alias: &str,
    fallback: &str,
    origin_inches: Option<[f32; 3]>,
    runtime: &AudioRuntime,
    pending: &mut CueFeedback,
    decisions: &mut StartDecisions,
    snd_ent: Option<u32>,
    class: SoundClass,
    epoch: u64,
    event: Option<crate::AudioEvent>,
) -> StartOutcome {
    let candidates = crate::aliases::surface_alias_candidates(alias, fallback);
    let Some(first) = candidates.first() else {
        return StartOutcome::Failed(StartFailure::MissingAlias);
    };
    play_oneshot_recorded(
        bank,
        AssetNamespace::Iw4,
        first,
        None,
        origin_inches,
        runtime,
        pending,
        decisions,
        snd_ent,
        class,
        epoch,
        1.0,
        candidates[1..]
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
        event,
    )
}

pub fn world_oneshot_pan(ear: Vec3, listener_right: Vec3, emitter: Vec3) -> f32 {
    crate::spatial::pan(
        ear.to_array(),
        listener_right.to_array(),
        emitter.to_array(),
    )
}

pub fn world_oneshot_channel_gains(
    ear: Vec3,
    listener_right: Vec3,
    emitter: Vec3,
    atten: f32,
) -> (f32, f32) {
    crate::spatial::channel_gains(
        ear.to_array(),
        listener_right.to_array(),
        emitter.to_array(),
        atten,
    )
}

fn collect_cue_decisions(
    mut pending: ResMut<CueFeedback>,
    mut decisions: ResMut<StartDecisions>,
    mut gaps: ResMut<MissingAliasGaps>,
) {
    pending.cues.retain(|entry| {
        let Some(decision) = entry.handle.completion() else {
            return true;
        };
        if decision.outcome.allows_binding_fallback() {
            gaps.record(&decision.alias);
        }
        decisions.record(decision);
        false
    });
}

pub(crate) fn play_alias_oneshot(
    bank: &Arc<SoundCatalog>,
    namespace: AssetNamespace,
    alias: &str,
    origin_inches: Option<[f32; 3]>,
    runtime: &AudioRuntime,
    pending: &mut CueFeedback,
    decisions: &mut StartDecisions,
    snd_ent: Option<u32>,
    class: SoundClass,
    epoch: u64,
) -> StartOutcome {
    play_oneshot_recorded(
        bank,
        namespace,
        alias,
        None,
        origin_inches,
        runtime,
        pending,
        decisions,
        snd_ent,
        class,
        epoch,
        1.0,
        Vec::new(),
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn play_oneshot_recorded(
    bank: &Arc<SoundCatalog>,
    namespace: AssetNamespace,
    alias: &str,
    bound: Option<usize>,
    origin_inches: Option<[f32; 3]>,
    runtime: &AudioRuntime,
    pending: &mut CueFeedback,
    decisions: &mut StartDecisions,
    snd_ent: Option<u32>,
    class: SoundClass,
    epoch: u64,
    pitch_scale: f32,
    fallbacks: Vec<String>,
    event: Option<crate::AudioEvent>,
) -> StartOutcome {
    let handle = runtime.trigger_cue(crate::cue_execution::CueTrigger {
        event,
        bank: bank.clone(),
        namespace,
        alias: alias.into(),
        bound,
        origin_inches,
        emitter: snd_ent,
        class,
        epoch,
        pitch_scale,
        fallbacks,
    });
    let outcome = StartOutcome::Pending;
    pending.push(handle);
    decisions.record(StartDecision {
        event,
        namespace,
        alias: alias.into(),
        variant: None,
        loaded_binding_origin: None,
        outcome: outcome.clone(),
        secondary: None,
        detail: None,
    });
    outcome
}
