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

use std::{collections::HashMap, io::Cursor};

use async_std::sync::{Arc, RwLock};
use darkfi_sdk::{
    crypto::{
        constants::MERKLE_DEPTH,
        contract_id::{DAO_CONTRACT_ID, MONEY_CONTRACT_ID},
        schnorr::{SchnorrPublic, SchnorrSecret},
        MerkleNode, PublicKey, SecretKey,
    },
    db::SMART_CONTRACT_ZKAS_DB_NAME,
    incrementalmerkletree::{bridgetree::BridgeTree, Tree},
    pasta::{group::ff::PrimeField, pallas},
};
use darkfi_serial::{deserialize, serialize, Decodable, Encodable, WriteExt};
use halo2_proofs::arithmetic::Field;
use log::{debug, error, info, warn};
use rand::rngs::OsRng;
use serde_json::json;

use super::{
    constants,
    lead_coin::LeadCoin,
    state::{ConsensusState, Fork, SlotCheckpoint, StateCheckpoint},
    BlockInfo, BlockProposal, Header, LeadInfo, LeadProof,
};

use crate::{
    blockchain::Blockchain,
    rpc::jsonrpc::JsonNotification,
    runtime::vm_runtime::Runtime,
    system::{Subscriber, SubscriberPtr},
    tx::Transaction,
    util::time::Timestamp,
    wallet::WalletPtr,
    zk::{
        proof::{ProvingKey, VerifyingKey},
        vm::ZkCircuit,
        vm_stack::empty_witnesses,
    },
    zkas::ZkBinary,
    Error, Result,
};

/// Atomic pointer to validator state.
pub type ValidatorStatePtr = Arc<RwLock<ValidatorState>>;

type VerifyingKeyMap = Arc<RwLock<HashMap<[u8; 32], Vec<(String, VerifyingKey)>>>>;

/// This struct represents the state of a validator node.
pub struct ValidatorState {
    /// Leader proof proving key
    pub lead_proving_key: Option<ProvingKey>,
    /// Leader proof verifying key
    pub lead_verifying_key: VerifyingKey,
    /// Hot/Live data used by the consensus algorithm
    pub consensus: ConsensusState,
    /// Canonical (finalized) blockchain
    pub blockchain: Blockchain,
    /// Pending transactions
    pub unconfirmed_txs: Vec<Transaction>,
    /// A map of various subscribers exporting live info from the blockchain
    /// TODO: Instead of JsonNotification, it can be an enum of internal objects,
    ///       and then we don't have to deal with json in this module but only
    //        externally.
    pub subscribers: HashMap<&'static str, SubscriberPtr<JsonNotification>>,
    /// ZK proof verifying keys for smart contract calls
    pub verifying_keys: VerifyingKeyMap,
    /// Wallet interface
    pub wallet: WalletPtr,
    /// Flag signalling node has finished initial sync
    pub synced: bool,
    /// Flag to enable single-node mode
    pub single_node: bool,
}

impl ValidatorState {
    pub async fn new(
        db: &sled::Db, // <-- TODO: Avoid this with some wrapping, sled should only be in blockchain
        bootstrap_ts: Timestamp,
        genesis_ts: Timestamp,
        genesis_data: blake3::Hash,
        initial_distribution: u64,
        wallet: WalletPtr,
        faucet_pubkeys: Vec<PublicKey>,
        enable_participation: bool,
        single_node: bool,
    ) -> Result<ValidatorStatePtr> {
        debug!(target: "consensus::validator", "Initializing ValidatorState");

        debug!(target: "consensus::validator", "Initializing wallet tables for consensus");

        // Initialize consensus coin table.
        // NOTE: In future this will be redundant as consensus coins will live in the money contract.
        if enable_participation {
            wallet.exec_sql(include_str!("consensus_coin.sql")).await?;
        }

        debug!(target: "consensus::validator", "Generating leader proof keys with k: {}", constants::LEADER_PROOF_K);
        let bincode = include_bytes!("../../proof/lead.zk.bin");
        let zkbin = ZkBinary::decode(bincode)?;
        let witnesses = empty_witnesses(&zkbin);
        let circuit = ZkCircuit::new(witnesses, zkbin);

        let lead_verifying_key = VerifyingKey::build(constants::LEADER_PROOF_K, &circuit);
        // We only need this proving key if we're going to participate in the consensus.
        let lead_proving_key = if enable_participation {
            Some(ProvingKey::build(constants::LEADER_PROOF_K, &circuit))
        } else {
            None
        };

        let blockchain = Blockchain::new(db, genesis_ts, genesis_data)?;
        let consensus = ConsensusState::new(
            wallet.clone(),
            blockchain.clone(),
            bootstrap_ts,
            genesis_ts,
            genesis_data,
            initial_distribution,
            single_node,
        )?;

        let unconfirmed_txs = vec![];

        // -----NATIVE WASM CONTRACTS-----
        // This is the current place where native contracts are being deployed.
        // When the `Blockchain` object is created, it doesn't care whether it
        // already has the contract data or not. If there's existing data, it
        // will just open the necessary db and trees, and give back what it has.
        // This means that on subsequent runs our native contracts will already
        // be in a deployed state, so what we actually do here is a redeployment.
        // This kind of operation should only modify the contract's state in case
        // it wasn't deployed before (meaning the initial run). Otherwise, it
        // shouldn't touch anything, or just potentially update the db schemas or
        // whatever is necessary. This logic should be handled in the init function
        // of the actual contract, so make sure the native contracts handle this well.

        // The faucet pubkeys are pubkeys which are allowed to create clear inputs
        // in the money contract.
        let money_contract_deploy_payload = serialize(&faucet_pubkeys);
        let dao_contract_deploy_payload = vec![];

        // In this hashmap, we keep references to ZK proof verifying keys needed
        // for the circuits our native contracts provide.
        let mut verifying_keys = HashMap::new();

        let native_contracts = vec![
            (
                "Money Contract",
                *MONEY_CONTRACT_ID,
                include_bytes!("../contract/money/money_contract.wasm").to_vec(),
                money_contract_deploy_payload,
            ),
            (
                "DAO Contract",
                *DAO_CONTRACT_ID,
                include_bytes!("../contract/dao/dao_contract.wasm").to_vec(),
                dao_contract_deploy_payload,
            ),
        ];

        info!(target: "consensus::validator", "Deploying native wasm contracts");
        for nc in native_contracts {
            info!(target: "consensus::validator", "Deploying {} with ContractID {}", nc.0, nc.1);
            let mut runtime = Runtime::new(&nc.2[..], blockchain.clone(), nc.1)?;
            runtime.deploy(&nc.3)?;
            info!(target: "consensus::validator", "Successfully deployed {}", nc.0);

            // When deployed, we can do a lookup for the zkas circuits and
            // initialize verifying keys for them.
            info!(target: "consensus::validator", "Creating ZK verifying keys for {} zkas circuits", nc.0);
            info!(target: "consensus::validator", "Looking up zkas db for {} (ContractID: {})", nc.0, nc.1);
            let zkas_db = blockchain.contracts.lookup(
                &blockchain.sled_db,
                &nc.1,
                SMART_CONTRACT_ZKAS_DB_NAME,
            )?;

            let mut vks = vec![];
            for i in zkas_db.iter() {
                info!(target: "consensus::validator", "Iterating over zkas db");
                let (zkas_ns, zkas_bincode) = i?;
                info!(target: "consensus::validator", "Deserializing namespace");
                let zkas_ns: String = deserialize(&zkas_ns)?;
                info!(target: "consensus::validator", "Creating VerifyingKey for zkas circuit with namespace {}", zkas_ns);
                let zkbin = ZkBinary::decode(&zkas_bincode)?;
                let circuit = ZkCircuit::new(empty_witnesses(&zkbin), zkbin);
                // FIXME: This k=13 man...
                let vk = VerifyingKey::build(13, &circuit);
                vks.push((zkas_ns, vk));
            }

            info!(target: "consensus::validator", "Finished creating VerifyingKey objects for {} (ContractID: {})", nc.0, nc.1);
            verifying_keys.insert(nc.1.to_bytes(), vks);
        }
        info!(target: "consensus::validator", "Finished deployment of native wasm contracts");
        // -----NATIVE WASM CONTRACTS-----

        // Here we initialize various subscribers that can export live consensus/blockchain data.
        let mut subscribers = HashMap::new();
        let block_subscriber = Subscriber::new();
        subscribers.insert("blocks", block_subscriber);

        let state = Arc::new(RwLock::new(ValidatorState {
            lead_proving_key,
            lead_verifying_key,
            consensus,
            blockchain,
            unconfirmed_txs,
            subscribers,
            verifying_keys: Arc::new(RwLock::new(verifying_keys)),
            wallet,
            synced: false,
            single_node,
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
                error!(target: "consensus::validator", "append_tx(): Failed querying txstore: {}", e);
                return false
            }
        };

        if self.unconfirmed_txs.contains(&tx) || tx_in_txstore {
            info!(target: "consensus::validator", "append_tx(): We have already seen this tx.");
            return false
        }

        info!(target: "consensus::validator", "append_tx(): Starting state transition validation");
        if let Err(e) = self.verify_transactions(&[tx.clone()], false).await {
            error!(target: "consensus::validator", "append_tx(): Failed to verify transaction: {}", e);
            return false
        };

        info!(target: "consensus::validator", "append_tx(): Appended tx to mempool");
        self.unconfirmed_txs.push(tx);
        true
    }

    /// Generate a block proposal for the current slot, containing all
    /// unconfirmed transactions. Proposal extends the longest fork
    /// chain the node is holding.
    pub fn propose(
        &mut self,
        slot: u64,
        fork_index: i64,
        coin_index: usize,
        sigma1: pallas::Base,
        sigma2: pallas::Base,
    ) -> Result<Option<(BlockProposal, LeadCoin, pallas::Scalar)>> {
        let eta = self.consensus.get_eta();
        // Check if node can produce proposals
        if !self.consensus.proposing {
            return Ok(None)
        }

        // Generate proposal
        let unproposed_txs = self.unproposed_txs(fork_index);
        let mut tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100);
        // The following is pretty weird, so something better should be done.
        for tx in &unproposed_txs {
            let mut hash = [0_u8; 32];
            hash[0..31].copy_from_slice(&blake3::hash(&serialize(tx)).as_bytes()[0..31]);
            tree.append(&MerkleNode::from(pallas::Base::from_repr(hash).unwrap()));
        }
        let root = tree.root(0).unwrap();

        // Checking if extending a fork or canonical
        let (prev_hash, coin) = if fork_index == -1 {
            (self.blockchain.last()?.1, self.consensus.coins[coin_index].clone())
        } else {
            let checkpoint = self.consensus.forks[fork_index as usize].sequence.last().unwrap();
            (checkpoint.proposal.hash, checkpoint.coins[coin_index].clone())
        };

        // Generate derived coin blind
        let derived_blind = pallas::Scalar::random(&mut OsRng);

        // Generating leader proof
        let (proof, public_inputs) = coin.create_lead_proof(
            sigma1,
            sigma2,
            eta.clone(),
            pallas::Base::from(self.consensus.current_slot()),
            self.lead_proving_key.as_ref().unwrap(),
            derived_blind,
        );

        // Signing using coin
        let secret_key = coin.coin1_sk;
        let header = Header::new(
            prev_hash,
            self.consensus.slot_epoch(slot),
            slot,
            Timestamp::current_time(),
            root,
        );
        let signed_proposal =
            SecretKey::from(secret_key).sign(&mut OsRng, &header.headerhash().as_bytes()[..]);
        let public_key = PublicKey::from_secret(secret_key.into());

        let lead_info = LeadInfo::new(
            signed_proposal,
            public_key,
            public_inputs,
            coin.slot,
            eta,
            LeadProof::from(proof?),
            self.consensus.previous_leaders,
        );

        Ok(Some((BlockProposal::new(header, unproposed_txs, lead_info), coin, derived_blind)))
    }

    /// Retrieve all unconfirmed transactions not proposed in previous blocks
    /// of provided index chain.
    pub fn unproposed_txs(&self, index: i64) -> Vec<Transaction> {
        let unproposed_txs = if index == -1 {
            // If index is -1 (canonical blockchain) a new fork will be generated,
            // therefore all unproposed transactions can be included in the proposal.
            self.unconfirmed_txs.clone()
        } else {
            // We iterate over the fork chain proposals to find already proposed
            // transactions and remove them from the local unproposed_txs vector.
            let mut filtered_txs = self.unconfirmed_txs.clone();
            let chain = &self.consensus.forks[index as usize];
            for state_checkpoint in &chain.sequence {
                for tx in &state_checkpoint.proposal.block.txs {
                    if let Some(pos) = filtered_txs.iter().position(|txs| *txs == *tx) {
                        filtered_txs.remove(pos);
                    }
                }
            }
            filtered_txs
        };

        // Check if transactions exceed configured cap
        let cap = constants::TXS_CAP;
        if unproposed_txs.len() > cap {
            return unproposed_txs[0..cap].to_vec()
        }

        unproposed_txs
    }

    /// Given a proposal, the node verify its sender (slot leader) and finds which blockchain
    /// it extends. If the proposal extends the canonical blockchain, a new fork chain is created.
    /// Returns flag to signal if proposal should be broadcasted. Only active consensus participants
    /// should broadcast proposals.
    pub async fn receive_proposal(
        &mut self,
        proposal: &BlockProposal,
        coin: Option<(usize, LeadCoin, pallas::Scalar)>,
    ) -> Result<bool> {
        let current = self.consensus.current_slot();
        // Node hasn't started participating
        match self.consensus.participating {
            Some(start) => {
                if current < start {
                    return Ok(false)
                }
            }
            None => return Ok(false),
        }

        // Node have already checked for finalization in this slot
        if current <= self.consensus.checked_finalization {
            warn!(target: "consensus::validator", "receive_proposal(): Proposal received after finalization sync period.");
            return Err(Error::ProposalAfterFinalizationError)
        }

        // Proposal validations
        let lf = &proposal.block.lead_info;
        let hdr = &proposal.block.header;

        // Ignore proposal if not for current slot
        if hdr.slot != current {
            return Err(Error::ProposalNotForCurrentSlotError)
        }

        // Verify that proposer can produce proposals.
        // Nodes that created coins in the bootstrap slot can propose immediately.
        // NOTE: Later, this will be enforced via contract, where it will be explicit
        // when a node can produce proposals, and after which slot they can be considered as valid.
        let elapsed_slots = current - lf.coin_slot;
        if lf.coin_slot != self.consensus.bootstrap_slot &&
            elapsed_slots <= (constants::EPOCH_LENGTH as u64)
        {
            warn!(
                target: "consensus::validator",
                "receive_proposal(): Proposer {} is not eligible to produce proposals",
                lf.public_key
            );
            return Err(Error::ProposalProposerNotEligible)
        }

        // Check if proposal extends any existing fork chains
        let index = self.consensus.find_extended_chain_index(proposal)?;
        if index == -2 {
            return Err(Error::ExtendedChainIndexNotFound)
        }

        // Check that proposal transactions don't exceed limit
        if proposal.block.txs.len() > constants::TXS_CAP {
            warn!(
                target: "consensus::validator",
                "receive_proposal(): Received proposal transactions exceed configured cap: {} - {}",
                proposal.block.txs.len(),
                constants::TXS_CAP
            );
            return Err(Error::ProposalTxsExceedCapError)
        }

        // Verify proposal signature is valid based on producer public key
        // TODO: derive public key from proof
        if !lf.public_key.verify(proposal.header.as_bytes(), &lf.signature) {
            warn!(target: "consensus::validator", "receive_proposal(): Proposer {} signature could not be verified", lf.public_key);
            return Err(Error::InvalidSignature)
        }

        // Check if proposal hash matches actual one
        let proposal_hash = proposal.block.blockhash();
        if proposal.hash != proposal_hash {
            warn!(
                target: "consensus::validator",
                "receive_proposal(): Received proposal contains mismatched hashes: {} - {}",
                proposal.hash, proposal_hash
            );
            return Err(Error::ProposalHashesMissmatchError)
        }

        // Check if proposal header matches actual one
        let proposal_header = hdr.headerhash();
        if proposal.header != proposal_header {
            warn!(
                target: "consensus::validator",
                "receive_proposal(): Received proposal contains mismatched headers: {} - {}",
                proposal.header, proposal_header
            );
            return Err(Error::ProposalHeadersMissmatchError)
        }

        // Ignore node coin validations if we oporate in single-node mode
        if !self.single_node {
            // Verify proposal leader proof
            if let Err(e) = lf.proof.verify(&self.lead_verifying_key, &lf.public_inputs) {
                error!(target: "consensus::validator", "receive_proposal(): Error during leader proof verification: {}", e);
                return Err(Error::LeaderProofVerification)
            };
            info!(target: "consensus::validator", "receive_proposal(): Leader proof verified successfully!");

            // Validate proposal public value against coin creation slot checkpoint
            let (mu_y, mu_rho) = LeadCoin::election_seeds_u64(
                self.consensus.get_eta(),
                self.consensus.current_slot(),
            );
            // y
            let prop_mu_y = lf.public_inputs[constants::PI_MU_Y_INDEX];

            if mu_y != prop_mu_y {
                error!(
                    target: "consensus::validator",
                    "receive_proposal(): Failed to verify mu_y: {:?}, proposed: {:?}",
                    mu_y, prop_mu_y
                );
                return Err(Error::ProposalPublicValuesMismatched)
            }

            // rho
            let prop_mu_rho = lf.public_inputs[constants::PI_MU_RHO_INDEX];

            if mu_rho != prop_mu_rho {
                error!(
                    target: "consensus::validator",
                    "receive_proposal(): Failed to verify mu_rho: {:?}, proposed: {:?}",
                    mu_rho, prop_mu_rho
                );
                return Err(Error::ProposalPublicValuesMismatched)
            }

            // Validate proposal coin sigmas against current slot checkpoint
            let checkpoint = self.consensus.get_slot_checkpoint(current)?;
            // sigma1
            let prop_sigma1 = lf.public_inputs[constants::PI_SIGMA1_INDEX];
            if checkpoint.sigma1 != prop_sigma1 {
                error!(
                    target: "consensus::validator",
                    "receive_proposal(): Failed to verify public value sigma1: {:?}, to proposed: {:?}",
                    checkpoint.sigma1, prop_sigma1
                );
            }
            // sigma2
            let prop_sigma2 = lf.public_inputs[constants::PI_SIGMA2_INDEX];
            if checkpoint.sigma2 != prop_sigma2 {
                error!(
                    target: "consensus::validator",
                    "receive_proposal(): Failed to verify public value sigma2: {:?}, to proposed: {:?}",
                    checkpoint.sigma2, prop_sigma2
                );
            }
        }

        // Create corresponding state checkpoint for validations
        let mut state_checkpoint = match index {
            -1 => {
                // Extends canonical
                StateCheckpoint::new(
                    proposal.clone(),
                    self.consensus.coins.clone(),
                    self.consensus.coins_tree.clone(),
                    self.consensus.nullifiers.clone(),
                )
            }
            _ => {
                // Extends a fork
                let previous = self.consensus.forks[index as usize].sequence.last().unwrap();
                StateCheckpoint::new(
                    proposal.clone(),
                    previous.coins.clone(),
                    previous.coins_tree.clone(),
                    previous.nullifiers.clone(),
                )
            }
        };

        // Check if proposal coin nullifiers already exist in the state checkpoint
        let prop_sn = lf.public_inputs[constants::PI_NULLIFIER_INDEX];
        for sn in &state_checkpoint.nullifiers {
            if *sn == prop_sn {
                error!(target: "consensus::validator", "receive_proposal(): Proposal nullifiers exist.");
                return Err(Error::ProposalIsSpent)
            }
        }

        // Validate state transition against canonical state
        // TODO: This should be validated against fork state
        info!(target: "consensus::validator", "receive_proposal(): Starting state transition validation");
        if let Err(e) = self.verify_transactions(&proposal.block.txs, false).await {
            error!(target: "consensus::validator", "receive_proposal(): Transaction verifications failed: {}", e);
            return Err(e)
        };

        // TODO: [PLACEHOLDER] Add rewards validation

        // If proposal came fromself, we derive new coin
        if let Some((idx, c, derived_blind)) = coin {
            info!(target: "consensus::validator", "receive_proposal(): Storing derived coin...");
            // Derive coin
            let derived = c.derive_coin(&mut state_checkpoint.coins_tree, derived_blind);
            // Update consensus coin in wallet
            // NOTE: In future this will be redundant as consensus coins will live in the money contract.
            // Get a wallet connection
            let mut conn = self.wallet.conn.acquire().await?;
            let query_str = format!(
                "UPDATE {} SET {} = ?1",
                constants::CONSENSUS_COIN_TABLE,
                constants::CONSENSUS_COIN_COL
            );
            let mut query = sqlx::query(&query_str);
            query = query.bind(serialize(&derived));
            query.execute(&mut conn).await?;

            state_checkpoint.coins[idx] = derived;
        }
        // Store proposal coins nullifiers
        state_checkpoint.nullifiers.push(prop_sn);

        // Extend corresponding chain
        match index {
            -1 => {
                let fork = Fork::new(self.consensus.genesis_block, state_checkpoint);
                self.consensus.forks.push(fork);
            }
            _ => {
                self.consensus.forks[index as usize].add(&state_checkpoint);
            }
        };

        // Increase slot leaders count
        self.consensus.previous_leaders += 1;

        Ok(true)
    }

    /// Remove provided transactions vector from unconfirmed_txs if they exist.
    pub fn remove_txs(&mut self, transactions: &Vec<Transaction>) -> Result<()> {
        for tx in transactions {
            if let Some(pos) = self.unconfirmed_txs.iter().position(|txs| txs == tx) {
                self.unconfirmed_txs.remove(pos);
            }
        }

        Ok(())
    }

    /// Node checks if any of the fork chains can be finalized.
    /// Consensus finalization logic:
    /// - If the node has observed the creation of a fork chain and no other forks exists at same or greater height,
    ///   it finalizes (appends to canonical blockchain) all proposals in that fork chain.
    /// When fork chain proposals are finalized, the rest of fork chains are removed and all
    /// slot checkpoints are apppended to canonical state.
    pub async fn chain_finalization(&mut self) -> Result<(Vec<BlockInfo>, Vec<SlotCheckpoint>)> {
        let slot = self.consensus.current_slot();
        info!(target: "consensus::validator", "chain_finalization(): Started finalization check for slot: {}", slot);
        // Set last slot finalization check occured to current slot
        self.consensus.checked_finalization = slot;

        // First we find longest fork without any other forks at same height
        let mut fork_index = -1;
        let mut max_length = 0;
        for (index, fork) in self.consensus.forks.iter().enumerate() {
            let length = fork.sequence.len();
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
                info!(target: "consensus::validator", "chain_finalization(): Eligible forks with same height exist, nothing to finalize.");
                return Ok((vec![], vec![]))
            }
            -1 => {
                info!(target: "consensus::validator", "chain_finalization(): Nothing to finalize.");
            }
            _ => {
                info!(target: "consensus::validator", "chain_finalization(): Chain {} can be finalized!", fork_index)
            }
        }

        if max_length == 0 {
            return Ok((vec![], vec![]))
        }

        // Starting finalization
        let fork = self.consensus.forks[fork_index as usize].clone();

        // Retrieving proposals to finalize
        let mut finalized: Vec<BlockInfo> = vec![];
        for state_checkpoint in &fork.sequence {
            finalized.push(state_checkpoint.proposal.clone().into());
        }

        // Adding finalized proposals to canonical
        info!(target: "consensus::validator", "consensus: Adding {} finalized block to canonical chain.", finalized.len());
        match self.blockchain.add(&finalized) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "consensus::validator", "consensus: Failed appending finalized blocks to canonical chain: {}", e);
                return Err(e)
            }
        };

        let blocks_subscriber = self.subscribers.get("blocks").unwrap().clone();

        // Validating state transitions
        let mut erroneous_txs = vec![];
        for proposal in &finalized {
            // TODO: Is this the right place? We're already doing this in protocol_sync.
            // TODO: These state transitions have already been checked. (I wrote this, but where?)
            // TODO: FIXME: The state transitions have already been written, they have to be in memory
            //              until this point.
            info!(target: "consensus::validator", "Applying state transition for finalized block");
            match self.verify_transactions(&proposal.txs, true).await {
                Ok(hashes) => erroneous_txs.extend(hashes),
                Err(e) => {
                    error!(target: "consensus::validator", "Finalized block transaction verifications failed: {}", e);
                    return Err(e)
                }
            }

            // Remove proposal transactions from memory pool
            if let Err(e) = self.remove_txs(&proposal.txs) {
                error!(target: "consensus::validator", "Removing finalized block transactions failed: {}", e);
                return Err(e)
            }

            // TODO: Don't hardcode this:
            let params = json!([bs58::encode(&serialize(proposal)).into_string()]);
            let notif = JsonNotification::new("blockchain.subscribe_blocks", params);
            info!(target: "consensus::validator", "consensus: Sending notification about finalized block");
            blocks_subscriber.notify(notif).await;
        }
        self.blockchain.add_erroneous_txs(&erroneous_txs)?;

        // Setting leaders history to last proposal leaders count
        let last_state_checkpoint = fork.sequence.last().unwrap().clone();

        // Setting canonical states from last finalized checkpoint
        self.consensus.coins = last_state_checkpoint.coins;
        self.consensus.coins_tree = last_state_checkpoint.coins_tree;
        self.consensus.nullifiers = last_state_checkpoint.nullifiers;

        // Adding finalized slot checkpoints to canonical
        let finalized_slot_checkpoints: Vec<SlotCheckpoint> =
            self.consensus.slot_checkpoints.clone();

        debug!(
            target: "consensus::validator",
            "consensus: Adding {} finalized slot checkpoints to canonical chain.",
            finalized_slot_checkpoints.len()
        );
        match self.blockchain.add_slot_checkpoints(&finalized_slot_checkpoints) {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "consensus::validator",
                    "consensus: Failed appending finalized slot checkpoints to canonical chain: {}",
                    e
                );
                return Err(e)
            }
        };

        // Resetting forks and slot checkpoints
        self.consensus.forks = vec![];
        self.consensus.slot_checkpoints = vec![];

        Ok((finalized, finalized_slot_checkpoints))
    }

    // ==========================
    // State transition functions
    // ==========================
    // TODO TESTNET: Write down all cases below
    // State transition checks should be happening in the following cases for a sync node:
    // 1) When a finalized block is received
    // 2) When a transaction is being broadcasted to us
    // State transition checks should be happening in the following cases for a consensus participating node:
    // 1) When a finalized block is received
    // 2) When a transaction is being broadcasted to us
    // ==========================

    /// Validate and append to canonical state received blocks.
    pub async fn receive_blocks(&mut self, blocks: &[BlockInfo]) -> Result<()> {
        // Verify state transitions for all blocks and their respective transactions.
        info!(target: "consensus::validator", "receive_blocks(): Starting state transition validations");
        let mut erroneous_txs = vec![];
        for block in blocks {
            match self.verify_transactions(&block.txs, true).await {
                Ok(hashes) => erroneous_txs.extend(hashes),
                Err(e) => {
                    error!(target: "consensus::validator", "receive_blocks(): Transaction verifications failed: {}", e);
                    return Err(e)
                }
            }
        }

        info!(target: "consensus::validator", "receive_blocks(): All state transitions passed");
        info!(target: "consensus::validator", "receive_blocks(): Appending blocks to ledger");
        self.blockchain.add(blocks)?;
        self.blockchain.add_erroneous_txs(&erroneous_txs)?;

        Ok(())
    }

    /// Validate and append to canonical state received finalized block.
    /// Returns boolean flag indicating already existing block.
    pub async fn receive_finalized_block(&mut self, block: BlockInfo) -> Result<bool> {
        match self.blockchain.has_block(&block) {
            Ok(v) => {
                if v {
                    info!(target: "consensus::validator", "receive_finalized_block(): Existing block received");
                    return Ok(false)
                }
            }
            Err(e) => {
                error!(target: "consensus::validator", "receive_finalized_block(): failed checking for has_block(): {}", e);
                return Ok(false)
            }
        };

        info!(target: "consensus::validator", "receive_finalized_block(): Executing state transitions");
        self.receive_blocks(&[block.clone()]).await?;

        // TODO: Don't hardcode this:
        let blocks_subscriber = self.subscribers.get("blocks").unwrap();
        let params = json!([bs58::encode(&serialize(&block)).into_string()]);
        let notif = JsonNotification::new("blockchain.subscribe_blocks", params);
        info!(target: "consensus::validator", "consensus: Sending notification about finalized block");
        blocks_subscriber.notify(notif).await;

        info!(target: "consensus::validator", "receive_finalized_block(): Removing block transactions from unconfirmed_txs");
        self.remove_txs(&block.txs)?;

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
                        info!(target: "consensus::validator", "receive_sync_blocks(): Existing block received");
                        continue
                    }
                    new_blocks.push(block.clone());
                }
                Err(e) => {
                    error!(target: "consensus::validator", "receive_sync_blocks(): failed checking for has_block(): {}", e);
                    continue
                }
            };
        }

        if new_blocks.is_empty() {
            info!(target: "consensus::validator", "receive_sync_blocks(): no new blocks to append");
            return Ok(())
        }

        info!(target: "consensus::validator", "receive_sync_blocks(): Executing state transitions");
        self.receive_blocks(&new_blocks[..]).await?;

        // TODO: Don't hardcode this:
        let blocks_subscriber = self.subscribers.get("blocks").unwrap();
        for block in new_blocks {
            let params = json!([bs58::encode(&serialize(&block)).into_string()]);
            let notif = JsonNotification::new("blockchain.subscribe_blocks", params);
            info!(target: "consensus::validator", "consensus: Sending notification about finalized block");
            blocks_subscriber.notify(notif).await;
        }

        Ok(())
    }

    /// Validate signatures, wasm execution, and zk proofs for given transactions.
    /// If all of those succeed, try to execute a state update for the contract calls.
    /// Currently the verifications are sequential, and the function will skip a
    /// transaction if any of the verifications fail.
    /// The function takes a boolean called `write` which tells it to actually write
    /// the state transitions to the database.
    // TODO: Currently we keep erroneous transactions in the vector and blocks,
    //       in order to apply max fee logic in the future, to prevent spamming.
    // TODO: This should be paralellized as if even one tx in the batch fails to verify,
    //       we can skip it.
    pub async fn verify_transactions(
        &self,
        txs: &[Transaction],
        write: bool,
    ) -> Result<Vec<Transaction>> {
        info!(target: "consensus::validator", "Verifying {} transaction(s)", txs.len());
        let mut erroneous_txs = vec![];
        for tx in txs {
            let tx_hash = blake3::hash(&serialize(tx));
            info!(target: "consensus::validator", "Verifying transaction {}", tx_hash);

            // Table of public inputs used for ZK proof verification
            let mut zkp_table = vec![];
            // Table of public keys used for signature verification
            let mut sig_table = vec![];
            // State updates produced by contract execcution
            let mut updates = vec![];

            // Iterate over all calls to get the metadata
            let mut skip = false;
            for (idx, call) in tx.calls.iter().enumerate() {
                info!(target: "consensus::validator", "Executing contract call {}", idx);
                let wasm = match self.blockchain.wasm_bincode.get(call.contract_id) {
                    Ok(v) => {
                        info!(target: "consensus::validator", "Found wasm bincode for {}", call.contract_id);
                        v
                    }
                    Err(e) => {
                        error!(
                            target: "consensus::validator",
                            "Could not find wasm bincode for contract {}: {}",
                            call.contract_id, e
                        );
                        skip = true;
                        break
                    }
                };

                // Write the actual payload data
                let mut payload = vec![];
                payload.write_u32(idx as u32)?; // Call index
                tx.calls.encode(&mut payload)?; // Actual call data

                // Instantiate the wasm runtime
                let mut runtime =
                    match Runtime::new(&wasm, self.blockchain.clone(), call.contract_id) {
                        Ok(v) => v,
                        Err(e) => {
                            error!(
                                target: "consensus::validator",
                                "Failed to instantiate WASM runtime for contract {}: {}",
                                call.contract_id, e
                            );
                            skip = true;
                            break
                        }
                    };

                info!(target: "consensus::validator", "Executing \"metadata\" call");
                let metadata = match runtime.metadata(&payload) {
                    Ok(v) => v,
                    Err(e) => {
                        error!(target: "consensus::validator", "Failed to execute \"metadata\" call: {}", e);
                        skip = true;
                        break
                    }
                };

                // Decode the metadata retrieved from the execution
                let mut decoder = Cursor::new(&metadata);
                let zkp_pub: Vec<(String, Vec<pallas::Base>)> = match Decodable::decode(
                    &mut decoder,
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        error!(target: "consensus::validator", "Failed to decode ZK public inputs from metadata: {}", e);
                        skip = true;
                        break
                    }
                };

                let sig_pub: Vec<PublicKey> = match Decodable::decode(&mut decoder) {
                    Ok(v) => v,
                    Err(e) => {
                        error!(target: "consensus::validator", "Failed to decode signature pubkeys from metadata: {}", e);
                        skip = true;
                        break
                    }
                };

                // TODO: Make sure we've read all the bytes above.
                info!(target: "consensus::validator", "Successfully executed \"metadata\" call");
                zkp_table.push(zkp_pub);
                sig_table.push(sig_pub);

                // After getting the metadata, we run the "exec" function with the same
                // runtime and the same payload.
                info!(target: "consensus::validator", "Executing \"exec\" call");
                match runtime.exec(&payload) {
                    Ok(v) => {
                        info!(target: "consensus::validator", "Successfully executed \"exec\" call");
                        updates.push(v);
                    }
                    Err(e) => {
                        error!(
                            target: "consensus::validator",
                            "Failed to execute \"exec\" call for contract id {}: {}",
                            call.contract_id, e
                        );
                        skip = true;
                        break
                    }
                };
                // At this point we're done with the call and move on to the next one.
            }
            if skip {
                warn!(target: "consensus::validator", "Skipping transaction {}", tx_hash);
                erroneous_txs.push(tx.clone());
                continue
            }

            // When we're done looping and executing over the tx's contract calls, we
            // move on with verification. First we verify the signatures as that's
            // cheaper, and then finally we verify the ZK proofs.
            info!(target: "consensus::validator", "Verifying signatures for transaction {}", tx_hash);
            if sig_table.len() != tx.signatures.len() {
                error!(target: "consensus::validator", "Incorrect number of signatures in tx {}", tx_hash);
                warn!(target: "consensus::validator", "Skipping transaction {}", tx_hash);
                erroneous_txs.push(tx.clone());
                continue
            }

            match tx.verify_sigs(sig_table) {
                Ok(()) => {
                    info!(target: "consensus::validator", "Signatures verification for tx {} successful", tx_hash)
                }
                Err(e) => {
                    error!(target: "consensus::validator", "Signature verification for tx {} failed: {}", tx_hash, e);
                    warn!(target: "consensus::validator", "Skipping transaction {}", tx_hash);
                    erroneous_txs.push(tx.clone());
                    continue
                }
            };

            // NOTE: When it comes to the ZK proofs, we first do a lookup of the
            // verifying keys, but if we do not find them, we'll generate them
            // inside of this function. This can be kinda expensive, so open to
            // alternatives.
            info!(target: "consensus::validator", "Verifying ZK proofs for transaction {}", tx_hash);
            match tx.verify_zkps(self.verifying_keys.clone(), zkp_table).await {
                Ok(()) => {
                    info!(target: "consensus::validator", "ZK proof verification for tx {} successful", tx_hash)
                }
                Err(e) => {
                    error!(target: "consensus::validator", "ZK proof verification for tx {} failed: {}", tx_hash, e);
                    warn!(target: "consensus::validator", "Skipping transaction {}", tx_hash);
                    erroneous_txs.push(tx.clone());
                    continue
                }
            };

            // After the verifications stage passes, if we're told to write, we
            // apply the state updates.
            assert!(tx.calls.len() == updates.len());
            if write {
                info!(target: "consensus::validator", "Performing state updates");
                for (call, update) in tx.calls.iter().zip(updates.iter()) {
                    // For this we instantiate the runtimes again.
                    // TODO: Optimize this
                    // TODO: Sum up the gas costs of previous calls during execution
                    //       and verification and these.
                    let wasm = match self.blockchain.wasm_bincode.get(call.contract_id) {
                        Ok(v) => {
                            info!(target: "consensus::validator", "Found wasm bincode for {}", call.contract_id);
                            v
                        }
                        Err(e) => {
                            error!(
                                target: "consensus::validator",
                                "Could not find wasm bincode for contract {}: {}",
                                call.contract_id, e
                            );
                            skip = true;
                            break
                        }
                    };

                    let mut runtime =
                        match Runtime::new(&wasm, self.blockchain.clone(), call.contract_id) {
                            Ok(v) => v,
                            Err(e) => {
                                error!(
                                    target: "consensus::validator",
                                    "Failed to instantiate WASM runtime for contract {}: {}",
                                    call.contract_id, e
                                );
                                skip = true;
                                break
                            }
                        };

                    info!(target: "consensus::validator", "Executing \"apply\" call");
                    match runtime.apply(update) {
                        // TODO: FIXME: This should be done in an atomic tx/batch
                        Ok(()) => {
                            info!(target: "consensus::validator", "State update applied successfully")
                        }
                        Err(e) => {
                            error!(target: "consensus::validator", "Failed to apply state update: {}", e);
                            skip = true;
                            break
                        }
                    };
                }
                if skip {
                    warn!(target: "consensus::validator", "Skipping transaction {}", tx_hash);
                    erroneous_txs.push(tx.clone());
                    continue
                }
            } else {
                info!(target: "consensus::validator", "Skipping apply of state updates because write=false");
            }

            info!(target: "consensus::validator", "Transaction {} verified successfully", tx_hash);
        }

        Ok(erroneous_txs)
    }

    /// Append to canonical state received finalized slot checkpoints from block sync task.
    pub async fn receive_slot_checkpoints(
        &mut self,
        slot_checkpoints: &[SlotCheckpoint],
    ) -> Result<()> {
        info!(target: "consensus::validator", "receive_slot_checkpoints(): Appending slot checkpoints to ledger");
        self.blockchain.add_slot_checkpoints(slot_checkpoints)?;

        Ok(())
    }

    /// Validate and append to canonical state received finalized slot checkpoint.
    /// Returns boolean flag indicating already existing slot checkpoint.
    pub async fn receive_finalized_slot_checkpoints(
        &mut self,
        slot_checkpoint: SlotCheckpoint,
    ) -> Result<bool> {
        match self.blockchain.has_slot_checkpoint(&slot_checkpoint) {
            Ok(v) => {
                if v {
                    info!(
                        target: "consensus::validator",
                        "receive_finalized_slot_checkpoints(): Existing slot checkpoint received"
                    );
                    return Ok(false)
                }
            }
            Err(e) => {
                error!(target: "consensus::validator", "receive_finalized_slot_checkpoints(): failed checking for has_slot_checkpoint(): {}", e);
                return Ok(false)
            }
        };
        self.receive_slot_checkpoints(&[slot_checkpoint]).await?;
        Ok(true)
    }
}
