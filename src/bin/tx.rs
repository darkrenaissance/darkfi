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
    note::Note,
    save_params, setup_mint_prover, setup_spend_prover, verify_mint_proof, verify_spend_proof,
    MintRevealedValues, SpendRevealedValues,
};
use sapvi::error::{Error, Result};
use sapvi::serial::{Decodable, Encodable, VarInt};
use sapvi::tx;

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
        clear_outputs: vec![],
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
    let mut tree = CommitmentTree::empty();
    // Here we simulate 5 fake random coins, adding them to our tree.
    for i in 0..5 {
        let cmu = Coin::new(bls12_381::Scalar::random(&mut OsRng).to_repr());
        tree.append(cmu);
    }
    // Now we receive the tx data
    let note = {
        let tx = tx::Transaction::decode(&tx_data[..]).unwrap();
        // Check the public key in the clear inputs
        // It should be a valid public key for the cashier
        assert_eq!(tx.clear_inputs[0].signature_public, cashier_public);
        // Check the tx verifies correctly
        assert!(tx.verify(&mint_pvk, &spend_pvk));
        // Add the new coins to the merkle tree
        tree.append(Coin::new(tx.outputs[0].revealed.coin))
            .expect("append merkle");

        // Now for every new tx we receive, the wallets should iterate over all outputs
        // and try to decrypt the coin's note.
        // If they can successfully decrypt it, then it's a coin destined for us.

        // Try to decrypt output note
        let note = tx.outputs[0]
            .enc_note
            .decrypt(&secret)
            .expect("note should be destined for us");
        // This contains the secret attributes so we can spend the coin
        note
    };

    // Wallet1 has received payment from the cashier.
    // Step 2 is complete.

    // We need to keep track of the witness for this coin.
    // This allows us to prove inclusion of the coin in the merkle tree with ZK.
    // Just as we update the merkle tree with every new coin, so we do the same with the witness.

    // Derive the current witness from the current tree.
    // This is done right after we add our coin to the tree (but before any other coins are added)
    let mut witness = IncrementalWitness::from_tree(&tree);
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
            merkle_root: tree,
            secret,
            note,
        }],
        // We can add more outputs to this list.
        // The only constraint is that sum(value in) == sum(value out)
        outputs: vec![tx::TransactionBuilderOutputInfo {
            value: 110,
            public: public2,
        }],
        clear_outputs: vec![],
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
        assert!(tx.verify(&mint_pvk, &spend_pvk));
    }

    // Step 4 withdraw the funds
}
