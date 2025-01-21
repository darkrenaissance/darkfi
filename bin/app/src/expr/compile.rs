/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use crate::{
    error::{Error, Result},
    //prop::{Property, PropertySubType, PropertyType, PropertySExprValue},
};
use std::collections::HashMap;

use super::{Op, SExprCode};

#[derive(Debug, Clone)]
enum Token {
    LoadVar(String),
    Add,
    Sub,
    Mul,
    Div,
    LeftParen,
    RightParen,
    ConstFloat32(f32),
    NestedExpr(Box<Vec<Token>>),
    SubExpr(Box<Vec<Token>>),
    If,
    Else,
    LeftBrace,
    RightBrace,
    LessThan,
    IfElse((Box<Vec<Token>>, Box<Vec<Token>>, Box<Vec<Token>>)),
    LessThanCompare((Box<Vec<Token>>, Box<Vec<Token>>)),
    Equals,
    SetValue((String, Box<Vec<Token>>)),
}

impl Token {
    fn is_sub_expr(&self) -> bool {
        match self {
            Self::SubExpr(_) => true,
            _ => false,
        }
    }

    fn flatten(self) -> Vec<Self> {
        match self {
            Self::NestedExpr(tokens) => {
                let tokens = Box::into_inner(tokens);
                tokens.into_iter().map(|t| t.flatten()).flatten().collect()
            }
            _ => vec![self],
        }
    }
}

pub struct Compiler {
    table: HashMap<String, Token>,
}

impl Compiler {
    pub fn new() -> Self {
        Self { table: HashMap::new() }
    }

    pub fn add_const_f32<S: Into<String>>(&mut self, name: S, val: f32) {
        self.table.insert(name.into(), Token::ConstFloat32(val));
    }
    /*
    pub fn get_const_f32<S: AsRef<str>>(&self, name: S) -> Option<f32> {
        let name = name.as_ref();
        let val = self.table.get(name)?;
        match val {
            Token::ConstFloat32(v) => Some(v)
            _ => None
        }
    }
    */

    pub fn compile<S: AsRef<str>>(&self, prestmts: S) -> Result<SExprCode> {
        let prestmts = prestmts.as_ref();
        // Strip all comments
        let mut stmts = String::new();
        for line in prestmts.lines() {
            if let Some(chr) = line.trim_start().chars().next() {
                if chr != '#' {
                    stmts.push_str(line);
                }
            }
        }

        let mut code = vec![];
        for stmt in stmts.split(';') {
            code.push(self.compile_line(stmt)?);
        }
        Ok(code)
    }

    fn compile_line(&self, stmt: &str) -> Result<Op> {
        let tokens = self.tokenize(&stmt);
        //println!("{tokens:#?}");
        let tokens = to_rpn(tokens)?;
        //println!("{tokens:#?}");
        Ok(convert(&mut tokens.into_iter())?)
    }

    fn tokenize(&self, stmt: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut current_token = String::new();
        for chr in stmt.chars() {
            match chr {
                ' ' | '\t' | '\n' => {
                    self.clear_accum(&mut current_token, &mut tokens);
                }
                '+' => {
                    self.clear_accum(&mut current_token, &mut tokens);
                    tokens.push(Token::Add);
                }
                '-' => {
                    self.clear_accum(&mut current_token, &mut tokens);
                    tokens.push(Token::Sub);
                }
                '*' => {
                    self.clear_accum(&mut current_token, &mut tokens);
                    tokens.push(Token::Mul);
                }
                '/' => {
                    self.clear_accum(&mut current_token, &mut tokens);
                    tokens.push(Token::Div);
                }
                '(' => {
                    self.clear_accum(&mut current_token, &mut tokens);
                    tokens.push(Token::LeftParen);
                }
                ')' => {
                    self.clear_accum(&mut current_token, &mut tokens);
                    tokens.push(Token::RightParen);
                }
                '{' => {
                    self.clear_accum(&mut current_token, &mut tokens);
                    tokens.push(Token::LeftBrace);
                }
                '}' => {
                    self.clear_accum(&mut current_token, &mut tokens);
                    tokens.push(Token::RightBrace);
                }
                '<' => {
                    self.clear_accum(&mut current_token, &mut tokens);
                    tokens.push(Token::LessThan);
                }
                '=' => {
                    self.clear_accum(&mut current_token, &mut tokens);
                    tokens.push(Token::Equals);
                }
                _ => current_token.push(chr),
            }
        }
        self.clear_accum(&mut current_token, &mut tokens);
        tokens
    }

    fn clear_accum(&self, current_token: &mut String, tokens: &mut Vec<Token>) {
        let prev_token = std::mem::replace(current_token, String::new());
        if prev_token.is_empty() {
            return
        }

        if let Some(token) = self.table.get(&prev_token) {
            tokens.push(token.clone());
            return
        }

        match prev_token.as_str() {
            "if" => {
                tokens.push(Token::If);
                return
            }
            "else" => {
                tokens.push(Token::Else);
                return
            }
            _ => {}
        }

        // Number or var?
        match prev_token.parse::<f32>() {
            Ok(v) => tokens.push(Token::ConstFloat32(v)),
            Err(_) => tokens.push(Token::LoadVar(prev_token)),
        }
    }
}

/// Convert from infix to reverse polish notation
fn to_rpn(tokens: Vec<Token>) -> Result<Vec<Token>> {
    //println!("to_rpn = {tokens:#?}");
    let mut out = Vec::new();
    let mut stack = Vec::new();

    // equals
    let mut iter = tokens.into_iter();
    let mut var = String::new();
    let mut lhs = vec![];
    let mut rhs = vec![];
    let mut comparison = 0;
    // 0: none
    // 1: =
    while let Some(token) = iter.next() {
        match token {
            Token::Equals => {
                if comparison != 0 {
                    return Err(Error::UnexpectedToken)
                }
                comparison = 1;
                if lhs.len() != 1 {
                    return Err(Error::UnexpectedToken)
                }
                let lhs = std::mem::take(&mut lhs);
                match lhs.into_iter().next().unwrap() {
                    Token::LoadVar(v) => var = v,
                    _ => return Err(Error::UnexpectedToken),
                }
            }
            token => {
                if comparison == 0 {
                    lhs.push(token);
                } else {
                    rhs.push(token);
                }
            }
        }
    }
    if comparison == 1 {
        let stack = std::mem::take(&mut rhs);
        let rpn = to_rpn(stack)?;
        out = vec![Token::SetValue((var, Box::new(rpn)))];
    } else {
        assert!(rhs.is_empty());
        out = lhs;
    }

    // Parens
    let tokens = std::mem::take(&mut out);
    let mut paren = 0;
    for token in tokens {
        match token {
            Token::LeftParen => {
                // Is this the first opening paren for this subexpr?
                if paren > 0 {
                    stack.push(token);
                }
                paren += 1;
                continue
            }
            Token::RightParen => {
                paren -= 1;

                // Whoops non-matching number of parens!
                if paren < 0 {
                    return Err(Error::UnexpectedToken)
                }

                // Did we finally reach the closing paren for this subexpr?
                if paren == 0 {
                    let stack = std::mem::take(&mut stack);
                    let mut rpn = to_rpn(stack)?;
                    out.push(Token::NestedExpr(Box::new(rpn)));
                } else {
                    stack.push(token);
                }

                continue
            }
            _ => {}
        }
        if paren > 0 {
            stack.push(token);
        } else {
            out.push(token);
        }
    }
    out.append(&mut stack);

    // Braces
    let tokens = std::mem::take(&mut out);
    assert!(stack.is_empty());
    let mut paren = 0;
    for token in tokens {
        match token {
            Token::LeftBrace => {
                // Is this the first opening paren for this subexpr?
                if paren > 0 {
                    stack.push(token);
                }
                paren += 1;
                continue
            }
            Token::RightBrace => {
                paren -= 1;

                // Whoops non-matching number of parens!
                if paren < 0 {
                    return Err(Error::UnexpectedToken)
                }

                // Did we finally reach the closing paren for this subexpr?
                if paren == 0 {
                    let stack = std::mem::take(&mut stack);
                    let mut rpn = to_rpn(stack)?;
                    out.push(Token::SubExpr(Box::new(rpn)));
                } else {
                    stack.push(token);
                }

                continue
            }
            _ => {}
        }
        if paren > 0 {
            stack.push(token);
        } else {
            out.push(token);
        }
    }
    out.append(&mut stack);

    let tokens = std::mem::take(&mut out);
    assert!(stack.is_empty());
    let mut iter = tokens.into_iter();
    'mainloop: while let Some(token) = iter.next() {
        match token {
            Token::If => {
                let mut section = 0;
                let mut cond_expr = vec![];
                let mut if_expr = vec![];
                let mut else_expr = vec![];
                while let Some(token) = iter.next() {
                    match token {
                        Token::SubExpr(tokens) => {
                            if section == 0 {
                                let cexpr = std::mem::take(&mut cond_expr);
                                cond_expr = to_rpn(cexpr)?;

                                if_expr = *tokens;
                            } else if section == 1 {
                                else_expr = *tokens;
                            } else {
                                return Err(Error::UnexpectedToken)
                            }

                            section += 1;
                        }
                        Token::Else => {
                            if section != 1 {
                                return Err(Error::UnexpectedToken)
                            }
                        }
                        token => {
                            if section != 0 {
                                out.push(Token::IfElse((
                                    Box::new(cond_expr),
                                    Box::new(if_expr),
                                    Box::new(else_expr),
                                )));
                                out.push(token);
                                continue 'mainloop
                            }
                            cond_expr.push(token);
                        }
                    }
                }
                // We reached the end
                out.push(Token::IfElse((
                    Box::new(cond_expr),
                    Box::new(if_expr),
                    Box::new(else_expr),
                )));
            }
            token => out.push(token),
        }
    }

    // comparisons <>=
    let tokens = std::mem::take(&mut out);
    let mut iter = tokens.into_iter();
    let mut lhs = vec![];
    let mut rhs = vec![];
    let mut comparison = 0;
    // 0: none
    // 1: <
    while let Some(token) = iter.next() {
        match token {
            Token::LessThan => {
                if comparison != 0 {
                    return Err(Error::UnexpectedToken)
                }
                comparison = 1;
            }
            token => {
                if comparison == 0 {
                    lhs.push(token);
                } else {
                    rhs.push(token);
                }
            }
        }
    }
    if comparison == 1 {
        out = vec![Token::LessThanCompare((Box::new(lhs), Box::new(rhs)))];
    } else {
        assert!(rhs.is_empty());
        out = lhs;
    }

    // */
    let tokens = std::mem::take(&mut out);
    assert!(stack.is_empty());
    let mut is_op = false;
    for token in tokens {
        match token {
            Token::Mul | Token::Div => {
                if is_op {
                    return Err(Error::UnexpectedToken)
                }
                is_op = true;

                let Some(prev_item) = out.pop() else { return Err(Error::UnexpectedToken) };
                stack.push(token);
                stack.push(prev_item);
            }
            _ => {
                if is_op {
                    is_op = false;
                    let mut expr = std::mem::take(&mut stack);
                    expr.push(token);
                    out.push(Token::NestedExpr(Box::new(expr)));

                    continue
                }

                out.push(token)
            }
        }
    }
    //println!("out = {out:#?}");
    //println!("stack = {stack:#?}");
    //assert!(!is_op);
    //assert!(stack.is_empty());
    if is_op || !stack.is_empty() {
        return Err(Error::UnexpectedToken)
    }

    // +-
    let tokens = std::mem::take(&mut out);
    let mut is_op = false;
    for token in tokens {
        match token {
            Token::Add | Token::Sub => {
                if is_op {
                    return Err(Error::UnexpectedToken)
                }
                is_op = true;

                let Some(prev_item) = out.pop() else { return Err(Error::UnexpectedToken) };
                stack.push(token);
                stack.push(prev_item);
            }
            _ => {
                if is_op {
                    is_op = false;
                    let mut expr = std::mem::take(&mut stack);
                    expr.push(token);
                    out.push(Token::NestedExpr(Box::new(expr)));

                    continue
                }

                out.push(token)
            }
        }
    }
    //assert!(!is_op);
    //assert!(stack.is_empty());
    if is_op || !stack.is_empty() {
        return Err(Error::UnexpectedToken)
    }

    // Flatten everything
    let out = out.into_iter().map(|t| t.flatten()).flatten().collect();
    Ok(out)
}

fn convert<I: Iterator<Item = Token>>(iter: &mut I) -> Result<Op> {
    let Some(token) = iter.next() else { return Err(Error::UnexpectedToken) };

    let op = match token {
        Token::ConstFloat32(v) => Op::ConstFloat32(v),
        Token::LoadVar(v) => Op::LoadVar(v),
        Token::Add => {
            let lhs = convert(iter)?;
            let rhs = convert(iter)?;
            Op::Add((Box::new(lhs), Box::new(rhs)))
        }
        Token::Sub => {
            let lhs = convert(iter)?;
            let rhs = convert(iter)?;
            Op::Sub((Box::new(lhs), Box::new(rhs)))
        }
        Token::Mul => {
            let lhs = convert(iter)?;
            let rhs = convert(iter)?;
            Op::Mul((Box::new(lhs), Box::new(rhs)))
        }
        Token::Div => {
            let lhs = convert(iter)?;
            let rhs = convert(iter)?;
            Op::Div((Box::new(lhs), Box::new(rhs)))
        }
        Token::IfElse((cond, if_val, else_val)) => {
            let cond = convert(&mut cond.into_iter())?;
            let if_val = convert(&mut if_val.into_iter())?;
            let else_val = convert(&mut else_val.into_iter())?;
            Op::IfElse((Box::new(cond), vec![if_val], vec![else_val]))
        }
        Token::LessThanCompare((lhs, rhs)) => {
            let lhs = convert(&mut lhs.into_iter())?;
            let rhs = convert(&mut rhs.into_iter())?;
            Op::LessThan((Box::new(lhs), Box::new(rhs)))
        }
        Token::SetValue((var, expr)) => {
            let expr = convert(&mut expr.into_iter())?;
            Op::StoreVar((var, Box::new(expr)))
        }
        _ => return Err(Error::UnexpectedToken),
    };
    Ok(op)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line() {
        let compiler = Compiler::new();

        let code = compiler.compile("h/2 - 200").unwrap();
        #[rustfmt::skip]
        let code2 = vec![Op::Sub((
            Box::new(Op::Div((
                Box::new(Op::LoadVar("h".to_string())),
                Box::new(Op::ConstFloat32(2.)),
            ))),
            Box::new(Op::ConstFloat32(200.)),
        ))];
        assert_eq!(code, code2);

        let code = compiler.compile("(x + h/2 + (y + 7)/5) - 200").unwrap();
        #[rustfmt::skip]
        let code2 = vec![Op::Sub((
            Box::new(Op::Add((
                Box::new(Op::Add((
                    Box::new(Op::LoadVar("x".to_string())),
                    Box::new(Op::Div((
                        Box::new(Op::LoadVar("h".to_string())),
                        Box::new(Op::ConstFloat32(2.))
                    ))),
                ))),

                Box::new(Op::Div((
                    Box::new(Op::Add((
                        Box::new(Op::LoadVar("y".to_string())),
                        Box::new(Op::ConstFloat32(7.))
                    ))),
                    Box::new(Op::ConstFloat32(5.))
                ))),
            ))),

            Box::new(Op::ConstFloat32(200.))
        ))];
        assert_eq!(code, code2);
    }

    #[test]
    fn h_minus_1() {
        let mut compiler = Compiler::new();
        let code = compiler.compile("h - 1").unwrap();
        #[rustfmt::skip]
        let code2 = vec![Op::Sub((
            Box::new(Op::LoadVar("h".to_string())),
            Box::new(Op::ConstFloat32(1.))
        ))];
        assert_eq!(code, code2);
    }

    #[test]
    fn dosub() {
        let mut compiler = Compiler::new();
        compiler.add_const_f32("HELLO", 110.);

        let code = compiler.compile("HELLO").unwrap();
        let code2 = vec![Op::ConstFloat32(110.)];
        assert_eq!(code, code2);
    }

    #[test]
    fn if_else() {
        let mut compiler = Compiler::new();
        let code = compiler
            .compile(
                "
            if h < 4 {
                h - 1
            } else {
                2 * h + 5
            }
        ",
            )
            .unwrap();
        let code2 = vec![Op::IfElse((
            Box::new(Op::LessThan((
                Box::new(Op::LoadVar("h".to_string())),
                Box::new(Op::ConstFloat32(4.)),
            ))),
            vec![Op::Sub((Box::new(Op::LoadVar("h".to_string())), Box::new(Op::ConstFloat32(1.))))],
            vec![Op::Add((
                Box::new(Op::Mul((
                    Box::new(Op::ConstFloat32(2.)),
                    Box::new(Op::LoadVar("h".to_string())),
                ))),
                Box::new(Op::ConstFloat32(5.)),
            ))],
        ))];
        assert_eq!(code, code2);
    }

    #[test]
    fn set_val() {
        let mut compiler = Compiler::new();
        let code = compiler
            .compile(
                "
            r = 10 / 4
        ",
            )
            .unwrap();
        let code2 = vec![Op::StoreVar((
            "r".to_string(),
            Box::new(Op::Div((Box::new(Op::ConstFloat32(10.)), Box::new(Op::ConstFloat32(4.))))),
        ))];
        assert_eq!(code, code2);
    }

    #[test]
    fn multiline() {
        let mut compiler = Compiler::new();
        let code = compiler
            .compile(
                "
            # This is a comment
            r = 10 / 4;

            s = if h < 4 {
                h - 1
            } else {
                2 * h + 5
            };

            r + 1
        ",
            )
            .unwrap();
        let code2 = vec![
            Op::StoreVar((
                "r".to_string(),
                Box::new(Op::Div((
                    Box::new(Op::ConstFloat32(10.)),
                    Box::new(Op::ConstFloat32(4.)),
                ))),
            )),
            Op::StoreVar((
                "s".to_string(),
                Box::new(Op::IfElse((
                    Box::new(Op::LessThan((
                        Box::new(Op::LoadVar("h".to_string())),
                        Box::new(Op::ConstFloat32(4.)),
                    ))),
                    vec![Op::Sub((
                        Box::new(Op::LoadVar("h".to_string())),
                        Box::new(Op::ConstFloat32(1.)),
                    ))],
                    vec![Op::Add((
                        Box::new(Op::Mul((
                            Box::new(Op::ConstFloat32(2.)),
                            Box::new(Op::LoadVar("h".to_string())),
                        ))),
                        Box::new(Op::ConstFloat32(5.)),
                    ))],
                ))),
            )),
            Op::Add((Box::new(Op::LoadVar("r".to_string())), Box::new(Op::ConstFloat32(1.)))),
        ];
        assert_eq!(code, code2);
    }
}
