//! Jak on foot: standing, walking, jumping and falling, the frame loop that
//! moves him, and the call for the board.
use glam::Vec3;

use crate::collide::CollideWorld;
use crate::control::{GROUND_TIMEOUT, status};
use crate::math::*;
use crate::pad::button;
use crate::surface::{self, Surface};
use crate::{Event, Jak, State};

pub const JUMP_HEIGHT_MIN: f32 = meters(1.01);
pub const JUMP_HEIGHT_MAX: f32 = meters(3.5);
pub const DOUBLE_JUMP_HEIGHT_MIN: f32 = meters(1.0);
pub const DOUBLE_JUMP_HEIGHT_MAX: f32 = meters(2.5);
const FALL_HEIGHT: f32 = meters(1.0);
const STUCK_TIME: i64 = seconds(0.3);
const STUCK_DISTANCE: f32 = meters(0.05);

impl Jak {
    pub(crate) fn on_foot_walking(&self) -> bool {
        self.state == State::Walk
    }

    fn walk_mods(&self) -> Surface {
        if self.gun.out {
            surface::GUN_WALK
        } else {
            surface::WALK
        }
    }

    pub(crate) fn foot_enter(&mut self, state: &State) {
        match *state {
            State::Stance | State::Walk => {
                self.control.mod_surface = self.walk_mods();
            }
            State::Jump { min, max } => {
                self.events.push(Event::Jump);
                self.init_var_jump(min, max, true, true, 2.0);
                let c = &mut self.control;
                c.status &= !(status::ON_SURFACE | status::ON_GROUND | status::TOUCH_SURFACE);
                c.mod_surface = surface::JUMP;
            }
            State::DoubleJump { min, max } => {
                self.events.push(Event::Jump);
                self.init_var_jump(min, max, true, true, 2.0);
                let c = &mut self.control;
                c.status &= !(status::ON_SURFACE | status::ON_GROUND | status::TOUCH_SURFACE);
                c.mod_surface = surface::DOUBLE_JUMP;
            }
            State::Falling => {
                self.control.mod_surface = surface::JUMP;
            }
            State::HitGround => {
                self.control.turn_go_the_long_way = 0.0;
                self.events.push(Event::Land);
                self.delete_back_vel();
                self.control.mod_surface = surface::WALK;
            }
            _ => {}
        }
    }

    pub(crate) fn foot_exit(&mut self, state: &State, _next: &State) {
        if *state == State::Stance {
            self.control.bend_target = 0.0;
        }
    }

    fn fall_test(&self) -> bool {
        let c = &self.control;
        !c.on_surface()
            && self.time_elapsed(c.last_time_on_surface, GROUND_TIMEOUT)
            && c.gravity_normal.dot(c.transv) <= 0.0
            && c.height_above_ground() >= FALL_HEIGHT
    }

    /// Landed, or stuck against something for long enough to count as it.
    fn falling_trans(&mut self, stuck_after: i64) -> Option<State> {
        if self.control.on_surface() {
            return Some(State::HitGround);
        }
        if stuck_after >= 0
            && self.control.move_dist(self.time, STUCK_TIME) < STUCK_DISTANCE
            && self.time_elapsed(self.state_time, stuck_after)
        {
            self.control.status |= status::ON_SURFACE;
            return Some(State::HitGround);
        }
        None
    }

    fn move_legs(&self) -> bool {
        self.pad.stick0_speed != 0.0
    }

    pub(crate) fn foot_trans(&mut self, world: &mut dyn CollideWorld) -> Option<State> {
        if self.want_to_board(world) {
            self.board_init();
            self.events.push(Event::BoardOn);
            return Some(State::BoardGetOn);
        }
        let jump = State::Jump {
            min: JUMP_HEIGHT_MIN,
            max: JUMP_HEIGHT_MAX,
        };
        match self.state {
            State::Stance | State::Walk | State::HitGround => {
                self.control.mod_surface = self.walk_mods();
                if self.pad.pressed(button::X) && self.can_jump(false) {
                    return Some(jump);
                }
                if self.state == State::Stance && self.move_legs() {
                    return Some(State::Walk);
                }
                if self.state == State::Walk && !self.move_legs() {
                    return Some(State::Stance);
                }
                if self.state == State::HitGround {
                    return Some(if self.move_legs() {
                        State::Walk
                    } else {
                        State::Stance
                    });
                }
                if self.fall_test() {
                    return Some(State::Falling);
                }
                None
            }
            State::Jump { .. } => {
                if let Some(next) = self.falling_trans(15) {
                    return Some(next);
                }
                let up = self.control.gravity_normal.dot(self.control.transv);
                if self.pad.pressed(button::X) && up < 12288.0 && -116736.0 < up {
                    return Some(State::DoubleJump {
                        min: DOUBLE_JUMP_HEIGHT_MIN,
                        max: DOUBLE_JUMP_HEIGHT_MAX,
                    });
                }
                self.control.jump_window =
                    self.control.jump_window.max(self.pad.pressure(button::X));
                self.mod_var_jump(self.pad.hold(button::X));
                None
            }
            State::DoubleJump { .. } => {
                if let Some(next) = self.falling_trans(-1) {
                    return Some(next);
                }
                self.mod_var_jump(self.pad.hold(button::X));
                None
            }
            State::Falling => self.falling_trans(0),
            _ => None,
        }
    }

    /// R2: once there is room above Jak and the gun has been put away, he
    /// calls the board.
    fn want_to_board(&mut self, world: &mut dyn CollideWorld) -> bool {
        if self.pad.pressed(button::R2) && self.room_above(world) {
            self.board.latch = true;
        }
        let c = &self.control;
        self.board.latch
            && (self.board.board_time == i64::MIN / 4
                || self.time_elapsed(self.board.board_time, seconds(0.5)))
            && self.board.board_time < c.list_time_on_ground
            && c.current.flags & surface::flag::DUCK == 0
            && self.gun.time_since_use(self.time) >= seconds(0.4)
    }

    /// Three spheres stacked above Jak's head, clear of the world.
    fn room_above(&mut self, world: &mut dyn CollideWorld) -> bool {
        let base = self.control.trans;
        let r = 2867.2;
        let lo = base + Vec3::new(-r, 8192.0 - r, -r);
        let hi = base + Vec3::new(r, 16384.0 + r, r);
        self.probe_cache.fill_box(world, lo, hi);
        [8192.0, 12288.0, 16384.0].iter().all(|&y| {
            !self
                .probe_cache
                .overlaps_sphere(base + Vec3::new(0.0, y, 0.0), r)
        })
    }

    /// One frame on foot.
    pub(crate) fn target_post(&mut self, world: &mut dyn CollideWorld) {
        if self.state == State::BoardGetOff {
            self.control.bend_speed = 0.0;
            self.control.bend_target = 0.0;
            self.control.draw_offset_y =
                seek(self.control.draw_offset_y, 0.0, 16384.0 * SECONDS_PER_FRAME);
        }
        self.flag_setup();
        if self.control.force_turn_to_strength < 0.0 {
            self.control.force_turn_to_strength = 1.0 - self.pad.stick0_speed;
        }
        self.build_conversions();
        self.do_rotations1();
        let pad_dir = self.read_pad();
        let speed = self.debounce_speed();
        self.turn_to_vector(pad_dir, speed);
        self.add_thrust();
        self.add_gravity();
        self.do_rotations2();
        self.reverse_conversions();
        self.pre_collide_setup();
        self.integrate_and_collide(world);
        self.bend_gravity();
        self.post_flag_setup();
        self.target_gspot(world);
    }

    /// While the gun is out: turning eases to the aim while Jak walks, and
    /// snaps round while he stands.
    pub(crate) fn gun_walk_hook(&mut self, cur: &mut Surface) {
        let to_target = self.control.to_target_pt_xz;
        let off = deg_diff(self.control.y_angle(), y_angle(to_target));
        let want = lerp_scale(0.0, 1.0, off.abs(), 1820.4445, 6371.5557);
        let g = &mut self.gun;
        if g.turn_blend < want {
            g.turn_blend = seek(g.turn_blend, want, 4.0 * SECONDS_PER_FRAME);
        } else {
            g.turn_blend = seek(g.turn_blend, want, SECONDS_PER_FRAME);
        }
        let mut turnv = 131072.0;
        let mut turnvf = 30.0;
        if 1.0 < g.turn_blend {
            turnv = lerp_scale(131072.0, 291271.12, g.turn_blend, 1.0, 2.0);
            turnvf = lerp_scale(30.0, 15.0, g.turn_blend, 1.0, 2.0);
            cur.turnvv = turnv;
            cur.turnvvf = turnvf;
        }
        cur.turnv = turnv;
        cur.turnvf = turnvf;
        if self.state == State::Stance {
            cur.turnv = 364088.88;
            cur.turnvf = 30.0;
        }
    }
}
