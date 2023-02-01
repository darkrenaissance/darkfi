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

use std::{
    io::{stdin, Read},
    process::exit,
    str::FromStr,
    time::Instant,
};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use darkfi::{tx::Transaction, util::parse::decode_base10, zk::halo2::Field};
use darkfi_money_contract::client::Coin;
use darkfi_sdk::{
    crypto::{PublicKey, SecretKey, TokenId},
    pasta::{group::ff::PrimeField, pallas},
};
use darkfi_serial::{deserialize, serialize};
use prettytable::{format, row, Table};
use rand::rngs::OsRng;
use serde_json::json;
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use url::Url;

use darkfi::{
    cli_desc,
    rpc::{client::RpcClient, jsonrpc::JsonRequest},
    util::{
        cli::{get_log_config, get_log_level},
        parse::encode_base10,
    },
};

/// Airdrop methods
mod rpc_airdrop;

/// Payment methods
mod rpc_transfer;

/// Swap methods
mod rpc_swap;
use rpc_swap::PartialSwapData;

/// DAO methods
mod rpc_dao;

/// Blockchain methods
mod rpc_blockchain;

/// CLI utility functions
mod cli_util;
use cli_util::{parse_token_pair, parse_value_pair};

/// Wallet functionality related to DAO
mod wallet_dao;
use wallet_dao::DaoParams;

/// Wallet functionality related to Money
mod wallet_money;

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
        /// Import secret keys from stdin into the wallet, separated by newlines
        import_secrets: bool,

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
        #[arg(short, long, default_value = "tls://faucetd.testnet.dark.fi:18340")]
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

        /// Mark if this is being sent to a DAO
        #[clap(long)]
        dao: bool,

        /// DAO bulla, if the tokens are being sent to a DAO
        dao_bulla: Option<String>,
    },

    /// OTC atomic swap
    #[command(subcommand, about = cli_desc!())]
    Otc(OtcSubcmd),

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

    /// DAO functionalities
    #[command(subcommand, about = cli_desc!())]
    Dao(DaoSubcmd),

    /// Scan the blockchain and parse relevant transactions
    Scan {
        #[arg(long)]
        /// Reset Merkle tree and start scanning from first slot
        reset: bool,

        #[arg(long)]
        /// List all available checkpoints
        list: bool,

        #[arg(short, long)]
        /// Reset Merkle tree to checkpoint index and start scanning
        checkpoint: Option<u64>,
    },
}

#[derive(Subcommand)]
enum OtcSubcmd {
    /// Initialize the first half of the atomic swap
    Init {
        /// Value pair to send:recv (11.55:99.42)
        #[clap(short, long)]
        value_pair: String,

        /// Token pair to send:recv (f00:b4r)
        #[clap(short, long)]
        token_pair: String,
    },

    /// Build entire swap tx given the first half from stdin
    Join,

    /// Inspect a swap half or the full swap tx from stdin
    Inspect,

    /// Sign a transaction given from stdin as the first-half
    Sign,
}

#[derive(Subcommand)]
enum DaoSubcmd {
    /// Create DAO parameters
    Create {
        /// The minimum amount of governance tokens needed to open a proposal for this DAO
        proposer_limit: String,
        /// Minimal threshold of participating total tokens needed for a proposal to pass
        quorum: String,
        /// The ratio of winning votes/total votes needed for a proposal to pass (2 decimals),
        approval_ratio: f64,
        /// DAO's governance token ID
        gov_token_id: String,
    },

    /// View DAO data from stdin
    View,

    /// Import DAO data from stdin
    Import {
        /// Named identifier for the DAO
        dao_name: String,
    },

    /// List imported DAOs (or info about a specific one)
    List {
        /// Numeric identifier for the DAO (optional)
        dao_id: Option<u64>,
    },

    /// Show the balance of a DAO
    Balance {
        /// Numeric identifier for the DAO
        dao_id: u64,
    },

    /// Mint an imported DAO on-chain
    Mint {
        /// Numeric identifier for the DAO
        dao_id: u64,
    },

    /// Create a proposal for a DAO
    Propose {
        /// Numeric identifier for the DAO
        dao_id: u64,

        /// Pubkey to send tokens to with proposal success
        recipient: String,

        /// Amount to send from DAO with proposal success
        amount: String,

        /// Token ID to send from DAO with proposal success
        token_id: String,
    },

    /// List DAO proposals
    Proposals {
        /// Numeric identifier for the DAO
        dao_id: u64,
    },

    /// View a DAO proposal data
    Proposal {
        /// Numeric identifier for the DAO
        dao_id: u64,

        /// Numeric identifier for the proposal
        proposal_id: u64,
    },

    /// Vote on a given proposal
    Vote {
        /// Numeric identifier for the DAO
        dao_id: u64,

        /// Numeric identifier for the proposal
        proposal_id: u64,

        /// Vote (0 for NO, 1 for YES)
        vote: u8,

        /// Vote weight (amount of governance tokens)
        vote_weight: String,
    },

    /// Execute a DAO proposal
    Exec {
        /// Numeric identifier for the DAO
        dao_id: u64,

        /// Numeric identifier for the proposal
        proposal_id: u64,
    },
}

pub struct Drk {
    pub rpc_client: RpcClient,
}

impl Drk {
    async fn new(endpoint: Url) -> Result<Self> {
        let rpc_client = RpcClient::new(endpoint).await?;
        Ok(Self { rpc_client })
    }

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
            let drk = Drk::new(args.endpoint).await?;
            drk.ping().await.with_context(|| "Failed to ping darkfid RPC endpoint")?;

            Ok(())
        }

        Subcmd::Wallet {
            initialize,
            keygen,
            balance,
            address,
            secrets,
            import_secrets,
            tree,
            coins,
        } => {
            if !initialize &&
                !keygen &&
                !balance &&
                !address &&
                !secrets &&
                !tree &&
                !coins &&
                !import_secrets
            {
                eprintln!("Error: You must use at least one flag for this subcommand");
                eprintln!("Run with \"wallet -h\" to see the subcommand usage.");
                exit(2);
            }

            let drk = Drk::new(args.endpoint).await?;

            if initialize {
                drk.initialize_money().await?;
                drk.initialize_dao().await?;
                return Ok(())
            }

            if keygen {
                drk.money_keygen().await.with_context(|| "Failed to generate keypair")?;
                return Ok(())
            }

            if balance {
                let balmap =
                    drk.money_balance().await.with_context(|| "Failed to fetch wallet balance")?;

                // Create a prettytable with the new data:
                let mut table = Table::new();
                table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                table.set_titles(row!["Token ID", "Balance"]);
                for (token_id, balance) in balmap.iter() {
                    // FIXME: Don't hardcode to 8 decimals
                    table.add_row(row![token_id, encode_base10(*balance, 8)]);
                }

                if table.is_empty() {
                    println!("No unspent balances found");
                } else {
                    println!("{}", table);
                }

                return Ok(())
            }

            if address {
                let address = drk
                    .wallet_address(1) // <-- TODO: Use is_default from the sql table
                    .await
                    .with_context(|| "Failed to fetch default address")?;

                println!("{}", address);

                return Ok(())
            }

            if secrets {
                let v = drk
                    .get_money_secrets()
                    .await
                    .with_context(|| "Failed to fetch wallet secrets")?;

                drk.rpc_client.close().await?;

                for i in v {
                    println!("{}", i);
                }

                return Ok(())
            }

            if import_secrets {
                let mut secrets = vec![];
                let lines = stdin().lines();
                for (i, line) in lines.enumerate() {
                    if let Ok(line) = line {
                        let bytes = bs58::decode(&line.trim()).into_vec()?;
                        let Ok(secret) = deserialize(&bytes) else {
                            eprintln!("Warning: Failed to deserialize secret on line {}", i);
                            continue
                        };
                        secrets.push(secret);
                    }
                }

                let pubkeys = drk
                    .import_money_secrets(secrets)
                    .await
                    .with_context(|| "Failed to import secret keys into wallet")?;

                drk.rpc_client.close().await?;

                for key in pubkeys {
                    println!("{}", key);
                }

                return Ok(())
            }

            if tree {
                let v =
                    drk.get_money_tree().await.with_context(|| "Failed to fetch Merkle tree")?;
                drk.rpc_client.close().await?;

                println!("{:#?}", v);

                return Ok(())
            }

            if coins {
                let coins = drk
                    .get_coins(true)
                    .await
                    .with_context(|| "Failed to fetch coins from wallet")?;

                drk.rpc_client.close().await?;

                if coins.is_empty() {
                    return Ok(())
                }

                let mut table = Table::new();
                table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                table.set_titles(row!["Coin", "Spent", "Token ID", "Value"]);
                for coin in coins {
                    table.add_row(row![
                        format!("{}", bs58::encode(&serialize(&coin.0.coin.inner())).into_string()),
                        coin.1,
                        coin.0.note.token_id,
                        format!("{} ({})", coin.0.note.value, encode_base10(coin.0.note.value, 8))
                    ]);
                }

                println!("{}", table);

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
            let drk = Drk::new(args.endpoint).await?;
            drk.unspend_coin(&coin).await.with_context(|| "Failed to mark coin as unspent")?;

            Ok(())
        }

        Subcmd::Airdrop { faucet_endpoint, amount, token, address } => {
            let amount = f64::from_str(&amount).with_context(|| "Invalid amount")?;
            let token_id = TokenId::try_from(token.as_str()).with_context(|| "Invalid Token ID")?;

            let drk = Drk::new(args.endpoint).await?;

            let address = match address {
                Some(v) => PublicKey::from_str(v.as_str()).with_context(|| "Invalid address")?,
                None => drk.wallet_address(1).await.with_context(|| {
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

        Subcmd::Transfer { amount, token, recipient, dao, dao_bulla } => {
            let _ = f64::from_str(&amount).with_context(|| "Invalid amount")?;
            let token_id = TokenId::try_from(token.as_str()).with_context(|| "Invalid Token ID")?;
            let rcpt = PublicKey::from_str(&recipient).with_context(|| "Invalid recipient")?;

            let drk = Drk::new(args.endpoint).await?;

            let tx = drk
                .transfer(&amount, token_id, rcpt, dao, dao_bulla)
                .await
                .with_context(|| "Failed to create payment transaction")?;

            println!("{}", bs58::encode(&serialize(&tx)).into_string());

            Ok(())
        }

        Subcmd::Otc(cmd) => {
            let drk = Drk::new(args.endpoint).await?;

            match cmd {
                OtcSubcmd::Init { value_pair, token_pair } => {
                    let (vp_send, vp_recv) = parse_value_pair(&value_pair)?;
                    let (tp_send, tp_recv) = parse_token_pair(&token_pair)?;

                    let half = drk
                        .init_swap(vp_send, tp_send, vp_recv, tp_recv)
                        .await
                        .with_context(|| "Failed to create swap transaction half")?;

                    println!("{}", bs58::encode(&serialize(&half)).into_string());
                    Ok(())
                }

                OtcSubcmd::Join => {
                    let mut buf = String::new();
                    stdin().read_to_string(&mut buf)?;
                    let bytes = bs58::decode(&buf.trim()).into_vec()?;
                    let partial: PartialSwapData = deserialize(&bytes)?;

                    let tx = drk
                        .join_swap(partial)
                        .await
                        .with_context(|| "Failed to create a join swap transaction")?;

                    println!("{}", bs58::encode(&serialize(&tx)).into_string());
                    Ok(())
                }

                OtcSubcmd::Inspect => {
                    let mut buf = String::new();
                    stdin().read_to_string(&mut buf)?;
                    let bytes = bs58::decode(&buf.trim()).into_vec()?;

                    drk.inspect_swap(bytes).await.with_context(|| "Failed to inspect swap")?;
                    Ok(())
                }

                OtcSubcmd::Sign => {
                    let mut buf = String::new();
                    stdin().read_to_string(&mut buf)?;
                    let bytes = bs58::decode(&buf.trim()).into_vec()?;
                    let mut tx: Transaction = deserialize(&bytes)?;

                    drk.sign_swap(&mut tx)
                        .await
                        .with_context(|| "Failed to sign joined swap transaction")?;

                    println!("{}", bs58::encode(&serialize(&tx)).into_string());
                    Ok(())
                }
            }
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

            let drk = Drk::new(args.endpoint).await?;

            let txid =
                drk.broadcast_tx(&tx).await.with_context(|| "Failed to broadcast transaction")?;

            println!("Transaction ID: {}", txid);

            Ok(())
        }

        Subcmd::Subscribe => {
            let drk = Drk::new(args.endpoint.clone()).await?;

            drk.subscribe_blocks(args.endpoint)
                .await
                .with_context(|| "Block subscription failed")?;

            Ok(())
        }

        Subcmd::Scan { reset, list, checkpoint } => {
            let drk = Drk::new(args.endpoint).await?;

            if reset {
                eprintln!("Reset requested.");
                drk.scan_blocks(true).await.with_context(|| "Failed during scanning")?;

                return Ok(())
            }

            if list {
                eprintln!("List requested.");
                // TODO: implement

                return Ok(())
            }

            if let Some(c) = checkpoint {
                eprintln!("Checkpoint requested: {}", c);
                // TODO: implement

                return Ok(())
            }

            drk.scan_blocks(false).await.with_context(|| "Failed during scanning")?;
            eprintln!("Finished scanning blockchain");

            Ok(())
        }

        Subcmd::Dao(cmd) => match cmd {
            DaoSubcmd::Create { proposer_limit, quorum, approval_ratio, gov_token_id } => {
                let _ = f64::from_str(&proposer_limit).with_context(|| "Invalid proposer limit")?;
                let _ = f64::from_str(&quorum).with_context(|| "Invalid quorum")?;

                let proposer_limit = decode_base10(&proposer_limit, 8, true)?;
                let quorum = decode_base10(&quorum, 8, true)?;

                if approval_ratio > 1.0 {
                    eprintln!("Error: Approval ratio cannot be >1.0");
                    exit(1);
                }

                let approval_ratio_quot = 100_u64;
                let approval_ratio_base = (approval_ratio * approval_ratio_quot as f64) as u64;

                let gov_token_id =
                    TokenId::try_from(gov_token_id.as_str()).with_context(|| "Invalid Token ID")?;

                let secret_key = SecretKey::random(&mut OsRng);
                let bulla_blind = pallas::Base::random(&mut OsRng);

                let dao_params = DaoParams {
                    proposer_limit,
                    quorum,
                    approval_ratio_base,
                    approval_ratio_quot,
                    gov_token_id,
                    secret_key,
                    bulla_blind,
                };

                let encoded = bs58::encode(&serialize(&dao_params)).into_string();
                println!("{}", encoded);

                Ok(())
            }

            DaoSubcmd::View => {
                let mut buf = String::new();
                stdin().read_to_string(&mut buf)?;
                let bytes = bs58::decode(&buf.trim()).into_vec()?;
                let dao_params: DaoParams = deserialize(&bytes)?;
                println!("{}", dao_params);

                Ok(())
            }

            DaoSubcmd::Import { dao_name } => {
                let mut buf = String::new();
                stdin().read_to_string(&mut buf)?;
                let bytes = bs58::decode(&buf.trim()).into_vec()?;
                let dao_params: DaoParams = deserialize(&bytes)?;

                let drk = Drk::new(args.endpoint).await?;

                drk.import_dao(dao_name, dao_params)
                    .await
                    .with_context(|| "Failed to import DAO")?;

                Ok(())
            }

            DaoSubcmd::List { dao_id } => {
                let drk = Drk::new(args.endpoint).await?;

                drk.dao_list(dao_id).await.with_context(|| "Failed to list DAO")?;

                Ok(())
            }

            DaoSubcmd::Balance { dao_id } => {
                let drk = Drk::new(args.endpoint).await?;

                let balmap =
                    drk.dao_balance(dao_id).await.with_context(|| "Failed to fetch DAO balance")?;

                // Create a prettytable with the new data:
                let mut table = Table::new();
                table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                table.set_titles(row!["Token ID", "Balance"]);
                for (token_id, balance) in balmap.iter() {
                    // FIXME: Don't hardcode to 8 decimals
                    table.add_row(row![token_id, encode_base10(*balance, 8)]);
                }

                if table.is_empty() {
                    println!("No unspent balances found");
                } else {
                    println!("{}", table);
                }

                return Ok(())
            }

            DaoSubcmd::Mint { dao_id } => {
                let drk = Drk::new(args.endpoint).await?;

                let tx = drk.dao_mint(dao_id).await.with_context(|| "Failed to mint DAO")?;
                println!("{}", bs58::encode(&serialize(&tx)).into_string());
                Ok(())
            }

            DaoSubcmd::Propose { dao_id, recipient, amount, token_id } => {
                let _ = f64::from_str(&amount).with_context(|| "Invalid amount")?;
                let amount = decode_base10(&amount, 8, true)?;

                let rcpt = PublicKey::from_str(&recipient).with_context(|| "Invalid recipient")?;
                let token_id =
                    TokenId::try_from(token_id.as_str()).with_context(|| "Invalid Token ID")?;

                let drk = Drk::new(args.endpoint).await?;

                let tx = drk
                    .dao_propose(dao_id, rcpt, amount, token_id)
                    .await
                    .with_context(|| "Failed to create DAO proposal")?;

                println!("{}", bs58::encode(&serialize(&tx)).into_string());
                Ok(())
            }

            DaoSubcmd::Proposals { dao_id } => {
                let drk = Drk::new(args.endpoint).await?;

                let proposals = drk.get_dao_proposals(dao_id).await?;

                for proposal in proposals {
                    println!("[{}] {:?}", proposal.id, proposal.bulla());
                }

                Ok(())
            }

            DaoSubcmd::Proposal { dao_id, proposal_id } => {
                let drk = Drk::new(args.endpoint).await?;

                let proposals = drk.get_dao_proposals(dao_id).await?;
                let Some(proposal) = proposals.iter().find(|x| x.id == proposal_id) else {
                    eprintln!("No such DAO proposal found");
                    exit(1);
                };

                println!("{}", proposal);

                Ok(())
            }

            DaoSubcmd::Vote { dao_id, proposal_id, vote, vote_weight } => {
                let drk = Drk::new(args.endpoint).await?;

                let _ = f64::from_str(&vote_weight).with_context(|| "Invalid vote weight")?;
                let weight = decode_base10(&vote_weight, 8, true)?;

                if vote > 1 {
                    eprintln!("Vote can be either 0 (NO) or 1 (YES)");
                    exit(1);
                }
                let vote = vote != 0;

                let tx = drk
                    .dao_vote(dao_id, proposal_id, vote, weight)
                    .await
                    .with_context(|| "Failed to create DAO Vote transaction")?;

                // TODO: Write our_vote in the proposal sql.

                println!("{}", bs58::encode(&serialize(&tx)).into_string());

                Ok(())
            }

            DaoSubcmd::Exec { dao_id, proposal_id } => {
                let drk = Drk::new(args.endpoint).await?;
                let dao = drk.get_dao_by_id(dao_id).await?;
                let proposal = drk.get_dao_proposal_by_id(proposal_id).await?;
                assert!(proposal.id == dao.id);

                let tx = drk
                    .dao_exec(dao, proposal)
                    .await
                    .with_context(|| "Failed to execute DAO proposal")?;

                println!("{}", bs58::encode(&serialize(&tx)).into_string());

                Ok(())
            }
        },
    }
}
