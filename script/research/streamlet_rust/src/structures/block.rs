use std::hash::{Hash, Hasher};

use super::vote::Vote;

use darkfi::{tx::Transaction, util::serial::Encodable};

/// This struct represents a tuple of the form (h, e, txs).
/// Each blocks parent hash h may be computed simply as a hash of the parent block.
#[derive(Debug, Clone)]
pub struct Block {
    /// parent hash
    pub h: String,
    /// epoch number
    pub e: u64,
    /// transactions payload
    pub txs: Vec<Transaction>,
    /// Epoch votes
    pub votes: Vec<Vote>,
    /// block notarization flag
    pub notarized: bool,
    /// block finalization flag
    pub finalized: bool,
}

impl Block {
    pub fn new(h: String, e: u64, txs: Vec<Transaction>) -> Block {
        Block { h, e, txs, votes: Vec::new(), notarized: false, finalized: false }
    }

    pub fn signature_encode(&self) -> Vec<u8> {
        let mut encoded_block = Vec::new();
        let mut len = 0;
        len += self.h.encode(&mut encoded_block).unwrap();
        len += self.e.encode(&mut encoded_block).unwrap();
        len += self.txs.encode(&mut encoded_block).unwrap();
        assert_eq!(len, encoded_block.len());
        encoded_block
    }
}

impl PartialEq for Block {
    fn eq(&self, other: &Self) -> bool {
        self.h == other.h && self.e == other.e && self.txs == other.txs
    }
}

impl Hash for Block {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        format!("{:?}{:?}{:?}", self.h, self.e, self.txs).hash(hasher);
    }
}
