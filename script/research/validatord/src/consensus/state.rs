use chrono::{NaiveDateTime, Utc};
use log::{debug, error, warn};
use rand::rngs::OsRng;
use std::{
    collections::{hash_map::DefaultHasher, BTreeMap},
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::{Arc, RwLock},
    time::Duration,
};

use darkfi::{
    crypto::{
        keypair::{PublicKey, SecretKey},
        schnorr::{SchnorrPublic, SchnorrSecret},
    },
    net,
    util::serial::{deserialize, serialize, Encodable, SerialDecodable, SerialEncodable},
    Error, Result,
};

use super::{
    block::{Block, BlockInfo, BlockProposal},
    blockchain::{Blockchain, ProposalsChain},
    metadata::{Metadata, StreamletMetadata},
    participant::Participant,
    tx::Tx,
    util::{get_current_time, Timestamp},
    vote::Vote,
};

pub const DELTA: u64 = 10;
const SLED_CONSESUS_STATE_TREE: &[u8] = b"_consensus_state";

/// This struct represents the information required by the consensus algorithm.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ConsensusState {
    /// Genesis block creation timestamp
    pub genesis: Timestamp,
    /// Fork chains containing block proposals
    pub proposals: Vec<ProposalsChain>,
    /// Orphan votes pool, in case a vote reaches a node before the corresponding block
    pub orphan_votes: Vec<Vote>,
    /// Node participation identity
    pub participant: Option<Participant>,
    /// Validators currently participating in the consensus
    pub participants: BTreeMap<u64, Participant>,
    /// Validators to be added on the next epoch as participants
    pub pending_participants: Vec<Participant>,
    /// Last slot participants where refreshed
    pub refreshed: u64,
}

impl ConsensusState {
    pub fn new(db: &sled::Db, id: u64, genesis: i64) -> Result<ConsensusState> {
        let tree = db.open_tree(SLED_CONSESUS_STATE_TREE)?;
        let consensus = if let Some(found) = tree.get(id.to_ne_bytes())? {
            deserialize(&found).unwrap()
        } else {
            let consensus = ConsensusState {
                genesis: Timestamp(genesis),
                proposals: Vec::new(),
                orphan_votes: Vec::new(),
                participant: None,
                participants: BTreeMap::new(),
                pending_participants: vec![],
                refreshed: 0,
            };
            let serialized = serialize(&consensus);
            tree.insert(id.to_ne_bytes(), serialized)?;
            consensus
        };
        Ok(consensus)
    }
}

/// Auxilary structure used for consensus syncing.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusRequest {
    /// Validator id
    pub id: u64,
}

impl net::Message for ConsensusRequest {
    fn name() -> &'static str {
        "consensusrequest"
    }
}

/// Auxilary structure used for consensus syncing.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ConsensusResponse {
    /// Hot/live data used by the consensus algorithm
    pub consensus: ConsensusState,
}

impl net::Message for ConsensusResponse {
    fn name() -> &'static str {
        "consensusresponse"
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
    /// Genesis block hash, used for validations
    pub genesis_block: blake3::Hash,
    /// Participation flag
    pub participating: bool,
}

impl ValidatorState {
    pub fn new(db_path: PathBuf, id: u64, genesis: i64) -> Result<ValidatorStatePtr> {
        // Missing: clock sync
        let secret = SecretKey::random(&mut OsRng);
        let db = sled::open(db_path)?;
        let public = PublicKey::from_secret(secret);
        let consensus = ConsensusState::new(&db, id, genesis)?;
        let blockchain = Blockchain::new(&db, genesis)?;
        let unconfirmed_txs = Vec::new();
        let genesis_block = blake3::hash(&serialize(&Block::genesis_block(genesis)));
        let participating = false;
        Ok(Arc::new(RwLock::new(ValidatorState {
            id,
            secret,
            public,
            db,
            consensus,
            blockchain,
            unconfirmed_txs,
            genesis_block,
            participating,
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
    
    /// Finds the last epoch a proposal or block was generated.
    pub fn last_epoch(&self) -> Result<u64> {
        let mut epoch = 0;
        for chain in &self.consensus.proposals {
            for proposal in &chain.proposals {
                if proposal.block.sl > epoch {
                    epoch = proposal.block.sl;
                }
            }
        }

        // We return here in case proposals exist,
        // so we don't query the sled database.
        if epoch > 0 {
            return Ok(epoch)
        }

        let (last_sl, _) = self.blockchain.last()?.unwrap();
        Ok(last_sl)
    }

    /// Node finds epochs leader, using a simple hash method.
    /// Leader calculation is based on how many nodes are participating in the network.
    pub fn epoch_leader(&mut self) -> u64 {
        let epoch = self.current_epoch();
        // DefaultHasher is used to hash the epoch number
        // because it produces a number string which then can be modulated by the len.
        // blake3 produces alphanumeric
        let mut hasher = DefaultHasher::new();
        epoch.hash(&mut hasher);
        let pos = hasher.finish() % (self.consensus.participants.len() as u64);
        // Since BTreeMap orders by key in asceding order, each node will have
        // the same key in calculated position.
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
        let (previous_hash, index) = self.longest_notarized_chain_last_hash().unwrap();
        let unproposed_txs = self.unproposed_txs(index);
        let metadata = Metadata::new(
            get_current_time(),
            String::from("proof"),
            String::from("r"),
            String::from("s"),
        );
        let sm = StreamletMetadata::new(self.consensus.participants.values().cloned().collect());
        let signed_block = self.secret.sign(
            BlockProposal::to_proposal_hash(previous_hash, epoch, &unproposed_txs, &metadata)
                .as_bytes(),
        );
        Ok(Some(BlockProposal::new(
            self.public,
            signed_block,
            self.id,
            previous_hash,
            epoch,
            unproposed_txs,
            metadata,
            sm,
        )))
    }

    /// Node retrieves all unconfirmed transactions not proposed
    /// in previous blocks of provided index chain.
    pub fn unproposed_txs(&self, index: i64) -> Vec<Tx> {
        let mut unproposed_txs = self.unconfirmed_txs.clone();

        // If index is -1(canonical blockchain) a new fork chain will be generated,
        // therefore all unproposed transactions can be included in the proposal.
        if index == -1 {
            return unproposed_txs
        }

        // We iterate the fork chain proposals to find already proposed transactions
        // and remove them from the local unproposed_txs vector.
        let chain = &self.consensus.proposals[index as usize];
        for proposal in &chain.proposals {
            for tx in &proposal.txs {
                if let Some(pos) = unproposed_txs.iter().position(|txs| *txs == *tx) {
                    unproposed_txs.remove(pos);
                }
            }
        }

        unproposed_txs
    }

    /// Finds the longest fully notarized blockchain the node holds and returns the last block hash
    /// and the chain index.
    pub fn longest_notarized_chain_last_hash(&self) -> Result<(blake3::Hash, i64)> {
        let mut longest_notarized_chain: Option<ProposalsChain> = None;
        let mut length = 0;
        let mut index = -1;
        if !self.consensus.proposals.is_empty() {
            for (i, chain) in self.consensus.proposals.iter().enumerate() {
                if chain.notarized() && chain.proposals.len() > length {
                    longest_notarized_chain = Some(chain.clone());
                    length = chain.proposals.len();
                    index = i as i64;
                }
            }
        }

        let hash = match longest_notarized_chain {
            Some(chain) => chain.proposals.last().unwrap().hash(),
            None => self.blockchain.last()?.unwrap().1,
        };

        Ok((hash, index))
    }

    /// Node receives the proposed block, verifies its sender(epoch leader),
    /// and proceeds with voting on it.
    pub fn receive_proposal(&mut self, proposal: &BlockProposal) -> Result<Option<Vote>> {
        // Node hasn't started participating
        if !self.participating {
            return Ok(None)
        }
        
        // Node refreshes participants records
        self.refresh_participants()?;

        let leader = self.epoch_leader();
        if leader != proposal.id {
            debug!(
                "Received proposal not from epoch leader ({:?}). Proposer: {:?}",
                leader, proposal.id
            );
            return Ok(None)
        }
        if !proposal.public_key.verify(
            BlockProposal::to_proposal_hash(
                proposal.st,
                proposal.sl,
                &proposal.txs,
                &proposal.metadata,
            )
            .as_bytes(),
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
        let mut proposal = proposal.clone();

        // Generate proposal hash
        let proposal_hash = proposal.hash();

        // Add orphan votes
        let mut orphans = Vec::new();
        for vote in self.consensus.orphan_votes.iter() {
            if vote.proposal == proposal_hash {
                proposal.sm.votes.push(vote.clone());
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
                self.consensus.proposals[index as usize].add(&proposal, &self.genesis_block);
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
        if chain.proposals.len() > 1 {
            for proposal in &chain.proposals[..(chain.proposals.len() - 1)] {
                if !proposal.sm.notarized {
                    return false
                }
            }
        }

        true
    }

    /// Given a proposal, node finds the index of the chain it extends.
    pub fn find_extended_chain_index(&mut self, proposal: &BlockProposal) -> Result<i64> {
        for (index, chain) in self.consensus.proposals.iter().enumerate() {
            let last = chain.proposals.last().unwrap();
            let hash = last.hash();
            if proposal.st == hash && proposal.sl > last.sl {
                return Ok(index as i64)
            }
            if proposal.st == last.st && proposal.sl == last.sl {
                debug!("Proposal already received.");
                return Ok(-2)
            }
        }

        let (last_sl, last_block) = self.blockchain.last()?.unwrap();
        if proposal.st != last_block || proposal.sl <= last_sl {
            error!("Proposal doesn't extend any known chains.");
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
    pub fn receive_vote(&mut self, vote: &Vote) -> Result<(bool, Option<Vec<BlockInfo>>)> {
        // Node hasn't started participating
        if !self.participating {
            return Ok((false, None))
        }

        let mut encoded_proposal = vec![];
        let result = vote.proposal.encode(&mut encoded_proposal);
        match result {
            Ok(_) => (),
            Err(e) => {
                error!("Proposal encoding failed. Error: {:?}", e);
                return Ok((false, None))
            }
        };

        if !vote.public_key.verify(&encoded_proposal[..], &vote.vote) {
            debug!("Voter signature couldn't be verified. Voter: {:?}", vote.id);
            return Ok((false, None))
        }
        
        // Node refreshes participants records
        self.refresh_participants()?;

        let nodes_count = self.consensus.participants.len();
        // Checking that the voter can actually vote.
        match self.consensus.participants.get(&vote.id) {
            Some(participant) => {
                if self.current_epoch() <= participant.joined {
                    debug!("Voter joined after current epoch. Voter: {:?}", vote.id);
                    return Ok((false, None))
                }
            }
            None => {
                debug!("Voter is not a participant. Voter: {:?}", vote.id);
                return Ok((false, None))
            }
        }

        let proposal = self.find_proposal(&vote.proposal).unwrap();
        if proposal == None {
            debug!("Received vote for unknown proposal.");
            if !self.consensus.orphan_votes.contains(vote) {
                self.consensus.orphan_votes.push(vote.clone());
            }
            return Ok((false, None))
        }

        let (unwrapped, chain_index) = proposal.unwrap();
        if !unwrapped.sm.votes.contains(vote) {
            unwrapped.sm.votes.push(vote.clone());

            let mut to_broadcast = Vec::new();
            if !unwrapped.sm.notarized && unwrapped.sm.votes.len() > (2 * nodes_count / 3) {
                unwrapped.sm.notarized = true;
                to_broadcast = self.chain_finalization(chain_index)?;
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

            return Ok((true, Some(to_broadcast)))
        }
        return Ok((false, None))
    }

    /// Node searches it the chains it holds for provided proposal.
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

    /// Note removes provided transactions vector, from unconfirmed_txs, if they exist.
    pub fn remove_txs(&mut self, transactions: Vec<Tx>) -> Result<()> {
        for tx in transactions {
            if let Some(pos) = self.unconfirmed_txs.iter().position(|txs| *txs == tx) {
                self.unconfirmed_txs.remove(pos);
            }
        }

        Ok(())
    }

    /// Provided an index, node checks if chain can be finalized.
    /// Consensus finalization logic: If node has observed the notarization of 3 consecutive
    /// proposals in a fork chain, it finalizes (appends to canonical blockchain) all proposals up to the middle block.
    /// When fork chain proposals are finalized, rest fork chains not starting by those proposals are removed.
    pub fn chain_finalization(&mut self, chain_index: i64) -> Result<Vec<BlockInfo>> {
        let mut to_broadcast = Vec::new();
        let chain = &mut self.consensus.proposals[chain_index as usize];
        let len = chain.proposals.len();
        if len > 2 {
            let mut consecutive = 0;
            for proposal in &chain.proposals {
                if proposal.sm.notarized {
                    consecutive += 1;
                } else {
                    break
                }
            }

            if consecutive > 2 {
                let mut finalized = Vec::new();
                for proposal in &mut chain.proposals[..(consecutive - 1)] {
                    proposal.sm.finalized = true;
                    finalized.push(proposal.clone());
                }
                chain.proposals.drain(0..(consecutive - 1));
                for proposal in &finalized {
                    self.blockchain.add_by_proposal(proposal.clone())?;
                    self.remove_txs(proposal.txs.clone())?;
                    to_broadcast.push(BlockInfo::new(
                        proposal.st,
                        proposal.sl,
                        proposal.txs.clone(),
                        proposal.metadata.clone(),
                        proposal.sm.clone(),
                    ));
                }

                let (last_sl, last_block) = self.blockchain.last()?.unwrap();
                let mut dropped = Vec::new();
                for chain in self.consensus.proposals.iter() {
                    let first = chain.proposals.first().unwrap();
                    if first.st != last_block || first.sl <= last_sl {
                        dropped.push(chain.clone());
                    }
                }
                for chain in dropped {
                    self.consensus.proposals.retain(|c| *c != chain);
                }

                // Remove orphan votes
                let mut orphans = Vec::new();
                for vote in self.consensus.orphan_votes.iter() {
                    if vote.sl <= last_sl {
                        orphans.push(vote.clone());
                    }
                }
                for vote in orphans {
                    self.consensus.orphan_votes.retain(|v| *v != vote);
                }
            }
        }

        Ok(to_broadcast)
    }
    
    /// Append node participant identity to the pending participants list.
    pub fn append_self_participant(&mut self, participant: Participant) {
        self.consensus.participant = Some(participant.clone());
        self.append_participant(participant);
    }

    /// Node retreives a new participant and appends it to the pending participants list.
    pub fn append_participant(&mut self, participant: Participant) -> bool {
        if self.consensus.pending_participants.contains(&participant) {
            return false
        }
        self.consensus.pending_participants.push(participant);
        true
    }

    /// Refresh the participants map, to retain only the active ones.
    /// Active nodes are considered those that on the epoch the last proposal
    /// was generated, either voted or joined the previous epoch.
    /// That ensures we cover the case of chosen leader beign inactive.
    pub fn refresh_participants(&mut self) -> Result<()> {
        // Node checks if it should refresh its participants list
        let epoch = self.current_epoch();
        if epoch <= self.consensus.refreshed {
            debug!("refresh_participants(): Participants have been refreshed this epoch.");
            return Ok(())
        }

        debug!("refresh_participants(): Adding pending participants");
        for participant in &self.consensus.pending_participants {
            self.consensus.participants.insert(participant.id, participant.clone());
        }

        if self.consensus.participants.is_empty() {
            debug!(
                "refresh_participants(): Didn't manage to add any participant, pending were empty."
            );
        }

        self.consensus.pending_participants = vec![];

        let mut inactive = Vec::new();
        let mut last_epoch = self.last_epoch()?;

        // This check ensures that we don't chech the current epoch,
        // as a node might receive the proposal of current epoch before
        // starting refreshing participants, so the last_epoch will be
        // the current one.
        if last_epoch >= epoch {
            last_epoch = epoch - 1;
        }

        let previous_epoch = last_epoch - 1;

        error!(
            "refresh_participants(): Checking epochs: previous - {:?}, last - {:?}",
            previous_epoch, last_epoch
        );

        for (index, participant) in self.consensus.participants.clone().iter() {
            match participant.voted {
                Some(epoch) => {
                    if epoch < last_epoch {
                        warn!("refresh_participants(): Inactive participant: {:?}", participant);
                        inactive.push(*index);
                    }
                }
                None => {
                    if participant.joined < previous_epoch {
                        warn!("refresh_participants(): Inactive participant: {:?}", participant);
                        inactive.push(*index);
                    }
                }
            }
        }

        for index in inactive {
            self.consensus.participants.remove(&index);
        }

        if self.consensus.participants.is_empty() {
            // If no nodes are active, node becomes a single node network.
            let mut participant = self.consensus.participant.clone().unwrap();
            participant.joined = epoch;
            self.consensus.participant = Some(participant.clone());
            self.consensus.participants.insert(participant.id, participant.clone());
        }

        self.consensus.refreshed = epoch;

        Ok(())
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

    /// Util function to reset the current consensus state.
    pub fn reset_consensus_state(&mut self) -> Result<()> {
        let genesis = self.consensus.genesis.clone();
        let consensus = ConsensusState {
            genesis,
            proposals: Vec::new(),
            orphan_votes: Vec::new(),
            participant: None,
            participants: BTreeMap::new(),
            pending_participants: vec![],
            refreshed: 0,
        };

        self.consensus = consensus;
        Ok(())
    }
}
