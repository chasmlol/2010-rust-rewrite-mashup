//! What the board stance animation is driven by: a sprung turn frame that
//! leans Jak into turns, a duck amount that crouches him on landings and
//! lifts him in the air, and a lean toward the slope under the board.
use crate::control::status;
use crate::math::*;
use crate::{Jak, State};

/// Frames either side of centre in the turn animations.
pub const TURN_FRAMES: f32 = 10.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoardAnim {
    pub turn_targ: f32,
    pub turn_frame: f32,
    pub turn_vel: f32,
    pub turn_mag: f32,
    pub duck: f32,
    pub duck_vel: f32,
    /// Lean toward the slope ahead (positive up) and to the side (positive
    /// left), each `[-1, 1]`.
    pub tween: [f32; 2],
}

impl BoardAnim {
    /// The turn animations' frame to show, `[-TURN_FRAMES, TURN_FRAMES]`,
    /// with the wobble that keeps a held turn alive.
    pub fn shown_frame(&self, since_state: i64) -> f32 {
        let wobble = sin(145.63556 * since_state as f32)
            * lerp_scale(1.0, 2.0, self.turn_frame.abs(), 0.0, TURN_FRAMES);
        (self
            .turn_frame
            .clamp(-(TURN_FRAMES - 1.0), TURN_FRAMES - 1.0)
            + wobble)
            .clamp(-TURN_FRAMES, TURN_FRAMES)
    }
}

impl Jak {
    /// Where the turn frame heads and how the duck amount moves, from how
    /// the board is turning.
    pub(crate) fn board_anim_trans(&mut self) {
        let dt = SECONDS_PER_FRAME;
        let c = &self.control;
        let wanted = deg_sub(
            quat_vector_y_angle(c.dir_targ, c.local_normal),
            quat_vector_y_angle(c.quat, c.local_normal),
        );
        let turning = deg_sub(
            quat_vector_y_angle(c.quat, c.local_normal),
            quat_vector_y_angle(c.last_quat, c.local_normal),
        ) * 60.0;
        let push = c.turn_to_magnitude;
        let a = &mut self.board.anim;
        let toward = lerp_scale(1.0, -1.0, turning, -32768.0, 32768.0);
        a.turn_mag = if toward.abs() < 0.1 || push < 0.3 {
            seek(a.turn_mag, 0.0, 8.0 * dt)
        } else {
            seek(a.turn_mag, toward, 2.0 * dt)
        };
        a.turn_targ = lerp_scale(
            TURN_FRAMES / 2.0,
            TURN_FRAMES / -2.0,
            wanted,
            -5461.3335,
            5461.3335,
        ) + lerp_scale(
            0.3 * TURN_FRAMES,
            -0.3 * TURN_FRAMES,
            turning,
            -32768.0,
            32768.0,
        ) + lerp_scale(TURN_FRAMES / 5.0, TURN_FRAMES / -5.0, a.turn_mag, 1.0, -1.0);
        a.duck_vel = a.duck_vel * 0.98 - 8.0 * dt;
        let on_ground = self.control.on_surface();
        if on_ground && self.time_elapsed(self.board.step_ease_time, seconds(0.2)) {
            let c = &self.control;
            let b = &mut self.board;
            if c.status & status::IMPACT_SURFACE != 0 {
                b.anim.duck_vel += lerp_scale(0.0, 15.0, c.normal_impact_vel, 0.0, 81920.0);
            }
            let level = b.up_vector[0].dot(b.up_vector[1]);
            if level < 1.0 {
                b.anim.duck_vel += lerp_scale(400.0, 0.0, level, 0.6, 1.0) * dt;
            }
        }
    }

    /// One frame of the sprung turn frame, the duck amount and the slope
    /// lean, as the stance animation advances them.
    pub(crate) fn board_turn_anim(&mut self) {
        let dt = SECONDS_PER_FRAME;
        let speed = self.control.ctrl_xz_vel;
        let near = 0.3 * self.board.transv_max;
        let since_surface = (self.time - self.control.last_time_on_surface) as f32;
        let on_surface = self.control.on_surface();
        let tilt = self.board.turn_anim_tilt;
        let halfpipe = self.control.ground_pat.mode == crate::collide::PatMode::Halfpipe;
        let slope = [
            if !tilt || halfpipe {
                0.0
            } else {
                1.6 * self.control.ctrl_slope_z
            },
            if tilt {
                1.6 * self.control.ctrl_slope_x
            } else {
                0.0
            },
        ];
        let a = &mut self.board.anim;
        a.turn_targ = a.turn_targ.clamp(-10.0, 10.0);
        let stiffness = lerp_scale(
            20.0,
            if a.turn_frame.abs() < a.turn_targ.abs() {
                30.0
            } else {
                60.0
            },
            speed,
            0.0,
            near,
        );
        a.turn_vel += (a.turn_targ - a.turn_frame) * stiffness * dt;
        a.turn_vel = (a.turn_vel * lerp_scale(0.96, 0.9, speed, 0.0, near)).clamp(-100.0, 100.0);
        a.turn_frame = (a.turn_frame + a.turn_vel * dt).clamp(-TURN_FRAMES, TURN_FRAMES);
        if (a.turn_frame >= TURN_FRAMES && a.turn_vel >= 0.0)
            || (-TURN_FRAMES >= a.turn_frame && 0.0 >= a.turn_vel)
        {
            a.turn_vel = 0.0;
        }

        let floor = if on_surface {
            seek(a.duck, 0.0, dt)
        } else {
            a.duck
        }
        .min(lerp_scale(0.0, -1.0, since_surface, 30.0, 120.0));
        a.duck += a.duck_vel * dt;
        if a.duck < floor {
            a.duck = floor;
            a.duck_vel = 0.0;
        } else if 1.0 < a.duck {
            a.duck = 1.0;
            a.duck_vel = 0.0;
        }

        for (blend, target) in a.tween.iter_mut().zip(slope) {
            let target = target.clamp(-1.0, 1.0);
            let rate = 0.1;
            let step = ((target - *blend).abs() * rate)
                .min(0.8 * rate)
                .max(rate / 5.0);
            *blend = seek(*blend, target, step);
        }
    }

    /// The stance states hand the animation over to the next state; leaving
    /// for anything but another stance recentres it.
    pub(crate) fn board_anim_exit(&mut self, next: &State) {
        if !matches!(
            next,
            State::BoardStance | State::BoardDuckStance | State::BoardTurnTo { .. }
        ) {
            let a = &mut self.board.anim;
            a.turn_frame = 0.0;
            a.turn_mag = 0.0;
            a.turn_vel = 0.0;
            a.duck = 0.0;
        }
    }

    /// Back on the board after the hop: the landing crouch.
    pub(crate) fn board_anim_land(&mut self) {
        self.board.anim.duck = 1.0;
        self.board.anim.duck_vel = 15.0;
    }
}
