use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::prelude::Resource;

const CAPACITY: usize = 8192;
const RETENTION: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PredictedFireStatus {
    Pending,
    Confirmed,
    Refused,
}

#[derive(Default)]
struct Verdicts {
    world: frame::WorldGeneration,
    results: HashMap<(sim::ClientId, sim::CommandSequence), (sim::FireCommandResult, Instant)>,
}

#[derive(Resource, Clone, Default)]
pub struct FireVerdictState(Arc<Mutex<Verdicts>>);

impl FireVerdictState {
    pub(crate) fn apply(
        &self,
        world: frame::WorldGeneration,
        results: &[sim::FireCommandResult],
    ) -> Result<(), &'static str> {
        let mut state = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        if state.world != world {
            state.world = world;
            state.results.clear();
        }
        let now = Instant::now();
        state
            .results
            .retain(|_, (_, at)| now.duration_since(*at) < RETENTION);
        let mut additions = HashMap::new();
        for result in results {
            let key = (result.client, result.command);
            if let Some(previous) = state
                .results
                .get(&key)
                .map(|(result, _)| result)
                .or_else(|| additions.get(&key))
            {
                if previous != result {
                    return Err("FireResultConflict: connection retired");
                }
            } else {
                if state.results.len() + additions.len() == CAPACITY {
                    return Err("FireResultBudget: outcome unknown");
                }
                additions.insert(key, *result);
            }
        }
        state.results.extend(
            additions
                .into_iter()
                .map(|(key, result)| (key, (result, now))),
        );
        Ok(())
    }

    pub fn status(
        &self,
        world: frame::WorldGeneration,
        cause: sim::FireCause,
    ) -> PredictedFireStatus {
        let state = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        if state.world != world {
            return PredictedFireStatus::Pending;
        }
        let Some((result, _)) = state.results.get(&(cause.client, cause.command)) else {
            return PredictedFireStatus::Pending;
        };
        match result.outcome {
            sim::FireCommandOutcome::Executed { accepted } if accepted.contains(&Some(cause)) => {
                PredictedFireStatus::Confirmed
            }
            _ => PredictedFireStatus::Refused,
        }
    }

    pub(crate) fn clear(&self) {
        let mut state = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        state.results.clear();
        state.world = frame::WorldGeneration::default();
    }
}
