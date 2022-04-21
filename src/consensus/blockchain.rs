use std::io;

use log::debug;

use crate::{
    impl_vec, net,
    util::serial::{Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt},
    Result,
};

use super::{
    block::{Block, BlockInfo, BlockOrderStore, BlockProposal, BlockStore},
    metadata::StreamletMetadataStore,
    tx::TxStore,
};

/// This struct represents the canonical (finalized) blockchain stored in sled database.
#[derive(Debug)]
pub struct Blockchain {
    /// Blocks sled database
    pub blocks: BlockStore,
    /// Blocks order sled database
    pub order: BlockOrderStore,
    /// Transactions sled database
    pub transactions: TxStore,
    /// Streamlet metadata sled database
    pub streamlet_metadata: StreamletMetadataStore,
}

impl Blockchain {
    pub fn new(db: &sled::Db, genesis: i64) -> Result<Blockchain> {
        let blocks = BlockStore::new(db, genesis)?;
        let order = BlockOrderStore::new(db, genesis)?;
        let transactions = TxStore::new(db)?;
        let streamlet_metadata = StreamletMetadataStore::new(db, genesis)?;
        Ok(Blockchain { blocks, order, transactions, streamlet_metadata })
    }

    /// Insertion of a block proposal.
    pub fn add_by_proposal(&mut self, proposal: BlockProposal) -> Result<blake3::Hash> {
        // Storing transactions
        let mut txs = Vec::new();
        for tx in proposal.txs {
            let hash = self.transactions.insert(&tx)?;
            txs.push(hash);
        }

        // Storing block
        let block = Block { st: proposal.st, sl: proposal.sl, txs, metadata: proposal.metadata };
        let hash = self.blocks.insert(&block)?;

        // Storing block order
        self.order.insert(block.sl, hash)?;

        // Storing streamlet metadata
        self.streamlet_metadata.insert(hash, &proposal.sm)?;

        Ok(hash)
    }

    /// Insertion of a block info.
    pub fn add_by_info(&mut self, info: BlockInfo) -> Result<blake3::Hash> {
        if self.has_block(&info)? {
            let blockhash =
                BlockProposal::to_proposal_hash(info.st, info.sl, &info.txs, &info.metadata);
            return Ok(blockhash)
        }

        // Storing transactions
        let mut txs = Vec::new();
        for tx in info.txs {
            let hash = self.transactions.insert(&tx)?;
            txs.push(hash);
        }

        // Storing block
        let block = Block { st: info.st, sl: info.sl, txs, metadata: info.metadata };
        let hash = self.blocks.insert(&block)?;

        // Storing block order
        self.order.insert(block.sl, hash)?;

        // Storing streamlet metadata
        self.streamlet_metadata.insert(hash, &info.sm)?;

        Ok(hash)
    }

    /// Retrieve the last block slot and hash.
    pub fn last(&self) -> Result<Option<(u64, blake3::Hash)>> {
        self.order.get_last()
    }

    /// Retrieve the last block slot and hash.
    pub fn has_block(&self, info: &BlockInfo) -> Result<bool> {
        let hashes = self.order.get(&vec![info.sl])?;
        if hashes.is_empty() {
            return Ok(false)
        }
        if let Some(found) = &hashes[0] {
            // Checking provided info produces same hash
            let blockhash =
                BlockProposal::to_proposal_hash(info.st, info.sl, &info.txs, &info.metadata);

            return Ok(blockhash == found.block)
        }
        Ok(false)
    }

    /// Retrieve n blocks with all their info, after start key.
    pub fn get_with_info(&self, key: u64, n: u64) -> Result<Vec<BlockInfo>> {
        let mut blocks_info = Vec::new();

        // Retrieve requested hashes from order store
        let hashes = self.order.get_after(key, n)?;

        // Retrieve blocks for found hashes
        let blocks = self.blocks.get(&hashes)?;

        // For each found block, retrieve its txs and metadata and convert to BlockProposal
        for option in blocks {
            match option {
                None => continue,
                Some((hash, block)) => {
                    let mut txs = Vec::new();
                    let found = self.transactions.get(&block.txs)?;
                    for option in found {
                        match option {
                            Some(tx) => txs.push(tx),
                            None => continue,
                        }
                    }
                    let sm = self.streamlet_metadata.get(&vec![hash])?[0].as_ref().unwrap().clone();
                    blocks_info.push(BlockInfo::new(block.st, block.sl, txs, block.metadata, sm));
                }
            }
        }

        Ok(blocks_info)
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
        genesis: &blake3::Hash,
    ) -> bool {
        if &proposal.st == genesis {
            debug!("Genesis block proposal provided.");
            return false
        }
        let previous_hash = previous.hash();
        if proposal.st != previous_hash || proposal.sl <= previous.sl {
            debug!("Provided proposal is invalid.");
            return false
        }
        true
    }

    /// A proposals chain is considered valid, when every proposal is valid, based on check_proposal function.
    pub fn check_chain(&self, genesis: &blake3::Hash) -> bool {
        for (index, proposal) in self.proposals[1..].iter().enumerate() {
            if !self.check_proposal(proposal, &self.proposals[index], genesis) {
                return false
            }
        }
        true
    }

    /// Insertion of a valid proposal.
    pub fn add(&mut self, proposal: &BlockProposal, genesis: &blake3::Hash) {
        if self.check_proposal(proposal, self.proposals.last().unwrap(), genesis) {
            self.proposals.push(proposal.clone());
        }
    }

    /// Proposals chain notarization check.
    pub fn notarized(&self) -> bool {
        for proposal in &self.proposals {
            if !proposal.sm.notarized {
                return false
            }
        }
        true
    }
}

impl_vec!(ProposalsChain);

/// Auxilary structure used for forks syncing.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct ForkOrder {
    /// Validator id
    pub id: u64,
}

impl net::Message for ForkOrder {
    fn name() -> &'static str {
        "forkorder"
    }
}

/// Auxilary structure used for forks syncing.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ForkResponse {
    /// Fork chains containing block proposals
    pub proposals: Vec<ProposalsChain>,
}

impl net::Message for ForkResponse {
    fn name() -> &'static str {
        "forkresponse"
    }
}
