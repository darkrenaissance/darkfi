use async_std::sync::{Arc, RwLock};
use fxhash::FxHashMap;
use rand::Rng;
use std::collections::HashSet;

use darkfi::{
    net,
    util::serial::{serialize, SerialDecodable, SerialEncodable},
    Result,
};

/// Atomic pointer to DHT daemon state
pub type StatePtr = Arc<RwLock<State>>;

// TODO: lookup table to be based on directly connected peers, not broadcast based
// TODO: replace Strings with blake3 hashes
// Using string in structures because we are at an external crate
// and cant use blake3 serialization. To be replaced once merged with core src.

/// Struct representing DHT daemon state.
pub struct State {
    /// Daemon id
    pub id: blake3::Hash,
    /// Daemon hasmap
    pub map: FxHashMap<String, Vec<u8>>,
    /// Network lookup map, containing nodes that holds each key
    pub lookup: FxHashMap<String, HashSet<String>>,
    /// Daemon seen requests/responses ids and timestamp,
    /// to prevent rebroadcasting and loops
    pub seen: FxHashMap<String, i64>,
}

impl State {
    pub async fn new() -> Result<StatePtr> {
        // Generate a random id
        let mut rng = rand::thread_rng();
        let n: u16 = rng.gen();
        let id = blake3::hash(&serialize(&n));
        let map = FxHashMap::default();
        let lookup = FxHashMap::default();
        let seen = FxHashMap::default();

        let state = Arc::new(RwLock::new(State { id, map, lookup, seen }));

        Ok(state)
    }

    /// Store provided key value pair and update local lookup map
    pub fn insert(&mut self, key: String, value: Vec<u8>) -> Result<()> {
        self.map.insert(key.clone(), value);
        self.lookup_insert(key, self.id.to_string())
    }

    /// Remove provided key value pair and update local lookup map
    pub fn remove(&mut self, key: String) -> Result<Option<String>> {
        // Check if key value pair existed and act accordingly
        let result = match self.map.remove(&key) {
            Some(_) => {
                self.lookup_remove(key.clone(), self.id.to_string())?;
                Some(key)
            }
            None => None,
        };

        Ok(result)
    }

    /// Store provided key node pair in local lookup map
    pub fn lookup_insert(&mut self, key: String, node_id: String) -> Result<()> {
        let mut lookup_set = match self.lookup.get(&key) {
            Some(s) => s.clone(),
            None => HashSet::new(),
        };

        lookup_set.insert(node_id);
        self.lookup.insert(key, lookup_set);

        Ok(())
    }

    /// Remove provided node id from keys set in local lookup map
    pub fn lookup_remove(&mut self, key: String, node_id: String) -> Result<()> {
        if let Some(s) = self.lookup.get(&key) {
            let mut lookup_set = s.clone();
            lookup_set.remove(&node_id);
            if lookup_set.is_empty() {
                self.lookup.remove(&key);
            } else {
                self.lookup.insert(key, lookup_set);
            }
        }

        Ok(())
    }
}

/// This struct represents a DHT key request
#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
pub struct KeyRequest {
    /// Request id    
    pub id: String,
    /// Daemon id requesting the key
    pub from: String,
    /// Daemon id holding the key
    pub to: String,
    /// Key entry
    pub key: String,
}

impl KeyRequest {
    pub fn new(from: String, to: String, key: String) -> Self {
        // Generate a random id
        let mut rng = rand::thread_rng();
        let n: u16 = rng.gen();
        let id = blake3::hash(&serialize(&n)).to_string();
        Self { id, from, to, key }
    }
}

impl net::Message for KeyRequest {
    fn name() -> &'static str {
        "keyrequest"
    }
}

/// This struct represents a DHT key request response
#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
pub struct KeyResponse {
    /// Response id
    pub id: String,
    /// Daemon id holding the key
    pub from: String,
    /// Daemon id holding the key
    pub to: String,
    /// Key entry
    pub key: String,
    /// Key value
    pub value: Vec<u8>,
}

impl KeyResponse {
    pub fn new(from: String, to: String, key: String, value: Vec<u8>) -> Self {
        // Generate a random id
        let mut rng = rand::thread_rng();
        let n: u16 = rng.gen();
        let id = blake3::hash(&serialize(&n)).to_string();
        Self { id, from, to, key, value }
    }
}

impl net::Message for KeyResponse {
    fn name() -> &'static str {
        "keyresponse"
    }
}

/// This struct represents a lookup map request
#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
pub struct LookupRequest {
    /// Request id    
    pub id: String,
    /// Daemon id executing the request
    pub daemon: String,
    /// Key entry
    pub key: String,
    /// Request type
    pub req_type: u8, // 0 for insert, 1 for remove
}

impl LookupRequest {
    pub fn new(daemon: String, key: String, req_type: u8) -> Self {
        // Generate a random id
        let mut rng = rand::thread_rng();
        let n: u16 = rng.gen();
        let id = blake3::hash(&serialize(&n)).to_string();
        Self { id, daemon, key, req_type }
    }
}

impl net::Message for LookupRequest {
    fn name() -> &'static str {
        "lookuprequest"
    }
}
