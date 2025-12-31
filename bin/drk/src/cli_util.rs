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
    collections::{HashMap, HashSet},
    io::{stdin, Cursor, Read},
    slice,
    str::FromStr,
};

use rodio::{Decoder, OutputStreamBuilder, Sink};
use smol::channel::Sender;
use structopt_toml::clap::{App, Arg, Shell, SubCommand};

use darkfi::{
    cli_desc,
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    util::{encoding::base64, parse::decode_base10},
    zk::Proof,
    Error, Result,
};
use darkfi_money_contract::model::TokenId;
use darkfi_sdk::{
    crypto::{keypair::Address, pasta_prelude::PrimeField, FuncId, SecretKey},
    dark_tree::DarkTree,
    pasta::pallas,
    ContractCallImport,
};
use darkfi_serial::deserialize_async;

use crate::{money::BALANCE_BASE10_DECIMALS, Drk};

/// Auxiliary function to parse a base64 encoded transaction from stdin.
pub async fn parse_tx_from_stdin() -> Result<Transaction> {
    let mut buf = String::new();
    stdin().read_to_string(&mut buf)?;
    match base64::decode(buf.trim()) {
        Some(bytes) => Ok(deserialize_async(&bytes).await?),
        None => Err(Error::ParseFailed("Failed to decode transaction")),
    }
}

/// Auxiliary function to parse base64-encoded contract calls from stdin.
pub async fn parse_calls_from_stdin() -> Result<Vec<ContractCallImport>> {
    let lines = stdin().lines();

    let mut calls = vec![];

    for line in lines {
        let Some(line) = base64::decode(&line?) else {
            return Err(Error::ParseFailed("Failed to decode base64"))
        };
        calls.push(deserialize_async(&line).await?);
    }

    Ok(calls)
}

/// Auxiliary function to parse a base64 encoded transaction from
/// provided input or fallback to stdin if its empty.
pub async fn parse_tx_from_input(input: &[String]) -> Result<Transaction> {
    match input.len() {
        0 => parse_tx_from_stdin().await,
        1 => match base64::decode(input[0].trim()) {
            Some(bytes) => Ok(deserialize_async(&bytes).await?),
            None => Err(Error::ParseFailed("Failed to decode transaction")),
        },
        _ => Err(Error::ParseFailed("Multiline input provided")),
    }
}

/// Auxiliary function to parse provided string into a values pair.
pub fn parse_value_pair(s: &str) -> Result<(u64, u64)> {
    let v: Vec<&str> = s.split(':').collect();
    if v.len() != 2 {
        return Err(Error::ParseFailed("Invalid value pair. Use a pair such as 13.37:11.0"))
    }

    let val0 = decode_base10(v[0], BALANCE_BASE10_DECIMALS, true);
    let val1 = decode_base10(v[1], BALANCE_BASE10_DECIMALS, true);

    if val0.is_err() || val1.is_err() {
        return Err(Error::ParseFailed("Invalid value pair. Use a pair such as 13.37:11.0"))
    }

    Ok((val0.unwrap(), val1.unwrap()))
}

/// Auxiliary function to parse provided string into a tokens pair.
pub async fn parse_token_pair(drk: &Drk, s: &str) -> Result<(TokenId, TokenId)> {
    let v: Vec<&str> = s.split(':').collect();
    if v.len() != 2 {
        return Err(Error::ParseFailed(
            "Invalid token pair. Use a pair such as:\nWCKD:MLDY\nor\n\
            A7f1RKsCUUHrSXA7a9ogmwg8p3bs6F47ggsW826HD4yd:FCuoMii64H5Ee4eVWBjP18WTFS8iLUJmGi16Qti1xFQ2"
        ))
    }

    let tok0 = drk.get_token(v[0].to_string()).await;
    let tok1 = drk.get_token(v[1].to_string()).await;

    if tok0.is_err() || tok1.is_err() {
        return Err(Error::ParseFailed(
            "Invalid token pair. Use a pair such as:\nWCKD:MLDY\nor\n\
            A7f1RKsCUUHrSXA7a9ogmwg8p3bs6F47ggsW826HD4yd:FCuoMii64H5Ee4eVWBjP18WTFS8iLUJmGi16Qti1xFQ2"
        ))
    }

    Ok((tok0.unwrap(), tok1.unwrap()))
}

/// Fun police go away
pub async fn kaching() {
    const WALLET_MP3: &[u8] = include_bytes!("../wallet.mp3");

    let cursor = Cursor::new(WALLET_MP3);

    let Ok(stream_handle) = OutputStreamBuilder::open_default_stream() else { return };
    let sink = Sink::connect_new(stream_handle.mixer());

    let Ok(source) = Decoder::new(cursor) else { return };
    sink.append(source);

    sink.sleep_until_end();
}

/// Auxiliary function to generate provided shell completions.
pub fn generate_completions(shell: &str) -> Result<String> {
    // Sub-commands

    // Interactive
    let interactive = SubCommand::with_name("interactive").about("Enter Drk interactive shell");

    // Kaching
    let kaching = SubCommand::with_name("kaching").about("Fun");

    // Ping
    let ping =
        SubCommand::with_name("ping").about("Send a ping request to the darkfid RPC endpoint");

    // Completions
    let shell_arg = Arg::with_name("shell").help("The Shell you want to generate script for");

    let completions = SubCommand::with_name("completions")
        .about("Generate a SHELL completion script and print to stdout")
        .arg(shell_arg);

    // Wallet
    let initialize = SubCommand::with_name("initialize").about("Initialize wallet database");

    let keygen = SubCommand::with_name("keygen").about("Generate a new keypair in the wallet");

    let balance = SubCommand::with_name("balance").about("Query the wallet for known balances");

    let address = SubCommand::with_name("address").about("Get the default address in the wallet");

    let addresses =
        SubCommand::with_name("addresses").about("Print all the addresses in the wallet");

    let index = Arg::with_name("index").help("Identifier of the address");

    let default_address = SubCommand::with_name("default-address")
        .about("Set the default address in the wallet")
        .arg(index.clone());

    let secrets =
        SubCommand::with_name("secrets").about("Print all the secret keys from the wallet");

    let import_secrets = SubCommand::with_name("import-secrets")
        .about("Import secret keys from stdin into the wallet, separated by newlines");

    let tree = SubCommand::with_name("tree").about("Print the Merkle tree in the wallet");

    let coins = SubCommand::with_name("coins").about("Print all the coins in the wallet");

    let spend_hook = Arg::with_name("spend-hook").help("Optional contract spend hook to use");

    let user_data = Arg::with_name("user-data").help("Optional user data to use");

    let mining_config = SubCommand::with_name("mining-config")
        .about("Print a wallet address mining configuration")
        .args(&[index, spend_hook.clone(), user_data.clone()]);

    let wallet = SubCommand::with_name("wallet").about("Wallet operations").subcommands(vec![
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
        mining_config,
    ]);

    // Spend
    let spend = SubCommand::with_name("spend")
        .about("Read a transaction from stdin and mark its input coins as spent");

    // Unspend
    let coin = Arg::with_name("coin").help("base64-encoded coin to mark as unspent");

    let unspend = SubCommand::with_name("unspend").about("Unspend a coin").arg(coin);

    // Transfer
    let amount = Arg::with_name("amount").help("Amount to send");

    let token = Arg::with_name("token").help("Token ID to send");

    let recipient = Arg::with_name("recipient").help("Recipient address");

    let half_split = Arg::with_name("half-split")
        .long("half-split")
        .help("Split the output coin into two equal halves");

    let transfer = SubCommand::with_name("transfer").about("Create a payment transaction").args(&[
        amount.clone(),
        token.clone(),
        recipient.clone(),
        spend_hook.clone(),
        user_data.clone(),
        half_split,
    ]);

    // Otc
    let value_pair = Arg::with_name("value-pair")
        .short("v")
        .long("value-pair")
        .takes_value(true)
        .help("Value pair to send:recv (11.55:99.42)");

    let token_pair = Arg::with_name("token-pair")
        .short("t")
        .long("token-pair")
        .takes_value(true)
        .help("Token pair to send:recv (f00:b4r)");

    let init = SubCommand::with_name("init")
        .about("Initialize the first half of the atomic swap")
        .args(&[value_pair, token_pair]);

    let join =
        SubCommand::with_name("join").about("Build entire swap tx given the first half from stdin");

    let inspect = SubCommand::with_name("inspect")
        .about("Inspect a swap half or the full swap tx from stdin");

    let sign = SubCommand::with_name("sign").about("Sign a swap transaction given from stdin");

    let otc = SubCommand::with_name("otc")
        .about("OTC atomic swap")
        .subcommands(vec![init, join, inspect, sign]);

    // DAO
    let proposer_limit = Arg::with_name("proposer-limit")
        .help("The minimum amount of governance tokens needed to open a proposal for this DAO");

    let quorum = Arg::with_name("quorum")
        .help("Minimal threshold of participating total tokens needed for a proposal to pass");

    let early_exec_quorum = Arg::with_name("early-exec-quorum")
        .help("Minimal threshold of participating total tokens needed for a proposal to be considered as strongly supported, enabling early execution. Must be greater or equal to normal quorum.");

    let approval_ratio = Arg::with_name("approval-ratio")
        .help("The ratio of winning votes/total votes needed for a proposal to pass (2 decimals)");

    let gov_token_id = Arg::with_name("gov-token-id").help("DAO's governance token ID");

    let create = SubCommand::with_name("create").about("Create DAO parameters").args(&[
        proposer_limit,
        quorum,
        early_exec_quorum,
        approval_ratio,
        gov_token_id,
    ]);

    let view = SubCommand::with_name("view").about("View DAO data from stdin");

    let name = Arg::with_name("name").help("Name identifier for the DAO");

    let import = SubCommand::with_name("import")
        .about("Import DAO data from stdin")
        .args(slice::from_ref(&name));

    let opt_name = Arg::with_name("dao-alias").help("Name identifier for the DAO (optional)");

    let list = SubCommand::with_name("list")
        .about("List imported DAOs (or info about a specific one)")
        .args(&[opt_name]);

    let balance = SubCommand::with_name("balance")
        .about("Show the balance of a DAO")
        .args(slice::from_ref(&name));

    let mint = SubCommand::with_name("mint")
        .about("Mint an imported DAO on-chain")
        .args(slice::from_ref(&name));

    let duration = Arg::with_name("duration").help("Duration of the proposal, in block windows");

    let propose_transfer = SubCommand::with_name("propose-transfer")
        .about("Create a transfer proposal for a DAO")
        .args(&[
            name.clone(),
            duration.clone(),
            amount,
            token,
            recipient,
            spend_hook.clone(),
            user_data.clone(),
        ]);

    let propose_generic = SubCommand::with_name("propose-generic")
        .about("Create a generic proposal for a DAO")
        .args(&[name.clone(), duration, user_data.clone()]);

    let proposals = SubCommand::with_name("proposals").about("List DAO proposals").arg(&name);

    let bulla = Arg::with_name("bulla").help("Bulla identifier for the proposal");

    let export = Arg::with_name("export").help("Encrypt the proposal and encode it to base64");

    let mint_proposal = Arg::with_name("mint-proposal").help("Create the proposal transaction");

    let proposal = SubCommand::with_name("proposal").about("View a DAO proposal data").args(&[
        bulla.clone(),
        export,
        mint_proposal,
    ]);

    let proposal_import = SubCommand::with_name("proposal-import")
        .about("Import a base64 encoded and encrypted proposal from stdin");

    let vote = Arg::with_name("vote").help("Vote (0 for NO, 1 for YES)");

    let vote_weight =
        Arg::with_name("vote-weight").help("Optional vote weight (amount of governance tokens)");

    let vote = SubCommand::with_name("vote").about("Vote on a given proposal").args(&[
        bulla.clone(),
        vote,
        vote_weight,
    ]);

    let early = Arg::with_name("early").long("early").help("Execute the proposal early");

    let exec = SubCommand::with_name("exec").about("Execute a DAO proposal").args(&[bulla, early]);

    let spend_hook_cmd = SubCommand::with_name("spend-hook")
        .about("Print the DAO contract base64-encoded spend hook");

    let mining_config =
        SubCommand::with_name("mining-config").about("Print a DAO mining configuration").arg(name);

    let dao = SubCommand::with_name("dao").about("DAO functionalities").subcommands(vec![
        create,
        view,
        import,
        list,
        balance,
        mint,
        propose_transfer,
        propose_generic,
        proposals,
        proposal,
        proposal_import,
        vote,
        exec,
        spend_hook_cmd,
        mining_config,
    ]);

    // AttachFee
    let attach_fee = SubCommand::with_name("attach-fee")
        .about("Attach the fee call to a transaction given from stdin");

    // Inspect
    let inspect = SubCommand::with_name("inspect").about("Inspect a transaction from stdin");

    // Broadcast
    let broadcast =
        SubCommand::with_name("broadcast").about("Read a transaction from stdin and broadcast it");

    // Scan
    let reset = Arg::with_name("reset")
        .long("reset")
        .help("Reset wallet state to provided block height and start scanning");

    let scan = SubCommand::with_name("scan")
        .about("Scan the blockchain and parse relevant transactions")
        .args(&[reset]);

    // Explorer
    let tx_hash = Arg::with_name("tx-hash").help("Transaction hash");

    let encode = Arg::with_name("encode").long("encode").help("Encode transaction to base64");

    let fetch_tx = SubCommand::with_name("fetch-tx")
        .about("Fetch a blockchain transaction by hash")
        .args(&[tx_hash, encode]);

    let simulate_tx =
        SubCommand::with_name("simulate-tx").about("Read a transaction from stdin and simulate it");

    let tx_hash = Arg::with_name("tx-hash").help("Fetch specific history record (optional)");

    let encode = Arg::with_name("encode")
        .long("encode")
        .help("Encode specific history record transaction to base64");

    let txs_history = SubCommand::with_name("txs-history")
        .about("Fetch broadcasted transactions history")
        .args(&[tx_hash, encode]);

    let clear_reverted =
        SubCommand::with_name("clear-reverted").about("Remove reverted transactions from history");

    let height = Arg::with_name("height").help("Fetch specific height record (optional)");

    let scanned_blocks = SubCommand::with_name("scanned-blocks")
        .about("Fetch scanned blocks records")
        .args(&[height]);

    let mining_config = SubCommand::with_name("mining-config")
        .about("Read a mining configuration from stdin and display its parts");

    let explorer =
        SubCommand::with_name("explorer").about("Explorer related subcommands").subcommands(vec![
            fetch_tx,
            simulate_tx,
            txs_history,
            clear_reverted,
            scanned_blocks,
            mining_config,
        ]);

    // Alias
    let alias = Arg::with_name("alias").help("Token alias");

    let token = Arg::with_name("token").help("Token to create alias for");

    let add = SubCommand::with_name("add").about("Create a Token alias").args(&[alias, token]);

    let alias = Arg::with_name("alias")
        .short("a")
        .long("alias")
        .takes_value(true)
        .help("Token alias to search for");

    let token = Arg::with_name("token")
        .short("t")
        .long("token")
        .takes_value(true)
        .help("Token to search alias for");

    let show = SubCommand::with_name("show")
        .about(
            "Print alias info of optional arguments. \
                    If no argument is provided, list all the aliases in the wallet.",
        )
        .args(&[alias, token]);

    let alias = Arg::with_name("alias").help("Token alias to remove");

    let remove = SubCommand::with_name("remove").about("Remove a Token alias").arg(alias);

    let alias = SubCommand::with_name("alias")
        .about("Manage Token aliases")
        .subcommands(vec![add, show, remove]);

    // Token
    let secret_key = Arg::with_name("secret-key").help("Mint authority secret key");

    let token_blind = Arg::with_name("token-blind").help("Mint authority token blind");

    let import = SubCommand::with_name("import")
        .about("Import a mint authority")
        .args(&[secret_key, token_blind]);

    let generate_mint =
        SubCommand::with_name("generate-mint").about("Generate a new mint authority");

    let list =
        SubCommand::with_name("list").about("List token IDs with available mint authorities");

    let token = Arg::with_name("token").help("Token ID to mint");

    let amount = Arg::with_name("amount").help("Amount to mint");

    let recipient = Arg::with_name("recipient").help("Recipient of the minted tokens");

    let mint = SubCommand::with_name("mint")
        .about("Mint tokens")
        .args(&[token, amount, recipient, spend_hook, user_data]);

    let token = Arg::with_name("token").help("Token ID to freeze");

    let freeze = SubCommand::with_name("freeze").about("Freeze a token mint").arg(token);

    let token = SubCommand::with_name("token").about("Token functionalities").subcommands(vec![
        import,
        generate_mint,
        list,
        mint,
        freeze,
    ]);

    // Contract
    let generate_deploy =
        SubCommand::with_name("generate-deploy").about("Generate a new deploy authority");

    let contract_id = Arg::with_name("contract-id").help("Contract ID (optional)");

    let list = SubCommand::with_name("list")
        .about("List deploy authorities in the wallet (or a specific one)")
        .args(&[contract_id]);

    let tx_hash = Arg::with_name("tx-hash").help("Record transaction hash");

    let export_data = SubCommand::with_name("export-data")
        .about("Export a contract history record wasm bincode and deployment instruction, encoded to base64")
        .args(&[tx_hash]);

    let deploy_auth = Arg::with_name("deploy-auth").help("Contract ID (deploy authority)");

    let wasm_path = Arg::with_name("wasm-path").help("Path to contract wasm bincode");

    let deploy_ix =
        Arg::with_name("deploy-ix").help("Optional path to serialized deploy instruction");

    let deploy = SubCommand::with_name("deploy").about("Deploy a smart contract").args(&[
        deploy_auth.clone(),
        wasm_path,
        deploy_ix,
    ]);

    let lock = SubCommand::with_name("lock").about("Lock a smart contract").args(&[deploy_auth]);

    let contract = SubCommand::with_name("contract")
        .about("Contract functionalities")
        .subcommands(vec![generate_deploy, list, export_data, deploy, lock]);

    // Main arguments
    let config = Arg::with_name("config")
        .short("c")
        .long("config")
        .takes_value(true)
        .help("Configuration file to use");

    let network = Arg::with_name("network")
        .long("network")
        .takes_value(true)
        .help("Blockchain network to use");

    let command = vec![
        interactive,
        kaching,
        ping,
        completions,
        wallet,
        spend,
        unspend,
        transfer,
        otc,
        attach_fee,
        inspect,
        broadcast,
        dao,
        scan,
        explorer,
        alias,
        token,
        contract,
    ];

    let fun = Arg::with_name("fun")
        .short("f")
        .long("fun")
        .help("Flag indicating whether you want some fun in your life");

    let log = Arg::with_name("log")
        .short("l")
        .long("log")
        .takes_value(true)
        .help("Set log file to ouput into");

    let verbose = Arg::with_name("verbose")
        .short("v")
        .multiple(true)
        .help("Increase verbosity (-vvv supported)");

    let mut app = App::new("drk")
        .about(cli_desc!())
        .args(&[config, network, fun, log, verbose])
        .subcommands(command);

    let shell = match Shell::from_str(shell) {
        Ok(s) => s,
        Err(e) => return Err(Error::Custom(e)),
    };

    let mut buf = vec![];
    app.gen_completions_to("./drk", shell, &mut buf);

    Ok(String::from_utf8(buf)?)
}

/// Auxiliary function to print provided string buffer.
pub fn print_output(buf: &[String]) {
    for line in buf {
        println!("{line}");
    }
}

/// Auxiliary function to print or insert provided messages to given
/// buffer reference. If a channel sender is provided, the messages
/// are send to that instead.
pub async fn append_or_print(
    buf: &mut Vec<String>,
    sender: Option<&Sender<Vec<String>>>,
    print: &bool,
    messages: Vec<String>,
) {
    // Send the messages to the channel, if provided
    if let Some(sender) = sender {
        if let Err(e) = sender.send(messages).await {
            let err_msg = format!("[append_or_print] Sending messages to channel failed: {e}");
            if *print {
                println!("{err_msg}");
            } else {
                buf.push(err_msg);
            }
        }
        return
    }

    // Print the messages
    if *print {
        for msg in messages {
            println!("{msg}");
        }
        return
    }

    // Insert the messages in the buffer
    for msg in messages {
        buf.push(msg);
    }
}

/// Auxiliary function to parse a base64 encoded mining configuration
/// from stdin.
pub async fn parse_mining_config_from_stdin(
) -> Result<(String, String, Option<String>, Option<String>)> {
    let mut buf = String::new();
    stdin().read_to_string(&mut buf)?;
    let config = buf.trim();
    let (recipient, spend_hook, user_data) = match base64::decode(config) {
        Some(bytes) => deserialize_async(&bytes).await?,
        None => return Err(Error::ParseFailed("Failed to decode mining configuration")),
    };
    Ok((config.to_string(), recipient, spend_hook, user_data))
}

/// Auxiliary function to parse a base64 encoded mining configuration
/// from provided input or fallback to stdin if its empty.
pub async fn parse_mining_config_from_input(
    input: &[String],
) -> Result<(String, String, Option<String>, Option<String>)> {
    match input.len() {
        0 => parse_mining_config_from_stdin().await,
        1 => {
            let config = input[0].trim();
            let (recipient, spend_hook, user_data) = match base64::decode(config) {
                Some(bytes) => deserialize_async(&bytes).await?,
                None => return Err(Error::ParseFailed("Failed to decode mining configuration")),
            };
            Ok((config.to_string(), recipient, spend_hook, user_data))
        }
        _ => Err(Error::ParseFailed("Multiline input provided")),
    }
}

/// Auxiliary function to display the parts of a mining configuration.
pub fn display_mining_config(
    config: &str,
    recipient_str: &str,
    spend_hook: &Option<String>,
    user_data: &Option<String>,
    output: &mut Vec<String>,
) {
    output.push(format!("DarkFi mining configuration address: {config}"));

    match Address::from_str(recipient_str) {
        Ok(recipient) => {
            output.push(format!("Recipient: {recipient_str}"));
            output.push(format!("Public key: {}", recipient.public_key()));
            output.push(format!("Network: {:?}", recipient.network()));
        }
        Err(e) => output.push(format!("Recipient: Invalid ({e})")),
    }

    let spend_hook = match spend_hook {
        Some(spend_hook_str) => match FuncId::from_str(spend_hook_str) {
            Ok(_) => String::from(spend_hook_str),
            Err(e) => format!("Invalid ({e})"),
        },
        None => String::from("-"),
    };
    output.push(format!("Spend hook: {spend_hook}"));

    let user_data = match user_data {
        Some(user_data_str) => match bs58::decode(&user_data_str).into_vec() {
            Ok(bytes) => match bytes.try_into() {
                Ok(bytes) => {
                    if pallas::Base::from_repr(bytes).is_some().into() {
                        String::from(user_data_str)
                    } else {
                        String::from("Invalid")
                    }
                }
                Err(e) => format!("Invalid ({e:?})"),
            },
            Err(e) => format!("Invalid ({e})"),
        },
        None => String::from("-"),
    };
    output.push(format!("User data: {user_data}"));
}

/// Cast `ContractCallImport` to `ContractCallLeaf`
fn to_leaf(call: &ContractCallImport) -> ContractCallLeaf {
    ContractCallLeaf {
        call: call.call().clone(),
        proofs: call.proofs().iter().map(|p| Proof::new(p.clone())).collect(),
    }
}

/// Recursively build subtree for a DarkTree
fn build_subtree(
    idx: usize,
    calls: &[ContractCallImport],
    children_map: &HashMap<usize, &Vec<usize>>,
) -> DarkTree<ContractCallLeaf> {
    let children_idx = children_map.get(&idx).map(|v| v.as_slice()).unwrap_or(&[]);

    let children: Vec<DarkTree<ContractCallLeaf>> =
        children_idx.iter().map(|&i| build_subtree(i, calls, children_map)).collect();

    DarkTree::new(to_leaf(&calls[idx]), children, None, None)
}

/// Build a `Transaction` given a slice of calls and their mapping
pub fn tx_from_calls_mapped(
    calls: &[ContractCallImport],
    map: &[(usize, Vec<usize>)],
) -> Result<(TransactionBuilder, Vec<SecretKey>)> {
    assert_eq!(calls.len(), map.len());

    let signature_secrets: Vec<SecretKey> =
        calls.iter().flat_map(|c| c.secrets().to_vec()).collect();

    let children_map: HashMap<usize, &Vec<usize>> = map.iter().map(|(k, v)| (*k, v)).collect();

    let (root_idx, root_children_idx) = &map[0];

    let root_children: Vec<DarkTree<ContractCallLeaf>> =
        root_children_idx.iter().map(|&i| build_subtree(i, calls, &children_map)).collect();

    let tx_builder = TransactionBuilder::new(to_leaf(&calls[*root_idx]), root_children)?;

    Ok((tx_builder, signature_secrets))
}

/// Auxiliary function to parse a contract call mapping.
///
/// The mapping is in the format of `{0: [1,2], 1: [], 2:[3], 3:[]}`.
/// It supports nesting and this kind of logic as expected.
///
/// Errors out if there are non-unique keys or cyclic references.
pub fn parse_tree(input: &str) -> std::result::Result<Vec<(usize, Vec<usize>)>, String> {
    let s = input
        .trim()
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or("expected {}")?
        .trim();

    let mut entries = vec![];
    let mut seen_keys = HashSet::new();

    if s.is_empty() {
        return Ok(entries)
    }

    let mut rest = s;
    while !rest.is_empty() {
        // Parse key
        let (key_str, after_key) = rest.split_once(':').ok_or("expected ':'")?;
        let key: usize = key_str.trim().parse().map_err(|_| "invalid key")?;

        if !seen_keys.insert(key) {
            return Err(format!("duplicate key: {}", key));
        }

        // Parse array
        let after_key = after_key.trim();
        let arr_start = after_key.strip_prefix('[').ok_or("expected '['")?;
        let (arr_content, after_arr) = arr_start.split_once(']').ok_or("expected ']'")?;

        let children: Vec<usize> = arr_content
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.parse().map_err(|_| "invalid child"))
            .collect::<std::result::Result<_, _>>()?;

        entries.push((key, children));

        // Move to next entry
        rest = after_arr.trim().strip_prefix(',').unwrap_or(after_arr).trim();
    }

    check_cycles(&entries)?;

    Ok(entries)
}

fn check_cycles(entries: &[(usize, Vec<usize>)]) -> std::result::Result<(), String> {
    let graph: HashMap<usize, &Vec<usize>> = entries.iter().map(|(k, v)| (*k, v)).collect();
    let mut visited = HashSet::new();
    let mut path = Vec::new();

    fn dfs(
        node: usize,
        graph: &HashMap<usize, &Vec<usize>>,
        visited: &mut HashSet<usize>,
        path: &mut Vec<usize>,
    ) -> std::result::Result<(), String> {
        if let Some(pos) = path.iter().position(|&n| n == node) {
            let cycle: Vec<_> = path[pos..].iter().chain(&[node]).map(|n| n.to_string()).collect();
            return Err(format!("cycle detected: {}", cycle.join(" -> ")));
        }

        if visited.contains(&node) {
            return Ok(());
        }

        path.push(node);
        if let Some(children) = graph.get(&node) {
            for &child in *children {
                dfs(child, graph, visited, path)?;
            }
        }
        path.pop();
        visited.insert(node);

        Ok(())
    }

    for &(key, _) in entries {
        dfs(key, &graph, &mut visited, &mut path)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tree() {
        // Valid inputs
        assert_eq!(parse_tree("{}").unwrap(), vec![]);
        assert_eq!(parse_tree("{  }").unwrap(), vec![]);
        assert_eq!(parse_tree("{ 0: [] }").unwrap(), vec![(0, vec![])]);
        assert_eq!(parse_tree("{ 0: [1, 2, 3] }").unwrap(), vec![(0, vec![1, 2, 3])]);
        assert_eq!(parse_tree("{0:[],1:[2]}").unwrap(), vec![(0, vec![]), (1, vec![2])]);
        assert_eq!(parse_tree("{ 0: [], 1: [], }").unwrap(), vec![(0, vec![]), (1, vec![])]);
        assert_eq!(parse_tree("{ 0: [1, 2,] }").unwrap(), vec![(0, vec![1, 2])]);

        assert_eq!(
            parse_tree("{ 0: [], 1: [2, 3], 2: [], 3: [4], 4: [] }").unwrap(),
            vec![(0, vec![]), (1, vec![2, 3]), (2, vec![]), (3, vec![4]), (4, vec![])]
        );

        assert_eq!(
            parse_tree("{   0  :  [  ]  ,   1  :  [  2  ,  3  ]   }").unwrap(),
            vec![(0, vec![]), (1, vec![2, 3])]
        );

        assert_eq!(
            parse_tree("{ 999: [1000, 1001], 1000: [], 1001: [] }").unwrap(),
            vec![(999, vec![1000, 1001]), (1000, vec![]), (1001, vec![])]
        );

        // Order preservation
        let keys: Vec<usize> =
            parse_tree("{ 5: [], 2: [], 9: [], 0: [] }").unwrap().iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec![5, 2, 9, 0]);

        // Valid DAG (not a cycle)
        assert!(parse_tree("{ 0: [1, 2], 1: [3], 2: [3], 3: [] }").is_ok());

        // Syntax errors
        assert!(parse_tree("0: [] }").is_err());
        assert!(parse_tree("{ 0: []").is_err());
        assert!(parse_tree("{ 0 [] }").is_err());
        assert!(parse_tree("{ 0: ] }").is_err());
        assert!(parse_tree("{ 0: [1, 2 }").is_err());
        assert!(parse_tree("{ abc: [] }").is_err());
        assert!(parse_tree("{ 0: [abc] }").is_err());
        assert!(parse_tree("{ -1: [] }").is_err());

        // Duplicate keys
        assert!(parse_tree("{ 0: [], 0: [1] }").unwrap_err().contains("duplicate key: 0"));
        assert!(parse_tree("{ 0: [], 1: [], 2: [], 1: [] }")
            .unwrap_err()
            .contains("duplicate key: 1"));

        // Cycle detection
        let err = parse_tree("{ 0: [0] }").unwrap_err();
        assert!(err.contains("cycle detected") && err.contains("0 -> 0"));

        let err = parse_tree("{ 0: [1], 1: [0] }").unwrap_err();
        assert!(err.contains("cycle detected"));

        let err = parse_tree("{ 0: [1], 1: [2], 2: [3], 3: [0] }").unwrap_err();
        assert!(err.contains("cycle detected") && err.contains("0 -> 1 -> 2 -> 3 -> 0"));

        let err = parse_tree("{ 0: [1], 1: [2], 2: [3], 3: [2] }").unwrap_err();
        assert!(err.contains("cycle detected") && err.contains("2 -> 3 -> 2"));
    }
}
