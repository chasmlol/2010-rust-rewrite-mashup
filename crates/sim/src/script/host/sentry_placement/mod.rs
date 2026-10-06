mod geometry;

use super::natives::engine::TraceIgnore;
use crate::bullet_collision::{
    CONTENTS_BODY, MASK_PLAYER_SOLID, PLAYER_MAXS, PLAYER_MINS, TraceOutcome,
};
use crate::frame::FrameWorld;
use crate::{AuthorityModelOwner, ClientId, ClientLifecycle};
use bevy_ecs::prelude::World;
use geometry::{Backend, Config, Request, Support};

const CONFIG: Config = Config {
    reach: 64.0,
    footprint_radius: 24.0,
    support_up: 48.0,
    support_down: 64.0,
    max_slope_radians: 25.0 * std::f64::consts::PI / 180.0,
    max_foot_gap: 6.0,
    clearance: 1.0,
    frame_radius: 2.0,
    body_radius: 12.0,
    body_bottom: 16.0,
    body_top: 48.0,
};

struct Queries<'w> {
    frame: FrameWorld<'w>,
    ignore: TraceIgnore,
    players: Vec<[f32; 3]>,
}

impl Queries<'_> {
    fn world_sweep(&self, start: [f32; 3], end: [f32; 3], radius: f32) -> trace_iw4::Trace {
        self.frame.trace_world_hull_except(
            movement_iw4::GroundTraceInput {
                start,
                end,
                mins: [-radius; 3],
                maxs: [radius; 3],
                tracemask: MASK_PLAYER_SOLID,
            },
            self.ignore.model.map(AuthorityModelOwner::ScriptModel),
        )
    }

    fn players_clear(&self, start: [f32; 3], end: [f32; 3], radius: f32) -> bool {
        self.players.iter().all(|origin| {
            let hit = clipmap_iw4::transformed_temp_capsule_trace(
                start,
                end,
                [-radius; 3],
                [radius; 3],
                *origin,
                PLAYER_MINS,
                PLAYER_MAXS,
                CONTENTS_BODY,
                MASK_PLAYER_SOLID,
            );
            hit.contents == 0 && hit.startsolid == 0 && hit.allsolid == 0
        })
    }
}

fn coordinates(v: [f64; 3]) -> Option<[f32; 3]> {
    let v = v.map(|x| x as f32);
    v.iter().all(|x| x.is_finite()).then_some(v)
}

impl Backend for Queries<'_> {
    fn support(&mut self, start: [f64; 3], end: [f64; 3]) -> Option<Support> {
        let hit = self.world_sweep(coordinates(start)?, coordinates(end)?, 0.0);
        if hit.startsolid != 0 || hit.allsolid != 0 || !(0.0..1.0).contains(&hit.fraction) {
            return None;
        }
        let point =
            core::array::from_fn(|i| start[i] + (end[i] - start[i]) * f64::from(hit.fraction));
        if !self.players_clear(coordinates(start)?, coordinates(point)?, 0.0) {
            return None;
        }
        Some(Support {
            point,
            normal: hit.normal.map(f64::from),
        })
    }

    fn sphere_clear(&mut self, start: [f64; 3], end: [f64; 3], radius: f64) -> bool {
        let (Some(start), Some(end)) = (coordinates(start), coordinates(end)) else {
            return false;
        };
        let radius = radius as f32;
        if !radius.is_finite() || radius < 0.0 {
            return false;
        }
        if radius == 0.0 {
            return matches!(
                self.frame.current_sensor_trace(crate::BulletTraceQuery {
                    start,
                    end,
                    mask: MASK_PLAYER_SOLID,
                    ignore: self.ignore.client,
                    ignore_hit: None,
                    ignore_model: self.ignore.model,
                }),
                TraceOutcome::Miss { .. }
            );
        }
        let hit = self.world_sweep(start, end, radius);
        hit.fraction >= 1.0
            && hit.startsolid == 0
            && hit.allsolid == 0
            && self.players_clear(start, end, radius)
    }
}

pub(super) fn place(
    world: &mut World,
    client: u32,
    origin: [f32; 3],
    yaw: f32,
    eye_height: f32,
) -> (bool, [f32; 3], [f32; 3]) {
    let model = {
        let runtime = world.resource::<crate::script::Runtime>();
        let actor = runtime.players.get(&client).map(|p| p.object);
        runtime.engine.turrets.iter().find_map(|(object, turret)| {
            (turret.carried && actor.is_some() && turret.owner == actor)
                .then(|| runtime.entities.get(object).and_then(|e| e.presence))
                .flatten()
        })
    };
    let frame = super::presence::settled(world);
    let players = frame
        .client_ids_sorted()
        .into_iter()
        .filter(|id| {
            id.0 != client
                && frame
                    .client_meta(*id)
                    .is_some_and(|meta| meta.lifecycle == ClientLifecycle::Alive)
        })
        .filter_map(|id| frame.player(id).map(|ps| ps.origin))
        .chain(
            frame
                .prediction_remote_bodies()
                .iter()
                .filter(|body| body.client.0 != client)
                .map(|body| body.origin),
        )
        .collect();
    let mut backend = Queries {
        frame,
        ignore: TraceIgnore {
            client: Some(ClientId(client)),
            model,
            ..Default::default()
        },
        players,
    };
    let (sy, cy) = f64::from(yaw).to_radians().sin_cos();
    let request = Request {
        player_origin: origin.map(f64::from),
        eye: [
            f64::from(origin[0]),
            f64::from(origin[1]),
            f64::from(origin[2]) + f64::from(eye_height),
        ],
        forward: [cy, sy, 0.0],
    };
    let result = geometry::solve(request, CONFIG, &mut backend);
    let Some(origin) = coordinates(result.pose.origin) else {
        return (false, origin, [0.0, yaw, 0.0]);
    };
    let angles = math_iw4::axis_to_angles(result.pose.axis.map(|row| row.map(|x| x as f32)));
    (result.valid, origin, angles)
}
