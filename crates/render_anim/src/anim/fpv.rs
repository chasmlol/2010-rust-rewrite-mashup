use asset_game::WeaponAnimSlot;

use crate::{EventResult, ViewmodelController, ViewmodelEvent, WeaponState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentFpvEvent {
    Fire,

    ReleaseAttack,
    RaiseFirst,
    Raise,
    Reload,
    ReloadEmpty,
    ReloadStart,
    ReloadEnd,
    Rechamber,
    Drop,
    SprintIn,
    SprintLoop,
    SprintOut,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AuthorityFpvCues {
    pub shot_accepted: bool,
    pub spawned: bool,
    pub attack_released: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FpvPoseSample {
    pub state: WeaponState,

    pub clip_name: Option<String>,
    pub clip_time: f32,
    pub fire_weight: f32,
    pub idle_weight: f32,
}

pub fn map_authority_to_fpv(cues: AuthorityFpvCues) -> Vec<PresentFpvEvent> {
    let mut out = Vec::new();

    if cues.shot_accepted {
        out.push(PresentFpvEvent::Fire);
    }
    if cues.attack_released {
        out.push(PresentFpvEvent::ReleaseAttack);
    }

    out
}

fn viewmodel_event_for(event: PresentFpvEvent) -> Option<ViewmodelEvent> {
    Some(match event {
        PresentFpvEvent::Fire => ViewmodelEvent::Fire,
        PresentFpvEvent::ReleaseAttack => return None,
        PresentFpvEvent::RaiseFirst => ViewmodelEvent::Raise {
            first: true,
            quick: false,
        },
        PresentFpvEvent::Raise => ViewmodelEvent::Raise {
            first: false,
            quick: false,
        },
        PresentFpvEvent::Reload => ViewmodelEvent::Reload { empty: false },
        PresentFpvEvent::ReloadEmpty => ViewmodelEvent::Reload { empty: true },
        PresentFpvEvent::ReloadStart => ViewmodelEvent::ReloadStart,
        PresentFpvEvent::ReloadEnd => ViewmodelEvent::ReloadEnd,
        PresentFpvEvent::Rechamber => ViewmodelEvent::Rechamber,
        PresentFpvEvent::Drop => ViewmodelEvent::Drop,
        PresentFpvEvent::SprintIn => ViewmodelEvent::SprintIn,
        PresentFpvEvent::SprintLoop => ViewmodelEvent::SprintLoop,
        PresentFpvEvent::SprintOut => ViewmodelEvent::SprintOut,
    })
}

fn apply_present_events(controller: &mut ViewmodelController, events: &[PresentFpvEvent]) {
    for event in events {
        match viewmodel_event_for(*event) {
            None => controller.settle_fire_to_idle(),
            Some(vm) => report_fpv_event_result(*event, controller.handle(vm)),
        }
    }
}

pub fn is_predicted_fire_weap_anim(masked: u32) -> bool {
    use weapon_iw4::weap_anim_event as event_id;
    matches!(
        masked,
        event_id::FIRE | event_id::LASTSHOT | event_id::ADS_FIRE | event_id::ADS_LASTSHOT
    )
}

fn report_dispatch_slot(slot: usize, result: EventResult) {
    match result {
        EventResult::Started(_) => {}
        EventResult::IgnoredState(state) => {
            diag::warn!(Fpv, "fpv: szXAnims[{slot}] ignored in state {state:?}");
        }
        EventResult::IgnoredMissingClip(named) => {
            diag::warn!(
                Fpv,
                "fpv: szXAnims[{slot}] ({named:?}) missing — not started"
            );
        }
    }
}

fn report_fpv_event_result(event: PresentFpvEvent, result: EventResult) {
    match result {
        EventResult::Started(_) => {}
        EventResult::IgnoredState(state) => {
            diag::warn!(Fpv, "fpv: {event:?} ignored in state {state:?}");
        }
        EventResult::IgnoredMissingClip(slot) => {
            diag::warn!(Fpv, "fpv: {event:?} ignored — szXAnims[{slot:?}] missing");
        }
    }
}

pub fn sample_pose(controller: &ViewmodelController) -> FpvPoseSample {
    let active = controller.active_anims().max_by(|a, b| {
        a.weight
            .partial_cmp(&b.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    FpvPoseSample {
        state: controller.state(),
        clip_name: active.map(|a| a.clip.name.clone()),
        clip_time: active.map(|a| a.time).unwrap_or(0.0),
        fire_weight: controller.animation_weight(WeaponAnimSlot::Fire),
        idle_weight: controller.animation_weight(WeaponAnimSlot::Idle),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FpvAuthoritySample {
    pub tick: u32,
    pub weaponstate: i32,
    pub cues: AuthorityFpvCues,

    pub sprinting: bool,

    pub ads_frac: f32,

    pub weap_anim: i32,

    pub weap_anim_secondary: i32,

    pub last_weapon_hand: i32,

    pub perks0: u32,

    pub clip_ammo: Option<i32>,

    pub left_clip_ammo: Option<i32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalShotIdentity {
    pub life_sequence: u32,
    pub hand: u8,
    pub weapon_shot_count: i32,

    pub weap_anim_restart: bool,
}

#[derive(Clone, Debug, Default)]
pub struct LocalShotCursor {
    last: Option<LocalShotIdentity>,
}

impl LocalShotCursor {
    pub fn clear(&mut self) {
        self.last = None;
    }

    pub fn observe(&mut self, identity: LocalShotIdentity) -> bool {
        match self.last {
            None => {
                self.last = Some(identity);
                false
            }
            Some(prev) if prev == identity => false,
            Some(prev) => {
                let count_edge = identity.weapon_shot_count != prev.weapon_shot_count;
                let restart_edge = identity.weap_anim_restart != prev.weap_anim_restart
                    && identity.weapon_shot_count == prev.weapon_shot_count;
                self.last = Some(identity);
                count_edge || restart_edge
            }
        }
    }

    pub fn last(&self) -> Option<LocalShotIdentity> {
        self.last
    }
}

pub fn local_shot_identity(
    life_sequence: u32,
    hand: u8,
    weapon_shot_count: i32,
    weap_anim: i32,
) -> LocalShotIdentity {
    LocalShotIdentity {
        life_sequence,
        hand,
        weapon_shot_count,
        weap_anim_restart: (weap_anim as u32 & 0x200) != 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeapAnimEdge {
    Unchanged,

    Idle,

    ForceIdle,

    Dispatch(usize),
}

#[derive(Clone, Debug, Default)]
pub struct FpvPresentState {
    pub last_tick: Option<u32>,

    pub last_ads_frac: Option<f32>,

    pub last_weap_anim: Option<i32>,

    pub last_weap_anim_secondary: Option<i32>,

    pub local_shot: LocalShotCursor,
}

impl FpvPresentState {
    pub fn clear(&mut self) {
        self.last_tick = None;
        self.last_ads_frac = None;
        self.last_weap_anim = None;
        self.last_weap_anim_secondary = None;
        self.local_shot.clear();
    }

    pub fn forget_weap_anim(&mut self) {
        self.last_weap_anim = None;
        self.last_weap_anim_secondary = None;
    }

    pub fn observe_weap_anim_edge(&mut self, raw: i32) -> WeapAnimEdge {
        observe_weap_anim_edge_on(&mut self.last_weap_anim, raw)
    }

    pub fn observe_weap_anim_secondary_edge(&mut self, raw: i32) -> WeapAnimEdge {
        observe_weap_anim_edge_on(&mut self.last_weap_anim_secondary, raw)
    }
}

#[derive(Debug)]
pub struct EquippedFpv {
    pub gun_xmodel: String,
    pub gun_index: assets::FpvMeshIndex,
    pub hands_index: assets::FpvMeshIndex,
    pub namespace: asset_core::AssetNamespace,
    pub hands: asset_model::FpvHands,
    pub controller: ViewmodelController,

    pub left: Option<ViewmodelController>,
}

impl EquippedFpv {
    pub fn new(
        gun_xmodel: impl Into<String>,
        gun_index: assets::FpvMeshIndex,
        hands_index: assets::FpvMeshIndex,
        namespace: asset_core::AssetNamespace,
        hands: asset_model::FpvHands,
        controller: ViewmodelController,
        left: Option<ViewmodelController>,
    ) -> Self {
        Self {
            gun_xmodel: gun_xmodel.into(),
            gun_index,
            hands_index,
            namespace,
            hands,
            controller,
            left,
        }
    }
}

pub fn events_for_authority_tick(
    present: &mut FpvPresentState,
    sample: FpvAuthoritySample,
) -> Vec<PresentFpvEvent> {
    if present.last_tick == Some(sample.tick) {
        return Vec::new();
    }
    let events = map_authority_to_fpv(sample.cues);
    present.last_tick = Some(sample.tick);
    present.last_ads_frac = Some(sample.ads_frac);
    events
}

#[derive(Clone, Debug)]
pub struct FpvNotetrack {
    pub hand: u8,
    pub notification: asset_anim::ClipNotification,
}

#[derive(Default)]
pub struct FpvNotetracks {
    pub records: Vec<FpvNotetrack>,
    pub discarded: u64,
}

impl FpvNotetracks {
    pub fn into_audio_batch(
        self,
        generation: frame::WorldGeneration,
        timeline: u64,
        client: sim::ClientId,
        life: sim::LifeSequence,
        weapon: u32,
        tick: sim::Tick,
    ) -> audio::ViewmodelNotetracks {
        audio::ViewmodelNotetracks {
            generation,
            client,
            life,
            weapon,
            records: self
                .records
                .into_iter()
                .map(|record| {
                    let notify = record.notification;
                    audio::ViewmodelNotetrack {
                        event: audio::AudioEvent::from_animation(
                            generation,
                            timeline,
                            client,
                            tick,
                            audio::AnimationMarkerId {
                                controller: notify.scheduler,
                                playback: notify.playback,
                                node: notify.node,
                                cycle: notify.cycle,
                                marker: notify.marker,
                                hand: record.hand,
                                life: life.0,
                                weapon,
                            },
                        ),
                        name: notify.name,
                    }
                })
                .collect(),
            discarded: self.discarded,
        }
    }
}

pub fn tick_equipped_fpv_with_predicted_fire(
    equipped: &mut EquippedFpv,
    present: &mut FpvPresentState,
    sample: Option<FpvAuthoritySample>,
    predicted_local_fire: bool,
    dt_secs: f32,
) -> (FpvPoseSample, FpvNotetracks) {
    tick_equipped_fpv_with_extra_events(
        equipped,
        present,
        sample,
        predicted_local_fire,
        &[],
        dt_secs,
    )
}

pub fn tick_equipped_fpv_with_extra_events(
    equipped: &mut EquippedFpv,
    present: &mut FpvPresentState,
    sample: Option<FpvAuthoritySample>,
    predicted_local_fire: bool,
    extra_events: &[PresentFpvEvent],
    dt_secs: f32,
) -> (FpvPoseSample, FpvNotetracks) {
    let mut events = match sample {
        Some(sample) => events_for_authority_tick(present, sample),
        None => Vec::new(),
    };
    if predicted_local_fire && !events.contains(&PresentFpvEvent::Fire) {
        events.push(PresentFpvEvent::Fire);
    }
    for event in extra_events {
        if !events.contains(event) {
            events.push(*event);
        }
    }
    if let Some(sample) = sample {
        equipped.controller.set_predicted_perks(sample.perks0);
    }
    apply_present_events(&mut equipped.controller, &events);
    if let Some(sample) = sample {
        match present.observe_weap_anim_edge(sample.weap_anim) {
            WeapAnimEdge::Unchanged => {}
            WeapAnimEdge::Idle => {
                let empty_mag = sample.clip_ammo == Some(0);
                if !equipped.controller.apply_idle_weap_anim(empty_mag) {
                    present.last_weap_anim = None;
                }
            }
            WeapAnimEdge::ForceIdle => {
                equipped
                    .controller
                    .apply_force_idle_weap_anim(sample.clip_ammo == Some(0));
            }
            WeapAnimEdge::Dispatch(slot) => {
                report_dispatch_slot(slot, equipped.controller.dispatch_sz_xanim_index(slot));
            }
        }
    }
    let advance = equipped.controller.advance(dt_secs);
    let mut notetracks = FpvNotetracks {
        records: advance
            .notifications
            .into_iter()
            .map(|notification| FpvNotetrack {
                hand: 0,
                notification,
            })
            .collect(),
        discarded: advance.discarded,
    };
    let ads_frac = sample
        .map(|s| s.ads_frac)
        .or(present.last_ads_frac)
        .unwrap_or(0.0);
    equipped.controller.apply_ads_overlay_frame(ads_frac);
    if let (Some(left), Some(sample)) = (equipped.left.as_mut(), sample) {
        if sample.last_weapon_hand == 1 {
            left.set_predicted_perks(sample.perks0);
            match present.observe_weap_anim_secondary_edge(sample.weap_anim_secondary) {
                WeapAnimEdge::Unchanged => {}
                WeapAnimEdge::Idle => {
                    if !left.apply_idle_weap_anim(sample.left_clip_ammo == Some(0)) {
                        present.last_weap_anim_secondary = None;
                    }
                }
                WeapAnimEdge::ForceIdle => {
                    left.apply_force_idle_weap_anim(sample.left_clip_ammo == Some(0));
                }
                WeapAnimEdge::Dispatch(slot) => {
                    report_dispatch_slot(slot, left.dispatch_sz_xanim_index(slot));
                }
            }
            let advance = left.advance(dt_secs);
            notetracks
                .records
                .extend(
                    advance
                        .notifications
                        .into_iter()
                        .map(|notification| FpvNotetrack {
                            hand: 1,
                            notification,
                        }),
                );
            notetracks.discarded = notetracks.discarded.saturating_add(advance.discarded);

            left.apply_ads_overlay_frame(0.0);
        }
    }
    (sample_pose(&equipped.controller), notetracks)
}

fn observe_weap_anim_edge_on(previous: &mut Option<i32>, raw: i32) -> WeapAnimEdge {
    let last = previous.replace(raw);
    if last == Some(raw) {
        return WeapAnimEdge::Unchanged;
    }
    let masked = raw as u32 & weapon_iw4::WEAP_ANIM_EVENT_MASK;
    match masked {
        0 => WeapAnimEdge::Idle,
        1 => WeapAnimEdge::ForceIdle,
        _ => match weapon_iw4::slot_for_weap_anim_event(masked) {
            Some(slot) => WeapAnimEdge::Dispatch(slot),
            None => WeapAnimEdge::Idle,
        },
    }
}
