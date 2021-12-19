use std::str::Chars;

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

    let mut iter = tokens.iter();
    while let Some(t) = iter.next() {
        // Start by declaring a section
        if !declaring_constant && !declaring_contract && !declaring_circuit {
            if t.token_type != TokenType::Symbol {
                println!("FOO");
            }
        }
    }
}
