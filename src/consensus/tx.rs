use std::io;

use crate::{
    impl_vec, net,
    util::serial::{
        deserialize, serialize, Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt,
    },
    Result,
};

const SLED_TX_TREE: &[u8] = b"_transactions";

/// Temporary structure used to represent transactions.
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Tx {
    pub payload: String,
}

impl net::Message for Tx {
    fn name() -> &'static str {
        "tx"
    }
}

impl_vec!(Tx);

#[derive(Debug)]
pub struct TxStore(sled::Tree);

impl TxStore {
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_TX_TREE)?;
        Ok(Self(tree))
    }

    /// Insert a tx into the txstore.
    /// The tx is hashed with blake3 and this txhash is used as
    /// the key, where value is the serialized tx itself.
    pub fn insert(&self, tx: &Tx) -> Result<blake3::Hash> {
        let serialized = serialize(tx);
        let txhash = blake3::hash(&serialized);
        self.0.insert(txhash.as_bytes(), serialized)?;

        Ok(txhash)
    }

    /// Retrieve all transactions.
    /// Be carefull as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<Option<(blake3::Hash, Tx)>>> {
        let mut txs = Vec::new();
        let mut iterator = self.0.into_iter().enumerate();
        while let Some((_, r)) = iterator.next() {
            let (k, v) = r.unwrap();
            let hash_bytes: [u8; 32] = k.as_ref().try_into().unwrap();
            let tx = deserialize(&v)?;
            txs.push(Some((hash_bytes.into(), tx)));
        }
        Ok(txs)
    }
}
