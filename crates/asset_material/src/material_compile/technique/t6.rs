use super::*;

pub(super) struct T6Compiler;

impl MaterialCompiler for T6Compiler {
    fn draw_rules(
        &self,
        material: &crate::AuthoredMaterial,
        unlit: bool,
    ) -> render_material::MaterialDrawRules {
        render_material::MaterialDrawRules {
            colour_camera_region: material.camera_region,
            smodel_colour_emits: true,
            unlit_sky: unlit
                && matches!(
                    material.sort_key,
                    asset_iw4::SORT_KEY_SKY | asset_iw4::SORT_KEY_SKYBOX
                ),
            postfx_host_supported: false,
        }
    }
    fn compile_state(&self, words: [u32; 2]) -> render_material::CompiledPassState {
        super::super::state::packed_state(words, None)
    }
    fn color_space(&self, _slot: u8) -> PassColorSpace {
        PassColorSpace::Unknown
    }
    fn hardware_shadow_compare(&self) -> bool {
        false
    }
}
