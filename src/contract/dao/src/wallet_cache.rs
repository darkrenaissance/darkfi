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

use darkfi_sdk::{
    bridgetree,
    crypto::{note::AeadEncryptedNote, pasta_prelude::Field, MerkleNode, MerkleTree, SecretKey},
    pasta::pallas,
};

use darkfi_money_contract::{client::MoneyNote, model::Coin};

pub struct OwnCoin {
    pub coin: Coin,
    pub note: MoneyNote,
    pub leaf_position: bridgetree::Position,
}

pub struct WalletCache {
    // Normally this would be a HashMap, but SecretKey is not Hash-able
    // TODO: This can be HashableBase
    cache: Vec<(SecretKey, Vec<OwnCoin>)>,
    /// The entire Money Merkle tree state
    pub tree: MerkleTree,
}

impl Default for WalletCache {
    fn default() -> Self {
        Self::new()
    }
}

impl WalletCache {
    pub fn new() -> Self {
        let mut tree = MerkleTree::new(100);
        tree.append(MerkleNode::from(pallas::Base::ZERO));
        let _ = tree.mark().unwrap();
        Self { cache: Vec::new(), tree }
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

    pub fn try_decrypt_note(&mut self, coin: Coin, ciphertext: &AeadEncryptedNote) {
        // Add the new coins to the Merkle tree
        self.tree.append(MerkleNode::from(coin.inner()));

        // Loop through all our secret keys...
        for (secret, own_coins) in self.cache.iter_mut() {
            // .. attempt to decrypt the note ...
            if let Ok(note) = ciphertext.decrypt(secret) {
                let leaf_position = self.tree.mark().expect("coin should be in tree");
                own_coins.push(OwnCoin { coin, note, leaf_position });
            }
        }
    }
}
