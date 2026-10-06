use bevy::prelude::*;
use frame::{AppScreen, ClassEquipResolved, LaunchReport, LocalSpawnArmed, RuntimeRole};
use net::{
    AuthorityInputGate, ClientActionInbox, ClientPredictionState, ClientSet, LocalPresentClient,
    LookState, PresentedSnapshot, look_angles_from_degrees,
};
use sim::ClientAction;

use render_frontend::prepare::scene::camera::SimCamera;

pub fn arm_local_from_presented(
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    policy: Res<crate::SessionReadinessPolicy>,
    mut armed: ResMut<LocalSpawnArmed>,
    role: Res<RuntimeRole>,
    generation: Res<frame::WorldGeneration>,
    mut screen: ResMut<AppScreen>,
    sim_cam: Option<ResMut<SimCamera>>,
    mut look: ResMut<LookState>,
) {
    if !role.runs_client() || armed.armed_for(*generation) {
        return;
    }

    if !policy.admission_allowed || policy.generation != *generation {
        return;
    }
    let Some(ps) = presented.alive_player(local.0) else {
        return;
    };

    look.angles = look_angles_from_degrees(std::array::from_fn(|axis| {
        ps.viewangles[axis] - ps.delta_angles[axis]
    }));
    if let Some(mut sim_cam) = sim_cam {
        sim_cam.enabled = true;
        sim_cam.freeze_fly = false;
    }

    *screen = AppScreen::InGame;

    diag::info!(
        Fpv,
        "spawn: presented Alive at [{:.1}, {:.1}, {:.1}] yaw={:.0} weapon={}",
        ps.origin[0],
        ps.origin[1],
        ps.origin[2],
        ps.viewangles[1],
        ps.weapon
    );
    armed.0 = *generation;
}

fn apply_local_input_policy(
    policy: Res<crate::SessionReadinessPolicy>,
    generation: Res<frame::WorldGeneration>,
    signon: Res<net::SignonState>,
    armed: Res<LocalSpawnArmed>,
    mut gate: ResMut<AuthorityInputGate>,
) {
    gate.local_cmds_enabled = policy.local_input_allowed(
        *generation,
        armed.armed_for(*generation),
        signon.phase.is_failed(),
    );
}

fn guard_local_input_before_prediction(
    policy: Res<crate::SessionReadinessPolicy>,
    generation: Res<frame::WorldGeneration>,
    signon: Res<net::SignonState>,
    armed: Res<LocalSpawnArmed>,
    mut gate: ResMut<AuthorityInputGate>,
) {
    gate.local_cmds_enabled = policy.local_input_allowed(
        *generation,
        armed.armed_for(*generation),
        signon.phase.is_failed(),
    );
}

pub fn join_local_on_class_select(
    screen: Res<AppScreen>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    mut actions: ResMut<ClientActionInbox>,
    mut request_ids: ResMut<net::ActionRequestIds>,
    role: Option<Res<RuntimeRole>>,
    policy: Res<crate::SessionReadinessPolicy>,
    mut joining: Local<Option<sim::ActionRequestId>>,
) {
    if !matches!(*screen, AppScreen::ClassSelect) {
        *joining = None;
        return;
    }
    if role.is_some_and(|role| *role == RuntimeRole::Replay) {
        return;
    }
    if !policy.admission_allowed {
        return;
    }
    // The authority lists a client as Connecting before it joins; only a
    // later lifecycle means the join landed.
    if presented
        .snapshot()
        .and_then(|s| s.meta.for_client(local.0))
        .is_some_and(|meta| meta.lifecycle != sim::ClientLifecycle::Connecting)
    {
        *joining = None;
        return;
    }

    if let Some(request_id) = *joining {
        if actions
            .pending_for(local.0)
            .any(|action| sim::action_request_id(&action) == request_id)
        {
            return;
        }

        *joining = None;
    }
    let request_id = request_ids.allocate();
    match actions.push(local.0, ClientAction::JoinMatch { request_id }) {
        Ok(()) => {
            *joining = Some(request_id);
            diag::info!(
                Fpv,
                "class select: queued JoinMatch request_id={request_id} \
                 (meta-only; spectator pm_type is a named gap)"
            );
        }
        Err(error) => diag::warn!(Fpv, "class select: JoinMatch not queued — {error}"),
    }
}

pub fn sync_prediction_metrics_to_probe(
    prediction: Res<ClientPredictionState>,
    mut report: Option<ResMut<LaunchReport>>,
) {
    if !prediction.is_changed() {
        return;
    }
    let Some(report) = report.as_deref_mut() else {
        return;
    };
    report.prediction_metrics = Some(prediction.0.metrics().report_line());
}

pub fn register_local_arm_systems(app: &mut App) {
    app.init_resource::<LocalSpawnArmed>()
        .add_systems(
            Update,
            (
                arm_local_from_presented.after(ClassEquipResolved),
                join_local_on_class_select.after(crate::admission::update_admission),
                apply_local_input_policy.after(arm_local_from_presented),
            )
                .in_set(ClientSet::Present),
        )
        .add_systems(
            Update,
            guard_local_input_before_prediction
                .in_set(ClientSet::Predict)
                .after(net::client::runtime::enforce_client_work_limits)
                .before(net::client::runtime::predict_local_move),
        )
        .add_systems(
            Update,
            sync_prediction_metrics_to_probe.in_set(ClientSet::Diag),
        );
}
