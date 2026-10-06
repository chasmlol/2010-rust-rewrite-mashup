use playerstate_iw4::{
    BREATH_GASP_TIME_MS, PlayerState, breath_hold_time_ms, buttons, pm_flags, weap_flags,
};

pub(crate) fn update_hold_breath(ps: &mut PlayerState, buttons: u32, msec: i32, eligible: bool) {
    let was_holding = ps.weap_flags & weap_flags::HOLD_BREATH != 0;
    let wants_hold = eligible
        && ps.f_weapon_pos_frac == 1.0
        && buttons & buttons::BREATH != 0
        && ps.pm_flags & (pm_flags::SPRINTING | pm_flags::MANTLE | pm_flags::LADDER) == 0
        && ps.weap_flags & weap_flags::OFFHAND_VIEW == 0
        && ps.health > 0;
    let hold_time = breath_hold_time_ms(ps);
    let holding = wants_hold && (was_holding || ps.hold_breath_timer == 0);
    ps.weap_flags &= !weap_flags::HOLD_BREATH;
    if holding {
        ps.hold_breath_timer = ps.hold_breath_timer.saturating_add(msec);
        if ps.hold_breath_timer >= hold_time {
            ps.hold_breath_timer = hold_time + BREATH_GASP_TIME_MS;
        } else {
            ps.weap_flags |= weap_flags::HOLD_BREATH;
        }
    } else {
        ps.hold_breath_timer = ps.hold_breath_timer.saturating_sub(msec).max(0);
    }
    let holding = ps.weap_flags & weap_flags::HOLD_BREATH != 0;
    let target = if holding {
        0.0
    } else {
        1.0 + 3.5 * ps.hold_breath_timer as f32 / (hold_time + BREATH_GASP_TIME_MS) as f32
    };
    let target = 1.0 + (target - 1.0) * ps.f_weapon_pos_frac;
    let rate = if holding { 1.0 } else { 6.0 };
    ps.hold_breath_scale += (target - ps.hold_breath_scale) * (rate * msec as f32 * 0.001).min(1.0);
}
