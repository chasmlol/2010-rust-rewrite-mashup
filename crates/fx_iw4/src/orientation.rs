use crate::flags::{FX_ELEM_RUN_RELATIVE_TO_EFFECT, FX_ELEM_RUN_RELATIVE_TO_SPAWN, elem_run_mode};
use crate::origin::{
    FX_ELEM_SPAWN_OFFSET_SPHERE, apply_spawn_origin, elem_spawn_relative, offset_spawn_origin,
};
use crate::vec::{vec3_length_sq, vec3_normalize, vector_vectors};

pub const FX_ORIENT_UP_DOT_GATE: f64 = 0.999_000_012_874_603_3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxOrientation {
    pub origin: [f32; 3],
    pub axis: [[f32; 3]; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxOrientFrame {
    pub origin: [f32; 3],
    pub axis: [[f32; 3]; 3],
}

impl FxOrientFrame {
    pub const IDENTITY: Self = Self {
        origin: [0.0; 3],
        axis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };
}

#[derive(Clone, Copy, Debug)]
pub struct FxOrientSpawnParams {
    pub spawn_origin: [[f32; 2]; 3],
    pub spawn_offset_radius: [f32; 2],
    pub spawn_offset_height: [f32; 2],
    pub seed: u64,
}

#[inline]
pub fn get_orientation(
    flags: i32,
    effect_now: &FxOrientFrame,
    effect_at_spawn: &FxOrientFrame,
    spawn: Option<FxOrientSpawnParams>,
) -> FxOrientation {
    let mode = elem_run_mode(flags);
    match mode {
        0 => FxOrientation {
            origin: [0.0; 3],
            axis: FxOrientFrame::IDENTITY.axis,
        },
        m if m == FX_ELEM_RUN_RELATIVE_TO_SPAWN => FxOrientation {
            origin: effect_at_spawn.origin,
            axis: effect_at_spawn.axis,
        },
        m if m == FX_ELEM_RUN_RELATIVE_TO_EFFECT => FxOrientation {
            origin: effect_now.origin,
            axis: effect_now.axis,
        },
        _ => {
            let Some(sp) = spawn else {
                return FxOrientation {
                    origin: effect_at_spawn.origin,
                    axis: effect_at_spawn.axis,
                };
            };
            get_orientation_spawn_built(flags, effect_at_spawn, sp)
        }
    }
}

fn get_orientation_spawn_built(
    flags: i32,
    effect_at_spawn: &FxOrientFrame,
    sp: FxOrientSpawnParams,
) -> FxOrientation {
    let relative = elem_spawn_relative(flags);
    let mut origin = apply_spawn_origin(
        effect_at_spawn.origin,
        effect_at_spawn.axis,
        sp.spawn_origin,
        sp.seed,
        relative,
    );
    let mut offset = [0.0f32; 3];
    offset_spawn_origin(
        &mut offset,
        effect_at_spawn.axis,
        flags,
        sp.spawn_offset_radius[0],
        sp.spawn_offset_radius[1],
        sp.spawn_offset_height[0],
        sp.spawn_offset_height[1],
        sp.seed,
    );
    origin[0] += offset[0];
    origin[1] += offset[1];
    origin[2] += offset[2];

    let mut forward = offset;
    let len_sq = vec3_length_sq(forward);
    if len_sq <= 1e-12 {
        forward = [1.0, 0.0, 0.0];
    } else {
        forward = vec3_normalize(forward);
    }

    let axis = if (flags & 0x30) == FX_ELEM_SPAWN_OFFSET_SPHERE {
        vector_vectors(forward)
    } else {
        let up_guess = if libm::fabsf(forward[2]) < (FX_ORIENT_UP_DOT_GATE as f32) {
            [0.0, 0.0, 1.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let right = vec3_normalize([
            up_guess[1] * forward[2] - up_guess[2] * forward[1],
            up_guess[2] * forward[0] - up_guess[0] * forward[2],
            up_guess[0] * forward[1] - up_guess[1] * forward[0],
        ]);
        let up = [
            forward[1] * right[2] - forward[2] * right[1],
            forward[2] * right[0] - forward[0] * right[2],
            forward[0] * right[1] - forward[1] * right[0],
        ];
        [forward, right, up]
    };

    FxOrientation { origin, axis }
}
