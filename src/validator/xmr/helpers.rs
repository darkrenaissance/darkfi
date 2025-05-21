/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 * Copyright (C) 2021 The Tari Project (BSD-3)
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

use std::{io, iter};

use log::warn;
use monero::{
    blockdata::transaction::{ExtraField, RawExtraField, SubField},
    consensus::Encodable as XmrEncodable,
    cryptonote::hash::Hashable,
    VarInt,
};
use primitive_types::U256;
use sha2::{Digest, Sha256};
use tiny_keccak::{Hasher, Keccak};

use super::merkle_tree_parameters::MerkleTreeParameters;
use crate::{
    blockchain::{
        header_store::HeaderHash,
        monero::{
            fixed_array::FixedByteArray,
            utils::{create_merkle_proof, tree_hash},
            MoneroPowData,
        },
    },
    Error,
    Error::MoneroMergeMineError,
    Result,
};

/// Deserializes the given hex-encoded string into a Monero block
pub fn deserialize_monero_block_from_hex<T>(data: T) -> io::Result<monero::Block>
where
    T: AsRef<[u8]>,
{
    let bytes = hex::decode(data).map_err(|_| io::Error::other("Invalid hex data"))?;
    let obj = monero::consensus::deserialize::<monero::Block>(&bytes)
        .map_err(|_| io::Error::other("Invalid XMR block"))?;
    Ok(obj)
}

/// Serializes the given Monero block into a hex-encoded string
pub fn serialize_monero_block_to_hex(obj: &monero::Block) -> io::Result<String> {
    let data = monero::consensus::serialize::<monero::Block>(obj);
    let bytes = hex::encode(data);
    Ok(bytes)
}

/// Create a set of ordered tx hashes from a Monero block
pub fn create_ordered_tx_hashes_from_block(block: &monero::Block) -> Vec<monero::Hash> {
    iter::once(block.miner_tx.hash()).chain(block.tx_hashes.clone()).collect()
}

/// Creates a hex-encoded Monero blockhashing_blob
pub fn create_blockhashing_blob(
    header: &monero::BlockHeader,
    merkle_root: &monero::Hash,
    transaction_count: u64,
) -> Vec<u8> {
    let mut blockhashing_blob = monero::consensus::serialize(header);
    blockhashing_blob.extend_from_slice(merkle_root.as_bytes());
    let mut count = monero::consensus::serialize(&VarInt(transaction_count));
    blockhashing_blob.append(&mut count);
    blockhashing_blob
}

/// Constructs [`MoneroPowData`] from the given block and seed
pub fn construct_monero_data(
    block: monero::Block,
    seed: FixedByteArray,
    ordered_aux_chain_hashes: Vec<monero::Hash>,
    darkfi_hash: HeaderHash,
) -> Result<MoneroPowData> {
    let hashes = create_ordered_tx_hashes_from_block(&block);
    let root = tree_hash(&hashes)?;

    let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).ok_or_else(|| {
        MoneroMergeMineError(
            "create_merkle_proof returned None because the block had no coinbase".to_string(),
        )
    })?;

    let coinbase = block.miner_tx.clone();

    let mut keccak = Keccak::v256();
    let mut encoder_prefix = vec![];

    coinbase
        .prefix
        .version
        .consensus_encode(&mut encoder_prefix)
        .map_err(|e| MoneroMergeMineError(e.to_string()))?;

    coinbase
        .prefix
        .unlock_time
        .consensus_encode(&mut encoder_prefix)
        .map_err(|e| MoneroMergeMineError(e.to_string()))?;

    coinbase
        .prefix
        .inputs
        .consensus_encode(&mut encoder_prefix)
        .map_err(|e| MoneroMergeMineError(e.to_string()))?;

    coinbase
        .prefix
        .outputs
        .consensus_encode(&mut encoder_prefix)
        .map_err(|e| MoneroMergeMineError(e.to_string()))?;

    keccak.update(&encoder_prefix);

    let t_hash = monero::Hash::from_slice(darkfi_hash.as_slice());
    let aux_chain_merkle_proof = create_merkle_proof(&ordered_aux_chain_hashes, &t_hash).ok_or_else(|| {
        MoneroMergeMineError(
            "create_merkle_proof returned None, could not find darkfi hash in ordered aux chain hashes".to_string(),
        )
    })?;

    Ok(MoneroPowData {
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

fn check_aux_chains(
    monero_data: &MoneroPowData,
    merge_mining_params: VarInt,
    aux_chain_merkle_root: &monero::Hash,
    darkfi_hash: HeaderHash,
    darkfi_genesis_hash: HeaderHash,
) -> bool {
    let df_hash = monero::Hash::from_slice(darkfi_hash.as_slice());

    if merge_mining_params == VarInt(0) {
        // Interpret 0 as only 1 chain
        if df_hash == *aux_chain_merkle_root {
            return true
        }
    }

    let merkle_tree_params = MerkleTreeParameters::from_varint(merge_mining_params);
    if merkle_tree_params.number_of_chains() == 0 {
        return false
    }

    let hash_position = U256::from_little_endian(
        &Sha256::new()
            .chain_update(darkfi_genesis_hash.as_slice())
            .chain_update(merkle_tree_params.aux_nonce().to_le_bytes())
            .chain_update((109_u8).to_le_bytes())
            .finalize(),
    )
    .low_u32() %
        u32::from(merkle_tree_params.number_of_chains());

    let (merkle_root, pos) = monero_data
        .aux_chain_merkle_proof
        .calculate_root_with_pos(&df_hash, merkle_tree_params.number_of_chains());

    if hash_position != pos {
        return false
    }

    merkle_root == *aux_chain_merkle_root
}

// Parsing an extra field from bytes will always return an extra field with sub-fields
// that could be read, even if it does not represent the original extra field. As per
// Monero consensus rules, an error here will not represent a failure to deserialize a
// block, so no need to error here.
fn parse_extra_field_truncate_on_error(raw_extra_field: &RawExtraField) -> ExtraField {
    match ExtraField::try_parse(raw_extra_field) {
        Ok(val) => val,
        Err(val) => {
            warn!(
                target: "validator::xmr::helpers",
                "[MERGEMINING] Some sub-fields could not be parsed from the Monero coinbase",
            );
            val
        }
    }
}

/// Extracts the Monero block hash from the coinbase transaction's extra field
pub fn extract_aux_merkle_root_from_block(monero: &monero::Block) -> Result<Option<monero::Hash>> {
    // When we extract the merge mining hash, we do not care if
    // the extra field can be parsed without error.
    let extra_field = parse_extra_field_truncate_on_error(&monero.miner_tx.prefix.extra);

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
        return Err(Error::MoneroMergeMineError(
            "More than one merge mining tag found in coinbase".to_string(),
        ))
    }

    if let Some(merge_mining_hash) = merge_mining_hashes.into_iter().next() {
        Ok(Some(merge_mining_hash))
    } else {
        Ok(None)
    }
}
