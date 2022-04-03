use std::io;

use crate::{
    impl_vec,
    util::serial::{Decodable, Encodable, ReadExt, VarInt, WriteExt},
    Result,
};

pub mod blockstore;
pub mod nfstore;
pub mod rootstore;
pub mod txstore;

use blockstore::BlockStore;
use nfstore::NullifierStore;
use rootstore::RootStore;
use txstore::TxStore;

pub struct Blockchain {
    pub db: sled::Db,
    pub blocks: BlockStore,
    pub transactions: TxStore,
    pub nullifiers: NullifierStore,
    pub merkle_roots: RootStore,
}

impl Blockchain {
    pub fn new(db_path: &str) -> Result<Self> {
        let db = sled::open(db_path)?;
        let blocks = BlockStore::new(&db)?;
        let transactions = TxStore::new(&db)?;
        let nullifiers = NullifierStore::new(&db)?;
        let merkle_roots = RootStore::new(&db)?;

        Ok(Self { db, blocks, transactions, nullifiers, merkle_roots })
    }
}

impl Encodable for blake3::Hash {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        s.write_slice(self.as_bytes())?;
        Ok(32)
    }
}

impl Decodable for blake3::Hash {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        Ok(bytes.into())
    }
}

impl_vec!(blake3::Hash);
