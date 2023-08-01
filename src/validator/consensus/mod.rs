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
use log::{error, warn};

use crate::{
    blockchain::{BlockInfo, Blockchain, BlockchainOverlay, BlockchainOverlayPtr},
    util::time::TimeKeeper,
    validator::verify_block,
    Error, Result,
};

/// DarkFi consensus PID controller
pub mod pid;

/// Base 10 big float implementation for high precision arithmetics
pub mod float_10;

/// Consensus configuration
const TXS_CAP: usize = 50;

/// This struct represents the information required by the consensus algorithm
pub struct Consensus {
    /// Canonical (finalized) blockchain
    pub blockchain: Blockchain,
    /// Helper structure to calculate time related operations
    pub time_keeper: TimeKeeper,
    /// Node is participating to consensus
    pub participating: bool,
    /// Last slot node check for finalization
    pub checked_finalization: u64,
    /// Fork chains containing block proposals
    pub forks: Vec<Fork>,
    /// Flag to enable testing mode
    pub testing_mode: bool,
}

impl Consensus {
    /// Generate a new Consensus state.
    pub fn new(blockchain: Blockchain, time_keeper: TimeKeeper, testing_mode: bool) -> Self {
        Self {
            blockchain,
            time_keeper,
            participating: false,
            checked_finalization: 0,
            forks: vec![],
            testing_mode,
        }
    }

    /// Given a proposal, the node verifys it and finds which fork it extends.
    /// If the proposal extends the canonical blockchain, a new fork chain is created.
    /// A proposal is considered valid when the following rules apply:
    ///     1. Node has not started current slot finalization
    ///     2. Proposal refers to current slot
    ///     3. Proposal hash matches the actual block one
    ///     4. Header hash matches the actual one
    ///     5. Block transactions don't exceed set limit
    ///     6. Block is valid
    /// Additional validity rules can be applied.
    pub async fn append_proposal(&mut self, proposal: &Proposal) -> Result<()> {
        // Generate a time keeper for current slot
        let time_keeper = self.time_keeper.current();

        // Node have already checked for finalization in this slot
        if time_keeper.verifying_slot <= self.checked_finalization {
            warn!(target: "validator::consensus::append_proposal", "Proposal received after finalization sync period.");
            return Err(Error::ProposalAfterFinalizationError)
        }

        // Proposal validations
        let hdr = &proposal.block.header;

        // Ignore proposal if not for current slot
        if hdr.slot != time_keeper.verifying_slot {
            return Err(Error::ProposalNotForCurrentSlotError)
        }

        // Check if proposal hash matches actual one
        let proposal_hash = proposal.block.blockhash();
        if proposal.hash != proposal_hash {
            warn!(
                target: "validator::consensus::append_proposal", "Received proposal contains mismatched hashes: {} - {}",
                proposal.hash, proposal_hash
            );
            return Err(Error::ProposalHashesMissmatchError)
        }

        // Check if proposal header matches actual one
        let proposal_header = hdr.headerhash();
        if proposal.header != proposal_header {
            warn!(
                target: "validator::consensus::append_proposal", "Received proposal contains mismatched headers: {} - {}",
                proposal.header, proposal_header
            );
            return Err(Error::ProposalHeadersMissmatchError)
        }

        // TODO: verify if this should happen here or not.
        // Check that proposal transactions don't exceed limit
        if proposal.block.txs.len() > TXS_CAP {
            warn!(
                target: "validator::consensus::append_proposal", "Received proposal transactions exceed configured cap: {} - {}",
                proposal.block.txs.len(),
                TXS_CAP
            );
            return Err(Error::ProposalTxsExceedCapError)
        }

        // Check if proposal extends any existing forks
        let (mut fork, index) = self.find_extended_fork_overlay(&proposal).await?;

        // Grab overlay last block
        let previous = fork.overlay.lock().unwrap().last_block()?;

        // Retrieve expected reward
        let expected_reward = next_block_reward();

        // Verify proposal block
        if verify_block(
            &fork.overlay,
            &time_keeper,
            &proposal.block,
            &previous,
            expected_reward,
            self.testing_mode,
        )
        .await
        .is_err()
        {
            error!(target: "validator::consensus::append_proposal", "Erroneous proposal block found");
            fork.overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
            return Err(Error::BlockIsInvalid(proposal.hash.to_string()))
        };

        // If a fork index was found, replace forks with the mutated one,
        // otherwise push the new fork.
        fork.proposals.push(proposal.hash);
        match index {
            Some(i) => {
                self.forks[i] = fork;
            }
            None => {
                self.forks.push(fork);
            }
        }

        Ok(())
    }

    /// Given a proposal, find the index of the fork chain it extends, along with the specific
    /// extended proposal index.
    fn find_extended_fork(&self, proposal: &Proposal) -> Result<(usize, usize)> {
        for (f_index, fork) in self.forks.iter().enumerate() {
            // Traverse fork proposals sequence in reverse
            for (p_index, p_hash) in fork.proposals.iter().enumerate().rev() {
                if &proposal.block.header.previous == p_hash {
                    return Ok((f_index, p_index))
                }
            }
        }

        Err(Error::ExtendedChainIndexNotFound)
    }

    /// Given a proposal, find the fork chain it extends, and return its full clone.
    /// If the proposal extends the fork not on its tail, a new fork is created and
    /// we re-apply the proposals up to the extending one. If proposal extends canonical,
    /// a new fork is created. Additionally, we return the fork index if a new fork
    /// was not created, so caller can replace the fork.
    async fn find_extended_fork_overlay(
        &self,
        proposal: &Proposal,
    ) -> Result<(Fork, Option<usize>)> {
        // Check if proposal extends any fork
        let found = self.find_extended_fork(proposal);
        if found.is_err() {
            // Check if we extend canonical
            let (last_slot, last_block) = self.blockchain.last()?;
            if proposal.block.header.previous != last_block ||
                proposal.block.header.slot <= last_slot
            {
                return Err(Error::ExtendedChainIndexNotFound)
            }

            return Ok((Fork::new(&self.blockchain)?, None))
        }

        let (f_index, p_index) = found.unwrap();
        let original_fork = &self.forks[f_index];
        // Check if proposal extends fork at last proposal
        if p_index == (original_fork.proposals.len() - 1) {
            return Ok((original_fork.full_clone()?, Some(f_index)))
        }

        // Rebuild fork
        let mut fork = Fork::new(&self.blockchain)?;
        fork.proposals = original_fork.proposals[..p_index + 1].to_vec();

        // Retrieve proposals blocks from original fork
        let blocks = &original_fork.overlay.lock().unwrap().get_blocks_by_hash(&fork.proposals)?;

        // Retrieve last block
        let mut previous = &fork.overlay.lock().unwrap().last_block()?;

        // Create a time keeper to validate each proposal block
        let mut time_keeper = self.time_keeper.clone();

        // Validate and insert each block
        for block in blocks {
            // Use block slot in time keeper
            time_keeper.verifying_slot = block.header.slot;

            // Retrieve expected reward
            let expected_reward = next_block_reward();

            // Verify block
            if verify_block(
                &fork.overlay,
                &time_keeper,
                block,
                previous,
                expected_reward,
                self.testing_mode,
            )
            .await
            .is_err()
            {
                error!(target: "validator::consensus::find_extended_fork_overlay", "Erroneous block found in set");
                fork.overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
                return Err(Error::BlockIsInvalid(block.blockhash().to_string()))
            };

            // Use last inserted block as next iteration previous
            previous = block;
        }

        Ok((fork, None))
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

/// This struct represents a forked blockchain state, using an overlay over original
/// blockchain, containing all pending to-write records. Additionally, each fork
/// keeps a vector of valid pending transactions hashes, in order of receival, and
/// the proposals hashes sequence, for validations.
#[derive(Clone)]
pub struct Fork {
    /// Overlay cache over canonical Blockchain
    pub overlay: BlockchainOverlayPtr,
    /// Fork proposal hashes sequence
    pub proposals: Vec<blake3::Hash>,
    /// Valid pending transaction hashes
    pub mempool: Vec<blake3::Hash>,
}

impl Fork {
    pub fn new(blockchain: &Blockchain) -> Result<Self> {
        let overlay = BlockchainOverlay::new(blockchain)?;
        Ok(Self { overlay, proposals: vec![], mempool: vec![] })
    }

    /// Auxiliary function to create a full clone using BlockchainOverlay::full_clone.
    /// Changes to this copy don't affect original fork overlay records, since underlying
    /// overlay pointer have been updated to the cloned one.
    pub fn full_clone(&self) -> Result<Self> {
        let overlay = self.overlay.lock().unwrap().full_clone()?;
        let proposals = self.proposals.clone();
        let mempool = self.mempool.clone();

        Ok(Self { overlay, proposals, mempool })
    }
}

/// Block producer reward.
/// TODO (res) implement reward mechanism with accord to DRK, DARK token-economics.
pub fn next_block_reward() -> u64 {
    // Configured block reward (1 DRK == 1 * 10^8)
    let reward: u64 = 100_000_000;
    reward
}
