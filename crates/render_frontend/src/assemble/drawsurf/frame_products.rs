use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::tasks::ComputeTaskPool;
use std::sync::Arc;

use super::command_context::LightAttenuationBind;

use super::material_runtime::{
    MaterialGenerationId, PreparedMaterialTable, RuntimeCodeSources, RuntimeMaterialCatalog,
    RuntimeProgramPort, RuntimeProgramRegistry, SurfaceSamplerInputs,
};
use super::retained_list::{RetainedDrawItem, RetainedDrawKind, StaticDrawLane};
use super::setup::TechType;
use super::tess::world::WorldDrawGpuPlan;
use crate::prepare::scene::model_lighting_atlas::WorldModelLightingAtlas;
use render_frame::PackedFrontendLists;
pub use render_frame::{
    FrameProduct, FrameProductKind, FrameProductStatus, FrameProductsSnapshot, MissingProductCause,
    ProductTarget, RenderFocusFrame, SpotShadowFrameSlot,
};

#[derive(Resource)]
pub struct DistortionSettings {
    pub enabled: bool,
}

impl Default for DistortionSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColourDrawMethod {
    #[default]
    Standard,
    Fullbright,
    DebugMaterial,
}

impl ColourDrawMethod {
    pub const fn tech_type(self) -> TechType {
        match self {
            Self::Standard => TechType(lighting_iw4::GFX_DRAW_METHOD_LIT_BEGIN),
            Self::Fullbright => TechType(4),
            Self::DebugMaterial => TechType(0x2e),
        }
    }

    pub const fn emissive_tech_type(self) -> TechType {
        match self {
            Self::Standard => TechType(5),
            Self::Fullbright => TechType(4),
            Self::DebugMaterial => TechType(0x2e),
        }
    }
}

pub(crate) fn product_log_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("IW4L_PRODUCT_LOG").is_some())
}

pub(crate) fn log_probe_index_census(
    frame: Res<RenderFrameProducts>,
    mut previous: Local<Option<String>>,
) {
    if !product_log_enabled() {
        return;
    }
    let colour = frame.product(FrameProductKind::Colour);
    if colour.ordered_draws.is_empty() {
        return;
    }
    let line = probe_index_census_line(&colour.ordered_draws);
    if previous.as_ref() == Some(&line) {
        return;
    }
    *previous = Some(line.clone());
    diag::warn!(World, "{line}");
}

fn probe_index_census_line(draws: &[RetainedDrawItem]) -> String {
    let mut world = std::collections::BTreeMap::<u8, usize>::new();
    let mut smodel = std::collections::BTreeMap::<u8, usize>::new();
    let mut fpv = std::collections::BTreeMap::<u8, usize>::new();
    let mut xmodel = std::collections::BTreeMap::<u8, usize>::new();
    let mut packed_ne_bound = 0usize;
    for draw in draws {
        let fields = dpvs_iw4::unpack(dpvs_iw4::GfxDrawSurf { packed: draw.key });
        let packed = fields.reflection_probe_index;
        let hist = match draw.kind {
            RetainedDrawKind::World { .. } => &mut world,
            RetainedDrawKind::Smodel { .. } => &mut smodel,
            RetainedDrawKind::XModel { .. }
                if fields.object_id == super::tess::xmodel::XMODEL_OBJECT_ID_VIEWMODEL =>
            {
                &mut fpv
            }
            RetainedDrawKind::XModel { .. } => &mut xmodel,
            RetainedDrawKind::CodeMesh { .. }
            | RetainedDrawKind::ParticleCloud { .. }
            | RetainedDrawKind::MarkMesh { .. }
            | RetainedDrawKind::Glass { .. } => {
                continue;
            }
        };
        let bound = draw.surface_samplers.reflection_probe.map(|id| id.0);
        if bound != Some(packed) {
            packed_ne_bound = packed_ne_bound.saturating_add(1);
        }
        *hist.entry(packed).or_default() += 1;
    }
    format!(
        "probe index census: world={} smodel={} fpv={} xmodel_other={} packed_ne_bound={packed_ne_bound} (zero names cube 0, not absence)",
        format_probe_hist(&world),
        format_probe_hist(&smodel),
        format_probe_hist(&fpv),
        format_probe_hist(&xmodel),
    )
}

fn format_probe_hist(hist: &std::collections::BTreeMap<u8, usize>) -> String {
    let n: usize = hist.values().sum();
    let zero = hist.get(&0).copied().unwrap_or(0);
    let body = hist
        .iter()
        .map(|(index, count)| format!("{index}:{count}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!("n={n} zero={zero} {{{body}}}")
}

pub use render_anim::RenderFocus;

const PRODUCT_BANKS: usize = 3;

#[derive(Resource, Debug)]
pub struct RenderFrameProducts {
    pub frame_id: u64,
    banks: [Arc<FrameProductsSnapshot>; PRODUCT_BANKS],
    write: usize,
    published: usize,

    world_run_revision: u64,

    pub last_bank_new: u8,
}

impl Default for RenderFrameProducts {
    fn default() -> Self {
        Self {
            frame_id: 0,
            banks: [
                Arc::new(FrameProductsSnapshot::empty()),
                Arc::new(FrameProductsSnapshot::empty()),
                Arc::new(FrameProductsSnapshot::empty()),
            ],
            write: 0,
            published: 0,
            world_run_revision: 0,
            last_bank_new: 0,
        }
    }
}

impl RenderFrameProducts {
    pub fn product(&self, kind: FrameProductKind) -> &FrameProduct {
        self.banks[self.published].product(kind)
    }

    pub fn published(&self) -> Arc<FrameProductsSnapshot> {
        Arc::clone(&self.banks[self.published])
    }

    fn begin_fill(&mut self) {
        let a = (self.published + 1) % PRODUCT_BANKS;
        let b = (self.published + 2) % PRODUCT_BANKS;
        self.write = if Arc::strong_count(&self.banks[a]) == 1 {
            a
        } else {
            b
        };
        self.last_bank_new = 0;
        if Arc::strong_count(&self.banks[self.write]) > 1 {
            self.banks[self.write] = Arc::new(FrameProductsSnapshot::empty());
            self.last_bank_new = 1;
        }
        let snap = Arc::get_mut(&mut self.banks[self.write]).expect("write bank is unique");
        snap.frame_id = 0;
        snap.focus = None;
    }

    fn adopt(&mut self, product: &mut FrameProduct) {
        let kind = product.kind;
        let slot = self
            .bank_mut()
            .iter_mut()
            .find(|slot| slot.kind == kind)
            .expect("RenderFrameProducts must contain every published product");
        std::mem::swap(slot, product);
    }

    fn bank_mut(&mut self) -> &mut [FrameProduct] {
        &mut Arc::get_mut(&mut self.banks[self.write])
            .expect("write bank is unique")
            .products
    }

    fn set_world_run_surfs(&mut self, surfs: &[u16]) {
        let same_as_published = self.banks[self.published].world_run_surfs.as_slice() == surfs;
        if !same_as_published {
            self.world_run_revision = self.world_run_revision.wrapping_add(1);
        }
        let snap = Arc::get_mut(&mut self.banks[self.write]).expect("write bank is unique");
        if snap.world_run_surfs.as_slice() != surfs {
            snap.world_run_surfs.clear();
            snap.world_run_surfs.extend_from_slice(surfs);
        }
        snap.world_run_revision = self.world_run_revision;
    }

    fn set_focus(&mut self, focus: Option<RenderFocusFrame>) {
        Arc::get_mut(&mut self.banks[self.write])
            .expect("write bank is unique")
            .focus = focus;
    }

    fn finish_fill(&mut self) {
        if let Some(snap) = Arc::get_mut(&mut self.banks[self.write]) {
            snap.frame_id = self.frame_id;
        }
        self.published = self.write;
    }
}

#[derive(Resource, Clone, Debug)]
pub struct ExtractedRenderFrameProducts(pub Arc<FrameProductsSnapshot>);

impl Default for ExtractedRenderFrameProducts {
    fn default() -> Self {
        Self(Arc::new(FrameProductsSnapshot::empty()))
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct MaterialGeneration {
    pub catalog: Arc<RuntimeMaterialCatalog>,
    pub programs: RuntimeProgramRegistry,

    pub prepared: Arc<PreparedMaterialTable>,

    pub exact_shaders: Vec<Handle<bevy::shader::Shader>>,

    pub postfx: super::RuntimePostFxResources,

    pub blood: Option<super::RuntimeBloodMaterial>,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct MaterialFrameInputs {
    pub(crate) material_bindings: Option<asset_material::PreparedMaterialBindings>,
    pub code_sources: RuntimeCodeSources,

    pub view_origin: Vec3,

    pub float_time: f32,

    pub clip_from_world: Option<Mat4>,

    pub view_from_world: Option<Mat4>,

    pub clip_from_view: Option<Mat4>,

    pub outdoor: Option<super::MapOutdoor>,

    pub viewmodel_clip_from_world: Option<Mat4>,

    pub viewmodel_near: Option<f32>,

    pub sun_shadow: Option<super::SunShadowForcedFrame>,

    pub spot_receivers: Vec<Option<render_frame::SpotShadowReceiver>>,
}

pub(crate) fn admit_material_generation(
    catalog: Arc<RuntimeMaterialCatalog>,
    programs: RuntimeProgramRegistry,
    exact_shaders: Vec<Handle<bevy::shader::Shader>>,
) -> MaterialGeneration {
    let prepared = PreparedMaterialTable::from_catalog(&catalog, |pass, vertex_type| {
        programs
            .port_for(pass, vertex_type)
            .map(RuntimeProgramPort::admitted_facts)
    });
    let postfx = super::build_runtime_postfx(&catalog, &prepared, &programs, &exact_shaders);
    let blood = super::build_runtime_blood(&catalog, &prepared, &programs, &exact_shaders)
        .inspect_err(|cause| diag::warn!(World, "blood material admission: RED cause={cause:?}"))
        .ok();
    MaterialGeneration {
        catalog,
        prepared: Arc::new(prepared),
        programs,
        exact_shaders,
        postfx,
        blood,
    }
}

#[derive(Debug, Default)]
struct ProductBindPersist {
    generation: MaterialGenerationId,
    compact_seen: HashMap<LogicalInputKey, u32>,
    compact_remap: Vec<u32>,
    compacted: bool,
    compact_tech: Vec<TechType>,
    last_refusals: Vec<render_frame::FrameMaterialRefusal>,
    last_mask: u64,
    last_has_codemesh: bool,
    last_world_pretess_id: u64,
    last_sun_near_n: usize,
}

impl ProductBindPersist {
    fn prepare(&mut self, generation: MaterialGenerationId) {
        self.generation = generation;
    }

    fn remember_compacted(&mut self, product: &FrameProduct) {
        self.compact_tech.clone_from(&product.draw_tech);
        self.last_refusals.clone_from(&product.material_refusals);
        self.last_mask = product.code_sampler_mask;
        self.last_has_codemesh = product.has_codemesh;
        self.last_world_pretess_id = product.world_pretess_id;
        self.last_sun_near_n = product.sun_near_n;
        self.compacted = true;
    }

    fn restore_compacted(&self, product: &mut FrameProduct, previous: &FrameProduct) -> bool {
        if !self.compacted || previous.ordered_draws.len() != self.compact_tech.len() {
            return false;
        }
        product.ordered_draws.clone_from(&previous.ordered_draws);
        product.draw_tech.clone_from(&self.compact_tech);
        product.code_sampler_mask = self.last_mask;
        product.material_refusals.clone_from(&self.last_refusals);
        product.has_codemesh = self.last_has_codemesh;
        product.world_pretess_id = self.last_world_pretess_id;
        product.sun_near_n = self.last_sun_near_n;
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CameraFillStamp {
    static_membership: u64,
    xmodel_membership: u64,
    fx_membership: u64,
    xmodel_payload: u64,
    fx_payload: u64,
    catalog: u64,
    world_generation: Option<u64>,
    colour_tech: u8,
    emissive_tech: u8,
    dfog: bool,
    sun_present: super::SunShadowReceivers,
    distortion_enabled: bool,
    distortion_sort_key: Option<u32>,
    light_types_id: u64,
    spot_lights_id: u64,
}

impl CameraFillStamp {
    fn composition_eq(self, other: Self) -> bool {
        self.static_membership == other.static_membership
            && self.xmodel_membership == other.xmodel_membership
            && self.fx_membership == other.fx_membership
            && self.catalog == other.catalog
            && self.world_generation == other.world_generation
            && self.colour_tech == other.colour_tech
            && self.emissive_tech == other.emissive_tech
            && self.dfog == other.dfog
            && self.sun_present == other.sun_present
            && self.distortion_enabled == other.distortion_enabled
            && self.distortion_sort_key == other.distortion_sort_key
            && self.light_types_id == other.light_types_id
            && self.spot_lights_id == other.spot_lights_id
    }

    fn payload_eq(self, other: Self) -> bool {
        self.xmodel_payload == other.xmodel_payload && self.fx_payload == other.fx_payload
    }

    fn digest(self) -> u64 {
        let mut id = super::list::CONTENT_ID_SEED;
        super::list::mix_content_id(&mut id, self.static_membership);
        super::list::mix_content_id(&mut id, self.xmodel_membership);
        super::list::mix_content_id(&mut id, self.fx_membership);
        super::list::mix_content_id(&mut id, self.catalog);
        super::list::mix_content_id(
            &mut id,
            self.world_generation.map_or(0, |g| g.wrapping_add(1)),
        );
        super::list::mix_content_id(&mut id, u64::from(self.colour_tech));
        super::list::mix_content_id(&mut id, u64::from(self.emissive_tech));
        super::list::mix_content_id(&mut id, u64::from(self.dfog));
        super::list::mix_content_id(&mut id, u64::from(self.sun_present.present));
        for word in self.sun_present.unmatched.0 {
            super::list::mix_content_id(&mut id, word);
        }
        super::list::mix_content_id(&mut id, u64::from(self.distortion_enabled));
        super::list::mix_content_id(
            &mut id,
            self.distortion_sort_key.map_or(0, |k| u64::from(k) + 1),
        );
        super::list::mix_content_id(&mut id, self.light_types_id);
        super::list::mix_content_id(&mut id, self.spot_lights_id);
        id
    }
}

fn light_types_id(types: &[u8]) -> u64 {
    let mut id = super::list::CONTENT_ID_SEED;
    super::list::mix_content_bytes(&mut id, types);
    id
}

fn world_pretess_id(product: &FrameProduct) -> u64 {
    expanded_world_pretess_id(product, &[])
}

pub(crate) fn expanded_world_pretess_id(product: &FrameProduct, run_surfs: &[u16]) -> u64 {
    let mut id = super::list::CONTENT_ID_SEED;
    for draw in &product.ordered_draws {
        let RetainedDrawKind::World {
            surf, run, run_off, ..
        } = draw.kind
        else {
            continue;
        };
        super::list::mix_content_id(&mut id, u64::from(surf));
        super::list::mix_content_id(&mut id, u64::from(run));
        super::list::mix_content_id(&mut id, u64::from(run_off));
        super::list::mix_content_id(&mut id, draw.key);
        mix_surface_sampler_inputs(&mut id, draw.surface_samplers);
        let members = run.max(1);
        for offset in 0..members {
            let member = if run <= 1 {
                surf
            } else {
                match run_surfs.get(run_off as usize + usize::from(offset)) {
                    Some(&s) => s,
                    None => u16::MAX,
                }
            };
            super::list::mix_content_id(&mut id, u64::from(member));
        }
    }
    id
}

fn mix_surface_sampler_inputs(id: &mut u64, samplers: SurfaceSamplerInputs) {
    let encode = |value: Option<u8>| value.map_or(0, |value| u64::from(value) + 1);
    super::list::mix_content_id(id, encode(samplers.reflection_probe.map(|value| value.0)));
    super::list::mix_content_id(id, encode(samplers.primary_lightmap.map(|value| value.0)));
    super::list::mix_content_id(id, encode(samplers.secondary_lightmap.map(|value| value.0)));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct LogicalInputKey {
    material_id: Option<render_material::MaterialAssetId>,
    material_rank: u32,
    packed: u64,
    tech: u8,
    kind_tag: u8,
    a: u32,
    b: u32,
}

fn logical_input_key(draw: &RetainedDrawItem, tech: TechType) -> LogicalInputKey {
    let (kind_tag, a, b) = match draw.kind {
        RetainedDrawKind::World {
            surf, run, run_off, ..
        } => (0, u32::from(surf) | (u32::from(run) << 16), run_off),
        RetainedDrawKind::Smodel {
            placement,
            surface,
            lighting_handle,
            ..
        } => (1, placement, surface ^ lighting_handle.rotate_left(16)),
        RetainedDrawKind::XModel {
            surface, object_id, ..
        } => (2, surface, u32::from(object_id)),
        RetainedDrawKind::CodeMesh {
            draw,
            arg_count,
            viewmodel,
            ..
        } => (3, draw, u32::from(arg_count) | (u32::from(viewmodel) << 8)),
        RetainedDrawKind::ParticleCloud { draw, .. } => (4, draw, 0),
        RetainedDrawKind::MarkMesh { draw, .. } => (5, draw, 0),
        RetainedDrawKind::Glass {
            draw,
            lighting_handle,
            ..
        } => (6, draw, lighting_handle),
    };
    LogicalInputKey {
        material_id: draw.material_id,
        material_rank: draw.material_rank,
        packed: draw.key,
        tech: tech.0,
        kind_tag,
        a,
        b,
    }
}

fn fill_vis_in_place(buf: &mut Vec<u8>, n: usize) {
    if buf.len() != n {
        buf.clear();
        buf.resize(n, 0);
    } else {
        buf.fill(0);
    }
}

fn apply_payload_update(product: &mut FrameProduct, persist: &mut ProductBindPersist) -> bool {
    let merged_len = product.ordered_draws.len();
    if persist.compacted && persist.compact_remap.len() == merged_len {
        let expect = persist.compact_tech.len();
        let mut filled = 0usize;
        for (input, &compact) in persist.compact_remap.iter().enumerate() {
            let slot = compact as usize;
            if slot != filled {
                continue;
            }
            let Some(src) = product.ordered_draws.get(input).copied() else {
                return false;
            };
            let Some(dst) = product.ordered_draws.get_mut(slot) else {
                return false;
            };
            *dst = src;
            filled += 1;
        }
        if filled == expect {
            product.ordered_draws.truncate(persist.compact_tech.len());
            product.draw_tech.clone_from(&persist.compact_tech);
            product.code_sampler_mask = persist.last_mask;
            product.material_refusals.clone_from(&persist.last_refusals);
            product.has_codemesh = persist.last_has_codemesh;
            product.world_pretess_id = persist.last_world_pretess_id;
            product.sun_near_n = persist.last_sun_near_n;
            return true;
        }
    }
    false
}

fn apply_camera_list(
    product: &mut FrameProduct,
    persist: &mut ProductBindPersist,
    generation_id: MaterialGenerationId,
    tech_type: TechType,
    light_types: &[u8],
    dfog: bool,
    remap_lit: bool,
    sun_shadow_map: super::SunShadowReceivers,
    spot_shadowed: &[u8],
    target: ProductTarget,
    catalog: &RuntimeMaterialCatalog,
    prepared: &PreparedMaterialTable,
    composition_digest: u64,
    reuse_compact: bool,
) {
    if reuse_compact && apply_payload_update(product, persist) {
        product.generation_id = generation_id;
        product.list_digest = composition_digest;
        product.status = FrameProductStatus::Ready { tech_type, target };
        return;
    }
    fill_product_list(
        product,
        generation_id,
        tech_type,
        light_types,
        dfog,
        remap_lit,
        sun_shadow_map,
        spot_shadowed,
        target,
        persist,
        catalog,
        prepared,
        composition_digest,
    );
}

fn compact_product_draws(
    product: &mut FrameProduct,
    tech_type: TechType,
    remap_lit: bool,
    light_types: &[u8],
    dfog: bool,
    sun_shadow_map: super::SunShadowReceivers,
    spot_shadowed: &[u8],
    persist: &mut ProductBindPersist,
    catalog: &RuntimeMaterialCatalog,
    prepared: &PreparedMaterialTable,
) {
    persist.compact_seen.clear();
    persist.compact_seen.reserve(product.ordered_draws.len());
    persist.compact_remap.clear();
    persist.compact_remap.reserve(product.ordered_draws.len());
    product.draw_tech.clear();
    product.draw_tech.reserve(product.ordered_draws.len());
    product.code_sampler_mask = 0;
    product.material_refusals.clear();
    product.has_codemesh = false;

    let input_len = product.ordered_draws.len();
    let input_near_n = product.sun_near_n.min(input_len);
    let split_sun = input_near_n > 0 && input_near_n < input_len;
    let mut output_near_n = input_near_n;
    let mut output = 0;
    for input in 0..input_len {
        if split_sun && input == input_near_n {
            output_near_n = output;
            persist.compact_seen.clear();
        }
        let draw = product.ordered_draws[input];
        let draw_tech = if product.kind == FrameProductKind::Light {
            let index = dpvs_iw4::GfxDrawSurf { packed: draw.key }.scene_light_index();
            let light_type = light_types.get(usize::from(index)).copied().unwrap_or(0);
            TechType(lighting_iw4::additional_light_tech_type(
                light_type,
                spot_shadowed.contains(&index),
            ))
        } else if remap_lit
            && tech_type.0 == lighting_iw4::GFX_DRAW_METHOD_LIT_BEGIN
            && render_material::resolve_sorted_material(
                catalog,
                render_material::MaterialDrawKey::new(draw.key, draw.material_rank)
                    .with_material_id(draw.material_id),
            )
            .is_ok_and(|material| material.draw_rules.unlit_sky)
        {
            TechType(asset_iw4::TECHNIQUE_UNLIT as u8 + u8::from(dfog))
        } else if remap_lit {
            super::colour_lit_technique(
                tech_type,
                draw.key,
                light_types,
                dfog,
                sun_shadow_map,
                spot_shadowed,
            )
        } else {
            tech_type
        };
        let key = logical_input_key(&draw, draw_tech);
        if let Some(&compact) = persist.compact_seen.get(&key) {
            persist.compact_remap.push(compact);
            continue;
        }
        let material_key = render_material::MaterialDrawKey::new(draw.key, draw.material_rank)
            .with_material_id(draw.material_id);
        let mask = match super::draw_code_sampler_mask(catalog, prepared, material_key, draw_tech) {
            Ok(mask) => mask,
            Err(cause) => {
                product
                    .material_refusals
                    .push(render_frame::FrameMaterialRefusal {
                        key: material_key,
                        tech_type: draw_tech,
                        cause,
                    });
                persist.compact_seen.insert(key, u32::MAX);
                persist.compact_remap.push(u32::MAX);
                continue;
            }
        };
        let compact = u32::try_from(output).unwrap_or(u32::MAX);
        persist.compact_seen.insert(key, compact);
        persist.compact_remap.push(compact);
        product.ordered_draws[output] = draw;
        product.draw_tech.push(draw_tech);
        product.code_sampler_mask |= mask;
        product.has_codemesh |= matches!(draw.kind, RetainedDrawKind::CodeMesh { .. });
        output += 1;
    }
    product.ordered_draws.truncate(output);
    if input_near_n > 0 {
        product.sun_near_n = if split_sun { output_near_n } else { output };
    }
}

pub(crate) fn frame_product_kind_for_camera_region(region: Option<u8>) -> Option<FrameProductKind> {
    match region {
        Some(asset_iw4::CAMERA_REGION_EMISSIVE) => Some(FrameProductKind::Emissive),
        Some(asset_iw4::CAMERA_REGION_NONE) => None,
        Some(asset_iw4::CAMERA_REGION_LIT_OPAQUE)
        | Some(asset_iw4::CAMERA_REGION_LIT_TRANS)
        | Some(asset_iw4::CAMERA_REGION_DEPTH_HACK)
        | None => Some(FrameProductKind::Colour),
        Some(_) => Some(FrameProductKind::Colour),
    }
}

fn merge_draw_lanes(out: &mut Vec<RetainedDrawItem>, lanes: [&[RetainedDrawItem]; 3]) {
    out.clear();
    out.reserve(lanes.iter().map(|lane| lane.len()).sum());
    let mut cursors = [0usize; 3];
    while let Some(lane_index) = next_merge_lane(lanes, &cursors) {
        out.push(lanes[lane_index][cursors[lane_index]]);
        cursors[lane_index] += 1;
    }
}

fn next_merge_lane(lanes: [&[RetainedDrawItem]; 3], cursors: &[usize; 3]) -> Option<usize> {
    let mut next: Option<(usize, (u64, u32))> = None;
    for (lane_index, lane) in lanes.iter().enumerate() {
        let Some(item) = lane.get(cursors[lane_index]) else {
            continue;
        };
        let key = (
            item.key,
            super::retained_list::retained_draw_order_tie(&item.kind),
        );
        if next.is_none_or(|(_, best)| key < best) {
            next = Some((lane_index, key));
        }
    }
    next.map(|(lane_index, _)| lane_index)
}

fn overlay_compact_from_lanes(
    dest: &mut [RetainedDrawItem],
    compact_remap: &[u32],
    lanes: [&[RetainedDrawItem]; 3],
    keep: impl Fn(&RetainedDrawItem) -> bool,
) -> bool {
    let expect = dest.len();
    let mut cursors = [0usize; 3];
    let mut filled = 0usize;
    let mut remap = compact_remap.iter();
    while let Some(lane_index) = next_merge_lane(lanes, &cursors) {
        let item = lanes[lane_index][cursors[lane_index]];
        cursors[lane_index] += 1;
        if !keep(&item) {
            continue;
        }
        let Some(&compact) = remap.next() else {
            return false;
        };
        let Ok(slot) = usize::try_from(compact) else {
            return false;
        };
        if slot != filled {
            continue;
        }
        let Some(dst) = dest.get_mut(filled) else {
            return false;
        };
        *dst = item;
        filled += 1;
    }
    remap.next().is_none() && filled == expect
}

fn colour_list_keep(
    draw: &RetainedDrawItem,
    base: TechType,
    remap_lit: bool,
    light_types: &[u8],
    dfog: bool,
    sun_shadow_map: super::SunShadowReceivers,
    spot_shadowed: &[u8],
    catalog: &RuntimeMaterialCatalog,
) -> bool {
    let tech = if remap_lit {
        super::colour_lit_technique(
            base,
            draw.key,
            light_types,
            dfog,
            sun_shadow_map,
            spot_shadowed,
        )
    } else {
        base
    };
    !super::material_runtime::resolve_material_technique(
        catalog,
        render_material::MaterialDrawKey::new(draw.key, draw.material_rank)
            .with_material_id(draw.material_id),
        tech,
    )
    .is_ok_and(|(_, technique)| technique.flags & 1 != 0)
}

fn overlay_filter_from_lanes(
    dest: &mut [RetainedDrawItem],
    lanes: [&[RetainedDrawItem]; 3],
    keep: impl Fn(&RetainedDrawItem) -> bool,
) -> bool {
    let expect = dest.len();
    let mut filled = 0usize;
    let mut cursors = [0usize; 3];
    while let Some(lane_index) = next_merge_lane(lanes, &cursors) {
        let item = lanes[lane_index][cursors[lane_index]];
        cursors[lane_index] += 1;
        if keep(&item) {
            let Some(dst) = dest.get_mut(filled) else {
                return false;
            };
            *dst = item;
            filled += 1;
        }
    }
    filled == expect
}

fn distortion_sort_keep(draw: &RetainedDrawItem, sort_key: u32) -> bool {
    u32::from(dpvs_iw4::unpack(dpvs_iw4::GfxDrawSurf { packed: draw.key }).primary_sort_key)
        == sort_key
}

fn commit_compacted_payload(
    product: &mut FrameProduct,
    persist: &mut ProductBindPersist,
    generation_id: MaterialGenerationId,
    composition_digest: u64,
    tech_type: TechType,
    target: ProductTarget,
) {
    product.ordered_draws.truncate(persist.compact_tech.len());
    product.draw_tech.clone_from(&persist.compact_tech);
    product.code_sampler_mask = persist.last_mask;
    product.material_refusals.clone_from(&persist.last_refusals);
    product.has_codemesh = persist.last_has_codemesh;
    product.world_pretess_id = persist.last_world_pretess_id;
    product.sun_near_n = persist.last_sun_near_n;
    product.generation_id = generation_id;
    product.list_digest = composition_digest;
    product.status = FrameProductStatus::Ready { tech_type, target };
}

fn fill_product_list(
    product: &mut FrameProduct,
    generation_id: MaterialGenerationId,
    tech_type: TechType,
    light_types: &[u8],
    dfog: bool,
    remap_lit: bool,
    sun_shadow_map: super::SunShadowReceivers,
    spot_shadowed: &[u8],
    target: ProductTarget,
    persist: &mut ProductBindPersist,
    catalog: &RuntimeMaterialCatalog,
    prepared: &PreparedMaterialTable,
    composition_digest: u64,
) {
    product.generation_id = generation_id;
    product.list_digest = composition_digest;
    compact_product_draws(
        product,
        tech_type,
        remap_lit,
        light_types,
        dfog,
        sun_shadow_map,
        spot_shadowed,
        persist,
        catalog,
        prepared,
    );
    product.world_pretess_id = world_pretess_id(product);
    persist.remember_compacted(product);
    product.status = FrameProductStatus::Ready { tech_type, target };
}

#[derive(Resource, Debug, Default)]
pub struct FrameAssemblyInputs {
    pub frame_id: u64,

    pub world_generation: frame::WorldGeneration,

    pub catalog_generation: MaterialGenerationId,
    pub inv_image_height: Option<f32>,
    pub dfog: bool,

    pub primary_lights: Vec<lighting_iw4::GfxLightPack>,

    pub map_light_n: usize,
    pub attenuation: Vec<LightAttenuationBind>,
    pub local_light_bindings: Vec<render_material::CompiledConstantOverlay>,
    pub reflection_probe_sh: Vec<Option<[[f32; 4]; 3]>>,
    pub smodel_lighting_sh: std::sync::Arc<Vec<Option<[[f32; 4]; 3]>>>,
}

pub(crate) fn open_frame_products(
    generation: Res<MaterialGeneration>,
    world_generation: Option<Res<frame::WorldGeneration>>,
    lighting: Option<Res<WorldModelLightingAtlas>>,
    dfog: Option<Res<super::DrawMethodDfog>>,
    primary_lights: Option<Res<super::MapPrimaryLights>>,
    smodel_plan: Option<Res<super::tess::smodel::SmodelGpuPlan>>,
    fx_dlights: Option<Res<render_fx::HostFxDlights>>,
    prepared: Option<Res<crate::prepare::scene::view_parms::PreparedSceneView>>,
    mut inputs: ResMut<FrameAssemblyInputs>,
    mut spot_casters: ResMut<super::SpotShadowCasterPlan>,
    mut spot_lights: ResMut<super::SpotShadowMapLights>,
    mut mat_frame: ResMut<MaterialFrameInputs>,
) {
    spot_casters.clear_frame();
    spot_lights.0.clear();
    mat_frame.spot_receivers.clear();
    let inputs = inputs.as_mut();
    inputs.frame_id = inputs.frame_id.wrapping_add(1);
    inputs.world_generation = world_generation
        .map(|generation| *generation)
        .unwrap_or_default();
    inputs.catalog_generation = generation.catalog.generation_id();
    inputs.inv_image_height = lighting.as_ref().and_then(|lighting| {
        lighting_iw4::model_lighting_inv_image_height(lighting.dims.image_height)
    });
    inputs.dfog = dfog.map(|flag| flag.0).unwrap_or(false);
    inputs.primary_lights.clear();
    inputs.attenuation.clear();
    inputs.local_light_bindings.clear();
    inputs.reflection_probe_sh.clear();
    inputs.smodel_lighting_sh = smodel_plan
        .map(|plan| plan.lighting_sh.clone())
        .unwrap_or_default();
    if let Some(map) = primary_lights.as_ref() {
        inputs
            .reflection_probe_sh
            .extend_from_slice(&map.reflection_probe_sh);
        inputs.primary_lights.extend_from_slice(&map.lights);
        inputs.attenuation.extend_from_slice(&map.attenuation);
    }
    inputs.map_light_n = inputs.primary_lights.len();
    if let (Some(fx), Some(dynamic)) = (
        fx_dlights.as_ref(),
        primary_lights.as_ref().and_then(|map| map.dynamic),
    ) {
        let view = prepared
            .as_ref()
            .map(|view| view.eye.to_array())
            .unwrap_or([0.0; 3]);
        let slots: Vec<lighting_iw4::SceneDlight> = fx
            .scene
            .iter()
            .copied()
            .map(|light| lighting_iw4::SceneDlight { light, used: false })
            .collect();
        let mut extra = [lighting_iw4::omni_light_pack([0.0; 3], 1.0, [0.0; 3]);
            lighting_iw4::R_DLIGHT_BACKEND_MAX];
        let planes = prepared.as_ref().and_then(|view| {
            if view.ready && !view.frustum_planes.is_empty() {
                Some(view.frustum_planes.as_slice())
            } else {
                None
            }
        });
        let n = lighting_iw4::append_scene_dlights_to_backend(
            &slots,
            view,
            lighting_iw4::R_DLIGHT_LIMIT_DEFAULT,
            true,
            planes,
            &mut extra,
        );
        for mut light in extra.iter().take(n).copied() {
            light.falloff_image_width = dynamic.falloff_image_width;
            light.lmap_lookup_start = dynamic.lmap_lookup_start;
            inputs.primary_lights.push(light);
            inputs.attenuation.push(dynamic.attenuation);
        }
    }
    let generation_id = inputs.catalog_generation;
    if mat_frame
        .material_bindings
        .as_ref()
        .is_none_or(|bindings| bindings.generation_id() != generation_id)
    {
        mat_frame.material_bindings = Some(asset_material::compile_material_bindings(
            &generation.catalog,
        ));
    }
    let bindings = mat_frame
        .material_bindings
        .as_ref()
        .expect("current material bindings");
    for (index, light) in inputs.primary_lights.iter().enumerate() {
        let pack = primary_lights
            .as_ref()
            .and_then(|map| map.overrides.get(index))
            .copied()
            .unwrap_or_default();
        let light_inputs = asset_material::MaterialLocalLightInputs {
            light_type: light.light_type,
            direction: light.direction,
            origin: light.origin,
            radius: light.radius,
            cos_outer: light.cos_outer,
            overrides: pack,
        };
        inputs.local_light_bindings.push(
            bindings
                .prepare_local_light(
                    &generation.catalog,
                    (index != 0).then_some(&light_inputs),
                    mat_frame.view_origin,
                    mat_frame.float_time,
                )
                .expect("local-light generation correlated"),
        );
    }
}

fn missing_product(kind: FrameProductKind) -> FrameProduct {
    FrameProductsSnapshot::empty()
        .products
        .into_iter()
        .find(|product| product.kind == kind)
        .expect("declared frame product kind")
}

const COLOUR_MISSING: MissingProductCause = MissingProductCause::MissingFrontendEmitter;
const SUN_MISSING: MissingProductCause = MissingProductCause::MissingSunShadowProducer;
const SPOT_MISSING: MissingProductCause = MissingProductCause::MissingSpotShadowProducer;

#[derive(Resource, Debug)]
pub struct CameraProducts {
    colour: FrameProduct,
    emissive: FrameProduct,
    distortion: FrameProduct,
    light: FrameProduct,
    colour_persist: ProductBindPersist,
    emissive_persist: ProductBindPersist,
    light_persist: ProductBindPersist,
    last_fill: Option<CameraFillStamp>,
}

impl Default for CameraProducts {
    fn default() -> Self {
        Self {
            colour: missing_product(FrameProductKind::Colour),
            emissive: missing_product(FrameProductKind::Emissive),
            distortion: missing_product(FrameProductKind::Distortion),
            light: missing_product(FrameProductKind::Light),
            colour_persist: ProductBindPersist::default(),
            emissive_persist: ProductBindPersist::default(),
            light_persist: ProductBindPersist::default(),
            last_fill: None,
        }
    }
}

#[derive(Resource, Debug)]
pub struct SunProduct {
    product: FrameProduct,
    persist: ProductBindPersist,
    last_stamp: Option<SunPackStamp>,
    last_packed: Option<Arc<[PackedFrontendLists; 2]>>,
}

impl Default for SunProduct {
    fn default() -> Self {
        Self {
            product: missing_product(FrameProductKind::SunShadow),
            persist: ProductBindPersist::default(),
            last_stamp: None,
            last_packed: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SunPackStamp {
    membership: u64,
    catalog: u64,
    world_generation: Option<u64>,
    world_verts: u32,
    world_range_n: usize,
    smodel_range_n: usize,
    xmodel_topology: u64,
    xmodel_range_n: usize,
}

impl SunPackStamp {
    fn digest(self) -> u64 {
        let mut id = super::list::CONTENT_ID_SEED;
        super::list::mix_content_id(&mut id, self.membership);
        super::list::mix_content_id(&mut id, self.catalog);
        super::list::mix_content_id(
            &mut id,
            self.world_generation.map_or(0, |g| g.wrapping_add(1)),
        );
        super::list::mix_content_id(&mut id, u64::from(self.world_verts));
        super::list::mix_content_id(&mut id, self.world_range_n as u64);
        super::list::mix_content_id(&mut id, self.smodel_range_n as u64);
        super::list::mix_content_id(&mut id, self.xmodel_topology);
        super::list::mix_content_id(&mut id, self.xmodel_range_n as u64);
        id
    }
}

fn caster_membership(draws: &[RetainedDrawItem], sun_near_n: usize) -> u64 {
    let mut id = super::list::CONTENT_ID_SEED;
    super::list::mix_content_id(&mut id, sun_near_n as u64);
    super::list::mix_content_id(&mut id, draws.len() as u64);
    for item in draws {
        super::retained_list::mix_draw_membership(&mut id, item);
    }
    id
}

fn try_reuse_sun_pack(
    product: &mut FrameProduct,
    persist: &mut ProductBindPersist,
    last_stamp: Option<SunPackStamp>,
    stamp: SunPackStamp,
    last_packed: &Option<Arc<[PackedFrontendLists; 2]>>,
    generation_id: MaterialGenerationId,
) -> bool {
    if last_stamp != Some(stamp) {
        return false;
    }
    let Some(packed) = last_packed.clone() else {
        return false;
    };
    if !apply_payload_update(product, persist) {
        return false;
    }
    product.generation_id = generation_id;
    product.list_digest = stamp.digest();
    product.sun_packed = Some(packed);
    product.status = FrameProductStatus::Ready {
        tech_type: TechType(super::SUN_SHADOW_CASTER_TECH),
        target: ProductTarget::SunShadowFallbackAtlas,
    };
    true
}

#[derive(Resource, Debug)]
pub struct SpotProduct {
    product: FrameProduct,
    persist: ProductBindPersist,
    last_stamp: Option<SpotFillStamp>,
    last_pack_id: Option<u64>,
    last_packed: Vec<Arc<PackedFrontendLists>>,
}

impl Default for SpotProduct {
    fn default() -> Self {
        Self {
            product: missing_product(FrameProductKind::SpotShadow),
            persist: ProductBindPersist::default(),
            last_stamp: None,
            last_pack_id: None,
            last_packed: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SpotFillStamp {
    membership: u64,
    catalog: u64,
    slot_n: usize,
}

impl SpotFillStamp {
    fn digest(self) -> u64 {
        let mut id = super::list::CONTENT_ID_SEED;
        super::list::mix_content_id(&mut id, self.membership);
        super::list::mix_content_id(&mut id, self.catalog);
        super::list::mix_content_id(&mut id, self.slot_n as u64);
        id
    }
}

fn try_reuse_spot_compact(
    product: &mut FrameProduct,
    persist: &mut ProductBindPersist,
    last_stamp: Option<SpotFillStamp>,
    stamp: SpotFillStamp,
    generation_id: MaterialGenerationId,
) -> bool {
    if last_stamp != Some(stamp) {
        return false;
    }
    if !apply_payload_update(product, persist) {
        return false;
    }
    product.generation_id = generation_id;
    product.list_digest = stamp.digest();
    product.status = FrameProductStatus::Ready {
        tech_type: TechType(super::SUN_SHADOW_CASTER_TECH),
        target: ProductTarget::SpotShadowMaps,
    };
    true
}

#[derive(Resource, Default)]
pub(crate) struct StaticSunCasters {
    partitions: Option<[super::SunShadowCasterPlan; 2]>,
    visibility_counts: [usize; 4],
}

pub(crate) fn bake_static_sun_shadow_casters(
    generation: Res<MaterialGeneration>,
    frame: Res<MaterialFrameInputs>,
    scene: Option<Res<crate::prepare::scene::world::WorldScene>>,
    world_plan: Option<Res<WorldDrawGpuPlan>>,
    smodel_plan: (
        Option<Res<super::SmodelGpuPlan>>,
        Res<crate::prepare::scene::smodel_geom_cache::LodRampDvar>,
        Option<Res<crate::prepare::scene::smodel_geom_cache::WorldStaticModelCache>>,
        Res<crate::prepare::scene::smodel_geom_cache::PretessDvar>,
        Option<Res<crate::prepare::scene::smodel_lighting::WorldSmodelLighting>>,
    ),
    mut casters: ResMut<super::SunShadowCasterPlan>,
    mut staged: ResMut<StaticSunCasters>,
    mut sun_staging: Local<super::SunShadowStaging>,
) {
    let _static = perf::Span::HostStaticSunMs.enter();
    let (smodel_plan, lod_ramp, smc_cache, pretess, smodel_lighting) = smodel_plan;
    let camera_view = frame.view_from_world;
    staged.partitions = None;
    let super::SunShadowStaging {
        surface_vis: sun_surface_vis,
        smodel_vis: sun_smodel_vis,
        frustum_draw_msb: sun_frustum_draw_msb,
        draw_cell_n: sun_draw_cell_n,
    } = &mut *sun_staging;
    let (Some(sun_frame), Some(scene), Some(world_plan), Some(smodel_plan)) = (
        frame.sun_shadow,
        scene.as_deref(),
        world_plan.as_deref(),
        smodel_plan.as_deref(),
    ) else {
        return;
    };
    if let Some(cull) = scene.cull.as_ref() {
        let surf_n = cull.surface_materials.len();
        let smodel_n = cull.smodel_vis.len().max(cull.dpvs.smodel_bounds.len());
        fill_vis_in_place(&mut sun_surface_vis[0], surf_n);
        fill_vis_in_place(&mut sun_surface_vis[1], surf_n);
        fill_vis_in_place(&mut sun_smodel_vis[0], smodel_n);
        fill_vis_in_place(&mut sun_smodel_vis[1], smodel_n);
        *sun_draw_cell_n = 0;
        let planes0 = sun_frame.partitions[0].clip_planes;
        let planes1 = sun_frame.partitions[1].clip_planes;
        let dpvs = &cull.dpvs;
        let cell_vis = cull.cell_vis.as_slice();
        let cell_vis_all = cull.cell_vis_all;
        let (surf_near, surf_far) = sun_surface_vis.split_at_mut(1);
        let (smodel_near, smodel_far) = sun_smodel_vis.split_at_mut(1);
        let (msb_near, msb_far) = sun_frustum_draw_msb.split_at_mut(1);

        let lod_origin_eye = camera_view.map(|view| view.inverse().transform_point3(Vec3::ZERO));
        let buckets = super::retained_list::SmodelBucketBakeSrc {
            pretess_enable: pretess.enabled,
            cache: smc_cache.as_deref(),
            lighting: smodel_lighting.as_deref(),
        };
        let mut bsp_ids = std::mem::take(&mut casters.bsp_ids);
        let mut smodel_ids = std::mem::take(&mut casters.smodel_ids);
        let mut bsp_ids_far = std::mem::take(&mut casters.bsp_ids_far);
        let mut smodel_ids_far = std::mem::take(&mut casters.smodel_ids_far);
        let catalog = &generation.catalog;
        let ramp = lod_ramp.args();

        // Each partition owns its visibility -> LOD -> bake chain. Only the
        // final merge needs both results; intermediate joins serialize the chains.
        let [(near, draw0), (far, draw1)] = ComputeTaskPool::get()
            .scope(|scope| {
                for (planes, surface_vis, smodel_vis, frustum_msb, bsp_ids, smodel_ids) in [
                    (
                        planes0.as_slice(),
                        &mut surf_near[0],
                        &mut smodel_near[0],
                        &mut msb_near[0],
                        &mut bsp_ids,
                        &mut smodel_ids,
                    ),
                    (
                        planes1.as_slice(),
                        &mut surf_far[0],
                        &mut smodel_far[0],
                        &mut msb_far[0],
                        &mut bsp_ids_far,
                        &mut smodel_ids_far,
                    ),
                ] {
                    scope.spawn(async move {
                        let (_, draw_cells) =
                            crate::prepare::scene::cull::add_world_surfaces_frustum_only(
                                dpvs,
                                planes,
                                cell_vis,
                                cell_vis_all,
                                surface_vis,
                                smodel_vis,
                                frustum_msb,
                            );
                        if let Some(eye) = lod_origin_eye {
                            dpvs_iw4::cull_smodel_sun_shadow_vis(
                                smodel_vis,
                                &smodel_plan.shadow_draw_insts,
                                eye.to_array(),
                                ramp.scale_last,
                            );
                        }
                        let plan = super::retained_list::bake_sun_shadow_caster_plan(
                            cull,
                            &smodel_plan.shadow_draw_insts,
                            world_plan,
                            smodel_plan,
                            catalog,
                            Some(surface_vis),
                            Some(smodel_vis),
                            lod_origin_eye,
                            ramp,
                            buckets,
                            bsp_ids,
                            smodel_ids,
                        );
                        (plan, draw_cells)
                    });
                }
            })
            .try_into()
            .expect("two sun partitions");
        *sun_draw_cell_n = draw0.max(draw1);
        casters.bsp_ids = bsp_ids;
        casters.smodel_ids = smodel_ids;
        casters.bsp_ids_far = bsp_ids_far;
        casters.smodel_ids_far = smodel_ids_far;
        if casters.generation_id != generation.catalog.generation_id() {
            staged.visibility_counts = [
                sun_surface_vis[0].iter().filter(|&&b| b != 0).count(),
                sun_surface_vis[1].iter().filter(|&&b| b != 0).count(),
                sun_smodel_vis[0].iter().filter(|&&b| b != 0).count(),
                sun_smodel_vis[1].iter().filter(|&&b| b != 0).count(),
            ];
        }
        staged.partitions = Some([near, far]);
    }
}

pub(crate) fn bake_sun_shadow_casters(
    inputs: Res<FrameAssemblyInputs>,
    generation: Res<MaterialGeneration>,
    frame: Res<MaterialFrameInputs>,
    xmodel_plan: Option<Res<super::XModelDrawPlan>>,
    mut staged: ResMut<StaticSunCasters>,
    mut casters: ResMut<super::SunShadowCasterPlan>,
    mut present: ResMut<super::SunShadowMapPresent>,
) {
    present.0 = false;
    let Some([mut near, mut far]) = staged.partitions.take() else {
        return;
    };
    let Some(sun_frame) = frame.sun_shadow else {
        return;
    };
    let planes0 = sun_frame.partitions[0].clip_planes;
    let planes1 = sun_frame.partitions[1].clip_planes;
    let gen_changed = casters.generation_id != inputs.catalog_generation;
    near.bsp_ids = std::mem::take(&mut casters.bsp_ids);
    near.smodel_ids = std::mem::take(&mut casters.smodel_ids);
    near.bsp_ids_far = std::mem::take(&mut casters.bsp_ids_far);
    near.smodel_ids_far = std::mem::take(&mut casters.smodel_ids_far);
    let (mut ordered0, ordered1) = super::retained_list::merge_sun_shadow_caster_partitions(
        &mut near,
        &mut far,
        xmodel_plan.as_deref(),
        &generation.catalog,
        [planes0.as_slice(), planes1.as_slice()],
    );
    let sun_near_n = ordered0.len();
    ordered0.extend(ordered1);
    *casters = near;
    casters.sun_near_n = sun_near_n;
    casters.items = ordered0;
    casters.world_eligible = casters.world_eligible.saturating_add(far.world_eligible);
    casters.world_missing_key = casters
        .world_missing_key
        .saturating_add(far.world_missing_key);
    casters.smodel_eligible = casters.smodel_eligible.saturating_add(far.smodel_eligible);
    casters.smodel_excluded = casters.smodel_excluded.saturating_add(far.smodel_excluded);
    casters.smodel_missing_key = casters
        .smodel_missing_key
        .saturating_add(far.smodel_missing_key);
    casters.xmodel_eligible = casters.xmodel_eligible.saturating_add(far.xmodel_eligible);
    casters.xmodel_skipped_viewmodel = casters
        .xmodel_skipped_viewmodel
        .saturating_add(far.xmodel_skipped_viewmodel);
    casters.xmodel_missing_key = casters
        .xmodel_missing_key
        .saturating_add(far.xmodel_missing_key);
    casters.smodel_bucket_flush_n = casters
        .smodel_bucket_flush_n
        .saturating_add(far.smodel_bucket_flush_n);
    casters.smodel_bucket_rigid_n = casters
        .smodel_bucket_rigid_n
        .saturating_add(far.smodel_bucket_rigid_n);
    casters.smodel_bucket_skinned_n = casters
        .smodel_bucket_skinned_n
        .saturating_add(far.smodel_bucket_skinned_n);
    casters.smodel_bucket_cached_n = casters
        .smodel_bucket_cached_n
        .saturating_add(far.smodel_bucket_cached_n);
    casters.smodel_bucket_unread_n = casters
        .smodel_bucket_unread_n
        .saturating_add(far.smodel_bucket_unread_n);
    casters.smodel_bucket_consume_n = casters
        .smodel_bucket_consume_n
        .saturating_add(far.smodel_bucket_consume_n);
    casters.smodel_bucket_context_refused_n = casters
        .smodel_bucket_context_refused_n
        .saturating_add(far.smodel_bucket_context_refused_n);
    casters.cutout_plus23 = casters.cutout_plus23.saturating_add(far.cutout_plus23);
    casters.cutout_missing_key = casters
        .cutout_missing_key
        .saturating_add(far.cutout_missing_key);
    casters.cutout_empty_ib = casters.cutout_empty_ib.saturating_add(far.cutout_empty_ib);
    casters.cutout_custom0 = casters.cutout_custom0.saturating_add(far.cutout_custom0);
    casters
        .smodel_pretess_indices
        .extend(far.smodel_pretess_indices);

    if gen_changed {
        diag::info!(
            World,
            "sun-shadow casters: world eligible={} missing_key={} smodel eligible={} excluded={} missing_key={} drawn_items={} vis0={} vis1={} smodel_vis0={} smodel_vis1={} near_n={}",
            casters.world_eligible,
            casters.world_missing_key,
            casters.smodel_eligible,
            casters.smodel_excluded,
            casters.smodel_missing_key,
            casters.items.len(),
            staged.visibility_counts[0],
            staged.visibility_counts[1],
            staged.visibility_counts[2],
            staged.visibility_counts[3],
            casters.sun_near_n,
        );
    }

    present.0 = !casters.items.is_empty();
}

pub(crate) fn execute_sun_product(
    inputs: Res<FrameAssemblyInputs>,
    generation: Res<MaterialGeneration>,
    frame: Res<MaterialFrameInputs>,
    retained: Res<StaticDrawLane>,
    lights: Option<Res<super::MapPrimaryLightTypes>>,
    world_plan: Option<Res<WorldDrawGpuPlan>>,
    xmodel_plan: Option<Res<super::XModelDrawPlan>>,
    smodel_plan: Option<Res<super::SmodelGpuPlan>>,
    present: Res<super::SunShadowMapPresent>,
    mut casters: ResMut<super::SunShadowCasterPlan>,
    mut owner: ResMut<SunProduct>,
) {
    let SunProduct {
        product,
        persist,
        last_stamp,
        last_packed,
    } = owner.as_mut();
    product.begin_fill(SUN_MISSING);
    persist.prepare(inputs.catalog_generation);
    if !present.0 {
        return;
    }
    let light_types = lights
        .as_ref()
        .map(|lights| lights.types.as_slice())
        .unwrap_or(&[]);
    let (_, _, _, _, world_run_surfs) = retained.live_for(inputs.world_generation);

    let (Some(_), Some(world_plan), Some(smodel_plan)) = (
        frame.sun_shadow,
        world_plan.as_deref(),
        smodel_plan.as_deref(),
    ) else {
        return;
    };
    if casters.items.is_empty() {
        return;
    }
    let xmodel_ranges = xmodel_plan
        .as_ref()
        .map(|plan| plan.range_rows())
        .unwrap_or(&[]);
    let world_ranges = world_plan.surface_ranges();
    let world_verts = world_plan.decoded_vertices().len() as u32;
    let smodel_ranges = smodel_plan.surface_ranges();
    let xmodel_topology = xmodel_plan
        .as_ref()
        .map(|plan| plan.topology_revision)
        .unwrap_or(0);

    product.ordered_draws = std::mem::take(&mut casters.items);
    product.sun_near_n = casters.sun_near_n;
    let stamp = SunPackStamp {
        membership: caster_membership(&product.ordered_draws, product.sun_near_n),
        catalog: inputs.catalog_generation.get(),
        world_generation: inputs.world_generation.0,
        world_verts,
        world_range_n: world_ranges.len(),
        smodel_range_n: smodel_ranges.len(),
        xmodel_topology,
        xmodel_range_n: xmodel_ranges.len(),
    };
    if try_reuse_sun_pack(
        product,
        persist,
        *last_stamp,
        stamp,
        last_packed,
        retained.generation_id,
    ) {
        return;
    }
    fill_product_list(
        product,
        retained.generation_id,
        TechType(super::SUN_SHADOW_CASTER_TECH),
        light_types,
        inputs.dfog,
        false,
        super::SunShadowReceivers::NONE,
        &[],
        ProductTarget::SunShadowFallbackAtlas,
        persist,
        &generation.catalog,
        &generation.prepared,
        stamp.digest(),
    );
    {
        let sun = &mut *product;
        let near_n = sun.sun_near_n.min(sun.ordered_draws.len());
        let draws = sun.ordered_draws.as_slice();

        let [packed0, packed1] = ComputeTaskPool::get()
            .scope(|scope| {
                scope.spawn(async {
                    let packed_draws = draws[..near_n]
                        .iter()
                        .map(super::retained_list::pack_draw)
                        .collect::<Vec<_>>();
                    crate::pack_sun_shadow_frontend(
                        &packed_draws,
                        world_run_surfs,
                        world_ranges,
                        world_verts,
                        smodel_ranges,
                        xmodel_ranges,
                    )
                });
                scope.spawn(async {
                    let packed_draws = draws[near_n..]
                        .iter()
                        .map(super::retained_list::pack_draw)
                        .collect::<Vec<_>>();
                    crate::pack_sun_shadow_frontend(
                        &packed_draws,
                        world_run_surfs,
                        world_ranges,
                        world_verts,
                        smodel_ranges,
                        xmodel_ranges,
                    )
                });
            })
            .try_into()
            .expect("two sun pack partitions");
        sun.sun_packed = Some(Arc::new([packed0, packed1]));
    }
    *last_packed = product.sun_packed.clone();
    *last_stamp = Some(stamp);
}

pub(crate) fn bake_spot_shadow_casters(
    inputs: (Res<FrameAssemblyInputs>, Res<MaterialGeneration>),
    retained: Res<StaticDrawLane>,
    scene: Option<Res<crate::prepare::scene::world::WorldScene>>,
    world_plan: Option<Res<WorldDrawGpuPlan>>,
    xmodel_plan: Option<Res<super::XModelDrawPlan>>,
    smodel_plan: (
        Option<Res<super::SmodelGpuPlan>>,
        Res<crate::prepare::scene::smodel_geom_cache::LodRampDvar>,
    ),
    spot_occ: Res<crate::prepare::scene::gfx_scene::SpotShadowSceneOccupancy>,
    host_gfx: Option<Res<crate::prepare::scene::gfx_scene::HostGfxScene>>,
    prepared: Option<Res<crate::prepare::scene::view_parms::PreparedSceneView>>,
    sm: (
        Res<crate::prepare::scene::view_parms::SmEnableDvar>,
        Res<crate::prepare::scene::view_parms::SmSunEnableDvar>,
    ),
    mut spot_casters: ResMut<super::SpotShadowCasterPlan>,
    mut spot_lights: ResMut<super::SpotShadowMapLights>,
    mut mat_frame: ResMut<MaterialFrameInputs>,
) {
    let (inputs, generation) = inputs;
    let (smodel_plan, lod_ramp) = smodel_plan;
    let (sm_enable, sm_sun_enable) = sm;
    let Some(world) = scene.as_deref() else {
        spot_casters.clear_frame();
        spot_lights.0.clear();
        mat_frame.spot_receivers.clear();
        return;
    };
    let (retained_items, _, _, _, world_run_surfs) = retained.live_for(inputs.world_generation);
    super::spot_shadow_casters::fill_spot_shadow_caster_plan(
        &mut spot_casters,
        &spot_occ,
        host_gfx.as_ref().map(|g| &g.scene),
        world,
        prepared.as_deref(),
        xmodel_plan.as_deref(),
        &generation.catalog,
        retained_items,
        world_run_surfs,
        world_plan
            .as_ref()
            .map(|p| p.surface_ranges())
            .unwrap_or(&[]),
        world_plan
            .as_ref()
            .map(|p| p.decoded_vertices().len() as u32)
            .unwrap_or(0),
        smodel_plan
            .as_ref()
            .map(|p| p.surface_ranges())
            .unwrap_or(&[]),
        smodel_plan.as_deref(),
        lod_ramp.args(),
        inputs.frame_id as u32,
        sm_enable.enabled,
        sm_sun_enable.enabled,
    );
    spot_lights.0 = spot_casters.shadowed_light_indices();
    mat_frame.spot_receivers = spot_casters.receivers_by_light_index();
}

pub(crate) fn execute_spot_product(
    inputs: Res<FrameAssemblyInputs>,
    generation: Res<MaterialGeneration>,
    retained: Res<StaticDrawLane>,
    lights: Option<Res<super::MapPrimaryLightTypes>>,
    mut spot_casters: ResMut<super::SpotShadowCasterPlan>,
    mut owner: ResMut<SpotProduct>,
) {
    let SpotProduct {
        product,
        persist,
        last_stamp,
        last_pack_id,
        last_packed,
    } = owner.as_mut();
    product.begin_fill(SPOT_MISSING);
    persist.prepare(inputs.catalog_generation);
    let spot_ready = spot_casters
        .packed
        .iter()
        .any(|packed| lighting_iw4::spot_shadow_packed_lists_ready(packed.work_entry_n()));
    if !spot_ready {
        return;
    }
    let light_types = lights
        .as_ref()
        .map(|lights| lights.types.as_slice())
        .unwrap_or(&[]);

    product.ordered_draws = std::mem::take(&mut spot_casters.items);
    let stamp = SpotFillStamp {
        membership: caster_membership(&product.ordered_draws, 0),
        catalog: inputs.catalog_generation.get(),
        slot_n: spot_casters.packed.len(),
    };
    if try_reuse_spot_compact(product, persist, *last_stamp, stamp, retained.generation_id) {
        product.spot_slots =
            spot_casters.adopt_frame_slots(&persist.compact_remap, last_pack_id, last_packed);
        return;
    }
    fill_product_list(
        product,
        retained.generation_id,
        TechType(super::SUN_SHADOW_CASTER_TECH),
        light_types,
        inputs.dfog,
        false,
        super::SunShadowReceivers::NONE,
        &[],
        ProductTarget::SpotShadowMaps,
        persist,
        &generation.catalog,
        &generation.prepared,
        stamp.digest(),
    );
    product.spot_slots =
        spot_casters.adopt_frame_slots(&persist.compact_remap, last_pack_id, last_packed);
    *last_stamp = Some(stamp);
}

pub(crate) fn execute_camera_products(
    inputs: Res<FrameAssemblyInputs>,
    lanes: (
        Res<StaticDrawLane>,
        Res<super::XModelDrawLane>,
        Res<super::FxDrawLane>,
    ),
    draw_method: Res<ColourDrawMethod>,
    distortion_settings: Res<DistortionSettings>,
    generation: Res<MaterialGeneration>,
    lights: Option<Res<super::MapPrimaryLightTypes>>,
    scene: Option<Res<crate::prepare::scene::world::WorldScene>>,
    gfx: Option<Res<crate::prepare::scene::gfx_scene::HostGfxScene>>,
    sun_present: Res<super::SunShadowMapPresent>,
    sun_unmatched: Res<super::SunShadowUnmatchedLights>,
    spot_lights: Res<super::SpotShadowMapLights>,
    published: Res<RenderFrameProducts>,
    mut owner: ResMut<CameraProducts>,
) {
    let _post_execute = perf::Span::HostPostExecuteMs.enter();
    let (retained, xmodel_lane, fx_lane) = lanes;
    let (_, static_colour, static_emissive, static_distortion, world_run_surfs) =
        retained.live_for(inputs.world_generation);
    let sort_key_distortion = scene
        .as_deref()
        .and_then(|scene| scene.cull.as_ref())
        .and_then(|cull| cull.sort_key_distortion);
    let map_types = lights
        .as_ref()
        .map(|lights| lights.types.as_slice())
        .unwrap_or(&[]);
    let light_types_owned = super::dlight_receivers::combined_light_types(map_types, &inputs);
    let sun_receivers = super::SunShadowReceivers {
        present: sun_present.0,
        unmatched: *sun_unmatched,
    };
    let light_types = map_types;
    let stamp = CameraFillStamp {
        static_membership: if retained.world_generation == inputs.world_generation {
            retained.membership_revision
        } else {
            0
        },
        xmodel_membership: xmodel_lane.membership_revision,
        fx_membership: fx_lane.membership_revision,
        xmodel_payload: xmodel_lane.payload_revision,
        fx_payload: fx_lane.payload_revision,
        catalog: inputs.catalog_generation.get(),
        world_generation: inputs.world_generation.0,
        colour_tech: draw_method.tech_type().0,
        emissive_tech: draw_method.emissive_tech_type().0,
        dfog: inputs.dfog,
        sun_present: sun_receivers,
        distortion_enabled: distortion_settings.enabled,
        distortion_sort_key: sort_key_distortion,
        light_types_id: light_types_id(light_types),
        spot_lights_id: light_types_id(&spot_lights.0),
    };
    let composition_digest = stamp.digest();
    let CameraProducts {
        colour,
        emissive,
        distortion,
        light,
        colour_persist,
        emissive_persist,
        light_persist,
        last_fill,
    } = owner.as_mut();
    let payload_hold =
        last_fill.is_some_and(|prev| prev.composition_eq(stamp) && prev.payload_eq(stamp));
    if payload_hold
        && colour_persist.restore_compacted(colour, published.product(FrameProductKind::Colour))
        && emissive_persist
            .restore_compacted(emissive, published.product(FrameProductKind::Emissive))
    {
        colour.generation_id = retained.generation_id;
        colour.list_digest = composition_digest;
        colour.status = FrameProductStatus::Ready {
            tech_type: draw_method.tech_type(),
            target: ProductTarget::Core3dViewColour,
        };
        emissive.generation_id = retained.generation_id;
        emissive.list_digest = composition_digest;
        emissive.status = FrameProductStatus::Ready {
            tech_type: draw_method.emissive_tech_type(),
            target: ProductTarget::Core3dViewColour,
        };
        distortion.generation_id = inputs.catalog_generation;
        distortion.ordered_draws.clone_from(
            &published
                .product(FrameProductKind::Distortion)
                .ordered_draws,
        );
        distortion.status = published.product(FrameProductKind::Distortion).status;
        fill_dlight_light(
            light,
            light_persist,
            colour,
            &inputs,
            scene.as_deref(),
            gfx.as_deref().map(|g| &g.scene),
            world_run_surfs,
            &spot_lights.0,
            retained.generation_id,
            draw_method.tech_type(),
            light_types_owned.as_slice(),
            inputs.dfog,
            &generation.catalog,
            &generation.prepared,
            composition_digest,
        );
        *last_fill = Some(stamp);
        return;
    }

    colour.begin_fill(COLOUR_MISSING);
    emissive.begin_fill(COLOUR_MISSING);
    distortion.begin_fill(COLOUR_MISSING);
    colour_persist.prepare(inputs.catalog_generation);
    emissive_persist.prepare(inputs.catalog_generation);

    let reuse_compact = last_fill.is_some_and(|prev| prev.composition_eq(stamp));
    let overlay_pair = {
        let colour_keep = |draw: &RetainedDrawItem| {
            distortion_settings.enabled
                || colour_list_keep(
                    draw,
                    draw_method.tech_type(),
                    true,
                    light_types,
                    inputs.dfog,
                    sun_receivers,
                    &spot_lights.0,
                    &generation.catalog,
                )
        };
        let emissive_keep = |draw: &RetainedDrawItem| {
            distortion_settings.enabled
                || colour_list_keep(
                    draw,
                    draw_method.emissive_tech_type(),
                    false,
                    light_types,
                    inputs.dfog,
                    super::SunShadowReceivers::NONE,
                    &[],
                    &generation.catalog,
                )
        };
        let overlay_colour = reuse_compact
            && colour_persist.compacted
            && colour_persist
                .restore_compacted(colour, published.product(FrameProductKind::Colour))
            && overlay_compact_from_lanes(
                &mut colour.ordered_draws,
                &colour_persist.compact_remap,
                [static_colour, &xmodel_lane.colour, &fx_lane.colour],
                colour_keep,
            );
        let overlay_emissive = reuse_compact
            && emissive_persist.compacted
            && emissive_persist
                .restore_compacted(emissive, published.product(FrameProductKind::Emissive))
            && overlay_compact_from_lanes(
                &mut emissive.ordered_draws,
                &emissive_persist.compact_remap,
                [static_emissive, &xmodel_lane.emissive, &fx_lane.emissive],
                emissive_keep,
            );
        overlay_colour && overlay_emissive
    };
    if overlay_pair {
        commit_compacted_payload(
            colour,
            colour_persist,
            retained.generation_id,
            composition_digest,
            draw_method.tech_type(),
            ProductTarget::Core3dViewColour,
        );
        commit_compacted_payload(
            emissive,
            emissive_persist,
            retained.generation_id,
            composition_digest,
            draw_method.emissive_tech_type(),
            ProductTarget::Core3dViewColour,
        );
    } else {
        merge_draw_lanes(
            &mut colour.ordered_draws,
            [static_colour, &xmodel_lane.colour, &fx_lane.colour],
        );
        merge_draw_lanes(
            &mut emissive.ordered_draws,
            [static_emissive, &xmodel_lane.emissive, &fx_lane.emissive],
        );

        if !distortion_settings.enabled {
            for (product, base, remap) in [
                (&mut *colour, draw_method.tech_type(), true),
                (&mut *emissive, draw_method.emissive_tech_type(), false),
            ] {
                product.ordered_draws.retain(|draw| {
                    colour_list_keep(
                        draw,
                        base,
                        remap,
                        light_types,
                        inputs.dfog,
                        if remap {
                            sun_receivers
                        } else {
                            super::SunShadowReceivers::NONE
                        },
                        if remap { spot_lights.0.as_slice() } else { &[] },
                        &generation.catalog,
                    )
                });
            }
        }
        apply_camera_list(
            colour,
            colour_persist,
            retained.generation_id,
            draw_method.tech_type(),
            light_types,
            inputs.dfog,
            true,
            sun_receivers,
            &spot_lights.0,
            ProductTarget::Core3dViewColour,
            &generation.catalog,
            &generation.prepared,
            composition_digest,
            reuse_compact,
        );
        apply_camera_list(
            emissive,
            emissive_persist,
            retained.generation_id,
            draw_method.emissive_tech_type(),
            light_types,
            inputs.dfog,
            false,
            super::SunShadowReceivers::NONE,
            &[],
            ProductTarget::Core3dViewColour,
            &generation.catalog,
            &generation.prepared,
            composition_digest,
            reuse_compact,
        );
    }

    distortion.generation_id = inputs.catalog_generation;
    match sort_key_distortion {
        Some(sort_key) if distortion_settings.enabled => {
            let lanes = [
                static_distortion,
                &xmodel_lane.distortion,
                &fx_lane.distortion,
            ];
            distortion.ordered_draws.clone_from(
                &published
                    .product(FrameProductKind::Distortion)
                    .ordered_draws,
            );
            if !reuse_compact
                || !overlay_filter_from_lanes(&mut distortion.ordered_draws, lanes, |draw| {
                    distortion_sort_keep(draw, sort_key)
                })
            {
                merge_draw_lanes(&mut distortion.ordered_draws, lanes);
                distortion
                    .ordered_draws
                    .retain(|draw| distortion_sort_keep(draw, sort_key));
            }
            distortion.status = FrameProductStatus::ResolveReady {
                target: ProductTarget::ResolvedPostSun,
            };
        }
        Some(_) => {
            distortion.status = FrameProductStatus::ResolveReady {
                target: ProductTarget::ResolvedPostSun,
            };
        }
        None => {
            distortion.status =
                FrameProductStatus::Missing(MissingProductCause::MissingDistortionSortKey);
        }
    }
    fill_dlight_light(
        light,
        light_persist,
        colour,
        &inputs,
        scene.as_deref(),
        gfx.as_deref().map(|g| &g.scene),
        world_run_surfs,
        &spot_lights.0,
        retained.generation_id,
        draw_method.tech_type(),
        light_types_owned.as_slice(),
        inputs.dfog,
        &generation.catalog,
        &generation.prepared,
        composition_digest,
    );
    *last_fill = Some(stamp);
}

fn dlight_list_digest(composition_digest: u64, inputs: &FrameAssemblyInputs) -> u64 {
    let mut digest = composition_digest;
    for (i, light) in inputs
        .primary_lights
        .iter()
        .enumerate()
        .skip(inputs.map_light_n)
    {
        digest ^= u64::from(i as u32).rotate_left(8)
            ^ u64::from(light.origin[0].to_bits())
            ^ u64::from(light.origin[1].to_bits()).rotate_left(11)
            ^ u64::from(light.origin[2].to_bits()).rotate_left(22)
            ^ u64::from(light.radius.to_bits()).rotate_left(3)
            ^ u64::from(light.light_type);
    }
    digest
}

fn fill_dlight_light(
    light: &mut FrameProduct,
    persist: &mut ProductBindPersist,
    colour: &FrameProduct,
    inputs: &FrameAssemblyInputs,
    scene: Option<&crate::prepare::scene::world::WorldScene>,
    gfx: Option<&render_scene::GfxScene>,
    world_run_surfs: &[u16],
    spot_shadowed: &[u8],
    generation_id: MaterialGenerationId,
    tech_type: TechType,
    light_types: &[u8],
    dfog: bool,
    catalog: &RuntimeMaterialCatalog,
    prepared: &PreparedMaterialTable,
    composition_digest: u64,
) {
    light.begin_fill(COLOUR_MISSING);
    persist.prepare(inputs.catalog_generation);
    if inputs.primary_lights.len() <= inputs.map_light_n {
        return;
    }
    for (i, dlight) in inputs
        .primary_lights
        .iter()
        .enumerate()
        .skip(inputs.map_light_n)
    {
        let Ok(index) = u8::try_from(i) else {
            continue;
        };
        for draw in &colour.ordered_draws {
            if super::dlight_receivers::dlight_receiver_keep(
                draw,
                dlight.origin,
                dlight.radius,
                scene,
                gfx,
                world_run_surfs,
            ) {
                let mut row = *draw;
                row.key = super::with_scene_light_index(draw.key, index);
                light.ordered_draws.push(row);
            }
        }
    }
    apply_camera_list(
        light,
        persist,
        generation_id,
        tech_type,
        light_types,
        dfog,
        true,
        super::SunShadowReceivers::NONE,
        spot_shadowed,
        ProductTarget::Core3dViewColour,
        catalog,
        prepared,
        dlight_list_digest(composition_digest, inputs),
        false,
    );
}

pub(crate) fn publish_frame_products(
    inputs: Res<FrameAssemblyInputs>,
    retained: Res<StaticDrawLane>,
    focus: Option<Res<RenderFocus>>,
    prepared: Option<Res<crate::prepare::scene::view_parms::PreparedSceneView>>,
    mut frame: ResMut<RenderFrameProducts>,
    mut camera: ResMut<CameraProducts>,
    mut sun: ResMut<SunProduct>,
    mut spot: ResMut<SpotProduct>,
) {
    let (_, _, _, _, world_run_surfs) = retained.live_for(inputs.world_generation);
    frame.frame_id = inputs.frame_id;
    frame.begin_fill();
    frame.set_world_run_surfs(world_run_surfs);
    let camera = camera.as_mut();
    frame.adopt(&mut camera.colour);
    frame.adopt(&mut camera.light);
    frame.adopt(&mut camera.emissive);
    frame.adopt(&mut camera.distortion);
    frame.adopt(&mut sun.product);
    frame.adopt(&mut spot.product);
    let focused = focus.as_ref().and_then(|focus| focus.frame.clone());
    frame.set_focus(focused.clone());
    frame.finish_fill();
    if let Some(focused) = focused.as_ref() {
        let camera_origin = prepared.as_ref().map(|view| view.eye.to_array());
        perf::render_owner_plan(
            frame.frame_id,
            inputs.world_generation.0,
            "script_model",
            focused.owner_id,
            focused.model.as_deref(),
            focused.outcome,
            focused.object_id,
            camera_origin,
            focused.camera_hidden,
            focused.lighting_handle,
            focused.planned_surfaces,
        );
    }

    let draws = frame
        .product(FrameProductKind::Colour)
        .ordered_draws
        .len()
        .saturating_add(frame.product(FrameProductKind::Light).ordered_draws.len())
        .saturating_add(
            frame
                .product(FrameProductKind::Emissive)
                .ordered_draws
                .len(),
        )
        .saturating_add(
            frame
                .product(FrameProductKind::SunShadow)
                .ordered_draws
                .len(),
        )
        .saturating_add(
            frame
                .product(FrameProductKind::SpotShadow)
                .ordered_draws
                .len(),
        );
    perf::Counter::CounterDraws.emit(draws as f64);
}
