// TODO: Use sets instead of vectors where possible.
use std::{
    collections::{hash_map::DefaultHasher, BTreeMap},
    hash::{Hash, Hasher},
    time::Duration,
};

use async_std::sync::{Arc, Mutex, RwLock};
use chrono::{NaiveDateTime, Utc};
use darkfi_serial::{serialize, SerialDecodable, SerialEncodable};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use lazy_init::Lazy;
use log::{debug, error, info, warn};
use rand::rngs::OsRng;

use super::{
    Block, BlockInfo, BlockProposal, Header, KeepAlive, LeadProof, Metadata, Participant,
    ProposalChain,
};

use crate::{
    blockchain::Blockchain,
    crypto::{
        address::Address,
        constants::MERKLE_DEPTH,
        keypair::{PublicKey, SecretKey},
        merkle_node::MerkleNode,
        schnorr::{SchnorrPublic, SchnorrSecret},
    },
    net,
    node::{
        state::{state_transition, ProgramState, StateUpdate},
        Client, MemoryState, State,
    },
    tx::Transaction,
    util::time::Timestamp,
    Result,
};

/// `2 * DELTA` represents slot time
pub const DELTA: u64 = 20;
/// Slots in an epoch
pub const EPOCH_SLOTS: u64 = 10;
/// Quarantine duration, in slots
pub const QUARANTINE_DURATION: u64 = 5;

/// This struct represents the information required by the consensus algorithm
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ConsensusState {
    /// Genesis block creation timestamp
    pub genesis_ts: Timestamp,
    /// Genesis block hash
    pub genesis_block: blake3::Hash,
    /// Fork chains containing block proposals
    pub proposals: Vec<ProposalChain>,
    /// Validators currently participating in the consensus
    pub participants: BTreeMap<Address, Participant>,
    /// Validators to be added on the next slot as participants
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
    pub unconfirmed_txs: Vec<Transaction>,
    /// Participating start slot
    pub participating: Option<u64>,
}

impl ValidatorState {
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

        // Create zk proof verification keys
        let _ = state_machine.lock().await.mint_vk();
        let _ = state_machine.lock().await.burn_vk();

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

    /// The node retrieves a transaction, validates its state transition,
    /// and appends it to the unconfirmed transactions list.
    pub async fn append_tx(&mut self, tx: Transaction) -> bool {
        let tx_hash = blake3::hash(&serialize(&tx));
        let tx_in_txstore = match self.blockchain.transactions.contains(&tx_hash) {
            Ok(v) => v,
            Err(e) => {
                error!("append_tx(): Failed querying txstore: {}", e);
                return false
            }
        };

        if self.unconfirmed_txs.contains(&tx) || tx_in_txstore {
            debug!("append_tx(): We have already seen this tx.");
            return false
        }

        debug!("append_tx(): Starting state transition validation");
        let canon_state_clone = self.state_machine.lock().await.clone();
        let mem_state = MemoryState::new(canon_state_clone);
        match Self::validate_state_transitions(mem_state, &[tx.clone()]) {
            Ok(_) => debug!("append_tx(): State transition valid"),
            Err(e) => {
                warn!("append_tx(): State transition fail: {}", e);
                return false
            }
        }

        debug!("append_tx(): Appended tx to mempool");
        self.unconfirmed_txs.push(tx);
        true
    }

    /// Calculates the epoch of the provided slot.
    /// Epoch duration is configured using the `EPOCH_SLOTS` value.
    pub fn slot_epoch(&self, slot: u64) -> u64 {
        slot / EPOCH_SLOTS
    }

    /// Calculates current slot, based on elapsed time from the genesis block.
    /// Slot duration is configured using the `DELTA` value.
    pub fn current_slot(&self) -> u64 {
        self.consensus.genesis_ts.elapsed() / (2 * DELTA)
    }

    /// Finds the last slot a proposal or block was generated.
    pub fn last_slot(&self) -> Result<u64> {
        let mut slot = 0;
        for chain in &self.consensus.proposals {
            for proposal in &chain.proposals {
                if proposal.block.header.slot > slot {
                    slot = proposal.block.header.slot;
                }
            }
        }

        // We return here in case proposals exist,
        // so we don't query the sled database.
        if slot > 0 {
            return Ok(slot)
        }

        let (last_slot, _) = self.blockchain.last()?;
        Ok(last_slot)
    }

    /// Calculates seconds until next Nth slot starting time.
    /// Slots duration is configured using the delta value.
    pub fn next_n_slot_start(&self, n: u64) -> Duration {
        let start_time = NaiveDateTime::from_timestamp(self.consensus.genesis_ts.0, 0);
        let current_slot = self.current_slot() + n;
        let next_slot_start = (current_slot * (2 * DELTA)) + (start_time.timestamp() as u64);
        let next_slot_start = NaiveDateTime::from_timestamp(next_slot_start as i64, 0);
        let current_time = NaiveDateTime::from_timestamp(Utc::now().timestamp(), 0);
        let diff = next_slot_start - current_time;

        Duration::new(diff.num_seconds().try_into().unwrap(), 0)
    }

    /// Set participating slot to next.
    pub fn set_participating(&mut self) -> Result<()> {
        self.participating = Some(self.current_slot() + 1);
        Ok(())
    }

    /// Find slot leader, using a simple hash method.
    /// Leader calculation is based on how many nodes are participating
    /// in the network.
    pub fn slot_leader(&mut self) -> Participant {
        let slot = self.current_slot();
        // DefaultHasher is used to hash the slot number
        // because it produces a number string which then can be modulated by the len.
        // blake3 produces alphanumeric
        let mut hasher = DefaultHasher::new();
        slot.hash(&mut hasher);
        let pos = hasher.finish() % (self.consensus.participants.len() as u64);
        // Since BTreeMap orders by key in asceding order, each node will have
        // the same key in calculated position.
        self.consensus.participants.iter().nth(pos as usize).unwrap().1.clone()
    }

    /// Check if we're the current slot leader
    pub fn is_slot_leader(&mut self) -> bool {
        let address = self.address;
        address == self.slot_leader().address
    }

    /// Generate a block proposal for the current slot, containing all
    /// unconfirmed transactions. Proposal extends the longest fork
    /// chain the node is holding.
    pub fn propose(&self) -> Result<Option<BlockProposal>> {
        let slot = self.current_slot();
        let (prev_hash, index) = self.longest_chain_last_hash().unwrap();
        let unproposed_txs = self.unproposed_txs(index);

        let mut tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100);
        for tx in &unproposed_txs {
            for output in &tx.outputs {
                tree.append(&MerkleNode::from_coin(&output.revealed.coin));
                tree.witness();
            }
        }
        let root = tree.root(0).unwrap();
        let header =
            Header::new(prev_hash, self.slot_epoch(slot), slot, Timestamp::current_time(), root);

        let signed_proposal = self.secret.sign(&header.headerhash().as_bytes()[..]);
        // TODO: Replace with correct proof
        let eta: [u8; 32] = *blake3::hash(b"let there be dark!").as_bytes();
        let proof = LeadProof::default();
        let participants = self.consensus.participants.values().cloned().collect();
        let metadata = Metadata::new(signed_proposal, self.address, eta, proof, participants);

        // TODO: [PLACEHOLDER] Add balance proof creation
        // TODO: [PLACEHOLDER] Add crypsinous leader proof creation (to replace balance proof)
        // TODO: [PLACEHOLDER] Add rewards calculation (proof?)
        // TODO: [PLACEHOLDER] Create and add rewards transaction
        Ok(Some(BlockProposal::new(header, unproposed_txs, metadata)))
    }

    /// Retrieve all unconfirmed transactions not proposed in previous blocks
    /// of provided index chain.
    pub fn unproposed_txs(&self, index: i64) -> Vec<Transaction> {
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

    /// Finds the longest blockchain the node holds and
    /// returns the last block hash and the chain index.
    pub fn longest_chain_last_hash(&self) -> Result<(blake3::Hash, i64)> {
        let mut longest: Option<ProposalChain> = None;
        let mut length = 0;
        let mut index = -1;

        if !self.consensus.proposals.is_empty() {
            for (i, chain) in self.consensus.proposals.iter().enumerate() {
                if chain.proposals.len() > length {
                    longest = Some(chain.clone());
                    length = chain.proposals.len();
                    index = i as i64;
                }
            }
        }

        let hash = match longest {
            Some(chain) => chain.proposals.last().unwrap().block.header.headerhash(),
            None => self.blockchain.last()?.1,
        };

        Ok((hash, index))
    }

    /// Given a proposal, the node verify its sender (slot leader), finds which blockchain
    /// it extends and check if it can be finalized. If the proposal extends
    /// the canonical blockchain, a new fork chain is created.
    pub async fn receive_proposal(
        &mut self,
        proposal: &BlockProposal,
    ) -> Result<Option<Vec<BlockInfo>>> {
        let current = self.current_slot();
        // Node hasn't started participating
        match self.participating {
            Some(start) => {
                if current < start {
                    return Ok(None)
                }
            }
            None => return Ok(None),
        }

        // Node refreshes participants records
        self.refresh_participants()?;

        let mut leader = self.slot_leader();
        if leader.address != proposal.block.metadata.address {
            warn!(
                "Received proposal not from slot leader ({}), but from ({})",
                leader.address, proposal.block.metadata.address
            );
            return Ok(None)
        }

        if !leader.public_key.verify(
            proposal.block.header.headerhash().as_bytes(),
            &proposal.block.metadata.signature,
        ) {
            warn!("Proposer ({}) signature could not be verified", proposal.block.metadata.address);
            return Ok(None)
        }

        debug!("receive_proposal(): Starting state transition validation");
        let canon_state_clone = self.state_machine.lock().await.clone();
        let mem_state = MemoryState::new(canon_state_clone);

        match Self::validate_state_transitions(mem_state, &proposal.block.txs) {
            Ok(_) => {
                debug!("receive_proposal(): State transition valid")
            }
            Err(e) => {
                warn!("receive_proposal(): State transition fail: {}", e);
                return Ok(None)
            }
        }

        // TODO: [PLACEHOLDER] Add balance proof validation
        // TODO: [PLACEHOLDER] Add crypsinous proof validation (to replace balance proof)
        // TODO: [PLACEHOLDER] Add rewards validation

        if current > leader.seen {
            leader.seen = current;
        }

        // Invalidating quarantine
        leader.quarantined = None;

        self.consensus.participants.insert(leader.address, leader);

        let index = self.find_extended_chain_index(&proposal)?;

        if index == -2 {
            return Ok(None)
        }

        let mut to_broadcast = vec![];
        match index {
            -1 => {
                let pc = ProposalChain::new(self.consensus.genesis_block, proposal.clone());
                self.consensus.proposals.push(pc);
            }
            _ => {
                self.consensus.proposals[index as usize].add(&proposal);
                match self.chain_finalization(index).await {
                    Ok(v) => {
                        to_broadcast = v;
                    }
                    Err(e) => {
                        error!("consensus: Block finalization failed: {}", e);
                        return Err(e)
                    }
                }
            }
        };

        Ok(Some(to_broadcast))
    }

    /// Given a proposal, find the index of the chain it extends.
    pub fn find_extended_chain_index(&mut self, proposal: &BlockProposal) -> Result<i64> {
        let mut fork = None;
        for (index, chain) in self.consensus.proposals.iter().enumerate() {
            let last = chain.proposals.last().unwrap();
            let hash = last.block.header.headerhash();
            if proposal.block.header.previous == hash &&
                proposal.block.header.slot > last.block.header.slot
            {
                return Ok(index as i64)
            }

            if proposal.block.header.previous == last.block.header.previous &&
                proposal.block.header.slot == last.block.header.slot
            {
                debug!("find_extended_chain_index(): Proposal already received");
                return Ok(-2)
            }

            if proposal.block.header.previous == last.block.header.previous &&
                proposal.block.header.slot > last.block.header.slot
            {
                fork = Some(chain.clone());
            }
        }

        if let Some(mut chain) = fork {
            debug!("Proposal to fork a forkchain was received.");
            chain.proposals.pop(); // removing last block to create the fork
            if !chain.proposals.is_empty() {
                // if len is 0 we will verify against blockchain last block
                self.consensus.proposals.push(chain);
                return Ok(self.consensus.proposals.len() as i64 - 1)
            }
        }

        let (last_slot, last_block) = self.blockchain.last()?;
        if proposal.block.header.previous != last_block || proposal.block.header.slot <= last_slot {
            debug!("find_extended_chain_index(): Proposal doesn't extend any known chain");
            return Ok(-2)
        }

        Ok(-1)
    }

    /// Search the chains we're holding for the given proposal.
    pub fn find_proposal(
        &mut self,
        input_proposal: &blake3::Hash,
    ) -> Result<Option<(&mut BlockProposal, i64)>> {
        for (index, chain) in &mut self.consensus.proposals.iter_mut().enumerate() {
            for proposal in chain.proposals.iter_mut().rev() {
                let proposal_hash = proposal.block.header.headerhash();
                if input_proposal == &proposal_hash {
                    return Ok(Some((proposal, index as i64)))
                }
            }
        }

        Ok(None)
    }

    /// Remove provided transactions vector from unconfirmed_txs if they exist.
    pub fn remove_txs(&mut self, transactions: Vec<Transaction>) -> Result<()> {
        for tx in transactions {
            if let Some(pos) = self.unconfirmed_txs.iter().position(|txs| *txs == tx) {
                self.unconfirmed_txs.remove(pos);
            }
        }

        Ok(())
    }

    /// Provided an index, the node checks if the chain can be finalized.
    /// Consensus finalization logic:
    /// - If the node has observed the creation of 3 proposals in a fork chain,
    ///   it finalizes (appends to canonical blockchain) all proposals up to the last one.
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

        let bound = chain.proposals.len() - 1;
        let mut finalized = vec![];
        for proposal in &mut chain.proposals[..bound] {
            finalized.push(proposal.clone().into());
        }

        chain.proposals.drain(0..bound);

        info!("consensus: Adding {} finalized block to canonical chain.", finalized.len());
        let blockhashes = match self.blockchain.add(&finalized) {
            Ok(v) => v,
            Err(e) => {
                error!("consensus: Failed appending finalized blocks to canonical chain: {}", e);
                return Err(e)
            }
        };

        for proposal in &finalized {
            // TODO: Is this the right place? We're already doing this in protocol_sync.
            // TODO: These state transitions have already been checked.
            debug!(target: "consensus", "Applying state transition for finalized block");
            let canon_state_clone = self.state_machine.lock().await.clone();
            let mem_st = MemoryState::new(canon_state_clone);
            let state_updates = Self::validate_state_transitions(mem_st, &proposal.txs)?;
            self.update_canon_state(state_updates, None).await?;
            self.remove_txs(proposal.txs.clone())?;
        }

        let last_block = *blockhashes.last().unwrap();
        let last_slot = finalized.last().unwrap().header.slot;

        let mut dropped = vec![];
        for chain in self.consensus.proposals.iter() {
            let first = chain.proposals.first().unwrap();
            if first.block.header.previous != last_block || first.block.header.slot <= last_slot {
                dropped.push(chain.clone());
            }
        }

        for chain in dropped {
            self.consensus.proposals.retain(|c| *c != chain);
        }

        Ok(finalized)
    }

    /// Append a new participant to the pending participants list.
    pub fn append_participant(&mut self, participant: Participant) -> bool {
        if self.consensus.pending_participants.contains(&participant) {
            return false
        }

        // TODO: [PLACEHOLDER] Add balance proof validation

        self.consensus.pending_participants.push(participant);
        true
    }

    /// Update participant seen.
    pub fn participant_keep_alive(&mut self, keep_alive: KeepAlive) -> bool {
        match self.consensus.participants.get(&keep_alive.address) {
            None => {
                warn!(
                    "Keep alive message from unknown participant: {}",
                    keep_alive.address.to_string()
                );
                false
            }
            Some(participant) => {
                let current = self.current_slot();
                if current != keep_alive.slot {
                    warn!("keep alive message slot is not current one for: {}", keep_alive.address);
                    return false
                }

                let serialized = serialize(&current);
                if !participant.public_key.verify(&serialized, &keep_alive.signature) {
                    warn!(
                        "Keep alive message signature could not be verified for: {}",
                        keep_alive.address
                    );
                    return false
                }

                // TODO: [PLACEHOLDER] Add balance proof validation

                // Updating participant last seen slot
                let mut participant = participant.clone();
                participant.seen = current;

                // Invalidating quarantine
                participant.quarantined = None;

                self.consensus.participants.insert(participant.address, participant);

                true
            }
        }
    }

    /// Refresh the participants map, to retain only the active ones.
    /// Active nodes are considered those that their last seen slot is
    /// in range: [current_slot - QUARANTINE_DURATION, current_slot]
    /// Inactive nodes are marked as quarantined, so they can be removed if
    /// they are in quarantine more than the predifined quarantine period.
    pub fn refresh_participants(&mut self) -> Result<()> {
        // Node checks if it should refresh its participants list
        let current = self.current_slot();
        if current <= self.consensus.refreshed {
            debug!("refresh_participants(): Participants have been refreshed this slot.");
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
        let low_bound = current - QUARANTINE_DURATION;

        debug!(
            "refresh_participants(): Node {} checking slots range: {} -> {}",
            self.address, low_bound, current
        );

        let leader = self.slot_leader();
        for (index, participant) in self.consensus.participants.iter_mut() {
            match participant.quarantined {
                Some(slot) => {
                    if slot < low_bound {
                        warn!(
                            "refresh_participants(): Removing participant: {} (seen {}, quarantined {})",
                            participant.address,
                            participant.seen,
                            slot
                        );
                        inactive.push(*index);
                    }
                }
                None => {
                    // Slot leader is always quarantined, to cover the case they become inactive the slot before
                    // becoming the leader. This can be used for slashing in the future.
                    if participant.address == leader.address {
                        debug!(
                            "refresh_participants(): Quaranteening leader: {} (seen {})",
                            participant.address, participant.seen
                        );
                        participant.quarantined = Some(current);
                        continue
                    }
                    if participant.seen < low_bound {
                        warn!(
                            "refresh_participants(): Quaranteening participant: {} (seen {})",
                            participant.address, participant.seen
                        );
                        participant.quarantined = Some(current);
                    }
                }
            }
        }

        for index in inactive {
            self.consensus.participants.remove(&index);
        }

        if self.consensus.participants.is_empty() {
            // If no nodes are active, node becomes a single node network.
            let participant = Participant::new(self.public, self.address, self.current_slot());
            self.consensus.participants.insert(participant.address, participant);
        }

        self.consensus.refreshed = current;

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

    /// Validate and append to canonical state received blocks.
    pub async fn receive_blocks(&mut self, blocks: &[BlockInfo]) -> Result<()> {
        // Verify state transitions for all blocks and their respective transactions.
        debug!("receive_blocks(): Starting state transition validations");
        let mut canon_updates = vec![];
        let canon_state_clone = self.state_machine.lock().await.clone();
        let mut mem_state = MemoryState::new(canon_state_clone);
        for block in blocks {
            let mut state_updates =
                Self::validate_state_transitions(mem_state.clone(), &block.txs)?;

            for update in &state_updates {
                mem_state.apply(update.clone());
            }

            canon_updates.append(&mut state_updates);
        }
        debug!("receive_blocks(): All state transitions passed");

        debug!("receive_blocks(): Updating canon state");
        self.update_canon_state(canon_updates, None).await?;

        debug!("receive_blocks(): Appending blocks to ledger");
        self.blockchain.add(blocks)?;

        Ok(())
    }

    /// Validate and append to canonical state received finalized block.
    /// Returns boolean flag indicating already existing block.
    pub async fn receive_finalized_block(&mut self, block: BlockInfo) -> Result<bool> {
        match self.blockchain.has_block(&block) {
            Ok(v) => {
                if v {
                    debug!("receive_finalized_block(): Existing block received");
                    return Ok(false)
                }
            }
            Err(e) => {
                error!("receive_finalized_block(): failed checking for has_block(): {}", e);
                return Ok(false)
            }
        };

        debug!("receive_finalized_block(): Executing state transitions");
        self.receive_blocks(&[block.clone()]).await?;

        debug!("receive_finalized_block(): Removing block transactions from unconfirmed_txs");
        self.remove_txs(block.txs.clone())?;

        Ok(true)
    }

    /// Validate and append to canonical state received finalized blocks from block sync task.
    /// Already existing blocks are ignored.
    pub async fn receive_sync_blocks(&mut self, blocks: &[BlockInfo]) -> Result<()> {
        let mut new_blocks = vec![];
        for block in blocks {
            match self.blockchain.has_block(block) {
                Ok(v) => {
                    if v {
                        debug!("receive_sync_blocks(): Existing block received");
                        continue
                    }
                    new_blocks.push(block.clone());
                }
                Err(e) => {
                    error!("receive_sync_blocks(): failed checking for has_block(): {}", e);
                    continue
                }
            };
        }

        if new_blocks.is_empty() {
            debug!("receive_sync_blocks(): no new blocks to append");
            return Ok(())
        }

        debug!("receive_sync_blocks(): Executing state transitions");
        self.receive_blocks(&new_blocks[..]).await?;

        Ok(())
    }

    /// Validate state transitions for given transactions and state and
    /// return a vector of [`StateUpdate`]
    pub fn validate_state_transitions(
        state: MemoryState,
        txs: &[Transaction],
    ) -> Result<Vec<StateUpdate>> {
        let mut ret = vec![];
        let mut st = state;

        for (i, tx) in txs.iter().enumerate() {
            let update = match state_transition(&st, tx.clone()) {
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
        notify: Option<smol::channel::Sender<(PublicKey, u64)>>,
    ) -> Result<()> {
        let secret_keys: Vec<SecretKey> =
            self.client.get_keypairs().await?.iter().map(|x| x.secret).collect();

        debug!("update_canon_state(): Acquiring state machine lock");
        let mut state = self.state_machine.lock().await;
        for update in updates {
            state
                .apply(update, secret_keys.clone(), notify.clone(), self.client.wallet.clone())
                .await?;
        }
        drop(state);
        debug!("update_canon_state(): Dropped state machine lock");

        debug!("update_canon_state(): Successfully applied state updates");
        Ok(())
    }
}
