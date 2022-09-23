use super::{LitType, Opcode, VarType};

#[derive(Clone, Debug)]
pub struct Constant {
    pub name: String,
    pub typ: VarType,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug)]
pub struct Witness {
    pub name: String,
    pub typ: VarType,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug)]
pub struct Variable {
    pub name: String,
    pub typ: VarType,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug)]
pub struct Literal {
    pub name: String,
    pub typ: LitType,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug)]
pub enum Var {
    Constant(Constant),
    Witness(Witness),
    Variable(Variable),
}

#[derive(Clone, Debug)]
pub enum Arg {
    Var(Variable),
    Lit(Literal),
    Func(Statement),
}

#[derive(Copy, Clone, PartialEq, Debug)]
#[repr(u8)]
pub enum StatementType {
    Noop = 0x00,
    Assign = 0x01,
    Call = 0x02,
}

#[derive(Clone, Debug)]
pub struct Statement {
    pub typ: StatementType,
    pub opcode: Opcode,
    pub lhs: Option<Variable>,
    pub rhs: Vec<Arg>,
    pub line: usize,
}

impl Default for Statement {
    fn default() -> Self {
        Self { typ: StatementType::Noop, opcode: Opcode::Noop, lhs: None, rhs: vec![], line: 0 }
    }
}
