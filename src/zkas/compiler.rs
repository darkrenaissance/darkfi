/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::{io::Result, str::Chars};

use darkfi_serial::{serialize, VarInt};

use super::{
    ast::{Arg, Constant, Literal, Statement, StatementType, Witness},
    error::ErrorEmitter,
    types::HeapType,
};

/// Version of the binary
pub const BINARY_VERSION: u8 = 2;
/// Magic bytes prepended to the binary
pub const MAGIC_BYTES: [u8; 4] = [0x0b, 0x01, 0xb1, 0x35];

pub struct Compiler {
    namespace: String,
    k: u32,
    constants: Vec<Constant>,
    witnesses: Vec<Witness>,
    statements: Vec<Statement>,
    literals: Vec<Literal>,
    debug_info: bool,
    error: ErrorEmitter,
}

impl Compiler {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        filename: &str,
        source: Chars,
        namespace: String,
        k: u32,
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

        Self { namespace, k, constants, witnesses, statements, literals, debug_info, error }
    }

    pub fn compile(&self) -> Result<Vec<u8>> {
        let mut bincode = vec![];

        // Write the magic bytes and version
        bincode.extend_from_slice(&MAGIC_BYTES);
        bincode.push(BINARY_VERSION);

        // Write the circuit's k param
        bincode.extend_from_slice(&serialize(&self.k));

        // Write the circuit's namespace
        bincode.extend_from_slice(&serialize(&self.namespace));

        // Temporary heap vector for lookups
        let mut tmp_heap = vec![];

        // In the .constant section of the binary, we write the constant's type,
        // and the name so the VM can look it up from `src/crypto/constants/`.
        bincode.extend_from_slice(b".constant");
        for i in &self.constants {
            tmp_heap.push(i.name.as_str());
            bincode.push(i.typ as u8);
            bincode.extend_from_slice(&serialize(&i.name));
        }

        // Currently, our literals are only Uint64 types, in the binary we'll
        // add them here in the .literal section. In the VM, they will be on
        // their own heap, used for reference by opcodes.
        bincode.extend_from_slice(b".literal");
        for i in &self.literals {
            bincode.push(i.typ as u8);
            bincode.extend_from_slice(&serialize(&i.name));
        }

        // In the .witness section, we write all our witness types, on the heap
        // they're in order of appearance.
        bincode.extend_from_slice(b".witness");
        for i in &self.witnesses {
            tmp_heap.push(i.name.as_str());
            bincode.push(i.typ as u8);
        }

        bincode.extend_from_slice(b".circuit");
        for i in &self.statements {
            match i.typ {
                StatementType::Assign => tmp_heap.push(&i.lhs.as_ref().unwrap().name),
                // In case of a simple call, we don't append anything to the heap
                StatementType::Call => {}
                _ => unreachable!("Invalid statement type in circuit: {:?}", i.typ),
            }

            bincode.push(i.opcode as u8);
            bincode.extend_from_slice(&serialize(&VarInt(i.rhs.len() as u64)));

            for arg in &i.rhs {
                match arg {
                    Arg::Var(arg) => {
                        let heap_idx =
                            Compiler::lookup_heap(&tmp_heap, &arg.name).ok_or_else(|| {
                                self.error.abort(
                                    &format!("Failed finding a heap reference for `{}`", arg.name),
                                    arg.line,
                                    arg.column,
                                )
                            })?;

                        bincode.push(HeapType::Var as u8);
                        bincode.extend_from_slice(&serialize(&VarInt(heap_idx as u64)));
                    }
                    Arg::Lit(lit) => {
                        let lit_idx = Compiler::lookup_literal(&self.literals, &lit.name)
                            .ok_or_else(|| {
                                self.error.abort(
                                    &format!("Failed finding literal `{}`", lit.name),
                                    lit.line,
                                    lit.column,
                                )
                            })?;

                        bincode.push(HeapType::Lit as u8);
                        bincode.extend_from_slice(&serialize(&VarInt(lit_idx as u64)));
                    }
                    _ => unreachable!(),
                };
            }
        }

        // If we're not doing debug info, we're done here and can return.
        if !self.debug_info {
            return Ok(bincode)
        }

        // TODO: Otherwise, we proceed appending debug info.

        Ok(bincode)
    }

    fn lookup_heap(heap: &[&str], name: &str) -> Option<usize> {
        heap.iter().position(|&n| n == name)
    }

    fn lookup_literal(literals: &[Literal], name: &str) -> Option<usize> {
        literals.iter().position(|n| n.name == name)
    }
}
