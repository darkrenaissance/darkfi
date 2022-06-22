use async_std::sync::Arc;
use std::{fs::File, io::Write};

use darkfi::{
    blockchain::{
        blockstore::{BlockOrderStore, BlockStore, HeaderStore},
        metadatastore::StreamletMetadataStore,
        txstore::TxStore,
        Blockchain,
    },
    consensus::{
        block::{Block, BlockProposal, Header, ProposalChain},
        metadata::{Metadata, StreamletMetadata},
        participant::Participant,
        state::{ConsensusState, ValidatorState},
        vote::Vote,
        TESTNET_GENESIS_HASH_BYTES,
    },
    crypto::{merkle_node::MerkleNode, token_list::DrkTokenList},
    node::Client,
    tx::Transaction,
    util::{expand_path, serial::serialize, time::Timestamp},
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
struct MetadataInfo {
    _proof: String,
    _r: String,
    _s: String,
}

impl MetadataInfo {
    pub fn new(metadata: &Metadata) -> MetadataInfo {
        let _proof = metadata.proof.clone();
        let _r = metadata.r.clone();
        let _s = metadata.s.clone();
        MetadataInfo { _proof, _r, _s }
    }
}

#[derive(Debug)]
struct ProposalInfo {
    _address: String,
    _block: BlockInfo,
    _sm: StreamletMetadataInfo,
}

impl ProposalInfo {
    pub fn new(proposal: &BlockProposal) -> ProposalInfo {
        let _address = proposal.address.to_string();
        let _header = proposal.block.header.headerhash();
        let mut _txs = vec![];
        for tx in &proposal.block.txs {
            let hash = blake3::hash(&serialize(tx));
            _txs.push(hash);
        }
        let _metadata = MetadataInfo::new(&proposal.block.metadata);
        let _block =
            BlockInfo { _hash: _header, _magic: proposal.block.magic, _header, _txs, _metadata };
        let _sm = StreamletMetadataInfo::new(&proposal.block.sm);
        ProposalInfo { _address, _block, _sm }
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
struct HeaderInfo {
    _hash: blake3::Hash,
    _v: u8,
    _st: blake3::Hash,
    _e: u64,
    _sl: u64,
    _timestamp: Timestamp,
    _root: MerkleNode,
}

impl HeaderInfo {
    pub fn new(_hash: blake3::Hash, header: &Header) -> HeaderInfo {
        let _v = header.v;
        let _st = header.st;
        let _e = header.e;
        let _sl = header.sl;
        let _timestamp = header.timestamp;
        let _root = header.root;
        HeaderInfo { _hash, _v, _st, _e, _sl, _timestamp, _root }
    }
}

#[derive(Debug)]
struct HeaderStoreInfo {
    _headers: Vec<HeaderInfo>,
}

impl HeaderStoreInfo {
    pub fn new(headerstore: &HeaderStore) -> HeaderStoreInfo {
        let mut _headers = Vec::new();
        let result = headerstore.get_all();
        match result {
            Ok(iter) => {
                for (hash, header) in iter.iter() {
                    _headers.push(HeaderInfo::new(hash.clone(), &header));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        HeaderStoreInfo { _headers }
    }
}

#[derive(Debug)]
struct BlockInfo {
    _hash: blake3::Hash,
    _magic: [u8; 4],
    _header: blake3::Hash,
    _txs: Vec<blake3::Hash>,
    _metadata: MetadataInfo,
}

impl BlockInfo {
    pub fn new(_hash: blake3::Hash, block: &Block) -> BlockInfo {
        let _magic = block.magic;
        let _header = block.header;
        let _txs = block.txs.clone();
        let _metadata = MetadataInfo::new(&block.metadata);
        BlockInfo { _hash, _magic, _header, _txs, _metadata }
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
                for (hash, block) in iter.iter() {
                    _blocks.push(BlockInfo::new(hash.clone(), &block));
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
                for (slot, hash) in iter.iter() {
                    _order.push(OrderInfo::new(slot.clone(), hash.clone()));
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
    _payload: Transaction,
}

impl TxInfo {
    pub fn new(_hash: blake3::Hash, tx: &Transaction) -> TxInfo {
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
                for (hash, tx) in iter.iter() {
                    _transactions.push(TxInfo::new(hash.clone(), &tx));
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
                for (hash, m) in iter.iter() {
                    _metadata.push(HashedMetadataInfo::new(hash.clone(), &m));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        MetadataStoreInfo { _metadata }
    }
}

#[derive(Debug)]
struct BlockchainInfo {
    _headers: HeaderStoreInfo,
    _blocks: BlockInfoChain,
    _order: BlockOrderStoreInfo,
    _transactions: TxStoreInfo,
    _metadata: MetadataStoreInfo,
}

impl BlockchainInfo {
    pub fn new(blockchain: &Blockchain) -> BlockchainInfo {
        let _headers = HeaderStoreInfo::new(&blockchain.headers);
        let _blocks = BlockInfoChain::new(&blockchain.blocks);
        let _order = BlockOrderStoreInfo::new(&blockchain.order);
        let _transactions = TxStoreInfo::new(&blockchain.transactions);
        let _metadata = MetadataStoreInfo::new(&blockchain.streamlet_metadata);
        BlockchainInfo { _headers, _blocks, _order, _transactions, _metadata }
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

async fn generate(name: &str, folder: &str) -> Result<()> {
    let genesis_ts = Timestamp(1648383795);
    let genesis_data = *TESTNET_GENESIS_HASH_BYTES;
    let pass = "changeme";
    // Initialize or load wallet
    let path = folder.to_owned() + "/wallet.db";
    let wallet = init_wallet(&path, &pass).await?;
    let address = wallet.get_default_address().await?;
    let tokenlist = Arc::new(DrkTokenList::new(&[
        ("drk", include_bytes!("../../../../contrib/token/darkfi_token_list.min.json")),
        ("btc", include_bytes!("../../../../contrib/token/bitcoin_token_list.min.json")),
        ("eth", include_bytes!("../../../../contrib/token/erc20_token_list.min.json")),
        ("sol", include_bytes!("../../../../contrib/token/solana_token_list.min.json")),
    ])?);
    let client = Arc::new(Client::new(wallet, tokenlist).await?);

    // Initialize or load sled database
    let path = folder.to_owned() + "/blockchain/testnet";
    let db_path = expand_path(&path).unwrap();
    let sled_db = sled::open(&db_path)?;

    // Data export
    println!("Exporting data for {:?} - {:?}", name, address.to_string());
    let state =
        ValidatorState::new(&sled_db, genesis_ts, genesis_data, client, vec![], vec![]).await?;
    let info = StateInfo::new(&*state.read().await);
    let info_string = format!("{:#?}", info);
    let path = name.to_owned() + "_testnet_db";
    let mut file = File::create(path)?;
    file.write(info_string.as_bytes())?;
    drop(sled_db);

    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    // darkfid0
    generate("darkfid0", "../../../contrib/localnet/darkfid0").await?;
    // darkfid1
    generate("darkfid1", "../../../contrib/localnet/darkfid1").await?;
    // darkfid2
    generate("darkfid2", "../../../contrib/localnet/darkfid2").await?;
    // faucetd
    generate("faucetd", "../../../contrib/localnet/faucetd").await?;

    Ok(())
}
