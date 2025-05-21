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

use monero::{consensus::Encodable as XmrEncodable, cryptonote::hash::Hashable, VarInt};
use tiny_keccak::{Hasher, Keccak};

use crate::{
    blockchain::{
        header_store::HeaderHash,
        monero::{
            fixed_array::FixedByteArray,
            utils::{create_merkle_proof, tree_hash},
            MoneroPowData,
        },
    },
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
