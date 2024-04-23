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

use darkfi::{
    error::TxVerifyFailed,
    validator::{consensus::TXS_CAP, verification::verify_transactions},
    Error, Result,
};
use darkfi_sdk::crypto::MerkleTree;
use log::info;

use crate::Darkfid;

// TODO: handle all ? so the task don't stop on errors

/// Async task used for purging erroneous pending transactions from the nodes mempool.
pub async fn garbage_collect_task(node: Arc<Darkfid>) -> Result<()> {
    info!(target: "darkfid::task::garbage_collect_task", "Starting garbage collection task...");

    // Grab all current unproposed transactions.  We verify them in batches,
    // to not load them all in memory.
    let (mut last_checked, mut txs) =
        node.validator.blockchain.transactions.get_after_pending(0, TXS_CAP)?;
    while !txs.is_empty() {
        // Verify each one against current forks
        for tx in txs {
            let tx_hash = tx.hash();
            let tx_vec = [tx.clone()];

            // Grab a lock over current consensus forks state
            let mut forks = node.validator.consensus.forks.write().await;

            // Iterate over them to verify transaction validity in their overlays
            for fork in forks.iter_mut() {
                // Clone forks' overlay
                let overlay = fork.overlay.lock().unwrap().full_clone()?;

                // Grab all current proposals transactions hashes
                let proposals_txs =
                    overlay.lock().unwrap().get_blocks_txs_hashes(&fork.proposals)?;

                // If the hash is contained in the proposals transactions vec, skip it
                if proposals_txs.contains(&tx_hash) {
                    continue
                }

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
                    Err(Error::TxVerifyFailed(TxVerifyFailed::ErroneousTxs(_))) => {
                        // Remove transaction from fork's mempool
                        fork.mempool.retain(|tx| *tx != tx_hash);
                    }
                    Err(e) => return Err(e),
                }
            }

            // Drop forks lock
            drop(forks);
        }
        (last_checked, txs) =
            node.validator.blockchain.transactions.get_after_pending(last_checked, TXS_CAP)?;
    }
    info!(target: "darkfid::task::garbage_collect_task", "Garbage collection finished successfully!");
    Ok(())
}
