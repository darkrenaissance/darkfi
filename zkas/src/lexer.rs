use std::{io, io::Write, process, str::Chars};

use termion::{color, style};

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub enum TokenType {
    Symbol,
    String,
    LeftBrace,
    RightBrace,
    LeftParen,
    RightParen,
    Comma,
    Semicolon,
    Colon,
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
    fn new(token: String, token_type: TokenType, line: usize, column: usize) -> Self {
        Token { token, token_type, line, column }
    }
}

pub struct Lexer<'a> {
    file: String,
    lines: Vec<String>,
    source: Chars<'a>,
}

impl<'a> Lexer<'a> {
    pub fn new(filename: &str, source: Chars<'a>) -> Self {
        // For nice error reporting, we'll load everything into a string
        // vector so we have references to lines.
        let lines: Vec<String> = source.as_str().lines().map(|x| x.to_string()).collect();
        Lexer { file: filename.to_string(), lines, source }
    }

    pub fn lex(self) -> Vec<Token> {
        let mut tokens = vec![];
        let mut lineno = 1;
        let mut column = 0;

        // We use these as a buffer to keep strings and symbols
        let mut strbuf = String::new();
        let mut symbuf = String::new();

        // We use these to keep state when iterating
        let mut in_comment = false;
        let mut in_string = false;
        let mut in_symbol = false;

        #[allow(clippy::explicit_counter_loop)]
        for c in self.source.clone() {
            column += 1;

            if c == '\n' {
                if in_symbol {
                    in_symbol = false;
                    tokens.push(Token::new(
                        symbuf.clone(),
                        TokenType::Symbol,
                        lineno,
                        column - symbuf.len(),
                    ));
                    symbuf = String::new();
                }

                if in_string {
                    // TODO: Allow newlines in strings?
                    self.error(format!("Invalid ending in string `{}`", &strbuf), lineno, column);
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
                        symbuf.clone(),
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
                        symbuf.clone(),
                        TokenType::Symbol,
                        lineno,
                        column - symbuf.len(),
                    ));
                    symbuf = String::new();
                }

                continue
            }

            if !in_string && is_letter(c) {
                in_symbol = true;
                symbuf.push(c);
                continue
            }

            if in_string && is_letter(c) {
                strbuf.push(c);
                continue
            }

            if c == '"' && !in_string {
                if in_symbol {
                    self.error(format!("Illegal char `{}` for symbol", c), lineno, column);
                }
                in_string = true;
                continue
            }

            if c == '"' && in_string {
                if strbuf.is_empty() {
                    self.error(format!("Invalid ending in string `{}`", &strbuf), lineno, column);
                }

                in_string = false;
                tokens.push(Token::new(
                    strbuf.clone(),
                    TokenType::String,
                    lineno,
                    column - strbuf.len(),
                ));
                strbuf = String::new();
                continue
            }

            if SPECIAL_CHARS.contains(&c) {
                if in_symbol {
                    in_symbol = false;
                    tokens.push(Token::new(
                        symbuf.clone(),
                        TokenType::Symbol,
                        lineno,
                        column - symbuf.len(),
                    ));
                    symbuf = String::new();
                }

                match c {
                    '{' => {
                        tokens.push(Token::new(
                            "{".to_string(),
                            TokenType::LeftBrace,
                            lineno,
                            column,
                        ));
                        continue
                    }
                    '}' => {
                        tokens.push(Token::new(
                            "}".to_string(),
                            TokenType::RightBrace,
                            lineno,
                            column,
                        ));
                        continue
                    }
                    '(' => {
                        tokens.push(Token::new(
                            "(".to_string(),
                            TokenType::LeftParen,
                            lineno,
                            column,
                        ));
                        continue
                    }
                    ')' => {
                        tokens.push(Token::new(
                            ")".to_string(),
                            TokenType::RightParen,
                            lineno,
                            column,
                        ));
                        continue
                    }
                    ',' => {
                        tokens.push(Token::new(",".to_string(), TokenType::Comma, lineno, column));
                        continue
                    }
                    ';' => {
                        tokens.push(Token::new(
                            ";".to_string(),
                            TokenType::Semicolon,
                            lineno,
                            column,
                        ));
                        continue
                    }
                    '=' => {
                        tokens.push(Token::new("=".to_string(), TokenType::Assign, lineno, column));
                        continue
                    }
                    _ => self.error(format!("Invalid token `{}`", c), lineno, column - 1),
                }
                continue
            }

            self.error(format!("Invalid token `{}`", c), lineno, column - 1);
        }

        tokens
    }

    fn error(&self, msg: String, ln: usize, col: usize) {
        let err_msg = format!("{} (line {}, column {})", msg, ln, col);
        let dbg_msg = format!("{}:{}:{}: {}", self.file, ln, col, self.lines[ln - 1]);
        let pad = dbg_msg.split(": ").next().unwrap().len() + col + 2;
        let caret = format!("{:width$}^", "", width = pad);
        let msg = format!("{}\n{}\n{}\n", err_msg, dbg_msg, caret);
        Lexer::abort(&msg);
    }

    fn abort(msg: &str) {
        let stderr = io::stderr();
        let mut handle = stderr.lock();
        write!(
            handle,
            "{}{}Lexer error:{} {}",
            style::Bold,
            color::Fg(color::Red),
            style::Reset,
            msg,
        )
        .unwrap();
        handle.flush().unwrap();
        process::exit(1);
    }
}

fn is_letter(ch: char) -> bool {
    ('a'..='z').contains(&ch) || ('A'..='Z').contains(&ch) || ch == '_'
}

/*
fn is_digit(ch: char) -> bool {
    ('0'..'9').contains(&ch)
}
*/
