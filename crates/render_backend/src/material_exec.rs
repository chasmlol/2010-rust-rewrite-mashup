use std::collections::HashMap;
use std::sync::Arc;

use glam::Mat4;

use crate::overlay::{
    OverlayCodeNeed, apply_shadowable_light, overlay_code_mesh_tess_code_constants,
    overlay_particle_cloud_tess_code_constants, overlay_smodel_tess_code_constants,
    overlay_smodel_world_matrix, overlay_viewmodel_depth_hack,
};
use render_frame::{
    MaterialExecFrame, RetainedDrawItem, RetainedDrawKind, host_viewmodel_render_fx_flags,
};
use render_material::{
    LayeredCodeSources, MaterialDrawKey, MaterialExecution, MaterialGenerationId, MaterialRefusal,
    PackedCodeConstantLane, PreparedMaterialTable, RuntimeCodeSources, RuntimeMaterialCatalog,
    StableMaterialShell, TechType, execute_material_with_shell, smodel_tess_vertex_type,
    world_tess_vertex_type, world_tess_vertex_type_authored,
};

#[derive(Clone, Copy)]
pub struct MaterialExecView<'a> {
    pub catalog: &'a RuntimeMaterialCatalog,
    pub prepared: &'a PreparedMaterialTable,
    pub frame: &'a MaterialExecFrame,

    pub clip_from_world: Option<Mat4>,

    pub view_from_world: Option<Mat4>,

    pub code_sources: &'a RuntimeCodeSources,
}

impl<'a> MaterialExecView<'a> {
    pub fn camera(
        catalog: &'a RuntimeMaterialCatalog,
        prepared: &'a PreparedMaterialTable,
        frame: &'a MaterialExecFrame,
    ) -> Self {
        Self {
            catalog,
            prepared,
            frame,
            clip_from_world: frame.clip_from_world,
            view_from_world: frame.view_from_world,
            code_sources: &frame.code_sources,
        }
    }

    pub fn shadow_partition(
        catalog: &'a RuntimeMaterialCatalog,
        prepared: &'a PreparedMaterialTable,
        frame: &'a MaterialExecFrame,
        clip_from_world: Mat4,
        view_from_world: Mat4,
        code_sources: &'a RuntimeCodeSources,
    ) -> Self {
        Self {
            catalog,
            prepared,
            frame,
            clip_from_world: Some(clip_from_world),
            view_from_world: Some(view_from_world),
            code_sources,
        }
    }
}

pub fn draw_vertex_type(
    catalog: &RuntimeMaterialCatalog,
    key: MaterialDrawKey,
    kind: &RetainedDrawKind,
    tech_type: TechType,
) -> u8 {
    match kind {
        RetainedDrawKind::Smodel { stream, .. } => smodel_tess_vertex_type(*stream),

        RetainedDrawKind::XModel { .. } => render_frame::xmodel_tess_info_vert_decl_type(1),

        RetainedDrawKind::CodeMesh { .. } | RetainedDrawKind::Glass { .. } => {
            asset_iw4::vertex_decl::PACKED_VERTEX_TYPE
        }
        RetainedDrawKind::World { .. } => world_tess_vertex_type(catalog, key, tech_type),

        RetainedDrawKind::MarkMesh { packed: true, .. } => {
            asset_iw4::vertex_decl::PACKED_VERTEX_TYPE
        }
        RetainedDrawKind::MarkMesh { packed: false, .. } => {
            world_tess_vertex_type_authored(catalog, key, tech_type)
        }
        RetainedDrawKind::ParticleCloud { .. } => asset_iw4::vertex_decl::POS_TEX_VERTEX_TYPE,
    }
}

fn execution_gpu_packed(packed: u64) -> u64 {
    packed & !0x3fff_ffff
}

fn execution_smodel_packed(packed: u64) -> u64 {
    packed & !0xffff
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ExecutionShareKey {
    gpu_packed: u64,

    material_rank: u32,
    material_id: Option<render_material::MaterialAssetId>,
    tech: u8,
    vertex_type: u8,
    extra: ExecutionShareExtra,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ShellInternKey {
    gpu_packed: u64,
    material_rank: u32,
    material_id: Option<render_material::MaterialAssetId>,
    tech: u8,
    vertex_type: u8,
}

impl ShellInternKey {
    fn from_share(share: ExecutionShareKey) -> Self {
        Self {
            gpu_packed: share.gpu_packed,
            material_rank: share.material_rank,
            material_id: share.material_id,
            tech: share.tech,
            vertex_type: share.vertex_type,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ExecutionShareExtra {
    Identity,
    ReflectionProbe(u8),
    ViewmodelCodeMesh,
    LightingHandle(u32),

    Instance { depth_hack: bool },
    Unique(u32),
    MarkMesh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct OverlayKeepKey {
    scene_light: u8,
    extra: OverlayKeepExtra,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum OverlayKeepExtra {
    World(Option<u8>),
    ViewmodelCodeMesh,
    Lighting {
        handle: u32,
        packed: Option<[u8; 4]>,
        depth_hack: bool,
    },
    Unique(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OverlayMode {
    Full,
    ObjOnly,
}

pub fn draw_material_key(draw: &RetainedDrawItem) -> MaterialDrawKey {
    MaterialDrawKey::new(draw.key, draw.material_rank).with_material_id(draw.material_id)
}

fn xmodel_depth_hack(key: u64) -> bool {
    host_viewmodel_render_fx_flags(dpvs_iw4::GfxDrawSurf { packed: key }.object_id()) != 0
}

fn per_surface_reflection_sh(
    frame: &MaterialExecFrame,
    draw: &RetainedDrawItem,
) -> Option<(u8, [[f32; 4]; 3])> {
    let probe = draw.surface_samplers.reflection_probe?.0;
    let sh = (*frame.reflection_probe_sh.get(usize::from(probe))?)?;
    Some((probe, sh))
}

fn overlay_reflection_sh(
    frame: &MaterialExecFrame,
    draw: &RetainedDrawItem,
    scratch: &mut RuntimeCodeSources,
) {
    if let Some((_, sh)) = per_surface_reflection_sh(frame, draw) {
        for (index, row) in render_material::CODE_T6_REFLECTION_SH.into_iter().zip(sh) {
            scratch.set_constant_rows(index, &[row.map(f32::to_bits)]);
        }
    }
}

fn overlay_smodel_lighting_sh(
    frame: &MaterialExecFrame,
    draw: &RetainedDrawItem,
    scratch: &mut RuntimeCodeSources,
) {
    let RetainedDrawKind::Smodel { placement, .. } = draw.kind else {
        return;
    };
    let sh = frame
        .smodel_lighting_sh
        .get(placement as usize)
        .copied()
        .flatten()
        .unwrap_or([[1.0, 1.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0], [0.0; 4]]);
    for (index, row) in render_material::CODE_T6_GRID_SH.into_iter().zip(sh) {
        scratch.set_constant_rows(index, &[row.map(f32::to_bits)]);
    }
}

fn execution_share_extra(
    frame: &MaterialExecFrame,
    draw: &RetainedDrawItem,
) -> ExecutionShareExtra {
    match draw.kind {
        RetainedDrawKind::MarkMesh { .. } => ExecutionShareExtra::MarkMesh,
        RetainedDrawKind::World { .. } => match per_surface_reflection_sh(frame, draw) {
            Some((probe, _)) => ExecutionShareExtra::ReflectionProbe(probe),
            None => ExecutionShareExtra::Identity,
        },
        RetainedDrawKind::Glass {
            lighting_handle, ..
        } => ExecutionShareExtra::LightingHandle(lighting_handle),
        RetainedDrawKind::Smodel { .. } => ExecutionShareExtra::Instance { depth_hack: false },
        RetainedDrawKind::XModel { .. } => ExecutionShareExtra::Instance {
            depth_hack: xmodel_depth_hack(draw.key),
        },
        RetainedDrawKind::ParticleCloud { draw, .. } => ExecutionShareExtra::Unique(draw),
        RetainedDrawKind::CodeMesh {
            draw, arg_count, ..
        } if arg_count != 0 => ExecutionShareExtra::Unique(draw),
        RetainedDrawKind::CodeMesh {
            viewmodel: true, ..
        } => ExecutionShareExtra::ViewmodelCodeMesh,
        RetainedDrawKind::CodeMesh { .. } => ExecutionShareExtra::Identity,
    }
}

fn execution_share_key(
    frame: &MaterialExecFrame,
    draw: &RetainedDrawItem,
    tech: TechType,
    vertex_type: u8,
) -> ExecutionShareKey {
    let gpu_packed = match draw.kind {
        RetainedDrawKind::Smodel { .. } | RetainedDrawKind::XModel { .. } => {
            execution_smodel_packed(draw.key)
        }
        _ => execution_gpu_packed(draw.key),
    };
    ExecutionShareKey {
        gpu_packed,
        material_rank: draw.material_rank,
        material_id: draw.material_id,
        tech: tech.0,
        vertex_type,
        extra: execution_share_extra(frame, draw),
    }
}

fn instance_matrix_bits(draw: &RetainedDrawItem) -> Option<[u32; 16]> {
    match draw.kind {
        RetainedDrawKind::Smodel {
            world_from_local, ..
        }
        | RetainedDrawKind::XModel {
            world_from_local, ..
        } => Some(world_from_local.to_cols_array().map(f32::to_bits)),
        _ => None,
    }
}

fn smodel_code_world_from_local(kind: &RetainedDrawKind) -> Mat4 {
    match *kind {
        RetainedDrawKind::Smodel {
            stream: Some(lighting_iw4::SmodelSurfPath::Skinned),
            ..
        } => Mat4::IDENTITY,
        RetainedDrawKind::Smodel {
            world_from_local, ..
        }
        | RetainedDrawKind::XModel {
            world_from_local, ..
        } => world_from_local,
        _ => Mat4::IDENTITY,
    }
}

fn overlay_keep_key(frame: &MaterialExecFrame, draw: &RetainedDrawItem) -> OverlayKeepKey {
    let scene_light = dpvs_iw4::GfxDrawSurf { packed: draw.key }.scene_light_index();
    let extra = match draw.kind {
        RetainedDrawKind::CodeMesh {
            viewmodel: true, ..
        } => OverlayKeepExtra::ViewmodelCodeMesh,
        RetainedDrawKind::World { .. } => {
            OverlayKeepExtra::World(per_surface_reflection_sh(frame, draw).map(|(probe, _)| probe))
        }
        RetainedDrawKind::CodeMesh { .. } => OverlayKeepExtra::World(None),
        RetainedDrawKind::MarkMesh {
            lighting_handle, ..
        } => OverlayKeepExtra::Lighting {
            handle: lighting_handle,
            packed: None,
            depth_hack: false,
        },
        RetainedDrawKind::Glass {
            lighting_handle, ..
        } => OverlayKeepExtra::Lighting {
            handle: lighting_handle,
            packed: None,
            depth_hack: false,
        },
        RetainedDrawKind::Smodel {
            lighting_handle,
            packed_lighting,
            ..
        } => OverlayKeepExtra::Lighting {
            handle: lighting_handle,
            packed: packed_lighting,
            depth_hack: false,
        },
        RetainedDrawKind::XModel {
            lighting_handle,
            packed_lighting,
            ..
        } => OverlayKeepExtra::Lighting {
            handle: lighting_handle,
            packed: packed_lighting,
            depth_hack: xmodel_depth_hack(draw.key),
        },
        RetainedDrawKind::ParticleCloud { draw, .. } => OverlayKeepExtra::Unique(draw),
    };
    OverlayKeepKey { scene_light, extra }
}

fn overlay_draw_material(
    runtime: MaterialExecView<'_>,
    draw: &RetainedDrawItem,
    scratch: &mut RuntimeCodeSources,
    need: OverlayCodeNeed,
) -> Result<(), MaterialRefusal> {
    let inv_image_height = runtime.frame.inv_image_height;
    scratch.begin_overlay();
    overlay_reflection_sh(runtime.frame, draw, scratch);
    overlay_smodel_lighting_sh(runtime.frame, draw, scratch);
    apply_shadowable_light(
        scratch,
        draw.key,
        &runtime.frame.primary_lights,
        &runtime.frame.attenuation,
        &runtime.frame.local_light_bindings,
        runtime.catalog,
        runtime.frame.view_origin,
        &runtime.frame.spot_receivers,
    )?;
    match draw.kind {
        RetainedDrawKind::Smodel {
            lighting_handle,
            packed_lighting,
            ..
        } => {
            overlay_smodel_world_matrix(
                scratch,
                runtime.frame.view_origin,
                smodel_code_world_from_local(&draw.kind),
                runtime.clip_from_world,
                runtime.view_from_world,
                need,
            );

            overlay_smodel_tess_code_constants(
                scratch,
                lighting_handle,
                inv_image_height,
                packed_lighting,
                need,
            );
        }
        RetainedDrawKind::XModel {
            lighting_handle,
            packed_lighting,
            world_from_local,
            ..
        } => {
            overlay_smodel_world_matrix(
                scratch,
                runtime.frame.view_origin,
                world_from_local,
                runtime.clip_from_world,
                runtime.view_from_world,
                need,
            );

            overlay_smodel_tess_code_constants(
                scratch,
                lighting_handle,
                inv_image_height,
                packed_lighting,
                need,
            );
            let object_id = dpvs_iw4::GfxDrawSurf { packed: draw.key }.object_id();
            if host_viewmodel_render_fx_flags(object_id) != 0
                && let Some(clip) = runtime.frame.viewmodel_clip_from_world
            {
                overlay_viewmodel_depth_hack(
                    scratch,
                    clip,
                    runtime.frame.view_origin,
                    world_from_local,
                );
            }
        }
        RetainedDrawKind::ParticleCloud { clouds, .. } => {
            overlay_particle_cloud_tess_code_constants(
                scratch,
                runtime.frame.view_origin,
                &clouds,
                runtime.clip_from_world,
                runtime.view_from_world,
                runtime.frame.clip_from_view,
                runtime.frame.outdoor.as_ref(),
            );
        }
        RetainedDrawKind::CodeMesh {
            args,
            arg_count,
            viewmodel,
            ..
        } => {
            if arg_count != 0 {
                overlay_code_mesh_tess_code_constants(scratch, &args[..usize::from(arg_count)]);
            }
            if viewmodel && let Some(clip) = runtime.frame.viewmodel_clip_from_world {
                overlay_viewmodel_depth_hack(
                    scratch,
                    clip,
                    runtime.frame.view_origin,
                    Mat4::IDENTITY,
                );
            }
        }
        RetainedDrawKind::Glass {
            lighting_handle, ..
        } => {
            overlay_smodel_tess_code_constants(
                scratch,
                lighting_handle,
                inv_image_height,
                None,
                need,
            );
        }
        RetainedDrawKind::MarkMesh {
            lighting_handle, ..
        } => {
            overlay_smodel_tess_code_constants(
                scratch,
                lighting_handle,
                inv_image_height,
                None,
                need,
            );
        }
        _ => {}
    }
    Ok(())
}

fn overlay_draw_obj_only(
    runtime: MaterialExecView<'_>,
    draw: &RetainedDrawItem,
    scratch: &mut RuntimeCodeSources,
    need: OverlayCodeNeed,
) {
    let inv_image_height = runtime.frame.inv_image_height;
    overlay_reflection_sh(runtime.frame, draw, scratch);
    overlay_smodel_lighting_sh(runtime.frame, draw, scratch);
    match draw.kind {
        RetainedDrawKind::Smodel {
            lighting_handle,
            packed_lighting,
            ..
        }
        | RetainedDrawKind::XModel {
            lighting_handle,
            packed_lighting,
            ..
        } => {
            overlay_smodel_world_matrix(
                scratch,
                runtime.frame.view_origin,
                smodel_code_world_from_local(&draw.kind),
                runtime.clip_from_world,
                runtime.view_from_world,
                need,
            );
            overlay_smodel_tess_code_constants(
                scratch,
                lighting_handle,
                inv_image_height,
                packed_lighting,
                need,
            );
            if matches!(draw.kind, RetainedDrawKind::XModel { .. })
                && xmodel_depth_hack(draw.key)
                && let Some(clip) = runtime.frame.viewmodel_clip_from_world
            {
                overlay_viewmodel_depth_hack(
                    scratch,
                    clip,
                    runtime.frame.view_origin,
                    smodel_code_world_from_local(&draw.kind),
                );
            }
        }
        RetainedDrawKind::ParticleCloud { clouds, .. } => {
            overlay_particle_cloud_tess_code_constants(
                scratch,
                runtime.frame.view_origin,
                &clouds,
                runtime.clip_from_world,
                runtime.view_from_world,
                runtime.frame.clip_from_view,
                runtime.frame.outdoor.as_ref(),
            );
        }
        RetainedDrawKind::CodeMesh {
            args,
            arg_count,
            viewmodel,
            ..
        } => {
            if arg_count != 0 {
                overlay_code_mesh_tess_code_constants(scratch, &args[..usize::from(arg_count)]);
            }
            if viewmodel && let Some(clip) = runtime.frame.viewmodel_clip_from_world {
                overlay_viewmodel_depth_hack(
                    scratch,
                    clip,
                    runtime.frame.view_origin,
                    Mat4::IDENTITY,
                );
            }
        }
        _ => {}
    }
}

/// Fill the overlay sources this draw's material will read.
///
/// `need` is what the shell this run is about to bind says it reads. A shell
/// that has been seen before answers that question *before* the overlay is
/// built, which is the point: a world-view-projection nobody samples is a
/// matrix multiply, a transpose and an allocation for a value that is then
/// dropped. When the shell is new the need is unknown and everything is
/// filled — once per shell per generation, and that miss is what teaches it.
fn overlay_draw_execution(
    runtime: MaterialExecView<'_>,
    draw: &RetainedDrawItem,
    scratch: &mut RuntimeCodeSources,
    overlay: OverlayMode,
    need: OverlayCodeNeed,
) -> Result<(), MaterialRefusal> {
    match overlay {
        OverlayMode::Full => overlay_draw_material(runtime, draw, scratch, need)?,
        OverlayMode::ObjOnly => overlay_draw_obj_only(runtime, draw, scratch, need),
    }
    Ok(())
}

fn overlay_need_from_execution(exec: &MaterialExecution) -> OverlayCodeNeed {
    let mut need = OverlayCodeNeed::NONE;
    for pass in exec.iter_passes() {
        for lane in pass.code_constants {
            need.note(lane.index);
        }
    }
    need
}

fn patch_code_lane(
    pass_index: u8,
    lane: &PackedCodeConstantLane,
    sources: &RuntimeCodeSources,
) -> Result<PackedCodeConstantLane, MaterialRefusal> {
    let Some(src) = sources.constant_arc(lane.index) else {
        return Ok(lane.clone());
    };
    let start = usize::from(lane.first_row);
    let end = start.saturating_add(usize::from(lane.row_count));
    if src.get(start..end).is_none() {
        return Err(MaterialRefusal::CodeConstantRowsOutOfRange {
            pass_index,
            stage: lane.stage,
            index: lane.index,
            first_row: lane.first_row,
            row_count: lane.row_count,
            available_rows: src.len(),
        });
    }
    Ok(PackedCodeConstantLane {
        stage: lane.stage,
        destination: lane.destination,
        index: lane.index,
        first_row: lane.first_row,
        row_count: lane.row_count,
        rows: src,
    })
}

#[derive(Clone, Copy, Debug)]
pub struct PlaceLanes<'a> {
    rows: &'a [(u32, u32)],
    lanes: &'a [PackedCodeConstantLane],
}

impl<'a> PlaceLanes<'a> {
    pub fn pass(&self, index: usize) -> Option<&'a [PackedCodeConstantLane]> {
        let &(start, len) = self.rows.get(index)?;
        let start = start as usize;
        self.lanes.get(start..start.saturating_add(len as usize))
    }
}

struct RunState {
    share: ExecutionShareKey,
    execution: MaterialExecution,

    first_matrix: Option<[u32; 16]>,
    first_overlay: OverlayKeepKey,

    need: OverlayCodeNeed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MaterialRunCensus {
    pub draws: u32,
    pub material_runs: u32,
    pub pass_setups: u32,
    pub obj_binds: u32,
    pub refused: u32,
    pub shell_hits: u32,
    pub shell_misses: u32,
    /// Code-constant writes the overlay made for this list, and how many of
    /// its runs knew what the shell reads before building it. A run that does
    /// not know fills everything; the two together are what says whether the
    /// writes fell because less is computed or because less was drawn.
    pub overlay_const_writes: u32,
    pub overlay_need_known: u32,
}

#[derive(Debug, Default)]
pub struct MaterialRunExecutor {
    scratch: RuntimeCodeSources,
    place_scratch: RuntimeCodeSources,
    last_overlay: Option<OverlayKeepKey>,
    run: Option<RunState>,
    place_rows: Vec<(u32, u32)>,
    place_lanes: Vec<PackedCodeConstantLane>,

    run_serial: u64,
    census: MaterialRunCensus,
    shells: HashMap<ShellInternKey, ShellEntry>,
    shell_generation: Option<MaterialGenerationId>,
}

/// A material shell kept across runs, and what it reads.
///
/// The need is a property of the shell — the code constants its passes bind —
/// so it is stored where the shell is and answers before the overlay for every
/// draw after the first of its kind.
#[derive(Debug)]
struct ShellEntry {
    shell: Arc<StableMaterialShell>,
    need: OverlayCodeNeed,
}

impl std::fmt::Debug for RunState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunState")
            .field("share", &self.share)
            .finish()
    }
}

impl MaterialRunExecutor {
    pub fn begin_list(&mut self) {
        self.scratch.reset_overlay();
        self.place_scratch.reset_overlay();
        self.last_overlay = None;
        self.run = None;
        self.place_rows.clear();
        self.place_lanes.clear();
        self.run_serial = self.run_serial.wrapping_add(1);
        self.census = MaterialRunCensus::default();
    }

    fn retain_shells_for(&mut self, generation: MaterialGenerationId) {
        if self.shell_generation != Some(generation) {
            self.shells.clear();
            self.run = None;
            self.last_overlay = None;
            self.scratch.reset_overlay();
            self.shell_generation = Some(generation);
        }
    }

    pub fn run_serial(&self) -> u64 {
        self.run_serial
    }

    pub fn census(&self) -> MaterialRunCensus {
        self.census
    }

    pub fn vertex_type(
        view: MaterialExecView<'_>,
        draw: &RetainedDrawItem,
        tech_type: TechType,
    ) -> u8 {
        draw_vertex_type(view.catalog, draw_material_key(draw), &draw.kind, tech_type)
    }

    pub fn execute(
        &mut self,
        view: MaterialExecView<'_>,
        draw: &RetainedDrawItem,
        tech_type: TechType,
        vertex_type: u8,
        patch_instance_code: bool,
    ) -> Result<(), MaterialRefusal> {
        self.census.draws = self.census.draws.saturating_add(1);
        self.retain_shells_for(view.catalog.generation_id());
        let scene_light = dpvs_iw4::GfxDrawSurf { packed: draw.key }.scene_light_index();
        if scene_light != 0
            && view
                .frame
                .primary_lights
                .get(usize::from(scene_light))
                .is_some()
        {
            let validation = view
                .frame
                .local_light_bindings
                .get(usize::from(scene_light))
                .ok_or(MaterialRefusal::MissingLightBindings { scene_light })
                .and_then(|bindings| bindings.validate_generation(view.catalog));
            if let Err(cause) = validation {
                self.census.refused = self.census.refused.saturating_add(1);
                return Err(cause);
            }
        }
        self.place_rows.clear();
        self.place_lanes.clear();
        let share = execution_share_key(view.frame, draw, tech_type, vertex_type);
        let overlay_key = overlay_keep_key(view.frame, draw);
        if self.run.as_ref().is_some_and(|run| run.share == share) {
            let Self {
                run,
                place_scratch,
                place_rows,
                place_lanes,
                ..
            } = self;
            let run = run.as_ref().expect("run checked above");
            let matrix = instance_matrix_bits(draw);
            if patch_instance_code
                && matrix.is_some()
                && (run.first_matrix != matrix || run.first_overlay != overlay_key)
            {
                self.census.obj_binds = self.census.obj_binds.saturating_add(1);
                place_scratch.begin_overlay();
                overlay_draw_obj_only(view, draw, place_scratch, run.need);
                for pass in run.execution.iter_passes() {
                    let start =
                        u32::try_from(place_lanes.len()).expect("place-code lane buffer fits u32");
                    for lane in pass.code_constants {
                        if !place_scratch.has_constant(lane.index) {
                            continue;
                        }
                        match patch_code_lane(pass.pass_index, lane, place_scratch) {
                            Ok(patched) => place_lanes.push(patched),
                            Err(cause) => {
                                place_rows.clear();
                                place_lanes.clear();
                                self.census.refused = self.census.refused.saturating_add(1);
                                return Err(cause);
                            }
                        }
                    }
                    let len = u32::try_from(place_lanes.len())
                        .expect("place-code lane buffer fits u32")
                        .saturating_sub(start);
                    place_rows.push((start, len));
                }
            }
            return Ok(());
        }
        let overlay = if self.last_overlay == Some(overlay_key) {
            OverlayMode::ObjOnly
        } else {
            OverlayMode::Full
        };
        self.last_overlay = Some(overlay_key);
        let mut recycled = self.run.take().map(|run| run.execution);
        if let Some(execution) = recycled.as_mut() {
            execution.release_code_rows();
        }
        // Before the overlay, not after: the shell a run will bind is decided
        // by the share key alone, so what it reads is known while there is
        // still a chance not to compute the rest. `retain_shells_for` comes
        // first because a shell from a previous material generation answers
        // for a material that is gone.
        let shell_key = ShellInternKey::from_share(share);
        let known_need = self.shells.get(&shell_key).map(|entry| entry.need);
        let need = known_need.unwrap_or(OverlayCodeNeed::ALL);
        let writes_before = self.scratch.const_writes();
        if let Err(cause) = overlay_draw_execution(view, draw, &mut self.scratch, overlay, need) {
            self.last_overlay = None;
            self.census.refused = self.census.refused.saturating_add(1);
            return Err(cause);
        }
        self.census.overlay_const_writes = self.census.overlay_const_writes.saturating_add(
            u32::try_from(self.scratch.const_writes() - writes_before).unwrap_or(u32::MAX),
        );
        if known_need.is_some() {
            self.census.overlay_need_known = self.census.overlay_need_known.saturating_add(1);
        }
        let execution = {
            let Self {
                shells,
                scratch,
                census,
                ..
            } = self;
            let sources = LayeredCodeSources {
                base: view.code_sources,
                overlay: scratch,
            };
            if let Some(entry) = shells.get(&shell_key) {
                census.shell_hits = census.shell_hits.saturating_add(1);
                let mut execution = recycled.unwrap_or_else(MaterialExecution::vacant);
                match execution.rebind_into(&entry.shell, view.catalog, &sources) {
                    Ok(()) => Ok(execution),
                    Err(cause) => Err(cause),
                }
            } else {
                census.shell_misses = census.shell_misses.saturating_add(1);
                drop(recycled);
                execute_material_with_shell(
                    view.catalog,
                    view.prepared,
                    &sources,
                    draw_material_key(draw),
                    tech_type,
                    vertex_type,
                )
                .map(|execution| {
                    shells.insert(
                        shell_key,
                        ShellEntry {
                            shell: Arc::clone(execution.shell_arc()),
                            need: overlay_need_from_execution(&execution),
                        },
                    );
                    execution
                })
            }
        };
        match execution {
            Ok(execution) => {
                self.run_serial = self.run_serial.wrapping_add(1);
                self.census.material_runs = self.census.material_runs.saturating_add(1);
                self.census.pass_setups = self
                    .census
                    .pass_setups
                    .saturating_add(u32::try_from(execution.pass_count()).unwrap_or(u32::MAX));
                self.run = Some(RunState {
                    share,
                    need: overlay_need_from_execution(&execution),
                    execution,
                    first_matrix: instance_matrix_bits(draw),
                    first_overlay: overlay_key,
                });
                Ok(())
            }
            Err(cause) => {
                self.last_overlay = None;
                self.census.refused = self.census.refused.saturating_add(1);
                Err(cause)
            }
        }
    }

    pub fn execution(&self) -> &MaterialExecution {
        &self
            .run
            .as_ref()
            .expect("execution() after a refused or unopened run")
            .execution
    }

    pub fn place(&self) -> Option<PlaceLanes<'_>> {
        (!self.place_rows.is_empty()).then_some(PlaceLanes {
            rows: &self.place_rows,
            lanes: &self.place_lanes,
        })
    }
}
