use crate::Result;

const SLED_NULLIFIER_TREE: &[u8] = b"_nullifiers";

pub struct NullifierStore(sled::Tree);

impl NullifierStore {
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_NULLIFIER_TREE)?;
        Ok(Self(tree))
    }
}
