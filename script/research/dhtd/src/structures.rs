use async_std::sync::{Arc, RwLock};
use fxhash::FxHashMap;
use rand::Rng;

use darkfi::{
    net,
    util::serial::{serialize, SerialDecodable, SerialEncodable},
    Result,
};

/// Atomic pointer to DHT daemon state
pub type StatePtr = Arc<RwLock<State>>;

// TODO: add lookup table
// Using string in structures because we are at an external crate
// and cant use blake3 serialization. To be replaced once merged with core src.

/// Struct representing DHT daemon state.
pub struct State {
    /// Daemon id
    pub id: blake3::Hash,
    /// Daemon hasmap
    pub map: FxHashMap<String, Vec<u8>>,
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
        let seen = FxHashMap::default();

        let state = Arc::new(RwLock::new(State { id, map, seen }));

        Ok(state)
    }
}

/// This struct represents a DHT key request
#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
pub struct KeyRequest {
    /// Request id    
    pub id: String,
    /// Daemon id requesting the key
    pub daemon: String,
    /// Key entry
    pub key: String,
}

impl KeyRequest {
    pub fn new(daemon: String, key: String) -> Self {
        // Generate a random id
        let mut rng = rand::thread_rng();
        let n: u16 = rng.gen();
        let id = blake3::hash(&serialize(&n)).to_string();
        Self { id, daemon, key }
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
    /// Daemon id requested the key
    pub daemon: String,
    /// Key entry
    pub key: String,
    /// Key value
    pub value: Vec<u8>,
}

impl KeyResponse {
    pub fn new(daemon: String, key: String, value: Vec<u8>) -> Self {
        // Generate a random id
        let mut rng = rand::thread_rng();
        let n: u16 = rng.gen();
        let id = blake3::hash(&serialize(&n)).to_string();
        Self { id, daemon, key, value }
    }
}

impl net::Message for KeyResponse {
    fn name() -> &'static str {
        "keyresponse"
    }
}
