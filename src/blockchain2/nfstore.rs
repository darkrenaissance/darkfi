use sled::Batch;

use crate::{
    crypto::nullifier::Nullifier,
    util::serial::{deserialize, serialize},
    Result,
};

const SLED_NULLIFIER_TREE: &[u8] = b"_nullifiers";

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

    /// Retrieve all nullifiers.
    /// Be careful as this will try to load everything im memory.
    pub fn get_all(&self) -> Result<Vec<Option<Nullifier>>> {
        let mut nfs = vec![];
        let mut iterator = self.0.into_iter().enumerate();
        while let Some((_, r)) = iterator.next() {
            let (k, _) = r.unwrap();
            let nullifier = deserialize(&k)?;
            nfs.push(Some(nullifier))
        }

        Ok(nfs)
    }
}
