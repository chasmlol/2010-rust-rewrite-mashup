use crate::{Iw4WireFormat, WirePointer};

pub const MAX_XFILE_COUNT: usize = 8;

pub const XFILE_BLOCK_TEMP: usize = 0;
pub const XFILE_BLOCK_PHYSICAL: usize = 1;
pub const XFILE_BLOCK_RUNTIME: usize = 2;
pub const XFILE_BLOCK_VIRTUAL: usize = 3;
pub const XFILE_BLOCK_LARGE: usize = 4;
pub const XFILE_BLOCK_CALLBACK: usize = 5;
pub const XFILE_BLOCK_VERTEX: usize = 6;
pub const XFILE_BLOCK_INDEX: usize = 7;

pub const PTR_SIZE: usize = 4;

const BLOCK_SHIFT: u32 = 28;
const OFFSET_MASK: u32 = 0x0FFF_FFFF;

const ZONE_PTR_FOLLOWING: u32 = 0xFFFF_FFFF;

const ZONE_PTR_INSERT: u32 = 0xFFFF_FFFE;

pub const XFILE_HEADER_LEN: usize = 8 + 4 * MAX_XFILE_COUNT;

pub const BLOCK_STACK_CAP: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockType {
    Temp,

    Normal,

    Runtime,
}

const BLOCK_TYPES: [BlockType; MAX_XFILE_COUNT] = [
    BlockType::Temp,
    BlockType::Normal,
    BlockType::Runtime,
    BlockType::Normal,
    BlockType::Normal,
    BlockType::Normal,
    BlockType::Normal,
    BlockType::Normal,
];

pub fn block_is_aliasable(block: u8) -> bool {
    matches!(BLOCK_TYPES.get(block as usize), Some(BlockType::Normal))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ZoneError {
    UnresolvedPointer(Ptr),
    InvalidWirePointer {
        raw: u64,
        format: Iw4WireFormat,
    },
    AmbiguousWireFormat,
    InvalidWireTable {
        x86: crate::WireTableError,
        x64: crate::WireTableError,
    },
    UnsupportedAssetLayout {
        format: Iw4WireFormat,
        ty: crate::AssetType,
    },
    Truncated {
        at: usize,
        needed: usize,
        len: usize,
    },
    BlockOverflow {
        block: usize,
        end: usize,
        size: usize,

        request: usize,
    },
    BadBlock(usize),
    BadOffset {
        block: usize,
        offset: usize,
        size: usize,
    },

    UnknownAssetType(u32),

    NoAssetLoader(crate::asset_type::AssetType),
    UnterminatedString {
        block: usize,
    },
    NotUtf8,
    StackOverflow,
    StackUnderflow,
    NoBlockPushed,

    InsertMapTooSmall {
        needed: usize,
        got: usize,
    },
}

impl core::fmt::Display for ZoneError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnresolvedPointer(slot) => write!(f, "unresolved pointer at {slot:?}"),
            Self::InvalidWirePointer { raw, format } => {
                write!(f, "invalid {format:?} pointer {raw:#x}")
            }
            Self::AmbiguousWireFormat => write!(f, "ambiguous IW4 wire format"),
            Self::InvalidWireTable { x86, x64 } => {
                write!(f, "invalid IW4 tables: x86 {x86:?}; x64 {x64:?}")
            }
            Self::UnsupportedAssetLayout { format, ty } => {
                write!(f, "unsupported {format:?} asset layout: {ty:?}")
            }
            ZoneError::Truncated { at, needed, len } => {
                write!(
                    f,
                    "zone truncated: needed {needed} bytes at {at}, have {len}"
                )
            }
            ZoneError::BlockOverflow {
                block,
                end,
                size,
                request,
            } => {
                write!(
                    f,
                    "block {block} overflow: end {end} exceeds size {size} (request {request})"
                )
            }
            ZoneError::BadBlock(b) => write!(f, "invalid block index {b} in zone pointer"),
            ZoneError::UnknownAssetType(v) => write!(f, "unknown asset pool id {v:#x}"),
            ZoneError::NoAssetLoader(ty) => {
                write!(f, "no zone walk implemented for asset type `{}`", ty.name())
            }
            ZoneError::BadOffset {
                block,
                offset,
                size,
            } => write!(
                f,
                "offset {offset} out of bounds in block {block} (size {size})"
            ),
            ZoneError::UnterminatedString { block } => {
                write!(f, "unterminated string in block {block}")
            }
            ZoneError::NotUtf8 => write!(f, "string is not valid UTF-8"),
            ZoneError::StackOverflow => write!(f, "block stack overflow"),
            ZoneError::StackUnderflow => write!(f, "block stack underflow"),
            ZoneError::NoBlockPushed => write!(f, "no block pushed"),
            ZoneError::InsertMapTooSmall { needed, got } => {
                write!(
                    f,
                    "insert-slot map too small: needed {needed} bytes, got {got}"
                )
            }
        }
    }
}

pub type Result<T> = core::result::Result<T, ZoneError>;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Ptr {
    pub block: u8,
    pub offset: u32,
}

impl Ptr {
    pub fn at(self, delta: usize) -> Ptr {
        Ptr {
            block: self.block,
            offset: self.offset.wrapping_add(delta as u32),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ZonePtr {
    Null,

    Following,

    Insert,

    Offset(Ptr),
}

impl ZonePtr {
    pub fn decode(v: u32) -> ZonePtr {
        match v {
            0 => ZonePtr::Null,
            ZONE_PTR_FOLLOWING => ZonePtr::Following,
            ZONE_PTR_INSERT => ZonePtr::Insert,
            _ => {
                let e = v - 1;
                ZonePtr::Offset(Ptr {
                    block: (e >> BLOCK_SHIFT) as u8,
                    offset: e & OFFSET_MASK,
                })
            }
        }
    }

    pub fn encode_offset(p: Ptr) -> u32 {
        (((p.block as u32) << BLOCK_SHIFT) | (p.offset & OFFSET_MASK)).wrapping_add(1)
    }

    pub fn is_following(self) -> bool {
        matches!(self, ZonePtr::Following | ZonePtr::Insert)
    }

    pub fn is_null(self) -> bool {
        matches!(self, ZonePtr::Null)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxImageGeometry {
    pub name: Option<Ptr>,
    pub map_type: u8,
    pub semantic: u8,
    pub category: u8,
    pub use_srgb_reads: bool,
    pub width: u16,
    pub height: u16,
    pub depth: u16,
    pub level_count: u8,
    pub format: u32,

    pub source_offset: usize,
    pub source_len: usize,
}

pub const MAX_LIGHTMAP_PAGES: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxLightmapPair {
    pub primary: Option<GfxImageGeometry>,
    pub secondary: Option<GfxImageGeometry>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MaterialGeometry {
    pub name: Option<Ptr>,
    pub draw_surf: u64,
    pub sort_key: u8,

    pub info_game_flags: u8,

    pub texture_atlas: [u8; 2],

    pub surface_type_bits: Option<u32>,

    pub state_flags: u8,

    pub camera_region: u8,

    pub header: Option<Ptr>,

    pub state_bits: Option<Ptr>,

    pub state_bits_count: usize,

    pub state_bits_entry: Option<[u8; asset_iw4::size::TECHNIQUE_SLOT_COUNT]>,

    pub technique_set: Option<Ptr>,
    pub textures: Option<Ptr>,

    pub texture_stride: usize,
    pub texture_count: usize,

    pub constants: Option<Ptr>,
    pub constant_count: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShaderGeometry {
    pub name: Option<Ptr>,

    pub program: Option<Ptr>,
    pub program_words: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexDeclGeometry {
    pub name: Option<Ptr>,

    pub stream_count: u8,

    pub has_optional_source: u8,

    pub routing: [[u8; 2]; asset_iw4::vertex_decl::ROUTING_COUNT],
}

impl Default for VertexDeclGeometry {
    fn default() -> Self {
        Self {
            name: None,
            stream_count: 0,
            has_optional_source: 0,
            routing: [[0; 2]; asset_iw4::vertex_decl::ROUTING_COUNT],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TechniqueSetGeometry {
    pub name: Option<Ptr>,

    pub technique_slots: u64,

    pub technique_slots_scanned: u64,

    pub technique_body_by_slot: [Option<Ptr>; asset_iw4::size::TECHNIQUE_SLOT_COUNT],

    pub technique0_flags: u8,

    pub world_vert_format: u8,

    pub uses_model_lighting_const: bool,

    pub max_pass_count: u16,

    pub pass_count_by_slot: [u8; asset_iw4::size::TECHNIQUE_SLOT_COUNT],

    pub technique_flags_by_slot: [u16; asset_iw4::size::TECHNIQUE_SLOT_COUNT],
}

impl Default for TechniqueSetGeometry {
    fn default() -> Self {
        Self {
            name: None,
            technique_slots: 0,
            technique_slots_scanned: 0,
            technique_body_by_slot: [None; asset_iw4::size::TECHNIQUE_SLOT_COUNT],
            technique0_flags: 0,
            world_vert_format: 0,
            uses_model_lighting_const: false,
            max_pass_count: 0,
            pass_count_by_slot: [0; asset_iw4::size::TECHNIQUE_SLOT_COUNT],
            technique_flags_by_slot: [0; asset_iw4::size::TECHNIQUE_SLOT_COUNT],
        }
    }
}

pub const TECHNIQUE_PASS_ROW_CAP: usize = 192;

pub const TECHNIQUE_ARGUMENT_CAP: usize = 1536;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TechniqueArgumentGeometry {
    pub raw: [u8; 8],
    pub literal_words: [u32; 4],
    pub literal_present: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TechniquePassGeometry {
    pub tech_slot: u8,
    pub pass_index: u8,
    pub technique_flags: u16,
    pub vertex_decl_slot: Ptr,
    pub vertex_shader_slot: Ptr,
    pub pixel_shader_slot: Ptr,
    pub per_prim_arg_count: u8,
    pub per_obj_arg_count: u8,
    pub stable_arg_count: u8,

    pub custom_sampler_flags: u8,
    pub argument_start: u16,
    pub argument_count: u16,
    pub arguments_truncated: bool,
}

impl Default for TechniquePassGeometry {
    fn default() -> Self {
        Self {
            tech_slot: 0,
            pass_index: 0,
            technique_flags: 0,
            vertex_decl_slot: Ptr {
                block: 0,
                offset: 0,
            },
            vertex_shader_slot: Ptr {
                block: 0,
                offset: 0,
            },
            pixel_shader_slot: Ptr {
                block: 0,
                offset: 0,
            },
            per_prim_arg_count: 0,
            per_obj_arg_count: 0,
            stable_arg_count: 0,
            custom_sampler_flags: 0,
            argument_start: 0,
            argument_count: 0,
            arguments_truncated: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TechniqueGraphGeometry {
    pub row_count: u16,
    pub rows: [TechniquePassGeometry; TECHNIQUE_PASS_ROW_CAP],
    pub argument_count: u16,
    pub arguments: [TechniqueArgumentGeometry; TECHNIQUE_ARGUMENT_CAP],
    pub rows_truncated: u16,
    pub arguments_truncated: u16,
}

impl Default for TechniqueGraphGeometry {
    fn default() -> Self {
        Self {
            row_count: 0,
            rows: [TechniquePassGeometry::default(); TECHNIQUE_PASS_ROW_CAP],
            argument_count: 0,
            arguments: [TechniqueArgumentGeometry::default(); TECHNIQUE_ARGUMENT_CAP],
            rows_truncated: 0,
            arguments_truncated: 0,
        }
    }
}

impl TechniqueGraphGeometry {
    pub fn iter_rows(&self) -> impl Iterator<Item = TechniquePassGeometry> + '_ {
        self.rows[..self.row_count as usize].iter().copied()
    }

    pub fn arguments_for(&self, row: TechniquePassGeometry) -> &[TechniqueArgumentGeometry] {
        let start = row.argument_start as usize;
        let end = start.saturating_add(row.argument_count as usize);
        self.arguments.get(start..end).unwrap_or(&[])
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComWorldGeometry {
    pub primary_lights: Option<Ptr>,
    pub primary_light_count: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxLightDefGeometry {
    pub name: Option<Ptr>,
    pub lmap_lookup_start: i32,

    pub attenuation_width: Option<u16>,

    pub attenuation_image: Option<Ptr>,

    pub attenuation_image_name: Option<Ptr>,

    pub attenuation_sampler: u8,
}

pub const MAX_LIGHT_DEFS: usize = 128;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxLightGridGeometry {
    pub has_light_regions: bool,

    pub sun_primary_light_index: u32,
    pub mins: [u16; 3],
    pub maxs: [u16; 3],
    pub row_axis: u32,
    pub col_axis: u32,
    pub row_data_start: Option<Ptr>,
    pub row_count: usize,
    pub raw_row_data: Option<Ptr>,
    pub raw_row_data_size: usize,
    pub entries: Option<Ptr>,
    pub entry_count: usize,
    pub colors: Option<Ptr>,
    pub color_count: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxWorldGeometry {
    pub sort_key_distortion: Option<u32>,
    pub vertices: Option<Ptr>,
    pub vertex_count: usize,
    pub indices: Option<Ptr>,
    pub index_count: usize,

    pub surfaces: Option<Ptr>,
    pub surface_count: usize,

    pub lightmap_count: usize,
    pub lightmaps: [GfxLightmapPair; MAX_LIGHTMAP_PAGES],

    pub first_lightmap_primary: Option<GfxImageGeometry>,
    pub first_lightmap_secondary: Option<GfxImageGeometry>,

    pub first_sky_image: Option<Ptr>,

    pub skies: Option<Ptr>,

    pub sky_count: usize,

    pub outdoor_image: Option<Ptr>,

    pub outdoor_lookup: [u32; 16],

    pub reflection_probes: Option<Ptr>,
    pub reflection_probe_origins: Option<Ptr>,
    pub reflection_probe_count: usize,
    pub light_grid: GfxLightGridGeometry,

    pub sun_primary_light_count: usize,

    pub primary_light_count: usize,

    pub cell_count: usize,

    pub plane_count: usize,

    pub node_count: usize,

    pub portal_count: usize,

    pub aabb_node_count: usize,

    pub planes: Option<Ptr>,

    pub nodes: Option<Ptr>,

    pub cells: Option<Ptr>,

    pub aabb_tree_counts: Option<Ptr>,

    pub aabb_trees: Option<Ptr>,

    pub dpvs_static: Option<Ptr>,
    pub smodel_count: usize,
    pub static_surface_count: usize,
    pub static_surface_count_no_decal: usize,

    pub lit_opaque_surfs_begin: u32,
    pub lit_opaque_surfs_end: u32,
    pub lit_trans_surfs_begin: u32,
    pub lit_trans_surfs_end: u32,
    pub shadow_caster_surfs_begin: u32,
    pub shadow_caster_surfs_end: u32,
    pub emissive_surfs_begin: u32,
    pub emissive_surfs_end: u32,
    pub sorted_surf_index: Option<Ptr>,
    pub smodel_insts: Option<Ptr>,

    pub surfaces_bounds: Option<Ptr>,
    pub smodel_draw_insts: Option<Ptr>,

    pub dyn_model_count: usize,

    pub dyn_brush_count: usize,

    pub light_regions: Option<Ptr>,

    pub shadow_geometry: Option<Ptr>,

    pub model_count: usize,

    pub models: Option<Ptr>,

    pub bounds: Option<[u32; 6]>,

    /// Owned sunflare bytes and material slots. Copied during load; not a live zone pointer.
    pub sun_effects: Option<GfxSunEffectsGeometry>,
}

/// Authored sunflare copied while `GfxWorld` is still intact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GfxSunEffectsGeometry {
    pub sprite: Ptr,
    pub flare: Ptr,
    pub sprite_header: Option<Ptr>,
    pub flare_header: Option<Ptr>,
    pub sprite_name: [u8; 32],
    pub sprite_name_len: u8,
    pub flare_name: [u8; 32],
    pub flare_name_len: u8,
    pub raw: [u8; 112],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FxWorldGeometry {
    pub glass_sys: Option<Ptr>,
    pub def_count: usize,
    pub piece_limit: usize,
    pub init_piece_count: usize,
    pub init_geo_count: usize,

    pub geo_data_limit: usize,

    pub piece_word_count: usize,

    pub cell_count: usize,

    pub defs: Option<Ptr>,

    pub init_piece_indices: Option<Ptr>,

    pub init_piece_states: Option<Ptr>,

    pub init_geo_data: Option<Ptr>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GlassDataGeometry {
    pub data: Option<Ptr>,
    pub piece_count: usize,
    pub name_count: usize,

    pub pieces: Option<Ptr>,

    pub names: Option<Ptr>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapEntsGeometry {
    pub trigger_models: Option<Ptr>,
    pub trigger_model_count: usize,
    pub trigger_hulls: Option<Ptr>,
    pub trigger_hull_count: usize,
    pub trigger_slabs: Option<Ptr>,
    pub trigger_slab_count: usize,
    pub entity_string: Option<Ptr>,
    pub entity_chars: usize,
    pub stages: Option<Ptr>,
    pub stage_count: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClipMapGeometry {
    pub name: Option<Ptr>,
    pub plane_count: usize,
    pub static_model_count: usize,

    pub static_models: Option<Ptr>,
    pub material_count: usize,
    pub brush_side_count: usize,
    pub node_count: usize,
    pub leaf_count: usize,
    pub leafbrush_node_count: usize,
    pub brush_count: usize,
    pub cmodel_count: usize,
    pub vert_count: usize,
    pub tri_count: usize,

    pub planes: Option<Ptr>,

    pub materials: Option<Ptr>,

    pub brush_sides: Option<Ptr>,

    pub brushes: Option<Ptr>,

    pub brush_bounds: Option<Ptr>,

    pub brush_contents: Option<Ptr>,

    pub nodes: Option<Ptr>,

    pub leaves: Option<Ptr>,

    pub leafbrushes: Option<Ptr>,
    pub leafbrush_count: usize,

    pub leafbrush_nodes: Option<Ptr>,

    pub verts: Option<Ptr>,

    pub tri_indices: Option<Ptr>,

    pub tri_edge_is_walkable: Option<Ptr>,

    pub collision_partitions: Option<Ptr>,
    pub partition_count: usize,

    pub collision_aabb_trees: Option<Ptr>,
    pub aabb_tree_count: usize,

    pub cmodels: Option<Ptr>,

    pub dyn_ent_count: [usize; 2],

    pub dyn_ent_defs: [Option<Ptr>; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysPresetGeometry {
    pub header: Ptr,

    pub name: Option<Ptr>,

    pub snd_alias_prefix: Option<Ptr>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct XModelGeometry {
    pub name: Option<Ptr>,

    pub material_handles: Option<Ptr>,

    pub material_handle_count: usize,

    pub surfaces: Option<Ptr>,
    pub surface_count: usize,

    pub lod_xsurfaces: [Option<Ptr>; 4],

    pub lod_surface_names: [Option<Ptr>; 4],

    pub lod_numsurfs: [u16; 4],

    pub lod_surf_index: [u16; 4],

    pub num_bones: usize,

    pub num_root_bones: usize,

    pub scale: f32,

    pub no_scale_part_bits: [u32; 6],

    pub bone_names: Option<Ptr>,

    pub parent_list: Option<Ptr>,

    pub quats: Option<Ptr>,

    pub trans: Option<Ptr>,

    pub base_mat: Option<Ptr>,

    pub part_classification: Option<Ptr>,

    pub bone_info: Option<Ptr>,

    pub coll_surfs: Option<Ptr>,

    pub num_coll_surfs: i32,

    pub coll_lod: i16,

    pub lod_dist: [f32; 4],

    pub lod_part_bits: [[u32; 6]; 4],

    pub lod_smc: [[u8; 4]; 4],

    pub lod_start: u8,

    pub num_lods: u8,

    pub contents: u32,

    pub radius: Option<f32>,

    pub bounds_mid: Option<[f32; 3]>,
    pub bounds_half: Option<[f32; 3]>,

    pub phys_preset: Option<Ptr>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxEffectDefGeometry {
    pub header: Ptr,
    pub name: Option<Ptr>,
    pub looping_count: i32,
    pub one_shot_count: i32,
    pub emission_count: i32,
    pub elem_defs: Option<Ptr>,
    pub elem_def_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxImpactTableGeometry {
    pub header: Ptr,
    pub name: Option<Ptr>,

    pub entries: Ptr,
    pub row_count: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct XAnimPartsGeometry {
    pub name: Option<Ptr>,
    pub numframes: u16,
    pub flags: u8,
    pub bone_count: [u8; 10],
    pub notify_count: usize,
    pub framerate: f32,
    pub frequency: f32,
    pub names: Option<Ptr>,
    pub notify: Option<Ptr>,
    pub data_byte: Option<Ptr>,
    pub data_byte_count: usize,
    pub data_short: Option<Ptr>,
    pub data_short_count: usize,
    pub data_int: Option<Ptr>,
    pub data_int_count: usize,
    pub random_data_short: Option<Ptr>,
    pub random_data_short_count: usize,
    pub random_data_byte: Option<Ptr>,
    pub random_data_byte_count: usize,
    pub random_data_int: Option<Ptr>,
    pub random_data_int_count: usize,
    pub indices: Option<Ptr>,
    pub index_count: usize,
    pub indices_are_bytes: bool,

    pub delta_trans: XAnimDeltaTransGeometry,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct XAnimDeltaTransGeometry {
    pub size: u16,
    pub small: u8,

    pub constant: Option<Ptr>,

    pub mins_step: Option<Ptr>,
    pub frames: Option<Ptr>,
    pub indices: Option<Ptr>,
    pub indices_are_bytes: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponKickCapture {
    pub f_ads_view_kick_center_speed: f32,

    pub f_hip_view_kick_center_speed: f32,
    pub gun_max_pitch: f32,
    pub gun_max_yaw: f32,
    pub ads_gun_kick_reduced_kick_bullets: i32,
    pub ads_gun_kick_reduced_kick_percent: f32,
    pub ads_gun_kick_pitch_min: f32,
    pub ads_gun_kick_pitch_max: f32,
    pub ads_gun_kick_yaw_min: f32,
    pub ads_gun_kick_yaw_max: f32,
    pub ads_gun_kick_accel: f32,
    pub ads_gun_kick_speed_max: f32,
    pub ads_gun_kick_speed_decay: f32,
    pub ads_gun_kick_static_decay: f32,
    pub ads_view_kick_pitch_min: f32,
    pub ads_view_kick_pitch_max: f32,
    pub ads_view_kick_yaw_min: f32,
    pub ads_view_kick_yaw_max: f32,
    pub hip_gun_kick_reduced_kick_bullets: i32,
    pub hip_gun_kick_reduced_kick_percent: f32,
    pub hip_gun_kick_pitch_min: f32,
    pub hip_gun_kick_pitch_max: f32,
    pub hip_gun_kick_yaw_min: f32,
    pub hip_gun_kick_yaw_max: f32,
    pub hip_gun_kick_accel: f32,
    pub hip_gun_kick_speed_max: f32,
    pub hip_gun_kick_speed_decay: f32,
    pub hip_gun_kick_static_decay: f32,
    pub hip_view_kick_pitch_min: f32,
    pub hip_view_kick_pitch_max: f32,
    pub hip_view_kick_yaw_min: f32,
    pub hip_view_kick_yaw_max: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponSwayCapture {
    pub sway_max_angle: f32,
    pub sway_lerp_speed: f32,
    pub sway_pitch_scale: f32,
    pub sway_yaw_scale: f32,
    pub sway_horiz_scale: f32,
    pub sway_vert_scale: f32,

    pub sway_shell_shock_scale: f32,
    pub ads_sway_max_angle: f32,
    pub ads_sway_lerp_speed: f32,
    pub ads_sway_pitch_scale: f32,
    pub ads_sway_yaw_scale: f32,
    pub ads_sway_horiz_scale: f32,
    pub ads_sway_vert_scale: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponMovementOfsCapture {
    pub stand_move: [f32; 3],
    pub stand_rot: [f32; 3],
    pub strafe_move: [f32; 3],
    pub strafe_rot: [f32; 3],
    pub ducked_move: [f32; 3],
    pub ducked_rot: [f32; 3],
    pub prone_move: [f32; 3],
    pub prone_rot: [f32; 3],
    pub pos_move_rate: f32,
    pub pos_prone_move_rate: f32,
    pub stand_move_min_speed: f32,
    pub ducked_move_min_speed: f32,
    pub prone_move_min_speed: f32,
    pub pos_rot_rate: f32,
    pub pos_prone_rot_rate: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponIdleCapture {
    pub ads_idle_amount: f32,
    pub hip_idle_amount: f32,
    pub ads_idle_speed: f32,
    pub hip_idle_speed: f32,
    pub idle_crouch_factor: f32,
    pub idle_prone_factor: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponGeometry {
    pub name: Option<Ptr>,
    pub alternate_weapon_name: Option<Ptr>,
    pub alternate_raise_time_ms: i32,
    pub alternate_drop_time_ms: i32,
    pub first_raise_time_ms: i32,

    pub weap_def: Option<Ptr>,

    pub display_name: Option<Ptr>,

    pub gun_xmodel_name: Option<Ptr>,

    pub gun_xmodel_names: [Option<Ptr>; 16],

    pub hand_xmodel_name: Option<Ptr>,

    pub world_model_name: Option<Ptr>,

    pub world_model_names: [Option<Ptr>; 16],

    pub projectile_model_name: Option<Ptr>,

    pub rocket_model_name: Option<Ptr>,

    pub sz_xanims: Option<Ptr>,

    pub sz_xanims_right: Option<Ptr>,

    pub sz_xanims_left: Option<Ptr>,

    pub hide_tags: Option<Ptr>,

    pub notetrack_sound_keys: Option<Ptr>,

    pub notetrack_sound_values: Option<Ptr>,

    pub notetrack_rumble_keys: Option<Ptr>,

    pub notetrack_rumble_values: Option<Ptr>,

    pub fire_sound_name: Option<Ptr>,

    pub fire_sound_player_name: Option<Ptr>,

    pub fire_last_sound_name: Option<Ptr>,

    pub fire_last_sound_player_name: Option<Ptr>,

    pub empty_fire_sound_name: Option<Ptr>,

    pub empty_fire_sound_player_name: Option<Ptr>,

    pub melee_swipe_sound_name: Option<Ptr>,

    pub melee_swipe_sound_player_name: Option<Ptr>,

    pub melee_hit_sound_name: Option<Ptr>,

    pub melee_miss_sound_name: Option<Ptr>,

    pub pickup_sound_name: Option<Ptr>,

    pub pickup_sound_player_name: Option<Ptr>,

    pub ammo_pickup_sound_name: Option<Ptr>,

    pub ammo_pickup_sound_player_name: Option<Ptr>,

    pub pullback_sound_name: Option<Ptr>,

    pub pullback_sound_player_name: Option<Ptr>,

    pub reload_sound_name: Option<Ptr>,

    pub reload_sound_player_name: Option<Ptr>,

    pub reload_empty_sound_name: Option<Ptr>,

    pub reload_empty_sound_player_name: Option<Ptr>,

    pub reload_start_sound_name: Option<Ptr>,

    pub reload_start_sound_player_name: Option<Ptr>,

    pub reload_end_sound_name: Option<Ptr>,

    pub reload_end_sound_player_name: Option<Ptr>,

    pub rechamber_sound_name: Option<Ptr>,

    pub rechamber_sound_player_name: Option<Ptr>,

    pub alt_switch_sound_name: Option<Ptr>,

    pub alt_switch_sound_player_name: Option<Ptr>,

    pub raise_sound_name: Option<Ptr>,

    pub raise_sound_player_name: Option<Ptr>,

    pub first_raise_sound_name: Option<Ptr>,

    pub first_raise_sound_player_name: Option<Ptr>,

    pub putaway_sound_name: Option<Ptr>,

    pub putaway_sound_player_name: Option<Ptr>,

    pub proj_explosion_sound_name: Option<Ptr>,

    pub projectile_sound_name: Option<Ptr>,

    pub proj_ignition_sound_name: Option<Ptr>,

    pub bounce_sound_names: [Option<Ptr>; asset_iw4::size::SURF_TYPE_NUM],

    pub fire_time_ms: i32,

    pub ads_zoom_fov: f32,

    pub ads_dof: [f32; 2],

    pub impact_type: i32,

    pub raise_time_ms: i32,

    pub drop_time_ms: i32,

    pub fire_delay_ms: i32,

    pub hold_fire_time_ms: i32,

    pub weap_type: i32,

    pub player_anim_type: i32,

    pub weap_class: i32,

    pub offhand_class: i32,

    pub shots_per_fire: i32,

    pub ammo_index: i32,

    pub clip_index: i32,

    pub ammo_counter_clip: i32,

    pub low_ammo_warning_threshold: f32,

    pub hip_spread_stand_min: f32,

    pub hip_spread_ducked_min: f32,

    pub hip_spread_prone_min: f32,

    pub hip_spread_stand_max: f32,

    pub hip_spread_ducked_max: f32,

    pub hip_spread_prone_max: f32,

    pub hip_spread_decay_rate: f32,

    pub hip_spread_fire_add: f32,

    pub hip_spread_turn_add: f32,

    pub hip_spread_move_add: f32,

    pub hip_spread_ducked_decay: f32,

    pub hip_spread_prone_decay: f32,

    pub reticle_center_material_slot: Option<Ptr>,

    pub reticle_side_material_slot: Option<Ptr>,

    pub overlay_material_slot: Option<Ptr>,

    pub hud_icon_slot: Option<Ptr>,

    pub pickup_icon_slot: Option<Ptr>,
    pub pickup_icon_ratio: i32,
    pub hud_icon_ratio: i32,

    pub kill_icon_slot: Option<Ptr>,

    pub kill_icon_name: Option<Ptr>,
    pub dpad_icon_name: Option<Ptr>,
    pub dpad_icon_ratio: i32,

    pub motion_tracker: bool,

    pub proj_trail_slot: Option<Ptr>,

    pub proj_beacon_slot: Option<Ptr>,

    pub proj_ignition_slot: Option<Ptr>,

    pub reticle_center_size: i32,

    pub i_reticle_side_size: i32,

    pub i_reticle_min_ofs: i32,

    pub hip_reticle_side_pos: f32,

    pub ads_aim_pitch: f32,

    pub ads_crosshair_in_frac: f32,

    pub ads_crosshair_out_frac: f32,

    pub ads_spread: f32,

    pub aim_down_sight: bool,

    pub thermal_scope: bool,

    pub no_ads_when_mag_empty: bool,

    pub inherits_perks: bool,

    pub ads_reload_trans_time_ms: i32,
    pub ads_in_rate: f32,

    pub ads_out_rate: f32,

    pub rechamber_while_ads: bool,

    pub ads_fire_only: bool,

    pub dual_wield_view_model_offset: f32,

    pub no_dual_wield: bool,

    pub melee_damage: i32,

    pub overlay_reticle: i32,
    pub overlay_interface: i32,

    pub ads_zoom_in_frac: f32,

    pub ads_zoom_out_frac: f32,

    pub ads_overlay_width: f32,

    pub ads_overlay_height: f32,

    pub melee_time_ms: i32,

    pub melee_delay_ms: i32,

    pub melee_charge_time_ms: i32,

    pub melee_charge_delay_ms: i32,

    pub knife_model: u32,

    pub quick_raise_time_ms: i32,

    pub quick_drop_time_ms: i32,

    pub select_requires_ammo: Option<bool>,

    pub offhand_hold_is_cancelable: Option<bool>,

    pub move_speed_scale: f32,

    pub ads_move_speed_scale: f32,

    pub sprint_duration_scale: f32,

    pub ducked_ofs: [f32; 3],

    pub prone_ofs: [f32; 3],

    pub night_vision_wear_time: i32,

    pub ads_bob_factor: f32,

    pub ads_view_bob_mult: f32,

    pub movement: WeaponMovementOfsCapture,

    pub idle: WeaponIdleCapture,

    pub clip_size: i32,

    pub penetrate_type: i32,

    pub penetrate_multiplier: f32,

    pub rifle_bullet: bool,
    pub ricochet_chance: f32,
    pub explosive_bullet: bool,

    pub inventory_type: i32,

    pub fire_type: i32,

    pub max_ammo: i32,

    pub damage: i32,

    pub rechamber_time_ms: i32,

    pub rechamber_bolt_time_ms: i32,

    pub rechamber_bolt_delay_ms: i32,

    pub reload_time_ms: i32,

    pub reload_show_rocket_time_ms: i32,

    pub reload_empty_time_ms: i32,

    pub reload_add_time_ms: i32,

    pub reload_start_time_ms: i32,

    pub reload_start_add_time_ms: i32,

    pub reload_end_time_ms: i32,

    pub kill_icon_ratio: i32,

    pub flip_kill_icon: bool,

    pub reload_ammo_add: i32,

    pub reload_start_add: i32,

    pub no_partial_reload: bool,

    pub bolt_action: bool,

    pub segmented_reload: bool,

    pub sprint_raise_time_ms: i32,

    pub sprint_loop_time_ms: i32,

    pub sprint_drop_time_ms: i32,
    pub stunned_start_time_ms: i32,
    pub stunned_end_time_ms: i32,

    pub fuse_time_ms: i32,
    pub auto_aim_range: f32,
    pub aim_assist_range: f32,
    pub aim_assist_range_ads: f32,

    pub cook_off_hold: bool,

    pub clip_only: bool,

    pub has_detonator: bool,
    pub detonate_delay_ms: i32,
    pub detonate_time_ms: i32,
    pub projectile_rotates: bool,
    pub timed_detonation: bool,

    pub proj_impact_explode: bool,

    pub stick_to_players: bool,

    pub explosion_radius: i32,
    pub explosion_radius_min: i32,

    pub explosion_inner_damage: i32,
    pub explosion_outer_damage: i32,
    pub damage_cone_angle: f32,

    pub missile_guidance: i32,
    pub ignition_delay_ms: i32,
    pub require_lock_to_fire: bool,
    pub stickiness: i32,
    pub projectile_speed: i32,
    pub projectile_speed_up: i32,
    pub projectile_speed_forward: i32,

    pub projectile_activate_dist: i32,

    pub projectile_explosion_type: i32,

    pub parallel_bounce: Option<[f32; 31]>,
    pub perpendicular_bounce: Option<[f32; 31]>,
    pub location_damage_mult: Option<[f32; 20]>,

    pub start_ammo: i32,

    pub min_damage: i32,

    pub min_player_damage: i32,

    pub max_damage_range: f32,

    pub min_damage_range: f32,

    pub kick: WeaponKickCapture,

    pub sway: WeaponSwayCapture,

    pub view_flash_slot: Option<Ptr>,

    pub world_flash_slot: Option<Ptr>,

    pub view_shell_eject_slot: Option<Ptr>,

    pub world_shell_eject_slot: Option<Ptr>,

    pub view_last_shot_eject_slot: Option<Ptr>,

    pub world_last_shot_eject_slot: Option<Ptr>,

    pub explosion_slot: Option<Ptr>,

    pub tracer_slot: Option<Ptr>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TracerDefGeometry {
    pub header: Option<Ptr>,
    pub name: Option<Ptr>,

    pub material_slot: Option<Ptr>,

    pub material_name: Option<Ptr>,

    pub material_fresh: bool,
    pub draw_interval: u32,
    pub speed: f32,
    pub beam_length: f32,
    pub beam_width: f32,
    pub screw_radius: f32,
    pub screw_dist: f32,
    pub colors: [[f32; 4]; 5],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZoneHeader {
    pub size: u32,
    pub external_size: u32,
    pub block_size: [u32; MAX_XFILE_COUNT],
}

pub fn parse_zone_header(image: &[u8]) -> Result<ZoneHeader> {
    if image.len() < XFILE_HEADER_LEN {
        return Err(ZoneError::Truncated {
            at: 0,
            needed: XFILE_HEADER_LEN,
            len: image.len(),
        });
    }
    let rd = |o: usize| u32::from_le_bytes([image[o], image[o + 1], image[o + 2], image[o + 3]]);
    let mut block_size = [0u32; MAX_XFILE_COUNT];
    for (i, b) in block_size.iter_mut().enumerate() {
        *b = rd(8 + i * 4);
    }
    Ok(ZoneHeader {
        size: rd(0),
        external_size: rd(4),
        block_size,
    })
}

fn align_up(v: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two(), "alignment must be a power of two");
    crate::stream::alloc_stream_pos(v as u32, align as u32 - 1) as usize
}

pub const EXPR_STMT_CACHE_N: usize = 128;
pub const EXPR_STMT_CACHE_BLOB: usize = 1024;

pub struct ZoneStream<'a> {
    format: Iw4WireFormat,
    data: &'a [u8],
    cursor: usize,
    blocks: [&'a mut [u8]; MAX_XFILE_COUNT],
    offsets: [usize; MAX_XFILE_COUNT],
    stack: [u8; BLOCK_STACK_CAP],
    stack_depth: usize,
    temp_saved: [usize; BLOCK_STACK_CAP],
    temp_depth: usize,

    insert_map: &'a mut [u8],
    pending_insert: Option<Ptr>,

    unsettled_offsets: usize,
    first_unsettled: Option<(Ptr, usize, usize)>,
    com_world: Option<ComWorldGeometry>,
    light_defs: [GfxLightDefGeometry; MAX_LIGHT_DEFS],
    light_def_count: usize,
    gfx_world: Option<GfxWorldGeometry>,
    fx_world: Option<FxWorldGeometry>,
    glass_data: Option<GlassDataGeometry>,
    clip_map: Option<ClipMapGeometry>,
    map_ents: Option<MapEntsGeometry>,
    xmodel: Option<XModelGeometry>,
    phys_preset: Option<PhysPresetGeometry>,
    weapon: Option<WeaponGeometry>,
    vehicle: Option<(Option<Ptr>, Option<Ptr>)>,
    vehicle_compass: ([[u8; 128]; 2], [i32; 2]),
    vehicle_accel: f32,
    latest_material: Option<MaterialGeometry>,
    latest_image: Option<GfxImageGeometry>,
    latest_technique_set: Option<TechniqueSetGeometry>,

    technique_graph: TechniqueGraphGeometry,
    technique_graph_seen: bool,
    latest_shader: Option<ShaderGeometry>,
    structured_data_def_set: Option<Ptr>,
    latest_vertex_decl: Option<VertexDeclGeometry>,
    latest_sound_name: Option<Ptr>,
    image_serial: u64,
    header: ZoneHeader,

    expr_stmt_keys: [u32; EXPR_STMT_CACHE_N],
    expr_stmt_blobs: [[u8; EXPR_STMT_CACHE_BLOB]; EXPR_STMT_CACHE_N],
    expr_stmt_lens: [u16; EXPR_STMT_CACHE_N],
    expr_stmt_n: usize,

    alloc_tag: u32,
    alloc_align: u32,
    alloc_bytes: u32,
}

impl<'a> ZoneStream<'a> {
    pub fn insert_map_len(header: &ZoneHeader) -> usize {
        let slots = header.block_size[XFILE_BLOCK_VIRTUAL] as usize / PTR_SIZE;
        slots.div_ceil(8) + 1
    }

    pub fn new(
        image: &'a [u8],
        blocks: [&'a mut [u8]; MAX_XFILE_COUNT],
        insert_map: &'a mut [u8],
    ) -> Result<ZoneStream<'a>> {
        let format = match (
            crate::read_wire_asset_table(image, Iw4WireFormat::X86),
            crate::read_wire_asset_table(image, Iw4WireFormat::X64),
        ) {
            (Ok(table), Err(_)) | (Err(_), Ok(table)) => table.format(),
            (Ok(_), Ok(_)) => return Err(ZoneError::AmbiguousWireFormat),
            (Err(x86), Err(x64)) => return Err(ZoneError::InvalidWireTable { x86, x64 }),
        };
        Self::new_with_format(image, blocks, insert_map, format)
    }

    pub fn new_with_format(
        image: &'a [u8],
        blocks: [&'a mut [u8]; MAX_XFILE_COUNT],
        insert_map: &'a mut [u8],
        format: Iw4WireFormat,
    ) -> Result<ZoneStream<'a>> {
        let header = parse_zone_header(image)?;
        for (i, b) in blocks.iter().enumerate() {
            let want = header.block_size[i] as usize;
            if b.len() < want {
                return Err(ZoneError::BlockOverflow {
                    block: i,
                    end: want,
                    size: b.len(),
                    request: want,
                });
            }
        }
        let needed = Self::insert_map_len(&header);
        if insert_map.len() < needed {
            return Err(ZoneError::InsertMapTooSmall {
                needed,
                got: insert_map.len(),
            });
        }
        insert_map.fill(0);
        Ok(ZoneStream {
            format,
            data: image,
            cursor: XFILE_HEADER_LEN,
            blocks,
            offsets: [0; MAX_XFILE_COUNT],
            stack: [0; BLOCK_STACK_CAP],
            stack_depth: 0,
            temp_saved: [0; BLOCK_STACK_CAP],
            temp_depth: 0,
            insert_map,
            pending_insert: None,
            unsettled_offsets: 0,
            first_unsettled: None,
            com_world: None,
            light_defs: [GfxLightDefGeometry {
                name: None,
                lmap_lookup_start: 0,
                attenuation_width: None,
                attenuation_image: None,
                attenuation_image_name: None,
                attenuation_sampler: 0,
            }; MAX_LIGHT_DEFS],
            light_def_count: 0,
            gfx_world: None,
            fx_world: None,
            glass_data: None,
            clip_map: None,
            map_ents: None,
            xmodel: None,
            phys_preset: None,
            weapon: None,
            vehicle: None,
            vehicle_compass: ([[0; 128]; 2], [0; 2]),
            vehicle_accel: 0.0,
            latest_material: None,
            latest_image: None,
            latest_technique_set: None,
            technique_graph: TechniqueGraphGeometry::default(),
            technique_graph_seen: false,
            latest_shader: None,
            structured_data_def_set: None,
            latest_vertex_decl: None,
            latest_sound_name: None,
            image_serial: 0,
            header,
            expr_stmt_keys: [0; EXPR_STMT_CACHE_N],
            expr_stmt_blobs: [[0; EXPR_STMT_CACHE_BLOB]; EXPR_STMT_CACHE_N],
            expr_stmt_lens: [0; EXPR_STMT_CACHE_N],
            expr_stmt_n: 0,
            alloc_tag: 0,
            alloc_align: 0,
            alloc_bytes: 0,
        })
    }

    pub fn wire_format(&self) -> Iw4WireFormat {
        self.format
    }

    pub fn pointer_bytes(&self) -> usize {
        self.format.pointer_bytes()
    }

    pub fn layout(&self, x86: usize, x64: usize) -> usize {
        match self.format {
            Iw4WireFormat::X86 => x86,
            Iw4WireFormat::X64 => x64,
        }
    }

    pub fn decode_pointer(&self, raw: u64) -> Result<ZonePtr> {
        match WirePointer::decode(raw, self.format) {
            Some(WirePointer::Null) => Ok(ZonePtr::Null),
            Some(WirePointer::Following) => Ok(ZonePtr::Following),
            Some(WirePointer::Insert) => Ok(ZonePtr::Insert),
            Some(WirePointer::Offset(p)) => Ok(ZonePtr::Offset(p)),
            None => Err(ZoneError::InvalidWirePointer {
                raw,
                format: self.format,
            }),
        }
    }

    pub fn header(&self) -> &ZoneHeader {
        &self.header
    }

    fn expr_stmt_key(p: Ptr) -> u32 {
        ZonePtr::encode_offset(p)
    }

    pub fn expr_stmt_insert(&mut self, p: Ptr, text: &str) {
        if text.is_empty() {
            return;
        }
        let k = Self::expr_stmt_key(p);
        for i in 0..self.expr_stmt_n {
            if self.expr_stmt_keys[i] == k {
                if self.expr_stmt_lens[i] == 0 {
                    let bytes = text.as_bytes();
                    let n = bytes.len().min(EXPR_STMT_CACHE_BLOB);
                    self.expr_stmt_blobs[i][..n].copy_from_slice(&bytes[..n]);
                    self.expr_stmt_lens[i] = n as u16;
                }
                return;
            }
        }
        if self.expr_stmt_n >= EXPR_STMT_CACHE_N {
            return;
        }
        let bytes = text.as_bytes();
        let n = bytes.len().min(EXPR_STMT_CACHE_BLOB);
        let i = self.expr_stmt_n;
        self.expr_stmt_keys[i] = k;
        self.expr_stmt_blobs[i][..n].copy_from_slice(&bytes[..n]);
        self.expr_stmt_lens[i] = n as u16;
        self.expr_stmt_n += 1;
    }

    pub fn expr_stmt_get(&self, p: Ptr) -> Option<&str> {
        let k = Self::expr_stmt_key(p);
        for i in 0..self.expr_stmt_n {
            if self.expr_stmt_keys[i] == k {
                let text = core::str::from_utf8(
                    &self.expr_stmt_blobs[i][..self.expr_stmt_lens[i] as usize],
                )
                .ok()?;
                if text.is_empty() {
                    return None;
                }
                return Some(text);
            }
        }
        None
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.cursor)
    }

    pub fn source_slice(&self, offset: usize, len: usize) -> Result<&[u8]> {
        self.data
            .get(offset..offset.saturating_add(len))
            .ok_or(ZoneError::Truncated {
                at: offset,
                needed: len,
                len: self.data.len(),
            })
    }

    pub fn peek(&self, n: usize) -> &[u8] {
        let end = (self.cursor + n).min(self.data.len());
        &self.data[self.cursor..end]
    }

    pub fn unsettled_offsets(&self) -> usize {
        self.unsettled_offsets
    }

    pub fn finish(&self) -> Result<()> {
        if self.remaining() != 0 {
            return Err(ZoneError::Truncated {
                at: self.cursor,
                needed: self.data.len(),
                len: self.data.len(),
            });
        }
        Ok(())
    }

    pub fn push(&mut self, block: usize) -> Result<()> {
        if block >= MAX_XFILE_COUNT {
            return Err(ZoneError::BadBlock(block));
        }
        if self.stack_depth >= BLOCK_STACK_CAP {
            return Err(ZoneError::StackOverflow);
        }
        if BLOCK_TYPES[block] == BlockType::Temp {
            if self.temp_depth >= BLOCK_STACK_CAP {
                return Err(ZoneError::StackOverflow);
            }
            self.temp_saved[self.temp_depth] = self.offsets[block];
            self.temp_depth += 1;
        }
        self.stack[self.stack_depth] = block as u8;
        self.stack_depth += 1;
        Ok(())
    }

    pub fn pop(&mut self) -> Result<()> {
        if self.stack_depth == 0 {
            return Err(ZoneError::StackUnderflow);
        }
        self.stack_depth -= 1;
        let b = self.stack[self.stack_depth] as usize;
        if BLOCK_TYPES[b] == BlockType::Temp {
            if self.temp_depth == 0 {
                return Err(ZoneError::StackUnderflow);
            }
            self.temp_depth -= 1;

            self.offsets[b] = self.temp_saved[self.temp_depth];
        }
        Ok(())
    }

    fn top(&self) -> Result<usize> {
        if self.stack_depth == 0 {
            return Err(ZoneError::NoBlockPushed);
        }
        Ok(self.stack[self.stack_depth - 1] as usize)
    }

    pub fn read_raw(&mut self, size: usize) -> Result<&[u8]> {
        let s = self.cursor;
        if s + size > self.data.len() {
            return Err(ZoneError::Truncated {
                at: s,
                needed: size,
                len: self.data.len(),
            });
        }
        self.cursor += size;
        Ok(&self.data[s..s + size])
    }

    pub fn alloc_load(&mut self, align: usize, size: usize) -> Result<Ptr> {
        let b = self.top()?;
        let pos = align_up(self.offsets[b], align);
        let end = pos + size;
        if end > self.blocks[b].len() {
            return Err(ZoneError::BlockOverflow {
                block: b,
                end,
                size: self.blocks[b].len(),
                request: size,
            });
        }

        if BLOCK_TYPES[b] == BlockType::Runtime {
            self.blocks[b][pos..end].fill(0);
        } else {
            let s = self.cursor;
            if s + size > self.data.len() {
                return Err(ZoneError::Truncated {
                    at: s,
                    needed: size,
                    len: self.data.len(),
                });
            }
            self.blocks[b][pos..end].copy_from_slice(&self.data[s..s + size]);
            self.cursor += size;
        }

        self.offsets[b] = end;
        let body = Ptr {
            block: b as u8,
            offset: pos as u32,
        };
        self.bind_pending_insert(body)?;
        Ok(body)
    }

    pub fn next_ptr(&self, align: usize) -> Result<Ptr> {
        let block = self.top()?;
        Ok(Ptr {
            block: block as u8,
            offset: align_up(self.offsets[block], align) as u32,
        })
    }

    pub fn load_string(&mut self) -> Result<Ptr> {
        let b = self.top()?;
        let start = self.offsets[b];
        let mut off = start;
        loop {
            if off >= self.blocks[b].len() {
                return Err(ZoneError::BlockOverflow {
                    block: b,
                    end: off,
                    size: self.blocks[b].len(),
                    request: 0,
                });
            }
            if self.cursor >= self.data.len() {
                return Err(ZoneError::UnterminatedString { block: b });
            }
            let byte = self.data[self.cursor];
            self.cursor += 1;
            self.blocks[b][off] = byte;
            off += 1;
            if byte == 0 {
                break;
            }
        }
        self.offsets[b] = off;
        let body = Ptr {
            block: b as u8,
            offset: start as u32,
        };
        self.bind_pending_insert(body)?;
        Ok(body)
    }

    pub fn insert_pointer_slot(&mut self) -> Result<Ptr> {
        let b = XFILE_BLOCK_VIRTUAL;
        let width = self.pointer_bytes();
        let pos = align_up(self.offsets[b], width);
        let end = pos + width;
        if end > self.blocks[b].len() {
            return Err(ZoneError::BlockOverflow {
                block: b,
                end,
                size: self.blocks[b].len(),
                request: width,
            });
        }
        self.offsets[b] = end;
        Ok(Ptr {
            block: b as u8,
            offset: pos as u32,
        })
    }

    fn bind_pending_insert(&mut self, body: Ptr) -> Result<()> {
        let Some(slot) = self.pending_insert.take() else {
            return Ok(());
        };
        self.fixup_slot(slot, body)?;
        self.mark_insert_slot(slot);
        Ok(())
    }

    fn insert_bit(slot: Ptr) -> (usize, u8) {
        let idx = slot.offset as usize / PTR_SIZE;
        (idx / 8, 1u8 << (idx % 8))
    }

    fn mark_insert_slot(&mut self, slot: Ptr) {
        if slot.block as usize != XFILE_BLOCK_VIRTUAL {
            return;
        }
        let (byte, bit) = Self::insert_bit(slot);
        if let Some(cell) = self.insert_map.get_mut(byte) {
            *cell |= bit;
        }
    }

    fn is_insert_slot(&self, p: Ptr) -> bool {
        if p.block as usize != XFILE_BLOCK_VIRTUAL {
            return false;
        }
        let (byte, bit) = Self::insert_bit(p);
        self.insert_map.get(byte).is_some_and(|c| c & bit != 0)
    }

    pub fn resolve_alias(&self, p: Ptr) -> Ptr {
        if !self.is_insert_slot(p) {
            return p;
        }
        match self.ptr_at(p, 0) {
            Ok(ZonePtr::Offset(body)) => body,
            _ => p,
        }
    }

    pub fn begin_body_with_insert(&mut self, slot: Ptr) -> Result<(bool, Option<Ptr>)> {
        match self.ptr_at(slot, 0)? {
            ZonePtr::Null => Ok((false, None)),
            ZonePtr::Offset(p) => {
                self.note_offset(p);
                Ok((false, None))
            }
            ZonePtr::Following => Ok((true, None)),
            ZonePtr::Insert => {
                let reserved = self.insert_pointer_slot()?;
                self.pending_insert = Some(reserved);
                Ok((true, Some(reserved)))
            }
        }
    }

    pub fn begin_body(&mut self, slot: Ptr) -> Result<bool> {
        Ok(self.begin_body_with_insert(slot)?.0)
    }

    fn bytes_at(&self, p: Ptr, off: usize, len: usize) -> Result<&[u8]> {
        let b = p.block as usize;
        if b >= MAX_XFILE_COUNT {
            return Err(ZoneError::BadBlock(b));
        }
        let s = p.offset as usize + off;
        let buf = &self.blocks[b];
        if s + len > buf.len() {
            return Err(ZoneError::BadOffset {
                block: b,
                offset: s,
                size: buf.len(),
            });
        }
        Ok(&buf[s..s + len])
    }

    pub fn u8_at(&self, p: Ptr, off: usize) -> Result<u8> {
        Ok(self.bytes_at(p, off, 1)?[0])
    }

    pub fn u16_at(&self, p: Ptr, off: usize) -> Result<u16> {
        let b = self.bytes_at(p, off, 2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn u32_at(&self, p: Ptr, off: usize) -> Result<u32> {
        let b = self.bytes_at(p, off, 4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn i16_at(&self, p: Ptr, off: usize) -> Result<i16> {
        Ok(self.u16_at(p, off)? as i16)
    }

    pub fn i32_at(&self, p: Ptr, off: usize) -> Result<i32> {
        Ok(self.u32_at(p, off)? as i32)
    }

    pub fn f32_at(&self, p: Ptr, off: usize) -> Result<f32> {
        Ok(f32::from_bits(self.u32_at(p, off)?))
    }

    pub fn slice_at(&self, p: Ptr, off: usize, len: usize) -> Result<&[u8]> {
        self.bytes_at(p, off, len)
    }

    pub fn ptr_at(&self, p: Ptr, off: usize) -> Result<ZonePtr> {
        let raw = match self.format {
            Iw4WireFormat::X86 => u64::from(self.u32_at(p, off)?),
            Iw4WireFormat::X64 => u64::from_le_bytes(self.bytes_at(p, off, 8)?.try_into().unwrap()),
        };
        self.decode_pointer(raw)
    }

    pub fn write_u32_at(&mut self, p: Ptr, off: usize, v: u32) -> Result<()> {
        let b = p.block as usize;
        if b >= MAX_XFILE_COUNT {
            return Err(ZoneError::BadBlock(b));
        }
        let s = p.offset as usize + off;
        let len = self.blocks[b].len();
        if s + 4 > len {
            return Err(ZoneError::BadOffset {
                block: b,
                offset: s,
                size: len,
            });
        }
        self.blocks[b][s..s + 4].copy_from_slice(&v.to_le_bytes());
        Ok(())
    }

    pub fn write_pointer_at(&mut self, p: Ptr, off: usize, raw: u64) -> Result<()> {
        self.decode_pointer(raw)?;
        let width = self.pointer_bytes();
        self.bytes_at(p, off, width)?;
        let start = p.offset as usize + off;
        self.blocks[p.block as usize][start..start + width]
            .copy_from_slice(&raw.to_le_bytes()[..width]);
        Ok(())
    }

    pub fn cstr(&self, p: Ptr) -> Result<&str> {
        let raw = self.cstr_bytes(p)?;
        core::str::from_utf8(raw).map_err(|_| ZoneError::NotUtf8)
    }

    pub fn cstr_bytes(&self, p: Ptr) -> Result<&[u8]> {
        let b = p.block as usize;
        if b >= MAX_XFILE_COUNT {
            return Err(ZoneError::BadBlock(b));
        }
        let buf = &self.blocks[b];
        let s = p.offset as usize;
        if s >= buf.len() {
            return Err(ZoneError::BadOffset {
                block: b,
                offset: s,
                size: buf.len(),
            });
        }
        let end = buf[s..]
            .iter()
            .position(|&c| c == 0)
            .map_or(buf.len(), |i| s + i);
        Ok(&buf[s..end])
    }

    pub fn watermark(&self, block: u8) -> usize {
        self.offsets
            .get(block as usize)
            .copied()
            .unwrap_or_default()
    }

    pub fn tag_alloc(&mut self, tag: u32, align: usize, bytes: usize) {
        self.alloc_tag = tag;
        self.alloc_align = align as u32;
        self.alloc_bytes = bytes as u32;
    }

    pub fn last_alloc_tag(&self) -> (u32, u32, u32) {
        (self.alloc_tag, self.alloc_align, self.alloc_bytes)
    }

    pub fn offset_is_settled(&self, p: Ptr) -> bool {
        (p.offset as usize) < self.watermark(p.block)
    }

    pub fn note_offset(&mut self, p: Ptr) {
        if !self.offset_is_settled(p) {
            self.unsettled_offsets += 1;
            if self.first_unsettled.is_none() {
                self.first_unsettled = Some((p, self.watermark(p.block), self.cursor));
            }
        }
    }

    pub fn record_com_world(&mut self, geometry: ComWorldGeometry) {
        self.com_world = Some(geometry);
    }

    pub fn com_world(&self) -> Option<ComWorldGeometry> {
        self.com_world
    }

    pub fn record_light_def(&mut self, geometry: GfxLightDefGeometry) {
        if self.light_def_count >= MAX_LIGHT_DEFS {
            return;
        }
        self.light_defs[self.light_def_count] = geometry;
        self.light_def_count += 1;
    }

    pub fn light_defs(&self) -> &[GfxLightDefGeometry] {
        &self.light_defs[..self.light_def_count]
    }

    pub fn record_gfx_world(&mut self, geometry: GfxWorldGeometry) {
        self.gfx_world = Some(geometry);
    }

    pub fn gfx_world(&self) -> Option<GfxWorldGeometry> {
        self.gfx_world
    }

    pub fn record_fx_world(&mut self, geometry: FxWorldGeometry) {
        self.fx_world = Some(geometry);
    }

    pub fn fx_world(&self) -> Option<FxWorldGeometry> {
        self.fx_world
    }

    pub fn record_g_glass_data(&mut self, geometry: GlassDataGeometry) {
        self.glass_data = Some(geometry);
    }

    pub fn glass_data(&self) -> Option<GlassDataGeometry> {
        self.glass_data
    }

    pub fn record_clip_map(&mut self, geometry: ClipMapGeometry) {
        self.clip_map = Some(geometry);
    }

    pub fn clip_map(&self) -> Option<ClipMapGeometry> {
        self.clip_map
    }

    pub fn record_map_ents(&mut self, geometry: MapEntsGeometry) {
        self.map_ents = Some(geometry);
    }

    pub fn map_ents(&self) -> Option<MapEntsGeometry> {
        self.map_ents
    }

    pub fn record_xmodel(&mut self, geometry: XModelGeometry) {
        self.xmodel = Some(geometry);
    }

    pub fn xmodel(&self) -> Option<XModelGeometry> {
        self.xmodel
    }

    pub fn record_phys_preset(&mut self, geometry: PhysPresetGeometry) {
        self.phys_preset = Some(geometry);
    }

    pub fn phys_preset(&self) -> Option<PhysPresetGeometry> {
        self.phys_preset
    }

    pub fn record_weapon(&mut self, geometry: WeaponGeometry) {
        self.weapon = Some(geometry);
    }

    pub fn weapon(&self) -> Option<WeaponGeometry> {
        self.weapon
    }

    pub(crate) fn record_vehicle(&mut self, name: Option<Ptr>, turret_weapon: Option<Ptr>) {
        self.vehicle = Some((name, turret_weapon));
    }

    pub(crate) fn record_vehicle_compass(&mut self, icons: [[u8; 128]; 2], size: [i32; 2]) {
        self.vehicle_compass = (icons, size);
    }

    pub(crate) fn record_vehicle_accel(&mut self, accel: f32) {
        self.vehicle_accel = accel;
    }

    pub fn vehicle_accel(&self) -> f32 {
        self.vehicle_accel
    }

    pub fn vehicle_compass(&self) -> ([&str; 2], [i32; 2]) {
        let icons = [&self.vehicle_compass.0[0], &self.vehicle_compass.0[1]].map(|bytes| {
            let len = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
            core::str::from_utf8(&bytes[..len]).unwrap_or("")
        });
        (icons, self.vehicle_compass.1)
    }

    pub fn vehicle(&self) -> Option<(Option<Ptr>, Option<Ptr>)> {
        self.vehicle
    }

    pub(crate) fn record_material(&mut self, material: MaterialGeometry) {
        self.latest_material = Some(material);
    }

    pub fn latest_material(&self) -> Option<MaterialGeometry> {
        self.latest_material
    }

    pub(crate) fn record_technique_set(&mut self, technique_set: TechniqueSetGeometry) {
        self.latest_technique_set = Some(technique_set);
    }

    pub fn latest_technique_set(&self) -> Option<TechniqueSetGeometry> {
        self.latest_technique_set
    }

    pub(crate) fn begin_technique_graph(&mut self) {
        self.technique_graph_seen = true;
        self.technique_graph.row_count = 0;
        self.technique_graph.argument_count = 0;
        self.technique_graph.rows_truncated = 0;
        self.technique_graph.arguments_truncated = 0;
    }

    pub(crate) fn technique_argument_count(&self) -> u16 {
        self.technique_graph.argument_count
    }

    pub(crate) fn push_technique_row(&mut self, row: TechniquePassGeometry) {
        let graph = &mut self.technique_graph;
        match graph.rows.get_mut(graph.row_count as usize) {
            Some(slot) => {
                *slot = row;
                graph.row_count = graph.row_count.saturating_add(1);
            }
            None => graph.rows_truncated = graph.rows_truncated.saturating_add(1),
        }
    }

    pub(crate) fn push_technique_argument(&mut self, argument: TechniqueArgumentGeometry) -> bool {
        let graph = &mut self.technique_graph;
        match graph.arguments.get_mut(graph.argument_count as usize) {
            Some(slot) => {
                *slot = argument;
                graph.argument_count = graph.argument_count.saturating_add(1);
                true
            }
            None => {
                graph.arguments_truncated = graph.arguments_truncated.saturating_add(1);
                false
            }
        }
    }

    pub(crate) fn note_technique_arguments_truncated(&mut self, count: u16) {
        let graph = &mut self.technique_graph;
        graph.arguments_truncated = graph.arguments_truncated.saturating_add(count);
    }

    pub fn latest_technique_graph(&self) -> Option<&TechniqueGraphGeometry> {
        self.technique_graph_seen.then_some(&self.technique_graph)
    }

    pub(crate) fn record_structured_data_def_set(&mut self, header: Ptr) {
        self.structured_data_def_set = Some(header);
    }

    pub fn structured_data_def_set(&self) -> Option<Ptr> {
        self.structured_data_def_set
    }

    pub(crate) fn record_shader(&mut self, shader: ShaderGeometry) {
        self.latest_shader = Some(shader);
    }

    pub fn latest_shader(&self) -> Option<ShaderGeometry> {
        self.latest_shader
    }

    pub(crate) fn record_vertex_decl(&mut self, decl: VertexDeclGeometry) {
        self.latest_vertex_decl = Some(decl);
    }

    pub fn latest_vertex_decl(&self) -> Option<VertexDeclGeometry> {
        self.latest_vertex_decl
    }

    pub(crate) fn record_sound_list_name(&mut self, name: Option<Ptr>) {
        self.latest_sound_name = name;
    }

    pub fn latest_sound_name(&self) -> Option<Ptr> {
        self.latest_sound_name
    }

    pub(crate) fn record_image(&mut self, image: GfxImageGeometry) {
        self.latest_image = Some(image);
        self.image_serial = self.image_serial.wrapping_add(1);
    }

    pub fn latest_image(&self) -> Option<GfxImageGeometry> {
        self.latest_image
    }

    pub(crate) fn image_serial(&self) -> u64 {
        self.image_serial
    }

    pub fn first_unsettled(&self) -> Option<(Ptr, usize, usize)> {
        self.first_unsettled
    }

    pub fn fixup_slot(&mut self, slot: Ptr, body: Ptr) -> Result<()> {
        if body.block as usize >= MAX_XFILE_COUNT || body.offset > OFFSET_MASK {
            return Err(ZoneError::BadOffset {
                block: body.block as usize,
                offset: body.offset as usize,
                size: OFFSET_MASK as usize + 1,
            });
        }
        self.write_pointer_at(slot, 0, u64::from(ZonePtr::encode_offset(body)))
    }

    pub fn follow_array(
        &mut self,
        parent: Ptr,
        field: usize,
        align: usize,
        elem_size: usize,
        count: usize,
    ) -> Result<Option<Ptr>> {
        let body = match self.ptr_at(parent, field)? {
            ZonePtr::Null => return Ok(None),
            ZonePtr::Offset(p) => {
                self.note_offset(p);
                self.resolve_alias(p)
            }
            _ => {
                self.begin_body(parent.at(field))?;
                self.alloc_load(align, elem_size * count)?
            }
        };
        self.fixup_slot(parent.at(field), body)?;
        Ok(Some(body))
    }

    pub fn plain_array(
        &mut self,
        p: Ptr,
        field: usize,
        align: usize,
        elem: usize,
        count: usize,
    ) -> Result<Option<Ptr>> {
        if count == 0 {
            let body = match self.ptr_at(p, field)? {
                ZonePtr::Null => return Ok(None),
                ZonePtr::Offset(q) => {
                    self.note_offset(q);
                    q
                }
                _ => self.alloc_load(align, 0)?,
            };
            self.fixup_slot(p.at(field), body)?;
            return Ok(Some(body));
        }
        self.follow_array(p, field, align, elem, count)
    }

    pub fn follow_string(&mut self, parent: Ptr, field: usize) -> Result<Option<Ptr>> {
        let body = match self.ptr_at(parent, field)? {
            ZonePtr::Null => return Ok(None),
            ZonePtr::Offset(p) => {
                self.note_offset(p);
                self.resolve_alias(p)
            }
            _ => {
                self.begin_body(parent.at(field))?;
                self.load_string()?
            }
        };
        self.fixup_slot(parent.at(field), body)?;
        Ok(Some(body))
    }
}
