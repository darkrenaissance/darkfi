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

use super::{Op, SExprCode};

fn remove_whitespace(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

pub fn compile(stmts: &str) -> Result<SExprCode> {
    let mut code = vec![];
    for stmt in stmts.split(';') {
        code.push(compile_line(stmt)?);
    }
    Ok(code)
}

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
}

impl Token {
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

fn tokenize(stmt: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut current_token = String::new();
    for chr in stmt.chars() {
        match chr {
            '+' => {
                clear_accum(&mut current_token, &mut tokens);
                tokens.push(Token::Add);
            }
            '-' => {
                clear_accum(&mut current_token, &mut tokens);
                tokens.push(Token::Sub);
            }
            '*' => {
                clear_accum(&mut current_token, &mut tokens);
                tokens.push(Token::Mul);
            }
            '/' => {
                clear_accum(&mut current_token, &mut tokens);
                tokens.push(Token::Div);
            }
            '(' => {
                clear_accum(&mut current_token, &mut tokens);
                tokens.push(Token::LeftParen);
            }
            ')' => {
                clear_accum(&mut current_token, &mut tokens);
                tokens.push(Token::RightParen);
            }
            _ => current_token.push(chr),
        }
    }
    clear_accum(&mut current_token, &mut tokens);
    tokens
}

fn clear_accum(current_token: &mut String, tokens: &mut Vec<Token>) {
    let prev_token = std::mem::replace(current_token, String::new());
    if prev_token.is_empty() {
        return
    }

    // Number or var?
    match prev_token.parse::<f32>() {
        Ok(v) => tokens.push(Token::ConstFloat32(v)),
        Err(_) => tokens.push(Token::LoadVar(prev_token)),
    }
}

/// Convert from infix to reverse polish notation
fn to_rpn(tokens: Vec<Token>) -> Result<Vec<Token>> {
    //println!("to_rpn = {tokens:#?}");
    let mut out = Vec::new();
    let mut stack = Vec::new();

    // Parens
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

    // */
    let tokens = out.clone();
    assert!(stack.is_empty());
    out.clear();
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
    let tokens = out.clone();
    out.clear();
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
        _ => return Err(Error::UnexpectedToken),
    };
    Ok(op)
}

fn compile_line(stmt: &str) -> Result<Op> {
    let stmt = remove_whitespace(stmt);
    let tokens = tokenize(&stmt);
    //println!("{tokens:#?}");
    let tokens = to_rpn(tokens)?;
    //println!("{tokens:#?}");
    Ok(convert(&mut tokens.into_iter())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line() {
        let code = compile("h/2 - 200").unwrap();
        #[rustfmt::skip]
        let code2 = vec![Op::Sub((
            Box::new(Op::Div((
                Box::new(Op::LoadVar("h".to_string())),
                Box::new(Op::ConstFloat32(2.)),
            ))),
            Box::new(Op::ConstFloat32(200.)),
        ))];
        assert_eq!(code, code2);

        let code = compile("(x + h/2 + (y + 7)/5) - 200").unwrap();
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
}
