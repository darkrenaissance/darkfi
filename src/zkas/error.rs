use std::{io, io::Write, process};

use termion::{color, style};

pub(super) struct ErrorEmitter {
    namespace: String,
    file: String,
    lines: Vec<String>,
}

impl ErrorEmitter {
    pub fn new(namespace: &str, file: &str, lines: Vec<String>) -> Self {
        Self { namespace: namespace.to_string(), file: file.to_string(), lines }
    }

    pub fn emit(&self, msg: String, ln: usize, col: usize) {
        let err_msg = format!("{} (line{}, column {})", msg, ln, col);
        let dbg_msg = format!("{}:{}:{}: {}", self.file, ln, col, self.lines[ln - 1]);
        let pad = dbg_msg.split(": ").next().unwrap().len() + col + 2;
        let caret = format!("{:width$}^", "", width = pad);
        let msg = format!("{}\n{}\n{}\n", err_msg, dbg_msg, caret);
        self.abort(&msg);
    }

    fn abort(&self, msg: &str) {
        let stderr = io::stderr();
        let mut handle = stderr.lock();
        write!(
            handle,
            "{}{}{} error:{} {}",
            style::Bold,
            color::Fg(color::Red),
            self.namespace,
            style::Reset,
            msg,
        )
        .unwrap();
        handle.flush().unwrap();
        process::exit(1);
    }
}
