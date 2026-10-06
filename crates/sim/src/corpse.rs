use crate::frame::FrameWorld;
use crate::world::ClientId;
use entity_iw4::{TR_GRAVITY, TR_INTERPOLATE, Trajectory, evaluate_trajectory};
use playerstate_iw4::{
    AnimPair, ENTITYNUM_NONE, LINK_FLAGS_WEAPON_VIEW_ONLY, MAX_CLIENT_CORPSES,
    PLAYER_CORPSE_ENTITY_BASE, PlayerState,
};

use crate::bullet_collision::{MASK_PLAYER_SOLID, PLAYER_MAXS, PLAYER_MINS};

pub const G_CLONE_PLAYER_MAX_VELOCITY: f32 = 80.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerCorpseSlot {
    pub occupied: bool,

    pub entnum: i32,
    pub victim: ClientId,
    pub origin: [f32; 3],
    pub viewangles: [f32; 3],

    pub anim: AnimPair,
    pub view_height_current: f32,
    pub weapon: u32,
    pub e_flags: u32,

    pub tr_type: i32,

    pub tr_time: i32,
    pub tr_duration: i32,
    pub tr_delta: [f32; 3],
    pub tr_base: [f32; 3],

    pub falling: bool,

    pub ground_entity_num: i32,
}

impl Default for PlayerCorpseSlot {
    fn default() -> Self {
        Self {
            occupied: false,
            entnum: -1,
            victim: ClientId(0),
            origin: [0.0; 3],
            viewangles: [0.0; 3],
            anim: AnimPair::default(),
            view_height_current: 0.0,
            weapon: 0,
            e_flags: 0,
            tr_type: 0,
            tr_time: 0,
            tr_duration: 0,
            tr_delta: [0.0; 3],
            tr_base: [0.0; 3],
            falling: false,
            ground_entity_num: ENTITYNUM_NONE,
        }
    }
}

impl PlayerCorpseSlot {
    pub fn trajectory(&self) -> Trajectory {
        Trajectory {
            tr_time: self.tr_time,
            tr_type: self.tr_type,
            tr_duration: self.tr_duration,
            tr_delta: self.tr_delta,
            tr_base: self.tr_base,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerCorpsePool {
    pub slots: [PlayerCorpseSlot; MAX_CLIENT_CORPSES as usize],

    pub spawn_ring: u8,
}

impl Default for PlayerCorpsePool {
    fn default() -> Self {
        Self {
            slots: [PlayerCorpseSlot::default(); MAX_CLIENT_CORPSES as usize],
            spawn_ring: 0,
        }
    }
}

impl PlayerCorpsePool {
    pub fn alloc_clone_entity(&mut self) -> i32 {
        let entnum = PLAYER_CORPSE_ENTITY_BASE + i32::from(self.spawn_ring);
        self.spawn_ring = (self.spawn_ring + 1) % MAX_CLIENT_CORPSES as u8;
        self.clear_entnum(entnum);
        entnum
    }

    pub fn clear_entnum(&mut self, entnum: i32) {
        for slot in &mut self.slots {
            if slot.entnum == entnum {
                *slot = PlayerCorpseSlot::default();
            }
        }
    }

    pub fn get_free(&mut self, ref_origin: [f32; 3]) -> u8 {
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.entnum == -1 {
                return i as u8;
            }
        }
        let mut best = 0u8;
        let mut best_dist = -1.0f32;
        for (i, slot) in self.slots.iter().enumerate() {
            let dx = ref_origin[0] - slot.origin[0];
            let dy = ref_origin[1] - slot.origin[1];
            let dz = ref_origin[2] - slot.origin[2];
            let dist = dx * dx + dy * dy + dz * dz;
            if dist > best_dist {
                best_dist = dist;
                best = i as u8;
            }
        }
        let entnum = self.slots[best as usize].entnum;
        self.clear_entnum(entnum);
        best
    }

    pub fn entnums(&self) -> [i32; MAX_CLIENT_CORPSES as usize] {
        let mut nums = [-1; MAX_CLIENT_CORPSES as usize];
        for (i, slot) in self.slots.iter().enumerate() {
            nums[i] = slot.entnum;
        }
        nums
    }

    pub fn occupy(&mut self, slot: u8, entnum: i32, mut body: PlayerCorpseSlot) -> u8 {
        let i = (slot as usize) % MAX_CLIENT_CORPSES as usize;
        body.occupied = true;
        body.entnum = entnum;
        self.slots[i] = body;
        i as u8
    }
}

pub fn level_time_ms(tick: crate::world::Tick) -> i32 {
    i32::try_from(tick.0)
        .unwrap_or(0)
        .saturating_mul(crate::MATCH_TICK_MS as i32)
}

pub(crate) fn occupy_player_clone(
    world: &mut FrameWorld,
    victim: ClientId,
    ps: &PlayerState,
    time_ms: i32,
) -> u8 {
    let pool = world.corpses_mut();
    let entnum = pool.alloc_clone_entity();
    let slot = pool.get_free(ps.origin);
    let slot = pool.occupy(
        slot,
        entnum,
        PlayerCorpseSlot {
            victim,
            origin: ps.origin,

            viewangles: [0.0, client_think_entity_yaw(ps), 0.0],
            anim: ps.anim(),
            view_height_current: ps.view_height_current,
            weapon: ps.weapon,
            e_flags: ps.e_flags,
            tr_type: TR_GRAVITY,
            tr_time: time_ms,
            tr_duration: 0,
            tr_delta: clamp_clone_tr_delta(ps.velocity),
            tr_base: ps.origin,
            falling: true,
            ..PlayerCorpseSlot::default()
        },
    );
    world.corpse_dobj_tree_install(entnum, ps.anim().legs_anim);
    slot
}

fn client_think_entity_yaw(ps: &PlayerState) -> f32 {
    if (ps.link_flags & LINK_FLAGS_WEAPON_VIEW_ONLY) != 0 {
        ps.link_weapon_angles[1]
    } else {
        ps.viewangles[1]
    }
}

fn corpse_land_angles(yaw: f32, normal: [f32; 3]) -> [f32; 3] {
    let (fwd, _, _) = math_iw4::angle_vectors([0.0, yaw, 0.0]);
    let axis0 = [fwd[0], fwd[1], fwd[2]];
    let axis2 = normal;
    let axis1 = [
        axis2[1] * axis0[2] - axis2[2] * axis0[1],
        axis2[2] * axis0[0] - axis2[0] * axis0[2],
        axis2[0] * axis0[1] - axis2[1] * axis0[0],
    ];
    let axis0 = [
        axis1[1] * axis2[2] - axis1[2] * axis2[1],
        axis1[2] * axis2[0] - axis1[0] * axis2[2],
        axis1[0] * axis2[1] - axis1[1] * axis2[0],
    ];
    math_iw4::axis_to_angles([axis0, axis1, axis2])
}

fn clamp_clone_tr_delta(mut delta: [f32; 3]) -> [f32; 3] {
    if delta[0] > G_CLONE_PLAYER_MAX_VELOCITY {
        delta[0] = G_CLONE_PLAYER_MAX_VELOCITY;
    }
    if delta[1] > G_CLONE_PLAYER_MAX_VELOCITY {
        delta[1] = G_CLONE_PLAYER_MAX_VELOCITY;
    }
    delta
}

const CORPSE_ANIM_DELTA_LEN_SQ_MIN: f32 = 1.0;

const CORPSE_ANIM_AXIS_Y_SCALE: f32 = -1.0;

const CORPSE_GROUND_PROBE_DROP: f32 = 1.0;

const CORPSE_ANIM_DELTA_TO_VELOCITY: f32 = 20.0;

const CORPSE_UNSTICK_Z_LIFT: f32 = 32.0;

const CORPSE_UNSTICK_CLIPMASK_DROP: u32 = 0x0001_0000;

const TR_RAGDOLL_SETTLED: i32 = 12;

const TR_BOUNCE_GRAVITY: i32 = 0xb;

fn is_ragdoll_tr_type(tr_type: i32) -> bool {
    (10..=12).contains(&tr_type)
}

pub(crate) fn phase_run_corpse_move(world: &mut FrameWorld, time_ms: i32) {
    let n = world.corpses().slots.len();
    for i in 0..n {
        let slot = world.corpses().slots[i];
        if !slot.occupied {
            continue;
        }
        let ragdoll = is_ragdoll_tr_type(slot.tr_type);

        let anim_delta = world
            .corpse_dobj_tree_delta(slot.entnum, crate::MATCH_TICK_MS as i32)
            .filter(|d| {
                d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > CORPSE_ANIM_DELTA_LEN_SQ_MIN
                    && (!slot.falling || !ragdoll)
            });
        if !slot.falling && anim_delta.is_none() {
            continue;
        }

        let mut desired = evaluate_trajectory(&slot.trajectory(), time_ms);
        let world_delta = anim_delta.map(|delta| corpse_anim_world_delta(slot.viewangles, delta));
        if let Some(world_delta) = world_delta {
            for axis in 0..3 {
                desired[axis] += world_delta[axis];
            }
        }

        let hit = world.trace_clip(
            slot.origin,
            desired,
            PLAYER_MINS,
            PLAYER_MAXS,
            MASK_PLAYER_SOLID,
        );
        let mut fraction = hit.fraction;
        let endpos = if fraction >= 1.0 { desired } else { hit.endpos };
        world.corpses_mut().slots[i].origin = endpos;
        if hit.startsolid != 0 {
            fraction = 0.0;
        }

        if fraction >= 1.0 {
            let Some(world_delta) = world_delta else {
                continue;
            };

            {
                let slot = &mut world.corpses_mut().slots[i];
                slot.tr_type = if ragdoll {
                    TR_RAGDOLL_SETTLED
                } else {
                    TR_INTERPOLATE
                };
                slot.tr_base = endpos;
                slot.tr_time = 0;
                slot.tr_duration = 0;
                slot.tr_delta = [0.0; 3];
            }
            if ragdoll {
                let mut probe = desired;
                probe[2] -= CORPSE_GROUND_PROBE_DROP;
                let ground =
                    world.trace_clip(endpos, probe, PLAYER_MINS, PLAYER_MAXS, MASK_PLAYER_SOLID);
                if ground.fraction < 1.0 || ground.startsolid != 0 {
                    let slot = &mut world.corpses_mut().slots[i];
                    slot.falling = true;
                    let mut vel = slot.tr_delta;
                    for axis in 0..3 {
                        vel[axis] = (vel[axis] + world_delta[axis]) * CORPSE_ANIM_DELTA_TO_VELOCITY;
                    }
                    slot.tr_delta = vel;
                    slot.tr_type = TR_BOUNCE_GRAVITY;
                    slot.tr_time = time_ms;
                    slot.tr_duration = 0;
                    continue;
                }
            }
            world.corpses_mut().slots[i].falling = false;
            continue;
        }

        if !slot.falling {
            continue;
        }

        if hit.allsolid != 0 {
            let mut lifted = world.corpses().slots[i].origin;
            lifted[2] += CORPSE_UNSTICK_Z_LIFT;
            let retry = world.trace_clip(
                lifted,
                desired,
                PLAYER_MINS,
                PLAYER_MAXS,
                MASK_PLAYER_SOLID & !CORPSE_UNSTICK_CLIPMASK_DROP,
            );
            if retry.allsolid == 0 {
                let f = retry.fraction;
                world.corpses_mut().slots[i].origin = [
                    lifted[0] + (desired[0] - lifted[0]) * f,
                    lifted[1] + (desired[1] - lifted[1]) * f,
                    lifted[2] + (desired[2] - lifted[2]) * f,
                ];
            }
        }
        corpse_land(world, i, hit, time_ms);
    }
    let live = world.corpses().entnums();
    world.corpse_dobj_tree_retain(&live);
}

fn corpse_anim_world_delta(angles: [f32; 3], delta: [f32; 3]) -> [f32; 3] {
    let (mut fwd, right, up) = math_iw4::angle_vectors(angles);
    let mut left = [
        right[0] * CORPSE_ANIM_AXIS_Y_SCALE,
        right[1] * CORPSE_ANIM_AXIS_Y_SCALE,
        right[2] * CORPSE_ANIM_AXIS_Y_SCALE,
    ];
    normalize(&mut fwd);
    normalize(&mut left);
    let mut out = [0.0f32; 3];
    for (axis, slot) in out.iter_mut().enumerate() {
        *slot = delta[0] * fwd[axis] + delta[1] * left[axis] + delta[2] * up[axis];
    }
    out
}

fn corpse_land(world: &mut FrameWorld, i: usize, hit: trace_iw4::Trace, time_ms: i32) {
    let slot = &mut world.corpses_mut().slots[i];
    let ragdoll = is_ragdoll_tr_type(slot.tr_type);
    slot.tr_delta = [0.0; 3];
    if hit.allsolid == 0 && hit.normal[2] <= 0.0 {
        for axis in 0..3 {
            slot.origin[axis] += hit.normal[axis];
        }
        slot.tr_base = slot.origin;
        slot.tr_time = time_ms;
        return;
    }
    slot.falling = false;
    slot.tr_type = if ragdoll {
        TR_RAGDOLL_SETTLED
    } else {
        TR_INTERPOLATE
    };
    slot.tr_base = slot.origin;
    slot.tr_time = 0;
    slot.tr_duration = 0;

    slot.ground_entity_num =
        i32::from(trace_iw4::trace_get_entity_hit_id(hit.hit_type, hit.hit_id));
    if hit.allsolid == 0 {
        slot.viewangles = corpse_land_angles(slot.viewangles[1], hit.normal);
    }
}

fn normalize(v: &mut [f32; 3]) {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let len = if len <= 0.0 { 1.0 } else { len };
    v[0] /= len;
    v[1] /= len;
    v[2] /= len;
}

pub(crate) fn phase_sync_corpse_info(world: &mut FrameWorld) {
    let n = world.corpses().slots.len();
    for i in 0..n {
        if world.corpses().slots[i].occupied {
            sync_corpse_info_player_anims(world, i);
        }
    }
}

pub(crate) fn sync_corpse_info_player_anims(world: &mut FrameWorld, slot: usize) {
    let body = world.corpses().slots[slot];
    let src = entity_iw4::CorpseInfoPlayerAnimCopy {
        legs_anim: body.anim.legs_anim,
        torso_anim: body.anim.torso_anim,
        torso_pitch: 0,
        waist_pitch: 0,
    };
    let Some(copied) = entity_iw4::corpse_info::copy_player_anims(false, src) else {
        return;
    };
    let slot = &mut world.corpses_mut().slots[slot];
    slot.anim.legs_anim = copied.legs_anim;
    slot.anim.torso_anim = copied.torso_anim;
}
