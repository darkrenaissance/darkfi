use std::str::Chars;

use super::error::ErrorEmitter;

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
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

const SPECIAL_CHARS: [char; 7] = ['{', '}', '(', ')', ',', ';', '='];

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct Token {
    pub token: String,
    pub token_type: TokenType,
    pub line: usize,
    pub column: usize,
}

impl Token {
    fn new(token: &str, token_type: TokenType, line: usize, column: usize) -> Self {
        Token { token: token.to_string(), token_type, line, column }
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

    pub fn lex(self) -> Vec<Token> {
        let mut tokens = vec![];
        let mut lineno = 1;
        let mut column = 0;

        // We use these as a buffer to keep strings and symbols
        let mut strbuf = String::new();
        let mut symbuf = String::new();
        let mut numbuf = String::new();

        // We use these to keep state when iterating
        let mut in_comment = false;
        let mut in_string = false;
        let mut in_number = false;
        let mut in_symbol = false;

        #[allow(clippy::explicit_counter_loop)]
        for c in self.source.clone() {
            column += 1;

            if c == '\n' {
                if in_symbol {
                    in_symbol = false;
                    tokens.push(Token::new(
                        &symbuf,
                        TokenType::Symbol,
                        lineno,
                        column - symbuf.len(),
                    ));
                    symbuf = String::new();
                }

                if in_string {
                    self.error.abort(
                        &format!("Strings can not contain newlines: `{}`", &strbuf),
                        lineno,
                        column,
                    );
                }

                if in_number {
                    self.error.abort(
                        &format!("Numbers can not contain newlines: `{}`", &numbuf),
                        lineno,
                        column,
                    );
                }

                in_comment = false;
                lineno += 1;
                column = 0;
                continue
            }

            if c == '#' || in_comment {
                if in_symbol {
                    in_symbol = false;
                    tokens.push(Token::new(
                        &symbuf,
                        TokenType::Symbol,
                        lineno,
                        column - symbuf.len(),
                    ));
                    symbuf = String::new();
                }

                if in_string {
                    strbuf.push(c);
                    continue
                }

                in_comment = true;
                continue
            }

            if c.is_whitespace() {
                if in_symbol {
                    in_symbol = false;
                    tokens.push(Token::new(
                        &symbuf,
                        TokenType::Symbol,
                        lineno,
                        column - symbuf.len(),
                    ));
                    symbuf = String::new();
                }

                if in_number {
                    in_number = false;
                    tokens.push(Token::new(
                        &numbuf,
                        TokenType::Number,
                        lineno,
                        column - numbuf.len(),
                    ));
                    numbuf = String::new();
                }

                continue
            }

            if in_number && !is_digit(c) {
                in_number = false;
                tokens.push(Token::new(&numbuf, TokenType::Number, lineno, column - numbuf.len()));
                numbuf = String::new();
            }

            if !in_string && is_letter(c) {
                in_symbol = true;
                symbuf.push(c);
                continue
            }

            if in_string && (is_letter(c) || is_digit(c)) {
                strbuf.push(c);
                continue
            }

            if in_number && is_digit(c) {
                numbuf.push(c);
                continue
            }

            if in_symbol && is_digit(c) {
                symbuf.push(c);
                continue
            }

            if !in_symbol && !in_string && !in_number && is_digit(c) {
                in_number = true;
                numbuf.push(c);
                continue
            }

            if c == '"' && !in_string {
                if in_symbol {
                    self.error.abort(&format!("Illegal char `{}` for symbol", c), lineno, column);
                }
                in_string = true;
                continue
            }

            if c == '"' && in_string {
                if strbuf.is_empty() {
                    self.error.abort(
                        &format!("Invalid ending in string `{}`", &strbuf),
                        lineno,
                        column,
                    );
                }

                in_string = false;
                tokens.push(Token::new(&strbuf, TokenType::String, lineno, column - strbuf.len()));
                strbuf = String::new();
                continue
            }

            if SPECIAL_CHARS.contains(&c) {
                if in_symbol {
                    in_symbol = false;
                    tokens.push(Token::new(
                        &symbuf,
                        TokenType::Symbol,
                        lineno,
                        column - symbuf.len(),
                    ));
                    symbuf = String::new();
                }

                if in_number {
                    in_number = false;
                    tokens.push(Token::new(
                        &numbuf,
                        TokenType::Symbol,
                        lineno,
                        column - numbuf.len(),
                    ));
                    numbuf = String::new();
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

fn is_letter(ch: char) -> bool {
    ('a'..='z').contains(&ch) || ('A'..='Z').contains(&ch) || ch == '_'
}

fn is_digit(ch: char) -> bool {
    ('0'..'9').contains(&ch)
}
