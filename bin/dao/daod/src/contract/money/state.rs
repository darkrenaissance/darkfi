/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use darkfi::crypto::coin::Coin;

use super::transfer;
use crate::note::EncryptedNote2;

type MerkleTree = BridgeTree<MerkleNode, MERKLE_DEPTH>;

pub struct OwnCoin {
    pub coin: Coin,
    pub note: transfer::wallet::Note,
    pub leaf_position: incrementalmerkletree::Position,
}

pub struct WalletCache {
    // Normally this would be a HashMap, but SecretKey is not Hash-able
    // TODO: This can be HashableBase
    cache: Vec<(SecretKey, Vec<OwnCoin>)>,
}

impl WalletCache {
    pub fn new() -> Self {
        Self { cache: Vec::new() }
    }

    /// Must be called at the start to begin tracking received coins for this secret.
    pub fn track(&mut self, secret: SecretKey) {
        self.cache.push((secret, Vec::new()));
    }

    /// Get all coins received by this secret key
    /// track() must be called on this secret before calling this or the function will panic.
    pub fn get_received(&mut self, secret: &SecretKey) -> Vec<OwnCoin> {
        for (other_secret, own_coins) in self.cache.iter_mut() {
            if *secret == *other_secret {
                // clear own_coins vec, and return current contents
                return std::mem::take(own_coins)
            }
        }
        panic!("you forget to track() this secret!");
    }

    pub fn try_decrypt_note(
        &mut self,
        coin: Coin,
        ciphertext: EncryptedNote2,
        tree: &mut MerkleTree,
    ) {
        // Loop through all our secret keys...
        for (secret, own_coins) in self.cache.iter_mut() {
            // .. attempt to decrypt the note ...
            if let Ok(note) = ciphertext.decrypt(secret) {
                let leaf_position = tree.witness().expect("coin should be in tree");
                own_coins.push(OwnCoin { coin, note, leaf_position });
            }
        }
    }
}

/// The state machine, held in memory.
pub struct State {
    /// The entire Merkle tree state
    pub tree: MerkleTree,
    /// List of all previous and the current Merkle roots.
    /// This is the hashed value of all the children.
    pub merkle_roots: Vec<MerkleNode>,
    /// Nullifiers prevent double spending
    pub nullifiers: Vec<Nullifier>,

    /// Public key of the cashier
    pub cashier_signature_public: PublicKey,

    /// Public key of the faucet
    pub faucet_signature_public: PublicKey,

    pub wallet_cache: WalletCache,
}

impl State {
    pub fn new(
        cashier_signature_public: PublicKey,
        faucet_signature_public: PublicKey,
    ) -> Box<Self> {
        Box::new(Self {
            tree: MerkleTree::new(100),
            merkle_roots: vec![],
            nullifiers: vec![],
            cashier_signature_public,
            faucet_signature_public,
            wallet_cache: WalletCache::new(),
        })
    }

    pub fn is_valid_cashier_public_key(&self, public: &PublicKey) -> bool {
        public == &self.cashier_signature_public
    }

    pub fn is_valid_faucet_public_key(&self, public: &PublicKey) -> bool {
        public == &self.faucet_signature_public
    }

    pub fn is_valid_merkle(&self, merkle_root: &MerkleNode) -> bool {
        self.merkle_roots.iter().any(|m| m == merkle_root)
    }

    pub fn nullifier_exists(&self, nullifier: &Nullifier) -> bool {
        self.nullifiers.iter().any(|n| n == nullifier)
    }
}
