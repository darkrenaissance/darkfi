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

use std::collections::HashMap;

use async_std::sync::{Arc, Mutex};
use chrono::Utc;
use log::{debug, error};

use crate::{net, util::async_util, Result};

mod consensus;
mod consensus_candidate;
mod consensus_follower;
mod consensus_leader;
mod datastore;
mod primitives;
mod protocol_raft;
mod settings;

pub use consensus::{gen_id, Raft};
pub use datastore::DataStore;
pub use primitives::NetMsg;
pub use protocol_raft::ProtocolRaft;
pub use settings::RaftSettings;

// Auxilary function to periodically prun items, based on when they were received.
async fn prune_map<T: Clone + Eq + std::hash::Hash>(
    map: Arc<Mutex<HashMap<T, i64>>>,
    seen_duration: i64,
) {
    loop {
        async_util::sleep(seen_duration as u64).await;
        debug!(target: "raft", "Pruning item in map");

        let now = Utc::now().timestamp();

        let mut map = map.lock().await;
        for (k, v) in map.clone().iter() {
            if now - v > seen_duration {
                map.remove(k);
            }
        }
    }
}

async fn p2p_send_loop(receiver: smol::channel::Receiver<NetMsg>, p2p: net::P2pPtr) -> Result<()> {
    loop {
        let msg: NetMsg = receiver.recv().await?;
        if let Err(e) = p2p.broadcast(msg).await {
            error!(target: "raft", "error occurred during broadcasting a msg: {}", e);
            continue
        }
    }
}
