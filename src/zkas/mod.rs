//! `src/zkas` is the library holding the zkas toolchain, consisting of a
//! lexer, parser, static/semantic analyzers, a binary compiler, and a
//! binary decoder.

/// Error emitter
mod error;

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
