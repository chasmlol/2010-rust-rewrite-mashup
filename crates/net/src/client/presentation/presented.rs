use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::Resource;
use playerstate_iw4::PlayerState;
use sim::{ClientId, ClientLifecycle, EntityEventRecord, EventRecord, SimEvent, Snapshot};

use crate::client::presentation::projectiles::PresentedProjectile;
use crate::client::proxy::PresentationSampleProvenance;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FpvEventCues {
    pub shot_accepted: bool,
    pub attack_released: bool,
    pub spawned: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FpvHandRecord {
    pub weap_anim: i32,
    pub weapon_time: i32,
    pub weapon_delay: i32,
    pub weapon_restrict_kick_time: i32,
    pub weaponstate: i32,
    pub weap_hand_flags: i32,
    pub weapon_shot_count: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FpvRawHandSample {
    pub hands: [FpvHandRecord; 2],
    pub f_weapon_pos_frac: f32,
    pub ads_delay_time: i32,
    pub last_weapon_hand: i32,
    pub weapon: u32,
    pub off_hand_index: i32,
    pub weap_flags: u32,
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct PresentLocalCensus {
    pub choice: Option<&'static str>,
    pub snapshot_delta_time: Option<i32>,
    pub predicted_delta_time: Option<i32>,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct PresentedSnapshot {
    inner: Option<Arc<Snapshot>>,
    previous_inner: Option<Arc<Snapshot>>,
    frame_interpolation: f32,
    trajectory_time_ms: Option<i32>,
    trajectory_next: Option<Arc<Snapshot>>,

    pose: HashMap<ClientId, PlayerState>,
    view_offset: [f32; 3],
    remote_provenance: HashMap<ClientId, PresentationSampleProvenance>,

    presented_projectiles: Vec<PresentedProjectile>,
}

impl PresentedSnapshot {
    pub fn weapon_epoch(&self) -> Option<u32> {
        self.snapshot()
            .map(|snapshot| snapshot.meta.world_objects.map_round_epoch)
    }

    pub fn clear(&mut self) {
        self.inner = None;
        self.previous_inner = None;
        self.frame_interpolation = 0.0;
        self.trajectory_time_ms = None;
        self.trajectory_next = None;
        self.pose.clear();
        self.view_offset = [0.0; 3];
        self.remote_provenance.clear();
        self.presented_projectiles.clear();
    }

    pub fn publish_decoded(&mut self, snapshot: Snapshot) {
        self.trajectory_time_ms = None;
        self.trajectory_next = None;
        self.presented_projectiles = authoritative_projectiles(&snapshot);
        self.pose.clear();
        self.inner = Some(Arc::new(snapshot));
        self.previous_inner = None;
        self.frame_interpolation = 0.0;
        self.remote_provenance.clear();
    }

    pub fn presentation_local_player_state(
        snapshot_local: Option<&PlayerState>,
        predicted: Option<PlayerState>,
        previous_presented: Option<PlayerState>,
    ) -> PlayerState {
        if let Some(ps) = snapshot_local
            && !ps.is_live_frame()
        {
            return *ps;
        }
        predicted
            .or(previous_presented)
            .or_else(|| snapshot_local.copied())
            .unwrap_or(PlayerState::ZERO)
    }

    pub fn presentation_local_choice(
        snapshot_local: Option<&PlayerState>,
        predicted: Option<&PlayerState>,
        previous_presented: Option<&PlayerState>,
    ) -> &'static str {
        if snapshot_local.is_some_and(|ps| !ps.is_live_frame()) {
            return "archived";
        }
        if predicted.is_some() {
            "predicted"
        } else if previous_presented.is_some() {
            "previous"
        } else if snapshot_local.is_some() {
            "snapshot"
        } else {
            "zero"
        }
    }

    pub fn presentation_local_player_state_unarmed(
        snapshot_local: Option<&PlayerState>,
    ) -> PlayerState {
        if let Some(ps) = snapshot_local
            && !ps.is_live_frame()
        {
            return *ps;
        }
        snapshot_local.copied().unwrap_or(PlayerState::ZERO)
    }

    pub fn presentation_local_choice_unarmed(snapshot_local: Option<&PlayerState>) -> &'static str {
        if snapshot_local.is_some_and(|ps| !ps.is_live_frame()) {
            "archived"
        } else if snapshot_local.is_some() {
            "snapshot"
        } else {
            "zero"
        }
    }

    pub fn publish_proxied(
        &mut self,
        snapshot: Arc<Snapshot>,
        local: ClientId,
        predicted_ps: PlayerState,
        mut remote_poses: HashMap<ClientId, PlayerState>,
        remote_provenance: HashMap<ClientId, PresentationSampleProvenance>,
    ) {
        self.trajectory_time_ms = None;
        self.trajectory_next = None;
        self.presented_projectiles = authoritative_projectiles(&snapshot);
        self.pose.clear();
        self.pose.insert(local, predicted_ps);
        self.pose.extend(remote_poses.drain());
        self.inner = Some(snapshot);
        self.previous_inner = None;
        self.frame_interpolation = 0.0;
        self.remote_provenance = remote_provenance;
    }

    pub fn interpolation_pair(&self) -> Option<(&Snapshot, &Snapshot, f32)> {
        Some((
            self.previous_inner.as_deref()?,
            self.inner.as_deref()?,
            self.frame_interpolation,
        ))
    }

    pub fn trajectory_time_ms(&self, render_time_ms: i32) -> i32 {
        self.trajectory_time_ms.unwrap_or(render_time_ms)
    }

    pub(crate) fn set_trajectory_sample(
        &mut self,
        time_ms: Option<i32>,
        next: Option<Arc<Snapshot>>,
    ) {
        self.trajectory_time_ms = time_ms;
        self.trajectory_next = next;
    }

    pub fn projectile_origin_at(
        &self,
        projectile: &sim::ProjectileState,
        render_time_ms: i32,
    ) -> [f32; 3] {
        let Some(time_ms) = self.trajectory_time_ms else {
            return projectile.origin_at(render_time_ms);
        };
        let Some(current) = self.snapshot() else {
            return projectile.origin_at(time_ms);
        };
        let current_time = sim::level_time_ms(current.tick);
        let Some(next) = self.trajectory_next.as_deref() else {
            return projectile.origin_at(time_ms.min(current_time));
        };
        let next_time = sim::level_time_ms(next.tick);
        let next_projectile = next.projectiles.iter().find(|p| {
            p.id == projectile.id
                && p.entnum == projectile.entnum
                && p.owner_life == projectile.owner_life
        });
        if let Some(next_projectile) = next_projectile
            && next_time > current_time
            && next_projectile.pos != projectile.pos
        {
            // A buffered impact/bounce ends the old trajectory; extrapolating it
            // through that transition sends the camera past the victim.
            let fraction = ((time_ms - current_time) as f32 / (next_time - current_time) as f32)
                .clamp(0.0, 1.0);
            return std::array::from_fn(|axis| {
                projectile.origin[axis]
                    + (next_projectile.origin[axis] - projectile.origin[axis]) * fraction
            });
        }
        projectile.origin_at(time_ms)
    }

    pub(crate) fn set_snapshot_interpolation(
        &mut self,
        previous: Option<Arc<Snapshot>>,
        frame_interpolation: f32,
    ) {
        self.previous_inner = previous;
        self.frame_interpolation = frame_interpolation;
    }

    pub fn set_presented_projectiles(&mut self, rows: Vec<PresentedProjectile>) {
        self.presented_projectiles = rows;
    }

    pub fn presented_projectiles(&self) -> &[PresentedProjectile] {
        &self.presented_projectiles
    }

    pub fn remote_sample_provenance(&self, id: ClientId) -> Option<PresentationSampleProvenance> {
        self.remote_provenance.get(&id).copied()
    }

    pub fn view_offset(&self) -> [f32; 3] {
        self.view_offset
    }

    pub fn set_view_offset(&mut self, o: [f32; 3]) {
        self.view_offset = o;
    }

    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.inner.as_deref()
    }

    pub fn tick(&self) -> Option<sim::Tick> {
        self.inner.as_ref().map(|s| s.tick)
    }

    pub fn player(&self, id: ClientId) -> Option<&PlayerState> {
        if let Some(ps) = self.pose.get(&id) {
            return Some(ps);
        }
        self.inner
            .as_ref()?
            .players
            .iter()
            .find(|(c, _)| *c == id)
            .map(|(_, ps)| ps)
    }

    pub fn raw_hand_sample(&self, id: ClientId) -> Option<FpvRawHandSample> {
        let ps = self.player(id)?;
        Some(FpvRawHandSample {
            hands: [
                FpvHandRecord {
                    weap_anim: ps.weap_anim,
                    weapon_time: ps.weapon_time,
                    weapon_delay: ps.weapon_delay,
                    weapon_restrict_kick_time: ps.weapon_restrict_kick_time,
                    weaponstate: ps.weaponstate_primary,
                    weap_hand_flags: ps.weap_hand_flags,
                    weapon_shot_count: ps.weapon_shot_count,
                },
                FpvHandRecord {
                    weap_anim: ps.weap_anim_secondary,
                    weapon_time: ps.weapon_time_secondary,
                    weapon_delay: ps.weapon_delay_secondary,
                    weapon_restrict_kick_time: ps.weapon_restrict_kick_time_secondary,
                    weaponstate: ps.weaponstate_secondary,
                    weap_hand_flags: ps.weap_hand_flags_secondary,
                    weapon_shot_count: ps.weapon_shot_count_secondary,
                },
            ],
            f_weapon_pos_frac: ps.f_weapon_pos_frac,
            ads_delay_time: ps.ads_delay_time,
            last_weapon_hand: ps.last_weapon_hand,
            weapon: ps.weapon,
            off_hand_index: ps.off_hand_index,
            weap_flags: ps.weap_flags,
        })
    }

    pub fn shellshock(&self, id: ClientId) -> Option<&hud_iw4::ShockParams> {
        self.inner
            .as_ref()?
            .meta
            .for_client(id)?
            .shellshock
            .as_ref()
    }

    pub fn alive_player(&self, id: ClientId) -> Option<&PlayerState> {
        let alive = self.inner.as_ref().and_then(|snap| {
            snap.meta
                .for_client(id)
                .map(|meta| meta.lifecycle == ClientLifecycle::Alive)
        })?;
        if !alive {
            return None;
        }
        self.player(id)
    }

    pub fn held_weapon_id(&self, id: ClientId) -> Option<u32> {
        Some(self.alive_player(id)?.weapon)
    }

    pub fn viewweapon_player(&self, id: ClientId) -> Option<&PlayerState> {
        if self
            .snapshot()
            .and_then(|snapshot| snapshot.meta.for_client(id))
            .is_some_and(|meta| meta.controls.weapons_disabled)
        {
            return None;
        }
        let ps = self.player(id)?;
        playerstate_iw4::viewweapon_frame_runs(ps.pm_type).then_some(ps)
    }

    pub fn fpv_cues(&self, id: ClientId) -> FpvEventCues {
        let Some(snap) = self.inner.as_ref() else {
            return FpvEventCues::default();
        };
        fpv_cues_from_events(id, &snap.meta.journal, &snap.meta.entity_events)
    }
}

pub(crate) fn interpolate_player_state(
    previous: &PlayerState,
    next: &PlayerState,
    frame_interpolation: f32,
) -> PlayerState {
    let f = frame_interpolation.max(0.0);
    let lerp = |a: f32, b: f32| (b - a) * f + a;
    let mut out = *next;

    let mut next_bob = next.bob_cycle;
    if next_bob < previous.bob_cycle {
        next_bob += 256;
    }
    out.bob_cycle = previous.bob_cycle + ((next_bob - previous.bob_cycle) as f32 * f) as i32;
    out.leanf = lerp(previous.leanf, next.leanf);
    out.aim_spread_scale = lerp(previous.aim_spread_scale, next.aim_spread_scale);
    out.f_weapon_pos_frac = lerp(previous.f_weapon_pos_frac, next.f_weapon_pos_frac);
    out.view_height_current = lerp(previous.view_height_current, next.view_height_current);
    for axis in 0..3 {
        out.origin[axis] = lerp(previous.origin[axis], next.origin[axis]);
        out.velocity[axis] = lerp(previous.velocity[axis], next.velocity[axis]);
        out.viewangles[axis] = previous.viewangles[axis]
            + math_iw4::angle_subtract(next.viewangles[axis], previous.viewangles[axis]) * f;
        out.delta_angles[axis] = previous.delta_angles[axis]
            + math_iw4::angle_subtract(next.delta_angles[axis], previous.delta_angles[axis]) * f;
    }
    out
}

fn authoritative_projectiles(snapshot: &Snapshot) -> Vec<PresentedProjectile> {
    snapshot
        .projectiles
        .iter()
        .copied()
        .map(PresentedProjectile::Authoritative)
        .collect()
}

fn fpv_cues_from_events(
    subject: ClientId,
    journal: &[EventRecord],
    entity_events: &[EntityEventRecord],
) -> FpvEventCues {
    let mut cues = FpvEventCues::default();
    for event in journal
        .iter()
        .filter(|record| record.audience.projects_to(subject))
        .map(|record| &record.event)
    {
        match *event {
            SimEvent::AttackReleased => cues.attack_released = true,
            SimEvent::Spawned { .. } => cues.spawned = true,
            SimEvent::Died { .. }
            | SimEvent::ClassAccepted { .. }
            | SimEvent::ClassRejected { .. }
            | SimEvent::GiveAccepted { .. }
            | SimEvent::GiveRejected { .. }
            | SimEvent::ConfigurationChangeAccepted { .. }
            | SimEvent::ConfigurationChangeRejected { .. }
            | SimEvent::ScoreChanged { .. }
            | SimEvent::MatchEnded { .. }
            | SimEvent::WeaponSwitchRequested { .. } => {}
        }
    }
    cues.shot_accepted = fire_weapon_count_in(entity_events, subject) > 0;
    cues
}

fn fire_weapon_count_in(entity_events: &[EntityEventRecord], subject: ClientId) -> u32 {
    entity_events
        .iter()
        .filter(|record| {
            (record.event == entity_iw4::EntityEventKind::FIRE_WEAPON
                || record.event == entity_iw4::EntityEventKind::FIRE_WEAPON_LASTSHOT
                || record.event == entity_iw4::EntityEventKind::FIRE_WEAPON_LEFT
                || record.event == entity_iw4::EntityEventKind::FIRE_WEAPON_LASTSHOT_LEFT)
                && record.payload.number == subject.0 as i32
                && record.audience.projects_to(subject)
        })
        .count() as u32
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct ViewweaponAim {
    pub live: bool,
    pub weapon: u32,
    pub angle_offset: [f32; 2],
    pub gun_pitch: f32,
    pub gun_yaw: f32,
    pub xhair_x: f32,
    pub xhair_y: f32,

    pub from_composed_axis: bool,
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalPresentClient(pub ClientId);

impl Default for LocalPresentClient {
    fn default() -> Self {
        Self(ClientId(0))
    }
}
