use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use asset_audio::{SoundCatalog, unit_random};
use asset_core::AssetNamespace;

use crate::admission::AdmissionPolicy;
use crate::clip_store::{ClipKey, MediaService, clip_key_for_sound};
use crate::render_core::AudioScope;
use crate::start::StartFailure;

const SELECTOR_CAPACITY: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CueFailure {
    QueueFull,
    SelectorBudget,
    CompositionBudget,
    CompositionCycle,
    InvalidMixerGroup(asset_audio::MixerGroupError),
    AmbiguousMediaBinding { matches: usize },
    PendingBudget,
    MissingAlias,
    MissingPolicy,
    InvalidMediaBinding,
    NoMedia,
    Cancelled,
    FireRefused,
    StaleScope,
    DuplicateEvent,
    StaleEvent,
    EventBudget,
}

#[derive(Clone)]
pub(crate) struct ResolvedCue {
    pub bank: Arc<SoundCatalog>,
    pub media: Option<MediaService>,
    pub policy: CueExecutionPolicy,
    pub variant: usize,
    pub alias_index: usize,
    pub clip: Option<ClipKey>,
    pub volume: Result<f32, asset_audio::MixerGroupError>,
    pub pitch: f32,
}

#[derive(Clone)]
pub(crate) struct CueExecutionPolicy {
    pub looping: asset_audio::LoopingPolicy,
    pub composition: asset_audio::CueCompositionPolicy,
    pub stereo_speaker_gains: Option<[[f32; 2]; 2]>,
    pub loaded_binding_origin: asset_audio::LoadedBindingOrigin,
    pub channel: Option<asset_audio::ChannelKey>,
    pub priority: Option<asset_audio::VoicePriority>,
    pub spatial: Option<Result<CueSpatialPolicy, StartFailure>>,
    admission: AdmissionPolicy,
}

pub(crate) use asset_audio::SpatialPlaybackPolicy as CueSpatialPolicy;

impl CueExecutionPolicy {
    fn lower(bound: asset_audio::BoundSound<'_>) -> Self {
        let policy = bound.policy();
        let spatial = policy.spatial.as_ref().map(|spatial| match spatial {
            Ok(spatial) => Ok(spatial.clone()),
            Err(failure) => Err(match failure {
                asset_audio::SpatialPolicyFailure::Unsupported(namespace) => {
                    StartFailure::UnsupportedSpatialPolicy(*namespace)
                }
                asset_audio::SpatialPolicyFailure::MissingChannel(namespace) => {
                    StartFailure::MissingChannelPolicy(*namespace)
                }
                asset_audio::SpatialPolicyFailure::MissingFalloffCurve => {
                    StartFailure::NoFalloffCurve
                }
                asset_audio::SpatialPolicyFailure::InvalidFalloffCurve => StartFailure::FalloffEval,
            }),
        });
        Self {
            looping: policy.looping(),
            composition: policy.composition.clone(),
            stereo_speaker_gains: policy.stereo_speaker_gains,
            loaded_binding_origin: policy.loaded_binding_origin,
            channel: policy.channel,
            priority: policy.priority.clone(),
            spatial,
            admission: alias_admission(bound.revision(), bound.alias_index(), policy),
        }
    }

    pub(crate) fn admission_for(&self, emitter: Option<u32>, priority: f32) -> AdmissionPolicy {
        AdmissionPolicy {
            emitter,
            priority,
            ..self.admission
        }
    }
}

fn alias_admission(
    bank_revision: u64,
    index: usize,
    policy: &asset_audio::AliasPlaybackPolicy,
) -> AdmissionPolicy {
    AdmissionPolicy {
        bank_revision,
        alias: Some(index),
        emitter: None,
        channel: policy.channel_admission,
        limits: policy.limits,
        priority: 0.0,
    }
}

pub(crate) struct CueState {
    pub(crate) release: Arc<crate::media::CueRelease>,
    result: OnceLock<Result<ResolvedCue, CueFailure>>,
    completion: OnceLock<crate::StartDecision>,
    pub(crate) playback: OnceLock<Arc<crate::render_core::InstanceState>>,
    children: Mutex<Vec<(String, Arc<CueState>)>>,
}

pub(crate) struct CueHandle(pub Arc<CueState>);

impl CueHandle {
    pub(crate) fn release(&self, frame: u64, frames: u64) {
        self.0.release.release(frame, frames);
    }
    pub(crate) fn active(&self) -> bool {
        self.0.active()
    }
    pub(crate) fn fade_to(&self, frame: u64, to: f32, frames: u64) -> bool {
        self.0.release.fade_to(frame, to, frames)
    }
    pub fn new() -> Self {
        Self(CueState::new())
    }

    pub(crate) fn completion(&self) -> Option<crate::StartDecision> {
        let mut decision = self.0.completion()?;
        if let Some(Ok(cue)) = self.result() {
            decision.detail = Some(self.0.playback.get().map_or_else(
                || {
                    format!(
                        "bank_revision={} looping_policy={:?}",
                        cue.bank.revision(),
                        cue.policy.looping
                    )
                },
                |instance| {
                    format!(
                        "instance={} bank_revision={} looping_policy={:?}",
                        instance.id,
                        cue.bank.revision(),
                        cue.policy.looping
                    )
                },
            ));
        }
        Some(decision)
    }

    pub fn result(&self) -> Option<Result<ResolvedCue, CueFailure>> {
        self.0.result.get().cloned()
    }
}

impl CueState {
    pub(crate) fn primary_outcome(&self) -> Option<crate::StartOutcome> {
        self.completion
            .get()
            .map(|decision| decision.outcome.clone())
    }

    fn active(&self) -> bool {
        self.playback.get().map_or_else(
            || self.completion.get().is_none(),
            |instance| !instance.has_reached(crate::render_core::InstanceStatus::Retired),
        ) || self
            .children
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .iter()
            .any(|(_, child)| child.active())
    }
    fn completion(&self) -> Option<crate::StartDecision> {
        let mut decision = self.completion.get()?.clone();
        for (alias, child) in self
            .children
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .iter()
        {
            let secondary = child.completion()?;
            decision.secondary = Some((alias.clone(), secondary.outcome));
        }
        Some(decision)
    }

    pub(crate) fn child(&self, alias: &str) -> Arc<Self> {
        let child = Arc::new(Self {
            release: self.release.clone(),
            result: OnceLock::new(),
            completion: OnceLock::new(),
            playback: OnceLock::new(),
            children: Mutex::new(Vec::new()),
        });
        self.children
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push((alias.into(), child.clone()));
        child
    }

    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            release: Arc::new(crate::media::CueRelease::default()),
            result: OnceLock::new(),
            completion: OnceLock::new(),
            playback: OnceLock::new(),
            children: Mutex::new(Vec::new()),
        })
    }

    pub(crate) fn complete(&self, decision: crate::StartDecision) {
        let _ = self.completion.set(decision);
    }
    pub(crate) fn resolved(&self, result: Result<ResolvedCue, CueFailure>) {
        let _ = self.result.set(result);
    }
}

pub(crate) struct CueRequest {
    pub state: Arc<CueState>,
    pub execution: crate::cue_execution::CueIntent,
    pub bank: Arc<SoundCatalog>,
    pub media: Option<MediaService>,
    pub namespace: AssetNamespace,
    pub alias: String,
    pub bound: Option<usize>,
    pub scope: AudioScope,
    pub epoch: u64,
    pub pitch_scale: f32,
}

impl CueRequest {
    pub(crate) fn reject(self, reason: CueFailure) {
        self.state.resolved(Err(reason));
        let decision = crate::StartDecision {
            event: self.execution.event,
            namespace: self.namespace,
            alias: self.alias,
            variant: None,
            loaded_binding_origin: None,
            outcome: crate::StartOutcome::Failed(crate::StartFailure::CueRefused(reason)),
            secondary: None,
            detail: None,
        };
        if crate::diagnostics::enabled() {
            crate::diagnostics::emit(decision.line());
        }
        self.state.complete(decision);
    }
}

pub(crate) struct CueResolver {
    lcg: u32,
    history: HashMap<(AudioScope, u64, u64, usize), usize>,
}

impl CueResolver {
    pub fn new() -> Self {
        Self {
            lcg: 0x00a5_5a5a,
            history: HashMap::new(),
        }
    }

    pub fn resolve(&mut self, request: &CueRequest) -> Result<ResolvedCue, CueFailure> {
        let bank = &request.bank;
        let index = request
            .bound
            .or_else(|| bank.index_in(request.namespace, &request.alias))
            .ok_or(CueFailure::MissingAlias)?;
        bank.sound_at(index).ok_or(CueFailure::MissingAlias)?;
        let published = bank
            .bind_published_alias(index, 0)
            .map_err(|_| CueFailure::MissingPolicy)?
            .policy();
        if published.namespace != request.namespace {
            return Err(CueFailure::MissingPolicy);
        }
        let epoch = if request.scope == AudioScope::Match {
            request.epoch
        } else {
            0
        };
        let key = (request.scope, epoch, bank.revision(), index);
        if self.history.len() == SELECTOR_CAPACITY && !self.history.contains_key(&key) {
            return Err(CueFailure::SelectorBudget);
        }
        let variant = bank
            .pick_variant_at(index, &mut self.lcg, self.history.get(&key).copied())
            .ok_or(CueFailure::MissingAlias)?;
        let bound = bank
            .bind_published_alias(index, variant)
            .map_err(|_| CueFailure::MissingPolicy)?;
        let policy = bound.policy();
        let clip = clip_key_for_sound(bound).map_err(|_| CueFailure::InvalidMediaBinding)?;
        let volume = policy.volume(unit_random(&mut self.lcg));
        let pitch = policy.pitch(unit_random(&mut self.lcg));
        let scale = if request.pitch_scale.is_finite() && request.pitch_scale > 0.0 {
            request.pitch_scale
        } else {
            1.0
        };
        self.history.insert(key, variant);
        Ok(ResolvedCue {
            policy: CueExecutionPolicy::lower(bound),
            bank: bank.clone(),
            media: request
                .media
                .clone()
                .filter(|media| media.bank_revision() == bank.revision()),
            variant,
            alias_index: index,
            clip,
            volume,
            pitch: pitch * scale,
        })
    }

    pub fn retain_epoch(&mut self, epoch: u64) {
        self.history
            .retain(|(scope, old, _, _), _| *scope != AudioScope::Match || *old == epoch);
    }
}
