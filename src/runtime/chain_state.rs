use darkfi_sdk::crypto::{MerkleNode, Nullifier};
use log::{debug, error};

use super::{memory::MemoryManipulation, vm_runtime::Env};
use crate::node::state::ProgramState;

/// Try to read a `Nullifier` from the given pointer and check if it's
/// an existing nullifier in the blockchain state machine.
pub fn nullifier_exists(env: &Env, ptr: u32, len: u32) -> i32 {
    if let Some(bytes) = env.memory.get_ref().unwrap().read(ptr, len as usize) {
        debug!(target: "wasm_runtime::nullifier_exists", "Read bytes: {:?}", bytes);

        let nullifier = match Nullifier::from_bytes(bytes.try_into().unwrap()) {
            Some(nf) => {
                debug!(target: "wasm_runtime::nullifier_exists", "Nullifier: {:?}", nf);
                nf
            }
            None => {
                error!(target: "wasm_runtime::nullifier_exists", "Could not convert bytes to Nullifier");
                return -1
            }
        };

        match env.state_machine.nullifier_exists(&nullifier) {
            true => return 1,
            false => return 0,
        }
    }

    error!(target: "wasm_runtime::nullifier_exists", "Failed to read bytes from VM memory");
    -2
}

/// Try to read a `MerkleNode` from the given pointer and check if it's
/// a valid Merkle root in the chain's Merkle tree.
pub fn is_valid_merkle(env: &Env, ptr: u32, len: u32) -> i32 {
    if let Some(bytes) = env.memory.get_ref().unwrap().read(ptr, len as usize) {
        debug!(target: "wasm_runtime::is_valid_merkle", "Read bytes: {:?}", bytes);

        let merkle_node = match MerkleNode::from_bytes(bytes.try_into().unwrap()) {
            Some(mn) => {
                debug!(target: "wasm_runtime::is_valid_merkle", "MerkleNode: {:?}", mn);
                mn
            }
            None => {
                error!(target: "wasm_runtime::is_valid_merkle", "Could not convert bytes to MerkleNode");
                return -1
            }
        };

        match env.state_machine.is_valid_merkle(&merkle_node) {
            true => return 1,
            false => return 0,
        }
    }

    error!(target: "wasm_runtime::is_valid_merkle", "Failed to read bytes from VM memory");
    -2
}
