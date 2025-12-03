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

use std::{sync::Arc, time::UNIX_EPOCH};
use tracing::{error, info, warn};

use crate::{
    dht::{event::DhtEvent, ChannelCacheItem, DhtHandler, DhtNode, SESSION_MANUAL},
    net::{
        hosts::HostColor,
        session::{SESSION_DIRECT, SESSION_INBOUND, SESSION_OUTBOUND},
    },
    system::sleep,
    util::time::Timestamp,
    Result,
};

/// Handle DHT events.
pub async fn events_task<H: DhtHandler>(handler: Arc<H>) -> Result<()> {
    let dht = handler.dht();
    let sub = dht.event_publisher.clone().subscribe().await;
    loop {
        let event = sub.receive().await;

        match event {
            // On [`DhtEvent::PingReceived`] set channel_cache.ping_received = true
            DhtEvent::PingReceived { from, .. } => {
                let channel_cache_lock = dht.channel_cache.clone();
                let mut channel_cache = channel_cache_lock.write().await;
                if let Some(cached) = channel_cache.get_mut(&from.info.id) {
                    cached.ping_received = true;
                }
            }
            // On [`DhtEvent::PingSent`] set channel_cache.ping_sent = true
            DhtEvent::PingSent { to, .. } => {
                let channel_cache_lock = dht.channel_cache.clone();
                let mut channel_cache = channel_cache_lock.write().await;
                if let Some(cached) = channel_cache.get_mut(&to.info.id) {
                    cached.ping_sent = true;
                }
            }
            _ => {}
        }
    }
}

/// Send a DHT ping request when there is a new channel, to know the node id of the new peer,
/// Then fill the channel cache and the buckets
pub async fn channel_task<H: DhtHandler>(handler: Arc<H>) -> Result<()> {
    let dht = handler.dht();
    let p2p = dht.p2p.clone();
    let channel_sub = p2p.hosts().subscribe_channel().await;
    loop {
        let res = channel_sub.receive().await;
        if res.is_err() {
            continue;
        }
        let channel = res.unwrap();

        let channel_cache_lock = dht.channel_cache.clone();
        let mut channel_cache = channel_cache_lock.write().await;

        // Skip this channel if it's not new
        if channel_cache.keys().any(|&k| k == channel.info.id) {
            continue;
        }

        channel_cache.insert(
            channel.info.id,
            ChannelCacheItem {
                node: None,
                last_used: Timestamp::current_time(),
                ping_received: false,
                ping_sent: false,
            },
        );
        drop(channel_cache);

        // It's a manual connection
        if channel.session_type_id() & SESSION_MANUAL != 0 {
            let ping_res = dht.ping(channel.clone()).await;

            if let Err(e) = ping_res {
                warn!(target: "dht::channel_task()", "Error while pinging manual connection (requesting node id) {}: {e}", channel.display_address());
                continue;
            }
        }

        // It's an outbound connection
        if channel.session_type_id() & SESSION_OUTBOUND != 0 {
            let _ = dht.ping(channel.clone()).await;
            continue;
        }

        // It's a direct connection
        if channel.session_type_id() & SESSION_DIRECT != 0 {
            p2p.session_direct().inc_channel_usage(&channel, 1).await;
            let _ = dht.ping(channel.clone()).await;
            dht.cleanup_channel(channel).await;
            continue;
        }
    }
}

/// Periodically send a DHT ping to known hosts. If the ping is successful, we
/// move the host to the whitelist (updating the last seen field).
///
/// This is necessary to prevent unresponsive nodes staying on the whitelist,
/// as the DHT does not require any outbound slot.
pub async fn dht_refinery_task<H: DhtHandler>(handler: Arc<H>) -> Result<()> {
    let interval = 60; // TODO: Make a setting
    let min_ping_interval = 10 * 60; // TODO: Make a setting
    let dht = handler.dht();
    let hosts = dht.p2p.hosts();

    loop {
        let mut hostlist = hosts.container.fetch_all(HostColor::Gold);
        hostlist.extend(hosts.container.fetch_all(HostColor::White));

        // Include the greylist only if the DHT is not bootstrapped yet
        if !handler.dht().is_bootstrapped().await {
            hostlist.extend(hosts.container.fetch_all(HostColor::Grey));
        }

        for entry in &hostlist {
            let url = &entry.0;
            let host_cache = dht.host_cache.read().await;
            let last_ping = host_cache.get(url).map(|h| h.last_ping.inner());
            if last_ping.is_some() &&
                last_ping.unwrap() > Timestamp::current_time().inner() - min_ping_interval
            {
                continue
            }
            drop(host_cache);

            let res = dht.create_channel(url).await;
            if res.is_err() {
                continue
            }
            let (channel, _) = res.unwrap();
            dht.cleanup_channel(channel).await;

            let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();
            if let Err(e) = hosts.whitelist_host(url, last_seen).await {
                error!(target: "dht::tasks::whitelist_refinery_task()", "Could not send {url} to the whitelist: {e}");
            }
            break
        }

        match hostlist.is_empty() {
            true => sleep(5).await,
            false => sleep(interval).await,
        }
    }
}

/// Add a node to the DHT buckets.
/// If the bucket is already full, we ping the least recently seen node in the
/// bucket: if successful it becomes the most recently seen node, if the ping
/// fails we remove it and add the new node.
/// [`Dht::update_node()`] increments a channel's usage count (in the direct
/// session) and triggers this task. This task decrements the usage count
/// using [`Dht::cleanup_channel()`].
pub async fn add_node_task<H: DhtHandler>(handler: Arc<H>) -> Result<()> {
    let dht = handler.dht();
    loop {
        let (node, channel) = dht.add_node_rx.recv().await.unwrap();

        let self_node = handler.node().await;
        if self_node.is_err() {
            continue;
        }
        let self_node = self_node.unwrap();

        let bucket_index = dht.get_bucket_index(&self_node.id(), &node.id()).await;
        let buckets_lock = dht.buckets.clone();
        let mut buckets = buckets_lock.write().await;
        let bucket = &mut buckets[bucket_index];

        // Do not add ourselves to the buckets
        if node.id() == self_node.id() {
            dht.cleanup_channel(channel).await;
            continue;
        }

        // Don't add this node if it has any external address that is the same as one of ours
        let node_addresses = node.addresses();
        if self_node.addresses().iter().any(|addr| node_addresses.contains(addr)) {
            dht.cleanup_channel(channel).await;
            continue;
        }

        // Do not add a node to the buckets if it does not have an address
        if node.addresses().is_empty() {
            dht.cleanup_channel(channel).await;
            continue;
        }

        // We already have this node, move it to the tail of the bucket
        if let Some(node_index) = bucket.nodes.iter().position(|n| n.id() == node.id()) {
            bucket.nodes.remove(node_index);
            bucket.nodes.push(node);
            dht.cleanup_channel(channel).await;
            continue;
        }

        // Bucket is full
        if bucket.nodes.len() >= dht.settings.k {
            // Ping the least recently seen node
            if let Ok((channel2, node)) = dht.get_channel(&bucket.nodes[0]).await {
                // Ping was successful, move the least recently seen node to the tail
                let n = bucket.nodes.remove(0);
                bucket.nodes.push(n);
                drop(buckets);
                dht.on_new_node(&node.clone(), channel2.clone()).await;
                dht.cleanup_channel(channel2).await;
                dht.cleanup_channel(channel).await;
                continue;
            }

            // Ping was not successful, remove the least recently seen node and add the new node
            bucket.nodes.remove(0);
            bucket.nodes.push(node.clone());
            drop(buckets);
            dht.on_new_node(&node.clone(), channel.clone()).await;
            dht.cleanup_channel(channel).await;
            continue;
        }

        // Bucket is not full, just add the node
        bucket.nodes.push(node.clone());
        drop(buckets);
        dht.on_new_node(&node.clone(), channel.clone()).await;
        dht.cleanup_channel(channel).await;
    }
}

/// Close inbound connections that are unused for too long.
pub async fn disconnect_inbounds_task<H: DhtHandler>(handler: Arc<H>) -> Result<()> {
    let interval = 10; // TODO: Make a setting
    let dht = handler.dht();

    loop {
        sleep(interval).await;

        let min_last_used = Timestamp::current_time().inner() - dht.settings.inbound_timeout;

        let channel_cache_lock = dht.channel_cache.clone();
        let mut channel_cache = channel_cache_lock.write().await;

        for (channel_id, cached) in channel_cache.clone() {
            // Check that:
            // The channel timed out,
            if cached.last_used.inner() >= min_last_used {
                continue;
            }
            // The channel exists,
            let channel = dht.p2p.get_channel(channel_id);
            if channel.is_none() {
                channel_cache.remove(&channel_id);
                continue;
            }
            let channel = channel.unwrap();
            // And the channel is inbound.
            if channel.session_type_id() & SESSION_INBOUND == 0 {
                continue;
            }

            // Now we can stop it and remove it from the channel cache
            info!(target: "dht::disconnect_inbounds_task()", "Closing expired inbound channel [{}]", channel.display_address());
            channel.stop().await;
            channel_cache.remove(&channel.info.id);
        }
    }
}

/// Removes entries from [`crate::dht::Dht::channel_cache`] when a channel is
/// stopped.
pub async fn cleanup_channels_task<H: DhtHandler>(handler: Arc<H>) -> Result<()> {
    let interval = 60; // TODO: Make a setting
    let dht = handler.dht();

    loop {
        sleep(interval).await;

        let channel_cache_lock = dht.channel_cache.clone();
        let mut channel_cache = channel_cache_lock.write().await;

        for (channel_id, _) in channel_cache.clone() {
            match dht.p2p.get_channel(channel_id) {
                Some(channel) => {
                    if channel.is_stopped() {
                        channel_cache.remove(&channel_id);
                    }
                }
                None => {
                    channel_cache.remove(&channel_id);
                }
            }
        }
    }
}
