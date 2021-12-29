use crate::opcode::Opcode;

pub enum StatementType {
    Assignment,
    Call,
    Noop,
}

pub struct Variable {
    pub name: String,
    pub line: usize,
    pub column: usize,
}

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
