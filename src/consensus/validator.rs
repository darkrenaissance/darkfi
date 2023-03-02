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
        contract_id::{CONSENSUS_CONTRACT_ID, DAO_CONTRACT_ID, MONEY_CONTRACT_ID},
        schnorr::{SchnorrPublic, SchnorrSecret},
        MerkleNode, PublicKey, SecretKey,
    },
    incrementalmerkletree::{bridgetree::BridgeTree, Tree},
    pasta::{group::ff::PrimeField, pallas},
};
use darkfi_serial::{serialize, Decodable, Encodable, WriteExt};
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
    blockchain::{Blockchain, BlockchainOverlay, BlockchainOverlayPtr},
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
    /// A map of various subscribers exporting live info from the blockchain
    /// TODO: Instead of JsonNotification, it can be an enum of internal objects,
    ///       and then we don't have to deal with json in this module but only
    //        externally.
    pub subscribers: HashMap<&'static str, SubscriberPtr<JsonNotification>>,
    /// Wallet interface
    pub wallet: WalletPtr,
    /// Flag signalling node has finished initial sync
    pub synced: bool,
    /// Flag to enable single-node mode
    pub single_node: bool,
}

impl ValidatorState {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        db: &sled::Db,
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
        );

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
        let consensus_contract_deploy_payload = vec![];

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
            (
                "Consensus Contract",
                *CONSENSUS_CONTRACT_ID,
                include_bytes!("../contract/consensus/consensus_contract.wasm").to_vec(),
                consensus_contract_deploy_payload,
            ),
        ];

        info!(target: "consensus::validator", "Deploying native wasm contracts");
        let blockchain_overlay = BlockchainOverlay::new(&blockchain)?;
        for nc in native_contracts {
            info!(target: "consensus::validator", "Deploying {} with ContractID {}", nc.0, nc.1);
            let mut runtime = Runtime::new(
                &nc.2[..],
                blockchain_overlay.clone(),
                nc.1,
                consensus.time_keeper.clone(),
            )?;
            runtime.deploy(&nc.3)?;
            info!(target: "consensus::validator", "Successfully deployed {}", nc.0);
        }
        blockchain_overlay.lock().unwrap().overlay.lock().unwrap().apply()?;

        info!(target: "consensus::validator", "Finished deployment of native wasm contracts");
        // -----END NATIVE WASM CONTRACTS-----

        // Here we initialize various subscribers that can export live consensus/blockchain data.
        let mut subscribers = HashMap::new();
        let block_subscriber = Subscriber::new();
        let err_txs_subscriber = Subscriber::new();
        subscribers.insert("blocks", block_subscriber);
        subscribers.insert("err_txs", err_txs_subscriber);

        let state = Arc::new(RwLock::new(ValidatorState {
            lead_proving_key,
            lead_verifying_key,
            consensus,
            blockchain,
            subscribers,
            wallet,
            synced: false,
            single_node,
        }));

        Ok(state)
    }

    /// The node retrieves a transaction, validates its state transition,
    /// and appends it to the pending txs store.
    pub async fn append_tx(&mut self, tx: Transaction) -> bool {
        let tx_hash = blake3::hash(&serialize(&tx));
        let tx_in_txstore = match self.blockchain.transactions.contains(&tx_hash) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "consensus::validator", "append_tx(): Failed querying txstore: {}", e);
                return false
            }
        };

        let tx_in_pending_txs_store = match self.blockchain.pending_txs.contains(&tx_hash) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "consensus::validator", "append_tx(): Failed querying pending txs store: {}", e);
                return false
            }
        };

        if tx_in_txstore || tx_in_pending_txs_store {
            info!(target: "consensus::validator", "append_tx(): We have already seen this tx.");
            return false
        }

        info!(target: "consensus::validator", "append_tx(): Starting state transition validation");
        match self.verify_transactions(&[tx.clone()], false).await {
            Ok(erroneous_txs) => {
                if !erroneous_txs.is_empty() {
                    error!(target: "consensus::validator", "append_tx(): Erroneous transaction detected");
                    return false
                }
            }
            Err(e) => {
                error!(target: "consensus::validator", "append_tx(): Failed to verify transaction: {}", e);
                return false
            }
        }

        if let Err(e) = self.blockchain.add_pending_txs(&[tx]) {
            error!(target: "consensus::validator", "append_tx(): Failed to insert transaction to pending txs store: {}", e);
            return false
        }
        info!(target: "consensus::validator", "append_tx(): Appended tx to pending txs store");
        true
    }

    /// The node retrieves transactions vector, validates their state transition,
    /// and appends successfull ones to the pending txs store.
    pub async fn append_pending_txs(&mut self, txs: &[Transaction]) {
        let mut filtered_txs = vec![];
        // Filter already seen transactions
        for tx in txs {
            let tx_hash = blake3::hash(&serialize(tx));
            let tx_in_txstore = match self.blockchain.transactions.contains(&tx_hash) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "consensus::validator", "append_pending_txs(): Failed querying txstore: {}", e);
                    continue
                }
            };

            let tx_in_pending_txs_store = match self.blockchain.pending_txs.contains(&tx_hash) {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "consensus::validator", "append_pending_txs(): Failed querying pending txs store: {}", e);
                    continue
                }
            };

            if tx_in_txstore || tx_in_pending_txs_store {
                info!(target: "consensus::validator", "append_pending_txs(): We have already seen this tx.");
                continue
            }

            filtered_txs.push(tx.clone());
        }

        // Verify transactions and filter erroneous ones
        info!(target: "consensus::validator", "append_pending_txs(): Starting state transition validation");
        let erroneous_txs = match self.verify_transactions(&filtered_txs[..], false).await {
            Ok(erroneous_txs) => erroneous_txs,
            Err(e) => {
                error!(target: "consensus::validator", "append_pending_txs(): Failed to verify transactions: {}", e);
                return
            }
        };
        if !erroneous_txs.is_empty() {
            filtered_txs.retain(|x| !erroneous_txs.contains(x));
        }

        if let Err(e) = self.blockchain.add_pending_txs(&filtered_txs) {
            error!(target: "consensus::validator", "append_pending_txs(): Failed to insert transactions to pending txs store: {}", e);
            return
        }
        info!(target: "consensus::validator", "append_pending_txs(): Appended tx to pending txs store");
    }

    /// The node removes erroneous transactions from the pending txs store.
    async fn purge_pending_txs(&self) -> Result<()> {
        info!(target: "consensus::validator", "purge_pending_txs(): Removing erroneous transactions from pending transactions store...");
        let pending_txs = self.blockchain.get_pending_txs()?;
        if pending_txs.is_empty() {
            info!(target: "consensus::validator", "purge_pending_txs(): No pending transactions found");
            return Ok(())
        }
        let erroneous_txs = self.verify_transactions(&pending_txs[..], false).await?;
        if erroneous_txs.is_empty() {
            info!(target: "consensus::validator", "purge_pending_txs(): No erroneous transactions found");
            return Ok(())
        }
        info!(target: "consensus::validator", "purge_pending_txs(): Removing {} erroneous transactions...", erroneous_txs.len());
        self.blockchain.remove_pending_txs(&erroneous_txs)?;

        // TODO: Don't hardcode this:
        let err_txs_subscriber = self.subscribers.get("err_txs").unwrap();
        for err_tx in erroneous_txs {
            let tx_hash = blake3::hash(&serialize(&err_tx)).to_hex().as_str().to_string();
            let params = json!([bs58::encode(&serialize(&tx_hash)).into_string()]);
            let notif = JsonNotification::new("blockchain.subscribe_err_txs", params);
            info!(target: "consensus::validator", "purge_pending_txs(): Sending notification about erroneous transaction");
            err_txs_subscriber.notify(notif).await;
        }

        Ok(())
    }

    /// Generate a block proposal for the current slot, containing all
    /// pending transactions. Proposal extends the longest fork
    /// chain the node is holding.
    pub async fn propose(
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
        let mut unproposed_txs = self.unproposed_txs(fork_index)?;
        // Verify transactions and filter erroneous ones
        let erroneous_txs = self.verify_transactions(&unproposed_txs[..], false).await?;
        if !erroneous_txs.is_empty() {
            unproposed_txs.retain(|x| !erroneous_txs.contains(x));
        }
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
            eta,
            pallas::Base::from(self.consensus.time_keeper.current_slot()),
            self.lead_proving_key.as_ref().unwrap(),
            derived_blind,
        );

        // Signing using coin
        let secret_key = coin.coin1_sk;
        let header = Header::new(
            prev_hash,
            self.consensus.time_keeper.slot_epoch(slot),
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

    /// Retrieve all pending transactions not proposed in previous blocks
    /// of provided index chain.
    pub fn unproposed_txs(&self, index: i64) -> Result<Vec<Transaction>> {
        let unproposed_txs = if index == -1 {
            // If index is -1 (canonical blockchain) a new fork will be generated,
            // therefore all unproposed transactions can be included in the proposal.
            self.blockchain.get_pending_txs()?
        } else {
            // We iterate over the fork chain proposals to find already proposed
            // transactions and remove them from the local unproposed_txs vector.
            let mut filtered_txs = self.blockchain.get_pending_txs()?;
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
            return Ok(unproposed_txs[0..cap].to_vec())
        }

        Ok(unproposed_txs)
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
        let current = self.consensus.time_keeper.current_slot();
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
                self.consensus.time_keeper.current_slot(),
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
        match self.verify_transactions(&proposal.block.txs, false).await {
            Ok(erroneous_txs) => {
                if !erroneous_txs.is_empty() {
                    error!(target: "consensus::validator", "Proposal contains erroneous transactions");
                    return Err(Error::ErroneousTxsDetected)
                }
            }
            Err(e) => {
                error!(target: "consensus::validator", "receive_proposal(): Transaction verifications failed: {}", e);
                return Err(e)
            }
        }

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

    /// Node checks if any of the fork chains can be finalized.
    /// Consensus finalization logic:
    /// - If the node has observed the creation of a fork chain and no other forks exists at same or greater height,
    ///   it finalizes (appends to canonical blockchain) all proposals in that fork chain.
    /// When fork chain proposals are finalized, the rest of fork chains are removed and all
    /// slot checkpoints are apppended to canonical state.
    pub async fn chain_finalization(&mut self) -> Result<(Vec<BlockInfo>, Vec<SlotCheckpoint>)> {
        let slot = self.consensus.time_keeper.current_slot();
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
        for proposal in &finalized {
            // TODO: Is this the right place? We're already doing this in protocol_sync.
            // TODO: These state transitions have already been checked. (I wrote this, but where?)
            // TODO: FIXME: The state transitions have already been written, they have to be in memory
            //              until this point.
            info!(target: "consensus::validator", "Applying state transition for finalized block");
            match self.verify_transactions(&proposal.txs, true).await {
                Ok(erroneous_txs) => {
                    if !erroneous_txs.is_empty() {
                        error!(target: "consensus::validator", "Finalized block contains erroneous transactions");
                        return Err(Error::ErroneousTxsDetected)
                    }
                }
                Err(e) => {
                    error!(target: "consensus::validator", "Finalized block transaction verifications failed: {}", e);
                    return Err(e)
                }
            }

            // Remove proposal transactions from pending txs store
            if let Err(e) = self.blockchain.remove_pending_txs(&proposal.txs) {
                error!(target: "consensus::validator", "Removing finalized block transactions failed: {}", e);
                return Err(e)
            }

            // TODO: Don't hardcode this:
            let params = json!([bs58::encode(&serialize(proposal)).into_string()]);
            let notif = JsonNotification::new("blockchain.subscribe_blocks", params);
            info!(target: "consensus::validator", "consensus: Sending notification about finalized block");
            blocks_subscriber.notify(notif).await;
        }

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

        // Purge pending erroneous txs since canonical state has been changed
        if let Err(e) = self.purge_pending_txs().await {
            error!(target: "consensus::validator", "consensus: Purging pending transactions failed: {}", e);
        }

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
    async fn receive_blocks(&mut self, blocks: &[BlockInfo]) -> Result<()> {
        // Verify state transitions for all blocks and their respective transactions.
        info!(target: "consensus::validator", "receive_blocks(): Starting state transition validations");

        for block in blocks {
            match self.verify_transactions(&block.txs, true).await {
                Ok(erroneous_txs) => {
                    if !erroneous_txs.is_empty() {
                        error!(target: "consensus::validator", "receive_blocks(): Block contains erroneous transactions");
                        return Err(Error::ErroneousTxsDetected)
                    }
                }
                Err(e) => {
                    error!(target: "consensus::validator", "receive_blocks(): Transaction verifications failed: {}", e);
                    return Err(e)
                }
            }
        }

        info!(target: "consensus::validator", "receive_blocks(): All state transitions passed. Appending blocks to ledger.");
        self.blockchain.add(blocks)?;

        Ok(())
    }

    /// Validate and append to canonical state received finalized block.
    /// Returns boolean flag indicating already existing block.
    pub async fn receive_finalized_block(&mut self, block: BlockInfo) -> Result<bool> {
        if block.header.slot > self.consensus.time_keeper.current_slot() {
            warn!(target: "consensus::validator", "receive_finalized_block(): Ignoring future block: {}", block.header.slot);
            return Ok(false)
        }
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

        info!(target: "consensus::validator", "receive_finalized_block(): Removing block transactions from pending txs store");
        self.blockchain.remove_pending_txs(&block.txs)?;

        // Purge pending erroneous txs since canonical state has been changed
        if let Err(e) = self.purge_pending_txs().await {
            error!(target: "consensus::validator", "receive_finalized_block(): Purging pending transactions failed: {}", e);
        }

        Ok(true)
    }

    /// Validate and append to canonical state received finalized blocks from block sync task.
    /// Already existing blocks are ignored.
    pub async fn receive_sync_blocks(&mut self, blocks: &[BlockInfo]) -> Result<()> {
        let mut new_blocks = vec![];
        for block in blocks {
            if block.header.slot > self.consensus.time_keeper.current_slot() {
                warn!(target: "consensus::validator", "receive_sync_blocks(): Ignoring future block: {}", block.header.slot);
                continue
            }
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

    /// Validate signatures, wasm execution, and zk proofs for given transaction in
    /// provided runtimes. If all of those succeed, try to execute a state update
    /// for the contract calls.
    async fn verify_transaction(
        &self,
        blockchain_overlay: BlockchainOverlayPtr,
        tx: &Transaction,
    ) -> Result<()> {
        let mut runtimes = HashMap::new();
        let tx_hash = blake3::hash(&serialize(tx));
        info!(target: "consensus::validator", "Verifying transaction {}", tx_hash);

        // Table of public inputs used for ZK proof verification
        let mut zkp_table = vec![];
        // Table of public keys used for signature verification
        let mut sig_table = vec![];
        // State updates produced by contract execution
        let mut updates = vec![];
        // Map of zk proof verifying keys for the current transaction
        let mut verifying_keys: HashMap<[u8; 32], HashMap<String, VerifyingKey>> = HashMap::new();

        // Initialize the map
        for call in tx.calls.iter() {
            verifying_keys.insert(call.contract_id.to_bytes(), HashMap::new());
        }

        // Iterate over all calls to get the metadata
        for (idx, call) in tx.calls.iter().enumerate() {
            info!(target: "consensus::validator", "Executing contract call {}", idx);

            // Write the actual payload data
            let mut payload = vec![];
            payload.write_u32(idx as u32)?; // Call index
            tx.calls.encode(&mut payload)?; // Actual call data

            // Instantiate the wasm runtime
            let runtime_key = call.contract_id.to_string();
            if !runtimes.contains_key(&runtime_key) {
                let wasm = self.blockchain.wasm_bincode.get(call.contract_id)?;
                let r = Runtime::new(
                    &wasm,
                    blockchain_overlay.clone(),
                    call.contract_id,
                    self.consensus.time_keeper.clone(),
                )?;
                runtimes.insert(runtime_key.clone(), r);
            }
            let runtime = runtimes.get_mut(&runtime_key).unwrap();

            info!(target: "consensus::validator", "Executing \"metadata\" call");
            let metadata = runtime.metadata(&payload)?;

            // Decode the metadata retrieved from the execution
            let mut decoder = Cursor::new(&metadata);

            // The tuple is (zkas_ns, public_inputs)
            let zkp_pub: Vec<(String, Vec<pallas::Base>)> = Decodable::decode(&mut decoder)?;

            let sig_pub: Vec<PublicKey> = Decodable::decode(&mut decoder)?;
            // TODO: Make sure we've read all the bytes above.
            info!(target: "consensus::validator", "Successfully executed \"metadata\" call");

            // Here we'll look up verifying keys and insert them into the per-contract map.
            info!(target: "consensus::validator", "Performing VerifyingKey lookups from the sled db");
            for (zkas_ns, _) in &zkp_pub {
                let inner_vk_map = verifying_keys.get_mut(&call.contract_id.to_bytes()).unwrap();

                if inner_vk_map.contains_key(zkas_ns.as_str()) {
                    continue
                }

                let (_, vk) = self.blockchain.contracts.get_zkas(
                    &self.blockchain.sled_db,
                    &call.contract_id,
                    zkas_ns,
                )?;

                inner_vk_map.insert(zkas_ns.to_string(), vk);
            }

            zkp_table.push(zkp_pub);
            sig_table.push(sig_pub);

            // After getting the metadata, we run the "exec" function with the same
            // runtime and the same payload.
            info!(target: "consensus::validator", "Executing \"exec\" call");
            let state_update = runtime.exec(&payload)?;

            info!(target: "consensus::validator", "Successfully executed \"exec\" call");
            updates.push(state_update);

            // At this point we're done with the call and move on to the next one.
        }

        // When we're done looping and executing over the tx's contract calls, we
        // move on with verification. First we verify the signatures as that's
        // cheaper, and then finally we verify the ZK proofs.
        info!(target: "consensus::validator", "Verifying signatures for transaction {}", tx_hash);
        if sig_table.len() != tx.signatures.len() {
            error!(target: "consensus::validator", "Incorrect number of signatures in tx {}", tx_hash);
            return Err(Error::InvalidSignature)
        }

        match tx.verify_sigs(sig_table) {
            Ok(()) => {
                info!(target: "consensus::validator", "Signatures verification for tx {} successful", tx_hash)
            }
            Err(e) => {
                error!(target: "consensus::validator", "Signature verification for tx {} failed: {}", tx_hash, e);
                return Err(e)
            }
        };

        info!(target: "consensus::validator", "Verifying ZK proofs for transaction {}", tx_hash);
        match tx.verify_zkps(verifying_keys.clone(), zkp_table).await {
            Ok(()) => {
                info!(target: "consensus::validator", "ZK proof verification for tx {} successful", tx_hash)
            }
            Err(e) => {
                error!(target: "consensus::validator", "ZK proof verification for tx {} failed: {}", tx_hash, e);
                return Err(e)
            }
        };

        // After the verifications stage passes we can apply the state updates.
        assert!(tx.calls.len() == updates.len());

        info!(target: "consensus::validator", "Performing state updates");
        for (call, update) in tx.calls.iter().zip(updates.iter()) {
            // Retrieve already initiated runtime and apply update
            // TODO: Sum up the gas costs of previous calls during execution
            //       and verification and these.
            let runtime = runtimes.get_mut(&call.contract_id.to_string()).unwrap();
            info!(target: "consensus::validator", "Executing \"apply\" call");
            runtime.apply(update)?;
            info!(target: "consensus::validator", "State update applied successfully")
        }

        info!(target: "consensus::validator", "Transaction {} verified successfully", tx_hash);

        Ok(())
    }

    /// Validate a set of [`Transaction`] in sequence and apply them if all are valid.
    /// Erroneous transactions are filtered out of the set and returned to caller.
    /// The function takes a boolean called `write` which tells it to actually write
    /// the state transitions to the database.
    pub async fn verify_transactions(
        &self,
        txs: &[Transaction],
        write: bool,
    ) -> Result<Vec<Transaction>> {
        info!(target: "consensus::validator", "Verifying {} transaction(s)", txs.len());

        let mut erroneous_txs = vec![];
        let blockchain_overlay = BlockchainOverlay::new(&self.blockchain)?;

        for tx in txs {
            if let Err(e) = self.verify_transaction(blockchain_overlay.clone(), tx).await {
                warn!(target: "consensus::validator", "Transaction verification failed: {}", e);
                erroneous_txs.push(tx.clone());
            }
        }

        let lock = blockchain_overlay.lock().unwrap();
        let overlay = lock.overlay.lock().unwrap();
        if !erroneous_txs.is_empty() {
            warn!(target: "consensus::validator", "Erroneous transactions found in set");
            overlay.purge_new_trees()?;
            return Ok(erroneous_txs)
        }

        if !write {
            info!(target: "consensus::validator", "Skipping apply of state updates because write=false");
            overlay.purge_new_trees()?;
            return Ok(erroneous_txs)
        }

        overlay.apply()?;

        Ok(erroneous_txs)
    }

    /// Append to canonical state received finalized slot checkpoints from block sync task.
    pub async fn receive_slot_checkpoints(
        &mut self,
        slot_checkpoints: &[SlotCheckpoint],
    ) -> Result<()> {
        info!(target: "consensus::validator", "receive_slot_checkpoints(): Appending slot checkpoints to ledger");
        let mut filtered = vec![];
        for slot_checkpoint in slot_checkpoints {
            if slot_checkpoint.slot > self.consensus.time_keeper.current_slot() {
                warn!(target: "consensus::validator", "receive_slot_checkpoints(): Ignoring future slot checkpoint: {}", slot_checkpoint.slot);
                continue
            }
            filtered.push(slot_checkpoint.clone());
        }
        self.blockchain.add_slot_checkpoints(&filtered[..])?;

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
