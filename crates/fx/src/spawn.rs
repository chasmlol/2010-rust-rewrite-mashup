use fx_iw4::{
    FX_ELEM_AT_REST_NONE, FX_ELEM_RUN_RELATIVE_TO_EFFECT, FX_ELEM_RUN_RELATIVE_TO_OFFSET,
    FX_ELEM_RUN_RELATIVE_TO_SPAWN, FX_ELEM_RUNNER_USES_RAND_ROT, FX_ELEM_TYPE_SPARK_CLOUD,
    FX_ELEM_TYPE_SPARK_FOUNTAIN, FX_ELEM_TYPE_TRAIL, FX_SPARK_CLOUD_HANDLE_NONE,
    FX_WARN_ELEM_LIMIT, FxElemType, FxRandomChannel, elem_random_seed, elem_run_mode,
    looping_catchup_begin, looping_spawn_schedule, randomly_rotate_axis, runner_rand_rot_degrees,
    sample_life_span_msec, sample_oneshot_spawn_count, sample_u16, spawn_effect_status,
    world_delta_to_local,
};

use crate::def::{FxEffectDefInfo, FxElemDefInfo};
use crate::elem::{
    FX_ELEM_HANDLE_NONE, FxElemSlot, elem_class_for_type, elem_handle_for_slot,
    elem_slot_for_handle,
};
use crate::spark::{alloc_spark_cloud, free_spark_cloud};
use crate::system::FxSystemHost;
use crate::trail::{FX_TRAIL_HANDLE_NONE, FxTrailSlot, trail_slot_for_handle};

pub fn start_new_effect(host: &mut FxSystemHost, effect_handle: u16, def: FxEffectDefInfo<'_>) {
    let Some(effect_slot) = host.slot_index_for_handle(effect_handle) else {
        return;
    };
    let looping = def.looping_count.max(0) as usize;
    let oneshot = def.one_shot_count.max(0) as usize;

    if let Some(effect) = host.effect_at_mut(effect_slot) {
        effect.status = spawn_effect_status(def.msec_looping_life);
    }

    alloc_trails_for_effect(host, effect_slot, def);

    let spawn_msec = host
        .effect_at(effect_slot)
        .map(|e| e.msec_begin)
        .unwrap_or(host.msec_now);

    for i in 0..looping {
        let Some(elem_def) = def.elems.get(i).copied() else {
            break;
        };
        if elem_def.elem_type == FX_ELEM_TYPE_TRAIL {
            continue;
        }
        spawn_elem(host, effect_slot, i as u8, elem_def, 0, spawn_msec);
    }

    for i in looping..(looping + oneshot) {
        let Some(elem_def) = def.elems.get(i).copied() else {
            break;
        };
        spawn_oneshot_elems(host, effect_slot, i, elem_def, spawn_msec);
    }

    crate::sort::sort_new_elems_in_effect(host, effect_slot, [0.0; 3]);
    let _ = FX_WARN_ELEM_LIMIT;
}

fn alloc_trails_for_effect(host: &mut FxSystemHost, effect_slot: usize, def: FxEffectDefInfo<'_>) {
    let total = def.total_elem_defs().max(0) as usize;
    let mut exhausted = false;
    for i in 0..total {
        let Some(elem_def) = def.elems.get(i).copied() else {
            break;
        };
        if elem_def.elem_type != FX_ELEM_TYPE_TRAIL {
            continue;
        }
        if exhausted {
            continue;
        }
        let Some(handle) = crate::trail::alloc_trail(host) else {
            exhausted = true;
            continue;
        };
        let Some(trail_slot) = trail_slot_for_handle(handle) else {
            continue;
        };
        let Some(effect) = host.effect_at_mut(effect_slot) else {
            free_trail_slot_only(host, trail_slot);
            return;
        };
        let prev_head = effect.first_trail_handle;
        effect.first_trail_handle = handle;
        host.trails[trail_slot] = FxTrailSlot {
            occupied: true,
            next_trail_handle: prev_head,
            first_elem_handle: FX_TRAIL_HANDLE_NONE,
            last_elem_handle: FX_TRAIL_HANDLE_NONE,
            def_index: i as i8,
            sequence: 0,
            split_leftover: 0.0,
        };
    }
}

fn free_trail_slot_only(host: &mut FxSystemHost, trail_slot: usize) {
    let next_free = host
        .trail_first_free
        .map(|d| d as u16)
        .unwrap_or(FX_TRAIL_HANDLE_NONE);
    host.trails[trail_slot] = FxTrailSlot {
        occupied: false,
        next_trail_handle: next_free,
        ..FxTrailSlot::default()
    };
    host.trail_first_free = Some(trail_slot);
    host.trail_live_count = host.trail_live_count.saturating_sub(1);
}

pub(crate) fn free_all_trails_for_effect(host: &mut FxSystemHost, effect_slot: usize) {
    let mut handle = host
        .effect_at(effect_slot)
        .map(|e| e.first_trail_handle)
        .unwrap_or(FX_TRAIL_HANDLE_NONE);
    while handle != FX_TRAIL_HANDLE_NONE {
        let Some(trail_slot) = trail_slot_for_handle(handle) else {
            break;
        };
        if !host.trails[trail_slot].occupied {
            break;
        }
        let next = host.trails[trail_slot].next_trail_handle;
        let freed_samples = free_trail_elems_on_trail(host, trail_slot);

        if freed_samples > 0 {
            if let Some(effect) = host.effect_at_mut(effect_slot) {
                effect.status = effect.status.wrapping_sub(freed_samples);
            }
        }
        free_trail_slot_only(host, trail_slot);
        handle = next;
    }
    if let Some(effect) = host.effect_at_mut(effect_slot) {
        effect.first_trail_handle = FX_TRAIL_HANDLE_NONE;
    }
}

fn free_trail_elems_on_trail(host: &mut FxSystemHost, trail_slot: usize) -> u32 {
    let mut handle = host.trails[trail_slot].first_elem_handle;
    let mut freed = 0u32;
    while handle != FX_TRAIL_HANDLE_NONE {
        let Some(elem_slot) = crate::trail::trail_elem_slot_for_handle(handle) else {
            break;
        };
        if !host.trail_elems[elem_slot].occupied {
            break;
        }
        let next = host.trail_elems[elem_slot].next_trail_elem_handle;
        let next_free = host
            .trail_elem_first_free
            .map(|d| d as u16)
            .unwrap_or(FX_TRAIL_HANDLE_NONE);
        host.trail_elems[elem_slot] = crate::trail::FxTrailElemSlot {
            occupied: false,
            next_trail_elem_handle: next_free,
            ..crate::trail::FxTrailElemSlot::default()
        };
        host.trail_elem_first_free = Some(elem_slot);
        host.trail_elem_live_count = host.trail_elem_live_count.saturating_sub(1);
        freed = freed.saturating_add(1);
        handle = next;
    }
    host.trails[trail_slot].first_elem_handle = FX_TRAIL_HANDLE_NONE;
    host.trails[trail_slot].last_elem_handle = FX_TRAIL_HANDLE_NONE;
    freed
}

pub fn sort_effect_elems(host: &mut FxSystemHost, effect_handle: u16, camera_origin: [f32; 3]) {
    let Some(slot) = host.slot_index_for_handle(effect_handle) else {
        return;
    };
    crate::sort::sort_new_elems_in_effect(host, slot, camera_origin);
}

pub fn spawn_looping_partial(
    host: &mut FxSystemHost,
    effect_slot: usize,
    def: FxEffectDefInfo<'_>,
    msec_update_begin: i32,
    msec_update_end: i32,
) {
    let looping = def.looping_count.max(0) as usize;
    let msec_when_played = host
        .effect_at(effect_slot)
        .map(|e| e.msec_begin)
        .unwrap_or(msec_update_begin);
    for i in 0..looping {
        let Some(elem_def) = def.elems.get(i).copied() else {
            break;
        };
        if elem_def.elem_type == FX_ELEM_TYPE_TRAIL {
            continue;
        }

        let interval = elem_def.spawn_a;
        let count = elem_def.spawn_b;

        let duration = elem_def
            .delay_base
            .wrapping_add(elem_def.delay_amp)
            .wrapping_add(elem_def.life_base)
            .wrapping_add(elem_def.life_amp);
        let begin = looping_catchup_begin(msec_update_begin, msec_update_end, duration);
        for spawn in
            looping_spawn_schedule(msec_when_played, begin, msec_update_end, interval, count)
        {
            let seq = spawn.sequence.min(u8::MAX as i32) as u8;
            spawn_elem(host, effect_slot, i as u8, elem_def, seq, spawn.msec);
        }
    }
}

pub fn stop_pending_loop(host: &mut FxSystemHost, effect_slot: usize) {
    use fx_iw4::{FX_STATUS_HAS_PENDING_LOOP_ELEMS, FX_STATUS_REF_COUNT_MASK_IW4};
    let (handle, refs) = match host.effect_at(effect_slot) {
        Some(e) if (e.status & FX_STATUS_HAS_PENDING_LOOP_ELEMS) != 0 => {
            (e.own_handle, e.status & FX_STATUS_REF_COUNT_MASK_IW4)
        }
        _ => return,
    };
    if let Some(effect) = host.effect_at_mut(effect_slot) {
        effect.status &= !FX_STATUS_HAS_PENDING_LOOP_ELEMS;
        effect.status = effect.status.wrapping_sub(1);
    }
    if refs == 1 {
        host.del_ref_to_effect(handle);
    }
}

pub fn stop_effect_non_recursive(host: &mut FxSystemHost, effect_slot: usize) {
    use fx_iw4::{
        FX_STATUS_REF_COUNT_MASK_IW4, begin_iterating_over_effects_exclusive,
        end_iterating_over_effects, end_iterating_runs_gc, stop_effect_has_owned,
        stop_effect_non_recursive_allows,
    };
    let (self_handle, status) = match host.effect_at(effect_slot) {
        Some(e) => (e.own_handle, e.status),
        None => return,
    };
    if !stop_effect_non_recursive_allows(status) {
        return;
    }
    stop_pending_loop(host, effect_slot);
    if stop_effect_has_owned(status) {
        host.iterator_count = begin_iterating_over_effects_exclusive(host.iterator_count);
        let mut cursor = host.first_active_effect;
        let end = host.first_new_effect;
        while cursor != end {
            let handle = host.handle_at_ring(cursor as u32);
            if handle != self_handle {
                if let Some(other) = host.slot_index_for_handle(handle) {
                    let owner = host.effect_at(other).map(|e| e.own_handle);
                    if owner == Some(self_handle) {
                        stop_effect_non_recursive(host, other);
                    }
                }
            }
            cursor = cursor.wrapping_add(1);
        }
        host.iterator_count = end_iterating_over_effects(host.iterator_count);
        if end_iterating_runs_gc(host.iterator_count, host.needs_garbage_collection) {
            host.run_garbage_collection();
        }
    }
    let refs = host
        .effect_at(effect_slot)
        .map(|e| e.status & FX_STATUS_REF_COUNT_MASK_IW4)
        .unwrap_or(0);
    if refs == 1 {
        host.del_ref_to_effect(self_handle);
    }
}

fn spawn_oneshot_elems(
    host: &mut FxSystemHost,
    effect_slot: usize,
    elem_def_index: usize,
    elem_def: FxElemDefInfo,
    spawn_msec: i32,
) {
    if elem_def.elem_type == FX_ELEM_TYPE_TRAIL {
        return;
    }
    let effect_seed = host
        .effect_at(effect_slot)
        .map(|e| e.random_seed)
        .unwrap_or(0);

    let rand16 = sample_u16(effect_seed, FxRandomChannel::OneShotCount);
    let count = sample_oneshot_spawn_count(elem_def.spawn_a, elem_def.spawn_b, rand16);
    if count <= 0 {
        return;
    }
    for seq in 0..count {
        spawn_elem(
            host,
            effect_slot,
            elem_def_index as u8,
            elem_def,
            seq as u8,
            spawn_msec,
        );
    }
}

fn spawn_origin_world(
    elem_def: FxElemDefInfo,
    effect_origin: [f32; 3],
    effect_axis: [[f32; 3]; 3],
    life_idx: u64,
) -> [f32; 3] {
    fx_iw4::spawn_origin_world(
        effect_origin,
        effect_axis,
        elem_def.spawn_origin,
        elem_def.flags,
        elem_def.spawn_offset_radius_base,
        elem_def.spawn_offset_radius_amp,
        elem_def.spawn_offset_height_base,
        elem_def.spawn_offset_height_amp,
        life_idx,
    )
}

fn elem_spawn_origin(
    elem_def: FxElemDefInfo,
    effect_origin: [f32; 3],
    effect_axis: [[f32; 3]; 3],
    life_idx: u64,
) -> [f32; 3] {
    match elem_run_mode(elem_def.flags) {
        FX_ELEM_RUN_RELATIVE_TO_OFFSET => [0.0; 3],
        _ => {
            let o = spawn_origin_world(elem_def, effect_origin, effect_axis, life_idx);
            let run = elem_run_mode(elem_def.flags);
            if run == FX_ELEM_RUN_RELATIVE_TO_SPAWN || run == FX_ELEM_RUN_RELATIVE_TO_EFFECT {
                world_delta_to_local(o, effect_origin, effect_axis)
            } else {
                o
            }
        }
    }
}

fn spawn_elem(
    host: &mut FxSystemHost,
    effect_slot: usize,
    def_index: u8,
    elem_def: FxElemDefInfo,
    sequence: u8,
    spawn_msec: i32,
) {
    let (effect_origin, effect_axis, random_seed, parent_name, catalog_index, bolt, mark_entity) = {
        let e = match host.effect_at(effect_slot) {
            Some(e) if e.ring_resident => e,
            _ => return,
        };
        (
            e.origin,
            e.axis,
            e.random_seed,
            e.def_name.as_str(),
            e.catalog_index,
            e.bolt,
            e.mark_entity,
        )
    };

    let after_delay_base = spawn_msec.wrapping_add(elem_def.delay_base);
    let delay = if elem_def.delay_amp != 0 {
        let delay_idx = elem_random_seed(random_seed, sequence, after_delay_base);
        let delay_rand = sample_u16(delay_idx, FxRandomChannel::Delay);
        sample_life_span_msec(elem_def.delay_base, elem_def.delay_amp, delay_rand)
    } else {
        elem_def.delay_base
    };
    let msec_begin = spawn_msec.wrapping_add(delay);
    let life_idx = elem_random_seed(random_seed, sequence, msec_begin);

    match FxElemType::from_u8(elem_def.elem_type) {
        Some(FxElemType::Sound) => {
            let parent_name = parent_name.to_owned();
            host.pending_sounds.push(crate::PendingSoundSpawn {
                parent_name,
                catalog_index,
                def_index,
                msec_begin,
                random_seed: life_idx,
                origin: spawn_origin_world(elem_def, effect_origin, effect_axis, life_idx),
            });
            return;
        }
        Some(FxElemType::Decal) => {
            let parent_name = parent_name.to_owned();

            host.pending_decals.push(crate::PendingDecalSpawn {
                mark_entity,
                parent_name,
                catalog_index,
                def_index,
                msec_begin,
                random_seed: life_idx,
                origin: effect_origin,
                bolt,
                axis: effect_axis,
            });
            return;
        }
        Some(FxElemType::Runner) => {
            let parent_name = parent_name.to_owned();

            let origin = spawn_origin_world(elem_def, effect_origin, effect_axis, life_idx);
            let (axis, rot_deg) = if (elem_def.flags & FX_ELEM_RUNNER_USES_RAND_ROT) != 0 {
                (
                    randomly_rotate_axis(effect_axis, life_idx),
                    Some(runner_rand_rot_degrees(life_idx)),
                )
            } else {
                (effect_axis, None)
            };
            host.pending_runners.push(crate::PendingRunnerSpawn {
                mark_entity,
                parent_name,
                catalog_index,
                def_index,
                msec_begin,
                random_seed: life_idx,
                origin,
                axis,
                rot_deg,
            });
            return;
        }
        _ => {}
    }
    let Some(class) = elem_class_for_type(elem_def.elem_type) else {
        return;
    };

    let life_rand = sample_u16(life_idx, FxRandomChannel::Life);
    let life = sample_life_span_msec(elem_def.life_base, elem_def.life_amp, life_rand);

    let life_end = msec_begin.wrapping_add(life);
    if !elem_def.keep_alive_by_child() && host.msec_now >= life_end {
        return;
    }

    let origin = elem_spawn_origin(elem_def, effect_origin, effect_axis, life_idx);

    let Some(elem_slot) = alloc_elem(host) else {
        host.elem_alloc_failures = host.elem_alloc_failures.saturating_add(1);
        return;
    };
    let handle = elem_handle_for_slot(elem_slot as u32);

    let Some(effect) = host.effect_at_mut(effect_slot) else {
        free_elem_slot_only(host, elem_slot);
        return;
    };

    effect.status = effect.status.wrapping_add(1);
    let prev_head = effect.first_elem_handle[class];
    effect.first_elem_handle[class] = handle;

    host.elems[elem_slot] = FxElemSlot {
        occupied: true,
        def_index,
        elem_type: elem_def.elem_type,
        flags: elem_def.flags,
        visual_count: elem_def.visual_count,
        sequence,
        at_rest_fraction: FX_ELEM_AT_REST_NONE,
        emit_residual: 0,
        next_elem_handle: prev_head,
        prev_elem_handle: FX_ELEM_HANDLE_NONE,
        msec_begin,
        life_span_msec: life,
        base_vel: [0.0; 3],
        origin,
        spawn_origin: elem_def.spawn_origin,
        spawn_offset_radius: [
            elem_def.spawn_offset_radius_base,
            elem_def.spawn_offset_radius_amp,
        ],
        spawn_offset_height: [
            elem_def.spawn_offset_height_base,
            elem_def.spawn_offset_height_amp,
        ],
        owner_effect_slot: effect_slot as u16,
        class_index: class as u8,
        sort_order: elem_def.sort_order,
        spark_cloud_handle: FX_SPARK_CLOUD_HANDLE_NONE,
    };
    if prev_head != FX_ELEM_HANDLE_NONE {
        if let Some(prev_slot) = elem_slot_for_handle(prev_head) {
            host.elems[prev_slot].prev_elem_handle = handle;
        }
    }

    if elem_def.elem_type == FX_ELEM_TYPE_SPARK_CLOUD {
        match alloc_spark_cloud(host) {
            Some(spark_handle) => {
                host.elems[elem_slot].spark_cloud_handle = spark_handle;
            }
            None => {
                free_elem(host, handle);
            }
        }
    } else if elem_def.elem_type == FX_ELEM_TYPE_SPARK_FOUNTAIN {
        match crate::spark_fountain::alloc_spark_fountain(host) {
            Some(fountain_handle) => {
                host.elems[elem_slot].spark_cloud_handle = fountain_handle;
                match crate::spark_fountain::spray_spark_fountain(
                    host,
                    fountain_handle,
                    origin,
                    fx_iw4::get_elem_angles_axis(
                        elem_def.spawn_angles,
                        elem_def.angular_velocity,
                        life_idx,
                        0.0,
                        effect_axis,
                    )[0],
                    elem_def.flags,
                    elem_def.spark_count,
                    elem_def.spark_vel_min,
                    elem_def.spark_vel_max,
                    elem_def.spark_vel_cone_frac,
                    elem_def.spark_gravity,
                    elem_def.spark_length,
                    elem_def.spark_loop_time,
                    elem_def.spark_boost_time,
                    elem_def.spark_boost_factor,
                    elem_def.spark_bounce_frac,
                    elem_def.spark_bounce_rand,
                ) {
                    crate::spark_fountain::FountainSpray::Ready => {}
                    crate::spark_fountain::FountainSpray::MeshFull => {
                        free_elem(host, handle);
                    }
                }
            }
            None => {
                free_elem(host, handle);
            }
        }
    }
}

fn alloc_elem(host: &mut FxSystemHost) -> Option<usize> {
    let dense = host.elem_first_free?;
    if dense >= host.elems.len() {
        host.elem_first_free = None;
        return None;
    }

    let next = host.elems[dense].next_elem_handle;
    host.elem_first_free = if next == FX_ELEM_HANDLE_NONE {
        None
    } else {
        Some(next as usize)
    };
    host.elem_live_count = host.elem_live_count.saturating_add(1);
    Some(dense)
}

pub(crate) fn free_elem(host: &mut FxSystemHost, handle: u16) {
    let Some(elem_slot) = elem_slot_for_handle(handle) else {
        return;
    };
    if !host.elems[elem_slot].occupied {
        return;
    }
    let next = host.elems[elem_slot].next_elem_handle;
    let prev = host.elems[elem_slot].prev_elem_handle;
    let effect_slot = host.elems[elem_slot].owner_effect_slot as usize;
    let class = host.elems[elem_slot].class_index as usize;

    if prev == FX_ELEM_HANDLE_NONE {
        if let Some(effect) = host.effect_at_mut(effect_slot) {
            if class < 3 && effect.first_elem_handle[class] == handle {
                effect.first_elem_handle[class] = next;
            }
        }
    } else if let Some(prev_slot) = elem_slot_for_handle(prev) {
        host.elems[prev_slot].next_elem_handle = next;
    }
    if next != FX_ELEM_HANDLE_NONE {
        if let Some(next_slot) = elem_slot_for_handle(next) {
            host.elems[next_slot].prev_elem_handle = prev;
        }
    }
    if class == 0 {
        if let Some(effect) = host.effect_at_mut(effect_slot) {
            if effect.first_sorted_elem_handle == handle {
                effect.first_sorted_elem_handle = next;
            }
        }
    }

    free_elem_slot_only(host, elem_slot);

    let mut unique = false;
    if let Some(effect) = host.effect_at_mut(effect_slot) {
        let refs = effect.status & fx_iw4::FX_STATUS_REF_COUNT_MASK_IW4;
        unique = refs == 1;
        effect.status = effect.status.wrapping_sub(1);
    }
    if unique {
        host.needs_garbage_collection = true;
    }
}

fn free_elem_slot_only(host: &mut FxSystemHost, elem_slot: usize) {
    let elem_type = host.elems[elem_slot].elem_type;
    let spark_handle = host.elems[elem_slot].spark_cloud_handle;
    if elem_type == FX_ELEM_TYPE_SPARK_CLOUD {
        free_spark_cloud(host, spark_handle);
    } else if elem_type == FX_ELEM_TYPE_SPARK_FOUNTAIN {
        crate::spark_fountain::free_spark_fountain(host, spark_handle);
    }
    let next_free = host
        .elem_first_free
        .map(|d| d as u16)
        .unwrap_or(FX_ELEM_HANDLE_NONE);
    host.elems[elem_slot] = FxElemSlot {
        occupied: false,
        next_elem_handle: next_free,
        ..FxElemSlot::default()
    };
    host.elem_first_free = Some(elem_slot);
    host.elem_live_count = host.elem_live_count.saturating_sub(1);
}

pub(crate) fn free_all_elems_for_effect(host: &mut FxSystemHost, effect_slot: usize) {
    for class in 0..3 {
        let mut handle = host
            .effect_at(effect_slot)
            .map(|e| e.first_elem_handle[class])
            .unwrap_or(FX_ELEM_HANDLE_NONE);
        while handle != FX_ELEM_HANDLE_NONE {
            let next = elem_slot_for_handle(handle)
                .and_then(|s| host.elems.get(s).map(|e| e.next_elem_handle))
                .unwrap_or(FX_ELEM_HANDLE_NONE);

            free_elem_bulk(host, handle);
            handle = next;
        }
        if let Some(effect) = host.effect_at_mut(effect_slot) {
            effect.first_elem_handle[class] = FX_ELEM_HANDLE_NONE;
            if class == 0 {
                effect.first_sorted_elem_handle = FX_ELEM_HANDLE_NONE;
            }
        }
    }
}

fn free_elem_bulk(host: &mut FxSystemHost, handle: u16) {
    let Some(elem_slot) = elem_slot_for_handle(handle) else {
        return;
    };
    if !host.elems[elem_slot].occupied {
        return;
    }
    let effect_slot = host.elems[elem_slot].owner_effect_slot as usize;
    free_elem_slot_only(host, elem_slot);
    if let Some(effect) = host.effect_at_mut(effect_slot) {
        effect.status = effect.status.wrapping_sub(1);
    }
}
