use colour::{e_prnt_ln, e_red};

pub struct LexerError {
    file: String,
    lines: Vec<String>,
}

impl LexerError {
    pub fn new(file: &str, lines: Vec<String>) -> Self {
        LexerError { file: file.to_string(), lines }
    }

    pub fn invalid_token(&self, t: char, ln: usize, col: usize) {
        let err_msg = format!("Invalid token `{}` on line {} (column {})\n", t, ln, col);
        let dbg_msg = format!("{}:{}:{}: {}", self.file, ln, col, self.lines[ln - 1]);
        let pad = dbg_msg.split(": ").next().unwrap().len() + col + 2;
        let caret = format!("{:width$}^", "", width = pad);
        let msg = format!("{}\n{}\n{}", err_msg, dbg_msg, caret);
        LexerError::lexer_error(&msg);
    }

    pub fn invalid_string(&self, s: &str, ln: usize, col: usize) {
        let err_msg = format!("Invalid ending in string `{}` on line {} (column {})", s, ln, col);
        let dbg_msg = format!("{}:{}:{}: {}", self.file, ln, col, self.lines[ln - 1]);
        let pad = dbg_msg.split(": ").next().unwrap().len() + col + 2;
        let caret = format!("{:width$}^", "", width = pad);
        let msg = format!("{}\n{}\n{}", err_msg, dbg_msg, caret);
        LexerError::lexer_error(&msg);
    }

    pub fn invalid_symbol(&self, s: &str, ln: usize, col: usize) {
        let err_msg = format!("Illegal char `{}` for symbol on line {} (column {})", s, ln, col);
        let dbg_msg = format!("{}:{}:{}: {}", self.file, ln, col, self.lines[ln - 1]);
        let pad = dbg_msg.split(": ").next().unwrap().len() + col + 2;
        let caret = format!("{:width$}^", "", width = pad);
        let msg = format!("{}\n{}\n{}", err_msg, dbg_msg, caret);
        LexerError::lexer_error(&msg);
    }

    fn lexer_error(msg: &str) {
        e_red!("Lexer error: ");
        e_prnt_ln!("{}", msg);
        std::process::exit(1);
    }
}

pub struct ParserError {
    file: String,
    lines: Vec<String>,
}

impl ParserError {
    pub fn new(file: &str, lines: Vec<String>) -> Self {
        ParserError { file: file.to_string(), lines }
    }

    pub fn invalid_section_declaration(&self, s: &str, m: &str, ln: usize, col: usize) {
        let err_msg =
            format!("Invalid `{}` section declaration on line {} (column {})", s, ln, col);
        let err_msg = format!("{}\n{}", err_msg, m);
        let dbg_msg = format!("{}:{}:{}: {}", self.file, ln, col, self.lines[ln - 1]);
        let pad = dbg_msg.split(": ").next().unwrap().len() + col + 2;
        let caret = format!("{:width$}^", "", width = pad);
        let msg = format!("{}\n{}\n{}", err_msg, dbg_msg, caret);
        ParserError::parser_error(&msg);
    }

    fn parser_error(msg: &str) {
        e_red!("Parser error: ");
        e_prnt_ln!("{}", msg);
        std::process::exit(1);
    }
}
