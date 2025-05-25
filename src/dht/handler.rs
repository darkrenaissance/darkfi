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

use async_trait::async_trait;
use futures::stream::FuturesUnordered;
use log::{debug, info, warn};
use num_bigint::BigUint;
use smol::{lock::Semaphore, stream::StreamExt};
use std::{collections::HashSet, sync::Arc, time::Duration};

use super::{Dht, DhtNode, DhtRouterItem, DhtRouterPtr};
use crate::{
    net::{
        connector::Connector,
        session::{Session, SESSION_REFINE, SESSION_SEED},
        ChannelPtr, Message,
    },
    system::{sleep, timeout::timeout},
    Error, Result,
};

#[async_trait]
pub trait DhtHandler {
    fn dht(&self) -> Arc<Dht>;

    /// Send a DHT ping request
    async fn ping(&self, channel: ChannelPtr) -> Result<DhtNode>;

    /// Triggered when we find a new node
    async fn on_new_node(&self, node: &DhtNode) -> Result<()>;

    /// Send FIND NODES request to a peer to get nodes close to `key`
    async fn fetch_nodes(&self, node: &DhtNode, key: &blake3::Hash) -> Result<Vec<DhtNode>>;

    /// Announce message for a key, and add ourselves to router
    async fn announce<M: Message>(
        &self,
        key: &blake3::Hash,
        message: &M,
        router: DhtRouterPtr,
    ) -> Result<()> {
        let self_node = self.dht().node().await;
        if self_node.addresses.is_empty() {
            return Err(().into()); // TODO
        }

        self.add_to_router(router.clone(), key, vec![self_node.clone().into()]).await;
        let nodes = self.lookup_nodes(key).await?;

        for node in nodes {
            let channel = self.get_channel(&node).await;
            if let Ok(ch) = channel {
                let _ = ch.send(message).await;
            }
        }

        Ok(())
    }

    /// Send a DHT ping request when there is a new channel, to know the node id of the new peer,
    /// Then fill the channel cache and the buckets
    async fn channel_task<M: Message>(&self) -> Result<()> {
        loop {
            let channel_sub = self.dht().p2p.hosts().subscribe_channel().await;
            let res = channel_sub.receive().await;
            channel_sub.unsubscribe().await;
            if res.is_err() {
                continue;
            }
            let channel = res.unwrap();
            let channel_cache_lock = self.dht().channel_cache.clone();
            let mut channel_cache = channel_cache_lock.write().await;

            // Skip this channel if it's stopped or not new.
            if channel.is_stopped() || channel_cache.values().any(|&v| v == channel.info.id) {
                continue;
            }
            // Skip this channel if it's a seed or refine session.
            if channel.session_type_id() & (SESSION_SEED | SESSION_REFINE) != 0 {
                continue;
            }

            let node = self.ping(channel.clone()).await;

            if let Ok(n) = node {
                channel_cache.insert(n.id, channel.info.id);
                drop(channel_cache);

                let node_cache_lock = self.dht().node_cache.clone();
                let mut node_cache = node_cache_lock.write().await;
                node_cache.insert(channel.info.id, n.clone());
                drop(node_cache);

                if !n.addresses.is_empty() {
                    self.add_node(n.clone()).await;
                    let _ = self.on_new_node(&n.clone()).await;
                }
            }
        }
    }

    /// Remove disconnected nodes from the channel cache
    async fn disconnect_task(&self) -> Result<()> {
        loop {
            sleep(15).await;

            let channel_cache_lock = self.dht().channel_cache.clone();
            let mut channel_cache = channel_cache_lock.write().await;
            for (node_id, channel_id) in channel_cache.clone() {
                let channel = self.dht().p2p.get_channel(channel_id);
                if channel.is_none() || (channel.is_some() && channel.unwrap().is_stopped()) {
                    channel_cache.remove(&node_id);
                }
            }
        }
    }

    /// Add a node in the correct bucket
    async fn add_node(&self, node: DhtNode) {
        // Do not add ourselves to the buckets
        if node.id == self.dht().node_id {
            return;
        }

        // Do not add a node to the buckets if it does not have an address
        if node.addresses.is_empty() {
            return;
        }

        let bucket_index = self.dht().get_bucket_index(&node.id).await;
        let buckets_lock = self.dht().buckets.clone();
        let mut buckets = buckets_lock.write().await;
        let bucket = &mut buckets[bucket_index];

        // Node is already in the bucket
        if bucket.nodes.iter().any(|n| n.id == node.id) {
            return;
        }

        // Bucket is full
        if bucket.nodes.len() >= self.dht().settings.k {
            // Ping the least recently seen node
            let channel = self.get_channel(&bucket.nodes[0]).await;
            if channel.is_ok() {
                let ping_res = self.ping(channel.unwrap()).await;
                if ping_res.is_ok() {
                    // Ping was successful, move the least recently seen node to the tail
                    let n = bucket.nodes.remove(0);
                    bucket.nodes.push(n);
                    return;
                }
            }

            // Ping was not successful, remove the least recently seen node and add the new node
            bucket.nodes.remove(0);
            bucket.nodes.push(node);
            return;
        }

        // Bucket is not full
        bucket.nodes.push(node);
    }

    /// Move a node to the tail in its bucket,
    /// to show that it is the most recently seen in the bucket.
    /// If the node is not in a bucket it will be added using `add_node`
    async fn update_node(&self, node: &DhtNode) {
        let bucket_index = self.dht().get_bucket_index(&node.id).await;
        let buckets_lock = self.dht().buckets.clone();
        let mut buckets = buckets_lock.write().await;
        let bucket = &mut buckets[bucket_index];

        let node_index = bucket.nodes.iter().position(|n| n.id == node.id);
        if node_index.is_none() {
            self.add_node(node.clone()).await;
            return;
        }

        let n = bucket.nodes.remove(node_index.unwrap());
        bucket.nodes.push(n);
    }

    /// Wait to acquire a semaphore, then run `self.fetch_nodes`.
    /// This is meant to be used in `lookup_nodes`.
    async fn fetch_nodes_sp(
        &self,
        semaphore: Arc<Semaphore>,
        node: DhtNode,
        key: &blake3::Hash,
    ) -> (DhtNode, Result<Vec<DhtNode>>) {
        let _permit = semaphore.acquire().await;
        (node.clone(), self.fetch_nodes(&node, key).await)
    }

    /// Find `k` nodes closest to a key
    async fn lookup_nodes(&self, key: &blake3::Hash) -> Result<Vec<DhtNode>> {
        info!(target: "dht::DhtHandler::lookup_nodes()", "Starting node lookup for key {}", bs58::encode(key.as_bytes()).into_string());

        let k = self.dht().settings.k;
        let a = self.dht().settings.alpha;
        let semaphore = Arc::new(Semaphore::new(self.dht().settings.concurrency));
        let mut futures = FuturesUnordered::new();

        // Nodes we did not send a request to (yet), sorted by distance from `key`
        let mut nodes_to_visit = self.dht().find_neighbors(key, k).await;
        // Nodes with a pending request or a request completed
        let mut visited_nodes = HashSet::<blake3::Hash>::new();
        // Nodes that responded to our request, sorted by distance from `key`
        let mut result = Vec::<DhtNode>::new();

        // Create the first `alpha` tasks
        for _ in 0..a {
            match nodes_to_visit.pop() {
                Some(node) => {
                    visited_nodes.insert(node.id);
                    futures.push(self.fetch_nodes_sp(semaphore.clone(), node, key));
                }
                None => {
                    break;
                }
            }
        }

        while let Some((queried_node, value_result)) = futures.next().await {
            match value_result {
                Ok(mut nodes) => {
                    info!(target: "dht::DhtHandler::lookup_nodes", "Queried {}, got {} nodes", bs58::encode(queried_node.id.as_bytes()).into_string(), nodes.len());

                    // Remove ourselves and already known nodes from the new nodes
                    nodes.retain(|node| {
                        node.id != self.dht().node_id &&
                            !visited_nodes.contains(&node.id) &&
                            !nodes_to_visit.iter().any(|n| n.id == node.id)
                    });

                    // Add new nodes to our buckets
                    for node in nodes.clone() {
                        self.add_node(node).await;
                    }

                    // Add nodes to the list of nodes to visit
                    nodes_to_visit.extend(nodes.clone());
                    self.dht().sort_by_distance(&mut nodes_to_visit, key);

                    // Update nearest_nodes
                    result.push(queried_node.clone());
                    self.dht().sort_by_distance(&mut result, key);

                    // Early termination check:
                    // Stop if our furthest visited node is closer than the closest node we will query,
                    // and we already have `k` or more nodes in the result set
                    if result.len() >= k {
                        if let Some(furthest) = result.last() {
                            if let Some(next_node) = nodes_to_visit.first() {
                                let furthest_dist =
                                    BigUint::from_bytes_be(&self.dht().distance(key, &furthest.id));
                                let next_dist = BigUint::from_bytes_be(
                                    &self.dht().distance(key, &next_node.id),
                                );
                                if furthest_dist < next_dist {
                                    info!(target: "dht::DhtHandler::lookup_nodes", "Early termination for lookup nodes");
                                    break;
                                }
                            }
                        }
                    }

                    // Create the `alpha` tasks
                    for _ in 0..a {
                        match nodes_to_visit.pop() {
                            Some(node) => {
                                visited_nodes.insert(node.id);
                                futures.push(self.fetch_nodes_sp(semaphore.clone(), node, key));
                            }
                            None => {
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(target: "dht::DhtHandler::lookup_nodes", "Error looking for nodes: {}", e);
                }
            }
        }

        result.truncate(k);
        return Ok(result.to_vec())
    }

    /// Get an existing channel, or create a new one
    async fn get_channel(&self, node: &DhtNode) -> Result<ChannelPtr> {
        let channel_cache_lock = self.dht().channel_cache.clone();
        let channel_cache = channel_cache_lock.read().await;

        if let Some(channel_id) = channel_cache.get(&node.id) {
            if let Some(channel) = self.dht().p2p.get_channel(*channel_id) {
                if channel.session_type_id() & (SESSION_SEED | SESSION_REFINE) != 0 {
                    return Err(Error::Custom(
                        "Could not get a channel (for DHT) as this is a seed or refine session"
                            .to_string(),
                    ));
                }

                if channel.is_stopped() {
                    channel.clone().start(self.dht().executor.clone());
                }
                return Ok(channel);
            }
        }

        // Create a channel
        for addr in node.addresses.clone() {
            let session_out = self.dht().p2p.session_outbound();
            let session_weak = Arc::downgrade(&self.dht().p2p.session_outbound());

            let connector = Connector::new(self.dht().p2p.settings(), session_weak);
            let dur = Duration::from_secs(self.dht().settings.timeout);
            let Ok(connect_res) = timeout(dur, connector.connect(&addr)).await else {
                warn!(target: "dht::DhtHandler::get_channel()", "Timeout trying to connect to {}", addr);
                return Err(Error::ConnectTimeout);
            };
            if connect_res.is_err() {
                warn!(target: "dht::DhtHandler::get_channel()", "Error while connecting to {}: {}", addr, connect_res.unwrap_err());
                continue;
            }
            let (_, channel) = connect_res.unwrap();

            if channel.session_type_id() & (SESSION_SEED | SESSION_REFINE) != 0 {
                return Err(Error::Custom(
                    "Could not create a channel (for DHT) as this is a seed or refine session"
                        .to_string(),
                ));
            }

            let register_res =
                session_out.register_channel(channel.clone(), self.dht().executor.clone()).await;
            if register_res.is_err() {
                channel.clone().stop().await;
                warn!(target: "dht::DhtHandler::get_channel()", "Error while registering channel {}: {}", channel.info.id, register_res.unwrap_err());
                continue;
            }

            return Ok(channel)
        }

        Err(Error::Custom("Could not create channel".to_string()))
    }

    /// Add nodes as a provider for a key
    async fn add_to_router(
        &self,
        router: DhtRouterPtr,
        key: &blake3::Hash,
        router_items: Vec<DhtRouterItem>,
    ) {
        let mut router_items = router_items.clone();
        router_items.retain(|item| !item.node.addresses.is_empty());

        debug!(target: "dht::DhtHandler::add_to_router()", "Inserting {} nodes to key {}", router_items.len(), bs58::encode(key.as_bytes()).into_string());

        let mut router_write = router.write().await;
        let key_r = router_write.get_mut(key);

        let router_cache_lock = self.dht().router_cache.clone();
        let mut router_cache = router_cache_lock.write().await;

        // Add to router
        if let Some(k) = key_r {
            k.retain(|it| !router_items.contains(it));
            k.extend(router_items.clone());
        } else {
            let mut hs = HashSet::new();
            hs.extend(router_items.clone());
            router_write.insert(*key, hs);
        }

        // Add to router_cache
        for router_item in router_items {
            let keys = router_cache.get_mut(&router_item.node.id);
            if let Some(k) = keys {
                k.insert(*key);
            } else {
                let mut keys = HashSet::new();
                keys.insert(*key);
                router_cache.insert(router_item.node.id, keys);
            }
        }
    }
}
