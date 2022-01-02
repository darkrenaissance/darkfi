use std::str::Chars;

use crate::ast::{Constants, Statements, Witnesses};

pub struct Analyzer {
    file: String,
    lines: Vec<String>,
    constants: Constants,
    witnesses: Witnesses,
    statements: Statements,
}

impl Analyzer {
    pub fn new(
        filename: &str,
        source: Chars,
        constants: Constants,
        witnesses: Witnesses,
        statements: Statements,
    ) -> Self {
        // For nice error reporting, we'll load everything into a string
        // vector so we have references to lines.
        let lines = source.as_str().lines().map(|x| x.to_string()).collect();
        Analyzer { file: filename.to_string(), lines, constants, witnesses, statements }
    }

    pub fn analyze(self) {}
}
