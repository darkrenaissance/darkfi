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

use std::collections::HashMap;

use darkfi_serial::{deserialize, serialize};

use crate::{tx::Transaction, Error, Result};

const SLED_TX_TREE: &[u8] = b"_transactions";
const SLED_PENDING_TX_TREE: &[u8] = b"_pending_transactions";
const SLED_PENDING_TX_ORDER_TREE: &[u8] = b"_pending_transactions_order";

/// The `TxStore` is a `sled` tree storing all the blockchain's
/// transactions where the key is the transaction hash, and the value is
/// the serialized transaction.
#[derive(Clone)]
pub struct TxStore(sled::Tree);

impl TxStore {
    /// Opens a new or existing `TxStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_TX_TREE)?;
        Ok(Self(tree))
    }

    /// Insert a slice of [`Transaction`] into the txstore. With sled, the
    /// operation is done as a batch.
    /// The transactions are hashed with BLAKE3 and this hash is used as
    /// the key, while the value is the serialized [`Transaction`] itself.
    /// On success, the function returns the transaction hashes in the same
    /// order as the input transactions.
    pub fn insert(&self, transactions: &[Transaction]) -> Result<Vec<blake3::Hash>> {
        let mut ret = Vec::with_capacity(transactions.len());
        let mut batch = sled::Batch::default();

        for tx in transactions {
            let serialized = serialize(tx);
            let tx_hash = blake3::hash(&serialized);
            batch.insert(tx_hash.as_bytes(), serialized);
            ret.push(tx_hash);
        }

        self.0.apply_batch(batch)?;
        Ok(ret)
    }

    /// Check if the txstore contains a given transaction hash.
    pub fn contains(&self, tx_hash: &blake3::Hash) -> Result<bool> {
        Ok(self.0.contains_key(tx_hash.as_bytes())?)
    }

    /// Fetch given tx hashes from the txstore.
    /// The resulting vector contains `Option`, which is `Some` if the tx
    /// was found in the txstore, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one block was not found.
    pub fn get(
        &self,
        tx_hashes: &[blake3::Hash],
        strict: bool,
    ) -> Result<Vec<Option<Transaction>>> {
        let mut ret = Vec::with_capacity(tx_hashes.len());

        for tx_hash in tx_hashes {
            if let Some(found) = self.0.get(tx_hash.as_bytes())? {
                let tx = deserialize(&found)?;
                ret.push(Some(tx));
            } else {
                if strict {
                    let s = tx_hash.to_hex().as_str().to_string();
                    return Err(Error::TransactionNotFound(s))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Retrieve all transactions from the txstore in the form of a tuple
    /// (`tx_hash`, `tx`).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<(blake3::Hash, Transaction)>> {
        let mut txs = vec![];

        for tx in self.0.iter() {
            let (key, value) = tx.unwrap();
            let hash_bytes: [u8; 32] = key.as_ref().try_into().unwrap();
            let tx = deserialize(&value)?;
            txs.push((hash_bytes.into(), tx));
        }

        Ok(txs)
    }

    /// Retrieve records count
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.len() == 0
    }
}

/// The `PendingTxStore` is a `sled` tree storing all the node pending
/// transactions where the key is the transaction hash, and the value is
/// the serialized transaction.
#[derive(Clone)]
pub struct PendingTxStore(sled::Tree);

impl PendingTxStore {
    /// Opens a new or existing `PendingTxStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_PENDING_TX_TREE)?;
        Ok(Self(tree))
    }

    /// Insert a slice of [`Transaction`] into the pending tx store.
    /// With sled, the operation is done as a batch.
    /// The transactions are hashed with BLAKE3 and this hash is used as
    /// the key, while the value is the serialized [`Transaction`] itself.
    /// On success, the function returns the transaction hashes in the same
    /// order as the input transactions.
    pub fn insert(&self, transactions: &[Transaction]) -> Result<Vec<blake3::Hash>> {
        let mut ret = Vec::with_capacity(transactions.len());
        let mut batch = sled::Batch::default();

        for tx in transactions {
            let serialized = serialize(tx);
            let tx_hash = blake3::hash(&serialized);
            batch.insert(tx_hash.as_bytes(), serialized);
            ret.push(tx_hash);
        }

        self.0.apply_batch(batch)?;
        Ok(ret)
    }

    /// Check if the pending tx store contains a given transaction hash.
    pub fn contains(&self, tx_hash: &blake3::Hash) -> Result<bool> {
        Ok(self.0.contains_key(tx_hash.as_bytes())?)
    }

    /// Retrieve all transactions from the pending tx store in the form of
    /// a HashMap with key the transaction hash and value the transaction
    /// itself.
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<HashMap<blake3::Hash, Transaction>> {
        let mut txs = HashMap::new();

        for tx in self.0.iter() {
            let (key, value) = tx.unwrap();
            let hash_bytes: [u8; 32] = key.as_ref().try_into().unwrap();
            let tx = deserialize(&value)?;
            txs.insert(hash_bytes.into(), tx);
        }

        Ok(txs)
    }

    /// Remove a slice of [`blake3::Hash`] from the pending tx store.
    /// With sled, the operation is done as a batch.
    pub fn remove(&self, txs_hashes: &[blake3::Hash]) -> Result<()> {
        let mut batch = sled::Batch::default();

        for tx_hash in txs_hashes {
            batch.remove(tx_hash.as_bytes());
        }

        self.0.apply_batch(batch)?;
        Ok(())
    }
}

/// The `PendingTxOrderStore` is a `sled` tree storing the order of all
/// the node pending transactions where the key is an incremental value,
/// and the value is the serialized transaction.
#[derive(Clone)]
pub struct PendingTxOrderStore(sled::Tree);

impl PendingTxOrderStore {
    /// Opens a new or existing `PendingTxOrderStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_PENDING_TX_ORDER_TREE)?;
        Ok(Self(tree))
    }

    /// Insert a slice of [`blake3::Hash`] into the pending tx order store.
    /// With sled, the operation is done as a batch.
    pub fn insert(&self, txs_hashes: &[blake3::Hash]) -> Result<()> {
        let mut batch = sled::Batch::default();

        let mut next_index = match self.0.last()? {
            Some(n) => {
                let prev_bytes: [u8; 8] = n.0.as_ref().try_into().unwrap();
                let prev = u64::from_be_bytes(prev_bytes);
                prev + 1
            }
            None => 0,
        };

        for txs_hash in txs_hashes {
            batch.insert(&next_index.to_be_bytes(), txs_hash.as_bytes());
            next_index += 1;
        }

        self.0.apply_batch(batch)?;
        Ok(())
    }

    /// Retrieve all transactions from the pending tx order store in the form
    /// of a tuple (`u64`, `blake3::Hash`).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<(u64, blake3::Hash)>> {
        let mut txs = vec![];

        for tx in self.0.iter() {
            let (key, value) = tx.unwrap();
            let index_bytes: [u8; 8] = key.as_ref().try_into().unwrap();
            let hash_bytes: [u8; 32] = value.as_ref().try_into().unwrap();
            let index = u64::from_be_bytes(index_bytes);
            let hash = blake3::Hash::from(hash_bytes);
            txs.push((index, hash));
        }

        Ok(txs)
    }

    /// Remove a slice of [`u64`] from the pending tx order store.
    /// With sled, the operation is done as a batch.
    pub fn remove(&self, indexes: &[u64]) -> Result<()> {
        let mut batch = sled::Batch::default();

        for index in indexes {
            batch.remove(&index.to_be_bytes());
        }

        self.0.apply_batch(batch)?;
        Ok(())
    }
}
