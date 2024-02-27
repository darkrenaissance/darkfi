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
        block_store::{Block, BlockOrderStore, BlockProducer, BlockStore},
        header_store::{Header, HeaderStore},
        slot_store::SlotStore,
        tx_store::TxStore,
        Blockchain,
    },
    cli_desc,
    consensus::constants::EPOCH_LENGTH,
    tx::Transaction,
    util::{path::expand_path, time::Timestamp},
    Result,
};
use darkfi_sdk::crypto::MerkleNode;

#[derive(Parser)]
#[command(about = cli_desc!())]
struct Args {
    #[arg(short, long, default_value = "../../../contrib/localnet/darkfid-single-node/")]
    /// Path containing the node folders
    path: String,

    #[arg(short, long, default_values = ["darkfid0", "faucetd"])]
    /// Node folder name (supports multiple values)
    node: Vec<String>,

    #[arg(short, long, default_value = "/blockchain/testnet")]
    /// Node blockchain folder
    blockchain: String,

    #[arg(short, long)]
    /// Export all contents into a JSON file
    export: bool,
}

#[derive(Debug)]
struct BlockProducerInfo {
    _signature: String,
    _proposal: Transaction,
}

impl BlockProducerInfo {
    pub fn new(producer: &BlockProducer) -> BlockProducerInfo {
        let _signature = format!("{:?}", producer.signature);
        let _proposal = producer.proposal.clone();
        BlockProducerInfo { _signature, _proposal }
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
    _producer: BlockProducerInfo,
    _slots: Vec<u64>,
}

impl BlockInfo {
    pub fn new(_hash: blake3::Hash, block: &Block) -> BlockInfo {
        let _magic = block.magic;
        let _header = block.header;
        let _txs = block.txs.clone();
        let _producer = BlockProducerInfo::new(&block.producer);
        let _slots = block.slots.clone();
        BlockInfo { _hash, _magic, _header, _txs, _producer, _slots }
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
struct SlotInfo {
    _id: u64,
    _eta: String,
    _sigma1: String,
    _sigma2: String,
}

impl SlotInfo {
    pub fn new(_id: u64, _eta: String, _sigma1: String, _sigma2: String) -> SlotInfo {
        SlotInfo { _id, _eta, _sigma1, _sigma2 }
    }
}

#[derive(Debug)]
struct SlotStoreInfo {
    _slots: Vec<SlotInfo>,
}

impl SlotStoreInfo {
    pub fn new(slot_store: &SlotStore) -> SlotStoreInfo {
        let mut _slots = Vec::new();
        let result = slot_store.get_all();
        match result {
            Ok(iter) => {
                for slot in iter.iter() {
                    _slots.push(SlotInfo::new(
                        slot.id,
                        format!("{:?}", slot.previous_eta),
                        format!("{:?}", slot.sigma1),
                        format!("{:?}", slot.sigma2),
                    ));
                }
            }
            Err(e) => println!("Error: {:?}", e),
        }
        SlotStoreInfo { _slots }
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
    _slots: SlotStoreInfo,
    _transactions: TxStoreInfo,
}

impl BlockchainInfo {
    pub fn new(blockchain: &Blockchain) -> BlockchainInfo {
        let _headers = HeaderStoreInfo::new(&blockchain.headers);
        let _blocks = BlockInfoChain::new(&blockchain.blocks);
        let _order = BlockOrderStoreInfo::new(&blockchain.order);
        let _slots = SlotStoreInfo::new(&blockchain.slots);
        let _transactions = TxStoreInfo::new(&blockchain.transactions);
        BlockchainInfo { _headers, _blocks, _order, _slots, _transactions }
    }
}

fn statistics(folder: &str, node: &str, blockchain: &str) -> Result<()> {
    println!("Retrieving blockchain statistics for {node}...");

    // Node folder
    let folder = folder.to_owned() + node;

    // Initialize or load sled database
    let path = folder.to_owned() + blockchain;
    let db_path = expand_path(&path).unwrap();
    let sled_db = sled::open(&db_path)?;

    // Retrieve statistics
    let blockchain = Blockchain::new(&sled_db)?;
    let slot = blockchain.last_slot()?.id;
    let epoch = slot / EPOCH_LENGTH as u64;
    let (_, block) = blockchain.last()?;
    let blocks = blockchain.len();
    let txs = blockchain.txs_len();
    drop(sled_db);

    // Print statistics
    println!("Latest slot: {slot}");
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
    let sled_db = sled::open(&db_path)?;

    // Data export
    let blockchain = Blockchain::new(&sled_db)?;
    let info = BlockchainInfo::new(&blockchain);
    let info_string = format!("{:#?}", info);
    let file_name = node.to_owned() + "_db";
    let mut file = File::create(file_name.clone())?;
    file.write(info_string.as_bytes())?;
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
