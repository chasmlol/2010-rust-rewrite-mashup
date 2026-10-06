use glam::{Mat4, Vec3};

use crate::LightAttenuationBind;
use render_material::{RuntimeCodeSources, RuntimeImageId};

#[derive(Clone, Copy, Debug, Default)]
pub struct OutdoorLookup {
    pub image: Option<RuntimeImageId>,
    pub lookup: [u32; 16],
}

#[derive(Clone, Debug, Default)]
pub struct MaterialExecFrame {
    pub code_sources: RuntimeCodeSources,
    pub view_origin: Vec3,
    pub float_time: f32,
    pub clip_from_world: Option<Mat4>,
    pub view_from_world: Option<Mat4>,
    pub clip_from_view: Option<Mat4>,
    pub outdoor: Option<OutdoorLookup>,
    pub viewmodel_clip_from_world: Option<Mat4>,

    pub viewmodel_near: Option<f32>,
    pub inv_image_height: Option<f32>,
    pub primary_lights: Vec<lighting_iw4::GfxLightPack>,
    pub attenuation: Vec<LightAttenuationBind>,
    pub local_light_bindings: Vec<render_material::CompiledConstantOverlay>,
    pub reflection_probe_sh: Vec<Option<[[f32; 4]; 3]>>,
    pub smodel_lighting_sh: std::sync::Arc<Vec<Option<[[f32; 4]; 3]>>>,

    pub spot_receivers: Vec<Option<SpotShadowReceiver>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpotShadowReceiver {
    pub lookup: [f32; 16],

    pub pixel_adjust: [f32; 4],
}
