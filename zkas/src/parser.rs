use std::str::Chars;

use itertools::Itertools;

use crate::{
    error::ParserError,
    lexer::{Token, TokenType},
};

pub fn parse(filename: &str, source: Chars, tokens: Vec<Token>) {
    // For nice error reporting, we'll load everything into a string vector
    // so we have references to lines.
    let lines: Vec<String> = source.as_str().lines().map(|x| x.to_string()).collect();
    let parser_error = ParserError::new(filename, lines);

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

            // The sections we are declaring in our source code
            match t.token.as_str() {
                "constant" => {
                    declaring_constant = true;
                    for inner in iter.by_ref() {
                        constant_tokens.push(inner);
                        if inner.token_type == TokenType::RightBrace {
                            break
                        }
                    }
                }

                "contract" => {
                    declaring_contract = true;
                    for inner in iter.by_ref() {
                        contract_tokens.push(inner);
                        if inner.token_type == TokenType::RightBrace {
                            break
                        }
                    }
                }

                "circuit" => {
                    declaring_circuit = true;
                    for inner in iter.by_ref() {
                        circuit_tokens.push(inner);
                        if inner.token_type == TokenType::RightBrace {
                            break
                        }
                    }
                }

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
        if declaring_constant {
            if let Some(err_msg) = check_section_structure(constant_tokens.clone()) {
                parser_error.invalid_section_declaration(
                    "constant",
                    err_msg,
                    constant_tokens[0].line,
                    constant_tokens[0].column,
                );
            }

            let mut constants = vec![];

            let mut constants_inner = constant_tokens[2..constant_tokens.len() - 1].iter();
            while let Some((typ, name, comma)) = constants_inner.next_tuple() {
                if comma.token_type != TokenType::Comma {
                    parser_error.separator_not_a_comma(comma.line, comma.column);
                }
                constants.push((typ, name));
            }

            declaring_constant = false;
        }

        if declaring_contract {
            if let Some(err_msg) = check_section_structure(contract_tokens.clone()) {
                parser_error.invalid_section_declaration(
                    "contract",
                    err_msg,
                    contract_tokens[0].line,
                    contract_tokens[0].column,
                );
            }

            let mut contract = vec![];

            let mut contract_inner = contract_tokens[2..contract_tokens.len() - 1].iter();
            while let Some((typ, name, comma)) = contract_inner.next_tuple() {
                if comma.token_type != TokenType::Comma {
                    parser_error.separator_not_a_comma(comma.line, comma.column);
                }
                contract.push((typ, name));
            }

            declaring_contract = false;
        }

        if declaring_circuit {
            declaring_circuit = false;
        }
    }
}

fn check_section_structure(tokens: Vec<&Token>) -> Option<&str> {
    if tokens[0].token_type != TokenType::String {
        return Some("Section declaration must start with a naming string.")
    }
    if tokens[1].token_type != TokenType::LeftBrace {
        return Some("Section opening is not correct. Must be opened with a left brace `{`")
    }
    if tokens[tokens.len() - 1].token_type != TokenType::RightBrace {
        return Some("Section closing is not correct. Must be closed with a right brace `}`")
    }

    if tokens[2..tokens.len() - 1].len() % 3 != 0 {
        return Some("Invalid number of elements in section. Must be pairs of `type:name` separated with a comma `,`")
    }

    None
}
