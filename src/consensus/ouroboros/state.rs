use incrementalmerkletree::{bridgetree::BridgeTree, Tree};

use crate::{
    crypto::{
        constants::MERKLE_DEPTH,
        merkle_node::MerkleNode,
        nullifier::Nullifier,
        util::poseidon_hash,
        note::{EncryptedNote, Note},
        coin::OwnCoin,
        proof::{VerifyingKey},
        keypair::{PublicKey, SecretKey},
    },
    node::state::{state_transition, StateUpdate},
};

pub struct StakeholderState {
    /// The entire Merkle tree state
    pub tree: BridgeTree<MerkleNode, MERKLE_DEPTH>,
    /// List of all previous and the current Merkle roots.
    /// This is the hashed value of all the children.
    pub merkle_roots: Vec<MerkleNode>,
    /// Nullifiers prevent double spending
    pub nullifiers: Vec<Nullifier>,
    /// All received coins
    // NOTE: We need maybe a flag to keep track of which ones are
    // spent. Maybe the spend field links to a tx hash:input index.
    // We should also keep track of the tx hash:output index where
    // this coin was received.
    pub own_coins: Vec<OwnCoin>,
    /// Verifying key for the mint zk circuit.
    pub mint_vk: VerifyingKey,
    /// Verifying key for the burn zk circuit.
    pub burn_vk: VerifyingKey,

    /// Public key of the cashier
    pub cashier_signature_public: PublicKey,

    /// Public key of the faucet
    pub faucet_signature_public: PublicKey,

    /// List of all our secret keys
    pub secrets: Vec<SecretKey>,
}

impl ProgramState for StakeholderState {
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

impl StakeholderState {
    pub fn apply(&mut self, mut update: StateUpdate) {
        // Extend our list of nullifiers with the ones from the update
        self.nullifiers.append(&mut update.nullifiers);

        // Update merkle tree and witnesses
        for (coin, enc_note) in update.coins.into_iter().zip(update.enc_notes.into_iter()) {
            // Add the new coins to the Merkle tree
            let node = MerkleNode(coin.0);
            self.tree.append(&node);

            // Keep track of all Merkle roots that have existed
            self.merkle_roots.push(self.tree.root(0).unwrap());

            // If it's our own coin, witness it and append to the vector.
            if let Some((note, secret)) = self.try_decrypt_note(enc_note) {
                let leaf_position = self.tree.witness().unwrap();
                let nullifier = poseidon_hash::<2>([secret.inner(), note.serial]);
                let own_coin = OwnCoin {
                    coin: coin,
                    note: note,
                    secret: secret,
                    nullifier: Nullifier::from(nullifier),
                    leaf_position: leaf_position
                };
                self.own_coins.push(own_coin);
            }
        }
    }

    fn try_decrypt_note(&self, ciphertext: EncryptedNote) -> Option<(Note, SecretKey)> {
        // Loop through all our secret keys...
        for secret in &self.secrets {
            // .. attempt to decrypt the note ...
            if let Ok(note) = ciphertext.decrypt(secret) {
                // ... and return the decrypted note for this coin.
                return Some((note, *secret))
            }
        }

        // We weren't able to decrypt the note with any of our keys.
        None
    }
}
