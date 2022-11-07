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

use darkfi_sdk::crypto::{constants::MERKLE_DEPTH, MerkleNode, Nullifier, PublicKey};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use log::debug;

use super::state::{ProgramState, State, StateUpdate};
use crate::crypto::proof::VerifyingKey;

/// In-memory state extension for state transition validations
#[derive(Clone)]
pub struct MemoryState {
    /// Canonical state
    pub canon: State,
    /// The entire Merkle tree state (copied from `canon`)
    pub tree: BridgeTree<MerkleNode, MERKLE_DEPTH>,
    /// List of all previous and the current merkle roots.
    pub merkle_roots: Vec<MerkleNode>,
    /// Nullifiers prevent double-spending
    pub nullifiers: Vec<Nullifier>,
}

impl ProgramState for MemoryState {
    fn is_valid_cashier_public_key(&self, public: &PublicKey) -> bool {
        self.canon.is_valid_cashier_public_key(public)
    }

    fn is_valid_faucet_public_key(&self, public: &PublicKey) -> bool {
        self.canon.is_valid_faucet_public_key(public)
    }

    fn is_valid_merkle(&self, merkle_root: &MerkleNode) -> bool {
        self.merkle_roots.contains(merkle_root) || self.canon.is_valid_merkle(merkle_root)
    }

    fn nullifier_exists(&self, nullifier: &Nullifier) -> bool {
        self.nullifiers.contains(nullifier) || self.canon.nullifier_exists(nullifier)
    }

    fn mint_vk(&self) -> &VerifyingKey {
        self.canon.mint_vk()
    }

    fn burn_vk(&self) -> &VerifyingKey {
        self.canon.burn_vk()
    }
}

impl MemoryState {
    pub fn new(canon_state: State) -> Self {
        Self {
            canon: canon_state.clone(),
            tree: canon_state.tree,
            merkle_roots: vec![],
            nullifiers: vec![],
        }
    }

    pub fn apply(&mut self, update: StateUpdate) {
        debug!(target: "state_apply", "(in-memory) Extend nullifier set");
        let mut nfs = update.nullifiers.clone();
        self.nullifiers.append(&mut nfs);

        debug!(target: "state_apply", "(in-memory) Update Merkle tree and witnesses");
        for coin in update.coins {
            let node = MerkleNode::from(coin.0);
            self.tree.append(&node);
            self.merkle_roots.push(self.tree.root(0).unwrap());
        }

        debug!(target: "state_apply", "(in-memory) Finished apply() successfully.");
    }
}
