use async_std::sync;
use log::*;
use bellman::groth16;
use rocksdb::DB;
use std::fs::File;
use rusqlite::{named_params, Connection};
use bls12_381::Bls12;
use ff::{Field, PrimeField};
use rand::rngs::OsRng;
use std::path::Path;
use drk::{Result, Error};

use drk::crypto::{
    coin::Coin,
    load_params,
    merkle::{CommitmentTree, IncrementalWitness},
    merkle_node::{hash_coin, MerkleNode},
    note::{EncryptedNote, Note},
    nullifier::Nullifier,
    save_params, setup_mint_prover, setup_spend_prover,
};
use drk::serial::{Decodable, Encodable};
use drk::state::{state_transition, ProgramState, StateUpdate};
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
    cashier_public: Vec<u8>,
    // List of all our secret keys
    secrets: Vec<jubjub::Fr>,
}

impl ProgramState for MemoryState {
    // Vec<u8> for keys
    fn is_valid_cashier_public_key(&self, public: &jubjub::SubgroupPoint) -> bool {
        let path = dirs::home_dir()
            .expect("Cannot find home directory.")
            .as_path()
            .join(".config/darkfi/cashier.db");
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        let mut stmt = connect.prepare("SELECT key_public FROM keys").unwrap();
        let key_iter = stmt.query_map::<Vec<u8>, _, _>([], |row| row.get(0)).unwrap();
        // does not actually check whether the cashier key is valid
        for key in key_iter {
            key.unwrap() == self.cashier_public;
        }
        true
    }
    // rocksdb
    fn is_valid_merkle(&self, merkle_root: &MerkleNode) -> bool {
        self.merkle_roots.iter().any(|m| *m == *merkle_root)
    }
    // rocksdb
    fn nullifier_exists(&self, nullifier: &Nullifier) -> bool {
        self.nullifiers.iter().any(|n| n.repr == nullifier.repr)
    }

    // loaded from disk
    fn mint_pvk(&self) -> &groth16::PreparedVerifyingKey<Bls12> {
        &self.mint_pvk
    }
    // loaded from disk
    fn spend_pvk(&self) -> &groth16::PreparedVerifyingKey<Bls12> {
        &self.spend_pvk
    }
}

impl MemoryState {
    fn apply(&mut self, mut update: StateUpdate) {
        // Extend our list of nullifiers with the ones from the update
        self.nullifiers.append(&mut update.nullifiers);

        // merkle tree is rocksdb
        // encrpt note is sql

        // Update merkle tree and witnesses
        for (coin, enc_note) in update.coins.into_iter().zip(update.enc_notes.into_iter()) {
            // Add the new coins to the merkle tree
            let node = MerkleNode::from_coin(&coin);
            self.tree.append(node).expect("Append to merkle tree");

            // Keep track of all merkle roots that have existed
            self.merkle_roots.push(self.tree.root());

            // own coins is sql
            // Also update all the coin witnesses
            for (_, _, _, witness) in self.own_coins.iter_mut() {
                witness.append(node).expect("append to witness");
            }

            // sql
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

    // sql
    fn try_decrypt_note(&self, ciphertext: EncryptedNote) -> Option<(Note, jubjub::Fr)> {
        debug!(target: "adapter", "try_decrypt_note() [START]");
        let path = dirs::home_dir()
            .expect("Cannot find home directory.")
            .as_path()
            .join(".config/darkfi/wallet.db");
        debug!(target: "adapter", "try_decrypt_note() [FOUND PATH]");
        println!("Found path: {:?}", &path);
        debug!(target: "adapter", "try_decrypt_note() [TRY DB CONNECT]");
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        let mut stmt = connect.prepare("SELECT key_private FROM keys").ok()?;
        let key_iter = stmt.query_map::<String, _, _>([], |row| row.get(0)).ok()?;
        for key in key_iter {
            println!("Found key {:?}", key.unwrap());
        }
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

    pub async fn key_gen(&self) -> Result<()> {
        let path = dirs::home_dir()
            .expect("Cannot find home directory.")
            .as_path()
            .join(".config/darkfi/wallet.db");
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        let id = 0;
        // Create keys
        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        debug!(target: "adapter", "key_gen() [Generating public key...]");
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let pubkey = drk::serial::serialize(&public);
        let privkey = drk::serial::serialize(&secret);
        // Write keys to database
        connect.execute(
            "INSERT INTO keys(key_id, key_private, key_public)
            VALUES (:id, :privkey, :pubkey)",
            named_params!{":id": id,
                           ":privkey": privkey,
                           ":pubkey": pubkey
                          }
        )?;
        Ok(())
    }

    pub async fn cashier_key_gen(&self) -> Result<()> {
        let path = dirs::home_dir()
            .expect("Cannot find home directory.")
            .as_path()
            .join(".config/darkfi/cashier.db");
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        let id = 0;
        // Create keys
        let secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
        let pubkey = drk::serial::serialize(&public);
        let privkey = drk::serial::serialize(&secret);
        // Write keys to database
        connect.execute(
            "INSERT INTO keys(key_id, key_private, key_public)
            VALUES (:id, :privkey, :pubkey)",
            named_params!{":id": id,
                           ":privkey": privkey,
                           ":pubkey": pubkey
                          }
        )?;
        Ok(())
    }

    pub async fn get_cashier_public_key(&self) -> Result<Vec<Vec<u8>>> {
        let path = dirs::home_dir()
            .expect("Cannot find home directory.")
            .as_path()
            .join(".config/darkfi/wallet.db");
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        let mut stmt = connect.prepare("SELECT key_public FROM cashier").unwrap();
        let key_iter = stmt.query_map::<Vec<u8>, _, _>([], |row| row.get(0)).unwrap();
        let mut pub_keys = Vec::new();
        for key in key_iter {
            pub_keys.push(key.unwrap());
        }
        Ok(pub_keys)
    }
}

fn main() {
    // Auto create trusted ceremony parameters if they don't exist
    if !Path::new("mint.params").exists() {
        let params = setup_mint_prover();
        save_params("mint.params", &params).expect("Failed to create mint.params.");
    }
    if !Path::new("spend.params").exists() {
        let params = setup_spend_prover();
        save_params("spend.params", &params).expect("Failed to create save.params");
    }

    // Load trusted setup parameters
    let (mint_params, mint_pvk) = load_params("mint.params").expect("params should load");
    let (spend_params, spend_pvk) = load_params("spend.params").expect("params should load");

    // Where is cashier private key stored? Does node have its own wallet schema
    // Cashier creates a secret key
    //let cashier_secret = jubjub::Fr::random(&mut OsRng);
    //// This is their public key
    //let cashier_public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * cashier_secret;

    // Wallet 1 creates a secret key
    //let secret = jubjub::Fr::random(&mut OsRng);
    // This is their public key
    //let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;

    let mut state = MemoryState {
        tree: CommitmentTree::empty(),
        merkle_roots: vec![],
        nullifiers: vec![],
        own_coins: vec![],
        mint_pvk,
        spend_pvk,
        cashier_public,
        secrets: vec![secret.clone()],
    };

    //let cashier_secret = state.cashier_key();
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

    // Wallet1 is receiving tx, and for every new coin it finds, it adds to its
    // merkle tree
    {
        // Here we simulate 5 fake random coins, adding them to our tree.
        let tree = &mut state.tree;
        for i in 0..5 {
            // Don't worry about any of the code in this block
            // We're just filling the tree with fake coins
            let cmu = MerkleNode::new(bls12_381::Scalar::random(&mut OsRng).to_repr());
            tree.append(cmu);

            let root = tree.root();
            state.merkle_roots.push(root.into());
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

    // Wallet1 has received payment from the cashier.
    // Step 2 is complete.
    assert_eq!(state.own_coins.len(), 1);
    //let (coin, note, secret, witness) = &mut state.own_coins[0];

    let merkle_path = {
        let tree = &mut state.tree;
        let (coin, _, _, witness) = &mut state.own_coins[0];
        // Check this is the 6th coin we added
        assert_eq!(witness.position(), 5);
        assert_eq!(tree.root(), witness.root());

        // Add some more random coins in
        for i in 0..10 {
            // Don't worry about any of the code in this block
            // We're just filling the tree with fake coins
            let cmu = MerkleNode::new(bls12_381::Scalar::random(&mut OsRng).to_repr());
            tree.append(cmu);
            witness.append(cmu);
            assert_eq!(tree.root(), witness.root());

            let root = tree.root();
            state.merkle_roots.push(root.into());
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
        let node = MerkleNode::from_coin(&coin);
        let root = tree.root();
        drop(tree);
        drop(witness);
        assert_eq!(merkle_path.root(node), root);
        let root = root.into();
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

    // Construct a new tx spending the coin
    // We need the decrypted note and our private key
    let builder = tx::TransactionBuilder {
        clear_inputs: vec![],
        inputs: vec![tx::TransactionBuilderInputInfo {
            merkle_path,
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
        state.apply(update);
    }
}
