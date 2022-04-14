use std::io;

use log::debug;

use crate::{
    impl_vec,
    util::serial::{Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt},
    Result,
};

use super::{
    block::{Block, BlockProposal, BlockStore},
    metadata::MetadataStore,
    tx::TxStore,
    util::{to_block_serial, GENESIS_HASH_BYTES},
};

/// This struct represents a sequence of blocks starting with the genesis block.
#[derive(Debug)]
pub struct Blockchain {
    pub blocks: BlockStore,
    pub transactions: TxStore,
    pub metadata: MetadataStore,
}

impl Blockchain {
    pub fn new(db: &sled::Db) -> Result<Blockchain> {
        let blocks = BlockStore::new(db)?;
        let transactions = TxStore::new(db)?;
        let metadata = MetadataStore::new(db)?;
        Ok(Blockchain { blocks, transactions, metadata })
    }

    /// Insertion of a block proposal.
    pub fn add(&mut self, proposal: BlockProposal) -> Result<blake3::Hash> {
        // Storing transactions
        let mut txs = Vec::new();
        for tx in proposal.txs {
            let hash = self.transactions.insert(&tx)?;
            txs.push(hash);
        }

        // Storing block
        let block = Block { st: proposal.st, sl: proposal.sl, txs };
        let hash = self.blocks.insert(&block)?;

        // Storing metadata
        self.metadata.insert(&proposal.metadata, hash)?;

        Ok(hash)
    }
}

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
        let previous_hash = blake3::hash(&to_block_serial(previous.st, previous.sl, &previous.txs));
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
