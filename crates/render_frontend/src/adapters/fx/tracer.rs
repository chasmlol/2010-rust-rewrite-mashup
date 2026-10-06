use bevy::prelude::*;
use fx_iw4::{
    FX_TRAIL_TANGENT_PACKED, FxBeamVert, beam_color_rgba, beam_generate_verts, beam_index_count,
    beam_vert_count, create_clip_matrix, trail_pack_normal, trail_pack_texcoord,
};
use render_fx::{CombatFxDump, FxWorldColorImages, TracerWorld};

use crate::adapters::fx::system::{FxCodeMeshBind, FxPresentSkip};
use crate::assemble::drawsurf::MaterialGeneration;
use crate::assemble::drawsurf::tess::fx::FxCodeMeshPlan;

pub(crate) fn present_tracer_beams(
    world: &TracerWorld,
    cam_tf: &Transform,
    clip_from_world: Option<[f32; 16]>,
    tan_half_fov: Option<(f32, f32)>,
    color_images: &FxWorldColorImages,
    runtime: &MaterialGeneration,
    plan: &mut FxCodeMeshPlan,
    combat: &mut CombatFxDump,
    bind: impl Fn(usize, &FxWorldColorImages, &MaterialGeneration) -> FxCodeMeshBind,
) {
    combat.tracer_live = world.pool.live_count() as u32;
    combat.beam_queued = world.queued.len() as u32;
    combat.beam_drawn = 0;
    combat.beam_miss_material = 0;
    combat.beam_miss_color = 0;
    combat.beam_miss_unprepared = 0;
    combat.beam_miss_ordinal = 0;
    combat.beam_miss_emissive = 0;
    combat.last_tracer_has_color = None;
    let view_pos = cam_tf.translation.to_array();
    let view_fwd = cam_tf.forward().to_array();
    let created = tan_half_fov.and_then(|(tx, ty)| {
        if tx > 0.0 && ty > 0.0 {
            let axis = [view_fwd, cam_tf.left().to_array(), cam_tf.up().to_array()];
            Some(create_clip_matrix(view_pos, axis, tx, ty))
        } else {
            None
        }
    });
    let created_inv = created.map(|c| {
        bevy::math::Mat4::from_cols_array(&c)
            .inverse()
            .to_cols_array()
    });
    let inv_clip = clip_from_world.map(|c| {
        let m = bevy::math::Mat4::from_cols_array(&c);
        m.inverse().to_cols_array()
    });
    let mut vert_buf = Vec::new();
    let mut idx_buf = Vec::new();
    for beam in &world.queued {
        let Some(asset_id) = beam.material else {
            combat.beam_miss_material = combat.beam_miss_material.saturating_add(1);
            continue;
        };
        let has_color = color_images.colors_by_asset.contains_key(&asset_id);
        combat.last_tracer_has_color = Some(i64::from(has_color));
        match bind(asset_id, color_images, runtime) {
            FxCodeMeshBind::Skip(FxPresentSkip::NoOrdinal) => {
                combat.beam_miss_ordinal = combat.beam_miss_ordinal.saturating_add(1);
            }
            FxCodeMeshBind::Ready {
                color,
                sort_key,
                ordinal,
            } => {
                let seg = beam.tess.segment_count.max(1) as usize;
                vert_buf.resize(
                    beam_vert_count(seg),
                    FxBeamVert {
                        xyz: [0.0; 3],
                        uv: [0.0; 2],
                        color: [0.0; 4],
                    },
                );
                idx_buf.resize(beam_index_count(seg), 0u16);
                let clip_pair = created
                    .as_ref()
                    .zip(created_inv.as_ref())
                    .or_else(|| clip_from_world.as_ref().zip(inv_clip.as_ref()));
                let Some((nv, ni)) = beam_generate_verts(
                    &beam.tess,
                    view_pos,
                    view_fwd,
                    clip_pair.map(|(c, i)| (c, i)),
                    &mut vert_buf,
                    &mut idx_buf,
                ) else {
                    continue;
                };
                let slot = plan.begin_material_draw(color, sort_key, Some(ordinal));
                let vert_used = plan.mesh.vert_used;
                let index_used = plan.mesh.index_used;
                let mut verts_ok = true;
                let normal = trail_pack_normal([0.0, 1.0, 0.0]);
                for v in &vert_buf[..nv] {
                    if !plan.push_trail_vert(
                        v.xyz,
                        beam_color_rgba(v.color),
                        trail_pack_texcoord(v.uv[0], v.uv[1]),
                        normal,
                        FX_TRAIL_TANGENT_PACKED,
                    ) {
                        verts_ok = false;
                        break;
                    }
                }
                if !verts_ok {
                    plan.shrink_verts_to(vert_used);
                    plan.end_material_draw(slot);
                    continue;
                }
                let base = vert_used;
                let mut index_ok = true;
                for tri in idx_buf[..ni].chunks_exact(3) {
                    let three = [
                        base.saturating_add(u32::from(tri[0])),
                        base.saturating_add(u32::from(tri[1])),
                        base.saturating_add(u32::from(tri[2])),
                    ];
                    if !plan.extend_indices(&three) {
                        index_ok = false;
                        break;
                    }
                }
                if !index_ok {
                    plan.shrink_verts_to(vert_used);
                    plan.shrink_indices_to(index_used);
                    plan.end_material_draw(slot);
                    continue;
                }
                plan.end_material_draw(slot);
                combat.beam_drawn = combat.beam_drawn.saturating_add(1);
            }
        }
    }
}
