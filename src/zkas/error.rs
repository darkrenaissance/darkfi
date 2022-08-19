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

    fn fmt(&self, msg: String, ln: usize, col: usize) -> String {
        let err_msg = format!("{} (line {}, column {})", msg, ln, col);
        let (dbg_msg, caret) = match ln {
            0 => ("".to_string(), "".to_string()),
            _ => {
                let dbg_msg = format!("{}:{}:{}: {}", self.file, ln, col, self.lines[ln - 1]);
                let pad = dbg_msg.split(": ").next().unwrap().len() + col + 2;
                let caret = format!("{:width$}^", "", width = pad);
                (dbg_msg, caret)
            }
        };
        format!("{}\n{}\n{}\n", err_msg, dbg_msg, caret)
    }

    pub fn abort(&self, msg: &str, ln: usize, col: usize) {
        let m = self.fmt(msg.to_string(), ln, col);
        self.emit("error", &m);
        process::exit(1);
    }

    pub fn warn(&self, msg: &str, ln: usize, col: usize) {
        let m = self.fmt(msg.to_string(), ln, col);
        self.emit("warning", &m);
    }

    pub fn emit(&self, typ: &str, msg: &str) {
        let stderr = io::stderr();
        let mut handle = stderr.lock();

        match typ {
            "error" => write!(
                handle,
                "{}{}{} error:{} {}",
                style::Bold,
                color::Fg(color::Red),
                self.namespace,
                style::Reset,
                msg
            )
            .unwrap(),

            "warning" => write!(
                handle,
                "{}{}{} warning:{} {}",
                style::Bold,
                color::Fg(color::Yellow),
                self.namespace,
                style::Reset,
                msg
            )
            .unwrap(),

            _ => unreachable!(),
        };

        handle.flush().unwrap();
    }
}
