use asset_game::{WeaponBodyFacts, WeaponKickFacts};
use assets::PreparedWeapons;
use bevy::prelude::*;
use frame::{LifeStarted, ViewSubject};
use hud_iw4::{
    CG_FOV_MIN_DEFAULT, CG_FOV_SCALE_DEFAULT, FovInputs, WeaponAdsOverlayFacts, calc_fov_from_ads,
    horizontal_to_vertical_fov_deg, zoom_sensitivity,
};
use math_iw4::{add_lean_to_position, angle_vectors};
use net::{
    AppliedEntityEventWalk, ClientActionInput, FrameClock, LocalPresentClient, PresentedSnapshot,
};
use weapon_iw4::{
    VIEW_DAMAGE_UNDIRECTED, VIEW_ORG_BOB_Z_MIN_OFS, ViewAngleBobInputs, ViewOrgBobInputs,
    crash_land_fall_height, crash_land_view_dip, damage_feedback_kick, get_viewmodel_weapon_index,
    land_origin_z, view_angle_bob, view_damage_angles, view_org_bob, viewweapon_land_origin_z,
};

use crate::anim::view_kick_state::{KickParams, ViewKickState, add_kick_to_viewangles};
use crate::anim::view_sway::ViewSwayState;
use crate::occupancy::remote_body::RemotePlayer;
use crate::occupancy::third_person::{
    death_watch_camera, presented_is_third_person, remote_missile_camera,
};
use render_scene::{FlyCamera, FpvLens, SimCamera, transform_from_iw_view};
use render_scene::{WorldCameraPose, WorldScriptModelInstance};

const MISSILE_CAM_FOV: f32 = 15.0;

fn kick_params(k: &WeaponKickFacts) -> KickParams {
    KickParams {
        f_ads_view_kick_center_speed: k.f_ads_view_kick_center_speed,
        f_hip_view_kick_center_speed: k.f_hip_view_kick_center_speed,
        gun_max_pitch: k.gun_max_pitch,
        gun_max_yaw: k.gun_max_yaw,
        ads_gun_kick_reduced_kick_percent: k.ads_gun_kick_reduced_kick_percent,
        ads_gun_kick_pitch_min: k.ads_gun_kick_pitch_min,
        ads_gun_kick_pitch_max: k.ads_gun_kick_pitch_max,
        ads_gun_kick_yaw_min: k.ads_gun_kick_yaw_min,
        ads_gun_kick_yaw_max: k.ads_gun_kick_yaw_max,
        ads_gun_kick_accel: k.ads_gun_kick_accel,
        ads_gun_kick_speed_max: k.ads_gun_kick_speed_max,
        ads_gun_kick_speed_decay: k.ads_gun_kick_speed_decay,
        ads_gun_kick_static_decay: k.ads_gun_kick_static_decay,
        ads_view_kick_pitch_min: k.ads_view_kick_pitch_min,
        ads_view_kick_pitch_max: k.ads_view_kick_pitch_max,
        ads_view_kick_yaw_min: k.ads_view_kick_yaw_min,
        ads_view_kick_yaw_max: k.ads_view_kick_yaw_max,
        hip_gun_kick_reduced_kick_percent: k.hip_gun_kick_reduced_kick_percent,
        hip_gun_kick_pitch_min: k.hip_gun_kick_pitch_min,
        hip_gun_kick_pitch_max: k.hip_gun_kick_pitch_max,
        hip_gun_kick_yaw_min: k.hip_gun_kick_yaw_min,
        hip_gun_kick_yaw_max: k.hip_gun_kick_yaw_max,
        hip_gun_kick_accel: k.hip_gun_kick_accel,
        hip_gun_kick_speed_max: k.hip_gun_kick_speed_max,
        hip_gun_kick_speed_decay: k.hip_gun_kick_speed_decay,
        hip_gun_kick_static_decay: k.hip_gun_kick_static_decay,
        hip_view_kick_pitch_min: k.hip_view_kick_pitch_min,
        hip_view_kick_pitch_max: k.hip_view_kick_pitch_max,
        hip_view_kick_yaw_min: k.hip_view_kick_yaw_min,
        hip_view_kick_yaw_max: k.hip_view_kick_yaw_max,
    }
}

#[derive(Resource, Default)]
pub struct PendingViewHurt(pub u32);

#[derive(Resource, Default)]
pub struct SessionViewKick {
    pub state: ViewKickState,
    pub sway: ViewSwayState,

    pub placement_move_origin: [f32; 3],

    pub placement_move_angles: [f32; 3],

    pub weap_idle_time: i32,

    pub last_idle_factor: f32,

    pub view_last_idle_factor: f32,

    pub land_change: f32,

    pub land_time: i32,

    pub land_view_dip: i32,

    pub viewweapon_land_z: f32,

    pub viewweapon_land_view: [f32; 3],

    pub refdef_view_angles: [f32; 3],

    pub refdef_vieworg: [f32; 3],

    pub horiz_fov_deg: f32,

    pub killcam_focus_distance: Option<f32>,

    pub damage_time: i32,

    pub v_dmg_pitch: f32,

    pub v_dmg_roll: f32,
    last_weapon_id: u32,
    last_origin: [f32; 3],
    last_velocity: [f32; 3],
    last_ground_entity: i32,
    have_land_prev: bool,
    last_damage_event: u32,
    have_damage_prev: bool,

    pub seeded_this_frame: u32,

    last_weapon_pos_frac: f32,

    pub b_position_to_ads: bool,
}

impl SessionViewKick {
    pub fn clear_for_new_life(&mut self) {
        self.state.reset();
        self.sway.reset();
        self.placement_move_origin = [0.0; 3];
        self.placement_move_angles = [0.0; 3];
        self.weap_idle_time = 0;
        self.last_idle_factor = 0.0;
        self.view_last_idle_factor = 0.0;
        self.land_change = 0.0;
        self.land_time = 0;
        self.land_view_dip = 0;
        self.viewweapon_land_z = 0.0;
        self.viewweapon_land_view = [0.0; 3];
        self.damage_time = 0;
        self.v_dmg_pitch = 0.0;
        self.v_dmg_roll = 0.0;
        self.last_origin = [0.0; 3];
        self.last_velocity = [0.0; 3];
        self.last_ground_entity = 0;
        self.have_land_prev = false;
        self.last_damage_event = 0;
        self.have_damage_prev = false;
        self.seeded_this_frame = 0;
        self.last_weapon_pos_frac = 0.0;
        self.b_position_to_ads = true;
    }
}

pub fn reset_view_kick_on_life_started(
    local: Res<LocalPresentClient>,
    mut started: MessageReader<LifeStarted>,
    mut kick: ResMut<SessionViewKick>,
    mut hurt: ResMut<PendingViewHurt>,
) {
    for ev in started.read() {
        if ev.client == local.0.0 {
            kick.clear_for_new_life();
            hurt.0 = 0;
        }
    }
}

pub fn tick_session_view_kick(
    clock: Res<FrameClock>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    weapons: Option<Res<PreparedWeapons>>,
    walk: Option<Res<AppliedEntityEventWalk>>,
    mut kick: ResMut<SessionViewKick>,
) {
    let Some(ps) = presented.player(local.0) else {
        return;
    };
    let viewmodel = get_viewmodel_weapon_index(ps);
    if viewmodel != kick.last_weapon_id {
        kick.state.reset();
        kick.sway.reset();
        kick.placement_move_origin = [0.0; 3];
        kick.placement_move_angles = [0.0; 3];
        kick.weap_idle_time = 0;
        kick.last_idle_factor = 0.0;
        kick.view_last_idle_factor = 0.0;
        kick.seeded_this_frame = 0;
        kick.last_weapon_id = viewmodel;
        kick.last_weapon_pos_frac = 0.0;
        kick.b_position_to_ads = true;
    }

    let Some(reg) = weapons.as_ref() else {
        return;
    };
    let Some(facts) = reg.0.facts_of(viewmodel) else {
        return;
    };
    if !facts.body_resolved {
        return;
    }

    let dt = clock.frametime_secs();
    kick.seeded_this_frame = 0;

    let n = walk.map(|w| w.local_fire).unwrap_or(0);
    if n > 0 {
        let reduce_window_active = ps.weapon_restrict_kick_time > 0;
        let params = kick_params(&facts.kick);
        for _ in 0..n {
            kick.state.seed_fire(
                &params,
                ps.f_weapon_pos_frac,
                ps.weap_flags,
                ps.recoil_scale,
                reduce_window_active,
            );
        }
        kick.seeded_this_frame = n;
    }

    let dt_ms = clock.frametime();
    let weapon_index = if viewmodel == 0 { 0 } else { 1 };
    let params = kick_params(&facts.kick);
    kick.state
        .advance(&params, weapon_index, ps.f_weapon_pos_frac, dt_ms);

    let overlay_active = facts.overlay_reticle != 0;
    kick.sway.advance(
        facts.sway.hip_params(),
        facts.sway.ads_params(),
        ps.viewangles,
        ps.f_weapon_pos_frac,
        facts.aim_down_sight,
        overlay_active,
        1.0,
        dt,
    );
}

pub fn sync_camera_from_presented(
    (skate, jak): (Res<frame::SkateMode>, Res<frame::JakMode>),
    mut killcam: Local<super::killcam::KillcamCamera>,
    clock: Res<FrameClock>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    sim_cam: Res<SimCamera>,
    settings: Res<frame::GameSettings>,
    mut kick: ResMut<SessionViewKick>,
    mut hurt: ResMut<PendingViewHurt>,
    weapons: Option<Res<PreparedWeapons>>,
    mut actions: Option<ResMut<ClientActionInput>>,
    mut q: Query<
        &mut Transform,
        (
            With<FlyCamera>,
            Without<RemotePlayer>,
            Without<WorldScriptModelInstance>,
        ),
    >,
    mut lenses: Query<&mut Projection, With<FpvLens>>,
    view: Res<ViewSubject>,
    death_cam_clip: Res<crate::occupancy::dyn_ent::DynEntPhysClip>,
) {
    kick.killcam_focus_distance = None;
    if !sim_cam.enabled {
        return;
    }
    let Some(ps) = presented.player(local.0) else {
        return;
    };
    let third_person = if skate.active { skate.camera } else { None }.or(if jak.active {
        jak.camera
    } else {
        None
    });
    if let Some((eye, fov)) = third_person {
        for mut transform in &mut q {
            *transform = eye;
        }
        for mut projection in &mut lenses {
            if let Projection::Perspective(p) = &mut *projection {
                p.fov = fov.to_radians();
            }
        }
        let forward = eye.forward();
        // Skate publishes vertical FOV; IW4's refdef uses a 4:3 horizontal FOV.
        kick.horiz_fov_deg = (2.0
            * ((fov.to_radians() * 0.5).tan() / hud_iw4::fov::CG_TANHALF_FOV_Y_SCALE).atan())
        .to_degrees();
        kick.refdef_vieworg = eye.translation.to_array();
        kick.refdef_view_angles = [
            (-forward.z).asin().to_degrees(),
            forward.y.atan2(forward.x).to_degrees(),
            0.,
        ];
        return;
    }
    let viewmodel = get_viewmodel_weapon_index(ps);
    if let Some((pose, fov, focus_distance)) = killcam.update(
        &presented,
        local.0,
        clock.time(),
        view.in_killcam(),
        weapons.as_deref(),
        death_cam_clip.0.as_deref(),
    ) {
        let eye = transform_from_iw_view(pose);
        for mut transform in &mut q {
            transform.translation = eye.translation;
            transform.rotation = eye.rotation;
        }
        for mut projection in &mut lenses {
            if let Projection::Perspective(perspective) = &mut *projection {
                perspective.fov = horizontal_to_vertical_fov_deg(fov).to_radians();
            }
        }
        kick.horiz_fov_deg = fov;
        kick.refdef_vieworg = pose.origin;
        kick.refdef_view_angles = pose.angles;
        kick.killcam_focus_distance = focus_distance;
        return;
    }
    if let Some(pose) = remote_missile_camera(&presented, local.0, clock.time()) {
        let eye = transform_from_iw_view(pose);
        for mut transform in &mut q {
            transform.translation = eye.translation;
            transform.rotation = eye.rotation;
        }
        let horiz = MISSILE_CAM_FOV;
        if let Some(actions) = actions.as_deref_mut() {
            actions.fov_scale = zoom_sensitivity(horiz) * actions.shellshock_look_scale;
        }
        let vertical = horizontal_to_vertical_fov_deg(horiz).to_radians();
        for mut projection in lenses.iter_mut() {
            if let Projection::Perspective(perspective) = &mut *projection {
                perspective.fov = vertical;
            }
        }
        kick.horiz_fov_deg = horiz;
        return;
    }
    if presented_is_third_person(&presented, local.0, view.in_killcam()) {
        let Some(pose) = death_watch_camera(&presented, local.0, death_cam_clip.0.as_deref())
        else {
            return;
        };
        let eye = transform_from_iw_view(pose);
        for mut transform in &mut q {
            transform.translation = eye.translation;
            transform.rotation = eye.rotation;
        }
        apply_fpv_lens_fov(
            &mut lenses,
            settings.fov,
            ps.pm_type,
            ps.link_flags,
            ps.e_flags,
            0.0,
            viewmodel,
            weapons.as_ref().and_then(|w| w.0.facts_of(viewmodel)),
            false,
            actions.as_deref_mut(),
        );
        return;
    }
    let offset = presented.view_offset();
    let xyspeed = {
        let vx = ps.velocity[0];
        let vy = ps.velocity[1];
        math_iw4::vec3_length([vx, vy, 0.0])
    };
    let org = ViewOrgBobInputs {
        bob_cycle: (ps.bob_cycle as u32 & 0xff) as u8,
        xyspeed,
        view_height_target: ps.view_height_target,
        pm_flags: ps.pm_flags,
        weapon_pos_frac: ps.f_weapon_pos_frac,
        perks0: ps.perks[0],
    };
    stamp_damage_feedback(
        &mut kick,
        &mut hurt,
        ps.damage_event,
        ps.damage_yaw,
        ps.damage_pitch,
        ps.damage_count,
        ps.viewangles,
        clock.time(),
    );
    let bob_angles = match weapons.as_ref().and_then(|w| w.0.facts_of(viewmodel)) {
        Some(facts) if facts.body_resolved => view_angle_bob(ViewAngleBobInputs {
            org,
            e_flags: ps.e_flags,
            overlay_reticle: facts.overlay_reticle,
            ads_bob_factor: facts.ads_bob_factor,
            ads_view_bob_mult: facts.ads_view_bob_mult,
            time: clock.time(),
            damage_time: kick.damage_time,
            v_dmg_pitch: kick.v_dmg_pitch,
            v_dmg_roll: kick.v_dmg_roll,
            aim_down_sight: facts.aim_down_sight,
            idle: facts.idle,
            frametime: clock.frametime_secs(),
            hold_breath_scale: 1.0,
            weap_idle_time: kick.weap_idle_time,
            view_last_idle_factor: kick.view_last_idle_factor,
        }),
        _ => view_damage_angles(ViewAngleBobInputs {
            org,
            time: clock.time(),
            damage_time: kick.damage_time,
            v_dmg_pitch: kick.v_dmg_pitch,
            v_dmg_roll: kick.v_dmg_roll,
            ..ViewAngleBobInputs::default()
        }),
    };
    kick.weap_idle_time = bob_angles.weap_idle_time;
    kick.view_last_idle_factor = bob_angles.view_last_idle_factor;
    let kick_angles = add_kick_to_viewangles(ps.viewangles, kick.state.kick_angles);
    let angles = [
        kick_angles[0] + bob_angles.pitch,
        kick_angles[1] + bob_angles.yaw,
        kick_angles[2] + bob_angles.roll,
    ];
    kick.refdef_view_angles = angles;
    let mut origin = [
        ps.origin[0] + offset[0],
        ps.origin[1] + offset[1],
        ps.origin[2] + offset[2] + ps.view_height_current,
    ];
    let bob = view_org_bob(org);
    origin[2] += bob.vertical;

    let (fwd, right, up) = angle_vectors(angles);
    origin[0] += bob.horizontal * right[0];
    origin[1] += bob.horizontal * right[1];
    origin[2] += bob.horizontal * right[2];
    let land_ofs = stamp_and_land_origin_z(
        &mut kick,
        ps.gravity,
        ps.origin,
        ps.velocity,
        ps.ground_entity_num,
        clock.time(),
    );
    origin[2] += land_ofs;
    let delta_ms = clock.time().wrapping_sub(kick.land_time);
    kick.viewweapon_land_z = viewweapon_land_origin_z(delta_ms, kick.land_change);
    kick.viewweapon_land_view = [
        kick.viewweapon_land_z * fwd[2],
        kick.viewweapon_land_z * right[2],
        kick.viewweapon_land_z * up[2],
    ];
    origin = add_lean_to_position(origin, ps.viewangles[1], ps.leanf, 16.0, 20.0);
    let min_z = ps.origin[2] + offset[2] + VIEW_ORG_BOB_Z_MIN_OFS;
    if origin[2] < min_z {
        origin[2] = min_z;
    }
    kick.refdef_vieworg = origin;
    let pose = WorldCameraPose { origin, angles };
    let eye = transform_from_iw_view(pose);
    for mut transform in &mut q {
        transform.translation = eye.translation;
        transform.rotation = eye.rotation;
    }

    if ps.f_weapon_pos_frac > kick.last_weapon_pos_frac {
        kick.b_position_to_ads = true;
    } else if ps.f_weapon_pos_frac < kick.last_weapon_pos_frac {
        kick.b_position_to_ads = false;
    }
    kick.last_weapon_pos_frac = ps.f_weapon_pos_frac;
    if let Some(horiz) = apply_fpv_lens_fov(
        &mut lenses,
        settings.fov,
        ps.pm_type,
        ps.link_flags,
        ps.e_flags,
        ps.f_weapon_pos_frac,
        viewmodel,
        weapons.as_ref().and_then(|w| w.0.facts_of(viewmodel)),
        kick.b_position_to_ads,
        actions.as_deref_mut(),
    ) {
        kick.horiz_fov_deg = horiz;
    }
}

fn apply_fpv_lens_fov(
    lenses: &mut Query<&mut Projection, With<FpvLens>>,
    base_fov: f32,
    pm_type: i32,
    link_flags: u32,
    e_flags: u32,
    f_weapon_pos_frac: f32,
    viewmodel: u32,
    facts: Option<WeaponBodyFacts>,
    b_position_to_ads: bool,
    actions: Option<&mut ClientActionInput>,
) -> Option<f32> {
    let facts = facts.filter(|f| f.body_resolved).unwrap_or_default();
    let overlay = WeaponAdsOverlayFacts {
        ads_zoom_in_frac: facts.ads_zoom_in_frac,
        ads_zoom_out_frac: facts.ads_zoom_out_frac,
        overlay_reticle: facts.overlay_reticle,
        ..WeaponAdsOverlayFacts::default()
    };
    let ads_target = if facts.ads_zoom_fov > 0.0 {
        facts.ads_zoom_fov
    } else {
        base_fov
    };
    let inputs = FovInputs {
        cg_fov: base_fov,
        pm_type,
        link_flags,
        e_flags,
        weapon_index_nonzero: viewmodel != 0,
        aim_down_sight: facts.aim_down_sight && facts.ads_zoom_fov > 0.0,
        ads_zoom_fov: ads_target,
        overlay_zoom: 0.0,
        fov_scale: CG_FOV_SCALE_DEFAULT,
        fov_min: CG_FOV_MIN_DEFAULT,
    };
    let (horiz, _) = calc_fov_from_ads(&inputs, f_weapon_pos_frac, b_position_to_ads, &overlay);
    let zoom_sensitivity = zoom_sensitivity(horiz);
    if let Some(actions) = actions {
        actions.fov_scale = zoom_sensitivity * actions.shellshock_look_scale;
    }
    let vertical = horizontal_to_vertical_fov_deg(horiz).to_radians();
    for mut projection in lenses.iter_mut() {
        if let Projection::Perspective(perspective) = &mut *projection {
            perspective.fov = vertical;
        }
    }
    Some(horiz)
}

const ENTITYNUM_NONE: i32 = 0x7FF;

fn stamp_and_land_origin_z(
    kick: &mut SessionViewKick,
    gravity: i32,
    origin: [f32; 3],
    velocity: [f32; 3],
    ground_entity: i32,
    cg_time: i32,
) -> f32 {
    if kick.have_land_prev
        && kick.last_ground_entity == ENTITYNUM_NONE
        && ground_entity != ENTITYNUM_NONE
    {
        if let Some(fall) = crash_land_fall_height(
            gravity,
            kick.last_origin[2],
            origin[2],
            kick.last_velocity[2],
        ) {
            let dip = crash_land_view_dip(fall);
            if dip > 0 {
                kick.land_change = -(dip as f32);
                kick.land_time = cg_time;
                kick.land_view_dip = dip;
            }
        }
    }
    kick.last_origin = origin;
    kick.last_velocity = velocity;
    kick.last_ground_entity = ground_entity;
    kick.have_land_prev = true;
    let delta = cg_time.wrapping_sub(kick.land_time) as f32;
    land_origin_z(delta, kick.land_change)
}

fn stamp_damage_feedback(
    kick: &mut SessionViewKick,
    hurt: &mut PendingViewHurt,
    damage_event: u32,
    damage_yaw: u32,
    damage_pitch: u32,
    damage_count: i32,
    viewangles: [f32; 3],
    cg_time: i32,
) {
    let mut stamped = false;
    if kick.have_damage_prev && damage_event != kick.last_damage_event && damage_count != 0 {
        let punch = damage_feedback_kick(damage_yaw, damage_pitch, damage_count, viewangles);
        kick.v_dmg_pitch = punch.v_dmg_pitch;
        kick.v_dmg_roll = punch.v_dmg_roll;
        kick.damage_time = cg_time.max(1);
        stamped = true;
    }
    if !stamped && hurt.0 > 0 {
        hurt.0 -= 1;
        let punch = damage_feedback_kick(
            VIEW_DAMAGE_UNDIRECTED,
            VIEW_DAMAGE_UNDIRECTED,
            1,
            viewangles,
        );
        kick.v_dmg_pitch = punch.v_dmg_pitch;
        kick.v_dmg_roll = punch.v_dmg_roll;
        kick.damage_time = cg_time.max(1);
    }
    kick.last_damage_event = damage_event;
    kick.have_damage_prev = true;
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct GunOffset {
    pub x: f32,

    pub y: f32,

    pub z: f32,
}

impl GunOffset {
    pub fn xyz(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
}

pub(crate) fn apply_cg_gun_offset_view(origin: [f32; 3], gun: [f32; 3]) -> [f32; 3] {
    [origin[0] + gun[0], origin[1] + gun[1], origin[2] + gun[2]]
}

pub(crate) fn apply_viewweapon_land_view(origin: [f32; 3], land_view: [f32; 3]) -> [f32; 3] {
    [
        origin[0] + land_view[0],
        origin[1] + land_view[1],
        origin[2] + land_view[2],
    ]
}

pub(crate) fn iw_view_placement_to_bevy_camera_local(
    origin: [f32; 3],
    angles_deg: [f32; 3],
) -> Transform {
    let [fwd, right, up] = origin;
    let translation = Vec3::new(right, up, -fwd);
    Transform {
        translation,
        rotation: crate::anim::fpv_pose::placement_angles_to_bevy_camera_quat(angles_deg),
        ..Default::default()
    }
}
