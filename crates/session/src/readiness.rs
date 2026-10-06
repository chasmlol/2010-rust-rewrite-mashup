use bevy::prelude::Resource;
use frame::{LocalLoadKey, RuntimeRole, WorldGeneration, WorldReadiness};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReadinessDecision {
    pub advancement: bool,
    pub presentation: bool,
    pub failure: Option<ReadinessFailure>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadinessFailure {
    Navigation,
    Rendering,
    Audio,
}

impl ReadinessFailure {
    pub fn source(self) -> &'static str {
        match self {
            Self::Navigation => "navigation preparation failed",
            Self::Rendering => "rendering preparation failed",
            Self::Audio => "audio preparation failed",
        }
    }
}

impl ReadinessDecision {
    pub fn failed(self) -> bool {
        self.failure.is_some()
    }
}

pub fn decide_readiness(
    role: RuntimeRole,
    headless: bool,
    generation: WorldGeneration,
    installed: bool,
    navigation: Option<WorldReadiness>,
    rendering: Option<WorldReadiness>,
    audio: Option<WorldReadiness>,
) -> ReadinessDecision {
    if !installed || generation.0.is_none() {
        return ReadinessDecision::default();
    }
    let ready =
        |report: Option<WorldReadiness>| report.is_some_and(|report| report.ready_for(generation));
    let graphics_required = role != RuntimeRole::Dedicated && !headless;
    let rendered = !graphics_required || ready(rendering);
    let presentation = rendered && (!graphics_required || ready(audio));
    let advancement = match role {
        RuntimeRole::Listen | RuntimeRole::Dedicated => ready(navigation) && rendered,
        RuntimeRole::Replay => presentation,
        RuntimeRole::Client => false,
    };
    let failure = if role.runs_authority()
        && navigation.is_some_and(|report| report.failed_for(generation))
    {
        Some(ReadinessFailure::Navigation)
    } else if graphics_required && rendering.is_some_and(|report| report.failed_for(generation)) {
        Some(ReadinessFailure::Rendering)
    } else if graphics_required && audio.is_some_and(|report| report.failed_for(generation)) {
        Some(ReadinessFailure::Audio)
    } else {
        None
    };
    ReadinessDecision {
        advancement: advancement && failure.is_none(),
        presentation: presentation && failure.is_none(),
        failure,
    }
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionReadinessPolicy {
    pub load_key: Option<LocalLoadKey>,
    pub generation: WorldGeneration,
    pub advancement_allowed: bool,
    pub presentation_allowed: bool,
    pub admission_allowed: bool,
    pub input_allowed: bool,
}

impl SessionReadinessPolicy {
    pub fn local_input_allowed(
        self,
        generation: WorldGeneration,
        armed: bool,
        failed: bool,
    ) -> bool {
        self.input_allowed
            && self.generation == generation
            && generation.0.is_some()
            && self
                .load_key
                .is_some_and(|key| generation.0 == Some(key.local_load_request_id))
            && armed
            && !failed
    }
}
