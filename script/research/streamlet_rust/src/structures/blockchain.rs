use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use super::block::Block;

/// This struct represents a sequence of blocks starting with the genesis block.
#[derive(Debug, Clone, PartialEq)]
pub struct Blockchain {
    pub blocks: Vec<Block>,
}

impl Blockchain {
    pub fn new(intial_block: Block) -> Blockchain {
        Blockchain { blocks: vec![intial_block] }
    }

    /// A block is considered valid when its parent hash is equal to the hash of the
    /// previous block and their epochs are incremental, exluding genesis.
    /// Additional validity rules can be applied.
    pub fn check_block_validity(&self, block: &Block, previous_block: &Block) {
        assert!(block.st != "⊥", "Genesis block provided.");
        let mut hasher = DefaultHasher::new();
        previous_block.hash(&mut hasher);
        assert!(
            block.st == hasher.finish().to_string() && block.sl > previous_block.sl,
            "Provided block is invalid."
        );
    }

    /// A blockchain is considered valid, when every block is valid, based on check_block_validity method.
    pub fn check_chain_validity(&self) {
        for (index, block) in self.blocks[1..].iter().enumerate() {
            self.check_block_validity(&block, &self.blocks[index])
        }
    }

    /// Insertion of a valid block.
    pub fn add_block(&mut self, block: &Block) {
        self.check_block_validity(&block, &self.blocks.last().unwrap());
        self.blocks.push(block.clone());
    }

    /// Blockchain notarization check.
    pub fn is_notarized(&self) -> bool {
        for block in &self.blocks {
            if !block.metadata.sm.notarized {
                return false
            }
        }
        true
    }
}
