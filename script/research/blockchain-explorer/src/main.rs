/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use clap::Parser;
use darkfi::{
    blockchain::{
        block_store::{Block, BlockDifficulty, BlockRanks, BlockStore},
        contract_store::ContractStore,
        header_store::{Header, HeaderHash, HeaderStore},
        tx_store::TxStore,
        Blockchain,
    },
    cli_desc,
    tx::Transaction,
    util::{path::expand_path, time::Timestamp},
    Result,
};
use darkfi_sdk::{
    blockchain::block_epoch,
    crypto::{ContractId, MerkleTree},
    tx::TransactionHash,
};
use num_bigint::BigUint;

#[derive(Parser)]
#[command(about = cli_desc!())]
struct Args {
    #[arg(short, long, default_value = "../../../contrib/localnet/darkfid-single-node/")]
    /// Path containing the node folders
    path: String,

    #[arg(short, long, default_values = ["darkfid"])]
    /// Node folder name (supports multiple values)
    node: Vec<String>,

    #[arg(short, long, default_value = "")]
    /// Node blockchain folder
    blockchain: String,

    #[arg(short, long)]
    /// Export all contents into a JSON file
    export: bool,
}

#[derive(Debug)]
struct HeaderInfo {
    _hash: HeaderHash,
    _version: u8,
    _previous: HeaderHash,
    _height: u64,
    _timestamp: Timestamp,
    _nonce: u64,
    _tree: MerkleTree,
}

impl HeaderInfo {
    pub fn new(_hash: HeaderHash, header: &Header) -> HeaderInfo {
        HeaderInfo {
            _hash,
            _version: header.version,
            _previous: header.previous,
            _height: header.height,
            _timestamp: header.timestamp,
            _nonce: header.nonce,
            _tree: header.tree.clone(),
        }
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
                    _headers.push(HeaderInfo::new(*hash, header));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        HeaderStoreInfo { _headers }
    }
}

#[derive(Debug)]
struct BlockInfo {
    _hash: HeaderHash,
    _header: HeaderHash,
    _txs: Vec<TransactionHash>,
    _signature: String,
}

impl BlockInfo {
    pub fn new(_hash: HeaderHash, block: &Block) -> BlockInfo {
        BlockInfo {
            _hash,
            _header: block.header,
            _txs: block.txs.clone(),
            _signature: format!("{:?}", block.signature),
        }
    }
}

#[derive(Debug)]
struct OrderInfo {
    _height: u64,
    _hash: HeaderHash,
}

impl OrderInfo {
    pub fn new(_height: u64, _hash: HeaderHash) -> OrderInfo {
        OrderInfo { _height, _hash }
    }
}

#[derive(Debug)]
struct BlockRanksInfo {
    _target_rank: BigUint,
    _targets_rank: BigUint,
    _hash_rank: BigUint,
    _hashes_rank: BigUint,
}

impl BlockRanksInfo {
    pub fn new(ranks: &BlockRanks) -> BlockRanksInfo {
        BlockRanksInfo {
            _target_rank: ranks.target_rank.clone(),
            _targets_rank: ranks.targets_rank.clone(),
            _hash_rank: ranks.hash_rank.clone(),
            _hashes_rank: ranks.hashes_rank.clone(),
        }
    }
}

#[derive(Debug)]
struct BlockDifficultyInfo {
    _height: u64,
    _timestamp: Timestamp,
    _difficulty: BigUint,
    _cummulative_difficulty: BigUint,
    _ranks: BlockRanksInfo,
}

impl BlockDifficultyInfo {
    pub fn new(difficulty: &BlockDifficulty) -> BlockDifficultyInfo {
        BlockDifficultyInfo {
            _height: difficulty.height,
            _timestamp: difficulty.timestamp,
            _difficulty: difficulty.difficulty.clone(),
            _cummulative_difficulty: difficulty.cummulative_difficulty.clone(),
            _ranks: BlockRanksInfo::new(&difficulty.ranks),
        }
    }
}

#[derive(Debug)]
struct BlockStoreInfo {
    _main: Vec<BlockInfo>,
    _order: Vec<OrderInfo>,
    _difficulty: Vec<BlockDifficultyInfo>,
}

impl BlockStoreInfo {
    pub fn new(blockstore: &BlockStore) -> BlockStoreInfo {
        let mut _main = Vec::new();
        let result = blockstore.get_all();
        match result {
            Ok(iter) => {
                for (hash, block) in iter.iter() {
                    _main.push(BlockInfo::new(*hash, block));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        let mut _order = Vec::new();
        let result = blockstore.get_all_order();
        match result {
            Ok(iter) => {
                for (height, hash) in iter.iter() {
                    _order.push(OrderInfo::new(*height, *hash));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        let mut _difficulty = Vec::new();
        let result = blockstore.get_all_difficulty();
        match result {
            Ok(iter) => {
                for (_, difficulty) in iter.iter() {
                    _difficulty.push(BlockDifficultyInfo::new(difficulty));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        BlockStoreInfo { _main, _order, _difficulty }
    }
}

#[derive(Debug)]
struct TxInfo {
    _hash: TransactionHash,
    _payload: Transaction,
}

impl TxInfo {
    pub fn new(_hash: TransactionHash, tx: &Transaction) -> TxInfo {
        TxInfo { _hash, _payload: tx.clone() }
    }
}

#[derive(Debug)]
struct TxLocationInfo {
    _hash: TransactionHash,
    _block_height: u64,
    _index: u64,
}

impl TxLocationInfo {
    pub fn new(_hash: TransactionHash, _block_height: u64, _index: u64) -> TxLocationInfo {
        TxLocationInfo { _hash, _block_height, _index }
    }
}

#[derive(Debug)]
struct PendingOrderInfo {
    _order: u64,
    _hash: TransactionHash,
}

impl PendingOrderInfo {
    pub fn new(_order: u64, _hash: TransactionHash) -> PendingOrderInfo {
        PendingOrderInfo { _order, _hash }
    }
}

#[derive(Debug)]
struct TxStoreInfo {
    _main: Vec<TxInfo>,
    _location: Vec<TxLocationInfo>,
    _pending: Vec<TxInfo>,
    _pending_order: Vec<PendingOrderInfo>,
}

impl TxStoreInfo {
    pub fn new(txstore: &TxStore) -> TxStoreInfo {
        let mut _main = Vec::new();
        let result = txstore.get_all();
        match result {
            Ok(iter) => {
                for (hash, tx) in iter.iter() {
                    _main.push(TxInfo::new(*hash, tx));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        let mut _location = Vec::new();
        let result = txstore.get_all_location();
        match result {
            Ok(iter) => {
                for (hash, location) in iter.iter() {
                    _location.push(TxLocationInfo::new(*hash, location.0, location.1));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        let mut _pending = Vec::new();
        let result = txstore.get_all_pending();
        match result {
            Ok(iter) => {
                for (hash, tx) in iter.iter() {
                    _pending.push(TxInfo::new(*hash, tx));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        let mut _pending_order = Vec::new();
        let result = txstore.get_all_pending_order();
        match result {
            Ok(iter) => {
                for (order, hash) in iter.iter() {
                    _pending_order.push(PendingOrderInfo::new(*order, *hash));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        TxStoreInfo { _main, _location, _pending, _pending_order }
    }
}

#[derive(Debug)]
struct ContractStateInfo {
    _id: ContractId,
    _state_hashes: Vec<blake3::Hash>,
}

impl ContractStateInfo {
    pub fn new(_id: ContractId, state_hashes: &[blake3::Hash]) -> ContractStateInfo {
        ContractStateInfo { _id, _state_hashes: state_hashes.to_vec() }
    }
}

#[derive(Debug)]
struct WasmInfo {
    _id: ContractId,
    _bincode_hash: blake3::Hash,
}

impl WasmInfo {
    pub fn new(_id: ContractId, bincode: &[u8]) -> WasmInfo {
        let _bincode_hash = blake3::hash(bincode);
        WasmInfo { _id, _bincode_hash }
    }
}

#[derive(Debug)]
struct ContractStoreInfo {
    _state: Vec<ContractStateInfo>,
    _wasm: Vec<WasmInfo>,
}

impl ContractStoreInfo {
    pub fn new(contractsstore: &ContractStore) -> ContractStoreInfo {
        let mut _state = Vec::new();
        let result = contractsstore.get_all_states();
        match result {
            Ok(iter) => {
                for (id, state_hash) in iter.iter() {
                    _state.push(ContractStateInfo::new(*id, state_hash));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        let mut _wasm = Vec::new();
        let result = contractsstore.get_all_wasm();
        match result {
            Ok(iter) => {
                for (id, bincode) in iter.iter() {
                    _wasm.push(WasmInfo::new(*id, bincode));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        ContractStoreInfo { _state, _wasm }
    }
}
#[derive(Debug)]
struct BlockchainInfo {
    _headers: HeaderStoreInfo,
    _blocks: BlockStoreInfo,
    _transactions: TxStoreInfo,
    _contracts: ContractStoreInfo,
}

impl BlockchainInfo {
    pub fn new(blockchain: &Blockchain) -> BlockchainInfo {
        BlockchainInfo {
            _headers: HeaderStoreInfo::new(&blockchain.headers),
            _blocks: BlockStoreInfo::new(&blockchain.blocks),
            _transactions: TxStoreInfo::new(&blockchain.transactions),
            _contracts: ContractStoreInfo::new(&blockchain.contracts),
        }
    }
}

fn statistics(folder: &str, node: &str, blockchain: &str) -> Result<()> {
    println!("Retrieving blockchain statistics for {node}...");

    // Node folder
    let folder = folder.to_owned() + node;

    // Initialize or load sled database
    let path = folder.to_owned() + blockchain;
    let db_path = expand_path(&path).unwrap();
    let sled_db = sled::open(db_path)?;

    // Retrieve statistics
    let blockchain = Blockchain::new(&sled_db)?;
    let (height, block) = blockchain.last()?;
    let epoch = block_epoch(height);
    let blocks = blockchain.len();
    let txs = blockchain.txs_len();
    drop(sled_db);

    // Print statistics
    println!("Latest height: {height}");
    println!("Epoch: {epoch}");
    println!("Latest block: {block}");
    println!("Total blocks: {blocks}");
    println!("Total transactions: {txs}");

    Ok(())
}

fn export(folder: &str, node: &str, blockchain: &str) -> Result<()> {
    println!("Exporting data for {node}...");

    // Node folder
    let folder = folder.to_owned() + node;

    // Initialize or load sled database
    let path = folder.to_owned() + blockchain;
    let db_path = expand_path(&path).unwrap();
    let sled_db = sled::open(db_path)?;

    // Data export
    let blockchain = Blockchain::new(&sled_db)?;
    let info = BlockchainInfo::new(&blockchain);
    let info_string = format!("{:#?}", info);
    let file_name = node.to_owned() + "_db";
    let mut file = File::create(file_name.clone())?;
    file.write_all(info_string.as_bytes())?;
    drop(sled_db);
    println!("Data exported to file: {file_name}");

    Ok(())
}

fn main() -> Result<()> {
    // Parse arguments
    let args = Args::parse();
    println!("Node folder path: {}", args.path);
    // Export data for each node
    for node in args.node {
        if args.export {
            export(&args.path, &node, &args.blockchain)?;
            continue
        }
        statistics(&args.path, &node, &args.blockchain)?;
    }

    Ok(())
}
