/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use darkfi_sdk::crypto::{MerkleNode, Nullifier};
use log::{debug, error};
use wasmer::{AsStoreRef, FunctionEnvMut, WasmPtr};

use crate::{
    node::state::ProgramState,
    runtime::{
        memory::MemoryManipulation,
        vm_runtime::{ContractSection, Env},
    },
};

pub(crate) fn set_update(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Exec => {
            let memory_view = env.memory_view(&ctx);

            // FIXME: make me preettty!
            let slice = ptr.slice(&memory_view, len);
            if slice.is_err() {
                return -2
            }
            let slice = slice.unwrap();

            // FIXME: make me double pretty
            // before:
            //let update_data = slice.read_to_vec();
            //if update_data.is_err() {
            //    return -2;
            //}
            //let update_data = update_data.unwrap();

            // after:
            let Ok(update_data) = slice.read_to_vec() else {
                return -2;
            };
            //

            // FIXME: Shouldn't assert here, but rather return an error.
            // An assert would make the host panic.
            assert!(env.contract_update.take().is_none());
            let func_id = update_data[0];
            let update_data = &update_data[1..];
            env.contract_update.set(Some((func_id, update_data.to_vec())));
            0
        }
        _ => -1,
    }
}

/// Try to read a `Nullifier` from the given pointer and check if it's
/// an existing nullifier in the blockchain state machine.
pub(crate) fn nullifier_exists(mut ctx: FunctionEnvMut<Env>, ptr: u32, len: u32) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Null => {
            unreachable!();
        }
        ContractSection::Deploy => {
            debug!(target: "nullifier_exists", "deploy!!!");
        }
        ContractSection::Exec => {
            debug!(target: "nullifier_exists", "exec!!!");
        }
        ContractSection::Update => {
            debug!(target: "nullifier_exists", "apply!!!");
        }
    }
    /*
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
    */

    //error!(target: "wasm_runtime::nullifier_exists", "Failed to read bytes from VM memory");
    //-2
    0
}

/// Try to read a `MerkleNode` from the given pointer and check if it's
/// a valid Merkle root in the chain's Merkle tree.
pub(crate) fn is_valid_merkle(mut ctx: FunctionEnvMut<Env>, ptr: u32, len: u32) -> i32 {
    /*
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
    */

    //error!(target: "wasm_runtime::is_valid_merkle", "Failed to read bytes from VM memory");
    //-2
    0
}
