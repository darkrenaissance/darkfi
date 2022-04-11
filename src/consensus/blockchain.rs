use std::io;

use log::debug;

use crate::{
    encode_payload, impl_vec,
    util::serial::{serialize, Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt},
    Result,
};

use super::{
    block::{Block, BlockProposal},
    util::GENESIS_HASH_BYTES,
};

/// This struct represents a sequence of blocks starting with the genesis block.
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Blockchain {
    pub blocks: Vec<Block>,
}

impl Blockchain {
    pub fn new(initial_block: Block) -> Blockchain {
        Blockchain { blocks: vec![initial_block] }
    }

    /// A block is considered valid when its parent hash is equal to the hash of the
    /// previous block and their epochs are incremental, exluding genesis.
    /// Additional validity rules can be applied.
    pub fn check_block(&self, block: &Block, previous: &Block) -> Result<bool> {
        if block.st.as_bytes() == &GENESIS_HASH_BYTES {
            debug!("Genesis block provided.");
            return Ok(false)
        }
        let mut buf = vec![];
        encode_payload!(&mut buf, previous.st, previous.sl, previous.txs);
        let previous_hash = blake3::hash(&serialize(&buf));
        if block.st != previous_hash || block.sl <= previous.sl {
            debug!("Provided block is invalid.");
            return Ok(false)
        }
        Ok(true)
    }

    /// A blockchain is considered valid, when every block is valid, based on check_block function.
    pub fn check_chain(&self) -> bool {
        for (index, block) in self.blocks[1..].iter().enumerate() {
            if !self.check_block(block, &self.blocks[index]).unwrap() {
                return false
            }
        }
        true
    }

    /// Insertion of a valid block.
    pub fn add(&mut self, block: &Block) {
        self.check_block(block, self.blocks.last().unwrap()).unwrap();
        self.blocks.push(block.clone());
    }

    /// Blockchain notarization check.
    pub fn notarized(&self) -> bool {
        for block in &self.blocks {
            if !block.metadata.sm.notarized {
                return false
            }
        }
        true
    }
}

impl_vec!(Blockchain);

/// This struct represents a sequence of block proposals.
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct ProposalsChain {
    pub proposals: Vec<BlockProposal>,
}

impl ProposalsChain {
    pub fn new(initial_proposal: BlockProposal) -> ProposalsChain {
        ProposalsChain { proposals: vec![initial_proposal] }
    }

    /// A proposal is considered valid when its parent hash is equal to the hash of the
    /// previous proposal and their epochs are incremental, exluding genesis block proposal.
    /// Additional validity rules can be applied.
    pub fn check_proposal(
        &self,
        proposal: &BlockProposal,
        previous: &BlockProposal,
    ) -> Result<bool> {
        if proposal.st.as_bytes() == &GENESIS_HASH_BYTES {
            debug!("Genesis block proposal provided.");
            return Ok(false)
        }
        let mut buf = vec![];
        encode_payload!(&mut buf, previous.st, previous.sl, previous.txs);
        let previous_hash = blake3::hash(&serialize(&buf));
        if proposal.st != previous_hash || proposal.sl <= previous.sl {
            debug!("Provided proposal is invalid.");
            return Ok(false)
        }
        Ok(true)
    }

    /// A proposals chain is considered valid, when every proposal is valid, based on check_proposal function.
    pub fn check_chain(&self) -> bool {
        for (index, proposal) in self.proposals[1..].iter().enumerate() {
            if !self.check_proposal(proposal, &self.proposals[index]).unwrap() {
                return false
            }
        }
        true
    }

    /// Insertion of a valid proposal.
    pub fn add(&mut self, proposal: &BlockProposal) {
        self.check_proposal(proposal, self.proposals.last().unwrap()).unwrap();
        self.proposals.push(proposal.clone());
    }

    /// Proposals chain notarization check.
    pub fn notarized(&self) -> bool {
        for proposal in &self.proposals {
            if !proposal.metadata.sm.notarized {
                return false
            }
        }
        true
    }
}

impl_vec!(ProposalsChain);
