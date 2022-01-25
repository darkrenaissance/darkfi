use std::{
    io,
    io::{stdin, stdout, Read, Write},
    process,
    str::Chars,
};

use termion::{color, style};

use super::{
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
        // `Analyzer` object when we're done.
        let mut statements = vec![];
        let mut stack = vec![];

        for statement in &self.statements {
            let mut stmt = statement.clone();

            let (return_types, arg_types) = statement.opcode.arg_types();
            let mut args = vec![];

            // For variable length args, we implement `BaseArray` and `ScalarArray`.
            // It's kinda ugly.
            if arg_types[0] == Type::BaseArray || arg_types[0] == Type::ScalarArray {
                if statement.args.is_empty() {
                    self.error(
                        format!(
                            "Passed no arguments to `{:?}` call. Expected at least 1.",
                            statement.opcode
                        ),
                        statement.line,
                        1,
                    );
                }

                for i in &statement.args {
                    if let Some(v) = self.lookup_var(&i.name) {
                        let var_type = match v {
                            Var::Constant(c) => c.typ,
                            Var::Witness(c) => c.typ,
                            Var::Variable(c) => c.typ,
                        };

                        if arg_types[0] == Type::BaseArray && var_type != Type::Base {
                            self.error(
                                format!(
                                    "Incorrect argument type. Expected `{:?}`, got `{:?}`",
                                    arg_types[0],
                                    Type::Base,
                                ),
                                i.line,
                                i.column,
                            );
                        }

                        if arg_types[0] == Type::ScalarArray && var_type != Type::Scalar {
                            self.error(
                                format!(
                                    "Incorrect argument type. Expected `{:?}`, got `{:?}`",
                                    arg_types[0],
                                    Type::Scalar,
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
                if statement.args.len() != arg_types.len() {
                    self.error(
                        format!(
                            "Incorrent number of args to `{:?}` call. Expected {}, got {}",
                            statement.opcode,
                            arg_types.len(),
                            statement.args.len()
                        ),
                        statement.line,
                        1,
                    );
                }

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

            match statement.typ {
                StatementType::Assignment => {
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
                    stmt.args = args;
                    statements.push(stmt);
                }
                _ => unreachable!(),
            }
        }

        self.statements = statements;
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
            let argnames: Vec<String> = i.args.iter().map(|x| x.name.clone()).collect();
            println!("Executing: {:?}({:?})", i.opcode, argnames);
            Analyzer::pause();

            for arg in &i.args {
                print!("Looking up `{}` on the stack... ", arg.name);
                if let Some(index) = stack.iter().position(|&r| r == &arg.name) {
                    println!("Found at stack index {}", index);
                } else {
                    self.error(
                        format!("Could not find `{}` on the stack", arg.name),
                        arg.line,
                        arg.column,
                    );
                }
                Analyzer::pause();
            }

            match i.typ {
                StatementType::Assignment => {
                    println!("Pushing result as `{}` to stack", &i.variable.as_ref().unwrap().name);
                    stack.push(&i.variable.as_ref().unwrap().name);
                    println!("Stack:\n{:#?}\n-----", stack);
                }
                StatementType::Call => {
                    println!("-----");
                }
                _ => unreachable!(),
            }
        }

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

    fn pause() {
        let msg = b"[Press Enter to continue]\r";
        let mut stdout = stdout();
        let _ = stdout.write(msg).unwrap();
        stdout.flush().unwrap();
        let _ = stdin().read(&mut [0]).unwrap();
        write!(stdout, "{}{}\r", termion::cursor::Up(1), termion::clear::CurrentLine).unwrap();
    }
}
