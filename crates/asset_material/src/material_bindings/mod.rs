use bevy::math::{Mat4, Vec3};
use render_material::{
    MaterialGenerationId, RuntimeArgumentBinding, RuntimeCodeSources, RuntimeMaterialCatalog,
};
mod fog;
mod iw5;
mod plan;
mod sky;
mod t5;
mod t6;
use plan::FrameStep;
mod local_light;
mod t5_light;
pub use local_light::{MaterialLightOverrides, MaterialLocalLightInputs};

#[derive(Clone, Debug)]
pub struct PreparedMaterialBindings {
    generation: MaterialGenerationId,
    requested: Vec<u16>,
    frame_steps: Vec<FrameStep>,
    local_light_ops: Vec<local_light::LightOperation>,
}

#[derive(Clone, Copy, Debug)]
pub struct MaterialFrameBindingInputs {
    pub eye: Vec3,
    pub clip_from_view: Mat4,
    pub world_from_view: Mat4,
    pub target_size: [i32; 2],
    pub time: f32,
    pub exposure: Option<f32>,
    pub world: MaterialWorldBindingInputs,
    pub tree_scatter: Option<[f32; 2]>,
    pub fog: MaterialFogInputs,
    pub fog_enabled: bool,
    pub sun: Option<MaterialSunInputs>,
}

#[derive(Clone, Copy, Debug)]
pub struct MaterialWorldBindingInputs {
    pub source_namespace: Option<asset_core::AssetNamespace>,
    pub exposure_stops: Option<f32>,
    pub model_lighting_decode_scale: f32,
    pub reflection_probe_alpha_weight: f32,
    pub sky_intensity: Option<[f32; 4]>,
}

impl Default for MaterialWorldBindingInputs {
    fn default() -> Self {
        Self {
            source_namespace: None,
            exposure_stops: None,
            model_lighting_decode_scale: core::f32::consts::FRAC_1_SQRT_2 / 2.0,
            reflection_probe_alpha_weight: 0.0,
            sky_intensity: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MaterialSunInputs {
    pub direction: [f32; 3],
    pub color: [f32; 3],
    pub diffuse_color: Option<[f32; 4]>,
    pub specular_color: Option<[f32; 4]>,
}

#[derive(Clone, Copy, Debug)]
pub struct MaterialFogInputs {
    pub color_rgb: [f32; 3],
    pub max_opacity: f32,
    pub halfway_dist: f32,
    pub start_dist: f32,
    pub volumetric: Option<MaterialFogVolumeInputs>,
    pub sun: Option<MaterialFogSunInputs>,
}

#[derive(Clone, Copy, Debug)]
pub struct MaterialFogVolumeInputs {
    pub halfway_height: f32,
    pub base_height: f32,
    pub color_scale: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct MaterialFogSunInputs {
    pub color_rgb: [f32; 3],
    pub sun_dir: [f32; 3],
    pub begin_angle_deg: f32,
    pub end_angle_deg: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaleMaterialBindings {
    pub prepared: MaterialGenerationId,
    pub current: MaterialGenerationId,
}

pub fn compile_material_bindings(catalog: &RuntimeMaterialCatalog) -> PreparedMaterialBindings {
    let mut requested: Vec<_> = catalog
        .parts()
        .technique_sets
        .iter()
        .flat_map(|set| set.techniques())
        .flat_map(|technique| &technique.passes)
        .flat_map(|pass| &pass.arguments)
        .filter_map(|argument| match argument {
            RuntimeArgumentBinding::CodeConstant { index, .. } => Some(*index),
            _ => None,
        })
        .collect();
    requested.sort_unstable();
    requested.dedup();
    PreparedMaterialBindings {
        generation: catalog.generation_id(),
        frame_steps: plan::prepare(&requested),
        local_light_ops: local_light::prepare(&requested),
        requested,
    }
}

impl PreparedMaterialBindings {
    pub fn generation_id(&self) -> MaterialGenerationId {
        self.generation
    }

    pub fn bind_frame(
        &self,
        catalog: &RuntimeMaterialCatalog,
        sources: &mut RuntimeCodeSources,
        inputs: &MaterialFrameBindingInputs,
    ) -> Result<(), StaleMaterialBindings> {
        let generation = catalog.generation_id();
        if generation != self.generation {
            return Err(StaleMaterialBindings {
                prepared: self.generation,
                current: generation,
            });
        }
        let mut writer = BindingWriter {
            requested: &self.requested,
            sources,
        };
        let sources = &mut writer;
        for step in &self.frame_steps {
            step.execute(sources, inputs);
        }

        Ok(())
    }
}

struct BindingWriter<'a> {
    requested: &'a [u16],
    sources: &'a mut RuntimeCodeSources,
}
impl BindingWriter<'_> {
    fn set_constant_rows(&mut self, index: u16, rows: &[[u32; 4]]) {
        if self.requested.binary_search(&index).is_ok() {
            self.sources.set_constant_rows(index, rows);
        }
    }
}
fn float4_bits(row: [f32; 4]) -> [u32; 4] {
    row.map(f32::to_bits)
}
pub fn fog_color_linear_and_gamma(rgb: [f32; 3], alpha: f32) -> ([f32; 4], [f32; 4]) {
    let pack = |c: f32| -> u8 { (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8 };

    let bytes = [pack(rgb[2]), pack(rgb[1]), pack(rgb[0]), pack(alpha)];
    const INV_255: f32 = 0.003_921_568_9;
    let gamma = [
        f32::from(bytes[2]) * INV_255,
        f32::from(bytes[1]) * INV_255,
        f32::from(bytes[0]) * INV_255,
        f32::from(bytes[3]) * INV_255,
    ];
    let linear = [
        lighting_iw4::color_srgb_to_linear(gamma[0]),
        lighting_iw4::color_srgb_to_linear(gamma[1]),
        lighting_iw4::color_srgb_to_linear(gamma[2]),
        gamma[3],
    ];
    (linear, gamma)
}

fn demands_any(requested: &[u16], indices: &[u16]) -> bool {
    indices.iter().any(|i| requested.binary_search(i).is_ok())
}
fn push_static(steps: &mut Vec<FrameStep>, requested: &[u16], rows: Vec<(u16, [u32; 4])>) {
    let rows: Vec<_> = rows
        .into_iter()
        .filter(|(i, _)| requested.binary_search(i).is_ok())
        .collect();
    if !rows.is_empty() {
        steps.push(FrameStep::Static(rows));
    }
}
