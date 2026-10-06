use crate::random::{FxRandomChannel, sample_u16};

pub const FX_ELEM_ATLAS_OFF: usize = 0xa8;

pub const FX_ELEM_ATLAS_SIZE: usize = 8;

const FX_ATLAS_START_MASK: u8 = 3;

const FX_ATLAS_START_RANDOM: u8 = 1;

const FX_ATLAS_LIFE_SCALE: u8 = 4;

const FX_ATLAS_CLAMP_LAST: u8 = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxSpriteAtlasUv {
    pub s0: f32,
    pub ds: f32,
    pub t0: f32,
    pub dt: f32,
    pub entry_count: u16,

    pub atlas_index: u16,
}

impl FxSpriteAtlasUv {
    pub const FULL: Self = Self {
        s0: 0.0,
        ds: 1.0,
        t0: 0.0,
        dt: 1.0,
        entry_count: 1,
        atlas_index: 0,
    };

    pub const fn corners(self) -> [[f32; 2]; 4] {
        [
            [self.s0, self.t0 + self.dt],
            [self.s0, self.t0],
            [self.s0 + self.ds, self.t0],
            [self.s0 + self.ds, self.t0 + self.dt],
        ]
    }
}

pub fn sprite_atlas_uv(
    atlas: &[u8],
    random_seed: u64,
    sequence: u8,
    msec_elapsed: i32,
    norm_time: f32,
) -> FxSpriteAtlasUv {
    if atlas.len() < FX_ELEM_ATLAS_SIZE {
        return FxSpriteAtlasUv::FULL;
    }
    let entry_count = i16::from_le_bytes([atlas[6], atlas[7]]) as i32;
    if entry_count <= 1 {
        return FxSpriteAtlasUv::FULL;
    }
    let behavior = atlas[0];
    let mut index: i32 = match behavior & FX_ATLAS_START_MASK {
        0 => i32::from(atlas[1]),
        FX_ATLAS_START_RANDOM => {
            let lo = u32::from(sample_u16(random_seed, FxRandomChannel::Atlas));
            ((lo.wrapping_mul(entry_count as u32)) >> 16) as i32
        }
        _ => i32::from(sequence) & (entry_count - 1),
    };
    if (behavior & FX_ATLAS_LIFE_SCALE) != 0 {
        index = index.wrapping_add((entry_count as f32 * norm_time) as i32);
    } else {
        let fps = atlas[2];
        if fps != 0 {
            let prod = (msec_elapsed as u32).wrapping_mul(u32::from(fps)) as i32;
            index = index.wrapping_add(prod / 1000);
        }
    }
    let loop_count = i32::from(atlas[3]);
    if (behavior & FX_ATLAS_CLAMP_LAST) != 0 && index >= loop_count.wrapping_mul(entry_count) {
        index = entry_count - 1;
    }
    let wrapped = (index as u32) & ((entry_count as u32).wrapping_sub(1));
    sprite_atlas_cell(wrapped, atlas[4], atlas[5], entry_count as u16)
}

pub fn sprite_atlas_cell(
    atlas_index: u32,
    col_bits: u8,
    row_bits: u8,
    entry_count: u16,
) -> FxSpriteAtlasUv {
    let col_bits = col_bits & 0x1f;
    let row_bits = row_bits & 0x1f;
    let cols = 1u32 << col_bits;
    let rows = 1u32 << row_bits;
    let ds = 1.0 / cols as f32;
    let dt = 1.0 / rows as f32;
    let col = atlas_index & (cols.wrapping_sub(1));
    let row = atlas_index >> col_bits;
    FxSpriteAtlasUv {
        s0: col as f32 * ds,
        ds,
        t0: row as f32 * dt,
        dt,
        entry_count,
        atlas_index: atlas_index as u16,
    }
}
