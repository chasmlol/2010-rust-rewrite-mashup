use crate::{AssetType, LoadedAsset, Ptr, WalkError, ZoneLoad};

pub const SURFACE_SIZE: usize = 80;
pub const VERTEX_SIZE: usize = 36;

pub struct WorldView<'a> {
    load: &'a ZoneLoad,
    asset: &'a LoadedAsset,
    surface_address: Ptr,
    surfaces: &'a [u8],
    pub vertex_count: usize,
    pub stream0: &'a [u8],
    pub stream1: &'a [u8],
    pub indices: &'a [u8],
}

#[derive(Clone, Copy)]
pub struct SurfaceView<'a> {
    world: &'a WorldView<'a>,
    ordinal: usize,
    bytes: &'a [u8],
}

#[derive(Clone, Copy, Debug)]
pub struct VertexView<'a> {
    pub stream_offset: usize,
    pub bytes: &'a [u8; VERTEX_SIZE],
}

fn word(bytes: &[u8], at: usize) -> Result<u32, WalkError> {
    let value = bytes.get(at..at + 4).ok_or(WalkError::Schema)?;
    Ok(u32::from_le_bytes(value.try_into().unwrap()))
}

fn array<'a>(
    load: &'a ZoneLoad,
    header: &[u8],
    pointer_at: usize,
    len: usize,
) -> Result<&'a [u8], WalkError> {
    if len == 0 {
        return Ok(&[]);
    }
    let pointer = crate::walk::decode_ptr(word(header, pointer_at)?).ok_or(WalkError::Schema)?;
    load.blocks.bytes(pointer, len)
}

impl<'a> WorldView<'a> {
    pub fn new(load: &'a ZoneLoad, asset: &'a LoadedAsset) -> Result<Self, WalkError> {
        if asset.ty != AssetType::GfxWorld || asset.header.len() != 1028 {
            return Err(WalkError::Schema);
        }
        let surface_count = word(&asset.header, 16)? as usize;
        let surface_address =
            crate::walk::decode_ptr(word(&asset.header, 872)?).ok_or(WalkError::Schema)?;
        let surfaces = load.blocks.bytes(
            surface_address,
            surface_count
                .checked_mul(SURFACE_SIZE)
                .ok_or(WalkError::Schema)?,
        )?;
        Ok(Self {
            load,
            asset,
            surface_address,
            surfaces,
            vertex_count: word(&asset.header, 424)? as usize,
            stream0: array(load, &asset.header, 432, word(&asset.header, 428)? as usize)?,
            stream1: array(load, &asset.header, 444, word(&asset.header, 440)? as usize)?,
            indices: array(
                load,
                &asset.header,
                456,
                (word(&asset.header, 452)? as usize)
                    .checked_mul(2)
                    .ok_or(WalkError::Schema)?,
            )?,
        })
    }

    pub fn surfaces(&'a self) -> impl ExactSizeIterator<Item = SurfaceView<'a>> {
        self.surfaces
            .as_chunks::<SURFACE_SIZE>()
            .0
            .iter()
            .enumerate()
            .map(|(ordinal, bytes)| SurfaceView {
                world: self,
                ordinal,
                bytes,
            })
    }
}

impl<'a> SurfaceView<'a> {
    pub fn ordinal(self) -> usize {
        self.ordinal
    }

    pub fn bounds(self) -> [[f32; 3]; 2] {
        core::array::from_fn(|side| {
            core::array::from_fn(|axis| {
                f32::from_bits(word(self.bytes, 56 + side * 12 + axis * 4).unwrap())
            })
        })
    }

    pub fn stream_offset(self, stream: usize) -> Option<usize> {
        let at = match stream {
            0 => 12,
            1 => 28,
            _ => return None,
        };
        usize::try_from(word(self.bytes, at).ok()? as i32).ok()
    }

    pub fn first_vertex(self) -> u32 {
        word(self.bytes, 32).unwrap()
    }

    pub fn triangle_count(self) -> usize {
        u16::from_le_bytes(self.bytes[42..44].try_into().unwrap()) as usize
    }

    pub fn material(self) -> Option<&'a LoadedAsset> {
        let offset = u32::try_from(self.ordinal.checked_mul(SURFACE_SIZE)?).ok()?;
        self.world
            .load
            .asset_in(self.world.asset, self.world.surface_address.at(offset + 48))
    }

    pub fn lightmap_index(self) -> u8 {
        self.bytes[52]
    }

    pub fn reflection_probe_index(self) -> u8 {
        self.bytes[53]
    }

    pub fn primary_light_index(self) -> u8 {
        self.bytes[54]
    }

    pub fn flags(self) -> u8 {
        self.bytes[55]
    }

    pub fn local_indices(self) -> Result<&'a [u8], WalkError> {
        let first = (word(self.bytes, 44)? as usize)
            .checked_mul(2)
            .ok_or(WalkError::Schema)?;
        let end = first
            .checked_add(self.triangle_count() * 6)
            .ok_or(WalkError::Schema)?;
        self.world.indices.get(first..end).ok_or(WalkError::Schema)
    }

    pub fn vertex(self, local_index: u16) -> Result<VertexView<'a>, WalkError> {
        let base = self.stream_offset(0).ok_or(WalkError::Schema)?;
        let stream_offset = base
            .checked_add(usize::from(local_index) * VERTEX_SIZE)
            .ok_or(WalkError::Schema)?;
        let end = stream_offset
            .checked_add(VERTEX_SIZE)
            .ok_or(WalkError::Schema)?;
        let bytes = self
            .world
            .stream0
            .get(stream_offset..end)
            .ok_or(WalkError::Schema)?;
        Ok(VertexView {
            stream_offset,
            bytes: bytes.try_into().unwrap(),
        })
    }
}

impl VertexView<'_> {
    pub fn position(self) -> [f32; 3] {
        core::array::from_fn(|axis| f32::from_bits(word(self.bytes, axis * 4).unwrap()))
    }
}
