#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;
use alloc::{string::String, vec, vec::Vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataType {
    Int,
    Byte,
    Bool,
    String(usize),
    Enum(usize),
    Struct(usize),
    IndexedArray(usize),
    EnumArray(usize),
    Float,
    Short,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumEntry {
    pub name: String,
    pub index: u16,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Enum {
    pub reserved: usize,
    pub entries: Vec<EnumEntry>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Property {
    pub name: String,
    pub ty: DataType,
    pub offset: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Struct {
    pub properties: Vec<Property>,
    pub size: usize,
    pub bit_offset: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexedArray {
    pub count: usize,
    pub element: DataType,
    pub stride: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnumArray {
    pub enumeration: usize,
    pub element: DataType,
    pub stride: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    pub version: i32,
    pub checksum: u32,
    pub size: usize,
    pub root: DataType,
    pub enums: Vec<Enum>,
    pub structs: Vec<Struct>,
    pub indexed_arrays: Vec<IndexedArray>,
    pub enum_arrays: Vec<EnumArray>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinitionSet {
    pub definitions: Vec<Definition>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key<'a> {
    Name(&'a str),
    Index(i32),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value<'a> {
    Int(i32),
    Bool(bool),
    Float(f32),
    String(&'a str),
    Bytes(&'a [u8]),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidDefinition,
    WrongKey,
    MissingKey,
    ExtraKey,
    UnknownName,
    IndexOutOfRange,
    BufferBounds,
    WrongValue,
    ByteRange,
    StringLength,
    InvalidString,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lookup {
    pub ty: DataType,
    pub offset: usize,
}

impl Definition {
    pub fn validate(&self) -> Result<(), Error> {
        let valid = |ty| match ty {
            DataType::Enum(i) => i < self.enums.len(),
            DataType::Struct(i) => i < self.structs.len(),
            DataType::IndexedArray(i) => i < self.indexed_arrays.len(),
            DataType::EnumArray(i) => i < self.enum_arrays.len(),
            _ => true,
        };
        if !valid(self.root) {
            return Err(Error::InvalidDefinition);
        }
        for en in &self.enums {
            if en
                .entries
                .iter()
                .any(|e| usize::from(e.index) >= en.reserved)
                || en.entries.windows(2).any(|p| p[0].name >= p[1].name)
            {
                return Err(Error::InvalidDefinition);
            }
        }
        for row in &self.structs {
            if row.properties.iter().any(|p| !valid(p.ty))
                || row.properties.windows(2).any(|p| p[0].name >= p[1].name)
            {
                return Err(Error::InvalidDefinition);
            }
        }
        if self.indexed_arrays.iter().any(|a| !valid(a.element))
            || self
                .enum_arrays
                .iter()
                .any(|a| !valid(a.element) || a.enumeration >= self.enums.len())
        {
            return Err(Error::InvalidDefinition);
        }
        LayoutValidation::new(self)?.validate()
    }

    fn lookup_path(&self, keys: &[Key<'_>]) -> Result<Lookup, Error> {
        let mut result = Lookup {
            ty: self.root,
            offset: 0,
        };
        for key in keys {
            let (ty, delta) = match result.ty {
                DataType::Struct(i) => {
                    let Key::Name(name) = key else {
                        return Err(Error::WrongKey);
                    };
                    let name = c_prefix(name);
                    let row = self.structs.get(i).ok_or(Error::InvalidDefinition)?;
                    let p = row
                        .properties
                        .binary_search_by(|p| p.name.as_str().cmp(name))
                        .ok()
                        .and_then(|i| row.properties.get(i))
                        .ok_or(Error::UnknownName)?;
                    (p.ty, p.offset)
                }
                DataType::IndexedArray(i) => {
                    let Key::Index(index) = key else {
                        return Err(Error::WrongKey);
                    };
                    let a = self.indexed_arrays.get(i).ok_or(Error::InvalidDefinition)?;
                    let index = usize::try_from(*index).map_err(|_| Error::IndexOutOfRange)?;
                    if index >= a.count {
                        return Err(Error::IndexOutOfRange);
                    }
                    (
                        a.element,
                        index.checked_mul(a.stride).ok_or(Error::BufferBounds)?,
                    )
                }
                DataType::EnumArray(i) => {
                    let Key::Name(name) = key else {
                        return Err(Error::WrongKey);
                    };
                    let name = c_prefix(name);
                    let a = self.enum_arrays.get(i).ok_or(Error::InvalidDefinition)?;
                    let en = self
                        .enums
                        .get(a.enumeration)
                        .ok_or(Error::InvalidDefinition)?;
                    let entry = en
                        .entries
                        .binary_search_by(|e| e.name.as_str().cmp(name))
                        .ok()
                        .and_then(|i| en.entries.get(i))
                        .ok_or(Error::UnknownName)?;
                    (
                        a.element,
                        usize::from(entry.index)
                            .checked_mul(a.stride)
                            .ok_or(Error::BufferBounds)?,
                    )
                }
                _ => return Err(Error::ExtraKey),
            };
            let base = if ty == DataType::Bool {
                result.offset.checked_mul(8)
            } else {
                Some(result.offset)
            };
            result = Lookup {
                ty,
                offset: base
                    .and_then(|base| base.checked_add(delta))
                    .ok_or(Error::BufferBounds)?,
            };
        }
        Ok(result)
    }

    pub fn path_type(&self, keys: &[Key<'_>]) -> Result<DataType, Error> {
        Ok(self.lookup_path(keys)?.ty)
    }

    pub fn lookup(&self, keys: &[Key<'_>]) -> Result<Lookup, Error> {
        let result = self.lookup_path(keys)?;
        match result.ty {
            DataType::Struct(_) | DataType::IndexedArray(_) | DataType::EnumArray(_) => {
                Err(Error::MissingKey)
            }
            _ => Ok(result),
        }
    }

    fn bytes<'a>(&self, data: &'a [u8], lookup: Lookup, length: usize) -> Result<&'a [u8], Error> {
        let end = lookup
            .offset
            .checked_add(length)
            .ok_or(Error::BufferBounds)?;
        if end > self.size {
            return Err(Error::BufferBounds);
        }
        data.get(lookup.offset..end).ok_or(Error::BufferBounds)
    }

    pub fn enum_index(&self, data: &[u8], lookup: Lookup) -> Result<Option<(u16, bool)>, Error> {
        let DataType::Enum(i) = lookup.ty else {
            return Ok(None);
        };
        let index = u16::from_le_bytes(
            self.bytes(data, lookup, 2)?
                .try_into()
                .map_err(|_| Error::BufferBounds)?,
        );
        let en = self.enums.get(i).ok_or(Error::InvalidDefinition)?;
        Ok(Some((
            index,
            en.entries.iter().any(|entry| entry.index == index),
        )))
    }

    pub fn read<'a>(&'a self, data: &'a [u8], lookup: Lookup) -> Result<Value<'a>, Error> {
        Ok(match lookup.ty {
            DataType::Int => Value::Int(i32::from_le_bytes(
                self.bytes(data, lookup, 4)?
                    .try_into()
                    .map_err(|_| Error::BufferBounds)?,
            )),
            DataType::Byte => Value::Int(i32::from(self.bytes(data, lookup, 1)?[0])),
            DataType::Short => Value::Int(i32::from(i16::from_le_bytes(
                self.bytes(data, lookup, 2)?
                    .try_into()
                    .map_err(|_| Error::BufferBounds)?,
            ))),
            DataType::Float => Value::Float(f32::from_le_bytes(
                self.bytes(data, lookup, 4)?
                    .try_into()
                    .map_err(|_| Error::BufferBounds)?,
            )),
            DataType::Bool => {
                let byte = Lookup {
                    offset: lookup.offset / 8,
                    ..lookup
                };
                Value::Bool(self.bytes(data, byte, 1)?[0] & (1 << (lookup.offset % 8)) != 0)
            }
            DataType::String(length) => {
                let bytes = self.bytes(data, lookup, length)?;
                let end = bytes
                    .iter()
                    .position(|b| *b == 0)
                    .ok_or(Error::InvalidString)?;
                match core::str::from_utf8(&bytes[..end]) {
                    Ok(text) => Value::String(text),
                    Err(_) => Value::Bytes(&bytes[..end]),
                }
            }
            DataType::Enum(i) => {
                let index = u16::from_le_bytes(
                    self.bytes(data, lookup, 2)?
                        .try_into()
                        .map_err(|_| Error::BufferBounds)?,
                );
                let en = self.enums.get(i).ok_or(Error::InvalidDefinition)?;
                let entry = en
                    .entries
                    .iter()
                    .find(|e| e.index == index)
                    .or_else(|| en.entries.iter().find(|e| e.index == 0))
                    .ok_or(Error::InvalidDefinition)?;
                Value::String(&entry.name)
            }
            _ => return Err(Error::WrongValue),
        })
    }

    pub fn write(
        &self,
        data: &mut [u8],
        dirty: &mut [u8],
        lookup: Lookup,
        value: Value<'_>,
    ) -> Result<bool, Error> {
        let mut number = [0u8; 4];
        let mut target = lookup;
        let bytes: &[u8] = match (lookup.ty, value) {
            (DataType::Int, Value::Int(v)) => {
                number = v.to_le_bytes();
                &number
            }
            (DataType::Byte, Value::Int(v)) => {
                number[0] = u8::try_from(v).map_err(|_| Error::ByteRange)?;
                &number[..1]
            }
            (DataType::Short, Value::Int(v)) => {
                number[..2].copy_from_slice(&(v as i16).to_le_bytes());
                &number[..2]
            }
            (DataType::Float, Value::Float(v)) => {
                number = v.to_le_bytes();
                &number
            }
            (DataType::Bool, Value::Bool(v)) => {
                target.offset /= 8;
                let byte = self.bytes(data, target, 1)?[0];
                let mask = 1 << (lookup.offset % 8);
                number[0] = if v { byte | mask } else { byte & !mask };
                &number[..1]
            }
            (DataType::Enum(i), Value::String(v)) => {
                let v = c_prefix(v);
                let en = self.enums.get(i).ok_or(Error::InvalidDefinition)?;
                let entry = en
                    .entries
                    .binary_search_by(|e| e.name.as_str().cmp(v))
                    .ok()
                    .and_then(|i| en.entries.get(i))
                    .ok_or(Error::UnknownName)?;
                number[..2].copy_from_slice(&entry.index.to_le_bytes());
                &number[..2]
            }
            (DataType::String(length), value @ (Value::String(_) | Value::Bytes(_))) => {
                let v = match value {
                    Value::String(text) => text.as_bytes(),
                    Value::Bytes(bytes) => bytes,
                    _ => unreachable!(),
                };
                let end = v.iter().position(|byte| *byte == 0).unwrap_or(v.len());
                let v = &v[..end];
                let length_needed = v.len().checked_add(1).ok_or(Error::StringLength)?;
                if length_needed > length {
                    return Err(Error::StringLength);
                }
                self.bytes(data, target, length)?;
                if dirty.len() < self.size.div_ceil(8) {
                    return Err(Error::BufferBounds);
                }
                let mut changed = false;
                for (i, byte) in v.iter().copied().chain(core::iter::once(0)).enumerate() {
                    changed |= write_byte(data, dirty, target.offset + i, byte);
                }
                return Ok(changed);
            }
            _ => return Err(Error::WrongValue),
        };
        self.bytes(data, target, bytes.len())?;
        if dirty.len() < self.size.div_ceil(8) {
            return Err(Error::BufferBounds);
        }
        let mut changed = false;
        for (i, byte) in bytes.iter().enumerate() {
            changed |= write_byte(data, dirty, target.offset + i, *byte);
        }
        Ok(changed)
    }
}
fn c_prefix(text: &str) -> &str {
    text.find('\0').map_or(text, |end| &text[..end])
}

fn write_byte(data: &mut [u8], dirty: &mut [u8], offset: usize, value: u8) -> bool {
    if data[offset] == value {
        return false;
    }
    data[offset] = value;
    dirty[offset / 8] |= 1 << (offset % 8);
    true
}

struct LayoutValidation<'a> {
    definition: &'a Definition,
    states: Vec<u8>,
    extents: Vec<usize>,
}

impl<'a> LayoutValidation<'a> {
    fn new(definition: &'a Definition) -> Result<Self, Error> {
        let count = definition
            .structs
            .len()
            .checked_add(definition.indexed_arrays.len())
            .and_then(|count| count.checked_add(definition.enum_arrays.len()))
            .ok_or(Error::InvalidDefinition)?;
        Ok(Self {
            definition,
            states: vec![0; count],
            extents: vec![0; count],
        })
    }

    fn node(&self, ty: DataType) -> Option<usize> {
        match ty {
            DataType::Struct(i) => Some(i),
            DataType::IndexedArray(i) => Some(self.definition.structs.len() + i),
            DataType::EnumArray(i) => {
                Some(self.definition.structs.len() + self.definition.indexed_arrays.len() + i)
            }
            _ => None,
        }
    }

    fn bits(bytes: usize) -> Result<usize, Error> {
        bytes.checked_mul(8).ok_or(Error::InvalidDefinition)
    }

    fn extent(&self, ty: DataType) -> Result<usize, Error> {
        match ty {
            DataType::Bool => Ok(1),
            DataType::Byte => Ok(8),
            DataType::Short | DataType::Enum(_) => Ok(16),
            DataType::Int | DataType::Float => Ok(32),
            DataType::String(0) => Err(Error::InvalidDefinition),
            DataType::String(length) => Self::bits(length),
            _ => self
                .node(ty)
                .and_then(|i| self.extents.get(i).copied())
                .ok_or(Error::InvalidDefinition),
        }
    }

    fn array_extent(&self, count: usize, element: DataType, stride: usize) -> Result<usize, Error> {
        let stride = if element == DataType::Bool {
            stride
        } else {
            Self::bits(stride)?
        };
        let element = self.extent(element)?;
        if stride < element {
            return Err(Error::InvalidDefinition);
        }
        let bits = count.checked_mul(stride).ok_or(Error::InvalidDefinition)?;
        Self::bits(bits.div_ceil(8))
    }

    fn finish(&self, ty: DataType) -> Result<usize, Error> {
        match ty {
            DataType::Struct(i) => {
                let row = &self.definition.structs[i];
                let mut extent = 0;
                for property in &row.properties {
                    let start = if property.ty == DataType::Bool {
                        property.offset
                    } else {
                        Self::bits(property.offset)?
                    };
                    let end = start
                        .checked_add(self.extent(property.ty)?)
                        .ok_or(Error::InvalidDefinition)?;
                    extent = extent.max(end);
                }
                let extent = Self::bits(extent.div_ceil(8))?;
                let size = if row.size == 0 {
                    extent
                } else {
                    Self::bits(row.size)?
                };
                if extent > size || row.bit_offset > size {
                    return Err(Error::InvalidDefinition);
                }
                Ok(size)
            }
            DataType::IndexedArray(i) => {
                let row = &self.definition.indexed_arrays[i];
                self.array_extent(row.count, row.element, row.stride)
            }
            DataType::EnumArray(i) => {
                let row = &self.definition.enum_arrays[i];
                self.array_extent(
                    self.definition.enums[row.enumeration].reserved,
                    row.element,
                    row.stride,
                )
            }
            _ => self.extent(ty),
        }
    }

    fn validate(mut self) -> Result<(), Error> {
        let mut stack = Vec::new();
        for i in 0..self.states.len() {
            let ty = if i < self.definition.structs.len() {
                DataType::Struct(i)
            } else if i < self.definition.structs.len() + self.definition.indexed_arrays.len() {
                DataType::IndexedArray(i - self.definition.structs.len())
            } else {
                DataType::EnumArray(
                    i - self.definition.structs.len() - self.definition.indexed_arrays.len(),
                )
            };
            stack.push((ty, false));
            while let Some((ty, finish)) = stack.pop() {
                let Some(node) = self.node(ty) else {
                    self.extent(ty)?;
                    continue;
                };
                if finish {
                    self.extents[node] = self.finish(ty)?;
                    self.states[node] = 2;
                    continue;
                }
                match self.states[node] {
                    2 => continue,
                    1 => return Err(Error::InvalidDefinition),
                    _ => {}
                }
                self.states[node] = 1;
                stack.push((ty, true));
                match ty {
                    DataType::Struct(i) => stack.extend(
                        self.definition.structs[i]
                            .properties
                            .iter()
                            .map(|property| (property.ty, false)),
                    ),
                    DataType::IndexedArray(i) => {
                        stack.push((self.definition.indexed_arrays[i].element, false))
                    }
                    DataType::EnumArray(i) => {
                        stack.push((self.definition.enum_arrays[i].element, false))
                    }
                    _ => unreachable!(),
                }
            }
        }
        if self.extent(self.definition.root)? > Self::bits(self.definition.size)? {
            return Err(Error::InvalidDefinition);
        }
        Ok(())
    }
}
