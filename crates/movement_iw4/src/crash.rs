use playerstate_iw4::{PlayerState, pm_flags};

use crate::{Pml, add_predictable_event, jump};

const EV_FOOTSTEP_RUN: i32 = 0x6c;
const EV_FOOTSTEP_WALK: i32 = 0x6d;

const EV_LANDING_FIRST: i32 = 0x70;

const EV_LANDING_PAIN_FIRST: i32 = 0x8f;

pub const FALL_DAMAGE_MIN_HEIGHT_IN: f32 = 128.0;

pub const FALL_DAMAGE_MAX_HEIGHT_IN: f32 = 300.0;

const FALL_LIGHT_IN: f32 = 4.0;

const FALL_MEDIUM_IN: f32 = 8.0;

const FALL_HARD_IN: f32 = 12.0;

const HARD_LAND_VEL_SCALE: f32 = 0.67;

const SURF_NODAMAGE: u32 = 0x1;

const SURF_SLICK: u32 = 0x2;

const SURF_SOFT_LANDING: u32 = 0x1000;

const PM_TYPE_DEAD: i32 = 8;

const HALF: f32 = 0.5;

const FOUR: f32 = 4.0;

const TWO: f32 = 2.0;

const NEG_ONE: f32 = -1.0;

pub fn crash_land(ps: &mut PlayerState, pml: &mut Pml) {
    let Some(fall_height) = crash_land_fall_height(ps, pml) else {
        return;
    };
    pml.landing_animation |= fall_height > FALL_HARD_IN;
    let surface_flags = pml.ground_trace[4];
    let surface = jump::ground_surface_type(surface_flags);
    let damage = if ps.pm_type < PM_TYPE_DEAD && surface_flags & SURF_NODAMAGE == 0 {
        fall_damage(fall_height, surface_flags & SURF_SOFT_LANDING != 0)
    } else {
        0
    };
    if damage > 0 {
        pml.fall_damage = damage;
        land_hurt(ps, damage, surface_flags);
        let soft = if surface_flags & SURF_SOFT_LANDING != 0 {
            0x80
        } else {
            0
        };
        add_predictable_event(ps, EV_LANDING_PAIN_FIRST + surface, damage | soft);
        return;
    }
    crash_land_apply_sfx(ps, fall_height, surface);
}

pub fn fall_damage(fall_height: f32, soft: bool) -> i32 {
    let max = if soft { 10.0 } else { 100.0 };
    if fall_height <= FALL_DAMAGE_MIN_HEIGHT_IN {
        return 0;
    }
    if fall_height >= FALL_DAMAGE_MAX_HEIGHT_IN {
        return max as i32;
    }
    ((fall_height - FALL_DAMAGE_MIN_HEIGHT_IN)
        / (FALL_DAMAGE_MAX_HEIGHT_IN - FALL_DAMAGE_MIN_HEIGHT_IN)
        * max) as i32
}

fn land_hurt(ps: &mut PlayerState, damage: i32, surface_flags: u32) {
    let scale = if damage < 100 && surface_flags & SURF_SLICK == 0 {
        let time = (damage * 35 + 500).min(2000);
        ps.pm_flags |= pm_flags::TIME_HARDLANDING;
        ps.pm_time = time;
        if time >= 1500 {
            0.2
        } else {
            0.5 - (time - 500) as f32 / 1000.0 * 0.3
        }
    } else {
        HARD_LAND_VEL_SCALE
    };
    ps.velocity[0] *= scale;
    ps.velocity[1] *= scale;
    ps.velocity[2] *= scale;
}

pub fn crash_land_fall_height(ps: &PlayerState, pml: &Pml) -> Option<f32> {
    if ps.gravity == 0 {
        return None;
    }
    let dist = pml.previous_origin[2] - ps.origin[2];
    let vel = pml.previous_velocity[2];
    let acc = -(ps.gravity as f32);
    let a = acc * HALF;
    let den = vel * vel - FOUR * a * dist;
    if den < 0.0 {
        return None;
    }
    let two_a = a * TWO;
    if two_a == 0.0 {
        return None;
    }
    let t = (-vel - libm::sqrtf(den)) / two_a;
    let land_vel = (t * acc + vel) * NEG_ONE;
    Some((land_vel * land_vel) / ((ps.gravity as f32) * TWO))
}

fn crash_land_apply_sfx(ps: &mut PlayerState, fall_height: f32, surface: i32) {
    if fall_height <= FALL_LIGHT_IN {
        return;
    }
    if fall_height < FALL_MEDIUM_IN {
        if surface != 0 {
            add_predictable_event(ps, EV_FOOTSTEP_WALK, surface);
        }
        return;
    }
    if fall_height < FALL_HARD_IN {
        if surface != 0 {
            add_predictable_event(ps, EV_FOOTSTEP_RUN, surface);
        }
        return;
    }
    ps.velocity[0] *= HARD_LAND_VEL_SCALE;
    ps.velocity[1] *= HARD_LAND_VEL_SCALE;
    ps.velocity[2] *= HARD_LAND_VEL_SCALE;
    if surface != 0 {
        add_predictable_event(ps, EV_LANDING_FIRST + surface, 0);
    }
}
