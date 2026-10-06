use fx_iw4::{
    FX_SPARK_CLOUD_HANDLE_NONE, FX_SPARK_CLOUD_HISTORY_CAPACITY, FX_SPARK_CLOUD_HISTORY_STRIDE,
    FX_SPARK_CLOUD_SAMPLE_MASK, FX_SPARK_CLOUD_SAMPLE_RING, FxOrientFrame, FxOrientSpawnParams,
    FxOrientation, GfxParticleCloud, clamp_elem_rotation_time, empty_particle_cloud,
    get_elem_angles_axis, get_orientation, orientation_pos_to_world, spark_cloud_handle_for_slot,
    sparkcloud_build_triplet, sparkcloud_fill_sample, sparkcloud_history_advance,
    sparkcloud_history_should_advance,
};

use crate::system::FxSystemHost;

#[derive(Clone, Debug)]
pub struct FxSparkCloudHistorySlot {
    pub occupied: bool,
    pub read_idx: u32,
    pub write_idx: u32,
    pub last_time: i32,
    pub samples: [GfxParticleCloud; FX_SPARK_CLOUD_SAMPLE_RING as usize],

    pub next_free: u16,
}

impl Default for FxSparkCloudHistorySlot {
    fn default() -> Self {
        Self {
            occupied: false,
            read_idx: 0,
            write_idx: 0,
            last_time: 0,
            samples: [empty_particle_cloud(); FX_SPARK_CLOUD_SAMPLE_RING as usize],
            next_free: FX_SPARK_CLOUD_HANDLE_NONE,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FxSparkFillVisual {
    pub size0: f32,

    pub scale: f32,
    pub color_rgba: [u8; 4],
    pub spawn_angles: [[f32; 2]; 3],
    pub angular_velocity: [[f32; 2]; 3],
}

#[derive(Clone, Debug)]
pub struct FxSparkCloudInstance {
    pub def_name: String,
    pub catalog_index: u16,
    pub def_index: u8,
    pub origin: [f32; 3],
    pub write_idx: u32,
    pub size0: f32,

    pub vis_size1: f32,
    pub clouds: [GfxParticleCloud; 3],
}

#[inline]
pub fn spark_handle_for_slot(slot: u32) -> u16 {
    spark_cloud_handle_for_slot(slot)
}

#[inline]
pub fn spark_slot_for_handle(handle: u16) -> Option<usize> {
    if handle == FX_SPARK_CLOUD_HANDLE_NONE {
        return None;
    }
    let byte = (handle as u32) << 4;
    if byte % FX_SPARK_CLOUD_HISTORY_STRIDE as u32 != 0 {
        return None;
    }
    let slot = (byte / FX_SPARK_CLOUD_HISTORY_STRIDE as u32) as usize;
    (slot < FX_SPARK_CLOUD_HISTORY_CAPACITY as usize).then_some(slot)
}

pub fn alloc_spark_cloud(host: &mut FxSystemHost) -> Option<u16> {
    let Some(dense) = host.spark_first_free else {
        host.spark_alloc_failures = host.spark_alloc_failures.saturating_add(1);
        return None;
    };
    if dense >= host.spark_clouds.len() {
        host.spark_first_free = None;
        host.spark_alloc_failures = host.spark_alloc_failures.saturating_add(1);
        return None;
    }
    let next = host.spark_clouds[dense].next_free;
    host.spark_first_free = if next == FX_SPARK_CLOUD_HANDLE_NONE {
        None
    } else {
        Some(next as usize)
    };
    host.spark_live_count = host.spark_live_count.saturating_add(1);
    host.spark_clouds[dense] = FxSparkCloudHistorySlot {
        occupied: true,
        read_idx: 0,
        write_idx: 0,
        last_time: 0,
        samples: [empty_particle_cloud(); FX_SPARK_CLOUD_SAMPLE_RING as usize],
        next_free: FX_SPARK_CLOUD_HANDLE_NONE,
    };
    Some(spark_handle_for_slot(dense as u32))
}

pub fn free_spark_cloud(host: &mut FxSystemHost, handle: u16) {
    let Some(dense) = spark_slot_for_handle(handle) else {
        return;
    };
    if !host.spark_clouds.get(dense).is_some_and(|s| s.occupied) {
        return;
    }
    let next_free = host
        .spark_first_free
        .map(|d| d as u16)
        .unwrap_or(FX_SPARK_CLOUD_HANDLE_NONE);
    host.spark_clouds[dense] = FxSparkCloudHistorySlot {
        occupied: false,
        next_free,
        ..FxSparkCloudHistorySlot::default()
    };
    host.spark_first_free = Some(dense);
    host.spark_live_count = host.spark_live_count.saturating_sub(1);
}

pub fn update_spark_history(
    host: &mut FxSystemHost,
    spark_handle: u16,
    world_origin: [f32; 3],
    elem_axis: [[f32; 3]; 3],
    size0: f32,
    placement_scale: f32,
    color_rgba: [u8; 4],
    elem_flags: i32,
    msec_now: i32,
) {
    let Some(dense) = spark_slot_for_handle(spark_handle) else {
        return;
    };
    let Some(slot) = host.spark_clouds.get_mut(dense).filter(|s| s.occupied) else {
        return;
    };
    if sparkcloud_history_should_advance(slot.write_idx, slot.last_time, msec_now) {
        let (read, write, t) = sparkcloud_history_advance(slot.read_idx, slot.write_idx, msec_now);
        slot.read_idx = read;
        slot.write_idx = write;
        slot.last_time = t;
    }
    if slot.write_idx == 0 {
        return;
    }
    let idx = ((slot.write_idx.wrapping_sub(1)) & FX_SPARK_CLOUD_SAMPLE_MASK) as usize;
    slot.samples[idx] = sparkcloud_fill_sample(
        world_origin,
        elem_axis,
        size0,
        placement_scale,
        color_rgba,
        elem_flags,
        msec_now,
    );
}

pub fn spark_elem_orientation(
    flags: i32,
    effect_now: &FxOrientFrame,
    effect_alt: &FxOrientFrame,
    spawn: Option<FxOrientSpawnParams>,
) -> FxOrientation {
    get_orientation(flags, effect_now, effect_alt, spawn)
}

pub fn spark_elem_world_origin(
    elem_origin: [f32; 3],
    flags: i32,
    effect_now: &FxOrientFrame,
    effect_alt: &FxOrientFrame,
    spawn: Option<FxOrientSpawnParams>,
) -> [f32; 3] {
    let orient = spark_elem_orientation(flags, effect_now, effect_alt, spawn);
    orientation_pos_to_world(orient.origin, orient.axis, elem_origin)
}

pub fn spark_elem_axis(
    spawn_angles: [[f32; 2]; 3],
    angular_velocity: [[f32; 2]; 3],
    seed: u64,
    age_msec: i32,
    life_msec: i32,
    at_rest_fraction: u8,
    effect_axis: [[f32; 3]; 3],
) -> [[f32; 3]; 3] {
    let rot_t = clamp_elem_rotation_time(age_msec as f32, life_msec as f32, at_rest_fraction);
    get_elem_angles_axis(spawn_angles, angular_velocity, seed, rot_t, effect_axis)
}

pub fn build_spark_cloud_instance(
    host: &FxSystemHost,
    spark_handle: u16,
    def_name: &str,
    catalog_index: u16,
    def_index: u8,
    msec_now: i32,
    vis_size1: f32,
) -> Option<FxSparkCloudInstance> {
    let dense = spark_slot_for_handle(spark_handle)?;
    let slot = host.spark_clouds.get(dense).filter(|s| s.occupied)?;
    if slot.write_idx == 0 {
        return None;
    }
    let latest =
        slot.samples[((slot.write_idx.wrapping_sub(1)) & FX_SPARK_CLOUD_SAMPLE_MASK) as usize];
    if latest.size0 == 0.0 || latest.placement_scale == 0.0 {
        return None;
    }
    let clouds = sparkcloud_build_triplet(
        &slot.samples,
        slot.read_idx,
        slot.write_idx,
        msec_now as f32,
        vis_size1,
    );
    Some(FxSparkCloudInstance {
        def_name: def_name.to_owned(),
        catalog_index,
        def_index,
        origin: latest.pos,
        write_idx: slot.write_idx,
        size0: latest.size0,
        vis_size1,
        clouds,
    })
}
