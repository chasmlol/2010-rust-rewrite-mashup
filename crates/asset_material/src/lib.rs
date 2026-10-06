mod material_bindings;
pub use material_bindings::{
    MaterialFogInputs, MaterialFogSunInputs, MaterialFogVolumeInputs, MaterialFrameBindingInputs,
    MaterialLightOverrides, MaterialLocalLightInputs, MaterialSunInputs,
    MaterialWorldBindingInputs, PreparedMaterialBindings, StaleMaterialBindings,
    compile_material_bindings, fog_color_linear_and_gamma,
};
mod material_compile;
pub use material_compile::compile_material_catalog;
pub mod iw5_tech_map;
pub mod material_catalog;
pub mod material_draw;
pub mod material_images;
pub mod t5_code_remap;
pub mod t5_tech_map;
pub mod t6_techset;
pub mod vertex_layout;
pub use vertex_layout::*;

pub use asset_core::*;
pub use asset_transport::{
    LoadProgress, StageHandle, cache_flight, cache_get, cache_put, fnv1a64, fnv1a64_more,
};
pub use material_catalog::*;
pub use material_draw::*;
pub use material_images::*;

pub mod asset_graph {
    pub use asset_core::*;
}
pub mod progress {
    pub use asset_transport::progress::*;
}

pub use material_compile::compile_material_state;
