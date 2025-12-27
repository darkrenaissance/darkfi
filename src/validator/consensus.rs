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

use std::collections::{HashMap, HashSet};

use darkfi_sdk::{
    crypto::MerkleTree,
    monotree::{self, Monotree},
    tx::TransactionHash,
};
use darkfi_serial::{async_trait, SerialDecodable, SerialEncodable};
use num_bigint::BigUint;
use sled_overlay::database::SledDbOverlayStateDiff;
use smol::lock::RwLock;
use tracing::{debug, error, info, warn};

use crate::{
    blockchain::{
        block_store::{BlockDifficulty, BlockRanks},
        BlockInfo, Blockchain, BlockchainOverlay, BlockchainOverlayPtr, Header, HeaderHash,
    },
    runtime::vm_runtime::GAS_LIMIT,
    tx::{Transaction, MAX_TX_CALLS},
    validator::{
        pow::{PoWModule, RANDOMX_KEY_CHANGE_DELAY, RANDOMX_KEY_CHANGING_HEIGHT},
        utils::{best_fork_index, block_rank, find_extended_fork_index},
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
    pub forks: RwLock<Vec<Fork>>,
    /// Canonical blockchain PoW module state
    pub module: RwLock<PoWModule>,
    /// Lock to restrict when proposals appends can happen
    pub append_lock: RwLock<()>,
}

impl Consensus {
    /// Generate a new Consensus state.
    pub fn new(
        blockchain: Blockchain,
        confirmation_threshold: usize,
        pow_target: u32,
        pow_fixed_difficulty: Option<BigUint>,
    ) -> Result<Self> {
        let forks = RwLock::new(vec![]);

        let module = RwLock::new(PoWModule::new(
            blockchain.clone(),
            pow_target,
            pow_fixed_difficulty,
            None,
        )?);

        let append_lock = RwLock::new(());

        Ok(Self { blockchain, confirmation_threshold, forks, module, append_lock })
    }

    /// Generate a new empty fork.
    pub async fn generate_empty_fork(&self) -> Result<()> {
        debug!(target: "validator::consensus::generate_empty_fork", "Generating new empty fork...");
        let mut forks = self.forks.write().await;
        // Check if we already have an empty fork
        for fork in forks.iter() {
            if fork.proposals.is_empty() {
                debug!(target: "validator::consensus::generate_empty_fork", "An empty fork already exists.");
                drop(forks);
                return Ok(())
            }
        }
        let fork = Fork::new(self.blockchain.clone(), self.module.read().await.clone()).await?;
        forks.push(fork);
        drop(forks);
        debug!(target: "validator::consensus::generate_empty_fork", "Fork generated!");
        Ok(())
    }

    /// Given a proposal, the node verifys it and finds which fork it extends.
    /// If the proposal extends the canonical blockchain, a new fork chain is created.
    pub async fn append_proposal(&self, proposal: &Proposal, verify_fees: bool) -> Result<()> {
        debug!(target: "validator::consensus::append_proposal", "Appending proposal {}", proposal.hash);

        // Check if proposal already exists
        let lock = self.forks.read().await;
        for fork in lock.iter() {
            for p in fork.proposals.iter().rev() {
                if p == &proposal.hash {
                    drop(lock);
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
                drop(lock);
                debug!(target: "validator::consensus::append_proposal", "Proposal {} already exists", proposal.hash);
                return Err(Error::ProposalAlreadyExists)
            }
        }
        drop(lock);

        // Verify proposal and grab corresponding fork
        let (mut fork, index) = verify_proposal(self, proposal, verify_fees).await?;

        // Append proposal to the fork
        fork.append_proposal(proposal).await?;

        // TODO: to keep memory usage low, we should only append forks that
        // are higher ranking than our current best one

        // If a fork index was found, replace forks with the mutated one,
        // otherwise push the new fork.
        let mut lock = self.forks.write().await;
        match index {
            Some(i) => {
                if i < lock.len() && lock[i].proposals == fork.proposals[..fork.proposals.len() - 1]
                {
                    lock[i] = fork;
                } else {
                    lock.push(fork);
                }
            }
            None => {
                lock.push(fork);
            }
        }
        drop(lock);

        info!(target: "validator::consensus::append_proposal", "Appended proposal {}", proposal.hash);

        Ok(())
    }

    /// Given a proposal, find the fork chain it extends, and return its full clone.
    /// If the proposal extends the fork not on its tail, a new fork is created and
    /// we re-apply the proposals up to the extending one. If proposal extends canonical,
    /// a new fork is created. Additionally, we return the fork index if a new fork
    /// was not created, so caller can replace the fork.
    pub async fn find_extended_fork(&self, proposal: &Proposal) -> Result<(Fork, Option<usize>)> {
        // Grab a lock over current forks
        let forks = self.forks.read().await;

        // Check if proposal extends any fork
        let found = find_extended_fork_index(&forks, proposal);
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
            for (f_index, fork) in forks.iter().enumerate() {
                if fork.proposals.is_empty() {
                    return Ok((forks[f_index].full_clone()?, Some(f_index)))
                }
            }

            // Generate a new fork extending canonical
            let fork = Fork::new(self.blockchain.clone(), self.module.read().await.clone()).await?;
            return Ok((fork, None))
        }

        let (f_index, p_index) = found.unwrap();
        let original_fork = &forks[f_index];
        // Check if proposal extends fork at last proposal
        if p_index == (original_fork.proposals.len() - 1) {
            return Ok((original_fork.full_clone()?, Some(f_index)))
        }

        // Rebuild fork
        let mut fork = Fork::new(self.blockchain.clone(), self.module.read().await.clone()).await?;
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

        // Rebuild fork contracts states monotree
        fork.compute_monotree()?;

        // Drop forks lock
        drop(forks);

        Ok((fork, None))
    }

    /// Check if best fork proposals can be confirmed.
    /// Consensus confirmation logic:
    /// - If the current best fork has reached greater length than the security threshold,
    ///   and no other fork exist with same rank, first proposal(s) in that fork can be
    ///   appended to canonical blockchain (confirme).
    ///
    /// When best fork can be confirmed, first block(s) should be appended to canonical,
    /// and forks should be rebuilt.
    pub async fn confirmation(&self) -> Result<Option<usize>> {
        debug!(target: "validator::consensus::confirmation", "Started confirmation check");

        // Grab best fork
        let forks = self.forks.read().await;
        let index = best_fork_index(&forks)?;
        let fork = &forks[index];

        // Check its length
        let length = fork.proposals.len();
        if length < self.confirmation_threshold {
            debug!(target: "validator::consensus::confirmation", "Nothing to confirme yet, best fork size: {length}");
            drop(forks);
            return Ok(None)
        }

        // Drop forks lock
        drop(forks);

        Ok(Some(index))
    }

    /// Auxiliary function to retrieve the fork header hash of provided height.
    /// The fork is identified by the provided header hash.
    pub async fn get_fork_header_hash(
        &self,
        height: u32,
        fork_header: &HeaderHash,
    ) -> Result<Option<HeaderHash>> {
        // Grab a lock over current forks
        let forks = self.forks.read().await;

        // Find the fork containing the provided header
        let mut found = None;
        'outer: for (index, fork) in forks.iter().enumerate() {
            for p in fork.proposals.iter().rev() {
                if p == fork_header {
                    found = Some(index);
                    break 'outer
                }
            }
        }
        if found.is_none() {
            drop(forks);
            return Ok(None)
        }
        let index = found.unwrap();

        // Grab header if it exists
        let header = forks[index].overlay.lock().unwrap().blocks.get_order(&[height], false)?[0];

        // Drop forks lock
        drop(forks);

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
        // Grab a lock over current forks
        let forks = self.forks.read().await;

        // Find the fork containing the provided header
        let mut found = None;
        'outer: for (index, fork) in forks.iter().enumerate() {
            for p in fork.proposals.iter().rev() {
                if p == fork_header {
                    found = Some(index);
                    break 'outer
                }
            }
        }
        let Some(index) = found else {
            drop(forks);
            return Ok(vec![])
        };

        // Grab headers
        let headers = forks[index].overlay.lock().unwrap().get_headers_by_hash(headers)?;

        // Drop forks lock
        drop(forks);

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
        // Grab a lock over current forks
        let forks = self.forks.read().await;

        // Find the fork containing the provided header
        let mut found = None;
        'outer: for (index, fork) in forks.iter().enumerate() {
            for p in fork.proposals.iter().rev() {
                if p == fork_header {
                    found = Some(index);
                    break 'outer
                }
            }
        }
        let Some(index) = found else {
            drop(forks);
            return Ok(vec![])
        };

        // Grab proposals
        let blocks = forks[index].overlay.lock().unwrap().get_blocks_by_hash(headers)?;
        let mut proposals = Vec::with_capacity(blocks.len());
        for block in blocks {
            proposals.push(Proposal::new(block));
        }

        // Drop forks lock
        drop(forks);

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
        // Grab a lock over current forks
        let forks = self.forks.read().await;

        // Create return vector
        let mut proposals = vec![];

        // Grab fork index to use
        let index = match fork_tip {
            Some(fork_tip) => {
                let mut found = None;
                'outer: for (index, fork) in forks.iter().enumerate() {
                    for p in fork.proposals.iter().rev() {
                        if p == &fork_tip {
                            found = Some(index);
                            break 'outer
                        }
                    }
                }
                if found.is_none() {
                    drop(forks);
                    return Ok(proposals)
                }
                found.unwrap()
            }
            None => best_fork_index(&forks)?,
        };

        // Check tip exists
        let Ok(existing_tips) = forks[index].overlay.lock().unwrap().get_blocks_by_hash(&[tip])
        else {
            drop(forks);
            return Ok(proposals)
        };

        // Check tip is not far behind
        let last_block_height = forks[index].overlay.lock().unwrap().last()?.0;
        if last_block_height - existing_tips[0].header.height >= limit {
            drop(forks);
            return Ok(proposals)
        }

        // Retrieve all proposals after requested one
        let headers = self.blockchain.blocks.get_all_after(existing_tips[0].header.height)?;
        let blocks = self.blockchain.get_blocks_by_hash(&headers)?;
        for block in blocks {
            proposals.push(Proposal::new(block));
        }
        let blocks =
            forks[index].overlay.lock().unwrap().get_blocks_by_hash(&forks[index].proposals)?;
        for block in blocks {
            proposals.push(Proposal::new(block));
        }

        // Drop forks lock
        drop(forks);

        Ok(proposals)
    }

    /// Auxiliary function to grab current mining RandomX key,
    /// based on next block height.
    /// If no forks exist, returns the canonical key.
    pub async fn current_mining_randomx_key(&self) -> Result<HeaderHash> {
        // Grab a lock over current forks
        let forks = self.forks.read().await;

        // Grab next block height and current keys.
        // If no forks exist, use canonical keys
        let (next_block_height, rx_keys) = if forks.is_empty() {
            let (next_block_height, _) = self.blockchain.last()?;
            (next_block_height + 1, self.module.read().await.darkfi_rx_keys)
        } else {
            // Grab best fork and its last proposal
            let fork = &forks[best_fork_index(&forks)?];
            let last = fork.last_proposal()?;
            (last.block.header.height + 1, fork.module.darkfi_rx_keys)
        };

        // We only use the next key when the next block is the
        // height changing one.
        if next_block_height > RANDOMX_KEY_CHANGING_HEIGHT &&
            next_block_height % RANDOMX_KEY_CHANGING_HEIGHT == RANDOMX_KEY_CHANGE_DELAY
        {
            Ok(rx_keys.1)
        } else {
            Ok(rx_keys.0)
        }
    }

    /// Auxiliary function to grab best current fork full clone.
    pub async fn best_current_fork(&self) -> Result<Fork> {
        let forks = self.forks.read().await;
        let index = best_fork_index(&forks)?;
        forks[index].full_clone()
    }

    /// Auxiliary function to retrieve current best fork last header.
    /// If no forks exist, grab the last header from canonical.
    pub async fn best_fork_last_header(&self) -> Result<(u32, HeaderHash)> {
        // Grab a lock over current forks
        let forks = self.forks.read().await;

        // Check if node has any forks
        if forks.is_empty() {
            drop(forks);
            return self.blockchain.last()
        }

        // Grab best fork
        let fork = &forks[best_fork_index(&forks)?];

        // Grab its last header
        let last = fork.last_proposal()?;
        drop(forks);
        Ok((last.block.header.height, last.hash))
    }

    /// Auxiliary function to purge current forks and reset the ones starting
    /// with the provided prefix, excluding provided confirmed fork.
    /// Additionally, remove confirmed transactions from the forks mempools,
    /// along with the unporposed transactions sled trees.
    /// This function assumes that the prefix blocks have already been appended
    /// to canonical chain from the confirmed fork.
    pub async fn reset_forks(
        &self,
        prefix: &[HeaderHash],
        confirmed_fork_index: &usize,
        confirmed_txs: &[Transaction],
    ) -> Result<()> {
        // Grab a lock over current forks
        let mut forks = self.forks.write().await;

        // Find all the forks that start with the provided prefix,
        // excluding confirmed fork index, and remove their prefixed
        // proposals, and their corresponding diffs.
        // If the fork is not starting with the provided prefix,
        // drop it. Additionally, keep track of all the referenced
        // trees in overlays that are valid.
        let excess = prefix.len();
        let prefix_last_index = excess - 1;
        let prefix_last = prefix.last().unwrap();
        let mut keep = vec![true; forks.len()];
        let mut referenced_trees = HashSet::new();
        let mut referenced_txs = HashSet::new();
        let confirmed_txs_hashes: Vec<TransactionHash> =
            confirmed_txs.iter().map(|tx| tx.hash()).collect();
        for (index, fork) in forks.iter_mut().enumerate() {
            if &index == confirmed_fork_index {
                // Store its tree references
                let fork_overlay = fork.overlay.lock().unwrap();
                let overlay = fork_overlay.overlay.lock().unwrap();
                for tree in &overlay.state.initial_tree_names {
                    referenced_trees.insert(tree.clone());
                }
                for tree in &overlay.state.new_tree_names {
                    referenced_trees.insert(tree.clone());
                }
                for tree in overlay.state.dropped_trees.keys() {
                    referenced_trees.insert(tree.clone());
                }
                // Remove confirmed proposals txs from fork's mempool
                fork.mempool.retain(|tx| !confirmed_txs_hashes.contains(tx));
                // Store its txs references
                for tx in &fork.mempool {
                    referenced_txs.insert(*tx);
                }
                drop(overlay);
                drop(fork_overlay);
                continue
            }

            if fork.proposals.is_empty() ||
                prefix_last_index >= fork.proposals.len() ||
                &fork.proposals[prefix_last_index] != prefix_last
            {
                keep[index] = false;
                continue
            }

            // Remove confirmed proposals txs from fork's mempool
            fork.mempool.retain(|tx| !confirmed_txs_hashes.contains(tx));
            // Store its txs references
            for tx in &fork.mempool {
                referenced_txs.insert(*tx);
            }

            // Remove the commited differences
            let rest_proposals = fork.proposals.split_off(excess);
            let rest_diffs = fork.diffs.split_off(excess);
            let mut diffs = fork.diffs.clone();
            fork.proposals = rest_proposals;
            fork.diffs = rest_diffs;
            for diff in diffs.iter_mut() {
                fork.overlay.lock().unwrap().overlay.lock().unwrap().remove_diff(diff);
            }

            // Store its tree references
            let fork_overlay = fork.overlay.lock().unwrap();
            let overlay = fork_overlay.overlay.lock().unwrap();
            for tree in &overlay.state.initial_tree_names {
                referenced_trees.insert(tree.clone());
            }
            for tree in &overlay.state.new_tree_names {
                referenced_trees.insert(tree.clone());
            }
            for tree in overlay.state.dropped_trees.keys() {
                referenced_trees.insert(tree.clone());
            }
            drop(overlay);
            drop(fork_overlay);
        }

        // Find the trees and pending txs that are no longer referenced by valid forks
        let mut dropped_trees = HashSet::new();
        let mut dropped_txs = HashSet::new();
        for (index, fork) in forks.iter_mut().enumerate() {
            if keep[index] {
                continue
            }
            for tx in &fork.mempool {
                if !referenced_txs.contains(tx) {
                    dropped_txs.insert(*tx);
                }
            }
            let fork_overlay = fork.overlay.lock().unwrap();
            let overlay = fork_overlay.overlay.lock().unwrap();
            for tree in &overlay.state.initial_tree_names {
                if !referenced_trees.contains(tree) {
                    dropped_trees.insert(tree.clone());
                }
            }
            for tree in &overlay.state.new_tree_names {
                if !referenced_trees.contains(tree) {
                    dropped_trees.insert(tree.clone());
                }
            }
            for tree in overlay.state.dropped_trees.keys() {
                if !referenced_trees.contains(tree) {
                    dropped_trees.insert(tree.clone());
                }
            }
            drop(overlay);
            drop(fork_overlay);
        }

        // Drop unreferenced trees from the database
        for tree in dropped_trees {
            self.blockchain.sled_db.drop_tree(tree)?;
        }

        // Drop invalid forks
        let mut iter = keep.iter();
        forks.retain(|_| *iter.next().unwrap());

        // Remove confirmed proposals txs from the unporposed txs sled tree
        self.blockchain.remove_pending_txs_hashes(&confirmed_txs_hashes)?;

        // Remove unreferenced txs from the unporposed txs sled tree
        self.blockchain.remove_pending_txs_hashes(&Vec::from_iter(dropped_txs))?;

        // Drop forks lock
        drop(forks);

        Ok(())
    }

    /// Auxiliary function to fully purge current forks and leave only a new empty fork.
    pub async fn purge_forks(&self) -> Result<()> {
        debug!(target: "validator::consensus::purge_forks", "Purging current forks...");
        let mut forks = self.forks.write().await;
        *forks = vec![Fork::new(self.blockchain.clone(), self.module.read().await.clone()).await?];
        drop(forks);
        debug!(target: "validator::consensus::purge_forks", "Forks purged!");
        Ok(())
    }

    /// Auxiliary function to reset PoW module.
    pub async fn reset_pow_module(&self) -> Result<()> {
        debug!(target: "validator::consensus::reset_pow_module", "Resetting PoW module...");

        let mut module = self.module.write().await;
        *module = PoWModule::new(
            self.blockchain.clone(),
            module.target,
            module.fixed_difficulty.clone(),
            None,
        )?;
        drop(module);
        debug!(target: "validator::consensus::reset_pow_module", "PoW module reset successfully!");
        Ok(())
    }

    /// Auxiliary function to check current contracts states
    /// Monotree(SMT) validity in all active forks and canonical.
    pub async fn healthcheck(&self) -> Result<()> {
        // Grab a lock over current forks
        let lock = self.forks.read().await;

        // Rebuild current canonical contract states monotree
        let state_monotree = self.blockchain.get_state_monotree()?;

        // Check that the root matches last block header state root
        let Some(state_root) = state_monotree.get_headroot()? else {
            return Err(Error::ContractsStatesRootNotFoundError);
        };
        let last_block_state_root = self.blockchain.last_header()?.state_root;
        if state_root != last_block_state_root {
            return Err(Error::ContractsStatesRootError(
                blake3::Hash::from_bytes(state_root).to_string(),
                blake3::Hash::from_bytes(last_block_state_root).to_string(),
            ));
        }

        // Check each fork health
        for fork in lock.iter() {
            fork.healthcheck()?;
        }

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
/// An overlay over the original blockchain is used, containing all pending to-write
/// records. Additionally, each fork keeps a vector of valid pending transactions hashes,
/// in order of receival, and the proposals hashes sequence, for validations.
#[derive(Clone)]
pub struct Fork {
    /// Canonical (confirmed) blockchain
    pub blockchain: Blockchain,
    /// Overlay cache over canonical Blockchain
    pub overlay: BlockchainOverlayPtr,
    /// Current PoW module state
    pub module: PoWModule,
    /// Current contracts states Monotree(SMT)
    pub state_monotree: Monotree<monotree::MemoryDb>,
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
        // Build current contract states monotree
        let state_monotree = overlay.lock().unwrap().get_state_monotree()?;
        // Retrieve last block difficulty to access current ranks
        let last_difficulty = blockchain.last_block_difficulty()?;
        let targets_rank = last_difficulty.ranks.targets_rank;
        let hashes_rank = last_difficulty.ranks.hashes_rank;
        Ok(Self {
            blockchain,
            overlay,
            module,
            state_monotree,
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
    /// Note: Always remember to purge new trees from the overlay if not needed.
    pub async fn unproposed_txs(
        &self,
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
        let mut total_gas_used = 0;
        let mut total_gas_paid = 0;

        // Map of ZK proof verifying keys for the current transaction batch
        let mut vks: HashMap<[u8; 32], HashMap<String, VerifyingKey>> = HashMap::new();

        // Grab all current proposals transactions hashes
        let proposals_txs = self.overlay.lock().unwrap().get_blocks_txs_hashes(&self.proposals)?;

        // Iterate through all pending transactions in the forks' mempool
        let mut unproposed_txs = vec![];
        for tx in &self.mempool {
            // If the hash is contained in the proposals transactions vec, skip it
            if proposals_txs.contains(tx) {
                continue
            }

            // Retrieve the actual unproposed transaction
            let unproposed_tx =
                self.blockchain.transactions.get_pending(&[*tx], true)?[0].clone().unwrap();

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
                    self.overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
                    self.overlay.lock().unwrap().revert_to_checkpoint()?;
                    continue
                }
            };

            // Store the gas used by the verified transaction
            let tx_gas_used = gas_data.total_gas_used();

            // Calculate current accumulated gas usage
            let accumulated_gas_usage = total_gas_used + tx_gas_used;

            // Check gas limit - if accumulated gas used exceeds it, break out of loop
            if accumulated_gas_usage > BLOCK_GAS_LIMIT {
                warn!(
                    target: "validator::consensus::unproposed_txs",
                    "Retrieving transaction {tx} would exceed configured unproposed transaction gas limit: {accumulated_gas_usage} - {BLOCK_GAS_LIMIT}"
                );
                self.overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
                self.overlay.lock().unwrap().revert_to_checkpoint()?;
                break
            }

            // Update accumulated total gas
            total_gas_used += tx_gas_used;
            total_gas_paid += gas_data.paid;

            // Push the tx hash into the unproposed transactions vector
            unproposed_txs.push(unproposed_tx);
        }

        Ok((unproposed_txs, total_gas_used, total_gas_paid))
    }

    /// Auxiliary function to create a full clone using BlockchainOverlay::full_clone.
    /// Changes to this copy don't affect original fork overlay records, since underlying
    /// overlay pointer have been updated to the cloned one.
    pub fn full_clone(&self) -> Result<Self> {
        let blockchain = self.blockchain.clone();
        let overlay = self.overlay.lock().unwrap().full_clone()?;
        let module = self.module.clone();
        let state_monotree = self.state_monotree.clone();
        let proposals = self.proposals.clone();
        let diffs = self.diffs.clone();
        let mempool = self.mempool.clone();
        let targets_rank = self.targets_rank.clone();
        let hashes_rank = self.hashes_rank.clone();

        Ok(Self {
            blockchain,
            overlay,
            module,
            state_monotree,
            proposals,
            diffs,
            mempool,
            targets_rank,
            hashes_rank,
        })
    }

    /// Build current contract states monotree.
    pub fn compute_monotree(&mut self) -> Result<()> {
        self.state_monotree = self.overlay.lock().unwrap().get_state_monotree()?;
        Ok(())
    }

    /// Auxiliary function to check current contracts states
    /// Monotree(SMT) validity.
    ///
    /// Note: This should be executed on fresh forks and/or when
    ///       a fork doesn't contain changes over the last appended
    //        proposal.
    pub fn healthcheck(&self) -> Result<()> {
        // Rebuild current contract states monotree
        let state_monotree = self.overlay.lock().unwrap().get_state_monotree()?;

        // Check that it matches forks' tree
        let Some(state_root) = state_monotree.get_headroot()? else {
            return Err(Error::ContractsStatesRootNotFoundError);
        };
        let Some(fork_state_root) = self.state_monotree.get_headroot()? else {
            return Err(Error::ContractsStatesRootNotFoundError);
        };
        if state_root != fork_state_root {
            return Err(Error::ContractsStatesRootError(
                blake3::Hash::from_bytes(state_root).to_string(),
                blake3::Hash::from_bytes(fork_state_root).to_string(),
            ));
        }

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

    /// Auxiliary function to purge all new trees from the fork
    /// overlay.
    pub fn purge_new_trees(&self) {
        if let Err(e) = self.overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees() {
            error!(target: "validator::consensus::fork::purge_new_trees", "Purging new trees in the overlay failed: {e}");
        }
    }
}
