use std::path::Path;

use fastfile_iw4::load_zone;

use super::helpers::{
    decode_reflection_probes, report_dpvs, report_ffa_spawns, report_intermission,
    report_map_models, report_world_batches, smodel_lighting_samples,
};
use super::{
    CommonCensus, CommonWalkSink, LaneGap, LoadedWorld, MaterialPopulation, MaterialPopulationSink,
    ZoneLane, ZoneWalkSink,
};
use crate::{
    lane_capability::{LaneStatus, PreparedCapability},
    session_load::PreparedWorld,
};
use asset_core::ZoneGame;
use asset_material::decode_material_color_maps;
use asset_transport::{LoadProgress, StageId, ZoneImage, ZoneMemory};
use asset_world::{
    WorldDrawPolicy, build_clip_collision, build_world_draw, census_entity_string_keys,
    dm_spawn_points, intermission_view, map_ents_entity_string, minimap_corners,
    worldspawn_north_yaw,
};

pub struct Iw4Lane;

impl Iw4Lane {
    pub const GAME: ZoneGame = ZoneGame::Iw4;
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
}

impl ZoneLane for Iw4Lane {
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
        common_film_visions: &super::FilmVisionCatalog,
    ) -> LoadedWorld {
        let mut report = Vec::new();
        let stage = progress.begin_scoped(StageId::MapAssets, "header", None);
        let header = match image.header() {
            Ok(h) => {
                stage.done();
                h
            }
            Err(e) => {
                stage.fail();
                return LoadedWorld::with_gap(
                    WorldDrawPolicy::iw4(),
                    PreparedCapability::PreparedWorld,
                    format!("zone header: {e}"),
                    Some("assets::lane::iw4::load_world/zone_header"),
                );
            }
        };

        let stage = progress.begin_scoped(StageId::MapAssets, "memory", None);
        report.push(asset_transport::xfile_arena_row(
            "zone arenas map",
            &header.block_size,
            fastfile_iw4::XFILE_BLOCK_TEMP,
            fastfile_iw4::XFILE_BLOCK_VIRTUAL,
        ));
        let mut memory = ZoneMemory::for_header(&header);
        let mut stream = match memory.stream(&image.bytes) {
            Ok(s) => {
                stage.done();
                s
            }
            Err(e) => {
                stage.fail();
                return LoadedWorld::with_gap(
                    WorldDrawPolicy::iw4(),
                    PreparedCapability::PreparedWorld,
                    format!("zone arenas: {e}"),
                    Some("assets::lane::iw4::load_world/zone_arenas"),
                );
            }
        };

        let mut sink =
            ZoneWalkSink::with_stage(progress.begin_scoped(StageId::MapAssets, "walk", None));
        let seeded_techsets = material_seed.technique_set_facts().to_vec();
        sink.seed_materials(material_seed);
        sink.map_xmodels.shared_surfaces = shared_surfaces;
        sink.set_capture_zone(asset_core::ZoneOwner::from_zone_path(path));
        sink.set_capture_ns(asset_core::AssetNamespace::Iw4);
        sink.sound = Some(asset_audio::ZoneSoundCapture::for_map(
            path,
            asset_audio::ZoneGame::Iw4,
            "map",
        ));
        let walked = load_zone(&mut stream, &mut sink);
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
        report.push(format!(
            "pointer drift: {} unsettled offsets",
            stream.unsettled_offsets()
        ));
        report.extend(sink.models.report("map"));
        if let Some(stage) = sink.stage.take() {
            stage.finish_from(&walked);
        }
        let light_def_table = sink.light_def_table;
        let light_def_bodies = sink.light_def_bodies;
        let mut materials = std::mem::take(&mut sink.materials);

        let absorbed = materials.absorb_technique_set_tables(&seeded_techsets);
        let stub_routed = materials.reroute_stub_materials();
        report.push(format!(
            "material route (pre-decode): absorbed_techsets={absorbed} stub_routed={stub_routed} \
         unrouted={}",
            materials.unrouted_material_count()
        ));

        let compass = std::mem::take(&mut sink.compass).resolve(&materials);
        let mut scripts = std::mem::take(&mut sink.scripts);
        if let Some(entities) = map_ents_entity_string(&stream) {
            scripts.set_entities(entities.to_owned());
        }
        let script_sound = std::mem::take(&mut sink.script_sound).finish();
        let exp_fog = sink.exp_fog.take();
        let createart_name = sink.createart_name.take();
        let vision_name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(|stem| format!("vision/{}.vision", stem.to_ascii_lowercase()));
        let mut film_visions = common_film_visions.clone();
        film_visions.extend(sink.film_visions.clone());
        let film_result = select_film_vision(
            vision_name.as_deref(),
            &sink.film_visions,
            common_film_visions,
        );
        let film_vision = match film_result {
            Ok(Some((vision, source))) => {
                report.push(format!(
                    "film vision: READY source={source} enable={} contrast={:.4} brightness={:.4} desaturation={:.5}",
                    vision.enable,
                    vision.contrast,
                    vision.brightness,
                    vision.desaturation,
                ));
                Some(vision)
            }
            Ok(None) => {
                report.push(format!(
                    "film vision: RED missing `{}` in map and common_mp fastfiles",
                    vision_name.as_deref().unwrap_or("<invalid map stem>")
                ));
                None
            }
            Err(error) => {
                report.push(format!("film vision: RED {error}"));
                None
            }
        };
        report.push(match (&compass.script, &compass.material, &compass.image) {
            (Some(script), Some(material), Some(image)) => format!(
                "compass: {script} declares `{material}` -> image `{image}` range={}",
                compass
                    .max_range
                    .map_or_else(|| "default".to_owned(), |r| format!("{r}"))
            ),
            (Some(script), Some(material), None) => {
                format!("compass gap: {script} declares `{material}` — no image in the zone")
            }
            _ => "compass gap: no map script declared a minimap".into(),
        });
        report.push(match (&script_sound.script, &script_sound.ambient_alias) {
            (Some(script), Some(alias)) => {
                format!("script sound: {script} ambientPlay `{alias}`")
            }
            (Some(script), None) => {
                format!("script sound gap: {script} has no ambientPlay")
            }
            _ => "script sound gap: no map script declared ambientPlay".into(),
        });
        report.push(match &exp_fog {
            Some(fog) => format!(
                "createart fog: READY start={:.3} half={:.3} maxOpacity={:.3} sun={}",
                fog.start_dist,
                fog.halfway_dist,
                fog.max_opacity,
                fog.sun.is_some()
            ),
            None => "createart fog: RED missing maps/createart/<map>_art.gsc setExpFog".into(),
        });
        let mut fx = std::mem::take(&mut sink.fx);
        let fx_models = std::mem::take(&mut sink.fx_models);
        let mut impact_fx = sink.impact_fx.take_table();
        let mut map_xmodels = std::mem::take(&mut sink.map_xmodels);
        let phys_presets = std::mem::take(&mut sink.phys_presets);
        let bodies = std::mem::take(&mut sink.bodies);
        let fpv_meshes = std::mem::take(&mut sink.fpv_meshes);
        let map_xanims = std::mem::take(&mut sink.xanims);
        report.push(format!(
            "map XAnim catalog: {} clips captured ({} gaps)",
            map_xanims.len(),
            map_xanims.capture_gaps
        ));
        report.push(format!(
            "fx catalog: {} FxEffectDef owned ({} capture gaps)",
            fx.len(),
            fx.capture_gaps
        ));
        if let Some(ref table) = impact_fx {
            report.push(format!(
                "impactfx: table `{}` rows={} gaps={}",
                table.name,
                table.row_count(),
                table.capture_gaps
            ));
        }

        let glass_names: Vec<String> = sink
            .fx_glass_def_materials
            .iter()
            .flat_map(|(intact, shattered)| [intact.clone(), shattered.clone()])
            .filter(|n| !n.is_empty())
            .collect();
        let fx_material_keys = fx.unique_material_keys();
        let leftover = std::mem::take(&mut sink.fx_glass_def_materials);
        let image_stage = progress.begin_scoped(StageId::Images, "map", None);
        let clip_stage = progress.begin_scoped(StageId::MapAssets, "collision", None);
        let mut clip = None;
        let mut clip_report = Vec::new();
        let mut fx_glass = None;
        let mut dyn_ents = asset_world::DynEntCatalog::default();
        let materials_side = &mut materials;

        let image_report = crate::session_load::load_pool().scope(|scope| {
            scope.spawn(async move {
                decode_map_material_images(
                    path,
                    materials_side,
                    image_stage,
                    fx_material_keys,
                    glass_names,
                )
            });

            clip = match stream.clip_map() {
                Some(geometry) => match build_clip_collision(&stream, geometry) {
                    Ok(mut clip) => {
                        asset_world::attach_static_models(
                            &stream,
                            geometry,
                            &sink.xmodel_coll,
                            &mut clip,
                        );
                        clip.trigger_models = asset_world::trigger_models(&stream);
                        clip_report.push(format!(
                            "trigger models: {} ({} with hulls)",
                            clip.trigger_models.len(),
                            clip.trigger_models.iter().filter(|h| !h.is_empty()).count()
                        ));
                        clip_report.push(format!(
                    "clipmap: planes={} brushes={} leaves={} nodes={} cmodels={} verts={} tris={} smodels={}",
                    geometry.plane_count,
                    geometry.brush_count,
                    geometry.leaf_count,
                    geometry.node_count,
                    geometry.cmodel_count,
                    geometry.vert_count,
                    geometry.tri_count,
                    clip.static_models.len()
                ));
                        clip_report.push(format!(
                            "clip brushes extracted: {} (player-solid filter at trace time)",
                            clip.brushes.len()
                        ));
                        Some(clip)
                    }
                    Err(e) => {
                        clip_report.push(format!("clipmap extract: {e}"));
                        None
                    }
                },
                None => {
                    clip_report.push("clipmap: not reached".into());
                    None
                }
            };
            let mut census = asset_world::build_glass_census(&stream, clip.as_ref());
            if leftover
                .iter()
                .any(|(intact, shattered)| !intact.is_empty() || !shattered.is_empty())
            {
                census.fx_def_materials = leftover.clone();
            }
            clip_report.push(census.report_line());
            let named = leftover
                .iter()
                .filter(|(a, b)| !a.is_empty() || !b.is_empty())
                .count();
            clip_report.push(format!(
            "fx glass def leftover: defs={} named={} (captured during load_fxworld; OFFSET walk is empty)",
            leftover.len(),
            named
        ));

            fx_glass = asset_world::build_fx_glass_reset(&stream).map(|mut g| {
                g.def_materials = leftover;
                g
            });
            dyn_ents = match stream.clip_map() {
                Some(geometry) => asset_world::build_dyn_ent_catalog(
                    &stream,
                    geometry,
                    |slot| map_xmodels.name_at_slot(slot).map(str::to_owned),
                    |slot| fx.name_at_slot(slot).map(str::to_owned),
                    |slot| phys_presets.at_slot(slot).cloned(),
                ),
                None => asset_world::DynEntCatalog::default(),
            };
            clip_stage.done();
        });
        report.extend(image_report.into_iter().flatten());
        report.extend(clip_report);
        let fx_glass = fx_glass.map(|mut g| {
            g.resolve_material_edges(&materials);
            g
        });
        match fx_glass.as_ref() {
            Some(g) => report.push(g.report_line()),
            None if stream.fx_world().is_some() => {
                report.push(
                    "fx glass reset: truncated (initPieceStates/initGeoData not kept)".into(),
                );
            }
            None => {}
        }
        if stream.clip_map().is_some() {
            report.push(dyn_ents.report_line());
        } else {
            report.push("dynents: clipmap not reached".into());
        }
        map_xmodels.set_dynent_phys_preset_n(dyn_ents.phys_preset_named_n());
        report.push(format!(
            "xmodel physPreset: slot={} named={} models={} (dyn-ent def named={} is a different graph)",
            map_xmodels.phys_preset_slot_n(),
            map_xmodels.phys_preset_name_hint_n(),
            map_xmodels.captured_model_n(),
            dyn_ents.phys_preset_named_n(),
        ));
        if let Some(gfx) = stream.gfx_world() {
            report.push(format!(
                "gfx dpvsDyn: sceneDynModel={} sceneDynBrush={} (RUNTIME tables; not DrawInst)",
                gfx.dyn_model_count, gfx.dyn_brush_count
            ));
        }
        let stage = progress.begin_scoped(StageId::MapAssets, "geometry", None);
        let Some(geometry) = stream.gfx_world() else {
            stage.fail();
            report.push("no GfxWorld reached — nothing to draw".into());
            push_mapents_key_census(&mut report, &stream);
            let dm_spawns = dm_spawn_points(&stream);
            drop(stream);
            let arena_bytes = memory.total_bytes();
            drop(memory);
            report.push(format!(
                "s1 arenas walked: map={arena_bytes} ({:.1}MiB) (ZoneMemory freed after the walk; ZoneImage dropped)",
                arena_bytes as f64 / (1024.0 * 1024.0),
            ));
            return LoadedWorld {
                scripts,
                sound: map_sound,
                materials,
                world: PreparedWorld {
                    fx: std::mem::take(&mut fx),
                    fx_models,
                    fx_glass,
                    impact_fx: impact_fx.take(),
                    dyn_ents,
                    exp_fog,
                    film_vision,
                    film_visions,
                    createart_name,
                    policy: WorldDrawPolicy::iw4(),
                    ..PreparedWorld::empty(WorldDrawPolicy::iw4())
                },
                collision: clip,
                spawns: dm_spawns,
                bodies,
                fpv_meshes,
                xanims: map_xanims,
                facts: crate::MapFacts {
                    compass,
                    script_sound,
                    ..Default::default()
                },
                arena_bytes,
                report,
                gaps: vec![LaneGap {
                    capability: PreparedCapability::PreparedWorld,
                    reason: "no GfxWorld reached — nothing to draw".into(),
                    addr: Some("assets::lane::iw4::load_world/no_gfx_world"),
                }],
            };
        };

        let world_draw = build_world_draw(&stream, geometry, materials);
        report.push(format!(
            "GfxLightDef map zone: table={light_def_table} bodies={light_def_bodies} recorded={}",
            stream.light_defs().len()
        ));
        stage.finish_from(&world_draw);
        match world_draw {
            Ok((draw, map_materials)) => {
                let stage = progress.begin_scoped(StageId::MapAssets, "models", None);
                let map_models = super::build_static_model_draw(&stream, geometry, map_xmodels);
                {
                    let n = draw.primary_lights.len();
                    let dir = draw
                        .primary_lights
                        .iter()
                        .filter(|l| l.light_type == lighting_iw4::GFX_LIGHT_TYPE_DIR)
                        .count();
                    let omni = draw
                        .primary_lights
                        .iter()
                        .filter(|l| l.light_type == lighting_iw4::GFX_LIGHT_TYPE_OMNI)
                        .count();
                    let spot = draw
                        .primary_lights
                        .iter()
                        .filter(|l| l.light_type == lighting_iw4::GFX_LIGHT_TYPE_SPOT)
                        .count();
                    let named = draw
                        .primary_lights
                        .iter()
                        .filter(|l| l.def_name.as_ref().is_some_and(|n| !n.is_empty()))
                        .count();
                    let falloff_w = draw
                        .primary_lights
                        .iter()
                        .filter(|l| l.falloff_image_width.is_some())
                        .count();
                    report.push(format!(
                        "primary lights: n={n} dir={dir} omni={omni} spot={spot} named_defs={named} falloff_width={falloff_w} (light-def name retained; atten_image filled after global absorb)"
                    ));
                }
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

                stage.done();
                let stage = progress.begin_scoped(StageId::MapAssets, "lighting", None);
                let light_grid =
                    asset_model::OwnedLightGrid::from_stream(&stream, geometry.light_grid);
                let smodel_lighting_samples = match &light_grid {
                    Some(grid) => smodel_lighting_samples(
                        &mut report,
                        grid,
                        asset_model::model_lighting::collect_smodel_lighting_origins(
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
                stage.done();
                let handoff = progress.begin_scoped(StageId::MapAssets, "handoff", None);
                let intermission_view = intermission_view(&stream);
                let minimap_corners = minimap_corners(&stream);
                let north_yaw = worldspawn_north_yaw(&stream);
                let airstrike_height = asset_world::airstrike_height(&stream);
                let dm_spawns = dm_spawn_points(&stream);
                push_mapents_key_census(&mut report, &stream);
                drop(stream);
                let arena_bytes = memory.total_bytes();
                drop(memory);
                report.push(format!(
                    "s1 arenas walked: map={arena_bytes} ({:.1}MiB) (ZoneMemory freed after the walk; ZoneImage dropped)",
                    arena_bytes as f64 / (1024.0 * 1024.0),
                ));
                report_ffa_spawns(&mut report, &dm_spawns);
                report.push(format!(
                    "world mesh: {} vertices, {} triangles, {} surfaces ({} skipped, {} sky)",
                    draw.stats.vertices,
                    draw.stats.triangles,
                    draw.stats.surfaces,
                    draw.stats.skipped_surfaces,
                    draw.stats.sky_surfaces
                ));
                report.push(format!(
                    "visibility: cells={} portals={} aabbNodes={} smodels={} sorted={} surfaceBounds={} smodelBounds={} skyStartSurfs={}",
                    geometry.cell_count,
                    geometry.portal_count,
                    geometry.aabb_node_count,
                    geometry.smodel_count,
                    draw.dpvs.sorted_surf_index.len(),
                    draw.dpvs.surface_bounds.len(),
                    draw.dpvs.smodel_bounds.len(),
                    draw.dpvs.sky_start_surfs.len()
                ));
                report.push(
                "draw path: DPVS portal walk + one AABB descent per visible cell into surfaceVisData/smodelVisData"
                    .into(),
            );
                report_intermission(&mut report, intermission_view.as_ref());
                report_world_batches(&mut report, &draw);
                let material_surfaces = draw
                    .surface_materials
                    .iter()
                    .filter(|material| material.is_some())
                    .count();
                report.push(format!(
                "material catalog: {}/{} surfaces resolved, {} materials, {} images, {} color bindings, {} normal bindings, {} alpha-test, {} blend, {} multiply, {} sky, {} capture gaps, {} agreed draw-mode, {} lit-band draw-mode conflicts, {} unresolved draw-mode",
                material_surfaces,
                draw.surface_materials.len(),
                map_materials.materials.len(),
                map_materials.images.len(),
                map_materials.color_binding_count(),
                map_materials.normal_binding_count(),
                map_materials.alpha_test_count(),
                map_materials.blend_count(),
                map_materials.multiply_count(),
                map_materials.sky_count(),
                map_materials.capture_gaps,
                map_materials.agreed_draw_mode_count(),
                map_materials.lit_band_draw_mode_conflict_count(),
                map_materials.unresolved_draw_mode_count(),
            ));
                report.push(format!(
                "material batches: {} compact meshes (material identity + lightmap state + primary light)",
                draw.batches.len()
            ));
                let mut sun_surfaces = 0usize;
                let mut bake_only_surfaces = 0usize;
                let mut local_light_surfaces = 0usize;
                for &(batch_index, _, _) in &draw.surface_batch_ranges {
                    let Some(batch) = draw.batches.get(batch_index) else {
                        continue;
                    };
                    if !batch.lightmapped || batch.primary_light_index == 0 {
                        bake_only_surfaces += 1;
                    } else if draw
                        .primary_lights
                        .get(usize::from(batch.primary_light_index))
                        .is_some_and(|light| light.is_sun)
                    {
                        sun_surfaces += 1;
                    } else {
                        local_light_surfaces += 1;
                    }
                }
                report.push(format!(
                "primary-light surfaces: {sun_surfaces} sun, {bake_only_surfaces} bake-only/fallback, {local_light_surfaces} local-light gap"
            ));
                match &draw.lightmap {
                    Ok(pages) => {
                        let decoded = pages.iter().flatten().count();
                        let names: Vec<&str> = pages
                            .iter()
                            .flatten()
                            .map(|p| p.ambient_source_name.as_str())
                            .collect();
                        report.push(format!(
                            "lightmap: {decoded}/{} pages decoded ({})",
                            pages.len(),
                            names.join(", ")
                        ));
                    }
                    Err(gap) => report.push(format!("lightmap gap: {gap}")),
                }
                let reflection_probe_images =
                    decode_reflection_probes(&mut report, &draw, &map_materials);
                report_dpvs(&mut report, &draw);
                let min = draw.stats.min;
                let max = draw.stats.max;
                let world_bounds = draw.stats.bounds;
                handoff.done();
                LoadedWorld {
                    scripts,
                    sound: map_sound,
                    materials: map_materials,
                    world: PreparedWorld {
                        source_namespace: Some(asset_core::AssetNamespace::Iw4),
                        draw: Some(draw),
                        dynamic_light: None,
                        static_model_meshes,
                        static_model_instances,
                        map_xmodel_scene_assets,
                        script_model_instances,
                        script_brush_models,
                        flag_descriptors,
                        script_structs,
                        dyn_ents,
                        smodel_lighting_samples,
                        light_grid,
                        fx,
                        fx_models,
                        fx_glass,
                        impact_fx,
                        reflection_probe_images,
                        intermission_view,
                        exp_fog,
                        t6_film_grade: None,
                        film_vision,
                        film_visions,
                        createart_name,
                        min,
                        max,
                        world_bounds,
                        policy: WorldDrawPolicy::iw4(),
                    },
                    collision: clip,
                    spawns: dm_spawns,
                    bodies,
                    fpv_meshes,
                    xanims: map_xanims,
                    facts: crate::MapFacts {
                        minimap_corners,
                        north_yaw,
                        airstrike_height,
                        compass,
                        script_sound,
                        ..Default::default()
                    },
                    arena_bytes,
                    report,
                    gaps: Vec::new(),
                }
            }
            Err(e) => {
                report.push(format!("world mesh: {e}"));
                let dm_spawns = dm_spawn_points(&stream);
                push_mapents_key_census(&mut report, &stream);
                drop(stream);
                let arena_bytes = memory.total_bytes();
                drop(memory);
                LoadedWorld {
                    scripts,
                    sound: map_sound,
                    materials: asset_material::MaterialCatalog::default(),
                    world: PreparedWorld {
                        fx,
                        fx_models,
                        fx_glass,
                        impact_fx,
                        dyn_ents,
                        exp_fog,
                        film_vision,
                        film_visions,
                        createart_name,
                        policy: WorldDrawPolicy::iw4(),
                        ..PreparedWorld::empty(WorldDrawPolicy::iw4())
                    },
                    collision: clip,
                    spawns: dm_spawns,
                    bodies,
                    fpv_meshes,
                    xanims: map_xanims,
                    facts: crate::MapFacts {
                        compass,
                        script_sound,
                        ..Default::default()
                    },
                    arena_bytes,
                    report,
                    gaps: vec![LaneGap {
                        capability: PreparedCapability::PreparedWorld,
                        reason: format!("world mesh: {e}"),
                        addr: Some("assets::lane::iw4::load_world/world_mesh"),
                    }],
                }
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
        let zone_name = path.file_stem().map_or_else(
            || "common_mp".to_owned(),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let header = match image.header() {
            Ok(header) => header,
            Err(error) => {
                return CommonCensus {
                    report: vec![format!("common_mp models: zone header: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut memory = ZoneMemory::for_header(&header);
        let mut stream = match memory.stream(&image.bytes) {
            Ok(stream) => stream,
            Err(error) => {
                return CommonCensus {
                    report: vec![format!("common_mp models: zone arenas: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut report_arenas = vec![asset_transport::xfile_arena_row(
            "zone arenas common_mp",
            &header.block_size,
            fastfile_iw4::XFILE_BLOCK_TEMP,
            fastfile_iw4::XFILE_BLOCK_VIRTUAL,
        )];
        let mut sink = CommonWalkSink::with_stage(progress.begin_scoped(
            StageId::CommonAssets,
            zone_name.clone(),
            None,
        ));
        sink.seed_materials(material_seed);
        sink.set_capture_zone(asset_core::ZoneOwner::intern(&zone_name));
        sink.set_capture_ns(asset_core::AssetNamespace::Iw4);
        sink.sound = asset_audio::ZoneSoundCapture::claim_common(
            path,
            asset_audio::ZoneGame::Iw4,
            "common census",
        );
        let walk = load_zone(&mut stream, &mut sink);
        if let Some(sound) = sink.sound.take() {
            sound.deposit(walk.as_ref().map(|_| ()).map_err(|e| e.to_string()));
        }
        if let Some(stage) = sink.stage.take() {
            stage.finish_from(&walk);
        }
        let mut report = sink.models.report("common_mp");
        report.splice(0..0, report_arenas.drain(..));
        if let Err(error) = walk {
            report.push(format!(
                "common_mp model walk: stopped after {} assets — {error}",
                sink.walked
            ));
        }
        report.push(format!(
            "common_mp model pointer drift: {} unsettled offsets",
            stream.unsettled_offsets()
        ));
        report.push(format!(
            "s2 common walk: reuse_mat={} reuse_img={} pool_mat={} pool_img={}",
            sink.materials.link_reused_materials,
            sink.materials.link_reused_images,
            sink.materials.materials.len(),
            sink.materials.images.len(),
        ));
        let captured = sink.weapons.len();

        sink.weapons.resolve_reticles(&sink.materials);
        let projectile_keys = sink.weapons.projectile_model_hints();
        let pending_unclassified = sink.projectile_meshes.len();
        sink.projectile_meshes.keep_referenced(&projectile_keys);
        for key in &projectile_keys {
            if sink.projectile_meshes.contains(key.namespace, &key.name) {
                continue;
            }
            if let Some(gun) = sink.world_weapons.get(key.namespace, &key.name) {
                sink.projectile_meshes.absorb_world_weapon(gun);
            }
        }
        let graph = crate::resolve_after_absorb(
            &sink.materials,
            &mut sink.tracers,
            &mut sink.fx,
            Some(&mut sink.weapons),
            None,
            Some(&mut sink.world_weapons),
            None,
            Some(&mut sink.fpv_meshes),
            Some(&mut sink.projectile_meshes),
        );
        let mut weapons = sink.weapons.into_build();
        weapons.apply_stats_tables(sink.stats_tables.values());
        report.push(format!(
            "common_mp statsTable: tables={} item_groups={}",
            sink.stats_tables.len(),
            weapons.item_group_count(),
        ));
        weapons.stamp_projectile_model_edges(
            &sink.projectile_meshes,
            asset_core::ZoneOwner::intern(&zone_name),
        );
        weapons.resolve_sz_xanim_edges(&sink.xanims);
        weapons.resolve_fpv_mesh_edges(&sink.fpv_meshes);
        weapons.resolve_world_model_edges(&sink.world_weapons);
        let gun_named = weapons.gun_xmodel_count();
        let (camo_weapons, camo_view, camo_world) = weapons.camo_census();
        report.push(format!(
            "common_mp camouflage: {camo_weapons} weapons; {camo_view} view and {camo_world} world models bound"
        ));
        report.push(format!(
        "common_mp weapons: {captured} captures → {} unique catalog ids (sorted); {gun_named} with gunXModel[0]; {} with szXAnims[IDLE]; {} with any szXAnims slot",
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
            "common_mp weapons worldModel[0]: {}",
            weapons.world_model_count(),
        ));
        report.push(format!(
            "common_mp projectileModel: slot={} bound={} unresolved_hint={} (pending unclassified XModels {}, retained {})",
            weapons.projectile_model_count(),
            weapons.projectile_model_bound_n(),
            weapons.projectile_model_name_hint_n(),
            pending_unclassified,
            sink.projectile_meshes.len()
        ));
        report.push(sink.projectile_meshes.report_line());
        report.push(format!(
            "common_mp weapon HUD Material*: bound={} unresolved={} absent={}",
            graph.weapon_hud_materials.bound,
            graph.weapon_hud_materials.unresolved,
            graph.weapon_hud_materials.absent,
        ));
        report.push(format!(
            "common_mp weapon projectile FX: bound={} unresolved={} absent={}",
            graph.weapon_projectile_fx.bound,
            graph.weapon_projectile_fx.unresolved,
            graph.weapon_projectile_fx.absent,
        ));
        let sz_xanims = weapons.sz_xanim_edge_census();
        report.push(format!(
            "common_mp weapon szXAnims: bound={} unresolved={} absent={}",
            sz_xanims.bound, sz_xanims.unresolved, sz_xanims.absent,
        ));
        report.push(format!(
            "common_mp TracerDef: {} named ({} bound, {} unresolved); weapon tracerType bound={} unresolved={} absent={}; weapon FxEffectDef* bound={} unresolved={} absent={}; nested fx_child bound={} unresolved={} absent={}; fx_runner bound={} unresolved={} absent={}; world-gun materialHandles bound={} unresolved={} absent={}; FPV materialHandles bound={} unresolved={} absent={}",
            sink.tracers.len(),
            graph.tracer_materials.bound,
            graph.tracer_materials.unresolved,
            graph.weapon_tracer_type.bound,
            graph.weapon_tracer_type.unresolved,
            graph.weapon_tracer_type.absent,
            graph.weapon_combat_fx.bound,
            graph.weapon_combat_fx.unresolved,
            graph.weapon_combat_fx.absent,
            graph.fx_nested_children.bound,
            graph.fx_nested_children.unresolved,
            graph.fx_nested_children.absent,
            graph.fx_runner_children.bound,
            graph.fx_runner_children.unresolved,
            graph.fx_runner_children.absent,
            graph.xmodel_gun_materials.bound,
            graph.xmodel_gun_materials.unresolved,
            graph.xmodel_gun_materials.absent,
            graph.xmodel_fpv_materials.bound,
            graph.xmodel_fpv_materials.unresolved,
            graph.xmodel_fpv_materials.absent,
        ));
        report.push(format!(
            "common_mp TracerDef materials: {}",
            sink.tracers
                .defs()
                .map(|t| format!("{:?}/{}={}", t.namespace, t.name, t.material_report()))
                .collect::<Vec<_>>()
                .join(" ")
        ));
        if sink.pen_table.is_some() {
            report.push("common_mp bullet_penetration_mp: loaded (4x31)".into());
        } else {
            report.push(
                "common_mp bullet_penetration_mp: missing — FirePenetrate depths are 0".into(),
            );
        }
        if sink.lochit_table.is_some() {
            report.push("common_mp mp_lochit_dmgtable: loaded (20)".into());
        } else {
            report
                .push("common_mp mp_lochit_dmgtable: missing — location scale is identity".into());
        }
        report.extend(sink.world_weapons.report_lines());
        report.push(format!(
            "common_mp XAnim catalog: {} clips captured ({} gaps)",
            sink.xanims.len(),
            sink.xanims.capture_gaps
        ));
        report.push(format!(
            "common_mp player animation sources: atr={} script={} types={} decode_errors={}",
            sink.player_anim_sources.multiplayer_atr().is_some(),
            sink.player_anim_sources.playeranim_script().is_some(),
            sink.player_anim_sources.playeranim_types().is_some(),
            sink.player_anim_sources.decode_errors().len()
        ));
        report.push(format!(
            "common_mp fx catalog: {} FxEffectDef ({} gaps)",
            sink.fx.len(),
            sink.fx.capture_gaps
        ));
        let light_defs = asset_world::capture_light_defs(&stream, &sink.materials);
        report.push(format!(
            "GfxLightDef common_mp: table={} bodies={} recorded={}",
            sink.light_def_table,
            sink.light_def_bodies,
            light_defs.len()
        ));
        drop(stream);
        let s1_common_bytes = memory.total_bytes();
        drop(memory);
        report.push(format!(
            "s1 arenas walked: common={s1_common_bytes} ({:.1}MiB) (ZoneMemory freed after the walk; ZoneImage dropped)",
            s1_common_bytes as f64 / (1024.0 * 1024.0),
        ));
        let impact_fx = sink.impact_fx.take_table();
        if let Some(ref table) = impact_fx {
            report.push(format!(
                "common_mp impactfx: table `{}` rows={} gaps={}",
                table.name,
                table.row_count(),
                table.capture_gaps
            ));
        } else {
            report.push("common_mp impactfx: no table captured".into());
        }
        let fpv_meshes = sink.fpv_meshes;
        sink.materials.resolve_technique_set_edges();
        if decode_color_maps {
            let mut material_population = sink.materials;

            let stage = progress.begin_scoped(StageId::Images, "common_mp", None);
            let (inline, mut plan) = asset_material::material_images::plan_material_color_maps(
                path,
                &mut material_population,
                &stage,
                crate::session_load::load_pool(),
            );
            report.push(format!(
                "common_mp materials: {} claimed for the merged pool, {} in-zone bodies decoded here ({} missing, {} unsupported)",
                plan.len(),
                inline.decoded,
                inline.missing,
                inline.unsupported
            ));
            let tracer_inline = asset_material::material_images::plan_images_for_keys(
                &mut plan,
                &mut material_population,
                sink.tracers.material_keys(),
                &stage,
                crate::session_load::load_pool(),
            );
            report.push(format!(
                "common_mp tracer beam images: {tracer_inline} in-zone images decoded, rest claimed"
            ));
            let fx_inline = asset_material::material_images::plan_images_for_keys(
                &mut plan,
                &mut material_population,
                sink.fx.unique_material_keys(),
                &stage,
                crate::session_load::load_pool(),
            );
            report.push(format!(
                "common_mp fx elem 2d images: {fx_inline} in-zone images decoded, rest claimed"
            ));
            // The planning phase is over and it succeeded: what is left of the
            // plan is decoded later, under its own stage. Dropping the handle
            // here would be recorded as an interrupted stage, which is what a
            // load that was cut short looks like.
            stage.done();
            let pending_images = Some(plan);
            {
                let unique: std::collections::BTreeSet<String> =
                    sink.tracers.named_materials().map(str::to_owned).collect();
                for name in unique {
                    let bind = asset_game::material_bind_name(&name);
                    let twins: Vec<&str> = material_population
                        .materials
                        .iter()
                        .filter(|m| m.name.as_str() == bind)
                        .map(|m| m.name.as_str())
                        .collect();
                    let images: Vec<&str> = material_population
                        .images
                        .iter()
                        .filter(|img| img.name.as_str() == bind)
                        .map(|img| img.name.as_str())
                        .collect();
                    report.push(format!(
                        "common_mp tracer material `{name}` twins={twins:?} images={images:?}"
                    ));
                    for mat in material_population
                        .materials
                        .iter()
                        .filter(|m| m.name.as_str() == bind)
                    {
                        let sem: Vec<u8> = mat.textures.iter().map(|t| t.semantic).collect();
                        let decoded = mat.textures.iter().any(|t| {
                            t.image
                                .and_then(|i| material_population.images.get(i))
                                .is_some_and(|img| img.decoded.is_some())
                        });
                        report.push(format!(
                        "common_mp tracer twin `{}` techset={} camera_region={} tex={} sem={sem:?} decoded={decoded}",
                        mat.name,
                        mat.technique_set,
                        mat.camera_region,
                        mat.textures.len(),
                    ));
                    }
                }
            }

            let tracer_named = sink.tracers.named_materials().count();
            report.push(format!(
            "common_mp fx color maps: 0 cloned ({tracer_named} tracer names Bound into global; no pool clone)"
        ));
            let moved = material_population.image_memory();
            report.push(moved.report_row("image memory common_mp population"));
            report.push(format!(
            "image memory common_mp population: bytes={} ({:.1}MiB) (moved into global at absorb)",
            moved.total_bytes(),
            moved.total_bytes() as f64 / (1024.0 * 1024.0),
        ));
            CommonCensus {
                pending_images,
                scene_models: sink.scene_models,
                shared_surfaces: sink.shared_surfaces,
                weapons,
                cac_tables: sink.stats_tables.into_values().collect(),
                fpv: fpv_meshes,
                world_weapons: sink.world_weapons,
                projectile_meshes: sink.projectile_meshes,
                xanims: sink.xanims,
                player_anim_sources: sink.player_anim_sources,
                fx: sink.fx,
                fx_models: sink.fx_models,
                tracers: sink.tracers,
                impact_fx,
                material_population,
                light_defs,
                report,
                pen_table: sink.pen_table.unwrap_or_default(),
                pen_table_loaded: sink.pen_table.is_some(),
                lochit_table: sink.lochit_table,
                xmodel_walk: sink.models.walk_census(),
                s1_common_bytes,
                teamsets: std::collections::HashMap::new(),
                scripts: sink.scripts,
                film_visions: sink.film_visions,
                preparation: None,
            }
        } else {
            let memory = sink.materials.image_memory();
            report.push(format!(
                "startup material population: zone={zone_name} materials={} images={} decoded={} payload_bytes={} (IWD decode deferred to absorb merge; not common_mp FPV decode)",
                sink.materials.materials.len(),
                memory.images,
                memory.decoded_images,
                memory.payload_bytes,
            ));
            CommonCensus {
                pending_images: None,
                scene_models: sink.scene_models,
                shared_surfaces: sink.shared_surfaces,
                weapons,
                cac_tables: sink.stats_tables.into_values().collect(),
                fpv: fpv_meshes,
                world_weapons: sink.world_weapons,
                projectile_meshes: sink.projectile_meshes,
                xanims: sink.xanims,
                player_anim_sources: sink.player_anim_sources,
                fx: sink.fx,
                fx_models: sink.fx_models,
                tracers: sink.tracers,
                impact_fx,
                material_population: sink.materials,
                light_defs,
                report,
                pen_table: sink.pen_table.unwrap_or_default(),
                pen_table_loaded: sink.pen_table.is_some(),
                lochit_table: sink.lochit_table,
                xmodel_walk: sink.models.walk_census(),
                s1_common_bytes,
                teamsets: std::collections::HashMap::new(),
                scripts: sink.scripts,
                film_visions: sink.film_visions,
                preparation: None,
            }
        }
    }

    fn load_material_population(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,
        material_seed: asset_material::MaterialCatalog,
    ) -> MaterialPopulation {
        let zone_name = path.file_stem().map_or_else(
            || "startup".to_owned(),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let header = match image.header() {
            Ok(header) => header,
            Err(error) => {
                return MaterialPopulation {
                    materials: material_seed,
                    report: vec![format!("startup materials: zone header: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut memory = ZoneMemory::for_header(&header);
        let mut stream = match memory.stream(&image.bytes) {
            Ok(stream) => stream,
            Err(error) => {
                return MaterialPopulation {
                    materials: material_seed,
                    report: vec![format!("startup materials: zone arenas: {error}")],
                    ..Default::default()
                };
            }
        };
        let mut sink = MaterialPopulationSink::with_stage(progress.begin_scoped(
            StageId::CommonAssets,
            zone_name.clone(),
            None,
        ));
        sink.seed_materials(material_seed);
        sink.set_capture_zone(asset_core::ZoneOwner::intern(&zone_name));
        sink.set_capture_ns(asset_core::AssetNamespace::Iw4);
        sink.sound = asset_audio::ZoneSoundCapture::claim_common(
            path,
            asset_audio::ZoneGame::Iw4,
            "material population",
        );
        let walk = load_zone(&mut stream, &mut sink);
        if let Some(sound) = sink.sound.take() {
            sound.deposit(walk.as_ref().map(|_| ()).map_err(|e| e.to_string()));
        }
        if let Some(stage) = sink.stage.take() {
            stage.finish_from(&walk);
        }
        let mut report = Vec::new();
        if let Err(error) = walk {
            report.push(format!(
                "startup materials: walk stopped after {} assets — {error}",
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
            light_defs: asset_world::capture_light_defs(&stream, &sink.materials),
            materials: sink.materials,
            report,
            cac_tables: sink.stats_tables.into_values().collect(),
            scripts: sink.scripts,
        }
    }
}

fn push_mapents_key_census(report: &mut Vec<String>, stream: &fastfile_iw4::ZoneStream<'_>) {
    match map_ents_entity_string(stream) {
        Some(text) => report.push(census_entity_string_keys(text).report_line()),
        None => report.push("mapents keys=none".into()),
    }
}

fn decode_map_material_images(
    path: &Path,
    catalog: &mut asset_material::MaterialCatalog,
    stage: asset_transport::progress::StageHandle,
    fx_material_keys: Vec<asset_core::MaterialKey>,
    glass_names: Vec<String>,
) -> Vec<String> {
    let mut report = Vec::new();
    match decode_material_color_maps(path, catalog, &stage, crate::session_load::load_pool()) {
        Ok(stats) => {
            report.push(format!(
                "IWD color/normal maps: {}/{} decoded, {} missing, {} unsupported from {} archives",
                stats.decoded, stats.requested, stats.missing, stats.unsupported, stats.archives
            ));
            if let Some(gap) = stats.first_gap {
                report.push(format!("IWD material-map gap: {gap}"));
            }
            if let Some(gap) = stats.first_unsupported {
                report.push(format!("IWD material-map unsupported: {gap}"));
            }
        }
        Err(error) => report.push(format!("IWD material-map gap: {error}")),
    }
    let mut keys = fx_material_keys;
    keys.extend(glass_names.iter().map(|name| asset_core::MaterialKey {
        namespace: asset_core::AssetNamespace::Iw4,
        name: name.clone(),
    }));
    match asset_material::material_images::decode_images_for_keys(
        path,
        catalog,
        keys,
        &stage,
        crate::session_load::load_pool(),
    ) {
        Ok(n) => report.push(format!(
            "fx elem 2d images: {n} decoded (unique Bound names)"
        )),
        Err(error) => report.push(format!("fx elem 2d image gap: {error}")),
    }

    if !glass_names.is_empty() {
        report.push(format!(
            "fx glass def leftover: names={} (NameHint; color stays in catalog, not a clone sidecar)",
            glass_names.len()
        ));
    }
    report.push(
        "image memory map fx color maps: n=0 bytes=0 (Bound into catalog; no clone sidecar)".into(),
    );
    stage.done();
    report
}

fn select_film_vision(
    name: Option<&str>,
    map: &super::FilmVisionCatalog,
    common: &super::FilmVisionCatalog,
) -> Result<Option<(asset_world::FilmVision, &'static str)>, String> {
    let name = name.ok_or_else(|| "map path has no UTF-8 stem".to_owned())?;
    let candidate = map
        .get(name)
        .map(|vision| (vision, "fastfile"))
        .or_else(|| common.get(name).map(|vision| (vision, "common_mp")));
    match candidate {
        Some((Ok(vision), source)) => Ok(Some((*vision, source))),
        Some((Err(error), source)) => Err(format!("{source} parse error {error:?}")),
        None => Ok(None),
    }
}
