use sled::Batch;

use crate::{
    consensus::Tx,
    util::serial::{deserialize, serialize},
    Error, Result,
};

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

    /// Fetch requested transactions from the txstore. The `strict` param
    /// will make the function fail if a transaction has not been found.
    pub fn get(&self, tx_hashes: &[blake3::Hash], strict: bool) -> Result<Vec<Option<Tx>>> {
        let mut ret: Vec<Option<Tx>> = Vec::with_capacity(tx_hashes.len());

        for i in tx_hashes {
            if let Some(found) = self.0.get(i.as_bytes())? {
                let tx = deserialize(&found)?;
                ret.push(Some(tx));
            } else {
                if strict {
                    let s = i.to_hex().as_str().to_string();
                    return Err(Error::TransactionNotFound(s))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Retrieve all transactions.
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<Option<(blake3::Hash, Tx)>>> {
        let mut txs = vec![];
        let iterator = self.0.into_iter().enumerate();
        for (_, r) in iterator {
            let (k, v) = r.unwrap();
            let hash_bytes: [u8; 32] = k.as_ref().try_into().unwrap();
            let tx = deserialize(&v)?;
            txs.push(Some((hash_bytes.into(), tx)));
        }

        Ok(txs)
    }
}
