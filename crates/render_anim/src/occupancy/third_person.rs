use asset_world::ClipCollision;
use net::PresentedSnapshot;
use playerstate_iw4::{
    CG_CAMERA_PULLBACK_BOX_HALF, CG_CAMERA_PULLBACK_CLIPMASK, CG_THIRD_PERSON_RANGE_DEFAULT,
    KillCamMode, OffsetThirdPersonViewInputs, ThirdPersonViewInputs, is_third_person_view,
    offset_third_person_view,
};
use sim::ClientId;

use render_scene::WorldCameraPose;

pub const CG_THIRD_PERSON_ANGLE_MP: f32 = 356.0;

pub fn presented_is_third_person(
    presented: &PresentedSnapshot,
    local: ClientId,
    in_killcam: bool,
    third_person: bool,
) -> bool {
    let Some(ps) = presented.player(local) else {
        return false;
    };
    if in_killcam && ps.kill_cam_entity != playerstate_iw4::ENTITYNUM_NONE {
        return true;
    }
    if remote_missile_camera(presented, local, 0).is_some() {
        return true;
    }
    is_third_person_view(ThirdPersonViewInputs {
        pm_type: ps.pm_type,
        other_flags: ps.other_flags,
        link_flags: ps.link_flags,
        cg_third_person: third_person && !in_killcam,
        in_killcam,
        killcam_mode: KillCamMode::Mode0,
    })
}

pub fn remote_missile_camera(
    presented: &PresentedSnapshot,
    local: ClientId,
    at_time: i32,
) -> Option<WorldCameraPose> {
    let link = presented
        .snapshot()?
        .meta
        .for_client(local)?
        .remote_missile
        .filter(|link| link.unlink_at_ms.is_none())?;
    let missile = presented
        .presented_projectiles()
        .iter()
        .find(|p| p.authoritative_id() == Some(link.projectile))?;
    Some(WorldCameraPose {
        origin: missile.origin_at(at_time),
        angles: link.angles,
    })
}

pub fn linked_weapon_camera(
    presented: &PresentedSnapshot,
    local: ClientId,
) -> Option<WorldCameraPose> {
    let current = presented.snapshot()?.meta.for_client(local)?;
    let view = current.linked_weapon_view?;
    let previous = presented
        .interpolation_pair()
        .and_then(|(before, _, fraction)| {
            let meta = before.meta.for_client(local)?;
            (meta.life_sequence == current.life_sequence)
                .then_some((meta.linked_weapon_view?, fraction.clamp(0.0, 1.0)))
        });
    Some(match previous {
        Some((previous, fraction)) => WorldCameraPose {
            origin: std::array::from_fn(|i| {
                previous.origin[i] + (view.origin[i] - previous.origin[i]) * fraction
            }),
            angles: std::array::from_fn(|i| {
                previous.angles[i]
                    + math_iw4::angle_subtract(view.angles[i], previous.angles[i]) * fraction
            }),
        },
        None => WorldCameraPose {
            origin: view.origin,
            angles: view.angles,
        },
    })
}

pub fn third_person_camera(
    presented: &PresentedSnapshot,
    local: ClientId,
    clip: Option<&ClipCollision>,
) -> Option<WorldCameraPose> {
    let ps = presented.player(local)?;
    let offset = presented.view_offset();
    let yaw = presented
        .snapshot()?
        .meta
        .for_client(local)
        .map(|meta| meta.look_at_killer_yaw as f32)
        .unwrap_or(ps.viewangles[1]);

    let half = CG_CAMERA_PULLBACK_BOX_HALF;
    let trace = |start: [f32; 3], end: [f32; 3]| {
        let Some(clip) = clip else { return 1.0 };
        clip.sweep_box(
            start,
            end,
            [-half, -half, -half],
            [half, half, half],
            CG_CAMERA_PULLBACK_CLIPMASK,
        )
        .fraction
    };
    let view = offset_third_person_view(
        OffsetThirdPersonViewInputs {
            origin: [
                ps.origin[0] + offset[0],
                ps.origin[1] + offset[1],
                ps.origin[2] + offset[2],
            ],
            view_height_current: ps.view_height_current,
            viewangles: ps.viewangles,
            pm_type: ps.pm_type,
            look_at_killer_yaw: yaw,

            corpse_j_mainroot: None,
            other_flags: ps.other_flags,
            delta_time: ps.delta_time,
            cg_third_person_angle: if ps.pm_type > 7 {
                CG_THIRD_PERSON_ANGLE_MP
            } else {
                0.0
            },
            cg_third_person_range: CG_THIRD_PERSON_RANGE_DEFAULT,
        },
        trace,
    );
    Some(WorldCameraPose {
        origin: view.origin,
        angles: view.angles,
    })
}
