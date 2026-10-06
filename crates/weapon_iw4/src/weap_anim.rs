pub const WEAP_ANIM_RESTART_BIT: u32 = 0x200;

pub mod weap_anim_event {
    pub const IDLE: u32 = 0;
    pub const FIRE: u32 = 2;
    pub const LASTSHOT: u32 = 3;
    pub const RECHAMBER: u32 = 4;
    pub const ADS_FIRE: u32 = 5;
    pub const ADS_LASTSHOT: u32 = 6;
    pub const ADS_RECHAMBER: u32 = 7;
    pub const MELEE: u32 = 8;
    pub const MELEE_CHARGE: u32 = 9;
    pub const DROP: u32 = 0xa;
    pub const RAISE: u32 = 0xb;
    pub const FIRST_RAISE: u32 = 0xc;
    pub const RELOAD: u32 = 0xd;
    pub const RELOAD_EMPTY: u32 = 0xe;
    pub const RELOAD_START: u32 = 0xf;
    pub const RELOAD_END: u32 = 0x10;
    pub const QUICK_DROP: u32 = 0x13;
    pub const QUICK_RAISE: u32 = 0x14;
    pub const SPRINT_IN: u32 = 0x17;
    pub const SPRINT_LOOP: u32 = 0x18;
    pub const SPRINT_OUT: u32 = 0x19;
    pub const STUNNED_START: u32 = 0x1a;
    pub const STUNNED_LOOP: u32 = 0x1b;
    pub const STUNNED_END: u32 = 0x1c;
    pub const HOLD_FIRE: u32 = 0x1d;
    pub const DETONATE: u32 = 0x1e;
    pub const RELOAD_QUICK: u32 = 0x21;
    pub const RELOAD_QUICK_EMPTY: u32 = 0x22;
}

pub fn start_weapon_anim(weap_anim: &mut i32, event: u32) {
    let old = *weap_anim as u32;
    *weap_anim = ((!old & WEAP_ANIM_RESTART_BIT) | event) as i32;
}

pub fn set_weap_anim(
    weap_anim: &mut i32,
    weap_anim_secondary: &mut i32,
    last_weapon_hand: i32,
    event: u32,
) {
    start_weapon_anim(weap_anim, event);
    if last_weapon_hand == 1 {
        start_weapon_anim(weap_anim_secondary, event);
    } else {
        *weap_anim_secondary = 0;
    }
}

pub fn weapon_idle_weap_anim(weap_anim: &mut i32, pm_type: i32) {
    if pm_type < 8 {
        start_weapon_anim(weap_anim, weap_anim_event::IDLE);
    }
}

pub fn continue_weapon_anim(weap_anim: &mut i32, event: u32, pm_type: i32) -> bool {
    if pm_type >= 8 {
        return false;
    }
    let old = *weap_anim as u32;
    let masked_old = old & !WEAP_ANIM_RESTART_BIT;
    if masked_old == event {
        return false;
    }
    start_weapon_anim(weap_anim, event);
    true
}

pub fn set_fps_fire_anim(weap_anim: &mut i32, ads: bool, last_shot: bool) {
    let event = if ads {
        if last_shot {
            weap_anim_event::ADS_LASTSHOT
        } else {
            weap_anim_event::ADS_FIRE
        }
    } else if last_shot {
        weap_anim_event::LASTSHOT
    } else {
        weap_anim_event::FIRE
    };
    start_weapon_anim(weap_anim, event);
}

pub fn set_rechamber_anim(weap_anim: &mut i32, ads: bool) {
    let event = if ads {
        weap_anim_event::ADS_RECHAMBER
    } else {
        weap_anim_event::RECHAMBER
    };
    start_weapon_anim(weap_anim, event);
}
