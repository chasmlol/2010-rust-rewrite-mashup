use bevy::prelude::*;

use crate::anim::scene_submission::{AnimDObjSceneSkels, AnimDObjSceneSubmission, AnimSceneSubmit};
use crate::anim::xmodel_pose::PosedModelSurface;
use crate::occupancy::script_model::pose_script_dobj_with_materials;
use crate::{
    ItemAssetDraw, ItemDrawPlan, ItemOwnerDraw, XMODEL_OBJECT_ID_ITEM_BASE, append_item_surfaces,
    lighting_box_half, stamp_plan_geometry, topology_fingerprint,
};
use anim_iw4::DOBJ_RADIUS_PARENT_ROOT;
use entity_iw4::{ET_ITEM, Trajectory, evaluate_trajectory};
use playerstate_iw4::PERK_SCAVENGER;
use render_scene::{
    HostGfxScene, ModelLightingOwner, ModelLightingRequest, ModelLightingRequests,
    SmodelPassMaterial, TessMaterials, WorldModelLightingAtlas, WorldPresentFacts,
    XModelSurfaceDraw, scene_quat_from_angles,
};

pub const ITEM_LIGHTING_Z_OFS: f32 = 4.0;

use crate::anim::world_weapon::ItemComposition;
pub(crate) use crate::anim::world_weapon::PreparedItemCompositions;

#[derive(Resource, Default)]
struct ItemOccupancy {
    rows: Vec<OccupiedItem>,
}

struct OccupiedItem {
    index: usize,
    entnum: i32,
    origin: [f32; 3],
    angles: [f32; 3],
    composition: std::sync::Arc<ItemComposition>,
    lighting_origin: [f32; 3],
}

#[derive(Resource, Default)]
struct ItemPoseProduct {
    assets: Vec<ItemPosedAsset>,
    owners: Vec<ItemPosedOwner>,
}

struct ItemPosedAsset {
    key: String,
    models: Vec<assets::WorldWeaponIndex>,
    surfaces: Vec<PosedModelSurface>,
}

struct ItemPosedOwner {
    asset_index: usize,
    index: usize,
    entnum: u32,
    origin: [f32; 3],
    angles: [f32; 3],
    name: String,
    model: assets::WorldWeaponIndex,
    lighting_origin: [f32; 3],
}

struct ItemSceneSlot<'a> {
    hidden: bool,
    skin_entries: &'a [dpvs_iw4::SceneEntSkinEntry],
}

fn item_scene_slot(scene: &render_scene::GfxScene, entnum: u32) -> ItemSceneSlot<'_> {
    ItemSceneSlot {
        hidden: scene.scene_ent_skips_draw(entnum),
        skin_entries: scene
            .scene_ent_skinned_surfs(entnum)
            .map(|surfs| surfs.entries.as_slice())
            .unwrap_or(&[]),
    }
}

pub fn register_item_systems(app: &mut App) {
    app.init_resource::<ItemDrawPlan>()
        .init_resource::<ItemOccupancy>()
        .init_resource::<PreparedItemCompositions>()
        .init_resource::<ItemPoseProduct>()
        .add_systems(
            Update,
            occupy_item_scene_ents
                .in_set(frame::RenderSet::Anim)
                .before(frame::WorkerCmdSet::CellDynModel)
                .in_set(render_scene::GfxSceneAdd)
                .in_set(AnimSceneSubmit),
        )
        .add_systems(
            Update,
            (pose_items, append_item_draws)
                .chain()
                .after(frame::WorkerCmdSet::CellSceneEnt)
                .after(frame::WorkerCmdSet::DpvsEnt)
                .before(frame::WorkerCmdSet::CellDynModel)
                .in_set(frame::WorkerCmdSet::SkinModel),
        );
}

pub fn item_lighting_origin(origin: [f32; 3]) -> [f32; 3] {
    [origin[0], origin[1], origin[2] + ITEM_LIGHTING_Z_OFS]
}

fn item_weapon_index(index: i32) -> Option<u32> {
    u32::try_from(index).ok().filter(|&weapon| weapon != 0)
}

fn item_world_from_local(origin: [f32; 3], angles: [f32; 3]) -> Mat4 {
    let [pitch, yaw, roll] = angles;
    Transform {
        translation: Vec3::from_array(origin),
        rotation: Quat::from_euler(
            EulerRot::ZYX,
            yaw.to_radians(),
            pitch.to_radians(),
            roll.to_radians(),
        ),
        scale: Vec3::ONE,
    }
    .to_matrix()
}

fn item_is_scavenger(snapshot: &sim::Snapshot, entnum: i32) -> bool {
    snapshot
        .meta
        .item_ammo
        .iter()
        .any(|row| row.entnum == entnum && row.scavenger != 0)
}

fn local_has_scavenger_perk(
    presented: Option<&net::PresentedSnapshot>,
    local: Option<&net::LocalPresentClient>,
) -> bool {
    let Some(id) = local else {
        return false;
    };
    presented
        .and_then(|p| p.player(id.0))
        .is_some_and(|ps| (ps.perks[0] & PERK_SCAVENGER) != 0)
}

fn occupy_item_scene_ents(
    presented: Option<Res<net::PresentedSnapshot>>,
    local_client: Option<Res<net::LocalPresentClient>>,
    weapons: Option<Res<assets::PreparedWeapons>>,
    world_weapons: Option<Res<assets::PreparedWorldWeapons>>,
    mut compositions: ResMut<PreparedItemCompositions>,
    mut occupancy: ResMut<ItemOccupancy>,
    mut scene_skels: ResMut<AnimDObjSceneSkels>,
    mut scene_submissions: MessageWriter<AnimDObjSceneSubmission>,
    cg_clock: Option<Res<net::FrameClock>>,
) {
    occupancy.rows.clear();
    let (Some(weapons), Some(world)) = (weapons.as_deref(), world_weapons.as_deref()) else {
        compositions.clear();
        return;
    };
    compositions.reset_for(weapons, world);
    let catalog = &world.0;

    let Some(presented_inner) = presented.as_deref() else {
        return;
    };
    let Some(snapshot) = presented_inner.snapshot() else {
        return;
    };
    let hide_scavenger = !local_has_scavenger_perk(Some(presented_inner), local_client.as_deref());
    let at_time = cg_clock
        .as_ref()
        .filter(|clock| clock.started())
        .map(|clock| clock.time())
        .unwrap_or_else(|| sim::level_time_ms(snapshot.tick));
    let items: Vec<_> = snapshot
        .meta
        .entities
        .iter()
        .filter(|es| es.e_type == ET_ITEM)
        .collect();
    if catalog.is_empty() {
        return;
    }

    for (index, es) in items.iter().enumerate() {
        let Some(weapon) = item_weapon_index(es.index) else {
            continue;
        };
        if es.e_flags & 0x20 != 0 {
            continue;
        }
        if hide_scavenger && item_is_scavenger(snapshot, es.number) {
            continue;
        }
        let Some(handle) = weapons
            .for_snapshot(presented_inner.weapon_epoch())
            .ok()
            .and_then(|owner| owner.row(weapon))
            .map(|weapon| weapon.handle())
        else {
            continue;
        };
        let Ok(composition) = compositions.prepare(weapons, world, handle, 0) else {
            continue;
        };
        let Some(entry) = catalog.get_at(composition.model.order()) else {
            continue;
        };
        let mut models = vec![scene_skels.shared(&composition.name, &entry.skel, 0)];
        models.extend(composition.attachments.iter().filter_map(|index| {
            let attachment = catalog.get_at(index.order())?;
            Some(scene_skels.shared(attachment.skel.name.as_str(), &attachment.skel, 0))
        }));
        let origin = evaluate_origin(es, at_time);
        let lighting_origin = item_lighting_origin(origin);
        let apos = Trajectory {
            tr_time: es.apos_tr_time,
            tr_type: es.apos_tr_type,
            tr_duration: es.apos_tr_duration,
            tr_delta: es.apos_tr_delta,
            tr_base: es.apos_tr_base,
        };
        let angles = evaluate_trajectory(&apos, at_time);
        let entnum = u32::try_from(es.number).unwrap_or(0);
        scene_submissions.write(AnimDObjSceneSubmission {
            render_fx_flags: 0,
            has_tree: false,
            origin,
            lighting_origin,
            radius: entry.skel.radius,
            entnum,
            quat: Some(scene_quat_from_angles(angles)),
            occupy_model_n: u8::try_from(models.len()).unwrap_or(u8::MAX),
            models,
            hide_part_bits: [0; 6],
            store_skin: true,
        });
        occupancy.rows.push(OccupiedItem {
            index,
            entnum: es.number,
            origin,
            angles,
            composition,
            lighting_origin,
        });
    }
}

fn pose_items(
    occupancy: Res<ItemOccupancy>,
    weapons: Option<Res<assets::PreparedWeapons>>,
    world_weapons: Option<Res<assets::PreparedWorldWeapons>>,
    gfx: Res<HostGfxScene>,
    mut product: ResMut<ItemPoseProduct>,
) {
    product.owners.clear();
    if weapons.as_ref().is_some_and(|weapons| weapons.is_changed())
        || world_weapons
            .as_ref()
            .is_some_and(|value| value.is_changed())
    {
        product.assets.clear();
    }
    let catalog = world_weapons
        .as_ref()
        .map(|prepared| &prepared.0)
        .filter(|c| !c.is_empty());
    let Some(catalog) = catalog else {
        product.assets.clear();
        return;
    };
    for row in &occupancy.rows {
        let entnum = u32::try_from(row.entnum).unwrap_or(0);
        let slot = item_scene_slot(&gfx.scene, entnum);
        if slot.hidden {
            continue;
        }
        let composition = &row.composition;
        if weapons
            .as_deref()
            .zip(world_weapons.as_deref())
            .is_none_or(|(w, c)| !composition.owned_by(w, c))
        {
            continue;
        }
        let asset_index = if let Some(index) = product
            .assets
            .iter()
            .position(|asset| asset.key == composition.key)
        {
            index
        } else {
            let mut skels = Vec::with_capacity(1 + composition.attachments.len());
            let mut model_indices = Vec::with_capacity(1 + composition.attachments.len());
            for index in std::iter::once(&composition.model).chain(&composition.attachments) {
                let Some(entry) = catalog.get_at(index.order()) else {
                    break;
                };
                skels.push(&*entry.skel);
                model_indices.push(*index);
            }
            if skels.len() != 1 + composition.attachments.len() {
                continue;
            }
            let dobj = &composition.dobj;
            let dobj_state =
                xmodel_runtime::DObjSemanticState::bind_pose(composition.name.clone(), 1, 1);
            let Ok(request) = dobj_state.resolve_request(|_| None) else {
                continue;
            };
            let Some((surfaces, _)) = pose_script_dobj_with_materials(
                None,
                &skels,
                dobj,
                &request,
                None,
                slot.skin_entries,
            ) else {
                continue;
            };
            let index = product.assets.len();
            product.assets.push(ItemPosedAsset {
                key: composition.key.clone(),
                models: model_indices,
                surfaces,
            });
            index
        };
        product.owners.push(ItemPosedOwner {
            asset_index,
            index: row.index,
            entnum,
            origin: row.origin,
            angles: row.angles,
            name: composition.name.clone(),
            model: composition.model,
            lighting_origin: row.lighting_origin,
        });
    }
}

fn append_item_draws(
    product: Res<ItemPoseProduct>,
    world_weapons: Option<Res<assets::PreparedWorldWeapons>>,
    atlas: Option<Res<WorldModelLightingAtlas>>,
    atpoint: Res<render_scene::DynAtPointLookup>,
    tess: Option<Res<TessMaterials>>,
    facts: Res<WorldPresentFacts>,
    mut plan: ResMut<ItemDrawPlan>,
    mut lighting_requests: ResMut<ModelLightingRequests>,
    model_materials: Res<crate::anim::model_materials::PreparedModelMaterials>,
    mut last_material_generation: Local<Option<render_material::MaterialGenerationId>>,
    mut draws: Local<Vec<XModelSurfaceDraw>>,
    mut owners: Local<Vec<ItemOwnerDraw>>,
) {
    draws.clear();
    owners.clear();
    let catalog = world_weapons
        .as_ref()
        .map(|prepared| &prepared.0)
        .filter(|c| !c.is_empty());
    let atlas_ref = atlas.as_deref();
    if catalog.is_none() || atlas_ref.is_none() || !facts.spawned || tess.is_none() {
        let (revision, generation) = (plan.revision, plan.generation);
        *plan = ItemDrawPlan::default();
        plan.revision = revision;
        plan.generation = generation;
        plan.publish_no_rows();
        *last_material_generation = None;
        return;
    }
    let world_weapons_changed = world_weapons
        .as_ref()
        .is_some_and(|value| value.is_changed());
    let atlas_changed = atlas.as_ref().is_some_and(|value| value.is_changed());
    let tess_changed = tess.as_ref().is_some_and(|value| value.is_changed());
    let (Some(catalog), Some(_), Some(tess)) = (catalog, atlas_ref, tess.as_deref()) else {
        unreachable!("required item draw resources checked above");
    };
    let material_generation = tess.catalog().generation_id();
    let generation_changed = world_weapons_changed
        || atlas_changed
        || tess_changed
        || *last_material_generation != Some(material_generation);
    if generation_changed {
        let revision = plan.revision.wrapping_add(1);
        let generation = plan.generation.wrapping_add(1);
        *plan = ItemDrawPlan::default();
        plan.revision = revision;
        plan.generation = generation;
        plan.revisions.bump_admission();
        *last_material_generation = Some(material_generation);
    }
    for row in &product.owners {
        let posed = &product.assets[row.asset_index];
        let asset_index = if let Some(index) = plan
            .assets
            .iter()
            .position(|asset| asset.model == posed.key)
        {
            index
        } else {
            let materials: Vec<Option<SmodelPassMaterial>> = posed
                .surfaces
                .iter()
                .map(|surface| {
                    let entry =
                        catalog.get_at(posed.models.get(usize::from(surface.model))?.order())?;
                    if !entry.material_edges.get(surface.surface_index)?.is_bound() {
                        return None;
                    }
                    let key = entry.material_keys.get(surface.surface_index)?.as_ref()?;
                    model_materials.material(&tess.catalog(), key).cloned()
                })
                .collect();
            if materials.iter().all(Option::is_none) {
                continue;
            }
            let surfaces = append_item_surfaces(&mut plan, &posed.surfaces, &materials);
            if surfaces.is_empty() {
                continue;
            }
            let index = plan.assets.len();
            plan.assets.push(ItemAssetDraw {
                model: posed.key.clone(),
                surfaces,
            });
            index
        };
        let Some(entry) = catalog.get_at(row.model.order()) else {
            continue;
        };
        let box_half = entry
            .skel
            .radius
            .and_then(|radius| lighting_box_half(&[radius], &[DOBJ_RADIUS_PARENT_ROOT]));
        let lookup_fallback = atpoint.fallback(row.lighting_origin, box_half);
        let pending_lighting = Some(lighting_requests.request(ModelLightingRequest {
            owner: ModelLightingOwner::Item(row.entnum),
            origin: row.lighting_origin,
            lookup_fallback,
        }));
        let object_id = XMODEL_OBJECT_ID_ITEM_BASE.saturating_add(row.index as u16);
        owners.push(ItemOwnerDraw {
            object_id,
            model: row.name.clone(),
        });
        let world_from_local = item_world_from_local(row.origin, row.angles);
        let caster_bound = entry
            .skel
            .radius
            .map(|radius| render_scene::XModelCasterBound {
                origin: row.origin,
                radius: radius.max(1.0),
            });
        for &(surface, material) in &plan.assets[asset_index].surfaces {
            draws.push(XModelSurfaceDraw {
                surface,
                material,
                world_from_local,
                lighting_handle: 0,
                pending_lighting,
                colour_refusal: None,
                object_id,
                scene_light_index: 0,
                reflection_probe_index: 0,
                packed_lighting: None,
                is_scope: false,
                scene_entnum: Some(row.entnum),
                body_client: None,
                caster_bound,
            });
        }
        perf::item(entity_iw4::ET_ITEM, None, None, Some("posed"));
    }
    if !draws.is_empty() {
        let topology =
            topology_fingerprint(&plan.indices, &plan.surface_ranges, plan.vertices.len());
        let rev = plan.revision;
        plan.revision = stamp_plan_geometry(&mut plan.revisions, rev, topology);
    }
    plan.publish_frame_rows(&mut draws, &mut owners);
}

fn evaluate_origin(es: &entity_iw4::EntityState, at_time: i32) -> [f32; 3] {
    let traj = Trajectory {
        tr_time: es.tr_time,
        tr_type: es.tr_type,
        tr_duration: es.tr_duration,
        tr_delta: es.tr_delta,
        tr_base: es.tr_base,
    };
    evaluate_trajectory(&traj, at_time)
}
