use core::ops::{Deref, DerefMut};

use bevy_ecs::prelude::{Component, Entity, Resource, World};
use playerstate_iw4::PlayerState;

use crate::gentity::{EntityRunKind, ScriptMoverGentity};
use crate::identities::ScriptModelId;
use crate::item::DroppedItem;
use crate::snapshot::Snapshot;
use crate::world::{ClientId, SimState, Tick, blank_player_state};
use crate::{ClientLifecycle, ProjectileState};

#[derive(Component, Clone, Copy, Debug)]
struct PlayerRow {
    client: ClientId,
    state: PlayerState,
}

#[derive(Component, Clone, Copy, Debug)]
struct ProjectileRow(ProjectileState);

#[derive(Component, Clone, Copy, Debug)]
struct ScriptMoverRow(ScriptMoverGentity);

#[derive(Component, Clone, Copy, Debug)]
struct DroppedItemRow(DroppedItem);

#[derive(Component, Debug, Default)]
pub(crate) struct PayloadIndex {
    by_number: Vec<Option<Entity>>,
    by_client: Vec<(ClientId, Entity)>,
}

#[derive(Resource, Clone, Copy)]
struct SimStateEntity(Entity);

pub(crate) fn install_state_entity(world: &mut World, entity: Entity) {
    world.insert_resource(SimStateEntity(entity));
}

pub(crate) struct FrameWorld<'w> {
    ecs: &'w mut World,
    state_entity: Entity,
}

impl<'w> FrameWorld<'w> {
    pub(crate) fn from_world(ecs: &'w mut World) -> Self {
        let state_entity = state_entity(ecs);
        Self { ecs, state_entity }
    }

    pub(crate) fn new(ecs: &'w mut World, state_entity: Entity) -> Self {
        Self { ecs, state_entity }
    }
}

impl Deref for FrameWorld<'_> {
    type Target = SimState;

    fn deref(&self) -> &Self::Target {
        self.ecs
            .get::<SimState>(self.state_entity)
            .expect("simulation state entity is missing its SimState component")
    }
}

impl DerefMut for FrameWorld<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ecs
            .get_mut::<SimState>(self.state_entity)
            .expect("simulation state entity is missing its SimState component")
            .into_inner()
    }
}

impl FrameWorld<'_> {
    pub(crate) fn missile_collision_models(&self, id: crate::ProjectileId) -> Vec<ScriptModelId> {
        self.ecs
            .get_resource::<crate::script::Runtime>()
            .map(|runtime| runtime.missile_collision_models(id))
            .unwrap_or_default()
    }

    pub(crate) fn emit_script_entity_trace(&self) {
        if perf::enabled()
            && let Some(runtime) = self.ecs.get_resource::<crate::script::Runtime>()
        {
            crate::script::host::trace::entities(runtime, self.entity_collision_capabilities());
        }
    }

    pub(crate) fn ecs(&mut self) -> &mut World {
        self.ecs
    }

    pub(crate) fn projectile_by_number(&self, entnum: i32) -> Option<ProjectileState> {
        let entity = entity_by_number(self.ecs, entnum)?;
        Some(self.ecs.get::<ProjectileRow>(entity)?.0)
    }

    pub(crate) fn projectile_mut_by_number(&mut self, entnum: i32) -> Option<&mut ProjectileState> {
        let entity = entity_by_number(self.ecs, entnum)?;
        Some(&mut self.ecs.get_mut::<ProjectileRow>(entity)?.into_inner().0)
    }

    pub(crate) fn remove_projectile_by_number(&mut self, entnum: i32) -> Option<ProjectileState> {
        let entity = entity_by_number(self.ecs, entnum)?;
        let projectile = self.ecs.get::<ProjectileRow>(entity)?.0;
        unbind_number(self.ecs, entnum);
        assert!(
            self.ecs.despawn(entity),
            "projectile component vanished before removal"
        );
        Some(projectile)
    }

    pub(crate) fn push_projectile(&mut self, projectile: ProjectileState) {
        spawn_projectile(self.ecs, projectile);
    }

    pub(crate) fn script_mover_mut_by_number(
        &mut self,
        number: i32,
    ) -> Option<&mut ScriptMoverGentity> {
        let entity = entity_by_number(self.ecs, number)?;
        Some(&mut self.ecs.get_mut::<ScriptMoverRow>(entity)?.into_inner().0)
    }

    pub(crate) fn script_mover_by_number(&self, number: i32) -> Option<ScriptMoverGentity> {
        let entity = entity_by_number(self.ecs, number)?;
        Some(self.ecs.get::<ScriptMoverRow>(entity)?.0)
    }

    pub(crate) fn remove_script_mover_by_number(&mut self, number: i32) -> bool {
        let Some(entity) = entity_by_number(self.ecs, number) else {
            return false;
        };
        if self.ecs.get::<ScriptMoverRow>(entity).is_none() {
            return false;
        }
        unbind_number(self.ecs, number);
        assert!(
            self.ecs.despawn(entity),
            "script mover vanished before removal"
        );
        self.free_dynamic_entity_number(number);
        true
    }

    pub(crate) fn visit_script_movers(&self, visit: impl FnMut(&ScriptMoverGentity)) {
        visit_script_movers(self.ecs, visit);
    }

    pub(crate) fn visit_projectiles(&self, visit: impl FnMut(&ProjectileState)) {
        visit_projectiles(self.ecs, visit);
    }

    pub fn spawn_script_mover(
        &mut self,
        id: ScriptModelId,
        origin: [f32; 3],
        angles: [f32; 3],
    ) -> Result<i32, crate::EntityAllocError> {
        spawn_script_mover(self, id, origin, angles)
    }

    pub fn gentity_number(&self, id: ScriptModelId) -> Option<i32> {
        gentity_number(self.ecs, id)
    }

    pub fn begin_script_mover_rotate_velocity(
        &mut self,
        id: ScriptModelId,
        speed: [f32; 3],
        total_time_seconds: f32,
        level_time_ms: i32,
    ) -> bool {
        begin_script_mover_rotate_velocity(self, id, speed, total_time_seconds, level_time_ms)
    }

    pub fn begin_script_movers_rotate_velocity_supplied(
        &mut self,
        speed: f32,
        level_time_ms: i32,
    ) -> usize {
        begin_script_movers_rotate_velocity_supplied(self, speed, level_time_ms)
    }

    pub fn set_script_mover_r_box(
        &mut self,
        number: i32,
        box_mid: [f32; 3],
        box_half: [f32; 3],
    ) -> bool {
        let Some(mover) = self.script_mover_mut_by_number(number) else {
            return false;
        };
        mover.box_mid = box_mid;
        mover.box_half = box_half;
        true
    }

    pub fn set_script_mover_origin(&mut self, number: i32, origin: [f32; 3]) -> bool {
        let Some(mover) = self.script_mover_mut_by_number(number) else {
            return false;
        };
        mover.state.tr_type = entity_iw4::TR_STATIONARY;
        mover.state.tr_delta = [0.0; 3];
        mover.state.tr_base = origin;
        true
    }

    // Clients evaluate the trajectory between snapshots; a bare base write draws as a 20 Hz step.
    pub fn set_script_mover_pose(
        &mut self,
        number: i32,
        time_ms: i32,
        origin: [f32; 3],
        angles: [f32; 3],
    ) -> bool {
        let Some(mover) = self.script_mover_mut_by_number(number) else {
            return false;
        };
        let state = &mut mover.state;
        let per_sec = 1000.0 / crate::MATCH_TICK_MS as f32;
        let consecutive = time_ms - crate::MATCH_TICK_MS as i32;
        let delta: [f32; 3] = if state.tr_time == consecutive {
            core::array::from_fn(|i| (origin[i] - state.tr_base[i]) * per_sec)
        } else {
            [0.0; 3]
        };
        let apos_delta: [f32; 3] = if state.apos_tr_time == consecutive {
            core::array::from_fn(|i| {
                math_iw4::angle_subtract(angles[i], state.apos_tr_base[i]) * per_sec
            })
        } else {
            [0.0; 3]
        };
        state.tr_type = if delta == [0.0; 3] {
            entity_iw4::TR_STATIONARY
        } else {
            entity_iw4::TR_LINEAR
        };
        state.tr_time = time_ms;
        state.tr_duration = 0;
        state.tr_base = origin;
        state.tr_delta = delta;
        state.apos_tr_type = if apos_delta == [0.0; 3] {
            entity_iw4::TR_STATIONARY
        } else {
            entity_iw4::TR_LINEAR
        };
        state.apos_tr_time = time_ms;
        state.apos_tr_duration = 0;
        state.apos_tr_base = angles;
        state.apos_tr_delta = apos_delta;
        true
    }

    pub(crate) fn dropped_item_count(&self) -> usize {
        dropped_item_row_count(self.ecs)
    }

    pub(crate) fn dropped_item_numbers_sorted(&self) -> Vec<i32> {
        dropped_item_numbers_sorted(self.ecs)
    }

    pub(crate) fn push_dropped_item(&mut self, item: DroppedItem) {
        spawn_dropped_item(self.ecs, item);
    }

    pub(crate) fn remove_dropped_item_by_number(&mut self, number: i32) -> Option<DroppedItem> {
        let entity = entity_by_number(self.ecs, number)?;
        let item = self.ecs.get::<DroppedItemRow>(entity)?.0;
        unbind_number(self.ecs, number);
        assert!(
            self.ecs.despawn(entity),
            "dropped item component vanished before removal"
        );
        Some(item)
    }

    pub(crate) fn dropped_item_mut_by_number(&mut self, number: i32) -> Option<&mut DroppedItem> {
        let entity = entity_by_number(self.ecs, number)?;
        Some(&mut self.ecs.get_mut::<DroppedItemRow>(entity)?.into_inner().0)
    }

    pub(crate) fn dropped_item_by_number(&self, number: i32) -> Option<DroppedItem> {
        dropped_item_by_number(self.ecs, number)
    }

    pub(crate) fn ensure_player(&mut self, id: ClientId) -> &mut PlayerState {
        {
            let _ = self.client_meta_mut(id);
        }
        if player_entity(self.ecs, id).is_none() {
            let mut ps = blank_player_state();

            ps.corpse_index = -1;
            spawn_player_row(self.ecs, id, ps);
        }
        self.player_mut(id)
            .expect("ensure_player just inserted the row")
    }

    pub fn player(&self, id: ClientId) -> Option<&PlayerState> {
        player_ref(self.ecs, id)
    }

    pub(crate) fn visit_players(&self, visit: impl FnMut(ClientId, &PlayerState)) {
        visit_players(self.ecs, visit);
    }

    pub(crate) fn retire_client(&mut self, id: ClientId) {
        crate::script::disconnect_player(self.ecs, id.0);
        self.deref_mut().forget_client_membership(id);
        let Some(entity) = player_entity(self.ecs, id) else {
            return;
        };
        unbind_client(self.ecs, id);
        assert!(
            self.ecs.despawn(entity),
            "player payload vanished before retire"
        );
    }

    pub(crate) fn player_mut(&mut self, id: ClientId) -> Option<&mut PlayerState> {
        let entity = player_entity(self.ecs, id)?;
        Some(&mut self.ecs.get_mut::<PlayerRow>(entity)?.into_inner().state)
    }

    pub(crate) fn link_player_area(
        &mut self,
        id: ClientId,
        bounds: movement_iw4::MoveBounds,
    ) -> bool {
        let Some(origin) = self.player(id).map(|ps| ps.origin) else {
            return false;
        };
        self.deref_mut()
            .link_player_area(id, origin, bounds.mins, bounds.maxs)
    }

    pub(crate) fn link_player_standing_area(&mut self, id: ClientId) -> bool {
        let Some(origin) = self.player(id).map(|ps| ps.origin) else {
            return false;
        };
        self.deref_mut().link_player_area(
            id,
            origin,
            crate::bullet_collision::PLAYER_MINS,
            crate::bullet_collision::PLAYER_MAXS,
        )
    }

    pub(crate) fn unlink_player_area(&mut self, id: ClientId) -> bool {
        self.deref_mut().unlink_player_area(id)
    }

    pub(crate) fn for_each_player_mut(
        &mut self,
        mut visit: impl FnMut(ClientId, &mut PlayerState),
    ) {
        let entities = player_entities(self.ecs);
        for entity in entities {
            let row = self
                .ecs
                .get_mut::<PlayerRow>(entity)
                .expect("indexed player component vanished")
                .into_inner();
            visit(row.client, &mut row.state);
        }
    }

    pub fn set_origin(&mut self, id: ClientId, origin: [f32; 3]) -> bool {
        let old_origin = {
            let Some(ps) = self.player_mut(id) else {
                return false;
            };
            let old_origin = ps.origin;
            ps.origin = origin;
            ps.velocity = [0.0, 0.0, 0.0];
            old_origin
        };
        self.translate_player_area(
            id,
            [
                origin[0] - old_origin[0],
                origin[1] - old_origin[1],
                origin[2] - old_origin[2],
            ],
        );
        true
    }

    /// Moves a player as a spawn or teleport does: stopped, and marked so
    /// prediction takes the new place instead of smoothing to it.
    pub fn teleport(&mut self, id: ClientId, origin: [f32; 3]) -> bool {
        if !self.set_origin(id, origin) {
            return false;
        }
        if let Some(ps) = self.player_mut(id) {
            ps.e_flags ^= playerstate_iw4::eflags::TELEPORT;
        }
        true
    }

    pub fn set_legs_anim(&mut self, id: ClientId, legs_anim: i32) -> bool {
        let Some(ps) = self.player_mut(id) else {
            return false;
        };
        ps.legs_anim = legs_anim;
        true
    }

    pub fn set_viewangles(&mut self, id: ClientId, viewangles: [f32; 3]) -> bool {
        let Some(ps) = self.player_mut(id) else {
            return false;
        };
        ps.viewangles = viewangles;
        true
    }

    pub fn set_e_flags(&mut self, id: ClientId, e_flags: u32) -> bool {
        let Some(ps) = self.player_mut(id) else {
            return false;
        };
        ps.e_flags = e_flags;
        true
    }

    pub fn set_leanf(&mut self, id: ClientId, leanf: f32) -> bool {
        let Some(ps) = self.player_mut(id) else {
            return false;
        };
        ps.leanf = leanf;
        true
    }

    pub fn set_view_height_target(&mut self, id: ClientId, view_height_target: i32) -> bool {
        let Some(ps) = self.player_mut(id) else {
            return false;
        };
        ps.view_height_target = view_height_target;
        true
    }

    pub fn gate_nudge_origin(&mut self, id: ClientId, delta: [f32; 3]) {
        if let Some(ps) = self.player_mut(id) {
            ps.origin[0] += delta[0];
            ps.origin[1] += delta[1];
            ps.origin[2] += delta[2];
            self.translate_player_area(id, delta);
        }
    }

    pub(crate) fn alive_collision_poses(
        &self,
    ) -> Vec<crate::bullet_collision::PlayerCollisionPose> {
        SimState::alive_collision_poses(self, |id| player_ref(self.ecs, id).copied())
    }

    /// `sensor_trace` as of now: players moved, spawned or killed earlier in
    /// this tick are traced where they are, not where the last record left them.
    pub(crate) fn current_sensor_trace(
        &self,
        query: crate::bullet_collision::BulletTraceQuery,
    ) -> crate::bullet_collision::TraceOutcome {
        let moved = self.player_poses_since_record(|id| player_ref(self.ecs, id).copied());
        self.bullet_trace(query, moved.as_deref())
    }

    pub(crate) fn record_collision_history(
        &mut self,
        tick: Tick,
        msec: i32,
        hitbox_cmds: Option<&[ClientId]>,
    ) {
        let ids = self.client_ids_sorted();
        let copies: Vec<(ClientId, PlayerState)> = ids
            .into_iter()
            .filter_map(|id| player_ref(self.ecs, id).copied().map(|ps| (id, ps)))
            .collect();
        SimState::record_collision_history(
            self,
            tick,
            msec,
            |id| {
                copies
                    .iter()
                    .find(|(client, _)| *client == id)
                    .map(|(_, ps)| *ps)
            },
            hitbox_cmds,
        );
    }

    pub fn lagcomp_query_for(
        &self,
        attacker: ClientId,
        current: Tick,
    ) -> crate::bullet_collision::LagcompQuery {
        SimState::lagcomp_query_for(self, attacker, current, |id| {
            player_ref(self.ecs, id).copied()
        })
    }

    pub(crate) fn alive_origins(&self) -> Vec<[f32; 3]> {
        self.client_ids_sorted()
            .into_iter()
            .filter(|id| {
                self.client_meta(*id)
                    .is_some_and(|meta| meta.lifecycle == ClientLifecycle::Alive)
            })
            .filter_map(|id| self.player(id).map(|ps| ps.origin))
            .collect()
    }

    pub fn snapshot(&self, tick: Tick) -> Snapshot {
        self.snapshot_with_dynamic_rows(
            tick,
            collect_players(self.ecs),
            collect_projectiles(self.ecs),
            collect_script_movers(self.ecs),
            collect_dropped_items(self.ecs),
        )
    }

    pub fn adopt_snapshot(&mut self, snapshot: &Snapshot) -> crate::AdoptReport {
        let (report, projectiles, movers, items) = self.adopt_snapshot_state(snapshot);
        adopt_payload_rows(self.ecs, &snapshot.players, projectiles, movers, items);
        report
    }

    pub fn adopt_prediction_snapshot(
        &mut self,
        snapshot: &Snapshot,
        local: ClientId,
    ) -> crate::AdoptReport {
        let (report, projectiles, movers, items) =
            self.adopt_prediction_snapshot_state(snapshot, local);
        let players = snapshot
            .players
            .iter()
            .find(|(id, _)| *id == local)
            .map(std::slice::from_ref)
            .unwrap_or_default();
        adopt_payload_rows(self.ecs, players, projectiles, movers, items);
        report
    }

    pub fn shutdown_game(&mut self) {
        *self.deref_mut() = SimState::new();
        clear_payload_rows(self.ecs);
    }
}

pub(crate) fn state_entity(world: &World) -> Entity {
    world.resource::<SimStateEntity>().0
}

pub(crate) fn collect_players(world: &World) -> Vec<(ClientId, PlayerState)> {
    let mut rows = Vec::new();
    for id in player_ids_sorted(world) {
        if let Some(state) = player_ref(world, id) {
            rows.push((id, *state));
        }
    }
    rows
}

pub(crate) fn collect_projectiles(world: &World) -> Vec<ProjectileState> {
    let mut rows = Vec::new();
    for entnum in projectile_numbers_sorted(world) {
        if let Some(projectile) = projectile_ref(world, entnum) {
            rows.push(*projectile);
        }
    }
    rows
}

pub(crate) fn collect_script_movers(world: &World) -> Vec<ScriptMoverGentity> {
    let mut rows = Vec::new();
    for number in script_mover_numbers_sorted(world) {
        if let Some(mover) = script_mover_by_number(world, number) {
            rows.push(mover);
        }
    }
    rows
}

pub(crate) fn collect_dropped_items(world: &World) -> Vec<DroppedItem> {
    let mut rows = Vec::new();
    for number in dropped_item_numbers_sorted(world) {
        if let Some(item) = dropped_item_by_number(world, number) {
            rows.push(item);
        }
    }
    rows
}

pub(crate) fn visit_players(world: &World, mut visit: impl FnMut(ClientId, &PlayerState)) {
    for id in player_ids_sorted(world) {
        if let Some(state) = player_ref(world, id) {
            visit(id, state);
        }
    }
}

pub(crate) fn visit_projectiles(world: &World, mut visit: impl FnMut(&ProjectileState)) {
    for entnum in projectile_numbers_sorted(world) {
        if let Some(projectile) = projectile_ref(world, entnum) {
            visit(projectile);
        }
    }
}

pub(crate) fn visit_script_movers(world: &World, mut visit: impl FnMut(&ScriptMoverGentity)) {
    for number in script_mover_numbers_sorted(world) {
        if let Some(mover) = script_mover_by_number(world, number) {
            visit(&mover);
        }
    }
}

pub(crate) fn player_row_count(world: &World) -> usize {
    payload_index(world).by_client.len()
}

pub(crate) fn script_mover_row_count(world: &World) -> usize {
    script_mover_numbers_sorted(world).len()
}

fn dropped_item_row_count(world: &World) -> usize {
    dropped_item_numbers_sorted(world).len()
}

pub(crate) fn script_mover_by_number(world: &World, number: i32) -> Option<ScriptMoverGentity> {
    let entity = entity_by_number(world, number)?;
    world.get::<ScriptMoverRow>(entity).map(|row| row.0)
}

pub(crate) fn script_mover_by_id(world: &World, id: ScriptModelId) -> Option<ScriptMoverGentity> {
    for number in script_mover_numbers_sorted(world) {
        if let Some(mover) = script_mover_by_number(world, number) {
            if mover.id == id {
                return Some(mover);
            }
        }
    }
    None
}

pub(crate) fn spawn_payload_rows(
    world: &mut World,
    players: Vec<(ClientId, PlayerState)>,
    projectiles: Vec<ProjectileState>,
    movers: Vec<ScriptMoverGentity>,
    items: Vec<DroppedItem>,
) {
    for (client, state) in players {
        spawn_player_row(world, client, state);
    }
    for projectile in projectiles {
        spawn_projectile(world, projectile);
    }
    for mover in movers {
        spawn_script_mover_row(world, mover);
    }
    for item in items {
        spawn_dropped_item(world, item);
    }
}

pub(crate) fn clear_payload_rows(world: &mut World) {
    let entities: Vec<Entity> = {
        let index = payload_index(world);
        index
            .by_number
            .iter()
            .copied()
            .flatten()
            .chain(index.by_client.iter().map(|(_, entity)| *entity))
            .collect()
    };
    for entity in entities {
        assert!(
            world.despawn(entity),
            "payload component vanished before clear"
        );
    }
    *payload_index_mut(world) = PayloadIndex::default();
}

fn adopt_payload_rows(
    world: &mut World,
    players: &[(ClientId, PlayerState)],
    projectiles: Vec<ProjectileState>,
    movers: Vec<ScriptMoverGentity>,
    items: Vec<DroppedItem>,
) {
    adopt_player_rows(world, players);
    let mut wanted_numbers = Vec::with_capacity(projectiles.len() + movers.len() + items.len());
    wanted_numbers.extend(projectiles.iter().map(|projectile| projectile.entnum));
    wanted_numbers.extend(movers.iter().map(|mover| mover.state.number));
    wanted_numbers.extend(items.iter().map(|item| item.state.number));
    despawn_unwanted_numbered(world, &wanted_numbers);
    for projectile in projectiles {
        upsert_projectile(world, projectile);
    }
    for mover in movers {
        upsert_script_mover(world, mover);
    }
    for item in items {
        upsert_dropped_item(world, item);
    }
}

fn adopt_player_rows(world: &mut World, players: &[(ClientId, PlayerState)]) {
    let stale: Vec<(ClientId, Entity)> = payload_index(world)
        .by_client
        .iter()
        .copied()
        .filter(|(id, _)| !players.iter().any(|(want, _)| want == id))
        .collect();
    for (id, entity) in stale {
        unbind_client(world, id);
        assert!(
            world.despawn(entity),
            "player payload vanished before adopt despawn"
        );
    }
    for (id, ps) in players {
        if let Some(entity) = player_entity(world, *id) {
            if let Some(mut row) = world.get_mut::<PlayerRow>(entity) {
                row.state = *ps;
                continue;
            }
            unbind_client(world, *id);
            assert!(
                world.despawn(entity),
                "player payload vanished before adopt respawn"
            );
        }
        spawn_player_row(world, *id, *ps);
    }
}

fn despawn_unwanted_numbered(world: &mut World, wanted: &[i32]) {
    let stale: Vec<(i32, Entity)> = payload_index(world)
        .by_number
        .iter()
        .enumerate()
        .filter_map(|(n, slot)| {
            let entity = (*slot)?;
            let number = i32::try_from(n).ok()?;
            if wanted.contains(&number) {
                None
            } else {
                Some((number, entity))
            }
        })
        .collect();
    for (number, entity) in stale {
        unbind_number(world, number);
        assert!(
            world.despawn(entity),
            "numbered payload vanished before adopt despawn"
        );
    }
}

fn upsert_projectile(world: &mut World, projectile: ProjectileState) {
    if let Some(entity) = entity_by_number(world, projectile.entnum) {
        if let Some(mut row) = world.get_mut::<ProjectileRow>(entity) {
            row.0 = projectile;
            return;
        }
        unbind_number(world, projectile.entnum);
        assert!(
            world.despawn(entity),
            "projectile payload vanished before adopt respawn"
        );
    }
    spawn_projectile(world, projectile);
}

fn upsert_script_mover(world: &mut World, mover: ScriptMoverGentity) {
    if let Some(entity) = entity_by_number(world, mover.state.number) {
        if let Some(mut row) = world.get_mut::<ScriptMoverRow>(entity) {
            row.0 = mover;
            return;
        }
        unbind_number(world, mover.state.number);
        assert!(
            world.despawn(entity),
            "script mover payload vanished before adopt respawn"
        );
    }
    spawn_script_mover_row(world, mover);
}

fn upsert_dropped_item(world: &mut World, item: DroppedItem) {
    if let Some(entity) = entity_by_number(world, item.state.number) {
        if let Some(mut row) = world.get_mut::<DroppedItemRow>(entity) {
            row.0 = item;
            return;
        }
        unbind_number(world, item.state.number);
        assert!(
            world.despawn(entity),
            "dropped item payload vanished before adopt respawn"
        );
    }
    spawn_dropped_item(world, item);
}

fn occupancy(world: &World, number: i32) -> Option<EntityRunKind> {
    kernel(world).occupied_kind(number)
}

fn kernel(world: &World) -> &crate::gentity::EntityKernel {
    world
        .get::<SimState>(state_entity(world))
        .expect("simulation ECS must contain exactly one SimState component")
        .entity_kernel()
}

fn payload_index(world: &World) -> &PayloadIndex {
    world
        .get::<PayloadIndex>(state_entity(world))
        .expect("simulation ECS must contain a PayloadIndex beside SimState")
}

fn payload_index_mut(world: &mut World) -> &mut PayloadIndex {
    let entity = state_entity(world);
    world
        .get_mut::<PayloadIndex>(entity)
        .expect("simulation ECS must contain a PayloadIndex beside SimState")
        .into_inner()
}

fn bind_number(world: &mut World, number: i32, payload: Entity) {
    let index = payload_index_mut(world);
    let n = usize::try_from(number).unwrap_or(0);
    if index.by_number.len() <= n {
        index.by_number.resize(n + 1, None);
    }
    index.by_number[n] = Some(payload);
}

fn unbind_number(world: &mut World, number: i32) {
    if let Ok(n) = usize::try_from(number) {
        if let Some(slot) = payload_index_mut(world).by_number.get_mut(n) {
            *slot = None;
        }
    }
}

fn entity_by_number(world: &World, number: i32) -> Option<Entity> {
    payload_index(world)
        .by_number
        .get(usize::try_from(number).ok()?)
        .copied()
        .flatten()
}

fn bind_client(world: &mut World, id: ClientId, payload: Entity) {
    let index = payload_index_mut(world);
    if let Some((_, existing)) = index.by_client.iter_mut().find(|(client, _)| *client == id) {
        *existing = payload;
        return;
    }
    index.by_client.push((id, payload));
    index.by_client.sort_by_key(|(client, _)| client.0);
}

fn unbind_client(world: &mut World, id: ClientId) {
    payload_index_mut(world)
        .by_client
        .retain(|(client, _)| *client != id);
}

fn spawn_player_row(world: &mut World, client: ClientId, state: PlayerState) {
    let payload = world.spawn(PlayerRow { client, state }).id();
    bind_client(world, client, payload);
}

fn spawn_projectile(world: &mut World, projectile: ProjectileState) {
    if occupancy(world, projectile.entnum) != Some(EntityRunKind::Missile) {
        panic!("in-flight projectile has no Missile think occupancy");
    }
    let entnum = projectile.entnum;
    let payload = world.spawn(ProjectileRow(projectile)).id();
    bind_number(world, entnum, payload);
}

fn projectile_ref(world: &World, entnum: i32) -> Option<&ProjectileState> {
    world
        .get::<ProjectileRow>(entity_by_number(world, entnum)?)
        .map(|row| &row.0)
}

fn player_ids_sorted(world: &World) -> Vec<ClientId> {
    payload_index(world)
        .by_client
        .iter()
        .map(|(id, _)| *id)
        .collect()
}

fn projectile_numbers_sorted(world: &World) -> Vec<i32> {
    kernel(world).occupied_numbers(|kind| kind == EntityRunKind::Missile)
}

fn script_mover_numbers_sorted(world: &World) -> Vec<i32> {
    kernel(world).occupied_numbers(|kind| {
        matches!(
            kind,
            EntityRunKind::ScriptMover | EntityRunKind::PrimaryLight
        )
    })
}

fn dropped_item_numbers_sorted(world: &World) -> Vec<i32> {
    kernel(world)
        .occupied_numbers(|kind| kind == EntityRunKind::Item)
        .into_iter()
        .filter(|&number| dropped_item_by_number(world, number).is_some())
        .collect()
}

fn spawn_dropped_item(world: &mut World, item: DroppedItem) {
    if occupancy(world, item.state.number) != Some(EntityRunKind::Item) {
        panic!("dropped ET_ITEM has no Item think occupancy");
    }
    let number = item.state.number;
    let payload = world.spawn(DroppedItemRow(item)).id();
    bind_number(world, number, payload);
}

fn spawn_script_mover_row(world: &mut World, mover: ScriptMoverGentity) {
    if occupancy(world, mover.state.number) != Some(EntityRunKind::ScriptMover) {
        panic!("script mover has no ScriptMover think occupancy");
    }
    let number = mover.state.number;
    let payload = world.spawn(ScriptMoverRow(mover)).id();
    bind_number(world, number, payload);
}

fn dropped_item_by_number(world: &World, number: i32) -> Option<DroppedItem> {
    world
        .get::<DroppedItemRow>(entity_by_number(world, number)?)
        .map(|row| row.0)
}

fn player_entities(world: &World) -> Vec<Entity> {
    payload_index(world)
        .by_client
        .iter()
        .map(|(_, entity)| *entity)
        .collect()
}

fn player_entity(world: &World, id: ClientId) -> Option<Entity> {
    payload_index(world)
        .by_client
        .iter()
        .find(|(client, _)| *client == id)
        .map(|(_, entity)| *entity)
}

pub(crate) fn player_ref(world: &World, id: ClientId) -> Option<&PlayerState> {
    world
        .get::<PlayerRow>(player_entity(world, id)?)
        .map(|row| &row.state)
}

fn gentity_number(world: &World, id: ScriptModelId) -> Option<i32> {
    script_mover_by_id(world, id).map(|mover| mover.state.number)
}

fn spawn_script_mover(
    world: &mut FrameWorld<'_>,
    id: ScriptModelId,
    origin: [f32; 3],
    angles: [f32; 3],
) -> Result<i32, crate::gentity::EntityAllocError> {
    if let Some(number) = world.gentity_number(id) {
        return Ok(number);
    }
    let entity = world.allocate_dynamic_entity(crate::gentity::EntityRunKind::ScriptMover)?;
    let number = entity.number();
    let state = crate::gentity::init_script_mover_state(number, origin, angles);
    spawn_script_mover_row(
        world.ecs,
        ScriptMoverGentity {
            id,
            state,
            ..Default::default()
        },
    );
    Ok(number)
}

fn begin_script_mover_rotate_velocity(
    world: &mut FrameWorld<'_>,
    id: ScriptModelId,
    speed: [f32; 3],
    total_time_seconds: f32,
    level_time_ms: i32,
) -> bool {
    let Some(number) = world.gentity_number(id) else {
        return false;
    };
    let Some(mover) = world.script_mover_mut_by_number(number) else {
        return false;
    };
    crate::gentity::begin_script_mover_rotate_velocity(
        &mut mover.state,
        speed,
        total_time_seconds,
        level_time_ms,
    );
    true
}

fn begin_script_movers_rotate_velocity_supplied(
    world: &mut FrameWorld<'_>,
    speed: f32,
    level_time_ms: i32,
) -> usize {
    let mut jobs = Vec::new();
    world.visit_script_movers(|mover| {
        let right = gamemode_iw4::fan_blade_right(mover.state.apos_tr_base);
        let delta = gamemode_iw4::fan_blade_rotate_delta(right, speed);
        jobs.push((mover.id, delta));
    });
    let mut n = 0;
    for (id, delta) in jobs {
        if world.begin_script_mover_rotate_velocity(
            id,
            delta,
            gamemode_iw4::FAN_BLADE_ROTATE_TIME,
            level_time_ms,
        ) {
            n += 1;
        }
    }
    n
}
