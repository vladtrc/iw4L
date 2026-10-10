use crate::error::Location;
use crate::value::Value;

pub const IR_VERSION: u32 = 5;

#[derive(Clone, Debug)]
pub struct Function {
    pub location: Location,
    pub parameters: usize,
    pub slots: usize,
    pub code: Vec<(Location, Op)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callee {
    Script(u32),
    Native(u32),
    Unlinked(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Global {
    SelfRef,
    Level,
    Game,
    Anim,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unary {
    Not,
    Complement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binary {
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
pub enum Op {
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
