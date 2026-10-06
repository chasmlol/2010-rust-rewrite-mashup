use std::collections::BTreeSet;

use bevy::prelude::*;
use dpvs_iw4::{
    AabbNodeView, Bounds, CPlane, CellPortalGraph, PortalView, SurfRange, bounds_from_origin_axis,
    cell_caster_matrix_words, generate_shadow_map_caster_cells,
};

pub use render_scene::WorldCameraPose;

#[derive(Clone, Copy, Debug, Resource)]
pub struct MapDirPrimaryLight {
    pub light_type: u8,
    pub direction: [f32; 3],
    pub color: [f32; 3],
    pub diffuse_color_scale: f32,
    pub specular_color_scale: f32,
    pub t5_diffuse_color: Option<[f32; 4]>,
    pub t5_specular_color: Option<[f32; 4]>,
}

pub use asset_material::MaterialLightOverrides;
pub use render_frame::LightAttenuationBind;

#[derive(Clone, Debug)]
pub struct WorldPortal {
    pub plane: [f32; 4],
    pub neighbor: u16,
    pub vert_start: usize,
    pub vert_count: usize,

    pub hull_axis: Option<[[f32; 3]; 2]>,
}

#[derive(Clone, Debug)]
pub struct WorldDpvs {
    pub planes: Vec<CPlane>,
    pub nodes: Vec<u16>,
    pub cell_count: usize,
    pub cell_roots: Vec<SurfRange>,
    pub aabb_trees: Vec<Vec<AabbNodeView>>,

    pub aabb_smodel_indices: Vec<Vec<u16>>,

    pub sorted_surf_index: Vec<u16>,

    pub static_surface_count: usize,

    pub static_surface_count_no_decal: usize,

    pub surface_bounds: Vec<Bounds>,

    pub smodel_bounds: Vec<Bounds>,
    pub portal_verts: Vec<[f32; 3]>,
    pub portals_per_cell: Vec<Vec<WorldPortal>>,

    pub cell_reflection_probes: Vec<Vec<u8>>,

    pub sky_start_surfs: Vec<u32>,

    pub cell_caster_bits: Vec<u32>,
    pub lit_opaque_begin: u32,
    pub lit_opaque_end: u32,
    pub camera_ranges: asset_world::CameraSurfRanges,
    pub emissive_surfs_begin: u32,

    pub emissive_surfs_end: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldDrawItem {
    pub key: u64,
    pub surf: u16,

    pub run: u16,
    pub kind: WorldDrawItemKind,
    pub setup_key_changed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldDrawItemKind {
    Bsp(asset_world::CameraRangeKind),
    BModel,
}

#[derive(Debug)]
pub struct WorldCull {
    pub packed_indices: Vec<u32>,

    pub surface_index_ranges: Vec<(u32, u32)>,

    pub surface_draw_fields: Vec<asset_world::SurfaceDrawFields>,
    pub surface_batch_ranges: Vec<(usize, u32, u32)>,

    pub surface_materials: Vec<Option<assets::MaterialIndex>>,

    pub surface_lightmap_indices: Vec<u8>,

    pub surface_reflection_probes: Vec<u8>,

    pub surface_primary_lights: Vec<u8>,

    pub sort_key_distortion: Option<u32>,

    pub surface_sort_keys: Vec<u8>,

    pub capture: asset_world::WorldCapture,

    pub brush_models: Vec<asset_world::GfxBrushModelSurfs>,

    pub brush_model_bounds: Vec<asset_world::GfxBrushModelBounds>,

    pub bmodel_world_from_local: Vec<Mat4>,
    pub dpvs: WorldDpvs,

    pub batch_count: u32,
    pub batch_lightmapped: Vec<bool>,

    pub surface_vis: Vec<u8>,

    pub draw_items: Vec<WorldDrawItem>,

    pub bsp_run_scratch: Vec<dpvs_iw4::BspDrawSurfRun<asset_world::CameraRangeKind>>,

    pub draw_items_id: u64,

    pub g0_surfs: Vec<u16>,

    pub static_model_entities: Vec<Option<Entity>>,
    pub static_model_cull_dists: Vec<u16>,

    pub smodel_vis: Vec<u8>,

    pub cell_vis: Vec<u32>,

    pub cell_vis_all: bool,
}

#[derive(Clone, Debug)]
pub struct WorldSmodelLightingSample {
    pub authored_slot: usize,
    pub lighting_origin: [f32; 3],
    pub tile_rgba: [u8; 256],

    pub packed_lighting: [u8; 4],
    pub lighting_sh: Option<[[f32; 4]; 3]>,
    pub vertex_lighting: Option<std::sync::Arc<[Vec<[u8; 4]>; 4]>>,
}

pub const SMODEL_LIGHTING_MAX_CLIENT_VIEWS: u32 = 1;

#[derive(Resource, Default)]
pub struct WorldScene {
    pub readiness: frame::WorldReadiness,
    pub sky_model: Option<WorldStaticModelMesh>,
    pub batches: Vec<WorldBatchGeometry>,

    pub runtime_material_catalog: std::sync::Arc<crate::assemble::drawsurf::RuntimeMaterialCatalog>,

    pub exact_material_images: Vec<Option<std::sync::Arc<Image>>>,
    pub common_profile_id: u64,
    pub products_id: u64,
    pub exact_material_common: Vec<bool>,

    /// Which prepared variant each of those images is, where it came from a
    /// decode plan. Two slots carrying the same variant hold byte-identical
    /// images under the same sampler and colour space, so the upload gives
    /// them one asset and a cloned handle instead of two textures.
    pub exact_material_variants: Vec<Option<asset_material::ImageVariantId>>,

    pub exact_material_names: Vec<String>,
    pub lightmaps: Vec<Option<WorldLightmap>>,
    pub reflection_probes: Vec<Option<Image>>,

    pub reflection_probe_origins: Vec<[f32; 3]>,

    pub static_model_meshes: Vec<WorldStaticModelMesh>,

    pub smodel_mesh_names: Vec<String>,

    pub script_brush_gameobjects: Vec<String>,

    pub script_brush_exploders: Vec<String>,

    pub script_brush_targetnames: Vec<String>,

    pub script_brush_models: Vec<asset_world::ScriptBrushModelPlacement>,

    pub static_model_instances: Vec<Option<WorldStaticModelInstance>>,

    pub smodel_mark_cpu: Option<SmodelMarkCpu>,

    pub map_xmodel_scene_assets: asset_world::MapXModelSceneCatalog,

    pub script_model_instances: Vec<WorldScriptModelInstance>,

    pub dyn_ent_instances: Vec<WorldDynEntInstance>,

    pub dyn_ent_brush_n: usize,

    pub dyn_ent_brushes: Vec<WorldDynEntBrush>,

    pub smodel_lighting_samples: Vec<WorldSmodelLightingSample>,

    pub light_grid: Option<asset_model::OwnedLightGrid>,
    pub(crate) material_world: asset_material::MaterialWorldBindingInputs,

    pub model_lighting_image: Option<Handle<Image>>,
    pub model_lighting_dims: Option<lighting_iw4::ModelLightingAtlasDims>,

    pub center: Vec3,

    pub radius: f32,

    pub world_bounds: Option<[f32; 6]>,

    pub sun_sample_size_near: f32,

    pub cull: Option<WorldCull>,

    pub intermission_view: Option<WorldCameraPose>,

    pub fx_glass: Option<asset_world::FxGlassReset>,

    pub retained_packed_vertices: asset_world::WorldVertexPayload,

    pub retained_vertex_layer: Vec<u8>,
    pub surface_vertex_layer: Vec<i32>,
    pub surface_first_vertex: Vec<u32>,

    pub retained_positions: Vec<[f32; 3]>,

    pub retained_normals: Vec<[f32; 3]>,
    pub retained_tangents: Vec<[f32; 4]>,
    pub retained_colors: Vec<[f32; 4]>,
    pub retained_texture_uvs: Vec<[f32; 2]>,

    pub retained_lightmap_uvs: Vec<[f32; 2]>,

    pub exp_fog: Option<asset_world::ExpFog>,
    pub t6_film_grade: Option<asset_world::T6FilmGrade>,

    pub film_vision: Option<asset_world::FilmVision>,
    pub film_visions: std::collections::BTreeMap<
        String,
        Result<asset_world::FilmVision, asset_world::FilmVisionParseError>,
    >,

    pub createart_name: Option<String>,

    pub dir_primary_light: Option<MapDirPrimaryLight>,

    pub sun_stages: Vec<asset_world::MapSunStage>,

    pub sun_lights: Vec<(u8, MapDirPrimaryLight)>,

    pub active_sun_light: Option<u8>,

    pub t5_sun_parse_exposure: Option<f32>,
    pub t6_exposure: Option<f32>,

    pub sky_dynamic_intensity: Option<[f32; 4]>,

    pub t5_tree_scatter_intensity: Option<f32>,

    pub t5_tree_scatter_amount: Option<f32>,

    pub t5_exposure_volume_count: u32,

    pub primary_light_types: Vec<u8>,

    pub primary_light_cull: Vec<lighting_iw4::PrimaryLightCull>,

    pub primary_light_pack: Vec<lighting_iw4::GfxLightPack>,

    pub primary_light_attenuation: Vec<LightAttenuationBind>,

    pub dynamic_light: Option<crate::assemble::drawsurf::DynamicLightBind>,

    pub primary_light_def_names: Vec<Option<String>>,

    pub primary_light_overrides: Vec<MaterialLightOverrides>,

    pub reflection_probe_sh: Vec<Option<[[f32; 4]; 3]>>,

    pub sun_primary_light_count: u32,

    pub light_region_hulls: Option<Vec<Vec<asset_world::WorldLightRegionHull>>>,

    pub shadow_geometry: Vec<asset_world::WorldShadowGeometry>,

    pub outdoor_image: Option<u32>,

    pub outdoor_lookup: [u32; 16],

    pub sun_effects: Option<render_frame::SunEffectsDef>,

    pub exact_world_refuse: Option<String>,

    pub exact_world_cause2: Option<String>,

    pub exact_packed_refuse: Option<String>,

    pub exact_pos_tex_refuse: Option<String>,

    pub exact_ifc_n: Option<i64>,

    pub exact_opcode: Option<String>,

    pub spawned: bool,

    pub asset_ref: asset_material::AssetRefDumpCensus,
}

impl WorldScene {}

#[derive(Debug)]
pub struct WorldLightmap {
    pub primary_image: Option<Image>,
    pub secondary_image: Option<Image>,

    pub secondary_b_image: Option<Image>,
    pub ambient_image: Image,
    pub directional_image: Image,
    pub sun_mask_image: Image,
    pub ambient_source_name: String,
    pub sun_mask_source_name: String,
    pub ambient_size: UVec2,
    pub sun_mask_size: UVec2,
}

#[derive(Debug)]
pub struct WorldDrawGeometry {
    pub batches: Vec<WorldBatchGeometry>,
    pub lightmaps: Vec<Option<WorldLightmap>>,
    pub reflection_probes: Vec<Option<Image>>,

    pub packed_vertices: asset_world::WorldVertexPayload,

    pub vertex_layer: Vec<u8>,
    pub surface_vertex_layer: Vec<i32>,
    pub surface_first_vertex: Vec<u32>,
    pub surface_draw_fields: Vec<asset_world::SurfaceDrawFields>,

    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 4]>,
    pub colors: Vec<[f32; 4]>,
    pub texture_uvs: Vec<[f32; 2]>,
    pub lightmap_uvs: Vec<[f32; 2]>,
    pub packed_indices: Vec<u32>,

    pub surface_index_ranges: Vec<(u32, u32)>,
    pub surface_batch_ranges: Vec<(usize, u32, u32)>,

    pub surface_materials: Vec<Option<assets::MaterialIndex>>,

    pub surface_lightmap_indices: Vec<u8>,

    pub surface_reflection_probes: Vec<u8>,

    pub surface_primary_lights: Vec<u8>,
    pub sort_key_distortion: Option<u32>,
    pub surface_sort_keys: Vec<u8>,

    pub capture: asset_world::WorldCapture,

    pub brush_models: Vec<asset_world::GfxBrushModelSurfs>,

    pub brush_model_bounds: Vec<asset_world::GfxBrushModelBounds>,

    pub bmodel_world_from_local: Vec<Mat4>,
    pub static_model_meshes: Vec<WorldStaticModelMesh>,

    pub static_model_instances: Vec<Option<WorldStaticModelInstance>>,
    pub map_xmodel_scene_assets: asset_world::MapXModelSceneCatalog,
    pub script_model_instances: Vec<WorldScriptModelInstance>,

    pub smodel_lighting_samples: Vec<WorldSmodelLightingSample>,
}

#[derive(Debug)]
pub struct WorldStaticModelMesh {
    pub name: String,

    pub lod_surfaces: [Vec<WorldStaticModelSurface>; 4],

    pub lod_smc: Option<[[u8; 4]; 4]>,
    pub lod: Option<asset_model::ModelLodSelector>,
}

impl WorldStaticModelMesh {
    pub fn surface_materials(&self) -> impl Iterator<Item = Option<assets::MaterialIndex>> + '_ {
        self.lod_surfaces
            .iter()
            .flatten()
            .map(|surface| surface.material)
    }
}

#[derive(Debug)]
pub struct WorldStaticModelSurface {
    pub mesh: Mesh,

    pub material: Option<assets::MaterialIndex>,

    pub packed_vertices: Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
    pub xsurface_plus_1: Option<u8>,
    pub xsurface_base_index: u16,
    pub xsurface_vert_offset: u16,
    pub collision: asset_world::XSurfaceCollisionPayload,
}

#[derive(Clone, Copy, Debug)]
pub struct WorldStaticModelInstance {
    pub mesh: usize,
    pub transform: Transform,
    pub origin: [f32; 3],
    pub axis: [[f32; 3]; 3],
    pub scale: f32,

    pub cull_dist: u16,

    pub reflection_probe_index: u8,

    pub primary_light_index: u8,

    pub flags: u8,
}

#[derive(Clone, Debug, Default)]
pub struct SmodelMarkCpu {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub meshes: Vec<Vec<SmodelMarkSurface>>,
}

#[derive(Clone, Debug)]
pub struct SmodelMarkSurface {
    pub index_start: u32,
    pub index_count: u32,
    pub material: Option<assets::MaterialIndex>,
    pub collision: asset_world::XSurfaceCollisionPayload,
}

pub use render_scene::{WorldDynEntInstance, WorldScriptModelInstance};

#[derive(Clone, Copy, Debug)]
pub struct WorldDynEntBrush {
    pub index: u16,
    pub origin: [f32; 3],

    pub bounds: Option<Bounds>,

    pub brush_model: u16,

    pub surface_count: u16,
}

pub(crate) fn transform_from_gfx_placement(origin: [f32; 3], quat: [f32; 4]) -> Transform {
    let q = Quat::from_xyzw(quat[0], quat[1], quat[2], quat[3]);
    let rotation = if q.length_squared() > 1e-12 {
        q.normalize()
    } else {
        Quat::IDENTITY
    };
    Transform {
        translation: Vec3::from_array(origin),
        rotation,
        scale: Vec3::ONE,
    }
}

fn bmodel_axis_from_angles(angles: [f32; 3]) -> [[f32; 3]; 3] {
    const DEG2RAD: f32 = 0.01745329238474369;
    let yaw = angles[1] * DEG2RAD;
    let pitch = angles[0] * DEG2RAD;
    let roll = angles[2] * DEG2RAD;
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let (sr, cr) = roll.sin_cos();
    [
        [cp * cy, cp * sy, -sp],
        [sr * sp * cy + -sy * cr, sr * sp * sy + cr * cy, sr * cp],
        [cr * sp * cy + sr * sy, cr * sp * sy + -sr * cy, cr * cp],
    ]
}

fn mat4_from_origin_axis(origin: [f32; 3], axis: [[f32; 3]; 3]) -> Mat4 {
    Mat4::from_cols(
        Vec4::new(axis[0][0], axis[0][1], axis[0][2], 0.0),
        Vec4::new(axis[1][0], axis[1][1], axis[1][2], 0.0),
        Vec4::new(axis[2][0], axis[2][1], axis[2][2], 0.0),
        Vec4::new(origin[0], origin[1], origin[2], 1.0),
    )
}

pub(crate) fn authored_bmodel_world_from_local(
    model_n: usize,
    script_brushes: &[asset_world::ScriptBrushModelPlacement],
    dynent_brushes: &[asset_world::DynEntDef],
) -> Vec<Mat4> {
    let mut slots: Vec<Option<Mat4>> = vec![None; model_n];
    for brush in script_brushes {
        let index = brush.cmodel_handle as usize;
        if index == 0 || index >= model_n || slots[index].is_some() {
            continue;
        }
        slots[index] = Some(mat4_from_origin_axis(
            brush.origin,
            bmodel_axis_from_angles(brush.angles),
        ));
    }
    for def in dynent_brushes {
        let index = usize::from(def.brush_model);
        if index == 0 || index >= model_n || slots[index].is_some() {
            continue;
        }
        let transform = transform_from_gfx_placement(def.origin, def.quat);
        slots[index] = Some(transform.to_matrix());
    }
    slots
        .into_iter()
        .map(|pose| pose.unwrap_or(Mat4::IDENTITY))
        .collect()
}

pub(crate) fn world_dyn_ent_instances(
    catalog: &asset_world::DynEntCatalog,
) -> (Vec<WorldDynEntInstance>, usize) {
    let mut instances = Vec::new();
    for def in &catalog.models {
        let Some(name) = def.xmodel.as_ref().filter(|name| !name.is_empty()) else {
            continue;
        };
        instances.push(WorldDynEntInstance {
            index: def.index,
            ty: def.ty,
            current_model: asset_world::MapXModelAssetKey(name.clone()),
            transform: transform_from_gfx_placement(def.origin, def.quat),
            lighting_origin: def.origin,
            phys_preset: def.phys_preset.clone(),
            health: def.health,
            destroy_fx: def.destroy_fx.clone(),
            dead: false,
        });
    }
    (instances, catalog.brushes.len())
}

pub(crate) fn world_dyn_ent_brushes(
    defs: &[asset_world::DynEntDef],
    local_bounds: &[asset_world::GfxBrushModelBounds],
    surfs: &[asset_world::GfxBrushModelSurfs],
) -> Vec<WorldDynEntBrush> {
    defs.iter()
        .map(|def| WorldDynEntBrush {
            index: def.index,
            origin: def.origin,
            bounds: local_bounds
                .get(usize::from(def.brush_model))
                .copied()
                .map(|local| posed_brush_bounds(def.origin, def.quat, local)),
            brush_model: def.brush_model,
            surface_count: surfs
                .get(usize::from(def.brush_model))
                .map(|model| model.surface_count)
                .unwrap_or(0),
        })
        .collect()
}

pub(crate) fn posed_brush_bounds(
    origin: [f32; 3],
    quat: [f32; 4],
    local: asset_world::GfxBrushModelBounds,
) -> Bounds {
    let rotation = {
        let q = Quat::from_xyzw(quat[0], quat[1], quat[2], quat[3]);
        if q.length_squared() > 1e-12 {
            q.normalize()
        } else {
            Quat::IDENTITY
        }
    };
    let m = Mat3::from_quat(rotation);
    bounds_from_origin_axis(
        origin,
        [
            m.x_axis.to_array(),
            m.y_axis.to_array(),
            m.z_axis.to_array(),
        ],
        Bounds::from_mid_half(local.mid, local.half),
    )
}

#[derive(Debug)]
pub struct WorldBatchGeometry {
    pub mesh: Mesh,
    pub packed_indices: Vec<u32>,

    pub material: Option<assets::MaterialIndex>,
    pub lightmapped: bool,
    pub lightmap_index: u8,
    pub primary_light_index: u8,
    pub reflection_probe_index: u8,
    pub sun: Option<WorldSun>,
}

#[derive(Clone, Copy, Debug)]
pub struct WorldSun {
    pub direction: [f32; 3],
    pub color: [f32; 3],
}

fn remap_local_material(
    local: Option<usize>,
    asset_ids: &[Option<usize>],
) -> Option<assets::MaterialIndex> {
    local
        .and_then(|index| asset_ids.get(index).copied().flatten())
        .map(assets::MaterialIndex::from_order)
}

fn sun_color_image(
    catalog: &crate::assemble::drawsurf::RuntimeMaterialCatalog,
    material: Option<assets::MaterialIndex>,
) -> Option<u32> {
    let material = catalog.derived(material?)?;
    material
        .texture_semantic(asset_material::TS_COLOR_MAP)
        .or_else(|| material.texture_semantic(asset_material::TS_2D))
        .map(|image| image.0)
}

fn install_sun_effects(
    capture: Option<&asset_world::SunEffectsCapture>,
    asset_ids: &[Option<usize>],
    catalog: &crate::assemble::drawsurf::RuntimeMaterialCatalog,
) -> Option<render_frame::SunEffectsDef> {
    let capture = capture?;
    let sprite_material = remap_local_material(capture.sprite_material, asset_ids);
    let flare_material = remap_local_material(capture.flare_material, asset_ids);
    Some(render_frame::SunEffectsDef {
        sprite_material: sprite_material.and_then(|id| u32::try_from(id.order()).ok()),
        flare_material: flare_material.and_then(|id| u32::try_from(id.order()).ok()),
        sprite_image: sun_color_image(catalog, sprite_material),
        flare_image: sun_color_image(catalog, flare_material),
        sprite_size: capture.sprite_size,
        flare_min_size: capture.flare_min_size,
        flare_min_dot: capture.flare_min_dot,
        flare_max_size: capture.flare_max_size,
        flare_max_dot: capture.flare_max_dot,
        flare_max_alpha: capture.flare_max_alpha,
        flare_fade_in_ms: capture.flare_fade_in_ms,
        flare_fade_out_ms: capture.flare_fade_out_ms,
        blind_min_dot: capture.blind_min_dot,
        blind_max_dot: capture.blind_max_dot,
        blind_max_darken: capture.blind_max_darken,
        blind_fade_in_ms: capture.blind_fade_in_ms,
        blind_fade_out_ms: capture.blind_fade_out_ms,
        glare_min_dot: capture.glare_min_dot,
        glare_max_dot: capture.glare_max_dot,
        glare_max_lighten: capture.glare_max_lighten,
        glare_fade_in_ms: capture.glare_fade_in_ms,
        glare_fade_out_ms: capture.glare_fade_out_ms,
        direction: capture.direction,
    })
}

impl WorldScene {
    pub fn from_bounds(
        mesh: Mesh,
        min: [f32; 3],
        max: [f32; 3],
        policy: asset_world::WorldDrawPolicy,
    ) -> Self {
        let min = Vec3::from_array(min);
        let max = Vec3::from_array(max);
        WorldScene {
            sky_model: None,
            batches: vec![WorldBatchGeometry {
                mesh,
                packed_indices: Vec::new(),
                material: None,
                lightmapped: false,
                lightmap_index: 0,
                primary_light_index: 0,
                reflection_probe_index: 0,
                sun: None,
            }],
            runtime_material_catalog: std::sync::Arc::new(
                crate::assemble::drawsurf::RuntimeMaterialCatalog::default(),
            ),
            exact_material_images: Vec::new(),
            common_profile_id: 0,
            products_id: 0,
            exact_material_common: Vec::new(),
            exact_material_variants: Vec::new(),
            exact_material_names: Vec::new(),
            lightmaps: Vec::new(),
            reflection_probes: Vec::new(),
            reflection_probe_origins: Vec::new(),
            static_model_meshes: Vec::new(),
            smodel_mesh_names: Vec::new(),
            script_brush_gameobjects: Vec::new(),
            script_brush_exploders: Vec::new(),
            script_brush_targetnames: Vec::new(),
            script_brush_models: Vec::new(),
            static_model_instances: Vec::new(),
            smodel_mark_cpu: None,
            map_xmodel_scene_assets: asset_world::MapXModelSceneCatalog::default(),
            script_model_instances: Vec::new(),
            dyn_ent_instances: Vec::new(),
            dyn_ent_brush_n: 0,
            dyn_ent_brushes: Vec::new(),
            smodel_lighting_samples: Vec::new(),
            light_grid: None,
            material_world: super::world_bindings::prepare(
                None,
                asset_model::LightGridColorEncoding::Rgb8,
                None,
                None,
            ),
            model_lighting_image: None,
            model_lighting_dims: None,
            center: (min + max) * 0.5,
            radius: ((max - min).length() * 0.5).max(1.0),
            world_bounds: None,
            sun_sample_size_near: policy.sun_sample_size_near,
            cull: None,
            intermission_view: None,
            fx_glass: None,
            retained_packed_vertices: asset_world::WorldVertexPayload::Unavailable {
                source_layout: "synthetic bounds mesh",
            },
            retained_vertex_layer: Vec::new(),
            surface_vertex_layer: Vec::new(),
            surface_first_vertex: Vec::new(),
            retained_positions: Vec::new(),
            retained_normals: Vec::new(),
            retained_tangents: Vec::new(),
            retained_colors: Vec::new(),
            retained_texture_uvs: Vec::new(),
            retained_lightmap_uvs: Vec::new(),
            exp_fog: None,
            t6_film_grade: None,
            film_vision: None,
            film_visions: Default::default(),
            createart_name: None,
            dir_primary_light: None,
            sun_stages: Vec::new(),
            sun_lights: Vec::new(),
            active_sun_light: None,
            t5_sun_parse_exposure: None,
            t6_exposure: None,
            sky_dynamic_intensity: None,
            t5_tree_scatter_intensity: None,
            t5_tree_scatter_amount: None,
            t5_exposure_volume_count: 0,
            primary_light_types: Vec::new(),
            primary_light_cull: Vec::new(),
            primary_light_pack: Vec::new(),
            primary_light_attenuation: Vec::new(),
            dynamic_light: None,
            primary_light_def_names: Vec::new(),
            primary_light_overrides: Vec::new(),
            reflection_probe_sh: Vec::new(),
            sun_primary_light_count: 0,
            light_region_hulls: None,
            shadow_geometry: Vec::new(),
            outdoor_image: None,
            outdoor_lookup: [0; 16],
            sun_effects: None,
            exact_world_refuse: None,
            exact_world_cause2: None,
            exact_packed_refuse: None,
            exact_pos_tex_refuse: None,
            exact_ifc_n: None,
            exact_opcode: None,
            spawned: false,
            readiness: frame::WorldReadiness::default(),
            asset_ref: asset_material::AssetRefDumpCensus::default(),
        }
    }

    pub fn from_draw(
        draw: WorldDrawGeometry,
        min: [f32; 3],
        max: [f32; 3],
        dpvs: WorldDpvs,
        intermission_view: Option<WorldCameraPose>,
        policy: asset_world::WorldDrawPolicy,
    ) -> Self {
        let min = Vec3::from_array(min);
        let max = Vec3::from_array(max);
        let mut scene = WorldScene {
            sky_model: None,
            batches: draw.batches,
            runtime_material_catalog: std::sync::Arc::new(
                crate::assemble::drawsurf::RuntimeMaterialCatalog::default(),
            ),
            exact_material_images: Vec::new(),
            common_profile_id: 0,
            products_id: 0,
            exact_material_common: Vec::new(),
            exact_material_variants: Vec::new(),
            exact_material_names: Vec::new(),
            lightmaps: draw.lightmaps,
            reflection_probes: draw.reflection_probes,
            reflection_probe_origins: Vec::new(),
            static_model_meshes: draw.static_model_meshes,
            smodel_mesh_names: Vec::new(),
            script_brush_gameobjects: Vec::new(),
            script_brush_exploders: Vec::new(),
            script_brush_targetnames: Vec::new(),
            script_brush_models: Vec::new(),
            static_model_instances: draw.static_model_instances,
            smodel_mark_cpu: None,
            map_xmodel_scene_assets: draw.map_xmodel_scene_assets,
            script_model_instances: draw.script_model_instances,
            dyn_ent_instances: Vec::new(),
            dyn_ent_brush_n: 0,
            dyn_ent_brushes: Vec::new(),
            smodel_lighting_samples: draw.smodel_lighting_samples,
            light_grid: None,
            material_world: super::world_bindings::prepare(
                None,
                asset_model::LightGridColorEncoding::Rgb8,
                None,
                None,
            ),
            model_lighting_image: None,
            model_lighting_dims: None,
            center: (min + max) * 0.5,
            radius: ((max - min).length() * 0.5).max(1.0),
            world_bounds: None,
            sun_sample_size_near: policy.sun_sample_size_near,
            cull: None,
            intermission_view: None,
            fx_glass: None,
            retained_packed_vertices: draw.packed_vertices,
            retained_vertex_layer: draw.vertex_layer,
            surface_vertex_layer: draw.surface_vertex_layer,
            surface_first_vertex: draw.surface_first_vertex,
            retained_positions: Vec::new(),
            retained_normals: Vec::new(),
            retained_tangents: Vec::new(),
            retained_colors: Vec::new(),
            retained_texture_uvs: Vec::new(),
            retained_lightmap_uvs: Vec::new(),
            exp_fog: None,
            t6_film_grade: None,
            film_vision: None,
            film_visions: Default::default(),
            createart_name: None,
            dir_primary_light: None,
            sun_stages: Vec::new(),
            sun_lights: Vec::new(),
            active_sun_light: None,
            t5_sun_parse_exposure: None,
            t6_exposure: None,
            sky_dynamic_intensity: None,
            t5_tree_scatter_intensity: None,
            t5_tree_scatter_amount: None,
            t5_exposure_volume_count: 0,
            primary_light_types: Vec::new(),
            primary_light_cull: Vec::new(),
            primary_light_pack: Vec::new(),
            primary_light_attenuation: Vec::new(),
            dynamic_light: None,
            primary_light_def_names: Vec::new(),
            primary_light_overrides: Vec::new(),
            reflection_probe_sh: Vec::new(),
            sun_primary_light_count: 0,
            light_region_hulls: None,
            shadow_geometry: Vec::new(),
            outdoor_image: None,
            outdoor_lookup: [0; 16],
            sun_effects: None,
            exact_world_refuse: None,
            exact_world_cause2: None,
            exact_packed_refuse: None,
            exact_pos_tex_refuse: None,
            exact_ifc_n: None,
            exact_opcode: None,
            spawned: false,
            readiness: frame::WorldReadiness::default(),
            asset_ref: asset_material::AssetRefDumpCensus::default(),
        };
        let batch_count = scene.batches.len() as u32;
        let batch_lightmapped = scene
            .batches
            .iter()
            .map(|batch| batch.lightmapped)
            .collect();

        for batch in &mut scene.batches {
            batch.mesh = Mesh::new(
                bevy::render::mesh::PrimitiveTopology::TriangleList,
                bevy::asset::RenderAssetUsages::MAIN_WORLD,
            );
            batch.packed_indices.clear();
        }
        scene.cull = Some(WorldCull {
            packed_indices: draw.packed_indices,
            surface_index_ranges: draw.surface_index_ranges,
            surface_draw_fields: draw.surface_draw_fields,
            surface_batch_ranges: draw.surface_batch_ranges,
            surface_materials: draw.surface_materials,
            surface_lightmap_indices: draw.surface_lightmap_indices,
            surface_reflection_probes: draw.surface_reflection_probes,
            surface_primary_lights: draw.surface_primary_lights,
            sort_key_distortion: draw.sort_key_distortion,
            surface_sort_keys: draw.surface_sort_keys,
            capture: draw.capture,
            brush_models: draw.brush_models,
            brush_model_bounds: draw.brush_model_bounds,
            bmodel_world_from_local: draw.bmodel_world_from_local,
            dpvs,
            batch_count,
            batch_lightmapped,
            surface_vis: Vec::new(),
            draw_items: Vec::new(),
            bsp_run_scratch: Vec::new(),
            draw_items_id: 0,
            g0_surfs: Vec::new(),
            static_model_entities: Vec::new(),
            static_model_cull_dists: Vec::new(),
            smodel_vis: Vec::new(),
            cell_vis: Vec::new(),
            cell_vis_all: false,
        });

        scene.retained_positions = draw.positions;
        scene.retained_normals = draw.normals;
        scene.retained_tangents = draw.tangents;
        scene.retained_colors = draw.colors;
        scene.retained_texture_uvs = draw.texture_uvs;
        scene.retained_lightmap_uvs = draw.lightmap_uvs;
        scene.intermission_view = intermission_view;
        scene
    }

    pub(crate) fn stamp_surface_materials(
        cull: &mut WorldCull,
        catalog: &crate::assemble::drawsurf::RuntimeMaterialCatalog,
    ) -> Result<(), asset_world::SurfaceMaterialStampError> {
        let baked: Vec<Option<dpvs_iw4::GfxDrawSurf>> = catalog
            .parts()
            .materials
            .iter()
            .map(|material| {
                material
                    .baked_draw_surf
                    .map(dpvs_iw4::GfxDrawSurf::from_packed)
            })
            .collect();
        let slots: Vec<Option<usize>> = cull
            .surface_materials
            .iter()
            .map(|id| id.map(|index| index.order()))
            .collect();
        asset_world::stamp_packed_surface_materials(
            &slots,
            &cull.surface_primary_lights,
            &baked,
            &mut cull.capture,
        )?;
        Ok(())
    }

    pub fn dyn_atpoint_lookup_fallback(&self, mid: [f32; 3], box_half: Option<[f32; 3]>) -> u8 {
        match self.dyn_atpoint_walk_trace(mid, box_half) {
            Some(trace) => trace.walk as u8,
            None => lighting_iw4::LIGHT_GRID_ATPOINT_EMPTY_PRIMARY,
        }
    }

    pub fn publish_dyn_atpoint(&self, lookup: &mut render_scene::DynAtPointLookup) {
        lookup.replace(
            self.primary_light_cull.clone(),
            self.sun_primary_light_count,
            self.light_region_hulls.clone(),
        );
    }

    pub fn publish_dpvs_cells(&self, cells: &mut render_scene::WorldDpvsCells) {
        match self.cull.as_ref() {
            Some(cull) => {
                cells.planes.clone_from(&cull.dpvs.planes);
                cells.nodes.clone_from(&cull.dpvs.nodes);
                cells.cell_count = cull.dpvs.cell_count;
            }
            None => *cells = render_scene::WorldDpvsCells::default(),
        }
    }

    pub fn with_light_region_hulls<R>(
        &self,
        f: impl FnOnce(Option<&[lighting_iw4::LightRegionHulls<'_>]>) -> R,
    ) -> R {
        let Some(region_lists) = self.light_region_hulls.as_ref() else {
            return f(None);
        };
        let hulls: Vec<Vec<lighting_iw4::LightRegionHull<'_>>> = region_lists
            .iter()
            .map(|list| {
                list.iter()
                    .map(|h| lighting_iw4::LightRegionHull {
                        kdop_mid: h.kdop_mid,
                        kdop_half: h.kdop_half,
                        axes: &h.axes,
                    })
                    .collect()
            })
            .collect();
        let refs: Vec<lighting_iw4::LightRegionHulls<'_>> =
            hulls.iter().map(|v| v.as_slice()).collect();
        f(Some(&refs))
    }

    pub fn dyn_atpoint_walk_trace(
        &self,
        mid: [f32; 3],
        box_half: Option<[f32; 3]>,
    ) -> Option<lighting_iw4::NonSunPrimaryWalkTrace> {
        let half = box_half?;
        let region_lists = self.light_region_hulls.as_ref()?;
        let hulls: Vec<Vec<lighting_iw4::LightRegionHull<'_>>> = region_lists
            .iter()
            .map(|list| {
                list.iter()
                    .map(|h| lighting_iw4::LightRegionHull {
                        kdop_mid: h.kdop_mid,
                        kdop_half: h.kdop_half,
                        axes: &h.axes,
                    })
                    .collect()
            })
            .collect();
        let refs: Vec<lighting_iw4::LightRegionHulls<'_>> =
            hulls.iter().map(|v| v.as_slice()).collect();
        Some(lighting_iw4::non_sun_primary_light_walk_trace(
            &self.primary_light_cull,
            self.sun_primary_light_count,
            Some(&refs),
            mid,
            half,
        ))
    }

    pub fn reflection_probe_for_lighting_origin(&self, origin: [f32; 3]) -> u8 {
        let origins = self.reflection_probe_origins.as_slice();
        let Some(cull) = self.cull.as_ref() else {
            return lighting_iw4::nearest_reflection_probe_skip_0(origins, origin);
        };
        let dpvs = &cull.dpvs;
        let planes = dpvs_iw4::DpvsPlanes {
            planes: &dpvs.planes,
            nodes: &dpvs.nodes,
            cell_count: dpvs.cell_count as u32,
        };
        match planes.cell_for_point(origin) {
            None => lighting_iw4::nearest_reflection_probe_skip_0(origins, origin),
            Some(cell) => {
                let list = dpvs
                    .cell_reflection_probes
                    .get(cell)
                    .map(|v| v.as_slice())
                    .unwrap_or(&[]);
                lighting_iw4::nearest_reflection_probe_in_cell_list(origins, list, origin)
            }
        }
    }
}

pub fn world_scene_from_draw(
    world: assets::PreparedWorld,
    materials: assets::MatchMaterials,
    installed_owners: &[(asset_world::ScriptModelId, sim::AuthorityModelOwner)],
) -> Result<WorldScene, asset_world::SurfaceMaterialStampError> {
    let dynamic_light = world.dynamic_light;
    let fx_glass = world.fx_glass;
    let world_bounds = world.world_bounds;
    let script_brush_models = world.script_brush_models.clone();
    let script_brush_gameobjects: Vec<String> = script_brush_models
        .iter()
        .map(|brush| brush.gameobject.clone())
        .collect();
    let script_brush_exploders: Vec<String> = script_brush_models
        .iter()
        .map(|brush| brush.script_exploder.clone())
        .collect();
    let script_brush_targetnames: Vec<String> = script_brush_models
        .iter()
        .map(|brush| brush.targetname.clone())
        .collect();
    let (dyn_ent_instances, dyn_ent_brush_n) = world_dyn_ent_instances(&world.dyn_ents);
    diag::info!(
        World,
        "{} admitted_models={}",
        world.dyn_ents.report_line(),
        dyn_ent_instances.len(),
    );
    let assets::MatchMaterials {
        population: global_materials,
        map_ids,
        common_profile_id,
        products_id,
    } = materials;
    let Some(mut draw) = world.draw else {
        let mut empty = WorldScene::default();
        empty.sun_sample_size_near = world.policy.sun_sample_size_near;
        empty.common_profile_id = common_profile_id;
        empty.products_id = products_id;
        empty.fx_glass = fx_glass;
        empty.script_brush_gameobjects = script_brush_gameobjects;
        empty.script_brush_exploders = script_brush_exploders;
        empty.script_brush_targetnames = script_brush_targetnames;
        empty.script_brush_models = script_brush_models;
        empty.dyn_ent_instances = dyn_ent_instances;
        empty.dyn_ent_brush_n = dyn_ent_brush_n;
        empty.dyn_ent_brushes = world_dyn_ent_brushes(&world.dyn_ents.brushes, &[], &[]);
        return Ok(empty);
    };
    let dyn_ent_brushes = world_dyn_ent_brushes(
        &world.dyn_ents.brushes,
        &draw.brush_model_bounds,
        &draw.brush_models,
    );
    let policy = world.policy;
    let reflection_probe_origins: Vec<[f32; 3]> = draw
        .reflection_probes
        .iter()
        .map(|probe| probe.origin)
        .collect();
    let runtime_material_catalog = asset_material::compile_material_catalog(&global_materials);
    let asset_ref = asset_material::AssetRefDumpCensus::from_catalog(&global_materials);

    let exact_material_images = global_materials
        .images
        .iter()
        .map(|image| image.decoded.clone())
        .collect::<Vec<_>>();
    let exact_material_common = global_materials
        .images
        .iter()
        .map(|image| image.common_owned)
        .collect::<Vec<_>>();
    let exact_material_variants = global_materials
        .images
        .iter()
        .map(|image| image.decoded_variant)
        .collect::<Vec<_>>();
    let exact_material_names: Vec<String> = global_materials
        .images
        .iter()
        .map(|image| image.name.as_str().to_owned())
        .collect();
    let builtin_gaps: Vec<String> = global_materials
        .images
        .iter()
        .enumerate()
        .filter_map(|(index, image)| {
            let name = image.name.as_str();
            if !name.starts_with('$') {
                return None;
            }
            Some(format!(
                "{index}:{} decoded={} payload={} {}x{} fmt={}",
                image.name,
                exact_material_images
                    .get(index)
                    .is_some_and(Option::is_some),
                image.payload.len(),
                image.width,
                image.height,
                image.format
            ))
        })
        .collect();
    if !builtin_gaps.is_empty() {
        diag::info!(
            World,
            "drawsurf builtin images: {}",
            builtin_gaps.join("; ")
        );
    }
    let undecoded_bound: Vec<String> = {
        let slots: BTreeSet<usize> = global_materials
            .materials
            .iter()
            .flat_map(|material| material.textures.iter())
            .filter_map(|texture| texture.image)
            .collect();
        slots
            .into_iter()
            .filter_map(|index| {
                if exact_material_images
                    .get(index)
                    .is_some_and(Option::is_some)
                {
                    return None;
                }
                let image = global_materials.images.get(index)?;

                if !matches!(image.semantic, 2 | 3 | 5 | 8) {
                    return None;
                }
                Some(format!(
                    "{index}:{} payload={} {}x{}x{} fmt={} map={} semantic={}",
                    image.name,
                    image.payload.len(),
                    image.width,
                    image.height,
                    image.depth,
                    image.format,
                    image.map_type,
                    image.semantic
                ))
            })
            .collect()
    };
    if !undecoded_bound.is_empty() {
        diag::info!(
            World,
            "drawsurf undecoded color/normal/specular: {}",
            undecoded_bound.join("; ")
        );
    }

    {
        let mut bound: BTreeSet<usize> = global_materials
            .materials
            .iter()
            .flat_map(|material| material.textures.iter())
            .filter_map(|texture| texture.image)
            .collect();

        bound.extend(
            draw.primary_lights
                .iter()
                .filter_map(|light| light.attenuation_image),
        );
        bound.extend(draw.outdoor_image);
        let rows: Vec<String> = bound
            .into_iter()
            .filter(|index| {
                !exact_material_images
                    .get(*index)
                    .is_some_and(Option::is_some)
            })
            .filter_map(|index| {
                let image = global_materials.images.get(index)?;
                Some(format!(
                    "{index}:{} payload={} {}x{}x{} fmt={} map={} semantic={}",
                    image.name,
                    image.payload.len(),
                    image.width,
                    image.height,
                    image.depth,
                    image.format,
                    image.map_type,
                    image.semantic
                ))
            })
            .collect();
        if !rows.is_empty() {
            diag::warn!(
                World,
                "drawsurf bound images that never decoded: n={} {}",
                rows.len(),
                rows.iter().take(24).cloned().collect::<Vec<_>>().join("; ")
            );
        }
    }
    if let crate::assemble::drawsurf::RuntimeSortedMaterialTable::BuildFailed(
        crate::assemble::drawsurf::CatalogBuildError::ShaderIdentityMissing { material, slot },
    ) = &runtime_material_catalog.parts().sorted_materials
    {
        let name = global_materials
            .materials
            .get(usize::from(material.0))
            .map(|material| material.name.as_str())
            .unwrap_or("<out-of-range>");
        diag::warn!(
            World,
            "drawsurf sorted-material input: material {} {name:?} slot {slot} has no exact shader identity",
            material.0
        );
    }
    let surface_material_asset_ids = draw
        .surface_materials
        .iter()
        .copied()
        .map(|local| remap_local_material(local, &map_ids))
        .collect::<Vec<_>>();

    let lightmaps: Vec<Option<WorldLightmap>> = match draw.lightmap {
        Ok(pages) => pages
            .into_iter()
            .map(|page| {
                page.map(|lightmap| WorldLightmap {
                    primary_image: lightmap.primary_image,
                    secondary_image: lightmap.secondary_image,
                    secondary_b_image: lightmap.secondary_b_image,
                    ambient_image: lightmap.ambient_image,
                    directional_image: lightmap.directional_image,
                    sun_mask_image: lightmap.sun_mask_image,
                    ambient_source_name: lightmap.ambient_source_name,
                    sun_mask_source_name: lightmap.sun_mask_source_name,
                    ambient_size: lightmap.ambient_size,
                    sun_mask_size: lightmap.sun_mask_size,
                })
            })
            .collect(),
        Err(_) => Vec::new(),
    };

    let mut dpvs = WorldDpvs {
        planes: draw.dpvs.planes,
        nodes: draw.dpvs.nodes,
        cell_count: draw.dpvs.cell_count,
        cell_roots: draw.dpvs.cell_roots,
        aabb_trees: draw.dpvs.aabb_trees,
        aabb_smodel_indices: draw.dpvs.aabb_smodel_indices,
        sorted_surf_index: draw.dpvs.sorted_surf_index,
        static_surface_count: draw.dpvs.static_surface_count,
        static_surface_count_no_decal: draw.dpvs.static_surface_count_no_decal,
        surface_bounds: draw.dpvs.surface_bounds,
        smodel_bounds: draw.dpvs.smodel_bounds,
        portal_verts: draw.dpvs.portal_verts,
        cell_reflection_probes: draw.dpvs.cell_reflection_probes,
        sky_start_surfs: draw.dpvs.sky_start_surfs,
        lit_opaque_begin: draw.dpvs.lit_opaque_begin,
        lit_opaque_end: draw.dpvs.lit_opaque_end,
        camera_ranges: draw.dpvs.camera_ranges,
        emissive_surfs_begin: draw.dpvs.emissive_surfs_begin,
        emissive_surfs_end: draw.dpvs.emissive_surfs_end,
        portals_per_cell: draw
            .dpvs
            .portals_per_cell
            .into_iter()
            .map(|cell| {
                cell.into_iter()
                    .map(|p| WorldPortal {
                        plane: p.plane,
                        neighbor: p.neighbor,
                        vert_start: p.vert_start,
                        vert_count: p.vert_count,
                        hull_axis: p.hull_axis,
                    })
                    .collect()
            })
            .collect(),
        cell_caster_bits: Vec::new(),
    };
    if let Some(light) = draw
        .primary_lights
        .iter()
        .find(|light| light.is_sun && light.light_type == lighting_iw4::GFX_LIGHT_TYPE_DIR)
    {
        dpvs.cell_caster_bits = cell_caster_bits(&dpvs, light.direction);
    }
    let sun_lights: Vec<(u8, MapDirPrimaryLight)> = draw
        .primary_lights
        .iter()
        .enumerate()
        .filter(|(_, light)| light.is_sun && light.light_type == lighting_iw4::GFX_LIGHT_TYPE_DIR)
        .filter_map(|(index, light)| {
            Some((u8::try_from(index).ok()?, map_dir_primary_light(light)))
        })
        .collect();

    let batches = draw
        .batches
        .into_iter()
        .map(|batch| {
            let sun = draw
                .primary_lights
                .get(usize::from(batch.primary_light_index))
                .filter(|light| light.is_sun)
                .map(|light| WorldSun {
                    direction: light.direction,
                    color: light.color,
                });
            let lightmapped = if policy.lightmap_requires_image {
                batch.lightmapped
                    && lightmaps
                        .get(usize::from(batch.lightmap_index))
                        .and_then(|page| page.as_ref())
                        .is_some()
            } else {
                batch.lightmapped
            };
            WorldBatchGeometry {
                mesh: batch.mesh,
                packed_indices: batch.packed_indices,
                material: remap_local_material(batch.material, &map_ids),
                lightmapped,
                lightmap_index: batch.lightmap_index,
                primary_light_index: batch.primary_light_index,
                reflection_probe_index: batch.reflection_probe_index,
                sun,
            }
        })
        .collect();

    let convert_model = |model: asset_world::ModelMesh| WorldStaticModelMesh {
        name: model.name,
        lod_smc: model.lod_smc,
        lod: model.lod,
        lod_surfaces: model.lod_surfaces.map(|lod| {
            lod.into_iter()
                .map(|surface| WorldStaticModelSurface {
                    mesh: surface.mesh,
                    material: remap_local_material(surface.material, &map_ids),
                    packed_vertices: surface.packed_vertices,
                    xsurface_plus_1: surface.xsurface_plus_1,
                    xsurface_base_index: surface.xsurface_base_index,
                    xsurface_vert_offset: surface.xsurface_vert_offset,
                    collision: surface.collision,
                })
                .collect()
        }),
    };
    let sky_model = draw.sky_model.take().map(&convert_model);
    let static_model_meshes = world
        .static_model_meshes
        .into_iter()
        .map(convert_model)
        .collect();

    let static_model_instances = world
        .static_model_instances
        .into_iter()
        .map(|placement| {
            placement.map(|placement| WorldStaticModelInstance {
                mesh: placement.mesh,
                transform: placement.transform,
                origin: placement.origin,
                axis: placement.axis,
                scale: placement.scale,
                cull_dist: placement.cull_dist,
                reflection_probe_index: placement.reflection_probe_index,
                primary_light_index: placement.primary_light_index,
                flags: placement.flags,
            })
        })
        .collect::<Vec<_>>();

    let script_model_instances = world
        .script_model_instances
        .into_iter()
        .map(|placement| {
            let authority_owner = installed_owners
                .iter()
                .find(|(id, _)| *id == placement.id)
                .map(|(_, owner)| *owner);
            WorldScriptModelInstance {
                id: placement.id,
                authority_owner,
                current_model: placement.current_model,
                transform: placement.transform,
                lighting_origin: placement.lighting_origin,
                dobj_state: placement.dobj_state,
                metadata: placement.metadata,
                gentity_number: None,
            }
        })
        .collect::<Vec<_>>();

    let surface_sort_keys: Vec<u8> = draw
        .surface_materials
        .iter()
        .copied()
        .map(|local| {
            remap_local_material(local, &map_ids)
                .and_then(|id| runtime_material_catalog.derived(id))
                .map(|material| material.sort_key)
                .unwrap_or(0)
        })
        .collect();

    let dir_primary_light = draw
        .t5_sun_light
        .map(|sun| MapDirPrimaryLight {
            light_type: lighting_iw4::GFX_LIGHT_TYPE_DIR,
            direction: sun.direction,
            color: [sun.diffuse[0], sun.diffuse[1], sun.diffuse[2]],
            diffuse_color_scale: lighting_iw4::R_COLOR_SCALE_DEFAULT,
            specular_color_scale: lighting_iw4::R_COLOR_SCALE_DEFAULT,
            t5_diffuse_color: Some(sun.diffuse),
            t5_specular_color: Some(sun.specular),
        })
        .or_else(|| {
            draw.primary_lights
                .iter()
                .find(|light| light.is_sun && light.light_type == lighting_iw4::GFX_LIGHT_TYPE_DIR)
                .map(map_dir_primary_light)
        });

    let mut map_xmodel_scene_assets = world.map_xmodel_scene_assets;
    let map_xmodel_materials = map_xmodel_scene_assets.resolve_surface_materials(&map_ids);
    diag::info!(
        World,
        "map xmodel surface materials: models={} surfaces={} bound={} unbound={} absent={}",
        map_xmodel_materials.models,
        map_xmodel_materials.surfaces,
        map_xmodel_materials.bound,
        map_xmodel_materials.unbound,
        map_xmodel_materials.absent,
    );

    let bmodel_world_from_local = authored_bmodel_world_from_local(
        draw.brush_models.len(),
        &world.script_brush_models,
        &world.dyn_ents.brushes,
    );
    let mut scene = WorldScene::from_draw(
        WorldDrawGeometry {
            batches,
            lightmaps,
            reflection_probes: world.reflection_probe_images,
            packed_vertices: draw.packed_vertices,
            vertex_layer: draw.vertex_layer,
            surface_vertex_layer: draw.surface_vertex_layer,
            surface_first_vertex: draw.surface_first_vertex,
            surface_draw_fields: draw.surface_draw_fields,
            positions: draw.positions,
            normals: draw.normals,
            tangents: draw.tangents,
            colors: draw.colors,
            texture_uvs: draw.texture_uvs,
            lightmap_uvs: draw.lightmap_uvs,
            packed_indices: draw.packed_indices,
            surface_index_ranges: draw.surface_index_ranges,
            surface_batch_ranges: draw.surface_batch_ranges,
            surface_materials: surface_material_asset_ids,
            surface_lightmap_indices: draw.surface_lightmap_indices,
            surface_reflection_probes: draw.surface_reflection_probes,
            surface_primary_lights: draw.surface_primary_lights,
            sort_key_distortion: draw.sort_key_distortion,
            surface_sort_keys,
            capture: draw.capture,
            brush_models: draw.brush_models,
            brush_model_bounds: draw.brush_model_bounds,
            bmodel_world_from_local,
            static_model_meshes,
            static_model_instances,
            map_xmodel_scene_assets,
            script_model_instances,
            smodel_lighting_samples: world
                .smodel_lighting_samples
                .into_iter()
                .map(|sample| WorldSmodelLightingSample {
                    authored_slot: sample.authored_slot,
                    lighting_origin: sample.lighting_origin,
                    tile_rgba: sample.tile_rgba,
                    packed_lighting: sample.packed_lighting,
                    lighting_sh: sample.lighting_sh,
                    vertex_lighting: sample.vertex_lighting,
                })
                .collect(),
        },
        world.min,
        world.max,
        dpvs,
        world.intermission_view.map(|view| WorldCameraPose {
            origin: view.origin,
            angles: view.angles,
        }),
        policy,
    );
    scene.fx_glass = fx_glass;
    scene.world_bounds = world_bounds;
    scene.sun_sample_size_near = policy.sun_sample_size_near;
    scene.reflection_probe_origins = reflection_probe_origins;
    scene.runtime_material_catalog = std::sync::Arc::new(runtime_material_catalog);
    if let Some(cull) = scene.cull.as_mut() {
        WorldScene::stamp_surface_materials(cull, &scene.runtime_material_catalog)?;
    }
    scene.exact_material_images = exact_material_images;
    scene.common_profile_id = common_profile_id;
    scene.products_id = products_id;
    scene.exact_material_common = exact_material_common;
    scene.exact_material_variants = exact_material_variants;
    scene.exact_material_names = exact_material_names;
    diag::info!(
        World,
        "canonical materials: n={} batch_mat={} smodel_mat={} (MaterialIndex into decoded catalog; runtime is derived)",
        scene.runtime_material_catalog.parts().materials.len(),
        scene
            .batches
            .iter()
            .filter(|batch| batch.material.is_some())
            .count(),
        scene
            .static_model_meshes
            .iter()
            .flat_map(|mesh| mesh.lod_surfaces.iter().flatten())
            .filter(|surface| surface.material.is_some())
            .count(),
    );
    scene.material_world = super::world_bindings::prepare(
        world.source_namespace,
        world
            .light_grid
            .as_ref()
            .map_or(asset_model::LightGridColorEncoding::Rgb8, |grid| {
                grid.color_encoding
            }),
        draw.t6_exposure,
        draw.sky_dynamic_intensity,
    );
    scene.light_grid = world.light_grid;
    scene.sky_model = sky_model;
    scene.exp_fog = world.exp_fog;
    scene.t6_film_grade = world.t6_film_grade;
    scene.film_vision = world.film_vision;
    scene.film_visions = world.film_visions;
    scene.createart_name = world.createart_name;
    scene.dir_primary_light = dir_primary_light;
    scene.sun_stages = draw.sun_stages;
    scene.sun_lights = sun_lights;
    scene.t5_sun_parse_exposure = draw.t5_sun_parse_exposure;
    scene.t6_exposure = draw.t6_exposure;
    scene.sky_dynamic_intensity = draw.sky_dynamic_intensity;
    scene.t5_tree_scatter_intensity = draw.t5_tree_scatter_intensity;
    scene.t5_tree_scatter_amount = draw.t5_tree_scatter_amount;
    scene.t5_exposure_volume_count = draw.t5_exposure_volume_count;
    scene.primary_light_types = draw
        .primary_lights
        .iter()
        .map(|light| light.light_type)
        .collect();
    scene.primary_light_cull = draw
        .primary_lights
        .iter()
        .map(asset_world::WorldPrimaryLight::cull_input)
        .collect();
    scene.primary_light_pack = draw
        .primary_lights
        .iter()
        .map(asset_world::WorldPrimaryLight::gfx_light_pack)
        .collect();
    scene.primary_light_attenuation = draw
        .primary_lights
        .iter()
        .map(|light| LightAttenuationBind {
            image: light
                .attenuation_image
                .and_then(|index| u32::try_from(index).ok()),
            sampler: light.attenuation_sampler,
        })
        .collect();
    scene.dynamic_light = dynamic_light.map(|light| crate::assemble::drawsurf::DynamicLightBind {
        attenuation: LightAttenuationBind {
            image: light
                .attenuation_image
                .and_then(|index| u32::try_from(index).ok()),
            sampler: light.attenuation_sampler,
        },
        falloff_image_width: light.falloff_image_width,
        lmap_lookup_start: light.lmap_lookup_start,
    });
    scene.primary_light_def_names = draw
        .primary_lights
        .iter()
        .map(|light| light.def_name.clone())
        .collect();
    scene.primary_light_overrides = draw
        .primary_lights
        .iter()
        .map(|light| MaterialLightOverrides {
            diffuse: light.t5_diffuse_color,
            specular: light.t5_specular_color,
            attenuation: light.t5_attenuation,
            falloff: light.t5_falloff,
            cone_bounds: light.t5_a_ab_b,
            rotation: light.t5_angle.map(|angle| angle[2]),
            cookie0: light.t5_cookie0,
            cookie1: light.t5_cookie1,
            cookie2: light.t5_cookie2,
        })
        .collect();
    scene.reflection_probe_sh = draw
        .reflection_probes
        .iter()
        .map(|probe| probe.lighting_sh)
        .collect();
    scene.sun_primary_light_count = draw.sun_primary_light_count;
    scene.light_region_hulls = draw.light_region_hulls;
    scene.shadow_geometry = draw.shadow_geometry;
    scene.outdoor_image = draw
        .outdoor_image
        .and_then(|index| u32::try_from(index).ok());
    scene.outdoor_lookup = draw.outdoor_lookup;
    scene.sun_effects = install_sun_effects(
        draw.sun_effects.as_ref(),
        &map_ids,
        &scene.runtime_material_catalog,
    );
    scene.asset_ref = asset_ref;
    scene.script_brush_gameobjects = script_brush_gameobjects;
    scene.script_brush_exploders = script_brush_exploders;
    scene.script_brush_targetnames = script_brush_targetnames;
    scene.script_brush_models = script_brush_models;
    scene.dyn_ent_instances = dyn_ent_instances;
    scene.dyn_ent_brush_n = dyn_ent_brush_n;
    scene.dyn_ent_brushes = dyn_ent_brushes;
    Ok(scene)
}

fn map_dir_primary_light(light: &asset_world::WorldPrimaryLight) -> MapDirPrimaryLight {
    MapDirPrimaryLight {
        light_type: light.light_type,
        direction: light.direction,
        color: light.color,
        diffuse_color_scale: lighting_iw4::R_COLOR_SCALE_DEFAULT,
        specular_color_scale: lighting_iw4::R_COLOR_SCALE_DEFAULT,
        t5_diffuse_color: light.t5_diffuse_color,
        t5_specular_color: light.t5_specular_color,
    }
}

pub(crate) fn cell_caster_bits(dpvs: &WorldDpvs, sun_dir: [f32; 3]) -> Vec<u32> {
    let len_sq = sun_dir[0] * sun_dir[0] + sun_dir[1] * sun_dir[1] + sun_dir[2] * sun_dir[2];
    if len_sq < 1e-12 {
        return Vec::new();
    }
    let view_dir = [-sun_dir[0], -sun_dir[1], -sun_dir[2]];
    let mut per_cell: Vec<Vec<PortalView<'_>>> = Vec::with_capacity(dpvs.cell_count);
    for cell_portals in &dpvs.portals_per_cell {
        let mut views = Vec::with_capacity(cell_portals.len());
        for p in cell_portals {
            let end = p.vert_start + p.vert_count;
            let verts = dpvs.portal_verts.get(p.vert_start..end).unwrap_or(&[]);
            views.push(PortalView {
                plane: p.plane,
                neighbor: p.neighbor,
                vertices: verts,
                hull_axis: p.hull_axis,
            });
        }
        per_cell.push(views);
    }
    let slices: Vec<&[PortalView<'_>]> = per_cell.iter().map(|v| v.as_slice()).collect();
    let graph = CellPortalGraph { portals: &slices };
    let mut bits = vec![0u32; cell_caster_matrix_words(dpvs.cell_count)];
    generate_shadow_map_caster_cells(&graph, view_dir, &mut bits);
    bits
}
