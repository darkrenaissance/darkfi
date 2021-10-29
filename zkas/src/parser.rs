use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io;

use crate::state::{Constants, Line};
use crate::tracer::DynamicTracer;
use crate::types::{FuncFormat, TypeId, ALLOWED_TYPES, FUNCTION_FORMATS};

pub type SchemaWitness = Vec<(TypeId, String, Line)>;
pub type SchemaCode = Vec<(FuncFormat, Vec<String>, Vec<String>, Line)>;
pub type Schema = Vec<(String, SchemaWitness, SchemaCode)>;

#[derive(Default, Debug)]
pub struct Syntax {
    pub contracts: HashMap<String, Vec<Line>>,
    pub circuits: HashMap<String, Vec<Line>>,
    pub constants: Constants,
}

impl Syntax {
    fn new() -> Self {
        Syntax {
            contracts: HashMap::new(),
            circuits: HashMap::new(),
            constants: Constants::new(),
        }
    }

    fn parse_contract(&mut self, line: Line, iter: &mut std::slice::Iter<'_, Line>) {
        assert!(line.tokens[0] == "contract");

        if line.tokens.len() != 3 || line.tokens[2] != "{" {
            println!("parser error: malformed contract opening");
            panic!("{:?}", line);
        }

        let name = line.tokens[1].clone();
        if self.contracts.contains_key(name.as_str()) {
            println!("parser error: duplicate contract '{}", name);
            panic!("{:?}", line);
        }

        let mut lines: Vec<Line> = vec![];

        loop {
            let ln = iter.next();
            if ln.is_none() {
                println!("parser error: unexpected eof parsing contract '{}'", name);
                panic!("{:?}", line);
            }
            let ln = ln.unwrap();

            assert!(!ln.tokens.is_empty());
            if ln.tokens[0] == "}" {
                break;
            }

            lines.push(ln.clone());
        }

        self.contracts.insert(name, lines);
    }

    fn parse_circuit(&mut self, line: Line, iter: &mut std::slice::Iter<'_, Line>) {
        assert!(line.tokens[0] == "circuit");

        if line.tokens.len() != 3 || line.tokens[2] != "{" {
            println!("parser error: malformed circuit opening");
            panic!("{:?}", line);
        }

        let name = line.tokens[1].clone();
        if self.circuits.contains_key(name.as_str()) {
            println!("parser error: duplicate circuit '{}'", name);
            panic!("{:?}", line);
        }

        let mut lines: Vec<Line> = vec![];

        loop {
            let ln = iter.next();
            if ln.is_none() {
                println!("parser error: unexpected eof parsing circuit '{}'", name);
                panic!("{:?}", line);
            }
            let ln = ln.unwrap();

            assert!(!ln.tokens.is_empty());
            if ln.tokens[0] == "}" {
                break;
            }

            lines.push(ln.clone());
        }

        self.circuits.insert(name, lines);
    }

    fn parse_constant(&mut self, line: Line) {
        assert!(line.tokens[0] == "constant");

        if line.tokens.len() != 3 {
            println!("parser error: malformed constant line");
            panic!("{:?}", line);
        }

        let type_name = line.tokens[1].clone();
        let variable = line.tokens[2].clone();

        if !ALLOWED_TYPES.contains_key(type_name.as_str()) {
            println!("parser error: unknown type '{}'", type_name);
            panic!("{:?}", line);
        }

        if let Some(type_id) = ALLOWED_TYPES.get(type_name.as_str()) {
            self.constants.add(variable, *type_id);
            return;
        }

        unreachable!();
    }

    fn static_checks(&self) {
        for (_, lines) in self.contracts.iter() {
            for line in lines {
                if line.tokens.len() != 2 {
                    println!("parser error: incorrect number of tokens");
                    panic!("{:?}", line);
                }

                let type_name = &line.tokens[0];
                let variable = &line.tokens[1];

                if !ALLOWED_TYPES.contains_key(type_name.as_str()) {
                    println!("parser error: unknown type for variable '{}'", variable);
                    panic!("{:?}", line);
                }
            }
        }

        for (_, lines) in self.circuits.iter() {
            for line in lines {
                assert!(!line.tokens.is_empty());
                let func_name = &line.tokens[0];
                let args = &line.tokens[1..];

                if !FUNCTION_FORMATS.contains_key(func_name.as_str()) {
                    println!("parser error: unknown function call '{}'", func_name);
                    panic!("{:?}", line);
                }

                let func_format = FUNCTION_FORMATS.get(func_name.as_str()).unwrap();

                if args.len() != func_format.total_arguments() {
                    println!(
                        "parser error: incorrect num of args for '{}' function call",
                        func_name
                    );
                    panic!("{:?}", line);
                }
            }
        }

        // Finally check there are matching circuits and contracts.
        let circuits: HashSet<_> = self.circuits.keys().collect();
        let contracts: HashSet<_> = self.contracts.keys().collect();
        for n in circuits.union(&contracts) {
            if !self.contracts.contains_key(*n) {
                panic!("missing contract for '{}'", n);
            }
            if !self.circuits.contains_key(*n) {
                panic!("missing circuit for '{}'", n);
            }
        }
    }

    fn format_data(&self) -> Schema {
        let mut schema = vec![];

        for (name, circuit) in self.circuits.iter() {
            assert!(self.contracts.contains_key(name));
            let contract = self.contracts.get(name).unwrap();

            let mut witness = vec![];
            for line in contract {
                assert!(line.tokens.len() == 2);
                let type_name = &line.tokens[0];
                let variable = &line.tokens[1];
                assert!(ALLOWED_TYPES.contains_key(type_name.as_str()));
                let type_id = ALLOWED_TYPES.get(type_name.as_str()).unwrap();
                witness.push((*type_id, variable.to_string(), line.clone()));
            }

            let mut code = vec![];
            for line in circuit {
                assert!(!line.tokens.is_empty());
                let func_name = &line.tokens[0];
                let mut args = &line.tokens[1..];
                assert!(FUNCTION_FORMATS.contains_key(func_name.as_str()));
                let func_format = FUNCTION_FORMATS.get(func_name.as_str()).unwrap();
                assert!(args.len() == func_format.total_arguments());

                let mut retvals = vec![];
                if !func_format.return_type_ids.is_empty() {
                    let rv_len = func_format.return_type_ids.len();
                    retvals.extend_from_slice(&args[..rv_len]);
                    args = &args[rv_len..];
                }

                // let _func_id = func_format.func_id;
                code.push((func_format.clone(), retvals, args.to_vec(), line.clone()));
            }

            schema.push((name.clone(), witness, code));
        }

        schema
    }

    fn trace_circuits(&self, schema: Schema) {
        for (name, witness, code) in schema {
            let tracer = DynamicTracer::new(name, witness, code, self.constants.clone());
            tracer.execute();
        }
    }

    pub fn verify(&self) -> Schema {
        self.static_checks();
        let schema = self.format_data();
        self.trace_circuits(schema.clone());
        schema
    }
}

pub fn load_lines(lines: io::Lines<io::BufReader<File>>) -> Vec<Line> {
    let mut source = vec![];

    for (n, original_line) in lines.enumerate() {
        if let Ok(ogline) = original_line {
            let line_number = n + 1;

            // Remove whitespace on both sides
            let orig = ogline.clone();
            let line = orig.trim_start().trim_end();

            // Strip out comments
            let spl: Vec<&str> = line.split('#').collect();
            if spl[0].is_empty() {
                continue;
            }

            // Split at whitespace
            let spl: Vec<String> = spl[0].split(' ').map(|s| s.to_string()).collect();

            source.push(Line::new(spl, line.to_string(), line_number as u32));
        }
    }

    source
}

pub fn parse_lines(lines: Vec<Line>) -> Syntax {
    let mut syntax = Syntax::new();
    let mut iter = lines.iter();

    loop {
        let line = iter.next();
        if line.is_none() {
            break;
        }

        let line = line.unwrap();
        assert!(!line.tokens.is_empty());

        match line.tokens[0].as_str() {
            "contract" => syntax.parse_contract(line.clone(), &mut iter),
            "circuit" => syntax.parse_circuit(line.clone(), &mut iter),
            "constant" => syntax.parse_constant(line.clone()),
            "}" => {
                println!("unmatched delimiter '}}'");
                panic!("{:?}", line);
            }
            _ => unreachable!(),
        }
    }

    syntax
}
