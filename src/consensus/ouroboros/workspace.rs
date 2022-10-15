use crate::{
    consensus::{
        BlockInfo, Header, Metadata,
    },
    tx::{
        Transaction,
    },
    util::{ time::Timestamp},
    crypto::{
        merkle_node::MerkleNode,
        proof::{Proof},
    }
};
use pasta_curves::pallas;

#[derive(Debug)]
pub struct SlotWorkspace {
    pub st: blake3::Hash,      // hash of the previous block
    pub e: u64,                // epoch index
    pub sl: u64,               // relative slot index
    pub txs: Vec<Transaction>, // unpublished block transactions
    pub root: MerkleNode,
    /// merkle root of txs
    pub m: Metadata,
    pub is_leader: bool,
    pub proof: Proof,
    pub block: BlockInfo,
}

impl Default for SlotWorkspace {
    fn default() -> Self {
        Self {
            st: blake3::hash(b""),
            e: 0,
            sl: 0,
            txs: vec![],
            root: MerkleNode(pallas::Base::zero()),
            is_leader: false,
            m: Metadata::default(),
            proof: Proof::default(),
            block: BlockInfo::default(),
        }
    }
}

impl SlotWorkspace {
    pub fn new_block(&self) -> (BlockInfo, blake3::Hash) {
        let header = Header::new(self.st, self.e, self.sl, Timestamp::current_time(), self.root);
        let block = BlockInfo::new(header, self.txs.clone(), self.m.clone());
        let hash = block.blockhash();
        (block, hash)
    }

    pub fn add_tx(&mut self, tx: Transaction) {
        self.txs.push(tx);
    }

    pub fn set_root(&mut self, root: MerkleNode) {
        self.root = root;
    }

    pub fn set_metadata(&mut self, meta: Metadata) {
        self.m = meta;
    }

    pub fn set_sl(&mut self, sl: u64) {
        self.sl = sl;
    }

    pub fn set_st(&mut self, st: blake3::Hash) {
        self.st = st;
    }

    pub fn set_e(&mut self, e: u64) {
        self.e = e;
    }

    pub fn set_proof(&mut self, proof: Proof) {
        self.proof = proof;
    }

    pub fn set_leader(&mut self, alead: bool) {
        self.is_leader = alead;
    }
}
