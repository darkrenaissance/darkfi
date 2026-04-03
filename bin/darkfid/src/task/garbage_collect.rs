/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::collections::HashMap;

use darkfi::{
    blockchain::parse_record, tx::Transaction, validator::verification::verify_transaction,
    zk::VerifyingKey, Result,
};
use darkfi_sdk::{crypto::MerkleTree, tx::TransactionHash};
use smol::channel::Receiver;
use tracing::{debug, error, info};

use crate::DarkfiNodePtr;

/// Auxiliary macro to check if channel receiver is empty so we can
/// abort current iteration.
macro_rules! trigger_queue_check {
    ($receiver:ident, $label:tt) => {
        if !$receiver.is_empty() {
            continue $label
         }
     };
}

/// Async task used for purging unreferenced trees and erroneous
/// pending transactions from the nodes mempool.
pub async fn garbage_collect_task(receiver: Receiver<()>, node: DarkfiNodePtr) -> Result<()> {
    info!(target: "darkfid::task::garbage_collect_task", "Starting garbage collection task...");

    'outer: loop {
        // Wait for a new trigger
        if let Err(e) = receiver.recv().await {
            error!(target: "darkfid::task::garbage_collect_task", "recv fail: {e}");
            continue
        };

        // Purge all unreferenced contract trees from the database
        trigger_queue_check!(receiver, 'outer);
        debug!(target: "darkfid::task::garbage_collect_task", "Starting garbage collection iteration...");
        if let Err(e) = node
            .validator
            .read()
            .await
            .consensus
            .purge_unreferenced_trees(&mut node.registry.state.read().await.new_trees())
            .await
        {
            error!(target: "darkfid::task::garbage_collect_task", "Purging unreferenced contract trees from the database failed: {e}");
            continue
        }
        debug!(target: "darkfid::task::garbage_collect_task", "Unreferenced trees purged successfully, retrieving pending transactions...");

        // Check if our mempool is empty
        trigger_queue_check!(receiver, 'outer);
        let validator = node.validator.read().await;
        if validator.blockchain.transactions.pending.is_empty() {
            debug!(target: "darkfid::task::garbage_collect_task", "No pending transactions to process");
            continue
        }

        // Grab validator current best fork and an iterator over its
        // pending transactions so we don't hold the validator lock.
        let pending = validator.blockchain.transactions.pending.iter();
        let fork = match validator.best_current_fork().await {
            Ok(f) => f,
            Err(e) => {
                error!(target: "darkfid::task::garbage_collect_task", "Retrieving validator current best fork failed: {e}");
                continue
            }
        };
        let verify_fees = validator.verify_fees;
        drop(validator);

        // Transactions Merkle tree
        trigger_queue_check!(receiver, 'outer);
        let mut tree = MerkleTree::new(1);

        // Map of ZK proof verifying keys for the current transactions
        // batch.
        let mut vks: HashMap<[u8; 32], HashMap<String, VerifyingKey>> = HashMap::new();

        // Grab forks' next block height
        let next_block_height = match fork.get_next_block_height() {
            Ok(h) => h,
            Err(e) => {
                error!(
                   target: "darkfid::task::garbage_collect_task",
                   "Next fork block height retrieval failed: {e}"
                );
                continue
            }
        };

        // Iterate over all pending transactions
        for record in pending {
            trigger_queue_check!(receiver, 'outer);
            let record = match record {
                Ok(r) => r,
                Err(e) => {
                    error!(target: "darkfid::task::garbage_collect_task", "Failed retrieving pending tx: {e}");
                    continue 'outer
                }
            };
            let (tx_hash, tx) = match parse_record::<TransactionHash, Transaction>(record) {
                Ok((h, t)) => (h, t),
                Err(e) => {
                    error!(target: "darkfid::task::garbage_collect_task", "Failed parsing pending tx: {e}");
                    continue
                }
            };

            // If the transaction has already been proposed, remove it
            trigger_queue_check!(receiver, 'outer);
            debug!(target: "darkfid::task::garbage_collect_task", "Checking transaction: {tx_hash}");
            if fork.overlay.lock().unwrap().transactions.contains(&tx_hash)? {
                debug!(target: "darkfid::task::garbage_collect_task", "Transaction {tx_hash} has already been proposed, removing...");
                if let Err(e) = fork.blockchain.remove_pending_txs_hashes(&[tx_hash]) {
                    error!(target: "darkfid::task::garbage_collect_task", "Failed removing pending tx: {e}");
                };
                continue
            }

            // Update the verifying keys map
            trigger_queue_check!(receiver, 'outer);
            for call in &tx.calls {
                vks.entry(call.data.contract_id.to_bytes()).or_default();
            }

            // Verify the transaction against current state
            trigger_queue_check!(receiver, 'outer);
            fork.overlay.lock().unwrap().checkpoint();
            let result = verify_transaction(
                &fork.overlay,
                next_block_height,
                fork.module.target,
                &tx,
                &mut tree,
                &mut vks,
                verify_fees,
            )
            .await;
            fork.overlay.lock().unwrap().revert_to_checkpoint();
            if let Err(e) = result {
                debug!(target: "darkfid::task::garbage_collect_task", "Pending transaction {tx_hash} verification failed: {e}");
                if let Err(e) = fork.blockchain.remove_pending_txs_hashes(&[tx_hash]) {
                    error!(target: "darkfid::task::garbage_collect_task", "Failed removing pending tx: {e}");
                };
                continue
            }
            debug!(target: "darkfid::task::garbage_collect_task", "Pending transaction {tx_hash} verification successfully.");
        }
    }
}
