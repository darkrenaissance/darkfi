/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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
    fs::{create_dir, read_dir, read_to_string},
    io::{stdin, Cursor, Read},
    process::exit,
    str::FromStr,
    sync::Arc,
};

use clap::{Parser, Subcommand};
use darkfi::{
    blockchain::{block_store::append_tx_to_merkle_tree, BlockInfo, Blockchain, BlockchainOverlay},
    cli_desc,
    tx::{ContractCallLeaf, TransactionBuilder},
    util::{encoding::base64, parse::decode_base10, path::expand_path, time::Timestamp},
    validator::{
        utils::deploy_native_contracts,
        verification::{apply_transaction, verify_genesis_block},
    },
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_contract_test_harness::vks;
use darkfi_money_contract::{
    client::genesis_mint_v1::GenesisMintCallBuilder, MoneyFunction, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{contract_id::MONEY_CONTRACT_ID, FuncId, MerkleTree, PublicKey, SecretKey},
    pasta::{group::ff::PrimeField, pallas},
    ContractCall,
};
use darkfi_serial::{deserialize_async, serialize_async, AsyncEncodable};
use sled_overlay::sled;
use smol::Executor;

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

        #[arg(short, long, default_value = "120")]
        /// Configured PoW target
        pow_target: u32,
    },

    /// Read a Darkfi genesis block from stdin and verify it
    Verify,

    /// Generate a Darkfi genesis transaction using the secret
    /// key from  stdin
    GenerateTx {
        /// Amounts to mint for this genesis transaction
        amounts: Vec<String>,

        #[arg(short, long)]
        /// Optional recipient's public key, in case we want to mint to a different address
        recipient: Option<String>,

        #[arg(short, long)]
        /// Optional contract spend hook to use
        spend_hook: Option<String>,

        #[arg(short, long)]
        /// Optional user data to use
        user_data: Option<String>,
    },
}

/// Auxiliary function to read a base64 genesis block from stdin
async fn read_block() -> Result<BlockInfo> {
    println!("Reading genesis block from stdin...");
    let mut buf = String::new();
    stdin().read_to_string(&mut buf)?;
    let bytes = base64::decode(buf.trim()).unwrap();
    let block = deserialize_async(&bytes).await?;
    Ok(block)
}

fn main() -> Result<()> {
    // Initialize an executor
    let executor = Arc::new(Executor::new());
    smol::block_on(executor.run(async {
        // Parse arguments
        let args = Args::parse();

        // Execute a subcommand
        match args.command {
            Subcmd::Display => {
                let genesis_block = read_block().await;
                // TODO: display in more details
                println!("{genesis_block:?}");
            }

            Subcmd::Generate { txs_folder, genesis_timestamp, pow_target } => {
                // Generate the genesis block
                let mut genesis_block = BlockInfo::default();

                // Retrieve genesis producer transaction
                let producer_tx = genesis_block.txs.pop().unwrap();

                // Initialize a temporary sled database
                let sled_db = sled::Config::new().temporary(true).open()?;
                let (_, vks) = vks::get_cached_pks_and_vks()?;
                vks::inject(&sled_db, &vks)?;

                // Create an overlay over whole blockchain
                let blockchain = Blockchain::new(&sled_db)?;
                let overlay = BlockchainOverlay::new(&blockchain)?;
                deploy_native_contracts(&overlay, 0).await?;

                // Grab genesis transactions from folder
                let txs_folder = expand_path(&txs_folder).unwrap();
                if !txs_folder.exists() {
                    create_dir(&txs_folder)?;
                }
                let mut tree = MerkleTree::new(1);
                for file in read_dir(txs_folder)? {
                    let file = file?;
                    let bytes = base64::decode(read_to_string(file.path())?.trim()).unwrap();
                    let tx = deserialize_async(&bytes).await?;
                    apply_transaction(&overlay, 0, pow_target, &tx, &mut tree).await?;
                    genesis_block.txs.push(tx);
                }

                // Update timestamp if one was provided
                if let Some(timestamp) = genesis_timestamp {
                    genesis_block.header.timestamp = Timestamp::from_u64(timestamp);
                }

                // Append producer tx
                append_tx_to_merkle_tree(&mut tree, &producer_tx);
                genesis_block.txs.push(producer_tx);

                // Update the transactions root
                genesis_block.header.transactions_root = tree.root(0).unwrap();

                // Grab the updated contracts states root
                let state_monotree = overlay.lock().unwrap().get_state_monotree()?;
                let Some(state_root) = state_monotree.get_headroot()? else {
                    return Err(Error::ContractsStatesRootNotFoundError);
                };
                genesis_block.header.state_root = state_root;

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
                deploy_native_contracts(&overlay, 0).await?;

                verify_genesis_block(&overlay, &genesis_block, 0).await?;

                println!("Genesis block {hash} verified successfully!");
            }

            Subcmd::GenerateTx { amounts, recipient, spend_hook, user_data } => {
                let mut buf = String::new();
                stdin().read_to_string(&mut buf)?;
                let signature_secret = SecretKey::from_str(buf.trim())?;

                let mut coin_amounts = vec![];
                for amount in amounts {
                    if let Err(e) = f64::from_str(&amount) {
                        eprintln!("Invalid amount: {e:?}");
                        exit(2);
                    }
                    coin_amounts.push(decode_base10(&amount, 8, true)?);
                }

                let recipient = match recipient {
                    Some(r) => match PublicKey::from_str(&r) {
                        Ok(r) => Some(r),
                        Err(e) => {
                            eprintln!("Invalid recipient: {e:?}");
                            exit(2);
                        }
                    },
                    None => None,
                };

                let spend_hook = match spend_hook {
                    Some(s) => match FuncId::from_str(&s) {
                        Ok(s) => Some(s),
                        Err(e) => {
                            eprintln!("Invalid spend hook: {e:?}");
                            exit(2);
                        }
                    },
                    None => None,
                };

                let user_data = match user_data {
                    Some(u) => {
                        let bytes: [u8; 32] = match bs58::decode(&u).into_vec()?.try_into() {
                            Ok(b) => b,
                            Err(e) => {
                                eprintln!("Invalid user data: {e:?}");
                                exit(2);
                            }
                        };

                        match pallas::Base::from_repr(bytes).into() {
                            Some(v) => Some(v),
                            None => {
                                eprintln!("Invalid user data");
                                exit(2);
                            }
                        }
                    }
                    None => None,
                };

                // Grab mint proving keys and zkbin
                let (pks, _) = vks::get_cached_pks_and_vks()?;
                let mut mint = None;
                for (bincode, namespace, pk) in pks {
                    if namespace.as_str() != MONEY_CONTRACT_ZKAS_MINT_NS_V1 {
                        continue
                    }
                    let mut reader = Cursor::new(pk);
                    let zkbin = ZkBinary::decode(&bincode, false)?;
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
                    signature_public: PublicKey::from_secret(signature_secret),
                    amounts: coin_amounts,
                    recipient,
                    spend_hook,
                    user_data,
                    mint_zkbin,
                    mint_pk,
                };

                let debris = builder.build()?;

                // Encode and build the transaction
                let mut data = vec![MoneyFunction::GenesisMintV1 as u8];
                debris.params.encode_async(&mut data).await?;
                let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };
                let mut tx_builder = TransactionBuilder::new(
                    ContractCallLeaf { call, proofs: debris.proofs },
                    vec![],
                )?;
                let mut tx = tx_builder.build()?;
                let sigs = tx.create_sigs(&[signature_secret])?;
                tx.signatures = vec![sigs];

                println!("{}", base64::encode(&serialize_async(&tx).await));
            }
        }

        Ok(())
    }))
}
