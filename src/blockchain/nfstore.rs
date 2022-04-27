use sled::Batch;

use crate::{
    crypto::nullifier::Nullifier,
    util::serial::{deserialize, serialize},
    Result,
};

const SLED_NULLIFIER_TREE: &[u8] = b"_nullifiers";

#[derive(Clone)]
pub struct NullifierStore(sled::Tree);

impl NullifierStore {
    /// Opens a new or existing `NullifierStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_NULLIFIER_TREE)?;
        Ok(Self(tree))
    }

    /// Insert a slice of [`Nullifier`] into the nullifier store.
    pub fn insert(&self, nullifiers: &[Nullifier]) -> Result<()> {
        let mut batch = Batch::default();
        for i in nullifiers {
            batch.insert(serialize(i), vec![] as Vec<u8>);
        }

        self.0.apply_batch(batch)?;
        Ok(())
    }

    /// Check whether given nullifier is in the database
    pub fn contains(&self, nullifier: &Nullifier) -> Result<bool> {
        Ok(self.0.contains_key(serialize(nullifier))?)
    }

    /// Retrieve all nullifiers.
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<Option<Nullifier>>> {
        let mut nfs = vec![];
        let iterator = self.0.into_iter().enumerate();
        for (_, r) in iterator {
            let (k, _) = r.unwrap();
            let nullifier = deserialize(&k)?;
            nfs.push(Some(nullifier))
        }

        Ok(nfs)
    }
}
