use fx_iw4::{
    FX_ELEM_AT_REST_NONE, FX_ELEM_POOL_CAPACITY, FX_ELEM_RUNTIME_STRIDE,
    FX_SPARK_CLOUD_HANDLE_NONE, elem_handle_from_ptr_delta,
};

pub const FX_ELEM_HANDLE_NONE: u16 = 0xffff;

#[derive(Clone, Debug)]
pub struct FxElemSlot {
    pub occupied: bool,
    pub def_index: u8,
    pub elem_type: u8,

    pub flags: i32,
    pub visual_count: u8,
    pub sequence: u8,
    pub at_rest_fraction: u8,

    pub emit_residual: u8,
    pub next_elem_handle: u16,
    pub prev_elem_handle: u16,
    pub msec_begin: i32,

    pub life_span_msec: i32,
    pub base_vel: [f32; 3],

    pub origin: [f32; 3],

    pub spawn_origin: [[f32; 2]; 3],
    pub spawn_offset_radius: [f32; 2],
    pub spawn_offset_height: [f32; 2],

    pub owner_effect_slot: u16,

    pub class_index: u8,

    pub sort_order: u8,

    pub spark_cloud_handle: u16,
}

impl Default for FxElemSlot {
    fn default() -> Self {
        Self {
            occupied: false,
            def_index: 0,
            elem_type: 0,
            flags: 0,
            visual_count: 0,
            sequence: 0,
            at_rest_fraction: FX_ELEM_AT_REST_NONE,
            emit_residual: 0,
            next_elem_handle: FX_ELEM_HANDLE_NONE,
            prev_elem_handle: FX_ELEM_HANDLE_NONE,
            msec_begin: 0,
            life_span_msec: 0,
            base_vel: [0.0; 3],
            origin: [0.0; 3],
            spawn_origin: [[0.0; 2]; 3],
            spawn_offset_radius: [0.0; 2],
            spawn_offset_height: [0.0; 2],
            owner_effect_slot: 0,
            class_index: 0,
            sort_order: 0,
            spark_cloud_handle: FX_SPARK_CLOUD_HANDLE_NONE,
        }
    }
}

impl FxElemSlot {
    #[inline]
    pub fn orient_spawn_params(&self, seed: u64) -> fx_iw4::FxOrientSpawnParams {
        fx_iw4::FxOrientSpawnParams {
            spawn_origin: self.spawn_origin,
            spawn_offset_radius: self.spawn_offset_radius,
            spawn_offset_height: self.spawn_offset_height,
            seed,
        }
    }
}

#[inline]
pub fn elem_handle_for_slot(slot: u32) -> u16 {
    elem_handle_from_ptr_delta(slot.wrapping_mul(FX_ELEM_RUNTIME_STRIDE as u32))
}

#[inline]
pub fn elem_slot_for_handle(handle: u16) -> Option<usize> {
    if handle == FX_ELEM_HANDLE_NONE {
        return None;
    }
    let byte = (handle as u32) << 2;
    if byte % FX_ELEM_RUNTIME_STRIDE as u32 != 0 {
        return None;
    }
    let slot = (byte / FX_ELEM_RUNTIME_STRIDE as u32) as usize;
    (slot < FX_ELEM_POOL_CAPACITY as usize).then_some(slot)
}

pub fn elem_class_for_type(elem_type: u8) -> Option<usize> {
    if elem_type == 9 {
        return None;
    }
    if elem_type < 4 {
        Some(0)
    } else if elem_type == 4 || elem_type == 5 {
        Some(2)
    } else {
        Some(1)
    }
}
