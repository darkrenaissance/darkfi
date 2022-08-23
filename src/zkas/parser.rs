use std::str::Chars;

use indexmap::IndexMap;
use itertools::Itertools;

use super::{
    ast::{Constant, Statement, Witness},
    error::ErrorEmitter,
    lexer::{Token, TokenType},
    LitType, VarType,
};

/// zkas language builtin keywords.
/// These can not be used anywhere except where they are expected.
const KEYWORDS: [&str; 3] = ["constant", "contract", "circuit"];

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

    pub fn parse(&self) -> (Vec<Constant>, Vec<Witness>, Vec<Statement>) {
        // We use these to keep state while parsing.
        let mut namespace = None;
        let (mut declaring_constant, mut declared_constant) = (false, false);
        let (mut declaring_contract, mut declared_contract) = (false, false);
        let (mut declaring_circuit, mut declared_circuit) = (false, false);

        // The tokens gathered from each of the sections
        let mut constant_tokens = vec![];
        let mut contract_tokens = vec![];
        let mut circuit_tokens = vec![];

        let mut circuit_stmt = vec![];
        let mut circuit_stmts = vec![];
        let mut ast_inner = IndexMap::new();
        let mut ast = IndexMap::new();

        if self.tokens[0].token_type != TokenType::Symbol {
            self.error.abort(
                "Source file does not start with a section. Expected `constant/contract/circuit`.",
                0,
                0,
            );
        }

        let mut iter = self.tokens.iter();
        while let Some(t) = iter.next() {
            // Sections "constant", "contract", and "circuit" are
            // the sections we must be declaring in our source code.
            // When we find one, we'll take all the tokens found in
            // the section and place them in their respective vec.
            // NOTE: Currently this logic depends on the fact that
            // the sections are closed off with braces. This should
            // be revisited later when we decide to add other lang
            // functionality that also depends on using braces.
            if !declaring_constant && !declaring_contract && !declaring_circuit {
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
                    "contract" => {
                        declaring_contract = true;
                        absorb_inner_tokens!(contract_tokens);
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

                if let Some(_) = constant_inner.next() {
                    self.error.abort("Internal error, leftovers in 'constant' iterator", 0, 0);
                }

                ast_inner.insert("constant".to_string(), constants_map);
                declaring_constant = false;
                declared_constant = true;
            }

            // Parse the contract section into the AST.
            if declaring_contract {
                if declared_contract {
                    self.error.abort("Duplicate `contract` section found.", t.line, t.column);
                }

                self.check_section_structure("contract", contract_tokens.clone());
                check_namespace!(contract_tokens);

                let mut witnesses_map = IndexMap::new();
                // This is everything between the braces: { ... }
                let mut contract_inner = contract_tokens[2..contract_tokens.len() - 1].iter();
                while let Some((typ, name, comma)) = contract_inner.next_tuple() {
                    if comma.token_type != TokenType::Comma {
                        self.error.abort("Separator is not a comma.", comma.line, comma.column);
                    }

                    // No variable shadowing
                    if witnesses_map.contains_key(name.token.as_str()) {
                        self.error.abort(
                            &format!(
                                "Section `contract` already contains the token `{}`.",
                                &name.token
                            ),
                            name.line,
                            name.column,
                        );
                    }

                    witnesses_map.insert(name.token.clone(), (name.clone(), typ.clone()));
                }

                if let Some(_) = contract_inner.next() {
                    self.error.abort("Internal error, leftovers in 'contract' iterator", 0, 0);
                }

                ast_inner.insert("contract".to_string(), witnesses_map);
                declaring_contract = false;
                declared_contract = true;
            }

            // Parse the circuit section into the AST.
            if declaring_circuit {
                if declared_circuit {
                    self.error.abort("Duplicate `circuit` section found.", t.line, t.column);
                }

                self.check_section_structure("circuit", circuit_tokens.clone());
                check_namespace!(circuit_tokens);

                for i in circuit_tokens[2..circuit_tokens.len() - 1].iter() {
                    if i.token_type == TokenType::Semicolon {
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
            let c = match ast.get(&ns).unwrap().get("contract") {
                Some(c) => c,
                None => {
                    self.error.abort("Missing `contract` section in .zk source.", 0, 0);
                    unreachable!();
                }
            };
            self.parse_ast_contract(c)
        };

        let statements = self.parse_ast_circuit(circuit_stmts);
        if statements.is_empty() {
            self.error.abort("Circuit section is empty.", 0, 0);
        }

        (constants, witnesses, statements)
    }

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
            "constant" | "contract" => {
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
            _ => panic!(),
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

    fn parse_ast_contract(&self, ast: &IndexMap<String, (Token, Token)>) -> Vec<Witness> {
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
        let mut ret = vec![];

        // In here, we want to support nested function calls, e.g.:
        //
        //      constrain_instance(ec_get_x(token_commit));
        //
        // The inner call's result would still get pushed on the stack,
        // but it will not be accessible in any other scope.

        // In certain opcodes, we also support literal types, and the
        // opcodes can return a variable type after running the operation.
        // e.g.
        //            one = witness_base(1);
        //           zero = witness_base(0);
        //
        // The literal type is used only in the function call's scope, but
        // the result is then accessible on the stack to be used by further
        // computation.

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

        ret
    }
}
