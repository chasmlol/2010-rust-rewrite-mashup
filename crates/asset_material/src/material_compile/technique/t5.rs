use super::*;

pub(super) struct T5Compiler;

impl MaterialCompiler for T5Compiler {
    fn draw_rules(
        &self,
        material: &crate::AuthoredMaterial,
        _unlit: bool,
    ) -> render_material::MaterialDrawRules {
        render_material::MaterialDrawRules {
            colour_camera_region: if material.camera_region == 3 {
                asset_iw4::CAMERA_REGION_NONE
            } else {
                material.camera_region
            },
            smodel_colour_emits: fastfile_t5::state_bits::smodel_camera_emits(
                material.info_game_flags,
                material.camera_region,
            ),
            unlit_sky: false,
            postfx_host_supported: false,
        }
    }
    fn compile_state(&self, words: [u32; 2]) -> render_material::CompiledPassState {
        super::super::state::packed_state(
            words,
            fastfile_t5::state_bits::alpha_test(words[0])
                .map(|(func, reference)| d3d9_state::AlphaTest::from_raw(func, reference)),
        )
    }
    fn color_space(&self, slot: u8) -> PassColorSpace {
        if lighting_iw4::is_lit_remap_slot(slot) {
            PassColorSpace::GammaEncoded
        } else {
            PassColorSpace::Unknown
        }
    }
    fn hardware_shadow_compare(&self) -> bool {
        true
    }
}
