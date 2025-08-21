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
    hash::{Hash, Hasher},
    marker::{Send, Sync},
    sync::Arc,
};

use async_trait::async_trait;
use num_bigint::BigUint;
use smol::lock::RwLock;
use url::Url;

use darkfi_serial::{SerialDecodable, SerialEncodable};

use crate::{net::P2pPtr, system::ExecutorPtr, util::time::Timestamp};

pub mod settings;
pub use settings::{DhtSettings, DhtSettingsOpt};

pub mod handler;
pub use handler::DhtHandler;

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

pub struct DhtBucket<N: DhtNode> {
    pub nodes: Vec<N>,
}

/// "Router" means: Key -> Set of nodes (+ additional data for each node)
pub type DhtRouterPtr<N> = Arc<RwLock<HashMap<blake3::Hash, HashSet<DhtRouterItem<N>>>>>;

#[derive(Debug, Clone, SerialEncodable, SerialDecodable, Eq)]
pub struct DhtRouterItem<N: DhtNode> {
    pub node: N,
    pub timestamp: u64,
}

impl<N: DhtNode> Hash for DhtRouterItem<N> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node.id().hash(state);
    }
}

impl<N: DhtNode> PartialEq for DhtRouterItem<N> {
    fn eq(&self, other: &Self) -> bool {
        self.node.id() == other.node.id()
    }
}

impl<N: DhtNode> From<N> for DhtRouterItem<N> {
    fn from(node: N) -> Self {
        DhtRouterItem { node, timestamp: Timestamp::current_time().inner() }
    }
}

#[derive(Clone)]
pub struct ChannelCacheItem<N: DhtNode> {
    /// The DHT node the channel is connected to.
    pub node: N,

    /// Topic is a hash that you set to remember what the channel is about,
    /// it's not shared with the peer. If you ask for a channel (with
    /// `handler.get_channel()`) for a specific topic, it will give you a
    /// channel that has no topic, has the same topic, or a new
    /// channel.
    topic: Option<blake3::Hash>,

    /// Usage count increments when you call `handler.get_channel()` and
    /// decrements when you call `handler.cleanup_channel()`. A channel's
    /// topic is cleared on cleanup if its usage count is zero.
    usage_count: u32,
}

pub struct Dht<N: DhtNode> {
    /// Are we bootstrapped?
    pub bootstrapped: Arc<RwLock<bool>>,
    /// Vec of buckets
    pub buckets: Arc<RwLock<Vec<DhtBucket<N>>>>,
    /// Number of buckets
    pub n_buckets: usize,
    /// Channel ID -> ChannelCacheItem
    pub channel_cache: Arc<RwLock<HashMap<u32, ChannelCacheItem<N>>>>,
    /// Node ID -> Set of keys
    pub router_cache: Arc<RwLock<HashMap<blake3::Hash, HashSet<blake3::Hash>>>>,

    pub settings: DhtSettings,

    pub p2p: P2pPtr,
    pub executor: ExecutorPtr,
}

impl<N: DhtNode> Dht<N> {
    pub async fn new(settings: &DhtSettings, p2p: P2pPtr, ex: ExecutorPtr) -> Self {
        // Create empty buckets
        let mut buckets = vec![];
        for _ in 0..256 {
            buckets.push(DhtBucket { nodes: vec![] })
        }

        Self {
            buckets: Arc::new(RwLock::new(buckets)),
            n_buckets: 256,
            bootstrapped: Arc::new(RwLock::new(false)),
            channel_cache: Arc::new(RwLock::new(HashMap::new())),
            router_cache: Arc::new(RwLock::new(HashMap::new())),

            settings: settings.clone(),

            p2p: p2p.clone(),
            executor: ex,
        }
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
    pub fn sort_by_distance(&self, nodes: &mut [N], key: &blake3::Hash) {
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
    pub async fn find_neighbors(&self, key: &blake3::Hash, n: usize) -> Vec<N> {
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

    /// Channel ID -> DhtNode
    pub async fn get_node_from_channel(&self, channel_id: u32) -> Option<N> {
        let channel_cache_lock = self.channel_cache.clone();
        let channel_cache = channel_cache_lock.read().await;
        if let Some(cached) = channel_cache.get(&channel_id).cloned() {
            return Some(cached.node)
        }

        None
    }

    /// Remove nodes in router that are older than expiry_secs
    pub async fn prune_router(&self, router: DhtRouterPtr<N>, expiry_secs: u32) {
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

    /// Reset the DHT state
    pub async fn reset(&self) {
        let mut bootstrapped = self.bootstrapped.write().await;
        *bootstrapped = false;

        let mut buckets = vec![];
        for _ in 0..256 {
            buckets.push(DhtBucket { nodes: vec![] })
        }

        *self.buckets.write().await = buckets;
    }
}
