/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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
    sync::Arc,
};

use log::info;
use prettytable::{format, row, Table};
use rand::rngs::OsRng;
use smol::{fs::read_to_string, stream::StreamExt};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

use darkfi::{
    async_daemonize, cli_desc,
    util::{
        encoding::base64,
        parse::{decode_base10, encode_base10},
        path::{expand_path, get_config_path},
    },
    zk::halo2::Field,
    Error, Result,
};
use darkfi_dao_contract::{blockwindow, model::DaoProposalBulla, DaoFunction};
use darkfi_money_contract::model::{Coin, CoinAttributes, TokenId};
use darkfi_sdk::{
    crypto::{
        note::AeadEncryptedNote, BaseBlind, FuncId, FuncRef, Keypair, PublicKey, SecretKey,
        DAO_CONTRACT_ID,
    },
    pasta::{group::ff::PrimeField, pallas},
    tx::TransactionHash,
};
use darkfi_serial::{deserialize_async, serialize_async};

use drk::{
    cli_util::{
        generate_completions, kaching, parse_token_pair, parse_tx_from_stdin, parse_value_pair,
    },
    dao::{DaoParams, ProposalRecord},
    money::BALANCE_BASE10_DECIMALS,
    swap::PartialSwapData,
    Drk,
};

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

    #[structopt(short, long, default_value = "testnet")]
    /// Blockchain network to use
    network: String,

    #[structopt(subcommand)]
    /// Sub command to execute
    command: Subcmd,

    #[structopt(short, long)]
    /// Flag indicating whether you want some fun in your life
    fun: bool,

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

    /// Read a transaction from stdin and mark its input coins as spent
    Spend,

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

        /// Optional contract spend hook to use
        spend_hook: Option<String>,

        /// Optional user data to use
        user_data: Option<String>,

        #[structopt(long)]
        /// Split the output coin into two equal halves
        half_split: bool,
    },

    /// OTC atomic swap
    Otc {
        #[structopt(subcommand)]
        /// Sub command to execute
        command: OtcSubcmd,
    },

    /// Attach the fee call to a transaction given from stdin
    AttachFee,

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
        /// Reset wallet state to provided block height and start scanning
        reset: Option<u32>,
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

    /// Contract functionalities
    Contract {
        #[structopt(subcommand)]
        /// Sub command to execute
        command: ContractSubcmd,
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

    /// Sign a swap transaction given from stdin
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
        /// Minimal threshold of participating total tokens needed for a proposal to
        /// be considered as strongly supported, enabling early execution.
        /// Must be greater or equal to normal quorum.
        early_exec_quorum: String,
        /// The ratio of winning votes/total votes needed for a proposal to pass (2 decimals)
        approval_ratio: f64,
        /// DAO's governance token ID
        gov_token_id: String,
    },

    /// View DAO data from stdin
    View,

    /// Import DAO data from stdin
    Import {
        /// Name identifier for the DAO
        name: String,
    },

    /// Update DAO keys from stdin
    UpdateKeys,

    /// List imported DAOs (or info about a specific one)
    List {
        /// Name identifier for the DAO (optional)
        name: Option<String>,
    },

    /// Show the balance of a DAO
    Balance {
        /// Name identifier for the DAO
        name: String,
    },

    /// Mint an imported DAO on-chain
    Mint {
        /// Name identifier for the DAO
        name: String,
    },

    /// Create a transfer proposal for a DAO
    ProposeTransfer {
        /// Name identifier for the DAO
        name: String,

        /// Duration of the proposal, in block windows
        duration: u64,

        /// Amount to send
        amount: String,

        /// Token ID to send
        token: String,

        /// Recipient address
        recipient: String,

        /// Optional contract spend hook to use
        spend_hook: Option<String>,

        /// Optional user data to use
        user_data: Option<String>,
    },

    /// Create a generic proposal for a DAO
    ProposeGeneric {
        /// Name identifier for the DAO
        name: String,

        /// Duration of the proposal, in block windows
        duration: u64,

        /// Optional user data to use
        user_data: Option<String>,
    },

    /// List DAO proposals
    Proposals {
        /// Name identifier for the DAO
        name: String,
    },

    /// View a DAO proposal data
    Proposal {
        /// Bulla identifier for the proposal
        bulla: String,

        #[structopt(long)]
        /// Encrypt the proposal and encode it to base64
        export: bool,

        #[structopt(long)]
        /// Create the proposal transaction
        mint_proposal: bool,
    },

    /// Import a base64 encoded and encrypted proposal from stdin
    ProposalImport,

    /// Vote on a given proposal
    Vote {
        /// Bulla identifier for the proposal
        bulla: String,

        /// Vote (0 for NO, 1 for YES)
        vote: u8,

        /// Optional vote weight (amount of governance tokens)
        vote_weight: Option<String>,
    },

    /// Execute a DAO proposal
    Exec {
        /// Bulla identifier for the proposal
        bulla: String,

        #[structopt(long)]
        /// Execute the proposal early
        early: bool,
    },

    /// Print the DAO contract base58-encoded spend hook
    SpendHook,
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

    /// Remove reverted transactions from history
    ClearReverted,

    /// Fetch scanned blocks records
    ScannedBlocks {
        /// Fetch specific height record (optional)
        height: Option<u32>,
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
    /// Import a mint authority
    Import {
        /// Mint authority secret key
        secret_key: String,

        /// Mint authority token blind
        token_blind: String,
    },

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

        /// Optional contract spend hook to use
        spend_hook: Option<String>,

        /// Optional user data to use
        user_data: Option<String>,
    },

    /// Freeze a token mint
    Freeze {
        /// Token ID to freeze
        token: String,
    },
}

#[derive(Clone, Debug, Deserialize, StructOpt)]
enum ContractSubcmd {
    /// Generate a new deploy authority
    GenerateDeploy,

    /// List deploy authorities in the wallet
    List,

    /// Deploy a smart contract
    Deploy {
        /// Contract ID (deploy authority)
        deploy_auth: u64,

        /// Path to contract wasm bincode
        wasm_path: String,

        /// Path to serialized deploy instruction
        deploy_ix: String,
    },

    /// Lock a smart contract
    Lock {
        /// Contract ID (deploy authority)
        deploy_auth: u64,
    },
}

/// Defines a blockchain network configuration.
/// Default values correspond to a local network.
#[derive(Clone, Debug, serde::Deserialize, structopt::StructOpt, structopt_toml::StructOptToml)]
#[structopt()]
struct BlockchainNetwork {
    #[structopt(long, default_value = "~/.local/share/darkfi/drk/localnet/cache")]
    /// Path to blockchain cache database
    cache_path: String,

    #[structopt(long, default_value = "~/.local/share/darkfi/drk/localnet/wallet.db")]
    /// Path to wallet database
    wallet_path: String,

    #[structopt(long, default_value = "changeme")]
    /// Password for the wallet database
    wallet_pass: String,

    #[structopt(short, long, default_value = "tcp://127.0.0.1:8240")]
    /// darkfid JSON-RPC endpoint
    endpoint: Url,
}

/// Auxiliary function to parse darkfid configuration file and extract requested
/// blockchain network config.
async fn parse_blockchain_config(
    config: Option<String>,
    network: &str,
) -> Result<BlockchainNetwork> {
    // Grab config path
    let config_path = get_config_path(config, CONFIG_FILE)?;

    // Parse TOML file contents
    let contents = read_to_string(&config_path).await?;
    let contents: toml::Value = match toml::from_str(&contents) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed parsing TOML config: {e}");
            return Err(Error::ParseFailed("Failed parsing TOML config"))
        }
    };

    // Grab requested network config
    let Some(table) = contents.as_table() else { return Err(Error::ParseFailed("TOML not a map")) };
    let Some(network_configs) = table.get("network_config") else {
        return Err(Error::ParseFailed("TOML does not contain network configurations"))
    };
    let Some(network_configs) = network_configs.as_table() else {
        return Err(Error::ParseFailed("`network_config` not a map"))
    };
    let Some(network_config) = network_configs.get(network) else {
        return Err(Error::ParseFailed("TOML does not contain requested network configuration"))
    };
    let network_config = toml::to_string(&network_config).unwrap();
    let network_config =
        match BlockchainNetwork::from_iter_with_toml::<Vec<String>>(&network_config, vec![]) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Failed parsing requested network configuration: {e}");
                return Err(Error::ParseFailed("Failed parsing requested network configuration"))
            }
        };

    Ok(network_config)
}

/// Auxiliary function to create a `Drk` wallet for provided configuration.
async fn new_wallet(
    cache_path: String,
    wallet_path: String,
    wallet_pass: String,
    endpoint: Option<Url>,
    ex: Arc<smol::Executor<'static>>,
    fun: bool,
) -> Drk {
    // Script kiddies protection
    if wallet_pass == "changeme" {
        eprintln!("Please don't use default wallet password...");
        exit(2);
    }

    match Drk::new(cache_path, wallet_path, wallet_pass, endpoint, ex, fun).await {
        Ok(wallet) => wallet,
        Err(e) => {
            eprintln!("Error initializing wallet: {e:?}");
            exit(2);
        }
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    // Grab blockchain network configuration
    let blockchain_config = match args.network.as_str() {
        "localnet" => parse_blockchain_config(args.config, "localnet").await?,
        "testnet" => parse_blockchain_config(args.config, "testnet").await?,
        "mainnet" => parse_blockchain_config(args.config, "mainnet").await?,
        _ => {
            eprintln!("Unsupported chain `{}`", args.network);
            return Err(Error::UnsupportedChain)
        }
    };

    match args.command {
        Subcmd::Kaching => {
            if !args.fun {
                println!("Apparently you don't like fun...");
                return Ok(())
            }
            kaching().await;
            Ok(())
        }

        Subcmd::Ping => {
            let drk = new_wallet(
                blockchain_config.cache_path,
                blockchain_config.wallet_path,
                blockchain_config.wallet_pass,
                Some(blockchain_config.endpoint),
                ex,
                args.fun,
            )
            .await;
            drk.ping().await?;
            drk.stop_rpc_client().await
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

            let drk = new_wallet(
                blockchain_config.cache_path,
                blockchain_config.wallet_path,
                blockchain_config.wallet_pass,
                None,
                ex,
                args.fun,
            )
            .await;

            if initialize {
                if let Err(e) = drk.initialize_wallet().await {
                    eprintln!("Error initializing wallet: {e:?}");
                    exit(2);
                }
                if let Err(e) = drk.initialize_money().await {
                    eprintln!("Failed to initialize Money: {e:?}");
                    exit(2);
                }
                if let Err(e) = drk.initialize_dao().await {
                    eprintln!("Failed to initialize DAO: {e:?}");
                    exit(2);
                }
                if let Err(e) = drk.initialize_deployooor() {
                    eprintln!("Failed to initialize Deployooor: {e:?}");
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
                if let Err(e) = drk.set_default_address(idx) {
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
                    "User Data",
                    "Spent TX"
                ]);
                for coin in coins {
                    let aliases = match aliases_map.get(&coin.0.note.token_id.to_string()) {
                        Some(a) => a,
                        None => "-",
                    };

                    let spend_hook = if coin.0.note.spend_hook != FuncId::none() {
                        format!("{}", coin.0.note.spend_hook)
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
                        user_data,
                        coin.2
                    ]);
                }

                println!("{table}");

                return Ok(())
            }

            unreachable!()
        }

        Subcmd::Spend => {
            let tx = parse_tx_from_stdin().await?;

            let drk = new_wallet(
                blockchain_config.cache_path,
                blockchain_config.wallet_path,
                blockchain_config.wallet_pass,
                None,
                ex,
                args.fun,
            )
            .await;

            if let Err(e) = drk.mark_tx_spend(&tx).await {
                eprintln!("Failed to mark transaction coins as spent: {e:?}");
                exit(2);
            };

            Ok(())
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
            let drk = new_wallet(
                blockchain_config.cache_path,
                blockchain_config.wallet_path,
                blockchain_config.wallet_pass,
                None,
                ex,
                args.fun,
            )
            .await;
            if let Err(e) = drk.unspend_coin(&coin).await {
                eprintln!("Failed to mark coin as unspent: {e:?}");
                exit(2);
            }

            Ok(())
        }

        Subcmd::Transfer { amount, token, recipient, spend_hook, user_data, half_split } => {
            let drk = new_wallet(
                blockchain_config.cache_path,
                blockchain_config.wallet_path,
                blockchain_config.wallet_pass,
                Some(blockchain_config.endpoint),
                ex,
                args.fun,
            )
            .await;

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

            let tx = match drk
                .transfer(&amount, token_id, rcpt, spend_hook, user_data, half_split)
                .await
            {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Failed to create payment transaction: {e:?}");
                    exit(2);
                }
            };

            println!("{}", base64::encode(&serialize_async(&tx).await));

            drk.stop_rpc_client().await
        }

        Subcmd::Otc { command } => match command {
            OtcSubcmd::Init { value_pair, token_pair } => {
                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    Some(blockchain_config.endpoint),
                    ex,
                    args.fun,
                )
                .await;
                let value_pair = parse_value_pair(&value_pair)?;
                let token_pair = parse_token_pair(&drk, &token_pair).await?;

                let half = match drk.init_swap(value_pair, token_pair, None, None, None).await {
                    Ok(h) => h,
                    Err(e) => {
                        eprintln!("Failed to create swap transaction half: {e:?}");
                        exit(2);
                    }
                };

                println!("{}", base64::encode(&serialize_async(&half).await));
                drk.stop_rpc_client().await
            }

            OtcSubcmd::Join => {
                let mut buf = String::new();
                stdin().read_to_string(&mut buf)?;
                let Some(bytes) = base64::decode(buf.trim()) else {
                    eprintln!("Failed to decode partial swap data");
                    exit(2);
                };

                let partial: PartialSwapData = deserialize_async(&bytes).await?;

                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    Some(blockchain_config.endpoint),
                    ex,
                    args.fun,
                )
                .await;
                let tx = match drk.join_swap(partial, None, None, None).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Failed to create a join swap transaction: {e:?}");
                        exit(2);
                    }
                };

                println!("{}", base64::encode(&serialize_async(&tx).await));
                drk.stop_rpc_client().await
            }

            OtcSubcmd::Inspect => {
                let mut buf = String::new();
                stdin().read_to_string(&mut buf)?;
                let Some(bytes) = base64::decode(buf.trim()) else {
                    eprintln!("Failed to decode swap transaction");
                    exit(2);
                };

                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;
                if let Err(e) = drk.inspect_swap(bytes).await {
                    eprintln!("Failed to inspect swap: {e:?}");
                    exit(2);
                };

                Ok(())
            }

            OtcSubcmd::Sign => {
                let mut tx = parse_tx_from_stdin().await?;

                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;
                if let Err(e) = drk.sign_swap(&mut tx).await {
                    eprintln!("Failed to sign joined swap transaction: {e:?}");
                    exit(2);
                };

                println!("{}", base64::encode(&serialize_async(&tx).await));
                Ok(())
            }
        },

        Subcmd::Dao { command } => match command {
            DaoSubcmd::Create {
                proposer_limit,
                quorum,
                early_exec_quorum,
                approval_ratio,
                gov_token_id,
            } => {
                if let Err(e) = f64::from_str(&proposer_limit) {
                    eprintln!("Invalid proposer limit: {e:?}");
                    exit(2);
                }
                if let Err(e) = f64::from_str(&quorum) {
                    eprintln!("Invalid quorum: {e:?}");
                    exit(2);
                }
                if let Err(e) = f64::from_str(&early_exec_quorum) {
                    eprintln!("Invalid early exec quorum: {e:?}");
                    exit(2);
                }

                let proposer_limit = decode_base10(&proposer_limit, BALANCE_BASE10_DECIMALS, true)?;
                let quorum = decode_base10(&quorum, BALANCE_BASE10_DECIMALS, true)?;
                let early_exec_quorum =
                    decode_base10(&early_exec_quorum, BALANCE_BASE10_DECIMALS, true)?;

                if approval_ratio > 1.0 {
                    eprintln!("Error: Approval ratio cannot be >1.0");
                    exit(2);
                }

                let approval_ratio_base = 100_u64;
                let approval_ratio_quot = (approval_ratio * approval_ratio_base as f64) as u64;

                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;
                let gov_token_id = match drk.get_token(gov_token_id).await {
                    Ok(g) => g,
                    Err(e) => {
                        eprintln!("Invalid Token ID: {e:?}");
                        exit(2);
                    }
                };

                let notes_keypair = Keypair::random(&mut OsRng);
                let proposer_keypair = Keypair::random(&mut OsRng);
                let proposals_keypair = Keypair::random(&mut OsRng);
                let votes_keypair = Keypair::random(&mut OsRng);
                let exec_keypair = Keypair::random(&mut OsRng);
                let early_exec_keypair = Keypair::random(&mut OsRng);
                let bulla_blind = BaseBlind::random(&mut OsRng);

                let params = DaoParams::new(
                    proposer_limit,
                    quorum,
                    early_exec_quorum,
                    approval_ratio_base,
                    approval_ratio_quot,
                    gov_token_id,
                    Some(notes_keypair.secret),
                    notes_keypair.public,
                    Some(proposer_keypair.secret),
                    proposer_keypair.public,
                    Some(proposals_keypair.secret),
                    proposals_keypair.public,
                    Some(votes_keypair.secret),
                    votes_keypair.public,
                    Some(exec_keypair.secret),
                    exec_keypair.public,
                    Some(early_exec_keypair.secret),
                    early_exec_keypair.public,
                    bulla_blind,
                );

                println!("{}", params.toml_str());

                Ok(())
            }

            DaoSubcmd::View => {
                let mut buf = String::new();
                stdin().read_to_string(&mut buf)?;
                let params = DaoParams::from_toml_str(&buf)?;
                println!("{params}");

                Ok(())
            }

            DaoSubcmd::Import { name } => {
                let mut buf = String::new();
                stdin().read_to_string(&mut buf)?;
                let params = DaoParams::from_toml_str(&buf)?;

                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;
                if let Err(e) = drk.import_dao(&name, &params).await {
                    eprintln!("Failed to import DAO: {e:?}");
                    exit(2);
                }

                Ok(())
            }

            DaoSubcmd::UpdateKeys => {
                let mut buf = String::new();
                stdin().read_to_string(&mut buf)?;
                let params = DaoParams::from_toml_str(&buf)?;

                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;
                if let Err(e) = drk.update_dao_keys(&params).await {
                    eprintln!("Failed to update DAO keys: {e:?}");
                    exit(2);
                }

                Ok(())
            }

            DaoSubcmd::List { name } => {
                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;
                if let Err(e) = drk.dao_list(&name).await {
                    eprintln!("Failed to list DAO: {e:?}");
                    exit(2);
                }

                Ok(())
            }

            DaoSubcmd::Balance { name } => {
                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;
                let balmap = match drk.dao_balance(&name).await {
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

            DaoSubcmd::Mint { name } => {
                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    Some(blockchain_config.endpoint),
                    ex,
                    args.fun,
                )
                .await;
                let tx = match drk.dao_mint(&name).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Failed to mint DAO: {e:?}");
                        exit(2);
                    }
                };

                println!("{}", base64::encode(&serialize_async(&tx).await));
                drk.stop_rpc_client().await
            }

            DaoSubcmd::ProposeTransfer {
                name,
                duration,
                amount,
                token,
                recipient,
                spend_hook,
                user_data,
            } => {
                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    Some(blockchain_config.endpoint),
                    ex,
                    args.fun,
                )
                .await;

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

                let proposal = match drk
                    .dao_propose_transfer(
                        &name, duration, &amount, token_id, rcpt, spend_hook, user_data,
                    )
                    .await
                {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("Failed to create DAO transfer proposal: {e:?}");
                        exit(2);
                    }
                };

                println!("Generated proposal: {}", proposal.bulla());

                drk.stop_rpc_client().await
            }

            DaoSubcmd::ProposeGeneric { name, duration, user_data } => {
                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    Some(blockchain_config.endpoint),
                    ex,
                    args.fun,
                )
                .await;

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

                let proposal = match drk.dao_propose_generic(&name, duration, user_data).await {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("Failed to create DAO transfer proposal: {e:?}");
                        exit(2);
                    }
                };

                println!("Generated proposal: {}", proposal.bulla());

                drk.stop_rpc_client().await
            }

            DaoSubcmd::Proposals { name } => {
                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;
                let proposals = drk.get_dao_proposals(&name).await?;

                for (i, proposal) in proposals.iter().enumerate() {
                    println!("{i}. {}", proposal.bulla());
                }

                Ok(())
            }

            DaoSubcmd::Proposal { bulla, export, mint_proposal } => {
                let bulla = match DaoProposalBulla::from_str(&bulla) {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("Invalid proposal bulla: {e:?}");
                        exit(2);
                    }
                };

                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    Some(blockchain_config.endpoint),
                    ex,
                    args.fun,
                )
                .await;
                let proposal = drk.get_dao_proposal_by_bulla(&bulla).await?;

                if export {
                    // Retrieve the DAO
                    let dao = drk.get_dao_by_bulla(&proposal.proposal.dao_bulla).await?;

                    // Encypt the proposal
                    let enc_note = AeadEncryptedNote::encrypt(
                        &proposal,
                        &dao.params.dao.proposals_public_key,
                        &mut OsRng,
                    )
                    .unwrap();

                    // Export it to base64
                    println!("{}", base64::encode(&serialize_async(&enc_note).await));
                    return drk.stop_rpc_client().await
                }

                if mint_proposal {
                    // Identify proposal type by its auth calls
                    for call in &proposal.proposal.auth_calls {
                        // We only support transfer right now
                        if call.function_code == DaoFunction::AuthMoneyTransfer as u8 {
                            let tx = match drk.dao_transfer_proposal_tx(&proposal).await {
                                Ok(tx) => tx,
                                Err(e) => {
                                    eprintln!("Failed to create DAO transfer proposal: {e:?}");
                                    exit(2);
                                }
                            };

                            println!("{}", base64::encode(&serialize_async(&tx).await));
                            return drk.stop_rpc_client().await
                        }
                    }

                    // If proposal has no auth calls, we consider it a generic one
                    if proposal.proposal.auth_calls.is_empty() {
                        let tx = match drk.dao_generic_proposal_tx(&proposal).await {
                            Ok(tx) => tx,
                            Err(e) => {
                                eprintln!("Failed to create DAO generic proposal: {e:?}");
                                exit(2);
                            }
                        };

                        println!("{}", base64::encode(&serialize_async(&tx).await));
                        return drk.stop_rpc_client().await
                    }

                    eprintln!("Unsuported DAO proposal");
                    exit(2);
                }

                println!("{proposal}");

                let mut contract_calls = "\nInvoked contracts:\n".to_string();
                for call in proposal.proposal.auth_calls {
                    contract_calls.push_str(&format!(
                        "\tContract: {}\n\tFunction: {}\n\tData: ",
                        call.contract_id, call.function_code
                    ));

                    if call.auth_data.is_empty() {
                        contract_calls.push_str("-\n");
                        continue;
                    }

                    if call.function_code == DaoFunction::AuthMoneyTransfer as u8 {
                        // We know that the plaintext data live in the data plaintext vec
                        if proposal.data.is_none() {
                            contract_calls.push_str("-\n");
                            continue;
                        }
                        let coin: CoinAttributes =
                            deserialize_async(proposal.data.as_ref().unwrap()).await?;
                        let spend_hook = if coin.spend_hook == FuncId::none() {
                            "-".to_string()
                        } else {
                            format!("{}", coin.spend_hook)
                        };

                        let user_data = if coin.user_data == pallas::Base::ZERO {
                            "-".to_string()
                        } else {
                            format!("{:?}", coin.user_data)
                        };

                        contract_calls.push_str(&format!("\n\t\t{}: {}\n\t\t{}: {} ({})\n\t\t{}: {}\n\t\t{}: {}\n\t\t{}: {}\n\t\t{}: {}\n\n",
                        "Recipient",
                        coin.public_key,
                        "Amount",
                        coin.value,
                        encode_base10(coin.value, BALANCE_BASE10_DECIMALS),
                        "Token",
                        coin.token_id,
                        "Spend hook",
                        spend_hook,
                        "User data",
                        user_data,
                        "Blind",
                        coin.blind));
                    }
                }

                println!("{contract_calls}");

                let votes = drk.get_dao_proposal_votes(&bulla).await?;
                let mut total_yes_vote_value = 0;
                let mut total_no_vote_value = 0;
                let mut total_all_vote_value = 0;
                let mut table = Table::new();
                table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                table.set_titles(row!["Transaction", "Tokens", "Vote"]);
                for vote in votes {
                    let vote_option = if vote.vote_option {
                        total_yes_vote_value += vote.all_vote_value;
                        "Yes"
                    } else {
                        total_no_vote_value += vote.all_vote_value;
                        "No"
                    };
                    total_all_vote_value += vote.all_vote_value;

                    table.add_row(row![
                        vote.tx_hash,
                        encode_base10(vote.all_vote_value, BALANCE_BASE10_DECIMALS),
                        vote_option
                    ]);
                }

                let outcome = if table.is_empty() {
                    println!("Votes: No votes found");
                    "Unknown"
                } else {
                    println!("Votes:");
                    println!("{table}");
                    println!(
                        "Total tokens votes: {}",
                        encode_base10(total_all_vote_value, BALANCE_BASE10_DECIMALS)
                    );
                    let approval_ratio =
                        (total_yes_vote_value as f64 * 100.0) / total_all_vote_value as f64;
                    println!(
                        "Total tokens Yes votes: {} ({approval_ratio:.2}%)",
                        encode_base10(total_yes_vote_value, BALANCE_BASE10_DECIMALS)
                    );
                    println!(
                        "Total tokens No votes: {} ({:.2}%)",
                        encode_base10(total_no_vote_value, BALANCE_BASE10_DECIMALS),
                        (total_no_vote_value as f64 * 100.0) / total_all_vote_value as f64
                    );

                    let dao = drk.get_dao_by_bulla(&proposal.proposal.dao_bulla).await?;
                    if total_all_vote_value >= dao.params.dao.quorum &&
                        approval_ratio >=
                            (dao.params.dao.approval_ratio_quot /
                                dao.params.dao.approval_ratio_base)
                                as f64
                    {
                        "Approved"
                    } else {
                        "Rejected"
                    }
                };

                if let Some(exec_tx_hash) = proposal.exec_tx_hash {
                    println!("Proposal was executed on transaction: {exec_tx_hash}");
                    return drk.stop_rpc_client().await
                }

                // Retrieve next block height and current block time target,
                // to compute their window.
                let next_block_height = drk.get_next_block_height().await?;
                let block_target = drk.get_block_target().await?;
                let current_window = blockwindow(next_block_height, block_target);
                let end_time = proposal.proposal.creation_blockwindow +
                    proposal.proposal.duration_blockwindows;
                let (voting_status, proposal_status_message) = if current_window < end_time {
                    ("Ongoing", format!("Current proposal outcome: {outcome}"))
                } else {
                    ("Concluded", format!("Proposal outcome: {outcome}"))
                };
                println!("Voting status: {voting_status}");
                println!("{proposal_status_message}");

                drk.stop_rpc_client().await
            }

            DaoSubcmd::ProposalImport => {
                let mut buf = String::new();
                stdin().read_to_string(&mut buf)?;
                let Some(bytes) = base64::decode(buf.trim()) else {
                    eprintln!("Failed to decode encrypted proposal data");
                    exit(2);
                };
                let encrypted_proposal: AeadEncryptedNote = deserialize_async(&bytes).await?;

                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;

                // Retrieve all DAOs to try to decrypt the proposal
                let daos = drk.get_daos().await?;
                for dao in &daos {
                    // Check if we have the proposals key
                    let Some(proposals_secret_key) = dao.params.proposals_secret_key else {
                        continue
                    };

                    // Try to decrypt the proposal
                    let Ok(proposal) =
                        encrypted_proposal.decrypt::<ProposalRecord>(&proposals_secret_key)
                    else {
                        continue
                    };

                    let proposal = match drk.get_dao_proposal_by_bulla(&proposal.bulla()).await {
                        Ok(p) => {
                            let mut our_proposal = p;
                            our_proposal.data = proposal.data;
                            our_proposal
                        }
                        Err(_) => proposal,
                    };

                    return drk.put_dao_proposal(&proposal).await
                }

                eprintln!("Couldn't decrypt the proposal with out DAO keys");
                exit(2);
            }

            DaoSubcmd::Vote { bulla, vote, vote_weight } => {
                let bulla = match DaoProposalBulla::from_str(&bulla) {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("Invalid proposal bulla: {e:?}");
                        exit(2);
                    }
                };

                if vote > 1 {
                    eprintln!("Vote can be either 0 (NO) or 1 (YES)");
                    exit(2);
                }
                let vote = vote != 0;

                let weight = match vote_weight {
                    Some(w) => {
                        if let Err(e) = f64::from_str(&w) {
                            eprintln!("Invalid vote weight: {e:?}");
                            exit(2);
                        }
                        Some(decode_base10(&w, BALANCE_BASE10_DECIMALS, true)?)
                    }
                    None => None,
                };

                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    Some(blockchain_config.endpoint),
                    ex,
                    args.fun,
                )
                .await;
                let tx = match drk.dao_vote(&bulla, vote, weight).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Failed to create DAO Vote transaction: {e:?}");
                        exit(2);
                    }
                };

                println!("{}", base64::encode(&serialize_async(&tx).await));
                drk.stop_rpc_client().await
            }

            DaoSubcmd::Exec { bulla, early } => {
                let bulla = match DaoProposalBulla::from_str(&bulla) {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("Invalid proposal bulla: {e:?}");
                        exit(2);
                    }
                };

                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    Some(blockchain_config.endpoint),
                    ex,
                    args.fun,
                )
                .await;
                let proposal = drk.get_dao_proposal_by_bulla(&bulla).await?;

                // Identify proposal type by its auth calls
                for call in &proposal.proposal.auth_calls {
                    // We only support transfer right now
                    if call.function_code == DaoFunction::AuthMoneyTransfer as u8 {
                        let tx = match drk.dao_exec_transfer(&proposal, early).await {
                            Ok(tx) => tx,
                            Err(e) => {
                                eprintln!("Failed to execute DAO transfer proposal: {e:?}");
                                exit(2);
                            }
                        };

                        println!("{}", base64::encode(&serialize_async(&tx).await));
                        return drk.stop_rpc_client().await
                    }
                }

                // If proposal has no auth calls, we consider it a generic one
                if proposal.proposal.auth_calls.is_empty() {
                    let tx = match drk.dao_exec_generic(&proposal, early).await {
                        Ok(tx) => tx,
                        Err(e) => {
                            eprintln!("Failed to execute DAO generic proposal: {e:?}");
                            exit(2);
                        }
                    };

                    println!("{}", base64::encode(&serialize_async(&tx).await));
                    return drk.stop_rpc_client().await
                }

                eprintln!("Unsuported DAO proposal");
                exit(2);
            }

            DaoSubcmd::SpendHook => {
                let spend_hook =
                    FuncRef { contract_id: *DAO_CONTRACT_ID, func_code: DaoFunction::Exec as u8 }
                        .to_func_id();

                println!("{spend_hook}");

                Ok(())
            }
        },

        Subcmd::AttachFee => {
            let mut tx = parse_tx_from_stdin().await?;

            let drk = new_wallet(
                blockchain_config.cache_path,
                blockchain_config.wallet_path,
                blockchain_config.wallet_pass,
                Some(blockchain_config.endpoint),
                ex,
                args.fun,
            )
            .await;
            if let Err(e) = drk.attach_fee(&mut tx).await {
                eprintln!("Failed to attach the fee call to the transaction: {e:?}");
                exit(2);
            };

            println!("{}", base64::encode(&serialize_async(&tx).await));

            drk.stop_rpc_client().await
        }

        Subcmd::Inspect => {
            let tx = parse_tx_from_stdin().await?;

            println!("{tx:#?}");

            Ok(())
        }

        Subcmd::Broadcast => {
            let tx = parse_tx_from_stdin().await?;

            let drk = new_wallet(
                blockchain_config.cache_path,
                blockchain_config.wallet_path,
                blockchain_config.wallet_pass,
                Some(blockchain_config.endpoint),
                ex,
                args.fun,
            )
            .await;

            if let Err(e) = drk.simulate_tx(&tx).await {
                eprintln!("Failed to simulate tx: {e:?}");
                exit(2);
            };

            if let Err(e) = drk.mark_tx_spend(&tx).await {
                eprintln!("Failed to mark transaction coins as spent: {e:?}");
                exit(2);
            };

            let txid = match drk.broadcast_tx(&tx).await {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Failed to broadcast transaction: {e:?}");
                    exit(2);
                }
            };

            println!("Transaction ID: {txid}");

            drk.stop_rpc_client().await
        }

        Subcmd::Subscribe => {
            let drk = new_wallet(
                blockchain_config.cache_path,
                blockchain_config.wallet_path,
                blockchain_config.wallet_pass,
                Some(blockchain_config.endpoint.clone()),
                ex.clone(),
                args.fun,
            )
            .await;

            if let Err(e) = drk.subscribe_blocks(blockchain_config.endpoint, ex).await {
                eprintln!("Block subscription failed: {e:?}");
                exit(2);
            }

            drk.stop_rpc_client().await
        }

        Subcmd::Scan { reset } => {
            let drk = new_wallet(
                blockchain_config.cache_path,
                blockchain_config.wallet_path,
                blockchain_config.wallet_pass,
                Some(blockchain_config.endpoint),
                ex,
                args.fun,
            )
            .await;

            if let Some(height) = reset {
                if let Err(e) = drk.reset_to_height(height).await {
                    eprintln!("Failed during wallet reset: {e:?}");
                    exit(2);
                }
            }

            if let Err(e) = drk.scan_blocks().await {
                eprintln!("Failed during scanning: {e:?}");
                exit(2);
            }
            println!("Finished scanning blockchain");

            drk.stop_rpc_client().await
        }

        Subcmd::Explorer { command } => match command {
            ExplorerSubcmd::FetchTx { tx_hash, full, encode } => {
                let tx_hash = TransactionHash(*blake3::Hash::from_hex(&tx_hash)?.as_bytes());

                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    Some(blockchain_config.endpoint),
                    ex,
                    args.fun,
                )
                .await;

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

                drk.stop_rpc_client().await
            }

            ExplorerSubcmd::SimulateTx => {
                let tx = parse_tx_from_stdin().await?;

                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    Some(blockchain_config.endpoint),
                    ex,
                    args.fun,
                )
                .await;

                let is_valid = match drk.simulate_tx(&tx).await {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("Failed to simulate tx: {e:?}");
                        exit(2);
                    }
                };

                println!("Transaction ID: {}", tx.hash());
                println!("State: {}", if is_valid { "valid" } else { "invalid" });

                drk.stop_rpc_client().await
            }

            ExplorerSubcmd::TxsHistory { tx_hash, encode } => {
                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;

                if let Some(c) = tx_hash {
                    let (tx_hash, status, block_height, tx) = drk.get_tx_history_record(&c).await?;

                    if encode {
                        println!("{}", base64::encode(&serialize_async(&tx).await));
                        exit(1)
                    }

                    println!("Transaction ID: {tx_hash}");
                    println!("Status: {status}");
                    match block_height {
                        Some(block_height) => println!("Block height: {block_height}"),
                        None => println!("Block height: -"),
                    }
                    println!("{tx:?}");

                    return Ok(())
                }

                let map = match drk.get_txs_history() {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Failed to retrieve transactions history records: {e:?}");
                        exit(2);
                    }
                };

                // Create a prettytable with the new data:
                let mut table = Table::new();
                table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                table.set_titles(row!["Transaction Hash", "Status", "Block Height"]);
                for (txs_hash, status, block_height) in map.iter() {
                    let block_height = match block_height {
                        Some(block_height) => block_height.to_string(),
                        None => String::from("-"),
                    };
                    table.add_row(row![txs_hash, status, block_height]);
                }

                if table.is_empty() {
                    println!("No transactions found");
                } else {
                    println!("{table}");
                }

                Ok(())
            }

            ExplorerSubcmd::ClearReverted => {
                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;

                if let Err(e) = drk.remove_reverted_txs() {
                    eprintln!("Failed to remove reverted transactions: {e:?}");
                    exit(2);
                };

                Ok(())
            }

            ExplorerSubcmd::ScannedBlocks { height } => {
                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;

                if let Some(height) = height {
                    let hash = match drk.get_scanned_block_hash(&height) {
                        Ok(h) => h,
                        Err(e) => {
                            eprintln!("Failed to retrieve scanned block record: {e:?}");
                            exit(2);
                        }
                    };

                    println!("Height: {height}");
                    println!("Hash: {hash}");

                    return Ok(())
                }

                let map = match drk.get_scanned_block_records() {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Failed to retrieve scanned blocks records: {e:?}");
                        exit(2);
                    }
                };

                // Create a prettytable with the new data:
                let mut table = Table::new();
                table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                table.set_titles(row!["Height", "Hash"]);
                for (height, hash) in map.iter() {
                    table.add_row(row![height, hash]);
                }

                if table.is_empty() {
                    println!("No scanned blocks records found");
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

                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;
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

                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;
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
                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;
                if let Err(e) = drk.remove_alias(alias).await {
                    eprintln!("Failed to remove alias: {e:?}");
                    exit(2);
                }

                Ok(())
            }
        },

        Subcmd::Token { command } => match command {
            TokenSubcmd::Import { secret_key, token_blind } => {
                let mint_authority = match SecretKey::from_str(&secret_key) {
                    Ok(ma) => ma,
                    Err(e) => {
                        eprintln!("Invalid mint authority: {e:?}");
                        exit(2);
                    }
                };

                let token_blind = match BaseBlind::from_str(&token_blind) {
                    Ok(tb) => tb,
                    Err(e) => {
                        eprintln!("Invalid token blind: {e:?}");
                        exit(2);
                    }
                };

                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;
                let token_id = drk.import_mint_authority(mint_authority, token_blind).await?;
                println!("Successfully imported mint authority for token ID: {token_id}");

                Ok(())
            }

            TokenSubcmd::GenerateMint => {
                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;
                let mint_authority = SecretKey::random(&mut OsRng);
                let token_blind = BaseBlind::random(&mut OsRng);
                let token_id = drk.import_mint_authority(mint_authority, token_blind).await?;
                println!("Successfully imported mint authority for token ID: {token_id}");

                Ok(())
            }

            TokenSubcmd::List => {
                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;
                let tokens = drk.get_mint_authorities().await?;
                let aliases_map = match drk.get_aliases_mapped_by_token().await {
                    Ok(map) => map,
                    Err(e) => {
                        eprintln!("Failed to fetch wallet aliases: {e:?}");
                        exit(2);
                    }
                };

                let mut table = Table::new();
                table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                table.set_titles(row![
                    "Token ID",
                    "Aliases",
                    "Mint Authority",
                    "Token Blind",
                    "Frozen"
                ]);

                for (token_id, authority, blind, frozen) in tokens {
                    let aliases = match aliases_map.get(&token_id.to_string()) {
                        Some(a) => a,
                        None => "-",
                    };

                    table.add_row(row![token_id, aliases, authority, blind, frozen]);
                }

                if table.is_empty() {
                    println!("No tokens found");
                } else {
                    println!("{table}");
                }

                Ok(())
            }

            TokenSubcmd::Mint { token, amount, recipient, spend_hook, user_data } => {
                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    Some(blockchain_config.endpoint),
                    ex,
                    args.fun,
                )
                .await;

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
                        eprintln!("Invalid Token ID: {e:?}");
                        exit(2);
                    }
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

                let tx = match drk.mint_token(&amount, rcpt, token_id, spend_hook, user_data).await
                {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Failed to create token mint transaction: {e:?}");
                        exit(2);
                    }
                };

                println!("{}", base64::encode(&serialize_async(&tx).await));

                drk.stop_rpc_client().await
            }

            TokenSubcmd::Freeze { token } => {
                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    Some(blockchain_config.endpoint),
                    ex,
                    args.fun,
                )
                .await;
                let token_id = match drk.get_token(token).await {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("Invalid Token ID: {e:?}");
                        exit(2);
                    }
                };

                let tx = match drk.freeze_token(token_id).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Failed to create token freeze transaction: {e:?}");
                        exit(2);
                    }
                };

                println!("{}", base64::encode(&serialize_async(&tx).await));

                drk.stop_rpc_client().await
            }
        },

        Subcmd::Contract { command } => match command {
            ContractSubcmd::GenerateDeploy => {
                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;

                if let Err(e) = drk.deploy_auth_keygen().await {
                    eprintln!("Error creating deploy auth keypair: {e}");
                    exit(2);
                }

                Ok(())
            }

            ContractSubcmd::List => {
                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    None,
                    ex,
                    args.fun,
                )
                .await;
                let auths = drk.list_deploy_auth().await?;

                let mut table = Table::new();
                table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                table.set_titles(row!["Index", "Contract ID", "Frozen"]);

                for (idx, contract_id, frozen) in auths {
                    table.add_row(row![idx, contract_id, frozen]);
                }

                if table.is_empty() {
                    println!("No deploy authorities found");
                } else {
                    println!("{table}");
                }

                Ok(())
            }

            ContractSubcmd::Deploy { deploy_auth, wasm_path, deploy_ix } => {
                // Read the wasm bincode and deploy instruction
                let wasm_bin = smol::fs::read(expand_path(&wasm_path)?).await?;
                let deploy_ix = smol::fs::read(expand_path(&deploy_ix)?).await?;

                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    Some(blockchain_config.endpoint),
                    ex,
                    args.fun,
                )
                .await;

                let mut tx = match drk.deploy_contract(deploy_auth, wasm_bin, deploy_ix).await {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("Error creating contract deployment tx: {e}");
                        exit(2);
                    }
                };

                if let Err(e) = drk.attach_fee(&mut tx).await {
                    eprintln!("Failed to attach the fee call to the transaction: {e:?}");
                    exit(2);
                };

                println!("{}", base64::encode(&serialize_async(&tx).await));

                drk.stop_rpc_client().await
            }

            ContractSubcmd::Lock { deploy_auth } => {
                let drk = new_wallet(
                    blockchain_config.cache_path,
                    blockchain_config.wallet_path,
                    blockchain_config.wallet_pass,
                    Some(blockchain_config.endpoint),
                    ex,
                    args.fun,
                )
                .await;

                let mut tx = match drk.lock_contract(deploy_auth).await {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("Error creating contract lock tx: {e}");
                        exit(2);
                    }
                };

                if let Err(e) = drk.attach_fee(&mut tx).await {
                    eprintln!("Failed to attach the fee call to the transaction: {e:?}");
                    exit(2);
                };

                println!("{}", base64::encode(&serialize_async(&tx).await));

                drk.stop_rpc_client().await
            }
        },
    }
}
