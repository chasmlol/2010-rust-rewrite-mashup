use crate::script::error::Location;
use crate::script::value::Value;

pub const IR_VERSION: u32 = 5;

#[derive(Clone, Debug)]
pub(crate) struct Function {
    pub(crate) location: Location,
    pub(crate) parameters: usize,
    pub(crate) slots: usize,
    pub(crate) code: Vec<(Location, Op)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Callee {
    Script(u32),
    Native(u32),
    Unlinked(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Global {
    SelfRef,
    Level,
    Game,
    Anim,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unary {
    Not,
    Complement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Binary {
    Or,
    Xor,
    And,
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    Shl,
    Shr,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Case,
}

#[derive(Clone, Debug)]
pub(crate) enum Op {
    Constant(Value),
    FunctionRef(u32),
    Global(Global),
    Load(u32),
    Store(u32),
    Pop,
    Unary(Unary),
    Binary(Binary),
    Vector,
    Jump(usize),
    JumpFalse(usize),
    Call(Callee, usize, bool),
    Spawn(Callee, usize, bool),
    Indirect(usize, bool, bool),
    Array,
    ArrayKeys,
    EnsureLocalArray(u32),
    EnsureFieldArray(u32),
    EnsureIndexArray,
    LoadIndex,
    StoreIndex,
    Dup,
    DupPair,
    Wait,
    FrameEnd,
    Return,
    Size,
    LoadField(u32),
    StoreField(u32),
    Notify(usize),
    Await(Vec<u32>),
    AwaitMatch(usize),
    Endon,
}
