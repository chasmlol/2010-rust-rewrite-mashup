const IW4_CODE_CONST: &[(&str, u16)] = &[
    ("LIGHT_POSITION", 0x0),
    ("LIGHT_DIFFUSE", 0x1),
    ("LIGHT_SPECULAR", 0x2),
    ("LIGHT_SPOTDIR", 0x3),
    ("LIGHT_SPOTFACTORS", 0x4),
    ("LIGHT_FALLOFF_PLACEMENT", 0x5),
    ("PARTICLE_CLOUD_COLOR", 0x6),
    ("GAMETIME", 0x7),
    ("PIXEL_COST_FRACS", 0x8),
    ("PIXEL_COST_DECODE", 0x9),
    ("FILTER_TAP_0", 0xa),
    ("FILTER_TAP_1", 0xb),
    ("FILTER_TAP_2", 0xc),
    ("FILTER_TAP_3", 0xd),
    ("FILTER_TAP_4", 0xe),
    ("FILTER_TAP_5", 0xf),
    ("FILTER_TAP_6", 0x10),
    ("FILTER_TAP_7", 0x11),
    ("COLOR_MATRIX_R", 0x12),
    ("COLOR_MATRIX_G", 0x13),
    ("COLOR_MATRIX_B", 0x14),
    ("SHADOWMAP_POLYGON_OFFSET", 0x15),
    ("RENDER_TARGET_SIZE", 0x16),
    ("DOF_EQUATION_VIEWMODEL_AND_FAR_BLUR", 0x17),
    ("DOF_EQUATION_SCENE", 0x18),
    ("DOF_LERP_SCALE", 0x19),
    ("DOF_LERP_BIAS", 0x1a),
    ("DOF_ROW_DELTA", 0x1b),
    ("MOTION_MATRIX_X", 0x1c),
    ("MOTION_MATRIX_Y", 0x1d),
    ("MOTION_MATRIX_W", 0x1e),
    ("SHADOWMAP_SWITCH_PARTITION", 0x1f),
    ("SHADOWMAP_SCALE", 0x20),
    ("ZNEAR", 0x21),
    ("LIGHTING_LOOKUP_SCALE", 0x22),
    ("DEBUG_BUMPMAP", 0x23),
    ("MATERIAL_COLOR", 0x24),
    ("FOG", 0x25),
    ("FOG_COLOR_LINEAR", 0x26),
    ("FOG_COLOR_GAMMA", 0x27),
    ("FOG_SUN_CONSTS", 0x28),
    ("FOG_SUN_COLOR_LINEAR", 0x29),
    ("FOG_SUN_COLOR_GAMMA", 0x2a),
    ("FOG_SUN_DIR", 0x2b),
    ("GLOW_SETUP", 0x2c),
    ("GLOW_APPLY", 0x2d),
    ("COLOR_BIAS", 0x2e),
    ("COLOR_TINT_BASE", 0x2f),
    ("COLOR_TINT_DELTA", 0x30),
    ("COLOR_TINT_QUADRATIC_DELTA", 0x31),
    ("OUTDOOR_FEATHER_PARMS", 0x32),
    ("ENVMAP_PARMS", 0x33),
    ("SUN_SHADOWMAP_PIXEL_ADJUST", 0x34),
    ("SPOT_SHADOWMAP_PIXEL_ADJUST", 0x35),
    ("COMPOSITE_FX_DISTORTION", 0x36),
    ("POSTFX_FADE_EFFECT", 0x37),
    ("VIEWPORT_DIMENSIONS", 0x38),
    ("FRAMEBUFFER_READ", 0x39),
    ("BASE_LIGHTING_COORDS", 0x3a),
    ("LIGHT_PROBE_AMBIENT", 0x3b),
    ("NEARPLANE_ORG", 0x3c),
    ("NEARPLANE_DX", 0x3d),
    ("NEARPLANE_DY", 0x3e),
    ("CLIP_SPACE_LOOKUP_SCALE", 0x3f),
    ("CLIP_SPACE_LOOKUP_OFFSET", 0x40),
    ("PARTICLE_CLOUD_MATRIX0", 0x41),
    ("PARTICLE_CLOUD_MATRIX1", 0x42),
    ("PARTICLE_CLOUD_MATRIX2", 0x43),
    ("PARTICLE_CLOUD_SPARK_COLOR0", 0x44),
    ("PARTICLE_CLOUD_SPARK_COLOR1", 0x45),
    ("PARTICLE_CLOUD_SPARK_COLOR2", 0x46),
    ("PARTICLE_FOUNTAIN_PARM0", 0x47),
    ("PARTICLE_FOUNTAIN_PARM1", 0x48),
    ("DEPTH_FROM_CLIP", 0x49),
    ("CODE_MESH_ARG_0", 0x4a),
    ("CODE_MESH_ARG_1", 0x4b),
    ("VIEW_MATRIX", 0x4c),
    ("INVERSE_VIEW_MATRIX", 0x4d),
    ("TRANSPOSE_VIEW_MATRIX", 0x4e),
    ("INVERSE_TRANSPOSE_VIEW_MATRIX", 0x4f),
    ("PROJECTION_MATRIX", 0x50),
    ("INVERSE_PROJECTION_MATRIX", 0x51),
    ("TRANSPOSE_PROJECTION_MATRIX", 0x52),
    ("INVERSE_TRANSPOSE_PROJECTION_MATRIX", 0x53),
    ("VIEW_PROJECTION_MATRIX", 0x54),
    ("INVERSE_VIEW_PROJECTION_MATRIX", 0x55),
    ("TRANSPOSE_VIEW_PROJECTION_MATRIX", 0x56),
    ("INVERSE_TRANSPOSE_VIEW_PROJECTION_MATRIX", 0x57),
    ("SHADOW_LOOKUP_MATRIX", 0x58),
    ("INVERSE_SHADOW_LOOKUP_MATRIX", 0x59),
    ("TRANSPOSE_SHADOW_LOOKUP_MATRIX", 0x5a),
    ("INVERSE_TRANSPOSE_SHADOW_LOOKUP_MATRIX", 0x5b),
    ("WORLD_OUTDOOR_LOOKUP_MATRIX", 0x5c),
    ("INVERSE_WORLD_OUTDOOR_LOOKUP_MATRIX", 0x5d),
    ("TRANSPOSE_WORLD_OUTDOOR_LOOKUP_MATRIX", 0x5e),
    ("INVERSE_TRANSPOSE_WORLD_OUTDOOR_LOOKUP_MATRIX", 0x5f),
    ("WORLD_MATRIX0", 0x60),
    ("INVERSE_WORLD_MATRIX0", 0x61),
    ("TRANSPOSE_WORLD_MATRIX0", 0x62),
    ("INVERSE_TRANSPOSE_WORLD_MATRIX0", 0x63),
    ("WORLD_VIEW_MATRIX0", 0x64),
    ("INVERSE_WORLD_VIEW_MATRIX0", 0x65),
    ("TRANSPOSE_WORLD_VIEW_MATRIX0", 0x66),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_MATRIX0", 0x67),
    ("WORLD_VIEW_PROJECTION_MATRIX0", 0x68),
    ("INVERSE_WORLD_VIEW_PROJECTION_MATRIX0", 0x69),
    ("TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX0", 0x6a),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX0", 0x6b),
    ("WORLD_MATRIX1", 0x6c),
    ("INVERSE_WORLD_MATRIX1", 0x6d),
    ("TRANSPOSE_WORLD_MATRIX1", 0x6e),
    ("INVERSE_TRANSPOSE_WORLD_MATRIX1", 0x6f),
    ("WORLD_VIEW_MATRIX1", 0x70),
    ("INVERSE_WORLD_VIEW_MATRIX1", 0x71),
    ("TRANSPOSE_WORLD_VIEW_MATRIX1", 0x72),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_MATRIX1", 0x73),
    ("WORLD_VIEW_PROJECTION_MATRIX1", 0x74),
    ("INVERSE_WORLD_VIEW_PROJECTION_MATRIX1", 0x75),
    ("TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX1", 0x76),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX1", 0x77),
    ("WORLD_MATRIX2", 0x78),
    ("INVERSE_WORLD_MATRIX2", 0x79),
    ("TRANSPOSE_WORLD_MATRIX2", 0x7a),
    ("INVERSE_TRANSPOSE_WORLD_MATRIX2", 0x7b),
    ("WORLD_VIEW_MATRIX2", 0x7c),
    ("INVERSE_WORLD_VIEW_MATRIX2", 0x7d),
    ("TRANSPOSE_WORLD_VIEW_MATRIX2", 0x7e),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_MATRIX2", 0x7f),
    ("WORLD_VIEW_PROJECTION_MATRIX2", 0x80),
    ("INVERSE_WORLD_VIEW_PROJECTION_MATRIX2", 0x81),
    ("TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX2", 0x82),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX2", 0x83),
];

const IW4_CODE_TEXTURE: &[(&str, u16)] = &[
    ("BLACK", 0x0),
    ("WHITE", 0x1),
    ("IDENTITY_NORMAL_MAP", 0x2),
    ("MODEL_LIGHTING", 0x3),
    ("LIGHTMAP_PRIMARY", 0x4),
    ("LIGHTMAP_SECONDARY", 0x5),
    ("SHADOWMAP_SUN", 0x6),
    ("SHADOWMAP_SPOT", 0x7),
    ("FEEDBACK", 0x8),
    ("RESOLVED_POST_SUN", 0x9),
    ("RESOLVED_SCENE", 0xa),
    ("POST_EFFECT_0", 0xb),
    ("POST_EFFECT_1", 0xc),
    ("LIGHT_ATTENUATION", 0xd),
    ("OUTDOOR", 0xe),
    ("FLOATZ", 0xf),
    ("PROCESSED_FLOATZ", 0x10),
    ("RAW_FLOATZ", 0x11),
    ("HALF_PARTICLES", 0x12),
    ("HALF_PARTICLES_Z", 0x13),
    ("CASE_TEXTURE", 0x14),
    ("CINEMATIC_Y", 0x15),
    ("CINEMATIC_CR", 0x16),
    ("CINEMATIC_CB", 0x17),
    ("CINEMATIC_A", 0x18),
    ("REFLECTION_PROBE", 0x19),
    ("ALTERNATE_SCENE", 0x1a),
];

const T5_CODE_CONST: &[(&str, u16)] = &[
    ("LIGHT_POSITION", 0x0),
    ("LIGHT_DIFFUSE", 0x1),
    ("LIGHT_SPECULAR", 0x2),
    ("LIGHT_SPOTDIR", 0x3),
    ("LIGHT_SPOTFACTORS", 0x4),
    ("LIGHT_ATTENUATION", 0x5),
    ("LIGHT_FALLOFF_A", 0x6),
    ("LIGHT_FALLOFF_B", 0x7),
    ("LIGHT_SPOT_MATRIX0", 0x8),
    ("LIGHT_SPOT_MATRIX1", 0x9),
    ("LIGHT_SPOT_MATRIX2", 0xa),
    ("LIGHT_SPOT_MATRIX3", 0xb),
    ("LIGHT_SPOT_AABB", 0xc),
    ("LIGHT_CONE_CONTROL1", 0xd),
    ("LIGHT_CONE_CONTROL2", 0xe),
    ("LIGHT_SPOT_COOKIE_SLIDE_CONTROL", 0xf),
    ("NEARPLANE_ORG", 0x10),
    ("NEARPLANE_DX", 0x11),
    ("NEARPLANE_DY", 0x12),
    ("SHADOW_PARMS", 0x13),
    ("SHADOWMAP_POLYGON_OFFSET", 0x14),
    ("RENDER_TARGET_SIZE", 0x15),
    ("VPOSX_TO_WORLD", 0x16),
    ("VPOSY_TO_WORLD", 0x17),
    ("VPOS1_TO_WORLD", 0x18),
    ("LIGHT_FALLOFF_PLACEMENT", 0x19),
    ("DOF_EQUATION_VIEWMODEL_AND_FAR_BLUR", 0x1a),
    ("DOF_EQUATION_SCENE", 0x1b),
    ("DOF_LERP_SCALE", 0x1c),
    ("DOF_LERP_BIAS", 0x1d),
    ("DOF_ROW_DELTA", 0x1e),
    ("PARTICLE_CLOUD_COLOR", 0x1f),
    ("GAMETIME", 0x20),
    ("ALPHA_FADE", 0x21),
    ("PIXEL_COST_FRACS", 0x22),
    ("PIXEL_COST_DECODE", 0x23),
    ("FILTER_TAP_0", 0x24),
    ("FILTER_TAP_1", 0x25),
    ("FILTER_TAP_2", 0x26),
    ("FILTER_TAP_3", 0x27),
    ("FILTER_TAP_4", 0x28),
    ("FILTER_TAP_5", 0x29),
    ("FILTER_TAP_6", 0x2a),
    ("FILTER_TAP_7", 0x2b),
    ("COLOR_MATRIX_R", 0x2c),
    ("COLOR_MATRIX_G", 0x2d),
    ("COLOR_MATRIX_B", 0x2e),
    ("SHADOWMAP_SWITCH_PARTITION", 0x2f),
    ("SHADOWMAP_SCALE", 0x30),
    ("ZNEAR", 0x31),
    ("SUN_POSITION", 0x32),
    ("SUN_DIFFUSE", 0x33),
    ("SUN_SPECULAR", 0x34),
    ("LIGHTING_LOOKUP_SCALE", 0x35),
    ("DEBUG_BUMPMAP", 0x36),
    ("MATERIAL_COLOR", 0x37),
    ("FOG", 0x38),
    ("FOG2", 0x39),
    ("FOG_COLOR", 0x3a),
    ("SUN_FOG", 0x3b),
    ("SUN_FOG_DIR", 0x3c),
    ("SUN_FOG_COLOR", 0x3d),
    ("GLOW_SETUP", 0x3e),
    ("GLOW_APPLY", 0x3f),
    ("COLOR_BIAS", 0x40),
    ("COLOR_TINT_BASE", 0x41),
    ("COLOR_TINT_DELTA", 0x42),
    ("OUTDOOR_FEATHER_PARMS", 0x43),
    ("SKY_TRANSITION", 0x44),
    ("ENVMAP_PARMS", 0x45),
    ("SPOT_SHADOWMAP_PIXEL_ADJUST", 0x46),
    ("DLIGHT_SPOT_SHADOWMAP_PIXEL_ADJUST", 0x47),
    ("CLIP_SPACE_LOOKUP_SCALE", 0x48),
    ("CLIP_SPACE_LOOKUP_OFFSET", 0x49),
    ("PARTICLE_CLOUD_MATRIX", 0x4a),
    ("DEPTH_FROM_CLIP", 0x4b),
    ("CODE_MESH_ARG_0", 0x4c),
    ("CODE_MESH_ARG_1", 0x4d),
    ("BASE_LIGHTING_COORDS", 0x4e),
    ("WIND_DIRECTION", 0x4f),
    ("WATER_PARMS", 0x50),
    ("GRASS_PARMS", 0x51),
    ("GRASS_FORCE0", 0x52),
    ("GRASS_FORCE1", 0x53),
    ("GRASS_WIND_FORCE0", 0x54),
    ("MOTIONBLUR_DIRECTION_AND_MAGNITUDE", 0x55),
    ("COMPOSITE_FX_DISTORTION", 0x56),
    ("GLOW_BLOOM_SCALE", 0x57),
    ("COMPOSITE_FX_OVERLAY_TEXCOORD", 0x58),
    ("COLOR_BIAS1", 0x59),
    ("COLOR_TINT_BASE1", 0x5a),
    ("COLOR_TINT_DELTA1", 0x5b),
    ("POSTFX_FADE_EFFECT", 0x5c),
    ("VIEWPORT_DIMENSIONS", 0x5d),
    ("FRAMEBUFFER_READ", 0x5e),
    ("RESIZE_PARAMS1", 0x5f),
    ("RESIZE_PARAMS2", 0x60),
    ("RESIZE_PARAMS3", 0x61),
    ("VARIANT_WIND_SPRING_0", 0x62),
    ("VARIANT_WIND_SPRING_1", 0x63),
    ("VARIANT_WIND_SPRING_2", 0x64),
    ("VARIANT_WIND_SPRING_3", 0x65),
    ("VARIANT_WIND_SPRING_4", 0x66),
    ("VARIANT_WIND_SPRING_5", 0x67),
    ("VARIANT_WIND_SPRING_6", 0x68),
    ("VARIANT_WIND_SPRING_7", 0x69),
    ("VARIANT_WIND_SPRING_8", 0x6a),
    ("VARIANT_WIND_SPRING_9", 0x6b),
    ("VARIANT_WIND_SPRING_10", 0x6c),
    ("VARIANT_WIND_SPRING_11", 0x6d),
    ("VARIANT_WIND_SPRING_12", 0x6e),
    ("VARIANT_WIND_SPRING_13", 0x6f),
    ("VARIANT_WIND_SPRING_14", 0x70),
    ("VARIANT_WIND_SPRING_15", 0x71),
    ("DESTRUCTIBLE_PARMS", 0x72),
    ("CLOUD_WORLD_AREA", 0x73),
    ("WATER_SCROLL", 0x74),
    ("CROSSFADE_PARMS", 0x75),
    ("CHARACTER_CHARRED_AMOUNT", 0x76),
    ("TREECANOPY_PARMS", 0x77),
    ("MARKS_HIT_NORMAL", 0x78),
    ("POSTFX_CONTROL0", 0x79),
    ("POSTFX_CONTROL1", 0x7a),
    ("POSTFX_CONTROL2", 0x7b),
    ("POSTFX_CONTROL3", 0x7c),
    ("POSTFX_CONTROL4", 0x7d),
    ("POSTFX_CONTROL5", 0x7e),
    ("POSTFX_CONTROL6", 0x7f),
    ("POSTFX_CONTROL7", 0x80),
    ("POSTFX_CONTROL8", 0x81),
    ("POSTFX_CONTROL9", 0x82),
    ("POSTFX_CONTROLA", 0x83),
    ("POSTFX_CONTROLB", 0x84),
    ("POSTFX_CONTROLC", 0x85),
    ("POSTFX_CONTROLD", 0x86),
    ("POSTFX_CONTROLE", 0x87),
    ("POSTFX_CONTROLF", 0x88),
    ("HDRCONTROL_0", 0x89),
    ("HDRCONTROL_1", 0x8a),
    ("GLIGHT_POSXS", 0x8b),
    ("GLIGHT_POSYS", 0x8c),
    ("GLIGHT_POSZS", 0x8d),
    ("GLIGHT_FALLOFFS", 0x8e),
    ("GLIGHT_REDS", 0x8f),
    ("GLIGHT_GREENS", 0x90),
    ("GLIGHT_BLUES", 0x91),
    ("DLIGHT_POSITION", 0x92),
    ("DLIGHT_DIFFUSE", 0x93),
    ("DLIGHT_SPECULAR", 0x94),
    ("DLIGHT_ATTENUATION", 0x95),
    ("DLIGHT_FALLOFF", 0x96),
    ("DLIGHT_SPOT_MATRIX_0", 0x97),
    ("DLIGHT_SPOT_MATRIX_1", 0x98),
    ("DLIGHT_SPOT_MATRIX_2", 0x99),
    ("DLIGHT_SPOT_MATRIX_3", 0x9a),
    ("DLIGHT_SPOT_DIR", 0x9b),
    ("DLIGHT_SPOT_FACTORS", 0x9c),
    ("DLIGHT_SHADOW_LOOKUP_MATRIX_0", 0x9d),
    ("DLIGHT_SHADOW_LOOKUP_MATRIX_1", 0x9e),
    ("DLIGHT_SHADOW_LOOKUP_MATRIX_2", 0x9f),
    ("DLIGHT_SHADOW_LOOKUP_MATRIX_3", 0xa0),
    ("CLOUD_LAYER_CONTROL0", 0xa1),
    ("CLOUD_LAYER_CONTROL1", 0xa2),
    ("CLOUD_LAYER_CONTROL2", 0xa3),
    ("CLOUD_LAYER_CONTROL3", 0xa4),
    ("CLOUD_LAYER_CONTROL4", 0xa5),
    ("HERO_LIGHTING_R", 0xa6),
    ("HERO_LIGHTING_G", 0xa7),
    ("HERO_LIGHTING_B", 0xa8),
    ("LIGHT_HERO_SCALE", 0xa9),
    ("CINEMATIC_BLUR_BOX", 0xaa),
    ("CINEMATIC_BLUR_BOX2", 0xab),
    ("ADSZSCALE", 0xac),
    ("UI3D_UV_SETUP_0", 0xad),
    ("UI3D_UV_SETUP_1", 0xae),
    ("UI3D_UV_SETUP_2", 0xaf),
    ("UI3D_UV_SETUP_3", 0xb0),
    ("UI3D_UV_SETUP_4", 0xb1),
    ("UI3D_UV_SETUP_5", 0xb2),
    ("CHARACTER_DISSOLVE_COLOR", 0xb3),
    ("CAMERA_LOOK", 0xb4),
    ("CAMERA_UP", 0xb5),
    ("CAMERA_SIDE", 0xb6),
    ("GENERIC_PARAM0", 0xb7),
    ("GENERIC_PARAM1", 0xb8),
    ("GENERIC_PARAM2", 0xb9),
    ("GENERIC_PARAM3", 0xba),
    ("GENERIC_PARAM4", 0xbb),
    ("GENERIC_PARAM5", 0xbc),
    ("GENERIC_PARAM6", 0xbd),
    ("GENERIC_PARAM7", 0xbe),
    ("EYEOFFSET", 0xbf),
    ("CUSTOMWIND_CENTER", 0xc0),
    ("CUSTOMWIND_SPRING", 0xc1),
    ("SKY_COLOR_MULTIPLIER", 0xc2),
    ("EXTRA_CAM_PARAM", 0xc3),
    ("EMBLEM_LUT_SELECTOR", 0xc4),
    ("WORLD_MATRIX", 0xc5),
    ("INVERSE_WORLD_MATRIX", 0xc6),
    ("TRANSPOSE_WORLD_MATRIX", 0xc7),
    ("INVERSE_TRANSPOSE_WORLD_MATRIX", 0xc8),
    ("VIEW_MATRIX", 0xc9),
    ("INVERSE_VIEW_MATRIX", 0xca),
    ("TRANSPOSE_VIEW_MATRIX", 0xcb),
    ("INVERSE_TRANSPOSE_VIEW_MATRIX", 0xcc),
    ("PROJECTION_MATRIX", 0xcd),
    ("INVERSE_PROJECTION_MATRIX", 0xce),
    ("TRANSPOSE_PROJECTION_MATRIX", 0xcf),
    ("INVERSE_TRANSPOSE_PROJECTION_MATRIX", 0xd0),
    ("WORLD_VIEW_MATRIX", 0xd1),
    ("INVERSE_WORLD_VIEW_MATRIX", 0xd2),
    ("TRANSPOSE_WORLD_VIEW_MATRIX", 0xd3),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_MATRIX", 0xd4),
    ("VIEW_PROJECTION_MATRIX", 0xd5),
    ("INVERSE_VIEW_PROJECTION_MATRIX", 0xd6),
    ("TRANSPOSE_VIEW_PROJECTION_MATRIX", 0xd7),
    ("INVERSE_TRANSPOSE_VIEW_PROJECTION_MATRIX", 0xd8),
    ("WORLD_VIEW_PROJECTION_MATRIX", 0xd9),
    ("INVERSE_WORLD_VIEW_PROJECTION_MATRIX", 0xda),
    ("TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX", 0xdb),
    ("INVERSE_TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX", 0xdc),
    ("SHADOW_LOOKUP_MATRIX", 0xdd),
    ("INVERSE_SHADOW_LOOKUP_MATRIX", 0xde),
    ("TRANSPOSE_SHADOW_LOOKUP_MATRIX", 0xdf),
    ("INVERSE_TRANSPOSE_SHADOW_LOOKUP_MATRIX", 0xe0),
    ("WORLD_OUTDOOR_LOOKUP_MATRIX", 0xe1),
    ("INVERSE_WORLD_OUTDOOR_LOOKUP_MATRIX", 0xe2),
    ("TRANSPOSE_WORLD_OUTDOOR_LOOKUP_MATRIX", 0xe3),
    ("INVERSE_TRANSPOSE_WORLD_OUTDOOR_LOOKUP_MATRIX", 0xe4),
];

const T5_CODE_TEXTURE: &[(&str, u16)] = &[
    ("BLACK", 0x0),
    ("WHITE", 0x1),
    ("IDENTITY_NORMAL_MAP", 0x2),
    ("MODEL_LIGHTING", 0x3),
    ("LIGHTMAP_PRIMARY", 0x4),
    ("LIGHTMAP_SECONDARY", 0x5),
    ("SHADOWMAP_SUN", 0x6),
    ("SHADOWMAP_SPOT", 0x7),
    ("FEEDBACK", 0x8),
    ("RESOLVED_POST_SUN", 0x9),
    ("RESOLVED_SCENE", 0xa),
    ("POST_EFFECT_SRC", 0xb),
    ("POST_EFFECT_GODRAYS", 0xc),
    ("POST_EFFECT_0", 0xd),
    ("POST_EFFECT_1", 0xe),
    ("SKY", 0xf),
    ("LIGHT_ATTENUATION", 0x10),
    ("DLIGHT_ATTENUATION", 0x11),
    ("OUTDOOR", 0x12),
    ("FLOATZ", 0x13),
    ("PROCESSED_FLOATZ", 0x14),
    ("RAW_FLOATZ", 0x15),
    ("CASE_TEXTURE", 0x16),
    ("CINEMATIC_Y", 0x17),
    ("CINEMATIC_CR", 0x18),
    ("CINEMATIC_CB", 0x19),
    ("CINEMATIC_A", 0x1a),
    ("REFLECTION_PROBE", 0x1b),
    ("FEATHER_FLOAT_Z", 0x1c),
    ("TERRAIN_SCORCH_TEXTURE_0", 0x1d),
    ("TERRAIN_SCORCH_TEXTURE_1", 0x1e),
    ("TERRAIN_SCORCH_TEXTURE_2", 0x1f),
    ("TERRAIN_SCORCH_TEXTURE_3", 0x20),
    ("LIGHTMAP_SECONDARYB", 0x21),
    ("TEXTURE_0", 0x22),
    ("TEXTURE_1", 0x23),
    ("TEXTURE_2", 0x24),
    ("TEXTURE_3", 0x25),
    ("IMPACT_MASK", 0x26),
    ("UI3D", 0x27),
    ("MISSILE_CAM", 0x28),
    ("COMPOSITE_RESULT", 0x29),
    ("HEATMAP", 0x2a),
];

const T5_CONST_ALIASES: &[(&str, &str)] = &[
    ("WORLD_MATRIX", "WORLD_MATRIX0"),
    ("INVERSE_WORLD_MATRIX", "INVERSE_WORLD_MATRIX0"),
    ("TRANSPOSE_WORLD_MATRIX", "TRANSPOSE_WORLD_MATRIX0"),
    (
        "INVERSE_TRANSPOSE_WORLD_MATRIX",
        "INVERSE_TRANSPOSE_WORLD_MATRIX0",
    ),
    ("WORLD_VIEW_MATRIX", "WORLD_VIEW_MATRIX0"),
    ("INVERSE_WORLD_VIEW_MATRIX", "INVERSE_WORLD_VIEW_MATRIX0"),
    (
        "TRANSPOSE_WORLD_VIEW_MATRIX",
        "TRANSPOSE_WORLD_VIEW_MATRIX0",
    ),
    (
        "INVERSE_TRANSPOSE_WORLD_VIEW_MATRIX",
        "INVERSE_TRANSPOSE_WORLD_VIEW_MATRIX0",
    ),
    (
        "WORLD_VIEW_PROJECTION_MATRIX",
        "WORLD_VIEW_PROJECTION_MATRIX0",
    ),
    (
        "INVERSE_WORLD_VIEW_PROJECTION_MATRIX",
        "INVERSE_WORLD_VIEW_PROJECTION_MATRIX0",
    ),
    (
        "TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX",
        "TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX0",
    ),
    (
        "INVERSE_TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX",
        "INVERSE_TRANSPOSE_WORLD_VIEW_PROJECTION_MATRIX0",
    ),
    ("PARTICLE_CLOUD_MATRIX", "PARTICLE_CLOUD_MATRIX0"),
];

pub const T5_CODE_CONST_LIMIT: usize = 0xe5;
pub const T5_CODE_TEXTURE_LIMIT: usize = 0x2b;

const fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}
const fn name_eq(a: &str, b: &str) -> bool {
    bytes_eq(a.as_bytes(), b.as_bytes())
}

const fn alias_of(name: &str) -> &str {
    let mut i = 0;
    while i < T5_CONST_ALIASES.len() {
        if name_eq(name, T5_CONST_ALIASES[i].0) {
            return T5_CONST_ALIASES[i].1;
        }
        i += 1;
    }
    name
}

const fn build_const_remap() -> [Option<u16>; T5_CODE_CONST_LIMIT] {
    let mut out = [None; T5_CODE_CONST_LIMIT];
    let mut i = 0;
    while i < T5_CODE_CONST.len() {
        let (name, t5) = T5_CODE_CONST[i];
        let want = alias_of(name);
        let slot = t5 as usize;
        let mut j = 0;
        while j < IW4_CODE_CONST.len() {
            if name_eq(want, IW4_CODE_CONST[j].0) {
                out[slot] = Some(IW4_CODE_CONST[j].1);
                break;
            }
            j += 1;
        }
        i += 1;
    }
    out
}

const fn build_texture_remap() -> [Option<u16>; T5_CODE_TEXTURE_LIMIT] {
    let mut out = [None; T5_CODE_TEXTURE_LIMIT];
    let mut i = 0;
    while i < T5_CODE_TEXTURE.len() {
        let (name, t5) = T5_CODE_TEXTURE[i];
        let slot = t5 as usize;
        let mut j = 0;
        while j < IW4_CODE_TEXTURE.len() {
            if name_eq(name, IW4_CODE_TEXTURE[j].0) {
                out[slot] = Some(IW4_CODE_TEXTURE[j].1);
                break;
            }
            j += 1;
        }
        i += 1;
    }
    out
}

const T5_TO_IW4_CODE_CONST: [Option<u16>; T5_CODE_CONST_LIMIT] = build_const_remap();
const T5_TO_IW4_CODE_TEXTURE: [Option<u16>; T5_CODE_TEXTURE_LIMIT] = build_texture_remap();

#[must_use]
pub fn remap_code_const_index(t5: u16) -> Option<u16> {
    if (T5_CODE_FOG..=T5_CODE_SUN_FOG_COLOR).contains(&t5) {
        return None;
    }
    T5_TO_IW4_CODE_CONST.get(usize::from(t5)).copied().flatten()
}

#[must_use]
pub fn t5_code_const_name(t5: u16) -> Option<&'static str> {
    T5_CODE_CONST
        .iter()
        .find(|(_, index)| *index == t5)
        .map(|(name, _)| *name)
}

pub const LEFTOVER_T5_CODE_BASE: u16 = 0x200;

pub const T5_CODE_VPOSX_TO_WORLD: u16 = 0x16;

pub const T5_CODE_VPOSY_TO_WORLD: u16 = 0x17;

pub const T5_CODE_VPOS1_TO_WORLD: u16 = 0x18;

pub const T5_CODE_EYEOFFSET: u16 = 0xbf;

pub const T5_CODE_FOG: u16 = 0x38;
pub const T5_CODE_FOG_COLOR: u16 = 0x3a;
pub const T5_CODE_SUN_FOG: u16 = 0x3b;
pub const T5_CODE_SUN_FOG_DIR: u16 = 0x3c;
pub const T5_CODE_SUN_FOG_COLOR: u16 = 0x3d;

pub const T5_CODE_FOG2: u16 = 0x39;

pub const T5_CODE_LIGHT_ATTENUATION: u16 = 0x5;

pub const T5_CODE_LIGHT_FALLOFF_A: u16 = 0x6;

pub const T5_CODE_LIGHT_FALLOFF_B: u16 = 0x7;

pub const T5_CODE_LIGHT_SPOT_MATRIX0: u16 = 0x8;

pub const T5_CODE_LIGHT_SPOT_MATRIX1: u16 = 0x9;

pub const T5_CODE_LIGHT_SPOT_MATRIX2: u16 = 0xa;

pub const T5_CODE_LIGHT_SPOT_MATRIX3: u16 = 0xb;

pub const T5_CODE_LIGHT_SPOT_AABB: u16 = 0xc;

pub const T5_CODE_LIGHT_CONE_CONTROL1: u16 = 0xd;

pub const T5_CODE_LIGHT_CONE_CONTROL2: u16 = 0xe;

pub const T5_CODE_LIGHT_SPOT_COOKIE_SLIDE: u16 = 0xf;

pub const T5_CODE_SUN_POSITION: u16 = 0x32;

pub const T5_CODE_SUN_DIFFUSE: u16 = 0x33;

pub const T5_CODE_SUN_SPECULAR: u16 = 0x34;

pub const T5_CODE_HDRCONTROL_0: u16 = 0x89;

pub const T5_CODE_POSTFX_CONTROL0: u16 = 0x79;

pub const T5_CODE_POSTFX_CONTROL6: u16 = 0x7f;

pub const T5_CODE_SKY_TRANSITION: u16 = 0x44;

pub const T5_CODE_SKY_COLOR_MULTIPLIER: u16 = 0xc2;

pub const T5_CODE_HDRCONTROL_1: u16 = 0x8a;

pub const T5_CODE_LIGHT_HERO_SCALE: u16 = 0xa9;

pub const T5_CODE_HERO_LIGHTING_R: u16 = 0xa6;

pub const T5_CODE_HERO_LIGHTING_G: u16 = 0xa7;

pub const T5_CODE_HERO_LIGHTING_B: u16 = 0xa8;

pub const T5_CODE_GENERIC_PARAM4: u16 = 0xbb;

pub const T5_CODE_EXTRA_CAM_PARAM: u16 = 0xc3;

pub const T5_CODE_GENERIC_PARAM0: u16 = 0xb7;

pub const T5_CODE_GENERIC_PARAM1: u16 = 0xb8;

pub const T5_CODE_GENERIC_PARAM5: u16 = 0xbc;

pub const T5_CODE_GENERIC_PARAM6: u16 = 0xbd;

pub const T5_CODE_WIND_DIRECTION: u16 = 0x4f;

pub const T5_CODE_GRASS_WIND_FORCE0: u16 = 0x54;

pub const T5_CODE_VARIANT_WIND_SPRING_0: u16 = 0x62;

pub const T5_CODE_VARIANT_WIND_SPRING_15: u16 = 0x71;

pub const T5_CODE_CHARACTER_CHARRED_AMOUNT: u16 = 0x76;

pub const T5_CODE_TREECANOPY_PARMS: u16 = 0x77;

pub const T5_CODE_CUSTOMWIND_CENTER: u16 = 0xc0;

pub const T5_CODE_CUSTOMWIND_SPRING: u16 = 0xc1;

pub const T5_CUSTOM_SAMPLER_DEST: [u16; 4] = [15, 12, 13, 14];

pub const IW4_CUSTOM_SAMPLER_DEST: [u16; 3] = [1, 2, 3];

#[must_use]
pub fn remap_t5_custom_sampler_flags(_t5: u8) -> u8 {
    0
}

#[must_use]
pub fn leftover_t5_code_bank(t5: u16) -> Option<u16> {
    match t5 {
        T5_CODE_VPOSX_TO_WORLD
        | T5_CODE_VPOSY_TO_WORLD
        | T5_CODE_VPOS1_TO_WORLD
        | T5_CODE_EYEOFFSET
        | T5_CODE_FOG
        | T5_CODE_FOG_COLOR
        | T5_CODE_SUN_FOG
        | T5_CODE_SUN_FOG_DIR
        | T5_CODE_SUN_FOG_COLOR
        | T5_CODE_FOG2
        | T5_CODE_LIGHT_ATTENUATION
        | T5_CODE_LIGHT_FALLOFF_A
        | T5_CODE_LIGHT_FALLOFF_B
        | T5_CODE_LIGHT_SPOT_MATRIX0
        | T5_CODE_LIGHT_SPOT_MATRIX1
        | T5_CODE_LIGHT_SPOT_MATRIX2
        | T5_CODE_LIGHT_SPOT_MATRIX3
        | T5_CODE_LIGHT_SPOT_AABB
        | T5_CODE_LIGHT_CONE_CONTROL1
        | T5_CODE_LIGHT_CONE_CONTROL2
        | T5_CODE_LIGHT_SPOT_COOKIE_SLIDE
        | T5_CODE_SUN_POSITION
        | T5_CODE_SUN_DIFFUSE
        | T5_CODE_SUN_SPECULAR
        | T5_CODE_SKY_TRANSITION
        | T5_CODE_SKY_COLOR_MULTIPLIER
        | T5_CODE_HDRCONTROL_0
        | T5_CODE_HDRCONTROL_1
        | T5_CODE_LIGHT_HERO_SCALE
        | T5_CODE_HERO_LIGHTING_R
        | T5_CODE_HERO_LIGHTING_G
        | T5_CODE_HERO_LIGHTING_B
        | T5_CODE_EXTRA_CAM_PARAM
        | T5_CODE_GENERIC_PARAM0
        | T5_CODE_GENERIC_PARAM1
        | T5_CODE_GENERIC_PARAM4
        | T5_CODE_GENERIC_PARAM5
        | T5_CODE_GENERIC_PARAM6
        | T5_CODE_WIND_DIRECTION
        | T5_CODE_GRASS_WIND_FORCE0
        | T5_CODE_TREECANOPY_PARMS
        | T5_CODE_CHARACTER_CHARRED_AMOUNT
        | T5_CODE_CUSTOMWIND_CENTER
        | T5_CODE_CUSTOMWIND_SPRING => Some(LEFTOVER_T5_CODE_BASE.checked_add(t5)?),
        T5_CODE_POSTFX_CONTROL0..=T5_CODE_POSTFX_CONTROL6
        | T5_CODE_VARIANT_WIND_SPRING_0..=T5_CODE_VARIANT_WIND_SPRING_15 => {
            Some(LEFTOVER_T5_CODE_BASE.checked_add(t5)?)
        }
        _ => None,
    }
}

#[must_use]
pub fn remap_t5_code_const_source(t5: u16) -> Option<u16> {
    remap_code_const_index(t5).or_else(|| leftover_t5_code_bank(t5))
}

#[must_use]
pub fn remap_code_texture_index(t5: u32) -> Option<u32> {
    let index = u16::try_from(t5).ok()?;
    T5_TO_IW4_CODE_TEXTURE
        .get(usize::from(index))
        .copied()
        .flatten()
        .map(u32::from)
}

#[must_use]
pub fn terrain_scorch_binding_hash(layer: usize) -> u32 {
    let name = format!("iw4l_t5_terrain_scorch_{layer}");
    crate::fnv1a64(name.as_bytes()) as u32
}

pub fn iw4_code_const_index(name: &str) -> Option<u16> {
    IW4_CODE_CONST
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, index)| *index)
}

pub fn iw4_code_texture_index(name: &str) -> Option<u32> {
    IW4_CODE_TEXTURE
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, index)| u32::from(*index))
}
