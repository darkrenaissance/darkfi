use log::{debug, error};

use super::{memory::MemoryManipulation, vm_runtime::Env};
use crate::{crypto::nullifier::Nullifier, node::state::ProgramState};

/// Try to read a `Nullifier` from the given pointer and check if it's
/// an existing nullifier in the blockchain state machine.
pub fn nullifier_exists(env: &Env, ptr: u32, len: u32) -> i32 {
    if let Some(bytes) = env.memory.get_ref().unwrap().read(ptr, len as usize) {
        debug!(target: "wasm-runtime", "[wasm::nullifier_exists] Read bytes: {:?}", bytes);

        let nullifier = match Nullifier::from_bytes(bytes.try_into().unwrap()) {
            Some(nf) => {
                debug!(target: "wasm-runtime", "[wasm::nullifier_exists] Nullifier: {:?}", nf);
                nf
            }
            None => {
                error!(target: "wasm-runtime", "[wasm_nullifier_exists] Could not convert bytes to Nullifier");
                return -1
            }
        };

        match env.state_machine.nullifier_exists(&nullifier) {
            true => return 1,
            false => return 0,
        }
    }

    error!(target: "wasm-runtime", "[wasm::nullifier_exists] Failed to read any bytes from VM memory");
    -2
}
