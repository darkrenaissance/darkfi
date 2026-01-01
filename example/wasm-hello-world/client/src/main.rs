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

use std::{collections::BTreeMap, str::FromStr, sync::Arc};

use clap::{Parser, Subcommand};
use darkfi::{
    cli_desc,
    rpc::{client::RpcClient, jsonrpc::JsonRequest, util::JsonValue},
    tx::{ContractCallLeaf, TransactionBuilder},
    util::encoding::base64,
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_sdk::{
    crypto::{ContractId, Keypair, PublicKey, SecretKey},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};
use smol::Executor;
use url::Url;

use wasm_hello_world::{
    ContractFunction, HELLO_CONTRACT_MEMBER_TREE, HELLO_CONTRACT_ZKAS_SECRETCOMMIT_NS,
};

mod commitment;
use commitment::ContractCallBuilder;

#[derive(Parser)]
#[command(about = cli_desc!())]
struct Args {
    #[arg(short, long)]
    /// Deployed Contract ID
    contract_id: String,

    #[arg(short, long, default_value = "tcp://127.0.0.1:8340")]
    /// darkfid JSON-RPC endpoint
    endpoint: Url,

    #[command(subcommand)]
    command: Subcmd,
}

#[derive(Subcommand)]
enum Subcmd {
    /// Display current members
    List {
        /// Specific member to check if present (optional)
        member: Option<String>,
    },

    /// Generate a transaction adding a new member
    Register {
        /// To be added member secret key
        member_secret: String,
    },

    /// Generate a transaction removing a member
    Deregister {
        /// To be removed member secret key
        member_secret: String,
    },
}

fn main() -> Result<()> {
    // Parse arguments
    let args = Args::parse();
    let contract_id = match ContractId::from_str(&args.contract_id) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Invalid contract id: {e}");
            return Err(Error::ParseFailed("Invalid contract id"));
        }
    };

    // Initialize an executor
    let executor = Arc::new(Executor::new());
    smol::block_on(executor.run(async {
        // Initialize an rpc client
        let rpc_client = RpcClient::new(args.endpoint, executor.clone()).await?;

        // Execute a subcommand
        match args.command {
            Subcmd::List { member } => {
                match member {
                    // Check if specific member exists in our contract members tree
                    Some(member) => {
                        // Parse the member public key
                        let member = PublicKey::from_str(&member)?;

                        // Create the request params
                        let params = JsonValue::Array(vec![
                            JsonValue::String(contract_id.to_string()),
                            JsonValue::String(HELLO_CONTRACT_MEMBER_TREE.to_string()),
                            JsonValue::String(member.to_string()),
                        ]);

                        // Execute the request
                        let req = JsonRequest::new("blockchain.get_contract_state_key", params);
                        let rep = rpc_client.request(req).await?;

                        // Parse response
                        let bytes = base64::decode(rep.get::<String>().unwrap()).unwrap();

                        // Print info message
                        println!("Member {member} was found!");
                        println!("Value validity check: {}", bytes.is_empty());
                    }
                    // Retrieve all contract members tree records
                    None => {
                        // Create the request params
                        let params = JsonValue::Array(vec![
                            JsonValue::String(contract_id.to_string()),
                            JsonValue::String(HELLO_CONTRACT_MEMBER_TREE.to_string()),
                        ]);

                        // Execute the request
                        let req = JsonRequest::new("blockchain.get_contract_state", params);
                        let rep = rpc_client.request(req).await?;

                        // Parse response
                        let bytes = base64::decode(rep.get::<String>().unwrap()).unwrap();
                        let members: BTreeMap<Vec<u8>, Vec<u8>> = deserialize(&bytes)?;

                        // Print records
                        println!("{contract_id} members:");
                        if members.is_empty() {
                            println!("No members found");
                        } else {
                            let mut index = 1;
                            for member in members.keys() {
                                let member: pallas::Base = deserialize(member)?;
                                println!("{index}. {member:?}");
                                index += 1;
                            }
                        }
                    }
                }
            }

            Subcmd::Register { member_secret } => {
                // Parse the member secret key
                let member_secret = SecretKey::from_str(&member_secret)?;
                let member = Keypair::new(member_secret);

                // Now we need to do a lookup for the zkas proof bincodes, and create
                // the circuit objects and proving keys so we can build the transaction.
                // We also do this through the RPC.
                let params = JsonValue::Array(vec![JsonValue::String(contract_id.to_string())]);

                // Execute the request
                let req = JsonRequest::new("blockchain.lookup_zkas", params);
                let rep = rpc_client.request(req).await?;
                let params = rep.get::<Vec<JsonValue>>().unwrap();

                // Parse response
                let mut zkas_bins = Vec::with_capacity(params.len());
                for param in params {
                    let zkas_ns = param[0].get::<String>().unwrap().clone();
                    let zkas_bincode_bytes =
                        base64::decode(param[1].get::<String>().unwrap()).unwrap();
                    zkas_bins.push((zkas_ns, zkas_bincode_bytes));
                }

                let Some(commitment_zkbin) =
                    zkas_bins.iter().find(|x| x.0 == HELLO_CONTRACT_ZKAS_SECRETCOMMIT_NS)
                else {
                    return Err(Error::Custom("Secret commitment circuit not found".to_string()))
                };
                let commitment_zkbin = ZkBinary::decode(&commitment_zkbin.1)?;
                let commitment_circuit =
                    ZkCircuit::new(empty_witnesses(&commitment_zkbin)?, &commitment_zkbin);

                // Creating secret commitment circuit proving keys
                let commitment_pk = ProvingKey::build(commitment_zkbin.k, &commitment_circuit);

                // Create the contract call
                let builder = ContractCallBuilder { member, commitment_zkbin, commitment_pk };
                let debris = builder.build()?;

                // Encode the call
                let mut data = vec![ContractFunction::Register as u8];
                debris.params.encode(&mut data)?;
                let call = ContractCall { contract_id, data };

                // Create the TransactionBuilder containing above call
                let mut tx_builder = TransactionBuilder::new(
                    ContractCallLeaf { call, proofs: debris.proofs },
                    vec![],
                )?;

                // Build the transaction and attach the corresponding signatures
                let mut tx = tx_builder.build()?;
                let sigs = tx.create_sigs(&[])?;
                tx.signatures.push(sigs);

                println!("{}", base64::encode(&serialize(&tx)));
            }

            Subcmd::Deregister { member_secret } => {
                // Parse the member secret key
                let member_secret = SecretKey::from_str(&member_secret)?;
                let member = Keypair::new(member_secret);

                // Now we need to do a lookup for the zkas proof bincodes, and create
                // the circuit objects and proving keys so we can build the transaction.
                // We also do this through the RPC.
                let params = JsonValue::Array(vec![JsonValue::String(contract_id.to_string())]);

                // Execute the request
                let req = JsonRequest::new("blockchain.lookup_zkas", params);
                let rep = rpc_client.request(req).await?;
                let params = rep.get::<Vec<JsonValue>>().unwrap();

                // Parse response
                let mut zkas_bins = Vec::with_capacity(params.len());
                for param in params {
                    let zkas_ns = param[0].get::<String>().unwrap().clone();
                    let zkas_bincode_bytes =
                        base64::decode(param[1].get::<String>().unwrap()).unwrap();
                    zkas_bins.push((zkas_ns, zkas_bincode_bytes));
                }

                let Some(commitment_zkbin) =
                    zkas_bins.iter().find(|x| x.0 == HELLO_CONTRACT_ZKAS_SECRETCOMMIT_NS)
                else {
                    return Err(Error::Custom("Secret commitment circuit not found".to_string()))
                };
                let commitment_zkbin = ZkBinary::decode(&commitment_zkbin.1)?;
                let commitment_circuit =
                    ZkCircuit::new(empty_witnesses(&commitment_zkbin)?, &commitment_zkbin);

                // Creating secret commitment circuit proving keys
                let commitment_pk = ProvingKey::build(commitment_zkbin.k, &commitment_circuit);

                // Create the contract call
                let builder = ContractCallBuilder { member, commitment_zkbin, commitment_pk };
                let debris = builder.build()?;

                // Encode the call
                let mut data = vec![ContractFunction::Deregister as u8];
                debris.params.encode(&mut data)?;
                let call = ContractCall { contract_id, data };

                // Create the TransactionBuilder containing above call
                let mut tx_builder = TransactionBuilder::new(
                    ContractCallLeaf { call, proofs: debris.proofs },
                    vec![],
                )?;

                // Build the transaction and attach the corresponding signatures
                let mut tx = tx_builder.build()?;
                let sigs = tx.create_sigs(&[])?;
                tx.signatures.push(sigs);

                println!("{}", base64::encode(&serialize(&tx)));
            }
        }

        // Stop the rpc client
        rpc_client.stop().await;

        Ok(())
    }))
}
