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
        block_store::{
            Block, BlockDifficulty, BlockDifficultyStore, BlockOrderStore, BlockRanks, BlockStore,
        },
        contract_store::{ContractStateStore, WasmStore},
        header_store::{Header, HeaderStore},
        tx_store::{PendingTxOrderStore, PendingTxStore, TxStore},
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
    _hash: blake3::Hash,
    _version: u8,
    _previous: blake3::Hash,
    _height: u64,
    _timestamp: Timestamp,
    _nonce: u64,
    _tree: MerkleTree,
}

impl HeaderInfo {
    pub fn new(_hash: blake3::Hash, header: &Header) -> HeaderInfo {
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
    _hash: blake3::Hash,
    _header: blake3::Hash,
    _txs: Vec<blake3::Hash>,
    _signature: String,
}

impl BlockInfo {
    pub fn new(_hash: blake3::Hash, block: &Block) -> BlockInfo {
        BlockInfo {
            _hash,
            _header: block.header,
            _txs: block.txs.clone(),
            _signature: format!("{:?}", block.signature),
        }
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
                    _blocks.push(BlockInfo::new(*hash, block));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        BlockInfoChain { _blocks }
    }
}

#[derive(Debug)]
struct OrderInfo {
    _height: u64,
    _hash: blake3::Hash,
}

impl OrderInfo {
    pub fn new(_height: u64, _hash: blake3::Hash) -> OrderInfo {
        OrderInfo { _height, _hash }
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
                for (height, hash) in iter.iter() {
                    _order.push(OrderInfo::new(*height, *hash));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        BlockOrderStoreInfo { _order }
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
struct BlockDifficultyStoreInfo {
    _difficulties: Vec<BlockDifficultyInfo>,
}

impl BlockDifficultyStoreInfo {
    pub fn new(difficultiesstore: &BlockDifficultyStore) -> BlockDifficultyStoreInfo {
        let mut _difficulties = Vec::new();
        let result = difficultiesstore.get_all();
        match result {
            Ok(iter) => {
                for (_, difficulty) in iter.iter() {
                    _difficulties.push(BlockDifficultyInfo::new(difficulty));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        BlockDifficultyStoreInfo { _difficulties }
    }
}

#[derive(Debug)]
struct TxInfo {
    _hash: blake3::Hash,
    _payload: Transaction,
}

impl TxInfo {
    pub fn new(_hash: blake3::Hash, tx: &Transaction) -> TxInfo {
        TxInfo { _hash, _payload: tx.clone() }
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
                    _transactions.push(TxInfo::new(*hash, tx));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        TxStoreInfo { _transactions }
    }
}

#[derive(Debug)]
struct PendingTxStoreInfo {
    _transactions: Vec<TxInfo>,
}

impl PendingTxStoreInfo {
    pub fn new(pendingtxstore: &PendingTxStore) -> PendingTxStoreInfo {
        let mut _transactions = Vec::new();
        let result = pendingtxstore.get_all();
        match result {
            Ok(iter) => {
                for (hash, tx) in iter.iter() {
                    _transactions.push(TxInfo::new(*hash, tx));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        PendingTxStoreInfo { _transactions }
    }
}

#[derive(Debug)]
struct PendingTxOrderStoreInfo {
    _order: Vec<OrderInfo>,
}

impl PendingTxOrderStoreInfo {
    pub fn new(orderstore: &PendingTxOrderStore) -> PendingTxOrderStoreInfo {
        let mut _order = Vec::new();
        let result = orderstore.get_all();
        match result {
            Ok(iter) => {
                for (height, hash) in iter.iter() {
                    _order.push(OrderInfo::new(*height, *hash));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        PendingTxOrderStoreInfo { _order }
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
struct ContractStateStoreInfo {
    _contracts: Vec<ContractStateInfo>,
}

impl ContractStateStoreInfo {
    pub fn new(contractsstore: &ContractStateStore) -> ContractStateStoreInfo {
        let mut _contracts = Vec::new();
        let result = contractsstore.get_all();
        match result {
            Ok(iter) => {
                for (id, state_hash) in iter.iter() {
                    _contracts.push(ContractStateInfo::new(*id, state_hash));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        ContractStateStoreInfo { _contracts }
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
struct WasmStoreInfo {
    _wasm_bincodes: Vec<WasmInfo>,
}

impl WasmStoreInfo {
    pub fn new(wasmstore: &WasmStore) -> WasmStoreInfo {
        let mut _wasm_bincodes = Vec::new();
        let result = wasmstore.get_all();
        match result {
            Ok(iter) => {
                for (id, bincode) in iter.iter() {
                    _wasm_bincodes.push(WasmInfo::new(*id, bincode));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        WasmStoreInfo { _wasm_bincodes }
    }
}

#[derive(Debug)]
struct BlockchainInfo {
    _headers: HeaderStoreInfo,
    _blocks: BlockInfoChain,
    _order: BlockOrderStoreInfo,
    _difficulties: BlockDifficultyStoreInfo,
    _transactions: TxStoreInfo,
    _pending_txs: PendingTxStoreInfo,
    _pending_txs_order: PendingTxOrderStoreInfo,
    _contracts: ContractStateStoreInfo,
    _wasm_bincode: WasmStoreInfo,
}

impl BlockchainInfo {
    pub fn new(blockchain: &Blockchain) -> BlockchainInfo {
        BlockchainInfo {
            _headers: HeaderStoreInfo::new(&blockchain.headers),
            _blocks: BlockInfoChain::new(&blockchain.blocks),
            _order: BlockOrderStoreInfo::new(&blockchain.order),
            _difficulties: BlockDifficultyStoreInfo::new(&blockchain.difficulties),
            _transactions: TxStoreInfo::new(&blockchain.transactions),
            _pending_txs: PendingTxStoreInfo::new(&blockchain.pending_txs),
            _pending_txs_order: PendingTxOrderStoreInfo::new(&blockchain.pending_txs_order),
            _contracts: ContractStateStoreInfo::new(&blockchain.contracts),
            _wasm_bincode: WasmStoreInfo::new(&blockchain.wasm_bincode),
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
