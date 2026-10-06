mod admitted;
mod backend;
mod colour_submit;
mod depth_range;
mod draw;
mod exact_pipeline;
mod floatz;
mod geometry_diagnostic;
mod gpu_prepare;
mod gpu_resources;
mod hud_blood;
mod iw_tess;
mod minecraft_light;
mod minecraft_world;
mod model_lighting_tiles;
mod postfx;
mod postfx_dof;
mod products;
mod resolved_scene;
pub(crate) mod scene_depth;
mod script_grade;
mod shadowmap_spot_gpu;
mod shadowmap_sun_gpu;
mod shellshock_screen;
mod sm3_wgsl;
mod smodel_cache_gpu;
mod smodel_cached;
mod state;
mod sun_effects;
mod texture_table;

pub use admitted::AdmittedExactPort;
pub use products::ExtractedRenderFrameProducts;
pub use render_backend::overlay::{
    CODE_BASE_LIGHTING_COORDS, CODE_SHADOWMAP_POLYGON_OFFSET, CODE_TEXTURE_OUTDOOR,
    CODE_TRANSPOSE_WORLD_VIEW_PROJECTION0, code_transpose_matrix_row4, code_transpose_matrix_rows,
};
pub use render_frame::{
    RetainedDrawItem, RetainedDrawKind, SUN_SHADOW_CASTER_TECH, SUN_SHADOW_PARTITION_COUNT,
    SurfaceSamplerInputs,
};
pub use render_material::{PackedCodeSamplerLane, RuntimeTextureBinding, TechType};
pub use render_material::{SamplerSource, SamplerTextureDimension};
pub mod gpu_contract {
    pub use render_frame::{
        TEXTURE_TABLE_2D_CAPACITY, TEXTURE_TABLE_3D_CAPACITY, TEXTURE_TABLE_CUBE_CAPACITY,
        TEXTURE_TABLE_SAMPLER_CAPACITY, WgpuBindLayoutEntry, WgpuBindingKind, WgpuLayoutRefusal,
        WgpuPassLayout, WgpuShaderVisibility, WgpuVertexAttribute, WgpuVertexBufferLayout,
        WgpuVertexFormat, derive_wgpu_pass_layout, texture_table_bind_entries,
    };
}
pub use gpu_contract::*;
pub use gpu_prepare::{
    ConstantPackRefusal, PassConstantBuffers, overlay_packed_code_on_banks, split_bind_layout,
};

pub use colour_submit::{
    CODE_TEXTURE_FLOATZ, CODE_TEXTURE_RESOLVED_POST_SUN, CODE_TEXTURE_SHADOWMAP_SPOT,
    CODE_TEXTURE_SHADOWMAP_SUN, ExtractedStaticGeometry, FocusedOwnerSubmitState,
    InstalledRenderWorld, PublishedRenderFrame, RenderFrameData, RenderWorldData,
    bind_group_layout_from_entries, cached_lighting_port_variant, colour_ports_static,
    colour_world_smodel_static, dump_shader_program_names, dump_sorted_material_names,
    emit_focused_owner_submit, vertex_layouts_from_contract,
};
pub(crate) use draw::register_drawsurf_render;
pub use geometry_diagnostic::{ExtractedDiagnosticGeometry, geometry_diagnostic_enabled};
pub use gpu_resources::*;
pub use minecraft_world::{
    MINECRAFT_VERTEX_BYTES, MinecraftAtlasImage, MinecraftClouds, MinecraftSectionUpload,
    MinecraftWorldFrame,
};
pub use model_lighting_tiles::{ModelLightingTileUpload, ModelLightingTileUploads};
pub use postfx::{ExtractedBlood, ExtractedFilm, ExtractedPostFx};
pub use postfx_dof::{DepthOfField, DofFrame, GLOW_APPLY_MATERIAL, GLOW_SETUP_MATERIAL, GlowFrame};
pub use shadowmap_spot_gpu::*;
pub use shadowmap_sun_gpu::*;
pub use sm3_wgsl::{
    PASS_FRAGMENT_ENTRY, PASS_VERTEX_ENTRY, ValidatedPassWgsl, alpha_test_fragment_entry,
};
pub use smodel_cache_gpu::*;
pub use state::{ChangeState0Host, ChangeState1Host, GfxPassState};
