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

use std::{
    fs::{read_dir, read_to_string},
    io::{stdin, Read},
};

use anyhow::Result;
use clap::{Parser, Subcommand};
use darkfi::{
    blockchain::{BlockInfo, Blockchain, BlockchainOverlay},
    cli_desc,
    tx::Transaction,
    util::{encoding::base64, path::expand_path, time::Timestamp},
    validator::verification::verify_genesis_block,
};
use darkfi_contract_test_harness::vks;
use darkfi_serial::{deserialize, serialize};

#[derive(Parser)]
#[command(about = cli_desc!())]
struct Args {
    #[command(subcommand)]
    command: Subcmd,
}

#[derive(Subcommand)]
enum Subcmd {
    /// Read a Darkfi genesis block from stdin and display it
    Display,

    /// Generate a Darkfi genesis block and write it to stdin
    Generate {
        #[arg(short, long, default_value = "genesis_txs")]
        /// Path to folder containing the genesis transactions
        txs_folder: String,

        #[arg(short, long)]
        /// Genesis timestamp to use, instead of current one
        genesis_timestamp: Option<u64>,
    },

    /// Read a Darkfi genesis block from stdin and verify it
    Verify,
}

/// Auxiliary function to read a bs58 genesis block from stdin
fn read_block() -> Result<BlockInfo> {
    eprintln!("Reading genesis block from stdin...");
    let mut buf = String::new();
    stdin().read_to_string(&mut buf)?;
    let bytes = base64::decode(buf.trim()).unwrap();
    let block = deserialize(&bytes)?;
    Ok(block)
}

#[async_std::main]
async fn main() -> Result<()> {
    // Parse arguments
    let args = Args::parse();

    // Execute a subcommand
    match args.command {
        Subcmd::Display => {
            let genesis_block = read_block()?;
            println!("{genesis_block:#?}");
            Ok(())
        }

        Subcmd::Generate { txs_folder, genesis_timestamp } => {
            // Grab genesis transactions from folder
            let txs_folder = expand_path(&txs_folder).unwrap();
            let mut genesis_txs: Vec<Transaction> = vec![];
            for file in read_dir(txs_folder)? {
                let bytes = base64::decode(read_to_string(file?.path())?.trim()).unwrap();
                let tx = deserialize(&bytes)?;
                genesis_txs.push(tx);
            }

            // Generate the genesis block
            let mut genesis_block = BlockInfo::default();

            // Update timestamp if one was provided
            if let Some(timestamp) = genesis_timestamp {
                genesis_block.header.timestamp = Timestamp(timestamp);
            }

            // Retrieve genesis producer transaction
            let producer_tx = genesis_block.txs.pop().unwrap();

            // Append genesis transactions
            if !genesis_txs.is_empty() {
                genesis_block.append_txs(genesis_txs)?;
            }
            genesis_block.append_txs(vec![producer_tx])?;

            // Write generated genesis block to stdin
            let encoded = base64::encode(&serialize(&genesis_block));
            println!("{encoded}");

            Ok(())
        }

        Subcmd::Verify => {
            let genesis_block = read_block()?;
            let hash = genesis_block.hash()?;

            println!("Verifying genesis block: {hash}");

            // Initialize a temporary sled database
            let sled_db = sled::Config::new().temporary(true).open()?;
            let (_, vks) = vks::get_cached_pks_and_vks()?;
            vks::inject(&sled_db, &vks)?;

            // Create an overlay over whole blockchain
            let blockchain = Blockchain::new(&sled_db)?;
            let overlay = BlockchainOverlay::new(&blockchain)?;

            verify_genesis_block(&overlay, &genesis_block).await?;

            println!("Genesis block {hash} verified successfully!");

            Ok(())
        }
    }
}
