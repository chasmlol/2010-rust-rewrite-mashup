use crate::melee::weapon_settle_ready;
use crate::tick::{WeaponCmd, WeaponCombatFacts, WeaponHandState, WeaponTickEvent};
use crate::weaponstate::WeaponState;
use playerstate_iw4::pm_flags;

pub const PMF_CHANGE_BLOCK: u32 = 0xc00;

pub fn check_for_change_admits(weaponstate: i32, weapon_time: i32, weapon_delay: i32) -> bool {
    let Ok(ws) = WeaponState::from_i32(weaponstate) else {
        return false;
    };
    if matches!(
        ws,
        WeaponState::MeleeInit
            | WeaponState::MeleeFire
            | WeaponState::MeleeEnd
            | WeaponState::OffhandInit
            | WeaponState::OffhandPrepare
            | WeaponState::OffhandHold
            | WeaponState::OffhandStart
            | WeaponState::Offhand
            | WeaponState::OffhandEnd
            | WeaponState::NightVisionWear
            | WeaponState::NightVisionRemove
    ) {
        return false;
    }
    if weapon_time == 0 {
        return true;
    }
    if matches!(
        ws,
        WeaponState::Reloading
            | WeaponState::ReloadStart
            | WeaponState::ReloadEnd
            | WeaponState::ReloadStartInterrupt
            | WeaponState::ReloadingInterrupt
            | WeaponState::Rechambering
    ) {
        return true;
    }

    ws != WeaponState::Firing && weapon_delay == 0
}

#[inline]
pub fn traversal_forces_holster(cmd: &WeaponCmd) -> bool {
    cmd.mantle_weapon_inactive || (cmd.pm_flags & pm_flags::LADDER) != 0
}

fn is_dropping(ws: i32) -> bool {
    matches!(
        WeaponState::from_i32(ws),
        Ok(WeaponState::Dropping)
            | Ok(WeaponState::DroppingQuick)
            | Ok(WeaponState::DroppingAltswitch)
    )
}

pub fn weapon_check_for_change(
    hand: &mut WeaponHandState,
    facts: &WeaponCombatFacts,
    cmd: &mut WeaponCmd,
) -> Option<WeaponTickEvent> {
    if !check_for_change_admits(hand.weaponstate, hand.weapon_time, hand.weapon_delay) {
        return None;
    }

    if traversal_forces_holster(cmd) {
        if hand.weapon != 0 {
            return begin_weapon_change(hand, facts, 0, true, cmd.pm_flags);
        }
        return None;
    }

    if is_dropping(hand.weaponstate) {
        if u32::from(cmd.cmd_weapon) == hand.weapon {
            weapon_settle_ready(hand, &mut cmd.weap_flags, &mut cmd.pm_flags, cmd.pm_type);
            crate::weap_anim::start_weapon_anim(&mut hand.weap_anim, 1);
        }
        return None;
    }

    let cmd_w = u32::from(cmd.cmd_weapon);

    if hand.weapon != cmd_w {
        let blocked = (cmd.pm_flags & PMF_CHANGE_BLOCK) != 0 && hand.weapon != 0;
        if blocked {
            return None;
        }
        if cmd_w == 0 || cmd.cmd_weapon_owned {
            let quick = cmd.mantle_quick_raise || cmd.cmd_weapon_pistol_quick;
            let event = begin_weapon_change(hand, facts, cmd_w, quick, cmd.pm_flags);
            if cmd.alternate_switch && event.is_some() && cmd.pm_flags & pm_flags::SPRINTING == 0 {
                hand.weaponstate = WeaponState::DroppingAltswitch as i32;
                hand.weapon_time = facts.alternate_drop_time_ms;
                crate::weap_anim::start_weapon_anim(&mut hand.weap_anim, 0x11);
                return Some(WeaponTickEvent::AlternateStarted);
            }
            return event;
        }
        return None;
    }

    None
}

pub fn begin_weapon_change(
    hand: &mut WeaponHandState,
    facts: &WeaponCombatFacts,
    new_weapon: u32,
    quick: bool,
    pm_flags: u32,
) -> Option<WeaponTickEvent> {
    if is_dropping(hand.weaponstate) {
        return None;
    }
    hand.weapon_delay = 0;
    hand.weapon_restrict_kick_time = 0;
    hand.shot_count = 0;
    hand.burst_latch = false;
    hand.rechamber_pending = false;

    if hand.weapon == 0 {
        hand.weaponstate = if quick {
            WeaponState::DroppingQuick as i32
        } else {
            WeaponState::Dropping as i32
        };
        hand.weapon_time = 0;
        return None;
    }

    let quick = quick || new_weapon == 0;
    hand.weaponstate = if quick {
        WeaponState::DroppingQuick as i32
    } else {
        WeaponState::Dropping as i32
    };
    hand.weapon_time = if quick {
        facts.quick_drop_time_ms
    } else {
        facts.drop_time_ms.max(1)
    };

    if pm_flags & pm_flags::SPRINTING == 0 {
        crate::weap_anim::start_weapon_anim(
            &mut hand.weap_anim,
            if quick {
                crate::weap_anim::weap_anim_event::QUICK_DROP
            } else {
                crate::weap_anim::weap_anim_event::DROP
            },
        );
    }
    let _ = new_weapon;
    Some(WeaponTickEvent::PutawayStarted)
}

fn raise_time_for_cmd(cmd: &WeaponCmd, quick: bool) -> i32 {
    if quick && cmd.switch_quick_raise_time_ms > 0 {
        cmd.switch_quick_raise_time_ms
    } else if cmd.switch_raise_time_ms > 0 {
        cmd.switch_raise_time_ms
    } else {
        1
    }
}

pub fn finish_putaway_while_holstered(hand: &mut WeaponHandState) {
    hand.weapon = 0;
    hand.weaponstate = WeaponState::Ready as i32;
    hand.weapon_time = 0;
    hand.weapon_delay = 0;
    hand.shot_count = 0;
    hand.burst_latch = false;
    hand.rechamber_pending = false;
    crate::weap_anim::start_weapon_anim(&mut hand.weap_anim, 0);
}

pub fn finish_putaway_to_cmd(hand: &mut WeaponHandState, cmd: &WeaponCmd) {
    let new_weapon = u32::from(cmd.cmd_weapon);
    if new_weapon == 0 {
        finish_putaway_while_holstered(hand);
        return;
    }

    let quick = hand.weaponstate == WeaponState::DroppingQuick as i32;
    let alternate = hand.weaponstate == WeaponState::DroppingAltswitch as i32;
    hand.weapon = new_weapon;
    hand.weapon_delay = 0;
    hand.shot_count = 0;
    hand.burst_latch = false;
    hand.rechamber_pending = false;
    hand.weaponstate = if alternate {
        WeaponState::RaisingAltswitch
    } else {
        WeaponState::Raising
    } as i32;
    let first = !alternate && cmd.switch_first_raise_time_ms > 0;
    hand.weapon_time = if alternate {
        cmd.switch_alternate_raise_time_ms
    } else if first {
        cmd.switch_first_raise_time_ms
    } else {
        raise_time_for_cmd(cmd, quick)
    };
    crate::weap_anim::start_weapon_anim(
        &mut hand.weap_anim,
        if alternate {
            0x12
        } else if first {
            crate::weap_anim::weap_anim_event::FIRST_RAISE
        } else if quick {
            crate::weap_anim::weap_anim_event::QUICK_RAISE
        } else {
            crate::weap_anim::weap_anim_event::RAISE
        },
    );
}
