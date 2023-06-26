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
    blockchain::Slot,
    crypto::{PublicKey, CONSENSUS_CONTRACT_ID, DAO_CONTRACT_ID, MONEY_CONTRACT_ID},
    pasta::pallas,
};
use darkfi_serial::{serialize, Decodable, Encodable, WriteExt};
use log::{debug, error, info, warn};

use crate::{
    blockchain::{Blockchain, BlockchainOverlay, BlockchainOverlayPtr},
    error::TxVerifyFailed,
    runtime::vm_runtime::Runtime,
    tx::Transaction,
    util::time::TimeKeeper,
    zk::VerifyingKey,
    Result,
};

/// DarkFi blockchain
pub mod blockchain;

/// DarkFi consensus
pub mod consensus;
use consensus::Consensus;

/// Configuration for initializing [`Validator`]
pub struct ValidatorConfig {
    /// Helper structure to calculate time related operations
    pub time_keeper: TimeKeeper,
    /// Genesis block
    pub genesis_block: blake3::Hash,
    /// Whitelisted faucet pubkeys (testnet stuff)
    pub faucet_pubkeys: Vec<PublicKey>,
}

impl ValidatorConfig {
    pub fn new(
        time_keeper: TimeKeeper,
        genesis_block: blake3::Hash,
        faucet_pubkeys: Vec<PublicKey>,
    ) -> Self {
        Self { time_keeper, genesis_block, faucet_pubkeys }
    }
}

/// Atomic pointer to validator.
pub type ValidatorPtr = Arc<RwLock<Validator>>;

/// This struct represents a DarkFi validator node.
pub struct Validator {
    /// Canonical (finalized) blockchain
    pub blockchain: Blockchain,
    /// Hot/Live data used by the consensus algorithm
    pub consensus: Consensus,
}

impl Validator {
    pub async fn new(db: &sled::Db, config: ValidatorConfig) -> Result<ValidatorPtr> {
        info!(target: "validator", "Initializing Validator");

        info!(target: "validator", "Initializing Blockchain");
        // TODO: Initialize chain, then check if its empty, so we can add the
        // genesis block and its transactions
        let blockchain = Blockchain::new(db, config.time_keeper.genesis_ts, config.genesis_block)?;

        info!(target: "validator", "Initializing Consensus");
        let consensus =
            Consensus::new(blockchain.clone(), config.time_keeper, config.genesis_block);

        // =====================
        // NATIVE WASM CONTRACTS
        // =====================
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
        // in the Money contract.
        let money_contract_deploy_payload = serialize(&config.faucet_pubkeys);

        // The DAO contract uses an empty payload to deploy itself.
        let dao_contract_deploy_payload = vec![];

        // The Consensus contract uses an empty payload to deploy itself.
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

        info!(target: "validator", "Deploying native WASM contracts");
        let blockchain_overlay = BlockchainOverlay::new(&blockchain)?;

        for nc in native_contracts {
            info!(target: "validator", "Deploying {} with ContractID {}", nc.0, nc.1);

            let mut runtime = Runtime::new(
                &nc.2[..],
                blockchain_overlay.clone(),
                nc.1,
                consensus.time_keeper.clone(),
            )?;

            runtime.deploy(&nc.3)?;

            info!(target: "validator", "Successfully deployed {}", nc.0);
        }

        // Write the changes to the actual chain db
        blockchain_overlay.lock().unwrap().overlay.lock().unwrap().apply()?;

        info!(target: "validator", "Finished deployment of native WASM contracts");

        // Create the actual state
        let state = Arc::new(RwLock::new(Self { blockchain, consensus }));

        Ok(state)
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

    /// Append to canonical state received finalized slots from block sync task.
    // TODO: integrate this to receive_blocks, as slots will be part of received block.
    pub async fn receive_slots(&mut self, slots: &[Slot]) -> Result<()> {
        debug!(target: "validator", "receive_slots(): Appending slots to ledger");
        let current_slot = self.consensus.time_keeper.current_slot();
        let mut filtered = vec![];
        for slot in slots {
            if slot.id > current_slot {
                warn!(target: "validator", "receive_slots(): Ignoring future slot: {}", slot.id);
                continue
            }
            filtered.push(slot.clone());
        }
        self.blockchain.add_slots(&filtered[..])?;

        Ok(())
    }

    /// Validate WASM execution, signatures, and ZK proofs for a given [`Transaction`].
    async fn verify_transaction(
        &self,
        blockchain_overlay: BlockchainOverlayPtr,
        tx: &Transaction,
        time_keeper: &TimeKeeper,
        verifying_keys: &mut HashMap<[u8; 32], HashMap<String, VerifyingKey>>,
    ) -> Result<()> {
        let tx_hash = tx.hash();
        debug!(target: "validator", "Validating transaction {}", tx_hash);

        // Table of public inputs used for ZK proof verification
        let mut zkp_table = vec![];
        // Table of public keys used for signature verification
        let mut sig_table = vec![];

        // Iterate over all calls to get the metadata
        for (idx, call) in tx.calls.iter().enumerate() {
            debug!(target: "validator", "Executing contract call {}", idx);

            // Write the actual payload data
            let mut payload = vec![];
            payload.write_u32(idx as u32)?; // Call index
            tx.calls.encode(&mut payload)?; // Actual call data

            debug!(target: "validator", "Instantiating WASM runtime");
            let wasm = self.blockchain.wasm_bincode.get(call.contract_id)?;

            let mut runtime = Runtime::new(
                &wasm,
                blockchain_overlay.clone(),
                call.contract_id,
                time_keeper.clone(),
            )?;

            debug!(target: "validator", "Executing \"metadata\" call");
            let metadata = runtime.metadata(&payload)?;

            // Decode the metadata retrieved from the execution
            let mut decoder = Cursor::new(&metadata);

            // The tuple is (zkasa_ns, public_inputs)
            let zkp_pub: Vec<(String, Vec<pallas::Base>)> = Decodable::decode(&mut decoder)?;
            let sig_pub: Vec<PublicKey> = Decodable::decode(&mut decoder)?;
            // TODO: Make sure we've read all the bytes above.
            debug!(target: "validator", "Successfully executed \"metadata\" call");

            // Here we'll look up verifying keys and insert them into the per-contract map.
            debug!(target: "validator", "Performing VerifyingKey lookups from the sled db");
            for (zkas_ns, _) in &zkp_pub {
                let inner_vk_map = verifying_keys.get_mut(&call.contract_id.to_bytes()).unwrap();

                // TODO: This will be a problem in case of ::deploy, unless we force a different
                // namespace and disable updating existing circuit. Might be a smart idea to do
                // so in order to have to care less about being able to verify historical txs.
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

            // After getting the metadata, we run the "exec" function with the same runtime
            // and the same payload.
            debug!(target: "validator", "Executing \"exec\" call");
            let state_update = runtime.exec(&payload)?;
            debug!(target: "validator", "Successfully executed \"exec\" call");

            // If that was successful, we apply the state update in the ephemeral overlay.
            debug!(target: "validator", "Executing \"apply\" call");
            runtime.apply(&state_update)?;
            debug!(target: "validator", "Successfully executed \"apply\" call");

            // At this point we're done with the call and move on to the next one.
        }

        // When we're done looping and executing over the tx's contract calls, we now
        // move on with verification. First we verify the signatures as that's cheaper,
        // and then finally we verify the ZK proofs.
        debug!(target: "validator", "Verifying signatures for transaction {}", tx_hash);
        if sig_table.len() != tx.signatures.len() {
            error!(target: "validator", "Incorrect number of signatures in tx {}", tx_hash);
            return Err(TxVerifyFailed::MissingSignatures.into())
        }

        // TODO: Go through the ZK circuits that have to be verified and account for the opcodes.

        if let Err(e) = tx.verify_sigs(sig_table) {
            error!(target: "validator", "Signature verification for tx {} failed: {}", tx_hash, e);
            return Err(TxVerifyFailed::InvalidSignature.into())
        }

        debug!(target: "validator", "Signature verification successful");

        debug!(target: "validator", "Verifying ZK proofs for transaction {}", tx_hash);
        if let Err(e) = tx.verify_zkps(verifying_keys, zkp_table).await {
            error!(target: "consensus::validator", "ZK proof verification for tx {} failed: {}", tx_hash, e);
            return Err(TxVerifyFailed::InvalidZkProof.into())
        }

        debug!(target: "validator", "ZK proof verification successful");
        debug!(target: "validator", "Transaction {} verified successfully", tx_hash);

        Ok(())
    }

    /// Validate a set of [`Transaction`] in sequence and apply them if all are valid.
    /// In case any of the transactions fail, they will be returned to the caller.
    /// The function takes a boolean called `write` which tells it to actually write
    /// the state transitions to the database.
    pub async fn verify_transactions(
        &self,
        txs: &[Transaction],
        verifying_slot: u64,
        write: bool,
    ) -> Result<()> {
        debug!(target: "validator", "Verifying {} transactions", txs.len());

        debug!(target: "validator", "Instantiating BlockchainOverlay");
        let blockchain_overlay = BlockchainOverlay::new(&self.blockchain)?;

        // Tracker for failed txs
        let mut erroneous_txs = vec![];

        // Map of ZK proof verifying keys for the current transaction batch
        let mut vks: HashMap<[u8; 32], HashMap<String, VerifyingKey>> = HashMap::new();

        // Initialize the map
        for tx in txs {
            for call in &tx.calls {
                vks.insert(call.contract_id.to_bytes(), HashMap::new());
            }
        }

        // Generate a time keeper using transaction verifying slot
        let time_keeper = TimeKeeper::new(
            self.consensus.time_keeper.genesis_ts,
            self.consensus.time_keeper.epoch_length,
            self.consensus.time_keeper.slot_time,
            verifying_slot,
        );

        // Iterate over transactions and attempt to verify them
        for tx in txs {
            blockchain_overlay.lock().unwrap().checkpoint();
            if let Err(e) = self
                .verify_transaction(blockchain_overlay.clone(), tx, &time_keeper, &mut vks)
                .await
            {
                warn!(target: "validator", "Transaction verification failed: {}", e);
                erroneous_txs.push(tx.clone());
                // TODO: verify this works as expected
                blockchain_overlay.lock().unwrap().revert_to_checkpoint()?;
            }
        }

        let lock = blockchain_overlay.lock().unwrap();
        let mut overlay = lock.overlay.lock().unwrap();
        if !erroneous_txs.is_empty() {
            warn!(target: "validator", "Erroneous transactions found in set");
            overlay.purge_new_trees()?;
            return Err(TxVerifyFailed::ErroneousTxs(erroneous_txs).into())
        }

        if !write {
            debug!(target: "validator", "Skipping apply of state updates because write=false");
            overlay.purge_new_trees()?;
            return Ok(())
        }

        debug!(target: "validator", "Applying overlay changes");
        overlay.apply()?;
        Ok(())
    }
}
