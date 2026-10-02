use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use bevy::prelude::*;
use frame::{HasWorld, MatchTornDown, RuntimeRole};
use playerstate_iw4::UserCmd;

use crate::ServerTime;
use crate::authority::inbox::{
    AUTHORITY_MS, ClientActionInbox, ClientCommandInbox, MAX_REDUNDANT_CMDS,
};
use crate::authority::runtime::{AuthorityInputGate, AuthorityWorld, ClientShotSamples};
use crate::client::frame_clock::{FrameClock, GameActive, GameJoinCensus};
use crate::client::input::{
    ClientActionInput, LookState, accumulate_look, build_usercmd, frame_time_msec, key_frame_msec,
    remote_control_axes,
};
use crate::client::predict::{ClientPrediction, CmdSeq};
use crate::client::presentation::entities::CEntityBirthCensus;
use crate::client::presentation::presented::{
    LocalPresentClient, PresentLocalCensus, PresentedSnapshot, interpolate_player_state,
};
use crate::client::presentation::projectiles::merge_presented_projectiles;
use crate::client::proxy::{ProxySample, RemoteProxy};
use crate::client::realtime::ClientRealtime;
use crate::schedule::ClientSet;
use crate::transport::loopback_live::{ListenLoopback, ReceivedTick};
use sim::Snapshot;

#[derive(Resource)]
pub struct ClientPredictionState(pub ClientPrediction);

#[derive(Resource, Default)]
pub struct PendingPresentedEntityEvents {
    pub live: Vec<sim::EntityEventRecord>,

    pub archived: Vec<sim::EntityEventRecord>,

    pub in_killcam: bool,
}

#[derive(Resource, Default)]
pub struct PendingPelletFx(pub Vec<sim::PelletFxRecord>);

fn collect_received_entity_events(
    last_tick: Option<sim::Tick>,
    local: sim::ClientId,
    snapshot: &mut Snapshot,
    pending: &mut PendingPresentedEntityEvents,
) -> bool {
    if crate::classify_snapshot(last_tick, snapshot.tick) != crate::SnapshotOrder::New {
        return false;
    }
    pending.in_killcam = snapshot
        .meta
        .for_client(local)
        .is_some_and(|m| m.killcam_hud.is_some());
    if pending.in_killcam {
        pending.archived.append(&mut snapshot.meta.entity_events);
    } else {
        pending.live.append(&mut snapshot.meta.entity_events);
    }
    true
}

impl Default for ClientPredictionState {
    fn default() -> Self {
        Self(ClientPrediction::new(sim::ClientId(0)))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClockTick {
    pub released: u32,

    pub dropped: u32,
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct ClientClock {
    pub accumulator_ms: f32,

    pub dropped_slots: u32,
}

impl ClientClock {
    pub fn tick(&mut self, delta_ms: f32) -> ClockTick {
        let before = self.accumulator_ms;
        self.accumulator_ms += delta_ms;
        if self.accumulator_ms < AUTHORITY_MS as f32 {
            return ClockTick {
                released: 0,
                dropped: 0,
            };
        }
        let slots = (self.accumulator_ms / AUTHORITY_MS as f32).floor() as u32;
        let dropped = slots.saturating_sub(1);
        self.dropped_slots = self.dropped_slots.wrapping_add(dropped);
        self.accumulator_ms %= AUTHORITY_MS as f32;

        if self.accumulator_ms >= AUTHORITY_MS as f32 - 0.0005 {
            self.accumulator_ms = 0.0;
        }
        debug_assert!(
            self.accumulator_ms >= 0.0 && self.accumulator_ms < AUTHORITY_MS as f32,
            "clock debt after tick: before={before} dt={delta_ms} after={}",
            self.accumulator_ms
        );
        ClockTick {
            released: 1,
            dropped,
        }
    }

    pub fn debt_ms(&self) -> f32 {
        self.accumulator_ms
    }
}

#[derive(Resource, Default)]
pub struct ReceivedTicks(pub VecDeque<ReceivedTick>);

#[derive(Resource, Default, Debug)]
pub struct LastAdoptedSnapshot {
    pub snap: Option<Arc<Snapshot>>,
    pub next_snap: Option<Arc<Snapshot>>,

    pub applied_this_frame: bool,
}

impl LastAdoptedSnapshot {
    pub fn next(&self) -> Option<&Snapshot> {
        self.next_snap.as_deref()
    }

    pub fn sound_alias_name(&self, index: u8) -> Option<&str> {
        for snap in [self.next_snap.as_ref(), self.snap.as_ref()]
            .into_iter()
            .flatten()
        {
            if let Some((_, name)) = snap.meta.sound_aliases.iter().find(|(i, _)| *i == index) {
                return Some(name.as_str());
            }
        }
        None
    }

    pub fn effect_name(&self, index: u8) -> Option<&str> {
        self.next().and_then(|snap| {
            snap.meta
                .effect_names
                .iter()
                .find(|(i, _)| *i == index)
                .map(|(_, name)| name.as_str())
        })
    }
}

#[derive(Resource, Default, Debug, Clone)]
pub struct PendingClientSends {
    cmds: VecDeque<(CmdSeq, UserCmd, sim::ShotSampleProvenance)>,

    evicted_unacked: u32,

    acknowledged: Option<CmdSeq>,
}

impl PendingClientSends {
    pub fn len(&self) -> usize {
        self.cmds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }

    pub fn push(&mut self, seq: CmdSeq, cmd: UserCmd, sample: sim::ShotSampleProvenance) {
        if self.cmds.len() >= MAX_REDUNDANT_CMDS {
            self.evicted_unacked = self.evicted_unacked.saturating_add(1);
            return;
        }
        self.cmds.push_back((seq, cmd, sample));
    }

    pub fn evicted_unacked(&self) -> u32 {
        self.evicted_unacked
    }

    pub fn clear_acknowledged(&mut self, acked: CmdSeq) {
        self.acknowledged = Some(self.acknowledged.map_or(acked, |last| last.max(acked)));
        while self
            .cmds
            .front()
            .is_some_and(|(seq, _, _)| seq.0 <= acked.0)
        {
            self.cmds.pop_front();
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &(CmdSeq, UserCmd, sim::ShotSampleProvenance)> {
        self.cmds.iter()
    }

    pub fn has_command_capacity(&self, server_time: i32) -> bool {
        if self.len() >= MAX_REDUNDANT_CMDS {
            return false;
        }
        // Reserve one full step for the gap from the authority's last consumed
        // command. Bound unacknowledged work before allocating a sequence: after
        // a delayed ACK we can resume without dropping commands or leaving holes.
        let step = crate::authority::inbox::COMMAND_TIME_STEP_MS;
        let mut work = step;
        let mut previous: Option<i32> = None;
        for time in self
            .cmds
            .iter()
            .map(|(_, cmd, _)| cmd.server_time)
            .chain(std::iter::once(server_time))
        {
            if let Some(previous) = previous {
                work = work.saturating_add(time.saturating_sub(previous).clamp(0, step));
            }
            previous = Some(time);
        }
        work <= crate::authority::inbox::MAX_QUEUED_COMMAND_MS
    }

    pub fn oldest_seq(&self) -> Option<CmdSeq> {
        self.cmds.front().map(|(seq, _, _)| *seq)
    }

    pub fn newest_seq(&self) -> Option<CmdSeq> {
        self.cmds.back().map(|(seq, _, _)| *seq)
    }
}

#[derive(Resource, Default)]
pub struct ClientCmdTemplate {
    pub cmd: UserCmd,
    pub ready: bool,
}

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeaponSelect {
    pub index: u32,

    pub time: i32,

    pub life_sequence: u32,

    pub held_index: u32,

    pub last_primary: u32,
    pub mapped_index: u32,
}

pub fn follow_held_weapon_select(
    select: &mut WeaponSelect,
    ps_weapon: u32,
    life_sequence: u32,
    cg_time: i32,
) {
    if ps_weapon == 0 {
        return;
    }
    if select.index == 0
        || select.life_sequence != life_sequence
        || (select.index == select.held_index && ps_weapon != select.held_index)
    {
        select.index = ps_weapon;
        select.mapped_index = ps_weapon;
        select.last_primary = 0;
        select.life_sequence = life_sequence;
        select.time = cg_time;
    }
    select.held_index = ps_weapon;
}

pub fn cycle_weapon_select(
    select: &mut WeaponSelect,
    input: &mut input_iw4::ClientInput,
    ps: &playerstate_iw4::PlayerState,
    world: &sim::SimWorld,
    time: i32,
    next: bool,
) {
    use input_iw4::weapon_select::{cycle_weapon, weapon_cycle_allowed};
    if !weapon_cycle_allowed(ps, time, select.time, 0, 0) {
        return;
    }

    if (0x10..0x13).contains(&ps.weaponstate_primary) {
        let offhand = ps.off_hand_index.max(0) as u32;
        let cancelable = world
            .weapon_combat_row(offhand)
            .unwrap_or_else(weapon_iw4::WeaponCombatFacts::none)
            .offhand_hold_is_cancelable
            .unwrap_or(false);
        if cancelable {
            input.offhand_hold_cancel = true;
            return;
        }
    }
    select.time = time;
    if world.weapon_combat_len() < 2 {
        return;
    }
    let facts = |weapon| {
        world
            .weapon_combat_row(weapon)
            .expect("owned weapon must have a captured catalog row")
    };
    let Some(target) = cycle_weapon(
        &ps.weapons,
        select.index,
        select.mapped_index,
        select.last_primary,
        next,
        |weapon| facts(weapon).inventory_type,
    ) else {
        return;
    };
    let target_facts = facts(target);
    let requires_ammo = target_facts.select_requires_ammo.unwrap_or(false);
    if requires_ammo {
        let ammo_key = weapon_iw4::ammo_table_key(target_facts.ammo_index, target);
        let clip_key = weapon_iw4::clip_table_key(target_facts.clip_index, target);
        if weapon_iw4::get_ammo_player_both_clips(ps, target, ammo_key, clip_key) == 0 {
            return;
        }
    }
    if select.index != target {
        if select.index != 0 && facts(select.index).inventory_type == 0 {
            select.last_primary = select.index;
        }
        select.index = target;
        select.mapped_index = target;
        input_iw4::set_ads(input, false);
    }
}

#[derive(Resource, Default, Debug, Clone)]
pub struct ClientPhaseTrace(pub Vec<&'static str>);

fn push_phase(trace: Option<ResMut<ClientPhaseTrace>>, name: &'static str) {
    if let Some(mut t) = trace {
        t.0.push(name);
    }
}

pub fn listen_prediction_needs_content(pred: &ClientPrediction, authority: &sim::SimWorld) -> bool {
    if !authority.has_world_clip() {
        return false;
    }
    if !pred.is_armed() || !pred.world().has_world_clip() {
        return true;
    }
    if pred.world().content_digest() != authority.content_digest() {
        return true;
    }
    authority.pen_table_loaded() && !pred.world().pen_table_loaded()
}

pub fn arm_listen_prediction(
    role: Res<RuntimeRole>,
    authority: Option<Res<AuthorityWorld>>,
    mut prediction: ResMut<ClientPredictionState>,
) {
    if !matches!(*role, RuntimeRole::Listen | RuntimeRole::Client) {
        return;
    }
    let Some(authority) = authority else {
        return;
    };
    if !listen_prediction_needs_content(&prediction.0, &authority.0) {
        return;
    }
    prediction.0.arm_from_content(&authority.0);
}

pub fn advance_cls_realtime(mut cls: ResMut<ClientRealtime>, time: Res<Time>) {
    cls.advance_listen(time.delta_secs());
}

pub fn advance_cg_frame_clock(
    mut clock: ResMut<FrameClock>,
    mut prediction: ResMut<ClientPredictionState>,
    mut join: ResMut<GameJoinCensus>,
    time: Res<Time>,
    cgame_active: Res<GameActive>,
    authority: Option<Res<crate::AuthorityClock>>,
    role: Res<RuntimeRole>,
    fixed: Res<Time<Fixed>>,
    adopted: Option<Res<LastAdoptedSnapshot>>,
) {
    let was_started = clock.started();
    let adopted_tick = adopted.as_ref().and_then(|a| a.next().map(|s| s.tick));

    let local_authority = authority.as_ref().filter(|_| role.runs_authority());
    let server_time = local_authority
        .map(|clock| ServerTime::from_ms(clock.time_ms))
        .or_else(|| adopted_tick.map(ServerTime::from_tick));
    if *role == RuntimeRole::Listen && cgame_active.0 && was_started {
        let server = authority.as_ref().expect("listen authority clock").time_ms;
        clock.assign_server_time(server.wrapping_add(fixed.overstep().as_millis() as i32));
    } else if *role == RuntimeRole::Client {
        clock.advance_remote(time.delta_secs(), cgame_active.0, server_time);
    } else {
        clock.advance_listen(time.delta_secs(), cgame_active.0, server_time);
    }
    if clock.started() && !was_started {
        join.latch(
            server_time.map(ServerTime::ms).unwrap_or(0),
            local_authority
                .map(|c| c.tick)
                .or_else(|| adopted_tick.map(|t| t.0))
                .unwrap_or(0),
            (time.elapsed_secs() * 1000.0) as i32,
        );
    }
    prediction.0.predicted_error_mut().sync_frame(&clock);
}

#[derive(Default)]
pub struct JoinLinkWatch {
    first_offer: Option<std::time::Instant>,
    last_line: Option<std::time::Instant>,
    offers: u32,
    connected: bool,
}

const JOIN_WAIT_LINE_SECS: f32 = 2.0;

pub fn receive_ticks(
    mut link: Option<ResMut<crate::transport::udp_session::UdpClientLink>>,
    loopback: Option<ResMut<ListenLoopback>>,
    mut received: ResMut<ReceivedTicks>,
    mut local: ResMut<LocalPresentClient>,
    mut prediction: ResMut<ClientPredictionState>,
    trace: Option<ResMut<ClientPhaseTrace>>,
    descriptor: Option<Res<crate::MatchDescriptor>>,
    mut watch: Local<JoinLinkWatch>,
    mut reliable: ReliableInbound,
) {
    push_phase(trace, "Receive");
    if let Some(link) = link.as_mut() {
        if descriptor.is_some() {
            if let Err(e) = link.ensure_connected() {
                diag::warn!(Net, "udp connect: {e}");
            } else if link.should_offer_connect() {
                watch.offers += 1;
                watch
                    .first_offer
                    .get_or_insert_with(std::time::Instant::now);
            }
        }
        match link.recv_ticks() {
            Ok(ticks) => {
                for tick in ticks {
                    received.0.push_back(tick);
                }
            }
            Err(e) => {
                if link.handshake_reject().is_some() {
                    diag::error!(Net, "udp recv: {e}");
                } else {
                    diag::warn!(Net, "udp recv: {e}");
                }
            }
        }
        for payload in link.take_controls() {
            reliable.apply(link.assigned_client.unwrap_or(local.0), &payload);
        }
        note_join_link(&mut watch, link);

        if let Some(id) = link.assigned_client {
            if local.0 != id {
                local.0 = id;
                prediction.0.set_local(id);
            }
        }
        return;
    }
    if let Some(mut loopback) = loopback {
        let mut drained = Vec::new();
        let outcome = loopback.recv_all(&mut drained);
        for tick in drained {
            received.0.push_back(tick);
        }
        if let Err(e) = outcome {
            diag::warn!(Net, "client_runtime: recv_all failed: {e}");
        }
    }
}

fn connect_wait_line_applies(should_offer: bool, rejected: bool, has_connection: bool) -> bool {
    should_offer && !rejected && !has_connection
}

fn note_join_link(watch: &mut JoinLinkWatch, link: &crate::transport::udp_session::UdpClientLink) {
    let waited = |watch: &JoinLinkWatch| {
        watch
            .first_offer
            .map(|start| start.elapsed().as_secs_f32())
            .unwrap_or(0.0)
    };
    if link.connection.is_some() {
        if !watch.connected {
            watch.connected = true;
            diag::info!(
                Net,
                "udp client accepted by {} as client {} after {:.1}s and {} Connect offer(s)",
                link.server,
                link.assigned_client.map(|c| c.0).unwrap_or(0),
                waited(watch),
                watch.offers
            );
        }
        return;
    }
    if !connect_wait_line_applies(
        link.should_offer_connect(),
        link.handshake_reject().is_some(),
        false,
    ) {
        return;
    }
    let now = std::time::Instant::now();
    if watch
        .last_line
        .is_some_and(|last| last.elapsed().as_secs_f32() < JOIN_WAIT_LINE_SECS)
    {
        return;
    }
    watch.last_line = Some(now);

    let ours = link.hello.content;
    diag::warn!(
        Net,
        "udp client waiting on {}: {} Connect offer(s) over {:.1}s, no Accept and no Reject — \
         protocol={} offering map={:016x} weapons={:016x} classes={:016x}",
        link.server,
        watch.offers,
        waited(watch),
        link.hello.protocol_version,
        ours.map,
        ours.weapons,
        ours.classes
    );
}

#[derive(Resource, Default)]
pub struct RemoteProxyState(pub RemoteProxy);

pub fn reconcile_prediction(
    mut received: ResMut<ReceivedTicks>,
    mut prediction: ResMut<ClientPredictionState>,
    local: Res<LocalPresentClient>,
    mut reliable: ReliableInbound,
    mut last_adopted: ResMut<LastAdoptedSnapshot>,
    mut proxy: ResMut<RemoteProxyState>,
    mut pending: ResMut<PendingClientSends>,
    mut entity_events: ResMut<PendingPresentedEntityEvents>,
    mut pellet_fx: ResMut<PendingPelletFx>,
    trace: Option<ResMut<ClientPhaseTrace>>,
) {
    push_phase(trace, "Reconcile");
    last_adopted.applied_this_frame = false;
    while let Some(mut tick) = received.0.pop_front() {
        if !collect_received_entity_events(
            last_adopted.next().map(|last| last.tick),
            local.0,
            &mut tick.snapshot,
            &mut entity_events,
        ) {
            continue;
        }
        pellet_fx.0.append(&mut tick.snapshot.meta.pellet_fx);
        reliable.apply(local.0, &tick.frame.reliable);
        let ack = tick.ack_for(local.0);
        if let Some(acked) = ack {
            pending.clear_acknowledged(acked);
        }
        last_adopted.applied_this_frame = true;
        if let Some(old_next) = last_adopted.next_snap.take() {
            last_adopted.snap = Some(old_next);
        }
        let snap = Arc::new(tick.snapshot);
        proxy.0.push_arc(Arc::clone(&snap));
        last_adopted.next_snap = Some(snap);
        let newest = !received.0.iter().any(|pending| {
            pending.snapshot.tick.0 > last_adopted.next().expect("next snapshot").tick.0
        });
        if newest {
            prediction
                .0
                .on_authority(last_adopted.next().expect("next_snap just stored"), ack);
        } else {
            prediction.0.retire_acks(ack);
        }
        for cmd in tick.frame.svc_sounds {
            reliable.svc.sound.write(crate::SvcLocalSound {
                stop: cmd.stop,
                index: cmd.index,
            });
        }
        for cmd in tick.frame.svc_card_slots {
            reliable.svc.card.write(crate::SvcCardSlotCmd {
                client: cmd.client,
                slot: cmd.slot,
            });
        }
        for cmd in tick.frame.svc_hud_splashes {
            reliable.svc.splash.write(cmd);
        }
        for cmd in tick.frame.svc_game_notifies {
            reliable.svc.notify.write(cmd);
        }
        for cmd in tick.frame.svc_open_menus {
            reliable.svc.menu.write(crate::SvcOpenMenuCmd {
                cs_index: cmd.cs_index,
            });
        }
        if let Some(cmd) = tick.frame.svc_scores {
            reliable.scores.parsed = crate::parse_scoreboard_cmd(&cmd);
            reliable.scores.cmd = Some(cmd);
        }
    }
}

#[derive(bevy::prelude::Message, Clone, Copy, Debug)]
pub struct ReliableControlEvent(pub sim::SimEvent);

#[derive(bevy::ecs::system::SystemParam)]
pub struct SvcFrameWriters<'w> {
    sound: MessageWriter<'w, crate::SvcLocalSound>,
    card: MessageWriter<'w, crate::SvcCardSlotCmd>,
    menu: MessageWriter<'w, crate::SvcOpenMenuCmd>,
    splash: MessageWriter<'w, crate::SvcHudSplash>,
    notify: MessageWriter<'w, crate::SvcGameNotify>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct ReliableInbound<'w> {
    actions: ResMut<'w, ClientActionInbox>,
    ack: ResMut<'w, ClientReliableAck>,
    events: MessageWriter<'w, ReliableControlEvent>,
    svc: SvcFrameWriters<'w>,
    scores: ResMut<'w, crate::Scoreboard>,
    signon: ResMut<'w, crate::SignonState>,
    bridge: Option<Res<'w, crate::MasterBridge>>,
}

impl ReliableInbound<'_> {
    fn fail(&mut self, reason: &str) {
        self.actions.clear();
        let match_key = crate::signon::live_match_key(self.bridge.as_deref());
        if let Some(bridge) = &self.bridge {
            bridge.fail(reason);
        }
        self.signon.set_phase(crate::SignonPhase::Failed(
            crate::SignonFailReason::Transport {
                source: reason.to_owned(),
                stage: crate::FailStage::Transport,
                match_key,
            },
        ));
    }

    fn apply(&mut self, local: sim::ClientId, payload: &crate::ReliablePayload) {
        if payload.dropped_oldest != 0 {
            self.fail("ReliableControlOverflow: outcome unknown");
            return;
        }
        if let Some(reason) = payload.rows.iter().find_map(|(_, row)| {
            if let crate::ReliableRow::Failure(reason) = row {
                Some(reason)
            } else {
                None
            }
        }) {
            self.fail(reason);
            return;
        }
        let mut rows: Vec<&(u16, crate::ReliableRow)> = payload.rows.iter().collect();
        rows.sort_by_key(|(seq, _)| seq.wrapping_sub(self.ack.0));
        for (seq, row) in rows {
            if !reliable_seq_after(*seq, self.ack.0) {
                continue;
            }
            match row {
                crate::ReliableRow::Failure(_) => unreachable!("terminal handled before sequence"),
                crate::ReliableRow::Sound(cmd) => {
                    self.svc.sound.write(crate::SvcLocalSound {
                        stop: cmd.stop,
                        index: cmd.index,
                    });
                }
                crate::ReliableRow::Card(cmd) => {
                    self.svc.card.write(crate::SvcCardSlotCmd {
                        client: cmd.client,
                        slot: cmd.slot,
                    });
                }
                crate::ReliableRow::Menu(cmd) => {
                    self.svc.menu.write(crate::SvcOpenMenuCmd {
                        cs_index: cmd.cs_index,
                    });
                }
                crate::ReliableRow::Splash(cmd) => {
                    self.svc.splash.write(cmd.clone());
                }
                crate::ReliableRow::Notify(cmd) => {
                    self.svc.notify.write(cmd.clone());
                }
                crate::ReliableRow::Scores(cmd) => {
                    self.scores.parsed = crate::parse_scoreboard_cmd(cmd);
                    self.scores.cmd = Some(cmd.clone());
                }
                crate::ReliableRow::ActionOutcome {
                    request_id,
                    verdict,
                } => {
                    match verdict {
                        crate::ActionVerdict::PayloadMismatch => {
                            self.fail("ActionPayloadMismatch: connection retired");
                            return;
                        }
                        crate::ActionVerdict::Expired => {
                            self.fail("ActionOutcomeUnknown: request expired");
                            return;
                        }
                        crate::ActionVerdict::Applied | crate::ActionVerdict::Refused => {}
                    }
                    self.actions.retire(local, *request_id);
                }
                crate::ReliableRow::Event(event) => {
                    self.events.write(ReliableControlEvent(*event));
                }
            }
            self.ack.0 = *seq;
        }
    }
}

fn reliable_seq_after(a: u16, b: u16) -> bool {
    a != b && a.wrapping_sub(b) < 0x8000
}

fn apply_weapon_switch_requests(
    mut events: MessageReader<ReliableControlEvent>,
    mut select: ResMut<WeaponSelect>,
    clock: Res<FrameClock>,
) {
    for event in events.read() {
        if let sim::SimEvent::WeaponSwitchRequested { weapon } = event.0 {
            select.index = weapon;
            select.mapped_index = weapon;
            select.time = clock.time();
        }
    }
}

pub fn sample_client_input(
    (skate, mut minecraft, jak): (
        Option<Res<frame::SkateMode>>,
        Option<ResMut<frame::MinecraftUi>>,
        Option<Res<frame::JakMode>>,
    ),
    time: Res<Time>,
    mut actions: ResMut<ClientActionInput>,
    mut look: ResMut<LookState>,
    mut template: ResMut<ClientCmdTemplate>,
    mut select: ResMut<WeaponSelect>,
    prediction: Res<ClientPredictionState>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    gate: Res<AuthorityInputGate>,
    cls: Res<ClientRealtime>,
    clock: Res<FrameClock>,
    mut action_inbox: Option<ResMut<ClientActionInbox>>,
    mut request_ids: Option<ResMut<crate::ActionRequestIds>>,
    view: Option<Res<frame::ViewSubject>>,
    (trace, mut cursor): (Option<ResMut<ClientPhaseTrace>>, ResMut<LocationCursor>),
) {
    push_phase(trace, "Input");
    if !gate.local_cmds_enabled {
        actions.client.weapon_cycles.clear();
        actions.client.action_slots.clear();
        return;
    }
    actions.frame_msec = key_frame_msec(time.delta_secs());
    actions.now_msec = frame_time_msec(time.elapsed_secs());
    let ps = prediction
        .0
        .predicted_local()
        .or_else(|| presented.player(local.0));

    let frozen = ps.is_some_and(|ps| (ps.pm_flags & 0x800) != 0)
        || presented
            .snapshot()
            .and_then(|snapshot| snapshot.meta.for_client(local.0))
            .is_none_or(|meta| meta.controls.frozen);

    if frozen {
        actions.mouse_x = 0.0;
        actions.mouse_y = 0.0;
        actions.pad_move = [0.0; 2];
        actions.pad_look = [0.0; 2];
    }
    // The controller's turn this frame, with aim assist over visible
    // enemies: other players not on the local team, and a block world's
    // mobs.
    if let Some(ps) = ps {
        let world = prediction.0.world();
        let eye = [ps.origin[0], ps.origin[1], ps.origin[2] + ps.view_height_current];
        let visible = |point: [f32; 3]| {
            let hit = world.trace_world(eye, point, [0.0; 3], [0.0; 3], hud_iw4::OVERHEAD_TRACE_MASK);
            hit.fraction >= 1.0 && hit.startsolid == 0
        };
        let angles = [
            look.angles[0] as f32 / input_iw4::ANGLE2SHORT,
            look.angles[1] as f32 / input_iw4::ANGLE2SHORT,
        ];
        let (sp, cp) = angles[0].to_radians().sin_cos();
        let (sy, cy) = angles[1].to_radians().sin_cos();
        let forward = [cp * cy, cp * sy, -sp];
        // In front of the player, allowing for the target's size.
        let in_front = |origin: [f32; 3], radius: f32| {
            let d = [origin[0] - ps.origin[0], origin[1] - ps.origin[1], origin[2] - ps.origin[2]];
            d[0] * forward[0] + d[1] * forward[1] + d[2] * forward[2] + radius >= 0.0
        };
        let mut targets: Vec<crate::client::pad_aim::AimTarget> = Vec::new();
        if actions.pad_aim_assist > 0 {
            const RADIUS: f32 = 10.0;
            let snapshot = presented.snapshot();
            let team = |id: sim::ClientId| snapshot.and_then(|s| s.meta.for_client(id)).map(|m| m.client_state_team);
            let teams = snapshot.is_some_and(|s| s.meta.kind.is_team());
            let mine = team(local.0);
            for id in snapshot.into_iter().flat_map(|s| s.players.iter().map(|(id, _)| *id)) {
                if id == local.0 || (teams && team(id) == mine) {
                    continue;
                }
                let Some(other) = presented.player(id).filter(|o| o.pm_type == 0) else {
                    continue;
                };
                let o = other.origin;
                let head = [o[0], o[1], o[2] + other.view_height_current];
                if !in_front(o, RADIUS) || !visible(head) {
                    continue;
                }
                let top = other.view_height_current + 8.0;
                targets.push(crate::client::pad_aim::AimTarget {
                    key: u64::from(id.0),
                    mins: [o[0] - RADIUS, o[1] - RADIUS, o[2]],
                    maxs: [o[0] + RADIUS, o[1] + RADIUS, o[2] + top],
                    aim: [o[0], o[1], o[2] + top * 0.75],
                    velocity: other.velocity,
                });
            }
            for (key, mins, maxs) in sim::voxel::mob_targets() {
                // The upper body, as for players.
                let aim = [
                    (mins[0] + maxs[0]) * 0.5,
                    (mins[1] + maxs[1]) * 0.5,
                    mins[2] + (maxs[2] - mins[2]) * 0.75,
                ];
                let radius = (maxs[0] - mins[0]).max(maxs[1] - mins[1]) * 0.5;
                if !in_front(aim, radius) || !visible(aim) {
                    continue;
                }
                targets.push(crate::client::pad_aim::AimTarget {
                    key: 1 << 40 | key,
                    mins,
                    maxs,
                    aim,
                    velocity: [0.0; 3],
                });
            }
        }
        let ranges = world
            .weapon_combat_row(playerstate_iw4::get_viewmodel_weapon_index(ps))
            .map(|facts| facts.aim_assist)
            .unwrap_or_default();
        let view = crate::client::pad_aim::AimView {
            eye,
            angles,
            velocity: ps.velocity,
            ads_lerp: ps.f_weapon_pos_frac,
            fov_scale: actions.fov_scale.max(0.01),
            ranges,
            dt: cls.frametime_secs(),
        };
        let ads = actions.client.using_ads || actions.client.kb.speed.active;
        crate::client::pad_aim::pad_look_frame(&mut actions, &view, &targets, ads);
    } else {
        actions.pad_look_delta = [0.0; 2];
    }
    if frozen {
        actions.pad_look_delta = [0.0; 2];
    }
    let choose_direction = presented
        .snapshot()
        .and_then(|snapshot| snapshot.meta.for_client(local.0))
        .and_then(|meta| meta.location_selection.as_ref())
        .map(|selection| selection.choose_direction);
    let location_mouse = choose_direction.is_some().then(|| {
        let mouse = (actions.mouse_x, actions.mouse_y);
        actions.mouse_x = 0.0;
        actions.mouse_y = 0.0;
        mouse
    });
    let remote_mouse = presented
        .snapshot()
        .and_then(|snapshot| snapshot.meta.for_client(local.0))
        .and_then(|meta| meta.remote_missile)
        .filter(|link| link.unlink_at_ms.is_none())
        .map(|_| {
            let mouse = (actions.mouse_x, actions.mouse_y);
            actions.mouse_x = 0.0;
            actions.mouse_y = 0.0;
            mouse
        });
    let look_state = ps
        .zip(presented.shellshock(local.0))
        .map(|(ps, shock)| {
            hud_iw4::update_shellshock_look_control(
                clock.time(),
                ps.shellshock_time,
                ps.shellshock_duration,
                shock.look,
            )
        })
        .unwrap_or(hud_iw4::ShellshockLookState {
            sensitivity: 1.0,
            max_pitch_speed: 0.0,
            max_yaw_speed: 0.0,
        });
    actions.shellshock_look_scale = look_state.sensitivity;
    actions.cgame_max_pitch_speed = look_state.max_pitch_speed;
    actions.cgame_max_yaw_speed = look_state.max_yaw_speed;
    let now_msec = actions.now_msec;
    let frame_msec = actions.frame_msec;
    let cl_yawspeed = actions.cl_yawspeed;
    let max_pitch = actions.cgame_max_pitch_speed;
    let max_yaw = actions.cgame_max_yaw_speed;
    accumulate_look(
        cls.frametime_secs(),
        now_msec,
        frame_msec,
        &mut actions.client,
        &mut look,
        cl_yawspeed,
        frozen,
        max_pitch,
        max_yaw,
    );
    if std::mem::take(&mut actions.client.center_view) {
        if let Some(ps) = ps {
            look.angles[0] = (-ps.delta_angles[0] * input_iw4::ANGLE2SHORT) as i32;
        }
    }
    let ps_weapon = ps.map(|ps| ps.weapon).unwrap_or(0);
    let life_sequence = presented
        .snapshot()
        .and_then(|snapshot| snapshot.meta.for_client(local.0))
        .map(|meta| meta.life_sequence.0)
        .unwrap_or(0);
    if actions.stance_life != Some(life_sequence) {
        actions.client.stance_latch = 0;
        actions.stance_life = Some(life_sequence);
        actions.stance_event_sequence = ps.map_or(0, |ps| ps.event_sequence);
    } else if let Some(ps) = ps {
        let pending = ps
            .event_sequence
            .wrapping_sub(actions.stance_event_sequence);
        if pending > 0 {
            let events = [ps.events_0, ps.events_1, ps.events_2, ps.events_3];
            for age in (0..pending.min(4)).rev() {
                let seq = ps.event_sequence.wrapping_sub(1 + age);
                match events[(seq & 3) as usize] {
                    6 => actions.client.stance_latch = 0,
                    7 => actions.client.stance_latch = playerstate_iw4::buttons::CROUCH as i32,
                    8 => actions.client.stance_latch = playerstate_iw4::buttons::PRONE as i32,
                    _ => {}
                }
            }
            actions.stance_event_sequence = ps.event_sequence;
        }
    }
    follow_held_weapon_select(&mut select, ps_weapon, life_sequence, clock.time());
    if let Some(ps) = ps {
        if select.index == ps.weapon {
            select.mapped_index = if prediction
                .0
                .world()
                .weapon_combat_row(ps.weapon)
                .is_some_and(|facts| facts.inventory_type == 3)
            {
                ps.weapon_primary
            } else {
                ps.weapon
            };
        }
    }
    // On a Minecraft map the hotbar picks the gun: MW2's weapon cycling and
    // action slots give way to the gun its selection asks for.
    let hotbar = minecraft.as_ref().is_some_and(|ui| ui.active);
    if hotbar {
        // Switching weapon swaps between the first two hotbar slots; action
        // slots 3 and 4 (the D-pad's left and right) step along it.
        let slots = std::mem::take(&mut actions.client.action_slots);
        let cycles = std::mem::take(&mut actions.client.weapon_cycles);
        if let Some(ui) = minecraft.as_mut() {
            let mut selected = ui.select.unwrap_or(ui.selected);
            if !cycles.is_empty() {
                selected = if selected == 0 { 1 } else { 0 };
            }
            for slot in slots {
                match slot {
                    2 => selected = (selected + 8) % 9,
                    3 => selected = (selected + 1) % 9,
                    _ => {}
                }
            }
            if selected != ui.selected {
                ui.select = Some(selected);
            }
        }
        if let (Some(ps), Some(ui)) = (ps.filter(|_| !frozen), minecraft.as_mut())
            && let Some(target) = ui.weapon_request
            && target != select.index
            && ps.weapons.contains(&(target as i32))
            && input_iw4::weapon_select::weapon_cycle_allowed(ps, clock.time(), select.time, 0, 0)
        {
            select.index = target;
            select.mapped_index = target;
            select.time = clock.time();
            input_iw4::set_ads(&mut actions.client, false);
        }
    }
    let slots = std::mem::take(&mut actions.client.action_slots);
    if let (Some(inbox), Some(ids)) = (action_inbox.as_mut(), request_ids.as_mut()) {
        for &slot in &slots {
            let request_id = ids.allocate();
            let action = sim::ClientAction::ActionSlot {
                request_id,
                slot: slot as u8,
            };
            if let Err(error) = inbox.push(local.0, action) {
                diag::warn!(Net, "action_slot: not queued — {error}");
            }
        }
    }
    if let Some(ps) = ps.filter(|_| !frozen) {
        for slot in slots {
            if !input_iw4::weapon_select::weapon_cycle_allowed(ps, clock.time(), select.time, 0, 0)
            {
                continue;
            }
            let target = match ps.action_slot_type.get(slot).copied() {
                Some(1) => ps.action_slot_param[slot].max(0) as u32,
                Some(2) => match prediction.0.world().weapon_combat_row(select.index) {
                    Some(facts) if facts.inventory_type == 3 => select.mapped_index,
                    Some(facts) => facts.alternate_weapon,
                    None => 0,
                },
                _ => 0,
            };
            if target == 0 {
                continue;
            }
            let parent = if ps.weapons.contains(&(target as i32)) {
                target
            } else {
                select.mapped_index
            };
            let owned = ps.weapons.contains(&(parent as i32))
                && (parent == target
                    || prediction
                        .0
                        .world()
                        .weapon_combat_row(parent)
                        .is_some_and(|f| f.alternate_weapon == target));
            if !owned {
                continue;
            }
            select.index = target;
            select.mapped_index = parent;
            select.time = clock.time();
            input_iw4::set_ads(&mut actions.client, false);
        }
    }
    let cycles = std::mem::take(&mut actions.client.weapon_cycles);
    let in_killcam = view.is_some_and(|v| v.in_killcam());
    if let Some(ps) = ps {
        for next in cycles {
            if next && gamemode_iw4::copycat_weapnext_bind_active(ps.pm_type, in_killcam) {
                if let (Some(inbox), Some(ids)) = (action_inbox.as_mut(), request_ids.as_mut()) {
                    let request_id = ids.allocate();
                    if let Err(error) =
                        inbox.push(local.0, sim::ClientAction::UseCopycat { request_id })
                    {
                        diag::warn!(Net, "use_copycat: not queued — {error}");
                    }
                }
            }
            cycle_weapon_select(
                &mut select,
                &mut actions.client,
                ps,
                prediction.0.world(),
                clock.time(),
                next,
            );
        }
    }
    let mut cmd = build_usercmd(&mut actions, &look, 0);
    // Reload/use reloads when nothing usable is in reach.
    if cmd.buttons & playerstate_iw4::buttons::USE_RELOAD != 0 && ps.is_some_and(|ps| ps.cursor_hint == 0) {
        cmd.buttons |= playerstate_iw4::buttons::RELOAD;
    }
    if minecraft.as_ref().is_some_and(|ui| ui.active && ui.holding_item) {
        cmd.buttons &= !(playerstate_iw4::buttons::ATTACK | playerstate_iw4::buttons::ADS);
    }
    look.angles = cmd.angles;
    if let Some((mouse_x, mouse_y)) = remote_mouse {
        cmd.remote_control = remote_control_axes(&actions, mouse_x, mouse_y);
        cmd.buttons |= playerstate_iw4::buttons::REMOTE_CONTROL;
    }

    cmd.weapon = select.index as u16;
    cmd.weapon_mapped = select.mapped_index as u16;
    if let Some(loadout) = presented
        .snapshot()
        .and_then(|snapshot| snapshot.meta.for_client(local.0))
        .and_then(|meta| meta.loadout.as_ref())
    {
        if actions.client.kb.frag.active || actions.client.kb.frag.was_pressed {
            cmd.off_hand_index = loadout.lethal as u16;
        } else if actions.client.kb.smoke.active || actions.client.kb.smoke.was_pressed {
            cmd.off_hand_index = loadout.tactical as u16;
        }
    }
    match location_mouse {
        Some((mouse_x, mouse_y)) => {
            let held = cmd.buttons;
            let directing =
                choose_direction == Some(true) && held & playerstate_iw4::buttons::ADS != 0;
            let cancel = if choose_direction == Some(true) {
                playerstate_iw4::buttons::MELEE_CHARGE
            } else {
                playerstate_iw4::buttons::ADS | playerstate_iw4::buttons::MELEE_CHARGE
            };
            cmd.selected_location = cursor.step(&actions, mouse_x, mouse_y, directing);
            if choose_direction != Some(true) {
                cmd.selected_location[2] = 0;
            }
            cmd.buttons &= playerstate_iw4::buttons::CROUCH | playerstate_iw4::buttons::PRONE;
            if held & playerstate_iw4::buttons::ATTACK != 0 {
                cmd.buttons |= playerstate_iw4::buttons::LOCATION_SELECT;
            } else if held & cancel != 0 {
                cmd.buttons |= playerstate_iw4::buttons::LOCATION_CANCEL;
            }
        }
        None => *cursor = LocationCursor::default(),
    }
    if cmd.buttons & playerstate_iw4::buttons::MELEE_CHARGE != 0
        && let (Some(ps), Some(snapshot)) = (ps, presented.snapshot())
        && let Some((yaw, dist)) = melee_charge_target(ps, local.0, snapshot, prediction.0.world())
    {
        cmd.melee_charge_yaw = yaw;
        cmd.melee_charge_dist = dist;
    }
    if skate.as_ref().is_some_and(|s| s.active) || jak.as_ref().is_some_and(|j| j.active) {
        cmd.forwardmove = 0; cmd.rightmove = 0; cmd.buttons = 0;
    }
    template.cmd = cmd;
    template.ready = true;
}

const AIM_AUTOMELEE_RANGE: f32 = 128.0;
const MELEE_REGION_TAN_X: f32 = 0.849 * 0.5;
const MELEE_REGION_TAN_Y: f32 = 0.478 * 0.5;
const PMF_PRONE: u32 = 0x1;

fn melee_charge_target(
    ps: &playerstate_iw4::PlayerState,
    local: sim::ClientId,
    snapshot: &Snapshot,
    world: &sim::SimWorld,
) -> Option<(f32, u8)> {
    if ps.pm_flags & PMF_PRONE != 0 {
        return None;
    }
    let local_team = snapshot.meta.for_client(local)?.client_state_team;
    let eye = [
        ps.origin[0],
        ps.origin[1],
        ps.origin[2] + ps.view_height_current,
    ];
    let (forward, right, up) = math_iw4::angle_vectors(ps.viewangles);
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let mut best: Option<(f32, [f32; 3])> = None;
    for (id, other) in &snapshot.players {
        if *id == local || other.pm_type > 1 {
            continue;
        }
        let Some(meta) = snapshot.meta.for_client(*id) else {
            continue;
        };
        if meta.lifecycle != sim::ClientLifecycle::Alive
            || meta.client_state_team == 3
            || (local_team != 0 && meta.client_state_team == local_team)
        {
            continue;
        }
        let delta = [
            other.origin[0] - ps.origin[0],
            other.origin[1] - ps.origin[1],
            other.origin[2] - ps.origin[2],
        ];
        if dot(delta, delta) > AIM_AUTOMELEE_RANGE * AIM_AUTOMELEE_RANGE {
            continue;
        }
        let center = [other.origin[0], other.origin[1], other.origin[2] + 36.0];
        let to = [center[0] - eye[0], center[1] - eye[1], center[2] - eye[2]];
        let depth = dot(to, forward);
        if depth <= 1.0 {
            continue;
        }
        let x = dot(to, right) / depth;
        let y = dot(to, up) / depth;
        if x.abs() - 15.0 / depth > MELEE_REGION_TAN_X
            || y.abs() - 36.0 / depth > MELEE_REGION_TAN_Y
        {
            continue;
        }
        let crosshair = x * x + y * y;
        if best.is_some_and(|(b, _)| b <= crosshair) {
            continue;
        }
        let hit = world.trace_static_world(eye, center, [0.0; 3], [0.0; 3], sim::MASK_SHOT);
        if hit.fraction < 1.0 {
            continue;
        }
        best = Some((crosshair, delta));
    }
    let (_, delta) = best?;
    let dist = dot(delta, delta).sqrt();
    if dist <= movement_iw4::MELEE_CHARGE_PLAYER_MELEE_RANGE_DEFAULT {
        return None;
    }
    Some((math_iw4::vec_to_yaw(delta[0], delta[1]), dist as u8))
}

/// `yaw` is counterclockwise from map up.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct LocationCursor {
    pub at: [f32; 2],
    pub aim: [f32; 2],
    pub yaw: f32,
    pub directing: bool,
}

impl Default for LocationCursor {
    fn default() -> Self {
        Self {
            at: [0.5, 0.5],
            aim: [0.5, 0.5],
            yaw: 0.0,
            directing: false,
        }
    }
}

const LOCATION_CURSOR_SPEED: f32 = 0.6;

impl LocationCursor {
    fn step(
        &mut self,
        input: &ClientActionInput,
        mouse_x: f32,
        mouse_y: f32,
        directing: bool,
    ) -> [u8; 3] {
        let scale = LOCATION_CURSOR_SPEED * input.sensitivity * 0.002;
        let moved = [mouse_x * scale, mouse_y * scale];
        self.directing = directing;
        if directing {
            self.aim = [
                (self.aim[0] + moved[0]).clamp(0.0, 1.0),
                (self.aim[1] + moved[1]).clamp(0.0, 1.0),
            ];
            let d = [self.aim[0] - self.at[0], self.aim[1] - self.at[1]];
            if d[0] != 0.0 || d[1] != 0.0 {
                self.yaw = (-d[0]).atan2(-d[1]).to_degrees().rem_euclid(360.0);
            }
        } else {
            self.at = [
                (self.at[0] + moved[0]).clamp(0.0, 1.0),
                (self.at[1] + moved[1]).clamp(0.0, 1.0),
            ];
            self.aim = self.at;
        }
        let byte = |v: f32| ((v * 255.0 - 128.0).round() as i32).clamp(-128, 127) as i8 as u8;
        let yaw = (self.yaw * (256.0 / 360.0)).round() as i32 as u8;
        [byte(self.at[0]), byte(self.at[1]), yaw]
    }
}

/// A stall is a stretch in which the authority acked none of the local client's
/// commands. Both measures of one — the age of the oldest command still waiting
/// to be acked, and the command time spanned by the unacknowledged history — are
/// wall-clock spans, so a single long frame fills either on its own — which is
/// why one stall is not a failure.
pub const BACKLOG_STALL_MS: i32 = 1000;

/// How many stalls the link may take before the session is failed for real. A
/// hitch costs one and is recovered from; an authority that has genuinely gone
/// away keeps earning them and fails once they add up.
pub const BACKLOG_STALLS_BEFORE_FAIL: u32 = 3;

#[derive(Debug, Default)]
pub struct BacklogStalls {
    counted: u32,

    acks_at_last: u64,

    last_ms: Option<i32>,
}

impl BacklogStalls {
    /// Count a stall, at most one per `BACKLOG_STALL_MS` and only while the
    /// authority has acked nothing since the last one. `None` means the caller
    /// has already acted on the stall still in progress.
    fn note(&mut self, now_ms: i32, acks: u64) -> Option<u32> {
        if acks != self.acks_at_last {
            self.acks_at_last = acks;
            self.counted = 0;
            self.last_ms = Some(now_ms);
            return None;
        }
        if self
            .last_ms
            .is_some_and(|last| now_ms.saturating_sub(last) < BACKLOG_STALL_MS)
        {
            return None;
        }
        self.last_ms = Some(now_ms);
        self.counted = self.counted.saturating_add(1);
        Some(self.counted)
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

pub fn enforce_client_work_limits(
    pending: Res<PendingClientSends>,
    mut prediction: ResMut<ClientPredictionState>,
    cg: Res<FrameClock>,
    cls: Res<ClientRealtime>,
    mut signon: ResMut<crate::SignonState>,
    bridge: Option<Res<crate::MasterBridge>>,
    mut gate: ResMut<AuthorityInputGate>,
    actions: Res<ClientActionInbox>,
    mut stalls: Local<BacklogStalls>,
) {
    if signon.phase.is_failed() {
        gate.local_cmds_enabled = false;
        return;
    }
    let oldest = pending.iter().next().map(|(_, cmd, _)| cmd.server_time);
    let duration =
        queued_command_duration(prediction.0.history().iter().map(|row| row.cmd.server_time));
    let stalled = oldest.is_some_and(|oldest| cg.time().saturating_sub(oldest) > BACKLOG_STALL_MS)
        || duration > BACKLOG_STALL_MS;
    let stall = if stalled {
        stalls.note(
            cls.realtime(),
            pending.acknowledged.map_or(0, |seq| u64::from(seq.0)),
        )
    } else {
        stalls.clear();
        None
    };
    let reason = if actions.timed_out() {
        Some("ActionOutcomeUnknown: authority outcome deadline exceeded")
    } else if stall.is_some_and(|counted| counted >= BACKLOG_STALLS_BEFORE_FAIL) {
        Some("InputBacklogExceeded")
    } else {
        None
    };
    let Some(reason) = reason else {
        if let Some(counted) = stall {
            diag::warn!(
                Net,
                "input backlog: no authority ack in {BACKLOG_STALL_MS} ms \
                 (stall {counted}/{BACKLOG_STALLS_BEFORE_FAIL}) — re-adopting"
            );
            prediction.0.force_resync();
        }
        return;
    };
    gate.local_cmds_enabled = false;
    prediction.0.disarm();
    let match_key = crate::signon::live_match_key(bridge.as_deref());
    if let Some(bridge) = bridge {
        bridge.fail(reason);
    }
    signon.set_phase(crate::SignonPhase::Failed(
        crate::SignonFailReason::Transport {
            source: reason.to_owned(),
            stage: crate::FailStage::Transport,
            match_key,
        },
    ));
}

fn queued_command_duration(times: impl IntoIterator<Item = i32>) -> i32 {
    let mut previous = None;
    let mut duration = 0i32;
    for time in times {
        if let Some(previous) = previous {
            duration = duration.saturating_add(time.saturating_sub(previous).max(0));
        }
        previous = Some(time);
    }
    duration
}

pub fn predict_local_move(
    time: Res<Time>,
    mut clock: ResMut<ClientClock>,
    role: Res<RuntimeRole>,
    mut prediction: ResMut<ClientPredictionState>,
    template: Res<ClientCmdTemplate>,
    mut actions: ResMut<ClientActionInput>,
    gate: Res<AuthorityInputGate>,
    mut pending: ResMut<PendingClientSends>,
    proxy: Res<RemoteProxyState>,
    cls: Res<ClientRealtime>,
    cg_clock: Res<FrameClock>,
    trace: Option<ResMut<ClientPhaseTrace>>,
) {
    push_phase(trace, "Predict");

    if *role != RuntimeRole::Replay {
        let _tick = clock.tick(time.delta_secs() * 1000.0);
    }
    if !gate.local_cmds_enabled
        || !prediction.0.is_armed()
        || !template.ready
        || !cg_clock.started()
        || !pending.has_command_capacity(cg_clock.time())
    {
        return;
    }

    let msec = cls.frametime();
    if msec <= 0 {
        return;
    }

    let sample = proxy.0.shot_sample(cg_clock.time());
    if let Some((seq, cmd)) = prediction
        .0
        .predict(template.cmd, ServerTime::from_ms(cg_clock.time()))
    {
        actions.consume_edges();
        pending.push(seq, cmd, sample);
    }
}

pub fn flush_bootstrap_applied(
    mut link: Option<ResMut<crate::transport::udp_session::UdpClientLink>>,
    adopted: Res<LastAdoptedSnapshot>,
    defer: Option<Res<crate::DeferBootstrapApplied>>,
) {
    if defer.is_some_and(|defer| defer.0) {
        return;
    }
    if !adopted.applied_this_frame {
        return;
    }
    if let Some(link) = link.as_mut() {
        link.flush_applied_after_adopt();
    }
}

pub fn send_pending_commands(
    link: Option<ResMut<crate::transport::udp_session::UdpClientLink>>,
    mut inbox: ResMut<ClientCommandInbox>,
    mut action_inbox: Option<ResMut<ClientActionInbox>>,
    reliable_ack: Res<ClientReliableAck>,
    mut pacer: ResMut<GameplaySendPacer>,
    cls: Res<ClientRealtime>,
    pending: Res<PendingClientSends>,
    local: Res<LocalPresentClient>,
    trace: Option<ResMut<ClientPhaseTrace>>,
) {
    push_phase(trace, "Send");
    if let Some(mut link) = link {
        if link.connection.is_none() || !link.has_entered_match() {
            return;
        }

        let actions: Vec<_> = action_inbox
            .as_deref()
            .map(|inbox| inbox.pending_for(local.0).collect())
            .unwrap_or_default();
        let now_ms = cls.realtime();
        let newest = pending.newest_seq();
        let gameplay_due = pacer.gameplay_due(now_ms, newest);

        let ack_due = reliable_ack.0 != pacer.last_sent_ack;
        let control_due = ack_due || link.has_unsent_actions(&actions);
        if !gameplay_due && !control_due {
            return;
        }

        let (cmds, retry_cursor) = if gameplay_due {
            pacer.command_window(&pending)
        } else {
            (Vec::new(), None)
        };
        let window = &cmds;
        if let Err(e) = link.send_commands(window, &actions, reliable_ack.0) {
            diag::warn!(Net, "udp send cmds: {e}");
            return;
        }
        if let Some(inbox) = action_inbox.as_deref_mut() {
            for action in &actions {
                inbox.mark_dispatched(local.0, sim::action_request_id(action));
            }
        }
        if gameplay_due {
            pacer.note_sent(now_ms, newest, reliable_ack.0);
            pacer.retry_cursor = retry_cursor;
        } else {
            pacer.last_sent_ack = reliable_ack.0;
        }
        if control_due {
            pacer.last_control_ms = Some(now_ms);
        }
        return;
    }

    if !pacer.gameplay_due(cls.realtime(), pending.newest_seq()) {
        return;
    }
    pacer.note_sent(cls.realtime(), pending.newest_seq(), reliable_ack.0);
    for (seq, cmd, sample) in pending.iter().copied() {
        inbox.push(local.0, Some(seq), cmd, Some(sample));
    }
}

pub const MAX_GAMEPLAY_SENDS_PER_SEC: i32 = 60;

pub const GAMEPLAY_SEND_INTERVAL_MS: i32 =
    (1000 + MAX_GAMEPLAY_SENDS_PER_SEC - 1) / MAX_GAMEPLAY_SENDS_PER_SEC;

#[derive(Resource, Debug, Default)]
pub struct GameplaySendPacer {
    last_send_ms: Option<i32>,

    last_sent_seq: Option<CmdSeq>,

    retry_cursor: Option<CmdSeq>,

    last_sent_ack: u16,

    last_control_ms: Option<i32>,
}

impl GameplaySendPacer {
    fn command_window(
        &self,
        pending: &PendingClientSends,
    ) -> (
        Vec<(CmdSeq, UserCmd, sim::ShotSampleProvenance)>,
        Option<CmdSeq>,
    ) {
        let (mut fresh, retries): (Vec<_>, Vec<_>) = pending
            .iter()
            .copied()
            .partition(|(seq, _, _)| self.last_sent_seq.is_none_or(|last| *seq > last));
        let mut cursor = self.retry_cursor;
        // Repeat the oldest command on every send to unblock the ordered stream,
        // plus a small rotating window. A full 16-command repeat at 60 Hz floods
        // high-latency links with commands that are already in flight.
        if let Some(oldest) = retries.first() {
            fresh.push(*oldest);
            let rotating = &retries[1..];
            if !rotating.is_empty() {
                let start = rotating
                    .iter()
                    .position(|(seq, _, _)| cursor.is_none_or(|last| *seq > last))
                    .unwrap_or(0);
                for row in rotating
                    .iter()
                    .cycle()
                    .skip(start)
                    .take(rotating.len().min(3))
                {
                    cursor = Some(row.0);
                    fresh.push(*row);
                }
            }
        }
        fresh.sort_by_key(|(seq, _, _)| *seq);
        (fresh, cursor)
    }

    fn gameplay_due(&self, now_ms: i32, newest: Option<CmdSeq>) -> bool {
        if newest.is_none() {
            return false;
        }
        match self.last_send_ms {
            None => true,
            Some(last) => now_ms.saturating_sub(last) >= GAMEPLAY_SEND_INTERVAL_MS,
        }
    }

    fn note_sent(&mut self, now_ms: i32, newest: Option<CmdSeq>, ack: u16) {
        self.last_send_ms = Some(now_ms);
        self.last_sent_seq = newest;
        self.last_sent_ack = ack;
    }
}

#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct ClientReliableAck(pub u16);

pub fn publish_presented(
    clock: Res<ClientClock>,
    cg_clock: Res<FrameClock>,
    mut entities: Query<&mut crate::CEntityRuntime>,
    role: Res<RuntimeRole>,
    prediction: Res<ClientPredictionState>,
    last_adopted: Res<LastAdoptedSnapshot>,
    proxy: Res<RemoteProxyState>,
    local: Res<LocalPresentClient>,
    has_world: Option<Res<HasWorld>>,
    mut presented: ResMut<PresentedSnapshot>,
    mut present_census: ResMut<PresentLocalCensus>,
    phase: Option<ResMut<crate::UpdatePhaseCensus>>,
    trace: Option<ResMut<ClientPhaseTrace>>,
) {
    let publish_started = std::time::Instant::now();
    struct PublishStamp<'a> {
        started: std::time::Instant,
        phase: Option<ResMut<'a, crate::UpdatePhaseCensus>>,
    }
    impl Drop for PublishStamp<'_> {
        fn drop(&mut self) {
            let ms = self.started.elapsed().as_secs_f32() * 1000.0;
            if let Some(phase) = self.phase.as_mut() {
                phase.publish_presented_ms = Some(ms);
            }
        }
    }
    let _publish_stamp = PublishStamp {
        started: publish_started,
        phase,
    };
    push_phase(trace, "Present");

    if has_world.is_some_and(|world| !world.0) {
        presented.clear();
        *present_census = PresentLocalCensus {
            choice: Some("no_next"),
            snapshot_delta_time: None,
            predicted_delta_time: prediction.0.predicted_local().map(|ps| ps.delta_time),
        };
        return;
    }
    let Some(snap_arc) = last_adopted.next_snap.clone() else {
        *present_census = PresentLocalCensus {
            choice: Some("no_next"),
            snapshot_delta_time: None,
            predicted_delta_time: prediction.0.predicted_local().map(|ps| ps.delta_time),
        };
        return;
    };
    let archived = snap_arc
        .players
        .iter()
        .any(|(id, ps)| *id == local.0 && !ps.is_live_frame());
    let snap_arc = if archived {
        proxy.0.snapshot_at(cg_clock.time()).unwrap_or(snap_arc)
    } else {
        snap_arc
    };
    let snapshot = snap_arc.as_ref();
    let snapshot_local = snapshot
        .players
        .iter()
        .find(|(id, _)| *id == local.0)
        .map(|(_, ps)| ps);
    let predicted_ps = prediction.0.predicted_local().copied();
    let previous = presented.player(local.0).copied();
    let armed = prediction.0.is_armed();
    let frame_interpolation = clock.accumulator_ms / AUTHORITY_MS as f32;
    let interpolation_previous = if *role == RuntimeRole::Replay {
        last_adopted
            .snap
            .clone()
            .filter(|previous| previous.tick.0 < snapshot.tick.0)
    } else {
        None
    };
    let interpolated = interpolation_previous.as_deref().and_then(|previous| {
        let previous_local = previous
            .players
            .iter()
            .find(|(id, _)| *id == local.0)
            .map(|(_, ps)| ps)?;
        let next_local = snapshot_local?;
        (previous_local.is_live_frame() && next_local.is_live_frame())
            .then(|| interpolate_player_state(previous_local, next_local, frame_interpolation))
    });
    *present_census = PresentLocalCensus {
        choice: Some(if interpolated.is_some() {
            "interpolated"
        } else if armed {
            PresentedSnapshot::presentation_local_choice(
                snapshot_local,
                predicted_ps.as_ref(),
                previous.as_ref(),
            )
        } else {
            PresentedSnapshot::presentation_local_choice_unarmed(snapshot_local)
        }),
        snapshot_delta_time: snapshot_local.map(|ps| ps.delta_time),
        predicted_delta_time: predicted_ps.as_ref().map(|ps| ps.delta_time),
    };
    let mut predicted = match interpolated {
        Some(interpolated) => interpolated,
        None if armed => PresentedSnapshot::presentation_local_player_state(
            snapshot_local,
            predicted_ps,
            previous,
        ),
        None => PresentedSnapshot::presentation_local_player_state_unarmed(snapshot_local),
    };
    let render_time_ms = if *role == RuntimeRole::Replay {
        (snapshot.tick.0 as i32 + 1) * AUTHORITY_MS + clock.accumulator_ms as i32
    } else {
        cg_clock.time()
    };

    let body_time_ms = if *role == RuntimeRole::Replay {
        snapshot.tick.0 as i32 * AUTHORITY_MS - AUTHORITY_MS + clock.accumulator_ms as i32
    } else if archived {
        cg_clock.time().saturating_sub(crate::PROXY_DELAY_MS)
    } else {
        cg_clock.time().saturating_sub(AUTHORITY_MS)
    };

    let mut remote_poses = HashMap::new();
    let mut remote_provenance = HashMap::new();
    for (id, _) in &snapshot.players {
        if *id == local.0 && !archived {
            continue;
        }
        match proxy.0.interpolate_at(*id, render_time_ms) {
            ProxySample::Pose { ps, provenance } => {
                remote_poses.insert(*id, ps);
                remote_provenance.insert(*id, provenance);
            }
            ProxySample::Starved { held, provenance } => {
                if let Some(held_ps) = held {
                    remote_poses.insert(*id, held_ps);
                }
                remote_provenance.insert(*id, provenance);
            }
        }
    }

    if archived {
        if let Some(seat) = remote_poses.remove(&local.0) {
            predicted = seat;
        }
    }
    for mut runtime in &mut entities {
        runtime.presented_player = None;
        if runtime.pose_e_type as i32 == entity_iw4::ET_PLAYER
            && let Ok(number) = u32::try_from(runtime.next_state.client_num)
            && let Some(ps) = remote_poses.get(&sim::ClientId(number))
        {
            runtime.origin = ps.origin;
            runtime.angles = ps.viewangles;
            let provenance = remote_provenance[&sim::ClientId(number)];
            let sample_time = match provenance.outcome {
                crate::PresentationSampleOutcome::Exact { snapshot }
                | crate::PresentationSampleOutcome::Starved {
                    held_snapshot: Some(snapshot),
                    ..
                } => snapshot.0 as i32 * AUTHORITY_MS,
                crate::PresentationSampleOutcome::Interpolated { .. } => {
                    provenance.effective_time.0
                }
                crate::PresentationSampleOutcome::Starved {
                    held_snapshot: None,
                    ..
                } => unreachable!("a proxy pose always has a source snapshot"),
            };
            runtime.presented_player = Some((
                crate::player_state_to_entity_state(sim::ClientId(number), ps),
                sample_time,
            ));
        } else {
            runtime.present_pose(body_time_ms);
        }
    }

    let mut predicted_projectiles = Vec::new();
    prediction
        .0
        .world()
        .visit_projectiles(|projectile| predicted_projectiles.push(*projectile));
    let merged_projectiles = if archived {
        merge_presented_projectiles(snapshot.projectiles.as_slice(), &[], local.0)
    } else {
        merge_presented_projectiles(
            snapshot.projectiles.as_slice(),
            &predicted_projectiles,
            local.0,
        )
    };
    let view_offset = if archived {
        [0.0; 3]
    } else {
        prediction
            .0
            .predicted_error()
            .offset_for_pm_type(predicted.pm_type)
    };
    presented.publish_proxied(
        snap_arc,
        local.0,
        predicted,
        remote_poses,
        remote_provenance,
    );
    presented.set_presented_projectiles(merged_projectiles);
    presented.set_view_offset(view_offset);
    presented.set_snapshot_interpolation(interpolation_previous, frame_interpolation);
    presented.set_trajectory_sample(
        archived.then_some(body_time_ms),
        archived
            .then(|| proxy.0.snapshot_after(cg_clock.time()))
            .flatten(),
    );
}

pub fn reset_cgame_on_match_torn_down(
    mut torn: MessageReader<MatchTornDown>,
    mut clock: ResMut<FrameClock>,
    mut active: ResMut<GameActive>,
    mut join: ResMut<GameJoinCensus>,
    mut adopted: ResMut<LastAdoptedSnapshot>,
    mut received: ResMut<ReceivedTicks>,
    (mut prediction, mut sends, mut signon, mut admission): (
        ResMut<ClientPredictionState>,
        ResMut<PendingClientSends>,
        ResMut<crate::SignonState>,
        ResMut<crate::ClientAdmission>,
    ),
    mut client_clock: ResMut<ClientClock>,
    mut proxy: ResMut<RemoteProxyState>,
    mut presented: ResMut<PresentedSnapshot>,
    mut present_census: ResMut<PresentLocalCensus>,
    mut select: ResMut<WeaponSelect>,
    (mut entity_events, mut pellet_fx, mut entity_event_cursor): (
        ResMut<PendingPresentedEntityEvents>,
        ResMut<PendingPelletFx>,
        ResMut<crate::EntityEventCursor>,
    ),
    (mut reliable_ack, mut actions, mut events, mut scores): (
        ResMut<ClientReliableAck>,
        Option<ResMut<ClientActionInbox>>,
        ResMut<Messages<ReliableControlEvent>>,
        ResMut<crate::Scoreboard>,
    ),
    mut birth: Option<ResMut<CEntityBirthCensus>>,
    mut pacer: ResMut<GameplaySendPacer>,
) {
    if torn.read().count() == 0 {
        return;
    }
    *signon = crate::SignonState::default();
    *admission = crate::ClientAdmission::default();
    clock.reset();
    active.0 = false;
    join.clear();
    *adopted = LastAdoptedSnapshot::default();
    received.0.clear();
    prediction.0.disarm();

    *sends = PendingClientSends::default();
    *client_clock = ClientClock::default();
    *pacer = GameplaySendPacer::default();
    *proxy = RemoteProxyState::default();
    presented.clear();
    *present_census = PresentLocalCensus::default();
    *select = WeaponSelect::default();

    *entity_events = PendingPresentedEntityEvents::default();
    pellet_fx.0.clear();
    *entity_event_cursor = crate::EntityEventCursor::default();

    *reliable_ack = ClientReliableAck::default();
    events.clear();
    *scores = crate::Scoreboard::default();
    if let Some(actions) = actions.as_mut() {
        actions.clear();
    }
    if let Some(birth) = birth.as_mut() {
        **birth = CEntityBirthCensus::default();
    }
    perf::cgame_hold("no_next", 0);
}

pub fn register_client_runtime(app: &mut App) {
    app.add_systems(
        Update,
        enforce_client_work_limits
            .before(predict_local_move)
            .after(reconcile_prediction),
    );
    app.init_resource::<crate::SignonState>()
        .init_resource::<crate::ClientAdmission>()
        .init_resource::<ClientPredictionState>()
        .init_resource::<ClientClock>()
        .init_resource::<FrameClock>()
        .init_resource::<GameActive>()
        .init_resource::<GameJoinCensus>()
        .init_resource::<ClientRealtime>()
        .init_resource::<ReceivedTicks>()
        .init_resource::<LastAdoptedSnapshot>()
        .init_resource::<PresentLocalCensus>()
        .init_resource::<PendingPresentedEntityEvents>()
        .init_resource::<PendingPelletFx>()
        .init_resource::<PendingClientSends>()
        .init_resource::<ClientCmdTemplate>()
        .init_resource::<WeaponSelect>()
        .init_resource::<LocationCursor>()
        .init_resource::<ClientReliableAck>()
        .init_resource::<GameplaySendPacer>()
        .add_message::<ReliableControlEvent>()
        .add_message::<frame::MatchInstalled>()
        .add_message::<frame::MatchTornDown>()
        .init_resource::<RemoteProxyState>()
        .init_resource::<ClientShotSamples>()
        .init_resource::<crate::Scoreboard>()
        .add_message::<crate::SvcLocalSound>()
        .add_message::<crate::SvcCardSlotCmd>()
        .add_message::<crate::SvcOpenMenuCmd>()
        .add_message::<crate::SvcHudSplash>()
        .add_message::<crate::SvcGameNotify>();
    crate::client::frame_census::register_update_phase_census(app);
    app.configure_sets(
        Update,
        (
            crate::PresentedPublished
                .in_set(ClientSet::Present)
                .after(publish_presented),
            frame::ClassEquipResolved
                .in_set(ClientSet::Present)
                .after(publish_presented),
        ),
    )
    .add_systems(
        Update,
        (
            advance_cls_realtime.in_set(ClientSet::Load),
            reset_cgame_on_match_torn_down
                .in_set(ClientSet::Load)
                .after(frame::SessionSwapApplied),
            advance_cg_frame_clock
                .in_set(ClientSet::Reconcile)
                .after(reconcile_prediction),
            receive_ticks.in_set(ClientSet::Receive),
            crate::signon::drive_client_admission_facts
                .in_set(ClientSet::Receive)
                .after(receive_ticks),
            crate::signon::drive_signon
                .in_set(ClientSet::Receive)
                .after(crate::signon::drive_client_admission_facts),
            crate::signon::drive_map_loaded
                .in_set(ClientSet::Receive)
                .after(crate::signon::drive_client_admission_facts),
            reconcile_prediction.in_set(ClientSet::Reconcile),
            flush_bootstrap_applied
                .in_set(ClientSet::Reconcile)
                .after(reconcile_prediction),
            apply_weapon_switch_requests
                .in_set(ClientSet::Input)
                .before(sample_client_input),
            sample_client_input.in_set(ClientSet::Input),
            predict_local_move.in_set(ClientSet::Predict),
            send_pending_commands.in_set(ClientSet::Send),
            publish_presented.in_set(ClientSet::Present),
        ),
    );
}

pub fn register_listen_prediction_arm(app: &mut App) {
    app.add_systems(Update, arm_listen_prediction.in_set(ClientSet::Load));
}
