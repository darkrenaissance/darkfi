use bellman::groth16;
use bls12_381::Bls12;
use ff::{Field, PrimeField};
use group::Group;
use rand::rngs::OsRng;
use std::io;
use std::path::Path;

use sapvi::crypto::{
    coin::Coin,
    create_mint_proof, create_spend_proof, load_params,
    merkle::{CommitmentTree, IncrementalWitness},
    note::{EncryptedNote, Note},
    save_params, setup_mint_prover, setup_spend_prover, verify_mint_proof, verify_spend_proof,
    MintRevealedValues, SpendRevealedValues,
};
use sapvi::error::{Error, Result};
use sapvi::serial::{Decodable, Encodable, VarInt};
use sapvi::state::{state_transition, ProgramState, StateUpdates};
use sapvi::tx;

struct MemoryState {
    tree: CommitmentTree<Coin>,
    nullifiers: Vec<[u8; 32]>,
    own_coins: Vec<([u8; 32], Note, jubjub::Fr, IncrementalWitness<Coin>)>,
    mint_pvk: groth16::PreparedVerifyingKey<Bls12>,
    spend_pvk: groth16::PreparedVerifyingKey<Bls12>,
    cashier_public: jubjub::SubgroupPoint,
    secrets: Vec<jubjub::Fr>,
}

impl ProgramState for MemoryState {
    fn is_valid_cashier_public_key(&self, public: &jubjub::SubgroupPoint) -> bool {
        public == &self.cashier_public
    }
    fn is_valid_merkle(&self, merkle: &bls12_381::Scalar) -> bool {
        true
    }
    fn nullifier_exists(&self, nullifier: &[u8; 32]) -> bool {
        false
    }

    fn mint_pvk(&self) -> &groth16::PreparedVerifyingKey<Bls12> {
        &self.mint_pvk
    }
    fn spend_pvk(&self) -> &groth16::PreparedVerifyingKey<Bls12> {
        &self.spend_pvk
    }
}

impl MemoryState {
    async fn apply(&mut self, mut updates: StateUpdates) {
        self.nullifiers.append(&mut updates.nullifiers);

        // Update merkle tree and witnesses
        for (coin, enc_note) in updates.coins.into_iter().zip(updates.enc_notes.into_iter()) {
            // Add the new coins to the merkle tree
            self.tree
                .append(Coin::new(coin.clone()))
                .expect("Append to merkle tree");

            if let Some((note, secret)) = self.try_decrypt_note(enc_note) {
                // We need to keep track of the witness for this coin.
                // This allows us to prove inclusion of the coin in the merkle tree with ZK.
                // Just as we update the merkle tree with every new coin, so we do the same with the witness.

                // Derive the current witness from the current tree.
                // This is done right after we add our coin to the tree (but before any other coins are added)

                // Make a new witness for this coin
                let witness = IncrementalWitness::from_tree(&self.tree);
                self.own_coins.push((coin, note, secret, witness));
            }
        }
    }

    fn try_decrypt_note(&self, ciphertext: EncryptedNote) -> Option<(Note, jubjub::Fr)> {
        // Loop through all our secret keys...
        for secret in &self.secrets {
            // ... attempt to decrypt the note ...
            match ciphertext.decrypt(secret) {
                Ok(note) => {
                    // ... and return the decrypted note for this coin.
                    return Some((note, secret.clone()));
                }
                Err(_) => {}
            }
        }
        // We weren't able to decrypt the note with any of our keys.
        None
    }
}

fn main() {
    // Auto create trusted ceremony parameters if they don't exist
    if !Path::new("mint.params").exists() {
        let params = setup_mint_prover();
        save_params("mint.params", &params);
    }
    if !Path::new("spend.params").exists() {
        let params = setup_spend_prover();
        save_params("spend.params", &params);
    }

    // Load trusted setup parameters
    let (mint_params, mint_pvk) = load_params("mint.params").expect("params should load");
    let (spend_params, spend_pvk) = load_params("spend.params").expect("params should load");

    // Cashier creates a secret key
    let cashier_secret = jubjub::Fr::random(&mut OsRng);
    // This is their public key
    let cashier_public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * cashier_secret;

    // Wallet 1 creates a secret key
    let secret = jubjub::Fr::random(&mut OsRng);
    // This is their public key
    let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;

    let mut state = MemoryState {
        tree: CommitmentTree::empty(),
        nullifiers: vec![],
        own_coins: vec![],
        mint_pvk,
        spend_pvk,
        cashier_public,
        secrets: vec![secret.clone()],
    };

    // Step 1: Cashier deposits to wallet1's address

    // Create the deposit for 110 BTC
    // Clear inputs are visible to everyone on the network
    let builder = tx::TransactionBuilder {
        clear_inputs: vec![tx::TransactionBuilderClearInputInfo {
            value: 110,
            signature_secret: cashier_secret,
        }],
        inputs: vec![],
        outputs: vec![tx::TransactionBuilderOutputInfo { value: 110, public }],
    };

    // We will 'compile' the tx, and then serialize it to this Vec<u8>
    let mut tx_data = vec![];
    {
        // Build the tx
        let tx = builder.build(&mint_params, &spend_params);
        // Now serialize it
        tx.encode(&mut tx_data).expect("encode tx");
    }

    // Step 1 is completed.
    // Tx data is posted to the blockchain

    // Step 2: wallet1 receive's payment from the cashier

    // Wallet1 is receiving tx, and for every new coin it finds, it adds to its merkle tree
    {
        // Here we simulate 5 fake random coins, adding them to our tree.
        let tree = &mut state.tree;
        for i in 0..5 {
            let cmu = Coin::new(bls12_381::Scalar::random(&mut OsRng).to_repr());
            tree.append(cmu);
        }
    }

    // Now we receive the tx data
    {
        let tx = tx::Transaction::decode(&tx_data[..]).unwrap();

        let update = state_transition(&state, tx).expect("step 2 state transition failed");

        smol::block_on(state.apply(update));
    }

    // Wallet1 has received payment from the cashier.
    // Step 2 is complete.
    assert_eq!(state.own_coins.len(), 1);
    //let (coin, note, secret, witness) = &mut state.own_coins[0];

    let auth_path =
    {
        let tree = &mut state.tree;
        let witness = &mut state.own_coins[0].3;
        // Check this is the 6th coin we added
        assert_eq!(witness.position(), 5);
        assert_eq!(tree.root(), witness.root());

        // Add some more random coins in
        for i in 0..10 {
            let cmu = Coin::new(bls12_381::Scalar::random(&mut OsRng).to_repr());
            tree.append(cmu);
            witness.append(cmu);
            assert_eq!(tree.root(), witness.root());
        }

        // TODO: Some stupid glue code. Need to put this somewhere else.
        let merkle_path = witness.path().unwrap();
        let auth_path: Vec<(bls12_381::Scalar, bool)> = merkle_path
            .auth_path
            .iter()
            .map(|(node, b)| ((*node).into(), *b))
            .collect();
        auth_path
    };

    // Step 3: wallet1 sends payment to wallet2

    // Wallet1 now wishes to send the coin to wallet2

    let secret2 = jubjub::Fr::random(&mut OsRng);
    // This is their public key
    let public2 = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret2;

    // Make a spend tx

    // Get the coin we're spending from the previous tx
    let coin = {
        let tx = tx::Transaction::decode(&tx_data[..]).unwrap();
        tx.outputs[0].revealed.coin
    };

    // Construct a new tx spending the coin
    // We need the decrypted note and our private key
    let builder = tx::TransactionBuilder {
        clear_inputs: vec![],
        inputs: vec![tx::TransactionBuilderInputInfo {
            coin,
            merkle_path: auth_path,
            secret: secret.clone(),
            note: state.own_coins[0].1.clone(),
        }],
        // We can add more outputs to this list.
        // The only constraint is that sum(value in) == sum(value out)
        outputs: vec![tx::TransactionBuilderOutputInfo {
            value: 110,
            public: public2,
        }],
    };
    // Build the tx
    let mut tx_data = vec![];
    {
        let tx = builder.build(&mint_params, &spend_params);
        tx.encode(&mut tx_data).expect("encode tx");
    }
    // Verify it's valid
    {
        let tx = tx::Transaction::decode(&tx_data[..]).unwrap();
        let update = state_transition(&state, tx).expect("step 3 state transition failed");
    }
}
