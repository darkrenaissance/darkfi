use crate::Result;

const SLED_ROOTS_TREE: &[u8] = b"_merkleroots";

pub struct RootStore(sled::Tree);

impl RootStore {
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_ROOTS_TREE)?;
        Ok(Self(tree))
    }
}
