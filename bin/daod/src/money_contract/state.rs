use incrementalmerkletree::{bridgetree::BridgeTree, Tree};

use darkfi::{
    crypto::{
        constants::MERKLE_DEPTH, keypair::PublicKey, merkle_node::MerkleNode, nullifier::Nullifier,
        proof::VerifyingKey,
    },
    node::state::{ProgramState, StateUpdate},
};

/// The state machine, held in memory.
pub struct State {
    /// The entire Merkle tree state
    pub tree: BridgeTree<MerkleNode, MERKLE_DEPTH>,
    /// List of all previous and the current Merkle roots.
    /// This is the hashed value of all the children.
    pub merkle_roots: Vec<MerkleNode>,
    /// Nullifiers prevent double spending
    pub nullifiers: Vec<Nullifier>,
    /// Verifying key for the mint zk circuit.
    pub mint_vk: VerifyingKey,
    /// Verifying key for the burn zk circuit.
    pub burn_vk: VerifyingKey,

    /// Public key of the cashier
    pub cashier_signature_public: PublicKey,

    /// Public key of the faucet
    pub faucet_signature_public: PublicKey,
}

impl ProgramState for State {
    fn is_valid_cashier_public_key(&self, public: &PublicKey) -> bool {
        public == &self.cashier_signature_public
    }

    fn is_valid_faucet_public_key(&self, public: &PublicKey) -> bool {
        public == &self.faucet_signature_public
    }

    fn is_valid_merkle(&self, merkle_root: &MerkleNode) -> bool {
        self.merkle_roots.iter().any(|m| m == merkle_root)
    }

    fn nullifier_exists(&self, nullifier: &Nullifier) -> bool {
        self.nullifiers.iter().any(|n| n == nullifier)
    }

    fn mint_vk(&self) -> &VerifyingKey {
        &self.mint_vk
    }

    fn burn_vk(&self) -> &VerifyingKey {
        &self.burn_vk
    }
}
