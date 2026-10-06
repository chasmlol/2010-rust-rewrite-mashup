use crate::frame::FrameWorld;

use bevy_ecs::prelude::{Resource, World};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use movement_iw4::{
    ANGLE2SHORT, AirMoveContext, CmdScaleWalkContext, CollisionBackend, GroundTraceInput,
    JumpLaunchContext, MoveBounds, PmoveSingleContext, SHORT2ANGLE, SprintContext, ViewAngleClamp,
    WalkMoveContext, consume_player_events, footsteps_anim_move_type, get_max_sprint_time, pmove,
};
use trace_iw4::{HITTYPE_ENTITY, Trace};

use crate::bullet_collision::{LinkedBrushCollisionBrush, PLAYER_MAXS, PLAYER_MINS};
use crate::combat::{advance_weapon_command, phase_emit, phase_trace};
use crate::identities::MatchPhase;
use crate::input::{ClientAction, TickInput};
use crate::match_state::{ClientLifecycle, EventAudience, SimEvent};
use crate::snapshot::Snapshot;
use crate::world::{
    ClientId, SimBrush, SimClipBsp, SimClipMesh, SimState, Tick, clip_move_to_bmodels,
    clip_move_to_model_brushes, clip_trace, give_weapon_to_ps_akimbo, gsc_give_weapon_is_akimbo,
    inventory_add_weapon,
};
use crate::{ActionOutcome, ActionResult, TickEffects, TickResult};
use playerstate_iw4::PlayerState;
use playerstate_iw4::buttons;

#[derive(Resource)]
pub(crate) struct StepRequest {
    pub(crate) tick: Tick,
    pub(crate) input: TickInput,
    msec: i32,
    pub(crate) reason: crate::StepReason,
    action_results: Vec<ActionResult>,
    fire_results: Vec<crate::FireCommandResult>,
    output: Option<TickResult>,
}

pub(crate) fn schedule() -> Schedule {
    let mut schedule = Schedule::default();
    schedule.add_systems(
        (
            advance_time_system,
            expire_transient_events_system,
            crate::script::apply_disconnects,
            crate::script::advance_mechanics,
            crate::script::advance_scheduler,
            (
                apply_script_signals_system,
                apply_actions_system,
                crate::script::sync_players,
                crate::script::sync_presence,
                run_players_system,
                record_collision_state_system,
                run_entity_types_system,
                dispatch_touches_system,
                crate::script::sync_engine_events,
                finalize_system,
                crate::script::host::natives::iw4::deliver_local_presentation_dvars,
                publish_snapshot_system,
            )
                .chain()
                .run_if(crate::script::healthy),
        )
            .chain(),
    );
    schedule
}

pub(crate) fn run_schedule(
    ecs: &mut World,
    schedule: &mut Schedule,
    tick: Tick,
    input: &TickInput,
    msec: i32,
    reason: crate::StepReason,
) -> Result<TickResult, crate::script::Fault> {
    crate::script::preflight(ecs, tick, reason)?;
    assert!(
        !ecs.contains_resource::<StepRequest>(),
        "simulation schedule already has a pending frame"
    );
    ecs.insert_resource(StepRequest {
        tick,
        input: input.clone(),
        msec: msec.clamp(1, 200),
        reason,
        action_results: Vec::new(),
        fire_results: Vec::new(),
        output: None,
    });
    schedule.run(ecs);
    let request = ecs
        .remove_resource::<StepRequest>()
        .expect("simulation request missing");
    if let Some(fault) = ecs.resource::<crate::script::Runtime>().fault.as_ref() {
        return Err(fault.clone());
    }
    Ok(request
        .output
        .expect("simulation schedule did not publish a snapshot"))
}

fn frame_world(world: &mut World) -> FrameWorld<'_> {
    crate::frame::FrameWorld::from_world(world)
}

fn apply_script_signals_system(world: &mut World) {
    let signals = crate::script::take_signals(world);
    if signals.is_empty() {
        return;
    }
    let mut frame = frame_world(world);
    for signal in signals {
        match &*signal {
            crate::script::EXIT_LEVEL
                if !matches!(
                    frame.phase(),
                    MatchPhase::Intermission | MatchPhase::PostGame
                ) =>
            {
                frame.set_phase(MatchPhase::Intermission);
            }
            crate::script::MAP_RESTART => {
                let tick = frame.ecs().resource::<StepRequest>().tick;
                crate::script::restart_level(frame.ecs(), tick);
            }
            name => crate::script::host::iw4_gametype::apply_level_notify(&mut frame, name),
        }
    }
}

pub fn phase_materialize_entity_dobjs(world: &mut SimState) {
    for capabilities in world.entity_collision_capabilities_mut() {
        if let Some(dobj) = &mut capabilities.dobj {
            dobj.materialize();
        }
    }
}

fn phase_animated_map_models(world: &mut FrameWorld, tick: Tick, msec: i32) {
    let dt = msec as f32 / 1000.0;

    let at_time = i32::try_from(tick.0.saturating_mul(crate::MATCH_TICK_MS)).unwrap_or(i32::MAX);
    let mut mover_apos = Vec::new();
    world.visit_script_movers(|mover| {
        mover_apos.push((
            mover.id,
            crate::gentity::apos_from_entity_state(&mover.state),
        ));
    });
    for capabilities in world.entity_collision_capabilities_mut() {
        let Some(dobj) = capabilities.dobj.as_mut() else {
            continue;
        };
        if dobj.play_anim.is_some() {
            dobj.advance_script_model_play_anim(dt);
        }
        if let Some(id) = capabilities.owner.script_model() {
            if let Some((_, apos)) = mover_apos.iter().find(|(mover_id, _)| *mover_id == id) {
                dobj.apply_trajectory_apos_at(*apos, at_time);
                continue;
            }
        }
        if dobj.apos.is_some() {
            dobj.apply_apos_at(at_time);
        }
    }
}

fn phase_stuck_in_client(world: &mut FrameWorld) {
    let mut ids: Vec<ClientId> = world.client_ids_sorted();
    ids.retain(|id| {
        world
            .client_meta(*id)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
            && world.player(*id).is_some()
    });
    let mut rows: Vec<gamemode_iw4::StuckClient> = ids
        .iter()
        .filter_map(|id| world.player(*id).map(stuck_row))
        .collect();
    let mut pairs = Vec::new();
    let eject_speed = gamemode_iw4::G_PLAYER_COLLISION_EJECT_SPEED_DEFAULT;
    let mut holdrand = *world.stuck_holdrand_mut();
    for i in 0..rows.len() {
        if let Some(pair) = gamemode_iw4::stuck_in_client(i, &mut rows, eject_speed, &mut holdrand)
        {
            pairs.push((ids[pair.self_idx], ids[pair.other_idx]));
        }
    }
    *world.stuck_holdrand_mut() = holdrand;
    for (id, row) in ids.iter().zip(rows.iter()) {
        if let Some(ps) = world.player_mut(*id) {
            ps.velocity[0] = row.velocity[0];
            ps.velocity[1] = row.velocity[1];
            ps.pm_time = row.pm_time;
            ps.pm_flags = row.pm_flags;
        }
    }
    world.set_stuck_ejects(pairs);
}

fn stuck_row(ps: &PlayerState) -> gamemode_iw4::StuckClient {
    gamemode_iw4::StuckClient {
        origin: ps.origin,
        velocity: ps.velocity,
        speed: ps.speed,
        other_flags: ps.other_flags,
        health: ps.health,
        pm_time: ps.pm_time,
        pm_flags: ps.pm_flags,
        maxs_x: PLAYER_MAXS[0],
        bounds_mid: [
            ps.origin[0] + (PLAYER_MINS[0] + PLAYER_MAXS[0]) * 0.5,
            ps.origin[1] + (PLAYER_MINS[1] + PLAYER_MAXS[1]) * 0.5,
            ps.origin[2] + (PLAYER_MINS[2] + PLAYER_MAXS[2]) * 0.5,
        ],
        bounds_half: [
            (PLAYER_MAXS[0] - PLAYER_MINS[0]) * 0.5,
            (PLAYER_MAXS[1] - PLAYER_MINS[1]) * 0.5,
            (PLAYER_MAXS[2] - PLAYER_MINS[2]) * 0.5,
        ],
    }
}

fn advance_time_system(world: &mut World) {
    let samples = world.resource::<StepRequest>().input.shot_samples.clone();
    frame_world(world).set_lagcomp_commands(samples);
    let tick;
    {
        let mut request = world.resource_mut::<StepRequest>();
        request.input.canonicalize();
        tick = request.tick;
    }
    let mut frame = frame_world(world);
    frame.mark_running();
    frame.begin_entity_frame(tick);
}

fn expire_transient_events_system(world: &mut World) {
    let tick = world.resource::<StepRequest>().tick;
    let mut frame = frame_world(world);
    frame.enter_kernel_phase(crate::gentity::KernelPhase::ExpireTransientEvents);
    frame.expire_dying_missiles(tick);
    frame.clear_tick_events(tick);
}

fn apply_actions_system(world: &mut World) {
    let tick = world.resource::<StepRequest>().tick;
    let actions = world.resource::<StepRequest>().input.actions.clone();
    let mut frame = frame_world(world);
    for (id, action) in &actions {
        if action_needs_player_row(action) {
            let _ = frame.ensure_player(*id);
        } else {
            let _ = frame.client_meta_mut(*id);
        }
    }

    frame.enter_kernel_phase(crate::gentity::KernelPhase::ApplyActions);
    let results = apply_actions(&mut frame, tick, &actions);
    drop(frame);
    world.resource_mut::<StepRequest>().action_results = results;
}

fn run_players_system(ecs: &mut World) {
    let tick = ecs.resource::<StepRequest>().tick;
    let level_time = crate::level_time_ms(tick);
    let input = ecs.resource::<StepRequest>().input.clone();
    let mut world = frame_world(ecs);
    world.enter_kernel_phase(crate::gentity::KernelPhase::RunPlayers);

    let allow_move = matches!(world.phase(), MatchPhase::Playing | MatchPhase::Warmup);
    let content = world.content();
    let cmodel_models = &content.clip_cmodels().models;
    let (brushes, bsp, mesh) = (
        content.clip_brushes(),
        content.clip_bsp(),
        content.clip_mesh(),
    );
    let original_buttons = world.old_buttons_mut().clone();
    let original_angles = world.old_cmd_angles_mut().clone();
    let mut consumed = Vec::new();
    let mut fire_results: Vec<_> = input
        .cmds
        .iter()
        .filter_map(|command| {
            command.sequence.map(|sequence| crate::FireCommandResult {
                client: command.client,
                life: world
                    .client_meta(command.client)
                    .map(|meta| meta.life_sequence),
                command: sequence,
                outcome: crate::FireCommandOutcome::NotRun(
                    crate::FireCommandRefusal::MatchInactive,
                ),
            })
        })
        .collect();
    let mut next_fire_result = 0;
    if allow_move {
        phase_materialize_entity_dobjs(&mut world);
        let model_brushes = world.model_movement_brushes();
        for command in &input.cmds {
            let result_index = command.sequence.map(|_| {
                let index = next_fire_result;
                next_fire_result += 1;
                fire_results[index].outcome =
                    crate::FireCommandOutcome::NotRun(crate::FireCommandRefusal::NotAlive);
                index
            });
            let id = &command.client;
            let cmd = &command.command;
            world.select_lagcomp_command(*id, cmd.server_time);
            if !world
                .client_meta(*id)
                .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
            {
                continue;
            }
            if let Some(index) = result_index {
                fire_results[index].outcome =
                    crate::FireCommandOutcome::NotRun(crate::FireCommandRefusal::MissingPlayer);
            }
            let Some(ps) = world.player(*id) else {
                continue;
            };

            let delta = cmd.server_time.wrapping_sub(ps.command_time);
            if delta <= 0 {
                if let Some(index) = result_index {
                    fire_results[index].outcome =
                        crate::FireCommandOutcome::NotRun(crate::FireCommandRefusal::StaleCommand);
                }
                continue;
            }

            // A local external controller publishes the complete player position.
            // Keep command acknowledgement moving without also applying PMove.
            if world.external_motion.contains(id) {
                if let Some(ps) = world.player_mut(*id) { ps.command_time = cmd.server_time; }
                continue;
            }
            let old_buttons = world
                .old_buttons_mut()
                .iter()
                .find(|(c, _)| c == id)
                .map_or(0, |(_, buttons)| *buttons);
            let Some(ps) = world.player(*id).copied() else {
                continue;
            };
            let weapon = ps.weapon;
            let scales = world.scales_for(weapon);
            let facts = world.combat_facts_for(weapon);
            let aim_down_sight = facts.is_some_and(|f| f.aim_down_sight);
            let ads_allowed = facts.is_some_and(|f| {
                weapon != 0 && {
                    let mut allow = f.ads_allow_facts();

                    allow.clip_index = weapon_iw4::clip_table_key(allow.clip_index, weapon);
                    weapon_iw4::is_ads_allowed(
                        &ps,
                        &allow,
                        &ps.ammoclip,
                        ps.last_weapon_hand.min(1) as u8,
                    )
                }
            });
            let overlay_reticle = facts.map(|f| f.overlay_reticle).unwrap_or(0);
            let (ads_in_rate, ads_out_rate, rechamber_while_ads, ads_fire_only) = facts
                .map(|f| f.ads_frac_context())
                .unwrap_or((1.0 / 200.0, 1.0 / 200.0, true, false));
            let (melee_delay_ms, melee_charge_delay_ms) = facts
                .map(|f| (f.melee_delay_ms, f.melee_charge_delay_ms))
                .unwrap_or((0, 0));
            let mut context = pmove_context(
                old_buttons,
                scales,
                ads_allowed,
                aim_down_sight,
                ads_in_rate,
                ads_out_rate,
                facts.map_or(0, |f| f.ads_reload_trans_time_ms),
                facts.is_some_and(|f| f.segmented_reload),
                rechamber_while_ads,
                ads_fire_only,
                melee_delay_ms,
                melee_charge_delay_ms,
                overlay_reticle,
                facts.is_some_and(|f| f.can_hold_breath),
                world
                    .client_meta(*id)
                    .and_then(|m| m.shellshock.as_ref())
                    .is_some_and(|shock| shock.movement),
            );
            context.view_angles.unclamped_pitch_bit =
                ps.link_flags & playerstate_iw4::LINK_FLAGS_WEAPON_VIEW_ONLY != 0;
            crate::script::player_commands(world.ecs(), id.0, cmd.buttons, old_buttons);
            let mut cmd = *cmd;
            crate::script_player::constrain_cmd(&mut world, *id, &mut cmd);
            crate::script::select_location(world.ecs(), id.0, &mut cmd, old_buttons);
            if world
                .client_meta(*id)
                .is_some_and(|m| m.remote_missile.is_some())
            {
                crate::remote_missile::steer(&mut world, *id, &cmd, delta.min(200));
                cmd.forwardmove = 0;
                cmd.rightmove = 0;
                cmd.buttons &= playerstate_iw4::buttons::CROUCH | playerstate_iw4::buttons::PRONE;
            }
            let commanded_move = cmd.forwardmove != 0 || cmd.rightmove != 0;
            world.set_anim_command_buttons(*id, cmd.buttons);
            let linked_brushes: Vec<LinkedBrushCollisionBrush> = world
                .entity_collision_capabilities()
                .iter()
                .flat_map(|c| c.solid_brushes().iter().cloned())
                .collect();
            let bodies = alive_body_clips(&world);
            let glass_damage = world.world_objects().glass_damage_pairs();
            let backend = ClipBackend {
                brushes: &brushes,
                bsp: &bsp,
                mesh: &mesh,
                glass_damage: &glass_damage,
                bodies: &bodies,
                self_entnum: id.0 as u16,
                cmodels: &cmodel_models,
                linked_brushes: &linked_brushes,
                model_brushes: &model_brushes,
            };
            let script = world.player_anim_script();
            let mantle = world.xanims();
            let (
                walking,
                linked_bounds,
                anim_movetype,
                view_w,
                primary,
                moved_from,
                moved_to,
                stance_event,
                reset_torso,
                jump_animations,
                force_movement_anim,
                landing_animation,
                fall_damage,
            ) = {
                let ps = world
                    .player_mut(*id)
                    .expect("Alive client has a player row");

                if ps.shellshock_time.wrapping_add(ps.shellshock_duration) < level_time {
                    ps.pm_flags &= !playerstate_iw4::pm_flags::SHELLSHOCKED;
                }
                let moved_from = ps.origin;
                let result = pmove(
                    ps,
                    &mut cmd,
                    context,
                    &backend,
                    mantle.as_ref(),
                    mantle.as_ref(),
                );
                let pml = result.pml;
                let anim_movetype = pml.mantle_movetype.or_else(|| {
                    footsteps_anim_move_type(
                        ps,
                        cmd.forwardmove,
                        cmd.rightmove,
                        pml.almost_ground_plane != 0,
                    )
                });
                let (view_w, primary) = crate::pmove_anim_weapon_ids(ps);
                let moved_to = ps.origin;
                (
                    pml.walking as i32,
                    result.bounds,
                    anim_movetype,
                    view_w,
                    primary,
                    moved_from,
                    moved_to,
                    result.stance_event,
                    result.reset_torso,
                    pml.jump_animations,
                    pml.mantle_movetype.is_some(),
                    pml.landing_animation,
                    pml.fall_damage,
                )
            };
            world
                .client_meta_mut(*id)
                .input_receipt
                .record(commanded_move, moved_from, moved_to);
            if reset_torso && let Some(ps) = world.player_mut(*id) {
                crate::player_anim_script::reset_stance_torso(ps);
            }
            if let Some(event) = stance_event {
                crate::combat::apply_player_anim_event(&mut world, *id, event);
            }
            for (animation, force) in jump_animations.into_iter().flatten() {
                let event = match animation {
                    movement_iw4::JumpAnimation::Forward => 3,
                    movement_iw4::JumpAnimation::Backward => 4,
                };
                crate::combat::apply_player_anim_event_forced(&mut world, *id, event, force);
            }
            if landing_animation {
                crate::combat::apply_player_anim_event(&mut world, *id, 5);
            }
            if fall_damage > 0 && world.publishes_snapshot() {
                apply_fall_damage(&mut world, tick, *id, fall_damage, moved_to);
            }
            if let Some(movetype) = anim_movetype {
                let view_facts = world.combat_facts_for(view_w);
                let primary_facts = world.combat_facts_for(primary);
                let previous_movetype = world.last_anim_movetype(*id);
                let ps = world
                    .player_mut(*id)
                    .expect("Alive client has a player row");
                let strafing =
                    crate::player_anim_script::anim_strafing(ps, cmd.forwardmove, cmd.rightmove);
                let conds = crate::anim_conditions_from_pmove(
                    ps,
                    view_facts,
                    primary_facts,
                    previous_movetype,
                    strafing,
                    cmd.buttons,
                );
                if let Some(script) = script.as_ref()
                    && let Some(selected) =
                        script.apply(ps, movetype, id.0, &conds, force_movement_anim)
                {
                    world.set_anim_movement(*id, selected, strafing);
                }
            }
            world.set_pmove_walking(*id, walking);
            world.link_player_area(*id, linked_bounds);

            let shots = advance_weapon_command(
                &mut world,
                tick,
                *id,
                cmd,
                delta.min(200),
                command.sequence,
            );
            if let Some(index) = result_index {
                assert!(
                    shots.len() <= 2,
                    "accepted firing exceeded weapon hand count"
                );
                fire_results[index].outcome = crate::FireCommandOutcome::Executed {
                    accepted: std::array::from_fn(|hand| {
                        shots.get(hand).and_then(|shot| shot.fire_cause)
                    }),
                };
            }
            for shot in shots {
                crate::missile::fire_accepted_shot(&mut world, tick, &shot);

                let hitbox_ids = [*id];
                let hitboxes = if world.publishes_snapshot() {
                    None
                } else {
                    Some(hitbox_ids.as_slice())
                };
                world.record_collision_history(tick, 0, hitboxes);
                if world.publishes_snapshot() {
                    world.record_entity_collision_history(tick);
                }
                let emissions = phase_emit(&world, core::slice::from_ref(&shot));
                phase_trace(&mut world, tick, &emissions);
            }
            crate::equipment::phase_offhand(&mut world, tick, &[(*id, cmd)]);
            world.set_old_cmd(*id, cmd.buttons, cmd.angles);
            consumed.push(crate::PlayerCommand {
                command: cmd,
                ..*command
            });
        }
    }

    *world.old_buttons_mut() = original_buttons;
    *world.old_cmd_angles_mut() = original_angles;
    crate::damage::apply_block_world_damage(&mut world, tick);
    crate::item::sustain_block_world_arsenal(&mut world);

    if world.publishes_snapshot() {
        crate::script::apply_player_links(world.ecs());
    }
    restamp_debug_move_look(&mut world, &input.actions);

    if allow_move && world.publishes_snapshot() {
        phase_stuck_in_client(&mut world);
    }
    drop(world);
    let mut request = ecs.resource_mut::<StepRequest>();
    if allow_move {
        request.input.cmds = consumed;
    }
    request.fire_results = fire_results;
}

fn apply_fall_damage(world: &mut FrameWorld, tick: Tick, id: ClientId, amount: i32, at: [f32; 3]) {
    let hit = crate::script::ScriptHit {
        piece: None,
        target: crate::script::HitTarget::Player(id),
        amount,
        origin: at,
        attacker: None,
        inflictor: None,
        means: "MOD_FALLING",
        weapon: 0,
        flags: 0,
        hitloc: 0,
    };
    crate::damage::apply_script_hit(world, tick, &hit);
}

fn record_collision_state_system(ecs: &mut World) {
    let tick = ecs.resource::<StepRequest>().tick;
    let msec = ecs.resource::<StepRequest>().msec;
    let cmd_ids: Vec<crate::world::ClientId> = ecs
        .resource::<StepRequest>()
        .input
        .cmds
        .iter()
        .map(|command| command.client)
        .collect();
    let mut world = frame_world(ecs);
    world.enter_kernel_phase(crate::gentity::KernelPhase::RecordCollisionState);

    let hitbox_cmds = if world.publishes_snapshot() {
        None
    } else {
        Some(cmd_ids)
    };
    world.record_collision_history(tick, msec, hitbox_cmds.as_deref());
}

fn run_entity_types_system(ecs: &mut World) {
    let tick = ecs.resource::<StepRequest>().tick;
    let msec = ecs.resource::<StepRequest>().msec;
    let mut world = frame_world(ecs);
    let allow_move = world.phase() == MatchPhase::Playing;
    world.enter_kernel_phase(crate::gentity::KernelPhase::RunEntityTypes);
    if allow_move {
        phase_materialize_entity_dobjs(&mut world);

        if world.publishes_snapshot() {
            world.record_entity_collision_history(tick);
        }

        crate::remote_missile::advance(&mut world, tick);
        crate::entity_run::phase_run_entity_thinks(&mut world, tick);
        if world.publishes_snapshot() {
            world.world_objects_mut().glass_update(
                i32::try_from(tick.0.saturating_mul(crate::MATCH_TICK_MS)).unwrap_or(i32::MAX),
            );
            phase_animated_map_models(&mut world, tick, msec);
        }
    } else {
        crate::entity_run::phase_walk_entity_thinks(&mut world);
    }
}

fn dispatch_touches_system(ecs: &mut World) {
    let tick = ecs.resource::<StepRequest>().tick;
    let input = ecs.resource::<StepRequest>().input.clone();
    let mut world = frame_world(ecs);
    let allow_move = world.phase() == MatchPhase::Playing;
    world.enter_kernel_phase(crate::gentity::KernelPhase::DispatchTouches);
    if allow_move {
        crate::item::phase_touch_items(&mut world, tick);
    }
    let command_buttons: Vec<_> = input
        .cmds
        .iter()
        .map(|command| (command.client.0, command.command.buttons))
        .collect();
    let latest: std::collections::BTreeMap<_, _> = input
        .cmds
        .iter()
        .map(|command| (command.client, command.command))
        .collect();
    let latest_cmds: Vec<_> = latest.into_iter().collect();
    let cmds: Vec<_> = latest_cmds
        .iter()
        .map(|(id, cmd)| (id.0, cmd.buttons))
        .collect();
    let old: Vec<(u32, u32)> = world
        .old_buttons_mut()
        .iter()
        .map(|(id, bits)| (id.0, *bits))
        .collect();
    let presses = crate::collect_use_presses(&command_buttons, &old);
    world.stamp_use_presses(presses.clone());
    if allow_move && world.publishes_snapshot() {
        crate::item::phase_use_items(&mut world, tick, &presses, &cmds);
    }
}

fn finalize_system(ecs: &mut World) {
    let tick = ecs.resource::<StepRequest>().tick;
    let input = ecs.resource::<StepRequest>().input.clone();
    let mut world = frame_world(ecs);
    world.enter_kernel_phase(crate::gentity::KernelPhase::Finalize);
    for command in &input.cmds {
        let (id, cmd) = (command.client, command.command);
        emit_attack_events(&mut world, tick, &[(id, cmd)]);
        if world.client_meta(id).is_some() {
            world.set_old_cmd(id, cmd.buttons, cmd.angles);
        }
    }

    let mut old_buttons = std::mem::take(world.old_buttons_mut());
    old_buttons.retain(|(id, _)| world.client_meta(*id).is_some());
    *world.old_buttons_mut() = old_buttons;
    let mut old_angles = std::mem::take(world.old_cmd_angles_mut());
    old_angles.retain(|(id, _)| world.client_meta(*id).is_some());
    *world.old_cmd_angles_mut() = old_angles;

    harvest_predictable_events(&mut world, tick);
    world.script_gaps_mut().report();
}

fn publish_snapshot_system(ecs: &mut World) {
    let tick = ecs.resource::<StepRequest>().tick;
    let mut world = frame_world(ecs);
    world.enter_kernel_phase(crate::gentity::KernelPhase::PublishSnapshot);

    if world.publishes_snapshot() {
        emit_player_ticks(&world, tick);
    }
    let snapshot = if world.publishes_snapshot() {
        let snapshot = world.snapshot(tick);
        emit_snapshot_events(&world, &snapshot);
        snapshot
    } else {
        Snapshot::unpublished(tick)
    };
    drop(world);
    let mut world = frame_world(ecs);
    let weapon_script_names = world.weapon_script_names();
    let pending_final_kill = world.take_pending_final_kill();
    let mut effects = TickEffects {
        prints: world.take_pending_prints(),
        local_sounds: world.take_pending_local_sounds(),
        player_cards: world.take_pending_player_cards(),
        script_audio: world.take_pending_script_audio(),
        kicks: Vec::new(),
    };
    drop(world);
    let script_seats = crate::script::script_seats(ecs);
    let mut runtime = ecs.resource_mut::<crate::script::Runtime>();
    let script_exit_level = std::mem::take(&mut runtime.exit_level);
    effects.kicks = std::mem::take(&mut runtime.kicks)
        .into_iter()
        .map(|(client, reason)| (ClientId(client), reason))
        .collect();
    drop(runtime);
    let mut request = ecs.resource_mut::<StepRequest>();
    let action_results = std::mem::take(&mut request.action_results);
    let fire_results = std::mem::take(&mut request.fire_results);
    request.output = Some(TickResult {
        input: request.input.clone(),
        snapshot,
        action_results,
        fire_results,
        effects,
        weapon_script_names,
        pending_final_kill,
        script_seats,
        script_exit_level,
    });
}

fn emit_player_ticks(world: &FrameWorld, tick: Tick) {
    let bot_name = entity_iw4::pack_client_state_name("bot");
    let level_time_ms = crate::level_time_ms(tick);
    let mut rows = Vec::new();
    world.visit_players(|id, ps| {
        rows.push((
            id,
            ps.command_time,
            ps.origin,
            ps.velocity[2],
            ps.viewangles[1],
            ps.viewangles[0],
            ps.jump_time,
            ps.weaponstate_primary,
            ps.legs_anim,
            ps.torso_anim,
        ));
    });
    for (id, time_ms, origin, vz, yaw, pitch, jump_time, weaponstate, legs_anim, torso_anim) in rows
    {
        let Some(meta) = world.client_meta(id) else {
            continue;
        };
        perf::player_tick(
            id.0,
            meta.name == bot_name,
            time_ms,
            level_time_ms,
            origin,
            vz,
            yaw,
            pitch,
            jump_time,
            world.old_buttons(id).unwrap_or(0),
            weaponstate,
            meta.ammo_clip,
            world.pmove_walking(id),
            legs_anim,
            torso_anim,
            lifecycle_label(meta.lifecycle),
        );
    }
}

fn lifecycle_label(life: ClientLifecycle) -> &'static str {
    match life {
        ClientLifecycle::Connecting => "Connecting",
        ClientLifecycle::ChoosingClass => "ChoosingClass",
        ClientLifecycle::SpawnPending => "SpawnPending",
        ClientLifecycle::Alive => "Alive",
        ClientLifecycle::Dead => "Dead",
        ClientLifecycle::RespawnPending => "RespawnPending",
        ClientLifecycle::Spectating => "Spectating",
        ClientLifecycle::Intermission => "Intermission",
    }
}

fn emit_snapshot_events(world: &FrameWorld, snapshot: &Snapshot) {
    world.emit_script_entity_trace();
    for slot in &snapshot.meta.corpses.slots {
        perf::corpse(
            i64::from(slot.occupied),
            slot.occupied.then_some(slot.origin[2]),
            None,
            None,
        );
    }
    for es in snapshot
        .meta
        .entities
        .iter()
        .filter(|es| es.e_type == entity_iw4::ET_ITEM)
    {
        let ammo = snapshot
            .meta
            .item_ammo
            .iter()
            .find(|row| row.entnum == es.number);
        perf::item(
            es.e_type,
            ammo.map(|row| row.clip_r),
            ammo.map(|row| row.scavenger),
            None,
        );
    }
}

fn harvest_predictable_events(world: &mut FrameWorld, _tick: Tick) {
    world.for_each_player_mut(|_, ps| {
        let mut cursor = ps.old_event_sequence;
        consume_player_events(ps, &mut cursor, |_| {});
        ps.old_event_sequence = cursor;
    });
}

fn emit_attack_events(
    world: &mut FrameWorld,
    tick: Tick,
    cmds: &[(ClientId, playerstate_iw4::UserCmd)],
) {
    for (id, cmd) in cmds {
        if !world
            .client_meta(*id)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
        {
            continue;
        }
        let old = world
            .old_buttons_mut()
            .iter()
            .find(|(c, _)| c == id)
            .map_or(0, |(_, b)| *b);
        let attack = cmd.buttons & buttons::ATTACK != 0;
        let was = old & buttons::ATTACK != 0;
        if !attack && was {
            world.push_event(tick, EventAudience::Client(*id), SimEvent::AttackReleased);
        }
    }
}

fn action_needs_player_row(action: &ClientAction) -> bool {
    !matches!(
        action,
        ClientAction::JoinMatch { .. }
            | ClientAction::LeaveMatch { .. }
            | ClientAction::SetName { .. }
            | ClientAction::SetProfile { .. }
            | ClientAction::UseCopycat { .. }
            | ClientAction::ActionSlot { .. }
            | ClientAction::ChooseDefaultClass { .. }
            | ClientAction::MenuResponse { .. }
    )
}

fn apply_actions(
    world: &mut FrameWorld,
    tick: Tick,
    actions: &[(ClientId, ClientAction)],
) -> Vec<ActionResult> {
    actions
        .iter()
        .map(|(client, action)| ActionResult {
            client: *client,
            action: *action,
            outcome: apply_action(world, tick, *client, action),
        })
        .collect()
}

fn apply_action(
    world: &mut FrameWorld,
    tick: Tick,
    id: ClientId,
    action: &ClientAction,
) -> ActionOutcome {
    match *action {
        ClientAction::JoinMatch { request_id: _ } => {
            let meta = world.client_meta_mut(id);
            if meta.lifecycle == ClientLifecycle::Connecting {
                meta.lifecycle = ClientLifecycle::ChoosingClass;
            }

            ActionOutcome::Applied
        }
        ClientAction::ChooseDefaultClass {
            request_id: _,
            index,
        } => {
            crate::script::answer_join(world.ecs(), id.0);
            choose_bot_class(world, id, index);

            ActionOutcome::Accepted
        }
        ClientAction::MenuResponse {
            request_id: _,
            menu,
            response,
        } => {
            let menu = crate::menu_response_text(&menu);
            let response = crate::menu_response_text(&response);
            crate::script::note_team_answer(world.ecs(), id.0, menu);
            if let Some(outcome) = answer_custom_class(world, id, menu, response) {
                return outcome;
            } else {
                crate::script::answer_menu(world.ecs(), id.0, menu, response);
            }

            ActionOutcome::Accepted
        }
        ClientAction::LeaveMatch { request_id: _ } => {
            world.retire_client(id);

            ActionOutcome::Applied
        }
        ClientAction::SetName {
            request_id: _,
            name,
        } => {
            world.client_meta_mut(id).name = name;

            ActionOutcome::Applied
        }
        ClientAction::UseCopycat { .. } | ClientAction::SpawnClient { .. } => {
            ActionOutcome::Refused
        }
        ClientAction::ActionSlot {
            request_id: _,
            slot,
        } => {
            crate::script::action_slot_command(world.ecs(), id.0, slot);

            ActionOutcome::Accepted
        }
        ClientAction::SelectClass {
            request_id,
            class_id,
            revision,
            loadout,
        } => {
            return apply_select_class(world, tick, id, request_id, class_id, revision, loadout);
        }
        ClientAction::GiveWeapon {
            request_id,
            weapon,
            model,
        } => {
            if !world.bootstrap_ref().allow_debug_actions {
                return ActionOutcome::Refused;
            }
            let outcome = apply_give_weapon(world, tick, id, request_id, weapon);
            if outcome != ActionOutcome::Applied {
                return outcome;
            }
            if let Some(ps) = world.player_mut(id) {
                weapon_iw4::set_weapon_model_for_held(
                    &ps.weapons,
                    &mut ps.weapon_data,
                    weapon,
                    model,
                );
            }

            ActionOutcome::Applied
        }
        ClientAction::ChangeWeaponConfiguration {
            request_id,
            from,
            to,
        } => {
            if !world.bootstrap_ref().allow_debug_actions {
                return ActionOutcome::Refused;
            }
            return apply_configuration_change(world, tick, id, request_id, from, to);
        }
        ClientAction::ForceSpawn {
            request_id: _,
            pick,
        } => {
            if !world.bootstrap_ref().allow_debug_actions {
                return ActionOutcome::Refused;
            }
            apply_force_spawn(world, id, pick)
        }
        ClientAction::SpawnIntermission { request_id: _ } => apply_spawn_intermission(world, id),
        ClientAction::ResupplyAmmo { .. } => {
            if !world.bootstrap_ref().allow_debug_actions
                || !world
                    .client_meta(id)
                    .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
            {
                return ActionOutcome::Refused;
            }
            for weapon in
                crate::script_player::weapons(world, id, crate::script_player::WeaponList::All)
            {
                crate::script_player::give_max_ammo(world, id, weapon);
            }

            ActionOutcome::Applied
        }
        ClientAction::SetProfile { profile, .. } => {
            if crate::script::set_profile(world.ecs(), id.0, profile) {
                ActionOutcome::Applied
            } else {
                ActionOutcome::Refused
            }
        }
        ClientAction::GiveKillstreak {
            request_id: _,
            name,
        } => {
            if !world.bootstrap_ref().allow_debug_actions
                || !world
                    .client_meta(id)
                    .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
            {
                return ActionOutcome::Refused;
            }
            let name = crate::menu_response_text(&name);
            if crate::script::give_killstreak(world.ecs(), id.0, name) {
                ActionOutcome::Accepted
            } else {
                ActionOutcome::Refused
            }
        }
        ClientAction::ForceDeath { request_id: _ } => {
            if !world.bootstrap_ref().allow_debug_actions
                || !world
                    .client_meta(id)
                    .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
            {
                return ActionOutcome::Refused;
            }
            crate::script::force_death(world.ecs(), tick, id.0);

            ActionOutcome::Applied
        }
        ClientAction::SetMatchPhase {
            request_id: _,
            phase,
        } => {
            if !world.bootstrap_ref().allow_debug_actions {
                return ActionOutcome::Refused;
            }
            if phase == MatchPhase::Playing && world.phase() == MatchPhase::Warmup {
                if crate::script::host::iw4_gametype::force_match_start(world.ecs(), tick) {
                    return ActionOutcome::Accepted;
                }
                crate::score::finish_prematch(world);
            } else {
                world.set_phase(phase);
            }

            ActionOutcome::Applied
        }
        ClientAction::Move {
            request_id: _,
            origin,
            angles,
        } => {
            return apply_debug_move(world, id, origin, angles);
        }
        ClientAction::BeginScriptMoverRotateVelocity {
            request_id: _,
            speed,
        } => {
            if !world.bootstrap_ref().allow_debug_actions || !speed.is_finite() {
                return ActionOutcome::Refused;
            }
            world.begin_script_movers_rotate_velocity_supplied(
                speed,
                crate::corpse::level_time_ms(tick),
            );

            ActionOutcome::Applied
        }
        ClientAction::ToggleGod { .. } => {
            if !world.bootstrap_ref().allow_debug_actions
                || !world
                    .client_meta(id)
                    .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
            {
                return ActionOutcome::Refused;
            }
            let meta = world.client_meta_mut(id);
            meta.god_mode = !meta.god_mode;
            diag::info!(
                Sim,
                "god: client={} {}",
                id.0,
                if meta.god_mode { "on" } else { "off" }
            );

            ActionOutcome::Applied
        }
        ClientAction::DebugDamage {
            request_id: _,
            amount,
        } => {
            return apply_debug_damage(world, tick, id, amount);
        }
    }
}

fn apply_force_spawn(
    world: &mut FrameWorld,
    id: ClientId,
    pick: crate::SpawnPick,
) -> ActionOutcome {
    if world
        .client_meta(id)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        crate::script_player::move_to_forced_spawn(world, id, pick);
        ActionOutcome::Applied
    } else {
        world.client_meta_mut(id).forced_spawn = Some(pick);
        ActionOutcome::Accepted
    }
}

fn apply_spawn_intermission(world: &mut FrameWorld, id: ClientId) -> ActionOutcome {
    let Some(meta) = world.client_meta(id) else {
        return ActionOutcome::Refused;
    };
    if meta.lifecycle == ClientLifecycle::Intermission {
        return ActionOutcome::Applied;
    }
    let view = world.bootstrap_ref().intermission_view.clone();
    {
        let meta = world.client_meta_mut(id);
        meta.lifecycle = ClientLifecycle::Intermission;
        meta.dead_since_tick = None;
    }
    world.unlink_player_area(id);
    let Some(view) = view else {
        return ActionOutcome::Applied;
    };
    let Some(ps) = world.player_mut(id) else {
        return ActionOutcome::Applied;
    };
    ps.origin = view.origin;
    ps.velocity = [0.0, 0.0, 0.0];
    ps.viewangles = view.angles;
    ps.delta_angles = packed_look_delta(view.angles);
    ps.e_flags ^= playerstate_iw4::eflags::TELEPORT;
    ActionOutcome::Applied
}

fn apply_debug_move(
    world: &mut FrameWorld,
    id: ClientId,
    origin: [f32; 3],
    angles: [f32; 3],
) -> ActionOutcome {
    if !world.bootstrap_ref().allow_debug_actions {
        return ActionOutcome::Refused;
    }
    if !world
        .client_meta(id)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        return ActionOutcome::Refused;
    }
    if !origin.iter().chain(angles.iter()).all(|v| v.is_finite()) {
        return ActionOutcome::Refused;
    }
    let old_origin = {
        let Some(ps) = world.player_mut(id) else {
            return ActionOutcome::Refused;
        };
        let old_origin = ps.origin;
        ps.origin = origin;
        ps.velocity = [0.0, 0.0, 0.0];
        ps.viewangles = angles;

        ps.e_flags ^= playerstate_iw4::eflags::TELEPORT;
        ps.delta_angles = packed_look_delta(angles);
        old_origin
    };
    world.translate_player_area(
        id,
        [
            origin[0] - old_origin[0],
            origin[1] - old_origin[1],
            origin[2] - old_origin[2],
        ],
    );
    ActionOutcome::Applied
}

fn packed_look_delta(viewangles: [f32; 3]) -> [f32; 3] {
    [
        viewangles[0] - (viewangles[0] * ANGLE2SHORT) as i32 as f32 * SHORT2ANGLE,
        viewangles[1] - (viewangles[1] * ANGLE2SHORT) as i32 as f32 * SHORT2ANGLE,
        viewangles[2] - (viewangles[2] * ANGLE2SHORT) as i32 as f32 * SHORT2ANGLE,
    ]
}

fn restamp_debug_move_look(world: &mut FrameWorld, actions: &[(ClientId, ClientAction)]) {
    for (id, action) in actions {
        let ClientAction::Move { angles, .. } = *action else {
            continue;
        };
        if !world.bootstrap_ref().allow_debug_actions {
            continue;
        }
        if !world
            .client_meta(*id)
            .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
        {
            continue;
        }
        let Some(ps) = world.player_mut(*id) else {
            continue;
        };
        ps.viewangles = angles;
        ps.delta_angles = packed_look_delta(angles);
    }
}

fn apply_debug_damage(
    world: &mut FrameWorld,
    tick: Tick,
    id: ClientId,
    amount: i32,
) -> ActionOutcome {
    if !world.bootstrap_ref().allow_debug_actions {
        return ActionOutcome::Refused;
    }
    if amount <= 0 {
        return ActionOutcome::Refused;
    }
    if !world
        .client_meta(id)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        return ActionOutcome::Refused;
    }
    crate::script_player::debug_damage(world, tick, id, amount);
    ActionOutcome::Applied
}

fn configuration_change_ammo(
    clip0: i32,
    clip1: i32,
    stock: i32,
    clip_capacity: i32,
    max_ammo: i32,
    dual: bool,
) -> (i32, i32, i32) {
    let total = clip0
        .max(0)
        .saturating_add(clip1.max(0))
        .saturating_add(stock.max(0));
    let cap = clip_capacity.max(0);
    let first = clip0.max(0).min(cap).min(total);
    let second = if dual {
        clip1.max(0).min(cap).min(total - first)
    } else {
        0
    };
    (first, second, (total - first - second).min(max_ammo.max(0)))
}

fn apply_configuration_change(
    world: &mut FrameWorld,
    tick: Tick,
    id: ClientId,
    request_id: u32,
    from: u32,
    to: u32,
) -> ActionOutcome {
    let reject = |world: &mut FrameWorld, reason: crate::ConfigurationChangeRejectReason| {
        world.push_event(
            tick,
            EventAudience::Client(id),
            SimEvent::ConfigurationChangeRejected {
                request_id,
                from,
                to,
                reason,
            },
        );
        ActionOutcome::Refused
    };
    use crate::ConfigurationChangeRejectReason as Reason;
    if !world
        .client_meta(id)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        return reject(world, Reason::NotAlive);
    }
    let Some(ps) = world.player(id).copied() else {
        return reject(world, Reason::NotAlive);
    };
    if ps.weapon != from {
        return reject(world, Reason::StaleSource);
    }
    if !world.can_transition_weapon(from, to) {
        return reject(world, Reason::DifferentFamily);
    }
    let (Some(old), Some(new)) = (world.combat_facts_for(from), world.combat_facts_for(to)) else {
        return reject(world, Reason::InvalidTarget);
    };
    if world
        .equipment_facts_for(to)
        .is_some_and(|eq| eq.is_offhand())
    {
        return reject(world, Reason::InvalidTarget);
    }
    if ps.weaponstate_primary != weapon_iw4::WeaponState::Ready as i32
        || ps.weapon_time > 0
        || (ps.last_weapon_hand >= 1
            && (ps.weaponstate_secondary != weapon_iw4::WeaponState::Ready as i32
                || ps.weapon_time_secondary > 0))
        || ps.weap_flags & playerstate_iw4::weap_flags::OFFHAND_VIEW != 0
    {
        return reject(world, Reason::Busy);
    }
    let Some(slot) = ps.weapons.iter().position(|&weapon| weapon == from as i32) else {
        return reject(world, Reason::NoInventorySlot);
    };
    if ps.weapons.contains(&(to as i32)) {
        return reject(world, Reason::InvalidTarget);
    }
    let old_ammo_key = weapon_iw4::ammo_table_key(old.ammo_index, from);
    let old_clip_key = weapon_iw4::clip_table_key(old.clip_index, from);
    let new_ammo_key = weapon_iw4::ammo_table_key(new.ammo_index, to);
    let new_clip_key = weapon_iw4::clip_table_key(new.clip_index, to);
    let another_owns = |key: i32, clip: bool| {
        ps.weapons
            .iter()
            .filter(|&&weapon| weapon > 0 && weapon != from as i32)
            .any(|&weapon| {
                world.combat_facts_for(weapon as u32).is_some_and(|facts| {
                    key == if clip {
                        weapon_iw4::clip_table_key(facts.clip_index, weapon as u32)
                    } else {
                        weapon_iw4::ammo_table_key(facts.ammo_index, weapon as u32)
                    }
                })
            })
    };
    let clip0 = weapon_iw4::get_clip_for_hand(&ps.ammoclip, old_clip_key, 0);
    let raw_clip1 = weapon_iw4::get_clip_for_hand(&ps.ammoclip, old_clip_key, 1);
    let clip1 = if ps.last_weapon_hand >= 1 {
        raw_clip1
    } else {
        0
    };
    let stock = weapon_iw4::get_ammo_not_in_clip(&ps.ammo, old_ammo_key);
    let akimbo = world
        .combat_facts_for(to)
        .is_some_and(|facts| facts.dual_wield)
        || gsc_give_weapon_is_akimbo(world.weapon_script_name(to));
    let (next_clip0, next_clip1, next_stock) =
        configuration_change_ammo(clip0, clip1, stock, new.clip_size, new.max_ammo, akimbo);
    if (old_ammo_key != new_ammo_key
        && (another_owns(old_ammo_key, false) || another_owns(new_ammo_key, false)))
        || (old_clip_key != new_clip_key
            && (another_owns(old_clip_key, true) || another_owns(new_clip_key, true)))
        || (old_ammo_key == new_ammo_key
            && next_stock != stock
            && another_owns(old_ammo_key, false))
        || (old_clip_key == new_clip_key
            && (next_clip0 != clip0 || next_clip1 != raw_clip1)
            && another_owns(old_clip_key, true))
    {
        return reject(world, Reason::SharedAmmoConflict);
    }
    let mut next = ps;
    next.weapons[slot] = to as i32;
    next.weapon_data[slot * 5..slot * 5 + 5].fill(0);
    weapon_iw4::latch_weapon_dual_wield(&next.weapons, &mut next.weapon_data, to, akimbo);
    next.weapon = to;
    next.weapon_primary = to;
    next.last_weapon_hand = weapon_iw4::num_hands_for_held(&next.weapons, &next.weapon_data, to);
    if old_ammo_key != new_ammo_key {
        for row in next.ammo.chunks_exact_mut(8) {
            if row[..4] == old_ammo_key.to_le_bytes() {
                row.fill(0);
            }
        }
    }
    if old_clip_key != new_clip_key {
        for row in next.ammoclip.chunks_exact_mut(12) {
            if row[..4] == old_clip_key.to_le_bytes() {
                row.fill(0);
            }
        }
    }
    if !weapon_iw4::set_ammo_not_in_clip(&mut next.ammo, new_ammo_key, next_stock)
        || !weapon_iw4::set_clip_for_hand(&mut next.ammoclip, new_clip_key, 0, next_clip0)
        || !weapon_iw4::set_clip_for_hand(&mut next.ammoclip, new_clip_key, 1, next_clip1)
    {
        return reject(world, Reason::AmmoTableFull);
    }
    let alternate_ammo = match (
        world.combat_facts_for(old.alternate_weapon),
        world.combat_facts_for(new.alternate_weapon),
    ) {
        (Some(old_alt), Some(new_alt))
            if old.alternate_weapon != 0
                && new.alternate_weapon != 0
                && old_alt.weap_type == new_alt.weap_type
                && old_alt.weap_class == new_alt.weap_class =>
        {
            let old_key = weapon_iw4::ammo_table_key(old_alt.ammo_index, old.alternate_weapon);
            let old_clip = weapon_iw4::clip_table_key(old_alt.clip_index, old.alternate_weapon);
            let new_key = weapon_iw4::ammo_table_key(new_alt.ammo_index, new.alternate_weapon);
            let new_clip = weapon_iw4::clip_table_key(new_alt.clip_index, new.alternate_weapon);
            if old_key != old_ammo_key
                && old_clip != old_clip_key
                && new_key != new_ammo_key
                && new_clip != new_clip_key
                && weapon_iw4::ammo_row_present(&ps.ammo, old_key)
                && weapon_iw4::clip_row_present(&ps.ammoclip, old_clip)
            {
                let shared = ps
                    .weapons
                    .iter()
                    .filter(|&&w| w > 0 && w != from as i32)
                    .any(|&w| {
                        let alt = world
                            .combat_facts_for(w as u32)
                            .map_or(0, |f| f.alternate_weapon);
                        [w as u32, alt].into_iter().filter(|&w| w != 0).any(|w| {
                            world.combat_facts_for(w).is_some_and(|f| {
                                let ammo = weapon_iw4::ammo_table_key(f.ammo_index, w);
                                let clip = weapon_iw4::clip_table_key(f.clip_index, w);
                                (old_key != new_key && (ammo == old_key || ammo == new_key))
                                    || (old_clip != new_clip
                                        && (clip == old_clip || clip == new_clip))
                            })
                        })
                    });
                if shared {
                    return reject(world, Reason::SharedAmmoConflict);
                }
                let (clip, _, stock) = configuration_change_ammo(
                    weapon_iw4::get_clip_for_hand(&ps.ammoclip, old_clip, 0),
                    0,
                    weapon_iw4::get_ammo_not_in_clip(&ps.ammo, old_key),
                    new_alt.clip_size,
                    new_alt.max_ammo,
                    false,
                );
                if old_key != new_key {
                    for row in next.ammo.chunks_exact_mut(8) {
                        if row[..4] == old_key.to_le_bytes() {
                            row.fill(0);
                        }
                    }
                }
                if old_clip != new_clip {
                    for row in next.ammoclip.chunks_exact_mut(12) {
                        if row[..4] == old_clip.to_le_bytes() {
                            row.fill(0);
                        }
                    }
                }
                if !weapon_iw4::set_ammo_not_in_clip(&mut next.ammo, new_key, stock)
                    || !weapon_iw4::set_clip_for_hand(&mut next.ammoclip, new_clip, 0, clip)
                {
                    return reject(world, Reason::AmmoTableFull);
                }
                Some((new.alternate_weapon, clip, stock))
            } else {
                None
            }
        }
        _ => None,
    };
    let hand = weapon_iw4::spawn_weapon_hand(to, &new, false);
    next.weaponstate_primary = hand.weaponstate;
    next.weapon_time = hand.weapon_time;
    next.weapon_delay = hand.weapon_delay;
    next.weap_anim = hand.weap_anim;
    next.weaponstate_secondary = hand.weaponstate;
    next.weapon_time_secondary = hand.weapon_time;
    next.weapon_delay_secondary = hand.weapon_delay;
    next.weap_anim_secondary = hand.weap_anim;
    next.weapon_shot_count = 0;
    next.weapon_shot_count_secondary = 0;
    next.weap_flags &= !(playerstate_iw4::weap_flags::NO_ADS
        | playerstate_iw4::weap_flags::DOUBLEBARREL_RECOIL
        | playerstate_iw4::weap_flags::RECOIL_SCALE);
    *world.player_mut(id).expect("validated alive player") = next;
    let meta = world.client_meta_mut(id);
    if let Some((alternate, clip, stock)) = alternate_ammo {
        meta.ammo_by_weapon
            .retain(|row| row.0 != old.alternate_weapon);
        meta.set_ammo(alternate, clip, stock);
    }
    let quick_reload_ready = meta.quick_reload_ready(from);
    meta.ammo_by_weapon.retain(|row| row.0 != from);
    meta.set_ammo(to, next_clip0, next_stock);
    meta.set_quick_reload_ready(from, true);
    meta.set_quick_reload_ready(to, quick_reload_ready);
    meta.mirror_held_ammo(to);
    meta.weapon_shot_count = 0;
    meta.burst_latch = false;
    meta.burst_latch_secondary = false;
    meta.rechamber_pending = false;
    meta.rechamber_pending_secondary = false;
    meta.pending_brass = [None; 2];
    world.push_event(
        tick,
        EventAudience::Client(id),
        SimEvent::ConfigurationChangeAccepted {
            request_id,
            from,
            to,
        },
    );
    ActionOutcome::Applied
}

fn apply_give_weapon(
    world: &mut FrameWorld,
    tick: Tick,
    id: ClientId,
    request_id: u32,
    weapon: u32,
) -> ActionOutcome {
    let reject = |world: &mut FrameWorld, reason: crate::GiveRejectReason| {
        world.push_event(
            tick,
            EventAudience::Client(id),
            SimEvent::GiveRejected {
                request_id,
                weapon,
                reason,
            },
        );
        ActionOutcome::Refused
    };

    if !world
        .client_meta(id)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        return reject(world, crate::GiveRejectReason::NotAlive);
    }
    if weapon == 0 {
        return reject(world, crate::GiveRejectReason::InvalidWeapon);
    }
    let table_len = world.weapon_combat_len();
    if (weapon as usize) >= table_len {
        return reject(world, crate::GiveRejectReason::UnknownWeaponId);
    }
    if !world.weapon_runnable(weapon) {
        return reject(world, crate::GiveRejectReason::UnsupportedWeapon);
    }
    let Some(facts) = world.combat_facts_for(weapon) else {
        return reject(world, crate::GiveRejectReason::UnknownWeaponId);
    };
    if world
        .equipment_facts_for(weapon)
        .is_some_and(|eq| eq.is_offhand())
    {
        return apply_give_offhand(world, tick, id, request_id, weapon, &facts);
    }
    if facts.fire_time_ms <= 0 && facts.raise_time_ms <= 0 {
        return reject(world, crate::GiveRejectReason::EmptyCombatProfile);
    }

    if world.weapon_script_name(weapon).starts_with("killstreak_") {
        let Some(ps) = world.player_mut(id) else {
            return reject(world, crate::GiveRejectReason::NotAlive);
        };
        inventory_add_weapon(ps, weapon, false);
        if !ps.weapons.contains(&(weapon as i32)) {
            return reject(world, crate::GiveRejectReason::InvalidWeapon);
        }
        let (clip, clip_alt, stock) = weapon_iw4::spawn_clip_stock(&facts, 0);
        seed_ps_ammo_tables(ps, weapon, &facts, clip, clip_alt, false, stock);
        world.client_meta_mut(id).set_ammo(weapon, clip, stock);
        world.push_event(
            tick,
            EventAudience::Client(id),
            SimEvent::GiveAccepted { request_id, weapon },
        );
        return ActionOutcome::Applied;
    }
    let akimbo = world
        .combat_facts_for(weapon)
        .is_some_and(|facts| facts.dual_wield)
        || gsc_give_weapon_is_akimbo(world.weapon_script_name(weapon));
    let Some(mut next) = world.player(id).copied() else {
        return reject(world, crate::GiveRejectReason::NotAlive);
    };

    let outgoing = if world
        .combat_facts_for(next.weapon)
        .is_some_and(|facts| facts.inventory_type == 3)
        && next.weapons.contains(&(next.weapon_primary as i32))
    {
        next.weapon_primary
    } else {
        next.weapon
    };
    let mut replaced = false;
    if !next.weapons.contains(&(weapon as i32)) {
        if let Some(slot) = next
            .weapons
            .iter()
            .position(|&w| w != 0 && w == outgoing as i32)
        {
            next.weapons[slot] = weapon as i32;
            next.weapon_data[slot * 5..slot * 5 + 5].fill(0);
            replaced = true;
        }
    }
    if replaced {
        let outgoing_facts = world
            .combat_facts_for(outgoing)
            .expect("held weapon combat facts");

        for (table, stride, clip) in [
            (&mut next.ammo[..], 8, false),
            (&mut next.ammoclip[..], 12, true),
        ] {
            let key = if clip {
                weapon_iw4::clip_table_key(outgoing_facts.clip_index, outgoing)
            } else {
                weapon_iw4::ammo_table_key(outgoing_facts.ammo_index, outgoing)
            };
            let owned = next.weapons.iter().filter(|&&w| w > 0).any(|&w| {
                world.combat_facts_for(w as u32).is_some_and(|f| {
                    key == if clip {
                        weapon_iw4::clip_table_key(f.clip_index, w as u32)
                    } else {
                        weapon_iw4::ammo_table_key(f.ammo_index, w as u32)
                    }
                })
            });
            if !owned {
                for row in table.chunks_exact_mut(stride) {
                    if row[..4] == key.to_le_bytes() {
                        row.fill(0);
                    }
                }
            }
        }
    }
    give_weapon_to_ps_akimbo(&mut next, weapon, akimbo);
    *world.player_mut(id).expect("validated alive player") = next;
    let (clip, stock) = if let Some(ps_mut) = world.player_mut(id) {
        arm_held_weapon(ps_mut, weapon, &facts)
    } else {
        (0, 0)
    };
    let meta = world.client_meta_mut(id);
    if replaced {
        meta.ammo_by_weapon.retain(|row| row.0 != outgoing);
        meta.set_quick_reload_ready(outgoing, true);
    }
    meta.set_ammo(weapon, clip, stock);
    meta.set_quick_reload_ready(weapon, true);
    meta.mirror_held_ammo(weapon);
    meta.weapon_shot_count = 0;
    meta.burst_latch = false;
    meta.burst_latch_secondary = false;
    meta.rechamber_pending = false;
    meta.rechamber_pending_secondary = false;
    meta.pending_brass = [None; 2];
    world.push_event(
        tick,
        EventAudience::Client(id),
        SimEvent::GiveAccepted { request_id, weapon },
    );
    ActionOutcome::Applied
}

fn apply_give_offhand(
    world: &mut FrameWorld,
    tick: Tick,
    id: ClientId,
    request_id: u32,
    weapon: u32,
    facts: &weapon_iw4::WeaponCombatFacts,
) -> ActionOutcome {
    let reject = |world: &mut FrameWorld, reason: crate::GiveRejectReason| {
        world.push_event(
            tick,
            EventAudience::Client(id),
            SimEvent::GiveRejected {
                request_id,
                weapon,
                reason,
            },
        );
        ActionOutcome::Refused
    };
    let Some(eq) = world.equipment_facts_for(weapon) else {
        return reject(world, crate::GiveRejectReason::InvalidWeapon);
    };
    let Some(mut next) = world.player(id).copied() else {
        return reject(world, crate::GiveRejectReason::NotAlive);
    };
    for slot in &mut next.weapons {
        if *slot > 0 && *slot != weapon as i32 {
            let same_slot = world
                .equipment_facts_for(*slot as u32)
                .is_some_and(|other| {
                    (matches!(eq.offhand_class, 1 | 4 | 5)
                        && matches!(other.offhand_class, 1 | 4 | 5))
                        || (matches!(eq.offhand_class, 2 | 3)
                            && matches!(other.offhand_class, 2 | 3))
                });
            if same_slot {
                *slot = 0;
            }
        }
    }
    inventory_add_weapon(&mut next, weapon, false);
    if !next.weapons.contains(&(weapon as i32)) {
        return reject(world, crate::GiveRejectReason::InvalidWeapon);
    }
    match eq.offhand_class {
        1 | 4 | 5 => next.offhand_primary = eq.offhand_class,
        2 | 3 => next.offhand_secondary = eq.offhand_class,
        _ => {}
    }
    let clip = eq.spawn_clip_count();
    seed_ps_ammo_tables(&mut next, weapon, facts, clip, 0, false, 0);
    *world.player_mut(id).expect("validated alive player") = next;
    {
        let meta = world.client_meta_mut(id);
        meta.ammo_by_weapon
            .retain(|(id, _, _)| next.weapons.contains(&(*id as i32)));
        meta.set_ammo(weapon, clip, 0);
        if let Some(loadout) = meta.loadout.as_mut() {
            match eq.offhand_class {
                1 | 4 | 5 => loadout.lethal = weapon,
                2 | 3 => loadout.tactical = weapon,
                _ => {}
            }
        }
    }
    world.push_event(
        tick,
        EventAudience::Client(id),
        SimEvent::GiveAccepted { request_id, weapon },
    );
    ActionOutcome::Applied
}

fn choose_bot_class(world: &mut FrameWorld, id: ClientId, index: u8) {
    let t5 = crate::script::is_t5(world.ecs());
    let bot_classes = &world.bootstrap_ref().bot_classes;
    let def = (!bot_classes.is_empty() && !t5)
        .then(|| bot_classes[index as usize % bot_classes.len()].clone())
        .filter(|def| !def.locked && validate_class_content(world, def).is_ok());
    match def {
        Some(def) => crate::script::choose_class(world.ecs(), id.0, &def),
        None => crate::script::choose_default_class(world.ecs(), id.0, index),
    }
}

fn answer_custom_class(
    world: &mut FrameWorld,
    id: ClientId,
    menu: &str,
    response: &str,
) -> Option<ActionOutcome> {
    if !menu.eq_ignore_ascii_case("changeclass") {
        return None;
    }
    let Some(slot) = response
        .strip_prefix("custom")
        .and_then(|n| n.parse::<u32>().ok())
        .and_then(|n| n.checked_sub(1))
    else {
        return None;
    };
    let Some(def) = crate::script::personal_class(world.ecs(), id.0, crate::ClassId(slot)) else {
        return Some(ActionOutcome::Refused);
    };
    if validate_class_content(world, &def).is_err() {
        return Some(ActionOutcome::Refused);
    }
    crate::script::choose_class(world.ecs(), id.0, &def);
    Some(ActionOutcome::Accepted)
}

fn apply_select_class(
    world: &mut FrameWorld,
    tick: Tick,
    id: ClientId,
    request_id: u32,
    class_id: crate::ClassId,
    revision: u32,
    loadout: crate::PersonalClass,
) -> ActionOutcome {
    let accepted = (class_id.0 < crate::match_state::PERSONAL_CLASS_SLOTS as u32)
        .then(|| loadout.definition(class_id, revision))
        .flatten();
    let Some(def) = accepted else {
        diag::info!(
            Sim,
            "class select: rejected client={} id={} rev={} reason=unknown_or_stale_class",
            id.0,
            class_id.0,
            revision
        );
        world.push_event(
            tick,
            EventAudience::Client(id),
            SimEvent::ClassRejected {
                request_id,
                class_id,
                revision,
                reason: crate::ClassRejectReason::UnknownOrStaleClass,
            },
        );
        return ActionOutcome::Refused;
    };

    if let Err(reason) = validate_class_content(world, &def) {
        diag::info!(
            Sim,
            "class select: rejected client={} id={} rev={} reason={}",
            id.0,
            class_id.0,
            revision,
            reason.as_str()
        );
        world.push_event(
            tick,
            EventAudience::Client(id),
            SimEvent::ClassRejected {
                request_id,
                class_id,
                revision,
                reason,
            },
        );
        return ActionOutcome::Refused;
    }

    crate::script::answer_join(world.ecs(), id.0);
    crate::script::choose_class(world.ecs(), id.0, &def);
    world.push_event(
        tick,
        EventAudience::Client(id),
        SimEvent::ClassAccepted {
            request_id,
            class_id,
            revision,
        },
    );
    ActionOutcome::Accepted
}

fn validate_class_content(
    world: &FrameWorld,
    def: &crate::ClassDef,
) -> Result<(), crate::ClassRejectReason> {
    for (slot, perk) in def.perks.iter().copied().enumerate() {
        if perk != 0 && crate::match_state::perk_slot_from_class_catalog(perk) != Some(slot) {
            return Err(crate::ClassRejectReason::LockedContent);
        }
    }
    let max_attachments = if def.perks[0] == 12 { 2 } else { 1 };
    let table_len = world.weapon_combat_len();
    for id in [def.primary, def.secondary] {
        if id == 0 {
            continue;
        }
        if (id as usize) >= table_len {
            return Err(crate::ClassRejectReason::UnknownWeaponId);
        }
        let Some(row) = world.weapon_combat_row(id) else {
            return Err(crate::ClassRejectReason::UnknownWeaponId);
        };
        if !row.is_usable()
            || !world.weapon_runnable(id)
            || world
                .weapon_setup(id)
                .is_some_and(|setup| setup.attachments.len() > max_attachments)
        {
            return Err(crate::ClassRejectReason::LockedContent);
        }
    }
    for (slot, id) in [def.lethal, def.tactical].into_iter().enumerate() {
        if id == 0 {
            continue;
        }
        if (id as usize) >= table_len {
            return Err(crate::ClassRejectReason::UnknownWeaponId);
        }
        let Some(facts) = world.offhand_loadout_row(id) else {
            return Err(crate::ClassRejectReason::LockedContent);
        };
        if !world.weapon_runnable(id)
            || !matches!((slot, facts.offhand_class), (0, 1 | 4 | 5) | (1, 2 | 3))
        {
            return Err(crate::ClassRejectReason::LockedContent);
        }
    }

    if def.primary_attachments.iter().any(|&a| a != 0)
        || def.secondary_attachments.iter().any(|&a| a != 0)
    {
        return Err(crate::ClassRejectReason::LockedContent);
    }

    Ok(())
}

pub(crate) fn log_forced_spawn(
    id: ClientId,
    pick: crate::SpawnPick,
    report: &crate::spawn::SpawnAttemptReport,
) {
    let asked = match pick {
        crate::SpawnPick::Seeded(seed) => format!("seed={seed}"),
        crate::SpawnPick::At { origin, yaw } => format!(
            "at=[{:.1}, {:.1}, {:.1}] yaw={yaw:.1}",
            origin[0], origin[1], origin[2]
        ),
    };
    let fallback = report
        .rejected
        .first()
        .filter(|(source, _)| *source == crate::spawn::FORCED_SPAWN_SOURCE)
        .map(|(_, reason)| format!(" fallback={}", reason.as_str()))
        .unwrap_or_default();
    match &report.accepted {
        Some(d) => diag::info!(
            Sim,
            "spawn: forced client={} {asked} origin=[{:.1}, {:.1}, {:.1}] yaw={:.1} source={} tried={}{fallback}",
            id.0,
            d.traced_origin[0],
            d.traced_origin[1],
            d.traced_origin[2],
            d.raw_angles[1],
            if d.source_index == crate::spawn::FORCED_SPAWN_SOURCE {
                String::from("at")
            } else {
                d.source_index.to_string()
            },
            report.tried
        ),
        None => diag::info!(
            Sim,
            "spawn: forced client={} {asked} refused tried={}{fallback}",
            id.0,
            report.tried
        ),
    }
}

struct PlayerBodyClip {
    entnum: u16,
    origin: [f32; 3],
}

fn alive_body_clips(world: &FrameWorld) -> Vec<PlayerBodyClip> {
    let mut bodies: Vec<_> = world
        .client_ids_sorted()
        .into_iter()
        .filter(|id| {
            world
                .client_meta(*id)
                .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
        })
        .filter_map(|id| {
            world.player(id).map(|ps| PlayerBodyClip {
                entnum: id.0 as u16,
                origin: ps.origin,
            })
        })
        .collect();
    bodies.extend(
        world
            .prediction_remote_bodies()
            .iter()
            .map(|body| PlayerBodyClip {
                entnum: body.client.0 as u16,
                origin: body.origin,
            }),
    );
    bodies
}

#[derive(Clone, Copy)]
struct ClipBackend<'a> {
    brushes: &'a [SimBrush],
    bsp: &'a SimClipBsp,
    mesh: &'a SimClipMesh,
    glass_damage: &'a [(u32, u16)],
    bodies: &'a [PlayerBodyClip],
    self_entnum: u16,
    cmodels: &'a [clipmap_iw4::ClipCmodel],
    linked_brushes: &'a [LinkedBrushCollisionBrush],
    model_brushes: &'a [SimBrush],
}

impl CollisionBackend for ClipBackend<'_> {
    fn penetrations(
        &self,
        input: GroundTraceInput,
        contacts: &mut movement_iw4::recovery::ContactBuffer,
    ) -> movement_iw4::recovery::Coverage {
        use movement_iw4::recovery::Coverage;
        let glass = |piece| {
            let damage = self
                .glass_damage
                .iter()
                .find(|(id, _)| *id == u32::from(piece))
                .map_or(0, |(_, d)| *d);
            crate::world_objects::glass_piece_is_solid(damage)
        };
        let scene = crate::penetration::RecoveryScene {
            brushes: self.brushes,
            bsp: self.bsp,
            mesh: self.mesh,
            cmodels: self.cmodels,
            linked: self.linked_brushes,
            models: self.model_brushes,
            glass_is_solid: &glass,
        };
        let status = crate::penetration::contacts(&scene, input, contacts);
        if status != Coverage::Complete {
            return status;
        }
        if input.tracemask & crate::world::CONTENTS_BODY == 0 {
            return status;
        }
        let Some(moving) = crate::penetration::capsule(input, true) else {
            return Coverage::Unsupported;
        };
        for body in self
            .bodies
            .iter()
            .filter(|body| body.entnum != self.self_entnum)
        {
            let body_input = GroundTraceInput {
                start: body.origin,
                end: body.origin,
                mins: PLAYER_MINS,
                maxs: PLAYER_MAXS,
                tracemask: input.tracemask,
            };
            let Some(fixed) = crate::penetration::capsule(body_input, true) else {
                return Coverage::Unsupported;
            };
            if let Err(status) = crate::penetration::append(
                movement_iw4::penetration::capsule_capsule(moving, fixed),
                contacts,
            ) {
                return status;
            }
        }
        Coverage::Complete
    }

    fn trace(&self, input: GroundTraceInput) -> Trace {
        let world_hit = clip_trace(
            self.brushes,
            self.bsp,
            self.mesh,
            input.start,
            input.end,
            input.mins,
            input.maxs,
            input.tracemask,
            &|piece| {
                let damage = self
                    .glass_damage
                    .iter()
                    .find(|(id, _)| *id == u32::from(piece))
                    .map_or(0, |(_, d)| *d);
                crate::world_objects::glass_piece_is_solid(damage)
            },
        );
        let with_bmodels = clip_move_to_bmodels(
            world_hit,
            self.cmodels,
            &self.bsp.leafbrushes,
            self.brushes,
            self.linked_brushes,
            input,
        );
        let with_models = clip_move_to_model_brushes(with_bmodels, self.model_brushes, input);
        clip_move_to_players(with_models, self.bodies, self.self_entnum, input)
    }
}

impl movement_iw4::MantleCapsuleTrace for ClipBackend<'_> {
    fn trace(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        contentmask: u32,
    ) -> Trace {
        CollisionBackend::trace(
            self,
            GroundTraceInput {
                start,
                end,
                mins,
                maxs,
                tracemask: contentmask,
            },
        )
    }
}

pub(crate) fn script_slide(
    world: &mut World,
    object: u64,
    origin: [f32; 3],
    slide: &mut crate::script::host::mechanics::Slide,
) -> [f32; 3] {
    let runtime = world.resource::<crate::script::Runtime>();
    let linked = |mut child: u64| {
        for _ in 0..runtime.entities.len() {
            if child == object {
                return true;
            }
            let parent = runtime
                .player_client(child)
                .and_then(|client| runtime.players.get(&client))
                .and_then(|player| player.link.as_ref().map(|link| link.parent))
                .or_else(|| {
                    runtime
                        .entities
                        .get(&child)
                        .and_then(|entity| entity.linked_to.as_ref().map(|link| link.parent))
                });
            let Some(parent) = parent else {
                return false;
            };
            child = parent;
        }
        false
    };
    let excluded_models: Vec<_> = runtime
        .entities
        .iter()
        .filter(|(id, _)| linked(**id))
        .filter_map(|(_, entity)| entity.presence)
        .collect();
    let excluded_clients: Vec<_> = runtime
        .players
        .iter()
        .filter(|(_, player)| linked(player.object))
        .map(|(client, _)| *client as u16)
        .collect();
    let mut mask = crate::bullet_collision::MASK_PLAYER_SOLID;
    let mut parent = runtime
        .entities
        .get(&object)
        .and_then(|entity| entity.linked_to.as_ref().map(|link| link.parent));
    for _ in 0..runtime.entities.len() {
        let Some(entity) = parent.and_then(|parent| runtime.entities.get(&parent)) else {
            break;
        };
        mask &= !(entity.contents as u32);
        parent = entity.linked_to.as_ref().map(|link| link.parent);
    }
    let frame = FrameWorld::from_world(world);
    let content = frame.content();
    let linked_brushes: Vec<_> = frame
        .entity_collision_capabilities()
        .iter()
        .filter(|row| {
            !row.owner
                .script_model()
                .is_some_and(|model| excluded_models.contains(&model))
        })
        .flat_map(|row| row.solid_brushes().iter().cloned())
        .collect();
    let model_brushes = frame.model_movement_brushes_where(|row| {
        !row.owner
            .script_model()
            .is_some_and(|model| excluded_models.contains(&model))
    });
    let mut bodies = alive_body_clips(&frame);
    bodies.retain(|body| !excluded_clients.contains(&body.entnum));
    let glass_damage = frame.world_objects().glass_damage_pairs();
    let backend = ClipBackend {
        brushes: content.clip_brushes(),
        bsp: content.clip_bsp(),
        mesh: content.clip_mesh(),
        glass_damage: &glass_damage,
        bodies: &bodies,
        self_entnum: playerstate_iw4::ENTITYNUM_NONE as u16,
        cmodels: &content.clip_cmodels().models,
        linked_brushes: &linked_brushes,
        model_brushes: &model_brushes,
    };
    slide.advance(origin, &backend, mask)
}

pub(crate) fn script_mantle(
    world: &mut FrameWorld,
    id: ClientId,
    force: bool,
) -> Result<bool, String> {
    let Some(meta) = world.client_meta(id) else {
        return Err("player has disconnected".into());
    };
    if meta.lifecycle != ClientLifecycle::Alive || meta.controls.linked {
        return Ok(false);
    }
    let mut ps = *world.player(id).ok_or("player has not spawned")?;
    if ps
        .origin
        .iter()
        .chain(ps.viewangles.iter())
        .any(|v| !v.is_finite())
    {
        return Err("mantle pose must be finite".into());
    }
    let content = world.content();
    let linked_brushes: Vec<_> = world
        .entity_collision_capabilities()
        .iter()
        .flat_map(|c| c.solid_brushes().iter().cloned())
        .collect();
    let model_brushes = world.model_movement_brushes();
    let bodies = alive_body_clips(world);
    let glass_damage = world.world_objects().glass_damage_pairs();
    let mut backend = ClipBackend {
        brushes: content.clip_brushes(),
        bsp: content.clip_bsp(),
        mesh: content.clip_mesh(),
        glass_damage: &glass_damage,
        bodies: &bodies,
        self_entnum: id.0 as u16,
        cmodels: &content.clip_cmodels().models,
        linked_brushes: &linked_brushes,
        model_brushes: &model_brushes,
    };
    let forward = math_iw4::angle_vectors(ps.viewangles).0;
    let mantle = world.xanims();
    let available = movement_iw4::mantle::check(
        &mut ps,
        movement_iw4::MantleCheckContext {
            find: movement_iw4::MantleFindLedgeContext::default(),
            buttons: buttons::JUMP,
            forwardmove: 0,
            facing_xy: [forward[0], forward[1]],
            tracemask: crate::bullet_collision::MASK_PLAYER_SOLID,
        },
        &mut backend,
        mantle.as_ref(),
        mantle.as_ref(),
    );
    if available && force {
        *world.player_mut(id).expect("checked player") = ps;
        world.link_player_standing_area(id);
    }
    Ok(available)
}

fn clip_move_to_players(
    mut hit: Trace,
    bodies: &[PlayerBodyClip],
    self_entnum: u16,
    input: GroundTraceInput,
) -> Trace {
    if hit.fraction == 0.0 {
        return hit;
    }
    for body in bodies {
        if body.entnum == self_entnum {
            continue;
        }
        let other = clipmap_iw4::transformed_temp_capsule_trace(
            input.start,
            input.end,
            input.mins,
            input.maxs,
            body.origin,
            PLAYER_MINS,
            PLAYER_MAXS,
            crate::world::CONTENTS_BODY,
            input.tracemask,
        );
        if other.fraction >= hit.fraction && other.startsolid == 0 && other.allsolid == 0 {
            continue;
        }
        let startsolid = hit.startsolid | other.startsolid;
        let allsolid = hit.allsolid | other.allsolid;
        if other.fraction < hit.fraction {
            hit = other;
            hit.hit_type = HITTYPE_ENTITY;
            hit.hit_id = body.entnum;
        }
        hit.startsolid = startsolid;
        hit.allsolid = allsolid;
    }
    hit
}

fn pmove_context(
    old_buttons: u32,
    weapon_scales: (f32, f32, f32),
    ads_allowed: bool,
    aim_down_sight: bool,
    ads_in_rate: f32,
    ads_out_rate: f32,
    ads_reload_trans_time_ms: i32,
    segmented_reload: bool,
    rechamber_while_ads: bool,
    ads_fire_only: bool,
    melee_delay_ms: i32,
    melee_charge_delay_ms: i32,
    overlay_reticle: i32,
    can_hold_breath: bool,
    shellshock_affects_movement: bool,
) -> PmoveSingleContext {
    let player_sprint_time = 4.0_f32;
    let weapon_max_sprint_time = get_max_sprint_time(weapon_scales.2, player_sprint_time);

    let air = AirMoveContext {
        player_spectate_speed_scale: 1.0,
        shellshock_gravity_scale: 1.0,
        shellshock_gravity_bias: 0.0,
    };
    PmoveSingleContext {
        walk: WalkMoveContext {
            cmd_scale: CmdScaleWalkContext {
                player_back_speed_scale: 0.7,
                player_strafe_speed_scale: 0.8,
                player_sprint_speed_scale: 1.5,
                player_last_stand_crawl_speed_scale: 0.15,
                weapon_move_speed_scale: weapon_scales.0,
                weapon_ads_move_speed_scale: weapon_scales.1,

                shellshock_affects_movement,
            },
            weapon_move_scale: 1.0,
            old_buttons,
            jump: JumpLaunchContext {
                jump_height: 39.0,
                dive: false,
                crouch_jump_scale: 1.0,
                jump_ladder_push_vel: 128.0,
            },
            air,
        },
        air,
        bounds: MoveBounds {
            mins: [-15.0, -15.0, 0.0],
            maxs: [15.0, 15.0, 70.0],
            tracemask: 0x0281_0011,
        },
        view_angles: ViewAngleClamp {
            pitch_up: 85.0,
            pitch_down: 85.0,
            unclamped_pitch_bit: false,
        },
        sprint: SprintContext {
            weapon_max_sprint_time,
            sprint_forever: false,
            min_sprint_time_seconds: 1.0,
            sprint_delay_seconds: 0.0,

            sprint_forward_minimum: 105,
            stand_up_clear: true,
            sprint_recharge_pause_seconds: 0.0,
        },
        ads_intent: movement_iw4::AdsIntentContext {
            ads_allowed,
            weapon_def_scope: overlay_reticle != 0,
            sprint_hold_ads: false,
        },
        ads_frac: movement_iw4::AdsFracContext {
            aim_down_sight,
            ads_in_rate,
            ads_out_rate,
            ads_reload_trans_time_ms,
            segmented_reload,
            rechamber_while_ads,
            ads_fire_only,
        },
        melee_charge: movement_iw4::MeleeChargeWeaponDelays {
            melee_delay_ms,
            melee_charge_delay_ms,
        },
        player_melee_range: movement_iw4::MELEE_CHARGE_PLAYER_MELEE_RANGE_DEFAULT,
        old_buttons,
        weapon_blocks_prone: false,
        can_hold_breath,
    }
}

pub(crate) fn seed_ps_ammo_tables(
    ps: &mut PlayerState,
    weapon: u32,
    facts: &weapon_iw4::WeaponCombatFacts,
    clip: i32,
    clip_alt: i32,
    dual: bool,
    stock: i32,
) {
    let ammo_index = weapon_iw4::ammo_table_key(facts.ammo_index, weapon);
    let clip_index = weapon_iw4::clip_table_key(facts.clip_index, weapon);
    if ammo_index != 0 {
        let _ = weapon_iw4::set_ammo_not_in_clip(&mut ps.ammo, ammo_index, stock);
    }
    if clip_index != 0 {
        let _ = weapon_iw4::set_clip_for_hand(&mut ps.ammoclip, clip_index, 0, clip);
        if dual {
            let _ = weapon_iw4::set_clip_for_hand(&mut ps.ammoclip, clip_index, 1, clip_alt);
        }
    }
}

pub(crate) fn arm_held_weapon(
    ps: &mut PlayerState,
    weapon: u32,
    facts: &weapon_iw4::WeaponCombatFacts,
) -> (i32, i32) {
    ps.weapon_primary = weapon;
    let last_hand = weapon_iw4::num_hands_for_held(&ps.weapons, &ps.weapon_data, weapon);
    ps.last_weapon_hand = last_hand;
    let (clip0, clip1, stock) = weapon_iw4::spawn_clip_stock(facts, last_hand);
    let hand = weapon_iw4::spawn_weapon_hand(weapon, facts, true);
    crate::combat::raise_given_weapon(ps, weapon, &hand);
    ps.weaponstate_primary = hand.weaponstate;
    ps.weapon_time = hand.weapon_time;
    ps.weapon_delay = hand.weapon_delay;
    ps.weap_anim = hand.weap_anim;
    if last_hand >= 1 {
        ps.weaponstate_secondary = hand.weaponstate;
        ps.weapon_time_secondary = hand.weapon_time;
        ps.weapon_delay_secondary = hand.weapon_delay;
        ps.weap_anim_secondary = hand.weap_anim;
    }
    seed_ps_ammo_tables(ps, weapon, facts, clip0, clip1, last_hand >= 1, stock);
    (clip0, stock)
}
