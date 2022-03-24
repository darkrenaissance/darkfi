use async_std::sync::Mutex;
use std::{io, sync::Arc};

use fxhash::FxHashSet;

use darkfi::{
    net,
    util::serial::{Decodable, Encodable},
    Result,
};

pub type TxHash = u32; // Change this to a proper hash type

#[derive(Debug, Clone)]
pub struct Tx {
    pub hash: TxHash,
    pub payload: String,
}

impl net::Message for Tx {
    fn name() -> &'static str {
        "tx"
    }
}

impl Encodable for Tx {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.hash.encode(&mut s)?;
        len += self.payload.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for Tx {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self { hash: Decodable::decode(&mut d)?, payload: Decodable::decode(&mut d)? })
    }
}

#[derive(Debug)]
pub struct SeenTxHashes {
    seen_tx_hashes: Mutex<FxHashSet<TxHash>>,
}

pub type SeenTxHashesPtr = Arc<SeenTxHashes>;

impl SeenTxHashes {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { seen_tx_hashes: Mutex::new(FxHashSet::default()) })
    }

    pub async fn add_seen(&self, hash: u32) {
        self.seen_tx_hashes.lock().await.insert(hash);
    }

    pub async fn is_seen(&self, hash: u32) -> bool {
        self.seen_tx_hashes.lock().await.contains(&hash)
    }
}
