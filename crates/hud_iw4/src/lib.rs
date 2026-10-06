#![no_std]
#![forbid(unsafe_code)]

mod ammo;
mod blood;
mod centerprint;
mod cg;
mod compass;
pub mod crosshair;
mod draw_text_cmd;
pub mod expr;
mod flashbang;
pub mod font;
pub mod fov;
mod gamemsg;
mod hudelem;
mod iris;
mod mantle_hint;
pub mod menu_transition;
mod overhead_names;
mod perk;
mod playercard;
mod render_commands;
mod replace_directive;
mod scorebar;
mod scrplace;
mod set_2d;
mod splash;
mod stretch_pic_cmd;
mod text_fx;
mod view_projection;
mod vision_set;

pub use ammo::{
    AmmoCounterClipKind, CLIP_PIP_ALIGN_RIGHT, CLIP_PIP_EMPTY_ALPHA, CLIP_PIP_EMPTY_RGB,
    ClipPipMetrics, LOW_AMMO_WARNING_EFLAGS_SILENCE, LOW_AMMO_WARNING_PULSE_HZ,
    LOW_AMMO_WARNING_PULSE_MAX, LOW_AMMO_WARNING_PULSE_MIN, LowAmmoWarningKind,
    LowAmmoWarningQuery, ammo_counter_clip_kind, check_player_for_low_clip, clip_pip_belt_xy,
    clip_pip_grid_xy, clip_pip_metrics, draw_player_weapon_low_ammo_warning,
    low_ammo_warning_color_pair, low_ammo_warning_kind, low_ammo_warning_outer_gate,
    low_ammo_warning_pulse_frac, vec4_lerp, weaponstate_skips_low_ammo_warning,
};
pub use blood::{
    HEALTH_FRAC_PM_TYPE_NONE, HUD_BLOOD_OVERLAY_LERP_RATE_DEFAULT, MSEC_TO_SEC,
    PAIN_VISION_LERP_OUT_RATE_DEFAULT, PAIN_VISION_TRIGGER_HEALTH_DEFAULT, SPLATTER_GRID_HEIGHT,
    SPLATTER_GRID_WIDTH, SPLATTER_HEALTH_INTENSITY_SCALE, blood_overlay_lerp, get_health_fraction,
    pain_vision_lerp_intensity, pain_vision_must_clear, pain_vision_wants_armed,
    should_draw_blood_overlay, splatter_envelope,
};
pub use centerprint::{
    CENTERPRINT_STRIDE, CG_CENTERPRINT_FADE_TAIL_MS, CG_CENTERTIME_DEFAULT_MS,
    centerprint_replace_name, fade_color, priority_center_print_accepts,
};
pub use cg::Cg;
pub use compass::{
    COMPASS_ENEMY_FIRING_PING_IMAGE, COMPASS_FRIENDLY_HEIGHT_DEFAULT,
    COMPASS_FRIENDLY_WIDTH_DEFAULT, COMPASS_MAX_RANGE_DEFAULT_MP, COMPASS_PLAYER_HEIGHT_DEFAULT,
    COMPASS_PLAYER_IMAGE, COMPASS_PLAYER_WIDTH_DEFAULT, COMPASS_RADAR_LINE_IMAGE,
    COMPASS_RADAR_LINE_THICKNESS_DEFAULT, COMPASS_RADAR_PING_FADE_TIME_DEFAULT,
    COMPASS_RADAR_UPDATE_TIME_DEFAULT, COMPASS_SIZE_DEFAULT, COMPASS_SOUND_PING_FADE_TIME_DEFAULT,
    CompassMapBounds, CompassMapUvWindow, RADARJAM_DIST_MAX, RADARJAM_DIST_MIN, RADARJAM_DIST_NONE,
    REQUIRED_MAP_ASPECT_RATIO_DEFAULT, compass_clamp_offset, compass_fade_alpha,
    compass_friendly_size, compass_map_bounds_from_corners,
    compass_map_bounds_from_minimap_corners, compass_partial_map_uv, compass_player_size,
    compass_sound_ping_fade, compass_up_yaw_vector, radar_contact_trail_visible,
    radar_jam_intensity, radar_jam_nearest_distance, radar_line, radar_line_texture_center_s,
    radar_lines_surround_point, setup_mini_map_frame, world_pos_to_compass_partial,
};
pub use crosshair::{
    AIM_SPREAD_SCALE_MAX, AdsTransition, CG_CROSSHAIR_ALPHA_DEFAULT,
    CG_CROSSHAIR_ALPHA_MIN_DEFAULT, CROSSHAIR_POS_X_SCALE, CROSSHAIR_POS_Y_SCALE,
    EF_CROSSHAIR_SPECIAL_RETICLE, EF_CROSSHAIR_TURRET_VEHICLE, HipCrosshairGate,
    OTHER_FLAGS_BLOCK_CROSSHAIR_HUD, RETICLE_SIDES_ALPHA_DRAW_MIN, RETICLE_VIRTUAL_HALF_HEIGHT,
    RETICLE_VIRTUAL_HEIGHT, WeaponAdsCrosshairFacts, WeaponReticleFacts, calc_crosshair_position,
    calc_reticle_alpha, calc_reticle_spread, hip_crosshair_trans_scale, hip_crosshair_visible,
    reticle_draw_size, should_draw_crosshair, transition_to_ads,
};
pub use draw_text_cmd::{
    AddDrawTextCmd, GFX_CMD_DRAW_TEXT_2D, GFX_CMD_DRAW_TEXT_COLOR, GFX_CMD_DRAW_TEXT_FLAGS,
    GFX_CMD_DRAW_TEXT_FONT, GFX_CMD_DRAW_TEXT_FX_BIRTH_TIME, GFX_CMD_DRAW_TEXT_FX_DECAY_DURATION,
    GFX_CMD_DRAW_TEXT_FX_DECAY_START, GFX_CMD_DRAW_TEXT_FX_LETTER_TIME,
    GFX_CMD_DRAW_TEXT_FX_MATERIAL, GFX_CMD_DRAW_TEXT_FX_MATERIAL_GLOW, GFX_CMD_DRAW_TEXT_MAXCHARS,
    GFX_CMD_DRAW_TEXT_PADDING, GFX_CMD_DRAW_TEXT_ROTATION, GFX_CMD_DRAW_TEXT_TEXT,
    GFX_CMD_DRAW_TEXT_X, GFX_CMD_DRAW_TEXT_XSCALE, GFX_CMD_DRAW_TEXT_Y, GFX_CMD_DRAW_TEXT_YSCALE,
    GfxCmdDrawText2D, GfxCmdDrawText2DArgs, GfxCmdDrawTextFx, add_cmd_draw_text,
    draw_text_render_flags, gfx_cmd_draw_text_size, parse_gfx_cmd_draw_text,
};
pub use expr::{
    ExprError, ExprHost, OP_GETSPLASHDESCRIPTION, OP_GETSPLASHMATERIAL, OP_GETSPLASHTEXT,
    OP_INKILLCAM, OP_MENUISOPEN, OP_MILLISECONDS, OP_SCOREBOARD_VISIBLE, OP_SECONDSASCOUNTDOWN,
    OP_SPLASHHASICON, OP_SPLASHROWNUM, OP_TEAMFIELD, OP_UIACTIVE, Operand, PartyFlag, Statement,
    WeaponLockView, evaluate as evaluate_expression, evaluate_float, evaluate_string,
    is_expression_true, source_int, source_str,
};
pub use flashbang::{
    SCREEN_BLEND_BLURRED, SCREEN_BLEND_FLASHED, SCREEN_BLEND_NONE, ShellshockLookParms,
    ShellshockLookState, ShellshockSoundParms, ShockParams, is_flashbanged, shellshock_blend_time,
    shellshock_flash_blend, shellshock_flash_fade_sin_cos, shellshock_remaining_ms,
    shellshock_screen_alpha, update_shellshock_look_control,
};
pub use font::{
    G_COLOR_TABLE, HUDELEM_FONT_DEFAULT_BASE_SCALE, HUDELEM_FONT_HALF_BASE_SCALE,
    HUDELEM_FONT_THIRD_BASE_SCALE, R_TEXT_EM, color_from_caret_digit, hudelem_em_px,
    hudelem_font_base_scale, hudelem_font_ui_enum, hudelem_text_scale, item_get_text_placement_y,
    item_text_origin, next_letter, normalized_text_scale, seconds_to_countdown_display,
    ui_get_font_handle, ui_text_height,
};
pub use fov::{
    CG_FOV_DEFAULT, CG_FOV_MIN_DEFAULT, CG_FOV_SCALE_DEFAULT, CG_TAN_HALF_FOV_65,
    CG_TANHALF_FOV_Y_SCALE, EFLAGS_TURRET_FOV, FovInputs, LINK_FLAGS_FORCE_ADS_ZOOM_FOV,
    PM_TYPE_INTERMISSION, calc_fov, calc_fov_from_ads, fminf, horizontal_to_vertical_fov_deg,
    tan_half_fov, zoom_sensitivity,
};
pub use gamemsg::{
    CON_CHANNEL_GAMENOTIFY, CON_CHANNEL_OBITUARY, EMBED_HUD_ICON_SIZE_BIAS,
    EMBED_HUD_ICON_SIZE_CLAMP_MAX, EMBED_HUD_ICON_SIZE_CLAMP_MIN, EMBED_HUD_ICON_SIZE_SCALE,
    EXE_LEFTGAME, GAME_MSG_CHAR_EM, GAME_MSG_WIN0_FADE_IN_TIME_MS, GAME_MSG_WIN0_FADE_OUT_TIME_MS,
    GAME_MSG_WIN0_HORZ_ALIGN, GAME_MSG_WIN0_LINE_COUNT, GAME_MSG_WIN0_MODE,
    GAME_MSG_WIN0_MSG_TIME_MS, GAME_MSG_WIN0_SCROLL_TIME_MS, GAME_MSG_WIN0_TEXT_SCALE,
    GAME_MSG_WIN0_TEXT_STYLE, GAME_MSG_WIN0_VERT_ALIGN, GAME_MSG_WIN0_X, GAME_MSG_WIN0_Y,
    GAME_MSG_WINDOW_COUNT, HITLOC_HEAD, HITLOC_HELMET, HITLOC_NONE, ITEM_TYPE_GAME_MESSAGE_WINDOW,
    KILLICON_BASE_SIZE, KILLICON_CRUSH, KILLICON_DIED, KILLICON_FALLING, KILLICON_HEADSHOT,
    KILLICON_IMPACT, KILLICON_MELEE, KILLICON_SHORT_SIZE, KILLICON_SUICIDE, KILLICON_WIDE_SIZE,
    MOD_HEAD_SHOT, MOD_MELEE, MOD_SUICIDE, MP_CONNECTED, decode_hud_icon_size,
    embed_hud_icon_size_byte, game_msg_win0_char_height, game_msg_win0_line_y, gamenotify_line,
    killicon_em_size, killicon_stretch_uv, killicon_virtual_size, obituary_is_headshot,
    obituary_mod, obituary_mod_killicon, pack_obituary_event_parm,
};
pub use hudelem::{
    ALIGN_SCREEN_HORZ_SHIFT, GAME_HUDELEM_ARCHIVED, GAME_HUDELEM_CAPACITY, GAME_HUDELEM_STRIDE,
    GameHudElem, HE_TYPE_CLOCK_DOWN, HE_TYPE_CLOCK_UP, HE_TYPE_FREE, HE_TYPE_MATERIAL,
    HE_TYPE_PLAYERNAME, HE_TYPE_TENTHS_TIMER_DOWN, HE_TYPE_TENTHS_TIMER_STATIC,
    HE_TYPE_TENTHS_TIMER_UP, HE_TYPE_TEXT, HE_TYPE_TIMER_DOWN, HE_TYPE_TIMER_STATIC,
    HE_TYPE_TIMER_UP, HE_TYPE_VALUE, HE_TYPE_WAYPOINT, HORZ_ALIGN_CENTER,
    HUDELEM_ARCHIVAL_REMAPPED_TIMES, HUDELEM_BANK_CAPACITY, HUDELEM_STRIDE, HUDELEM_TYPE_NAMES,
    HudElem, HudElemPlacement, ORG_LEADING, ORG_MIDDLE, ORG_TRAILING, PLAYERSTATE_HUD_ARCHIVAL,
    PLAYERSTATE_HUD_BANKS_END, PLAYERSTATE_HUD_CURRENT, VERT_ALIGN_MIDDLE, WAYPOINT_CONSTANT_SIZE,
    WAYPOINT_HIDE_OFFSCREEN, WAYPOINT_PULSE_OFFSCREEN, align_org, align_screen, color_rgba,
    copy_in_use_prefix, flags, hud_elem_glow_color, hud_elem_lerp_font_scale,
    hud_elem_material_size, hud_elem_movement_frac, hud_elem_origin, hud_elem_placement,
    hud_elem_position, hud_elem_scale_frac, hud_elem_screen_align, lerp_hud_colors,
    rebase_archival_times, unpack_rgba,
};
pub use iris::{
    ADS_IRIS_ZOOM_ACTIVE_MIN, ADS_OVERLAY_FOUR_QUAD_LETTERBOX_SCALE, ADS_OVERLAY_ONE_QUAD_HALF,
    ADS_OVERLAY_ONE_QUAD_MIN_HEIGHT, ADS_OVERLAY_ONE_QUAD_MIN_WIDTH, AdsOverlayLayout,
    AdsOverlayMaterialSlot, AdsOverlayQuad, LOW_RES_VIEWPORT_MAX_HEIGHT,
    OTHER_FLAGS_EMP_OVERLAY_MATERIAL, ReticleDraw, ReticleZoom, WeaponAdsOverlayFacts,
    ads_overlay_material, ads_overlay_uses_four_quads, calc_ads_overlay_zoom,
    draw_ads_overlay_layout, draw_weap_reticle, draw_weap_reticle_hip_alpha, get_weap_reticle_zoom,
    iris_overlay_configured, viewweapon_drawgun, viewweapon_drawgun_skip,
};
pub use mantle_hint::{
    CG_OWNERDRAW_MANTLE, HINT_MANTLE_MATERIAL, MANTLE_HINT_FLAG, MantleHintLayout, PLATFORM_MANTLE,
    draw_mantle_hint_layout, draw_mantle_hint_visible, hint_replace_bind,
};
pub use menu_transition::{
    MENU_TRANSITION_LERP, MENU_TRANSITION_STRIDE, MenuAnim, MenuLerpFromScript, MenuTransition,
    item_run_script_lerp, item_text_paint_scale, window_paint_scale_rect,
};
pub use overhead_names::{
    CROSSHAIR_SCAN_DISTANCE, ENEMY_NAME_FADE_MS, FLASHBANG_NAME_FADE_IN_DEFAULT_MS,
    FLASHBANG_NAME_FADE_OUT_DEFAULT_MS, FRIENDLY_NAME_FADE_IN_DEFAULT_MS,
    FRIENDLY_NAME_FADE_OUT_DEFAULT_MS, OVERHEAD_FAR_DISTANCE_DEFAULT, OVERHEAD_FAR_SCALE_DEFAULT,
    OVERHEAD_HEAD_LIFT, OVERHEAD_ICON_SIZE_DEFAULT, OVERHEAD_MAX_DISTANCE_DEFAULT,
    OVERHEAD_NAME_SIZE_DEFAULT, OVERHEAD_NEAR_DISTANCE_DEFAULT, OVERHEAD_ORIGIN_FALLBACK_Z,
    OVERHEAD_RANK_SIZE_DEFAULT, OVERHEAD_TRACE_MASK, OverheadHeadResult, OverheadView,
    PartyRelation, RelativeTeamColorChoice, RelativeTeamColorKey, overhead_anchor,
    overhead_distance_scale, overhead_fade_alpha, relative_team_color_key,
    world_pos_to_overhead_pixel,
};
pub use perk::{
    PERK_SLOT_NAMES, PERKS_INFO_HD_MENU, SCRIPT_MENU_PERK_DISPLAY, SCRIPT_MENU_PERK_DISPLAY_NAME,
    SCRIPT_MENU_PERK_HIDE, SCRIPT_MENU_PERK_HIDE_NAME, SPECIALTY_NULL,
    WEAPON_NAME_FADE_DURATION_MS, WEAPON_NAME_FADE_TAIL_MS, WEAPONBAR_HD_MENU, get_perk_slot_index,
    perk_code_key,
};
pub use playercard::{
    PLAYER_CARD_SCRIPT_SLOT_COUNT, PLAYER_CARD_SLOT_KILLEDBY, PLAYER_CARD_SLOT_YOUKILLED,
    PLAYERCARD_INFO_AGE, PLAYERCARD_INFO_CLAN, PLAYERCARD_INFO_ICON, PLAYERCARD_INFO_NAME,
    PLAYERCARD_INFO_NAMEPLATE, PLAYERCARD_INFO_PRESTIGE, PLAYERCARD_INFO_RANK, PLAYERCARD_INFO_STR,
    PLAYERCARD_INFO_TEAM, PLAYERCARD_INFO_TITLE, PLAYERCARD_INFO_VALID, PLAYERCARD_KILLED_BY_MENU,
    PLAYERCARD_YOU_KILLED_MENU, PlayerCardData, SCRIPT_MENU_KILLEDBY_DISPLAY,
    SCRIPT_MENU_KILLEDBY_DISPLAY_NAME, SCRIPT_MENU_KILLEDBY_HIDE, SCRIPT_MENU_KILLEDBY_HIDE_NAME,
    SCRIPT_MENU_YOUKILLED_DISPLAY, SCRIPT_MENU_YOUKILLED_DISPLAY_NAME,
    player_cards_set_script_slot, script_menu_cs_index, script_menu_name,
    ui_run_op_get_player_card_info,
};
pub use render_commands::{
    GfxCmdView, GfxCmdWalk, GfxCmdWalkRefuse, GfxRenderCommandBuf, walk_render_commands,
};
pub use replace_directive::{
    KEY_UNBOUND, default_mp_key_binding, hudelem_default_text_scale, replace_directive,
    unbound_directive,
};
pub use scorebar::{
    SCOREBAR_BACKDROP, SCOREBAR_BACKDROP_IMAGE, SCOREBAR_BOTTOM_BG, SCOREBAR_BOTTOM_CAP,
    SCOREBAR_BOTTOM_FILL, SCOREBAR_BOTTOMBAR_BG_IMAGE, SCOREBAR_BOTTOMBAR_IMAGE,
    SCOREBAR_BOTTOMCAP_BG_IMAGE, SCOREBAR_BOTTOMCAP_IMAGE, SCOREBAR_CLOCK, SCOREBAR_COLOR_BACKDROP,
    SCOREBAR_COLOR_BOTTOM_FILL, SCOREBAR_COLOR_TOP_FILL, SCOREBAR_COLOR_TRACK,
    SCOREBAR_GAMETYPE_WINDOW_MS, SCOREBAR_LEAD_SCORE, SCOREBAR_LOC_GAMETYPE_DM,
    SCOREBAR_LOC_LOSING, SCOREBAR_LOC_TIED, SCOREBAR_LOC_WINNING, SCOREBAR_LOCAL_SCORE,
    SCOREBAR_SCORE_TEXTSCALE, SCOREBAR_STATUS, SCOREBAR_STATUS_CYCLE_MS, SCOREBAR_STATUS_TEXTSCALE,
    SCOREBAR_TOP_BG, SCOREBAR_TOP_CAP, SCOREBAR_TOP_FILL, SCOREBAR_TOPBAR_BG_IMAGE,
    SCOREBAR_TOPBAR_IMAGE, SCOREBAR_TOPCAP_BG_IMAGE, SCOREBAR_TOPCAP_IMAGE, ScorebarCycleSlot,
    ScorebarRect, ScorebarStatus, ffa_scorebar_lead, match_time_remaining_ms, mm_ss_nonneg,
    scorebar_clock_forecolor, scorebar_cycle_slot, scorebar_ffa_standing, scorebar_ffa_status,
    scorebar_fill_cap_x, scorebar_gametype_loc_key, scorebar_status_forecolor,
    scorebar_status_from_vis, scorebar_status_loc_key, scorebar_track_frac,
};
pub use scrplace::{
    ALIGN_CENTER, ALIGN_CENTER_SAFE, ALIGN_FULLSCREEN, ALIGN_NOSCALE, ALIGN_SUB, ALIGN_TO_VIRTUAL,
    ALIGN_USER_CENTER, ALIGN_USER_MAX, ALIGN_USER_MIN, ALIGN_VIEWABLE, ALIGN_VIEWABLE_MAX,
    AppliedRect, SAFE_AREA_ADJUSTED_DEFAULT, SAFE_AREA_DEFAULT, SCRPLACE_VIRTUAL_HEIGHT,
    SCRPLACE_VIRTUAL_WIDTH, ScreenPlacement,
};
pub use set_2d::{
    FLOAT64_ONE, GFX_SCENE_DEF_DWORDS, GFX_VIEW_MODE_2D, GFX_VIEW_MODE_3D, GFX_VIEW_MODE_NONE,
    GFX_VIEWPARMS_DWORDS, GFX_VIEWPARMS_INV_VP, GFX_VIEWPARMS_INV_VP_M33, GFX_VIEWPARMS_ORIGIN,
    GFX_VIEWPARMS_PROJECTION, GFX_VIEWPARMS_SIZE, GFX_VIEWPARMS_VIEW,
    GFX_VIEWPARMS_VIEW_PROJECTION, GFX_VIEWPORT_FULL, GfxBeginViewResult, GfxCmdBufSource2d,
    GfxNearPlaneConstants, GfxSet2dMatrices, GfxSet3dResult, GfxViewport, SET2D_M11_SCALE,
    SET2D_M30_BIAS, begin_view, cmd_buf_set_2d, cmd_buf_set_2d_projection, cmd_buf_set_3d,
    derive_near_plane_constants, get_viewport, gfx_scene_def_float_time, gfx_viewparms_origin,
    gfx_viewparms_write_matrix, gfx_viewparms_write_origin, set_2d, set_2d_clip_coeffs,
    set_2d_clip_xy, set_3d,
};
pub use splash::{
    SPLASH_COL_DESCRIPTION, SPLASH_COL_DURATION, SPLASH_COL_MATERIAL, SPLASH_COL_MENU,
    SPLASH_COL_SOUND, SPLASH_COL_TEXT, SPLASH_SLOT_COUNT, SPLASH_TABLE_NAME, SplashSlot,
    activate_splash, splash_duration_ms, splash_has_icon, splash_replace_optional,
};
pub use stretch_pic_cmd::{
    AddStretchPicCmd, COLOR_TO_BYTE_BIAS, COLOR_TO_BYTE_SCALE, GFX_CMD_DRAW_STRETCHPIC,
    GFX_CMD_DRAW_STRETCHPIC_SIZE, GFX_CMD_STRETCHPIC_COLOR, GFX_CMD_STRETCHPIC_H,
    GFX_CMD_STRETCHPIC_MATERIAL, GFX_CMD_STRETCHPIC_S0, GFX_CMD_STRETCHPIC_S1,
    GFX_CMD_STRETCHPIC_T0, GFX_CMD_STRETCHPIC_T1, GFX_CMD_STRETCHPIC_W, GFX_CMD_STRETCHPIC_X,
    GFX_CMD_STRETCHPIC_Y, GFX_RENDER_CMD_BUF_SIZE, GFX_RENDER_CMD_TAIL_RESERVE,
    GFX_TESS_2D_PACKED_NORMAL, GFX_TESS_VERTEX_STRIDE, GfxCmdStretchPic, GfxCmdStretchPicArgs,
    GfxTessVertex2d, RB_DRAW_STRETCHPIC_INDICES, TESS_STRETCHPIC_FLUSH_INDEX_PLUS_SIX,
    TESS_STRETCHPIC_FLUSH_VERT_PLUS_FOUR, add_cmd_draw_stretch_pic, convert_color_to_bytes,
    draw_stretch_pic_corners, draw_stretch_pic_pack, parse_gfx_cmd_stretch_pic, set_vertex_2d,
    tess_stretchpic_must_flush, unpack_color_bgra,
};
pub use text_fx::{
    DECODE_CHARACTERS_GLOW_MATERIAL, DECODE_CHARACTERS_MATERIAL, DecayingLetter,
    FX_DECAY_LETTER_FADE_MS, FX_DECAY_TICKS_PER_SECOND, FX_DECODE_RENDERFLAGS,
    FX_EXTRA_CHAR_ATLAS_STEP, FX_EXTRA_CHAR_LETTER, FX_RANDOM_CHARS, FX_TYPING_LETTER_ALPHA,
    HUDELEM_SOUND_SLOTS, PulseFxVars, TEXT_OUTLINE_OFFSETS, TEXT_RENDERFLAG_BIG_SHADOW,
    TEXT_RENDERFLAG_DROP_SHADOW, TEXT_RENDERFLAG_FX_DECODE, TEXT_RENDERFLAG_OUTLINE,
    TEXT_RENDERFLAG_OUTLINE_EXTRA, TEXT_RENDERFLAG_PADDING, TextPulseFx, TextPulseSound,
    decay_tick_count, decode_fx_char_st, font_get_random_letter, get_decaying_letter_info,
    modulate_byte_colors, play_text_fx_pulse_sounds, rand_with_seed, seh_print_strlen,
    setup_pulse_fx_vars, text_drop_shadow_offset, text_outline_size,
};
pub use view_projection::{
    R_INFINITE_PERSPECTIVE_K, R_SUBWINDOW_DEFAULT, R_SUBWINDOW_EDGE_EPS, R_ZNEAR_DEFAULT,
    R_ZNEAR_DEPTHHACK_DEFAULT, R_ZNEAR_FLOOR, compose_view_projection, depth_hack_near_clip,
    matrix_for_viewer, matrix_multiply44, set_view_parms_matrices, setup_finite_projection_matrix,
    setup_projection_matrix, subwindow_clamp, subwindow_is_full, subwindow_to_viewport,
    znear_from_refdef,
};
pub use vision_set::{
    R_GLOW_ALLOWED_DEFAULT, R_GLOW_ALLOWED_SCRIPT_FORCED_DEFAULT, VISION_DEF_FIELD_COUNT,
    VISION_DEF_FIELDS, VISION_GLOW_FADE_CUTOFF_MIN, VISION_GLOW_FADE_INTENSITY_MAX,
    VISION_HOLD_BLEND_RATE, VISION_HOLD_BLEND_RATE_NEG, VISION_SET_LERP_BACKFORTH_LINEAR,
    VISION_SET_LERP_BACKFORTH_SMOOTH, VISION_SET_LERP_HOLD, VISION_SET_LERP_NONE,
    VISION_SET_LERP_TO_LINEAR, VISION_SET_LERP_TO_SMOOTH, VISION_SET_VARS_SIZE, VisionDefField,
    VisionSetLerpData, VisionSetVars, vision_hold_blend_rate, vision_lerp_bool, vision_lerp_float,
    vision_lerp_vars, vision_lerp_vec3, vision_set_start, vision_sets_update,
};
