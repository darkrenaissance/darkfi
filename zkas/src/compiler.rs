use std::str::Chars;

use crate::ast::{Constants, Statements, Variables, Witnesses};

pub struct Compiler {
    file: String,
    lines: Vec<String>,
    constants: Constants,
    witnesses: Witnesses,
    statements: Statements,
    stack: Variables,
    debug_info: bool,
}

impl Compiler {
    pub fn new(
        filename: &str,
        source: Chars,
        constants: Constants,
        witnesses: Witnesses,
        statements: Statements,
        stack: Variables,
        debug_info: bool,
    ) -> Self {
        // For nice error reporting, we'll load everything into a string
        // vector so we have references to lines.
        let lines = source.as_str().lines().map(|x| x.to_string()).collect();
        Compiler {
            file: filename.to_string(),
            lines,
            constants,
            witnesses,
            statements,
            stack,
            debug_info,
        }
    }

    pub fn compile(&self) -> Vec<u8> {
        if self.debug_info {
            return self.compile_with_debug_info()
        }

        self.compile_without_debug_info()
    }

    fn compile_with_debug_info(&self) -> Vec<u8> {
        vec![]
    }

    fn compile_without_debug_info(&self) -> Vec<u8> {
        vec![]
    }
}
