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

use darkfi_serial::{SerialDecodable, SerialEncodable};

use crate::{
    blockchain::{BlockInfo, Blockchain, BlockchainOverlay, BlockchainOverlayPtr},
    util::time::TimeKeeper,
    Result,
};

/// DarkFi consensus PID controller
pub mod pid;

/// Base 10 big float implementation for high precision arithmetics
pub mod float_10;

/// This struct represents the information required by the consensus algorithm
pub struct Consensus {
    /// Canonical (finalized) blockchain
    pub blockchain: Blockchain,
    /// Helper structure to calculate time related operations
    pub time_keeper: TimeKeeper,
    /// Node is participating to consensus
    pub participating: bool,
    /// Fork chains containing block proposals
    pub forks: Vec<Fork>,
}

impl Consensus {
    /// Generate a new Consensus state.
    pub fn new(blockchain: Blockchain, time_keeper: TimeKeeper) -> Self {
        Self { blockchain, time_keeper, participating: false, forks: vec![] }
    }

    /// Given a proposal, the node verifys it and finds which fork it extends.
    /// If the proposal extends the canonical blockchain, a new fork chain is created.
    pub async fn append_proposal(&mut self, _proposal: &Proposal) -> Result<()> {
        // TODO

        Ok(())
    }
}

/// This struct represents a block proposal, used for consensus.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Proposal {
    /// Block hash
    pub hash: blake3::Hash,
    /// Block header hash
    pub header: blake3::Hash,
    /// Block data
    pub block: BlockInfo,
}

impl Proposal {
    pub fn new(block: BlockInfo) -> Self {
        let hash = block.blockhash();
        let header = block.header.headerhash();
        Self { hash, header, block }
    }
}

impl From<Proposal> for BlockInfo {
    fn from(proposal: Proposal) -> BlockInfo {
        proposal.block
    }
}

/// This struct represents a sequence of block proposals, along with a blockchain
/// overlay, containing all pending to-write records.
#[derive(Clone)]
pub struct Fork {
    pub overlay: BlockchainOverlayPtr,
    pub proposals: Vec<Proposal>,
}

impl Fork {
    pub fn new(blockchain: &Blockchain) -> Result<Self> {
        let overlay = BlockchainOverlay::new(blockchain)?;
        Ok(Self { overlay, proposals: vec![] })
    }
}

/// Block producer reward.
/// TODO (res) implement reward mechanism with accord to DRK, DARK token-economics.
pub fn next_block_reward() -> u64 {
    // Configured block reward (1 DRK == 1 * 10^8)
    let reward: u64 = 100_000_000;
    reward
}
