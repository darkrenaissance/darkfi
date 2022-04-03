use crate::Result;

const SLED_BLOCK_TREE: &[u8] = b"_blocks";

pub struct BlockStore(sled::Tree);

impl BlockStore {
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_BLOCK_TREE)?;
        Ok(Self(tree))
    }
}
