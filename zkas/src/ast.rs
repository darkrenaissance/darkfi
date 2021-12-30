use crate::opcode::Opcode;

#[derive(PartialEq, Clone, Debug)]
pub enum StatementType {
    Assignment,
    Call,
    Noop,
}

#[derive(Clone, Debug)]
pub struct Variable {
    pub name: String,
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
