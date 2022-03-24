use async_std::sync::Mutex;
use std::{io, sync::Arc};

use fxhash::FxHashSet;

use darkfi::{
    net,
    util::serial::{Decodable, Encodable},
    Result,
};

pub type TxHash = u32; // Change this to a proper hash type

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
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
pub struct TxPool {
    tx_pool: Mutex<FxHashSet<Tx>>,
}

pub type TxPoolPtr = Arc<TxPool>;

impl TxPool {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { tx_pool: Mutex::new(FxHashSet::default()) })
    }

    pub async fn add_tx(&self, tx: Tx) {
        self.tx_pool.lock().await.insert(tx);
    }

    pub async fn tx_exists(&self, tx: &Tx) -> bool {
        self.tx_pool.lock().await.contains(tx)
    }
}
