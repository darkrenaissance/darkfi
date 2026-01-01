/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use super::{LitType, Opcode, VarType};

#[derive(Clone, Debug)]
pub struct Constant {
    pub name: String,
    pub typ: VarType,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug)]
pub struct Witness {
    pub name: String,
    pub typ: VarType,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug)]
pub struct Variable {
    pub name: String,
    pub typ: VarType,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug)]
pub struct Literal {
    pub name: String,
    pub typ: LitType,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug)]
pub enum Var {
    Constant(Constant),
    Witness(Witness),
    Variable(Variable),
}

#[derive(Clone, Debug)]
pub enum Arg {
    Var(Variable),
    Lit(Literal),
    Func(Statement),
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum StatementType {
    Noop = 0x00,
    Assign = 0x01,
    Call = 0x02,
}

#[derive(Clone, Debug)]
pub struct Statement {
    pub typ: StatementType,
    pub opcode: Opcode,
    pub lhs: Option<Variable>,
    pub rhs: Vec<Arg>,
    pub line: usize,
}

impl Default for Statement {
    fn default() -> Self {
        Self { typ: StatementType::Noop, opcode: Opcode::Noop, lhs: None, rhs: vec![], line: 0 }
    }
}
