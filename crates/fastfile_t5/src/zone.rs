pub const MAX_XFILE_COUNT: usize = 7;

pub const XFILE_BLOCK_TEMP: usize = 0;
pub const XFILE_BLOCK_RUNTIME: usize = 1;
pub const XFILE_BLOCK_LARGE_RUNTIME: usize = 2;
pub const XFILE_BLOCK_PHYSICAL_RUNTIME: usize = 3;
pub const XFILE_BLOCK_VIRTUAL: usize = 4;
pub const XFILE_BLOCK_LARGE: usize = 5;
pub const XFILE_BLOCK_PHYSICAL: usize = 6;

pub const PTR_SIZE: usize = 4;

const BLOCK_SHIFT: u32 = 29;
const OFFSET_MASK: u32 = 0x1FFF_FFFF;

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
    BlockType::Runtime,
    BlockType::Runtime,
    BlockType::Runtime,
    BlockType::Normal,
    BlockType::Normal,
    BlockType::Normal,
];

pub fn block_is_aliasable(block: u8) -> bool {
    matches!(BLOCK_TYPES.get(block as usize), Some(BlockType::Normal))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ZoneError {
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
    LightDefTableFull,
    NoBlockPushed,
    InsertMapTooSmall {
        needed: usize,
        got: usize,
    },
}

impl core::fmt::Display for ZoneError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
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
            ZoneError::LightDefTableFull => write!(f, "light-def capture capacity exceeded (128)"),
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
    debug_assert!(align.is_power_of_two());
    let mask = (align as u32).wrapping_sub(1);
    (v as u32).wrapping_add(mask) as usize & !(mask as usize)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum AlignWasteSite {
    #[default]
    Unscoped = 0,
    Verts0 = 1,
    TriIndices = 2,
    CollNodes = 3,
    PhysGeoms = 4,
    BrushWrapper = 5,
    MatConstants = 6,
    OtherA16 = 7,
    BoneArrays = 8,
    VertInfo = 9,
    CollLeafs = 10,
    VertList = 11,
    XModelMisc = 12,
}

pub const ALIGN_WASTE_SITE_COUNT: usize = 13;

#[derive(Clone, Copy, Debug, Default)]
pub struct AlignWasteStats {
    pub allocs: u32,
    pub pad_bytes: u64,
    pub allocs_align_2: u32,
    pub pad_align_2: u64,
    pub allocs_align_4: u32,
    pub pad_align_4: u64,
    pub allocs_align_16: u32,
    pub pad_align_16: u64,
    pub allocs_align_other: u32,
    pub pad_align_other: u64,
    pub insert_slots: u32,
    pub insert_pad_bytes: u64,

    pub zero_size_a16_allocs: u32,
    pub zero_size_a16_pad: u64,
    pub site_allocs: [u32; ALIGN_WASTE_SITE_COUNT],
    pub site_pad: [u64; ALIGN_WASTE_SITE_COUNT],
    pub site_a16_allocs: [u32; ALIGN_WASTE_SITE_COUNT],
    pub site_a16_pad: [u64; ALIGN_WASTE_SITE_COUNT],

    pub verts0_phase: [u32; 16],

    pub xmodel_entry_phase: [u32; 16],
}

impl AlignWasteSite {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unscoped => "unscoped",
            Self::Verts0 => "verts0",
            Self::TriIndices => "triIndices",
            Self::CollNodes => "collNodes",
            Self::PhysGeoms => "physGeoms",
            Self::BrushWrapper => "brushWrapper",
            Self::MatConstants => "matConstants",
            Self::OtherA16 => "otherA16",
            Self::BoneArrays => "boneArrays",
            Self::VertInfo => "vertInfo",
            Self::CollLeafs => "collLeafs",
            Self::VertList => "vertList",
            Self::XModelMisc => "xmodelMisc",
        }
    }

    pub const fn index(self) -> usize {
        self as u8 as usize
    }
}

pub struct ZoneStream<'a> {
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
    align_waste: AlignWasteStats,
    align_waste_site: AlignWasteSite,

    diag_skip_coll_nodes_virtual: bool,
    header: ZoneHeader,
    xmodel: Option<XModelGeometry>,
    weapon: Option<WeaponGeometry>,
    gfx_world: Option<GfxWorldGeometry>,
    clip_map: Option<ClipMapGeometry>,
    map_ents: Option<MapEntsGeometry>,
    com_world: Option<ComWorldGeometry>,
    light_defs: [GfxLightDefGeometry; 128],
    light_def_count: usize,
    latest_image: Option<GfxImageGeometry>,
    latest_material: Option<MaterialGeometry>,
    latest_technique_set: Option<TechniqueSetGeometry>,

    technique_graph: TechniqueGraphGeometry,
    technique_graph_seen: bool,
    latest_shader: Option<ShaderGeometry>,
    latest_vertex_decl: Option<VertexDeclGeometry>,
    image_serial: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct XModelGeometry {
    pub name: Option<Ptr>,
    pub material_handles: Option<Ptr>,
    pub surfaces: Option<Ptr>,
    pub surface_count: usize,

    pub lod_surf_span: [(u16, u16); 4],

    pub lod_dist: [f32; 4],

    pub num_lods: i16,

    pub num_bones: usize,

    pub num_root_bones: usize,

    pub bone_names: Option<Ptr>,

    pub parent_list: Option<Ptr>,

    pub quats: Option<Ptr>,

    pub trans: Option<Ptr>,

    pub base_mat: Option<Ptr>,

    pub part_classification: Option<Ptr>,

    pub bone_info: Option<Ptr>,

    pub coll_surfs: Option<Ptr>,

    pub num_coll_surfs: usize,

    pub coll_lod: i16,

    pub contents: u32,

    pub radius: Option<f32>,

    pub collision_maps: Option<Ptr>,

    pub collision_map_count: usize,
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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponGeometry {
    pub overlay_material_name: Option<Ptr>,
    pub overlay_material_lowres_name: Option<Ptr>,
    pub reticle_center_name: Option<Ptr>,
    pub reticle_side_name: Option<Ptr>,
    pub name: Option<Ptr>,
    pub alternate_weapon_name: Option<Ptr>,
    pub alternate_raise_time_ms: i32,
    pub alternate_drop_time_ms: i32,

    pub display_name: Option<Ptr>,

    pub weap_def: Option<Ptr>,

    pub gun_xmodel_name: Option<Ptr>,

    pub hand_xmodel_name: Option<Ptr>,

    pub world_model_name: Option<Ptr>,

    pub rocket_model_name: Option<Ptr>,

    pub projectile_model_name: Option<Ptr>,

    pub move_speed_scale: f32,

    pub ads_move_speed_scale: f32,

    pub weap_type: i32,

    pub weap_class: i32,

    pub fire_type: i32,

    pub fire_time_ms: i32,

    pub rechamber_time_ms: i32,

    pub drop_time_ms: i32,

    pub raise_time_ms: i32,

    pub bolt_action: bool,

    pub clip_size: i32,

    pub sz_xanims: Option<Ptr>,

    pub hide_tags: Option<Ptr>,

    pub variant: Option<Ptr>,
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
}

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

pub const MAX_LIGHTMAP_PAGES: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxLightmapImages {
    pub primary: Option<GfxImageGeometry>,
    pub secondary: Option<GfxImageGeometry>,
    pub secondary_b: Option<GfxImageGeometry>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxWorldGeometry {
    pub terrain_scorch_images: Option<Ptr>,
    pub vertices: Option<Ptr>,
    pub vertex_count: usize,

    pub vertex_layer: Option<Ptr>,
    pub vertex_layer_size: usize,
    pub indices: Option<Ptr>,
    pub index_count: usize,
    pub surfaces: Option<Ptr>,
    pub surface_count: usize,
    pub lightmap_count: usize,

    pub lightmaps: [GfxLightmapImages; MAX_LIGHTMAP_PAGES],

    pub first_sky_image: Option<Ptr>,

    pub sky_surf_count: usize,

    pub sky_start_surfs: Option<Ptr>,

    pub sky_box_model: Option<Ptr>,

    pub sun_light: Option<Ptr>,

    pub reflection_probes: Option<Ptr>,
    pub reflection_probe_count: usize,

    pub sun_primary_light_index: usize,

    pub sun_parse_exposure_bits: Option<u32>,

    pub sky_dynamic_intensity_bits: Option<[u32; 4]>,

    pub sun_parse_tree_scatter_intensity_bits: Option<u32>,

    pub sun_parse_tree_scatter_amount_bits: Option<u32>,

    pub exposure_volume_count: usize,

    pub light_grid: GfxLightGridGeometry,
    pub cell_count: usize,
    pub plane_count: usize,
    pub node_count: usize,
    pub smodel_count: usize,
    pub static_surface_count: usize,
    pub lit_surfs_begin: u32,
    pub lit_surfs_end: u32,

    pub decal_surfs_begin: u32,
    pub decal_surfs_end: u32,

    pub emissive_surfs_begin: u32,
    pub emissive_surfs_end: u32,

    pub planes: Option<Ptr>,

    pub nodes: Option<Ptr>,

    pub cells: Option<Ptr>,
    pub sorted_surf_index: Option<Ptr>,
    pub smodel_insts: Option<Ptr>,
    pub smodel_draw_insts: Option<Ptr>,
    pub dpvs_static: Option<Ptr>,

    pub bounds: Option<[u32; 6]>,
}

#[derive(Clone, Copy, Debug)]
pub struct GfxLightDefGeometry {
    pub name: Option<Ptr>,
    pub attenuation_image_name: Option<Ptr>,
    pub attenuation_width: Option<u16>,
    pub attenuation_sampler: u8,
    pub lmap_lookup_start: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComWorldGeometry {
    pub primary_lights: Option<Ptr>,
    pub primary_light_count: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClipMapGeometry {
    pub name: Option<Ptr>,

    pub static_models: Option<Ptr>,

    pub static_model_count: usize,
    pub plane_count: usize,
    pub brush_count: usize,
    pub cmodel_count: usize,
    pub leaf_count: usize,
    pub node_count: usize,
    pub vert_count: usize,
    pub tri_count: usize,
    pub planes: Option<Ptr>,
    pub brushes: Option<Ptr>,

    pub verts: Option<Ptr>,

    pub tri_indices: Option<Ptr>,

    pub tri_edge_is_walkable: Option<Ptr>,

    pub materials: Option<Ptr>,
    pub material_count: usize,

    pub collision_partitions: Option<Ptr>,
    pub partition_count: usize,

    pub collision_aabb_trees: Option<Ptr>,
    pub aabb_tree_count: usize,

    pub nodes: Option<Ptr>,

    pub leaves: Option<Ptr>,

    pub leafbrush_nodes: Option<Ptr>,

    pub cmodels: Option<Ptr>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapEntsGeometry {
    pub entity_string: Option<Ptr>,
    pub entity_chars: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MaterialGeometry {
    pub name: Option<Ptr>,
    pub draw_surf: u64,

    pub surface_type_bits: u32,
    pub layered_surface_types: u32,
    pub sort_key: u8,

    pub info_game_flags: u8,

    pub state_flags: u8,

    pub camera_region: u8,

    pub state_bits: Option<Ptr>,

    pub state_bits_count: usize,

    pub state_bits_entry: Option<[u8; crate::size::TECHNIQUE_SLOT_COUNT]>,
    pub textures: Option<Ptr>,
    pub texture_count: usize,
    pub constants: Option<Ptr>,
    pub constant_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TechniqueSetGeometry {
    pub name: Option<Ptr>,

    pub technique_slots: [u64; crate::size::TECHNIQUE_OCCUPANCY_WORDS],

    pub technique_slots_scanned: [u64; crate::size::TECHNIQUE_OCCUPANCY_WORDS],

    pub technique_body_by_slot: [Option<Ptr>; crate::size::TECHNIQUE_SLOT_COUNT],

    pub technique0_flags: u8,

    pub world_vert_format: u8,

    pub max_pass_count: u16,

    pub pass_count_by_slot: [u8; crate::size::TECHNIQUE_SLOT_COUNT],

    pub technique_flags_by_slot: [u16; crate::size::TECHNIQUE_SLOT_COUNT],
}

impl Default for TechniqueSetGeometry {
    fn default() -> Self {
        Self {
            name: None,
            technique_slots: [0; crate::size::TECHNIQUE_OCCUPANCY_WORDS],
            technique_slots_scanned: [0; crate::size::TECHNIQUE_OCCUPANCY_WORDS],
            technique_body_by_slot: [None; crate::size::TECHNIQUE_SLOT_COUNT],
            technique0_flags: 0,
            world_vert_format: 0,
            max_pass_count: 0,
            pass_count_by_slot: [0; crate::size::TECHNIQUE_SLOT_COUNT],
            technique_flags_by_slot: [0; crate::size::TECHNIQUE_SLOT_COUNT],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexDeclGeometry {
    pub header: Option<Ptr>,
    pub stream_count: u8,
    pub has_optional_source: u8,
    pub routing: [[u8; 2]; crate::size::VERTEX_DECL_ROUTING_COUNT],
}

impl Default for VertexDeclGeometry {
    fn default() -> Self {
        Self {
            header: None,
            stream_count: 0,
            has_optional_source: 0,
            routing: [[0; 2]; crate::size::VERTEX_DECL_ROUTING_COUNT],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShaderGeometry {
    pub header: Option<Ptr>,
    pub name: Option<Ptr>,
    pub program: Option<Ptr>,
    pub program_words: usize,
}

pub const TECHNIQUE_PASS_ROW_CAP: usize = 512;
pub const TECHNIQUE_ARGUMENT_CAP: usize = 4096;

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
            align_waste: AlignWasteStats::default(),
            align_waste_site: AlignWasteSite::Unscoped,
            diag_skip_coll_nodes_virtual: false,
            header,
            xmodel: None,
            weapon: None,
            gfx_world: None,
            clip_map: None,
            map_ents: None,
            com_world: None,
            light_defs: [GfxLightDefGeometry {
                name: None,
                attenuation_image_name: None,
                attenuation_width: None,
                attenuation_sampler: 0,
                lmap_lookup_start: 0,
            }; 128],
            light_def_count: 0,
            latest_image: None,
            latest_material: None,
            latest_technique_set: None,
            technique_graph: TechniqueGraphGeometry::default(),
            technique_graph_seen: false,
            latest_shader: None,
            latest_vertex_decl: None,
            image_serial: 0,
        })
    }

    pub fn header(&self) -> &ZoneHeader {
        &self.header
    }

    pub fn record_xmodel(&mut self, geometry: XModelGeometry) {
        self.xmodel = Some(geometry);
    }

    pub fn latest_xmodel(&self) -> Option<XModelGeometry> {
        self.xmodel
    }

    pub fn record_weapon(&mut self, geometry: WeaponGeometry) {
        self.weapon = Some(geometry);
    }

    pub fn weapon(&self) -> Option<WeaponGeometry> {
        self.weapon
    }

    pub fn record_gfx_world(&mut self, geometry: GfxWorldGeometry) {
        self.gfx_world = Some(geometry);
    }

    pub fn gfx_world(&self) -> Option<GfxWorldGeometry> {
        self.gfx_world
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

    pub fn record_com_world(&mut self, geometry: ComWorldGeometry) {
        self.com_world = Some(geometry);
    }

    pub fn com_world(&self) -> Option<ComWorldGeometry> {
        self.com_world
    }

    pub fn record_image(&mut self, image: GfxImageGeometry) {
        self.latest_image = Some(image);
        self.image_serial = self.image_serial.wrapping_add(1);
    }

    pub fn latest_image(&self) -> Option<GfxImageGeometry> {
        self.latest_image
    }

    pub fn image_serial(&self) -> u32 {
        self.image_serial
    }

    pub fn record_material(&mut self, geometry: MaterialGeometry) {
        self.latest_material = Some(geometry);
    }

    pub fn latest_material(&self) -> Option<MaterialGeometry> {
        self.latest_material
    }

    pub fn record_technique_set(&mut self, geometry: TechniqueSetGeometry) {
        self.latest_technique_set = Some(geometry);
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

    pub fn record_light_def(&mut self, def: GfxLightDefGeometry) -> Result<()> {
        let slot = self
            .light_defs
            .get_mut(self.light_def_count)
            .ok_or(ZoneError::LightDefTableFull)?;
        *slot = def;
        self.light_def_count += 1;
        Ok(())
    }
    pub fn light_defs(&self) -> &[GfxLightDefGeometry] {
        &self.light_defs[..self.light_def_count]
    }

    pub fn record_shader(&mut self, shader: ShaderGeometry) {
        self.latest_shader = Some(shader);
    }

    pub fn latest_shader(&self) -> Option<ShaderGeometry> {
        self.latest_shader
    }

    pub fn record_vertex_decl(&mut self, decl: VertexDeclGeometry) {
        self.latest_vertex_decl = Some(decl);
    }

    pub fn latest_vertex_decl(&self) -> Option<VertexDeclGeometry> {
        self.latest_vertex_decl
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

    pub fn peek(&self, n: usize) -> &[u8] {
        let end = (self.cursor + n).min(self.data.len());
        &self.data[self.cursor..end]
    }

    pub fn diag_skip_stream(&mut self, size: usize) -> Result<()> {
        let s = self.cursor;
        if s + size > self.data.len() {
            return Err(ZoneError::Truncated {
                at: s,
                needed: size,
                len: self.data.len(),
            });
        }
        self.cursor += size;
        Ok(())
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

    pub fn align_pos(&mut self, align: usize) -> Result<()> {
        let b = self.top()?;
        let before = self.offsets[b];
        let pos = align_up(before, align);
        if pos > self.blocks[b].len() {
            return Err(ZoneError::BlockOverflow {
                block: b,
                end: pos,
                size: self.blocks[b].len(),
                request: 0,
            });
        }
        self.record_align_waste(b, align, pos - before, 0);
        self.offsets[b] = pos;
        Ok(())
    }

    pub fn alloc_load(&mut self, align: usize, size: usize) -> Result<Ptr> {
        let b = self.top()?;
        let before = self.offsets[b];
        let pos = align_up(before, align);
        let end = pos + size;
        if end > self.blocks[b].len() {
            return Err(ZoneError::BlockOverflow {
                block: b,
                end,
                size: self.blocks[b].len(),
                request: size,
            });
        }
        self.record_align_waste(b, align, pos - before, size);

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
        let before = self.offsets[b];
        let pos = align_up(before, PTR_SIZE);
        let end = pos + PTR_SIZE;
        if end > self.blocks[b].len() {
            return Err(ZoneError::BlockOverflow {
                block: b,
                end,
                size: self.blocks[b].len(),
                request: PTR_SIZE,
            });
        }
        let pad = (pos - before) as u64;
        self.align_waste.insert_slots += 1;
        self.align_waste.insert_pad_bytes += pad;

        self.record_align_waste(b, PTR_SIZE, pos - before, PTR_SIZE);
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
        let b = slot.block as usize;
        let off = slot.offset as usize;
        if b >= MAX_XFILE_COUNT || off + PTR_SIZE > self.blocks[b].len() {
            return Err(ZoneError::BadOffset {
                block: b,
                offset: off,
                size: self.blocks.get(b).map_or(0, |x| x.len()),
            });
        }
        self.blocks[b][off..off + PTR_SIZE]
            .copy_from_slice(&ZonePtr::encode_offset(body).to_le_bytes());
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
        match self.u32_at(p, 0).map(ZonePtr::decode) {
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

    pub fn slice_at(&self, p: Ptr, off: usize, len: usize) -> Result<&[u8]> {
        self.bytes_at(p, off, len)
    }

    pub fn u16_at(&self, p: Ptr, off: usize) -> Result<u16> {
        let b = self.bytes_at(p, off, 2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn i16_at(&self, p: Ptr, off: usize) -> Result<i16> {
        Ok(self.u16_at(p, off)? as i16)
    }

    pub fn i32_at(&self, p: Ptr, off: usize) -> Result<i32> {
        Ok(self.u32_at(p, off)? as i32)
    }

    pub fn u32_at(&self, p: Ptr, off: usize) -> Result<u32> {
        let b = self.bytes_at(p, off, 4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn f32_at(&self, p: Ptr, off: usize) -> Result<f32> {
        Ok(f32::from_bits(self.u32_at(p, off)?))
    }

    pub fn ptr_at(&self, p: Ptr, off: usize) -> Result<ZonePtr> {
        Ok(ZonePtr::decode(self.u32_at(p, off)?))
    }

    pub fn first_unsettled(&self) -> Option<(Ptr, usize, usize)> {
        self.first_unsettled
    }

    pub fn fixup_slot(&mut self, slot: Ptr, body: Ptr) -> Result<()> {
        self.write_u32_at(slot, 0, ZonePtr::encode_offset(body))
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

    pub fn cstr(&self, p: Ptr) -> Result<&str> {
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
        core::str::from_utf8(&buf[s..end]).map_err(|_| ZoneError::NotUtf8)
    }

    pub fn watermark(&self, block: u8) -> usize {
        self.offsets
            .get(block as usize)
            .copied()
            .unwrap_or_default()
    }

    pub fn block_overrun(&self, block: u8) -> usize {
        let index = block as usize;
        assert!(
            index < MAX_XFILE_COUNT,
            "T5 block index {block} exceeds {MAX_XFILE_COUNT} blocks"
        );
        self.offsets[index].saturating_sub(self.header.block_size[index] as usize)
    }

    pub fn align_waste(&self) -> AlignWasteStats {
        self.align_waste
    }

    pub(crate) fn align_waste_verts0_phase(&mut self, phase: usize) {
        if let Some(slot) = self.align_waste.verts0_phase.get_mut(phase) {
            *slot += 1;
        }
    }

    pub(crate) fn align_waste_xmodel_entry_phase(&mut self, phase: usize) {
        if let Some(slot) = self.align_waste.xmodel_entry_phase.get_mut(phase) {
            *slot += 1;
        }
    }

    pub fn set_diag_skip_coll_nodes_virtual(&mut self, skip: bool) {
        self.diag_skip_coll_nodes_virtual = skip;
    }

    pub fn diag_skip_coll_nodes_virtual(&self) -> bool {
        self.diag_skip_coll_nodes_virtual
    }

    pub fn with_align_site<R>(
        &mut self,
        site: AlignWasteSite,
        f: impl FnOnce(&mut Self) -> Result<R>,
    ) -> Result<R> {
        let prev = self.align_waste_site;
        self.align_waste_site = site;
        let out = f(self);
        self.align_waste_site = prev;
        out
    }

    fn record_align_waste(&mut self, block: usize, align: usize, pad: usize, size: usize) {
        if block != XFILE_BLOCK_VIRTUAL {
            return;
        }
        let pad = pad as u64;
        let site = self.align_waste_site.index();
        self.align_waste.allocs += 1;
        self.align_waste.pad_bytes += pad;
        self.align_waste.site_allocs[site] += 1;
        self.align_waste.site_pad[site] += pad;
        match align {
            2 => {
                self.align_waste.allocs_align_2 += 1;
                self.align_waste.pad_align_2 += pad;
            }
            4 => {
                self.align_waste.allocs_align_4 += 1;
                self.align_waste.pad_align_4 += pad;
            }
            16 => {
                self.align_waste.allocs_align_16 += 1;
                self.align_waste.pad_align_16 += pad;
                self.align_waste.site_a16_allocs[site] += 1;
                self.align_waste.site_a16_pad[site] += pad;
                if size == 0 {
                    self.align_waste.zero_size_a16_allocs += 1;
                    self.align_waste.zero_size_a16_pad += pad;
                }
            }
            _ => {
                self.align_waste.allocs_align_other += 1;
                self.align_waste.pad_align_other += pad;
            }
        }
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
}
