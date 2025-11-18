/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use std::{
    fmt,
    io::{self, Cursor, Error, Read, Write},
    iter,
};

use darkfi_sdk::{hex::decode_hex, AsHex};
#[cfg(feature = "async-serial")]
use darkfi_serial::{async_trait, AsyncDecodable, AsyncEncodable, AsyncRead, AsyncWrite};
use darkfi_serial::{Decodable, Encodable};
use monero::{
    blockdata::transaction::{ExtraField, RawExtraField, SubField},
    consensus::{Decodable as XmrDecodable, Encodable as XmrEncodable},
    cryptonote::hash::Hashable,
    util::ringct::{RctSigBase, RctType},
    BlockHeader,
};
use tiny_keccak::{Hasher, Keccak};
use tracing::warn;

use crate::{Error::MoneroMergeMineError, Result};

pub mod fixed_array;
use fixed_array::{FixedByteArray, MaxSizeVec};

pub mod merkle_proof;
use merkle_proof::MerkleProof;

pub mod keccak;
use keccak::{keccak_from_bytes, keccak_to_bytes};

pub mod utils;
use utils::{create_blockhashing_blob, create_merkle_proof, tree_hash};

pub mod merkle_tree_parameters;
pub use merkle_tree_parameters::MerkleTreeParameters;

pub type AuxChainHashes = MaxSizeVec<monero::Hash, 128>;

/// This struct represents all the Proof of Work information required
/// for merge mining.
#[derive(Clone)]
pub struct MoneroPowData {
    /// Monero Header fields
    pub header: BlockHeader,
    /// RandomX VM key - length varies to a max len of 60.
    pub randomx_key: FixedByteArray,
    /// The number of transactions included in this Monero block.
    /// This is used to produce the blockhashing_blob.
    pub transaction_count: u16,
    /// Transaction root
    pub merkle_root: monero::Hash,
    /// Coinbase Merkle proof hashes
    pub coinbase_merkle_proof: MerkleProof,
    /// Incomplete hashed state of the coinbase transaction
    pub coinbase_tx_hasher: Keccak,
    /// Extra field of the coinbase
    pub coinbase_tx_extra: RawExtraField,
    /// Aux chain Merkle proof hashes
    pub aux_chain_merkle_proof: MerkleProof,
}

impl MoneroPowData {
    /// Constructs the Monero PoW data from the given block and seed
    pub fn new(
        block: monero::Block,
        seed: FixedByteArray,
        aux_chain_merkle_proof: MerkleProof,
    ) -> Result<Self> {
        let hashes = create_ordered_tx_hashes_from_block(&block);
        let root = tree_hash(&hashes)?;
        let hash =
            hashes.first().ok_or(MoneroMergeMineError("No hashes for Merkle proof".to_string()))?;

        let coinbase_merkle_proof = create_merkle_proof(&hashes, hash).ok_or_else(|| {
            MoneroMergeMineError(
                "create_merkle_proof returned None because the block has no coinbase".to_string(),
            )
        })?;

        let coinbase = block.miner_tx.clone();

        let mut keccak = Keccak::v256();
        let mut encoder_prefix = vec![];
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix)?;
        coinbase.prefix.unlock_time.consensus_encode(&mut encoder_prefix)?;
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix)?;
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix)?;
        keccak.update(&encoder_prefix);

        Ok(Self {
            header: block.header,
            randomx_key: seed,
            transaction_count: hashes.len() as u16,
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_tx_extra: block.miner_tx.prefix.extra,
            coinbase_tx_hasher: keccak,
            aux_chain_merkle_proof,
        })
    }

    /// Returns `true` if the coinbase Merkle proof produces the `merkle_root`
    /// hash, otherwise `false`.
    pub fn is_coinbase_valid_merkle_root(&self) -> bool {
        let mut finalised_prefix_keccak = self.coinbase_tx_hasher.clone();
        let mut encoder_extra_field = vec![];

        self.coinbase_tx_extra.consensus_encode(&mut encoder_extra_field).unwrap();
        finalised_prefix_keccak.update(&encoder_extra_field);
        let mut prefix_hash: [u8; 32] = [0u8; 32];
        finalised_prefix_keccak.finalize(&mut prefix_hash);

        let final_prefix_hash = monero::Hash::from_slice(&prefix_hash);

        // let mut finalised_keccak = Keccak::v256();
        let rct_sig_base = RctSigBase {
            rct_type: RctType::Null,
            txn_fee: Default::default(),
            pseudo_outs: vec![],
            ecdh_info: vec![],
            out_pk: vec![],
        };

        let hashes = vec![final_prefix_hash, rct_sig_base.hash(), monero::Hash::null()];

        let encoder_final: Vec<u8> =
            hashes.into_iter().flat_map(|h| Vec::from(&h.to_bytes()[..])).collect();

        let coinbase_hash = monero::Hash::new(encoder_final);

        let merkle_root = self.coinbase_merkle_proof.calculate_root(&coinbase_hash);
        (self.merkle_root == merkle_root) && self.coinbase_merkle_proof.check_coinbase_path()
    }

    /// Returns the blockhashing_blob for the Monero block
    pub fn to_blockhashing_blob(&self) -> Vec<u8> {
        create_blockhashing_blob(&self.header, &self.merkle_root, u64::from(self.transaction_count))
    }

    /// Returns the RandomX VM key
    pub fn randomx_key(&self) -> &[u8] {
        self.randomx_key.as_slice()
    }
}

impl fmt::Debug for MoneroPowData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut digest = [0u8; 32];
        self.coinbase_tx_hasher.clone().finalize(&mut digest);
        f.debug_struct("MoneroPowData")
            .field("header", &self.header)
            .field("randomx_key", &self.randomx_key)
            .field("transaction_count", &self.transaction_count)
            .field("merkle_root", &self.merkle_root)
            .field("coinbase_merkle_proof", &self.coinbase_merkle_proof)
            .field("coinbase_tx_extra", &self.coinbase_tx_extra)
            .field("aux_chain_merkle_proof", &self.aux_chain_merkle_proof)
            .finish()
    }
}

impl Encodable for MoneroPowData {
    fn encode<S: Write>(&self, s: &mut S) -> io::Result<usize> {
        let mut n = 0;

        n += self.header.consensus_encode(s)?;
        n += self.randomx_key.encode(s)?;
        n += self.transaction_count.encode(s)?;
        n += self.merkle_root.consensus_encode(s)?;
        n += self.coinbase_merkle_proof.encode(s)?;

        // This is an incomplete hasher. Dump it from memory
        // and write it down. We can restore it the same way.
        let buf = keccak_to_bytes(&self.coinbase_tx_hasher);
        n += buf.encode(s)?;

        n += self.coinbase_tx_extra.0.encode(s)?;
        n += self.aux_chain_merkle_proof.encode(s)?;

        Ok(n)
    }
}

#[cfg(feature = "async-serial")]
#[async_trait]
impl AsyncEncodable for MoneroPowData {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> io::Result<usize> {
        let mut n = 0;

        let mut buf = vec![];
        self.header.consensus_encode(&mut buf)?;
        n += buf.encode_async(s).await?;

        n += self.randomx_key.encode_async(s).await?;
        n += self.transaction_count.encode_async(s).await?;

        let mut buf = vec![];
        self.merkle_root.consensus_encode(&mut buf)?;
        n += buf.encode_async(s).await?;

        n += self.coinbase_merkle_proof.encode_async(s).await?;

        // This is an incomplete hasher. Dump it from memory
        // and write it down. We can restore it the same way.
        let buf = keccak_to_bytes(&self.coinbase_tx_hasher);
        n += buf.encode_async(s).await?;

        n += self.coinbase_tx_extra.0.encode_async(s).await?;
        n += self.aux_chain_merkle_proof.encode_async(s).await?;

        Ok(n)
    }
}

impl Decodable for MoneroPowData {
    fn decode<D: Read>(d: &mut D) -> io::Result<Self> {
        let header =
            BlockHeader::consensus_decode(d).map_err(|_| Error::other("Invalid XMR header"))?;

        let randomx_key: FixedByteArray = Decodable::decode(d)?;
        let transaction_count: u16 = Decodable::decode(d)?;

        let merkle_root =
            monero::Hash::consensus_decode(d).map_err(|_| Error::other("Invalid XMR hash"))?;

        let coinbase_merkle_proof: MerkleProof = Decodable::decode(d)?;

        let buf: Vec<u8> = Decodable::decode(d)?;
        let coinbase_tx_hasher = keccak_from_bytes(&buf);

        let coinbase_tx_extra: Vec<u8> = Decodable::decode(d)?;
        let coinbase_tx_extra = RawExtraField(coinbase_tx_extra);
        let aux_chain_merkle_proof: MerkleProof = Decodable::decode(d)?;

        Ok(Self {
            header,
            randomx_key,
            transaction_count,
            merkle_root,
            coinbase_merkle_proof,
            coinbase_tx_hasher,
            coinbase_tx_extra,
            aux_chain_merkle_proof,
        })
    }
}

#[cfg(feature = "async-serial")]
#[async_trait]
impl AsyncDecodable for MoneroPowData {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> io::Result<Self> {
        let buf: Vec<u8> = AsyncDecodable::decode_async(d).await?;
        let mut buf = Cursor::new(buf);
        let header = BlockHeader::consensus_decode(&mut buf)
            .map_err(|_| Error::other("Invalid XMR header"))?;

        let randomx_key: FixedByteArray = AsyncDecodable::decode_async(d).await?;
        let transaction_count: u16 = AsyncDecodable::decode_async(d).await?;

        let buf: Vec<u8> = AsyncDecodable::decode_async(d).await?;
        let mut buf = Cursor::new(buf);
        let merkle_root = monero::Hash::consensus_decode(&mut buf)
            .map_err(|_| Error::other("Invalid XMR hash"))?;

        let coinbase_merkle_proof: MerkleProof = AsyncDecodable::decode_async(d).await?;

        let buf: Vec<u8> = AsyncDecodable::decode_async(d).await?;
        let coinbase_tx_hasher = keccak_from_bytes(&buf);

        let coinbase_tx_extra: Vec<u8> = AsyncDecodable::decode_async(d).await?;
        let coinbase_tx_extra = RawExtraField(coinbase_tx_extra);
        let aux_chain_merkle_proof: MerkleProof = AsyncDecodable::decode_async(d).await?;

        Ok(Self {
            header,
            randomx_key,
            transaction_count,
            merkle_root,
            coinbase_merkle_proof,
            coinbase_tx_hasher,
            coinbase_tx_extra,
            aux_chain_merkle_proof,
        })
    }
}

/// Create a set of ordered transaction hashes from a Monero block
pub fn create_ordered_tx_hashes_from_block(block: &monero::Block) -> Vec<monero::Hash> {
    iter::once(block.miner_tx.hash()).chain(block.tx_hashes.clone()).collect()
}

/// Inserts aux chain merkle root and info into a Monero block
pub fn insert_aux_chain_mr_and_info_into_block<T: AsRef<[u8]>>(
    block: &mut monero::Block,
    aux_chain_mr: T,
    aux_chain_count: u8,
    aux_nonce: u32,
) -> Result<()> {
    if aux_chain_count == 0 {
        return Err(MoneroMergeMineError("Zero aux chains".to_string()))
    }

    if aux_chain_mr.as_ref().len() != monero::Hash::len_bytes() {
        return Err(MoneroMergeMineError("Aux chain root invalid length".to_string()))
    }

    // When we insert the Merge Mining tag, we need to make sure
    // that the extra field is valid.
    let mut extra_field = match ExtraField::try_parse(&block.miner_tx.prefix.extra) {
        Ok(v) => v,
        Err(e) => return Err(MoneroMergeMineError(e.to_string())),
    };

    // Adding more than one Merge Mining tag is not allowed
    for item in &extra_field.0 {
        if let SubField::MergeMining(_, _) = item {
            return Err(MoneroMergeMineError("More than one mm tag in coinbase".to_string()))
        }
    }

    // If `SubField::Padding(n)` with `n < 255` is the last subfield in the
    // extra field, then appending a new field will always fail to deserialize
    // (`ExtraField::try_parse`) - the new field cannot be parsed in that
    // sequence.
    // To circumvent this, we create a new extra field by appending the
    // original extra field to the merge mining field instead.
    let hash = monero::Hash::from_slice(aux_chain_mr.as_ref());
    let encoded = if aux_chain_count == 1 {
        monero::VarInt(0)
    } else {
        let mt_params = MerkleTreeParameters::new(aux_chain_count, aux_nonce)?;
        mt_params.to_varint()
    };
    extra_field.0.insert(0, SubField::MergeMining(encoded, hash));

    block.miner_tx.prefix.extra = extra_field.into();

    // Let's test the block to ensure it serializes correctly.
    let blocktemplate_ser = monero::consensus::serialize(block);
    let blocktemplate_hex = blocktemplate_ser.hex();
    let blocktemplate_bytes = decode_hex(&blocktemplate_hex)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let de_block: monero::Block = match monero::consensus::deserialize(&blocktemplate_bytes) {
        Ok(v) => v,
        Err(e) => return Err(io::Error::new(io::ErrorKind::InvalidData, e).into()),
    };

    if block != &de_block {
        return Err(MoneroMergeMineError("Blocks don't match after serialization".to_string()))
    }

    Ok(())
}

/*
/// Creates a hex-encoded Monero `blockhashing_blob`
fn create_block_hashing_blob(
    header: &monero::BlockHeader,
    merkle_root: &monero::Hash,
    transaction_count: u64,
) -> Vec<u8> {
    let mut blockhashing_blob = monero::consensus::serialize(header);
    blockhashing_blob.extend_from_slice(merkle_root.as_bytes());
    let mut count = monero::consensus::serialize(&monero::VarInt(transaction_count));
    blockhashing_blob.append(&mut count);
    blockhashing_blob
}

/// Creates a hex-encoded Monero `blockhashing_blob` that's used by the PoW hash
fn create_blockhashing_blob_from_blob(block: &monero::Block) -> Result<String> {
    let tx_hashes = create_ordered_tx_hashes_from_block(block);
    let root = tree_hash(&tx_hashes)?;
    let blob = create_block_hashing_blob(&block.header, &root, tx_hashes.len() as u64);
    Ok(blob.hex())
}
*/

/// Try to decode a `monero::Block` given a hex blob
pub fn monero_block_deserialize(blob: &str) -> Result<monero::Block> {
    let bytes: Vec<u8> = decode_hex(blob)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let mut reader = Cursor::new(bytes);

    match monero::Block::consensus_decode(&mut reader) {
        Ok(v) => Ok(v),
        Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e).into()),
    }
}

/// Parsing an extra field from bytes will always return an extra field with
/// subfields that could be read even if it does not represent the original
// extra field.
/// As per Monero consensus rules, an error here will not represent failure
/// to deserialize a block, so no need to error here.
fn parse_extra_field_truncate_on_error(raw_extra_field: &RawExtraField) -> ExtraField {
    match ExtraField::try_parse(raw_extra_field) {
        Ok(v) => v,
        Err(v) => {
            warn!(
                target: "blockchain::monero::parse_extra_field_truncate_on_error",
                "[BLOCKCHAIN] Some Monero tx_extra subfields could not be parsed",
            );
            v
        }
    }
}

/// Extract the Monero block hash from the coinbase transaction's extra field
pub fn extract_aux_merkle_root_from_block(monero: &monero::Block) -> Result<Option<monero::Hash>> {
    extract_aux_merkle_root(&monero.miner_tx.prefix.extra)
}

/// Extract the Monero block hash from the coinbase transaction's extra field
pub fn extract_aux_merkle_root(extra_field: &RawExtraField) -> Result<Option<monero::Hash>> {
    let extra_field = parse_extra_field_truncate_on_error(extra_field);
    // Only one merge mining tag is allowed
    let merge_mining_hashes: Vec<monero::Hash> = extra_field
        .0
        .iter()
        .filter_map(|item| {
            if let SubField::MergeMining(_depth, merge_mining_hash) = item {
                Some(*merge_mining_hash)
            } else {
                None
            }
        })
        .collect();

    if merge_mining_hashes.len() > 1 {
        return Err(MoneroMergeMineError("More than one MM tag found in coinbase".to_string()))
    }

    if let Some(merge_mining_hash) = merge_mining_hashes.into_iter().next() {
        Ok(Some(merge_mining_hash))
    } else {
        Ok(None)
    }
}
