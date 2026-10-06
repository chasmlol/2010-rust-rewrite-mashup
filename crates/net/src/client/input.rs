use bevy::prelude::Resource;
use input_iw4::{
    ANGLE2SHORT, AdjustAnglesInput, CL_ANGLESPEEDKEY_DEFAULT, CL_PITCHSPEED_DEFAULT,
    CL_YAWSPEED_DEFAULT, ClientInput, CreateCmdInput, adjust_angles, axis_to_move, create_cmd,
    mouse_move_angles, sample_move,
};
use playerstate_iw4::UserCmd;
use playerstate_iw4::buttons;
use std::collections::BTreeSet;

pub const KEY_FRAME_MSEC_MAX: u32 = 200;

pub fn frame_time_msec(elapsed_secs: f32) -> i32 {
    (elapsed_secs * 1000.0).max(1.0) as i32
}

pub fn key_frame_msec(delta_secs: f32) -> u32 {
    let ms = (delta_secs.max(0.0) * 1000.0).round() as i32;
    ms.clamp(1, KEY_FRAME_MSEC_MAX as i32) as u32
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct LookState {
    pub angles: [i32; 3],
}

#[derive(Resource, Clone, Debug)]
pub struct ClientActionInput {
    pub client: ClientInput,
    pub(crate) stance_life: Option<u32>,
    pub(crate) stance_event_sequence: i32,

    pub scripted_ids: BTreeSet<u32>,
    pub mouse_x: f32,
    pub mouse_y: f32,
    pub sensitivity: f32,
    pub mouse_accel: f32,
    pub fov_scale: f32,

    pub shellshock_look_scale: f32,

    pub cgame_max_pitch_speed: f32,

    pub cgame_max_yaw_speed: f32,
    pub m_yaw: f32,
    pub m_pitch: f32,

    pub cl_yawspeed: f32,
    pub now_msec: i32,
    pub frame_msec: u32,

    /// Forward and right.
    pub pad_move: [f32; 2],
    /// Right and up.
    pub pad_look: [f32; 2],
    pub pad_deflection: f32,
    pub pad_sensitivity: f32,
    pub pad_acceleration: bool,
    pub pad_ads_sensitivity: f32,
    pub pad_turn_rate: [f32; 2],
    /// Pitch and yaw, degrees.
    pub pad_look_delta: [f32; 2],
    pub pad_aim_assist: u8,
    pub pad_lockon: Option<u64>,
    pub pad_autoaim: Option<(u64, f32)>,
    pub pad_was_ads: bool,
    pub use_reload_down: bool,
    pub use_reload_over_item: Option<(i32, i32)>,
}

impl Default for ClientActionInput {
    fn default() -> Self {
        Self {
            client: ClientInput::default(),
            stance_life: None,
            stance_event_sequence: 0,
            scripted_ids: BTreeSet::new(),
            mouse_x: 0.0,
            mouse_y: 0.0,
            sensitivity: 5.0,
            mouse_accel: 0.0,
            fov_scale: 1.0,
            shellshock_look_scale: 1.0,
            cgame_max_pitch_speed: 0.0,
            cgame_max_yaw_speed: 0.0,
            m_yaw: 0.022,
            m_pitch: 0.022,
            cl_yawspeed: CL_YAWSPEED_DEFAULT,
            now_msec: 16,
            frame_msec: 16,
            pad_move: [0.0; 2],
            pad_look: [0.0; 2],
            pad_deflection: 0.0,
            pad_sensitivity: 1.0,
            pad_acceleration: true,
            pad_ads_sensitivity: 1.0,
            pad_turn_rate: [0.0; 2],
            pad_look_delta: [0.0; 2],
            pad_aim_assist: 0,
            pad_lockon: None,
            pad_autoaim: None,
            pad_was_ads: false,
            use_reload_down: false,
            use_reload_over_item: None,
        }
    }
}

impl ClientActionInput {
    pub fn consume_edges(&mut self) {
        self.client.kb.clear_was_pressed();
    }
}

pub fn look_angles_from_degrees(viewangles: [f32; 3]) -> [i32; 3] {
    [
        (viewangles[0] * ANGLE2SHORT) as i32,
        (viewangles[1] * ANGLE2SHORT) as i32,
        (viewangles[2] * ANGLE2SHORT) as i32,
    ]
}

pub fn accumulate_look(
    dt: f32,
    now_msec: i32,
    frame_msec: u32,
    client: &mut ClientInput,
    look: &mut LookState,
    cl_yawspeed: f32,
    frozen: bool,
    cgame_max_pitch_speed: f32,
    cgame_max_yaw_speed: f32,
) {
    let (pitch_deg, yaw_deg) = adjust_angles(
        &mut client.kb,
        AdjustAnglesInput {
            dt,
            now_msec,
            frame_msec,
            cl_yawspeed,
            cl_pitchspeed: CL_PITCHSPEED_DEFAULT,
            cl_anglespeedkey: CL_ANGLESPEEDKEY_DEFAULT,
            cgame_max_yaw_speed,
            cgame_max_pitch_speed,
            frozen,
        },
    );
    look.angles[0] = look.angles[0].wrapping_add((pitch_deg * ANGLE2SHORT) as i32);
    look.angles[1] = look.angles[1].wrapping_add((yaw_deg * ANGLE2SHORT) as i32);
}

pub fn build_usercmd(input: &mut ClientActionInput, look: &LookState, server_time: i32) -> UserCmd {
    let now = input.now_msec.max(1);
    let frame = input.frame_msec.max(1);
    let (bits, axes) = sample_move(&mut input.client, now, frame);
    let bits = if input.client.offhand_hold_cancel {
        input.client.offhand_hold_cancel = false;
        bits | buttons::OFFHAND_HOLD_CANCEL
    } else {
        bits
    };

    let mouse_counts = input.mouse_x.abs() + input.mouse_y.abs();
    let frame_f = frame as f32;
    let (mx, my) = input_iw4::apply_mouse_sensitivity(
        input.mouse_x,
        input.mouse_y,
        mouse_counts,
        frame_f,
        input.sensitivity,
        input.mouse_accel,
        input.fov_scale,
    );
    let (mouse_pitch, mouse_yaw) = mouse_move_angles(mx, my, input.m_yaw, input.m_pitch);
    let mouse_look = input.mouse_x != 0.0 || input.mouse_y != 0.0;
    let pad_pitch = if mouse_look {
        0
    } else {
        (input.pad_look_delta[0] * ANGLE2SHORT) as i32
    };
    let pad_yaw = if mouse_look {
        0
    } else {
        (input.pad_look_delta[1] * ANGLE2SHORT) as i32
    };
    let forward = (axes.forward + input.pad_move[0]).clamp(-1.0, 1.0);
    let right = (axes.right + input.pad_move[1]).clamp(-1.0, 1.0);

    create_cmd(&CreateCmdInput {
        server_time,
        angles: look.angles,
        buttons: bits,
        forwardmove: axis_to_move(forward),
        rightmove: axis_to_move(right),
        mouse_pitch_delta: mouse_pitch + pad_pitch,
        mouse_yaw_delta: mouse_yaw + pad_yaw,
        key_pitch_delta: 0,
        key_yaw_delta: 0,
        frozen: false,
    })
}

pub fn remote_control_axes(input: &ClientActionInput, mouse_x: f32, mouse_y: f32) -> [u8; 2] {
    let (mx, my) = input_iw4::apply_mouse_sensitivity(
        mouse_x,
        mouse_y,
        mouse_x.abs() + mouse_y.abs(),
        input.frame_msec.max(1) as f32,
        input.sensitivity,
        input.mouse_accel,
        input.fov_scale,
    );
    let axis = |v: f32| (v.clamp(-1.0, 1.0) * 127.0).round() as i8 as u8;
    [axis(-input.m_pitch * my), axis(input.m_yaw * mx)]
}

pub fn idle_usercmd(server_time: i32) -> UserCmd {
    UserCmd {
        server_time,
        ..UserCmd::default()
    }
}
