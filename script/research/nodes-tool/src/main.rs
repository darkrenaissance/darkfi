/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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
        block_store::{BlockOrderStore, BlockStore, HeaderStore},
        slot_checkpoint_store::SlotCheckpointStore,
        tx_store::{ErroneousTxStore, TxStore},
        Blockchain,
    },
    consensus::{
        block::{Block, Header},
        constants::TESTNET_GENESIS_HASH_BYTES,
        lead_info::LeadInfo,
        validator::ValidatorState,
    },
    tx::Transaction,
    util::{path::expand_path, time::Timestamp},
    wallet::walletdb::init_wallet,
    Result,
};
use darkfi_sdk::crypto::MerkleNode;

#[derive(Debug)]
struct LeadInfoInfo {
    _signature: String,
    _public_key: String,
    _public_inputs: Vec<String>,
    _coin_slot: u64,
    _coin_eta: String,
    _proof: String,
    _leaders: u64,
}

impl LeadInfoInfo {
    pub fn new(lead_info: &LeadInfo) -> LeadInfoInfo {
        let _signature = format!("{:?}", lead_info.signature);
        let _public_key = lead_info.public_key.to_string();
        let mut _public_inputs = vec![];
        for public_input in &lead_info.public_inputs {
            _public_inputs.push(format!("{:?}", public_input));
        }
        let _coin_slot = lead_info.coin_slot;
        let _coin_eta = format!("{:?}", lead_info.coin_eta);
        let _proof = format!("{:?}", lead_info.proof);
        let _leaders = lead_info.leaders;
        LeadInfoInfo {
            _signature,
            _public_key,
            _public_inputs,
            _coin_slot,
            _coin_eta,
            _proof,
            _leaders,
        }
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
struct SlotCheckpointInfo {
    _slot: u64,
    _eta: String,
    _sigma1: String,
    _sigma2: String,
}

impl SlotCheckpointInfo {
    pub fn new(_slot: u64, _eta: String, _sigma1: String, _sigma2: String) -> SlotCheckpointInfo {
        SlotCheckpointInfo { _slot, _eta, _sigma1, _sigma2 }
    }
}

#[derive(Debug)]
struct SlotCheckpointStoreInfo {
    _slot_checkpoints: Vec<SlotCheckpointInfo>,
}

impl SlotCheckpointStoreInfo {
    pub fn new(slotcheckpointstore: &SlotCheckpointStore) -> SlotCheckpointStoreInfo {
        let mut _slot_checkpoints = Vec::new();
        let result = slotcheckpointstore.get_all();
        match result {
            Ok(iter) => {
                for slot_checkpoint in iter.iter() {
                    _slot_checkpoints.push(SlotCheckpointInfo::new(
                        slot_checkpoint.slot,
                        format!("{:?}", slot_checkpoint.eta),
                        format!("{:?}", slot_checkpoint.sigma1),
                        format!("{:?}", slot_checkpoint.sigma2),
                    ));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        SlotCheckpointStoreInfo { _slot_checkpoints }
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
struct ErroneousTxStoreInfo {
    _transactions: Vec<TxInfo>,
}

impl ErroneousTxStoreInfo {
    pub fn new(erroneoustxstore: &ErroneousTxStore) -> ErroneousTxStoreInfo {
        let mut _transactions = Vec::new();
        let result = erroneoustxstore.get_all();
        match result {
            Ok(iter) => {
                for (hash, tx) in iter.iter() {
                    _transactions.push(TxInfo::new(hash.clone(), &tx));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        ErroneousTxStoreInfo { _transactions }
    }
}

#[derive(Debug)]
struct BlockchainInfo {
    _headers: HeaderStoreInfo,
    _blocks: BlockInfoChain,
    _order: BlockOrderStoreInfo,
    _slot_checkpoints: SlotCheckpointStoreInfo,
    _transactions: TxStoreInfo,
    _erroneous_txs: ErroneousTxStoreInfo,
}

impl BlockchainInfo {
    pub fn new(blockchain: &Blockchain) -> BlockchainInfo {
        let _headers = HeaderStoreInfo::new(&blockchain.headers);
        let _blocks = BlockInfoChain::new(&blockchain.blocks);
        let _order = BlockOrderStoreInfo::new(&blockchain.order);
        let _slot_checkpoints = SlotCheckpointStoreInfo::new(&blockchain.slot_checkpoints);
        let _transactions = TxStoreInfo::new(&blockchain.transactions);
        let _erroneous_txs = ErroneousTxStoreInfo::new(&blockchain.erroneous_txs);
        BlockchainInfo {
            _headers,
            _blocks,
            _order,
            _slot_checkpoints,
            _transactions,
            _erroneous_txs,
        }
    }
}

#[derive(Debug)]
struct StateInfo {
    _blockchain: BlockchainInfo,
}

impl StateInfo {
    pub fn new(state: &ValidatorState) -> StateInfo {
        let _blockchain = BlockchainInfo::new(&state.blockchain);
        StateInfo { _blockchain }
    }
}

async fn generate(localnet: &str, name: &str) -> Result<()> {
    println!("Exporting data for {name}...");

    // Node folder
    let folder = localnet.to_owned() + name;

    // Consensus configuration
    let bootstrap_ts = Timestamp(1648383795);
    let genesis_ts = Timestamp(1648383795);
    let genesis_data = *TESTNET_GENESIS_HASH_BYTES;
    let initial_distribution = 1000;
    let pass = "changeme";

    // Initialize or load wallet
    let path = folder.to_owned() + "/wallet.db";
    let wallet = init_wallet(&path, &pass).await?;

    // Initialize or load sled database
    let path = folder.to_owned() + "/blockchain/testnet";
    let db_path = expand_path(&path).unwrap();
    let sled_db = sled::open(&db_path)?;

    // Data export
    let state = ValidatorState::new(
        &sled_db,
        bootstrap_ts,
        genesis_ts,
        genesis_data,
        initial_distribution,
        wallet,
        vec![],
        false,
        false,
    )
    .await?;
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
    // Localnet folder
    let localnet = "../../../contrib/localnet/darkfid-single-node/";
    println!("Localnet folder: {localnet}");
    // darkfid0
    generate(localnet, "darkfid0").await?;
    // faucetd
    generate(localnet, "faucetd").await?;

    Ok(())
}
