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

use darkfi_sdk::{
    crypto::{MerkleNode, Nullifier},
    entrypoint,
    error::ContractResult,
    incrementalmerkletree::bridgetree::BridgeTree,
};
use darkfi_serial::{deserialize, SerialDecodable, SerialEncodable};

/// Available functions for this contract.
/// We identify them with the first byte passed in through the payload.
#[repr(u8)]
pub enum Function {
    Transfer = 0x00,
}

impl From<u8> for Function {
    fn from(b: u8) -> Self {
        match b {
            0x00 => Self::Transfer,
            _ => panic!("Invalid function ID: {:#04x?}", b),
        }
    }
}

pub mod transfer;

/// `State` represents this contract's state on-chain. The contract's
/// entrypoint knows its own `ContractId` since it's passed in by the
/// wasm runtime, so it knows what to request. Retrieval of the state
/// from the blockchain is done with a host function called `lookup_state`.
/// For more info, see:
/// * `darkfi/src/blockchain/statestore.rs`
/// * ~~~`darkfi/src/runtime/chain_state.rs`~~~
#[repr(C)]
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct State {
    /// The Merkle tree of all coins used by this contract.
    pub tree: BridgeTree<MerkleNode, 32>,
    /// List of all previous and current Merkle roots.
    pub merkle_roots: Vec<MerkleNode>,
    /// Published nullifiers that have been seen.
    pub nullifiers: Vec<Nullifier>,
}

impl State {
    pub fn is_valid_merkle(&self, merkle_root: &MerkleNode) -> bool {
        self.merkle_roots.iter().any(|m| m == merkle_root)
    }

    pub fn nullifier_exists(&self, nullifier: &Nullifier) -> bool {
        self.nullifiers.iter().any(|n| n == nullifier)
    }
}

#[cfg(not(feature = "no-entrypoint"))]
entrypoint!(process_instruction);
fn process_instruction(state: &[u8], ix: &[u8]) -> ContractResult {
    // This is the entrypoint function of the smart contract which gets executed
    // by the wasm runtime. The `contract_id` passed in is used to lookup the
    // current state from the ledger using the `lookup_state` function.
    // `ix` is an arbitrary payload fed into the contract. In this case, the
    // first byte of the payload is a pointer to a function we with to run, and
    // the remainter is a serialized `Transaction` object we'll try to deserialize
    // and work with.
    let mut state: State = deserialize(state)?;

    match Function::from(ix[0]) {
        Function::Transfer => {
            let transaction = deserialize(&ix[1..])?;
            transfer::exec(&mut state, transaction)?;
            // If `transfer` succeeded, `state` will contain the updated state, so
            // we can change it in the VM environment which is accessible by the
            // host. Then if everything else outside of the wasm execution is
            // valid, the host can reference this new state and update it on the
            // ledger.
            //apply_state(&serialize(&state))?;
        }
    }

    Ok(())
}
