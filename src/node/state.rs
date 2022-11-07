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

use darkfi_sdk::crypto::{constants::MERKLE_DEPTH, MerkleNode, Nullifier, PublicKey, SecretKey};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use lazy_init::Lazy;
use log::{debug, error};

use crate::{
    blockchain::{nfstore::NullifierStore, rootstore::RootStore, Blockchain},
    consensus::{TESTNET_GENESIS_HASH_BYTES, TESTNET_GENESIS_TIMESTAMP},
    crypto::{
        coin::{Coin, OwnCoin},
        note::{EncryptedNote, Note},
        proof::VerifyingKey,
        util::poseidon_hash,
    },
    tx::Transaction,
    wallet::walletdb::WalletPtr,
    zk::circuit::{BurnContract, MintContract},
    Result, VerifyFailed, VerifyResult,
};

/// Trait implementing the state functions used by the state transition.
pub trait ProgramState {
    /// Check if the public key is coming from a trusted cashier
    fn is_valid_cashier_public_key(&self, public: &PublicKey) -> bool;
    /// Check if the public key is coming from a trusted faucet
    fn is_valid_faucet_public_key(&self, public: &PublicKey) -> bool;
    /// Check if a merkle root is valid in this context
    fn is_valid_merkle(&self, merkle: &MerkleNode) -> bool;
    /// Check if the nullifier has been seen already
    fn nullifier_exists(&self, nullifier: &Nullifier) -> bool;
    /// Mint proof verification key
    fn mint_vk(&self) -> &VerifyingKey;
    /// Burn proof verification key
    fn burn_vk(&self) -> &VerifyingKey;
}

/// A struct representing a state update.
/// This gets applied on top of an existing state.
#[derive(Clone)]
pub struct StateUpdate {
    /// All nullifiers in a transaction
    pub nullifiers: Vec<Nullifier>,
    /// All coins in a transaction
    pub coins: Vec<Coin>,
    /// All encrypted notes in a transaction
    pub enc_notes: Vec<EncryptedNote>,
}

/// State transition function
pub fn state_transition<S: ProgramState>(state: &S, tx: Transaction) -> VerifyResult<StateUpdate> {
    // Check the public keys in the clear inputs to see if they're coming
    // from a valid cashier or faucet.
    debug!(target: "state_transition", "Iterate clear_inputs");
    for (i, input) in tx.clear_inputs.iter().enumerate() {
        let pk = &input.signature_public;
        // TODO: this depends on the token ID
        if !state.is_valid_cashier_public_key(pk) && !state.is_valid_faucet_public_key(pk) {
            error!(target: "state_transition", "Invalid pubkey for clear input: {:?}", pk);
            return Err(VerifyFailed::InvalidCashierOrFaucetKey(i))
        }
    }

    // Nullifiers in the transaction
    let mut nullifiers = Vec::with_capacity(tx.inputs.len());

    debug!(target: "state_transition", "Iterate inputs");
    for (i, input) in tx.inputs.iter().enumerate() {
        let merkle = &input.revealed.merkle_root;

        // The Merkle root is used to know whether this is a coin that
        // existed in a previous state.
        if !state.is_valid_merkle(merkle) {
            error!(target: "state_transition", "Invalid Merkle root (input {})", i);
            debug!(target: "state_transition", "root: {:?}", merkle);
            return Err(VerifyFailed::InvalidMerkle(i))
        }

        // The nullifiers should not already exist.
        // It is the double-spend protection.
        let nullifier = &input.revealed.nullifier;
        if state.nullifier_exists(nullifier) ||
            (1..nullifiers.len()).any(|i| nullifiers[i..].contains(&nullifiers[i - 1]))
        {
            error!(target: "state_transition", "Duplicate nullifier found (input {})", i);
            debug!(target: "state_transition", "nullifier: {:?}", nullifier);
            return Err(VerifyFailed::NullifierExists(i))
        }

        nullifiers.push(input.revealed.nullifier);
    }

    debug!(target: "state_transition", "Verifying zk proofs");
    match tx.verify(state.mint_vk(), state.burn_vk()) {
        Ok(()) => debug!(target: "state_transition", "Verified successfully"),
        Err(e) => {
            error!(target: "state_transition", "Failed verifying zk proofs: {}", e);
            return Err(VerifyFailed::ProofVerifyFailed(e.to_string()))
        }
    }

    // Newly created coins for this transaction
    let mut coins = Vec::with_capacity(tx.outputs.len());
    let mut enc_notes = Vec::with_capacity(tx.outputs.len());
    for output in tx.outputs {
        // Gather all the coins
        coins.push(output.revealed.coin);
        enc_notes.push(output.enc_note);
    }

    Ok(StateUpdate { nullifiers, coins, enc_notes })
}

/// Struct holding the state which we can apply a [`StateUpdate`] onto.
#[derive(Clone)]
pub struct State {
    /// The entire Merkle tree state
    pub tree: BridgeTree<MerkleNode, MERKLE_DEPTH>,
    /// List of all previous and the current merkle roots.
    /// This is the hashed value of all the children.
    pub merkle_roots: RootStore,
    /// Nullifiers prevent double-spending
    pub nullifiers: NullifierStore,
    /// List of Cashier public keys
    pub cashier_pubkeys: Vec<PublicKey>,
    /// List of Faucet public keys
    pub faucet_pubkeys: Vec<PublicKey>,
    /// Verifying key for the Mint ZK proof
    pub mint_vk: Lazy<VerifyingKey>,
    /// Verifying key for the Burn ZK proof
    pub burn_vk: Lazy<VerifyingKey>,
}

impl State {
    /// Create a dummy state
    pub fn dummy() -> Result<Self> {
        let db = sled::Config::new().temporary(true).open()?;
        let bc = Blockchain::new(&db, *TESTNET_GENESIS_TIMESTAMP, *TESTNET_GENESIS_HASH_BYTES)?;
        let mt = BridgeTree::<MerkleNode, 32>::new(100);
        Ok(State {
            tree: mt,
            merkle_roots: bc.merkle_roots,
            nullifiers: bc.nullifiers,
            cashier_pubkeys: vec![],
            faucet_pubkeys: vec![],
            mint_vk: Lazy::new(),
            burn_vk: Lazy::new(),
        })
    }

    /// Apply a [`StateUpdate`] to some state.
    pub async fn apply(
        &mut self,
        update: StateUpdate,
        secret_keys: Vec<SecretKey>,
        notify: Option<smol::channel::Sender<(PublicKey, u64)>>,
        wallet: WalletPtr,
    ) -> Result<()> {
        debug!(target: "state_apply", "Extend nullifier set");
        debug!("Existing nullifiers: {:#?}", self.nullifiers.get_all()?);
        debug!("Update's nullifiers: {:#?}", update.nullifiers);
        self.nullifiers.insert(&update.nullifiers)?;

        debug!(target: "state_apply", "Update Merkle tree and witnesses");
        for (coin, enc_note) in update.coins.into_iter().zip(update.enc_notes.iter()) {
            // Add the new coins to the Merkle tree
            let node = MerkleNode::from(coin.0);
            debug!("Current merkle tree: {:#?}", self.tree);
            self.tree.append(&node);
            debug!("Merkle tree after append: {:#?}", self.tree);

            // Keep track of all Merkle roots that have existed
            debug!("Existing merkle roots: {:#?}", self.merkle_roots.get_all()?);
            debug!("New merkle root: {:#?}", self.tree.root(0).unwrap());
            self.merkle_roots.insert(&[self.tree.root(0).unwrap()])?;

            for secret in secret_keys.iter() {
                if let Some(note) = State::try_decrypt_note(enc_note, *secret) {
                    debug!(target: "state_apply", "Received a coin: amount {}", note.value);
                    let leaf_position = self.tree.witness().unwrap();
                    let nullifier =
                        Nullifier::from(poseidon_hash::<2>([secret.inner(), note.serial]));
                    let own_coin = OwnCoin {
                        coin,
                        note: note.clone(),
                        secret: *secret,
                        nullifier,
                        leaf_position,
                    };

                    // TODO: FIXME: BUG check values inside the note are correct
                    // We need to hash them all and check them against the coin
                    // for them to be accepted.
                    // Don't trust - verify.

                    wallet.put_own_coin(own_coin).await?;

                    if let Some(ch) = notify.clone() {
                        debug!(target: "state_apply", "Send a notification");
                        let pubkey = PublicKey::from_secret(*secret);
                        ch.send((pubkey, note.value)).await?;
                    }
                }
            }

            // Save updated merkle tree into the wallet.
            wallet.put_tree(&self.tree).await?;
        }

        debug!(target: "state_apply", "Finished apply() successfully.");
        Ok(())
    }

    pub fn try_decrypt_note(ciphertext: &EncryptedNote, secret: SecretKey) -> Option<Note> {
        match ciphertext.decrypt(&secret) {
            Ok(note) => Some(note),
            Err(_) => None,
        }
    }
}

impl ProgramState for State {
    fn is_valid_cashier_public_key(&self, public: &PublicKey) -> bool {
        debug!(target: "state_transition", "Checking if pubkey is a valid cashier");
        self.cashier_pubkeys.contains(public)
    }

    fn is_valid_faucet_public_key(&self, public: &PublicKey) -> bool {
        debug!(target: "state_transition", "Checking if pubkey is a valid faucet");
        self.faucet_pubkeys.contains(public)
    }

    fn is_valid_merkle(&self, merkle_root: &MerkleNode) -> bool {
        debug!(target: "state_transition", "Checking if Merkle root is valid");
        if let Ok(mr) = self.merkle_roots.contains(merkle_root) {
            return mr
        }

        panic!("RootStore db corruption, could not check merkle_roots.contains()");
    }

    fn nullifier_exists(&self, nullifier: &Nullifier) -> bool {
        debug!(target: "state_transition", "Checking if Nullifier exists");
        if let Ok(nf) = self.nullifiers.contains(nullifier) {
            return nf
        }

        panic!("NullifierStore db corruption, could not check nullifiers.contains()");
    }

    fn mint_vk(&self) -> &VerifyingKey {
        self.mint_vk.get_or_create(build_mint_vk)
    }

    fn burn_vk(&self) -> &VerifyingKey {
        self.burn_vk.get_or_create(build_burn_vk)
    }
}

fn build_mint_vk() -> VerifyingKey {
    debug!("Building verifying key for MintContract");
    VerifyingKey::build(11, &MintContract::default())
}

fn build_burn_vk() -> VerifyingKey {
    debug!("Building verifying key for BurnContract");
    VerifyingKey::build(11, &BurnContract::default())
}
