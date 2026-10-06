use std::sync::Arc;
use std::sync::Mutex;

use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;

use crate::adapters::anim::dyn_ent::DynEntCellBits;
use crate::prepare::scene::camera::FlyCamera;
use crate::prepare::scene::cull::{DynEntModelEntity, ScriptModelEntity, StaticModelEntity};
use crate::prepare::scene::world::WorldScene;
use frame::{LaunchReport, MatchInstalled, MatchTornDown, WorldGeneration};
use render_fx::{HostFxSystem, PreparedFxCatalog, PreparedTracers};

pub(crate) use super::world_gpu::WorldGpuReady;

/// Every reflection probe as one flat grey of the probes' mean brightness,
/// for the Minecraft map: its models reflect neither the stand-in map's
/// scenery nor its colour.
fn neutral_reflection_probes(probes: &mut [Option<Image>]) {
    let (mut sum, mut count) = (0u64, 0u64);
    for data in probes.iter().flatten().filter_map(|image| image.data.as_ref()) {
        for texel in data.chunks_exact(4) {
            sum += u64::from(texel[0]) + u64::from(texel[1]) + u64::from(texel[2]);
            count += 3;
        }
    }
    if count == 0 {
        return;
    }
    let grey = (sum / count) as u8;
    for data in probes.iter_mut().flatten().filter_map(|image| image.data.as_mut()) {
        for texel in data.chunks_exact_mut(4) {
            texel[..3].fill(grey);
        }
    }
}

fn hist_u8(values: impl IntoIterator<Item = u8>) -> String {
    let mut hist = std::collections::BTreeMap::<u8, usize>::new();
    let mut n = 0usize;
    for value in values {
        *hist.entry(value).or_default() += 1;
        n += 1;
    }
    let zero = hist.get(&0).copied().unwrap_or(0);
    let body = hist
        .iter()
        .map(|(index, count)| format!("{index}:{count}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!("n={n} zero={zero} {{{body}}}")
}

fn top_census_cause(causes: &std::collections::BTreeMap<String, u32>) -> Option<String> {
    ranked_census_cause(causes, 0)
}

fn ranked_census_cause(
    causes: &std::collections::BTreeMap<String, u32>,
    index: usize,
) -> Option<String> {
    let mut ranked: Vec<(&String, &u32)> = causes.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    ranked
        .get(index)
        .map(|(cause, count)| format!("{cause}:{count}"))
}

fn census_key_is_ifc(key: &str) -> bool {
    key.contains("UnknownOpcode(0x29)")
        || key.contains("UnbalancedControlFlow")
        || key.contains("Ifc")
}

fn census_ifc_n(causes: &std::collections::BTreeMap<String, u32>) -> u32 {
    causes
        .iter()
        .filter(|(key, _)| census_key_is_ifc(key))
        .map(|(_, count)| *count)
        .sum()
}

fn top_unknown_opcode(causes: &std::collections::BTreeMap<String, u32>) -> Option<String> {
    let mut ranked: Vec<(&String, &u32)> = causes
        .iter()
        .filter(|(key, _)| key.starts_with("UnknownOpcode"))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    ranked
        .first()
        .map(|(cause, count)| format!("{cause}:{count}"))
}

const SPAWN_FRAME_BUDGET: std::time::Duration = std::time::Duration::from_millis(40);

pub const LENS_VIEW_SIGNATURE: (TextureFormat, u32) = (TextureFormat::Rgba8Unorm, 1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WorldSpawnPhase {
    #[default]
    Unarmed,
    Yield,
    Programs,

    Admit,
    Images,
    Plan,

    WorldTess,

    Gpu,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldSpawnOwner {
    Coordinator,
    Material,
    Gpu,
    Assemble,
}

impl WorldSpawnPhase {
    pub const fn owner(self) -> WorldSpawnOwner {
        match self {
            Self::Unarmed | Self::Yield | Self::Done => WorldSpawnOwner::Coordinator,
            Self::Programs | Self::Admit => WorldSpawnOwner::Material,
            Self::Images | Self::Gpu => WorldSpawnOwner::Gpu,
            Self::Plan | Self::WorldTess => WorldSpawnOwner::Assemble,
        }
    }
}

#[derive(Resource, Default)]
pub struct WorldSpawnJob {
    pub phase: WorldSpawnPhase,
    pub last_work_ms: f32,

    pub images: super::world_images::WorldImageUpload,

    compile: crate::assemble::drawsurf::MaterialProgramCompile,

    admit: crate::assemble::drawsurf::MaterialProgramAdmit,

    pub gpu_wait: super::world_gpu::WorldGpuWait,

    pub spawn: WorldGeneration,

    last_slice_at: Option<std::time::Instant>,
}

impl WorldSpawnJob {
    pub(crate) fn slice_gap_ms(&mut self, now: std::time::Instant) -> f32 {
        let gap = self
            .last_slice_at
            .map(|at| now.duration_since(at).as_secs_f32() * 1000.0)
            .unwrap_or(0.0);
        self.last_slice_at = Some(now);
        gap
    }

    pub(crate) fn tess_image_handles(
        &self,
    ) -> (
        &[Option<Handle<Image>>],
        &[Option<crate::assemble::drawsurf::RuntimeLightmapHandles>],
        &[Option<Handle<Image>>],
    ) {
        self.images.tess_handles()
    }

    fn needs_programs(&self) -> bool {
        matches!(
            self.phase,
            WorldSpawnPhase::Unarmed | WorldSpawnPhase::Yield | WorldSpawnPhase::Programs
        )
    }

    pub fn request_pending_shaders(
        &mut self,
        shaders: &[bevy::asset::AssetId<bevy::shader::Shader>],
    ) {
        if !self.admit.has_pending() {
            return;
        }
        for id in shaders {
            self.admit.request_if_pending(*id);
        }
    }
}

pub(crate) fn spawn_world(
    mut commands: Commands,
    load: Option<Res<assets::MapLoadProcess>>,
    mut scene: ResMut<WorldScene>,
    mut images: ResMut<Assets<Image>>,
    mut common_images: ResMut<super::world_images::ResidentGpuImages>,
    shaders: Res<Assets<bevy::shader::Shader>>,
    mut job: ResMut<WorldSpawnJob>,
    mut tess: ResMut<render_scene::TessMaterials>,
    mut fx_host: Option<ResMut<HostFxSystem>>,
    gpu: Option<Res<WorldGpuReady>>,
    tracers: Option<Res<PreparedTracers>>,
    fx_catalog: Option<Res<PreparedFxCatalog>>,
    report: Option<Res<LaunchReport>>,
    present_ack: Res<WorldPresentAck>,
    prepared: (
        Res<render_anim::PreparedFpv>,
        Res<render_anim::PreparedModelMaterials>,
        Option<Res<render_fx::PreparedFxModels>>,
    ),
) {
    let (fpv, model_materials, fx_models) = prepared;
    if scene.readiness.generation != job.spawn || job.spawn.0.is_none() {
        return;
    }
    // Pacing belongs to the load that is still running, not to the screen that
    // happens to be drawing it: a run without an overlay must spawn the world
    // the same way this one does.
    let progress = load
        .as_deref()
        .filter(|process| !process.is_complete())
        .map(|process| process.progress.clone());
    let paced = progress.is_some();
    if job.phase == WorldSpawnPhase::Gpu {
        if !paced {
            finish_world_spawn(&mut scene, &mut job, &mut commands);
            return;
        }
        let gap_ms = job.slice_gap_ms(std::time::Instant::now());
        let spawn = job.spawn;
        let gpu_ready = gpu.as_deref();
        let gpu_done = super::world_gpu::poll(&mut job.gpu_wait, spawn, gpu_ready, gap_ms);
        let fpv_done =
            fpv.settled_for(&tess.catalog()) && model_materials.settled_for(&tess.catalog());
        if gpu_done && !fpv_done {
            diag::info!(
                World,
                "world spawn: GPU ready; overlay holds for model and first-person preparation"
            );
        }
        if gpu_done && fpv_done {
            let quiet = job.gpu_wait.quiet();
            let elapsed_ms = job.gpu_wait.elapsed().as_secs_f32() * 1000.0;
            finish_world_spawn(&mut scene, &mut job, &mut commands);
            diag::info!(
                World,
                "world spawn: GPU ready quiet={} images={} pipelines={} elapsed={:.1}ms; overlay waits for admission",
                quiet,
                gpu_ready.map(|g| i32::from(g.images)).unwrap_or(-1),
                gpu_ready.map(|g| i32::from(g.pipelines)).unwrap_or(-1),
                elapsed_ms,
            );
            log_load_ledger(&job, gpu_ready, &fpv, elapsed_ms);
        }
        return;
    }
    if scene.spawned {
        record_first_world_frame(progress.as_ref(), &job, &present_ack, report.as_deref());
        return;
    }
    if scene.batches.is_empty() {
        let gfx_miss = report.as_ref().is_some_and(|r| {
            r.world_report
                .iter()
                .any(|line| line.contains("no GfxWorld reached"))
        });
        if paced && job.phase != WorldSpawnPhase::Done && gfx_miss {
            diag::warn!(
                World,
                "world spawn: empty batches — spawn_world returns without Camera3d (no GfxWorld or world_draw produced none)"
            );
            job.phase = WorldSpawnPhase::Done;
        }
        return;
    }
    if job.phase == WorldSpawnPhase::Done {
        return;
    }

    if paced && job.phase == WorldSpawnPhase::Unarmed {
        job.phase = WorldSpawnPhase::Yield;
        diag::info!(
            World,
            "world spawn: yield one frame so the overlay can tick after match install"
        );
        return;
    }
    if paced && job.phase == WorldSpawnPhase::Yield {
        job.phase = WorldSpawnPhase::Programs;
    }

    let frame_started = std::time::Instant::now();
    let deadline = paced.then(|| frame_started + SPAWN_FRAME_BUDGET);

    if job.needs_programs() {
        job.phase = WorldSpawnPhase::Programs;
    } else if !matches!(
        job.phase,
        WorldSpawnPhase::Admit | WorldSpawnPhase::Images | WorldSpawnPhase::Plan
    ) {
        return;
    }

    if job.phase == WorldSpawnPhase::Programs && !job.compile.armed {
        if load
            .as_deref()
            .is_some_and(|load| assets::minecraft_map::is_minecraft_load(&load.zone))
        {
            neutral_reflection_probes(&mut scene.reflection_probes);
        }
        job.images.arm(&mut scene, progress.as_ref());
    }
    let images_finished = if job.images.unfinished() {
        let gap_ms = job.slice_gap_ms(frame_started);
        let max_this_frame = if paced {
            super::world_images::IMAGES_PER_OVERLAY_FRAME
        } else {
            u32::MAX
        };
        let byte_budget = (super::world_images::IMAGE_BYTES_PER_OVERLAY_FRAME as f32
            * (gap_ms.max(16.7) / 16.7))
            .round() as u64;
        let byte_budget = byte_budget.min(super::world_images::MAX_IMAGE_BYTES_PER_SLICE);
        let bytes_before = job.images.handed_bytes;
        let finished = job.images.until(
            &mut images,
            &mut common_images,
            deadline,
            max_this_frame,
            byte_budget,
        );
        job.last_work_ms = frame_started.elapsed().as_secs_f32() * 1000.0;
        diag::info!(
            World,
            "world spawn slice: phase=images done={}/{} resident={} skipped={} reused_handles={}/{}B common_reused={}/{}B {:.1}ms gap={gap_ms:.1}ms handed_bytes={} total_handed={} byte_budget={} largest_step={:.1}ms/{}B (budget 40ms; bytes are handed to Assets<Image>, not copied to the GPU; a reused handle is a slot whose variant was already an asset)",
            job.images.done,
            job.images.total,
            job.images.resident,
            job.images.skipped,
            job.images.reused_handles,
            job.images.reused_handle_bytes,
            job.images.reused_common_handles,
            job.images.reused_common_bytes,
            job.last_work_ms,
            job.images.handed_bytes - bytes_before,
            job.images.handed_bytes,
            byte_budget,
            job.images.largest_step_ns as f64 / 1.0e6,
            job.images.largest_step_bytes,
        );
        finished
    } else {
        true
    };

    if job.phase == WorldSpawnPhase::Programs {
        if !job.compile.armed {
            job.compile
                .arm(&scene.runtime_material_catalog, progress.as_ref());
            job.admit = crate::assemble::drawsurf::MaterialProgramAdmit::default();
        }
        let compile_finished = job.compile.until(&scene.runtime_material_catalog, deadline);
        job.last_work_ms = frame_started.elapsed().as_secs_f32() * 1000.0;
        let gap_ms = job.slice_gap_ms(std::time::Instant::now());
        if compile_finished || job.last_work_ms >= 20.0 || job.compile.done() % 32 == 0 {
            let (wgsl_hit, wgsl_miss, wgsl_io_ms) = crate::assemble::drawsurf::wgsl_cache_stats();
            diag::info!(
                World,
                "world spawn slice: phase=programs {} {}/{} {:.1}ms gap={gap_ms:.1}ms wgsl_cache=hit:{wgsl_hit} miss:{wgsl_miss} io:{wgsl_io_ms:.0}ms (budget 40ms)",
                job.compile.last_label(),
                job.compile.done(),
                job.compile.total(),
                job.last_work_ms
            );
        }
        if !compile_finished {
            return;
        }
        // Both halves publish themselves as they end; anything still open at
        // the phase boundary ends here rather than being dropped unsaid.
        let (compiled, merged) = job.compile.take_stages();
        if let Some(stage) = compiled {
            stage.done();
        }
        if let Some(stage) = merged {
            stage.done();
        }
        job.phase = WorldSpawnPhase::Admit;
        if deadline.is_some_and(|end| std::time::Instant::now() >= end) {
            return;
        }
    }

    if job.phase == WorldSpawnPhase::Admit {
        let job = &mut *job;
        if !job.admit.armed() {
            job.admit.arm(&mut job.compile, progress.as_ref());
            scene.exact_world_refuse = top_census_cause(&job.compile.world_causes);
            scene.exact_world_cause2 = ranked_census_cause(&job.compile.world_causes, 1);
            scene.exact_packed_refuse = top_census_cause(&job.compile.packed_causes);
            scene.exact_pos_tex_refuse = top_census_cause(&job.compile.pos_tex_causes);
            scene.exact_ifc_n = Some(i64::from(
                census_ifc_n(&job.compile.world_causes)
                    .saturating_add(census_ifc_n(&job.compile.packed_causes)),
            ));
            scene.exact_opcode = top_unknown_opcode(&job.compile.world_causes)
                .or_else(|| top_unknown_opcode(&job.compile.packed_causes));
            job.admit.reserve_shaders(&shaders);
            job.last_work_ms = frame_started.elapsed().as_secs_f32() * 1000.0;
            diag::info!(
                World,
                "world spawn slice: phase=admit from_ports ports={} {:.1}ms (budget 40ms)",
                job.admit.port_count(),
                job.last_work_ms
            );
            if deadline.is_some_and(|end| std::time::Instant::now() >= end) {
                return;
            }
        }
        let (programs, exact_shaders) = job.admit.take_generation();
        let generation = crate::assemble::drawsurf::admit_material_generation(
            scene.runtime_material_catalog.clone(),
            programs,
            exact_shaders,
        );
        commands.insert_resource(
            render_scene::TessMaterials::new(
                std::sync::Arc::clone(&generation.catalog),
                std::sync::Arc::clone(&generation.prepared),
            )
            .expect("admitted material publication"),
        );
        commands.insert_resource(scene.map_xmodel_scene_assets.clone());
        match &generation.postfx {
            crate::assemble::drawsurf::RuntimePostFxResources::Ready(film) => diag::info!(
                World,
                "post-fx materials: READY passes={} tech=4 vertex_type=0",
                film.len(),
            ),
            crate::assemble::drawsurf::RuntimePostFxResources::Refused(cause) => {
                diag::warn!(World, "post-fx film material: RED cause={cause:?}")
            }
            crate::assemble::drawsurf::RuntimePostFxResources::Uninstalled => {
                diag::warn!(World, "post-fx film material: RED cause=Uninstalled")
            }
        }
        commands.insert_resource(generation);
        commands.insert_resource(crate::assemble::drawsurf::MaterialFrameInputs::default());
        match scene.exp_fog {
            Some(fog) => {
                diag::info!(
                    World,
                    "drawsurf createart fog: READY start={:.3} half={:.3} maxOpacity={:.3} sun={}",
                    fog.start_dist,
                    fog.halfway_dist,
                    fog.max_opacity,
                    fog.sun.is_some()
                );
                commands.insert_resource(crate::assemble::drawsurf::MapFrameFog::new(fog));
            }
            None => {
                commands.remove_resource::<crate::assemble::drawsurf::MapFrameFog>();
                diag::warn!(
                    World,
                    "drawsurf createart fog: missing setExpFog — fog code constants are fed as fog off"
                );
            }
        }
        match scene.dir_primary_light {
            Some(light) => {
                diag::info!(
                    World,
                    "drawsurf DIR primary light: READY type={} dir=({:.3},{:.3},{:.3}) color=({:.3},{:.3},{:.3})",
                    light.light_type,
                    light.direction[0],
                    light.direction[1],
                    light.direction[2],
                    light.color[0],
                    light.color[1],
                    light.color[2]
                );
                commands.insert_resource(light);
            }
            None => {
                commands.remove_resource::<crate::assemble::drawsurf::MapDirPrimaryLight>();
                diag::warn!(
                    World,
                    "drawsurf DIR primary light: RED no is_sun GFX_LIGHT_TYPE_DIR — code constants 0/1/2 stay unproduced"
                );
            }
        }
        match scene.t5_sun_parse_exposure {
            Some(exposure) => {
                diag::info!(
                    World,
                    "drawsurf T5 sunParse exposure: READY {exposure:.4} volumes={}",
                    scene.t5_exposure_volume_count
                );
                commands
                    .insert_resource(crate::assemble::drawsurf::MapT5SunParseExposure { exposure });
            }
            None => {
                commands.remove_resource::<crate::assemble::drawsurf::MapT5SunParseExposure>();
            }
        }
        match (
            scene.t5_tree_scatter_intensity,
            scene.t5_tree_scatter_amount,
        ) {
            (Some(intensity), Some(amount)) => {
                commands.insert_resource(crate::assemble::drawsurf::MapT5TreeScatter {
                    intensity,
                    amount,
                });
            }
            _ => {
                commands.remove_resource::<crate::assemble::drawsurf::MapT5TreeScatter>();
            }
        }
        commands.insert_resource(crate::assemble::drawsurf::MapOutdoor {
            image: scene
                .outdoor_image
                .map(|id| crate::assemble::drawsurf::RuntimeImageId(id)),
            lookup: scene.outdoor_lookup,
        });
        commands.insert_resource(crate::assemble::drawsurf::MapSunEffects {
            def: scene.sun_effects,
        });
        match scene.sun_effects {
            Some(sun) => diag::info!(
                World,
                "sun effects: READY sprite={} flare={} sprite_image={:?} flare_image={:?} blind={} glare={} dir=({:.3},{:.3},{:.3})",
                sun.sprite_material.map_or("-".into(), |id| id.to_string()),
                sun.flare_material.map_or("-".into(), |id| id.to_string()),
                sun.sprite_image,
                sun.flare_image,
                sun.blind_max_darken,
                sun.glare_max_lighten,
                sun.direction[0],
                sun.direction[1],
                sun.direction[2]
            ),
            None => diag::info!(World, "sun effects: off (no authored IW4 record)"),
        }
        match scene.outdoor_image {
            Some(id) => diag::info!(
                World,
                "drawsurf outdoor: READY image={id} lookup_m00={:.6e} lookup_m30={:.4}",
                f32::from_bits(scene.outdoor_lookup[0]),
                f32::from_bits(scene.outdoor_lookup[12])
            ),
            None => diag::warn!(
                World,
                "drawsurf outdoor: RED no $outdoor / GfxWorld outdoorImage — code texture 14 stays unproduced (lookup_m00={:.6e} lookup_m30={:.4})",
                f32::from_bits(scene.outdoor_lookup[0]),
                f32::from_bits(scene.outdoor_lookup[12])
            ),
        }
        commands.insert_resource(crate::assemble::drawsurf::MapPrimaryLightTypes {
            types: scene.primary_light_types.clone(),
        });
        commands.insert_resource(crate::assemble::drawsurf::MapPrimaryLights {
            lights: scene.primary_light_pack.clone(),
            attenuation: scene.primary_light_attenuation.clone(),
            overrides: scene.primary_light_overrides.clone(),
            reflection_probe_sh: scene.reflection_probe_sh.clone(),
            dynamic: scene.dynamic_light,
        });
        {
            let n = scene.primary_light_pack.len();
            let falloff_w = scene
                .primary_light_pack
                .iter()
                .filter(|light| light.falloff_image_width.is_some())
                .count();
            let atten = scene
                .primary_light_attenuation
                .iter()
                .filter(|bind| bind.image.is_some())
                .count();
            let t5_atten = scene
                .primary_light_overrides
                .iter()
                .filter(|pack| pack.attenuation.is_some())
                .count();
            diag::info!(
                World,
                "drawsurf primary lights: n={n} falloff_width={falloff_w} atten_image={atten} t5_atten={t5_atten} (tex 0xD only when atten_image is Some)"
            );
            let width_none: Vec<String> = scene
                .primary_light_pack
                .iter()
                .enumerate()
                .filter(|(_, light)| light.falloff_image_width.is_none())
                .map(|(index, light)| {
                    let def = scene
                        .primary_light_def_names
                        .get(index)
                        .and_then(|name| name.as_deref())
                        .unwrap_or("-");
                    format!("{index}:t{}:{def}", light.light_type)
                })
                .collect();
            let atten_none: Vec<String> = scene
                .primary_light_attenuation
                .iter()
                .enumerate()
                .filter(|(_, bind)| bind.image.is_none())
                .map(|(index, _)| {
                    let light_type = scene
                        .primary_light_pack
                        .get(index)
                        .map(|light| light.light_type)
                        .unwrap_or(0);
                    let def = scene
                        .primary_light_def_names
                        .get(index)
                        .and_then(|name| name.as_deref())
                        .unwrap_or("-");
                    format!("{index}:t{light_type}:{def}")
                })
                .collect();
            diag::info!(
                World,
                "drawsurf primary lights missing falloff_width: {} (the def image width always yields 5; None is a retention gap)",
                if width_none.is_empty() {
                    "none".to_owned()
                } else {
                    width_none.join(",")
                }
            );
            diag::info!(
                World,
                "drawsurf primary lights missing atten_image: {}",
                if atten_none.is_empty() {
                    "none".to_owned()
                } else {
                    atten_none.join(",")
                }
            );
        }
        {
            let surf_lights = scene
                .cull
                .as_ref()
                .map(|cull| cull.surface_primary_lights.as_slice())
                .unwrap_or(&[]);
            let surf_nz = surf_lights.iter().filter(|&&i| i != 0).count();
            let smodel_n = scene.static_model_instances.len();
            let smodel_nz = scene
                .static_model_instances
                .iter()
                .flatten()
                .filter(|inst| inst.primary_light_index != 0)
                .count();
            diag::info!(
                World,
                "scene light index sources: surface nz={}/{} static model nz={}/{}",
                surf_nz,
                surf_lights.len(),
                smodel_nz,
                smodel_n
            );
            let world_probes = hist_u8(scene.batches.iter().map(|b| b.reflection_probe_index));
            let smodel_probes = hist_u8(
                scene
                    .static_model_instances
                    .iter()
                    .flatten()
                    .map(|inst| inst.reflection_probe_index),
            );
            diag::info!(
                World,
                "reflectionProbeIndex sources: world_batches {} smodel_drawinst {} cell_lists={}/{} origins={} (XModel/FPV/script Colour packs lightingInfo.hi from Assigned)",
                world_probes,
                smodel_probes,
                scene
                    .cull
                    .as_ref()
                    .map(|cull| cull.dpvs.cell_reflection_probes.len())
                    .unwrap_or(0),
                scene
                    .cull
                    .as_ref()
                    .map(|cull| cull.dpvs.cell_count)
                    .unwrap_or(0),
                scene.reflection_probe_origins.len(),
            );
        }
        commands.insert_resource(crate::assemble::drawsurf::DrawMethodDfog(false));
        if let Some(stage) = job.admit.take_stage() {
            stage.done();
        }
        job.phase = WorldSpawnPhase::Images;
        job.last_work_ms = frame_started.elapsed().as_secs_f32() * 1000.0;
        diag::info!(
            World,
            "world spawn slice: phase=admit install {:.1}ms (budget 40ms)",
            job.last_work_ms
        );
        if deadline.is_some_and(|end| std::time::Instant::now() >= end) {
            return;
        }
    }

    if job.phase == WorldSpawnPhase::Images {
        if !images_finished {
            return;
        }
        job.phase = WorldSpawnPhase::Plan;
        if deadline.is_some_and(|end| std::time::Instant::now() >= end) {
            return;
        }
    }

    if job.phase == WorldSpawnPhase::Plan {
        super::world_plan::install(
            &mut commands,
            &scene,
            &images,
            job.images.exact_handles().to_vec(),
            job.images.probe_handles().to_vec(),
            job.images.lightmap_handles().to_vec(),
            tracers.as_deref(),
            fx_catalog.as_deref(),
        );
        let fx_geometry =
            super::world_plan::prepare_fx_model_geometry(&scene, fx_models.as_deref());
        commands.insert_resource(fx_geometry.0.clone());
        commands.insert_resource(fx_geometry);
        tess.material_images = std::sync::Arc::new(job.images.exact_handles().to_vec());
        if let Some(host) = fx_host.as_mut() {
            match scene.fx_glass.as_ref() {
                Some(glass) => host.0.glass.reset(fx::FxGlassInitTables {
                    piece_limit: glass.piece_limit as u32,
                    geo_data_limit: glass.geo_data_limit as u32,
                    init_states: &glass.init_piece_states,
                    init_geo: &glass.init_geo_data,
                    defs: &glass.defs,
                }),
                None => host.0.glass = fx::FxGlassSystemHost::default(),
            }
        }

        job.phase = WorldSpawnPhase::WorldTess;
    }
}

pub(crate) fn spawn_world_finish(
    mut commands: Commands,
    load: Option<Res<assets::MapLoadProcess>>,
    mut scene: ResMut<WorldScene>,
    mut images: ResMut<Assets<Image>>,
    mut job: ResMut<WorldSpawnJob>,
    mut tess: ResMut<render_scene::TessMaterials>,
) {
    if scene.readiness.generation != job.spawn
        || job.spawn.0.is_none()
        || job.phase != WorldSpawnPhase::WorldTess
    {
        return;
    }
    let progress = load
        .as_deref()
        .filter(|process| !process.is_complete())
        .map(|process| process.progress.clone());
    let paced = progress.is_some();
    let frame_started = std::time::Instant::now();
    super::world_occupancy::place(
        &mut commands,
        &mut scene,
        &mut images,
        job.images.exact_handles().to_vec(),
        job.images.probe_handles().to_vec(),
        job.images.lightmap_handles().to_vec(),
    );
    tess.material_images = std::sync::Arc::new(job.images.exact_handles().to_vec());
    job.last_work_ms = frame_started.elapsed().as_secs_f32() * 1000.0;
    if paced {
        job.phase = WorldSpawnPhase::Gpu;
        job.gpu_wait.arm(progress.as_ref());
        diag::info!(
            World,
            "world spawn: Camera3d up; overlay holds for GPU images/pipelines last_slice={:.1}ms images={}/{}",
            job.last_work_ms,
            job.images.done,
            job.images.total
        );
        return;
    }
    finish_world_spawn(&mut scene, &mut job, &mut commands);
    diag::info!(
        World,
        "world spawn: complete last_slice={:.1}ms images={}/{} (no overlay)",
        job.last_work_ms,
        job.images.done,
        job.images.total
    );
}

fn record_first_world_frame(
    progress: Option<&asset_transport::LoadProgress>,
    job: &WorldSpawnJob,
    present_ack: &WorldPresentAck,
    report: Option<&LaunchReport>,
) {
    let Some(progress) = progress else {
        return;
    };
    let Some(presented_at) = present_ack.presented_at(job.spawn) else {
        return;
    };
    if !progress.record_first_frame(presented_at) {
        return;
    }
    let zone = report
        .map(|report| report.zone.as_str())
        .unwrap_or("<unknown>");
    let image = progress
        .zone_image_bytes()
        .map_or_else(|| "NULL".to_owned(), |bytes| bytes.to_string());
    let first_frame = progress
        .first_frame_ms()
        .map_or_else(|| "NULL".to_owned(), |ms| format!("{ms:.1}"));
    let rss_peak = progress
        .load_rss_peak_bytes()
        .map_or_else(|| "NULL".to_owned(), |bytes| bytes.to_string());
    let rss_at_open = progress
        .rss_at_open_bytes()
        .map_or_else(|| "NULL".to_owned(), |bytes| bytes.to_string());
    diag::info!(
        World,
        "load measurement: zone={zone} zone_image_bytes={image} first_frame_ms={first_frame} rss_peak_bytes={rss_peak} rss_at_open_bytes={rss_at_open}"
    );
}

#[derive(Resource, Clone, Default)]
pub(crate) struct WorldPresentAck(Arc<Mutex<Option<(WorldGeneration, std::time::Instant)>>>);

impl WorldPresentAck {
    fn presented_at(&self, generation: WorldGeneration) -> Option<std::time::Instant> {
        self.0.lock().ok().and_then(|ack| {
            ack.as_ref()
                .filter(|(seen, _)| *seen == generation)
                .map(|(_, at)| *at)
        })
    }
}

#[derive(Resource, Default)]
struct ExtractedWorldPresent(Option<WorldGeneration>);

fn log_load_ledger(
    job: &WorldSpawnJob,
    gpu: Option<&WorldGpuReady>,
    fpv: &render_anim::PreparedFpv,
    gpu_wait_ms: f32,
) {
    let images = &job.images;
    let pipelines_added = gpu.and_then(|gpu| job.gpu_wait.pipelines_added(gpu.pipeline_n));
    let fpv_census = fpv.table().map(|table| table.census());
    diag::info!(
        World,
        "load ledger: images handed={} handed_bytes={} reused_slot={} reused_slot_bytes={} reused_common={} reused_common_bytes={} resident_map={} | pipelines cached={} compiled_this_load={} | fpv models={} rigs={} materials={} built_ms={} | gpu_wait_ms={gpu_wait_ms:.1}",
        images
            .done
            .saturating_sub(images.skipped)
            .saturating_sub(images.reused_handles),
        images.handed_bytes,
        images.reused_handles,
        images.reused_handle_bytes,
        images.reused_common_handles,
        images.reused_common_bytes,
        images.resident,
        gpu.map_or_else(|| "?".to_owned(), |gpu| gpu.pipeline_n.to_string()),
        pipelines_added.map_or_else(|| "?".to_owned(), |n| n.to_string()),
        fpv_census.map_or_else(|| "?".to_owned(), |c| c.models.to_string()),
        fpv_census.map_or_else(|| "?".to_owned(), |c| c.rigs.to_string()),
        fpv_census.map_or_else(|| "?".to_owned(), |c| c.materials.to_string()),
        fpv_census.map_or_else(|| "?".to_owned(), |c| format!("{:.1}", c.elapsed_ms)),
    );
}

fn finish_world_spawn(scene: &mut WorldScene, job: &mut WorldSpawnJob, commands: &mut Commands) {
    if scene.readiness.generation != job.spawn || job.spawn.0.is_none() {
        return;
    }
    job.phase = WorldSpawnPhase::Done;
    scene.spawned = true;
    scene.readiness.state = frame::ReadinessState::Ready;
    commands.insert_resource(render_scene::WorldPresentFacts { spawned: true });
    perf::world_ready(1);
}

pub(crate) fn despawn_world_entities_on_teardown(
    mut torn: MessageReader<MatchTornDown>,
    smodels: Query<Entity, With<StaticModelEntity>>,
    scripts: Query<Entity, With<ScriptModelEntity>>,
    dynents: Query<Entity, With<DynEntModelEntity>>,
    mut commands: Commands,
) {
    if torn.read().count() == 0 {
        return;
    }
    for entity in smodels.iter().chain(scripts.iter()).chain(dynents.iter()) {
        commands.entity(entity).despawn();
    }
}

pub(crate) fn reset_world_spawn_on_teardown(
    mut torn: MessageReader<MatchTornDown>,
    mut job: ResMut<WorldSpawnJob>,
    mut gpu: ResMut<WorldGpuReady>,
    mut demand: ResMut<super::world_gpu::PipelineDemandTracker>,
    mut image_handles: Option<ResMut<crate::assemble::drawsurf::RuntimeImageHandles>>,
    mut retiring: ResMut<frame::Retiring>,
    images: Res<Assets<Image>>,
    mut commands: Commands,
) {
    if torn.read().count() == 0 {
        return;
    }
    let live_before = images.len();
    retiring.hand_over(std::mem::take(&mut *job));
    *gpu = WorldGpuReady::default();
    *demand = super::world_gpu::PipelineDemandTracker::default();
    if let Some(handles) = image_handles.as_mut() {
        **handles = crate::assemble::drawsurf::RuntimeImageHandles::default();
    }
    diag::info!(
        World,
        "world: render side let go of its images ({live_before} live in Assets<Image> at the \
         moment of release; what survives is what another owner still holds)"
    );
    commands.insert_resource(crate::assemble::drawsurf::MapSunEffects::default());
    commands.queue(|world: &mut World| {
        frame::retire::retire_resources(world, |batch| {
            batch
                .resource::<crate::assemble::drawsurf::WorldDrawGpuPlan>()
                .resource::<crate::assemble::drawsurf::SmodelGpuPlan>()
                .resource::<crate::assemble::drawsurf::tess::sky::SkyModelDrawPlan>()
                .resource::<crate::prepare::scene::smodel_lighting::WorldSmodelLighting>()
                .resource::<crate::prepare::scene::smodel_geom_cache::WorldStaticModelCache>()
                .resource::<crate::prepare::scene::model_lighting_atlas::WorldModelLightingAtlas>()
                .resource::<crate::prepare::scene::model_lighting_cache::WorldModelLightingCache>();
        });
    });
}

pub(crate) fn shutdown_world_on_teardown(
    mut torn: MessageReader<MatchTornDown>,
    mut retiring: ResMut<frame::Retiring>,
    mut scene: Option<ResMut<WorldScene>>,
    mut membership: Option<ResMut<DynEntCellBits>>,
    mut present: ResMut<render_scene::WorldPresentFacts>,
    mut tess: ResMut<render_scene::TessMaterials>,
    mut lookup: ResMut<render_scene::DynAtPointLookup>,
    mut cells: ResMut<render_scene::WorldDpvsCells>,
) {
    if torn.read().count() == 0 {
        return;
    }
    if let Some(scene) = scene.as_mut() {
        retiring.hand_over(std::mem::take(&mut **scene));
    }
    if let Some(membership) = membership.as_mut() {
        **membership = DynEntCellBits::default();
    }
    *present = render_scene::WorldPresentFacts::default();
    *tess = render_scene::TessMaterials::default();
    lookup.clear();
    *cells = render_scene::WorldDpvsCells::default();
    let spawned = scene.as_ref().map(|s| i64::from(s.spawned)).unwrap_or(0);
    perf::world_hold(spawned, 0, 0);
    diag::info!(
        World,
        "world: shutdown — leftover geometry, tess plans, and model lighting dropped"
    );
}

pub(crate) fn despawn_fly_cameras_on_teardown(
    mut torn: MessageReader<MatchTornDown>,
    cameras: Query<Entity, With<FlyCamera>>,
    mut commands: Commands,
) {
    if torn.read().count() == 0 {
        return;
    }
    for entity in &cameras {
        commands.entity(entity).despawn();
    }
}

pub(crate) fn arm_world_spawn_on_install(
    mut installed: MessageReader<MatchInstalled>,
    mut job: ResMut<WorldSpawnJob>,
    mut gpu: ResMut<WorldGpuReady>,
) {
    let Some(install) = installed.read().last().cloned() else {
        return;
    };
    let spawn = WorldGeneration::from_install(install.request_id);
    *job = WorldSpawnJob {
        spawn,
        ..Default::default()
    };
    *gpu = WorldGpuReady {
        spawn,
        ..Default::default()
    };
}

pub(crate) fn register_world_gpu_ready(app: &mut App) {
    super::world_gpu::register_resources(app);
    app.init_resource::<super::world_images::ResidentGpuImages>();
    app.add_systems(Update, supply_requested_shaders.before(spawn_world));
    let present_ack = WorldPresentAck::default();
    app.insert_resource(present_ack.clone());
    let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) else {
        return;
    };
    render_app
        .insert_resource(present_ack)
        .init_resource::<ExtractedWorldPresent>();
    render_app.add_systems(
        bevy::render::ExtractSchedule,
        (
            super::world_gpu::extract_images_ready,
            extract_world_present,
        ),
    );
    render_app.add_systems(
        bevy::render::Render,
        acknowledge_world_present
            .after(bevy::render::RenderSystems::Render)
            .before(bevy::render::RenderSystems::Cleanup),
    );
}

fn extract_world_present(
    main_world: Res<bevy::render::MainWorld>,
    mut extracted: ResMut<ExtractedWorldPresent>,
) {
    let generation = main_world
        .get_resource::<WorldGeneration>()
        .copied()
        .unwrap_or(WorldGeneration(None));
    let spawned = main_world
        .get_resource::<WorldScene>()
        .is_some_and(|scene| scene.spawned && scene.readiness.ready_for(generation));
    extracted.0 = (spawned && generation.0.is_some()).then_some(generation);
}

fn acknowledge_world_present(extracted: Res<ExtractedWorldPresent>, ack: Res<WorldPresentAck>) {
    let Some(generation) = extracted.0 else {
        return;
    };
    if let Ok(mut slot) = ack.0.lock() {
        *slot = Some((generation, std::time::Instant::now()));
    }
}

fn supply_requested_shaders(
    mut job: ResMut<WorldSpawnJob>,
    mut shaders: ResMut<Assets<bevy::shader::Shader>>,
) {
    job.admit.supply(&mut shaders);
}
