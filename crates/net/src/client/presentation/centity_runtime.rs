use entity_iw4::{
    ET_PLAYER, ET_PLAYER_CORPSE, EntityState, TR_INTERPOLATE, Trajectory, evaluate_trajectory,
};
use playerstate_iw4::{AnimPair, PLAYER_CORPSE_ENTITY_BASE, PlayerState, eflags, other_flags};
use sim::{ClientId, PlayerCorpseSlot};

pub const EFLAGS_TELEPORT: u32 = 2;

pub const CURRENT_VALID_IN_NEXT: u8 = 0x1;

const EFLAGS_CLONE_TREE: u32 = 0x80000;

pub const EFLAGS_DEAD: u32 = 0x20000;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CurrentLerpState {
    pub e_flags: u32,
    pub pos: Trajectory,
    pub apos: Trajectory,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CEntityFxHandle {
    #[default]
    None,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CEntityDobjHandle {
    #[default]
    None,
}

#[derive(bevy::prelude::Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct CEntityRuntime {
    pub next_state: EntityState,

    pub presented_player: Option<(EntityState, i32)>,
    pub current: CurrentLerpState,
    pub current_valid: u8,
    pub previous_event_sequence: i32,
    pub pose_e_type: u8,
    pub origin: [f32; 3],
    pub angles: [f32; 3],

    pub pose_time_ms: i32,

    pub previous_pose: Option<(CurrentLerpState, i32)>,
    pub fx_handle: CEntityFxHandle,
    pub dobj_handle: CEntityDobjHandle,
}

impl CEntityRuntime {
    pub fn in_next_snap(&self) -> bool {
        self.current_valid & CURRENT_VALID_IN_NEXT != 0
    }

    pub fn adopted_origin(&self) -> [f32; 3] {
        evaluate_trajectory(&self.current.pos, self.pose_time_ms)
    }

    pub fn present_pose(&mut self, at_time_ms: i32) {
        let Some((previous, previous_time)) = self.previous_pose else {
            return;
        };
        if !self.in_next_snap()
            || !matches!(self.pose_e_type as i32, ET_PLAYER | ET_PLAYER_CORPSE)
            || previous.pos.tr_type != TR_INTERPOLATE
            || self.current.pos.tr_type != TR_INTERPOLATE
        {
            return;
        }
        let span = self.pose_time_ms - previous_time;
        if span <= 0 {
            return;
        }
        let fraction = ((at_time_ms - previous_time) as f32 / span as f32).clamp(0.0, 1.0);
        let old_origin = evaluate_trajectory(&previous.pos, previous_time);
        let old_angles = evaluate_trajectory(&previous.apos, previous_time);
        let origin = self.adopted_origin();
        let angles = evaluate_trajectory(&self.current.apos, self.pose_time_ms);
        for axis in 0..3 {
            self.origin[axis] = old_origin[axis] + fraction * (origin[axis] - old_origin[axis]);
            self.angles[axis] = old_angles[axis]
                + fraction * math_iw4::angle_subtract(angles[axis], old_angles[axis]);
        }
    }

    pub fn transition_copy_lerp(&mut self) {
        self.current = lerp_from_entity_state(&self.next_state);
        self.pose_e_type = u8::try_from(self.next_state.e_type).unwrap_or(0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerDrawGate {
    pub eyes_entity_num: i32,
    pub other_flags: u32,
    pub rendering_third_person: bool,
}

impl PlayerDrawGate {
    pub const SELF_ENTITY_MASK: u32 = other_flags::PLAYER | other_flags::DEAD_KILLCAM_TPV;

    pub fn is_player_view(self, number: u16) -> bool {
        (self.other_flags & Self::SELF_ENTITY_MASK) != 0
            && i32::from(number) == self.eyes_entity_num
    }

    pub fn skip_self_fpv(self, number: u16) -> bool {
        self.is_player_view(number) && !self.rendering_third_person
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteBodySubmitKind {
    NotInSnap,
    Corpse,
    Hidden,
    Dead,
    SelfFpv,
    Player,
}

impl RemoteBodySubmitKind {
    pub fn submits(self) -> bool {
        matches!(self, Self::Corpse | Self::Player)
    }

    pub fn dump_token(self) -> &'static str {
        match self {
            Self::NotInSnap => "not_in_snap",
            Self::Corpse => "corpse",
            Self::Hidden => "hidden",
            Self::Dead => "dead",
            Self::SelfFpv => "self_fpv",
            Self::Player => "ok",
        }
    }
}

pub fn remote_body_submit_kind(
    number: u16,
    runtime: &CEntityRuntime,
    gate: PlayerDrawGate,
) -> RemoteBodySubmitKind {
    if !runtime.in_next_snap() {
        return RemoteBodySubmitKind::NotInSnap;
    }
    let is_corpse = i32::from(number) >= PLAYER_CORPSE_ENTITY_BASE;
    if is_corpse {
        let corpse = runtime.pose_e_type == ET_PLAYER_CORPSE as u8
            || runtime.next_state.e_type == ET_PLAYER_CORPSE;
        return if corpse {
            RemoteBodySubmitKind::Corpse
        } else {
            RemoteBodySubmitKind::NotInSnap
        };
    }
    if runtime.next_state.e_flags & playerstate_iw4::eflags::NODRAW != 0 {
        return RemoteBodySubmitKind::Hidden;
    }
    if runtime.next_state.e_flags & EFLAGS_DEAD != 0 {
        return RemoteBodySubmitKind::Dead;
    }
    if gate.skip_self_fpv(number) {
        return RemoteBodySubmitKind::SelfFpv;
    }
    if runtime.pose_e_type == ET_PLAYER as u8 || runtime.next_state.e_type == ET_PLAYER {
        RemoteBodySubmitKind::Player
    } else {
        RemoteBodySubmitKind::NotInSnap
    }
}

pub fn remote_body_submits(number: u16, runtime: &CEntityRuntime, gate: PlayerDrawGate) -> bool {
    remote_body_submit_kind(number, runtime, gate).submits()
}

pub fn pos_trajectory(es: &EntityState) -> Trajectory {
    Trajectory {
        tr_time: es.tr_time,
        tr_type: es.tr_type,
        tr_duration: es.tr_duration,
        tr_delta: es.tr_delta,
        tr_base: es.tr_base,
    }
}

pub fn apos_trajectory(es: &EntityState) -> Trajectory {
    Trajectory {
        tr_time: es.apos_tr_time,
        tr_type: es.apos_tr_type,
        tr_duration: es.apos_tr_duration,
        tr_delta: es.apos_tr_delta,
        tr_base: es.apos_tr_base,
    }
}

pub fn lerp_from_entity_state(es: &EntityState) -> CurrentLerpState {
    CurrentLerpState {
        e_flags: es.e_flags,
        pos: pos_trajectory(es),
        apos: apos_trajectory(es),
    }
}

pub fn shutdown_entity(rt: &mut CEntityRuntime) {
    rt.pose_e_type = 0;
    rt.previous_pose = None;
    rt.current_valid &= !CURRENT_VALID_IN_NEXT;
    rt.fx_handle = CEntityFxHandle::None;
    rt.dobj_handle = CEntityDobjHandle::None;
}

pub fn reset_entity(rt: &mut CEntityRuntime, next: EntityState, at_time_ms: i32, new_entity: bool) {
    shutdown_entity(rt);
    rt.next_state = next;
    rt.current = lerp_from_entity_state(&next);
    rt.pose_e_type = u8::try_from(next.e_type).unwrap_or(0);
    sample_pose(rt, at_time_ms);
    rt.current_valid |= CURRENT_VALID_IN_NEXT;
    write_reset_cursor(rt, &next, new_entity);
}

fn sample_pose(rt: &mut CEntityRuntime, at_time_ms: i32) {
    rt.pose_time_ms = at_time_ms;
    rt.origin = evaluate_trajectory(&rt.current.pos, at_time_ms);
    rt.angles = evaluate_trajectory(&rt.current.apos, at_time_ms);
}

fn write_reset_cursor(rt: &mut CEntityRuntime, next: &EntityState, new_entity: bool) {
    match next.e_type {
        8 | 9 => {}
        0 | 4 => {
            if new_entity {
                rt.previous_event_sequence = 0;
            }
        }
        2 if next.e_flags & EFLAGS_CLONE_TREE != 0 => rt.previous_event_sequence = 0,
        _ => rt.previous_event_sequence = next.event_sequence,
    }
}

pub fn apply_existing(rt: &mut CEntityRuntime, next: EntityState, at_time_ms: i32) {
    let was_in = rt.in_next_snap();
    let e_type_changed = rt.next_state.e_type != next.e_type;
    let teleport = (rt.next_state.e_flags ^ next.e_flags) & EFLAGS_TELEPORT != 0;
    if !was_in || e_type_changed || teleport {
        let new_entity = !was_in || e_type_changed;
        reset_entity(rt, next, at_time_ms, new_entity);
        return;
    }
    if at_time_ms > rt.pose_time_ms {
        rt.previous_pose = Some((rt.current, rt.pose_time_ms));
    } else if at_time_ms < rt.pose_time_ms {
        rt.previous_pose = None;
    }
    rt.next_state = next;
    rt.current = lerp_from_entity_state(&next);
    rt.current_valid |= CURRENT_VALID_IN_NEXT;
    sample_pose(rt, at_time_ms);
}

pub fn player_state_to_entity_state(client: ClientId, ps: &PlayerState) -> EntityState {
    let mut es = EntityState::default();
    es.number = i32::try_from(client.0).unwrap_or(0);
    es.e_type = ET_PLAYER;
    es.client_num = es.number;
    es.e_flags = ps.e_flags;
    if ps.pm_type >= playerstate_iw4::PM_TYPE_DEAD {
        es.e_flags |= EFLAGS_DEAD;
    }
    es.tr_type = TR_INTERPOLATE;
    es.tr_base = ps.origin;
    es.apos_tr_type = TR_INTERPOLATE;
    es.apos_tr_base = ps.viewangles;
    es.ground_entity_num = ps.ground_entity_num;
    es.event_sequence = ps.event_sequence;
    es.events = [ps.events_0, ps.events_1, ps.events_2, ps.events_3];
    es.event_parms = [
        ps.event_parms_0,
        ps.event_parms_1,
        ps.event_parms_2,
        ps.event_parms_3,
    ];
    es.legs_anim = ps.legs_anim;
    es.torso_anim = ps.torso_anim;
    es.index = i32::try_from(ps.weapon).unwrap_or(0);
    es.weapon_model = weapon_iw4::weapon_model_for_held(&ps.weapons, &ps.weapon_data, ps.weapon);
    es
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RemotePoseSample {
    pub anim: AnimPair,

    pub rate_origin: [f32; 3],
    pub rate_time_ms: i32,
    pub weapon: u32,
    pub weapon_model: u8,
    pub view_pitch_deg: f32,

    pub prone: bool,

    pub crouch: bool,
}

pub fn remote_pose_sample(runtime: &CEntityRuntime) -> RemotePoseSample {
    let (state, rate_origin, rate_time_ms) = match runtime.presented_player.as_ref() {
        Some((state, time)) => (state, state.tr_base, *time),
        None => (
            &runtime.next_state,
            runtime.adopted_origin(),
            runtime.pose_time_ms,
        ),
    };
    RemotePoseSample {
        rate_origin,
        rate_time_ms,
        anim: AnimPair {
            legs_anim: state.legs_anim,
            torso_anim: state.torso_anim,
        },
        weapon: u32::try_from(state.index).unwrap_or(0),
        weapon_model: state.weapon_model,
        view_pitch_deg: runtime.angles[0],
        prone: state.e_flags & eflags::PRONE != 0,
        crouch: state.e_flags & eflags::DUCK != 0,
    }
}

pub fn corpse_slot_to_entity_state(slot: &PlayerCorpseSlot) -> EntityState {
    let mut es = EntityState::default();
    es.number = slot.entnum;
    es.e_type = ET_PLAYER_CORPSE;
    es.client_num = i32::try_from(slot.victim.0).unwrap_or(0);
    es.e_flags = slot.e_flags;
    es.tr_type = slot.tr_type;
    es.tr_time = slot.tr_time;
    es.tr_duration = slot.tr_duration;
    es.tr_delta = slot.tr_delta;
    es.tr_base = slot.tr_base;
    es.apos_tr_type = TR_INTERPOLATE;
    es.apos_tr_base = slot.viewangles;
    es.legs_anim = slot.anim.legs_anim;
    es.torso_anim = slot.anim.torso_anim;
    es.index = i32::try_from(slot.weapon).unwrap_or(0);

    es.ground_entity_num = slot.ground_entity_num;
    es
}
