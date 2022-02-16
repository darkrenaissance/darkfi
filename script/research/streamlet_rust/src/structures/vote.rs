use super::block::Block;
use darkfi::crypto::schnorr::Signature;

/// This struct represents a tuple of the form (vote, B, id).
#[derive(Debug, Clone, PartialEq)]
pub struct Vote {
    /// signed block
    pub vote: Signature,
    /// block to vote on
    pub block: Block,
    /// node id
    pub id: u64,
}

impl Vote {
    pub fn new(vote: Signature, block: Block, id: u64) -> Vote {
        Vote { vote, block, id }
    }
}
