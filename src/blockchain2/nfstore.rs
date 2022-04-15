use sled::Batch;

use crate::{crypto::nullifier::Nullifier, util::serial::serialize, Result};

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
}
