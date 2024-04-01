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
    fs,
    io::{stdin, Read},
    process::exit,
    str::FromStr,
    sync::Arc,
    time::Instant,
};

use prettytable::{format, row, Table};
use rand::rngs::OsRng;
use smol::stream::StreamExt;
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

use darkfi::{
    async_daemonize, cli_desc,
    rpc::{client::RpcClient, jsonrpc::JsonRequest, util::JsonValue},
    tx::Transaction,
    util::{
        encoding::base64,
        parse::{decode_base10, encode_base10},
        path::expand_path,
    },
    zk::halo2::Field,
    Result,
};
use darkfi_money_contract::model::{Coin, TokenId};
use darkfi_sdk::{
    crypto::{FuncId, PublicKey, SecretKey},
    pasta::{group::ff::PrimeField, pallas},
    tx::TransactionHash,
};
use darkfi_serial::{deserialize_async, serialize_async};

/// Error codes
mod error;

/// darkfid JSON-RPC related methods
mod rpc;

/// Payment methods
mod transfer;

/// Swap methods
mod swap;
use swap::PartialSwapData;

/// Token methods
mod token;

/// CLI utility functions
mod cli_util;
use cli_util::{generate_completions, kaching, parse_token_pair, parse_value_pair};

/// Wallet functionality related to Money
mod money;
use money::BALANCE_BASE10_DECIMALS;

/// Wallet functionality related to Dao
mod dao;
use dao::DaoParams;

/// Wallet functionality related to transactions history
mod txs_history;

/// Wallet database operations handler
mod walletdb;
use walletdb::{WalletDb, WalletPtr};

const CONFIG_FILE: &str = "drk_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../drk_config.toml");

// Dev Note: when adding/modifying args here,
// don't forget to update cli_util::generate_completions()
#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "drk", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long, default_value = "~/.local/darkfi/drk/wallet.db")]
    /// Path to wallet database
    wallet_path: String,

    #[structopt(long, default_value = "changeme")]
    /// Password for the wallet database
    wallet_pass: String,

    #[structopt(short, long, default_value = "tcp://127.0.0.1:8340")]
    /// darkfid JSON-RPC endpoint
    endpoint: Url,

    #[structopt(subcommand)]
    /// Sub command to execute
    command: Subcmd,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

// Dev Note: when adding/modifying commands here,
// don't forget to update cli_util::generate_completions()
#[derive(Clone, Debug, Deserialize, StructOpt)]
enum Subcmd {
    /// Fun
    Kaching,

    /// Send a ping request to the darkfid RPC endpoint
    Ping,

    /// Generate a SHELL completion script and print to stdout
    Completions {
        /// The Shell you want to generate script for
        shell: String,
    },

    /// Wallet operations
    Wallet {
        #[structopt(long)]
        /// Initialize wallet database
        initialize: bool,

        #[structopt(long)]
        /// Generate a new keypair in the wallet
        keygen: bool,

        #[structopt(long)]
        /// Query the wallet for known balances
        balance: bool,

        #[structopt(long)]
        /// Get the default address in the wallet
        address: bool,

        #[structopt(long)]
        /// Print all the addresses in the wallet
        addresses: bool,

        #[structopt(long)]
        /// Set the default address in the wallet
        default_address: Option<usize>,

        #[structopt(long)]
        /// Print all the secret keys from the wallet
        secrets: bool,

        #[structopt(long)]
        /// Import secret keys from stdin into the wallet, separated by newlines
        import_secrets: bool,

        #[structopt(long)]
        /// Print the Merkle tree in the wallet
        tree: bool,

        #[structopt(long)]
        /// Print all the coins in the wallet
        coins: bool,
    },

    /// Unspend a coin
    Unspend {
        /// base58-encoded coin to mark as unspent
        coin: String,
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

    /// OTC atomic swap
    Otc {
        #[structopt(subcommand)]
        /// Sub command to execute
        command: OtcSubcmd,
    },

    /// Inspect a transaction from stdin
    Inspect,

    /// Read a transaction from stdin and broadcast it
    Broadcast,

    /// This subscription will listen for incoming blocks from darkfid and look
    /// through their transactions to see if there's any that interest us.
    /// With `drk` we look at transactions calling the money contract so we can
    /// find coins sent to us and fill our wallet with the necessary metadata.
    Subscribe,

    /// DAO functionalities
    Dao {
        #[structopt(subcommand)]
        /// Sub command to execute
        command: DaoSubcmd,
    },

    /// Scan the blockchain and parse relevant transactions
    Scan {
        #[structopt(long)]
        /// Reset Merkle tree and start scanning from first block
        reset: bool,

        #[structopt(long)]
        /// List all available checkpoints
        list: bool,

        #[structopt(long)]
        /// Reset Merkle tree to checkpoint index and start scanning
        checkpoint: Option<u64>,
    },

    /// Explorer related subcommands
    Explorer {
        #[structopt(subcommand)]
        /// Sub command to execute
        command: ExplorerSubcmd,
    },

    /// Manage Token aliases
    Alias {
        #[structopt(subcommand)]
        /// Sub command to execute
        command: AliasSubcmd,
    },

    /// Token functionalities
    Token {
        #[structopt(subcommand)]
        /// Sub command to execute
        command: TokenSubcmd,
    },
}

#[derive(Clone, Debug, Deserialize, StructOpt)]
enum OtcSubcmd {
    /// Initialize the first half of the atomic swap
    Init {
        /// Value pair to send:recv (11.55:99.42)
        #[structopt(short, long)]
        value_pair: String,

        /// Token pair to send:recv (f00:b4r)
        #[structopt(short, long)]
        token_pair: String,
    },

    /// Build entire swap tx given the first half from stdin
    Join,

    /// Inspect a swap half or the full swap tx from stdin
    Inspect,

    /// Sign a transaction given from stdin as the first-half
    Sign,
}

#[derive(Clone, Debug, Deserialize, StructOpt)]
enum DaoSubcmd {
    /// Create DAO parameters
    Create {
        /// The minimum amount of governance tokens needed to open a proposal for this DAO
        proposer_limit: String,
        /// Minimal threshold of participating total tokens needed for a proposal to pass
        quorum: String,
        /// The ratio of winning votes/total votes needed for a proposal to pass (2 decimals)
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
        dao_alias: Option<String>,
    },

    /// Show the balance of a DAO
    Balance {
        /// Name or numeric identifier for the DAO
        dao_alias: String,
    },

    /// Mint an imported DAO on-chain
    Mint {
        /// Name or numeric identifier for the DAO
        dao_alias: String,
    },

    /// Create a proposal for a DAO
    Propose {
        /// Name or numeric identifier for the DAO
        dao_alias: String,

        /// Pubkey to send tokens to with proposal success
        recipient: String,

        /// Amount to send from DAO with proposal success
        amount: String,

        /// Token ID to send from DAO with proposal success
        token: String,
    },

    /// List DAO proposals
    Proposals {
        /// Name or numeric identifier for the DAO
        dao_alias: String,
    },

    /// View a DAO proposal data
    Proposal {
        /// Name or numeric identifier for the DAO
        dao_alias: String,

        /// Numeric identifier for the proposal
        proposal_id: u64,
    },

    /// Vote on a given proposal
    Vote {
        /// Name or numeric identifier for the DAO
        dao_alias: String,

        /// Numeric identifier for the proposal
        proposal_id: u64,

        /// Vote (0 for NO, 1 for YES)
        vote: u8,

        /// Vote weight (amount of governance tokens)
        vote_weight: String,
    },

    /// Execute a DAO proposal
    Exec {
        /// Name or numeric identifier for the DAO
        dao_alias: String,

        /// Numeric identifier for the proposal
        proposal_id: u64,
    },
}

#[derive(Clone, Debug, Deserialize, StructOpt)]
enum ExplorerSubcmd {
    /// Fetch a blockchain transaction by hash
    FetchTx {
        /// Transaction hash
        tx_hash: String,

        #[structopt(long)]
        /// Print the full transaction information
        full: bool,

        #[structopt(long)]
        /// Encode transaction to base58
        encode: bool,
    },

    /// Read a transaction from stdin and simulate it
    SimulateTx,

    /// Fetch broadcasted transactions history
    TxsHistory {
        /// Fetch specific history record (optional)
        tx_hash: Option<String>,

        #[structopt(long)]
        /// Encode specific history record transaction to base58
        encode: bool,
    },
}

#[derive(Clone, Debug, Deserialize, StructOpt)]
enum AliasSubcmd {
    /// Create a Token alias
    Add {
        /// Token alias
        alias: String,

        /// Token to create alias for
        token: String,
    },

    /// Print alias info of optional arguments.
    /// If no argument is provided, list all the aliases in the wallet.
    Show {
        /// Token alias to search for
        #[structopt(short, long)]
        alias: Option<String>,

        /// Token to search alias for
        #[structopt(short, long)]
        token: Option<String>,
    },

    /// Remove a Token alias
    Remove {
        /// Token alias to remove
        alias: String,
    },
}

#[derive(Clone, Debug, Deserialize, StructOpt)]
enum TokenSubcmd {
    /// Import a mint authority secret from stdin
    Import,

    /// Generate a new mint authority
    GenerateMint,

    /// List token IDs with available mint authorities
    List,

    /// Mint tokens
    Mint {
        /// Token ID to mint
        token: String,

        /// Amount to mint
        amount: String,

        /// Recipient of the minted tokens
        recipient: String,
    },

    /// Freeze a token mint
    Freeze {
        /// Token ID to freeze
        token: String,
    },
}

/// CLI-util structure
pub struct Drk {
    /// Wallet database operations handler
    pub wallet: WalletPtr,
    /// JSON-RPC client to execute requests to darkfid daemon
    pub rpc_client: RpcClient,
}

impl Drk {
    async fn new(
        wallet_path: String,
        wallet_pass: String,
        endpoint: Url,
        ex: Arc<smol::Executor<'static>>,
    ) -> Result<Self> {
        // Script kiddies protection
        if wallet_pass == "changeme" {
            eprintln!("Please don't use default wallet password...");
            exit(2);
        }

        // Initialize wallet
        let wallet_path = expand_path(&wallet_path)?;
        if !wallet_path.exists() {
            if let Some(parent) = wallet_path.parent() {
                fs::create_dir_all(parent)?;
            }
        }
        let wallet = match WalletDb::new(Some(wallet_path), Some(&wallet_pass)) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Error initializing wallet: {e:?}");
                exit(2);
            }
        };

        // Initialize rpc client
        let rpc_client = RpcClient::new(endpoint, ex).await?;

        Ok(Self { wallet, rpc_client })
    }

    /// Initialize wallet with tables for drk
    async fn initialize_wallet(&self) -> Result<()> {
        let wallet_schema = include_str!("../wallet.sql");
        if let Err(e) = self.wallet.exec_batch_sql(wallet_schema).await {
            eprintln!("Error initializing wallet: {e:?}");
            exit(2);
        }

        Ok(())
    }

    /// Auxiliary function to ping configured darkfid daemon for liveness.
    async fn ping(&self) -> Result<()> {
        println!("Executing ping request to darkfid...");
        let latency = Instant::now();
        let req = JsonRequest::new("ping", JsonValue::Array(vec![]));
        let rep = self.rpc_client.oneshot_request(req).await?;
        let latency = latency.elapsed();
        println!("Got reply: {rep:?}");
        println!("Latency: {latency:?}");
        Ok(())
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    match args.command {
        Subcmd::Kaching => {
            kaching().await;
            Ok(())
        }

        Subcmd::Ping => {
            let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;
            drk.ping().await
        }

        Subcmd::Completions { shell } => generate_completions(&shell),

        Subcmd::Wallet {
            initialize,
            keygen,
            balance,
            address,
            addresses,
            default_address,
            secrets,
            import_secrets,
            tree,
            coins,
        } => {
            if !initialize &&
                !keygen &&
                !balance &&
                !address &&
                !addresses &&
                default_address.is_none() &&
                !secrets &&
                !tree &&
                !coins &&
                !import_secrets
            {
                eprintln!("Error: You must use at least one flag for this subcommand");
                eprintln!("Run with \"wallet -h\" to see the subcommand usage.");
                exit(2);
            }

            let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;

            if initialize {
                drk.initialize_wallet().await?;
                if let Err(e) = drk.initialize_money().await {
                    eprintln!("Failed to initialize Money: {e:?}");
                    exit(2);
                }
                if let Err(e) = drk.initialize_dao().await {
                    eprintln!("Failed to initialize DAO: {e:?}");
                    exit(2);
                }
                return Ok(())
            }

            if keygen {
                if let Err(e) = drk.money_keygen().await {
                    eprintln!("Failed to generate keypair: {e:?}");
                    exit(2);
                }
                return Ok(())
            }

            if balance {
                let balmap = drk.money_balance().await?;

                let aliases_map = drk.get_aliases_mapped_by_token().await?;

                // Create a prettytable with the new data:
                let mut table = Table::new();
                table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                table.set_titles(row!["Token ID", "Aliases", "Balance"]);
                for (token_id, balance) in balmap.iter() {
                    let aliases = match aliases_map.get(token_id) {
                        Some(a) => a,
                        None => "-",
                    };

                    table.add_row(row![
                        token_id,
                        aliases,
                        encode_base10(*balance, BALANCE_BASE10_DECIMALS)
                    ]);
                }

                if table.is_empty() {
                    println!("No unspent balances found");
                } else {
                    println!("{table}");
                }

                return Ok(())
            }

            if address {
                let address = match drk.default_address().await {
                    Ok(a) => a,
                    Err(e) => {
                        eprintln!("Failed to fetch default address: {e:?}");
                        exit(2);
                    }
                };

                println!("{address}");

                return Ok(())
            }

            if addresses {
                let addresses = drk.addresses().await?;

                // Create a prettytable with the new data:
                let mut table = Table::new();
                table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                table.set_titles(row!["Key ID", "Public Key", "Secret Key", "Is Default"]);
                for (key_id, public_key, secret_key, is_default) in addresses {
                    let is_default = match is_default {
                        1 => "*",
                        _ => "",
                    };
                    table.add_row(row![key_id, public_key, secret_key, is_default]);
                }

                if table.is_empty() {
                    println!("No addresses found");
                } else {
                    println!("{table}");
                }

                return Ok(())
            }

            if let Some(idx) = default_address {
                if let Err(e) = drk.set_default_address(idx).await {
                    eprintln!("Failed to set default address: {e:?}");
                    exit(2);
                }
                return Ok(())
            }

            if secrets {
                let v = drk.get_money_secrets().await?;

                for i in v {
                    println!("{i}");
                }

                return Ok(())
            }

            if import_secrets {
                let mut secrets = vec![];
                let lines = stdin().lines();
                for (i, line) in lines.enumerate() {
                    if let Ok(line) = line {
                        let bytes = bs58::decode(&line.trim()).into_vec()?;
                        let Ok(secret) = deserialize_async(&bytes).await else {
                            println!("Warning: Failed to deserialize secret on line {i}");
                            continue
                        };
                        secrets.push(secret);
                    }
                }

                let pubkeys = match drk.import_money_secrets(secrets).await {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("Failed to import secret keys into wallet: {e:?}");
                        exit(2);
                    }
                };

                for key in pubkeys {
                    println!("{key}");
                }

                return Ok(())
            }

            if tree {
                let tree = drk.get_money_tree().await?;

                println!("{tree:#?}");

                return Ok(())
            }

            if coins {
                let coins = drk.get_coins(true).await?;

                let aliases_map = drk.get_aliases_mapped_by_token().await?;

                if coins.is_empty() {
                    return Ok(())
                }

                let mut table = Table::new();
                table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                table.set_titles(row![
                    "Coin",
                    "Spent",
                    "Token ID",
                    "Aliases",
                    "Value",
                    "Spend Hook",
                    "User Data"
                ]);
                for coin in coins {
                    let aliases = match aliases_map.get(&coin.0.note.token_id.to_string()) {
                        Some(a) => a,
                        None => "-",
                    };

                    let spend_hook = if coin.0.note.spend_hook != FuncId::none() {
                        bs58::encode(&serialize_async(&coin.0.note.spend_hook.inner()).await)
                            .into_string()
                            .to_string()
                    } else {
                        String::from("-")
                    };

                    let user_data = if coin.0.note.user_data != pallas::Base::ZERO {
                        bs58::encode(&serialize_async(&coin.0.note.user_data).await)
                            .into_string()
                            .to_string()
                    } else {
                        String::from("-")
                    };

                    table.add_row(row![
                        bs58::encode(&serialize_async(&coin.0.coin.inner()).await)
                            .into_string()
                            .to_string(),
                        coin.1,
                        coin.0.note.token_id,
                        aliases,
                        format!(
                            "{} ({})",
                            coin.0.note.value,
                            encode_base10(coin.0.note.value, BALANCE_BASE10_DECIMALS)
                        ),
                        spend_hook,
                        user_data
                    ]);
                }

                println!("{table}");

                return Ok(())
            }

            unreachable!()
        }

        Subcmd::Unspend { coin } => {
            let bytes: [u8; 32] = match bs58::decode(&coin).into_vec()?.try_into() {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Invalid coin: {e:?}");
                    exit(2);
                }
            };

            let elem: pallas::Base = match pallas::Base::from_repr(bytes).into() {
                Some(v) => v,
                None => {
                    eprintln!("Invalid coin");
                    exit(2);
                }
            };

            let coin = Coin::from(elem);
            let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;
            if let Err(e) = drk.unspend_coin(&coin).await {
                eprintln!("Failed to mark coin as unspent: {e:?}");
                exit(2);
            }

            Ok(())
        }

        Subcmd::Transfer { amount, token, recipient } => {
            let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;

            if let Err(e) = f64::from_str(&amount) {
                eprintln!("Invalid amount: {e:?}");
                exit(2);
            }

            let rcpt = match PublicKey::from_str(&recipient) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Invalid recipient: {e:?}");
                    exit(2);
                }
            };

            let token_id = match drk.get_token(token).await {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Invalid token alias: {e:?}");
                    exit(2);
                }
            };

            let tx = match drk.transfer(&amount, token_id, rcpt).await {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Failed to create payment transaction: {e:?}");
                    exit(2);
                }
            };

            println!("{}", base64::encode(&serialize_async(&tx).await));

            Ok(())
        }

        Subcmd::Otc { command } => {
            let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;

            match command {
                OtcSubcmd::Init { value_pair, token_pair } => {
                    let (vp_send, vp_recv) = parse_value_pair(&value_pair)?;
                    let (tp_send, tp_recv) = parse_token_pair(&drk, &token_pair).await?;

                    let half = match drk.init_swap(vp_send, tp_send, vp_recv, tp_recv).await {
                        Ok(h) => h,
                        Err(e) => {
                            eprintln!("Failed to create swap transaction half: {e:?}");
                            exit(2);
                        }
                    };

                    println!("{}", base64::encode(&serialize_async(&half).await));
                    Ok(())
                }

                OtcSubcmd::Join => {
                    let mut buf = String::new();
                    stdin().read_to_string(&mut buf)?;
                    let Some(bytes) = base64::decode(buf.trim()) else {
                        eprintln!("Failed to decode partial swap data");
                        exit(2);
                    };

                    let partial: PartialSwapData = deserialize_async(&bytes).await?;

                    let tx = match drk.join_swap(partial).await {
                        Ok(tx) => tx,
                        Err(e) => {
                            eprintln!("Failed to create a join swap transaction: {e:?}");
                            exit(2);
                        }
                    };

                    println!("{}", base64::encode(&serialize_async(&tx).await));
                    Ok(())
                }

                OtcSubcmd::Inspect => {
                    let mut buf = String::new();
                    stdin().read_to_string(&mut buf)?;
                    let Some(bytes) = base64::decode(buf.trim()) else {
                        eprintln!("Failed to decode swap transaction");
                        exit(2);
                    };

                    if let Err(e) = drk.inspect_swap(bytes).await {
                        eprintln!("Failed to inspect swap: {e:?}");
                        exit(2);
                    };

                    Ok(())
                }

                OtcSubcmd::Sign => {
                    let mut buf = String::new();
                    stdin().read_to_string(&mut buf)?;
                    let Some(bytes) = base64::decode(buf.trim()) else {
                        eprintln!("Failed to decode swap transaction");
                        exit(1);
                    };

                    let mut tx: Transaction = deserialize_async(&bytes).await?;

                    if let Err(e) = drk.sign_swap(&mut tx).await {
                        eprintln!("Failed to sign joined swap transaction: {e:?}");
                        exit(2);
                    };

                    println!("{}", base64::encode(&serialize_async(&tx).await));
                    Ok(())
                }
            }
        }

        Subcmd::Dao { command } => match command {
            DaoSubcmd::Create { proposer_limit, quorum, approval_ratio, gov_token_id } => {
                if let Err(e) = f64::from_str(&proposer_limit) {
                    eprintln!("Invalid proposer limit: {e:?}");
                    exit(2);
                }
                if let Err(e) = f64::from_str(&quorum) {
                    eprintln!("Invalid quorum: {e:?}");
                    exit(2);
                }

                let proposer_limit = decode_base10(&proposer_limit, BALANCE_BASE10_DECIMALS, true)?;
                let quorum = decode_base10(&quorum, BALANCE_BASE10_DECIMALS, true)?;

                if approval_ratio > 1.0 {
                    eprintln!("Error: Approval ratio cannot be >1.0");
                    exit(2);
                }

                let approval_ratio_base = 100_u64;
                let approval_ratio_quot = (approval_ratio * approval_ratio_base as f64) as u64;

                let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;
                let gov_token_id = match drk.get_token(gov_token_id).await {
                    Ok(g) => g,
                    Err(e) => {
                        eprintln!("Invalid Token ID: {e:?}");
                        exit(2);
                    }
                };

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

                let encoded = bs58::encode(&serialize_async(&dao_params).await).into_string();
                println!("{encoded}");

                Ok(())
            }

            DaoSubcmd::View => {
                let mut buf = String::new();
                stdin().read_to_string(&mut buf)?;
                let bytes = bs58::decode(&buf.trim()).into_vec()?;
                let dao_params: DaoParams = deserialize_async(&bytes).await?;
                println!("{dao_params}");

                Ok(())
            }

            DaoSubcmd::Import { dao_name } => {
                let mut buf = String::new();
                stdin().read_to_string(&mut buf)?;
                let bytes = bs58::decode(&buf.trim()).into_vec()?;
                let dao_params: DaoParams = deserialize_async(&bytes).await?;

                let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;

                if let Err(e) = drk.import_dao(dao_name, dao_params).await {
                    eprintln!("Failed to import DAO: {e:?}");
                    exit(2);
                }

                Ok(())
            }

            DaoSubcmd::List { dao_alias } => {
                let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;
                // We cannot use .map() since get_dao_id() uses ?
                let dao_id = match dao_alias {
                    Some(alias) => Some(drk.get_dao_id(&alias).await?),
                    None => None,
                };

                if let Err(e) = drk.dao_list(dao_id).await {
                    eprintln!("Failed to list DAO: {e:?}");
                    exit(2);
                }

                Ok(())
            }

            DaoSubcmd::Balance { dao_alias } => {
                let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;
                let dao_id = drk.get_dao_id(&dao_alias).await?;

                let balmap = match drk.dao_balance(dao_id).await {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("Failed to fetch DAO balance: {e:?}");
                        exit(2);
                    }
                };

                let aliases_map = match drk.get_aliases_mapped_by_token().await {
                    Ok(a) => a,
                    Err(e) => {
                        eprintln!("Failed to fetch wallet aliases: {e:?}");
                        exit(2);
                    }
                };

                // Create a prettytable with the new data:
                let mut table = Table::new();
                table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                table.set_titles(row!["Token ID", "Aliases", "Balance"]);
                for (token_id, balance) in balmap.iter() {
                    let aliases = match aliases_map.get(token_id) {
                        Some(a) => a,
                        None => "-",
                    };

                    table.add_row(row![
                        token_id,
                        aliases,
                        encode_base10(*balance, BALANCE_BASE10_DECIMALS)
                    ]);
                }

                if table.is_empty() {
                    println!("No unspent balances found");
                } else {
                    println!("{table}");
                }

                Ok(())
            }

            DaoSubcmd::Mint { dao_alias } => {
                let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;
                let dao_id = drk.get_dao_id(&dao_alias).await?;

                let tx = match drk.dao_mint(dao_id).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Failed to mint DAO: {e:?}");
                        exit(2);
                    }
                };
                println!("{}", base64::encode(&serialize_async(&tx).await));
                Ok(())
            }

            DaoSubcmd::Propose { dao_alias, recipient, amount, token } => {
                if let Err(e) = f64::from_str(&amount) {
                    eprintln!("Invalid amount: {e:?}");
                    exit(2);
                }
                let amount = decode_base10(&amount, BALANCE_BASE10_DECIMALS, true)?;
                let rcpt = match PublicKey::from_str(&recipient) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Invalid recipient: {e:?}");
                        exit(2);
                    }
                };

                let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;
                let dao_id = drk.get_dao_id(&dao_alias).await?;
                let token_id = match drk.get_token(token).await {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("Invalid token alias: {e:?}");
                        exit(2);
                    }
                };

                let tx = match drk.dao_propose(dao_id, rcpt, amount, token_id).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Failed to create DAO proposal: {e:?}");
                        exit(2);
                    }
                };
                println!("{}", base64::encode(&serialize_async(&tx).await));
                Ok(())
            }

            DaoSubcmd::Proposals { dao_alias } => {
                let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;
                let dao_id = drk.get_dao_id(&dao_alias).await?;

                let proposals = drk.get_dao_proposals(dao_id).await?;

                for proposal in proposals {
                    println!("[{}] {:?}", proposal.id, proposal.bulla());
                }

                Ok(())
            }

            DaoSubcmd::Proposal { dao_alias, proposal_id } => {
                let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;
                let dao_id = drk.get_dao_id(&dao_alias).await?;

                let proposals = drk.get_dao_proposals(dao_id).await?;
                let Some(proposal) = proposals.iter().find(|x| x.id == proposal_id) else {
                    eprintln!("No such DAO proposal found");
                    exit(2);
                };

                println!("{proposal}");

                let votes = drk.get_dao_proposal_votes(proposal_id).await?;
                println!("votes:");
                for vote in votes {
                    let option = if vote.vote_option { "yes" } else { "no " };
                    eprintln!("  {option} {}", vote.all_vote_value);
                }

                Ok(())
            }

            DaoSubcmd::Vote { dao_alias, proposal_id, vote, vote_weight } => {
                let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;
                let dao_id = drk.get_dao_id(&dao_alias).await?;

                if let Err(e) = f64::from_str(&vote_weight) {
                    eprintln!("Invalid vote weight: {e:?}");
                    exit(2);
                }
                let weight = decode_base10(&vote_weight, BALANCE_BASE10_DECIMALS, true)?;

                if vote > 1 {
                    eprintln!("Vote can be either 0 (NO) or 1 (YES)");
                    exit(2);
                }
                let vote = vote != 0;

                let tx = match drk.dao_vote(dao_id, proposal_id, vote, weight).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Failed to create DAO Vote transaction: {e:?}");
                        exit(2);
                    }
                };

                // TODO: Write our_vote in the proposal sql.

                println!("{}", bs58::encode(&serialize_async(&tx).await).into_string());

                Ok(())
            }

            DaoSubcmd::Exec { dao_alias, proposal_id } => {
                let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;
                let dao_id = drk.get_dao_id(&dao_alias).await?;
                let dao = drk.get_dao_by_id(dao_id).await?;
                let proposal = drk.get_dao_proposal_by_id(proposal_id).await?;
                assert!(proposal.dao_bulla == dao.bulla());

                let tx = match drk.dao_exec(dao, proposal).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Failed to execute DAO proposal: {e:?}");
                        exit(2);
                    }
                };
                println!("{}", base64::encode(&serialize_async(&tx).await));

                Ok(())
            }
        },

        Subcmd::Inspect => {
            let mut buf = String::new();
            stdin().read_to_string(&mut buf)?;
            let Some(bytes) = base64::decode(buf.trim()) else {
                eprintln!("Failed to decode transaction");
                exit(1);
            };

            let tx: Transaction = deserialize_async(&bytes).await?;
            println!("{tx:#?}");
            Ok(())
        }

        Subcmd::Broadcast => {
            println!("Reading transaction from stdin...");
            let mut buf = String::new();
            stdin().read_to_string(&mut buf)?;
            let Some(bytes) = base64::decode(buf.trim()) else {
                eprintln!("Failed to decode transaction");
                exit(1);
            };

            let tx = deserialize_async(&bytes).await?;

            let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;

            let txid = match drk.broadcast_tx(&tx).await {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Failed to broadcast transaction: {e:?}");
                    exit(2);
                }
            };

            println!("Transaction ID: {txid}");

            Ok(())
        }

        Subcmd::Subscribe => {
            let drk =
                Drk::new(args.wallet_path, args.wallet_pass, args.endpoint.clone(), ex.clone())
                    .await?;

            if let Err(e) = drk.subscribe_blocks(args.endpoint, ex).await {
                eprintln!("Block subscription failed: {e:?}");
                exit(2);
            }

            Ok(())
        }

        Subcmd::Scan { reset, list, checkpoint } => {
            let drk =
                Drk::new(args.wallet_path, args.wallet_pass, args.endpoint.clone(), ex.clone())
                    .await?;

            if reset {
                println!("Reset requested.");
                if let Err(e) = drk.scan_blocks(true).await {
                    eprintln!("Failed during scanning: {e:?}");
                    exit(2);
                }
                println!("Finished scanning blockchain");

                return Ok(())
            }

            if list {
                println!("List requested.");
                // TODO: implement
                unimplemented!()
            }

            if let Some(c) = checkpoint {
                println!("Checkpoint requested: {c}");
                // TODO: implement
                unimplemented!()
            }

            if let Err(e) = drk.scan_blocks(false).await {
                eprintln!("Failed during scanning: {e:?}");
                exit(2);
            }
            println!("Finished scanning blockchain");

            Ok(())
        }

        Subcmd::Explorer { command } => match command {
            ExplorerSubcmd::FetchTx { tx_hash, full, encode } => {
                let tx_hash = TransactionHash(*blake3::Hash::from_hex(&tx_hash)?.as_bytes());

                let drk =
                    Drk::new(args.wallet_path, args.wallet_pass, args.endpoint.clone(), ex.clone())
                        .await?;

                let tx = match drk.get_tx(&tx_hash).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Failed to fetch transaction: {e:?}");
                        exit(2);
                    }
                };

                let Some(tx) = tx else {
                    println!("Transaction was not found");
                    exit(1);
                };

                // Make sure the tx is correct
                assert_eq!(tx.hash(), tx_hash);

                if encode {
                    println!("{}", base64::encode(&serialize_async(&tx).await));
                    exit(1)
                }

                println!("Transaction ID: {tx_hash}");
                if full {
                    println!("{tx:?}");
                }

                Ok(())
            }

            ExplorerSubcmd::SimulateTx => {
                println!("Reading transaction from stdin...");
                let mut buf = String::new();
                stdin().read_to_string(&mut buf)?;
                let Some(bytes) = base64::decode(buf.trim()) else {
                    eprintln!("Failed to decode transaction");
                    exit(1);
                };

                let tx = deserialize_async(&bytes).await?;

                let drk =
                    Drk::new(args.wallet_path, args.wallet_pass, args.endpoint.clone(), ex.clone())
                        .await?;

                let is_valid = match drk.simulate_tx(&tx).await {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("Failed to simulate tx: {e:?}");
                        exit(2);
                    }
                };

                println!("Transaction ID: {}", tx.hash());
                println!("State: {}", if is_valid { "valid" } else { "invalid" });

                Ok(())
            }

            ExplorerSubcmd::TxsHistory { tx_hash, encode } => {
                let drk =
                    Drk::new(args.wallet_path, args.wallet_pass, args.endpoint.clone(), ex.clone())
                        .await?;

                if let Some(c) = tx_hash {
                    let (tx_hash, status, tx) = drk.get_tx_history_record(&c).await?;

                    if encode {
                        println!("{}", base64::encode(&serialize_async(&tx).await));
                        exit(1)
                    }

                    println!("Transaction ID: {tx_hash}");
                    println!("Status: {status}");
                    println!("{tx:?}");

                    return Ok(())
                }

                let map = match drk.get_txs_history().await {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Failed to retrieve transactions history records: {e:?}");
                        exit(2);
                    }
                };

                // Create a prettytable with the new data:
                let mut table = Table::new();
                table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                table.set_titles(row!["Transaction Hash", "Status"]);
                for (txs_hash, status) in map.iter() {
                    table.add_row(row![txs_hash, status]);
                }

                if table.is_empty() {
                    println!("No transactions found");
                } else {
                    println!("{table}");
                }

                Ok(())
            }
        },

        Subcmd::Alias { command } => match command {
            AliasSubcmd::Add { alias, token } => {
                if alias.chars().count() > 5 {
                    eprintln!("Error: Alias exceeds 5 characters");
                    exit(2);
                }

                let token_id = match TokenId::from_str(token.as_str()) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("Invalid Token ID: {e:?}");
                        exit(2);
                    }
                };

                let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;
                if let Err(e) = drk.add_alias(alias, token_id).await {
                    eprintln!("Failed to add alias: {e:?}");
                    exit(2);
                }

                Ok(())
            }

            AliasSubcmd::Show { alias, token } => {
                let token_id = match token {
                    Some(t) => match TokenId::from_str(t.as_str()) {
                        Ok(t) => Some(t),
                        Err(e) => {
                            eprintln!("Invalid Token ID: {e:?}");
                            exit(2);
                        }
                    },
                    None => None,
                };

                let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;
                let map = drk.get_aliases(alias, token_id).await?;

                // Create a prettytable with the new data:
                let mut table = Table::new();
                table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                table.set_titles(row!["Alias", "Token ID"]);
                for (alias, token_id) in map.iter() {
                    table.add_row(row![alias, token_id]);
                }

                if table.is_empty() {
                    println!("No aliases found");
                } else {
                    println!("{table}");
                }

                Ok(())
            }

            AliasSubcmd::Remove { alias } => {
                let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;
                if let Err(e) = drk.remove_alias(alias).await {
                    eprintln!("Failed to remove alias: {e:?}");
                    exit(2);
                }

                Ok(())
            }
        },

        Subcmd::Token { command } => match command {
            TokenSubcmd::Import => {
                let mut buf = String::new();
                stdin().read_to_string(&mut buf)?;
                let mint_authority = match SecretKey::from_str(buf.trim()) {
                    Ok(ma) => ma,
                    Err(e) => {
                        eprintln!("Invalid secret key: {e:?}");
                        exit(2);
                    }
                };

                let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;
                if let Err(e) = drk.import_mint_authority(mint_authority).await {
                    eprintln!("Importing mint authority failed: {e:?}");
                    exit(2);
                };

                let token_id = TokenId::derive(mint_authority);
                println!("Successfully imported mint authority for token ID: {token_id}");

                Ok(())
            }

            TokenSubcmd::GenerateMint => {
                let mint_authority = SecretKey::random(&mut OsRng);

                let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;

                if let Err(e) = drk.import_mint_authority(mint_authority).await {
                    eprintln!("Importing mint authority failed: {e:?}");
                    exit(2);
                };

                // TODO: see TokenAttributes struct. I'm not sure how to restructure this rn.
                let token_id = TokenId::derive(mint_authority);
                println!("Successfully imported mint authority for token ID: {token_id}");

                Ok(())
            }

            TokenSubcmd::List => {
                let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;
                let tokens = drk.list_tokens().await?;
                let aliases_map = match drk.get_aliases_mapped_by_token().await {
                    Ok(map) => map,
                    Err(e) => {
                        eprintln!("Failed to fetch wallet aliases: {e:?}");
                        exit(2);
                    }
                };

                let mut table = Table::new();
                table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                table.set_titles(row!["Token ID", "Aliases", "Mint Authority", "Frozen"]);

                for (token_id, authority, frozen) in tokens {
                    let aliases = match aliases_map.get(&token_id.to_string()) {
                        Some(a) => a,
                        None => "-",
                    };

                    table.add_row(row![token_id, aliases, authority, frozen]);
                }

                if table.is_empty() {
                    println!("No tokens found");
                } else {
                    println!("{table}");
                }

                Ok(())
            }

            // TODO: Mint directly into DAO treasury
            TokenSubcmd::Mint { token, amount, recipient } => {
                let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;

                if let Err(e) = f64::from_str(&amount) {
                    eprintln!("Invalid amount: {e:?}");
                    exit(2);
                }

                let _rcpt = match PublicKey::from_str(&recipient) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Invalid recipient: {e:?}");
                        exit(2);
                    }
                };

                let _token_id = match drk.get_token(token).await {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("Invalid Token ID: {e:?}");
                        exit(2);
                    }
                };

                panic!("temporarily disabled due to change of API for drk.mint_token() fn");
                //let tx = match drk.mint_token(&amount, rcpt, token_id).await {
                //    Ok(tx) => tx,
                //    Err(e) => {
                //        eprintln!("Failed to create token mint transaction: {e:?}");
                //        exit(2);
                //    }
                //};

                //println!("{}", base64::encode(&serialize_async(&tx).await));

                //Ok(())
            }

            TokenSubcmd::Freeze { token } => {
                let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;
                let _token_id = match drk.get_token(token).await {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("Invalid Token ID: {e:?}");
                        exit(2);
                    }
                };

                panic!("temporarily disabled due to change of API for drk.mint_token() fn");
                //let tx = match drk.freeze_token(token_id).await {
                //    Ok(tx) => tx,
                //    Err(e) => {
                //        eprintln!("Failed to create token freeze transaction: {e:?}");
                //        exit(2);
                //    }
                //};

                //println!("{}", base64::encode(&serialize_async(&tx).await));

                //Ok(())
            }
        },
    }
}
