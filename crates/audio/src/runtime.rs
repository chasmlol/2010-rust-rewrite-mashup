use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bevy::prelude::Resource;

use crate::admission::{AdmissionFailure, AdmissionPolicy, Occupant};
use crate::clip_store::MediaService;
use crate::cue::{CueFailure, CueHandle, CueRequest, CueResolver};
use crate::cue_execution::{
    CueCancellation, CueIntent, CueLease, CueMix, CueStep, CueTrigger, CueWork,
};
use crate::media::RenderMedia;
use crate::render_core::{
    Assignment, AudioScope, InstanceState, InstanceStatus, PHYSICAL_VOICES, QUANTUM, RenderShared,
    RenderVoiceId, SAMPLE_RATE,
};
use crate::sources::{
    DesiredSource, SourceCue, SourceInbox, SourceKey, SourcePublisher, SourceScene,
};
use crate::spatial::{ListenerSnapshot, ListenerState, SpatialSource};
use std::collections::VecDeque;

pub const LOGICAL_INSTANCES: usize = 2048;
const CONTROL_BATCH: usize = 64;

#[derive(Clone, Copy, Debug)]
pub struct AudioDiagnostics {
    pub audio_frame: u64,
    pub device_blocks: u64,
    pub null_blocks: u64,
    pub busy_blocks: u64,
    pub device_underruns: u64,
    pub peak: f32,
    pub source_revision: u64,
    pub logical_sources: usize,
    pub pending_source_layers: usize,
    pub rendered_sources: usize,
    pub virtual_sources: usize,
    pub dropped_sources: u64,
}

struct StartRequest {
    event: Option<crate::AudioEvent>,
    cancellation: Option<CueLease>,
    spatial: Option<SpatialSource>,
    admission: AdmissionPolicy,
    media: RenderMedia,
    instance: Arc<InstanceState>,
    looping: bool,
    frame: u64,
    start_deadline: Option<Instant>,
    protect_attack: bool,
}

#[derive(Clone, Copy)]
struct SourceBinding {
    key: SourceKey,
    version: u64,
    gain: f32,
    rate: f32,
    group: Option<crate::sources::SourceRenderGroup>,
    eligible: bool,
}

struct LogicalInstance {
    request: StartRequest,
    slot: Option<usize>,
    cursor: f64,
    advanced_at: u64,
    source: Option<SourceBinding>,
    first_device_reported: bool,
}

#[derive(Resource)]
pub struct AudioRuntime {
    shared: Arc<RenderShared>,
    cue_tx: SyncSender<CueRequest>,
    pcm_tx: SyncSender<(RenderMedia, f32, f32, u64)>,
    media: Mutex<Option<MediaService>>,
    cue_mix: Mutex<Option<CueMix>>,
    cue_cancellation: CueCancellation,
    cue_budget: crate::pending::PendingBudget,
    sources: Arc<SourceInbox>,
    listener: Arc<ListenerState>,
    event_context: Arc<crate::event::EventContextState>,
    fire_verdicts: Arc<Mutex<net::FireVerdictState>>,
    source_publisher: Mutex<SourcePublisher>,
    rejections: Arc<[AtomicU64; 7]>,
    shutdown: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Default for AudioRuntime {
    fn default() -> Self {
        Self::new(!crate::AudioSilent::active())
    }
}

impl AudioRuntime {
    pub fn new(device_enabled: bool) -> Self {
        let shared = Arc::new(RenderShared::new());
        shared
            .device_required
            .store(device_enabled, Ordering::Relaxed);
        let shutdown = Arc::new(AtomicBool::new(false));
        let (cue_tx, cue_rx) = sync_channel(LOGICAL_INSTANCES);
        let (pcm_tx, pcm_rx) = sync_channel(CONTROL_BATCH);
        let thread_shared = shared.clone();
        let thread_shutdown = shutdown.clone();
        let sources = Arc::new(SourceInbox::new());
        let control_sources = sources.clone();
        let listener = Arc::new(ListenerState::default());
        let control_listener = listener.clone();
        let event_context = Arc::new(crate::event::EventContextState::default());
        let control_event_context = event_context.clone();
        let fire_verdicts = Arc::new(Mutex::new(net::FireVerdictState::default()));
        let control_fire_verdicts = fire_verdicts.clone();
        let control_ids = Arc::new(AtomicU64::new(1));
        let rejections = Arc::new(std::array::from_fn(|_| AtomicU64::new(0)));
        let control_rejections = rejections.clone();
        let cue_budget = crate::pending::PendingBudget::default();
        let control_budget = cue_budget.clone();
        let worker = std::thread::Builder::new()
            .name("audio-control".into())
            .spawn(move || {
                control(
                    thread_shared,
                    thread_shutdown,
                    cue_rx,
                    pcm_rx,
                    device_enabled,
                    control_sources,
                    control_listener,
                    control_event_context,
                    control_fire_verdicts,
                    control_ids,
                    control_rejections,
                    control_budget,
                )
            })
            .expect("cannot start audio control thread");
        Self {
            shared,
            cue_tx,
            pcm_tx,
            media: Mutex::new(None),
            cue_mix: Mutex::new(None),
            cue_cancellation: CueCancellation::default(),
            cue_budget,
            sources,
            listener,
            event_context,
            fire_verdicts,
            source_publisher: Mutex::new(SourcePublisher::new()),
            rejections,
            shutdown,
            worker: Some(worker),
        }
    }

    pub fn set_fire_verdicts(&self, verdicts: net::FireVerdictState) {
        *self
            .fire_verdicts
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = verdicts;
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
    }

    pub(crate) fn play_pcm(&self, media: RenderMedia, gain: f32, rate: f32, epoch: u64) {
        if self.pcm_tx.try_send((media, gain, rate, epoch)).is_ok()
            && let Some(worker) = &self.worker
        {
            worker.thread().unpark();
        }
    }

    pub fn audio_frame(&self) -> u64 {
        self.shared.frame.load(Ordering::Acquire)
    }

    pub fn diagnostics(&self) -> AudioDiagnostics {
        AudioDiagnostics {
            audio_frame: self.audio_frame(),
            device_blocks: self.shared.device_blocks.load(Ordering::Relaxed),
            null_blocks: self.shared.null_blocks.load(Ordering::Relaxed),
            busy_blocks: self.shared.busy_blocks.load(Ordering::Relaxed),
            device_underruns: self.shared.device_underruns.load(Ordering::Relaxed),
            peak: f32::from_bits(self.shared.peak.load(Ordering::Relaxed)),
            source_revision: self.sources.revision.load(Ordering::Acquire),
            logical_sources: self.sources.active.load(Ordering::Relaxed),
            pending_source_layers: self.sources.pending_layers.load(Ordering::Relaxed),
            rendered_sources: self.sources.rendered.load(Ordering::Relaxed),
            virtual_sources: self.sources.virtualized.load(Ordering::Relaxed),
            dropped_sources: self.sources.dropped.load(Ordering::Relaxed),
        }
    }

    pub fn rejection_count(&self, reason: AdmissionFailure) -> u64 {
        self.rejections[reason as usize - 1].load(Ordering::Relaxed)
    }

    pub fn set_master_volume(&self, gain: f32) {
        let gain = if gain.is_finite() { gain.max(0.0) } else { 0.0 };
        self.shared.master.store(gain.to_bits(), Ordering::Release);
    }

    pub fn set_match_epoch(&self, epoch: u64) {
        self.shared.match_epoch.store(epoch, Ordering::Release);
        self.source_publisher
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .retain_epoch(epoch);
    }

    pub fn cancel_all(&self) {
        self.shared.cancelled.store(true, Ordering::Release);
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
        self.source_publisher
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .cancel();
    }

    pub(crate) fn set_sources(&mut self, sources: Vec<DesiredSource>) {
        let (scene, rejected) = self
            .source_publisher
            .get_mut()
            .unwrap_or_else(|poison| poison.into_inner())
            .reconcile(
                sources,
                self.shared.match_epoch.load(Ordering::Acquire),
                self.shared.cancelled.load(Ordering::Acquire),
            );
        self.sources.dropped.fetch_add(rejected, Ordering::Relaxed);
        self.sources.publish(scene);
    }

    pub(crate) fn source_cue(
        &self,
        request: crate::sources::SourceCueRequest,
    ) -> Option<Arc<SourceCue>> {
        let crate::sources::SourceCueRequest {
            bank,
            namespace,
            alias,
            emitter,
            scope,
            epoch,
            group,
        } = request;
        let lease = self
            .cue_cancellation
            .lease(namespace, &alias, emitter, scope, epoch)?;
        let media = self.media_for_bank(&bank)?;
        let mix = self
            .cue_mix
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .as_ref()
            .filter(|mix| scope == AudioScope::Match && mix.epoch == epoch)
            .cloned();
        Some(Arc::new(SourceCue {
            bank,
            namespace,
            alias,
            emitter,
            media,
            mix,
            lease,
            group,
        }))
    }

    pub(crate) fn set_listener(&self, listener: Option<ListenerSnapshot>) {
        self.listener.set(listener);
    }

    pub(crate) fn set_media_service(&self, media: Option<MediaService>) {
        let mut installed = self
            .media
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let unchanged = match (&*installed, &media) {
            (None, None) => true,
            (Some(old), Some(new)) => old.same_owner(new),
            _ => false,
        };
        if !unchanged {
            *installed = media;
        }
    }

    pub(crate) fn media_for_bank(&self, bank: &asset_audio::SoundCatalog) -> Option<MediaService> {
        self.media
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .as_ref()
            .filter(|media| media.bank_revision() == bank.revision())
            .cloned()
    }

    pub(crate) fn set_cue_mix(&self, mix: Option<CueMix>) {
        *self
            .cue_mix
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = mix;
    }

    pub(crate) fn set_event_context(&self, context: Option<crate::event::EventContext>) {
        self.event_context.set(context);
    }

    pub(crate) fn trigger_cue(&self, request: CueTrigger) -> CueHandle {
        self.trigger_with_fade(request, None)
    }

    pub(crate) fn trigger_faded_cue(&self, request: CueTrigger, frames: u64) -> CueHandle {
        self.trigger_with_fade(request, Some(frames))
    }

    fn trigger_with_fade(&self, request: CueTrigger, fade: Option<u64>) -> CueHandle {
        let CueTrigger {
            event,
            bank,
            namespace,
            alias,
            bound,
            origin_inches,
            emitter,
            class,
            epoch,
            pitch_scale,
            fallbacks,
        } = request;
        if fallbacks.len() > 16 {
            return refused_cue(namespace, &alias, event, CueFailure::CompositionBudget);
        }
        let Some(ticket) = self.cue_budget.reserve() else {
            return refused_cue(namespace, &alias, event, CueFailure::PendingBudget);
        };
        let Some(lease) =
            self.cue_cancellation
                .lease(namespace, &alias, emitter, class.scope(), epoch)
        else {
            return refused_cue(namespace, &alias, event, CueFailure::PendingBudget);
        };
        let mix = self
            .cue_mix
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .as_ref()
            .filter(|mix| class.scope() == AudioScope::Match && mix.epoch == epoch)
            .cloned();
        let media = self.media_for_bank(&bank);
        let state = crate::cue::CueState::new();
        if let Some(frames) = fade {
            state.release.fade_in(self.audio_frame(), frames);
        }
        self.enqueue_cue(CueRequest {
            state,
            bank,
            media,
            namespace,
            alias,
            bound,
            scope: class.scope(),
            epoch,
            pitch_scale,
            execution: CueIntent {
                event,
                origin_inches,
                emitter,
                class,
                deadline: Instant::now() + class.start_wait(),
                depth: 0,
                fallbacks,
                mix,
                lease,
                ticket,
            },
        })
    }

    pub(crate) fn stop_cue(
        &self,
        namespace: asset_core::AssetNamespace,
        alias: &str,
        emitter: Option<u32>,
        epoch: u64,
    ) {
        self.cue_cancellation
            .cancel(Some(namespace), Some(alias), Some(emitter), epoch);
    }

    pub(crate) fn stop_emitter_cues(&self, emitter: u32, epoch: u64) {
        self.cue_cancellation
            .cancel(None, None, Some(Some(emitter)), epoch);
    }

    fn enqueue_cue(&self, request: CueRequest) -> CueHandle {
        let handle = CueHandle(request.state.clone());
        if self.shared.cancelled.load(Ordering::Acquire) {
            request.reject(CueFailure::Cancelled);
        } else if let Err(error) = self.cue_tx.try_send(request) {
            let (std::sync::mpsc::TrySendError::Full(request)
            | std::sync::mpsc::TrySendError::Disconnected(request)) = error;
            request.reject(CueFailure::QueueFull);
        } else if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
        handle
    }
}

impl Drop for AudioRuntime {
    fn drop(&mut self) {
        self.cancel_all();
        self.shutdown.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn control(
    shared: Arc<RenderShared>,
    shutdown: Arc<AtomicBool>,
    cue_rx: Receiver<CueRequest>,
    pcm_rx: Receiver<(RenderMedia, f32, f32, u64)>,
    device_enabled: bool,
    sources: Arc<SourceInbox>,
    listener: Arc<ListenerState>,
    event_context: Arc<crate::event::EventContextState>,
    fire_verdicts: Arc<Mutex<net::FireVerdictState>>,
    next_id: Arc<AtomicU64>,
    rejections: Arc<[AtomicU64; 7]>,
    cue_budget: crate::pending::PendingBudget,
) {
    assets::session_load::use_process_cpus();
    crate::diagnostics::thread("audio-control");
    let diag_anchor = Instant::now();
    let mut diag_last = Instant::now();
    let mut diag_max_pass = Duration::ZERO;
    let mut diag_previous_pass = Instant::now();
    let mut diag_max_gap = Duration::ZERO;
    let mut resolver = CueResolver::new();
    let mut events = crate::event::EventJournal::new();
    let mut pending_cues = VecDeque::<CueWork>::with_capacity(LOGICAL_INSTANCES);
    let device = device_enabled.then(|| {
        let shared = shared.clone();
        let shutdown = shutdown.clone();
        std::thread::Builder::new()
            .name("audio-device".into())
            .spawn(move || crate::device::supervise(shared, shutdown))
            .expect("cannot start audio device thread")
    });
    let mut device_was_active = false;
    let mut null_anchor = Instant::now();
    let mut null_frame = 0;
    let mut instances: Vec<LogicalInstance> = Vec::with_capacity(LOGICAL_INSTANCES);
    let mut next_voice = 1;
    let mut desired = SourceScene {
        revision: 0,
        sources: Vec::new(),
        asserted: Vec::new(),
    };
    let mut present_sources = HashSet::with_capacity(LOGICAL_INSTANCES);
    let mut source_cues = HashMap::<(SourceKey, u64), Arc<crate::cue::CueState>>::with_capacity(
        crate::sources::SOURCE_HISTORY,
    );
    let mut silence = [[0.0; 2]; QUANTUM];
    while !shutdown.load(Ordering::Acquire) {
        let diag_pass = Instant::now();
        diag_max_gap = diag_max_gap.max(diag_pass.duration_since(diag_previous_pass));
        diag_previous_pass = diag_pass;
        let listener = listener.get();
        for _ in 0..CONTROL_BATCH {
            let Ok((media, gain, rate, epoch)) = pcm_rx.try_recv() else {
                break;
            };
            let admission = AdmissionPolicy::default();
            let instance = make_instance(
                next_id.fetch_add(1, Ordering::Relaxed),
                AudioScope::Match,
                epoch,
                &admission,
                Instant::now(),
            );
            set_parameters(&instance, gain, rate, true);
            admit(
                StartRequest {
                    event: None,
                    cancellation: None,
                    spatial: None,
                    admission,
                    media,
                    instance,
                    looping: false,
                    frame: shared.frame.load(Ordering::Acquire),
                    start_deadline: Some(Instant::now() + Duration::from_millis(250)),
                    protect_attack: false,
                },
                &shared,
                &mut instances,
                listener,
                &rejections,
                None,
            );
        }
        events.advance(event_context.get());
        let diag_stage = crate::diagnostics::slow_stage("context", diag_pass);
        for logical in &mut instances {
            let first_us = logical
                .request
                .instance
                .first_device_us
                .load(Ordering::Acquire);
            if crate::diagnostics::enabled()
                && !logical.first_device_reported
                && first_us != u64::MAX
            {
                crate::diagnostics::emit(format!(
                    "audio diag: first_device instance={} request_to_device_us={} audio_frame={}",
                    logical.request.instance.id,
                    first_us,
                    logical
                        .request
                        .instance
                        .first_device_frame
                        .load(Ordering::Relaxed)
                ));
                logical.first_device_reported = true;
            }
            if device_enabled
                && logical
                    .request
                    .start_deadline
                    .is_some_and(|deadline| Instant::now() >= deadline)
                && (!shared.device_active.load(Ordering::Acquire)
                    || !logical
                        .request
                        .instance
                        .has_reached(InstanceStatus::Started))
            {
                reject(
                    &logical.request.instance,
                    &rejections,
                    AdmissionFailure::OutputUnavailable,
                );
            }
            if logical
                .request
                .cancellation
                .as_ref()
                .is_some_and(CueLease::cancelled)
                || !events.current(logical.request.event)
                || (!logical
                    .request
                    .instance
                    .has_reached(InstanceStatus::Started)
                    && crate::event::EventJournal::fire_refused(
                        logical.request.event,
                        &fire_verdicts
                            .lock()
                            .unwrap_or_else(|poison| poison.into_inner()),
                    ))
            {
                logical
                    .request
                    .instance
                    .stopped
                    .store(true, Ordering::Release);
            }
            if logical.source.is_none() {
                update_spatial(&logical.request, listener);
            }
        }
        if let Some(scene) = sources.take()
            && scene.revision > desired.revision
        {
            desired = scene;
        }
        let epoch = shared.match_epoch.load(Ordering::Acquire);
        desired.sources.retain(|source| {
            !shared.cancelled.load(Ordering::Acquire)
                && (source.key.scope != AudioScope::Match || source.key.epoch == epoch)
        });
        desired.asserted.retain(|source| {
            !shared.cancelled.load(Ordering::Acquire)
                && (source.key.scope != AudioScope::Match || source.key.epoch == epoch)
        });
        source_cues.retain(|(key, version), state| {
            let current = desired
                .asserted
                .binary_search_by_key(key, |source| source.key)
                .ok()
                .is_some_and(|index| desired.asserted[index].version == *version);
            if !current {
                state
                    .release
                    .release(shared.frame.load(Ordering::Acquire), 0);
            }
            current
        });
        for logical in &mut instances {
            let Some(binding) = logical.source else {
                continue;
            };
            let source = desired
                .asserted
                .binary_search_by_key(&binding.key, |source| source.key)
                .ok()
                .map(|index| &desired.asserted[index]);
            if let Some(source) = source.filter(|source| source.version == binding.version) {
                let executable = executable_source(&desired, binding.key, binding.version);
                set_parameters(
                    &logical.request.instance,
                    source.gain * binding.gain,
                    source.rate * binding.rate,
                    if binding.group.is_some() {
                        logical.request.instance.audible.load(Ordering::Acquire)
                    } else {
                        source.audible && executable
                    },
                );
                if let (Some(origin), Some(spatial)) =
                    (source.origin_inches, &mut logical.request.spatial)
                {
                    spatial.origin_inches = origin;
                }
                update_spatial(&logical.request, listener);
                let eligible = source.audible
                    && executable
                    && logical.request.spatial.as_ref().is_none_or(|spatial| {
                        listener
                            .is_some_and(|listener| spatial.evaluate(listener).gains != [0.0; 2])
                    });
                logical.source.as_mut().expect("source binding").eligible = eligible;
                if binding.group.is_none() {
                    logical
                        .request
                        .instance
                        .audible
                        .store(eligible, Ordering::Release);
                }
            } else {
                logical
                    .request
                    .instance
                    .stopped
                    .store(true, Ordering::Release);
            }
        }
        apply_source_render_budget(&instances);
        let diag_stage = crate::diagnostics::slow_stage("source_parameters", diag_stage);
        let device_active = shared.device_active.load(Ordering::Acquire);
        if device_was_active && !device_active {
            null_anchor = Instant::now();
            null_frame = shared.frame.load(Ordering::Acquire);
        }
        device_was_active = device_active;
        if !device_active {
            let elapsed = (null_anchor.elapsed().as_secs_f64() * f64::from(SAMPLE_RATE)) as u64;
            let target = null_frame.saturating_add(elapsed);
            for _ in 0..CONTROL_BATCH {
                if shared.frame.load(Ordering::Acquire) + QUANTUM as u64 > target {
                    break;
                }
                shared.render_for(&mut silence, Some(false));
            }
        }
        for index in 0..PHYSICAL_VOICES {
            // This thread is the sole publisher/reclaimer, even in null transport.
            if let Some(assignment) = unsafe { shared.reclaim(index) } {
                if crate::diagnostics::enabled() {
                    retire_diagnostic(&assignment.instance, shared.frame.load(Ordering::Acquire));
                }
                if assignment.instance.status() == InstanceStatus::Virtual {
                    if let Some(logical) = instances
                        .iter_mut()
                        .find(|logical| logical.request.instance.id == assignment.instance.id)
                    {
                        logical.slot = None;
                        logical.cursor =
                            f64::from_bits(assignment.instance.cursor.load(Ordering::Acquire));
                        logical.advanced_at =
                            assignment.instance.rendered_at.load(Ordering::Acquire);
                    }
                } else {
                    assignment.instance.retire();
                }
            }
        }
        instances.retain(|logical| {
            if logical.request.instance.status() == InstanceStatus::Retired {
                if logical.slot.is_none() {
                    retire_diagnostic(
                        &logical.request.instance,
                        shared.frame.load(Ordering::Acquire),
                    );
                }
                false
            } else {
                true
            }
        });

        let diag_stage = crate::diagnostics::slow_stage("null_and_reclaim", diag_stage);
        resolver.retain_epoch(shared.match_epoch.load(Ordering::Acquire));
        for _ in 0..CONTROL_BATCH {
            let Ok(request) = cue_rx.try_recv() else {
                break;
            };
            // Publication can race a control pass already processing source layers.
            // Validate a newly received cue against the latest published clocks.
            events.advance(event_context.get());
            if request.scope == AudioScope::Match
                && request.epoch != shared.match_epoch.load(Ordering::Acquire)
            {
                request.reject(CueFailure::StaleScope);
            } else if crate::event::EventJournal::fire_refused(
                request.execution.event,
                &fire_verdicts
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner()),
            ) {
                request.reject(CueFailure::FireRefused);
            } else if let Err(reason) = events.accept(request.execution.event) {
                if crate::diagnostics::enabled() {
                    crate::diagnostics::emit(format!(
                        "audio diag: event_rejected reason={reason:?} event={:?} context={:?}",
                        request.execution.event,
                        event_context.get()
                    ));
                }
                request.reject(reason);
            } else if pending_cues.len() == LOGICAL_INSTANCES {
                request.reject(CueFailure::PendingBudget);
            } else {
                pending_cues.push_back(CueWork::new(request));
            }
        }
        let waiting = pending_cues.len().min(CONTROL_BATCH);
        for _ in 0..waiting {
            let mut work = pending_cues.pop_front().expect("pending cue");
            if shared.cancelled.load(Ordering::Acquire)
                || (work.request.scope == AudioScope::Match
                    && work.request.epoch != shared.match_epoch.load(Ordering::Acquire))
            {
                work.complete(crate::StartOutcome::Failed(
                    crate::StartFailure::CueRefused(if shared.cancelled.load(Ordering::Acquire) {
                        CueFailure::Cancelled
                    } else {
                        CueFailure::StaleScope
                    }),
                ));
                continue;
            }
            if crate::event::EventJournal::fire_refused(
                work.request.execution.event,
                &fire_verdicts
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner()),
            ) {
                work.complete(crate::StartOutcome::Failed(
                    crate::StartFailure::CueRefused(CueFailure::FireRefused),
                ));
                continue;
            }
            if !events.current(work.request.execution.event) {
                work.complete(crate::StartOutcome::Failed(
                    crate::StartFailure::CueRefused(CueFailure::StaleEvent),
                ));
                continue;
            }
            match events.claim_fire(
                work.request.execution.event,
                &work.request.state,
                work.request.execution.depth,
            ) {
                Ok(true) => {}
                Ok(false) => {
                    if std::time::Instant::now() >= work.request.execution.deadline {
                        work.complete(crate::StartOutcome::Failed(crate::StartFailure::Expired));
                    } else {
                        pending_cues.push_back(work);
                    }
                    continue;
                }
                Err(reason) => {
                    work.complete(crate::StartOutcome::Failed(
                        crate::StartFailure::CueRefused(reason),
                    ));
                    continue;
                }
            }
            if let Some((key, version)) = work.source {
                let current = desired
                    .asserted
                    .binary_search_by_key(&key, |source| source.key)
                    .ok()
                    .map(|index| &desired.asserted[index])
                    .filter(|source| source.version == version);
                let Some(source) = current else {
                    work.complete(crate::StartOutcome::Failed(
                        crate::StartFailure::CueRefused(CueFailure::Cancelled),
                    ));
                    continue;
                };
                if (!executable_source(&desired, key, version)
                    || instances.len() == LOGICAL_INSTANCES)
                    && !work.request.execution.lease.cancelled()
                    && !work.request.state.release.requested()
                {
                    pending_cues.push_back(work);
                    continue;
                }
                work.request.execution.origin_inches = source.origin_inches;
            }
            let mut children = Vec::new();
            let step = work.step(
                &mut resolver,
                listener,
                &mut children,
                !device_enabled || shared.device_active.load(Ordering::Acquire),
            );
            match step {
                CueStep::Waiting => pending_cues.push_back(work),
                CueStep::Finished => {}
                CueStep::Start(start) => {
                    let instance = make_instance(
                        next_id.fetch_add(1, Ordering::Relaxed),
                        work.request.scope,
                        work.request.epoch,
                        &start.admission,
                        work.request
                            .execution
                            .deadline
                            .checked_sub(work.request.execution.class.start_wait())
                            .filter(|_| work.source.is_none())
                            .unwrap_or_else(Instant::now),
                    );
                    let source = work.source.and_then(|(key, version)| {
                        desired
                            .sources
                            .binary_search_by_key(&key, |source| source.key)
                            .ok()
                            .map(|index| &desired.sources[index])
                            .filter(|source| source.version == version)
                    });
                    set_parameters(
                        &instance,
                        start.gain * source.map_or(1.0, |source| source.gain),
                        start.rate * source.map_or(1.0, |source| source.rate),
                        source.is_none_or(|source| source.audible),
                    );
                    admit(
                        StartRequest {
                            event: work.request.execution.event,
                            start_deadline: (work.source.is_none() && !start.looping)
                                .then_some(work.request.execution.deadline),
                            protect_attack: work.source.is_none()
                                && !start.looping
                                && work.request.execution.class == crate::SoundClass::Weapon,
                            cancellation: Some(start.lease),
                            spatial: start.spatial,
                            admission: start.admission,
                            media: start.media,
                            instance: instance.clone(),
                            looping: start.looping,
                            frame: source.map_or_else(
                                || shared.frame.load(Ordering::Acquire),
                                |source| source.start_frame,
                            ),
                        },
                        &shared,
                        &mut instances,
                        listener,
                        &rejections,
                        work.source.map(|(key, version)| SourceBinding {
                            key,
                            version,
                            gain: start.gain,
                            rate: start.rate,
                            group: source.and_then(|source| source.cue.group),
                            eligible: source.is_none_or(|source| source.audible),
                        }),
                    );
                    let _ = work.request.state.playback.set(instance.clone());
                    work.complete(instance.rejection().map_or(
                        crate::StartOutcome::Submitted,
                        |reason| {
                            crate::StartOutcome::Failed(crate::StartFailure::AdmissionRefused(
                                reason,
                            ))
                        },
                    ));
                }
            }
            for child in children {
                if pending_cues.len() == LOGICAL_INSTANCES {
                    child.complete(crate::StartOutcome::Failed(
                        crate::StartFailure::CueRefused(CueFailure::PendingBudget),
                    ));
                } else {
                    pending_cues.push_back(child);
                }
            }
        }
        let diag_stage = crate::diagnostics::slow_stage("cue_steps", diag_stage);
        apply_source_render_budget(&instances);
        instances.sort_by_key(|logical| logical.source.is_some());
        let now = shared.frame.load(Ordering::Acquire);
        let epoch = shared.match_epoch.load(Ordering::Acquire);

        instances.retain_mut(|logical| {
            let instance = &logical.request.instance;
            if instance.status() == InstanceStatus::Retired {
                return false;
            }
            if logical.slot.is_some() {
                return true;
            }
            if shared.cancelled.load(Ordering::Acquire)
                || instance.stopped.load(Ordering::Acquire)
                || logical
                    .request
                    .media
                    .release
                    .as_ref()
                    .is_some_and(|release| release.expired(now))
                || (instance.scope == AudioScope::Match && instance.epoch != epoch)
            {
                instance.retire();
                retire_diagnostic(instance, shared.frame.load(Ordering::Acquire));
                return false;
            }
            if logical.source.is_none()
                && !instance.has_reached(InstanceStatus::Started)
                && instance.audible.load(Ordering::Acquire)
            {
                logical.advanced_at = now;
            }
            logical.advanced_at = logical.advanced_at.max(logical.request.frame);
            if !instance.paused.load(Ordering::Relaxed) {
                let rate = f32::from_bits(instance.rate.load(Ordering::Relaxed));
                logical.cursor += now.saturating_sub(logical.advanced_at) as f64
                    * f64::from(logical.request.media.rate())
                    / f64::from(SAMPLE_RATE)
                    * f64::from(rate);
            }
            logical.advanced_at = now;
            let frames = logical.request.media.frames() as f64;
            if logical.request.looping {
                logical.cursor %= frames;
            } else if logical.cursor >= frames {
                instance.retire();
                retire_diagnostic(instance, shared.frame.load(Ordering::Acquire));
                return false;
            }
            instance
                .cursor
                .store(logical.cursor.to_bits(), Ordering::Release);
            instance.rendered_at.store(now, Ordering::Release);
            if !instance.audible.load(Ordering::Acquire) {
                instance.set_status(InstanceStatus::Virtual);
                return true;
            }
            let assignment = Assignment {
                voice: RenderVoiceId(next_voice),
                media: logical.request.media.clone(),
                instance: instance.clone(),
                start_frame: now.max(logical.request.frame),
                start_cursor: logical.cursor,
                looping: logical.request.looping,
            };
            next_voice += 1;
            instance.set_status(InstanceStatus::Scheduled);
            // Only this thread may publish a new slot payload.
            match unsafe { shared.publish(assignment) } {
                Ok(slot) => logical.slot = Some(slot),
                Err(_) if logical.request.looping => instance.set_status(InstanceStatus::Virtual),
                Err(_) if instance.has_reached(InstanceStatus::Started) => {
                    instance.set_status(InstanceStatus::Virtual)
                }
                Err(_) => {}
            }
            true
        });
        let diag_stage = crate::diagnostics::slow_stage("voice_scheduling", diag_stage);
        if !shared.cancelled.load(Ordering::Acquire) {
            present_sources.clear();
            present_sources.extend(instances.iter().filter_map(|logical| {
                logical.source.map(|binding| (binding.key, binding.version))
            }));
            let mut source_intakes = 0;
            for source in &desired.sources {
                if (source.key.scope == AudioScope::Match && source.key.epoch != epoch)
                    || present_sources.contains(&(source.key, source.version))
                    || instances.len() == LOGICAL_INSTANCES
                {
                    continue;
                }
                let cue = &source.cue;
                if source_intakes == CONTROL_BATCH
                    || source_cues.contains_key(&(source.key, source.version))
                    || pending_cues.len() == LOGICAL_INSTANCES
                {
                    continue;
                }
                let Some(ticket) = cue_budget.reserve() else {
                    continue;
                };
                let state = crate::cue::CueState::new();
                let mut work = CueWork::new(CueRequest {
                    state: state.clone(),
                    bank: cue.bank.clone(),
                    media: Some(cue.media.clone()),
                    namespace: cue.namespace,
                    alias: cue.alias.clone(),
                    bound: None,
                    scope: source.key.scope,
                    epoch: source.key.epoch,
                    pitch_scale: 1.0,
                    execution: CueIntent {
                        event: None,
                        origin_inches: source.origin_inches,
                        emitter: cue.emitter,
                        class: crate::SoundClass::Ambience,
                        deadline: Instant::now(),
                        depth: 0,
                        fallbacks: Vec::new(),
                        mix: cue.mix.clone(),
                        lease: cue.lease.clone(),
                        ticket,
                    },
                });
                work.source = Some((source.key, source.version));
                source_intakes += 1;
                source_cues.insert((source.key, source.version), state);
                pending_cues.push_back(work);
            }
        }
        let mut active = 0;
        let mut rendered = 0;
        let mut virtualized = 0;
        for logical in instances.iter().filter(|logical| logical.source.is_some()) {
            active += 1;
            match logical.request.instance.status() {
                InstanceStatus::Started => rendered += 1,
                InstanceStatus::Virtual => virtualized += 1,
                _ => {}
            }
        }
        sources.pending_layers.store(
            pending_cues
                .iter()
                .filter(|work| work.source.is_some())
                .count(),
            Ordering::Relaxed,
        );
        sources.active.store(active, Ordering::Relaxed);
        sources.rendered.store(rendered, Ordering::Relaxed);
        sources.virtualized.store(virtualized, Ordering::Relaxed);
        sources.revision.store(desired.revision, Ordering::Release);
        let _ = crate::diagnostics::slow_stage("source_intake", diag_stage);
        if crate::diagnostics::enabled() {
            diag_max_pass = diag_max_pass.max(diag_pass.elapsed());
            if diag_last.elapsed() >= Duration::from_secs(1) {
                crate::diagnostics::emit(format!(
                    "audio diag: transport elapsed_ms={:.3} audio_frame={} active={} device_blocks={} null_blocks={} underruns={} logical={} pending={} max_control_ms={:.3} max_control_gap_ms={:.3}",
                    diag_anchor.elapsed().as_secs_f64() * 1000.0,
                    shared.frame.load(Ordering::Acquire),
                    shared.device_active.load(Ordering::Acquire),
                    shared.device_blocks.load(Ordering::Relaxed),
                    shared.null_blocks.load(Ordering::Relaxed),
                    shared.device_underruns.load(Ordering::Relaxed),
                    instances.len(),
                    pending_cues.len(),
                    diag_max_pass.as_secs_f64() * 1000.0,
                    diag_max_gap.as_secs_f64() * 1000.0
                ));
                diag_last = Instant::now();
                diag_max_pass = Duration::ZERO;
                diag_max_gap = Duration::ZERO;
            }
        }
        std::thread::park_timeout(Duration::from_millis(2));
    }
    for work in pending_cues {
        work.complete(crate::StartOutcome::Failed(
            crate::StartFailure::CueRefused(CueFailure::Cancelled),
        ));
    }
    while let Ok(request) = cue_rx.try_recv() {
        request.reject(CueFailure::Cancelled);
    }
    // Join/destroy the device before the last owner can free render payloads.
    if let Some(device) = device {
        let _ = device.join();
    }
    for logical in instances {
        logical.request.instance.retire();
    }
    diag::info!(
        Audio,
        "audio: control stopped frames={} device_blocks={} null_blocks={} busy_blocks={}",
        shared.frame.load(Ordering::Acquire),
        shared.device_blocks.load(Ordering::Relaxed),
        shared.null_blocks.load(Ordering::Relaxed),
        shared.busy_blocks.load(Ordering::Relaxed)
    );
}

fn apply_source_render_budget(instances: &[LogicalInstance]) {
    let mut ranked: Vec<_> = instances
        .iter()
        .filter(|logical| {
            logical.source.is_some_and(|source| {
                source.group == Some(crate::sources::SourceRenderGroup::MapEmitter)
            }) && live(&logical.request.instance)
        })
        .map(|logical| {
            let gain = f32::from_bits(logical.request.instance.gain.load(Ordering::Relaxed));
            let level = logical.request.media.pan.as_ref().map_or(gain, |pan| {
                let (left, right) = pan.get();
                gain * left.hypot(right)
            });
            let level = if logical.source.is_some_and(|source| source.eligible) && level.is_finite()
            {
                level
            } else {
                0.0
            };
            (logical, level)
        })
        .collect();
    ranked.sort_unstable_by(|(a, ag), (b, bg)| {
        bg.total_cmp(ag)
            .then(a.request.instance.id.cmp(&b.request.instance.id))
    });
    for (rank, (logical, gain)) in ranked.iter().enumerate() {
        if rank >= 8 || *gain < 0.002 {
            logical
                .request
                .instance
                .audible
                .store(false, Ordering::Release);
        }
    }
    for (logical, gain) in ranked.into_iter().take(8) {
        if gain >= 0.002 {
            logical
                .request
                .instance
                .audible
                .store(true, Ordering::Release);
        }
    }
}

fn executable_source(scene: &SourceScene, key: SourceKey, version: u64) -> bool {
    scene
        .sources
        .binary_search_by_key(&key, |source| source.key)
        .ok()
        .is_some_and(|index| scene.sources[index].version == version)
}

fn update_spatial(request: &StartRequest, listener: Option<ListenerSnapshot>) {
    if let (Some(spatial), Some(listener)) = (&request.spatial, listener) {
        let parameters = spatial.evaluate(listener);
        if let Some(pan) = &request.media.pan {
            pan.set(parameters.gains[0], parameters.gains[1]);
        }
        request.instance.priority.store(
            if parameters.priority.is_finite() {
                parameters.priority
            } else {
                0.0
            }
            .to_bits(),
            Ordering::Release,
        );
    }
}

fn set_parameters(instance: &InstanceState, gain: f32, rate: f32, audible: bool) {
    let gain = if gain.is_finite() { gain.max(0.0) } else { 0.0 };
    instance.gain.store(gain.to_bits(), Ordering::Relaxed);
    set_transport(instance, rate, audible);
}

fn set_transport(instance: &InstanceState, rate: f32, audible: bool) {
    let rate = if rate.is_finite() {
        rate.clamp(0.01, 4.0)
    } else {
        1.0
    };
    instance.rate.store(rate.to_bits(), Ordering::Relaxed);
    instance.audible.store(audible, Ordering::Release);
}

fn retire_diagnostic(instance: &InstanceState, frame: u64) {
    if crate::diagnostics::enabled() {
        crate::diagnostics::emit(format!(
            "audio diag: retire instance={} status={:?} device_frames={} null_frames={} cursor={:.3} audio_frame={} first_device_us={} first_device_frame={} rejection={:?} stopped={}",
            instance.id,
            instance.status(),
            instance.device_frames.load(Ordering::Relaxed),
            instance.null_frames.load(Ordering::Relaxed),
            f64::from_bits(instance.cursor.load(Ordering::Acquire)),
            frame,
            instance.first_device_us.load(Ordering::Acquire),
            instance.first_device_frame.load(Ordering::Relaxed),
            instance.rejection(),
            instance.stopped.load(Ordering::Acquire)
        ));
    }
}

fn protected_attack(logical: &LogicalInstance) -> bool {
    logical.request.protect_attack
        && logical
            .request
            .instance
            .device_frames
            .load(Ordering::Relaxed)
            + logical.request.instance.null_frames.load(Ordering::Relaxed)
            < crate::render_core::MIN_ATTACK_FRAMES
}

fn live(instance: &InstanceState) -> bool {
    !instance.stopped.load(Ordering::Acquire)
        && !matches!(
            instance.status(),
            InstanceStatus::SourceEnded | InstanceStatus::Finished | InstanceStatus::Retired
        )
}

fn reject(instance: &InstanceState, counts: &[AtomicU64; 7], reason: AdmissionFailure) {
    counts[reason as usize - 1].fetch_add(1, Ordering::Relaxed);
    instance.reject(reason);
}

fn refused_cue(
    namespace: asset_core::AssetNamespace,
    alias: &str,
    event: Option<crate::AudioEvent>,
    reason: CueFailure,
) -> CueHandle {
    let handle = CueHandle::new();
    handle.0.resolved(Err(reason));
    handle.0.complete(crate::StartDecision {
        event,
        namespace,
        alias: alias.into(),
        variant: None,
        loaded_binding_origin: None,
        outcome: crate::StartOutcome::Failed(crate::StartFailure::CueRefused(reason)),
        secondary: None,
        detail: None,
    });
    handle
}

fn make_instance(
    id: u64,
    scope: AudioScope,
    epoch: u64,
    policy: &AdmissionPolicy,
    requested_at: Instant,
) -> Arc<InstanceState> {
    Arc::new(InstanceState {
        id,
        scope,
        epoch,
        stopped: AtomicBool::new(false),
        finish_attack: AtomicBool::new(false),
        paused: AtomicBool::new(false),
        audible: AtomicBool::new(true),
        gain: AtomicU32::new(0),
        rate: AtomicU32::new(1.0f32.to_bits()),
        priority: AtomicU32::new(policy.priority.to_bits()),
        rejection: AtomicU8::new(0),
        cursor: AtomicU64::new(0.0f64.to_bits()),
        rendered_at: AtomicU64::new(0),
        requested_at,
        first_device_us: AtomicU64::new(u64::MAX),
        first_device_frame: AtomicU64::new(u64::MAX),
        device_frames: AtomicU64::new(0),
        null_frames: AtomicU64::new(0),
        status: AtomicU8::new(InstanceStatus::Requested as u8),
        transitions: AtomicU8::new(1 << InstanceStatus::Requested as u8),
    })
}

fn admit(
    request: StartRequest,
    shared: &RenderShared,
    instances: &mut Vec<LogicalInstance>,
    listener: Option<ListenerSnapshot>,
    rejections: &[AtomicU64; 7],
    source: Option<SourceBinding>,
) {
    let failure = if shared.cancelled.load(Ordering::Acquire)
        || request.instance.stopped.load(Ordering::Acquire)
    {
        Some(AdmissionFailure::Cancelled)
    } else if request.instance.scope == AudioScope::Match
        && request.instance.epoch != shared.match_epoch.load(Ordering::Acquire)
    {
        Some(AdmissionFailure::StaleScope)
    } else if instances.len() == LOGICAL_INSTANCES {
        Some(AdmissionFailure::LogicalBudget)
    } else {
        None
    };
    if let Some(failure) = failure {
        reject(&request.instance, rejections, failure);
        return;
    }
    update_spatial(&request, listener);
    let mut policy = request.admission;
    policy.priority = f32::from_bits(request.instance.priority.load(Ordering::Acquire));
    let occupants: Vec<_> = instances
        .iter()
        .filter(|logical| {
            live(&logical.request.instance)
                && logical.request.instance.scope == request.instance.scope
                && (request.instance.scope != AudioScope::Match
                    || logical.request.instance.epoch == request.instance.epoch)
        })
        .map(|logical| Occupant {
            id: logical.request.instance.id,
            policy: logical.request.admission,
            priority: f32::from_bits(logical.request.instance.priority.load(Ordering::Acquire)),
        })
        .collect();
    let victims = match crate::admission::plan(&policy, &occupants) {
        Ok(victims) => victims,
        Err(reason) => {
            reject(&request.instance, rejections, reason);
            return;
        }
    };
    let committed = instances
        .iter()
        .filter(|logical| {
            live(&logical.request.instance)
                && (logical.slot.is_some()
                    || (logical.source.is_none()
                        && logical.request.instance.audible.load(Ordering::Acquire)
                        && logical.request.instance.status() != InstanceStatus::Virtual))
                && (victims.binary_search(&logical.request.instance.id).is_err()
                    || protected_attack(logical))
        })
        .count();
    if source.is_none()
        && request.instance.audible.load(Ordering::Acquire)
        && committed >= PHYSICAL_VOICES
    {
        reject(
            &request.instance,
            rejections,
            AdmissionFailure::PhysicalBudget,
        );
        return;
    }
    for logical in instances.iter() {
        if victims.binary_search(&logical.request.instance.id).is_ok() {
            if protected_attack(logical) {
                logical
                    .request
                    .instance
                    .finish_attack
                    .store(true, Ordering::Release);
            } else {
                logical
                    .request
                    .instance
                    .stopped
                    .store(true, Ordering::Release);
            }
        }
    }
    request.instance.set_status(InstanceStatus::Accepted);
    let frame = request.frame;
    instances.push(LogicalInstance {
        request,
        slot: None,
        cursor: 0.0,
        advanced_at: frame,
        source,
        first_device_reported: false,
    });
}
