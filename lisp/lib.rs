use bellman::groth16;
use bls12_381::{Bls12, Scalar};
use std::collections::{HashMap, HashSet};

pub use crate::bls_extensions::BlsStringConversion;
pub use crate::error::{Error, Result};
pub use crate::serial::{Decodable, Encodable};
pub use crate::vm::{
    AllocType, ConstraintInstruction, CryptoOperation, VariableIndex, VariableRef, ZKVMCircuit,
    ZKVirtualMachine,
};

