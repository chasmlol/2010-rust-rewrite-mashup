use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::container::read_u32;
use crate::signature::read_cstr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReflectionError {
    Truncated,
}

impl fmt::Display for ReflectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RDEF chunk is truncated")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variable {
    pub name: String,
    pub start_offset: u32,
    pub size: u32,
    pub flags: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstantBuffer {
    pub name: String,
    pub size: u32,
    pub variables: Vec<Variable>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingKind {
    ConstantBuffer,
    TextureBuffer,
    Texture,
    Sampler,
    Other(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub name: String,
    pub kind: BindingKind,
    pub dimension: u32,
    pub bind_point: u32,
    pub bind_count: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reflection {
    pub constant_buffers: Vec<ConstantBuffer>,
    pub bindings: Vec<Binding>,
}

impl Reflection {
    pub fn parse(chunk: &[u8]) -> Result<Self, ReflectionError> {
        let u = |at: usize| read_u32(chunk, at).map_err(|_| ReflectionError::Truncated);
        let (buffer_count, buffer_at) = (u(0)? as usize, u(4)? as usize);
        let (binding_count, binding_at) = (u(8)? as usize, u(12)? as usize);
        let major = (u(16)? >> 8) & 0xff;
        let variable_stride = if major >= 5 { 40 } else { 24 };
        let mut constant_buffers = Vec::with_capacity(buffer_count);
        for index in 0..buffer_count {
            let at = buffer_at + 24 * index;
            let (variable_count, variable_at) = (u(at + 4)? as usize, u(at + 8)? as usize);
            let mut variables = Vec::with_capacity(variable_count);
            for v in 0..variable_count {
                let vat = variable_at + variable_stride * v;
                variables.push(Variable {
                    name: read_cstr(chunk, u(vat)? as usize),
                    start_offset: u(vat + 4)?,
                    size: u(vat + 8)?,
                    flags: u(vat + 12)?,
                });
            }
            constant_buffers.push(ConstantBuffer {
                name: read_cstr(chunk, u(at)? as usize),
                size: u(at + 12)?,
                variables,
            });
        }
        let mut bindings = Vec::with_capacity(binding_count);
        for index in 0..binding_count {
            let at = binding_at + 32 * index;
            bindings.push(Binding {
                name: read_cstr(chunk, u(at)? as usize),
                kind: match u(at + 4)? {
                    0 => BindingKind::ConstantBuffer,
                    1 => BindingKind::TextureBuffer,
                    2 => BindingKind::Texture,
                    3 => BindingKind::Sampler,
                    other => BindingKind::Other(other),
                },
                dimension: u(at + 12)?,
                bind_point: u(at + 20)?,
                bind_count: u(at + 24)?,
            });
        }
        Ok(Self {
            constant_buffers,
            bindings,
        })
    }
}
