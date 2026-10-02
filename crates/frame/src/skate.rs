use bevy::prelude::*;

/// Local skating presentation. Physics remains owned by the optional Skate host.
#[derive(Resource, Default)]
pub struct SkateMode {
    pub active: bool,
    pub entering: bool,
    pub preloaded: bool,
    pub preload_pending: bool,
    pub controller: Option<usize>,
    pub toggle_requested: bool,
    pub input_blocked: bool,
    pub client: u32,
    pub root: Mat4,
    pub bones: Vec<Mat4>,
    pub names: Vec<String>,
    pub camera: Option<(Transform, f32)>,
    pub tick: u64,
    pub status: String,
    pub marker_placed: bool,
    pub marker_can_return: bool,
    pub marker_progress: f32,
    pub dropper_open: bool,
    pub dropper_status: String,
    pub dropper_camera: Option<Transform>,
    pub dropper_placement_distance: f32,
}
