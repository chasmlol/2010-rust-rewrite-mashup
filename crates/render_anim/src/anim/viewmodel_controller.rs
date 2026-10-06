use std::sync::Arc;

use asset_anim::{ActiveAnim, ClipScheduler};
use asset_game::{
    ACTION_GOAL_TIME_SECS, ACTIVE_GOAL_WEIGHT, AdsOverlayConvention, IDLE_INTERRUPT_GOAL_TIME_SECS,
    INACTIVE_GOAL_WEIGHT, WEAPON_ANIM_SLOTS, WeaponAnimSlot, WeaponAnimations, playback_rate,
    slot_uses_native_rate,
};
use asset_iw4::size::WEAPON_ANIM_COUNT;

const DISPATCH_SLOT_START: usize = 1;
const DISPATCH_SLOT_END: usize = 0x22;

fn dispatch_slots() -> impl Iterator<Item = usize> {
    (DISPATCH_SLOT_START..=DISPATCH_SLOT_END).chain(WEAPON_ANIM_COUNT..WEAPON_ANIM_SLOTS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewmodelEvent {
    Fire,

    Raise { first: bool, quick: bool },
    Reload { empty: bool },
    ReloadStart,
    ReloadEnd,
    Rechamber,
    Drop,
    SprintIn,
    SprintLoop,
    SprintOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponState {
    Ready,
    Raising { first: bool, quick: bool },
    Firing,
    Reloading { empty: bool },
    ReloadStarting,
    ReloadEnding,
    Rechambering,
    Dropping,
    SprintIn,
    SprintLoop,
    SprintOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventResult {
    Started(WeaponState),
    IgnoredState(WeaponState),
    IgnoredMissingClip(WeaponAnimSlot),
}

#[derive(Debug, Clone, Default)]
pub struct AdvanceResult {
    pub notifications: Vec<asset_anim::ClipNotification>,
    pub discarded: u64,
}

#[derive(Debug)]
pub struct ViewmodelController {
    weapon: WeaponAnimations,
    tree: ClipScheduler,
    state: WeaponState,
    action: Option<WeaponAnimSlot>,

    action_remaining: Option<f32>,

    last_ads_frac: f32,

    predicted_perks0: u32,
}

impl ViewmodelController {
    pub fn new(weapon: WeaponAnimations) -> Self {
        let mut tree = ClipScheduler::new(WEAPON_ANIM_SLOTS);
        weapon
            .install_clips(&mut tree)
            .expect("weapon animation index is within tree");

        let mut this = Self {
            weapon,
            tree,
            state: WeaponState::Ready,
            action: None,
            action_remaining: None,
            last_ads_frac: 0.0,
            predicted_perks0: 0,
        };
        this.set_weight(
            WeaponAnimSlot::Idle,
            ACTIVE_GOAL_WEIGHT,
            ACTION_GOAL_TIME_SECS,
        );

        // Bones animated only by ADS clips still need their lowered pose at the hip.
        if let Some(clip) = this.weapon.clip(WeaponAnimSlot::AdsDown).cloned() {
            this.set_weight(WeaponAnimSlot::AdsDown, ACTIVE_GOAL_WEIGHT, 0.0);
            this.tree
                .set_time(WeaponAnimSlot::AdsDown.index(), clip.duration())
                .expect("ads down index is within tree");
            this.tree
                .set_rate(WeaponAnimSlot::AdsDown.index(), 0.0)
                .expect("ads down index is within tree");
        } else if this.weapon.ads_overlay == AdsOverlayConvention::PlayAdsAnim
            && this.weapon.clip(WeaponAnimSlot::AdsUp).is_some()
        {
            this.set_weight(WeaponAnimSlot::AdsUp, ACTIVE_GOAL_WEIGHT, 0.0);
        }
        this
    }

    pub fn state(&self) -> WeaponState {
        self.state
    }

    pub fn set_predicted_perks(&mut self, perks0: u32) {
        self.predicted_perks0 = perks0;
    }

    pub fn weapon_name(&self) -> &str {
        &self.weapon.name
    }

    pub fn weapon(&self) -> &WeaponAnimations {
        &self.weapon
    }

    pub fn active_anims(&self) -> impl Iterator<Item = ActiveAnim<'_>> {
        self.tree.active()
    }

    pub fn animation_weight(&self, slot: WeaponAnimSlot) -> f32 {
        self.tree.weight(slot.index()).unwrap_or(0.0)
    }

    pub fn animation_time(&self, slot: WeaponAnimSlot) -> f32 {
        self.tree.time(slot.index()).unwrap_or(0.0)
    }

    pub fn handle(&mut self, event: ViewmodelEvent) -> EventResult {
        if !self.event_allowed(event) {
            return EventResult::IgnoredState(self.state);
        }
        match event {
            ViewmodelEvent::Fire => {
                let slot = if self.last_ads_frac > 0.0
                    && self.weapon.clip(WeaponAnimSlot::AdsFire).is_some()
                {
                    WeaponAnimSlot::AdsFire
                } else {
                    WeaponAnimSlot::Fire
                };
                let timer = self.weapon.fire_time_ms;
                self.start_action(
                    slot,
                    WeaponState::Firing,
                    if timer > 0 { Some(timer) } else { None },
                )
            }
            ViewmodelEvent::Raise { first, quick } => {
                let slot = match (first, quick) {
                    (true, _) => WeaponAnimSlot::FirstRaise,
                    (false, true) => WeaponAnimSlot::QuickRaise,
                    (false, false) => WeaponAnimSlot::Raise,
                };

                let timer = match (first, quick) {
                    (false, false) => Some(self.weapon.raise_time_ms),
                    (false, true) => positive_ms(self.weapon.quick_raise_time_ms),
                    (true, _) => None,
                };
                self.start_action(slot, WeaponState::Raising { first, quick }, timer)
            }
            ViewmodelEvent::Reload { empty } => {
                let slot = if empty {
                    WeaponAnimSlot::ReloadEmpty
                } else {
                    WeaponAnimSlot::Reload
                };
                let timer = if empty {
                    positive_ms(self.weapon.reload_empty_time_ms)
                } else {
                    positive_ms(self.weapon.reload_time_ms)
                };
                self.start_action(slot, WeaponState::Reloading { empty }, timer)
            }
            ViewmodelEvent::ReloadStart => self.start_action(
                WeaponAnimSlot::ReloadStart,
                WeaponState::ReloadStarting,
                positive_ms(self.weapon.reload_start_time_ms),
            ),
            ViewmodelEvent::ReloadEnd => self.start_action(
                WeaponAnimSlot::ReloadEnd,
                WeaponState::ReloadEnding,
                positive_ms(self.weapon.reload_end_time_ms),
            ),
            ViewmodelEvent::Rechamber => {
                self.start_action(WeaponAnimSlot::Rechamber, WeaponState::Rechambering, None)
            }
            ViewmodelEvent::Drop => self.start_action(
                WeaponAnimSlot::Drop,
                WeaponState::Dropping,
                positive_ms(self.weapon.drop_time_ms),
            ),
            ViewmodelEvent::SprintIn => self.start_action(
                WeaponAnimSlot::SprintIn,
                WeaponState::SprintIn,
                positive_ms(self.weapon.sprint_raise_time_ms),
            ),
            ViewmodelEvent::SprintLoop => self.start_action(
                WeaponAnimSlot::SprintLoop,
                WeaponState::SprintLoop,
                positive_ms(self.weapon.sprint_loop_time_ms),
            ),
            ViewmodelEvent::SprintOut => self.start_action(
                WeaponAnimSlot::SprintOut,
                WeaponState::SprintOut,
                positive_ms(self.weapon.sprint_drop_time_ms),
            ),
        }
    }

    pub fn dispatch_sz_xanim_index(&mut self, slot_index: usize) -> EventResult {
        if slot_index == WeaponAnimSlot::AdsUp.index()
            || slot_index == WeaponAnimSlot::AdsDown.index()
        {
            return EventResult::IgnoredState(self.state);
        }
        let Some(slot) = WeaponAnimSlot::from_index(slot_index) else {
            return EventResult::IgnoredState(self.state);
        };
        if matches!(slot, WeaponAnimSlot::Idle | WeaponAnimSlot::EmptyIdle) {
            self.start_idle();
            return EventResult::Started(WeaponState::Ready);
        }
        let (state, timer) = self.state_and_timer_for_slot(slot);
        self.start_action(slot, state, timer)
    }

    fn state_and_timer_for_slot(&self, slot: WeaponAnimSlot) -> (WeaponState, Option<i32>) {
        match slot {
            WeaponAnimSlot::Fire
            | WeaponAnimSlot::AdsFire
            | WeaponAnimSlot::LastShot
            | WeaponAnimSlot::AdsLastShot
            | WeaponAnimSlot::HoldFire => {
                let timer = self.weapon.fire_time_ms;
                (
                    WeaponState::Firing,
                    if timer > 0 { Some(timer) } else { None },
                )
            }
            WeaponAnimSlot::FirstRaise => (
                WeaponState::Raising {
                    first: true,
                    quick: false,
                },
                None,
            ),
            WeaponAnimSlot::QuickRaise => (
                WeaponState::Raising {
                    first: false,
                    quick: true,
                },
                positive_ms(self.weapon.quick_raise_time_ms),
            ),

            WeaponAnimSlot::AltRaise => (
                WeaponState::Raising {
                    first: false,
                    quick: false,
                },
                positive_ms(self.weapon.alternate_raise_time_ms),
            ),
            WeaponAnimSlot::EmptyRaise => (
                WeaponState::Raising {
                    first: false,
                    quick: false,
                },
                None,
            ),
            WeaponAnimSlot::Raise => (
                WeaponState::Raising {
                    first: false,
                    quick: false,
                },
                Some(self.weapon.raise_time_ms),
            ),
            WeaponAnimSlot::Reload => (
                WeaponState::Reloading { empty: false },
                positive_ms(self.weapon.reload_time_ms),
            ),
            WeaponAnimSlot::ReloadEmpty => (
                WeaponState::Reloading { empty: true },
                positive_ms(self.weapon.reload_empty_time_ms),
            ),
            WeaponAnimSlot::ReloadQuick => (
                WeaponState::Reloading { empty: false },
                positive_ms(self.weapon.reload_quick_time_ms),
            ),
            WeaponAnimSlot::ReloadQuickEmpty => (
                WeaponState::Reloading { empty: true },
                positive_ms(self.weapon.reload_quick_empty_time_ms),
            ),
            WeaponAnimSlot::ReloadStart => (
                WeaponState::ReloadStarting,
                positive_ms(self.weapon.reload_start_time_ms),
            ),
            WeaponAnimSlot::ReloadEnd => (
                WeaponState::ReloadEnding,
                positive_ms(self.weapon.reload_end_time_ms),
            ),
            WeaponAnimSlot::Rechamber | WeaponAnimSlot::AdsRechamber => {
                (WeaponState::Rechambering, None)
            }
            WeaponAnimSlot::Drop => (WeaponState::Dropping, positive_ms(self.weapon.drop_time_ms)),
            WeaponAnimSlot::QuickDrop => (
                WeaponState::Dropping,
                positive_ms(self.weapon.quick_drop_time_ms),
            ),

            WeaponAnimSlot::AltDrop => (
                WeaponState::Dropping,
                positive_ms(self.weapon.alternate_drop_time_ms),
            ),
            WeaponAnimSlot::EmptyDrop => (WeaponState::Dropping, None),
            WeaponAnimSlot::SprintIn => (
                WeaponState::SprintIn,
                positive_ms(self.weapon.sprint_raise_time_ms),
            ),
            WeaponAnimSlot::SprintLoop => (
                WeaponState::SprintLoop,
                positive_ms(self.weapon.sprint_loop_time_ms),
            ),
            WeaponAnimSlot::SprintOut => (
                WeaponState::SprintOut,
                positive_ms(self.weapon.sprint_drop_time_ms),
            ),
            WeaponAnimSlot::Melee => (WeaponState::Ready, positive_ms(self.weapon.melee_time_ms)),
            WeaponAnimSlot::MeleeCharge => (
                WeaponState::Ready,
                positive_ms(self.weapon.melee_charge_time_ms),
            ),
            WeaponAnimSlot::Idle | WeaponAnimSlot::EmptyIdle => (WeaponState::Ready, None),
            WeaponAnimSlot::AdsUp | WeaponAnimSlot::AdsDown => (WeaponState::Ready, None),
        }
    }

    pub fn advance(&mut self, dt_secs: f32) -> AdvanceResult {
        let dt_secs = dt_secs.max(0.0);
        let advance = self.tree.advance(dt_secs);

        if let Some(remaining) = self.action_remaining.as_mut() {
            *remaining = (*remaining - dt_secs).max(0.0);
        }

        if matches!(self.state, WeaponState::Firing)
            && let Some(slot) = self.action
            && self.action_finished(slot)
        {
            self.finish_fire_cycle();
        }

        AdvanceResult {
            notifications: advance.notifications,
            discarded: advance.discarded,
        }
    }

    pub fn apply_idle_weap_anim(&mut self, empty_mag: bool) -> bool {
        if self.dispatch_range_unfinished() {
            return false;
        }
        self.start_idle_family(empty_mag, false);
        true
    }

    pub fn apply_force_idle_weap_anim(&mut self, empty_mag: bool) {
        let interrupt = self.dispatch_range_unfinished();
        self.start_idle_family(empty_mag, interrupt);
    }

    pub fn settle_fire_to_idle(&mut self) {
        if !matches!(self.state, WeaponState::Ready) || self.action.is_some() {
            return;
        }
        if self.animation_weight(WeaponAnimSlot::Fire) > 0.01 {
            self.start_idle();
        }
    }

    fn event_allowed(&self, event: ViewmodelEvent) -> bool {
        match event {
            ViewmodelEvent::Fire => {
                matches!(self.state, WeaponState::Ready | WeaponState::Firing)
            }

            ViewmodelEvent::SprintLoop => !matches!(self.state, WeaponState::SprintLoop),
            ViewmodelEvent::SprintIn => {
                matches!(
                    self.state,
                    WeaponState::Ready | WeaponState::Firing | WeaponState::SprintOut
                )
            }

            ViewmodelEvent::Raise { .. }
            | ViewmodelEvent::Reload { .. }
            | ViewmodelEvent::ReloadStart
            | ViewmodelEvent::ReloadEnd
            | ViewmodelEvent::Rechamber
            | ViewmodelEvent::Drop
            | ViewmodelEvent::SprintOut => true,
        }
    }

    pub fn apply_ads_overlay_frame(&mut self, f_weapon_pos_frac: f32) {
        let scrub = weapon_iw4::ads_overlay_scrub(f_weapon_pos_frac);
        self.last_ads_frac = scrub.ads_up_weight;
        if matches!(
            self.action,
            Some(WeaponAnimSlot::Melee | WeaponAnimSlot::MeleeCharge)
        ) {
            self.set_weight(WeaponAnimSlot::AdsUp, INACTIVE_GOAL_WEIGHT, 0.0);
            self.set_weight(WeaponAnimSlot::AdsDown, INACTIVE_GOAL_WEIGHT, 0.0);
            return;
        }
        const OVERLAY_SCRUB_RATE: f32 = 0.0;
        let aiming = f_weapon_pos_frac > 0.0;
        let has_ads_down = self.weapon.clip(WeaponAnimSlot::AdsDown).is_some();
        if let Some(clip) = self.weapon.clip(WeaponAnimSlot::AdsUp).cloned() {
            let time = clip.duration() * scrub.ads_up_time_norm;
            let ads_up_weight = match self.weapon.ads_overlay {
                AdsOverlayConvention::WeightIsFrac => scrub.ads_up_weight,
                AdsOverlayConvention::PlayAdsAnim => {
                    if has_ads_down && !aiming {
                        INACTIVE_GOAL_WEIGHT
                    } else {
                        ACTIVE_GOAL_WEIGHT
                    }
                }
            };
            self.tree
                .set_rate(WeaponAnimSlot::AdsUp.index(), OVERLAY_SCRUB_RATE)
                .expect("ads up index is within tree");
            self.tree
                .set_time(WeaponAnimSlot::AdsUp.index(), time)
                .expect("ads up index is within tree");
            self.tree
                .set_goal_weight(
                    WeaponAnimSlot::AdsUp.index(),
                    ads_up_weight,
                    ACTION_GOAL_TIME_SECS,
                )
                .expect("ads up index is within tree");
        }
        if let Some(clip) = self.weapon.clip(WeaponAnimSlot::AdsDown).cloned() {
            let time = clip.duration() * scrub.ads_down_time_norm;
            self.tree
                .set_rate(WeaponAnimSlot::AdsDown.index(), OVERLAY_SCRUB_RATE)
                .expect("ads down index is within tree");
            self.tree
                .set_time(WeaponAnimSlot::AdsDown.index(), time)
                .expect("ads down index is within tree");
            let ads_down_weight =
                if self.weapon.ads_overlay == AdsOverlayConvention::PlayAdsAnim && aiming {
                    INACTIVE_GOAL_WEIGHT
                } else {
                    ACTIVE_GOAL_WEIGHT
                };
            self.tree
                .set_goal_weight(
                    WeaponAnimSlot::AdsDown.index(),
                    ads_down_weight,
                    ACTION_GOAL_TIME_SECS,
                )
                .expect("ads down index is within tree");
        }
    }

    fn perk_scaled_reload_timer(&self, state: WeaponState, timer_ms: Option<i32>) -> Option<i32> {
        let reload_family = matches!(
            state,
            WeaponState::Reloading { .. } | WeaponState::ReloadStarting | WeaponState::ReloadEnding
        );
        if !(reload_family
            && weapon_iw4::perk_fastreload_eligible(
                self.predicted_perks0,
                self.weapon.inherits_perks,
            ))
        {
            return timer_ms;
        }
        let m = weapon_iw4::PERK_WEAP_RELOAD_MULTIPLIER_DEFAULT;
        if m <= 0.0 {
            return timer_ms;
        }
        timer_ms.map(|t| ((t as f32) * m).round() as i32)
    }

    fn start_action(
        &mut self,
        slot: WeaponAnimSlot,
        state: WeaponState,
        timer_ms: Option<i32>,
    ) -> EventResult {
        let Some(clip) = self.weapon.clip(slot).cloned() else {
            self.zero_dispatch_slots(ACTION_GOAL_TIME_SECS);
            self.action = None;
            self.action_remaining = None;
            self.state = state;
            return EventResult::IgnoredMissingClip(slot);
        };
        let duration = clip.duration();
        let timer_ms = self.perk_scaled_reload_timer(state, timer_ms);
        self.zero_dispatch_slots(ACTION_GOAL_TIME_SECS);
        self.tree
            .set_clip(slot.index(), Arc::clone(&clip))
            .expect("action index is within tree");
        let rate = if slot_uses_native_rate(slot.index()) {
            1.0
        } else {
            timer_ms
                .filter(|time| *time > 0)
                .and_then(|time| {
                    let len_ms = (duration * 1000.0).round() as i32;
                    Some(playback_rate(len_ms, time))
                })
                .unwrap_or(1.0)
        };
        self.tree
            .set_rate(slot.index(), rate)
            .expect("action index is within tree");
        self.tree
            .set_goal_weight(slot.index(), ACTIVE_GOAL_WEIGHT, ACTION_GOAL_TIME_SECS)
            .expect("action index is within tree");
        self.action = Some(slot);

        self.action_remaining = if clip.looping {
            None
        } else {
            timer_ms
                .filter(|time| *time > 0)
                .map(|time| time as f32 / 1000.0)
        };
        self.state = state;
        EventResult::Started(state)
    }

    fn start_idle(&mut self) {
        self.start_idle_family(false, false);
    }

    fn start_idle_family(&mut self, empty: bool, interrupt: bool) {
        let slot = if empty && self.weapon.clip(WeaponAnimSlot::EmptyIdle).is_some() {
            WeaponAnimSlot::EmptyIdle
        } else {
            WeaponAnimSlot::Idle
        };
        let goal_time = if interrupt {
            IDLE_INTERRUPT_GOAL_TIME_SECS
        } else {
            ACTION_GOAL_TIME_SECS
        };
        self.action = None;
        self.action_remaining = None;
        self.state = WeaponState::Ready;
        self.zero_dispatch_slots(goal_time);
        self.set_weight(slot, ACTIVE_GOAL_WEIGHT, goal_time);
    }

    fn dispatch_range_unfinished(&self) -> bool {
        for node in dispatch_slots() {
            let weight = self.tree.weight(node).unwrap_or(0.0);
            if weight <= 0.0 {
                continue;
            }
            let Some(clip) = self.weapon.clip_at(node) else {
                continue;
            };
            if clip.looping {
                return true;
            }
            let time = self.tree.time(node).unwrap_or(0.0);
            if time < clip.duration() {
                return true;
            }
        }
        false
    }

    fn zero_dispatch_slots(&mut self, goal_time: f32) {
        for node in dispatch_slots() {
            self.tree
                .set_goal_weight(node, INACTIVE_GOAL_WEIGHT, goal_time)
                .expect("action index is within tree");
            self.tree
                .set_rate(node, 1.0)
                .expect("action index is within tree");
        }
    }

    fn finish_fire_cycle(&mut self) {
        self.action = None;
        self.action_remaining = None;
        self.state = WeaponState::Ready;
    }

    fn action_finished(&self, slot: WeaponAnimSlot) -> bool {
        if let Some(remaining) = self.action_remaining {
            return remaining <= 0.0;
        }
        let Some(clip) = self.weapon.clip(slot) else {
            return true;
        };
        !clip.looping && self.animation_time(slot) >= clip.duration()
    }

    fn set_weight(&mut self, slot: WeaponAnimSlot, weight: f32, goal_time: f32) {
        if self.weapon.clip(slot).is_some() {
            self.tree
                .set_goal_weight(slot.index(), weight, goal_time)
                .expect("verified animation index is within tree");
        }
    }
}

fn positive_ms(ms: i32) -> Option<i32> {
    (ms > 0).then_some(ms)
}
