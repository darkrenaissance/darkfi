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
};

use futures::stream::FuturesUnordered;
use num_bigint::BigUint;
use smol::{
    channel,
    lock::{Mutex, RwLock, Semaphore},
    stream::StreamExt,
};
use tracing::{info, warn};
use url::Url;

use crate::{
    dht::event::DhtEvent,
    net::{
        connector::Connector,
        session::{SESSION_DIRECT, SESSION_MANUAL},
        ChannelPtr, Message, P2pPtr,
    },
    system::{msleep, ExecutorPtr, Publisher, PublisherPtr, Subscription},
    util::time::Timestamp,
    Error, Result,
};

pub mod settings;
pub use settings::{DhtSettings, DhtSettingsOpt};

pub mod handler;
pub use handler::DhtHandler;

pub mod tasks;

pub mod event;

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

enum DhtLookupType {
    Nodes,
    Value,
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

type PingLock<N> = Arc<Mutex<Option<Result<N>>>>;

#[derive(Clone, Debug)]
pub struct ChannelCacheItem<N: DhtNode> {
    /// The DHT node the channel is connected to.
    pub node: Option<N>,
    /// The last time this channel was used by the [`DhtHandler`]. It's used
    /// to stop inbound connections in [`crate::dht::tasks::disconnect_inbounds_task()`].
    pub last_used: Timestamp,
    /// Have we already received a DHT ping from this channel?
    pub ping_received: bool,
    /// Have we already sent a DHT ping to this channel?
    pub ping_sent: bool,
}

#[derive(Clone, Debug)]
pub struct HostCacheItem {
    /// The last time we tried to send a DHT ping to this host.
    pub last_ping: Timestamp,
    /// The last known node id for this host.
    pub node_id: blake3::Hash,
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
    /// Host address -> ChannelCacheItem
    pub host_cache: Arc<RwLock<HashMap<Url, HostCacheItem>>>,
    /// Locks that prevent pinging the same channel multiple times at once.
    ping_locks: Arc<Mutex<HashMap<u32, PingLock<H::Node>>>>,
    /// Add node sender
    pub add_node_tx: channel::Sender<(H::Node, ChannelPtr)>,
    /// Add node receiver
    pub add_node_rx: channel::Receiver<(H::Node, ChannelPtr)>,
    /// DHT settings
    pub settings: DhtSettings,
    /// DHT event publisher
    pub event_publisher: PublisherPtr<DhtEvent<H::Node, H::Value>>,
    /// P2P network pointer
    pub p2p: P2pPtr,
    /// Connector to create manual connections
    pub connector: Connector,
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

        let (add_node_tx, add_node_rx) = smol::channel::unbounded();

        let session_weak = Arc::downgrade(&p2p.session_manual());
        let connector = Connector::new(p2p.settings(), session_weak);

        Self {
            handler: RwLock::new(Weak::new()),
            buckets: Arc::new(RwLock::new(buckets)),
            hash_table: Arc::new(RwLock::new(HashMap::new())),
            n_buckets: 256,
            bootstrapped: Arc::new(RwLock::new(false)),
            channel_cache: Arc::new(RwLock::new(HashMap::new())),
            host_cache: Arc::new(RwLock::new(HashMap::new())),
            ping_locks: Arc::new(Mutex::new(HashMap::new())),
            add_node_tx,
            add_node_rx,

            event_publisher: Publisher::new(),

            settings: settings.clone(),

            p2p: p2p.clone(),
            connector,
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

    pub async fn subscribe(&self) -> Subscription<DhtEvent<H::Node, H::Value>> {
        self.event_publisher.clone().subscribe().await
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
            return 0;
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
            return cached.node.clone();
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
        let self_node = self.handler().await.node().await?;
        if self_node.addresses().is_empty() {
            return Err(().into()); // TODO
        }

        self.handler().await.add_value(key, value).await;
        let nodes = self.lookup_nodes(key).await;
        info!(target: "dht::announce()", "[DHT] Announcing {} to {} nodes", H::key_to_string(key), nodes.len());

        for node in nodes {
            if let Ok((channel, _)) = self.get_channel(&node).await {
                let _ = channel.send(message).await;
                self.cleanup_channel(channel).await;
            }
        }

        Ok(())
    }

    /// Lookup our own node id
    pub async fn bootstrap(&self) {
        let self_node = self.handler().await.node().await;
        if self_node.is_err() {
            return;
        }
        let self_node = self_node.unwrap();

        self.set_bootstrapped(true).await;

        info!(target: "dht::bootstrap()", "[DHT] Bootstrapping");
        self.event_publisher.notify(DhtEvent::BootstrapStarted).await;

        let _nodes = self.lookup_nodes(&self_node.id()).await;

        // if nodes.is_empty() {
        //     self.set_bootstrapped(false).await;
        // } else {
        // }

        self.event_publisher.notify(DhtEvent::BootstrapCompleted).await;
    }

    // TODO: Optimize this
    async fn on_new_node(&self, node: &H::Node, channel: ChannelPtr) {
        info!(target: "dht::on_new_node()", "[DHT] Found new node {}", H::key_to_string(&node.id()));

        // If this is the first node we know about then bootstrap
        if !self.is_bootstrapped().await {
            self.bootstrap().await;
        }

        // Send keys that are closer to this node than we are
        let self_node = self.handler().await.node().await;
        if self_node.is_err() {
            return;
        }
        let self_id = self_node.unwrap().id();
        for (key, value) in self.hash_table.read().await.iter() {
            let node_distance = BigUint::from_bytes_be(&self.distance(key, &node.id()));
            let self_distance = BigUint::from_bytes_be(&self.distance(key, &self_id));
            if node_distance <= self_distance {
                let _ = self.handler().await.store(channel.clone(), key, value).await;
            }
        }
    }

    /// Move a node to the tail in its bucket,
    /// to show that it is the most recently seen in the bucket.
    /// If the node is not in a bucket it will be added using `add_node`.
    pub async fn update_node(&self, node: &H::Node, channel: ChannelPtr) {
        self.p2p.session_direct().inc_channel_usage(&channel, 1).await;
        if let Err(e) = self.add_node_tx.send((node.clone(), channel.clone())).await {
            warn!(target: "dht::update_node()", "[DHT] Cannot add node {}: {e}", H::key_to_string(&node.id()))
        }
    }

    /// Remove a node from the buckets.
    pub async fn remove_node(&self, node_id: &blake3::Hash) {
        let handler = self.handler().await;
        let self_node = handler.node().await;
        if self_node.is_err() {
            return;
        }
        let bucket_index = handler.dht().get_bucket_index(&self_node.unwrap().id(), node_id).await;
        let buckets_lock = handler.dht().buckets.clone();
        let mut buckets = buckets_lock.write().await;
        let bucket = &mut buckets[bucket_index];
        bucket.nodes.retain(|node| node.id() != *node_id);
    }

    /// Send a DHT ping to `channel` using the handler's ping method.
    /// Prevents sending multiple pings at once to the same channel.
    pub async fn ping(&self, channel: ChannelPtr) -> Result<H::Node> {
        let lock_map = self.ping_locks.clone();
        let mut locks = lock_map.lock().await;

        // Get or create the lock
        let lock = if let Some(lock) = locks.get(&channel.info.id) {
            lock.clone()
        } else {
            let lock = Arc::new(Mutex::new(None));
            locks.insert(channel.info.id, lock.clone());
            lock
        };
        drop(locks);

        // Acquire the lock
        let mut result = lock.lock().await;

        if let Some(res) = result.clone() {
            return res
        }

        // Do the actual pinging process as defined by the handler
        let ping_result = self.handler().await.ping(channel.clone()).await;
        *result = Some(ping_result.clone());
        ping_result
    }

    /// Lookup algorithm for both nodes lookup and value lookup.
    async fn lookup(
        &self,
        key: blake3::Hash,
        lookup_type: DhtLookupType,
    ) -> (Vec<H::Node>, Vec<H::Value>) {
        let net_settings = self.p2p.settings().read_arc().await;
        let active_profiles = net_settings.active_profiles.clone();
        drop(net_settings);
        let external_addrs = self.p2p.hosts().external_addrs().await;

        let (k, a) = (self.settings.k, self.settings.alpha);
        let semaphore = Arc::new(Semaphore::new(self.settings.concurrency));
        let queried_addrs = Arc::new(Mutex::new(HashSet::new()));
        let mut seen_nodes = HashSet::new();
        let mut nodes_to_visit = self.find_neighbors(&key, k).await;
        let mut result = Vec::new();
        let mut futures = FuturesUnordered::new();
        let mut consecutive_stalls = 0;

        let mut values = Vec::new();

        let distance_check = |(furthest, next): (&H::Node, &H::Node)| {
            BigUint::from_bytes_be(&self.distance(&key, &furthest.id())) <
                BigUint::from_bytes_be(&self.distance(&key, &next.id()))
        };

        // Create a channel if necessary and send a FIND NODES or FIND VALUE
        // request to `addr`
        let lookup = async |node: H::Node, key, addrs: Vec<Url>| {
            let _permit = semaphore.acquire().await;

            // Try all valid addresses for the node
            let mut last_err = None;
            for addr in addrs {
                let mut queried_addrs_set = queried_addrs.lock().await;
                // Skip if this address has already been queried
                if queried_addrs_set.contains(&addr) {
                    continue;
                }
                queried_addrs_set.insert(addr.clone());
                drop(queried_addrs_set);

                // Try to create or find an existing channel
                let channel = self.create_channel(&addr).await.map(|(ch, _)| ch);

                if let Err(e) = channel {
                    last_err = Some(e);
                    continue
                }
                let channel = channel.unwrap();

                let handler = self.handler().await;
                let res = match &lookup_type {
                    DhtLookupType::Nodes => {
                        info!(target: "dht::lookup()", "[DHT] [LOOKUP] Querying node {} for nodes lookup of key {}", H::key_to_string(&node.id()), H::key_to_string(key));
                        handler.find_nodes(channel.clone(), key).await.map(DhtLookupReply::Nodes)
                    }
                    DhtLookupType::Value => {
                        info!(target: "dht::lookup()", "[DHT] [LOOKUP] Querying node {} for value lookup of key {}", H::key_to_string(&node.id()), H::key_to_string(key));
                        handler.find_value(channel.clone(), key).await
                    }
                };

                self.cleanup_channel(channel).await;
                if res.is_ok() {
                    return (node, res)
                }
                last_err = res.err();
            }
            if let Some(e) = last_err {
                return (node, Err(e))
            }

            (node, Err(Error::Custom("All node's addresses failed".to_string())))
        };

        // Spawn up to `alpha` futures for lookup()
        let spawn_futures = async |nodes_to_visit: &mut Vec<H::Node>,
                                   futures: &mut FuturesUnordered<_>| {
            for _ in 0..a {
                if !nodes_to_visit.is_empty() {
                    let node = nodes_to_visit.remove(0);
                    let valid_addrs: Vec<Url> = node
                        .addresses()
                        .iter()
                        .filter(|addr| {
                            active_profiles.contains(&addr.scheme().to_string()) &&
                                !external_addrs.contains(addr)
                        })
                        .cloned()
                        .collect();
                    if !valid_addrs.is_empty() {
                        futures.push(Box::pin(lookup(node, &key, valid_addrs)));
                    }
                }
            }
        };

        // Initial futures
        spawn_futures(&mut nodes_to_visit, &mut futures).await;

        // Process lookup responses
        while let Some((queried_node, res)) = futures.next().await {
            if let Err(e) = res {
                warn!(target: "dht::lookup()", "[DHT] [LOOKUP] Error in lookup: {e}");

                // Spawn next `alpha` futures if there are no more futures but
                // we still have nodes to visit
                if futures.is_empty() {
                    spawn_futures(&mut nodes_to_visit, &mut futures).await;
                }

                continue;
            }

            let (nodes, value) = match res.unwrap() {
                DhtLookupReply::Nodes(nodes) => (Some(nodes), None),
                DhtLookupReply::Value(value) => (None, Some(value)),
                DhtLookupReply::NodesAndValue(nodes, value) => (Some(nodes), Some(value)),
            };

            // Send the value we found to the publisher
            if let Some(value) = value {
                info!(target: "dht::lookup()", "[DHT] [LOOKUP] Found value for {} from {}", H::key_to_string(&key), H::key_to_string(&queried_node.id()));
                values.push(value.clone());
                self.event_publisher.notify(DhtEvent::ValueFound { key, value }).await;
            }

            // Update nodes_to_visit
            if let Some(mut nodes) = nodes {
                if !nodes.is_empty() {
                    info!(target: "dht::lookup()", "[DHT] [LOOKUP] Found {} nodes from {}", nodes.len(), H::key_to_string(&queried_node.id()));

                    self.event_publisher
                        .notify(DhtEvent::NodesFound { key, nodes: nodes.clone() })
                        .await;

                    // Remove our own node and duplicates
                    if let Ok(self_node) = self.handler().await.node().await {
                        let self_id = self_node.id();
                        nodes.retain(|node: &H::Node| {
                            node.id() != self_id && seen_nodes.insert(node.id())
                        });
                    }

                    // Add new nodes to the list of nodes to visit
                    nodes_to_visit.extend(nodes.clone());
                    self.sort_by_distance(&mut nodes_to_visit, &key);
                }
            }

            result.push(queried_node);
            self.sort_by_distance(&mut result, &key);

            // Early termination logic:
            // The closest node to visit must be further than the furthest
            // queried node, 3 consecutive times
            if result.len() >= k &&
                result.last().zip(nodes_to_visit.first()).is_some_and(distance_check)
            {
                consecutive_stalls += 1;
                if consecutive_stalls >= 3 {
                    break;
                }
            } else {
                consecutive_stalls = 0;
            }

            // Spawn next `alpha` futures
            spawn_futures(&mut nodes_to_visit, &mut futures).await;
        }

        info!(target: "dht::lookup()", "[DHT] [LOOKUP] Lookup for {} completed", H::key_to_string(&key));

        let nodes: Vec<_> = result.into_iter().take(k).collect();
        (nodes, values)
    }

    /// Find `k` nodes closest to a key
    pub async fn lookup_nodes(&self, key: &blake3::Hash) -> Vec<H::Node> {
        info!(target: "dht::lookup_nodes()", "[DHT] [LOOKUP] Starting node lookup for key {}", H::key_to_string(key));

        self.event_publisher.notify(DhtEvent::NodesLookupStarted { key: *key }).await;

        let (nodes, _) = self.lookup(*key, DhtLookupType::Nodes).await;

        self.event_publisher
            .notify(DhtEvent::NodesLookupCompleted { key: *key, nodes: nodes.clone() })
            .await;

        nodes
    }

    /// Find value for `key`
    pub async fn lookup_value(&self, key: &blake3::Hash) -> (Vec<H::Node>, Vec<H::Value>) {
        info!(target: "dht::lookup_value()", "[DHT] [LOOKUP] Starting value lookup for key {}", H::key_to_string(key));

        self.event_publisher.notify(DhtEvent::ValueLookupStarted { key: *key }).await;

        let (nodes, values) = self.lookup(*key, DhtLookupType::Value).await;

        self.event_publisher
            .notify(DhtEvent::ValueLookupCompleted {
                key: *key,
                nodes: nodes.clone(),
                values: values.clone(),
            })
            .await;

        (nodes, values)
    }

    /// Update a channel's `last_used` field in the channel cache.
    pub async fn update_channel(&self, channel_id: u32) {
        let channel_cache_lock = self.channel_cache.clone();
        let mut channel_cache = channel_cache_lock.write().await;

        if let Some(cached) = channel_cache.get_mut(&channel_id) {
            cached.last_used = Timestamp::current_time();
        }
    }

    /// Get a channel (existing or create a new one) to `node`.
    /// Don't forget to call `cleanup_channel()` once you are done with it.
    pub async fn get_channel(&self, node: &H::Node) -> Result<(ChannelPtr, H::Node)> {
        let node_id = node.id();

        // Look in the channel cache for a channel connected to this node.
        // We skip direct session channels, for those we will call
        // `create_channel()` which increments the sessions's usage counter.
        let channel_cache = self.channel_cache.read().await.clone();
        if let Some((channel_id, cached)) = channel_cache
            .clone()
            .iter()
            .find(|(_, cached)| cached.node.clone().is_some_and(|n| n.id() == node_id))
        {
            if let Some(channel) = self.p2p.get_channel(*channel_id) {
                if channel.session_type_id() & SESSION_DIRECT == 0 {
                    if channel.is_stopped() {
                        self.cleanup_channel(channel).await;
                    } else {
                        return Ok((channel, cached.node.clone().unwrap()))
                    }
                }
            }
        }

        self.create_channel_to_node(node).await
    }

    /// Create a channel in the direct session, ping the peer, add the
    /// DHT node to our buckets and the channel to our channel cache.
    pub async fn create_channel(&self, addr: &Url) -> Result<(ChannelPtr, H::Node)> {
        let external_addrs = self.p2p.hosts().external_addrs().await;
        if external_addrs.contains(addr) {
            return Err(Error::Custom(
                "Can't create a channel to our own external address".to_string(),
            ))
        }

        let channel = self.p2p.session_direct().get_channel(addr).await?;
        let channel_cache = self.channel_cache.read().await;
        if let Some(cached) = channel_cache.get(&channel.info.id) {
            if let Some(node) = &cached.node {
                return Ok((channel, node.clone()))
            }
        }
        drop(channel_cache);

        let node = self.ping(channel.clone()).await;
        // If ping failed, cleanup the channel and abort
        if let Err(e) = node {
            self.cleanup_channel(channel).await;
            return Err(e);
        }
        let node = node.unwrap();
        self.add_channel_to_cache(channel.info.id, &node).await;
        Ok((channel, node))
    }

    pub async fn create_channel_to_node(&self, node: &H::Node) -> Result<(ChannelPtr, H::Node)> {
        let net_settings = self.p2p.settings().read_arc().await;
        let active_profiles = net_settings.active_profiles.clone();
        drop(net_settings);

        // Create a channel
        let mut addrs = node.addresses().clone();
        addrs.retain(|addr| active_profiles.contains(&addr.scheme().to_string()));
        for addr in addrs {
            let res = self.create_channel(&addr).await;

            if res.is_err() {
                continue;
            }

            let (channel, node) = res.unwrap();
            return Ok((channel, node));
        }

        Err(Error::Custom("Could not create channel".to_string()))
    }

    /// Insert a channel to the DHT's channel cache. If the channel is already
    /// in the cache, `last_used` is updated.
    pub async fn add_channel_to_cache(&self, channel_id: u32, node: &H::Node) {
        let mut channel_cache = self.channel_cache.write().await;
        channel_cache
            .entry(channel_id)
            .and_modify(|c| c.last_used = Timestamp::current_time())
            .or_insert(ChannelCacheItem {
                node: Some(node.clone()),
                last_used: Timestamp::current_time(),
                ping_received: false,
                ping_sent: false,
            });
    }

    /// Wait until we received a DHT ping and sent a DHT ping on a channel.
    pub async fn wait_fully_pinged(&self, channel_id: u32) -> Result<()> {
        loop {
            let channel_cache = self.channel_cache.read().await;
            let cached = channel_cache
                .get(&channel_id)
                .ok_or(Error::Custom("Missing channel".to_string()))?;
            if cached.ping_received && cached.ping_sent {
                return Ok(())
            }
            drop(channel_cache);
            msleep(100).await;
        }
    }

    /// Call [`crate::net::session::DirectSession::cleanup_channel()`] and cleanup the DHT caches.
    pub async fn cleanup_channel(&self, channel: ChannelPtr) {
        let channel_cache_lock = self.channel_cache.clone();
        let mut channel_cache = channel_cache_lock.write().await;
        let mut ping_locks = self.ping_locks.lock().await;
        if self.p2p.session_direct().cleanup_channel(channel.clone()).await {
            channel_cache.remove(&channel.info.id);
            ping_locks.remove(&channel.info.id);
        }
    }
}
