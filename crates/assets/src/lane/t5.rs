use std::path::Path;

use super::helpers::{
    decode_reflection_probes, report_dpvs, report_ffa_spawns, report_intermission,
    report_map_models, report_world_batches, smodel_lighting_samples,
};
use super::{
    CommonCensus, CommonWalkSink, LoadedWorld, MaterialPopulation, MaterialPopulationSink,
    ZoneLane, ZoneWalkSink,
};
use crate::lane_capability::{LaneStatus, PreparedCapability};
use crate::session_load::PreparedWorld;
use asset_core::ZoneGame;
use asset_material::decode_material_color_maps;
use asset_transport::progress::{LoadProgress, StageId};
use asset_transport::{T5ZoneMemory, ZoneImage};
use asset_world::{
    MASK_PLAYER_SOLID, WorldDrawPolicy, build_t5_clip_collision, build_t5_world_draw,
    dm_spawn_points_t5, intermission_view_t5, minimap_corners_t5,
};

pub struct T5Lane;

impl T5Lane {
    pub const GAME: ZoneGame = ZoneGame::T5;
    pub const CAPABILITIES: &'static [(PreparedCapability, LaneStatus)] = &[
        (PreparedCapability::Envelope, LaneStatus::SupportedPopulated),
        (
            PreparedCapability::PreparedWorld,
            LaneStatus::SupportedPopulated,
        ),
        (
            PreparedCapability::CollisionSpawns,
            LaneStatus::SupportedPopulated,
        ),
        (
            PreparedCapability::WeaponCatalog,
            LaneStatus::MissingEvidence,
        ),
        (
            PreparedCapability::BodySkeleton,
            LaneStatus::MissingEvidence,
        ),
        (
            PreparedCapability::PlayableFfa,
            LaneStatus::UnsupportedByRuntimeProfile,
        ),
    ];
}

impl ZoneLane for T5Lane {
    fn game(&self) -> ZoneGame {
        Self::GAME
    }

    fn capabilities(&self) -> &'static [(PreparedCapability, LaneStatus)] {
        Self::CAPABILITIES
    }

    fn load_world(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,
        _shared_surfaces: asset_model::SharedXModelSurfaces,
        material_seed: asset_material::MaterialCatalog,
        _common_film_visions: &super::FilmVisionCatalog,
    ) -> LoadedWorld {
        let mut report = vec![format!("game: T5 ({})", path.display())];
        let stage = progress.begin_scoped(StageId::MapAssets, "header", None);
        let header = match image.t5_header() {
            Ok(h) => {
                stage.done();
                h
            }
            Err(e) => {
                stage.fail();
                return LoadedWorld::with_gap(
                    WorldDrawPolicy::t5(),
                    PreparedCapability::PreparedWorld,
                    format!("T5 zone header: {e}"),
                    Some("assets::lane::t5::load_world/zone_header"),
                );
            }
        };

        let stage = progress.begin_scoped(StageId::MapAssets, "memory", None);
        report.push(asset_transport::xfile_arena_row(
            "zone arenas map",
            &header.block_size,
            fastfile_t5::XFILE_BLOCK_TEMP,
            fastfile_t5::XFILE_BLOCK_VIRTUAL,
        ));
        let mut memory = T5ZoneMemory::for_header(&header);
        let mut stream = match memory.stream(&image.bytes) {
            Ok(s) => {
                stage.done();
                s
            }
            Err(e) => {
                stage.fail();
                return LoadedWorld::with_gap(
                    WorldDrawPolicy::t5(),
                    PreparedCapability::PreparedWorld,
                    format!("T5 zone arenas: {e}"),
                    Some("assets::lane::t5::load_world/zone_arenas"),
                );
            }
        };

        let stage = progress.begin_scoped(StageId::MapAssets, "walk", None);
        let mut sink = ZoneWalkSink::default();
        let seeded_techsets = material_seed.technique_set_facts().to_vec();
        sink.seed_materials(material_seed);
        sink.set_capture_zone(asset_core::ZoneOwner::from_zone_path(path));
        sink.set_capture_ns(asset_core::AssetNamespace::T5);
        sink.sound = Some(asset_audio::ZoneSoundCapture::for_map(
            path,
            asset_audio::ZoneGame::T5,
            "map",
        ));
        let walked = fastfile_t5::load_zone(&mut stream, &mut sink);
        let mut map_sound = sink
            .sound
            .take()
            .map(|sound| sound.finish(walked.as_ref().map(|_| ()).map_err(|e| e.to_string())));
        if let Some(Ok(catalog)) = map_sound.as_mut() {
            report.push(absorb_localized_map_sound(path, catalog));
        }
        match &walked {
            Ok(_) => report.push(format!("zone walk: complete, {} assets", sink.walked)),
            Err(e) => report.push(format!(
                "zone walk: stopped after {} assets — {e}",
                sink.walked
            )),
        }
        stage.set_completed(sink.walked as u64);
        report.push(format!(
            "pointer drift: {} unsettled offsets",
            stream.unsettled_offsets()
        ));
        let runtime_overrun = stream.block_overrun(fastfile_t5::XFILE_BLOCK_RUNTIME as u8);
        report.push(format!("block overrun: runtime +{runtime_overrun} bytes"));

        let clip = if let Some(geometry) = stream.clip_map() {
            report.push(format!(
                "clipmap: planes={} brushes={} leaves={} nodes={} cmodels={} verts={} tris={}",
                geometry.plane_count,
                geometry.brush_count,
                geometry.leaf_count,
                geometry.node_count,
                geometry.cmodel_count,
                geometry.vert_count,
                geometry.tri_count
            ));
            match build_t5_clip_collision(&stream, geometry).and_then(|mut clip| {
                sink.map_xmodels
                    .attach_t5_clip_models(&stream, geometry, &mut clip)?;
                Ok(clip)
            }) {
                Ok(clip) => {
                    let solid = clip
                        .brushes
                        .iter()
                        .filter(|b| b.contents & MASK_PLAYER_SOLID != 0)
                        .count();
                    let dropped = geometry.brush_count.saturating_sub(clip.brushes.len());
                    report.push(format!(
                        "clip brushes extracted: {} ({} player-solid by contents mask)",
                        clip.brushes.len(),
                        solid
                    ));
                    if dropped > 0 {
                        report.push(format!(
                            "t5 clip: dropped {dropped} map-sized player-solid volumes \
                             (IW4 pmove startsolid inside the playable box)"
                        ));
                    }
                    report.push(format!(
                        "t5 clip mesh: verts={} tris={} (walk verts={} tris={})",
                        clip.mesh.verts.len(),
                        clip.mesh.tri_indices.len() / 3,
                        geometry.vert_count,
                        geometry.tri_count
                    ));
                    Some(clip)
                }
                Err(e) => {
                    report.push(format!("clip brushes: extract failed — {e}"));
                    None
                }
            }
        } else {
            report.push("clipmap: not reached".into());
            None
        };

        let exp_fog = sink.exp_fog;
        let createart_name = sink.createart_name.clone();
        let t5_teamset = sink.t5_teamset.clone();
        let script_sound = std::mem::take(&mut sink.script_sound).finish();
        let mut scripts = std::mem::take(&mut sink.scripts);
        if let Some(entities) = asset_world::map_ents_entity_string_t5(&stream) {
            scripts.set_entities(entities.to_owned());
        }
        match (&createart_name, exp_fog) {
            (Some(name), Some(fog)) => report.push(format!(
                "t5 createart: {name} fog=ready start={:.1} half={:.1}",
                fog.start_dist, fog.halfway_dist
            )),
            (Some(name), None) => {
                report.push(format!("t5 createart: {name} fog=unparsed"));
            }
            (None, _) => report
                .push("t5 createart: none (RawFile not captured or walk aborted first)".into()),
        }
        report.push(match t5_teamset.as_deref() {
            Some(name) => format!("t5 teamset: {name} (map GSC _teamset_*::level_init)"),
            None => {
                "t5 teamset: none (map GSC missing, packed decode failed, or no level_init)".into()
            }
        });

        stage.finish_from(&walked);
        let stage = progress.begin_scoped(StageId::Images, "map", None);
        let compass = std::mem::take(&mut sink.compass).resolve(&sink.materials);
        report.push(format!("compass: {:?}", compass));
        let mut materials = std::mem::take(&mut sink.materials);
        let absorbed = materials.absorb_technique_set_tables(&seeded_techsets);
        let promoted = materials.promote_iw5_fallback_tables();
        let t5_alias = materials.absorb_t5_feature_token_donors();
        let stub_routed = materials.reroute_stub_materials();
        report.push(format!(
            "material route (pre-decode): absorbed_techsets={absorbed} iw5_promoted={promoted} t5_tech_alias={t5_alias} \
             stub_routed={stub_routed} unrouted={}",
            materials.unrouted_material_count()
        ));
        let map_xmodels = std::mem::take(&mut sink.map_xmodels);
        let bodies = std::mem::take(&mut sink.bodies);
        let fpv_meshes = std::mem::take(&mut sink.fpv_meshes);
        match decode_material_color_maps(
            path,
            &mut materials,
            &stage,
            crate::session_load::load_pool(),
        ) {
            Ok(stats) => {
                report.push(format!(
                "IWD color/normal maps: {}/{} decoded, {} missing, {} unsupported from {} archives",
                stats.decoded, stats.requested, stats.missing, stats.unsupported, stats.archives
            ));
                if let Some(gap) = stats.first_gap {
                    report.push(format!("IWD material-map gap: {gap}"));
                }
            }
            Err(error) => report.push(format!("IWD material-map gap: {error}")),
        }
        report.push(format!(
            "material catalog: {} materials, {} images, {} capture gaps",
            materials.materials.len(),
            materials.images.len(),
            materials.capture_gaps
        ));

        stage.done();
        let stage = progress.begin_scoped(StageId::MapAssets, "geometry", None);
        let Some(geometry) = stream.gfx_world() else {
            stage.fail();
            report.push("no GfxWorld retained — nothing to draw".into());
            let dm_spawns = dm_spawn_points_t5(&stream);
            let mut loaded = LoadedWorld {
                scripts,
                sound: map_sound,
                world: PreparedWorld {
                    policy: WorldDrawPolicy::t5(),
                    exp_fog,
                    createart_name,
                    ..PreparedWorld::empty(WorldDrawPolicy::t5())
                },
                collision: clip,
                spawns: dm_spawns,
                bodies,
                fpv_meshes,
                facts: crate::MapFacts {
                    t5_teamset: t5_teamset.clone(),
                    script_sound: script_sound.clone(),
                    ..Default::default()
                },
                report,
                ..LoadedWorld::empty(WorldDrawPolicy::t5())
            };
            loaded.push_gap(
                PreparedCapability::PreparedWorld,
                "no GfxWorld retained — nothing to draw",
                Some("assets::lane::t5::load_world/no_gfx_world"),
            );
            return loaded;
        };
        report.push(format!(
            "gfx retained: verts={} indices={} surfaces={} lightmaps={} cells={} smodels={}",
            geometry.vertex_count,
            geometry.index_count,
            geometry.surface_count,
            geometry.lightmap_count,
            geometry.cell_count,
            geometry.smodel_count
        ));

        let world_draw = build_t5_world_draw(&stream, geometry, materials);
        stage.finish_from(&world_draw);
        match world_draw {
            Ok((mut draw, map_materials)) => {
                let mut map_xmodels = map_xmodels;
                if let Some(name) = geometry.sky_box_model.and_then(|p| stream.cstr(p).ok()) {
                    draw.sky_model = map_xmodels.take_named_mesh(name);
                    report.push(format!(
                        "skybox model: {name} captured={}",
                        draw.sky_model.is_some()
                    ));
                }
                let map_models = super::build_t5_static_model_draw(&stream, geometry, map_xmodels);
                report_map_models(&mut report, &map_models, geometry.smodel_count);
                let asset_world::PreparedMapModels {
                    static_draw:
                        asset_world::StaticModelDraw {
                            meshes: static_model_meshes,
                            placements: static_model_instances,
                            ..
                        },
                    scene_assets: map_xmodel_scene_assets,
                    script_instances: script_model_instances,
                    script_brush_models,
                    flag_descriptors,
                    script_structs,
                    ..
                } = map_models;
                let intermission_view = intermission_view_t5(&stream);
                let minimap_corners = minimap_corners_t5(&stream);
                let north_yaw = asset_world::worldspawn_north_yaw_t5(&stream);
                let dm_spawns = dm_spawn_points_t5(&stream);
                report_ffa_spawns(&mut report, &dm_spawns);
                report_intermission(&mut report, intermission_view.as_ref());
                report.push(format!(
                    "world mesh: {} vertices, {} triangles, {} surfaces ({} skipped)",
                    draw.stats.vertices,
                    draw.stats.triangles,
                    draw.stats.surfaces,
                    draw.stats.skipped_surfaces
                ));
                report_world_batches(&mut report, &draw);
                let material_surfaces = draw
                    .surface_materials
                    .iter()
                    .filter(|material| material.is_some())
                    .count();
                report.push(format!(
                    "material surfaces: {material_surfaces}/{}, {} compact batches",
                    draw.surface_materials.len(),
                    draw.batches.len()
                ));
                report.push(format!(
                    "primary lights: {} (sun index ≤ {})",
                    draw.primary_lights.len(),
                    geometry.sun_primary_light_index
                ));
                report.push(
                    "draw path: T5 materials + lightmaps + clip + DPVS + lights/probes/sky".into(),
                );
                match &draw.lightmap {
                    Ok(pages) => {
                        let decoded = pages.iter().flatten().count();
                        let exact = pages
                            .iter()
                            .flatten()
                            .filter(|p| p.primary_image.is_some() && p.secondary_image.is_some())
                            .count();
                        let names: Vec<&str> = pages
                            .iter()
                            .flatten()
                            .map(|p| p.ambient_source_name.as_str())
                            .collect();
                        report.push(format!(
                            "lightmap: {decoded}/{} pages decoded exact={exact}/{} ({})",
                            pages.len(),
                            pages.len(),
                            names.join(", ")
                        ));
                    }
                    Err(gap) => report.push(format!("lightmap gap: {gap}")),
                }
                let reflection_probe_images =
                    decode_reflection_probes(&mut report, &draw, &map_materials);
                report_dpvs(&mut report, &draw);
                let light_grid =
                    asset_model::OwnedLightGrid::from_t5_stream(&stream, geometry.light_grid);
                let smodel_lighting_samples = match &light_grid {
                    Some(grid) => {
                        report.push(format!(
                            "t5 light_grid: READY rows={} entries={} colors={} row_axis={} col_axis={} regions={}",
                            grid.row_data_start.len() / 2,
                            grid.entries.len() / 4,
                            grid.color_count,
                            grid.row_axis,
                            grid.col_axis,
                            u8::from(grid.has_light_regions)
                        ));
                        smodel_lighting_samples(
                            &mut report,
                            grid,
                            asset_model::model_lighting::collect_t5_smodel_lighting_origins(
                                &stream, geometry,
                            ),
                            &static_model_instances,
                            clip.as_ref(),
                        )
                    }
                    None => {
                        report.push(format!(
                            "t5 light_grid: none (missing tables or rowAxis/colAxis not 0..2; row={} col={} entries={} colors={})",
                            geometry.light_grid.row_axis,
                            geometry.light_grid.col_axis,
                            geometry.light_grid.entry_count,
                            geometry.light_grid.color_count
                        ));
                        Vec::new()
                    }
                };
                let min = draw.stats.min;
                let max = draw.stats.max;
                let world_bounds = draw.stats.bounds;
                LoadedWorld {
                    scripts,
                    sound: map_sound,
                    materials: map_materials,
                    world: PreparedWorld {
                        source_namespace: Some(asset_core::AssetNamespace::T5),
                        draw: Some(draw),
                        dynamic_light: None,
                        static_model_meshes,
                        static_model_instances,
                        map_xmodel_scene_assets,
                        script_model_instances,
                        script_brush_models,
                        flag_descriptors,
                        script_structs,
                        dyn_ents: asset_world::DynEntCatalog::default(),
                        smodel_lighting_samples,
                        light_grid,
                        fx: sink.fx,
                        fx_models: Default::default(),

                        fx_glass: None,
                        impact_fx: sink.impact_fx.take_table(),
                        reflection_probe_images,
                        intermission_view,
                        exp_fog,
                        t6_film_grade: None,
                        film_vision: None,
                        film_visions: Default::default(),
                        createart_name,
                        min,
                        max,
                        world_bounds,
                        policy: WorldDrawPolicy::t5(),
                    },
                    collision: clip,
                    spawns: dm_spawns,
                    bodies,
                    fpv_meshes,
                    facts: crate::MapFacts {
                        minimap_corners,
                        north_yaw,
                        compass,
                        t5_teamset: t5_teamset.clone(),
                        script_sound: script_sound.clone(),
                        ..Default::default()
                    },
                    report,
                    ..LoadedWorld::empty(WorldDrawPolicy::t5())
                }
            }
            Err(e) => {
                report.push(format!("T5 world mesh: {e}"));
                let dm_spawns = dm_spawn_points_t5(&stream);
                let mut loaded = LoadedWorld {
                    scripts,
                    sound: map_sound,
                    world: PreparedWorld {
                        policy: WorldDrawPolicy::t5(),
                        exp_fog,
                        createart_name,
                        ..PreparedWorld::empty(WorldDrawPolicy::t5())
                    },
                    collision: clip,
                    spawns: dm_spawns,
                    bodies,
                    fpv_meshes,
                    facts: crate::MapFacts {
                        t5_teamset: t5_teamset.clone(),
                        script_sound: script_sound.clone(),
                        ..Default::default()
                    },
                    report,
                    ..LoadedWorld::empty(WorldDrawPolicy::t5())
                };
                loaded.push_gap(
                    PreparedCapability::PreparedWorld,
                    format!("T5 world mesh: {e}"),
                    Some("assets::lane::t5::load_world/world_mesh"),
                );
                loaded
            }
        }
    }

    fn load_common_mp(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,
        decode_color_maps: bool,
        material_seed: asset_material::MaterialCatalog,
    ) -> CommonCensus {
        let header = match image.t5_header() {
            Ok(header) => header,
            Err(error) => {
                return CommonCensus {
                    report: vec![format!("common_mp models: T5 zone header: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut memory = T5ZoneMemory::for_header(&header);
        let mut stream = match memory.stream(&image.bytes) {
            Ok(stream) => stream,
            Err(error) => {
                return CommonCensus {
                    report: vec![format!("common_mp models: T5 zone arenas: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut sink = CommonWalkSink::default();
        sink.seed_materials(material_seed);
        sink.set_capture_zone(asset_core::ZoneOwner::from_zone_path(path));
        sink.set_capture_ns(asset_core::AssetNamespace::T5);
        sink.sound = asset_audio::ZoneSoundCapture::claim_common(
            path,
            asset_audio::ZoneGame::T5,
            "common census",
        );
        let walk = fastfile_t5::load_zone(&mut stream, &mut sink);
        if let Some(sound) = sink.sound.take() {
            sound.deposit(walk.as_ref().map(|_| ()).map_err(|e| e.to_string()));
        }
        let mut report = vec![format!("common_mp (T5): walked {} assets", sink.walked)];
        if let Err(error) = walk {
            report.push(format!(
                "common_mp T5 walk: stopped after {} assets — {error}",
                sink.walked
            ));
        }
        report.push(format!(
            "common_mp T5 pointer drift: {} unsettled offsets",
            stream.unsettled_offsets()
        ));
        report.push(format!(
            "s2 t5 walk: reuse_mat={} reuse_img={} pool_mat={} pool_img={}",
            sink.materials.link_reused_materials,
            sink.materials.link_reused_images,
            sink.materials.materials.len(),
            sink.materials.images.len(),
        ));
        let captured = sink.weapons.len();

        sink.weapons.resolve_reticles(&sink.materials);
        sink.weapons.resolve_projectile_fx_edges(&sink.fx);
        if let Some(table) = sink.impact_fx.table.as_ref() {
            sink.weapons.resolve_projectile_impact_fx(table);
        }
        sink.weapons
            .resolve_combat_fx(&sink.fx, &asset_game::TracerCatalog::default());
        let leftover_fx = sink.fx.len();
        let leftover_fx_gaps = sink.fx.capture_gaps;
        let projectile_keys = sink.weapons.projectile_model_hints();
        let mut weapons = sink.weapons.into_build();
        let namespace = asset_core::AssetNamespace::T5;
        for id in 1..=weapons.len() as u32 {
            for name in [
                weapons.gun_xmodel_of(id),
                weapons.hand_xmodel_of(id),
                weapons.rocket_model_of(id),
            ]
            .into_iter()
            .flatten()
            {
                if sink.fpv_meshes.get(namespace, name).is_none()
                    && let Some(entry) = sink.projectile_meshes.get(namespace, name)
                {
                    sink.fpv_meshes
                        .insert_in(namespace, entry.skel.clone(), Some(&sink.materials));
                }
            }
            if let Some(name) = weapons.world_model_of(id)
                && sink.world_weapons.get(namespace, name).is_none()
                && let Some(entry) = sink.projectile_meshes.get(namespace, name)
            {
                sink.world_weapons
                    .insert_in(namespace, entry.skel.clone(), Some(&sink.materials));
            }
        }
        sink.projectile_meshes.keep_referenced(&projectile_keys);
        weapons.apply_stats_tables(sink.stats_tables.values());
        weapons.resolve_sz_xanim_edges(&sink.xanims);
        weapons.resolve_fpv_mesh_edges(&sink.fpv_meshes);
        weapons.resolve_world_model_edges(&sink.world_weapons);
        report.push(format!(
            "common_mp statsTable: tables={} item_groups={}",
            sink.stats_tables.len(),
            weapons.item_group_count(),
        ));
        let gun_named = weapons.gun_xmodel_count();
        report.push(format!(
        "common_mp weapons: {captured} captures → {} unique catalog ids (sorted); {gun_named} with gunXModel[0]",
        weapons.len()
    ));
        report.push(format!(
            "common_mp leftover t5 fx: {leftover_fx} captured ({leftover_fx_gaps} gaps)"
        ));
        report.push(format!(
        "common_mp FPV mesh catalog: {} bind-pose viewmodel_* meshes retained ({} with tag_view)",
        sink.fpv_meshes.len(),
        sink.fpv_meshes.tag_view_count()
    ));
        report.push(format!(
            "common_mp T5 xanims: {} captured ({} gaps)",
            sink.xanims.len(),
            sink.xanims.capture_gaps
        ));

        let mut materials = sink.materials;

        let mut pending_images = None;
        if decode_color_maps {
            let stage = progress.begin_scoped(StageId::Images, "common_mp", None);
            let (inline, plan) = asset_material::material_images::plan_material_color_maps(
                path,
                &mut materials,
                &stage,
                crate::session_load::load_pool(),
            );
            report.push(format!(
                "common_mp T5 IWD color maps: {} claimed for the merged pool, {} in-zone bodies decoded here ({} missing, {} unsupported)",
                plan.len(),
                inline.decoded,
                inline.missing,
                inline.unsupported
            ));
            // The plan itself is decoded later, under its own stage. This one
            // planned and decoded the in-zone bodies, and it finished; letting
            // the handle drop would record it as interrupted.
            stage.done();
            pending_images = Some(plan);
        }
        report.push(format!(
            "common_mp T5 teamset scripts: {} (_teamset_*.gsc icons)",
            sink.teamsets.len()
        ));
        CommonCensus {
            pending_images,
            weapons,
            cac_tables: sink.stats_tables.into_values().collect(),
            material_population: materials,
            fpv: sink.fpv_meshes,
            projectile_meshes: sink.projectile_meshes,
            world_weapons: sink.world_weapons,
            xanims: sink.xanims,
            fx: sink.fx,
            impact_fx: sink.impact_fx.take_table(),
            fx_models: sink.fx_models,
            report,
            teamsets: sink.teamsets,
            scene_models: sink.scene_models,
            film_visions: std::collections::BTreeMap::new(),
            ..Default::default()
        }
    }

    fn load_material_population(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,
        material_seed: asset_material::MaterialCatalog,
    ) -> MaterialPopulation {
        let _ = progress;
        let zone_name = path.file_stem().map_or_else(
            || "startup".to_owned(),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let header = match image.t5_header() {
            Ok(header) => header,
            Err(error) => {
                return MaterialPopulation {
                    materials: material_seed,
                    report: vec![format!("startup materials: T5 zone header: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut memory = T5ZoneMemory::for_header(&header);
        let mut stream = match memory.stream(&image.bytes) {
            Ok(stream) => stream,
            Err(error) => {
                return MaterialPopulation {
                    materials: material_seed,
                    report: vec![format!("startup materials: T5 zone arenas: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut sink = MaterialPopulationSink::default();
        sink.seed_materials(material_seed);
        sink.set_capture_zone(asset_core::ZoneOwner::from_zone_path(path));
        sink.set_capture_ns(asset_core::AssetNamespace::T5);
        sink.sound = asset_audio::ZoneSoundCapture::claim_common(
            path,
            asset_audio::ZoneGame::T5,
            "material population",
        );
        let walk = fastfile_t5::load_zone(&mut stream, &mut sink);
        if let Some(sound) = sink.sound.take() {
            sound.deposit(walk.as_ref().map(|_| ()).map_err(|e| e.to_string()));
        }
        let mut report = Vec::new();
        if let Err(error) = walk {
            report.push(format!(
                "startup materials: T5 walk stopped after {} assets — {error}",
                sink.walked
            ));
        }
        let memory = sink.materials.image_memory();
        report.push(format!(
            "startup material walk: zone={zone_name} assets={} materials={} images={} decoded={} fpv=0 weapons=0 (materials-only sink; not load_common_mp)",
            sink.walked,
            sink.materials.materials.len(),
            memory.images,
            memory.decoded_images,
        ));
        MaterialPopulation {
            walked: sink.walked,
            light_defs: Vec::new(),
            materials: sink.materials,
            report,
            cac_tables: sink.stats_tables.into_values().collect(),
            scripts: sink.scripts,
        }
    }
}

fn absorb_localized_map_sound(path: &Path, catalog: &mut asset_audio::SoundCatalog) -> String {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return "localized map sound gap: map zone has no name".into();
    };
    let found = match asset_transport::discover::find_t5_localized_zone(path, None, stem) {
        Ok(Some(found)) => found,
        Ok(None) => return format!("localized map sound: no language archive for {stem}"),
        Err(error) => return format!("localized map sound gap: {error}"),
    };
    let walk = || -> Result<(asset_audio::SoundCatalog, usize), String> {
        let image = asset_transport::open_zone(&found.path).map_err(|e| e.to_string())?;
        let header = image.t5_header().map_err(|e| e.to_string())?;
        let mut memory = T5ZoneMemory::for_header(&header);
        let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
        let mut sink = MaterialPopulationSink::default();
        sink.set_capture_zone(asset_core::ZoneOwner::from_zone_path(&found.path));
        sink.set_capture_ns(asset_core::AssetNamespace::T5);
        sink.sound = Some(asset_audio::ZoneSoundCapture::for_map(
            &found.path,
            asset_audio::ZoneGame::T5,
            "localized map",
        ));
        let walked = fastfile_t5::load_zone(&mut stream, &mut sink).map(|_| ());
        let sound = sink.sound.take().ok_or("sound capture dropped")?;
        let part = sound.finish(walked.map_err(|e| e.to_string()))?;
        let aliases = part.sounds().len();
        Ok((part, aliases))
    };
    match walk() {
        Ok((part, aliases)) => {
            catalog.absorb(part);
            format!("localized map sound: {} {aliases} aliases", found.zone_name)
        }
        Err(error) => format!("localized map sound gap: {}: {error}", found.zone_name),
    }
}
