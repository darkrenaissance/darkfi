use darkfi_sdk::crypto::MerkleNode;
use pasta_curves::pallas;

use crate::{
    consensus::{BlockInfo, Header, Metadata},
    tx::Transaction,
    util::time::Timestamp,
};

#[derive(Debug)]
pub struct SlotWorkspace {
    pub st: blake3::Hash,      // hash of the previous block
    pub e: u64,                // epoch index
    pub sl: u64,               // relative slot index
    pub txs: Vec<Transaction>, // unpublished block transactions
    pub root: MerkleNode,
    /// merkle root of txs
    pub m: Vec<Metadata>,
    pub is_leader: Vec<bool>,
    pub block: BlockInfo,
    pub idx: usize, // index of the highest winning coin
}

impl Default for SlotWorkspace {
    fn default() -> Self {
        Self {
            st: blake3::hash(b""),
            e: 0,
            sl: 0,
            txs: vec![],
            root: MerkleNode::from(pallas::Base::zero()),
            is_leader: vec![],
            m: vec![],
            block: BlockInfo::default(),
            idx: 0,
        }
    }
}

impl SlotWorkspace {
    /// create new block from the workspace
    /// if there are multiple winning coins (owned by the same stakeholder)
    /// then pick the highest winning coin.
    /// returns tuple of blockinfo, hash of that block
    pub fn new_block(&self) -> (BlockInfo, blake3::Hash) {
        let header = Header::new(self.st, self.e, self.sl, Timestamp::current_time(), self.root);
        let block = BlockInfo::new(header, self.txs.clone(), self.m[self.idx].clone());
        let hash = block.blockhash();
        (block, hash)
    }

    // each opoch research the worksapce
    fn reset(&mut self) {
        self.is_leader = vec![];
        self.m = vec![];
    }

    pub fn add_tx(&mut self, tx: Transaction) {
        self.txs.push(tx);
    }

    pub fn set_root(&mut self, root: MerkleNode) {
        self.root = root;
    }

    pub fn add_metadata(&mut self, meta: Metadata) {
        self.m.push(meta);
    }

    pub fn set_sl(&mut self, sl: u64) {
        self.sl = sl;
        self.reset();
    }

    pub fn set_st(&mut self, st: blake3::Hash) {
        self.st = st;
    }

    pub fn set_e(&mut self, e: u64) {
        self.e = e;
    }

    pub fn add_leader(&mut self, alead: bool) {
        self.is_leader.push(alead);
    }

    pub fn has_leader(&self) -> bool {
        self.is_leader.iter().any(|&x| x)
    }

    pub fn set_idx(&mut self, idx: usize) {
        self.idx = idx;
    }
}
