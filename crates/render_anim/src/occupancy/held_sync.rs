use bevy::prelude::*;
use frame::{LaunchReport, LifeEnded, LifeFrontPublished, LifeStarted, ViewSubject};
use net::{ClientSet, LocalPresentClient, PresentedSnapshot};

use crate::anim::fpv_prepared::{FpvOwnerInputs, PreparedFpv};
use crate::gaps::{RenderGap, RenderGapCause, RenderPresentationGaps};
use crate::occupancy::fpv_present::{
    FpvHeldLife, FpvHeldSettled, FpvPlacementRoot, FpvState, FpvStatusGap, PendingFpvSpawn,
    PendingFpvSpawnRequest, SessionViewmodel,
};
use weapon_iw4::get_viewmodel_weapon_index;

pub fn sync_fpv_to_held_weapon(
    mut commands: Commands,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    view: Res<ViewSubject>,
    prepared: Res<PreparedFpv>,
    owners: FpvOwnerInputs,
    mut pending_fpv: ResMut<PendingFpvSpawn>,
    mut session_vm: ResMut<SessionViewmodel>,
    mut settled: ResMut<FpvHeldSettled>,
    mut held_life: ResMut<FpvHeldLife>,
    mut status: ResMut<FpvStatusGap>,
    gaps: Res<RenderPresentationGaps>,
    mut report: Option<ResMut<LaunchReport>>,
    existing_fpv: Query<Entity, With<FpvPlacementRoot>>,
    mut started: MessageReader<LifeStarted>,
    mut ended: MessageReader<LifeEnded>,
) {
    let clear_fpv = |commands: &mut Commands,
                     pending: &mut PendingFpvSpawn,
                     session: &mut SessionViewmodel,
                     settled: &mut FpvHeldSettled,
                     existing: &Query<Entity, With<FpvPlacementRoot>>| {
        for entity in existing {
            commands.entity(entity).try_despawn();
        }
        pending.0 = None;
        session.0 = None;
        settled.0 = None;
    };

    let mut ended_local = false;
    for ev in ended.read() {
        if ev.client == local.0.0 {
            ended_local = true;
        }
    }
    let mut started_local = false;
    for ev in started.read() {
        if ev.client == local.0.0 {
            started_local = true;
            held_life.0 = Some(ev.life);
        }
    }
    if ended_local && !started_local {
        if session_vm.0.is_some() || pending_fpv.0.is_some() || settled.0.is_some() {
            clear_fpv(
                &mut commands,
                &mut pending_fpv,
                &mut session_vm,
                &mut settled,
                &existing_fpv,
            );
            status.0 = Some(FpvState::ClearedNotAlive);
        }
        held_life.0 = None;
    }

    if presented
        .snapshot()
        .is_some_and(|s| s.meta.phase == sim::MatchPhase::Intermission)
        && !view.in_killcam()
    {
        if session_vm.0.is_some() || pending_fpv.0.is_some() || settled.0.is_some() {
            clear_fpv(
                &mut commands,
                &mut pending_fpv,
                &mut session_vm,
                &mut settled,
                &existing_fpv,
            );
            status.0 = Some(FpvState::ClearedNotAlive);
        }
        return;
    }

    let Some(ps) = presented.viewweapon_player(local.0) else {
        if session_vm.0.is_some() || pending_fpv.0.is_some() || settled.0.is_some() {
            clear_fpv(
                &mut commands,
                &mut pending_fpv,
                &mut session_vm,
                &mut settled,
                &existing_fpv,
            );
            status.0 = Some(FpvState::ClearedNotAlive);
        }
        held_life.0 = None;
        return;
    };
    if started_local {
        if session_vm.0.is_some() || pending_fpv.0.is_some() || settled.0.is_some() {
            clear_fpv(
                &mut commands,
                &mut pending_fpv,
                &mut session_vm,
                &mut settled,
                &existing_fpv,
            );
        }
    }
    let held = prepared
        .table()
        .map_or(get_viewmodel_weapon_index(ps), |t| {
            crate::occupancy::fpv_present::fpv_viewmodel_weapon(ps, t)
        });

    if held == 0 {
        if session_vm.0.is_some() || pending_fpv.0.is_some() || settled.0 != Some(0) {
            clear_fpv(
                &mut commands,
                &mut pending_fpv,
                &mut session_vm,
                &mut settled,
                &existing_fpv,
            );
            settled.0 = Some(0);
            let state = FpvState::ClearedNoWeapon;
            if let Some(report) = report.as_deref_mut() {
                report.sim_gap = state.label();
            }
            status.0 = Some(state);
        }
        return;
    }

    if pending_fpv
        .0
        .as_ref()
        .is_some_and(|req| req.weapon_id == held && req.parent_weapon == ps.weapon_primary)
    {
        return;
    }
    if session_vm.0.as_ref().is_some_and(|session| {
        session.weapon_id == held && session.parent_weapon == ps.weapon_primary
    }) {
        settled.0 = Some(held);
        return;
    }

    if settled.0 == Some(held) && session_vm.0.is_none() && pending_fpv.0.is_none() {
        return;
    }

    let Some(table) = prepared.table() else {
        let waiting = FpvState::Blocked(RenderGapCause::FpvCatalogMissing);
        if status.0.as_ref() != Some(&waiting) {
            status.0 = Some(waiting);
        }
        return;
    };
    let Some((weapon_handle, parent_handle)) =
        owners.handles(presented.weapon_epoch(), held, ps.weapon_primary)
    else {
        return;
    };
    if owners.bind(table).is_none() {
        return;
    }
    let state = match table.gun_index(held) {
        Some(gun_index) => {
            for entity in &existing_fpv {
                commands.entity(entity).try_despawn();
            }
            session_vm.0 = None;
            pending_fpv.0 = Some(PendingFpvSpawnRequest {
                weapon_handle,
                parent_handle,
                gun_index,
                catalog_id: table.catalog_id(),
                weapon_id: held,
                parent_weapon: ps.weapon_primary,
            });
            settled.0 = None;
            FpvState::Queued
        }
        None => {
            for entity in &existing_fpv {
                commands.entity(entity).try_despawn();
            }
            session_vm.0 = None;
            pending_fpv.0 = None;
            settled.0 = Some(held);
            FpvState::Blocked(RenderGapCause::FpvGunXModelUnresolved { weapon_id: held })
        }
    };

    if let Some(cause) = state.cause() {
        gaps.raise(cause.clone());
    } else {
        gaps.clear(RenderGap::FpvViewmodel);
    }
    diag::info!(Fpv, "fpv held sync: weapon={held} — {state}");
    if let Some(report) = report.as_deref_mut() {
        report.sim_gap = state.label();
    }
    status.0 = Some(state);
}

pub fn sync_fpv_status_to_probe(
    status: Res<FpvStatusGap>,
    mut report: Option<ResMut<LaunchReport>>,
) {
    if !status.is_changed() {
        return;
    }
    let Some(state) = status.0.as_ref() else {
        return;
    };
    if let Some(report) = report.as_deref_mut() {
        report.sim_gap = state.label();
    }
}

pub(crate) fn register_held_sync_systems(app: &mut App) {
    app.add_message::<LifeStarted>()
        .add_message::<LifeEnded>()
        .add_systems(Update, sync_fpv_to_held_weapon.in_set(LifeFrontPublished))
        .add_systems(Update, sync_fpv_status_to_probe.in_set(ClientSet::Diag));
}
