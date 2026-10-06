pub const SND_DRIVER_GLOBALS: usize = 52;

pub const SND_DRIVER_GLOBAL_ARRAYS: [(usize, usize, usize); 6] = [
    (4, 8, 80),
    (12, 16, 100),
    (20, 24, 60),
    (28, 32, 32),
    (36, 40, 40),
    (44, 48, 176),
];

pub const TECHNIQUE_SET: usize = 528;
pub const TECHNIQUE_SLOT_COUNT: usize = 130;

pub const TECHNIQUE_SET_TECHNIQUES_OFF: usize = 8;

pub const TECHNIQUE_SET_WORLD_VERT_FORMAT_OFF: usize = 4;

pub const TECHNIQUE_FLAGS_OFF: usize = 4;

pub const TECHNIQUE_PASS_COUNT_OFF: usize = 6;

pub const TECHNIQUE_OCCUPANCY_WORDS: usize = 3;
pub const MATERIAL_PASS: usize = 20;
pub const MATERIAL_SHADER_ARGUMENT: usize = 8;
pub const VERTEX_DECL: usize = 108;

pub const VERTEX_DECL_STREAM_COUNT_OFF: usize = 0;
pub const VERTEX_DECL_HAS_OPTIONAL_SOURCE_OFF: usize = 1;

pub const VERTEX_DECL_ROUTING_OFF: usize = 4;
pub const VERTEX_DECL_ROUTING_COUNT: usize = 16;
pub const VERTEX_SHADER: usize = 16;

pub const MATERIAL: usize = 192;

pub const MATERIAL_INFO_SIZE: usize = 40;

pub const MATERIAL_INFO_GAME_FLAGS_OFF: usize = 4;

pub const MATERIAL_STATE_BITS_ENTRY_OFF: usize = 0x28;

pub const MATERIAL_TEXTURE_COUNT_OFF: usize = 170;
pub const MATERIAL_TECHNIQUE_SET_OFF: usize = 176;
pub const MATERIAL_TEXTURE_TABLE_OFF: usize = 180;
pub const MATERIAL_CONSTANT_TABLE_OFF: usize = 184;
pub const MATERIAL_STATE_BITS_OFF: usize = 188;

pub const MATERIAL_STATE_FLAGS_OFF: usize = 0xad;

pub const MATERIAL_CAMERA_REGION_OFF: usize = 0xae;

pub const MATERIAL_TEXTURE_DEF: usize = 16;

pub const MATERIAL_TEXTURE_DEF_U_OFF: usize = 12;
pub const MATERIAL_TEXTURE_DEF_SEMANTIC_OFF: usize = 7;
pub const MATERIAL_CONSTANT_DEF: usize = 32;
pub const GFX_STATE_BITS: usize = 8;
pub const WATER: usize = 68;
pub const SEMANTIC_WATER: u8 = 11;

pub const GFX_IMAGE: usize = 52;

pub const GFX_IMAGE_NAME_OFF: usize = 44;

pub const GFX_IMAGE_LOAD_DEF_HEAD: usize = 12;

pub const RAW_FILE: usize = 12;

pub const LOCALIZE_ENTRY: usize = 8;
pub const PHYS_PRESET: usize = 84;

pub const STRING_TABLE: usize = 20;
pub const STRING_TABLE_CELL: usize = 8;
pub const STRING_TABLE_VALUES_OFF: usize = 12;
pub const STRING_TABLE_CELL_INDEX_OFF: usize = 16;

pub const FONT: usize = 24;
pub const FONT_GLYPH: usize = 24;
pub const FONT_GLYPH_COUNT_OFF: usize = 8;
pub const FONT_MATERIAL_OFF: usize = 12;
pub const FONT_GLOW_MATERIAL_OFF: usize = 16;
pub const FONT_GLYPHS_OFF: usize = 20;

pub const XMODEL: usize = 252;
pub const XMODEL_COLL_SURFS_OFF: usize = 172;
pub const XMODEL_NUM_COLL_SURFS_OFF: usize = 176;
pub const XMODEL_BONE_INFO_OFF: usize = 184;

pub const XMODEL_RADIUS_OFF: usize = 188;
pub const XMODEL_STREAM_INFO_OFF: usize = 220;
pub const XMODEL_PHYS_PRESET_OFF: usize = 236;
pub const XMODEL_NUM_COLLMAPS_OFF: usize = 240;
pub const XMODEL_COLLMAPS_OFF: usize = 244;
pub const XMODEL_PHYS_CONSTRAINTS_OFF: usize = 248;

pub const XMODEL_QUAT: usize = 8;

pub const XMODEL_TRANS_OCCUPANCY: usize = 16;
pub const XMODEL_TRANS_STRIDE: usize = 12;
pub const DOBJ_ANIM_MAT: usize = 32;
pub const XSURFACE: usize = 68;

pub const XSURFACE_PART_BITS_OFF: usize = 48;
pub const XSURFACE_PART_BITS_WORDS: usize = 5;
pub const XRIGID_VERT_LIST: usize = 12;
pub const XSURFACE_COLLISION_TREE: usize = 40;
pub const XSURFACE_COLLISION_NODE: usize = 16;
pub const XSURFACE_COLLISION_LEAF: usize = 2;
pub const GFX_PACKED_VERTEX: usize = 32;
pub const XMODEL_COLL_SURF: usize = 44;
pub const XMODEL_COLL_TRI: usize = 48;
pub const XBONE_INFO: usize = 44;
pub const XMODEL_HIGH_MIP_BOUNDS: usize = 16;
pub const COLLMAP: usize = 4;
pub const PHYS_GEOM_LIST: usize = 12;
pub const PHYS_GEOM_INFO: usize = 68;
pub const BRUSH_WRAPPER: usize = 96;
pub const BRUSH_WRAPPER_NUMSIDES_OFF: usize = 28;
pub const BRUSH_WRAPPER_SIDES_OFF: usize = 32;
pub const BRUSH_WRAPPER_NUMVERTS_OFF: usize = 84;
pub const BRUSH_WRAPPER_VERTS_OFF: usize = 88;
pub const BRUSH_WRAPPER_PLANES_OFF: usize = 92;
pub const CBRUSH_SIDE: usize = 12;
pub const CPLANE: usize = 20;

pub const PHYS_CONSTRAINTS: usize = 2696;
pub const PHYS_CONSTRAINT: usize = 168;
pub const PHYS_CONSTRAINT_BONE1_OFF: usize = 20;
pub const PHYS_CONSTRAINT_BONE2_OFF: usize = 36;
pub const PHYS_CONSTRAINT_MATERIAL_OFF: usize = 0x8c;

pub const DESTRUCTIBLE_DEF: usize = 24;
pub const DESTRUCTIBLE_PIECE: usize = 312;
pub const DESTRUCTIBLE_STAGE: usize = 48;
pub const DESTRUCTIBLE_PIECE_PHYS_OFF: usize = 268;
pub const DESTRUCTIBLE_PIECE_DAMAGE_SOUND_OFF: usize = 276;
pub const DESTRUCTIBLE_PIECE_BURN_FX_OFF: usize = 280;
pub const DESTRUCTIBLE_PIECE_BURN_SOUND_OFF: usize = 284;

pub const COM_WORLD: usize = 64;
pub const COM_PRIMARY_LIGHT: usize = 220;

pub const COM_PRIMARY_LIGHT_DIFFUSE_COLOR_OFF: usize = 72;

pub const COM_PRIMARY_LIGHT_SPECULAR_COLOR_OFF: usize = 88;

pub const COM_PRIMARY_LIGHT_ATTENUATION_OFF: usize = 104;

pub const COM_PRIMARY_LIGHT_FALLOFF_OFF: usize = 120;

pub const COM_PRIMARY_LIGHT_A_AB_B_OFF: usize = 152;

pub const COM_PRIMARY_LIGHT_ANGLE_OFF: usize = 136;

pub const COM_PRIMARY_LIGHT_COOKIE0_OFF: usize = 168;

pub const COM_PRIMARY_LIGHT_COOKIE1_OFF: usize = 184;

pub const COM_PRIMARY_LIGHT_COOKIE2_OFF: usize = 200;
pub const COM_PRIMARY_LIGHT_DEF_NAME_OFF: usize = 216;
pub const COM_WATER_CELL: usize = 8;
pub const COM_BURNABLE_CELL: usize = 12;
pub const GFX_LIGHT_DEF: usize = 16;

pub const FX_EFFECT_DEF: usize = 60;
pub const FX_EFFECT_DEF_FLAGS_OFF: usize = 4;
pub const FX_EFFECT_DEF_MSEC_LOOPING_LIFE_OFF: usize = 12;
pub const FX_EFFECT_DEF_LOOPING_OFF: usize = 16;
pub const FX_EFFECT_DEF_ONESHOT_OFF: usize = 20;
pub const FX_EFFECT_DEF_EMISSION_OFF: usize = 24;
pub const FX_EFFECT_DEF_ELEMS_OFF: usize = 28;
pub const FX_ELEM_DEF: usize = 292;

pub const FX_ELEM_ATLAS_OFF: usize = 0xac;
pub const FX_ELEM_ATLAS_SIZE: usize = 8;
pub const FX_ELEM_TYPE_OFF: usize = 184;
pub const FX_ELEM_VEL_SAMPLES_OFF: usize = 188;
pub const FX_ELEM_VIS_SAMPLES_OFF: usize = 192;
pub const FX_ELEM_VISUALS_OFF: usize = 196;
pub const FX_ELEM_COLL_MINS_OFF: usize = 0xc8;
pub const FX_ELEM_EFFECT_ON_IMPACT_OFF: usize = 224;
pub const FX_ELEM_EFFECT_ON_DEATH_OFF: usize = 228;
pub const FX_ELEM_EFFECT_EMITTED_OFF: usize = 232;
pub const FX_ELEM_EMIT_DIST_OFF: usize = 0xec;
pub const FX_ELEM_EFFECT_ATTACHED_OFF: usize = 252;
pub const FX_ELEM_TRAIL_DEF_OFF: usize = 256;
pub const FX_ELEM_SORT_ORDER_OFF: usize = 260;
pub const FX_ELEM_LIGHTING_FRAC_OFF: usize = 261;
pub const FX_ELEM_SPAWN_SOUND_OFF: usize = 280;
pub const FX_ELEM_VEL_STATE_SAMPLE: usize = 96;
pub const FX_ELEM_VIS_STATE_SAMPLE: usize = 48;
pub const FX_ELEM_MARK_VISUALS: usize = 8;
pub const FX_ELEM_VISUALS: usize = 4;
pub const FX_TRAIL_DEF: usize = 28;
pub const FX_TRAIL_VERTEX: usize = 20;

pub const FX_ELEM_MODEL: u8 = 7;
pub const FX_ELEM_OMNI_LIGHT: u8 = 8;
pub const FX_ELEM_SPOT_LIGHT: u8 = 9;
pub const FX_ELEM_SOUND: u8 = 10;
pub const FX_ELEM_DECAL: u8 = 11;
pub const FX_ELEM_RUNNER: u8 = 12;

pub const fn leftover_iw4_elem_type(t5: u8) -> u8 {
    match t5 {
        0 => 0,
        1 => 1,
        2 => 1,
        3 => 2,
        4 => 2,
        5 => 3,
        6 => 4,
        other => other,
    }
}

pub const LITERAL_VERTEX_CONST: u16 = 1;
pub const LITERAL_PIXEL_CONST: u16 = 7;

pub const GFX_WORLD: usize = 1084;
pub const GFX_WORLD_STREAM_INFO_OFF: usize = 0x14;
pub const GFX_WORLD_SKY_SURF_COUNT_OFF: usize = 0x24;
pub const GFX_WORLD_SKY_START_SURFS_OFF: usize = 0x28;
pub const GFX_WORLD_SKY_IMAGE_OFF: usize = 0x2c;
pub const GFX_WORLD_SKY_BOX_MODEL_OFF: usize = 0x34;

pub const GFX_WORLD_SUN_PARSE_EXPOSURE_OFF: usize = 0xe0;

pub const GFX_WORLD_SUN_PARSE_TREE_SCATTER_INTENSITY_OFF: usize = 0x78;

pub const GFX_WORLD_SUN_PARSE_TREE_SCATTER_AMOUNT_OFF: usize = 0x7c;
pub const GFX_WORLD_SUN_LIGHT_OFF: usize = 0xec;
pub const GFX_WORLD_SUN_PRIMARY_LIGHT_INDEX_OFF: usize = 0xfc;
pub const GFX_WORLD_PRIMARY_LIGHT_COUNT_OFF: usize = 0x100;
pub const GFX_WORLD_CULL_GROUP_COUNT_OFF: usize = 0x104;
pub const GFX_WORLD_CORONA_COUNT_OFF: usize = 0x108;
pub const GFX_WORLD_CORONAS_OFF: usize = 0x10c;
pub const GFX_WORLD_SHADOW_MAP_VOLUME_COUNT_OFF: usize = 0x110;
pub const GFX_WORLD_SHADOW_MAP_VOLUMES_OFF: usize = 0x114;
pub const GFX_WORLD_SHADOW_MAP_VOLUME_PLANE_COUNT_OFF: usize = 0x118;
pub const GFX_WORLD_SHADOW_MAP_VOLUME_PLANES_OFF: usize = 0x11c;
pub const GFX_WORLD_EXPOSURE_VOLUME_COUNT_OFF: usize = 0x120;
pub const GFX_WORLD_EXPOSURE_VOLUMES_OFF: usize = 0x124;
pub const GFX_WORLD_EXPOSURE_VOLUME_PLANE_COUNT_OFF: usize = 0x128;
pub const GFX_WORLD_EXPOSURE_VOLUME_PLANES_OFF: usize = 0x12c;
pub const GFX_WORLD_DPVS_PLANES_OFF: usize = 0x140;
pub const GFX_WORLD_CELLS_OFF: usize = 0x154;
pub const GFX_WORLD_DRAW_OFF: usize = 0x158;
pub const GFX_WORLD_LIGHT_GRID_OFF: usize = 0x218;
pub const GFX_WORLD_MODEL_COUNT_OFF: usize = 0x250;
pub const GFX_WORLD_MODELS_OFF: usize = 0x254;
pub const GFX_WORLD_MINS_OFF: usize = 0x258;
pub const GFX_WORLD_MATERIAL_MEMORY_COUNT_OFF: usize = 0x274;
pub const GFX_WORLD_MATERIAL_MEMORY_OFF: usize = 0x278;
pub const GFX_WORLD_SUN_OFF: usize = 0x27c;
pub const GFX_WORLD_OUTDOOR_IMAGE_OFF: usize = 0x31c;
pub const GFX_WORLD_CELL_CASTER_BITS_OFF: usize = 0x320;
pub const GFX_WORLD_SCENE_DYN_MODEL_OFF: usize = 0x324;
pub const GFX_WORLD_SCENE_DYN_BRUSH_OFF: usize = 0x328;
pub const GFX_WORLD_PRIMARY_LIGHT_ENTITY_SHADOW_VIS_OFF: usize = 0x32c;
pub const GFX_WORLD_PRIMARY_LIGHT_DYN_ENT_SHADOW_VIS_OFF: usize = 0x330;
pub const GFX_WORLD_NON_SUN_PRIMARY_LIGHT_FOR_MODEL_DYN_ENT_OFF: usize = 0x338;
pub const GFX_WORLD_SHADOW_GEOM_OFF: usize = 0x33c;
pub const GFX_WORLD_LIGHT_REGION_OFF: usize = 0x340;
pub const GFX_WORLD_DPVS_OFF: usize = 0x344;
pub const GFX_WORLD_DPVS_DYN_OFF: usize = 0x3b4;
pub const GFX_WORLD_LOD_CHAIN_COUNT_OFF: usize = 0x3e4;
pub const GFX_WORLD_LOD_CHAINS_OFF: usize = 0x3e8;
pub const GFX_WORLD_LOD_INFO_COUNT_OFF: usize = 0x3ec;
pub const GFX_WORLD_LOD_INFOS_OFF: usize = 0x3f0;
pub const GFX_WORLD_LOD_SURFACE_COUNT_OFF: usize = 0x3f4;
pub const GFX_WORLD_LOD_SURFACES_OFF: usize = 0x3f8;
pub const GFX_WORLD_WATER_BUFFERS_OFF: usize = 0x400;
pub const GFX_WORLD_WATER_MATERIAL_OFF: usize = 0x410;
pub const GFX_WORLD_CORONA_MATERIAL_OFF: usize = 0x414;
pub const GFX_WORLD_ROPE_MATERIAL_OFF: usize = 0x418;
pub const GFX_WORLD_NUM_OCCLUDERS_OFF: usize = 0x41c;
pub const GFX_WORLD_OCCLUDERS_OFF: usize = 0x420;
pub const GFX_WORLD_NUM_OUTDOOR_BOUNDS_OFF: usize = 0x424;
pub const GFX_WORLD_OUTDOOR_BOUNDS_OFF: usize = 0x428;
pub const GFX_WORLD_HERO_LIGHT_COUNT_OFF: usize = 0x42c;
pub const GFX_WORLD_HERO_LIGHT_TREE_COUNT_OFF: usize = 0x430;
pub const GFX_WORLD_HERO_LIGHTS_OFF: usize = 0x434;
pub const GFX_WORLD_HERO_LIGHT_TREE_OFF: usize = 0x438;

pub const GFX_WORLD_STREAM_INFO: usize = 16;
pub const GFX_STREAMING_AABB_TREE: usize = 32;
pub const GFX_LIGHT: usize = 368;
pub const GFX_LIGHT_DEF_OFF: usize = 0x160;
pub const GFX_LIGHT_CORONA: usize = 32;
pub const GFX_SHADOW_MAP_VOLUME: usize = 16;
pub const GFX_EXPOSURE_VOLUME: usize = 24;
pub const GFX_VOLUME_PLANE: usize = 16;
pub const GFX_WORLD_DPVS_PLANES: usize = 16;
pub const GFX_CELL: usize = 56;
pub const GFX_AABB_TREE: usize = 40;
pub const GFX_AABB_TREE_SMODEL_INDEX_COUNT_OFF: usize = 0x1e;
pub const GFX_AABB_TREE_SMODEL_INDEXES_OFF: usize = 0x20;
pub const GFX_PORTAL: usize = 68;
pub const GFX_PORTAL_CELL_OFF: usize = 0x20;
pub const GFX_PORTAL_VERTICES_OFF: usize = 0x24;
pub const GFX_PORTAL_VERTEX_COUNT_OFF: usize = 0x28;
pub const GFX_WORLD_DRAW: usize = 192;
pub const GFX_REFLECTION_PROBE: usize = 24;
pub const GFX_REFLECTION_PROBE_IMAGE_OFF: usize = 12;
pub const GFX_REFLECTION_PROBE_VOLUMES_OFF: usize = 16;
pub const GFX_REFLECTION_PROBE_VOLUME_COUNT_OFF: usize = 20;
pub const GFX_REFLECTION_PROBE_VOLUME_DATA: usize = 96;
pub const GFX_LIGHTMAP_ARRAY: usize = 12;
pub const GFX_WORLD_VERTEX: usize = 44;
pub const GFX_LIGHT_GRID: usize = 56;
pub const GFX_LIGHT_GRID_COLORS: usize = 168;
pub const GFX_BRUSH_MODEL: usize = 60;
pub const MATERIAL_MEMORY: usize = 8;
pub const SUNFLARE: usize = 96;
pub const SUNFLARE_SPRITE_MATERIAL_OFF: usize = 4;
pub const SUNFLARE_FLARE_MATERIAL_OFF: usize = 8;
pub const GFX_SCENE_DYN_MODEL: usize = 6;
pub const GFX_SCENE_DYN_BRUSH: usize = 4;
pub const GFX_SHADOW_GEOMETRY: usize = 12;
pub const GFX_LIGHT_REGION: usize = 8;
pub const GFX_LIGHT_REGION_HULL: usize = 80;
pub const GFX_LIGHT_REGION_AXIS: usize = 20;
pub const GFX_WORLD_DPVS_STATIC: usize = 112;
pub const GFX_WORLD_DPVS_DYNAMIC: usize = 48;

pub const GFX_STATIC_MODEL_INST: usize = 40;
pub const GFX_STATIC_MODEL_INST_LIGHTING_ORIGIN: usize = 0x18;
pub const GFX_SURFACE: usize = 80;
pub const GFX_SURFACE_MATERIAL_OFF: usize = 0x30;

pub const GFX_CULL_GROUP: usize = 32;
pub const GFX_STATIC_MODEL_DRAW_INST: usize = 76;
pub const GFX_STATIC_MODEL_DRAW_INST_MODEL_OFF: usize = 0x38;
pub const GFX_DRAW_SURF: usize = 8;
pub const GFX_WORLD_LOD_CHAIN: usize = 24;
pub const GFX_WORLD_LOD_INFO: usize = 12;
pub const GFX_WATER_BUFFER: usize = 8;
pub const OCCLUDER: usize = 68;
pub const GFX_OUTDOOR_BOUNDS: usize = 24;
pub const GFX_HERO_LIGHT: usize = 56;
pub const GFX_HERO_LIGHT_TREE: usize = 24;

pub const GAME_WORLD_MP: usize = 44;
// GameWorldSp: same { name, PathData } shell as the MP variant.
pub const GAME_WORLD_SP: usize = 44;
// XGlobals: name + five ints + clear color. No pointers.
pub const XGLOBALS: usize = 40;
// EmblemSet header: counts interleaved with table pointers.
pub const EMBLEM_SET: usize = 44;
pub const EMBLEM_LAYER: usize = 12;
pub const EMBLEM_CATEGORY: usize = 8;
pub const EMBLEM_ICON: usize = 40;
pub const EMBLEM_BACKGROUND: usize = 24;
pub const PATH_DATA: usize = 40;
pub const PATH_DATA_OFF: usize = 4;
pub const PATH_NODE: usize = 128;
pub const PATH_NODE_TOTAL_LINK_COUNT_OFF: usize = 0x3e;
pub const PATH_NODE_LINKS_OFF: usize = 0x40;
pub const PATH_LINK: usize = 12;
pub const PATH_BASE_NODE: usize = 16;
pub const PATH_NODE_TREE: usize = 16;
pub const PATH_NODE_TREE_NODES: usize = 8;

pub const CLIP_MAP: usize = 332;
pub const C_STATIC_MODEL: usize = 80;
pub const C_STATIC_MODEL_XMODEL_OFF: usize = 4;
pub const DMATERIAL: usize = 72;
pub const CBRUSH_SIDE_PLANE_OFF: usize = 0;
pub const C_NODE: usize = 8;
pub const C_LEAF: usize = 44;
pub const C_LEAF_BRUSH_NODE: usize = 20;
pub const C_LEAF_BRUSH_NODE_COUNT_OFF: usize = 2;
pub const C_LEAF_BRUSH_NODE_DATA_OFF: usize = 8;
pub const COLLISION_BORDER: usize = 28;
pub const COLLISION_PARTITION: usize = 20;
pub const COLLISION_PARTITION_BORDERS_OFF: usize = 0x10;
pub const COLLISION_AABB_TREE: usize = 32;
pub const C_MODEL: usize = 72;
pub const C_BRUSH: usize = 96;
pub const C_BRUSH_SIDES_OFF: usize = 0x20;
pub const C_BRUSH_VERTS_OFF: usize = 0x58;
pub const MAP_ENTS: usize = 12;
pub const DYN_ENTITY_DEF: usize = 84;
pub const DYN_ENTITY_DEF_XMODEL_OFF: usize = 0x20;
pub const DYN_ENTITY_DEF_DESTROYED_XMODEL_OFF: usize = 0x24;
pub const DYN_ENTITY_DEF_DESTROY_FX_OFF: usize = 0x2c;
pub const DYN_ENTITY_DEF_DESTROY_PIECES_OFF: usize = 0x34;
pub const DYN_ENTITY_DEF_PHYS_PRESET_OFF: usize = 0x38;
pub const DYN_ENTITY_POSE: usize = 32;
pub const DYN_ENTITY_CLIENT: usize = 20;
pub const DYN_ENTITY_SERVER: usize = 8;
pub const DYN_ENTITY_COLL: usize = 32;
pub const ROPE: usize = 3188;
pub const XMODEL_PIECES: usize = 12;
pub const XMODEL_PIECE: usize = 16;

pub const SND_BANK: usize = 40;
pub const SND_ALIAS_LIST: usize = 20;
pub const SND_ALIAS: usize = 84;
pub const SND_INDEX_ENTRY: usize = 4;
pub const SND_RADVERB: usize = 96;
pub const SND_SNAPSHOT: usize = 348;
pub const SND_PATCH: usize = 20;
pub const SOUND_FILE: usize = 8;
pub const LOADED_SOUND: usize = 60;
pub const STREAMED_SOUND: usize = 8;
pub const PRIMED_SOUND: usize = 12;
pub const SND_ASSET: usize = 56;

pub const SAT_LOADED: u8 = 1;
pub const SOUND_FILE_UNION_OFF: usize = 0;
pub const SOUND_FILE_TYPE_OFF: usize = 4;
pub const SOUND_FILE_EXISTS_OFF: usize = 5;
pub const STREAMED_SOUND_FILENAME_OFF: usize = 0;
pub const LOADED_SOUND_NAME_OFF: usize = 0;
pub const LOADED_SOUND_ASSET_OFF: usize = 4;
pub const SND_ASSET_FRAME_COUNT_OFF: usize = 0x4;
pub const SND_ASSET_FRAME_RATE_OFF: usize = 0x8;
pub const SND_ASSET_CHANNEL_COUNT_OFF: usize = 0xc;
pub const SND_ASSET_BLOCK_SIZE_OFF: usize = 0x14;
pub const SND_ASSET_FORMAT_OFF: usize = 0x1c;
pub const SND_ASSET_SEEK_TABLE_COUNT_OFF: usize = 0x28;
pub const SND_ASSET_SEEK_TABLE_OFF: usize = 0x2c;
pub const SND_ASSET_DATA_SIZE_OFF: usize = 0x30;
pub const SND_ASSET_DATA_OFF: usize = 0x34;

pub const SND_ASSET_FORMAT_PCMS16: i32 = 0;

pub const SND_ASSET_FORMAT_WMA: i32 = 7;

pub const SND_ALIAS_LIST_NAME_OFF: usize = 0;
pub const SND_ALIAS_LIST_HEAD_OFF: usize = 8;
pub const SND_ALIAS_LIST_COUNT_OFF: usize = 12;
pub const SND_ALIAS_NAME_OFF: usize = 0;
pub const SND_ALIAS_SUBTITLE_OFF: usize = 8;
pub const SND_ALIAS_SECONDARY_OFF: usize = 12;
pub const SND_ALIAS_SOUND_FILE_OFF: usize = 16;
pub const SND_ALIAS_FLAGS_OFF: usize = 20;
pub const SND_ALIAS_START_DELAY_OFF: usize = 38;
pub const SND_ALIAS_VOL_MIN_OFF: usize = 44;
pub const SND_ALIAS_VOL_MAX_OFF: usize = 46;
pub const SND_ALIAS_PITCH_MIN_OFF: usize = 50;
pub const SND_ALIAS_PITCH_MAX_OFF: usize = 52;
pub const SND_ALIAS_DIST_MIN_OFF: usize = 56;
pub const SND_ALIAS_DIST_MAX_OFF: usize = 58;
pub const SND_ALIAS_PRIORITY_DISTANCE_OFF: usize = 60;
pub const SND_ALIAS_ENVELOP_MIN_OFF: usize = 62;
pub const SND_ALIAS_ENVELOP_MAX_OFF: usize = 64;
pub const SND_ALIAS_ENVELOP_PERCENTAGE_OFF: usize = 66;
pub const SND_ALIAS_PRIORITY_MIN_THRESHOLD_OFF: usize = 68;
pub const SND_ALIAS_PRIORITY_MAX_THRESHOLD_OFF: usize = 69;
pub const SND_ALIAS_PROBABILITY_OFF: usize = 70;
pub const SND_ALIAS_PRIORITY_MIN_OFF: usize = 73;
pub const SND_ALIAS_PRIORITY_MAX_OFF: usize = 74;

pub const SND_ALIAS_LIMIT_COUNT_OFF: usize = 80;
pub const SND_ALIAS_ENTITY_LIMIT_COUNT_OFF: usize = 81;

const _: () = assert!(SND_ALIAS_LIMIT_COUNT_OFF + 2 < SND_ALIAS);
const _: () = assert!(SND_ALIAS_ENTITY_LIMIT_COUNT_OFF + 1 < SND_ALIAS);

pub const XANIM_PARTS: usize = 104;
pub const XANIM_NAMES_OFF: usize = 0x40;
pub const XANIM_DATA_BYTE_OFF: usize = 0x44;
pub const XANIM_DATA_SHORT_OFF: usize = 0x48;
pub const XANIM_DATA_INT_OFF: usize = 0x4c;
pub const XANIM_RANDOM_DATA_SHORT_OFF: usize = 0x50;
pub const XANIM_RANDOM_DATA_BYTE_OFF: usize = 0x54;
pub const XANIM_RANDOM_DATA_INT_OFF: usize = 0x58;
pub const XANIM_INDICES_OFF: usize = 0x5c;
pub const XANIM_NOTIFY_OFF: usize = 0x60;
pub const XANIM_DELTA_PART_OFF: usize = 0x64;
pub const XANIM_NOTIFY_INFO: usize = 8;
pub const XANIM_DELTA_PART: usize = 8;

pub const FX_IMPACT_TABLE: usize = 8;

pub const FX_IMPACT_ENTRY: usize = 140;

pub const FX_IMPACT_ENTRY_COUNT: usize = 21;
pub const SURF_TYPE_NUM: usize = 31;
pub const FX_IMPACT_FLESH_COUNT: usize = 4;

pub const WEAPON_VARIANT_DEF: usize = 228;

pub const WEAPON_DEF: usize = 2056;

pub const WEAPON_MOVE_SPEED_SCALE_OFF: usize = 0x4a8;
pub const WEAPON_ADS_MOVE_SPEED_SCALE_OFF: usize = 0x4ac;

pub const WEAPON_TYPE_OFF: usize = 0x1c;

pub const WEAPON_CLASS_OFF: usize = 0x20;

pub const WEAPON_FIRE_TYPE_OFF: usize = 0x30;

pub const WEAPON_FIRE_TIME_OFF: usize = 0x3ac;

pub const WEAPON_RECHAMBER_TIME_OFF: usize = 0x3b4;

pub const WEAPON_DEF_RECHAMBER_BOLT_TIME_OFF: usize = 0x3b8;

pub const WEAPON_DROP_TIME_OFF: usize = 0x3f8;

pub const WEAPON_RAISE_TIME_OFF: usize = 0x3fc;

pub const WEAPON_QUICK_DROP_TIME_OFF: usize = 0x404;
pub const WEAPON_QUICK_RAISE_TIME_OFF: usize = 0x408;

const _: () = assert!(WEAPON_QUICK_DROP_TIME_OFF == WEAPON_DROP_TIME_OFF + 12);
const _: () = assert!(WEAPON_QUICK_RAISE_TIME_OFF == WEAPON_QUICK_DROP_TIME_OFF + 4);
const _: () = assert!(WEAPON_RAISE_TIME_OFF == WEAPON_DROP_TIME_OFF + 4);

pub const WEAPON_BOLT_ACTION_OFF: usize = 0x54f;

pub const WEAPON_VARIANT_CLIP_SIZE_OFF: usize = 0x20;

pub const WEAPON_VARIANT_RELOAD_TIME_OFF: usize = 0x24;
pub const WEAPON_VARIANT_RELOAD_EMPTY_TIME_OFF: usize = 0x28;
pub const WEAPON_VARIANT_RELOAD_QUICK_TIME_OFF: usize = 0x2c;
pub const WEAPON_VARIANT_RELOAD_QUICK_EMPTY_TIME_OFF: usize = 0x30;

pub const WEAPON_VARIANT_SILENCED_OFF: usize = 0x84;

pub const WEAPON_VARIANT_DUAL_MAG_OFF: usize = 0x85;
pub const WEAPON_VARIANT_RAPID_FIRE_OFF: usize = 0x88;

pub const WEAPON_VARIANT_ADS_TRANS_IN_OFF: usize = 0x34;
pub const WEAPON_VARIANT_ADS_TRANS_OUT_OFF: usize = 0x38;

pub const WEAPON_VARIANT_ADS_ZOOM_FOV1_OFF: usize = 0x64;

pub const WEAPON_VARIANT_ADS_ZOOM_FOV2_OFF: usize = 0x68;

pub const WEAPON_VARIANT_ADS_ZOOM_FOV3_OFF: usize = 0x6c;
pub const WEAPON_VARIANT_ADS_ZOOM_IN_FRAC_OFF: usize = 0x70;
pub const WEAPON_VARIANT_ADS_ZOOM_OUT_FRAC_OFF: usize = 0x74;

pub const WEAPON_VARIANT_ADS_VIEW_KICK_CENTER_SPEED_OFF: usize = 0x5c;

pub const WEAPON_VARIANT_HIP_VIEW_KICK_CENTER_SPEED_OFF: usize = 0x60;

pub const WEAPON_VARIANT_ADS_IN_RATE_OFF: usize = 0x7c;
pub const WEAPON_VARIANT_ADS_OUT_RATE_OFF: usize = 0x80;

pub const WEAPON_VARIANT_OVERLAY_SHADER_OFF: usize = 0x8c;

pub const WEAPON_VARIANT_OVERLAY_SHADER_LOWRES_OFF: usize = 0x90;

pub const WEAPON_DEF_NOTE_SOUND_KEYS_OFF: usize = 0x10;
pub const WEAPON_DEF_NOTE_SOUND_VALUES_OFF: usize = 0x14;

pub const WEAPON_DEF_IMPACT_TYPE_OFF: usize = 0x28;
pub const WEAPON_DEF_PARENT_WEAPON_NAME_OFF: usize = 0x3c;
pub const WEAPON_DEF_VIEW_FLASH_OFF: usize = 0x74;
pub const WEAPON_DEF_WORLD_FLASH_OFF: usize = 0x78;
pub const WEAPON_DEF_VIEW_SHELL_EJECT_OFF: usize = 0x190;
pub const WEAPON_DEF_WORLD_SHELL_EJECT_OFF: usize = 0x194;
pub const WEAPON_DEF_VIEW_LAST_SHOT_EJECT_OFF: usize = 0x198;
pub const WEAPON_DEF_WORLD_LAST_SHOT_EJECT_OFF: usize = 0x19c;

pub const WEAPON_DEF_SND_FIRE_OFF: usize = 0x98;
pub const WEAPON_DEF_SND_FIRE_PLAYER_OFF: usize = 0x9c;
pub const WEAPON_DEF_SND_EMPTY_FIRE_OFF: usize = 0xc0;
pub const WEAPON_DEF_SND_EMPTY_FIRE_PLAYER_OFF: usize = 0xc4;
pub const WEAPON_DEF_SND_MELEE_SWIPE_OFF: usize = 0xd0;
pub const WEAPON_DEF_SND_MELEE_SWIPE_PLAYER_OFF: usize = 0xd4;
pub const WEAPON_DEF_SND_MELEE_HIT_OFF: usize = 0xd8;
pub const WEAPON_DEF_SND_MELEE_MISS_OFF: usize = 0xdc;
pub const WEAPON_DEF_SND_RECHAMBER_OFF: usize = 0xe0;
pub const WEAPON_DEF_SND_RECHAMBER_PLAYER_OFF: usize = 0xe4;
pub const WEAPON_DEF_SND_RELOAD_OFF: usize = 0xe8;
pub const WEAPON_DEF_SND_RELOAD_PLAYER_OFF: usize = 0xec;
pub const WEAPON_DEF_SND_RELOAD_EMPTY_OFF: usize = 0xf0;
pub const WEAPON_DEF_SND_RELOAD_EMPTY_PLAYER_OFF: usize = 0xf4;
pub const WEAPON_DEF_SND_RELOAD_START_OFF: usize = 0xf8;
pub const WEAPON_DEF_SND_RELOAD_START_PLAYER_OFF: usize = 0xfc;
pub const WEAPON_DEF_SND_RELOAD_END_OFF: usize = 0x100;
pub const WEAPON_DEF_SND_RELOAD_END_PLAYER_OFF: usize = 0x104;
pub const WEAPON_DEF_SND_RAISE_PLAYER_OFF: usize = 0x154;
pub const WEAPON_DEF_SND_PUTAWAY_PLAYER_OFF: usize = 0x164;
pub const WEAPON_DEF_AMMO_COUNTER_CLIP_OFF: usize = 0x338;
pub const WEAPON_DEF_START_AMMO_OFF: usize = 0x33c;
pub const WEAPON_DEF_MAX_AMMO_OFF: usize = 0x344;

pub const WEAPON_DEF_AMMO_COUNT_CLIP_RELATIVE_OFF: usize = 0x359;

pub const WEAPON_DEF_SHOT_COUNT_OFF: usize = 0x348;
pub const WEAPON_DEF_DAMAGE_OFF: usize = 0x35c;

pub const WEAPON_DEF_FIRE_DELAY_OFF: usize = 0x378;
pub const WEAPON_DEF_MELEE_DAMAGE_OFF: usize = 0x36c;
pub const WEAPON_DEF_MELEE_DELAY_OFF: usize = 0x37c;
pub const WEAPON_DEF_MELEE_CHARGE_DELAY_OFF: usize = 0x380;
pub const WEAPON_DEF_MELEE_TIME_OFF: usize = 0x3c4;
pub const WEAPON_DEF_MELEE_CHARGE_TIME_OFF: usize = 0x3c8;
pub const WEAPON_DEF_USE_AS_MELEE_OFF: usize = 0x64c;
const _: () = assert!(WEAPON_DEF_MELEE_DELAY_OFF + 4 == WEAPON_DEF_MELEE_CHARGE_DELAY_OFF);
const _: () = assert!(WEAPON_DEF_MELEE_TIME_OFF + 4 == WEAPON_DEF_MELEE_CHARGE_TIME_OFF);
pub const WEAPON_DEF_RELOAD_ADD_TIME_OFF: usize = 0x3dc;

pub const WEAPON_DEF_RELOAD_EMPTY_ADD_TIME_OFF: usize = 0x3e0;
pub const WEAPON_DEF_RELOAD_QUICK_ADD_TIME_OFF: usize = 0x3e4;
pub const WEAPON_DEF_RELOAD_QUICK_EMPTY_ADD_TIME_OFF: usize = 0x3e8;
pub const WEAPON_DEF_RELOAD_START_TIME_OFF: usize = 0x3ec;
pub const WEAPON_DEF_RELOAD_START_ADD_TIME_OFF: usize = 0x3f0;

pub const WEAPON_DEF_RELOAD_END_TIME_OFF: usize = 0x3f4;
pub const WEAPON_DEF_ADS_OVERLAY_RETICLE_OFF: usize = 0x4b4;
pub const WEAPON_DEF_ADS_OVERLAY_INTERFACE_OFF: usize = 0x4b8;
pub const WEAPON_DEF_ADS_OVERLAY_WIDTH_OFF: usize = 0x4bc;
pub const WEAPON_DEF_ADS_OVERLAY_HEIGHT_OFF: usize = 0x4c0;

pub const WEAPON_DEF_HIP_SPREAD_STAND_MIN_OFF: usize = 0x4cc;
pub const WEAPON_DEF_NO_ADS_WHEN_MAG_EMPTY_OFF: usize = 0x47d;
pub const WEAPON_DEF_ADS_IDLE_AMOUNT_OFF: usize = 0x500;
pub const WEAPON_DEF_HIP_IDLE_AMOUNT_OFF: usize = 0x504;

pub const WEAPON_DEF_ADS_IDLE_SPEED_OFF: usize = 0x508;
pub const WEAPON_DEF_HIP_IDLE_SPEED_OFF: usize = 0x50c;
pub const WEAPON_DEF_IDLE_CROUCH_FACTOR_OFF: usize = 0x510;
pub const WEAPON_DEF_IDLE_PRONE_FACTOR_OFF: usize = 0x514;

pub const WEAPON_DEF_GUN_MAX_PITCH_OFF: usize = 0x518;
pub const WEAPON_DEF_GUN_MAX_YAW_OFF: usize = 0x51c;

pub const WEAPON_DEF_IMPACT_PAYLOAD_OFF: usize = 0x58c;
pub const WEAPON_DEF_PROJECTILE_SPEED_OFF: usize = 0x5c8;
pub const WEAPON_DEF_PROJECTILE_SPEED_UP_OFF: usize = 0x5cc;
pub const WEAPON_DEF_PROJECTILE_ACTIVATE_DIST_OFF: usize = 0x5d8;

pub const WEAPON_DEF_EXPLOSION_RADIUS_OFF: usize = 0x5b0;
pub const WEAPON_DEF_EXPLOSION_RADIUS_MIN_OFF: usize = 0x5b4;
pub const WEAPON_DEF_EXPLOSION_INNER_DAMAGE_OFF: usize = 0x5bc;
pub const WEAPON_DEF_EXPLOSION_OUTER_DAMAGE_OFF: usize = 0x5c0;

pub const WEAPON_DEF_PROJ_EXPLOSION_TYPE_OFF: usize = 0x5ec;
pub const WEAPON_DEF_PROJ_EXPLOSION_EFFECT_OFF: usize = 0x5f0;
pub const WEAPON_DEF_PROJ_EXPLOSION_SOUND_OFF: usize = 0x61c;
pub const WEAPON_DEF_PROJ_TRAIL_EFFECT_OFF: usize = 0x658;
pub const WEAPON_DEF_PROJ_IGNITION_EFFECT_OFF: usize = 0x674;
pub const WEAPON_DEF_PROJ_IGNITION_SOUND_OFF: usize = 0x678;

pub const WEAPON_DEF_PROJ_IMPACT_EXPLODE_OFF: usize = 0x62c;

pub const WEAPON_DEF_STICKINESS_OFF: usize = 0x630;
pub const WEAPON_DEF_HAS_DETONATOR_OFF: usize = 0x639;
pub const WEAPON_DEF_TIMED_DETONATION_OFF: usize = 0x63a;
pub const WEAPON_DEF_ROTATE_OFF: usize = 0x63c;

pub const WEAPON_DEF_HOLD_BUTTON_TO_THROW_OFF: usize = 0x63e;

pub const WEAPON_DEF_OFFHAND_HOLD_IS_CANCELABLE_OFF: usize = 0x63f;
const _: () = assert!(WEAPON_DEF_PROJ_IMPACT_EXPLODE_OFF < WEAPON_DEF_HOLD_BUTTON_TO_THROW_OFF);
const _: () =
    assert!(WEAPON_DEF_OFFHAND_HOLD_IS_CANCELABLE_OFF == WEAPON_DEF_HOLD_BUTTON_TO_THROW_OFF + 1);
const _: () = assert!(WEAPON_DEF_OFFHAND_HOLD_IS_CANCELABLE_OFF < WEAPON_DEF_PARALLEL_BOUNCE_OFF);

pub const WEAPON_DEF_PARALLEL_BOUNCE_OFF: usize = 0x650;
pub const WEAPON_DEF_PERPENDICULAR_BOUNCE_OFF: usize = 0x654;

pub const WEAPON_DEF_OFFHAND_CLASS_OFF: usize = 0x68;

pub const WEAPON_DEF_FUSE_TIME_OFF: usize = 0x46c;
pub const WEAPON_DEF_REQUIRE_LOCK_TO_FIRE_OFF: usize = 0x47c;
pub const WEAPON_DEF_MISSILE_GUIDANCE_OFF: usize = 0x668;
pub const WEAPON_DEF_IGNITION_DELAY_OFF: usize = 0x670;

pub const WEAPON_DEF_HOLD_FIRE_TIME_OFF: usize = 0x3bc;
const _: () = assert!(WEAPON_DEF_RECHAMBER_BOLT_TIME_OFF == WEAPON_RECHAMBER_TIME_OFF + 4);
const _: () = assert!(WEAPON_DEF_HOLD_FIRE_TIME_OFF == WEAPON_DEF_RECHAMBER_BOLT_TIME_OFF + 4);

pub const WEAPON_DEF_DUAL_WIELD_OFF: usize = 0x56a;
pub const WEAPON_DEF_DUAL_WIELD_WEAPON_NAME_OFF: usize = 0x590;

pub const WEAPON_DEF_COOK_OFF_HOLD_OFF: usize = 0x560;
pub const WEAPON_DEF_AIM_DOWN_SIGHT_OFF: usize = 0x553;
pub const WEAPON_DEF_RECHAMBER_WHILE_ADS_OFF: usize = 0x554;

pub const WEAPON_DEF_ADS_FIRE_ONLY_OFF: usize = 0x564;

pub const WEAPON_DEF_RETICLE_CENTER_OFF: usize = 0x1a0;
pub const WEAPON_DEF_RETICLE_SIDE_OFF: usize = 0x1a4;

pub const WEAPON_DEF_RETICLE_CENTER_SIZE_OFF: usize = 0x1a8;
pub const WEAPON_DEF_RETICLE_SIDE_SIZE_OFF: usize = 0x1ac;
pub const WEAPON_DEF_RETICLE_MIN_OFS_OFF: usize = 0x1b0;
const _: () = assert!(WEAPON_DEF_RETICLE_CENTER_SIZE_OFF == WEAPON_DEF_RETICLE_CENTER_OFF + 8);
const _: () = assert!(WEAPON_DEF_RETICLE_SIDE_SIZE_OFF == WEAPON_DEF_RETICLE_SIDE_OFF + 8);
const _: () = assert!(WEAPON_DEF_RETICLE_MIN_OFS_OFF == WEAPON_DEF_RETICLE_SIDE_SIZE_OFF + 4);

pub const WEAPON_DEF_HUD_ICON_OFF: usize = 0x320;
pub const WEAPON_DEF_KILL_ICON_OFF: usize = 0x578;
pub const WEAPON_DEF_KILL_ICON_RATIO_OFF: usize = 0x57c;
pub const WEAPON_DEF_FLIP_KILL_ICON_OFF: usize = 0x580;

pub const WEAPON_DEF_HIP_RETICLE_SIDE_POS_OFF: usize = 0x4fc;
const _: () =
    assert!(WEAPON_DEF_HIP_RETICLE_SIDE_POS_OFF == WEAPON_DEF_HIP_SPREAD_STAND_MIN_OFF + 48);
const _: () = assert!(WEAPON_DEF_HIP_RETICLE_SIDE_POS_OFF + 4 == WEAPON_DEF_ADS_IDLE_AMOUNT_OFF);
pub const WEAPON_DEF_NO_PARTIAL_RELOAD_OFF: usize = 0x581;
pub const WEAPON_DEF_SEGMENTED_RELOAD_OFF: usize = 0x582;
pub const WEAPON_DEF_RELOAD_AMMO_ADD_OFF: usize = 0x584;
pub const WEAPON_DEF_RELOAD_START_ADD_OFF: usize = 0x588;
pub const WEAPON_DEF_MIN_DAMAGE_OFF: usize = 0x7a0;
pub const WEAPON_DEF_MAX_DAMAGE_RANGE_OFF: usize = 0x7a8;
pub const WEAPON_DEF_MIN_DAMAGE_RANGE_OFF: usize = 0x7ac;
pub const WEAPON_DEF_ADS_GUN_KICK_REDUCED_BULLETS_OFF: usize = 0x688;
pub const WEAPON_DEF_ADS_GUN_KICK_REDUCED_PERCENT_OFF: usize = 0x68c;
pub const WEAPON_DEF_ADS_GUN_KICK_PITCH_MIN_OFF: usize = 0x690;
pub const WEAPON_DEF_ADS_GUN_KICK_PITCH_MAX_OFF: usize = 0x694;
pub const WEAPON_DEF_ADS_GUN_KICK_YAW_MIN_OFF: usize = 0x698;
pub const WEAPON_DEF_ADS_GUN_KICK_YAW_MAX_OFF: usize = 0x69c;
pub const WEAPON_DEF_ADS_GUN_KICK_ACCEL_OFF: usize = 0x6a0;
pub const WEAPON_DEF_ADS_GUN_KICK_SPEED_MAX_OFF: usize = 0x6a4;
pub const WEAPON_DEF_ADS_GUN_KICK_SPEED_DECAY_OFF: usize = 0x6a8;
pub const WEAPON_DEF_ADS_GUN_KICK_STATIC_DECAY_OFF: usize = 0x6ac;
pub const WEAPON_DEF_ADS_VIEW_KICK_PITCH_MIN_OFF: usize = 0x6b0;
pub const WEAPON_DEF_ADS_VIEW_KICK_PITCH_MAX_OFF: usize = 0x6b4;
pub const WEAPON_DEF_ADS_VIEW_KICK_YAW_MIN_OFF: usize = 0x6b8;
pub const WEAPON_DEF_ADS_VIEW_KICK_YAW_MAX_OFF: usize = 0x6bc;
pub const WEAPON_DEF_HIP_GUN_KICK_REDUCED_BULLETS_OFF: usize = 0x6cc;
pub const WEAPON_DEF_HIP_GUN_KICK_REDUCED_PERCENT_OFF: usize = 0x6d0;
pub const WEAPON_DEF_HIP_GUN_KICK_PITCH_MIN_OFF: usize = 0x6d4;
pub const WEAPON_DEF_HIP_GUN_KICK_PITCH_MAX_OFF: usize = 0x6d8;
pub const WEAPON_DEF_HIP_GUN_KICK_YAW_MIN_OFF: usize = 0x6dc;
pub const WEAPON_DEF_HIP_GUN_KICK_YAW_MAX_OFF: usize = 0x6e0;
pub const WEAPON_DEF_HIP_GUN_KICK_ACCEL_OFF: usize = 0x6e4;
pub const WEAPON_DEF_HIP_GUN_KICK_SPEED_MAX_OFF: usize = 0x6e8;
pub const WEAPON_DEF_HIP_GUN_KICK_SPEED_DECAY_OFF: usize = 0x6ec;
pub const WEAPON_DEF_HIP_GUN_KICK_STATIC_DECAY_OFF: usize = 0x6f0;
pub const WEAPON_DEF_HIP_VIEW_KICK_PITCH_MIN_OFF: usize = 0x6f4;
pub const WEAPON_DEF_HIP_VIEW_KICK_PITCH_MAX_OFF: usize = 0x6f8;
pub const WEAPON_DEF_HIP_VIEW_KICK_YAW_MIN_OFF: usize = 0x6fc;
pub const WEAPON_DEF_HIP_VIEW_KICK_YAW_MAX_OFF: usize = 0x700;
pub const FLAME_TABLE: usize = 476;
pub const WEAPON_XANIM_COUNT: usize = 66;

pub mod weap_anim {
    pub const ROOT: usize = 0x0;
    pub const IDLE: usize = 0x1;
    pub const EMPTY_IDLE: usize = 0x2;
    pub const FIRE: usize = 0x3;
    pub const HOLD_FIRE: usize = 0x4;
    pub const LASTSHOT: usize = 0x5;
    pub const RECHAMBER: usize = 0x6;
    pub const MELEE: usize = 0x7;
    pub const MELEE_CHARGE: usize = 0x8;
    pub const RELOAD: usize = 0x9;
    pub const RELOAD_RIGHT: usize = 0xA;
    pub const RELOAD_EMPTY: usize = 0xB;
    pub const RELOAD_START: usize = 0xC;
    pub const RELOAD_END: usize = 0xD;
    pub const RELOAD_QUICK: usize = 0xE;
    pub const RELOAD_QUICK_EMPTY: usize = 0xF;
    pub const RAISE: usize = 0x10;
    pub const FIRST_RAISE: usize = 0x11;
    pub const DROP: usize = 0x12;
    pub const ALT_RAISE: usize = 0x13;
    pub const ALT_DROP: usize = 0x14;
    pub const QUICK_RAISE: usize = 0x15;
    pub const QUICK_DROP: usize = 0x16;
    pub const EMPTY_RAISE: usize = 0x17;
    pub const EMPTY_DROP: usize = 0x18;
    pub const SPRINT_IN: usize = 0x19;
    pub const SPRINT_LOOP: usize = 0x1A;
    pub const SPRINT_OUT: usize = 0x1B;
    pub const DETONATE: usize = 0x27;
    pub const NIGHTVISION_WEAR: usize = 0x28;
    pub const NIGHTVISION_REMOVE: usize = 0x29;
    pub const ADS_FIRE: usize = 0x2A;
    pub const ADS_LASTSHOT: usize = 0x2B;
    pub const ADS_RECHAMBER: usize = 0x2C;
    pub const FIRE_LEFT: usize = 0x3A;
    pub const LASTSHOT_LEFT: usize = 0x3B;
    pub const IDLE_LEFT: usize = 0x3C;
    pub const EMPTY_IDLE_LEFT: usize = 0x3D;
    pub const RELOAD_EMPTY_LEFT: usize = 0x3E;
    pub const RELOAD_LEFT: usize = 0x3F;
    pub const ADS_UP: usize = 0x40;
    pub const ADS_DOWN: usize = 0x41;
}

pub const WEAPON_VARIANT_HIDE_TAGS_OFF: usize = 0x18;
pub const WEAPON_HIDE_TAG_COUNT: usize = 32;
pub const WEAPON_GUN_MODEL_COUNT: usize = 16;
pub const WEAPON_BOUNCE_SOUND_COUNT: usize = 31;

pub const WEAPON_BOUNCE_COEFFICIENT_COUNT: usize = SURF_TYPE_NUM;
pub const WEAPON_LOCATION_DAMAGE_COUNT: usize = 19;
pub const WEAPON_NOTETRACK_COUNT: usize = 20;

pub const MENU_LIST: usize = 12;
pub const MENU_DEF: usize = 400;
pub const WINDOW_DEF: usize = 164;
pub const ITEM_DEF: usize = 272;
pub const EXPRESSION_STATEMENT: usize = 16;
pub const EXPRESSION_RPN: usize = 12;
pub const GENERIC_EVENT_HANDLER: usize = 12;
pub const GENERIC_EVENT_SCRIPT: usize = 44;
pub const ITEM_KEY_HANDLER: usize = 12;
pub const SCRIPT_CONDITION: usize = 16;
pub const UI_ANIM_INFO: usize = 236;
pub const ANIM_PARAMS_DEF: usize = 108;
pub const RECT_DATA: usize = 64;
pub const TEXT_DEF: usize = 68;
pub const IMAGE_DEF: usize = 16;
pub const OWNER_DRAW_DEF: usize = 16;
pub const FOCUS_ITEM_DEF: usize = 24;
pub const LIST_BOX_DEF: usize = 668;
pub const MULTI_DEF: usize = 396;
pub const EDIT_FIELD_DEF: usize = 36;
pub const ENUM_DVAR_DEF: usize = 4;
pub const GAME_MSG_DEF: usize = 8;
pub const TEXT_EXP: usize = 16;
pub const MENU_CELL: usize = 12;
pub const MENU_ROW: usize = 24;

pub fn occupancy_bit(slot: usize) -> Option<(usize, u64)> {
    if slot >= TECHNIQUE_SLOT_COUNT {
        return None;
    }
    Some((slot / 64, 1u64 << (slot % 64)))
}

pub fn occupancy_set(words: &mut [u64; TECHNIQUE_OCCUPANCY_WORDS], slot: usize) {
    if let Some((word, bit)) = occupancy_bit(slot) {
        words[word] |= bit;
    }
}

pub fn occupancy_test(words: &[u64; TECHNIQUE_OCCUPANCY_WORDS], slot: usize) -> bool {
    occupancy_bit(slot).is_some_and(|(word, bit)| words[word] & bit != 0)
}

pub fn occupancy_any(words: &[u64; TECHNIQUE_OCCUPANCY_WORDS]) -> bool {
    words.iter().copied().any(|word| word != 0)
}
