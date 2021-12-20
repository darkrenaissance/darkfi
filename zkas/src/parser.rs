use std::str::Chars;

use crate::{
    error::ParserError,
    lexer::{Token, TokenType},
};

pub fn parse(filename: &str, source: Chars, tokens: Vec<Token>) {
    // For nice error reporting, we'll load everything into a string vector
    // so we have references to lines.
    let lines: Vec<String> = source.as_str().lines().map(|x| x.to_string()).collect();
    let _parser_error = ParserError::new(filename, lines);

    // We use these to keep state when iterating
    let mut declaring_constant = false;
    let mut declaring_contract = false;
    let mut declaring_circuit = false;

    let mut constant_tokens = vec![];
    let mut contract_tokens = vec![];
    let mut circuit_tokens = vec![];

    let mut iter = tokens.iter();
    while let Some(t) = iter.next() {
        // Start by declaring a section
        if !declaring_constant && !declaring_contract && !declaring_circuit {
            if t.token_type != TokenType::Symbol {
                // TODO: Revisit
                // TODO: Visit this again when we are allowing imports
                panic!();
            }

            match t.token.as_str() {
                "constant" => {
                    declaring_constant = true;
                    //while let Some(inner) = iter.next() {
                    for inner in iter.by_ref() {
                        constant_tokens.push(inner);
                        if inner.token_type == TokenType::RightBrace {
                            break
                        }
                    }
                }

                "contract" => {
                    declaring_contract = true;
                    //while let Some(inner) = iter.next() {
                    for inner in iter.by_ref() {
                        contract_tokens.push(inner);
                        if inner.token_type == TokenType::RightBrace {
                            break
                        }
                    }
                }

                "circuit" => {
                    declaring_circuit = true;
                    //while let Some(inner) = iter.next() {
                    for inner in iter.by_ref() {
                        circuit_tokens.push(inner);
                        if inner.token_type == TokenType::RightBrace {
                            break
                        }
                    }
                }

                // Fall through
                _ => unreachable!(),
            }
        }

        // We shouldn't be reaching these states
        if declaring_constant && (declaring_contract || declaring_circuit) {
            unreachable!()
        }
        if declaring_contract && (declaring_constant || declaring_circuit) {
            unreachable!()
        }
        if declaring_circuit && (declaring_constant || declaring_contract) {
            unreachable!()
        }

        // Now go through the token vectors and work it through
    }
}
