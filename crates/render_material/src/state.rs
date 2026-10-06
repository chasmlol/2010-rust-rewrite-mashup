use d3d9_state::{AlphaTest, BlendFactor};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum DrawBlendOperation {
    Add,
    Subtract,
    ReverseSubtract,
    Min,
    Max,
    Unknown(u8),
}

impl DrawBlendOperation {
    fn from_raw(raw: u8) -> Self {
        match raw {
            1 => Self::Add,
            2 => Self::Subtract,
            3 => Self::ReverseSubtract,
            4 => Self::Min,
            5 => Self::Max,
            value => Self::Unknown(value),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct DrawBlendComponent {
    pub src: BlendFactor,
    pub dst: BlendFactor,
    pub operation: DrawBlendOperation,
}

impl DrawBlendComponent {
    fn from_bits(bits: u32) -> Self {
        Self {
            src: BlendFactor::from_raw(bits & 0xf),
            dst: BlendFactor::from_raw((bits >> 4) & 0xf),
            operation: DrawBlendOperation::from_raw(((bits >> 8) & 0x7) as u8),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum DrawBlend {
    #[default]
    Opaque,

    Factors {
        colour: DrawBlendComponent,
        alpha: DrawBlendComponent,
    },

    Multiply {
        alpha: DrawBlendComponent,
    },
}

impl DrawBlend {
    fn from_word0(word0: u32, multiply_pass: bool) -> Self {
        if multiply_pass {
            let colour = DrawBlendComponent {
                src: BlendFactor::Zero,
                dst: BlendFactor::SrcColor,
                operation: DrawBlendOperation::Add,
            };
            let alpha_bits = (word0 >> 16) & 0x7ff;
            let alpha = if alpha_bits & 0x700 == 0 {
                colour
            } else {
                DrawBlendComponent::from_bits(alpha_bits)
            };
            return Self::Multiply { alpha };
        }
        let blend_op = (word0 >> 8) & 0b111;
        let src = word0 & 0xf;
        let dst = (word0 >> 4) & 0xf;
        if blend_op == 0 {
            return Self::Opaque;
        }

        let colour = DrawBlendComponent::from_bits(word0);
        let alpha_bits = (word0 >> 16) & 0x7ff;
        let alpha = if alpha_bits & 0x700 == 0 {
            colour
        } else {
            DrawBlendComponent::from_bits(alpha_bits)
        };
        if blend_op == 1 && src == 1 && dst == 3 {
            Self::Multiply { alpha }
        } else {
            Self::Factors { colour, alpha }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct UnsupportedStateFields {
    pub unknown_blend_factor: bool,
    pub unknown_blend_operation: bool,
    pub stencil: bool,
}

impl UnsupportedStateFields {
    pub fn any(self) -> bool {
        self.unknown_blend_factor || self.unknown_blend_operation || self.stencil
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AuthoredStateFields {
    pub non_add_blend: bool,
    pub independent_alpha_blend: bool,
    pub partial_colour_write: bool,
    pub line_fill: bool,
    pub stencil: bool,
}

/// Execution must reject `unsupported_host_fields` before adapting this product to a GPU.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CompiledPassState {
    authored_words: [u32; 2],
    blend: DrawBlend,
    multiply_blend: DrawBlend,
    alpha_test: Option<AlphaTest>,
    cull: u8,
    srgb_write: bool,
    colour_write: u8,
    line_fill: bool,
    stencil: u32,
    depth_write: bool,
    depth_test_enable: bool,
    depth_func: u8,
    polyoffset_level: u8,
    authored: AuthoredStateFields,
    unsupported: UnsupportedStateFields,
}

impl CompiledPassState {
    pub fn authored_words(self) -> [u32; 2] {
        self.authored_words
    }
    pub fn blend(self, multiply_pass: bool) -> DrawBlend {
        if multiply_pass {
            self.multiply_blend
        } else {
            self.blend
        }
    }
    pub fn alpha_test(self) -> Option<AlphaTest> {
        self.alpha_test
    }
    pub fn cull(self) -> u8 {
        self.cull
    }
    pub fn srgb_write_enable(self) -> bool {
        self.srgb_write
    }
    pub fn colour_write(self) -> u8 {
        self.colour_write
    }
    pub fn line_fill(self) -> bool {
        self.line_fill
    }
    pub fn stencil(self) -> u32 {
        self.stencil
    }
    pub fn depth_write(self) -> bool {
        self.depth_write
    }
    pub fn depth_test_enable(self) -> bool {
        self.depth_test_enable
    }
    pub fn depth_func(self) -> u8 {
        self.depth_func
    }
    pub fn polyoffset_level(self) -> u8 {
        self.polyoffset_level
    }
    pub fn authored_host_fields(self) -> AuthoredStateFields {
        self.authored
    }
    pub fn unsupported_host_fields(self) -> Option<UnsupportedStateFields> {
        self.unsupported.any().then_some(self.unsupported)
    }
}

pub fn compile_packed_state(
    words: [u32; 2],
    alpha_test: Option<AlphaTest>,
    cull: u8,
) -> CompiledPassState {
    let [word0, word1] = words;
    let colour_op = ((word0 >> 8) & 0x7) as u8;
    let colour_blend = word0 & 0x7ff;
    let alpha_blend = (word0 >> 16) & 0x7ff;
    let alpha_op = ((alpha_blend >> 8) & 0x7) as u8;
    let colour = DrawBlendComponent::from_bits(word0);
    let alpha = DrawBlendComponent::from_bits(alpha_blend);
    CompiledPassState {
        authored_words: words,
        blend: DrawBlend::from_word0(word0, false),
        multiply_blend: DrawBlend::from_word0(word0, true),
        alpha_test,
        cull,
        srgb_write: word0 & 0x4000_0000 != 0,
        colour_write: u8::from(word0 & 0x0800_0000 != 0)
            | (u8::from(word0 & 0x1000_0000 != 0) << 1),
        line_fill: word0 & 0x8000_0000 != 0,
        stencil: word1 & 0xffff_ffc0,
        depth_write: word1 & 1 != 0,
        depth_test_enable: word1 & 2 == 0,
        depth_func: ((word1 >> 2) & 3) as u8,
        polyoffset_level: ((word1 >> 4) & 3) as u8,
        authored: AuthoredStateFields {
            non_add_blend: colour_op > 1 || (colour_op != 0 && alpha_op > 1),
            independent_alpha_blend: colour_op != 0 && alpha_op != 0 && alpha_blend != colour_blend,
            partial_colour_write: word0 & 0x1800_0000 != 0x1800_0000,
            line_fill: word0 & 0x8000_0000 != 0,
            stencil: word1 & 0xc0 != 0,
        },
        unsupported: UnsupportedStateFields {
            unknown_blend_factor: colour_op != 0
                && (!colour.src.is_known()
                    || !colour.dst.is_known()
                    || (alpha_op != 0 && (!alpha.src.is_known() || !alpha.dst.is_known()))),
            unknown_blend_operation: colour_op > 5 || (colour_op != 0 && alpha_op > 5),
            stencil: false,
        },
    }
}
