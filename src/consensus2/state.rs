// TODO: Use sets instead of vectors where possible.

use std::time::Duration;

use async_std::sync::{Arc, RwLock};
use chrono::{NaiveDateTime, Utc};
use fxhash::FxBuildHasher;
use indexmap::IndexMap;
use log::{debug, error, info, warn};
use rand::{rngs::OsRng, Rng};

use super::{
    Block, BlockProposal, Metadata, Participant, ProposalChain, StreamletMetadata, Timestamp, Tx,
    Vote,
};
use crate::{
    blockchain::Blockchain,
    crypto::{
        keypair::{PublicKey, SecretKey},
        schnorr::{SchnorrPublic, SchnorrSecret},
    },
    util::serial::{serialize, Encodable},
    Result,
};

type FxIndexMap<K, V> = IndexMap<K, V, FxBuildHasher>;

const DELTA: u64 = 10;

/// This struct represents the information required by the consensus algorithm
#[derive(Debug)]
pub struct ConsensusState {
    /// Genesis block creation timestamp
    pub genesis_ts: Timestamp,
    /// Genesis block hash
    pub genesis_block: blake3::Hash,
    /// Fork chains containing block proposals
    pub proposals: Vec<ProposalChain>,
    /// Orphan votes pool, in case a vote reaches a node before the
    /// corresponding block
    pub orphan_votes: Vec<Vote>,
    /// Validators currently participating in the consensus
    pub participants: FxIndexMap<u64, Participant>,
    /// Validators to be added on the next epoch as participants
    pub pending_participants: Vec<Participant>,
}

impl ConsensusState {
    pub fn new(genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Result<Self> {
        let genesis_block =
            blake3::hash(&serialize(&Block::genesis_block(genesis_ts, genesis_data)));

        Ok(Self {
            genesis_ts,
            genesis_block,
            proposals: vec![],
            orphan_votes: vec![],
            participants: FxIndexMap::with_hasher(FxBuildHasher::default()),
            pending_participants: vec![],
        })
    }
}

/// Atomic pointer to validator state.
pub type ValidatorStatePtr = Arc<RwLock<ValidatorState>>;

/// This struct represents the state of a validator node.
pub struct ValidatorState {
    /// Validator ID
    pub id: u64,
    /// Secret key, to sign messages
    pub secret: SecretKey,
    /// Validator public key
    pub public: PublicKey,
    /// Hot/Live data used by the consensus algorithm
    pub consensus: ConsensusState,
    /// Canonical (finalized) blockchain
    pub blockchain: Blockchain,
    /// Pending transactions
    pub unconfirmed_txs: Vec<Tx>,
}

impl ValidatorState {
    // TODO: Clock sync
    // TODO: ID shouldn't be done like this
    pub fn new(
        db: &sled::Db, // <-- TODO: Avoid this with some wrapping, sled should only be in blockchain
        id: u64,
        genesis_ts: Timestamp,
        genesis_data: blake3::Hash,
    ) -> Result<ValidatorStatePtr> {
        let secret = SecretKey::random(&mut OsRng);
        let public = PublicKey::from_secret(secret);
        let consensus = ConsensusState::new(genesis_ts, genesis_data)?;
        let blockchain = Blockchain::new(db, genesis_ts, genesis_data)?;
        let unconfirmed_txs = vec![];

        let state = Arc::new(RwLock::new(ValidatorState {
            id,
            secret,
            public,
            consensus,
            blockchain,
            unconfirmed_txs,
        }));

        Ok(state)
    }

    /// The node retrieves a transaction and appends it to the unconfirmed
    /// transactions list. Additional validity rules must be defined by the
    /// protocol for transactions.
    pub fn append_tx(&mut self, tx: Tx) -> bool {
        if self.unconfirmed_txs.contains(&tx) {
            debug!("append_tx(): We already have this tx");
            return false
        }

        debug!("append_tx(): Appended tx to mempool");
        self.unconfirmed_txs.push(tx);
        true
    }

    /// Calculates current epoch, based on elapsed time from the genesis block.
    /// Epoch duration is configured using the `DELTA` value.
    pub fn current_epoch(&self) -> u64 {
        self.consensus.genesis_ts.elapsed() / (2 * DELTA)
    }

    /// Calculates seconds until next epoch starting time.
    /// Epochs durationis configured using the delta value.
    pub fn next_epoch_start(&self) -> Duration {
        let start_time = NaiveDateTime::from_timestamp(self.consensus.genesis_ts.0, 0);
        let current_epoch = self.current_epoch() + 1;
        let next_epoch_start = (current_epoch * (2 * DELTA)) + (start_time.timestamp() as u64);
        let next_epoch_start = NaiveDateTime::from_timestamp(next_epoch_start as i64, 0);
        let current_time = NaiveDateTime::from_timestamp(Utc::now().timestamp(), 0);
        let diff = next_epoch_start - current_time;

        Duration::new(diff.num_seconds().try_into().unwrap(), 0)
    }

    /// Find epoch leader, using a simple hash method.
    /// Leader calculation is based on how many nodes are participating
    /// in the network.
    pub fn epoch_leader(&mut self) -> u64 {
        let len = self.consensus.participants.len();
        assert!(len > 0);
        let idx = rand::thread_rng().gen_range(0..len);
        self.consensus.participants.get_index(idx).unwrap().1.id
    }

    /// Check if we're the current epoch leader
    pub fn is_epoch_leader(&mut self) -> bool {
        self.id == self.epoch_leader()
    }

    /// Generate a block proposal for the current epoch, containing all
    /// unconfirmed transactions. Proposal extends the longest notarized fork
    /// chain the node is holding.
    pub fn propose(&self) -> Result<Option<BlockProposal>> {
        let epoch = self.current_epoch();
        let prev_hash = self.longest_notarized_chain_last_hash().unwrap();
        let unproposed_txs = self.unproposed_txs();

        let metadata = Metadata::new(
            Timestamp::current_time(),
            String::from("proof"),
            String::from("r"),
            String::from("s"),
        );

        let sm = StreamletMetadata::new(self.consensus.participants.values().cloned().collect());
        let prop = BlockProposal::to_proposal_hash(prev_hash, epoch, &unproposed_txs, &metadata);
        let signed_proposal = self.secret.sign(&prop.as_bytes()[..]);

        Ok(Some(BlockProposal::new(
            self.public,
            signed_proposal,
            self.id,
            prev_hash,
            epoch,
            unproposed_txs,
            metadata,
            sm,
        )))
    }

    /// Retrieve all unconfirmed transactions not proposed in previous blocks.
    pub fn unproposed_txs(&self) -> Vec<Tx> {
        let mut unproposed_txs = self.unconfirmed_txs.clone();
        for chain in &self.consensus.proposals {
            for proposal in &chain.proposals {
                for tx in &proposal.block.txs {
                    if let Some(pos) = unproposed_txs.iter().position(|txs| *txs == *tx) {
                        unproposed_txs.remove(pos);
                    }
                }
            }
        }

        unproposed_txs
    }

    /// Finds the longest fully notarized blockchain the node holds and
    /// returns the last block hash.
    pub fn longest_notarized_chain_last_hash(&self) -> Result<blake3::Hash> {
        let hash = if !self.consensus.proposals.is_empty() {
            let mut longest_notarized_chain = &self.consensus.proposals[0];
            let mut length = longest_notarized_chain.proposals.len();
            if self.consensus.proposals.len() > 1 {
                for chain in &self.consensus.proposals[1..] {
                    if chain.notarized() && chain.proposals.len() > length {
                        length = chain.proposals.len();
                        longest_notarized_chain = chain;
                    }
                }
            }

            longest_notarized_chain.proposals.last().unwrap().hash()
        } else {
            self.blockchain.last()?.unwrap().1
        };

        Ok(hash)
    }

    /// Receive the proposed block, verify its sender (epoch leader),
    /// and proceed with voting on it.
    pub fn receive_proposal(&mut self, proposal: &BlockProposal) -> Result<Option<Vote>> {
        let leader = self.epoch_leader();
        if leader != proposal.id {
            warn!(
                "Received proposal not from epoch leader ({}), but from ({})",
                leader, proposal.id
            );
            return Ok(None)
        }

        if !proposal.public_key.verify(
            BlockProposal::to_proposal_hash(
                proposal.block.st,
                proposal.block.sl,
                &proposal.block.txs,
                &proposal.block.metadata,
            )
            .as_bytes(),
            &proposal.signature,
        ) {
            warn!("Proposer ({}) signature could not be verified", proposal.id);
            return Ok(None)
        }

        self.vote(proposal)
    }

    /// Given a proposal, the node finds which blockchain it extends.
    /// If the proposal extends the canonical blockchain, a new fork chain
    /// is created. The node votes on the proposal only if it extends the
    /// longest notarized fork chain it has seen.
    pub fn vote(&mut self, proposal: &BlockProposal) -> Result<Option<Vote>> {
        self.zero_participants_check();
        let mut proposal = proposal.clone();

        // Generate proposal hash
        let proposal_hash = proposal.hash();

        // Add orphan votes
        let mut orphans = Vec::new();
        for vote in self.consensus.orphan_votes.iter() {
            if vote.proposal == proposal_hash {
                proposal.block.sm.votes.push(vote.clone());
                orphans.push(vote.clone());
            }
        }

        for vote in orphans {
            self.consensus.orphan_votes.retain(|v| *v != vote);
        }

        let index = self.find_extended_chain_index(&proposal)?;

        if index == -2 {
            return Ok(None)
        }

        let chain = match index {
            -1 => {
                let pc = ProposalChain::new(self.consensus.genesis_block, proposal.clone());
                self.consensus.proposals.push(pc);
                self.consensus.proposals.last().unwrap()
            }
            _ => {
                self.consensus.proposals[index as usize].add(&proposal);
                &self.consensus.proposals[index as usize]
            }
        };

        if !self.extends_notarized_chain(chain) {
            debug!("vote(): Proposal does not extend notarized chain");
            return Ok(None)
        }

        let signed_hash = self.secret.sign(&serialize(&proposal_hash));
        Ok(Some(Vote::new(self.public, signed_hash, proposal_hash, proposal.block.sl, self.id)))
    }

    /// Verify if the provided chain is notarized excluding the last block.
    pub fn extends_notarized_chain(&self, chain: &ProposalChain) -> bool {
        for proposal in &chain.proposals[..(chain.proposals.len() - 1)] {
            if !proposal.block.sm.notarized {
                return false
            }
        }

        true
    }

    /// Given a proposal, find the index of the chain it extends.
    pub fn find_extended_chain_index(&self, proposal: &BlockProposal) -> Result<i64> {
        for (index, chain) in self.consensus.proposals.iter().enumerate() {
            let last = chain.proposals.last().unwrap();
            let hash = last.hash();
            if proposal.block.st == hash && proposal.block.sl > last.block.sl {
                return Ok(index as i64)
            }

            if proposal.block.st == last.block.st && proposal.block.sl == last.block.sl {
                debug!("find_extended_chain_index(): Proposal already received");
                return Ok(-2)
            }
        }

        let (last_sl, last_block) = self.blockchain.last()?.unwrap();
        if proposal.block.st != last_block || proposal.block.sl <= last_sl {
            debug!("find_extended_chain_index(): Proposal doesn't extend any known chain");
            return Ok(-2)
        }

        Ok(-1)
    }

    /// Receive a vote for a proposal.
    /// First, sender is verified using their public key.
    /// The proposal is then searched for in the node's fork chains.
    /// If the vote wasn't received before, it is appended to the proposal
    /// votes list.
    /// When a node sees 2n/3 votes for a proposal, it notarizes it.
    /// When a proposal gets notarized, the transactions it contains are
    /// removed from the node's unconfirmed tx list.
    /// Finally, we check if the notarization of the proposal can finalize
    /// parent proposals in its chain.
    pub fn receive_vote(&mut self, vote: &Vote) -> Result<bool> {
        let mut encoded_proposal = vec![];

        match vote.proposal.encode(&mut encoded_proposal) {
            Ok(_) => (),
            Err(e) => {
                error!(target: "consensus", "Proposal encoding failed: {:?}", e);
                return Ok(false)
            }
        };

        if !vote.public_key.verify(&encoded_proposal, &vote.vote) {
            warn!(target: "consensus", "Voter ({}), signature couldn't be verified", vote.id);
            return Ok(false)
        }

        let node_count = self.consensus.participants.len();
        self.zero_participants_check();

        // Checking that the voter can actually vote.
        match self.consensus.participants.get(&vote.id) {
            Some(participant) => {
                if self.current_epoch() <= participant.joined {
                    warn!(target: "consensus", "Voter ({}) joined after current epoch.", vote.id);
                    return Ok(false)
                }
            }
            None => {
                warn!(target: "consensus", "Voter ({}) is not a participant!", vote.id);
                return Ok(false)
            }
        }

        let proposal = match self.find_proposal(&vote.proposal) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "consensus", "find_proposal() failed: {}", e);
                return Err(e)
            }
        };

        if proposal.is_none() {
            warn!(target: "consensus", "Received vote for unknown proposal.");
            if !self.consensus.orphan_votes.contains(vote) {
                self.consensus.orphan_votes.push(vote.clone());
            }

            return Ok(false)
        }

        let (proposal, chain_idx) = proposal.unwrap();
        if proposal.block.sm.votes.contains(vote) {
            debug!("receive_vote(): Already seen this proposal");
            return Ok(false)
        }

        proposal.block.sm.votes.push(vote.clone());

        if !proposal.block.sm.notarized && proposal.block.sm.votes.len() > (2 * node_count / 3) {
            debug!("receive_vote(): Notarized a block");
            proposal.block.sm.notarized = true;
            match self.chain_finalization(chain_idx) {
                Ok(()) => {}
                Err(e) => {
                    error!(target: "consensus", "Block finalization failed: {}", e);
                    return Err(e)
                }
            }
        }

        // Updating participant vote
        let mut participant = match self.consensus.participants.get(&vote.id) {
            Some(p) => p.clone(),
            None => Participant::new(vote.id, vote.sl),
        };

        match participant.voted {
            Some(voted) => {
                if vote.sl > voted {
                    participant.voted = Some(vote.sl);
                }
            }
            None => participant.voted = Some(vote.sl),
        }

        self.consensus.participants.insert(participant.id, participant);
        Ok(true)
    }

    /// Search the chains we're holding for the given proposal.
    pub fn find_proposal(
        &mut self,
        vote_proposal: &blake3::Hash,
    ) -> Result<Option<(&mut BlockProposal, i64)>> {
        for (index, chain) in &mut self.consensus.proposals.iter_mut().enumerate() {
            for proposal in chain.proposals.iter_mut().rev() {
                let proposal_hash = proposal.hash();
                if vote_proposal == &proposal_hash {
                    return Ok(Some((proposal, index as i64)))
                }
            }
        }

        Ok(None)
    }

    /// Remove provided transactions vector from unconfirmed_txs if they exist.
    pub fn remove_txs(&mut self, transactions: Vec<Tx>) -> Result<()> {
        for tx in transactions {
            if let Some(pos) = self.unconfirmed_txs.iter().position(|txs| *txs == tx) {
                self.unconfirmed_txs.remove(pos);
            }
        }

        Ok(())
    }

    /// Provided an index, the node checks if the chain can be finalized.
    /// Consensus finalization logic:
    /// - If the node has observed the notarization of 3 consecutive
    ///   proposals in a fork chain, it finalizes (appends to canonical
    ///   blockchain) all proposals up to the middle block.
    /// When fork chain proposals are finalized, the rest of fork chains not
    /// starting by those proposals are removed.
    pub fn chain_finalization(&mut self, chain_index: i64) -> Result<()> {
        let chain = &mut self.consensus.proposals[chain_index as usize];

        if chain.proposals.len() < 3 {
            debug!(
                "chain_finalization(): Less than 3 proposals in chain {}, nothing to finalize",
                chain_index
            );
            return Ok(())
        }

        let mut consecutive = 0;
        for proposal in &chain.proposals {
            if proposal.block.sm.notarized {
                consecutive += 1;
                continue
            }

            break
        }

        if consecutive < 3 {
            debug!(
                "chain_finalization(): Less than 3 notarized blocks in chain {}, nothing to finalize",
                chain_index
            );
            return Ok(())
        }

        let mut finalized = vec![];
        for proposal in &mut chain.proposals[..(consecutive - 1)] {
            proposal.block.sm.finalized = true;
            finalized.push(proposal.clone().into());
        }

        chain.proposals.drain(0..(consecutive - 1));

        info!(target: "consensus", "Adding finalized block to canonical chain");
        let blockhashes = match self.blockchain.add(&finalized) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "consensus", "Failed appending finalized blocks to canonical chain: {}", e);
                return Err(e)
            }
        };

        for proposal in &finalized {
            self.remove_txs(proposal.txs.clone())?;
        }

        let last_block = *blockhashes.last().unwrap();
        let last_sl = finalized.last().unwrap().sl;

        let mut dropped = vec![];
        for chain in self.consensus.proposals.iter() {
            let first = chain.proposals.first().unwrap();
            if first.block.st != last_block || first.block.sl <= last_sl {
                dropped.push(chain.clone());
            }
        }

        for chain in dropped {
            self.consensus.proposals.retain(|c| *c != chain);
        }

        // Remove orphan votes
        let mut orphans = vec![];
        for vote in self.consensus.orphan_votes.iter() {
            if vote.sl <= last_sl {
                orphans.push(vote.clone());
            }
        }

        for vote in orphans {
            self.consensus.orphan_votes.retain(|v| *v != vote);
        }

        Ok(())
    }

    /// Append a new participant to the pending participants list.
    pub fn append_participant(&mut self, participant: Participant) -> bool {
        if self.consensus.pending_participants.contains(&participant) {
            return false
        }

        self.consensus.pending_participants.push(participant);
        true
    }

    /// Prevent the extreme case scenario where network is initialized, but
    /// some nodes have not pushed the initial participants in the map.
    pub fn zero_participants_check(&mut self) {
        if self.consensus.participants.is_empty() {
            debug!("zero_participants_check(): Participants are empty, trying to add pending ones");
            for participant in &self.consensus.pending_participants {
                self.consensus.participants.insert(participant.id, participant.clone());
            }

            if self.consensus.participants.is_empty() {
                debug!("zero_participants_check(): Didn't manage to add any participant, pending were empty");
            }

            self.consensus.pending_participants = Vec::new();
        }
    }

    /// Refresh the participants map, to retain only the active ones.
    /// Active nodes are considered those who joined or voted on a previous epoch.
    pub fn refresh_participants(&mut self) {
        debug!("refresh_participants(): Adding pending participants");
        for participant in &self.consensus.pending_participants {
            self.consensus.participants.insert(participant.id, participant.clone());
        }

        if self.consensus.participants.is_empty() {
            debug!(
                "refresh_participants(): Didn't manage to add any participant, pending were empty"
            );
        }

        self.consensus.pending_participants = vec![];

        let mut inactive = Vec::new();
        let previous_epoch = self.current_epoch() - 1;
        for (index, participant) in self.consensus.participants.clone().iter() {
            match participant.voted {
                Some(epoch) => {
                    if epoch < previous_epoch {
                        inactive.push(*index);
                    }
                }

                None => {
                    if participant.joined < previous_epoch {
                        inactive.push(*index);
                    }
                }
            }
        }

        for index in inactive {
            self.consensus.participants.remove(&index);
        }
    }

    /// Utility function to reset the current consensus state.
    pub fn reset_consensus_state(&mut self) -> Result<()> {
        let genesis_ts = self.consensus.genesis_ts.clone();
        let genesis_block = self.consensus.genesis_block.clone();
        let consensus = ConsensusState {
            genesis_ts,
            genesis_block,
            proposals: vec![],
            orphan_votes: vec![],
            participants: FxIndexMap::with_hasher(FxBuildHasher::default()),
            pending_participants: vec![],
        };

        self.consensus = consensus;
        Ok(())
    }
}
