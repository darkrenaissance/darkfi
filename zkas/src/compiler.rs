use std::collections::HashMap;

use crate::parser::{SchemaCode, SchemaWitness};
use crate::state::{Constants, Line};
use crate::types::{FuncFormat, TypeId};

#[derive(Debug, Clone)]
pub struct CodeLine {
    pub func_format: FuncFormat,
    pub return_values: Vec<String>,
    pub args: Vec<String>,
    pub arg_idxs: Vec<usize>,
    pub code_line: Line,
}

impl CodeLine {
    pub fn new(
        func_format: FuncFormat,
        return_values: Vec<String>,
        args: Vec<String>,
        arg_idxs: Vec<usize>,
        code_line: Line,
    ) -> Self {
        CodeLine {
            func_format,
            return_values,
            args,
            arg_idxs,
            code_line,
        }
    }
}

fn alloc(
    stacks: &mut Vec<Vec<String>>,
    stack_vars: &mut HashMap<String, (TypeId, usize)>,
    variable: String,
    type_id: TypeId,
) {
    assert!(type_id as usize <= stacks.len());
    let idx = stacks[type_id as usize].len();
    // Add variable to the stack for its TypeId
    stacks[type_id as usize].push(variable.clone());
    // Create mapping from variable name
    stack_vars.insert(variable, (type_id, idx));
}

pub struct Compiler {
    pub witness: SchemaWitness,
    pub uncompiled_code: SchemaCode,
    pub constants: Constants,
}

impl Compiler {
    pub fn new(witness: SchemaWitness, uncompiled_code: SchemaCode, constants: Constants) -> Self {
        Compiler {
            witness,
            uncompiled_code,
            constants,
        }
    }

    pub fn compile(&self) -> Vec<CodeLine> {
        let mut code = vec![];

        // Each unique TypeID has its own stack
        let mut stacks = vec![];
        let mut stack: Vec<String>;
        for _ in 0..TypeId::LastId as usize {
            stack = vec![];
            stacks.push(stack);
        }

        // Map from variable name to stacks above
        let mut stack_vars = HashMap::new();

        // Load constants
        for variable in self.constants.variables() {
            let type_id = self.constants.lookup(variable.to_string());
            alloc(&mut stacks, &mut stack_vars, variable.to_string(), type_id);
        }

        // Preload stack with our witness values
        for (type_id, variable, _line) in self.witness.clone() {
            alloc(&mut stacks, &mut stack_vars, variable.to_string(), type_id);
        }

        for (func_format, retvals, args, code_line) in &self.uncompiled_code {
            assert!(args.len() == func_format.param_types.len());

            let mut arg_idxs = vec![];

            // Loop through all arguments
            for (variable, type_id) in args.iter().zip(func_format.param_types.iter()) {
                assert!(*type_id as usize <= stacks.len());
                assert!(stack_vars.contains_key(variable));
                // Find the index for the M by N matrix of our variable
                let (loc_type_id, loc_idx) = stack_vars.get(variable).unwrap();
                assert!(type_id == loc_type_id);
                assert!(&stacks[*loc_type_id as usize][*loc_idx as usize] == variable);

                // This is the info to be serialized, not the variable names
                arg_idxs.push(*loc_idx);
            }

            assert!(retvals.len() == func_format.return_type_ids.len());

            for (retval, ret_id) in retvals.iter().zip(func_format.return_type_ids.iter()) {
                // Allocate returned values so they can be used by
                // subsequent function calls.
                alloc(&mut stacks, &mut stack_vars, retval.to_string(), *ret_id);
            }

            code.push(CodeLine::new(
                func_format.clone(),
                retvals.clone(),
                args.clone(),
                arg_idxs,
                code_line.clone(),
            ));
        }

        code
    }
}

#[derive(Debug, Clone)]
pub struct CompiledContract {
    pub name: String,
    pub witness: SchemaWitness,
    pub code: Vec<CodeLine>,
}

impl CompiledContract {
    pub fn new(name: String, witness: SchemaWitness, code: Vec<CodeLine>) -> Self {
        CompiledContract {
            name,
            witness,
            code,
        }
    }
}
