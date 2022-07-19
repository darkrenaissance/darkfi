use async_executor::Executor;
use async_std::sync::{Arc, RwLock};
use chrono::Utc;
use futures::{select, FutureExt};
use fxhash::FxHashMap;
use log::{debug, error};
use rand::Rng;
use std::{collections::HashSet, time::Duration};

use darkfi::{
    net,
    net::P2pPtr,
    util::{serial::serialize, sleep},
    Error::TorError,
    Result,
};

use crate::{
    messages::{KeyRequest, KeyResponse, LookupRequest},
    protocol::Protocol,
};

// Constants configuration
const REQUEST_TIMEOUT: u64 = 2400;
const SEEN_DURATION: i64 = 120;

/// Atomic pointer to DHT state
pub type DhtPtr = Arc<RwLock<Dht>>;

// TODO: proper errors
// TODO: lookup table to be based on directly connected peers, not broadcast based
// TODO: replace Strings with blake3 hashes
// Using string in structures because we are at an external crate
// and cant use blake3 serialization. To be replaced once merged with core src.

/// Struct representing DHT state.
pub struct Dht {
    /// Daemon id
    pub id: blake3::Hash,
    /// Daemon hasmap
    pub map: FxHashMap<String, Vec<u8>>,
    /// Network lookup map, containing nodes that holds each key
    pub lookup: FxHashMap<String, HashSet<String>>,
    /// P2P network pointer
    p2p: P2pPtr,
    /// Channel to receive responses from P2P
    p2p_recv_channel: async_channel::Receiver<KeyResponse>,
    /// Stop signal channel to terminate background processes
    stop_signal: async_channel::Receiver<()>,
    /// Daemon seen requests/responses ids and timestamp,
    /// to prevent rebroadcasting and loops
    pub seen: FxHashMap<String, i64>,
}

impl Dht {
    pub async fn new(
        initial: Option<FxHashMap<String, HashSet<String>>>,
        p2p_ptr: P2pPtr,
        stop_signal: async_channel::Receiver<()>,
        ex: Arc<Executor<'_>>,
    ) -> Result<DhtPtr> {
        // Generate a random id
        let mut rng = rand::thread_rng();
        let n: u16 = rng.gen();
        let id = blake3::hash(&serialize(&n));
        let map = FxHashMap::default();
        let lookup = match initial {
            Some(l) => l,
            None => FxHashMap::default(),
        };
        let p2p = p2p_ptr.clone();
        let (p2p_send_channel, p2p_recv_channel) = async_channel::unbounded::<KeyResponse>();
        let seen = FxHashMap::default();

        let dht = Arc::new(RwLock::new(Dht {
            id,
            map,
            lookup,
            p2p,
            p2p_recv_channel,
            stop_signal,
            seen,
        }));

        // Registering P2P protocols
        let registry = p2p_ptr.protocol_registry();
        let _dht = dht.clone();
        registry
            .register(net::SESSION_ALL, move |channel, p2p_ptr| {
                let sender = p2p_send_channel.clone();
                let dht = _dht.clone();
                async move { Protocol::init(channel, sender, dht, p2p_ptr).await.unwrap() }
            })
            .await;

        // Task to periodically clean up daemon seen messages
        ex.spawn(prune_seen_messages(dht.clone())).detach();

        Ok(dht)
    }

    /// Store provided key value pair and update lookup map
    pub async fn insert(&mut self, key: String, value: Vec<u8>) -> Result<Option<String>> {
        self.map.insert(key.clone(), value);
        self.lookup_insert(key, self.id.to_string()).await
    }

    /// Remove provided key value pair and update lookup map
    pub async fn remove(&mut self, key: String) -> Result<Option<String>> {
        // Check if key value pair existed and act accordingly
        match self.map.remove(&key) {
            Some(_) => {
                debug!("Key removed: {}", key);
                let daemon = self.id.to_string();
                let request = LookupRequest::new(daemon, key.clone(), 1);
                if let Err(e) = self.p2p.broadcast(request).await {
                    error!("Failed broadcasting request: {}", e);
                    return Err(e)
                }

                self.lookup_remove(key.clone(), self.id.to_string())
            }
            None => Ok(None),
        }
    }

    /// Store provided key node pair in lookup map and update network
    pub async fn lookup_insert(&mut self, key: String, node_id: String) -> Result<Option<String>> {
        let mut lookup_set = match self.lookup.get(&key) {
            Some(s) => s.clone(),
            None => HashSet::new(),
        };

        lookup_set.insert(node_id);
        self.lookup.insert(key.clone(), lookup_set);

        let daemon = self.id.to_string();
        let request = LookupRequest::new(daemon, key.clone(), 0);
        if let Err(e) = self.p2p.broadcast(request).await {
            error!("Failed broadcasting request: {}", e);
            return Err(e)
        }

        Ok(Some(key))
    }

    /// Remove provided node id from keys set in local lookup map
    pub fn lookup_remove(&mut self, key: String, node_id: String) -> Result<Option<String>> {
        if let Some(s) = self.lookup.get(&key) {
            let mut lookup_set = s.clone();
            lookup_set.remove(&node_id);
            if lookup_set.is_empty() {
                self.lookup.remove(&key);
            } else {
                self.lookup.insert(key.clone(), lookup_set);
            }
        }

        Ok(Some(key))
    }

    /// Verify if provided key exists and return flag if local or in network
    pub fn contains_key(&self, key: String) -> Option<bool> {
        match self.lookup.contains_key(&key) {
            true => Some(self.map.contains_key(&key)),
            false => None,
        }
    }

    /// Get key from local map, acting as daemon cache
    pub fn get(&self, key: String) -> Option<&Vec<u8>> {
        self.map.get(&key)
    }

    /// Generate key request and broadcast it to the network
    pub async fn request_key(&self, key: String) -> Result<()> {
        // Verify the key exist in the lookup map.
        let peers = match self.lookup.get(&key) {
            Some(v) => v.clone(),
            None => {
                error!("Key doesn't exist.");
                return Err(TorError("Key doesn't exist.".to_string()))
            }
        };

        debug!("Key is in peers: {:?}", peers);

        // We retrieve p2p network connected channels, to verify if we
        // are connected to a network.
        // Using len here because is_empty() uses unstable library feature
        // called 'exact_size_is_empty'.
        if self.p2p.channels().lock().await.values().len() == 0 {
            error!("Node is not connected to other nodes.");
            return Err(TorError("Node is not connected to other nodes.".to_string()))
        }

        // We create a key request, and broadcast it to the network
        let daemon = self.id.to_string();
        // We choose last known peer as request recipient
        let peer = peers.iter().last().unwrap().to_string();
        let request = KeyRequest::new(daemon.clone(), peer, key.clone());
        // TODO: ask connected peers directly, not broadcast
        if let Err(e) = self.p2p.broadcast(request).await {
            error!("Failed broadcasting request: {}", e);
            return Err(e)
        }

        Ok(())
    }
}

// Auxilary function to wait for a key response from the P2P network.
pub async fn waiting_for_response(dht: DhtPtr) -> Result<Option<KeyResponse>> {
    let (p2p_recv_channel, stop_signal) = {
        let _dht = dht.read().await;
        (_dht.p2p_recv_channel.clone(), _dht.stop_signal.clone())
    };
    let ex = Arc::new(async_executor::Executor::new());
    let (timeout_s, timeout_r) = async_channel::unbounded::<()>();
    ex.spawn(async move {
        sleep(Duration::from_millis(REQUEST_TIMEOUT).as_secs()).await;
        timeout_s.send(()).await.unwrap_or(());
    })
    .detach();

    loop {
        select! {
            msg = p2p_recv_channel.recv().fuse() => {
                let response = msg?;
                return Ok(Some(response))
            },
            _ = stop_signal.recv().fuse() => break,
            _ = timeout_r.recv().fuse() => break,
        }
    }
    Ok(None)
}

// Auxilary function to periodically prun seen messages, based on when they were received.
// This helps us to prevent broadcasting loops.
async fn prune_seen_messages(dht: DhtPtr) {
    loop {
        sleep(SEEN_DURATION as u64).await;
        debug!("Pruning seen messages");

        let now = Utc::now().timestamp();

        let mut prune = vec![];
        let map = dht.read().await.seen.clone();
        for (k, v) in map.iter() {
            if now - v > SEEN_DURATION {
                prune.push(k);
            }
        }

        let mut map = map.clone();
        for i in prune {
            map.remove(i);
        }

        dht.write().await.seen = map;
    }
}
