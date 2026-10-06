use std::f32::consts::FRAC_PI_4;
use std::sync::Arc;
use std::sync::Mutex;

use crate::attenuation::distance_attenuation;

#[derive(Clone, Copy)]
pub(crate) struct ListenerSnapshot {
    pub origin_inches: [f32; 3],
    pub right: [f32; 3],
}

#[derive(Default)]
pub(crate) struct ListenerState(Mutex<Option<ListenerSnapshot>>);

impl ListenerState {
    pub(crate) fn set(&self, listener: Option<ListenerSnapshot>) {
        *self.0.lock().unwrap_or_else(|poison| poison.into_inner()) = listener.filter(|listener| {
            listener
                .origin_inches
                .iter()
                .chain(&listener.right)
                .all(|value| value.is_finite())
        });
    }

    pub(crate) fn get(&self) -> Option<ListenerSnapshot> {
        *self.0.lock().unwrap_or_else(|poison| poison.into_inner())
    }
}

#[derive(Clone)]
pub(crate) struct SpatialSource {
    pub origin_inches: [f32; 3],
    pub dist_min: f32,
    pub dist_max: f32,
    pub knots: Arc<[[f32; 2]]>,
    pub near_knots: Option<Arc<[[f32; 2]]>>,
    pub priority: Option<asset_audio::VoicePriority>,
    pub base_volume: f32,
}

pub(crate) struct SpatialParameters {
    pub gains: [f32; 2],
    pub priority: f32,
}

impl SpatialSource {
    pub(crate) fn evaluate(&self, listener: ListenerSnapshot) -> SpatialParameters {
        let offset =
            std::array::from_fn::<_, 3, _>(|i| self.origin_inches[i] - listener.origin_inches[i]);
        let distance = offset.iter().map(|value| value * value).sum::<f32>().sqrt();
        let attenuation = if self.knots.is_empty() {
            0.0
        } else {
            distance_attenuation(
                &self.knots,
                self.near_knots.as_deref(),
                distance,
                self.dist_min,
                self.dist_max,
            )
        };
        let gain = self.base_volume * attenuation;
        let gain = if distance.is_finite() && attenuation >= 0.0 && gain.is_finite() {
            gain.max(0.0)
        } else {
            0.0
        };
        let (left, right) = channel_gains(
            listener.origin_inches,
            listener.right,
            self.origin_inches,
            gain,
        );
        SpatialParameters {
            gains: [left, right],
            priority: if distance.is_finite() {
                self.priority
                    .as_ref()
                    .map_or(0.0, |priority| priority.evaluate(Some(distance)))
            } else {
                0.0
            },
        }
    }
}

pub(crate) fn pan(ear: [f32; 3], right: [f32; 3], emitter: [f32; 3]) -> f32 {
    let offset = std::array::from_fn::<_, 3, _>(|i| emitter[i] - ear[i]);
    let length_squared = offset.iter().map(|value| value * value).sum::<f32>();
    if !length_squared.is_finite() || length_squared < 1e-8 {
        return 0.0;
    }
    let inverse_length = length_squared.sqrt().recip();
    let projection = offset
        .iter()
        .zip(right)
        .map(|(value, right)| value * inverse_length * right)
        .sum::<f32>();
    if projection.is_finite() {
        projection.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

pub(crate) fn channel_gains(
    ear: [f32; 3],
    right: [f32; 3],
    emitter: [f32; 3],
    gain: f32,
) -> (f32, f32) {
    let angle = (pan(ear, right, emitter) + 1.0) * FRAC_PI_4;
    (gain * angle.cos(), gain * angle.sin())
}
