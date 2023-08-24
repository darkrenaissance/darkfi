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

use async_trait::async_trait;
use darkfi_sdk::{
    blockchain::{PidOutput, PreviousSlot, Slot},
    crypto::{schnorr::SchnorrSecret, MerkleNode, MerkleTree, SecretKey},
    pasta::{group::ff::PrimeField, pallas},
};
use darkfi_serial::{serialize, SerialDecodable, SerialEncodable};
use log::{error, info, warn};
use rand::rngs::OsRng;
use smol::io::{AsyncRead, AsyncWrite};

use crate::{
    blockchain::{
        BlockInfo, BlockProducer, Blockchain, BlockchainOverlay, BlockchainOverlayPtr, Header,
    },
    tx::Transaction,
    util::time::{TimeKeeper, Timestamp},
    validator::{consensus::pid::slot_pid_output, verify_block, verify_transactions},
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

    /// Generate current hot/live slot for all current forks.
    pub fn generate_slot(&mut self) -> Result<()> {
        // Grab current slot id
        let id = self.time_keeper.current_slot();

        // If no forks exist, create a new one as a basis to extend
        if self.forks.is_empty() {
            self.forks.push(Fork::new(&self.blockchain)?);
        }

        // Grab previous slot information
        let (producers, last_hashes, second_to_last_hashes) = self.previous_slot_info(id - 1)?;

        for fork in self.forks.iter_mut() {
            fork.generate_slot(id, producers, &last_hashes, &second_to_last_hashes)?;
        }

        Ok(())
    }

    /// Retrieve previous slot producers, last proposal hashes,
    /// and their second to last hashes, from all current forks.
    fn previous_slot_info(&self, slot: u64) -> Result<(u64, Vec<blake3::Hash>, Vec<blake3::Hash>)> {
        let mut producers = 0;
        let mut last_hashes = vec![];
        let mut second_to_last_hashes = vec![];

        for fork in &self.forks {
            let last_proposal = fork.last_proposal()?;
            if last_proposal.block.header.slot == slot {
                producers += 1;
            }
            last_hashes.push(last_proposal.hash);
            second_to_last_hashes.push(last_proposal.block.header.previous);
        }

        Ok((producers, last_hashes, second_to_last_hashes))
    }

    /// Generate a block proposal for the current hot/live(last) slot,
    /// containing all pending transactions. Proposal extends the longest fork
    /// chain the node is holding. This should only be called after
    /// generate_slot(). Proposal is signed using provided secret key, which
    /// must also have signed the provided proposal transaction.
    pub async fn generate_proposal(
        &self,
        secret_key: SecretKey,
        proposal_tx: Transaction,
    ) -> Result<Proposal> {
        // Generate a time keeper for current slot
        let time_keeper = self.time_keeper.current();

        // Retrieve longest known fork
        let mut fork_index = 0;
        let mut max_fork_length = 0;
        for (index, fork) in self.forks.iter().enumerate() {
            if fork.proposals.len() > max_fork_length {
                fork_index = index;
                max_fork_length = fork.proposals.len();
            }
        }
        let fork = &self.forks[fork_index];

        // Grab forks' unproposed transactions and their root
        let unproposed_txs = fork.unproposed_txs(&self.blockchain, &time_keeper).await?;
        let mut tree = MerkleTree::new(100);
        // The following is pretty weird, so something better should be done.
        for tx in &unproposed_txs {
            let mut hash = [0_u8; 32];
            hash[0..31].copy_from_slice(&blake3::hash(&serialize(tx)).as_bytes()[0..31]);
            tree.append(MerkleNode::from(pallas::Base::from_repr(hash).unwrap()));
        }
        let root = tree.root(0).unwrap();

        // Grab forks' last block proposal(previous)
        let previous = fork.last_proposal()?;

        // Generate the new header
        let slot = fork.slots.last().unwrap();
        // TODO: verify if header timestamp should be blockchain or system timestamp
        let header = Header::new(
            previous.block.blockhash(),
            time_keeper.slot_epoch(slot.id),
            slot.id,
            Timestamp::current_time(),
            root,
        );

        // TODO: sign more stuff?
        // Sign block header using provided secret key
        let signature = secret_key.sign(&mut OsRng, &header.headerhash().as_bytes()[..]);

        // Generate block producer info
        let block_producer = BlockProducer::new(signature, proposal_tx, slot.last_eta);

        // Generate the block and its proposal
        let block = BlockInfo::new(header, unproposed_txs, block_producer, fork.slots.clone());
        let proposal = Proposal::new(block);

        Ok(proposal)
    }

    /// Given a proposal, the node verifys it and finds which fork it extends.
    /// If the proposal extends the canonical blockchain, a new fork chain is created.
    /// A proposal is considered valid when the following rules apply:
    ///     1. Node has not started current slot finalization
    ///     2. Proposal refers to current slot
    ///     3. Proposal hash matches the actual block one
    ///     4. Block transactions don't exceed set limit
    ///     5. If proposal extends a known fork, verify block slots
    ///        correspond to the fork hot/live ones
    ///     6. Block is valid
    /// Additional validity rules can be applied.
    pub async fn append_proposal(&mut self, proposal: &Proposal) -> Result<()> {
        // Generate a time keeper for current slot
        let time_keeper = self.time_keeper.current();

        // Node have already checked for finalization in this slot (1)
        if time_keeper.verifying_slot <= self.checked_finalization {
            warn!(target: "validator::consensus::append_proposal", "Proposal received after finalization sync period.");
            return Err(Error::ProposalAfterFinalizationError)
        }

        // Proposal validations
        let hdr = &proposal.block.header;

        // Ignore proposal if not for current slot (2)
        if hdr.slot != time_keeper.verifying_slot {
            return Err(Error::ProposalNotForCurrentSlotError)
        }

        // Check if proposal hash matches actual one (3)
        let proposal_hash = proposal.block.blockhash();
        if proposal.hash != proposal_hash {
            warn!(
                target: "validator::consensus::append_proposal", "Received proposal contains mismatched hashes: {} - {}",
                proposal.hash, proposal_hash
            );
            return Err(Error::ProposalHashesMissmatchError)
        }

        // TODO: verify if this should happen here or not.
        // Check that proposal transactions don't exceed limit (4)
        if proposal.block.txs.len() > TXS_CAP {
            warn!(
                target: "validator::consensus::append_proposal", "Received proposal transactions exceed configured cap: {} - {}",
                proposal.block.txs.len(),
                TXS_CAP
            );
            return Err(Error::ProposalTxsExceedCapError)
        }

        // Check if proposal extends any existing forks
        let (mut fork, index) = self.find_extended_fork(proposal).await?;

        // Verify block slots correspond to the forks' hot/live ones (5)
        if !fork.slots.is_empty() && fork.slots != proposal.block.slots {
            return Err(Error::ProposalContainsUnknownSlots)
        }

        // Insert last block slot so transactions can be validated against.
        // Rest (empty) slots will be inserted along with the block.
        // Since this fork uses an overlay clone, original overlay is not affected.
        fork.overlay.lock().unwrap().slots.insert(&[proposal
            .block
            .slots
            .last()
            .unwrap()
            .clone()])?;

        // Grab overlay last block
        let previous = fork.overlay.lock().unwrap().last_block()?;

        // Retrieve expected reward
        let expected_reward = next_block_reward();

        // Verify proposal block (6)
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
        fork.slots = vec![];
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
    fn find_extended_fork_index(&self, proposal: &Proposal) -> Result<(usize, usize)> {
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
    async fn find_extended_fork(&self, proposal: &Proposal) -> Result<(Fork, Option<usize>)> {
        // Check if proposal extends any fork
        let found = self.find_extended_fork_index(proposal);
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

    /// Node checks if any of the forks can be finalized.
    /// Consensus finalization logic:
    /// - If the node has observed the creation of a fork and no other forks exists at same or greater height,
    ///   all proposals in that fork can be finalized (append to canonical blockchain).
    /// When a fork can be finalized, blocks(proposals) should be appended to canonical,
    /// and forks should be removed.
    pub async fn forks_finalization(&mut self) -> Result<Vec<BlockInfo>> {
        let slot = self.time_keeper.current_slot();
        info!(target: "validator::consensus::forks_finalization", "Started finalization check for slot: {}", slot);
        // Set last slot finalization check occured to current slot
        self.checked_finalization = slot;

        // First we find longest fork without any other forks at same height
        let mut fork_index = -1;
        let mut max_length = 0;
        for (index, fork) in self.forks.iter().enumerate() {
            let length = fork.proposals.len();
            // Check if less than max
            if length < max_length {
                continue
            }
            // Check if same length as max
            if length == max_length {
                // Setting fork_index so we know we have multiple
                // forks at same length.
                fork_index = -2;
                continue
            }
            // Set fork as max
            fork_index = index as i64;
            max_length = length;
        }

        // Check if we found any fork to finalize
        match fork_index {
            -2 => {
                info!(target: "validator::consensus::forks_finalization", "Eligible forks with same height exist, nothing to finalize.");
                return Ok(vec![])
            }
            -1 => {
                info!(target: "validator::consensus::forks_finalization", "Nothing to finalize.");
            }
            _ => {
                info!(target: "validator::consensus::forks_finalization", "Fork {} can be finalized!", fork_index)
            }
        }

        if max_length == 0 {
            return Ok(vec![])
        }

        // Starting finalization
        let fork = &self.forks[fork_index as usize];
        let finalized = fork.overlay.lock().unwrap().get_blocks_by_hash(&fork.proposals)?;
        info!(target: "validator::consensus::forks_finalization", "Finalized blocks: {}", finalized.len());

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
    pub fn new(block: BlockInfo) -> Self {
        let hash = block.blockhash();
        Self { hash, block }
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
    /// Hot/live slots
    pub slots: Vec<Slot>,
    /// Valid pending transaction hashes
    pub mempool: Vec<blake3::Hash>,
}

impl Fork {
    pub fn new(blockchain: &Blockchain) -> Result<Self> {
        let mempool =
            blockchain.get_pending_txs()?.iter().map(|tx| blake3::hash(&serialize(tx))).collect();
        let overlay = BlockchainOverlay::new(blockchain)?;
        Ok(Self { overlay, proposals: vec![], slots: vec![], mempool })
    }

    /// Auxiliary function to retrieve last proposal
    pub fn last_proposal(&self) -> Result<Proposal> {
        let block = if self.proposals.is_empty() {
            self.overlay.lock().unwrap().last_block()?
        } else {
            self.overlay.lock().unwrap().get_blocks_by_hash(&[*self.proposals.last().unwrap()])?[0]
                .clone()
        };

        Ok(Proposal::new(block))
    }

    /// Utility function to extract leader selection lottery randomness(eta),
    /// defined as the hash of the last block, converted to pallas base.
    fn get_last_eta(&self) -> Result<pallas::Base> {
        // Retrieve last block(or proposal) hash
        let hash = if self.proposals.is_empty() {
            self.overlay.lock().unwrap().last_block()?.blockhash()
        } else {
            *self.proposals.last().unwrap()
        };

        // Read first 240 bits
        let mut bytes: [u8; 32] = *hash.as_bytes();
        bytes[30] = 0;
        bytes[31] = 0;

        Ok(pallas::Base::from_repr(bytes).unwrap())
    }

    /// Auxiliary function to retrieve unproposed valid transactions.
    pub async fn unproposed_txs(
        &self,
        blockchain: &Blockchain,
        time_keeper: &TimeKeeper,
    ) -> Result<Vec<Transaction>> {
        // Retrieve all mempool transactions
        let mut unproposed_txs: Vec<Transaction> = blockchain
            .pending_txs
            .get(&self.mempool, true)?
            .iter()
            .map(|x| x.clone().unwrap())
            .collect();

        // Iterate over fork proposals to find already proposed transactions
        // and remove them from the unproposed_txs vector.
        let proposals = self.overlay.lock().unwrap().get_blocks_by_hash(&self.proposals)?;
        for proposal in proposals {
            for tx in &proposal.txs {
                unproposed_txs.retain(|x| x != tx);
            }
        }

        // Check if transactions exceed configured cap
        if unproposed_txs.len() > TXS_CAP {
            unproposed_txs = unproposed_txs[0..TXS_CAP].to_vec()
        }

        // Clone forks' overlay
        let overlay = self.overlay.lock().unwrap().full_clone()?;

        // Verify transactions
        let erroneous_txs = verify_transactions(&overlay, time_keeper, &unproposed_txs).await?;
        if !erroneous_txs.is_empty() {
            unproposed_txs.retain(|x| !erroneous_txs.contains(x));
        }

        Ok(unproposed_txs)
    }

    /// Generate current hot/live slot
    pub fn generate_slot(
        &mut self,
        id: u64,
        producers: u64,
        last_hashes: &[blake3::Hash],
        second_to_last_hashes: &[blake3::Hash],
    ) -> Result<()> {
        // Grab last known fork slot
        let previous_slot = if self.slots.is_empty() {
            self.overlay.lock().unwrap().slots.get_last()?
        } else {
            self.slots.last().unwrap().clone()
        };

        // Generate previous slot information
        let previous = PreviousSlot::new(
            producers,
            last_hashes.to_vec(),
            second_to_last_hashes.to_vec(),
            previous_slot.pid.error,
        );

        // Generate PID controller output
        let (f, error, sigma1, sigma2) = slot_pid_output(&previous_slot, producers);
        let pid = PidOutput::new(f, error, sigma1, sigma2);

        // Each slot starts as an empty slot(not reward) when generated, carrying
        // last eta
        let last_eta = self.get_last_eta()?;
        let total_tokens = previous_slot.total_tokens + previous_slot.reward;
        let reward = 0;
        let slot = Slot::new(id, previous, pid, last_eta, total_tokens, reward);
        self.slots.push(slot);

        Ok(())
    }

    /// Auxiliary function to create a full clone using BlockchainOverlay::full_clone.
    /// Changes to this copy don't affect original fork overlay records, since underlying
    /// overlay pointer have been updated to the cloned one.
    pub fn full_clone(&self) -> Result<Self> {
        let overlay = self.overlay.lock().unwrap().full_clone()?;
        let proposals = self.proposals.clone();
        let slots = self.slots.clone();
        let mempool = self.mempool.clone();

        Ok(Self { overlay, proposals, slots, mempool })
    }
}

/// Block producer reward.
/// TODO (res) implement reward mechanism with accord to DRK, DARK token-economics.
pub fn next_block_reward() -> u64 {
    // Configured block reward (1 DRK == 1 * 10^8)
    let reward: u64 = 100_000_000;
    reward
}
