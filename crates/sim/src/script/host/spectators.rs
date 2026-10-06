use super::args::{int, string, vector};
use super::natives::player::player;
use crate::frame::FrameWorld;
use crate::script::{NativeRegistry, Runtime, Value};
use crate::{ClientId, ClientLifecycle};
use bevy_ecs::prelude::World;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Spectator {
    pub target: Option<u32>,
    pub defaults: Option<([f32; 3], [f32; 3])>,
    pub buttons: u32,
    pub free: bool,
}

fn permitted(slot: &super::players::PlayerSlot, team: &str) -> bool {
    slot.spectate.get(team).copied().unwrap_or(true)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    use crate::script::Namespace::Method;
    registry.register(Method, "allowspectateteam", |world, receiver, args| {
        let client = player(world, receiver)?;
        let team = string(args, 0)?;
        if !matches!(&*team, "axis" | "allies" | "none" | "freelook") {
            return Err(format!("invalid spectate team '{team}'"));
        }
        let on = int(args, 1)? != 0;
        world
            .resource_mut::<Runtime>()
            .players
            .get_mut(&client)
            .unwrap()
            .spectate
            .insert(team.into(), on);
        update(world, false);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setspectatedefaults", |world, receiver, args| {
        let client = player(world, receiver)?;
        let defaults = (vector(args, 0)?, vector(args, 1)?);
        world
            .resource_mut::<Runtime>()
            .players
            .get_mut(&client)
            .unwrap()
            .spectator
            .defaults = Some(defaults);
        update(world, false);
        let slot = &world.resource::<Runtime>().players[&client];
        if &*slot.sessionstate == "spectator"
            && slot.spectator.target.is_none()
            && slot.seat.archive_ms == 0
        {
            let mut frame = FrameWorld::from_world(world);
            reset_pose(&mut frame, ClientId(client), defaults);
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "getspectatingplayer", |world, receiver, _| {
        let client = player(world, receiver)?;
        update(world, false);
        let runtime = world.resource::<Runtime>();
        Ok(runtime
            .players
            .get(&client)
            .and_then(|slot| slot.spectator.target)
            .and_then(|target| runtime.players.get(&target))
            .map_or(Value::Undefined, |slot| Value::Object(slot.object)))
    });
}

fn reset_pose(frame: &mut FrameWorld, client: ClientId, defaults: ([f32; 3], [f32; 3])) {
    let command = frame
        .ecs()
        .get_resource::<crate::step::StepRequest>()
        .and_then(|request| {
            request
                .input
                .cmds
                .iter()
                .rev()
                .find(|command| command.client == client)
                .map(|command| command.command.angles)
        })
        .or_else(|| frame.old_cmd_angles(client));
    frame.set_origin(client, defaults.0);
    frame.set_viewangles(client, defaults.1);
    if let Some(ps) = frame.player_mut(client) {
        ps.delta_angles = std::array::from_fn(|axis| {
            defaults.1[axis] - command.unwrap_or([0; 3])[axis] as f32 * movement_iw4::SHORT2ANGLE
        });
    }
}

pub(crate) fn advance(world: &mut World) {
    update(world, true);
}

fn update(world: &mut World, input: bool) {
    let mut runtime = std::mem::take(&mut *world.resource_mut::<Runtime>());
    let clients: Vec<u32> = runtime.players.keys().copied().collect();
    let mut frame = FrameWorld::from_world(world);
    let targets: Vec<(u32, i32, bool)> = clients
        .iter()
        .filter_map(|client| {
            let meta = frame.client_meta(ClientId(*client))?;
            frame.player(ClientId(*client))?;
            Some((
                *client,
                meta.client_state_team,
                meta.lifecycle == ClientLifecycle::Alive,
            ))
        })
        .collect();
    let mut notes = Vec::new();
    for client in clients {
        let slot = runtime.players.get_mut(&client).unwrap();
        if &*slot.sessionstate != "spectator" {
            slot.spectator.target = None;
            slot.spectator.free = false;
            slot.spectator.buttons = 0;
            continue;
        }
        let archived = slot.seat.archive_ms > 0;
        let allowed: Vec<u32> = targets
            .iter()
            .filter(|(target, team, alive)| {
                *target != client
                    && (*alive || archived)
                    && permitted(slot, super::players::team_name(*team))
            })
            .map(|(client, _, _)| *client)
            .collect();
        let previous = slot.spectator.target;
        let buttons = if input {
            crate::script_player::buttons(&mut frame, ClientId(client))
        } else {
            slot.spectator.buttons
        };
        let pressed = buttons & !slot.spectator.buttons;
        if input {
            slot.spectator.buttons = buttons;
        }
        let forced = u32::try_from(slot.seat.spectator_client).ok();
        let cycle = input
            && !archived
            && pressed & (playerstate_iw4::buttons::ATTACK | playerstate_iw4::buttons::ADS) != 0;
        if let Some(forced) = forced {
            slot.spectator.target = allowed.contains(&forced).then_some(forced);
            slot.spectator.free = false;
        } else if input
            && pressed & playerstate_iw4::buttons::USE != 0
            && permitted(slot, "freelook")
        {
            slot.spectator.target = None;
            slot.spectator.free = true;
        } else if cycle {
            let backward = pressed & playerstate_iw4::buttons::ADS != 0;
            slot.spectator.target = if allowed.is_empty() {
                None
            } else {
                let index = previous.and_then(|target| allowed.iter().position(|id| *id == target));
                Some(
                    allowed[match index {
                        Some(index) if backward => (index + allowed.len() - 1) % allowed.len(),
                        Some(index) => (index + 1) % allowed.len(),
                        None if backward => allowed.len() - 1,
                        None => 0,
                    }],
                )
            };
            slot.spectator.free = false;
        } else if !slot.spectator.free || !permitted(slot, "freelook") {
            slot.spectator.free = false;
            slot.spectator.target = previous
                .filter(|target| allowed.contains(target))
                .or_else(|| allowed.first().copied());
        }
        if previous != slot.spectator.target || cycle {
            notes.push(slot.object);
        }
        if archived {
            continue;
        }
        let id = ClientId(client);
        if let Some(target) = slot.spectator.target {
            if let Some(target) = frame.player(ClientId(target)).copied() {
                frame.set_origin(id, target.origin);
                frame.set_viewangles(id, target.viewangles);
            }
        } else {
            let freelook = permitted(slot, "freelook");
            if (previous.is_some() || !freelook)
                && let Some((origin, angles)) = slot.spectator.defaults
            {
                reset_pose(&mut frame, id, (origin, angles));
            }
            if input && freelook {
                let commands = frame
                    .ecs()
                    .resource::<crate::step::StepRequest>()
                    .input
                    .cmds
                    .clone();
                for command in commands.iter().filter(|command| command.client == id) {
                    let cmd = &command.command;
                    if let Some(ps) = frame.player_mut(id) {
                        let delta = cmd.server_time.wrapping_sub(ps.command_time);
                        if delta <= 0 {
                            continue;
                        }
                        let dt = delta.min(200) as f32 / 1000.0;
                        ps.command_time = cmd.server_time;
                        ps.viewangles = std::array::from_fn(|axis| {
                            cmd.angles[axis] as f32 * movement_iw4::SHORT2ANGLE
                                + ps.delta_angles[axis]
                        });
                        let (forward, right, _) = math_iw4::angle_vectors(ps.viewangles);
                        let vertical = i32::from(cmd.buttons & playerstate_iw4::buttons::JUMP != 0)
                            - i32::from(cmd.buttons & playerstate_iw4::buttons::CROUCH != 0);
                        let direction: [f32; 3] = std::array::from_fn(|axis| {
                            forward[axis] * cmd.forwardmove as f32 / 127.0
                                + right[axis] * cmd.rightmove as f32 / 127.0
                                + if axis == 2 { vertical as f32 } else { 0.0 }
                        });
                        let length = direction.iter().map(|v| v * v).sum::<f32>().sqrt().max(1.0);
                        ps.origin = std::array::from_fn(|axis| {
                            ps.origin[axis] + direction[axis] / length * 240.0 * dt
                        });
                        ps.pm_type = playerstate_iw4::PM_TYPE_SPECTATOR;
                    }
                }
            }
        }
    }
    *frame.ecs().resource_mut::<Runtime>() = runtime;
    for object in notes {
        crate::script::runtime::raise(
            frame.ecs(),
            Value::Object(object),
            "spectating_cycle",
            Vec::new(),
        );
    }
}
