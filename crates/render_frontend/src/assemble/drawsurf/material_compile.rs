use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use bevy::tasks::{AsyncComputeTaskPool, Task, TaskPool, futures_lite::future};

use super::postfx_plan::{POSTFX_TECH_TYPE, POSTFX_VERTEX_TYPE, postfx_compile_location};
use super::sun_shadow::SUN_SHADOW_CASTER_TECH;
use super::{
    PortId, RuntimeMaterialCatalog, RuntimeProgramPort, RuntimeProgramRegistry, RuntimeTechnique,
    TechType,
};

pub(crate) const EMISSIVE_TECH_TYPE: u8 = 5;

fn technique_compile_vertex_types(
    tech: u8,
    technique: &RuntimeTechnique,
    world_vert_format: u8,
) -> ([u8; 5], usize) {
    let types = asset_iw4::vertex_decl::emissive_tess_compile_vertex_types(
        technique.flags,
        world_vert_format,
    );
    let used = if tech == EMISSIVE_TECH_TYPE {
        types.len()
    } else {
        types.len() - 1
    };
    (types, used)
}

fn compile_one_job(catalog: &RuntimeMaterialCatalog, job: CompileJob) -> PassOutcome {
    let Some(pass) =
        RuntimeProgramRegistry::catalog_pass(catalog, TechType(job.tech), job.set_i, job.pass_i)
    else {
        return PassOutcome::Missing;
    };
    match RuntimeProgramPort::compile(catalog, pass, job.vertex_type) {
        Ok(port) => PassOutcome::Port {
            tech: job.tech,
            port,
        },
        Err(cause) => PassOutcome::Refused {
            tech: job.tech,
            vertex_type: job.vertex_type,
            key: cause.census_key(),
        },
    }
}

/// Compile every job, in chunks, on the load pool.
///
/// The pool's workers already exist and hold the process affinity; a thread
/// spawned here would inherit the coordinator's narrower one instead.
///
/// The results keep the order of `jobs` whatever order the chunks finish in:
/// `TaskPool::scope` returns one `Vec` per task in spawn order, and every task
/// here is spawned directly from this closure.
///
/// The cooperative policy is explicit and off: the calling thread waits for the
/// chunks instead of ticking the pool's queue, so no unrelated load work is
/// pulled onto the narrow-affinity thread or onto the main thread mid-spawn.
fn compile_jobs_parallel(
    catalog: Arc<RuntimeMaterialCatalog>,
    jobs: Vec<CompileJob>,
    progress: Arc<AtomicU32>,
) -> Vec<PassOutcome> {
    if jobs.is_empty() {
        return Vec::new();
    }
    let mut unique = Vec::<(CompileJob, u32)>::new();
    let mut indices = Vec::with_capacity(jobs.len());
    let mut by_pass = HashMap::new();
    for &job in &jobs {
        let pass = RuntimeProgramRegistry::catalog_pass(
            &catalog,
            TechType(job.tech),
            job.set_i,
            job.pass_i,
        );
        let index = if let Some(pass) = pass {
            let key = (
                pass.shader_pair,
                pass.custom_sampler_flags,
                pass.t5_custom_sampler_flags,
                pass.color_space,
                pass.hardware_shadow_compare,
                pass.arguments.as_slice(),
                job.vertex_type,
            );
            *by_pass.entry(key).or_insert_with(|| {
                unique.push((job, 0));
                unique.len() - 1
            })
        } else {
            unique.push((job, 0));
            unique.len() - 1
        };
        unique[index].1 += 1;
        indices.push(index);
    }
    let nthreads = assets::load_workers().clamp(1, unique.len());
    let chunk_len = unique.len().div_ceil(nthreads * 16);

    let outcomes: Vec<_> = assets::load_pool()
        .scope_with_executor(false, None, |scope| {
            for chunk in unique.chunks(chunk_len) {
                let catalog = Arc::clone(&catalog);
                let progress = Arc::clone(&progress);
                scope.spawn(async move {
                    let mut out = Vec::with_capacity(chunk.len());
                    for &(job, count) in chunk {
                        out.push(compile_one_job(&catalog, job));
                        progress.fetch_add(count, Ordering::Relaxed);
                    }
                    out
                });
            }
        })
        .into_iter()
        .flatten()
        .collect();
    jobs.iter()
        .zip(indices)
        .map(|(job, index)| match outcomes[index].clone() {
            PassOutcome::Port { port, .. } => PassOutcome::Port {
                tech: job.tech,
                port,
            },
            PassOutcome::Refused {
                vertex_type, key, ..
            } => PassOutcome::Refused {
                tech: job.tech,
                vertex_type,
                key,
            },
            PassOutcome::Missing => PassOutcome::Missing,
        })
        .collect()
}

fn log_catalog_generation(catalog: &RuntimeMaterialCatalog) {
    let baked_materials = catalog
        .parts()
        .materials
        .iter()
        .filter(|material| material.baked_draw_surf.is_some())
        .count();
    match &catalog.parts().sorted_materials {
        super::RuntimeSortedMaterialTable::Ready {
            asset_ids_by_ordinal,
            skipped_n,
            slot_gap_n,
            ..
        } => diag::info!(
            World,
            "drawsurf material generation: READY ordinals={} skipped={skipped_n} slot_gap={slot_gap_n} baked={baked_materials}",
            asset_ids_by_ordinal.len()
        ),
        super::RuntimeSortedMaterialTable::BuildFailed(cause) => {
            diag::warn!(
                World,
                "drawsurf material generation: RED build failed: {cause:?}; baked={baked_materials}"
            )
        }
        super::RuntimeSortedMaterialTable::Missing => {
            diag::warn!(
                World,
                "drawsurf material generation: RED no sorted-material writer; baked={baked_materials}"
            )
        }
    }
    let state_entry_n = catalog
        .parts()
        .materials
        .iter()
        .filter(|material| material.state_bits_entry.is_some())
        .count();
    match catalog.parts().iw5_remap.as_deref() {
        Some(leftover) => diag::info!(
            World,
            "iw5 stateBitsEntry remap: state_entry_n={state_entry_n} {leftover}"
        ),
        None => diag::info!(
            World,
            "iw5 stateBitsEntry remap: none (IW4/T5) state_entry_n={state_entry_n}"
        ),
    }
    match catalog.parts().t5_remap.as_deref() {
        Some(leftover) => diag::info!(
            World,
            "t5 stateBitsEntry remap: state_entry_n={state_entry_n} {leftover}"
        ),
        None => diag::info!(
            World,
            "t5 stateBitsEntry remap: none (IW4/IW5) state_entry_n={state_entry_n}"
        ),
    }
    let coverage = catalog.opcode_surface_coverage();
    diag::info!(World, "drawsurf {}", coverage.summary_line());
    {
        let mut sets = 0u32;
        let mut with_tex_d = 0u32;
        let mut hist = std::collections::BTreeMap::<u32, u32>::new();
        for set in &catalog.parts().technique_sets {
            let Some(technique) = set.technique(TechType(0x0F)) else {
                continue;
            };
            sets = sets.saturating_add(1);
            let mut has_d = false;
            for pass in &technique.passes {
                for argument in &pass.arguments {
                    if let super::RuntimeArgumentBinding::CodeTexture { index, .. } = argument {
                        *hist.entry(*index).or_default() += 1;
                        if *index == 0xD {
                            has_d = true;
                        }
                    }
                }
            }
            if has_d {
                with_tex_d = with_tex_d.saturating_add(1);
            }
        }
        diag::info!(
            World,
            "drawsurf tech15 CodeTexture: sets={sets} with_attenuation={with_tex_d} hist={hist:?} (type-4 payload)"
        );
    }
}

#[derive(Clone, Copy)]
struct CompileJob {
    tech: u8,
    vertex_type: u8,
    set_i: usize,
    pass_i: usize,
}

#[derive(Clone)]
enum PassOutcome {
    Missing,
    Port {
        tech: u8,
        port: RuntimeProgramPort,
    },
    Refused {
        tech: u8,
        vertex_type: u8,
        key: String,
    },
}

#[derive(Default)]
pub struct MaterialProgramCompile {
    pub(crate) armed: bool,
    jobs: Vec<CompileJob>,
    task: Option<Task<Vec<PassOutcome>>>,
    progress: Option<Arc<AtomicU32>>,
    started: Option<std::time::Instant>,
    load: Option<asset_transport::LoadProgress>,
    /// What the pool workers have computed. It ends when the last job is
    /// computed, which is not when the result is published.
    compile_stage: Option<asset_transport::StageHandle>,
    /// What the main thread has absorbed out of those outcomes. Until a slice
    /// takes an outcome, nothing has been published from it.
    merge_stage: Option<asset_transport::StageHandle>,
    done: u32,
    total: u32,
    catalog: Option<Arc<RuntimeMaterialCatalog>>,
    port_by_id: HashMap<PortId, usize>,
    pending: Vec<PassOutcome>,
    absorb_at: usize,
    pub(crate) ports: Vec<RuntimeProgramPort>,
    pub(crate) postfx_ports: Vec<PortId>,
    pub(crate) world_causes: BTreeMap<String, u32>,
    pub(crate) packed_causes: BTreeMap<String, u32>,
    pub(crate) pos_tex_causes: BTreeMap<String, u32>,
    pub(crate) postfx_causes: BTreeMap<String, u32>,
    pub(crate) refused_world: usize,
    pub(crate) refused_packed: usize,
    pub(crate) refused_pos_tex: usize,
    pub(crate) refused_postfx: usize,
}

impl MaterialProgramCompile {
    pub fn done(&self) -> u32 {
        if !self.pending.is_empty() {
            return u32::try_from(self.absorb_at).unwrap_or(u32::MAX);
        }
        self.done
    }

    pub fn last_label(&self) -> String {
        if self.task.is_some() {
            format!("pool workers={}", assets::load_workers())
        } else if !self.pending.is_empty() {
            format!("absorb {}/{}", self.absorb_at, self.pending.len())
        } else {
            "done".to_owned()
        }
    }

    pub fn total(&self) -> u32 {
        self.total
    }

    /// Hand both stages back, so the caller ends them where the work is really
    /// over instead of leaving them to a drop.
    pub fn take_stages(
        &mut self,
    ) -> (
        Option<asset_transport::StageHandle>,
        Option<asset_transport::StageHandle>,
    ) {
        (self.compile_stage.take(), self.merge_stage.take())
    }

    pub(crate) fn take_ports(&mut self) -> Vec<RuntimeProgramPort> {
        std::mem::take(&mut self.ports)
    }

    pub(crate) fn is_postfx(&self, id: PortId) -> bool {
        self.postfx_ports.contains(&id)
    }

    fn absorb_one(&mut self, outcome: PassOutcome) {
        match outcome {
            PassOutcome::Missing => {}
            PassOutcome::Port { tech, port } => {
                if tech == POSTFX_TECH_TYPE && !self.postfx_ports.contains(&port.id()) {
                    self.postfx_ports.push(port.id());
                }
                match self.port_by_id.get(&port.id()) {
                    Some(&idx) if self.ports[idx].same_admission_identity(&port) => {}
                    Some(_) => {
                        self.ports.push(port);
                    }
                    None => {
                        self.port_by_id.insert(port.id(), self.ports.len());
                        self.ports.push(port);
                    }
                }
            }
            PassOutcome::Refused {
                tech,
                vertex_type,
                key,
            } => {
                if tech == POSTFX_TECH_TYPE {
                    *self.postfx_causes.entry(key).or_default() += 1;
                    self.refused_postfx += 1;
                } else if vertex_type == asset_iw4::vertex_decl::PACKED_VERTEX_TYPE {
                    *self.packed_causes.entry(key).or_default() += 1;
                    self.refused_packed += 1;
                } else if vertex_type == asset_iw4::vertex_decl::POS_TEX_VERTEX_TYPE {
                    *self.pos_tex_causes.entry(key).or_default() += 1;
                    self.refused_pos_tex += 1;
                } else {
                    *self.world_causes.entry(key).or_default() += 1;
                    self.refused_world += 1;
                }
            }
        }
    }

    fn absorb_pending(&mut self, deadline: Option<std::time::Instant>) -> bool {
        const CHUNK: usize = 64;
        let slice_started = std::time::Instant::now();
        while self.absorb_at < self.pending.len() {
            if deadline.is_some_and(|end| std::time::Instant::now() >= end) {
                diag::info!(
                    World,
                    "world spawn compile absorb: {}/{} {:.1}ms (budget)",
                    self.absorb_at,
                    self.pending.len(),
                    slice_started.elapsed().as_secs_f32() * 1000.0
                );
                // One publish per main-thread slice, at the boundary the
                // budget already draws.
                if let Some(stage) = &self.merge_stage {
                    stage.set_completed(self.absorb_at as u64);
                }
                return false;
            }
            let end = (self.absorb_at + CHUNK).min(self.pending.len());
            while self.absorb_at < end {
                let outcome =
                    std::mem::replace(&mut self.pending[self.absorb_at], PassOutcome::Missing);
                self.absorb_at += 1;
                self.absorb_one(outcome);
            }
        }
        diag::info!(
            World,
            "world spawn compile absorb: finished {} ports {:.1}ms",
            self.ports.len(),
            slice_started.elapsed().as_secs_f32() * 1000.0
        );
        let merged = self.pending.len() as u64;
        self.pending.clear();
        self.absorb_at = 0;
        self.done = self.total;
        if let Some(stage) = self.merge_stage.take() {
            stage.set_completed(merged);
            stage.done();
        }
        self.progress = None;
        self.task = None;
        true
    }

    /// Compiled outcomes arrive as one block; publishing them is its own pass
    /// over that block, with its own count.
    fn take_outcomes(&mut self, outcomes: Vec<PassOutcome>) {
        self.finish_compile();
        if let Some(load) = &self.load {
            self.merge_stage = Some(load.begin(
                asset_transport::StageId::ProgramMerge,
                Some(outcomes.len() as u64),
            ));
        }
        self.pending = outcomes;
        self.absorb_at = 0;
    }

    fn finish_compile(&mut self) {
        if let Some(stage) = self.compile_stage.take() {
            stage.set_completed(u64::from(self.total));
            stage.done();
        }
    }

    fn sync_progress(&mut self) {
        if let Some(progress) = &self.progress {
            self.done = progress.load(Ordering::Relaxed);
        }
        if let Some(stage) = &self.compile_stage {
            stage.set_completed(u64::from(self.done));
        }
    }

    pub fn until(
        &mut self,
        catalog: &Arc<RuntimeMaterialCatalog>,
        deadline: Option<std::time::Instant>,
    ) -> bool {
        let paced = deadline.is_some();
        if self.task.is_none() && self.pending.is_empty() && self.done >= self.total && self.armed {
            return true;
        }
        if !self.pending.is_empty() {
            return self.absorb_pending(deadline);
        }
        if self.task.is_none() {
            let jobs = std::mem::take(&mut self.jobs);
            if jobs.is_empty() {
                self.done = self.total;
                self.finish_compile();
                if let Some(load) = &self.load {
                    load.record_skipped(asset_transport::StageId::ProgramMerge);
                }
                return true;
            }
            let catalog = Arc::clone(self.catalog.as_ref().unwrap_or(catalog));
            let progress = Arc::new(AtomicU32::new(0));
            self.progress = Some(Arc::clone(&progress));
            self.started = Some(std::time::Instant::now());
            diag::info!(
                World,
                "world spawn compile pool: jobs={} workers={}",
                self.total,
                assets::load_workers()
            );
            if paced {
                let task = AsyncComputeTaskPool::get_or_init(TaskPool::default).spawn(async move {
                    future::yield_now().await;
                    compile_jobs_parallel(catalog, jobs, progress)
                });
                self.task = Some(task);
            } else {
                let outcomes = compile_jobs_parallel(catalog, jobs, progress);
                let wall_ms = self
                    .started
                    .map(|t| t.elapsed().as_secs_f32() * 1000.0)
                    .unwrap_or(0.0);
                self.take_outcomes(outcomes);
                let _ = self.absorb_pending(None);
                diag::info!(
                    World,
                    "world spawn compile pool: finished jobs={} ports={} {:.1}ms",
                    self.total,
                    self.ports.len(),
                    wall_ms
                );
                return true;
            }
        }
        self.sync_progress();
        let ready = {
            let Some(task) = self.task.as_mut() else {
                return true;
            };
            future::block_on(future::poll_once(task))
        };
        let Some(outcomes) = ready else {
            return false;
        };
        let wall_ms = self
            .started
            .map(|t| t.elapsed().as_secs_f32() * 1000.0)
            .unwrap_or(0.0);
        self.take_outcomes(outcomes);
        self.task = None;
        let absorbed = self.absorb_pending(deadline);
        if absorbed {
            diag::info!(
                World,
                "world spawn compile pool: finished jobs={} ports={} {:.1}ms",
                self.total,
                self.ports.len(),
                wall_ms
            );
        }
        absorbed
    }

    pub fn arm(
        &mut self,
        catalog: &Arc<RuntimeMaterialCatalog>,
        progress: Option<&asset_transport::LoadProgress>,
    ) {
        self.jobs.clear();
        self.catalog = Some(Arc::clone(catalog));
        log_catalog_generation(catalog);
        if let Some(stage) = self.merge_stage.take() {
            stage.cancel();
        }
        if let Some(stage) = self.compile_stage.take() {
            stage.cancel();
        }
        self.load = progress.cloned();
        if let Some(progress) = progress {
            self.compile_stage = Some(progress.begin(asset_transport::StageId::Programs, None));
        }
        let mut techs = Vec::new();
        techs.extend(lighting_iw4::LIT_TECH_NO_SHADOW_DIR_SLOTS);
        techs.extend(lighting_iw4::LIT_TECH_NO_SHADOW_LOCAL_SLOTS);
        techs.extend(lighting_iw4::LIT_TECH_SHADOW_DIR_SLOTS);
        techs.extend(lighting_iw4::LIT_TECH_SHADOW_SPOT_SLOTS);
        techs.extend([
            lighting_iw4::TECHNIQUE_LIGHT_SPOT,
            lighting_iw4::TECHNIQUE_LIGHT_OMNI,
            lighting_iw4::TECHNIQUE_LIGHT_SPOT_SHADOW,
        ]);
        techs.push(SUN_SHADOW_CASTER_TECH);
        techs.push(EMISSIVE_TECH_TYPE);
        let reachable: HashSet<usize> = catalog
            .parts()
            .materials
            .iter()
            .filter(|material| material.baked_draw_surf.is_some())
            .map(|material| material.local_technique_set.0 as usize)
            .collect();
        let only_sets = (!reachable.is_empty()).then_some(&reachable);
        self.total = 0;
        let mut type_hist = std::collections::BTreeMap::<u8, u32>::new();
        for tech in techs {
            let tech_type = TechType(tech);
            let variants = RuntimeProgramRegistry::catalog_unique_pass_indices_in(
                catalog, tech_type, only_sets,
            );
            for &(set_i, pass_i) in &variants {
                let Some(set) = catalog.parts().technique_sets.get(set_i) else {
                    continue;
                };
                let Some(technique) = set.technique(tech_type) else {
                    continue;
                };
                let mut seen = std::collections::BTreeSet::new();
                let (types, used) =
                    technique_compile_vertex_types(tech, technique, set.world_vert_format);
                for vertex_type in types[..used].iter().copied() {
                    if usize::from(vertex_type) >= asset_iw4::vertex_decl::VERTEX_TYPE_COUNT
                        || !seen.insert(vertex_type)
                    {
                        continue;
                    }
                    *type_hist.entry(vertex_type).or_default() += 1;
                    self.total = self.total.saturating_add(1);
                    self.jobs.push(CompileJob {
                        tech,
                        vertex_type,
                        set_i,
                        pass_i,
                    });
                }
            }
        }
        for &name in super::postfx_plan::POSTFX_MATERIALS {
            match postfx_compile_location(catalog, name) {
                Ok((set_i, pass_i)) => {
                    *type_hist.entry(POSTFX_VERTEX_TYPE).or_default() += 1;
                    self.total = self.total.saturating_add(1);
                    self.jobs.push(CompileJob {
                        tech: POSTFX_TECH_TYPE,
                        vertex_type: POSTFX_VERTEX_TYPE,
                        set_i,
                        pass_i,
                    });
                }
                Err(cause) => {
                    diag::warn!(World, "post-fx film compile admission: RED cause={cause:?}");
                }
            }
        }
        for &name in super::postfx_plan::GLOW_MATERIALS
            .iter()
            .chain([&super::postfx_plan::BLOOD_MATERIAL])
        {
            match postfx_compile_location(catalog, name) {
                Ok((set_i, pass_i)) => {
                    *type_hist.entry(POSTFX_VERTEX_TYPE).or_default() += 1;
                    self.total = self.total.saturating_add(1);
                    self.jobs.push(CompileJob {
                        tech: POSTFX_TECH_TYPE,
                        vertex_type: POSTFX_VERTEX_TYPE,
                        set_i,
                        pass_i,
                    });
                }
                Err(super::postfx_plan::PostFxAdmissionRefusal::MaterialMissing) => {}
                Err(cause) => {
                    diag::warn!(
                        World,
                        "2D material compile skipped material={name} cause={cause:?}"
                    );
                }
            }
        }
        self.armed = true;
        self.task = None;
        self.progress = None;
        self.started = None;
        self.done = 0;
        self.ports.clear();
        self.port_by_id.clear();
        self.pending.clear();
        self.absorb_at = 0;
        self.postfx_ports.clear();
        self.world_causes.clear();
        self.packed_causes.clear();
        self.pos_tex_causes.clear();
        self.postfx_causes.clear();
        self.refused_world = 0;
        self.refused_packed = 0;
        self.refused_pos_tex = 0;
        self.refused_postfx = 0;
        if let Some(stage) = &self.compile_stage {
            stage.set_total(u64::from(self.total));
        }
        diag::info!(
            World,
            "world spawn compile armed: jobs={} unique_passes={} vertex_type_hist={type_hist:?}",
            self.jobs.len(),
            self.total
        );
    }
}
