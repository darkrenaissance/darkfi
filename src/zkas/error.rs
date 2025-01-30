/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::io::{self, Error, ErrorKind, Write};

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
        let (err_msg, dbg_msg, caret) = match ln {
            0 => (msg, "".to_string(), "".to_string()),
            _ => {
                let err_msg = format!("{} (line {}, column {})", msg, ln, col);
                let dbg_msg = format!("{}:{}:{}: {}", self.file, ln, col, self.lines[ln - 1]);
                let pad = dbg_msg.split(": ").next().unwrap().len() + col + 1;
                let caret = format!("{:width$}^", "", width = pad);
                (err_msg, dbg_msg, caret)
            }
        };
        format!("{}\n{}\n{}\n", err_msg, dbg_msg, caret)
    }

    pub fn abort(&self, msg: &str, ln: usize, col: usize) -> Error {
        let m = self.fmt(msg.to_string(), ln, col);
        self.emit("error", &m);
        Error::new(ErrorKind::Other, m)
    }

    pub fn warn(&self, msg: &str, ln: usize, col: usize) {
        let m = self.fmt(msg.to_string(), ln, col);
        self.emit("warning", &m);
    }

    pub fn emit(&self, typ: &str, msg: &str) {
        if std::env::var("ZKAS_SILENT").is_ok() {
            return
        }

        let stderr = io::stderr();
        let mut handle = stderr.lock();

        match typ {
            "error" => {
                write!(handle, "\x1b[31;1m{} error:\x1b[0m {}", self.namespace, msg).unwrap()
            }

            "warning" => {
                write!(handle, "\x1b[33;1m{} warning:\x1b[0m {}", self.namespace, msg).unwrap()
            }

            _ => unreachable!(),
        };

        handle.flush().unwrap();
    }
}
