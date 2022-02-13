use std::hash::{Hash, Hasher};

use super::vote::Vote;

/// This struct represents a tuple of the form (h, e, txs).
/// Each blocks parent hash h may be computed simply as a hash of the parent block.
#[derive(Debug, Clone)]
pub struct Block {
    /// parent hash
    pub h: String,
    /// epoch number
    pub e: i64,
    /// transactions payload
    pub txs: Vec<String>,
    /// Epoch votes
    pub votes: Vec<Vote>,
    /// block notarization flag
    pub notarized: bool,
    /// block finalization flag
    pub finalized: bool,
}

impl Block {
    pub fn new(h: String, e: i64, txs: Vec<String>) -> Block {
        Block { h, e, txs, votes: Vec::new(), notarized: false, finalized: false }
    }
}

impl PartialEq for Block {
    fn eq(&self, other: &Self) -> bool {
        self.h == other.h && self.e == other.e && self.txs == other.txs
    }
}

impl Hash for Block {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        (&self.h, &self.e, &self.txs).hash(hasher);
    }
}
