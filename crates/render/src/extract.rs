use std::sync::Arc;
use std::time::Instant;

use bevy::prelude::*;
use bevy::render::Extract;
use render_frontend::assemble::drawsurf::tess::fx::FxCodeMeshPlan;
use render_frontend::assemble::drawsurf::tess::glass::GfxGlassMeshPlan;
use render_frontend::assemble::drawsurf::tess::mark::GfxMarkMeshPlan;
use render_frontend::assemble::drawsurf::tess::particle_cloud::FxParticleCloudPlan;
use render_frontend::assemble::drawsurf::tess::smodel::SmodelGpuPlan;
use render_frontend::assemble::drawsurf::tess::world::WorldDrawGpuPlan;
use render_frontend::assemble::drawsurf::tess::xmodel::XModelDrawPlan;
use render_gpu::diag::render_frame_diag::SharedRenderStagesSlot;
use render_gpu::{
    ExtractedRenderFrameProducts, ExtractedRuntimeImageHandles, InstalledRenderWorld,
    PublishedRenderFrame, SamplerTable,
};

fn take_published<T>(share: Option<&Arc<Vec<T>>>) -> (Arc<Vec<T>>, u32) {
    match share {
        Some(rows) => (Arc::clone(rows), 1),
        None => (Arc::new(Vec::new()), 0),
    }
}

fn overlay_world_shares(
    plan: Option<&WorldDrawGpuPlan>,
) -> (
    Arc<Vec<render_frame::WorldVertex>>,
    Arc<Vec<u32>>,
    Arc<Vec<(u32, u32)>>,
) {
    match plan {
        Some(plan) => (
            take_published(plan.decoded_share.as_ref()).0,
            take_published(plan.index_share.as_ref()).0,
            take_published(plan.range_share.as_ref()).0,
        ),
        None => (
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
        ),
    }
}

fn overlay_smodel_shares(
    plan: Option<&SmodelGpuPlan>,
) -> (
    Arc<Vec<render_frame::SmodelVertex>>,
    Arc<Vec<u32>>,
    Arc<Vec<(u32, u32)>>,
) {
    match plan {
        Some(plan) => (
            take_published(plan.decoded_share.as_ref()).0,
            take_published(plan.index_share.as_ref()).0,
            take_published(plan.range_share.as_ref()).0,
        ),
        None => (
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
        ),
    }
}

fn overlay_xmodel_shares(
    plan: Option<&XModelDrawPlan>,
) -> (
    Arc<Vec<render_frame::SmodelVertex>>,
    Arc<Vec<u32>>,
    Arc<Vec<(u32, u32)>>,
) {
    match plan {
        Some(plan) => (
            take_published(plan.decoded_share.as_ref()).0,
            take_published(plan.index_share.as_ref()).0,
            take_published(plan.range_share.as_ref()).0,
        ),
        None => (
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
        ),
    }
}

fn insert_empty_colour(commands: &mut Commands) {
    commands.insert_resource(InstalledRenderWorld::default());
    commands.insert_resource(PublishedRenderFrame::default());
}

fn world_colour_extract_counts(plan: Option<&WorldDrawGpuPlan>) -> (usize, usize, usize) {
    match plan {
        Some(plan) => match plan.exact_packed_vertices() {
            Ok(vertices) => (
                vertices.len(),
                plan.indices().len(),
                plan.vertex_layer_rows().len(),
            ),
            Err(_) => (0, 0, 0),
        },
        None => (0, 0, 0),
    }
}

fn smodel_colour_extract_counts(plan: Option<&SmodelGpuPlan>) -> (usize, usize) {
    match plan {
        Some(plan) => match plan.exact_packed_vertices() {
            Ok(vertices) => (vertices.len(), plan.indices().len()),
            Err(_) => (0, 0),
        },
        None => (0, 0),
    }
}

pub fn extract_exact_colour(
    mut commands: Commands,
    sealed: Extract<Option<Res<PublishedRenderFrame>>>,
    existing: Option<Res<PublishedRenderFrame>>,
    mut focus_submit: ResMut<render_gpu::FocusedOwnerSubmitState>,
    slot: Option<Res<SharedRenderStagesSlot>>,
) {
    let started = Instant::now();
    if let Some(existing) = existing.as_ref() {
        render_gpu::emit_focused_owner_submit(
            &existing.frame_products,
            &mut focus_submit,
            None,
            None,
            "render_not_scheduled",
            0,
            0,
            0,
        );
    }
    if let Some(sealed) = sealed.as_ref() {
        commands.insert_resource(sealed.world().clone());
        commands.insert_resource((**sealed).clone());
    } else {
        insert_empty_colour(&mut commands);
    }
    if let Some(slot) = slot {
        slot.stamp_extract_products(started.elapsed().as_secs_f32() * 1000.0, 1, 0);
        slot.stamp_extract_colour(0.0, 0.0, 1, 0, 0, 1, 1, 0);
    }
}

pub fn seal_render_frame(
    mut commands: Commands,
    material: (
        Option<Res<render_frontend::assemble::drawsurf::MaterialGeneration>>,
        Option<Res<render_frontend::assemble::drawsurf::MaterialFrameInputs>>,
        Option<Res<render_frontend::assemble::drawsurf::FrameAssemblyInputs>>,
        Option<Res<render_frontend::assemble::drawsurf::RenderFrameProducts>>,
    ),
    world: Option<Res<WorldDrawGpuPlan>>,
    smodel: Option<Res<SmodelGpuPlan>>,
    smc: Option<Res<render_frontend::prepare::scene::smodel_geom_cache::WorldStaticModelCache>>,
    static_identity: (
        Option<Res<render_frontend::assemble::drawsurf::StaticDrawLane>>,
        Option<Res<frame::WorldGeneration>>,
        Option<Res<frame::WorldProducts>>,
    ),
    xmodel: Option<Res<XModelDrawPlan>>,
    fx: Option<Res<FxCodeMeshPlan>>,
    particle_cloud: Option<Res<FxParticleCloudPlan>>,
    mark_mesh: Option<Res<GfxMarkMeshPlan>>,
    glass_mesh: Option<Res<GfxGlassMeshPlan>>,
    samplers: Option<Res<SamplerTable>>,
    images: Option<Res<render_frontend::assemble::drawsurf::RuntimeImageHandles>>,
    spawn_job: Option<Res<render_gpu::GpuSubmitReady>>,
    sun: (
        Option<Res<render_frontend::assemble::drawsurf::MapSunEffects>>,
        Option<Res<render_frontend::assemble::drawsurf::SunEffectsFrameInput>>,
    ),
    (existing_world, existing_frame): (
        Option<Res<InstalledRenderWorld>>,
        Option<Res<PublishedRenderFrame>>,
    ),
) {
    let (retained, world_generation, world_products) = static_identity;
    let world_products = world_products.map(|products| *products).unwrap_or_default();
    let (runtime, mat_frame, assembly, products) = material;
    let Some(runtime) = runtime.as_ref() else {
        insert_empty_colour(&mut commands);
        return;
    };
    let Some(mat_frame) = mat_frame.as_ref() else {
        insert_empty_colour(&mut commands);
        return;
    };
    let generation = runtime.catalog.generation_id();
    let world_generation = world_generation
        .as_ref()
        .map(|generation| **generation)
        .unwrap_or(frame::WorldGeneration(None));
    let Some(products) = products.as_ref() else {
        insert_empty_colour(&mut commands);
        return;
    };
    let snapshot = products.published();
    let coherent =
        frame_submission_matches(assembly.as_deref(), &snapshot, generation, world_generation);
    if !coherent {
        insert_empty_colour(&mut commands);
        return;
    }
    let frame_products = ExtractedRenderFrameProducts(snapshot);
    let cpu_port_len = runtime.programs.ports().len();
    let skip_ports = existing_world.as_ref().is_some_and(|world| {
        render_gpu::colour_ports_static(
            world.generation,
            world.ports.len(),
            generation,
            cpu_port_len,
        )
    });
    let (ports, _ports_ms) = if skip_ports {
        (Vec::new(), 0.0_f32)
    } else {
        let ports_started = Instant::now();

        let ports: Vec<render_gpu::AdmittedExactPort> = runtime
            .programs
            .ports()
            .iter()
            .map(|port| render_gpu::AdmittedExactPort {
                id: port.id(),
                abi: port.abi().clone(),
                module: port.shared_module(),
                layout: port.wgpu_layout().clone(),
            })
            .collect();
        (ports, ports_started.elapsed().as_secs_f32() * 1000.0)
    };
    let warm_pipelines = spawn_job.as_ref().is_some_and(|job| job.warm_pipelines);
    let (world_v, world_i, world_layer_n) =
        world_colour_extract_counts(world.as_ref().map(|plan| &**plan));
    let (smodel_v, smodel_i) = smodel_colour_extract_counts(smodel.as_ref().map(|plan| &**plan));

    let skip_world_smodel = existing_world.as_ref().is_some_and(|world| {
        render_gpu::colour_world_smodel_static(
            (world.world_generation, world.world_products),
            world.static_geometry.world_vertices.len(),
            world.static_geometry.world_indices.len(),
            world.static_geometry.world_layer.len(),
            world.static_geometry.smodel_vertices.len(),
            world.static_geometry.smodel_indices.len(),
            (world_generation, world_products),
            world_v,
            world_i,
            world_layer_n,
            smodel_v,
            smodel_i,
        )
    });
    let smc_revision = smc.as_ref().map(|cache| cache.content_revision());
    let skip_smc_maps = skip_world_smodel
        && existing_world
            .as_ref()
            .is_some_and(|world| world.smc_revision == smc_revision);
    let empty_static = || {
        (
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            None,
        )
    };
    let (world_vertices, world_layer, world_indices, world_surface_ranges, world_vertex_refusal) =
        if skip_world_smodel {
            empty_static()
        } else {
            match world.as_ref() {
                Some(plan) => match plan.exact_packed_vertices() {
                    Ok(_) => {
                        let (verts, _) = take_published(plan.vertex_share.as_ref());
                        let (layer, _) = take_published(plan.layer_share.as_ref());
                        let (inds, _) = take_published(plan.index_share.as_ref());
                        let (ranges, _) = take_published(plan.range_share.as_ref());
                        (verts, layer, inds, ranges, None)
                    }
                    Err(cause) => {
                        let (empty_v, empty_l, empty_i, empty_r, _) = empty_static();
                        (empty_v, empty_l, empty_i, empty_r, Some(cause))
                    }
                },
                None => empty_static(),
            }
        };
    let empty_smodel = || {
        (
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            None,
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
        )
    };
    let (
        smodel_vertices,
        smodel_indices,
        smodel_surface_ranges,
        smodel_vertex_refusal,
        smodel_cached_vertices,
        smodel_surface_verts,
    ) = if skip_world_smodel {
        empty_smodel()
    } else {
        match smodel.as_ref() {
            Some(plan) => match plan.exact_packed_vertices() {
                Ok(_) => {
                    let (verts, _) = take_published(plan.packed_share.as_ref());
                    let (inds, _) = take_published(plan.index_share.as_ref());
                    let (ranges, _) = take_published(plan.range_share.as_ref());
                    let (cached, _) = take_published(plan.cached_share.as_ref());
                    let (vert_ranges, _) = take_published(plan.vert_range_share.as_ref());
                    (verts, inds, ranges, None, cached, vert_ranges)
                }
                Err(cause) => {
                    let (empty_v, empty_i, empty_r, _, empty_c, empty_vr) = empty_smodel();
                    (empty_v, empty_i, empty_r, Some(cause), empty_c, empty_vr)
                }
            },
            None => empty_smodel(),
        }
    };
    let smodel_vertex_lighting = match smodel.as_ref() {
        Some(plan) if !skip_world_smodel && smodel_vertex_refusal.is_none() => {
            take_published(plan.vertex_lighting_share.as_ref()).0
        }
        _ => Arc::new(Vec::new()),
    };
    let mut smc_vb_patches = Vec::new();
    let mut smc_ib_patches = Vec::new();
    let mut smc_index_baked = Vec::new();
    let smodel_pretess_indices = retained
        .as_ref()
        .map(|retained| Arc::clone(&retained.smodel_pretess_indices))
        .unwrap_or_else(|| Arc::new(Vec::new()));
    let smodel_index_layout_revision = retained
        .as_ref()
        .map(|retained| retained.smodel_index_layout_revision)
        .unwrap_or(0);
    if let Some(cache) = smc.as_ref() {
        smc_vb_patches = cache.vb_patches().to_vec();
        smc_ib_patches = cache.ib_patches().to_vec();
        if !skip_smc_maps {
            smc_index_baked = cache.baked_cache_indices();
        }
    }
    let (
        xmodel_vertices,
        xmodel_indices,
        xmodel_surface_ranges,
        xmodel_vertex_refusal,
        _xmodel_arc,
        _xmodel_i_arc,
        _xmodel_r_arc,
    ) = match xmodel.as_ref() {
        Some(plan) => match plan.exact_packed_vertices() {
            Ok(_) => {
                let (verts, packed_arc) = take_published(plan.packed_share.as_ref());
                let (inds, i_arc) = take_published(plan.index_share.as_ref());
                let (ranges, r_arc) = take_published(plan.range_share.as_ref());
                (verts, inds, ranges, None, packed_arc, i_arc, r_arc)
            }
            Err(cause) => (
                Arc::new(Vec::new()),
                Arc::new(Vec::new()),
                Arc::new(Vec::new()),
                Some(cause),
                0u32,
                1u32,
                1u32,
            ),
        },
        None => (
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            None,
            0u32,
            1u32,
            1u32,
        ),
    };
    let xmodel_packed_segments = xmodel
        .as_ref()
        .map(|plan| plan.packed_segments)
        .unwrap_or_default();
    let xmodel_revision = xmodel.as_ref().map(|plan| plan.revision).unwrap_or(0);
    let xmodel_topology_revision = xmodel
        .as_ref()
        .map(|plan| plan.topology_revision)
        .unwrap_or(0);
    let (
        fx_vertices,
        fx_indices,
        fx_surface_ranges,
        fx_vertex_refusal,
        fx_revision,
        _fx_v_arc,
        _fx_i_arc,
        _fx_r_arc,
    ) = match fx.as_ref() {
        Some(plan) => match plan.exact_packed_vertices() {
            Ok(_) => {
                let verts = Arc::clone(&plan.vertices);
                let inds = Arc::clone(&plan.indices);
                let (ranges, r_arc) = take_published(plan.range_share.as_ref());
                (verts, inds, ranges, None, plan.revision, 1u32, 1u32, r_arc)
            }
            Err(cause) => (
                Arc::new(Vec::new()),
                Arc::new(Vec::new()),
                Arc::new(Vec::new()),
                Some(cause),
                plan.revision,
                1u32,
                1u32,
                1u32,
            ),
        },
        None => (
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            None,
            0,
            1u32,
            1u32,
            1u32,
        ),
    };
    let (
        particle_cloud_vertices,
        particle_cloud_indices,
        particle_cloud_surface_ranges,
        particle_cloud_template,
        particle_cloud_revision,
        _particle_r_arc,
    ) = match particle_cloud.as_ref() {
        Some(plan) => {
            let (ranges, r_arc) = take_published(plan.range_share.as_ref());
            (
                Arc::clone(&plan.vertices),
                Arc::clone(&plan.indices),
                ranges,
                plan.template_counts(),
                plan.revision,
                r_arc,
            )
        }
        None => (
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            (0, 0),
            0,
            1u32,
        ),
    };
    let (
        mark_mesh_vertices,
        mark_mesh_indices,
        mark_mesh_surface_ranges,
        mark_mesh_revision,
        _mark_v_arc,
        _mark_i_arc,
        _mark_r_arc,
    ) = match mark_mesh.as_ref() {
        Some(plan) => {
            let verts = Arc::clone(&plan.vertices);
            let inds = Arc::clone(&plan.indices);
            let (ranges, r_arc) = take_published(plan.range_share.as_ref());
            (verts, inds, ranges, plan.revision, 1u32, 1u32, r_arc)
        }
        None => (
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            0,
            1u32,
            1u32,
            1u32,
        ),
    };
    let (
        glass_mesh_vertices,
        glass_mesh_indices,
        glass_mesh_surface_ranges,
        glass_mesh_vertex_refusal,
        glass_mesh_revision,
        _glass_v_arc,
        _glass_i_arc,
        _glass_r_arc,
    ) = match glass_mesh.as_ref() {
        Some(plan) => match plan.exact_packed_vertices() {
            Ok(_) => {
                let verts = Arc::clone(&plan.vertices);
                let inds = Arc::clone(&plan.indices);
                let (ranges, r_arc) = take_published(plan.range_share.as_ref());
                (verts, inds, ranges, None, plan.revision, 1u32, 1u32, r_arc)
            }
            Err(cause) => (
                Arc::new(Vec::new()),
                Arc::new(Vec::new()),
                Arc::new(Vec::new()),
                Some(cause),
                plan.revision,
                1u32,
                1u32,
                1u32,
            ),
        },
        None => (
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            Arc::new(Vec::new()),
            None,
            0,
            1u32,
            1u32,
            1u32,
        ),
    };
    let image_handles = images
        .as_ref()
        .map(|handles| (**handles).clone())
        .unwrap_or_default();

    let mut exec_frame = existing_frame
        .as_ref()
        .map(|frame| frame.exec_frame.clone())
        .unwrap_or_default();
    render_frontend::assemble::drawsurf::material_exec::refresh(
        &mut exec_frame,
        mat_frame,
        assembly.as_deref(),
    );
    let mut next_world = render_gpu::RenderWorldData {
        generation,
        catalog: Some(Arc::clone(&runtime.catalog)),
        prepared: Some(Arc::clone(&runtime.prepared)),
        world_generation,
        world_products,
        smc_revision,
        ports: Arc::new(ports),
        static_geometry: Arc::new(render_gpu::ExtractedStaticGeometry {
            world_vertices,
            world_layer,
            world_indices,
            world_surface_ranges,
            world_vertex_refusal,
            smodel_vertices,
            smodel_indices,
            smodel_surface_ranges,
            smodel_vertex_refusal,
            smodel_cached_vertices,
            smodel_surface_verts,
            smodel_vertex_lighting,
        }),
        smc_index_baked: Arc::new(smc_index_baked),
        smodel_pretess_indices,
        smodel_index_layout_revision,
        sampler_table: samplers.as_ref().map(|table| (**table).clone()),
        image_handles,
        sorted_material_names: Arc::new(if skip_ports {
            Vec::new()
        } else {
            render_gpu::dump_sorted_material_names(&runtime.catalog)
        }),
        shader_program_names: Arc::new(if skip_ports {
            Vec::new()
        } else {
            render_gpu::dump_shader_program_names(&runtime.catalog)
        }),
        sun_effects: sun.0.and_then(|sun| sun.def),
    };
    let next_frame = render_gpu::RenderFrameData {
        frame_products,
        generation,
        world_generation,
        exec_frame,
        sun_shadow: mat_frame.sun_shadow,
        sun_effects: sun.1.and_then(|input| input.frame),
        warm_pipelines,
        pipeline_world_materials: spawn_job
            .as_ref()
            .map(|job| job.pipeline_world_materials.clone())
            .unwrap_or_default(),
        pipeline_smodel_materials: spawn_job
            .as_ref()
            .map(|job| job.pipeline_smodel_materials.clone())
            .unwrap_or_default(),
        pipeline_demand_revision: spawn_job
            .as_ref()
            .map(|job| job.pipeline_demand_revision)
            .unwrap_or(0),
        smc_vb_patches,
        smc_ib_patches,
        xmodel_vertices,
        xmodel_indices,
        xmodel_surface_ranges,
        xmodel_vertex_refusal,
        fx_vertices,
        fx_indices,
        fx_surface_ranges,
        fx_vertex_refusal,
        fx_revision,
        xmodel_revision,
        xmodel_topology_revision,
        xmodel_packed_segments,
        particle_cloud_vertices,
        particle_cloud_indices,
        particle_cloud_surface_ranges,
        particle_cloud_template,
        particle_cloud_revision,
        mark_mesh_vertices,
        mark_mesh_indices,
        mark_mesh_surface_ranges,
        mark_mesh_revision,
        glass_mesh_vertices,
        glass_mesh_indices,
        glass_mesh_surface_ranges,
        glass_mesh_revision,
        glass_mesh_vertex_refusal,
        glass_mesh_bounds: glass_mesh
            .as_ref()
            .and_then(|plan| plan.bounds_share.clone())
            .unwrap_or_default(),
    };
    if let Some(existing) = existing_world.as_ref() {
        reuse_installed_rows(
            &mut next_world,
            existing,
            WorldRowReuse {
                static_geometry: skip_world_smodel,
                smc_index_baked: skip_smc_maps,
                ports: skip_ports,
            },
        );
    }
    let world = InstalledRenderWorld::new(next_world);
    commands.insert_resource(world.clone());
    commands.insert_resource(PublishedRenderFrame::seal(world, next_frame));
}

/// Which rows of the installed world the seal decided are still the ones the
/// GPU already holds. The flags come from the static comparisons above; this
/// only moves the previously published handles across so publication does not
/// hand the render world a second copy of geometry it did not rebuild.
#[derive(Clone, Copy, Debug)]
struct WorldRowReuse {
    static_geometry: bool,
    smc_index_baked: bool,
    ports: bool,
}

fn reuse_installed_rows(
    next: &mut render_gpu::RenderWorldData,
    existing: &InstalledRenderWorld,
    reuse: WorldRowReuse,
) {
    if reuse.static_geometry {
        next.static_geometry = existing.static_geometry.clone();
    }
    if reuse.smc_index_baked {
        next.smc_index_baked = existing.smc_index_baked.clone();
    }
    if reuse.ports {
        next.ports = existing.ports.clone();
        next.sorted_material_names = existing.sorted_material_names.clone();
        next.shader_program_names = existing.shader_program_names.clone();
    }
}

fn frame_submission_matches(
    inputs: Option<&render_frontend::assemble::drawsurf::FrameAssemblyInputs>,
    snapshot: &render_frame::FrameProductsSnapshot,
    generation: render_frontend::assemble::drawsurf::MaterialGenerationId,
    world_generation: frame::WorldGeneration,
) -> bool {
    inputs.is_some_and(|inputs| {
        inputs.frame_id == snapshot.frame_id
            && inputs.catalog_generation == generation
            && inputs.world_generation == world_generation
    }) && snapshot.products.iter().all(|product| {
        matches!(product.status, render_frame::FrameProductStatus::Missing(_))
            || product.generation_id == generation
    })
}

pub fn extract_image_handles(
    mut commands: Commands,
    handles: Extract<Option<Res<render_frontend::assemble::drawsurf::RuntimeImageHandles>>>,
    existing: Option<ResMut<ExtractedRuntimeImageHandles>>,
    slot: Option<Res<SharedRenderStagesSlot>>,
) {
    let Some(handles) = handles.as_ref() else {
        commands.insert_resource(ExtractedRuntimeImageHandles::default());
        return;
    };
    let started = Instant::now();
    let extract_images_arc = 1;
    let next = (**handles).clone();
    if let Some(mut existing) = existing {
        existing.handles = next;
    } else {
        commands.insert_resource(ExtractedRuntimeImageHandles { handles: next });
    }
    if let Some(slot) = slot {
        slot.stamp_extract_images(started.elapsed().as_secs_f32() * 1000.0, extract_images_arc);
    }
}

pub fn extract_postfx(
    runtime: Extract<Option<Res<render_frontend::assemble::drawsurf::MaterialGeneration>>>,
    samplers: Extract<Option<Res<SamplerTable>>>,
    frame: Extract<Res<render_frontend::assemble::drawsurf::dof::DofFrame>>,
    film: Extract<Res<render_frontend::assemble::drawsurf::FilmVisionView>>,
    glow_dvars: Extract<Res<render_frontend::assemble::drawsurf::dof::GlowDvars>>,
    draw_method: Extract<Res<render_frontend::assemble::drawsurf::ColourDrawMethod>>,
    scene: Extract<Option<Res<render_frontend::prepare::scene::world::WorldScene>>>,
    minecraft: Extract<Option<Res<render_anim::minecraft_world::MinecraftWorldView>>>,
    mut extracted: ResMut<render_gpu::ExtractedPostFx>,
) {
    use render_frontend::assemble::drawsurf::postfx_plan::RuntimePostFxResources;
    let films = runtime.as_ref().and_then(|r| match &r.postfx {
        RuntimePostFxResources::Ready(films) => Some(films),
        _ => None,
    });
    match films {
        Some(films)
            if extracted.films.first().map(|f| f.generation)
                != films.first().map(|f| f.generation) =>
        {
            extracted.films = films
                .iter()
                .map(|film| render_gpu::ExtractedFilm {
                    name: film.name,
                    generation: film.generation,
                    catalog: std::sync::Arc::clone(&runtime.as_ref().expect("film owner").catalog),
                    port: render_gpu::AdmittedExactPort {
                        id: film.port.id(),
                        abi: film.port.abi().clone(),
                        module: film.port.shared_module(),
                        layout: film.port.wgpu_layout().clone(),
                    },
                    shader: film.shader.clone(),
                    shell: film.shell.clone(),
                })
                .collect();
        }
        None => extracted.films.clear(),
        _ => {}
    }
    let blood = runtime.as_ref().and_then(|r| r.blood.as_ref());
    if extracted.blood.as_ref().map(|b| b.film.generation) != blood.map(|b| b.film.generation) {
        extracted.blood = blood.map(|blood| render_gpu::ExtractedBlood {
            film: render_gpu::ExtractedFilm {
                name: blood.film.name,
                generation: blood.film.generation,
                catalog: std::sync::Arc::clone(&runtime.as_ref().expect("blood owner").catalog),
                port: render_gpu::AdmittedExactPort {
                    id: blood.film.port.id(),
                    abi: blood.film.port.abi().clone(),
                    module: blood.film.port.shared_module(),
                    layout: blood.film.port.wgpu_layout().clone(),
                },
                shader: blood.film.shader.clone(),
                shell: blood.film.shell.clone(),
            },
            texture_slots: blood.texture_slots.clone(),
        });
    }
    // The Minecraft world is graded by its own lightmap and fog, not by the
    // film tweaks and glow of the map it stands in for.
    extracted.vision = if minecraft.as_ref().is_some_and(|view| view.active) {
        None
    } else {
        film.current
    };
    extracted.t6_film_grade = scene.as_ref().and_then(|scene| scene.t6_film_grade);
    extracted.frame = render_gpu::DofFrame {
        dof: render_gpu::DepthOfField {
            view_model_start: frame.dof.view_model_start,
            view_model_end: frame.dof.view_model_end,
            near_start: frame.dof.near_start,
            near_end: frame.dof.near_end,
            far_start: frame.dof.far_start,
            far_end: frame.dof.far_end,
            near_blur: frame.dof.near_blur,
            far_blur: frame.dof.far_blur,
        },
        blur: film.blur,
        grading: film.grading,
        bias: frame.bias,
        scene_near: frame.scene_near,
        view_model_near: frame.view_model_near,
        glow: render_gpu::GlowFrame {
            r_glow: glow_dvars.enable || film.script_forced,
            r_fullbright: !film.script_forced
                && matches!(
                    **draw_method,
                    render_frontend::assemble::drawsurf::ColourDrawMethod::Fullbright
                ),
            ..render_gpu::GlowFrame::from_vision(extracted.vision)
        },
    };
    extracted.sampler = samplers.as_ref().and_then(|s| s.decode(0x62).ok());
    extracted.depth_sampler = samplers.as_ref().and_then(|s| s.decode(0x61).ok());
}

pub fn extract_geometry(
    mut commands: Commands,
    world: Extract<Option<Res<WorldDrawGpuPlan>>>,
    smodel: Extract<Option<Res<SmodelGpuPlan>>>,
    xmodel: Extract<Option<Res<XModelDrawPlan>>>,
    runtime: Extract<Option<Res<render_frontend::assemble::drawsurf::MaterialGeneration>>>,
    dpvs: Extract<Option<Res<render_frontend::prepare::scene::cull::DpvsFrameStats>>>,
    spawn_job: Extract<Option<Res<render_gpu::GpuSubmitReady>>>,
    existing: Option<ResMut<render_gpu::ExtractedDiagnosticGeometry>>,
    slot: Option<Res<SharedRenderStagesSlot>>,
) {
    if !render_gpu::geometry_diagnostic_enabled() {
        return;
    }
    let Some(runtime) = runtime.as_ref() else {
        return;
    };
    let started = Instant::now();
    let g0_world_surfs = dpvs
        .as_ref()
        .map(|stats| stats.g0_world_surfs.clone())
        .unwrap_or_default();
    let generation = runtime.catalog.generation_id();
    let world_v = world
        .as_ref()
        .map_or(0, |plan| plan.decoded_vertices().len());
    let world_i = world.as_ref().map_or(0, |plan| plan.indices().len());
    let smodel_v = smodel
        .as_ref()
        .map_or(0, |plan| plan.decoded_vertices().len());
    let smodel_i = smodel.as_ref().map_or(0, |plan| plan.indices().len());
    let overlay_gpu_wait = spawn_job.as_ref().is_some_and(|job| job.overlay_gpu_wait);
    let skip_world_smodel = overlay_gpu_wait
        || existing.as_ref().is_some_and(|extracted| {
            extracted.generation == generation
                && extracted.world_vertices.len() == world_v
                && extracted.world_indices.len() == world_i
                && extracted.smodel_vertices.len() == smodel_v
                && extracted.smodel_indices.len() == smodel_i
        });
    if skip_world_smodel {
        let Some(mut existing) = existing else {
            if let Some(slot) = slot {
                slot.stamp_extract_diag(started.elapsed().as_secs_f32() * 1000.0);
            }
            return;
        };
        existing.overlay_gpu_wait = overlay_gpu_wait;
        if overlay_gpu_wait {
            if let Some(slot) = slot {
                slot.stamp_extract_diag(started.elapsed().as_secs_f32() * 1000.0);
            }
            return;
        }
        let (_, _, world_ranges) = overlay_world_shares(world.as_ref().map(|plan| plan.as_ref()));
        let (_, _, smodel_ranges) =
            overlay_smodel_shares(smodel.as_ref().map(|plan| plan.as_ref()));
        let (xmodel_vertices, xmodel_indices, xmodel_surface_ranges) =
            overlay_xmodel_shares(xmodel.as_ref().map(|plan| plan.as_ref()));
        existing.world_surface_ranges = world_ranges;
        existing.smodel_surface_ranges = smodel_ranges;
        existing.xmodel_vertices = xmodel_vertices;
        existing.xmodel_indices = xmodel_indices;
        existing.xmodel_surface_ranges = xmodel_surface_ranges;
        existing.xmodel_revision = xmodel.as_ref().map_or(0, |plan| plan.revision);
        existing.g0_world_surfs = g0_world_surfs;
        if let Some(slot) = slot {
            slot.stamp_extract_diag(started.elapsed().as_secs_f32() * 1000.0);
        }
        return;
    }
    let (world_vertices, world_indices, world_surface_ranges) =
        overlay_world_shares(world.as_ref().map(|plan| plan.as_ref()));
    let (smodel_vertices, smodel_indices, smodel_surface_ranges) =
        overlay_smodel_shares(smodel.as_ref().map(|plan| plan.as_ref()));
    let (xmodel_vertices, xmodel_indices, xmodel_surface_ranges) =
        overlay_xmodel_shares(xmodel.as_ref().map(|plan| plan.as_ref()));
    let next = render_gpu::ExtractedDiagnosticGeometry {
        generation,
        overlay_gpu_wait,
        world_vertices,
        world_indices,
        world_surface_ranges,
        smodel_vertices,
        smodel_indices,
        smodel_surface_ranges,
        xmodel_vertices,
        xmodel_indices,
        xmodel_surface_ranges,
        xmodel_revision: xmodel.as_ref().map_or(0, |plan| plan.revision),
        g0_world_surfs,
    };
    if let Some(mut existing) = existing {
        *existing = next;
    } else {
        commands.insert_resource(next);
    }
    if let Some(slot) = slot {
        slot.stamp_extract_diag(started.elapsed().as_secs_f32() * 1000.0);
    }
}

pub fn extract_model_lighting_tiles(
    mut main_world: ResMut<bevy::render::MainWorld>,
    mut uploads: ResMut<render_gpu::ModelLightingTileUploads>,
) {
    let pending = main_world
        .get_resource::<render_scene::ModelLightingAtlasTileWrites>()
        .is_some_and(|writes| !writes.tiles.is_empty());
    if !pending {
        return;
    }
    let mut writes = main_world.resource_mut::<render_scene::ModelLightingAtlasTileWrites>();
    uploads.tiles.extend(
        writes
            .tiles
            .drain(..)
            .map(|tile| render_gpu::ModelLightingTileUpload {
                image: tile.image,
                origin: tile.origin,
                texels: tile.texels,
            }),
    );
}

/// Moves the Minecraft world's new section meshes and state to the render
/// world, converting the atlas once per world.
pub fn extract_minecraft_world(
    mut main_world: ResMut<bevy::render::MainWorld>,
    mut frame: ResMut<render_gpu::MinecraftWorldFrame>,
    mut atlas_of: Local<Option<(usize, std::sync::Arc<render_gpu::MinecraftAtlasImage>)>>,
    mut celestial_of: Local<Option<(usize, std::sync::Arc<render_gpu::MinecraftAtlasImage>)>>,
    mut clouds_of: Local<Option<(usize, std::sync::Arc<render_gpu::MinecraftClouds>)>>,
    mut cracks_of: Local<Option<(usize, std::sync::Arc<render_gpu::MinecraftAtlasImage>)>>,
) {
    let Some(mut view) =
        main_world.get_resource_mut::<render_anim::minecraft_world::MinecraftWorldView>()
    else {
        return;
    };
    frame.active = view.active;
    frame.origin = view.origin;
    frame.view_distance = 8.0;
    if frame.generation != view.generation {
        frame.generation = view.generation;
        frame.uploads.clear();
        frame.removed.clear();
    }
    frame.atlas = view.atlas.as_ref().map(|atlas| {
        let key = std::sync::Arc::as_ptr(atlas) as usize;
        if let Some((held, converted)) = atlas_of.as_ref()
            && *held == key
        {
            return converted.clone();
        }
        let mut levels = vec![atlas.pixels.as_raw().clone()];
        levels.extend(atlas.mipmaps.iter().map(|level| level.as_raw().clone()));
        let converted = std::sync::Arc::new(render_gpu::MinecraftAtlasImage {
            width: atlas.pixels.width(),
            height: atlas.pixels.height(),
            levels,
        });
        *atlas_of = Some((key, converted.clone()));
        converted
    });
    let to_pos = |p: minecraft_terrain::sections::SectionPos| [p.0, p.1, p.2];
    frame.removed.extend(view.removed.drain(..).map(to_pos));
    frame
        .uploads
        .extend(
            view.uploads
                .drain(..)
                .map(|(pos, mesh)| render_gpu::MinecraftSectionUpload {
                    pos: to_pos(pos),
                    vertices: bytemuck::cast_slice(&mesh.vertices).to_vec(),
                    indices: mesh.indices,
                    transparent_start: mesh.transparent_start,
                }),
        );
    frame.visible = view.visible.iter().map(|(pos, _)| to_pos(*pos)).collect();
    frame.environment = view.environment;
    frame.eye_light = view.eye_light;
    frame.light_volume = view.light_volume.clone();
    frame.particles = std::mem::take(&mut view.particles);
    frame.cracks = std::mem::take(&mut view.cracks);
    frame.entity_meshes = std::mem::take(&mut view.entity_meshes);
    frame.backdrop = view.backdrop;
    frame.hand = std::mem::take(&mut view.hand);
    frame.hand_clip = view.hand_clip;
    frame.crack_texture = view.crack_texture.as_ref().map(|image| {
        let key = std::sync::Arc::as_ptr(image) as usize;
        if let Some((held, converted)) = cracks_of.as_ref()
            && *held == key
        {
            return converted.clone();
        }
        let converted = std::sync::Arc::new(render_gpu::MinecraftAtlasImage {
            width: image.width(),
            height: image.height(),
            levels: vec![image.as_raw().clone()],
        });
        *cracks_of = Some((key, converted.clone()));
        converted
    });
    frame.celestial = view.celestial.as_ref().map(|image| {
        let key = std::sync::Arc::as_ptr(image) as usize;
        if let Some((held, converted)) = celestial_of.as_ref()
            && *held == key
        {
            return converted.clone();
        }
        let converted = std::sync::Arc::new(render_gpu::MinecraftAtlasImage {
            width: image.width(),
            height: image.height(),
            levels: vec![image.as_raw().clone()],
        });
        *celestial_of = Some((key, converted.clone()));
        converted
    });
    frame.clouds = view.clouds.as_ref().map(|clouds| {
        let key = std::sync::Arc::as_ptr(clouds) as usize;
        if let Some((held, converted)) = clouds_of.as_ref()
            && *held == key
        {
            return converted.clone();
        }
        let converted = std::sync::Arc::new(render_gpu::MinecraftClouds {
            vertices: clouds
                .0
                .iter()
                .map(|v| {
                    let (p, c) = (v.position, v.color);
                    [p[0], p[1], p[2], c[0], c[1], c[2], c[3]]
                })
                .collect(),
            indices: clouds.1.clone(),
        });
        *clouds_of = Some((key, converted.clone()));
        converted
    });
}
