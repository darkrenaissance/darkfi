/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{fs::File, io::Write};

use darkfi::{
    blockchain::{
        blockstore::{BlockOrderStore, BlockStore, HeaderStore},
        txstore::TxStore,
        Blockchain,
    },
    consensus::{
        block::{Block, BlockProposal, Header, ProposalChain},
        constants::TESTNET_GENESIS_HASH_BYTES,
        lead_info::LeadInfo,
        state::{ConsensusState, ValidatorState},
    },
    tx::Transaction,
    util::{path::expand_path, time::Timestamp},
    wallet::walletdb::init_wallet,
    Result,
};
use darkfi_sdk::crypto::MerkleNode;
use darkfi_serial::serialize;

// TODO: Add missing fields
#[derive(Debug)]
struct LeadInfoInfo {
    _public_key: String,
}

impl LeadInfoInfo {
    pub fn new(lead_info: &LeadInfo) -> LeadInfoInfo {
        let _public_key = lead_info.public_key.to_string();
        LeadInfoInfo { _public_key }
    }
}

#[derive(Debug)]
struct ProposalInfo {
    _block: BlockInfo,
}

impl ProposalInfo {
    pub fn new(proposal: &BlockProposal) -> ProposalInfo {
        let _header = proposal.block.header.headerhash();
        let mut _txs = vec![];
        for tx in &proposal.block.txs {
            let hash = blake3::hash(&serialize(tx));
            _txs.push(hash);
        }
        let _lead_info = LeadInfoInfo::new(&proposal.block.lead_info);
        let _block =
            BlockInfo { _hash: _header, _magic: proposal.block.magic, _header, _txs, _lead_info };
        ProposalInfo { _block }
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
    _version: u8,
    _previous: blake3::Hash,
    _epoch: u64,
    _slot: u64,
    _timestamp: Timestamp,
    _root: MerkleNode,
}

impl HeaderInfo {
    pub fn new(_hash: blake3::Hash, header: &Header) -> HeaderInfo {
        let _version = header.version;
        let _previous = header.previous;
        let _epoch = header.epoch;
        let _slot = header.slot;
        let _timestamp = header.timestamp;
        let _root = header.root;
        HeaderInfo { _hash, _version, _previous, _epoch, _slot, _timestamp, _root }
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
    _lead_info: LeadInfoInfo,
}

impl BlockInfo {
    pub fn new(_hash: blake3::Hash, block: &Block) -> BlockInfo {
        let _magic = block.magic;
        let _header = block.header;
        let _txs = block.txs.clone();
        let _lead_info = LeadInfoInfo::new(&block.lead_info);
        BlockInfo { _hash, _magic, _header, _txs, _lead_info }
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
    _slot: u64,
    _hash: blake3::Hash,
}

impl OrderInfo {
    pub fn new(_slot: u64, _hash: blake3::Hash) -> OrderInfo {
        OrderInfo { _slot, _hash }
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
struct BlockchainInfo {
    _headers: HeaderStoreInfo,
    _blocks: BlockInfoChain,
    _order: BlockOrderStoreInfo,
    _transactions: TxStoreInfo,
}

impl BlockchainInfo {
    pub fn new(blockchain: &Blockchain) -> BlockchainInfo {
        let _headers = HeaderStoreInfo::new(&blockchain.headers);
        let _blocks = BlockInfoChain::new(&blockchain.blocks);
        let _order = BlockOrderStoreInfo::new(&blockchain.order);
        let _transactions = TxStoreInfo::new(&blockchain.transactions);
        BlockchainInfo { _headers, _blocks, _order, _transactions }
    }
}

#[derive(Debug)]
struct StateInfo {
    _consensus: ConsensusInfo,
    _blockchain: BlockchainInfo,
}

impl StateInfo {
    pub fn new(state: &ValidatorState) -> StateInfo {
        let _consensus = ConsensusInfo::new(&state.consensus);
        let _blockchain = BlockchainInfo::new(&state.blockchain);
        StateInfo { _consensus, _blockchain }
    }
}

async fn generate(name: &str, folder: &str) -> Result<()> {
    let genesis_ts = Timestamp(1648383795);
    let genesis_data = *TESTNET_GENESIS_HASH_BYTES;
    let pass = "changeme";
    // Initialize or load wallet
    let path = folder.to_owned() + "/wallet.db";
    let wallet = init_wallet(&path, &pass).await?;

    // Initialize or load sled database
    let path = folder.to_owned() + "/blockchain/testnet";
    let db_path = expand_path(&path).unwrap();
    let sled_db = sled::open(&db_path)?;

    // Data export
    let state =
        ValidatorState::new(&sled_db, genesis_ts, genesis_data, wallet, vec![], false).await?;
    println!("Exporting data for {:?}", name);
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
    generate("darkfid0", "../../../contrib/localnet/darkfid/darkfid0").await?;
    // darkfid1
    generate("darkfid1", "../../../contrib/localnet/darkfid/darkfid1").await?;
    // darkfid2
    generate("darkfid2", "../../../contrib/localnet/darkfid/darkfid2").await?;
    // faucetd
    generate("faucetd", "../../../contrib/localnet/darkfid/faucetd").await?;

    Ok(())
}
