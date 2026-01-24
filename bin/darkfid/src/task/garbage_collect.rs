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

use darkfi::{error::TxVerifyFailed, validator::verification::verify_transactions, Error, Result};
use darkfi_sdk::crypto::MerkleTree;
use tracing::{debug, error, info};

use crate::DarkfiNodePtr;

/// Async task used for purging erroneous pending transactions from the nodes mempool.
pub async fn garbage_collect_task(node: DarkfiNodePtr) -> Result<()> {
    info!(target: "darkfid::task::garbage_collect_task", "Starting garbage collection task...");

    // Grab all current unproposed transactions.  We verify them in batches,
    // to not load them all in memory.
    let (mut last_checked, mut txs) =
        match node.validator.blockchain.transactions.get_after_pending(0, node.txs_batch_size) {
            Ok(pair) => pair,
            Err(e) => {
                error!(
                    target: "darkfid::task::garbage_collect_task",
                    "Uproposed transactions retrieval failed: {e}"
                );
                return Ok(())
            }
        };

    // Check if we have transactions to process
    if txs.is_empty() {
        info!(target: "darkfid::task::garbage_collect_task", "Garbage collection finished successfully!");
        return Ok(())
    }

    while !txs.is_empty() {
        // Verify each one against current forks
        for tx in txs {
            let tx_hash = tx.hash();
            let tx_vec = [tx.clone()];
            let mut valid = false;

            // Grab a lock over current consensus forks state
            let mut forks = node.validator.consensus.forks.write().await;

            // Iterate over them to verify transaction validity in their overlays
            for fork in forks.iter_mut() {
                // Clone forks' overlay
                let overlay = match fork.overlay.lock().unwrap().full_clone() {
                    Ok(o) => o,
                    Err(e) => {
                        error!(
                            target: "darkfid::task::garbage_collect_task",
                            "Overlay full clone creation failed: {e}"
                        );
                        return Err(e)
                    }
                };

                // Grab all current proposals transactions hashes
                let proposals_txs =
                    match overlay.lock().unwrap().get_blocks_txs_hashes(&fork.proposals) {
                        Ok(txs) => txs,
                        Err(e) => {
                            error!(
                                target: "darkfid::task::garbage_collect_task",
                                "Proposal transactions retrieval failed: {e}"
                            );
                            return Err(e)
                        }
                    };

                // If the hash is contained in the proposals transactions vec, skip it
                if proposals_txs.contains(&tx_hash) {
                    continue
                }

                // Grab forks' next block height
                let next_block_height = match fork.get_next_block_height() {
                    Ok(h) => h,
                    Err(e) => {
                        error!(
                            target: "darkfid::task::garbage_collect_task",
                            "Next fork block height retrieval failed: {e}"
                        );
                        return Err(e)
                    }
                };

                // Verify transaction
                let result = verify_transactions(
                    &overlay,
                    next_block_height,
                    node.validator.consensus.module.read().await.target,
                    &tx_vec,
                    &mut MerkleTree::new(1),
                    false,
                )
                .await;

                // Check result
                match result {
                    Ok(_) => valid = true,
                    Err(Error::TxVerifyFailed(TxVerifyFailed::ErroneousTxs(_))) => {
                        // Remove transaction from fork's mempool
                        fork.mempool.retain(|tx| *tx != tx_hash);
                    }
                    Err(e) => {
                        error!(
                            target: "darkfid::task::garbage_collect_task",
                            "Verifying transaction {tx_hash} failed: {e}"
                        );
                        return Err(e)
                    }
                }
            }

            // Drop forks lock
            drop(forks);

            // Remove transaction if its invalid for all the forks
            if !valid {
                debug!(target: "darkfid::task::garbage_collect_task", "Removing invalid transaction: {tx_hash}");
                if let Err(e) = node.validator.blockchain.remove_pending_txs_hashes(&[tx_hash]) {
                    error!(
                        target: "darkfid::task::garbage_collect_task",
                        "Removing invalid transaction {tx_hash} failed: {e}"
                    );
                };
            }
        }

        // Grab next batch
        (last_checked, txs) = match node
            .validator
            .blockchain
            .transactions
            .get_after_pending(last_checked + node.txs_batch_size as u64, node.txs_batch_size)
        {
            Ok(pair) => pair,
            Err(e) => {
                error!(
                    target: "darkfid::task::garbage_collect_task",
                    "Uproposed transactions next batch retrieval failed: {e}"
                );
                break
            }
        };
    }

    info!(target: "darkfid::task::garbage_collect_task", "Garbage collection finished successfully!");
    Ok(())
}

/// Auxiliary function to purge all unreferenced contract trees from
/// the node database.
pub async fn purge_unreferenced_trees(node: &DarkfiNodePtr) {
    // Grab node registry locks
    let submit_lock = node.registry.submit_lock.write().await;
    let block_templates = node.registry.block_templates.write().await;
    let jobs = node.registry.jobs.write().await;
    let mm_jobs = node.registry.mm_jobs.write().await;

    // Purge all unreferenced contract trees from the database
    if let Err(e) = node
        .validator
        .consensus
        .purge_unreferenced_trees(&mut node.registry.new_trees(&block_templates))
        .await
    {
        error!(target: "darkfid::task::garbage_collect::purge_unreferenced_trees", "Purging unreferenced contract trees from the database failed: {e}");
    }

    // Release registry locks
    drop(block_templates);
    drop(jobs);
    drop(mm_jobs);
    drop(submit_lock);
}
