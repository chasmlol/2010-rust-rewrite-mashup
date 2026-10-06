use trace_iw4::Trace;

use crate::penetration::Vec3;
use crate::recovery::{self, Backend, Config, ContactBuffer, Coverage};
use crate::{CollisionBackend, GroundTraceInput};

const SUPPORT_DEPTH: f32 = 0.5;

#[derive(Clone, Debug, PartialEq)]
pub struct CorrectSolidOutcome {
    pub origin: [f32; 3],
    pub trace: Trace,
}

struct RecoveryBackend<'a, C: CollisionBackend + ?Sized> {
    collision: &'a C,
    input: GroundTraceInput,
}

impl<C: CollisionBackend + ?Sized> Backend for RecoveryBackend<'_, C> {
    fn contacts(&mut self, position: Vec3, contacts: &mut ContactBuffer) -> Coverage {
        let point = <[f64; 3]>::from(position).map(|x| x as f32);
        if !point.iter().all(|x| x.is_finite()) {
            return Coverage::Unsupported;
        }
        self.collision.penetrations(
            GroundTraceInput {
                start: point,
                end: point,
                ..self.input
            },
            contacts,
        )
    }

    fn clear(&mut self, position: Vec3) -> bool {
        let point = <[f64; 3]>::from(position).map(|x| x as f32);
        if !point.iter().all(|x| x.is_finite()) {
            return false;
        }
        let trace = self.collision.trace(GroundTraceInput {
            start: point,
            end: point,
            ..self.input
        });
        trace.startsolid == 0
            && trace.allsolid == 0
            && trace.fraction.is_finite()
            && trace.fraction == 1.0
    }
}

pub fn correct_solid<C: CollisionBackend + ?Sized>(
    origin: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    tracemask: u32,
    collision: &C,
) -> Option<CorrectSolidOutcome> {
    if !origin
        .iter()
        .chain(mins.iter())
        .chain(maxs.iter())
        .all(|x| x.is_finite())
        || (0..3).any(|i| mins[i] > maxs[i])
    {
        return None;
    }
    let config = Config::default();
    let input = GroundTraceInput {
        start: origin,
        end: origin,
        mins,
        maxs,
        tracemask,
    };
    let initial = Vec3::from(origin.map(f64::from));
    let mut backend = RecoveryBackend { collision, input };
    let outcome = recovery::recover(initial, config, &mut backend);
    if !outcome.success() {
        return None;
    }
    let point = <[f64; 3]>::from(outcome.position).map(|x| x as f32);
    let rounded = Vec3::from(point.map(f64::from));
    if (rounded - initial).length() >= config.max_displacement || !backend.clear(rounded) {
        return None;
    }
    let trace = collision.trace(GroundTraceInput {
        start: point,
        end: [point[0], point[1], point[2] - SUPPORT_DEPTH],
        ..input
    });
    Some(CorrectSolidOutcome {
        origin: point,
        trace,
    })
}
