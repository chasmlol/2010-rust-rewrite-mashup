use bevy::render::render_resource::{
    BlendComponent, BlendFactor as WgpuBlendFactor, BlendOperation, BlendState, ColorWrites,
};
use d3d9_state::BlendFactor;

pub use render_material::state::{
    AuthoredStateFields, DrawBlendComponent, DrawBlendOperation, UnsupportedStateFields,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DrawBlend(render_material::state::DrawBlend);

impl DrawBlend {
    pub fn blend_state(self) -> Option<BlendState> {
        let (color, alpha) = match self.0 {
            render_material::state::DrawBlend::Opaque => return None,
            render_material::state::DrawBlend::Factors { colour, alpha } => {
                (blend_component(colour), blend_component(alpha))
            }

            render_material::state::DrawBlend::Multiply { alpha } => (
                BlendComponent {
                    src_factor: WgpuBlendFactor::Zero,
                    dst_factor: WgpuBlendFactor::Src,
                    operation: BlendOperation::Add,
                },
                blend_component(alpha),
            ),
        };
        Some(BlendState { color, alpha })
    }
}
fn blend_component(component: DrawBlendComponent) -> BlendComponent {
    if matches!(
        component.operation,
        DrawBlendOperation::Min | DrawBlendOperation::Max
    ) {
        return BlendComponent {
            src_factor: WgpuBlendFactor::One,
            dst_factor: WgpuBlendFactor::One,
            operation: blend_operation(component.operation).unwrap(),
        };
    }
    BlendComponent {
        src_factor: d3d_blend_to_wgpu(component.src)
            .expect("unsupported D3D9 source blend factor reached the GPU adapter"),
        dst_factor: d3d_blend_to_wgpu(component.dst)
            .expect("unsupported D3D9 destination blend factor reached the GPU adapter"),
        operation: blend_operation(component.operation)
            .expect("unsupported D3D9 blend operation reached the GPU adapter"),
    }
}
fn blend_operation(operation: DrawBlendOperation) -> Option<BlendOperation> {
    Some(match operation {
        DrawBlendOperation::Add => BlendOperation::Add,
        DrawBlendOperation::Subtract => BlendOperation::Subtract,
        DrawBlendOperation::ReverseSubtract => BlendOperation::ReverseSubtract,
        DrawBlendOperation::Min => BlendOperation::Min,
        DrawBlendOperation::Max => BlendOperation::Max,
        DrawBlendOperation::Unknown(_) => return None,
    })
}
fn d3d_blend_to_wgpu(factor: BlendFactor) -> Option<WgpuBlendFactor> {
    Some(match factor {
        BlendFactor::Zero => WgpuBlendFactor::Zero,
        BlendFactor::One => WgpuBlendFactor::One,
        BlendFactor::SrcColor => WgpuBlendFactor::Src,
        BlendFactor::InvSrcColor => WgpuBlendFactor::OneMinusSrc,
        BlendFactor::SrcAlpha => WgpuBlendFactor::SrcAlpha,
        BlendFactor::InvSrcAlpha => WgpuBlendFactor::OneMinusSrcAlpha,
        BlendFactor::DestAlpha => WgpuBlendFactor::DstAlpha,
        BlendFactor::InvDestAlpha => WgpuBlendFactor::OneMinusDstAlpha,
        BlendFactor::DestColor => WgpuBlendFactor::Dst,
        BlendFactor::InvDestColor => WgpuBlendFactor::OneMinusDst,
        BlendFactor::SrcAlphaSat => WgpuBlendFactor::SrcAlphaSaturated,
        BlendFactor::Unknown(_) => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChangeState0Host {
    pub blend: DrawBlend,

    pub cull: u8,
    pub srgb_write: bool,

    pub colour_write: u8,
    pub line_fill: bool,

    pub alpha_test: Option<d3d9_state::AlphaTest>,
}

impl ChangeState0Host {
    pub fn colour_writes(self) -> ColorWrites {
        let mut writes = ColorWrites::empty();
        if self.colour_write & 1 != 0 {
            writes |= ColorWrites::RED | ColorWrites::GREEN | ColorWrites::BLUE;
        }
        if self.colour_write & 2 != 0 {
            writes |= ColorWrites::ALPHA;
        }
        writes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChangeState1Host {
    pub stencil: u32,
    pub depth_write: bool,
    pub depth_test_enable: bool,

    pub depth_func: u8,

    pub polyoffset_level: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GfxPassState(render_material::CompiledPassState);

impl GfxPassState {
    pub fn from_prepared(state: render_material::CompiledPassState) -> Self {
        Self(state)
    }
    pub fn authored_words(self) -> [u32; 2] {
        self.0.authored_words()
    }
    pub fn srgb_write_enable(self) -> bool {
        self.0.srgb_write_enable()
    }
    pub fn authored_host_fields(self) -> AuthoredStateFields {
        self.0.authored_host_fields()
    }
    pub fn unsupported_host_fields(self) -> Option<UnsupportedStateFields> {
        self.0.unsupported_host_fields()
    }
    pub fn apply_change_state_0_host(
        self,
        _alpha_mode: bevy::prelude::AlphaMode,
        multiply_pass: bool,
    ) -> ChangeState0Host {
        ChangeState0Host {
            blend: DrawBlend(self.0.blend(multiply_pass)),
            cull: self.0.cull(),
            srgb_write: self.0.srgb_write_enable(),
            colour_write: self.0.colour_write(),
            line_fill: self.0.line_fill(),
            alpha_test: self.0.alpha_test(),
        }
    }
    pub fn apply_change_state_1_host(self) -> ChangeState1Host {
        ChangeState1Host {
            stencil: self.0.stencil(),
            depth_write: self.0.depth_write(),
            depth_test_enable: self.0.depth_test_enable(),
            depth_func: self.0.depth_func(),
            polyoffset_level: self.0.polyoffset_level(),
        }
    }
}
