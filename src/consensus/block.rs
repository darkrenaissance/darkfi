/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use darkfi_sdk::crypto::{constants::MERKLE_DEPTH, MerkleNode};
use darkfi_serial::{serialize, SerialDecodable, SerialEncodable};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use pasta_curves::pallas;

use super::{
    constants::{BLOCK_MAGIC_BYTES, BLOCK_VERSION},
    LeadInfo,
};
use crate::{net, tx::Transaction, util::time::Timestamp};

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

    /// Generate the genesis block.
    pub fn genesis_header(genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Self {
        let tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100);
        let root = tree.root(0).unwrap();

        Self::new(genesis_data, 0, 0, genesis_ts, root)
    }

    /// Calculate the header hash
    pub fn headerhash(&self) -> blake3::Hash {
        blake3::hash(&serialize(self))
    }
}

impl Default for Header {
    fn default() -> Self {
        Header::new(
            blake3::hash(b""),
            0,
            0,
            Timestamp::current_time(),
            MerkleNode::from(pallas::Base::zero()),
        )
    }
}

/// This struct represents a tuple of the form (`magic`, `header`, `counter`, `txs`, `lead_info`).
/// The header and transactions are stored as hashes, serving as pointers to
/// the actual data in the sled database.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Block {
    /// Block magic bytes
    pub magic: [u8; 4],
    /// Block header
    pub header: blake3::Hash,
    /// Trasaction hashes
    pub txs: Vec<blake3::Hash>,
    /// Lead Info
    pub lead_info: LeadInfo,
}

impl net::Message for Block {
    fn name() -> &'static str {
        "block"
    }
}

impl Block {
    pub fn new(
        previous: blake3::Hash,
        epoch: u64,
        slot: u64,
        txs: Vec<blake3::Hash>,
        root: MerkleNode,
        lead_info: LeadInfo,
    ) -> Self {
        let magic = BLOCK_MAGIC_BYTES;
        let timestamp = Timestamp::current_time();
        let header = Header::new(previous, epoch, slot, timestamp, root);
        let header = header.headerhash();
        Self { magic, header, txs, lead_info }
    }

    /// Generate the genesis block.
    pub fn genesis_block(genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Self {
        let magic = BLOCK_MAGIC_BYTES;
        let header = Header::genesis_header(genesis_ts, genesis_data);
        let header = header.headerhash();
        let lead_info = LeadInfo::default();
        Self { magic, header, txs: vec![], lead_info }
    }

    /// Calculate the block hash
    pub fn blockhash(&self) -> blake3::Hash {
        blake3::hash(&serialize(self))
    }
}

/// Auxiliary structure used for blockchain syncing.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct BlockOrder {
    /// Slot UID
    pub slot: u64,
    /// Block headerhash of that slot
    pub block: blake3::Hash,
}

impl net::Message for BlockOrder {
    fn name() -> &'static str {
        "blockorder"
    }
}

/// Structure representing full block data.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BlockInfo {
    /// BlockInfo magic bytes
    pub magic: [u8; 4],
    /// Block header data
    pub header: Header,
    /// Transactions payload
    pub txs: Vec<Transaction>,
    /// Lead Info,
    pub lead_info: LeadInfo,
}

impl Default for BlockInfo {
    fn default() -> Self {
        let magic = BLOCK_MAGIC_BYTES;
        Self { magic, header: Header::default(), txs: vec![], lead_info: LeadInfo::default() }
    }
}

impl net::Message for BlockInfo {
    fn name() -> &'static str {
        "blockinfo"
    }
}

impl BlockInfo {
    pub fn new(header: Header, txs: Vec<Transaction>, lead_info: LeadInfo) -> Self {
        let magic = BLOCK_MAGIC_BYTES;
        Self { magic, header, txs, lead_info }
    }

    /// Calculate the block hash
    pub fn blockhash(&self) -> blake3::Hash {
        let block: Block = self.clone().into();
        block.blockhash()
    }
}

impl From<BlockInfo> for Block {
    fn from(block_info: BlockInfo) -> Self {
        let txs = block_info.txs.iter().map(|x| blake3::hash(&serialize(x))).collect();
        Self {
            magic: block_info.magic,
            header: block_info.header.headerhash(),
            txs,
            lead_info: block_info.lead_info,
        }
    }
}

/// Auxiliary structure used for blockchain syncing
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BlockResponse {
    /// Response blocks.
    pub blocks: Vec<BlockInfo>,
}

impl net::Message for BlockResponse {
    fn name() -> &'static str {
        "blockresponse"
    }
}

/// This struct represents a block proposal, used for consensus.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BlockProposal {
    /// Block hash
    pub hash: blake3::Hash,
    /// Block header hash
    pub header: blake3::Hash,
    /// Block data
    pub block: BlockInfo,
}

impl BlockProposal {
    #[allow(clippy::too_many_arguments)]
    pub fn new(header: Header, txs: Vec<Transaction>, lead_info: LeadInfo) -> Self {
        let block = BlockInfo::new(header, txs, lead_info);
        let hash = block.blockhash();
        let header = block.header.headerhash();
        Self { hash, header, block }
    }
}

impl PartialEq for BlockProposal {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash &&
            self.header == other.header &&
            self.block.header == other.block.header &&
            self.block.txs == other.block.txs
    }
}

impl fmt::Display for BlockProposal {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_fmt(format_args!(
            "BlockProposal {{ leader public key: {}, hash: {}, header: {}, epoch: {}, slot: {}, txs: {} }}",
            self.block.lead_info.public_key,
            self.hash,
            self.header,
            self.block.header.epoch,
            self.block.header.slot,
            self.block.txs.len()
        ))
    }
}

impl net::Message for BlockProposal {
    fn name() -> &'static str {
        "proposal"
    }
}

impl From<BlockProposal> for BlockInfo {
    fn from(block: BlockProposal) -> BlockInfo {
        block.block
    }
}
