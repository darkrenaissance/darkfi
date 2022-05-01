// TODO: Use sets instead of vectors where possible.
use std::{
    collections::{hash_map::DefaultHasher, BTreeMap},
    hash::{Hash, Hasher},
    time::Duration,
};

use async_std::sync::{Arc, Mutex, RwLock};
use chrono::{NaiveDateTime, Utc};
use lazy_init::Lazy;
use log::{debug, error, info, warn};
use rand::rngs::OsRng;

use super::{
    Block, BlockInfo, BlockProposal, Metadata, Participant, ProposalChain, StreamletMetadata,
    Timestamp, Tx, Vote,
};
use crate::{
    blockchain::Blockchain,
    crypto::{
        address::Address,
        keypair::{PublicKey, SecretKey},
        schnorr::{SchnorrPublic, SchnorrSecret},
    },
    net,
    node::{
        state::{state_transition, StateUpdate},
        Client, MemoryState, State,
    },
    util::serial::{serialize, Encodable, SerialDecodable, SerialEncodable},
    Result,
};

/// `2 * DELTA` represents epoch time
pub const DELTA: u64 = 30;

/// This struct represents the information required by the consensus algorithm
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
    pub participants: BTreeMap<Address, Participant>,
    /// Validators to be added on the next epoch as participants
    pub pending_participants: Vec<Participant>,
    /// Last slot participants where refreshed
    pub refreshed: u64,
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
            participants: BTreeMap::new(),
            pending_participants: vec![],
            refreshed: 0,
        })
    }
}

/// Auxiliary structure used for consensus syncing.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusRequest {
    /// Validator wallet address
    pub address: Address,
}

impl net::Message for ConsensusRequest {
    fn name() -> &'static str {
        "consensusrequest"
    }
}

/// Auxiliary structure used for consensus syncing.
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
    /// Node wallet address
    pub address: Address,
    /// Secret key, to sign messages
    pub secret: SecretKey,
    /// Node public key
    pub public: PublicKey,
    /// Hot/Live data used by the consensus algorithm
    pub consensus: ConsensusState,
    /// Canonical (finalized) blockchain
    pub blockchain: Blockchain,
    /// Canonical state machine
    pub state_machine: Arc<Mutex<State>>,
    /// Client providing wallet access
    pub client: Arc<Client>,
    /// Pending transactions
    pub unconfirmed_txs: Vec<Tx>,
    /// Participating start epoch
    pub participating: Option<u64>,
}

impl ValidatorState {
    // TODO: Clock sync
    pub async fn new(
        db: &sled::Db, // <-- TODO: Avoid this with some wrapping, sled should only be in blockchain
        genesis_ts: Timestamp,
        genesis_data: blake3::Hash,
        client: Arc<Client>,
        cashier_pubkeys: Vec<PublicKey>,
        faucet_pubkeys: Vec<PublicKey>,
    ) -> Result<ValidatorStatePtr> {
        let secret = SecretKey::random(&mut OsRng);
        let public = PublicKey::from_secret(secret);
        let consensus = ConsensusState::new(genesis_ts, genesis_data)?;
        let blockchain = Blockchain::new(db, genesis_ts, genesis_data)?;
        let unconfirmed_txs = vec![];
        let participating = None;

        let address = client.wallet.get_default_address().await?;
        let state_machine = Arc::new(Mutex::new(State {
            tree: client.get_tree().await?,
            merkle_roots: blockchain.merkle_roots.clone(),
            nullifiers: blockchain.nullifiers.clone(),
            cashier_pubkeys,
            faucet_pubkeys,
            mint_vk: Lazy::new(),
            burn_vk: Lazy::new(),
        }));

        let state = Arc::new(RwLock::new(ValidatorState {
            address,
            secret,
            public,
            consensus,
            blockchain,
            state_machine,
            client,
            unconfirmed_txs,
            participating,
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

    /// Set participating epoch to next.
    pub fn set_participating(&mut self) -> Result<()> {
        self.participating = Some(self.current_epoch() + 1);
        Ok(())
    }

    /// Find epoch leader, using a simple hash method.
    /// Leader calculation is based on how many nodes are participating
    /// in the network.
    pub fn epoch_leader(&mut self) -> Address {
        let epoch = self.current_epoch();
        // DefaultHasher is used to hash the epoch number
        // because it produces a number string which then can be modulated by the len.
        // blake3 produces alphanumeric
        let mut hasher = DefaultHasher::new();
        epoch.hash(&mut hasher);
        let pos = hasher.finish() % (self.consensus.participants.len() as u64);
        // Since BTreeMap orders by key in asceding order, each node will have
        // the same key in calculated position.
        self.consensus.participants.iter().nth(pos as usize).unwrap().1.address
    }

    /// Check if we're the current epoch leader
    pub fn is_epoch_leader(&mut self) -> bool {
        let address = self.address;
        address == self.epoch_leader()
    }

    /// Generate a block proposal for the current epoch, containing all
    /// unconfirmed transactions. Proposal extends the longest notarized fork
    /// chain the node is holding.
    pub fn propose(&self) -> Result<Option<BlockProposal>> {
        let epoch = self.current_epoch();
        let (prev_hash, index) = self.longest_notarized_chain_last_hash().unwrap();
        let unproposed_txs = self.unproposed_txs(index);

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
            self.address,
            prev_hash,
            epoch,
            unproposed_txs,
            metadata,
            sm,
        )))
    }

    /// Retrieve all unconfirmed transactions not proposed in previous blocks
    /// of provided index chain.
    pub fn unproposed_txs(&self, index: i64) -> Vec<Tx> {
        let mut unproposed_txs = self.unconfirmed_txs.clone();

        // If index is -1 (canonical blockchain) a new fork will be generated,
        // therefore all unproposed transactions can be included in the proposal.
        if index == -1 {
            return unproposed_txs
        }

        // We iterate over the fork chain proposals to find already proposed
        // transactions and remove them from the local unproposed_txs vector.
        let chain = &self.consensus.proposals[index as usize];
        for proposal in &chain.proposals {
            for tx in &proposal.block.txs {
                if let Some(pos) = unproposed_txs.iter().position(|txs| *txs == *tx) {
                    unproposed_txs.remove(pos);
                }
            }
        }

        unproposed_txs
    }

    /// Finds the longest fully notarized blockchain the node holds and
    /// returns the last block hash and the chain index.
    pub fn longest_notarized_chain_last_hash(&self) -> Result<(blake3::Hash, i64)> {
        let mut longest_notarized_chain: Option<ProposalChain> = None;
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

    /// Receive the proposed block, verify its sender (epoch leader),
    /// and proceed with voting on it.
    pub fn receive_proposal(&mut self, proposal: &BlockProposal) -> Result<Option<Vote>> {
        // Node hasn't started participating
        match self.participating {
            Some(start) => {
                if self.current_epoch() < start {
                    return Ok(None)
                }
            }
            None => return Ok(None),
        }

        // Node refreshes participants records
        self.refresh_participants()?;

        let leader = self.epoch_leader();
        if leader != proposal.address {
            warn!(
                "Received proposal not from epoch leader ({}), but from ({})",
                leader,
                proposal.address.to_string()
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
            warn!("Proposer ({}) signature could not be verified", proposal.address.to_string());
            return Ok(None)
        }

        self.vote(proposal)
    }

    /// Given a proposal, the node finds which blockchain it extends.
    /// If the proposal extends the canonical blockchain, a new fork chain
    /// is created. The node votes on the proposal only if it extends the
    /// longest notarized fork chain it has seen.
    pub fn vote(&mut self, proposal: &BlockProposal) -> Result<Option<Vote>> {
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
        Ok(Some(Vote::new(
            self.public,
            signed_hash,
            proposal_hash,
            proposal.block.sl,
            self.address,
        )))
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
    pub fn find_extended_chain_index(&mut self, proposal: &BlockProposal) -> Result<i64> {
        let mut fork = None;
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

            if proposal.block.st == last.block.st && proposal.block.sl > last.block.sl {
                fork = Some(chain.clone());
            }
        }

        match fork {
            Some(mut chain) => {
                debug!("Proposal to fork a forkchain was received.");
                chain.proposals.pop(); // removing last block to create the fork
                if !chain.proposals.is_empty() {
                    // if len is 0 we will verify against blockchain last block
                    self.consensus.proposals.push(chain);
                    return Ok(self.consensus.proposals.len() as i64 - 1)
                }
            }
            None => (),
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
    pub async fn receive_vote(&mut self, vote: &Vote) -> Result<(bool, Option<Vec<BlockInfo>>)> {
        let current_epoch = self.current_epoch();
        // Node hasn't started participating
        match self.participating {
            Some(start) => {
                if current_epoch < start {
                    return Ok((false, None))
                }
            }
            None => return Ok((false, None)),
        }

        let mut encoded_proposal = vec![];

        match vote.proposal.encode(&mut encoded_proposal) {
            Ok(_) => (),
            Err(e) => {
                error!(target: "consensus", "Proposal encoding failed: {:?}", e);
                return Ok((false, None))
            }
        };

        if !vote.public_key.verify(&encoded_proposal, &vote.vote) {
            warn!(target: "consensus", "Voter ({}), signature couldn't be verified", vote.address.to_string());
            return Ok((false, None))
        }

        // Node refreshes participants records
        self.refresh_participants()?;

        let node_count = self.consensus.participants.len();

        // Checking that the voter can actually vote.
        match self.consensus.participants.get(&vote.address) {
            Some(participant) => {
                let mut participant = participant.clone();
                if current_epoch <= participant.joined {
                    warn!(target: "consensus", "Voter ({}) joined after current epoch.", vote.address.to_string());
                    return Ok((false, None))
                }

                // Updating participant vote
                match participant.voted {
                    Some(voted) => {
                        if vote.sl > voted {
                            participant.voted = Some(vote.sl);
                        }
                    }
                    None => participant.voted = Some(vote.sl),
                }

                self.consensus.participants.insert(participant.address, participant);
            }
            None => {
                warn!(target: "consensus", "Voter ({}) is not a participant!", vote.address.to_string());
                return Ok((false, None))
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
            debug!(target: "consensus", "Received vote for unknown proposal.");
            if !self.consensus.orphan_votes.contains(vote) {
                self.consensus.orphan_votes.push(vote.clone());
            }

            return Ok((false, None))
        }

        let (proposal, chain_idx) = proposal.unwrap();
        if proposal.block.sm.votes.contains(vote) {
            debug!("receive_vote(): Already seen this vote");
            return Ok((false, None))
        }

        proposal.block.sm.votes.push(vote.clone());

        let mut to_broadcast = vec![];
        if !proposal.block.sm.notarized && proposal.block.sm.votes.len() > (2 * node_count / 3) {
            debug!("receive_vote(): Notarized a block");
            proposal.block.sm.notarized = true;
            match self.chain_finalization(chain_idx).await {
                Ok(v) => {
                    to_broadcast = v;
                }
                Err(e) => {
                    error!(target: "consensus", "Block finalization failed: {}", e);
                    return Err(e)
                }
            }
        }

        Ok((true, Some(to_broadcast)))
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
    pub async fn chain_finalization(&mut self, chain_index: i64) -> Result<Vec<BlockInfo>> {
        let chain = &mut self.consensus.proposals[chain_index as usize];

        if chain.proposals.len() < 3 {
            debug!(
                "chain_finalization(): Less than 3 proposals in chain {}, nothing to finalize",
                chain_index
            );
            return Ok(vec![])
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
            return Ok(vec![])
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
            // TODO: Is this the right place? We're already doing this in protocol_sync.
            // TODO: These state transitions have already been checked.
            let canon_state_clone = self.state_machine.lock().await.clone();
            let mem_st = MemoryState::new(canon_state_clone);
            let state_updates = ValidatorState::validate_state_transitions(mem_st, &proposal.txs)?;
            self.update_canon_state(state_updates, None).await?;

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

        Ok(finalized)
    }

    /// Append a new participant to the pending participants list.
    pub fn append_participant(&mut self, participant: Participant) -> bool {
        if self.consensus.pending_participants.contains(&participant) {
            return false
        }

        self.consensus.pending_participants.push(participant);
        true
    }

    /// Refresh the participants map, to retain only the active ones.
    /// Active nodes are considered those that joined previous epoch
    /// or on the epoch the last proposal was generated, either voted
    /// or joined the previous of that epoch. That ensures we cover
    /// the case of a node joining while the chosen epoch leader is inactive.
    pub fn refresh_participants(&mut self) -> Result<()> {
        // Node checks if it should refresh its participants list
        let epoch = self.current_epoch();
        if epoch <= self.consensus.refreshed {
            debug!("refresh_participants(): Participants have been refreshed this epoch.");
            return Ok(())
        }

        debug!("refresh_participants(): Adding pending participants");
        for participant in &self.consensus.pending_participants {
            self.consensus.participants.insert(participant.address, participant.clone());
        }

        if self.consensus.pending_participants.is_empty() {
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

        let previous_epoch = epoch - 1;
        let previous_from_last_epoch = last_epoch - 1;

        debug!(
            "refresh_participants(): Node {:?} checking epochs: previous - {:?}, last - {:?}, previous from last - {:?}",
            self.address.to_string(), previous_epoch, last_epoch, previous_from_last_epoch
        );

        for (index, participant) in self.consensus.participants.clone().iter() {
            match participant.voted {
                Some(epoch) => {
                    if epoch < last_epoch {
                        warn!(
                            "refresh_participants(): Inactive participant: {:?} (joined {:?}, voted {:?})",
                            participant.address.to_string(),
                            participant.joined,
                            participant.voted
                        );
                        inactive.push(*index);
                    }
                }
                None => {
                    if (previous_epoch == last_epoch && participant.joined < previous_epoch) ||
                        (previous_epoch != last_epoch &&
                            participant.joined < previous_from_last_epoch)
                    {
                        warn!(
                            "refresh_participants(): Inactive participant: {:?} (joined {:?}, voted {:?})",
                            participant.address.to_string(),
                            participant.joined,
                            participant.voted
                        );
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
            let participant = Participant::new(self.address, self.current_epoch());
            self.consensus.participants.insert(participant.address, participant);
        }

        self.consensus.refreshed = epoch;

        Ok(())
    }

    /// Utility function to reset the current consensus state.
    pub fn reset_consensus_state(&mut self) -> Result<()> {
        let genesis_ts = self.consensus.genesis_ts;
        let genesis_block = self.consensus.genesis_block;

        let consensus = ConsensusState {
            genesis_ts,
            genesis_block,
            proposals: vec![],
            orphan_votes: vec![],
            participants: BTreeMap::new(),
            pending_participants: vec![],
            refreshed: 0,
        };

        self.consensus = consensus;
        Ok(())
    }

    // ==========================
    // State transition functions
    // ==========================

    /// Validate state transitions for given transactions and state and
    /// return a vector of [`StateUpdate`]
    pub fn validate_state_transitions(state: MemoryState, txs: &[Tx]) -> Result<Vec<StateUpdate>> {
        let mut ret = vec![];
        let mut st = state;

        for (i, tx) in txs.iter().enumerate() {
            let update = match state_transition(&st, tx.0.clone()) {
                Ok(v) => v,
                Err(e) => {
                    warn!("validate_state_transition(): Failed for tx {}: {}", i, e);
                    return Err(e.into())
                }
            };
            st.apply(update.clone());
            ret.push(update);
        }

        Ok(ret)
    }

    /// Apply a vector of [`StateUpdate`] to the canonical state.
    pub async fn update_canon_state(
        &self,
        updates: Vec<StateUpdate>,
        notify: Option<async_channel::Sender<(PublicKey, u64)>>,
    ) -> Result<()> {
        let secret_keys: Vec<SecretKey> =
            self.client.get_keypairs().await?.iter().map(|x| x.secret).collect();

        debug!("update_canon_state(): Acquiring state machine lock");
        let mut state = self.state_machine.lock().await;
        for update in updates {
            state
                .apply(
                    update,
                    secret_keys.clone(),
                    notify.clone(),
                    self.client.wallet.clone(),
                    self.client.tokenlist.clone(),
                )
                .await?;
        }
        drop(state);
        debug!("update_canon_state(): Dropped state machine lock");

        debug!("update_canon_state(): Successfully applied state updates");
        Ok(())
    }
}
