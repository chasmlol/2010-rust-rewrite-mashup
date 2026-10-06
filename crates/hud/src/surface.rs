use bevy::prelude::*;
use hud_iw4::{AppliedRect, ScreenPlacement};

const SQUARE_PIXELS: f32 = 1.0;

#[derive(Resource, Clone, Copy, Debug)]
pub struct Hud2dSurface {
    width: f32,
    height: f32,
    place: ScreenPlacement,
}

impl Default for Hud2dSurface {
    fn default() -> Self {
        Self::new(0.0, 0.0)
    }
}

impl Hud2dSurface {
    pub fn new(width: f32, height: f32) -> Self {
        let place =
            ScreenPlacement::setup_fullscreen(width.max(1.0), height.max(1.0), SQUARE_PIXELS);
        Self {
            width,
            height,
            place,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.width > 1.0 && self.height > 1.0
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn height(&self) -> f32 {
        self.height
    }

    pub fn apply_rect(
        &self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        horz_align: i32,
        vert_align: i32,
    ) -> AppliedRect {
        self.place.apply_rect(x, y, w, h, horz_align, vert_align)
    }

    pub fn scale_virtual_to_real(&self) -> [f32; 2] {
        self.place.scale_virtual_to_real
    }

    pub fn display_pixel_aspect(&self) -> f32 {
        SQUARE_PIXELS
    }

    pub fn placement(&self) -> &ScreenPlacement {
        &self.place
    }
}

pub(crate) fn update_hud_surface(
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut surface: ResMut<Hud2dSurface>,
) {
    let next = match windows.single() {
        Ok(window) => Hud2dSurface::new(window.width(), window.height()),
        Err(_) => Hud2dSurface::new(0.0, 0.0),
    };
    *surface = next;
}
