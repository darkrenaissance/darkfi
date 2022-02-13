use super::block::Block;

/// This struct represents a tuple of the form (vote, B, id).
#[derive(Debug, Clone)]
pub struct Vote {
    /// signed block
    pub vote: String,
    /// block to vote on
    pub block: Block,
    /// node id
    pub id: u64,
}

impl Vote {
    pub fn new(vote: String, block: Block, id: u64) -> Vote {
        Vote { vote, block, id }
    }
}

impl PartialEq for Vote {
    fn eq(&self, other: &Self) -> bool {
        self.vote == other.vote && self.block == other.block && self.id == other.id
    }
}
