use bevy::prelude::*;
use frame::{FxSoundPublished, MatchTornDown, SessionSwapApplied};
use fx::{FxGapCause, FxMsec, set_presentation_clock};
use net::{
    ClientSet, FrameClock, GameActive, LastAdoptedSnapshot, advance_cg_frame_clock,
    reconcile_prediction,
};
use render_anim::sync_camera_from_presented;
use render_scene::{FlyCamera, WorldPresentFacts};

use crate::{
    EntityMarks, FxCameraOrigin, FxJournalCursor, HostFxDlights, HostFxPostLights, HostFxSystem,
    PreparedFxCatalog, PreparedFxModels, PreparedImpactFx, PreparedTracers, PresentedVehicleFx,
    TracerDrawGate, TracerWorld,
};

pub fn register_fx_orchestration(app: &mut App) {
    app.add_systems(
        Update,
        (
            kill_fx_on_match_torn_down
                .in_set(ClientSet::Load)
                .after(SessionSwapApplied),
            latch_cgame_active
                .in_set(ClientSet::Reconcile)
                .after(reconcile_prediction)
                .before(advance_cg_frame_clock),
            stamp_presentation_clock
                .in_set(ClientSet::Reconcile)
                .after(advance_cg_frame_clock),
        ),
    )
    .add_systems(
        Update,
        clear_presented_vehicle_fx.in_set(ClientSet::Receive),
    )
    .add_systems(
        Update,
        stamp_fx_camera_origin
            .after(sync_camera_from_presented)
            .in_set(ClientSet::Present),
    )
    .add_systems(
        Update,
        play_pending_fx_sounds
            .before(FxSoundPublished)
            .in_set(ClientSet::Effects),
    );
}

pub fn stamp_fx_camera_origin(
    cameras: Query<&Transform, With<FlyCamera>>,
    mut origin: ResMut<FxCameraOrigin>,
) {
    origin.0 = cameras
        .iter()
        .next()
        .map(|transform| transform.translation.to_array())
        .unwrap_or([0.0; 3]);
}

fn kill_fx_on_match_torn_down(
    mut torn: MessageReader<MatchTornDown>,
    mut host: ResMut<HostFxSystem>,
    mut cursor: ResMut<FxJournalCursor>,
    mut dlights: ResMut<HostFxDlights>,
    mut post_lights: ResMut<HostFxPostLights>,
    mut tracers: ResMut<TracerWorld>,
    mut tracer_gate: ResMut<TracerDrawGate>,
    mut entity_marks: ResMut<EntityMarks>,
    mut commands: Commands,
) {
    if torn.read().count() == 0 {
        return;
    }
    *host = HostFxSystem::default();
    cursor.createfx_booted = false;
    cursor.createfx_boot_msec = None;
    *dlights = HostFxDlights::default();
    *post_lights = HostFxPostLights::default();
    *tracers = TracerWorld::default();
    *tracer_gate = TracerDrawGate::default();
    *entity_marks = EntityMarks::default();
    commands.remove_resource::<PreparedFxCatalog>();
    commands.remove_resource::<PreparedFxModels>();
    commands.remove_resource::<crate::PreparedFxModelGeometry>();
    commands.insert_resource(crate::FxModelDrawPlan::default());
    commands.insert_resource(crate::FxModelStaging::default());
    commands.remove_resource::<PreparedImpactFx>();
    commands.remove_resource::<PreparedTracers>();
}

fn latch_cgame_active(
    facts: Res<WorldPresentFacts>,
    adopted: Option<Res<LastAdoptedSnapshot>>,
    mut active: ResMut<GameActive>,
) {
    let spawned = facts.spawned;
    let has_snap = adopted.as_ref().is_some_and(|snap| snap.next().is_some());
    active.0 = GameActive::from_first_snapshot(spawned, has_snap);
}

fn stamp_presentation_clock(mut host: ResMut<HostFxSystem>, clock: Res<FrameClock>) {
    let msec = FxMsec(clock.time());
    set_presentation_clock(&mut host.0, msec);
}

fn play_pending_fx_sounds(
    mut host: ResMut<HostFxSystem>,
    catalog: Option<Res<PreparedFxCatalog>>,
    bank: Option<Res<audio::SoundBank>>,
    clock: Res<FrameClock>,
    mut output: MessageWriter<audio::AliasCommand>,
) {
    let Some(catalog) = catalog else {
        return;
    };
    let Some(bank) = bank else {
        return;
    };
    let batch = std::mem::take(&mut host.0.pending_sounds);
    for req in batch {
        if req.msec_begin < clock.old_time() {
            continue;
        }
        match crate::present::catalog_lookup(&catalog.0, req.catalog_index)
            .and_then(|parent| parent.elems.get(req.def_index as usize))
            .map(|elem| elem.sound_in_bank(req.random_seed, &bank.0))
        {
            None | Some(asset_game::FxBankSound::Gap) => {
                host.0.gaps.raise(FxGapCause::ElemSoundSpawnSkipped {
                    def_index: req.def_index,
                });
            }
            Some(asset_game::FxBankSound::Silent) => {}
            Some(asset_game::FxBankSound::Play { namespace, alias }) => {
                output.write(audio::AliasCommand::Play(audio::PlayAlias {
                    event: None,
                    namespace,
                    alias: alias.to_owned(),
                    fallback: None,
                    origin_inches: Some(req.origin),
                    snd_ent: Some(fx_iw4::FX_ENTITYNUM_WORLD),
                }));
            }
        }
    }
}

fn clear_presented_vehicle_fx(mut presented: ResMut<PresentedVehicleFx>) {
    presented.by_id.clear();
}
