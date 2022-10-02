use std::str::Chars;

use super::{
    ast::{Arg, Constant, Literal, Statement, StatementType, Witness},
    error::ErrorEmitter,
    types::StackType,
};
use crate::serial::{serialize, VarInt};

/// Version of the binary
pub const BINARY_VERSION: u8 = 2;
/// Magic bytes prepended to the binary
pub const MAGIC_BYTES: [u8; 4] = [0x0b, 0x01, 0xb1, 0x35];

pub struct Compiler {
    constants: Vec<Constant>,
    witnesses: Vec<Witness>,
    statements: Vec<Statement>,
    literals: Vec<Literal>,
    debug_info: bool,
    error: ErrorEmitter,
}

impl Compiler {
    pub fn new(
        filename: &str,
        source: Chars,
        constants: Vec<Constant>,
        witnesses: Vec<Witness>,
        statements: Vec<Statement>,
        literals: Vec<Literal>,
        debug_info: bool,
    ) -> Self {
        // For nice error reporting, we'll load everything into a string
        // vector so we have references to lines.
        let lines: Vec<String> = source.as_str().lines().map(|x| x.to_string()).collect();
        let error = ErrorEmitter::new("Compiler", filename, lines);

        Self { constants, witnesses, statements, literals, debug_info, error }
    }

    pub fn compile(&self) -> Vec<u8> {
        let mut bincode = vec![];

        // Write the magic bytes and version
        bincode.extend_from_slice(&MAGIC_BYTES);
        bincode.push(BINARY_VERSION);

        // Temporaty stack vector for lookups
        let mut tmp_stack = vec![];

        // In the .constant section of the binary, we write the constant's type,
        // and the name so the VM can look it up from `src/crypto/constants/`.
        bincode.extend_from_slice(b".constant");
        for i in &self.constants {
            tmp_stack.push(i.name.as_str());
            bincode.push(i.typ as u8);
            bincode.extend_from_slice(&serialize(&i.name));
        }

        // Currently, our literals are only Uint64 types, in the binary we'll
        // add them here in the .literal section. In the VM, they will be on
        // their own stack, used for reference by opcodes.
        bincode.extend_from_slice(b".literal");
        for i in &self.literals {
            bincode.push(i.typ as u8);
            bincode.extend_from_slice(&serialize(&i.name));
        }

        // In the .contract section, we write all our witness types, on the stack
        // they're in order of appearance.
        bincode.extend_from_slice(b".contract");
        for i in &self.witnesses {
            tmp_stack.push(i.name.as_str());
            bincode.push(i.typ as u8);
        }

        bincode.extend_from_slice(b".circuit");
        for i in &self.statements {
            match i.typ {
                StatementType::Assign => tmp_stack.push(&i.lhs.as_ref().unwrap().name),
                // In case of a simple call, we don't append anything to the stack
                StatementType::Call => {}
                _ => unreachable!(),
            }

            bincode.push(i.opcode as u8);
            bincode.extend_from_slice(&serialize(&VarInt(i.rhs.len() as u64)));

            for arg in &i.rhs {
                match arg {
                    Arg::Var(arg) => {
                        if let Some(found) = Compiler::lookup_stack(&tmp_stack, &arg.name) {
                            bincode.push(StackType::Var as u8);
                            bincode.extend_from_slice(&serialize(&VarInt(found as u64)));
                            continue
                        }

                        self.error.abort(
                            &format!("Failed finding a stack reference for `{}`", arg.name),
                            arg.line,
                            arg.column,
                        );
                    }
                    Arg::Lit(lit) => {
                        if let Some(found) = Compiler::lookup_literal(&self.literals, &lit.name) {
                            bincode.push(StackType::Lit as u8);
                            bincode.extend_from_slice(&serialize(&VarInt(found as u64)));
                            continue
                        }

                        self.error.abort(
                            &format!("Failed finding literal `{}`", lit.name),
                            lit.line,
                            lit.column,
                        );
                    }
                    _ => unreachable!(),
                };
            }
        }

        // If we're not doing debug info, we're done here and can return.
        if !self.debug_info {
            return bincode
        }

        // TODO: Otherwise, we proceed appending debug info.

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

    fn lookup_literal(literals: &[Literal], name: &str) -> Option<usize> {
        for (idx, n) in literals.iter().enumerate() {
            if n.name == name {
                return Some(idx)
            }
        }

        None
    }
}
