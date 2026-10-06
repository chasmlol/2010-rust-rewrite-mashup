use super::*;

static NEXT_PRODUCTS_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub(super) async fn walk_prepared_match(
    zone_ff: Result<PathBuf, String>,
    common_mp: Result<PathBuf, String>,
    progress: LoadProgress,
) -> (MatchLoadOutcome, Option<Arc<CommonSet>>) {
    let zone_name = zone_ff
        .as_ref()
        .ok()
        .and_then(|path| path.file_stem())
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();

    let pool = load_pool();
    let map_open = {
        let zone_ff = zone_ff.clone();
        let progress = progress.clone();
        pool.spawn(async move {
            let path = match zone_ff {
                Ok(path) => path,
                Err(error) => {
                    progress.record_skipped_scoped(StageId::MapAssets, "open");
                    return Err(format!("zone not found: {error}"));
                }
            };
            let stage = progress.begin_scoped(StageId::MapAssets, "open", None);
            progress.begin_zone_open();
            let opened = open_zone_shared(&path);
            finish_zone_open(stage, &opened);
            let image = opened.map_err(|error| format!("open zone: {error}"))?;
            progress.record_zone_image_bytes(image.bytes.len());
            Ok::<_, String>((path, image))
        })
    };

    let mut donor_report = Vec::new();
    let key = CommonKey::for_match(
        zone_ff.as_ref().ok().map(PathBuf::as_path),
        common_mp.as_ref().ok().map(PathBuf::as_path),
        &mut donor_report,
    );
    let waiting = progress.begin_scoped(StageId::CommonAssets, "shared common", None);
    let (common, reach) = ensure_common(key).await;
    waiting.done();
    if progress.is_canceled() {
        diag::info!(
            World,
            "match walk: canceled while the common set was prepared — load retargeted"
        );
        return (MatchLoadOutcome::Canceled, None);
    }

    let cloning = std::time::Instant::now();
    let CommonProducts {
        scripts: mut iw4_scripts,
        mut t5_scene_models,
        material_seed,
        shared_surfaces,
        scene_models: mut common_scene_models,
        light_defs: common_light_defs,
        pen_table: common_pen_table,
        pen_table_loaded: common_pen_loaded,
        lochit_table: common_lochit_table,
        tracers: mut common_tracers,
        xmodel_walk,
        s1_common_bytes,
        teamsets,
        film_visions: common_film_visions,
        mut weapons,
        mut fpv_meshes,
        mut world_weapons,
        mut projectile_meshes,
        mut xanims,
        mut player_anim_sources,
        fx: common_fx,
        fx_models: common_fx_models,
        impact_fx: common_impact,
        t5_xanims,
        t5_fx,
        t5_impact_fx,
        iw5_materials,
        mut iw5_scene_models,
        iw5_shared_surfaces,
        strings,
        counts:
            CommonCounts {
                startup_count,
                t5_mat_count,
                t5_reuse_mat,
                t5_xanim_n,
                foreign_count,
                seed_mat,
                seed_img,
                common_reuse_mat,
                common_reuse_img,
            },
        report: mut common_report,
        localize_report,
    } = common.products.clone();
    let clone_ms = cloning.elapsed().as_secs_f32() * 1000.0;
    let iw5_mat_n = iw5_materials.materials.len();
    common_report.append(&mut donor_report);
    common_report.push(format!(
        "common set: {reach} for {} (prepared in {:.0}ms, {:.1}s ago); match copy {clone_ms:.0}ms; {} donor image batches and {} kept payloads ({:.1}MiB) shared, not decoded again",
        common.key,
        common.prepared_ms,
        common.ready_at.elapsed().as_secs_f32(),
        common.donor_batches(),
        common.retained_payloads(),
        common.retained_bytes() as f64 / (1024.0 * 1024.0),
    ));
    let common_images = hold_image_plan(
        "common_mp FPV",
        common.fpv_plan.clone(),
        load_jobs::open(JobKind::ImageDecode).namespace("iw4"),
    );

    let opened_map = map_open.await;
    if progress.is_canceled() {
        drop(opened_map);
        diag::info!(
            World,
            "match walk: canceled before the map walk — load retargeted"
        );
        return (MatchLoadOutcome::Canceled, None);
    }
    let (loaded, map_namespace) = match opened_map {
        Ok((path, image)) => {
            let game = image.game;
            let shared_surfaces = match game {
                asset_core::ZoneGame::Iw5 => iw5_shared_surfaces,
                _ => shared_surfaces,
            };
            (
                lane(game).load_world(
                    &path,
                    &image,
                    &progress,
                    shared_surfaces,
                    material_seed,
                    &common_film_visions,
                ),
                Some(asset_core::AssetNamespace::from_zone_game(game)),
            )
        }
        Err(gap) => {
            return (MatchLoadOutcome::Refused(gap), Some(common));
        }
    };

    let Some(map_family) = map_namespace else {
        return (
            MatchLoadOutcome::Refused("map family missing".into()),
            Some(common),
        );
    };
    let LoadedWorld {
        scripts: map_scripts,
        mut world,
        mut materials,
        collision: clip,
        spawns: dm_spawns,
        mut bodies,
        fpv_meshes: map_fpv,
        xanims: map_xanims,
        mut facts,
        arena_bytes: s1_map_bytes,
        sound,
        mut report,
        gaps,
    } = loaded;
    world.source_namespace = map_namespace;
    report.append(&mut common_report);

    if facts.team_settings.allies.is_none() && facts.team_settings.axis.is_none() {
        if let Some(name) = facts.t5_teamset.as_ref() {
            if let Some(icons) = teamsets.get(name) {
                facts.team_settings = icons.clone();
            }
        }
    }
    if facts.t5_teamset.is_some() {
        facts.script_sound.attackers = facts
            .script_sound
            .attackers
            .or_else(|| facts.team_settings.attackers.clone());
        facts.script_sound.defenders = facts
            .script_sound
            .defenders
            .or_else(|| facts.team_settings.defenders.clone());
    }
    if map_namespace == Some(asset_core::AssetNamespace::T5) {
        facts.objective_visuals = asset_game::ObjectiveVisuals::t5(
            facts.team_settings.allies_charset.as_deref(),
            facts.team_settings.axis_charset.as_deref(),
        );
    }
    match (
        facts.t5_teamset.as_deref(),
        facts.team_settings.allies.as_ref(),
        facts.team_settings.axis.as_ref(),
    ) {
        (Some(ts), Some(a), Some(x)) => {
            report.push(format!("team icons: teamset={ts} allies={a} axis={x}"));
        }
        (Some(ts), ..) => {
            report.push(format!(
                "team icons gap: teamset={ts} but common_mp had no matching _teamset_*.gsc icons"
            ));
        }
        _ => {}
    }

    if matches!(
        map_namespace,
        Some(
            asset_core::AssetNamespace::T5
                | asset_core::AssetNamespace::Iw5
                | asset_core::AssetNamespace::T6
        )
    ) && !foreign_faction_rows(&mut iw4_scripts, &facts.team_settings)
    {
        report.push(format!(
            "team rows gap: {FACTION_TABLE} lacks {IW4_ALLIES_CHARSET}/{IW4_AXIS_CHARSET}; IW4 scripts show IW4 teams"
        ));
    }
    let map_scripts = match map_namespace {
        Some(asset_core::AssetNamespace::T5 | asset_core::AssetNamespace::T6) => {
            let scripts = t5_map_under_iw4_rules(&map_scripts, &zone_name, &facts);
            report.push(format!(
                "map script: maps/mp/{zone_name} written from the T5 map's declarations; T5 map scripts left out"
            ));
            scripts
        }
        _ => map_scripts,
    };
    let mut scripts = iw4_scripts;
    scripts.overlay(map_scripts);
    scripts.insert_source(
        "iw4l_t6/equipment",
        crate::map_scripts::T6_EQUIPMENT.to_owned(),
    );
    report.push(format!(
        "GSC source assets: {} (map overrides common_mp)",
        scripts.len()
    ));

    report.push(format!(
        "map teams: allies={:?} axis={:?} attackers={:?} defenders={:?}",
        facts.team_settings.allies_name,
        facts.team_settings.axis_name,
        facts.script_sound.attackers,
        facts.script_sound.defenders,
    ));

    report.push(format!(
        "s1 pool walked: common={s1_common_bytes} map={s1_map_bytes} total={} rss={}",
        s1_common_bytes.saturating_add(s1_map_bytes),
        asset_transport::process_resident_bytes().unwrap_or(0),
    ));

    let common_xanim_count = xanims.len();
    let map_xanim_count = map_xanims.len();
    let t5_xanim_added = xanims.absorb(t5_xanims);
    xanims.absorb_local(map_xanims);
    weapons.resolve_sz_xanim_edges(&xanims);
    let weapon_clip_indices = weapons.bound_weapon_xanim_indices();
    let clip_prewarm_started = std::time::Instant::now();
    let failed_weapon_clips: Vec<_> = weapon_clip_indices
        .iter()
        .copied()
        .filter(|&index| xanims.clip_at(index).is_none())
        .collect();
    report.push(format!(
        "weapon XAnim CPU prewarm: linked={} decoded={} failed={} elapsed_ms={:.1}",
        weapon_clip_indices.len(),
        weapon_clip_indices.len() - failed_weapon_clips.len(),
        failed_weapon_clips.len(),
        clip_prewarm_started.elapsed().as_secs_f64() * 1000.0,
    ));
    for index in failed_weapon_clips.iter().take(16) {
        report.push(format!(
            "weapon XAnim decode gap: index={index} name={}",
            xanims.name_at(*index).unwrap_or("<unknown>")
        ));
    }
    let t6_alt_raises = weapons.time_t6_alternate_raises(&xanims);
    report.push(format!(
        "T6 alternate raises timed by their clips: {t6_alt_raises}"
    ));
    let (note_actions, inline_note_actions) = weapons.resolve_notetrack_actions(&xanims);
    report.push(format!(
        "weapon notetrack actions linked: {note_actions} ({inline_note_actions} T5 inline)"
    ));
    let sz_xanims = weapons.sz_xanim_edge_census();
    report.push(format!(
        "XAnim generation: common={common_xanim_count} t5={t5_xanim_n} t5_keys={t5_xanim_added} collide={} map={map_xanim_count} merged={}",
        xanims.collide_name_count(),
        xanims.len()
    ));
    report.push(format!(
        "weapon szXAnims after absorb: bound={} unresolved={} absent={}",
        sz_xanims.bound, sz_xanims.unresolved, sz_xanims.absent,
    ));
    player_anim_sources.bind_leaves(&xanims);
    report.push(player_anim_sources.bind_report_line());
    let gap_lines: Vec<String> = gaps
        .iter()
        .map(|gap| match gap.addr {
            Some(addr) => format!("lane gap [{addr}]: {}", gap.reason),
            None => format!("lane gap: {}", gap.reason),
        })
        .collect();
    report.extend(gap_lines.iter().cloned());
    report.extend(bodies.report_lines());
    let map_fpv_n = map_fpv.len();
    let map_fpv_added = fpv_meshes.absorb(map_fpv);
    fpv_meshes.set_map_namespace(map_namespace);
    weapons.resolve_fpv_mesh_edges(&fpv_meshes);
    weapons.resolve_fpv_hands(&fpv_meshes, &bodies);

    world_weapons.seal_identity();
    weapons.resolve_world_model_edges(&world_weapons);
    let world_model_edges = weapons.world_model_edge_census();
    report.push(format!(
        "world gun generation: catalog={}; worldModel edges bound={} unresolved={} absent={}",
        world_weapons.len(),
        world_model_edges.bound,
        world_model_edges.unresolved,
        world_model_edges.absent,
    ));
    report.push(format!(
        "FPV map fanout: map={map_fpv_n} added={map_fpv_added} merged={} map_ns={map_namespace:?}",
        fpv_meshes.len()
    ));

    let before_unrouted = materials.unrouted_material_count();
    let promoted = materials.promote_iw5_fallback_tables();
    let t5_alias_map = materials.absorb_t5_feature_token_donors();
    let stub_routed = materials.reroute_stub_materials();
    let after_unrouted = materials.unrouted_material_count();
    let takes_ml = materials
        .materials
        .iter()
        .filter(|m| materials.takes_model_lighting(m) == Some(true))
        .count();
    let t5_fb = materials
        .technique_set_facts()
        .iter()
        .filter(|facts| facts.t5_fallback_table.is_some())
        .count();
    report.push(format!(
        "material route: iw5_promoted={promoted} \
             t5_tech_alias={t5_alias_map} t5_fallback={t5_fb} stub_routed={stub_routed} \
             unrouted {before_unrouted}→{after_unrouted}; takes_model_lighting={takes_ml}"
    ));

    let map_reuse_mat = materials.link_reused_materials;
    let map_reuse_img = materials.link_reused_images;
    let pool_mat = materials.materials.len();
    let pool_img = materials.images.len();
    report.push(format!(
            "s2 pool: seed_mat={seed_mat} seed_img={seed_img} common_reuse_mat={common_reuse_mat} common_reuse_img={common_reuse_img} map_reuse_mat={map_reuse_mat} map_reuse_img={map_reuse_img} pool_mat={pool_mat} pool_img={pool_img}"
        ));
    let map_new = pool_mat.saturating_sub(seed_mat);

    let mut global = materials;
    let iw5_linked = global.absorb_asset_population_host_materials_win(iw5_materials);
    let provisional_map_ids: Vec<usize> = (0..global.materials.len()).collect();
    if iw5_mat_n > 0 {
        iw5_scene_models.remap_walk_materials(&iw5_linked);
    }
    let mut scene_names = scripts.asset_names();
    scene_names.extend(facts.objective_visuals.model_names().map(str::to_owned));
    common_scene_models.retain_names(&scene_names);
    t5_scene_models.retain_names(&scene_names);
    iw5_scene_models.retain_names(&scene_names);
    match map_namespace {
        Some(asset_core::AssetNamespace::T5) => world
            .map_xmodel_scene_assets
            .absorb_captured(t5_scene_models),
        Some(asset_core::AssetNamespace::Iw5) => world
            .map_xmodel_scene_assets
            .absorb_captured(iw5_scene_models),
        _ => {}
    }
    world
        .map_xmodel_scene_assets
        .absorb_captured(common_scene_models);
    report.push(format!(
        "iw5 leftover materials: donor={iw5_mat_n} linked={} pool={}",
        iw5_linked.len(),
        global.materials.len(),
    ));
    let promoted = global.promote_iw5_fallback_tables();
    let t5_alias = global.absorb_t5_feature_token_donors();
    global.reroute_stub_materials();
    let mat_refs = global.material_ref_census();
    let img_refs = global.image_ref_census();
    let shader_refs = global.shader_ref_census();
    let decl_refs = global.vertex_decl_ref_census();
    report.push(format!(
            "asset_ref before finalize: materials n={} real={} reference={}; images n={} real={} reference={}; shaders n={} real={} reference={}; decls n={} real={} reference={}",
            mat_refs.n, mat_refs.real, mat_refs.reference,
            img_refs.n, img_refs.real, img_refs.reference,
            shader_refs.n, shader_refs.real, shader_refs.reference,
            decl_refs.n, decl_refs.real, decl_refs.reference,
        ));
    crate::bot_model::install_local_bot_materials(&mut global);
    let (mut global, finalized_ids) = global.publish();
    let map_ids = provisional_map_ids
        .into_iter()
        .map(|id| finalized_ids.get(id).copied().flatten())
        .collect::<Vec<_>>();
    let t5_fb = global
        .technique_set_facts()
        .iter()
        .filter(|facts| facts.t5_fallback_table.is_some())
        .count();
    report.push(format!(
            "material generation: startup={startup_count} t5={t5_mat_count} t5_reuse={t5_reuse_mat} foreign={foreign_count} common_pool={seed_mat} map_new={map_new} pooled={pool_mat} global={} common_reuse={common_reuse_mat} map_reuse={map_reuse_mat} unresolved_aliases={} iw5_promoted={promoted} t5_tech_alias={t5_alias} t5_fallback={t5_fb}",
            global.materials.len(),
            finalized_ids.iter().filter(|id| id.is_none()).count(),
        ));
    let mat_refs = global.material_ref_census();
    let img_refs = global.image_ref_census();
    let shader_refs = global.shader_ref_census();
    let decl_refs = global.vertex_decl_ref_census();
    report.push(format!(
            "asset_ref after finalize: materials n={} real={} reference={}; images n={} real={} reference={}; shaders n={} real={} reference={}; decls n={} real={} reference={}",
            mat_refs.n, mat_refs.real, mat_refs.reference,
            img_refs.n, img_refs.real, img_refs.reference,
            shader_refs.n, shader_refs.real, shader_refs.reference,
            decl_refs.n, decl_refs.real, decl_refs.reference,
        ));

    let global_memory = global.image_memory();
    report.push(global_memory.report_row("image memory global generation"));
    report.push(format!(
        "canonical materials after absorb: n={} images={} decoded={}",
        global.materials.len(),
        global_memory.images,
        global_memory.decoded_images,
    ));
    let shader_census = global.shader_source_census();
    report.push(format!(
        "shader source corpus: programs={} unresolved_aliases={} byteless={}",
        shader_census.programs, shader_census.unresolved_aliases, shader_census.byteless,
    ));
    let decl_streams = global.vertex_decl_stream_census();
    report.push(format!(
        "vertex decls: n={} stream0={} ppcc0t0t0nn n={} streams={}",
        decl_streams.n,
        decl_streams.stream0,
        decl_streams.ppcc_n,
        decl_streams
            .ppcc_stream_count
            .map(|n| n.to_string())
            .unwrap_or_else(|| "missing".into()),
    ));

    // The donors first, and only then the plan that claims the same names
    // they do. Its claims are resolved against the catalog they leave behind
    // rather than against the one it was built from, which is why it waited.
    for kept in common.donor_images().await {
        let job = load_jobs::open(JobKind::ImageDecode)
            .namespace(kept.namespace)
            .canonical(kept.label)
            .depends_on(kept.job)
            .plan_ready()
            .enqueued()
            .started()
            .finished()
            .joined();
        merge_image_batch(
            &mut global,
            kept.label,
            kept.batch.share(),
            job,
            &mut report,
        );
    }
    if let Some(held) = common_images
        && let Some(pending) = held.prune_then_enqueue(&mut global, &progress, &mut report)
    {
        let job = pending.job;
        let (label, batch) = pending.join().await;
        let kept = common.retain(&batch);
        report.push(format!(
            "{label} payloads kept for the next map: +{:.1}MiB (common set holds {} payloads, {:.1}MiB)",
            kept as f64 / (1024.0 * 1024.0),
            common.retained_payloads(),
            common.retained_bytes() as f64 / (1024.0 * 1024.0),
        ));
        merge_image_batch(&mut global, label, batch, job, &mut report);
    }
    if let Ok(path) = &zone_ff {
        let stage = progress.begin_scoped(StageId::Images, "merged", None);
        let decoded =
            asset_material::decode_material_color_maps(path, &mut global, &stage, load_pool());
        stage.finish_from(&decoded);
        match decoded {
            Ok(stats) => report.push(format!(
                "merged material images: {}/{} decoded, {} missing, {} unsupported",
                stats.decoded, stats.requested, stats.missing, stats.unsupported
            )),
            Err(error) => report.push(format!("merged material images gap: {error}")),
        }
    }
    if let Some(draw) = world.draw.as_mut() {
        world.dynamic_light = resolve_world_lights(
            draw,
            &mut global,
            &common_light_defs,
            map_namespace,
            &zone_ff,
            &progress,
            &mut report,
        );
    }
    let builtins = asset_material::decode_in_zone_builtin_images(&mut global);
    if builtins != 0 {
        report.push(format!(
            "in-zone builtin images: decoded {builtins} leftover $ 2D loadDefs after absorb"
        ));
    }
    if let Ok(path) = &zone_ff {
        let stage = progress.begin_scoped(StageId::Images, "tracers", None);
        let decoded = asset_material::material_images::decode_images_for_keys(
            path,
            &mut global,
            common_tracers.material_keys(),
            &stage,
            load_pool(),
        );
        stage.finish_from(&decoded);
        match decoded {
            Ok(n) => report.push(format!("tracer beam images after absorb: {n} decoded")),
            Err(error) => report.push(format!("tracer beam images after absorb: {error}")),
        }
    }
    let unique: std::collections::BTreeSet<String> = common_tracers
        .named_materials()
        .map(str::to_owned)
        .collect();
    for name in &unique {
        let bind = asset_game::material_bind_name(name);
        let twins: Vec<&str> = global
            .materials
            .iter()
            .filter(|m| m.name.as_str() == bind)
            .map(|m| m.name.as_str())
            .collect();
        report.push(format!("tracer material `{name}` global twins={twins:?}"));
    }
    report.push(format!(
        "tracer color maps after absorb: Bound into global ({} unique names; no clone sidecar)",
        unique.len()
    ));

    world.fx.absorb(common_fx);
    let common_fx_model_n = common_fx_models.len();
    let map_fx_model_n = world.fx_models.len();
    let common_fx_model_added = world.fx_models.absorb(common_fx_models);
    let leftover_t5_fx_n = t5_fx.len();
    let leftover_t5_fx_gaps = t5_fx.capture_gaps;
    world.fx.absorb_missing(t5_fx);
    report.push(format!(
        "leftover t5 fx absorb_missing: donor={leftover_t5_fx_n} gaps={leftover_t5_fx_gaps} host now {}",
        world.fx.len()
    ));
    let fx_model_walked_n = world.fx_models.len();
    world.fx_models.keep_referenced(&world.fx.model_hints());
    world.fx.resolve_model_edges(&world.fx_models);
    let fx_model_edges = world.fx.model_edge_census();
    report.push(format!(
        "FX model generation: map={map_fx_model_n} common={common_fx_model_n} added={common_fx_model_added} walked={fx_model_walked_n} retained={} edges bound={} unresolved={} absent={}",
        world.fx_models.len(),
        fx_model_edges.bound,
        fx_model_edges.unresolved,
        fx_model_edges.absent,
    ));
    weapons.resolve_combat_fx(&world.fx, &common_tracers);
    weapons.resolve_projectile_fx_edges(&world.fx);
    let projectile_fx = weapons.projectile_fx_edge_census();
    report.push(format!(
        "weapon projectile FX after absorb: bound={} unresolved={} absent={}",
        projectile_fx.bound, projectile_fx.unresolved, projectile_fx.absent,
    ));
    link_materials_after_absorb(
        &global,
        &mut world,
        &mut weapons,
        &mut common_tracers,
        &mut bodies,
        &mut world_weapons,
        &mut fpv_meshes,
        &mut projectile_meshes,
        &mut report,
    );
    // Bind rigs and tracks to the finished mesh publication, after material linking.
    let fpv_meshes = fpv_meshes.publish();
    let assembly_started = std::time::Instant::now();
    let assemblies = weapons.resolve_fpv_assemblies(&fpv_meshes, &xanims);
    report.push(format!(
        "FPV assemblies: built={} kit sides linked={} refused={} clip track tables={} elapsed_ms={:.1}",
        assemblies.built,
        assemblies.linked,
        assemblies.refused,
        assemblies.clip_tables,
        assembly_started.elapsed().as_secs_f64() * 1000.0,
    ));
    let dependency_gaps = weapons.dependency_gaps();
    let selectable_gap_ids: std::collections::BTreeSet<_> = dependency_gaps
        .iter()
        .map(|gap| gap.id)
        .filter(|&id| weapons.describe_configuration(id).is_some())
        .collect();
    report.push(format!(
        "weapon dependency audit: {} gaps in {} definitions; selectable={}",
        dependency_gaps.len(),
        dependency_gaps
            .iter()
            .map(|gap| gap.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        selectable_gap_ids.len(),
    ));
    for gap in &dependency_gaps {
        report.push(format!(
            "weapon dependency gap: {} {} `{}`",
            weapons.name_of(gap.id),
            gap.kind,
            gap.name
        ));
    }
    report.push(
        "fx color maps handoff: n=0 bytes=0 (Bound GPU bind at spawn; no CPU clone sidecar; stub_aliases=0)"
            .into(),
    );
    decode_fx_colour_maps(&world.fx, &mut global, &zone_ff, &progress, &mut report);
    if world.impact_fx.is_none() {
        world.impact_fx = if map_namespace == Some(asset_core::AssetNamespace::T5) {
            t5_impact_fx.or(common_impact)
        } else {
            common_impact
        };
    } else if let Some(common_table) = common_impact {
        report.push(format!(
            "impactfx: map table kept; common_mp table `{}` discarded",
            common_table.name
        ));
    }
    if let Some(ref table) = world.impact_fx {
        report.push(format!(
            "impactfx handoff: {:?}:`{}` rows={} (fx catalog now {} defs)",
            table.namespace(),
            table.name,
            table.row_count(),
            world.fx.len()
        ));
    } else {
        report.push("impactfx handoff: missing — combat play_oriented will miss cells".into());
    }
    report.push(format!(
        "tracer catalog handoff: {} named ({} bound, {} unresolved)",
        common_tracers.len(),
        common_tracers.bound_count(),
        common_tracers.unresolved_count()
    ));

    let prepared_map = PreparedMap {
        zone: zone_name,
        namespace: map_namespace,
        spawns: dm_spawns,
        facts,
        gaps: PreparedGaps { lines: gap_lines },
    };
    if prepared_map.facts.minimap_corners.is_some() {
        report.push("compass: minimap_corner pair from MapEnts".into());
    } else {
        report.push("compass gap: minimap_corner missing — no world-to-map frame".into());
    }
    match prepared_map.facts.north_yaw {
        Some(yaw) => report.push(format!("compass: worldspawn northyaw {yaw}")),
        None => {
            report.push("compass gap: worldspawn has no northyaw — map is drawn north-up".into())
        }
    }

    report.extend(localize_report);
    let (directory_ms, opens, inflate_ms) = asset_material::iwd_read_cost();
    report.push(format!(
        "IWD read cost: central_dir={directory_ms:.0}ms over {opens} opens, inflate={inflate_ms:.0}ms (summed over worker threads, not wall)"
    ));
    let (mip_hit, mip_miss, mip_io_ms) = asset_material::mip_cache_cost();
    report.push(format!(
        "mip cache: hit={mip_hit} miss={mip_miss} io={mip_io_ms:.0}ms"
    ));
    let (payload_reads, header_reads) = asset_material::iwd_entry_reads();
    report.push(format!(
        "IWD entry reads: payload={payload_reads} header-only={header_reads} (a header answers whether an image is a cubemap; a payload read is the whole entry inflated)"
    ));
    if let Some(draw) = world.draw.as_ref() {
        census_image_working_set(
            draw,
            &world,
            &global,
            &map_ids,
            &fpv_meshes,
            &common_tracers,
            &mut report,
        );
    }
    let mut fx = std::mem::take(&mut world.fx).publish();
    fx.set_map_namespace(map_family);
    let xanims = xanims.publish();
    let destructible_death =
        crate::stamp_match_destructible_death(&xanims, &world.map_xmodel_scene_assets);
    for row in &destructible_death {
        report.push(format!(
            "destructible death {}: clip={} husk={}",
            row.kind,
            row.clip.edge_kind(),
            row.husk.edge_kind()
        ));
    }
    let prepared = PreparedMatch {
        scripts,
        fx,
        world,
        materials: crate::MatchMaterials {
            population: Arc::new(global),
            map_ids,
            common_profile_id: common.id,
            products_id: NEXT_PRODUCTS_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        },
        clip: clip.map(Arc::new),
        weapons: Arc::new(weapons.publish()),
        fpv_meshes,
        bodies: Arc::new(bodies.publish()),
        world_weapons: world_weapons.publish(),
        projectile_meshes: projectile_meshes.publish(),
        xanims,
        destructible_death,
        player_anim_sources,
        tracers: common_tracers.publish(),
        strings,
        report,
        prepared_map,
        pen_table: common_pen_table,
        pen_table_loaded: common_pen_loaded,
        lochit_table: common_lochit_table,
        xmodel_walk,
        sound,
        sound_gaps: 0,
        script_sound_aliases: None,
    };
    (MatchLoadOutcome::Ready(prepared), Some(common))
}

fn resolve_world_lights(
    draw: &mut WorldDraw,
    global: &mut asset_material::MaterialDefinitions,
    common_light_defs: &[asset_world::CapturedLightDef],
    map_namespace: Option<asset_core::AssetNamespace>,
    zone_ff: &Result<PathBuf, String>,
    progress: &LoadProgress,
    report: &mut Vec<String>,
) -> Option<asset_world::ResolvedLightDef> {
    asset_world::resolve_primary_light_attenuation(draw, global, common_light_defs);
    let dynamic_light_name =
        (map_namespace == Some(asset_core::AssetNamespace::Iw4)).then_some("light_dynamic");
    let dynamic_light = dynamic_light_name.and_then(|name| {
        asset_world::resolve_named_light_def(name, &draw.light_defs, common_light_defs, global)
    });
    let (ordinal, source) = asset_world::resolve_outdoor_image(
        draw.outdoor_image_name.as_deref(),
        map_namespace?,
        global,
    );
    draw.outdoor_image = ordinal;
    if source == "$outdoor" && draw.outdoor_image_name.is_none() {
        draw.outdoor_image_name = Some("$outdoor".into());
    }
    report.push(format!(
        "outdoorImage: source={source} name={} global_ordinal={} lookup_m00={:.6e} lookup_m30={:.4}",
        draw.outdoor_image_name.as_deref().unwrap_or("-"),
        draw.outdoor_image
            .map(|i| i.to_string())
            .unwrap_or_else(|| "NONE".into()),
        f32::from_bits(draw.outdoor_lookup[0]),
        f32::from_bits(draw.outdoor_lookup[12]),
    ));
    let named = draw
        .primary_lights
        .iter()
        .filter(|light| light.def_name.as_ref().is_some_and(|name| !name.is_empty()))
        .count();
    let atten = draw
        .primary_lights
        .iter()
        .filter(|light| light.attenuation_image.is_some())
        .count();
    report.push(format!(
        "GfxLightDef resolve: common_defs={} map_defs={} named_lights={named} atten_image={atten} (light-def name → global catalog; missing stays None)",
        common_light_defs.len(),
        draw.light_defs.len(),
    ));
    report.push(format!(
        "GfxLightDef names: common={:?} map={:?} lights={:?} images_common={:?} images_map={:?}",
        common_light_defs
            .iter()
            .map(|def| def.name.as_str())
            .collect::<Vec<_>>(),
        draw.light_defs
            .iter()
            .map(|def| def.name.as_str())
            .collect::<Vec<_>>(),
        draw.primary_lights
            .iter()
            .filter_map(|light| light.def_name.as_deref())
            .collect::<Vec<_>>(),
        common_light_defs
            .iter()
            .map(|def| def.attenuation_image_name.as_deref())
            .collect::<Vec<_>>(),
        draw.light_defs
            .iter()
            .map(|def| def.attenuation_image_name.as_deref())
            .collect::<Vec<_>>(),
    ));
    if let Ok(path) = &zone_ff {
        let mut requested: Vec<(usize, u8)> = draw
            .primary_lights
            .iter()
            .filter_map(|light| Some((light.attenuation_image?, light.attenuation_sampler)))
            .collect();
        if let Some(dynamic) = dynamic_light
            && let Some(image) = dynamic.attenuation_image
        {
            requested.push((image, dynamic.attenuation_sampler));
        }
        let want: std::collections::BTreeSet<usize> =
            requested.iter().map(|(index, _)| *index).collect();
        let want = want.len();
        let stage = progress.begin_scoped(StageId::Images, "attenuation", None);
        let decoded = asset_material::decode_catalog_images_from_iwd(
            path,
            global,
            requested,
            &stage,
            load_pool(),
        );
        stage.finish_from(&decoded);
        match decoded {
            Ok(n) => report.push(format!(
                "IWD light attenuation: decoded {n} of {want} GfxLightDef images (empty payload is not a host ramp)"
            )),
            Err(error) => report.push(format!("IWD light attenuation: {error}")),
        }
    }

    asset_world::resolve_primary_light_attenuation(draw, global, common_light_defs);
    let resolved_dynamic = dynamic_light_name.and_then(|name| {
        asset_world::resolve_named_light_def(name, &draw.light_defs, common_light_defs, global)
    });
    let dynamic_decoded = resolved_dynamic
        .and_then(|light| light.attenuation_image)
        .is_some_and(|index| {
            global
                .images
                .get(index)
                .is_some_and(|image| image.decoded.is_some())
        });
    report.push(format!(
        "FX light_dynamic: image={:?} decoded={} width={:?} sampler={}",
        resolved_dynamic.and_then(|light| light.attenuation_image),
        dynamic_decoded,
        resolved_dynamic.and_then(|light| light.falloff_image_width),
        resolved_dynamic.map_or(0, |light| light.attenuation_sampler),
    ));
    let dynamic_light =
        resolved_dynamic.filter(|light| dynamic_decoded && light.falloff_image_width.is_some());
    if dynamic_light_name.is_some() && dynamic_light.is_none() {
        report.push(
            "FX light_dynamic GAP: light definition or decoded attenuation image missing; additional FX lights unavailable"
                .into(),
        );
    }
    dynamic_light
}

#[allow(clippy::too_many_arguments)]
fn link_materials_after_absorb(
    materials: &asset_material::MaterialDefinitions,
    world: &mut PreparedWorld,
    weapons: &mut WeaponBuild,
    tracers: &mut asset_game::TracerCatalog,
    bodies: &mut asset_model::BodyMeshBuild,
    world_weapons: &mut WorldWeaponBuild,
    fpv_meshes: &mut FpvMeshBuild,
    projectile_meshes: &mut ProjectileMeshBuild,
    report: &mut Vec<String>,
) {
    let fx_model_materials = world.fx_models.resolve_materials(materials);
    report.push(format!(
        "FX model materialHandles after absorb: bound={} unresolved={} absent={}",
        fx_model_materials.bound, fx_model_materials.unresolved, fx_model_materials.absent,
    ));
    weapons.resolve_hud_material_edges(materials);
    if let Some(glass) = world.fx_glass.as_mut() {
        glass.resolve_material_edges(materials);
        let census = glass.material_edge_census();
        report.push(format!(
            "glass Material* after absorb: bound={} unresolved={} absent={}",
            census.bound, census.unresolved, census.absent
        ));
    }
    let hud_materials = weapons.hud_material_edge_census();
    let technique_sets = materials.technique_set_edge_census();
    report.push(format!(
        "material pointer graph after finalize: weapon_hud bound={} unresolved={} absent={}; technique_set bound={} unresolved={} absent={}",
        hud_materials.bound,
        hud_materials.unresolved,
        hud_materials.absent,
        technique_sets.bound,
        technique_sets.unresolved,
        technique_sets.absent,
    ));
    let graph = crate::resolve_after_absorb(
        materials,
        tracers,
        &mut world.fx,
        None,
        Some(bodies),
        Some(world_weapons),
        None,
        Some(fpv_meshes),
        Some(projectile_meshes),
    );
    report.push(format!(
        "asset graph after absorb: tracer_mat bound={} unresolved={} absent={}; fx_elem bound={} unresolved={} absent={}; fx_child bound={} unresolved={} absent={}; fx_runner bound={} unresolved={} absent={}; body materialHandles bound={} unresolved={} absent={}; world-gun materialHandles bound={} unresolved={} absent={}; FPV materialHandles bound={} unresolved={} absent={} (WeaponDef.tracerType stamped at common_mp walk)",
        graph.tracer_materials.bound,
        graph.tracer_materials.unresolved,
        graph.tracer_materials.absent,
        graph.fx_elem_materials.bound,
        graph.fx_elem_materials.unresolved,
        graph.fx_elem_materials.absent,
        graph.fx_nested_children.bound,
        graph.fx_nested_children.unresolved,
        graph.fx_nested_children.absent,
        graph.fx_runner_children.bound,
        graph.fx_runner_children.unresolved,
        graph.fx_runner_children.absent,
        graph.xmodel_body_materials.bound,
        graph.xmodel_body_materials.unresolved,
        graph.xmodel_body_materials.absent,
        graph.xmodel_gun_materials.bound,
        graph.xmodel_gun_materials.unresolved,
        graph.xmodel_gun_materials.absent,
        graph.xmodel_fpv_materials.bound,
        graph.xmodel_fpv_materials.unresolved,
        graph.xmodel_fpv_materials.absent,
    ));
    report.push(format!(
        "Material stamp: iw4={} t5={} iw5={} (name-link identity; colliding T5 names are the later-zone row)",
        materials.namespace_count(asset_core::AssetNamespace::Iw4),
        materials.namespace_count(asset_core::AssetNamespace::T5),
        materials.namespace_count(asset_core::AssetNamespace::Iw5),
    ));
    report.push(format!(
        "fx elem material edges after absorb: {} bound ({} unique), {} unresolved (temp={}, catalog_miss={}), {} absent of {} material visuals; {} decal mark arms ({} Bound slots, {} unresolved, {} temp, {} array-unpatched)",
        world.fx.material_visual_bound_count(),
        world.fx.material_visual_unique_bound_count(),
        world.fx.material_visual_unresolved_count(),
        world.fx.material_visual_unresolved_temp_count(),
        world.fx.material_visual_unresolved_miss_count(),
        world.fx.material_visual_absent_count(),
        world.fx.material_visual_count(),
        world.fx.material_visual_decal_count(),
        world.fx.decal_mark_bound_slot_count(),
        world.fx.decal_mark_unresolved_slot_count(),
        world.fx.decal_mark_temp_slot_count(),
        world.fx.material_visual_decal_unpatched_count()
    ));
    report.push(format!(
        "fx elem decal unique Bound after absorb: {} (mc={}, wc={}); decoded color/2D in catalog {} of {} (not sprite Plan)",
        world.fx.unique_decal_mark_count(),
        world.fx.unique_decal_mark_mc_count(),
        world.fx.unique_decal_mark_wc_count(),
        world.fx.unique_decal_mark_decoded_color_count(materials),
        world.fx.unique_decal_mark_count()
    ));
    let miss = world
        .fx
        .unique_decal_mark_decoded_miss_samples(materials, 8);
    if !miss.is_empty() {
        report.push(format!(
            "fx elem decal catalog nocolor sample: {}",
            miss.join("; ")
        ));
    }
    let samples = world.fx.unresolved_material_samples(8);
    if !samples.is_empty() {
        report.push(format!(
            "fx elem unresolved Material* sample: {}",
            samples.join("; ")
        ));
    }
    let mark_samples = world.fx.decal_mark_samples(8);
    if !mark_samples.is_empty() {
        report.push(format!(
            "fx elem decal mark sample: {}",
            mark_samples.join("; ")
        ));
    }
}

fn decode_fx_colour_maps(
    fx: &FxCatalog,
    global: &mut asset_material::MaterialDefinitions,
    zone_ff: &Result<PathBuf, String>,
    progress: &LoadProgress,
    report: &mut Vec<String>,
) {
    let missing: Vec<asset_core::MaterialKey> = fx
        .unique_bound_hints()
        .into_iter()
        .chain(fx.unique_decal_mark_hints())
        .filter(|(index, _)| !asset_game::color_decoded_in_catalog(global, *index))
        .filter_map(|(index, _)| {
            let material = global.materials.get(index)?;
            Some(asset_core::MaterialKey {
                namespace: material.namespace,
                name: material.name.as_str().to_owned(),
            })
        })
        .collect();
    if !missing.is_empty() {
        if let Ok(path) = zone_ff {
            let stage = progress.begin_scoped(StageId::Images, "fx_elem", None);
            let decoded = asset_material::material_images::decode_images_for_keys(
                path,
                global,
                missing,
                &stage,
                load_pool(),
            );
            stage.finish_from(&decoded);
            match decoded {
                Ok(n) => report.push(format!("fx elem 2d images after absorb: {n} decoded")),
                Err(error) => report.push(format!("fx elem 2d images after absorb: {error}")),
            }
        }
    }
    let nocolor: Vec<(usize, String)> = fx
        .unique_bound_hints()
        .into_iter()
        .filter(|(index, _)| !asset_game::color_decoded_in_catalog(global, *index))
        .map(|(index, hint)| (index, hint.to_owned()))
        .collect();
    if !nocolor.is_empty() {
        let (distortion, other): (Vec<_>, Vec<_>) = nocolor
            .into_iter()
            .partition(|(_, hint)| hint.contains("distortion"));
        report.push(format!(
            "fx elem Bound without decoded color: {} of {} unique (distortion={}, other={}) other_sample: {}",
            distortion.len() + other.len(),
            fx.material_visual_unique_bound_count(),
            distortion.len(),
            other.len(),
            if other.is_empty() {
                "-".to_string()
            } else {
                other
                    .iter()
                    .map(|(index, hint)| format!("{index}:{hint}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            }
        ));
        for (index, hint) in &other {
            let Some(mat) = global.materials.get(*index) else {
                report.push(format!(
                    "fx elem Bound `{index}:{hint}` has no global material row"
                ));
                continue;
            };
            let sem: Vec<u8> = mat.textures.iter().map(|t| t.semantic).collect();
            let decoded = mat.textures.iter().any(|t| {
                t.image
                    .and_then(|i| global.images.get(i))
                    .is_some_and(|img| img.decoded.is_some())
            });
            report.push(format!(
                "fx elem Bound `{index}:{}` techset={} camera_region={} tex={} sem={sem:?} decoded={decoded}",
                mat.name,
                mat.technique_set,
                mat.camera_region,
                mat.textures.len(),
            ));
        }
    }
}

fn census_image_working_set(
    draw: &WorldDraw,
    world: &PreparedWorld,
    global: &asset_material::MaterialDefinitions,
    map_ids: &[Option<usize>],
    fpv_meshes: &asset_model::FpvMeshCatalog,
    tracers: &asset_game::TracerCatalog,
    report: &mut Vec<String>,
) {
    let world_mats = draw.batches.iter().filter_map(|batch| {
        let local = batch.material?;
        map_ids.get(local).copied().flatten().or(Some(local))
    });

    let smodel_mats = world.static_model_meshes.iter().flat_map(|mesh| {
        mesh.lod_surfaces.iter().flatten().filter_map(|surface| {
            let local = surface.material?;
            map_ids.get(local).copied().flatten()
        })
    });
    let fpv_mats = fpv_meshes.bound_material_indices();
    let fx_mats = world
        .fx
        .unique_bound_hints()
        .into_iter()
        .map(|(index, _)| index)
        .chain(
            world
                .fx
                .unique_decal_mark_hints()
                .into_iter()
                .map(|(index, _)| index),
        )
        .chain(tracers.defs().filter_map(|def| def.material.bound_index()));
    let mut set = asset_material::material_images::census_image_working_set(
        global,
        world_mats,
        smodel_mats,
        fpv_mats,
        fx_mats,
    );
    let (probe_n, probe_bytes) = asset_material::material_images::cpu_image_census(
        world.reflection_probe_images.iter().flatten(),
    );
    let (lightmap_n, lightmap_bytes) = match &draw.lightmap {
        Ok(pages) => asset_material::material_images::cpu_image_census(
            pages.iter().flatten().flat_map(|page| {
                [
                    page.primary_image.as_ref(),
                    page.secondary_image.as_ref(),
                    Some(&page.ambient_image),
                    Some(&page.directional_image),
                    Some(&page.sun_mask_image),
                ]
                .into_iter()
                .flatten()
            }),
        ),
        Err(_) => (0, 0),
    };
    set.probe_n = probe_n;
    set.probe_bytes = probe_bytes;
    set.lightmap_n = lightmap_n;
    set.lightmap_bytes = lightmap_bytes;
    asset_material::material_images::store_image_working_set(set);
    report.push(format!(
        "image working set: decoded={} ({:.1}MiB) world-batch={} ({:.1}MiB) smodel={} ({:.1}MiB) fpv={} ({:.1}MiB) probe={} ({:.1}MiB) lightmap={} ({:.1}MiB) fx={} ({:.1}MiB); subsets, not a skipped load",
        set.decoded_n,
        set.decoded_bytes as f64 / (1024.0 * 1024.0),
        set.world_n,
        set.world_bytes as f64 / (1024.0 * 1024.0),
        set.smodel_n,
        set.smodel_bytes as f64 / (1024.0 * 1024.0),
        set.fpv_n,
        set.fpv_bytes as f64 / (1024.0 * 1024.0),
        set.probe_n,
        set.probe_bytes as f64 / (1024.0 * 1024.0),
        set.lightmap_n,
        set.lightmap_bytes as f64 / (1024.0 * 1024.0),
        set.fx_n,
        set.fx_bytes as f64 / (1024.0 * 1024.0),
    ));
}

const FACTION_TABLE: &str = "mp/factionTable.csv";
const IW4_ALLIES_CHARSET: &str = "us_army";
const IW4_AXIS_CHARSET: &str = "opforce_composite";

fn foreign_faction_rows(
    scripts: &mut crate::ScriptSources,
    teams: &asset_game::MapTeamSettings,
) -> bool {
    let allies = faction_cells(
        &teams.allies_strings,
        teams.allies_name.as_ref(),
        teams.allies.as_ref(),
        teams.allies_color,
    );
    let axis = faction_cells(
        &teams.axis_strings,
        teams.axis_name.as_ref(),
        teams.axis.as_ref(),
        teams.axis_color,
    );
    scripts.set_table_cells(FACTION_TABLE, IW4_ALLIES_CHARSET, &allies)
        & scripts.set_table_cells(FACTION_TABLE, IW4_AXIS_CHARSET, &axis)
}

fn faction_cells(
    strings: &asset_game::FactionStrings,
    short_name: Option<&asset_core::AssetKey>,
    icon: Option<&asset_core::AssetKey>,
    color: Option<[f32; 3]>,
) -> Vec<(usize, String)> {
    let mut cells: Vec<(usize, String)> = [
        (1, strings.name.clone()),
        (2, short_name.map(|k| k.logical_name().to_owned())),
        (3, strings.eliminated.clone()),
        (4, strings.forfeited.clone()),
        (5, icon.map(asset_core::AssetKey::display)),
    ]
    .into_iter()
    .filter_map(|(column, value)| Some((column, value?)))
    .collect();
    if let Some(rgb) = color {
        cells.extend((14..).zip(rgb.map(|c| c.to_string())));
    }
    cells
}

fn t5_map_under_iw4_rules(
    map_scripts: &crate::ScriptSources,
    zone_name: &str,
    facts: &crate::MapFacts,
) -> crate::ScriptSources {
    let module = format!("maps/mp/{zone_name}");
    let main = map_scripts
        .read(&module)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default();
    let mut declarations = asset_world::MapDeclarations::from_t5_map_main(&main);
    declarations.set_teams(
        facts.script_sound.attackers.as_deref().unwrap_or("allies"),
        facts.script_sound.defenders.as_deref().unwrap_or("axis"),
    );
    let entities = map_scripts
        .entities()
        .map(asset_world::t5_entities_for_iw4_rules)
        .unwrap_or_default();
    let mut scripts = crate::ScriptSources::default();
    let mut map_main = declarations.map_script(&entities);
    let end = map_main.rfind('}').expect("generated map main");
    map_main.insert_str(end, "\tthread iw4l_maps\\destructibles::main();\n");
    scripts.insert_source(
        "iw4l_maps/destructibles",
        crate::map_scripts::DESTRUCTIBLES.to_owned(),
    );
    if zone_name == "mp_radiation" {
        let end = map_main.rfind('}').expect("generated map main");
        map_main.insert_str(end, "\tthread iw4l_maps\\radiation::main();\n");
        scripts.insert_source(
            "iw4l_maps/radiation",
            crate::map_scripts::RADIATION.to_owned(),
        );
    }
    scripts.insert_source(&module, map_main);
    scripts.set_entities(entities);
    scripts
}
