use playerstate_iw4::{PlayerState, pm_flags};

use crate::Pml;

#[allow(clippy::assign_op_pattern)]
pub fn friction(ps: &mut PlayerState, pml: &Pml) {
    let speed = velocity_speed(&ps.velocity);
    if stop_slow_velocity(&mut ps.velocity, speed) {
        return;
    }

    let flags = ps.pm_flags;
    let mut drop = 0.0_f32;
    if (flags & pm_flags::MELEE_CHARGE) == 0 {
        if pml.walking != 0 && (pml.ground_trace[4] & 2) == 0 && (flags & 0x100) == 0 {
            let mut control = speed;
            if speed < 100.0 {
                control = 100.0;
            }

            if (flags & pm_flags::TIME_HARDLANDING) != 0 {
                control *= 0.3;
            } else if (flags & pm_flags::JUMPING) != 0 {
                control *= walking_scale(ps);
            }

            drop = control * 5.5;
            drop = drop * pml.frametime + 0.0;
        }
    } else {
        drop = (speed / (ps.melee_charge_time as f32 * 0.001)) * pml.frametime;
    }

    if ps.pm_type == 5 {
        drop = speed * 5.0 * pml.frametime + drop;
    }

    let mut new_speed = speed - drop;
    if new_speed < 0.0 {
        new_speed = 0.0;
    }
    let scale = new_speed / speed;
    ps.velocity[0] = scale * ps.velocity[0];
    ps.velocity[1] = ps.velocity[1] * scale;
    ps.velocity[2] = scale * ps.velocity[2];
}

fn walking_scale(ps: &mut PlayerState) -> f32 {
    if ps.pm_time < 0x709 {
        if (ps.pm_flags & 0x400000) == 0 {
            if ps.pm_time > 0x6a3 {
                return 2.5;
            }

            return (ps.pm_time as f32 * 1.5 * 0.0005882352706976235) + 1.0;
        }
    } else {
        ps.pm_flags &= 0xffbfdfff;
        ps.jump_origin_z = 0.0;
    }
    1.0
}

fn velocity_speed(velocity: &[f32; 3]) -> f32 {
    let speed_squared =
        velocity[0] * velocity[0] + velocity[1] * velocity[1] + velocity[2] * velocity[2];
    libm::sqrtf(speed_squared)
}

fn stop_slow_velocity(velocity: &mut [f32; 3], speed: f32) -> bool {
    if speed < 1.0 {
        *velocity = [0.0, 0.0, 0.0];
        true
    } else {
        false
    }
}
