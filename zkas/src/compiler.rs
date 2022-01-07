use std::{io, io::Write, process, str::Chars};

use termion::{color, style};

use crate::ast::{Constants, StatementType, Statements, Witnesses};

/// Version of the binary
pub const BINARY_VERSION: u8 = 1;
/// Magic bytes prepended to the binary
pub const MAGIC_BYTES: [u8; 4] = [0x0b, 0x00, 0xb1, 0x35];

pub struct Compiler {
    file: String,
    lines: Vec<String>,
    constants: Constants,
    witnesses: Witnesses,
    statements: Statements,
    debug_info: bool,
}

impl Compiler {
    pub fn new(
        filename: &str,
        source: Chars,
        constants: Constants,
        witnesses: Witnesses,
        statements: Statements,
        debug_info: bool,
    ) -> Self {
        // For nice error reporting, we'll load everything into a string
        // vector so we have references to lines.
        let lines = source.as_str().lines().map(|x| x.to_string()).collect();
        Compiler { file: filename.to_string(), lines, constants, witnesses, statements, debug_info }
    }

    // TODO: varint encoding
    pub fn compile(&self) -> Vec<u8> {
        let mut bincode = vec![];

        // Write the magic bytes and version
        bincode.extend_from_slice(&MAGIC_BYTES);
        bincode.push(BINARY_VERSION);

        let mut stack_idx: u64 = 0;

        // Temporary stack vector for lookups
        let mut tmp_stack = vec![];

        bincode.extend_from_slice(b".constant");
        for i in &self.constants {
            tmp_stack.push(i.name.as_str());
            bincode.push(i.typ as u8);
            bincode.extend_from_slice(&stack_idx.to_le_bytes());
            bincode.extend_from_slice(i.name.as_bytes());
            stack_idx += 1;
        }

        bincode.extend_from_slice(b".contract");
        for i in &self.witnesses {
            tmp_stack.push(i.name.as_str());
            bincode.push(i.typ as u8);
            stack_idx += 1;
        }

        bincode.extend_from_slice(b".circuit");
        for i in &self.statements {
            match i.typ {
                StatementType::Assignment => {
                    tmp_stack.push(&i.variable.as_ref().unwrap().name);
                    stack_idx += 1;
                }
                // In case of a simple call, we don't append anything to the stack
                StatementType::Call => {}
                _ => unreachable!(),
            }

            bincode.push(i.opcode as u8);
            bincode.extend_from_slice(&i.args.len().to_le_bytes());

            for arg in &i.args {
                if let Some(found) = Compiler::lookup_stack(&tmp_stack, &arg.name) {
                    bincode.extend_from_slice(&found.to_le_bytes());
                    continue
                }

                self.error(
                    format!("Failed finding a stack reference for `{}`", arg.name),
                    arg.line,
                    arg.column,
                );
            }
        }

        // If we're not doing debug info, we're done here and can return.
        if !self.debug_info {
            return bincode
        }

        // TODO: Otherwise, we proceed appending debug info

        bincode
    }

    fn lookup_stack(stack: &[&str], name: &str) -> Option<u64> {
        for (idx, n) in stack.iter().enumerate() {
            if n == &name {
                return Some(idx.try_into().unwrap())
            }
        }

        None
    }

    fn error(&self, msg: String, ln: usize, col: usize) {
        let err_msg = format!("{} (line {}, column {})", msg, ln, col);
        let dbg_msg = format!("{}:{}:{}: {}", self.file, ln, col, self.lines[ln - 1]);
        let pad = dbg_msg.split(": ").next().unwrap().len() + col + 2;
        let caret = format!("{:width$}^", "", width = pad);
        let msg = format!("{}\n{}\n{}\n", err_msg, dbg_msg, caret);
        Compiler::abort(&msg);
    }

    fn abort(msg: &str) {
        let stderr = io::stderr();
        let mut handle = stderr.lock();
        write!(
            handle,
            "{}{}Compiler error:{} {}",
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
