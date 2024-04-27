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

use darkfi::{rpc::util::JsonValue, system::StoppableTask, util::encoding::base64, Error, Result};
use darkfi_serial::serialize_async;
use log::{error, info};

use crate::{task::garbage_collect_task, Darkfid};

/// async task used for listening for new blocks and perform consensus.
pub async fn consensus_task(node: Arc<Darkfid>, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    info!(target: "darkfid::task::consensus_task", "Starting consensus task...");

    // Grab blocks subscriber
    let block_sub = node.subscribers.get("blocks").unwrap();

    // Grab proposals subscriber and subscribe to it
    let proposals_sub = node.subscribers.get("proposals").unwrap();
    let subscription = proposals_sub.sub.clone().subscribe().await;

    // Create the garbage collection task using a dummy task
    let gc_task = StoppableTask::new();
    gc_task.clone().start(
        async { Ok(()) },
        |_| async { /* Do nothing */ },
        Error::GarbageCollectionTaskStopped,
        ex.clone(),
    );

    loop {
        subscription.receive().await;

        // Check if we can finalize anything and broadcast them
        let finalized = match node.validator.finalization().await {
            Ok(f) => f,
            Err(e) => {
                error!(
                    target: "darkfid::task::consensus_task",
                    "Finalization failed: {e}"
                );
                continue
            }
        };
        if !finalized.is_empty() {
            let mut notif_blocks = Vec::with_capacity(finalized.len());
            for block in finalized {
                notif_blocks
                    .push(JsonValue::String(base64::encode(&serialize_async(&block).await)));
            }
            block_sub.notify(JsonValue::Array(notif_blocks)).await;

            // Invoke the detached garbage collection task
            gc_task.clone().stop().await;
            gc_task.clone().start(
                garbage_collect_task(node.clone()),
                |res| async {
                    match res {
                        Ok(()) | Err(Error::GarbageCollectionTaskStopped) => { /* Do nothing */ }
                        Err(e) => error!(target: "darkfid", "Failed starting garbage collection task: {}", e),
                    }
                },
                Error::GarbageCollectionTaskStopped,
                ex.clone(),
            );
        }
    }
}
