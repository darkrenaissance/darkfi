use std::str::Chars;

use super::{
    ast::{Constants, StatementType, Statements, Witnesses},
    error::ErrorEmitter,
};
use crate::util::serial::{serialize, VarInt};

/// Version of the binary
pub const BINARY_VERSION: u8 = 1;
/// Magic bytes prepended to the binary
pub const MAGIC_BYTES: [u8; 4] = [0x0b, 0x00, 0xb1, 0x35];

pub struct Compiler {
    constants: Constants,
    witnesses: Witnesses,
    statements: Statements,
    debug_info: bool,
    error: ErrorEmitter,
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
        let lines: Vec<String> = source.as_str().lines().map(|x| x.to_string()).collect();
        let error = ErrorEmitter::new("Compiler", filename, lines.clone());

        Compiler { constants, witnesses, statements, debug_info, error }
    }

    pub fn compile(&self) -> Vec<u8> {
        let mut bincode = vec![];

        // Write the magic bytes and version
        bincode.extend_from_slice(&MAGIC_BYTES);
        bincode.push(BINARY_VERSION);

        // Temporary stack vector for lookups
        let mut tmp_stack = vec![];

        bincode.extend_from_slice(b".constant");
        for i in &self.constants {
            tmp_stack.push(i.name.as_str());
            bincode.push(i.typ as u8);
            bincode.extend_from_slice(&serialize(&i.name));
        }

        bincode.extend_from_slice(b".contract");
        for i in &self.witnesses {
            tmp_stack.push(i.name.as_str());
            bincode.push(i.typ as u8);
        }

        bincode.extend_from_slice(b".circuit");
        for i in &self.statements {
            match i.typ {
                StatementType::Assignment => {
                    tmp_stack.push(&i.variable.as_ref().unwrap().name);
                }
                // In case of a simple call, we don't append anything to the stack
                StatementType::Call => {}
                _ => unreachable!(),
            }

            bincode.push(i.opcode as u8);
            bincode.extend_from_slice(&serialize(&VarInt(i.args.len() as u64)));

            for arg in &i.args {
                if let Some(found) = Compiler::lookup_stack(&tmp_stack, &arg.name) {
                    bincode.extend_from_slice(&serialize(&VarInt(found as u64)));
                    continue
                }

                self.error.emit(
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

    fn lookup_stack(stack: &[&str], name: &str) -> Option<usize> {
        for (idx, n) in stack.iter().enumerate() {
            if n == &name {
                return Some(idx)
            }
        }

        None
    }
}
