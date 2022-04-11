use chrono::{NaiveDateTime, Utc};
use log::{debug, error};
use rand::rngs::OsRng;
use std::{
    collections::{hash_map::DefaultHasher, BTreeMap},
    hash::{Hash, Hasher},
    sync::{Arc, RwLock},
    time::Duration,
};

use crate::{
    crypto::{
        keypair::{PublicKey, SecretKey},
        schnorr::{SchnorrPublic, SchnorrSecret},
    },
    encode_payload,
    util::serial::{deserialize, serialize, Encodable, SerialDecodable, SerialEncodable},
    Error, Result,
};

use super::{
    block::{Block, BlockProposal},
    blockchain::{Blockchain, ProposalsChain},
    participant::Participant,
    tx::Tx,
    util::{get_current_time, Timestamp, GENESIS_HASH_BYTES},
    vote::Vote,
};

const DELTA: u64 = 60;
const SLED_STATE_TREE: &[u8] = b"_state";

/// Atomic pointer to state.
pub type StatePtr = Arc<RwLock<State>>;

/// This struct represents the state of a consensus node.
/// Each node is numbered and has a secret-public keys pair, to sign messages.
/// Nodes hold the canonical(finalized) blockchain, a set of fork chains containing proposals
/// and a set of unconfirmed pending transactions.
/// Additionally, each node keeps tracks of all participating nodes.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct State {
    pub id: u64,
    pub genesis: Timestamp,
    pub secret: SecretKey,
    pub public: PublicKey,
    pub blockchain: Blockchain,
    pub proposals: Vec<ProposalsChain>,
    pub unconfirmed_txs: Vec<Tx>,
    pub orphan_votes: Vec<Vote>,
    pub participants: BTreeMap<u64, Participant>,
    pub pending_participants: Vec<Participant>,
}

impl State {
    pub fn new(id: u64, genesis: Timestamp, init_block: Block) -> State {
        // TODO: clock sync
        let secret = SecretKey::random(&mut OsRng);
        State {
            id,
            genesis,
            secret,
            public: PublicKey::from_secret(secret),
            blockchain: Blockchain::new(init_block),
            proposals: Vec::new(),
            unconfirmed_txs: Vec::new(),
            orphan_votes: Vec::new(),
            participants: BTreeMap::new(),
            pending_participants: Vec::new(),
        }
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
        let start_time = NaiveDateTime::from_timestamp(self.genesis.0, 0);
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
        self.genesis.clone().elapsed() / (2 * DELTA)
    }

    /// Node finds epochs leader, using a simple hash method.
    /// Leader calculation is based on how many nodes are participating in the network.
    pub fn epoch_leader(&mut self) -> u64 {
        let epoch = self.current_epoch();
        let mut hasher = DefaultHasher::new();
        epoch.hash(&mut hasher);
        self.zero_participants_check();
        let pos = hasher.finish() % (self.participants.len() as u64);
        self.participants.iter().nth(pos as usize).unwrap().1.id
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
        let mut encoded_block = vec![];
        encode_payload!(&mut encoded_block, previous_hash, epoch, unproposed_txs);
        let signed_block = self.secret.sign(&encoded_block[..]);
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
            self.participants.values().cloned().collect(),
        )))
    }

    /// Node retrieves all unconfiremd transactions not proposed in previous blocks.
    pub fn unproposed_txs(&self) -> Vec<Tx> {
        let mut unproposed_txs = self.unconfirmed_txs.clone();
        for chain in &self.proposals {
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
        let mut buf = vec![];
        if !self.proposals.is_empty() {
            let mut longest_notarized_chain = &self.proposals[0];
            let mut length = longest_notarized_chain.proposals.len();
            if self.proposals.len() > 1 {
                for chain in &self.proposals[1..] {
                    if chain.notarized() && chain.proposals.len() > length {
                        length = chain.proposals.len();
                        longest_notarized_chain = chain;
                    }
                }
            }
            let last = longest_notarized_chain.proposals.last().unwrap();
            encode_payload!(&mut buf, last.st, last.sl, last.txs);
        } else {
            let last = self.blockchain.blocks.last().unwrap();
            encode_payload!(&mut buf, last.st, last.sl, last.txs);
        };
        Ok(blake3::hash(&serialize(&buf)))
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
        let mut encoded_block = vec![];
        encode_payload!(&mut encoded_block, proposal.st, proposal.sl, proposal.txs);
        if !proposal.public_key.verify(&encoded_block[..], &proposal.signature) {
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
        let mut buf = vec![];
        encode_payload!(&mut buf, proposal.st, proposal.sl, proposal.txs);
        let proposal_hash = blake3::hash(&serialize(&buf));

        // Add orphan votes
        let mut orphans = Vec::new();
        for vote in self.orphan_votes.iter() {
            if vote.proposal == proposal_hash {
                proposal.metadata.sm.votes.push(vote.clone());
                orphans.push(vote.clone());
            }
        }
        for vote in orphans {
            self.orphan_votes.retain(|v| *v != vote);
        }

        let index = self.find_extended_chain_index(&proposal).unwrap();

        if index == -2 {
            return Ok(None)
        }
        let chain = match index {
            -1 => {
                let proposalschain = ProposalsChain::new(proposal.clone());
                self.proposals.push(proposalschain);
                self.proposals.last().unwrap()
            }
            _ => {
                self.proposals[index as usize].add(&proposal);
                &self.proposals[index as usize]
            }
        };

        if self.extends_notarized_chain(chain) {
            let mut encoded_hash = vec![];
            encode_payload!(&mut encoded_hash, proposal_hash);
            let signed_hash = self.secret.sign(&encoded_hash[..]);
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
        for (index, chain) in self.proposals.iter().enumerate() {
            let last = chain.proposals.last().unwrap();
            let mut buf = vec![];
            encode_payload!(&mut buf, last.st, last.sl, last.txs);
            let hash = blake3::hash(&serialize(&buf));
            if proposal.st == hash && proposal.sl > last.sl {
                return Ok(index as i64)
            }
            if proposal.st == last.st && proposal.sl == last.sl {
                debug!("Proposal already received.");
                return Ok(-2)
            }
        }

        let last = self.blockchain.blocks.last().unwrap();
        let mut buf = vec![];
        encode_payload!(&mut buf, last.st, last.sl, last.txs);
        let hash = blake3::hash(&serialize(&buf));
        if proposal.st != hash || proposal.sl <= last.sl {
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
    pub fn receive_vote(&mut self, vote: &Vote) -> bool {
        let mut encoded_proposal = vec![];
        let result = vote.proposal.encode(&mut encoded_proposal);
        match result {
            Ok(_) => (),
            Err(e) => {
                error!("Proposal encoding failed. Error: {:?}", e);
                return false
            }
        };

        if !vote.public_key.verify(&encoded_proposal[..], &vote.vote) {
            debug!("Voter signature couldn't be verified. Voter: {:?}", vote.id);
            return false
        }

        let nodes_count = self.participants.len();
        self.zero_participants_check();

        let proposal = self.find_proposal(&vote.proposal).unwrap();
        if proposal == None {
            debug!("Received vote for unknown proposal.");
            if !self.orphan_votes.contains(vote) {
                self.orphan_votes.push(vote.clone());
            }
            return false
        }

        let (unwrapped, chain_index) = proposal.unwrap();
        if !unwrapped.metadata.sm.votes.contains(vote) {
            unwrapped.metadata.sm.votes.push(vote.clone());

            if !unwrapped.metadata.sm.notarized &&
                unwrapped.metadata.sm.votes.len() > (2 * nodes_count / 3)
            {
                unwrapped.metadata.sm.notarized = true;
                self.chain_finalization(chain_index);
            }

            // updating participant vote
            let exists = self.participants.get(&vote.id);
            let mut participant = match exists {
                Some(p) => p.clone(),
                None => Participant::new(vote.id, vote.sl),
            };
            participant.voted = Some(vote.sl);
            self.participants.insert(participant.id, participant);

            return true
        }
        false
    }

    /// Node searches it the chains it holds for provided proposal.
    pub fn find_proposal(
        &mut self,
        vote_proposal: &blake3::Hash,
    ) -> Result<Option<(&mut BlockProposal, i64)>> {
        for (index, chain) in &mut self.proposals.iter_mut().enumerate() {
            for proposal in chain.proposals.iter_mut().rev() {
                let mut buf = vec![];
                encode_payload!(&mut buf, proposal.st, proposal.sl, proposal.txs);
                let proposal_hash = blake3::hash(&serialize(&buf));
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
    pub fn chain_finalization(&mut self, chain_index: i64) {
        let chain = &mut self.proposals[chain_index as usize];
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
                    self.blockchain.blocks.push(Block::from_proposal(proposal.clone()));
                }

                let last = self.blockchain.blocks.last().unwrap();
                let hash = blake3::hash(&serialize(last));
                let mut dropped = Vec::new();
                for chain in self.proposals.iter() {
                    let first = chain.proposals.first().unwrap();
                    if first.st != hash || first.sl <= last.sl {
                        dropped.push(chain.clone());
                    }
                }
                for chain in dropped {
                    self.proposals.retain(|c| *c != chain);
                }

                // Remove orphan votes
                let mut orphans = Vec::new();
                for vote in self.orphan_votes.iter() {
                    if vote.sl <= last.sl {
                        orphans.push(vote.clone());
                    }
                }
                for vote in orphans {
                    self.orphan_votes.retain(|v| *v != vote);
                }
            }
        }
    }

    /// Node retreives a new participant and appends it to the pending participants list.
    pub fn append_participant(&mut self, participant: Participant) -> bool {
        if self.pending_participants.contains(&participant) {
            return false
        }
        self.pending_participants.push(participant);
        true
    }

    /// This prevent the extreme case scenario where network is initialized, but some nodes
    /// have not pushed the initial participants in the map.
    pub fn zero_participants_check(&mut self) {
        if self.participants.len() == 0 {
            for participant in &self.pending_participants {
                self.participants.insert(participant.id, participant.clone());
            }
            self.pending_participants = Vec::new();
        }
    }

    /// Node refreshes participants map, to retain only the active ones.
    /// Active nodes are considered those who joined or voted on previous epoch.
    pub fn refresh_participants(&mut self) {
        // adding pending participants
        for participant in &self.pending_participants {
            self.participants.insert(participant.id, participant.clone());
        }
        self.pending_participants = Vec::new();

        let mut inactive = Vec::new();
        let previous_epoch = self.current_epoch() - 1;
        for (index, participant) in self.participants.clone().iter() {
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
            self.participants.remove(&index);
        }
    }

    /// Util function to save the current node state to provided file path.
    pub fn save(&self, db: &sled::Db) -> Result<()> {
        let tree = db.open_tree(SLED_STATE_TREE).unwrap();
        let serialized = serialize(self);
        match tree.insert(self.id.to_ne_bytes(), serialized) {
            Err(_) => Err(Error::OperationFailed),
            _ => Ok(()),
        }
    }

    /// Util function to load current node state by the provided file path.
    //  If file is not found, node state is reset.
    pub fn load_or_create(genesis: i64, id: u64, db: &sled::Db) -> Result<State> {
        let tree = db.open_tree(SLED_STATE_TREE).unwrap();
        if let Some(found) = tree.get(id.to_ne_bytes()).unwrap() {
            Ok(deserialize(&found).unwrap())
        } else {
            Self::reset(genesis, id, db)
        }
    }

    /// Util function to load the current node state by the provided folder path.
    pub fn load_current_state(genesis: i64, id: u64, db: &sled::Db) -> Result<StatePtr> {
        let state = Self::load_or_create(genesis, id, db)?;
        Ok(Arc::new(RwLock::new(state)))
    }

    /// Util function to reset node state.
    pub fn reset(genesis: i64, id: u64, db: &sled::Db) -> Result<State> {
        // Genesis block is generated.
        let mut genesis_block = Block::new(
            blake3::Hash::from(GENESIS_HASH_BYTES),
            0,
            vec![],
            get_current_time(),
            String::from("proof"),
            String::from("r"),
            String::from("s"),
            vec![],
        );
        genesis_block.metadata.sm.notarized = true;
        genesis_block.metadata.sm.finalized = true;

        let genesis_time = Timestamp(genesis);

        let state = Self::new(id, genesis_time, genesis_block);
        state.save(db)?;
        Ok(state)
    }
}
