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

use async_std::sync::{Arc, RwLock};
use darkfi_sdk::{blockchain::Slot, crypto::PublicKey};
use log::{debug, error, info, warn};

use crate::{
    blockchain::{BlockInfo, Blockchain, BlockchainOverlay},
    error::TxVerifyFailed,
    tx::Transaction,
    util::time::TimeKeeper,
    Error, Result,
};

/// DarkFi consensus module
pub mod consensus;
use consensus::{next_block_reward, Consensus};

/// Verification functions
pub mod verification;
use verification::{verify_block, verify_transactions};

/// Helper utilities
pub mod utils;
use utils::deploy_native_contracts;

/// Configuration for initializing [`Validator`]
#[derive(Clone)]
pub struct ValidatorConfig {
    /// Helper structure to calculate time related operations
    pub time_keeper: TimeKeeper,
    /// Genesis block
    pub genesis_block: BlockInfo,
    /// Whitelisted faucet pubkeys (testnet stuff)
    pub faucet_pubkeys: Vec<PublicKey>,
    /// Flag to enable testing mode
    pub testing_mode: bool,
}

impl ValidatorConfig {
    pub fn new(
        time_keeper: TimeKeeper,
        genesis_block: BlockInfo,
        faucet_pubkeys: Vec<PublicKey>,
        testing_mode: bool,
    ) -> Self {
        Self { time_keeper, genesis_block, faucet_pubkeys, testing_mode }
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
    /// Flag to enable testing mode
    pub testing_mode: bool,
}

impl Validator {
    pub async fn new(db: &sled::Db, config: ValidatorConfig) -> Result<ValidatorPtr> {
        info!(target: "validator", "Initializing Validator");
        let testing_mode = config.testing_mode;

        info!(target: "validator", "Initializing Blockchain");
        let blockchain = Blockchain::new(db)?;

        // Create an overlay over whole blockchain so we can write stuff
        let overlay = BlockchainOverlay::new(&blockchain)?;

        // Add genesis block if blockchain is empty
        if blockchain.genesis().is_err() {
            info!(target: "validator", "Appending genesis block");
            verify_block(
                &overlay,
                &config.time_keeper,
                &config.genesis_block,
                None,
                0,
                testing_mode,
            )
            .await?;
        };

        // Deploy native wasm contracts
        deploy_native_contracts(&overlay, &config.time_keeper, &config.faucet_pubkeys)?;

        // Write the changes to the actual chain db
        overlay.lock().unwrap().overlay.lock().unwrap().apply()?;

        info!(target: "validator", "Initializing Consensus");
        let consensus = Consensus::new(blockchain.clone(), config.time_keeper);

        // Create the actual state
        let state = Arc::new(RwLock::new(Self { blockchain, consensus, testing_mode }));
        info!(target: "validator", "Finished initializing validator");

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

    /// Validate a set of [`BlockInfo`] in sequence and apply them if all are valid.
    pub async fn add_blocks(&self, blocks: &[BlockInfo]) -> Result<()> {
        debug!(target: "validator", "Instantiating BlockchainOverlay");
        let overlay = BlockchainOverlay::new(&self.blockchain)?;

        // Retrieve last block. If blockchain is empty it will error out here
        let last_block = match overlay.lock().unwrap().last_block() {
            Ok(l) => l,
            Err(_) => BlockInfo::default(),
        };
        // We only need the reference, thats why we do it like this
        let mut previous =
            if !overlay.lock().unwrap().is_empty()? { Some(&last_block) } else { None };

        // Create a time keeper to validate each block
        let mut time_keeper = self.consensus.time_keeper.clone();

        // Validate and insert each block
        for block in blocks {
            // Use block slot in time keeper
            time_keeper.verifying_slot = block.header.slot;

            // Retrieve expected reward
            let expected_reward = next_block_reward();

            // Verify block
            if verify_block(
                &overlay,
                &time_keeper,
                block,
                previous,
                expected_reward,
                self.testing_mode,
            )
            .await
            .is_err()
            {
                error!(target: "validator", "Erroneous block found in set");
                overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
                return Err(Error::BlockIsInvalid(block.blockhash().to_string()))
            };

            // Use last inserted block as next iteration previous
            previous = Some(block);
        }

        debug!(target: "validator", "Applying overlay changes");
        overlay.lock().unwrap().overlay.lock().unwrap().apply()?;
        Ok(())
    }

    /// Validate a set of [`Transaction`] in sequence and apply them if all are valid.
    /// In case any of the transactions fail, they will be returned to the caller.
    /// The function takes a boolean called `write` which tells it to actually write
    /// the state transitions to the database.
    pub async fn add_transactions(
        &self,
        txs: &[Transaction],
        verifying_slot: u64,
        write: bool,
    ) -> Result<()> {
        debug!(target: "validator", "Instantiating BlockchainOverlay");
        let overlay = BlockchainOverlay::new(&self.blockchain)?;

        // Generate a time keeper using transaction verifying slot
        let time_keeper = TimeKeeper::new(
            self.consensus.time_keeper.genesis_ts,
            self.consensus.time_keeper.epoch_length,
            self.consensus.time_keeper.slot_time,
            verifying_slot,
        );

        // Verify all transactions and get erroneous ones
        let erroneous_txs = verify_transactions(&overlay, &time_keeper, txs).await?;

        let lock = overlay.lock().unwrap();
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

    /// Append to canonical state received slot.
    /// This should be only used for test purposes.
    pub async fn receive_test_slot(&mut self, slot: &Slot) -> Result<()> {
        debug!(target: "validator", "receive_slot(): Appending slot to ledger");
        self.blockchain.slots.insert(&[slot.clone()])?;

        Ok(())
    }

    /// Retrieve all existing blocks and try to apply them
    /// to an in memory overlay to verify their correctness.
    /// Be careful as this will try to load everything in memory.
    pub async fn validate_blockchain(&self) -> Result<()> {
        let blocks = self.blockchain.get_all()?;

        // An empty blockchain is considered valid
        if blocks.is_empty() {
            return Ok(())
        }

        // Create an in memory blockchain overlay
        let sled_db = sled::Config::new().temporary(true).open()?;
        let blockchain = Blockchain::new(&sled_db)?;
        let overlay = BlockchainOverlay::new(&blockchain)?;

        // Set previous
        let mut previous = None;

        // Create a time keeper to validate each block
        let mut time_keeper = self.consensus.time_keeper.clone();

        // Validate and insert each block
        for block in &blocks {
            // Use block slot in time keeper
            time_keeper.verifying_slot = block.header.slot;

            // Retrieve expected reward
            let expected_reward = next_block_reward();

            // Verify block
            if verify_block(
                &overlay,
                &time_keeper,
                block,
                previous,
                expected_reward,
                self.testing_mode,
            )
            .await
            .is_err()
            {
                error!(target: "validator", "Erroneous block found in set");
                overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
                return Err(Error::BlockIsInvalid(block.blockhash().to_string()))
            };

            // Use last inserted block as next iteration previous
            previous = Some(block);
        }

        Ok(())
    }
}
