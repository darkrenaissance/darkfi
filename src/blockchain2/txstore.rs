use sled::Batch;

use crate::{consensus2::Tx, util::serial::serialize, Result};

const SLED_TX_TREE: &[u8] = b"_transactions";

pub struct TxStore(sled::Tree);

impl TxStore {
    /// Opens a new or existing `TxStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_TX_TREE)?;
        Ok(Self(tree))
    }

    /// Insert a slice of [`Tx`] into the txstore. With sled, the
    /// operation is done as a batch.
    /// The transactions are hashed with BLAKE3 and this hash is
    /// used as the key, while value is the serialized tx itself.
    pub fn insert(&self, txs: &[Tx]) -> Result<Vec<blake3::Hash>> {
        let mut ret = Vec::with_capacity(txs.len());
        let mut batch = Batch::default();
        for i in txs {
            let serialized = serialize(i);
            let txhash = blake3::hash(&serialized);
            batch.insert(txhash.as_bytes(), serialized);
            ret.push(txhash);
        }

        self.0.apply_batch(batch)?;
        Ok(ret)
    }
}
