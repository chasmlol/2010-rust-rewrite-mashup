use asset_iw4::vertex_decl as iw4;
use fastfile_t5::vertex_decl as t5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VertexLayoutFamily {
    #[default]
    Iw4,
    T5,
    T6,
}

pub const T5_WORLD_LAYER_HOST_STRIDE: usize = 40;

pub const T6_VERTEX_LIGHTING_SOURCE: u8 = iw4::STREAM_SOURCE_COUNT as u8;

pub const T6_VERTEX_LIGHTING_STREAM: u8 = 1;

const fn t6_vertex_lighting_streamed(vertex_type: u8) -> bool {
    vertex_type == iw4::PACKED_VERTEX_TYPE || vertex_type == iw4::STATICMODELCACHE_VERTEX_TYPE
}

impl VertexLayoutFamily {
    pub fn source_count(self) -> usize {
        match self {
            Self::Iw4 => iw4::STREAM_SOURCE_COUNT,
            Self::T5 => 10,
            Self::T6 => iw4::STREAM_SOURCE_COUNT + 1,
        }
    }
    pub fn destination_usage(self, dest: u8) -> Option<(u8, u8)> {
        match self {
            Self::Iw4 | Self::T6 => iw4::destination_usage(dest),
            Self::T5 => t5::DESTINATION_USAGE_TABLE
                .get(usize::from(dest))
                .map(|&[u, i]| (u, i)),
        }
    }
    pub fn source_layout(self, vertex_type: u8, source: u8) -> Option<iw4::StreamSourceLayout> {
        match self {
            Self::Iw4 => iw4::source_layout(vertex_type, source),
            Self::T6 if source == T6_VERTEX_LIGHTING_SOURCE => {
                if t6_vertex_lighting_streamed(vertex_type) {
                    Some(iw4::StreamSourceLayout {
                        stream: T6_VERTEX_LIGHTING_STREAM,
                        offset: 0,
                        decl_type: iw4::D3dDeclType::UByte4N,
                    })
                } else {
                    iw4::source_layout(vertex_type, 2)
                }
            }
            Self::T6 => iw4::source_layout(vertex_type, source),
            Self::T5 => {
                let &[stream, offset, raw] = t5::SOURCE_LAYOUT_TABLE
                    .get(usize::from(vertex_type))?
                    .get(usize::from(source))?;
                (stream != 255).then_some(iw4::StreamSourceLayout {
                    stream,
                    offset,
                    decl_type: iw4::D3dDeclType::from_raw(raw),
                })
            }
        }
    }
    pub fn host_stream_stride(self, vertex_type: u8, stream: u8) -> Option<u16> {
        match self {
            Self::Iw4 => iw4::stream_extent(vertex_type, stream),
            Self::T6
                if stream == T6_VERTEX_LIGHTING_STREAM
                    && t6_vertex_lighting_streamed(vertex_type) =>
            {
                Some(4)
            }
            Self::T6 => iw4::stream_extent(vertex_type, stream),
            Self::T5 if stream == 1 && (2..=13).contains(&vertex_type) => {
                Some(T5_WORLD_LAYER_HOST_STRIDE as u16)
            }
            Self::T5 => (0..10)
                .filter_map(|i| self.source_layout(vertex_type, i))
                .filter(|l| l.stream == stream)
                .filter_map(|l| {
                    l.decl_type
                        .byte_len()
                        .map(|len| u16::from(l.offset) + u16::from(len))
                })
                .max(),
        }
    }
}
