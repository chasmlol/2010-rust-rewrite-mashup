use bevy::prelude::*;
use playerstate_iw4::{ENTITYNUM_NONE, PLAYER_CORPSE_ENTITY_BASE};
use sim::{ClientId, PlayerCorpsePool, PlayerCorpseSlot};

use crate::client::presentation::centity_runtime::{
    CEntityRuntime, apply_existing, corpse_slot_to_entity_state, player_state_to_entity_state,
    reset_entity, shutdown_entity,
};
use crate::gaps::{NetGapCause, NetIdentityGaps};
use crate::{AuthoritySet, ClientSet, LastAdoptedSnapshot, ServerTick};

pub const CLIENT_ENTITY_SLOT_COUNT: usize = ENTITYNUM_NONE as usize;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(immutable)]
pub struct CEntity {
    number: u16,
    client: Option<ClientId>,
}

impl CEntity {
    pub const fn number(&self) -> u16 {
        self.number
    }

    pub const fn client(&self) -> Option<ClientId> {
        self.client
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CEntitySlot {
    client: Option<ClientId>,
    entity: Entity,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct CEntityBirthCensus {
    pub authority_births: u32,
    pub client_births: u32,
    pub client_applies: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CEntitySyncArm {
    Authority,
    Client,
}

impl CEntityBirthCensus {
    fn note_birth(&mut self, arm: CEntitySyncArm) {
        match arm {
            CEntitySyncArm::Authority => {
                self.authority_births = self.authority_births.saturating_add(1);
            }
            CEntitySyncArm::Client => {
                self.client_births = self.client_births.saturating_add(1);
            }
        }
    }

    fn note_apply(&mut self, arm: CEntitySyncArm) {
        if arm == CEntitySyncArm::Client {
            self.client_applies = self.client_applies.saturating_add(1);
        }
    }
}

#[derive(Resource, Debug)]
pub struct CEntitySlots {
    slots: Vec<Option<CEntitySlot>>,
}

impl Default for CEntitySlots {
    fn default() -> Self {
        Self {
            slots: vec![None; CLIENT_ENTITY_SLOT_COUNT],
        }
    }
}

impl CEntitySlots {
    pub fn entity_for_number(&self, number: u16) -> Option<Entity> {
        self.slots
            .get(usize::from(number))
            .and_then(|slot| slot.map(|slot| slot.entity))
    }

    pub fn entity_for_client(&self, client: ClientId) -> Option<Entity> {
        self.slots
            .iter()
            .flatten()
            .find(|slot| slot.client == Some(client))
            .map(|slot| slot.entity)
    }

    pub fn iter(&self) -> impl Iterator<Item = (u16, Option<ClientId>, Entity)> + '_ {
        self.slots.iter().enumerate().filter_map(|(number, slot)| {
            let slot = (*slot)?;
            Some((number as u16, slot.client, slot.entity))
        })
    }
}

fn player_entity_number(client: ClientId) -> Option<u16> {
    let number = usize::try_from(client.0).ok()?;
    (number < CLIENT_ENTITY_SLOT_COUNT).then_some(number as u16)
}

fn snapshot_time_ms(tick: sim::Tick) -> i32 {
    crate::ServerTime::from_tick(tick).ms()
}

pub fn sync_client_entities(
    mut commands: Commands,
    adopted: Res<LastAdoptedSnapshot>,
    proxy: Option<Res<crate::RemoteProxyState>>,
    clock: Option<Res<crate::FrameClock>>,
    local: Option<Res<crate::LocalPresentClient>>,
    mut selected_tick: Local<Option<(sim::Tick, bool)>>,
    mut slots: ResMut<CEntitySlots>,
    mut gaps: ResMut<NetIdentityGaps>,
    identities: Query<&CEntity>,
    mut runtimes: Query<&mut CEntityRuntime>,
    mut census: ResMut<CEntityBirthCensus>,
) {
    let archived = adopted.next().is_some_and(|snap| {
        snap.players.iter().any(|(id, ps)| {
            local.as_ref().is_some_and(|local| *id == local.0) && !ps.is_live_frame()
        })
    });
    let selected = if archived {
        proxy
            .as_ref()
            .zip(clock.as_ref())
            .zip(local.as_ref())
            .and_then(|((proxy, clock), local)| proxy.0.snapshot_at(local.0, clock.time()))
    } else {
        None
    };
    let next = selected.as_deref().or_else(|| adopted.next());
    let key = next.map(|snap| (snap.tick, archived));
    if archived {
        if *selected_tick == key {
            return;
        }
    } else if !client_entity_sync_needed(adopted.applied_this_frame, next.is_some())
        && *selected_tick == key
    {
        return;
    }
    *selected_tick = key;
    let players = next
        .map(|snapshot| snapshot.players.as_slice())
        .unwrap_or_default();
    let empty = PlayerCorpsePool::default();
    let corpses = next
        .map(|snapshot| &snapshot.meta.corpses)
        .unwrap_or(&empty);
    let empty_entities: &[entity_iw4::EntityState] = &[];
    let entities = next
        .map(|snapshot| snapshot.meta.entities.as_slice())
        .unwrap_or(empty_entities);
    let at_time_ms = next
        .map(|snapshot| snapshot_time_ms(snapshot.tick))
        .unwrap_or(0);
    sync_snapshot_entities(
        &mut commands,
        players,
        corpses,
        entities,
        &mut slots,
        &mut gaps,
        &identities,
        &mut runtimes,
        at_time_ms,
        adopted.snap.is_some(),
        archived
            .then(|| local.as_ref().map(|local| local.0))
            .flatten(),
        CEntitySyncArm::Client,
        Some(&mut census),
    );
}

pub(crate) fn sync_authority_entities(
    mut commands: Commands,
    server_tick: Res<ServerTick>,
    mut slots: ResMut<CEntitySlots>,
    mut gaps: ResMut<NetIdentityGaps>,
    identities: Query<&CEntity>,
    mut runtimes: Query<&mut CEntityRuntime>,
    mut census: ResMut<CEntityBirthCensus>,
) {
    let players = server_tick
        .0
        .as_ref()
        .map(|tick| tick.snapshot.players.as_slice())
        .unwrap_or_default();
    let empty = PlayerCorpsePool::default();
    let corpses = server_tick
        .0
        .as_ref()
        .map(|tick| &tick.snapshot.meta.corpses)
        .unwrap_or(&empty);
    let empty_entities: &[entity_iw4::EntityState] = &[];
    let entities = server_tick
        .0
        .as_ref()
        .map(|tick| tick.snapshot.meta.entities.as_slice())
        .unwrap_or(empty_entities);
    let at_time_ms = server_tick
        .0
        .as_ref()
        .map(|tick| snapshot_time_ms(tick.snapshot.tick))
        .unwrap_or(0);
    sync_snapshot_entities(
        &mut commands,
        players,
        corpses,
        entities,
        &mut slots,
        &mut gaps,
        &identities,
        &mut runtimes,
        at_time_ms,
        false,
        None,
        CEntitySyncArm::Authority,
        Some(&mut census),
    );
}

#[derive(Clone, Copy)]
struct WantedOccupant {
    client: Option<ClientId>,
    next_state: entity_iw4::EntityState,
}

fn occupant_from_player(client: ClientId, ps: &playerstate_iw4::PlayerState) -> WantedOccupant {
    WantedOccupant {
        client: Some(client),
        next_state: player_state_to_entity_state(client, ps),
    }
}

fn occupant_from_corpse(slot: &PlayerCorpseSlot) -> WantedOccupant {
    WantedOccupant {
        client: Some(slot.victim),
        next_state: corpse_slot_to_entity_state(slot),
    }
}

fn occupant_from_replicated(es: entity_iw4::EntityState) -> WantedOccupant {
    let client = (es.e_type == entity_iw4::ET_PLAYER)
        .then(|| u32::try_from(es.client_num).ok().map(ClientId))
        .flatten();
    WantedOccupant {
        client,
        next_state: es,
    }
}

fn client_entity_sync_needed(applied_this_frame: bool, has_next: bool) -> bool {
    applied_this_frame || !has_next
}

fn replicated_entity_occupies(es: &entity_iw4::EntityState) -> bool {
    es.e_type == entity_iw4::ET_SCRIPTMOVER
        || es.e_type == entity_iw4::ET_MISSILE
        || es.e_type == entity_iw4::ET_ITEM
        || es.e_type == entity_iw4::ET_PLAYER
}

fn sync_snapshot_entities(
    commands: &mut Commands,
    players: &[(ClientId, playerstate_iw4::PlayerState)],
    corpses: &PlayerCorpsePool,
    entities: &[entity_iw4::EntityState],
    slots: &mut CEntitySlots,
    gaps: &mut NetIdentityGaps,
    identities: &Query<&CEntity>,
    runtimes: &mut Query<&mut CEntityRuntime>,
    at_time_ms: i32,
    pair_ready: bool,
    seated_viewer: Option<ClientId>,
    arm: CEntitySyncArm,
    mut census: Option<&mut CEntityBirthCensus>,
) {
    if pair_ready {
        for mut runtime in runtimes.iter_mut() {
            if runtime.in_next_snap() {
                runtime.transition_copy_lerp();
            }
        }
    }

    let mut claimed = [false; CLIENT_ENTITY_SLOT_COUNT];
    let mut wanted: Vec<(u16, WantedOccupant)> = Vec::new();
    let mut claim = |number: u16, occupant: WantedOccupant, gaps: &mut NetIdentityGaps| {
        let index = usize::from(number);
        if claimed[index] {
            gaps.raise(NetGapCause::NumberClaimedTwice { number });
            return;
        }
        claimed[index] = true;
        wanted.push((number, occupant));
    };

    for (client, ps) in players {
        if Some(*client) == seated_viewer {
            continue;
        }
        let Some(number) = player_entity_number(*client) else {
            gaps.raise(NetGapCause::ClientOutOfNumberSpace { client: *client });
            continue;
        };
        claim(number, occupant_from_player(*client, ps), gaps);
    }

    for slot in &corpses.slots {
        if !slot.occupied || slot.entnum < PLAYER_CORPSE_ENTITY_BASE {
            continue;
        }
        let Ok(number) = u16::try_from(slot.entnum) else {
            continue;
        };
        if usize::from(number) >= CLIENT_ENTITY_SLOT_COUNT {
            continue;
        }
        claim(number, occupant_from_corpse(slot), gaps);
    }

    for es in entities {
        if !replicated_entity_occupies(es) {
            continue;
        }
        let Ok(number) = u16::try_from(es.number) else {
            continue;
        };
        if usize::from(number) >= CLIENT_ENTITY_SLOT_COUNT {
            continue;
        }
        claim(number, occupant_from_replicated(*es), gaps);
    }

    for (number, occupant) in wanted {
        let index = usize::from(number);
        let client = occupant.client;
        let next_state = occupant.next_state;
        let valid = slots.slots[index].is_some_and(|slot| {
            identities
                .get(slot.entity)
                .is_ok_and(|identity| identity.number == number && identity.client == client)
                && runtimes.get(slot.entity).is_ok()
        });
        if valid {
            let entity = slots.slots[index].expect("valid slot").entity;
            if let Ok(mut runtime) = runtimes.get_mut(entity) {
                apply_existing(&mut runtime, next_state, at_time_ms);
            }
            if let Some(census) = census.as_mut() {
                census.note_apply(arm);
            }
            continue;
        }
        if let Some(stale) = slots.slots[index].take() {
            if let Ok(mut runtime) = runtimes.get_mut(stale.entity) {
                shutdown_entity(&mut runtime);
            }
            commands.entity(stale.entity).despawn();
        }
        let mut runtime = CEntityRuntime::default();
        reset_entity(&mut runtime, next_state, at_time_ms, true);
        let entity = commands.spawn((CEntity { number, client }, runtime)).id();
        slots.slots[index] = Some(CEntitySlot { client, entity });
        if let Some(census) = census.as_mut() {
            census.note_birth(arm);
        }
    }

    for (index, slot) in slots.slots.iter_mut().enumerate() {
        if claimed[index] {
            continue;
        }
        if let Some(stale) = slot.take() {
            if let Ok(mut runtime) = runtimes.get_mut(stale.entity) {
                shutdown_entity(&mut runtime);
            }
            commands.entity(stale.entity).despawn();
        }
    }

    gaps.report();
}

pub(crate) fn register_authority_entities(app: &mut App) {
    app.init_resource::<CEntitySlots>()
        .init_resource::<NetIdentityGaps>()
        .init_resource::<CEntityBirthCensus>()
        .add_systems(
            FixedUpdate,
            sync_authority_entities.in_set(AuthoritySet::Fanout),
        );
}

pub fn register_client_entities(app: &mut App) {
    app.init_resource::<CEntitySlots>()
        .init_resource::<NetIdentityGaps>()
        .init_resource::<CEntityBirthCensus>()
        .add_systems(
            Update,
            sync_client_entities
                .in_set(ClientSet::Reconcile)
                .after(crate::reconcile_prediction),
        );
}
