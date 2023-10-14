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

use darkfi_sdk::{
    blockchain::{expected_reward, PidOutput, PreviousSlot, Slot, POS_START},
    crypto::{schnorr::SchnorrSecret, SecretKey},
    pasta::{group::ff::PrimeField, pallas},
};
use darkfi_serial::{async_trait, serialize, SerialDecodable, SerialEncodable};
use log::{debug, error, info};
use rand::rngs::OsRng;

use crate::{
    blockchain::{BlockInfo, Blockchain, BlockchainOverlay, BlockchainOverlayPtr, Header},
    tx::Transaction,
    util::time::{TimeKeeper, Timestamp},
    validator::{
        pid::slot_pid_output, pow::PoWModule, utils::block_rank, verify_block, verify_proposal,
        verify_transactions,
    },
    Error, Result,
};

// Consensus configuration
/// Block/proposal maximum transactions
pub const TXS_CAP: usize = 50;
/// Fork size(length) after which it can be finalized
const FINALIZATION_SECURITY_THRESSHOLD: usize = 3;

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
    /// Canonical blockchain PoW module state
    pub module: PoWModule,
    /// Flag to enable testing mode
    pub testing_mode: bool,
}

impl Consensus {
    /// Generate a new Consensus state.
    pub fn new(
        blockchain: Blockchain,
        time_keeper: TimeKeeper,
        pow_target: Option<usize>,
        testing_mode: bool,
    ) -> Result<Self> {
        let module = PoWModule::new(blockchain.clone(), None, pow_target)?;
        Ok(Self {
            blockchain,
            time_keeper,
            participating: false,
            checked_finalization: 0,
            forks: vec![],
            module,
            testing_mode,
        })
    }

    /// Generate next hot/live PoW slot for all current forks.
    pub fn generate_pow_slot(&mut self) -> Result<()> {
        // If no forks exist, create a new one as a basis to extend
        if self.forks.is_empty() {
            self.forks.push(Fork::new(&self.blockchain, self.module.clone())?);
        }

        for fork in self.forks.iter_mut() {
            fork.generate_pow_slot()?;
        }

        Ok(())
    }

    /// Generate current hot/live PoS slot for all current forks.
    pub fn generate_pos_slot(&mut self) -> Result<()> {
        // Grab current slot id
        let id = self.time_keeper.current_slot();

        // If no forks exist, create a new one as a basis to extend
        if self.forks.is_empty() {
            self.forks.push(Fork::new(&self.blockchain, self.module.clone())?);
        }

        // Grab previous slot information
        let (producers, last_hashes, second_to_last_hashes) = self.previous_slot_info(id - 1)?;

        for fork in self.forks.iter_mut() {
            fork.generate_pos_slot(id, producers, &last_hashes, &second_to_last_hashes)?;
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
            if last_proposal.block.header.height == slot {
                producers += 1;
            }
            last_hashes.push(last_proposal.hash);
            second_to_last_hashes.push(last_proposal.block.header.previous);
        }

        Ok((producers, last_hashes, second_to_last_hashes))
    }

    /// Generate a block proposal for the next/current hot/live(last) slot,
    /// containing all pending transactions. Proposal extends the best fork
    /// chain the node is holding. This should only be called after
    /// generating next/current slot. Proposal is signed using provided secret
    /// key, which must also have signed the provided proposal transaction.
    /// Best fork index is also returned in case its required.
    pub async fn generate_proposal(
        &self,
        secret_key: &SecretKey,
        proposal_tx: Transaction,
    ) -> Result<(Proposal, usize)> {
        // Grab best forks, pick the first and its last slot
        let fork_index = self.best_forks_indexes()?[0];
        let fork = &self.forks[fork_index];
        let slot = fork.slots.last().unwrap();

        // Generate a time keeper for next/current slot
        let time_keeper = if slot.id < POS_START {
            let mut t = self.time_keeper.current();
            t.verifying_slot = slot.id;
            t
        } else {
            self.time_keeper.current()
        };

        // Grab forks' unproposed transactions
        let mut unproposed_txs = fork.unproposed_txs(&self.blockchain, &time_keeper).await?;
        unproposed_txs.push(proposal_tx);

        // Grab forks' last block proposal(previous)
        let previous = fork.last_proposal()?;

        // Generate the new header
        // TODO: verify if header timestamp should be blockchain or system timestamp
        let header = Header::new(
            previous.block.hash()?,
            time_keeper.slot_epoch(slot.id),
            slot.id,
            Timestamp::current_time(),
            slot.last_nonce,
        );

        // Generate the block
        let mut block = BlockInfo::new_empty(header, fork.slots.clone());

        // Add transactions to the block
        block.append_txs(unproposed_txs)?;

        // TODO: sign more stuff?
        // Sign block header using provided secret key
        block.signature = secret_key.sign(&mut OsRng, &block.header.hash()?.as_bytes()[..]);

        // Generate the block proposal from the block
        let proposal = Proposal::new(block)?;

        Ok((proposal, fork_index))
    }

    /// Given a proposal, the node verifys it and finds which fork it extends.
    /// If the proposal extends the canonical blockchain, a new fork chain is created.
    pub async fn append_proposal(&mut self, proposal: &Proposal) -> Result<()> {
        info!(target: "validator::consensus::append_proposal", "Appending proposal {}", proposal.hash);

        // Verify proposal and grab corresponding fork
        let (mut fork, index) = verify_proposal(self, proposal).await?;

        // Append proposal to the fork
        fork.append_proposal(proposal.hash, self.testing_mode)?;

        // Update fork slots based on proposal version
        match proposal.block.header.version {
            // PoW proposal
            1 => {
                // Update PoW module
                fork.module
                    .append(proposal.block.header.timestamp.0, &fork.module.next_difficulty()?);
                // and generate next PoW slot for this specific fork
                fork.generate_pow_slot()?;
            }
            // PoS proposal
            2 => fork.slots = vec![],
            _ => return Err(Error::BlockVersionIsInvalid(proposal.block.header.version)),
        }

        // If a fork index was found, replace forks with the mutated one,
        // otherwise push the new fork.
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

    /// Auxiliary function to find current best ranked forks indexes.
    pub fn best_forks_indexes(&self) -> Result<Vec<usize>> {
        // Check if node has any forks
        if self.forks.is_empty() {
            return Err(Error::ForksNotFound)
        }

        // Find the best ranked forks
        let mut best = 0;
        let mut indexes = vec![];
        for (f_index, fork) in self.forks.iter().enumerate() {
            let rank = fork.rank;

            // Fork ranks lower that current best
            if rank < best {
                continue
            }

            // Fork has same rank as current best
            if rank == best {
                indexes.push(f_index);
                continue
            }

            // Fork ranks higher that current best
            best = rank;
            indexes = vec![f_index];
        }

        Ok(indexes)
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
    pub async fn find_extended_fork(&self, proposal: &Proposal) -> Result<(Fork, Option<usize>)> {
        // Check if proposal extends any fork
        let found = self.find_extended_fork_index(proposal);
        if found.is_err() {
            // Check if proposal extends canonical
            let (last_slot, last_block) = self.blockchain.last()?;
            if proposal.block.header.previous != last_block ||
                proposal.block.header.height <= last_slot
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
            let mut fork = Fork::new(&self.blockchain, self.module.clone())?;
            if proposal.block.header.height < POS_START {
                fork.generate_pow_slot()?;
            } else {
                let id = self.time_keeper.current_slot();
                let (producers, last_hashes, second_to_last_hashes) =
                    self.previous_slot_info(id - 1)?;
                fork.generate_pos_slot(id, producers, &last_hashes, &second_to_last_hashes)?;
            }

            return Ok((fork, None))
        }

        let (f_index, p_index) = found.unwrap();
        let original_fork = &self.forks[f_index];
        // Check if proposal extends fork at last proposal
        if p_index == (original_fork.proposals.len() - 1) {
            return Ok((original_fork.full_clone()?, Some(f_index)))
        }

        // Rebuild fork
        let mut fork = Fork::new(&self.blockchain, self.module.clone())?;
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
            time_keeper.verifying_slot = block.header.height;

            // Retrieve expected reward
            let expected_reward = expected_reward(time_keeper.verifying_slot);

            // Verify block
            if verify_block(
                &fork.overlay,
                &time_keeper,
                &fork.module,
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
                return Err(Error::BlockIsInvalid(block.hash()?.to_string()))
            };

            // Update PoW module
            if block.header.version == 1 {
                fork.module.append(block.header.timestamp.0, &fork.module.next_difficulty()?);
            }

            // Use last inserted block as next iteration previous
            previous = block;
        }

        // Rebuilt fork hot/live slots
        if proposal.block.header.height < POS_START {
            fork.generate_pow_slot()?;
        } else {
            let id = time_keeper.verifying_slot;
            let (producers, last_hashes, second_to_last_hashes) =
                self.previous_slot_info(id - 1)?;
            fork.generate_pos_slot(id, producers, &last_hashes, &second_to_last_hashes)?;
        }

        Ok((fork, None))
    }

    /// Consensus finalization logic:
    /// - If the current best fork has reached greater length than the security thresshold, and
    ///   no other fork exist with same rank, all proposals excluding the last one in that fork
    //    can be finalized (append to canonical blockchain).
    /// When best fork can be finalized, blocks(proposals) should be appended to canonical, excluding the
    /// last one, and fork should be rebuilt.
    pub async fn finalization(&mut self) -> Result<Vec<BlockInfo>> {
        // Set last slot finalization check occured to current slot
        let slot = self.time_keeper.current_slot();
        debug!(target: "validator::consensus::finalization", "Started finalization check for slot: {}", slot);
        self.checked_finalization = slot;

        // Grab best forks
        let forks_indexes = self.best_forks_indexes()?;
        // Check if multiple forks with same rank were found
        if forks_indexes.len() > 1 {
            debug!(target: "validator::consensus::finalization", "Multiple best ranked forks were found");
            return Ok(vec![])
        }

        // Grag the actual best fork
        let fork = &self.forks[forks_indexes[0]];

        // Check its length
        let length = fork.proposals.len();
        if length < FINALIZATION_SECURITY_THRESSHOLD {
            debug!(target: "validator::consensus::finalization", "Nothing to finalize yet, best fork size: {}", length);
            return Ok(vec![])
        }

        // Grab finalized blocks
        let finalized = fork.overlay.lock().unwrap().get_blocks_by_hash(&fork.proposals)?;

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
    /// Hot/live slots
    pub slots: Vec<Slot>,
    /// Valid pending transaction hashes
    pub mempool: Vec<blake3::Hash>,
    /// Current fork rank, cached for better performance
    pub rank: u64,
}

impl Fork {
    pub fn new(blockchain: &Blockchain, module: PoWModule) -> Result<Self> {
        let mempool =
            blockchain.get_pending_txs()?.iter().map(|tx| blake3::hash(&serialize(tx))).collect();
        let overlay = BlockchainOverlay::new(blockchain)?;
        Ok(Self { overlay, module, proposals: vec![], slots: vec![], mempool, rank: 0 })
    }

    /// Auxiliary function to append a proposal and recalculate current fork rank
    pub fn append_proposal(&mut self, proposal: blake3::Hash, testing_mode: bool) -> Result<()> {
        self.proposals.push(proposal);
        self.rank = self.rank(testing_mode)?;

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

    /// Utility function to extract leader selection lottery randomness(nonce/eta),
    /// defined as the hash of the last block, converted to pallas base.
    fn get_last_nonce(&self) -> Result<pallas::Base> {
        // Retrieve last block(or proposal)
        let proposal = self.last_proposal()?;

        match proposal.block.header.version {
            1 => Ok(pallas::Base::from(proposal.block.header.nonce)),
            2 => {
                // Read first 240 bits of proposal hash
                let mut bytes: [u8; 32] = *proposal.hash.as_bytes();
                bytes[30] = 0;
                bytes[31] = 0;

                Ok(pallas::Base::from_repr(bytes).unwrap())
            }
            _ => Err(Error::BlockVersionIsInvalid(proposal.block.header.version)),
        }
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

    /// Generate next hot/live PoW slot
    pub fn generate_pow_slot(&mut self) -> Result<()> {
        // Grab last proposal
        let last = self.last_proposal()?;

        // Generate the slot
        let last_slot = last.block.slots.last().unwrap().clone();
        let id = last_slot.id + 1;
        let producers = 1;
        let previous = PreviousSlot::new(
            producers,
            vec![last.hash],
            vec![last.block.header.previous],
            last_slot.pid.error,
        );
        let pid = PidOutput::default();
        let total_tokens = last_slot.total_tokens + last_slot.reward;
        let reward = expected_reward(id);
        let slot = Slot::new(
            id,
            previous,
            pid,
            pallas::Base::from(last.block.header.nonce),
            total_tokens,
            reward,
        );

        // Update fork hot/live slots vector
        self.slots = vec![slot];

        Ok(())
    }

    /// Generate current hot/live PoS slot
    pub fn generate_pos_slot(
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
        // last nonce(eta)
        let last_nonce = self.get_last_nonce()?;
        let total_tokens = previous_slot.total_tokens + previous_slot.reward;
        let reward = 0;
        let slot = Slot::new(id, previous, pid, last_nonce, total_tokens, reward);
        self.slots.push(slot);

        Ok(())
    }

    /// Auxiliarry function to compute fork's rank, assuming all proposals are valid.
    pub fn rank(&self, testing_mode: bool) -> Result<u64> {
        // If the fork is empty its rank is 0
        if self.proposals.is_empty() {
            return Ok(0)
        }

        // Retrieve the sum of all fork proposals ranks
        let mut sum = 0;
        let proposals = self.overlay.lock().unwrap().get_blocks_by_hash(&self.proposals)?;
        for proposal in &proposals {
            // For block height > 3, retrieve their previous previous block
            let previous_previous = if proposal.header.height > 3 {
                let previous = &self
                    .overlay
                    .lock()
                    .unwrap()
                    .get_blocks_by_hash(&[proposal.header.previous])?[0];
                self.overlay.lock().unwrap().get_blocks_by_hash(&[previous.header.previous])?[0]
                    .clone()
            } else {
                proposal.clone()
            };
            sum += block_rank(proposal, &previous_previous, testing_mode)?;
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
        let slots = self.slots.clone();
        let mempool = self.mempool.clone();
        let rank = self.rank;

        Ok(Self { overlay, module, proposals, slots, mempool, rank })
    }
}
