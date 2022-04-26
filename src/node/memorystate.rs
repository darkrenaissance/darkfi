use incrementalmerkletree::{bridgetree::BridgeTree, Frontier};
use log::debug;

use super::state::{ProgramState, State, StateUpdate};
use crate::crypto::{
    keypair::PublicKey, merkle_node::MerkleNode, nullifier::Nullifier, proof::VerifyingKey,
};

/// In-memory state extension for state transition validations
pub struct MemoryState {
    /// Canonical state
    pub canon: State,
    /// The entire Merkle tree state (copied from `canon`)
    pub tree: BridgeTree<MerkleNode, 32>,
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
        self.canon.is_valid_merkle(merkle_root) || self.merkle_roots.contains(merkle_root)
    }

    fn nullifier_exists(&self, nullifier: &Nullifier) -> bool {
        self.canon.nullifier_exists(nullifier) || self.nullifiers.contains(nullifier)
    }

    fn mint_vk(&self) -> &VerifyingKey {
        self.canon.mint_vk()
    }

    fn burn_vk(&self) -> &VerifyingKey {
        self.canon.burn_vk()
    }
}

impl MemoryState {
    pub async fn apply(&mut self, update: StateUpdate) {
        debug!(target: "state_apply", "(in-memory) Extend nullifier set");
        let mut nfs = update.nullifiers.clone();
        self.nullifiers.append(&mut nfs);

        debug!(target: "state_apply", "(in-memory) Update Merkle tree and witnesses");
        for coin in update.coins {
            let node = MerkleNode(coin.0);
            self.tree.append(&node);
            self.merkle_roots.push(self.tree.root());
        }

        debug!(target: "state_apply", "(in-memory) Finished apply() successfully.");
    }
}
