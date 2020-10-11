use bls12_381::Scalar;
use std::collections::HashMap;

pub mod bls_extensions;
pub mod endian;
pub mod error;
pub mod serial;
pub mod vm;
pub mod vm_serial;

pub use crate::bls_extensions::BlsStringConversion;
pub use crate::error::{Error, Result};
pub use crate::serial::{Decodable, Encodable};
pub use crate::vm::{
    AllocType, ConstraintInstruction, CryptoOperation, VariableIndex, VariableRef, ZKVMCircuit,
    ZKVirtualMachine,
};

pub type Bytes = Vec<u8>;

pub struct ZKSupervisor {
    pub name: String,
    pub vm: ZKVirtualMachine,
    params_map: HashMap<String, VariableIndex>,
    pub params: HashMap<VariableIndex, Scalar>,
    public_map: HashMap<String, VariableIndex>,
}

struct ZKProof {
    public_values: HashMap<String, Scalar>,
    //proof:
}

impl ZKSupervisor {
    // Just have a load() and save()
    // Load the contract, do the setup, save it...

    pub fn load_contract(bytes: Bytes) -> Self {
        Self {
            name: "".to_string(),
            vm: ZKVirtualMachine {
                ops: Vec::new(),
                aux: Vec::new(),
                alloc: Vec::new(),
                constraints: Vec::new(),
                params: None,
                verifying_key: None,
                constants: Vec::new(),
            },
            params_map: HashMap::new(),
            params: HashMap::new(),
            public_map: HashMap::new(),
        }
    }

    fn setup(&self) {}
    fn save_setup(&self) {}

    fn load_setup(&self) {}

    pub fn param_names(&self) -> Vec<String> {
        self.params_map.keys().cloned().collect()
    }
    pub fn set_param(&mut self, name: &str, value: Scalar) -> Result<()> {
        match self.params_map.get(name) {
            Some(index) => {
                self.params.insert(*index, value);
                Ok(())
            }
            None => Err(Error::InvalidParamName),
        }
    }

    fn prove(&self) {
        // error if params not all set

        // execute
        // prove
        // return proof and public values (Hashmap string -> scalars)
    }
    fn verify(&self) {
        // takes proof and public values
    }
}
