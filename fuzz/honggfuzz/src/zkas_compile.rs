/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

// extern crate darkfi_serial;

use honggfuzz::fuzz;
use darkfi::zkas::{Lexer, Parser, Compiler, Analyzer};
use std::str;

fn main() {
    loop {
        fuzz!(|data: &[u8]| {
            // The lex, parse, compile code below is taken from bin/zkas/src/main.rs
            
            // Use only inputs that can be encoded as .chars(), as this is what the
            // zkas binary uses
            let chars = match str::from_utf8(data) {
                Ok(v) => v.chars(),
                Err(_e) => return,
            };

            let filename = "/dev/null";
            let lexer = Lexer::new(filename, chars.clone());
            let tokens = match lexer.lex() {
                Ok(v) => v,
                Err(_) => return,
            };

            // The parser goes over the tokens provided by the lexer and builds
            // the initial AST, not caring much about the semantics, just enforcing
            // syntax and general structure.
            let parser = Parser::new(filename, chars.clone(), tokens);
            let (namespace, k, constants, witnesses, statements) = match parser.parse() {
                Ok(v) => v,
                Err(_) => return,
            };

            // The analyzer goes through the initial AST provided by the parser and
            // converts return and variable types to their correct forms, and also
            // checks that the semantics of the ZK script are correct.
            let mut analyzer = Analyzer::new(filename, chars.clone(), constants, witnesses, statements);
            if analyzer.analyze_types().is_err() {
                return
            }

            if analyzer.analyze_semantic().is_err() {
                return
            }

            let compiler = Compiler::new(
                filename,
                chars.clone(),
                namespace,
                k,
                analyzer.constants,
                analyzer.witnesses,
                analyzer.statements,
                analyzer.literals,
                false, // a guess
            );

            match compiler.compile() {
                Ok(v) => v,
                Err(_) => return,
            };
                
        });
    }
}
