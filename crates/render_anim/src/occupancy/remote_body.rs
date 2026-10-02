use crate::anim::remote_body::{
    AdvancedRemoteTree, CpuBodyGeom, CpuSurfMeta, PendingGunSkin, PreparedRemoteKits,
    RemoteBodySkinnedItem, RemoteBodySkinnedQueue, RemoteBodyTrees, RemoteModelLods,
    RemoteSkinModels, RemoteSkinPoseHashes, SkinAfterPose, WorldGunGap, advance_remote_tree,
    bind_remote_skin_models, clone_corpse_tree_from_victim, commit_assembled_body,
    ensure_remote_dobj, hash_skin_matrices, occupy_lod_byte, occupy_remote_kit_dobj, packed_anim,
    pose_remote_dobj, push_cached_surfaces, radii, remote_player_controller, select_remote_lods,
    select_remote_models, skin_after_pose, skin_slot_need, skip_frozen_corpse_dobj,
    take_unique_geom, validate_remote_tracks, zero_anim,
};
use crate::anim::scene_submission::{AnimDObjSceneSkels, AnimDObjSceneSubmission, AnimSceneSubmit};
use crate::anim::xmodel_pose::{build_skin_layout, skin_packed_into, stream_lod_surface_rigid};
use crate::gaps::{RenderGap, RenderGapCause, RenderPresentationGaps};
use crate::lighting_box_half;
use crate::{
    RemoteBodyDrawPlan, append_remote_body_cpu_blob, finish_remote_body_draw_plan,
    install_body_packed_session, push_remote_body_cpu_draw, take_body_packed_session,
    topology_fingerprint,
};
use anim_iw4::{PLAYER_ANIM_RAW_MASK, PlayerAnimValue};
use asset_model::ModelSkel;
use assets::{PreparedBodies, PreparedWeapons, PreparedWorldWeapons};
use bevy::prelude::*;
use bevy::tasks::ComputeTaskPool;
use entity_iw4::{ET_PLAYER, ET_PLAYER_CORPSE};
use frame::{ModelLightingSeated, ViewSubject, WorkerCmdSet};
use net::{
    CEntity, CEntityRuntime, ClientSet, FrameClock, LocalPresentClient, PlayerDrawGate,
    PresentedPublished, PresentedSnapshot, remote_body_submits, remote_pose_sample,
};
use render_scene::HostGfxScene;
use render_scene::WorldModelLightingAtlas;
use render_scene::WorldScriptModelInstance;
use render_scene::{FlyCamera, FpvLens};
use render_scene::{LodRampArgs, smodel_camera_lod};
use render_scene::{
    ModelLightingOwner, ModelLightingRequest, ModelLightingRequests, ResolvedModelLighting,
    ResolvedModelLightingTable,
};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[derive(Component, Debug, Clone, Copy)]
#[require(RemoteFxBolts)]
pub struct RemotePlayer {
    pub is_bot: bool,
    pub ffa_team: Option<u8>,

    pub client_state_team: i32,
}

#[derive(Component, Default)]
pub struct RemoteFxBolts {
    pub flash: Option<fx::FxBoltTarget>,

    pub brass: Option<fx::FxBoltTarget>,

    pub knife: Option<fx::FxBoltTarget>,

    pub laser: Option<fx::FxBoltTarget>,
}

#[derive(Resource, Default)]
struct RemoteBodyLightingBinds {
    by_client: HashMap<u32, ModelLightingRequest>,
}

pub fn register_remote_body_systems(app: &mut App) {
    app.init_resource::<RenderPresentationGaps>()
        .init_resource::<RemoteBodyDrawPlan>()
        .init_resource::<RemoteBodyTrees>()
        .init_resource::<RemoteSkinPoseHashes>()
        .init_resource::<RemoteBodySkinnedQueue>()
        .init_resource::<RemoteBodyLightingBinds>()
        .init_resource::<PreparedRemoteKits>()
        .add_systems(Update, prepare_remote_kits.in_set(ClientSet::Load))
        .init_resource::<crate::anim::dobj_pose::HostDObjPoseFrame>()
        .init_resource::<crate::anim::dobj_pose::PosedPlayerFrame>()
        .add_systems(
            Update,
            sync_remote_bodies
                .after(PresentedPublished)
                .after(crate::minecraft_world::place_inventory_puppet)
                .before(occupy_remote_scene_ents)
                .in_set(render_scene::GfxSceneAdd),
        )
        .add_systems(
            Update,
            occupy_remote_scene_ents
                .after(sync_remote_bodies)
                .after(PresentedPublished)
                .after(crate::occupancy::fpv_present::occupy_fpv_scene)
                .in_set(render_scene::GfxSceneAdd)
                .in_set(AnimSceneSubmit),
        )
        .add_systems(
            Update,
            (
                begin_remote_body_tess
                    .after(sync_remote_bodies)
                    .before(pose_remote_bodies),
                pose_remote_bodies
                    .after(sync_remote_bodies)
                    .after(begin_remote_body_tess)
                    .after(crate::anim::dobj_pose::begin_dobj_pose_frame)
                    .after(WorkerCmdSet::CellStatic)
                    .after(WorkerCmdSet::CellSceneEnt)
                    .after(WorkerCmdSet::DpvsEnt),
            )
                .after(PresentedPublished)
                .in_set(WorkerCmdSet::SkinModel),
        )
        .add_systems(
            Update,
            crate::anim::dobj_pose::begin_dobj_pose_frame
                .after(PresentedPublished)
                .in_set(ClientSet::Present),
        )
        .add_systems(
            Update,
            enqueue_remote_body_lighting
                .after(WorkerCmdSet::SkinModel)
                .after(crate::occupancy::script_model::ScriptModelSkinSet)
                .before(WorkerCmdSet::CellDynModel)
                .after(PresentedPublished)
                .in_set(ClientSet::Present),
        )
        .add_systems(
            Update,
            (
                submit_remote_bodies.after(ModelLightingSeated),
                finish_body_draw_plan.after(submit_remote_bodies),
            )
                .in_set(WorkerCmdSet::AddSceneEnt),
        );
}

fn prepare_remote_kits(
    bodies: Option<Res<PreparedBodies>>,
    weapons: Option<Res<PreparedWeapons>>,
    world_weapons: Option<Res<PreparedWorldWeapons>>,
    mut kits: ResMut<PreparedRemoteKits>,
) {
    let (Some(bodies), Some(weapons), Some(world)) = (bodies, weapons, world_weapons) else {
        return;
    };
    if kits.owned_by(&bodies, &weapons, &world) {
        return;
    }
    let started = std::time::Instant::now();
    *kits = PreparedRemoteKits::prepare(&bodies, &weapons, &world);
    diag::info!(
        World,
        "remote kits: prepared for {} weapons in {:.1}ms",
        weapons.0.len(),
        started.elapsed().as_secs_f64() * 1000.0
    );
}

fn finish_body_draw_plan(mut plan: ResMut<RemoteBodyDrawPlan>) {
    finish_remote_body_draw_plan(&mut plan);
}

fn begin_remote_body_tess() {}

fn occupy_remote_scene_ents(
    (skate, jak): (Res<frame::SkateMode>, Res<frame::JakMode>),
    puppet: Option<Res<frame::InventoryPuppet>>,
    mut scene_skels: ResMut<AnimDObjSceneSkels>,
    mut scene_submissions: MessageWriter<AnimDObjSceneSubmission>,
    lod_skinned: Res<render_scene::LodRampSkinnedDvar>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    local: Res<LocalPresentClient>,
    presented: Res<PresentedSnapshot>,
    subject: Option<Res<ViewSubject>>,
    bodies: Option<Res<PreparedBodies>>,
    weapons: Option<Res<PreparedWeapons>>,
    world_weapons: Option<Res<PreparedWorldWeapons>>,
    prepared_kits: Res<PreparedRemoteKits>,
    remotes: Query<
        (&CEntity, &CEntityRuntime, &Transform),
        (
            Without<WorldScriptModelInstance>,
            Without<FpvLens>,
            Without<FlyCamera>,
        ),
    >,
) {
    let Some(bodies) = bodies.as_deref() else {
        return;
    };
    if let Some(world) = world_weapons.as_deref() {
        scene_skels.seed(bodies, world);
    }
    let live_kits = match (weapons.as_deref(), world_weapons.as_deref()) {
        (Some(weapons), Some(world)) if prepared_kits.owned_by(bodies, weapons, world) => {
            Some((&*prepared_kits, world))
        }
        _ => None,
    };
    let kits = bodies.0.kits();
    if kits.allies.is_none() && kits.axis.is_none() {
        return;
    }
    let eye = cameras.single().ok().map(|xf| xf.translation());
    let in_killcam = subject.as_ref().is_some_and(|s| s.in_killcam());
    let eyes = match subject.as_deref() {
        Some(ViewSubject::Seat {
            focus: Some(focus), ..
        }) => i32::try_from(*focus).unwrap_or(0),
        _ => i32::try_from(local.0.0).unwrap_or(0),
    };
    let gate = PlayerDrawGate {
        eyes_entity_num: eyes,
        other_flags: presented
            .player(local.0)
            .map(|ps| ps.other_flags)
            .unwrap_or(0),
        rendering_third_person: (skate.active && !skate.bones.is_empty())
            || jak.active
            || puppet.as_ref().is_some_and(|p| p.active)
            || crate::occupancy::third_person::presented_is_third_person(
                &presented, local.0, in_killcam,
            ),
    };
    for (identity, runtime, transform) in &remotes {
        if !remote_body_submits(identity.number(), runtime, gate) {
            continue;
        }
        let Some(client) = identity.client() else {
            continue;
        };
        let meta = presented
            .snapshot()
            .and_then(|snap| snap.meta.for_client(client));
        let ffa_team = meta.and_then(|m| m.ffa_team);
        let client_state_team = meta.map(|m| m.client_state_team).unwrap_or(0);
        let axis = asset_model::kit_assignment_is_axis(client_state_team, ffa_team);
        let held = remote_pose_sample(runtime).weapon;
        let (kit_models, radius, hide_part_bits) = match live_kits {
            Some((kits, world)) => {
                let Some(kit) = kits.get(axis, held) else {
                    continue;
                };
                let Some(models) = kit.models(bodies, world) else {
                    continue;
                };
                (models, kit.radius, kit.hide_part_bits)
            }
            None => {
                let Some((models, radius)) = occupy_remote_kit_dobj(
                    bodies,
                    weapons.as_deref(),
                    world_weapons.as_deref(),
                    axis,
                    held,
                    true,
                ) else {
                    continue;
                };
                let bits = crate::anim::remote_body::kit_hide_part_bits(&models);
                (models, radius, bits)
            }
        };
        let origin = transform.translation.to_array();
        let quat = [
            transform.rotation.x,
            transform.rotation.y,
            transform.rotation.z,
            transform.rotation.w,
        ];
        let entnum = u32::from(identity.number());

        let ramp = lod_skinned.args();
        let lod_bytes: Vec<i8> = kit_models
            .iter()
            .map(|model| occupy_lod_byte(skel_camera_lod(model.skel, origin, eye, ramp)))
            .collect();
        let models: Vec<_> = kit_models
            .iter()
            .enumerate()
            .map(|(i, model)| {
                scene_skels.model(
                    model.name,
                    model.skel,
                    lod_bytes.get(i).copied().unwrap_or(-1),
                )
            })
            .collect();
        scene_submissions.write(AnimDObjSceneSubmission {
            render_fx_flags: 0,
            has_tree: true,
            origin,
            lighting_origin: origin,
            radius,
            entnum,
            quat: Some(quat),
            occupy_model_n: u8::try_from(models.len()).unwrap_or(u8::MAX),
            models,
            hide_part_bits,
            store_skin: true,
        });
    }
}

fn sync_remote_bodies(
    (skate, jak): (Res<frame::SkateMode>, Res<frame::JakMode>),
    puppet: Option<Res<frame::InventoryPuppet>>,
    mut commands: Commands,
    local: Res<LocalPresentClient>,
    presented: Res<PresentedSnapshot>,
    subject: Option<Res<ViewSubject>>,
    bodies: Option<Res<PreparedBodies>>,
    existing: Query<(Entity, &CEntity, &CEntityRuntime, Option<&RemotePlayer>)>,
) {
    let Some(bodies) = bodies else {
        return;
    };

    let kits = bodies.0.kits();
    if kits.allies.is_none() && kits.axis.is_none() {
        return;
    }

    let in_killcam = subject.as_ref().is_some_and(|s| s.in_killcam());
    let eyes = match subject.as_deref() {
        Some(ViewSubject::Seat {
            focus: Some(focus), ..
        }) => i32::try_from(*focus).unwrap_or(0),
        _ => i32::try_from(local.0.0).unwrap_or(0),
    };
    let gate = PlayerDrawGate {
        eyes_entity_num: eyes,
        other_flags: presented
            .player(local.0)
            .map(|ps| ps.other_flags)
            .unwrap_or(0),
        rendering_third_person: (skate.active && !skate.bones.is_empty())
            || jak.active
            || puppet.as_ref().is_some_and(|p| p.active)
            || crate::occupancy::third_person::presented_is_third_person(
                &presented, local.0, in_killcam,
            ),
    };

    for (entity, identity, runtime, remote) in &existing {
        let Some(client) = identity.client() else {
            if remote.is_some() {
                commands
                    .entity(entity)
                    .remove::<(RemotePlayer, RemoteFxBolts)>();
            }
            continue;
        };
        if !remote_body_submits(identity.number(), runtime, gate) {
            if remote.is_some() {
                commands
                    .entity(entity)
                    .remove::<(RemotePlayer, RemoteFxBolts)>();
            }
            continue;
        }
        let meta = presented
            .snapshot()
            .and_then(|snap| snap.meta.for_client(client));
        let ffa_team = meta.and_then(|m| m.ffa_team);
        let client_state_team = meta.map(|m| m.client_state_team).unwrap_or(0);
        let Some(kit) = kits.kit(asset_model::kit_assignment_is_axis(client_state_team, ffa_team))
        else {
            if remote.is_some() {
                commands
                    .entity(entity)
                    .remove::<(RemotePlayer, RemoteFxBolts)>();
            }
            continue;
        };
        let Some(body_entry) = bodies.0.get(&kit.body) else {
            if remote.is_some() {
                commands
                    .entity(entity)
                    .remove::<(RemotePlayer, RemoteFxBolts)>();
            }
            continue;
        };
        if body_entry.skel.positions.is_empty() || body_entry.skel.bones.is_empty() {
            if remote.is_some() {
                commands
                    .entity(entity)
                    .remove::<(RemotePlayer, RemoteFxBolts)>();
            }
            continue;
        }

        let origin = runtime.origin;
        let yaw = runtime.angles[1];
        let mut pose = Transform {
            translation: Vec3::from_array(origin),
            rotation: Quat::from_rotation_z(yaw.to_radians()),
            scale: Vec3::ONE,
        };
        if skate.active && !skate.bones.is_empty() && client.0 == skate.client {
            pose = Transform::from_matrix(skate.root);
        }
        if jak.active && client.0 == jak.client {
            pose = Transform::from_matrix(jak.root);
        }
        if let Some(puppet) = puppet.as_ref().filter(|p| p.active && client.0 == p.client) {
            pose = Transform::from_matrix(puppet.root);
        }
        let marker = RemotePlayer {
            is_bot: meta.is_some_and(|m| {
                m.name == entity_iw4::pack_client_state_name("bot")
                    || m.name == entity_iw4::pack_client_state_name("dummy")
            }),
            ffa_team,
            client_state_team,
        };
        if remote.is_some() {
            commands.entity(entity).insert((pose, marker));
        } else {
            attach_remote_root(&mut commands, entity, pose, marker);
        }
    }
}

struct PendingBodySkin<'a> {
    skate: Option<&'a frame::SkateMode>,
    jak_board: Option<Mat4>,
    jak_skins: Option<std::sync::Arc<frame::JakSkins>>,
    is_bot: bool,
    persist_key: u32,
    transform: Transform,
    matrices: Vec<Mat4>,
    body: &'a asset_model::BodyMeshEntry,
    head: Option<(&'a asset_model::BodyMeshEntry, usize)>,
    gun: Option<PendingGunSkin<'a>>,
    body_lod: u8,
    head_lod: Option<u8>,
    gun_lod: Option<u8>,

    skin_entries: &'a [dpvs_iw4::SceneEntSkinEntry],
    head_model: Option<u16>,
    gun_model: Option<u16>,
    attachments: Vec<(PendingGunSkin<'a>, u16, Option<u8>)>,

    dest: CpuBodyGeom,
}

enum RemoteSkinAction<'a> {
    Culled,
    ReuseCache,
    Blend(PendingBodySkin<'a>),
}

struct RemotePoseFrame<'a> {
    skate: &'a frame::SkateMode,
    jak: &'a frame::JakMode,
    puppet: Option<&'a frame::InventoryPuppet>,
    script: &'a asset_anim::ParsedPlayerAnimScript,
    tree: &'a asset_anim::CompiledAnimTreeDefinition,
    catalog: &'a asset_anim::XAnimCatalog,
    bodies: &'a PreparedBodies,
    weapons: Option<&'a PreparedWeapons>,
    world_weapons: Option<&'a PreparedWorldWeapons>,
    kits: Option<&'a PreparedRemoteKits>,
    dt: f32,
    eye: Option<Vec3>,
    ramp: LodRampArgs,
    trees: &'a mut RemoteBodyTrees,
    pose_hashes: &'a mut RemoteSkinPoseHashes,
    submit: &'a mut RemoteBodySkinnedQueue,
    dobj_poses: &'a mut crate::anim::dobj_pose::HostDObjPoseFrame,
    posed_players: &'a mut crate::anim::dobj_pose::PosedPlayerFrame,
    scene: &'a render_scene::GfxScene,
    last_cache_hits: &'a HashSet<u32>,
    pending: Vec<PendingBodySkin<'a>>,

    world_gun_gap: Option<WorldGunGap>,
}

struct RemoteBodySceneSlot<'a> {
    hidden: bool,
    surface_count: Option<u32>,
    lods: &'a [i8],
    skin_entries: &'a [dpvs_iw4::SceneEntSkinEntry],
}

fn remote_body_scene_slot(
    scene: &render_scene::GfxScene,
    persist_key: u32,
) -> RemoteBodySceneSlot<'_> {
    RemoteBodySceneSlot {
        hidden: scene.scene_ent_hidden(persist_key),
        surface_count: scene.scene_ent_surface_count(persist_key),
        lods: scene
            .scene_dobj(persist_key)
            .map(|dobj| dobj.lods.as_slice())
            .unwrap_or(&[]),
        skin_entries: scene
            .scene_ent_skinned_surfs(persist_key)
            .map(|surfs| surfs.entries.as_slice())
            .unwrap_or(&[]),
    }
}

fn pose_remote_bodies(
    (skate, jak): (Res<frame::SkateMode>, Res<frame::JakMode>),
    puppet: Option<Res<frame::InventoryPuppet>>,
    time: Res<Time>,
    gaps: Res<RenderPresentationGaps>,
    sources: Option<Res<asset_anim::PlayerAnimSources>>,
    xanims: Option<Res<assets::PreparedXAnims>>,
    bodies: Option<Res<PreparedBodies>>,
    weapons: Option<Res<PreparedWeapons>>,
    world_weapons: Option<Res<PreparedWorldWeapons>>,
    prepared_kits: Res<PreparedRemoteKits>,
    mut trees: ResMut<RemoteBodyTrees>,
    mut pose_hashes: ResMut<RemoteSkinPoseHashes>,
    mut submit: ResMut<RemoteBodySkinnedQueue>,
    dpvs: (
        Query<&GlobalTransform, With<render_scene::FpvLens>>,
        Res<render_scene::LodRampSkinnedDvar>,
        ResMut<crate::anim::dobj_pose::HostDObjPoseFrame>,
        ResMut<crate::anim::dobj_pose::PosedPlayerFrame>,
        Res<HostGfxScene>,
        Option<Res<FrameClock>>,
    ),
    mut roots: Query<
        (
            &CEntity,
            &CEntityRuntime,
            &Transform,
            &mut RemoteFxBolts,
            &RemotePlayer,
        ),
        (
            With<RemotePlayer>,
            Without<WorldScriptModelInstance>,
            Without<render_scene::FpvLens>,
            Without<render_scene::FlyCamera>,
        ),
    >,
) {
    let (cameras, lod_skinned, mut dobj_poses, mut posed_players, gfx, cg_clock) = dpvs;
    submit.clear();
    for (_, _, _, mut bolts, _) in &mut roots {
        *bolts = RemoteFxBolts::default();
    }
    let legs_written = remote_legs_written(roots.iter().map(|(_, runtime, _, _, _)| runtime));
    let leaves_bound = sources
        .as_deref()
        .map(|sources| {
            remote_written_leaves_bound(roots.iter().map(|(_, runtime, _, _, _)| runtime), sources)
        })
        .unwrap_or(false);

    let source_cause = match sources.as_deref() {
        None => Some(RenderGapCause::PlayerAnimSourcesNotPrepared),
        Some(sources) if !sources.decode_errors().is_empty() => {
            Some(RenderGapCause::PlayerAnimSourceDecodeFailed {
                first: sources
                    .decode_errors()
                    .first()
                    .map(|error| format!("{error:?}"))
                    .unwrap_or_default(),
            })
        }
        Some(sources) if sources.multiplayer_atr().is_none() => {
            Some(RenderGapCause::MultiplayerAtrAbsent)
        }
        Some(sources) if sources.playeranim_script().is_none() => {
            Some(RenderGapCause::PlayeranimScriptAbsent)
        }
        Some(sources) => match sources.compiled() {
            None => Some(RenderGapCause::AnimtreeCompilerMissing),
            Some(Err(error)) => Some(RenderGapCause::AnimtreeCompileFailed {
                reason: error.to_string(),
            }),
            Some(Ok(_)) => match sources.parsed_script() {
                Some(Err(error)) => Some(RenderGapCause::AnimScriptParseFailed {
                    reason: error.to_string(),
                }),
                Some(Ok(_)) if !legs_written => Some(RenderGapCause::AnimScriptEvaluatorMissing),
                Some(Ok(_)) if !leaves_bound => Some(RenderGapCause::XAnimLeafBindMissing),
                Some(Ok(_)) => None,
                _ => Some(RenderGapCause::AnimScriptEvaluatorMissing),
            },
        },
    };
    if let Some(cause) = source_cause {
        gaps.raise(cause);
        return;
    }

    let Some(sources) = sources.as_deref() else {
        gaps.raise(RenderGapCause::PlayerAnimSourcesNotPrepared);
        return;
    };
    let Some(Ok(tree)) = sources.compiled() else {
        gaps.raise(RenderGapCause::AnimtreeCompilerMissing);
        return;
    };
    let Some(Ok(script)) = sources.parsed_script() else {
        gaps.raise(RenderGapCause::XAnimCalcFailed {
            reason: "playeranim.script not parsed".into(),
        });
        return;
    };
    let Some(xanims) = xanims.as_deref() else {
        gaps.raise(RenderGapCause::XAnimCalcFailed {
            reason: "PreparedXAnims not installed".into(),
        });
        return;
    };
    let Some(bodies) = bodies.as_deref() else {
        gaps.raise(RenderGapCause::XAnimCalcFailed {
            reason: "PreparedBodies not installed".into(),
        });
        return;
    };

    let eye = cameras.single().ok().map(|xf| xf.translation());
    let skinned_ramp = lod_skinned.args();
    let mut posed_bodies = 0usize;
    let mut scene_hidden_bodies = 0usize;
    let mut last_calc_error = None;
    let mut live = HashSet::new();
    let last_cache_hits = pose_hashes.take_last_cache_hits();
    let mut pose_frame = RemotePoseFrame {
        skate: &skate,
        jak: &jak,
        puppet: puppet.as_deref(),
        script,
        tree,
        catalog: &xanims.0,
        bodies,
        weapons: weapons.as_deref(),
        world_weapons: world_weapons.as_deref(),
        kits: match (weapons.as_deref(), world_weapons.as_deref()) {
            (Some(weapons), Some(world)) if prepared_kits.owned_by(bodies, weapons, world) => {
                Some(&*prepared_kits)
            }
            _ => None,
        },
        dt: cg_clock
            .as_ref()
            .map(|clock| clock.frametime_secs())
            .unwrap_or_else(|| time.delta_secs()),
        eye,
        ramp: skinned_ramp,
        trees: &mut trees,
        pose_hashes: &mut pose_hashes,
        submit: &mut submit,
        dobj_poses: &mut dobj_poses,
        posed_players: &mut posed_players,
        scene: &gfx.scene,
        last_cache_hits: &last_cache_hits,
        pending: Vec::new(),
        world_gun_gap: None,
    };
    for (identity, runtime, transform, mut bolts, remote) in &mut roots {
        let persist_key = u32::from(identity.number());

        live.insert(persist_key);
        let posed = pose_frame.pose_one_remote(identity, runtime, transform, &mut bolts, remote);
        match posed {
            Ok(PoseOneOutcome::Posed) => posed_bodies += 1,
            Ok(PoseOneOutcome::SceneHidden) => scene_hidden_bodies += 1,
            Ok(PoseOneOutcome::NoBodyLod | PoseOneOutcome::NoAnim) => {}
            Err(reason) => last_calc_error = Some(reason),
        }
    }
    pose_frame.trees.retain_live(&live);
    pose_frame.pose_hashes.retain_live(&live);

    match pose_frame.world_gun_gap.take() {
        Some(gap) => gaps.raise(RenderGapCause::RemoteBodyWorldGunMissing {
            weapon: gap.weapon,
            world_model: gap.world_model,
        }),
        None => gaps.clear(RenderGap::RemoteBodyWorldGun),
    }

    if posed_bodies == 0 {
        if scene_hidden_bodies > 0 {
            return;
        }
        gaps.raise(match last_calc_error {
            Some(reason) => RenderGapCause::XAnimCalcFailed { reason },
            None => RenderGapCause::XAnimCalcMissing,
        });
        return;
    }

    let _skin_model = perf::Span::HostSkinModelMs.enter();
    let workers = skin_blend_worker_count(pose_frame.pending.len());
    let pending = std::mem::take(&mut pose_frame.pending);
    let assembled = assemble_meshes_parallel(pending, workers);
    for assembled in assembled {
        if let Ok(assembled) = assembled {
            commit_assembled_body(
                assembled.persist_key,
                &assembled.transform,
                assembled.geom,
                assembled.radii,
                assembled.radius_parents,
                assembled.lods,
                pose_frame.submit,
                pose_frame.pose_hashes,
            );
        }
    }
}

#[derive(Clone, Copy)]
enum PoseOneOutcome {
    Posed,
    SceneHidden,
    NoBodyLod,
    NoAnim,
}

impl<'a> RemotePoseFrame<'a> {
    fn pose_one_remote(
        &mut self,
        identity: &CEntity,
        runtime: &CEntityRuntime,
        transform: &Transform,
        bolts: &mut RemoteFxBolts,
        remote: &RemotePlayer,
    ) -> Result<PoseOneOutcome, String> {
        *bolts = RemoteFxBolts::default();
        let persist_key = u32::from(identity.number());
        let is_corpse = runtime.next_state.e_type == ET_PLAYER_CORPSE
            || runtime.pose_e_type == ET_PLAYER_CORPSE as u8;
        let occupation_tr_time = is_corpse.then_some(runtime.next_state.tr_time);
        if is_corpse {
            let victim = u32::try_from(runtime.next_state.client_num).unwrap_or(0);
            clone_corpse_tree_from_victim(
                self.trees,
                persist_key,
                victim,
                runtime.next_state.tr_time,
            );
        }

        if runtime.previous_pose.is_none() {
            if let Some(slot) = self.trees.get_mut(persist_key) {
                slot.legs_rate_sample.time_ms = 0;
                slot.torso_rate_sample.time_ms = 0;
            }
        }
        let sample = remote_pose_sample(runtime);
        if is_corpse {
            let legs_leaf =
                PlayerAnimValue::from_raw((sample.anim.legs_anim as u16) & PLAYER_ANIM_RAW_MASK)
                    .map(|v| v.effective_index())
                    .filter(|&index| index != 0)
                    .and_then(|index| self.tree.node(index).map(|n| n.name.as_str()));
            perf::corpse(1, None, Some(i64::from(ET_PLAYER_CORPSE)), legs_leaf);
        }
        let Some(legs) = packed_anim(sample.anim.legs_anim) else {
            self.trees.remove(persist_key);
            return Ok(PoseOneOutcome::NoAnim);
        };
        let torso = packed_anim(sample.anim.torso_anim).unwrap_or_else(zero_anim);
        let e_type = if is_corpse {
            ET_PLAYER_CORPSE
        } else {
            ET_PLAYER
        };
        let tree = self.tree;
        let catalog = self.catalog;
        let bodies = self.bodies;
        let weapons = self.weapons;
        let world_weapons = self.world_weapons;
        let kits = self.kits;
        let weapon = sample.weapon;
        let view_pitch_deg = match self.puppet {
            Some(puppet) if puppet.active && persist_key == puppet.client => puppet.pitch,
            _ => sample.view_pitch_deg,
        };
        let prone = sample.prone;
        let crouch = sample.crouch;
        let dt = self.dt;
        let trees = &mut *self.trees;
        let pending = &mut self.pending;
        let pose_hashes = &mut *self.pose_hashes;
        let submit = &mut *self.submit;
        let slot = remote_body_scene_slot(self.scene, persist_key);
        let scene_ent_surface_count = slot.surface_count;
        let skin_entries = slot.skin_entries;
        let slot_lods = slot.lods;
        let scene_ent_hidden = slot.hidden;
        let last_cache_hits = self.last_cache_hits;
        let axis = asset_model::kit_assignment_is_axis(remote.client_state_team, remote.ffa_team);
        let eye = self.eye;
        let ramp = self.ramp;
        let dobj_poses = &mut *self.dobj_poses;
        let posed_players = &mut *self.posed_players;
        let world_gun_gap = &mut self.world_gun_gap;
        let result = (|| {
            let origin = transform.translation.to_array();
            let advanced = advance_remote_tree(
                tree,
                self.script,
                catalog,
                legs,
                torso,
                persist_key,
                dt,
                sample.rate_origin,
                sample.rate_time_ms,
                trees,
            )?;

            if scene_ent_hidden {
                return Ok(PoseOneOutcome::SceneHidden);
            }
            let AdvancedRemoteTree {
                runtime: anim_runtime,
                clips,
                reused: reuse,
            } = advanced;
            let model_set = select_remote_models(bodies, weapons, world_weapons, axis, weapon)?;
            if let Some(gap) = model_set.world_gun_gap.clone() {
                *world_gun_gap = Some(gap);
            }
            let body = model_set.body;
            let Some(lods) = select_remote_lods(&model_set, slot_lods, |skel| {
                skel_camera_lod(skel, origin, eye, ramp)
            }) else {
                return Ok(PoseOneOutcome::NoBodyLod);
            };
            if skip_frozen_corpse_dobj(
                e_type == ET_PLAYER_CORPSE,
                reuse,
                last_cache_hits.contains(&persist_key),
                pose_hashes.has_lods(persist_key, lods.tuple()),
            ) {
                push_cached_surfaces(persist_key, transform, pose_hashes, submit);
                return Ok(PoseOneOutcome::Posed);
            }
            let kit = kits
                .and_then(|kits| kits.get(axis, weapon))
                .ok_or_else(|| format!("remote kit not prepared: axis={axis} weapon={weapon}"))?;
            let prepared_dobj = kit.dobj.as_ref();
            ensure_remote_dobj(&model_set, e_type, persist_key, trees, prepared_dobj)?;
            let dobj = trees
                .get(persist_key)
                .and_then(|slot| slot.dobj.as_deref())
                .expect("composed");
            validate_remote_tracks(dobj, clips.as_ref(), body, &model_set.body_name)?;

            let skating = self.skate.active
                && !self.skate.bones.is_empty()
                && persist_key == self.skate.client;
            let world = if skating {
                crate::skate::rig::pose(dobj, self.skate)
            } else {
                pose_remote_dobj(
                    dobj,
                    anim_runtime,
                    remote_player_controller(is_corpse, view_pitch_deg, prone, crouch),
                )?
            };
            let skin = publish_remote_dobj(
                dobj,
                kit.bolt_bones,
                &world,
                persist_key,
                runtime,
                transform,
                bolts,
                dobj_poses,
            )?;
            if !is_corpse {
                publish_posed_player_head(
                    kit.head_bone,
                    &world,
                    identity,
                    transform,
                    posed_players,
                )?;
            }
            let hash = hash_skin_matrices(&skin);
            let pose_same = pose_hashes.remember_pose_hash(persist_key, hash);

            let skin_models = bind_remote_skin_models(dobj, &model_set)?;
            let lods_reusable = pose_hashes.has_lods(persist_key, lods.tuple());
            let action = remote_skin_action(
                persist_key,
                transform,
                skin,
                skin_entries,
                lods,
                skin_models,
                skin_after_pose(scene_ent_surface_count == Some(0), pose_same, lods_reusable),
            );
            match action {
                RemoteSkinAction::Culled => {}
                RemoteSkinAction::ReuseCache => {
                    push_cached_surfaces(persist_key, transform, pose_hashes, submit);
                }
                RemoteSkinAction::Blend(mut job) => {
                    job.is_bot = remote.is_bot;
                    if skating {
                        job.skate = Some(self.skate);
                        job.gun = None;
                        job.attachments.clear();
                    }
                    if self.jak.active && persist_key == self.jak.client {
                        job.jak_board = self.jak.board;
                        job.jak_skins = self.jak.skins.clone();
                        job.gun = None;
                        job.attachments.clear();
                    }
                    job.dest = take_unique_geom(pose_hashes, persist_key);
                    pending.push(job);
                }
            }
            Ok(PoseOneOutcome::Posed)
        })();
        if let Some(tr_time) = occupation_tr_time
            && let Some(slot) = trees.get_mut(persist_key)
            && slot.occupation_tr_time.is_none()
        {
            slot.occupation_tr_time = Some(tr_time);
        }
        result
    }
}

fn remote_skin_action<'a>(
    persist_key: u32,
    transform: &Transform,
    matrices: Vec<Mat4>,
    skin_entries: &'a [dpvs_iw4::SceneEntSkinEntry],
    lods: RemoteModelLods,
    models: RemoteSkinModels<'a>,
    after_pose: SkinAfterPose,
) -> RemoteSkinAction<'a> {
    match after_pose {
        SkinAfterPose::Culled => RemoteSkinAction::Culled,
        SkinAfterPose::ReuseCache => RemoteSkinAction::ReuseCache,
        SkinAfterPose::Blend => RemoteSkinAction::Blend(PendingBodySkin {
            skate: None,
            jak_board: None,
            jak_skins: None,
            is_bot: false,
            persist_key,
            transform: *transform,
            matrices,
            body: models.body,
            head: models.head,
            gun: models.gun,
            body_lod: lods.body,
            head_lod: lods.head,
            gun_lod: lods.gun,
            skin_entries,
            head_model: models.head_model,
            gun_model: models.gun_model,
            attachments: models
                .attachments
                .into_iter()
                .zip(lods.attachments)
                .map(|((skin, model), lod)| (skin, model, lod))
                .collect(),
            dest: CpuBodyGeom::default(),
        }),
    }
}

fn publish_remote_dobj(
    dobj: &xmodel_runtime::DObj,
    bolt_bones: [Option<u16>; 4],
    world: &[Mat4],
    persist_key: u32,
    runtime: &CEntityRuntime,
    transform: &Transform,
    bolts: &mut RemoteFxBolts,
    dobj_poses: &mut crate::anim::dobj_pose::HostDObjPoseFrame,
) -> Result<Vec<Mat4>, String> {
    dobj_poses
        .publish(
            persist_key,
            runtime.in_next_snap(),
            runtime.next_state.e_flags,
            transform.to_matrix(),
            world,
        )
        .map_err(|error| format!("DObj bone publication failed: {error:?}"))?;
    let target = |bone: Option<u16>| -> Option<fx::FxBoltTarget> {
        let bone = bone?;
        let orientation = dobj_poses.resolve(persist_key, i32::from(bone)).ok()?;
        Some(fx::FxBoltTarget {
            dobj: persist_key,
            bone,
            centity_teleport: fx_iw4::bolt_spawn_teleport_bit(
                persist_key,
                runtime.next_state.e_flags,
            ),
            orientation,
        })
    };
    bolts.flash = target(bolt_bones[0]);
    bolts.brass = target(bolt_bones[1]);
    bolts.knife = target(bolt_bones[2]);
    bolts.laser = target(bolt_bones[3]);
    Ok(dobj.skin_matrices(world))
}

fn publish_posed_player_head(
    head_bone: Option<usize>,
    world: &[Mat4],
    identity: &CEntity,
    transform: &Transform,
    frame: &mut crate::anim::dobj_pose::PosedPlayerFrame,
) -> Result<(), String> {
    let head = match head_bone {
        None => crate::anim::dobj_pose::PosedPlayerHead::NoDObjOrHead,
        Some(index) => {
            let bone = world.get(index).ok_or_else(|| {
                format!(
                    "DObj head index {index} exceeds {} published bones",
                    world.len()
                )
            })?;
            crate::anim::dobj_pose::PosedPlayerHead::Exact(
                (transform.to_matrix() * *bone).transform_point3(Vec3::ZERO),
            )
        }
    };
    frame.publish(crate::anim::dobj_pose::PosedPlayer {
        entnum: identity.number(),
        head,
    });
    Ok(())
}

fn skel_camera_lod(
    skel: &ModelSkel,
    origin: [f32; 3],
    eye: Option<Vec3>,
    ramp: LodRampArgs,
) -> Option<u8> {
    smodel_camera_lod(skel.lod, origin, 1.0, eye, ramp)
}

fn surface_material_name(
    surface_index: usize,
    keys: &[Option<asset_core::MaterialKey>],
    edges: &[assets::AssetEdge<assets::MaterialSpace>],
) -> Option<String> {
    match edges.get(surface_index) {
        Some(assets::AssetEdge::Bound(_)) => Some(keys.get(surface_index)?.as_ref()?.name.clone()),
        _ => None,
    }
}

fn skin_slot_into(
    skel: &ModelSkel,
    skin: &[Mat4],
    base: usize,
    lod: u8,
    model: u16,
    entries: &[dpvs_iw4::SceneEntSkinEntry],
    keys: &[Option<asset_core::MaterialKey>],
    edges: &[assets::AssetEdge<assets::MaterialSpace>],
    geom: &mut CpuBodyGeom,
    skip_empty: bool,
) -> Result<(), String> {
    skin_slot_need(skel, skin, base)?;

    let layout = build_skin_layout(skel, lod, |lod_local| {
        dpvs_iw4::SceneEntSkinEntry::stream_draws(entries, model, lod_local as u16)
    })
    .ok_or_else(|| "skin_model refused the posed skeleton".to_string())?;
    let dest_first = geom.packed.len();
    geom.packed.resize(
        dest_first.saturating_add(layout.dest_vertex_n),
        [0u8; asset_iw4::size::GFX_PACKED_VERTEX],
    );
    skin_packed_into(
        skel,
        |bone| skin[base + bone],
        |surface| stream_lod_surface_rigid(skel, lod, entries, model, surface),
        lod,
        &layout,
        &mut geom.packed[dest_first..dest_first + layout.dest_vertex_n],
    );
    let vert_rebase = dest_first as u32;
    for surf in &layout.surfaces {
        if skip_empty && (!surf.visible || surf.index_count == 0) {
            continue;
        }
        if !surf.visible || surf.index_count == 0 {
            geom.surfaces.push(CpuSurfMeta {
                index_start: 0,
                index_count: 0,
                name: surface_material_name(surf.surface_index, keys, edges),
            });
            continue;
        }
        let index_start = geom.indices.len() as u32;
        let src = surf.index_start as usize;
        let end = src.saturating_add(surf.index_count as usize);
        geom.indices.extend(
            layout.indices[src..end]
                .iter()
                .map(|&index| index.saturating_add(vert_rebase)),
        );
        geom.surfaces.push(CpuSurfMeta {
            index_start,
            index_count: surf.index_count,
            name: surface_material_name(surf.surface_index, keys, edges),
        });
    }
    geom.decoded_n = geom.packed.len();
    Ok(())
}

fn skin_blend_worker_count(jobs: usize) -> usize {
    if jobs == 0 {
        return 1;
    }
    std::thread::available_parallelism()
        .map(|n| n.get().saturating_sub(1).max(1))
        .unwrap_or(1)
        .min(jobs)
}

fn assemble_meshes_parallel(
    jobs: Vec<PendingBodySkin<'_>>,
    workers: usize,
) -> Vec<Result<AssembledMeshes, String>> {
    if jobs.is_empty() {
        return Vec::new();
    }
    if workers <= 1 || jobs.len() == 1 {
        return jobs.into_iter().map(assemble_meshes).collect();
    }
    let chunk_len = jobs.len().div_ceil(workers);
    let mut chunks = Vec::new();
    let mut rest = jobs;
    while !rest.is_empty() {
        let tail = if rest.len() > chunk_len {
            rest.split_off(chunk_len)
        } else {
            Vec::new()
        };
        chunks.push(rest);
        rest = tail;
    }

    ComputeTaskPool::get()
        .scope(|scope| {
            for chunk in chunks {
                scope.spawn(
                    async move { chunk.into_iter().map(assemble_meshes).collect::<Vec<_>>() },
                );
            }
        })
        .into_iter()
        .flatten()
        .collect()
}

struct AssembledMeshes {
    persist_key: u32,
    transform: Transform,
    geom: CpuBodyGeom,
    radii: Vec<f32>,
    radius_parents: Vec<u8>,
    lods: (Option<u8>, Option<u8>, Option<u8>),
}

fn assemble_meshes(job: PendingBodySkin<'_>) -> Result<AssembledMeshes, String> {
    let persist_key = job.persist_key;
    let transform = job.transform;
    let lods = (Some(job.body_lod), job.head_lod, job.gun_lod);
    let mut geom = job.dest;
    if let Some(model) = assets::bot_model::local_bot_model().filter(|_| job.is_bot) {
        super::bot_model::skin(model, &job.body.skel, &job.matrices, &mut geom)?;
    } else if let Some(skins) = &job.jak_skins {
        crate::jak_pose::skin(skins, &mut geom);
    } else {
        skin_slot_into(
            &job.body.skel,
            &job.matrices,
            0,
            job.body_lod,
            0,
            job.skin_entries,
            &job.body.material_keys,
            &job.body.material_edges,
            &mut geom,
            false,
        )?;
        match (job.head, job.head_lod) {
            (None, _) | (Some(_), None) => {}
            (Some((head, base)), Some(lod)) => {
                skin_slot_into(
                    &head.skel,
                    &job.matrices,
                    base,
                    lod,
                    job.head_model.unwrap_or(1),
                    job.skin_entries,
                    &head.material_keys,
                    &head.material_edges,
                    &mut geom,
                    false,
                )?;
            }
        }
    }
    match (&job.gun, job.gun_lod) {
        (None, _) | (Some(_), None) => {}
        (Some(gun), Some(lod)) => {
            skin_slot_into(
                &gun.entry.skel,
                &job.matrices,
                gun.base,
                lod,
                job.gun_model.unwrap_or(0),
                job.skin_entries,
                &gun.entry.material_keys,
                &gun.entry.material_edges,
                &mut geom,
                true,
            )?;
        }
    }
    for (attachment, model, lod) in &job.attachments {
        let Some(lod) = *lod else {
            continue;
        };
        skin_slot_into(
            &attachment.entry.skel,
            &job.matrices,
            attachment.base,
            lod,
            *model,
            job.skin_entries,
            &attachment.entry.material_keys,
            &attachment.entry.material_edges,
            &mut geom,
            true,
        )?;
    }
    if let Some(skate) = job.skate {
        crate::skate::rig::board(skate, &mut geom)?;
    }
    if let Some(board) = job.jak_board.filter(|_| job.jak_skins.is_none()) {
        crate::jak::board_mesh(board, &mut geom);
    }
    let (radii, radius_parents) = radii(
        job.body,
        job.head.map(|(head, _)| head),
        job.gun.as_ref().map(|gun| gun.entry),
    );
    Ok(AssembledMeshes {
        persist_key,
        transform,
        geom,
        radii,
        radius_parents,
        lods,
    })
}

fn enqueue_remote_body_lighting(
    submit: ResMut<RemoteBodySkinnedQueue>,
    mut binds: ResMut<RemoteBodyLightingBinds>,
    atpoint: Res<render_scene::DynAtPointLookup>,
    mut lighting_requests: ResMut<ModelLightingRequests>,
) {
    binds.by_client.clear();
    if submit.is_empty() {
        return;
    }
    for item in submit.iter() {
        let box_half = lighting_box_half(&item.radii, &item.radius_parents);
        // Player origins are at their feet. Sample the posed model's centre
        // so the light-grid lookup is above the ground and follows stance.
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for vertex in &item.geom.packed {
            let p = Vec3::new(
                f32::from_le_bytes(vertex[0..4].try_into().unwrap()),
                f32::from_le_bytes(vertex[4..8].try_into().unwrap()),
                f32::from_le_bytes(vertex[8..12].try_into().unwrap()),
            );
            min = min.min(p);
            max = max.max(p);
        }
        let origin = if min.is_finite() && max.is_finite() {
            item.world_from_local
                .transform_point3((min + max) * 0.5)
                .to_array()
        } else {
            item.origin
        };
        let lookup_fallback = atpoint.fallback(origin, box_half);
        let client = u16::try_from(item.client).unwrap_or(u16::MAX);
        let owner = if assets::bot_model::local_bot_model().is_some_and(|model| {
            item.geom.surfaces.iter().any(|surface| {
                model
                    .surfaces
                    .iter()
                    .any(|custom| surface.name.as_deref() == Some(custom.material.as_str()))
            })
        }) {
            ModelLightingOwner::LocalBotOverride(client)
        } else {
            ModelLightingOwner::RemoteClient(client)
        };
        binds.by_client.insert(
            item.client,
            lighting_requests.request(ModelLightingRequest {
                owner,
                origin,
                lookup_fallback,
            }),
        );
    }
}

fn submit_remote_bodies(
    mut plan: ResMut<RemoteBodyDrawPlan>,
    gaps: Res<RenderPresentationGaps>,
    mut submit: ResMut<RemoteBodySkinnedQueue>,
    mut binds: ResMut<RemoteBodyLightingBinds>,
    resolved: Res<ResolvedModelLightingTable>,
    atlas: Option<Res<WorldModelLightingAtlas>>,
    tess: Option<Res<render_scene::TessMaterials>>,
    model_materials: Res<crate::anim::model_materials::PreparedModelMaterials>,
    mut last_catalog: Local<Option<Arc<render_material::RuntimeMaterialCatalog>>>,
) {
    let items = submit.take();
    let binds = std::mem::take(&mut binds.by_client);
    if items.is_empty() {
        if plan.decoded_n != 0 || plan.last_packed_id.is_some() {
            plan.clear_geometry();
        }
        *last_catalog = None;
        return;
    }
    if atlas.is_none() {
        plan.clear_geometry();
        *last_catalog = None;
        gaps.raise(RenderGapCause::RemoteBodyLightingAllocFailed);
        return;
    }
    let Some(tess) = tess else {
        plan.clear_geometry();
        *last_catalog = None;
        gaps.raise(RenderGapCause::RemoteBodyLightingAllocFailed);
        return;
    };
    if !last_catalog
        .as_ref()
        .is_some_and(|old| Arc::ptr_eq(old, &tess.catalog))
    {
        plan.clear_geometry();
        *last_catalog = Some(Arc::clone(&tess.catalog));
    }

    let mut last_cause = None;
    let mut seated = Vec::with_capacity(items.len());
    for item in items {
        let seated_light = binds
            .get(&item.client)
            .and_then(|request| resolved.get(request.owner));
        let Some((handle, scene_light, probe)) = (match seated_light {
            Some(ResolvedModelLighting::Seated {
                handle,
                scene_light_index,
                reflection_probe_index,
                ..
            }) => Some((handle, scene_light_index, reflection_probe_index)),
            Some(ResolvedModelLighting::Failed) | None => {
                last_cause = Some(RenderGapCause::RemoteBodyLightingAllocFailed);
                None
            }
        }) else {
            continue;
        };
        seated.push((item, handle, scene_light, probe));
    }
    if seated.is_empty() {
        if plan.decoded_n != 0 || plan.last_packed_id.is_some() {
            plan.clear_geometry();
        }
        *last_catalog = None;
        gaps.raise(last_cause.unwrap_or(RenderGapCause::RemoteBodySubmitMissing));
        return;
    }

    let packed_id = seated_packed_id(&seated);
    let draw_id = seated_draw_id(&seated);
    if plan.last_packed_id == Some(packed_id) && seated_clients_match(&plan, &seated) {
        if plan.last_draw_id != Some(draw_id) {
            refresh_remote_body_draws(&mut plan, &seated);
            plan.revisions.bump_draws();
            plan.revision = plan.revision.wrapping_add(1);
            plan.last_draw_id = Some(draw_id);
        }
        gaps.clear(RenderGap::RemoteBodyAnimation);
        return;
    }

    plan.clear_geometry();
    let vert_n: usize = seated
        .iter()
        .map(|(item, _, _, _)| item.geom.decoded_n)
        .sum();
    let idx_n: usize = seated
        .iter()
        .map(|(item, _, _, _)| item.geom.indices.len())
        .sum();
    plan.indices.reserve(idx_n);
    let mut session = take_body_packed_session(&mut plan);
    session.reserve_packed(vert_n);
    let mut mat_by_name: HashMap<String, u32> = HashMap::new();
    let mut submitted_verts = 0usize;
    let mut any_missing_material = false;
    for (item, handle, scene_light, probe) in seated {
        let before = plan.decoded_n;
        let appended = append_remote_body_cpu_blob(
            &mut plan,
            &mut session,
            &item.geom.packed,
            &item.geom.indices,
            item.geom.decoded_n,
        );
        let index_base = appended.map(|(_, index_base)| index_base);
        for surface in item.geom.surfaces.iter() {
            let Some(name) = surface.name.as_deref() else {
                any_missing_material = true;
                last_cause = Some(RenderGapCause::RemoteBodyMaterialMissing {
                    name: String::new(),
                });
                continue;
            };
            let mat_idx = if let Some(&idx) = mat_by_name.get(name) {
                idx
            } else {
                let Some(material) = model_materials.material(&tess.catalog, name).cloned() else {
                    any_missing_material = true;
                    last_cause = Some(RenderGapCause::RemoteBodyMaterialMissing {
                        name: name.to_owned(),
                    });
                    continue;
                };
                let idx = plan.materials.len() as u32;
                plan.materials.push(material);
                mat_by_name.insert(name.to_owned(), idx);
                idx
            };
            if let Some(index_base) = index_base {
                push_remote_body_cpu_draw(
                    &mut plan,
                    index_base.saturating_add(surface.index_start),
                    surface.index_count,
                    item.world_from_local,
                    handle,
                    scene_light,
                    probe,
                    mat_idx,
                    Some(item.client),
                );
            }
        }
        if plan.decoded_n > before {
            submitted_verts += plan.decoded_n - before;
        }
    }
    install_body_packed_session(&mut plan, session);
    if submitted_verts > 0 && !any_missing_material {
        plan.revisions.bump_surfaces();
        plan.revisions.bump_draws();
        let topology = topology_fingerprint(&plan.indices, &plan.surface_ranges, plan.decoded_n);
        if plan.revisions.topology != topology {
            plan.revisions.topology = topology;
        }
        plan.revision = plan.revision.wrapping_add(1);
        plan.last_packed_id = Some(packed_id);
        plan.last_draw_id = Some(draw_id);
        gaps.clear(RenderGap::RemoteBodyAnimation);
        return;
    }
    gaps.raise(last_cause.unwrap_or(RenderGapCause::RemoteBodySubmitMissing));
}

fn seated_packed_id(seated: &[(RemoteBodySkinnedItem, u32, u8, u8)]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    seated.len().hash(&mut hasher);
    for (item, _, _, _) in seated {
        item.client.hash(&mut hasher);
        item.geometry_revision.hash(&mut hasher);
        item.geom.decoded_n.hash(&mut hasher);
    }
    hasher.finish()
}

fn seated_draw_id(seated: &[(RemoteBodySkinnedItem, u32, u8, u8)]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    seated.len().hash(&mut hasher);
    for (item, handle, scene_light, probe) in seated {
        item.client.hash(&mut hasher);
        for lane in item.world_from_local.to_cols_array() {
            lane.to_bits().hash(&mut hasher);
        }
        handle.hash(&mut hasher);
        scene_light.hash(&mut hasher);
        probe.hash(&mut hasher);
    }
    hasher.finish()
}

fn seated_clients_match(
    plan: &RemoteBodyDrawPlan,
    seated: &[(RemoteBodySkinnedItem, u32, u8, u8)],
) -> bool {
    let mut live: Vec<u32> = seated.iter().map(|(item, _, _, _)| item.client).collect();
    let mut drawn: Vec<u32> = plan.draws.iter().filter_map(|d| d.scene_entnum).collect();
    live.sort_unstable();
    live.dedup();
    drawn.sort_unstable();
    drawn.dedup();
    live == drawn
}

fn refresh_remote_body_draws(
    plan: &mut RemoteBodyDrawPlan,
    seated: &[(RemoteBodySkinnedItem, u32, u8, u8)],
) {
    for draw in &mut plan.draws {
        let Some(ent) = draw.scene_entnum else {
            continue;
        };
        let Some((_, handle, scene_light, probe, world)) =
            seated
                .iter()
                .find_map(|(item, handle, scene_light, probe)| {
                    (item.client == ent).then_some((
                        item,
                        *handle,
                        *scene_light,
                        *probe,
                        item.world_from_local,
                    ))
                })
        else {
            continue;
        };
        draw.world_from_local = world;
        draw.lighting_handle = handle;
        draw.scene_light_index = scene_light;
        draw.reflection_probe_index = probe;
    }
}

fn remote_legs_written<'a>(mut runtimes: impl Iterator<Item = &'a CEntityRuntime>) -> bool {
    runtimes.any(|runtime| {
        let sample = remote_pose_sample(runtime);
        PlayerAnimValue::from_raw((sample.anim.legs_anim as u16) & PLAYER_ANIM_RAW_MASK)
            .is_some_and(|value| value.effective_index() != 0)
    })
}

fn remote_written_leaves_bound<'a>(
    runtimes: impl Iterator<Item = &'a CEntityRuntime>,
    sources: &asset_anim::PlayerAnimSources,
) -> bool {
    let Some(binds) = sources.leaf_binds() else {
        return false;
    };
    let mut any = false;
    for runtime in runtimes {
        let sample = remote_pose_sample(runtime);
        let Some(value) =
            PlayerAnimValue::from_raw((sample.anim.legs_anim as u16) & PLAYER_ANIM_RAW_MASK)
        else {
            continue;
        };
        let index = value.effective_index();
        if index == 0 {
            continue;
        }
        any = true;
        if !binds.is_bound(index) {
            return false;
        }
        let Some(torso) =
            PlayerAnimValue::from_raw((sample.anim.torso_anim as u16) & PLAYER_ANIM_RAW_MASK)
        else {
            continue;
        };
        let torso_index = torso.effective_index();
        if torso_index != 0 && !binds.is_bound(torso_index) {
            return false;
        }
    }
    any
}

fn attach_remote_root(
    commands: &mut Commands,
    entity: Entity,
    transform: Transform,
    remote: RemotePlayer,
) {
    commands
        .entity(entity)
        .insert((remote, transform, Visibility::default()));
}
