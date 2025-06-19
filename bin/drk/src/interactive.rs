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
    io::{stdin, ErrorKind, Read},
    str::FromStr,
};

use futures::{select, FutureExt};
use libc::{fcntl, F_GETFL, F_SETFL, O_NONBLOCK};
use linenoise_rs::{
    linenoise_history_add, linenoise_history_load, linenoise_history_save,
    linenoise_set_completion_callback, linenoise_set_hints_callback, LinenoiseState,
};
use prettytable::{format, row, Table};
use smol::channel::{unbounded, Receiver, Sender};
use url::Url;

use darkfi::{
    cli_desc,
    system::{msleep, ExecutorPtr, StoppableTask, StoppableTaskPtr},
    util::{encoding::base64, parse::encode_base10, path::expand_path},
    zk::halo2::Field,
    Error,
};
use darkfi_money_contract::model::Coin;
use darkfi_sdk::{
    crypto::{FuncId, PublicKey},
    pasta::{group::ff::PrimeField, pallas},
};
use darkfi_serial::{deserialize_async, serialize_async};

use crate::{
    cli_util::{
        generate_completions, kaching, parse_token_pair, parse_tx_from_stdin, parse_value_pair,
    },
    money::BALANCE_BASE10_DECIMALS,
    rpc::subscribe_blocks,
    swap::PartialSwapData,
    DrkPtr,
};

// TODO:
//  1. add rest commands handling, along with their completions, hints and help message.
//  2. add input definitions, so you input from files not just stdin.
//  3. add output definitions, so you can output to files not just stdout.
//  4. create a transactions cache in the wallet db, so you can use it to handle them.

/// Auxiliary function to print the help message.
fn help() {
    println!("{}", cli_desc!());
    println!("Commands:");
    println!("\thelp: Prints the help message");
    println!("\tkaching: Fun");
    println!("\tping: Send a ping request to the darkfid RPC endpoint");
    println!("\tcompletions: Generate a SHELL completion script and print to stdout");
    println!("\twallet: Wallet operations");
    println!("\tspend: Read a transaction from stdin and mark its input coins as spent");
    println!("\tunspend: Unspend a coin");
    println!("\ttransfer: Create a payment transaction");
    println!("\totc: OTC atomic swap");
    println!("\tattach-fee: Attach the fee call to a transaction given from stdin");
    println!("\tinspect: Inspect a transaction from stdin");
    println!("\tbroadcast: Read a transaction from stdin and broadcast it");
    println!(
        "\tsubscribe: Perform a scan and then subscribe to darkfid to listen for incoming blocks"
    );
    println!("\tunsubscribe: Stops the background subscription, if its active");
    println!("\tsnooze: Disables the background subscription messages printing");
    println!("\tunsnooze: Enables the background subscription messages printing");
    println!("\tscan: Scan the blockchain and parse relevant transactions");
}

/// Auxiliary function to define the interactive shell completions.
fn completion(buf: &str, lc: &mut Vec<String>) {
    // First we define the specific commands prefixes
    if buf.starts_with("h") {
        lc.push("help".to_string());
        return
    }

    if buf.starts_with("k") {
        lc.push("kaching".to_string());
        return
    }

    if buf.starts_with("p") {
        lc.push("ping".to_string());
        return
    }

    if buf.starts_with("c") {
        lc.push("completions".to_string());
        return
    }

    if buf.starts_with("w") {
        lc.push("wallet".to_string());
        lc.push("wallet initialize".to_string());
        lc.push("wallet keygen".to_string());
        lc.push("wallet balance".to_string());
        lc.push("wallet address".to_string());
        lc.push("wallet addresses".to_string());
        lc.push("wallet default-address".to_string());
        lc.push("wallet secrets".to_string());
        lc.push("wallet import-secrets".to_string());
        lc.push("wallet tree".to_string());
        lc.push("wallet coins".to_string());
        return
    }

    if buf.starts_with("sp") {
        lc.push("spend".to_string());
        return
    }

    if buf.starts_with("unsp") {
        lc.push("unspend".to_string());
        return
    }

    if buf.starts_with("t") {
        lc.push("transfer".to_string());
        return
    }

    if buf.starts_with("o") {
        lc.push("otc".to_string());
        lc.push("otc init".to_string());
        lc.push("otc join".to_string());
        lc.push("otc inspect".to_string());
        lc.push("otc sign".to_string());
        return
    }

    if buf.starts_with("a") {
        lc.push("attach-fee".to_string());
        return
    }

    if buf.starts_with("i") {
        lc.push("inspect".to_string());
        return
    }

    if buf.starts_with("b") {
        lc.push("broadcast".to_string());
        return
    }

    if buf.starts_with("su") {
        lc.push("subscribe".to_string());
        return
    }

    if buf.starts_with("unsu") {
        lc.push("unsubscribe".to_string());
        return
    }

    if buf.starts_with("sn") {
        lc.push("snooze".to_string());
        return
    }

    if buf.starts_with("unsn") {
        lc.push("unsnooze".to_string());
        return
    }

    if buf.starts_with("sc") {
        lc.push("scan".to_string());
        lc.push("scan --reset".to_string());
        return
    }

    // Now the catch alls
    if buf.starts_with("s") {
        lc.push("spend".to_string());
        lc.push("subscribe".to_string());
        lc.push("snooze".to_string());
        lc.push("scan".to_string());
        lc.push("scan --reset".to_string());
        return
    }

    if buf.starts_with("u") {
        lc.push("unspend".to_string());
        lc.push("unsubscribe".to_string());
        lc.push("unsnooze".to_string());
    }
}

/// Auxiliary function to define the interactive shell hints.
fn hints(buf: &str) -> Option<(String, i32, bool)> {
    match buf {
        "completions " => Some(("<shell>".to_string(), 35, false)), // 35 = magenta
        "wallet " => Some(("(initialize|keygen|balance|address|addresses|default-address|secrets|import-secrets|tree|coins)".to_string(), 35, false)),
        "wallet default-address " => Some(("<index>".to_string(), 35, false)),
        "unspend " => Some(("<coin>".to_string(), 35, false)),
        "transfer " => Some(("[--half-split] <amount> <token> <recipient> [spend_hook] [user_data]".to_string(), 35, false)),
        "otc " => Some(("(init|join|inspect|sign)".to_string(), 35, false)),
        "otc init " => Some(("<value_pair> <token_pair>".to_string(), 35, false)),
        "scan --reset " => Some(("<height>".to_string(), 35, false)),
        _ => None,
    }
}

/// Auxiliary function to start provided Drk as an interactive shell.
/// Only sane/linenoise terminals are suported.
pub async fn interactive(drk: &DrkPtr, endpoint: &Url, history_path: &str, ex: &ExecutorPtr) {
    // Expand the history file path
    let history_path = match expand_path(history_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error while expanding history file path: {e}");
            return
        }
    };
    let history_path = history_path.into_os_string();
    let history_file = history_path.to_str().unwrap();

    // Set the completion callback. This will be called every time the
    // user uses the <tab> key.
    linenoise_set_completion_callback(completion);

    // Set the shell hints
    linenoise_set_hints_callback(hints);

    // Load history from file.The history file is just a plain text file
    // where entries are separated by newlines.
    let _ = linenoise_history_load(history_file);

    // Create a detached task to use for block subscription
    let mut subscription_active = false;
    let mut snooze_active = false;
    let subscription_task = StoppableTask::new();

    // Create an unbounded smol channel, so we can have a printing
    // queue the background task can submit messages to the shell.
    let (shell_sender, shell_receiver) = unbounded();

    // Start the interactive shell
    loop {
        // Wait for next line to process
        let line = listen_for_line(&snooze_active, &shell_receiver).await;

        // Grab input or end if Ctrl-D or Ctrl-C was pressed
        let Some(line) = line else { break };

        // Check if line is empty
        if line.is_empty() {
            continue
        }

        // Add line to history
        linenoise_history_add(&line);

        // Parse command parts
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue
        }

        // Handle command
        match parts[0] {
            "help" => help(),
            "kaching" => kaching().await,
            "ping" => handle_ping(drk).await,
            "completions" => handle_completions(&parts),
            "wallet" => handle_wallet(drk, &parts).await,
            "spend" => handle_spend(drk).await,
            "unspend" => handle_unspend(drk, &parts).await,
            "transfer" => handle_transfer(drk, &parts).await,
            "otc" => handle_otc(drk, &parts).await,
            "attach-fee" => handle_attach_fee(drk).await,
            "inspect" => handle_inspect().await,
            "broadcast" => handle_broadcast(drk).await,
            "subscribe" => {
                handle_subscribe(
                    drk,
                    endpoint,
                    &mut subscription_active,
                    &subscription_task,
                    &shell_sender,
                    ex,
                )
                .await
            }
            "unsubscribe" => handle_unsubscribe(&mut subscription_active, &subscription_task).await,
            "snooze" => snooze_active = true,
            "unsnooze" => snooze_active = false,
            "scan" => handle_scan(drk, &subscription_active, &parts).await,
            _ => println!("Unreconized command: {}", parts[0]),
        }
    }

    // Stop the subscription task if its active
    if subscription_active {
        subscription_task.stop().await;
    }

    // Write history file
    let _ = linenoise_history_save(history_file);
}

/// Auxiliary function to listen for linenoise input line and handle
/// background task messages.
async fn listen_for_line(
    snooze_active: &bool,
    shell_receiver: &Receiver<Vec<String>>,
) -> Option<String> {
    // Generate the linoise state structure
    let mut state = match LinenoiseState::edit_start(-1, -1, "drk> ") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error while generating linenoise state: {e}");
            return None
        }
    };

    // Set stdin to non-blocking mode
    let fd = state.get_fd();
    unsafe {
        let flags = fcntl(fd, F_GETFL, 0);
        fcntl(fd, F_SETFL, flags | O_NONBLOCK);
    }

    // Read until we get a line to process
    let mut line = None;
    loop {
        // Future that polls stdin for input
        let input_future = async {
            loop {
                match state.edit_feed() {
                    Ok(Some(l)) => {
                        line = Some(l);
                        break
                    }
                    Ok(None) => break,
                    Err(e) if e.kind() == ErrorKind::Interrupted => break,
                    Err(e) if e.kind() == ErrorKind::WouldBlock => {
                        // No data available, yield and retry
                        msleep(10).await;
                        continue
                    }
                    Err(e) => {
                        eprintln!("Error while reading linenoise feed: {e}");
                        break
                    }
                }
            }
        };

        // Future that polls the channel
        let channel_future = async {
            loop {
                if !shell_receiver.is_empty() {
                    break
                }
                msleep(1000).await;
            }
        };

        // Manage the futures
        select! {
            // When input is ready we break out the loop
            _ = input_future.fuse() => break,
            // Manage filled channel
            _ = channel_future.fuse() => {
                while !shell_receiver.is_empty() {
                    match shell_receiver.recv().await {
                        Ok(msg) => {
                            // We only print if snooze is inactive,
                            // but have to consume the message regardless,
                            // so the queue gets empty.
                            if *snooze_active {
                                continue
                            }
                            // Hide prompt, print output, show prompt again
                            let _ = state.hide();
                            for line in msg {
                                println!("{}\r", line.replace("\n", "\n\r"));
                            }
                            let _ = state.show();
                        }
                        Err(e) => {
                            eprintln!("Error while reading shell receiver channel: {e}");
                            break
                        }
                    }
                }
            }
        }
    }

    // Restore blocking mode
    unsafe {
        let flags = fcntl(fd, F_GETFL, 0);
        fcntl(fd, F_SETFL, flags & !O_NONBLOCK);
    }

    let _ = state.edit_stop();
    line
}

/// Auxiliary function to define the ping command handling.
async fn handle_ping(drk: &DrkPtr) {
    if let Err(e) = drk.read().await.ping().await {
        println!("Error while executing ping command: {e}")
    }
}

/// Auxiliary function to define the completions command handling.
fn handle_completions(parts: &[&str]) {
    // Check correct command structure
    if parts.len() != 2 {
        println!("Malformed `completions` command");
        println!("Usage: completions <shell>");
        return
    }

    if let Err(e) = generate_completions(parts[1]) {
        println!("Error while executing completions command: {e}")
    }
}

/// Auxiliary function to define the wallet command handling.
async fn handle_wallet(drk: &DrkPtr, parts: &[&str]) {
    // Check correct command structure
    if parts.len() < 2 {
        println!("Malformed `wallet` command");
        println!("Usage: wallet (initialize|keygen|balance|address|addresses|default-address|secrets|import-secrets|tree|coins)");
        return
    }

    // Handle subcommand
    match parts[1] {
        "initialize" => handle_wallet_initialize(drk).await,
        "keygen" => handle_wallet_keygen(drk).await,
        "balance" => handle_wallet_balance(drk).await,
        "address" => handle_wallet_address(drk).await,
        "addresses" => handle_wallet_addresses(drk).await,
        "default-address" => handle_wallet_default_address(drk, parts).await,
        "secrets" => handle_wallet_secrets(drk).await,
        "import-secrets" => handle_wallet_import_secrets(drk).await,
        "tree" => handle_wallet_tree(drk).await,
        "coins" => handle_wallet_coins(drk).await,
        _ => {
            println!("Unreconized wallet subcommand: {}", parts[1]);
            println!("Usage: wallet (initialize|keygen|balance|address|addresses|default-address|secrets|import-secrets|tree|coins)");
        }
    }
}

/// Auxiliary function to define the wallet initialize subcommand handling.
async fn handle_wallet_initialize(drk: &DrkPtr) {
    let lock = drk.read().await;
    if let Err(e) = lock.initialize_wallet().await {
        println!("Error initializing wallet: {e:?}");
        return
    }
    if let Err(e) = lock.initialize_money().await {
        println!("Failed to initialize Money: {e:?}");
        return
    }
    if let Err(e) = lock.initialize_dao().await {
        println!("Failed to initialize DAO: {e:?}");
        return
    }
    if let Err(e) = lock.initialize_deployooor() {
        println!("Failed to initialize Deployooor: {e:?}");
    }
}

/// Auxiliary function to define the wallet keygen subcommand handling.
async fn handle_wallet_keygen(drk: &DrkPtr) {
    if let Err(e) = drk.read().await.money_keygen().await {
        println!("Failed to generate keypair: {e:?}");
    }
}

/// Auxiliary function to define the wallet balance subcommand handling.
async fn handle_wallet_balance(drk: &DrkPtr) {
    let lock = drk.read().await;
    let balmap = match lock.money_balance().await {
        Ok(m) => m,
        Err(e) => {
            println!("Failed to fetch balances map: {e:?}");
            return
        }
    };

    let aliases_map = match lock.get_aliases_mapped_by_token().await {
        Ok(m) => m,
        Err(e) => {
            println!("Failed to fetch aliases map: {e:?}");
            return
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

        table.add_row(row![token_id, aliases, encode_base10(*balance, BALANCE_BASE10_DECIMALS)]);
    }

    if table.is_empty() {
        println!("No unspent balances found");
    } else {
        println!("{table}");
    }
}

/// Auxiliary function to define the wallet address subcommand handling.
async fn handle_wallet_address(drk: &DrkPtr) {
    match drk.read().await.default_address().await {
        Ok(address) => println!("{address}"),
        Err(e) => println!("Failed to fetch default address: {e:?}"),
    }
}

/// Auxiliary function to define the wallet addresses subcommand handling.
async fn handle_wallet_addresses(drk: &DrkPtr) {
    let addresses = match drk.read().await.addresses().await {
        Ok(a) => a,
        Err(e) => {
            println!("Failed to fetch addresses: {e:?}");
            return
        }
    };

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
}

/// Auxiliary function to define the wallet default address subcommand handling.
async fn handle_wallet_default_address(drk: &DrkPtr, parts: &[&str]) {
    if parts.len() != 3 {
        println!("Malformed `wallet default-address` subcommand");
        println!("Usage: wallet default-address <index>");
        return
    }

    let index = match usize::from_str(parts[2]) {
        Ok(i) => i,
        Err(e) => {
            println!("Invalid address id: {e:?}");
            return
        }
    };

    if let Err(e) = drk.read().await.set_default_address(index) {
        println!("Failed to set default address: {e:?}");
    }
}

/// Auxiliary function to define the wallet secrets subcommand handling.
async fn handle_wallet_secrets(drk: &DrkPtr) {
    match drk.read().await.get_money_secrets().await {
        Ok(secrets) => {
            for secret in secrets {
                println!("{secret}");
            }
        }
        Err(e) => println!("Failed to fetch secrets: {e:?}"),
    }
}

/// Auxiliary function to define the wallet import secrets subcommand handling.
async fn handle_wallet_import_secrets(drk: &DrkPtr) {
    let mut secrets = vec![];
    // TODO: read from a file here not stdin
    let lines = stdin().lines();
    for (i, line) in lines.enumerate() {
        if let Ok(line) = line {
            let Ok(bytes) = bs58::decode(&line.trim()).into_vec() else {
                println!("Warning: Failed to decode secret on line {i}");
                continue
            };
            let Ok(secret) = deserialize_async(&bytes).await else {
                println!("Warning: Failed to deserialize secret on line {i}");
                continue
            };
            secrets.push(secret);
        }
    }

    match drk.read().await.import_money_secrets(secrets).await {
        Ok(pubkeys) => {
            for key in pubkeys {
                println!("{key}");
            }
        }
        Err(e) => println!("Failed to import secrets: {e:?}"),
    }
}

/// Auxiliary function to define the wallet tree subcommand handling.
async fn handle_wallet_tree(drk: &DrkPtr) {
    // TODO: write to a file here not stdout
    match drk.read().await.get_money_tree().await {
        Ok(tree) => println!("{tree:#?}"),
        Err(e) => println!("Failed to fetch tree: {e:?}"),
    }
}

/// Auxiliary function to define the wallet coins subcommand handling.
async fn handle_wallet_coins(drk: &DrkPtr) {
    let lock = drk.read().await;
    let coins = match lock.get_coins(true).await {
        Ok(c) => c,
        Err(e) => {
            println!("Failed to fetch coins: {e:?}");
            return
        }
    };

    if coins.is_empty() {
        return
    }

    let aliases_map = match lock.get_aliases_mapped_by_token().await {
        Ok(m) => m,
        Err(e) => {
            println!("Failed to fetch aliases map: {e:?}");
            return
        }
    };

    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row![
        "Coin",
        "Token ID",
        "Aliases",
        "Value",
        "Spend Hook",
        "User Data",
        "Creation Height",
        "Spent",
        "Spent Height",
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
            bs58::encode(&serialize_async(&coin.0.note.user_data).await).into_string().to_string()
        } else {
            String::from("-")
        };

        let spent_height = match coin.3 {
            Some(spent_height) => spent_height.to_string(),
            None => String::from("-"),
        };

        table.add_row(row![
            bs58::encode(&serialize_async(&coin.0.coin.inner()).await).into_string().to_string(),
            coin.0.note.token_id,
            aliases,
            format!(
                "{} ({})",
                coin.0.note.value,
                encode_base10(coin.0.note.value, BALANCE_BASE10_DECIMALS)
            ),
            spend_hook,
            user_data,
            coin.1,
            coin.2,
            spent_height,
            coin.4,
        ]);
    }

    println!("{table}");
}

/// Auxiliary function to define the spend command handling.
async fn handle_spend(drk: &DrkPtr) {
    let tx = match parse_tx_from_stdin().await {
        Ok(t) => t,
        Err(e) => {
            println!("Error while parsing transaction: {e}");
            return
        }
    };

    if let Err(e) = drk.read().await.mark_tx_spend(&tx).await {
        println!("Failed to mark transaction coins as spent: {e}")
    }
}

/// Auxiliary function to define the unspend command handling.
async fn handle_unspend(drk: &DrkPtr, parts: &[&str]) {
    // Check correct command structure
    if parts.len() != 2 {
        println!("Malformed `unspend` command");
        println!("Usage: unspend <coin>");
        return
    }

    let bytes = match bs58::decode(&parts[1]).into_vec() {
        Ok(b) => b,
        Err(e) => {
            println!("Invalid coin: {e}");
            return
        }
    };

    let bytes: [u8; 32] = match bytes.try_into() {
        Ok(b) => b,
        Err(e) => {
            println!("Invalid coin: {e:?}");
            return
        }
    };

    let elem: pallas::Base = match pallas::Base::from_repr(bytes).into() {
        Some(v) => v,
        None => {
            println!("Invalid coin");
            return
        }
    };

    if let Err(e) = drk.read().await.unspend_coin(&Coin::from(elem)).await {
        println!("Failed to mark coin as unspent: {e}")
    }
}

/// Auxiliary function to define the transfer command handling.
async fn handle_transfer(drk: &DrkPtr, parts: &[&str]) {
    // Check correct command structure
    if parts.len() < 4 || parts.len() > 7 {
        println!("Malformed `transfer` command");
        println!(
            "Usage: transfer [--half-split] <amount> <token> <recipient> [spend_hook] [user_data]"
        );
        return
    }

    // Parse command
    let mut index = 1;
    let mut half_split = false;
    if parts[index] == "--half-split" {
        half_split = true;
        index += 1;
    }

    let amount = String::from(parts[index]);
    if let Err(e) = f64::from_str(&amount) {
        println!("Invalid amount: {e}");
        return
    }
    index += 1;

    let lock = drk.read().await;
    let token_id = match lock.get_token(String::from(parts[index])).await {
        Ok(t) => t,
        Err(e) => {
            println!("Invalid token alias: {e}");
            return
        }
    };
    index += 1;

    let rcpt = match PublicKey::from_str(parts[index]) {
        Ok(r) => r,
        Err(e) => {
            println!("Invalid recipient: {e}");
            return
        }
    };
    index += 1;

    let spend_hook = if index < parts.len() {
        match FuncId::from_str(parts[index]) {
            Ok(s) => Some(s),
            Err(e) => {
                println!("Invalid spend hook: {e}");
                return
            }
        }
    } else {
        None
    };
    index += 1;

    let user_data = if index < parts.len() {
        let bytes = match bs58::decode(&parts[index]).into_vec() {
            Ok(b) => b,
            Err(e) => {
                println!("Invalid user data: {e}");
                return
            }
        };

        let bytes: [u8; 32] = match bytes.try_into() {
            Ok(b) => b,
            Err(e) => {
                println!("Invalid user data: {e:?}");
                return
            }
        };

        let elem: pallas::Base = match pallas::Base::from_repr(bytes).into() {
            Some(v) => v,
            None => {
                println!("Invalid user data");
                return
            }
        };

        Some(elem)
    } else {
        None
    };

    // TODO: write to a file here not stdout
    match lock.transfer(&amount, token_id, rcpt, spend_hook, user_data, half_split).await {
        Ok(t) => println!("{}", base64::encode(&serialize_async(&t).await)),
        Err(e) => println!("Failed to create payment transaction: {e}"),
    }
}

/// Auxiliary function to define the otc command handling.
async fn handle_otc(drk: &DrkPtr, parts: &[&str]) {
    // Check correct command structure
    if parts.len() < 2 {
        println!("Malformed `otc` command");
        println!("Usage: otc (init|join|inspect|sign)");
        return
    }

    // Handle subcommand
    match parts[1] {
        "init" => handle_otc_init(drk, parts).await,
        "join" => handle_otc_join(drk, parts).await,
        "inspect" => handle_otc_inspect(drk, parts).await,
        "sign" => handle_otc_sign(drk, parts).await,
        _ => {
            println!("Unreconized OTC subcommand: {}", parts[1]);
            println!("Usage: otc (init|join|inspect|sign)");
        }
    }
}

/// Auxiliary function to define the otc init subcommand handling.
async fn handle_otc_init(drk: &DrkPtr, parts: &[&str]) {
    // Check correct subcommand structure
    if parts.len() != 4 {
        println!("Malformed `otc init` subcommand");
        println!("Usage: otc init <value_pair> <token_pair>");
        return
    }

    let value_pair = match parse_value_pair(parts[2]) {
        Ok(v) => v,
        Err(e) => {
            println!("Invalid value pair: {e}");
            return
        }
    };

    let lock = drk.read().await;
    let token_pair = match parse_token_pair(&lock, parts[3]).await {
        Ok(t) => t,
        Err(e) => {
            println!("Invalid token pair: {e}");
            return
        }
    };

    match lock.init_swap(value_pair, token_pair, None, None, None).await {
        Ok(half) => println!("{}", base64::encode(&serialize_async(&half).await)),
        Err(e) => eprintln!("Failed to create swap transaction half: {e}"),
    }
}

/// Auxiliary function to define the otc join subcommand handling.
async fn handle_otc_join(drk: &DrkPtr, parts: &[&str]) {
    // Check correct subcommand structure
    if parts.len() != 2 {
        println!("Malformed `otc join` subcommand");
        println!("Usage: otc join");
        return
    }

    // TODO: read from a file here not stdin
    let mut buf = String::new();
    if let Err(e) = stdin().read_to_string(&mut buf) {
        println!("Failed to read from stdin: {e}");
        return
    };

    let Some(bytes) = base64::decode(buf.trim()) else {
        println!("Failed to decode partial swap data");
        return
    };

    let partial: PartialSwapData = match deserialize_async(&bytes).await {
        Ok(p) => p,
        Err(e) => {
            println!("Failed to deserialize partial swap data: {e}");
            return
        }
    };

    match drk.read().await.join_swap(partial, None, None, None).await {
        Ok(tx) => println!("{}", base64::encode(&serialize_async(&tx).await)),
        Err(e) => eprintln!("Failed to create a join swap transaction: {e}"),
    }
}

/// Auxiliary function to define the otc inspect subcommand handling.
async fn handle_otc_inspect(drk: &DrkPtr, parts: &[&str]) {
    // Check correct subcommand structure
    if parts.len() != 2 {
        println!("Malformed `otc inspect` subcommand");
        println!("Usage: otc inspect");
        return
    }

    // TODO: read from a file here not stdin
    let mut buf = String::new();
    if let Err(e) = stdin().read_to_string(&mut buf) {
        println!("Failed to read from stdin: {e}");
        return
    };

    let Some(bytes) = base64::decode(buf.trim()) else {
        println!("Failed to decode swap transaction");
        return
    };

    if let Err(e) = drk.read().await.inspect_swap(bytes).await {
        println!("Failed to inspect swap: {e}");
    }
}

/// Auxiliary function to define the otc sign subcommand handling.
async fn handle_otc_sign(drk: &DrkPtr, parts: &[&str]) {
    // Check correct subcommand structure
    if parts.len() != 2 {
        println!("Malformed `otc sign` subcommand");
        println!("Usage: otc sign");
        return
    }

    // TODO: read from a file here not stdin
    let mut tx = match parse_tx_from_stdin().await {
        Ok(t) => t,
        Err(e) => {
            println!("Error while parsing transaction: {e}");
            return
        }
    };

    match drk.read().await.sign_swap(&mut tx).await {
        Ok(_) => println!("{}", base64::encode(&serialize_async(&tx).await)),
        Err(e) => println!("Failed to sign joined swap transaction: {e}"),
    }
}

/// Auxiliary function to define the attach fee command handling.
async fn handle_attach_fee(drk: &DrkPtr) {
    // TODO: read from a file here not stdin
    let mut tx = match parse_tx_from_stdin().await {
        Ok(t) => t,
        Err(e) => {
            println!("Error while parsing transaction: {e}");
            return
        }
    };

    match drk.read().await.attach_fee(&mut tx).await {
        Ok(_) => println!("{}", base64::encode(&serialize_async(&tx).await)),
        Err(e) => println!("Failed to attach the fee call to the transaction: {e}"),
    }
}

/// Auxiliary function to define the inspect command handling.
async fn handle_inspect() {
    // TODO: read from a file here not stdin
    match parse_tx_from_stdin().await {
        Ok(tx) => println!("{tx:#?}"),
        Err(e) => println!("Error while parsing transaction: {e}"),
    }
}

/// Auxiliary function to define the broadcast command handling.
async fn handle_broadcast(drk: &DrkPtr) {
    // TODO: read from a file here not stdin
    let tx = match parse_tx_from_stdin().await {
        Ok(t) => t,
        Err(e) => {
            println!("Error while parsing transaction: {e}");
            return
        }
    };

    let lock = drk.read().await;
    if let Err(e) = lock.simulate_tx(&tx).await {
        println!("Failed to simulate tx: {e}");
        return
    };

    if let Err(e) = lock.mark_tx_spend(&tx).await {
        println!("Failed to mark transaction coins as spent: {e}");
        return
    };

    match lock.broadcast_tx(&tx).await {
        Ok(txid) => println!("Transaction ID: {txid}"),
        Err(e) => println!("Failed to broadcast transaction: {e}"),
    }
}

/// Auxiliary function to define the subscribe command handling.
async fn handle_subscribe(
    drk: &DrkPtr,
    endpoint: &Url,
    subscription_active: &mut bool,
    subscription_task: &StoppableTaskPtr,
    shell_sender: &Sender<Vec<String>>,
    ex: &ExecutorPtr,
) {
    if *subscription_active {
        println!("Subscription is already active!");
        return
    }

    if let Err(e) = drk.read().await.scan_blocks().await {
        println!("Failed during scanning: {e:?}");
        return
    }
    println!("Finished scanning blockchain");

    // Start the subcristion task
    let drk_ = drk.clone();
    let endpoint_ = endpoint.clone();
    let shell_sender_ = shell_sender.clone();
    let ex_ = ex.clone();
    subscription_task.clone().start(
        async move { subscribe_blocks(&drk_, shell_sender_, endpoint_, &ex_).await },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => println!("Failed starting subscription task: {e}"),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    *subscription_active = true;
}

/// Auxiliary function to define the unsubscribe command handling.
async fn handle_unsubscribe(subscription_active: &mut bool, subscription_task: &StoppableTaskPtr) {
    if !*subscription_active {
        println!("Subscription is already inactive!");
        return
    }
    subscription_task.stop().await;
    *subscription_active = false;
}

/// Auxiliary function to define the scan command handling.
async fn handle_scan(drk: &DrkPtr, subscription_active: &bool, parts: &[&str]) {
    if *subscription_active {
        println!("Subscription is already active!");
        return
    }

    // Check correct command structure
    if parts.len() != 1 && parts.len() != 3 {
        println!("Malformed `scan` command");
        return
    }

    // Check if reset was requested
    let lock = drk.read().await;
    if parts.len() == 3 {
        if parts[1] != "--reset" {
            println!("Malformed `scan` command");
            println!("Usage: scan --reset <height>");
            return
        }

        let height = match u32::from_str(parts[2]) {
            Ok(h) => h,
            Err(e) => {
                println!("Invalid reset height: {e:?}");
                return
            }
        };

        if let Err(e) = lock.reset_to_height(height) {
            println!("Failed during wallet reset: {e:?}");
            return
        }
    }

    if let Err(e) = lock.scan_blocks().await {
        println!("Failed during scanning: {e:?}");
        return
    }
    println!("Finished scanning blockchain");
}
