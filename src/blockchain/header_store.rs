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

use darkfi_sdk::crypto::{MerkleNode, MerkleTree};

#[cfg(feature = "async-serial")]
use darkfi_serial::async_trait;
use darkfi_serial::{deserialize, serialize, Encodable, SerialDecodable, SerialEncodable};

use crate::{util::time::Timestamp, Error, Result};

use super::{block_store::BLOCK_VERSION, parse_record, SledDbOverlayPtr};

/// This struct represents a tuple of the form (version, previous, epoch, slot, timestamp, merkle_root).
#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Header {
    /// Block version
    pub version: u8,
    /// Previous block hash
    pub previous: blake3::Hash,
    /// Epoch
    pub epoch: u64,
    /// Slot UID
    pub slot: u64,
    /// Block creation timestamp
    pub timestamp: Timestamp,
    /// Root of the transaction hashes merkle tree
    pub root: MerkleNode,
}

impl Header {
    pub fn new(
        previous: blake3::Hash,
        epoch: u64,
        slot: u64,
        timestamp: Timestamp,
        root: MerkleNode,
    ) -> Self {
        let version = BLOCK_VERSION;
        Self { version, previous, epoch, slot, timestamp, root }
    }

    /// Calculate the header hash
    pub fn headerhash(&self) -> Result<blake3::Hash> {
        let mut hasher = blake3::Hasher::new();
        self.encode(&mut hasher)?;
        Ok(hasher.finalize())
    }
}

impl Default for Header {
    /// Represents the genesis header on current timestamp
    fn default() -> Self {
        Header::new(
            blake3::hash(b"Let there be dark!"),
            0,
            0,
            Timestamp::current_time(),
            MerkleTree::new(100).root(0).unwrap(),
        )
    }
}

/// [`Header`] sled tree
const SLED_HEADER_TREE: &[u8] = b"_headers";

/// The `HeaderStore` is a `sled` tree storing all the blockchain's blocks' headers
/// where the key is the headers' hash, and value is the serialized header.
#[derive(Clone)]
pub struct HeaderStore(pub sled::Tree);

impl HeaderStore {
    /// Opens a new or existing `HeaderStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_HEADER_TREE)?;
        Ok(Self(tree))
    }

    /// Insert a slice of [`Header`] into the blockstore.
    pub fn insert(&self, headers: &[Header]) -> Result<Vec<blake3::Hash>> {
        let (batch, ret) = self.insert_batch(headers)?;
        self.0.apply_batch(batch)?;
        Ok(ret)
    }

    /// Generate the sled batch corresponding to an insert, so caller
    /// can handle the write operation.
    /// The headers are hashed with BLAKE3 and this header hash is used as
    /// the key, while value is the serialized [`Header`] itself.
    /// On success, the function returns the header hashes in the same
    /// order, along with the corresponding operation batch.
    pub fn insert_batch(&self, headers: &[Header]) -> Result<(sled::Batch, Vec<blake3::Hash>)> {
        let mut ret = Vec::with_capacity(headers.len());
        let mut batch = sled::Batch::default();

        for header in headers {
            let serialized = serialize(header);
            let headerhash = blake3::hash(&serialized);
            batch.insert(headerhash.as_bytes(), serialized);
            ret.push(headerhash);
        }

        Ok((batch, ret))
    }

    /// Check if the headerstore contains a given headerhash.
    pub fn contains(&self, headerhash: &blake3::Hash) -> Result<bool> {
        Ok(self.0.contains_key(headerhash.as_bytes())?)
    }

    /// Fetch given headerhashes from the headerstore.
    /// The resulting vector contains `Option`, which is `Some` if the header
    /// was found in the headerstore, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one header was not found.
    pub fn get(&self, headerhashes: &[blake3::Hash], strict: bool) -> Result<Vec<Option<Header>>> {
        let mut ret = Vec::with_capacity(headerhashes.len());

        for hash in headerhashes {
            if let Some(found) = self.0.get(hash.as_bytes())? {
                let header = deserialize(&found)?;
                ret.push(Some(header));
            } else {
                if strict {
                    let s = hash.to_hex().as_str().to_string();
                    return Err(Error::HeaderNotFound(s))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Retrieve all headers from the headerstore in the form of a tuple
    /// (`headerhash`, `header`).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<(blake3::Hash, Header)>> {
        let mut headers = vec![];

        for header in self.0.iter() {
            headers.push(parse_record(header.unwrap())?);
        }

        Ok(headers)
    }
}

/// Overlay structure over a [`HeaderStore`] instance.
pub struct HeaderStoreOverlay(SledDbOverlayPtr);

impl HeaderStoreOverlay {
    pub fn new(overlay: &SledDbOverlayPtr) -> Result<Self> {
        overlay.lock().unwrap().open_tree(SLED_HEADER_TREE)?;
        Ok(Self(overlay.clone()))
    }

    /// Insert a slice of [`Header`] into the overlay.
    /// The headers are hashed with BLAKE3 and this headerhash is used as
    /// the key, while value is the serialized [`Header`] itself.
    /// On success, the function returns the header hashes in the same order.
    pub fn insert(&self, headers: &[Header]) -> Result<Vec<blake3::Hash>> {
        let mut ret = Vec::with_capacity(headers.len());
        let mut lock = self.0.lock().unwrap();

        for header in headers {
            let serialized = serialize(header);
            let headerhash = blake3::hash(&serialized);
            lock.insert(SLED_HEADER_TREE, headerhash.as_bytes(), &serialized)?;
            ret.push(headerhash);
        }

        Ok(ret)
    }

    /// Fetch given headerhashes from the overlay.
    /// The resulting vector contains `Option`, which is `Some` if the header
    /// was found in the overlay, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one header was not found.
    pub fn get(&self, headerhashes: &[blake3::Hash], strict: bool) -> Result<Vec<Option<Header>>> {
        let mut ret = Vec::with_capacity(headerhashes.len());
        let lock = self.0.lock().unwrap();

        for hash in headerhashes {
            if let Some(found) = lock.get(SLED_HEADER_TREE, hash.as_bytes())? {
                let header = deserialize(&found)?;
                ret.push(Some(header));
            } else {
                if strict {
                    let s = hash.to_hex().as_str().to_string();
                    return Err(Error::HeaderNotFound(s))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }
}
