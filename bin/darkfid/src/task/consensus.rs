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

use darkfi::{rpc::util::JsonValue, Result};
use darkfi_serial::serialize;
use log::info;

use crate::Darkfid;

// TODO: handle all ? so the task don't stop on errors

/// async task used for listening for new blocks and perform consensus
pub async fn consensus_task(node: &Darkfid) -> Result<()> {
    info!(target: "darkfid::task::consensus_task", "Starting consensus task...");

    // Grab blocks subscriber
    let block_sub = node.subscribers.get("blocks").unwrap();

    // Grab proposals subscriber and subscribe to it
    let proposals_sub = node.subscribers.get("proposals").unwrap();
    let subscription = proposals_sub.sub.clone().subscribe().await;

    loop {
        subscription.receive().await;

        // Check if we can finalize anything and broadcast them
        let finalized = node.validator.finalization().await?;
        if !finalized.is_empty() {
            let mut notif_blocks = Vec::with_capacity(finalized.len());
            for block in finalized {
                notif_blocks
                    .push(JsonValue::String(bs58::encode(&serialize(&block)).into_string()));
            }
            block_sub.notify(JsonValue::Array(notif_blocks)).await;
        }
    }
}
