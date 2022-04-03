use crate::Result;

const SLED_TX_TREE: &[u8] = b"_transactions";

pub struct TxStore(sled::Tree);

impl TxStore {
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_TX_TREE)?;
        Ok(Self(tree))
    }
}
