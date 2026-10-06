#![allow(dead_code)]

pub const RAW_FILE: usize = 16;

pub const FONT: usize = 24;
pub const GLYPH: usize = 24;
pub const STRING_TABLE: usize = 16;
pub const STRING_TABLE_CELL: usize = 8;
pub const SCRIPT_FILE: usize = 24;
pub const PHYS_PRESET: usize = 68;
pub const LOCALIZE_ENTRY: usize = 8;

pub const LEADERBOARD_DEF: usize = 32;

pub const LB_COLUMN_DEF: usize = 40;

pub const STRUCTURED_DATA_DEF_SET: usize = 12;
pub const STRUCTURED_DATA_DEF: usize = 52;
pub const STRUCTURED_DATA_ENUM: usize = 12;
pub const STRUCTURED_DATA_ENUM_ENTRY: usize = 8;
pub const STRUCTURED_DATA_STRUCT: usize = 16;

pub const STRUCTURED_DATA_STRUCT_PROPERTY: usize = 20;
pub const STRUCTURED_DATA_ARRAY: usize = 16;

pub const TECHNIQUE_SET: usize = 228;

pub const TECHNIQUE_SLOT_COUNT: usize = 54;
pub const TECHNIQUE_SLOTS_OFF: usize = 12;

pub const TECHNIQUE_SET_WORLD_VERT_FORMAT_OFF: usize = 4;
pub const MATERIAL_PASS: usize = 20;
pub const MATERIAL_SHADER_ARGUMENT: usize = 8;
pub const VERTEX_DECL: usize = 100;

pub const VERTEX_DECL_STREAM_COUNT_OFF: usize = 0x04;
pub const VERTEX_DECL_HAS_OPTIONAL_SOURCE_OFF: usize = 0x05;
pub const VERTEX_DECL_ROUTING_OFF: usize = 0x08;
pub const VERTEX_DECL_ROUTING_COUNT: usize = 13;
pub const PIXEL_SHADER: usize = 16;

pub const MATERIAL: usize = 104;

pub const MATERIAL_STATE_BITS_ENTRY_OFF: usize = 0x18;

pub const MATERIAL_TEXTURE_COUNT_OFF: usize = 78;
pub const MATERIAL_TECHNIQUE_SET_OFF: usize = 84;
pub const MATERIAL_TEXTURE_TABLE_OFF: usize = 88;
pub const MATERIAL_CONSTANT_TABLE_OFF: usize = 92;
pub const MATERIAL_STATE_BITS_OFF: usize = 96;

pub const MATERIAL_TEXTURE_DEF: usize = 12;
pub const MATERIAL_TEXTURE_DEF_U_OFF: usize = 8;
pub const MATERIAL_TEXTURE_DEF_SEMANTIC_OFF: usize = 7;
pub const MATERIAL_CONSTANT_DEF: usize = 32;
pub const GFX_STATE_BITS: usize = 8;
pub const SEMANTIC_WATER: u8 = 0x0B;

pub const GFX_IMAGE: usize = 32;
pub const GFX_IMAGE_NAME_OFF: usize = 28;
pub const GFX_IMAGE_LOAD_DEF: usize = 16;

pub const XANIM_PARTS: usize = 88;
pub const XANIM_NOTIFY_COUNT_OFF: usize = 27;
pub const XANIM_NAMES_OFF: usize = 48;
pub const XANIM_DATA_BYTE_OFF: usize = 52;
pub const XANIM_DATA_SHORT_OFF: usize = 56;
pub const XANIM_DATA_INT_OFF: usize = 60;
pub const XANIM_RANDOM_DATA_SHORT_OFF: usize = 64;
pub const XANIM_RANDOM_DATA_BYTE_OFF: usize = 68;
pub const XANIM_RANDOM_DATA_INT_OFF: usize = 72;
pub const XANIM_INDICES_OFF: usize = 76;
pub const XANIM_NOTIFY_OFF: usize = 80;
pub const XANIM_DELTA_PART_OFF: usize = 84;
pub const XANIM_NOTIFY_INFO: usize = 8;
pub const XANIM_DELTA_PART: usize = 12;

pub const WEAPON_COMPLETE: usize = 184;
pub const WEAPON_DEF: usize = 1956;

pub const WEAPON_ANIM_COUNT: usize = 42;
pub const WEAPON_ANIM_BLAST_FRONT: usize = 0x23;
pub const WEAPON_ANIM_ADS_UP: usize = 0x27;
pub const WEAPON_ANIM_ADS_DOWN: usize = 0x28;
pub const WEAPON_HIDE_TAG_COUNT: usize = 32;
pub const WEAPON_SCOPE_COUNT: usize = 6;
pub const WEAPON_UNDERBARREL_COUNT: usize = 3;
pub const WEAPON_OTHER_ATTACH_COUNT: usize = 4;
pub const WEAPON_ATTACHMENT_SLOT_COUNT: usize =
    WEAPON_SCOPE_COUNT + WEAPON_UNDERBARREL_COUNT + WEAPON_OTHER_ATTACH_COUNT;

pub const WEAPON_ATTACHMENT: usize = 164;

pub const ATTACH_ADS_OVERLAY_OFF: usize = 92;
pub const ATTACH_WORLD_MODELS_OFF: usize = 20;
pub const ATTACH_VIEW_MODELS_OFF: usize = 24;
pub const ATTACH_RETICLE_OFF: usize = 28;

pub const ATTACH_ADS_SETTINGS_OFF: usize = 72;
pub const ATTACH_SIGHT_OFF: usize = 36;
pub const ATTACH_ADDONS_OFF: usize = 44;
pub const ATTACH_GENERAL_OFF: usize = 48;
pub const ATTACH_AMMUNITION_OFF: usize = 56;
pub const ATTACH_ADS_SETTINGS_MAIN_OFF: usize = 76;
pub const ATTACH_SCALES_OFF: usize = 108;
pub const ATTACH_FLAGS_OFF: usize = 160;
pub const ATTACH_MODEL_COUNT: usize = 16;
pub const ATTACH_RETICLE_COUNT: usize = 8;

pub const WEAPON_COMPLETE_SCOPES_OFF: usize = 16;

pub const WEAPON_COMPLETE_HIDE_TAGS_OFF: usize = 0x0c;

pub const WEAPON_COMPLETE_ADS_ZOOM_FOV_OFF: usize = 0x48;
pub const WEAPON_COMPLETE_ADS_TRANS_IN_TIME_OFF: usize = 0x4c;
pub const WEAPON_COMPLETE_ADS_TRANS_OUT_TIME_OFF: usize = 0x50;
pub const WEAPON_COMPLETE_CLIP_OFF: usize = 0x54;
pub const WEAPON_COMPLETE_PENETRATE_MULTIPLIER_OFF: usize = 0x68;
pub const WEAPON_COMPLETE_MOTION_TRACKER_OFF: usize = 0xb4;

pub const WEAPON_COMPLETE_IMPACT_TYPE_OFF: usize = 0x58;
pub const WEAPON_COMPLETE_FIRE_TIME_OFF: usize = 0x5c;

pub const WEAPON_DEF_WEAP_TYPE_OFF: usize = 0x2c;
pub const WEAPON_DEF_WEAP_CLASS_OFF: usize = 0x30;
pub const WEAPON_DEF_FIRE_TYPE_OFF: usize = 0x3c;

pub const WEAPON_DEF_WORLD_MODEL_OFF: usize = 0x1e0;

pub const WEAPON_DEF_OVERLAY_SHADER_OFF: usize = 1016;

pub const WEAPON_DEF_OVERLAY_RETICLE_OFF: usize = 1032;
pub const WEAPON_DEF_OVERLAY_WIDTH_OFF: usize = 1036;
pub const WEAPON_DEF_OVERLAY_HEIGHT_OFF: usize = 1040;
const _: () = assert!(WEAPON_DEF_OVERLAY_RETICLE_OFF == WEAPON_DEF_OVERLAY_SHADER_OFF + 16);
const _: () = assert!(WEAPON_DEF_OVERLAY_WIDTH_OFF == WEAPON_DEF_OVERLAY_RETICLE_OFF + 4);
const _: () = assert!(WEAPON_DEF_OVERLAY_HEIGHT_OFF == WEAPON_DEF_OVERLAY_WIDTH_OFF + 4);
pub const WEAPON_DEF_OVERLAY_INTERFACE_OFF: usize = 1052;
const _: () = assert!(WEAPON_DEF_OVERLAY_INTERFACE_OFF == WEAPON_DEF_OVERLAY_HEIGHT_OFF + 12);

pub const WEAPON_DEF_AMMO_COUNTER_CLIP_OFF: usize = 0x20c;

pub const WEAPON_DEF_START_AMMO_OFF: usize = 0x210;
pub const WEAPON_DEF_AMMO_INDEX_OFF: usize = 0x218;
pub const WEAPON_DEF_CLIP_INDEX_OFF: usize = 0x220;
pub const WEAPON_DEF_MAX_AMMO_OFF: usize = 0x224;
pub const WEAPON_DEF_SHOTS_PER_FIRE_OFF: usize = 0x228;

pub const WEAPON_DEF_MELEE_DELAY_OFF: usize = 0x24c;
pub const WEAPON_DEF_MELEE_CHARGE_DELAY_OFF: usize = 0x250;
pub const WEAPON_DEF_MELEE_TIME_OFF: usize = 0x26c;
pub const WEAPON_DEF_MELEE_CHARGE_TIME_OFF: usize = 0x270;
pub const WEAPON_DEF_KNIFE_MODEL_OFF: usize = 492;

pub const WEAPON_DEF_FIRE_DELAY_OFF: usize = 0x248;
pub const WEAPON_DEF_RECHAMBER_TIME_OFF: usize = 0x258;

pub const WEAPON_DEF_RECHAMBER_BOLT_TIME_OFF: usize = 0x25c;

pub const WEAPON_DEF_RECHAMBER_BOLT_DELAY_OFF: usize = 0x260;
const _: () = assert!(WEAPON_DEF_RECHAMBER_BOLT_TIME_OFF == WEAPON_DEF_RECHAMBER_TIME_OFF + 4);
const _: () =
    assert!(WEAPON_DEF_RECHAMBER_BOLT_DELAY_OFF == WEAPON_DEF_RECHAMBER_BOLT_TIME_OFF + 4);
pub const WEAPON_DEF_RELOAD_TIME_OFF: usize = 0x274;
pub const WEAPON_DEF_RELOAD_EMPTY_TIME_OFF: usize = 0x27c;
pub const WEAPON_DEF_RELOAD_ADD_TIME_OFF: usize = 0x280;
pub const WEAPON_DEF_RELOAD_START_TIME_OFF: usize = 0x284;
pub const WEAPON_DEF_RELOAD_START_ADD_TIME_OFF: usize = 0x288;
pub const WEAPON_DEF_RELOAD_END_TIME_OFF: usize = 0x28c;
pub const WEAPON_DEF_DROP_TIME_OFF: usize = 0x290;
pub const WEAPON_DEF_RAISE_TIME_OFF: usize = 0x294;

pub const WEAPON_DEF_QUICK_DROP_TIME_OFF: usize = 0x29c;
pub const WEAPON_DEF_QUICK_RAISE_TIME_OFF: usize = 0x2a0;
const _: () = assert!(WEAPON_DEF_QUICK_DROP_TIME_OFF == WEAPON_DEF_RAISE_TIME_OFF + 8);
const _: () = assert!(WEAPON_DEF_QUICK_RAISE_TIME_OFF == WEAPON_DEF_QUICK_DROP_TIME_OFF + 4);
pub const WEAPON_DEF_SPRINT_RAISE_TIME_OFF: usize = 0x2b0;
pub const WEAPON_DEF_SPRINT_LOOP_TIME_OFF: usize = 0x2b4;
pub const WEAPON_DEF_SPRINT_DROP_TIME_OFF: usize = 0x2b8;
pub const WEAPON_DEF_MOVE_SPEED_OFF: usize = 0x3e4;
pub const WEAPON_DEF_ADS_MOVE_SPEED_OFF: usize = 0x3e8;

pub const WEAPON_DEF_ADS_ZOOM_IN_FRAC_OFF: usize = 0x3f0;
pub const WEAPON_DEF_ADS_ZOOM_OUT_FRAC_OFF: usize = 0x3f4;

pub const WEAPON_DEF_ADS_IN_RATE_OFF: usize = 0x694;
pub const WEAPON_DEF_ADS_OUT_RATE_OFF: usize = 0x698;

pub const WEAPON_DEF_MIN_DAMAGE_OFF: usize = 0x69c;
pub const WEAPON_DEF_MIN_PLAYER_DAMAGE_OFF: usize = 0x6a0;
pub const WEAPON_DEF_MAX_DAMAGE_RANGE_OFF: usize = 0x6a4;
pub const WEAPON_DEF_MIN_DAMAGE_RANGE_OFF: usize = 0x6a8;

pub const WEAPON_DEF_DAMAGE_OFF: usize = 0x238;
pub const WEAPON_DEF_MELEE_DAMAGE_OFF: usize = 0x240;

pub const WEAPON_DEF_BOOL_PACK_OFF: usize = 0x768;
pub const WEAPON_DEF_INHERITS_PERKS_OFF: usize = 0x76f;
pub const WEAPON_DEF_RIFLE_BULLET_OFF: usize = 0x771;
pub const WEAPON_DEF_RICOCHET_CHANCE_OFF: usize = 0x538;
pub const WEAPON_DEF_EXPLOSIVE_BULLET_OFF: usize = 0x778;
pub const WEAPON_DEF_SILENCED_OFF: usize = 0x787;
pub const WEAPON_DEF_BOLT_ACTION_OFF: usize = 0x773;
pub const WEAPON_DEF_AIM_DOWN_SIGHT_OFF: usize = 0x774;
pub const WEAPON_DEF_CAN_HOLD_BREATH_OFF: usize = 0x775;

pub const WEAPON_DEF_RECHAMBER_WHILE_ADS_OFF: usize = 0x777;

pub const WEAPON_DEF_ADS_FIRE_ONLY_OFF: usize = 0x77c;

pub const WEAPON_DEF_DISABLE_SWITCH_TO_WHEN_EMPTY_OFF: usize = 0x77e;

pub const WEAPON_DEF_NO_PARTIAL_RELOAD_OFF: usize = 0x784;
pub const WEAPON_DEF_SEGMENTED_RELOAD_OFF: usize = 0x785;

pub const WEAPON_DEF_OFFHAND_HOLD_IS_CANCELABLE_OFF: usize = 0x79a;

const _: () = assert!(WEAPON_DEF_DISABLE_SWITCH_TO_WHEN_EMPTY_OFF == WEAPON_DEF_BOOL_PACK_OFF + 22);
const _: () = assert!(WEAPON_DEF_ADS_FIRE_ONLY_OFF == WEAPON_DEF_BOOL_PACK_OFF + 20);
const _: () = assert!(WEAPON_DEF_STICK_TO_PLAYERS_OFF == WEAPON_DEF_PROJ_IMPACT_EXPLODE_OFF + 1);
const _: () =
    assert!(WEAPON_DEF_NO_PARTIAL_RELOAD_OFF == WEAPON_DEF_DISABLE_SWITCH_TO_WHEN_EMPTY_OFF + 6);
const _: () = assert!(WEAPON_DEF_OFFHAND_HOLD_IS_CANCELABLE_OFF == WEAPON_DEF_BOOL_PACK_OFF + 50);
const _: () = assert!(
    WEAPON_DEF_OFFHAND_HOLD_IS_CANCELABLE_OFF == WEAPON_DEF_DISABLE_SWITCH_TO_WHEN_EMPTY_OFF + 28
);

pub const WEAPON_DEF_HIP_SPREAD_STAND_MIN_OFF: usize = 0x428;

pub const WEAPON_DEF_ADS_IDLE_AMOUNT_OFF: usize = 0x45c;
pub const WEAPON_DEF_HIP_IDLE_AMOUNT_OFF: usize = 0x460;
pub const WEAPON_DEF_ADS_IDLE_SPEED_OFF: usize = 0x464;
pub const WEAPON_DEF_HIP_IDLE_SPEED_OFF: usize = 0x468;
pub const WEAPON_DEF_IDLE_CROUCH_FACTOR_OFF: usize = 0x46c;
pub const WEAPON_DEF_IDLE_PRONE_FACTOR_OFF: usize = 0x470;

pub const WEAPON_DEF_OFFHAND_CLASS_OFF: usize = 0x40;

pub const WEAPON_DEF_HOLD_FIRE_TIME_OFF: usize = 0x264;

pub const WEAPON_DEF_FUSE_TIME_OFF: usize = 0x2e0;

pub const WEAPON_DEF_EXPLOSION_RADIUS_OFF: usize = 0x4e0;
pub const WEAPON_DEF_EXPLOSION_RADIUS_MIN_OFF: usize = 0x4e4;
pub const WEAPON_DEF_EXPLOSION_INNER_DAMAGE_OFF: usize = 0x4e8;
pub const WEAPON_DEF_EXPLOSION_OUTER_DAMAGE_OFF: usize = 0x4ec;
pub const WEAPON_DEF_PROJECTILE_SPEED_OFF: usize = 0x4fc;
pub const WEAPON_DEF_PROJECTILE_SPEED_UP_OFF: usize = 0x500;
pub const WEAPON_DEF_PROJECTILE_ACTIVATE_DIST_OFF: usize = 0x508;

pub const WEAPON_DEF_PROJ_EXPLOSION_TYPE_OFF: usize = 0x51c;

pub const WEAPON_DEF_COOK_OFF_HOLD_OFF: usize = 0x779;
pub const WEAPON_DEF_PROJ_IMPACT_EXPLODE_OFF: usize = 0x78a;

pub const WEAPON_DEF_STICK_TO_PLAYERS_OFF: usize = 0x78b;

pub const WEAPON_DEF_STICKINESS_OFF: usize = 0x530;
pub const WEAPON_DEF_HAS_DETONATOR_OFF: usize = 0x78e;
pub const WEAPON_DEF_TIMED_DETONATION_OFF: usize = 0x790;
pub const WEAPON_DEF_ROTATE_OFF: usize = 0x791;

pub const WEAPON_DEF_RETICLE_CENTER_OFF: usize = 296;
pub const WEAPON_DEF_RETICLE_SIDE_OFF: usize = 300;

pub const WEAPON_DEF_RETICLE_CENTER_SIZE_OFF: usize = 304;
pub const WEAPON_DEF_RETICLE_SIDE_SIZE_OFF: usize = 308;
pub const WEAPON_DEF_RETICLE_MIN_OFS_OFF: usize = 312;

pub const WEAPON_DEF_HUD_ICON_OFF: usize = 500;
pub const WEAPON_DEF_KILL_ICON_RATIO_OFF: usize = 1224;
pub const WEAPON_DEF_FLIP_KILL_ICON_OFF: usize = 1923;
pub const WEAPON_COMPLETE_KILL_ICON_OFF: usize = 132;

pub const SND_ALIAS_CUSTOM: usize = 4;

pub const WEAPON_DEF_NOTE_SOUND_KEYS_OFF: usize = 24;
pub const WEAPON_DEF_NOTE_SOUND_VALUES_OFF: usize = 28;
pub const WEAPON_DEF_NOTE_SOUND_MAP_COUNT: usize = 24;

pub const WEAPON_DEF_NOTE_RUMBLE_KEYS_OFF: usize = 32;
pub const WEAPON_DEF_NOTE_RUMBLE_VALUES_OFF: usize = 36;
pub const WEAPON_DEF_NOTE_RUMBLE_MAP_COUNT: usize = 16;

pub const WEAPON_DEF_VIEW_FLASH_OFF: usize = 72;
pub const WEAPON_DEF_WORLD_FLASH_OFF: usize = 76;
pub const WEAPON_DEF_VIEW_SHELL_EJECT_OFF: usize = 280;
pub const WEAPON_DEF_WORLD_SHELL_EJECT_OFF: usize = 284;
pub const WEAPON_DEF_VIEW_LAST_SHOT_EJECT_OFF: usize = 288;
pub const WEAPON_DEF_WORLD_LAST_SHOT_EJECT_OFF: usize = 292;
const _: () = assert!(WEAPON_DEF_SND_PICKUP_OFF == WEAPON_DEF_WORLD_FLASH_OFF + 4);

pub const WEAPON_DEF_SND_PICKUP_OFF: usize = 80;
pub const WEAPON_DEF_SND_PICKUP_PLAYER_OFF: usize = 84;
pub const WEAPON_DEF_SND_AMMO_PICKUP_OFF: usize = 88;
pub const WEAPON_DEF_SND_AMMO_PICKUP_PLAYER_OFF: usize = 92;
pub const WEAPON_DEF_SND_PULLBACK_OFF: usize = 100;
pub const WEAPON_DEF_SND_PULLBACK_PLAYER_OFF: usize = 104;
pub const WEAPON_DEF_SND_FIRE_OFF: usize = 108;
pub const WEAPON_DEF_SND_FIRE_PLAYER_OFF: usize = 112;

pub const WEAPON_DEF_SND_FIRE_PLAYER_AKIMBO_OFF: usize = 116;
pub const WEAPON_DEF_SND_FIRE_LOOP_OFF: usize = 120;
pub const WEAPON_DEF_SND_FIRE_LOOP_PLAYER_OFF: usize = 124;
pub const WEAPON_DEF_SND_FIRE_STOP_OFF: usize = 128;
pub const WEAPON_DEF_SND_FIRE_STOP_PLAYER_OFF: usize = 132;
pub const WEAPON_DEF_SND_FIRE_LAST_OFF: usize = 136;
pub const WEAPON_DEF_SND_FIRE_LAST_PLAYER_OFF: usize = 140;
pub const WEAPON_DEF_SND_EMPTY_FIRE_OFF: usize = 144;
pub const WEAPON_DEF_SND_EMPTY_FIRE_PLAYER_OFF: usize = 148;
pub const WEAPON_DEF_SND_MELEE_SWIPE_OFF: usize = 152;
pub const WEAPON_DEF_SND_MELEE_SWIPE_PLAYER_OFF: usize = 156;
pub const WEAPON_DEF_SND_MELEE_HIT_OFF: usize = 160;
pub const WEAPON_DEF_SND_MELEE_MISS_OFF: usize = 164;
pub const WEAPON_DEF_SND_RECHAMBER_OFF: usize = 168;
pub const WEAPON_DEF_SND_RECHAMBER_PLAYER_OFF: usize = 172;
pub const WEAPON_DEF_SND_RELOAD_OFF: usize = 176;
pub const WEAPON_DEF_SND_RELOAD_PLAYER_OFF: usize = 180;
pub const WEAPON_DEF_SND_RELOAD_EMPTY_OFF: usize = 184;
pub const WEAPON_DEF_SND_RELOAD_EMPTY_PLAYER_OFF: usize = 188;
pub const WEAPON_DEF_SND_RELOAD_START_OFF: usize = 192;
pub const WEAPON_DEF_SND_RELOAD_START_PLAYER_OFF: usize = 196;
pub const WEAPON_DEF_SND_RELOAD_END_OFF: usize = 200;
pub const WEAPON_DEF_SND_RELOAD_END_PLAYER_OFF: usize = 204;
pub const WEAPON_DEF_SND_ALT_SWITCH_OFF: usize = 232;
pub const WEAPON_DEF_SND_ALT_SWITCH_PLAYER_OFF: usize = 236;
pub const WEAPON_DEF_SND_RAISE_OFF: usize = 240;
pub const WEAPON_DEF_SND_RAISE_PLAYER_OFF: usize = 244;
pub const WEAPON_DEF_SND_FIRST_RAISE_OFF: usize = 248;
pub const WEAPON_DEF_SND_FIRST_RAISE_PLAYER_OFF: usize = 252;
pub const WEAPON_DEF_SND_PUTAWAY_OFF: usize = 256;
pub const WEAPON_DEF_SND_PUTAWAY_PLAYER_OFF: usize = 260;
pub const SURF_TYPE_COUNT: usize = 31;
pub const HITLOC_COUNT: usize = 20;
pub const VEHICLE_DEF: usize = 1020;
pub const VEHICLE_PHYS_DEF: usize = 180;
pub const VEHICLE_SURFACE_SND_COUNT: usize = 31;
pub const TRACER_DEF: usize = 112;
pub const ADS_OVERLAY: usize = 36;

pub const ADS_OVERLAY_SHADER_OFF: usize = 0;

pub const ADS_OVERLAY_SHADER_LOWRES_OFF: usize = 4;
pub const ADS_OVERLAY_SHADER_EMP_OFF: usize = 8;
pub const ADS_OVERLAY_SHADER_EMP_LOWRES_OFF: usize = 12;
pub const ADS_OVERLAY_RETICLE_OFF: usize = 16;
pub const ADS_OVERLAY_WIDTH_OFF: usize = 20;
pub const ADS_OVERLAY_HEIGHT_OFF: usize = 24;

pub const ADS_OVERLAY_THERMAL_OFF: usize = 36;

pub const ATT_AMMO_GENERAL: usize = 24;
pub const ATT_SIGHT: usize = 7;
pub const ATT_RELOAD: usize = 2;
pub const ATT_ADDONS: usize = 2;
pub const ATT_GENERAL: usize = 32;
pub const ATT_AIM_ASSIST: usize = 12;
pub const ATT_AMMUNITION: usize = 24;
pub const ATT_DAMAGE: usize = 28;
pub const ATT_LOCATION_DAMAGE: usize = 76;
pub const ATT_IDLE_SETTINGS: usize = 24;
pub const ATT_ADS_SETTINGS: usize = 56;

pub const ATT_ADS_ZOOM_FOV_OFF: usize = 28;
pub const ATT_ADS_ZOOM_IN_FRAC_OFF: usize = 32;
pub const ATT_ADS_ZOOM_OUT_FRAC_OFF: usize = 36;
pub const ATT_HIP_SPREAD: usize = 48;
pub const ATT_GUN_KICK: usize = 80;
pub const ATT_VIEW_KICK: usize = 40;
pub const ATT_ADS_OVERLAY: usize = 40;
pub const ATT_UI: usize = 20;
pub const ATT_RUMBLES: usize = 8;
pub const ATT_PROJECTILE: usize = 92;

pub const ANIM_OVERRIDE_ENTRY: usize = 24;
pub const WEAPON_COMPLETE_ANIM_OVERRIDE_COUNT_OFF: usize = 32;
pub const WEAPON_COMPLETE_ANIM_OVERRIDES_OFF: usize = 36;
pub const ANIM_OVERRIDE_OVERRIDE_ANIM_OFF: usize = 4;
pub const ANIM_OVERRIDE_ALTMODE_ANIM_OFF: usize = 8;
pub const ANIM_OVERRIDE_ANIM_TREE_TYPE_OFF: usize = 12;
pub const ANIM_OVERRIDE_ANIM_TIME_OFF: usize = 16;
pub const ANIM_OVERRIDE_ALT_TIME_OFF: usize = 20;
pub const SOUND_OVERRIDE_ENTRY: usize = 16;

pub const WEAPON_COMPLETE_SOUND_OVERRIDE_COUNT_OFF: usize = 40;
pub const WEAPON_COMPLETE_SOUND_OVERRIDES_OFF: usize = 44;

pub const SND_OVERRIDE_TYPE_FIRE: u32 = 1;
pub const SND_OVERRIDE_TYPE_PLAYER_FIRE: u32 = 2;
pub const SND_OVERRIDE_TYPE_PLAYER_AKIMBO: u32 = 3;
pub const SND_OVERRIDE_TYPE_PLAYER_LASTSHOT: u32 = 4;
pub const FX_OVERRIDE_ENTRY: usize = 16;
pub const NOTE_TRACK_SOUND_ENTRY: usize = 12;
pub const RELOAD_STATE_TIMER_ENTRY: usize = 12;

pub const WEAPON_COMPLETE_RELOAD_OVERRIDE_COUNT_OFF: usize = 56;
pub const WEAPON_COMPLETE_RELOAD_OVERRIDES_OFF: usize = 60;

pub const RELOAD_STATE_TIMER_ADD_TIME_OFF: usize = 4;

pub const MENU_LIST: usize = 12;
pub const MENU_DEF: usize = 176;
pub const MENU_DATA: usize = 212;
pub const WINDOW_DEF: usize = 164;
pub const ITEM_DEF: usize = 384;
pub const ITEM_DEF_DATA: usize = 4;
pub const EXPRESSION_SUPPORTING_DATA: usize = 24;
pub const UI_FUNCTION_LIST: usize = 8;
pub const STATEMENT: usize = 80;
pub const EXPRESSION_ENTRY: usize = 12;
pub const ENTRY_INTERNAL_DATA: usize = 8;
pub const OPERAND: usize = 8;
pub const OPERAND_INTERNAL: usize = 4;
pub const EXPRESSION_STRING: usize = 4;
pub const STATIC_DVAR_LIST: usize = 8;
pub const STATIC_DVAR: usize = 8;
pub const STRING_LIST: usize = 8;
pub const MENU_EVENT_HANDLER_SET: usize = 8;
pub const MENU_EVENT_HANDLER: usize = 8;
pub const EVENT_DATA: usize = 4;
pub const CONDITIONAL_SCRIPT: usize = 8;
pub const SET_LOCAL_VAR_DATA: usize = 8;
pub const ITEM_KEY_HANDLER: usize = 12;
pub const LIST_BOX_DEF: usize = 456;
pub const MULTI_DEF: usize = 392;
pub const ITEM_FLOAT_EXPRESSION: usize = 8;
pub const EDIT_FIELD_DEF: usize = 32;
pub const NEWS_TICKER_DEF: usize = 12;
pub const TEXT_SCROLL_DEF: usize = 4;

pub const MAP_ENTS: usize = 96;

pub const SND_ALIAS_LIST: usize = 0xc;
pub const SND_ALIAS: usize = 0x70;

pub const SND_ALIAS_ALIAS_NAME_OFF: usize = 0;
pub const SND_ALIAS_SUBTITLE_OFF: usize = 4;
pub const SND_ALIAS_SECONDARY_OFF: usize = 8;
pub const SND_ALIAS_CHAIN_OFF: usize = 12;
pub const SND_ALIAS_MIXER_GROUP_OFF: usize = 16;
pub const SND_ALIAS_SOUND_FILE_OFF: usize = 20;
pub const SND_ALIAS_SEQUENCE_OFF: usize = 24;
pub const SND_ALIAS_VOL_MIN_OFF: usize = 28;
pub const SND_ALIAS_VOL_MAX_OFF: usize = 32;
pub const SND_ALIAS_VOL_MOD_INDEX_OFF: usize = 36;
pub const SND_ALIAS_PITCH_MIN_OFF: usize = 40;
pub const SND_ALIAS_PITCH_MAX_OFF: usize = 44;
pub const SND_ALIAS_DIST_MIN_OFF: usize = 48;
pub const SND_ALIAS_DIST_MAX_OFF: usize = 52;
pub const SND_ALIAS_VELOCITY_MIN_OFF: usize = 56;
pub const SND_ALIAS_FLAGS_OFF: usize = 60;
pub const SND_ALIAS_SLAVE_PERCENTAGE_OFF: usize = 72;
pub const SND_ALIAS_PROBABILITY_OFF: usize = 76;
pub const SND_ALIAS_LFE_PERCENTAGE_OFF: usize = 80;
pub const SND_ALIAS_CENTER_PERCENTAGE_OFF: usize = 84;
pub const SND_ALIAS_START_DELAY_OFF: usize = 88;
pub const SND_ALIAS_VOLUME_FALLOFF_CURVE_OFF: usize = 92;
pub const SND_ALIAS_ENVELOP_MIN_OFF: usize = 96;
pub const SND_ALIAS_ENVELOP_MAX_OFF: usize = 100;
pub const SND_ALIAS_ENVELOP_PERCENTAGE_OFF: usize = 104;
pub const SND_ALIAS_SPEAKER_MAP_OFF: usize = 108;
pub const SOUND_FILE: usize = 0xc;
pub const SPEAKER_MAP: usize = 0x198;
pub const LOADED_SOUND: usize = 0x2c;
pub const SND_CURVE: usize = 0x88;

pub const XMODEL: usize = 308;
pub const XMODEL_LOD_INFO: usize = 44;
pub const XMODEL_SURFS: usize = 36;

pub const XSURFACE: usize = 68;

pub const XSURFACE_FLAGS_OFF: usize = 1;
pub const XSURFACE_FLAG_DEFORMED: u8 = 0x40;
pub const XSURFACE_VERT_INFO_OFF: usize = 20;
pub const XSURFACE_VERTS0_OFF: usize = 32;
pub const XSURFACE_VERT_LIST_COUNT_OFF: usize = 36;
pub const XSURFACE_VERT_LIST_OFF: usize = 40;
pub const XSURFACE_TRI_INDICES_OFF: usize = 16;
pub const XRIGID_VERT_LIST: usize = 12;
pub const XSURFACE_COLLISION_TREE: usize = 40;
pub const XSURFACE_COLLISION_NODE: usize = 16;
pub const XSURFACE_COLLISION_LEAF: usize = 2;
pub const XSURFACE_TRI16: usize = 6;
pub const XMODEL_COLL_SURF: usize = 44;
pub const XMODEL_COLL_TRI: usize = 48;
pub const XBONE_INFO: usize = 28;
pub const XMODEL_QUAT: usize = 8;
pub const DOBJ_ANIM_MAT: usize = 32;

pub const GFX_PACKED_VERTEX: usize = 32;

pub const XMODEL_BONE_NAMES_OFF: usize = 36;
pub const XMODEL_MATERIAL_HANDLES_OFF: usize = 60;
pub const XMODEL_LOD_INFO_OFF: usize = 64;
pub const XMODEL_COLL_SURFS_OFF: usize = 244;
pub const XMODEL_NUM_COLL_SURFS_OFF: usize = 248;
pub const XMODEL_BONE_INFO_OFF: usize = 256;

pub const XMODEL_RADIUS_OFF: usize = 0x104;
pub const XMODEL_PHYS_PRESET_OFF: usize = 296;
pub const XMODEL_PHYS_COLLMAP_OFF: usize = 300;

pub const PHYS_COLLMAP: usize = 72;
pub const PHYS_GEOM_INFO: usize = 68;
pub const BRUSH_WRAPPER: usize = 68;
pub const CBRUSH_SIDE: usize = 8;
pub const CPLANE: usize = 20;

pub mod mtl_arg {
    pub const MATERIAL_VERTEX_CONST: u16 = 0;
    pub const LITERAL_VERTEX_CONST: u16 = 1;
    pub const MATERIAL_VERTEX_SAMPLER: u16 = 2;
    pub const MATERIAL_PIXEL_SAMPLER: u16 = 3;
    pub const CODE_VERTEX_CONST: u16 = 4;
    pub const CODE_PIXEL_SAMPLER: u16 = 5;
    pub const CODE_PIXEL_CONST: u16 = 6;
    pub const MATERIAL_PIXEL_CONST: u16 = 7;

    pub const LITERAL_PIXEL_CONST: u16 = 8;
}

pub const WATER: usize = 68;
pub const WATER_H0_OFF: usize = 4;
pub const WATER_WTERM_OFF: usize = 8;
pub const WATER_M_OFF: usize = 12;
pub const WATER_N_OFF: usize = 16;
pub const WATER_IMAGE_OFF: usize = 64;

pub const FX_EFFECT_DEF: usize = 0x34;
pub const FX_EFFECT_LOOPING_OFF: usize = 0x10;
pub const FX_EFFECT_ONESHOT_OFF: usize = 0x14;
pub const FX_EFFECT_EMISSION_OFF: usize = 0x18;
pub const FX_EFFECT_ELEMS_OFF: usize = 0x30;

pub const FX_ELEM_DEF: usize = 0x100;
pub const FX_ELEM_TYPE_OFF: usize = 0xb0;
pub const FX_ELEM_VEL_SAMPLES_OFF: usize = 0xb4;
pub const FX_ELEM_VIS_SAMPLES_OFF: usize = 0xb8;
pub const FX_ELEM_VISUALS_OFF: usize = 0xbc;
pub const FX_ELEM_EFFECT_ON_IMPACT_OFF: usize = 0xd8;
pub const FX_ELEM_EFFECT_ON_DEATH_OFF: usize = 0xdc;
pub const FX_ELEM_EFFECT_EMITTED_OFF: usize = 0xe0;
pub const FX_ELEM_EXTENDED_OFF: usize = 0xf4;

pub const FX_ELEM_VEL_STATE_SAMPLE: usize = 0x60;
pub const FX_ELEM_VIS_STATE_SAMPLE: usize = 0x30;
pub const FX_ELEM_MARK_VISUALS: usize = 8;
pub const FX_ELEM_VISUALS: usize = 4;

pub const FX_TRAIL_DEF: usize = 0x24;
pub const FX_TRAIL_VERT_COUNT_OFF: usize = 0x14;
pub const FX_TRAIL_VERTS_OFF: usize = 0x18;
pub const FX_TRAIL_IND_COUNT_OFF: usize = 0x1c;
pub const FX_TRAIL_INDS_OFF: usize = 0x20;
pub const FX_TRAIL_VERTEX: usize = 0x14;
pub const FX_SPARK_FOUNTAIN_DEF: usize = 0x34;
pub const FX_SPOT_LIGHT_DEF: usize = 0x18;

pub const FX_IMPACT_TABLE: usize = 8;
pub const FX_IMPACT_ENTRY: usize = 140;
pub const SURFACE_FX_TABLE: usize = 8;
pub const SURFACE_FX_ENTRY: usize = 124;
pub const SURF_TYPE_NUM: usize = 31;

pub const GFX_LIGHT_DEF: usize = 0x18;
pub const GFX_LIGHT_DEF_IMAGE_OFF: usize = 4;

pub const GFX_LIGHT_DEF_SAMPLER_OFF: usize = 8;

pub const GFX_LIGHT_DEF_CUCOLORIS_IMAGE_OFF: usize = 12;

pub const GFX_LIGHT_DEF_LMAP_LOOKUP_OFF: usize = 20;

pub const COM_WORLD: usize = 0x10;
pub const COM_PRIMARY_LIGHT: usize = 0x50;
pub const COM_PRIMARY_LIGHT_DEF_NAME_OFF: usize = 0x4c;

pub const CLIP_MAP: usize = 0x100;
pub const CLIP_INFO: usize = 0x40;
pub const C_STATIC_MODEL: usize = 0x4c;
pub const CLIP_MATERIAL: usize = 0xc;
pub const C_NODE: usize = 8;
pub const C_LEAF: usize = 0x28;
pub const C_LEAF_BRUSH_NODE: usize = 0x14;
pub const CBRUSH: usize = 0x24;
pub const BOUNDS: usize = 0x18;
pub const COLLISION_BORDER: usize = 0x1c;
pub const COLLISION_PARTITION: usize = 0xc;
pub const COLLISION_AABB_TREE: usize = 0x20;
pub const CMODEL: usize = 0x48;
pub const SMODEL_AABB_NODE: usize = 0x1c;
pub const STAGE: usize = 0x14;
pub const MAP_TRIGGERS: usize = 0x18;
pub const TRIGGER_MODEL: usize = 8;
pub const TRIGGER_HULL: usize = 0x20;
pub const TRIGGER_SLAB: usize = 0x14;
pub const CLIENT_TRIGGERS: usize = 0x3c;
pub const CLIENT_TRIGGER_AABB_NODE: usize = 0x1c;
pub const DYN_ENTITY_DEF: usize = 0x60;
pub const DYN_ENTITY_POSE: usize = 0x20;
pub const DYN_ENTITY_CLIENT: usize = 0x10;
pub const DYN_ENTITY_COLL: usize = 0x14;
pub const DYN_ENTITY_HINGE: usize = 0x2c;

pub const GFX_WORLD: usize = 0x280;
pub const GFX_SKY: usize = 0x10;

pub const GFX_WORLD_DRAW: usize = 0x54;

pub const GFX_WORLD_LIGHT_GRID_OFF: usize = 0xa4;

pub const GFX_LIGHT_GRID: usize = 0x38;
pub const GFX_CELL: usize = 0x30;
pub const GFX_PORTAL: usize = 0x3c;
pub const GFX_PORTAL_HULL_AXIS: usize = 0x24;

pub const GFX_AABB_TREE: usize = 0x2c;
pub const GFX_BRUSH_MODEL: usize = 0x3c;

pub const GFX_WORLD_VERTEX: usize = 0x2c;

pub const GFX_SURFACE: usize = 0x18;

pub const GFX_SURFACE_BOUNDS: usize = 0x18;
pub const GFX_STATIC_MODEL_INST: usize = 0x24;
pub const GFX_STATIC_MODEL_DRAW_INST: usize = 0x4c;
pub const GFX_LIGHT_GRID_COLORS: usize = 0xa8;
pub const GFX_LIGHTMAP_ARRAY: usize = 8;
pub const GFX_SHADOW_GEOMETRY: usize = 0xc;
pub const GFX_LIGHT_REGION: usize = 8;
pub const GFX_LIGHT_REGION_HULL: usize = 0x50;
pub const GFX_HERO_ONLY_LIGHT: usize = 0x44;

pub const GFX_DPVS_STATIC: usize = 0x6c;
pub const GFX_DPVS_DYNAMIC: usize = 0x30;
pub const MATERIAL_MEMORY: usize = 8;

pub const FX_WORLD: usize = 0x74;
pub const FX_GLASS_SYSTEM: usize = 0x70;
pub const FX_GLASS_DEF: usize = 0x24;
pub const FX_GLASS_DEF_MATERIAL_OFF: usize = 0x18;
pub const FX_GLASS_DEF_MATERIAL_SHATTERED_OFF: usize = 0x1c;
pub const FX_GLASS_DEF_PHYS_OFF: usize = 0x20;
pub const FX_GLASS_PIECE_PLACE: usize = 0x20;
pub const FX_GLASS_PIECE_STATE: usize = 0x20;
pub const FX_GLASS_PIECE_DYNAMICS: usize = 0x24;
pub const FX_GLASS_GEOMETRY_DATA: usize = 4;
pub const FX_GLASS_INIT_PIECE_STATE: usize = 0x34;
pub const FX_GLASS_LINK_ORG: usize = 0xc;

pub const GLASS_WORLD: usize = 8;
pub const G_GLASS_DATA: usize = 128;
pub const G_GLASS_PIECE: usize = 12;
pub const G_GLASS_NAME: usize = 12;

// AddonMapEnts header: name + entity string + count + inline MapTriggers +
// ClipInfo ptr + sub-model counts/arrays.
pub const ADDON_MAP_ENTS: usize = 52;
// cmodel2_t: Bounds + radius + ClipInfo ptr + leaf.
pub const CMODEL2: usize = 72;
// PathData (aipaths asset): name + counts + node/tree arrays.
pub const PATH_DATA: usize = 44;
// VehicleTrack asset: name + segment table.
pub const VEHICLE_TRACK: usize = 12;
pub const VEHICLE_SEGMENT: usize = 44;
pub const VEHICLE_SECTOR: usize = 60;
pub const VEHICLE_OBSTACLE: usize = 12;
// pathnode_t x86 = constant(64) + dynamic(44) + transient(28). Constant
// script-strings are u16 indices, never followed.
pub const PATH_NODE: usize = 136;
pub const PATH_LINK: usize = 12;
pub const PATH_BASENODE: usize = 16;
pub const PATHNODE_TREE: usize = 16;

pub mod fx_elem {
    pub const TRAIL: u8 = 3;
    pub const SPARK_FOUNTAIN: u8 = 6;
    pub const MODEL: u8 = 7;
    pub const SPOT_LIGHT: u8 = 9;
    pub const SOUND: u8 = 10;
    pub const DECAL: u8 = 11;
    pub const RUNNER: u8 = 12;

    pub const fn is_sprite(t: u8) -> bool {
        matches!(t, 0 | 1 | 2 | TRAIL | 4 | 5 | SPARK_FOUNTAIN)
    }
}

pub const WEAPON_COMPLETE_ADS_VIEW_KICK_CENTER_SPEED_OFF: usize = 0x6c;
pub const WEAPON_COMPLETE_HIP_VIEW_KICK_CENTER_SPEED_OFF: usize = 0x70;
pub const WEAPON_DEF_GUN_MAX_PITCH_OFF: usize = 0x474;
pub const WEAPON_DEF_GUN_MAX_YAW_OFF: usize = 0x478;
pub const WEAPON_DEF_ADS_GUN_KICK_REDUCED_KICK_BULLETS_OFF: usize = 0x584;
pub const WEAPON_DEF_ADS_GUN_KICK_REDUCED_KICK_PERCENT_OFF: usize = 0x588;
pub const WEAPON_DEF_ADS_GUN_KICK_PITCH_MIN_OFF: usize = 0x58c;
pub const WEAPON_DEF_ADS_GUN_KICK_PITCH_MAX_OFF: usize = 0x590;
pub const WEAPON_DEF_ADS_GUN_KICK_YAW_MIN_OFF: usize = 0x594;
pub const WEAPON_DEF_ADS_GUN_KICK_YAW_MAX_OFF: usize = 0x598;
pub const WEAPON_DEF_ADS_GUN_KICK_ACCEL_OFF: usize = 0x59c;
pub const WEAPON_DEF_ADS_GUN_KICK_SPEED_MAX_OFF: usize = 0x5a0;
pub const WEAPON_DEF_ADS_GUN_KICK_SPEED_DECAY_OFF: usize = 0x5a4;
pub const WEAPON_DEF_ADS_GUN_KICK_STATIC_DECAY_OFF: usize = 0x5a8;
pub const WEAPON_DEF_ADS_VIEW_KICK_PITCH_MIN_OFF: usize = 0x5ac;
pub const WEAPON_DEF_ADS_VIEW_KICK_PITCH_MAX_OFF: usize = 0x5b0;
pub const WEAPON_DEF_ADS_VIEW_KICK_YAW_MIN_OFF: usize = 0x5b4;
pub const WEAPON_DEF_ADS_VIEW_KICK_YAW_MAX_OFF: usize = 0x5b8;
pub const WEAPON_DEF_HIP_GUN_KICK_REDUCED_KICK_BULLETS_OFF: usize = 0x5c8;
pub const WEAPON_DEF_HIP_GUN_KICK_REDUCED_KICK_PERCENT_OFF: usize = 0x5cc;
pub const WEAPON_DEF_HIP_GUN_KICK_PITCH_MIN_OFF: usize = 0x5d0;
pub const WEAPON_DEF_HIP_GUN_KICK_PITCH_MAX_OFF: usize = 0x5d4;
pub const WEAPON_DEF_HIP_GUN_KICK_YAW_MIN_OFF: usize = 0x5d8;
pub const WEAPON_DEF_HIP_GUN_KICK_YAW_MAX_OFF: usize = 0x5dc;
pub const WEAPON_DEF_HIP_GUN_KICK_ACCEL_OFF: usize = 0x5e0;
pub const WEAPON_DEF_HIP_GUN_KICK_SPEED_MAX_OFF: usize = 0x5e4;
pub const WEAPON_DEF_HIP_GUN_KICK_SPEED_DECAY_OFF: usize = 0x5e8;
pub const WEAPON_DEF_HIP_GUN_KICK_STATIC_DECAY_OFF: usize = 0x5ec;
pub const WEAPON_DEF_HIP_VIEW_KICK_PITCH_MIN_OFF: usize = 0x5f0;
pub const WEAPON_DEF_HIP_VIEW_KICK_PITCH_MAX_OFF: usize = 0x5f4;
pub const WEAPON_DEF_HIP_VIEW_KICK_YAW_MIN_OFF: usize = 0x5f8;
pub const WEAPON_DEF_HIP_VIEW_KICK_YAW_MAX_OFF: usize = 0x5fc;
