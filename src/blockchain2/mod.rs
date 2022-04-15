use std::io;

use crate::{
    consensus2::{block::BlockProposal, util::Timestamp},
    impl_vec,
    util::serial::{Decodable, Encodable, ReadExt, VarInt, WriteExt},
    Result,
};

pub mod blockstore;
use blockstore::BlockStore;

pub mod metadatastore;
use metadatastore::StreamletMetadataStore;

pub mod nfstore;
use nfstore::NullifierStore;

pub mod rootstore;
use rootstore::RootStore;

pub mod txstore;
use txstore::TxStore;

pub struct Blockchain {
    /// Blocks sled tree
    pub blocks: BlockStore,
    /// Transactions sled tree
    pub transactions: TxStore,
    /// Streamlet metadata sled tree
    pub streamlet_metadata: StreamletMetadataStore,
    // TODO:
    //pub nullifiers: NullifierStore,
    //pub merkle_roots: RootStore,
}

impl Blockchain {
    pub fn new(db: &sled::Db, genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Result<Self> {
        let blocks = BlockStore::new(db, genesis_ts, genesis_data)?;
        let transactions = TxStore::new(db)?;
        let streamlet_metadata = StreamletMetadataStore::new(db)?;

        Ok(Self { blocks, transactions, streamlet_metadata })
    }

    /// Batch insert [`BlockProposal`]s.
    pub fn add(&mut self, proposals: &[BlockProposal]) -> Result<Vec<blake3::Hash>> {
        todo!()
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
