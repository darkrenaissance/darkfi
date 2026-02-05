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
use rand::rngs::OsRng;
use smol::channel::{Receiver, Sender};
use url::Url;

use darkfi::{
    cli_desc,
    system::{msleep, ExecutorPtr, StoppableTask, StoppableTaskPtr},
    util::{
        encoding::base64,
        parse::{decode_base10, encode_base10},
        path::expand_path,
    },
    zk::halo2::Field,
    Error,
};
use darkfi_dao_contract::{blockwindow, model::DaoProposalBulla, DaoFunction};
use darkfi_money_contract::model::{Coin, CoinAttributes, TokenId};
use darkfi_sdk::{
    crypto::{
        keypair::{Address, StandardAddress},
        note::AeadEncryptedNote,
        BaseBlind, ContractId, FuncId, FuncRef, Keypair, SecretKey, DAO_CONTRACT_ID,
    },
    pasta::{group::ff::PrimeField, pallas},
    tx::TransactionHash,
};
use darkfi_serial::{deserialize_async, serialize_async};

use crate::{
    cli_util::{
        append_or_print, display_mining_config, generate_completions, kaching,
        parse_calls_from_input, parse_mining_config_from_input, parse_token_pair, parse_tree,
        parse_tx_from_input, parse_value_pair, print_output, tx_from_calls_mapped,
    },
    common::*,
    dao::{DaoParams, ProposalRecord},
    money::BALANCE_BASE10_DECIMALS,
    rpc::subscribe_blocks,
    swap::PartialSwapData,
    DrkPtr,
};

// TODO:
//  1. Create a transactions cache in the wallet db, so you can use it to handle them.

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
    output.push(String::from("\tdao: DAO functionalities"));
    output
        .push(String::from("\tattach-fee: Attach the fee call to a transaction given from stdin"));
    output.push(String::from(
        "\ttx-from-calls: Create a transaction from newline-separated calls from stdin and attach the fee call",
    ));
    output.push(String::from("\tinspect: Inspect a transaction from stdin"));
    output.push(String::from("\tbroadcast: Read a transaction from stdin and broadcast it"));
    output.push(String::from(
        "\tsubscribe: Perform a scan and then subscribe to darkfid to listen for incoming blocks",
    ));
    output.push(String::from("\tunsubscribe: Stops the background subscription, if its active"));
    output.push(String::from("\tsnooze: Disables the background subscription messages printing"));
    output.push(String::from("\tunsnooze: Enables the background subscription messages printing"));
    output.push(String::from("\tscan: Scan the blockchain and parse relevant transactions"));
    output.push(String::from("\texplorer: Explorer related subcommands"));
    output.push(String::from("\talias: Token alias"));
    output.push(String::from("\ttoken: Token functionalities"));
    output.push(String::from("\tcontract: Contract functionalities"));
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

    if last.starts_with("com") {
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
        lc.push(prefix.clone() + "wallet coins");
        lc.push(prefix + "wallet mining-config");
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

    if last.starts_with("tr") {
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

    if last.starts_with("d") {
        lc.push(prefix.clone() + "dao");
        lc.push(prefix.clone() + "dao create");
        lc.push(prefix.clone() + "dao view");
        lc.push(prefix.clone() + "dao import");
        lc.push(prefix.clone() + "dao remove");
        lc.push(prefix.clone() + "dao list");
        lc.push(prefix.clone() + "dao balance");
        lc.push(prefix.clone() + "dao mint");
        lc.push(prefix.clone() + "dao propose-transfer");
        lc.push(prefix.clone() + "dao propose-generic");
        lc.push(prefix.clone() + "dao proposals");
        lc.push(prefix.clone() + "dao proposal");
        lc.push(prefix.clone() + "dao proposal-import");
        lc.push(prefix.clone() + "dao vote");
        lc.push(prefix.clone() + "dao exec");
        lc.push(prefix.clone() + "dao spend-hook");
        lc.push(prefix + "dao mining-config");
        return
    }

    if last.starts_with("at") {
        lc.push(prefix + "attach-fee");
        return
    }

    if last.starts_with("tx") {
        lc.push(prefix + "tx-from-calls");
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

    if last.starts_with("e") {
        lc.push(prefix.clone() + "explorer");
        lc.push(prefix.clone() + "explorer fetch-tx");
        lc.push(prefix.clone() + "explorer simulate-tx");
        lc.push(prefix.clone() + "explorer txs-history");
        lc.push(prefix.clone() + "explorer clear-reverted");
        lc.push(prefix.clone() + "explorer scanned-blocks");
        lc.push(prefix + "explorer mining-config");
        return
    }

    if last.starts_with("al") {
        lc.push(prefix.clone() + "alias");
        lc.push(prefix.clone() + "alias add");
        lc.push(prefix.clone() + "alias show");
        lc.push(prefix + "alias remove");
        return
    }

    if last.starts_with("to") {
        lc.push(prefix.clone() + "token");
        lc.push(prefix.clone() + "token import");
        lc.push(prefix.clone() + "token generate-mint");
        lc.push(prefix.clone() + "token list");
        lc.push(prefix.clone() + "token mint");
        lc.push(prefix + "token freeze");
        return
    }

    if last.starts_with("con") {
        lc.push(prefix.clone() + "contract");
        lc.push(prefix.clone() + "contract generate-deploy");
        lc.push(prefix.clone() + "contract list");
        lc.push(prefix.clone() + "contract export-data");
        lc.push(prefix.clone() + "contract deploy");
        lc.push(prefix + "contract lock");
        return
    }

    // Now the catch alls
    if last.starts_with("a") {
        lc.push(prefix.clone() + "attach-fee");
        lc.push(prefix.clone() + "alias");
        lc.push(prefix.clone() + "alias add");
        lc.push(prefix.clone() + "alias show");
        lc.push(prefix + "alias remove");
        return
    }

    if last.starts_with("c") {
        lc.push(prefix.clone() + "completions");
        lc.push(prefix.clone() + "contract");
        lc.push(prefix.clone() + "contract generate-deploy");
        lc.push(prefix.clone() + "contract list");
        lc.push(prefix.clone() + "contract export-data");
        lc.push(prefix.clone() + "contract deploy");
        lc.push(prefix + "contract lock");
        return
    }

    if last.starts_with("s") {
        lc.push(prefix.clone() + "spend");
        lc.push(prefix.clone() + "subscribe");
        lc.push(prefix.clone() + "snooze");
        lc.push(prefix.clone() + "scan");
        lc.push(prefix + "scan --reset");
        return
    }

    if last.starts_with("t") {
        lc.push(prefix.clone() + "transfer");
        lc.push(prefix.clone() + "token");
        lc.push(prefix.clone() + "token import");
        lc.push(prefix.clone() + "token generate-mint");
        lc.push(prefix.clone() + "token list");
        lc.push(prefix.clone() + "token mint");
        lc.push(prefix + "token freeze");
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
    let color = 35; // magenta
    let bold = false;
    match last {
        "completions " => Some(("<shell>".to_string(), color, bold)),
        "wallet " => Some(("(initialize|keygen|balance|address|addresses|default-address|secrets|import-secrets|tree|coins|mining-config)".to_string(), color, bold)),
        "wallet default-address " => Some(("<index>".to_string(), color, bold)),
        "wallet mining-config " => Some(("<index> [spend_hook] [user_data]".to_string(), color, bold)),
        "unspend " => Some(("<coin>".to_string(), color, bold)),
        "transfer " => Some(("[--half-split] <amount> <token> <recipient> [spend_hook] [user_data]".to_string(), color, bold)),
        "otc " => Some(("(init|join|inspect|sign)".to_string(), color, bold)),
        "otc init " => Some(("<value_pair> <token_pair>".to_string(), color, bold)),
        "dao " => Some(("(create|view|import|remove|list|balance|mint|propose-transfer|propose-generic|proposals|proposal|proposal-import|vote|exec|spend-hook|mining-config)".to_string(), color, bold)),
        "dao create " => Some(("<proposer-limit> <quorum> <early-exec-quorum> <approval-ratio> <gov-token-id>".to_string(), color, bold)),
        "dao import " => Some(("<name>".to_string(), color, bold)),
        "dao remove " => Some(("<name>".to_string(), color, bold)),
        "dao list " => Some(("[name]".to_string(), color, bold)),
        "dao balance " => Some(("<name>".to_string(), color, bold)),
        "dao mint " => Some(("<name>".to_string(), color, bold)),
        "dao propose-transfer " => Some(("<name> <duration> <amount> <token> <recipient> [spend-hook] [user-data]".to_string(), color, bold)),
        "dao propose-generic" => Some(("<name> <duration> [user-data]".to_string(), color, bold)),
        "dao proposals " => Some(("<name>".to_string(), color, bold)),
        "dao proposal " => Some(("[--(export|mint-proposal)] <bulla>".to_string(), color, bold)),
        "dao vote " => Some(("<bulla> <vote> [vote-weight]".to_string(), color, bold)),
        "dao exec " => Some(("[--early] <bulla>".to_string(), color, bold)),
        "dao mining-config " => Some(("<name>".to_string(), color, bold)),
        "scan " => Some(("[--reset]".to_string(), color, bold)),
        "scan --reset " => Some(("<height>".to_string(), color, bold)),
        "explorer " => Some(("(fetch-tx|simulate-tx|txs-history|clear-reverted|scanned-blocks|mining-config)".to_string(), color, bold)),
        "explorer fetch-tx " => Some(("[--encode] <tx-hash>".to_string(), color, bold)),
        "explorer txs-history " => Some(("[--encode] [tx-hash]".to_string(), color, bold)),
        "explorer scanned-blocks " => Some(("[height]".to_string(), color, bold)),
        "alias " => Some(("(add|show|remove)".to_string(), color, bold)),
        "alias add " => Some(("<alias> <token>".to_string(), color, bold)),
        "alias show " => Some(("[-a, --alias <alias>] [-t, --token <token>]".to_string(), color, bold)),
        "alias remove " => Some(("<alias>".to_string(), color, bold)),
        "token " => Some(("(import|generate-mint|list|mint|freeze)".to_string(), color, bold)),
        "token import " => Some(("<secret-key> <token-blind>".to_string(), color, bold)),
        "token mint " => Some(("<token> <amount> <recipient> [spend-hook] [user-data]".to_string(), color, bold)),
        "token freeze " => Some(("<token>".to_string(), color, bold)),
        "contract " => Some(("(generate-deploy|list|export-data|deploy|lock)".to_string(), color, bold)),
        "contract list " => Some(("[contract-id]".to_string(), color, bold)),
        "contract export-data " => Some(("<tx-hash>".to_string(), color, bold)),
        "contract deploy " => Some(("<deploy-auth> <wasm-path> [deploy-ix]".to_string(), color, bold)),
        "contract lock " => Some(("<deploy-auth>".to_string(), color, bold)),
        "tx-from-calls " => Some(("[calls-map]".to_string(), color, bold)),
        _ => None,
    }
}

/// Auxiliary function to start provided Drk as an interactive shell.
/// Only sane/linenoise terminals are suported.
pub async fn interactive(
    drk: &DrkPtr,
    endpoint: &Url,
    history_path: &str,
    shell_sender: &Sender<Vec<String>>,
    shell_receiver: &Receiver<Vec<String>>,
    ex: &ExecutorPtr,
) {
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

    // Create two detached tasks to use for block subscription
    let mut subscription_active = false;
    let mut snooze_active = false;
    let subscription_tasks = [StoppableTask::new(), StoppableTask::new()];

    // Start the interactive shell
    loop {
        // Wait for next line to process
        let line = listen_for_line(&snooze_active, shell_receiver).await;

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
        'commands_loop: for (command_index, command) in commands.iter().enumerate() {
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
                (*command, None, false)
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
                "dao" => handle_dao(drk, &parts, &input, &mut output).await,
                "attach-fee" => handle_attach_fee(drk, &input, &mut output).await,
                "tx-from-calls" => handle_tx_from_calls(drk, &parts, &input, &mut output).await,
                "inspect" => handle_inspect(&input, &mut output).await,
                "broadcast" => handle_broadcast(drk, &input, &mut output).await,
                "subscribe" => {
                    handle_subscribe(
                        drk,
                        endpoint,
                        &mut subscription_active,
                        &subscription_tasks,
                        shell_sender,
                        ex,
                    )
                    .await
                }
                "unsubscribe" => {
                    handle_unsubscribe(&mut subscription_active, &subscription_tasks).await
                }
                "snooze" => snooze_active = true,
                "unsnooze" => snooze_active = false,
                "scan" => {
                    handle_scan(
                        drk,
                        &subscription_active,
                        &parts,
                        &mut output,
                        &(command_index + 1 == commands.len() && file.is_none()),
                    )
                    .await
                }
                "explorer" => handle_explorer(drk, &parts, &input, &mut output).await,
                "alias" => handle_alias(drk, &parts, &mut output).await,
                "token" => handle_token(drk, &parts, &mut output).await,
                "contract" => handle_contract(drk, &parts, &mut output).await,
                _ => output.push(format!("Unrecognized command: {}", parts[0])),
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

    // Stop the subscription tasks if they are active
    subscription_tasks[0].stop_nowait();
    subscription_tasks[1].stop_nowait();

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
                // We only print if snooze is inactive,
                // but have to consume the message regardless,
                // so the queue gets empty.
                if *snooze_active {
                    while !shell_receiver.is_empty() {
                        if let Err(e) = shell_receiver.recv().await {
                            eprintln!("Error while reading shell receiver channel: {e}");
                            break
                        }
                    }
                } else {
                    // Hide prompt
                    if let Err(e) = state.hide() {
                        eprintln!("Error while hiding linenoise state: {e}");
                        break
                    };

                    // Restore blocking mode
                    unsafe {
                        let flags = fcntl(fd, F_GETFL, 0);
                        fcntl(fd, F_SETFL, flags & !O_NONBLOCK);
                    }

                    // Print output
                    while !shell_receiver.is_empty() {
                        match shell_receiver.recv().await {
                            Ok(msg) => {
                                for line in msg {
                                    println!("{}\r", line.replace("\n", "\n\r"));
                                }
                            }
                            Err(e) => {
                                eprintln!("Error while reading shell receiver channel: {e}");
                                break
                            }
                        }
                    }

                    // Set stdin to non-blocking mode
                    unsafe {
                        let flags = fcntl(fd, F_GETFL, 0);
                        fcntl(fd, F_SETFL, flags | O_NONBLOCK);
                    }

                    // Show prompt again
                    if let Err(e) = state.show() {
                        eprintln!("Error while showing linenoise state: {e}");
                        break
                    };
                }
            }
        }
    }

    // Restore blocking mode
    unsafe {
        let flags = fcntl(fd, F_GETFL, 0);
        fcntl(fd, F_SETFL, flags & !O_NONBLOCK);
    }

    if let Err(e) = state.edit_stop() {
        eprintln!("Error while stopping linenoise state: {e}");
    };
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
        output.push(String::from("Usage: wallet (initialize|keygen|balance|address|addresses|default-address|secrets|import-secrets|tree|coins|mining-config)"));
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
        "mining-config" => handle_wallet_mining_config(drk, parts, output).await,
        _ => {
            output.push(format!("Unrecognized wallet subcommand: {}", parts[1]));
            output.push(String::from("Usage: wallet (initialize|keygen|balance|address|addresses|default-address|secrets|import-secrets|tree|coins|mining-config)"));
        }
    }
}

/// Auxiliary function to define the wallet initialize subcommand handling.
async fn handle_wallet_initialize(drk: &DrkPtr, output: &mut Vec<String>) {
    let lock = drk.read().await;
    if let Err(e) = lock.initialize_wallet().await {
        output.push(format!("Error initializing wallet: {e}"));
        return
    }
    if let Err(e) = lock.initialize_money(output).await {
        output.push(format!("Failed to initialize Money: {e}"));
        return
    }
    if let Err(e) = lock.initialize_dao().await {
        output.push(format!("Failed to initialize DAO: {e}"));
        return
    }
    if let Err(e) = lock.initialize_deployooor() {
        output.push(format!("Failed to initialize Deployooor: {e}"));
    }
}

/// Auxiliary function to define the wallet keygen subcommand handling.
async fn handle_wallet_keygen(drk: &DrkPtr, output: &mut Vec<String>) {
    if let Err(e) = drk.read().await.money_keygen(output).await {
        output.push(format!("Failed to generate keypair: {e}"));
    }
}

/// Auxiliary function to define the wallet balance subcommand handling.
async fn handle_wallet_balance(drk: &DrkPtr, output: &mut Vec<String>) {
    let drk = drk.read().await;

    let balmap = match drk.money_balance().await {
        Ok(m) => m,
        Err(e) => {
            output.push(format!("Failed to fetch balances map: {e}"));
            return
        }
    };

    let alimap = match drk.get_aliases_mapped_by_token().await {
        Ok(m) => m,
        Err(e) => {
            output.push(format!("Failed to fetch aliases map: {e}"));
            return
        }
    };

    let table = prettytable_balance(&balmap, &alimap);

    if table.is_empty() {
        output.push(String::from("No unspent balances found"));
    } else {
        output.push(format!("{table}"));
    }
}

/// Auxiliary function to define the wallet address subcommand handling.
async fn handle_wallet_address(drk: &DrkPtr, output: &mut Vec<String>) {
    let drk = drk.read().await;

    let public_key = match drk.default_address().await {
        Ok(v) => v,
        Err(e) => {
            output.push(format!("Failed to fetch default address: {e}"));
            return
        }
    };

    let addr: Address = StandardAddress::from_public(drk.network, public_key).into();
    output.push(format!("{addr}"));
}

/// Auxiliary function to define the wallet addresses subcommand handling.
async fn handle_wallet_addresses(drk: &DrkPtr, output: &mut Vec<String>) {
    let drk = drk.read().await;
    let network = drk.network;
    let addresses = match drk.addresses().await {
        Ok(v) => v,
        Err(e) => {
            output.push(format!("Failed to fetch addresses: {e}"));
            return
        }
    };

    let table = prettytable_addrs(network, &addresses);

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
            output.push(format!("Invalid address id: {e}"));
            return
        }
    };

    if let Err(e) = drk.read().await.set_default_address(index) {
        output.push(format!("Failed to set default address: {e}"));
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
        Err(e) => output.push(format!("Failed to fetch secrets: {e}")),
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
        Err(e) => output.push(format!("Failed to import secrets: {e}")),
    }
}

/// Auxiliary function to define the wallet tree subcommand handling.
async fn handle_wallet_tree(drk: &DrkPtr, output: &mut Vec<String>) {
    match drk.read().await.get_money_tree().await {
        Ok(tree) => output.push(format!("{tree:#?}")),
        Err(e) => output.push(format!("Failed to fetch tree: {e}")),
    }
}

/// Auxiliary function to define the wallet coins subcommand handling.
async fn handle_wallet_coins(drk: &DrkPtr, output: &mut Vec<String>) {
    let lock = drk.read().await;
    let coins = match lock.get_coins(true).await {
        Ok(c) => c,
        Err(e) => {
            output.push(format!("Failed to fetch coins: {e}"));
            return
        }
    };

    if coins.is_empty() {
        return
    }

    let aliases_map = match lock.get_aliases_mapped_by_token().await {
        Ok(m) => m,
        Err(e) => {
            output.push(format!("Failed to fetch aliases map: {e}"));
            return
        }
    };

    let table = prettytable_coins(&coins, &aliases_map);
    output.push(format!("{table}"));
}

/// Auxiliary function to define the wallet mining config subcommand handling.
async fn handle_wallet_mining_config(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct command structure
    if parts.len() < 3 || parts.len() > 5 {
        output.push(String::from("Malformed `wallet mining-address` subcommand"));
        output.push(String::from("Usage: wallet mining-config <index> [spend_hook] [user_data]"));
        return
    }

    // Parse command
    let mut index = 2;
    let wallet_index = match usize::from_str(parts[index]) {
        Ok(i) => i,
        Err(e) => {
            output.push(format!("Invalid address id: {e}"));
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

    if let Err(e) =
        drk.read().await.mining_config(wallet_index, spend_hook, user_data, output).await
    {
        output.push(format!("Failed to generate wallet mining configuration: {e}"));
    }
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
            output.push(format!("Invalid token ID: {e}"));
            return
        }
    };
    index += 1;

    let rcpt = match Address::from_str(parts[index]) {
        Ok(r) => r,
        Err(e) => {
            output.push(format!("Invalid recipient: {e}"));
            return
        }
    };

    if rcpt.network() != lock.network {
        output.push("Mismatched recipient address prefix".to_string());
        return
    }

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

    match lock
        .transfer(&amount, token_id, *rcpt.public_key(), spend_hook, user_data, half_split)
        .await
    {
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
            output.push(format!("Unrecognized OTC subcommand: {}", parts[1]));
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

/// Auxiliary function to define the dao command handling.
async fn handle_dao(drk: &DrkPtr, parts: &[&str], input: &[String], output: &mut Vec<String>) {
    // Check correct command structure
    if parts.len() < 2 {
        output.push(String::from("Malformed `dao` command"));
        output.push(String::from("Usage: dao (create|view|import|remove|list|balance|mint|propose-transfer|propose-generic|proposals|proposal|proposal-import|vote|exec|spend-hook|mining-config)"));
        return
    }

    // Handle subcommand
    match parts[1] {
        "create" => handle_dao_create(drk, parts, output).await,
        "view" => handle_dao_view(parts, input, output).await,
        "import" => handle_dao_import(drk, parts, input, output).await,
        "remove" => handle_dao_remove(drk, parts, output).await,
        "list" => handle_dao_list(drk, parts, output).await,
        "balance" => handle_dao_balance(drk, parts, output).await,
        "mint" => handle_dao_mint(drk, parts, output).await,
        "propose-transfer" => handle_dao_propose_transfer(drk, parts, output).await,
        "propose-generic" => handle_dao_propose_generic(drk, parts, output).await,
        "proposals" => handle_dao_proposals(drk, parts, output).await,
        "proposal" => handle_dao_proposal(drk, parts, output).await,
        "proposal-import" => handle_dao_proposal_import(drk, parts, input, output).await,
        "vote" => handle_dao_vote(drk, parts, output).await,
        "exec" => handle_dao_exec(drk, parts, output).await,
        "spend-hook" => handle_dao_spend_hook(parts, output).await,
        "mining-config" => handle_dao_mining_config(drk, parts, output).await,
        _ => {
            output.push(format!("Unrecognized DAO subcommand: {}", parts[1]));
            output.push(String::from("Usage: dao (create|view|import|remove|list|balance|mint|propose-transfer|propose-generic|proposals|proposal|proposal-import|vote|exec|spend-hook|mining-config)"));
        }
    }
}

/// Auxiliary function to define the dao create subcommand handling.
async fn handle_dao_create(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 7 {
        output.push(String::from("Malformed `dao create` subcommand"));
        output.push(String::from("Usage: dao create <proposer-limit> <quorum> <early-exec-quorum> <approval-ratio> <gov-token-id>"));
        return
    }

    if let Err(e) = f64::from_str(parts[2]) {
        output.push(format!("Invalid proposer limit: {e}"));
        return
    }
    let proposer_limit = match decode_base10(parts[2], BALANCE_BASE10_DECIMALS, true) {
        Ok(p) => p,
        Err(e) => {
            output.push(format!("Error while parsing proposer limit: {e}"));
            return
        }
    };

    if let Err(e) = f64::from_str(parts[3]) {
        output.push(format!("Invalid quorum: {e}"));
        return
    }
    let quorum = match decode_base10(parts[3], BALANCE_BASE10_DECIMALS, true) {
        Ok(q) => q,
        Err(e) => {
            output.push(format!("Error while parsing quorum: {e}"));
            return
        }
    };

    if let Err(e) = f64::from_str(parts[4]) {
        output.push(format!("Invalid early exec quorum: {e}"));
        return
    }
    let early_exec_quorum = match decode_base10(parts[4], BALANCE_BASE10_DECIMALS, true) {
        Ok(e) => e,
        Err(e) => {
            output.push(format!("Error while parsing early exec quorum: {e}"));
            return
        }
    };

    let approval_ratio = match f64::from_str(parts[5]) {
        Ok(a) => {
            if a > 1.0 {
                output.push(String::from("Error: Approval ratio cannot be >1.0"));
                return
            }
            a
        }
        Err(e) => {
            output.push(format!("Invalid approval ratio: {e}"));
            return
        }
    };
    let approval_ratio_base = 100_u64;
    let approval_ratio_quot = (approval_ratio * approval_ratio_base as f64) as u64;

    let gov_token_id = match drk.read().await.get_token(String::from(parts[6])).await {
        Ok(g) => g,
        Err(e) => {
            output.push(format!("Invalid Token ID: {e}"));
            return
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

    output.push(params.toml_str());
}

/// Auxiliary function to define the dao view subcommand handling.
async fn handle_dao_view(parts: &[&str], input: &[String], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 2 {
        output.push(String::from("Malformed `dao view` subcommand"));
        output.push(String::from("Usage: dao view"));
        return
    }

    // Parse lines from input or fallback to stdin if its empty
    let buf = match input.len() {
        0 => {
            let mut buf = String::new();
            if let Err(e) = stdin().read_to_string(&mut buf) {
                output.push(format!("Failed to read from stdin: {e}"));
                return
            };
            buf
        }
        _ => input.join("\n"),
    };

    let params = match DaoParams::from_toml_str(&buf) {
        Ok(p) => p,
        Err(e) => {
            output.push(format!("Error while parsing DAO params: {e}"));
            return
        }
    };

    output.push(format!("{params}"));
}

/// Auxiliary function to define the dao import subcommand handling.
async fn handle_dao_import(
    drk: &DrkPtr,
    parts: &[&str],
    input: &[String],
    output: &mut Vec<String>,
) {
    // Check correct subcommand structure
    if parts.len() != 3 {
        output.push(String::from("Malformed `dao import` subcommand"));
        output.push(String::from("Usage: dao import <name>"));
        return
    }

    // Parse lines from input or fallback to stdin if its empty
    let buf = match input.len() {
        0 => {
            let mut buf = String::new();
            if let Err(e) = stdin().read_to_string(&mut buf) {
                output.push(format!("Failed to read from stdin: {e}"));
                return
            };
            buf
        }
        _ => input.join("\n"),
    };

    let params = match DaoParams::from_toml_str(&buf) {
        Ok(p) => p,
        Err(e) => {
            output.push(format!("Error while parsing DAO params: {e}"));
            return
        }
    };

    if let Err(e) = drk.read().await.import_dao(parts[2], &params, output).await {
        output.push(format!("Failed to import DAO: {e}"))
    }
}

/// Auxiliary function to define the dao remove subcommand handling.
async fn handle_dao_remove(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 3 {
        output.push(String::from("Malformed `dao remove` subcommand"));
        output.push(String::from("Usage: dao remove <name>"));
        return
    }

    if let Err(e) = drk.read().await.remove_dao(parts[2], output).await {
        output.push(format!("Failed to remove DAO: {e}"))
    }
}

/// Auxiliary function to define the dao list subcommand handling.
async fn handle_dao_list(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 2 && parts.len() != 3 {
        output.push(String::from("Malformed `dao list` subcommand"));
        output.push(String::from("Usage: dao list [name]"));
        return
    }

    let name = if parts.len() == 3 { Some(String::from(parts[2])) } else { None };

    if let Err(e) = drk.read().await.dao_list(&name, output).await {
        output.push(format!("Failed to list DAO: {e}"))
    }
}

/// Auxiliary function to define the dao balance subcommand handling.
async fn handle_dao_balance(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 3 {
        output.push(String::from("Malformed `dao balance` subcommand"));
        output.push(String::from("Usage: dao balance <name>"));
        return
    }

    let lock = drk.read().await;
    let balmap = match lock.dao_balance(parts[2]).await {
        Ok(b) => b,
        Err(e) => {
            output.push(format!("Failed to fetch DAO balance: {e}"));
            return
        }
    };

    let alimap = match lock.get_aliases_mapped_by_token().await {
        Ok(m) => m,
        Err(e) => {
            output.push(format!("Failed to fetch aliases map: {e}"));
            return
        }
    };

    let table = prettytable_balance(&balmap, &alimap);

    if table.is_empty() {
        output.push(String::from("No unspent balances found"))
    } else {
        output.push(format!("{table}"));
    }
}

/// Auxiliary function to define the dao mint subcommand handling.
async fn handle_dao_mint(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 3 {
        output.push(String::from("Malformed `dao mint` subcommand"));
        output.push(String::from("Usage: dao mint <name>"));
        return
    }

    match drk.read().await.dao_mint(parts[2]).await {
        Ok(tx) => output.push(base64::encode(&serialize_async(&tx).await)),
        Err(e) => output.push(format!("Failed to mint DAO: {e}")),
    }
}

/// Auxiliary function to define the dao propose transfer subcommand handling.
async fn handle_dao_propose_transfer(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() < 7 || parts.len() > 9 {
        output.push(String::from("Malformed `dao proposal-transfer` subcommand"));
        output.push(String::from("Usage: dao proposal-transfer <name> <duration> <amount> <token> <recipient> [spend-hook] [user-data]"));
        return
    }

    let duration = match u64::from_str(parts[3]) {
        Ok(d) => d,
        Err(e) => {
            output.push(format!("Invalid duration: {e}"));
            return
        }
    };

    let amount = String::from(parts[4]);
    if let Err(e) = f64::from_str(&amount) {
        output.push(format!("Invalid amount: {e}"));
        return
    }

    let lock = drk.read().await;
    let token_id = match lock.get_token(String::from(parts[5])).await {
        Ok(t) => t,
        Err(e) => {
            output.push(format!("Invalid token ID: {e}"));
            return
        }
    };

    let rcpt = match Address::from_str(parts[6]) {
        Ok(r) => r,
        Err(e) => {
            output.push(format!("Invalid recipient: {e}"));
            return
        }
    };

    if rcpt.network() != lock.network {
        output.push("Recipient address prefix mismatch".to_string());
        return
    }

    let mut index = 7;
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

    match drk
        .read()
        .await
        .dao_propose_transfer(
            parts[2],
            duration,
            &amount,
            token_id,
            *rcpt.public_key(),
            spend_hook,
            user_data,
        )
        .await
    {
        Ok(proposal) => output.push(format!("Generated proposal: {}", proposal.bulla())),
        Err(e) => output.push(format!("Failed to create DAO transfer proposal: {e}")),
    }
}

/// Auxiliary function to define the dao propose generic subcommand handling.
async fn handle_dao_propose_generic(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 4 && parts.len() != 5 {
        output.push(String::from("Malformed `dao proposal-generic` subcommand"));
        output.push(String::from("Usage: dao proposal-generic <name> <duration> [user-data]"));
        return
    }

    let duration = match u64::from_str(parts[3]) {
        Ok(d) => d,
        Err(e) => {
            output.push(format!("Invalid duration: {e}"));
            return
        }
    };

    let user_data = if parts.len() == 5 {
        let bytes = match bs58::decode(&parts[4]).into_vec() {
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

    match drk.read().await.dao_propose_generic(parts[2], duration, user_data).await {
        Ok(proposal) => output.push(format!("Generated proposal: {}", proposal.bulla())),
        Err(e) => output.push(format!("Failed to create DAO generic proposal: {e}")),
    }
}

/// Auxiliary function to define the dao proposals subcommand handling.
async fn handle_dao_proposals(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 3 {
        output.push(String::from("Malformed `dao proposals` subcommand"));
        output.push(String::from("Usage: dao proposals <name>"));
        return
    }

    match drk.read().await.get_dao_proposals(parts[2]).await {
        Ok(proposals) => {
            for (i, proposal) in proposals.iter().enumerate() {
                output.push(format!("{i}. {}", proposal.bulla()));
            }
        }
        Err(e) => output.push(format!("Failed to retrieve DAO proposals: {e}")),
    }
}

/// Auxiliary function to define the dao proposal subcommand handling.
async fn handle_dao_proposal(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 3 && parts.len() != 4 {
        output.push(String::from("Malformed `dao proposal` subcommand"));
        output.push(String::from("Usage: dao proposal [--(export|mint-proposal)] <bulla>"));
        return
    }

    let mut index = 2;
    let (export, mint_proposal) = if parts.len() == 4 {
        let flags = match parts[index] {
            "--export" => (true, false),
            "--mint-proposal" => (false, true),
            _ => {
                output.push(String::from("Malformed `dao proposal` subcommand"));
                output.push(String::from("Usage: dao proposal [--(export|mint-proposal)] <bulla>"));
                return
            }
        };
        index += 1;
        flags
    } else {
        (false, false)
    };

    let bulla = match DaoProposalBulla::from_str(parts[index]) {
        Ok(b) => b,
        Err(e) => {
            output.push(format!("Invalid proposal bulla: {e}"));
            return
        }
    };

    let lock = drk.read().await;
    let proposal = match lock.get_dao_proposal_by_bulla(&bulla).await {
        Ok(p) => p,
        Err(e) => {
            output.push(format!("Failed to fetch DAO proposal: {e}"));
            return
        }
    };

    if export {
        // Retrieve the DAO
        let dao = match lock.get_dao_by_bulla(&proposal.proposal.dao_bulla).await {
            Ok(d) => d,
            Err(e) => {
                output.push(format!("Failed to fetch DAO: {e}"));
                return
            }
        };

        // Encypt the proposal
        let enc_note =
            AeadEncryptedNote::encrypt(&proposal, &dao.params.dao.proposals_public_key, &mut OsRng)
                .unwrap();

        // Export it to base64
        output.push(base64::encode(&serialize_async(&enc_note).await));
        return
    }

    if mint_proposal {
        // Identify proposal type by its auth calls
        for call in &proposal.proposal.auth_calls {
            // We only support transfer right now
            if call.function_code == DaoFunction::AuthMoneyTransfer as u8 {
                match lock.dao_transfer_proposal_tx(&proposal).await {
                    Ok(tx) => output.push(base64::encode(&serialize_async(&tx).await)),
                    Err(e) => output.push(format!("Failed to create DAO transfer proposal: {e}")),
                }
                return
            }
        }

        // If proposal has no auth calls, we consider it a generic one
        if proposal.proposal.auth_calls.is_empty() {
            match lock.dao_generic_proposal_tx(&proposal).await {
                Ok(tx) => output.push(base64::encode(&serialize_async(&tx).await)),
                Err(e) => output.push(format!("Failed to create DAO generic proposal: {e}")),
            }
            return
        }

        output.push(String::from("Unsuported DAO proposal"));
        return
    }

    output.push(format!("{proposal}"));

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
            let coin: CoinAttributes = match deserialize_async(proposal.data.as_ref().unwrap())
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    output.push(format!("Failed to deserialize transfer proposal coin data: {e}"));
                    return
                }
            };
            let recipient: Address =
                StandardAddress::from_public(lock.network, coin.public_key).into();
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

            contract_calls.push_str(&format!(
                "\n\t\t{}: {}\n\t\t{}: {} ({})\n\t\t{}: {}\n\t\t{}: {}\n\t\t{}: {}\n\t\t{}: {}\n\n",
                "Recipient",
                recipient,
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
                coin.blind
            ));
        }
    }

    output.push(contract_calls);

    let votes = match lock.get_dao_proposal_votes(&bulla).await {
        Ok(v) => v,
        Err(e) => {
            output.push(format!("Failed to fetch DAO proposal votes: {e}"));
            return
        }
    };
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
        output.push(String::from("Votes: No votes found"));
        "Unknown"
    } else {
        output.push(String::from("Votes:"));
        output.push(format!("{table}"));
        output.push(format!(
            "Total tokens votes: {}",
            encode_base10(total_all_vote_value, BALANCE_BASE10_DECIMALS)
        ));
        let approval_ratio = (total_yes_vote_value as f64 * 100.0) / total_all_vote_value as f64;
        output.push(format!(
            "Total tokens Yes votes: {} ({approval_ratio:.2}%)",
            encode_base10(total_yes_vote_value, BALANCE_BASE10_DECIMALS)
        ));
        output.push(format!(
            "Total tokens No votes: {} ({:.2}%)",
            encode_base10(total_no_vote_value, BALANCE_BASE10_DECIMALS),
            (total_no_vote_value as f64 * 100.0) / total_all_vote_value as f64
        ));

        let dao = match lock.get_dao_by_bulla(&proposal.proposal.dao_bulla).await {
            Ok(d) => d,
            Err(e) => {
                output.push(format!("Failed to fetch DAO: {e}"));
                return
            }
        };
        if total_all_vote_value >= dao.params.dao.quorum &&
            approval_ratio >=
                (dao.params.dao.approval_ratio_quot / dao.params.dao.approval_ratio_base)
                    as f64
        {
            "Approved"
        } else {
            "Rejected"
        }
    };

    if let Some(exec_tx_hash) = proposal.exec_tx_hash {
        output.push(format!("Proposal was executed on transaction: {exec_tx_hash}"));
        return
    }

    // Retrieve next block height and current block time target,
    // to compute their window.
    let next_block_height = match lock.get_next_block_height().await {
        Ok(n) => n,
        Err(e) => {
            output.push(format!("Failed to fetch next block height: {e}"));
            return
        }
    };
    let block_target = match lock.get_block_target().await {
        Ok(b) => b,
        Err(e) => {
            output.push(format!("Failed to fetch block target: {e}"));
            return
        }
    };
    let current_window = blockwindow(next_block_height, block_target);
    let end_time = proposal.proposal.creation_blockwindow + proposal.proposal.duration_blockwindows;
    let (voting_status, proposal_status_message) = if current_window < end_time {
        ("Ongoing", format!("Current proposal outcome: {outcome}"))
    } else {
        ("Concluded", format!("Proposal outcome: {outcome}"))
    };
    output.push(format!("Voting status: {voting_status}"));
    output.push(proposal_status_message);
}

/// Auxiliary function to define the dao proposal import subcommand handling.
async fn handle_dao_proposal_import(
    drk: &DrkPtr,
    parts: &[&str],
    input: &[String],
    output: &mut Vec<String>,
) {
    // Check correct subcommand structure
    if parts.len() != 2 {
        output.push(String::from("Malformed `dao proposal-import` subcommand"));
        output.push(String::from("Usage: dao proposal-import"));
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
        output.push(String::from("Failed to decode encrypted proposal data"));
        return
    };

    let encrypted_proposal: AeadEncryptedNote = match deserialize_async(&bytes).await {
        Ok(e) => e,
        Err(e) => {
            output.push(format!("Failed to deserialize encrypted proposal data: {e}"));
            return
        }
    };

    let lock = drk.read().await;
    let daos = match lock.get_daos().await {
        Ok(d) => d,
        Err(e) => {
            output.push(format!("Failed to retrieve DAOs: {e}"));
            return
        }
    };

    for dao in &daos {
        // Check if we have the proposals key
        let Some(proposals_secret_key) = dao.params.proposals_secret_key else { continue };

        // Try to decrypt the proposal
        let Ok(proposal) = encrypted_proposal.decrypt::<ProposalRecord>(&proposals_secret_key)
        else {
            continue
        };

        let proposal = match lock.get_dao_proposal_by_bulla(&proposal.bulla()).await {
            Ok(p) => {
                let mut our_proposal = p;
                our_proposal.data = proposal.data;
                our_proposal
            }
            Err(_) => proposal,
        };

        if let Err(e) = lock.put_dao_proposal(&proposal).await {
            output.push(format!("Failed to put DAO proposal: {e}"));
        }
        return
    }

    output.push(String::from("Couldn't decrypt the proposal with out DAO keys"));
}

/// Auxiliary function to define the dao vote subcommand handling.
async fn handle_dao_vote(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 4 && parts.len() != 5 {
        output.push(String::from("Malformed `dao vote` subcommand"));
        output.push(String::from("Usage: dao vote <bulla> <vote> [vote-weight]"));
        return
    }

    let bulla = match DaoProposalBulla::from_str(parts[2]) {
        Ok(b) => b,
        Err(e) => {
            output.push(format!("Invalid proposal bulla: {e}"));
            return
        }
    };

    let vote = match u8::from_str(parts[3]) {
        Ok(v) => {
            if v > 1 {
                output.push(String::from("Vote can be either 0 (NO) or 1 (YES)"));
                return
            }
            v != 0
        }
        Err(e) => {
            output.push(format!("Invalid vote: {e}"));
            return
        }
    };

    let weight = if parts.len() == 5 {
        if let Err(e) = f64::from_str(parts[4]) {
            output.push(format!("Invalid vote weight: {e}"));
            return
        }
        match decode_base10(parts[4], BALANCE_BASE10_DECIMALS, true) {
            Ok(w) => Some(w),
            Err(e) => {
                output.push(format!("Error while parsing vote weight: {e}"));
                return
            }
        }
    } else {
        None
    };

    match drk.read().await.dao_vote(&bulla, vote, weight).await {
        Ok(tx) => output.push(base64::encode(&serialize_async(&tx).await)),
        Err(e) => output.push(format!("Failed to create DAO Vote transaction: {e}")),
    }
}

/// Auxiliary function to define the dao exec subcommand handling.
async fn handle_dao_exec(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 3 && parts.len() != 4 {
        output.push(String::from("Malformed `dao exec` subcommand"));
        output.push(String::from("Usage: dao exec [--early] <bulla>"));
        return
    }

    let mut index = 2;
    let mut early = false;
    if parts[index] == "--early" {
        early = true;
        index += 1;
    }

    let bulla = match DaoProposalBulla::from_str(parts[index]) {
        Ok(b) => b,
        Err(e) => {
            output.push(format!("Invalid proposal bulla: {e}"));
            return
        }
    };

    let lock = drk.read().await;
    let proposal = match lock.get_dao_proposal_by_bulla(&bulla).await {
        Ok(p) => p,
        Err(e) => {
            output.push(format!("Failed to fetch DAO proposal: {e}"));
            return
        }
    };

    // Identify proposal type by its auth calls
    for call in &proposal.proposal.auth_calls {
        // We only support transfer right now
        if call.function_code == DaoFunction::AuthMoneyTransfer as u8 {
            match lock.dao_exec_transfer(&proposal, early).await {
                Ok(tx) => output.push(base64::encode(&serialize_async(&tx).await)),
                Err(e) => output.push(format!("Failed to execute DAO transfer proposal: {e}")),
            };
            return
        }
    }

    // If proposal has no auth calls, we consider it a generic one
    if proposal.proposal.auth_calls.is_empty() {
        match lock.dao_exec_generic(&proposal, early).await {
            Ok(tx) => output.push(base64::encode(&serialize_async(&tx).await)),
            Err(e) => output.push(format!("Failed to execute DAO generic proposal: {e}")),
        };
        return
    }

    output.push(String::from("Unsuported DAO proposal"));
}

/// Auxiliary function to define the dao spent hook subcommand handling.
async fn handle_dao_spend_hook(parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 2 {
        output.push(String::from("Malformed `dao spent-hook` subcommand"));
        output.push(String::from("Usage: dao spent-hook"));
        return
    }

    let spend_hook =
        FuncRef { contract_id: *DAO_CONTRACT_ID, func_code: DaoFunction::Exec as u8 }.to_func_id();
    output.push(format!("{spend_hook}"));
}

/// Auxiliary function to define the dao mining config subcommand handling.
async fn handle_dao_mining_config(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 3 {
        output.push(String::from("Malformed `dao mining-config` subcommand"));
        output.push(String::from("Usage: dao mining-config <name>"));
        return
    }

    if let Err(e) = drk.read().await.dao_mining_config(parts[2], output).await {
        output.push(format!("Failed to generate DAO mining configuration: {e}"));
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

/// Auxiliary function to define the tx from calls command handling.
async fn handle_tx_from_calls(
    drk: &DrkPtr,
    parts: &[&str],
    input: &[String],
    output: &mut Vec<String>,
) {
    // Check correct subcommand structure
    if parts.len() != 1 && parts.len() != 2 {
        output.push(String::from("Malformed `tx-from-calls` subcommand"));
        output.push(String::from("Usage: tx-from-calls [calls-map]"));
        return
    }

    // Parse calls
    let calls = match parse_calls_from_input(input).await {
        Ok(c) => c,
        Err(e) => {
            output.push(format!("Error while parsing transaction calls: {e}"));
            return
        }
    };
    if calls.is_empty() {
        output.push(String::from("No calls were parsed"));
        return
    }

    // If there is a given map, parse it, otherwise construct a
    // linear map.
    let calls_map = if parts.len() == 2 {
        match parse_tree(parts[1]) {
            Ok(m) => m,
            Err(e) => {
                output.push(format!("Failed parsing calls map: {e}"));
                return
            }
        }
    } else {
        let mut calls_map = Vec::with_capacity(calls.len());
        for (i, _) in calls.iter().enumerate() {
            calls_map.push((i, vec![]));
        }
        calls_map
    };
    if calls_map.len() != calls.len() {
        output.push(String::from("Calls map size not equal to parsed calls"));
        return
    }

    // Create a transaction from the mapped calls
    let (mut tx_builder, signature_secrets) = match tx_from_calls_mapped(&calls, &calls_map) {
        Ok(pair) => pair,
        Err(e) => {
            output.push(format!("Failed to create a transaction from the mapped calls: {e}"));
            return
        }
    };

    // Now build and sign the fee-less tx
    let mut tx = match tx_builder.build() {
        Ok(tx) => tx,
        Err(e) => {
            output.push(format!("Failed to build the transaction: {e}"));
            return
        }
    };
    let sigs = match tx.create_sigs(&signature_secrets) {
        Ok(s) => s,
        Err(e) => {
            output.push(format!("Failed to create the transaction signatures: {e}"));
            return
        }
    };
    tx.signatures.push(sigs);

    // Attach its fee and grab its signature
    if let Err(e) = drk.read().await.attach_fee(&mut tx).await {
        output.push(format!("Failed to attach the fee call to the transaction: {e}"));
        return
    }
    // Its safe to unwrap here since we know the fee signature
    // is in the last position.
    let fee_signature = tx.signatures.last().unwrap().clone();

    // Re-sign the tx using the calls secrets
    let sigs = match tx.create_sigs(&signature_secrets) {
        Ok(s) => s,
        Err(e) => {
            output.push(format!("Failed to create the transaction signatures: {e}"));
            return
        }
    };
    tx.signatures = vec![sigs, fee_signature];

    output.push(base64::encode(&serialize_async(&tx).await));
}

/// Auxiliary function to define the inspect command handling.
async fn handle_inspect(input: &[String], output: &mut Vec<String>) {
    match parse_tx_from_input(input).await {
        Ok(tx) => output.push(pretty_tx(&tx)),
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
    subscription_tasks: &[StoppableTaskPtr; 2],
    shell_sender: &Sender<Vec<String>>,
    ex: &ExecutorPtr,
) {
    // Kill zombie tasks if they failed
    subscription_tasks[0].stop_nowait();
    subscription_tasks[1].stop_nowait();
    *subscription_active = true;

    // Start the subscription task
    let drk_ = drk.clone();
    let rpc_task_ = subscription_tasks[1].clone();
    let shell_sender_ = shell_sender.clone();
    let endpoint_ = endpoint.clone();
    let ex_ = ex.clone();
    subscription_tasks[0].clone().start(
        async move { subscribe_blocks(&drk_, rpc_task_, shell_sender_, endpoint_, &ex_).await },
        |_| async { /* Do nothing */ },
        Error::DetachedTaskStopped,
        ex.clone(),
    );
}

/// Auxiliary function to define the unsubscribe command handling.
async fn handle_unsubscribe(
    subscription_active: &mut bool,
    subscription_tasks: &[StoppableTaskPtr; 2],
) {
    subscription_tasks[0].stop_nowait();
    subscription_tasks[1].stop_nowait();
    *subscription_active = false;
}

/// Auxiliary function to define the scan command handling.
async fn handle_scan(
    drk: &DrkPtr,
    subscription_active: &bool,
    parts: &[&str],
    output: &mut Vec<String>,
    print: &bool,
) {
    if *subscription_active {
        append_or_print(output, None, print, vec![String::from("Subscription is already active!")])
            .await;
        return
    }

    // Check correct command structure
    if parts.len() != 1 && parts.len() != 3 {
        append_or_print(output, None, print, vec![String::from("Malformed `scan` command")]).await;
        return
    }

    // Check if reset was requested
    let lock = drk.read().await;
    if parts.len() == 3 {
        if parts[1] != "--reset" {
            append_or_print(
                output,
                None,
                print,
                vec![
                    String::from("Malformed `scan` command"),
                    String::from("Usage: scan --reset <height>"),
                ],
            )
            .await;
            return
        }

        let height = match u32::from_str(parts[2]) {
            Ok(h) => h,
            Err(e) => {
                append_or_print(output, None, print, vec![format!("Invalid reset height: {e}")])
                    .await;
                return
            }
        };

        let mut buf = vec![];
        if let Err(e) = lock.reset_to_height(height, &mut buf).await {
            buf.push(format!("Failed during wallet reset: {e}"));
            append_or_print(output, None, print, buf).await;
            return
        }
        append_or_print(output, None, print, buf).await;
    }

    if let Err(e) = lock.scan_blocks(output, None, print).await {
        append_or_print(output, None, print, vec![format!("Failed during scanning: {e}")]).await;
        return
    }
    append_or_print(output, None, print, vec![String::from("Finished scanning blockchain")]).await;
}

/// Auxiliary function to define the explorer command handling.
async fn handle_explorer(drk: &DrkPtr, parts: &[&str], input: &[String], output: &mut Vec<String>) {
    // Check correct command structure
    if parts.len() < 2 {
        output.push(String::from("Malformed `explorer` command"));
        output.push(String::from(
            "Usage: explorer (fetch-tx|simulate-tx|txs-history|clear-reverted|scanned-blocks|mining-config)",
        ));
        return
    }

    // Handle subcommand
    match parts[1] {
        "fetch-tx" => handle_explorer_fetch_tx(drk, parts, output).await,
        "simulate-tx" => handle_explorer_simulate_tx(drk, parts, input, output).await,
        "txs-history" => handle_explorer_txs_history(drk, parts, output).await,
        "clear-reverted" => handle_explorer_clear_reverted(drk, parts, output).await,
        "scanned-blocks" => handle_explorer_scanned_blocks(drk, parts, output).await,
        "mining-config" => handle_explorer_mining_config(parts, input, output).await,
        _ => {
            output.push(format!("Unrecognized explorer subcommand: {}", parts[1]));
            output.push(String::from(
                "Usage: explorer (fetch-tx|simulate-tx|txs-history|clear-reverted|scanned-blocks|mining-config)",
            ));
        }
    }
}

/// Auxiliary function to define the explorer fetch transaction subcommand handling.
async fn handle_explorer_fetch_tx(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 3 && parts.len() != 4 {
        output.push(String::from("Malformed `explorer fetch-tx` subcommand"));
        output.push(String::from("Usage: explorer fetch-tx [--encode] <tx-hash>"));
        return
    }

    let mut index = 2;
    let mut encode = false;
    if parts[index] == "--encode" {
        encode = true;
        index += 1;
    }

    let hash = match blake3::Hash::from_hex(parts[index]) {
        Ok(h) => h,
        Err(e) => {
            output.push(format!("Invalid transaction hash: {e}"));
            return
        }
    };
    let tx_hash = TransactionHash(*hash.as_bytes());

    let tx = match drk.read().await.get_tx(&tx_hash).await {
        Ok(tx) => tx,
        Err(e) => {
            output.push(format!("Failed to fetch transaction: {e}"));
            return
        }
    };

    let Some(tx) = tx else {
        output.push(String::from("Transaction was not found"));
        return
    };

    // Make sure the tx is correct
    if tx.hash() != tx_hash {
        output.push(format!("Transaction hash missmatch: {tx_hash} - {}", tx.hash()));
        return
    }

    if encode {
        output.push(base64::encode(&serialize_async(&tx).await));
        return
    }

    output.push(format!("Transaction ID: {tx_hash}"));
    output.push(format!("{tx:?}"));
}

/// Auxiliary function to define the explorer simulate transaction subcommand handling.
async fn handle_explorer_simulate_tx(
    drk: &DrkPtr,
    parts: &[&str],
    input: &[String],
    output: &mut Vec<String>,
) {
    // Check correct subcommand structure
    if parts.len() != 2 {
        output.push(String::from("Malformed `explorer simulate-tx` subcommand"));
        output.push(String::from("Usage: explorer simulate-tx"));
        return
    }

    let tx = match parse_tx_from_input(input).await {
        Ok(t) => t,
        Err(e) => {
            output.push(format!("Error while parsing transaction: {e}"));
            return
        }
    };

    match drk.read().await.simulate_tx(&tx).await {
        Ok(is_valid) => {
            output.push(format!("Transaction ID: {}", tx.hash()));
            output.push(format!("State: {}", if is_valid { "valid" } else { "invalid" }));
        }
        Err(e) => output.push(format!("Failed to simulate tx: {e}")),
    }
}

/// Auxiliary function to define the explorer transactions history subcommand handling.
async fn handle_explorer_txs_history(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct command structure
    if parts.len() < 2 || parts.len() > 4 {
        output.push(String::from("Malformed `explorer txs-history` command"));
        output.push(String::from("Usage: explorer txs-history [--encode] [tx-hash]"));
        return
    }

    let lock = drk.read().await;
    if parts.len() > 2 {
        let mut index = 2;
        let mut encode = false;
        if parts[index] == "--encode" {
            encode = true;
            index += 1;
        }

        let (tx_hash, status, block_height, tx) =
            match lock.get_tx_history_record(parts[index]).await {
                Ok(i) => i,
                Err(e) => {
                    output.push(format!("Failed to fetch transaction: {e}"));
                    return
                }
            };

        if encode {
            output.push(base64::encode(&serialize_async(&tx).await));
            return
        }

        output.push(format!("Transaction ID: {tx_hash}"));
        output.push(format!("Status: {status}"));
        match block_height {
            Some(block_height) => output.push(format!("Block height: {block_height}")),
            None => output.push(String::from("Block height: -")),
        }
        output.push(format!("{tx:?}"));
        return
    }

    let map = match lock.get_txs_history() {
        Ok(m) => m,
        Err(e) => {
            output.push(format!("Failed to retrieve transactions history records: {e}"));
            return
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
        output.push(String::from("No transactions found"));
    } else {
        output.push(format!("{table}"));
    }
}

/// Auxiliary function to define the explorer clear reverted subcommand handling.
async fn handle_explorer_clear_reverted(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 2 {
        output.push(String::from("Malformed `explorer clear-reverted` subcommand"));
        output.push(String::from("Usage: explorer clear-reverted"));
        return
    }

    if let Err(e) = drk.read().await.remove_reverted_txs(output) {
        output.push(format!("Failed to remove reverted transactions: {e}"));
    }
}

/// Auxiliary function to define the explorer scanned blocks subcommand handling.
async fn handle_explorer_scanned_blocks(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 2 && parts.len() != 3 {
        output.push(String::from("Malformed `explorer scanned-blocks` subcommand"));
        output.push(String::from("Usage: explorer scanned-blocks [height]"));
        return
    }

    let lock = drk.read().await;
    if parts.len() == 3 {
        let height = match u32::from_str(parts[2]) {
            Ok(d) => d,
            Err(e) => {
                output.push(format!("Invalid height: {e}"));
                return
            }
        };

        match lock.get_scanned_block(&height) {
            Ok((hash, signing_key)) => {
                output.push(format!("Height: {height}"));
                output.push(format!("Hash: {hash}"));
                output.push(format!("Signing key: {signing_key}"));
            }
            Err(e) => output.push(format!("Failed to retrieve scanned block record: {e}")),
        };
        return
    }

    let map = match lock.get_scanned_block_records() {
        Ok(m) => m,
        Err(e) => {
            output.push(format!("Failed to retrieve scanned blocks records: {e}"));
            return
        }
    };

    let table = prettytable_scanned_blocks(&map);

    if table.is_empty() {
        output.push(String::from("No scanned blocks records found"));
    } else {
        output.push(format!("{table}"));
    }
}

/// Auxiliary function to define the explorer mining config subcommand handling.
async fn handle_explorer_mining_config(parts: &[&str], input: &[String], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 2 {
        output.push(String::from("Malformed `explorer mining-config` subcommand"));
        output.push(String::from("Usage: explorer mining-config"));
        return
    }

    let (config, recipient, spend_hook, user_data) =
        match parse_mining_config_from_input(input).await {
            Ok(c) => c,
            Err(e) => {
                output.push(format!("Error while parsing mining config: {e}"));
                return
            }
        };

    display_mining_config(&config, &recipient, &spend_hook, &user_data, output)
}

/// Auxiliary function to define the alias command handling.
async fn handle_alias(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct command structure
    if parts.len() < 2 {
        output.push(String::from("Malformed `alias` command"));
        output.push(String::from("Usage: alias (add|show|remove)"));
        return
    }

    // Handle subcommand
    match parts[1] {
        "add" => handle_alias_add(drk, parts, output).await,
        "show" => handle_alias_show(drk, parts, output).await,
        "remove" => handle_alias_remove(drk, parts, output).await,
        _ => {
            output.push(format!("Unrecognized alias subcommand: {}", parts[1]));
            output.push(String::from("Usage: alias (add|show|remove)"));
        }
    }
}

/// Auxiliary function to define the alias add subcommand handling.
async fn handle_alias_add(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 4 {
        output.push(String::from("Malformed `alias add` subcommand"));
        output.push(String::from("Usage: alias add <alias> <token>"));
        return
    }

    if parts[2].len() > 5 {
        output.push(String::from("Error: Alias exceeds 5 characters"));
        return
    }

    let token_id = match TokenId::from_str(parts[3]) {
        Ok(t) => t,
        Err(e) => {
            output.push(format!("Invalid Token ID: {e}"));
            return
        }
    };

    if let Err(e) = drk.read().await.add_alias(String::from(parts[2]), token_id, output).await {
        output.push(format!("Failed to add alias: {e}"));
    }
}

/// Auxiliary function to define the alias show subcommand handling.
async fn handle_alias_show(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct command structure
    if parts.len() != 2 && parts.len() != 4 && parts.len() != 6 {
        output.push(String::from("Malformed `alias show` command"));
        output.push(String::from("Usage: alias show [-a, --alias <alias>] [-t, --token <token>]"));
        return
    }

    let mut alias = None;
    let mut token_id = None;
    if parts.len() > 2 {
        let mut index = 2;
        if parts[index] == "-a" || parts[index] == "--alias" {
            alias = Some(String::from(parts[index + 1]));
            index += 2;
        }

        if index < parts.len() && (parts[index] == "-t" || parts[index] == "--token") {
            match TokenId::from_str(parts[index + 1]) {
                Ok(t) => token_id = Some(t),
                Err(e) => {
                    output.push(format!("Invalid Token ID: {e}"));
                    return
                }
            };
            index += 2;
        }

        // Check alias again in case it was after token
        if index < parts.len() && (parts[index] == "-a" || parts[index] == "--alias") {
            alias = Some(String::from(parts[index + 1]));
        }
    }

    let map = match drk.read().await.get_aliases(alias, token_id).await {
        Ok(m) => m,
        Err(e) => {
            output.push(format!("Failed to fetch aliases map: {e}"));
            return
        }
    };

    let table = prettytable_aliases(&map);

    if table.is_empty() {
        output.push(String::from("No aliases found"));
    } else {
        output.push(format!("{table}"));
    }
}

/// Auxiliary function to define the alias remove subcommand handling.
async fn handle_alias_remove(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 3 {
        output.push(String::from("Malformed `alias remove` subcommand"));
        output.push(String::from("Usage: alias remove <alias>"));
        return
    }

    if let Err(e) = drk.read().await.remove_alias(String::from(parts[2]), output).await {
        output.push(format!("Failed to remove alias: {e}"));
    }
}

/// Auxiliary function to define the token command handling.
async fn handle_token(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct command structure
    if parts.len() < 2 {
        output.push(String::from("Malformed `token` command"));
        output.push(String::from("Usage: token (import|generate-mint|list|mint|freeze)"));
        return
    }

    // Handle subcommand
    match parts[1] {
        "import" => handle_token_import(drk, parts, output).await,
        "generate-mint" => handle_token_generate_mint(drk, parts, output).await,
        "list" => handle_token_list(drk, parts, output).await,
        "mint" => handle_token_mint(drk, parts, output).await,
        "freeze" => handle_token_freeze(drk, parts, output).await,
        _ => {
            output.push(format!("Unrecognized token subcommand: {}", parts[1]));
            output.push(String::from("Usage: token (import|generate-mint|list|mint|freeze)"));
        }
    }
}

/// Auxiliary function to define the token import subcommand handling.
async fn handle_token_import(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 4 {
        output.push(String::from("Malformed `token import` subcommand"));
        output.push(String::from("Usage: token import <secret-key> <token-blind>"));
        return
    }

    let mint_authority = match SecretKey::from_str(parts[2]) {
        Ok(ma) => ma,
        Err(e) => {
            output.push(format!("Invalid mint authority: {e}"));
            return
        }
    };

    let token_blind = match BaseBlind::from_str(parts[3]) {
        Ok(tb) => tb,
        Err(e) => {
            output.push(format!("Invalid token blind: {e}"));
            return
        }
    };

    match drk.read().await.import_mint_authority(mint_authority, token_blind).await {
        Ok(token_id) => {
            output.push(format!("Successfully imported mint authority for token ID: {token_id}"))
        }
        Err(e) => output.push(format!("Failed to import mint authority: {e}")),
    }
}

/// Auxiliary function to define the token generate mint subcommand handling.
async fn handle_token_generate_mint(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 2 {
        output.push(String::from("Malformed `token generate-mint` subcommand"));
        output.push(String::from("Usage: token generate-mint"));
        return
    }

    let mint_authority = SecretKey::random(&mut OsRng);
    let token_blind = BaseBlind::random(&mut OsRng);
    match drk.read().await.import_mint_authority(mint_authority, token_blind).await {
        Ok(token_id) => {
            output.push(format!("Successfully imported mint authority for token ID: {token_id}"))
        }
        Err(e) => output.push(format!("Failed to import mint authority: {e}")),
    }
}

/// Auxiliary function to define the token list subcommand handling.
async fn handle_token_list(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 2 {
        output.push(String::from("Malformed `token list` subcommand"));
        output.push(String::from("Usage: token list"));
        return
    }

    let lock = drk.read().await;
    let tokens = match lock.get_mint_authorities().await {
        Ok(m) => m,
        Err(e) => {
            output.push(format!("Failed to fetch mint authorities: {e}"));
            return
        }
    };

    let aliases_map = match lock.get_aliases_mapped_by_token().await {
        Ok(m) => m,
        Err(e) => {
            output.push(format!("Failed to fetch aliases map: {e}"));
            return
        }
    };

    let table = prettytable_tokenlist(&tokens, &aliases_map);

    if table.is_empty() {
        output.push(String::from("No tokens found"));
    } else {
        output.push(format!("{table}"));
    }
}

/// Auxiliary function to define the token mint subcommand handling.
async fn handle_token_mint(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct command structure
    if parts.len() < 5 || parts.len() > 7 {
        output.push(String::from("Malformed `token mint` subcommand"));
        output.push(String::from(
            "Usage: token mint <token> <amount> <recipient> [spend-hook] [user-data]",
        ));
        return
    }

    let amount = String::from(parts[3]);
    if let Err(e) = f64::from_str(&amount) {
        output.push(format!("Invalid amount: {e}"));
        return
    }

    let rcpt = match Address::from_str(parts[4]) {
        Ok(r) => r,
        Err(e) => {
            output.push(format!("Invalid recipient: {e}"));
            return
        }
    };

    let lock = drk.read().await;

    if rcpt.network() != lock.network {
        output.push("Recipient address prefix mismatch".to_string());
        return
    }

    let token_id = match lock.get_token(String::from(parts[2])).await {
        Ok(t) => t,
        Err(e) => {
            output.push(format!("Invalid token ID: {e}"));
            return
        }
    };

    // Parse command
    let mut index = 5;
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

    match lock.mint_token(&amount, *rcpt.public_key(), token_id, spend_hook, user_data).await {
        Ok(t) => output.push(base64::encode(&serialize_async(&t).await)),
        Err(e) => output.push(format!("Failed to create token mint transaction: {e}")),
    }
}

/// Auxiliary function to define the token freeze subcommand handling.
async fn handle_token_freeze(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 3 {
        output.push(String::from("Malformed `token freeze` subcommand"));
        output.push(String::from("Usage: token freeze <token>"));
        return
    }

    let lock = drk.read().await;
    let token_id = match lock.get_token(String::from(parts[2])).await {
        Ok(t) => t,
        Err(e) => {
            output.push(format!("Invalid token ID: {e}"));
            return
        }
    };

    match lock.freeze_token(token_id).await {
        Ok(t) => output.push(base64::encode(&serialize_async(&t).await)),
        Err(e) => output.push(format!("Failed to create token freeze transaction: {e}")),
    }
}

/// Auxiliary function to define the contract command handling.
async fn handle_contract(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct command structure
    if parts.len() < 2 {
        output.push(String::from("Malformed `contract` command"));
        output.push(String::from("Usage: contract (generate-deploy|list|export-data|deploy|lock)"));
        return
    }

    // Handle subcommand
    match parts[1] {
        "generate-deploy" => handle_contract_generate_deploy(drk, parts, output).await,
        "list" => handle_contract_list(drk, parts, output).await,
        "export-data" => handle_contract_export_data(drk, parts, output).await,
        "deploy" => handle_contract_deploy(drk, parts, output).await,
        "lock" => handle_contract_lock(drk, parts, output).await,
        _ => {
            output.push(format!("Unrecognized contract subcommand: {}", parts[1]));
            output.push(String::from(
                "Usage: contract (generate-deploy|list|export-data|deploy|lock)",
            ));
        }
    }
}

/// Auxiliary function to define the contract generate deploy subcommand handling.
async fn handle_contract_generate_deploy(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 2 {
        output.push(String::from("Malformed `contract generate-deploy` subcommand"));
        output.push(String::from("Usage: contract generate-deploy"));
        return
    }

    if let Err(e) = drk.read().await.deploy_auth_keygen(output).await {
        output.push(format!("Error creating deploy auth keypair: {e}"));
    }
}

/// Auxiliary function to define the contract list subcommand handling.
async fn handle_contract_list(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 2 && parts.len() != 3 {
        output.push(String::from("Malformed `contract list` subcommand"));
        output.push(String::from("Usage: contract list [contract-id]"));
        return
    }

    if parts.len() == 3 {
        let deploy_auth = match ContractId::from_str(parts[2]) {
            Ok(d) => d,
            Err(e) => {
                output.push(format!("Invalid deploy authority: {e}"));
                return
            }
        };

        let history = match drk.read().await.get_deploy_auth_history(&deploy_auth).await {
            Ok(a) => a,
            Err(e) => {
                output.push(format!("Failed to fetch deploy authority history records: {e}"));
                return
            }
        };

        let table = prettytable_contract_history(&history);
        if table.is_empty() {
            output.push(String::from("No history records found"));
        } else {
            output.push(format!("{table}"));
        }
        return
    }

    let auths = match drk.read().await.list_deploy_auth().await {
        Ok(a) => a,
        Err(e) => {
            output.push(format!("Failed to fetch deploy authorities: {e}"));
            return
        }
    };

    let table = prettytable_contract_auth(&auths);

    if table.is_empty() {
        output.push(String::from("No deploy authorities found"));
    } else {
        output.push(format!("{table}"));
    }
}

/// Auxiliary function to define the contract export data subcommand handling.
async fn handle_contract_export_data(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 3 {
        output.push(String::from("Malformed `contract export-data` subcommand"));
        output.push(String::from("Usage: contract export-data <tx-hash>"));
        return
    }

    match drk.read().await.get_deploy_history_record_data(parts[2]).await {
        Ok(pair) => output.push(base64::encode(&serialize_async(&pair).await)),
        Err(e) => output.push(format!("Failed to retrieve history record: {e}")),
    }
}

/// Auxiliary function to define the contract deploy subcommand handling.
async fn handle_contract_deploy(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 4 && parts.len() != 5 {
        output.push(String::from("Malformed `contract deploy` subcommand"));
        output.push(String::from("Usage: contract deploy <deploy-auth> <wasm-path> [deploy-ix]"));
        return
    }

    let deploy_auth = match ContractId::from_str(parts[2]) {
        Ok(d) => d,
        Err(e) => {
            output.push(format!("Invalid deploy authority: {e}"));
            return
        }
    };

    // Read the wasm bincode and deploy instruction
    let file_path = match expand_path(parts[3]) {
        Ok(p) => p,
        Err(e) => {
            output.push(format!("Error while expanding wasm bincode file path: {e}"));
            return
        }
    };
    let wasm_bin = match smol::fs::read(file_path).await {
        Ok(w) => w,
        Err(e) => {
            output.push(format!("Error while reading wasm bincode file: {e}"));
            return
        }
    };

    let deploy_ix = if parts.len() == 5 {
        let file_path = match expand_path(parts[4]) {
            Ok(p) => p,
            Err(e) => {
                output.push(format!("Error while expanding deploy instruction file path: {e}"));
                return
            }
        };
        match smol::fs::read(file_path).await {
            Ok(d) => d,
            Err(e) => {
                output.push(format!("Error while reading deploy instruction file: {e}"));
                return
            }
        }
    } else {
        vec![]
    };

    match drk.read().await.deploy_contract(&deploy_auth, wasm_bin, deploy_ix).await {
        Ok(t) => output.push(base64::encode(&serialize_async(&t).await)),
        Err(e) => output.push(format!("Failed to create contract deployment transaction: {e}")),
    }
}

/// Auxiliary function to define the contract lock subcommand handling.
async fn handle_contract_lock(drk: &DrkPtr, parts: &[&str], output: &mut Vec<String>) {
    // Check correct subcommand structure
    if parts.len() != 3 {
        output.push(String::from("Malformed `contract lock` subcommand"));
        output.push(String::from("Usage: contract lock <deploy-auth>"));
        return
    }

    let deploy_auth = match ContractId::from_str(parts[2]) {
        Ok(d) => d,
        Err(e) => {
            output.push(format!("Invalid deploy authority: {e}"));
            return
        }
    };

    match drk.read().await.lock_contract(&deploy_auth).await {
        Ok(t) => output.push(base64::encode(&serialize_async(&t).await)),
        Err(e) => output.push(format!("Failed to create contract lock transaction: {e}")),
    }
}
