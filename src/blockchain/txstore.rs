use crate::{
    tx::Transaction,
    util::serial::{deserialize, serialize},
    Error, Result,
};

const SLED_TX_TREE: &[u8] = b"_transactions";

/// The `TxStore` is a `sled` tree storing all the blockchain's
/// transactions where the key is the transaction hash, and the value is
/// the serialized transaction.
#[derive(Clone)]
pub struct TxStore(sled::Tree);

impl TxStore {
    /// Opens a new or existing `TxStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_TX_TREE)?;
        Ok(Self(tree))
    }

    /// Insert a slice of [`Transaction`] into the txstore. With sled, the
    /// operation is done as a batch.
    /// The transactions are hashed with BLAKE3 and this hash is used as
    /// the key, while the value is the serialized [`Transaction`] itself.
    /// On success, the function returns the transaction hashes in the same
    /// order as the input transactions.
    pub fn insert(&self, transactions: &[Transaction]) -> Result<Vec<blake3::Hash>> {
        let mut ret = Vec::with_capacity(transactions.len());
        let mut batch = sled::Batch::default();

        for tx in transactions {
            let serialized = serialize(tx);
            let txhash = blake3::hash(&serialized);
            batch.insert(txhash.as_bytes(), serialized);
            ret.push(txhash);
        }

        self.0.apply_batch(batch)?;
        Ok(ret)
    }

    /// Check if the txstore contains a given transaction hash.
    pub fn contains(&self, txid: &blake3::Hash) -> Result<bool> {
        Ok(self.0.contains_key(txid.as_bytes())?)
    }

    /// Fetch given tx hashes from the txstore.
    /// The resulting vector contains `Option`, which is `Some` if the tx
    /// was found in the txstore, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one block was not found.
    pub fn get(&self, txids: &[blake3::Hash], strict: bool) -> Result<Vec<Option<Transaction>>> {
        let mut ret = Vec::with_capacity(txids.len());

        for txid in txids {
            if let Some(found) = self.0.get(txid.as_bytes())? {
                let tx = deserialize(&found)?;
                ret.push(Some(tx));
            } else {
                if strict {
                    let s = txid.to_hex().as_str().to_string();
                    return Err(Error::TransactionNotFound(s))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Retrieve all transactions from the txstore in the form of a tuple
    /// (`tx_hash`, `tx`).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<(blake3::Hash, Transaction)>> {
        let mut txs = vec![];

        let iterator = self.0.into_iter().enumerate();
        for (_, r) in iterator {
            let (k, v) = r.unwrap();
            let hash_bytes: [u8; 32] = k.as_ref().try_into().unwrap();
            let tx = deserialize(&v)?;
            txs.push((hash_bytes.into(), tx));
        }

        Ok(txs)
    }
}
