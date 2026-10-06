use trace_iw4::{BrushRef, Trace, trace_capsule};

use crate::BrushView;

#[derive(Clone, Copy, Debug)]
pub struct ClipCmodel {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub radius: f32,
    pub first_brush: u32,
    pub num_brushes: u16,
}

pub fn clip_handle_to_model(cmodels: &[ClipCmodel], handle: u32) -> Option<&ClipCmodel> {
    cmodels.get(handle as usize)
}

pub fn transformed_capsule_trace<B: BrushView>(
    cmodel: &ClipCmodel,
    leafbrushes: &[u16],
    brushes: &[B],
    start: [f32; 3],
    end: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    origin: [f32; 3],
    angles: [f32; 3],
    mask: u32,
) -> Trace {
    if angles[0] == 0.0 && angles[1] == 0.0 && angles[2] == 0.0 {
        return transformed_capsule_trace_origin_only(
            cmodel,
            leafbrushes,
            brushes,
            start,
            end,
            mins,
            maxs,
            origin,
            mask,
        );
    }

    transformed_capsule_trace_rotated(
        cmodel,
        leafbrushes,
        brushes,
        start,
        end,
        mins,
        maxs,
        origin,
        angles,
        mask,
    )
}

fn transformed_capsule_trace_origin_only<B: BrushView>(
    cmodel: &ClipCmodel,
    leafbrushes: &[u16],
    brushes: &[B],
    start: [f32; 3],
    end: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    origin: [f32; 3],
    mask: u32,
) -> Trace {
    let local_start = [
        start[0] - origin[0],
        start[1] - origin[1],
        start[2] - origin[2],
    ];
    let local_end = [end[0] - origin[0], end[1] - origin[1], end[2] - origin[2]];
    let mut hit = capsule_vs_cmodel(
        cmodel,
        leafbrushes,
        brushes,
        local_start,
        local_end,
        mins,
        maxs,
        mask,
    );
    hit.endpos = [
        hit.endpos[0] + origin[0],
        hit.endpos[1] + origin[1],
        hit.endpos[2] + origin[2],
    ];
    hit
}

fn transformed_capsule_trace_rotated<B: BrushView>(
    cmodel: &ClipCmodel,
    leafbrushes: &[u16],
    brushes: &[B],
    start: [f32; 3],
    end: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    origin: [f32; 3],
    angles: [f32; 3],
    mask: u32,
) -> Trace {
    let matrix = angles_to_axis(angles);
    let mut start_l = [
        start[0] - origin[0],
        start[1] - origin[1],
        start[2] - origin[2],
    ];
    let mut end_l = [end[0] - origin[0], end[1] - origin[1], end[2] - origin[2]];
    rotate_point(&mut start_l, &matrix);
    rotate_point(&mut end_l, &matrix);

    let mut hit = capsule_vs_cmodel(
        cmodel,
        leafbrushes,
        brushes,
        start_l,
        end_l,
        mins,
        maxs,
        mask,
    );
    if hit.fraction < 1.0 {
        let transpose = transpose_matrix(&matrix);
        rotate_point(&mut hit.normal, &transpose);
    }
    let inv = transpose_matrix(&matrix);
    rotate_point(&mut hit.endpos, &inv);
    hit.endpos = [
        hit.endpos[0] + origin[0],
        hit.endpos[1] + origin[1],
        hit.endpos[2] + origin[2],
    ];
    hit
}

fn capsule_vs_cmodel<B: BrushView>(
    cmodel: &ClipCmodel,
    leafbrushes: &[u16],
    brushes: &[B],
    start: [f32; 3],
    end: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    mask: u32,
) -> Trace {
    let first = cmodel.first_brush as usize;
    let last = first.saturating_add(cmodel.num_brushes as usize);
    let ids = leafbrushes.get(first..last).unwrap_or(&[]);
    let selected: alloc::vec::Vec<BrushRef<'_>> = ids
        .iter()
        .filter_map(|&id| brushes.get(id as usize))
        .map(crate::brush_ref)
        .collect();
    trace_capsule(selected.iter().copied(), start, end, mins, maxs, mask)
}

fn angles_to_axis(angles: [f32; 3]) -> [[f32; 3]; 3] {
    const DEG2RAD: f32 = 0.01745329238474369_f32;
    let yaw = angles[1] * DEG2RAD;
    let pitch = angles[0] * DEG2RAD;
    let roll = angles[2] * DEG2RAD;
    let cy = libm::cosf(yaw);
    let sy = libm::sinf(yaw);
    let cp = libm::cosf(pitch);
    let sp = libm::sinf(pitch);
    let cr = libm::cosf(roll);
    let sr = libm::sinf(roll);
    [
        [cp * cy, cp * sy, -sp],
        [sr * sp * cy + -sy * cr, sr * sp * sy + cr * cy, sr * cp],
        [cr * sp * cy + -sr * -sy, cr * sp * sy + -sr * cy, cr * cp],
    ]
}

fn rotate_point(point: &mut [f32; 3], mat: &[[f32; 3]; 3]) {
    let x = point[0];
    let y = point[1];
    let z = point[2];
    point[0] = mat[0][0] * x + mat[0][1] * y + mat[0][2] * z;
    point[1] = mat[1][0] * x + mat[1][1] * y + mat[1][2] * z;
    point[2] = mat[2][0] * x + mat[2][1] * y + mat[2][2] * z;
}

fn transpose_matrix(mat: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    [
        [mat[0][0], mat[1][0], mat[2][0]],
        [mat[0][1], mat[1][1], mat[2][1]],
        [mat[0][2], mat[1][2], mat[2][2]],
    ]
}

#[derive(Clone, Copy, Debug)]
pub struct CmodelTransform {
    origin: [f32; 3],
    matrix: [[f32; 3]; 3],
}

impl CmodelTransform {
    pub fn new(origin: [f32; 3], angles: [f32; 3]) -> Option<Self> {
        if !origin.iter().chain(angles.iter()).all(|v| v.is_finite()) {
            return None;
        }
        Some(Self {
            origin,
            matrix: angles_to_axis(angles),
        })
    }

    pub fn local_point(self, point: [f32; 3]) -> [f32; 3] {
        let mut local = core::array::from_fn(|i| point[i] - self.origin[i]);
        rotate_point(&mut local, &self.matrix);
        local
    }

    pub fn world_direction(self, mut direction: [f64; 3]) -> [f64; 3] {
        let local = direction;
        for (i, value) in direction.iter_mut().enumerate() {
            *value = (0..3)
                .map(|j| f64::from(self.matrix[j][i]) * local[j])
                .sum();
        }
        direction
    }
}
