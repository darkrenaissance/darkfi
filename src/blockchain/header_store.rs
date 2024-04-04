/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use std::fmt;

use darkfi_sdk::{blockchain::block_version, crypto::MerkleTree, AsHex};

#[cfg(feature = "async-serial")]
use darkfi_serial::async_trait;
use darkfi_serial::{deserialize, serialize, Encodable, SerialDecodable, SerialEncodable};

use crate::{util::time::Timestamp, Error, Result};

use super::{parse_record, SledDbOverlayPtr};

#[derive(Copy, Clone, Debug, Eq, PartialEq, SerialEncodable, SerialDecodable)]
// We have to introduce a type rather than using an alias so we can restrict API access
pub struct HeaderHash(pub [u8; 32]);

impl HeaderHash {
    pub fn new(data: [u8; 32]) -> Self {
        Self(data)
    }

    #[inline]
    pub fn inner(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn as_string(&self) -> String {
        self.0.hex().to_string()
    }
}

impl fmt::Display for HeaderHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.hex())
    }
}

/// This struct represents a tuple of the form (version, previous, height, timestamp, nonce, merkle_tree).
#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Header {
    /// Block version
    pub version: u8,
    /// Previous block hash
    pub previous: HeaderHash,
    /// Block height
    pub height: u64,
    /// Block creation timestamp
    pub timestamp: Timestamp,
    /// The block's nonce. This value changes arbitrarily with mining.
    pub nonce: u64,
    /// Merkle tree of the transactions hashes contained in this block
    pub tree: MerkleTree,
}

impl Header {
    pub fn new(previous: HeaderHash, height: u64, timestamp: Timestamp, nonce: u64) -> Self {
        let version = block_version(height);
        let tree = MerkleTree::new(1);
        Self { version, previous, height, timestamp, nonce, tree }
    }

    /// Compute the header's hash
    pub fn hash(&self) -> HeaderHash {
        let mut hasher = blake3::Hasher::new();

        // Blake3 hasher .update() method never fails.
        // This call returns a Result due to how the Write trait is specified.
        // Calling unwrap() here should be safe.
        self.version.encode(&mut hasher).expect("blake3 hasher");
        self.previous.encode(&mut hasher).expect("blake3 hasher");
        self.height.encode(&mut hasher).expect("blake3 hasher");
        self.timestamp.encode(&mut hasher).expect("blake3 hasher");
        self.nonce.encode(&mut hasher).expect("blake3 hasher");
        self.tree.root(0).unwrap().encode(&mut hasher).expect("blake3 hasher");

        HeaderHash(hasher.finalize().into())
    }
}

impl Default for Header {
    /// Represents the genesis header on current timestamp
    fn default() -> Self {
        Header::new(
            HeaderHash::new(blake3::hash(b"Let there be dark!").into()),
            0,
            Timestamp::current_time(),
            0,
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
    pub fn insert(&self, headers: &[Header]) -> Result<Vec<HeaderHash>> {
        let (batch, ret) = self.insert_batch(headers);
        self.0.apply_batch(batch)?;
        Ok(ret)
    }

    /// Generate the sled batch corresponding to an insert, so caller
    /// can handle the write operation.
    /// The header's hash() function output is used as the key,
    /// while value is the serialized [`Header`] itself.
    /// On success, the function returns the header hashes in the same
    /// order, along with the corresponding operation batch.
    pub fn insert_batch(&self, headers: &[Header]) -> (sled::Batch, Vec<HeaderHash>) {
        let mut ret = Vec::with_capacity(headers.len());
        let mut batch = sled::Batch::default();

        for header in headers {
            let headerhash = header.hash();
            batch.insert(headerhash.inner(), serialize(header));
            ret.push(headerhash);
        }

        (batch, ret)
    }

    /// Check if the headerstore contains a given headerhash.
    pub fn contains(&self, headerhash: &HeaderHash) -> Result<bool> {
        Ok(self.0.contains_key(headerhash.inner())?)
    }

    /// Fetch given headerhashes from the headerstore.
    /// The resulting vector contains `Option`, which is `Some` if the header
    /// was found in the headerstore, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one header was not found.
    pub fn get(&self, headerhashes: &[HeaderHash], strict: bool) -> Result<Vec<Option<Header>>> {
        let mut ret = Vec::with_capacity(headerhashes.len());

        for hash in headerhashes {
            if let Some(found) = self.0.get(hash.inner())? {
                let header = deserialize(&found)?;
                ret.push(Some(header));
                continue
            }
            if strict {
                return Err(Error::HeaderNotFound(hash.inner().hex()))
            }
            ret.push(None);
        }

        Ok(ret)
    }

    /// Retrieve all headers from the headerstore in the form of a tuple
    /// (`headerhash`, `header`).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<(HeaderHash, Header)>> {
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
    /// The header's hash() function output is used as the key,
    /// while value is the serialized [`Header`] itself.
    /// On success, the function returns the header hashes in the same order.
    pub fn insert(&self, headers: &[Header]) -> Result<Vec<HeaderHash>> {
        let mut ret = Vec::with_capacity(headers.len());
        let mut lock = self.0.lock().unwrap();

        for header in headers {
            let headerhash = header.hash();
            lock.insert(SLED_HEADER_TREE, headerhash.inner(), &serialize(header))?;
            ret.push(headerhash);
        }

        Ok(ret)
    }

    /// Fetch given headerhashes from the overlay.
    /// The resulting vector contains `Option`, which is `Some` if the header
    /// was found in the overlay, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one header was not found.
    pub fn get(&self, headerhashes: &[HeaderHash], strict: bool) -> Result<Vec<Option<Header>>> {
        let mut ret = Vec::with_capacity(headerhashes.len());
        let lock = self.0.lock().unwrap();

        for hash in headerhashes {
            if let Some(found) = lock.get(SLED_HEADER_TREE, hash.inner())? {
                let header = deserialize(&found)?;
                ret.push(Some(header));
                continue
            }
            if strict {
                return Err(Error::HeaderNotFound(hash.inner().hex()))
            }
            ret.push(None);
        }

        Ok(ret)
    }
}
