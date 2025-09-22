/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use log::warn;
use std::sync::Arc;

use crate::{
    dht::{ChannelCacheItem, DhtHandler, DhtNode},
    net::session::{SESSION_REFINE, SESSION_SEED},
    Result,
};

/// Send a DHT ping request when there is a new channel, to know the node id of the new peer,
/// Then fill the channel cache and the buckets
pub async fn channel_task<H: DhtHandler>(handler: Arc<H>) -> Result<()> {
    loop {
        let channel_sub = handler.dht().p2p.hosts().subscribe_channel().await;
        let res = channel_sub.receive().await;
        channel_sub.unsubscribe().await;
        if res.is_err() {
            continue;
        }
        let channel = res.unwrap();
        let channel_cache_lock = handler.dht().channel_cache.clone();
        let mut channel_cache = channel_cache_lock.write().await;

        // Skip this channel if it's stopped or not new.
        if channel.is_stopped() || channel_cache.keys().any(|&k| k == channel.info.id) {
            continue;
        }
        // Skip this channel if it's a seed or refine session.
        if channel.session_type_id() & (SESSION_SEED | SESSION_REFINE) != 0 {
            continue;
        }

        let ping_res = handler.ping(channel.clone()).await;

        if let Err(e) = ping_res {
            warn!(target: "dht::channel_task()", "Error while pinging (requesting node id) {}: {e}", channel.address());
            // channel.stop().await;
            continue;
        }

        let node = ping_res.unwrap();

        channel_cache.entry(channel.info.id).or_insert_with(|| ChannelCacheItem {
            node: node.clone(),
            topic: None,
            usage_count: 0,
        });
        drop(channel_cache);

        if !node.addresses().is_empty() {
            handler.dht().add_node(node.clone()).await;
            let _ = handler.on_new_node(&node.clone()).await;
        }
    }
}
