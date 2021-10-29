use std::collections::HashMap;

use crate::parser::{SchemaCode, SchemaWitness};
use crate::state::Constants;

pub struct DynamicTracer {
    //name: String,
    witness: SchemaWitness,
    code: SchemaCode,
    constants: Constants,
}

impl DynamicTracer {
    pub fn new(
        _name: String,
        witness: SchemaWitness,
        code: SchemaCode,
        constants: Constants,
    ) -> Self {
        DynamicTracer {
            //name,
            witness,
            code,
            constants,
        }
    }

    pub fn execute(&self) {
        let mut stack = HashMap::new();

        // Load constants
        for variable in self.constants.variables() {
            stack.insert(variable, self.constants.lookup(variable.to_string()));
        }

        // Preload stack with our witness values
        for (type_id, variable, _) in &self.witness {
            stack.insert(&variable, *type_id);
        }

        for (func_format, retvals, args, code_line) in &self.code {
            assert!(args.len() == func_format.param_types.len());

            for (variable, type_id) in args.iter().zip(func_format.param_types.iter()) {
                if !stack.contains_key(variable) {
                    println!("variable '{}' is not defined", variable);
                    panic!("{:?}", code_line);
                }

                let stack_type_id = stack.get(variable).unwrap();
                if stack_type_id != type_id {
                    println!("variable '{}' has incorrect type", variable);
                    println!("found '{:?}' but expected '{:?}'", stack_type_id, type_id);
                    panic!("{:?}", code_line);
                }

                assert!(retvals.len() == func_format.return_type_ids.len());
                for (retval, ret_id) in retvals.iter().zip(func_format.return_type_ids.iter()) {
                    // Note that later variables shadow earlier ones.
                    // We accept this.

                    stack.insert(retval, *ret_id);
                }
            }
        }
    }
}
