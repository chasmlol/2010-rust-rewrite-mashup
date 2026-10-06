use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, TaskPool};
use frame::{
    AppScreen, ClassSelectHandoff, HasWorld, LaunchIdentity, LocalLoadKey, MapLoadApproved,
    MapLoadFailed, MatchInstalled, MatchKey, MatchTornDown, Retiring, ReturnedToMenu, RuntimeRole,
    TeardownReason, WorldGeneration,
};
use net::{AuthorityLoadHold, ClientSet, MatchDescriptor, PresentedSnapshot};

#[derive(Resource, Default, Debug)]
pub struct TeardownRequest(pub Option<TeardownReason>);

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct LiveWorldIdentity {
    pub load_key: LocalLoadKey,
}

#[derive(Resource, Debug, Default)]
pub struct SessionSwapRequest {
    next_id: u64,
    pending: Option<PendingSessionSwap>,
    completed: Option<SessionSwapCompletion>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionSwapTarget {
    Zone(String),

    Demo {
        name: String,
        zone: String,
        quit_on_end: bool,
    },

    Menu {
        leave_session: bool,
    },
}

#[derive(Debug)]
struct PendingSessionSwap {
    id: u64,
    target: SessionSwapTarget,
    phase: SessionSwapPhase,

    abort_install: bool,

    tore_down_world: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionSwapPhase {
    Requested,
    WaitingForTeardown,
    WaitingForInstall,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSwapCompletion {
    pub id: u64,
    pub result: SessionSwapResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionSwapResult {
    Installed { zone: String },
    Menu,
    Failed { zone: String, error: String },
}

impl SessionSwapRequest {
    pub(crate) fn accepts_install(&self, request_id: u64) -> bool {
        match &self.pending {
            Some(pending) => {
                pending.id == request_id
                    && pending.phase == SessionSwapPhase::WaitingForInstall
                    && !matches!(pending.target, SessionSwapTarget::Menu { .. })
            }
            None => request_id == 0 && self.next_id == 0,
        }
    }

    pub fn request_zone(&mut self, zone: String) -> Result<u64, String> {
        self.begin(SessionSwapTarget::Zone(zone))
    }

    pub fn request_demo(
        &mut self,
        name: String,
        zone: String,
        quit_on_end: bool,
    ) -> Result<u64, String> {
        self.begin(SessionSwapTarget::Demo {
            name,
            zone,
            quit_on_end,
        })
    }

    pub fn request_menu(&mut self) -> Result<u64, String> {
        self.request_return(false)
    }

    pub fn request_leave(&mut self) -> Result<u64, String> {
        self.request_return(true)
    }

    fn request_return(&mut self, leave_session: bool) -> Result<u64, String> {
        let Some(pending) = self.pending.as_mut() else {
            return self.begin(SessionSwapTarget::Menu { leave_session });
        };
        let verb = if leave_session {
            "disconnect"
        } else {
            "match end"
        };
        match &mut pending.target {
            SessionSwapTarget::Menu {
                leave_session: already,
            } => {
                *already = *already || leave_session;
                Ok(pending.id)
            }
            SessionSwapTarget::Zone(zone) => {
                diag::info!(
                    Sim,
                    "session: {verb} superseded in-flight map `{zone}` (swap #{}) → menu",
                    pending.id
                );
                pending.target = SessionSwapTarget::Menu { leave_session };
                pending.abort_install = true;
                Ok(pending.id)
            }
            SessionSwapTarget::Demo { name, .. } => {
                diag::info!(
                    Sim,
                    "session: {verb} superseded in-flight demo `{name}` (swap #{}) → menu",
                    pending.id
                );
                pending.target = SessionSwapTarget::Menu { leave_session };
                pending.abort_install = true;
                Ok(pending.id)
            }
        }
    }

    fn begin(&mut self, target: SessionSwapTarget) -> Result<u64, String> {
        if self.pending.is_some() {
            return Err("session swap already in progress".into());
        }
        self.next_id = self.next_id.saturating_add(1);
        let id = self.next_id;
        let target_s = match &target {
            SessionSwapTarget::Zone(zone) => format!("zone:{zone}"),
            SessionSwapTarget::Demo { name, .. } => format!("demo:{name}"),
            SessionSwapTarget::Menu {
                leave_session: true,
            } => "leave".into(),
            SessionSwapTarget::Menu { .. } => "menu".into(),
        };
        self.pending = Some(PendingSessionSwap {
            id,
            target,
            phase: SessionSwapPhase::Requested,
            abort_install: false,
            tore_down_world: false,
        });
        perf::swap(id, "requested", &target_s);
        Ok(id)
    }

    pub fn take_completed(&mut self) -> Option<SessionSwapCompletion> {
        self.completed.take()
    }

    pub fn dump_id(&self) -> Option<u64> {
        self.pending.as_ref().map(|pending| pending.id)
    }
}

#[derive(Resource, Default, Debug)]
pub struct TeardownGaps(pub Vec<&'static str>);

pub fn run_teardown(
    mut commands: Commands,
    mut request: ResMut<TeardownRequest>,
    mut screen: ResMut<AppScreen>,
    mut has_world: ResMut<HasWorld>,
    mut world_generation: ResMut<WorldGeneration>,
    mut readiness: ResMut<crate::SessionReadinessPolicy>,
    mut armed: ResMut<frame::LocalSpawnArmed>,
    mut handoff: ResMut<ClassSelectHandoff>,
    mut identity: Option<ResMut<LaunchIdentity>>,
    mut load_hold: Option<ResMut<AuthorityLoadHold>>,
    live_world: Option<Res<LiveWorldIdentity>>,
    signon: Option<Res<net::SignonState>>,
    bridge: Option<Res<net::MasterBridge>>,
    mut gaps: ResMut<TeardownGaps>,
    mut retiring: ResMut<Retiring>,
    mut torn: MessageWriter<MatchTornDown>,
) {
    let Some(reason) = request.0.take() else {
        return;
    };

    if let Some(bridge) = bridge.as_ref() {
        bridge.set_installed_load(None);
    }
    let torn_generation = *world_generation;
    let match_key = live_world
        .map(|live| live.load_key.match_key)
        .unwrap_or_else(|| {
            bridge
                .as_ref()
                .map(|bridge| bridge.state().identity().match_key())
                .filter(|key| !key.is_none())
                .unwrap_or_else(|| {
                    MatchKey::new([0; 16], signon.map(|signon| signon.epoch.0).unwrap_or(0))
                })
        });

    commands.remove_resource::<crate::SessionContentManifest>();
    commands.remove_resource::<MatchDescriptor>();
    commands.insert_resource(LiveWorldIdentity::default());

    *screen = AppScreen::MainMenu;
    *has_world = HasWorld(false);
    *world_generation = WorldGeneration(None);
    *readiness = crate::SessionReadinessPolicy::default();
    *armed = frame::LocalSpawnArmed::default();
    *handoff = ClassSelectHandoff::default();

    if let Some(hold) = load_hold.as_mut() {
        hold.0 = true;
    }

    if let Some(identity) = identity.as_mut() {
        identity.zone.clear();
    }

    gaps.0 = vec![
        "Bevy Assets<Image/Mesh> handles dropped by WorldScene::default stay \
         until Bevy GC; they are not drawable leftover world",
        "WorldScene Resource stays so hold still freezes level.time; render \
         empties geometry / tess / GPU plans",
        "AuthorityWorld Resource stays; clip is SimWorld::shutdown_game, not remove",
    ];

    diag::info!(
        Sim,
        "session: match torn down ({reason:?}); teardown ran, {} declared gaps",
        gaps.0.len()
    );
    perf::match_torn(reason.label());
    diag::lifecycle_boundary(
        "local_session_revoked",
        &format!(" reason={}", reason.label()),
    );
    retiring.mark_teardown();
    torn.write(MatchTornDown {
        reason,
        world_generation: torn_generation,
        match_key,
        match_epoch: match_key.match_epoch,
    });
}

fn stamp_loaded_zone(
    mut installed: MessageReader<MatchInstalled>,
    mut identity: Option<ResMut<LaunchIdentity>>,
) {
    let Some(fact) = installed.read().last() else {
        return;
    };
    if let Some(identity) = identity.as_mut() {
        identity.zone = fact.zone.clone();
    }
}

fn stamp_runtime_role(
    role: &mut Option<ResMut<RuntimeRole>>,
    identity: &mut Option<ResMut<LaunchIdentity>>,
    next: RuntimeRole,
) {
    if let Some(role) = role.as_mut() {
        **role = next;
    }
    if let Some(identity) = identity.as_mut() {
        identity.role_label = format!("{next:?}");
    }
}

fn teardown_reason_for(target: &SessionSwapTarget) -> TeardownReason {
    match target {
        SessionSwapTarget::Menu {
            leave_session: true,
        } => TeardownReason::Disconnect,
        SessionSwapTarget::Menu { .. } => TeardownReason::MatchEnded,
        SessionSwapTarget::Zone(_) | SessionSwapTarget::Demo { .. } => TeardownReason::Replaced,
    }
}

fn load_key_for_swap(request_id: u64, bridge: Option<&net::MasterBridge>) -> LocalLoadKey {
    let Some(bridge) = bridge else {
        return LocalLoadKey::from_request(request_id, MatchKey::NONE, 0);
    };
    match bridge.state() {
        net::MasterBridgeState::Hosting { .. } => {
            LocalLoadKey::from_request(request_id, MatchKey::NONE, bridge.incarnation())
        }
        net::MasterBridgeState::Joined {
            identity,
            in_match: true,
            ..
        } if identity.epoch != 0 => LocalLoadKey::from_request(
            request_id,
            MatchKey::new(identity.room_id.0, identity.epoch),
            bridge.incarnation(),
        ),
        _ => LocalLoadKey::from_request(request_id, MatchKey::NONE, 0),
    }
}

fn occupy_after_teardown(
    pending: &mut PendingSessionSwap,
    role: &mut Option<ResMut<RuntimeRole>>,
    identity: &mut Option<ResMut<LaunchIdentity>>,
    approved: &mut MessageWriter<MapLoadApproved>,
    menu: &mut MessageWriter<ReturnedToMenu>,
    leave: &mut Option<ResMut<net::PendingMasterMenuAction>>,
    bridge: Option<&net::MasterBridge>,
) -> Option<SessionSwapCompletion> {
    match pending.target.clone() {
        SessionSwapTarget::Menu { leave_session } => {
            stamp_runtime_role(role, identity, RuntimeRole::Listen);

            if leave_session
                && bridge.is_some()
                && let Some(action) = leave.as_mut()
                && action.0.is_none()
            {
                action.0 = Some(net::MasterMenuAction::LeaveLobby);
            }
            menu.write(ReturnedToMenu {
                swap_id: pending.id,
                had_world: pending.tore_down_world,
            });
            Some(SessionSwapCompletion {
                id: pending.id,
                result: SessionSwapResult::Menu,
            })
        }
        SessionSwapTarget::Zone(zone) => {
            let keep_client = role
                .as_ref()
                .is_some_and(|role| **role == RuntimeRole::Client);
            stamp_runtime_role(
                role,
                identity,
                if keep_client {
                    RuntimeRole::Client
                } else {
                    RuntimeRole::Listen
                },
            );
            approved.write(MapLoadApproved {
                request_id: pending.id,
                load_key: load_key_for_swap(pending.id, bridge),
                zone,
            });
            pending.phase = SessionSwapPhase::WaitingForInstall;
            None
        }
        SessionSwapTarget::Demo { zone, .. } => {
            stamp_runtime_role(role, identity, RuntimeRole::Replay);
            approved.write(MapLoadApproved {
                request_id: pending.id,
                load_key: load_key_for_swap(pending.id, bridge),
                zone,
            });
            pending.phase = SessionSwapPhase::WaitingForInstall;
            None
        }
    }
}

fn run_session_swap(
    mut commands: Commands,
    mut transition: ResMut<SessionSwapRequest>,
    has_world: Option<Res<HasWorld>>,
    mut teardown: ResMut<TeardownRequest>,
    mut role: Option<ResMut<RuntimeRole>>,
    mut identity: Option<ResMut<LaunchIdentity>>,
    mut approved: MessageWriter<MapLoadApproved>,
    mut torn: MessageReader<MatchTornDown>,
    mut failed: MessageReader<MapLoadFailed>,
    mut dvars: ResMut<frame::UiMenuDvars>,
    mut installed: MessageReader<MatchInstalled>,
    mut menu: MessageWriter<ReturnedToMenu>,
    mut leave: Option<ResMut<net::PendingMasterMenuAction>>,
    bridge: Option<Res<net::MasterBridge>>,
) {
    let failure = failed
        .read()
        .find(|fact| transition.accepts_install(fact.request_id));
    let mut finish: Option<SessionSwapCompletion> = None;
    if let Some(fact) = failure {
        let had_world = transition
            .pending
            .as_ref()
            .is_some_and(|pending| pending.tore_down_world);
        stamp_runtime_role(&mut role, &mut identity, RuntimeRole::Listen);
        if let Some(identity) = identity.as_mut() {
            identity.zone.clear();
        }
        let error = fact.error.chars().take(420).collect::<String>();
        let details = if fact.error.chars().count() > 420 {
            format!("{error}…\nSee the log for details.")
        } else {
            error
        };
        dvars.set(
            "ui_map_error_reason",
            format!("Could not load map '{}'.\n\n{details}", fact.zone),
        );
        dvars.set("ui_map_error", "1");
        menu.write(ReturnedToMenu {
            swap_id: fact.request_id,
            had_world,
        });
        diag::warn!(
            Sim,
            "session: map `{}` failed (swap #{}): {} — returned to menu",
            fact.zone,
            fact.request_id,
            fact.error
        );
        finish = Some(SessionSwapCompletion {
            id: fact.request_id,
            result: SessionSwapResult::Failed {
                zone: fact.zone.clone(),
                error: fact.error.clone(),
            },
        });
    } else if let Some(pending) = transition.pending.as_mut() {
        if pending.abort_install {
            commands.insert_resource(assets::MatchLoadAbort(pending.id));
            pending.abort_install = false;
        }
        let expected_teardown = teardown_reason_for(&pending.target);
        let menu_accepts_replaced = matches!(pending.target, SessionSwapTarget::Menu { .. });
        match pending.phase {
            SessionSwapPhase::Requested if has_world.is_some_and(|world| world.0) => {
                teardown.0 = Some(expected_teardown);
                pending.tore_down_world = true;
                pending.phase = SessionSwapPhase::WaitingForTeardown;
            }
            SessionSwapPhase::Requested | SessionSwapPhase::WaitingForTeardown
                if matches!(pending.phase, SessionSwapPhase::Requested)
                    || torn.read().any(|fact| {
                        fact.reason == expected_teardown
                            || (menu_accepts_replaced && fact.reason == TeardownReason::Replaced)
                    }) =>
            {
                finish = occupy_after_teardown(
                    pending,
                    &mut role,
                    &mut identity,
                    &mut approved,
                    &mut menu,
                    &mut leave,
                    bridge.as_deref(),
                );
            }
            SessionSwapPhase::WaitingForInstall
                if matches!(pending.target, SessionSwapTarget::Menu { .. }) =>
            {
                finish = occupy_after_teardown(
                    pending,
                    &mut role,
                    &mut identity,
                    &mut approved,
                    &mut menu,
                    &mut leave,
                    bridge.as_deref(),
                );
            }
            SessionSwapPhase::WaitingForInstall
                if let Some(fact) = installed.read().find(|fact| fact.request_id == pending.id) =>
            {
                finish = Some(SessionSwapCompletion {
                    id: pending.id,
                    result: SessionSwapResult::Installed {
                        zone: fact.zone.clone(),
                    },
                });
            }
            _ => {}
        }
    }
    if let Some(completed) = finish {
        let done_target = match &completed.result {
            SessionSwapResult::Installed { zone } => format!("installed:{zone}"),
            SessionSwapResult::Menu => "menu".into(),
            SessionSwapResult::Failed { zone, .. } => format!("failed:{zone}"),
        };
        perf::swap(completed.id, "done", &done_target);
        transition.pending = None;
        transition.completed = Some(completed);
    }
}

fn run_exit_level(
    mut called: MessageReader<frame::ExitLevelCalled>,
    mut transition: ResMut<SessionSwapRequest>,
) {
    if called.read().next().is_none() {
        return;
    }
    match transition.request_menu() {
        Ok(id) => diag::info!(Sim, "session: exitLevel( false ) → lobby (swap #{id})"),
        Err(err) => diag::info!(Sim, "session: exitLevel refused — {err}"),
    }
}

fn retire_handed_over(mut retiring: ResMut<Retiring>) {
    let (batch, teardown) = retiring.take_batch();
    if batch.is_empty() {
        return;
    }
    let values = batch.len();
    let guard = retiring.ship(teardown);
    AsyncComputeTaskPool::get_or_init(TaskPool::default)
        .spawn(async move {
            drop(batch);
            if guard.settle() == Some(0) {
                diag::lifecycle_boundary("retirement_settled", &format!(" values={values}"));
            }
        })
        .detach();
    let (jobs, total, waiting) = retiring.counts();
    diag::info!(
        Sim,
        "session: retired {values} value(s) off the frame (batches={jobs} values={total} waiting={waiting})"
    );
    if teardown {
        diag::lifecycle_boundary(
            "old_runtime_teardown_complete",
            &format!(" values={values} in_flight={}", retiring.in_flight()),
        );
    }
}

fn joined_in_match(state: Option<&net::MasterBridgeState>) -> Option<bool> {
    match state {
        Some(net::MasterBridgeState::Joined { in_match, .. }) => Some(*in_match),
        _ => None,
    }
}

fn peer_should_leave_match(was_in_match: bool, in_match: bool) -> bool {
    was_in_match && !in_match
}

fn journal_has_match_ended(presented: Option<&PresentedSnapshot>) -> bool {
    presented.and_then(|p| p.snapshot()).is_some_and(|snap| {
        snap.meta
            .journal
            .iter()
            .any(|r| matches!(r.event, sim::SimEvent::MatchEnded { .. }))
    })
}

fn udp_peer_exit_applies(role: RuntimeRole, master_joined: bool, has_world: bool) -> bool {
    role == RuntimeRole::Client && !master_joined && has_world
}

fn udp_match_ended_rising(latched: bool, journal_has: bool) -> bool {
    journal_has && !latched
}

fn connection_notice(reason: &str) -> String {
    let reason = reason
        .strip_prefix("handshake rejected: ")
        .or_else(|| reason.strip_prefix("join admission: "))
        .unwrap_or(reason);
    let message = if reason.starts_with("class catalog mismatch") {
        "Your class rules differ from the host's. Both players need the same game build."
    } else if reason.starts_with("map content mismatch") {
        "Your map data differs from the host's. Both players need the same map data to join."
    } else if reason.starts_with("weapon table mismatch") {
        "Your weapon data differs from the host's. Check that both players use the same build and game content."
    } else {
        reason
    };
    if message.chars().count() > 420 {
        format!(
            "{}…\nSee the log for details.",
            message.chars().take(420).collect::<String>()
        )
    } else {
        message.to_owned()
    }
}

fn run_peer_lobby_return(
    signon: Res<net::SignonState>,
    bridge: Option<Res<net::MasterBridge>>,
    mut transition: ResMut<SessionSwapRequest>,
    mut dvars: ResMut<frame::UiMenuDvars>,
    mut live: Local<bool>,
    mut terminal_cleanup: Local<Option<u64>>,
) {
    if dvars.get("ui_password_pending") == Some("1") {
        return;
    }
    let held = bridge.as_ref().map(|b| b.state());
    if signon.phase.is_failed()
        || matches!(
            held.as_ref(),
            Some(
                net::MasterBridgeState::Failed { .. }
                    | net::MasterBridgeState::Closed { .. }
                    | net::MasterBridgeState::Left { .. }
            )
        )
    {
        let incarnation = bridge
            .as_ref()
            .map(|bridge| bridge.incarnation())
            .unwrap_or(0);
        if *terminal_cleanup != Some(incarnation) {
            match transition.request_menu() {
                Ok(id) => {
                    *terminal_cleanup = Some(incarnation);
                    // Keep the notice outside cgame: teardown clears SignonState
                    // and the menu stack before the frontend becomes interactive.
                    let reason = match held.as_ref() {
                        Some(net::MasterBridgeState::Failed { error, .. }) => {
                            Some(error.source.clone())
                        }
                        Some(net::MasterBridgeState::Closed { reason, .. }) => {
                            Some(net::SignonFailReason::SessionClosed(*reason).to_string())
                        }
                        Some(net::MasterBridgeState::Left { .. }) => None,
                        _ => match &signon.phase {
                            net::SignonPhase::Failed(net::SignonFailReason::Transport {
                                source,
                                ..
                            }) => Some(source.clone()),
                            net::SignonPhase::Failed(reason) => Some(reason.to_string()),
                            _ => None,
                        },
                    };
                    if let Some(reason) = reason {
                        dvars.set("partyend_reason", connection_notice(&reason));
                        dvars.set("ui_connection_error", "1");
                    }
                    diag::info!(Sim, "session: peer terminal → lobby (swap #{id})");
                }
                Err(err) => diag::info!(Sim, "session: peer terminal lobby refused — {err}"),
            }
        }
        *live = false;
        return;
    }
    *terminal_cleanup = None;
    let Some(in_match) = joined_in_match(held.as_ref()) else {
        *live = false;
        return;
    };
    if peer_should_leave_match(*live, in_match) {
        match transition.request_menu() {
            Ok(id) => diag::info!(Sim, "session: peer match ended → lobby (swap #{id})"),
            Err(err) => diag::info!(Sim, "session: peer lobby return refused — {err}"),
        }
    }
    *live = in_match;
}

fn run_udp_peer_lobby_return(
    role: Res<RuntimeRole>,
    presented: Option<Res<PresentedSnapshot>>,
    bridge: Option<Res<net::MasterBridge>>,
    has_world: Option<Res<HasWorld>>,
    mut transition: ResMut<SessionSwapRequest>,
    mut latched: Local<bool>,
) {
    let master_joined = joined_in_match(bridge.as_ref().map(|b| b.state()).as_ref()).is_some();
    let has_world = has_world.is_some_and(|h| h.0);
    if !udp_peer_exit_applies(*role, master_joined, has_world) {
        *latched = false;
        return;
    }
    let journal_has = journal_has_match_ended(presented.as_deref());
    if udp_match_ended_rising(*latched, journal_has) {
        match transition.request_menu() {
            Ok(id) => diag::info!(Sim, "session: udp MatchEnded → lobby (swap #{id})"),
            Err(err) => diag::info!(Sim, "session: udp lobby return refused — {err}"),
        }
    }
    if journal_has {
        *latched = true;
    }
}

fn follow_master_match(
    mut commands: Commands,
    mut offers: ResMut<net::MasterMatchStart>,
    bridge: Option<Res<net::MasterBridge>>,
    identity: Res<LaunchIdentity>,
    live: Res<LiveWorldIdentity>,
    has_world: Res<HasWorld>,
    busy: Option<Res<assets::MatchLoadBusy>>,
    request: Option<Res<assets::MatchLoadRequest>>,
    ready: Option<Res<assets::PreparedMatchReady>>,
    mut swap: ResMut<SessionSwapRequest>,
    mut role: ResMut<RuntimeRole>,
    mut pending: Local<Option<(net::MasterMatchOffer, bool)>>,
) {
    if let Some(offer) = offers.take() {
        *pending = Some((offer, false));
    }
    let Some((offer, retiring)) = pending.as_mut() else {
        return;
    };
    let Some(bridge) = bridge else {
        *pending = None;
        return;
    };
    let state = bridge.state();
    if !state.in_match() || state.identity().match_key() != offer.match_key {
        *pending = None;
        return;
    }
    let host = matches!(state, net::MasterBridgeState::Hosting { .. });
    if has_world.0
        && (live.load_key.match_key == offer.match_key
            || (host && live.load_key.match_key.is_none() && identity.zone == offer.map))
    {
        *pending = None;
        return;
    }
    let loading = busy.is_some_and(|busy| busy.0) || request.is_some() || ready.is_some();
    if !*retiring && (has_world.0 || loading || swap.pending.is_some()) {
        let id = ready
            .as_ref()
            .map(|r| r.request_id)
            .or_else(|| request.as_ref().map(|r| r.request_id))
            .or_else(|| swap.pending.as_ref().map(|p| p.id))
            .unwrap_or(0);
        commands.insert_resource(assets::MatchLoadAbort(id));

        if let Err(error) = swap.request_menu() {
            diag::warn!(Net, "master match teardown refused: {error}");
            return;
        }
        *retiring = true;
        return;
    }
    if has_world.0 || loading || swap.pending.is_some() {
        return;
    }
    let Some(mode) = sim::HostGameModeSelection::from_token(&offer.mode) else {
        diag::warn!(Net, "master match mode refused: {}", offer.mode);
        *pending = None;
        return;
    };
    *role = if host {
        RuntimeRole::Listen
    } else {
        RuntimeRole::Client
    };
    commands.insert_resource(mode);
    match swap.request_zone(offer.map.clone()) {
        Ok(id) => {
            diag::info!(
                Net,
                "master match {}:{} -> session load #{id} {}",
                bridge.incarnation(),
                offer.match_key.match_epoch,
                offer.map
            );
            *pending = None;
        }
        Err(error) => diag::warn!(Net, "master match load refused: {error}"),
    }
}

pub fn register_lifecycle(app: &mut App) {
    app.init_resource::<TeardownRequest>()
        .init_resource::<frame::UiMenuDvars>()
        .init_resource::<SessionSwapRequest>()
        .init_resource::<TeardownGaps>()
        .init_resource::<WorldGeneration>()
        .init_resource::<frame::WorldProducts>()
        .init_resource::<LiveWorldIdentity>()
        .init_resource::<Retiring>()
        .add_message::<MapLoadApproved>()
        .add_message::<MapLoadFailed>()
        .add_message::<MatchInstalled>()
        .add_message::<MatchTornDown>()
        .add_message::<ReturnedToMenu>()
        .configure_sets(Update, frame::SessionSwapApplied.in_set(ClientSet::Load))
        .add_systems(
            Update,
            (
                follow_master_match,
                run_exit_level,
                run_peer_lobby_return,
                run_udp_peer_lobby_return,
                run_teardown,
                run_session_swap,
            )
                .chain()
                .in_set(frame::SessionSwapApplied),
        )
        .add_systems(
            Update,
            stamp_loaded_zone
                .after(crate::apply_prepared_match)
                .in_set(ClientSet::Load),
        )
        .add_systems(Update, retire_handed_over.in_set(ClientSet::Diag));
}
