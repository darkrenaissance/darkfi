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

use std::collections::{BTreeSet, HashMap, HashSet};

use darkfi_sdk::{crypto::MerkleTree, tx::TransactionHash};
use darkfi_serial::{async_trait, deserialize, SerialDecodable, SerialEncodable};
use num_bigint::BigUint;
use sled_overlay::{database::SledDbOverlayStateDiff, sled::IVec};
use tracing::{debug, info, warn};

use crate::{
    blockchain::{
        block_store::{BlockDifficulty, BlockRanks},
        BlockInfo, Blockchain, BlockchainOverlay, BlockchainOverlayPtr, Header, HeaderHash,
    },
    runtime::vm_runtime::GAS_LIMIT,
    tx::{Transaction, MAX_TX_CALLS},
    validator::{
        pow::{PoWModule, RANDOMX_KEY_CHANGE_DELAY, RANDOMX_KEY_CHANGING_HEIGHT},
        utils::{best_fork_index, block_rank, find_extended_fork_index, worst_fork_index},
        verification::{verify_proposal, verify_transaction},
    },
    zk::VerifyingKey,
    Error, Result,
};

/// Gas limit for total block transactions(50 full transactions).
pub const BLOCK_GAS_LIMIT: u64 = GAS_LIMIT * MAX_TX_CALLS as u64 * 50;

/// This struct represents the information required by the consensus algorithm
pub struct Consensus {
    /// Canonical (confirmed) blockchain
    pub blockchain: Blockchain,
    /// Fork size(length) after which it can be confirmed
    pub confirmation_threshold: usize,
    /// Fork chains containing block proposals
    pub forks: Vec<Fork>,
    /// Max in-memory forks to maintain.
    max_forks: usize,
    /// Canonical blockchain PoW module state
    pub module: PoWModule,
}

impl Consensus {
    /// Generate a new Consensus state.
    pub fn new(
        blockchain: Blockchain,
        confirmation_threshold: usize,
        max_forks: usize,
        pow_target: u32,
        pow_fixed_difficulty: Option<BigUint>,
    ) -> Result<Self> {
        let max_forks = if max_forks == 0 { 1 } else { max_forks };
        let module = PoWModule::new(blockchain.clone(), pow_target, pow_fixed_difficulty, None)?;

        Ok(Self { blockchain, confirmation_threshold, forks: vec![], max_forks, module })
    }

    /// Try to generate a new empty fork. If the forks bound has been
    /// reached, try to replace the worst ranking one with the new
    /// empty fork.
    pub async fn generate_empty_fork(&mut self) -> Result<()> {
        debug!(target: "validator::consensus::generate_empty_fork", "Generating new empty fork...");
        // Check if we already have an empty fork
        for fork in &self.forks {
            if fork.proposals.is_empty() {
                debug!(target: "validator::consensus::generate_empty_fork", "An empty fork already exists.");
                return Ok(())
            }
        }
        let fork = Fork::new(self.blockchain.clone(), self.module.clone()).await?;
        self.push_fork(fork);
        debug!(target: "validator::consensus::generate_empty_fork", "Fork generated!");

        Ok(())
    }

    /// Auxiliary function to push a fork into the forks vector
    /// respecting the bounding confirguration. The fork will be
    /// inserted iff the bound has not be reached or it ranks higher
    /// than the lowest ranking existing fork.
    fn push_fork(&mut self, fork: Fork) {
        // Check if we have reached the bound
        if self.forks.len() < self.max_forks {
            self.forks.push(fork);
            return
        }

        // Grab worst fork. We don't care about competing forks since
        // any of them can be replaced. It's safe to unwrap here since
        // we already checked forks length. `best_fork_index` returns
        // an error iff we pass an empty forks vector.
        let index = worst_fork_index(&self.forks).unwrap();

        // Check if the provided one ranks lower
        if fork.targets_rank < self.forks[index].targets_rank {
            return
        }

        // Break tie using their hash distances rank
        if fork.targets_rank == self.forks[index].targets_rank &&
            fork.hashes_rank <= self.forks[index].hashes_rank
        {
            return
        }

        // Replace the current worst fork with the provided one
        self.forks[index] = fork;
    }

    /// Given a proposal, the node verifys it and finds which fork it
    /// extends. If the proposal extends the canonical blockchain, a
    /// new fork chain is created.
    pub async fn append_proposal(&mut self, proposal: &Proposal, verify_fees: bool) -> Result<()> {
        debug!(target: "validator::consensus::append_proposal", "Appending proposal {}", proposal.hash);

        // Check if proposal already exists
        for fork in &self.forks {
            for p in fork.proposals.iter().rev() {
                if p == &proposal.hash {
                    debug!(target: "validator::consensus::append_proposal", "Proposal {} already exists", proposal.hash);
                    return Err(Error::ProposalAlreadyExists)
                }
            }
        }
        // Check if proposal is canonical
        if let Ok(canonical_headers) =
            self.blockchain.blocks.get_order(&[proposal.block.header.height], true)
        {
            if canonical_headers[0].unwrap() == proposal.hash {
                debug!(target: "validator::consensus::append_proposal", "Proposal {} already exists", proposal.hash);
                return Err(Error::ProposalAlreadyExists)
            }
        }

        // Verify proposal and grab corresponding fork
        let (mut fork, index) = verify_proposal(self, proposal, verify_fees).await?;

        // Append proposal to the fork
        fork.append_proposal(proposal).await?;

        // If a fork index was found, replace fork with the mutated
        // one, otherwise try to push the new fork.
        match index {
            Some(i) => {
                if i < self.forks.len() &&
                    self.forks[i].proposals == fork.proposals[..fork.proposals.len() - 1]
                {
                    self.forks[i] = fork;
                } else {
                    self.push_fork(fork);
                }
            }
            None => {
                self.push_fork(fork);
            }
        }

        info!(target: "validator::consensus::append_proposal", "Appended proposal {} - {}", proposal.hash, proposal.block.header.height);

        Ok(())
    }

    /// Given a proposal, find the fork chain it extends, and return its full clone.
    /// If the proposal extends the fork not on its tail, a new fork is created and
    /// we re-apply the proposals up to the extending one. If proposal extends canonical,
    /// a new fork is created. Additionally, we return the fork index if a new fork
    /// was not created, so caller can replace the fork.
    pub async fn find_extended_fork(&self, proposal: &Proposal) -> Result<(Fork, Option<usize>)> {
        // Check if proposal extends any fork
        let found = find_extended_fork_index(&self.forks, proposal);
        if found.is_err() {
            if let Err(Error::ProposalAlreadyExists) = found {
                return Err(Error::ProposalAlreadyExists)
            }

            // Check if proposal extends canonical
            let (last_height, last_block) = self.blockchain.last()?;
            if proposal.block.header.previous != last_block ||
                proposal.block.header.height <= last_height
            {
                return Err(Error::ExtendedChainIndexNotFound)
            }

            // Check if we have an empty fork to use
            for (f_index, fork) in self.forks.iter().enumerate() {
                if fork.proposals.is_empty() {
                    return Ok((self.forks[f_index].full_clone()?, Some(f_index)))
                }
            }

            // Generate a new fork extending canonical
            let fork = Fork::new(self.blockchain.clone(), self.module.clone()).await?;
            return Ok((fork, None))
        }

        let (f_index, p_index) = found.unwrap();
        let original_fork = &self.forks[f_index];
        // Check if proposal extends fork at last proposal
        if p_index == (original_fork.proposals.len() - 1) {
            return Ok((original_fork.full_clone()?, Some(f_index)))
        }

        // Rebuild fork
        let mut fork = Fork::new(self.blockchain.clone(), self.module.clone()).await?;
        fork.proposals = original_fork.proposals[..p_index + 1].to_vec();
        fork.diffs = original_fork.diffs[..p_index + 1].to_vec();

        // Retrieve proposals blocks from original fork
        let blocks = &original_fork.overlay.lock().unwrap().get_blocks_by_hash(&fork.proposals)?;
        for (index, block) in blocks.iter().enumerate() {
            // Apply block diffs
            fork.overlay.lock().unwrap().overlay.lock().unwrap().add_diff(&fork.diffs[index])?;

            // Grab next mine target and difficulty
            let (next_target, next_difficulty) = fork.module.next_mine_target_and_difficulty()?;

            // Calculate block rank
            let (target_distance_sq, hash_distance_sq) = block_rank(block, &next_target)?;

            // Update PoW module
            fork.module.append(&block.header, &next_difficulty)?;

            // Update fork ranks
            fork.targets_rank += target_distance_sq;
            fork.hashes_rank += hash_distance_sq;
        }

        Ok((fork, None))
    }

    /// Check if best fork proposals can be confirmed.
    /// Consensus confirmation logic:
    /// - If the current best fork has reached greater length than the security threshold,
    ///   and no other fork exist with same rank, first proposal(s) in that fork can be
    ///   appended to canonical/confirmed blockchain.
    ///
    /// When best fork can be confirmed, first block(s) should be appended to canonical,
    /// and forks should be rebuilt.
    pub async fn confirmation(&self) -> Result<Option<usize>> {
        debug!(target: "validator::consensus::confirmation", "Started confirmation check");

        // Grab best fork index
        let index = best_fork_index(&self.forks)?;

        // Check its length
        if self.forks[index].proposals.len() < self.confirmation_threshold {
            debug!(target: "validator::consensus::confirmation", "Nothing to confirm yet, best fork size: {}", self.forks[index].proposals.len());
            return Ok(None)
        }

        // Ensure no other fork exists with same rank
        for (f_index, fork) in self.forks.iter().enumerate() {
            // Skip best fork
            if f_index == index {
                continue
            }

            // Skip lower ranking forks
            if fork.targets_rank != self.forks[index].targets_rank {
                continue
            }

            // Check hash distances rank
            if fork.hashes_rank == self.forks[index].hashes_rank {
                debug!(target: "validator::consensus::confirmation", "Competing best forks found");
                return Ok(None)
            }
        }

        Ok(Some(index))
    }

    /// Auxiliary function to find the index of a fork containing the provided
    /// header hash in its proposals.
    fn find_fork_by_header(&self, fork_header: &HeaderHash) -> Option<usize> {
        for (index, fork) in self.forks.iter().enumerate() {
            for p in fork.proposals.iter().rev() {
                if p == fork_header {
                    return Some(index)
                }
            }
        }
        None
    }

    /// Auxiliary function to retrieve the fork header hash of provided height.
    /// The fork is identified by the provided header hash.
    pub async fn get_fork_header_hash(
        &self,
        height: u32,
        fork_header: &HeaderHash,
    ) -> Result<Option<HeaderHash>> {
        // Find the fork containing the provided header
        let Some(index) = self.find_fork_by_header(fork_header) else { return Ok(None) };

        // Grab header if it exists
        let header =
            self.forks[index].overlay.lock().unwrap().blocks.get_order(&[height], false)?[0];

        Ok(header)
    }

    /// Auxiliary function to retrieve the fork headers of provided hashes.
    /// The fork is identified by the provided header hash. If fork doesn't
    /// exists, an empty vector is returned.
    pub async fn get_fork_headers(
        &self,
        headers: &[HeaderHash],
        fork_header: &HeaderHash,
    ) -> Result<Vec<Header>> {
        // Find the fork containing the provided header
        let Some(index) = self.find_fork_by_header(fork_header) else { return Ok(vec![]) };

        // Grab headers
        let headers = self.forks[index].overlay.lock().unwrap().get_headers_by_hash(headers)?;

        Ok(headers)
    }

    /// Auxiliary function to retrieve the fork proposals of provided hashes.
    /// The fork is identified by the provided header hash. If fork doesn't
    /// exists, an empty vector is returned.
    pub async fn get_fork_proposals(
        &self,
        headers: &[HeaderHash],
        fork_header: &HeaderHash,
    ) -> Result<Vec<Proposal>> {
        // Find the fork containing the provided header
        let Some(index) = self.find_fork_by_header(fork_header) else { return Ok(vec![]) };

        // Grab proposals
        let blocks = self.forks[index].overlay.lock().unwrap().get_blocks_by_hash(headers)?;
        let mut proposals = Vec::with_capacity(blocks.len());
        for block in blocks {
            proposals.push(Proposal::new(block));
        }

        Ok(proposals)
    }

    /// Auxiliary function to retrieve a fork proposals, starting from provided tip.
    /// If provided tip is too far behind, unknown, or fork doesn't exists, an empty
    /// vector is returned. The fork is identified by the optional provided header hash.
    /// If its `None`, we use our best fork.
    pub async fn get_fork_proposals_after(
        &self,
        tip: HeaderHash,
        fork_tip: Option<HeaderHash>,
        limit: u32,
    ) -> Result<Vec<Proposal>> {
        // Create return vector
        let mut proposals = vec![];

        // Grab fork index to use
        let index = match fork_tip {
            Some(fork_tip) => {
                let Some(found) = self.find_fork_by_header(&fork_tip) else { return Ok(proposals) };
                found
            }
            None => best_fork_index(&self.forks)?,
        };

        // Check tip exists
        let Ok(existing_tips) =
            self.forks[index].overlay.lock().unwrap().get_blocks_by_hash(&[tip])
        else {
            return Ok(proposals)
        };

        // Check tip is not far behind
        let last_block_height = self.forks[index].overlay.lock().unwrap().last()?.0;
        if last_block_height.saturating_sub(existing_tips[0].header.height) >= limit {
            return Ok(proposals)
        }

        // Retrieve all proposals after requested one
        let headers = self.blockchain.blocks.get_all_after(existing_tips[0].header.height)?;
        let blocks = self.blockchain.get_blocks_by_hash(&headers)?;
        for block in blocks {
            proposals.push(Proposal::new(block));
        }
        let blocks = self.forks[index]
            .overlay
            .lock()
            .unwrap()
            .get_blocks_by_hash(&self.forks[index].proposals)?;
        for block in blocks {
            // Add fork proposals after the requested one height
            if block.header.height > existing_tips[0].header.height {
                proposals.push(Proposal::new(block));
            }
        }

        Ok(proposals)
    }

    /// Auxiliary function to grab current mining RandomX key,
    /// based on next block height.
    /// If no forks exist, returns the canonical key.
    pub async fn current_mining_randomx_key(&self) -> Result<HeaderHash> {
        // Grab next block height and current keys.
        // If no forks exist, use canonical keys
        let (next_block_height, rx_keys) = if self.forks.is_empty() {
            let (next_block_height, _) = self.blockchain.last()?;
            (next_block_height + 1, self.module.darkfi_rx_keys)
        } else {
            // Grab best fork and its last proposal
            let index = best_fork_index(&self.forks)?;
            let fork = &self.forks[index];
            let last = fork.last_proposal()?;
            (last.block.header.height + 1, fork.module.darkfi_rx_keys)
        };

        // We only use the next key when the next block is the
        // height changing one.
        if next_block_height > RANDOMX_KEY_CHANGING_HEIGHT &&
            next_block_height % RANDOMX_KEY_CHANGING_HEIGHT == RANDOMX_KEY_CHANGE_DELAY
        {
            Ok(rx_keys.1.ok_or_else(|| Error::ParseFailed("darkfi_rx_keys.1 unwrap() error"))?)
        } else {
            Ok(rx_keys.0)
        }
    }

    /// Auxiliary function to grab best current fork full clone.
    pub async fn best_current_fork(&self) -> Result<Fork> {
        let index = best_fork_index(&self.forks)?;
        self.forks[index].full_clone()
    }

    /// Auxiliary function to retrieve current best fork last header.
    /// If no forks exist, grab the last header from canonical.
    pub async fn best_fork_last_header(&self) -> Result<(u32, HeaderHash)> {
        // Check if node has any forks
        if self.forks.is_empty() {
            return self.blockchain.last()
        }

        // Grab best fork
        let index = best_fork_index(&self.forks)?;
        let fork = &self.forks[index];

        // Grab its last header
        let last = fork.last_proposal()?;
        Ok((last.block.header.height, last.hash))
    }

    /// Auxiliary function to purge current forks and reset the ones
    /// starting with the provided prefix, excluding provided confirmed
    /// fork. Additionally, remove confirmed transactions from the
    /// forks mempools. This function assumes that the prefix blocks
    /// have already been appended to canonical chain from the
    /// confirmed fork.
    ///
    /// Note: Always remember to purge new trees from the database if
    /// not needed.
    pub async fn reset_forks(
        &mut self,
        prefix: &[HeaderHash],
        confirmed_fork_index: &usize,
        confirmed_txs: &[Transaction],
    ) -> Result<()> {
        // Find all the forks that start with the provided prefix,
        // excluding confirmed fork index, and remove their prefixed
        // proposals, and their corresponding diffs. If the fork is not
        // starting with the provided prefix, drop it.
        let excess = prefix.len();
        let prefix_last_index = excess - 1;
        let prefix_last = prefix.last().unwrap();
        let mut keep = vec![true; self.forks.len()];
        let confirmed_txs_hashes: Vec<TransactionHash> =
            confirmed_txs.iter().map(|tx| tx.hash()).collect();
        for (index, fork) in self.forks.iter_mut().enumerate() {
            if &index == confirmed_fork_index {
                // Remove confirmed proposals txs from fork's mempool
                fork.mempool.retain(|tx| !confirmed_txs_hashes.contains(tx));
                continue
            }

            // If a fork is empty, has less proposals than the prefix
            // or it doesn't start with the provided prefix we mark it
            // for removal. It's sufficient to check the prefix last
            // as the hashes sequence matching is enforced by it, since
            // it contains all previous ones.
            if fork.proposals.is_empty() ||
                prefix_last_index >= fork.proposals.len() ||
                &fork.proposals[prefix_last_index] != prefix_last
            {
                keep[index] = false;
                continue
            }

            // Remove confirmed proposals txs from fork's mempool
            fork.mempool.retain(|tx| !confirmed_txs_hashes.contains(tx));

            // Remove the commited differences
            let rest_proposals = fork.proposals.split_off(excess);
            let rest_diffs = fork.diffs.split_off(excess);
            let mut diffs = fork.diffs.clone();
            fork.proposals = rest_proposals;
            fork.diffs = rest_diffs;
            for diff in diffs.iter_mut() {
                fork.overlay.lock().unwrap().overlay.lock().unwrap().remove_diff(diff);
            }
        }

        // Drop invalid forks
        let mut iter = keep.iter();
        self.forks.retain(|_| *iter.next().unwrap());

        // Remove confirmed proposals txs from the unporposed txs sled tree
        self.blockchain.remove_pending_txs_hashes(&confirmed_txs_hashes)?;

        Ok(())
    }

    /// Auxiliary function to fully purge current forks and leave only a new empty fork.
    pub async fn purge_forks(&mut self) -> Result<()> {
        debug!(target: "validator::consensus::purge_forks", "Purging current forks...");
        self.forks = vec![Fork::new(self.blockchain.clone(), self.module.clone()).await?];
        debug!(target: "validator::consensus::purge_forks", "Forks purged!");

        Ok(())
    }

    /// Auxiliary function to reset PoW module.
    pub async fn reset_pow_module(&mut self) -> Result<()> {
        debug!(target: "validator::consensus::reset_pow_module", "Resetting PoW module...");
        self.module = PoWModule::new(
            self.blockchain.clone(),
            self.module.target,
            self.module.fixed_difficulty.clone(),
            None,
        )?;
        debug!(target: "validator::consensus::reset_pow_module", "PoW module reset successfully!");

        Ok(())
    }

    /// Auxiliary function to check current contracts states
    /// Monotree(SMT) validity in all active forks and canonical.
    pub async fn healthcheck(&self) -> Result<()> {
        // Grab current canonical contracts states monotree root
        let state_root = self.blockchain.contracts.get_state_monotree_root()?;

        // Check that the root matches last block header state root
        let last_block_state_root = self.blockchain.last_header()?.state_root;
        if state_root != last_block_state_root {
            return Err(Error::ContractsStatesRootError(
                blake3::Hash::from_bytes(state_root).to_string(),
                blake3::Hash::from_bytes(last_block_state_root).to_string(),
            ));
        }

        // Check each fork health
        for fork in &self.forks {
            fork.healthcheck()?;
        }

        Ok(())
    }

    /// Auxiliary function to purge all unreferenced contract trees
    /// from the database.
    pub async fn purge_unreferenced_trees(
        &self,
        referenced_trees: &mut BTreeSet<IVec>,
    ) -> Result<()> {
        // Check if we have forks
        if self.forks.is_empty() {
            // If no forks exist, build a new one so we retrieve the
            // native/protected trees references.
            let fork = Fork::new(self.blockchain.clone(), self.module.clone()).await?;
            fork.referenced_trees(referenced_trees);
        } else {
            // Iterate over current forks to retrieve referenced trees
            for fork in &self.forks {
                fork.referenced_trees(referenced_trees);
            }
        }

        // Retrieve current database trees
        let current_trees = self.blockchain.sled_db.tree_names();

        // Iterate over current database trees and drop unreferenced
        // contracts ones.
        for tree in current_trees {
            // Check if its referenced
            if referenced_trees.contains(&tree) {
                continue
            }

            // Check if its a contract tree pointer
            let Ok(tree) = deserialize::<[u8; 32]>(&tree) else { continue };

            // Drop it
            debug!(target: "validator::consensus::purge_unreferenced_trees", "Dropping unreferenced tree: {}", blake3::Hash::from(tree));
            self.blockchain.sled_db.drop_tree(tree)?;
        }

        Ok(())
    }

    /// Auxiliary function to purge all unproposed pending
    /// transactions from the database.
    pub async fn purge_unproposed_pending_txs(
        &mut self,
        mut proposed_txs: HashSet<TransactionHash>,
    ) -> Result<()> {
        // Iterate over all forks to find proposed txs
        for fork in &self.forks {
            // Grab all current proposals transactions hashes
            let proposals_txs =
                fork.overlay.lock().unwrap().get_blocks_txs_hashes(&fork.proposals)?;
            for tx in proposals_txs {
                proposed_txs.insert(tx);
            }
        }

        // Iterate over all forks again to remove unproposed txs from
        // their mempools.
        for fork in self.forks.iter_mut() {
            fork.mempool.retain(|tx| proposed_txs.contains(tx));
        }

        // Remove unproposed txs from the pending store
        let proposed_txs: Vec<TransactionHash> = proposed_txs.into_iter().collect();
        self.blockchain.reset_pending_txs(&proposed_txs)?;

        Ok(())
    }
}

/// This struct represents a block proposal, used for consensus.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Proposal {
    /// Block hash
    pub hash: HeaderHash,
    /// Block data
    pub block: BlockInfo,
}

impl Proposal {
    pub fn new(block: BlockInfo) -> Self {
        let hash = block.hash();
        Self { hash, block }
    }
}

impl From<Proposal> for BlockInfo {
    fn from(proposal: Proposal) -> BlockInfo {
        proposal.block
    }
}

/// Struct representing a forked blockchain state.
///
/// An overlay over the original blockchain is used, containing all
/// pending to-write records. Additionally, each fork keeps a vector of
/// valid pending transactions hashes, in order of receival, and the
/// proposals hashes sequence, for validations.
#[derive(Clone)]
pub struct Fork {
    /// Canonical (confirmed) blockchain
    pub blockchain: Blockchain,
    /// Overlay cache over canonical Blockchain
    pub overlay: BlockchainOverlayPtr,
    /// Current PoW module state
    pub module: PoWModule,
    /// Fork proposal hashes sequence
    pub proposals: Vec<HeaderHash>,
    /// Fork proposal overlay diffs sequence
    pub diffs: Vec<SledDbOverlayStateDiff>,
    /// Valid pending transaction hashes
    pub mempool: Vec<TransactionHash>,
    /// Current fork mining targets rank, cached for better performance
    pub targets_rank: BigUint,
    /// Current fork hashes rank, cached for better performance
    pub hashes_rank: BigUint,
}

impl Fork {
    pub async fn new(blockchain: Blockchain, module: PoWModule) -> Result<Self> {
        let mempool = blockchain.get_pending_txs()?.iter().map(|tx| tx.hash()).collect();
        let overlay = BlockchainOverlay::new(&blockchain)?;
        // Retrieve last block difficulty to access current ranks
        let last_difficulty = blockchain.last_block_difficulty()?;
        let targets_rank = last_difficulty.ranks.targets_rank;
        let hashes_rank = last_difficulty.ranks.hashes_rank;
        Ok(Self {
            blockchain,
            overlay,
            module,
            proposals: vec![],
            diffs: vec![],
            mempool,
            targets_rank,
            hashes_rank,
        })
    }

    /// Auxiliary function to append a proposal and update current fork rank.
    pub async fn append_proposal(&mut self, proposal: &Proposal) -> Result<()> {
        // Grab next mine target and difficulty
        let (next_target, next_difficulty) = self.module.next_mine_target_and_difficulty()?;

        // Calculate block rank
        let (target_distance_sq, hash_distance_sq) = block_rank(&proposal.block, &next_target)?;

        // Update fork ranks
        self.targets_rank += target_distance_sq.clone();
        self.hashes_rank += hash_distance_sq.clone();

        // Generate block difficulty and update PoW module
        let cumulative_difficulty =
            self.module.cumulative_difficulty.clone() + next_difficulty.clone();
        let ranks = BlockRanks::new(
            target_distance_sq,
            self.targets_rank.clone(),
            hash_distance_sq,
            self.hashes_rank.clone(),
        );
        let block_difficulty = BlockDifficulty::new(
            proposal.block.header.height,
            proposal.block.header.timestamp,
            next_difficulty,
            cumulative_difficulty,
            ranks,
        );
        self.module.append_difficulty(&self.overlay, &proposal.block.header, block_difficulty)?;

        // Push proposal's hash
        self.proposals.push(proposal.hash);

        // Push proposal overlay diff
        self.diffs.push(self.overlay.lock().unwrap().overlay.lock().unwrap().diff(&self.diffs)?);

        Ok(())
    }

    /// Auxiliary function to retrieve last proposal.
    pub fn last_proposal(&self) -> Result<Proposal> {
        let block = if let Some(last) = self.proposals.last() {
            self.overlay.lock().unwrap().get_blocks_by_hash(&[*last])?[0].clone()
        } else {
            self.overlay.lock().unwrap().last_block()?
        };

        Ok(Proposal::new(block))
    }

    /// Auxiliary function to compute forks' next block height.
    pub fn get_next_block_height(&self) -> Result<u32> {
        let proposal = self.last_proposal()?;
        Ok(proposal.block.header.height + 1)
    }

    /// Auxiliary function to retrieve unproposed valid transactions,
    /// along with their total gas used and total paid fees.
    ///
    /// Note: Always remember to purge new trees from the database if
    /// not needed.
    pub async fn unproposed_txs(
        &mut self,
        verifying_block_height: u32,
        verify_fees: bool,
    ) -> Result<(Vec<Transaction>, u64, u64)> {
        // Check if our mempool is not empty
        if self.mempool.is_empty() {
            return Ok((vec![], 0, 0))
        }

        // Transactions Merkle tree
        let mut tree = MerkleTree::new(1);

        // Total gas accumulators
        let mut total_gas_used = 0_u64;
        let mut total_gas_paid = 0_u64;

        // Map of ZK proof verifying keys for the current transaction batch
        let mut vks: HashMap<[u8; 32], HashMap<String, VerifyingKey>> = HashMap::new();

        // Grab all current proposals transactions hashes
        let proposals_txs = self.overlay.lock().unwrap().get_blocks_txs_hashes(&self.proposals)?;

        // Iterate through all pending transactions in the forks' mempool
        let mut unproposed_txs = vec![];
        let mut erroneous_txs = vec![];
        for tx in &self.mempool {
            // If the hash is contained in the proposals transactions vec, skip it
            if proposals_txs.contains(tx) {
                continue
            }

            // Retrieve the actual unproposed transaction
            let unproposed_tx = match self.blockchain.transactions.get_pending(&[*tx], true) {
                Ok(txs) => txs[0].clone().unwrap(),
                Err(e) => {
                    debug!(target: "validator::consensus::unproposed_txs", "Transaction retrieval failed: {e}");
                    erroneous_txs.push(*tx);
                    continue
                }
            };

            // Update the verifying keys map
            for call in &unproposed_tx.calls {
                vks.entry(call.data.contract_id.to_bytes()).or_default();
            }

            // Verify the transaction against current state
            self.overlay.lock().unwrap().checkpoint();
            let gas_data = match verify_transaction(
                &self.overlay,
                verifying_block_height,
                self.module.target,
                &unproposed_tx,
                &mut tree,
                &mut vks,
                verify_fees,
            )
            .await
            {
                Ok(gas_values) => gas_values,
                Err(e) => {
                    debug!(target: "validator::consensus::unproposed_txs", "Transaction verification failed: {e}");
                    self.overlay.lock().unwrap().revert_to_checkpoint();
                    erroneous_txs.push(*tx);
                    continue
                }
            };

            // Store the gas used by the verified transaction
            let tx_gas_used = gas_data.total_gas_used();

            // Calculate current accumulated gas usage
            let accumulated_gas_usage = total_gas_used.saturating_add(tx_gas_used);

            // Check gas limit - if accumulated gas used exceeds it, break out of loop
            if accumulated_gas_usage > BLOCK_GAS_LIMIT {
                warn!(
                    target: "validator::consensus::unproposed_txs",
                    "Retrieving transaction {tx} would exceed configured unproposed transaction gas limit: {accumulated_gas_usage} - {BLOCK_GAS_LIMIT}"
                );
                self.overlay.lock().unwrap().revert_to_checkpoint();
                break
            }

            // Update accumulated total gas
            total_gas_used = total_gas_used.saturating_add(tx_gas_used);
            total_gas_paid = total_gas_paid.saturating_add(gas_data.paid);

            // Push the tx hash into the unproposed transactions vector
            unproposed_txs.push(unproposed_tx);
        }

        // Remove erroneous transactions txs from fork's mempool
        self.mempool.retain(|tx| !erroneous_txs.contains(tx));

        Ok((unproposed_txs, total_gas_used, total_gas_paid))
    }

    /// Auxiliary function to create a full clone using
    /// BlockchainOverlay::full_clone. Changes to this copy don't
    /// affect original fork overlay records, since underlying overlay
    /// pointer have been updated to the cloned one.
    pub fn full_clone(&self) -> Result<Self> {
        let blockchain = self.blockchain.clone();
        let overlay = self.overlay.lock().unwrap().full_clone()?;
        let module = self.module.clone();
        let proposals = self.proposals.clone();
        let diffs = self.diffs.clone();
        let mempool = self.mempool.clone();
        let targets_rank = self.targets_rank.clone();
        let hashes_rank = self.hashes_rank.clone();

        Ok(Self {
            blockchain,
            overlay,
            module,
            proposals,
            diffs,
            mempool,
            targets_rank,
            hashes_rank,
        })
    }

    /// Auxiliary function to check current contracts states
    /// Monotree(SMT) validity.
    ///
    /// Note: This should be executed on fresh forks and/or when
    ///       a fork doesn't contain changes over the last appended
    //        proposal.
    pub fn healthcheck(&self) -> Result<()> {
        // Grab current contracts states monotree root
        let state_root = self.overlay.lock().unwrap().contracts.get_state_monotree_root()?;

        // Check that the root matches last block header state root
        let last_block_state_root = self.last_proposal()?.block.header.state_root;
        if state_root != last_block_state_root {
            return Err(Error::ContractsStatesRootError(
                blake3::Hash::from_bytes(state_root).to_string(),
                blake3::Hash::from_bytes(last_block_state_root).to_string(),
            ));
        }

        Ok(())
    }

    /// Auxiliary function to retrieve all referenced trees from the
    /// fork overlay and insert them to provided `BTreeSet`.
    pub fn referenced_trees(&self, trees: &mut BTreeSet<IVec>) {
        // Grab its current overlay
        let fork_overlay = self.overlay.lock().unwrap();
        let overlay = fork_overlay.overlay.lock().unwrap();

        // Retrieve its initial trees
        for initial_tree in &overlay.state.initial_tree_names {
            trees.insert(initial_tree.clone());
        }

        // Retrieve its new trees
        for new_tree in &overlay.state.new_tree_names {
            trees.insert(new_tree.clone());
        }

        // Retrieve its dropped trees
        for dropped_tree in overlay.state.dropped_trees.keys() {
            trees.insert(dropped_tree.clone());
        }

        // Retrieve its protected trees
        for protected_tree in &overlay.state.protected_tree_names {
            trees.insert(protected_tree.clone());
        }
    }
}
