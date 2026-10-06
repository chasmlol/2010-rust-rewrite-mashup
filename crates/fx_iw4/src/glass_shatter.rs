use crate::random::{FxRandomChannel, sample_at};

pub const FX_GLASS_SHATTER_BRANCH_SCALE: f32 = -3.0;

pub const FX_GLASS_SHATTER_TWO_PI: f32 = 6.283_185_482_025_146_5;

pub const FX_GLASS_FRINGE_MAXCOVERAGE: f32 = 0.2;

pub const FX_GLASS_SHARD_MAXSIZE: f32 = 300.0;

pub const FX_GLASS_FRINGE_MAXSIZE: f32 = 150.0;

pub const FX_GLASS_MAX_PIECES_PER_FRAME: f32 = 100.0;

pub const FX_GLASS_SPLIT_OP_CAP: u32 = 48;

pub const FX_GLASS_SHARD_LIFETIME_MSEC: i32 = 5000;

pub const FX_GLASS_SETTLED_LIFETIME_MSEC: i32 = 1500;

pub const FX_GLASS_SETTLED_FADE_MSEC: i32 = 250;

pub const FX_GLASS_SETTLED_CAP: u32 = 64;

pub const FX_GLASS_AIRBORNE_PER_BREAK: u32 = 64;

pub const FX_GLASS_RESTITUTION: f32 = 0.15;

pub const FX_GLASS_MOTION_STEP_MSEC: i32 = 16;

pub const FX_GLASS_CATCHUP_STEPS: i32 = 4;

pub const FX_GLASS_PENDING_SUPPORT_FRAC: f32 = 0.25;

pub const FX_GLASS_PENDING_MIN_MSEC: i32 = 150;

pub const FX_GLASS_PENDING_MAX_MSEC: i32 = 1000;

pub const FX_GLASS_ACCENT_BOUNCE_CAP: u32 = 16;

pub const FX_GLASS_AIRBORNE_CAP: u32 = 256;

pub const FX_GLASS_SHATTER_FX_PER_FRAME: u32 = 6;

pub const FX_GLASS_LANDING_CELL: f32 = 64.0;

pub const FX_GLASS_LANDING_AGGREGATE_MSEC: i32 = 100;

pub const FX_GLASS_STATE_FLAG_DAMAGED: u16 = 1;

pub const FX_GLASS_STATE_FLAG_SIMPLE: u16 = 2;

pub const FX_GLASS_STATE_FLAG_CHILD_CLEAR: u16 = 4;

pub const FX_GLASS_SHATTER_FX_32: &str = "code/glass_shatter_32x32";

pub const FX_GLASS_SHATTER_FX_64: &str = "code/glass_shatter_64x64";

pub const FX_GLASS_SHATTER_FX_PIECE: &str = "code/glass_shatter_piece";

pub fn glass_shatter_fx_name(landing: bool) -> &'static str {
    if landing {
        FX_GLASS_SHATTER_FX_PIECE
    } else {
        FX_GLASS_SHATTER_FX_64
    }
}

pub fn glass_shatter_fx_fallback(landing: bool) -> Option<&'static str> {
    if landing {
        None
    } else {
        Some(FX_GLASS_SHATTER_FX_32)
    }
}

pub const FX_GLASS_LINEAR_VEL_MIN: f32 = 200.0;

pub const FX_GLASS_LINEAR_VEL_MAX: f32 = 400.0;

pub const FX_GLASS_ANGULAR_VEL_MIN: f32 = 5.0;

pub const FX_GLASS_ANGULAR_VEL_MAX: f32 = 35.0;

const FX_GLASS_MASS_AREA_MIN: f32 = 0.7;

const FX_GLASS_MASS_AREA_MAX: f32 = 2048.0;

const FX_GLASS_MASS_RSQRT_SCALE: f32 = 11.313_709_259_033_203;

pub fn glass_shatter_rand(cursor: &mut u32) -> f32 {
    let value = sample_at(0, FxRandomChannel::GlassShatter, u64::from(*cursor));
    *cursor = cursor.wrapping_add(1);
    value
}

pub fn glass_interior_branch_count(rand01: f32) -> u32 {
    let rounded = libm::roundf(rand01 * FX_GLASS_SHATTER_BRANCH_SCALE) as i32;
    3u32.wrapping_sub(rounded as u32).clamp(3, 6)
}

pub fn glass_piece_speed_scale(area_x2: f32) -> f32 {
    let v = area_x2.clamp(FX_GLASS_MASS_AREA_MIN, FX_GLASS_MASS_AREA_MAX);
    let i = 0x5f37_59df_u32.wrapping_sub(v.to_bits() >> 1);
    let y = f32::from_bits(i);
    y * (1.5 - 0.5 * v * y * y) * FX_GLASS_MASS_RSQRT_SCALE
}

pub fn glass_lerp_range(min: f32, max: f32, rand01: f32) -> f32 {
    min + (max - min) * rand01
}

pub fn glass_loop_area_x2(verts: &[[i16; 2]]) -> f32 {
    if verts.len() < 3 {
        return 0.0;
    }
    let mut acc = 0.0f32;
    for i in 0..verts.len() {
        let a = verts[i];
        let b = verts[(i + 1) % verts.len()];
        acc += a[0] as f32 * b[1] as f32 - b[0] as f32 * a[1] as f32;
    }
    acc.abs() * crate::glass::FX_GLASS_VERT_SCALE * crate::glass::FX_GLASS_VERT_SCALE
}

pub fn glass_centroid(verts: &[[i16; 2]]) -> [f32; 2] {
    if verts.is_empty() {
        return [0.0, 0.0];
    }
    let n = verts.len() as f32;
    let mut x = 0.0;
    let mut y = 0.0;
    for v in verts {
        x += v[0] as f32;
        y += v[1] as f32;
    }
    [x / n, y / n]
}

pub fn glass_shard_size_cap(original_area: f32, supported: bool) -> f32 {
    let base = if supported {
        FX_GLASS_FRINGE_MAXSIZE
    } else {
        FX_GLASS_SHARD_MAXSIZE
    };
    let floor = if FX_GLASS_MAX_PIECES_PER_FRAME > 0.0 {
        original_area / FX_GLASS_MAX_PIECES_PER_FRAME
    } else {
        0.0
    };
    if base < floor { floor } else { base }
}

pub fn glass_needs_size_split(area_x2: f32, original_area: f32, supported: bool) -> bool {
    area_x2 > glass_shard_size_cap(original_area, supported)
}

pub fn glass_fringe_cap(original_area: f32) -> f32 {
    FX_GLASS_FRINGE_MAXCOVERAGE * original_area
}

pub fn glass_life_fade(age_msec: i32, life_msec: i32, fade_msec: i32) -> f32 {
    if life_msec <= 0 {
        return 0.0;
    }
    let age = age_msec.max(0);
    if age >= life_msec {
        return 0.0;
    }
    if fade_msec <= 0 {
        return 1.0;
    }
    let remain = life_msec - age;
    if remain >= fade_msec {
        return 1.0;
    }
    remain as f32 / fade_msec as f32
}

pub fn glass_scale_color_alpha(rgba: [u8; 4], fade: f32) -> [u8; 4] {
    let fade = if fade < 0.0 {
        0.0
    } else if fade > 1.0 {
        1.0
    } else {
        fade
    };
    let a = (rgba[3] as f32 * fade) as u8;
    [rgba[0], rgba[1], rgba[2], a]
}

pub fn glass_normalize3(v: [f32; 3]) -> Option<[f32; 3]> {
    let len_sq = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    if len_sq <= 1e-12 {
        return None;
    }
    let inv = 1.0 / libm::sqrtf(len_sq);
    Some([v[0] * inv, v[1] * inv, v[2] * inv])
}

pub fn glass_cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn glass_launch_dir(dir: [f32; 3], pane_normal: [f32; 3]) -> [f32; 3] {
    glass_normalize3(dir)
        .or_else(|| glass_normalize3(pane_normal))
        .unwrap_or([0.0, 0.0, 1.0])
}

pub fn glass_splitmix64(state: &mut u64) -> f32 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 * (1.0 / 16_777_216.0)
}

/// Moves a piece's origin and texture offset by an integer vertex offset.
///
/// Piece geometry is stored about the piece origin on the packed grid, so a shard is
/// shifted onto its own centroid and the place and UV base follow it by the same step.
pub fn glass_recenter_offset(
    offset: [f32; 2],
    origin: [f32; 3],
    axis: [[f32; 3]; 3],
    uv: [f32; 2],
    tex: [[f32; 2]; 2],
) -> ([f32; 3], [f32; 2]) {
    let scale = crate::glass::FX_GLASS_VERT_SCALE;
    let new_origin = [
        origin[0] + (axis[0][0] * offset[0] + axis[1][0] * offset[1]) * scale,
        origin[1] + (axis[0][1] * offset[0] + axis[1][1] * offset[1]) * scale,
        origin[2] + (axis[0][2] * offset[0] + axis[1][2] * offset[1]) * scale,
    ];
    let new_uv = [
        uv[0] + tex[0][0] * offset[0] + tex[0][1] * offset[1],
        uv[1] + tex[1][0] * offset[0] + tex[1][1] * offset[1],
    ];
    (new_origin, new_uv)
}

pub fn glass_support_frac(support_mask: u32, vert_n: u8) -> f32 {
    let edges = vert_n.max(1);
    support_mask.count_ones() as f32 / f32::from(edges)
}

pub fn glass_launch_avel(dir: [f32; 3], pane_axis: [[f32; 3]; 3], ang: f32) -> [f32; 3] {
    let spin = glass_cross3(dir, pane_axis[2]);
    if let Some(axis) = glass_normalize3(spin) {
        [axis[0] * ang, axis[1] * ang, axis[2] * ang]
    } else {
        [
            pane_axis[0][0] * ang,
            pane_axis[0][1] * ang,
            pane_axis[0][2] * ang,
        ]
    }
}
