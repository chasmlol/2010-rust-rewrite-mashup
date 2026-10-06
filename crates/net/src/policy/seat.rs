use std::collections::HashMap;

use bevy::prelude::Resource;
use playerstate_iw4::{PlayerState, SeatFocus, apply_killcam_seat, rebase_archived_timers};
use sim::{ClientId, ClientLifecycle, Snapshot};

use crate::authority::inbox::AUTHORITY_MS;
use crate::player_state_to_entity_state;
use crate::transport::archive::{ARCHIVE_TICK_MS, ArchiveLookup, FrameArchive};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KillcamSession {
    pub archivetime_ms: i32,

    pub focus_client: ClientId,

    pub focus: SeatFocus,

    pub ends_at_ms: i32,

    pub final_kill: bool,

    pub started_at_ms: i32,

    pub killcamoffset_ms: i32,

    pub predelay_ms: i32,

    pub postdelay_ms: i32,

    pub recalc_pending: bool,

    pub entity_focus: Option<killcam_iw4::focus::Entity>,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct ActiveKillcams {
    sessions: HashMap<ClientId, KillcamSession>,
}

impl ActiveKillcams {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, viewer: ClientId) -> Option<&KillcamSession> {
        self.sessions.get(&viewer)
    }

    pub fn arm(&mut self, viewer: ClientId, session: KillcamSession) {
        self.sessions.insert(viewer, session);
    }

    pub fn clear(&mut self, viewer: ClientId) {
        self.sessions.remove(&viewer);
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    pub fn expire_clients(&mut self, now_ms: i32) -> Vec<ClientId> {
        let mut expired = Vec::new();
        self.sessions.retain(|id, s| {
            if now_ms < s.ends_at_ms {
                true
            } else {
                expired.push(*id);
                false
            }
        });
        expired
    }

    pub fn viewers(&self) -> Vec<ClientId> {
        self.sessions.keys().copied().collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SeatSample {
    pub player_state: PlayerState,
    pub lookup: ArchiveLookup,

    pub rebase_ms: i32,
}

pub fn sample_killcam_seat(
    archive: &FrameArchive,
    snapshot_now: &Snapshot,
    viewer: ClientId,
    session: &KillcamSession,
    now_ms: i32,
) -> Option<SeatSample> {
    if session.killcamoffset_ms == 0 {
        let target = snapshot_now
            .players
            .iter()
            .find(|(id, _)| *id == session.focus_client)?
            .1;
        let mut player_state = target;
        player_state.pm_type = playerstate_iw4::PM_TYPE_SPECTATOR;
        player_state.other_flags &= !(playerstate_iw4::other_flags::PLAYER
            | playerstate_iw4::other_flags::DEAD_KILLCAM_TPV);
        player_state.kill_cam_entity = playerstate_iw4::ENTITYNUM_NONE;
        player_state.kill_cam_look_at_entity = playerstate_iw4::ENTITYNUM_NONE;
        player_state.kill_cam_client_num = session.focus_client.0 as i32;
        player_state.delta_time = 0;
        return Some(SeatSample {
            player_state,
            lookup: ArchiveLookup {
                attained_ms: 0,
                tick: None,
            },
            rebase_ms: 0,
        });
    }
    let lookup = archive.lookup(session.archivetime_ms);
    if lookup.nothing_to_show() {
        return None;
    }
    let tick = lookup.tick?;
    let frame = archive.frame(tick)?;
    if !frame.player_state_exists(session.focus_client) {
        return None;
    }
    let archived = frame
        .snapshot
        .players
        .iter()
        .find(|(id, _)| *id == session.focus_client)
        .map(|(_, ps)| *ps)?;

    let viewer_ps = snapshot_now
        .players
        .iter()
        .find(|(id, _)| *id == viewer)
        .map(|(_, ps)| *ps)
        .unwrap_or(PlayerState::ZERO);

    let frame_base_ms = i32::try_from(tick.0).ok()?.saturating_mul(ARCHIVE_TICK_MS);
    let rebase_ms = now_ms.saturating_sub(frame_base_ms);

    let mut remapped = archived;
    rebase_archived_timers(&mut remapped, rebase_ms);
    let player_state = apply_killcam_seat(&viewer_ps, &remapped, session.focus);

    Some(SeatSample {
        player_state,
        lookup,
        rebase_ms,
    })
}

pub fn apply_seat_to_snapshot(
    archive: &FrameArchive,
    snapshot: &mut Snapshot,
    viewer: ClientId,
    session: &KillcamSession,
    now_ms: i32,
) -> Option<SeatSample> {
    let sample = sample_killcam_seat(archive, snapshot, viewer, session, now_ms)?;
    if let Some((_, ps)) = snapshot.players.iter_mut().find(|(id, _)| *id == viewer) {
        *ps = sample.player_state;
    } else {
        snapshot.players.push((viewer, sample.player_state));
    }
    Some(sample)
}

pub fn snapshot_for_viewer(
    archive: &FrameArchive,
    seats: &ActiveKillcams,
    live: &Snapshot,
    viewer: ClientId,
    now_ms: i32,
) -> Snapshot {
    snapshot_and_sample_for_viewer(archive, seats, live, viewer, now_ms).0
}

pub fn snapshot_override_for_viewer(
    archive: &FrameArchive,
    seats: &ActiveKillcams,
    live: &Snapshot,
    viewer: ClientId,
    now_ms: i32,
) -> Option<Snapshot> {
    let hidden_movers = live.meta.script_movers.iter().any(|mover| {
        mover.shown_to != 0 && mover.state.e_flags & entity_iw4::CG_SCRIPT_MOVER_NODRAW != 0
    });
    (hidden_movers || seat_applies(seats, live, viewer))
        .then(|| snapshot_for_viewer(archive, seats, live, viewer, now_ms))
}

fn seat_applies(seats: &ActiveKillcams, live: &Snapshot, viewer: ClientId) -> bool {
    seats.get(viewer).is_some_and(|session| {
        session.final_kill
            || !live
                .meta
                .for_client(viewer)
                .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    })
}

pub fn snapshot_and_sample_for_viewer(
    archive: &FrameArchive,
    seats: &ActiveKillcams,
    live: &Snapshot,
    viewer: ClientId,
    now_ms: i32,
) -> (Snapshot, Option<SeatSample>) {
    let (mut out, sample) = seat_snapshot(archive, seats, live, viewer, now_ms);
    reveal_shown_movers(&mut out, viewer);
    (out, sample)
}

fn reveal_shown_movers(out: &mut Snapshot, viewer: ClientId) {
    let Some(bit) = 1u64.checked_shl(viewer.0) else {
        return;
    };
    let Snapshot { meta, .. } = out;
    for mover in &mut meta.script_movers {
        if mover.shown_to == 0 || mover.state.e_flags & entity_iw4::CG_SCRIPT_MOVER_NODRAW == 0 {
            continue;
        }
        if mover.shown_to & bit == 0 {
            meta.entity_dobjs
                .retain(|(owner, _)| owner.script_model() != Some(mover.id));
            continue;
        }
        let hidden = mover.state.clone();
        mover.state.e_flags &= !entity_iw4::CG_SCRIPT_MOVER_NODRAW;
        for es in meta.entities.iter_mut().filter(|es| **es == hidden) {
            *es = mover.state.clone();
        }
    }
}

fn seat_snapshot(
    archive: &FrameArchive,
    seats: &ActiveKillcams,
    live: &Snapshot,
    viewer: ClientId,
    now_ms: i32,
) -> (Snapshot, Option<SeatSample>) {
    let Some(session) = seats
        .get(viewer)
        .filter(|_| seat_applies(seats, live, viewer))
    else {
        return (live.clone(), None);
    };
    if session.killcamoffset_ms == 0 {
        let mut out = live.clone();
        let sample = apply_seat_to_snapshot(archive, &mut out, viewer, session, now_ms);
        return (out, sample);
    }
    let lookup = archive.lookup(session.archivetime_ms);
    let mut out = match lookup.tick.and_then(|tick| archive.frame(tick)) {
        Some(frame) => overlay_archived_world(live, &frame.snapshot, viewer, session.focus_client),
        None => live.clone(),
    };
    let sample = apply_seat_to_snapshot(archive, &mut out, viewer, session, now_ms);
    let archived = lookup.tick.and_then(|tick| archive.frame(tick));
    overlay_killcam_hud(
        &mut out,
        live,
        archived.map(|f| &f.snapshot),
        viewer,
        session,
        sample.as_ref().map(|s| s.rebase_ms).unwrap_or(0),
    );
    (out, sample)
}

fn overlay_archived_world(
    live: &Snapshot,
    archived: &Snapshot,
    viewer: ClientId,
    focus: ClientId,
) -> Snapshot {
    let mut out = archived.clone();
    out.meta
        .entity_events
        .retain(|record| record.audience.projects_to(focus));
    for record in &mut out.meta.entity_events {
        record.audience = sim::EventAudience::Client(viewer);
    }
    out.tick = live.tick;
    out.meta.phase = live.meta.phase;
    out.meta.match_elapsed_ms = live.meta.match_elapsed_ms;
    out.meta.prematch = live.meta.prematch;
    out.meta.score_limit = live.meta.score_limit;
    out.meta.time_limit_ms = live.meta.time_limit_ms;
    out.meta.kind = live.meta.kind;
    out.meta.sound_aliases = live.meta.sound_aliases.clone();
    out.meta.hud_strings = live.meta.hud_strings.clone();
    out.meta.hud_materials = live.meta.hud_materials.clone();

    let archived_viewer = archived
        .players
        .iter()
        .find(|(id, _)| *id == viewer)
        .map(|(_, ps)| *ps);
    if let Some(ps) = archived_viewer {
        let es = player_state_to_entity_state(viewer, &ps);
        if !out.meta.entities.iter().any(|e| e.number == es.number) {
            out.meta.entities.push(es);
        }
    }

    let viewer_row = live.players.iter().find(|(id, _)| *id == viewer).cloned();
    out.players.retain(|(id, _)| *id != viewer);
    if let Some(row) = viewer_row {
        out.players.push(row);
    }

    let viewer_meta = live
        .meta
        .clients
        .iter()
        .find(|(id, _)| *id == viewer)
        .cloned();
    out.meta.clients.retain(|(id, _)| *id != viewer);
    if let Some(row) = viewer_meta {
        out.meta.clients.push(row);
    }
    let delta_ms = sim::level_time_ms(live.tick).wrapping_sub(sim::level_time_ms(archived.tick));
    rebase_archived_world(&mut out, viewer, delta_ms);
    out
}

fn rebase_entity_state(es: &mut entity_iw4::EntityState, delta_ms: i32) {
    if es.tr_time != 0 {
        es.tr_time = es.tr_time.wrapping_add(delta_ms);
    }
    if es.apos_tr_time != 0 {
        es.apos_tr_time = es.apos_tr_time.wrapping_add(delta_ms);
    }
    if es.time2 != 0 {
        es.time2 = es.time2.wrapping_add(delta_ms);
    }
    // General's data[0] and missile launchTime share one union slot.
    if es.e_type == entity_iw4::ET_GENERAL || es.e_type == entity_iw4::ET_MISSILE {
        es.set_launch_time(es.launch_time().wrapping_add(delta_ms));
    }
}

fn rebase_archived_world(out: &mut Snapshot, viewer: ClientId, delta_ms: i32) {
    for es in &mut out.meta.entities {
        rebase_entity_state(es, delta_ms);
    }
    // Adopt requires each typed mover to equal its entityState row.
    for mover in &mut out.meta.script_movers {
        rebase_entity_state(&mut mover.state, delta_ms);
    }
    for p in &mut out.projectiles {
        if p.pos.tr_time != 0 {
            p.pos.tr_time = p.pos.tr_time.wrapping_add(delta_ms);
        }
        if p.apos.tr_time != 0 {
            p.apos.tr_time = p.apos.tr_time.wrapping_add(delta_ms);
        }
        p.launch_time = p.launch_time.wrapping_add(delta_ms);
        p.spawn_time_ms = p.spawn_time_ms.wrapping_add(delta_ms);
        p.detonate_at_ms = p.detonate_at_ms.map(|t| t.wrapping_add(delta_ms));
        p.cleanup_at_ms = p.cleanup_at_ms.wrapping_add(delta_ms);
    }
    for (id, ps) in &mut out.players {
        if *id != viewer {
            rebase_archived_timers(ps, delta_ms);
        }
    }
}

fn overlay_killcam_hud(
    out: &mut Snapshot,
    live: &Snapshot,
    archived: Option<&Snapshot>,
    viewer: ClientId,
    session: &KillcamSession,
    rebase_ms: i32,
) {
    let current = live
        .meta
        .for_client(viewer)
        .map(|m| m.hud_current.clone())
        .unwrap_or_default();
    let wipe = session.focus.kill_cam_entity != playerstate_iw4::HITSCAN_KILL_CAM_ENTITY;
    let mut archival = if wipe {
        Vec::new()
    } else {
        archived
            .and_then(|snap| snap.meta.for_client(session.focus_client))
            .map(|m| m.hud_archival.clone())
            .unwrap_or_default()
    };
    sim::rebase_hud_archival(&mut archival, rebase_ms);
    if let Some((_, meta)) = out.meta.clients.iter_mut().find(|(id, _)| *id == viewer) {
        meta.killcam_hud = Some(sim::KillcamHud {
            final_kill: session.final_kill,
        });
        meta.hud_current = current;
        meta.hud_archival = archival;
    }
}

pub const fn killcam_seconds_to_ms(seconds: f32) -> i32 {
    let ms = seconds * 1000.0;
    let ticks = (ms / (AUTHORITY_MS as f32) + 0.5) as i32;
    ticks.saturating_mul(AUTHORITY_MS)
}
