/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

// TODO: Use sets instead of vectors where possible.
use std::{
    collections::{hash_map::DefaultHasher, BTreeMap},
    hash::{Hash, Hasher},
    time::Duration,
};

use async_std::sync::{Arc, Mutex, RwLock};
use chrono::{NaiveDateTime, Utc};
use darkfi_sdk::crypto::{
    constants::MERKLE_DEPTH,
    schnorr::{SchnorrPublic, SchnorrSecret},
    Address, ContractId, MerkleNode, PublicKey, SecretKey,
};
use darkfi_serial::{serialize, SerialDecodable, SerialEncodable};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use lazy_init::Lazy;
use log::{debug, error, info, warn};
use pasta_curves::{group::ff::PrimeField, pallas};
use rand::rngs::OsRng;

use super::{
    coins, leadcoin::LeadCoin, Block, BlockInfo, BlockProposal, Header, LeadProof, Metadata,
    Participant, ProposalChain, DELTA, EPOCH_LENGTH, LEADER_PROOF_K,
};

use crate::{
    blockchain::Blockchain,
    crypto::proof::{ProvingKey, VerifyingKey},
    net,
    node::{
        state::{state_transition, ProgramState, StateUpdate},
        Client, MemoryState, State,
    },
    runtime::vm_runtime::Runtime,
    tx::Transaction,
    util::time::Timestamp,
    zk::circuit::LeadContract,
    Error, Result,
};

/// This struct represents the information required by the consensus algorithm
#[derive(Debug)]
pub struct ConsensusState {
    /// Genesis block creation timestamp
    pub genesis_ts: Timestamp,
    /// Genesis block hash
    pub genesis_block: blake3::Hash,
    /// Fork chains containing block proposals
    pub proposals: Vec<ProposalChain>,
    /// Validators currently participating in the consensus
    pub participants: BTreeMap<Address, Participant>,
    /// Last slot participants where refreshed
    pub refreshed: u64,
    /// Current epoch
    pub epoch: u64,
    /// Current epoch eta
    pub epoch_eta: pallas::Base,
    /// Current epoch competing coins
    pub coins: Vec<Vec<LeadCoin>>,
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
            refreshed: 0,
            epoch: 0,
            epoch_eta: pallas::Base::one(),
            coins: vec![],
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
    pub proposals: Vec<ProposalChain>,
    pub participants: BTreeMap<Address, Participant>,
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
    /// Leader proof proving key
    pub proving_key: ProvingKey,
    /// Leader proof verifying key
    pub verifying_key: VerifyingKey,
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
        info!("Generating leader proof keys with k: {}", *LEADER_PROOF_K);
        let proving_key = ProvingKey::build(*LEADER_PROOF_K, &LeadContract::default());
        let verifying_key = VerifyingKey::build(*LEADER_PROOF_K, &LeadContract::default());
        let consensus = ConsensusState::new(genesis_ts, genesis_data)?;
        let blockchain = Blockchain::new(db, genesis_ts, genesis_data)?;
        let unconfirmed_txs = vec![];
        let participating = None;

        // -----BEGIN ARTIFACT: WASM INTEGRATION-----
        // This is the current place where this stuff is being done, and very loosely.
        // We initialize and "deploy" _native_ contracts here - currently the money contract.
        // Eventually, the crypsinous consensus should be a native contract like payments are.
        // This means the previously existing Blockchain state will be a bit different and is
        // going to have to be changed.
        // When the `Blockchain` object is created, it doesn't care whether it already has
        // data or not. If there's existing data it will just open the necessary db and trees,
        // and give back what it has. This means, on subsequent runs our native contracts will
        // already be in a deployed state. So what we do here is a "re-deployment". This kind
        // of operation should only modify the contract's state in case it wasn't deployed
        // before (meaning the initial run). Otherwise, it shouldn't touch anything, or just
        // potentially update the database schemas or whatever is necessary. Here it's
        // transparent and generic, and the entire logic for this db protection is supposed to
        // be in the `init` function of the contract, so look there for a reference of the
        // databases and the state.
        info!("ValidatorState::new(): Deploying \"money contract\"");
        let money_contract_wasm_bincode = include_bytes!("../contract/money/money_contract.wasm");
        // XXX: FIXME: This ID should be something that does not solve the pallas curve equation,
        //             and/or just hardcoded and forbidden in non-native contract deployment.
        let cid = ContractId::from(pallas::Base::from(u64::MAX - 420));
        let mut runtime = Runtime::new(&money_contract_wasm_bincode[..], blockchain.clone(), cid)?;
        runtime.deploy(&[])?;
        info!("Deployed Money Contract with ID: {}", cid);
        // -----END ARTIFACT-----

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
            proving_key,
            verifying_key,
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

    /// Calculates current epoch.
    pub fn current_epoch(&self) -> u64 {
        self.slot_epoch(self.current_slot())
    }

    /// Calculates the epoch of the provided slot.
    /// Epoch duration is configured using the `EPOCH_LENGTH` value.
    pub fn slot_epoch(&self, slot: u64) -> u64 {
        slot / *EPOCH_LENGTH
    }

    /// Calculates current slot, based on elapsed time from the genesis block.
    /// Slot duration is configured using the `DELTA` value.
    pub fn current_slot(&self) -> u64 {
        self.consensus.genesis_ts.elapsed() / (2 * *DELTA)
    }

    /// Calculates the relative number of the provided slot.
    pub fn relative_slot(&self, slot: u64) -> u64 {
        slot % *EPOCH_LENGTH
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
        assert!(n > 0);
        let start_time = NaiveDateTime::from_timestamp(self.consensus.genesis_ts.0, 0);
        let current_slot = self.current_slot() + n;
        let next_slot_start = (current_slot * (2 * *DELTA)) + (start_time.timestamp() as u64);
        let next_slot_start = NaiveDateTime::from_timestamp(next_slot_start as i64, 0);
        let current_time = NaiveDateTime::from_timestamp(Utc::now().timestamp(), 0);
        let diff = next_slot_start - current_time;

        Duration::new(diff.num_seconds().try_into().unwrap(), 0)
    }

    /// Calculate slots until next Nth epoch.
    /// Epoch duration is configured using the EPOCH_LENGTH value.
    pub fn slots_to_next_n_epoch(&self, n: u64) -> u64 {
        assert!(n > 0);
        let slots_till_next_epoch = *EPOCH_LENGTH - self.relative_slot(self.current_slot());
        ((n - 1) * *EPOCH_LENGTH) + slots_till_next_epoch
    }

    /// Calculates seconds until next Nth epoch starting time.
    pub fn next_n_epoch_start(&self, n: u64) -> Duration {
        self.next_n_slot_start(self.slots_to_next_n_epoch(n))
    }

    /// Set participating slot to next.
    pub fn set_participating(&mut self) -> Result<()> {
        self.participating = Some(self.current_slot() + 1);
        Ok(())
    }

    /// Find slot leader, using a simple hash method.
    /// Leader calculation is based on how many nodes are participating
    /// in the network.
    /// Note: leaving this for future usage
    /// TODO: if not used, participants BTreeMap can become a HashSet
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

    /// Check if new epoch has started, to create new epoch coins.
    /// Returns flag to signify if epoch has changed and vector of
    /// new epoch competing coins.
    pub async fn epoch_changed(&mut self) -> Result<bool> {
        let epoch = self.current_epoch();
        if epoch <= self.consensus.epoch {
            return Ok(false)
        }
        let eta = self.get_eta();
        // Retrieving nodes wallet coins
        let owned = self.client.get_own_coins().await?;
        // TODO: slot parameter should be absolute slot, not relative.
        // At start of epoch, relative slot is 0.
        self.consensus.coins = coins::create_epoch_coins(eta, &owned, epoch, 0);
        self.consensus.epoch = epoch;
        self.consensus.epoch_eta = eta;
        Ok(true)
    }

    /// Wrapper for coins::is_leader
    pub fn is_slot_leader(&self) -> (bool, usize) {
        coins::is_leader(self.relative_slot(self.current_slot()), &self.consensus.coins)
    }

    /// Generate a block proposal for the current slot, containing all
    /// unconfirmed transactions. Proposal extends the longest fork
    /// chain the node is holding.
    pub fn propose(&mut self, idx: usize) -> Result<Option<BlockProposal>> {
        let slot = self.current_slot();
        let (prev_hash, index) = self.longest_chain_last_hash().unwrap();
        let unproposed_txs = self.unproposed_txs(index);

        let mut tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100);
        for tx in &unproposed_txs {
            for output in &tx.outputs {
                tree.append(&MerkleNode::from(output.revealed.coin.0));
                tree.witness();
            }
        }
        let root = tree.root(0).unwrap();
        let header =
            Header::new(prev_hash, self.slot_epoch(slot), slot, Timestamp::current_time(), root);

        let signed_proposal = self.secret.sign(&mut OsRng, &header.headerhash().as_bytes()[..]);
        let eta = self.consensus.epoch_eta.to_repr();
        // Generating leader proof
        let relative_slot = self.relative_slot(slot) as usize;
        let coin = self.consensus.coins[relative_slot][idx];
        // TODO: Generate new LeadCoin from newlly minted coin, will reuse original coin for now
        //let coin2 = something();
        let proof = coin.create_lead_proof(&self.proving_key)?;
        let participants = self.consensus.participants.values().cloned().collect();
        let metadata = Metadata::new(
            signed_proposal,
            self.address,
            coin.public_inputs(),
            coin.public_inputs(),
            idx,
            coin.sn,
            eta,
            LeadProof::from(proof),
            participants,
        );
        // TODO: replace old coin with new coin
        self.consensus.coins[relative_slot][idx] = coin;

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
            Some(chain) => chain.proposals.last().unwrap().header,
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

        // Check if leader is a known consensus participant
        let leader = self.consensus.participants.get(&proposal.block.metadata.address);
        if leader.is_none() {
            warn!(
                "receive_proposal(): Received proposal from unknown node: ({})",
                proposal.block.metadata.address
            );
            return Err(Error::UnknownNodeError)
        }
        let mut leader = leader.unwrap().clone();

        // Check if proposal header matches actual one
        let proposal_header = proposal.block.header.headerhash();
        if proposal.header != proposal_header {
            warn!(
                "receive_proposal(): Received proposal contains missmatched headers: {} - {}",
                proposal.header, proposal_header
            );
            return Err(Error::ProposalHeadersMissmatchError)
        }

        // Verify proposal winning coin public inputs match known ones
        let public_inputs = &leader.coins[self.relative_slot(current) as usize]
            [proposal.block.metadata.winning_index];
        if public_inputs != &proposal.block.metadata.public_inputs {
            warn!("receive_proposal(): Received proposal public inputs are invalid.");
            return Err(Error::InvalidPublicInputsError)
        }

        // TODO: Verify winning coin serial number

        // Verify proposal leader proof
        match proposal.block.metadata.proof.verify(&self.verifying_key, public_inputs) {
            Ok(_) => info!("receive_proposal(): Proof veryfied succsessfully!"),
            Err(e) => {
                error!("receive_proposal(): Error during leader proof verification: {}", e);
                return Err(Error::LeaderProofVerificationError)
            }
        }

        // Verify proposal signature is valid based on leader known valid key
        if !leader.public_key.verify(proposal.header.as_bytes(), &proposal.block.metadata.signature)
        {
            warn!(
                "receive_proposal(): Proposer ({}) signature could not be verified",
                proposal.block.metadata.address
            );
            return Err(Error::InvalidSignature)
        }

        // Check if proposal extends any existing fork chains
        let index = self.find_extended_chain_index(proposal)?;
        if index == -2 {
            return Err(Error::ExtendedChainIndexNotFoundError)
        }

        // Validate state transition against canonical state
        // TODO: This should be validated against fork state
        debug!("receive_proposal(): Starting state transition validation");
        let canon_state_clone = self.state_machine.lock().await.clone();
        let mem_state = MemoryState::new(canon_state_clone);

        match Self::validate_state_transitions(mem_state, &proposal.block.txs) {
            Ok(_) => {
                debug!("receive_proposal(): State transition valid")
            }
            Err(e) => {
                warn!("receive_proposal(): State transition fail: {}", e);
                return Err(Error::StateTransitionError)
            }
        }

        // TODO: [PLACEHOLDER] Add rewards validation
        // TODO: Append serial to merkle tree

        // Replacing participants public inputs with the newlly minted ones
        leader.coins[self.relative_slot(current) as usize][proposal.block.metadata.winning_index] =
            proposal.block.metadata.new_public_inputs.clone();
        self.append_participant(leader);

        // Check if proposal fork has can be finalized, to broadcast those blocks
        let mut to_broadcast = vec![];
        match index {
            -1 => {
                let pc = ProposalChain::new(self.consensus.genesis_block, proposal.clone());
                self.consensus.proposals.push(pc);
            }
            _ => {
                self.consensus.proposals[index as usize].add(proposal);
                match self.chain_finalization(index as usize).await {
                    Ok(v) => {
                        to_broadcast = v;
                    }
                    Err(e) => {
                        error!("receive_proposal(): Block finalization failed: {}", e);
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
            let hash = last.header;
            if proposal.block.header.previous == hash &&
                proposal.block.header.slot > last.block.header.slot
            {
                return Ok(index as i64)
            }

            if proposal.block.header.previous == last.block.header.previous &&
                proposal.block.header.slot > last.block.header.slot
            {
                fork = Some(chain.clone());
                break
            }
        }

        if let Some(mut chain) = fork {
            debug!("find_extended_chain_index(): Proposal to fork a forkchain was received.");
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
    pub fn proposal_exists(&self, input_proposal: &blake3::Hash) -> bool {
        for chain in self.consensus.proposals.iter() {
            for proposal in chain.proposals.iter() {
                if input_proposal == &proposal.header {
                    return true
                }
            }
        }

        false
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
    /// - If the node has observed the creation of 3 proposals in a fork chain and no other
    ///   forks exists at same or greater height, it finalizes (appends to canonical blockchain)
    ///   all proposals up to the last one.
    /// When fork chain proposals are finalized, the rest of fork chains not
    /// starting by those proposals are removed.
    pub async fn chain_finalization(&mut self, chain_index: usize) -> Result<Vec<BlockInfo>> {
        let length = self.consensus.proposals[chain_index].proposals.len();
        if length < 3 {
            debug!(
                "chain_finalization(): Less than 3 proposals in chain {}, nothing to finalize",
                chain_index
            );
            return Ok(vec![])
        }

        for (i, c) in self.consensus.proposals.iter().enumerate() {
            if i == chain_index {
                continue
            }
            if c.proposals.len() >= length {
                debug!("chain_finalization(): Same or greater length fork chain exists, nothing to finalize");
                return Ok(vec![])
            }
        }

        let chain = &mut self.consensus.proposals[chain_index];
        let bound = length - 1;
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

    /// Append a new participant to the participants list.
    pub fn append_participant(&mut self, participant: Participant) -> bool {
        if let Some(p) = self.consensus.participants.get(&participant.address) {
            if p == &participant {
                return false
            }
        }
        // TODO: [PLACEHOLDER] don't blintly trust the public inputs/validate them
        self.consensus.participants.insert(participant.address, participant);
        true
    }

    /// Utility function to extract leader selection lottery randomness(eta),
    /// defined as the hash of the previous lead proof converted to pallas base.
    fn get_eta(&self) -> pallas::Base {
        let proof_tx_hash = self.blockchain.get_last_proof_hash().unwrap();
        let mut bytes: [u8; 32] = *proof_tx_hash.as_bytes();
        // read first 254 bits
        bytes[30] = 0;
        bytes[31] = 0;
        pallas::Base::from_repr(bytes).unwrap()
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
