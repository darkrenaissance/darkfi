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
    io::{stdin, Cursor, Read},
    process::exit,
    str::FromStr,
};

use clap::{Parser, Subcommand};
use darkfi::{
    blockchain::{BlockInfo, Blockchain, BlockchainOverlay},
    cli_desc,
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    util::{encoding::base64, parse::decode_base10, path::expand_path, time::Timestamp},
    validator::{utils::deploy_native_contracts, verification::verify_genesis_block},
    zk::{empty_witnesses, halo2::Field, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_contract_test_harness::vks;
use darkfi_money_contract::{
    client::genesis_mint_v1::GenesisMintCallBuilder, MoneyFunction, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{contract_id::MONEY_CONTRACT_ID, FuncId, Keypair, SecretKey},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize_async, serialize_async, AsyncEncodable};

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

    /// Generate a Darkfi genesis transaction using the secret
    /// key from  stdin
    GenerateTx {
        /// Amount to mint for this genesis transaction
        amount: String,
    },
}

/// Auxiliary function to read a bs58 genesis block from stdin
async fn read_block() -> Result<BlockInfo> {
    println!("Reading genesis block from stdin...");
    let mut buf = String::new();
    stdin().read_to_string(&mut buf)?;
    let bytes = base64::decode(buf.trim()).unwrap();
    let block = deserialize_async(&bytes).await?;
    Ok(block)
}

#[async_std::main]
async fn main() -> Result<()> {
    // Parse arguments
    let args = Args::parse();

    // Execute a subcommand
    match args.command {
        Subcmd::Display => {
            let genesis_block = read_block().await;
            println!("{genesis_block:#?}");
        }

        Subcmd::Generate { txs_folder, genesis_timestamp } => {
            // Grab genesis transactions from folder
            let txs_folder = expand_path(&txs_folder).unwrap();
            let mut genesis_txs: Vec<Transaction> = vec![];
            for file in read_dir(txs_folder)? {
                let file = file?;
                let bytes = base64::decode(read_to_string(file.path())?.trim()).unwrap();
                let tx = deserialize_async(&bytes).await?;
                genesis_txs.push(tx);
            }

            // Generate the genesis block
            let mut genesis_block = BlockInfo::default();

            // Update timestamp if one was provided
            if let Some(timestamp) = genesis_timestamp {
                genesis_block.header.timestamp = Timestamp::from_u64(timestamp);
            }

            // Retrieve genesis producer transaction
            let producer_tx = genesis_block.txs.pop().unwrap();

            // Append genesis transactions
            if !genesis_txs.is_empty() {
                genesis_block.append_txs(genesis_txs);
            }
            genesis_block.append_txs(vec![producer_tx]);

            // Write generated genesis block to stdin
            let encoded = base64::encode(&serialize_async(&genesis_block).await);
            println!("{encoded}");
        }

        Subcmd::Verify => {
            let genesis_block = read_block().await?;
            let hash = genesis_block.hash();

            println!("Verifying genesis block: {hash}");

            // Initialize a temporary sled database
            let sled_db = sled::Config::new().temporary(true).open()?;
            let (_, vks) = vks::get_cached_pks_and_vks()?;
            vks::inject(&sled_db, &vks)?;

            // Create an overlay over whole blockchain
            let blockchain = Blockchain::new(&sled_db)?;
            let overlay = BlockchainOverlay::new(&blockchain)?;
            deploy_native_contracts(&overlay).await?;

            verify_genesis_block(&overlay, &genesis_block).await?;

            println!("Genesis block {hash} verified successfully!");
        }

        Subcmd::GenerateTx { amount } => {
            let mut buf = String::new();
            stdin().read_to_string(&mut buf)?;
            let Ok(bytes) = bs58::decode(&buf.trim()).into_vec() else {
                eprintln!("Error: Failed to decode stdin buffer");
                exit(2);
            };
            let secret = deserialize_async::<SecretKey>(&bytes).await?;
            let keypair = Keypair::new(secret);

            if let Err(e) = f64::from_str(&amount) {
                eprintln!("Invalid amount: {e:?}");
                exit(2);
            }
            let amount = decode_base10(&amount, 8, false)?;

            // Grab mint proving keys and zkbin
            let (pks, _) = vks::get_cached_pks_and_vks()?;
            let mut mint = None;
            for (bincode, namespace, pk) in pks {
                if namespace.as_str() != MONEY_CONTRACT_ZKAS_MINT_NS_V1 {
                    continue
                }
                let mut reader = Cursor::new(pk);
                let zkbin = ZkBinary::decode(&bincode)?;
                let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
                let proving_key = ProvingKey::read(&mut reader, circuit)?;
                mint = Some((proving_key, zkbin));
            }
            let Some((mint_pk, mint_zkbin)) = mint else {
                eprintln!("Mint proving keys not found.");
                exit(2);
            };

            // Build the contract call
            let builder = GenesisMintCallBuilder {
                keypair,
                amount,
                spend_hook: FuncId::none(),
                user_data: pallas::Base::ZERO,
                mint_zkbin,
                mint_pk,
            };

            let debris = builder.build()?;

            // Encode and build the transaction
            let mut data = vec![MoneyFunction::GenesisMintV1 as u8];
            debris.params.encode_async(&mut data).await?;
            let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };
            let mut tx_builder =
                TransactionBuilder::new(ContractCallLeaf { call, proofs: debris.proofs }, vec![])?;
            let mut tx = tx_builder.build()?;
            let sigs = tx.create_sigs(&[keypair.secret])?;
            tx.signatures = vec![sigs];

            println!("{}", base64::encode(&serialize_async(&tx).await));
        }
    }

    Ok(())
}
