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

use std::{fmt, str::FromStr};

use darkfi_sdk::{
    blockchain::block_version,
    crypto::{MerkleNode, MerkleTree},
    monotree::{Hash as StateHash, EMPTY_HASH},
};
#[cfg(feature = "async-serial")]
use darkfi_serial::{async_trait, FutAsyncWriteExt};
use darkfi_serial::{deserialize, serialize, Encodable, SerialDecodable, SerialEncodable};
use sled_overlay::sled;

use crate::{util::time::Timestamp, Error, Result};

use super::{
    monero::{extract_aux_merkle_root, MoneroPowData},
    parse_record, parse_u32_key_record, SledDbOverlayPtr,
};

/// Struct representing the Proof of Work used in a block.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
#[allow(clippy::large_enum_variant)]
pub enum PowData {
    /// Native DarkFi PoW
    DarkFi,
    /// Monero merge mining PoW
    Monero(MoneroPowData),
}

impl fmt::Display for PowData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::DarkFi => write!(f, "PoW: DarkFi"),
            Self::Monero(_) => write!(f, "PoW: Monero"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, SerialEncodable, SerialDecodable)]
// We have to introduce a type rather than using an alias so we can restrict API access.
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
        blake3::Hash::from_bytes(self.0).to_string()
    }

    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl FromStr for HeaderHash {
    type Err = Error;

    fn from_str(header_hash_str: &str) -> Result<Self> {
        Ok(Self(*blake3::Hash::from_str(header_hash_str)?.as_bytes()))
    }
}

impl fmt::Display for HeaderHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_string())
    }
}

/// This struct represents a tuple of the form (version, previous, height, timestamp, nonce, merkle_tree).
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct Header {
    /// Block version
    pub version: u8,
    /// Previous block hash
    pub previous: HeaderHash,
    /// Block height
    pub height: u32,
    /// The block's nonce. This value changes arbitrarily with mining.
    pub nonce: u32,
    /// Block creation timestamp
    pub timestamp: Timestamp,
    /// Merkle tree root of the transactions hashes contained in this block
    pub transactions_root: MerkleNode,
    /// Contracts states Monotree(SMT) root this block commits to
    pub state_root: StateHash,
    /// Block Proof of Work type
    pub pow_data: PowData,
}

impl Header {
    /// Generates a new header with default transactions and state root,
    /// using DarkFi native Proof of Work data.
    pub fn new(previous: HeaderHash, height: u32, nonce: u32, timestamp: Timestamp) -> Self {
        let version = block_version(height);
        let transactions_root = MerkleTree::new(1).root(0).unwrap();
        let state_root = *EMPTY_HASH;
        let pow_data = PowData::DarkFi;
        Self {
            version,
            previous,
            height,
            nonce,
            timestamp,
            transactions_root,
            state_root,
            pow_data,
        }
    }

    /// Compute the header's hash.
    pub fn hash(&self) -> HeaderHash {
        let mut hasher = blake3::Hasher::new();

        // Blake3 hasher .update() method never fails.
        // This call returns a Result due to how the Write trait is specified.
        // Calling unwrap() here should be safe.
        self.encode(&mut hasher).expect("blake3 hasher");

        HeaderHash(hasher.finalize().into())
    }

    /// Compute the header's template hash, which excludes its Proof of Work data.
    pub fn template_hash(&self) -> HeaderHash {
        let mut hasher = blake3::Hasher::new();

        // Blake3 hasher .update() method never fails.
        // This call returns a Result due to how the Write trait is specified.
        // Calling unwrap() here should be safe.
        self.version.encode(&mut hasher).expect("blake3 hasher");
        self.previous.encode(&mut hasher).expect("blake3 hasher");
        self.height.encode(&mut hasher).expect("blake3 hasher");
        self.timestamp.encode(&mut hasher).expect("blake3 hasher");
        self.nonce.encode(&mut hasher).expect("blake3 hasher");
        self.transactions_root.encode(&mut hasher).expect("blake3 hasher");
        self.state_root.encode(&mut hasher).expect("blake3 hasher");

        HeaderHash(hasher.finalize().into())
    }

    /// Validate PowData from the header.
    pub fn validate_powdata(&self) -> bool {
        match &self.pow_data {
            // For native DarkFi PoW, this is handled so we just return `true`.
            PowData::DarkFi => true,
            // For Monero PoW, we have to check a few things.
            PowData::Monero(powdata) => {
                if !powdata.is_coinbase_valid_merkle_root() {
                    return false
                }

                // Verify that MoneroPowData correctly corresponds to this header.
                let Ok(Some(merkle_root)) = extract_aux_merkle_root(&powdata.coinbase_tx_extra)
                else {
                    return false
                };

                let aux_hash = monero::Hash::from(self.template_hash().inner());
                powdata.aux_chain_merkle_proof.calculate_root(&aux_hash) == merkle_root
            }
        }
    }

    /// Create a block hashing blob from this header.
    pub fn to_block_hashing_blob(&self) -> Vec<u8> {
        // For XMRig, we need to pad the blob so that our nonce ends
        // up at byte offset 39.
        let mut blob = vec![0x00, 0x00];
        blob.extend_from_slice(&serialize(self));
        blob
    }
}

impl Default for Header {
    /// Represents the genesis header on current timestamp.
    fn default() -> Self {
        Header::new(
            HeaderHash::new(blake3::hash(b"Let there be dark!").into()),
            0u32,
            0u32,
            Timestamp::current_time(),
        )
    }
}

impl fmt::Display for Header {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = format!(
            "{} {{\n\t{}: {}\n\t{}: {}\n\t{}: {}\n\t{}: {}\n\t{}: {}\n\t{}: {}\n\t{}: {}\n\t{}: {}\n\t{}: {:?}\n}}",
            "Header",
            "Hash",
            self.hash(),
            "Version",
            self.version,
            "Previous",
            self.previous,
            "Height",
            self.height,
            "Timestamp",
            self.timestamp,
            "Nonce",
            self.nonce,
            "Transactions Root",
            self.transactions_root,
            "State Root",
            blake3::Hash::from_bytes(self.state_root),
            "Proof of Work data",
            self.pow_data,
        );

        write!(f, "{s}")
    }
}

pub const SLED_HEADER_TREE: &[u8] = b"_headers";
pub const SLED_SYNC_HEADER_TREE: &[u8] = b"_sync_headers";

/// The `HeaderStore` is a structure representing all `sled` trees related
/// to storing the blockchain's blocks's header information.
#[derive(Clone)]
pub struct HeaderStore {
    /// Main `sled` tree, storing all the blockchain's blocks' headers,
    /// where the key is the headers' hash, and value is the serialized header.
    pub main: sled::Tree,
    /// The `sled` tree storing all the node pending headers while syncing,
    /// where the key is the height number, and the value is the serialized
    /// header.
    pub sync: sled::Tree,
}

impl HeaderStore {
    /// Opens a new or existing `HeaderStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let main = db.open_tree(SLED_HEADER_TREE)?;
        let sync = db.open_tree(SLED_SYNC_HEADER_TREE)?;
        Ok(Self { main, sync })
    }

    /// Insert a slice of [`Header`] into the store's main tree.
    pub fn insert(&self, headers: &[Header]) -> Result<Vec<HeaderHash>> {
        let (batch, ret) = self.insert_batch(headers);
        self.main.apply_batch(batch)?;
        Ok(ret)
    }

    /// Insert a slice of [`Header`] into the store's sync tree.
    pub fn insert_sync(&self, headers: &[Header]) -> Result<()> {
        let batch = self.insert_batch_sync(headers);
        self.sync.apply_batch(batch)?;
        Ok(())
    }

    /// Generate the sled batch corresponding to an insert to the main
    /// tree, so caller can handle the write operation.
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

    /// Generate the sled batch corresponding to an insert to the sync
    /// tree, so caller can handle the write operation.
    /// The header height is used as the key, while value is the serialized
    /// [`Header`] itself.
    pub fn insert_batch_sync(&self, headers: &[Header]) -> sled::Batch {
        let mut batch = sled::Batch::default();

        for header in headers {
            batch.insert(&header.height.to_be_bytes(), serialize(header));
        }

        batch
    }

    /// Check if the store's main tree contains a given header hash.
    pub fn contains(&self, headerhash: &HeaderHash) -> Result<bool> {
        Ok(self.main.contains_key(headerhash.inner())?)
    }

    /// Fetch given header hashes from the store's main tree.
    /// The resulting vector contains `Option`, which is `Some` if the header
    /// was found in the store's main tree, and otherwise it is `None`, if it
    /// has not. The second parameter is a boolean which tells the function to
    /// fail in case at least one header was not found.
    pub fn get(&self, headerhashes: &[HeaderHash], strict: bool) -> Result<Vec<Option<Header>>> {
        let mut ret = Vec::with_capacity(headerhashes.len());

        for hash in headerhashes {
            if let Some(found) = self.main.get(hash.inner())? {
                let header = deserialize(&found)?;
                ret.push(Some(header));
                continue
            }
            if strict {
                return Err(Error::HeaderNotFound(hash.as_string()))
            }
            ret.push(None);
        }

        Ok(ret)
    }

    /// Retrieve all headers from the store's main tree in the form of a tuple
    /// (`headerhash`, `header`).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<(HeaderHash, Header)>> {
        let mut headers = vec![];

        for header in self.main.iter() {
            headers.push(parse_record(header.unwrap())?);
        }

        Ok(headers)
    }

    /// Retrieve all headers from the store's sync tree in the form of a tuple
    /// (`height`, `header`).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all_sync(&self) -> Result<Vec<(u32, Header)>> {
        let mut headers = vec![];

        for record in self.sync.iter() {
            headers.push(parse_u32_key_record(record.unwrap())?);
        }

        Ok(headers)
    }

    /// Fetch the fisrt header in the store's sync tree, based on the `Ord`
    /// implementation for `Vec<u8>`.
    pub fn get_first_sync(&self) -> Result<Option<Header>> {
        let Some(found) = self.sync.first()? else { return Ok(None) };
        let (_, header) = parse_u32_key_record(found)?;

        Ok(Some(header))
    }

    /// Fetch the last header in the store's sync tree, based on the `Ord`
    /// implementation for `Vec<u8>`.
    pub fn get_last_sync(&self) -> Result<Option<Header>> {
        let Some(found) = self.sync.last()? else { return Ok(None) };
        let (_, header) = parse_u32_key_record(found)?;

        Ok(Some(header))
    }

    /// Fetch n hashes after given height. In the iteration, if a header
    /// height is not found, the iteration stops and the function returns what
    /// it has found so far in the store's sync tree.
    pub fn get_after_sync(&self, height: u32, n: usize) -> Result<Vec<Header>> {
        let mut ret = vec![];

        let mut key = height;
        let mut counter = 0;
        while counter < n {
            if let Some(found) = self.sync.get_gt(key.to_be_bytes())? {
                let (height, hash) = parse_u32_key_record(found)?;
                key = height;
                ret.push(hash);
                counter += 1;
                continue
            }
            break
        }

        Ok(ret)
    }

    /// Retrieve store's sync tree records count.
    pub fn len_sync(&self) -> usize {
        self.sync.len()
    }

    /// Check if store's sync tree contains any records.
    pub fn is_empty_sync(&self) -> bool {
        self.sync.is_empty()
    }

    /// Remove a slice of [`u32`] from the store's sync tree.
    pub fn remove_sync(&self, heights: &[u32]) -> Result<()> {
        let batch = self.remove_batch_sync(heights);
        self.sync.apply_batch(batch)?;
        Ok(())
    }

    /// Remove all records from the store's sync tree.
    pub fn remove_all_sync(&self) -> Result<()> {
        let headers = self.get_all_sync()?;
        let heights = headers.iter().map(|h| h.0).collect::<Vec<u32>>();
        let batch = self.remove_batch_sync(&heights);
        self.sync.apply_batch(batch)?;
        Ok(())
    }

    /// Generate the sled batch corresponding to a remove from the store's sync
    /// tree, so caller can handle the write operation.
    pub fn remove_batch_sync(&self, heights: &[u32]) -> sled::Batch {
        let mut batch = sled::Batch::default();

        for height in heights {
            batch.remove(&height.to_be_bytes());
        }

        batch
    }
}

/// Overlay structure over a [`HeaderStore`] instance.
pub struct HeaderStoreOverlay(SledDbOverlayPtr);

impl HeaderStoreOverlay {
    pub fn new(overlay: &SledDbOverlayPtr) -> Result<Self> {
        overlay.lock().unwrap().open_tree(SLED_HEADER_TREE, true)?;
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
                return Err(Error::HeaderNotFound(hash.as_string()))
            }
            ret.push(None);
        }

        Ok(ret)
    }
}
