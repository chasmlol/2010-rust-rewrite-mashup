use std::path::Path;

use super::helpers::{
    decode_reflection_probes, report_ffa_spawns, report_intermission, report_map_models,
    smodel_lighting_samples,
};
use super::{
    CommonCensus, CommonWalkSink, LaneGap, LoadedWorld, MaterialPopulation, MaterialPopulationSink,
    ZoneLane, ZoneWalkSink,
};
use crate::lane_capability::{LaneStatus, PreparedCapability};
use crate::session_load::PreparedWorld;
use asset_anim::XAnimBuild;
use asset_core::ZoneGame;
use asset_model::{BodyMeshBuild, OwnedLightGrid};
use asset_transport::progress::{LoadProgress, StageId};
use asset_transport::{Iw5ZoneMemory, ZoneImage};
use asset_world::{
    MASK_PLAYER_SOLID, WorldDrawPolicy, attach_iw5_static_models, build_iw5_clip_collision,
    build_iw5_world_draw, dm_spawn_points_iw5, intermission_view_iw5, minimap_corners_iw5,
};

pub struct Iw5Lane;

impl Iw5Lane {
    pub const GAME: ZoneGame = ZoneGame::Iw5;
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
            LaneStatus::SupportedPopulated,
        ),
        (
            PreparedCapability::PlayableFfa,
            LaneStatus::UnsupportedByRuntimeProfile,
        ),
    ];

    fn stamp_map_tree_team_settings(path: &Path, loaded: &mut LoadedWorld) {
        if loaded.facts.team_settings.allies.is_some() || loaded.facts.team_settings.axis.is_some()
        {
            return;
        }
        match asset_transport::find_zone_for_tree(path, "code_post_gfx_mp") {
            Ok(found) => {
                let (arena, table) = asset_game::load_iw5_team_sources(&found.path);
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if let Some(table) = table.as_ref() {
                    loaded.facts.team_settings = asset_game::team_settings_for_zone(
                        table,
                        asset_core::AssetNamespace::Iw5,
                        arena.as_deref(),
                        stem,
                    );
                    let settings = &loaded.facts.team_settings;
                    loaded.facts.objective_visuals =
                        asset_game::ObjectiveVisuals::from_faction_table(
                            table,
                            Some(
                                settings
                                    .allies_charset
                                    .as_deref()
                                    .unwrap_or(IW5_DEFAULT_ALLIES),
                            ),
                            Some(settings.axis_charset.as_deref().unwrap_or(IW5_DEFAULT_AXIS)),
                        );
                }
                match (
                    loaded.facts.team_settings.allies.as_ref(),
                    loaded.facts.team_settings.axis.as_ref(),
                ) {
                    (Some(a), Some(x)) => loaded.report.push(format!(
                        "team icons: iw5 arena allies={a} axis={x} zone={stem}"
                    )),
                    _ => loaded.report.push(format!(
                        "team icons gap: IW5 code_post_gfx_mp arena/table missed zone={stem}"
                    )),
                }
            }
            Err(error) => loaded.report.push(format!(
                "team icons gap: IW5 same-tree code_post_gfx_mp: {error}"
            )),
        }
    }

    fn finish_loaded(
        scripts: Option<&(String, Option<String>)>,
        path: &Path,
        mut loaded: LoadedWorld,
    ) -> LoadedWorld {
        Self::stamp_map_tree_team_settings(path, &mut loaded);
        if let Some((entities, stand_in)) = scripts {
            loaded.scripts.set_entities(entities.clone());
            let map = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_ascii_lowercase());
            if let (Some(stand_in), Some(map)) = (stand_in, map) {
                loaded
                    .scripts
                    .insert_source(&format!("maps/mp/{map}"), stand_in.clone());
                loaded.report.push(format!(
                    "map script: maps/mp/{map} written from the zone's declarations"
                ));
            }
        }
        loaded
    }
}

impl ZoneLane for Iw5Lane {
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
        shared_surfaces: asset_model::SharedXModelSurfaces,
        material_seed: asset_material::MaterialCatalog,
        _common_film_visions: &super::FilmVisionCatalog,
    ) -> LoadedWorld {
        let mut report = vec![format!("game: IW5 ({})", path.display())];
        let stage = progress.begin_scoped(StageId::MapAssets, "header", None);
        let header = match image.iw5_header() {
            Ok(h) => {
                stage.done();
                h
            }
            Err(e) => {
                stage.fail();
                return Self::finish_loaded(
                    None,
                    path,
                    LoadedWorld::with_gap(
                        WorldDrawPolicy::iw5(),
                        PreparedCapability::PreparedWorld,
                        format!("IW5 zone header: {e}"),
                        Some("assets::lane::iw5::load_world/zone_header"),
                    ),
                );
            }
        };

        let stage = progress.begin_scoped(StageId::MapAssets, "memory", None);
        report.push(asset_transport::xfile_arena_row(
            "zone arenas map",
            &header.block_size,
            fastfile_iw5::XFILE_BLOCK_TEMP,
            fastfile_iw5::XFILE_BLOCK_VIRTUAL,
        ));
        let mut memory = Iw5ZoneMemory::for_header(&header);
        let mut stream = match memory.stream(&image.bytes) {
            Ok(s) => {
                stage.done();
                s
            }
            Err(e) => {
                stage.fail();
                return Self::finish_loaded(
                    None,
                    path,
                    LoadedWorld::with_gap(
                        WorldDrawPolicy::iw5(),
                        PreparedCapability::PreparedWorld,
                        format!("IW5 zone arenas: {e}"),
                        Some("assets::lane::iw5::load_world/zone_arenas"),
                    ),
                );
            }
        };

        let stage = progress.begin_scoped(StageId::MapAssets, "walk", None);
        let mut sink = ZoneWalkSink::default();
        let seeded_techsets = material_seed.technique_set_facts().to_vec();
        sink.seed_materials(material_seed);
        sink.map_xmodels.shared_surfaces = shared_surfaces;
        sink.set_capture_zone(asset_core::ZoneOwner::from_zone_path(path));
        sink.set_capture_ns(asset_core::AssetNamespace::Iw5);
        sink.sound = Some(asset_audio::ZoneSoundCapture::for_map(
            path,
            asset_audio::ZoneGame::Iw5,
            "map",
        ));
        let walked = fastfile_iw5::load_zone(&mut stream, &mut sink);
        let map_sound = sink
            .sound
            .take()
            .map(|sound| sound.finish(walked.as_ref().map(|_| ()).map_err(|e| e.to_string())));
        match &walked {
            Ok(_) => report.push(format!("zone walk: complete, {} assets", sink.walked)),
            Err(e) => report.push(format!(
                "zone walk: stopped after {} assets — {e}",
                sink.walked
            )),
        }
        stage.set_completed(sink.walked as u64);
        let exp_fog = sink.exp_fog.take();
        let script_sound = std::mem::take(&mut sink.script_sound).finish();
        let createart_name = sink.createart_name.take();
        let scripts = asset_world::map_ents_entity_string_iw5(&stream)
            .map(asset_world::iw5_entity_string_named)
            .map(|entities| {
                let stand_in =
                    (!sink.iw5_map.is_empty()).then(|| sink.iw5_map.map_script(&entities));
                (entities, stand_in)
            });
        report.push(match &exp_fog {
            Some(fog) => format!(
                "createart fog: READY start={:.3} half={:.3} maxOpacity={:.3} sun={} name={}",
                fog.start_dist,
                fog.halfway_dist,
                fog.max_opacity,
                fog.sun.is_some(),
                createart_name.as_deref().unwrap_or("?")
            ),
            None => match createart_name.as_deref() {
                Some(name) => {
                    format!("createart fog: RED {name} walked but setExpFog did not parse")
                }
                None => "createart fog: RED missing maps/createart/<map>_art|_fog setExpFog".into(),
            },
        });
        report.push(format!(
            "serialization: {} ({} B pointers)",
            stream.wire_format().name(),
            stream.pointer_bytes()
        ));
        report.push(format!(
            "pointer drift: {} unsettled offsets",
            stream.unsettled_offsets()
        ));
        report.push(format!(
            "materials: {} authored ({} images, {} capture gaps)",
            sink.materials.materials.len(),
            sink.materials.images.len(),
            sink.materials.capture_gaps
        ));
        let compass = std::mem::take(&mut sink.compass).resolve(&sink.materials);
        report.push(format!("compass: {:?}", compass));
        let mut materials = sink.materials;
        let absorbed = materials.absorb_technique_set_tables(&seeded_techsets);
        let promoted = materials.promote_iw5_fallback_tables();
        let stub_routed = materials.reroute_stub_materials();
        report.push(format!(
            "material route (pre-draw): absorbed_techsets={absorbed} iw5_promoted={promoted} stub_routed={stub_routed} \
         unrouted={}",
            materials.unrouted_material_count()
        ));
        stage.finish_from(&walked);
        let clip_stage = progress.begin_scoped(StageId::MapAssets, "collision", None);
        let map_xmodels = std::mem::take(&mut sink.map_xmodels);
        let bodies = std::mem::take(&mut sink.bodies);
        let fpv_meshes = std::mem::take(&mut sink.fpv_meshes);
        let map_xanims = std::mem::take(&mut sink.xanims);

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
            match build_iw5_clip_collision(&stream, geometry) {
                Ok(mut clip) => {
                    attach_iw5_static_models(&stream, geometry, &sink.xmodel_coll, &mut clip);
                    clip.trigger_models = asset_world::trigger_models_iw5(&stream);
                    report.push(format!(
                        "trigger models: {} ({} with hulls)",
                        clip.trigger_models.len(),
                        clip.trigger_models.iter().filter(|h| !h.is_empty()).count()
                    ));
                    let solid = clip
                        .brushes
                        .iter()
                        .filter(|b| b.contents & MASK_PLAYER_SOLID != 0)
                        .count();
                    report.push(format!(
                        "clip brushes extracted: {} ({} player-solid by contents mask)",
                        clip.brushes.len(),
                        solid
                    ));
                    report.push(format!(
                        "clip mesh extracted: verts={} tris={} nodes={} leaves={} aabb={} cmodels={} smodels={}",
                        clip.mesh.verts.len(),
                        clip.mesh.tri_indices.len() / 3,
                        clip.nodes.len(),
                        clip.leaves.len(),
                        clip.mesh.aabb_trees.len(),
                        clip.cmodels.len(),
                        clip.static_models.len()
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

        clip_stage.done();
        let stage = progress.begin_scoped(StageId::MapAssets, "geometry", None);
        let Some(geometry) = stream.gfx_world() else {
            stage.fail();
            report.push("no GfxWorld retained — nothing to draw".into());
            report.push("bodies: empty (no GfxWorld; XModel bone capture not reached)".into());
            let dm_spawns = dm_spawn_points_iw5(&stream);
            return Self::finish_loaded(
                scripts.as_ref(),
                path,
                LoadedWorld {
                    sound: map_sound,
                    world: PreparedWorld {
                        exp_fog,
                        createart_name,
                        policy: WorldDrawPolicy::iw5(),
                        ..PreparedWorld::empty(WorldDrawPolicy::iw5())
                    },
                    collision: clip,
                    spawns: dm_spawns,
                    bodies: BodyMeshBuild::default(),
                    fpv_meshes,
                    xanims: XAnimBuild::default(),
                    report,
                    gaps: vec![LaneGap {
                        capability: PreparedCapability::PreparedWorld,
                        reason: "no GfxWorld retained — nothing to draw".into(),
                        addr: Some("assets::lane::iw5::load_world/no_gfx_world"),
                    }],
                    ..LoadedWorld::empty(WorldDrawPolicy::iw5())
                },
            );
        };
        report.push(format!(
        "gfx retained: verts={} indices={} surfaces={} lightmaps={} cells={} smodels={} sun_lights={}",
        geometry.vertex_count,
        geometry.index_count,
        geometry.surface_count,
        geometry.lightmap_count,
        geometry.cell_count,
        geometry.smodel_count,
        geometry.sun_primary_light_count
    ));
        if let Some(com) = stream.com_world() {
            report.push(format!(
                "com_world: {} primary lights retained",
                com.primary_light_count
            ));
        } else {
            report.push("com_world: not retained".into());
        }

        let world_draw = build_iw5_world_draw(&stream, geometry, materials);
        stage.finish_from(&world_draw);
        match world_draw {
            Ok((draw, map_materials)) => {
                let map_models = super::build_iw5_static_model_draw(&stream, geometry, map_xmodels);
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
                let intermission_view = intermission_view_iw5(&stream);
                let minimap_corners = minimap_corners_iw5(&stream);
                let north_yaw = asset_world::worldspawn_north_yaw_iw5(&stream);
                let airstrike_height = asset_world::airstrike_height_iw5(&stream);
                let dm_spawns = dm_spawn_points_iw5(&stream);
                report_ffa_spawns(&mut report, &dm_spawns);
                report_intermission(&mut report, intermission_view.as_ref());
                report.push(format!(
                    "world mesh: {} vertices, {} triangles, {} surfaces ({} skipped), {} batches, sky_surfs={} sky_start={}",
                    draw.stats.vertices,
                    draw.stats.triangles,
                    draw.stats.surfaces,
                    draw.stats.skipped_surfaces,
                    draw.batches.len(),
                    draw.stats.sky_surfaces,
                    draw.dpvs.sky_start_surfs.len()
                ));
                report.push(format!(
                    "materials in draw: {}; primary lights: {}; named_defs={}; recorded_light_defs={}",
                    map_materials.materials.len(),
                    draw.primary_lights.len(),
                    draw.primary_lights
                        .iter()
                        .filter(|light| light.def_name.is_some())
                        .count(),
                    draw.light_defs.len()
                ));
                report.push(match &draw.light_region_hulls {
                    None => "light_region_hulls: not retained".into(),
                    Some(lists) => {
                        let hulls: usize = lists.iter().map(Vec::len).sum();
                        format!("light_region_hulls: {} lights, {hulls} hulls", lists.len())
                    }
                });
                match &draw.lightmap {
                    Ok(pages) => {
                        let ok = pages.iter().filter(|p| p.is_some()).count();
                        let exact = pages
                            .iter()
                            .flatten()
                            .filter(|page| {
                                page.primary_image.is_some() && page.secondary_image.is_some()
                            })
                            .count();
                        report.push(format!(
                            "lightmaps: {ok}/{} pages decoded exact={exact}/{}",
                            pages.len(),
                            pages.len()
                        ));
                    }
                    Err(e) => report.push(format!("lightmaps: {e}")),
                }
                let reflection_probe_images =
                    decode_reflection_probes(&mut report, &draw, &map_materials);
                let portal_count: usize = draw.dpvs.portals_per_cell.iter().map(|c| c.len()).sum();
                let aabb_nodes: usize = draw.dpvs.aabb_trees.iter().map(|t| t.len()).sum();
                report.push(format!(
                "dpvs: planes={} nodes={} cells={} portals={} aabb_nodes={} sorted_surfs={} lit_opaque=[{},{}) surfaces_bounds={} smodel_bounds={} cleared_boxes={}",
                draw.dpvs.planes.len(),
                draw.dpvs.nodes.len(),
                draw.dpvs.cell_count,
                portal_count,
                aabb_nodes,
                draw.dpvs.sorted_surf_index.len(),
                draw.dpvs.lit_opaque_begin,
                draw.dpvs.lit_opaque_end,
                draw.dpvs.surface_bounds.len(),
                draw.dpvs.smodel_bounds.len(),
                draw.dpvs.cleared_boxes
            ));
                let light_grid = OwnedLightGrid::from_iw5_stream(&stream, geometry.light_grid);
                match &light_grid {
                    Some(owned) => report.push(format!(
                        "light_grid: rows={} entries={} colors={} has_light_regions={} sun_primary={}",
                        owned.row_data_start.len() / 2,
                        owned.entries.len() / 4,
                        owned.color_count,
                        owned.has_light_regions,
                        owned.sun_primary_light_index,
                    )),
                    None => report.push(format!(
                        "light_grid: none (row_axis={} col_axis={} entries={} colors={})",
                        geometry.light_grid.row_axis,
                        geometry.light_grid.col_axis,
                        geometry.light_grid.entry_count,
                        geometry.light_grid.color_count,
                    )),
                }
                let smodel_lighting_samples = match &light_grid {
                    Some(grid) => smodel_lighting_samples(
                        &mut report,
                        grid,
                        asset_model::model_lighting::collect_iw5_smodel_lighting_origins(
                            &stream, geometry,
                        ),
                        &static_model_instances,
                        clip.as_ref(),
                    ),
                    None => {
                        report.push("smodel lighting: none (no owned light-grid tables)".into());
                        Vec::new()
                    }
                };
                match &light_grid {
                    Some(grid) => report.push(format!(
                        "light grid: retained entries={} colors={} row_axis={} col_axis={}",
                        grid.entries.len() / 4,
                        grid.color_count,
                        grid.row_axis,
                        grid.col_axis
                    )),
                    None => report.push(
                        "light grid: none (missing tables or colAxis sandwich not 0..2)".into(),
                    ),
                }
                report.push("draw path: IW5 materials+lightmaps+lights+DPVS+static models".into());
                let body_lines = bodies.report_lines();
                if body_lines.is_empty() {
                    report.push(format!(
                        "bodies: 0 soldier XModels captured (decoder wired, {} map XModels walked)",
                        map_xmodel_scene_assets.len()
                    ));
                } else {
                    report.extend(body_lines);
                }
                report.push(format!("map xanims: {}", map_xanims.len()));
                Self::finish_loaded(
                    scripts.as_ref(),
                    path,
                    LoadedWorld {
                        sound: map_sound,
                        materials: map_materials,
                        world: PreparedWorld {
                            min: draw.stats.min,
                            max: draw.stats.max,
                            world_bounds: draw.stats.bounds,
                            draw: Some(draw),
                            static_model_meshes,
                            static_model_instances,
                            map_xmodel_scene_assets,
                            script_model_instances,
                            script_brush_models,
                            flag_descriptors,
                            script_structs,
                            intermission_view,
                            light_grid,
                            reflection_probe_images,
                            exp_fog,
                            createart_name,
                            policy: WorldDrawPolicy::iw5(),
                            smodel_lighting_samples,
                            ..PreparedWorld::empty(WorldDrawPolicy::iw5())
                        },
                        collision: clip,
                        spawns: dm_spawns,
                        bodies,
                        fpv_meshes,
                        xanims: map_xanims,
                        facts: crate::MapFacts {
                            script_sound: script_sound.clone(),
                            minimap_corners,
                            north_yaw,
                            airstrike_height,
                            compass,
                            ..Default::default()
                        },
                        report,
                        ..LoadedWorld::empty(WorldDrawPolicy::iw5())
                    },
                )
            }
            Err(e) => {
                report.push(format!("world draw: {e}"));
                let dm_spawns = dm_spawn_points_iw5(&stream);
                Self::finish_loaded(
                    scripts.as_ref(),
                    path,
                    LoadedWorld {
                        sound: map_sound,
                        world: PreparedWorld {
                            exp_fog,
                            createart_name,
                            policy: WorldDrawPolicy::iw5(),
                            ..PreparedWorld::empty(WorldDrawPolicy::iw5())
                        },
                        collision: clip,
                        spawns: dm_spawns,
                        bodies,
                        fpv_meshes,
                        xanims: map_xanims,
                        report,
                        gaps: vec![LaneGap {
                            capability: PreparedCapability::PreparedWorld,
                            reason: format!("world draw: {e}"),
                            addr: Some("assets::lane::iw5::load_world/world_mesh"),
                        }],
                        ..LoadedWorld::empty(WorldDrawPolicy::iw5())
                    },
                )
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
        let header = match image.iw5_header() {
            Ok(header) => header,
            Err(error) => {
                return CommonCensus {
                    report: vec![format!("common_mp models: IW5 zone header: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut memory = Iw5ZoneMemory::for_header(&header);
        let mut stream = match memory.stream(&image.bytes) {
            Ok(stream) => stream,
            Err(error) => {
                return CommonCensus {
                    report: vec![format!("common_mp models: IW5 zone arenas: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut sink = CommonWalkSink::default();
        sink.seed_materials(material_seed);
        sink.set_capture_zone(asset_core::ZoneOwner::from_zone_path(path));
        sink.set_capture_ns(asset_core::AssetNamespace::Iw5);
        sink.sound = asset_audio::ZoneSoundCapture::claim_common(
            path,
            asset_audio::ZoneGame::Iw5,
            "common census",
        );
        let walk = fastfile_iw5::load_zone(&mut stream, &mut sink);
        if let Some(sound) = sink.sound.take() {
            sound.deposit(walk.as_ref().map(|_| ()).map_err(|e| e.to_string()));
        }
        let mut report = vec![format!("common_mp (IW5): walked {} assets", sink.walked)];
        if let Err(error) = walk {
            report.push(format!(
                "common_mp IW5 walk: stopped after {} assets — {error}",
                sink.walked
            ));
        }
        report.push(format!(
            "common_mp IW5 pointer drift: {} unsettled offsets",
            stream.unsettled_offsets()
        ));
        sink.weapons.resolve_reticles(&sink.materials);
        let captured = sink.weapons.len();
        let mut weapons = sink.weapons.into_build();
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
        "common_mp weapons: {captured} captures -> {} unique catalog ids (sorted); {gun_named} with gunXModel[0]; {} with szXAnims[IDLE]; {} with any szXAnims slot",
        weapons.len(),
        weapons.idle_anim_count(),
        weapons.sz_xanims_count()
    ));
        report.push(format!(
        "common_mp FPV mesh catalog: {} bind-pose viewmodel_* meshes retained ({} with tag_view)",
        sink.fpv_meshes.len(),
        sink.fpv_meshes.tag_view_count()
    ));

        report.push(format!(
            "common_mp XAnimParts: {} captured ({} gaps)",
            sink.xanims.len(),
            sink.xanims.capture_gaps
        ));
        report.push(format!(
            "common_mp IW5 materials: {} authored ({} techsets, {} images, {} capture gaps)",
            sink.materials.materials.len(),
            sink.materials.technique_set_facts().len(),
            sink.materials.images.len(),
            sink.materials.capture_gaps
        ));
        let light_defs = asset_world::capture_iw5_light_defs(&stream, &sink.materials);
        report.push(format!(
            "GfxLightDef common_mp: table={} bodies={} recorded={}",
            sink.light_def_table,
            sink.light_def_bodies,
            light_defs.len()
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
                "common_mp IW5 IWD color maps: {} claimed for the merged pool, {} in-zone bodies decoded here ({} missing, {} unsupported)",
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
        CommonCensus {
            pending_images,
            weapons,
            cac_tables: sink.stats_tables.into_values().collect(),
            material_population: materials,
            fpv: sink.fpv_meshes,
            world_weapons: sink.world_weapons,
            xanims: sink.xanims,
            light_defs,
            scene_models: sink.scene_models,
            shared_surfaces: sink.shared_surfaces,
            report,
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
        let header = match image.iw5_header() {
            Ok(header) => header,
            Err(error) => {
                return MaterialPopulation {
                    materials: material_seed,
                    report: vec![format!("startup materials: IW5 zone header: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut memory = Iw5ZoneMemory::for_header(&header);
        let mut stream = match memory.stream(&image.bytes) {
            Ok(stream) => stream,
            Err(error) => {
                return MaterialPopulation {
                    materials: material_seed,
                    report: vec![format!("startup materials: IW5 zone arenas: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut sink = MaterialPopulationSink::default();
        sink.seed_materials(material_seed);
        sink.set_capture_zone(asset_core::ZoneOwner::from_zone_path(path));
        sink.set_capture_ns(asset_core::AssetNamespace::Iw5);
        sink.sound = asset_audio::ZoneSoundCapture::claim_common(
            path,
            asset_audio::ZoneGame::Iw5,
            "material population",
        );
        let walk = fastfile_iw5::load_zone(&mut stream, &mut sink);
        if let Some(sound) = sink.sound.take() {
            sound.deposit(walk.as_ref().map(|_| ()).map_err(|e| e.to_string()));
        }
        let mut report = Vec::new();
        if let Err(error) = walk {
            report.push(format!(
                "startup materials: IW5 walk stopped after {} assets — {error}",
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

const IW5_DEFAULT_ALLIES: &str = "sas_urban";
const IW5_DEFAULT_AXIS: &str = "opforce_henchmen";
