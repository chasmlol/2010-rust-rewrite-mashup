use bevy::prelude::*;
use frame::{MatchInstalled, MatchTornDown, SessionSwapApplied};
use net::ClientSet;

use crate::anim::fpv_prepared::PreparedFpv;
use crate::anim::scene_submission::AnimDObjSceneSkels;
use crate::occupancy::dyn_ent::{DynEntPhysClip, DynEntPhysWorld};
use crate::occupancy::fpv_present::{
    FpvHeldLife, FpvHeldSettled, FpvPlacementRoot, FpvPresentCursor, PendingFpvSpawn,
    SessionViewmodel,
};

pub(crate) fn reset_anim_for_match(
    mut commands: Commands,
    mut installed: MessageReader<MatchInstalled>,
    mut torn: MessageReader<MatchTornDown>,
    mut cursor: ResMut<FpvPresentCursor>,
    mut pending: ResMut<PendingFpvSpawn>,
    mut viewmodel: ResMut<SessionViewmodel>,
    mut prepared_fpv: ResMut<PreparedFpv>,
    mut item_compositions: ResMut<crate::occupancy::item::PreparedItemCompositions>,
    mut remote_kits: ResMut<crate::anim::remote_body::PreparedRemoteKits>,
    mut model_materials: ResMut<crate::anim::model_materials::PreparedModelMaterials>,
    mut settled: ResMut<FpvHeldSettled>,
    mut held_life: ResMut<FpvHeldLife>,
    mut scene_skels: ResMut<AnimDObjSceneSkels>,
    existing: Query<Entity, With<FpvPlacementRoot>>,
) {
    let installs = installed.read().count();
    let teardowns = torn.read().count();
    if installs == 0 && teardowns == 0 {
        return;
    }

    cursor.0.clear();
    pending.0 = None;
    *viewmodel = SessionViewmodel::default();
    prepared_fpv.clear();
    item_compositions.clear();
    remote_kits.clear();
    model_materials.clear();
    settled.0 = None;
    held_life.0 = None;
    *scene_skels = AnimDObjSceneSkels::default();

    for entity in &existing {
        commands.entity(entity).try_despawn();
    }
}

fn reset_dyn_ent_phys_on_teardown(mut torn: MessageReader<MatchTornDown>, mut commands: Commands) {
    if torn.read().count() == 0 {
        return;
    }
    commands.insert_resource(DynEntPhysClip::default());
    commands.insert_resource(DynEntPhysWorld::default());
}

pub(crate) fn register_match_reset_systems(app: &mut App) {
    app.add_systems(Update, reset_anim_for_match.in_set(ClientSet::Load))
        .add_systems(
            Update,
            reset_dyn_ent_phys_on_teardown
                .after(SessionSwapApplied)
                .in_set(ClientSet::Load),
        );
}
