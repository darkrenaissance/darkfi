use std::io;

use log::debug;

use std::error;

use crate::{
    consensus::{Block, BlockInfo, StreamletMetadata},
    impl_vec,
    util::{
        serial::{Decodable, Encodable, ReadExt, VarInt, WriteExt},
        time::Timestamp,
    },
    Result,
};

pub mod epoch;
pub use epoch::{Epoch, EpochItem,EpochConsensus};

pub mod blockstore;
pub use blockstore::{BlockOrderStore, BlockStore};

pub mod metadatastore;
pub use metadatastore::StreamletMetadataStore;
pub use metadatastore::OuroborosMetadataStore;

pub mod nfstore;
pub use nfstore::NullifierStore;

pub mod rootstore;
pub use rootstore::RootStore;

pub mod txstore;
pub use txstore::TxStore;

/// Structure holding all sled trees that comprise the concept of Blockchain.
pub struct Blockchain {
    /// Blocks sled tree
    pub blocks: BlockStore,
    /// Block order sled tree
    pub order: BlockOrderStore,
    /// Transactions sled tree
    pub transactions: TxStore,
    /// Ourobors metadata sled tree
    pub ouroboros_metadata: OuroborosMetadataStore,
    /// Nullifiers sled tree
    pub nullifiers: NullifierStore,
    /// Merkle roots sled tree
    pub merkle_roots: RootStore,
}

impl Blockchain {
    //FIXME why the blockchain taking genesis_data on the constructor as a hash?
    //genesis data are supposed to be a a hash?
    /// Instantiate a new `Blockchain` with the given `sled` database.
    pub fn new(db: &sled::Db, genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Result<Self> {
        let blocks = BlockStore::new(db, genesis_ts, genesis_data)?;
        let order = BlockOrderStore::new(db, genesis_ts, genesis_data)?;
        let ouroboros_metadata = OuroborosMetadataStore::new(db, genesis_ts, genesis_data)?;
        let transactions = TxStore::new(db)?;
        let nullifiers = NullifierStore::new(db)?;
        let merkle_roots = RootStore::new(db)?;

        Ok(Self { blocks, order, transactions, ouroboros_metadata, nullifiers, merkle_roots })
    }

    /// Insert a given slice of [`BlockInfo`] into the blockchain database.
    /// This functions wraps all the logic of separating the block into specific
    /// data that can be fed into the different trees of the database.
    /// Upon success, the functions returns a vector of the block hashes that
    /// were given and appended to the ledger.
    pub fn add(&self, blocks: &[BlockInfo]) -> Result<Vec<blake3::Hash>> {
        let mut ret = Vec::with_capacity(blocks.len());

        for block in blocks {
            // Store transactions
            let tx_hashes = self.transactions.insert(&block.txs)?;

            // Store block
            let _block = Block::new(block.st, block.e, block.sl, tx_hashes, block.metadata.clone());
            let blockhash = self.blocks.insert(&[_block])?;
            ret.push(blockhash[0]);

            // Store block order
            self.order.insert(&[block.sl], &[blockhash[0]])?;

            // Store ouroboros metadata
            self.ouroboros_metadata.insert(&[blockhash[0]], &[block.metadata.om.clone()])?;

            // NOTE: The nullifiers and Merkle roots are applied in the state
            // transition apply function.
        }

        Ok(ret)
    }

    /// Check if the given [`BlockInfo`] is in the database and all trees.
    pub fn has_block(&self, block: &BlockInfo) -> Result<bool> {
        let blockhash = match self.order.get(&[block.sl], true) {
            Ok(v) => v[0].unwrap(),
            Err(_) => return Ok(false),
        };

        // TODO: Check if we have all transactions

        // Check provided info produces the same hash
        Ok(blockhash == block.blockhash())
    }

    /// Retrieve [`BlockInfo`]s by given hashes. Fails if any of them are not found.
    pub fn get_blocks_by_hash(&self, hashes: &[blake3::Hash]) -> Result<Vec<BlockInfo>> {
        let mut ret = Vec::with_capacity(hashes.len());

        let blocks = self.blocks.get(hashes, true)?;
        let metadata = self.ouroboros_metadata.get(hashes, true)?;

        for (i, block) in blocks.iter().enumerate() {
            let block = block.clone().unwrap();
            // empty streamlet censensus.
            let sm = StreamletMetadata::new(vec![]);
            let txs = self.transactions.get(&block.txs, true)?;
            let txs = txs.iter().map(|x| x.clone().unwrap()).collect();

            let info = BlockInfo::new(block.st, block.e, block.sl, txs, block.metadata.clone(), sm);
            ret.push(info);
        }

        Ok(ret)
    }

    /// Retrieve [`BlockInfo`]s by given slots. Does not fail if any of them are not found.
    pub fn get_blocks_by_slot(&self, slots: &[u64]) -> Result<Vec<BlockInfo>> {
        debug!("get_blocks_by_slot(): {:?}", slots);
        let blockhashes = self.order.get(slots, false)?;

        let mut hashes = vec![];
        for i in blockhashes.into_iter().flatten() {
            hashes.push(i);
        }

        self.get_blocks_by_hash(&hashes)
    }

    /// Retrieve n blocks after given start slot.
    pub fn get_blocks_after(&self, slot: u64, n: u64) -> Result<Vec<BlockInfo>> {
        debug!("get_blocks_after(): {} -> {}", slot, n);
        let hashes = self.order.get_after(slot, n)?;
        self.get_blocks_by_hash(&hashes)
    }

    /// Retrieve the last block slot and hash.
    pub fn last(&self) -> Result<(u64, blake3::Hash)> {
        self.order.get_last()
    }

    pub fn get_last_proof_hash(&self) -> Result<blake3::Hash> {
        let (hash, om) = self.ouroboros_metadata.get_last().unwrap();
        Ok(hash)
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
