use std::{collections::HashMap, io, io::Write, process, str::Chars};

use itertools::Itertools;
use termion::{color, style};

use crate::{
    ast::{
        Constant, Constants, Statement, StatementType, Statements, UnparsedConstants,
        UnparsedWitnesses, Variable, Witness, Witnesses,
    },
    lexer::{Token, TokenType},
    opcode::Opcode,
    types::Type,
};

pub struct Parser {
    file: String,
    lines: Vec<String>,
    tokens: Vec<Token>,
}

impl Parser {
    pub fn new(filename: &str, source: Chars, tokens: Vec<Token>) -> Self {
        // For nice error reporting, we'll load everything into a string
        // vector so we have references to lines.
        let lines = source.as_str().lines().map(|x| x.to_string()).collect();
        Parser { file: filename.to_string(), lines, tokens }
    }

    pub fn parse(self) -> (Constants, Witnesses, Statements) {
        // We use these to keep state when iterating
        let mut declaring_constant = false;
        let mut declaring_contract = false;
        let mut declaring_circuit = false;

        let mut constant_tokens = vec![];
        let mut contract_tokens = vec![];
        let mut circuit_tokens = vec![];
        // Single statement in the circuit
        let mut circuit_statement = vec![];
        // All the circuit statements
        let mut circuit_statements = vec![];

        let mut ast = HashMap::new();
        let mut namespace = String::new();
        let mut ast_inner = HashMap::new();
        let mut namespace_found = false; // Nasty

        let mut iter = self.tokens.iter();
        while let Some(t) = iter.next() {
            // Start by declaring a section
            if !declaring_constant && !declaring_contract && !declaring_circuit {
                if t.token_type != TokenType::Symbol {
                    // TODO: Revisit
                    // TODO: Visit this again when we are allowing imports
                    unimplemented!();
                }

                // The sections we must be declaring in our source code
                match t.token.as_str() {
                    "constant" => {
                        declaring_constant = true;
                        // Eat all the tokens within the `constant` section
                        for inner in iter.by_ref() {
                            constant_tokens.push(inner.clone());
                            if inner.token_type == TokenType::RightBrace {
                                break
                            }
                        }
                    }

                    "contract" => {
                        declaring_contract = true;
                        // Eat all the tokens within the `contract` section
                        for inner in iter.by_ref() {
                            contract_tokens.push(inner.clone());
                            if inner.token_type == TokenType::RightBrace {
                                break
                            }
                        }
                    }

                    "circuit" => {
                        declaring_circuit = true;
                        // Eat all the tokens within the `circuit` section
                        // TODO: Revisit when we support if/else and loops
                        for inner in iter.by_ref() {
                            circuit_tokens.push(inner.clone());
                            if inner.token_type == TokenType::RightBrace {
                                break
                            }
                        }
                    }

                    x => self.error(format!("Unknown `{}` proof section", x), t.line, t.column),
                }
            }

            // We shouldn't be reaching these states
            if declaring_constant && (declaring_contract || declaring_circuit) {
                unreachable!()
            }
            if declaring_contract && (declaring_constant || declaring_circuit) {
                unreachable!()
            }
            if declaring_circuit && (declaring_constant || declaring_contract) {
                unreachable!()
            }

            // Now go through the token vectors and work it through
            if declaring_constant {
                self.check_section_structure("constant", constant_tokens.clone());

                // TODO: Do we need this?
                if namespace_found && namespace != constant_tokens[0].token {
                    self.error(
                        format!(
                            "Found `{}` namespace. Expected `{}`.",
                            constant_tokens[0].token, namespace
                        ),
                        constant_tokens[0].line,
                        constant_tokens[0].column,
                    );
                } else {
                    namespace = constant_tokens[0].token.clone();
                    namespace_found = true;
                }

                let constants_cloned = constant_tokens.clone();
                let mut constants_map = HashMap::new();
                // This is everything between the braces: { .. }
                let mut constants_inner = constants_cloned[2..constant_tokens.len() - 1].iter();

                while let Some((typ, name, comma)) = constants_inner.next_tuple() {
                    if comma.token_type != TokenType::Comma {
                        self.error(
                            "Separator is not a comma".to_string(),
                            comma.line,
                            comma.column,
                        );
                    }

                    if constants_map.contains_key(name.token.as_str()) {
                        self.error(
                            format!(
                                "Section `constant` already contains the token `{}`.",
                                &name.token
                            ),
                            name.line,
                            name.column,
                        );
                    }

                    constants_map.insert(name.token.clone(), (name.clone(), typ.clone()));
                }

                ast_inner.insert("constant".to_string(), constants_map);
                declaring_constant = false;
            }

            if declaring_contract {
                self.check_section_structure("contract", contract_tokens.clone());

                // TODO: Do we need this?
                if namespace_found && namespace != contract_tokens[0].token {
                    self.error(
                        format!(
                            "Found `{}` namespace. Expected `{}`.",
                            contract_tokens[0].token, namespace
                        ),
                        contract_tokens[0].line,
                        contract_tokens[0].column,
                    );
                } else {
                    namespace = contract_tokens[0].token.clone();
                    namespace_found = true;
                }

                let contract_cloned = contract_tokens.clone();
                let mut contract_map = HashMap::new();
                // This is everything between the braces: { .. }
                let mut contract_inner = contract_cloned[2..contract_tokens.len() - 1].iter();

                while let Some((typ, name, comma)) = contract_inner.next_tuple() {
                    if comma.token_type != TokenType::Comma {
                        self.error(
                            "Separator is not a comma".to_string(),
                            comma.line,
                            comma.column,
                        );
                    }

                    if contract_map.contains_key(name.token.as_str()) {
                        self.error(
                            format!(
                                "Section `contract` already contains the token `{}`.",
                                &name.token
                            ),
                            name.line,
                            name.column,
                        );
                    }

                    contract_map.insert(name.token.clone(), (name.clone(), typ.clone()));
                }

                ast_inner.insert("contract".to_string(), contract_map);
                declaring_contract = false;
            }

            if declaring_circuit {
                self.check_section_structure("circuit", contract_tokens.clone());

                if circuit_tokens[circuit_tokens.len() - 2].token_type != TokenType::Semicolon {
                    self.error(
                        "Circuit section does not end with a semicolon. Would never finish parsing.".to_string(),
                        circuit_tokens[circuit_tokens.len()-2].line,
                        circuit_tokens[circuit_tokens.len()-2].column
                    );
                }

                // TODO: Do we need this?
                if namespace_found && namespace != circuit_tokens[0].token {
                    self.error(
                        format!(
                            "Found `{}` namespace. Expected `{}`.",
                            circuit_tokens[0].token, namespace
                        ),
                        circuit_tokens[0].line,
                        circuit_tokens[0].column,
                    );
                } else {
                    namespace = circuit_tokens[0].token.clone();
                    namespace_found = true;
                }

                for i in circuit_tokens.clone()[2..circuit_tokens.len() - 1].iter() {
                    if i.token_type == TokenType::Semicolon {
                        circuit_statements.push(circuit_statement.clone());
                        // println!("{:?}", circuit_statement);
                        circuit_statement = vec![];
                        continue
                    }
                    circuit_statement.push(i.clone());
                }

                declaring_circuit = false;
            }
        }

        ast.insert(namespace.clone(), ast_inner);
        // TODO: Verify there are both constant/contract sections
        // TODO: Verify there is a circuit section

        // Clean up the `constant` section
        let c = ast.get(&namespace).unwrap().get("constant").unwrap();
        let constants = self.parse_ast_constants(c);

        // Clean up the `contract` section
        let c = ast.get(&namespace).unwrap().get("contract").unwrap();
        let witnesses = self.parse_ast_contract(c);

        // Clean up the `circuit` section
        let stmt = self.parse_ast_circuit(circuit_statements);

        (constants, witnesses, stmt)
    }

    fn check_section_structure(&self, section: &str, tokens: Vec<Token>) {
        if tokens[0].token_type != TokenType::String {
            self.error(
                format!("{} section declaration must start with a naming string.", section),
                tokens[0].line,
                tokens[0].column,
            );
        }

        if tokens[1].token_type != TokenType::LeftBrace {
            self.error(
                format!(
                    "{} section opening is not correct. Must be opened with a left brace `{{`",
                    section
                ),
                tokens[0].line,
                tokens[0].column,
            );
        }

        if tokens[tokens.len() - 1].token_type != TokenType::RightBrace {
            self.error(
                format!(
                    "{} section closing is not correct. Must be closed with a right brace `}}`",
                    section
                ),
                tokens[0].line,
                tokens[0].column,
            );
        }

        if (section == "constant" || section == "contract") &&
            tokens[2..tokens.len() - 1].len() % 3 != 0
        {
            self.error(
                format!(
                    "Invalid number of elements in `{}` section. Must be pairs of `type:name` separated with a comma `,`",
                    section
                ),
                tokens[0].line,
                tokens[0].column,
            );
        }
    }

    fn parse_ast_constants(&self, ast: &UnparsedConstants) -> Constants {
        let mut ret = vec![];

        for (k, v) in ast {
            if &v.0.token != k {
                self.error(
                    format!("Constant name `{}` doesn't match token `{}`.", v.0.token, k),
                    v.0.line,
                    v.0.column,
                );
            }

            if v.0.token_type != TokenType::Symbol {
                self.error(
                    format!("Constant name `{}` is not a symbol.", v.0.token),
                    v.0.line,
                    v.0.column,
                );
            }

            if v.1.token_type != TokenType::Symbol {
                self.error(
                    format!("Constant type `{}` is not a symbol.", v.1.token),
                    v.1.line,
                    v.1.column,
                );
            }

            match v.1.token.as_str() {
                "EcFixedPoint" => {
                    ret.push(Constant {
                        name: k.to_string(),
                        typ: Type::EcFixedPoint,
                        line: v.0.line,
                        column: v.0.column,
                    });
                }

                x => {
                    self.error(
                        format!("`{}` is an illegal constant type", x),
                        v.1.line,
                        v.1.column,
                    );
                }
            }
        }

        ret
    }

    fn parse_ast_contract(&self, ast: &UnparsedWitnesses) -> Witnesses {
        let mut ret = vec![];

        for (k, v) in ast {
            if &v.0.token != k {
                self.error(
                    format!("Witness name `{}` doesn't match token `{}`.", v.0.token, k),
                    v.0.line,
                    v.0.column,
                );
            }

            if v.0.token_type != TokenType::Symbol {
                self.error(
                    format!("Witness name `{}` is not a symbol.", v.0.token),
                    v.0.line,
                    v.0.column,
                );
            }

            if v.1.token_type != TokenType::Symbol {
                self.error(
                    format!("Witness type `{}` is not a symbol.", v.1.token),
                    v.1.line,
                    v.1.column,
                );
            }

            match v.1.token.as_str() {
                "Base" => {
                    ret.push(Witness {
                        name: k.to_string(),
                        typ: Type::Base,
                        line: v.0.line,
                        column: v.0.column,
                    });
                }

                "Scalar" => {
                    ret.push(Witness {
                        name: k.to_string(),
                        typ: Type::Scalar,
                        line: v.0.line,
                        column: v.0.column,
                    });
                }

                "MerklePath" => {
                    ret.push(Witness {
                        name: k.to_string(),
                        typ: Type::MerklePath,
                        line: v.0.line,
                        column: v.0.column,
                    });
                }

                x => {
                    self.error(format!("`{}` is an illegal witness type", x), v.1.line, v.1.column);
                }
            }
        }

        ret
    }

    fn parse_ast_circuit(&self, statements: Vec<Vec<Token>>) -> Vec<Statement> {
        // 1. Scan the tokens to map opcodes (function calls)
        // 2. For each statement, see if there are variable assignments
        // 3. When referencing, check if they're in Constants, Witnesses
        //    and finally, or they've been assigned

        let mut stmts = vec![];

        for statement in statements {
            // TODO: If there are parentheses, verify that there are both
            //       openings and closings.

            // C = poseidon_hash(pub_x, pub_y, value, token, serial, coin_blind)
            // | |         |                     |
            // V V         V                     V
            // variable   opcode                args
            // assign

            // constrain_instance(C)
            //     |              |
            //     V              V
            //   opcode         args

            let mut iter = statement.iter().peekable();
            let mut stmt = Statement::default();

            let mut parsing = false;

            while let Some(token) = iter.next() {
                if !parsing {
                    if let Some(next_token) = iter.peek() {
                        if next_token.token_type == TokenType::Assign {
                            stmt.typ = StatementType::Assignment;
                            stmt.variable = Some(Variable {
                                name: token.token.clone(),
                                line: token.line,
                                column: token.column,
                            });
                            // Skip over the `=` token.
                            iter.next();
                            parsing = true;
                            continue
                        }

                        if next_token.token_type == TokenType::LeftParen {
                            stmt.typ = StatementType::Call;
                            stmt.variable = None;
                            parsing = true;
                        }

                        if !parsing {
                            self.error(
                                format!("Illegal token `{}`", next_token.token),
                                next_token.line,
                                next_token.column,
                            );
                        }
                    }
                }

                // TODO: Clean up this redundancy
                match token.token.as_str() {
                    "poseidon_hash" => {
                        if let Some(next_token) = iter.peek() {
                            if next_token.token_type != TokenType::LeftParen {
                                self.error(
                                    "Invalid function call opening. Must start with a `(`"
                                        .to_string(),
                                    next_token.line,
                                    next_token.column,
                                );
                            }
                            // Skip the opening parenthesis
                            iter.next();
                        } else {
                            self.error(
                                "Premature ending of statement".to_string(),
                                token.line,
                                token.column,
                            );
                        }

                        // Eat up function arguments
                        let mut args = vec![];
                        while let Some((arg, sep)) = iter.next_tuple() {
                            args.push(Variable {
                                name: arg.token.clone(),
                                line: arg.line,
                                column: arg.column,
                            });

                            if sep.token_type == TokenType::RightParen {
                                // Reached end of args
                                break
                            }

                            if sep.token_type != TokenType::Comma {
                                self.error(
                                    "Argument separator is not a comma (`,`)".to_string(),
                                    sep.line,
                                    sep.column,
                                );
                            }
                        }

                        stmt.args = args.clone();
                        stmt.opcode = Opcode::PoseidonHash;
                        stmt.line = token.line;
                        stmts.push(stmt.clone());

                        parsing = false;
                        continue
                    }

                    "constrain_instance" => {
                        if let Some(next_token) = iter.peek() {
                            if next_token.token_type != TokenType::LeftParen {
                                self.error(
                                    "Invalid function call opening. Must start with a `(`"
                                        .to_string(),
                                    next_token.line,
                                    next_token.column,
                                );
                            }
                            // Skip the opening parenthesis
                            iter.next();
                        } else {
                            self.error(
                                "Premature ending of statement".to_string(),
                                token.line,
                                token.column,
                            );
                        }

                        // Eat up function arguments
                        let mut args = vec![];
                        while let Some((arg, sep)) = iter.next_tuple() {
                            args.push(Variable {
                                name: arg.token.clone(),
                                line: arg.line,
                                column: arg.column,
                            });

                            if sep.token_type == TokenType::RightParen {
                                // Reached end of args
                                break
                            }

                            if sep.token_type != TokenType::Comma {
                                self.error(
                                    "Argument separator is not a comma (`,`)".to_string(),
                                    sep.line,
                                    sep.column,
                                );
                            }
                        }

                        stmt.args = args.clone();
                        stmt.opcode = Opcode::ConstrainInstance;
                        stmt.line = token.line;
                        stmts.push(stmt.clone());

                        parsing = false;
                        continue
                    }

                    "calculate_merkle_root" => {
                        if let Some(next_token) = iter.peek() {
                            if next_token.token_type != TokenType::LeftParen {
                                self.error(
                                    "Invalid function call opening. Must start with a `(`"
                                        .to_string(),
                                    next_token.line,
                                    next_token.column,
                                );
                            }
                            // Skip the opening parenthesis
                            iter.next();
                        } else {
                            self.error(
                                "Premature ending of statement".to_string(),
                                token.line,
                                token.column,
                            );
                        }

                        // Eat up function arguments
                        let mut args = vec![];
                        while let Some((arg, sep)) = iter.next_tuple() {
                            args.push(Variable {
                                name: arg.token.clone(),
                                line: arg.line,
                                column: arg.column,
                            });

                            if sep.token_type == TokenType::RightParen {
                                // Reached end of args
                                break
                            }

                            if sep.token_type != TokenType::Comma {
                                self.error(
                                    "Argument separator is not a comma (`,`)".to_string(),
                                    sep.line,
                                    sep.column,
                                );
                            }
                        }

                        stmt.args = args.clone();
                        stmt.opcode = Opcode::CalculateMerkleRoot;
                        stmt.line = token.line;
                        stmts.push(stmt.clone());

                        parsing = false;
                        continue
                    }

                    "ec_mul_short" => {
                        if let Some(next_token) = iter.peek() {
                            if next_token.token_type != TokenType::LeftParen {
                                self.error(
                                    "Invalid function call opening. Must start with a `(`"
                                        .to_string(),
                                    next_token.line,
                                    next_token.column,
                                );
                            }
                            // Skip the opening parenthesis
                            iter.next();
                        } else {
                            self.error(
                                "Premature ending of statement".to_string(),
                                token.line,
                                token.column,
                            );
                        }

                        // Eat up function arguments
                        let mut args = vec![];
                        while let Some((arg, sep)) = iter.next_tuple() {
                            args.push(Variable {
                                name: arg.token.clone(),
                                line: arg.line,
                                column: arg.column,
                            });

                            if sep.token_type == TokenType::RightParen {
                                // Reached end of args
                                break
                            }

                            if sep.token_type != TokenType::Comma {
                                self.error(
                                    "Argument separator is not a comma (`,`)".to_string(),
                                    sep.line,
                                    sep.column,
                                );
                            }
                        }

                        stmt.args = args.clone();
                        stmt.opcode = Opcode::EcMulShort;
                        stmt.line = token.line;
                        stmts.push(stmt.clone());

                        parsing = false;
                        continue
                    }

                    "ec_mul" => {
                        if let Some(next_token) = iter.peek() {
                            if next_token.token_type != TokenType::LeftParen {
                                self.error(
                                    "Invalid function call opening. Must start with a `(`"
                                        .to_string(),
                                    next_token.line,
                                    next_token.column,
                                );
                            }
                            // Skip the opening parenthesis
                            iter.next();
                        } else {
                            self.error(
                                "Premature ending of statement".to_string(),
                                token.line,
                                token.column,
                            );
                        }

                        // Eat up function arguments
                        let mut args = vec![];
                        while let Some((arg, sep)) = iter.next_tuple() {
                            args.push(Variable {
                                name: arg.token.clone(),
                                line: arg.line,
                                column: arg.column,
                            });

                            if sep.token_type == TokenType::RightParen {
                                // Reached end of args
                                break
                            }

                            if sep.token_type != TokenType::Comma {
                                self.error(
                                    "Argument separator is not a comma (`,`)".to_string(),
                                    sep.line,
                                    sep.column,
                                );
                            }
                        }

                        stmt.args = args.clone();
                        stmt.opcode = Opcode::EcMul;
                        stmt.line = token.line;
                        stmts.push(stmt.clone());

                        parsing = false;
                        continue
                    }

                    "ec_get_x" => {
                        if let Some(next_token) = iter.peek() {
                            if next_token.token_type != TokenType::LeftParen {
                                self.error(
                                    "Invalid function call opening. Must start with a `(`"
                                        .to_string(),
                                    next_token.line,
                                    next_token.column,
                                );
                            }
                            // Skip the opening parenthesis
                            iter.next();
                        } else {
                            self.error(
                                "Premature ending of statement".to_string(),
                                token.line,
                                token.column,
                            );
                        }

                        // Eat up function arguments
                        let mut args = vec![];
                        while let Some((arg, sep)) = iter.next_tuple() {
                            args.push(Variable {
                                name: arg.token.clone(),
                                line: arg.line,
                                column: arg.column,
                            });

                            if sep.token_type == TokenType::RightParen {
                                // Reached end of args
                                break
                            }

                            if sep.token_type != TokenType::Comma {
                                self.error(
                                    "Argument separator is not a comma (`,`)".to_string(),
                                    sep.line,
                                    sep.column,
                                );
                            }
                        }

                        stmt.args = args.clone();
                        stmt.opcode = Opcode::EcGetX;
                        stmt.line = token.line;
                        stmts.push(stmt.clone());

                        parsing = false;
                        continue
                    }

                    "ec_get_y" => {
                        if let Some(next_token) = iter.peek() {
                            if next_token.token_type != TokenType::LeftParen {
                                self.error(
                                    "Invalid function call opening. Must start with a `(`"
                                        .to_string(),
                                    next_token.line,
                                    next_token.column,
                                );
                            }
                            // Skip the opening parenthesis
                            iter.next();
                        } else {
                            self.error(
                                "Premature ending of statement".to_string(),
                                token.line,
                                token.column,
                            );
                        }

                        // Eat up function arguments
                        let mut args = vec![];
                        while let Some((arg, sep)) = iter.next_tuple() {
                            args.push(Variable {
                                name: arg.token.clone(),
                                line: arg.line,
                                column: arg.column,
                            });

                            if sep.token_type == TokenType::RightParen {
                                // Reached end of args
                                break
                            }

                            if sep.token_type != TokenType::Comma {
                                self.error(
                                    "Argument separator is not a comma (`,`)".to_string(),
                                    sep.line,
                                    sep.column,
                                );
                            }
                        }

                        stmt.args = args.clone();
                        stmt.opcode = Opcode::EcGetY;
                        stmt.line = token.line;
                        stmts.push(stmt.clone());

                        parsing = false;
                        continue
                    }

                    "ec_add" => {
                        if let Some(next_token) = iter.peek() {
                            if next_token.token_type != TokenType::LeftParen {
                                self.error(
                                    "Invalid function call opening. Must start with a `(`"
                                        .to_string(),
                                    next_token.line,
                                    next_token.column,
                                );
                            }
                            // Skip the opening parenthesis
                            iter.next();
                        } else {
                            self.error(
                                "Premature ending of statement".to_string(),
                                token.line,
                                token.column,
                            );
                        }

                        // Eat up function arguments
                        let mut args = vec![];
                        while let Some((arg, sep)) = iter.next_tuple() {
                            args.push(Variable {
                                name: arg.token.clone(),
                                line: arg.line,
                                column: arg.column,
                            });

                            if sep.token_type == TokenType::RightParen {
                                // Reached end of args
                                break
                            }

                            if sep.token_type != TokenType::Comma {
                                self.error(
                                    "Argument separator is not a comma (`,`)".to_string(),
                                    sep.line,
                                    sep.column,
                                );
                            }
                        }

                        stmt.args = args.clone();
                        stmt.opcode = Opcode::EcAdd;
                        stmt.line = token.line;
                        stmts.push(stmt.clone());

                        parsing = false;
                        continue
                    }

                    x => {
                        self.error(
                            format!("Unimplemented function call `{}`", x),
                            token.line,
                            token.column,
                        );
                    }
                }
            }
        }

        // println!("{:#?}", stmts);
        stmts
    }

    fn error(&self, msg: String, ln: usize, col: usize) {
        let err_msg = format!("{} (line {}, column {})", msg, ln, col);
        let dbg_msg = format!("{}:{}:{}: {}", self.file, ln, col, self.lines[ln - 1]);
        let pad = dbg_msg.split(": ").next().unwrap().len() + col + 2;
        let caret = format!("{:width$}^", "", width = pad);
        let msg = format!("{}\n{}\n{}\n", err_msg, dbg_msg, caret);
        Parser::abort(&msg);
    }

    fn abort(msg: &str) {
        let stderr = io::stderr();
        let mut handle = stderr.lock();
        write!(
            handle,
            "{}{}Parser error:{} {}",
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
