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

use std::{
    borrow::Borrow, collections::HashMap, hash::Hash, io::Result, iter::Peekable, str::Chars,
};

use super::{
    ast::{Arg, Constant, Literal, Statement, StatementType, Variable, Witness},
    constants::{ALLOWED_FIELDS, MAX_K, MAX_NS_LEN},
    error::ErrorEmitter,
    lexer::{Token, TokenType},
    LitType, Opcode, VarType,
};

/// zkas language builtin keywords.
/// These can not be used anywhere except where they are expected.
const KEYWORDS: [&str; 5] = ["k", "field", "constant", "witness", "circuit"];

/// Forbidden namespaces
const NOPE_NS: [&str; 4] = [".constant", ".literal", ".witness", ".circuit"];

/// Valid constant types and their allowed names.
const CONSTANT_TYPES: &[(&str, VarType, &[&str])] = &[
    ("EcFixedPoint", VarType::EcFixedPoint, &["VALUE_COMMIT_RANDOM"]),
    ("EcFixedPointShort", VarType::EcFixedPointShort, &["VALUE_COMMIT_VALUE"]),
    ("EcFixedPointBase", VarType::EcFixedPointBase, &["VALUE_COMMIT_RANDOM_BASE", "NULLIFIER_K"]),
];

#[derive(Clone)]
struct IndexMap<K, V> {
    pub order: Vec<K>,
    pub map: HashMap<K, V>,
}

impl<K, V> IndexMap<K, V> {
    fn new() -> Self {
        Self { order: vec![], map: HashMap::new() }
    }
}

impl<K, V> IndexMap<K, V>
where
    K: Eq + Hash + Send + Sync + Clone + 'static,
    V: Send + Sync + Clone + 'static,
{
    fn contains_key<Q: Hash + Eq + ?Sized>(&self, k: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.map.contains_key(k)
    }

    fn get<Q: Hash + Eq + ?Sized>(&self, k: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.map.get(k)
    }

    fn insert(&mut self, k: K, v: V) -> Option<V> {
        self.order.push(k.clone());
        self.map.insert(k, v)
    }

    fn scam_iter(&self) -> Vec<(K, V)> {
        self.order.iter().map(|k| (k.clone(), self.get(k).unwrap().clone())).collect()
    }
}

// Valid witness types
impl TryFrom<&Token> for VarType {
    type Error = String;

    fn try_from(token: &Token) -> std::result::Result<Self, String> {
        match token.token.as_str() {
            "EcPoint" => Ok(Self::EcPoint),
            "EcNiPoint" => Ok(Self::EcNiPoint),
            "Base" => Ok(Self::Base),
            "Scalar" => Ok(Self::Scalar),
            "MerklePath" => Ok(Self::MerklePath),
            "SparseMerklePath" => Ok(Self::SparseMerklePath),
            "Uint32" => Ok(Self::Uint32),
            "Uint64" => Ok(Self::Uint64),
            x => Err(format!("{x} is an unsupported witness type")),
        }
    }
}

pub struct Parser {
    tokens: Vec<Token>,
    error: ErrorEmitter,
}

type Parsed = (String, u32, Vec<Constant>, Vec<Witness>, Vec<Statement>);

/// Intermediate structure to hold parsed section tokens.
/// The tokens gathered from each of the sections are stored here
/// before being converted into AST nodes.
struct SectionTokens {
    constant: Vec<Token>,
    witness: Vec<Token>,
    circuit: Vec<Token>,
}

impl SectionTokens {
    fn new() -> Self {
        Self { constant: vec![], witness: vec![], circuit: vec![] }
    }
}

impl Parser {
    pub fn new(filename: &str, source: Chars, tokens: Vec<Token>) -> Self {
        // For nice error reporting, we'll load everything into a string
        // vector so we have references to lines.
        let lines: Vec<String> = source.as_str().lines().map(|x| x.to_string()).collect();
        let error = ErrorEmitter::new("Parser", filename, lines);

        Self { tokens, error }
    }

    pub fn parse(&self) -> Result<Parsed> {
        if self.tokens.is_empty() {
            return Err(self.error.abort("Source file does not contain any valid tokens.", 0, 0))
        }

        if self.tokens[0].token_type != TokenType::Symbol {
            return Err(self.error.abort(
                "Source file does not start with a section. Expected `constant/witness/circuit`.",
                0,
                0,
            ))
        }

        let mut iter = self.tokens.iter();

        // Parse header (k and field declarations)
        let declared_k = self.parse_header(&mut iter)?;

        // Parse all sections and collect their tokens
        let (namespace, section_tokens) = self.parse_sections(&mut iter)?;

        // Build AST from section tokens
        let constants = self.build_constants(&section_tokens.constant)?;
        let witnesses = self.build_witnesses(&section_tokens.witness)?;
        let statements = self.parse_ast_circuit(&section_tokens.circuit)?;

        if statements.is_empty() {
            return Err(self.error.abort("Circuit section is empty.", 0, 0))
        }

        Ok((namespace, declared_k, constants, witnesses, statements))
    }

    /// Parse the file header: k=N; field="...";
    ///
    /// The first thing that has to be declared in the source code is the
    /// constant "k" which defines 2^k rows that the circuit needs to
    /// successfully execute.
    ///
    /// Then we declare the field we're working in.
    fn parse_header<'a>(&self, iter: &mut impl Iterator<Item = &'a Token>) -> Result<u32> {
        let Some((k, equal, number, semicolon)) = Self::next_tuple4(iter) else {
            return Err(self.error.abort("Source file does not start with k=n;", 0, 0))
        };

        self.expect_token_type(k, TokenType::Symbol)?;
        self.expect_token_type(equal, TokenType::Assign)?;
        self.expect_token_type(number, TokenType::Number)?;
        self.expect_token_type(semicolon, TokenType::Semicolon)?;

        if k.token != "k" {
            return Err(self.error.abort("Source file does not start with k=n;", k.line, k.column))
        }

        // Ensure that the value for k can be parsed correctly into the token type.
        let declared_k: u32 = number.token.parse().map_err(|e| {
            self.error.abort(
                &format!("k param is invalid, max allowed is {MAX_K}. Error: {e}"),
                number.line,
                number.column,
            )
        })?;

        if declared_k > MAX_K {
            return Err(self.error.abort(
                &format!("k param is too high, max allowed is {MAX_K}"),
                number.line,
                number.column,
            ))
        }

        // Parse field declaration
        let Some((field, equal, field_name, semicolon)) = Self::next_tuple4(iter) else {
            return Err(self.error.abort("Source file does not declare field after k", 0, 0))
        };

        self.expect_token_type(field, TokenType::Symbol)?;
        self.expect_token_type(equal, TokenType::Assign)?;
        self.expect_token_type(field_name, TokenType::String)?;
        self.expect_token_type(semicolon, TokenType::Semicolon)?;

        if field.token != "field" {
            return Err(self.error.abort(
                "Source file does not declare field after k",
                field.line,
                field.column,
            ))
        }

        if !ALLOWED_FIELDS.contains(&field_name.token.as_str()) {
            return Err(self.error.abort(
                &format!(
                    "Declared field \"{}\" is not supported. Use any of: {ALLOWED_FIELDS:?}",
                    field_name.token
                ),
                field_name.line,
                field_name.column,
            ))
        }

        Ok(declared_k)
    }

    /// Parse all sections (constant, witness, circuit) and return their tokens.
    ///
    /// Sections "constant", "witness", and "circuit" are the sections we must
    /// be declaring in our source code. When we find one, we'll take all the
    /// tokens found in the section and place them in their respective vec.
    ///
    /// NOTE: Currently this logic depends on the fact that the sections are
    /// closed off with braces. This should be revisited later when we decide
    /// to add other lang functionality that also depends on using braces.
    fn parse_sections<'a>(
        &self,
        iter: &mut impl Iterator<Item = &'a Token>,
    ) -> Result<(String, SectionTokens)> {
        let mut sections = SectionTokens::new();
        let mut namespace: Option<String> = None;
        let mut declared = (false, false, false); // constant, witness, circuit

        while let Some(t) = iter.next() {
            let section_tokens = match t.token.as_str() {
                "constant" => {
                    if declared.0 {
                        return Err(self.error.abort(
                            "Duplicate `constant` section found.",
                            t.line,
                            t.column,
                        ))
                    }
                    declared.0 = true;
                    &mut sections.constant
                }
                "witness" => {
                    if declared.1 {
                        return Err(self.error.abort(
                            "Duplicate `witness` section found.",
                            t.line,
                            t.column,
                        ))
                    }
                    declared.1 = true;
                    &mut sections.witness
                }
                "circuit" => {
                    if declared.2 {
                        return Err(self.error.abort(
                            "Duplicate `circuit` section found.",
                            t.line,
                            t.column,
                        ))
                    }
                    declared.2 = true;
                    &mut sections.circuit
                }
                x => {
                    return Err(self.error.abort(
                        &format!("Section `{x}` is not a valid section"),
                        t.line,
                        t.column,
                    ))
                }
            };

            // Absorb all tokens until closing brace
            self.absorb_section_tokens(iter, section_tokens)?;

            // Validate and extract namespace
            namespace =
                Some(self.validate_section_namespace(&t.token, section_tokens, namespace)?);
        }

        let ns =
            namespace.ok_or_else(|| self.error.abort("Missing namespace in .zk source.", 0, 0))?;

        if !declared.0 {
            return Err(self.error.abort("Missing `constant` section in .zk source.", 0, 0))
        }
        if !declared.1 {
            return Err(self.error.abort("Missing `witness` section in .zk source.", 0, 0))
        }
        if !declared.2 {
            return Err(self.error.abort("Missing `circuit` section in .zk source.", 0, 0))
        }

        Ok((ns, sections))
    }

    /// Absorb tokens from iterator until a closing brace is found.
    /// Validates that no keywords are used in improper places.
    fn absorb_section_tokens<'a>(
        &self,
        iter: &mut impl Iterator<Item = &'a Token>,
        dest: &mut Vec<Token>,
    ) -> Result<()> {
        for inner in iter {
            if KEYWORDS.contains(&inner.token.as_str()) && inner.token_type == TokenType::Symbol {
                return Err(self.error.abort(
                    &format!("Keyword '{}' used in improper place.", inner.token),
                    inner.line,
                    inner.column,
                ))
            }

            dest.push(inner.clone());
            if inner.token_type == TokenType::RightBrace {
                break
            }
        }
        Ok(())
    }

    /// Validate namespace consistency across sections.
    /// All sections must use the same namespace, and it must not be a reserved name.
    fn validate_section_namespace(
        &self,
        section_name: &str,
        tokens: &[Token],
        existing_ns: Option<String>,
    ) -> Result<String> {
        if tokens.is_empty() {
            return Err(self.error.abort(&format!("Section `{section_name}` has no tokens"), 0, 0))
        }

        let ns_token = &tokens[0];

        if let Some(ns) = existing_ns {
            if ns != ns_token.token {
                return Err(self.error.abort(
                    &format!("Found '{}' namespace, expected '{ns}'.", ns_token.token),
                    ns_token.line,
                    ns_token.column,
                ))
            }
            return Ok(ns)
        }

        if NOPE_NS.contains(&ns_token.token.as_str()) {
            return Err(self.error.abort(
                &format!("'{}' cannot be a namespace.", ns_token.token),
                ns_token.line,
                ns_token.column,
            ))
        }

        if ns_token.token.len() > MAX_NS_LEN {
            return Err(self.error.abort(
                &format!("Namespace too long, max {MAX_NS_LEN} bytes"),
                ns_token.line,
                ns_token.column,
            ))
        }

        Ok(ns_token.token.clone())
    }

    /// Build constants from section tokens.
    /// Validates constant types against the CONSTANT_TYPES table.
    fn build_constants(&self, tokens: &[Token]) -> Result<Vec<Constant>> {
        self.check_section_structure("constant", tokens)?;

        let parsed = self.parse_typed_section("constant", tokens)?;
        let mut ret = vec![];

        // name = constant name
        for (name, (name_token, type_token)) in parsed.scam_iter() {
            self.validate_section_entry("Constant", &name, &name_token, &type_token)?;

            // Look up the constant type in our table
            let type_name = type_token.token.as_str();
            let constant_def = CONSTANT_TYPES.iter().find(|(t, _, _)| *t == type_name);

            match constant_def {
                Some((_, var_type, valid_names)) => {
                    if !valid_names.contains(&name_token.token.as_str()) {
                        return Err(self.error.abort(
                            &format!(
                                "`{}` is not a valid {type_name} constant. Supported: {valid_names:?}",
                                name_token.token
                            ),
                            name_token.line,
                            name_token.column,
                        ))
                    }

                    ret.push(Constant {
                        name: name.to_string(),
                        typ: *var_type,
                        line: type_token.line,
                        column: type_token.column,
                    });
                }
                None => {
                    return Err(self.error.abort(
                        &format!("`{type_name}` is an unsupported constant type."),
                        type_token.line,
                        type_token.column,
                    ))
                }
            }
        }

        Ok(ret)
    }

    /// Build witnesses from section tokens.
    fn build_witnesses(&self, tokens: &[Token]) -> Result<Vec<Witness>> {
        self.check_section_structure("witness", tokens)?;

        let parsed = self.parse_typed_section("witness", tokens)?;
        let mut ret = vec![];

        // name = witness name
        for (name, (name_token, type_token)) in parsed.scam_iter() {
            self.validate_section_entry("Witness", &name, &name_token, &type_token)?;

            match VarType::try_from(&type_token) {
                Ok(typ) => {
                    ret.push(Witness {
                        name: name.to_string(),
                        typ,
                        line: name_token.line,
                        column: name_token.column,
                    });
                }
                Err(e) => return Err(self.error.abort(&e, type_token.line, type_token.column)),
            }
        }

        Ok(ret)
    }

    /// Parse a typed section (constant or witness) into an IndexMap.
    /// Both sections have the same structure: pairs of '<Type> <n>' separated by commas.
    fn parse_typed_section(
        &self,
        section_name: &str,
        tokens: &[Token],
    ) -> Result<IndexMap<String, (Token, Token)>> {
        let mut result = IndexMap::new();

        // Skip namespace and braces: tokens[0] is namespace, tokens[1] is {, last is }
        // This is everything between the braces: { ... }
        let inner_tokens = &tokens[2..tokens.len() - 1];
        let mut iter = inner_tokens.iter();

        while let Some((typ, name, comma)) = Self::next_tuple3(&mut iter) {
            if comma.token_type != TokenType::Comma {
                return Err(self.error.abort("Separator is not a comma.", comma.line, comma.column))
            }

            // No variable shadowing
            if result.contains_key(name.token.as_str()) {
                return Err(self.error.abort(
                    &format!(
                        "Section `{section_name}` already contains the token `{}`.",
                        &name.token
                    ),
                    name.line,
                    name.column,
                ))
            }

            result.insert(name.token.clone(), (name.clone(), typ.clone()));
        }

        if iter.next().is_some() {
            return Err(self.error.abort(
                &format!("Internal error, leftovers in '{section_name}' iterator"),
                0,
                0,
            ))
        }

        Ok(result)
    }

    /// Common validation for constant/witness entries.
    /// Ensures name and type tokens are symbols and match expected values.
    fn validate_section_entry(
        &self,
        section_type: &str,
        name: &str,
        name_token: &Token,
        type_token: &Token,
    ) -> Result<()> {
        if name_token.token != name {
            return Err(self.error.abort(
                &format!(
                    "{section_type} name `{}` doesn't match token `{name}`.",
                    name_token.token
                ),
                name_token.line,
                name_token.column,
            ))
        }

        if name_token.token_type != TokenType::Symbol {
            return Err(self.error.abort(
                &format!("{section_type} name `{}` is not a symbol.", name_token.token),
                name_token.line,
                name_token.column,
            ))
        }

        if type_token.token_type != TokenType::Symbol {
            return Err(self.error.abort(
                &format!("{section_type} type `{}` is not a symbol.", type_token.token),
                type_token.line,
                type_token.column,
            ))
        }

        Ok(())
    }

    /// Routine checks on section structure.
    /// Validates that sections have proper opening/closing braces and correct element counts.
    fn check_section_structure(&self, section: &str, tokens: &[Token]) -> Result<()> {
        // Offsets 0 and 1 are accessed directly below, so we need a length of at
        // least 2 in order to avoid an index-out-of-bounds panic.
        if tokens.len() < 2 {
            return Err(self.error.abort("Insufficient number of tokens in section.", 0, 0))
        }
        if tokens[0].token_type != TokenType::String {
            return Err(self.error.abort(
                "Section declaration must start with a naming string.",
                tokens[0].line,
                tokens[0].column,
            ))
        }

        if tokens[1].token_type != TokenType::LeftBrace {
            return Err(self.error.abort(
                "Section must be opened with a left brace '{'",
                tokens[0].line,
                tokens[0].column,
            ))
        }

        if tokens.last().unwrap().token_type != TokenType::RightBrace {
            return Err(self.error.abort(
                "Section must be closed with a right brace '}'",
                tokens[0].line,
                tokens[0].column,
            ))
        }

        match section {
            "constant" | "witness" => {
                if tokens.len() == 3 {
                    self.error.warn(&format!("{section} section is empty."), 0, 0);
                }

                if !tokens[2..tokens.len() - 1].len().is_multiple_of(3) {
                    return Err(self.error.abort(
                        &format!("Invalid number of elements in '{section}' section. Must be pairs of '<Type> <n>' separated with a comma ','."),
                        tokens[0].line,
                        tokens[0].column
                    ))
                }
            }
            "circuit" => {
                if tokens.len() == 3 {
                    return Err(self.error.abort("circuit section is empty.", 0, 0))
                }

                if tokens[tokens.len() - 2].token_type != TokenType::Semicolon {
                    return Err(self.error.abort(
                        "Circuit section does not end with a semicolon. Would never finish parsing.",
                        tokens[tokens.len()-2].line,
                        tokens[tokens.len()-2].column,
                    ))
                }
            }
            _ => unreachable!(),
        };

        Ok(())
    }

    /// Parse the circuit section into statements.
    ///
    /// The statement layouts/syntax in the language are as follows:
    ///
    /// ```text
    /// C = poseidon_hash(pub_x, pub_y, value, token, serial);
    /// | |          |                   |       |
    /// V V          V                   V       V
    /// variable    opcode              arg     arg
    /// assign
    ///
    ///                    constrain_instance(C);
    ///                       |               |
    ///                       V               V
    ///                     opcode           arg
    ///
    ///                                              inner opcode arg
    ///                                               |
    ///                  constrain_instance(ec_get_x(foo));
    ///                        |                 |
    ///                        V                 V
    ///                     opcode          arg as opcode
    /// ```
    ///
    /// In the latter, we want to support nested function calls, e.g.:
    ///
    /// ```text
    /// constrain_instance(ec_get_x(token_commit));
    /// ```
    ///
    /// The inner call's result would still get pushed on the heap,
    /// but it will not be accessible in any other scope.
    ///
    /// In certain opcodes, we also support literal types, and the
    /// opcodes can return a variable type after running the operation.
    /// e.g.
    /// ```text
    /// one = witness_base(1);
    /// zero = witness_base(0);
    /// ```
    ///
    /// The literal type is used only in the function call's scope, but
    /// the result is then accessible on the heap to be used by further
    /// computation.
    ///
    /// Regarding multiple return values from opcodes, this is perhaps
    /// not necessary for the current language scope, as this is a low
    /// level representation. Note that it could be relatively easy to
    /// modify the parsing logic to support that here. For now we'll
    /// defer it, and if at some point we decide that the language is
    /// too expressive and noisy, we'll consider having multiple return
    /// types. It also very much depends on the type of functions/opcodes
    /// that we want to support.
    fn parse_ast_circuit(&self, tokens: &[Token]) -> Result<Vec<Statement>> {
        self.check_section_structure("circuit", tokens)?;

        // Split circuit tokens into statements (delimited by semicolons).
        // Here, our statements tokens have been parsed and delimited by
        // semicolons (;) in the source file. This iterator contains each
        // of those statements as an array of tokens we then consume and
        // build the AST further.
        let mut circuit_stmts: Vec<Vec<Token>> = vec![];
        let mut current_stmt: Vec<Token> = vec![];

        for token in tokens[2..tokens.len() - 1].iter() {
            if token.token_type == TokenType::Semicolon {
                // Push completed statement to the heap
                circuit_stmts.push(current_stmt);
                current_stmt = vec![];
                continue
            }
            current_stmt.push(token.clone());
        }

        // Vec of statements to return from this entire parsing operation.
        let mut ret = vec![];

        for statement in circuit_stmts {
            if statement.is_empty() {
                continue
            }

            self.validate_statement_brackets(&statement)?;

            // Peekable iterator so we can see tokens in advance
            // without consuming the iterator.
            let mut iter = statement.iter().peekable();
            let stmt = self.parse_statement(&mut iter)?;
            ret.push(stmt);
        }

        Ok(ret)
    }

    /// Validate matching brackets in a statement.
    /// Ensures parentheses and brackets are balanced and properly nested.
    fn validate_statement_brackets(&self, statement: &[Token]) -> Result<()> {
        let (mut left_paren, mut right_paren, mut left_bracket, mut right_bracket) = (0, 0, 0, 0);

        for token in statement {
            match token.token.as_str() {
                "(" => left_paren += 1,
                ")" => right_paren += 1,
                "[" => left_bracket += 1,
                "]" => right_bracket += 1,
                _ => {}
            }
        }

        if (left_paren == 0 && right_paren == 0) && (left_bracket == 0 && right_bracket == 0) {
            return Err(self.error.abort(
                "Statement must include a function call or array initialization. No parentheses or square brackets present.",
                statement[0].line,
                statement[0].column,
            ))
        }

        if (left_bracket != right_bracket) || (left_paren != right_paren) {
            return Err(self.error.abort(
                "Parentheses or brackets are not matched.",
                statement[0].line,
                statement[0].column,
            ))
        }

        // Is there a valid use-case for defining nested arrays? For now,
        // if square brackets are present, raise an error unless there is
        // exactly one pair.
        if left_bracket > 1 {
            return Err(self.error.abort(
                "Only one pair of brackets allowed for array declaration",
                statement[0].line,
                statement[0].column,
            ))
        }

        Ok(())
    }

    /// Parse a single statement from tokens.
    /// Determines if this is an assignment (var = ...) or a direct call (opcode(...)).
    fn parse_statement(
        &self,
        iter: &mut Peekable<std::slice::Iter<'_, Token>>,
    ) -> Result<Statement> {
        // Dummy statement that we'll hopefully fill now.
        let mut stmt = Statement::default();

        let Some(token) = iter.next() else {
            return Err(self.error.abort("Empty statement", 0, 0))
        };

        // TODO: MAKE SURE IT'S A SYMBOL

        // Check if this is an assignment (var = ...) or a direct call (opcode(...))
        // This logic must be changed if we want to support multiple return values.
        if let Some(next_token) = iter.peek() {
            if next_token.token_type == TokenType::Assign {
                // Assignment statement
                stmt.line = token.line;
                stmt.typ = StatementType::Assign;
                stmt.lhs = Some(Variable {
                    name: token.token.clone(),
                    typ: VarType::Dummy,
                    line: token.line,
                    column: token.column,
                });

                // Skip over the `=` token.
                iter.next();

                // Get the opcode token
                let Some(opcode_token) = iter.next() else {
                    return Err(self.error.abort(
                        "Expected opcode after assignment",
                        token.line,
                        token.column,
                    ))
                };

                self.parse_opcode_call(opcode_token, iter, &mut stmt)?;
            } else if next_token.token_type == TokenType::LeftParen {
                // Direct call statement
                stmt.line = token.line;
                stmt.typ = StatementType::Call;
                stmt.lhs = None;

                self.parse_opcode_call(token, iter, &mut stmt)?;
            } else if next_token.token_type == TokenType::LeftBracket {
                // Array declaration.
                // TODO: Support function calls in array declarations.
                // Currently only literals can be used to construct an array.
                return Err(self.error.abort(
                    "Arrays are not implemented yet.",
                    token.line,
                    token.column,
                ))
            } else {
                return Err(self.error.abort(
                    &format!("Illegal token `{}`.", next_token.token),
                    next_token.line,
                    next_token.column,
                ))
            }
        }

        // At this stage of parsing, we should have assigned `stmt` a
        // StatementType that is not a Noop. If we have failed to do so, we
        // cannot proceed because Noops must never be passed to the compiler.
        // This can occur when multiple independent statements are passed on
        // one line, or if a statement is not terminated by a semicolon.
        if stmt.typ == StatementType::Noop {
            return Err(self.error.abort(
                "Statement is a NOOP; not allowed. (Did you miss a semicolon?)",
                token.line,
                token.column,
            ))
        }

        Ok(stmt)
    }

    /// Parse an opcode call and fill in the statement.
    /// The assumption here is that the current token is a function call,
    /// so we check if it's legit and start digging.
    fn parse_opcode_call(
        &self,
        token: &Token,
        iter: &mut Peekable<std::slice::Iter<'_, Token>>,
        stmt: &mut Statement,
    ) -> Result<()> {
        let func_name = token.token.as_str();

        // Ensure the current function is a symbol
        if token.token_type != TokenType::Symbol {
            return Err(self.error.abort("This token is not a symbol.", token.line, token.column))
        }

        if let Some(op) = Opcode::from_name(func_name) {
            let rhs = self.parse_function_call(token, iter)?;
            stmt.opcode = op;
            stmt.rhs = rhs;
            Ok(())
        } else {
            Err(self.error.abort(
                &format!("Unimplemented opcode `{func_name}`."),
                token.line,
                token.column,
            ))
        }
    }

    /// Parse a function call and its arguments.
    /// Handles nested function calls recursively, creating intermediate
    /// variables for inner call results.
    fn parse_function_call(
        &self,
        token: &Token,
        iter: &mut Peekable<std::slice::Iter<'_, Token>>,
    ) -> Result<Vec<Arg>> {
        if let Some(next_token) = iter.peek() {
            if next_token.token_type != TokenType::LeftParen {
                return Err(self.error.abort(
                    "Invalid function call opening. Must start with a '('.",
                    next_token.line,
                    next_token.column,
                ))
            }
            // Skip the opening parenthesis
            iter.next();
        } else {
            return Err(self.error.abort("Premature ending of statement.", token.line, token.column))
        }

        let mut ret = vec![];

        // The next element in the iter now hopefully contains an opcode
        // argument. If it's another opcode, we'll recurse into this
        // function's logic.
        // Otherwise, we look for variable and literal types.
        while let Some(arg) = iter.next() {
            // ============================
            // Parse a nested function call
            // ============================
            if let Some(op_inner) = Opcode::from_name(&arg.token) {
                if let Some(paren) = iter.peek() {
                    if paren.token_type != TokenType::LeftParen {
                        return Err(self.error.abort(
                            "Invalid function call opening. Must start with a '('.",
                            paren.line,
                            paren.column,
                        ))
                    }

                    // Recurse this function to get the params of the nested one.
                    let args = self.parse_function_call(arg, iter)?;

                    // Then we assign a "fake" variable that serves as a heap
                    // reference.
                    let var = Variable {
                        name: format!("_op_inner_{}_{}", arg.line, arg.column),
                        typ: VarType::Dummy,
                        line: arg.line,
                        column: arg.column,
                    };

                    let arg = Arg::Func(Statement {
                        typ: StatementType::Assign,
                        opcode: op_inner,
                        lhs: Some(var),
                        rhs: args,
                        line: arg.line,
                    });

                    ret.push(arg);
                    continue
                }

                return Err(self.error.abort(
                    "Missing tokens in statement, there's a syntax error here.",
                    arg.line,
                    arg.column,
                ))
            }

            // ==========================================
            // Parse normal argument, not a function call
            // ==========================================
            if let Some(sep) = iter.next() {
                // See if we have a variable or a literal type.
                match arg.token_type {
                    TokenType::Symbol => ret.push(Arg::Var(Variable {
                        name: arg.token.clone(),
                        typ: VarType::Dummy,
                        line: arg.line,
                        column: arg.column,
                    })),

                    TokenType::Number => {
                        // Check if we can actually convert this into a number.
                        arg.token.parse::<u64>().map_err(|e| {
                            self.error.abort(
                                &format!("Failed to convert literal into u64: {e}"),
                                arg.line,
                                arg.column,
                            )
                        })?;

                        ret.push(Arg::Lit(Literal {
                            name: arg.token.clone(),
                            typ: LitType::Uint64,
                            line: arg.line,
                            column: arg.column,
                        }))
                    }

                    TokenType::RightParen => {
                        if let Some(comma) = iter.peek() {
                            if comma.token_type == TokenType::Comma {
                                iter.next();
                            }
                        }
                        break
                    }

                    // Note: Unimplemented symbols throw an error now instead of a panic.
                    // This assists with fuzz testing as existing features can still be tested
                    // without causing the fuzzer to choke due to the panic created
                    // by unimplmented!().
                    _ => {
                        return Err(self.error.abort(
                            "Character is illegal/unimplemented in this context",
                            arg.line,
                            arg.column,
                        ))
                    }
                };

                if sep.token_type == TokenType::RightParen {
                    if let Some(comma) = iter.peek() {
                        if comma.token_type == TokenType::Comma {
                            iter.next();
                        }
                    }
                    // Reached end of args
                    break
                }

                if sep.token_type != TokenType::Comma {
                    return Err(self.error.abort(
                        "Argument separator is not a comma (`,`)",
                        sep.line,
                        sep.column,
                    ))
                }
            }
        }

        Ok(ret)
    }

    /// Check that a token has the expected type.
    fn expect_token_type(&self, token: &Token, expected: TokenType) -> Result<()> {
        if token.token_type != expected {
            return Err(self.error.abort(
                &format!("Expected {:?}, got {:?}", expected, token.token_type),
                token.line,
                token.column,
            ))
        }
        Ok(())
    }

    /// Get next 3 items from an iterator as a tuple.
    fn next_tuple3<I, T>(iter: &mut I) -> Option<(T, T, T)>
    where
        I: Iterator<Item = T>,
    {
        let a = iter.next()?;
        let b = iter.next()?;
        let c = iter.next()?;
        Some((a, b, c))
    }

    /// Get next 4 items from an iterator as a tuple.
    fn next_tuple4<I, T>(iter: &mut I) -> Option<(T, T, T, T)>
    where
        I: Iterator<Item = T>,
    {
        let a = iter.next()?;
        let b = iter.next()?;
        let c = iter.next()?;
        let d = iter.next()?;
        Some((a, b, c, d))
    }
}
