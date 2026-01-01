/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use darkfi_sdk::tx::TransactionHash;
use darkfi_serial::{deserialize, serialize};
use sled_overlay::{
    serial::{parse_record, parse_u64_key_record},
    sled,
};

use crate::{tx::Transaction, Error, Result};

use super::SledDbOverlayPtr;

pub const SLED_TX_TREE: &[u8] = b"_transactions";
pub const SLED_TX_LOCATION_TREE: &[u8] = b"_transaction_location";
pub const SLED_PENDING_TX_TREE: &[u8] = b"_pending_transactions";
pub const SLED_PENDING_TX_ORDER_TREE: &[u8] = b"_pending_transactions_order";

/// The `TxStore` is a structure representing all `sled` trees related
/// to storing the blockchain's transactions information.
#[derive(Clone)]
pub struct TxStore {
    /// Main `sled` tree, storing all the blockchain's transactions, where
    /// the key is the transaction hash, and the value is the serialized
    /// transaction.
    pub main: sled::Tree,
    /// The `sled` tree storing the location of the blockchain's transactions
    /// locations, where the key is the transaction hash, and the value is a
    /// serialized tuple containing the height and the vector index of the
    /// block the transaction is included.
    pub location: sled::Tree,
    /// The `sled` tree storing all the node pending transactions, where
    /// the key is the transaction hash, and the value is the serialized
    /// transaction.
    pub pending: sled::Tree,
    /// The `sled` tree storing the order of all the node pending transactions,
    /// where the key is an incremental value, and the value is the serialized
    /// transaction.
    pub pending_order: sled::Tree,
}

impl TxStore {
    /// Opens a new or existing `TxStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let main = db.open_tree(SLED_TX_TREE)?;
        let location = db.open_tree(SLED_TX_LOCATION_TREE)?;
        let pending = db.open_tree(SLED_PENDING_TX_TREE)?;
        let pending_order = db.open_tree(SLED_PENDING_TX_ORDER_TREE)?;
        Ok(Self { main, location, pending, pending_order })
    }

    /// Insert a slice of [`Transaction`] into the store's main tree.
    pub fn insert(&self, transactions: &[Transaction]) -> Result<Vec<TransactionHash>> {
        let (batch, ret) = self.insert_batch(transactions);
        self.main.apply_batch(batch)?;
        Ok(ret)
    }

    /// Insert a slice of [`TransactionHash`] into the store's location tree.
    pub fn insert_location(&self, txs_hashes: &[TransactionHash], block_height: u32) -> Result<()> {
        let batch = self.insert_batch_location(txs_hashes, block_height);
        self.location.apply_batch(batch)?;
        Ok(())
    }

    /// Insert a slice of [`Transaction`] into the store's pending txs tree.
    pub fn insert_pending(&self, transactions: &[Transaction]) -> Result<Vec<TransactionHash>> {
        let (batch, ret) = self.insert_batch_pending(transactions);
        self.pending.apply_batch(batch)?;
        Ok(ret)
    }

    /// Insert a slice of [`TransactionHash`] into the store's pending txs order tree.
    pub fn insert_pending_order(&self, txs_hashes: &[TransactionHash]) -> Result<()> {
        let batch = self.insert_batch_pending_order(txs_hashes)?;
        self.pending_order.apply_batch(batch)?;
        Ok(())
    }

    /// Generate the sled batch corresponding to an insert to the main tree,
    /// so caller can handle the write operation.
    /// The transactions are hashed with BLAKE3 and this hash is used as
    /// the key, while the value is the serialized [`Transaction`] itself.
    /// On success, the function returns the transaction hashes in the same
    /// order as the input transactions, along with the corresponding operation
    /// batch.
    pub fn insert_batch(
        &self,
        transactions: &[Transaction],
    ) -> (sled::Batch, Vec<TransactionHash>) {
        let mut ret = Vec::with_capacity(transactions.len());
        let mut batch = sled::Batch::default();

        for tx in transactions {
            let tx_hash = tx.hash();
            batch.insert(tx_hash.inner(), serialize(tx));
            ret.push(tx_hash);
        }

        (batch, ret)
    }

    /// Generate the sled batch corresponding to an insert to the location tree,
    /// so caller can handle the write operation.
    /// The location tuple is built using the index of each transaction has in
    /// the slice, along with the provided block height
    pub fn insert_batch_location(
        &self,
        txs_hashes: &[TransactionHash],
        block_height: u32,
    ) -> sled::Batch {
        let mut batch = sled::Batch::default();

        for (index, tx_hash) in txs_hashes.iter().enumerate() {
            let serialized = serialize(&(block_height, index as u16));
            batch.insert(tx_hash.inner(), serialized);
        }

        batch
    }

    /// Generate the sled batch corresponding to an insert to the pending txs tree,
    /// so caller can handle the write operation.
    /// The transactions are hashed with BLAKE3 and this hash is used as
    /// the key, while the value is the serialized [`Transaction`] itself.
    /// On success, the function returns the transaction hashes in the same
    /// order as the input transactions, along with the corresponding operation
    /// batch.
    pub fn insert_batch_pending(
        &self,
        transactions: &[Transaction],
    ) -> (sled::Batch, Vec<TransactionHash>) {
        let mut ret = Vec::with_capacity(transactions.len());
        let mut batch = sled::Batch::default();

        for tx in transactions {
            let tx_hash = tx.hash();
            batch.insert(tx_hash.inner(), serialize(tx));
            ret.push(tx_hash);
        }

        (batch, ret)
    }

    /// Generate the sled batch corresponding to an insert to the pending txs
    /// order tree, so caller can handle the write operation.
    pub fn insert_batch_pending_order(&self, tx_hashes: &[TransactionHash]) -> Result<sled::Batch> {
        let mut batch = sled::Batch::default();

        let mut next_index = match self.pending_order.last()? {
            Some(n) => {
                let prev_bytes: [u8; 8] = n.0.as_ref().try_into().unwrap();
                let prev = u64::from_be_bytes(prev_bytes);
                prev + 1
            }
            None => 0,
        };

        for tx_hash in tx_hashes {
            batch.insert(&next_index.to_be_bytes(), tx_hash.inner());
            next_index += 1;
        }

        Ok(batch)
    }

    /// Check if the store's main tree contains a given transaction hash.
    pub fn contains(&self, tx_hash: &TransactionHash) -> Result<bool> {
        Ok(self.main.contains_key(tx_hash.inner())?)
    }

    /// Check if the store's pending txs tree contains a given transaction hash.
    pub fn contains_pending(&self, tx_hash: &TransactionHash) -> Result<bool> {
        Ok(self.pending.contains_key(tx_hash.inner())?)
    }

    /// Fetch given tx hashes from the store's main tree.
    /// The resulting vector contains `Option`, which is `Some` if the tx
    /// was found in the txstore, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one tx was not found.
    pub fn get(
        &self,
        tx_hashes: &[TransactionHash],
        strict: bool,
    ) -> Result<Vec<Option<Transaction>>> {
        let mut ret = Vec::with_capacity(tx_hashes.len());

        for tx_hash in tx_hashes {
            if let Some(found) = self.main.get(tx_hash.inner())? {
                let tx = deserialize(&found)?;
                ret.push(Some(tx));
                continue
            }
            if strict {
                return Err(Error::TransactionNotFound(tx_hash.as_string()))
            }
            ret.push(None);
        }

        Ok(ret)
    }

    /// Fetch given tx hashes locations from the store's location tree.
    /// The resulting vector contains `Option`, which is `Some` if the tx
    /// was found in the txstore, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one tx was not found.
    pub fn get_location(
        &self,
        tx_hashes: &[TransactionHash],
        strict: bool,
    ) -> Result<Vec<Option<(u32, u16)>>> {
        let mut ret = Vec::with_capacity(tx_hashes.len());

        for tx_hash in tx_hashes {
            if let Some(found) = self.location.get(tx_hash.inner())? {
                let location = deserialize(&found)?;
                ret.push(Some(location));
                continue
            }
            if strict {
                return Err(Error::TransactionNotFound(tx_hash.as_string()))
            }
            ret.push(None);
        }

        Ok(ret)
    }

    /// Fetch given tx hashes from the store's pending txs tree.
    /// The resulting vector contains `Option`, which is `Some` if the tx
    /// was found in the pending tx store, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one tx was not found.
    pub fn get_pending(
        &self,
        tx_hashes: &[TransactionHash],
        strict: bool,
    ) -> Result<Vec<Option<Transaction>>> {
        let mut ret = Vec::with_capacity(tx_hashes.len());

        for tx_hash in tx_hashes {
            if let Some(found) = self.pending.get(tx_hash.inner())? {
                let tx = deserialize(&found)?;
                ret.push(Some(tx));
                continue
            }
            if strict {
                return Err(Error::TransactionNotFound(tx_hash.as_string()))
            }
            ret.push(None);
        }

        Ok(ret)
    }

    /// Retrieve all transactions from the store's main tree in the form of
    /// a tuple (`tx_hash`, `tx`).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<(TransactionHash, Transaction)>> {
        let mut txs = vec![];

        for tx in self.main.iter() {
            txs.push(parse_record(tx.unwrap())?);
        }

        Ok(txs)
    }

    /// Retrieve all transactions locations from the store's location tree in
    /// the form of a tuple (`tx_hash`, (`block_height`, `index`)).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all_location(&self) -> Result<Vec<(TransactionHash, (u32, u16))>> {
        let mut locations = vec![];

        for location in self.location.iter() {
            locations.push(parse_record(location.unwrap())?);
        }

        Ok(locations)
    }

    /// Retrieve all transactions from the store's pending txs tree in the
    /// form of a HashMap with key the transaction hash and value the
    /// transaction itself.
    /// Be careful as this will try to load everything in memory.
    pub fn get_all_pending(&self) -> Result<HashMap<TransactionHash, Transaction>> {
        let mut txs = HashMap::new();

        for tx in self.pending.iter() {
            let (key, value) = parse_record(tx.unwrap())?;
            txs.insert(key, value);
        }

        Ok(txs)
    }

    /// Retrieve all transactions from the store's pending txs order tree in
    /// the form of a tuple (`u64`, `TransactionHash`).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all_pending_order(&self) -> Result<Vec<(u64, TransactionHash)>> {
        let mut txs = vec![];

        for tx in self.pending_order.iter() {
            txs.push(parse_u64_key_record(tx.unwrap())?);
        }

        Ok(txs)
    }

    /// Fetch n transactions after given order([order..order+n)). In the iteration,
    /// if a transaction order is not found, the iteration stops and the function
    /// returns what it has found so far in the store's pending order tree.
    pub fn get_after_pending(&self, order: u64, n: usize) -> Result<(u64, Vec<Transaction>)> {
        let mut hashes = vec![];

        // First we grab the order itself
        if let Some(found) = self.pending_order.get(order.to_be_bytes())? {
            let hash = deserialize(&found)?;
            hashes.push(hash);
        }

        // Then whatever comes after it
        let mut key = order;
        let mut counter = 0;
        while counter < n {
            if let Some(found) = self.pending_order.get_gt(key.to_be_bytes())? {
                let (order, hash) = parse_u64_key_record(found)?;
                key = order;
                hashes.push(hash);
                counter += 1;
                continue
            }
            break
        }

        if hashes.is_empty() {
            return Ok((key, vec![]))
        }

        let txs = self.get_pending(&hashes, true)?.iter().map(|tx| tx.clone().unwrap()).collect();

        Ok((key, txs))
    }

    /// Retrieve records count of the store's main tree.
    pub fn len(&self) -> usize {
        self.main.len()
    }

    /// Check if the store's main tree is empty.
    pub fn is_empty(&self) -> bool {
        self.main.is_empty()
    }

    /// Remove a slice of [`TransactionHash`] from the store's pending txs tree.
    pub fn remove_pending(&self, txs_hashes: &[TransactionHash]) -> Result<()> {
        let batch = self.remove_batch_pending(txs_hashes);
        self.pending.apply_batch(batch)?;
        Ok(())
    }

    /// Remove a slice of [`u64`] from the store's pending txs order tree.
    pub fn remove_pending_order(&self, indexes: &[u64]) -> Result<()> {
        let batch = self.remove_batch_pending_order(indexes);
        self.pending_order.apply_batch(batch)?;
        Ok(())
    }

    /// Generate the sled batch corresponding to a remove from the store's pending
    /// txs tree, so caller can handle the write operation.
    pub fn remove_batch_pending(&self, txs_hashes: &[TransactionHash]) -> sled::Batch {
        let mut batch = sled::Batch::default();

        for tx_hash in txs_hashes {
            batch.remove(tx_hash.inner());
        }

        batch
    }

    /// Generate the sled batch corresponding to a remove from the store's pending
    /// txs order tree, so caller can handle the write operation.
    pub fn remove_batch_pending_order(&self, indexes: &[u64]) -> sled::Batch {
        let mut batch = sled::Batch::default();

        for index in indexes {
            batch.remove(&index.to_be_bytes());
        }

        batch
    }
}

/// Overlay structure over a [`TxStore`] instance.
pub struct TxStoreOverlay(SledDbOverlayPtr);

impl TxStoreOverlay {
    pub fn new(overlay: &SledDbOverlayPtr) -> Result<Self> {
        overlay.lock().unwrap().open_tree(SLED_TX_TREE, true)?;
        overlay.lock().unwrap().open_tree(SLED_TX_LOCATION_TREE, true)?;
        Ok(Self(overlay.clone()))
    }

    /// Insert a slice of [`Transaction`] into the overlay's main tree.
    /// The transactions are hashed with BLAKE3 and this hash is used as
    /// the key, while the value is the serialized [`Transaction`] itself.
    /// On success, the function returns the transaction hashes in the same
    /// order as the input transactions.
    pub fn insert(&self, transactions: &[Transaction]) -> Result<Vec<TransactionHash>> {
        let mut ret = Vec::with_capacity(transactions.len());
        let mut lock = self.0.lock().unwrap();

        for tx in transactions {
            let tx_hash = tx.hash();
            lock.insert(SLED_TX_TREE, tx_hash.inner(), &serialize(tx))?;
            ret.push(tx_hash);
        }

        Ok(ret)
    }

    /// Insert a slice of [`TransactionHash`] into the overlay's location tree.
    /// The location tuple is built using the index of each transaction hash
    /// in the slice, along with the provided block height
    pub fn insert_location(&self, txs_hashes: &[TransactionHash], block_height: u32) -> Result<()> {
        let mut lock = self.0.lock().unwrap();

        for (index, tx_hash) in txs_hashes.iter().enumerate() {
            let serialized = serialize(&(block_height, index as u16));
            lock.insert(SLED_TX_LOCATION_TREE, tx_hash.inner(), &serialized)?;
        }

        Ok(())
    }

    /// Fetch given tx hashes from the overlay's main tree.
    /// The resulting vector contains `Option`, which is `Some` if the tx
    /// was found in the overlay, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one tx was not found.
    pub fn get(
        &self,
        tx_hashes: &[TransactionHash],
        strict: bool,
    ) -> Result<Vec<Option<Transaction>>> {
        let mut ret = Vec::with_capacity(tx_hashes.len());
        let lock = self.0.lock().unwrap();

        for tx_hash in tx_hashes {
            if let Some(found) = lock.get(SLED_TX_TREE, tx_hash.inner())? {
                let tx = deserialize(&found)?;
                ret.push(Some(tx));
                continue
            }
            if strict {
                return Err(Error::TransactionNotFound(tx_hash.as_string()))
            }
            ret.push(None);
        }

        Ok(ret)
    }

    /// Fetch given tx hash from the overlay's main tree. This function uses
    /// raw bytes as input and doesn't deserialize the retrieved value.
    /// The resulting vector contains `Option`, which is `Some` if the tx
    /// was found in the overlay, and otherwise it is `None`, if it has not.
    pub fn get_raw(&self, tx_hash: &[u8; 32]) -> Result<Option<Vec<u8>>> {
        let lock = self.0.lock().unwrap();
        if let Some(found) = lock.get(SLED_TX_TREE, tx_hash)? {
            return Ok(Some(found.to_vec()))
        }
        Ok(None)
    }

    /// Fetch given tx hash location from the overlay's location tree.
    /// This function uses raw bytes as input and doesn't deserialize the
    /// retrieved value. The resulting vector contains `Option`, which is
    /// `Some` if the location was found in the overlay, and otherwise it
    /// is `None`, if it has not.
    pub fn get_location_raw(&self, tx_hash: &[u8; 32]) -> Result<Option<Vec<u8>>> {
        let lock = self.0.lock().unwrap();
        if let Some(found) = lock.get(SLED_TX_LOCATION_TREE, tx_hash)? {
            return Ok(Some(found.to_vec()))
        }
        Ok(None)
    }
}
