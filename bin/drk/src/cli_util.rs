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
use std::{io::Cursor, process::exit, str::FromStr};

use rodio::{source::Source, Decoder, OutputStream};
use structopt_toml::clap::{App, Arg, Shell, SubCommand};

use darkfi::{cli_desc, system::sleep, util::parse::decode_base10, Error, Result};
use darkfi_money_contract::model::TokenId;

use crate::{money::BALANCE_BASE10_DECIMALS, Drk};

/// Auxiliary function to parse provided string into a values pair.
pub fn parse_value_pair(s: &str) -> Result<(u64, u64)> {
    let v: Vec<&str> = s.split(':').collect();
    if v.len() != 2 {
        eprintln!("Invalid value pair. Use a pair such as 13.37:11.0");
        exit(1);
    }

    let val0 = decode_base10(v[0], BALANCE_BASE10_DECIMALS, true);
    let val1 = decode_base10(v[1], BALANCE_BASE10_DECIMALS, true);

    if val0.is_err() || val1.is_err() {
        eprintln!("Invalid value pair. Use a pair such as 13.37:11.0");
        exit(1);
    }

    Ok((val0.unwrap(), val1.unwrap()))
}

/// Auxiliary function to parse provided string into a tokens pair.
pub async fn parse_token_pair(drk: &Drk, s: &str) -> Result<(TokenId, TokenId)> {
    let v: Vec<&str> = s.split(':').collect();
    if v.len() != 2 {
        eprintln!("Invalid token pair. Use a pair such as:");
        eprintln!("WCKD:MLDY");
        eprintln!("or");
        eprintln!("A7f1RKsCUUHrSXA7a9ogmwg8p3bs6F47ggsW826HD4yd:FCuoMii64H5Ee4eVWBjP18WTFS8iLUJmGi16Qti1xFQ2");
        exit(1);
    }

    let tok0 = drk.get_token(v[0].to_string()).await;
    let tok1 = drk.get_token(v[1].to_string()).await;

    if tok0.is_err() || tok1.is_err() {
        eprintln!("Invalid token pair. Use a pair such as:");
        eprintln!("WCKD:MLDY");
        eprintln!("or");
        eprintln!("A7f1RKsCUUHrSXA7a9ogmwg8p3bs6F47ggsW826HD4yd:FCuoMii64H5Ee4eVWBjP18WTFS8iLUJmGi16Qti1xFQ2");
        exit(1);
    }

    Ok((tok0.unwrap(), tok1.unwrap()))
}

/// Fun police go away
pub async fn kaching() {
    const WALLET_MP3: &[u8] = include_bytes!("../wallet.mp3");

    let cursor = Cursor::new(WALLET_MP3);

    let Ok((_stream, stream_handle)) = OutputStream::try_default() else { return };

    let Ok(source) = Decoder::new(cursor) else { return };

    if stream_handle.play_raw(source.convert_samples()).is_err() {
        return
    }

    sleep(2).await;
}

/// Auxiliary function to generate provided shell completions.
pub fn generate_completions(shell: &str) -> Result<()> {
    // Sub-commands

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
    let initialize =
        Arg::with_name("initialize").long("initialize").help("Initialize wallet database");

    let keygen =
        Arg::with_name("keygen").long("keygen").help("Generate a new keypair in the wallet");

    let balance =
        Arg::with_name("balance").long("balance").help("Query the wallet for known balances");

    let address =
        Arg::with_name("address").long("address").help("Get the default address in the wallet");

    let addresses =
        Arg::with_name("addresses").long("addresses").help("Print all the addresses in the wallet");

    let default_address = Arg::with_name("default-address")
        .long("default-address")
        .takes_value(true)
        .help("Set the default address in the wallet");

    let secrets =
        Arg::with_name("secrets").long("secrets").help("Print all the secret keys from the wallet");

    let import_secrets = Arg::with_name("import-secrets")
        .long("import-secrets")
        .help("Import secret keys from stdin into the wallet, separated by newlines");

    let tree = Arg::with_name("tree").long("tree").help("Print the Merkle tree in the wallet");

    let coins = Arg::with_name("coins").long("coins").help("Print all the coins in the wallet");

    let wallet = SubCommand::with_name("wallet").about("Wallet operations").args(&vec![
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
    ]);

    // Unspend
    let coin = Arg::with_name("coin").help("base58-encoded coin to mark as unspent");

    let unspend = SubCommand::with_name("unspend").about("Unspend a coin").arg(coin);

    // Transfer
    let amount = Arg::with_name("amount").help("Amount to send");

    let token = Arg::with_name("token").help("Token ID to send");

    let recipient = Arg::with_name("recipient").help("Recipient address");

    let transfer = SubCommand::with_name("transfer")
        .about("Create a payment transaction")
        .args(&vec![amount, token, recipient]);

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
        .args(&vec![value_pair, token_pair]);

    let join =
        SubCommand::with_name("join").about("Build entire swap tx given the first half from stdin");

    let inspect = SubCommand::with_name("inspect")
        .about("Inspect a swap half or the full swap tx from stdin");

    let sign = SubCommand::with_name("sign")
        .about("Sign a transaction given from stdin as the first-half");

    let otc = SubCommand::with_name("otc")
        .about("OTC atomic swap")
        .subcommands(vec![init, join, inspect, sign]);

    // Inspect
    let inspect = SubCommand::with_name("inspect").about("Inspect a transaction from stdin");

    // Broadcast
    let broadcast =
        SubCommand::with_name("broadcast").about("Read a transaction from stdin and broadcast it");

    // Subscribe
    let subscribe = SubCommand::with_name("subscribe").about(
        "This subscription will listen for incoming blocks from darkfid and look \
                    through their transactions to see if there's any that interest us. \
                    With `drk` we look at transactions calling the money contract so we can \
                    find coins sent to us and fill our wallet with the necessary metadata.",
    );

    // DAO
    let proposer_limit = Arg::with_name("proposer-limit")
        .help("The minimum amount of governance tokens needed to open a proposal for this DAO");

    let quorum = Arg::with_name("quorum")
        .help("Minimal threshold of participating total tokens needed for a proposal to pass");

    let approval_ratio = Arg::with_name("approval-ratio")
        .help("The ratio of winning votes/total votes needed for a proposal to pass (2 decimals)");

    let gov_token_id = Arg::with_name("gov-token-id").help("DAO's governance token ID");

    let create = SubCommand::with_name("create").about("Create DAO parameters").args(&vec![
        proposer_limit,
        quorum,
        approval_ratio,
        gov_token_id,
    ]);

    let view = SubCommand::with_name("view").about("View DAO data from stdin");

    let dao_name = Arg::with_name("dao-name").help("Named identifier for the DAO");

    let import =
        SubCommand::with_name("import").about("Import DAO data from stdin").args(&vec![dao_name]);

    let dao_alias = Arg::with_name("dao-alias").help("Numeric identifier for the DAO (optional)");

    let list = SubCommand::with_name("list")
        .about("List imported DAOs (or info about a specific one)")
        .args(&vec![dao_alias]);

    let dao_alias = Arg::with_name("dao-alias").help("Name or numeric identifier for the DAO");

    let balance = SubCommand::with_name("balance")
        .about("Show the balance of a DAO")
        .args(&vec![dao_alias.clone()]);

    let mint = SubCommand::with_name("mint")
        .about("Mint an imported DAO on-chain")
        .args(&vec![dao_alias.clone()]);

    let recipient =
        Arg::with_name("recipient").help("Pubkey to send tokens to with proposal success");

    let amount = Arg::with_name("amount").help("Amount to send from DAO with proposal success");

    let token = Arg::with_name("token").help("Token ID to send from DAO with proposal success");

    let propose = SubCommand::with_name("propose")
        .about("Create a proposal for a DAO")
        .args(&vec![dao_alias.clone(), recipient, amount, token]);

    let proposals = SubCommand::with_name("proposals")
        .about("List DAO proposals")
        .args(&vec![dao_alias.clone()]);

    let proposal_id = Arg::with_name("proposal-id").help("Numeric identifier for the proposal");

    let proposal = SubCommand::with_name("proposal")
        .about("View a DAO proposal data")
        .args(&vec![dao_alias.clone(), proposal_id.clone()]);

    let vote = Arg::with_name("vote").help("Vote (0 for NO, 1 for YES)");

    let vote_weight =
        Arg::with_name("vote-weight").help("Vote weight (amount of governance tokens)");

    let vote = SubCommand::with_name("vote").about("Vote on a given proposal").args(&vec![
        dao_alias.clone(),
        proposal_id.clone(),
        vote,
        vote_weight,
    ]);

    let exec = SubCommand::with_name("exec")
        .about("Execute a DAO proposal")
        .args(&vec![dao_alias, proposal_id]);

    let dao = SubCommand::with_name("dao").about("DAO functionalities").subcommands(vec![
        create, view, import, list, balance, mint, propose, proposals, proposal, vote, exec,
    ]);

    // Scan
    let reset = Arg::with_name("reset")
        .long("reset")
        .help("Reset Merkle tree and start scanning from first block");

    let list = Arg::with_name("list").long("list").help("List all available checkpoints");

    let checkpoint = Arg::with_name("checkpoint")
        .long("checkpoint")
        .takes_value(true)
        .help("Reset Merkle tree to checkpoint index and start scanning");

    let scan = SubCommand::with_name("scan")
        .about("Scan the blockchain and parse relevant transactions")
        .args(&vec![reset, list, checkpoint]);

    // Explorer
    let tx_hash = Arg::with_name("tx-hash").help("Transaction hash");

    let full = Arg::with_name("full").long("full").help("Print the full transaction information");

    let encode = Arg::with_name("encode").long("encode").help("Encode transaction to base58");

    let fetch_tx = SubCommand::with_name("fetch-tx")
        .about("Fetch a blockchain transaction by hash")
        .args(&vec![tx_hash, full, encode]);

    let simulate_tx =
        SubCommand::with_name("simulate-tx").about("Read a transaction from stdin and simulate it");

    let tx_hash = Arg::with_name("tx-hash").help("Transaction hash");

    let encode = Arg::with_name("encode")
        .long("encode")
        .help("Encode specific history record transaction to base58");

    let txs_history = SubCommand::with_name("txs-history")
        .about("Fetch broadcasted transactions history")
        .args(&vec![tx_hash, encode]);

    let explorer = SubCommand::with_name("explorer")
        .about("Explorer related subcommands")
        .subcommands(vec![fetch_tx, simulate_tx, txs_history]);

    // Alias
    let alias = Arg::with_name("alias").help("Token alias");

    let token = Arg::with_name("token").help("Token to create alias for");

    let add = SubCommand::with_name("add").about("Create a Token alias").args(&vec![alias, token]);

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
        .args(&vec![alias, token]);

    let alias = Arg::with_name("alias").help("Token alias to remove");

    let remove = SubCommand::with_name("remove").about("Remove a Token alias").arg(alias);

    let alias = SubCommand::with_name("alias")
        .about("Manage Token aliases")
        .subcommands(vec![add, show, remove]);

    // Token
    let import = SubCommand::with_name("import").about("Import a mint authority secret from stdin");

    let generate_mint =
        SubCommand::with_name("generate-mint").about("Generate a new mint authority");

    let list =
        SubCommand::with_name("list").about("List token IDs with available mint authorities");

    let token = Arg::with_name("token").help("Token ID to mint");

    let amount = Arg::with_name("amount").help("Amount to mint");

    let recipient = Arg::with_name("recipient").help("Recipient of the minted tokens");

    let mint =
        SubCommand::with_name("mint").about("Mint tokens").args(&vec![token, amount, recipient]);

    let token = Arg::with_name("token").help("Token ID to freeze");

    let freeze = SubCommand::with_name("freeze").about("Freeze a token mint").arg(token);

    let token = SubCommand::with_name("token").about("Token functionalities").subcommands(vec![
        import,
        generate_mint,
        list,
        mint,
        freeze,
    ]);

    // Main arguments
    let config = Arg::with_name("config")
        .short("c")
        .long("config")
        .takes_value(true)
        .help("Configuration file to use");

    let wallet_path = Arg::with_name("wallet_path")
        .long("wallet-path")
        .takes_value(true)
        .help("Path to wallet database");

    let wallet_pass = Arg::with_name("wallet_pass")
        .long("wallet-pass")
        .takes_value(true)
        .help("Password for the wallet database");

    let endpoint = Arg::with_name("endpoint")
        .short("e")
        .long("endpoint")
        .takes_value(true)
        .help("darkfid JSON-RPC endpoint");

    let command = vec![
        kaching,
        ping,
        completions,
        wallet,
        unspend,
        transfer,
        otc,
        inspect,
        broadcast,
        subscribe,
        dao,
        scan,
        explorer,
        alias,
        token,
    ];

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
        .args(&vec![config, wallet_path, wallet_pass, endpoint, log, verbose])
        .subcommands(command);

    let shell = match Shell::from_str(shell) {
        Ok(s) => s,
        Err(e) => return Err(Error::Custom(e)),
    };

    app.gen_completions_to("./drk", shell, &mut std::io::stdout());

    Ok(())
}
