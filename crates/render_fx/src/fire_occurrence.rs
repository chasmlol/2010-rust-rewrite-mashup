use std::collections::HashMap;

use bevy::prelude::Resource;

const CAPACITY: usize = 8192;
const RETENTION_MS: i32 = 5000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FireFxOccurrence {
    Muzzle,
    Brass,
}

#[derive(Resource, Default)]
pub struct PresentedFireFx {
    scope: Option<(Option<u64>, u64)>,
    presented: HashMap<(sim::FireCause, FireFxOccurrence), i32>,
    pub budget_refused: u32,
}

impl PresentedFireFx {
    pub fn may_present(
        &mut self,
        generation: frame::WorldGeneration,
        timeline: u64,
        event: &net::DispatchedEntityEvent,
        occurrence: FireFxOccurrence,
        now: i32,
        verdicts: &net::FireVerdictState,
    ) -> bool {
        if generation != event.world || timeline != event.timeline {
            return false;
        }
        if event.domain == net::EntityEventDomain::Predicted
            && event.payload.fire_cause.is_some_and(|cause| {
                verdicts.status(event.world, cause) == net::PredictedFireStatus::Refused
            })
        {
            return false;
        }
        let scope = (generation.0, timeline);
        if self.scope != Some(scope) {
            self.scope = Some(scope);
            self.presented.clear();
        }
        self.presented.retain(|_, at| {
            let age = now.wrapping_sub(*at);
            age < RETENTION_MS
        });
        let Some(cause) = event.payload.fire_cause else {
            return true;
        };
        if self.presented.contains_key(&(cause, occurrence)) {
            return false;
        }
        if self.presented.len() == CAPACITY {
            self.budget_refused = self.budget_refused.saturating_add(1);
            return false;
        }
        true
    }

    pub fn presented(
        &mut self,
        event: &net::DispatchedEntityEvent,
        occurrence: FireFxOccurrence,
        now: i32,
    ) {
        if let Some(cause) = event.payload.fire_cause {
            self.presented.insert((cause, occurrence), now);
        }
    }
}
