#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialDrawMode {
    Opaque,

    AlphaTest { ge_half: bool },
    Blend,

    Multiply,
    Additive,
    Screen,
}

impl MaterialDrawMode {
    pub fn from_state_bits(load_bits: [u32; 2]) -> Self {
        let word0 = load_bits[0];
        let alpha_test_disabled = (word0 >> 11) & 1 != 0;
        let alpha_test = (word0 >> 12) & 0b11;
        let blend_op = (word0 >> 8) & 0b111;
        let src_blend = word0 & 0xf;
        let dst_blend = (word0 >> 4) & 0xf;

        if blend_op != 0 {
            if blend_op == 1 && src_blend == 1 && dst_blend == 3 {
                return Self::Multiply;
            }
            if blend_op == 1 && src_blend == 2 && dst_blend == 2 {
                return Self::Additive;
            }
            if blend_op == 1 && src_blend == 10 && dst_blend == 2 {
                return Self::Screen;
            }
            return Self::Blend;
        }
        if !alpha_test_disabled {
            return Self::AlphaTest {
                ge_half: alpha_test == 3,
            };
        }
        Self::Opaque
    }

    pub fn alpha_cutoff(self) -> Option<f32> {
        match self {
            Self::AlphaTest { ge_half: true } => Some(0.5),
            Self::AlphaTest { ge_half: false } => Some(0.01),
            _ => None,
        }
    }
}

pub fn alpha_test_cutoff_from_state_bits(load_bits: [u32; 2]) -> Option<f32> {
    let word0 = load_bits[0];
    let alpha_test_disabled = (word0 >> 11) & 1 != 0;
    if alpha_test_disabled {
        return None;
    }
    let alpha_test = (word0 >> 12) & 0b11;
    Some(if alpha_test == 3 { 0.5 } else { 0.01 })
}

pub const GFXS0_SRGBWRITEENABLE: u32 = 0x4000_0000;

#[inline]
pub fn srgb_write_enable_from_state_bits(load_bits: [u32; 2]) -> bool {
    (load_bits[0] & GFXS0_SRGBWRITEENABLE) != 0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMapTransform {
    Square,

    Unknown,
}

pub use asset_iw4::{
    AlphaTest, D3DCULL_CCW, D3DCULL_CW, D3DCULL_NONE, D3DRS_CULLMODE, GFXS0_ATEST_DISABLE,
    GFXS0_ATEST_GE_128, GFXS0_ATEST_GT_0, GFXS0_ATEST_LT_128, GFXS0_ATEST_MASK, GFXS0_ATEST_SHIFT,
    GFXS0_CULL_BACK, GFXS0_CULL_FRONT, GFXS0_CULL_MASK, GFXS0_CULL_NONE, GFXS0_CULL_SHIFT,
    Gfxs0AlphaTest, Gfxs0CullFace as MaterialCullFace, S_ALPHA_TEST_TABLE, S_CULL_TABLE,
    alpha_test_from_state_bits, cull_face_from_state_bits, d3d_cull_mode_from_state_bits,
};

pub fn material_constant_name(name: &[u8; 12]) -> &str {
    let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
    std::str::from_utf8(&name[..end]).unwrap_or("")
}
