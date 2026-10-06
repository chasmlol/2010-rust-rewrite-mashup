use bevy::prelude::*;
use entity_iw4::{
    EntityEventAction, EntityEventKind, UnsupportedEntityEvent, consume_entity_events,
    entity_event_action, packet_entity_uses_event_ring,
};
use sim::{EntityEventPayload, EventSequence, Tick};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate as net;
use crate::CEntity;
use crate::CEntitySlots;
use crate::client::input::ClientActionInput;
use crate::client::presentation::centity_runtime::CEntityRuntime;
use crate::client::presentation::entity_event_registry::{EntityEventDispatch, ev_dispatch_row};
use crate::client::presentation::presented::LocalPresentClient;
use crate::client::runtime::{
    ClientPredictionState, LastAdoptedSnapshot, PendingPresentedEntityEvents,
};
use crate::gaps::{NetGapCause, NetIdentityGaps};
use crate::schedule::ClientSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntityEventDomain {
    Snapshot,
    Ring,
    Predicted,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DispatchedEntityEvent {
    pub world: frame::WorldGeneration,
    pub domain: EntityEventDomain,
    pub timeline: u64,
    pub sequence: EventSequence,
    pub tick: Tick,
    pub event: EntityEventKind,
    pub payload: EntityEventPayload,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityRumble {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityEventSound {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityWeaponFire {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(Clone, Copy, Debug)]
pub struct WeaponFirePing {
    pub number: i32,
    pub origin_xy: [f32; 2],
}

#[derive(Resource, Default, Debug)]
pub struct WeaponFirePingBus {
    pub pings: Vec<WeaponFirePing>,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityEjectBrass {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityBulletHit {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityGrenadeContact {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

// Fires before the first events of the other world, so FX for the switch
// are cleared before the replay plays them again.
#[derive(Event, Clone, Copy, Debug, PartialEq, Eq)]
pub struct KillcamFxTransition {
    pub entering: bool,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityExplosion {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityPhysicsSphere {
    pub entity: Entity,
    pub origin: [f32; 3],
    pub outer_radius: f32,
    pub inner_radius: f32,
    pub magnitude: f32,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityPlayFx {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityObituary {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
    pub in_killcam: bool,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityMovementSound {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityResetAds {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

#[derive(EntityEvent, Clone, Copy, Debug, PartialEq)]
pub struct EntityMeleeBlood {
    pub entity: Entity,
    pub event: DispatchedEntityEvent,
}

const TARGET_RETRY_CAPACITY: usize = 8192;
const TARGET_RETRY_WORK: usize = 256;
const TARGET_RETRY_WAIT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug)]
struct DeferredTarget {
    world: frame::WorldGeneration,
    timeline: u64,
    client: sim::ClientId,
    life: sim::LifeSequence,
    deadline: Instant,
}

#[derive(Debug)]
struct DeferredEntityEvent {
    record: sim::EntityEventRecord,
    target: DeferredTarget,
}

#[derive(Resource, Default, Debug)]
pub struct EntityEventCursor {
    seen_through: u32,

    archived_through: Option<(EventSequence, Tick)>,
    in_killcam: bool,
    timeline: u64,
    predicted_shots: u32,
    authority_shots: u32,
    deferred: VecDeque<DeferredEntityEvent>,
}

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppliedEntityEventWalk {
    pub walked: u32,
    pub dispatched: u32,
    pub local_fire: u32,
    pub seen_through: u32,

    pub occupancy_fired: u32,

    pub last_event: i32,

    pub last_number: i32,
}

#[derive(Resource, Default, Debug)]
pub struct UnsupportedEntityEvents {
    pub total: u32,

    pub first: Option<EntityEventKind>,
    warned: bool,
}

impl UnsupportedEntityEvents {
    fn note(&mut self, event: EntityEventKind) {
        self.total = self.total.saturating_add(1);
        if self.first.is_none() {
            self.first = Some(event);
        }
        if !self.warned {
            self.warned = true;

            let (name, why) = match ev_dispatch_row(event) {
                Some(row) => (
                    row.name,
                    match row.dispatch {
                        EntityEventDispatch::Unsupported(reason) => reason,
                        EntityEventDispatch::Observer(_) | EntityEventDispatch::OwnerInput => {
                            "declared as classified in EV_DISPATCH_REGISTRY, yet the classifier \
                             rejected it — G-BUS-1 should have caught this"
                        }
                    },
                ),
                None => (
                    "unmodelled",
                    "not in entity_iw4::EntityEventKind::TAXONOMY: a table slot this build \
                     does not model at all",
                ),
            };
            diag::warn!(
                Net,
                "entity events: {} (#{:#x}) has no client handler — {} \
                 (further unsupported events counted, not printed)",
                name,
                event.0,
                why
            );
        }
    }
}

impl EntityEventCursor {
    pub fn accepts(&mut self, record: &sim::EntityEventRecord, local: sim::ClientId) -> bool {
        if !record.audience.projects_to(local) {
            return false;
        }
        if !record
            .sequence
            .is_newer_than(EventSequence(self.seen_through))
        {
            return false;
        }
        self.seen_through = record.sequence.0;
        true
    }

    pub fn accepts_archived(
        &mut self,
        record: &sim::EntityEventRecord,
        local: sim::ClientId,
    ) -> bool {
        if !record.audience.projects_to(local) {
            return false;
        }
        // Each snapshot re-carries the records of the last temp-event lifetime,
        // so only a tick further back than that window marks a rewind.
        if let Some((sequence, tick)) = self.archived_through
            && tick
                .0
                .saturating_sub(record.tick.0)
                .saturating_mul(sim::MATCH_TICK_MS)
                <= sim::GENTITY_TEMP_EVENT_LIFETIME_MS as u32
            && !record.sequence.is_newer_than(sequence)
        {
            return false;
        }
        if self.archived_through.is_some_and(|(_, tick)| {
            tick.0
                .saturating_sub(record.tick.0)
                .saturating_mul(sim::MATCH_TICK_MS)
                > sim::GENTITY_TEMP_EVENT_LIFETIME_MS as u32
        }) {
            self.timeline = self.timeline.wrapping_add(1);
        }
        self.archived_through = Some((record.sequence, record.tick));
        true
    }

    pub const fn timeline(&self) -> u64 {
        self.timeline
    }

    pub const fn seen_through(&self) -> EventSequence {
        EventSequence(self.seen_through)
    }
}

fn dispatch_entity_events(
    mut commands: Commands,
    adopted: Res<LastAdoptedSnapshot>,
    mut pending: ResMut<PendingPresentedEntityEvents>,
    local: Res<LocalPresentClient>,
    slots: Res<CEntitySlots>,
    generation: Res<frame::WorldGeneration>,
    mut cursor: ResMut<EntityEventCursor>,
    mut walk: ResMut<AppliedEntityEventWalk>,
    mut unsupported: ResMut<UnsupportedEntityEvents>,
    mut target_gaps: ResMut<NetIdentityGaps>,
    mut runtimes: Query<(Entity, &CEntity, &mut CEntityRuntime)>,
) {
    walk.walked = 0;
    walk.dispatched = 0;
    walk.local_fire = 0;
    walk.occupancy_fired = 0;
    walk.last_event = 0;
    walk.last_number = 0;
    walk.seen_through = cursor.seen_through;
    if !adopted.applied_this_frame && cursor.deferred.is_empty() {
        return;
    }
    let tick = adopted
        .next()
        .map(|snapshot| snapshot.tick)
        .unwrap_or(Tick(0));
    let local_number = i32::try_from(local.0.0).unwrap_or(-1);
    let killcam_transition = pending.in_killcam != cursor.in_killcam;
    if killcam_transition {
        cursor.timeline = cursor.timeline.wrapping_add(1);
        cursor.in_killcam = pending.in_killcam;
        cursor.archived_through = None;
        commands.trigger(KillcamFxTransition {
            entering: pending.in_killcam,
        });
    }
    let timeline = cursor.timeline;
    for (entity, identity, mut runtime) in runtimes.iter_mut() {
        if !packet_entity_uses_event_ring(runtime.next_state.e_type) {
            continue;
        }

        let next_state = runtime.next_state;
        // The archive and the live world share entity numbers but not event
        // rings: adopt the new ring position instead of replaying across it.
        if killcam_transition {
            runtime.previous_event_sequence = next_state.event_sequence;
            continue;
        }
        let origin = runtime.origin;
        let mut cursor = runtime.previous_event_sequence;
        consume_entity_events(&next_state, &mut cursor, |ev| {
            walk.occupancy_fired = walk.occupancy_fired.saturating_add(1);
            let number = i32::from(identity.number());
            dispatch_classified(
                &mut commands,
                local_number,
                number,
                entity,
                DispatchedEntityEvent {
                    world: *generation,
                    domain: EntityEventDomain::Ring,
                    timeline,
                    sequence: EventSequence(u32::try_from(ev.sequence).unwrap_or(0)),
                    tick,
                    event: ev.event,
                    payload: EntityEventPayload {
                        number,
                        other_entity_num: next_state.other_entity_num,
                        attacker_entity_num: next_state.attacker_entity_num,
                        event_parm: ev.event_parm,
                        origin,
                        surf_type: (ev.event_parm & 0x1f) as u8,
                        weapon: u32::try_from(next_state.index).unwrap_or(0),
                        ..Default::default()
                    },
                },
                pending.in_killcam,
                &mut walk,
                &mut unsupported,
            );
        });
        runtime.previous_event_sequence = cursor;
    }
    let pending = &mut *pending;
    let count = cursor.deferred.len().min(TARGET_RETRY_WORK);
    let remaining = cursor.deferred.split_off(count);
    let retry = std::mem::replace(&mut cursor.deferred, remaining);
    let records = retry
        .into_iter()
        .map(|retry| (false, retry.record, Some(retry.target)))
        .chain(pending.live.drain(..).map(|record| (false, record, None)))
        .chain(
            pending
                .archived
                .drain(..)
                .map(|record| (true, record, None)),
        );
    let now = Instant::now();
    for (archived, record, retry) in records {
        walk.walked = walk.walked.saturating_add(1);
        if let Some(target) = retry {
            if !record.audience.projects_to(local.0) {
                continue;
            }
            let number = record.payload.number;
            if target.world != *generation || target.timeline != cursor.timeline {
                target_gaps.raise(NetGapCause::EventRetryScopeChanged {
                    number,
                    sequence: record.sequence,
                });
                continue;
            }
            if now >= target.deadline {
                target_gaps.raise(NetGapCause::EventRetryExpired {
                    number,
                    sequence: record.sequence,
                });
                continue;
            }
            let life = adopted
                .next()
                .and_then(|snapshot| snapshot.meta.for_client(target.client))
                .map(|meta| meta.life_sequence);
            if life != Some(target.life) {
                target_gaps.raise(NetGapCause::EventTargetLifeChanged {
                    number,
                    expected: target.life,
                    current: life,
                });
                continue;
            }
        }
        let accepted = if retry.is_some() {
            true
        } else if archived {
            cursor.accepts_archived(&record, local.0)
        } else {
            cursor.accepts(&record, local.0)
        };
        if !accepted {
            continue;
        }
        if !archived
            && retry.is_none()
            && record.payload.number == local_number
            && entity_event_action(record.event) == Ok(EntityEventAction::WeaponFire)
        {
            cursor.authority_shots = cursor.authority_shots.wrapping_add(1);
            perf::owner_shot("authority", cursor.authority_shots, record.tick.0);
        }
        walk.last_event = record.event.0;
        walk.last_number = record.payload.number;

        let dispatched = DispatchedEntityEvent {
            world: *generation,
            domain: EntityEventDomain::Snapshot,
            timeline: cursor.timeline,
            sequence: record.sequence,
            tick: record.tick,
            event: record.event,
            payload: record.payload,
        };
        let number = record.payload.number;
        if !archived
            && let Some(cause) = record.payload.fire_cause
            && i32::try_from(cause.client.0).ok() == Some(number)
        {
            let life = adopted
                .next()
                .and_then(|snapshot| snapshot.meta.for_client(cause.client))
                .map(|meta| meta.life_sequence);
            if life.is_some_and(|life| life != cause.life) {
                target_gaps.raise(NetGapCause::EventTargetLifeChanged {
                    number,
                    expected: cause.life,
                    current: life,
                });
                continue;
            }
        }
        let resolved = match u16::try_from(number) {
            Ok(number) => slots.entity_for_number(number),
            Err(_) => {
                target_gaps.raise(NetGapCause::EventNumberOutOfRange { number });
                continue;
            }
        };

        let entity = match super::entity_event_recipient::recipient(record.event, resolved) {
            Some(entity) => entity,
            None => {
                target_gaps.raise(NetGapCause::EventNumberHasNoEntity { number });
                if !archived {
                    let target = retry.or_else(|| {
                        let client = sim::ClientId(u32::try_from(number).ok()?);
                        let life = adopted.next()?.meta.for_client(client)?.life_sequence;
                        Some(DeferredTarget {
                            world: *generation,
                            timeline: cursor.timeline,
                            client,
                            life,
                            deadline: now + TARGET_RETRY_WAIT,
                        })
                    });
                    if let Some(target) = target {
                        if cursor.deferred.len() == TARGET_RETRY_CAPACITY {
                            target_gaps.raise(NetGapCause::EventRetryBudget {
                                number,
                                sequence: record.sequence,
                            });
                        } else {
                            cursor
                                .deferred
                                .push_back(DeferredEntityEvent { record, target });
                        }
                    }
                }
                continue;
            }
        };
        dispatch_classified(
            &mut commands,
            local_number,
            number,
            entity,
            dispatched,
            archived,
            &mut walk,
            &mut unsupported,
        );
    }
    walk.seen_through = cursor.seen_through;
}

fn dispatch_owner_events(
    mut commands: Commands,
    mut prediction: ResMut<ClientPredictionState>,
    (local, generation): (Res<LocalPresentClient>, Res<frame::WorldGeneration>),
    slots: Res<CEntitySlots>,
    verdicts: Res<crate::FireVerdictState>,
    mut cursor: ResMut<EntityEventCursor>,
    mut walk: ResMut<AppliedEntityEventWalk>,
    mut unsupported: ResMut<UnsupportedEntityEvents>,
) {
    let local_number = i32::try_from(local.0.0).unwrap_or(-1);
    if cursor.in_killcam {
        prediction.0.take_owner_events();
        return;
    }
    let Some(entity) = u16::try_from(local_number)
        .ok()
        .and_then(|number| slots.entity_for_number(number))
    else {
        return;
    };
    let life = prediction.0.local_life();
    for record in prediction.0.take_owner_events() {
        if record.payload.fire_cause.is_some_and(|cause| {
            cause.client != local.0
                || Some(cause.life) != life
                || verdicts.status(*generation, cause) == crate::PredictedFireStatus::Refused
        }) {
            continue;
        }
        if entity_event_action(record.event) == Ok(EntityEventAction::WeaponFire) {
            cursor.predicted_shots = cursor.predicted_shots.wrapping_add(1);
            perf::owner_shot("predicted", cursor.predicted_shots, record.tick.0);
        }
        dispatch_classified(
            &mut commands,
            local_number,
            local_number,
            entity,
            DispatchedEntityEvent {
                world: *generation,
                domain: EntityEventDomain::Predicted,
                timeline: cursor.timeline,
                sequence: record.sequence,
                tick: record.tick,
                event: record.event,
                payload: record.payload,
            },
            false,
            &mut walk,
            &mut unsupported,
        );
    }
}

fn dispatch_classified(
    commands: &mut Commands,
    local_number: i32,
    number: i32,
    entity: Entity,
    dispatched: DispatchedEntityEvent,
    in_killcam: bool,
    walk: &mut AppliedEntityEventWalk,
    unsupported: &mut UnsupportedEntityEvents,
) {
    let mut did = false;
    match entity_event_action(dispatched.event) {
        Ok(EntityEventAction::None | EntityEventAction::OwnerStance) => {}
        Ok(EntityEventAction::Sound) => {
            did = true;
            commands.trigger(EntityEventSound {
                entity,
                event: dispatched,
            });
        }
        Ok(EntityEventAction::WeaponFire) => {
            did = true;
            if number == local_number {
                walk.local_fire = walk.local_fire.saturating_add(1);
            }
            commands.trigger(EntityWeaponFire {
                entity,
                event: dispatched,
            });
        }
        Ok(EntityEventAction::EjectBrass) => {
            did = true;
            commands.trigger(EntityEjectBrass {
                entity,
                event: dispatched,
            });
        }
        Ok(EntityEventAction::BulletHit) => {
            did = true;
            commands.trigger(EntityBulletHit {
                entity,
                event: dispatched,
            });
        }
        Ok(EntityEventAction::GrenadeContact) => {
            did = true;
            commands.trigger(EntityGrenadeContact {
                entity,
                event: dispatched,
            });
        }
        Ok(EntityEventAction::Explosion) => {
            did = true;
            commands.trigger(EntityExplosion {
                entity,
                event: dispatched,
            });
        }
        Ok(EntityEventAction::Rumble) => {
            did = true;
            commands.trigger(EntityRumble {
                entity,
                event: dispatched,
            });
        }
        Ok(EntityEventAction::PhysicsSphere) => {
            did = true;
            let [outer_radius, inner_radius, magnitude] = dispatched.payload.origin2;
            commands.trigger(EntityPhysicsSphere {
                entity,
                origin: dispatched.payload.origin,
                outer_radius,
                inner_radius,
                magnitude,
            });
        }
        Ok(EntityEventAction::PlayFx) => {
            did = true;
            commands.trigger(EntityPlayFx {
                entity,
                event: dispatched,
            });
        }
        Ok(EntityEventAction::Obituary) => {
            did = true;
            commands.trigger(EntityObituary {
                entity,
                event: dispatched,
                in_killcam,
            });
        }
        Ok(EntityEventAction::MovementSound) => {
            did = true;
            commands.trigger(EntityMovementSound {
                entity,
                event: dispatched,
            });
        }
        Ok(EntityEventAction::ResetAds) => {
            did = true;
            commands.trigger(EntityResetAds {
                entity,
                event: dispatched,
            });
        }
        Ok(EntityEventAction::MeleeBlood) => {
            did = true;
            commands.trigger(EntityMeleeBlood {
                entity,
                event: dispatched,
            });
        }
        Err(UnsupportedEntityEvent(event)) => {
            did = true;
            unsupported.note(event);
        }
    }
    if did {
        walk.dispatched = walk.dispatched.saturating_add(1);
    }
}

fn set_ads_from_reset(
    reset: On<net::EntityResetAds>,
    mut input: ResMut<ClientActionInput>,
    local: Res<LocalPresentClient>,
) {
    if (*reset).event.payload.number != local.0.0 as i32 {
        return;
    }
    input_iw4::set_ads(&mut input.client, false);
}

pub fn register_entity_event_dispatch(app: &mut App) {
    app.init_resource::<EntityEventCursor>()
        .init_resource::<frame::WorldGeneration>()
        .init_resource::<AppliedEntityEventWalk>()
        .init_resource::<UnsupportedEntityEvents>()
        .init_resource::<NetIdentityGaps>()
        .init_resource::<WeaponFirePingBus>()
        .add_observer(set_ads_from_reset)
        .add_systems(
            Update,
            (
                dispatch_entity_events
                    .in_set(ClientSet::Reconcile)
                    .after(crate::sync_client_entities),
                dispatch_owner_events
                    .in_set(frame::OwnerEventsPublished)
                    .after(crate::predict_local_move),
            ),
        );
}
