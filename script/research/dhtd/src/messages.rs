use rand::Rng;

use darkfi::{
    net,
    util::serial::{serialize, SerialDecodable, SerialEncodable},
};

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
