use std::sync::Arc;

use asset_audio::{SoundCatalog, load_mp_sound_bank};
use asset_transport::GamesRoot;
use assets::{NamespaceSoundIwd, NamespaceTrees};
use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, TaskPool, futures_lite::future},
};
use frame::{
    ClientSet, LaunchIdentity, ReturnedToMenu, UiPlayMusic, UiPlaySound, UiStopMusic,
    register_ui_contracts,
};

use crate::{
    ClipStore, SoundClass,
    ambient::SoundIwd,
    clip_store::CueFeedback,
    playback::{MissingAliasGaps, SoundBank, play_alias_oneshot},
    start::StartDecisions,
};

#[derive(Resource, Clone)]
pub struct FrontendAudio {
    pub bank: Arc<SoundCatalog>,
    pub iwd: Arc<NamespaceSoundIwd>,
}

const FRONTEND_SOUND_ZONE: &str = "iw4:code_post_gfx_mp";

#[derive(Resource)]
struct FrontendAudioPrepare(Task<FrontendAudioWalked>);

struct FrontendAudioWalked {
    bank: Option<Arc<SoundCatalog>>,
    iwd: Arc<NamespaceSoundIwd>,
    lines: Vec<String>,
}

#[derive(Resource, Default)]
pub(crate) struct MenuSources {
    alias: Option<String>,
    next_version: u64,
    pub source: Option<crate::sources::DesiredSource>,
}

pub(crate) fn register_frontend_audio(app: &mut App) {
    register_ui_contracts(app);
    app.init_resource::<MenuSources>()
        .add_message::<ReturnedToMenu>()
        .add_systems(
            Update,
            (
                start_frontend_audio_prepare,
                install_frontend_audio_prepare.after(start_frontend_audio_prepare),
                restore_frontend_audio_on_menu
                    .after(install_frontend_audio_prepare)
                    .after(crate::ambient::stop_map_ambient_on_match_end),
            )
                .in_set(ClientSet::Load),
        )
        .add_systems(
            Update,
            (play_ui_sound_messages, play_ui_music_messages).in_set(ClientSet::Effects),
        );
}

fn start_frontend_audio_prepare(
    frontend: Option<Res<FrontendAudio>>,
    running: Option<Res<FrontendAudioPrepare>>,
    identity: Option<Res<LaunchIdentity>>,
    silent: Option<Res<crate::AudioSilent>>,
    loading: (
        Option<Res<assets::MatchLoadBusy>>,
        Option<Res<assets::MatchLoadRequest>>,
        Option<Res<assets::MatchLoadAccepted>>,
        Option<Res<assets::PreparedMatchReady>>,
        Option<Res<crate::ambient::SoundBankCompose>>,
    ),
    mut commands: Commands,
) {
    if frontend.is_some() || running.is_some() || silent.is_some() {
        return;
    }
    let (busy, request, accepted, ready, walk) = loading;
    if busy.is_some_and(|busy| busy.0)
        || request.is_some()
        || accepted.is_some()
        || ready.is_some()
        || walk.is_some()
    {
        return;
    }
    let Some(identity) = identity else {
        return;
    };
    if identity.games_root.as_os_str().is_empty() {
        return;
    }
    let games = GamesRoot(identity.games_root.clone());
    let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
    let task = pool.spawn(async move {
        let mut lines = Vec::new();
        let bank = match load_mp_sound_bank(&games, FRONTEND_SOUND_ZONE) {
            Ok(loaded) => {
                lines.extend(loaded.gap_lines());
                Some(Arc::new(loaded.catalog))
            }
            Err(error) => {
                lines.push(format!("frontend sound bank: {error}"));
                None
            }
        };
        let (indices, open_lines) = NamespaceSoundIwd::open(&NamespaceTrees::discover(&games));
        lines.extend(open_lines);
        FrontendAudioWalked {
            bank,
            iwd: Arc::new(indices),
            lines,
        }
    });
    commands.insert_resource(FrontendAudioPrepare(task));
}

fn install_frontend_audio_prepare(
    mut prepare: Option<ResMut<FrontendAudioPrepare>>,
    mut commands: Commands,
) {
    let Some(prepare) = prepare.as_deref_mut() else {
        return;
    };
    let Some(walked) = future::block_on(future::poll_once(&mut prepare.0)) else {
        return;
    };
    commands.remove_resource::<FrontendAudioPrepare>();
    for line in walked.lines {
        diag::info!(Audio, "audio: frontend {line}");
    }
    let Some(bank) = walked.bank else {
        diag::warn!(
            Audio,
            "audio: frontend sound bank did not build — the menu stays silent (typed gap)"
        );
        return;
    };
    commands.insert_resource(FrontendAudio {
        bank,
        iwd: walked.iwd,
    });
    diag::info!(Audio, "audio: frontend sound resident");
}

pub(crate) fn restore_frontend_audio_on_menu(
    mut returned: MessageReader<ReturnedToMenu>,
    frontend: Option<Res<FrontendAudio>>,
    live: Option<Res<SoundBank>>,
    silent: Option<Res<crate::AudioSilent>>,
    mut commands: Commands,
) {
    if returned.read().count() == 0 || silent.is_some() {
        return;
    }
    let Some(frontend) = frontend else {
        diag::warn!(
            Audio,
            "audio: no resident frontend bank — the menu runs without sound (typed gap)"
        );
        return;
    };
    if live.is_some_and(|live| Arc::ptr_eq(&live.0, &frontend.bank)) {
        return;
    }
    commands.insert_resource(CueFeedback::default());
    commands.insert_resource(SoundBank(Arc::clone(&frontend.bank)));
    commands.insert_resource(SoundIwd(Arc::clone(&frontend.iwd)));
    commands.insert_resource(ClipStore::start(
        Arc::clone(&frontend.bank),
        Some(Arc::clone(&frontend.iwd)),
    ));
    diag::info!(Audio, "audio: frontend sound restored");
}

fn play_ui_sound_messages(
    mut events: MessageReader<UiPlaySound>,
    runtime: Res<crate::AudioRuntime>,
    mut gaps: ResMut<MissingAliasGaps>,
    mut pending: ResMut<CueFeedback>,
    mut decisions: ResMut<StartDecisions>,
    bank: Option<Res<SoundBank>>,
    epoch: Res<crate::backend::MatchEpoch>,
) {
    let Some(bank) = bank else {
        for event in events.read().filter(|_| !crate::AudioSilent::active()) {
            diag::warn!(
                Audio,
                "audio: UiPlaySound `{}` dropped — no SoundBank (typed gap)",
                event.alias
            );
        }
        return;
    };
    for event in events.read() {
        if event.alias.is_empty() {
            continue;
        }
        let outcome = play_alias_oneshot(
            &bank.0,
            asset_core::AssetNamespace::Iw4,
            &event.alias,
            None,
            &runtime,
            &mut pending,
            &mut decisions,
            Some(crate::SND_ENT_LOCAL),
            SoundClass::Ui,
            epoch.0,
        );
        if outcome.allows_binding_fallback() {
            gaps.record(&event.alias);
        }
    }
}

fn play_ui_music_messages(
    mut play_events: MessageReader<UiPlayMusic>,
    mut stop_events: MessageReader<UiStopMusic>,
    mut desired: ResMut<MenuSources>,
    bank: Option<Res<SoundBank>>,
    runtime: Res<crate::AudioRuntime>,
) {
    if stop_events.read().count() > 0 {
        desired.alias = None;
        desired.source = None;
    }
    for event in play_events.read() {
        if !event.alias.is_empty() {
            desired.alias = Some(event.alias.clone());
        }
    }
    let Some(alias) = desired.alias.clone() else {
        return;
    };
    if desired
        .source
        .as_ref()
        .is_some_and(|source| source.cue.alias == alias)
    {
        return;
    }
    desired.source = None;
    let Some(bank) = bank else {
        return;
    };
    let Some(cue) = runtime.source_cue(crate::sources::SourceCueRequest {
        bank: bank.0.clone(),
        namespace: asset_core::AssetNamespace::Iw4,
        alias,
        emitter: None,
        scope: crate::backend::AudioScope::Menu,
        epoch: 0,
        group: None,
    }) else {
        return;
    };
    desired.next_version = desired
        .next_version
        .checked_add(1)
        .expect("source version exhausted");
    desired.source = Some(crate::sources::DesiredSource {
        key: crate::sources::SourceKey {
            scope: crate::backend::AudioScope::Menu,
            epoch: 0,
            object: 0,
            slot: 4,
        },
        version: desired.next_version,
        cue,
        origin_inches: None,
        start_frame: runtime.audio_frame(),
        gain: 0.55,
        rate: 1.0,
        audible: true,
    });
}
