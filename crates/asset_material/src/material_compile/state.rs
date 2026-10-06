use render_material::CompiledPassState;

pub(super) fn iw_state(words: [u32; 2]) -> CompiledPassState {
    packed_state(
        words,
        asset_iw4::alpha_test_from_state_bits(words).map(asset_iw4::Gfxs0AlphaTest::d3d),
    )
}

pub(super) fn packed_state(
    words: [u32; 2],
    alpha: Option<d3d9_state::AlphaTest>,
) -> CompiledPassState {
    let cull = match asset_iw4::cull_face_from_state_bits(words) {
        asset_iw4::Gfxs0CullFace::Back => 1,
        asset_iw4::Gfxs0CullFace::Front => 2,
        asset_iw4::Gfxs0CullFace::None => 0,
    };
    render_material::compile_packed_state(words, alpha, cull)
}

pub fn compile_material_state(family: asset_core::FamilyId, words: [u32; 2]) -> CompiledPassState {
    super::technique::compiler_for(family).compile_state(words)
}
