use std::collections::HashMap;

use asset_iw4::size as sz;
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use dpvs_iw4::{
    AabbNodeView, Bounds, CPlane, SurfRange, custom_index_from_info_game_flags,
    surface_casts_sun_shadow_bit,
};

use crate::world_mesh::{
    BoundsTable, WorldMeshError, WorldMeshStats, normalize_or_up, unpack_color, unpack_unit_vec,
};
use asset_material::{AuthoredMaterial, MaterialCatalog, MaterialDefinitions};
use fastfile_iw4::{GfxSunEffectsGeometry, GfxWorldGeometry, Ptr, ZonePtr, ZoneStream};

use crate::{SurfaceCastsSunShadow, WorldCapture, world_capture_from_casters};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxBrushModelSurfs {
    pub start_surf: u16,
    pub surface_count: u16,
    pub surface_count_no_decal: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GfxBrushModelBounds {
    pub mid: [f32; 3],
    pub half: [f32; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfacePass {
    pub sky: bool,

    pub shadow_only: bool,

    pub multiply: bool,

    pub unlit: bool,

    pub unrouted: bool,

    pub state_bits_undecided: bool,
}

impl SurfacePass {
    pub fn takes_lightmap(&self) -> bool {
        !(self.sky || self.shadow_only || self.multiply || self.unlit)
    }
}

pub fn surface_pass(
    materials: &MaterialCatalog,
    authored: Option<&AuthoredMaterial>,
) -> SurfacePass {
    let Some(material) = authored else {
        return SurfacePass::default();
    };
    let state_bits_undecided = materials.agreed_draw_mode(material).is_none();
    let Some(unlit) = materials.is_unlit(material) else {
        return SurfacePass {
            multiply: materials.is_multiply(material),
            unrouted: true,
            state_bits_undecided,
            ..SurfacePass::default()
        };
    };
    SurfacePass {
        sky: materials.is_sky(material),
        shadow_only: materials.is_shadowcaster(material),
        multiply: materials.is_multiply(material),
        unlit,
        unrouted: false,
        state_bits_undecided,
    }
}

#[derive(Clone, Debug)]
pub struct OwnedPortal {
    pub plane: [f32; 4],
    pub neighbor: u16,
    pub vert_start: usize,
    pub vert_count: usize,

    pub hull_axis: Option<[[f32; 3]; 2]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraRangeKind {
    LitOpaque,
    LitTrans,
    Decal,
    Emissive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraSurfRange {
    pub kind: CameraRangeKind,
    pub begin: u32,
    pub end: u32,
}

#[derive(Clone, Debug)]
pub struct CameraSurfRanges {
    first: CameraSurfRange,
    rest: Vec<CameraSurfRange>,
}

impl CameraSurfRanges {
    pub fn new(first: CameraSurfRange, rest: Vec<CameraSurfRange>) -> Self {
        Self { first, rest }
    }

    pub fn iter(&self) -> impl Iterator<Item = CameraSurfRange> + '_ {
        core::iter::once(self.first).chain(self.rest.iter().copied())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceDrawFields {
    pub first_vertex: u32,
    pub tri_count: u16,
    pub base_index: u32,
    pub lightmap_index: u8,
    pub reflection_probe_index: u8,
    pub primary_light_index: u8,
}

impl dpvs_iw4::BspSurfaceDrawFields for SurfaceDrawFields {
    fn first_vertex(self) -> u32 {
        self.first_vertex
    }

    fn tri_count(self) -> u16 {
        self.tri_count
    }

    fn base_index(self) -> u32 {
        self.base_index
    }

    fn lightmap_index(self) -> u8 {
        self.lightmap_index
    }

    fn reflection_probe_index(self) -> u8 {
        self.reflection_probe_index
    }

    fn primary_light_index(self) -> u8 {
        self.primary_light_index
    }
}

#[derive(Clone, Debug)]
pub struct DpvsWorldData {
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
    pub portals_per_cell: Vec<Vec<OwnedPortal>>,

    pub cell_reflection_probes: Vec<Vec<u8>>,
    pub lit_opaque_begin: u32,
    pub lit_opaque_end: u32,
    pub camera_ranges: CameraSurfRanges,

    pub emissive_surfs_begin: u32,

    pub emissive_surfs_end: u32,

    pub sky_start_surfs: Vec<u32>,

    pub cleared_boxes: usize,
}

impl DpvsWorldData {
    pub fn new(camera_ranges: CameraSurfRanges) -> Self {
        Self {
            planes: Vec::new(),
            nodes: Vec::new(),
            cell_count: 0,
            cell_roots: Vec::new(),
            aabb_trees: Vec::new(),
            aabb_smodel_indices: Vec::new(),
            sorted_surf_index: Vec::new(),
            static_surface_count: 0,
            static_surface_count_no_decal: 0,
            surface_bounds: Vec::new(),
            smodel_bounds: Vec::new(),
            portal_verts: Vec::new(),
            portals_per_cell: Vec::new(),
            cell_reflection_probes: Vec::new(),
            lit_opaque_begin: 0,
            lit_opaque_end: 0,
            camera_ranges,
            emissive_surfs_begin: 0,
            emissive_surfs_end: 0,
            sky_start_surfs: Vec::new(),
            cleared_boxes: 0,
        }
    }

    pub(crate) fn checked(mut self) -> Result<Self, WorldMeshError> {
        let mut cleared = 0usize;
        for (cell, nodes) in self.aabb_trees.iter().enumerate() {
            let table = BoundsTable::AabbNode { cell };
            cleared += reject_negative_half(table, nodes.iter().map(|node| node.bounds))?;
        }
        cleared += reject_negative_half(BoundsTable::Surface, self.surface_bounds.iter().copied())?;
        cleared +=
            reject_negative_half(BoundsTable::SmodelInst, self.smodel_bounds.iter().copied())?;
        self.cleared_boxes = cleared;
        Ok(self)
    }
}

fn reject_negative_half(
    table: BoundsTable,
    boxes: impl Iterator<Item = Bounds>,
) -> Result<usize, WorldMeshError> {
    let mut cleared = 0usize;
    for (index, bounds) in boxes.enumerate() {
        if bounds.is_cleared() {
            cleared += 1;
            continue;
        }
        if let Some(axis) = bounds.negative_half_axis() {
            return Err(WorldMeshError::NegativeBoundsHalf {
                table,
                index,
                axis,
                half: bounds.half()[axis],
            });
        }
    }
    Ok(cleared)
}

#[derive(Clone, Debug)]
pub enum WorldVertexPayload {
    Iw4(Vec<[u8; asset_iw4::size::GFX_WORLD_VERTEX]>),

    Iw5(Vec<[u8; fastfile_iw5::size::GFX_WORLD_VERTEX]>),

    T5(Vec<[u8; fastfile_t5::size::GFX_WORLD_VERTEX]>),
    T6(Vec<[u8; asset_iw4::size::GFX_WORLD_VERTEX]>),
    Unavailable { source_layout: &'static str },
}

impl Default for WorldVertexPayload {
    fn default() -> Self {
        Self::Unavailable {
            source_layout: "world vertex payload not installed",
        }
    }
}

impl WorldVertexPayload {
    pub fn type2_stream0(
        &self,
    ) -> Result<&[[u8; asset_iw4::size::GFX_WORLD_VERTEX]], &'static str> {
        const _: () =
            assert!(asset_iw4::size::GFX_WORLD_VERTEX == fastfile_iw5::size::GFX_WORLD_VERTEX);
        const _: () =
            assert!(asset_iw4::size::GFX_WORLD_VERTEX == fastfile_t5::size::GFX_WORLD_VERTEX);
        match self {
            Self::Iw4(rows) | Self::Iw5(rows) | Self::T5(rows) | Self::T6(rows) => Ok(rows),
            Self::Unavailable { source_layout } => Err(*source_layout),
        }
    }

    pub fn dump_kind(&self) -> &'static str {
        match self {
            Self::Iw4(_) => "iw4",
            Self::Iw5(_) => "iw5",
            Self::T5(_) => "t5",
            Self::T6(_) => "t6",
            Self::Unavailable { source_layout } => *source_layout,
        }
    }
}

#[derive(Clone, Debug)]
pub struct WorldDraw {
    pub batches: Vec<WorldBatch>,

    pub sky_model: Option<crate::model_mesh::ModelMesh>,

    pub lightmap: Result<Vec<Option<WorldLightmap>>, WorldLightmapGap>,
    pub stats: WorldMeshStats,

    pub packed_vertices: WorldVertexPayload,

    pub vertex_layer: Vec<u8>,

    pub surface_vertex_layer: Vec<i32>,

    pub surface_first_vertex: Vec<u32>,

    pub surface_draw_fields: Vec<SurfaceDrawFields>,

    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 4]>,
    pub colors: Vec<[f32; 4]>,
    pub texture_uvs: Vec<[f32; 2]>,
    pub lightmap_uvs: Vec<[f32; 2]>,

    pub packed_indices: Vec<u32>,

    pub surface_index_ranges: Vec<(u32, u32)>,

    pub surface_batch_ranges: Vec<(usize, u32, u32)>,

    pub surface_lightmapped: Vec<bool>,

    pub surface_lightmap_indices: Vec<u8>,

    pub surface_reflection_probes: Vec<u8>,

    pub surface_primary_lights: Vec<u8>,

    pub sort_key_distortion: Option<u32>,

    pub capture: WorldCapture,

    pub brush_models: Vec<GfxBrushModelSurfs>,

    pub brush_model_bounds: Vec<GfxBrushModelBounds>,

    pub surface_materials: Vec<Option<usize>>,

    pub primary_lights: Vec<WorldPrimaryLight>,

    pub light_defs: Vec<CapturedLightDef>,

    pub sun_primary_light_count: u32,

    pub sun_stages: Vec<crate::MapSunStage>,

    pub light_region_hulls: Option<Vec<Vec<WorldLightRegionHull>>>,

    pub shadow_geometry: Vec<WorldShadowGeometry>,

    pub reflection_probes: Vec<WorldReflectionProbe>,
    pub dpvs: DpvsWorldData,

    pub outdoor_image_name: Option<String>,

    pub outdoor_image: Option<usize>,

    pub outdoor_lookup: [u32; 16],

    pub sun_effects: Option<SunEffectsCapture>,

    pub t5_sun_parse_exposure: Option<f32>,
    pub t6_exposure: Option<f32>,

    pub sky_dynamic_intensity: Option<[f32; 4]>,

    pub t5_sun_light: Option<WorldSunLight>,

    pub t5_tree_scatter_intensity: Option<f32>,

    pub t5_tree_scatter_amount: Option<f32>,

    pub t5_exposure_volume_count: u32,
}

/// Zone-local sun-effects capture. Material slots are walk-local indices.
#[derive(Clone, Copy, Debug)]
pub struct SunEffectsCapture {
    pub sprite_material: Option<usize>,
    pub flare_material: Option<usize>,
    pub sprite_size: f32,
    pub flare_min_size: f32,
    pub flare_min_dot: f32,
    pub flare_max_size: f32,
    pub flare_max_dot: f32,
    pub flare_max_alpha: f32,
    pub flare_fade_in_ms: i32,
    pub flare_fade_out_ms: i32,
    pub blind_min_dot: f32,
    pub blind_max_dot: f32,
    pub blind_max_darken: f32,
    pub blind_fade_in_ms: i32,
    pub blind_fade_out_ms: i32,
    pub glare_min_dot: f32,
    pub glare_max_dot: f32,
    pub glare_max_lighten: f32,
    pub glare_fade_in_ms: i32,
    pub glare_fade_out_ms: i32,
    pub direction: [f32; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct WorldSunLight {
    pub direction: [f32; 3],
    pub diffuse: [f32; 4],
    pub specular: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct WorldBatch {
    pub mesh: Mesh,
    pub packed_indices: Vec<u32>,
    pub material: Option<usize>,
    pub lightmapped: bool,

    pub lightmap_index: u8,
    pub primary_light_index: u8,
    pub reflection_probe_index: u8,
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorldShadowGeometry {
    pub surfaces: Vec<u16>,
    pub smodels: Vec<u16>,
}

#[derive(Clone, Debug)]
pub struct WorldPrimaryLight {
    pub is_sun: bool,
    pub light_type: u8,
    pub can_cast_shadow: bool,
    pub exponent: u8,
    pub color: [f32; 3],
    pub direction: [f32; 3],
    pub origin: [f32; 3],
    pub radius: f32,
    pub cos_outer: f32,
    pub cos_inner: f32,

    pub cos_half_fov_expanded: f32,

    pub def_name: Option<String>,

    pub falloff_image_width: Option<u16>,

    pub lmap_lookup_start: i32,

    pub attenuation_image: Option<usize>,

    pub attenuation_sampler: u8,

    pub t5_attenuation: Option<[f32; 4]>,

    pub t5_falloff: Option<[f32; 4]>,

    pub t5_a_ab_b: Option<[f32; 4]>,

    pub t5_angle: Option<[f32; 4]>,

    pub t5_cookie0: Option<[f32; 4]>,

    pub t5_cookie1: Option<[f32; 4]>,

    pub t5_cookie2: Option<[f32; 4]>,

    pub t5_diffuse_color: Option<[f32; 4]>,

    pub t5_specular_color: Option<[f32; 4]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedLightDef {
    pub namespace: crate::AssetNamespace,
    pub name: crate::AssetRef,
    pub attenuation_image_name: Option<String>,
    pub attenuation_width: Option<u16>,
    pub attenuation_sampler: u8,
    pub lmap_lookup_start: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResolvedLightDef {
    pub attenuation_image: Option<usize>,
    pub attenuation_sampler: u8,
    pub falloff_image_width: Option<u16>,
    pub lmap_lookup_start: i32,
}

pub fn resolve_named_light_def(
    name: &str,
    map_defs: &[CapturedLightDef],
    common_defs: &[CapturedLightDef],
    catalog: &MaterialDefinitions,
) -> Option<ResolvedLightDef> {
    let def = find_light_def(name, map_defs, common_defs)?;
    let image = def
        .attenuation_image_name
        .as_deref()
        .and_then(|name| catalog.image_index_by_key(def.namespace, name));
    let width = image
        .and_then(|index| catalog.images.get(index))
        .and_then(|image| {
            image
                .decoded
                .as_ref()
                .and_then(|decoded| u16::try_from(decoded.texture_descriptor.size.width).ok())
                .or(Some(image.width))
        })
        .filter(|&width| width != 0)
        .or(def.attenuation_width.filter(|&width| width != 0));
    Some(ResolvedLightDef {
        attenuation_image: image,
        attenuation_sampler: def.attenuation_sampler,
        falloff_image_width: width,
        lmap_lookup_start: def.lmap_lookup_start,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldLightRegionHull {
    pub kdop_mid: [f32; 9],
    pub kdop_half: [f32; 9],
    pub axes: Vec<lighting_iw4::LightRegionAxis>,
}

impl WorldPrimaryLight {
    pub fn cull_input(&self) -> lighting_iw4::PrimaryLightCull {
        lighting_iw4::PrimaryLightCull {
            light_type: self.light_type,
            origin: self.origin,
            direction: self.direction,
            radius: self.radius,
            cos_half_fov_expanded: self.cos_half_fov_expanded,
        }
    }

    pub fn gfx_light_pack(&self) -> lighting_iw4::GfxLightPack {
        lighting_iw4::GfxLightPack {
            light_type: self.light_type,
            color: self.color,
            direction: self.direction,
            origin: self.origin,
            radius: self.radius,
            cos_outer: self.cos_outer,
            cos_inner: self.cos_inner,
            exponent: self.exponent,
            falloff_image_width: self.falloff_image_width,
            lmap_lookup_start: self.lmap_lookup_start,
        }
    }
}

#[derive(Clone, Debug)]
pub struct WorldReflectionProbe {
    pub image: Option<usize>,
    pub origin: [f32; 3],
    pub lighting_sh: Option<[[f32; 4]; 3]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldLightmapGap {
    Missing,
    MultiplePages(usize),
    MissingPrimary,
    MissingSecondary,
    MissingSecondaryB,
    UnsupportedPrimaryFormat(u32),
    UnsupportedSecondaryFormat(u32),
    UnsupportedSecondaryBFormat(u32),
    InvalidDimensions { width: u16, height: u16, depth: u16 },
    MismatchedAspectRatio { primary: UVec2, secondary: UVec2 },
    Truncated { needed: usize, have: usize },
}

impl std::fmt::Display for WorldLightmapGap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => write!(f, "world has no baked lightmap"),
            Self::MultiplePages(count) => write!(
                f,
                "world has {count} lightmaps; none decoded (page walk failed)"
            ),
            Self::MissingPrimary => write!(f, "lightmap primary image was not retained"),
            Self::MissingSecondaryB => write!(f, "lightmap secondaryB image was not retained"),
            Self::UnsupportedSecondaryBFormat(format) => {
                write!(f, "lightmap secondaryB D3D format {format} is unsupported")
            }
            Self::MissingSecondary => write!(f, "lightmap secondary image was not retained"),
            Self::UnsupportedPrimaryFormat(format) => {
                write!(f, "lightmap primary D3D format {format} is unsupported")
            }
            Self::UnsupportedSecondaryFormat(format) => {
                write!(f, "lightmap secondary D3D format {format} is unsupported")
            }
            Self::InvalidDimensions {
                width,
                height,
                depth,
            } => write!(f, "invalid lightmap dimensions {width}x{height}x{depth}"),
            Self::MismatchedAspectRatio { primary, secondary } => write!(
                f,
                "lightmap primary {}x{} and secondary page {}x{} have different aspect ratios",
                primary.x, primary.y, secondary.x, secondary.y
            ),
            Self::Truncated { needed, have } => {
                write!(
                    f,
                    "lightmap payload is truncated: need {needed}, have {have}"
                )
            }
        }
    }
}

pub(crate) fn scatter_vertex_layer_type3(
    packed: &[u8],
    vertex_count: usize,
    first_vertex: &[u32],
    counts: &[u32],
    layer_off: &[i32],
) -> Vec<u8> {
    let stride = usize::from(
        asset_iw4::vertex_decl::stream_extent(asset_iw4::vertex_decl::WORLD_VERTEX_TYPE + 1, 1)
            .unwrap_or(0),
    );
    if stride == 0 || vertex_count == 0 {
        return Vec::new();
    }
    let mut parallel = vec![0u8; vertex_count.saturating_mul(stride)];
    for ((first, count), off) in first_vertex.iter().zip(counts).zip(layer_off) {
        if *off < 0 || *count == 0 {
            continue;
        }
        let src = *off as usize;
        let dst = (*first as usize).saturating_mul(stride);
        let n = (*count as usize).saturating_mul(stride);
        let Some(src_end) = src.checked_add(n) else {
            continue;
        };
        let Some(dst_end) = dst.checked_add(n) else {
            continue;
        };
        if src_end > packed.len() || dst_end > parallel.len() {
            continue;
        }
        parallel[dst..dst_end].copy_from_slice(&packed[src..src_end]);
    }
    parallel
}

pub fn build_world_draw(
    s: &ZoneStream<'_>,
    geometry: GfxWorldGeometry,
    materials: MaterialCatalog,
) -> Result<(WorldDraw, MaterialCatalog), WorldMeshError> {
    let (Some(vertices), Some(indices)) = (geometry.vertices, geometry.indices) else {
        return Err(WorldMeshError::NoGeometry);
    };
    let Some(surfaces) = geometry.surfaces else {
        return Err(WorldMeshError::NoGeometry);
    };

    let mut packed_vertices = Vec::with_capacity(geometry.vertex_count);
    let mut positions = Vec::with_capacity(geometry.vertex_count);
    let mut normals = Vec::with_capacity(geometry.vertex_count);
    let mut tangents = Vec::with_capacity(geometry.vertex_count);
    let mut colors = Vec::with_capacity(geometry.vertex_count);
    let mut texture_uvs = Vec::with_capacity(geometry.vertex_count);
    let mut lightmap_uvs = Vec::with_capacity(geometry.vertex_count);
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];

    for i in 0..geometry.vertex_count {
        let v = vertices.at(i * sz::GFX_WORLD_VERTEX);
        let mut packed = [0u8; sz::GFX_WORLD_VERTEX];
        for (offset, byte) in packed.iter_mut().enumerate() {
            *byte = s.u8_at(v, offset)?;
        }
        packed_vertices.push(packed);
        let xyz = [s.f32_at(v, 0)?, s.f32_at(v, 4)?, s.f32_at(v, 8)?];
        for axis in 0..3 {
            min[axis] = min[axis].min(xyz[axis]);
            max[axis] = max[axis].max(xyz[axis]);
        }
        positions.push(xyz);
        normals.push(normalize_or_up(unpack_unit_vec(s.u32_at(v, 36)?)));

        let tangent = normalize_or_up(unpack_unit_vec(s.u32_at(v, 40)?));
        tangents.push([tangent[0], tangent[1], tangent[2], s.f32_at(v, 12)?]);
        colors.push(unpack_color(s.u32_at(v, 16)?));
        texture_uvs.push([s.f32_at(v, 20)?, s.f32_at(v, 24)?]);
        lightmap_uvs.push([s.f32_at(v, 28)?, s.f32_at(v, 32)?]);
    }

    let mut packed_indices: Vec<u32> = Vec::new();
    let mut surface_index_ranges = Vec::with_capacity(geometry.surface_count);
    let mut authored_lightmapped = Vec::with_capacity(geometry.surface_count);
    let mut surface_lightmap_indices = Vec::with_capacity(geometry.surface_count);
    let mut surface_materials = Vec::with_capacity(geometry.surface_count);
    let mut surface_primary_lights = Vec::with_capacity(geometry.surface_count);
    let mut surface_reflection_probes = Vec::with_capacity(geometry.surface_count);
    let mut surface_draw_fields = Vec::with_capacity(geometry.surface_count);
    let mut surface_casts_sun_shadow = SurfaceCastsSunShadow::with_len(geometry.surface_count);
    let mut skipped_surfaces = 0usize;
    let mut sky_surfaces = 0usize;
    let mut skipped_shadowcaster_surfaces = 0usize;
    let mut unrouted_surfaces = 0usize;
    let mut undecided_state_bits_surfaces = 0usize;

    for i in 0..geometry.surface_count {
        let surface = surfaces.at(i * s.layout(sz::GFX_SURFACE, 32));
        let first_vertex = s.u32_at(surface, 4)? as usize;
        let vertex_count = s.u16_at(surface, 8)? as usize;
        let tri_count = s.u16_at(surface, 10)? as usize;
        let base_index = s.u32_at(surface, 12)? as usize;
        let lightmap_index = s.u8_at(surface, s.layout(20, 24))? as usize;
        let material = materials.material_index(surface.at(16));
        let authored = material.and_then(|index| materials.materials.get(index.get()));
        let pass = crate::world_draw::surface_pass(&materials, authored);
        let is_sky = pass.sky;
        let is_shadowcaster = pass.shadow_only;
        if is_sky {
            sky_surfaces += 1;
        }
        unrouted_surfaces += usize::from(pass.unrouted);
        undecided_state_bits_surfaces += usize::from(pass.state_bits_undecided);

        authored_lightmapped
            .push(lightmap_index < geometry.lightmap_count && pass.takes_lightmap());
        surface_lightmap_indices.push(lightmap_index.min(255) as u8);
        surface_materials.push(material.map(|i| i.get()));
        surface_primary_lights.push(s.u8_at(surface, s.layout(22, 26))?);
        surface_reflection_probes.push(s.u8_at(surface, s.layout(21, 25))?);
        surface_draw_fields.push(SurfaceDrawFields {
            first_vertex: first_vertex as u32,
            tri_count: tri_count as u16,
            base_index: base_index as u32,
            lightmap_index: lightmap_index.min(255) as u8,
            reflection_probe_index: s.u8_at(surface, s.layout(21, 25))?,
            primary_light_index: s.u8_at(surface, s.layout(22, 26))?,
        });

        let flags_bit0 = s.u8_at(surface, s.layout(23, 27))? & 1 != 0;
        let custom_index = authored
            .map(|m| custom_index_from_info_game_flags(m.info_game_flags))
            .unwrap_or(0);
        if surface_casts_sun_shadow_bit(custom_index, flags_bit0) {
            surface_casts_sun_shadow.set(i);
        }
        let range_start = packed_indices.len() as u32;

        if is_shadowcaster {
            skipped_shadowcaster_surfaces += 1;
            surface_index_ranges.push((range_start, 0));
            continue;
        }

        if first_vertex + vertex_count > geometry.vertex_count
            || base_index + tri_count * 3 > geometry.index_count
        {
            skipped_surfaces += 1;
            surface_index_ranges.push((range_start, 0));
            continue;
        }

        let mut ok = true;
        for t in 0..tri_count * 3 {
            let local = s.u16_at(indices, (base_index + t) * 2)? as usize;
            let absolute = first_vertex + local;
            if absolute >= geometry.vertex_count {
                skipped_surfaces += 1;
                ok = false;
                break;
            }
            packed_indices.push(absolute as u32);
        }
        let count = if ok {
            packed_indices.len() as u32 - range_start
        } else {
            packed_indices.truncate(range_start as usize);
            0
        };
        surface_index_ranges.push((range_start, count));
    }

    let stats = WorldMeshStats {
        vertices: positions.len(),
        triangles: packed_indices.len() / 3,
        surfaces: geometry.surface_count,
        skipped_surfaces: skipped_surfaces + skipped_shadowcaster_surfaces,
        sky_surfaces,

        sky_material: None,
        unrouted_surfaces,
        undecided_state_bits_surfaces,
        min,
        max,
        bounds: geometry.bounds.map(|bits| bits.map(f32::from_bits)),
    };

    let lightmap = decode_lightmaps(s, geometry);
    let surface_lightmapped: Vec<bool> = match &lightmap {
        Ok(pages) => authored_lightmapped
            .into_iter()
            .zip(surface_lightmap_indices.iter().copied())
            .map(|(authored, index)| {
                authored && pages.get(index as usize).and_then(|p| p.as_ref()).is_some()
            })
            .collect(),
        Err(_) => vec![false; geometry.surface_count],
    };
    let (batches, surface_batch_ranges) = make_material_batches(
        &positions,
        &normals,
        &tangents,
        &colors,
        &texture_uvs,
        &lightmap_uvs,
        &packed_indices,
        &surface_index_ranges,
        &surface_materials,
        &surface_lightmapped,
        &surface_lightmap_indices,
        &surface_primary_lights,
        &surface_reflection_probes,
    );

    let dpvs = extract_dpvs(s, geometry)?;
    let primary_lights = extract_primary_lights(s, geometry.sun_primary_light_count)?;
    let light_defs = capture_light_defs(s, &materials);
    let light_region_hulls = extract_light_regions(s, geometry)?;
    let shadow_geometry = extract_shadow_geometry(s, geometry)?;
    let reflection_probes = extract_reflection_probes(s, geometry, &materials)?;
    let outdoor_image_name = geometry.outdoor_image.and_then(|slot| {
        materials.image_index(slot).and_then(|index| {
            materials
                .images
                .get(index)
                .map(|image| image.name.to_string())
        })
    });
    let sun_effects = extract_sun_effects(s, geometry.sun_effects, &materials)?;
    let (brush_models, brush_model_bounds) = decode_brush_models(s, geometry)?;

    Ok((
        WorldDraw {
            sky_model: None,
            batches,
            lightmap,
            stats,
            packed_vertices: WorldVertexPayload::Iw4(packed_vertices),
            vertex_layer: Vec::new(),
            surface_vertex_layer: Vec::new(),
            surface_first_vertex: surface_draw_fields
                .iter()
                .map(|surface| surface.first_vertex)
                .collect(),
            surface_draw_fields,
            positions,
            normals,
            tangents,
            colors,
            texture_uvs,
            lightmap_uvs,
            packed_indices,
            surface_index_ranges,
            surface_batch_ranges,
            surface_lightmapped,
            surface_lightmap_indices,
            surface_reflection_probes,
            surface_primary_lights,
            sort_key_distortion: geometry.sort_key_distortion,
            capture: world_capture_from_casters(surface_casts_sun_shadow),
            brush_models,
            brush_model_bounds,
            surface_materials,
            primary_lights,
            light_defs,
            sun_primary_light_count: geometry.sun_primary_light_count as u32,
            sun_stages: crate::sun_stages(s),
            light_region_hulls,
            shadow_geometry,
            reflection_probes,
            dpvs,
            outdoor_image_name,
            outdoor_image: None,
            outdoor_lookup: geometry.outdoor_lookup,
            sun_effects,
            t5_sun_parse_exposure: None,
            t6_exposure: None,
            sky_dynamic_intensity: None,
            t5_sun_light: None,
            t5_tree_scatter_intensity: None,
            t5_tree_scatter_amount: None,
            t5_exposure_volume_count: 0,
        },
        materials,
    ))
}

#[derive(Default)]
struct BatchBuilder {
    material: Option<usize>,
    lightmapped: bool,
    lightmap_index: u8,
    primary_light_index: u8,
    reflection_probe_index: u8,
    vertices: HashMap<u32, u32>,
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    tangents: Vec<[f32; 4]>,
    colors: Vec<[f32; 4]>,
    texture_uvs: Vec<[f32; 2]>,
    lightmap_uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn make_material_batches(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    tangents: &[[f32; 4]],
    colors: &[[f32; 4]],
    texture_uvs: &[[f32; 2]],
    lightmap_uvs: &[[f32; 2]],
    packed_indices: &[u32],
    surface_index_ranges: &[(u32, u32)],
    surface_materials: &[Option<usize>],
    surface_lightmapped: &[bool],
    surface_lightmap_indices: &[u8],
    surface_primary_lights: &[u8],
    surface_reflection_probes: &[u8],
) -> (Vec<WorldBatch>, Vec<(usize, u32, u32)>) {
    let mut batch_by_key = HashMap::new();
    let mut builders: Vec<BatchBuilder> = Vec::new();
    let mut surface_batches = Vec::with_capacity(surface_index_ranges.len());
    for (surface, &(start, count)) in surface_index_ranges.iter().enumerate() {
        let material = surface_materials.get(surface).copied().flatten();
        let lightmapped = surface_lightmapped.get(surface).copied().unwrap_or(false);
        let lightmap_index = surface_lightmap_indices.get(surface).copied().unwrap_or(0);
        let primary_light_index = surface_primary_lights.get(surface).copied().unwrap_or(0);
        let reflection_probe_index = surface_reflection_probes.get(surface).copied().unwrap_or(0);
        let batch = *batch_by_key
            .entry((
                material,
                lightmapped,
                lightmap_index,
                primary_light_index,
                reflection_probe_index,
            ))
            .or_insert_with(|| {
                let index = builders.len();
                builders.push(BatchBuilder {
                    material,
                    lightmapped,
                    lightmap_index,
                    primary_light_index,
                    reflection_probe_index,
                    ..Default::default()
                });
                index
            });
        let builder = &mut builders[batch];
        let batch_start = builder.indices.len() as u32;
        if let Some(indices) = packed_indices.get(start as usize..(start + count) as usize) {
            for &global in indices {
                let local = if let Some(&local) = builder.vertices.get(&global) {
                    local
                } else {
                    let global_index = global as usize;
                    let Some(position) = positions.get(global_index) else {
                        continue;
                    };
                    let local = builder.positions.len() as u32;
                    builder.vertices.insert(global, local);
                    builder.positions.push(*position);
                    builder.normals.push(normals[global_index]);
                    builder.tangents.push(tangents[global_index]);
                    builder.colors.push(colors[global_index]);
                    builder.texture_uvs.push(texture_uvs[global_index]);
                    builder.lightmap_uvs.push(lightmap_uvs[global_index]);
                    local
                };
                builder.indices.push(local);
            }
        }
        surface_batches.push((
            batch,
            batch_start,
            builder.indices.len() as u32 - batch_start,
        ));
    }

    let batches = builders
        .into_iter()
        .map(|builder| WorldBatch {
            mesh: make_mesh(
                builder.positions,
                builder.normals,
                builder.tangents,
                builder.colors,
                builder.texture_uvs,
                builder.lightmap_uvs,
                builder.indices.clone(),
            ),
            packed_indices: builder.indices,
            material: builder.material,
            lightmapped: builder.lightmapped,
            lightmap_index: builder.lightmap_index,
            primary_light_index: builder.primary_light_index,
            reflection_probe_index: builder.reflection_probe_index,
        })
        .collect();
    (batches, surface_batches)
}

fn make_mesh(
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    tangents: Vec<[f32; 4]>,
    colors: Vec<[f32; 4]>,
    texture_uvs: Vec<[f32; 2]>,
    lightmap_uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, texture_uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, lightmap_uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn decode_brush_models(
    s: &ZoneStream<'_>,
    geometry: GfxWorldGeometry,
) -> Result<(Vec<GfxBrushModelSurfs>, Vec<GfxBrushModelBounds>), WorldMeshError> {
    let mut surfs = Vec::with_capacity(geometry.model_count);
    let mut bounds = Vec::with_capacity(geometry.model_count);
    let Some(models) = geometry.models else {
        return Ok((surfs, bounds));
    };
    for i in 0..geometry.model_count {
        let model = models.at(i * sz::GFX_BRUSH_MODEL);
        surfs.push(GfxBrushModelSurfs {
            surface_count: s.u16_at(model, 52)?,
            start_surf: s.u16_at(model, 54)?,
            surface_count_no_decal: s.u16_at(model, 56)?,
        });
        bounds.push(GfxBrushModelBounds {
            mid: [
                s.f32_at(model, 24)?,
                s.f32_at(model, 28)?,
                s.f32_at(model, 32)?,
            ],
            half: [
                s.f32_at(model, 36)?,
                s.f32_at(model, 40)?,
                s.f32_at(model, 44)?,
            ],
        });
    }
    Ok((surfs, bounds))
}

pub fn brush_model_vertex_centroid(
    positions: &[[f32; 3]],
    packed_indices: &[u32],
    surface_index_ranges: &[(u32, u32)],
    model: GfxBrushModelSurfs,
) -> Option<[f32; 3]> {
    let start = usize::from(model.start_surf);
    let end = start.saturating_add(usize::from(model.surface_count));
    let last = end.min(surface_index_ranges.len());
    let mut acc = [0.0f32; 3];
    let mut n = 0u32;
    for surf in start..last {
        let (index_start, index_count) = surface_index_ranges[surf];
        let begin = index_start as usize;
        let stop = begin.saturating_add(index_count as usize);
        for &raw in packed_indices.get(begin..stop)? {
            let pos = *positions.get(raw as usize)?;
            acc[0] += pos[0];
            acc[1] += pos[1];
            acc[2] += pos[2];
            n += 1;
        }
    }
    if n == 0 {
        return None;
    }
    let inv = 1.0 / n as f32;
    Some([acc[0] * inv, acc[1] * inv, acc[2] * inv])
}

fn decode_lightmaps(
    s: &ZoneStream<'_>,
    geometry: GfxWorldGeometry,
) -> Result<Vec<Option<WorldLightmap>>, WorldLightmapGap> {
    if geometry.lightmap_count == 0 {
        return Err(WorldLightmapGap::Missing);
    }
    let retain = geometry
        .lightmap_count
        .min(fastfile_iw4::MAX_LIGHTMAP_PAGES);
    let mut pages = Vec::with_capacity(retain);
    let mut decoded = 0usize;
    let mut last_err = None;
    for i in 0..retain {
        let pair = geometry.lightmaps[i];
        match decode_lightmap_pair(s, pair.primary, pair.secondary) {
            Ok(page) => {
                pages.push(Some(page));
                decoded += 1;
            }
            Err(err) => {
                pages.push(None);
                last_err = Some(err);
            }
        }
    }
    if decoded == 0 {
        return Err(last_err.unwrap_or(WorldLightmapGap::Missing));
    }
    Ok(pages)
}

fn decode_lightmap_pair(
    s: &ZoneStream<'_>,
    primary: Option<fastfile_iw4::GfxImageGeometry>,
    secondary: Option<fastfile_iw4::GfxImageGeometry>,
) -> Result<WorldLightmap, WorldLightmapGap> {
    let image = secondary.ok_or(WorldLightmapGap::MissingSecondary)?;
    let primary = primary.ok_or(WorldLightmapGap::MissingPrimary)?;
    decode_lightmap_images(s, primary, image)
}

fn decode_lightmap_images(
    s: &ZoneStream<'_>,
    primary: fastfile_iw4::GfxImageGeometry,
    image: fastfile_iw4::GfxImageGeometry,
) -> Result<WorldLightmap, WorldLightmapGap> {
    if image.width == 0 || image.height < 2 || image.height % 2 != 0 || image.depth != 1 {
        return Err(WorldLightmapGap::InvalidDimensions {
            width: image.width,
            height: image.height,
            depth: image.depth,
        });
    }

    if image.format != 21 {
        return Err(WorldLightmapGap::UnsupportedSecondaryFormat(image.format));
    }
    let page_height = usize::from(image.height / 2);
    let row_bytes = usize::from(image.width) * 4;
    let needed = row_bytes * usize::from(image.height);
    if image.source_len < needed {
        return Err(WorldLightmapGap::Truncated {
            needed,
            have: image.source_len,
        });
    }
    let source = s
        .source_slice(image.source_offset, image.source_len)
        .map_err(|_| WorldLightmapGap::Truncated {
            needed,
            have: image.source_len,
        })?;
    let (ambient_rgba, directional_rgba) = secondary_page_payloads(source, row_bytes, page_height)?;
    let mut secondary_rgba = Vec::with_capacity(needed);
    secondary_rgba.extend_from_slice(&ambient_rgba);
    secondary_rgba.extend_from_slice(&directional_rgba);

    if std::env::var_os("IW4L_LIGHTMAP_FLAT").is_some() {
        for (index, byte) in secondary_rgba.iter_mut().enumerate() {
            *byte = if index % 4 == 3 { 128 } else { 96 };
        }
    }
    let mut secondary_image = Image::new(
        Extent3d {
            width: u32::from(image.width),
            height: u32::from(image.height),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        secondary_rgba,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    secondary_image.sampler = ImageSampler::linear();
    let mut ambient_image = Image::new(
        Extent3d {
            width: u32::from(image.width),
            height: page_height as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        ambient_rgba,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    ambient_image.sampler = ImageSampler::linear();
    let mut directional_image = Image::new(
        Extent3d {
            width: u32::from(image.width),
            height: page_height as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        directional_rgba,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    directional_image.sampler = ImageSampler::linear();
    let ambient_source_name = image
        .name
        .and_then(|name| s.cstr(name).ok())
        .unwrap_or("<unnamed lightmap secondary>")
        .to_owned();
    if primary.format != 50 {
        return Err(WorldLightmapGap::UnsupportedPrimaryFormat(primary.format));
    }
    if primary.width == 0 || primary.height == 0 || primary.depth != 1 {
        return Err(WorldLightmapGap::InvalidDimensions {
            width: primary.width,
            height: primary.height,
            depth: primary.depth,
        });
    }
    let primary_size = UVec2::new(u32::from(primary.width), u32::from(primary.height));
    let secondary_size = UVec2::new(u32::from(image.width), page_height as u32);
    if primary_size.x * secondary_size.y != primary_size.y * secondary_size.x {
        return Err(WorldLightmapGap::MismatchedAspectRatio {
            primary: primary_size,
            secondary: secondary_size,
        });
    }
    let mask_source = s
        .source_slice(primary.source_offset, primary.source_len)
        .map_err(|_| WorldLightmapGap::Truncated {
            needed: usize::from(primary.width) * usize::from(primary.height),
            have: primary.source_len,
        })?;
    let mask_source = primary_mask_payload(mask_source, primary.width, primary.height)?;
    let mut primary_image = Image::new(
        Extent3d {
            width: u32::from(primary.width),
            height: u32::from(primary.height),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        mask_source.to_vec(),
        TextureFormat::R8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    primary_image.sampler = ImageSampler::linear();
    let sun_mask_image = primary_image.clone();
    let sun_mask_source_name = primary
        .name
        .and_then(|name| s.cstr(name).ok())
        .unwrap_or("<unnamed lightmap primary>")
        .to_owned();
    Ok(WorldLightmap {
        primary_image: Some(primary_image),
        secondary_image: Some(secondary_image),
        secondary_b_image: None,
        ambient_image,
        directional_image,
        sun_mask_image,
        ambient_source_name,
        sun_mask_source_name,
        ambient_size: secondary_size,
        sun_mask_size: primary_size,
    })
}

pub(crate) fn secondary_page_payloads(
    source: &[u8],
    row_bytes: usize,
    page_height: usize,
) -> Result<(Vec<u8>, Vec<u8>), WorldLightmapGap> {
    let page_bytes = row_bytes * page_height;
    let needed = page_bytes * 2;
    let source = source.get(..needed).ok_or(WorldLightmapGap::Truncated {
        needed,
        have: source.len(),
    })?;
    let mut ambient = source[..page_bytes].to_vec();
    let mut directional = source[page_bytes..].to_vec();
    for pixel in ambient
        .chunks_exact_mut(4)
        .chain(directional.chunks_exact_mut(4))
    {
        pixel.swap(0, 2);
    }
    Ok((ambient, directional))
}

pub(crate) fn primary_mask_payload(
    source: &[u8],
    width: u16,
    height: u16,
) -> Result<&[u8], WorldLightmapGap> {
    let needed = usize::from(width) * usize::from(height);
    source.get(..needed).ok_or(WorldLightmapGap::Truncated {
        needed,
        have: source.len(),
    })
}

fn extract_primary_lights(
    s: &ZoneStream<'_>,
    sun_primary_light_count: usize,
) -> Result<Vec<WorldPrimaryLight>, WorldMeshError> {
    let Some(world) = s.com_world() else {
        return Ok(Vec::new());
    };
    let Some(lights) = world.primary_lights else {
        return Ok(Vec::new());
    };
    let mut output = Vec::with_capacity(world.primary_light_count);
    for index in 0..world.primary_light_count {
        let light = lights.at(index * s.layout(sz::COM_PRIMARY_LIGHT, 72));
        let def_name = def_name_from_light(s, light);
        let (falloff_image_width, lmap_lookup_start, attenuation_sampler) =
            falloff_from_light_def(s, light);
        output.push(WorldPrimaryLight {
            is_sun: index != 0 && index <= sun_primary_light_count,
            light_type: s.u8_at(light, 0)?,
            can_cast_shadow: s.u8_at(light, 1)? != 0,
            exponent: s.u8_at(light, 2)?,
            color: [
                s.f32_at(light, 4)?,
                s.f32_at(light, 8)?,
                s.f32_at(light, 12)?,
            ],
            direction: [
                s.f32_at(light, 16)?,
                s.f32_at(light, 20)?,
                s.f32_at(light, 24)?,
            ],
            origin: [
                s.f32_at(light, 28)?,
                s.f32_at(light, 32)?,
                s.f32_at(light, 36)?,
            ],
            radius: s.f32_at(light, 40)?,
            cos_outer: s.f32_at(light, 44)?,
            cos_inner: s.f32_at(light, 48)?,
            cos_half_fov_expanded: s.f32_at(light, 0x34)?,
            def_name,
            falloff_image_width,
            lmap_lookup_start,

            attenuation_image: None,
            attenuation_sampler,
            t5_attenuation: None,
            t5_falloff: None,
            t5_a_ab_b: None,
            t5_angle: None,
            t5_cookie0: None,
            t5_cookie1: None,
            t5_cookie2: None,
            t5_diffuse_color: None,
            t5_specular_color: None,
        });
    }
    Ok(output)
}

fn def_name_from_light(s: &ZoneStream<'_>, light: Ptr) -> Option<String> {
    let ZonePtr::Offset(name_ptr) = s.ptr_at(light, 0x40).ok()? else {
        return None;
    };
    s.cstr(name_ptr).ok().map(str::to_owned)
}

fn falloff_from_light_def(s: &ZoneStream<'_>, light: Ptr) -> (Option<u16>, i32, u8) {
    let Some(want) = def_name_from_light(s, light) else {
        return (None, 0, 0);
    };
    for def in s.light_defs() {
        let Some(np) = def.name else {
            continue;
        };
        let Ok(n) = s.cstr(np) else {
            continue;
        };
        if crate::AssetRef::bare_name(n) == crate::AssetRef::bare_name(&want) {
            return (
                def.attenuation_width,
                def.lmap_lookup_start,
                def.attenuation_sampler,
            );
        }
    }
    (None, 0, 0)
}

pub fn capture_light_defs(
    s: &ZoneStream<'_>,
    materials: &MaterialCatalog,
) -> Vec<CapturedLightDef> {
    s.light_defs()
        .iter()
        .filter_map(|def| {
            let name = crate::AssetRef::decode(def.name.and_then(|p| s.cstr(p).ok())?);
            if name.is_empty() {
                return None;
            }
            let attenuation_image_name = def
                .attenuation_image
                .and_then(|ptr| {
                    materials
                        .image_index(ptr)
                        .and_then(|i| materials.images.get(i).map(|image| image.name.to_string()))
                        .or_else(|| image_name_at(s, ptr))
                })
                .or_else(|| {
                    def.attenuation_image_name
                        .and_then(|p| s.cstr(p).ok().map(str::to_owned))
                });
            Some(CapturedLightDef {
                namespace: crate::AssetNamespace::Iw4,
                name,
                attenuation_image_name,
                attenuation_width: def.attenuation_width,
                attenuation_sampler: def.attenuation_sampler,
                lmap_lookup_start: def.lmap_lookup_start,
            })
        })
        .collect()
}

fn image_name_at(s: &ZoneStream<'_>, img: Ptr) -> Option<String> {
    match s.ptr_at(img, s.layout(28, 32)) {
        Ok(ZonePtr::Offset(n)) => s.cstr(s.resolve_alias(n)).ok().map(str::to_owned),
        _ => None,
    }
}

pub(crate) fn find_light_def<'a>(
    name: &str,
    map_defs: &'a [CapturedLightDef],
    common_defs: &'a [CapturedLightDef],
) -> Option<&'a CapturedLightDef> {
    let want = crate::AssetRef::bare_name(name);
    if want.is_empty() {
        return None;
    }
    let pick = |defs: &'a [CapturedLightDef], imaged: bool| {
        let named = |def: &&CapturedLightDef| {
            def.name.as_str() == want && (!imaged || def.attenuation_image_name.is_some())
        };
        defs.iter()
            .rev()
            .find(|def| def.name.is_real() && named(def))
            .or_else(|| defs.iter().rev().find(named))
    };
    pick(map_defs, true)
        .or_else(|| pick(common_defs, true))
        .or_else(|| pick(map_defs, false))
        .or_else(|| pick(common_defs, false))
}

pub fn resolve_primary_light_attenuation(
    draw: &mut WorldDraw,
    materials: &MaterialDefinitions,
    common_defs: &[CapturedLightDef],
) {
    for i in 0..draw.primary_lights.len() {
        let Some(name) = draw.primary_lights[i].def_name.clone() else {
            continue;
        };
        let Some(def) = find_light_def(&name, &draw.light_defs, common_defs).cloned() else {
            continue;
        };
        apply_captured_def(&mut draw.primary_lights[i], &def, materials);
    }
}

pub fn resolve_outdoor_image(
    world_ptr_name: Option<&str>,
    map_namespace: crate::AssetNamespace,
    catalog: &MaterialDefinitions,
) -> (Option<usize>, &'static str) {
    if let Some(name) = world_ptr_name
        && let Some(index) = catalog.image_index_by_key(map_namespace, name)
    {
        return (Some(index), "gfxworld+412");
    }
    match catalog.image_index_by_key(map_namespace, "$outdoor") {
        Some(index) => (Some(index), "$outdoor"),
        None => (None, "missing"),
    }
}

fn apply_captured_def(
    light: &mut WorldPrimaryLight,
    def: &CapturedLightDef,
    catalog: &MaterialDefinitions,
) {
    light.lmap_lookup_start = def.lmap_lookup_start;
    light.attenuation_sampler = def.attenuation_sampler;
    light.attenuation_image = def
        .attenuation_image_name
        .as_deref()
        .and_then(|image_name| catalog.image_index_by_key(def.namespace, image_name));

    light.falloff_image_width = light
        .attenuation_image
        .and_then(|index| {
            let image = &catalog.images[index];
            image
                .decoded
                .as_ref()
                .and_then(|decoded| u16::try_from(decoded.texture_descriptor.size.width).ok())
                .or(Some(image.width))
        })
        .filter(|&width| width != 0)
        .or(def.attenuation_width.filter(|&width| width != 0));
}

fn extract_light_regions(
    s: &ZoneStream<'_>,
    geometry: GfxWorldGeometry,
) -> Result<Option<Vec<Vec<WorldLightRegionHull>>>, WorldMeshError> {
    let Some(regions) = geometry.light_regions else {
        return Ok(None);
    };
    let mut out = Vec::with_capacity(geometry.primary_light_count);
    for i in 0..geometry.primary_light_count {
        let region = regions.at(i * s.layout(sz::GFX_LIGHT_REGION, 16));
        let hull_count = s.u32_at(region, 0)? as usize;
        let mut hulls = Vec::with_capacity(hull_count);
        if hull_count > 0 {
            let ZonePtr::Offset(arr) = s.ptr_at(region, s.layout(4, 8))? else {
                return Err(WorldMeshError::NoGeometry);
            };
            for j in 0..hull_count {
                let hull = arr.at(j * s.layout(sz::GFX_LIGHT_REGION_HULL, 88));
                let mut kdop_mid = [0.0_f32; 9];
                let mut kdop_half = [0.0_f32; 9];
                for k in 0..9 {
                    kdop_mid[k] = s.f32_at(hull, k * 4)?;
                    kdop_half[k] = s.f32_at(hull, 36 + k * 4)?;
                }
                let axis_count = s.u32_at(hull, 72)? as usize;
                let mut axes = Vec::with_capacity(axis_count);
                if axis_count > 0 {
                    let ZonePtr::Offset(axis_arr) = s.ptr_at(hull, s.layout(76, 80))? else {
                        return Err(WorldMeshError::NoGeometry);
                    };
                    for k in 0..axis_count {
                        let a = axis_arr.at(k * 20);
                        axes.push(lighting_iw4::LightRegionAxis {
                            dir: [s.f32_at(a, 0)?, s.f32_at(a, 4)?, s.f32_at(a, 8)?],
                            mid: s.f32_at(a, 12)?,
                            half: s.f32_at(a, 16)?,
                        });
                    }
                }
                hulls.push(WorldLightRegionHull {
                    kdop_mid,
                    kdop_half,
                    axes,
                });
            }
        }
        out.push(hulls);
    }
    Ok(Some(out))
}

fn extract_shadow_geometry(
    s: &ZoneStream<'_>,
    geometry: GfxWorldGeometry,
) -> Result<Vec<WorldShadowGeometry>, WorldMeshError> {
    let Some(shadows) = geometry.shadow_geometry else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(geometry.primary_light_count);
    for i in 0..geometry.primary_light_count {
        let row = shadows.at(i * s.layout(sz::GFX_SHADOW_GEOMETRY, 24));
        let surf_n = s.u16_at(row, 0)? as usize;
        let smodel_n = s.u16_at(row, 2)? as usize;
        let mut surfaces = Vec::with_capacity(surf_n);
        if surf_n > 0 {
            let ZonePtr::Offset(arr) = s.ptr_at(row, s.layout(4, 8))? else {
                return Err(WorldMeshError::NoGeometry);
            };
            for j in 0..surf_n {
                surfaces.push(s.u16_at(arr, j * 2)?);
            }
        }
        let mut smodels = Vec::with_capacity(smodel_n);
        if smodel_n > 0 {
            let ZonePtr::Offset(arr) = s.ptr_at(row, s.layout(8, 16))? else {
                return Err(WorldMeshError::NoGeometry);
            };
            for j in 0..smodel_n {
                smodels.push(s.u16_at(arr, j * 2)?);
            }
        }
        out.push(WorldShadowGeometry { surfaces, smodels });
    }
    Ok(out)
}

fn extract_reflection_probes(
    s: &ZoneStream<'_>,
    geometry: GfxWorldGeometry,
    materials: &MaterialCatalog,
) -> Result<Vec<WorldReflectionProbe>, WorldMeshError> {
    let (Some(images), Some(origins)) = (
        geometry.reflection_probes,
        geometry.reflection_probe_origins,
    ) else {
        return Ok(Vec::new());
    };
    let mut probes = Vec::with_capacity(geometry.reflection_probe_count);
    for i in 0..geometry.reflection_probe_count {
        let origin = origins.at(i * 12);
        probes.push(WorldReflectionProbe {
            image: materials.image_index(images.at(i * s.pointer_bytes())),
            lighting_sh: None,
            origin: [
                s.f32_at(origin, 0)?,
                s.f32_at(origin, 4)?,
                s.f32_at(origin, 8)?,
            ],
        });
    }
    Ok(probes)
}

fn read_cell_reflection_probes(
    s: &ZoneStream<'_>,
    cell: Ptr,
    count_off: usize,
    ptr_off: usize,
) -> Result<Vec<u8>, WorldMeshError> {
    let count = s.u8_at(cell, count_off)? as usize;
    let mut list = Vec::with_capacity(count);
    if count == 0 {
        return Ok(list);
    }
    match s.ptr_at(cell, ptr_off)? {
        ZonePtr::Offset(arr) => {
            for k in 0..count {
                list.push(s.u8_at(arr, k)?);
            }
        }
        _ => {}
    }
    Ok(list)
}

fn read_bounds(s: &ZoneStream<'_>, p: fastfile_iw4::Ptr) -> Result<Bounds, WorldMeshError> {
    Ok(Bounds::from_mid_half(
        [s.f32_at(p, 0)?, s.f32_at(p, 4)?, s.f32_at(p, 8)?],
        [s.f32_at(p, 12)?, s.f32_at(p, 16)?, s.f32_at(p, 20)?],
    ))
}

fn aabb_children_offset(s: &ZoneStream<'_>, node: Ptr) -> Result<i32, WorldMeshError> {
    let offset = s.i32_at(node, s.layout(40, 48))?;
    let stride = s.layout(sz::GFX_AABB_TREE, 56) as i32;
    if offset <= 0 {
        return Ok(offset);
    }
    if offset % stride != 0 {
        return Err(WorldMeshError::InvalidAabbChildrenOffset { offset, stride });
    }
    Ok(offset / stride * dpvs_iw4::AABB_NODE_STRIDE as i32)
}

fn sun_f32(raw: &[u8], off: usize) -> Option<f32> {
    let bytes = raw.get(off..off + 4)?;
    Some(f32::from_bits(u32::from_le_bytes(bytes.try_into().ok()?)))
}

fn sun_i32(raw: &[u8], off: usize) -> Option<i32> {
    let bytes = raw.get(off..off + 4)?;
    Some(i32::from_le_bytes(bytes.try_into().ok()?))
}

fn sun_name(bytes: &[u8], len: u8) -> Option<&str> {
    let len = usize::from(len);
    core::str::from_utf8(bytes.get(..len)?)
        .ok()
        .filter(|name| !name.is_empty())
}

fn resolve_sun_material(
    materials: &MaterialCatalog,
    slot: Ptr,
    header: Option<Ptr>,
    name: Option<&str>,
) -> Option<usize> {
    materials
        .material_index(slot)
        .or_else(|| header.and_then(|header| materials.material_index(header)))
        .map(|index| index.get())
        .or_else(|| {
            let name = name?;
            materials
                .materials
                .iter()
                .position(|material| material.name.as_ref() == name)
        })
}

fn extract_sun_effects(
    s: &ZoneStream<'_>,
    sun: Option<GfxSunEffectsGeometry>,
    materials: &MaterialCatalog,
) -> Result<Option<SunEffectsCapture>, WorldMeshError> {
    let Some(sun) = sun else {
        return Ok(None);
    };
    let sprite_name = sun_name(&sun.sprite_name, sun.sprite_name_len);
    let flare_name = sun_name(&sun.flare_name, sun.flare_name_len);
    let sprite_index = resolve_sun_material(materials, sun.sprite, sun.sprite_header, sprite_name);
    let flare_index = resolve_sun_material(materials, sun.flare, sun.flare_header, flare_name);
    if sun.raw[0] == 0 {
        return Ok(None);
    }
    let direction = [
        sun_f32(&sun.raw, s.layout(84, 96)).unwrap_or(0.0),
        sun_f32(&sun.raw, s.layout(88, 100)).unwrap_or(0.0),
        sun_f32(&sun.raw, s.layout(92, 104)).unwrap_or(0.0),
    ];
    let len_sq =
        direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2];
    if !len_sq.is_finite() || len_sq < 0.25 {
        return Ok(None);
    }
    let inv = 1.0 / len_sq.sqrt();
    let need =
        |off32: usize, off64: usize| sun_f32(&sun.raw, s.layout(off32, off64)).unwrap_or(0.0);
    let need_i =
        |off32: usize, off64: usize| sun_i32(&sun.raw, s.layout(off32, off64)).unwrap_or(0);
    Ok(Some(SunEffectsCapture {
        sprite_material: sprite_index,
        flare_material: flare_index,
        sprite_size: need(12, 24),
        flare_min_size: need(16, 28),
        flare_min_dot: need(20, 32),
        flare_max_size: need(24, 36),
        flare_max_dot: need(28, 40),
        flare_max_alpha: need(32, 44),
        flare_fade_in_ms: need_i(36, 48),
        flare_fade_out_ms: need_i(40, 52),
        blind_min_dot: need(44, 56),
        blind_max_dot: need(48, 60),
        blind_max_darken: need(52, 64),
        blind_fade_in_ms: need_i(56, 68),
        blind_fade_out_ms: need_i(60, 72),
        glare_min_dot: need(64, 76),
        glare_max_dot: need(68, 80),
        glare_max_lighten: need(72, 84),
        glare_fade_in_ms: need_i(76, 88),
        glare_fade_out_ms: need_i(80, 92),
        direction: [direction[0] * inv, direction[1] * inv, direction[2] * inv],
    }))
}

fn extract_dpvs(s: &ZoneStream<'_>, g: GfxWorldGeometry) -> Result<DpvsWorldData, WorldMeshError> {
    let mut out = DpvsWorldData::new(CameraSurfRanges::new(
        CameraSurfRange {
            kind: CameraRangeKind::LitOpaque,
            begin: g.lit_opaque_surfs_begin,
            end: g.lit_opaque_surfs_end,
        },
        vec![
            CameraSurfRange {
                kind: CameraRangeKind::LitTrans,
                begin: g.lit_trans_surfs_begin,
                end: g.lit_trans_surfs_end,
            },
            CameraSurfRange {
                kind: CameraRangeKind::Emissive,
                begin: g.emissive_surfs_begin,
                end: g.emissive_surfs_end,
            },
        ],
    ));
    out.cell_count = g.cell_count;
    out.lit_opaque_begin = g.lit_opaque_surfs_begin;
    out.lit_opaque_end = g.lit_opaque_surfs_end;
    out.emissive_surfs_begin = g.emissive_surfs_begin;
    out.emissive_surfs_end = g.emissive_surfs_end;
    out.static_surface_count = g.static_surface_count;
    out.static_surface_count_no_decal = g.static_surface_count_no_decal;

    if let (Some(planes_ptr), Some(nodes_ptr)) = (g.planes, g.nodes) {
        out.planes.reserve(g.plane_count);
        for i in 0..g.plane_count {
            let p = planes_ptr.at(i * sz::CPLANE);
            out.planes.push(CPlane {
                normal: [s.f32_at(p, 0)?, s.f32_at(p, 4)?, s.f32_at(p, 8)?],
                dist: s.f32_at(p, 12)?,
                r#type: s.u8_at(p, 16)?,
            });
        }
        out.nodes.reserve(g.node_count);
        for i in 0..g.node_count {
            out.nodes.push(s.u16_at(nodes_ptr, i * 2)?);
        }
    }

    if let Some(sorted_ptr) = g.sorted_surf_index {
        let n = g.static_surface_count + g.static_surface_count_no_decal;
        out.sorted_surf_index.reserve(n);
        for i in 0..n {
            out.sorted_surf_index.push(s.u16_at(sorted_ptr, i * 2)?);
        }
    }

    if let Some(bounds_ptr) = g.surfaces_bounds {
        out.surface_bounds.reserve(g.surface_count);
        for i in 0..g.surface_count {
            out.surface_bounds
                .push(read_bounds(s, bounds_ptr.at(i * sz::GFX_SURFACE_BOUNDS))?);
        }
    }
    if let Some(skies_ptr) = g.skies {
        for i in 0..g.sky_count {
            let sky = skies_ptr.at(i * s.layout(sz::GFX_SKY, 32));
            let count = s.i32_at(sky, 0)?.max(0) as usize;
            let ZonePtr::Offset(arr) = s.ptr_at(sky, s.layout(4, 8))? else {
                continue;
            };
            for j in 0..count {
                out.sky_start_surfs.push(s.u32_at(arr, j * 4)?);
            }
        }
    }
    if let Some(insts_ptr) = g.smodel_insts {
        out.smodel_bounds.reserve(g.smodel_count);
        for i in 0..g.smodel_count {
            out.smodel_bounds
                .push(read_bounds(s, insts_ptr.at(i * sz::GFX_STATIC_MODEL_INST))?);
        }
    }

    out.cell_roots = vec![SurfRange::default(); g.cell_count];
    out.aabb_trees = vec![Vec::new(); g.cell_count];
    out.aabb_smodel_indices = vec![Vec::new(); g.cell_count];
    if let (Some(counts_ptr), Some(trees_ptr)) = (g.aabb_tree_counts, g.aabb_trees) {
        for i in 0..g.cell_count {
            let count = s.i32_at(counts_ptr, i * 4)?.max(0) as usize;
            if count == 0 {
                continue;
            }
            let tree = match s.ptr_at(trees_ptr, i * s.pointer_bytes())? {
                ZonePtr::Offset(p) => p,
                _ => continue,
            };
            out.cell_roots[i] = SurfRange {
                start: s.u16_at(tree, 28)?,
                count: s.u16_at(tree, 26)?,
            };
            let mut nodes = Vec::with_capacity(count);
            let mut smodel_indices: Vec<u16> = Vec::with_capacity(count);
            for j in 0..count {
                let n = tree.at(j * s.layout(sz::GFX_AABB_TREE, 56));
                let smodel_count = s.u16_at(n, 34)?;
                let smodel_index_start = smodel_indices.len() as u32;
                let mut smodel_index_count = 0u16;
                if let ZonePtr::Offset(indexes) = s.ptr_at(n, s.layout(36, 40))? {
                    for k in 0..usize::from(smodel_count) {
                        smodel_indices.push(s.u16_at(indexes, k * 2)?);
                        smodel_index_count += 1;
                    }
                }

                nodes.push(AabbNodeView {
                    bounds: Bounds::from_mid_half(
                        [s.f32_at(n, 0)?, s.f32_at(n, 4)?, s.f32_at(n, 8)?],
                        [s.f32_at(n, 12)?, s.f32_at(n, 16)?, s.f32_at(n, 20)?],
                    ),
                    child_count: s.u16_at(n, 24)?,
                    children_offset: aabb_children_offset(s, n)?,
                    start_surf: s.u16_at(n, 28)?,
                    surface_count: s.u16_at(n, 26)?,
                    start_surf_no_decal: s.u16_at(n, 32)?,
                    surface_count_no_decal: s.u16_at(n, 30)?,
                    smodel_index_start,
                    smodel_index_count,
                });
            }
            out.aabb_trees[i] = nodes;
            out.aabb_smodel_indices[i] = smodel_indices;
        }
    }

    if let Some(cells_ptr) = g.cells {
        out.portals_per_cell = Vec::with_capacity(g.cell_count);
        out.cell_reflection_probes = Vec::with_capacity(g.cell_count);
        for i in 0..g.cell_count {
            let cell = cells_ptr.at(i * s.layout(sz::GFX_CELL, 56));
            let portal_count = s.i32_at(cell, 24)?.max(0) as usize;
            let mut cell_portals = Vec::with_capacity(portal_count);
            let portals_ptr = match s.ptr_at(cell, s.layout(28, 32))? {
                ZonePtr::Offset(p) => Some(p),
                ZonePtr::Null => None,
                _ => None,
            };
            if let Some(arr) = portals_ptr {
                for j in 0..portal_count {
                    let portal = arr.at(j * s.layout(sz::GFX_PORTAL, 80));
                    let plane = [
                        s.f32_at(portal, s.layout(12, 24))?,
                        s.f32_at(portal, s.layout(16, 28))?,
                        s.f32_at(portal, s.layout(20, 32))?,
                        s.f32_at(portal, s.layout(24, 36))?,
                    ];
                    let neighbor = s.u16_at(portal, s.layout(32, 48))?;
                    let vertex_count = s.u8_at(portal, s.layout(34, 50))? as usize;
                    let vert_start = out.portal_verts.len();
                    if let ZonePtr::Offset(verts) = s.ptr_at(portal, s.layout(28, 40))? {
                        for k in 0..vertex_count {
                            let v = verts.at(k * 12);
                            out.portal_verts.push([
                                s.f32_at(v, 0)?,
                                s.f32_at(v, 4)?,
                                s.f32_at(v, 8)?,
                            ]);
                        }
                    }
                    let hull_axis = [
                        [
                            s.f32_at(portal, s.layout(sz::GFX_PORTAL_HULL_AXIS, 52))?,
                            s.f32_at(portal, s.layout(sz::GFX_PORTAL_HULL_AXIS, 52) + 4)?,
                            s.f32_at(portal, s.layout(sz::GFX_PORTAL_HULL_AXIS, 52) + 8)?,
                        ],
                        [
                            s.f32_at(portal, s.layout(sz::GFX_PORTAL_HULL_AXIS, 52) + 12)?,
                            s.f32_at(portal, s.layout(sz::GFX_PORTAL_HULL_AXIS, 52) + 16)?,
                            s.f32_at(portal, s.layout(sz::GFX_PORTAL_HULL_AXIS, 52) + 20)?,
                        ],
                    ];
                    cell_portals.push(OwnedPortal {
                        plane,
                        neighbor,
                        vert_start,
                        vert_count: out.portal_verts.len() - vert_start,
                        hull_axis: Some(hull_axis),
                    });
                }
            }
            out.portals_per_cell.push(cell_portals);
            out.cell_reflection_probes.push(read_cell_reflection_probes(
                s,
                cell,
                s.layout(0x20, 40),
                s.layout(0x24, 48),
            )?);
        }
    }

    out.checked()
}
