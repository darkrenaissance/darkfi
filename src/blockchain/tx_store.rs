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

use darkfi_serial::{deserialize, serialize};

use crate::{tx::Transaction, Error, Result};

const SLED_TX_TREE: &[u8] = b"_transactions";
const SLED_ERRONEOUS_TX_TREE: &[u8] = b"_erroneous_transactions";

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
}

/// The `ErroneousTxStore` is a `sled` tree storing all the blockchain's
/// erroneous transactions where the key is the transaction hash, and the value is
/// the serialized transaction.
#[derive(Clone)]
pub struct ErroneousTxStore(sled::Tree);

impl ErroneousTxStore {
    /// Opens a new or existing `ErroneousTxStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_ERRONEOUS_TX_TREE)?;
        Ok(Self(tree))
    }

    /// Insert a slice of [`Transaction`] into the erroneoustxstore. With sled, the
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

    /// Check if the erroneoustxstore contains a given transaction hash.
    pub fn contains(&self, tx_hash: &blake3::Hash) -> Result<bool> {
        Ok(self.0.contains_key(tx_hash.as_bytes())?)
    }

    /// Fetch given erroneous tx hashes from the erroneoustxstore.
    /// The resulting vector contains `Option`, which is `Some` if the tx
    /// was found in the erroneoustxstore, and otherwise it is `None`, if it has not.
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

    /// Retrieve all erroneous transactions from the erroneoustxstore
    /// in the form of a tuple (`tx_hash`, `tx`).
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
}
