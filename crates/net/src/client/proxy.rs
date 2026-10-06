use std::collections::VecDeque;
use std::sync::Arc;

use playerstate_iw4::PlayerState;
use sim::{ClientId, Snapshot};

use crate::authority::inbox::AUTHORITY_MS;

pub const PROXY_BUFFER_TICKS: usize = 32;

pub const PROXY_DELAY_MS: i32 = 100;

const PROXY_DELAY_MIN_MS: i32 = AUTHORITY_MS;
const PROXY_DELAY_MAX_MS: i32 = 250;
const PROXY_DELAY_MARGIN_MS: i32 = 8;
const PROXY_DELAY_RISE_MS: i32 = 20;
const PROXY_DELAY_DECAY_MS: i32 = 2;
const PROXY_GAP_OUTLIERS: usize = PROXY_GAP_WINDOW / 10;
const PROXY_GAP_WINDOW: usize = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PresentationSampleTime(pub i32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProxyPolicyRevision(pub u16);

pub const ADAPTIVE_DELAY_POLICY_REVISION: ProxyPolicyRevision = ProxyPolicyRevision(4);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProxyStarvationReason {
    EmptyBuffer,
    BeforeOldest,
    AfterNewest,
    MissingEntityRow,
}

impl ProxyStarvationReason {
    pub fn dump_label(self) -> &'static str {
        match self {
            Self::EmptyBuffer => "empty",
            Self::BeforeOldest => "before",
            Self::AfterNewest => "after",
            Self::MissingEntityRow => "missing",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PresentationSampleOutcome {
    Exact {
        snapshot: sim::Tick,
    },
    Interpolated {
        left: sim::Tick,
        right: sim::Tick,
        alpha: f32,
    },
    Starved {
        reason: ProxyStarvationReason,
        held_snapshot: Option<sim::Tick>,
    },
}

impl PresentationSampleOutcome {
    pub fn dump_kind(self) -> &'static str {
        match self {
            Self::Exact { .. } => "exact",
            Self::Interpolated { .. } => "interpolated",
            Self::Starved { .. } => "starved",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresentationSampleProvenance {
    pub policy_revision: ProxyPolicyRevision,
    pub effective_time: PresentationSampleTime,
    pub outcome: PresentationSampleOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProxySample {
    Pose {
        ps: PlayerState,
        provenance: PresentationSampleProvenance,
    },
    Starved {
        held: Option<PlayerState>,
        provenance: PresentationSampleProvenance,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimGap {
    None,
}

#[derive(Clone, Debug)]
struct BufferedSnapshot {
    snap: Arc<Snapshot>,
    time_ms: i32,
}

#[derive(Clone, Debug)]
pub struct RemoteProxy {
    buffer: VecDeque<BufferedSnapshot>,
    arrival_gaps: VecDeque<i32>,
    delay_ms: i32,
}

impl Default for RemoteProxy {
    fn default() -> Self {
        Self {
            buffer: VecDeque::new(),
            arrival_gaps: VecDeque::new(),
            delay_ms: PROXY_DELAY_MS,
        }
    }
}

impl RemoteProxy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn delay_ms(&self) -> i32 {
        self.delay_ms
    }

    pub fn push(&mut self, snapshot: &Snapshot) {
        self.push_arc(Arc::new(snapshot.clone()), None);
    }

    pub fn push_arc(&mut self, snapshot: Arc<Snapshot>, render_time_ms: Option<i32>) {
        let time_ms = snapshot_time_ms(&snapshot);
        if self
            .buffer
            .back()
            .is_some_and(|last| time_ms <= last.time_ms)
        {
            return;
        }
        if let (Some(render), Some(newest)) = (render_time_ms, self.buffer.back()) {
            self.adapt_delay(render.saturating_sub(newest.time_ms));
        }
        self.buffer.push_back(BufferedSnapshot {
            time_ms,
            snap: snapshot,
        });
        while self.buffer.len() > PROXY_BUFFER_TICKS {
            self.buffer.pop_front();
        }
    }

    fn adapt_delay(&mut self, gap_ms: i32) {
        self.arrival_gaps.push_back(gap_ms);
        while self.arrival_gaps.len() > PROXY_GAP_WINDOW {
            self.arrival_gaps.pop_front();
        }
        let mut gaps: Vec<i32> = self.arrival_gaps.iter().copied().collect();
        gaps.sort_unstable();
        let covered = gaps[gaps.len() - 1 - PROXY_GAP_OUTLIERS.min(gaps.len() - 1)];
        let target = covered
            .saturating_add(PROXY_DELAY_MARGIN_MS)
            .clamp(PROXY_DELAY_MIN_MS, PROXY_DELAY_MAX_MS);
        self.delay_ms = if target > self.delay_ms {
            self.delay_ms + (target - self.delay_ms).min(PROXY_DELAY_RISE_MS)
        } else {
            self.delay_ms - (self.delay_ms - target).min(PROXY_DELAY_DECAY_MS)
        };
    }

    pub fn shot_sample(&self, render_time_ms: i32) -> sim::ShotSampleProvenance {
        use sim::ShotSampleQuality;

        let effective = render_time_ms.saturating_sub(self.delay_ms);
        let (Some(oldest), Some(newest)) = (self.buffer.front(), self.buffer.back()) else {
            return sim::ShotSampleProvenance::default();
        };
        if effective < oldest.time_ms {
            return sim::ShotSampleProvenance {
                left: oldest.snap.tick,
                right: oldest.snap.tick,
                alpha: 0.0,
                quality: ShotSampleQuality::Held,
            };
        }
        if effective > newest.time_ms {
            return sim::ShotSampleProvenance {
                left: newest.snap.tick,
                right: newest.snap.tick,
                alpha: 0.0,
                quality: ShotSampleQuality::Starved,
            };
        }
        let mut left: Option<&BufferedSnapshot> = None;
        let mut right: Option<&BufferedSnapshot> = None;
        for entry in &self.buffer {
            if entry.time_ms <= effective {
                left = Some(entry);
            }
            if entry.time_ms >= effective {
                right = Some(entry);
                break;
            }
        }
        let (Some(left), Some(right)) = (left, right) else {
            return sim::ShotSampleProvenance::default();
        };
        if left.time_ms == right.time_ms {
            return sim::ShotSampleProvenance {
                left: left.snap.tick,
                right: right.snap.tick,
                alpha: 0.0,
                quality: ShotSampleQuality::Exact,
            };
        }
        let span = (right.time_ms - left.time_ms) as f32;
        let alpha = ((effective - left.time_ms) as f32 / span).clamp(0.0, 1.0);
        sim::ShotSampleProvenance {
            left: left.snap.tick,
            right: right.snap.tick,
            alpha,
            quality: ShotSampleQuality::Interpolated,
        }
    }

    pub fn snapshot_at(&self, client: ClientId, render_time_ms: i32) -> Option<Arc<Snapshot>> {
        let effective = render_time_ms.saturating_sub(self.delay_ms);
        let (_, newest) = self.last_sample(client)?;
        let epoch = || {
            self.buffer.iter().rev().take_while(|entry| {
                player_row(&entry.snap.players, client)
                    .is_some_and(|ps| same_presentation_epoch(ps, &newest))
            })
        };
        epoch()
            .find(|entry| entry.time_ms <= effective)
            .or_else(|| epoch().last())
            .map(|entry| Arc::clone(&entry.snap))
    }

    pub(crate) fn snapshot_after(
        &self,
        client: ClientId,
        render_time_ms: i32,
    ) -> Option<Arc<Snapshot>> {
        let effective = render_time_ms.saturating_sub(self.delay_ms);
        let (_, newest) = self.last_sample(client)?;
        self.buffer
            .iter()
            .rev()
            .take_while(|entry| {
                player_row(&entry.snap.players, client)
                    .is_some_and(|ps| same_presentation_epoch(ps, &newest))
            })
            .filter(|entry| entry.time_ms > effective)
            .last()
            .map(|entry| Arc::clone(&entry.snap))
    }

    pub fn interpolate_at(&self, client: ClientId, render_time_ms: i32) -> ProxySample {
        let sample = self.sample_at(client, render_time_ms);
        self.hold_across_transition(client, sample)
    }

    fn hold_across_transition(&self, client: ClientId, sample: ProxySample) -> ProxySample {
        let (sampled, provenance) = match &sample {
            ProxySample::Pose { ps, provenance } => (Some(ps), *provenance),
            ProxySample::Starved { held, provenance } => (held.as_ref(), *provenance),
        };
        let (Some(sampled), Some((_, newest))) = (sampled, self.last_sample(client)) else {
            return sample;
        };
        if same_presentation_epoch(sampled, &newest) {
            return sample;
        }
        let first_after = self
            .buffer
            .iter()
            .rev()
            .filter_map(|entry| {
                player_row(&entry.snap.players, client).map(|ps| (entry.snap.tick, ps))
            })
            .take_while(|(_, ps)| same_presentation_epoch(ps, &newest))
            .last();
        let Some((tick, ps)) = first_after else {
            return sample;
        };
        ProxySample::Pose {
            ps: *ps,
            provenance: PresentationSampleProvenance {
                outcome: PresentationSampleOutcome::Exact { snapshot: tick },
                ..provenance
            },
        }
    }

    fn sample_at(&self, client: ClientId, render_time_ms: i32) -> ProxySample {
        let effective_time = PresentationSampleTime(render_time_ms.saturating_sub(self.delay_ms));
        let held = if self
            .buffer
            .front()
            .is_some_and(|first| effective_time.0 < first.time_ms)
        {
            self.buffer.front().and_then(|entry| {
                player_row(&entry.snap.players, client).map(|ps| (entry.snap.tick, *ps))
            })
        } else {
            self.last_sample(client)
        };
        let starved = |reason| ProxySample::Starved {
            held: held.map(|(_, ps)| ps),
            provenance: PresentationSampleProvenance {
                policy_revision: ADAPTIVE_DELAY_POLICY_REVISION,
                effective_time,
                outcome: PresentationSampleOutcome::Starved {
                    reason,
                    held_snapshot: held.map(|(tick, _)| tick),
                },
            },
        };

        let Some(oldest) = self.buffer.front() else {
            return starved(ProxyStarvationReason::EmptyBuffer);
        };
        let Some(newest) = self.buffer.back() else {
            return starved(ProxyStarvationReason::EmptyBuffer);
        };
        if effective_time.0 < oldest.time_ms {
            return starved(ProxyStarvationReason::BeforeOldest);
        }
        if effective_time.0 > newest.time_ms {
            return starved(ProxyStarvationReason::AfterNewest);
        }

        if let Some(exact) = self
            .buffer
            .iter()
            .find(|entry| entry.time_ms == effective_time.0)
        {
            return match player_row(&exact.snap.players, client) {
                Some(ps) => ProxySample::Pose {
                    ps: *ps,
                    provenance: PresentationSampleProvenance {
                        policy_revision: ADAPTIVE_DELAY_POLICY_REVISION,
                        effective_time,
                        outcome: PresentationSampleOutcome::Exact {
                            snapshot: exact.snap.tick,
                        },
                    },
                },
                None => starved(ProxyStarvationReason::MissingEntityRow),
            };
        }

        let mut before: Option<&BufferedSnapshot> = None;
        let mut after: Option<&BufferedSnapshot> = None;
        for entry in &self.buffer {
            if entry.time_ms <= effective_time.0 {
                before = Some(entry);
            }
            if entry.time_ms >= effective_time.0 {
                after = Some(entry);
                break;
            }
        }

        let (Some(left), Some(right)) = (before, after) else {
            return starved(ProxyStarvationReason::MissingEntityRow);
        };
        let (Some(ps0), Some(ps1)) = (
            player_row(&left.snap.players, client),
            player_row(&right.snap.players, client),
        ) else {
            return starved(ProxyStarvationReason::MissingEntityRow);
        };
        let life_changed = left
            .snap
            .meta
            .for_client(client)
            .zip(right.snap.meta.for_client(client))
            .is_some_and(|(a, b)| a.life_sequence != b.life_sequence);
        if life_changed || !same_presentation_epoch(ps0, ps1) {
            return ProxySample::Pose {
                ps: *ps1,
                provenance: PresentationSampleProvenance {
                    policy_revision: ADAPTIVE_DELAY_POLICY_REVISION,
                    effective_time,
                    outcome: PresentationSampleOutcome::Exact {
                        snapshot: right.snap.tick,
                    },
                },
            };
        }
        let span = (right.time_ms - left.time_ms) as f32;
        let alpha = (effective_time.0 - left.time_ms) as f32 / span;
        ProxySample::Pose {
            ps: lerp_player_state(ps0, ps1, alpha),
            provenance: PresentationSampleProvenance {
                policy_revision: ADAPTIVE_DELAY_POLICY_REVISION,
                effective_time,
                outcome: PresentationSampleOutcome::Interpolated {
                    left: left.snap.tick,
                    right: right.snap.tick,
                    alpha,
                },
            },
        }
    }

    fn last_sample(&self, client: ClientId) -> Option<(sim::Tick, PlayerState)> {
        self.buffer.iter().rev().find_map(|entry| {
            player_row(&entry.snap.players, client).map(|ps| (entry.snap.tick, *ps))
        })
    }
}

fn same_presentation_epoch(a: &PlayerState, b: &PlayerState) -> bool {
    a.is_live_frame() == b.is_live_frame()
        && a.kill_cam_client_num == b.kill_cam_client_num
        && (a.e_flags & playerstate_iw4::eflags::TELEPORT)
            == (b.e_flags & playerstate_iw4::eflags::TELEPORT)
}

fn snapshot_time_ms(snapshot: &Snapshot) -> i32 {
    snapshot.tick.0 as i32 * AUTHORITY_MS
}

fn player_row(players: &[(ClientId, PlayerState)], client: ClientId) -> Option<&PlayerState> {
    players
        .iter()
        .find(|(id, _)| *id == client)
        .map(|(_, ps)| ps)
}

fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        lerp_f32(a[0], b[0], t),
        lerp_f32(a[1], b[1], t),
        lerp_f32(a[2], b[2], t),
    ]
}

fn lerp_player_state(a: &PlayerState, b: &PlayerState, alpha: f32) -> PlayerState {
    let mut out = *a;
    if (a.e_flags ^ b.e_flags) & playerstate_iw4::eflags::TELEPORT != 0 || a.pm_type != b.pm_type {
        return out;
    }
    out.origin = lerp3(a.origin, b.origin, alpha);
    out.velocity = lerp3(a.velocity, b.velocity, alpha);
    for axis in 0..3 {
        out.viewangles[axis] = a.viewangles[axis]
            + alpha * math_iw4::angle_subtract(b.viewangles[axis], a.viewangles[axis]);
        out.delta_angles[axis] = a.delta_angles[axis]
            + alpha * math_iw4::angle_subtract(b.delta_angles[axis], a.delta_angles[axis]);
    }
    out.leanf = lerp_f32(a.leanf, b.leanf, alpha);
    out.view_height_current = lerp_f32(a.view_height_current, b.view_height_current, alpha);
    out.f_weapon_pos_frac = lerp_f32(a.f_weapon_pos_frac, b.f_weapon_pos_frac, alpha);
    out.aim_spread_scale = lerp_f32(a.aim_spread_scale, b.aim_spread_scale, alpha);
    out.move_speed_scale_multiplier = lerp_f32(
        a.move_speed_scale_multiplier,
        b.move_speed_scale_multiplier,
        alpha,
    );
    out.jump_origin_z = lerp_f32(a.jump_origin_z, b.jump_origin_z, alpha);
    out.mantle_yaw = lerp_f32(a.mantle_yaw, b.mantle_yaw, alpha);
    out
}
