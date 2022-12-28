/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use std::{
    io::{stdin, stdout, Read, Write},
    str::Chars,
};

use super::{
    ast::{Arg, Constant, Literal, Statement, StatementType, Var, Variable, Witness},
    error::ErrorEmitter,
    Opcode, VarType,
};

pub struct Analyzer {
    pub constants: Vec<Constant>,
    pub witnesses: Vec<Witness>,
    pub statements: Vec<Statement>,
    pub literals: Vec<Literal>,
    pub stack: Vec<Variable>,
    error: ErrorEmitter,
}

impl Analyzer {
    pub fn new(
        filename: &str,
        source: Chars,
        constants: Vec<Constant>,
        witnesses: Vec<Witness>,
        statements: Vec<Statement>,
    ) -> Self {
        // For nice error reporting, we'll load everything into a string
        // vector so we have references to lines.
        let lines: Vec<String> = source.as_str().lines().map(|x| x.to_string()).collect();
        let error = ErrorEmitter::new("Semantic", filename, lines);

        Self { constants, witnesses, statements, literals: vec![], stack: vec![], error }
    }

    pub fn analyze_types(&mut self) {
        // To work around the pedantic safety, we'll make new vectors and then
        // replace the `statements` and `stack` vectors from the `Analyzer`
        // object when we are done.
        let mut statements = vec![];
        let mut stack = vec![];

        for statement in &self.statements {
            //println!("{:?}", statement);
            let mut stmt = statement.clone();

            let (return_types, arg_types) = statement.opcode.arg_types();
            let mut rhs = vec![];

            // This handling is kinda limiting, but it'll do for now.
            if !(arg_types[0] == VarType::BaseArray || arg_types[0] == VarType::ScalarArray) {
                // Check that number of args is correct
                if statement.rhs.len() != arg_types.len() {
                    self.error.abort(
                        &format!(
                            "Incorrect number of arguments for statement. Expected {}, got {}.",
                            arg_types.len(),
                            statement.rhs.len()
                        ),
                        statement.line,
                        1,
                    );
                }
            } else {
                // In case of arrays, check there's at least one element.
                if statement.rhs.is_empty() {
                    self.error.abort(
                        "Expected at least one element for statement using arrays.",
                        statement.line,
                        1,
                    );
                }
            }

            // Edge-cases for some opcodes
            #[allow(clippy::single_match)]
            match &statement.opcode {
                Opcode::RangeCheck => {
                    if let Arg::Lit(arg0) = &statement.rhs[0] {
                        if &arg0.name != "64" && &arg0.name != "253" {
                            self.error.abort(
                                "Supported range checks are only 64 and 253 bits.",
                                arg0.line,
                                arg0.column,
                            );
                        }
                    } else {
                        self.error.abort(
                            "Invalid argument for range_check opcode.",
                            statement.line,
                            0,
                        );
                    }
                }

                _ => {}
            }

            for (idx, arg) in statement.rhs.iter().enumerate() {
                // In case an argument is a function call, we will first
                // convert it to another statement that will get executed
                // before this one. An important assumption is that this
                // opcode has a return value. When executed we will push
                // this value onto the stack and use it as a reference to
                // the actual statement we're parsing at this moment.
                // TODO: FIXME: This needs a recursive algorithm, as this
                //              only allows a single nested function.
                if let Arg::Func(func) = arg {
                    let (f_return_types, f_arg_types) = func.opcode.arg_types();
                    if f_return_types.is_empty() {
                        self.error.abort(
                            &format!(
                                "Used a function argument which doesn't have a return value: {:?}",
                                func.opcode
                            ),
                            statement.line,
                            1,
                        );
                    }

                    let v = Variable {
                        name: func.lhs.clone().unwrap().name,
                        typ: f_return_types[0],
                        line: func.lhs.clone().unwrap().line,
                        column: func.lhs.clone().unwrap().column,
                    };

                    // FIXME: Needs better *Array handling.
                    if arg_types[0] == VarType::BaseArray {
                        if f_return_types[0] != VarType::Base {
                            self.error.abort(
                                &format!(
                                    "Function passed as argument returns wrong type. Expected `{:?}`, got `{:?}`.",
                                    VarType::Base,
                                    f_return_types[0],
                                ),
                                v.line,
                                v.column,
                            );
                        }
                    } else if arg_types[0] == VarType::ScalarArray {
                        if f_return_types[0] != VarType::Scalar {
                            self.error.abort(
                                &format!(
                                    "Function passed as argument returns wrong type. Expected `{:?}`, got `{:?}`.",
                                    VarType::Scalar,
                                    f_return_types[0],
                                ),
                                v.line,
                                v.column,
                            );
                        }
                    } else if f_return_types[0] != arg_types[idx] {
                        self.error.abort(
                            &format!(
                                "Function passed as argument returns wrong type. Expected `{:?}`, got `{:?}`.",
                                arg_types[idx],
                                f_return_types[0],
                            ),
                            v.line,
                            v.column,
                        );
                    }

                    // Replace the statement function call with the variable from
                    // the statement we just created to represent this nest.
                    stmt.rhs[idx] = Arg::Var(v.clone());

                    let mut rhs_inner = vec![];
                    for (inner_idx, i) in func.rhs.iter().enumerate() {
                        if let Arg::Var(v) = i {
                            if let Some(var_ref) = self.lookup_var(&v.name) {
                                let (var_type, ln, col) = match var_ref {
                                    Var::Constant(c) => (c.typ, c.line, c.column),
                                    Var::Witness(c) => (c.typ, c.line, c.column),
                                    Var::Variable(c) => (c.typ, c.line, c.column),
                                };

                                if var_type != f_arg_types[inner_idx] {
                                    self.error.abort(
                                        &format!(
                                            "Incorrect argument type. Expected `{:?}`, got `{:?}`.",
                                            f_arg_types[inner_idx], var_type
                                        ),
                                        ln,
                                        col,
                                    );
                                }

                                // Apply the proper type.
                                let mut v_new = v.clone();
                                v_new.typ = var_type;
                                rhs_inner.push(Arg::Var(v_new));

                                continue
                            }

                            self.error.abort(
                                &format!("Unknown variable reference `{}`.", v.name),
                                v.line,
                                v.column,
                            );
                        } else {
                            unimplemented!()
                        }
                    }

                    let s = Statement {
                        typ: func.typ,
                        opcode: func.opcode,
                        lhs: Some(v.clone()),
                        rhs: rhs_inner,
                        line: func.line,
                    };

                    // The lhs of the inner function call becomes rhs of the outer one.
                    rhs.push(Arg::Var(v.clone()));

                    // Add this to the list of statements.
                    statements.push(s);

                    // We replace self.stack here so we can do proper stack lookups.
                    stack.push(v.clone());
                    self.stack = stack.clone();

                    //println!("{:#?}", stack);
                    //println!("{:#?}", statements);
                    continue
                } // <-- Arg::Func

                // The literals get pushed on their own "stack", and
                // then the compiler will reference them by their own
                // index when it comes to running the statement that
                // requires the literal type.
                if let Arg::Lit(v) = arg {
                    // Match this literal type to a VarType for
                    // type checking.
                    let var_type = v.typ.to_vartype();
                    if var_type != arg_types[idx] {
                        self.error.abort(
                            &format!(
                                "Incorrect argument type. Expected `{:?}`, got `{:?}`.",
                                arg_types[idx], var_type
                            ),
                            v.line,
                            v.column,
                        );
                    }

                    self.literals.push(v.clone());
                    rhs.push(Arg::Lit(v.clone()));
                    continue
                }

                if let Arg::Var(v) = arg {
                    // Look up variable and check if type is correct.
                    if let Some(s_var) = self.lookup_var(&v.name) {
                        let (var_type, _ln, _col) = match s_var {
                            Var::Constant(c) => (c.typ, c.line, c.column),
                            Var::Witness(c) => (c.typ, c.line, c.column),
                            Var::Variable(c) => (c.typ, c.line, c.column),
                        };

                        // FIXME: Better array handling
                        if arg_types[0] == VarType::BaseArray {
                            if var_type != VarType::Base {
                                self.error.abort(
                                    &format!(
                                        "Incorrect argument type. Expected `{:?}`, got `{:?}`.",
                                        VarType::Base,
                                        var_type
                                    ),
                                    v.line,
                                    v.column,
                                );
                            }
                        } else if arg_types[0] == VarType::ScalarArray {
                            if var_type != VarType::Scalar {
                                self.error.abort(
                                    &format!(
                                        "Incorrect argument type. Expected `{:?}`, got `{:?}`.",
                                        VarType::Scalar,
                                        var_type
                                    ),
                                    v.line,
                                    v.column,
                                );
                            }
                        } else if var_type != arg_types[idx] {
                            self.error.abort(
                                &format!(
                                    "Incorrect argument type. Expected `{:?}`, got `{:?}`.",
                                    arg_types[idx], var_type
                                ),
                                v.line,
                                v.column,
                            );
                        }

                        // Replace Dummy type with correct type.
                        let mut v_new = v.clone();
                        v_new.typ = var_type;
                        rhs.push(Arg::Var(v_new));
                        continue
                    }

                    self.error.abort(
                        &format!("Unknown variable reference `{}`.", v.name),
                        v.line,
                        v.column,
                    );
                }
            } // <-- statement.rhs.iter().enumerate()

            // We now type-checked and assigned types to the statement rhs,
            // so now we apply it to the statement.
            stmt.rhs = rhs;

            // In case this statement is an assignment, we will push its
            // result on the stack.
            if statement.typ == StatementType::Assign {
                let mut var = statement.lhs.clone().unwrap();
                var.typ = return_types[0];
                stmt.lhs = Some(var.clone());
                stack.push(var.clone());
                self.stack = stack.clone();
            }

            //println!("{:#?}", stmt);
            statements.push(stmt);
        } // <-- for statement in &self.statements

        // Here we replace the self.statements and self.stack with what we
        // built so far. These can be used later on by the compiler after
        // this function is finished.
        self.statements = statements;
        self.stack = stack;

        //println!("=================STATEMENTS===============\n{:#?}", self.statements);
        //println!("===================STACK==================\n{:#?}", self.stack);
        //println!("==================LITERALS================\n{:#?}", self.literals);
    }

    fn lookup_var(&self, name: &str) -> Option<Var> {
        if let Some(r) = self.lookup_constant(name) {
            return Some(Var::Constant(r))
        }

        if let Some(r) = self.lookup_witness(name) {
            return Some(Var::Witness(r))
        }

        if let Some(r) = self.lookup_stack(name) {
            return Some(Var::Variable(r))
        }

        None
    }

    fn lookup_constant(&self, name: &str) -> Option<Constant> {
        for i in &self.constants {
            if i.name == name {
                return Some(i.clone())
            }
        }

        None
    }

    fn lookup_witness(&self, name: &str) -> Option<Witness> {
        for i in &self.witnesses {
            if i.name == name {
                return Some(i.clone())
            }
        }

        None
    }

    fn lookup_stack(&self, name: &str) -> Option<Variable> {
        for i in &self.stack {
            if i.name == name {
                return Some(i.clone())
            }
        }

        None
    }

    pub fn analyze_semantic(&mut self) {
        let mut stack = vec![];

        println!("Loading constants...\n-----");
        for i in &self.constants {
            println!("Adding `{}` to stack", i.name);
            stack.push(&i.name);
            Analyzer::pause();
        }
        println!("Stack:\n{:#?}\n-----", stack);
        println!("Loading witnesses...\n-----");
        for i in &self.witnesses {
            println!("Adding `{}` to stack", i.name);
            stack.push(&i.name);
            Analyzer::pause();
        }
        println!("Stack:\n{:#?}\n-----", stack);
        println!("Loading circuit...");
        for i in &self.statements {
            let mut argnames = vec![];
            for arg in &i.rhs {
                if let Arg::Var(arg) = arg {
                    argnames.push(arg.name.clone());
                } else if let Arg::Lit(lit) = arg {
                    argnames.push(lit.name.clone());
                } else {
                    unreachable!()
                }
            }
            println!("Executing: {:?}({:?})", i.opcode, argnames);

            Analyzer::pause();

            for arg in &i.rhs {
                if let Arg::Var(arg) = arg {
                    print!("Looking up `{}` on the stack... ", arg.name);
                    if let Some(index) = stack.iter().position(|&r| r == &arg.name) {
                        println!("Found at stack index {}", index);
                    } else {
                        self.error.abort(
                            &format!("Could not find `{}` on the stack", arg.name),
                            arg.line,
                            arg.column,
                        );
                    }
                } else if let Arg::Lit(lit) = arg {
                    println!("Using literal `{}`", lit.name);
                } else {
                    println!("{:#?}", arg);
                    unreachable!();
                }

                Analyzer::pause();
            }
            match i.typ {
                StatementType::Assign => {
                    println!("Pushing result as `{}` to stack", &i.lhs.as_ref().unwrap().name);
                    stack.push(&i.lhs.as_ref().unwrap().name);
                    println!("Stack:\n{:#?}\n-----", stack);
                }
                StatementType::Call => {
                    println!("-----");
                }
                _ => unreachable!(),
            }
        }
    }

    fn pause() {
        let msg = b"[Press Enter to continue]\r";
        let mut stdout = stdout();
        let _ = stdout.write(msg).unwrap();
        stdout.flush().unwrap();
        let _ = stdin().read(&mut [0]).unwrap();
        write!(stdout, "\x1b[1A\r\x1b[K\r").unwrap();
    }
}
