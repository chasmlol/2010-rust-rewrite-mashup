#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::vec::Vec;

mod adjust_angles;
pub mod names;
pub mod weapon_select;

pub use adjust_angles::{
    AdjustAnglesInput, CL_ANGLESPEEDKEY_DEFAULT, CL_PITCHSPEED_DEFAULT, CL_YAWSPEED_DEFAULT,
    adjust_angles,
};
pub use names::{
    HOLD_PAIR_LIMIT, INPUT_COMMAND_NAMES, SCRIPT_KEYNUM, command_id_from_name, command_id_lookup,
    command_name, command_names, key_up_command_id,
};

use playerstate_iw4::{UserCmd, buttons};

pub const ANGLE2SHORT: f32 = 65536.0 / 360.0;

pub const KEY_COUNT: usize = 256;

#[derive(Clone, Copy, Debug, Default)]
pub struct Kbutton {
    pub down: [u64; 5],
    pub downtime: i32,
    pub msec: u32,
    pub active: bool,
    pub was_pressed: bool,
}

fn held_key(key: i32) -> Option<(usize, u64)> {
    let index = if key == SCRIPT_KEYNUM {
        KEY_COUNT
    } else {
        let index = usize::try_from(key).ok()?;
        if index >= KEY_COUNT {
            return None;
        }
        index
    };
    Some((index / 64, 1 << (index % 64)))
}

pub fn in_key_down(btn: &mut Kbutton, key: i32, extra_time: i32) {
    let Some((slot, mask)) = held_key(key) else {
        return;
    };
    if btn.down[slot] & mask != 0 {
        return;
    }
    btn.down[slot] |= mask;
    if !btn.active {
        btn.downtime = extra_time;
        btn.active = true;
        btn.was_pressed = true;
    }
}

pub fn in_key_up(btn: &mut Kbutton, uptime: i32, key: i32, frame_msec: u32) {
    let Some((slot, mask)) = held_key(key) else {
        return;
    };
    if btn.down[slot] & mask == 0 {
        return;
    }
    btn.down[slot] &= !mask;
    if btn.down.iter().any(|bits| *bits != 0) {
        return;
    }
    if btn.active {
        if btn.downtime != 0 {
            let delta = uptime.wrapping_sub(btn.downtime);
            if delta > 0 {
                btn.msec = btn.msec.saturating_add(delta as u32);
            }
        } else {
            btn.msec = btn.msec.saturating_add(frame_msec >> 1);
        }
        btn.active = false;
        btn.downtime = 0;
    }
}

pub fn key_state(btn: &mut Kbutton, now_msec: i32, frame_msec: u32) -> f32 {
    let mut sample = btn.msec;
    btn.msec = 0;
    if btn.active {
        if btn.downtime != 0 {
            let add = now_msec.wrapping_sub(btn.downtime);
            if add > 0 {
                sample = sample.saturating_add(add as u32);
            }
        }
        btn.downtime = now_msec;
    }
    if sample == 0 || frame_msec == 0 {
        return 0.0;
    }
    if sample < frame_msec {
        (sample as f32) / (frame_msec as f32)
    } else {
        1.0
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct KeyState {
    pub down: i32,
    pub repeats: i32,
    pub binding: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct KbuttonSet {
    pub attack: Kbutton,
    pub melee: Kbutton,
    pub frag: Kbutton,
    pub smoke: Kbutton,
    pub usereload: Kbutton,
    pub speed: Kbutton,
    pub gostand: Kbutton,
    pub forward: Kbutton,
    pub back: Kbutton,
    pub moveleft: Kbutton,
    pub moveright: Kbutton,
    pub movedown: Kbutton,
    pub left: Kbutton,
    pub right: Kbutton,
    pub lookup: Kbutton,
    pub lookdown: Kbutton,
    pub strafe: Kbutton,
    pub holdbreath: Kbutton,
    pub activate: Kbutton,
    pub reload: Kbutton,
    pub prone: Kbutton,
    pub mlook: Kbutton,
    pub throw_btn: Kbutton,
    pub sprint: Kbutton,
    pub scores: Kbutton,
    pub talk: Kbutton,
    pub stance: Kbutton,
}

impl KbuttonSet {
    pub fn visit_active_names(&self, mut f: impl FnMut(&'static str)) {
        if self.attack.active || self.attack.was_pressed {
            f("+attack");
        }
        if self.melee.active || self.melee.was_pressed {
            f("+melee");
        }
        if self.frag.active || self.frag.was_pressed {
            f("+frag");
        }
        if self.smoke.active || self.smoke.was_pressed {
            f("+smoke");
        }
        if self.usereload.active || self.usereload.was_pressed {
            f("+usereload");
        }
        if self.speed.active || self.speed.was_pressed {
            f("+speed_throw");
        }
        if self.gostand.active || self.gostand.was_pressed {
            f("+gostand");
        }
        if self.forward.active || self.forward.was_pressed {
            f("+forward");
        }
        if self.back.active || self.back.was_pressed {
            f("+back");
        }
        if self.moveleft.active || self.moveleft.was_pressed {
            f("+moveleft");
        }
        if self.moveright.active || self.moveright.was_pressed {
            f("+moveright");
        }
        if self.movedown.active || self.movedown.was_pressed {
            f("+movedown");
        }
        if self.left.active || self.left.was_pressed {
            f("+left");
        }
        if self.right.active || self.right.was_pressed {
            f("+right");
        }
        if self.lookup.active || self.lookup.was_pressed {
            f("+lookup");
        }
        if self.lookdown.active || self.lookdown.was_pressed {
            f("+lookdown");
        }
        if self.activate.active || self.activate.was_pressed {
            f("+activate");
        }
        if self.reload.active || self.reload.was_pressed {
            f("+reload");
        }
        if self.prone.active || self.prone.was_pressed {
            f("+prone");
        }
        if self.sprint.active || self.sprint.was_pressed {
            f("+sprint");
        }
    }

    pub fn clear_was_pressed(&mut self) {
        self.stance.was_pressed = false;
        self.attack.was_pressed = false;
        self.melee.was_pressed = false;
        self.frag.was_pressed = false;
        self.smoke.was_pressed = false;
        self.usereload.was_pressed = false;
        self.speed.was_pressed = false;
        self.gostand.was_pressed = false;
        self.forward.was_pressed = false;
        self.back.was_pressed = false;
        self.moveleft.was_pressed = false;
        self.moveright.was_pressed = false;
        self.movedown.was_pressed = false;
        self.left.was_pressed = false;
        self.right.was_pressed = false;
        self.lookup.was_pressed = false;
        self.lookdown.was_pressed = false;
        self.strafe.was_pressed = false;
        self.holdbreath.was_pressed = false;
        self.activate.was_pressed = false;
        self.reload.was_pressed = false;
        self.prone.was_pressed = false;
        self.mlook.was_pressed = false;
        self.throw_btn.was_pressed = false;
        self.sprint.was_pressed = false;
        self.scores.was_pressed = false;
        self.talk.was_pressed = false;
    }
}

#[derive(Clone, Debug)]
pub struct ClientInput {
    pub keys: [KeyState; KEY_COUNT],
    pub kb: KbuttonSet,

    pub using_ads: bool,

    pub stance_latch: i32,

    pub weapon_cycles: Vec<bool>,
    pub action_slots: Vec<usize>,

    pub offhand_hold_cancel: bool,

    pub stance_held: Option<(i32, i32)>,
    pub center_view: bool,
    pub airborne: bool,
}

impl Default for ClientInput {
    fn default() -> Self {
        Self {
            keys: [KeyState::default(); KEY_COUNT],
            kb: KbuttonSet::default(),
            using_ads: false,
            stance_latch: 0,
            weapon_cycles: Vec::new(),
            action_slots: Vec::new(),
            offhand_hold_cancel: false,
            stance_held: None,
            center_view: false,
            airborne: false,
        }
    }
}

pub fn set_ads(client: &mut ClientInput, ads: bool) {
    client.using_ads = ads;
}

fn pair_down(cmd_id: u32) -> bool {
    cmd_id < HOLD_PAIR_LIMIT && cmd_id % 2 == 1
}

fn apply_pair(btn: &mut Kbutton, cmd_id: u32, key: i32, now_msec: i32, frame_msec: u32) {
    if pair_down(cmd_id) {
        in_key_down(btn, key, now_msec);
    } else {
        in_key_up(btn, now_msec, key, frame_msec);
    }
}

pub fn input_cmd(client: &mut ClientInput, cmd_id: u32, key: i32, now_msec: i32, frame_msec: u32) {
    match cmd_id {
        1 | 2 => apply_pair(&mut client.kb.attack, cmd_id, key, now_msec, frame_msec),
        3 | 4 => apply_pair(&mut client.kb.melee, cmd_id, key, now_msec, frame_msec),
        5 | 6 => apply_pair(&mut client.kb.frag, cmd_id, key, now_msec, frame_msec),
        7 | 8 => apply_pair(&mut client.kb.smoke, cmd_id, key, now_msec, frame_msec),
        9 | 10 => {
            apply_pair(&mut client.kb.holdbreath, cmd_id, key, now_msec, frame_msec);
            apply_pair(&mut client.kb.sprint, cmd_id, key, now_msec, frame_msec);
        }
        11 | 12 => apply_pair(&mut client.kb.usereload, cmd_id, key, now_msec, frame_msec),
        13 | 14 => {
            if pair_down(cmd_id) {
                client.using_ads = false;
            }
            apply_pair(&mut client.kb.speed, cmd_id, key, now_msec, frame_msec);
            apply_pair(&mut client.kb.throw_btn, cmd_id, key, now_msec, frame_msec);
        }
        15..=22 => {
            if pair_down(cmd_id) {
                client.action_slots.push(((cmd_id - 15) / 2) as usize);
            }
        }
        23 | 24 => {
            let was_active = client.kb.stance.active;
            apply_pair(&mut client.kb.stance, cmd_id, key, now_msec, frame_msec);
            if was_active != client.kb.stance.active {
                stance_button(client, client.kb.stance.active, now_msec);
            }
        }
        25 | 26 => {
            apply_pair(&mut client.kb.gostand, cmd_id, key, now_msec, frame_msec);
        }
        27 | 28 => apply_pair(&mut client.kb.forward, cmd_id, key, now_msec, frame_msec),
        29 | 30 => apply_pair(&mut client.kb.back, cmd_id, key, now_msec, frame_msec),
        31 | 32 => apply_pair(&mut client.kb.moveleft, cmd_id, key, now_msec, frame_msec),
        33 | 34 => apply_pair(&mut client.kb.moveright, cmd_id, key, now_msec, frame_msec),
        35 | 36 => apply_pair(&mut client.kb.movedown, cmd_id, key, now_msec, frame_msec),
        37 | 38 => apply_pair(&mut client.kb.left, cmd_id, key, now_msec, frame_msec),
        39 | 40 => apply_pair(&mut client.kb.right, cmd_id, key, now_msec, frame_msec),
        41 | 42 => apply_pair(&mut client.kb.lookup, cmd_id, key, now_msec, frame_msec),
        43 | 44 => apply_pair(&mut client.kb.lookdown, cmd_id, key, now_msec, frame_msec),
        45 | 46 => apply_pair(&mut client.kb.strafe, cmd_id, key, now_msec, frame_msec),
        47 | 48 => apply_pair(&mut client.kb.holdbreath, cmd_id, key, now_msec, frame_msec),
        49 | 50 => apply_pair(&mut client.kb.activate, cmd_id, key, now_msec, frame_msec),
        51 | 52 => apply_pair(&mut client.kb.reload, cmd_id, key, now_msec, frame_msec),
        53 | 54 => apply_pair(&mut client.kb.prone, cmd_id, key, now_msec, frame_msec),
        55 | 56 => apply_pair(&mut client.kb.mlook, cmd_id, key, now_msec, frame_msec),
        57 | 58 => {
            if pair_down(cmd_id) {
                client.using_ads = !client.using_ads;
            }
            apply_pair(&mut client.kb.throw_btn, cmd_id, key, now_msec, frame_msec);
        }
        59 | 60 => apply_pair(&mut client.kb.sprint, cmd_id, key, now_msec, frame_msec),
        61 | 62 => apply_pair(&mut client.kb.scores, cmd_id, key, now_msec, frame_msec),
        63 | 64 => apply_pair(&mut client.kb.talk, cmd_id, key, now_msec, frame_msec),
        65 | 67 | 68 | 69 => {}
        66 | 70 => client.weapon_cycles.push(cmd_id == 66),
        71 => client.center_view = true,
        72 | 73 => {
            let stance = if cmd_id == 72 {
                buttons::CROUCH
            } else {
                buttons::PRONE
            } as i32;
            client.stance_latch = if client.stance_latch == stance {
                0
            } else {
                stance
            };
        }
        74 => client.stance_latch = buttons::PRONE as i32,
        75 => client.stance_latch = buttons::CROUCH as i32,
        76 => client.using_ads = !client.using_ads,
        77 => set_ads(client, false),
        _ => {}
    }
}

const STANCE_HOLD_MS: i32 = 300;

fn stance_button(client: &mut ClientInput, down: bool, now_msec: i32) {
    if client.kb.prone.active || client.kb.movedown.active {
        return;
    }
    let crouch = buttons::CROUCH as i32;
    if down {
        client.stance_held = Some((client.stance_latch, now_msec));
        client.stance_latch = crouch;
    } else if let Some((from, _)) = client.stance_held.take()
        && from == crouch
    {
        client.stance_latch = 0;
    }
}

fn stance_hold(client: &mut ClientInput, now_msec: i32) {
    let Some((from, since)) = client.stance_held else {
        return;
    };
    if client.airborne {
        client.stance_held = Some((from, now_msec));
        return;
    }
    if now_msec.wrapping_sub(since) >= STANCE_HOLD_MS {
        let prone = buttons::PRONE as i32;
        client.stance_latch = if from == prone { 0 } else { prone };
        client.stance_held = None;
    }
}

pub fn key_event(
    client: &mut ClientInput,
    key_num: usize,
    down: bool,
    now_msec: i32,
    frame_msec: u32,
) {
    if key_num >= KEY_COUNT {
        return;
    }
    if down {
        client.keys[key_num].down = 1;
        client.keys[key_num].repeats = client.keys[key_num].repeats.saturating_add(1);
        let id = client.keys[key_num].binding;
        if id != 0 {
            input_cmd(client, id, key_num as i32, now_msec, frame_msec);
        }
    } else {
        client.keys[key_num].down = 0;
        client.keys[key_num].repeats = 0;
        let id = client.keys[key_num].binding;
        if let Some(up_id) = key_up_command_id(id) {
            input_cmd(client, up_id, key_num as i32, now_msec, frame_msec);
        }
    }
}

pub fn apply_mouse_sensitivity(
    mx: f32,
    my: f32,
    mouse_counts: f32,
    frame_msec: f32,
    sensitivity: f32,
    mouse_accel: f32,
    fov_scale: f32,
) -> (f32, f32) {
    let frame = if frame_msec > 0.0 { frame_msec } else { 1.0 };
    let rate = mouse_counts / frame;
    let scale = (rate * mouse_accel + sensitivity) * fov_scale;
    (mx * scale, my * scale)
}

pub fn mouse_move_angles(mx: f32, my: f32, m_yaw: f32, m_pitch: f32) -> (i32, i32) {
    let pitch = my * m_pitch * ANGLE2SHORT;
    let yaw = -mx * m_yaw * ANGLE2SHORT;
    (pitch as i32, yaw as i32)
}

pub fn update_cmd_button(btn: &Kbutton, bit: u32, out_buttons: &mut u32) {
    if btn.active || btn.was_pressed {
        *out_buttons |= bit;
    }
}

pub fn cmd_buttons(state: &KbuttonSet) -> u32 {
    let mut bits = 0u32;
    update_cmd_button(&state.attack, buttons::ATTACK, &mut bits);
    update_cmd_button(&state.holdbreath, buttons::BREATH, &mut bits);
    update_cmd_button(&state.frag, buttons::FRAG, &mut bits);
    update_cmd_button(&state.smoke, buttons::SMOKE, &mut bits);
    update_cmd_button(&state.melee, buttons::MELEE_CHARGE, &mut bits);
    update_cmd_button(&state.activate, buttons::USE, &mut bits);
    update_cmd_button(&state.reload, buttons::RELOAD, &mut bits);
    update_cmd_button(&state.usereload, buttons::USE_RELOAD, &mut bits);
    update_cmd_button(&state.movedown, buttons::CROUCH, &mut bits);
    update_cmd_button(&state.prone, buttons::PRONE, &mut bits);
    update_cmd_button(&state.gostand, buttons::JUMP, &mut bits);
    update_cmd_button(&state.throw_btn, buttons::THROW, &mut bits);
    bits
}

pub fn key_move_ads_bit(speed: &Kbutton, using_ads: bool) -> u32 {
    if speed.active != using_ads {
        buttons::ADS
    } else {
        0
    }
}

pub fn key_move_bits(kb: &KbuttonSet, using_ads: bool, mut bits: u32) -> u32 {
    if kb.prone.active {
        bits &= !buttons::CROUCH;
        bits |= buttons::PRONE | buttons::STANCE_HELD;
    } else if kb.movedown.active {
        bits &= !buttons::PRONE;
        bits |= buttons::CROUCH | buttons::STANCE_HELD;
    } else {
        bits &= !(buttons::PRONE | buttons::CROUCH | buttons::STANCE_HELD);
    }
    if key_move_ads_bit(&kb.speed, using_ads) != 0 {
        bits |= buttons::ADS;
    } else {
        bits &= !buttons::ADS;
    }
    if kb.sprint.active || kb.sprint.was_pressed {
        if !kb.back.active {
            bits |= buttons::SPRINT;
        }
    } else {
        bits &= !buttons::SPRINT;
    }
    bits
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MoveAxes {
    pub forward: f32,
    pub right: f32,
}

pub fn key_move_from_fractions(forward: f32, back: f32, right: f32, left: f32) -> MoveAxes {
    MoveAxes {
        forward: (forward - back).clamp(-1.0, 1.0),
        right: (right - left).clamp(-1.0, 1.0),
    }
}

pub fn axis_to_move(axis: f32) -> i8 {
    let v = axis * 127.0;
    let rounded = if v >= 0.0 {
        (v + 0.5) as i32
    } else {
        (v - 0.5) as i32
    };
    if rounded > 127 {
        127
    } else if rounded < -127 {
        -127
    } else {
        rounded as i8
    }
}

#[derive(Clone, Debug, Default)]
pub struct CreateCmdInput {
    pub server_time: i32,
    pub angles: [i32; 3],
    pub buttons: u32,
    pub forwardmove: i8,
    pub rightmove: i8,
    pub mouse_pitch_delta: i32,
    pub mouse_yaw_delta: i32,
    pub key_pitch_delta: i32,
    pub key_yaw_delta: i32,
    pub frozen: bool,
}

pub fn create_cmd(input: &CreateCmdInput) -> UserCmd {
    if input.frozen {
        return UserCmd {
            server_time: input.server_time,
            angles: input.angles,
            ..UserCmd::default()
        };
    }
    let mut angles = input.angles;
    angles[0] = angles[0]
        .wrapping_add(input.key_pitch_delta)
        .wrapping_add(input.mouse_pitch_delta);
    angles[1] = angles[1]
        .wrapping_add(input.key_yaw_delta)
        .wrapping_add(input.mouse_yaw_delta);
    UserCmd {
        server_time: input.server_time,
        buttons: input.buttons,
        angles,
        weapon: 0,
        weapon_mapped: 0,
        off_hand_index: 0,
        forwardmove: input.forwardmove,
        rightmove: input.rightmove,
        melee_charge_yaw: 0.0,
        melee_charge_dist: 0,
        selected_location: [0; 3],
        remote_control: [0; 2],
        gun_angle_offset: [0.0; 2],
    }
}

pub fn sample_move(client: &mut ClientInput, now_msec: i32, frame_msec: u32) -> (u32, MoveAxes) {
    stance_hold(client, now_msec);
    let mut bits = key_move_bits(&client.kb, client.using_ads, cmd_buttons(&client.kb));
    if client.kb.gostand.was_pressed {
        client.stance_latch = 0;
        client.stance_held = None;
    }
    if bits & buttons::STANCE_HELD == 0 {
        bits |= client.stance_latch as u32;
    }
    let axes = key_move_from_fractions(
        movement_key_state(&mut client.kb.forward, now_msec, frame_msec),
        movement_key_state(&mut client.kb.back, now_msec, frame_msec),
        movement_key_state(&mut client.kb.moveright, now_msec, frame_msec),
        movement_key_state(&mut client.kb.moveleft, now_msec, frame_msec),
    );
    (bits, axes)
}

fn movement_key_state(btn: &mut Kbutton, now_msec: i32, frame_msec: u32) -> f32 {
    let fraction = key_state(btn, now_msec, frame_msec);
    if btn.active { 1.0 } else { fraction }
}

pub const CMD_RING_MASK: u32 = 0x7f;
