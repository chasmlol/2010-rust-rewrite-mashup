//! The JET-Board: its tuning, the per-frame board loop (thrust, the hover
//! suspension, glancing off walls, the step-up probe), spins and the charged
//! jump, and its states.
use glam::Vec3;

use crate::collide::{CollideWorld, PatMode, TriHit};
use crate::control::{self, GROUND_TIMEOUT, status};
use crate::math::*;
use crate::pad::button;
use crate::surface::{self, Hook, Surface, flag};
use crate::{Event, Jak, State, Trick};

pub const JUMP_HEIGHT_MIN: f32 = meters(1.01);
pub const JUMP_HEIGHT_MAX: f32 = meters(3.5);
pub const DUCK_JUMP_HEIGHT_MIN: f32 = meters(2.5);
pub const DUCK_JUMP_HEIGHT_MAX: f32 = meters(5.0);
pub const CUSHION: f32 = meters(1.0);
pub const CHARGE_JUMP_TIME: i64 = seconds(0.7);
pub const CHARGE_JUMP_FADE_TIME: i64 = seconds(0.25);
pub const CHARGE_JUMP_HEIGHT: f32 = meters(3.5);
pub const ZAP_DURATION: i64 = seconds(0.25);
pub const ZAP_RESET_TIME: i64 = seconds(0.25);
/// The zap's attack sphere: its height above Jak's origin and radius.
pub const ZAP_OFFSET_Y: f32 = 6553.6;
pub const ZAP_RADIUS: f32 = 12288.0;
/// The spin's attack sphere radius, at the same height.
pub const SPIN_RADIUS: f32 = 6963.2;
/// The highest speed any board mode allows.
pub const MAX_SPEED: f32 = 409600.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Danger {
    Spin,
    Zap,
}

#[derive(Clone, Debug)]
pub struct BoardInfo {
    pub latch: bool,
    /// What the stance animation is driven by.
    pub anim: crate::board_anim::BoardAnim,
    pub stick_off: bool,
    pub thrust_scale: f32,
    pub transv_max: f32,
    pub slip_factor: f32,
    pub mods_backup: Surface,
    pub board_time: i64,
    pub board_get_on_time: i64,
    pub in_air_time: i64,
    pub last_jump_time: i64,
    pub jump_end_time: i64,
    /// When the board last eased off a step it rode up.
    pub step_ease_time: i64,
    pub jump_land_time: i64,
    pub on_flat_time: i64,
    pub ride_time: i64,
    pub halfpipe_time: i64,
    pub slow_transv: Vec3,
    pub up_vector: [Vec3; 2],
    pub probe: Option<TriHit>,

    pub cushion_base: f32,
    pub cushion_offset: f32,
    pub shock_offset: f32,
    pub shock_offsetv: f32,

    pub smack_surface_time: i64,
    pub smack_speed: f32,
    pub smack_normal: Vec3,
    pub glance_time: i64,
    pub glance_speed: f32,
    pub glance_normal: Vec3,
    pub glance_in_transv: Vec3,
    pub glance_out_transv: Vec3,

    pub spin_check_time: i64,
    pub spin_time: i64,
    pub spin_start_time: i64,
    pub spin_ground_start_time: i64,
    pub spin_ground_time: i64,
    pub spin_ground_press_time: i64,
    pub spin_control: f32,
    pub flip_control: f32,
    pub flip_count: i32,
    pub trick_x: f32,
    pub trick_z: f32,
    pub spinning: bool,
    pub rotyv_max: f32,
    pub rotyv: f32,
    pub roty: f32,
    pub roty_cum: f32,

    pub duck_start_time: i64,
    pub l2_start_time: i64,
    pub tricky_exit_time: i64,
    pub hold_exit_time: i64,
    pub charge_start_time: i64,
    pub charge_time: i64,
    pub charge_progress: f32,
    pub zap_start_time: i64,
    pub danger: Option<Danger>,
    pub turn_anim_tilt: bool,
    pub tricks: Vec<(Trick, f32)>,
}

const LONG_AGO: i64 = i64::MIN / 4;

impl Default for BoardInfo {
    fn default() -> Self {
        Self {
            latch: false,
            anim: Default::default(),
            stick_off: false,
            thrust_scale: 1.0,
            transv_max: 0.0,
            slip_factor: 1.0,
            mods_backup: surface::board::WALK,
            board_time: LONG_AGO,
            board_get_on_time: LONG_AGO,
            in_air_time: LONG_AGO,
            last_jump_time: LONG_AGO,
            jump_end_time: LONG_AGO,
            step_ease_time: LONG_AGO,
            jump_land_time: LONG_AGO,
            on_flat_time: LONG_AGO,
            ride_time: LONG_AGO,
            halfpipe_time: LONG_AGO,
            slow_transv: Vec3::ZERO,
            up_vector: [Vec3::Y; 2],
            probe: None,
            cushion_base: 0.0,
            cushion_offset: 0.0,
            shock_offset: 0.0,
            shock_offsetv: 0.0,
            smack_surface_time: LONG_AGO,
            smack_speed: 0.0,
            smack_normal: Vec3::Y,
            glance_time: LONG_AGO,
            glance_speed: 0.0,
            glance_normal: Vec3::Y,
            glance_in_transv: Vec3::ZERO,
            glance_out_transv: Vec3::ZERO,
            spin_check_time: LONG_AGO,
            spin_time: LONG_AGO,
            spin_start_time: LONG_AGO,
            spin_ground_start_time: LONG_AGO,
            spin_ground_time: LONG_AGO,
            spin_ground_press_time: LONG_AGO,
            spin_control: 0.0,
            flip_control: 0.0,
            flip_count: 0,
            trick_x: 0.0,
            trick_z: 0.0,
            spinning: false,
            rotyv_max: 0.0,
            rotyv: 0.0,
            roty: 0.0,
            roty_cum: 0.0,
            duck_start_time: LONG_AGO,
            l2_start_time: LONG_AGO,
            tricky_exit_time: LONG_AGO,
            hold_exit_time: LONG_AGO,
            charge_start_time: LONG_AGO,
            charge_time: LONG_AGO,
            charge_progress: 0.0,
            zap_start_time: LONG_AGO,
            danger: None,
            turn_anim_tilt: false,
            tricks: Vec::new(),
        }
    }
}

impl Jak {
    pub fn on_board(&self) -> bool {
        self.state.is_board()
    }

    fn board_on_ground(&self) -> bool {
        self.control.on_surface()
    }

    /// The per-frame rewrite of the multiplied tuning that depends on what
    /// Jak is doing.
    pub(crate) fn apply_mult_hook(&mut self, cur: &mut Surface) {
        let now = self.time;
        let mods = self.control.mod_surface;
        match cur.hook {
            Hook::None => {}
            Hook::BoardWalk => {
                if 0.9 < self.control.surface_angle {
                    self.board.on_flat_time = now;
                }
                if MAX_SPEED < self.control.ctrl_xz_vel {
                    cur.transv_max = lerp(self.control.ctrl_xz_vel, MAX_SPEED, 0.2);
                } else {
                    cur.transv_max = cur.transv_max.min(MAX_SPEED);
                }
                self.board.slip_factor = lerp_scale(
                    1.0,
                    cur.slip_factor,
                    self.control.ctrl_slope_heading.abs(),
                    0.0,
                    1.0,
                );
                cur.slip_factor = self.board.slip_factor;
                cur.slope_up_factor = mods.slope_up_factor;
                cur.slope_down_factor = mods.slope_down_factor;
                let since_spin = (now - self.board.spin_time) as f32;
                cur.seek0 = lerp_scale(cur.seek0 / 10.0, cur.seek0, since_spin, 0.0, 600.0);
                cur.seek90 = lerp_scale(cur.seek90 / 10.0, cur.seek90, since_spin, 0.0, 600.0);
                cur.vel_turn = lerp_scale(131072.0, mods.vel_turn, since_spin, 0.0, 600.0);
                cur.turnv = lerp_scale(91022.22, mods.turnv, since_spin, 0.0, 600.0);
                if !self.time_elapsed(self.board.spin_ground_start_time, seconds(0.3)) {
                    self.board.spin_ground_time = now;
                    cur.seek0 /= 10.0;
                    cur.seek90 /= 10.0;
                    cur.vel_turn = 131072.0;
                    cur.turnv = 91022.22;
                }
            }
            Hook::BoardAir => {}
            Hook::BoardRideJump => {
                if self.pad.stick0_speed != 0.0 {
                    if MAX_SPEED < self.control.ctrl_xz_vel {
                        cur.transv_max = lerp(self.control.ctrl_xz_vel, MAX_SPEED, 0.2);
                    } else {
                        cur.transv_max = cur.transv_max.min(MAX_SPEED);
                    }
                }
            }
            Hook::BoardWallKick => {
                if !self.time_elapsed(self.state_time, seconds(0.05)) {
                    cur.turnv = 0.0;
                    cur.turnvf = 0.0;
                }
            }
            Hook::BoardRide => {
                cur.slope_up_factor = mods.slope_up_factor;
                cur.slope_down_factor = mods.slope_down_factor;
                if MAX_SPEED < self.control.ctrl_xz_vel {
                    cur.transv_max = lerp(self.control.ctrl_xz_vel, MAX_SPEED, 0.2);
                } else {
                    cur.transv_max = cur.transv_max.min(MAX_SPEED);
                }
            }
            Hook::GunWalk => self.gun_walk_hook(cur),
        }
    }

    /// Getting on: the gun goes away and the board's tuning, gravity and
    /// state are reset.
    pub(crate) fn board_init(&mut self) {
        self.gun.end_mode();
        let now = self.time;
        let b = &mut self.board;
        b.latch = false;
        b.board_get_on_time = now;
        b.stick_off = false;
        b.slip_factor = 1.0;
        b.probe = None;
        b.spin_control = 0.0;
        b.rotyv = 0.0;
        b.roty = 0.0;
        b.roty_cum = 0.0;
        b.flip_control = 0.0;
        b.flip_count = 0;
        b.trick_x = 0.0;
        b.trick_z = 0.0;
        b.tricks.clear();
        b.charge_progress = 0.0;
        b.thrust_scale = 1.0;
        b.cushion_base = 0.0;
        b.cushion_offset = 0.0;
        b.shock_offset = 0.0;
        b.shock_offsetv = 0.0;
        b.spinning = false;
        let c = &mut self.control;
        c.bend_target = 1.0;
        c.gravity_max = control::STANDARD_GRAVITY_MAX;
        c.gravity_length = control::STANDARD_GRAVITY;
    }

    /// Leaving the board for good, unless the next state is another board
    /// state.
    pub(crate) fn board_exit(&mut self, next: &State) {
        if next.is_board() {
            return;
        }
        self.board.latch = false;
        self.board.spinning = false;
        self.board.danger = None;
        let c = &mut self.control;
        c.gravity_max = control::STANDARD_GRAVITY_MAX;
        c.gravity_length = control::STANDARD_GRAVITY;
        c.no_normal_reset = false;
        c.mod_surface = surface::WALK;
        c.draw_offset_y = 0.0;
        self.events.push(Event::BoardOff);
    }

    pub(crate) fn add_trick(&mut self, trick: Trick, points: f32) {
        if self.board.tricks.len() < 16 {
            self.board.tricks.push((trick, points));
        }
        self.events.push(Event::Trick { trick, points });
    }

    fn flush_tricks(&mut self) {
        self.board.tricks.clear();
    }

    fn resolve_points(&mut self) {
        if self.board.spinning {
            self.add_spin_points();
        }
        self.board.roty_cum = 0.0;
        self.board.flip_count = 0;
        if self.time_elapsed(self.board.in_air_time, seconds(0.2)) {
            self.flush_tricks();
        }
    }

    fn add_spin_points(&mut self) {
        let turned = self.board.roty_cum.abs();
        if turned >= 191146.67 {
            self.add_trick(Trick::Spin, 4000.0);
        } else if turned >= 126520.89 {
            self.add_trick(Trick::Spin, 2000.0);
        } else if turned >= 49152.0 {
            self.add_trick(Trick::Spin, 500.0);
        }
    }

    /// Hitting a wall square-on at speed: remembered for the wall kick.
    pub(crate) fn board_smack_surface(&mut self) -> bool {
        let c = &self.control;
        if c.status & status::TOUCH_WALL != 0
            && self.time + seconds(-0.05) < c.last_time_touching_actor
            && 0.7 < c.touch_angle
            && 73728.0 < c.ctrl_xz_vel
            && c.wall_contact_normal.dot(c.gravity_normal) < 0.3
            && c.status & status::TOUCH_ACTOR == 0
        {
            self.board.smack_surface_time = self.time;
            self.board.smack_speed = self.control.ctrl_xz_vel;
            self.board.smack_normal = self.control.wall_contact_normal;
            true
        } else {
            false
        }
    }

    fn board_add_thrust(&mut self) {
        let now = self.time;
        let air = self.control.mod_surface.flags & flag::AIR != 0;
        let halfpipe = self.control.ground_pat.mode == PatMode::Halfpipe;
        let b = &mut self.board;
        let c = &mut self.control;
        let mut v = c.local_transv;
        let xz = xz_length(v);
        let heading = if xz == 0.0 { 0.0 } else { v.z / xz };
        let seek_rate = if heading >= 0.0 {
            heading * c.current.seek0 + (1.0 - heading) * c.current.seek90
        } else {
            heading.abs() * c.current.seek180 + (1.0 + heading) * c.current.seek90
        };
        let push = if b.stick_off {
            0.0
        } else if halfpipe {
            1.0
        } else if v.z < 0.0 {
            c.turn_to_magnitude.max(0.75)
        } else {
            c.turn_to_magnitude
        };
        if !(c.current.vel_turn == 0.0 || v.z < 0.0) {
            let angle = atan(v.x, v.z);
            v = rotate_y(
                v,
                (0.03 * -angle).min(c.current.vel_turn * SECONDS_PER_FRAME),
            );
        }
        let mut thrust = seek_rate * b.thrust_scale * lerp_scale(0.4, 1.0, push, 0.3, 1.0);
        let mut target = c.current.target_speed
            + if c.local_slope_z < 0.0 {
                -c.local_slope_z * c.current.slope_down_factor
            } else {
                -c.local_slope_z * c.current.slope_up_factor
            };
        if b.stick_off {
            thrust = 0.0;
            target = 0.0;
        }
        if now - c.last_time_touching_actor < seconds(1.0)
            && push >= 0.5
            && c.wall_contact_normal.dot(c.to_target_pt_xz) < -0.7
            && air
            && 0.0 < v.y
        {
            if now - c.last_time_touching_actor >= seconds(0.1) && 0.3 < c.blocked_factor {
                thrust = target;
            } else if v.z < 0.0 {
                v.z = 0.0;
            }
        }
        v.z += thrust * SECONDS_PER_FRAME;
        if target != 0.0 {
            let drag = 1.0 - (seek_rate * SECONDS_PER_FRAME) / target;
            v.x *= drag;
            v.z *= drag;
        }
        b.transv_max = target;
        if xz_length(v) >= c.current.transv_max {
            v = xz_normalize(v, c.current.transv_max);
        }
        c.local_transv = v;
    }

    /// The hover suspension: how far the board sits above its collision,
    /// sprung by what the collision did to the vertical speed.
    fn board_physics(&mut self, before: Vec3) {
        let k = 0.5;
        let on_ground = self.board_on_ground();
        let shock_air = !self.time_elapsed(self.control.last_time_on_surface, seconds(0.2));
        let eased = self.time_elapsed(self.board.step_ease_time, seconds(0.3));
        let c = &self.control;
        let b = &mut self.board;
        if eased {
            b.shock_offsetv += (before.y - c.transv.y) * k;
        } else {
            b.up_vector[1] = b.up_vector[0];
        }
        b.shock_offsetv += -10.0 * SECONDS_PER_FRAME * b.shock_offset
            + if on_ground {
                c.gravity_length * SECONDS_PER_FRAME * k
            } else {
                0.0
            };
        if c.mod_surface.mode == surface::Mode::Ride {
            b.shock_offsetv = 0.0;
            b.shock_offset *= 0.96;
        } else if (shock_air || b.shock_offset < 0.0) && c.mod_surface.mode != surface::Mode::Air {
            b.shock_offset += b.shock_offsetv * SECONDS_PER_FRAME;
        }
        b.shock_offset *= lerp_scale(0.99, 0.98, c.ctrl_xz_vel, 0.0, 28672.0);
        if 40960.0 < b.shock_offset {
            b.shock_offset = 40960.0;
            b.shock_offsetv = 0.0;
        } else if b.shock_offset < -3072.0 {
            if b.shock_offsetv < -12288.0 {
                let pitch = lerp_scale(1.0, 0.3, b.shock_offsetv, -40960.0, -12288.0);
                self.events.push(Event::BoardBounce { pitch });
            }
            let b = &mut self.board;
            b.shock_offset = -3072.0;
            b.shock_offsetv = 0.0;
        }
        let b = &self.board;
        self.control.draw_offset_y = (b.cushion_base + b.cushion_offset + b.shock_offset).max(0.0);
    }

    fn board_pre_move(&mut self) {
        let now = self.time;
        let state = self.state.kind();
        let air = self.control.mod_surface.flags & flag::AIR != 0 && !self.control.on_surface();
        if air {
            self.board.in_air_time = now;
        }
        let c = &self.control;
        let grounded_states = matches!(
            state,
            StateKind::BoardStance | StateKind::BoardDuckStance | StateKind::BoardTurnTo
        );
        let trick_state = state == StateKind::BoardTrick;
        let float_free = air
            && !grounded_states
            && ((((20480.0 < c.gravity_normal.dot(c.trans - c.gspot_pos))
                || 0.0 < c.gravity_normal.dot(c.transv))
                && c.gspot_slope_z < 0.0)
                || !(c.gspot_pat.mode == PatMode::Ground || c.gspot_pat.mode == PatMode::Halfpipe)
                || c.gspot_pat.event == crate::collide::PatEvent::Rail
                || trick_state)
            && self.time_elapsed(self.board.last_jump_time, seconds(0.2));
        if float_free {
            let c = &mut self.control;
            c.bend_speed = 0.0;
            c.bend_target = 0.0;
        } else {
            let young_jump = !self.time_elapsed(self.board.last_jump_time, seconds(0.1));
            let c = &mut self.control;
            c.bend_speed = 1024.0;
            c.bend_target = 1.0;
            if young_jump {
                c.dir_targ = forward_up_nopitch_quat(z_axis(c.dir_targ), c.gspot_normal);
                c.quat = c.quat.slerp(c.dir_targ, 0.1);
            }
        }
        let since_surface = now - self.control.last_time_on_surface;
        let c = &mut self.control;
        match state {
            StateKind::BoardGetOn | StateKind::BoardGetOff => {}
            StateKind::BoardTrick => {
                c.gravity_length = seek(c.gravity_length, 245760.0, 30.0 * SECONDS_PER_FRAME);
            }
            _ if c.mod_surface.name == surface::Name::Spin => {
                c.gravity_length =
                    lerp_scale(245760.0, 204800.0, self.board.rotyv.abs(), 0.0, 145635.56);
            }
            StateKind::BoardStance | StateKind::BoardDuckStance | StateKind::BoardTurnTo => {
                if since_surface >= seconds(0.1) {
                    c.gravity_length =
                        seek(c.gravity_length, 245760.0, 245760.0 * SECONDS_PER_FRAME);
                } else {
                    c.gravity_length = 81920.0;
                }
            }
            _ => c.gravity_length = 245760.0,
        }
        if c.force_turn_to_strength < 0.0 {
            c.force_turn_to_strength = 1.0 - self.pad.stick0_speed;
        }
        let Some(probe) = self.board.probe else {
            return;
        };
        let c = &self.control;
        let g = c.gravity_normal;
        let above_trans = g.dot(probe.intersect - c.trans);
        let above_gspot = g.dot(probe.intersect - c.gspot_pos);
        let vertical = g.dot(c.transv);
        let slope_rise = c.local_slope_z * flatten(c.trans - probe.intersect, Vec3::Y).length();
        let b = &self.board;
        if probe.pat.mode == PatMode::Ground
            && self.time_elapsed(b.step_ease_time, seconds(0.2))
            && 819.2 < above_trans - slope_rise
            && (b.jump_end_time < c.last_time_on_surface
                || !self.time_elapsed(b.jump_end_time, seconds(0.2)))
            && 0.98 < probe.normal.dot(Vec3::Y)
            && above_gspot < 8192.0
            && above_trans < 8192.0
            && (vertical / 5.0 < above_trans || !self.time_elapsed(b.last_jump_time, seconds(0.1)))
        {
            let lift = 7.0 * above_trans.min(4096.0);
            self.control.transv += g * lift;
            if self.time_elapsed(self.board.last_jump_time, seconds(0.1)) {
                self.board.jump_end_time = now;
            }
            self.board.last_jump_time = now;
        }
        let c = &self.control;
        let b = &self.board;
        if !c.on_surface()
            && 0.0 < vertical
            && (((c.height_above_ground() < 4096.0
                || !self.time_elapsed(b.last_jump_time, seconds(0.1)))
                && 8192.0 < vertical - g.dot(b.slow_transv))
                || !self.time_elapsed(b.step_ease_time, seconds(0.1)))
            && 204.8 >= above_trans
            && state != StateKind::BoardJump
        {
            self.board.step_ease_time = now;
            let c = &mut self.control;
            let up = g.dot(c.transv);
            c.transv = with_vertical(c.transv, g, 0.9 * up);
            self.board.slow_transv = self.control.transv;
        }
    }

    fn board_collision(&mut self, world: &mut dyn CollideWorld) {
        let before = self.control.transv;
        self.integrate_and_collide(world);
        self.board_physics(before);
        let speed = xz_length(before);
        let c = &self.control;
        if c.status & status::TOUCH_WALL != 0
            && 16384.0 < speed
            && c.ground_pat.mode != PatMode::Halfpipe
            && self.time_elapsed(self.board.halfpipe_time, seconds(0.1))
        {
            let n = c.wall_contact_normal.normalize_or_zero();
            let mut out = reflect(before, n);
            let mut facing = c.local_to_world.f;
            out.y = 0.0;
            out = out.normalize_or_zero();
            facing.y = 0.0;
            facing = facing.normalize_or_zero();
            let now_local = c.local_to_world.to_local(c.transv);
            let mut out_local = c.local_to_world.to_local(out);
            let glance_speed = xz_length(now_local);
            out_local.y = now_local.y;
            out_local = xz_normalize(out_local, glance_speed);
            let transv = c.local_to_world.to_world(out_local);
            let c = &mut self.control;
            c.status |= status::GLANCE;
            c.transv = transv;
            c.dir_targ = forward_up_nopitch_quat(transv.normalize_or_zero(), y_axis(c.dir_targ));
            let b = &mut self.board;
            b.glance_time = self.time;
            b.glance_speed = glance_speed;
            b.glance_normal = n;
            b.glance_in_transv = before;
            b.glance_out_transv = transv;
            if facing.dot(n) < -0.77 {
                let airborne =
                    self.control.mod_surface.flags & flag::AIR != 0 && !self.control.on_surface();
                if !airborne {
                    if 32768.0 < speed {
                        self.events.push(Event::BoardGlance);
                    }
                    self.pending = Some(State::BoardTurnTo {
                        dir: transv,
                        duration: seconds(0.2),
                    });
                } else {
                    let c = &mut self.control;
                    c.turn_lockout_end_time =
                        (self.time + seconds(0.1)).max(c.turn_lockout_end_time);
                }
            }
        }
        let c = &self.control;
        let g = c.gravity_normal;
        let start = c.trans + c.local_to_world.f * 10240.0 + g * 8192.0;
        let motion = g * -49152.0;
        self.board.probe = crate::collide::fill_and_probe_line_sphere(
            world,
            &mut self.probe_cache,
            start,
            motion,
            1638.4,
        );
        if self.control.move_dist(self.time, seconds(0.3)) < meters(0.05) {
            self.control.last_time_of_stuck = self.time;
        }
    }

    /// One frame of riding.
    pub(crate) fn board_post(&mut self, world: &mut dyn CollideWorld) {
        let now = self.time;
        self.flag_setup();
        self.board_pre_move();
        self.build_conversions();
        self.do_rotations1();
        let pad_dir = self.read_pad();
        if self.board.stick_off {
            self.pad.stick0_speed = 0.0;
            self.control.pad_magnitude = 0.0;
        }
        self.turn_to_vector(pad_dir, self.control.pad_magnitude);
        let free_turn = now >= self.control.turn_lockout_end_time
            && self.control.current.flags & flag::TURN_TO_VEL == 0
            && self.time_elapsed(self.board.last_jump_time, seconds(0.1));
        if self.control.pad_magnitude == 0.0 && free_turn {
            self.control.dir_targ = self.control.quat;
        }
        if !self.time_elapsed(self.control.last_time_on_surface, seconds(0.1)) && free_turn {
            let c = &mut self.control;
            let down = flatten(-c.gravity_normal, c.local_normal);
            let toward = forward_up_nopitch_quat(down, y_axis(c.dir_targ));
            c.dir_targ = c.dir_targ.slerp(toward, 0.05 * c.ctrl_slope_heading.abs());
        }
        if self.pad.pressed(button::L1) {
            self.board.duck_start_time = now;
        }
        if self.pad.pressed(button::L2) {
            self.board.l2_start_time = now;
        }
        self.board_add_thrust();
        self.add_gravity();
        self.board_spin_physics();
        self.board_charge();
        self.board_zap();
        self.do_rotations2();
        self.reverse_conversions();
        self.pre_collide_setup();
        self.board_cushion();
        self.board_collision(world);
        if self.control.bend_speed != 0.0 {
            self.bend_gravity();
        }
        self.post_flag_setup();
        self.target_gspot(world);
        self.board.board_time = now;
        if let Some(next) = self.board_exit_check() {
            self.pending = Some(next);
        }
        if self.board_on_ground() {
            let b = &mut self.board;
            b.up_vector[1] = b.up_vector[0];
            b.up_vector[0] = self.control.local_normal;
        }
    }

    fn board_cushion(&mut self) {
        let state = self.state.kind();
        let airborne =
            self.control.mod_surface.flags & flag::AIR != 0 && !self.control.on_surface();
        let height = self.control.height_above_ground();
        let b = &mut self.board;
        if state == StateKind::BoardGetOff {
            b.cushion_offset = seek(b.cushion_offset, 0.0, 20480.0 * SECONDS_PER_FRAME);
        } else if airborne {
            b.cushion_offset = seek(
                b.cushion_offset,
                lerp_scale(CUSHION, 0.0, height, 0.0, 12288.0),
                20480.0 * SECONDS_PER_FRAME,
            );
        } else {
            b.cushion_offset = seek(b.cushion_offset, CUSHION, 8192.0 * SECONDS_PER_FRAME);
        }
    }

    fn board_spin_physics(&mut self) {
        let dt = SECONDS_PER_FRAME;
        if self.control.mod_surface.name == surface::Name::Spin {
            let b = &mut self.board;
            b.rotyv = 0.95 * b.rotyv.clamp(-b.rotyv_max, b.rotyv_max);
            let step = b.rotyv * dt;
            b.roty = wrap_angle(b.roty + step);
            b.roty_cum += step;
            b.rotyv_max = seek(b.rotyv_max, 91022.22, 91022.22 * dt);
            b.spinning = true;
            return;
        }
        if self.board.spinning {
            self.add_spin_points();
            self.board.spinning = false;
            let in_trick = matches!(self.state.kind(), StateKind::BoardFlip);
            let c = &mut self.control;
            let facing = rotate_y(
                flatten(z_axis(c.quat), Vec3::Y).normalize_or(Vec3::Z),
                self.board.roty,
            );
            if !in_trick {
                c.quat = forward_up_nopitch_quat(facing, y_axis(c.quat));
            }
            c.dir_targ = forward_up_nopitch_quat(c.transv.normalize_or_zero(), y_axis(c.dir_targ));
            self.board.roty = 0.0;
            self.board.rotyv = 0.0;
        } else if self.board.danger == Some(Danger::Spin)
            && (self.board_on_ground() || self.time_elapsed(self.board.spin_time, seconds(0.5)))
        {
            self.board.danger = None;
        }
        if self.board_on_ground() {
            let b = &mut self.board;
            b.spin_control = 0.0;
            b.flip_control = 0.0;
            b.trick_x = 0.0;
            b.trick_z = 0.0;
        }
    }

    fn board_charge(&mut self) {
        let now = self.time;
        let on_ground = self.board_on_ground();
        let b = &mut self.board;
        if self.pad.pressed(button::L1) {
            b.charge_start_time = now;
        } else if self.pad.hold(button::L1) {
            if on_ground {
                b.charge_time = now;
                b.charge_progress = seek(
                    b.charge_progress,
                    1.0,
                    (300.0 / CHARGE_JUMP_TIME as f32) * SECONDS_PER_FRAME,
                );
            }
        } else if b.charge_progress != 0.0 {
            b.charge_progress = seek(
                b.charge_progress,
                0.0,
                (300.0 / CHARGE_JUMP_FADE_TIME as f32) * SECONDS_PER_FRAME,
            );
        }
    }

    fn board_zap(&mut self) {
        let now = self.time;
        let mounting = matches!(
            self.state.kind(),
            StateKind::BoardGetOn | StateKind::BoardGetOff
        );
        if self.pad.pressed(button::CIRCLE)
            && self.time_elapsed(self.board.zap_start_time, ZAP_DURATION + ZAP_RESET_TIME)
            && self.board.danger.is_none()
            && !mounting
        {
            self.board.danger = Some(Danger::Zap);
            self.board.zap_start_time = now;
            self.events.push(Event::BoardZap {
                center: self.control.trans + Vec3::new(0.0, ZAP_OFFSET_Y, 0.0),
                radius: ZAP_RADIUS,
            });
        } else if self.board.danger == Some(Danger::Zap)
            && self.time_elapsed(self.board.zap_start_time, ZAP_DURATION)
        {
            self.board.danger = None;
        }
    }

    /// R2 while riding: off the board, once it has been ridden a second and
    /// touched the ground since mounting.
    fn board_exit_check(&mut self) -> Option<State> {
        if !self.pad.pressed(button::R2) {
            return None;
        }
        let c = &self.control;
        if self.time_elapsed(self.board.board_get_on_time, seconds(1.0))
            && self.board.board_get_on_time < c.list_time_on_ground.max(c.last_time_of_stuck)
            && !matches!(self.state.kind(), StateKind::BoardGetOff)
        {
            Some(State::BoardGetOff)
        } else {
            None
        }
    }

    /// Landing out of a spin of at least two thirds of a turn: a speed boost
    /// along the way the board is travelling.
    fn board_ground_check(&mut self) {
        if !self.board_on_ground() {
            self.board.turn_anim_tilt = true;
            return;
        }
        if self.board.roty_cum != 0.0 {
            let turned = self.board.roty_cum.abs();
            if turned >= 41870.223 {
                if turned >= 191146.67 {
                    self.add_trick(Trick::Boost, 2000.0);
                } else if turned >= 126520.89 {
                    self.add_trick(Trick::Boost, 1000.0);
                } else {
                    self.add_trick(Trick::Boost, 500.0);
                }
                let c = &mut self.control;
                let boost = lerp_scale(20480.0, 40960.0, turned, 49152.0, 182044.44);
                c.transv += c.transv.normalize_or_zero() * boost;
                if c.transv.length() < 114688.0 {
                    c.transv = normalize(c.transv, 114688.0);
                }
                if c.old_status & status::ON_SURFACE == 0 {
                    self.events.push(Event::BoardBoost);
                }
            }
        }
        self.resolve_points();
        self.board.turn_anim_tilt = true;
    }

    /// R1 in the air spins the board with the stick; R1 with the stick
    /// pushed forward or back flips. Returns the flip when one starts.
    fn board_spin_check(&mut self) -> Option<State> {
        let now = self.time;
        if self.pad.pressed(button::R1)
            || (self.pad.hold(button::R1)
                && self.time_elapsed(self.board.spin_check_time, seconds(0.3)))
        {
            let c = &mut self.control;
            self.board.spin_start_time = now;
            c.turn_to_alt_heading = flatten(c.local_to_world.f, Vec3::Y).normalize_or_zero();
            c.dir_targ = c.quat;
            let since_surface = (now - c.last_time_on_surface) as f32;
            self.board.rotyv_max = lerp_scale(218453.33, 91022.22, since_surface, 0.0, 300.0);
            self.board.rotyv = 0.0;
            self.board.roty = 0.0;
            self.board.roty_cum = 0.0;
            self.control.mod_surface = surface::board::SPIN;
        }
        let mut next = None;
        if self.pad.hold(button::R1)
            && (self.control.mod_surface.name == surface::Name::Spin
                || !self.time_elapsed(self.board.spin_time, seconds(0.05)))
        {
            self.board.turn_anim_tilt = false;
            self.control.mod_surface = surface::board::SPIN;
            self.board.spin_time = now;
            let (x, z) = self.pad.left_trick_axes();
            let spin = analog_input((128.0 * x) as i32, 0.0, 64.0, 110.0, 2184533.2);
            let flip = analog_input((128.0 * z) as i32, 0.0, 96.0, 110.0, 1.0);
            let b = &mut self.board;
            if 0.9 * b.spin_control.abs() < spin.abs() && b.spin_control * spin >= 0.0 {
                b.spin_control = spin;
            }
            if self.board.danger.is_none() {
                self.board.danger = Some(Danger::Spin);
            }
            let b = &mut self.board;
            b.rotyv += b.spin_control * SECONDS_PER_FRAME;
            let c = &self.control;
            if spin.abs() < 1092266.6
                && b.spin_control.abs() < 1092266.6
                && 0.9 * b.flip_control.abs() < flip.abs()
                && b.flip_control * flip >= 0.0
                && !c.on_surface()
                && 0.0 < c.gravity_normal.dot(c.transv)
                && c.height_above_ground() >= 4096.0
            {
                b.flip_control = flip;
                next = Some(State::BoardFlip);
            }
        } else {
            self.control.mod_surface = self.board.mods_backup;
        }
        self.board.spin_check_time = now;
        if next.is_some() {
            return next;
        }
        self.board_trick_check()
    }

    /// L1 or L2 in the air: the board goes slack and, with the stick, a grab
    /// or flip trick hops Jak a little higher.
    fn board_trick_check(&mut self) -> Option<State> {
        let now = self.time;
        let since_surface = self.control.last_time_on_surface;
        let airborne_up = !self.board_on_ground()
            && 0.0 < self.control.gravity_normal.dot(self.control.transv)
            && self.control.height_above_ground() >= 4096.0
            && seconds(0.165) < self.time_to_ground();
        let (x, z) = self.pad.left_trick_axes();
        let side = analog_input((128.0 * x) as i32, 0.0, 64.0, 110.0, 1.0);
        let push = analog_input((128.0 * z) as i32, 0.0, 96.0, 110.0, 1.0);
        if self.pad.pressed(button::L1) || since_surface < self.board.duck_start_time {
            self.board.turn_anim_tilt = false;
            self.control.mod_surface = surface::board::SPIN;
            let b = &self.board;
            if side.abs() < 0.5
                && b.trick_x.abs() < 0.5
                && b.trick_z == 0.0
                && 0.9 * b.trick_z.abs() < push.abs()
                && b.trick_z * push >= 0.0
                && airborne_up
                && self.time_elapsed(b.tricky_exit_time, seconds(0.05))
                && b.tricky_exit_time < b.duck_start_time
            {
                self.board.trick_z = push;
                return Some(State::BoardTrick(BoardTrick::Grab));
            }
            let b = &self.board;
            if push.abs() < 0.5
                && b.trick_z.abs() < 0.5
                && b.trick_x == 0.0
                && 0.9 * b.trick_x.abs() < side.abs()
                && b.trick_x * side >= 0.0
                && airborne_up
                && self.time_elapsed(b.tricky_exit_time, seconds(0.05))
            {
                self.board.trick_x = side;
                return Some(State::BoardTrick(BoardTrick::Kick));
            }
        } else if self.pad.pressed(button::L2) || since_surface < self.board.l2_start_time {
            self.board.turn_anim_tilt = false;
            self.control.mod_surface = surface::board::SPIN;
            let b = &self.board;
            if side.abs() < 0.5
                && b.trick_x.abs() < 0.5
                && b.trick_z == 0.0
                && 0.9 * b.trick_z.abs() < push.abs()
                && b.trick_z * push >= 0.0
                && airborne_up
                && self.time_elapsed(b.hold_exit_time, seconds(0.05))
                && b.hold_exit_time < b.l2_start_time
            {
                self.board.trick_z = push;
                return Some(State::BoardTrick(BoardTrick::Hold));
            }
            let b = &self.board;
            if push.abs() < 0.5
                && b.trick_z.abs() < 0.5
                && b.trick_x == 0.0
                && 0.9 * b.trick_x.abs() < side.abs()
                && b.trick_x * side >= 0.0
                && airborne_up
                && self.time_elapsed(b.hold_exit_time, seconds(0.05))
            {
                self.board.trick_x = side;
                return Some(State::BoardTrick(BoardTrick::Hold));
            }
        }
        let _ = now;
        None
    }

    /// In the air on the board: landing, kicking off walls, spins.
    fn board_jump_trans(&mut self) -> Option<State> {
        if self.time != self.state_time && self.hit_ground_or_stuck() {
            self.board.jump_land_time = self.time;
            return Some(State::BoardHitGround);
        }
        if self.time_elapsed(self.state_time, seconds(0.1)) {
            self.board_smack_surface();
        }
        self.board_spin_check()
    }

    pub(crate) fn board_enter(&mut self, state: &State) {
        let now = self.time;
        match *state {
            State::BoardGetOn => {
                self.board.shock_offsetv = 0.0;
                let c = &mut self.control;
                c.status &= !(status::ON_SURFACE | status::ON_GROUND | status::TOUCH_SURFACE);
                c.mod_surface = surface::board::JUMP;
                self.board.mods_backup = c.mod_surface;
                let to_ground = self.time_to_ground();
                let c = &mut self.control;
                let g = c.gravity_normal;
                let up = g.dot(c.transv);
                let lift = if to_ground < seconds(0.25) {
                    up.clamp(0.0, 40960.0)
                        + 0.0016666667 * (seconds(0.66) - to_ground) as f32 * c.gravity_length
                } else {
                    up.clamp(0.0, 40960.0)
                };
                c.transv = with_vertical(c.transv, g, lift);
                let up = g.dot(c.transv);
                let mut flat = flatten(c.transv, g);
                if 81920.0 < flat.length() {
                    flat = normalize(flat, 81920.0);
                }
                c.transv = flat + g * up;
                self.board.slow_transv = self.control.transv;
            }
            State::BoardStance => {
                self.control.mod_surface = surface::board::WALK;
                self.board.mods_backup = surface::board::WALK;
            }
            State::BoardDuckStance => {
                self.control.mod_surface = surface::board::DUCK;
                self.board.mods_backup = surface::board::DUCK;
            }
            State::BoardJump { min, max, duck } => {
                let mut mods = if duck {
                    surface::board::DUCK_JUMP
                } else {
                    surface::board::JUMP
                };
                if !self.time_elapsed(self.board.ride_time, seconds(0.5)) {
                    mods = surface::board::RIDE_JUMP;
                }
                self.control.gravity_length = 245760.0;
                let mut extra = 0.0;
                if self.pad.hold(button::L1) && self.board.charge_progress != 0.0 {
                    extra += self.board.charge_progress * CHARGE_JUMP_HEIGHT;
                    self.events.push(Event::BoardLaunch);
                }
                self.board.charge_progress = 0.0;
                if 0.0 < self.board.shock_offsetv
                    && !self.time_elapsed(self.board.jump_land_time, seconds(0.5))
                    && self.time_elapsed(self.board.ride_time, seconds(0.5))
                {
                    extra += 8192.0;
                    self.add_trick(Trick::QuickJump, 0.0);
                } else if duck {
                    self.add_trick(Trick::DuckJump, 0.0);
                } else {
                    self.add_trick(Trick::Jump, 0.0);
                }
                self.events.push(Event::BoardJump);
                let bonus = if self.time_elapsed(self.board.last_jump_time, seconds(0.1)) {
                    2.0
                } else {
                    0.0
                };
                self.init_var_jump(min + extra, max + extra, true, false, bonus);
                let c = &mut self.control;
                c.status &= !(status::ON_SURFACE | status::ON_GROUND | status::TOUCH_SURFACE);
                c.mod_surface = mods;
                self.board.mods_backup = mods;
                self.board.shock_offsetv = 0.0;
                self.board.slow_transv = self.control.transv;
            }
            State::BoardFalling => {
                self.control.mod_surface = surface::board::JUMP;
                self.board.mods_backup = surface::board::JUMP;
            }
            State::BoardHitGround => {}
            State::BoardTurnTo { dir, duration } => {
                let c = &mut self.control;
                if !(c.gravity_length == 245760.0 || now - self.board.ride_time < seconds(0.2)) {
                    let g = c.gravity_normal;
                    let mut up = g.dot(c.transv);
                    if up < 0.0 {
                        up *= 5.0;
                    }
                    c.transv = with_vertical(c.transv, g, up);
                }
                c.mod_surface = surface::board::TURN_TO;
                self.board.mods_backup = surface::board::TURN_TO;
                let d = dir.normalize_or_zero();
                c.dir_targ = forward_up_nopitch_quat(d, y_axis(c.dir_targ));
                c.turn_lockout_end_time = now + duration;
            }
            State::BoardFlip => {
                let c = &mut self.control;
                c.dir_targ = forward_up_nopitch_quat(c.transv, y_axis(c.dir_targ));
                c.gravity_length = 245760.0;
                self.init_var_jump(JUMP_HEIGHT_MIN, JUMP_HEIGHT_MAX, true, false, 1.0);
                let c = &mut self.control;
                c.status &= !(status::ON_SURFACE | status::ON_GROUND | status::TOUCH_SURFACE);
                c.mod_surface = surface::board::FLIP;
                self.board.mods_backup = surface::board::FLIP;
                self.board.danger = Some(Danger::Spin);
                self.events.push(Event::BoardFlip);
            }
            State::BoardTrick(trick) => {
                let c = &mut self.control;
                c.dir_targ = forward_up_nopitch_quat(c.transv, y_axis(c.dir_targ));
                c.gravity_length = 245760.0;
                self.init_var_jump(TRICK_HEIGHT_MIN, TRICK_HEIGHT_MAX, true, false, 1.0);
                let c = &mut self.control;
                c.status &= !(status::ON_SURFACE | status::ON_GROUND | status::TOUCH_SURFACE);
                c.mod_surface = surface::board::FLIP;
                self.board.mods_backup = surface::board::FLIP;
                c.gravity_length = 147456.0;
                let (_, z) = self.pad.left_trick_axes();
                let push = analog_input((128.0 * z) as i32, 0.0, 96.0, 110.0, 1.0);
                let named = match trick {
                    BoardTrick::Grab if push < 0.0 => Trick::Kickspin,
                    BoardTrick::Grab if 40960.0 < self.control.height_above_ground() => {
                        Trick::Nosegrab
                    }
                    BoardTrick::Grab => Trick::Noseflip,
                    BoardTrick::Kick if self.board.trick_x >= 0.0 => Trick::Kickflip,
                    BoardTrick::Kick => Trick::BoardSpin,
                    BoardTrick::Hold => Trick::Method,
                };
                self.add_trick(named, 500.0);
            }
            State::BoardGetOff => {
                self.board.shock_offsetv = 0.0;
                let c = &mut self.control;
                c.status &= !(status::ON_SURFACE | status::ON_GROUND | status::TOUCH_SURFACE);
                c.mod_surface = surface::board::GET_OFF;
                c.gravity_length = control::STANDARD_GRAVITY;
                let to_ground = self.time_to_ground();
                let c = &mut self.control;
                let g = c.gravity_normal;
                let up = g.dot(c.transv).clamp(0.0, 40960.0);
                let lift = if to_ground < seconds(0.207) {
                    up + 0.0016666667 * (seconds(0.66) - to_ground) as f32 * c.gravity_length
                } else {
                    up
                };
                c.transv = with_vertical(c.transv, g, lift);
                self.board.slow_transv = self.control.transv;
            }
            _ => {}
        }
    }

    pub(crate) fn board_exit_state(&mut self, state: &State, next: &State) {
        match *state {
            State::BoardStance | State::BoardDuckStance => {
                self.control.no_normal_reset = false;
                self.board.turn_anim_tilt = false;
                self.board_anim_exit(next);
            }
            State::BoardGetOn => self.board_anim_land(),
            State::BoardTrick(trick) => {
                self.board.trick_z = 0.0;
                match trick {
                    BoardTrick::Hold => self.board.hold_exit_time = self.time,
                    BoardTrick::Grab | BoardTrick::Kick => self.board.tricky_exit_time = self.time,
                }
            }
            State::BoardFlip => {
                self.board.danger = None;
                self.board.flip_control = 0.0;
                let points = 500.0 * self.board.flip_count as f32
                    + if self.board.flip_count >= 2 {
                        2000.0
                    } else {
                        0.0
                    };
                self.add_trick(Trick::Flip, points);
            }
            _ => {}
        }
        self.board_exit(next);
    }

    /// The per-frame decisions of a board state. Returns the state to go to.
    pub(crate) fn board_trans(&mut self) -> Option<State> {
        let now = self.time;
        match self.state {
            State::BoardGetOn => {
                if self.control.on_surface()
                    || (now != self.state_time && self.hit_ground_or_stuck())
                {
                    self.control.status |= status::ON_SURFACE;
                    return Some(State::BoardHitGround);
                }
                None
            }
            State::BoardStance | State::BoardDuckStance => self.board_stance_trans(),
            State::BoardJump { .. } => {
                if let Some(next) = self.board_jump_trans() {
                    return Some(next);
                }
                let holding = self.pad.hold(button::X);
                self.control.jump_window =
                    self.control.jump_window.max(self.pad.pressure(button::X));
                self.mod_var_jump(holding);
                self.board.slow_transv = self.control.transv;
                self.board.shock_offset *= 0.8;
                None
            }
            State::BoardFalling => self.board_jump_trans(),
            State::BoardHitGround => {
                if self.pad.hold(button::L1) {
                    return Some(State::BoardDuckStance);
                }
                if self.pad.pressed(button::X) && self.can_jump(true) {
                    self.flush_tricks();
                    return Some(State::BoardJump {
                        min: JUMP_HEIGHT_MIN,
                        max: JUMP_HEIGHT_MAX,
                        duck: false,
                    });
                }
                self.board_smack_surface();
                self.control.mod_surface = surface::board::WALK;
                Some(State::BoardStance)
            }
            State::BoardTurnTo { duration, .. } => {
                if self.pad.pressed(button::X) && self.can_jump(true) {
                    self.flush_tricks();
                    return Some(State::BoardJump {
                        min: JUMP_HEIGHT_MIN,
                        max: JUMP_HEIGHT_MAX,
                        duck: false,
                    });
                }
                if self.time_elapsed(self.state_time, duration) {
                    return Some(State::BoardStance);
                }
                self.board_anim_trans();
                if self.board_on_ground() {
                    let tilt = self.board.turn_anim_tilt;
                    self.board_ground_check();
                    self.board.turn_anim_tilt = tilt;
                }
                self.board.anim.turn_targ *= 10.0;
                None
            }
            State::BoardFlip => {
                self.control.jump_window =
                    self.control.jump_window.max(self.pad.pressure(button::X));
                let holding = self.pad.hold(button::X);
                self.mod_var_jump(holding);
                self.board.slow_transv = self.control.transv;
                let (_, z) = self.pad.left_trick_axes();
                let stick = analog_input((128.0 * z) as i32, 0.0, 96.0, 110.0, 1.0);
                let finished = !self.pad.hold(button::R1)
                    || stick == 0.0
                    || self.hit_ground_or_stuck()
                    || self.time_to_ground() < seconds(0.5);
                if finished && self.time_elapsed(self.state_time, seconds(0.1)) {
                    if self.hit_ground_or_stuck() {
                        return Some(State::BoardHitGround);
                    }
                    return Some(State::BoardFalling);
                }
                None
            }
            State::BoardTrick(trick) => {
                if let Some(next) = self.board_spin_check() {
                    return Some(next);
                }
                self.control.jump_window =
                    self.control.jump_window.max(self.pad.pressure(button::X));
                let holding = self.pad.hold(button::X);
                self.mod_var_jump(holding);
                self.board.slow_transv = self.control.transv;
                if trick == BoardTrick::Kick && self.hit_ground_or_stuck() {
                    self.board.jump_land_time = now;
                    return Some(State::BoardHitGround);
                }
                let held = match trick {
                    BoardTrick::Grab => self.pad.hold(button::L1),
                    BoardTrick::Hold => self.pad.hold(button::L2),
                    BoardTrick::Kick => {
                        let (_, z) = self.pad.left_trick_axes();
                        self.pad.hold(button::R1)
                            && analog_input((128.0 * z) as i32, 0.0, 96.0, 110.0, 1.0) != 0.0
                    }
                };
                let limit = if trick == BoardTrick::Kick {
                    seconds(0.5)
                } else {
                    seconds(0.3)
                };
                if !held || self.hit_ground_or_stuck() || self.time_to_ground() < limit {
                    let c = &mut self.control;
                    let g = c.gravity_normal;
                    let up = g.dot(c.transv).min(0.0);
                    c.transv = with_vertical(c.transv, g, up);
                    return Some(State::BoardFalling);
                }
                None
            }
            State::BoardGetOff => {
                if now != self.state_time && self.hit_ground_or_stuck() {
                    return Some(if self.control.on_surface() {
                        State::HitGround
                    } else {
                        State::Falling
                    });
                }
                None
            }
            _ => None,
        }
    }

    fn board_stance_trans(&mut self) -> Option<State> {
        let now = self.time;
        let duck = self.state == State::BoardDuckStance;
        let recently_on_surface =
            !self.time_elapsed(self.control.last_time_on_surface, GROUND_TIMEOUT);
        if !duck && self.pad.hold(button::L1) && recently_on_surface {
            return Some(State::BoardDuckStance);
        }
        if duck && (!self.pad.hold(button::L1) || !recently_on_surface) {
            return Some(State::BoardStance);
        }
        if self.pad.pressed(button::X) && self.can_jump(true) {
            self.flush_tricks();
            let (min, max) = if duck {
                (DUCK_JUMP_HEIGHT_MIN, DUCK_JUMP_HEIGHT_MAX)
            } else {
                (JUMP_HEIGHT_MIN, JUMP_HEIGHT_MAX)
            };
            return Some(State::BoardJump { min, max, duck });
        }
        if self.pad.pressed(button::R1) {
            self.board.spin_ground_press_time = now;
        }
        if duck
            && self.pad.hold(button::R1)
            && !self.time_elapsed(self.board.spin_ground_press_time, seconds(0.3))
        {
            self.board.spin_ground_start_time = now;
        }
        if !duck
            && self.pad.pressed(button::TRIANGLE)
            && self.control.height_above_ground() < 8192.0
            && !self.time_elapsed(self.board.spin_ground_press_time, seconds(0.3))
        {
            self.board.spin_ground_start_time = now;
        }
        self.board_smack_surface();
        self.board_anim_trans();
        if self.board_on_ground() {
            self.board_ground_check();
            let mods = if duck {
                surface::board::DUCK
            } else {
                surface::board::WALK
            };
            self.control.mod_surface = mods;
            self.board.mods_backup = mods;
            None
        } else if self.time_elapsed(self.control.last_time_on_surface, seconds(0.1))
            && (self.control.mod_surface.name == surface::Name::Spin
                || 4096.0 < self.control.height_above_ground())
        {
            let mods = if duck {
                surface::board::DUCK_JUMP
            } else {
                surface::board::JUMP
            };
            self.control.mod_surface = mods;
            self.board.mods_backup = mods;
            self.board_spin_check()
        } else {
            self.control.mod_surface = surface::board::AIR;
            self.board.mods_backup = surface::board::AIR;
            None
        }
    }
}

pub const TRICK_HEIGHT_MIN: f32 = meters(0.9);
pub const TRICK_HEIGHT_MAX: f32 = meters(1.2);

/// The air tricks: a grab with L1 and the stick forward or back, a kick with
/// L1 and the stick across, a hold with L2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardTrick {
    Grab,
    Kick,
    Hold,
}

/// The state without its payload, for the comparisons the board loop makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StateKind {
    BoardTrick,
    OnFoot,
    BoardGetOn,
    BoardStance,
    BoardDuckStance,
    BoardJump,
    BoardFalling,
    BoardHitGround,
    BoardTurnTo,
    BoardFlip,
    BoardGetOff,
}
