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
    fs::{File, OpenOptions},
    io::{stdin, BufRead, BufReader, ErrorKind, Read, Seek, SeekFrom, Write},
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
        generate_completions, kaching, parse_token_pair, parse_tx_from_input, parse_value_pair,
        print_output,
    },
    money::BALANCE_BASE10_DECIMALS,
    rpc::subscribe_blocks,
    swap::PartialSwapData,
    DrkPtr,
};

// TODO:
//  1. Add rest commands handling, along with their completions, hints and help message.
//  2. Subscribe/scan ux is a bit flaky, fix it.
//  3. Create a transactions cache in the wallet db, so you can use it to handle them.

/// Auxiliary function to print the help message.
fn help(output: &mut Vec<String>) {
    output.push(String::from(cli_desc!()));
    output.push(String::from("Commands:"));
    output.push(String::from("\thelp: Prints the help message"));
    output.push(String::from("\tkaching: Fun"));
    output.push(String::from("\tping: Send a ping request to the darkfid RPC endpoint"));
    output.push(String::from(
        "\tcompletions: Generate a SHELL completion script and print to stdout",
    ));
    output.push(String::from("\twallet: Wallet operations"));
    output.push(String::from(
        "\tspend: Read a transaction from stdin and mark its input coins as spent",
    ));
    output.push(String::from("\tunspend: Unspend a coin"));
    output.push(String::from("\ttransfer: Create a payment transaction"));
    output.push(String::from("\totc: OTC atomic swap"));
    output
        .push(String::from("\tattach-fee: Attach the fee call to a transaction given from stdin"));
    output.push(String::from("\tinspect: Inspect a transaction from stdin"));
    output.push(String::from("\tbroadcast: Read a transaction from stdin and broadcast it"));
    output.push(String::from(
        "\tsubscribe: Perform a scan and then subscribe to darkfid to listen for incoming blocks",
    ));
    output.push(String::from("\tunsubscribe: Stops the background subscription, if its active"));
    output.push(String::from("\tsnooze: Disables the background subscription messages printing"));
    output.push(String::from("\tunsnooze: Enables the background subscription messages printing"));
    output.push(String::from("\tscan: Scan the blockchain and parse relevant transactions"));
}

/// Auxiliary function to define the interactive shell completions.
fn completion(buffer: &str, lc: &mut Vec<String>) {
    // Split commands so we always process the last one
    let commands: Vec<&str> = buffer.split('|').collect();
    // Grab the prefix
    let prefix = if commands.len() > 1 {
        commands[..commands.len() - 1].join("|") + "| "
    } else {
        String::from("")
    };
    let last = commands.last().unwrap().trim_start();

    // First we define the specific commands prefixes
    if last.starts_with("h") {
        lc.push(prefix + "help");
        return
    }

    if last.starts_with("k") {
        lc.push(prefix + "kaching");
        return
    }

    if last.starts_with("p") {
        lc.push(prefix + "ping");
        return
    }

    if last.starts_with("c") {
        lc.push(prefix + "completions");
        return
    }

    if last.starts_with("w") {
        lc.push(prefix.clone() + "wallet");
        lc.push(prefix.clone() + "wallet initialize");
        lc.push(prefix.clone() + "wallet keygen");
        lc.push(prefix.clone() + "wallet balance");
        lc.push(prefix.clone() + "wallet address");
        lc.push(prefix.clone() + "wallet addresses");
        lc.push(prefix.clone() + "wallet default-address");
        lc.push(prefix.clone() + "wallet secrets");
        lc.push(prefix.clone() + "wallet import-secrets");
        lc.push(prefix.clone() + "wallet tree");
        lc.push(prefix + "wallet coins");
        return
    }

    if last.starts_with("sp") {
        lc.push(prefix + "spend");
        return
    }

    if last.starts_with("unsp") {
        lc.push(prefix + "unspend");
        return
    }

    if last.starts_with("t") {
        lc.push(prefix + "transfer");
        return
    }

    if last.starts_with("o") {
        lc.push(prefix.clone() + "otc");
        lc.push(prefix.clone() + "otc init");
        lc.push(prefix.clone() + "otc join");
        lc.push(prefix.clone() + "otc inspect");
        lc.push(prefix + "otc sign");
        return
    }

    if last.starts_with("a") {
        lc.push(prefix + "attach-fee");
        return
    }

    if last.starts_with("i") {
        lc.push(prefix + "inspect");
        return
    }

    if last.starts_with("b") {
        lc.push(prefix + "broadcast");
        return
    }

    if last.starts_with("su") {
        lc.push(prefix + "subscribe");
        return
    }

    if last.starts_with("unsu") {
        lc.push(prefix + "unsubscribe");
        return
    }

    if last.starts_with("sn") {
        lc.push(prefix + "snooze");
        return
    }

    if last.starts_with("unsn") {
        lc.push(prefix + "unsnooze");
        return
    }

    if last.starts_with("sc") {
        lc.push(prefix.clone() + "scan");
        lc.push(prefix + "scan --reset");
        return
    }

    // Now the catch alls
    if last.starts_with("s") {
        lc.push(prefix.clone() + "spend");
        lc.push(prefix.clone() + "subscribe");
        lc.push(prefix.clone() + "snooze");
        lc.push(prefix.clone() + "scan");
        lc.push(prefix + "scan --reset");
        return
    }

    if last.starts_with("u") {
        lc.push(prefix.clone() + "unspend");
        lc.push(prefix.clone() + "unsubscribe");
        lc.push(prefix + "unsnooze");
    }
}

/// Auxiliary function to define the interactive shell hints.
fn hints(buffer: &str) -> Option<(String, i32, bool)> {
    // Split commands so we always process the last one
    let commands: Vec<&str> = buffer.split('|').collect();
    let last = commands.last().unwrap().trim_start();
    match last {
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

        // Split commands
        let commands: Vec<&str> = line.split('|').collect();

        // Process each command
        let mut output = vec![];
        'commands_loop: for command in commands {
            let mut input = output;
            output = vec![];

            // Check if we have to output to a file
            let (mut command, file, append) = if command.contains('>') {
                // Check if we write or append to the file
                let mut split = ">";
                let mut append = false;
                if command.contains(">>") {
                    split = ">>";
                    append = true;
                }

                // Parse command parts
                let parts: Vec<&str> = command.split(split).collect();
                if parts.len() == 1 || parts[0].contains('>') {
                    output.push(format!("Malformed command output file definition: {command}"));
                    continue
                }
                let file = String::from(parts[1..].join("").trim());
                if file.is_empty() || file.contains('>') {
                    output.push(format!("Malformed command output file definition: {command}"));
                    continue
                }
                (parts[0], Some(file), append)
            } else {
                (command, None, false)
            };

            // Check if we have to use a file as input
            if command.contains('<') {
                // Parse command parts
                let parts: Vec<&str> = command.split('<').collect();
                if parts.len() == 1 {
                    output.push(format!("Malformed command input file definition: {command}"));
                    continue
                }

                // Read the input file name
                let file = String::from(parts[1..].join("").trim());
                if file.is_empty() {
                    output.push(format!("Malformed command input file definition: {command}"));
                    continue
                }

                // Expand the input file path
                let file_path = match expand_path(&file) {
                    Ok(p) => p,
                    Err(e) => {
                        output.push(format!("Error while expanding input file path: {e}"));
                        continue
                    }
                };

                // Read the file lines
                let file = match File::open(file_path) {
                    Ok(f) => f,
                    Err(e) => {
                        output.push(format!("Error while openning input file: {e}"));
                        continue
                    }
                };
                input = vec![];
                for (index, line) in BufReader::new(file).lines().enumerate() {
                    match line {
                        Ok(l) => input.push(l),
                        Err(e) => {
                            output
                                .push(format!("Error while reading input file line {index}: {e}"));
                            continue 'commands_loop
                        }
                    }
                }
                command = parts[0];
            }

            // Parse command parts
            let parts: Vec<&str> = command.split_whitespace().collect();
            if parts.is_empty() {
                continue
            }

            // Handle command
            match parts[0] {
                "help" => help(&mut output),
                "kaching" => kaching().await,
                "ping" => handle_ping(drk, &mut output).await,
                "completions" => handle_completions(&parts, &mut output),
                "wallet" => handle_wallet(drk, &parts, &input, &mut output).await,
                "spend" => handle_spend(drk, &input, &mut output).await,
                "unspend" => handle_unspend(drk, &parts, &mut output).await,
                "transfer" => handle_transfer(drk, &parts, &mut output).await,
                "otc" => handle_otc(drk, &parts, &input, &mut output).await,
                "attach-fee" => handle_attach_fee(drk, &input, &mut output).await,
                "inspect" => handle_inspect(&input, &mut output).await,
                "broadcast" => handle_broadcast(drk, &input, &mut output).await,
                "subscribe" => {
                    handle_subscribe(
                        drk,
                        endpoint,
                        &mut subscription_active,
                        &subscription_task,
                        &shell_sender,
                        ex,
                        &mut output,
                    )
                    .await
                }
                "unsubscribe" => {
                    handle_unsubscribe(&mut subscription_active, &subscription_task, &mut output)
                        .await
                }
                "snooze" => snooze_active = true,
                "unsnooze" => snooze_active = false,
                "scan" => handle_scan(drk, &subscription_active, &parts, &mut output).await,
                _ => output.push(format!("Unreconized command: {}", parts[0])),
            }

            // Write output to file, if requested
            if let Some(file) = file {
                // Expand the output file path
                let file_path = match expand_path(&file) {
                    Ok(p) => p,
                    Err(e) => {
                        output.push(format!("Error while expanding output file path: {e}"));
                        continue
                    }
                };

                // Open the file
                let file = if append {
                    OpenOptions::new().create(true).append(true).open(&file_path)
                } else {
                    File::create(file_path)
                };
                let mut file = match file {
                    Ok(f) => f,
                    Err(e) => {
                        output.push(format!("Error while openning output file: {e}"));
                        continue
                    }
                };

                // Seek end if we append to it
                if append {
                    if let Err(e) = file.seek(SeekFrom::End(0)) {
                        output.push(format!("Error while seeking end of output file: {e}"));
                        continue
                    }
                }

                // Write the output
                if let Err(e) = file.write_all((output.join("\n") + "\n").as_bytes()) {
                    output.push(format!("Error while writing output file: {e}"));
                    continue
                }
                output = vec![];
            }
        }

        // Handle last command output
        print_output(&output);
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
async fn handle_ping(drk: &DrkPtr, output: &mut Vec<String>) {
    if let Err(e) = drk.read().await.ping(output).await {
        output.push(format!("Error while executing ping command: {e}"))
    }
}

/// Auxiliary function to define the completions command handling.
fn handle_completions(parts: &[&str], output: &mut Vec<String>) {
    // Check correct command structure
    if parts.len() != 2 {
        output.push(String::from("Malformed `completions` command"));
        output.push(String::from("Usage: completions <shell>"));
        return
    }

    match generate_completions(parts[1]) {
        Ok(completions) => output.push(completions),
        Err(e) => output.push(format!("Error while executing completions command: {e}")),
    }
}

/// Auxiliary function to define the wallet command handling.
async fn handle_wallet(drk: &DrkPtr, parts: &[&str], input: &[String], output: &mut Vec<String>) {
    // Check correct command structure
    if parts.len() < 2 {
        output.push(String::from("Malformed `wallet` command"));
        output.push(String::from("Usage: wallet (initialize|keygen|balance|address|addresses|default-address|secrets|import-secrets|tree|coins)"));
        return
    }

    // Handle subcommand
    match parts[1] {
        "initialize" => handle_wallet_initialize(drk, output).await,
        "keygen" => handle_wallet_keygen(drk, output).await,
        "balance" => handle_wallet_balance(drk, output).await,
        "address" => handle_wallet_address(drk, output).await,
        "addresses" => handle_wallet_addresses(drk, output).await,
        "default-address" => handle_wallet_default_address(drk, parts, output).await,
        "secrets" => handle_wallet_secrets(drk, output).await,
        "import-secrets" => handle_wallet_import_secrets(drk, input, output).await,
        "tree" => handle_wallet_tree(drk, output).await,
        "coins" => handle_wallet_coins(drk, output).await,
        _ => {
            output.push(format!("Unreconized wallet subcommand: {}", parts[1]));
            output.push(String::from("Usage: wallet (initialize|keygen|balance|address|addresses|default-address|secrets|import-secrets|tree|coins)"));
        }
    }
}

/// Auxiliary function to define the wallet initialize subcommand handling.
async fn handle_wallet_initialize(drk: &DrkPtr, output: &mut Vec<String>) {
    let lock = drk.read().await;
    if let Err(e) = lock.initialize_wallet().await {
        output.push(format!("Error initializing wallet: {e:?}"));
        return
    }
    if let Err(e) = lock.initialize_money().await {
        output.push(format!("Failed to initialize Money: {e:?}"));
        return
    }
    if let Err(e) = lock.initialize_dao().await {
        output.push(format!("Failed to initialize DAO: {e:?}"));
        return
    }
    if let Err(e) = lock.initialize_deployooor() {
        output.push(format!("Failed to initialize Deployooor: {e:?}"));
    }
}

/// Auxiliary function to define the wallet keygen subcommand handling.
async fn handle_wallet_keygen(drk: &DrkPtr, output: &mut Vec<String>) {
    if let Err(e) = drk.read().await.money_keygen(output).await {
        output.push(format!("Failed to generate keypair: {e:?}"));
    }
}

/// Auxiliary function to define the wallet balance subcommand handling.
async fn handle_wallet_balance(drk: &DrkPtr, output: &mut Vec<String>) {
    let lock = drk.read().await;
    let balmap = match lock.money_balance().await {
        Ok(m) => m,
        Err(e) => {
            output.push(format!("Failed to fetch balances map: {e:?}"));
            return
        }
    };

    let aliases_map = match lock.get_aliases_mapped_by_token().await {
        Ok(m) => m,
        Err(e) => {
            output.push(format!("Failed to fetch aliases map: {e:?}"));
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
        output.push(String::from("No unspent balances found"));
    } else {
        output.push(format!("{table}"));
    }
}

/// Auxiliary function to define the wallet address subcommand handling.
async fn handle_wallet_address(drk: &DrkPtr, output: &mut Vec<String>) {
    match drk.read().await.default_address().await {
        Ok(address) => output.push(format!("{address}")),
        Err(e) => output.push(format!("Failed to fetch default address: {e:?}")),
    }
}

/// Auxiliary function to define the wallet addresses subcommand handling.
async fn handle_wallet_addresses(drk: &DrkPtr, output: &mut Vec<String>) {
    let addresses = match drk.read().await.addresses().await {
        Ok(a) => a,
        Err(e) => {
            output.push(format!("Failed to fetch addresses: {e:?}"));
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
        output.push(String::from("No addresses found"));
    } else {
        output.push(format!("{table}"));
    }
}

/// Auxiliary function to define the wallet default address subcommand handling.
async fn handle_wallet_default_address(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    if parts.len() != 3 {
        output.push(String::from("Malformed `wallet default-address` subcommand"));
        output.push(String::from("Usage: wallet default-address <index>"));
        return
    }

    let index = match usize::from_str(parts[2]) {
        Ok(i) => i,
        Err(e) => {
            output.push(format!("Invalid address id: {e:?}"));
            return
        }
    };

    if let Err(e) = drk.read().await.set_default_address(index) {
        output.push(format!("Failed to set default address: {e:?}"));
    }
}

/// Auxiliary function to define the wallet secrets subcommand handling.
async fn handle_wallet_secrets(drk: &DrkPtr, output: &mut Vec<String>) {
    match drk.read().await.get_money_secrets().await {
        Ok(secrets) => {
            for secret in secrets {
                output.push(format!("{secret}"));
            }
        }
        Err(e) => output.push(format!("Failed to fetch secrets: {e:?}")),
    }
}

/// Auxiliary function to define the wallet import secrets subcommand handling.
async fn handle_wallet_import_secrets(drk: &DrkPtr, input: &[String], output: &mut Vec<String>) {
    let mut secrets = vec![];
    // Parse input or read from stdin
    if input.is_empty() {
        for (i, line) in stdin().lines().enumerate() {
            let Ok(line) = line else { continue };

            let Ok(bytes) = bs58::decode(&line.trim()).into_vec() else {
                output.push(format!("Warning: Failed to decode secret on line {i}"));
                continue
            };
            let Ok(secret) = deserialize_async(&bytes).await else {
                output.push(format!("Warning: Failed to deserialize secret on line {i}"));
                continue
            };
            secrets.push(secret);
        }
    } else {
        for (i, line) in input.iter().enumerate() {
            let Ok(bytes) = bs58::decode(&line.trim()).into_vec() else {
                output.push(format!("Warning: Failed to decode secret on line {i}"));
                continue
            };
            let Ok(secret) = deserialize_async(&bytes).await else {
                output.push(format!("Warning: Failed to deserialize secret on line {i}"));
                continue
            };
            secrets.push(secret);
        }
    }

    match drk.read().await.import_money_secrets(secrets, output).await {
        Ok(pubkeys) => {
            for key in pubkeys {
                output.push(format!("{key}"));
            }
        }
        Err(e) => output.push(format!("Failed to import secrets: {e:?}")),
    }
}

/// Auxiliary function to define the wallet tree subcommand handling.
async fn handle_wallet_tree(drk: &DrkPtr, output: &mut Vec<String>) {
    match drk.read().await.get_money_tree().await {
        Ok(tree) => output.push(format!("{tree:#?}")),
        Err(e) => output.push(format!("Failed to fetch tree: {e:?}")),
    }
}

/// Auxiliary function to define the wallet coins subcommand handling.
async fn handle_wallet_coins(drk: &DrkPtr, output: &mut Vec<String>) {
    let lock = drk.read().await;
    let coins = match lock.get_coins(true).await {
        Ok(c) => c,
        Err(e) => {
            output.push(format!("Failed to fetch coins: {e:?}"));
            return
        }
    };

    if coins.is_empty() {
        return
    }

    let aliases_map = match lock.get_aliases_mapped_by_token().await {
        Ok(m) => m,
        Err(e) => {
            output.push(format!("Failed to fetch aliases map: {e:?}"));
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

    output.push(format!("{table}"));
}

/// Auxiliary function to define the spend command handling.
async fn handle_spend(drk: &DrkPtr, input: &[String], output: &mut Vec<String>) {
    let tx = match parse_tx_from_input(input).await {
        Ok(t) => t,
        Err(e) => {
            output.push(format!("Error while parsing transaction: {e}"));
            return
        }
    };

    if let Err(e) = drk.read().await.mark_tx_spend(&tx, output).await {
        output.push(format!("Failed to mark transaction coins as spent: {e}"))
    }
}

/// Auxiliary function to define the unspend command handling.
async fn handle_unspend(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct command structure
    if parts.len() != 2 {
        output.push(String::from("Malformed `unspend` command"));
        output.push(String::from("Usage: unspend <coin>"));
        return
    }

    let bytes = match bs58::decode(&parts[1]).into_vec() {
        Ok(b) => b,
        Err(e) => {
            output.push(format!("Invalid coin: {e}"));
            return
        }
    };

    let bytes: [u8; 32] = match bytes.try_into() {
        Ok(b) => b,
        Err(e) => {
            output.push(format!("Invalid coin: {e:?}"));
            return
        }
    };

    let elem: pallas::Base = match pallas::Base::from_repr(bytes).into() {
        Some(v) => v,
        None => {
            output.push(String::from("Invalid coin"));
            return
        }
    };

    if let Err(e) = drk.read().await.unspend_coin(&Coin::from(elem)).await {
        output.push(format!("Failed to mark coin as unspent: {e}"))
    }
}

/// Auxiliary function to define the transfer command handling.
async fn handle_transfer(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct command structure
    if parts.len() < 4 || parts.len() > 7 {
        output.push(String::from("Malformed `transfer` command"));
        output.push(String::from(
            "Usage: transfer [--half-split] <amount> <token> <recipient> [spend_hook] [user_data]",
        ));
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
        output.push(format!("Invalid amount: {e}"));
        return
    }
    index += 1;

    let lock = drk.read().await;
    let token_id = match lock.get_token(String::from(parts[index])).await {
        Ok(t) => t,
        Err(e) => {
            output.push(format!("Invalid token alias: {e}"));
            return
        }
    };
    index += 1;

    let rcpt = match PublicKey::from_str(parts[index]) {
        Ok(r) => r,
        Err(e) => {
            output.push(format!("Invalid recipient: {e}"));
            return
        }
    };
    index += 1;

    let spend_hook = if index < parts.len() {
        match FuncId::from_str(parts[index]) {
            Ok(s) => Some(s),
            Err(e) => {
                output.push(format!("Invalid spend hook: {e}"));
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
                output.push(format!("Invalid user data: {e}"));
                return
            }
        };

        let bytes: [u8; 32] = match bytes.try_into() {
            Ok(b) => b,
            Err(e) => {
                output.push(format!("Invalid user data: {e:?}"));
                return
            }
        };

        let elem: pallas::Base = match pallas::Base::from_repr(bytes).into() {
            Some(v) => v,
            None => {
                output.push(String::from("Invalid user data"));
                return
            }
        };

        Some(elem)
    } else {
        None
    };

    match lock.transfer(&amount, token_id, rcpt, spend_hook, user_data, half_split).await {
        Ok(t) => output.push(base64::encode(&serialize_async(&t).await)),
        Err(e) => output.push(format!("Failed to create payment transaction: {e}")),
    }
}

/// Auxiliary function to define the otc command handling.
async fn handle_otc(drk: &DrkPtr, parts: &[&str], input: &[String], output: &mut Vec<String>) {
    // Check correct command structure
    if parts.len() < 2 {
        output.push(String::from("Malformed `otc` command"));
        output.push(String::from("Usage: otc (init|join|inspect|sign)"));
        return
    }

    // Handle subcommand
    match parts[1] {
        "init" => handle_otc_init(drk, parts, output).await,
        "join" => handle_otc_join(drk, parts, input, output).await,
        "inspect" => handle_otc_inspect(drk, parts, input, output).await,
        "sign" => handle_otc_sign(drk, parts, input, output).await,
        _ => {
            output.push(format!("Unreconized OTC subcommand: {}", parts[1]));
            output.push(String::from("Usage: otc (init|join|inspect|sign)"));
        }
    }
}

/// Auxiliary function to define the otc init subcommand handling.
async fn handle_otc_init(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 4 {
        output.push(String::from("Malformed `otc init` subcommand"));
        output.push(String::from("Usage: otc init <value_pair> <token_pair>"));
        return
    }

    let value_pair = match parse_value_pair(parts[2]) {
        Ok(v) => v,
        Err(e) => {
            output.push(format!("Invalid value pair: {e}"));
            return
        }
    };

    let lock = drk.read().await;
    let token_pair = match parse_token_pair(&lock, parts[3]).await {
        Ok(t) => t,
        Err(e) => {
            output.push(format!("Invalid token pair: {e}"));
            return
        }
    };

    match lock.init_swap(value_pair, token_pair, None, None, None).await {
        Ok(half) => output.push(base64::encode(&serialize_async(&half).await)),
        Err(e) => output.push(format!("Failed to create swap transaction half: {e}")),
    }
}

/// Auxiliary function to define the otc join subcommand handling.
async fn handle_otc_join(drk: &DrkPtr, parts: &[&str], input: &[String], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 2 {
        output.push(String::from("Malformed `otc join` subcommand"));
        output.push(String::from("Usage: otc join"));
        return
    }

    // Parse line from input or fallback to stdin if its empty
    let buf = match input.len() {
        0 => {
            let mut buf = String::new();
            if let Err(e) = stdin().read_to_string(&mut buf) {
                output.push(format!("Failed to read from stdin: {e}"));
                return
            };
            buf
        }
        1 => input[0].clone(),
        _ => {
            output.push(String::from("Multiline input provided"));
            return
        }
    };

    let Some(bytes) = base64::decode(buf.trim()) else {
        output.push(String::from("Failed to decode partial swap data"));
        return
    };

    let partial: PartialSwapData = match deserialize_async(&bytes).await {
        Ok(p) => p,
        Err(e) => {
            output.push(format!("Failed to deserialize partial swap data: {e}"));
            return
        }
    };

    match drk.read().await.join_swap(partial, None, None, None).await {
        Ok(tx) => output.push(base64::encode(&serialize_async(&tx).await)),
        Err(e) => output.push(format!("Failed to create a join swap transaction: {e}")),
    }
}

/// Auxiliary function to define the otc inspect subcommand handling.
async fn handle_otc_inspect(
    drk: &DrkPtr,
    parts: &[&str],
    input: &[String],
    output: &mut Vec<String>,
) {
    // Check correct subcommand structure
    if parts.len() != 2 {
        output.push(String::from("Malformed `otc inspect` subcommand"));
        output.push(String::from("Usage: otc inspect"));
        return
    }

    // Parse line from input or fallback to stdin if its empty
    let buf = match input.len() {
        0 => {
            let mut buf = String::new();
            if let Err(e) = stdin().read_to_string(&mut buf) {
                output.push(format!("Failed to read from stdin: {e}"));
                return
            };
            buf
        }
        1 => input[0].clone(),
        _ => {
            output.push(String::from("Multiline input provided"));
            return
        }
    };

    let Some(bytes) = base64::decode(buf.trim()) else {
        output.push(String::from("Failed to decode swap transaction"));
        return
    };

    if let Err(e) = drk.read().await.inspect_swap(bytes, output).await {
        output.push(format!("Failed to inspect swap: {e}"));
    }
}

/// Auxiliary function to define the otc sign subcommand handling.
async fn handle_otc_sign(drk: &DrkPtr, parts: &[&str], input: &[String], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 2 {
        output.push(String::from("Malformed `otc sign` subcommand"));
        output.push(String::from("Usage: otc sign"));
        return
    }

    let mut tx = match parse_tx_from_input(input).await {
        Ok(t) => t,
        Err(e) => {
            output.push(format!("Error while parsing transaction: {e}"));
            return
        }
    };

    match drk.read().await.sign_swap(&mut tx).await {
        Ok(_) => output.push(base64::encode(&serialize_async(&tx).await)),
        Err(e) => output.push(format!("Failed to sign joined swap transaction: {e}")),
    }
}

/// Auxiliary function to define the attach fee command handling.
async fn handle_attach_fee(drk: &DrkPtr, input: &[String], output: &mut Vec<String>) {
    let mut tx = match parse_tx_from_input(input).await {
        Ok(t) => t,
        Err(e) => {
            output.push(format!("Error while parsing transaction: {e}"));
            return
        }
    };

    match drk.read().await.attach_fee(&mut tx).await {
        Ok(_) => output.push(base64::encode(&serialize_async(&tx).await)),
        Err(e) => output.push(format!("Failed to attach the fee call to the transaction: {e}")),
    }
}

/// Auxiliary function to define the inspect command handling.
async fn handle_inspect(input: &[String], output: &mut Vec<String>) {
    match parse_tx_from_input(input).await {
        Ok(tx) => output.push(format!("{tx:#?}")),
        Err(e) => output.push(format!("Error while parsing transaction: {e}")),
    }
}

/// Auxiliary function to define the broadcast command handling.
async fn handle_broadcast(drk: &DrkPtr, input: &[String], output: &mut Vec<String>) {
    let tx = match parse_tx_from_input(input).await {
        Ok(t) => t,
        Err(e) => {
            output.push(format!("Error while parsing transaction: {e}"));
            return
        }
    };

    let lock = drk.read().await;
    if let Err(e) = lock.simulate_tx(&tx).await {
        output.push(format!("Failed to simulate tx: {e}"));
        return
    };

    if let Err(e) = lock.mark_tx_spend(&tx, output).await {
        output.push(format!("Failed to mark transaction coins as spent: {e}"));
        return
    };

    match lock.broadcast_tx(&tx, output).await {
        Ok(txid) => output.push(format!("Transaction ID: {txid}")),
        Err(e) => output.push(format!("Failed to broadcast transaction: {e}")),
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
    output: &mut Vec<String>,
) {
    if *subscription_active {
        output.push(String::from("Subscription is already active!"));
        return
    }

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
async fn handle_unsubscribe(
    subscription_active: &mut bool,
    subscription_task: &StoppableTaskPtr,
    output: &mut Vec<String>,
) {
    if !*subscription_active {
        output.push(String::from("Subscription is already inactive!"));
        return
    }
    subscription_task.stop().await;
    *subscription_active = false;
}

/// Auxiliary function to define the scan command handling.
async fn handle_scan(
    drk: &DrkPtr,
    subscription_active: &bool,
    parts: &[&str],
    output: &mut Vec<String>,
) {
    if *subscription_active {
        output.push(String::from("Subscription is already active!"));
        return
    }

    // Check correct command structure
    if parts.len() != 1 && parts.len() != 3 {
        output.push(String::from("Malformed `scan` command"));
        return
    }

    // Check if reset was requested
    let lock = drk.read().await;
    if parts.len() == 3 {
        if parts[1] != "--reset" {
            output.push(String::from("Malformed `scan` command"));
            output.push(String::from("Usage: scan --reset <height>"));
            return
        }

        let height = match u32::from_str(parts[2]) {
            Ok(h) => h,
            Err(e) => {
                output.push(format!("Invalid reset height: {e:?}"));
                return
            }
        };

        if let Err(e) = lock.reset_to_height(height, output) {
            output.push(format!("Failed during wallet reset: {e:?}"));
            return
        }
    }

    if let Err(e) = lock.scan_blocks(output).await {
        output.push(format!("Failed during scanning: {e:?}"));
        return
    }
    output.push(String::from("Finished scanning blockchain"));
}
