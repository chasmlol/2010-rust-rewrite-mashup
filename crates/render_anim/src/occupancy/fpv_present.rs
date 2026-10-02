use std::sync::Arc;

use anim_iw4::{DOBJ_RADIUS_PARENT_ROOT, compute_bounds_radius};
use assets::PreparedFpvMeshes;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use frame::{LifeFrontPublished, PresentedPublished, ViewSubject, WorkerCmdSet};
use math_iw4::vec3_length;
use net::{ClientSet, FrameClock, LocalPresentClient, PresentedSnapshot, ViewweaponAim};
use render_scene::{SCENE_VIEWMODEL_ENTNUM, SCENE_VIEWMODEL_FX_FLAGS, SCENE_VIEWMODEL_LEFT_ENTNUM};

use crate::anim::fpv::{
    AuthorityFpvCues, EquippedFpv, FpvAuthoritySample, is_predicted_fire_weap_anim,
    local_shot_identity,
};
use crate::anim::fpv_host::{FpvGenerateArgs, FpvPoseKind, FpvPoseRefuse, generate_fpv_pose};
use crate::anim::fpv_prepared::{FpvWeaponSlot, FpvWeaponTable, FpvWeaponView, PreparedFpv};
use crate::anim::fpv_rig::PreparedFpvRig;
use crate::anim::scene_submission::{AnimDObjSceneSubmission, AnimSceneSubmit};
use crate::anim::viewmodel_controller::ViewmodelController;
use crate::gaps::{RenderGap, RenderGapCause, RenderPresentationGaps};
use crate::occupancy::remote_body::RemotePlayer;
use crate::occupancy::third_person::presented_is_third_person;
use crate::occupancy::view_kick::{
    GunOffset, PendingViewHurt, SessionViewKick, apply_cg_gun_offset_view,
    apply_viewweapon_land_view, iw_view_placement_to_bevy_camera_local,
    reset_view_kick_on_life_started, sync_camera_from_presented, tick_session_view_kick,
};
use crate::{fpv_dobj_skel_radii, viewmodel_lighting_origin};
use hud_iw4::{
    WeaponAdsOverlayFacts, calc_crosshair_position, get_weap_reticle_zoom, tan_half_fov,
    viewweapon_drawgun, viewweapon_drawgun_skip,
};
use math_iw4::angle_vectors;
use playerstate_iw4::PlayerState;
use render_material::RuntimeMaterialCatalog;
use render_scene::WorldScriptModelInstance;
use render_scene::{FlyCamera, FpvLens};
use render_scene::{HostGfxScene, scene_quat_from_viewmodel_axes};
use weapon_iw4::{
    GunKickSpring, GunRecoilPlacementState, PLACEMENT_ASSEMBLE_STEP_COUNT,
    StanceTransitionFadeGlobals, WeaponBobInputs, WeaponBobWaveformInputs,
    WeaponMovementKinematics, WeaponPlacementAssembleStep, WeaponPlacementPsInputs,
    WeaponPlacementState, WeaponStanceStaticOfsInputs, calculate_weapon_movement_bob_waveform,
    clip_table_key, dual_wield_view_model_origin_add, get_clip_for_hand,
    get_viewmodel_weapon_index, viewmodel_rocket_should_be_attached,
    viewweapon_iron_ads_saves_composed_axis, viewweapon_save_gun_pitch_yaw,
    viewweapon_view_to_world_delta, weapon_placement_assemble,
};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FpvPlacementSet;

/// The first-person vertices for this frame are written. The merge downstream
/// orders itself after this set.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FpvGeometrySet;

#[derive(Component)]
pub struct FpvViewmodel;

#[derive(Component)]
pub struct FpvPlacementRoot;

pub use crate::anim::fpv_host::{
    FpvBoltTargets, FpvHeldLife, FpvHeldSettled, FpvPoseProduct, FpvPresentCursor,
    PendingFpvNotetracks, PendingFpvSpawn, PendingFpvSpawnRequest,
};

#[derive(Resource, Default)]
pub struct SessionViewmodel(pub Option<SessionFpvMeshesHandles>);

pub struct SessionFpvMeshesHandles {
    pub weapon_id: u32,
    pub parent_weapon: u32,
    pub catalog_id: u64,
    pub axis: bool,
    pub view: Arc<FpvWeaponView>,
    pub fpv: EquippedFpv,
    pub(crate) active_rig: Option<Arc<PreparedFpvRig>>,
    pub(crate) material_catalog: Arc<RuntimeMaterialCatalog>,
}

fn same_compositions(a: &asset_game::FpvSideAssemblies, b: &asset_game::FpvSideAssemblies) -> bool {
    Arc::ptr_eq(&a.bare, &b.bare)
        && match (&a.rocket, &b.rocket) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
}

fn same_material_catalog(
    owned: &Arc<RuntimeMaterialCatalog>,
    current: Option<&render_scene::TessMaterials>,
) -> bool {
    current.is_some_and(|current| Arc::ptr_eq(owned, &current.catalog))
}

#[derive(Resource, Default)]
pub struct LocalSpawnArmed(pub bool);

#[derive(Resource, Default, Clone, Debug, PartialEq)]
pub struct FpvStatusGap(pub Option<FpvState>);

#[derive(Clone, Debug, PartialEq)]
pub enum FpvState {
    ClearedNotAlive,

    ClearedNoWeapon,

    Queued,

    Drawn { idle_sampled: bool },

    Blocked(RenderGapCause),
}

impl FpvState {
    pub fn label(&self) -> &'static str {
        match self {
            FpvState::ClearedNotAlive => "not Alive — FPV cleared",
            FpvState::ClearedNoWeapon => "held weapon 0 — FPV cleared",
            FpvState::Queued => "held weapon changed; FPV queued (linked gunXModel)",
            FpvState::Drawn { idle_sampled: true } => {
                "spawned after Equip; idle-sampled eye-posed hands+gun; retained FPV + ModelLightingCache"
            }
            FpvState::Drawn {
                idle_sampled: false,
            } => {
                "spawned after Equip; bind-pose eye-posed hands+gun; idle sample gap; retained FPV"
            }
            FpvState::Blocked(cause) => cause.label(),
        }
    }

    pub fn cause(&self) -> Option<&RenderGapCause> {
        match self {
            FpvState::Blocked(cause) => Some(cause),
            _ => None,
        }
    }
}

impl core::fmt::Display for FpvState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FpvState::Blocked(cause) => write!(f, "{cause}"),
            other => f.write_str(other.label()),
        }
    }
}

pub(crate) fn fpv_viewmodel_weapon(ps: &PlayerState, weapons: &FpvWeaponTable) -> u32 {
    let viewmodel = get_viewmodel_weapon_index(ps);
    let stabbing = matches!(
        weapon_iw4::WeaponState::from_i32(ps.weaponstate_primary),
        Ok(weapon_iw4::WeaponState::MeleeInit | weapon_iw4::WeaponState::MeleeFire)
    );
    if stabbing && viewmodel == ps.weapon {
        weapons.melee_weapon_of(viewmodel)
    } else {
        viewmodel
    }
}

fn fpv_rocket_should_attach(
    weapons: &FpvWeaponTable,
    weapon_id: u32,
    ps: Option<&PlayerState>,
) -> bool {
    let Some(ps) = ps else {
        return true;
    };
    let Some(facts) = weapons.facts_of(weapon_id) else {
        return false;
    };
    let viewmodel = fpv_viewmodel_weapon(ps, weapons);
    let clip_key = clip_table_key(facts.clip_index, viewmodel);
    let clip = get_clip_for_hand(&ps.ammoclip, clip_key, 0);
    viewmodel_rocket_should_be_attached(
        clip,
        ps.weaponstate_primary,
        ps.weapon_time,
        facts.reload_time_ms,
        facts.reload_show_rocket_time_ms,
    )
}

#[derive(SystemParam)]
pub struct SpawnPendingFpvInputs<'w> {
    prepared: Res<'w, PreparedFpv>,
    tess: Res<'w, render_scene::TessMaterials>,
    presented: Res<'w, PresentedSnapshot>,
    local: Res<'w, LocalPresentClient>,
}

pub fn spawn_pending_fpv(
    mut commands: Commands,
    mut pending: ResMut<PendingFpvSpawn>,
    inputs: SpawnPendingFpvInputs,
    mut session_vm: ResMut<SessionViewmodel>,
    mut cursor: ResMut<FpvPresentCursor>,
    mut fpv_plan: ResMut<crate::FpvDrawPlan>,
    cameras: Query<Entity, With<FlyCamera>>,
    existing_fpv: Query<Entity, With<FpvPlacementRoot>>,
    mut status: ResMut<FpvStatusGap>,
    gaps: Res<RenderPresentationGaps>,
) {
    let SpawnPendingFpvInputs {
        prepared,
        tess,
        presented,
        local,
    } = inputs;
    let Some(request) = pending.0.take() else {
        return;
    };
    let Some(table) = prepared
        .table()
        .filter(|table| same_material_catalog(table.material_catalog(), Some(&*tess)))
    else {
        pending.0 = Some(request);
        return;
    };
    let instance_started = std::time::Instant::now();
    cursor.0.forget_weap_anim();
    for entity in &existing_fpv {
        commands.entity(entity).try_despawn();
    }
    session_vm.0 = None;

    if table.catalog_id() != request.catalog_id
        || table.gun_index(request.weapon_id) != Some(request.gun_index)
    {
        let cause = RenderGapCause::FpvGunXModelUnresolved {
            weapon_id: request.weapon_id,
        };
        gaps.raise(cause.clone());
        status.0 = Some(FpvState::Blocked(cause));
        return;
    }
    let meta = presented
        .snapshot()
        .and_then(|snap| snap.meta.for_client(local.0));
    let ffa_team = meta.and_then(|m| m.ffa_team);
    let client_state_team = meta.map(|m| m.client_state_team).unwrap_or(0);
    let axis = asset_model::kit_assignment_is_axis(client_state_team, ffa_team);
    let view = match table.slot(request.weapon_id, request.parent_weapon, axis) {
        FpvWeaponSlot::Ready(view) => Arc::clone(view),
        FpvWeaponSlot::Refused(cause) => {
            gaps.raise(cause.clone());
            status.0 = Some(FpvState::Blocked(cause.clone()));
            return;
        }
        FpvWeaponSlot::Absent => {
            let cause = RenderGapCause::FpvGunXModelUnresolved {
                weapon_id: request.weapon_id,
            };
            gaps.raise(cause.clone());
            status.0 = Some(FpvState::Blocked(cause));
            return;
        }
    };
    let Ok(host) = cameras.single() else {
        diag::info!(
            Fpv,
            "fpv: no FlyCamera to parent viewmodel under — re-queue"
        );
        pending.0 = Some(request);
        gaps.raise(RenderGapCause::FpvNoCamera);
        status.0 = Some(FpvState::Blocked(RenderGapCause::FpvNoCamera));
        return;
    };

    commands.entity(host).with_children(|parent| {
        parent.spawn((
            FpvViewmodel,
            FpvPlacementRoot,
            Transform::IDENTITY,
            Visibility::Visible,
        ));
    });

    crate::clear_fpv_draw_plan(&mut fpv_plan, 0);
    let census = &view.census;
    fpv_plan.gun_colormap_skip_n = Some(census.gun_colormap_skip_n);
    fpv_plan.gun_ordinal_skip_n = Some(0);
    fpv_plan.gun_colormap_skip_names = (!census.gun_colormap_skip_names.is_empty())
        .then(|| census.gun_colormap_skip_names.join(","));
    fpv_plan.plan_mat_hints = (!census.mat_hints.is_empty()).then(|| census.mat_hints.join(","));

    let controller = ViewmodelController::new(view.right.clone());
    let left = view.left.clone().map(ViewmodelController::new);
    diag::info!(
        Fpv,
        "fpv: viewmodel controller `{}` — {} clips{}, fireTime={}ms raiseTime={}ms (action from weapAnim)",
        view.right.name,
        view.right.resolved_count(),
        view.left
            .as_ref()
            .map(|left| format!(" R, {} clips L (dual DObj)", left.resolved_count()))
            .unwrap_or_else(String::new),
        view.right.fire_time_ms,
        view.right.raise_time_ms,
    );
    let idle_kind = match view.idle_name.as_deref() {
        Some(name) => format!("idle `{name}` (szXAnims[IDLE])"),
        None => "no szXAnims[IDLE] — guess forbidden".to_owned(),
    };
    let idle_sampled = view.idle_name.is_some();
    diag::info!(
        Fpv,
        "fpv: equipped `{}` ({}; prepared before Ready) — instance {:.2}ms",
        view.gun_name,
        idle_kind,
        instance_started.elapsed().as_secs_f64() * 1000.0,
    );
    session_vm.0 = Some(SessionFpvMeshesHandles {
        weapon_id: request.weapon_id,
        parent_weapon: request.parent_weapon,
        catalog_id: request.catalog_id,
        axis,
        fpv: EquippedFpv::new(
            view.gun_name.clone(),
            view.gun_index,
            view.hands_index,
            view.namespace,
            view.hands.clone(),
            controller,
            left,
        ),
        view,
        active_rig: None,
        material_catalog: Arc::clone(table.material_catalog()),
    });

    gaps.clear(RenderGap::FpvViewmodel);
    status.0 = Some(FpvState::Drawn { idle_sampled });
}

fn viewweapon_drawgun_admit(
    ps: &PlayerState,
    weapons: Option<&FpvWeaponTable>,
    b_position_to_ads: bool,
) -> Option<(bool, Option<&'static str>)> {
    let reg = weapons?;
    let viewmodel = fpv_viewmodel_weapon(ps, reg);
    let facts = reg.facts_of(viewmodel)?;

    let hud_iris = reg.overlay_is_hud_iris(viewmodel);
    let weap = WeaponAdsOverlayFacts {
        ads_zoom_in_frac: facts.ads_zoom_in_frac,
        ads_zoom_out_frac: facts.ads_zoom_out_frac,
        overlay_material: u32::from(hud_iris),
        overlay_reticle: facts.overlay_reticle,
        ads_overlay_width: facts.ads_overlay_width,
        ads_overlay_height: facts.ads_overlay_height,
        ..WeaponAdsOverlayFacts::default()
    };
    let iris = get_weap_reticle_zoom(ps.f_weapon_pos_frac, b_position_to_ads, &weap);
    Some((
        viewweapon_drawgun(false, true, iris),
        viewweapon_drawgun_skip(false, true, iris),
    ))
}

fn viewweapon_drawgun_value(
    ps: &PlayerState,
    weapons: Option<&FpvWeaponTable>,
    b_position_to_ads: bool,
) -> Option<i32> {
    let (admit, _skip) = viewweapon_drawgun_admit(ps, weapons, b_position_to_ads)?;
    Some(i32::from(admit))
}

fn fpv_occupy_submission(
    lighting: [f32; 3],
    radius: Option<f32>,
    entnum: u32,
    model_n: u8,
) -> AnimDObjSceneSubmission {
    AnimDObjSceneSubmission {
        render_fx_flags: SCENE_VIEWMODEL_FX_FLAGS,
        has_tree: true,
        origin: lighting,
        lighting_origin: lighting,
        radius,
        entnum,
        quat: None,
        occupy_model_n: model_n,
        models: Vec::new(),
        hide_part_bits: [0; 6],
        store_skin: false,
    }
}

pub fn occupy_fpv_scene(
    (skate, jak): (Res<frame::SkateMode>, Res<frame::JakMode>),
    puppet: Option<Res<frame::InventoryPuppet>>,
    minecraft: Option<Res<frame::MinecraftUi>>,
    mut submissions: MessageWriter<AnimDObjSceneSubmission>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    view: Res<ViewSubject>,
    prepared: Res<PreparedFpv>,
    kick: Option<Res<SessionViewKick>>,
    session_vm: Option<Res<SessionViewmodel>>,
    tess: Option<Res<render_scene::TessMaterials>>,
    fpv_meshes: Option<Res<PreparedFpvMeshes>>,
) {
    if (skate.active && !skate.bones.is_empty())
        || jak.active
        || puppet.as_ref().is_some_and(|p| p.active)
        || minecraft.as_ref().is_some_and(|ui| ui.active && ui.holding_item && !ui.empty_hand)
        || presented.viewweapon_player(local.0).is_none()
        || presented_is_third_person(&presented, local.0, view.in_killcam())
    {
        return;
    }
    let Some(ps) = presented.viewweapon_player(local.0) else {
        return;
    };
    let admit = viewweapon_drawgun_admit(
        ps,
        prepared.table().map(|table| &**table),
        kick.as_ref().map(|k| k.b_position_to_ads).unwrap_or(true),
    );
    if !admit.is_some_and(|(ok, _)| ok) {
        return;
    }
    let Some(session) = session_vm.as_ref().and_then(|s| s.0.as_ref()) else {
        return;
    };
    if !same_material_catalog(&session.material_catalog, tess.as_deref()) {
        return;
    }
    let lighting = viewmodel_lighting_origin(
        ps.origin,
        ps.view_height_current,
        ps.viewangles[1],
        ps.leanf,
    );
    let radius = fpv_meshes.as_ref().and_then(|cat| {
        let (hands, gun) =
            fpv_dobj_skel_radii(&cat.0, session.fpv.hands_index, session.fpv.gun_index);
        match (hands, gun) {
            (Some(h), Some(g)) => Some(compute_bounds_radius(
                &[h, g],
                &[DOBJ_RADIUS_PARENT_ROOT, 0],
            )),
            (Some(r), None) | (None, Some(r)) => Some(r),
            (None, None) => None,
        }
    });
    let model_n: u8 = if session.fpv.gun_xmodel.is_empty() {
        1
    } else {
        2
    };
    submissions.write(fpv_occupy_submission(
        lighting,
        radius,
        SCENE_VIEWMODEL_ENTNUM,
        model_n,
    ));
    if ps.last_weapon_hand == 1 {
        submissions.write(fpv_occupy_submission(
            lighting,
            radius,
            SCENE_VIEWMODEL_LEFT_ENTNUM,
            model_n,
        ));
    }
}

fn refuse_gap_cause(refuse: FpvPoseRefuse) -> RenderGapCause {
    match refuse {
        FpvPoseRefuse::CatalogMissing => RenderGapCause::FpvCatalogMissing,
        FpvPoseRefuse::NoActiveClips => RenderGapCause::FpvNoActiveClips,
        FpvPoseRefuse::EyePoseFailed { gun_xmodel } => {
            RenderGapCause::FpvEyePoseFailed { gun_xmodel }
        }
        FpvPoseRefuse::DependencyUnresolved {
            weapon_id,
            role,
            name,
        } => RenderGapCause::FpvDependencyUnresolved {
            weapon_id,
            role,
            name,
        },
    }
}

pub fn tick_fpv_viewmodel(
    time: Res<Time>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    mut cursor: ResMut<FpvPresentCursor>,
    prepared: Res<PreparedFpv>,
    fpv_meshes: Option<Res<PreparedFpvMeshes>>,
    mut session_vm: ResMut<SessionViewmodel>,
    tess: Option<Res<render_scene::TessMaterials>>,
    mut settled: ResMut<FpvHeldSettled>,
    mut pending: ResMut<PendingFpvSpawn>,
    mut product: ResMut<FpvPoseProduct>,
    mut pending_notes: ResMut<PendingFpvNotetracks>,
    mut bolts: ResMut<FpvBoltTargets>,
    kick: Option<Res<SessionViewKick>>,
    view: Res<ViewSubject>,
) {
    let table = prepared.table().map(|table| &**table);
    pending_notes.weapon = 0;
    pending_notes.names.clear();
    bolts.clear();
    *product = FpvPoseProduct::default();
    if let Some(ps) = presented.viewweapon_player(local.0) {
        if !presented_is_third_person(&presented, local.0, view.in_killcam()) {
            product.drawgun = viewweapon_drawgun_value(
                ps,
                table,
                kick.as_ref().map(|k| k.b_position_to_ads).unwrap_or(true),
            );
        }
    }
    let Some(session) = session_vm.0.as_mut() else {
        product.kind = FpvPoseKind::Hide;
        return;
    };
    if !same_material_catalog(&session.material_catalog, tess.as_deref()) {
        diag::warn!(
            Fpv,
            "fpv: material catalog changed; retiring stale rig and bindings"
        );
        session_vm.0 = None;
        settled.0 = None;
        pending.0 = None;
        product.kind = FpvPoseKind::Hide;
        return;
    }
    if fpv_meshes
        .as_ref()
        .is_none_or(|fpv| fpv.0.identity() != session.catalog_id)
    {
        session_vm.0 = None;
        settled.0 = None;
        pending.0 = None;
        product.kind = FpvPoseKind::Hide;
        return;
    }
    if presented_is_third_person(&presented, local.0, view.in_killcam()) {
        product.kind = FpvPoseKind::Hide;
        return;
    }
    let Some(fpv) = fpv_meshes.as_ref() else {
        product.kind = FpvPoseKind::Refuse(FpvPoseRefuse::CatalogMissing);
        return;
    };

    if let Some(ps) = presented.viewweapon_player(local.0) {
        let weapon = table.map_or(get_viewmodel_weapon_index(ps), |t| {
            fpv_viewmodel_weapon(ps, t)
        });
        if weapon != 0
            && (weapon != session.weapon_id || ps.weapon_primary != session.parent_weapon)
        {
            if let Some(table) = table {
                match table.gun_index(weapon) {
                    Some(gun_index) if gun_index == session.fpv.gun_index => {
                        let same = match table.slot(weapon, ps.weapon_primary, session.axis) {
                            FpvWeaponSlot::Ready(next) => {
                                same_compositions(&next.assemblies, &session.view.assemblies)
                            }
                            _ => false,
                        };
                        if same {
                            session.weapon_id = weapon;
                            session.parent_weapon = ps.weapon_primary;
                        } else {
                            pending.0 = Some(PendingFpvSpawnRequest {
                                gun_index,
                                catalog_id: fpv.0.identity(),
                                weapon_id: weapon,
                                parent_weapon: ps.weapon_primary,
                            });
                            product.kind = FpvPoseKind::Hide;
                            return;
                        }
                    }
                    Some(gun_index) => {
                        pending.0 = Some(PendingFpvSpawnRequest {
                            gun_index,
                            catalog_id: fpv.0.identity(),
                            weapon_id: weapon,
                            parent_weapon: ps.weapon_primary,
                        });
                        diag::info!(
                            Fpv,
                            "fpv: weapon id → {weapon}; re-queue FPV for new gunXModel"
                        );
                        product.kind = FpvPoseKind::Hide;
                        return;
                    }
                    None => {
                        product.kind = FpvPoseKind::Hide;
                        return;
                    }
                }
            } else {
                product.kind = FpvPoseKind::Refuse(FpvPoseRefuse::CatalogMissing);
                return;
            }
        }
    }

    let dt = time.delta_secs();
    let (sample, predicted_fire) = match presented.snapshot() {
        Some(snap) => {
            let ps = presented.player(local.0);
            let ws = ps.map(|p| p.weaponstate_primary).unwrap_or(0);
            const PMF_SPRINTING: u32 = 0x4000;
            let sprinting = ps
                .map(|p| (p.pm_flags & PMF_SPRINTING) != 0)
                .unwrap_or(false);
            let ads_frac = ps.map(|p| p.f_weapon_pos_frac).unwrap_or(0.0);
            let weap_anim = ps.map(|p| p.weap_anim).unwrap_or(0);
            let clip_ammo = |hand| match (ps, table) {
                (Some(ps), Some(table)) => {
                    let viewmodel = fpv_viewmodel_weapon(ps, table);
                    table.facts_of(viewmodel).map(|facts| {
                        let key = clip_table_key(facts.clip_index, viewmodel);
                        get_clip_for_hand(&ps.ammoclip, key, hand)
                    })
                }
                _ => None,
            };
            let cues = presented.fpv_cues(local.0);
            let predicted_fire = if let Some(ps) = ps {
                let life = snap
                    .meta
                    .for_client(local.0)
                    .map(|m| m.life_sequence.0)
                    .unwrap_or(0);
                let id = local_shot_identity(life, 0, ps.weapon_shot_count, ps.weap_anim);
                let edged = cursor.0.local_shot.observe(id);
                let masked = ps.weap_anim as u32 & weapon_iw4::WEAP_ANIM_EVENT_MASK;
                edged && is_predicted_fire_weap_anim(masked)
            } else {
                false
            };
            (
                Some(FpvAuthoritySample {
                    tick: snap.tick.0,
                    weaponstate: ws,
                    cues: AuthorityFpvCues {
                        shot_accepted: false,
                        attack_released: cues.attack_released,
                        spawned: cues.spawned,
                    },
                    sprinting,
                    ads_frac,
                    weap_anim,
                    weap_anim_secondary: ps.map(|p| p.weap_anim_secondary).unwrap_or(0),
                    last_weapon_hand: ps.map(|p| p.last_weapon_hand).unwrap_or(0),
                    perks0: ps.map(|p| p.perks[0]).unwrap_or(0),
                    clip_ammo: clip_ammo(0),
                    left_clip_ammo: clip_ammo(1),
                }),
                predicted_fire,
            )
        }
        None => (None, false),
    };
    let rocket_visible = table.is_some_and(|table| {
        fpv_rocket_should_attach(table, session.weapon_id, presented.player(local.0))
    });
    let dual = presented
        .viewweapon_player(local.0)
        .is_some_and(|ps| ps.last_weapon_hand == 1);
    let dual_offset = if dual {
        presented.viewweapon_player(local.0).and_then(|ps| {
            table?
                .facts_of(fpv_viewmodel_weapon(ps, table?))
                .map(|f| f.dual_wield_view_model_offset)
        })
    } else {
        None
    };
    let weapon_id = session.weapon_id;
    let SessionFpvMeshesHandles {
        fpv: equipped,
        active_rig,
        view: equipped_view,
        ..
    } = session;
    let mut kind = generate_fpv_pose(FpvGenerateArgs {
        dt,
        equipped,
        rigs: &equipped_view.rigs,
        active: active_rig,
        cursor: &mut cursor.0,
        rocket: rocket_visible,
        sample,
        predicted_fire,
        dual,
        dual_offset,
    });
    if let FpvPoseKind::Posed(frame) = &mut kind {
        if weapon_id != 0 && !frame.notetracks.is_empty() {
            pending_notes.weapon = weapon_id;
            pending_notes.names.clone_from(&frame.notetracks);
        }
        // Nothing downstream of the bones waits for a vertex.
        for (hand, pose) in frame.poses.iter_mut().enumerate() {
            if let Some(pose) = pose.as_mut() {
                bolts.set_pose(hand, core::mem::take(&mut pose.bolt));
            }
        }
    }
    product.kind = kind;
}

/// Write this frame's vertices into the buffer the rig published, and nothing
/// else: indices, surface ranges, materials and draws belong to the composition.
fn skin_fpv_geometry(
    product: Res<FpvPoseProduct>,
    session_vm: Option<Res<SessionViewmodel>>,
    tess: Option<Res<render_scene::TessMaterials>>,
    fpv_meshes: Option<Res<PreparedFpvMeshes>>,
    mut fpv_plan: ResMut<crate::FpvDrawPlan>,
    mut status: ResMut<FpvStatusGap>,
    gaps: Res<RenderPresentationGaps>,
    minecraft: Option<Res<frame::MinecraftUi>>,
    mut lenses: Query<
        &mut Transform,
        (
            With<FpvLens>,
            Without<RemotePlayer>,
            Without<WorldScriptModelInstance>,
        ),
    >,
) {
    fpv_plan.drawgun = product.drawgun;
    let handle = fpv_plan.lighting_handle;
    match &product.kind {
        FpvPoseKind::Hide => crate::clear_fpv_draw_plan(&mut fpv_plan, handle),
        FpvPoseKind::Refuse(refuse) => {
            let cause = refuse_gap_cause(refuse.clone());
            crate::clear_fpv_draw_plan(&mut fpv_plan, handle);
            gaps.raise(cause.clone());
            status.0 = Some(FpvState::Blocked(cause));
        }
        FpvPoseKind::Posed(frame) => {
            let session = session_vm.as_ref().and_then(|session| session.0.as_ref());
            if !session.is_some_and(|session| {
                same_material_catalog(&session.material_catalog, tess.as_deref())
            }) {
                crate::clear_fpv_draw_plan(&mut fpv_plan, handle);
                return;
            }
            let rig = session.and_then(|session| session.active_rig.as_ref());
            let (Some(rig), Some(catalog)) = (rig, fpv_meshes.as_ref()) else {
                crate::clear_fpv_draw_plan(&mut fpv_plan, handle);
                return;
            };
            if session.is_none_or(|session| session.catalog_id != catalog.0.identity()) {
                crate::clear_fpv_draw_plan(&mut fpv_plan, handle);
                return;
            }
            // An empty hand on a Minecraft map shows the hands without the gun.
            let hands_only = minecraft.as_ref().is_some_and(|ui| ui.active && ui.empty_hand);
            if fpv_plan.rig_generation != rig.generation() || fpv_plan.hands_only != hands_only {
                crate::install_prepared_fpv_plan(&mut fpv_plan, &rig.geometry, handle);
                if hands_only {
                    fpv_plan.draws.retain(|draw| draw.hands);
                }
                fpv_plan.hands_only = hands_only;
                fpv_plan.revisions.bump_draws();
                fpv_plan.revision = fpv_plan.revision.wrapping_add(1);
                fpv_plan.rig_generation = rig.generation();
            }
            if let Some(rows) = fpv_plan.packed_rows_mut() {
                rig.skin_into(&catalog.0, &frame.poses, rows);
            }
            fpv_plan.revisions.bump_vertices();
            fpv_plan.geometry_ok = !fpv_plan.draws().is_empty();
            fpv_plan.settle_visible();
            gaps.clear(RenderGap::FpvViewmodel);
            status.0 = Some(FpvState::Drawn {
                idle_sampled: frame.idle_sampled,
            });
            for mut lens_tf in &mut lenses {
                *lens_tf = Transform::from_matrix(frame.lens);
            }
        }
    }
}

/// Where the viewmodel sits this frame. It reads the placement the camera and
/// the root carry and nothing the rig produced, so it does not wait behind the
/// geometry.
pub fn stamp_fpv_placement_matrix(
    mut fpv_plan: ResMut<crate::FpvDrawPlan>,
    cameras: Query<&Transform, (With<FlyCamera>, Without<FpvPlacementRoot>)>,
    roots: Query<&Transform, With<FpvPlacementRoot>>,
) {
    let (Ok(cam), Ok(local)) = (cameras.single(), roots.single()) else {
        fpv_plan.placement_ok = false;
        fpv_plan.settle_visible();
        return;
    };
    fpv_plan.world_from_local = cam.to_matrix() * local.to_matrix();
    fpv_plan.placement_ok = true;
    fpv_plan.settle_visible();
}

pub fn publish_fpv_dobj_pose(
    fpv_plan: Res<crate::FpvDrawPlan>,
    roots: Query<(), With<FpvPlacementRoot>>,
    mut bolts: ResMut<FpvBoltTargets>,
    mut dobj_poses: ResMut<crate::anim::dobj_pose::HostDObjPoseFrame>,
) {
    if roots.single().is_err() {
        bolts.clear();
        return;
    }
    bolts.tracker_screen = None;
    bolts.tracker_light = None;
    for hand in 0..2usize {
        let Some(frame) = bolts.pose[hand].take() else {
            continue;
        };
        let dobj = fx_iw4::FX_BOLT_VIEWMODEL_DOBJ_BASE + hand as u32;

        if dobj_poses
            .publish(dobj, true, 0, fpv_plan.world_from_local, &frame.bones)
            .is_err()
        {
            continue;
        }
        let target = |bone: Option<u16>| -> Option<fx::FxBoltTarget> {
            let bone = bone?;
            let orientation = dobj_poses.resolve(dobj, i32::from(bone)).ok()?;
            Some(fx::FxBoltTarget {
                dobj,
                bone,
                centity_teleport: false,
                orientation,
            })
        };

        if hand == 0 && fpv_plan.placement_ok {
            bolts.tracker_light = target(frame.tags.tracker_light);
            bolts.tracker_screen = (|| {
                let mut points = [Vec3::ZERO; 3];
                for (point, bone) in points.iter_mut().zip(frame.tags.tracker_screen) {
                    *point = fpv_plan
                        .world_from_local
                        .transform_point3(frame.bones.get(usize::from(bone?))?.w_axis.truncate());
                }
                Some(points)
            })();
        }
        bolts.flash[hand] = target(frame.tags.flash);
        bolts.brass[hand] = target(frame.tags.brass);
        bolts.knife[hand] = target(frame.tags.knife);
        bolts.laser[hand] = target(frame.tags.laser);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn apply_fpv_placement(
    clock: Res<FrameClock>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    prepared: Res<PreparedFpv>,
    mut kick: ResMut<SessionViewKick>,
    cg_gun: Res<GunOffset>,
    mut aim: ResMut<ViewweaponAim>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut roots: Query<
        &mut Transform,
        (
            With<FpvPlacementRoot>,
            Without<RemotePlayer>,
            Without<WorldScriptModelInstance>,
        ),
    >,
    view: Res<ViewSubject>,
    mut gfx_scene: ResMut<HostGfxScene>,
    minecraft: Option<Res<frame::MinecraftUi>>,
) {
    *aim = ViewweaponAim::default();
    let Ok(mut transform) = roots.single_mut() else {
        return;
    };
    let Some(ps) = presented.player(local.0) else {
        return;
    };
    if presented_is_third_person(&presented, local.0, view.in_killcam()) {
        return;
    }
    let Some(table) = prepared.table() else {
        return;
    };
    let viewmodel = fpv_viewmodel_weapon(ps, table);
    let Some(facts) = table.facts_of(viewmodel) else {
        return;
    };

    let mut state = WeaponPlacementState {
        sway_springs: kick.sway.springs(),
        gun_recoil: GunRecoilPlacementState {
            pitch_offset: kick.state.gun_angles[0],
            pitch_speed: kick.state.gun_speed[0],
            yaw_offset: kick.state.gun_angles[1],
            yaw_speed: kick.state.gun_speed[1],
        },
        movement_origin: kick.placement_move_origin,
        movement_angles: kick.placement_move_angles,
        weap_idle_time: kick.weap_idle_time,
        last_idle_factor: kick.last_idle_factor,
        damage_kick_time: clock.time(),
        damage_time: kick.damage_time,
        v_dmg_pitch: kick.v_dmg_pitch,
        v_dmg_roll: kick.v_dmg_roll,
        ..Default::default()
    };

    let overlay_reticle = if facts.overlay_reticle != 0 {
        facts.overlay_reticle
    } else if table.overlay_is_hud_iris(viewmodel) {
        1
    } else {
        0
    };
    let ps_in = WeaponPlacementPsInputs {
        e_flags: ps.e_flags,
        weapon_pos_frac: ps.f_weapon_pos_frac,
        weapon_time: ps.weapon_time,
        aim_down_sight: facts.aim_down_sight,
        overlay_reticle,
        weapon_transition_active: false,

        lean_fraction: 0.0,
    };
    let stance = WeaponStanceStaticOfsInputs {
        ducked_ofs: facts.ducked_ofs,
        prone_ofs: facts.prone_ofs,
        ads_aim_pitch: facts.ads_aim_pitch,
        night_vision_wear_time: facts.night_vision_wear_time,
    };
    let bob_inputs = WeaponBobInputs {
        ads_bob_factor: facts.ads_bob_factor,
    };
    let xyspeed = {
        let vx = ps.velocity[0];
        let vy = ps.velocity[1];
        vec3_length([vx, vy, 0.0])
    };
    let kinematics = WeaponMovementKinematics {
        xyspeed,
        speed: ps.speed as f32,
        velocity: ps.velocity,
        viewangles: ps.viewangles,
        weaponstate: ps.weaponstate_primary,
        weaponstate_secondary: ps.weaponstate_secondary,
        pm_flags: ps.pm_flags,
        frametime: clock.frametime_secs(),
    };
    let waveform = calculate_weapon_movement_bob_waveform(WeaponBobWaveformInputs {
        bob_cycle: (ps.bob_cycle as u32 & 0xff) as u8,
        xyspeed,
        view_height_target: ps.view_height_target,
        pm_flags: ps.pm_flags,
        weapon_pos_frac: ps.f_weapon_pos_frac,
    });
    let hip = GunKickSpring {
        accel: facts.kick.hip_gun_kick_accel,
        speed_max: facts.kick.hip_gun_kick_speed_max,
        speed_decay: facts.kick.hip_gun_kick_speed_decay,
        static_decay: facts.kick.hip_gun_kick_static_decay,
    };
    let ads = GunKickSpring {
        accel: facts.kick.ads_gun_kick_accel,
        speed_max: facts.kick.ads_gun_kick_speed_max,
        speed_decay: facts.kick.ads_gun_kick_speed_decay,
        static_decay: facts.kick.ads_gun_kick_static_decay,
    };
    let mut steps = [WeaponPlacementAssembleStep::Sway; PLACEMENT_ASSEMBLE_STEP_COUNT];

    let contrib = weapon_placement_assemble(
        &mut state,
        ps_in,
        stance,
        StanceTransitionFadeGlobals::default(),
        facts.movement,
        kinematics,
        bob_inputs,
        facts.idle,
        Some(waveform),
        hip,
        ads,
        facts.kick.gun_max_pitch,
        facts.kick.gun_max_yaw,
        0.0,
        &mut steps,
    );
    kick.placement_move_origin = state.movement_origin;
    kick.placement_move_angles = state.movement_angles;
    kick.weap_idle_time = state.weap_idle_time;
    kick.last_idle_factor = state.last_idle_factor;
    let mut origin = apply_viewweapon_land_view(
        apply_cg_gun_offset_view(contrib.origin, cg_gun.xyz()),
        kick.viewweapon_land_view,
    );
    if ps.last_weapon_hand == 1 {
        let add = dual_wield_view_model_origin_add(
            0,
            [0.0, 1.0, 0.0],
            facts.dual_wield_view_model_offset,
        );
        origin[0] += add[0];
        origin[1] += add[1];
        origin[2] += add[2];
    }
    let from_axis = viewweapon_iron_ads_saves_composed_axis(
        facts.aim_down_sight,
        ps.f_weapon_pos_frac,
        overlay_reticle,
    );
    let [gun_pitch, gun_yaw] = viewweapon_save_gun_pitch_yaw(
        contrib.angles,
        kick.refdef_view_angles,
        facts.aim_down_sight,
        ps.f_weapon_pos_frac,
        overlay_reticle,
    );
    let xhair = if kick.horiz_fov_deg > 0.0 {
        if let Ok(window) = windows.single() {
            let height = window.height().max(1.0);
            let aspect = window.width() / height;
            let (tan_x, tan_y) = tan_half_fov(kick.horiz_fov_deg, aspect);
            let (vf, vr, vu) = angle_vectors(kick.refdef_view_angles);
            calc_crosshair_position(
                gun_pitch,
                gun_yaw,
                kick.refdef_view_angles[2],
                vf,
                vr,
                vu,
                tan_x,
                tan_y,
            )
        } else {
            [0.0, 0.0]
        }
    } else {
        [0.0, 0.0]
    };
    *aim = ViewweaponAim {
        live: true,
        gun_pitch,
        gun_yaw,
        xhair_x: xhair[0],
        xhair_y: xhair[1],
        from_composed_axis: from_axis,
    };
    let placed = iw_view_placement_to_bevy_camera_local(origin, contrib.angles);
    let world_delta = viewweapon_view_to_world_delta(origin, kick.refdef_view_angles);
    let pose_origin = [
        kick.refdef_vieworg[0] + world_delta[0],
        kick.refdef_vieworg[1] + world_delta[1],
        kick.refdef_vieworg[2] + world_delta[2],
    ];
    let pose_quat = scene_quat_from_viewmodel_axes(contrib.angles, kick.refdef_view_angles);
    gfx_scene
        .scene
        .store_pose_origin_quat(SCENE_VIEWMODEL_ENTNUM, pose_origin, Some(pose_quat));
    if ps.last_weapon_hand == 1 {
        gfx_scene.scene.store_pose_origin_quat(
            SCENE_VIEWMODEL_LEFT_ENTNUM,
            pose_origin,
            Some(pose_quat),
        );
    }
    *transform = placed;
    if let Some(ui) = minecraft.as_ref().filter(|ui| ui.active && ui.empty_hand) {
        minecraft_hand_placement(&mut transform, ui.hand_swing);
    }
}

/// The bare hands where Minecraft holds its arm: lower and further right
/// than a gun, jabbing forward and down through a swing as
/// `ItemInHandRenderer.renderPlayerArm` moves the arm.
fn minecraft_hand_placement(transform: &mut Transform, swing: f32) {
    use std::f32::consts::PI;
    // Inches in camera space: right, up, back.
    const REST: Vec3 = Vec3::new(2.5, -2.0, 1.0);
    // Inches to one of Minecraft's arm units.
    const ARM: f32 = 14.0;
    let root = swing.sqrt();
    let across = -0.3 * (root * PI).sin();
    let lift = 0.4 * (root * 2.0 * PI).sin();
    let thrust = -0.4 * (swing * PI).sin();
    let jab = Vec3::new(across, lift, thrust) * ARM;
    let turn = (root * PI).sin();
    let roll = (swing * swing * PI).sin();
    let rotation = Quat::from_rotation_y(turn * 12f32.to_radians())
        * Quat::from_rotation_x(-turn * 18f32.to_radians())
        * Quat::from_rotation_z(-roll * 10f32.to_radians());
    transform.translation = rotation * (transform.translation + REST) + jab;
    transform.rotation = rotation * transform.rotation;
}

fn fpv_spawn_queued(pending: Res<PendingFpvSpawn>) -> bool {
    pending.0.is_some()
}

fn flush_fpv_spawn(world: &mut World) {
    world.flush();
}

fn publish_fpv_notetracks(
    generation: Res<frame::WorldGeneration>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    pending: Res<PendingFpvNotetracks>,
    mut notes: MessageWriter<audio::ViewmodelNotetracks>,
) {
    if pending.weapon == 0 || pending.names.is_empty() {
        return;
    }
    let Some(meta) = presented
        .snapshot()
        .and_then(|snapshot| snapshot.meta.for_client(local.0))
    else {
        return;
    };
    notes.write(audio::ViewmodelNotetracks {
        generation: *generation,
        client: local.0,
        life: meta.life_sequence,
        weapon: pending.weapon,
        names: pending.names.clone(),
    });
}

pub fn register_fpv_present_systems(app: &mut App) {
    app.init_resource::<LocalSpawnArmed>()
        .init_resource::<SessionViewmodel>()
        .init_resource::<PreparedFpv>()
        .init_resource::<crate::anim::model_materials::PreparedModelMaterials>()
        .init_resource::<SessionViewKick>()
        .init_resource::<GunOffset>()
        .init_resource::<ViewweaponAim>()
        .init_resource::<PendingViewHurt>()
        .init_resource::<FpvStatusGap>()
        .init_resource::<RenderPresentationGaps>()
        .add_systems(
            Update,
            reset_view_kick_on_life_started.in_set(LifeFrontPublished),
        )
        .add_systems(
            Update,
            occupy_fpv_scene
                .after(PresentedPublished)
                .in_set(render_scene::GfxSceneAdd)
                .in_set(AnimSceneSubmit),
        )
        .add_systems(
            Update,
            (
                tick_session_view_kick.after(reset_view_kick_on_life_started),
                sync_camera_from_presented.after(tick_session_view_kick),
                spawn_pending_fpv
                    .run_if(fpv_spawn_queued)
                    .after(sync_camera_from_presented),
                flush_fpv_spawn.after(spawn_pending_fpv),
                tick_fpv_viewmodel.after(flush_fpv_spawn),
                skin_fpv_geometry
                    .after(tick_fpv_viewmodel)
                    .in_set(FpvGeometrySet),
                publish_fpv_notetracks.after(tick_fpv_viewmodel),
                apply_fpv_placement
                    .after(tick_fpv_viewmodel)
                    .before(WorkerCmdSet::CellSceneEnt),
                // Neither placement nor bone publication reads a vertex.
                stamp_fpv_placement_matrix
                    .after(apply_fpv_placement)
                    .in_set(FpvPlacementSet),
                publish_fpv_dobj_pose
                    .after(stamp_fpv_placement_matrix)
                    .after(crate::anim::dobj_pose::begin_dobj_pose_frame),
            )
                .in_set(ClientSet::Present),
        );
}
