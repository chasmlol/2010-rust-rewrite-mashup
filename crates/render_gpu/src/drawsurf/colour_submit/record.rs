use super::*;

pub(super) fn draw_exact_colour(
    view: ViewQuery<(
        &ViewTarget,
        &SceneDepthTexture,
        &ExtractedView,
        Option<&Msaa>,
    )>,
    world: Res<InstalledRenderWorld>,
    frame: Res<PublishedRenderFrame>,
    geometry: Res<ExactColourGeometry>,
    smodel_cache_gpu: Res<SmodelCacheGpu>,
    pipeline: Res<ExactColourPipeline>,
    registry: Res<ExactPipelineRegistry>,

    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    binding_cache: Res<ExactColourBindingCache>,
    shadow_binding: Res<ExactShadowBindingCache>,
    constant_arena: Res<ExactConstantArena>,
    mut census: ResMut<ExactColourSubmitCensus>,
    (
        adapter,
        pretess,
        mut indirect,
        mut floatz,
        resolved_scene,
        shadow_arena,
        spot_arena,
        static_draws,
        (
            mut focus_submit,
            mut scratch,
            mut shadow_scratch,
            mut cam,
            mut working_set,
            mut texture_table,
            mut shadow_table,
            minecraft,
        ),
    ): (
        Res<bevy::render::renderer::RenderAdapter>,
        Res<CameraWorldPretess>,
        ResMut<super::indirect::ExactIndirectDraws>,
        ResMut<ExactFloatZResolve>,
        Res<super::super::resolved_scene::ResolvedScene>,
        Res<ShadowmapSunArena>,
        Res<ShadowmapSpotArena>,
        Res<ResidentShadowStaticDraws>,
        (
            ResMut<FocusedOwnerSubmitState>,
            ResMut<ColourSubmitScratch>,
            ResMut<ShadowSubmitScratch>,
            ResMut<CameraPrepareState>,
            ResMut<crate::ColourWorkingSet>,
            ResMut<SceneTextureTables>,
            ResMut<ShadowTextureTable>,
            Res<super::super::minecraft_light::MinecraftLightGpu>,
        ),
    ),
    mut context: RenderContext,
) {
    let extracted = ExtractedColourRefs::new(&world, &frame);
    let products = &extracted.frame.frame_products;
    let _colour_submit = perf::Span::RenderColourSubmitMs.enter();
    let census_on = perf::recording();

    *working_set = crate::ColourWorkingSet::default();
    if pipeline.ports.is_empty() {
        emit_focused_owner_submit(
            &products,
            &mut focus_submit,
            Some((
                &extracted.world.sorted_material_names,
                &extracted.world.image_handles,
            )),
            exec_tables(extracted),
            "no_exact_ports",
            0,
            0,
            0,
        );
        return;
    }
    if extracted.world.sampler_table.is_none() {
        emit_focused_owner_submit(
            &products,
            &mut focus_submit,
            Some((
                &extracted.world.sorted_material_names,
                &extracted.world.image_handles,
            )),
            exec_tables(extracted),
            "sampler_table_missing",
            0,
            0,
            0,
        );
        return;
    }

    let scene_epoch = scratch.prepared_scene_epoch;
    let shadow_epoch = shadow_scratch.prepared_shadow_epoch;
    let table_epoch_ok = scene_epoch.generation == extracted.world.generation
        && binding_cache.generation == extracted.world.generation
        && shadow_binding.generation == extracted.world.generation
        && constant_arena.generation == extracted.world.generation
        && shadow_table.epoch() == shadow_epoch
        && texture_table
            .0
            .iter()
            .all(|table| table.epoch() == scene_epoch);
    if !table_epoch_ok {
        diag::error!(
            World,
            "drawsurf colour submit: prepared work is from another table epoch (extracted={:?} binding_cache={:?} shadow_binding={:?} arena={:?} prepared_scene={scene_epoch:?} prepared_shadow={shadow_epoch:?} scene={:?} shadow={:?}) — prepare did not run before record; refusing to record against a table this frame never numbered",
            extracted.world.generation,
            binding_cache.generation,
            shadow_binding.generation,
            constant_arena.generation,
            texture_table.0.each_ref().map(ExactTextureTable::epoch),
            shadow_table.epoch(),
        );
        emit_focused_owner_submit(
            &products,
            &mut focus_submit,
            Some((
                &extracted.world.sorted_material_names,
                &extracted.world.image_handles,
            )),
            exec_tables(extracted),
            "table_epoch_mismatch",
            0,
            0,
            0,
        );
        return;
    }
    let colour = products.0.product(FrameProductKind::Colour);
    let light = products.0.product(FrameProductKind::Light);
    let emissive = products.0.product(FrameProductKind::Emissive);
    scratch.skinned_tess.upload(&device, &queue);
    shadow_scratch.skinned_tess.upload(&device, &queue);
    // Handles only: taking the caches would drop the kept skinned surfaces.
    let tess_vertex = scratch.skinned_tess.vertex_buffer().cloned();
    let tess_index = scratch.skinned_tess.index_buffer().cloned();
    let tess_vertex_lighting = scratch.skinned_tess.vertex_lighting_buffer().cloned();
    let shadow_tess_vertex = shadow_scratch.skinned_tess.vertex_buffer().cloned();
    let shadow_tess_vertex_lighting = shadow_scratch
        .skinned_tess
        .vertex_lighting_buffer()
        .cloned();
    let shadow_tess_index = shadow_scratch.skinned_tess.index_buffer().cloned();
    let smodel_skinned_vertex = tess_vertex.as_ref();
    let smodel_skinned_index = tess_index.as_ref();
    let smodel_skinned_vertex_lighting = tess_vertex_lighting.as_ref();
    let shadow_skinned_vertex = shadow_tess_vertex.as_ref();
    let shadow_skinned_vertex_lighting = shadow_tess_vertex_lighting.as_ref();
    let shadow_skinned_index = shadow_tess_index.as_ref();
    let sun_prepared = std::mem::take(&mut shadow_scratch.sun_prepared);
    let spot_prepared = std::mem::take(&mut shadow_scratch.spot_prepared);
    let sun_started = colour_census_clock(census_on);
    let sun_submit = record_shadowmap_sun(
        sun_prepared,
        pipeline.as_ref(),
        &registry,
        &geometry,
        &device,
        &mut shadow_table,
        &shadow_arena,
        &static_draws,
        &mut context,
        shadow_skinned_vertex,
        shadow_skinned_vertex_lighting,
        shadow_skinned_index,
        minecraft.off(),
    );
    let spot_submit = record_shadowmap_spot(
        spot_prepared,
        pipeline.as_ref(),
        &registry,
        &geometry,
        &device,
        &mut shadow_table,
        &spot_arena,
        &mut context,
        shadow_skinned_vertex,
        shadow_skinned_vertex_lighting,
        shadow_skinned_index,
        minecraft.off(),
    );
    let mut record_n = sun_submit.record_n;
    record_n.add(spot_submit.record_n);
    if census_on {
        census.frame.sun_shadow_gpu = Some(sun_submit.gpu);
        census.frame.sun_shadow_gpu_miss = Some(sun_submit.miss);
        census.frame.sun_shadow_gpu_cause = sun_submit.cause;
        census.frame.sun_shadow_gpu_causes = sun_submit.causes;
        census.sun_shadow_static_hit = Some(sun_submit.static_hit);
        census.frame.sun_shadow_world_ib_n = Some(sun_submit.world_ib_n);
        census.frame.sun_shadow_submit_ms = colour_census_ms(sun_started);
        census.sun_shadow_prepare_ms = Some(sun_submit.prepare_ms);
        census.sun_shadow_patch_ms = Some(sun_submit.patch_ms);
        census.sun_shadow_arena_ms = Some(sun_submit.arena_ms);
        census.sun_shadow_record_ms = Some(sun_submit.record_ms);
        census.frame.spot_shadow_gpu = Some(spot_submit.gpu);
        census.frame.spot_shadow_gpu_miss = Some(spot_submit.miss);
        census.frame.spot_shadow_gpu_cause = spot_submit.cause;
        census.frame.spot_shadow_slot_n = Some(spot_submit.slots);
        census.frame.set_bind_group_n = Some(record_n.total());
        census.frame.set_state_n = Some(record_n.state);
        census.frame.multi_draw_n = Some(record_n.multi_draws);
        census.frame.multi_draw_commands_n = Some(record_n.multi_draw_commands);
        census.set_bind_group0_n = Some(record_n.group0);
        census.set_bind_group1_n = Some(record_n.group1);
        census.sun_shadow_finish_ms = Some(sun_submit.finish_ms);
        census.sun_shadow_queue_ms = Some(sun_submit.emit_ms);
        let named = sun_submit.emit_ms
            + sun_submit.prepare_ms
            + sun_submit.patch_ms
            + sun_submit.arena_ms
            + sun_submit.record_ms
            + sun_submit.finish_ms;
        census.sun_shadow_unnamed_ms = census
            .frame
            .sun_shadow_submit_ms
            .filter(|envelope| *envelope >= named)
            .map(|envelope| envelope - named);
    }
    if colour.ordered_draws.is_empty()
        && light.ordered_draws.is_empty()
        && emissive.ordered_draws.is_empty()
    {
        emit_focused_owner_submit(
            &products,
            &mut focus_submit,
            Some((
                &extracted.world.sorted_material_names,
                &extracted.world.image_handles,
            )),
            exec_tables(extracted),
            "product_empty",
            0,
            0,
            0,
        );
        return;
    }
    let (target, depth, extracted_view, _msaa) = view.into_inner();
    if !cam.active {
        emit_focused_owner_submit(
            &products,
            &mut focus_submit,
            Some((
                &extracted.world.sorted_material_names,
                &extracted.world.image_handles,
            )),
            exec_tables(extracted),
            "camera_prepare_skipped",
            0,
            0,
            0,
        );
        return;
    }
    let ready_draws = cam.ready_draws;
    let mut refused_draws = cam.refused_draws;
    let mut pipeline_not_ready = cam.pipeline_not_ready;
    let mut last_refusal = cam.last_refusal.take();
    let mut refusals = DrawRefusalCensus::taking(
        census_on,
        std::mem::take(&mut cam.submit_refusals),
        std::mem::take(&mut cam.exec_refusals),
    );
    let exec_refused = cam.exec_refused;
    let unsupported_state = cam.unsupported_state;
    let bsp_submit_refused_surfaces = cam.bsp_submit_refused_surfaces;
    let pnr_smodel_mats = std::mem::take(&mut cam.pnr_smodel_mats);
    let pnr_world_mats = std::mem::take(&mut cam.pnr_world_mats);
    let pnr_smodel_ps = std::mem::take(&mut cam.pnr_smodel_ps);
    let pnr_world_ps = std::mem::take(&mut cam.pnr_world_ps);
    let pnr_smodel_keys = std::mem::take(&mut cam.pnr_smodel_keys);
    let pnr_world_keys = std::mem::take(&mut cam.pnr_world_keys);
    let pnr_ports = std::mem::take(&mut cam.pnr_ports);
    let bind_smodel_mats = std::mem::take(&mut cam.bind_smodel_mats);
    let last_markmesh_refusal = cam.last_markmesh_refusal.take();
    let last_markmesh_exec_skip = cam.last_markmesh_exec_skip;
    let markmesh_missing_58 = cam.markmesh_missing_58;
    let last_glassmesh_exec_skip = cam.last_glassmesh_exec_skip;
    let last_glassmesh_refusal = cam.last_glassmesh_refusal.take();
    let last_mark_packed_custom = cam.last_mark_packed_custom;
    let last_mark_packed_scene_light = cam.last_mark_packed_scene_light;
    let last_mark_lmap_sampler = cam.last_mark_lmap_sampler;
    let last_glass_packed_probe = cam.last_glass_packed_probe;
    let last_glass_probe_sampler = cam.last_glass_probe_sampler;
    let markmesh_hits = cam.markmesh_hits;
    let glassmesh_hits = cam.glassmesh_hits;
    let prepared_hits = cam.prepared_hits;
    let prepare_cost = std::mem::take(&mut cam.prepare_cost);
    let colour_run_census = cam.colour_run_census;
    let focused_object_id = cam.focused_object_id;
    let viewmodel_held = cam.viewmodel_held;
    let needs_floatz = cam.needs_floatz;
    let needs_resolved_scene = cam.needs_resolved_scene;
    let _world_ib_skip = cam.world_ib_skip;
    let mut prepared = std::mem::take(&mut scratch.prepared);
    let submitted_keys = &scratch.submitted_keys;
    let world_exec_ready_keys = &scratch.world_exec_ready_keys;
    let mut floatz_blit = 0u32;
    let mut resolved_scene_copy = 0u32;
    let mut focused_drawn_passes = 0u32;

    let sun_shadow_produced = sun_submit.gpu != 0;
    let spot_shadow_produced = spot_submit.slots != 0 || spot_submit.gpu != 0;
    let held = retain_camera_draws_with_sun_content(
        &mut prepared,
        sun_shadow_produced,
        spot_shadow_produced,
    );
    if held > 0 {
        refused_draws = refused_draws.saturating_add(held);
        refusals.note_submit_class("colour", "ProductDependencyNotReady", held);
        last_refusal = Some(GpuSubmitRefusal::ProductDependencyNotReady {
            product: FrameProductKind::SunShadow,
        });
    }
    if census_on {
        census.frame.pack_intern_hit_n = Some(prepare_cost.intern_hit_n);
        census.frame.pack_intern_miss_n = Some(prepare_cost.intern_miss_n);
        census.frame.gpu_exec_reuse_n = Some(0);
        census.frame.gpu_exec_unique_n = Some(0);
        census.frame.pack_overlay_n = Some(0);
        census.frame.pack_overlay_row_n = Some(0);
        census.frame.pack_overlay_pixel_share_n = Some(0);
        census.frame.pack_seed_n = Some(prepare_cost.pack_seed_n);
        census.frame.pack_walk_n = Some(prepare_cost.pack_walk_n);
        census.frame.tex_bind_hit_n = Some(prepare_cost.tex_bind_hit_n);
        census.frame.tex_bind_miss_n = Some(prepare_cost.tex_bind_miss_n);
        census.frame.markmesh_hits = Some(as_u32(markmesh_hits));
        census.frame.last_markmesh_refusal =
            last_markmesh_refusal.map(|cause| submit_refusal_class(&cause).to_owned());
        census.frame.last_markmesh_exec_skip = last_markmesh_exec_skip.map(str::to_owned);
        census.frame.markmesh_missing_58 = Some(markmesh_missing_58);
        census.frame.glassmesh_hits = Some(as_u32(glassmesh_hits));
        census.frame.last_glassmesh_exec_skip = last_glassmesh_exec_skip.map(str::to_owned);
        census.frame.last_glassmesh_refusal =
            last_glassmesh_refusal.map(|cause| submit_refusal_class(&cause).to_owned());
        census.frame.last_glass_packed_probe = last_glass_packed_probe;
        census.frame.last_glass_probe_sampler = last_glass_probe_sampler;
        census.frame.last_mark_packed_custom = last_mark_packed_custom;
        census.frame.last_mark_packed_scene_light = last_mark_packed_scene_light;
        census.frame.last_mark_lmap_sampler = last_mark_lmap_sampler;
    }

    let mut gpu_indexed = 0u32;
    let mut bsp_drawn_surfaces = [0u32; 4];
    let mut bsp_draw_refused_surfaces = [0u32; 4];
    if !prepared.is_empty() {
        coalesce_exact_draws(&mut prepared);

        indirect.publish(&mut prepared, &device, &adapter, &queue);
        for draw in &prepared {
            if draw.bsp_counted
                && let Some(kind) = draw.bsp_kind
            {
                let lane = bsp_kind_index(kind);
                if census_on {
                    census.frame.bsp_submitted_surfaces[lane] = census.frame.bsp_submitted_surfaces
                        [lane]
                        .saturating_add(u32::from(draw.bsp_surf_count));
                }
            }
        }
        if census_on {
            census.frame.gpu_prepared = Some(prepared_hits);
            census.frame.gpu_world_ready = Some(as_u32(
                prepared
                    .iter()
                    .filter(|draw| draw.tess == ExactTessBind::World)
                    .count(),
            ));
            census.frame.gpu_smodel_ready = Some(as_u32(
                prepared
                    .iter()
                    .filter(|draw| {
                        draw.tess == ExactTessBind::Smodel
                            || draw.tess == ExactTessBind::SmodelCached
                            || draw.tess == ExactTessBind::SmodelSkinned
                    })
                    .count(),
            ));
            census.frame.gpu_xmodel_ready = Some(as_u32(
                prepared
                    .iter()
                    .filter(|draw| draw.tess == ExactTessBind::XModel)
                    .count(),
            ));
        }
        let has_late_scene = prepared.iter().any(|draw| draw.after_scene_resolve);
        let table_layout =
            texture_table_layout(pipeline.as_ref()).expect("ports are non-empty above");
        let table_binds = texture_table
            .0
            .each_mut()
            .map(|table| &table.binds(&device, &registry, table_layout).scene);

        let diagnostics = context.diagnostic_recorder();
        let diagnostics = diagnostics.as_deref();
        let encoder = context.command_encoder();
        let record_started = colour_census_clock(census_on);
        let mut pass_end_ms = 0.0f32;
        let mut pass_end_n = 0u32;
        let mut encode_not_ready = 0u32;
        let gpu_span = diagnostics.time_span(encoder, GPU_SPAN_COLOUR);
        let (indexed, drop_ms, drawn, draw_refused, focused_drawn, binds) = submit_exact_draws(
            encoder,
            &device,
            target,
            depth,
            extracted_view,
            &geometry,
            &smodel_cache_gpu,
            smodel_skinned_vertex,
            smodel_skinned_vertex_lighting,
            smodel_skinned_index,
            pretess.as_ref(),
            &indirect,
            &registry,
            &constant_arena,
            [table_binds[0], table_binds[1]],
            prepared
                .iter()
                .filter(|draw| !draw.after_scene_resolve && !draw.minecraft_hidden),
            "iw4_exact_colour_pass",
            &mut refused_draws,
            &mut last_refusal,
            &mut encode_not_ready,
            focused_object_id,
            minecraft.camera(),
        );
        gpu_indexed = gpu_indexed.saturating_add(indexed);
        focused_drawn_passes = focused_drawn_passes.saturating_add(focused_drawn);
        record_n.add(binds);
        for lane in 0..3 {
            bsp_drawn_surfaces[lane] = bsp_drawn_surfaces[lane].saturating_add(drawn[lane]);
            bsp_draw_refused_surfaces[lane] =
                bsp_draw_refused_surfaces[lane].saturating_add(draw_refused[lane]);
        }
        pass_end_ms += drop_ms;
        pass_end_n = pass_end_n.saturating_add(1);
        gpu_span.end(encoder);

        if needs_floatz {
            let gpu_span = diagnostics.time_span(encoder, GPU_SPAN_FLOATZ);
            if floatz::blit_depth(encoder, &cache, &device, &floatz) {
                floatz_blit = 1;
                floatz.resolved_frame = Some(products.0.frame_id);
            }
            gpu_span.end(encoder);
        }

        if needs_resolved_scene {
            resolved_scene.copy(encoder, target.main_texture());
            resolved_scene_copy = 1;
        }
        if has_late_scene {
            if !needs_floatz || floatz_blit == 1 {
                let gpu_span = diagnostics.time_span(encoder, GPU_SPAN_EMISSIVE);
                let (indexed, drop_ms, drawn, draw_refused, focused_drawn, binds) =
                    submit_exact_draws(
                        encoder,
                        &device,
                        target,
                        depth,
                        extracted_view,
                        &geometry,
                        &smodel_cache_gpu,
                        smodel_skinned_vertex,
                        smodel_skinned_vertex_lighting,
                        smodel_skinned_index,
                        pretess.as_ref(),
                        &indirect,
                        &registry,
                        &constant_arena,
                        [table_binds[2], table_binds[3]],
                        prepared
                            .iter()
                            .filter(|draw| draw.after_scene_resolve && !draw.minecraft_hidden),
                        "iw4_exact_emissive_pass",
                        &mut refused_draws,
                        &mut last_refusal,
                        &mut encode_not_ready,
                        focused_object_id,
                        minecraft.camera(),
                    );
                gpu_span.end(encoder);
                gpu_indexed = gpu_indexed.saturating_add(indexed);
                focused_drawn_passes = focused_drawn_passes.saturating_add(focused_drawn);
                record_n.add(binds);
                for lane in 0..3 {
                    bsp_drawn_surfaces[lane] = bsp_drawn_surfaces[lane].saturating_add(drawn[lane]);
                    bsp_draw_refused_surfaces[lane] =
                        bsp_draw_refused_surfaces[lane].saturating_add(draw_refused[lane]);
                }
                pass_end_ms += drop_ms;
                pass_end_n = pass_end_n.saturating_add(1);
            } else {
                let skipped = prepared
                    .iter()
                    .filter(|draw| draw.after_scene_resolve)
                    .count() as u32;
                if skipped > 0 {
                    refusals.note_submit_class("codemesh", "FloatZBlitNotReady", skipped);
                }
            }
        }
        if census_on {
            if pass_end_n > 0 {
                census.frame.pass_end_ms = Some(pass_end_ms);
            }
            census.frame.submit_record_ms = colour_census_ms(record_started);
            census.frame.encoder_finish_ms = Some(0.0);
        }
        if encode_not_ready > 0 {
            pipeline_not_ready = pipeline_not_ready.saturating_add(encode_not_ready);
            refusals.note_submit_class("encode", "PipelineNotReady", encode_not_ready);
        }
    }
    *working_set = crate::ColourWorkingSet {
        hits: ready_draws.saturating_add(refused_draws),
        pipeline_not_ready,
    };

    if census_on {
        census.ready_draws = ready_draws;
        census.frame.bsp_submit_refused_surfaces = bsp_submit_refused_surfaces;
        census.frame.bsp_drawn_surfaces = bsp_drawn_surfaces;
        census.frame.bsp_draw_refused_surfaces = bsp_draw_refused_surfaces;
        census.refused_draws = refused_draws;
        census.gpu_ready = Some(gpu_indexed);
        census.frame.set_bind_group_n = Some(record_n.total());
        census.frame.set_state_n = Some(record_n.state);
        census.frame.multi_draw_n = Some(record_n.multi_draws);
        census.frame.multi_draw_commands_n = Some(record_n.multi_draw_commands);
        census.set_bind_group0_n = Some(record_n.group0);
        census.set_bind_group1_n = Some(record_n.group1);
        census.material_runs_n = Some(colour_run_census.material_runs);
        census.pass_setups_n = Some(colour_run_census.pass_setups);
        census.obj_binds_n = Some(colour_run_census.obj_binds);
        census.shell_hits_n = Some(colour_run_census.shell_hits);
        census.shell_misses_n = Some(colour_run_census.shell_misses);
        census.overlay_const_writes_n = Some(colour_run_census.overlay_const_writes);
        census.overlay_need_known_n = Some(colour_run_census.overlay_need_known);
        census.frame.smodel_reuse_n = Some(0);
        census.frame.xmodel_reuse_n = Some(0);
        if census.frame.gpu_prepared.is_none() {
            census.frame.gpu_prepared = Some(prepared_hits);
            census.frame.gpu_world_ready = Some(0);
            census.frame.gpu_smodel_ready = Some(0);
            census.frame.gpu_xmodel_ready = Some(0);
        }
        if census.frame.pack_arena_share_n.is_none() {
            census.frame.pack_arena_share_n = Some(0);
            census.frame.pack_arena_vertex_n = Some(0);
            census.frame.pack_arena_pixel_n = Some(0);
        }
        if census.frame.encoder_finish_ms.is_none() {
            census.frame.encoder_finish_ms = Some(0.0);
        }
        if census.frame.submit_arena_ms.is_none() {
            census.frame.submit_arena_ms = Some(0.0);
        }
        if census.frame.submit_record_ms.is_none() {
            census.frame.submit_record_ms = Some(0.0);
        }
        let mut ranked: Vec<_> = refusals.submit.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        census.frame.submit_cause = ranked
            .first()
            .map(|((family, cause), n)| format!("{family}:{cause}:{n}"));
        census.frame.submit_cause2 = ranked
            .get(1)
            .map(|((family, cause), n)| format!("{family}:{cause}:{n}"));
        census.frame.gpu_not_ready_n = Some(
            refusals
                .submit
                .iter()
                .filter(|((_, cause), _)| *cause == "PipelineNotReady")
                .map(|(_, n)| *n)
                .sum(),
        );
        census.frame.gpu_no_port_n = Some(
            refusals
                .submit
                .iter()
                .filter(|((_, cause), _)| *cause == "NoExactPort")
                .map(|(_, n)| *n)
                .sum(),
        );
        census.frame.pnr_smodel_mat = rank_count_map(&pnr_smodel_mats, 8);
        census.frame.pnr_world_mat = rank_count_map(&pnr_world_mats, 8);
        census.frame.pnr_smodel_ps = rank_count_map(&pnr_smodel_ps, 8);
        census.frame.pnr_world_ps = rank_count_map(&pnr_world_ps, 8);
        census.frame.pnr_smodel_key_n = Some(as_u32(pnr_smodel_keys.len()));
        census.frame.pnr_world_key_n = Some(as_u32(pnr_world_keys.len()));
        census.frame.pnr_port_n = Some(as_u32(pnr_ports.len()));
        census.frame.gpu_smodel_bind_mat = rank_count_map(&bind_smodel_mats, 8);
    }
    let focused_prepared_surfaces = focused_object_id.map_or(0, |object_id| {
        submitted_keys
            .iter()
            .filter(|key| drawsurf_object_id(**key) == object_id)
            .count() as u32
    });
    let focused_prepared_passes = focused_object_id.map_or(0, |object_id| {
        prepared
            .iter()
            .filter(|draw| draw.owner_object_id == Some(object_id))
            .count() as u32
    });
    emit_focused_owner_submit(
        &products,
        &mut focus_submit,
        Some((
            &extracted.world.sorted_material_names,
            &extracted.world.image_handles,
        )),
        exec_tables(extracted),
        "complete",
        focused_prepared_surfaces,
        focused_prepared_passes,
        focused_drawn_passes,
    );
    census.submitted_keys.clone_from(submitted_keys);
    census
        .world_exec_ready_keys
        .clone_from(world_exec_ready_keys);

    let xmodel_draws = prepared
        .iter()
        .filter(|d| d.tess == ExactTessBind::XModel)
        .count();
    let codemesh_draws = prepared
        .iter()
        .filter(|d| d.tess == ExactTessBind::CodeMesh)
        .count();
    let markmesh_draws = prepared
        .iter()
        .filter(|d| d.tess == ExactTessBind::MarkMesh)
        .count();
    let glassmesh_draws = prepared
        .iter()
        .filter(|d| d.tess == ExactTessBind::Glass)
        .count();
    if census_on {
        census.frame.markmesh_prepared = Some(as_u32(markmesh_draws));
        census.frame.glassmesh_prepared = Some(as_u32(glassmesh_draws));
        census.log_frame = census.log_frame.wrapping_add(1);
        if census.log_frame == 1 || census.log_frame.is_multiple_of(64) {
            let mut authored_state = AuthoredStateCensus::default();
            for draw in &prepared {
                authored_state.note(draw.state.authored_host_fields());
            }
            diag::warn!(
                World,
                "drawsurf production gpu submit: material_runs={} pass_setups={} obj_binds={} shell_hits={} shell_misses={} overlay_const_writes={} overlay_need_known={} ready_draws={ready_draws} refused_draws={refused_draws} exec_refused={exec_refused} authored_state={authored_state:?} unsupported_state={unsupported_state:?} exec_causes={} submit_cause={} submit_cause2={} prepared={} xmodel_prepared={xmodel_draws} codemesh_prepared={codemesh_draws} markmesh_prepared={markmesh_draws} glassmesh_prepared={glassmesh_draws} floatz_blit={floatz_blit} resolved_scene_copy={resolved_scene_copy} viewmodel_held={viewmodel_held} scene_tables(before_linear,before_srgb,after_linear,after_srgb)(2d,cube,3d,samplers)={:?} texture_table_rebuilds={} shadow_table={:?} cached_slot_words={} tex_bind(hit,miss)=({},{}) constant_bind_groups={} indirect(folded,batches,uploaded_words)=({},{},{}) last={last_refusal:?}",
                colour_run_census.material_runs,
                colour_run_census.pass_setups,
                colour_run_census.obj_binds,
                colour_run_census.shell_hits,
                colour_run_census.shell_misses,
                colour_run_census.overlay_const_writes,
                colour_run_census.overlay_need_known,
                rank_pair_map(&refusals.exec, 2)
                    .as_deref()
                    .unwrap_or("none"),
                census.frame.submit_cause.as_deref().unwrap_or("none"),
                census.frame.submit_cause2.as_deref().unwrap_or("none"),
                prepared.len(),
                texture_table.0.each_ref().map(ExactTextureTable::census),
                texture_table
                    .iter()
                    .map(|table| table.rebuild_n)
                    .sum::<u32>(),
                shadow_table.census(),
                binding_cache.interned_n() + shadow_binding.textures.len(),
                prepare_cost.tex_bind_hit_n,
                prepare_cost.tex_bind_miss_n,
                constant_arena
                    .gpu
                    .iter()
                    .filter(|arena| arena.bind_group.is_some())
                    .count(),
                indirect.folded,
                indirect.batches,
                indirect.uploaded_words,
            );
        }
    }
    scratch.prepared = prepared;
}

pub(super) fn copy_submit_prepare_ms(
    census: Res<ExactColourSubmitCensus>,
    slot: Option<Res<crate::diag::render_frame_diag::SharedRenderStagesSlot>>,
) {
    if !perf::recording() {
        return;
    }

    for (counter, value) in [
        (perf::Counter::CounterDipsColour, census.gpu_ready),
        (perf::Counter::CounterDipsSun, census.frame.sun_shadow_gpu),
        (perf::Counter::CounterBindGroup0, census.set_bind_group0_n),
        (perf::Counter::CounterBindGroup1, census.set_bind_group1_n),
        (perf::Counter::CounterCmdState, census.frame.set_state_n),
        (
            perf::Counter::CounterOverlayConstWrites,
            census.overlay_const_writes_n,
        ),
        (perf::Counter::CounterMultiDraws, census.frame.multi_draw_n),
        (
            perf::Counter::CounterMultiDrawCommands,
            census.frame.multi_draw_commands_n,
        ),
    ] {
        if let Some(value) = value {
            counter.emit(f64::from(value));
        }
    }
    for (counter, value) in [
        (
            perf::Counter::RenderSubmitSunMs,
            census.frame.sun_shadow_submit_ms,
        ),
        (
            perf::Counter::RenderSubmitGatherMs,
            census.frame.submit_gather_ms,
        ),
        (
            perf::Counter::RenderSubmitPrepareMs,
            census.frame.submit_prepare_ms,
        ),
        (
            perf::Counter::RenderSubmitArenaMs,
            census.frame.submit_arena_ms,
        ),
        (
            perf::Counter::RenderSubmitRecordMs,
            census.frame.submit_record_ms,
        ),
    ] {
        if let Some(value) = value {
            counter.emit(f64::from(value));
        }
    }
    let Some(slot) = slot else {
        return;
    };
    if let Ok(mut guard) = slot.0.lock() {
        guard.submit_prepare_ms = census.frame.submit_prepare_ms;
        guard.colour_submit_ms = census.frame.colour_submit_ms;
        guard.submit_encode_ms = census.frame.submit_encode_ms;
        guard.submit_gather_ms = census.frame.submit_gather_ms;
        guard.pass_end_ms = census.frame.pass_end_ms;
        guard.encoder_finish_ms = census.frame.encoder_finish_ms;
        guard.submit_arena_ms = census.frame.submit_arena_ms;
        guard.submit_record_ms = census.frame.submit_record_ms;
        guard.pack_intern_hit_n = census.frame.pack_intern_hit_n;
        guard.pack_intern_miss_n = census.frame.pack_intern_miss_n;
        guard.pack_arena_share_n = census.frame.pack_arena_share_n;
        guard.gpu_exec_reuse_n = census.frame.gpu_exec_reuse_n;
        guard.gpu_exec_unique_n = census.frame.gpu_exec_unique_n;
        guard.pack_overlay_n = census.frame.pack_overlay_n;
        guard.pack_overlay_row_n = census.frame.pack_overlay_row_n;
        guard.pack_overlay_pixel_share_n = census.frame.pack_overlay_pixel_share_n;
        guard.pack_arena_vertex_n = census.frame.pack_arena_vertex_n;
        guard.pack_arena_pixel_n = census.frame.pack_arena_pixel_n;
        guard.pack_seed_n = census.frame.pack_seed_n;
        guard.pack_walk_n = census.frame.pack_walk_n;
        guard.tex_bind_hit_n = census.frame.tex_bind_hit_n;
        guard.tex_bind_miss_n = census.frame.tex_bind_miss_n;
        guard.markmesh_hits = census.frame.markmesh_hits;
        guard.markmesh_prepared = census.frame.markmesh_prepared;
        guard.last_markmesh_refusal = census.frame.last_markmesh_refusal.clone();
        guard.last_markmesh_exec_skip = census.frame.last_markmesh_exec_skip.clone();
        guard.markmesh_missing_58 = census.frame.markmesh_missing_58;
        guard.last_mark_packed_custom = census.frame.last_mark_packed_custom;
        guard.last_mark_packed_scene_light = census.frame.last_mark_packed_scene_light;
        guard.last_mark_lmap_sampler = census.frame.last_mark_lmap_sampler;
        guard.glassmesh_hits = census.frame.glassmesh_hits;
        guard.glassmesh_prepared = census.frame.glassmesh_prepared;
        guard.last_glassmesh_exec_skip = census.frame.last_glassmesh_exec_skip.clone();
        guard.last_glassmesh_refusal = census.frame.last_glassmesh_refusal.clone();
        guard.last_glass_packed_probe = census.frame.last_glass_packed_probe;
        guard.last_glass_probe_sampler = census.frame.last_glass_probe_sampler;
        guard.gpu_ready = census.gpu_ready;
        guard.set_bind_group_n = census.frame.set_bind_group_n;
        guard.gpu_prepared = census.frame.gpu_prepared;
        guard.gpu_world_ready = census.frame.gpu_world_ready;
        guard.bsp_submitted_surfaces = census.frame.bsp_submitted_surfaces;
        guard.bsp_submit_refused_surfaces = census.frame.bsp_submit_refused_surfaces;
        guard.bsp_drawn_surfaces = census.frame.bsp_drawn_surfaces;
        guard.bsp_draw_refused_surfaces = census.frame.bsp_draw_refused_surfaces;
        guard.gpu_smodel_ready = census.frame.gpu_smodel_ready;
        guard.gpu_xmodel_ready = census.frame.gpu_xmodel_ready;
        guard.end_depth_restore_n = census.frame.end_depth_restore_n;
        guard.end_depth_range_type = census.frame.end_depth_range_type;
        guard.code_mesh_gpu_kind = census.code_mesh_gpu_kind;
        guard.sun_shadow_gpu = census.frame.sun_shadow_gpu;
        guard.sun_shadow_gpu_miss = census.frame.sun_shadow_gpu_miss;
        guard.sun_shadow_gpu_cause = census.frame.sun_shadow_gpu_cause.clone();
        guard.sun_shadow_gpu_causes = census.frame.sun_shadow_gpu_causes.clone();
        guard.spot_shadow_gpu = census.frame.spot_shadow_gpu;
        guard.spot_shadow_gpu_miss = census.frame.spot_shadow_gpu_miss;
        guard.spot_shadow_gpu_cause = census.frame.spot_shadow_gpu_cause.clone();
        guard.spot_shadow_slot_n = census.frame.spot_shadow_slot_n;
        guard.sun_shadow_submit_ms = census.frame.sun_shadow_submit_ms;
        guard.sun_shadow_prepare_ms = census.sun_shadow_prepare_ms;
        guard.sun_shadow_patch_ms = census.sun_shadow_patch_ms;
        guard.sun_shadow_arena_ms = census.sun_shadow_arena_ms;
        guard.sun_shadow_record_ms = census.sun_shadow_record_ms;
        guard.sun_shadow_finish_ms = census.sun_shadow_finish_ms;
        guard.sun_shadow_queue_ms = census.sun_shadow_queue_ms;
        guard.sun_shadow_unnamed_ms = census.sun_shadow_unnamed_ms;
        guard.sun_shadow_static_hit = census.sun_shadow_static_hit;
        guard.sun_shadow_world_ib_n = census.frame.sun_shadow_world_ib_n;
        guard.sun_shadow_static_n = census.sun_shadow_static_n;
        guard.sun_shadow_dynamic_n = census.sun_shadow_dynamic_n;
        guard.sun_shadow_wvp_intern_hit = census.sun_shadow_wvp_intern_hit;
        guard.sun_shadow_wvp_intern_miss = census.sun_shadow_wvp_intern_miss;
        guard.sun_shadow_wvp_intern_n = census.sun_shadow_wvp_intern_n;
        guard.sun_shadow_wvp_unique_base = census.sun_shadow_wvp_unique_base;
        guard.sun_shadow_wvp_unique_wvp = census.sun_shadow_wvp_unique_wvp;
        guard.sun_shadow_state_pipe_n = census.sun_shadow_state_pipe_n;
        guard.sun_shadow_state_tess_n = census.sun_shadow_state_tess_n;
        guard.sun_shadow_state_bind_n = census.sun_shadow_state_bind_n;
        guard.sun_shadow_state_off_n = census.sun_shadow_state_off_n;
        guard.sun_shadow_state_group_n = census.sun_shadow_state_group_n;
        guard.sun_shadow_state_run_n = census.sun_shadow_state_run_n;
        guard.sun_shadow_state_run_max = census.sun_shadow_state_run_max;
        guard.sun_shadow_state_top10 = census.sun_shadow_state_top10;
        guard.world_index_gaps = census.frame.world_index_gaps;
        guard.world_run_indices_n = census.frame.world_run_indices_n;
        guard.world_material_runs = census.frame.world_material_runs;
        guard.world_material_runs_seq = census.frame.world_material_runs_seq;
        guard.world_key_runs = census.frame.world_key_runs;
        guard.world_key_runs_seq = census.frame.world_key_runs_seq;
        guard.world_mixed_breaks = census.frame.world_mixed_breaks;
        guard.world_gathered = census.frame.world_gathered;
        guard.world_ib_skip = census.frame.world_ib_skip;
        guard.world_gpu_runs = census.frame.world_gpu_runs;
        guard.world_gpu_runs_seq = census.frame.world_gpu_runs_seq;
        guard.world_sampler_runs_seq = census.frame.world_sampler_runs_seq;
        guard.world_probe_runs_seq = census.frame.world_probe_runs_seq;
        guard.world_light_runs_seq = census.frame.world_light_runs_seq;
        guard.smodel_reuse_n = census.frame.smodel_reuse_n;
        guard.xmodel_reuse_n = census.frame.xmodel_reuse_n;
        guard.xmodel_material_runs = census.frame.xmodel_material_runs;
        guard.smodel_index_gaps = census.frame.smodel_index_gaps;
        guard.smodel_material_runs = census.frame.smodel_material_runs;
        guard.smodel_material_runs_seq = census.frame.smodel_material_runs_seq;
        guard.smodel_material_run_max = census.frame.smodel_material_run_max;
        guard.smodel_same_surface_n = census.frame.smodel_same_surface_n;
        guard.smodel_unique_surfaces = census.frame.smodel_unique_surfaces;
        guard.smodel_hits = census.frame.smodel_hits;
        guard.smodel_lighting_runs = census.frame.smodel_lighting_runs;
        guard.smodel_lighting_run_max = census.frame.smodel_lighting_run_max;
        guard.smodel_pretess_runs = census.frame.smodel_pretess_runs;
        guard.smodel_pretess_hits = census.frame.smodel_pretess_hits;
        guard.smodel_pretess_verts = census.frame.smodel_pretess_verts;
        guard.smodel_pretess_indices = census.frame.smodel_pretess_indices;
        guard.smodel_cached_lighting = census.frame.smodel_cached_lighting;
        guard.smodel_pretess_local = census.frame.smodel_pretess_local;
        guard.smodel_pretess_length1 = census.frame.smodel_pretess_length1;
        guard.smodel_pretess_skip = census.frame.smodel_pretess_skip;
        guard.submit_cause = census.frame.submit_cause.clone();
        guard.submit_cause2 = census.frame.submit_cause2.clone();
        guard.gpu_not_ready_n = census.frame.gpu_not_ready_n;
        guard.gpu_no_port_n = census.frame.gpu_no_port_n;
        guard.pnr_smodel_mat = census.frame.pnr_smodel_mat.clone();
        guard.pnr_world_mat = census.frame.pnr_world_mat.clone();
        guard.pnr_smodel_ps = census.frame.pnr_smodel_ps.clone();
        guard.pnr_world_ps = census.frame.pnr_world_ps.clone();
        guard.pnr_smodel_key_n = census.frame.pnr_smodel_key_n;
        guard.pnr_world_key_n = census.frame.pnr_world_key_n;
        guard.pnr_port_n = census.frame.pnr_port_n;
        guard.gpu_smodel_bind_mat = census.frame.gpu_smodel_bind_mat.clone();
    }
}
