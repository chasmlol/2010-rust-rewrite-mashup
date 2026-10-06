use crate::at_rest::FX_ON_GROUND_NORMAL_Z;
use crate::origin::sample_float_range;
use crate::random::{FxRandomChannel, sample_f32};
use crate::vec::vec3_length_sq;

pub const FX_COLLIDE_SUBSTEP_MS: i32 = 0x32;

pub const FX_TRACE_MASK: u32 = 0x811;

pub const FX_TRACE_MASK_ITEM_CLIP: u32 = 0xc11;

pub const FX_IMPACT_CHILD_MIN_SPEED_SQ: f32 = 1.0;

pub const FX_COLLISION_REFLECT_SCALE: f64 = -2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxCollideSubstep {
    pub msec_start: i32,
    pub msec_end: i32,
}

#[derive(Clone, Debug)]
pub struct FxCollideSubstepIter {
    cur: i32,
    end: i32,
    done: bool,
}

impl Iterator for FxCollideSubstepIter {
    type Item = FxCollideSubstep;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done || self.cur >= self.end {
            self.done = true;
            return None;
        }
        let next_cut = self.cur.wrapping_add(FX_COLLIDE_SUBSTEP_MS);
        if next_cut < self.end {
            let out = FxCollideSubstep {
                msec_start: self.cur,
                msec_end: next_cut,
            };
            self.cur = next_cut;
            Some(out)
        } else {
            let out = FxCollideSubstep {
                msec_start: self.cur,
                msec_end: self.end,
            };
            self.done = true;
            Some(out)
        }
    }
}

#[inline]
pub fn collide_substep_schedule(msec_start: i32, msec_end: i32) -> FxCollideSubstepIter {
    FxCollideSubstepIter {
        cur: msec_start,
        end: msec_end,
        done: msec_end <= msec_start,
    }
}

#[inline]
pub const fn trace_mask(use_item_clip: bool) -> u32 {
    if use_item_clip {
        FX_TRACE_MASK_ITEM_CLIP
    } else {
        FX_TRACE_MASK
    }
}

#[inline]
pub fn impact_child_speed_allows(pre_impact_speed_sq: f32) -> bool {
    pre_impact_speed_sq > FX_IMPACT_CHILD_MIN_SPEED_SQ
}

#[inline]
pub fn sample_reflection_factor(base: f32, amplitude: f32, seed: u64) -> f32 {
    sample_float_range(
        base,
        amplitude,
        sample_f32(seed, FxRandomChannel::Reflection),
    )
}

#[inline]
pub fn collide_on_ground(normal_z: f32) -> bool {
    normal_z > FX_ON_GROUND_NORMAL_Z
}

#[inline]
pub fn collide_marks_at_rest(scaled_vel: [f32; 3], normal_z: f32) -> bool {
    collide_on_ground(normal_z) && !impact_child_speed_allows(vec3_length_sq(scaled_vel))
}

#[inline]
pub fn collision_reflect_base_vel_delta(
    pre_impact_vel: [f32; 3],
    normal: [f32; 3],
    reflection_factor: f32,
) -> [f32; 3] {
    let scaled = [
        pre_impact_vel[0] * reflection_factor,
        pre_impact_vel[1] * reflection_factor,
        pre_impact_vel[2] * reflection_factor,
    ];
    let dot = scaled[0] * normal[0] + scaled[1] * normal[1] + scaled[2] * normal[2];
    let f = (FX_COLLISION_REFLECT_SCALE as f32) * dot;
    let reflected = [
        scaled[0] + f * normal[0],
        scaled[1] + f * normal[1],
        scaled[2] + f * normal[2],
    ];
    [
        reflected[0] - pre_impact_vel[0],
        reflected[1] - pre_impact_vel[1],
        reflected[2] - pre_impact_vel[2],
    ]
}
