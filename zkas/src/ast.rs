use indexmap::IndexMap;

use crate::{lexer::Token, opcode::Opcode, types::Type};

#[derive(PartialEq, Clone, Debug)]
pub enum StatementType {
    Assignment,
    Call,
    Noop,
}

pub enum Var {
    Constant(Constant),
    Witness(Witness),
    Variable(Variable),
}

#[derive(Clone, Debug)]
pub struct Variable {
    pub name: String,
    pub typ: Type,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug)]
pub struct Statement {
    pub typ: StatementType,
    pub variable: Option<Variable>,
    pub opcode: Opcode,
    pub args: Vec<Variable>,
    pub line: usize,
}

impl Default for Statement {
    fn default() -> Self {
        Statement {
            typ: StatementType::Noop,
            variable: None,
            opcode: Opcode::Noop,
            args: vec![],
            line: 0,
        }
    }
}

pub type UnparsedConstants = IndexMap<String, (Token, Token)>;
pub type UnparsedWitnesses = IndexMap<String, (Token, Token)>;

pub type Constants = Vec<Constant>;
pub type Witnesses = Vec<Witness>;
pub type Variables = Vec<Variable>;
pub type Statements = Vec<Statement>;

#[derive(Clone, Debug)]
pub struct Constant {
    pub name: String,
    pub typ: Type,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug)]
pub struct Witness {
    pub name: String,
    pub typ: Type,
    pub line: usize,
    pub column: usize,
}
