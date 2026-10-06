use std::path::PathBuf;

use bevy::prelude::*;
use bevy::tasks::{Task, futures_lite::future};
use frame::{ClientSet, LaunchIdentity, MapLoadApproved, MapLoadFailed};

use crate::map_load_process::MapLoadProcess;
use crate::session_load::{MatchLoadOutcome, PreparedMatch, load_prepared_match};
use asset_transport::progress::LoadProgress;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapLoadApproval;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchLoadDispatch;

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MatchLoadBusy(pub bool);

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchLoadAbort(pub u64);

#[derive(Resource)]
pub struct MatchLoadRequest {
    pub request_id: u64,
    pub load_key: frame::LocalLoadKey,

    pub zone: String,
    pub zone_ff: Result<PathBuf, String>,
    pub common_mp: Result<PathBuf, String>,
    pub progress: LoadProgress,
}

#[derive(Resource)]
struct MatchLoadTask {
    task: Task<Result<Option<PreparedMatchReady>, MapLoadFailed>>,
    request_id: u64,
    progress: LoadProgress,
}

#[derive(Resource, Clone, Debug)]
pub struct MatchLoadAccepted {
    pub request_id: u64,
    pub load_key: frame::LocalLoadKey,
    pub zone: String,
}

#[derive(Resource)]
pub struct PreparedMatchReady {
    pub request_id: u64,
    pub load_key: frame::LocalLoadKey,
    pub zone: String,
    pub prepared: Option<PreparedMatch>,
}

#[derive(Resource)]
pub struct PreparedMatchSound {
    pub namespace: asset_core::AssetNamespace,
    pub load_key: frame::LocalLoadKey,
    pub zone: String,
    pub common_profile_id: u64,
    pub products_id: u64,
    pub sound: Result<asset_audio::SoundCatalog, String>,
    pub gaps: usize,
}

fn start_match_load(
    mut commands: Commands,
    identity: Option<Res<LaunchIdentity>>,
    request: Option<Res<MatchLoadRequest>>,
    busy: Option<Res<MatchLoadTask>>,
    retiring: Option<Res<frame::Retiring>>,
    abort: Option<Res<MatchLoadAbort>>,
    mut waiting_for_retirement: Local<Option<u64>>,
    mut inflight: ResMut<MatchLoadBusy>,
    mut failed: MessageWriter<MapLoadFailed>,
) {
    let Some(request) = request else {
        return;
    };
    if abort
        .as_ref()
        .is_some_and(|abort| abort.0 == request.request_id)
    {
        *waiting_for_retirement = None;
        commands.remove_resource::<MatchLoadRequest>();
        if busy.is_none() {
            inflight.0 = false;
        }
        diag::info!(
            World,
            "match load: abort request #{} before the walk started",
            request.request_id
        );
        return;
    }
    if busy.is_some() {
        return;
    }
    if retiring.as_ref().is_some_and(|retiring| retiring.busy()) {
        if *waiting_for_retirement != Some(request.request_id) {
            diag::info!(
                World,
                "match load: request #{} waits for retired resources",
                request.request_id
            );
            *waiting_for_retirement = Some(request.request_id);
        }
        return;
    }
    if *waiting_for_retirement == Some(request.request_id) {
        diag::info!(
            World,
            "match load: request #{} resumes after retirement",
            request.request_id
        );
    }
    *waiting_for_retirement = None;
    if let Some(error) = request
        .zone_ff
        .as_ref()
        .err()
        .or_else(|| request.common_mp.as_ref().err())
    {
        failed.write(MapLoadFailed {
            request_id: request.request_id,
            load_key: request.load_key,
            zone: request.zone.clone(),
            error: error.clone(),
        });
        commands.remove_resource::<MatchLoadRequest>();
        inflight.0 = false;
        return;
    }
    let zone_ff = request.zone_ff.clone();
    let common_mp = request.common_mp.clone();
    let progress = request.progress.clone();
    let request_id = request.request_id;
    let load_key = request.load_key;
    let zone = request.zone.clone();
    let zone_accepted = zone.clone();
    let cancel_handle = progress.clone();
    // The process outlives the overlay and every task this spawns: it is what
    // the table reads, what admission finishes, and what the bench still has
    // once the screen is gone.
    commands.insert_resource(MapLoadProcess::new(
        request_id,
        load_key,
        zone.clone(),
        cancel_handle.clone(),
    ));

    let games = identity.map(|identity| asset_transport::GamesRoot(identity.games_root.clone()));
    let task = crate::session_load::load_pool().spawn(async move {
        let sound_path = zone_ff.clone();
        match load_prepared_match(zone_ff, common_mp, progress.clone()).await {
            MatchLoadOutcome::Ready(mut prepared) => {
                if progress.is_canceled() {
                    return Ok(None);
                }
                prepared.sound = Some(match (games, sound_path) {
                    (Some(games), Ok(path)) => {
                        let sources = asset_audio::gather_sound_sources(&games, &path);
                        let map = prepared
                            .sound
                            .take()
                            .unwrap_or_else(|| Err("the map zone never opened".into()));
                        let bank = asset_audio::compose_sound_bank(
                            sources,
                            &zone,
                            prepared
                                .prepared_map
                                .namespace
                                .expect("prepared map family"),
                            map,
                        );
                        prepared.sound_gaps = bank.gaps.len();
                        prepared.script_sound_aliases = Some(bank.catalog.script_alias_looping());
                        Ok(bank.catalog)
                    }
                    _ => Err("no launch identity or map path to compose the sound bank".into()),
                });
                if progress.is_canceled() {
                    return Ok(None);
                }
                Ok(Some(PreparedMatchReady {
                    request_id,
                    load_key,
                    zone,
                    prepared: Some(prepared),
                }))
            }
            MatchLoadOutcome::Canceled => Ok(None),
            MatchLoadOutcome::Refused(error) => Err(MapLoadFailed {
                request_id,
                load_key,
                zone,
                error,
            }),
        }
    });
    inflight.0 = true;
    commands.insert_resource(MatchLoadAccepted {
        request_id,
        load_key,
        zone: zone_accepted,
    });
    commands.insert_resource(MatchLoadTask {
        task,
        request_id,
        progress: cancel_handle,
    });
    commands.remove_resource::<MatchLoadRequest>();
}

fn approve_map_load(
    mut commands: Commands,
    mut approved: MessageReader<MapLoadApproved>,
    mut failed: MessageWriter<MapLoadFailed>,
    identity: Res<LaunchIdentity>,
    abort: Option<Res<MatchLoadAbort>>,
) {
    for request in approved.read() {
        if abort
            .as_ref()
            .is_some_and(|abort| abort.0 == request.request_id)
        {
            diag::info!(
                World,
                "match load: abort approved request #{} (`{}`) — no walk",
                request.request_id,
                request.zone
            );
            continue;
        }
        let games = asset_transport::GamesRoot(identity.games_root.clone());
        let found = crate::minecraft_map::find_zone_file(&games, &request.zone);
        let zone = found
            .as_ref()
            .map(|zone| zone.zone_name.clone())
            .unwrap_or_else(|_| request.zone.clone());
        let zone_ff = found.map(|zone| zone.path);
        let common_mp = match &zone_ff {
            Ok(path) => asset_transport::find_runtime_common_mp(&games, path).map(|zone| zone.path),
            Err(error) => Err(error.clone()),
        };
        let error = zone_ff
            .as_ref()
            .err()
            .or_else(|| common_mp.as_ref().err())
            .cloned();
        if let Some(error) = error {
            failed.write(MapLoadFailed {
                request_id: request.request_id,
                load_key: request.load_key,
                zone,
                error,
            });
            continue;
        }
        // Its own state, never the live map's: tasks from the request before
        // this one still hold handles, and they must not land in these rows.
        let progress = LoadProgress::new(request.request_id);
        commands.insert_resource(MatchLoadRequest {
            request_id: request.request_id,
            load_key: request.load_key,
            zone,
            zone_ff,
            common_mp,
            progress,
        });
    }
}

fn poll_match_load(
    mut commands: Commands,
    mut task: Option<ResMut<MatchLoadTask>>,
    mut inflight: ResMut<MatchLoadBusy>,
    abort: Option<Res<MatchLoadAbort>>,
    mut failed: MessageWriter<MapLoadFailed>,
) {
    let Some(task) = task.as_deref_mut() else {
        return;
    };

    if abort
        .as_ref()
        .is_some_and(|abort| abort.0 == task.request_id)
        && !task.progress.is_canceled()
    {
        task.progress.cancel();
        diag::info!(
            World,
            "match load: cancel in-flight walk request #{} — the walk returns at its next checkpoint",
            task.request_id
        );
    }
    let Some(outcome) = future::block_on(future::poll_once(&mut task.task)) else {
        return;
    };
    let request_id = task.request_id;
    inflight.0 = false;
    commands.remove_resource::<MatchLoadTask>();
    commands.remove_resource::<MatchLoadAccepted>();
    let outcome = match outcome {
        Ok(ready) => ready,
        Err(error) => {
            if !abort
                .as_ref()
                .is_some_and(|abort| abort.0 == error.request_id)
            {
                failed.write(error);
            }
            return;
        }
    };
    let Some(mut ready) = outcome else {
        diag::info!(
            World,
            "match load: walk request #{request_id} returned canceled — nothing prepared"
        );
        return;
    };
    if abort.is_some_and(|abort| abort.0 == ready.request_id) {
        diag::info!(
            World,
            "match load: discarded aborted walk request #{} (`{}`)",
            ready.request_id,
            ready.zone
        );
        return;
    }
    let Some(prepared) = ready.prepared.as_mut() else {
        return;
    };
    let Some(namespace) = prepared.prepared_map.namespace else {
        diag::error!(World, "prepared match has no family");
        return;
    };
    commands.insert_resource(PreparedMatchSound {
        namespace,
        load_key: ready.load_key,
        zone: ready.zone.clone(),
        common_profile_id: prepared.materials.common_profile_id,
        products_id: prepared.materials.products_id,
        gaps: prepared.sound_gaps,
        sound: prepared
            .sound
            .take()
            .unwrap_or_else(|| Err("the map zone never opened".to_owned())),
    });
    commands.insert_resource(ready);
}

fn expire_match_load_abort(
    mut commands: Commands,
    abort: Option<Res<MatchLoadAbort>>,
    task: Option<Res<MatchLoadTask>>,
    request: Option<Res<MatchLoadRequest>>,
    ready: Option<Res<PreparedMatchReady>>,
) {
    if abort.is_none() {
        return;
    }
    if task.is_some() || request.is_some() || ready.is_some() {
        return;
    }
    commands.remove_resource::<MatchLoadAbort>();
}

pub fn register_match_load_systems(app: &mut App) {
    app.init_resource::<MatchLoadBusy>().add_systems(
        Update,
        (
            approve_map_load.in_set(MapLoadApproval),
            (
                start_match_load,
                poll_match_load.after(start_match_load),
                expire_match_load_abort.after(poll_match_load),
            )
                .in_set(MatchLoadDispatch)
                .after(MapLoadApproval),
        )
            .after(frame::SessionSwapApplied)
            .in_set(ClientSet::Load),
    );
}
