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

//! `src/zkas` is the library holding the zkas toolchain, consisting of a
//! lexer, parser, static/semantic analyzers, a binary compiler, and a
//! binary decoder.

/// Error emitter
mod error;

/// Constants
pub mod constants;

/// Language opcodes
pub mod opcode;
pub use opcode::Opcode;

/// Language types
pub mod types;
pub use types::{LitType, VarType};

/// Language AST
pub mod ast;

/// Lexer module
pub mod lexer;
pub use lexer::Lexer;

/// Parser module
pub mod parser;
pub use parser::Parser;

/// Analyzer module
pub mod analyzer;
pub use analyzer::Analyzer;

/// Compiler module
pub mod compiler;
pub use compiler::Compiler;

/// Decoder module
pub mod decoder;
pub use decoder::ZkBinary;
