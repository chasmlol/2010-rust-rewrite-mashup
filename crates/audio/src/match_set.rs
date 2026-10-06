use std::collections::{BTreeSet, HashSet};

use asset_audio::SoundCatalog;
use asset_core::AssetNamespace;
use asset_game::WeaponRegistry;
use assets::{MapLoadProcess, MatchType10SoundHints, PreparedWeapons};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use frame::{ClientSet, LaunchIdentity, MatchTornDown, ReturnedToMenu};

use crate::aliases::movement_prepare_names;
use crate::ambient::{SoundBankCompose, SoundBankLoadAttempted, SoundBankNamespace};
use crate::clip_store::{ClipKey, ClipStore, clip_keys_for_alias};
use crate::playback::SoundBank;

#[derive(Resource, Default)]
pub struct AudioReady(pub frame::WorldReadiness);

#[derive(Resource)]
pub struct AudioSilent;

impl AudioSilent {
    pub fn active() -> bool {
        static SILENT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *SILENT.get_or_init(|| {
            std::env::var("IW4L_SOUND").is_ok_and(|value| matches!(value.as_str(), "off" | "0"))
        })
    }
}

const PREFETCH_PER_PASS: usize = 64;

const MATCH_HUD_PULSE: &[&str] = &["ui_pulse_text_type", "ui_pulse_text_delete"];

const MENU_CODE: [&str; 2] = ["mouse_over", "mouse_click"];

const MATCH_CLOCK: &[&str] = &["ui_mp_timer_countdown"];

#[derive(Default)]
struct MatchRequests {
    required: HashSet<ClipKey>,
    missing: BTreeSet<String>,
    resolved_aliases: usize,
    capacity_failure: bool,
}

#[derive(Resource, Default)]
struct MatchClipPrep {
    generation: frame::WorldGeneration,
    submitted: bool,
    capacity_failure: bool,
    required: HashSet<ClipKey>,
    total: usize,
    stage: Option<asset_transport::StageHandle>,
    /// Resident sample bytes this process had already produced when the set
    /// was queued. The decoders count for the life of the process, so what
    /// this match cost is the distance from here.
    sample_bytes_at_queue: u64,
}

#[derive(SystemParam)]
struct AudioLoadScope<'w> {
    silent: Option<Res<'w, AudioSilent>>,
    accepted: Option<Res<'w, assets::MatchLoadAccepted>>,
    incoming: Option<Res<'w, assets::PreparedMatchReady>>,
    installed: Res<'w, frame::WorldGeneration>,
}

impl AudioLoadScope<'_> {
    fn generation(&self) -> frame::WorldGeneration {
        self.accepted
            .as_ref()
            .map(|accepted| accepted.load_key)
            .or_else(|| self.incoming.as_ref().map(|incoming| incoming.load_key))
            .map(|key| frame::WorldGeneration::from_install(key.local_load_request_id))
            .unwrap_or(*self.installed)
    }
}

fn prepared_sample_bytes() -> u64 {
    crate::clip_prep_cost()
        .paths
        .iter()
        .map(|(_, cost)| cost.sample_bytes)
        .sum()
}

pub(crate) fn register(app: &mut App) {
    if AudioSilent::active() {
        diag::info!(Audio, "audio: Silent (IW4L_SOUND=off)");
        app.insert_resource(AudioSilent);
    }
    app.init_resource::<AudioReady>()
        .init_resource::<MatchClipPrep>()
        .init_resource::<crate::match_voices::AnnouncerRoutes>()
        .add_systems(
            Update,
            (
                queue_match_clips.after(crate::ambient::install_sound_bank),
                poll_match_audio_ready.after(queue_match_clips),
                reset_match_audio_on_match_end,
            )
                .in_set(ClientSet::Load),
        );
}

fn reset_match_audio_on_match_end(
    mut torn: MessageReader<MatchTornDown>,
    mut returned: MessageReader<ReturnedToMenu>,
    mut ready: ResMut<AudioReady>,
    mut prep: ResMut<MatchClipPrep>,
    mut announcer: ResMut<crate::match_voices::AnnouncerRoutes>,
) {
    let retired = torn.read().fold(false, |retired, event| {
        retired || event.world_generation == prep.generation
    });
    let menu = returned.read().fold(false, |menu, event| {
        menu || (event.had_world && prep.generation.0.is_none())
    });
    if !retired && !menu {
        return;
    }
    *ready = AudioReady::default();
    if let Some(stage) = prep.stage.take() {
        stage.cancel();
    }
    *prep = MatchClipPrep::default();
    *announcer = Default::default();
}

fn queue_match_clips(
    attempted: Res<SoundBankLoadAttempted>,
    walk: Option<Res<SoundBankCompose>>,
    mut clips: Option<ResMut<ClipStore>>,
    weapons: Option<Res<PreparedWeapons>>,
    type10: Option<Res<MatchType10SoundHints>>,
    bank: Option<Res<SoundBank>>,
    identity: Option<Res<LaunchIdentity>>,
    catalog: Option<Res<asset_game::MenuCatalog>>,
    script_sound: Option<Res<asset_audio::SessionMapScriptSound>>,
    teams: Option<Res<asset_game::SessionTeamSettings>>,
    namespace: Option<Res<SoundBankNamespace>>,
    loading: Option<Res<MapLoadProcess>>,
    mut prep: ResMut<MatchClipPrep>,
    mut announcer: ResMut<crate::match_voices::AnnouncerRoutes>,
    mut ready: ResMut<AudioReady>,
    load_scope: AudioLoadScope,
) {
    let scope = load_scope.generation();
    if scope.0.is_none() {
        return;
    }
    if prep.generation != scope {
        if let Some(stage) = prep.stage.take() {
            stage.cancel();
        }
        *prep = MatchClipPrep {
            generation: scope,
            ..Default::default()
        };
        ready.0 = frame::WorldReadiness::new(scope, frame::ReadinessState::Pending);
    }
    if ready.0.state != frame::ReadinessState::Pending || prep.submitted {
        return;
    }
    if load_scope.silent.is_some() {
        ready.0.state = frame::ReadinessState::Silent;
        if let Some(loading) = loading.as_ref() {
            loading
                .progress
                .record_skipped(asset_transport::StageId::Audio);
        }
        diag::info!(Audio, "audio: AudioReady — Silent, no clips prepared");
        return;
    }
    if !attempted.0 {
        return;
    }
    if walk.is_some() {
        return;
    }
    let Some(clips) = clips.as_mut() else {
        ready.0.state = frame::ReadinessState::Failed;
        if let Some(loading) = loading.as_ref() {
            loading
                .progress
                .begin(asset_transport::StageId::Audio, None)
                .fail();
        }
        diag::warn!(
            Audio,
            "audio: preparation failed — sound bank did not install"
        );
        return;
    };
    if *load_scope.installed != scope {
        return;
    }
    let Some(weapons) = weapons else {
        return;
    };
    let Some(type10) = type10 else {
        return;
    };
    let Some(bank) = bank else {
        return;
    };
    let Some(namespace) = namespace else {
        return;
    };
    let Some(script_sound) = script_sound else {
        return;
    };
    if namespace.generation != scope {
        return;
    }
    let mut set = MatchRequests::default();
    let mut aliases = 0usize;
    for weapon in 1..=weapons.registry().len() as u32 {
        aliases += request_weapon_aliases(clips, &bank.0, &weapons.registry(), weapon, &mut set);
    }
    for alias in movement_prepare_names() {
        aliases += 1;
        request_named(clips, &bank.0, AssetNamespace::Iw4, alias, &mut set);
    }
    if let Some(identity) = identity.as_deref() {
        let ns = namespace.namespace;
        let zone = namespace.zone.as_str();
        if zone != identity.zone {
            return;
        }
        for emitter in bank.0.createfx_loop_sounds(ns, zone) {
            aliases += 1;
            request_named(clips, &bank.0, ns, &emitter.soundalias, &mut set);
        }
    }
    if let Some(identity) = identity.as_deref() {
        let (allies, axis) = crate::match_voices::voice_prefixes_for_zone(
            catalog.as_deref(),
            Some(bank.as_ref()),
            identity,
        );
        let voices =
            crate::match_voices::team_voice_aliases(&bank.0, allies.as_deref(), axis.as_deref());
        let native = teams.as_deref().map(|teams| &teams.0);
        *announcer = crate::match_voices::AnnouncerRoutes::build(
            &bank.0,
            namespace.namespace,
            [allies.as_deref(), axis.as_deref()],
            native,
            &voices,
        );
        diag::info!(
            Audio,
            "audio: announcer: {} of {} team voice aliases play as {:?}",
            announcer.len(),
            voices.len(),
            namespace.namespace
        );
        for alias in &voices {
            aliases += 1;
            let (ns, alias) = announcer.route(alias);
            request_named(clips, &bank.0, ns, alias, &mut set);
        }
    }
    if let Some(alias) = script_sound.0.ambient_alias.as_deref() {
        aliases += 1;
        request_named(clips, &bank.0, namespace.namespace, alias, &mut set);
    }
    if let Some(catalog) = catalog.as_deref() {
        for alias in catalog.played_sound_aliases().into_iter().chain(MENU_CODE) {
            aliases += 1;
            request_named(clips, &bank.0, AssetNamespace::Iw4, alias, &mut set);
        }
    }
    for alias in MATCH_HUD_PULSE
        .iter()
        .chain([&"motiontracker_ping", &"motiontracker_pong"])
        .chain(crate::weapon_lock::ALIASES.iter())
        .chain([&gamemode_iw4::damage_feedback::HIT_ALERT_ALIAS])
        .chain(MATCH_CLOCK)
    {
        aliases += 1;
        request_named(clips, &bank.0, AssetNamespace::Iw4, alias, &mut set);
    }
    let mut breath_policies = HashSet::new();
    for weapon in 1..=weapons.registry().len() as u32 {
        if weapons
            .registry()
            .bind_published_row(weapon)
            .ok()
            .and_then(|weapon| weapon.hud_facts())
            .is_some_and(|facts| facts.can_hold_breath)
            && let Some(policy) = weapons.registry().semantic_policy_of(weapon)
            && breath_policies.insert((policy.cue_namespace.namespace(), policy.breath_cues))
        {
            let ns = policy.cue_namespace.namespace();
            for alias in policy.breath_cues.aliases() {
                aliases += 1;
                request_named(clips, &bank.0, ns, alias, &mut set);
            }
        }
    }
    for alias in &type10.0 {
        aliases += 1;
        request_fx_type10(clips, &bank.0, alias, &mut set);
    }
    if !set.missing.is_empty() {
        let names: Vec<&str> = set.missing.iter().map(String::as_str).collect();
        diag::warn!(
            Audio,
            "audio: match-set gap: {} aliases name no loaded sound: {}",
            names.len(),
            names.join(" ")
        );
    }
    prep.total = set.required.len();
    prep.required = set.required;
    prep.capacity_failure = set.capacity_failure;
    prep.submitted = true;
    if let Some(loading) = loading {
        prep.sample_bytes_at_queue = prepared_sample_bytes();
        if prep.total == 0 && set.resolved_aliases > 0 {
            loading
                .progress
                .record_reused_scoped(asset_transport::StageId::Audio, "clips");
        } else {
            prep.stage = Some(
                loading
                    .progress
                    .begin(asset_transport::StageId::Audio, Some(prep.total as u64)),
            );
        }
    }
    diag::info!(
        Audio,
        "audio: match-set queued {} aliases ({} type-10), {} required clips ({} workers); resident reused={} clips/{}B",
        aliases,
        type10.0.len(),
        prep.total,
        clips.workers(),
        clips.reused_resident().0,
        clips.reused_resident().1,
    );
    if prep.capacity_failure {
        fail_capacity(&mut ready, &mut prep, Some(&mut **clips));
    } else if prep.total == 0 {
        mark_ready(&mut ready, &mut prep, Some(&mut **clips));
    }
}

fn poll_match_audio_ready(
    mut clips: Option<ResMut<ClipStore>>,
    mut prep: ResMut<MatchClipPrep>,
    mut ready: ResMut<AudioReady>,
    load_scope: AudioLoadScope,
) {
    let scope = load_scope.generation();
    if prep.generation != scope
        || ready.0.generation != scope
        || ready.0.state != frame::ReadinessState::Pending
        || !prep.submitted
        || prep.capacity_failure
    {
        return;
    }
    let Some(clips) = clips.as_mut() else {
        return;
    };
    let mut capacity_failure = false;
    let mut submissions = PREFETCH_PER_PASS;
    prep.required.retain(|key| {
        if submissions != 0 {
            match clips.prefetch(key.clone()) {
                crate::clip_store::MediaRequest::Submitted
                | crate::clip_store::MediaRequest::Resident => submissions -= 1,
                crate::clip_store::MediaRequest::Deferred => submissions = 0,
                crate::clip_store::MediaRequest::Existing => {}
                crate::clip_store::MediaRequest::Refused => {
                    capacity_failure = true;
                    submissions = 0;
                }
            }
        }
        match clips.ready(key) {
            Some(Err(
                crate::clip_store::ClipError::RequestLimit
                | crate::clip_store::ClipError::InvalidPcm(crate::media::PcmError::MemoryLimit),
            )) => {
                capacity_failure = true;
                false
            }
            Some(_) => false,
            None => true,
        }
    });
    if capacity_failure {
        prep.capacity_failure = true;
        fail_capacity(&mut ready, &mut prep, Some(&mut **clips));
        return;
    }
    let done = prep.total.saturating_sub(prep.required.len());
    let decoded = prepared_sample_bytes().saturating_sub(prep.sample_bytes_at_queue);
    if let Some(stage) = prep.stage.as_ref() {
        stage.set_completed(done as u64);
        stage.set_bytes(decoded);
    }
    if prep.required.is_empty() {
        mark_ready(&mut ready, &mut prep, Some(&mut **clips));
    }
}

fn fail_capacity(ready: &mut AudioReady, prep: &mut MatchClipPrep, clips: Option<&mut ClipStore>) {
    if let Some(stage) = prep.stage.take() {
        stage.fail();
    }
    diag::warn!(
        Audio,
        "audio: match media capacity exceeded; {} of {} clips not prepared",
        prep.required.len(),
        prep.total
    );
    ready.0.state = frame::ReadinessState::Failed;
    if let Some(clips) = clips {
        clips.arm_match_live();
    }
}

fn mark_ready(ready: &mut AudioReady, prep: &mut MatchClipPrep, clips: Option<&mut ClipStore>) {
    if let Some(stage) = prep.stage.take() {
        stage.set_completed(prep.total as u64);
        stage.set_bytes(prepared_sample_bytes().saturating_sub(prep.sample_bytes_at_queue));
        stage.done();
    }
    if ready.0.generation == prep.generation && ready.0.state == frame::ReadinessState::Pending {
        ready.0.state = frame::ReadinessState::Ready;
        diag::info!(Audio, "audio: AudioReady ({} clips prepared)", prep.total);
        if let Some(clips) = clips {
            clips.arm_match_live();
        }
    }
}

fn request_named(
    clips: &mut ClipStore,
    bank: &SoundCatalog,
    ns: AssetNamespace,
    alias: &str,
    set: &mut MatchRequests,
) {
    let keys = clip_keys_for_alias(bank, ns, alias);
    if keys.is_empty() {
        set.missing.insert(alias.to_owned());
    } else {
        set.resolved_aliases += 1;
    }
    for key in keys.into_iter().filter(|key| key.is_loaded()) {
        match clips.ready(&key) {
            Some(Err(
                crate::clip_store::ClipError::RequestLimit
                | crate::clip_store::ClipError::InvalidPcm(crate::media::PcmError::MemoryLimit),
            )) => set.capacity_failure = true,
            Some(_) => {}
            None => {
                if set.required.len() == crate::clip_store::MEDIA_REQUEST_LIMIT
                    && !set.required.contains(&key)
                {
                    set.capacity_failure = true;
                } else {
                    set.required.insert(key);
                }
            }
        }
    }
}

fn request_fx_type10(
    clips: &mut ClipStore,
    bank: &SoundCatalog,
    alias: &str,
    set: &mut MatchRequests,
) {
    let Some(ns) = bank
        .index_unique(alias)
        .and_then(|index| bank.namespace_of_alias(index))
    else {
        return;
    };
    request_named(clips, bank, ns, alias, set);
}

fn request_weapon_aliases(
    clips: &mut ClipStore,
    bank: &SoundCatalog,
    registry: &WeaponRegistry,
    weapon: u32,
    set: &mut MatchRequests,
) -> usize {
    let sounds = registry.sounds_of(weapon);
    let mut n = 0usize;
    if let Some(aliases) = sounds {
        for alias in aliases.reachable_aliases() {
            n += 1;
            if let Some(ns) = registry.sound_namespace_for(weapon, alias) {
                request_named(clips, bank, ns, alias, set);
            }
        }
    }
    let mut seen = HashSet::new();
    for alias in registry.notetrack_sound_aliases_of(weapon) {
        if seen.insert(alias) {
            n += 1;
            if let Some(ns) = registry.sound_namespace_for(weapon, alias) {
                request_named(clips, bank, ns, alias, set);
            }
        }
    }
    n
}
