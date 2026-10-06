use std::sync::Arc;

use asset_audio::SoundCatalog;
use asset_core::AssetNamespace;
use asset_transport::GamesRoot;
use assets::{NamespaceSoundIwd, NamespaceTrees};
use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, TaskPool, futures_lite::future},
};
use frame::{MatchTornDown, ReturnedToMenu};

use crate::backend::{AudioScope, MatchEpoch};
use crate::playback::SoundBank;
use crate::sources::{DesiredSource, SourceCueRequest, SourceKey, SourceRenderGroup};

pub(crate) const MAP_BED_SLOT: u32 = 2;
const MAP_EMITTER_SLOT: u32 = 3;

#[derive(Resource, Default)]
pub(crate) struct MapSources {
    pub desired: Vec<DesiredSource>,
}

#[derive(Resource, Clone)]
pub struct SoundIwd(pub Arc<NamespaceSoundIwd>);

#[derive(Resource, Default)]
pub struct MapAmbientBooted(pub bool);

#[derive(Resource, Default)]
pub(crate) struct SoundBankLoadAttempted(pub bool);

const SOUND_BANK_COMPOSE_STALL: std::time::Duration = std::time::Duration::from_secs(20);

#[derive(Resource)]
pub(crate) struct SoundBankCompose {
    load_key: frame::LocalLoadKey,
    zone: String,
    iwd: IwdOpen,
    bank: Option<Task<ComposedBank>>,
    common_profile_id: u64,
    products_id: u64,
    started: std::time::Instant,
    stall_reported: bool,
}

enum IwdOpen {
    Opening(Task<(NamespaceSoundIwd, Vec<String>)>),
    Open(Arc<NamespaceSoundIwd>),
}

struct ComposedBank {
    loaded: Result<(Arc<SoundCatalog>, usize), String>,
    namespace: AssetNamespace,
    reused: bool,
}

#[derive(Resource, Default)]
pub(crate) struct ResidentSoundBank(Option<ResidentBank>);

struct ResidentBank {
    zone: String,
    iwd: Arc<NamespaceSoundIwd>,
    bank: Option<ResidentComposed>,
}

struct ResidentComposed {
    products_id: u64,
    common_profile_id: u64,
    catalog: Arc<SoundCatalog>,
    namespace: AssetNamespace,
    gaps: usize,
}

#[derive(Resource)]
pub(crate) struct SoundBankNamespace {
    pub(crate) generation: frame::WorldGeneration,
    pub(crate) zone: String,
    pub(crate) namespace: AssetNamespace,
}

pub(crate) fn stop_map_ambient_on_match_end(
    mut torn: MessageReader<MatchTornDown>,
    mut returned: MessageReader<ReturnedToMenu>,
    mut sources: ResMut<MapSources>,
    mut booted: ResMut<MapAmbientBooted>,
    mut attempted: ResMut<SoundBankLoadAttempted>,
    mut epoch: ResMut<MatchEpoch>,
    compose: Option<Res<SoundBankCompose>>,
    accepted: Option<Res<assets::MatchLoadAccepted>>,
    mut commands: Commands,
) {
    let retired: Vec<_> = torn.read().map(|fact| fact.world_generation).collect();
    let left_session = returned.read().count() > 0;
    if retired.is_empty() && !left_session {
        return;
    }
    epoch.bump();
    sources.desired.clear();
    booted.0 = false;
    commands.remove_resource::<asset_audio::CreateFxOneshotEmitters>();

    let compose_is_for_the_incoming_map = !left_session
        && compose
            .as_ref()
            .zip(accepted.as_ref())
            .is_some_and(|(compose, accepted)| {
                compose.load_key == accepted.load_key
                    && !retired.contains(&frame::WorldGeneration::from_install(
                        compose.load_key.local_load_request_id,
                    ))
            });
    if !compose_is_for_the_incoming_map {
        commands.remove_resource::<SoundBankCompose>();
        commands.remove_resource::<assets::PreparedMatchSound>();
        attempted.0 = false;
    }
    commands.queue(|world: &mut bevy::prelude::World| {
        frame::retire::retire_resources(world, |batch| {
            batch
                .resource::<SoundBankNamespace>()
                .resource::<SoundBank>()
                .resource::<SoundIwd>()
                .resource::<crate::ClipStore>();
        });
    });
    perf::ambient_hold(i64::from(booted.0));
    diag::info!(Audio, "audio: map ambient stopped");
}

pub(crate) fn start_sound_bank_compose(
    mut attempted: ResMut<SoundBankLoadAttempted>,
    accepted: Option<Res<assets::MatchLoadAccepted>>,
    abort: Option<Res<assets::MatchLoadAbort>>,
    identity: Option<Res<frame::LaunchIdentity>>,
    silent: Option<Res<crate::AudioSilent>>,
    resident: Res<ResidentSoundBank>,
    mut commands: Commands,
) {
    if attempted.0 {
        return;
    }
    if silent.is_some() {
        attempted.0 = true;
        return;
    }
    let Some(accepted) = accepted else {
        return;
    };
    if abort.is_some_and(|abort| abort.0 == accepted.request_id) {
        return;
    }
    let Some(identity) = identity else {
        return;
    };
    if accepted.zone.is_empty() || identity.games_root.as_os_str().is_empty() {
        return;
    }
    attempted.0 = true;
    let games = GamesRoot(identity.games_root.clone());
    let zone = accepted.zone.clone();
    let opened_zone = zone.clone();

    let iwd = match resident.0.as_ref().filter(|resident| resident.zone == zone) {
        Some(resident) => {
            diag::info!(Audio, "audio: sound archives for `{zone}` reused");
            IwdOpen::Open(Arc::clone(&resident.iwd))
        }
        None => {
            let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
            IwdOpen::Opening(pool.spawn(async move {
                let mut trees = NamespaceTrees::discover(&games);
                if let Ok(zone) = assets::find_zone_file(&games, &opened_zone) {
                    trees.adopt_zone(&zone.path);
                }
                NamespaceSoundIwd::open(&trees)
            }))
        }
    };
    commands.insert_resource(SoundBankCompose {
        load_key: accepted.load_key,
        zone,
        iwd,
        bank: None,
        common_profile_id: 0,
        products_id: 0,
        started: std::time::Instant::now(),
        stall_reported: false,
    });
}

pub(crate) fn install_sound_bank(
    mut compose: Option<ResMut<SoundBankCompose>>,
    identity: Option<Res<frame::LaunchIdentity>>,
    accepted: Option<Res<assets::MatchLoadAccepted>>,
    abort: Option<Res<assets::MatchLoadAbort>>,
    mut map_sound: Option<ResMut<assets::PreparedMatchSound>>,
    mut epoch: ResMut<MatchEpoch>,
    resident_clips: Res<crate::clip_store::ResidentClipCache>,
    mut resident: ResMut<ResidentSoundBank>,
    mut commands: Commands,
) {
    let Some(compose) = compose.as_deref_mut() else {
        return;
    };

    let superseded = accepted
        .as_ref()
        .is_some_and(|accepted| accepted.load_key != compose.load_key);
    if abort.is_some_and(|abort| abort.0 == compose.load_key.local_load_request_id) || superseded {
        diag::info!(
            Audio,
            "audio: sound bank for `{}` dropped — the session moved on",
            compose.zone
        );
        commands.remove_resource::<SoundBankCompose>();
        return;
    }
    // Install on the load key, not `identity.zone`: gating on a display spelling
    // that never converges would hold AudioReady and the world spawn forever.
    let identity_zone = identity.as_ref().map(|identity| identity.zone.as_str());
    if let Some(zone) = identity_zone.filter(|zone| *zone != compose.zone) {
        diag::warn!(
            Audio,
            "audio: launch identity says `{zone}` while the accepted load walked `{}` — installing on the load key",
            compose.zone
        );
    }

    if let IwdOpen::Opening(task) = &mut compose.iwd {
        if let Some((indices, lines)) = future::block_on(future::poll_once(task)) {
            for line in lines {
                diag::info!(Audio, "{line}");
            }
            compose.iwd = IwdOpen::Open(Arc::new(indices));
        }
    }
    if compose.bank.is_none() {
        if let Some(arrived) = map_sound.as_deref_mut() {
            if arrived.load_key == compose.load_key {
                compose.common_profile_id = arrived.common_profile_id;
                let products_id = arrived.products_id;
                let map = std::mem::replace(&mut arrived.sound, Err(String::new()));
                commands.remove_resource::<assets::PreparedMatchSound>();
                let kept = resident
                    .0
                    .as_ref()
                    .filter(|resident| resident.zone == compose.zone)
                    .and_then(|resident| resident.bank.as_ref())
                    .filter(|bank| {
                        bank.products_id == products_id
                            && bank.common_profile_id == compose.common_profile_id
                    })
                    .map(|bank| ComposedBank {
                        loaded: Ok((Arc::clone(&bank.catalog), bank.gaps)),
                        namespace: bank.namespace,
                        reused: true,
                    });
                let namespace = arrived.namespace;
                let gaps = arrived.gaps;
                let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
                compose.bank = Some(pool.spawn(async move {
                    kept.unwrap_or_else(|| ComposedBank {
                        loaded: map.map(|catalog| (Arc::new(catalog), gaps)),
                        namespace,
                        reused: false,
                    })
                }));
                compose.products_id = products_id;
            }
        }
    }

    let composed = match (&compose.iwd, compose.bank.as_mut()) {
        (IwdOpen::Open(_), Some(bank)) => future::block_on(future::poll_once(bank)),
        _ => None,
    };
    let Some(composed) = composed else {
        if !compose.stall_reported && compose.started.elapsed() >= SOUND_BANK_COMPOSE_STALL {
            compose.stall_reported = true;
            diag::warn!(
                Audio,
                "audio: sound bank for `{}` still not composed after {:.0}s (archives {}, map sound {}) — AudioReady, the world spawn and host admission all wait on it",
                compose.zone,
                compose.started.elapsed().as_secs_f32(),
                if matches!(compose.iwd, IwdOpen::Open(_)) {
                    "open"
                } else {
                    "opening"
                },
                if compose.bank.is_some() {
                    "arrived"
                } else {
                    "pending"
                },
            );
        }
        return;
    };
    commands.remove_resource::<SoundBankCompose>();
    let IwdOpen::Open(iwd) = std::mem::replace(&mut compose.iwd, IwdOpen::Open(Arc::default()))
    else {
        unreachable!("the bank is polled only once the archives are open");
    };
    let ComposedBank {
        loaded,
        namespace,
        reused,
    } = composed;
    match loaded {
        Ok((bank, gaps)) => {
            epoch.bump();
            if iwd.is_empty() {
                diag::warn!(
                    Audio,
                    "audio: no IWD sound archives for `{}` — streamed aliases will gap",
                    compose.zone
                );
            }
            commands.insert_resource(SoundIwd(Arc::clone(&iwd)));
            resident.0 = Some(ResidentBank {
                zone: compose.zone.clone(),
                iwd: Arc::clone(&iwd),
                bank: Some(ResidentComposed {
                    products_id: compose.products_id,
                    common_profile_id: compose.common_profile_id,
                    catalog: Arc::clone(&bank),
                    namespace,
                    gaps,
                }),
            });

            let new_clips = crate::ClipStore::start_with_common(
                Arc::clone(&bank),
                Some(iwd),
                compose.common_profile_id,
                Some(resident_clips.clone()),
            );
            commands.queue(move |world: &mut World| {
                frame::retire::retire_resources(world, |batch| {
                    batch.resource::<crate::ClipStore>();
                });
                world.insert_resource(new_clips);
            });
            commands.insert_resource(crate::clip_store::CueFeedback::default());
            commands.insert_resource(SoundBank(bank));
            diag::info!(
                Audio,
                "audio: sound bank {} for {} ({} zone gaps) in {:.0}ms",
                if reused { "reused" } else { "ready" },
                compose.zone,
                gaps,
                compose.started.elapsed().as_secs_f32() * 1000.0
            );
            commands.insert_resource(SoundBankNamespace {
                generation: frame::WorldGeneration::from_install(
                    compose.load_key.local_load_request_id,
                ),
                zone: compose.zone.clone(),
                namespace,
            });
        }
        Err(e) => {
            diag::warn!(
                Audio,
                "audio: sound bank load failed for {}: {e}",
                compose.zone
            );
        }
    }
}

pub(crate) fn boot_map_ambient_once(
    presented: Option<Res<net::PresentedSnapshot>>,
    mut booted: ResMut<MapAmbientBooted>,
    loading: Option<Res<assets::LoadingScreen>>,
    bank: Option<Res<SoundBank>>,
    identity: Option<Res<frame::LaunchIdentity>>,
    script_sound: Option<Res<asset_audio::SessionMapScriptSound>>,
    namespace: Option<Res<SoundBankNamespace>>,
    epoch: Res<MatchEpoch>,
    ready: Res<crate::AudioReady>,
    generation: Res<frame::WorldGeneration>,
    runtime: Res<crate::AudioRuntime>,
    mut sources: ResMut<MapSources>,
    mut commands: Commands,
) {
    if booted.0 {
        return;
    }

    if loading.is_some_and(|screen| !screen.is_complete()) {
        return;
    }
    if !ready.0.ready_for(*generation) {
        return;
    }
    let Some(bank) = bank else {
        return;
    };
    let Some(identity) = identity else {
        return;
    };
    if identity.zone.is_empty() {
        return;
    }
    // A Minecraft world has no ambience of the map it stands in for.
    if assets::minecraft_map::is_minecraft_load(&identity.zone) {
        booted.0 = true;
        return;
    }
    let Some(namespace) = namespace.filter(|ns| ns.zone == identity.zone) else {
        return;
    };
    let scripted = presented
        .as_ref()
        .and_then(|p| p.snapshot())
        .is_some_and(|s| s.meta.objectives.ambient.is_some());
    let ambient_alias = script_sound
        .as_deref()
        .and_then(|facts| facts.0.ambient_alias.as_deref())
        .filter(|_| !scripted);
    if runtime.media_for_bank(&bank.0).is_none() {
        return;
    }
    sources.desired.clear();
    let rows = ambient_alias
        .into_iter()
        .map(|alias| (0, MAP_BED_SLOT, alias.to_owned(), None, 0.55, None))
        .chain(
            bank.0
                .createfx_loop_sounds(namespace.namespace, &identity.zone)
                .into_iter()
                .enumerate()
                .map(|(ordinal, emitter)| {
                    (
                        ordinal as u64,
                        MAP_EMITTER_SLOT,
                        emitter.soundalias,
                        Some(emitter.origin_inches),
                        1.0,
                        Some(SourceRenderGroup::MapEmitter),
                    )
                }),
        );
    for (object, slot, alias, origin, gain, group) in rows.take(crate::runtime::LOGICAL_INSTANCES) {
        let Some(cue) = runtime.source_cue(SourceCueRequest {
            bank: bank.0.clone(),
            namespace: namespace.namespace,
            alias,
            emitter: None,
            scope: AudioScope::Match,
            epoch: epoch.0,
            group,
        }) else {
            continue;
        };
        sources.desired.push(DesiredSource {
            key: SourceKey {
                scope: AudioScope::Match,
                epoch: epoch.0,
                object,
                slot,
            },
            version: 1,
            cue,
            origin_inches: origin,
            start_frame: runtime.audio_frame(),
            gain,
            rate: 1.0,
            audible: true,
        });
    }
    commands.insert_resource(asset_audio::CreateFxOneshotEmitters(
        bank.0
            .createfx_oneshots(namespace.namespace, &identity.zone),
    ));
    booted.0 = true;
    perf::ambient_boot(&identity.zone, ambient_alias);
    diag::info!(
        Audio,
        "audio: map ambient boot complete for {}",
        identity.zone
    );
}
