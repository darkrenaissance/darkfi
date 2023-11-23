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

use darkfi::{system::sleep, util::encoding::base64};
use darkfi_serial::serialize;
use log::{debug, info, warn, error};
use tinyjson::JsonValue;

use crate::{
    proto::{SyncRequest, SyncResponse},
    Darkfid,
};

/// async task used for block syncing
pub async fn sync_task(node: &Darkfid) {
    info!(target: "darkfid::task::sync_task", "Starting blockchain sync...");
    // Block until at least node is connected to at least one peer
    loop {
        if !node.sync_p2p.channels().await.is_empty() {
            break
        }
        warn!(target: "darkfid::task::sync_task", "Node is not connected to other nodes, waiting to retry...");
        sleep(10).await;
    }

    // Getting a random connected channel to ask from peers
    let channel = node.sync_p2p.random_channel().await.unwrap();

    // Communication setup
    let msg_subsystem = channel.message_subsystem();
    msg_subsystem.add_dispatch::<SyncResponse>().await;
    let block_response_sub = match channel.subscribe_msg::<SyncResponse>().await {
        Err(why) => {
            // if there is Error::NetworkOperationFailed returns at dispatcher subscription attempt
            panic!("darkiid2::task::sync_task channel subscribe_msg failed at dispatcher subscription: {:?}", why);
        },
        Ok(value) => value
    };
    let notif_sub = node.subscribers.get("blocks").unwrap();

    // TODO: make this parallel and use a head selection method,
    // for example use a manual known head and only connect to nodes
    // that follow that. Also use a random peer on every block range
    // we sync.

    // Node sends the last known block hash of the canonical blockchain
    // and loops until the response is the same block (used to utilize
    // batch requests).
    let mut last = match node.validator.read().await.blockchain.last() {
        Err(why) => {
            panic!("darkfid::task::sync_task attempting to retrive last block at empty BlockOrderStore sledTree: {:?}", why)
        },
        Ok(value) => value
    };
    info!(target: "darkfid::task::sync_task", "Last known block: {:?} - {:?}", last.0, last.1);
    loop {
        // Node creates a `SyncRequest` and sends it
        let request = SyncRequest { slot: last.0, block: last.1 };
        if let Err(why) = channel.send(&request).await {
            error!(target: "darkfid::task::sync_task", "request send failure: {:}", why);
            sleep(10).await;
            continue;
        }

        // TODO: add a timeout here to retry
        // Node waits for response
        let response = match block_response_sub.receive().await {
            Err(why) => {
                panic!("darkfid::task::sync_task block_response_sub receive error at recv_queue recv: {:?}", why)
            },
            Ok(value) => value
        };

        // Verify and store retrieved blocks
        debug!(target: "darkfid::task::sync_task", "Processing received blocks");
        if let Err(why) = node.validator.write().await.add_blocks(&response.blocks).await {
            panic!("darkfid::task::sync_task block validation failed: {:?}", why)
        };

        // Notify subscriber
        for block in &response.blocks {
            let encoded_block = JsonValue::String(base64::encode(&serialize(block)));
            notif_sub.notify(vec![encoded_block].into()).await;
        }

        let last_received = match node.validator.read().await.blockchain.last() {
            Err(why) => {
                panic!("darkfid::task::sync_task attempting to retrive last block at empty BlockOrderStore sledTree: {:?}", why)
            },
            Ok(value) => value
        };
        info!(target: "darkfid::task::sync_task", "Last received block: {:?} - {:?}", last_received.0, last_received.1);

        if last == last_received {
            break
        }

        last = last_received;
    }

    node.validator.write().await.synced = true;
    info!(target: "darkfid::task::sync_task", "Blockchain synced!");
}
