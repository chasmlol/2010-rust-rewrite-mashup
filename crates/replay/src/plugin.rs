use bevy::prelude::*;
use frame::{MatchTornDown, ReturnedToMenu, RuntimeRole, SessionSwapApplied};
use net::{
    AUTHORITY_MS, AuthorityClock, AuthorityLoadHold, AuthoritySet, ClientClock, ClientSet,
    ReceivedTick, ReceivedTicks, ServerTick, ServerTime, authority_should_tick,
};

use crate::clip::ClipRing;
use crate::session::{Playback, Recording};

#[derive(Resource, Default)]
pub struct PendingReplayArm {
    pub playback: Option<Playback>,
    pub quit_on_end: bool,
}

#[derive(Resource, Default)]
pub struct ReplaySession(pub Option<Recording>);

#[derive(Resource, Default)]
pub struct ReplayDiagnostics {
    pub line: Option<String>,
}

#[derive(Resource)]
pub struct ReplayPlayback {
    playback: Playback,
    acc_ms: f32,
    pub ended: bool,

    pub quit_on_end: bool,
    pub exit_queued: bool,
}

impl ReplayPlayback {
    pub fn new(playback: Playback) -> Self {
        Self {
            playback,
            acc_ms: AUTHORITY_MS as f32,
            ended: false,
            quit_on_end: true,
            exit_queued: false,
        }
    }

    pub fn with_quit_on_end(mut self, quit_on_end: bool) -> Self {
        self.quit_on_end = quit_on_end;
        self
    }

    pub fn ticks(&self) -> u64 {
        self.playback.ticks()
    }

    pub fn acc_ms(&self) -> f32 {
        self.acc_ms
    }

    pub fn path(&self) -> &std::path::Path {
        self.playback.path()
    }
}

pub struct ReplayPlugin;

impl Plugin for ReplayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ReplaySession>()
            .init_resource::<ReplayDiagnostics>()
            .init_resource::<PendingReplayArm>()
            .init_resource::<ClipRing>();
        let role = app.world().get_resource::<RuntimeRole>().copied();
        if role.is_some_and(|role| role.runs_authority() || role == RuntimeRole::Replay) {
            app.add_systems(
                FixedUpdate,
                record_server_tick
                    .in_set(AuthoritySet::Fanout)
                    .run_if(authority_should_tick),
            );
        }
        app.add_systems(
            Update,
            (
                record_received_ticks
                    .after(ClientSet::Receive)
                    .before(ClientSet::Reconcile),
                sync_theater_occupancy
                    .in_set(ClientSet::Load)
                    .after(SessionSwapApplied),
                pump_playback
                    .in_set(ClientSet::Load)
                    .run_if(resource_exists::<ReplayPlayback>)
                    .after(sync_theater_occupancy),
            ),
        );
    }
}

fn sync_theater_occupancy(
    mut commands: Commands,
    mut torn: MessageReader<MatchTornDown>,
    mut returned: MessageReader<ReturnedToMenu>,
    mut pending: ResMut<PendingReplayArm>,
    mut ring: ResMut<ClipRing>,
    role: Res<RuntimeRole>,
) {
    if torn.read().count() > 0 || returned.read().count() > 0 {
        ring.clear();
        commands.remove_resource::<ReplayPlayback>();
        perf::theater(0, None, None);
    }
    if *role != RuntimeRole::Replay {
        return;
    }
    let Some(playback) = pending.playback.take() else {
        return;
    };
    let quit_on_end = pending.quit_on_end;
    commands.insert_resource(ReplayPlayback::new(playback).with_quit_on_end(quit_on_end));
    perf::theater(
        1,
        Some(i64::from(quit_on_end)),
        Some(i64::from(role.runs_authority())),
    );
}

fn record_received_ticks(
    received: Res<ReceivedTicks>,
    role: Res<RuntimeRole>,
    mut ring: ResMut<ClipRing>,
) {
    if role.runs_authority() {
        return;
    }
    for tick in &received.0 {
        ring.push(tick.snapshot.clone());
    }
}

fn record_server_tick(
    mut session: ResMut<ReplaySession>,
    mut ring: ResMut<ClipRing>,
    server_tick: Res<ServerTick>,
    role: Res<RuntimeRole>,
    mut diag: ResMut<ReplayDiagnostics>,
) {
    let Some(tick) = server_tick.0.as_ref() else {
        return;
    };
    if role.runs_authority() {
        ring.push(tick.snapshot.clone());
    }
    let Some(recording) = session.0.as_mut() else {
        return;
    };
    if let Err(error) = recording.record(&tick.snapshot) {
        let line = format!("record: write failed, recording stopped: {error}");
        diag::info!(Net, "{line}");
        diag.line = Some(line);
        session.0 = None;
    }
}

fn pump_playback(
    mut session: ResMut<ReplayPlayback>,
    mut received: ResMut<ReceivedTicks>,
    mut clock: Option<ResMut<AuthorityClock>>,
    mut client_clock: ResMut<ClientClock>,
    hold: Option<Res<AuthorityLoadHold>>,
    time: Res<Time>,
) {
    if session.ended {
        return;
    }
    if hold.is_none_or(|h| h.0) {
        return;
    }
    session.acc_ms += time.delta_secs() * 1000.0;
    let slot = AUTHORITY_MS as f32;
    while session.acc_ms >= slot {
        session.acc_ms -= slot;
        match session.playback.next_tick() {
            Ok(Some(played)) => {
                if let Some(clock) = clock.as_mut() {
                    clock.tick = played.snapshot.tick.0;
                    clock.time_ms = ServerTime::from_tick(played.snapshot.tick).ms();
                }
                // What this frame is about to present. Two runs of the same
                // clip at different frame rates land on different frame
                // indices, so wall time cannot pair their frames and the demo
                // state can.
                perf::frames::set_replay_tick(u64::from(played.snapshot.tick.0));
                received.0.push_back(ReceivedTick {
                    snapshot: played.snapshot,
                    frame: played.frame,
                });
            }
            Ok(None) => {
                session.ended = true;
                diag::info!(
                    Net,
                    "play: ended after {} ticks ({})",
                    session.playback.ticks(),
                    session.playback.path().display()
                );
                break;
            }
            Err(error) => {
                session.ended = true;
                let line = format!("play: {error}");
                diag::warn!(Net, "{line}");
                break;
            }
        }
    }
    client_clock.accumulator_ms = session.acc_ms;
}
