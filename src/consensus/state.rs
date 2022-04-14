use chrono::{NaiveDateTime, Utc};
use log::{debug, error};
use rand::rngs::OsRng;
use std::{
    collections::{hash_map::DefaultHasher, BTreeMap},
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::{Arc, RwLock},
    time::Duration,
};

use crate::{
    crypto::{
        keypair::{PublicKey, SecretKey},
        schnorr::{SchnorrPublic, SchnorrSecret},
    },
    util::serial::{deserialize, serialize, Encodable, SerialDecodable, SerialEncodable},
    Error, Result,
};

use super::{
    block::BlockProposal,
    blockchain::{Blockchain, ProposalsChain},
    participant::Participant,
    tx::Tx,
    util::{get_current_time, to_block_serial, Timestamp, GENESIS_HASH_BYTES},
    vote::Vote,
};

const DELTA: u64 = 60;
const SLED_CONSESUS_STATE_TREE: &[u8] = b"_consensus_state";

/// This struct represents the information required by the consensus algorithm.
/// Last finalized block hash and slot are used because SLED order follows the Ord implementation for Vec<u8>.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusState {
    /// Genesis block creation timestamp
    pub genesis: Timestamp,
    /// Last finalized block hash,
    pub last_block: blake3::Hash,
    /// Last finalized block slot,
    pub last_sl: u64,
    /// Fork chains containing block proposals
    pub proposals: Vec<ProposalsChain>,
    /// Orphan votes pool, in case a vote reaches a node before the corresponding block
    pub orphan_votes: Vec<Vote>,
    /// Validators currently participating in the concensus
    pub participants: BTreeMap<u64, Participant>,
    /// Validators to be added on next epoch as participants
    pub pending_participants: Vec<Participant>,
}

impl ConsensusState {
    pub fn new(db: &sled::Db, id: u64, genesis: i64) -> Result<ConsensusState> {
        let tree = db.open_tree(SLED_CONSESUS_STATE_TREE)?;
        let consensus = if let Some(found) = tree.get(id.to_ne_bytes())? {
            deserialize(&found).unwrap()
        } else {
            let hash = blake3::Hash::from(GENESIS_HASH_BYTES);
            let genesis_hash = blake3::hash(&to_block_serial(hash, 0, &vec![]));
            let consensus = ConsensusState {
                genesis: Timestamp(genesis),
                last_block: genesis_hash,
                last_sl: 0,
                proposals: Vec::new(),
                orphan_votes: Vec::new(),
                participants: BTreeMap::new(),
                pending_participants: Vec::new(),
            };
            let serialized = serialize(&consensus);
            tree.insert(id.to_ne_bytes(), serialized)?;
            consensus
        };
        Ok(consensus)
    }
}

/// Atomic pointer to validator state.
pub type ValidatorStatePtr = Arc<RwLock<ValidatorState>>;

/// This struct represents the state of a validator node.
pub struct ValidatorState {
    /// Validator id
    pub id: u64,
    /// Secret key, to sign messages
    pub secret: SecretKey,
    /// Validator public key
    pub public: PublicKey,
    /// Sled database for storage
    pub db: sled::Db,
    /// Hot/live data used by the consensus algorithm
    pub consensus: ConsensusState,
    /// Canonical (finalized) blockchain
    pub blockchain: Blockchain,
    /// Pending transactions
    pub unconfirmed_txs: Vec<Tx>,
}

impl ValidatorState {
    pub fn new(db_path: PathBuf, id: u64, genesis: i64) -> Result<ValidatorStatePtr> {
        // TODO: clock sync
        let secret = SecretKey::random(&mut OsRng);
        let db = sled::open(db_path)?;
        let public = PublicKey::from_secret(secret);
        let consensus = ConsensusState::new(&db, id, genesis)?;
        let blockchain = Blockchain::new(&db)?;
        let unconfirmed_txs = Vec::new();
        Ok(Arc::new(RwLock::new(ValidatorState {
            id,
            secret,
            public,
            db,
            consensus,
            blockchain,
            unconfirmed_txs,
        })))
    }

    /// Node retreives a transaction and append it to the unconfirmed transactions list.
    /// Additional validity rules must be defined by the protocol for transactions.
    pub fn append_tx(&mut self, tx: Tx) -> bool {
        if self.unconfirmed_txs.contains(&tx) {
            return false
        }
        self.unconfirmed_txs.push(tx);
        true
    }

    /// Node calculates seconds until next epoch starting time.
    /// Epochs duration is configured using the delta value.
    pub fn next_epoch_start(&self) -> Duration {
        let start_time = NaiveDateTime::from_timestamp(self.consensus.genesis.0, 0);
        let current_epoch = self.current_epoch() + 1;
        let next_epoch_start_timestamp =
            (current_epoch * (2 * DELTA)) + (start_time.timestamp() as u64);
        let next_epoch_start =
            NaiveDateTime::from_timestamp(next_epoch_start_timestamp.try_into().unwrap(), 0);
        let current_time = NaiveDateTime::from_timestamp(Utc::now().timestamp(), 0);
        let diff = next_epoch_start - current_time;
        Duration::new(diff.num_seconds().try_into().unwrap(), 0)
    }

    /// Node calculates current epoch, based on elapsed time from the genesis block.
    /// Epochs duration is configured using the delta value.
    pub fn current_epoch(&self) -> u64 {
        self.consensus.genesis.clone().elapsed() / (2 * DELTA)
    }

    /// Node finds epochs leader, using a simple hash method.
    /// Leader calculation is based on how many nodes are participating in the network.
    pub fn epoch_leader(&mut self) -> u64 {
        let epoch = self.current_epoch();
        let mut hasher = DefaultHasher::new();
        epoch.hash(&mut hasher);
        self.zero_participants_check();
        let pos = hasher.finish() % (self.consensus.participants.len() as u64);
        self.consensus.participants.iter().nth(pos as usize).unwrap().1.id
    }

    /// Node checks if they are the current epoch leader.
    pub fn is_epoch_leader(&mut self) -> bool {
        let leader = self.epoch_leader();
        self.id == leader
    }

    /// Node generates a block proposal for the current epoch,
    /// containing all uncorfirmed transactions.
    /// Proposal extends the longest notarized fork chain the node holds.
    pub fn propose(&self) -> Result<Option<BlockProposal>> {
        let epoch = self.current_epoch();
        let previous_hash = self.longest_notarized_chain_last_hash().unwrap();
        let unproposed_txs = self.unproposed_txs();
        let signed_block =
            self.secret.sign(&to_block_serial(previous_hash, epoch, &unproposed_txs)[..]);
        Ok(Some(BlockProposal::new(
            self.public,
            signed_block,
            self.id,
            previous_hash,
            epoch,
            unproposed_txs,
            get_current_time(),
            String::from("proof"),
            String::from("r"),
            String::from("s"),
            self.consensus.participants.values().cloned().collect(),
        )))
    }

    /// Node retrieves all unconfiremd transactions not proposed in previous blocks.
    pub fn unproposed_txs(&self) -> Vec<Tx> {
        let mut unproposed_txs = self.unconfirmed_txs.clone();
        for chain in &self.consensus.proposals {
            for proposal in &chain.proposals {
                for tx in &proposal.txs {
                    if let Some(pos) = unproposed_txs.iter().position(|txs| *txs == *tx) {
                        unproposed_txs.remove(pos);
                    }
                }
            }
        }
        unproposed_txs
    }

    /// Finds the longest fully notarized blockchain the node holds and returns the last block hash.
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
            let last = longest_notarized_chain.proposals.last().unwrap();
            blake3::hash(&to_block_serial(last.st, last.sl, &last.txs))
        } else {
            self.consensus.last_block
        };
        Ok(hash)
    }

    /// Node receives the proposed block, verifies its sender(epoch leader),
    /// and proceeds with voting on it.
    pub fn receive_proposal(&mut self, proposal: &BlockProposal) -> Result<Option<Vote>> {
        let leader = self.epoch_leader();
        if leader != proposal.id {
            debug!(
                "Received proposal not from epoch leader ({:?}). Proposer: {:?}",
                leader, proposal.id
            );
            return Ok(None)
        }
        if !proposal.public_key.verify(
            &to_block_serial(proposal.st, proposal.sl, &proposal.txs)[..],
            &proposal.signature,
        ) {
            debug!("Proposer signature couldn't be verified. Proposer: {:?}", proposal.id);
            return Ok(None)
        }
        self.vote(proposal)
    }

    /// Given a proposal, node finds which blockchain it extends.
    /// If proposal extends the canonical blockchain, a new fork chain is created.
    /// Node votes on the proposal, only if it extends the longest notarized fork chain it has seen.
    pub fn vote(&mut self, proposal: &BlockProposal) -> Result<Option<Vote>> {
        self.zero_participants_check();
        let mut proposal = proposal.clone();

        // Generate proposal hash
        let proposal_hash = blake3::hash(&to_block_serial(proposal.st, proposal.sl, &proposal.txs));

        // Add orphan votes
        let mut orphans = Vec::new();
        for vote in self.consensus.orphan_votes.iter() {
            if vote.proposal == proposal_hash {
                proposal.metadata.sm.votes.push(vote.clone());
                orphans.push(vote.clone());
            }
        }
        for vote in orphans {
            self.consensus.orphan_votes.retain(|v| *v != vote);
        }

        let index = self.find_extended_chain_index(&proposal).unwrap();

        if index == -2 {
            return Ok(None)
        }
        let chain = match index {
            -1 => {
                let proposalschain = ProposalsChain::new(proposal.clone());
                self.consensus.proposals.push(proposalschain);
                self.consensus.proposals.last().unwrap()
            }
            _ => {
                self.consensus.proposals[index as usize].add(&proposal);
                &self.consensus.proposals[index as usize]
            }
        };

        if self.extends_notarized_chain(chain) {
            let signed_hash = self.secret.sign(&serialize(&proposal_hash)[..]);
            return Ok(Some(Vote::new(
                self.public,
                signed_hash,
                proposal_hash,
                proposal.sl,
                self.id,
            )))
        }
        Ok(None)
    }

    /// Node verifies if provided chain is notarized excluding the last block.
    pub fn extends_notarized_chain(&self, chain: &ProposalsChain) -> bool {
        for proposal in &chain.proposals[..(chain.proposals.len() - 1)] {
            if !proposal.metadata.sm.notarized {
                return false
            }
        }
        true
    }

    /// Given a proposal, node finds the index of the chain it extends.
    pub fn find_extended_chain_index(&self, proposal: &BlockProposal) -> Result<i64> {
        for (index, chain) in self.consensus.proposals.iter().enumerate() {
            let last = chain.proposals.last().unwrap();
            let hash = blake3::hash(&to_block_serial(last.st, last.sl, &last.txs));
            if proposal.st == hash && proposal.sl > last.sl {
                return Ok(index as i64)
            }
            if proposal.st == last.st && proposal.sl == last.sl {
                debug!("Proposal already received.");
                return Ok(-2)
            }
        }

        if proposal.st != self.consensus.last_block || proposal.sl <= self.consensus.last_sl {
            debug!("Proposal doesn't extend any known chains.");
            return Ok(-2)
        }

        Ok(-1)
    }

    /// Node receives a vote for a proposal.
    /// First, sender is verified using their public key.
    /// Proposal is searched in nodes fork chains.
    /// If the vote wasn't received before, it is appended to proposal votes list.
    /// When a node sees 2n/3 votes for a proposal it notarizes it.
    /// When a proposal gets notarized, the transactions it contains are removed from
    /// nodes unconfirmed transactions list.
    /// Finally, we check if the notarization of the proposal can finalize parent proposals
    /// in its chain.
    pub fn receive_vote(&mut self, vote: &Vote) -> Result<bool> {
        let mut encoded_proposal = vec![];
        let result = vote.proposal.encode(&mut encoded_proposal);
        match result {
            Ok(_) => (),
            Err(e) => {
                error!("Proposal encoding failed. Error: {:?}", e);
                return Ok(false)
            }
        };

        if !vote.public_key.verify(&encoded_proposal[..], &vote.vote) {
            debug!("Voter signature couldn't be verified. Voter: {:?}", vote.id);
            return Ok(false)
        }

        let nodes_count = self.consensus.participants.len();
        self.zero_participants_check();

        // Checking that the voter can actually vote.
        match self.consensus.participants.get(&vote.id) {
            Some(participant) => {
                if self.current_epoch() <= participant.joined {
                    debug!("Voter joined after current epoch. Voter: {:?}", vote.id);
                    return Ok(false)
                }
            }
            None => {
                debug!("Voter is not a participant. Voter: {:?}", vote.id);
                return Ok(false)
            }
        }

        let proposal = self.find_proposal(&vote.proposal).unwrap();
        if proposal == None {
            debug!("Received vote for unknown proposal.");
            if !self.consensus.orphan_votes.contains(vote) {
                self.consensus.orphan_votes.push(vote.clone());
            }
            return Ok(false)
        }

        let (unwrapped, chain_index) = proposal.unwrap();
        if !unwrapped.metadata.sm.votes.contains(vote) {
            unwrapped.metadata.sm.votes.push(vote.clone());

            if !unwrapped.metadata.sm.notarized &&
                unwrapped.metadata.sm.votes.len() > (2 * nodes_count / 3)
            {
                unwrapped.metadata.sm.notarized = true;
                self.chain_finalization(chain_index)?;
            }

            // updating participant vote
            let exists = self.consensus.participants.get(&vote.id);
            let mut participant = match exists {
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

            return Ok(true)
        }
        Ok(false)
    }

    /// Node searches it the chains it holds for provided proposal.
    pub fn find_proposal(
        &mut self,
        vote_proposal: &blake3::Hash,
    ) -> Result<Option<(&mut BlockProposal, i64)>> {
        for (index, chain) in &mut self.consensus.proposals.iter_mut().enumerate() {
            for proposal in chain.proposals.iter_mut().rev() {
                let proposal_hash =
                    blake3::hash(&to_block_serial(proposal.st, proposal.sl, &proposal.txs));
                if vote_proposal == &proposal_hash {
                    return Ok(Some((proposal, index as i64)))
                }
            }
        }
        Ok(None)
    }

    /// Provided an index, node checks if chain can be finalized.
    /// Consensus finalization logic: If node has observed the notarization of 3 consecutive
    /// proposals in a fork chain, it finalizes (appends to canonical blockchain) all proposals up to the middle block.
    /// When fork chain proposals are finalized, rest fork chains not starting by those proposals are removed.
    pub fn chain_finalization(&mut self, chain_index: i64) -> Result<()> {
        let chain = &mut self.consensus.proposals[chain_index as usize];
        let len = chain.proposals.len();
        if len > 2 {
            let mut consecutive = 0;
            for proposal in &chain.proposals {
                if proposal.metadata.sm.notarized {
                    consecutive += 1;
                } else {
                    break
                }
            }

            if consecutive > 2 {
                let mut finalized = Vec::new();
                for proposal in &mut chain.proposals[..(consecutive - 1)] {
                    proposal.metadata.sm.finalized = true;
                    finalized.push(proposal.clone());
                    for tx in proposal.txs.clone() {
                        if let Some(pos) = self.unconfirmed_txs.iter().position(|txs| *txs == tx) {
                            self.unconfirmed_txs.remove(pos);
                        }
                    }
                }
                chain.proposals.drain(0..(consecutive - 1));
                for proposal in &finalized {
                    let hash = self.blockchain.add(proposal.clone())?;
                    self.consensus.last_block = hash;
                    self.consensus.last_sl = proposal.sl;
                }

                let mut dropped = Vec::new();
                for chain in self.consensus.proposals.iter() {
                    let first = chain.proposals.first().unwrap();
                    if first.st != self.consensus.last_block || first.sl <= self.consensus.last_sl {
                        dropped.push(chain.clone());
                    }
                }
                for chain in dropped {
                    self.consensus.proposals.retain(|c| *c != chain);
                }

                // Remove orphan votes
                let mut orphans = Vec::new();
                for vote in self.consensus.orphan_votes.iter() {
                    if vote.sl <= self.consensus.last_sl {
                        orphans.push(vote.clone());
                    }
                }
                for vote in orphans {
                    self.consensus.orphan_votes.retain(|v| *v != vote);
                }
            }
        }

        Ok(())
    }

    /// Node retreives a new participant and appends it to the pending participants list.
    pub fn append_participant(&mut self, participant: Participant) -> bool {
        if self.consensus.pending_participants.contains(&participant) {
            return false
        }
        self.consensus.pending_participants.push(participant);
        true
    }

    /// This prevent the extreme case scenario where network is initialized, but some nodes
    /// have not pushed the initial participants in the map.
    pub fn zero_participants_check(&mut self) {
        if self.consensus.participants.len() == 0 {
            for participant in &self.consensus.pending_participants {
                self.consensus.participants.insert(participant.id, participant.clone());
            }
            self.consensus.pending_participants = Vec::new();
        }
    }

    /// Node refreshes participants map, to retain only the active ones.
    /// Active nodes are considered those who joined or voted on previous epoch.
    pub fn refresh_participants(&mut self) {
        // adding pending participants
        for participant in &self.consensus.pending_participants {
            self.consensus.participants.insert(participant.id, participant.clone());
        }
        self.consensus.pending_participants = Vec::new();

        let mut inactive = Vec::new();
        let previous_epoch = self.current_epoch() - 1;
        for (index, participant) in self.consensus.participants.clone().iter() {
            match participant.voted {
                Some(epoch) => {
                    if epoch < previous_epoch {
                        inactive.push(index.clone());
                    }
                }
                None => {
                    if participant.joined < previous_epoch {
                        inactive.push(index.clone());
                    }
                }
            }
        }
        for index in inactive {
            self.consensus.participants.remove(&index);
        }
    }

    /// Util function to save the current consensus state to provided file path.
    pub fn save_consensus_state(&self) -> Result<()> {
        let tree = self.db.open_tree(SLED_CONSESUS_STATE_TREE).unwrap();
        let serialized = serialize(&self.consensus);
        match tree.insert(self.id.to_ne_bytes(), serialized) {
            Err(_) => Err(Error::OperationFailed),
            _ => Ok(()),
        }
    }
}
