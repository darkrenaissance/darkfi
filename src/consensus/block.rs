use std::fmt;

use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use log::debug;
use pasta_curves::pallas;
use super::{StakeholderMetadata, StreamletMetadata, OuroborosMetadata, BLOCK_MAGIC_BYTES, BLOCK_VERSION};

use crate::{
    crypto::{
        constants::MERKLE_DEPTH, merkle_node::MerkleNode,
    },
    net,
    tx::Transaction,
    util::{
        serial::{serialize, SerialDecodable, SerialEncodable},
        time::Timestamp,
    },
};

/// This struct represents a tuple of the form (version, state, epoch, slot, timestamp, merkle_root).
#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Header {
    /// Block version
    pub version: u8,
    /// Previous block hash
    pub state: blake3::Hash,
    /// Epoch
    pub epoch: u64,
    /// Slot UID
    pub slot: u64,
    /// Block creation timestamp
    pub timestamp: Timestamp,
    /// Root of the transaction hashes merkle tree
    pub root: MerkleNode,
}

impl Header {
    pub fn new(
        state: blake3::Hash,
        epoch: u64,
        slot: u64,
        timestamp: Timestamp,
        root: MerkleNode,
    ) -> Self {
        let version = *BLOCK_VERSION;
        Self { version, state, epoch, slot, timestamp, root }
    }

    /// Generate the genesis block.
    pub fn genesis_header(genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Self {
        let tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100);
        let root = tree.root(0).unwrap();

        Self::new(genesis_data, 0, 0, genesis_ts, root)
    }

    /// Calculate the header hash
    pub fn headerhash(&self) -> blake3::Hash {
        blake3::hash(&serialize(self))
    }
}

impl Default for Header {
    fn default() -> Self {
        Header::new(blake3::hash(b""), 0 ,0, Timestamp::current_time(), MerkleNode(pallas::Base::zero()))
    }
}

/// This struct represents a tuple of the form (`magic`, `header`, `counter`, `txs`, `metadata`).
/// The header and transactions are stored as hashes, serving as pointers to
/// the actual data in the sled database.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Block {
    /// Block magic bytes
    pub magic: [u8; 4],
    /// Block header
    pub header: blake3::Hash,
    /// Trasaction hashes
    pub txs: Vec<blake3::Hash>,
    /// stakeholder metadata
    pub m: StakeholderMetadata,
    /// ouroboros block information,
    pub om: OuroborosMetadata,
    /// streamlet
    pub sm: StreamletMetadata,
}

impl net::Message for Block {
    fn name() -> &'static str {
        "block"
    }
}

impl Block {
    pub fn new(
        st: blake3::Hash,
        e: u64,
        sl: u64,
        txs: Vec<blake3::Hash>,
        root: MerkleNode,
        m: StakeholderMetadata,
        om: OuroborosMetadata,
        sm: StreamletMetadata,
    ) -> Self {
        let magic = *BLOCK_MAGIC_BYTES;
        let ts = Timestamp::current_time();
        let header = Header::new(st, e, sl, ts, root);
        let headerhash = header.headerhash();
        Self { magic:magic,
               header: headerhash,
               txs: txs,
               m: m,
               om: om,
               sm: sm
        }
    }

    /// Generate the genesis block.
    pub fn genesis_block(genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Self {
        let magic = *BLOCK_MAGIC_BYTES;
        //let eta : [u8; 32] = *blake3::hash(b"let there be dark!").as_bytes();
        //let empty_lead_proof = TransactionLeadProof::default();
        let header = Header::genesis_header(genesis_ts, genesis_data);
        let m = StakeholderMetadata::default();
        let om = OuroborosMetadata::default();
        let sm = StreamletMetadata::default();
        Self{ magic: magic,
              header: header.headerhash(),
              txs: vec![],
              m: m,
              om: om,
              sm: sm
        }
    }

    /// Calculate the block hash
    pub fn blockhash(&self) -> blake3::Hash {
        blake3::hash(&serialize(self))
    }
}

/// Auxiliary structure used for blockchain syncing.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct BlockOrder {
    /// Slot UID
    pub slot: u64,
    /// Block headerhash of that slot
    pub block: blake3::Hash,
}

impl net::Message for BlockOrder {
    fn name() -> &'static str {
        "blockorder"
    }
}

/// Structure representing full block data.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BlockInfo {
    /// BlockInfo magic bytes
    pub magic: [u8; 4],
    /// Block header data
    pub header: Header,
    /// Transactions payload
    pub txs: Vec<Transaction>,
    /// stakeholder metadata,
    pub m: StakeholderMetadata,
    /// ouroboros metadata
    pub om: OuroborosMetadata,
    /// Proposal information used by Streamlet consensus
    pub sm: StreamletMetadata,
}

impl Default for BlockInfo {
    fn default() -> Self {
        let magic = *BLOCK_MAGIC_BYTES;
        Self {
            magic: magic,
            header: Header::default(),
            txs: vec![],
            m: StakeholderMetadata::default(),
            om: OuroborosMetadata::default(),
            sm: StreamletMetadata::default(),
        }
    }
}

impl net::Message for BlockInfo {
    fn name() -> &'static str {
        "blockinfo"
    }
}

impl BlockInfo {
    pub fn new(
        header: Header,
        txs: Vec<Transaction>,
        m: StakeholderMetadata,
        om: OuroborosMetadata,
        sm: StreamletMetadata
    ) -> Self {
        let magic = *BLOCK_MAGIC_BYTES;
        Self {magic, header, txs, m, om, sm}
    }

    /// Calculate the block hash
    pub fn blockhash(&self) -> blake3::Hash {
        let block: Block = self.clone().into();
        block.blockhash()
    }
}

impl From<BlockInfo> for Block {
    fn from(b: BlockInfo) -> Self {
        let txids = b.txs.iter().map(|x| blake3::hash(&serialize(x))).collect();
        Self { magic: b.magic,
               header: b.header.headerhash(),
               txs: txids,
               m: b.m,
               om: b.om,
               sm: b.sm,
        }
    }
}

/// Auxiliary structure used for blockchain syncing
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BlockResponse {
    /// Response blocks.
    pub blocks: Vec<BlockInfo>,
}

impl net::Message for BlockResponse {
    fn name() -> &'static str {
        "blockresponse"
    }
}

/// This struct represents a block proposal, used for consensus.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BlockProposal {
    /// Block data
    pub block: BlockInfo,
}

impl BlockProposal {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        header: Header,
        txs: Vec<Transaction>,
        m: StakeholderMetadata,
        om: OuroborosMetadata,
        sm: StreamletMetadata,
    ) -> Self {
        let block = BlockInfo::new(header, txs, m, om, sm);
        Self { block }
    }
}

impl PartialEq for BlockProposal {
    fn eq(&self, other: &Self) -> bool {
        self.block.header == other.block.header &&
            self.block.txs == other.block.txs
    }
}

impl fmt::Display for BlockProposal {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_fmt(format_args!(
            "BlockProposal {{ leader addr: {}, hash: {}, epoch: {}, slot: {}, txs: {} }}",
            self.block.m.address,
            self.block.header.headerhash(),
            self.block.header.epoch,
            self.block.header.slot,
            self.block.txs.len()
        ))
    }
}

impl net::Message for BlockProposal {
    fn name() -> &'static str {
        "proposal"
    }
}

impl From<BlockProposal> for BlockInfo {
    fn from(block: BlockProposal) -> BlockInfo {
        block.block
    }
}

/// This struct represents a sequence of block proposals.
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct ProposalChain {
    pub genesis_block: blake3::Hash,
    pub proposals: Vec<BlockProposal>,
}

impl ProposalChain {
    pub fn new(genesis_block: blake3::Hash, initial_proposal: BlockProposal) -> Self {
        Self { genesis_block, proposals: vec![initial_proposal] }
    }

    /// A proposal is considered valid when its parent hash is equal to the
    /// hash of the previous proposal and their slots are incremental,
    /// excluding the genesis block proposal.
    /// Additional validity rules can be applied.
    pub fn check_proposal(&self, proposal: &BlockProposal, previous: &BlockProposal) -> bool {
        if proposal.block.header.state == self.genesis_block {
            debug!("check_proposal(): Genesis block proposal provided.");
            return false
        }

        let prev_hash = previous.block.header.headerhash();
        if proposal.block.header.state != prev_hash ||
            proposal.block.header.slot <= previous.block.header.slot
        {
            debug!("check_proposal(): Provided proposal is invalid.");
            return false
        }

        true
    }

    /// A proposals chain is considered valid when every proposal is valid,
    /// based on the `check_proposal` function.
    pub fn check_chain(&self) -> bool {
        for (index, proposal) in self.proposals[1..].iter().enumerate() {
            if !self.check_proposal(proposal, &self.proposals[index]) {
                return false
            }
        }

        true
    }

    /// Insertion of a valid proposal.
    pub fn add(&mut self, proposal: &BlockProposal) {
        if self.check_proposal(proposal, self.proposals.last().unwrap()) {
            self.proposals.push(proposal.clone());
        }
    }

    /// Proposals chain notarization check.
    pub fn notarized(&self) -> bool {
        for proposal in &self.proposals {
            if !proposal.block.sm.notarized {
                return false
            }
        }

        true
    }
}
