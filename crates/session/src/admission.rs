use assets::LoadingScreen;
use bevy::prelude::*;
use frame::{AppScreen, ClientSet, HasWorld, RuntimeRole};
use net::{AuthorityLoadHold, ClientAdmission, SignonPhase, SignonState};
use render_frontend::prepare::scene::world::WorldScene;

use crate::LiveWorldIdentity;

pub fn update_admission(
    skate: Option<Res<frame::SkateMode>>,
    mut signon: ResMut<SignonState>,
    mut admission: ResMut<ClientAdmission>,
    role: Res<RuntimeRole>,
    mut hold: Option<ResMut<AuthorityLoadHold>>,
    has_world: Option<Res<HasWorld>>,
    scene: Option<Res<WorldScene>>,
    audio: Option<Res<audio::AudioReady>>,
    mut live: Option<ResMut<LiveWorldIdentity>>,
    headless: Option<Res<frame::Headless>>,
    generation: Res<frame::WorldGeneration>,
    navigation: Option<Res<frame::BotNavigationReady>>,
    mut policy: ResMut<crate::SessionReadinessPolicy>,
    minecraft: Option<Res<frame::MinecraftUi>>,
) {
    if let (Some(live), Some(installed)) = (live.as_mut(), admission.core.installed())
        && live.load_key.local_load_request_id == installed.local_load_request_id
        && live.load_key.match_key.is_none()
    {
        live.load_key = installed;
    }
    let installed = has_world.is_some_and(|world| world.0)
        && live.as_ref().is_some_and(|live| {
            generation.0 == Some(live.load_key.local_load_request_id)
                && admission.core.installed() == Some(live.load_key)
        });
    let mut decision = crate::readiness::decide_readiness(
        *role,
        headless.is_some(),
        *generation,
        installed,
        navigation.as_ref().map(|report| report.0),
        scene.as_ref().map(|scene| scene.readiness),
        audio.as_ref().map(|report| report.0),
    );
    let mashup_ready = headless.is_some()
        || ((*role != RuntimeRole::Listen || skate.is_none_or(|skate| !skate.preload_pending))
            && minecraft.is_none_or(|ui| !ui.loading_world));
    decision.presentation &= mashup_ready;
    decision.advancement &= mashup_ready;
    if let Some(hold) = hold.as_mut() {
        hold.0 = !decision.advancement;
    }
    if let Some(live) = live.as_ref() {
        admission
            .core
            .apply_presentation(live.load_key, decision.presentation);
        admission
            .core
            .apply_local_authority_ready(live.load_key, decision.advancement);
        if let Some(failure) = decision.failure {
            let failure = net::SessionFail {
                stage: net::FailStage::Load,
                source: failure.source().to_owned(),
                match_key: live.load_key.match_key,
            };
            admission.core.apply_fail(failure.clone());
            signon.phase = SignonPhase::Failed(net::SignonFailReason::Transport {
                source: failure.source,
                stage: failure.stage,
                match_key: failure.match_key,
            });
        }
    }
    let admitted = installed
        && decision.presentation
        && !signon.phase.is_failed()
        && match *role {
            RuntimeRole::Client => admission.core.class_select_allowed(),
            RuntimeRole::Listen | RuntimeRole::Dedicated => {
                admission.core.local_class_select_allowed()
            }
            RuntimeRole::Replay => !signon.phase.is_failed(),
        };
    *policy = crate::SessionReadinessPolicy {
        load_key: live
            .as_ref()
            .filter(|_| installed)
            .map(|live| live.load_key),
        generation: *generation,
        advancement_allowed: decision.advancement,
        presentation_allowed: decision.presentation,
        admission_allowed: admitted,
        input_allowed: admitted && matches!(*role, RuntimeRole::Listen | RuntimeRole::Client),
    };
    let presentation_ready = decision.presentation;
    let audio_ready = audio
        .as_ref()
        .is_some_and(|report| report.0.ready_for(*generation));
    if signon.admitted != admitted {
        signon.admitted = admitted;
        diag::info!(
            Sim,
            "admission admitted={admitted} role={role:?} phase={:?} presentation={presentation_ready} audio={audio_ready}",
            signon.phase
        );
    }
}

pub fn drive_class_select_screen(
    mut screen: ResMut<AppScreen>,
    mut loading: Option<ResMut<LoadingScreen>>,
    mut load: Option<ResMut<assets::MapLoadProcess>>,
    signon: Res<SignonState>,
    has_world: Option<Res<HasWorld>>,
) {
    let world_installed = has_world.is_some_and(|world| world.0);
    let admitted = signon.may_select_class();
    if signon.phase.is_failed() {
        if let Some(loading) = loading.as_deref_mut()
            && let SignonPhase::Failed(reason) = &signon.phase
        {
            if loading.failure().is_none() {
                loading.fail(reason.to_string());
            }
        }
        if let Some(load) = load.as_deref_mut() {
            load.fail();
        }
        if matches!(*screen, AppScreen::ClassSelect | AppScreen::InGame) {
            *screen = AppScreen::MainMenu;
        }
        return;
    }
    if !admitted || !world_installed {
        // Local preparation can be finished long before the session says this
        // client may continue. That wait is the load's own row, so the table
        // can say what is holding it rather than showing everything green.
        if world_installed && let Some(load) = load.as_deref_mut() {
            load.await_admission();
        }
        return;
    }
    // The load ends here, with or without a screen over it.
    if let Some(load) = load.as_deref_mut() {
        load.finish();
    }
    // The overlay comes down on its own terms, the class-select hop on the
    // screen's. A replay is already `InGame` by the time this runs — the first
    // presented snapshot arms it in `ClientSet::Present`, one set ahead of here —
    // and gating the overlay on the screen too left it up for the whole demo,
    // hiding the world behind `UiLayer::Loading` and holding back map ambience.
    if let Some(loading) = loading.as_deref_mut() {
        loading.finish();
    }
    if matches!(*screen, AppScreen::Loading | AppScreen::MainMenu) {
        *screen = AppScreen::ClassSelect;
    }
}

pub fn register_admission(app: &mut App) {
    app.init_resource::<crate::SessionReadinessPolicy>()
        .add_systems(
            Update,
            update_admission
                .in_set(ClientSet::Present)
                .before(crate::local_arm::arm_local_from_presented),
        )
        .add_systems(Update, drive_class_select_screen.in_set(ClientSet::Ui));
}
