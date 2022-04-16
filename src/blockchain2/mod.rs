use std::io;

use crate::{
    consensus2::{block::BlockProposal, util::Timestamp, Block},
    impl_vec,
    util::serial::{Decodable, Encodable, ReadExt, VarInt, WriteExt},
    Result,
};

pub mod blockstore;
pub use blockstore::BlockStore;

pub mod metadatastore;
pub use metadatastore::StreamletMetadataStore;

pub mod nfstore;
pub use nfstore::NullifierStore;

pub mod rootstore;
pub use rootstore::RootStore;

pub mod txstore;
pub use txstore::TxStore;

pub struct Blockchain {
    /// Blocks sled tree
    pub blocks: BlockStore,
    /// Transactions sled tree
    pub transactions: TxStore,
    /// Streamlet metadata sled tree
    pub streamlet_metadata: StreamletMetadataStore,
    /// Nullifiers sled tree
    pub nullifiers: NullifierStore,
    /// Merkle roots sled tree
    pub merkle_roots: RootStore,
}

impl Blockchain {
    pub fn new(db: &sled::Db, genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Result<Self> {
        let blocks = BlockStore::new(db, genesis_ts, genesis_data)?;
        let transactions = TxStore::new(db)?;
        let streamlet_metadata = StreamletMetadataStore::new(db)?;
        let nullifiers = NullifierStore::new(db)?;
        let merkle_roots = RootStore::new(db)?;

        Ok(Self { blocks, transactions, streamlet_metadata, nullifiers, merkle_roots })
    }

    /// Batch insert [`BlockProposal`]s.
    pub fn add(&mut self, proposals: &[BlockProposal]) -> Result<Vec<blake3::Hash>> {
        // TODO: Engineer this function in a better way.
        let mut ret = Vec::with_capacity(proposals.len());

        for prop in proposals {
            // Store transactions
            let tx_hashes = self.transactions.insert(&prop.txs)?;

            // Store block
            let block =
                Block { st: prop.st, sl: prop.sl, txs: tx_hashes, metadata: prop.metadata.clone() };
            let blockhash = self.blocks.insert(&[block])?;
            ret.push(blockhash[0]);

            // Store streamlet metadata
            self.streamlet_metadata.insert(&[blockhash[0]], &[prop.sm.clone()])?;
        }

        Ok(ret)
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
