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
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use super::{ChannelCacheItem, Dht, DhtNode, DhtRouterItem, DhtRouterPtr};
use crate::{
    geode::hash_to_string,
    net::{
        connector::Connector,
        session::{Session, SESSION_REFINE, SESSION_SEED},
        ChannelPtr, Message,
    },
    system::timeout::timeout,
    Error, Result,
};

#[async_trait]
pub trait DhtHandler<N: DhtNode> {
    fn dht(&self) -> Arc<Dht<N>>;

    /// Get our own node
    async fn node(&self) -> N;

    /// Send a DHT ping request
    async fn ping(&self, channel: ChannelPtr) -> Result<N>;

    /// Triggered when we find a new node
    async fn on_new_node(&self, node: &N) -> Result<()>;

    /// Send FIND NODES request to a peer to get nodes close to `key`
    async fn fetch_nodes(&self, node: &N, key: &blake3::Hash) -> Result<Vec<N>>;

    /// Announce message for a key, and add ourselves to router
    async fn announce<M: Message>(
        &self,
        key: &blake3::Hash,
        message: &M,
        router: DhtRouterPtr<N>,
    ) -> Result<()>
    where
        N: 'async_trait,
    {
        let self_node = self.node().await;
        if self_node.addresses().is_empty() {
            return Err(().into()); // TODO
        }

        self.add_to_router(router.clone(), key, vec![self_node.clone().into()]).await;
        let nodes = self.lookup_nodes(key).await?;
        info!(target: "dht::DhtHandler::announce()", "Announcing {} to {} nodes", hash_to_string(key), nodes.len());

        for node in nodes {
            let channel_res = self.get_channel(&node, None).await;
            if let Ok(channel) = channel_res {
                let _ = channel.send(message).await;
                self.cleanup_channel(channel).await;
            }
        }

        Ok(())
    }

    /// Lookup our own node id to bootstrap our DHT
    async fn bootstrap(&self) {
        self.dht().set_bootstrapped(true).await;

        let self_node_id = self.node().await.id();
        debug!(target: "dht::DhtHandler::bootstrap()", "DHT bootstrapping {}", hash_to_string(&self_node_id));
        let nodes = self.lookup_nodes(&self_node_id).await;

        if nodes.is_err() || nodes.map_or(true, |v| v.is_empty()) {
            self.dht().set_bootstrapped(false).await;
        }
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
            if channel.is_stopped() || channel_cache.keys().any(|&k| k == channel.info.id) {
                continue;
            }
            // Skip this channel if it's a seed or refine session.
            if channel.session_type_id() & (SESSION_SEED | SESSION_REFINE) != 0 {
                continue;
            }

            let ping_res = self.ping(channel.clone()).await;

            if let Err(e) = ping_res {
                warn!(target: "dht::DhtHandler::channel_task()", "Error while pinging (requesting node id) {}: {e}", channel.address());
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
                self.add_node(node.clone()).await;
                let _ = self.on_new_node(&node.clone()).await;
            }
        }
    }

    /// Add a node in the correct bucket
    async fn add_node(&self, node: N)
    where
        N: 'async_trait,
    {
        let self_node = self.node().await;

        // Do not add ourselves to the buckets
        if node.id() == self_node.id() {
            return;
        }

        // Don't add this node if it has any external address that is the same as one of ours
        let node_addresses = node.addresses();
        if self_node.addresses().iter().any(|addr| node_addresses.contains(addr)) {
            return;
        }

        // Do not add a node to the buckets if it does not have an address
        if node.addresses().is_empty() {
            return;
        }

        let bucket_index = self.dht().get_bucket_index(&self.node().await.id(), &node.id()).await;
        let buckets_lock = self.dht().buckets.clone();
        let mut buckets = buckets_lock.write().await;
        let bucket = &mut buckets[bucket_index];

        // Node is already in the bucket
        if bucket.nodes.iter().any(|n| n.id() == node.id()) {
            return;
        }

        // Bucket is full
        if bucket.nodes.len() >= self.dht().settings.k {
            // Ping the least recently seen node
            if let Ok(channel) = self.get_channel(&bucket.nodes[0], None).await {
                let ping_res = self.ping(channel.clone()).await;
                self.cleanup_channel(channel).await;
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
    async fn update_node(&self, node: &N) {
        let bucket_index = self.dht().get_bucket_index(&self.node().await.id(), &node.id()).await;
        let buckets_lock = self.dht().buckets.clone();
        let mut buckets = buckets_lock.write().await;
        let bucket = &mut buckets[bucket_index];

        let node_index = bucket.nodes.iter().position(|n| n.id() == node.id());
        if node_index.is_none() {
            drop(buckets);
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
        node: N,
        key: &blake3::Hash,
    ) -> (N, Result<Vec<N>>)
    where
        N: 'async_trait,
    {
        let _permit = semaphore.acquire().await;
        (node.clone(), self.fetch_nodes(&node, key).await)
    }

    /// Find `k` nodes closest to a key
    async fn lookup_nodes(&self, key: &blake3::Hash) -> Result<Vec<N>> {
        info!(target: "dht::DhtHandler::lookup_nodes()", "Starting node lookup for key {}", bs58::encode(key.as_bytes()).into_string());

        let self_node_id = self.node().await.id();
        let k = self.dht().settings.k;
        let a = self.dht().settings.alpha;
        let semaphore = Arc::new(Semaphore::new(self.dht().settings.concurrency));
        let mut futures = FuturesUnordered::new();

        // Nodes we did not send a request to (yet), sorted by distance from `key`
        let mut nodes_to_visit = self.dht().find_neighbors(key, k).await;
        // Nodes with a pending request or a request completed
        let mut visited_nodes = HashSet::<blake3::Hash>::new();
        // Nodes that responded to our request, sorted by distance from `key`
        let mut result = Vec::<N>::new();

        // Create the first `alpha` tasks
        for _ in 0..a {
            match nodes_to_visit.pop() {
                Some(node) => {
                    visited_nodes.insert(node.id());
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
                    info!(target: "dht::DhtHandler::lookup_nodes", "Queried {}, got {} nodes", bs58::encode(queried_node.id().as_bytes()).into_string(), nodes.len());

                    // Remove ourselves and already known nodes from the new nodes
                    nodes.retain(|node| {
                        node.id() != self_node_id &&
                            !visited_nodes.contains(&node.id()) &&
                            !nodes_to_visit.iter().any(|n| n.id() == node.id())
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
                                let furthest_dist = BigUint::from_bytes_be(
                                    &self.dht().distance(key, &furthest.id()),
                                );
                                let next_dist = BigUint::from_bytes_be(
                                    &self.dht().distance(key, &next_node.id()),
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
                                visited_nodes.insert(node.id());
                                futures.push(self.fetch_nodes_sp(semaphore.clone(), node, key));
                            }
                            None => {
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(target: "dht::DhtHandler::lookup_nodes", "Error looking for nodes: {e}");
                }
            }
        }

        result.truncate(k);
        return Ok(result.to_vec())
    }

    /// Get a channel (existing or create a new one) to `node` about `topic`.
    /// Don't forget to call `cleanup_channel()` once you are done with it.
    async fn get_channel(&self, node: &N, topic: Option<blake3::Hash>) -> Result<ChannelPtr> {
        let channel_cache_lock = self.dht().channel_cache.clone();
        let mut channel_cache = channel_cache_lock.write().await;

        // Get existing channels for this node, regardless of topic
        let channels: HashMap<u32, ChannelCacheItem<N>> = channel_cache
            .iter()
            .filter(|&(_, item)| item.node == *node)
            .map(|(&key, item)| (key, item.clone()))
            .collect();

        let (channel_id, topic, usage_count) =
            // If we already have a channel for this node and topic, use it
            if let Some((cid, cached)) = channels.iter().find(|&(_, c)| c.topic == topic) {
                (Some(*cid), cached.topic, cached.usage_count)
            }
            // If we have a topicless channel for this node, use it
            else if let Some((cid, cached)) = channels.iter().find(|&(_, c)| c.topic.is_none()) {
                (Some(*cid), topic, cached.usage_count)
            }
            // If we don't need any specific topic, use the first channel we have
            else if topic.is_none() {
                match channels.iter().next() {
                    Some((cid, cached)) => (Some(*cid), cached.topic, cached.usage_count),
                    _ => (None, topic, 0),
                }
            }
            // There is no existing channel we can use, we will create one
            else {
                (None, topic, 0)
            };

        // If we found an existing channel we can use, try to use it
        if let Some(channel_id) = channel_id {
            if let Some(channel) = self.dht().p2p.get_channel(channel_id) {
                if channel.session_type_id() & (SESSION_SEED | SESSION_REFINE) != 0 {
                    return Err(Error::Custom(
                        "Could not get a channel (for DHT) as this is a seed or refine session"
                            .to_string(),
                    ));
                }

                if channel.is_stopped() {
                    channel.clone().start(self.dht().executor.clone());
                }

                channel_cache.insert(
                    channel_id,
                    ChannelCacheItem { node: node.clone(), topic, usage_count: usage_count + 1 },
                );
                return Ok(channel);
            }
        }

        drop(channel_cache);

        // Create a channel
        for addr in node.addresses().clone() {
            let session_out = self.dht().p2p.session_outbound();
            let session_weak = Arc::downgrade(&self.dht().p2p.session_outbound());

            let connector = Connector::new(self.dht().p2p.settings(), session_weak);
            let dur = Duration::from_secs(self.dht().settings.timeout);
            let Ok(connect_res) = timeout(dur, connector.connect(&addr)).await else {
                warn!(target: "dht::DhtHandler::get_channel()", "Timeout trying to connect to {addr}");
                return Err(Error::ConnectTimeout);
            };
            if connect_res.is_err() {
                warn!(target: "dht::DhtHandler::get_channel()", "Error while connecting to {addr}: {}", connect_res.unwrap_err());
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

            let mut channel_cache = channel_cache_lock.write().await;
            channel_cache.insert(
                channel.info.id,
                ChannelCacheItem { node: node.clone(), topic, usage_count: 1 },
            );

            return Ok(channel)
        }

        Err(Error::Custom("Could not create channel".to_string()))
    }

    /// Decrement the channel usage count, if it becomes 0 then set the topic
    /// to None, so that this channel is available for another task
    async fn cleanup_channel(&self, channel: ChannelPtr) {
        let channel_cache_lock = self.dht().channel_cache.clone();
        let mut channel_cache = channel_cache_lock.write().await;

        if let Some(cached) = channel_cache.get_mut(&channel.info.id) {
            if cached.usage_count > 0 {
                cached.usage_count -= 1;
            }

            // If the channel is not used by anything, remove the topic
            if cached.usage_count == 0 {
                cached.topic = None;
            }
        }
    }

    /// Add nodes as a provider for a key
    async fn add_to_router(
        &self,
        router: DhtRouterPtr<N>,
        key: &blake3::Hash,
        router_items: Vec<DhtRouterItem<N>>,
    ) where
        N: 'async_trait,
    {
        let mut router_items = router_items.clone();
        router_items.retain(|item| !item.node.addresses().is_empty());

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
            let keys = router_cache.get_mut(&router_item.node.id());
            if let Some(k) = keys {
                k.insert(*key);
            } else {
                let mut keys = HashSet::new();
                keys.insert(*key);
                router_cache.insert(router_item.node.id(), keys);
            }
        }
    }
}
