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

use darkfi::{blockchain::BlockInfo, system::sleep, util::time::Timestamp, Result};
use darkfi_sdk::{
    blockchain::{expected_reward, PidOutput, PreviousSlot, Slot},
    pasta::{group::ff::Field, pallas},
};
use log::info;
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
    // Miner loop
    let timekeeper = node.validator.read().await.consensus.time_keeper.clone();
    loop {
        // TODO: consensus should generate next block, along with its difficulty,
        //       derived from best fork
        // Retrieve last block
        let last = node.validator.read().await.blockchain.last_block()?;
        let last_hash = last.hash()?;

        // Generate next PoW slot
        let last_slot = last.slots.last().unwrap().clone();
        let id = last_slot.id + 1;
        let producers = 1;
        let previous = PreviousSlot::new(
            producers,
            vec![last_hash],
            vec![last.header.previous],
            last_slot.pid.error,
        );
        let pid = PidOutput::default();
        let total_tokens = last_slot.total_tokens + last_slot.reward;
        let reward = expected_reward(id);
        let slot = Slot::new(id, previous, pid, pallas::Base::ZERO, total_tokens, reward);

        // Mine next block
        let mut next_block = BlockInfo::default();
        next_block.header.previous = last_hash;
        next_block.header.height = slot.id;
        next_block.header.epoch = timekeeper.slot_epoch(next_block.header.height);
        next_block.header.timestamp = Timestamp::current_time();
        next_block.slots = vec![slot];
        node.validator.read().await.consensus.module.mine_block(&mut next_block, stop_signal)?;

        // Verify it
        node.validator.read().await.consensus.module.verify_current_block(&next_block)?;

        // Generate stuff before pushing block to blockchain
        let timestamp = next_block.header.timestamp.0;
        let message = BlockInfoMessage::from(&next_block);

        // Append block to blockchain
        let mut lock = node.validator.write().await;
        lock.add_blocks(&[next_block]).await?;

        // Update PoW module
        let difficulty = lock.consensus.module.next_difficulty();
        lock.consensus.module.append(timestamp, &difficulty);

        // Broadcast block
        node.sync_p2p.broadcast(&message).await;
    }
}
