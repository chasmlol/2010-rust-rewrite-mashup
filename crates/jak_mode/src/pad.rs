//! The controller as Jak's movement reads it: a DualShock's buttons and its
//! sticks as bytes, the left stick turned into a direction and a speed with
//! the game's dead zone.
use crate::math::atan;

pub mod button {
    pub const X: u32 = 1 << 0;
    pub const SQUARE: u32 = 1 << 1;
    pub const CIRCLE: u32 = 1 << 2;
    pub const TRIANGLE: u32 = 1 << 3;
    pub const L1: u32 = 1 << 4;
    pub const L2: u32 = 1 << 5;
    pub const R1: u32 = 1 << 6;
    pub const R2: u32 = 1 << 7;
    pub const L3: u32 = 1 << 8;
    pub const R3: u32 = 1 << 9;
}

const STICK_DEADZONE: f32 = 0.3;

/// One simulation step's input. Sticks are `[-1, 1]` with +x right and +y up;
/// `pressed` holds the buttons that went down since the previous step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PadInput {
    pub held: u32,
    pub pressed: u32,
    pub left: [f32; 2],
    pub right: [f32; 2],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Pad {
    pub held: u32,
    pub pressed: u32,
    pub leftx: u8,
    pub lefty: u8,
    pub rightx: u8,
    pub righty: u8,
    /// Left stick direction (0 up, a quarter turn to the left) and how far it
    /// is pushed, 0 inside the dead zone.
    pub stick0_dir: f32,
    pub stick0_speed: f32,
}

/// A stick axis as the pad reports it: 0 to 255, centered on 128, pushed
/// right and pulled back high.
fn byte_x(v: f32) -> u8 {
    (128.0 + v.clamp(-1.0, 1.0) * 128.0)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn byte_y(v: f32) -> u8 {
    (128.0 - v.clamp(-1.0, 1.0) * 128.0)
        .round()
        .clamp(0.0, 255.0) as u8
}

impl Pad {
    pub fn update(&mut self, input: &PadInput) {
        self.held = input.held;
        self.pressed = input.pressed;
        self.leftx = byte_x(input.left[0]);
        self.lefty = byte_y(input.left[1]);
        self.rightx = byte_x(input.right[0]);
        self.righty = byte_y(input.right[1]);
        let x = (f32::from(self.leftx) - 128.0) / 128.0;
        let y = (127.0 - f32::from(self.lefty)) / 128.0;
        self.stick0_dir = atan(-x, y);
        self.stick0_speed = (x * x + y * y).sqrt().min(1.0);
        if self.stick0_speed < STICK_DEADZONE {
            self.stick0_speed = 0.0;
        }
    }

    pub fn hold(&self, buttons: u32) -> bool {
        self.held & buttons != 0
    }

    pub fn pressed(&self, buttons: u32) -> bool {
        self.pressed & buttons != 0
    }

    /// A button's pressure, 0 to 1. Digital pads report full travel.
    pub fn pressure(&self, buttons: u32) -> f32 {
        if self.hold(buttons) { 1.0 } else { 0.0 }
    }

    /// The left stick as the trick code reads it: x and z in `[-1, 1]`,
    /// pushed left and pushed forward positive.
    pub fn left_trick_axes(&self) -> (f32, f32) {
        (
            -(f32::from(self.leftx) - 128.0) / 128.0,
            -(f32::from(self.lefty) - 128.0) / 128.0,
        )
    }
}
