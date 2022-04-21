use std::io;

use crate::{
    consensus2::{block::BlockInfo, util::Timestamp, Block, BlockProposal},
    impl_vec,
    util::serial::{Decodable, Encodable, ReadExt, VarInt, WriteExt},
    Result,
};

pub mod blockstore;
pub use blockstore::{BlockOrderStore, BlockStore};

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
    /// Block order sled tree
    pub order: BlockOrderStore,
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
        let order = BlockOrderStore::new(db, genesis_ts, genesis_data)?;
        let transactions = TxStore::new(db)?;
        let streamlet_metadata = StreamletMetadataStore::new(db)?;
        let nullifiers = NullifierStore::new(db)?;
        let merkle_roots = RootStore::new(db)?;

        Ok(Self { blocks, order, transactions, streamlet_metadata, nullifiers, merkle_roots })
    }

    /// Batch insert [`BlockInfo`]s.
    pub fn add(&mut self, blocks: &[BlockInfo]) -> Result<Vec<blake3::Hash>> {
        let mut ret = Vec::with_capacity(blocks.len());

        for block in blocks {
            // Store transactions
            let tx_hashes = self.transactions.insert(&block.txs)?;

            // Store block
            let _block = Block::new(block.st, block.sl, tx_hashes, block.metadata.clone());
            let blockhash = self.blocks.insert(&[_block])?;
            ret.push(blockhash[0]);

            // Store block order
            self.order.insert(&[block.sl], &[blockhash[0]])?;

            // Store Streamlet metadata
            self.streamlet_metadata.insert(&[blockhash[0]], &[block.sm.clone()])?;
        }

        Ok(ret)
    }

    /// Retrieve blocks by given hashes. Fails if any of them are not found.
    pub fn get_blocks_by_hash(&self, blockhashes: &[blake3::Hash]) -> Result<Vec<BlockInfo>> {
        let mut ret = Vec::with_capacity(blockhashes.len());

        let blocks = self.blocks.get(blockhashes, true)?;
        let metadata = self.streamlet_metadata.get(blockhashes, true)?;

        for (i, block) in blocks.iter().enumerate() {
            let block = block.clone().unwrap();
            let sm = metadata[i].clone().unwrap();

            let txs = self.transactions.get(&block.txs, true)?;
            let txs = txs.iter().map(|x| x.clone().unwrap()).collect();

            let info = BlockInfo::new(block.st, block.sl, txs, block.metadata.clone(), sm);
            ret.push(info);
        }

        Ok(ret)
    }

    /// Retrieve blocks by given slots. Fails if any of them are not found.
    pub fn get_blocks_by_slot(&self, slots: &[u64]) -> Result<Vec<BlockInfo>> {
        let blockhashes = self.order.get(slots, true)?;
        let blockhashes: Vec<blake3::Hash> = blockhashes.iter().map(|x| x.unwrap()).collect();
        self.get_blocks_by_hash(&blockhashes)
    }

    /// Check if the given [`BlockInfo`] is in the database
    pub fn has_block(&self, info: &BlockInfo) -> Result<bool> {
        let hashes = self.order.get(&[info.sl], true)?;
        if hashes.is_empty() {
            return Ok(false)
        }

        if let Some(found) = &hashes[0] {
            // Check provided info produces the same hash
            // TODO: This BlockProposal::to_proposal_hash function should be in a better place.
            let blockhash =
                BlockProposal::to_proposal_hash(info.st, info.sl, &info.txs, &info.metadata);

            return Ok(&blockhash == found)
        }

        Ok(false)
    }

    /// Retrieve the last block slot and hash
    pub fn last(&self) -> Result<Option<(u64, blake3::Hash)>> {
        self.order.get_last()
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
