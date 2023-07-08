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

use std::collections::{HashMap, HashSet};

use async_std::sync::{Arc, RwLock};
use chrono::Utc;
use darkfi_serial::serialize;
use futures::{select, FutureExt};
use log::{debug, error, warn};
use rand::{rngs::OsRng, Rng};
use smol::Executor;

use crate::{
    net,
    net::P2pPtr,
    util::async_util::sleep,
    Error::{NetworkNotConnected, UnknownKey},
    Result,
};

mod messages;
use messages::{KeyRequest, KeyResponse, LookupMapRequest, LookupMapResponse, LookupRequest};
mod protocol;
use protocol::Protocol;

// Constants configuration
const SEEN_DURATION: i64 = 120;

/// Atomic pointer to DHT state
pub type DhtPtr = Arc<RwLock<Dht>>;

// TODO: proper errors
// TODO: lookup table to be based on directly connected peers, not broadcast based
// Using string in structures because we are at an external crate
// and cant use blake3 serialization. To be replaced once merged with core src.

/// Struct representing DHT state.
pub struct Dht {
    /// Daemon id
    pub id: blake3::Hash,
    /// Daemon hasmap
    pub map: HashMap<blake3::Hash, Vec<u8>>,
    /// Network lookup map, containing nodes that holds each key
    pub lookup: HashMap<blake3::Hash, HashSet<blake3::Hash>>,
    /// P2P network pointer
    pub p2p: P2pPtr,
    /// Channel to receive responses from P2P
    p2p_recv_channel: smol::channel::Receiver<KeyResponse>,
    /// Stop signal channel to terminate background processes
    stop_signal: smol::channel::Receiver<()>,
    /// Daemon seen requests/responses ids and timestamp,
    /// to prevent rebroadcasting and loops
    pub seen: HashMap<blake3::Hash, i64>,
}

impl Dht {
    pub async fn new(
        initial: Option<HashMap<blake3::Hash, HashSet<blake3::Hash>>>,
        p2p_ptr: P2pPtr,
        stop_signal: smol::channel::Receiver<()>,
        ex: Arc<Executor<'_>>,
    ) -> Result<DhtPtr> {
        // Generate a random id
        let n: u16 = OsRng.gen();
        let id = blake3::hash(&serialize(&n));
        let map = HashMap::default();
        let lookup = match initial {
            Some(l) => l,
            None => HashMap::default(),
        };
        let p2p = p2p_ptr.clone();
        let (p2p_send_channel, p2p_recv_channel) = smol::channel::unbounded::<KeyResponse>();
        let seen = HashMap::default();

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

    /// Store provided key value pair, update lookup map and broadcast new insert to network
    pub async fn insert(
        &mut self,
        key: blake3::Hash,
        value: Vec<u8>,
    ) -> Result<Option<blake3::Hash>> {
        self.map.insert(key, value);

        if let Err(e) = self.lookup_insert(key, self.id) {
            error!(target: "dht", "Failed to insert record to lookup map: {}", e);
            return Err(e)
        };

        let request = LookupRequest::new(self.id, key, 0);
        self.p2p.broadcast(&request).await;

        Ok(Some(key))
    }

    /// Remove provided key value pair and update lookup map
    pub async fn remove(&mut self, key: blake3::Hash) -> Result<Option<blake3::Hash>> {
        // Check if key value pair existed and act accordingly
        match self.map.remove(&key) {
            Some(_) => {
                debug!(target: "dht", "Key removed: {}", key);
                let request = LookupRequest::new(self.id, key, 1);
                self.p2p.broadcast(&request).await;

                self.lookup_remove(key, self.id)
            }
            None => Ok(None),
        }
    }

    /// Store provided key node pair in lookup map and update network
    pub fn lookup_insert(
        &mut self,
        key: blake3::Hash,
        node_id: blake3::Hash,
    ) -> Result<Option<blake3::Hash>> {
        let mut lookup_set = match self.lookup.get(&key) {
            Some(s) => s.clone(),
            None => HashSet::new(),
        };

        lookup_set.insert(node_id);
        self.lookup.insert(key, lookup_set);

        Ok(Some(key))
    }

    /// Remove provided node id from keys set in local lookup map
    pub fn lookup_remove(
        &mut self,
        key: blake3::Hash,
        node_id: blake3::Hash,
    ) -> Result<Option<blake3::Hash>> {
        if let Some(s) = self.lookup.get(&key) {
            let mut lookup_set = s.clone();
            lookup_set.remove(&node_id);
            if lookup_set.is_empty() {
                self.lookup.remove(&key);
            } else {
                self.lookup.insert(key, lookup_set);
            }
        }

        Ok(Some(key))
    }

    /// Verify if provided key exists and return flag if local or in network
    pub fn contains_key(&self, key: blake3::Hash) -> Option<bool> {
        match self.lookup.contains_key(&key) {
            true => Some(self.map.contains_key(&key)),
            false => None,
        }
    }

    /// Get key from local map, acting as daemon cache
    pub fn get(&self, key: blake3::Hash) -> Option<&Vec<u8>> {
        self.map.get(&key)
    }

    /// Generate key request and broadcast it to the network
    pub async fn request_key(&self, key: blake3::Hash) -> Result<()> {
        // Verify the key exist in the lookup map.
        let peers = match self.lookup.get(&key) {
            Some(v) => v.clone(),
            None => return Err(UnknownKey),
        };

        debug!(target: "dht", "Key is in peers: {:?}", peers);

        // We retrieve p2p network connected channels, to verify if we
        // are connected to a network.
        // Using len here because is_empty() uses unstable library feature
        // called 'exact_size_is_empty'.
        if self.p2p.channels().lock().await.values().len() == 0 {
            return Err(NetworkNotConnected)
        }

        // We create a key request, and broadcast it to the network
        // We choose last known peer as request recipient
        let peer = *peers.iter().last().unwrap();
        let request = KeyRequest::new(self.id, peer, key);
        // TODO: ask connected peers directly, not broadcast
        self.p2p.broadcast(&request).await;

        Ok(())
    }

    /// Auxilary function to sync lookup map with network
    pub async fn sync_lookup_map(&mut self) -> Result<()> {
        debug!(target: "dht", "Starting lookup map sync...");
        let channels_map = self.p2p.channels().lock().await.clone();
        let values = channels_map.values();
        // Using len here because is_empty() uses unstable library feature
        // called 'exact_size_is_empty'.
        if values.len() != 0 {
            // Node iterates the channel peers to ask for their lookup map
            for channel in values {
                // Communication setup
                let msg_subsystem = channel.message_subsystem();
                msg_subsystem.add_dispatch::<LookupMapResponse>().await;
                let response_sub = channel.subscribe_msg::<LookupMapResponse>().await?;

                // Node creates a `LookupMapRequest` and sends it
                let order = LookupMapRequest::new(self.id);
                channel.send(&order).await?;

                // Node stores response data.
                let resp = response_sub.receive().await?;
                if resp.lookup.is_empty() {
                    warn!(target: "dht", "Retrieved empty lookup map from an unsynced node, retrying...");
                    continue
                }

                // Store retrieved records
                debug!(target: "dht", "Processing received records");
                for (k, v) in &resp.lookup {
                    for node in v {
                        self.lookup_insert(*k, *node)?;
                    }
                }

                break
            }
        } else {
            warn!(target: "dht", "Node is not connected to other nodes");
        }

        debug!(target: "dht", "Lookup map synced!");
        Ok(())
    }
}

// Auxilary function to wait for a key response from the P2P network.
pub async fn waiting_for_response(dht: DhtPtr) -> Result<Option<KeyResponse>> {
    let (p2p_recv_channel, stop_signal, timeout) = {
        let _dht = dht.read().await;
        (_dht.p2p_recv_channel.clone(), _dht.stop_signal.clone(), 666)
    };
    let ex = Arc::new(Executor::new());
    let (timeout_s, timeout_r) = smol::channel::unbounded::<()>();
    ex.spawn(async move {
        sleep(timeout).await;
        timeout_s.send(()).await.unwrap_or(());
    })
    .detach();

    select! {
        msg = p2p_recv_channel.recv().fuse() => {
                let response = msg?;
                return Ok(Some(response))
        },
        _ = stop_signal.recv().fuse() => {},
        _ = timeout_r.recv().fuse() => {},
    }
    Ok(None)
}

// Auxilary function to periodically prun seen messages, based on when they were received.
// This helps us to prevent broadcasting loops.
async fn prune_seen_messages(dht: DhtPtr) {
    loop {
        sleep(SEEN_DURATION as u64).await;
        debug!(target: "dht", "Pruning seen messages");

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
