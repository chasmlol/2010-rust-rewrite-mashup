use crate::envelope::MAX_XFILE_COUNT;

pub const XFILE_HEADER_LEN: usize = 8 + 4 * MAX_XFILE_COUNT;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZoneHeader {
    pub size: u32,
    pub external_size: u32,
    pub block_size: [u32; MAX_XFILE_COUNT],
}

impl ZoneHeader {
    pub fn parse(image: &[u8]) -> Option<Self> {
        let head = image.get(..XFILE_HEADER_LEN)?;
        let rd = |o: usize| u32::from_le_bytes(head[o..o + 4].try_into().unwrap());
        Some(Self {
            size: rd(0),
            external_size: rd(4),
            block_size: core::array::from_fn(|i| rd(8 + i * 4)),
        })
    }
}
