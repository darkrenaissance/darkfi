use crate::{
    crypto::nullifier::Nullifier,
    util::serial::{deserialize, serialize},
    Result,
};

const SLED_NULLIFIER_TREE: &[u8] = b"_nullifiers";

/// The `NullifierStore` is a `sled` tree storing all the nullifiers seen
/// in existing blocks. The key is the nullifier itself, while the value
/// is an empty vector that's not used. As a sidenote, perhaps we could
/// hold the transaction hash where the nullifier was seen in the value.
#[derive(Clone)]
pub struct NullifierStore(sled::Tree);

impl NullifierStore {
    /// Opens a new or existing `NullifierStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_NULLIFIER_TREE)?;
        Ok(Self(tree))
    }

    /// Insert a slice of [`Nullifier`] into the store. With sled, the
    /// operation is done as a batch. The nullifier is used as a key,
    /// while the value is an empty vector.
    pub fn insert(&self, nfs: &[Nullifier]) -> Result<()> {
        let mut batch = sled::Batch::default();

        for nf in nfs {
            batch.insert(serialize(nf), vec![] as Vec<u8>);
        }

        self.0.apply_batch(batch)?;
        Ok(())
    }

    /// Check if the nullifierstore contains a given nullifier.
    pub fn contains(&self, nullifier: &Nullifier) -> Result<bool> {
        Ok(self.0.contains_key(serialize(nullifier))?)
    }

    /// Retrieve all nullifiers from the store.
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<Nullifier>> {
        let mut nfs = vec![];

        let iterator = self.0.into_iter().enumerate();
        for (_, r) in iterator {
            let (k, _) = r.unwrap();
            let nullifier = deserialize(&k)?;
            nfs.push(nullifier);
        }

        Ok(nfs)
    }
}
