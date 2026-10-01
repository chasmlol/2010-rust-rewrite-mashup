//!82D6BF90; S2 82DBB358 verifies packet/index-shift roles, not S3's new cone.
use super::{Candidate, DT, MAX_CANDIDATES, Packet, Settings, Vector, math::*};
use crate::air::trajectory::Trajectory;
pub(super) fn prepare(
    p: Packet,
    gravity: Vector,
    s: Settings,
) -> Result<(Vec<Candidate>, Vector, Vector), &'static str> {
    let count = p
        .kind_108
        .checked_add(p.kind_112)
        .ok_or("BipedAir candidate count overflow")? as usize;
    if p.kind_108 == 0
        || count > MAX_CANDIDATES
        || !s.height.is_finite()
        || !s.sphere_radius.is_finite()
        || s.sphere_radius <= 0.
    {
        return Err("Invalid authored BipedAir selector settings");
    }
    for (message, value) in [
        ("Nonfinite BipedAir launch packet: scalar_96", p.scalar_96),
        ("Nonfinite BipedAir launch packet: scalar_100", p.scalar_100),
        ("Nonfinite BipedAir launch packet: scalar_104", p.scalar_104),
    ] {
        if !value.is_finite() {
            return Err(message);
        }
    }
    for (message, vector) in [
        ("Nonfinite BipedAir launch packet: velocity_0", p.velocity_0),
        (
            "Nonfinite BipedAir launch packet: secondary_velocity_16",
            p.secondary_velocity_16,
        ),
        ("Nonfinite BipedAir launch packet: position_32", p.position_32),
        ("Nonfinite BipedAir launch packet: up_48", p.up_48),
        ("Nonfinite BipedAir launch packet: forward_64", p.forward_64),
        (
            "Nonfinite BipedAir launch packet: board_position_80",
            p.board_position_80,
        ),
        ("Nonfinite BipedAir launch packet: gravity", gravity),
    ] {
        if vector.into_iter().any(|x| !x.is_finite()) {
            return Err(message);
        }
    }
    let initial = scale(UP, -(s.height - s.sphere_radius));
    let mut correction = [0., 0.1, 0., 0.];
    if p.has_board_position_116 {
        correction = add(
            correction,
            sub(p.board_position_80, add(p.position_32, initial)),
        );
    }
    let offset = add(initial, correction);
    let base = sub(p.velocity_0, scale(correction, reciprocal(1.)));
    let mut velocities = vec![base; count];
    if count > 1 {
        let speed = length(p.velocity_0);
        let (axis, right) = if speed >= 0.001 {
            let axis = scale(p.velocity_0, reciprocal(speed));
            let mut right = cross(UP, axis);
            if length(right) < 0.1 {
                right = cross(axis, p.forward_64);
            }
            (axis, scale(right, reciprocal(length(right))))
        } else {
            (UP, scale(p.forward_64, -1.))
        };
        let vertical = cross(axis, right);
        //8252D980 is tan; Biped launch already supplies radians.
        let tangents = [p.scalar_96.tan(), p.scalar_100.tan(), p.scalar_104.tan()];
        let base_speed = length(base);
        let mut minimum = length(flatten(p.velocity_0));
        for i in 1..p.kind_108 as usize {
            let angle = (1. - (i as f32 / (p.kind_108 - 1) as f32) * 2.) * std::f32::consts::PI;
            let sin = crate::trigonometry::sin(angle);
            let cos = crate::trigonometry::cos(angle);
            let cone = if cos <= 0. { tangents[1] } else { tangents[2] };
            let v = madd(vertical, cos * cone, madd(right, sin * tangents[0], base));
            velocities[i] = scale(v, base_speed * reciprocal(length(v)));
            minimum = minimum.min(length(flatten(velocities[i])));
        }
        if p.kind_112 > 0 {
            let secondary = flatten(p.secondary_velocity_16);
            let n = length(secondary);
            let direction = if n > 0.01 {
                scale(secondary, reciprocal(n))
            } else {
                normalize_or(p.forward_64, [0.; 4])
            };
            let increment = minimum.max(1.).min(4.) / (p.kind_112 + 1) as f32;
            let mut speed = 0.;
            //Original822F9444=41A95811: fixed native secondary vertical seed.
            let square = f32::from_bits(0x41a95811);
            let vertical =
                square * crate::physics::board_motion_output::inverse_length_squared(square, 2);
            for i in p.kind_108 as usize..count {
                speed += increment;
                velocities[i] = madd(direction, speed, [0., vertical, 0., 0.]);
            }
        }
    }
    let mut candidates = Vec::with_capacity(count);
    for velocity in velocities {
        let mut trajectory = Trajectory {
            position: add(p.position_32, offset),
            velocity,
            acceleration: gravity,
            duration: 2.,
        };
        shift(&mut trajectory, s.start_index as f32 * DT);
        super::validate_request(super::request(trajectory, s.sphere_radius))?;
        let mut c = Candidate::reset();
        c.trajectory = trajectory;
        c.start_frame_112 = s.start_index;
        candidates.push(c);
    }
    Ok((candidates, offset, initial))
}
