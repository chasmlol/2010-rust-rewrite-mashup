use fx_iw4::{
    FX_EFFECT_HANDLE_RING_SIZE, FX_EFFECT_POOL_CAPACITY, FX_EFFECT_SLOT_SIZE,
    FX_ELEM_POOL_CAPACITY, FX_SPARK_CLOUD_HANDLE_NONE, FX_SPARK_CLOUD_HISTORY_CAPACITY,
    FX_SPARK_FOUNTAIN_CLUSTER_CAPACITY, FX_SPARK_FOUNTAIN_HANDLE_NONE,
    FX_SPARK_FOUNTAIN_MESH_CAPACITY, FX_SPAWN_BOLT_NONE, FX_SPOT_LIGHT_LIMIT,
    FX_STATUS_REF_COUNT_MASK_IW4, FX_TRAIL_ELEM_POOL_CAPACITY, FX_TRAIL_POOL_CAPACITY,
    FxOrientFrame, bolt_alloc, bolt_init_next_index, effect_handle_for_slot, effect_random_key,
    status_is_unique_done,
};

use crate::elem::{FX_ELEM_HANDLE_NONE, FxElemSlot};
use crate::gaps::FxGaps;
use crate::trail::{FX_TRAIL_HANDLE_NONE, FxTrailElemSlot, FxTrailSlot};

pub const FX_CATALOG_INDEX_NONE: u16 = u16::MAX;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxBoltOrientation {
    pub origin: [f32; 3],
    pub axis: [[f32; 3]; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxBoltTarget {
    pub dobj: u32,
    pub bone: u16,
    pub centity_teleport: bool,
    pub orientation: FxBoltOrientation,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxResolvedBoltPose {
    pub centity_teleport: bool,
    pub orientation: Option<FxBoltOrientation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FxPackedLightingSrc {
    White = 0,

    AtPointRgb = 1,

    Missing = 2,
}

#[derive(Clone, Debug)]
pub struct FxEffectSlot {
    pub def_name: String,

    pub catalog_index: u16,

    pub status: u32,

    pub first_elem_handle: [u16; 3],

    pub first_sorted_elem_handle: u16,

    pub first_trail_handle: u16,

    pub random_seed: u64,

    pub own_handle: u16,

    pub packed_lighting: [u8; 3],

    pub packed_lighting_src: FxPackedLightingSrc,

    pub bolt: u8,

    pub mark_entity: Option<u16>,

    pub bolt_packed: u32,

    pub bolt_centity_teleport: bool,

    pub bolt_parent_quat: [f32; 4],

    pub bolt_parent_origin: [f32; 3],

    pub bolt_bone_pose: Option<([f32; 3], [[f32; 3]; 3])>,

    pub frame_stamp: i32,

    pub msec_begin: i32,

    pub msec_last_update: i32,

    pub origin: [f32; 3],

    pub axis: [[f32; 3]; 3],

    pub origin_when_played: [f32; 3],
    pub axis_when_played: [[f32; 3]; 3],

    pub origin_last: [f32; 3],
    pub axis_last: [[f32; 3]; 3],

    pub distance: f32,
    pub ring_resident: bool,
}

impl Default for FxEffectSlot {
    fn default() -> Self {
        Self {
            def_name: String::new(),
            catalog_index: FX_CATALOG_INDEX_NONE,
            status: 0,
            first_elem_handle: [FX_ELEM_HANDLE_NONE; 3],
            first_sorted_elem_handle: FX_ELEM_HANDLE_NONE,
            first_trail_handle: FX_ELEM_HANDLE_NONE,
            random_seed: 0,
            own_handle: 0,
            packed_lighting: [0xff; 3],
            packed_lighting_src: FxPackedLightingSrc::White,
            bolt: 0xff,
            mark_entity: None,
            bolt_packed: fx_iw4::FX_BOLT_HANDLE_NONE,
            bolt_centity_teleport: false,
            bolt_parent_quat: fx_iw4::FX_BOLT_PARENT_IDENTITY_QUAT,
            bolt_parent_origin: [0.0; 3],
            bolt_bone_pose: None,
            frame_stamp: -1,
            msec_begin: 0,
            msec_last_update: 0,
            origin: [0.0; 3],
            axis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            origin_when_played: [0.0; 3],
            axis_when_played: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            origin_last: [0.0; 3],
            axis_last: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            distance: 0.0,
            ring_resident: false,
        }
    }
}

impl FxEffectSlot {
    pub fn has_refs(&self) -> bool {
        (self.status & FX_STATUS_REF_COUNT_MASK_IW4) != 0
    }

    pub fn frame_now(&self) -> FxOrientFrame {
        FxOrientFrame {
            origin: self.origin,
            axis: self.axis,
        }
    }

    pub fn frame_when_played(&self) -> FxOrientFrame {
        FxOrientFrame {
            origin: self.origin_when_played,
            axis: self.axis_when_played,
        }
    }

    pub fn commit_frame_last_from_now(&mut self) {
        self.origin_last = self.origin;
        self.axis_last = self.axis;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnFail {
    RingFull,

    TooManySpotlights,

    EffectLimit,
}

#[derive(Clone, Debug)]
pub struct PendingRunnerSpawn {
    pub mark_entity: Option<u16>,
    pub parent_name: String,
    pub catalog_index: u16,
    pub def_index: u8,
    pub msec_begin: i32,
    pub random_seed: u64,
    pub origin: [f32; 3],
    pub axis: [[f32; 3]; 3],

    pub rot_deg: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct PendingSoundSpawn {
    pub parent_name: String,
    pub catalog_index: u16,
    pub def_index: u8,
    pub msec_begin: i32,
    pub random_seed: u64,
    pub origin: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct PendingDecalSpawn {
    pub parent_name: String,
    pub catalog_index: u16,
    pub def_index: u8,
    pub msec_begin: i32,
    pub random_seed: u64,
    pub origin: [f32; 3],

    pub bolt: u8,

    pub mark_entity: Option<u16>,

    pub axis: [[f32; 3]; 3],
}

#[derive(Clone, Debug)]
pub struct PendingTrailImpact {
    pub parent_def_name: String,
    pub catalog_index: u16,
    pub def_index: u8,
    pub origin: [f32; 3],
    pub pre_vel: [f32; 3],
    pub msec: i32,
}

pub struct FxSystemHost {
    pub spawn_mark_entity: Option<u16>,
    pub last_decal_mark_entity: Option<u16>,
    effects: Vec<FxEffectSlot>,

    pub(crate) elems: Vec<FxElemSlot>,
    pub(crate) sort_distances: Vec<(u32, f32)>,
    pub(crate) sort_epoch: u32,

    pub(crate) elem_first_free: Option<usize>,

    pub elem_live_count: u32,

    pub elem_alloc_failures: u32,

    pub(crate) trails: Vec<FxTrailSlot>,

    pub(crate) trail_first_free: Option<usize>,

    pub trail_live_count: u32,

    pub trail_alloc_failures: u32,

    pub(crate) trail_elems: Vec<FxTrailElemSlot>,

    pub(crate) trail_elem_first_free: Option<usize>,

    pub trail_elem_live_count: u32,

    pub trail_elem_alloc_failures: u32,

    pub(crate) spark_clouds: Vec<crate::spark::FxSparkCloudHistorySlot>,

    pub(crate) spark_first_free: Option<usize>,
    pub spark_live_count: u32,

    pub spark_alloc_failures: u32,

    pub(crate) spark_fountains: Vec<crate::spark_fountain::FxSparkFountainClusterSlot>,

    pub(crate) spark_fountain_first_free: Option<usize>,

    pub spark_fountain_live_count: u32,
    pub spark_fountain_alloc_failures: u32,
    pub(crate) spark_fountain_meshes: Vec<crate::spark_fountain::FxSparkFountainMeshSlot>,
    pub(crate) spark_fountain_mesh_first_free: Option<usize>,

    pub spark_fountain_holdrand: u32,

    pub vis_blocker_write: fx_iw4::FxVisBlockerBuf,

    pub vis_blocker_read: fx_iw4::FxVisBlockerBuf,

    pub gaps: FxGaps,

    pub glass: crate::glass::FxGlassSystemHost,

    handles: Vec<u16>,
    pub first_active_effect: i32,
    pub first_new_effect: i32,
    pub first_free_effect: i32,
    pub spotlight_count: i32,

    pub bolted_warn_count: i32,

    pub bolt_first_free: i32,

    bolt_next: Vec<i32>,
    pub msec_now: i32,

    pub frame_stamp: i32,

    pub iterator_count: i32,

    pub needs_garbage_collection: bool,

    pub last_runner_parent: Option<String>,

    pub last_runner_elem: Option<u8>,

    pub last_runner_child: Option<String>,

    pub last_runner_msec: Option<i32>,

    pub last_runner_rot_deg: Option<f32>,

    pub last_decal_parent: Option<String>,

    pub last_decal_elem: Option<u8>,

    pub last_decal_msec: Option<i32>,

    pub last_decal_vis: Option<u8>,

    pub last_decal_size0: Option<f32>,

    pub last_decal_rotation: Option<f32>,

    pub last_decal_mat0_edge: Option<String>,
    pub last_decal_mat1_edge: Option<String>,

    pub last_decal_mat0: Option<String>,
    pub last_decal_mat1: Option<String>,

    pub last_decal_origin: Option<[f32; 3]>,

    pub last_decal_axis: Option<[[f32; 3]; 3]>,

    pub last_decal_bolt: Option<u8>,

    pub last_decal_against_world: Option<bool>,

    pub last_decal_against_models: Option<bool>,

    pub mark_pool_live: Option<u32>,

    pub mark_first_free: Option<u16>,

    pub tri_first_free: Option<u32>,

    pub point_first_free: Option<u32>,

    pub mark_alloced: Option<u32>,

    pub mark_go_world_skip: Option<u32>,

    pub mark_go_world_fire: Option<u32>,

    pub mark_go_models_skip: Option<u32>,

    pub mark_go_models_fire: Option<u32>,

    pub last_models_smodel_n: Option<u32>,

    pub last_models_clip_kept: Option<u32>,

    pub last_models_no_instance: Option<u32>,

    pub last_models_no_cpu: Option<u32>,

    pub last_models_no_mesh: Option<u32>,

    pub last_models_surf_keep: Option<u32>,

    pub mark_box_surfaces_skip: Option<u32>,

    pub mark_box_surfaces_run: Option<u32>,

    pub gfx_mark_surf_n: Option<u32>,

    pub gfx_mark_vert_n: Option<u32>,

    pub gfx_mark_index_n: Option<u32>,

    pub gfx_mark_surf_warn: Option<u32>,

    pub gfx_mark_vert_warn: Option<u32>,

    pub gfx_mark_index_warn: Option<u32>,

    pub gfx_mark_mesh_xyz0: Option<[f32; 3]>,

    pub gfx_mark_packed_n: Option<u32>,

    pub gfx_mark_draw_n: Option<u32>,
    pub gfx_mark_draw_tri: Option<u32>,
    pub gfx_mark_draw_skip_why: Option<String>,

    pub last_mark_lmap: Option<u8>,

    pub last_mark_primary_light: Option<u8>,

    pub last_mark_probe: Option<u8>,

    pub last_mark_lmap_none_n: Option<u32>,

    pub last_mark_lmap_page_n: Option<u32>,

    pub last_mark_mesh: Option<crate::GfxMarkMeshCensus>,

    pub last_decal_color: Option<u32>,

    pub marks: crate::FxMarksSystemHost,

    pub pending_runners: Vec<PendingRunnerSpawn>,

    pub pending_sounds: Vec<PendingSoundSpawn>,

    pub pending_decals: Vec<PendingDecalSpawn>,

    pub pending_trail_impacts: Vec<PendingTrailImpact>,

    pub mark_receivers: crate::MarkReceiverEnable,
    mark_trace_seq: u32,

    pub mark_traces: Vec<crate::MarkTraceRecord>,

    pub last_mark_alloc_slot: Option<u16>,
}

impl Default for FxSystemHost {
    fn default() -> Self {
        Self::new()
    }
}

impl FxSystemHost {
    pub fn new() -> Self {
        let mut handles = Vec::with_capacity(FX_EFFECT_HANDLE_RING_SIZE as usize);
        for slot in 0..FX_EFFECT_POOL_CAPACITY {
            handles.push(effect_handle_for_slot(slot));
        }
        let mut elems: Vec<FxElemSlot> = (0..FX_ELEM_POOL_CAPACITY)
            .map(|_| FxElemSlot::default())
            .collect();

        for i in 0..FX_ELEM_POOL_CAPACITY as usize {
            elems[i].next_elem_handle = if i + 1 < FX_ELEM_POOL_CAPACITY as usize {
                (i + 1) as u16
            } else {
                FX_ELEM_HANDLE_NONE
            };
        }

        let mut trails: Vec<FxTrailSlot> = (0..FX_TRAIL_POOL_CAPACITY)
            .map(|_| FxTrailSlot::default())
            .collect();
        for i in 0..FX_TRAIL_POOL_CAPACITY as usize {
            trails[i].next_trail_handle = if i + 1 < FX_TRAIL_POOL_CAPACITY as usize {
                (i + 1) as u16
            } else {
                FX_TRAIL_HANDLE_NONE
            };
        }

        let mut trail_elems: Vec<FxTrailElemSlot> = (0..FX_TRAIL_ELEM_POOL_CAPACITY)
            .map(|_| FxTrailElemSlot::default())
            .collect();
        for i in 0..FX_TRAIL_ELEM_POOL_CAPACITY as usize {
            trail_elems[i].next_trail_elem_handle = if i + 1 < FX_TRAIL_ELEM_POOL_CAPACITY as usize
            {
                (i + 1) as u16
            } else {
                FX_TRAIL_HANDLE_NONE
            };
        }
        let mut spark_clouds: Vec<crate::spark::FxSparkCloudHistorySlot> = (0
            ..FX_SPARK_CLOUD_HISTORY_CAPACITY)
            .map(|_| crate::spark::FxSparkCloudHistorySlot::default())
            .collect();
        for i in 0..FX_SPARK_CLOUD_HISTORY_CAPACITY as usize {
            spark_clouds[i].next_free = if i + 1 < FX_SPARK_CLOUD_HISTORY_CAPACITY as usize {
                (i + 1) as u16
            } else {
                FX_SPARK_CLOUD_HANDLE_NONE
            };
        }
        let mut spark_fountains: Vec<crate::spark_fountain::FxSparkFountainClusterSlot> = (0
            ..FX_SPARK_FOUNTAIN_CLUSTER_CAPACITY)
            .map(|_| crate::spark_fountain::FxSparkFountainClusterSlot::default())
            .collect();
        for i in 0..FX_SPARK_FOUNTAIN_CLUSTER_CAPACITY as usize {
            spark_fountains[i].next_free = if i + 1 < FX_SPARK_FOUNTAIN_CLUSTER_CAPACITY as usize {
                (i + 1) as u16
            } else {
                FX_SPARK_FOUNTAIN_HANDLE_NONE
            };
        }
        let bolt_next: Vec<i32> = (0..fx_iw4::FX_BOLT_RECORD_CAPACITY as i32)
            .map(bolt_init_next_index)
            .collect();
        let mut spark_fountain_meshes: Vec<crate::spark_fountain::FxSparkFountainMeshSlot> = (0
            ..FX_SPARK_FOUNTAIN_MESH_CAPACITY)
            .map(|_| crate::spark_fountain::FxSparkFountainMeshSlot::default())
            .collect();
        for i in 0..FX_SPARK_FOUNTAIN_MESH_CAPACITY as usize {
            spark_fountain_meshes[i].next_free = if i + 1 < FX_SPARK_FOUNTAIN_MESH_CAPACITY as usize
            {
                (i + 1) as u16
            } else {
                FX_SPARK_FOUNTAIN_HANDLE_NONE
            };
        }
        Self {
            effects: (0..FX_EFFECT_POOL_CAPACITY)
                .map(|_| FxEffectSlot::default())
                .collect(),
            elems,
            sort_distances: vec![(0, 0.0); FX_ELEM_POOL_CAPACITY as usize],
            sort_epoch: 0,
            elem_first_free: Some(0),
            elem_live_count: 0,
            elem_alloc_failures: 0,
            trails,
            trail_first_free: Some(0),
            trail_live_count: 0,
            trail_alloc_failures: 0,
            trail_elems,
            trail_elem_first_free: Some(0),
            trail_elem_live_count: 0,
            trail_elem_alloc_failures: 0,
            spark_clouds,
            spark_first_free: Some(0),
            spark_live_count: 0,
            spark_alloc_failures: 0,
            spark_fountains,
            spark_fountain_first_free: Some(0),
            spark_fountain_live_count: 0,
            spark_fountain_alloc_failures: 0,
            spark_fountain_meshes,
            spark_fountain_mesh_first_free: Some(0),
            spark_fountain_holdrand: fx_iw4::MSVCRT_HOLDRAND_DEFAULT,
            vis_blocker_write: fx_iw4::FxVisBlockerBuf::default(),
            vis_blocker_read: fx_iw4::FxVisBlockerBuf::default(),
            gaps: FxGaps::default(),
            glass: crate::glass::FxGlassSystemHost::default(),
            handles,
            first_active_effect: 0,
            first_new_effect: 0,
            first_free_effect: 0,
            spotlight_count: 0,
            bolted_warn_count: 0,
            bolt_first_free: 0,
            bolt_next,
            msec_now: 0,
            frame_stamp: 0,
            iterator_count: 0,
            needs_garbage_collection: false,
            last_runner_parent: None,
            last_runner_elem: None,
            last_runner_child: None,
            last_runner_msec: None,
            last_runner_rot_deg: None,
            last_decal_parent: None,
            last_decal_elem: None,
            last_decal_msec: None,
            last_decal_vis: None,
            last_decal_size0: None,
            last_decal_rotation: None,
            last_decal_mat0_edge: None,
            last_decal_mat1_edge: None,
            last_decal_mat0: None,
            last_decal_mat1: None,
            last_decal_color: None,
            last_decal_origin: None,
            last_decal_axis: None,
            last_decal_bolt: None,
            spawn_mark_entity: None,
            last_decal_mark_entity: None,
            last_decal_against_world: None,
            last_decal_against_models: None,
            mark_pool_live: Some(0),
            mark_first_free: Some(0),
            tri_first_free: Some(0),
            point_first_free: Some(0),
            mark_alloced: Some(0),
            mark_go_world_skip: Some(0),
            mark_go_world_fire: Some(0),
            mark_go_models_skip: Some(0),
            mark_go_models_fire: Some(0),
            last_models_smodel_n: None,
            last_models_clip_kept: None,
            last_models_no_instance: None,
            last_models_no_cpu: None,
            last_models_no_mesh: None,
            last_models_surf_keep: None,
            mark_box_surfaces_skip: Some(0),
            mark_box_surfaces_run: Some(0),
            gfx_mark_surf_n: None,
            gfx_mark_vert_n: None,
            gfx_mark_index_n: None,
            gfx_mark_surf_warn: None,
            gfx_mark_vert_warn: None,
            gfx_mark_index_warn: None,
            gfx_mark_mesh_xyz0: None,
            gfx_mark_packed_n: None,
            gfx_mark_draw_n: None,
            gfx_mark_draw_tri: None,
            gfx_mark_draw_skip_why: None,
            last_mark_lmap: None,
            last_mark_primary_light: None,
            last_mark_probe: None,
            last_mark_lmap_none_n: None,
            last_mark_lmap_page_n: None,
            last_mark_mesh: None,
            marks: crate::FxMarksSystemHost::init(),
            pending_runners: Vec::new(),
            pending_sounds: Vec::new(),
            pending_decals: Vec::new(),
            pending_trail_impacts: Vec::new(),
            mark_receivers: crate::MarkReceiverEnable::default(),
            mark_trace_seq: 0,
            mark_traces: Vec::new(),
            last_mark_alloc_slot: None,
        }
    }

    pub fn live_effects(&self) -> impl Iterator<Item = &FxEffectSlot> {
        self.effects
            .iter()
            .filter(|e| e.ring_resident && e.has_refs())
    }

    pub fn stop_bolted(&mut self, def_name: &str, dobj: u32, bone: u16) {
        let slots: Vec<_> = self
            .effects
            .iter()
            .enumerate()
            .filter_map(|(slot, effect)| {
                (effect.ring_resident
                    && effect.def_name == def_name
                    && fx_iw4::bolt_dobj(effect.bolt_packed) == dobj
                    && fx_iw4::bolt_bone(effect.bolt_packed) == u32::from(bone))
                .then_some(slot)
            })
            .collect();
        for slot in slots {
            crate::spawn::stop_effect_non_recursive(self, slot);
        }
    }

    pub fn stop_owned(&mut self, handle: u16) {
        let Some(slot) = self.slot_index_for_handle(handle) else {
            return;
        };
        crate::spawn::stop_effect_non_recursive(self, slot);
        self.play_release_ownership(handle);
    }

    pub fn kill_def_newer_than(&mut self, def_name: &str, msec_begin: i32) {
        let slots: Vec<_> = self
            .effects
            .iter()
            .enumerate()
            .filter_map(|(slot, effect)| {
                (effect.ring_resident
                    && effect.has_refs()
                    && effect.def_name == def_name
                    && effect.msec_begin >= msec_begin)
                    .then_some(slot)
            })
            .collect();
        if slots.is_empty() {
            return;
        }
        for slot in slots {
            crate::spawn::free_all_elems_for_effect(self, slot);
            crate::spawn::free_all_trails_for_effect(self, slot);
            if let Some(effect) = self.effect_at_mut(slot) {
                effect.status &=
                    !(FX_STATUS_REF_COUNT_MASK_IW4 | fx_iw4::FX_STATUS_HAS_PENDING_LOOP_ELEMS);
            }
        }
        self.needs_garbage_collection = true;
        self.run_garbage_collection();
    }

    pub fn refresh_bolt_poses(
        &mut self,
        mut resolve: impl FnMut(u32, u16) -> Option<FxResolvedBoltPose>,
    ) {
        for effect in &mut self.effects {
            if !effect.ring_resident || fx_iw4::bolt_handle_is_none(effect.bolt_packed) {
                continue;
            }
            let dobj = fx_iw4::bolt_dobj(effect.bolt_packed);
            let bone = fx_iw4::bolt_bone(effect.bolt_packed) as u16;
            match resolve(dobj, bone) {
                Some(resolved) => {
                    effect.bolt_centity_teleport = resolved.centity_teleport;
                    effect.bolt_bone_pose =
                        resolved.orientation.map(|pose| (pose.origin, pose.axis));
                }
                None => {
                    effect.bolt_centity_teleport = false;
                    effect.bolt_bone_pose = None;
                }
            }
        }
    }

    pub fn live_elems(&self) -> impl Iterator<Item = &crate::FxElemSlot> {
        self.elems.iter().filter(|e| e.occupied)
    }

    pub fn slot_for_handle(&self, handle: u16) -> Option<&FxEffectSlot> {
        let slot = handle_to_slot(handle)?;
        let e = self.effects.get(slot)?;
        e.ring_resident.then_some(e)
    }

    pub fn slot_for_handle_mut(&mut self, handle: u16) -> Option<&mut FxEffectSlot> {
        let slot = handle_to_slot(handle)?;
        let e = self.effects.get_mut(slot)?;
        e.ring_resident.then_some(e)
    }

    pub(crate) fn slot_index_for_handle(&self, handle: u16) -> Option<usize> {
        let slot = handle_to_slot(handle)?;
        self.effects
            .get(slot)
            .filter(|e| e.ring_resident)
            .map(|_| slot)
    }

    pub fn effect_at(&self, slot: usize) -> Option<&FxEffectSlot> {
        self.effects.get(slot)
    }

    pub(crate) fn effect_at_mut(&mut self, slot: usize) -> Option<&mut FxEffectSlot> {
        self.effects.get_mut(slot)
    }

    pub(crate) fn handle_at_ring(&self, ring_index: u32) -> u16 {
        let i = (ring_index & (FX_EFFECT_HANDLE_RING_SIZE - 1)) as usize;
        self.handles[i]
    }

    fn set_handle_at_ring(&mut self, ring_index: u32, handle: u16) {
        let i = (ring_index & (FX_EFFECT_HANDLE_RING_SIZE - 1)) as usize;
        self.handles[i] = handle;
    }

    pub fn ring_occupancy(&self) -> i32 {
        self.first_free_effect
            .wrapping_sub(self.first_active_effect)
    }

    pub fn record_impact_mark_from_decal(&mut self, bolt: u8) -> crate::MarkImpactResult {
        let skip_world = marks_iw4::impact_mark_skip_world_from_stored_bolt(bolt);
        let result = self.marks.impact_mark(crate::MarkImpactRequest {
            skip_world,
            receivers: self.mark_receivers,
        });
        self.last_decal_against_world = result.entered.then_some(result.against_world);
        self.last_decal_against_models = result.entered.then_some(result.against_models);
        self.sync_mark_pool_heads();
        result
    }

    pub fn sync_mark_pool_heads(&mut self) {
        self.mark_pool_live = Some(self.marks.live_count());
        self.mark_first_free = Some(self.marks.first_free_handle());
        self.tri_first_free = Some(self.marks.tri_first_free());
        self.point_first_free = Some(self.marks.point_first_free());
        self.mark_alloced = Some(self.marks.alloced_count());
    }

    pub fn note_box_surfaces_skip(&mut self) {
        self.mark_box_surfaces_skip = Some(self.mark_box_surfaces_skip.unwrap_or(0) + 1);
    }

    pub fn note_box_surfaces_run(&mut self) {
        self.mark_box_surfaces_run = Some(self.mark_box_surfaces_run.unwrap_or(0) + 1);
    }

    pub fn note_world_go_skip(&mut self, def_index: u8) {
        self.mark_go_world_skip = Some(self.mark_go_world_skip.unwrap_or(0) + 1);
        self.gaps
            .raise(crate::FxGapCause::MarkFragmentsSkipped { def_index });
    }

    pub fn note_world_go_fire(&mut self) {
        self.mark_go_world_fire = Some(self.mark_go_world_fire.unwrap_or(0) + 1);
        self.sync_mark_pool_heads();
    }

    pub fn note_models_go_skip(&mut self, def_index: u8) {
        self.mark_go_models_skip = Some(self.mark_go_models_skip.unwrap_or(0) + 1);
        self.gaps
            .raise(crate::FxGapCause::MarkFragmentsSkipped { def_index });
    }

    pub fn note_models_go_fire(&mut self) {
        self.mark_go_models_fire = Some(self.mark_go_models_fire.unwrap_or(0) + 1);
        self.sync_mark_pool_heads();
    }

    pub fn note_impact_mark_outer_skip(&mut self, def_index: u8) {
        self.gaps
            .raise(crate::FxGapCause::ElemDecalSpawnSkipped { def_index });
    }

    pub fn generate_world_mark_verts(&mut self) {
        if !self.mark_receivers.fx_marks {
            self.stamp_empty_world_mark_mesh();
            return;
        }
        let mesh = self.marks.generate_world_mark_verts();
        self.gfx_mark_surf_n = Some(mesh.budget.surf_n);
        self.gfx_mark_vert_n = Some(mesh.budget.vert_n);
        self.gfx_mark_index_n = Some(mesh.budget.index_n);
        self.gfx_mark_surf_warn = Some(mesh.budget.surf_warn);
        self.gfx_mark_vert_warn = Some(mesh.budget.vert_warn);
        self.gfx_mark_index_warn = Some(mesh.budget.index_warn);
        self.gfx_mark_mesh_xyz0 = mesh.first_xyz;
        self.gfx_mark_packed_n = Some(mesh.packed.len() as u32);
        self.last_mark_mesh = Some(mesh);
    }

    fn stamp_empty_world_mark_mesh(&mut self) {
        let mesh = crate::GfxMarkMeshCensus::default();
        self.gfx_mark_surf_n = Some(0);
        self.gfx_mark_vert_n = Some(0);
        self.gfx_mark_index_n = Some(0);
        self.gfx_mark_surf_warn = Some(mesh.budget.surf_warn);
        self.gfx_mark_vert_warn = Some(mesh.budget.vert_warn);
        self.gfx_mark_index_warn = Some(mesh.budget.index_warn);
        self.gfx_mark_mesh_xyz0 = None;
        self.gfx_mark_packed_n = Some(0);
        self.last_mark_mesh = Some(mesh);
    }

    fn alloc_bolt_record(&mut self) -> Option<u8> {
        let first = self.bolt_first_free;
        let next = self
            .bolt_next
            .get(first as usize)
            .copied()
            .unwrap_or(fx_iw4::FX_BOLT_FREE_NONE);
        let (idx, new_first) = bolt_alloc(first, next)?;
        self.bolt_first_free = new_first;
        self.bolted_warn_count = self.bolted_warn_count.saturating_add(1);
        Some(idx as u8)
    }

    pub fn spawn_effect(
        &mut self,
        def_name: &str,
        origin: [f32; 3],
        axis: [[f32; 3]; 3],
        msec: i32,
        bolt: u32,
        wants_spotlight: bool,
        catalog_index: u16,
    ) -> Result<u16, SpawnFail> {
        if wants_spotlight && self.spotlight_count >= FX_SPOT_LIGHT_LIMIT {
            return Err(SpawnFail::TooManySpotlights);
        }
        let bolt_index = if bolt == FX_SPAWN_BOLT_NONE {
            0xff
        } else {
            match self.alloc_bolt_record() {
                Some(i) => i,
                None => return Err(SpawnFail::EffectLimit),
            }
        };

        let alloc = self.first_free_effect;
        if self.ring_occupancy() >= FX_EFFECT_HANDLE_RING_SIZE as i32 {
            return Err(SpawnFail::RingFull);
        }
        self.first_free_effect = alloc.wrapping_add(1);

        let ring_i = (alloc as u32 & (FX_EFFECT_HANDLE_RING_SIZE - 1)) as usize;
        let handle = self.handles[ring_i];
        let slot = handle_to_slot(handle).expect("Init ring only holds valid slot handles");

        let effect = &mut self.effects[slot];
        let (bolt_parent_quat, bolt_parent_origin) = fx_iw4::bolt_init_parent_orientation();
        *effect = FxEffectSlot {
            def_name: def_name.to_owned(),
            catalog_index,

            status: 0x4000_0001,
            first_elem_handle: [FX_ELEM_HANDLE_NONE; 3],
            first_sorted_elem_handle: FX_ELEM_HANDLE_NONE,
            first_trail_handle: FX_ELEM_HANDLE_NONE,

            random_seed: effect_random_key(u64::from(alloc as u32), msec),
            own_handle: handle,
            packed_lighting: [0xff; 3],
            packed_lighting_src: FxPackedLightingSrc::White,
            bolt: bolt_index,
            mark_entity: self.spawn_mark_entity,
            bolt_packed: fx_iw4::FX_BOLT_HANDLE_NONE,
            bolt_centity_teleport: false,
            bolt_parent_quat,
            bolt_parent_origin,
            bolt_bone_pose: None,
            frame_stamp: -1,
            msec_begin: msec,
            msec_last_update: msec,
            origin,
            axis,
            origin_when_played: origin,
            axis_when_played: axis,
            origin_last: origin,
            axis_last: axis,
            distance: 0.0,
            ring_resident: true,
        };

        if self.first_new_effect <= alloc {
            self.first_new_effect = alloc.wrapping_add(1);
        }
        if wants_spotlight {
            self.spotlight_count = self.spotlight_count.saturating_add(1);
        }
        Ok(handle)
    }

    pub fn del_ref_to_effect(&mut self, _handle: u16) {
        self.needs_garbage_collection = true;
    }

    pub fn play_release_ownership(&mut self, handle: u16) {
        let unique = self
            .slot_for_handle(handle)
            .map(|e| status_is_unique_done(e.status))
            .unwrap_or(false);
        if unique {
            self.del_ref_to_effect(handle);
        }
        if let Some(effect) = self.slot_for_handle_mut(handle) {
            effect.status = effect.status.wrapping_sub(1);
        }
    }

    pub fn run_garbage_collection(&mut self) {
        if !self.needs_garbage_collection {
            return;
        }
        let first_active = self.first_active_effect;
        let mut active_index = self.first_new_effect;
        let mut freed: Vec<u16> = Vec::new();
        while active_index != first_active {
            active_index = active_index.wrapping_sub(1);
            let handle = self.handle_at_ring(active_index as u32);
            let slot = handle_to_slot(handle);
            let dead = slot
                .and_then(|s| self.effects.get(s))
                .map(|e| !e.ring_resident || !e.has_refs())
                .unwrap_or(true);
            if dead {
                if let Some(s) = slot {
                    if self.effects.get(s).is_some_and(|e| e.ring_resident) {
                        crate::spawn::free_all_elems_for_effect(self, s);
                        crate::spawn::free_all_trails_for_effect(self, s);
                    }
                }
                freed.push(handle);
            } else {
                let dest = (active_index as u32).wrapping_add(freed.len() as u32);
                self.set_handle_at_ring(dest, handle);
            }
        }
        while let Some(handle) = freed.pop() {
            self.set_handle_at_ring(active_index as u32, handle);
            active_index = active_index.wrapping_add(1);
            if let Some(slot) = handle_to_slot(handle) {
                self.effects[slot] = FxEffectSlot::default();
            }
        }
        self.first_active_effect = active_index;
        self.needs_garbage_collection = false;
    }

    pub fn set_msec_now(&mut self, msec: i32) {
        self.msec_now = msec;
    }

    pub fn advance_frame_stamp(&mut self) {
        self.frame_stamp = self.frame_stamp.wrapping_add(1);
    }

    pub fn push_mark_trace(&mut self, result: &crate::MarkImpactResult) {
        self.mark_trace_seq = self.mark_trace_seq.wrapping_add(1);
        if self.mark_trace_seq == 0 {
            self.mark_trace_seq = 1;
        }
        let slot = self.last_mark_alloc_slot;
        let constructed = slot.and_then(|h| self.marks.constructed(h).copied());
        let skip_why = if !result.entered {
            if self.marks.no_marks {
                Some("no_marks")
            } else if !self.mark_receivers.fx_marks {
                Some("fx_marks=0")
            } else {
                Some("outer_gate")
            }
        } else if slot.is_none() {
            Some("no_alloc")
        } else {
            None
        };
        let rec = crate::MarkTraceRecord {
            id: self.mark_trace_seq,
            bolt: self.last_decal_bolt.unwrap_or(0xff),
            entered: result.entered,
            against_world: result.against_world,
            against_models: result.against_models,
            parent: self.last_decal_parent.clone(),
            elem: self.last_decal_elem,
            msec: self.last_decal_msec,
            origin: self.last_decal_origin,
            size0: self.last_decal_size0,
            color: self.last_decal_color,
            color_kind: if self.last_decal_color.is_some() {
                "sampled"
            } else {
                "none"
            },
            mat0: self.last_decal_mat0.clone(),
            mat1: self.last_decal_mat1.clone(),
            bound: slot.and_then(|s| self.marks.material_name(s).map(str::to_owned)),
            slot,
            tri_n: constructed.map(|c| c.tri_count),
            point_n: constructed.map(|c| c.point_count),
            native_color: constructed.map(|c| c.native_color),
            context: constructed.map(|c| c.context),
            skip_why,
            vis: self.last_decal_vis,
            mat0_edge: self.last_decal_mat0_edge.clone(),
            mat1_edge: self.last_decal_mat1_edge.clone(),
            nx: self.last_decal_axis.map(|a| a[2][0]),
            ny: self.last_decal_axis.map(|a| a[2][1]),
            nz: self.last_decal_axis.map(|a| a[2][2]),
        };
        if self.mark_traces.len() >= 512 {
            self.mark_traces.remove(0);
        }
        self.mark_traces.push(rec);
        self.last_mark_alloc_slot = None;
    }

    pub fn mark_profile_line(&self) -> String {
        format!(
            "fx_mark_profile live={}/512 alloced={} tri_free={} point_free={} gfx_surf={} gfx_draw={} unlinked={} ents_AddEntity=typed_gap draw_gate={}",
            self.marks.live_count(),
            self.marks.alloced_count(),
            self.marks.tri_first_free(),
            self.marks.point_first_free(),
            self.gfx_mark_surf_n
                .map_or_else(|| "null".to_owned(), |n| n.to_string()),
            self.gfx_mark_draw_n
                .map_or_else(|| "null".to_owned(), |n| n.to_string()),
            self.marks.live_count(),
            if self.mark_receivers.fx_marks {
                "on"
            } else {
                "fx_marks=0"
            },
        )
    }
}

fn handle_to_slot(handle: u16) -> Option<usize> {
    let byte_off = (handle as u32) << 2;
    if byte_off % FX_EFFECT_SLOT_SIZE as u32 != 0 {
        return None;
    }
    let slot = (byte_off / FX_EFFECT_SLOT_SIZE as u32) as usize;
    (slot < FX_EFFECT_POOL_CAPACITY as usize).then_some(slot)
}
