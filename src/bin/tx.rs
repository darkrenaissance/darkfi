use bellman::groth16;
use bls12_381::Bls12;
use ff::{Field, PrimeField};
use rand::rngs::OsRng;
use std::path::Path;

use drk::crypto::{
    coin::Coin,
    load_params,
    merkle::{CommitmentTree, IncrementalWitness},
    merkle_node::MerkleNode,
    note::{EncryptedNote, Note},
    nullifier::Nullifier,
    save_params, setup_mint_prover, setup_spend_prover,
};
use drk::serial::{Decodable, Encodable};
use drk::state::{ProgramState, StateUpdate};
use drk::tx;

struct MemoryState {
    // The entire merkle tree state
    tree: CommitmentTree<MerkleNode>,
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
    own_coins: Vec<(Coin, Note, jubjub::Fr, IncrementalWitness<MerkleNode>)>,

    // Mint verifying key used by ZK
    mint_pvk: groth16::PreparedVerifyingKey<Bls12>,
    // Spend verifying key used by ZK
    spend_pvk: groth16::PreparedVerifyingKey<Bls12>,

    // Public key of the cashier
    cashier_public: jubjub::SubgroupPoint,
    // List of all our secret keys
    secrets: Vec<jubjub::Fr>,
}

impl ProgramState for MemoryState {
    fn is_valid_cashier_public_key(&self, public: &jubjub::SubgroupPoint) -> bool {
        public == &self.cashier_public
    }
    fn is_valid_merkle(&self, merkle_root: &MerkleNode) -> bool {
        self.merkle_roots.iter().any(|m| *m == *merkle_root)
    }
    fn nullifier_exists(&self, nullifier: &Nullifier) -> bool {
        self.nullifiers.iter().any(|n| n.repr == nullifier.repr)
    }

    fn mint_pvk(&self) -> &groth16::PreparedVerifyingKey<Bls12> {
        &self.mint_pvk
    }
    fn spend_pvk(&self) -> &groth16::PreparedVerifyingKey<Bls12> {
        &self.spend_pvk
    }
}

impl MemoryState {
    fn apply(&mut self, mut update: StateUpdate) {
        // Extend our list of nullifiers with the ones from the update
        self.nullifiers.append(&mut update.nullifiers);

        // Update merkle tree and witnesses
        for (coin, enc_note) in update.coins.into_iter().zip(update.enc_notes.into_iter()) {
            // Add the new coins to the merkle tree
            let node = MerkleNode::from_coin(&coin);
            self.tree.append(node).expect("Append to merkle tree");

            // Keep track of all merkle roots that have existed
            self.merkle_roots.push(self.tree.root());

            // Also update all the coin witnesses
            for (_, _, _, witness) in self.own_coins.iter_mut() {
                witness.append(node).expect("append to witness");
            }

            if let Some((note, secret)) = self.try_decrypt_note(enc_note) {
                // We need to keep track of the witness for this coin.
                // This allows us to prove inclusion of the coin in the merkle tree with ZK.
                // Just as we update the merkle tree with every new coin, so we do the same with
                // the witness.

                // Derive the current witness from the current tree.
                // This is done right after we add our coin to the tree (but before any other
                // coins are added)

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
                    return Some((note, *secret));
                }
                Err(_) => {}
            }
        }
        // We weren't able to decrypt the note with any of our keys.
        None
    }
}

#[async_std::main]
async fn main() {
    // Auto create trusted ceremony parameters if they don't exist
    if !Path::new("mint.params").exists() {
        let params = setup_mint_prover();
        save_params("mint.params", &params).unwrap();
    }
    if !Path::new("spend.params").exists() {
        let params = setup_spend_prover();
        save_params("spend.params", &params).unwrap();
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
        merkle_roots: vec![],
        nullifiers: vec![],
        own_coins: vec![],
        mint_pvk,
        spend_pvk,
        cashier_public,
        secrets: vec![secret],
    };

    // Step 1: Cashier deposits to wallet1's address

    // Create the deposit for 110 BTC
    // Clear inputs are visible to everyone on the network

    let token_id = jubjub::Fr::random(&mut OsRng);
    let builder = tx::TransactionBuilder {
        clear_inputs: vec![tx::TransactionBuilderClearInputInfo {
            value: 110,
            token_id,
            signature_secret: cashier_secret,
        }],
        inputs: vec![],
        outputs: vec![tx::TransactionBuilderOutputInfo {
            value: 110,
            token_id,
            public,
        }],
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

    // Wallet1 is receiving tx, and for every new coin it finds, it adds to its
    // merkle tree
    {
        // Here we simulate 5 fake random coins, adding them to our tree.
        let tree = &mut state.tree;
        for _i in 0..5 {
            // Don't worry about any of the code in this block
            // We're just filling the tree with fake coins
            let cmu = MerkleNode::new(bls12_381::Scalar::random(&mut OsRng).to_repr());
            tree.append(cmu).unwrap();

            let root = tree.root();
            state.merkle_roots.push(root);
        }
    }

    // Now we receive the tx data
    {
        let tx = tx::Transaction::decode(&tx_data[..]).unwrap();

        let update = state_transition(&state, tx).expect("step 2 state transition failed");
        // Our state impl is memory online for this demo
        // but in the real version, this function will be async
        // and using the databases.
        state.apply(update);
    }

    //// Wallet1 has received payment from the cashier.
    //// Step 2 is complete.
    assert_eq!(state.own_coins.len(), 1);
    ////let (coin, note, secret, witness) = &mut state.own_coins[0];

    let merkle_path = {
        let tree = &mut state.tree;
        let (coin, _, _, witness) = &mut state.own_coins[0];
        // Check this is the 6th coin we added
        assert_eq!(witness.position(), 5);
        assert_eq!(tree.root(), witness.root());

        // Add some more random coins in
        for _i in 0..10 {
            // Don't worry about any of the code in this block
            // We're just filling the tree with fake coins
            let cmu = MerkleNode::new(bls12_381::Scalar::random(&mut OsRng).to_repr());
            tree.append(cmu).unwrap();
            witness.append(cmu).unwrap();
            assert_eq!(tree.root(), witness.root());

            let root = tree.root();
            state.merkle_roots.push(root);
        }

        assert_eq!(state.merkle_roots.len(), 16);

        // This is the value we need to spend the coin
        // We use the witness and the merkle root (both in sync with each other)
        // to prove our coin exists inside the tree.
        // The coin is not revealed publicly but is proved to exist inside
        // a merkle tree. Only the root will be revealed, and then the
        // verifier checks that merkle root actually existed before.
        let merkle_path = witness.path().unwrap();

        // Just test the path is good because we just added a bunch of fake coins
        let node = MerkleNode::from_coin(coin);
        let root = tree.root();
        drop(tree);
        drop(witness);
        assert_eq!(merkle_path.root(node), root);
        let root = root;
        assert!(state.is_valid_merkle(&root));

        merkle_path
    };

    // Step 3: wallet1 sends payment to wallet2

    // Wallet1 now wishes to send the coin to wallet2

    // The receiving wallet has a secret key
    let secret2 = jubjub::Fr::random(&mut OsRng);
    // This is their public key to receive payment
    let public2 = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret2;

    // Make a spend tx

    //let inputs: Vec<tx::TransactionBuilderInputInfo> = vec![];
    // Construct a new tx spending the coin
    // We need the decrypted note and our private key
    let builder = tx::TransactionBuilder {
        clear_inputs: vec![],
        inputs: vec![tx::TransactionBuilderInputInfo {
            merkle_path,
            secret,
            note: state.own_coins[0].1.clone(),
        }],
        // We can add more outputs to this list.
        // The only constraint is that sum(value in) == sum(value out)
        outputs: vec![tx::TransactionBuilderOutputInfo {
            value: 110,
            token_id,
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
        state.apply(update);
    }
}

use drk::state::{VerifyFailed, VerifyResult};
use log::*;

pub fn state_transition<S: ProgramState>(
    state: &S,
    tx: tx::Transaction,
) -> VerifyResult<StateUpdate> {
    // Check deposits are legit

    debug!(target: "STATE TRANSITION", "iterate clear_inputs");

    for (i, input) in tx.clear_inputs.iter().enumerate() {
        // Check the public key in the clear inputs
        // It should be a valid public key for the cashier

        if !state.is_valid_cashier_public_key(&input.signature_public) {
            log::error!(target: "STATE TRANSITION", "Not valid cashier public key");
            return Err(VerifyFailed::InvalidCashierKey(i));
        }
    }

    debug!(target: "STATE TRANSITION", "iterate inputs");

    for (i, input) in tx.inputs.iter().enumerate() {
        // Check merkle roots
        let merkle = &input.revealed.merkle_root;

        // Merkle is used to know whether this is a coin that existed
        // in a previous state.
        if !state.is_valid_merkle(merkle) {
            return Err(VerifyFailed::InvalidMerkle(i));
        }

        // The nullifiers should not already exist
        // It is double spend protection.
        let nullifier = &input.revealed.nullifier;

        if state.nullifier_exists(nullifier) {
            return Err(VerifyFailed::DuplicateNullifier(i));
        }
    }

    debug!(target: "STATE TRANSITION", "Check the tx Verifies correctly");
    // Check the tx verifies correctly
    tx.verify(state.mint_pvk(), state.spend_pvk())?;

    let mut nullifiers = vec![];
    for input in tx.inputs {
        nullifiers.push(input.revealed.nullifier);
    }

    // Newly created coins for this tx
    let mut coins = vec![];
    let mut enc_notes = vec![];
    for output in tx.outputs {
        // Gather all the coins
        coins.push(Coin::new(output.revealed.coin));
        enc_notes.push(output.enc_note);
    }

    Ok(StateUpdate {
        nullifiers,
        coins,
        enc_notes,
    })
}
