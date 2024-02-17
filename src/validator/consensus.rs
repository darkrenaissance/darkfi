/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use darkfi_sdk::crypto::{MerkleTree, SecretKey};
use darkfi_serial::{async_trait, serialize, SerialDecodable, SerialEncodable};
use log::{debug, error, info};
use num_bigint::BigUint;
use smol::lock::RwLock;

use crate::{
    blockchain::{BlockInfo, Blockchain, BlockchainOverlay, BlockchainOverlayPtr, Header},
    tx::Transaction,
    util::time::Timestamp,
    validator::{
        pow::PoWModule,
        utils::{best_forks_indexes, block_rank, find_extended_fork_index},
        verify_block, verify_proposal, verify_transactions, TxVerifyFailed,
    },
    Error, Result,
};

// Consensus configuration
/// Block/proposal maximum transactions, exluding producer transaction
pub const TXS_CAP: usize = 50;

/// This struct represents the information required by the consensus algorithm
pub struct Consensus {
    /// Canonical (finalized) blockchain
    pub blockchain: Blockchain,
    /// Fork size(length) after which it can be finalized
    pub finalization_threshold: usize,
    /// Fork chains containing block proposals
    pub forks: RwLock<Vec<Fork>>,
    /// Canonical blockchain PoW module state
    pub module: RwLock<PoWModule>,
}

impl Consensus {
    /// Generate a new Consensus state.
    pub fn new(
        blockchain: Blockchain,
        finalization_threshold: usize,
        pow_target: usize,
        pow_fixed_difficulty: Option<BigUint>,
    ) -> Result<Self> {
        let module =
            RwLock::new(PoWModule::new(blockchain.clone(), pow_target, pow_fixed_difficulty)?);
        Ok(Self { blockchain, finalization_threshold, forks: RwLock::new(vec![]), module })
    }

    /// Generate an unsigned block for provided fork, containing all
    /// pending transactions.
    pub async fn generate_unsigned_block(
        &self,
        fork: &Fork,
        producer_tx: Transaction,
    ) -> Result<BlockInfo> {
        // Grab forks' next block height
        let next_block_height = fork.get_next_block_height()?;

        // Grab forks' unproposed transactions
        let mut unproposed_txs = fork.unproposed_txs(&self.blockchain, next_block_height).await?;
        unproposed_txs.push(producer_tx);

        // Grab forks' last block proposal(previous)
        let previous = fork.last_proposal()?;

        // Generate the new header
        let header =
            Header::new(previous.block.hash()?, next_block_height, Timestamp::current_time(), 0);

        // Generate the block
        let mut block = BlockInfo::new_empty(header);

        // Add transactions to the block
        block.append_txs(unproposed_txs)?;

        Ok(block)
    }

    /// Generate a block proposal for provided fork, containing all
    /// pending transactions. Proposal is signed using provided secret key,
    /// which must also have signed the provided proposal transaction.
    pub async fn generate_signed_proposal(
        &self,
        fork: &Fork,
        producer_tx: Transaction,
        secret_key: &SecretKey,
    ) -> Result<Proposal> {
        let mut block = self.generate_unsigned_block(fork, producer_tx).await?;

        // Sign block
        block.sign(secret_key)?;

        // Generate the block proposal from the block
        let proposal = Proposal::new(block)?;

        Ok(proposal)
    }

    /// Generate a new empty fork.
    pub async fn generate_empty_fork(&self) -> Result<()> {
        debug!(target: "validator::consensus::generate_empty_fork", "Generating new empty fork...");
        let mut lock = self.forks.write().await;
        let fork = Fork::new(&self.blockchain, self.module.read().await.clone()).await?;
        lock.push(fork);
        drop(lock);
        debug!(target: "validator::consensus::generate_empty_fork", "Fork generated!");
        Ok(())
    }

    /// Given a proposal, the node verifys it and finds which fork it extends.
    /// If the proposal extends the canonical blockchain, a new fork chain is created.
    pub async fn append_proposal(&self, proposal: &Proposal) -> Result<()> {
        debug!(target: "validator::consensus::append_proposal", "Appending proposal {}", proposal.hash);

        // Verify proposal and grab corresponding fork
        let (mut fork, index) = verify_proposal(self, proposal).await?;

        // Append proposal to the fork
        fork.append_proposal(proposal.hash).await?;

        // Update PoW module
        fork.module.append(proposal.block.header.timestamp.0, &fork.module.next_difficulty()?);

        // If a fork index was found, replace forks with the mutated one,
        // otherwise push the new fork.
        let mut lock = self.forks.write().await;
        match index {
            Some(i) => {
                lock[i] = fork;
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
            let fork = Fork::new(&self.blockchain, self.module.read().await.clone()).await?;
            return Ok((fork, None))
        }

        let (f_index, p_index) = found.unwrap();
        let original_fork = &forks[f_index];
        // Check if proposal extends fork at last proposal
        if p_index == (original_fork.proposals.len() - 1) {
            return Ok((original_fork.full_clone()?, Some(f_index)))
        }

        // Rebuild fork
        let mut fork = Fork::new(&self.blockchain, self.module.read().await.clone()).await?;
        fork.proposals = original_fork.proposals[..p_index + 1].to_vec();

        // Retrieve proposals blocks from original fork
        let blocks = &original_fork.overlay.lock().unwrap().get_blocks_by_hash(&fork.proposals)?;

        // Retrieve last block
        let mut previous = &fork.overlay.lock().unwrap().last_block()?;

        // Validate and insert each block
        for block in blocks {
            // Verify block
            if verify_block(&fork.overlay, &fork.module, block, previous).await.is_err() {
                error!(target: "validator::consensus::find_extended_fork_overlay", "Erroneous block found in set");
                fork.overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
                return Err(Error::BlockIsInvalid(block.hash()?.to_string()))
            };

            // Update PoW module
            fork.module.append(block.header.timestamp.0, &fork.module.next_difficulty()?);

            // Use last inserted block as next iteration previous
            previous = block;
        }

        // Drop forks lock
        drop(forks);

        Ok((fork, None))
    }

    /// Consensus finalization logic:
    /// - If the current best fork has reached greater length than the security threshold, and
    ///   no other fork exist with same rank, all proposals excluding the last one in that fork
    //    can be finalized (append to canonical blockchain).
    /// When best fork can be finalized, blocks(proposals) should be appended to canonical, excluding the
    /// last one, and fork should be rebuilt.
    pub async fn finalization(&self) -> Result<Vec<BlockInfo>> {
        debug!(target: "validator::consensus::finalization", "Started finalization check");

        // Grab best forks
        let forks = self.forks.read().await;
        let forks_indexes = best_forks_indexes(&forks)?;
        // Check if multiple forks with same rank were found
        if forks_indexes.len() > 1 {
            debug!(target: "validator::consensus::finalization", "Multiple best ranked forks were found");
            return Ok(vec![])
        }

        // Grag the actual best fork
        let fork = &forks[forks_indexes[0]];

        // Check its length
        let length = fork.proposals.len();
        if length < self.finalization_threshold {
            debug!(target: "validator::consensus::finalization", "Nothing to finalize yet, best fork size: {}", length);
            return Ok(vec![])
        }

        // Grab finalized blocks
        let finalized = fork.overlay.lock().unwrap().get_blocks_by_hash(&fork.proposals)?;

        // Drop forks lock
        drop(forks);

        Ok(finalized)
    }
}

/// This struct represents a block proposal, used for consensus.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Proposal {
    /// Block hash
    pub hash: blake3::Hash,
    /// Block data
    pub block: BlockInfo,
}

impl Proposal {
    pub fn new(block: BlockInfo) -> Result<Self> {
        let hash = block.hash()?;
        Ok(Self { hash, block })
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
    /// Current PoW module state,
    pub module: PoWModule,
    /// Fork proposal hashes sequence
    pub proposals: Vec<blake3::Hash>,
    /// Valid pending transaction hashes
    pub mempool: Vec<blake3::Hash>,
    /// Current fork rank, cached for better performance
    pub rank: BigUint,
}

impl Fork {
    pub async fn new(blockchain: &Blockchain, module: PoWModule) -> Result<Self> {
        let mempool =
            blockchain.get_pending_txs()?.iter().map(|tx| blake3::hash(&serialize(tx))).collect();
        let overlay = BlockchainOverlay::new(blockchain)?;
        Ok(Self { overlay, module, proposals: vec![], mempool, rank: BigUint::from(0u64) })
    }

    /// Auxiliary function to append a proposal and recalculate current fork rank
    pub async fn append_proposal(&mut self, proposal: blake3::Hash) -> Result<()> {
        self.proposals.push(proposal);
        self.rank = self.rank().await?;

        Ok(())
    }

    /// Auxiliary function to retrieve last proposal
    pub fn last_proposal(&self) -> Result<Proposal> {
        let block = if self.proposals.is_empty() {
            self.overlay.lock().unwrap().last_block()?
        } else {
            self.overlay.lock().unwrap().get_blocks_by_hash(&[*self.proposals.last().unwrap()])?[0]
                .clone()
        };

        Proposal::new(block)
    }

    /// Auxiliary function to compute forks' next block height.
    pub fn get_next_block_height(&self) -> Result<u64> {
        let proposal = self.last_proposal()?;
        Ok(proposal.block.header.height + 1)
    }

    /// Auxiliary function to retrieve unproposed valid transactions.
    pub async fn unproposed_txs(
        &self,
        blockchain: &Blockchain,
        verifying_block_height: u64,
    ) -> Result<Vec<Transaction>> {
        // Check if our mempool is not empty
        if self.mempool.is_empty() {
            return Ok(vec![])
        }

        // Grab all current proposals transactions hashes
        let proposals_txs = self.overlay.lock().unwrap().get_blocks_txs_hashes(&self.proposals)?;

        // Iterate through all pending transactions in the forks' mempool
        let mut unproposed_txs = vec![];
        for tx in &self.mempool {
            // If the hash is contained in the proposals transactions vec, skip it
            if proposals_txs.contains(tx) {
                continue
            }

            // Push the tx hash into the unproposed transactions vector
            unproposed_txs.push(*tx);

            // Check limit
            if unproposed_txs.len() == TXS_CAP {
                break
            }
        }

        // Check if we have any unproposed transactions
        if unproposed_txs.is_empty() {
            return Ok(vec![])
        }

        // Retrieve the actual unproposed transactions
        let mut unproposed_txs: Vec<Transaction> = blockchain
            .pending_txs
            .get(&unproposed_txs, true)?
            .iter()
            .map(|x| x.clone().unwrap())
            .collect();

        // Clone forks' overlay
        let overlay = self.overlay.lock().unwrap().full_clone()?;

        // Verify transactions
        if let Err(e) = verify_transactions(
            &overlay,
            verifying_block_height,
            &unproposed_txs,
            &mut MerkleTree::new(1),
            false,
        )
        .await
        {
            match e {
                Error::TxVerifyFailed(TxVerifyFailed::ErroneousTxs(erroneous_txs)) => {
                    unproposed_txs.retain(|x| !erroneous_txs.contains(x))
                }
                _ => return Err(e),
            }
        }

        Ok(unproposed_txs)
    }

    /// Auxiliarry function to compute fork's rank, assuming all proposals are valid.
    pub async fn rank(&self) -> Result<BigUint> {
        // If the fork is empty its rank is 0
        if self.proposals.is_empty() {
            return Ok(0u64.into())
        }

        // Retrieve the sum of all fork proposals ranks
        let mut sum = BigUint::from(0_u64);
        let proposals = self.overlay.lock().unwrap().get_blocks_by_hash(&self.proposals)?;
        for proposal in &proposals {
            // For block height < 3 we use the same proposal reference, since
            // block_rank() will ignore it
            if proposal.header.height < 3 {
                sum += block_rank(proposal, proposal).await?;
                continue
            }

            // For block height > 2, retrieve their previous previous block
            let previous =
                &self.overlay.lock().unwrap().get_blocks_by_hash(&[proposal.header.previous])?[0];
            let previous_previous =
                &self.overlay.lock().unwrap().get_blocks_by_hash(&[previous.header.previous])?[0];
            sum += block_rank(proposal, previous_previous).await?;
        }

        // Use fork(proposals) length as a multiplier to compute the actual fork rank
        let rank = proposals.len() as u64 * sum;

        Ok(rank)
    }

    /// Auxiliary function to create a full clone using BlockchainOverlay::full_clone.
    /// Changes to this copy don't affect original fork overlay records, since underlying
    /// overlay pointer have been updated to the cloned one.
    pub fn full_clone(&self) -> Result<Self> {
        let overlay = self.overlay.lock().unwrap().full_clone()?;
        let module = self.module.clone();
        let proposals = self.proposals.clone();
        let mempool = self.mempool.clone();
        let rank = self.rank.clone();

        Ok(Self { overlay, module, proposals, mempool, rank })
    }
}
