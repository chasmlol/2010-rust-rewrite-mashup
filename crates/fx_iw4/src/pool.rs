pub const FX_SYSTEM_STRIDE: usize = 0xad0;

pub const FX_BUFFERS_POOL_STRIDE: usize = 0x107a_80;

pub const FX_EFFECT_SLOT_SIZE: usize = 0x90;

pub const FX_EFFECT_POOL_BYTES: usize = 0x24000;

pub const FX_EFFECT_POOL_CAPACITY: u32 = 0x400;

pub const FX_ELEM_RUNTIME_STRIDE: usize = 0x28;

pub const FX_ELEM_POOL_CAPACITY: u32 = 0x800;

pub const FX_TRAIL_RUNTIME_STRIDE: usize = 0x10;

pub const FX_TRAIL_POOL_CAPACITY: u32 = 0x80;

pub const FX_TRAIL_ELEM_RUNTIME_STRIDE: usize = 0x20;

pub const FX_TRAIL_ELEM_POOL_CAPACITY: u32 = 0x800;

pub const FX_EFFECT_HANDLE_RING_SIZE: u32 = 0x400;
pub const FX_EFFECT_HANDLE_RING_MASK: u32 = 0x3ff;

pub const FX_BUFFERS_OFF_EFFECTS: u32 = 0x0;
pub const FX_BUFFERS_OFF_ELEMS: u32 = 0x24000;
pub const FX_BUFFERS_OFF_SPARK_CLOUD: u32 = 0x4a800;

pub const FX_PLAY_BOLT_NONE: u32 = 0x7ff;

pub const FX_ENTITYNUM_WORLD: u32 = 0x7fe;

pub const FX_SPAWN_BOLT_NONE: u32 = 0xfff;

pub const FX_SPOT_LIGHT_LIMIT: i32 = 1;

pub const FX_WARN_TOO_MANY_SPOTLIGHTS: u32 = 0x30;

pub const FX_WARN_ELEM_LIMIT: u32 = 0x31;

pub const FX_WARN_EFFECT_LIMIT: u32 = 0x32;

pub const FX_WARN_SPARK_CLOUD_LIMIT: u32 = 0x1e;

pub const FX_STATUS_UNIQUE_MASK: u32 = 0x1fff;
pub const FX_STATUS_UNIQUE_DONE: u32 = 1;

#[inline]
pub const fn effect_handle_from_byte_offset(byte_offset: u32) -> u16 {
    (byte_offset >> 2) as u16
}

#[inline]
pub const fn effect_byte_offset_from_handle(handle: u16) -> u32 {
    (handle as u32) << 2
}

#[inline]
pub const fn effect_addr(effects_base: u32, handle: u16) -> u32 {
    effects_base.wrapping_add(effect_byte_offset_from_handle(handle))
}

#[inline]
pub const fn effect_handle_for_slot(slot: u32) -> u16 {
    effect_handle_from_byte_offset(slot.wrapping_mul(FX_EFFECT_SLOT_SIZE as u32))
}

#[inline]
pub const fn elem_handle_from_ptr_delta(byte_delta: u32) -> u16 {
    (byte_delta >> 2) as u16
}

#[inline]
pub const fn elem_addr(elems_base: u32, handle: u16) -> u32 {
    elems_base.wrapping_add((handle as u32) << 2)
}

#[inline]
pub const fn trail_handle_from_byte_offset(byte_offset: u32) -> u16 {
    (byte_offset >> 2) as u16
}

#[inline]
pub const fn trail_handle_for_slot(slot: u32) -> u16 {
    trail_handle_from_byte_offset(slot.wrapping_mul(FX_TRAIL_RUNTIME_STRIDE as u32))
}

#[inline]
pub const fn trail_addr(trails_base: u32, handle: u16) -> u32 {
    trails_base.wrapping_add((handle as u32) << 2)
}

#[inline]
pub const fn trail_elem_handle_for_slot(slot: u32) -> u16 {
    trail_handle_from_byte_offset(slot.wrapping_mul(FX_TRAIL_ELEM_RUNTIME_STRIDE as u32))
}

#[inline]
pub const fn trail_elem_addr(trail_elems_base: u32, handle: u16) -> u32 {
    trail_elems_base.wrapping_add((handle as u32) << 2)
}
