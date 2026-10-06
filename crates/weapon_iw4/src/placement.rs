use core::f32::consts::{FRAC_PI_2, PI, TAU};

use math_iw4::{angle_vectors, get_lean_fraction};
use playerstate_iw4::{eflags, pm_flags};

use crate::kick::{
    GunRecoilPlacementState, GunRecoilResponse, calculate_weapon_position_gun_recoil,
    gun_recoil_angle_contribution,
};
use crate::sway::{SwaySpringState, sway_contribution};
use crate::weaponstate::WeaponState;

pub const VIEWHEIGHT_TARGET_PRONE: i32 = 0x0b;

pub const VIEWHEIGHT_TARGET_CROUCH: i32 = 0x28;

pub const WEAPON_BOB_AMPLITUDE_BASE: f32 = 0.16;

pub const WEAPON_BOB_MAX: f32 = 8.0;

pub const WEAPON_BOB_LAG: f32 = 0.25;

pub const WEAPON_BOB_AMPLITUDE_ROLL: f32 = 1.5;

pub const WEAPON_BOB_UP_PHASE: f32 = 0.471_238_94;

pub const WEAPON_BOB_AMP_STANDING: [f32; 2] = [0.055, 0.025];

pub const WEAPON_BOB_AMP_DUCKED: [f32; 2] = [0.045, 0.025];

pub const WEAPON_BOB_AMP_PRONE: [f32; 2] = [0.02, 0.005];

pub const WEAPON_BOB_AMP_SPRINTING: [f32; 2] = [0.02, 0.014];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponBobState {
    pub pitch: f32,

    pub yaw: f32,

    pub roll: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponStanceStaticOfsInputs {
    pub ducked_ofs: [f32; 3],

    pub prone_ofs: [f32; 3],

    pub ads_aim_pitch: f32,

    pub night_vision_wear_time: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponMovementOfsInputs {
    pub stand_move: [f32; 3],

    pub stand_rot: [f32; 3],

    pub strafe_move: [f32; 3],

    pub strafe_rot: [f32; 3],

    pub ducked_move: [f32; 3],

    pub ducked_rot: [f32; 3],

    pub prone_move: [f32; 3],

    pub prone_rot: [f32; 3],

    pub pos_move_rate: f32,

    pub pos_prone_move_rate: f32,

    pub stand_move_min_speed: f32,

    pub ducked_move_min_speed: f32,

    pub prone_move_min_speed: f32,

    pub pos_rot_rate: f32,

    pub pos_prone_rot_rate: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponMovementKinematics {
    pub xyspeed: f32,

    pub speed: f32,

    pub velocity: [f32; 3],

    pub viewangles: [f32; 3],

    pub weaponstate: i32,

    pub weaponstate_secondary: i32,

    pub pm_flags: u32,

    pub frametime: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponBobInputs {
    pub ads_bob_factor: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponIdleInputs {
    pub ads_idle_amount: f32,

    pub hip_idle_amount: f32,

    pub ads_idle_speed: f32,

    pub hip_idle_speed: f32,

    pub idle_crouch_factor: f32,

    pub idle_prone_factor: f32,
}

pub const WEAPON_IDLE_AMOUNT_DEFAULT: f32 = 80.0;

pub const WEAPON_IDLE_TIME_MS_SCALE: f32 = 1000.0;

pub const WEAPON_IDLE_FACTOR_LERP: f32 = 0.5;

pub const WEAPON_IDLE_ROLL_FREQ: f32 = 0.0005;

pub const WEAPON_IDLE_YAW_FREQ: f32 = 0.0007;

pub const WEAPON_IDLE_PITCH_FREQ: f32 = 0.001;

pub const WEAPON_IDLE_SIN_SCALE: f32 = 0.01;

pub const GUN_DAMAGE_DEFLECT_MS: f32 = 100.0;

pub const GUN_DAMAGE_RETURN_MS: f32 = 400.0;

pub const GUN_DAMAGE_ADS_HALF: f32 = 0.5;

pub const GUN_DAMAGE_OVERLAY_MIX: f32 = 0.75;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponBobWaveformInputs {
    pub bob_cycle: u8,

    pub xyspeed: f32,

    pub view_height_target: i32,

    pub pm_flags: u32,

    pub weapon_pos_frac: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StanceTransitionFadeGlobals {
    pub fade_start_frac: f32,

    pub fade_end_frac: f32,
}

impl Default for StanceTransitionFadeGlobals {
    fn default() -> Self {
        Self {
            fade_start_frac: 0.0,
            fade_end_frac: 1.0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WeaponPlacementState {
    pub movement_angles: [f32; 3],

    pub movement_origin: [f32; 3],

    pub unfaded_angles: [f32; 3],
    pub sway_springs: SwaySpringState,
    pub gun_recoil: GunRecoilPlacementState,
    pub bob: WeaponBobState,

    pub weap_idle_time: i32,

    pub last_idle_factor: f32,

    pub idle_sway_angles: [f32; 3],

    pub damage_kick_time: i32,

    pub damage_time: i32,

    pub v_dmg_pitch: f32,
    pub v_dmg_roll: f32,

    pub damage_kick_angles: [f32; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponPlacementPsInputs {
    pub e_flags: u32,

    pub weapon_pos_frac: f32,

    pub weapon_time: i32,

    pub aim_down_sight: bool,

    pub overlay_reticle: i32,

    pub weapon_transition_active: bool,

    pub lean_fraction: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponPlacementContribution {
    pub origin: [f32; 3],

    pub angles: [f32; 3],
}

pub const PLACEMENT_ASSEMBLE_STEP_COUNT: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponPlacementAssembleStep {
    BaseStanceMovementAngles,
    Sway,
    GunRecoil,
    Bob,
    ApplyAngles,
    ApplyOrigin,
}

#[inline]
pub fn stance_transition_fade(
    weapon_transition_active: bool,
    weapon_time: i32,
    wear_time: i32,
    globals: StanceTransitionFadeGlobals,
) -> f32 {
    if !weapon_transition_active || wear_time <= 0 {
        return 1.0;
    }
    let t = weapon_time as f32 / wear_time as f32;
    if t <= globals.fade_start_frac {
        1.0
    } else if t >= globals.fade_end_frac {
        0.0
    } else {
        let denom = globals.fade_end_frac - globals.fade_start_frac;
        if denom <= 0.0 {
            0.0
        } else {
            1.0 - (t - globals.fade_start_frac) / denom
        }
    }
}

pub fn weapon_stance_static_ofs(
    ps: WeaponPlacementPsInputs,
    stance: WeaponStanceStaticOfsInputs,
    fade_globals: StanceTransitionFadeGlobals,
    pitch_out: &mut f32,
    origin_out: &mut [f32; 3],
) {
    let fade = stance_transition_fade(
        ps.weapon_transition_active,
        ps.weapon_time,
        stance.night_vision_wear_time,
        fade_globals,
    );

    let duck = ps.e_flags & eflags::DUCK != 0;
    let prone = ps.e_flags & eflags::PRONE != 0;

    if duck {
        origin_out[0] += fade * stance.ducked_ofs[0];
        origin_out[1] += fade * stance.ducked_ofs[1];
        origin_out[2] += fade * stance.ducked_ofs[2];
    } else if prone {
        origin_out[0] += fade * stance.prone_ofs[0];
        origin_out[1] += fade * stance.prone_ofs[1];
        origin_out[2] += fade * stance.prone_ofs[2];
    }

    if ps.aim_down_sight {
        *pitch_out += stance.ads_aim_pitch * ps.weapon_pos_frac;
    }
}

#[inline]
pub fn weapon_bob_ads_attenuation(weapon_pos_frac: f32, ads_bob_factor: f32) -> f32 {
    if weapon_pos_frac <= 0.0 {
        return 1.0;
    }
    1.0 - weapon_pos_frac * (1.0 - ads_bob_factor)
}

#[inline]
pub fn weapon_bob_apply_ads_attenuation(
    bob: &mut WeaponBobState,
    weapon_pos_frac: f32,
    ads_bob_factor: f32,
) {
    let scale = weapon_bob_ads_attenuation(weapon_pos_frac, ads_bob_factor);
    bob.pitch *= scale;
    bob.yaw *= scale;
    bob.roll *= scale;
}

#[inline]
pub fn weapon_bob_set_waveform(state: &mut WeaponPlacementState, waveform: WeaponBobState) {
    state.bob = waveform;
}

pub fn calculate_weapon_movement_bob(
    state: &mut WeaponPlacementState,
    ps: WeaponPlacementPsInputs,
    bob_inputs: WeaponBobInputs,
    waveform: WeaponBobState,
) {
    state.bob = waveform;
    if ps.weapon_pos_frac > 0.0 {
        weapon_bob_apply_ads_attenuation(
            &mut state.bob,
            ps.weapon_pos_frac,
            bob_inputs.ads_bob_factor,
        );
    }
}

#[inline]
fn weapon_bob_sinf(x: f32) -> f32 {
    libm::sinf(x)
}

fn weapon_bob_helper_amplitude(
    view_height_target: i32,
    pm_flags: u32,
    weapon_pos_frac: f32,
    speed: f32,
    horizontal: bool,
) -> f32 {
    let idx = if horizontal { 0 } else { 1 };
    let ads_inner = 1.0 - weapon_pos_frac;
    let scale = if view_height_target == VIEWHEIGHT_TARGET_PRONE {
        WEAPON_BOB_AMP_PRONE[idx]
    } else if view_height_target == VIEWHEIGHT_TARGET_CROUCH {
        ads_inner * WEAPON_BOB_AMP_DUCKED[idx]
    } else if (pm_flags & pm_flags::SPRINTING) == 0 {
        ads_inner * WEAPON_BOB_AMP_STANDING[idx]
    } else {
        WEAPON_BOB_AMP_SPRINTING[idx]
    };
    let mut amp = speed * scale;
    if amp > WEAPON_BOB_MAX {
        amp = WEAPON_BOB_MAX;
    }
    amp
}

fn calc_weapon_bob_vertical(
    cycle: f32,
    speed: f32,
    view_height_target: i32,
    pm_flags: u32,
    weapon_pos_frac: f32,
) -> f32 {
    let amp =
        weapon_bob_helper_amplitude(view_height_target, pm_flags, weapon_pos_frac, speed, false);
    (weapon_bob_sinf(cycle * 4.0 + FRAC_PI_2) * 0.2 + weapon_bob_sinf(cycle + cycle)) * 0.75 * amp
}

fn calc_weapon_bob_horizontal(
    cycle: f32,
    speed: f32,
    view_height_target: i32,
    pm_flags: u32,
    weapon_pos_frac: f32,
) -> f32 {
    let amp =
        weapon_bob_helper_amplitude(view_height_target, pm_flags, weapon_pos_frac, speed, true);
    weapon_bob_sinf(cycle) * amp
}

pub fn calculate_weapon_movement_bob_waveform(inputs: WeaponBobWaveformInputs) -> WeaponBobState {
    let bob = (f32::from(inputs.bob_cycle) / 255.0) * PI;
    let cycle = TAU + bob + bob + TAU + WEAPON_BOB_LAG * PI;
    let speed = WEAPON_BOB_AMPLITUDE_BASE * inputs.xyspeed;
    let pitch = calc_weapon_bob_vertical(
        cycle,
        speed,
        inputs.view_height_target,
        inputs.pm_flags,
        inputs.weapon_pos_frac,
    ) * -1.0;
    let yaw = calc_weapon_bob_horizontal(
        cycle,
        speed,
        inputs.view_height_target,
        inputs.pm_flags,
        inputs.weapon_pos_frac,
    ) * -1.0;
    let mut roll = calc_weapon_bob_horizontal(
        cycle - WEAPON_BOB_UP_PHASE,
        WEAPON_BOB_AMPLITUDE_ROLL * speed,
        inputs.view_height_target,
        inputs.pm_flags,
        inputs.weapon_pos_frac,
    );

    if (roll < 0.0) == (roll == 0.0) {
        roll = 0.0;
    }
    WeaponBobState { pitch, yaw, roll }
}

#[inline]
pub fn weapon_bob_add_to_angles(
    angles: &mut [f32; 3],
    bob: WeaponBobState,
    weapon_pos_frac: f32,
    overlay_reticle: i32,
) {
    let scale = if overlay_reticle != 0 {
        1.0 - weapon_pos_frac
    } else {
        1.0
    };
    angles[0] += bob.pitch * scale;
    angles[1] += bob.yaw * scale;
    angles[2] += bob.roll * scale;
}

#[inline]
pub fn weapon_bob_rotate_origin(origin: &mut [f32; 3], bob: WeaponBobState) {
    let (forward, right, up) = angle_vectors([bob.pitch, bob.yaw, bob.roll]);
    let axis = [forward, [-right[0], -right[1], -right[2]], up];
    let src = *origin;
    for (j, out) in origin.iter_mut().enumerate() {
        *out = src[0] * axis[0][j] + src[1] * axis[1][j] + src[2] * axis[2][j];
    }
}

#[inline]
pub fn placement_movement_channel_scale(weapon_pos_frac: f32) -> f32 {
    let s = 1.0 - weapon_pos_frac - weapon_pos_frac;
    if s > 0.0 { s } else { 0.0 }
}

pub fn weapon_placement_apply_origin(
    state: &WeaponPlacementState,
    ps: WeaponPlacementPsInputs,
) -> [f32; 3] {
    let mut out = [0.0_f32; 3];
    let move_scale = placement_movement_channel_scale(ps.weapon_pos_frac);
    if move_scale > 0.0 {
        out[0] += move_scale * state.movement_origin[0];
        out[1] += move_scale * state.movement_origin[1];
        out[2] += move_scale * state.movement_origin[2];
    }
    let sway = sway_contribution(state.sway_springs);
    out[1] -= sway.origin[1];
    out[2] += sway.origin[2];
    if ps.lean_fraction != 0.0 {
        panic!("viewmodel lean origin is unsupported");
    }
    weapon_bob_rotate_origin(&mut out, state.bob);
    out
}

pub fn weapon_idle_amount_speed(ps: WeaponPlacementPsInputs, idle: WeaponIdleInputs) -> (f32, f32) {
    if ps.aim_down_sight {
        let amount = (idle.ads_idle_amount - idle.hip_idle_amount) * ps.weapon_pos_frac
            + idle.hip_idle_amount;
        let speed =
            (idle.ads_idle_speed - idle.hip_idle_speed) * ps.weapon_pos_frac + idle.hip_idle_speed;
        return (amount, speed);
    }
    if idle.hip_idle_amount != 0.0 {
        return (idle.hip_idle_amount, idle.hip_idle_speed);
    }
    (WEAPON_IDLE_AMOUNT_DEFAULT, 1.0)
}

pub fn apply_idle_sway_scale(
    state: &mut WeaponPlacementState,
    ps: WeaponPlacementPsInputs,
    idle: WeaponIdleInputs,
    frametime: f32,
    angles: &mut [f32; 3],
) {
    let (amount, speed) = weapon_idle_amount_speed(ps, idle);
    let add = libm::roundf(speed * WEAPON_IDLE_TIME_MS_SCALE * frametime) as i32;
    state.weap_idle_time = state.weap_idle_time.wrapping_add(add);

    let target = if ps.e_flags & eflags::PRONE != 0 {
        idle.idle_prone_factor
    } else if ps.e_flags & eflags::DUCK != 0 {
        idle.idle_crouch_factor
    } else {
        1.0
    };
    let last = state.last_idle_factor;
    if last != target {
        let step = frametime * WEAPON_IDLE_FACTOR_LERP;
        state.last_idle_factor = if target <= last {
            let next = last - step;
            if next < target { target } else { next }
        } else {
            let next = last + step;
            if next > target { target } else { next }
        };
    }

    let mut scaled = state.last_idle_factor * amount;
    if ps.overlay_reticle != 0 {
        scaled *= 1.0 - ps.weapon_pos_frac;
    }
    let t = state.weap_idle_time as f32;
    let s = scaled * WEAPON_IDLE_SIN_SCALE;
    let add_p = libm::sinf(t * WEAPON_IDLE_PITCH_FREQ) * s;
    let add_y = libm::sinf(t * WEAPON_IDLE_YAW_FREQ) * s;
    let add_r = libm::sinf(t * WEAPON_IDLE_ROLL_FREQ) * s;
    state.idle_sway_angles = [add_p, add_y, add_r];
    angles[0] += add_p;
    angles[1] += add_y;
    angles[2] += add_r;
}

pub fn weapon_damage_kick_angles(
    time: i32,
    damage_time: i32,
    v_dmg_pitch: f32,
    v_dmg_roll: f32,
    weapon_pos_frac: f32,
    overlay_reticle: i32,
) -> [f32; 3] {
    if damage_time == 0 {
        return [0.0; 3];
    }
    let mut factor = GUN_DAMAGE_ADS_HALF + weapon_pos_frac * GUN_DAMAGE_ADS_HALF;
    let deflect = GUN_DAMAGE_DEFLECT_MS * factor;
    let ret = GUN_DAMAGE_RETURN_MS * factor;
    if weapon_pos_frac != 0.0 && overlay_reticle != 0 {
        factor *= 1.0 - weapon_pos_frac * GUN_DAMAGE_OVERLAY_MIX;
    }
    let delta = (time - damage_time) as f32;
    let scale = if deflect <= delta {
        let f = 1.0 - (delta - deflect) / ret;
        if f <= 0.0 {
            return [0.0; 3];
        }
        (1.0 - get_lean_fraction(1.0 - f)) * factor
    } else {
        get_lean_fraction(delta / deflect) * factor
    };
    [
        scale * v_dmg_pitch * GUN_DAMAGE_ADS_HALF,
        -(v_dmg_roll * scale),
        scale * v_dmg_roll * GUN_DAMAGE_ADS_HALF,
    ]
}

pub fn weapon_placement_apply_angles(
    state: &WeaponPlacementState,
    ps: WeaponPlacementPsInputs,
) -> [f32; 3] {
    let mut out = [0.0_f32; 3];
    let move_scale = placement_movement_channel_scale(ps.weapon_pos_frac);
    if move_scale > 0.0 {
        out[0] += move_scale * state.movement_angles[0];
        out[1] += move_scale * state.movement_angles[1];
        out[2] += move_scale * state.movement_angles[2];
    }
    let sway = sway_contribution(state.sway_springs);
    out[0] += sway.angles[0];
    out[1] += sway.angles[1];
    if ps.lean_fraction != 0.0 {
        panic!("viewmodel lean roll is unsupported");
    }
    out[0] += state.unfaded_angles[0];
    out[1] += state.unfaded_angles[1];
    out[2] += state.unfaded_angles[2];
    weapon_bob_add_to_angles(&mut out, state.bob, ps.weapon_pos_frac, ps.overlay_reticle);
    let recoil = gun_recoil_angle_contribution(state.gun_recoil);
    out[0] += recoil[0];
    out[1] += recoil[1];
    out
}

#[inline]
fn mad3(out: &mut [f32; 3], v: [f32; 3], scale: f32) {
    out[0] += v[0] * scale;
    out[1] += v[1] * scale;
    out[2] += v[2] * scale;
}

#[inline]
fn lerp3(current: [f32; 3], target: [f32; 3], t: f32) -> [f32; 3] {
    [
        current[0] + (target[0] - current[0]) * t,
        current[1] + (target[1] - current[1]) * t,
        current[2] + (target[2] - current[2]) * t,
    ]
}

#[inline]
fn clamp_unit(t: f32) -> f32 {
    if t > 1.0 { 1.0 } else { t }
}

pub fn calculate_weapon_movement_targets(
    ps: WeaponPlacementPsInputs,
    kinematics: WeaponMovementKinematics,
    movement: WeaponMovementOfsInputs,
    min_speed: f32,
    origin_out: &mut [f32; 3],
    angles_out: &mut [f32; 3],
) {
    let duck = ps.e_flags & eflags::DUCK != 0;
    let prone = ps.e_flags & eflags::PRONE != 0;
    let (mv, rot) = if prone {
        (movement.prone_move, movement.prone_rot)
    } else if duck {
        (movement.ducked_move, movement.ducked_rot)
    } else {
        (movement.stand_move, movement.stand_rot)
    };
    let mut speed_frac = (kinematics.xyspeed - min_speed) / (kinematics.speed - min_speed);
    if speed_frac >= 1.0 {
        speed_frac = 1.0;
    }
    let ads_scale = 1.0 - ps.weapon_pos_frac;
    mad3(origin_out, mv, speed_frac);
    mad3(angles_out, rot, speed_frac * ads_scale);

    let (_, right, _) = angle_vectors(kinematics.viewangles);
    let along = (right[0] * kinematics.velocity[0] + right[1] * kinematics.velocity[1])
        * speed_frac
        / kinematics.xyspeed;
    mad3(origin_out, movement.strafe_move, along);
    mad3(angles_out, movement.strafe_rot, along * ads_scale);
}

pub fn stance_movement_lerp(
    state: &mut WeaponPlacementState,
    origin_target: [f32; 3],
    angles_target: [f32; 3],
    movement: WeaponMovementOfsInputs,
    kinematics: WeaponMovementKinematics,
    prone: bool,
) {
    let pos_rate = if prone {
        movement.pos_prone_move_rate
    } else {
        movement.pos_move_rate
    };
    let rot_rate = if prone {
        movement.pos_prone_rot_rate
    } else {
        movement.pos_rot_rate
    };
    let pos_t = clamp_unit(pos_rate * kinematics.frametime);
    let rot_t = clamp_unit(rot_rate * kinematics.frametime);
    state.movement_origin = lerp3(state.movement_origin, origin_target, pos_t);
    state.movement_angles = lerp3(state.movement_angles, angles_target, rot_t);
}

pub fn base_stance_movement_angles(
    state: &mut WeaponPlacementState,
    ps: WeaponPlacementPsInputs,
    stance: WeaponStanceStaticOfsInputs,
    fade_globals: StanceTransitionFadeGlobals,
    movement: WeaponMovementOfsInputs,
    kinematics: WeaponMovementKinematics,
) {
    let duck = ps.e_flags & eflags::DUCK != 0;
    let prone = ps.e_flags & eflags::PRONE != 0;
    let min_speed = if prone {
        movement.prone_move_min_speed
    } else if duck {
        movement.ducked_move_min_speed
    } else {
        movement.stand_move_min_speed
    };
    let moving = kinematics.xyspeed > min_speed && kinematics.speed > min_speed;
    let reloading = kinematics.weaponstate == WeaponState::Reloading as i32
        || kinematics.weaponstate_secondary == WeaponState::Reloading as i32;
    let night_vision = kinematics.weaponstate == WeaponState::NightVisionWear as i32
        || kinematics.weaponstate == WeaponState::NightVisionRemove as i32;
    let ladder = (kinematics.pm_flags & pm_flags::LADDER) != 0;

    let mut origin_target = [0.0_f32; 3];
    let mut angles_target = [0.0_f32; 3];

    if (duck || prone) && (!moving || !night_vision || !prone) {
        let mut pitch = 0.0_f32;
        weapon_stance_static_ofs(ps, stance, fade_globals, &mut pitch, &mut origin_target);
        angles_target[0] += pitch;
    }
    if moving && !ladder && !reloading && !night_vision {
        calculate_weapon_movement_targets(
            ps,
            kinematics,
            movement,
            min_speed,
            &mut origin_target,
            &mut angles_target,
        );
    }
    stance_movement_lerp(
        state,
        origin_target,
        angles_target,
        movement,
        kinematics,
        prone,
    );
}

pub fn weapon_placement_assemble(
    state: &mut WeaponPlacementState,
    ps: WeaponPlacementPsInputs,
    stance: WeaponStanceStaticOfsInputs,
    fade_globals: StanceTransitionFadeGlobals,
    movement: WeaponMovementOfsInputs,
    kinematics: WeaponMovementKinematics,
    bob_inputs: WeaponBobInputs,
    idle: WeaponIdleInputs,
    bob_waveform: Option<WeaponBobState>,
    gun_spring_hip: GunRecoilResponse,
    gun_spring_ads: GunRecoilResponse,
    gun_max_pitch: f32,
    gun_max_yaw: f32,
    dt_secs: f32,
    steps_out: &mut [WeaponPlacementAssembleStep; PLACEMENT_ASSEMBLE_STEP_COUNT],
) -> WeaponPlacementContribution {
    let mut step = 0_usize;
    steps_out[step] = WeaponPlacementAssembleStep::BaseStanceMovementAngles;
    step += 1;
    base_stance_movement_angles(state, ps, stance, fade_globals, movement, kinematics);

    steps_out[step] = WeaponPlacementAssembleStep::Sway;
    step += 1;

    steps_out[step] = WeaponPlacementAssembleStep::GunRecoil;
    step += 1;
    calculate_weapon_position_gun_recoil(
        &mut state.gun_recoil,
        dt_secs,
        ps.weapon_pos_frac,
        ps.aim_down_sight,
        gun_spring_hip,
        gun_spring_ads,
        [gun_max_pitch, gun_max_yaw],
    );

    steps_out[step] = WeaponPlacementAssembleStep::Bob;
    step += 1;
    if let Some(wave) = bob_waveform {
        calculate_weapon_movement_bob(state, ps, bob_inputs, wave);
    }

    steps_out[step] = WeaponPlacementAssembleStep::ApplyAngles;
    step += 1;
    let mut angles = weapon_placement_apply_angles(state, ps);
    apply_idle_sway_scale(state, ps, idle, kinematics.frametime, &mut angles);
    let dmg = weapon_damage_kick_angles(
        state.damage_kick_time,
        state.damage_time,
        state.v_dmg_pitch,
        state.v_dmg_roll,
        ps.weapon_pos_frac,
        ps.overlay_reticle,
    );
    state.damage_kick_angles = dmg;
    angles[0] += dmg[0];
    angles[1] += dmg[1];
    angles[2] += dmg[2];

    steps_out[step] = WeaponPlacementAssembleStep::ApplyOrigin;
    let _ = step + 1;
    let origin = weapon_placement_apply_origin(state, ps);

    WeaponPlacementContribution { origin, angles }
}

pub fn weapon_placement_jump_land_ofs() -> [f32; 3] {
    panic!("jump/land gun path is not modelled; eye bob is the first-person view offset");
}

pub const DUAL_WIELD_VIEW_MODEL_OFFSET_LEFT_SCALE: f32 = 2.0;

#[must_use]
pub fn dual_wield_view_model_origin_add(hand: i32, right: [f32; 3], offset: f32) -> [f32; 3] {
    let scale = if hand == 0 {
        1.0
    } else {
        DUAL_WIELD_VIEW_MODEL_OFFSET_LEFT_SCALE
    };
    let s = offset * scale;
    [right[0] * s, right[1] * s, right[2] * s]
}
