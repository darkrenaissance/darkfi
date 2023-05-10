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

use std::{iter::Peekable, str::Chars};

use indexmap::IndexMap;
use itertools::Itertools;

use super::{
    ast::{Arg, Constant, Literal, Statement, StatementType, Variable, Witness},
    error::ErrorEmitter,
    lexer::{Token, TokenType},
    LitType, Opcode, VarType,
};

/// zkas language builtin keywords.
/// These can not be used anywhere except where they are expected.
const KEYWORDS: [&str; 3] = ["constant", "witness", "circuit"];

/// Forbidden namespaces
const NOPE_NS: [&str; 4] = [".constant", ".literal", ".witness", ".circuit"];

/// Valid EcFixedPoint constant names supported by the VM.
const VALID_ECFIXEDPOINT: [&str; 1] = ["VALUE_COMMIT_RANDOM"];

/// Valid EcFixedPointShort constant names supported by the VM.
const VALID_ECFIXEDPOINTSHORT: [&str; 1] = ["VALUE_COMMIT_VALUE"];

/// Valid EcFixedPointBase constant names supported by the VM.
const VALID_ECFIXEDPOINTBASE: [&str; 1] = ["NULLIFIER_K"];

pub struct Parser {
    tokens: Vec<Token>,
    error: ErrorEmitter,
}

impl Parser {
    pub fn new(filename: &str, source: Chars, tokens: Vec<Token>) -> Self {
        // For nice error reporting, we'll load everything into a string
        // vector so we have references to lines.
        let lines: Vec<String> = source.as_str().lines().map(|x| x.to_string()).collect();
        let error = ErrorEmitter::new("Parser", filename, lines);

        Self { tokens, error }
    }

    pub fn parse(&self) -> (String, Vec<Constant>, Vec<Witness>, Vec<Statement>) {
        // We use these to keep state while parsing.
        let mut namespace = None;
        let (mut declaring_constant, mut declared_constant) = (false, false);
        let (mut declaring_witness, mut declared_witness) = (false, false);
        let (mut declaring_circuit, mut declared_circuit) = (false, false);

        // The tokens gathered from each of the sections
        let mut constant_tokens = vec![];
        let mut witness_tokens = vec![];
        let mut circuit_tokens = vec![];

        // Tokens belonging to the current statement
        let mut circuit_stmt = vec![];
        // All completed statements are pushed here
        let mut circuit_stmts = vec![];
        // Contains constant and witness sections
        let mut ast_inner = IndexMap::new();
        let mut ast = IndexMap::new();

        if self.tokens[0].token_type != TokenType::Symbol {
            self.error.abort(
                "Source file does not start with a section. Expected `constant/witness/circuit`.",
                0,
                0,
            );
        }

        let mut iter = self.tokens.iter();
        while let Some(t) = iter.next() {
            // Sections "constant", "witness", and "circuit" are
            // the sections we must be declaring in our source code.
            // When we find one, we'll take all the tokens found in
            // the section and place them in their respective vec.
            // NOTE: Currently this logic depends on the fact that
            // the sections are closed off with braces. This should
            // be revisited later when we decide to add other lang
            // functionality that also depends on using braces.
            if !declaring_constant && !declaring_witness && !declaring_circuit {
                //
                // We use this macro to avoid code repetition in the following
                // match statement for soaking up the section tokens.
                macro_rules! absorb_inner_tokens {
                    ($v:ident) => {
                        for inner in iter.by_ref() {
                            if KEYWORDS.contains(&inner.token.as_str()) &&
                                inner.token_type == TokenType::Symbol
                            {
                                self.error.abort(
                                    &format!("Keyword '{}' used in improper place.", inner.token),
                                    inner.line,
                                    inner.column,
                                );
                            }

                            $v.push(inner.clone());
                            if inner.token_type == TokenType::RightBrace {
                                break
                            }
                        }
                    };
                }

                match t.token.as_str() {
                    "constant" => {
                        declaring_constant = true;
                        absorb_inner_tokens!(constant_tokens);
                    }
                    "witness" => {
                        declaring_witness = true;
                        absorb_inner_tokens!(witness_tokens);
                    }
                    "circuit" => {
                        declaring_circuit = true;
                        absorb_inner_tokens!(circuit_tokens);
                    }

                    x => self.error.abort(
                        &format!("Section `{}` is not a valid section", x),
                        t.line,
                        t.column,
                    ),
                }
            }

            // We use this macro to set or check that the namespace of all sections
            // is the same and no stray strings appeared.
            macro_rules! check_namespace {
                ($t:ident) => {
                    if let Some(ns) = namespace.clone() {
                        if ns != $t[0].token {
                            self.error.abort(
                                &format!("Found '{}' namespace, expected '{}'.", $t[0].token, ns),
                                $t[0].line,
                                $t[0].column,
                            );
                        }
                    } else {
                        if NOPE_NS.contains(&$t[0].token.as_str()) {
                            self.error.abort(
                                &format!("'{}' cannot be a namespace.", $t[0].token),
                                $t[0].line,
                                $t[0].column,
                            );
                        }
                        namespace = Some($t[0].token.clone());
                    }
                };
            }

            // Parse the constant section into the AST.
            if declaring_constant {
                if declared_constant {
                    self.error.abort("Duplicate `constant` section found.", t.line, t.column);
                }

                self.check_section_structure("constant", constant_tokens.clone());
                check_namespace!(constant_tokens);

                let mut constants_map = IndexMap::new();
                // This is everything between the braces: { ... }
                let mut constant_inner = constant_tokens[2..constant_tokens.len() - 1].iter();
                while let Some((typ, name, comma)) = constant_inner.next_tuple() {
                    if comma.token_type != TokenType::Comma {
                        self.error.abort("Separator is not a comma.", comma.line, comma.column);
                    }

                    // No variable shadowing
                    if constants_map.contains_key(name.token.as_str()) {
                        self.error.abort(
                            &format!(
                                "Section `constant` already contains the token `{}`.",
                                &name.token
                            ),
                            name.line,
                            name.column,
                        );
                    }

                    constants_map.insert(name.token.clone(), (name.clone(), typ.clone()));
                }

                if constant_inner.next().is_some() {
                    self.error.abort("Internal error, leftovers in 'constant' iterator", 0, 0);
                }

                ast_inner.insert("constant".to_string(), constants_map);
                declaring_constant = false;
                declared_constant = true;
            }

            // Parse the witness section into the AST.
            if declaring_witness {
                if declared_witness {
                    self.error.abort("Duplicate `witness` section found.", t.line, t.column);
                }

                self.check_section_structure("witness", witness_tokens.clone());
                check_namespace!(witness_tokens);

                let mut witnesses_map = IndexMap::new();
                // This is everything between the braces: { ... }
                let mut witness_inner = witness_tokens[2..witness_tokens.len() - 1].iter();
                while let Some((typ, name, comma)) = witness_inner.next_tuple() {
                    if comma.token_type != TokenType::Comma {
                        self.error.abort("Separator is not a comma.", comma.line, comma.column);
                    }

                    // No variable shadowing
                    if witnesses_map.contains_key(name.token.as_str()) {
                        self.error.abort(
                            &format!(
                                "Section `witness` already contains the token `{}`.",
                                &name.token
                            ),
                            name.line,
                            name.column,
                        );
                    }

                    witnesses_map.insert(name.token.clone(), (name.clone(), typ.clone()));
                }

                if witness_inner.next().is_some() {
                    self.error.abort("Internal error, leftovers in 'witness' iterator", 0, 0);
                }

                ast_inner.insert("witness".to_string(), witnesses_map);
                declaring_witness = false;
                declared_witness = true;
            }

            // Parse the circuit section into the AST.
            if declaring_circuit {
                if declared_circuit {
                    self.error.abort("Duplicate `circuit` section found.", t.line, t.column);
                }

                self.check_section_structure("circuit", circuit_tokens.clone());
                check_namespace!(circuit_tokens);

                // Grab tokens for each statement
                for i in circuit_tokens[2..circuit_tokens.len() - 1].iter() {
                    if i.token_type == TokenType::Semicolon {
                        // Push completed statement to the heap
                        circuit_stmts.push(circuit_stmt.clone());
                        circuit_stmt = vec![];
                        continue
                    }
                    circuit_stmt.push(i.clone());
                }

                declaring_circuit = false;
                declared_circuit = true;
            }
        }

        // Tokens have been processed and ast is complete

        let ns = namespace.unwrap();
        ast.insert(ns.clone(), ast_inner);

        let constants = {
            let c = match ast.get(&ns).unwrap().get("constant") {
                Some(c) => c,
                None => {
                    self.error.abort("Missing `constant` section in .zk source.", 0, 0);
                    unreachable!();
                }
            };
            self.parse_ast_constants(c)
        };

        let witnesses = {
            let c = match ast.get(&ns).unwrap().get("witness") {
                Some(c) => c,
                None => {
                    self.error.abort("Missing `witness` section in .zk source.", 0, 0);
                    unreachable!();
                }
            };
            self.parse_ast_witness(c)
        };

        let statements = self.parse_ast_circuit(circuit_stmts);
        if statements.is_empty() {
            self.error.abort("Circuit section is empty.", 0, 0);
        }

        (ns, constants, witnesses, statements)
    }

    /// Routine checks on section structure
    fn check_section_structure(&self, section: &str, tokens: Vec<Token>) {
        if tokens[0].token_type != TokenType::String {
            self.error.abort(
                "Section declaration must start with a naming string.",
                tokens[0].line,
                tokens[0].column,
            );
        }

        if tokens[1].token_type != TokenType::LeftBrace {
            self.error.abort(
                "Section must be opened with a left brace '{'",
                tokens[0].line,
                tokens[0].column,
            );
        }

        if tokens.last().unwrap().token_type != TokenType::RightBrace {
            self.error.abort(
                "Section must be closed with a right brace '}'",
                tokens[0].line,
                tokens[0].column,
            );
        }

        match section {
            "constant" | "witness" => {
                if tokens.len() == 3 {
                    self.error.warn(&format!("{} section is empty.", section), 0, 0);
                }

                if tokens[2..tokens.len() - 1].len() % 3 != 0 {
                    self.error.abort(
                        &format!("Invalid number of elements in '{}' section. Must be pairs of '<Type> <name>' separated with a comma ','.", section),
                        tokens[0].line,
                        tokens[0].column
                    );
                }
            }
            "circuit" => {
                if tokens.len() == 3 {
                    self.error.abort("circuit section is empty.", 0, 0);
                }

                if tokens[tokens.len() - 2].token_type != TokenType::Semicolon {
                    self.error.abort(
                        "Circuit section does not end with a semicolon. Would never finish parsing.",
                        tokens[tokens.len()-2].line,
                        tokens[tokens.len()-2].column,
                    );
                }
            }
            _ => unreachable!(),
        };
    }

    fn parse_ast_constants(&self, ast: &IndexMap<String, (Token, Token)>) -> Vec<Constant> {
        let mut ret = vec![];

        // k = name
        // v = (name, type)
        for (k, v) in ast {
            if &v.0.token != k {
                self.error.abort(
                    &format!("Constant name `{}` doesn't match token `{}`.", v.0.token, k),
                    v.0.line,
                    v.0.column,
                );
            }

            if v.0.token_type != TokenType::Symbol {
                self.error.abort(
                    &format!("Constant name `{}` is not a symbol.", v.0.token),
                    v.0.line,
                    v.0.column,
                );
            }

            if v.1.token_type != TokenType::Symbol {
                self.error.abort(
                    &format!("Constant type `{}` is not a symbol.", v.1.token),
                    v.1.line,
                    v.1.column,
                );
            }

            // Valid constant types, these are the constants/generators supported
            // in `src/crypto/constants.rs` and `src/crypto/constants/`.
            match v.1.token.as_str() {
                "EcFixedPoint" => {
                    if !VALID_ECFIXEDPOINT.contains(&v.0.token.as_str()) {
                        self.error.abort(
                            &format!(
                                "`{}` is not a valid EcFixedPoint constant. Supported: {:?}",
                                v.0.token.as_str(),
                                VALID_ECFIXEDPOINT
                            ),
                            v.0.line,
                            v.0.column,
                        );
                    }

                    ret.push(Constant {
                        name: k.to_string(),
                        typ: VarType::EcFixedPoint,
                        line: v.1.line,
                        column: v.1.column,
                    });
                }

                "EcFixedPointShort" => {
                    if !VALID_ECFIXEDPOINTSHORT.contains(&v.0.token.as_str()) {
                        self.error.abort(
                            &format!(
                                "`{}` is not a valid EcFixedPointShort constant. Supported: {:?}",
                                v.0.token.as_str(),
                                VALID_ECFIXEDPOINTSHORT
                            ),
                            v.0.line,
                            v.0.column,
                        );
                    }

                    ret.push(Constant {
                        name: k.to_string(),
                        typ: VarType::EcFixedPointShort,
                        line: v.1.line,
                        column: v.1.column,
                    });
                }

                "EcFixedPointBase" => {
                    if !VALID_ECFIXEDPOINTBASE.contains(&v.0.token.as_str()) {
                        self.error.abort(
                            &format!(
                                "`{}` is not a valid EcFixedPointBase constant. Supported: {:?}",
                                v.0.token.as_str(),
                                VALID_ECFIXEDPOINTBASE
                            ),
                            v.0.line,
                            v.0.column,
                        );
                    }

                    ret.push(Constant {
                        name: k.to_string(),
                        typ: VarType::EcFixedPointBase,
                        line: v.1.line,
                        column: v.1.column,
                    });
                }

                x => {
                    self.error.abort(
                        &format!("`{}` is an unsupported constant type.", x),
                        v.1.line,
                        v.1.column,
                    );
                }
            }
        }

        ret
    }

    fn parse_ast_witness(&self, ast: &IndexMap<String, (Token, Token)>) -> Vec<Witness> {
        let mut ret = vec![];

        // k = name
        // v = (name, type)
        for (k, v) in ast {
            if &v.0.token != k {
                self.error.abort(
                    &format!("Witness name `{}` doesn't match token `{}`.", v.0.token, k),
                    v.0.line,
                    v.0.column,
                );
            }

            if v.0.token_type != TokenType::Symbol {
                self.error.abort(
                    &format!("Witness name `{}` is not a symbol.", v.0.token),
                    v.0.line,
                    v.0.column,
                );
            }

            if v.1.token_type != TokenType::Symbol {
                self.error.abort(
                    &format!("Witness type `{}` is not a symbol.", v.1.token),
                    v.1.line,
                    v.1.column,
                );
            }

            // Valid witness types
            match v.1.token.as_str() {
                "EcPoint" => {
                    ret.push(Witness {
                        name: k.to_string(),
                        typ: VarType::EcPoint,
                        line: v.0.line,
                        column: v.0.column,
                    });
                }

                "EcNiPoint" => {
                    ret.push(Witness {
                        name: k.to_string(),
                        typ: VarType::EcNiPoint,
                        line: v.0.line,
                        column: v.0.column,
                    });
                }

                "Base" => {
                    ret.push(Witness {
                        name: k.to_string(),
                        typ: VarType::Base,
                        line: v.0.line,
                        column: v.0.column,
                    });
                }

                "Scalar" => {
                    ret.push(Witness {
                        name: k.to_string(),
                        typ: VarType::Scalar,
                        line: v.0.line,
                        column: v.0.column,
                    });
                }

                "MerklePath" => {
                    ret.push(Witness {
                        name: k.to_string(),
                        typ: VarType::MerklePath,
                        line: v.0.line,
                        column: v.0.column,
                    });
                }

                "Uint32" => {
                    ret.push(Witness {
                        name: k.to_string(),
                        typ: VarType::Uint32,
                        line: v.0.line,
                        column: v.0.column,
                    });
                }

                "Uint64" => {
                    ret.push(Witness {
                        name: k.to_string(),
                        typ: VarType::Uint64,
                        line: v.0.line,
                        column: v.0.column,
                    });
                }

                x => {
                    self.error.abort(
                        &format!("`{}` is an unsupported witness type.", x),
                        v.1.line,
                        v.1.column,
                    );
                }
            }
        }

        ret
    }

    fn parse_ast_circuit(&self, statements: Vec<Vec<Token>>) -> Vec<Statement> {
        // The statement layouts/syntax in the language are as follows:
        //
        // C = poseidon_hash(pub_x, pub_y, value, token, serial, coin_blind);
        // | |          |                   |       |
        // V V          V                   V       V
        // variable    opcode              arg     arg
        // assign
        //
        //                    constrain_instance(C);
        //                       |               |
        //                       V               V
        //                     opcode           arg
        //
        //                                              inner opcode arg
        //                                               |
        //                  constrain_instance(ec_get_x(foo));
        //                        |                 |
        //                        V                 V
        //                     opcode          arg as opcode
        //
        // In the latter, we want to support nested function calls, e.g.:
        //
        //          constrain_instance(ec_get_x(token_commit));
        //
        // The inner call's result would still get pushed on the heap,
        // but it will not be accessible in any other scope.
        //
        // In certain opcodes, we also support literal types, and the
        // opcodes can return a variable type after running the operation.
        // e.g.
        //            one = witness_base(1);
        //           zero = witness_base(0);
        //
        // The literal type is used only in the function call's scope, but
        // the result is then accessible on the heap to be used by further
        // computation.
        //
        // Regarding multiple return values from opcodes, this is perhaps
        // not necessary for the current language scope, as this is a low
        // level representation. Note that it could be relatively easy to
        // modify the parsing logic to support that here. For now we'll
        // defer it, and if at some point we decide that the language is
        // too expressive and noisy, we'll consider having multiple return
        // types. It also very much depends on the type of functions/opcodes
        // that we want to support.

        // Vec of statements to return from this entire parsing operation.
        let mut ret = vec![];

        // Here, our statements tokens have been parsed and delimited by
        // semicolons (;) in the source file. This iterator contains each
        // of those statements as an array of tokens we then consume and
        // build the AST further.
        for statement in statements {
            if statement.is_empty() {
                continue
            }

            let (mut left_paren, mut right_paren) = (0, 0);
            for i in &statement {
                match i.token.as_str() {
                    "(" => left_paren += 1,
                    ")" => right_paren += 1,
                    _ => {}
                }
            }

            if left_paren != right_paren || (left_paren == 0 || right_paren == 0) {
                self.error.abort(
                    "Incorrect number of left and right parenthesis for statement.",
                    statement[0].line,
                    statement[0].column,
                );
            }

            // Peekable iterator so we can see tokens in advance
            // without consuming the iterator.
            let mut iter = statement.iter().peekable();

            // Dummy statement that we'll hopefully fill now.
            let mut stmt = Statement::default();

            let mut parsing = false;
            while let Some(token) = iter.next() {
                if !parsing {
                    // TODO: MAKE SURE IT'S A SYMBOL

                    // This logic must be changed if we want to support
                    // multiple return values.
                    if let Some(next_token) = iter.peek() {
                        if next_token.token_type == TokenType::Assign {
                            stmt.line = token.line;
                            stmt.typ = StatementType::Assign;
                            stmt.rhs = vec![];
                            stmt.lhs = Some(Variable {
                                name: token.token.clone(),
                                typ: VarType::Dummy,
                                line: token.line,
                                column: token.column,
                            });

                            // Skip over the `=` token.
                            iter.next();
                            parsing = true;
                            continue
                        }

                        if next_token.token_type == TokenType::LeftParen {
                            stmt.line = token.line;
                            stmt.typ = StatementType::Call;
                            stmt.rhs = vec![];
                            stmt.lhs = None;
                            parsing = true;
                        }

                        if !parsing {
                            self.error.abort(
                                &format!("Illegal token `{}`.", next_token.token),
                                next_token.line,
                                next_token.column,
                            );
                        }
                    }
                }

                // If parsing == true, we now know if we're making a variable
                // assignment or a function call without a return value.
                // Let's dig deeper to see what the statement's call is, and
                // what it contains as arguments. With this we'll fill `rhs`.
                // The arguments could be literal types, other variables, or
                // even nested function calls.
                // For now, we don't care if the params are valid, as this is
                // the job of the semantic analyzer which comes after the
                // parsing module.

                // The assumption here is that the current token is a function
                // call, so we check if it's legit and start digging.
                let func_name = token.token.as_str();

                // TODO: MAKE SURE IT'S A SYMBOL
                if let Some(op) = Opcode::from_name(func_name) {
                    let rhs = self.parse_function_call(token, &mut iter);
                    stmt.opcode = op;
                    stmt.rhs = rhs;
                } else {
                    self.error.abort(
                        &format!("Unimplemented opcode `{}`.", func_name),
                        token.line,
                        token.column,
                    );
                }

                ret.push(stmt);
                stmt = Statement::default();
            }
        }

        ret
    }

    fn parse_function_call(
        &self,
        token: &Token,
        iter: &mut Peekable<std::slice::Iter<'_, Token>>,
    ) -> Vec<Arg> {
        if let Some(next_token) = iter.peek() {
            if next_token.token_type != TokenType::LeftParen {
                self.error.abort(
                    "Invalid function call opening. Must start with a '('.",
                    next_token.line,
                    next_token.column,
                );
            }
            // Skip the opening parenthesis
            iter.next();
        } else {
            self.error.abort("Premature ending of statement.", token.line, token.column);
        }

        let mut ret = vec![];

        // The next element in the iter now hopefully contains an opcode
        // argument. If it's another opcode, we'll recurse into this
        // function's logic.
        // Otherwise, we look for variable and literal types.
        while let Some(arg) = iter.next() {
            // ============================
            // Parse a nested function call
            // ============================
            if let Some(op_inner) = Opcode::from_name(&arg.token) {
                if let Some(paren) = iter.peek() {
                    if paren.token_type != TokenType::LeftParen {
                        self.error.abort(
                            "Invalid function call opening. Must start with a '('.",
                            paren.line,
                            paren.column,
                        );
                    }

                    // Recurse this function to get the params of the nested one.
                    let args = self.parse_function_call(arg, iter);

                    // Then we assign a "fake" variable that serves as a heap
                    // reference.
                    let var = Variable {
                        name: format!("_op_inner_{}_{}", arg.line, arg.column),
                        typ: VarType::Dummy,
                        line: arg.line,
                        column: arg.column,
                    };

                    let arg = Arg::Func(Statement {
                        typ: StatementType::Assign,
                        opcode: op_inner,
                        lhs: Some(var),
                        rhs: args,
                        line: arg.line,
                    });

                    ret.push(arg);
                    continue
                }

                self.error.abort(
                    "Missing tokens in statement, there's a syntax error here.",
                    arg.line,
                    arg.column,
                );
            }

            // ==========================================
            // Parse normal argument, not a function call
            // ==========================================
            if let Some(sep) = iter.next() {
                // See if we have a variable or a literal type.
                match arg.token_type {
                    TokenType::Symbol => ret.push(Arg::Var(Variable {
                        name: arg.token.clone(),
                        typ: VarType::Dummy,
                        line: arg.line,
                        column: arg.column,
                    })),

                    TokenType::Number => {
                        // Check if we can actually convert this into a number.
                        match arg.token.parse::<u64>() {
                            Ok(_) => {}
                            Err(e) => {
                                self.error.abort(
                                    &format!("Failed to convert literal into u64: {}", e),
                                    arg.line,
                                    arg.column,
                                );
                            }
                        };

                        ret.push(Arg::Lit(Literal {
                            name: arg.token.clone(),
                            typ: LitType::Uint64,
                            line: arg.line,
                            column: arg.column,
                        }))
                    }

                    TokenType::RightParen => {
                        if let Some(comma) = iter.peek() {
                            if comma.token_type == TokenType::Comma {
                                iter.next();
                            }
                        }
                        break
                    }

                    x => unimplemented!("{:#?}", x),
                };

                if sep.token_type == TokenType::RightParen {
                    if let Some(comma) = iter.peek() {
                        if comma.token_type == TokenType::Comma {
                            iter.next();
                        }
                    }
                    // Reached end of args
                    break
                }

                if sep.token_type != TokenType::Comma {
                    self.error.abort(
                        "Argument separator is not a comma (`,`)",
                        sep.line,
                        sep.column,
                    );
                }
            }
        }

        ret
    }
}
