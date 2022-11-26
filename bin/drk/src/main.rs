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

use std::{
    io::{stdin, Read},
    process::exit,
    str::FromStr,
    time::Instant,
};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use darkfi::tx::Transaction;
use darkfi_money_contract::client::Coin;
use darkfi_sdk::{
    crypto::{PublicKey, TokenId},
    pasta::{group::ff::PrimeField, pallas},
};
use darkfi_serial::{deserialize, serialize};
use serde_json::json;
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use url::Url;

use darkfi::{
    cli_desc,
    rpc::{client::RpcClient, jsonrpc::JsonRequest},
    util::cli::{get_log_config, get_log_level},
};

/// Airdrop methods
mod rpc_airdrop;

/// Payment methods
mod rpc_transfer;

/// Blockchain methods
mod rpc_blockchain;

/// Wallet operation methods for darkfid's JSON-RPC
mod rpc_wallet;

#[derive(Parser)]
#[command(about = cli_desc!())]
struct Args {
    #[arg(short, action = clap::ArgAction::Count)]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[arg(short, long, default_value = "tcp://127.0.0.1:8340")]
    /// darkfid JSON-RPC endpoint
    endpoint: Url,

    #[command(subcommand)]
    command: Subcmd,
}

#[derive(Subcommand)]
enum Subcmd {
    /// Send a ping request to the darkfid RPC endpoint
    Ping,

    /// Wallet operations
    Wallet {
        #[arg(long)]
        /// Initialize wallet with data for Money Contract (run this first)
        initialize: bool,

        #[arg(long)]
        /// Generate a new keypair in the wallet
        keygen: bool,

        #[arg(long)]
        /// Query the wallet for known balances
        balance: bool,

        #[arg(long)]
        /// Get the default address in the wallet
        address: bool,

        #[arg(long)]
        /// Print all the secret keys from the wallet
        secrets: bool,

        #[arg(long)]
        /// Print the Merkle tree in the wallet
        tree: bool,

        #[arg(long)]
        /// Print all the coins in the wallet
        coins: bool,
    },

    /// Unspend a coin
    Unspend {
        /// base58-encoded coin to mark as unspent
        coin: String,
    },

    /// Airdrop some tokens
    Airdrop {
        /// Faucet JSON-RPC endpoint
        #[arg(short, long, default_value = "tcp://127.0.0.1:8340")]
        faucet_endpoint: Url,

        /// Amount to request from the faucet
        amount: String,

        /// Token ID to request from the faucet
        token: String,

        /// Optional address to send tokens to (defaults to main address in wallet)
        address: Option<String>,
    },

    /// Create a payment transaction
    Transfer {
        /// Amount to send
        amount: String,

        /// Token ID to send
        token: String,

        /// Recipient address
        recipient: String,
    },

    /// Inspect a transaction from stdin
    Inspect,

    /// Read a transaction from stdin and broadcast it
    Broadcast,

    /// Subscribe to incoming blocks from darkfid
    ///
    /// This subscription will listen for incoming blocks from darkfid and look
    /// through their transactions to see if there's any that interest us.
    /// With `drk` we look at transactions calling the money contract so we can
    /// find coins sent to us and fill our wallet with the necessary metadata.
    Subscribe,
}

pub struct Drk {
    pub rpc_client: RpcClient,
}

impl Drk {
    async fn ping(&self) -> Result<()> {
        let latency = Instant::now();
        let req = JsonRequest::new("ping", json!([]));
        let rep = self.rpc_client.oneshot_request(req).await?;
        let latency = latency.elapsed();
        println!("Got reply: {}", rep);
        println!("Latency: {:?}", latency);
        Ok(())
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.verbose > 0 {
        let log_level = get_log_level(args.verbose.into());
        let log_config = get_log_config();
        TermLogger::init(log_level, log_config, TerminalMode::Mixed, ColorChoice::Auto)?;
    }

    match args.command {
        Subcmd::Ping => {
            let rpc_client = RpcClient::new(args.endpoint)
                .await
                .with_context(|| "Could not connect to darkfid RPC endpoint")?;

            let drk = Drk { rpc_client };
            drk.ping().await.with_context(|| "Failed to ping darkfid RPC endpoint")?;
            Ok(())
        }

        Subcmd::Wallet { initialize, keygen, balance, address, secrets, tree, coins } => {
            if !initialize && !keygen && !balance && !address && !secrets && !tree && !coins {
                eprintln!("Error: You must use at least one flag for this subcommand");
                eprintln!("Run with \"wallet -h\" to see the subcommand usage.");
                exit(2);
            }

            let rpc_client = RpcClient::new(args.endpoint)
                .await
                .with_context(|| "Could not connect to darkfid RPC endpoint")?;

            let drk = Drk { rpc_client };

            if initialize {
                drk.wallet_initialize().await.with_context(|| "Failed to initialize wallet")?;
                return Ok(())
            }

            if keygen {
                drk.wallet_keygen().await.with_context(|| "Failed to generate keypair")?;
                return Ok(())
            }

            if balance {
                drk.wallet_balance().await.with_context(|| "Failed to fetch wallet balance")?;
                return Ok(())
            }

            if address {
                let address = drk
                    .wallet_address(0)
                    .await
                    .with_context(|| "Failed to fetch default address")?;

                println!("{}", address);

                return Ok(())
            }

            if secrets {
                let v =
                    drk.wallet_secrets().await.with_context(|| "Failed to fetch wallet secrets")?;

                drk.rpc_client.close().await?;

                for i in v {
                    println!("{}", i);
                }

                return Ok(())
            }

            if tree {
                let v = drk.wallet_tree().await.with_context(|| "Failed to fetch Merkle tree")?;
                drk.rpc_client.close().await?;

                println!("{:#?}", v);

                return Ok(())
            }

            if coins {
                let coins = drk
                    .wallet_coins(true)
                    .await
                    .with_context(|| "Failed to fetch coins from wallet")?;

                drk.rpc_client.close().await?;

                for i in coins {
                    print!("{} ", bs58::encode(i.0.coin.inner().to_repr()).into_string());
                    if i.1 {
                        println!("(spent)");
                    } else {
                        println!("(unspent)");
                    }
                }

                return Ok(())
            }

            unreachable!()
        }

        Subcmd::Unspend { coin } => {
            let bytes: [u8; 32] = bs58::decode(&coin).into_vec()?.try_into().unwrap();

            let elem: pallas::Base = match pallas::Base::from_repr(bytes).into() {
                Some(v) => v,
                None => return Err(anyhow!("Invalid coin")),
            };

            let coin = Coin::from(elem);

            let rpc_client = RpcClient::new(args.endpoint)
                .await
                .with_context(|| "Could not connect to darkfid RPC endpoint")?;

            let drk = Drk { rpc_client };
            drk.unspend_coin(&coin).await.with_context(|| "Failed to mark coin as unspent")?;

            return Ok(())
        }

        Subcmd::Airdrop { faucet_endpoint, amount, token, address } => {
            let amount = f64::from_str(&amount).with_context(|| "Invalid amount")?;
            let token_id = TokenId::try_from(token.as_str()).with_context(|| "Invalid Token ID")?;

            let rpc_client = RpcClient::new(args.endpoint)
                .await
                .with_context(|| "Could not connect to darkfid RPC endpoint")?;

            let drk = Drk { rpc_client };

            let address = match address {
                Some(v) => PublicKey::from_str(v.as_str()).with_context(|| "Invalid address")?,
                None => drk.wallet_address(0).await.with_context(|| {
                    "Failed to fetch default address, perhaps the wallet was not initialized?"
                })?,
            };

            let txid = drk
                .request_airdrop(faucet_endpoint, amount, token_id, address)
                .await
                .with_context(|| "Failed to request airdrop")?;

            println!("Transaction ID: {}", txid);
            Ok(())
        }

        Subcmd::Transfer { amount, token, recipient } => {
            let _ = f64::from_str(&amount).with_context(|| "Invalid amount")?;
            let token_id = TokenId::try_from(token.as_str()).with_context(|| "Invalid Token ID")?;
            let rcpt = PublicKey::from_str(&recipient).with_context(|| "Invalid recipient")?;

            let rpc_client = RpcClient::new(args.endpoint)
                .await
                .with_context(|| "Could not connect to darkfid RPC endpoint")?;

            let drk = Drk { rpc_client };

            let tx = drk
                .transfer(&amount, token_id, rcpt)
                .await
                .with_context(|| "Failed to create payment transaction")?;

            println!("{}", bs58::encode(&serialize(&tx)).into_string());

            Ok(())
        }

        Subcmd::Inspect => {
            let mut buf = String::new();
            stdin().read_to_string(&mut buf)?;

            let bytes = bs58::decode(&buf.trim()).into_vec()?;
            let tx: Transaction = deserialize(&bytes)?;

            println!("{:#?}", tx);

            Ok(())
        }

        Subcmd::Broadcast => {
            eprintln!("Reading transaction from stdin...");
            let mut buf = String::new();
            stdin().read_to_string(&mut buf)?;

            let bytes = bs58::decode(&buf.trim()).into_vec()?;
            let tx = deserialize(&bytes)?;

            let rpc_client = RpcClient::new(args.endpoint)
                .await
                .with_context(|| "Could not connect to darkfid RPC endpoint")?;

            let drk = Drk { rpc_client };

            let txid =
                drk.broadcast_tx(&tx).await.with_context(|| "Failed to broadcast transaction")?;

            eprintln!("Transaction ID: {}", txid);

            Ok(())
        }

        Subcmd::Subscribe => {
            let rpc_client = RpcClient::new(args.endpoint.clone())
                .await
                .with_context(|| "Could not connect to darkfid RPC endpoint")?;

            let drk = Drk { rpc_client };

            drk.subscribe_blocks(args.endpoint)
                .await
                .with_context(|| "Block subscription failed")?;

            Ok(())
        }
    }
}
