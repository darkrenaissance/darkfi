use std::str::Chars;

use super::error::ErrorEmitter;

const SPECIAL_CHARS: [char; 7] = ['{', '}', '(', ')', ',', ';', '='];

fn is_letter(ch: char) -> bool {
    ('a'..='z').contains(&ch) || ('A'..='Z').contains(&ch) || ch == '_'
}

fn is_digit(ch: char) -> bool {
    ('0'..='9').contains(&ch)
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TokenType {
    Symbol,
    String,
    Number,
    LeftBrace,
    RightBrace,
    LeftParen,
    RightParen,
    Comma,
    Semicolon,
    Assign,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub token: String,
    pub token_type: TokenType,
    pub line: usize,
    pub column: usize,
}

impl Token {
    fn new(token: &str, token_type: TokenType, line: usize, column: usize) -> Self {
        Self { token: token.to_string(), token_type, line, column }
    }
}

pub struct Lexer<'a> {
    source: Chars<'a>,
    error: ErrorEmitter,
}

impl<'a> Lexer<'a> {
    pub fn new(filename: &str, source: Chars<'a>) -> Self {
        // For nice error reporting, we'll load everything into a string
        // vector so we have references to lines.
        let lines: Vec<String> = source.as_str().lines().map(|x| x.to_string()).collect();
        let error = ErrorEmitter::new("Lexer", filename, lines);

        Self { source, error }
    }

    pub fn lex(&self) -> Vec<Token> {
        let mut tokens = vec![];
        let mut lineno = 1;
        let mut column = 0;

        // We use this as a buffer to store a single token, which is then
        // reset after a token is pushed to the returning vec.
        let mut buf = String::new();

        // We use these to keep state when iterating.
        let mut in_comment = false;
        let mut in_string = false;
        let mut in_number = false;
        let mut in_symbol = false;

        macro_rules! new_symbol {
            () => {
                tokens.push(Token::new(&buf, TokenType::Symbol, lineno, column - buf.len()));
                in_symbol = false;
                buf = String::new();
            };
        }
        macro_rules! new_string {
            () => {
                tokens.push(Token::new(&buf, TokenType::String, lineno, column - buf.len()));
                in_string = false;
                buf = String::new();
            };
        }
        macro_rules! new_number {
            () => {
                tokens.push(Token::new(&buf, TokenType::Number, lineno, column - buf.len()));
                in_number = false;
                buf = String::new();
            };
        }

        #[allow(clippy::explicit_counter_loop)]
        for c in self.source.clone() {
            column += 1;

            if c == '\n' {
                if in_symbol {
                    new_symbol!();
                }

                if in_string {
                    self.error.abort("Strings can't contain newlines", lineno, column);
                }

                if in_number {
                    self.error.abort("Numbers can't contain newlines", lineno, column);
                }

                in_comment = false;
                lineno += 1;
                column = 0;
                continue
            }

            if c == '#' || in_comment {
                if in_symbol {
                    new_symbol!();
                }

                if in_number {
                    new_number!();
                }

                if in_string {
                    buf.push(c);
                    continue
                }

                in_comment = true;
                continue
            }

            if c.is_whitespace() {
                if in_symbol {
                    new_symbol!();
                }

                if in_number {
                    new_number!();
                }

                if in_string {
                    // TODO: Perhaps forbid whitespace.
                    buf.push(c);
                }

                continue
            }

            // Main cases, in_comment is already checked above.
            if !in_number && !in_symbol && !in_string && is_digit(c) {
                in_number = true;
                buf.push(c);
                continue
            }

            if in_number && !is_digit(c) {
                new_number!();
            }

            if in_number && is_digit(c) {
                buf.push(c);
                continue
            }

            if !in_number && !in_symbol && !in_string && is_letter(c) {
                in_symbol = true;
                buf.push(c);
                continue
            }

            if !in_number && !in_symbol && !in_string && c == '"' {
                // " I need to fix my Rust vis lexer
                in_string = true;
                continue
            }

            if (in_symbol || in_string) && (is_letter(c) || is_digit(c)) {
                buf.push(c);
                continue
            }

            if in_string && c == '"' {
                // " I need to fix my vis lexer
                if buf.is_empty() {
                    self.error.abort("String cannot be empty", lineno, column);
                }
                new_string!();
                continue
            }

            if SPECIAL_CHARS.contains(&c) {
                if in_symbol {
                    new_symbol!();
                }

                if in_number {
                    new_number!();
                }

                if in_string {
                    // TODO: Perhaps forbid these chars inside strings.
                }

                match c {
                    '{' => {
                        tokens.push(Token::new("{", TokenType::LeftBrace, lineno, column));
                        continue
                    }
                    '}' => {
                        tokens.push(Token::new("}", TokenType::RightBrace, lineno, column));
                        continue
                    }
                    '(' => {
                        tokens.push(Token::new("(", TokenType::LeftParen, lineno, column));
                        continue
                    }
                    ')' => {
                        tokens.push(Token::new(")", TokenType::RightParen, lineno, column));
                        continue
                    }
                    ',' => {
                        tokens.push(Token::new(",", TokenType::Comma, lineno, column));
                        continue
                    }
                    ';' => {
                        tokens.push(Token::new(";", TokenType::Semicolon, lineno, column));
                        continue
                    }
                    '=' => {
                        tokens.push(Token::new("=", TokenType::Assign, lineno, column));
                        continue
                    }
                    _ => self.error.abort(&format!("Invalid token `{}`", c), lineno, column - 1),
                }
                continue
            }

            self.error.abort(&format!("Invalid token `{}`", c), lineno, column - 1);
        }

        tokens
    }
}
