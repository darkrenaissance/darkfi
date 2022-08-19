use std::{iter::Peekable, str::Chars};

use fxhash::FxBuildHasher;
use indexmap::IndexMap;
use itertools::Itertools;

use super::{
    ast::{
        Constant, Constants, Statement, StatementType, Statements, UnparsedConstants,
        UnparsedWitnesses, Variable, Witness, Witnesses,
    },
    error::ErrorEmitter,
    lexer::{Token, TokenType},
    opcode::Opcode,
    types::Type,
};

pub struct Parser {
    tokens: Vec<Token>,
    error: ErrorEmitter,
}

impl Parser {
    pub fn new(filename: &str, source: Chars, tokens: Vec<Token>) -> Self {
        // For nice error reporting, we'll load everything into a string
        // vector so we have references to lines.
        let lines: Vec<String> = source.as_str().lines().map(|x| x.to_string()).collect();
        let error = ErrorEmitter::new("Parser", filename, lines);

        Parser { tokens, error }
    }

    pub fn parse(self) -> (Constants, Witnesses, Statements) {
        // We use these to keep state when iterating
        let (mut declaring_constant, mut declared_constant) = (false, false);
        let (mut declaring_contract, mut declared_contract) = (false, false);
        let (mut declaring_circuit, mut declared_circuit) = (false, false);

        let mut constant_tokens = vec![];
        let mut contract_tokens = vec![];
        let mut circuit_tokens = vec![];
        // Single statement in the circuit
        let mut circuit_statement = vec![];
        // All the circuit statements
        let mut circuit_statements = vec![];

        let mut ast = IndexMap::with_hasher(FxBuildHasher::default());
        let mut namespace = String::new();
        let mut ast_inner = IndexMap::new();
        let mut namespace_found = false; // Nasty

        let mut iter = self.tokens.iter();
        while let Some(t) = iter.next() {
            // Start by declaring a section
            if !declaring_constant && !declaring_contract && !declaring_circuit {
                if t.token_type != TokenType::Symbol {
                    self.error.abort(
                        "Source file does not start with a section.
Expected `constant/contract/circuit`.",
                        0,
                        0,
                    );
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

                    x => self.error.abort(
                        &format!("Unknown `{}` proof section", x),
                        t.line,
                        t.column,
                    ),
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
                if declared_constant {
                    self.error.abort("Duplicate `constant` section found", 0, 0);
                }
                self.check_section_structure("constant", constant_tokens.clone());

                if namespace_found && namespace != constant_tokens[0].token {
                    self.error.abort(
                        &format!(
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
                let mut constants_map = IndexMap::new();
                // This is everything between the braces: { .. }
                let mut constants_inner = constants_cloned[2..constant_tokens.len() - 1].iter();

                while let Some((typ, name, comma)) = constants_inner.next_tuple() {
                    if comma.token_type != TokenType::Comma {
                        self.error.abort("Separator is not a comma", comma.line, comma.column);
                    }

                    if constants_map.contains_key(name.token.as_str()) {
                        self.error.abort(
                            &format!(
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
                declared_constant = true;
            }

            if declaring_contract {
                if declared_contract {
                    self.error.abort("Duplicate `contract` section found", 0, 0);
                }
                self.check_section_structure("contract", contract_tokens.clone());

                if namespace_found && namespace != contract_tokens[0].token {
                    self.error.abort(
                        &format!(
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
                let mut contract_map = IndexMap::new();
                // This is everything between the braces: { .. }
                let mut contract_inner = contract_cloned[2..contract_tokens.len() - 1].iter();

                while let Some((typ, name, comma)) = contract_inner.next_tuple() {
                    if comma.token_type != TokenType::Comma {
                        self.error.abort("Separator is not a comma", comma.line, comma.column);
                    }

                    if contract_map.contains_key(name.token.as_str()) {
                        self.error.abort(
                            &format!(
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
                declared_contract = true;
            }

            if declaring_circuit {
                if declared_circuit {
                    self.error.abort("Duplicate `circuit` section found", 0, 0);
                }
                self.check_section_structure("circuit", contract_tokens.clone());

                if circuit_tokens[circuit_tokens.len() - 2].token_type != TokenType::Semicolon {
                    self.error.abort(
                        "Circuit section does not end with a semicolon. Would never finish parsing.",
                        circuit_tokens[circuit_tokens.len()-2].line,
                        circuit_tokens[circuit_tokens.len()-2].column
                    );
                }

                if namespace_found && namespace != circuit_tokens[0].token {
                    self.error.abort(
                        &format!(
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
                declared_circuit = true;
            }
        }

        ast.insert(namespace.clone(), ast_inner);
        // TODO: Check that there are no duplicate names in constants, contract
        //       and circuit assignments

        // Clean up the `constant` section
        let c = match ast.get(&namespace).unwrap().get("constant") {
            Some(c) => c,
            None => {
                self.error.abort("Missing `constant` section in .zk source", 0, 0);
                unreachable!()
            }
        };
        let constants = self.parse_ast_constants(c);
        if constants.is_empty() {
            self.error.warn("Constant section is empty", 0, 0);
        }

        // Clean up the `contract` section
        let c = match ast.get(&namespace).unwrap().get("contract") {
            Some(c) => c,
            None => {
                self.error.abort("Missing `contract` section in .zk source", 0, 0);
                unreachable!()
            }
        };
        let witnesses = self.parse_ast_contract(c);
        if witnesses.is_empty() {
            self.error.abort("Contract section is empty", 0, 0);
        }

        // Clean up the `circuit` section
        let stmt = self.parse_ast_circuit(circuit_statements);
        if stmt.is_empty() {
            self.error.abort("Circuit section is empty", 0, 0);
        }

        (constants, witnesses, stmt)
    }

    fn check_section_structure(&self, section: &str, tokens: Vec<Token>) {
        if tokens[0].token_type != TokenType::String {
            self.error.abort(
                &format!("{} section declaration must start with a naming string.", section),
                tokens[0].line,
                tokens[0].column,
            );
        }

        if tokens[1].token_type != TokenType::LeftBrace {
            self.error.abort(
                &format!(
                    "{} section opening is not correct. Must be opened with a left brace `{{`",
                    section
                ),
                tokens[0].line,
                tokens[0].column,
            );
        }

        if tokens[tokens.len() - 1].token_type != TokenType::RightBrace {
            self.error.abort(
                &format!(
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
            self.error.abort(
                &format!(
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
                self.error.abort(
                    &format!("Constant name `{}` doesn't match token `{}`.", v.0.token, k),
                    v.0.line,
                    v.0.column,
                );
            }

            if v.0.token_type != TokenType::Symbol {
                self.error.abort(
                    &format!("Constant name `{}` is not a symbol.", v.0.token),
                    v.0.line,
                    v.0.column,
                );
            }

            if v.1.token_type != TokenType::Symbol {
                self.error.abort(
                    &format!("Constant type `{}` is not a symbol.", v.1.token),
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

                "EcFixedPointShort" => {
                    ret.push(Constant {
                        name: k.to_string(),
                        typ: Type::EcFixedPointShort,
                        line: v.0.line,
                        column: v.0.column,
                    });
                }

                "EcFixedPointBase" => {
                    ret.push(Constant {
                        name: k.to_string(),
                        typ: Type::EcFixedPointBase,
                        line: v.0.line,
                        column: v.0.column,
                    });
                }

                x => {
                    self.error.abort(
                        &format!("`{}` is an illegal constant type", x),
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
                self.error.abort(
                    &format!("Witness name `{}` doesn't match token `{}`.", v.0.token, k),
                    v.0.line,
                    v.0.column,
                );
            }

            if v.0.token_type != TokenType::Symbol {
                self.error.abort(
                    &format!("Witness name `{}` is not a symbol.", v.0.token),
                    v.0.line,
                    v.0.column,
                );
            }

            if v.1.token_type != TokenType::Symbol {
                self.error.abort(
                    &format!("Witness type `{}` is not a symbol.", v.1.token),
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

                "Uint32" => {
                    ret.push(Witness {
                        name: k.to_string(),
                        typ: Type::Uint32,
                        line: v.0.line,
                        column: v.0.column,
                    });
                }

                "Uint64" => {
                    ret.push(Witness {
                        name: k.to_string(),
                        typ: Type::Uint64,
                        line: v.0.line,
                        column: v.0.column,
                    });
                }

                x => {
                    self.error.abort(
                        &format!("`{}` is an illegal witness type", x),
                        v.1.line,
                        v.1.column,
                    );
                }
            }
        }

        ret
    }

    fn parse_ast_circuit(&self, statements: Vec<Vec<Token>>) -> Vec<Statement> {
        let mut stmts = vec![];

        for statement in statements {
            let (mut left_paren, mut right_paren) = (0, 0);
            for i in &statement {
                match i.token.as_str() {
                    "(" => left_paren += 1,
                    ")" => right_paren += 1,
                    _ => {}
                }
            }
            if left_paren != right_paren {
                self.error.abort(
                    "Incorrect number of left and right parenthesis for statement.",
                    statement[0].line,
                    statement[0].column,
                );
            }

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
                                typ: Type::Dummy,
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
                            self.error.abort(
                                &format!("Illegal token `{}`", next_token.token),
                                next_token.line,
                                next_token.column,
                            );
                        }
                    }
                }

                // This matching could be moved over into the semantic analyzer.
                // We could just parse any kind of symbol here, and then do lookup
                // from the analyzer, to see if the calls actually exist and are
                // supported.
                // But for now, we'll just leave it here and expand later.
                let func_name = token.token.as_str();

                macro_rules! parse_func {
                    ($opcode: expr) => {
                        stmt.args = self.parse_function_call(token, &mut iter);
                        stmt.opcode = $opcode;
                        stmt.line = token.line;
                        stmts.push(stmt.clone());

                        parsing = false;
                        continue
                    };
                }

                match func_name {
                    "poseidon_hash" => {
                        parse_func!(Opcode::PoseidonHash);
                    }

                    "constrain_instance" => {
                        parse_func!(Opcode::ConstrainInstance);
                    }

                    "calculate_merkle_root" => {
                        parse_func!(Opcode::CalculateMerkleRoot);
                    }

                    "ec_mul_short" => {
                        parse_func!(Opcode::EcMulShort);
                    }

                    "ec_mul_base" => {
                        parse_func!(Opcode::EcMulBase);
                    }

                    "ec_mul" => {
                        parse_func!(Opcode::EcMul);
                    }

                    "ec_get_x" => {
                        parse_func!(Opcode::EcGetX);
                    }

                    "ec_get_y" => {
                        parse_func!(Opcode::EcGetY);
                    }

                    "ec_add" => {
                        parse_func!(Opcode::EcAdd);
                    }

                    "base_add" => {
                        parse_func!(Opcode::BaseAdd);
                    }

                    "base_mul" => {
                        parse_func!(Opcode::BaseMul);
                    }

                    "base_sub" => {
                        parse_func!(Opcode::BaseSub);
                    }

                    "greater_than" => {
                        parse_func!(Opcode::GreaterThan);
                    }

                    x => {
                        self.error.abort(
                            &format!("Unimplemented function call `{}`", x),
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

    fn parse_function_call(
        &self,
        token: &Token,
        iter: &mut Peekable<std::slice::Iter<'_, Token>>,
    ) -> Vec<Variable> {
        if let Some(next_token) = iter.peek() {
            if next_token.token_type != TokenType::LeftParen {
                self.error.abort(
                    "Invalid function call opening. Must start with a `(`",
                    next_token.line,
                    next_token.column,
                );
            }
            // Skip the opening parenthesis
            iter.next();
        } else {
            self.error.abort("Premature ending of statement", token.line, token.column);
        }

        // Eat up function arguments
        let mut args = vec![];
        while let Some((arg, sep)) = iter.next_tuple() {
            args.push(Variable {
                name: arg.token.clone(),
                typ: Type::Dummy,
                line: arg.line,
                column: arg.column,
            });

            if sep.token_type == TokenType::RightParen {
                // Reached end of args
                break
            }

            if sep.token_type != TokenType::Comma {
                self.error.abort("Argument separator is not a comma (`,`)", sep.line, sep.column);
            }
        }

        args
    }
}
