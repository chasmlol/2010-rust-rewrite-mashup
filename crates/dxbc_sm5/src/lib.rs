#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod container;
pub mod ir;
pub mod program;
pub mod reflection;
pub mod signature;
pub mod wgsl;

pub use container::{Chunk, Container, ContainerError};
pub use ir::{
    Components, DecodedInstruction, Index, IrError, Modifier, Operand, RegisterType,
    decode_instruction,
};
pub use program::{Instruction, OpcodeName, Program, ProgramError, ProgramKind};
pub use reflection::{Binding, BindingKind, ConstantBuffer, Reflection, ReflectionError, Variable};
pub use signature::{Signature, SignatureElement, SignatureError};
