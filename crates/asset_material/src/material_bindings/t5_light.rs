use super::local_light::{LightOperation, LocalLightWriter, MaterialLocalLightInputs};
use bevy::math::Vec3;
fn float4_bits(row: [f32; 4]) -> [u32; 4] {
    row.map(f32::to_bits)
}
const CODE_LIGHT_DIFFUSE: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_DIFFUSE;
const CODE_LIGHT_SPECULAR: u16 = lighting_iw4::CONST_SRC_CODE_LIGHT_SPECULAR;
const CODE_LEFTOVER_T5_LIGHT_ATTENUATION: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_LIGHT_ATTENUATION;

const CODE_LEFTOVER_T5_LIGHT_FALLOFF_A: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_LIGHT_FALLOFF_A;

const CODE_LEFTOVER_T5_LIGHT_FALLOFF_B: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_LIGHT_FALLOFF_B;

const CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX0: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_LIGHT_SPOT_MATRIX0;

const CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX1: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_LIGHT_SPOT_MATRIX1;

const CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX2: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_LIGHT_SPOT_MATRIX2;

const CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX3: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_LIGHT_SPOT_MATRIX3;

const CODE_LEFTOVER_T5_LIGHT_SPOT_AABB: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_LIGHT_SPOT_AABB;

const CODE_LEFTOVER_T5_LIGHT_CONE_CONTROL1: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_LIGHT_CONE_CONTROL1;

const CODE_LEFTOVER_T5_LIGHT_CONE_CONTROL2: u16 =
    crate::t5_code_remap::LEFTOVER_T5_CODE_BASE + crate::t5_code_remap::T5_CODE_LIGHT_CONE_CONTROL2;

const CODE_LEFTOVER_T5_LIGHT_SPOT_COOKIE_SLIDE: u16 = crate::t5_code_remap::LEFTOVER_T5_CODE_BASE
    + crate::t5_code_remap::T5_CODE_LIGHT_SPOT_COOKIE_SLIDE;
const T5_LIGHT_ATTENUATION_EPSILON: f32 = 0.000015287891;

const T5_LIGHT_ATTENUATION_DEFAULT: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

const T5_LIGHT_FALLOFF_NEAR: f32 = 0.0;

const T5_LIGHT_AABB_DEFAULT: [f32; 4] = [0.75, 1.0, 0.75, 1.0];

const T5_LIGHT_SPOT_ROLL_DEFAULT: f32 = 0.0;

const T5_LIGHT_COOKIE_DEFAULT: [f32; 4] = [0.0, 0.0, 0.0, 0.0];

pub(super) fn produce(
    sources: &mut LocalLightWriter<'_>,
    light: &MaterialLocalLightInputs,
    eye: Vec3,
    float_time: f32,
    operation: LightOperation,
) {
    let pack = Some(&light.overrides);
    let omni_or_spot = light.light_type == lighting_iw4::GFX_LIGHT_TYPE_OMNI
        || light.light_type == lighting_iw4::GFX_LIGHT_TYPE_SPOT;
    let is_spot = light.light_type == lighting_iw4::GFX_LIGHT_TYPE_SPOT;
    match operation {
        LightOperation::Colors => {
            for (index, value) in [
                (CODE_LIGHT_DIFFUSE, pack.and_then(|p| p.diffuse)),
                (CODE_LIGHT_SPECULAR, pack.and_then(|p| p.specular)),
            ] {
                if let Some(v) = value {
                    sources.set_constant_rows(index, &[float4_bits([v[0], v[1], v[2], 1.0])]);
                }
            }
        }
        LightOperation::Attenuation => {
            if let Some(attenuation) = pack.and_then(|pack| pack.attenuation) {
                sources.set_constant_rows(
                    CODE_LEFTOVER_T5_LIGHT_ATTENUATION,
                    &[float4_bits(t5_light_attenuation_row(attenuation))],
                );
            } else if omni_or_spot {
                sources.set_constant_rows(
                    CODE_LEFTOVER_T5_LIGHT_ATTENUATION,
                    &[float4_bits(t5_light_attenuation_row(
                        T5_LIGHT_ATTENUATION_DEFAULT,
                    ))],
                );
            }
        }
        LightOperation::Falloff => {
            if !omni_or_spot {
                return;
            }
            let falloff = pack.and_then(|pack| pack.falloff).unwrap_or([
                T5_LIGHT_FALLOFF_NEAR,
                light.radius,
                0.0,
                0.0,
            ]);
            let edges = t5_light_falloff_edges(falloff);
            if light.light_type == lighting_iw4::GFX_LIGHT_TYPE_OMNI {
                sources.set_constant_rows(
                    CODE_LEFTOVER_T5_LIGHT_FALLOFF_A,
                    &[float4_bits([
                        edges.s_add,
                        edges.e_add,
                        edges.e_mul,
                        edges.rs,
                    ])],
                );
                return;
            }
            let a_ab_b = pack
                .and_then(|pack| pack.cone_bounds)
                .unwrap_or(T5_LIGHT_AABB_DEFAULT);
            if let Some((falloff_a, falloff_b)) = t5_spot_falloff_rows(edges, a_ab_b) {
                sources
                    .set_constant_rows(CODE_LEFTOVER_T5_LIGHT_FALLOFF_A, &[float4_bits(falloff_a)]);
                sources
                    .set_constant_rows(CODE_LEFTOVER_T5_LIGHT_FALLOFF_B, &[float4_bits(falloff_b)]);
            }
        }
        LightOperation::SpotBounds => {
            if !is_spot {
                return;
            }
            let a_ab_b = pack
                .and_then(|pack| pack.cone_bounds)
                .unwrap_or(T5_LIGHT_AABB_DEFAULT);
            sources.set_constant_rows(CODE_LEFTOVER_T5_LIGHT_SPOT_AABB, &[float4_bits(a_ab_b)]);
            sources.set_constant_rows(
                CODE_LEFTOVER_T5_LIGHT_CONE_CONTROL2,
                &[float4_bits(t5_spot_cone_control2(a_ab_b))],
            );
        }
        LightOperation::ConeControl => {
            if !is_spot {
                return;
            }
            let attenuation = pack
                .and_then(|pack| pack.attenuation)
                .unwrap_or(T5_LIGHT_ATTENUATION_DEFAULT);
            sources.set_constant_rows(
                CODE_LEFTOVER_T5_LIGHT_CONE_CONTROL1,
                &[float4_bits(t5_spot_cone_control1(attenuation))],
            );
        }
        LightOperation::Cookie => {
            if !is_spot {
                return;
            }
            let (cookie0, cookie1, cookie2) = match pack {
                Some(pack)
                    if pack.cookie0.is_some()
                        && pack.cookie1.is_some()
                        && pack.cookie2.is_some() =>
                {
                    (
                        pack.cookie0.unwrap(),
                        pack.cookie1.unwrap(),
                        pack.cookie2.unwrap(),
                    )
                }
                _ => (
                    T5_LIGHT_COOKIE_DEFAULT,
                    T5_LIGHT_COOKIE_DEFAULT,
                    T5_LIGHT_COOKIE_DEFAULT,
                ),
            };
            sources.set_constant_rows(
                CODE_LEFTOVER_T5_LIGHT_SPOT_COOKIE_SLIDE,
                &[float4_bits(t5_spot_cookie_slide(
                    cookie0, cookie1, cookie2, float_time,
                ))],
            );
        }
        LightOperation::Matrix => {
            if !is_spot {
                return;
            }
            let falloff = pack.and_then(|pack| pack.falloff).unwrap_or([
                T5_LIGHT_FALLOFF_NEAR,
                light.radius,
                0.0,
                0.0,
            ]);
            let angle_z = pack
                .and_then(|pack| pack.rotation)
                .unwrap_or(T5_LIGHT_SPOT_ROLL_DEFAULT);
            let columns = t5_spot_matrix_columns(
                light.direction,
                angle_z,
                light.cos_outer,
                falloff[0],
                falloff[1],
                [
                    light.origin[0] - eye.x,
                    light.origin[1] - eye.y,
                    light.origin[2] - eye.z,
                ],
            );
            sources.set_constant_rows(
                CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX0,
                &[float4_bits(columns[0])],
            );
            sources.set_constant_rows(
                CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX1,
                &[float4_bits(columns[1])],
            );
            sources.set_constant_rows(
                CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX2,
                &[float4_bits(columns[2])],
            );
            sources.set_constant_rows(
                CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX3,
                &[float4_bits(columns[3])],
            );
        }
    }
}

fn t5_spot_cone_control1(attenuation: [f32; 4]) -> [f32; 4] {
    let v44 = if attenuation[3] == 0.0 {
        1.0
    } else {
        2.0 / attenuation[3]
    };
    [v44, -1.0 / v44, 1.0, attenuation[3]]
}

fn t5_spot_cone_control2(a_ab_b: [f32; 4]) -> [f32; 4] {
    [a_ab_b[0] * a_ab_b[2], a_ab_b[1] * a_ab_b[3], -2.0, 3.0]
}

fn t5_spot_cookie_slide(
    cookie0: [f32; 4],
    cookie1: [f32; 4],
    cookie2: [f32; 4],
    float_time: f32,
) -> [f32; 4] {
    let rc = cookie2[3] * float_time + cookie2[2];
    let (v63, v9) = rc.sin_cos();
    let sy = (cookie2[0] - v9 * cookie2[0]) + v63 * cookie2[1];
    let mx00 = (cookie2[1] - v63 * cookie2[0]) - v9 * cookie2[1];
    let mx10 = cookie1[2] * float_time + cookie1[0];
    let mx20 = cookie1[3] * float_time + cookie1[0];
    let mut mx01 = v9 * cookie0[2];
    let mut mx11 = (-v63) * cookie0[2];
    let mx21 = sy * cookie0[2] + mx10;
    let mut v53 = v63 * cookie0[3];
    let mut v52 = v9 * cookie0[3];
    let mut a_a_mul = mx00 * cookie0[3] + mx20;
    v53 = mx01 * cookie0[1] + v53;
    v52 = mx11 * cookie0[1] + v52;
    a_a_mul = mx21 * cookie0[1] + a_a_mul;
    mx01 = v53 * cookie0[0] + mx01;
    mx11 = v52 * cookie0[0] + mx11;
    let _mx21 = a_a_mul * cookie0[0] + mx21;
    [mx01 * 0.5, mx11 * 0.5, v53 * 0.5, v52 * 0.5]
}

fn t5_spot_matrix_columns(
    direction: [f32; 3],
    rotation: f32,
    cos_fov: f32,
    z_near: f32,
    z_far: f32,
    relative_origin: [f32; 3],
) -> [[f32; 4]; 4] {
    let mut view = spot_light_view_matrix(direction, rotation);
    view[3][0] = -(relative_origin[0] * view[0][0]
        + relative_origin[1] * view[1][0]
        + relative_origin[2] * view[2][0]);
    view[3][1] = -(relative_origin[0] * view[0][1]
        + relative_origin[1] * view[1][1]
        + relative_origin[2] * view[2][1]);
    view[3][2] = -(relative_origin[0] * view[0][2]
        + relative_origin[1] * view[1][2]
        + relative_origin[2] * view[2][2]);
    let proj = spot_light_projection_matrix(cos_fov, z_near, z_far);
    let product = mat4_mul_row_major(view, proj);
    [
        [product[0][0], product[1][0], product[2][0], product[3][0]],
        [product[0][1], product[1][1], product[2][1], product[3][1]],
        [product[0][2], product[1][2], product[2][2], product[3][2]],
        [product[0][3], product[1][3], product[2][3], product[3][3]],
    ]
}

fn spot_light_projection_matrix(cos_fov: f32, z_near: f32, z_far: f32) -> [[f32; 4]; 4] {
    let mut matrix = [[0.0f32; 4]; 4];
    let near = if z_near >= 0.001 { z_near } else { 0.001 };
    let q = z_far / (z_far - near);
    let cotan = 1.0 / ((1.0 - cos_fov * cos_fov).sqrt() / cos_fov);
    matrix[0][0] = cotan;
    matrix[1][1] = cotan;
    matrix[2][2] = q;
    matrix[2][3] = 1.0;
    matrix[3][2] = -q * near;
    matrix
}

fn spot_light_view_matrix(direction: [f32; 3], rotation: f32) -> [[f32; 4]; 4] {
    let forward = Vec3::new(-direction[0], -direction[1], -direction[2]).normalize_or_zero();
    let up = perpendicular_vector(forward);
    let right = up.cross(forward).normalize_or_zero();
    let up = up.normalize_or_zero();
    let (sin, cos) = rotation.sin_cos();
    let rotated_right = right * cos - up * sin;
    let rotated_up = right * sin + up * cos;
    [
        [rotated_right.x, rotated_up.x, forward.x, 0.0],
        [rotated_right.y, rotated_up.y, forward.y, 0.0],
        [rotated_right.z, rotated_up.z, forward.z, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn perpendicular_vector(src: Vec3) -> Vec3 {
    let src_sq = [src.x * src.x, src.y * src.y, src.z * src.z];
    let mut pos = usize::from(src_sq[0] > src_sq[1]);
    if src_sq[pos] > src_sq[2] {
        pos = 2;
    }
    let d = -src[pos];
    let mut dst = src * d;
    dst[pos] += 1.0;
    dst.normalize_or_zero()
}

fn mat4_mul_row_major(left: [[f32; 4]; 4], right: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0f32; 4]; 4];
    for row in 0..4 {
        for col in 0..4 {
            out[row][col] = left[row][0] * right[0][col]
                + left[row][1] * right[1][col]
                + left[row][2] * right[2][col]
                + left[row][3] * right[3][col];
        }
    }
    out
}

fn t5_light_attenuation_row(attenuation: [f32; 4]) -> [f32; 4] {
    [
        attenuation[0] + T5_LIGHT_ATTENUATION_EPSILON,
        attenuation[1],
        attenuation[2],
        attenuation[3],
    ]
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct T5FalloffEdges {
    s_add: f32,
    e_mul: f32,
    e_add: f32,
    rs: f32,
}

fn t5_light_falloff_edges(falloff: [f32; 4]) -> T5FalloffEdges {
    let far_edge = falloff[0];
    let v75 = falloff[1];
    let s_mul = falloff[2];
    let v73 = falloff[3];
    let (s_add, e_mul) = if far_edge == s_mul {
        (1.0, -far_edge)
    } else {
        let inv = 1.0 / (s_mul - far_edge);
        (inv, -far_edge * inv)
    };
    let (e_add, rs) = if v73 == v75 {
        (-1.0, v75)
    } else {
        let inv = 1.0 / (v73 - v75);
        (inv, -v75 * inv)
    };
    T5FalloffEdges {
        s_add,
        e_mul,
        e_add,
        rs,
    }
}

fn t5_spot_falloff_rows(edges: T5FalloffEdges, a_ab_b: [f32; 4]) -> Option<([f32; 4], [f32; 4])> {
    let den0 = a_ab_b[0] - a_ab_b[1];
    let den1 = a_ab_b[2] - a_ab_b[3];
    if den0 == 0.0 || den1 == 0.0 {
        return None;
    }
    let bb_add = 1.0 / den0;
    let v47 = -a_ab_b[1] * bb_add;
    let re = 1.0 / den1;
    let v45 = -a_ab_b[3] * re;
    Some((
        [edges.s_add, re, bb_add, edges.e_add],
        [edges.e_mul, v45, v47, edges.rs],
    ))
}

pub(super) fn prepare(requested: &[u16]) -> Vec<LightOperation> {
    let candidates: &[(LightOperation, &[u16])] = &[
        (
            LightOperation::Colors,
            &[CODE_LIGHT_DIFFUSE, CODE_LIGHT_SPECULAR],
        ),
        (
            LightOperation::Attenuation,
            &[CODE_LEFTOVER_T5_LIGHT_ATTENUATION],
        ),
        (
            LightOperation::Falloff,
            &[
                CODE_LEFTOVER_T5_LIGHT_FALLOFF_A,
                CODE_LEFTOVER_T5_LIGHT_FALLOFF_B,
            ],
        ),
        (
            LightOperation::SpotBounds,
            &[
                CODE_LEFTOVER_T5_LIGHT_SPOT_AABB,
                CODE_LEFTOVER_T5_LIGHT_CONE_CONTROL2,
            ],
        ),
        (
            LightOperation::ConeControl,
            &[CODE_LEFTOVER_T5_LIGHT_CONE_CONTROL1],
        ),
        (
            LightOperation::Cookie,
            &[CODE_LEFTOVER_T5_LIGHT_SPOT_COOKIE_SLIDE],
        ),
        (
            LightOperation::Matrix,
            &[
                CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX0,
                CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX1,
                CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX2,
                CODE_LEFTOVER_T5_LIGHT_SPOT_MATRIX3,
            ],
        ),
    ];
    candidates
        .iter()
        .filter_map(|(op, indices)| super::demands_any(requested, indices).then_some(*op))
        .collect()
}
