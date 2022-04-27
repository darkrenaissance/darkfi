use async_std::sync::Arc;
use std::{fs::File, io::Write};

use darkfi::{
    blockchain::{
        blockstore::{BlockOrderStore, BlockStore},
        metadatastore::StreamletMetadataStore,
        txstore::TxStore,
        Blockchain,
    },
    consensus::{
        block::{Block, BlockProposal, ProposalChain},
        metadata::{Metadata, OuroborosMetadata, StreamletMetadata},
        participant::Participant,
        state::{ConsensusState, ValidatorState},
        tx::Tx,
        util::Timestamp,
        vote::Vote,
        TESTNET_GENESIS_HASH_BYTES,
    },
    node::Client,
    util::expand_path,
    wallet::walletdb::init_wallet,
    Result,
};

#[derive(Debug)]
struct ParticipantInfo {
    _address: String,
    _joined: u64,
    _voted: Option<u64>,
}

impl ParticipantInfo {
    pub fn new(participant: &Participant) -> ParticipantInfo {
        let _address = participant.address.to_string();
        let _joined = participant.joined;
        let _voted = participant.voted;
        ParticipantInfo { _address, _joined, _voted }
    }
}

#[derive(Debug)]
struct VoteInfo {
    _proposal: blake3::Hash,
    _sl: u64,
    _address: String,
}

impl VoteInfo {
    pub fn new(vote: &Vote) -> VoteInfo {
        let _proposal = vote.proposal;
        let _sl = vote.sl;
        let _address = vote.address.to_string();
        VoteInfo { _proposal, _sl, _address }
    }
}

#[derive(Debug)]
struct StreamletMetadataInfo {
    _votes: Vec<VoteInfo>,
    _notarized: bool,
    _finalized: bool,
    _participants: Vec<ParticipantInfo>,
}

impl StreamletMetadataInfo {
    pub fn new(metadata: &StreamletMetadata) -> StreamletMetadataInfo {
        let mut _votes = Vec::new();
        for vote in &metadata.votes {
            _votes.push(VoteInfo::new(&vote));
        }
        let _notarized = metadata.notarized;
        let _finalized = metadata.finalized;
        let mut _participants = Vec::new();
        for participant in &metadata.participants {
            _participants.push(ParticipantInfo::new(&participant));
        }
        StreamletMetadataInfo { _votes, _notarized, _finalized, _participants }
    }
}

#[derive(Debug)]
struct OuroborosMetadataInfo {
    _proof: String,
    _r: String,
    _s: String,
}

impl OuroborosMetadataInfo {
    pub fn new(metadata: &OuroborosMetadata) -> OuroborosMetadataInfo {
        let _proof = metadata.proof.clone();
        let _r = metadata.r.clone();
        let _s = metadata.s.clone();
        OuroborosMetadataInfo { _proof, _r, _s }
    }
}

#[derive(Debug)]
struct MetadataInfo {
    _timestamp: Timestamp,
    _om: OuroborosMetadataInfo,
}

impl MetadataInfo {
    pub fn new(metadata: &Metadata) -> MetadataInfo {
        let _timestamp = metadata.timestamp.clone();
        let _om = OuroborosMetadataInfo::new(&metadata.om);
        MetadataInfo { _timestamp, _om }
    }
}

#[derive(Debug)]
struct ProposalInfo {
    _address: String,
    _st: blake3::Hash,
    _sl: u64,
    _txs: Vec<Tx>,
    _metadata: MetadataInfo,
    _sm: StreamletMetadataInfo,
}

impl ProposalInfo {
    pub fn new(proposal: &BlockProposal) -> ProposalInfo {
        let _address = proposal.address.to_string();
        let _st = proposal.block.st;
        let _sl = proposal.block.sl;
        let _txs = proposal.block.txs.clone();
        let _metadata = MetadataInfo::new(&proposal.block.metadata);
        let _sm = StreamletMetadataInfo::new(&proposal.block.sm);
        ProposalInfo { _address, _st, _sl, _txs, _metadata, _sm }
    }
}

#[derive(Debug)]
struct ProposalInfoChain {
    _proposals: Vec<ProposalInfo>,
}

impl ProposalInfoChain {
    pub fn new(proposals: &ProposalChain) -> ProposalInfoChain {
        let mut _proposals = Vec::new();
        for proposal in &proposals.proposals {
            _proposals.push(ProposalInfo::new(&proposal));
        }
        ProposalInfoChain { _proposals }
    }
}

#[derive(Debug)]
struct ConsensusInfo {
    _genesis_ts: Timestamp,
    _proposals: Vec<ProposalInfoChain>,
}

impl ConsensusInfo {
    pub fn new(consensus: &ConsensusState) -> ConsensusInfo {
        let _genesis_ts = consensus.genesis_ts.clone();
        let mut _proposals = Vec::new();
        for proposal in &consensus.proposals {
            _proposals.push(ProposalInfoChain::new(&proposal));
        }
        ConsensusInfo { _genesis_ts, _proposals }
    }
}

#[derive(Debug)]
struct BlockInfo {
    _hash: blake3::Hash,
    _st: blake3::Hash,
    _sl: u64,
    _txs: Vec<blake3::Hash>,
}

impl BlockInfo {
    pub fn new(_hash: blake3::Hash, block: &Block) -> BlockInfo {
        let _st = block.st;
        let _sl = block.sl;
        let _txs = block.txs.clone();
        BlockInfo { _hash, _st, _sl, _txs }
    }
}

#[derive(Debug)]
struct BlockInfoChain {
    _blocks: Vec<BlockInfo>,
}

impl BlockInfoChain {
    pub fn new(blockstore: &BlockStore) -> BlockInfoChain {
        let mut _blocks = Vec::new();
        let result = blockstore.get_all();
        match result {
            Ok(iter) => {
                for item in iter.iter() {
                    match item {
                        Some((hash, block)) => _blocks.push(BlockInfo::new(hash.clone(), &block)),
                        None => (),
                    };
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        BlockInfoChain { _blocks }
    }
}

#[derive(Debug)]
struct OrderInfo {
    _sl: u64,
    _hash: blake3::Hash,
}

impl OrderInfo {
    pub fn new(_sl: u64, _hash: blake3::Hash) -> OrderInfo {
        OrderInfo { _sl, _hash }
    }
}

#[derive(Debug)]
struct BlockOrderStoreInfo {
    _order: Vec<OrderInfo>,
}

impl BlockOrderStoreInfo {
    pub fn new(orderstore: &BlockOrderStore) -> BlockOrderStoreInfo {
        let mut _order = Vec::new();
        let result = orderstore.get_all();
        match result {
            Ok(iter) => {
                for item in iter.iter() {
                    match item {
                        Some((slot, hash)) => {
                            _order.push(OrderInfo::new(slot.clone(), hash.clone()))
                        }
                        None => (),
                    };
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        BlockOrderStoreInfo { _order }
    }
}

#[derive(Debug)]
struct TxInfo {
    _hash: blake3::Hash,
    _payload: Tx,
}

impl TxInfo {
    pub fn new(_hash: blake3::Hash, tx: &Tx) -> TxInfo {
        let _payload = tx.clone();
        TxInfo { _hash, _payload }
    }
}

#[derive(Debug)]
struct TxStoreInfo {
    _transactions: Vec<TxInfo>,
}

impl TxStoreInfo {
    pub fn new(txstore: &TxStore) -> TxStoreInfo {
        let mut _transactions = Vec::new();
        let result = txstore.get_all();
        match result {
            Ok(iter) => {
                for item in iter.iter() {
                    match item {
                        Some((hash, tx)) => _transactions.push(TxInfo::new(hash.clone(), &tx)),
                        None => (),
                    };
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        TxStoreInfo { _transactions }
    }
}

#[derive(Debug)]
struct HashedMetadataInfo {
    _block: blake3::Hash,
    _metadata: StreamletMetadataInfo,
}

impl HashedMetadataInfo {
    pub fn new(_block: blake3::Hash, metadata: &StreamletMetadata) -> HashedMetadataInfo {
        let _metadata = StreamletMetadataInfo::new(&metadata);
        HashedMetadataInfo { _block, _metadata }
    }
}

#[derive(Debug)]
struct MetadataStoreInfo {
    _metadata: Vec<HashedMetadataInfo>,
}

impl MetadataStoreInfo {
    pub fn new(metadatastore: &StreamletMetadataStore) -> MetadataStoreInfo {
        let mut _metadata = Vec::new();
        let result = metadatastore.get_all();
        match result {
            Ok(iter) => {
                for item in iter.iter() {
                    match item {
                        Some((hash, m)) => {
                            _metadata.push(HashedMetadataInfo::new(hash.clone(), &m))
                        }
                        None => (),
                    };
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        MetadataStoreInfo { _metadata }
    }
}

#[derive(Debug)]
struct BlockchainInfo {
    _blocks: BlockInfoChain,
    _order: BlockOrderStoreInfo,
    _transactions: TxStoreInfo,
    _metadata: MetadataStoreInfo,
}

impl BlockchainInfo {
    pub fn new(blockchain: &Blockchain) -> BlockchainInfo {
        let _blocks = BlockInfoChain::new(&blockchain.blocks);
        let _order = BlockOrderStoreInfo::new(&blockchain.order);
        let _transactions = TxStoreInfo::new(&blockchain.transactions);
        let _metadata = MetadataStoreInfo::new(&blockchain.streamlet_metadata);
        BlockchainInfo { _blocks, _order, _transactions, _metadata }
    }
}

#[derive(Debug)]
struct StateInfo {
    _address: String,
    _consensus: ConsensusInfo,
    _blockchain: BlockchainInfo,
}

impl StateInfo {
    pub fn new(state: &ValidatorState) -> StateInfo {
        let _address = state.address.to_string();
        let _consensus = ConsensusInfo::new(&state.consensus);
        let _blockchain = BlockchainInfo::new(&state.blockchain);
        StateInfo { _address, _consensus, _blockchain }
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    let nodes = 4;
    let genesis_ts = Timestamp(1648383795);
    let genesis_data = *TESTNET_GENESIS_HASH_BYTES;
    let pass = "changeme";
    for i in 0..nodes {
        // Initialize or load wallet
        let path = format!("../../../tmp/node{:?}/wallet.db", i);
        let wallet = init_wallet(&path, &pass).await?;
        let address = wallet.get_default_address().await?;
        let client = Arc::new(Client::new(wallet).await?);

        // Initialize or load sled database
        let path = format!("../../../tmp/node{:?}/blockchain/testnet", i);
        let db_path = expand_path(&path).unwrap();
        let sled_db = sled::open(&db_path)?;

        // Data export
        println!("Exporting data for node{:?} - {:?}", i, address.to_string());
        let state =
            ValidatorState::new(&sled_db, genesis_ts, genesis_data, client, vec![], vec![]).await?;
        let info = StateInfo::new(&*state.read().await);
        let info_string = format!("{:#?}", info);
        let path = format!("node{:?}_testnet_db", i);
        let mut file = File::create(path)?;
        file.write(info_string.as_bytes())?;
        drop(sled_db);
    }

    Ok(())
}
