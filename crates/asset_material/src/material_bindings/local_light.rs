use super::{PreparedMaterialBindings, StaleMaterialBindings};
use bevy::math::Vec3;
use render_material::{CompiledConstantOverlay, RuntimeMaterialCatalog, compile_constant_overlay};

#[derive(Clone, Copy, Debug)]
pub struct MaterialLocalLightInputs {
    pub light_type: u8,
    pub direction: [f32; 3],
    pub origin: [f32; 3],
    pub radius: f32,
    pub cos_outer: f32,
    pub overrides: MaterialLightOverrides,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MaterialLightOverrides {
    pub diffuse: Option<[f32; 4]>,
    pub specular: Option<[f32; 4]>,
    pub attenuation: Option<[f32; 4]>,
    pub falloff: Option<[f32; 4]>,
    pub cone_bounds: Option<[f32; 4]>,
    pub rotation: Option<f32>,
    pub cookie0: Option<[f32; 4]>,
    pub cookie1: Option<[f32; 4]>,
    pub cookie2: Option<[f32; 4]>,
}

impl PreparedMaterialBindings {
    pub fn prepare_local_light(
        &self,
        catalog: &RuntimeMaterialCatalog,
        light: Option<&MaterialLocalLightInputs>,
        eye: Vec3,
        time: f32,
    ) -> Result<CompiledConstantOverlay, StaleMaterialBindings> {
        let generation = catalog.generation_id();
        if generation != self.generation {
            return Err(StaleMaterialBindings {
                prepared: self.generation,
                current: generation,
            });
        }
        let mut writer = LocalLightWriter {
            requested: &self.requested,
            rows: Vec::new(),
        };
        if let Some(light) = light {
            for &operation in &self.local_light_ops {
                super::t5_light::produce(&mut writer, light, eye, time, operation);
            }
        }
        Ok(compile_constant_overlay(catalog, writer.rows))
    }
}

pub(super) struct LocalLightWriter<'a> {
    requested: &'a [u16],
    rows: Vec<(u16, [u32; 4])>,
}
impl LocalLightWriter<'_> {
    pub(super) fn set_constant_rows(&mut self, index: u16, rows: &[[u32; 4]]) {
        if self.requested.binary_search(&index).is_ok() {
            self.rows.push((index, rows[0]));
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LightOperation {
    Colors,
    Attenuation,
    Falloff,
    SpotBounds,
    ConeControl,
    Cookie,
    Matrix,
}
pub(super) fn prepare(requested: &[u16]) -> Vec<LightOperation> {
    super::t5_light::prepare(requested)
}
