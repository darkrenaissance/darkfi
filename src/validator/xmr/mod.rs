/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
 * Copyright (C) 2021      The Tari Project (BSD-3)
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

use monero::{
    blockdata::transaction::RawExtraField, consensus::Encodable, cryptonote::hash::Hashable,
    Block as MoneroBlock, BlockHeader as MoneroBlockHeader,
};
use tiny_keccak::{Hasher, Keccak};

use crate::{error::MergeMineError, Result};

mod merkle_tree;
use merkle_tree::MerkleProof as MoneroMerkleProof;

#[cfg(test)]
mod tests;

pub struct MoneroPowData {
    /// Monero header fields
    header: MoneroBlockHeader,
    /// RandomX VM key
    randomx_key: [u8; 64],
    /// Transaction count
    transaction_count: u16,
    /// Transaction root
    merkle_root: monero::Hash,
    /// Coinbase Merkle proof hashes
    coinbase_merkle_proof: MoneroMerkleProof,
    /// Incomplete hashed state of the coinbase transaction
    coinbase_tx_hasher: Keccak,
    /// Extra fields of the coinbase
    coinbase_tx_extra: RawExtraField,
    /// Aux chain Merkle proof hashes
    aux_chain_merkle_proof: MoneroMerkleProof,
}

impl MoneroPowData {
    /// Constructs the Monero PoW data from the given block and seed.
    /// The data comes from `merge_mining_submit_solution` RPC.
    pub fn new(
        block: &MoneroBlock,
        seed: [u8; 64],
        ordered_aux_chain_hashes: Vec<monero::Hash>,
        darkfi_hash: [u8; 64],
    ) -> Result<Self> {
        let mut hashes = Vec::with_capacity(1 + block.tx_hashes.len());
        hashes.push(block.miner_tx.hash());
        hashes.copy_from_slice(&block.tx_hashes);

        let root = merkle_tree::tree_hash(&hashes)?;

        let coinbase_merkle_proof = merkle_tree::create_merkle_proof(&hashes, &hashes[0])
            .ok_or_else(|| {
                MergeMineError::ValidationError(
                    "create_merkle_proof returned None because the block had no coinbase \
                     (which is impossible because the Block type does not allow that)"
                        .to_string(),
                )
            })?;

        let coinbase = &block.miner_tx;
        let mut keccak = Keccak::v256();
        let mut encoder_prefix = vec![];

        coinbase.prefix.version.consensus_encode(&mut encoder_prefix)?;
        coinbase.prefix.unlock_time.consensus_encode(&mut encoder_prefix)?;
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix)?;
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix)?;
        keccak.update(&encoder_prefix);

        let t_hash = monero::Hash::from_slice(&darkfi_hash);

        let aux_chain_merkle_proof =
            merkle_tree::create_merkle_proof(&ordered_aux_chain_hashes, &t_hash).ok_or_else(
                || {
                    MergeMineError::ValidationError(
                        "create_merkle_proof returned None, could not find DarkFi hash in \
                         ordered aux chain hashes"
                            .to_string(),
                    )
                },
            )?;

        #[allow(clippy::cast_possible_truncation)]
        Ok(Self {
            header: block.header.clone(),
            randomx_key: seed,
            transaction_count: hashes.len() as u16,
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_tx_extra: block.miner_tx.prefix.extra.clone(),
            coinbase_tx_hasher: keccak,
            aux_chain_merkle_proof,
        })
    }

    /// Returns the `blockhashing_blob` for the Monero block
    pub fn to_blockhashing_blob(&self) -> Vec<u8> {
        let mut blockhashing_blob = monero::consensus::serialize(&self.header);
        blockhashing_blob.extend_from_slice(self.merkle_root.as_bytes());
        let mut count =
            monero::consensus::serialize(&monero::VarInt(self.transaction_count as u64));
        blockhashing_blob.append(&mut count);
        blockhashing_blob
    }
}
