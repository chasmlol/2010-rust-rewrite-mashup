#![no_std]
#![forbid(unsafe_code)]

mod angles;
mod at_rest;
mod atlas;
mod beam;
mod bolt;
mod collide;
mod cull;
mod draw;
mod effect_def;
mod elem;
mod elem_def;
mod emit;
mod flags;
mod glass;
mod glass_crack;
mod glass_geo;
mod glass_shard;
mod glass_shatter;
mod gravity;
mod impact;
mod integrate;
mod laser;
mod life;
mod lighting;
mod orient;
mod orientation;
mod origin;
mod particle_cloud;
mod pool;
mod post_light;
mod quat;
mod random;
mod rotate_axis;
mod rotation;
mod sort;
mod spark_fountain;
mod spawn;
mod sprite_quad;
mod status;
mod system;
mod tail_draw;
mod trail;
mod trail_def;
mod trail_runtime;
mod vec;
mod velocity;
mod view;
mod vis_blocker;
mod visual;

pub use angles::{angles_to_axis_radians, get_elem_angles_axis, mat3_mul, sample_elem_angles};

pub use at_rest::{
    FX_AT_REST_BIAS, FX_AT_REST_SCALE, FX_ON_GROUND_NORMAL_Z, FX_RECIP_255_AT_REST,
    get_at_rest_fraction, msec_for_sampling_axis,
};
pub use atlas::{
    FX_ELEM_ATLAS_OFF, FX_ELEM_ATLAS_SIZE, FxSpriteAtlasUv, sprite_atlas_cell, sprite_atlas_uv,
};
pub use beam::{
    FX_BEAM_ADD_CAP, FX_BEAM_CLIP_DZ_AND, FX_BEAM_CLIP_PLANE_Z1, FX_BEAM_CLIP_T_MAX,
    FX_BEAM_CLIP_W_BIAS, FX_BEAM_FLAT_DELTA_MIN_LEN_SQ, FX_BEAM_MAX_SEGMENTS,
    FX_BEAM_VIEWER_SHUFFLE0, FX_BEAM_VIEWER_SHUFFLE1, FX_BEAM_VIEWER_SHUFFLE2,
    FX_BEAM_VIEWER_SHUFFLE3, FX_BEAM_WIGGLE, FX_CREATE_CLIP_ZNEAR, FX_INFINITE_PERSPECTIVE_K,
    FX_TRACER_FIRST_PERSON_MAX_WIDTH, FX_TRACER_MIN_DIST, FxBeamTess, FxBeamVert,
    beam_clip_against_plane, beam_clip_pair_z_planes, beam_color_rgba, beam_flat_basis,
    beam_generate_verts, beam_index_count, beam_perp_from_flat, beam_sample_color,
    beam_segment_count, beam_vert_count, beam_viewer_pshufb, create_clip_matrix,
    generate_beam_get_flat_delta, infinite_perspective_matrix, mat4_mul, mat4_mul_vec4,
    matrix_for_viewer,
};
pub use bolt::{
    BoneOrientationRefuse, FX_BOLT_BONE_MASK, FX_BOLT_BONE_SHIFT, FX_BOLT_CENTITY_LIMIT,
    FX_BOLT_CENTITY_STRIDE, FX_BOLT_CENTITY_TELEPORT_MASK, FX_BOLT_DOBJ_MASK, FX_BOLT_FREE_NONE,
    FX_BOLT_HANDLE_NONE, FX_BOLT_INIT_LAST, FX_BOLT_LOST_OR, FX_BOLT_PARENT_DWORDS,
    FX_BOLT_PARENT_IDENTITY_QUAT, FX_BOLT_RECORD_CAPACITY, FX_BOLT_RECORD_OFF_PACKED,
    FX_BOLT_RECORD_OFF_PARENT, FX_BOLT_RECORD_STRIDE, FX_BOLT_TELEPORT_SHIFT,
    FX_BOLT_VIEWMODEL_DOBJ_BASE, FX_QUAT_TRANSFORM_SCALE, FxGetBoneOrientationRoute,
    FxUpdateEffectBolt, begin_iterating_over_effects_exclusive, bolt_alloc, bolt_bone,
    bolt_centity_teleport_for_compare, bolt_compose_orientation, bolt_dobj, bolt_handle_is_none,
    bolt_init_next_index, bolt_init_parent_orientation, bolt_mark_lost, bolt_pack,
    bolt_record_index_from_byte_delta, bolt_spawn_teleport_bit, bolt_teleport_bit,
    end_iterating_over_effects, end_iterating_runs_gc, get_bone_orientation_route, quat_mul,
    quat_transform_vec, stop_effect_has_owned, stop_effect_non_recursive_allows,
    update_effect_bolt,
};
pub use collide::{
    FX_COLLIDE_SUBSTEP_MS, FX_COLLISION_REFLECT_SCALE, FX_IMPACT_CHILD_MIN_SPEED_SQ, FX_TRACE_MASK,
    FX_TRACE_MASK_ITEM_CLIP, FxCollideSubstep, collide_marks_at_rest, collide_on_ground,
    collide_substep_schedule, collision_reflect_base_vel_delta, impact_child_speed_allows,
    sample_reflection_factor, trace_mask,
};
pub use cull::{
    FX_ELEM_FLAG_CULL_DRAW_5_PLANES, cull_cloud, cull_cloud_plane_count, cull_cloud_radius,
    cull_elem_for_spawn_allows, cull_elem_light, cull_sphere, elem_light_color_bgr,
};
pub use draw::{FX_DRAW_ELEM_HANDLER_PRESENT, FxElemType, draw_elem_handler_present};
pub use effect_def::FxEffectDef;
pub use elem::{FX_ELEM_AT_REST_NONE, FxElem};
pub use elem_def::FxElemDef;
pub use emit::{
    FX_ELEM_EMIT_ORIENT_AXIS, FX_EMIT_CRT_RAND_SCALE, FX_EMIT_DIST_TO_RESIDUAL,
    FX_EMIT_RESIDUAL_ROUND_BIAS, FX_EMIT_RESIDUAL_TO_DIST, FX_EMIT_SPAWN_CAP, FxEmitSchedule,
    FxEmitSpawn, emit_dist_range, emit_lerp_origin, emit_pack_residual, emit_unpack_residual_start,
    process_emitting_schedule,
};
pub use flags::{
    FX_ELEM_DIE_ON_TOUCH, FX_ELEM_RUN_MASK, FX_ELEM_RUN_RELATIVE_TO_EFFECT,
    FX_ELEM_RUN_RELATIVE_TO_OFFSET, FX_ELEM_RUN_RELATIVE_TO_SPAWN, FX_ELEM_RUNNER_USES_RAND_ROT,
    FX_ELEM_SPAWN_FRUSTUM_CULL, FX_ELEM_UPDATE_HAS_VEL_GRAPH, FX_ELEM_USE_COLLISION,
    FX_ELEM_USE_MODEL_PHYSICS, FX_ELEM_VEL_LOCAL, FX_ELEM_VEL_WORLD, elem_dies_on_touch,
    elem_run_mode, elem_skips_position_update, elem_spawn_frustum_cull,
    elem_update_has_velocity_graph, elem_uses_collision, elem_uses_vel_local, elem_uses_vel_world,
};
pub use glass::{
    FX_GLASS_AVEL_HALF, FX_GLASS_DEF, FX_GLASS_DYN_AVEL, FX_GLASS_DYN_FALL_TIME,
    FX_GLASS_DYN_PHYS_OBJ, FX_GLASS_DYN_VEL, FX_GLASS_FALL_GRAVITY, FX_GLASS_FALL_TIME_NEVER,
    FX_GLASS_FREE_SENTINEL, FX_GLASS_GEOMETRY_DATA, FX_GLASS_INIT_AREA_X2, FX_GLASS_INIT_DEF_INDEX,
    FX_GLASS_INIT_FAN_DATA_COUNT, FX_GLASS_INIT_ORIGIN, FX_GLASS_INIT_PIECE_STATE,
    FX_GLASS_INIT_PLACE_BYTES, FX_GLASS_INIT_SUPPORT_MASK, FX_GLASS_INIT_TEXCOORD,
    FX_GLASS_INIT_VERT_COUNT, FX_GLASS_LINK_ORG_FREE, FX_GLASS_MSEC_TO_SEC,
    FX_GLASS_PIECE_DYNAMICS, FX_GLASS_PIECE_PLACE, FX_GLASS_PIECE_STATE, FX_GLASS_SHATTERED_SCALE,
    FX_GLASS_STATE_AREA_X2, FX_GLASS_STATE_CRACK_DATA_COUNT, FX_GLASS_STATE_DEF_INDEX,
    FX_GLASS_STATE_FAN_DATA_COUNT, FX_GLASS_STATE_FLAGS, FX_GLASS_STATE_GEO_DATA_START,
    FX_GLASS_STATE_HOLE_DATA_COUNT, FX_GLASS_STATE_INIT_INDEX, FX_GLASS_STATE_SUPPORT_MASK,
    FX_GLASS_STATE_VERT_COUNT, FX_GLASS_TRACE_INTERVAL_MSEC, FX_GLASS_VERT_SCALE,
    FxGlassIntactVert, FxGlassResetPiece, FxGlassSlabVert, FxGlassVertXform, glass_alloc_piece,
    glass_ballistic_origin, glass_clear_in_use, glass_def_color_rgba, glass_def_tex_vecs,
    glass_dynamics_avel, glass_dynamics_fall_time, glass_dynamics_init_row,
    glass_dynamics_phys_obj, glass_dynamics_software_launch, glass_dynamics_vel, glass_emit_slab,
    glass_free_piece, glass_geo_vert, glass_in_use_mask, glass_in_use_word, glass_init_origin,
    glass_intact_verts, glass_is_in_use, glass_last_trace_tick, glass_pack_geo_vert,
    glass_piece_tex_vecs, glass_piece_verts, glass_place_next_free, glass_place_origin,
    glass_place_quat, glass_place_radius, glass_place_set_next_free, glass_place_set_origin,
    glass_place_set_quat, glass_reset_copy_geo, glass_reset_copy_piece, glass_reset_free_list,
    glass_set_in_use, glass_slab_counts, glass_software_rotate_quat, glass_software_trace_due,
    glass_state_area_x2, glass_state_crack_count, glass_state_def_index, glass_state_fan_count,
    glass_state_flags, glass_state_geo_span, glass_state_geo_start, glass_state_hole_count,
    glass_state_init_index, glass_state_set_flags, glass_state_set_geo_start,
    glass_state_set_support_mask, glass_state_support_mask, glass_state_vert_count,
    glass_trace_phase, unit_quat_to_axis,
};
pub use glass_crack::{
    FX_GLASS_CRACK_BRANCH_MAX, FX_GLASS_CRACK_EDGE_MAX, FX_GLASS_CRACK_LOOP_MAX,
    FX_GLASS_CRACK_PT_MAX, FX_GLASS_EDGE_BORDER, FX_GLASS_EDGE_CRACK, FX_GLASS_EDGE_NONE,
    FX_GLASS_EDGE_SUPPORTED, FxGlassClipSegment, FxGlassCrackBranch, FxGlassCrackEdge,
    FxGlassCrackLoop, FxGlassCrackRand, FxGlassCrackWalk, FxGlassCrackWork,
};
pub use glass_geo::{
    FX_GLASS_CRACK_VERT_FREE, FX_GLASS_SHARD_CRACK_MAX, FX_GLASS_SHARD_GEO_MAX,
    FX_GLASS_SHARD_HOLE_MAX, FX_GLASS_SHARD_TRI_MAX, FX_GLASS_SHARD_VERT_MAX, FxGlassGeoCrack,
    FxGlassGeoSpan, FxGlassPieceGeo, glass_clamp_to_piece, glass_contour_area_x2, glass_decode_geo,
    glass_encode_fans, glass_fan_word_count, glass_geo_count, glass_pack_crack_header,
    glass_pack_geo_count, glass_pack_verts, glass_point_in_contour, glass_point_in_piece,
    glass_tri_count, glass_triangulate,
};
pub use glass_shard::{FX_GLASS_SHARD_MAX, FxGlassShard, glass_extract_shards};
pub use glass_shatter::{
    FX_GLASS_ACCENT_BOUNCE_CAP, FX_GLASS_AIRBORNE_CAP, FX_GLASS_AIRBORNE_PER_BREAK,
    FX_GLASS_ANGULAR_VEL_MAX, FX_GLASS_ANGULAR_VEL_MIN, FX_GLASS_CATCHUP_STEPS,
    FX_GLASS_FRINGE_MAXCOVERAGE, FX_GLASS_FRINGE_MAXSIZE, FX_GLASS_LANDING_AGGREGATE_MSEC,
    FX_GLASS_LANDING_CELL, FX_GLASS_LINEAR_VEL_MAX, FX_GLASS_LINEAR_VEL_MIN,
    FX_GLASS_MAX_PIECES_PER_FRAME, FX_GLASS_MOTION_STEP_MSEC, FX_GLASS_PENDING_MAX_MSEC,
    FX_GLASS_PENDING_MIN_MSEC, FX_GLASS_PENDING_SUPPORT_FRAC, FX_GLASS_RESTITUTION,
    FX_GLASS_SETTLED_CAP, FX_GLASS_SETTLED_FADE_MSEC, FX_GLASS_SETTLED_LIFETIME_MSEC,
    FX_GLASS_SHARD_LIFETIME_MSEC, FX_GLASS_SHARD_MAXSIZE, FX_GLASS_SHATTER_BRANCH_SCALE,
    FX_GLASS_SHATTER_FX_32, FX_GLASS_SHATTER_FX_64, FX_GLASS_SHATTER_FX_PER_FRAME,
    FX_GLASS_SHATTER_FX_PIECE, FX_GLASS_SHATTER_TWO_PI, FX_GLASS_SPLIT_OP_CAP,
    FX_GLASS_STATE_FLAG_CHILD_CLEAR, FX_GLASS_STATE_FLAG_DAMAGED, FX_GLASS_STATE_FLAG_SIMPLE,
    glass_centroid, glass_cross3, glass_fringe_cap, glass_interior_branch_count, glass_launch_avel,
    glass_launch_dir, glass_lerp_range, glass_life_fade, glass_loop_area_x2,
    glass_needs_size_split, glass_normalize3, glass_piece_speed_scale, glass_recenter_offset,
    glass_scale_color_alpha, glass_shard_size_cap, glass_shatter_fx_fallback,
    glass_shatter_fx_name, glass_shatter_rand, glass_splitmix64, glass_support_frac,
};
pub use gravity::{
    FX_GRAVITY, elem_gravity_accel_z, elem_gravity_accel_z_sampled, sample_gravity_authored,
};
pub use impact::{
    FX_IMPACT_ENTRY_SIZE, FX_IMPACT_EXIT_SURFACE_FLAG, FX_IMPACT_FLESH_COUNT,
    FX_IMPACT_NONFLESH_COUNT, FX_IMPACT_TABLE_ROWS, FX_SURF_TYPE_FLESH, FxImpactEntry,
    FxImpactTable, ImpactType, flesh_effect_index, flesh_hit_flags, impact_entry_cell_offset,
    impact_table_row, surface_type_index,
};
pub use integrate::{
    FX_SPARKCLOUD_HISTORY_FAR_MS, FX_SPARKCLOUD_HISTORY_NEAR_MS, FxElemVec3Range,
    integrate_velocity_graph, particle_cloud_cell_count, sample_vel_graph_at_age,
    sparkcloud_history_lookback_ms,
};
pub use laser::{
    FX_LASER_POST_LIGHT_COLOR, FX_LASER_POST_LIGHT_HALF, FX_LASER_POST_LIGHT_MATERIAL,
    FX_LASER_POST_LIGHT_MIN_SPAN, FX_LASER_POST_LIGHT_PAD, FX_LASER_RADIUS_DIST_BIAS,
    FX_LASER_RADIUS_DIST_SCALE, FX_LASER_SURF_EXTRA_END, FX_LASER_TAG, FX_LASER_TRACE_BOUNDS,
    laser_brush_trace_allows, laser_from_brush_trace, laser_from_tag_orientation,
    laser_point_on_ray, laser_post_light, laser_post_light_allows, laser_post_light_end_t,
    laser_post_light_span, laser_radius_from_dist,
};
pub use life::{
    life_span_range_from_bytes, sample_life_span_msec, trail_elem_base_vel_z_pack, trail_elem_keep,
    trail_elem_norm_ages,
};
pub use lighting::{
    FX_LIGHTING_FRAC_CHANNEL_MAP, apply_lighting_frac_bgra, apply_lighting_frac_channel,
    effect_def_needs_lighting_sample, elem_uses_lighting_frac,
};
pub use orient::{orientation_pos_from_world, orientation_pos_to_world};
pub use orientation::{
    FX_ORIENT_UP_DOT_GATE, FxOrientFrame, FxOrientSpawnParams, FxOrientation, get_orientation,
};
pub use origin::{
    FX_ELEM_SPAWN_OFFSET_CYLINDER, FX_ELEM_SPAWN_OFFSET_MASK, FX_ELEM_SPAWN_OFFSET_SPHERE,
    FX_ELEM_SPAWN_RELATIVE, FX_TWO_PI, FxSpawnOffsetMode, apply_spawn_origin,
    elem_spawn_offset_mode, elem_spawn_relative, offset_spawn_origin, random_dir,
    sample_float_range, sample_spawn_origin_offset, spawn_origin_world, world_delta_to_local,
};
pub use particle_cloud::{
    FX_CODE_FOUNTAIN_PARM0, FX_CODE_FOUNTAIN_PARM1, FX_CODE_PARTICLE_CLOUD_COLOR,
    FX_CODE_PARTICLE_CLOUD_MATRIX0, FX_CODE_SPARK_COLOR0, FX_PARTICLE_CLOUD_CELL_ORIGIN,
    FX_PARTICLE_CLOUD_CELL_SCALE_XY, FX_PARTICLE_CLOUD_CELL_SCALE_Z,
    FX_PARTICLE_CLOUD_CRT_RAND_MAX, FX_PARTICLE_CLOUD_FLAG_SPARK, FX_PARTICLE_CLOUD_GRID_X,
    FX_PARTICLE_CLOUD_GRID_Y, FX_PARTICLE_CLOUD_GRID_Z, FX_PARTICLE_CLOUD_INDICES_PER_CELL,
    FX_PARTICLE_CLOUD_PRIM_TYPE, FX_PARTICLE_CLOUD_PRIMS_PER_CELL, FX_PARTICLE_CLOUD_QUAD_INDICES,
    FX_PARTICLE_CLOUD_TEMPLATE_CELLS, FX_PARTICLE_CLOUD_UV, FX_PARTICLE_CLOUD_VERT_DECL_TYPE,
    FX_PARTICLE_CLOUD_VERTS_PER_CELL, FX_PARTICLE_FOUNTAIN_AGE_BIAS_MS,
    FX_PARTICLE_FOUNTAIN_AGE_SCALE, FX_PARTICLE_SPARK_INDICES, FX_PARTICLE_SPARK_INDICES_PER_CELL,
    FX_PARTICLE_SPARK_PRIMS_PER_CELL, FX_PARTICLE_SPARK_UV, FX_PARTICLE_SPARK_VERTS_PER_CELL,
    FX_SPARK_CLOUD_HANDLE_NONE, FX_SPARK_CLOUD_HISTORY_CAPACITY, FX_SPARK_CLOUD_HISTORY_STRIDE,
    FX_SPARK_CLOUD_SAMPLE_MASK, FX_SPARK_CLOUD_SAMPLE_RING, FX_SPARKCLOUD_HISTORY_MIN_DT_MS,
    FX_SPARKCLOUD_UV_V_1_3, FX_SPARKCLOUD_UV_V_2_3, FxSparkCloudHistory, GFX_PARTICLE_CLOUD_STRIDE,
    GFX_POS_TEX_VERTEX_STRIDE, GfxParticleCloud, GfxPosTexVertex, MSVCRT_HOLDRAND_DEFAULT,
    build_cloud, empty_particle_cloud, gfx_pos_tex_vertex_bytes, msvcrt_rand, msvcrt_rand01,
    pack_gfx_color, particle_cloud_cell_indices, particle_cloud_cell_radius_sq,
    particle_cloud_cell_verts, particle_cloud_cell_xyz, particle_cloud_color_const,
    particle_cloud_compare_cell_radius, particle_cloud_draw_cell_count, particle_cloud_draw_counts,
    particle_cloud_matrix_diag, particle_cloud_particle_id, particle_fountain_parm0,
    particle_spark_cell_indices, particle_spark_cell_verts, spark_cloud_addr,
    spark_cloud_handle_for_slot, spark_cloud_handle_from_ptr_delta, sparkcloud_build_triplet,
    sparkcloud_fill_sample, sparkcloud_history_advance, sparkcloud_history_should_advance,
    sparkcloud_lerp_sample, sparkcloud_tent_weights,
};
pub use pool::{
    FX_BUFFERS_OFF_EFFECTS, FX_BUFFERS_OFF_ELEMS, FX_BUFFERS_OFF_SPARK_CLOUD,
    FX_BUFFERS_POOL_STRIDE, FX_EFFECT_HANDLE_RING_MASK, FX_EFFECT_HANDLE_RING_SIZE,
    FX_EFFECT_POOL_BYTES, FX_EFFECT_POOL_CAPACITY, FX_EFFECT_SLOT_SIZE, FX_ELEM_POOL_CAPACITY,
    FX_ELEM_RUNTIME_STRIDE, FX_ENTITYNUM_WORLD, FX_PLAY_BOLT_NONE, FX_SPAWN_BOLT_NONE,
    FX_SPOT_LIGHT_LIMIT, FX_STATUS_UNIQUE_DONE, FX_STATUS_UNIQUE_MASK, FX_SYSTEM_STRIDE,
    FX_TRAIL_ELEM_POOL_CAPACITY, FX_TRAIL_ELEM_RUNTIME_STRIDE, FX_TRAIL_POOL_CAPACITY,
    FX_TRAIL_RUNTIME_STRIDE, FX_WARN_EFFECT_LIMIT, FX_WARN_ELEM_LIMIT, FX_WARN_SPARK_CLOUD_LIMIT,
    FX_WARN_TOO_MANY_SPOTLIGHTS, effect_addr, effect_byte_offset_from_handle,
    effect_handle_for_slot, effect_handle_from_byte_offset, elem_addr, elem_handle_from_ptr_delta,
    trail_addr, trail_elem_addr, trail_elem_handle_for_slot, trail_handle_for_slot,
    trail_handle_from_byte_offset,
};
pub use post_light::{
    FX_POST_LIGHT_ADD_CAP, FX_POST_LIGHT_ANGLE_STEP, FX_POST_LIGHT_ARG_COUNT,
    FX_POST_LIGHT_DRAW_NAME, FX_POST_LIGHT_INDEX_COUNT, FX_POST_LIGHT_MIN_DELTA_SQ,
    FX_POST_LIGHT_POLYGON_RADIUS_GROW, FX_POST_LIGHT_STRIDE, FX_POST_LIGHT_VERT_COUNT, FxPostLight,
    FxPostLightTess, post_light_add_allows, post_light_generate_verts, post_light_pack_vert,
};
pub use quat::{axis_to_quat, quat_nlerp, quat_normalize};
pub use random::{
    FX_RANDOM_VERSION, FxRandomChannel, effect_random_key, elem_random_seed, elem_visual_index,
    sample_at, sample_f32, sample_u16, trail_random_seed,
};
pub use rotate_axis::{
    FX_DEG_TO_RAD, FX_RAD_TO_DEG, FX_RAND_ROT_DEGREES, impact_mark_axis, randomly_rotate_axis,
    rotate_point_around_vector, runner_rand_rot_degrees,
};
pub use rotation::{
    FX_RECIP_255, FX_ROT_TIME_EASE_LIMIT_MS, FX_ROT_TIME_EASE_RECIP, FX_ROT_TIME_MAX_LEAD_MS,
    clamp_elem_rotation_time,
};
pub use sort::{FxInsertSortElem, existing_elem_sorts_before_new, sort_dist_to_cam_sq};
pub use spark_fountain::{
    FX_ELEM_FLAG_FOUNTAIN_WRITE_SPARK_N, FX_SPARK_FOUNTAIN_BALLISTIC_HALF,
    FX_SPARK_FOUNTAIN_BOOST_HALF_PI, FX_SPARK_FOUNTAIN_CELL_OFF_ORIGIN,
    FX_SPARK_FOUNTAIN_CELL_OFF_TIMES, FX_SPARK_FOUNTAIN_CELL_OFF_VEL, FX_SPARK_FOUNTAIN_CELLS,
    FX_SPARK_FOUNTAIN_CLUSTER_CAPACITY, FX_SPARK_FOUNTAIN_CLUSTER_MESH_MAX,
    FX_SPARK_FOUNTAIN_CLUSTER_OFF_KEYFRAME, FX_SPARK_FOUNTAIN_CLUSTER_OFF_MESH_IDX,
    FX_SPARK_FOUNTAIN_CLUSTER_OFF_READY, FX_SPARK_FOUNTAIN_CLUSTER_OFF_SPARK_N,
    FX_SPARK_FOUNTAIN_CLUSTER_OFF_WRITE, FX_SPARK_FOUNTAIN_CLUSTER_STRIDE,
    FX_SPARK_FOUNTAIN_CONE_DOT_GATE, FX_SPARK_FOUNTAIN_CONE_FLIP,
    FX_SPARK_FOUNTAIN_DEF_OFF_SPARK_COUNT, FX_SPARK_FOUNTAIN_DEF_SIZE,
    FX_SPARK_FOUNTAIN_HANDLE_NONE, FX_SPARK_FOUNTAIN_HIT_TIME_EPS, FX_SPARK_FOUNTAIN_HIT_TIME_FOUR,
    FX_SPARK_FOUNTAIN_INDICES_PER_CELL, FX_SPARK_FOUNTAIN_INTEGRATE_BUDGET,
    FX_SPARK_FOUNTAIN_INTEGRATE_CELLS, FX_SPARK_FOUNTAIN_KEYFRAME_STEP,
    FX_SPARK_FOUNTAIN_KEYFRAME_STRIDE, FX_SPARK_FOUNTAIN_MESH_CAPACITY,
    FX_SPARK_FOUNTAIN_MESH_STRIDE, FX_SPARK_FOUNTAIN_RAND_CUBE, FX_SPARK_FOUNTAIN_SAMPLES,
    FX_SPARK_FOUNTAIN_TIME_SENTINEL, FX_SPARK_FOUNTAIN_TRACE_BOUNDS, FX_SPARK_FOUNTAIN_TRACE_GROW,
    FX_SPARK_FOUNTAIN_TRACE_LOOKAHEAD_ACCEL, FX_SPARK_FOUNTAIN_TRACE_LOOKAHEAD_VEL,
    FX_SPARK_FOUNTAIN_TRACE_MASK, FX_SPARK_FOUNTAIN_TRACE_SUBSTEP,
    FX_SPARK_FOUNTAIN_VERTS_PER_SPARK, FxSparkFountainTrace, R_PARTICLE_CLOUD_CUSTOM_CAP,
    add_particle_cloud_custom_allows, reserve_particle_cloud_verts_allows,
    spark_fountain_accel_from_gravity, spark_fountain_atlas_uv, spark_fountain_ballistic,
    spark_fountain_boost, spark_fountain_bounce_vel, spark_fountain_cell_indices,
    spark_fountain_cell_verts, spark_fountain_cluster_draw_allows, spark_fountain_cone_dir,
    spark_fountain_cone_is_isotropic, spark_fountain_def_allows_draw,
    spark_fountain_draw_clouds_allows, spark_fountain_generate_ribbon,
    spark_fountain_handle_for_slot, spark_fountain_hit_time, spark_fountain_hit_time_abs,
    spark_fountain_index_count, spark_fountain_integrate_cell, spark_fountain_integrate_cell_begin,
    spark_fountain_integrate_miss_cell, spark_fountain_isotropic_dir, spark_fountain_mark_ready,
    spark_fountain_prim_count, spark_fountain_reserve_verts, spark_fountain_same_sample_ribbon,
    spark_fountain_sample_window, spark_fountain_slot_for_handle, spark_fountain_spark_n_clamped,
    spark_fountain_speed, spark_fountain_spray_dir, spark_fountain_trace_start,
    spark_fountain_trace_until_miss_or_hit, spark_fountain_update_keyframe_cursor,
    spark_fountain_vel_at_time, spark_fountain_wrap_loop_time,
};
pub use spawn::{
    FX_ELEM_TYPE_SPARK_CLOUD, FX_ELEM_TYPE_SPARK_FOUNTAIN, FX_ELEM_TYPE_TRAIL, FxLoopingSpawn,
    FxLoopingSpawnSchedule, looping_catchup_begin, looping_spawn_schedule,
    sample_oneshot_spawn_count, spawn_def_from_bytes, spawn_effect_status,
};
pub use sprite_quad::{
    FX_SPRITE_QUAD_INDICES, FX_SPRITE_QUAD_LOCAL_XY, FX_SPRITE_QUAD_UV_FULL, sprite_quad_indices,
};
pub use status::{
    FX_STATUS_DEFER_UPDATE, FX_STATUS_HAS_PENDING_LOOP_ELEMS, FX_STATUS_IS_LOCKED,
    FX_STATUS_IS_LOCKED_MASK, FX_STATUS_OWNED_EFFECTS_MASK, FX_STATUS_REF_COUNT_MASK,
    FX_STATUS_REF_COUNT_MASK_IW4, status_is_unique_done,
};
pub use system::{FxEffect, FxSystem};
pub use tail_draw::{tail_anchor_origin, tail_sprite_axes, tail_sprite_full_extent};
pub use trail::{
    FX_CODE_MESH_BINORMAL_SIGN, FX_CODE_MESH_VERTEX_STRIDE, FX_TRAIL_BASIS_SCALE,
    FX_TRAIL_NORMAL_BIAS, FX_TRAIL_NORMAL_SCALE, FX_TRAIL_TANGENT_PACKED, FxTrailEmittedVert,
    FxTrailSegmentDrawState, compress_basis_from_axis, compress_basis_from_quat,
    pack_code_mesh_vertex, pack_code_mesh_vertex_signed, trail_compress_basis, trail_compress_char,
    trail_compute_u, trail_emit_index_quad, trail_emit_segment_vert, trail_emit_segment_verts,
    trail_index_quad_tris, trail_pack_normal, trail_pack_texcoord, trail_uncompress_basis,
};
pub use trail_def::{FxTrailDef, FxTrailVertex};
pub use trail_runtime::{
    FX_TRAIL_SPLIT_UNIT, FxTrail, FxTrailElem, FxTrailSplit, trail_split_interpolant_msec,
    trail_split_interpolant_t, trail_split_lerp_axis, trail_split_lerp_origin,
    trail_split_lerp_quat, trail_split_skips_update, trail_split_window,
};
pub use vec::{
    FX_PERP_VECTOR_UNIT, effect_orient_arc, perpendicular_vector, vec3_distance, vec3_length_sq,
    vec3_normalize, vector_vectors,
};
pub use velocity::{FX_VEL_AT_TIME_SCALE, get_velocity_at_time};
pub use view::{
    FX_EFFECT_DEF_SIZE, FX_ELEM_DEF_STRIDE, FxEffectDefView, FxElemDefView, effect_def_view,
    elem_def_gravity_accel_z, elem_def_view, elem_def_view_x64,
};
pub use vis_blocker::{
    FX_CLIENT_VISIBILITY_THRESHOLD, FX_DISTANCE_FADE_BIAS, FX_DISTANCE_FADE_SCALE,
    FX_ELEM_FLAG_VIS_BLOCKER, FX_VIS_BLOCKER_BYTE_TO_UNIT, FX_VIS_BLOCKER_PARAM3_SCALE,
    FX_VIS_BLOCKER_PARAM4_INV_SCALE, FX_VIS_BLOCKER_REC_STRIDE, FX_VIS_BLOCKER_SLOT_CAP,
    FX_VIS_MIN_TRACE_DIST_DEFAULT, FxVisBlockerBuf, FxVisBlockerRec, distance_fade_range,
    evaluate_distance_fade, get_client_visibility, vis_blocker_add, vis_blocker_add_prepared,
    vis_blocker_generate_verts, vis_blocker_param4,
};
pub use visual::{
    FX_ELEM_VIS_STATE_SAMPLE_SIZE, FX_ELEM_VISUAL_STATE_SIZE, FX_VIS_COLOR_OFF,
    FX_VIS_ROT_DELTA_OFF, FX_VIS_ROT_TOTAL_OFF, FX_VIS_SCALE_OFF, FX_VIS_SIZE0_OFF,
    FX_VIS_SIZE1_OFF, elem_norm_time, evaluate_color_bgra, evaluate_rotation_total, evaluate_scale,
    evaluate_size0, evaluate_size1, evaluate_vis_alpha, integrate_rotation_from_zero,
    setup_visual_sample_point,
};
