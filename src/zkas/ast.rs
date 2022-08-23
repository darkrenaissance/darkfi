use super::{LitType, Opcode, VarType};

pub struct Constant {
    pub name: String,
    pub typ: VarType,
    pub line: usize,
    pub column: usize,
}

pub struct Witness {
    pub name: String,
    pub typ: VarType,
    pub line: usize,
    pub column: usize,
}

pub struct Variable {
    pub name: String,
    pub typ: VarType,
    pub line: usize,
    pub column: usize,
}

pub struct Literal {
    pub name: String,
    pub typ: LitType,
    pub line: usize,
    pub column: usize,
}

pub enum Arg {
    Var(Variable),
    Lit(Literal),
}

#[repr(u8)]
pub enum StatementType {
    Assign = 0x00,
    Call = 0x01,
}

pub struct Statement {
    pub typ: StatementType,
    pub opcode: Opcode,
    pub lhs: Option<Variable>,
    pub rhs: Vec<Arg>,
    pub line: usize,
}
