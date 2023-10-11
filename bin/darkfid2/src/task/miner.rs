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

use darkfi::{system::sleep, tx::Transaction, validator::consensus::Proposal, Result};
use darkfi_sdk::crypto::SecretKey;
use log::info;
use rand::rngs::OsRng;
use smol::channel::Receiver;

use crate::{proto::BlockInfoMessage, Darkfid};

/// async task used for participating in the PoW consensus protocol
pub async fn miner_task(node: &Darkfid, stop_signal: &Receiver<()>) -> Result<()> {
    // TODO: For now we asume we have a single miner that produces block,
    //       until the PoW consensus and proper validations have been added.
    //       The miner workflow would be:
    //          First we wait for next finalization, for optimal conditions.
    //          After that we ask all our connected peers for their blocks,
    //          and append them to our consensus state, creating their forks.
    //          Then we evaluate each fork and find the best one, so we can
    //          mine its next.
    //          We start running 2 tasks, one listenning for blocks(proposals)
    //          from other miners, and one mining the best fork next block.
    //          These two tasks run in parallel. If we receive a block from
    //          another miner, we evaluate it and if it produces a higher
    //          ranking fork that the one we currectly mine, we stop, check
    //          if we can finalize any fork, and then start mining that fork
    //          next block. If we manage to mine the block next, we broadcast
    //          it and then execute the finalization check and start mining
    //          next best fork block.
    info!(target: "darkfid::task::miner_task", "Starting miner task...");

    // TODO: Remove this once proper validations are added
    // We sleep so our miner can grab their pickaxe
    sleep(10).await;

    // Start miner loop
    miner_loop(node, stop_signal).await?;

    Ok(())
}

/// Miner loop
async fn miner_loop(node: &Darkfid, stop_signal: &Receiver<()>) -> Result<()> {
    // TODO: secret should be a daemon arg(.toml config)
    let secret_key = SecretKey::random(&mut OsRng);
    let tx = Transaction::default();

    // Generate a new fork to be able to extend
    node.validator.write().await.consensus.generate_pow_slot()?;

    // Miner loop
    loop {
        // Mine next block proposal
        let (next_proposal, fork_index) = node
            .validator
            .read()
            .await
            .consensus
            .generate_proposal(&secret_key, tx.clone())
            .await?;
        let mut next_block = next_proposal.block;
        let module = node.validator.read().await.consensus.forks[fork_index].module.clone();
        module.mine_block(&mut next_block, stop_signal)?;

        // Verify it
        node.validator.read().await.consensus.module.verify_current_block(&next_block)?;

        // Append the mined proposal
        let proposal = Proposal::new(next_block)?;
        let mut lock = node.validator.write().await;
        lock.consensus.append_proposal(&proposal).await?;

        // Check if we can finalize anything and broadcast them
        let finalized = lock.finalization().await?;
        if !finalized.is_empty() {
            for block in finalized {
                let message = BlockInfoMessage::from(&block);
                node.sync_p2p.broadcast(&message).await;
            }
        }
    }
}
