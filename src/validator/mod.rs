/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use std::sync::Arc;

use darkfi_sdk::crypto::MerkleTree;
use darkfi_serial::serialize_async;
use log::{debug, error, info, warn};
use num_bigint::BigUint;
use smol::lock::RwLock;

use crate::{
    blockchain::{
        block_store::{BlockDifficulty, BlockInfo},
        Blockchain, BlockchainOverlay,
    },
    error::TxVerifyFailed,
    tx::Transaction,
    Error, Result,
};

/// DarkFi consensus module
pub mod consensus;
use consensus::{Consensus, Proposal};

/// DarkFi PoW module
pub mod pow;
use pow::PoWModule;

/// Verification functions
pub mod verification;
use verification::{
    verify_block, verify_genesis_block, verify_producer_transaction, verify_proposal,
    verify_transactions,
};

/// Fee calculation helpers
pub mod fees;

/// Helper utilities
pub mod utils;
use utils::deploy_native_contracts;

/// Configuration for initializing [`Validator`]
#[derive(Clone)]
pub struct ValidatorConfig {
    /// Currently configured finalization security threshold
    pub finalization_threshold: usize,
    /// Currently configured PoW target
    pub pow_target: usize,
    /// Optional fixed difficulty, for testing purposes
    pub pow_fixed_difficulty: Option<BigUint>,
    /// Genesis block
    pub genesis_block: BlockInfo,
    /// Flag to enable tx fee verification
    pub verify_fees: bool,
}

/// Atomic pointer to validator.
pub type ValidatorPtr = Arc<Validator>;

/// This struct represents a DarkFi validator node.
pub struct Validator {
    /// Canonical (finalized) blockchain
    pub blockchain: Blockchain,
    /// Hot/Live data used by the consensus algorithm
    pub consensus: Consensus,
    /// Flag signalling node has finished initial sync
    pub synced: RwLock<bool>,
    /// Flag to enable tx fee verification
    pub verify_fees: bool,
}

impl Validator {
    pub async fn new(db: &sled::Db, config: ValidatorConfig) -> Result<ValidatorPtr> {
        info!(target: "validator::new", "Initializing Validator");

        info!(target: "validator::new", "Initializing Blockchain");
        let blockchain = Blockchain::new(db)?;

        // Create an overlay over whole blockchain so we can write stuff
        let overlay = BlockchainOverlay::new(&blockchain)?;

        // Deploy native wasm contracts
        deploy_native_contracts(&overlay).await?;

        // Add genesis block if blockchain is empty
        if blockchain.genesis().is_err() {
            info!(target: "validator::new", "Appending genesis block");
            verify_genesis_block(&overlay, &config.genesis_block).await?;
        };

        // Write the changes to the actual chain db
        overlay.lock().unwrap().overlay.lock().unwrap().apply()?;

        info!(target: "validator::new", "Initializing Consensus");
        let consensus = Consensus::new(
            blockchain.clone(),
            config.finalization_threshold,
            config.pow_target,
            config.pow_fixed_difficulty,
        )?;

        // Create the actual state
        let state = Arc::new(Self {
            blockchain,
            consensus,
            synced: RwLock::new(false),
            verify_fees: config.verify_fees,
        });

        info!(target: "validator::new", "Finished initializing validator");
        Ok(state)
    }

    /// The node retrieves a transaction, validates its state transition,
    /// and appends it to the pending txs store.
    pub async fn append_tx(&self, tx: &Transaction, write: bool) -> Result<()> {
        let tx_hash = blake3::hash(&serialize_async(tx).await);

        // Check if we have already seen this tx
        let tx_in_txstore = self.blockchain.transactions.contains(&tx_hash)?;
        let tx_in_pending_txs_store = self.blockchain.pending_txs.contains(&tx_hash)?;

        if tx_in_txstore || tx_in_pending_txs_store {
            info!(target: "validator::append_tx", "We have already seen this tx");
            return Err(TxVerifyFailed::AlreadySeenTx(tx_hash.to_string()).into())
        }

        // Verify state transition
        info!(target: "validator::append_tx", "Starting state transition validation");
        let tx_vec = [tx.clone()];
        let mut valid = false;

        // Grab a lock over current consensus forks state
        let mut forks = self.consensus.forks.write().await;

        // If node participates in consensus and holds any forks, iterate over them
        // to verify transaction validity in their overlays
        for fork in forks.iter_mut() {
            // Clone forks' overlay
            let overlay = fork.overlay.lock().unwrap().full_clone()?;

            // Grab forks' next block height
            let next_block_height = fork.get_next_block_height()?;

            // Verify transaction
            match verify_transactions(
                &overlay,
                next_block_height,
                &tx_vec,
                &mut MerkleTree::new(1),
                false,
            )
            .await
            {
                Ok(_) => {}
                Err(crate::Error::TxVerifyFailed(TxVerifyFailed::ErroneousTxs(_))) => continue,
                Err(e) => return Err(e),
            }

            valid = true;

            // Store transaction hash in forks' mempool
            if write {
                fork.mempool.push(tx_hash);
            }
        }

        // Verify transaction against canonical state
        let overlay = BlockchainOverlay::new(&self.blockchain)?;
        let next_block_height = self.blockchain.last_block()?.header.height + 1;
        let mut erroneous_txs = vec![];
        match verify_transactions(
            &overlay,
            next_block_height,
            &tx_vec,
            &mut MerkleTree::new(1),
            false,
        )
        .await
        {
            Ok(_) => valid = true,
            Err(crate::Error::TxVerifyFailed(TxVerifyFailed::ErroneousTxs(etx))) => {
                erroneous_txs = etx
            }
            Err(e) => return Err(e),
        }

        // Drop forks lock
        drop(forks);

        // Return error if transaction is not valid for canonical or any fork
        if !valid {
            return Err(TxVerifyFailed::ErroneousTxs(erroneous_txs).into())
        }

        // Add transaction to pending txs store
        if write {
            self.blockchain.add_pending_txs(&tx_vec)?;
            info!(target: "validator::append_tx", "Appended tx to pending txs store");
        }

        Ok(())
    }

    /// The node removes invalid transactions from the pending txs store.
    pub async fn purge_pending_txs(&self) -> Result<()> {
        info!(target: "validator::purge_pending_txs", "Removing invalid transactions from pending transactions store...");

        // Check if any pending transactions exist
        let pending_txs = self.blockchain.get_pending_txs()?;
        if pending_txs.is_empty() {
            info!(target: "validator::purge_pending_txs", "No pending transactions found");
            return Ok(())
        }

        // Grab a lock over current consensus forks state
        let mut forks = self.consensus.forks.write().await;

        let mut removed_txs = vec![];
        for tx in pending_txs {
            let tx_hash = &blake3::hash(&serialize_async(&tx).await);
            let tx_vec = [tx.clone()];
            let mut valid = false;

            // If node participates in consensus and holds any forks, iterate over them
            // to verify transaction validity in their overlays
            for fork in forks.iter_mut() {
                // Clone forks' overlay
                let overlay = fork.overlay.lock().unwrap().full_clone()?;

                // Grab forks' next block height
                let next_block_height = fork.get_next_block_height()?;

                // Verify transaction
                match verify_transactions(
                    &overlay,
                    next_block_height,
                    &tx_vec,
                    &mut MerkleTree::new(1),
                    false,
                )
                .await
                {
                    Ok(_) => {
                        valid = true;
                        continue
                    }
                    Err(crate::Error::TxVerifyFailed(TxVerifyFailed::ErroneousTxs(_))) => {}
                    Err(e) => return Err(e),
                }

                // Remove erroneous transaction from forks' mempool
                fork.mempool.retain(|x| x != tx_hash);
            }

            // Verify transaction against canonical state
            let overlay = BlockchainOverlay::new(&self.blockchain)?;
            let next_block_height = self.blockchain.last_block()?.header.height + 1;
            match verify_transactions(
                &overlay,
                next_block_height,
                &tx_vec,
                &mut MerkleTree::new(1),
                false,
            )
            .await
            {
                Ok(_) => valid = true,
                Err(crate::Error::TxVerifyFailed(TxVerifyFailed::ErroneousTxs(_))) => {}
                Err(e) => return Err(e),
            }

            // Remove pending transaction if it's not valid for canonical or any fork
            if !valid {
                removed_txs.push(tx)
            }
        }

        // Drop forks lock
        drop(forks);

        if removed_txs.is_empty() {
            info!(target: "validator::purge_pending_txs", "No erroneous transactions found");
            return Ok(())
        }
        info!(target: "validator::purge_pending_txs", "Removing {} erroneous transactions...", removed_txs.len());
        self.blockchain.remove_pending_txs(&removed_txs)?;

        Ok(())
    }

    /// The node locks its consensus state and tries to append provided proposal.
    pub async fn append_proposal(&self, proposal: &Proposal) -> Result<()> {
        // Grab append lock so we restrict concurrent calls of this function
        let append_lock = self.consensus.append_lock.write().await;

        // Execute append
        self.consensus.append_proposal(proposal).await?;

        // Release append lock
        drop(append_lock);

        Ok(())
    }

    /// The node checks if best fork can be finalized.
    /// If proposals can be finalized, node appends them to canonical,
    /// and rebuilds the best fork.
    pub async fn finalization(&self) -> Result<Vec<BlockInfo>> {
        // Grab append lock so no new proposals can be appended while
        // we execute finalization
        let append_lock = self.consensus.append_lock.write().await;

        info!(target: "validator::finalization", "Performing finalization check");

        // Grab best fork index that can be finalized
        let finalized_fork = self.consensus.finalization().await?;
        if finalized_fork.is_none() {
            info!(target: "validator::finalization", "No proposals can be finalized");
            drop(append_lock);
            return Ok(vec![])
        }

        // Grab fork proposals sequence
        let forks = self.consensus.forks.read().await;
        let fork = &forks[finalized_fork.unwrap()];
        let mut proposals = fork.overlay.lock().unwrap().get_blocks_by_hash(&fork.proposals)?;
        drop(forks);

        // Find the excess over finalization threshold
        let excess = (proposals.len() - self.consensus.finalization_threshold) + 1;

        // Grab non finalized blocks
        let non_finalized = proposals.split_off(excess);

        // Append finalized blocks
        info!(target: "validator::finalization", "Finalizing proposals:");
        for block in &proposals {
            info!(target: "validator::finalization", "\t{} - {}", block.hash()?, block.header.height);
        }
        self.add_blocks(&proposals).await?;

        // Rebuild best fork using rest proposals
        self.consensus.rebuild_best_fork(&non_finalized).await?;
        info!(target: "validator::finalization", "Finalization completed!");

        // Release append lock
        drop(append_lock);

        Ok(proposals)
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
        debug!(target: "validator::add_blocks", "Instantiating BlockchainOverlay");
        let overlay = BlockchainOverlay::new(&self.blockchain)?;

        // Retrieve last block
        let mut previous = &overlay.lock().unwrap().last_block()?;

        // Grab current PoW module to validate each block
        let mut module = self.consensus.module.read().await.clone();

        // Keep track of all blocks transactions to remove them from pending txs store
        let mut removed_txs = vec![];

        // Validate and insert each block
        for block in blocks {
            // Skip already existing block
            if overlay.lock().unwrap().has_block(block)? {
                previous = block;
                continue;
            }

            // Verify block
            if verify_block(&overlay, &module, block, previous).await.is_err() {
                error!(target: "validator::add_blocks", "Erroneous block found in set");
                overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
                return Err(Error::BlockIsInvalid(block.hash()?.to_string()))
            };

            // Generate block difficulty
            let difficulty = module.next_difficulty()?;
            let cummulative_difficulty = module.cummulative_difficulty.clone() + difficulty.clone();
            let block_difficulty = BlockDifficulty::new(
                block.header.height,
                block.header.timestamp.0,
                difficulty,
                cummulative_difficulty,
            );
            module.append_difficulty(&overlay, block_difficulty)?;

            // Store block transactions
            for tx in &block.txs {
                removed_txs.push(tx.clone());
            }

            // Use last inserted block as next iteration previous
            previous = block;
        }

        debug!(target: "validator::add_blocks", "Applying overlay changes");
        overlay.lock().unwrap().overlay.lock().unwrap().apply()?;

        // Purge pending erroneous txs since canonical state has been changed
        self.blockchain.remove_pending_txs(&removed_txs)?;
        self.purge_pending_txs().await?;

        // Update PoW module
        *self.consensus.module.write().await = module;

        Ok(())
    }

    /// Validate a set of [`Transaction`] in sequence and apply them if all are valid.
    /// In case any of the transactions fail, they will be returned to the caller.
    /// The function takes a boolean called `write` which tells it to actually write
    /// the state transitions to the database.
    ///
    /// Returns the total gas used for the given transactions.
    pub async fn add_transactions(
        &self,
        txs: &[Transaction],
        verifying_block_height: u64,
        write: bool,
        verify_fees: bool,
    ) -> Result<u64> {
        debug!(target: "validator::add_transactions", "Instantiating BlockchainOverlay");
        let overlay = BlockchainOverlay::new(&self.blockchain)?;

        // Verify all transactions and get erroneous ones
        let verify_result = verify_transactions(
            &overlay,
            verifying_block_height,
            txs,
            &mut MerkleTree::new(1),
            verify_fees,
        )
        .await;

        let lock = overlay.lock().unwrap();
        let mut overlay = lock.overlay.lock().unwrap();

        if let Err(e) = verify_result {
            overlay.purge_new_trees()?;
            return Err(e)
        }

        let gas_used = verify_result.unwrap();

        if !write {
            debug!(target: "validator::add_transactions", "Skipping apply of state updates because write=false");
            overlay.purge_new_trees()?;
            return Ok(gas_used)
        }

        debug!(target: "validator::add_transactions", "Applying overlay changes");
        overlay.apply()?;
        Ok(gas_used)
    }

    /// Validate a producer `Transaction` and apply it if valid.
    /// In case the transactions fail, ir will be returned to the caller.
    /// The function takes a boolean called `write` which tells it to actually write
    /// the state transitions to the database.
    /// This should be only used for test purposes.
    pub async fn add_test_producer_transaction(
        &self,
        tx: &Transaction,
        verifying_block_height: u64,
        write: bool,
    ) -> Result<()> {
        debug!(target: "validator::add_test_producer_transaction", "Instantiating BlockchainOverlay");
        let overlay = BlockchainOverlay::new(&self.blockchain)?;

        // Verify transaction
        let mut erroneous_txs = vec![];
        if let Err(e) = verify_producer_transaction(
            &overlay,
            verifying_block_height,
            tx,
            &mut MerkleTree::new(1),
        )
        .await
        {
            warn!(target: "validator::add_test_producer_transaction", "Transaction verification failed: {}", e);
            erroneous_txs.push(tx.clone());
        }

        let lock = overlay.lock().unwrap();
        let mut overlay = lock.overlay.lock().unwrap();
        if !erroneous_txs.is_empty() {
            warn!(target: "validator::add_test_producer_transaction", "Erroneous transactions found in set");
            overlay.purge_new_trees()?;
            return Err(TxVerifyFailed::ErroneousTxs(erroneous_txs).into())
        }

        if !write {
            debug!(target: "validator::add_test_producer_transaction", "Skipping apply of state updates because write=false");
            overlay.purge_new_trees()?;
            return Ok(())
        }

        debug!(target: "validator::add_test_producer_transaction", "Applying overlay changes");
        overlay.apply()?;
        Ok(())
    }

    /// Retrieve all existing blocks and try to apply them
    /// to an in memory overlay to verify their correctness.
    /// Be careful as this will try to load everything in memory.
    pub async fn validate_blockchain(
        &self,
        pow_target: usize,
        pow_fixed_difficulty: Option<BigUint>,
    ) -> Result<()> {
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
        let mut previous = &blocks[0];

        // Create a time keeper and a PoW module to validate each block
        let mut module = PoWModule::new(blockchain.clone(), pow_target, pow_fixed_difficulty)?;

        // Deploy native wasm contracts
        deploy_native_contracts(&overlay).await?;

        // Validate genesis block
        verify_genesis_block(&overlay, previous).await?;

        // Validate and insert each block
        for block in &blocks[1..] {
            // Verify block
            if verify_block(&overlay, &module, block, previous).await.is_err() {
                error!(target: "validator::validate_blockchain", "Erroneous block found in set");
                overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;
                return Err(Error::BlockIsInvalid(block.hash()?.to_string()))
            };

            // Update PoW module
            if block.header.version == 1 {
                module.append(block.header.timestamp.0, &module.next_difficulty()?);
            }

            // Use last inserted block as next iteration previous
            previous = block;
        }

        Ok(())
    }
}
