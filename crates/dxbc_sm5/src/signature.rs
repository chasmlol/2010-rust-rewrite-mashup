use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::container::read_u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignatureError {
    Truncated,
}

impl fmt::Display for SignatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("signature chunk is truncated")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureElement {
    pub semantic: String,
    pub semantic_index: u32,
    pub system_value: u32,
    pub component_type: u32,
    pub register: u32,
    pub mask: u8,
    pub rw_mask: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Signature {
    pub elements: Vec<SignatureElement>,
}

pub(crate) fn read_cstr(bytes: &[u8], at: usize) -> String {
    let tail = bytes.get(at..).unwrap_or(&[]);
    let end = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
    String::from_utf8_lossy(&tail[..end]).into_owned()
}

impl Signature {
    pub fn parse(chunk: &[u8], stride: usize) -> Result<Self, SignatureError> {
        let err = |_| SignatureError::Truncated;
        let count = read_u32(chunk, 0).map_err(err)? as usize;
        let lead = if stride == 28 { 4 } else { 0 };
        let mut elements = Vec::with_capacity(count);
        for index in 0..count {
            let at = 8 + index * stride + lead;
            let name = read_u32(chunk, at).map_err(err)? as usize;
            let packed = read_u32(chunk, at + 20).map_err(err)?;
            elements.push(SignatureElement {
                semantic: read_cstr(chunk, name),
                semantic_index: read_u32(chunk, at + 4).map_err(err)?,
                system_value: read_u32(chunk, at + 8).map_err(err)?,
                component_type: read_u32(chunk, at + 12).map_err(err)?,
                register: read_u32(chunk, at + 16).map_err(err)?,
                mask: packed as u8,
                rw_mask: (packed >> 8) as u8,
            });
        }
        Ok(Self { elements })
    }
}
