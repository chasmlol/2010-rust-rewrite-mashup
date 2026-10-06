use super::*;

pub(super) struct Iw4Compiler;

impl MaterialCompiler for Iw4Compiler {
    fn draw_rules(
        &self,
        material: &crate::AuthoredMaterial,
        _unlit: bool,
    ) -> render_material::MaterialDrawRules {
        render_material::MaterialDrawRules {
            colour_camera_region: material.camera_region,
            smodel_colour_emits: true,
            unlit_sky: false,
            postfx_host_supported: true,
        }
    }
    fn compile_state(&self, words: [u32; 2]) -> render_material::CompiledPassState {
        super::super::state::iw_state(words)
    }
    fn color_space(&self, _slot: u8) -> PassColorSpace {
        PassColorSpace::Linear
    }
    fn hardware_shadow_compare(&self) -> bool {
        false
    }
}
