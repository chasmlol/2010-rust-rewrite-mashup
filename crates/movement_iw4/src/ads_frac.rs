use playerstate_iw4::{PlayerState, pm_flags};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdsFracContext {
    pub aim_down_sight: bool,

    pub ads_reload_trans_time_ms: i32,
    pub segmented_reload: bool,

    pub ads_in_rate: f32,

    pub ads_out_rate: f32,

    pub rechamber_while_ads: bool,

    pub ads_fire_only: bool,
}

impl Default for AdsFracContext {
    fn default() -> Self {
        Self {
            aim_down_sight: true,

            ads_reload_trans_time_ms: 0,
            segmented_reload: false,
            ads_in_rate: 1.0 / 200.0,
            ads_out_rate: 1.0 / 200.0,
            rechamber_while_ads: true,
            ads_fire_only: false,
        }
    }
}

pub fn update_ads_frac(ps: &mut PlayerState, msec: i32, context: AdsFracContext) {
    if !context.aim_down_sight {
        ps.f_weapon_pos_frac = 0.0;
        ps.ads_delay_time = 0;
        return;
    }

    let ws = ps.weaponstate_primary;
    if matches!(ws, 0xd | 0xe | 0xf) {
        ps.f_weapon_pos_frac = 0.0;
        ps.ads_delay_time = 0;
        return;
    }

    let mut ads_requested = (ps.pm_flags & pm_flags::ADS_INTENT) != 0;
    let reload_blocks_ads = if context.segmented_reload {
        matches!(ws, 8..=11) || (ws == 12 && ps.weapon_time > context.ads_reload_trans_time_ms)
    } else {
        ws == 8 && ps.weapon_time > context.ads_reload_trans_time_ms
    };
    if reload_blocks_ads || (!context.rechamber_while_ads && ws == 7) {
        ads_requested = false;
    }

    if context.ads_fire_only
        && ((ps.weapon_delay != 0 && ws == 6)
            || (ps.weapon_delay_secondary != 0 && ps.weaponstate_secondary == 6))
    {
        ads_requested = true;
    }
    let mut dt = msec.max(0) as f32;
    if ps.perks[0] & playerstate_iw4::PERK_QUICKDRAW != 0 {
        dt *= playerstate_iw4::PERK_QUICKDRAW_SPEED_SCALE;
    }

    if ads_requested {
        if ps.f_weapon_pos_frac >= 1.0 {
            ps.f_weapon_pos_frac = 1.0;
            ps.ads_delay_time = 0;
            return;
        }
        let rate = context.ads_in_rate;
        if rate <= 0.0 {
            ps.f_weapon_pos_frac = 1.0;
        } else {
            ps.f_weapon_pos_frac = (ps.f_weapon_pos_frac + rate * dt).clamp(0.0, 1.0);
        }
        ps.ads_delay_time = 0;
    } else {
        if ps.f_weapon_pos_frac <= 0.0 {
            ps.f_weapon_pos_frac = 0.0;
            ps.ads_delay_time = 0;
            return;
        }

        let rate = context.ads_out_rate;
        if rate <= 0.0 {
            ps.f_weapon_pos_frac = 0.0;
        } else {
            ps.f_weapon_pos_frac = (ps.f_weapon_pos_frac - rate * dt).clamp(0.0, 1.0);
        }
        ps.ads_delay_time = 0;
    }
}
