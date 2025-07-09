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
    cmp::Eq,
    collections::{HashMap, HashSet},
    fmt::Debug,
    hash::Hash,
    marker::{Send, Sync},
    sync::{Arc, Weak},
    time::Duration,
};

use futures::stream::FuturesUnordered;
use num_bigint::BigUint;
use smol::{
    lock::{RwLock, Semaphore},
    stream::StreamExt,
};
use tracing::{debug, info, warn};
use url::Url;

use crate::{
    net::{
        connector::Connector,
        session::{Session, SESSION_REFINE, SESSION_SEED},
        ChannelPtr, Message, P2pPtr,
    },
    system::{timeout::timeout, ExecutorPtr, PublisherPtr},
    Error, Result,
};

pub mod settings;
pub use settings::{DhtSettings, DhtSettingsOpt};

pub mod handler;
pub use handler::DhtHandler;

pub mod tasks;

pub trait DhtNode: Debug + Clone + Send + Sync + PartialEq + Eq + Hash {
    fn id(&self) -> blake3::Hash;
    fn addresses(&self) -> Vec<Url>;
}

/// Implements default Hash, PartialEq, and Eq for a struct implementing [`DhtNode`]
#[macro_export]
macro_rules! impl_dht_node_defaults {
    ($t:ty) => {
        impl std::hash::Hash for $t {
            fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
                self.id().hash(state);
            }
        }
        impl std::cmp::PartialEq for $t {
            fn eq(&self, other: &Self) -> bool {
                self.id() == other.id()
            }
        }
        impl std::cmp::Eq for $t {}
    };
}
pub use impl_dht_node_defaults;

enum DhtLookupType<V> {
    Nodes(blake3::Hash),
    Value(blake3::Hash, PublisherPtr<Option<V>>),
}

pub enum DhtLookupReply<N: DhtNode, V> {
    Nodes(Vec<N>),
    Value(V),
    NodesAndValue(Vec<N>, V),
}

pub struct DhtBucket<N: DhtNode> {
    pub nodes: Vec<N>,
}

/// Our local hash table, storing DHT keys and values
pub type DhtHashTable<V> = Arc<RwLock<HashMap<blake3::Hash, V>>>;

#[derive(Clone)]
pub struct ChannelCacheItem<N: DhtNode> {
    /// The DHT node the channel is connected to.
    pub node: N,

    /// Topic is a hash that you set to remember what the channel is about,
    /// it's not shared with the peer. If you ask for a channel (with
    /// `dht.get_channel()`) for a specific topic, it will give you a
    /// channel that has no topic, has the same topic, or a new
    /// channel.
    topic: Option<blake3::Hash>,

    /// Usage count increments when you call `handler.get_channel()` and
    /// decrements when you call `handler.cleanup_channel()`. A channel's
    /// topic is cleared on cleanup if its usage count is zero.
    usage_count: u32,
}

pub struct Dht<H: DhtHandler> {
    /// [`DhtHandler`] that implements application-specific behaviors over a [`Dht`]
    pub handler: RwLock<Weak<H>>,
    /// Are we bootstrapped?
    pub bootstrapped: Arc<RwLock<bool>>,
    /// Vec of buckets
    pub buckets: Arc<RwLock<Vec<DhtBucket<H::Node>>>>,
    /// Our local hash table, storing a part of the full DHT keys/values
    pub hash_table: DhtHashTable<H::Value>,
    /// Number of buckets
    pub n_buckets: usize,
    /// Channel ID -> ChannelCacheItem
    pub channel_cache: Arc<RwLock<HashMap<u32, ChannelCacheItem<H::Node>>>>,
    /// DHT settings
    pub settings: DhtSettings,
    /// P2P network pointer
    pub p2p: P2pPtr,
    /// Global multithreaded executor reference
    pub executor: ExecutorPtr,
}

impl<H: DhtHandler> Dht<H> {
    pub async fn new(settings: &DhtSettings, p2p: P2pPtr, ex: ExecutorPtr) -> Self {
        // Create empty buckets
        let mut buckets = vec![];
        for _ in 0..256 {
            buckets.push(DhtBucket { nodes: vec![] })
        }

        Self {
            handler: RwLock::new(Weak::new()),
            buckets: Arc::new(RwLock::new(buckets)),
            hash_table: Arc::new(RwLock::new(HashMap::new())),
            n_buckets: 256,
            bootstrapped: Arc::new(RwLock::new(false)),
            channel_cache: Arc::new(RwLock::new(HashMap::new())),

            settings: settings.clone(),

            p2p: p2p.clone(),
            executor: ex,
        }
    }

    pub async fn handler(&self) -> Arc<H> {
        self.handler.read().await.upgrade().unwrap()
    }

    pub async fn is_bootstrapped(&self) -> bool {
        let bootstrapped = self.bootstrapped.read().await;
        *bootstrapped
    }

    pub async fn set_bootstrapped(&self, value: bool) {
        let mut bootstrapped = self.bootstrapped.write().await;
        *bootstrapped = value;
    }

    /// Get the distance between `key_1` and `key_2`
    pub fn distance(&self, key_1: &blake3::Hash, key_2: &blake3::Hash) -> [u8; 32] {
        let bytes1 = key_1.as_bytes();
        let bytes2 = key_2.as_bytes();

        let mut result_bytes = [0u8; 32];

        for i in 0..32 {
            result_bytes[i] = bytes1[i] ^ bytes2[i];
        }

        result_bytes
    }

    /// Sort `nodes` by distance from `key`
    pub fn sort_by_distance(&self, nodes: &mut [H::Node], key: &blake3::Hash) {
        nodes.sort_by(|a, b| {
            let distance_a = BigUint::from_bytes_be(&self.distance(key, &a.id()));
            let distance_b = BigUint::from_bytes_be(&self.distance(key, &b.id()));
            distance_a.cmp(&distance_b)
        });
    }

    /// `key` -> bucket index
    pub async fn get_bucket_index(&self, self_node_id: &blake3::Hash, key: &blake3::Hash) -> usize {
        if key == self_node_id {
            return 0
        }
        let distance = self.distance(self_node_id, key);
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

    /// Get `n` closest known nodes to a key
    /// TODO: Can be optimized
    pub async fn find_neighbors(&self, key: &blake3::Hash, n: usize) -> Vec<H::Node> {
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

    /// Channel ID -> [`DhtNode`]
    pub async fn get_node_from_channel(&self, channel_id: u32) -> Option<H::Node> {
        let channel_cache_lock = self.channel_cache.clone();
        let channel_cache = channel_cache_lock.read().await;
        if let Some(cached) = channel_cache.get(&channel_id) {
            return Some(cached.node.clone())
        }

        None
    }

    /// Reset the DHT state (nodes and hash table)
    pub async fn reset(&self) {
        let mut bootstrapped = self.bootstrapped.write().await;
        *bootstrapped = false;

        let mut buckets = vec![];
        for _ in 0..256 {
            buckets.push(DhtBucket { nodes: vec![] })
        }

        *self.buckets.write().await = buckets;
        *self.hash_table.write().await = HashMap::new();
    }

    /// Add `value` to our hash table and send `message` for a `key` to the closest nodes found
    pub async fn announce<M: Message>(
        &self,
        key: &blake3::Hash,
        value: &H::Value,
        message: &M,
    ) -> Result<()> {
        let self_node = self.handler().await.node().await;
        if self_node.addresses().is_empty() {
            return Err(().into()); // TODO
        }

        self.handler().await.add_value(key, value).await;
        let nodes = self.lookup_nodes(key).await?;
        info!(target: "dht::announce()", "Announcing {} to {} nodes", H::key_to_string(key), nodes.len());

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
    pub async fn bootstrap(&self) {
        self.set_bootstrapped(true).await;

        let self_node_id = self.handler().await.node().await.id();
        debug!(target: "dht::bootstrap()", "DHT bootstrapping {}", H::key_to_string(&self_node_id));
        let nodes = self.lookup_nodes(&self_node_id).await;

        if nodes.is_err() || nodes.map_or(true, |v| v.is_empty()) {
            self.set_bootstrapped(false).await;
        }
    }

    /// Add a node in the correct bucket
    pub async fn add_node(&self, node: H::Node) {
        let self_node = self.handler().await.node().await;

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

        let bucket_index =
            self.get_bucket_index(&self.handler().await.node().await.id(), &node.id()).await;
        let buckets_lock = self.buckets.clone();
        let mut buckets = buckets_lock.write().await;
        let bucket = &mut buckets[bucket_index];

        // Node is already in the bucket
        if bucket.nodes.iter().any(|n| n.id() == node.id()) {
            return;
        }

        // Bucket is full
        if bucket.nodes.len() >= self.settings.k {
            // Ping the least recently seen node
            if let Ok(channel) = self.get_channel(&bucket.nodes[0], None).await {
                let ping_res = self.handler().await.ping(channel.clone()).await;
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
    pub async fn update_node(&self, node: &H::Node) {
        let bucket_index =
            self.get_bucket_index(&self.handler().await.node().await.id(), &node.id()).await;
        let buckets_lock = self.buckets.clone();
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

    /// Lookup algorithm for both nodes lookup and value lookup
    async fn lookup(&self, lookup_type: DhtLookupType<H::Value>) -> Result<Vec<H::Node>> {
        let (key, value_pub) = match lookup_type {
            DhtLookupType::Nodes(key) => (key, None),
            DhtLookupType::Value(key, ref pub_ptr) => (key, Some(pub_ptr)),
        };

        let (k, a) = (self.settings.k, self.settings.alpha);
        let semaphore = Arc::new(Semaphore::new(self.settings.concurrency));

        let mut unique_nodes = HashSet::new();
        let mut nodes_to_visit = self.find_neighbors(&key, k).await;
        let mut result = Vec::new();
        let mut futures = FuturesUnordered::new();

        let distance_check = |(furthest, next): (&H::Node, &H::Node)| {
            BigUint::from_bytes_be(&self.distance(&key, &furthest.id())) <
                BigUint::from_bytes_be(&self.distance(&key, &next.id()))
        };

        let lookup = async |node: H::Node, key| {
            let _permit = semaphore.acquire().await;
            let n = node.clone();
            let handler = self.handler().await;
            match &lookup_type {
                DhtLookupType::Nodes(_) => {
                    (n, handler.find_nodes(&node, key).await.map(DhtLookupReply::Nodes))
                }
                DhtLookupType::Value(_, _) => (n, handler.find_value(&node, key).await),
            }
        };

        let spawn_futures = async |nodes_to_visit: &mut Vec<H::Node>,
                                   unique_nodes: &mut HashSet<_>,
                                   futures: &mut FuturesUnordered<_>| {
            for _ in 0..a {
                if let Some(node) = nodes_to_visit.pop() {
                    unique_nodes.insert(node.id());
                    futures.push(Box::pin(lookup(node, &key)));
                }
            }
        };

        spawn_futures(&mut nodes_to_visit, &mut unique_nodes, &mut futures).await; // Initial alpha tasks

        while let Some((queried_node, res)) = futures.next().await {
            if let Err(e) = res {
                warn!(target: "dht::lookup()", "Error in DHT lookup: {e}");

                // Spawn next `alpha` futures if there are no more futures but
                // we still have nodes to visit
                if futures.is_empty() {
                    spawn_futures(&mut nodes_to_visit, &mut unique_nodes, &mut futures).await;
                }

                continue;
            }

            let (nodes, value) = match res.unwrap() {
                DhtLookupReply::Nodes(nodes) => (Some(nodes), None),
                DhtLookupReply::Value(value) => (None, Some(value)),
                DhtLookupReply::NodesAndValue(nodes, value) => (Some(nodes), Some(value)),
            };

            if let Some(value) = value {
                if let Some(publisher) = value_pub {
                    publisher.notify(Some(value)).await;
                }
            }

            if let Some(mut nodes) = nodes {
                let self_id = self.handler().await.node().await.id();
                nodes.retain(|node| node.id() != self_id && unique_nodes.insert(node.id()));

                nodes_to_visit.extend(nodes.clone());
                self.sort_by_distance(&mut nodes_to_visit, &key);
            }

            result.push(queried_node);
            self.sort_by_distance(&mut result, &key);

            // Early termination logic
            if result.len() >= k &&
                result.last().zip(nodes_to_visit.first()).is_some_and(distance_check)
            {
                break;
            }

            // Spawn next `alpha` futures
            spawn_futures(&mut nodes_to_visit, &mut unique_nodes, &mut futures).await;
        }

        if let Some(publisher) = value_pub {
            publisher.notify(None).await;
        }

        Ok(result.into_iter().take(k).collect())
    }

    /// Find `k` nodes closest to a key
    pub async fn lookup_nodes(&self, key: &blake3::Hash) -> Result<Vec<H::Node>> {
        info!(target: "dht::lookup_nodes()", "Starting node lookup for key {}", H::key_to_string(key));
        self.lookup(DhtLookupType::Nodes(*key)).await
    }

    /// Find value for `key`
    pub async fn lookup_value(
        &self,
        key: &blake3::Hash,
        value_pub: PublisherPtr<Option<H::Value>>,
    ) -> Result<Vec<H::Node>> {
        info!(target: "dht::lookup_value()", "Starting value lookup for key {}", H::key_to_string(key));
        self.lookup(DhtLookupType::Value(*key, value_pub)).await
    }

    /// Get a channel (existing or create a new one) to `node` about `topic`.
    /// Don't forget to call `cleanup_channel()` once you are done with it.
    pub async fn get_channel(
        &self,
        node: &H::Node,
        topic: Option<blake3::Hash>,
    ) -> Result<ChannelPtr> {
        let channel_cache_lock = self.channel_cache.clone();
        let mut channel_cache = channel_cache_lock.write().await;

        // Get existing channels for this node, regardless of topic
        let channels: HashMap<u32, ChannelCacheItem<H::Node>> = channel_cache
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
            if let Some(channel) = self.p2p.get_channel(channel_id) {
                if channel.session_type_id() & (SESSION_SEED | SESSION_REFINE) != 0 {
                    return Err(Error::Custom(
                        "Could not get a channel (for DHT) as this is a seed or refine session"
                            .to_string(),
                    ));
                }

                if channel.is_stopped() {
                    channel.clone().start(self.executor.clone());
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
            let session_out = self.p2p.session_outbound();
            let session_weak = Arc::downgrade(&self.p2p.session_outbound());

            let connector = Connector::new(self.p2p.settings(), session_weak);
            let dur = Duration::from_secs(self.settings.timeout);
            let Ok(connect_res) = timeout(dur, connector.connect(&addr)).await else {
                warn!(target: "dht::get_channel()", "Timeout trying to connect to {addr}");
                return Err(Error::ConnectTimeout);
            };
            if connect_res.is_err() {
                warn!(target: "dht::get_channel()", "Error while connecting: {}", connect_res.unwrap_err());
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
                session_out.register_channel(channel.clone(), self.executor.clone()).await;
            if register_res.is_err() {
                channel.clone().stop().await;
                warn!(target: "dht::get_channel()", "Error while registering channel {}: {}", channel.info.id, register_res.unwrap_err());
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
    pub async fn cleanup_channel(&self, channel: ChannelPtr) {
        let channel_cache_lock = self.channel_cache.clone();
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
}
