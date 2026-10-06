extern crate alloc;

use alloc::string::String;

use crate::scrplace::{ALIGN_USER_MAX, ALIGN_USER_MIN};

pub const GAME_MSG_WINDOW_COUNT: usize = 4;

pub const GAME_MSG_WIN0_LINE_COUNT: usize = 4;

pub const GAME_MSG_WIN0_MSG_TIME_MS: i32 = 5000;
pub const GAME_MSG_WIN0_SCROLL_TIME_MS: i32 = 250;
pub const GAME_MSG_WIN0_FADE_IN_TIME_MS: i32 = 250;
pub const GAME_MSG_WIN0_FADE_OUT_TIME_MS: i32 = 500;

pub const GAME_MSG_WIN0_X: f32 = 6.0;

pub const GAME_MSG_WIN0_Y: f32 = -60.0;

pub const GAME_MSG_WIN0_HORZ_ALIGN: i32 = ALIGN_USER_MIN;

pub const GAME_MSG_WIN0_VERT_ALIGN: i32 = ALIGN_USER_MAX;

pub const GAME_MSG_WIN0_TEXT_SCALE: f32 = 0.375;

pub const GAME_MSG_WIN0_TEXT_STYLE: i32 = 3;

#[allow(dead_code)]
pub const GAME_MSG_WIN0_FONT_ENUM: i32 = 0;

#[allow(dead_code)]
pub const GAME_MSG_WIN0_TEXT_ALIGN: i32 = 0;

pub const GAME_MSG_WIN0_MODE: i32 = 1;

pub const KILLICON_BASE_SIZE: f32 = f32::from_bits(0x3fb33333);

pub const KILLICON_WIDE_SIZE: f32 = f32::from_bits(0x40333333);

pub const KILLICON_SHORT_SIZE: f32 = f32::from_bits(0x3f333333);

pub const GAME_MSG_CHAR_EM: f32 = 48.0;

pub const EMBED_HUD_ICON_SIZE_SCALE: f64 = f64::from_bits(0x4040_0000_0000_0000);

pub const EMBED_HUD_ICON_SIZE_BIAS: f64 = f64::from_bits(0x3e10_0000_0000_0000);

pub const EMBED_HUD_ICON_SIZE_CLAMP_MIN: i32 = 0x10;

pub const EMBED_HUD_ICON_SIZE_CLAMP_MAX: i32 = 0x7f;

pub const KILLICON_DIED: &str = "killicondied";

pub const KILLICON_MELEE: &str = "killiconmelee";

pub const KILLICON_HEADSHOT: &str = "killiconheadshot";

pub const KILLICON_CRUSH: &str = "killiconcrush";

pub const KILLICON_FALLING: &str = "killiconfalling";

pub const KILLICON_SUICIDE: &str = "killiconsuicide";

pub const KILLICON_IMPACT: &str = "killiconimpact";

pub const HITLOC_NONE: u8 = 0;
pub const HITLOC_HELMET: u8 = 1;
pub const HITLOC_HEAD: u8 = 2;

pub const MOD_MELEE: i32 = 8;

pub const MOD_HEAD_SHOT: i32 = 9;

pub const MOD_SUICIDE: i32 = 0xc;

pub const CON_CHANNEL_OBITUARY: i32 = 5;

pub const CON_CHANNEL_GAMENOTIFY: i32 = 2;

pub const EXE_LEFTGAME: &str = "EXE_LEFTGAME";

pub const MP_CONNECTED: &str = "MP_CONNECTED";

pub const ITEM_TYPE_GAME_MESSAGE_WINDOW: i32 = 0x13;

pub fn game_msg_win0_char_height() -> f32 {
    GAME_MSG_WIN0_TEXT_SCALE * GAME_MSG_CHAR_EM
}

pub fn killicon_em_size(kill_icon_ratio: i32) -> (f32, f32) {
    let mut width = KILLICON_BASE_SIZE;
    let mut height = KILLICON_BASE_SIZE;
    if kill_icon_ratio != 0 {
        width = KILLICON_WIDE_SIZE;
        if kill_icon_ratio != 1 {
            height = KILLICON_SHORT_SIZE;
        }
    }
    (width, height)
}

pub fn killicon_virtual_size(kill_icon_ratio: i32) -> (f32, f32) {
    let (width_em, height_em) = killicon_em_size(kill_icon_ratio);
    (
        decode_hud_icon_size(embed_hud_icon_size_byte(width_em)),
        decode_hud_icon_size(embed_hud_icon_size_byte(height_em)),
    )
}

pub fn embed_hud_icon_size_byte(em: f32) -> u8 {
    let rounded =
        libm::round(f64::from(em) * EMBED_HUD_ICON_SIZE_SCALE + EMBED_HUD_ICON_SIZE_BIAS) as i32;
    let clamped = rounded.clamp(EMBED_HUD_ICON_SIZE_CLAMP_MIN, EMBED_HUD_ICON_SIZE_CLAMP_MAX);
    u8::try_from(clamped + EMBED_HUD_ICON_SIZE_CLAMP_MIN).unwrap_or(u8::MAX)
}

pub fn decode_hud_icon_size(stored: u8) -> f32 {
    let pixel_height = GAME_MSG_CHAR_EM as i32;
    let inner = (pixel_height * (i32::from(stored) - EMBED_HUD_ICON_SIZE_CLAMP_MIN) + 16) / 32;
    inner as f32 * GAME_MSG_WIN0_TEXT_SCALE
}

pub fn killicon_stretch_uv(flip_kill_icon: bool) -> (f32, f32) {
    if flip_kill_icon {
        (1.0, 0.0)
    } else {
        (0.0, 1.0)
    }
}

pub fn obituary_mod(event_parm: i32) -> Option<i32> {
    (event_parm < 0).then(|| -1 - event_parm)
}

pub fn obituary_mod_killicon(means_of_death: i32) -> Option<&'static str> {
    match means_of_death {
        8 => Some(KILLICON_MELEE),
        9 => Some(KILLICON_HEADSHOT),
        10 => Some(KILLICON_CRUSH),
        0xb => Some(KILLICON_FALLING),
        0xc => Some(KILLICON_SUICIDE),
        0xd => Some(KILLICON_DIED),
        0xf => Some(KILLICON_IMPACT),
        _ => None,
    }
}

pub fn obituary_is_headshot(hitloc: u8) -> bool {
    hitloc == HITLOC_HELMET || hitloc == HITLOC_HEAD
}

#[must_use]
pub fn gamenotify_line(template: &str, name: &str) -> String {
    if template.contains("&&") {
        return crate::centerprint_replace_name(template, name);
    }
    if name.is_empty() {
        return String::from(template);
    }
    let mut out = String::from(name);
    out.push_str("^7 ");
    out.push_str(template);
    out
}

pub fn pack_obituary_event_parm(
    weapon: u32,
    means_of_death: i32,
    weap_type: i32,
    weap_class: i32,
) -> i32 {
    let pack_mod = match means_of_death {
        0xf => weap_class != 9,
        8 => weap_type != 3,
        9 | 10 | 0xb | 0xc => true,
        _ => false,
    };
    if pack_mod {
        -1 - means_of_death
    } else {
        weapon as i32
    }
}

pub fn game_msg_win0_line_y(line_from_newest: usize) -> f32 {
    GAME_MSG_WIN0_Y - game_msg_win0_char_height() * (line_from_newest as f32 + 1.0)
}
