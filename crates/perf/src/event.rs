use std::borrow::Cow;

use perfetto_sdk::track_event::{EventContext, TrackEventDebugArg};
use perfetto_sdk::{track_event_begin, track_event_end};

#[allow(unused_imports)]
use crate::vocabulary::perfetto_te_ns;

fn i64_arg(ctx: &mut EventContext, name: &'static str, value: i64) {
    ctx.add_debug_arg(name, TrackEventDebugArg::Int64(value));
}

fn f64_arg(ctx: &mut EventContext, name: &'static str, value: f32) {
    ctx.add_debug_arg(name, TrackEventDebugArg::Double(f64::from(value)));
}

fn str_arg(ctx: &mut EventContext, name: &'static str, value: &str) {
    // Perfetto debug strings require an interior-NUL-free C string.
    let value = if value.contains('\0') {
        Cow::Owned(value.replace('\0', "\\0"))
    } else {
        Cow::Borrowed(value)
    };
    ctx.add_debug_arg(name, TrackEventDebugArg::String(&value));
}

pub fn player_tick(
    client_id: u32,
    is_bot: bool,
    time_ms: i32,
    level_time_ms: i32,
    origin: [f32; 3],
    vz: f32,
    yaw: f32,
    pitch: f32,
    jump_time: i32,
    buttons: u32,
    weaponstate: i32,
    ammo_clip: i32,
    walking: Option<i32>,
    legs_anim: i32,
    torso_anim: i32,
    lifecycle: &'static str,
) {
    track_event_begin!("iw4l.sim", "player_tick", |ctx: &mut EventContext| {
        i64_arg(ctx, "client_id", i64::from(client_id));
        i64_arg(ctx, "is_bot", i64::from(u8::from(is_bot)));
        i64_arg(ctx, "time_ms", i64::from(time_ms));
        i64_arg(ctx, "level_time_ms", i64::from(level_time_ms));
        f64_arg(ctx, "origin_x", origin[0]);
        f64_arg(ctx, "origin_y", origin[1]);
        f64_arg(ctx, "origin_z", origin[2]);
        f64_arg(ctx, "vz", vz);
        f64_arg(ctx, "yaw", yaw);
        f64_arg(ctx, "pitch", pitch);
        i64_arg(ctx, "jump_time", i64::from(jump_time));
        i64_arg(ctx, "buttons", i64::from(buttons));
        i64_arg(ctx, "weaponstate", i64::from(weaponstate));
        i64_arg(ctx, "ammo_clip", i64::from(ammo_clip));
        if let Some(walking) = walking {
            i64_arg(ctx, "walking", i64::from(walking));
        }
        i64_arg(ctx, "legs_anim", i64::from(legs_anim));
        i64_arg(ctx, "torso_anim", i64::from(torso_anim));
        str_arg(ctx, "lifecycle", lifecycle);
    });
    track_event_end!("iw4l.sim");
}

pub fn death(victim: u32, attacker: Option<u32>, suicide: u8, tick: u32) {
    track_event_begin!("iw4l.sim", "death", |ctx: &mut EventContext| {
        i64_arg(ctx, "victim", i64::from(victim));
        if let Some(attacker) = attacker {
            i64_arg(ctx, "attacker", i64::from(attacker));
        }
        i64_arg(ctx, "suicide", i64::from(suicide));
        i64_arg(ctx, "tick", i64::from(tick));
    });
    track_event_end!("iw4l.sim");
}

pub fn owner_shot(source: &str, ordinal: u32, tick: u32) {
    track_event_begin!("iw4l.sim", "owner_shot", |ctx: &mut EventContext| {
        str_arg(ctx, "source", source);
        i64_arg(ctx, "ordinal", i64::from(ordinal));
        i64_arg(ctx, "tick", i64::from(tick));
    });
    track_event_end!("iw4l.sim");
}

pub fn net_leg(leg: &str, key: u32, bytes: u32) {
    track_event_begin!("iw4l.sim", "net_leg", |ctx: &mut EventContext| {
        str_arg(ctx, "leg", leg);
        i64_arg(ctx, "key", i64::from(key));
        i64_arg(ctx, "bytes", i64::from(bytes));
    });
    track_event_end!("iw4l.sim");
}

pub fn net_path(role: &str, rtt_us: u64, cwnd: u64, lost: u64, congestion: u64, queued: u64) {
    track_event_begin!("iw4l.sim", "net_path", |ctx: &mut EventContext| {
        str_arg(ctx, "role", role);
        i64_arg(ctx, "rtt_us", rtt_us as i64);
        i64_arg(ctx, "cwnd", cwnd as i64);
        i64_arg(ctx, "lost", lost as i64);
        i64_arg(ctx, "congestion", congestion as i64);
        i64_arg(ctx, "queued", queued as i64);
    });
    track_event_end!("iw4l.sim");
}

pub fn projectile_phase(weapon: u32, projectile: u32, phase: &str, deadline_ms: i32) {
    track_event_begin!("iw4l.sim", "projectile_phase", |ctx: &mut EventContext| {
        i64_arg(ctx, "weapon", i64::from(weapon));
        i64_arg(ctx, "projectile", i64::from(projectile));
        str_arg(ctx, "phase", phase);
        i64_arg(ctx, "deadline_ms", i64::from(deadline_ms));
    });
    track_event_end!("iw4l.sim");
}

pub fn projectile(weapon: u32) {
    track_event_begin!("iw4l.sim", "projectile", |ctx: &mut EventContext| {
        i64_arg(ctx, "weapon", i64::from(weapon));
        str_arg(ctx, "weapon_kind", "missile");
    });
    track_event_end!("iw4l.sim");
}

pub fn truck(
    script_model_id: u32,
    state: Option<i64>,
    health: Option<i64>,
    death_clip: Option<&str>,
    present_gap: Option<&str>,
) {
    track_event_begin!("iw4l.sim", "truck", |ctx: &mut EventContext| {
        i64_arg(ctx, "script_model_id", i64::from(script_model_id));
        if let Some(state) = state {
            i64_arg(ctx, "state", state);
        }
        if let Some(health) = health {
            i64_arg(ctx, "health", health);
        }
        if let Some(death_clip) = death_clip {
            str_arg(ctx, "death_clip", death_clip);
        }
        if let Some(present_gap) = present_gap {
            str_arg(ctx, "present_gap", present_gap);
        }
    });
    track_event_end!("iw4l.sim");
}

pub fn feel(
    time_ms: i32,
    lifecycle: Option<&str>,
    fanout_seat_applied: Option<i32>,
    seat_lookup_tick: Option<i32>,
    present_choice: Option<&str>,
    present_snapshot_delta_time: Option<i32>,
    authority_origin: Option<[f32; 3]>,
    adopted_origin: Option<[f32; 3]>,
    client_clock_debt_ms: Option<f32>,
    ingress_queue_depth: Option<i64>,
    authority_command_time: Option<i32>,
    predicted_command_time: Option<i32>,
) {
    track_event_begin!("iw4l.sim", "feel", |ctx: &mut EventContext| {
        i64_arg(ctx, "time_ms", i64::from(time_ms));
        if let Some(lifecycle) = lifecycle {
            str_arg(ctx, "lifecycle", lifecycle);
        }
        if let Some(applied) = fanout_seat_applied {
            i64_arg(ctx, "fanout_seat_applied", i64::from(applied));
        }
        if let Some(tick) = seat_lookup_tick {
            i64_arg(ctx, "seat_lookup_tick", i64::from(tick));
        }
        if let Some(choice) = present_choice {
            str_arg(ctx, "present_choice", choice);
        }
        if let Some(dt) = present_snapshot_delta_time {
            i64_arg(ctx, "present_snapshot_delta_time", i64::from(dt));
        }
        if let Some(o) = authority_origin {
            f64_arg(ctx, "authority_origin_x", o[0]);
            f64_arg(ctx, "authority_origin_y", o[1]);
            f64_arg(ctx, "authority_origin_z", o[2]);
        }
        if let Some(o) = adopted_origin {
            f64_arg(ctx, "adopted_origin_x", o[0]);
            f64_arg(ctx, "adopted_origin_y", o[1]);
            f64_arg(ctx, "adopted_origin_z", o[2]);
        }
        if let Some(debt) = client_clock_debt_ms {
            f64_arg(ctx, "client_clock_debt_ms", debt);
        }
        if let Some(q) = ingress_queue_depth {
            i64_arg(ctx, "ingress_queue_depth", q);
        }
        if let Some(t) = authority_command_time {
            i64_arg(ctx, "authority_command_time", i64::from(t));
        }
        if let Some(t) = predicted_command_time {
            i64_arg(ctx, "predicted_command_time", i64::from(t));
        }
    });
    track_event_end!("iw4l.sim");
}

pub fn corpse(
    occupied: i64,
    origin_z: Option<f32>,
    pose_e_type: Option<i64>,
    legs_leaf_name: Option<&str>,
) {
    track_event_begin!("iw4l.sim", "corpse", |ctx: &mut EventContext| {
        i64_arg(ctx, "occupied", occupied);
        if let Some(z) = origin_z {
            f64_arg(ctx, "origin_z", z);
        }
        if let Some(e_type) = pose_e_type {
            i64_arg(ctx, "pose_e_type", e_type);
        }
        if let Some(name) = legs_leaf_name {
            str_arg(ctx, "legs_leaf_name", name);
        }
    });
    track_event_end!("iw4l.sim");
}

pub fn item(e_type: i32, clip_r: Option<i32>, scavenger: Option<i32>, present_gap: Option<&str>) {
    track_event_begin!("iw4l.sim", "item", |ctx: &mut EventContext| {
        i64_arg(ctx, "e_type", i64::from(e_type));
        if let Some(clip_r) = clip_r {
            i64_arg(ctx, "clip_r", i64::from(clip_r));
        }
        if let Some(scavenger) = scavenger {
            i64_arg(ctx, "scavenger", i64::from(scavenger));
        }
        if let Some(gap) = present_gap {
            str_arg(ctx, "present_gap", gap);
        }
    });
    track_event_end!("iw4l.sim");
}

pub fn pickup(picker_pm_type: i32) {
    track_event_begin!("iw4l.sim", "pickup", |ctx: &mut EventContext| {
        i64_arg(ctx, "picker_pm_type", i64::from(picker_pm_type));
    });
    track_event_end!("iw4l.sim");
}

pub fn pickup_rejected(picker: u32, item: i32, pm_type: i32) {
    track_event_begin!("iw4l.sim", "pickup_rejected", |ctx: &mut EventContext| {
        i64_arg(ctx, "picker", i64::from(picker));
        i64_arg(ctx, "item", i64::from(item));
        i64_arg(ctx, "picker_pm_type", i64::from(pm_type));
    });
    track_event_end!("iw4l.sim");
}

pub fn remote(
    client: u32,
    time_ms: i32,
    pose_e_type: i32,
    origin: [f32; 3],
    snap_origin: Option<[f32; 3]>,
    proxy_outcome: Option<&str>,
) {
    track_event_begin!("iw4l.render", "remote", |ctx: &mut EventContext| {
        i64_arg(ctx, "client", i64::from(client));
        i64_arg(ctx, "time_ms", i64::from(time_ms));
        i64_arg(ctx, "pose_e_type", i64::from(pose_e_type));
        f64_arg(ctx, "origin_x", origin[0]);
        f64_arg(ctx, "origin_y", origin[1]);
        f64_arg(ctx, "origin_z", origin[2]);
        if let Some(o) = snap_origin {
            f64_arg(ctx, "snap_origin_x", o[0]);
            f64_arg(ctx, "snap_origin_y", o[1]);
            f64_arg(ctx, "snap_origin_z", o[2]);
        }
        if let Some(outcome) = proxy_outcome {
            str_arg(ctx, "proxy_outcome", outcome);
        }
    });
    track_event_end!("iw4l.render");
}

pub fn lighting_fail() {
    track_event_begin!("iw4l.render", "lighting_fail", |_ctx: &mut EventContext| {});
    track_event_end!("iw4l.render");
}

#[allow(clippy::too_many_arguments)]
pub fn render_owner_plan(
    frame_id: u64,
    world_generation: Option<u64>,
    owner_kind: &str,
    owner_id: u32,
    model: Option<&str>,
    outcome: &str,
    object_id: Option<u16>,
    camera_origin: Option<[f32; 3]>,
    camera_hidden: Option<bool>,
    lighting_handle: Option<u32>,
    planned_surfaces: u32,
) {
    track_event_begin!(
        "iw4l.render",
        "render_owner_plan",
        |ctx: &mut EventContext| {
            i64_arg(ctx, "frame_id", frame_id as i64);
            if let Some(generation) = world_generation {
                i64_arg(ctx, "world_generation", generation as i64);
            }
            str_arg(ctx, "owner_kind", owner_kind);
            i64_arg(ctx, "owner_id", i64::from(owner_id));
            if let Some(model) = model {
                str_arg(ctx, "model", model);
            }
            str_arg(ctx, "outcome", outcome);
            if let Some(object_id) = object_id {
                i64_arg(ctx, "object_id", i64::from(object_id));
            }
            if let Some(origin) = camera_origin {
                f64_arg(ctx, "camera_x", origin[0]);
                f64_arg(ctx, "camera_y", origin[1]);
                f64_arg(ctx, "camera_z", origin[2]);
            }
            if let Some(hidden) = camera_hidden {
                i64_arg(ctx, "camera_hidden", i64::from(u8::from(hidden)));
            }
            if let Some(handle) = lighting_handle {
                i64_arg(ctx, "lighting_handle", i64::from(handle));
            }
            i64_arg(ctx, "planned_surfaces", i64::from(planned_surfaces));
        }
    );
    track_event_end!("iw4l.render");
}

#[allow(clippy::too_many_arguments)]
pub fn render_owner_submit(
    frame_id: u64,
    owner_kind: &str,
    owner_id: u32,
    object_id: Option<u16>,
    outcome: &str,
    materials: Option<&str>,
    material_textures: Option<&str>,
    product_surfaces: u32,
    execution_ready_surfaces: u32,
    prepared_surfaces: u32,
    prepared_passes: u32,
    drawn_passes: u32,
) {
    track_event_begin!(
        "iw4l.render",
        "render_owner_submit",
        |ctx: &mut EventContext| {
            i64_arg(ctx, "frame_id", frame_id as i64);
            str_arg(ctx, "owner_kind", owner_kind);
            i64_arg(ctx, "owner_id", i64::from(owner_id));
            if let Some(object_id) = object_id {
                i64_arg(ctx, "object_id", i64::from(object_id));
            }
            str_arg(ctx, "outcome", outcome);
            if let Some(materials) = materials {
                str_arg(ctx, "materials", materials);
            }
            if let Some(material_textures) = material_textures {
                str_arg(ctx, "material_textures", material_textures);
            }
            i64_arg(ctx, "product_surfaces", i64::from(product_surfaces));
            i64_arg(
                ctx,
                "execution_ready_surfaces",
                i64::from(execution_ready_surfaces),
            );
            i64_arg(ctx, "prepared_surfaces", i64::from(prepared_surfaces));
            i64_arg(ctx, "prepared_passes", i64::from(prepared_passes));
            i64_arg(ctx, "drawn_passes", i64::from(drawn_passes));
        }
    );
    track_event_end!("iw4l.render");
}

pub fn match_torn(reason: &str) {
    track_event_begin!("iw4l.sim", "match_torn", |ctx: &mut EventContext| {
        str_arg(ctx, "reason", reason);
    });
    track_event_end!("iw4l.sim");
}

pub fn match_installed(zone: &str, has_world: i64) {
    track_event_begin!("iw4l.sim", "match_installed", |ctx: &mut EventContext| {
        str_arg(ctx, "zone", zone);
        i64_arg(ctx, "has_world", has_world);
    });
    track_event_end!("iw4l.sim");
}

pub fn world_hold(spawned: i64, gpu_plan: i64, glass_n: i64) {
    track_event_begin!("iw4l.render", "world_hold", |ctx: &mut EventContext| {
        i64_arg(ctx, "spawned", spawned);
        i64_arg(ctx, "gpu_plan", gpu_plan);
        i64_arg(ctx, "glass_n", glass_n);
    });
    track_event_end!("iw4l.render");
}

pub fn world_ready(spawned: i64) {
    track_event_begin!("iw4l.render", "world_ready", |ctx: &mut EventContext| {
        i64_arg(ctx, "spawned", spawned);
    });
    track_event_end!("iw4l.render");
}

pub fn sim_hold(running: i64, movers: i64, loopback_pending: i64) {
    track_event_begin!("iw4l.sim", "sim_hold", |ctx: &mut EventContext| {
        i64_arg(ctx, "running", running);
        i64_arg(ctx, "movers", movers);
        i64_arg(ctx, "loopback_pending", loopback_pending);
    });
    track_event_end!("iw4l.sim");
}

pub fn ambient_hold(booted: i64) {
    track_event_begin!("iw4l.sim", "ambient_hold", |ctx: &mut EventContext| {
        i64_arg(ctx, "booted", booted);
    });
    track_event_end!("iw4l.sim");
}

pub fn ambient_boot(zone: &str, alias: Option<&str>) {
    track_event_begin!("iw4l.sim", "ambient_boot", |ctx: &mut EventContext| {
        str_arg(ctx, "zone", zone);
        if let Some(alias) = alias {
            str_arg(ctx, "alias", alias);
        }
    });
    track_event_end!("iw4l.sim");
}

pub fn swap(id: u64, phase: &str, target: &str) {
    track_event_begin!("iw4l.sim", "swap", |ctx: &mut EventContext| {
        i64_arg(ctx, "id", id as i64);
        str_arg(ctx, "phase", phase);
        str_arg(ctx, "target", target);
    });
    track_event_end!("iw4l.sim");
}

pub fn theater(present: i64, quit_on_end: Option<i64>, runs_authority: Option<i64>) {
    track_event_begin!("iw4l.sim", "theater", |ctx: &mut EventContext| {
        i64_arg(ctx, "present", present);
        if let Some(quit) = quit_on_end {
            i64_arg(ctx, "quit_on_end", quit);
        }
        if let Some(runs) = runs_authority {
            i64_arg(ctx, "runs_authority", runs);
        }
    });
    track_event_end!("iw4l.sim");
}

pub fn cgame_hold(present_choice: &str, presented_has_ps: i64) {
    track_event_begin!("iw4l.sim", "cgame_hold", |ctx: &mut EventContext| {
        str_arg(ctx, "present_choice", present_choice);
        i64_arg(ctx, "presented_has_ps", presented_has_ps);
    });
    track_event_end!("iw4l.sim");
}

pub fn benchmark_mark(label: &str, sequence: u64, elapsed_ns: u64) {
    track_event_begin!("iw4l.frame", "benchmark_mark", |ctx: &mut EventContext| {
        str_arg(ctx, "label", label);
        i64_arg(ctx, "sequence", sequence as i64);
        i64_arg(ctx, "elapsed_ns", elapsed_ns as i64);
    });
    track_event_end!("iw4l.frame");
}
