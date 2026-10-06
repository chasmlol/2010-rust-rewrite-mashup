use assets::BoundWeapons;
use bevy::prelude::*;
use fx::{FxMsec, FxSystemHost, PlayResult, SpawnFail, axis_from_hit_normal};
use fx_iw4::{
    FX_IMPACT_EXIT_SURFACE_FLAG, FX_SURF_TYPE_FLESH, flesh_effect_index, impact_table_row,
};
use net::CEntitySlots;
use weapon_iw4::SURFACE_TYPE_NAMES;

use crate::present::{
    FxElemInfoCache, FxScene, play_named_bolted_in_world, play_named_oriented_in_world,
};
use crate::{
    CombatFxDump, FxJournalCursor, HostFxSystem, PreparedFxCatalog, PreparedImpactFx,
    PreparedTracers, TracerDrawGate, TracerSpawnSkip, TracerWorld, try_spawn_tracer,
};

pub const IDENTITY_AXIS: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

pub fn try_play_weapon_fx_at_origin(
    host: &mut FxSystemHost,
    catalog: &asset_game::FxDefinitions,
    cache: &mut FxElemInfoCache,
    name: Option<asset_game::FxName<'_>>,
    origin: [f32; 3],
    axis: [[f32; 3]; 3],
    played: &mut u32,
    scene: Option<&dyn FxScene>,
) -> bool {
    let Some(name) = name else {
        return false;
    };
    cache.sync(catalog);
    match play_named_oriented_in_world(host, catalog, cache, name, origin, axis, scene) {
        Some(PlayResult::PlayedReleased { .. } | PlayResult::Held { .. }) => {
            *played = played.saturating_add(1);
            true
        }
        Some(PlayResult::Failed(_)) | None => false,
    }
}

pub fn try_play_weapon_fx_bolted(
    host: &mut FxSystemHost,
    catalog: &asset_game::FxDefinitions,
    cache: &mut FxElemInfoCache,
    name: Option<asset_game::FxName<'_>>,
    target: Option<fx::FxBoltTarget>,
    played: &mut u32,
    scene: Option<&dyn FxScene>,
) -> bool {
    let (Some(name), Some(target)) = (name, target) else {
        return false;
    };
    cache.sync(catalog);
    match play_named_bolted_in_world(host, catalog, cache, name, target, scene) {
        Some(PlayResult::PlayedReleased { .. } | PlayResult::Held { .. }) => {
            *played = played.saturating_add(1);
            true
        }
        Some(PlayResult::Failed(_)) | None => false,
    }
}

pub fn play_shell_eject(
    host: &mut FxSystemHost,
    catalog: &asset_game::FxDefinitions,
    cache: &mut FxElemInfoCache,
    combat_fx: Option<&asset_game::WeaponCombatFx>,
    player_view: bool,
    last_shot: bool,
    target: Option<fx::FxBoltTarget>,
    cursor: &mut FxJournalCursor,
    combat: &mut CombatFxDump,
    scene: Option<&dyn FxScene>,
) {
    combat.last_brass_lastshot = Some(i64::from(
        last_shot && combat_fx.is_some_and(|fx| fx.last_shot_eject_pair_authored()),
    ));
    let name = combat_fx.and_then(|fx| fx.brass_present_for_event(player_view, last_shot));
    combat.last_weapon_brass_edge = combat_fx.map(|fx| {
        fx.brass_edge_for_event(player_view, last_shot)
            .edge_kind()
            .to_owned()
    });
    if let Some(name) = name {
        combat.last_brass_name = Some(name.name.to_owned());
    }
    if !try_play_weapon_fx_bolted(
        host,
        catalog,
        cache,
        name,
        target,
        &mut cursor.brass_played,
        scene,
    ) {
        cursor.brass_gap = cursor.brass_gap.saturating_add(1);
    }
}

pub fn missile_bolt_target(
    poses: Option<&render_anim::HostDObjPoseFrame>,
    meshes: &asset_model::ProjectileMeshCatalog,
    namespace: asset_core::AssetNamespace,
    model: &str,
    entnum: u32,
) -> Option<fx::FxBoltTarget> {
    let entry = meshes.get(namespace, model)?;
    let bone = entry
        .skel
        .bone_names
        .iter()
        .position(|name| name == "tag_fx")?;
    let bone = u16::try_from(bone).ok()?;
    let resolved = poses?.resolve_live_bolt(entnum, bone)?;
    let orientation = resolved.orientation?;
    Some(fx::FxBoltTarget {
        dobj: entnum,
        bone,
        centity_teleport: resolved.centity_teleport,
        orientation,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExplosionFxNames<'a> {
    pub table: Option<asset_game::FxName<'a>>,
    pub slot: Option<asset_game::FxName<'a>>,
    pub row: Option<usize>,
}

pub fn explosion_fx_names<'a>(
    impact_type: Option<i32>,
    surf_type: u8,
    table: Option<&'a asset_game::OwnedFxImpactTable>,
    slot: Option<asset_game::FxName<'a>>,
) -> ExplosionFxNames<'a> {
    let row = impact_type.and_then(|t| {
        table.map_or_else(
            || impact_table_row(t, false),
            |table| table.impact_row(t, false),
        )
    });
    let surf = surf_type as usize;
    let flesh = (surf == FX_SURF_TYPE_FLESH).then_some(0);
    let table = row.and_then(|row| table.and_then(|t| t.effect_name(row, surf, flesh)));
    let table = table.filter(|name| slot.is_none_or(|slot| slot.namespace == name.namespace));
    let slot = slot.filter(|name| Some(*name) != table);
    ExplosionFxNames { table, slot, row }
}

#[allow(clippy::too_many_arguments)]
pub fn play_pellet_segment(
    attacker_entity_num: i32,
    weapon: u32,
    correlation: u32,
    pellet: u16,
    hand: u8,
    seg_start: [f32; 3],
    seg_end: [f32; 3],
    normal: [f32; 3],
    surf_type: u8,
    surface_flags: u32,
    flesh_flags: u32,
    world_bolts: &Query<&render_anim::RemoteFxBolts>,
    fpv_bolts: &render_anim::FpvBoltTargets,
    slots: &CEntitySlots,
    catalog: Option<&PreparedFxCatalog>,
    elem_infos: &mut FxElemInfoCache,
    impact_fx: Option<&PreparedImpactFx>,
    weapons: Option<&BoundWeapons<'_>>,
    tracers: Option<&PreparedTracers>,
    tracer_world: &mut TracerWorld,
    gate: &mut TracerDrawGate,
    local_number: i32,
    host: &mut HostFxSystem,
    cursor: &mut FxJournalCursor,
    combat: &mut CombatFxDump,
    scene: Option<&dyn FxScene>,
) {
    host.0.glass.hit_segment(
        seg_start,
        seg_end,
        (u64::from(correlation) << 32)
            ^ (u64::from(attacker_entity_num as u32) << 16)
            ^ u64::from(pellet),
    );
    combat.last_surf = Some(i64::from(surf_type));
    combat.last_surf_flags = Some(i64::from(surface_flags));
    combat.last_surf_name = SURFACE_TYPE_NAMES
        .get(surf_type as usize)
        .map(|name| (*name).to_owned());
    let tracer_edge = weapons
        .and_then(|weapons| weapons.registry().combat_fx_of(weapon))
        .map(|fx| fx.tracer)
        .unwrap_or(assets::AssetEdge::Absent);
    let own_shot = attacker_entity_num == local_number;
    let source_id = attacker_entity_num.max(0) as u32;

    let tag_start = gate
        .first_segment_of_pellet(source_id, correlation, pellet)
        .then(|| {
            if own_shot {
                fpv_bolts.flash[usize::from(hand).min(fpv_bolts.flash.len() - 1)]
            } else {
                u16::try_from(attacker_entity_num)
                    .ok()
                    .and_then(|number| slots.entity_for_number(number))
                    .and_then(|entity| world_bolts.get(entity).ok())
                    .and_then(|bolts| bolts.flash)
            }
        })
        .flatten()
        .map(|target| target.orientation.origin);
    let start = tag_start.unwrap_or(seg_start);
    match tracers.map(|t| {
        try_spawn_tracer(
            gate,
            tracer_world,
            &t.0,
            tracer_edge,
            source_id,
            correlation,
            pellet,
            start,
            seg_end,
            own_shot,
            FxMsec::from_host(&host.0),
            combat,
        )
    }) {
        None | Some(Err(TracerSpawnSkip::NoDef)) => {
            cursor.tracer_gap = cursor.tracer_gap.saturating_add(1);
            cursor.tracer_skip_no_def = cursor.tracer_skip_no_def.saturating_add(1);
        }
        Some(Err(TracerSpawnSkip::Interval)) => {
            cursor.tracer_gap = cursor.tracer_gap.saturating_add(1);
            cursor.tracer_skip_interval = cursor.tracer_skip_interval.saturating_add(1);
        }
        Some(Err(TracerSpawnSkip::Short)) => {
            cursor.tracer_gap = cursor.tracer_gap.saturating_add(1);
            cursor.tracer_skip_short = cursor.tracer_skip_short.saturating_add(1);
        }
        Some(Ok(())) => {
            combat.tracer_spawned = combat.tracer_spawned.saturating_add(1);
            if tag_start.is_some() {
                cursor.tracer_from_tag = cursor.tracer_from_tag.saturating_add(1);
            }
        }
    }
    if normal == [0.0, 0.0, 0.0] {
        sync_combat_dump(cursor, combat);
        return;
    }
    let Some(impact_type) = weapons
        .and_then(|w| w.row(weapon).and_then(|weapon| weapon.event_facts()))
        .map(|f| f.impact_type)
    else {
        cursor.impact_miss_table = cursor.impact_miss_table.saturating_add(1);
        combat.last_impact_miss_why = Some("no_weapon".into());
        sync_combat_dump(cursor, combat);
        return;
    };
    play_impact_table_cell(
        impact_type,
        seg_end,
        normal,
        surf_type,
        surface_flags,
        flesh_flags,
        catalog,
        impact_fx,
        elem_infos,
        host,
        cursor,
        combat,
        scene,
    );
}

pub fn play_impact_table_cell(
    impact_type: i32,
    origin: [f32; 3],
    normal: [f32; 3],
    surf_type: u8,
    surface_flags: u32,
    flesh_flags: u32,
    catalog: Option<&PreparedFxCatalog>,
    impact_fx: Option<&PreparedImpactFx>,
    cache: &mut FxElemInfoCache,
    host: &mut HostFxSystem,
    cursor: &mut FxJournalCursor,
    combat: &mut CombatFxDump,
    scene: Option<&dyn FxScene>,
) {
    combat.last_surf = Some(i64::from(surf_type));
    combat.last_surf_name = SURFACE_TYPE_NAMES
        .get(surf_type as usize)
        .map(|name| (*name).to_owned());
    let exit = surface_flags & FX_IMPACT_EXIT_SURFACE_FLAG != 0;
    let table = impact_fx.and_then(|fx| fx.0.as_ref());
    let Some(row) = table.map_or_else(
        || impact_table_row(impact_type, exit),
        |table| table.impact_row(impact_type, exit),
    ) else {
        cursor.impact_miss_table = cursor.impact_miss_table.saturating_add(1);
        combat.last_impact_miss_why = Some("no_row".into());
        sync_combat_dump(cursor, combat);
        return;
    };
    combat.last_row = Some(row as i64);
    let surf = surf_type as usize;
    let flesh_slot = (surf == FX_SURF_TYPE_FLESH).then(|| flesh_effect_index(flesh_flags));
    combat.last_impact_cell_empty = table.and_then(|t| {
        let entry = t.entries.get(row)?;
        if let Some(slot) = flesh_slot {
            return Some(i64::from(
                entry.flesh.get(slot).is_none_or(|name| name.is_empty()),
            ));
        }
        if surf >= fx_iw4::FX_IMPACT_NONFLESH_COUNT {
            return Some(0);
        }
        Some(i64::from(entry.nonflesh[surf].is_empty()))
    });
    let Some(def_name) = table.and_then(|t| t.effect_name(row, surf, flesh_slot)) else {
        cursor.impact_miss_def = cursor.impact_miss_def.saturating_add(1);
        combat.last_impact_miss_why = Some(
            if table.is_none() {
                "no_table"
            } else if combat.last_impact_cell_empty == Some(1) {
                "empty_cell"
            } else {
                "cell_miss"
            }
            .into(),
        );
        if !cursor.impact_miss_def_warned {
            cursor.impact_miss_def_warned = true;
            diag::warn!(
                World,
                "fx: BulletImpact row={row} surf={surf} — no ImpactFx cell name; play_oriented skipped (further misses counted)"
            );
        }
        sync_combat_dump(cursor, combat);
        return;
    };
    combat.last_impact_def = Some(def_name.name.to_owned());
    combat.last_impact_miss_why = None;
    let Some(catalog) = catalog else {
        return;
    };
    cache.sync(&catalog.0);
    let Some(result) = play_named_oriented_in_world(
        &mut host.0,
        &catalog.0,
        cache,
        def_name,
        origin,
        axis_from_hit_normal(normal),
        scene,
    ) else {
        cursor.impact_miss_def = cursor.impact_miss_def.saturating_add(1);
        combat.last_impact_miss_why = Some("catalog".into());
        if !cursor.impact_miss_def_warned {
            cursor.impact_miss_def_warned = true;
            diag::warn!(
                World,
                "fx: BulletImpact cell `{def_name}` not in FxCatalog — play_oriented skipped"
            );
        }
        sync_combat_dump(cursor, combat);
        return;
    };
    if matches!(
        result,
        PlayResult::PlayedReleased { .. } | PlayResult::Held { .. }
    ) {
        cursor.impact_played = cursor.impact_played.saturating_add(1);
        combat.impact_msec = Some(host.0.msec_now);
    } else if let PlayResult::Failed(e) = result {
        cursor.impact_miss_def = cursor.impact_miss_def.saturating_add(1);
        combat.last_impact_miss_why = Some(
            match e {
                SpawnFail::RingFull => "ring_full",
                SpawnFail::TooManySpotlights => "spotlights",
                SpawnFail::EffectLimit => "effect_limit",
            }
            .into(),
        );
        diag::warn!(World, "fx: play_oriented `{def_name}` failed: {e:?}");
    }

    log_combat_fx_gaps(cursor, combat);
    sync_combat_dump(cursor, combat);
}

pub fn log_combat_fx_gaps(cursor: &mut FxJournalCursor, combat: &CombatFxDump) {
    if !cursor.logged
        && (cursor.muzzle_played > 0
            || cursor.muzzle_gap > 0
            || combat.tracer_spawned > 0
            || cursor.tracer_gap > 0
            || cursor.brass_played > 0
            || cursor.pellet_played > 0
            || cursor.impact_miss_def > 0
            || cursor.boom_played > 0
            || cursor.explosion_gap > 0
            || cursor.impact_played > 0)
    {
        diag::info!(
            World,
            "fx: muzzle_gap={} muzzle_played={} muzzle_bolted={} tracer_gap={} tracer_spawned={} tracer_from_tag={} tracer_skip_interval={} tracer_skip_short={} tracer_skip_no_def={} fire_sound_gap={} explosion_sound_gap={} explosion_gap={} brass_gap={} brass_played={} pellet_played={} impact_played={} impact_miss_table={} impact_miss_def={} boom_played={}",
            cursor.muzzle_gap,
            cursor.muzzle_played,
            cursor.muzzle_bolted,
            cursor.tracer_gap,
            combat.tracer_spawned,
            cursor.tracer_from_tag,
            cursor.tracer_skip_interval,
            cursor.tracer_skip_short,
            cursor.tracer_skip_no_def,
            cursor.fire_sound_gap,
            cursor.explosion_sound_gap,
            cursor.explosion_gap,
            cursor.brass_gap,
            cursor.brass_played,
            cursor.pellet_played,
            cursor.impact_played,
            cursor.impact_miss_table,
            cursor.impact_miss_def,
            cursor.boom_played
        );
        cursor.logged = true;
    }
}

pub fn sync_combat_dump(cursor: &FxJournalCursor, combat: &mut CombatFxDump) {
    combat.muzzle_gap = cursor.muzzle_gap;
    combat.muzzle_played = cursor.muzzle_played;
    combat.muzzle_bolted = cursor.muzzle_bolted;
    combat.tracer_from_tag = cursor.tracer_from_tag;
    combat.tracer_gap = cursor.tracer_gap;
    combat.brass_gap = cursor.brass_gap;
    combat.brass_played = cursor.brass_played;
    combat.pellet_played = cursor.pellet_played;
    combat.explosion_played = cursor.boom_played;
    combat.explosion_gap = cursor.explosion_gap;
    combat.impact_played = cursor.impact_played;
    combat.impact_miss_table = cursor.impact_miss_table;
    combat.impact_miss_def = cursor.impact_miss_def;
    combat.tracer_skip_interval = cursor.tracer_skip_interval;
    combat.tracer_skip_short = cursor.tracer_skip_short;
    combat.tracer_skip_no_def = cursor.tracer_skip_no_def;
    combat.fire_sound_gap = cursor.fire_sound_gap;
}
