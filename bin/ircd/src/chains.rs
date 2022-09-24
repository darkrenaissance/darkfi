use async_std::sync::Mutex;
use std::collections::VecDeque;

use chrono::Utc;
use fxhash::FxHashMap;
use ripemd::{Digest, Ripemd256};

use darkfi::serial::{SerialDecodable, SerialEncodable};

const MAX_CHAIN_SIZE: usize = 4096;

pub type PrivmsgId = String;

#[derive(Debug, Clone, SerialEncodable, SerialDecodable, Eq, PartialEq)]
pub struct Privmsg {
    pub id: PrivmsgId,
    pub nickname: String,
    pub target: String,
    pub message: String,
    pub timestamp: i64,
    pub read_confirms: u8,
    pub prev_msg_id: String,
}

impl Privmsg {
    pub fn new(nickname: &str, target: &str, message: &str, prev_msg_id: &str) -> Self {
        let timestamp = Utc::now().timestamp();
        let id = Self::hash(nickname, target, message, prev_msg_id, timestamp);
        let read_confirms = 0;

        Self {
            id,
            nickname: nickname.to_string(),
            target: target.to_string(),
            message: message.to_string(),
            timestamp,
            read_confirms,
            prev_msg_id: prev_msg_id.to_string(),
        }
    }

    pub fn hash(
        nickname: &str,
        target: &str,
        message: &str,
        prev_msg_id: &str,
        timestamp: i64,
    ) -> String {
        let mut hasher = Ripemd256::new();
        hasher.update(format!("{nickname}{target}{message}{timestamp}{prev_msg_id}"));
        hex::encode(hasher.finalize())
    }
}

impl std::string::ToString for Privmsg {
    fn to_string(&self) -> String {
        format!(":{}!anon@dark.fi PRIVMSG {} :{}\r\n", self.nickname, self.target, self.message)
    }
}

pub struct Chain {
    buffer: VecDeque<Privmsg>,
    hashes: Vec<String>,
}

impl Chain {
    pub fn new() -> Self {
        Self { buffer: VecDeque::new(), hashes: Vec::new() }
    }

    pub fn push_hashes(&mut self, hashes: Vec<String>) {
        self.hashes.extend(hashes);
    }

    pub fn push_msg(&mut self, msg: &Privmsg) -> bool {
        // Rehash the msg to check if it's valid
        let hash = Privmsg::hash(
            &msg.nickname,
            &msg.target,
            &msg.message,
            &msg.prev_msg_id,
            msg.timestamp,
        );

        if hash != msg.id {
            return false
        }

        // Prune last messages from the buffer if it has exceeded the MAX_CHAIN_SIZE
        if self.buffer.len() >= MAX_CHAIN_SIZE {
            self.buffer.pop_front();
        }

        // Check if the hashes already has the msg id, if so add the msg to the buffer
        if self.hashes.contains(&msg.id) {
            // TODO: it should do sorting by the msg id in this step
            self.buffer.push_back(msg.clone());
            return true
        }

        // Check if the last msg in the chains is equal to the previous_msg_id in privmsg,
        // if not and both the chain and previous_msg_id are empty,
        // then it will be add it as genesis msg
        if let Some(last_hash) = self.last_hash() {
            if last_hash != msg.prev_msg_id {
                return false
            }
        } else if !msg.prev_msg_id.is_empty() && !self.hashes.is_empty() {
            return false
        }

        // Push the msg to the chain
        self.buffer.push_back(msg.clone());
        self.hashes.push(msg.id.clone());
        true
    }

    pub fn last_msg(&self) -> Option<Privmsg> {
        self.buffer.iter().last().cloned()
    }

    pub fn last_hash(&self) -> Option<String> {
        self.hashes.iter().last().cloned()
    }

    pub fn height(&self) -> usize {
        self.hashes.len()
    }

    pub fn get_msgs(&self, hashes: &[String]) -> Vec<Privmsg> {
        self.buffer.iter().filter(|m| hashes.contains(&m.id)).cloned().collect()
    }

    pub fn get_hashes(&self, height: usize) -> Vec<String> {
        if height >= self.height() {
            return vec![]
        }

        self.hashes[height..].to_vec()
    }
}

pub struct Chains {
    chains: Mutex<FxHashMap<String, Chain>>,
}

impl Chains {
    pub fn new(targets: Vec<String>) -> Self {
        let mut map = FxHashMap::default();
        for target in targets {
            map.insert(target, Chain::new());
        }
        Self { chains: Mutex::new(map) }
    }

    pub async fn push_hashes(&self, target: String, height: usize, hashes: Vec<String>) -> bool {
        let mut chains = self.chains.lock().await;

        if !chains.contains_key(&target) {
            return false
        }

        let chain = chains.get_mut(&target).unwrap();

        if chain.height() + 1 != height {
            return false
        }

        chain.push_hashes(hashes);
        true
    }

    pub async fn push_msg(&self, msg: &Privmsg) -> bool {
        let mut chains = self.chains.lock().await;

        if !chains.contains_key(&msg.target) {
            return false
        }

        chains.get_mut(&msg.target).unwrap().push_msg(msg);
        true
    }

    pub async fn get_msgs(&self, target: &str, hashes: &[String]) -> Vec<Privmsg> {
        let chains = self.chains.lock().await;

        if !chains.contains_key(target) {
            return vec![]
        }

        chains.get(target).unwrap().get_msgs(hashes)
    }

    pub async fn get_hashes(&self, target: &str, height: usize) -> Vec<String> {
        let chains = self.chains.lock().await;

        if !chains.contains_key(target) {
            return vec![]
        }

        chains.get(target).unwrap().get_hashes(height)
    }

    pub async fn get_height(&self, target: &str) -> usize {
        let chains = self.chains.lock().await;

        if !chains.contains_key(target) {
            return 0
        }

        chains.get(target).unwrap().height()
    }
}
