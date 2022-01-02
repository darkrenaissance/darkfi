use std::{io, io::Write, process, str::Chars};

use termion::{color, style};

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

    pub fn analyze(self) {
        self.error("Semantic analyzer not implemented".to_string(), 1, 0);
    }

    fn error(&self, msg: String, ln: usize, col: usize) {
        let err_msg = format!("{} (line {}, column {})", msg, ln, col);
        let dbg_msg = format!("{}:{}:{}: {}", self.file, ln, col, self.lines[ln - 1]);
        let pad = dbg_msg.split(": ").next().unwrap().len() + col + 2;
        let caret = format!("{:width$}^", "", width = pad);
        let msg = format!("{}\n{}\n{}\n", err_msg, dbg_msg, caret);
        Analyzer::abort(&msg);
    }

    fn abort(msg: &str) {
        let stderr = io::stderr();
        let mut handle = stderr.lock();
        write!(
            handle,
            "{}{}Semantic error:{} {}",
            style::Bold,
            color::Fg(color::Red),
            style::Reset,
            msg,
        )
        .unwrap();
        handle.flush().unwrap();
        process::exit(1);
    }
}
