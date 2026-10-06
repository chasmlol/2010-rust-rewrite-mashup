#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pml {
    pub forward: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub frametime: f32,
    pub msec: i32,
    pub walking: u32,
    pub ground_plane: u32,
    pub almost_ground_plane: u32,
    pub ground_trace: [u32; 11],
    pub previous_origin: [f32; 3],
    pub previous_velocity: [f32; 3],
    pub holdrand: i32,
    pub jump_animations: [Option<(crate::JumpAnimation, bool)>; 4],
    pub mantle_movetype: Option<u8>,
    pub landing_animation: bool,
    pub fall_damage: i32,
}

impl Pml {
    pub(crate) fn record_jump_animation(&mut self, animation: crate::JumpAnimation, force: bool) {
        let slot = self
            .jump_animations
            .iter_mut()
            .find(|slot| slot.is_none())
            .expect("at most four jump animation producers per movement step");
        *slot = Some((animation, force));
    }
}
