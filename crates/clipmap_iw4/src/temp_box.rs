use trace_iw4::Trace;

use crate::capsule_query::sweep_point;

struct CapsuleBounds {
    center: [f64; 3],
    radius: f64,
    half_segment: f64,
}

impl CapsuleBounds {
    fn new(mins: [f32; 3], maxs: [f32; 3]) -> Option<Self> {
        if (0..3).any(|i| !mins[i].is_finite() || !maxs[i].is_finite() || mins[i] > maxs[i]) {
            return None;
        }
        let center = core::array::from_fn(|i| (f64::from(mins[i]) + f64::from(maxs[i])) * 0.5);
        let half: [f64; 3] =
            core::array::from_fn(|i| (f64::from(maxs[i]) - f64::from(mins[i])) * 0.5);
        let radius = half[0].min(half[1]).min(half[2]);
        Some(Self {
            center,
            radius,
            half_segment: half[2] - radius,
        })
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "moving hull and temporary model bounds"
)]
pub fn transformed_temp_capsule_trace(
    start: [f32; 3],
    end: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    origin: [f32; 3],
    box_mins: [f32; 3],
    box_maxs: [f32; 3],
    contents: u32,
    mask: u32,
) -> Trace {
    let mut trace = Trace {
        fraction: 1.0,
        endpos: end,
        ..Trace::default()
    };
    if contents & mask == 0 {
        return trace;
    }
    let (Some(moving), Some(fixed)) = (
        CapsuleBounds::new(mins, maxs),
        CapsuleBounds::new(box_mins, box_maxs),
    ) else {
        return trace;
    };
    let relative = |position: [f32; 3]| {
        core::array::from_fn(|i| {
            f64::from(position[i]) + moving.center[i] - f64::from(origin[i]) - fixed.center[i]
        })
    };
    if let Some(hit) = sweep_point(
        relative(start),
        relative(end),
        moving.radius + fixed.radius,
        moving.half_segment + fixed.half_segment,
    ) {
        trace.fraction = hit.fraction as f32;
        trace.normal = hit.normal.map(|x| x as f32);
        trace.startsolid = u8::from(hit.startsolid);
        trace.allsolid = u8::from(hit.allsolid);
        trace.contents = contents;
        trace.walkable = u8::from(!hit.startsolid && trace.normal[2] >= 0.7);
        trace.endpos = core::array::from_fn(|i| {
            (f64::from(start[i]) + (f64::from(end[i]) - f64::from(start[i])) * hit.fraction) as f32
        });
    }
    trace
}
