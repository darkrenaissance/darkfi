use incrementalmerkletree::{bridgetree::BridgeTree, Frontier, Tree};
use rand::rngs::OsRng;

use darkfi::{
    crypto::{
        coin::Coin,
        keypair::{Keypair, PublicKey, SecretKey},
        merkle_node::MerkleNode,
        note::{EncryptedNote, Note},
        nullifier::Nullifier,
        proof::{ProvingKey, VerifyingKey},
        token_id::generate_id2,
    },
    node::state::{state_transition, ProgramState, StateUpdate},
    tx,
    util::NetworkName,
    zk::circuit::{mint_contract::MintContract, spend_contract::SpendContract},
    Result,
};

struct MemoryState {
    // The entire merkle tree state
    tree: BridgeTree<MerkleNode, 32>,
    // List of all previous and the current merkle roots
    // This is the hashed value of all the children.
    merkle_roots: Vec<MerkleNode>,
    // Nullifiers prevent double spending
    nullifiers: Vec<Nullifier>,
    // All received coins
    // NOTE: we need maybe a flag to keep track of which ones are spent
    // Maybe the spend field links to a tx hash:input index
    // We should also keep track of the tx hash:output index where this
    // coin was received
    own_coins: Vec<(Coin, Note)>,
    mint_vk: VerifyingKey,
    spend_vk: VerifyingKey,

    // Public key of the cashier
    cashier_signature_public: PublicKey,
    // List of all our secret keys
    secrets: Vec<SecretKey>,
}

impl ProgramState for MemoryState {
    fn is_valid_cashier_public_key(&self, public: &PublicKey) -> bool {
        public == &self.cashier_signature_public
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

    fn spend_vk(&self) -> &VerifyingKey {
        &self.spend_vk
    }
}

impl MemoryState {
    fn apply(&mut self, mut update: StateUpdate) {
        // Extend our list of nullifiers with the ones from the update
        self.nullifiers.append(&mut update.nullifiers);

        // Update merkle tree and witnesses
        for (coin, enc_note) in update.coins.into_iter().zip(update.enc_notes.into_iter()) {
            // Add the new coins to the merkle tree
            let node = MerkleNode(coin.0);
            self.tree.append(&node);

            // Keep track of all merkle roots that have existed
            self.merkle_roots.push(self.tree.root());

            if let Some((note, _secret)) = self.try_decrypt_note(enc_note) {
                self.own_coins.push((coin, note));
                self.tree.witness();
            }
        }
    }

    fn try_decrypt_note(&self, ciphertext: EncryptedNote) -> Option<(Note, SecretKey)> {
        // Loop through all our secret keys...
        for secret in &self.secrets {
            // ... attempt to decrypt the note ...
            if let Ok(note) = ciphertext.decrypt(secret) {
                // ... and return the decrypted note for this coin.
                return Some((note, *secret))
            }
        }
        // We weren't able to decrypt the note with any of our keys.
        None
    }
}

pub fn demo() -> Result<()> {
    // Create the treasury token: xDRK
    //   - mint a new token supply using clear inputs
    // Create the governance token: gDRK
    //   - mint a new token supply using clear inputs
    // Create the DAO instance
    //   - create proposal auth keypair
    //   - mint a new bulla:
    //
    //       DAO {
    //           proposal_auth_key
    //           gov_token_id
    //           treasury_token_id
    //       }
    //
    // Receive payment to DAO treasury
    //   - send token to a coin that has:
    //     - parent set to DAO bulla
    //     - owner set to contract:function unique address (checked by consensus)
    // Create a proposal
    // Proposal is signed
    // Successful voting
    // Proposal is executed
    //   - burn conditions are met
    //     - DAO bulla matches parent field in coins being spent
    //     - correct contract:function fields are set
    //     - burn the coins, but not the DAO
    //   - main dao execute: voting threshold and outcome

    let xdrk_supply = 1_000_000;
    let gdrk_supply = 1_000_000;

    Ok(())
}
