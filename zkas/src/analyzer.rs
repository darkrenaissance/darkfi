use std::{io, io::Write, process, str::Chars};

use termion::{color, style};

use crate::{
    ast::{
        Constant, Constants, StatementType, Statements, Var, Variable, Variables, Witness,
        Witnesses,
    },
    types::Type,
};

pub struct Analyzer {
    file: String,
    lines: Vec<String>,
    pub constants: Constants,
    pub witnesses: Witnesses,
    pub statements: Statements,
    pub stack: Variables,
}

impl Analyzer {
    pub fn new(
        filename: &str,
        source: Chars,
        constants: Constants,
        witnesses: Witnesses,
        statements: Statements,
    ) -> Self {
        // For nice error reporting, we'll load everything into a string
        // vector so we have references to lines.
        let lines = source.as_str().lines().map(|x| x.to_string()).collect();
        Analyzer {
            file: filename.to_string(),
            lines,
            constants,
            witnesses,
            statements,
            stack: vec![],
        }
    }

    pub fn analyze_types(&mut self) {
        // To work around the pedantic safety, we'll make new vectors and
        // then replace the `statements` and `stack` vectors from the
        // Analyzer object when we're done.
        let mut statements = vec![];
        let mut stack = vec![];

        for statement in &self.statements {
            let mut stmt = statement.clone();

            match statement.typ {
                StatementType::Assignment => {
                    let (return_types, arg_types) = statement.opcode.arg_types();
                    let mut args = vec![];

                    // For variable length args, we implement BaseArray.
                    // It's kinda ugly.
                    if arg_types[0] == Type::BaseArray {
                        for i in &statement.args {
                            if let Some(v) = self.lookup_var(&i.name) {
                                let var_type = match v {
                                    Var::Constant(c) => c.typ,
                                    Var::Witness(c) => c.typ,
                                    Var::Variable(c) => c.typ,
                                };
                                if var_type != Type::Base {
                                    self.error(
                                        format!(
                                            "Incorrect argument type. Expected `{:?}`, got `{:?}`",
                                            Type::Base,
                                            var_type
                                        ),
                                        i.line,
                                        i.column,
                                    );
                                }

                                let mut arg = i.clone();
                                arg.typ = var_type;
                                args.push(arg);
                            } else {
                                self.error(
                                    format!("Unknown argument reference `{}`.", i.name),
                                    i.line,
                                    i.column,
                                );
                            }
                        }
                    } else {
                        for (idx, i) in statement.args.iter().enumerate() {
                            if let Some(v) = self.lookup_var(&i.name) {
                                let var_type = match v {
                                    Var::Constant(c) => c.typ,
                                    Var::Witness(c) => c.typ,
                                    Var::Variable(c) => c.typ,
                                };

                                if var_type != arg_types[idx] {
                                    self.error(
                                        format!(
                                            "Incorrect argument type. Expected `{:?}`, got `{:?}`",
                                            arg_types[idx], var_type,
                                        ),
                                        i.line,
                                        i.column,
                                    );
                                }

                                let mut arg = i.clone();
                                arg.typ = var_type;
                                args.push(arg);
                            } else {
                                self.error(
                                    format!("Unknown argument reference `{}`.", i.name),
                                    i.line,
                                    i.column,
                                );
                            }
                        }
                    }

                    // Currently we just support a single return type.
                    let mut var = statement.variable.clone().unwrap();
                    var.typ = return_types[0];
                    stmt.variable = Some(var.clone());
                    stack.push(var.clone());
                    self.stack = stack.clone();
                    stmt.args = args;
                    statements.push(stmt);
                }

                StatementType::Call => {
                    let (_, arg_types) = statement.opcode.arg_types();
                    let mut args = vec![];

                    // For variable length args, we implement BaseArray.
                    // It's kinda ugly.
                    if arg_types[0] == Type::BaseArray {
                        for i in &statement.args {
                            if let Some(v) = self.lookup_var(&i.name) {
                                let var_type = match v {
                                    Var::Constant(c) => c.typ,
                                    Var::Witness(c) => c.typ,
                                    Var::Variable(c) => c.typ,
                                };
                                if var_type != Type::Base {
                                    self.error(
                                        format!(
                                            "Incorrect argument type. Expected `{:?}`, got `{:?}`",
                                            Type::Base,
                                            var_type
                                        ),
                                        i.line,
                                        i.column,
                                    );
                                }

                                let mut arg = i.clone();
                                arg.typ = var_type;
                                args.push(arg);
                            } else {
                                self.error(
                                    format!("Unknown argument reference `{}`.", i.name),
                                    i.line,
                                    i.column,
                                );
                            }
                        }
                    } else {
                        for (idx, i) in statement.args.iter().enumerate() {
                            if let Some(v) = self.lookup_var(&i.name) {
                                let var_type = match v {
                                    Var::Constant(c) => c.typ,
                                    Var::Witness(c) => c.typ,
                                    Var::Variable(c) => c.typ,
                                };

                                if var_type != arg_types[idx] {
                                    self.error(
                                        format!(
                                            "Incorrect argument type. Expected `{:?}`, got `{:?}`",
                                            arg_types[idx], var_type,
                                        ),
                                        i.line,
                                        i.column,
                                    );
                                }

                                let mut arg = i.clone();
                                arg.typ = var_type;
                                args.push(arg);
                            } else {
                                self.error(
                                    format!("Unknown argument reference `{}`.", i.name),
                                    i.line,
                                    i.column,
                                );
                            }
                        }
                    }
                    stmt.args = args;
                    statements.push(stmt);
                }
                StatementType::Noop => unreachable!(),
            }
        }

        self.statements = statements;
    }

    pub fn analyze_semantic(&mut self) {
        // println!("{:#?}", self.constants);
        // println!("{:#?}", self.witnesses);
        // println!("{:#?}", self.statements);
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

    fn error(&self, msg: String, ln: usize, col: usize) {
        let err_msg = format!("{} (line {}, column {})", msg, ln, col);
        let dbg_msg = format!("{}:{}:{}: {}", self.file, ln, col, self.lines[ln - 1]);
        let pad = dbg_msg.split(": ").next().unwrap().len() + col + 2;
        let caret = format!("{:width$}^", "", width = pad);
        let msg = format!("{}\n{}\n{}\n", err_msg, dbg_msg, caret);
        Analyzer::abort(&msg);
    }

    fn abort(msg: &str) {
        let stderr = io::stderr();
        let mut handle = stderr.lock();
        write!(
            handle,
            "{}{}Semantic error:{} {}",
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
