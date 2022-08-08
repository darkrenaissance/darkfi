// Example transaction flow
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use pasta_curves::{group::ff::Field, pallas};
use rand::rngs::OsRng;

use darkfi::{
    crypto::{
        constants::MERKLE_DEPTH,
        keypair::{Keypair, PublicKey, SecretKey},
        merkle_node::MerkleNode,
        note::{EncryptedNote, Note},
        nullifier::Nullifier,
        proof::{ProvingKey, VerifyingKey},
        OwnCoin, OwnCoins,
    },
    node::state::{state_transition, ProgramState, StateUpdate},
    tx::builder::{
        TransactionBuilder, TransactionBuilderClearInputInfo, TransactionBuilderInputInfo,
        TransactionBuilderOutputInfo,
    },
    zk::circuit::{BurnContract, MintContract},
    Result,
};

/// The state machine, held in memory.
struct MemoryState {
    /// The entire Merkle tree state
    tree: BridgeTree<MerkleNode, MERKLE_DEPTH>,
    /// List of all previous and the current Merkle roots.
    /// This is the hashed value of all the children.
    merkle_roots: Vec<MerkleNode>,
    /// Nullifiers prevent double spending
    nullifiers: Vec<Nullifier>,
    /// All received coins
    // NOTE: We need maybe a flag to keep track of which ones are
    // spent. Maybe the spend field links to a tx hash:input index.
    // We should also keep track of the tx hash:output index where
    // this coin was received.
    own_coins: OwnCoins,
    /// Verifying key for the mint zk circuit.
    mint_vk: VerifyingKey,
    /// Verifying key for the burn zk circuit.
    burn_vk: VerifyingKey,

    /// Public key of the cashier
    cashier_signature_public: PublicKey,

    /// Public key of the faucet
    faucet_signature_public: PublicKey,

    /// List of all our secret keys
    secrets: Vec<SecretKey>,
}

impl ProgramState for MemoryState {
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

impl MemoryState {
    fn apply(&mut self, mut update: StateUpdate) {
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
                let nullifier = Nullifier::new(secret, note.serial);
                let own_coin = OwnCoin { coin, note, secret, nullifier, leaf_position };
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

fn main() -> Result<()> {
    let cashier_signature_secret = SecretKey::random(&mut OsRng);
    let cashier_signature_public = PublicKey::from_secret(cashier_signature_secret);

    let faucet_signature_secret = SecretKey::random(&mut OsRng);
    let faucet_signature_public = PublicKey::from_secret(faucet_signature_secret);

    let keypair = Keypair::random(&mut OsRng);

    let mint_vk = VerifyingKey::build(11, &MintContract::default());
    let burn_vk = VerifyingKey::build(11, &BurnContract::default());

    let mut state = MemoryState {
        tree: BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100),
        merkle_roots: vec![],
        nullifiers: vec![],
        own_coins: vec![],
        mint_vk,
        burn_vk,
        cashier_signature_public,
        faucet_signature_public,
        secrets: vec![keypair.secret],
    };

    let token_id = pallas::Base::random(&mut OsRng);

    let builder = TransactionBuilder {
        clear_inputs: vec![TransactionBuilderClearInputInfo {
            value: 110,
            token_id,
            signature_secret: cashier_signature_secret,
        }],
        inputs: vec![],
        outputs: vec![TransactionBuilderOutputInfo {
            value: 110,
            token_id,
            public: keypair.public,
        }],
    };

    let mint_pk = ProvingKey::build(11, &MintContract::default());
    let burn_pk = ProvingKey::build(11, &BurnContract::default());
    let tx = builder.build(&mint_pk, &burn_pk)?;

    tx.verify(&state.mint_vk, &state.burn_vk)?;

    let _note = tx.outputs[0].enc_note.decrypt(&keypair.secret)?;

    let update = state_transition(&state, tx)?;
    state.apply(update);

    // Now spend
    let owncoin = &state.own_coins[0];
    let note = owncoin.note;
    let leaf_position = owncoin.leaf_position;
    let root = state.tree.root(0).unwrap();
    let merkle_path = state.tree.authentication_path(leaf_position, &root).unwrap();

    let builder = TransactionBuilder {
        clear_inputs: vec![],
        inputs: vec![TransactionBuilderInputInfo {
            leaf_position,
            merkle_path,
            secret: keypair.secret,
            note,
        }],
        outputs: vec![TransactionBuilderOutputInfo {
            value: 110,
            token_id,
            public: keypair.public,
        }],
    };

    let tx = builder.build(&mint_pk, &burn_pk)?;

    let update = state_transition(&state, tx)?;
    state.apply(update);

    Ok(())
}
