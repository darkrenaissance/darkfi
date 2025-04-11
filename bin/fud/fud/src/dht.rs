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

use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::Arc,
};

use async_trait::async_trait;
use darkfi::{
    net::{connector::Connector, session::Session, ChannelPtr, Message, P2pPtr},
    system::{sleep, ExecutorPtr},
    util::time::Timestamp,
    Error, Result,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};
use futures::future::join_all;
use log::{debug, error, warn};
use num_bigint::BigUint;
use smol::lock::RwLock;
use url::Url;

#[derive(Debug, Clone, SerialEncodable, SerialDecodable, Eq)]
pub struct DhtNode {
    pub id: blake3::Hash,
    pub addresses: Vec<Url>,
}

impl Hash for DhtNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl PartialEq for DhtNode {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

pub struct DhtBucket {
    pub nodes: Vec<DhtNode>,
}

/// "Router" means: Key -> Set of nodes (+ additional data for each node)
pub type DhtRouterPtr = Arc<RwLock<HashMap<blake3::Hash, HashSet<DhtRouterItem>>>>;

#[derive(Debug, Clone, SerialEncodable, SerialDecodable, Eq)]
pub struct DhtRouterItem {
    pub node: DhtNode,
    pub timestamp: u64,
}

impl Hash for DhtRouterItem {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node.id.hash(state);
    }
}

impl PartialEq for DhtRouterItem {
    fn eq(&self, other: &Self) -> bool {
        self.node.id == other.node.id
    }
}

impl From<DhtNode> for DhtRouterItem {
    fn from(node: DhtNode) -> Self {
        DhtRouterItem { node, timestamp: Timestamp::current_time().inner() }
    }
}

// TODO: Add a DhtSettings
pub struct Dht {
    /// Our own node id
    pub node_id: blake3::Hash,
    /// Are we bootstrapped?
    pub bootstrapped: Arc<RwLock<bool>>,
    /// Vec of buckets
    pub buckets: Arc<RwLock<Vec<DhtBucket>>>,
    /// Number of parallel lookup requests
    pub alpha: usize,
    /// Number of nodes in a bucket
    pub k: usize,
    /// Number of buckets
    pub n_buckets: usize,
    /// Channel ID -> Node ID
    pub node_cache: Arc<RwLock<HashMap<u32, DhtNode>>>,
    /// Node ID -> Channel ID
    pub channel_cache: Arc<RwLock<HashMap<blake3::Hash, u32>>>,
    /// Node ID -> Set of keys
    pub router_cache: Arc<RwLock<HashMap<blake3::Hash, HashSet<blake3::Hash>>>>,
    /// Seconds
    pub timeout: u64,

    pub p2p: P2pPtr,
    pub executor: ExecutorPtr,
}
impl Dht {
    pub async fn new(
        node_id: &blake3::Hash,
        a: usize,
        k: usize,
        timeout: u64,
        p2p: P2pPtr,
        ex: ExecutorPtr,
    ) -> Self {
        // Create empty buckets
        let mut buckets = vec![];
        for _ in 0..256 {
            buckets.push(DhtBucket { nodes: vec![] })
        }

        Self {
            node_id: *node_id,
            buckets: Arc::new(RwLock::new(buckets)),
            bootstrapped: Arc::new(RwLock::new(false)),
            alpha: a,
            k,
            n_buckets: 256,
            node_cache: Arc::new(RwLock::new(HashMap::new())),
            channel_cache: Arc::new(RwLock::new(HashMap::new())),
            router_cache: Arc::new(RwLock::new(HashMap::new())),
            timeout,

            p2p: p2p.clone(),
            executor: ex,
        }
    }

    pub async fn is_bootstrapped(&self) -> bool {
        let bootstrapped = self.bootstrapped.read().await;
        *bootstrapped
    }

    pub async fn set_bootstrapped(&self) {
        let mut bootstrapped = self.bootstrapped.write().await;
        *bootstrapped = true;
    }

    /// Get own node
    pub async fn node(&self) -> DhtNode {
        DhtNode {
            id: self.node_id,
            addresses: self
                .p2p
                .clone()
                .hosts()
                .external_addrs()
                .await
                .iter()
                .filter(|addr| !addr.to_string().contains("[::]"))
                .cloned()
                .collect(),
        }
    }

    // Get the distance between `key_1` and `key_2`
    pub fn distance(&self, key_1: &blake3::Hash, key_2: &blake3::Hash) -> [u8; 32] {
        let bytes1 = key_1.as_bytes();
        let bytes2 = key_2.as_bytes();

        let mut result_bytes = [0u8; 32];

        for i in 0..32 {
            result_bytes[i] = bytes1[i] ^ bytes2[i];
        }

        result_bytes
    }

    // Sort `nodes` by distance from `key`
    pub fn sort_by_distance(&self, nodes: &mut [DhtNode], key: &blake3::Hash) {
        nodes.sort_by(|a, b| {
            let distance_a = BigUint::from_bytes_be(&self.distance(key, &a.id));
            let distance_b = BigUint::from_bytes_be(&self.distance(key, &b.id));
            distance_a.cmp(&distance_b)
        });
    }

    // key -> bucket index
    pub async fn get_bucket_index(&self, key: &blake3::Hash) -> usize {
        if key == &self.node_id {
            return 0
        }
        let distance = self.distance(&self.node_id, key);
        let mut leading_zeros = 0;

        for &byte in &distance {
            if byte == 0 {
                leading_zeros += 8;
            } else {
                leading_zeros += byte.leading_zeros() as usize;
                break;
            }
        }

        let bucket_index = self.n_buckets - leading_zeros;
        std::cmp::min(bucket_index, self.n_buckets - 1)
    }

    // Get `n` closest known nodes to a key
    // TODO: Can be optimized
    pub async fn find_neighbors(&self, key: &blake3::Hash, n: usize) -> Vec<DhtNode> {
        let buckets_lock = self.buckets.clone();
        let buckets = buckets_lock.read().await;

        let mut neighbors = Vec::new();

        for i in 0..self.n_buckets {
            if let Some(bucket) = buckets.get(i) {
                neighbors.extend(bucket.nodes.iter().cloned());
            }
        }

        self.sort_by_distance(&mut neighbors, key);

        neighbors.truncate(n);

        neighbors
    }

    // Channel ID -> DhtNode
    pub async fn get_node_from_channel(&self, channel_id: u32) -> Option<DhtNode> {
        let node_cache_lock = self.node_cache.clone();
        let node_cache = node_cache_lock.read().await;
        node_cache.get(&channel_id).cloned()
    }

    // Remove nodes in router that are older than expiry_secs
    pub async fn prune_router(&self, router: DhtRouterPtr, expiry_secs: u32) {
        let expiry_timestamp = Timestamp::current_time().inner() - (expiry_secs as u64);
        let mut router_write = router.write().await;

        let keys: Vec<_> = router_write.keys().cloned().collect();

        for key in keys {
            let items = router_write.get_mut(&key).unwrap();
            items.retain(|item| item.timestamp > expiry_timestamp);
            if items.is_empty() {
                router_write.remove(&key);
            }
        }
    }
}

#[async_trait]
pub trait DhtHandler {
    fn dht(&self) -> Arc<Dht>;

    // Send a DHT ping request
    async fn ping(&self, channel: ChannelPtr) -> Result<DhtNode>;

    // Triggered when we find a new node
    async fn on_new_node(&self, node: &DhtNode) -> Result<()>;

    // Send FIND NODES request to a peer to get nodes close to `key`
    async fn fetch_nodes(&self, node: &DhtNode, key: &blake3::Hash) -> Result<Vec<DhtNode>>;

    // Announce message `m` for a key, and add ourselves to router
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

    // Send a DHT ping request when there is a new channel, to know the node id of the new peer,
    // Then fill the channel cache and the buckets
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
            if !channel.is_stopped() && !channel_cache.values().any(|&v| v == channel.info.id) {
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
    }

    // Remove disconnected nodes from the channel cache
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

    // Add a node in the correct bucket
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
        if bucket.nodes.len() >= self.dht().k {
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

    // Find nodes closest to a key
    async fn lookup_nodes(&self, key: &blake3::Hash) -> Result<Vec<DhtNode>> {
        debug!(target: "dht::DhtHandler::lookup_nodes()", "Starting node lookup for key {}", key);

        let k = self.dht().k;
        let a = self.dht().alpha;
        let mut visited_nodes = HashSet::new();
        let mut nodes_to_visit = self.dht().find_neighbors(key, k).await;
        let mut nearest_nodes: Vec<DhtNode> = vec![];

        while !nodes_to_visit.is_empty() {
            let mut queries: Vec<DhtNode> = Vec::with_capacity(a);

            // Get `alpha` nodes from `nodes_to_visit` which is sorted by distance
            for _ in 0..a {
                match nodes_to_visit.pop() {
                    Some(node) => {
                        queries.push(node);
                    }
                    None => {
                        break;
                    }
                }
            }

            let mut tasks = Vec::with_capacity(queries.len());
            for node in &queries {
                // Avoid visiting the same node multiple times
                if !visited_nodes.insert(node.id) {
                    continue;
                }

                // Query the node for the value associated with the key
                tasks.push(self.fetch_nodes(node, key));
            }

            let results = join_all(tasks).await;
            for (i, value_result) in results.into_iter().enumerate() {
                match value_result {
                    Ok(mut nodes) => {
                        // Remove ourselves from the new nodes
                        nodes.retain(|node| node.id != self.dht().node_id);

                        // Add each new node to our buckets
                        for node in nodes.clone() {
                            self.add_node(node).await;
                        }

                        // Add nodes to the list of nodes to visit
                        nodes_to_visit.extend(nodes);
                        self.dht().sort_by_distance(&mut nodes_to_visit, key);

                        // Update nearest_nodes
                        nearest_nodes.push(queries[i].clone());
                        self.dht().sort_by_distance(&mut nearest_nodes, key);
                    }
                    Err(e) => {
                        error!(target: "dht::DhtHandler::lookup_nodes", "{}", e);
                    }
                }
            }

            // Early termination check
            // Stops if our furthest visited node is closer than the closest node we will query
            if let Some(furthest) = nearest_nodes.last() {
                if let Some(next_node) = nodes_to_visit.first() {
                    let furthest_dist =
                        BigUint::from_bytes_be(&self.dht().distance(key, &furthest.id));
                    let next_dist =
                        BigUint::from_bytes_be(&self.dht().distance(key, &next_node.id));
                    if furthest_dist < next_dist {
                        break;
                    }
                }
            }
        }

        nearest_nodes.truncate(k);
        return Ok(nearest_nodes)
    }

    // Get an existing channel, or create a new one
    async fn get_channel(&self, node: &DhtNode) -> Result<ChannelPtr> {
        let channel_cache_lock = self.dht().channel_cache.clone();
        let channel_cache = channel_cache_lock.read().await;

        if let Some(channel_id) = channel_cache.get(&node.id) {
            if let Some(channel) = self.dht().p2p.get_channel(*channel_id) {
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
            let connect_res = connector.connect(&addr).await;
            if connect_res.is_err() {
                warn!(target: "dht::DhtHandler::get_channel()", "Error while connecting to {}: {}", addr, connect_res.unwrap_err());
                continue;
            }
            let (_, channel) = connect_res.unwrap();
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

    // Add nodes as a provider for a key
    async fn add_to_router(
        &self,
        router: DhtRouterPtr,
        key: &blake3::Hash,
        router_items: Vec<DhtRouterItem>,
    ) {
        let mut router_items = router_items.clone();
        router_items.retain(|item| !item.node.addresses.is_empty());

        debug!(target: "dht::DhtHandler::add_to_router()", "Inserting {} nodes to key {}", router_items.len(), key);

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
