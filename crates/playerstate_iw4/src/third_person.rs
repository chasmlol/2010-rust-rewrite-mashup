use crate::chrome::{KillCamMode, third_person_in_killcam};
use crate::other_flags;
use math_iw4::{angle_vectors, vec_to_yaw};

pub const PM_TYPE_SPECTATOR: i32 = 5;

pub const PM_TYPE_INTERMISSION: i32 = 6;

pub const PM_TYPE_LAST_STAND: i32 = 7;

pub const PM_TYPE_DEAD: i32 = 8;

pub const PM_TYPE_DEAD_LINKED: i32 = 9;

pub const PM_TYPE_NORMAL_LINKED: i32 = 1;

pub const MAX_CLIENT_CORPSES: i32 = 8;

pub const PLAYER_CORPSE_ENTITY_BASE: i32 = 18;

pub const GENTITY_SPAWN_BASE: i32 = PLAYER_CORPSE_ENTITY_BASE + MAX_CLIENT_CORPSES;

pub const CG_THIRD_PERSON_RANGE_DEFAULT: f32 = 120.0;

pub const CG_THIRD_PERSON_PITCH_CLAMP: f32 = 45.0;

pub const CG_THIRD_PERSON_FOCUS_Z: f32 = 8.0;

pub const CG_THIRD_PERSON_PITCH_SCALE: f32 = 0.5;

pub const CG_THIRD_PERSON_ATAN2_DEG: f32 = -57.2957795;

pub const CG_THIRD_PERSON_FOCUS_DISTANCE: f32 = 512.0;

pub const CG_CAMERA_PULLBACK_Z_BIAS: f32 = 32.0;

pub const CG_CAMERA_PULLBACK_BOX_HALF: f32 = 4.0;

pub const CG_CAMERA_PULLBACK_CLIPMASK: u32 = 0x0281_0011;

pub const LINK_FLAGS_WEAPON_VIEW_ONLY: u32 = 4;

#[derive(Clone, Copy, Debug)]
pub struct ThirdPersonViewInputs {
    pub pm_type: i32,
    pub other_flags: u32,
    pub link_flags: u32,

    pub cg_third_person: bool,
    pub in_killcam: bool,
    pub killcam_mode: KillCamMode,
}

pub fn is_third_person_view(i: ThirdPersonViewInputs) -> bool {
    let mut tpv = i.pm_type > 7
        || (i.other_flags & 2) != 0
        || (i.cg_third_person
            && ((i.other_flags & other_flags::DEAD_KILLCAM_TPV) == 0 || i.in_killcam));
    if third_person_in_killcam(i.in_killcam, i.killcam_mode) {
        tpv = true;
    }
    if (i.link_flags & LINK_FLAGS_WEAPON_VIEW_ONLY) != 0 {
        tpv = true;
    }
    tpv
}

pub fn look_at_killer_yaw(
    attacker_origin: Option<[f32; 3]>,
    self_origin: [f32; 3],
    self_view_yaw: f32,
) -> i32 {
    match attacker_origin {
        Some(origin) => vec_to_yaw(origin[0] - self_origin[0], origin[1] - self_origin[1]) as i32,
        None => self_view_yaw as i32,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OffsetThirdPersonViewInputs {
    pub origin: [f32; 3],
    pub view_height_current: f32,
    pub viewangles: [f32; 3],
    pub pm_type: i32,

    pub look_at_killer_yaw: f32,

    pub corpse_j_mainroot: Option<[f32; 3]>,
    pub other_flags: u32,
    pub delta_time: i32,
    pub cg_third_person_angle: f32,
    pub cg_third_person_range: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThirdPersonView {
    pub origin: [f32; 3],
    pub angles: [f32; 3],
}

pub fn pull_back_camera_through_brush(
    start: [f32; 3],
    end: [f32; 3],
    mut trace_fraction: impl FnMut([f32; 3], [f32; 3]) -> f32,
) -> [f32; 3] {
    let first = trace_fraction(start, end);
    if first == 1.0 {
        return end;
    }
    let mut lifted = [
        start[0] + (end[0] - start[0]) * first,
        start[1] + (end[1] - start[1]) * first,
        start[2] + (end[2] - start[2]) * first,
    ];
    lifted[2] += (1.0 - first) * CG_CAMERA_PULLBACK_Z_BIAS;
    let second = trace_fraction(start, lifted);
    [
        start[0] + (lifted[0] - start[0]) * second,
        start[1] + (lifted[1] - start[1]) * second,
        start[2] + (lifted[2] - start[2]) * second,
    ]
}

pub fn offset_third_person_view(
    i: OffsetThirdPersonViewInputs,
    trace_fraction: impl FnMut([f32; 3], [f32; 3]) -> f32,
) -> ThirdPersonView {
    if (i.other_flags & other_flags::DEAD_KILLCAM_TPV) != 0 && i.delta_time == 0 {
        panic!("death-cam seat: spectator/failed-archive, not death-watch");
    }

    let mut cam_org = i.origin;
    cam_org[2] += i.view_height_current;
    if i.pm_type > 7
        && let Some(root) = i.corpse_j_mainroot
    {
        cam_org = root;
    }

    let mut focus_pitch = i.viewangles[0];
    let mut focus_yaw = i.viewangles[1];
    if i.pm_type > 7 {
        focus_yaw = i.look_at_killer_yaw;
    }
    if CG_THIRD_PERSON_PITCH_CLAMP < focus_pitch {
        focus_pitch = CG_THIRD_PERSON_PITCH_CLAMP;
    }
    let (focus_fwd, _, _) = angle_vectors([focus_pitch, focus_yaw, 0.0]);
    let focus_point = [
        cam_org[0] + focus_fwd[0] * CG_THIRD_PERSON_FOCUS_DISTANCE,
        cam_org[1] + focus_fwd[1] * CG_THIRD_PERSON_FOCUS_DISTANCE,
        cam_org[2] + focus_fwd[2] * CG_THIRD_PERSON_FOCUS_DISTANCE,
    ];

    let mut view = cam_org;
    view[2] += CG_THIRD_PERSON_FOCUS_Z;

    let cam_yaw = focus_yaw - i.cg_third_person_angle;
    let cam_pitch_half = i.viewangles[0] * CG_THIRD_PERSON_PITCH_SCALE;
    let (forward, _, _) = angle_vectors([cam_pitch_half, cam_yaw, 0.0]);
    let back = -i.cg_third_person_range;
    view[0] += back * forward[0];
    view[1] += back * forward[1];
    view[2] += back * forward[2];

    let view = pull_back_camera_through_brush(cam_org, view, trace_fraction);

    let dx = focus_point[0] - view[0];
    let dy = focus_point[1] - view[1];
    let dz = focus_point[2] - view[2];
    let focus_dist = libm::sqrtf(dx * dx + dy * dy).max(1.0);
    let cam_pitch = libm::atan2f(dz, focus_dist) * CG_THIRD_PERSON_ATAN2_DEG;

    ThirdPersonView {
        origin: view,
        angles: [cam_pitch, cam_yaw, 0.0],
    }
}

pub fn viewweapon_frame_runs(pm_type: i32) -> bool {
    pm_type != PM_TYPE_SPECTATOR && pm_type != PM_TYPE_INTERMISSION && pm_type < PM_TYPE_DEAD
}
